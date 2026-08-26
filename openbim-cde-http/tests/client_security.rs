#![allow(missing_docs)]

use axum::{
    body::Bytes,
    extract::State,
    http::{header, HeaderMap, StatusCode},
    response::Redirect,
    routing::{get, post, put},
    Json, Router,
};
use base64::{engine::general_purpose::STANDARD, Engine as _};
use openbim_cde::{
    documents::{HeaderValue, Headers, HttpMethod, MultipartFormData, UploadFilePartInstruction},
    foundation::DynamicClientRegistrationRequest,
    OptionalNonNull,
};
use openbim_cde_http::{Credential, HttpClientBuilder, HttpError, UploadSource};
use serde_json::json;
use std::sync::{
    atomic::{AtomicUsize, Ordering},
    Arc,
};
use zeroize::Zeroize;

async fn spawn(app: Router) -> String {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    tokio::spawn(async move { axum::serve(listener, app).await.unwrap() });
    format!("http://{address}")
}

fn local_client(base: &str) -> HttpClientBuilder {
    HttpClientBuilder::new(
        base,
        format!("{base}/foundation/1.1"),
        format!("{base}/documents/1.0"),
    )
    .unwrap()
    .allow_http_for_local_development()
}

#[test]
fn https_is_required_unless_loopback_http_is_explicitly_enabled() {
    let builder = HttpClientBuilder::new(
        "http://127.0.0.1:1234",
        "http://127.0.0.1:1234/foundation/1.1",
        "http://127.0.0.1:1234/documents/1.0",
    )
    .unwrap();
    assert!(builder.build().is_err());

    assert!(HttpClientBuilder::new(
        "http://example.com",
        "http://example.com/foundation/1.1",
        "http://example.com/documents/1.0",
    )
    .unwrap()
    .allow_http_for_local_development()
    .build()
    .is_err());
}

#[test]
fn bearer_and_basic_debug_output_is_redacted() {
    let bearer = format!("{:?}", Credential::bearer("token-marker"));
    let basic = format!("{:?}", Credential::basic("user-marker", "password-marker"));
    for secret in ["token-marker", "user-marker", "password-marker"] {
        assert!(!bearer.contains(secret));
        assert!(!basic.contains(secret));
    }
}

#[test]
fn credentials_zeroize_eagerly_and_on_drop() {
    fn assert_zeroize_on_drop<T: zeroize::ZeroizeOnDrop>() {}
    assert_zeroize_on_drop::<Credential>();
    let mut credential = Credential::basic("user", "password");
    credential.zeroize();
    assert_eq!(credential, Credential::basic("", ""));
}

#[tokio::test]
async fn cross_origin_advertised_url_does_not_inherit_authorization() {
    let origin = spawn(Router::new()).await;
    let other = spawn(Router::new().route(
        "/selected",
        get(|| async { Json(json!({"documents": []})) }),
    ))
    .await;
    let client = local_client(&origin)
        .credential(Credential::bearer("never-send-me"))
        .build()
        .unwrap();
    let error = client
        .selected_documents(&format!("{other}/selected"))
        .await
        .unwrap_err();
    assert!(matches!(error, HttpError::CrossOriginAuthorization));
}

#[tokio::test]
async fn redirects_are_not_followed() {
    let hits = Arc::new(AtomicUsize::new(0));
    let app = Router::new()
        .route(
            "/foundation/versions",
            get(|| async { Redirect::temporary("/target") }),
        )
        .route(
            "/target",
            get(|State(hits): State<Arc<AtomicUsize>>| async move {
                hits.fetch_add(1, Ordering::SeqCst);
                Json(json!({"versions": []}))
            }),
        )
        .with_state(hits.clone());
    let base = spawn(app).await;
    let error = local_client(&base)
        .build()
        .unwrap()
        .versions()
        .await
        .unwrap_err();
    assert!(matches!(error, HttpError::Response { status: 307, .. }));
    assert_eq!(hits.load(Ordering::SeqCst), 0);
}

#[tokio::test]
async fn invalid_json_success_content_type_is_rejected() {
    let base = spawn(Router::new().route(
        "/foundation/versions",
        get(|| async { (StatusCode::OK, "{\"versions\":[]}") }),
    ))
    .await;
    let error = local_client(&base)
        .build()
        .unwrap()
        .versions()
        .await
        .unwrap_err();
    assert!(matches!(error, HttpError::InvalidContentType));
}

#[tokio::test]
async fn json_limit_fails_while_streaming_the_response() {
    let base = spawn(Router::new().route(
        "/foundation/versions",
        get(|| async { Json(json!({"versions": [], "padding": "x".repeat(2048)})) }),
    ))
    .await;
    let error = local_client(&base)
        .max_json_body(64)
        .build()
        .unwrap()
        .versions()
        .await
        .unwrap_err();
    assert!(matches!(error, HttpError::BodyTooLarge { limit: 64 }));
}

#[tokio::test]
async fn bounded_error_diagnostics_have_one_explicit_untrusted_surface() {
    let secret = "sensitive-token-marker";
    let server_text = "opaque-server-message-marker";
    let body = json!({"message": format!("failure {secret} {server_text} {}", "z".repeat(64))});
    let base = spawn(Router::new().route(
        "/foundation/versions",
        get(move || {
            let body = body.clone();
            async move { (StatusCode::BAD_REQUEST, Json(body)) }
        }),
    ))
    .await;
    let error = local_client(&base)
        .max_error_body(256)
        .credential(Credential::bearer(secret))
        .build()
        .unwrap()
        .versions()
        .await
        .unwrap_err();
    let debug = format!("{error:?}");
    assert!(!debug.contains(secret));
    assert!(!debug.contains(server_text));
    match error {
        HttpError::Response { status, diagnostic } => {
            assert_eq!(status, 400);
            assert!(diagnostic.expose_untrusted().contains(server_text));
            assert!(diagnostic.expose_untrusted().len() <= 256);
        }
        other => panic!("unexpected error: {other:?}"),
    }
}

#[tokio::test]
async fn upload_instruction_streams_framing_and_applies_only_safe_headers() {
    let (sender, mut receiver) = tokio::sync::mpsc::channel(1);
    let app = Router::new()
        .route(
            "/part",
            put(
                |State(sender): State<tokio::sync::mpsc::Sender<(HeaderMap, Bytes)>>,
                 headers: HeaderMap,
                 body: Bytes| async move {
                    assert_eq!(headers[header::CONTENT_TYPE], "application/octet-stream");
                    sender.send((headers, body)).await.unwrap();
                    StatusCode::OK
                },
            ),
        )
        .with_state(sender);
    let upload_origin = spawn(app).await;
    let client = local_client("http://127.0.0.1:9").build().unwrap();
    let mut instruction = UploadFilePartInstruction {
        url: format!("{upload_origin}/part"),
        http_method: HttpMethod::Put,
        additional_headers: OptionalNonNull::Present(Headers {
            values: vec![HeaderValue {
                name: "x-upload-signature".into(),
                value: "signed-header-secret".into(),
            }],
        }),
        include_authorization: false,
        multipart_form_data: OptionalNonNull::Present(MultipartFormData {
            prefix: STANDARD.encode("pre-"),
            suffix: STANDARD.encode("-post"),
        }),
        content_range_start: 0,
        content_range_end: 6,
    };
    client
        .upload_file_part_put(&instruction, UploadSource::from_bytes("payload"))
        .await
        .unwrap();
    let (headers, body) = receiver.recv().await.unwrap();
    assert_eq!(headers["x-upload-signature"], "signed-header-secret");
    assert!(headers.get("authorization").is_none());
    assert_eq!(&body[..], b"pre-payload-post");

    instruction.additional_headers = OptionalNonNull::Present(Headers {
        values: vec![HeaderValue {
            name: "authorization".into(),
            value: "must-not-be-used".into(),
        }],
    });
    let error = client
        .upload_file_part_put(&instruction, UploadSource::from_bytes("payload"))
        .await
        .unwrap_err();
    assert!(matches!(error, HttpError::InvalidHeader(_)));
    assert!(!format!("{error:?}").contains("must-not-be-used"));
}

#[tokio::test]
async fn signed_advertised_query_is_redacted_from_all_error_fields() {
    let secret = "signed-query-secret";
    let encoded_secret = "signed%2Dquery%2Dsecret";
    let app = Router::new().route(
        "/selected",
        get(move |headers: HeaderMap| async move {
            assert!(headers.get(header::AUTHORIZATION).is_none());
            (
                StatusCode::BAD_REQUEST,
                Json(json!({"message": format!(
                    "echo {secret} {encoded_secret}"
                )})),
            )
        }),
    );
    let base = spawn(app).await;
    let url = format!("{base}/selected?sig={encoded_secret}");
    let client = local_client(&base).build().unwrap();
    let error = client.selected_documents(&url).await.unwrap_err();
    match error {
        HttpError::Response { diagnostic, .. } => {
            assert!(!diagnostic.expose_untrusted().contains(secret));
            assert!(!diagnostic.expose_untrusted().contains(encoded_secret));
            assert_eq!(format!("{diagnostic:?}"), "Diagnostic([REDACTED])");
            assert_eq!(
                diagnostic.to_string(),
                "[untrusted HTTP diagnostic redacted]"
            );
        }
        other => panic!("unexpected error: {other:?}"),
    }
}

#[tokio::test]
async fn clients_require_operation_status_and_accept_uppercase_json_suffix() {
    let app = Router::new()
        .route(
            "/foundation/versions",
            get(|| async {
                (
                    [(header::CONTENT_TYPE, "application/problem+JSON")],
                    "{\"versions\":[]}",
                )
            }),
        )
        .route(
            "/foundation/1.1/register",
            post(|| async {
                (
                    StatusCode::OK,
                    Json(json!({"client_id":"id","client_secret":"secret"})),
                )
            }),
        );
    let base = spawn(app).await;
    let client = local_client(&base).build().unwrap();
    assert!(client.versions().await.is_ok());
    let request: openbim_cde::foundation::DynamicClientRegistrationRequest =
        serde_json::from_value(json!({
            "client_name":"client",
            "redirect_url":"https://client.invalid/callback"
        }))
        .unwrap();
    let error = client
        .register_client(&format!("{base}/foundation/1.1/register"), &request)
        .await
        .unwrap_err();
    assert!(matches!(error, HttpError::Response { status: 200, .. }));
}

#[tokio::test]
async fn escaped_dynamic_registration_json_is_rejected_before_secret_deserialization() {
    let app = Router::new().route(
        "/register",
        post(|| async {
            (
                StatusCode::CREATED,
                [(header::CONTENT_TYPE, "application/json")],
                r#"{"client_id":"id","client_secret":"sec\u0072et-marker"}"#,
            )
        }),
    );
    let base = spawn(app).await;
    let client = local_client(&base).build().unwrap();
    let request: DynamicClientRegistrationRequest = serde_json::from_value(json!({
        "client_name":"client",
        "redirect_url":"https://client.invalid/callback"
    }))
    .unwrap();

    let error = client
        .register_client(&format!("{base}/register"), &request)
        .await
        .unwrap_err();
    assert!(matches!(error, HttpError::Json));
}

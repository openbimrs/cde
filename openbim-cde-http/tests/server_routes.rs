#![allow(missing_docs)]

use async_trait::async_trait;
use axum::{
    body::{to_bytes, Body, Bytes},
    http::{Request, StatusCode},
};
use futures_util::StreamExt;
use openbim_cde::documents::{
    DocumentDiscoverySessionInitialization, DocumentMetadata, DocumentQuery,
    DocumentUploadSessionInitialization, DocumentVersion, DocumentVersions, DocumentsToUpload,
    SelectDocuments, SelectedDocuments, UploadDocuments, UploadFileDetails,
};
use openbim_cde::foundation::{
    AuthenticationDiscovery, ClientSecret, DynamicClientRegistrationRequest,
    DynamicClientRegistrationResponse, User, VersionsResponse,
};
use openbim_cde_http::{
    documents_router, foundation_router, AllowAnonymous, AuthContext, AuthPolicy, AuthRequest,
    DocumentDownload, DocumentQueryServiceResponse, DocumentRoutes, DocumentsAuthPolicies,
    DocumentsService, FoundationAuthPolicies, FoundationRoutes, FoundationService, RequestContext,
    ServerLimits, ServiceError, UploadCancellationResponse, UploadPartResponse,
};
use std::sync::Arc;
use tower::ServiceExt;

fn unavailable() -> ServiceError {
    ServiceError::new(StatusCode::NOT_IMPLEMENTED, "not implemented")
}

async fn consume_upload(body: Body) -> Result<UploadPartResponse, ServiceError> {
    let mut stream = body.into_data_stream();
    while let Some(item) = stream.next().await {
        item.map_err(|_| ServiceError::new(StatusCode::PAYLOAD_TOO_LARGE, "upload too large"))?;
    }
    Ok(UploadPartResponse::default())
}

struct QueryAwareAuth;
struct RejectAuth;

#[async_trait]
impl AuthPolicy for RejectAuth {
    async fn authenticate(&self, _: &AuthRequest) -> Result<AuthContext, ServiceError> {
        Err(ServiceError::new(StatusCode::UNAUTHORIZED, "denied"))
    }
}

#[async_trait]
impl AuthPolicy for QueryAwareAuth {
    async fn authenticate(&self, request: &AuthRequest) -> Result<AuthContext, ServiceError> {
        if request.uri().query().is_some() {
            assert_eq!(request.uri().query(), Some("sig=request-secret"));
            assert!(!format!("{request:?}").contains("request-secret"));
        }
        Ok(AuthContext::anonymous())
    }
}

struct Foundation;

#[async_trait]
impl FoundationService for Foundation {
    async fn versions(&self, _: RequestContext) -> Result<VersionsResponse, ServiceError> {
        Ok(VersionsResponse { versions: vec![] })
    }
    async fn authentication(
        &self,
        _: RequestContext,
    ) -> Result<AuthenticationDiscovery, ServiceError> {
        Err(unavailable())
    }
    async fn register_client(
        &self,
        _: RequestContext,
        _: DynamicClientRegistrationRequest,
    ) -> Result<DynamicClientRegistrationResponse, ServiceError> {
        Ok(DynamicClientRegistrationResponse {
            client_id: "created-client".into(),
            client_secret: ClientSecret::new("created-secret"),
        })
    }
    async fn current_user(&self, _: RequestContext) -> Result<User, ServiceError> {
        Err(unavailable())
    }
}

struct Documents;

#[async_trait]
impl DocumentsService for Documents {
    async fn select_documents(
        &self,
        _: RequestContext,
        _: SelectDocuments,
    ) -> Result<DocumentDiscoverySessionInitialization, ServiceError> {
        Err(unavailable())
    }
    async fn selected_documents(
        &self,
        _: RequestContext,
    ) -> Result<SelectedDocuments, ServiceError> {
        Err(unavailable())
    }
    async fn upload_documents(
        &self,
        _: RequestContext,
        _: UploadDocuments,
    ) -> Result<DocumentUploadSessionInitialization, ServiceError> {
        Err(unavailable())
    }
    async fn download_document(&self, _: RequestContext) -> Result<DocumentDownload, ServiceError> {
        Err(unavailable())
    }
    async fn document_metadata(&self, _: RequestContext) -> Result<DocumentMetadata, ServiceError> {
        Err(unavailable())
    }
    async fn document_versions(&self, _: RequestContext) -> Result<DocumentVersions, ServiceError> {
        Err(unavailable())
    }
    async fn document_version(&self, _: RequestContext) -> Result<DocumentVersion, ServiceError> {
        Err(unavailable())
    }
    async fn upload_instructions(
        &self,
        _: RequestContext,
        _: UploadFileDetails,
    ) -> Result<DocumentsToUpload, ServiceError> {
        Err(unavailable())
    }
    async fn upload_file_part_put(
        &self,
        _: RequestContext,
        body: Body,
    ) -> Result<UploadPartResponse, ServiceError> {
        consume_upload(body).await
    }
    async fn upload_file_part_post(
        &self,
        _: RequestContext,
        body: Body,
    ) -> Result<UploadPartResponse, ServiceError> {
        consume_upload(body).await
    }
    async fn upload_completion(&self, _: RequestContext) -> Result<DocumentVersion, ServiceError> {
        Err(unavailable())
    }
    async fn upload_cancellation(
        &self,
        _: RequestContext,
    ) -> Result<UploadCancellationResponse, ServiceError> {
        Ok(UploadCancellationResponse::default())
    }
    async fn query_document_versions(
        &self,
        _: RequestContext,
        _: DocumentQuery,
        _: Option<String>,
    ) -> Result<DocumentQueryServiceResponse, ServiceError> {
        Err(unavailable())
    }
}

fn routes() -> DocumentRoutes {
    DocumentRoutes {
        select_documents: "/docs/select".into(),
        selected_documents: "/sessions/{session}/selected".into(),
        upload_documents: "/docs/upload".into(),
        document_download: "/files/{id}/content".into(),
        document_metadata: "/files/{id}/metadata".into(),
        document_versions: "/files/{id}/versions".into(),
        document_version: "/files/{id}/versions/{version}".into(),
        upload_instructions: "/uploads/{id}/instructions".into(),
        upload_file_part: "/uploads/{id}/parts/{part}".into(),
        upload_completion: "/uploads/{id}/complete".into(),
        upload_cancellation: "/uploads/{id}/cancel".into(),
        query_document_versions: "/docs/query".into(),
    }
}

#[tokio::test]
async fn foundation_routes_map_methods_and_bound_json_before_dispatch() {
    let app = foundation_router(
        Arc::new(Foundation),
        FoundationAuthPolicies::uniform(Arc::new(QueryAwareAuth)),
        FoundationRoutes::new("/foundation/1.1", "/oauth/register").unwrap(),
        ServerLimits {
            max_json_body: 128,
            ..ServerLimits::default()
        },
    )
    .unwrap();
    let response = app
        .clone()
        .oneshot(
            Request::get("/foundation/versions?sig=request-secret")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    let response = app
        .clone()
        .oneshot(
            Request::post("/foundation/versions")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::METHOD_NOT_ALLOWED);

    for path in ["/foundation/1.1/auth", "/foundation/1.1/current-user"] {
        let response = app
            .clone()
            .oneshot(Request::get(path).body(Body::empty()).unwrap())
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::NOT_IMPLEMENTED, "{path}");
    }
    let response = app
        .clone()
        .oneshot(
            Request::post("/oauth/register")
                .header("content-type", "application/problem+JSON")
                .body(Body::from(
                    r#"{"client_name":"client","redirect_url":"https://client.example/callback"}"#,
                ))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::CREATED);
    assert_eq!(response.headers()["content-type"], "application/json");
    let body = to_bytes(response.into_body(), 1024).await.unwrap();
    let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(json["client_id"], "created-client");

    let response = app
        .oneshot(
            Request::post("/oauth/register")
                .header("content-type", "application/problem+JSON")
                .body(Body::from("x".repeat(129)))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::PAYLOAD_TOO_LARGE);
}

#[tokio::test]
async fn documents_routes_use_host_templates_and_distinguish_part_put_and_post() {
    let app = documents_router(
        Arc::new(Documents),
        DocumentsAuthPolicies::uniform(Arc::new(AllowAnonymous)),
        routes(),
        ServerLimits::default(),
    )
    .unwrap();
    for method in ["PUT", "POST"] {
        let request = Request::builder()
            .method(method)
            .uri("/uploads/u/parts/1")
            .header("content-type", "application/octet-stream")
            .body(Body::from("streamed"))
            .unwrap();
        let response = app.clone().oneshot(request).await.unwrap();
        assert_eq!(response.status(), StatusCode::OK);
    }
    for path in [
        "/sessions/s/selected",
        "/files/f/content",
        "/files/f/metadata",
        "/files/f/versions",
        "/files/f/versions/v",
    ] {
        let response = app
            .clone()
            .oneshot(Request::get(path).body(Body::empty()).unwrap())
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::NOT_IMPLEMENTED, "{path}");
    }
    for path in ["/uploads/u/complete"] {
        let response = app
            .clone()
            .oneshot(Request::post(path).body(Body::empty()).unwrap())
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::NOT_IMPLEMENTED, "{path}");
    }
    let response = app
        .clone()
        .oneshot(
            Request::post("/uploads/u/cancel")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::NO_CONTENT);

    for path in [
        "/docs/select",
        "/docs/upload",
        "/uploads/u/instructions",
        "/docs/query",
    ] {
        let response = app
            .clone()
            .oneshot(
                Request::post(path)
                    .header("content-type", "application/problem+JSON")
                    .body(Body::from("{}"))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::BAD_REQUEST, "{path}");
    }
}

#[tokio::test]
async fn oversized_streaming_upload_is_rejected_before_service_dispatch() {
    let limits = ServerLimits {
        max_json_body: 128,
        max_upload_body: 4,
    };
    let app = documents_router(
        Arc::new(Documents),
        DocumentsAuthPolicies::uniform(Arc::new(AllowAnonymous)),
        routes(),
        limits,
    )
    .unwrap();
    let request = Request::put("/uploads/u/parts/1")
        .header("content-type", "application/octet-stream")
        .header("content-length", "8")
        .body(Body::from("too-long"))
        .unwrap();
    let response = app.clone().oneshot(request).await.unwrap();
    assert_eq!(response.status(), StatusCode::PAYLOAD_TOO_LARGE);

    let request = Request::put("/uploads/u/parts/1")
        .body(Body::from("tiny"))
        .unwrap();
    let response = app.clone().oneshot(request).await.unwrap();
    assert_eq!(response.status(), StatusCode::UNSUPPORTED_MEDIA_TYPE);

    let chunks = futures_util::stream::once(async {
        Ok::<Bytes, std::convert::Infallible>(Bytes::from_static(b"too-long"))
    });
    let request = Request::put("/uploads/u/parts/1")
        .header("content-type", "application/octet-stream")
        .body(Body::from_stream(chunks))
        .unwrap();
    let response = app.oneshot(request).await.unwrap();
    assert_eq!(response.status(), StatusCode::PAYLOAD_TOO_LARGE);
}

#[test]
fn placeholder_fake_routes_are_rejected() {
    let mut configured = routes();
    configured.document_download = "/server-provided-path-document-download".into();
    let error = documents_router(
        Arc::new(Documents),
        DocumentsAuthPolicies::uniform(Arc::new(AllowAnonymous)),
        configured,
        ServerLimits::default(),
    )
    .unwrap_err();
    assert!(error.to_string().contains("placeholder"));
}

#[test]
fn malformed_and_overlapping_routes_return_config_errors_without_panicking() {
    let mut malformed = routes();
    malformed.document_download = "/files/{id".into();
    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        documents_router(
            Arc::new(Documents),
            DocumentsAuthPolicies::uniform(Arc::new(AllowAnonymous)),
            malformed,
            ServerLimits::default(),
        )
    }));
    assert!(result.is_ok());
    assert!(result.unwrap().is_err());

    let mut overlap = routes();
    overlap.document_metadata = "/files/{other}/content".into();
    let result = documents_router(
        Arc::new(Documents),
        DocumentsAuthPolicies::uniform(Arc::new(AllowAnonymous)),
        overlap,
        ServerLimits::default(),
    );
    assert!(result.is_err());
}

#[tokio::test]
async fn documents_authentication_is_operation_specific() {
    let mut policies = DocumentsAuthPolicies::uniform(Arc::new(AllowAnonymous));
    policies.upload_cancellation = Arc::new(RejectAuth);
    let app = documents_router(
        Arc::new(Documents),
        policies,
        routes(),
        ServerLimits::default(),
    )
    .unwrap();

    let upload = app
        .clone()
        .oneshot(
            Request::put("/uploads/u/parts/1")
                .header("content-type", "application/octet-stream")
                .body(Body::from("part"))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(upload.status(), StatusCode::OK);

    let cancellation = app
        .oneshot(
            Request::post("/uploads/u/cancel")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(cancellation.status(), StatusCode::UNAUTHORIZED);
}

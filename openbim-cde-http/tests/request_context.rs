#![allow(missing_docs)]

use axum::http::{HeaderMap, HeaderValue, StatusCode};
use openbim_cde_http::{AuthContext, RequestContext, ServiceError, UploadPartResponse};
use std::collections::HashMap;

#[test]
fn request_context_preserves_headers_without_debugging_values() {
    let mut headers = HeaderMap::new();
    headers.insert("x-upload-token", HeaderValue::from_static("secret-marker"));
    let mut parameters = HashMap::new();
    parameters.insert("upload_token".into(), "signed-path-secret".into());
    let context = RequestContext {
        auth: AuthContext::anonymous(),
        parameters,
        uri: "/upload?sig=query-secret".parse().unwrap(),
        headers,
    };

    assert_eq!(context.headers["x-upload-token"], "secret-marker");
    let debug = format!("{context:?}");
    assert!(debug.contains("x-upload-token"));
    assert!(debug.contains("upload_token"));
    assert!(!debug.contains("secret-marker"));
    assert!(!debug.contains("signed-path-secret"));
    assert!(!debug.contains("query-secret"));
}

#[test]
fn upload_response_debug_omits_header_values() {
    let mut headers = HeaderMap::new();
    headers.insert(
        "x-upload-token",
        HeaderValue::from_static("response-secret"),
    );
    let response = UploadPartResponse { headers };
    let debug = format!("{response:?}");
    assert!(debug.contains("x-upload-token"));
    assert!(!debug.contains("response-secret"));
}

#[test]
fn service_error_debug_omits_public_message() {
    let error = ServiceError::new(StatusCode::INTERNAL_SERVER_ERROR, "service-secret");
    let debug = format!("{error:?}");
    let display = error.to_string();
    assert!(!debug.contains("service-secret"));
    assert!(!display.contains("service-secret"));
}

use super::{
    AuthPolicy, AuthRequest, DocumentDownload, DocumentRoutes, DocumentsAuthPolicies,
    DocumentsService, FoundationAuthPolicies, FoundationRoutes, FoundationService, RequestContext,
    ServerLimits, UploadCancellationResponse, UploadPartResponse,
};
use crate::{ConfigError, ServiceError};
use axum::{
    body::{to_bytes, Body},
    extract::{RawPathParams, State},
    http::{header, HeaderMap, Request, StatusCode},
    response::{IntoResponse, Response},
    routing::{get, post, put},
    Json, Router,
};
use openbim_cde::{
    documents::{DocumentQuery, SelectDocuments, UploadDocuments, UploadFileDetails},
    foundation::{DynamicClientRegistrationRequest, VERSIONS_PATH},
};
use serde::{de::DeserializeOwned, Serialize};
use std::{
    collections::HashMap,
    panic::{catch_unwind, AssertUnwindSafe},
    sync::Arc,
};
use tower_http::limit::RequestBodyLimitLayer;

#[derive(Clone)]
struct FoundationState {
    service: Arc<dyn FoundationService>,
    auth: FoundationAuthPolicies,
    limits: ServerLimits,
}

#[derive(Clone)]
struct DocumentsState {
    service: Arc<dyn DocumentsService>,
    auth: DocumentsAuthPolicies,
    limits: ServerLimits,
}

/// Build the Foundation router.
///
/// The discovery route is always mounted at `/foundation/versions`. The three
/// remaining routes are taken from [`FoundationRoutes`].
pub fn foundation_router(
    service: Arc<dyn FoundationService>,
    auth: FoundationAuthPolicies,
    routes: FoundationRoutes,
    limits: ServerLimits,
) -> Result<Router, ConfigError> {
    routes.validate()?;
    limits.validate()?;
    ensure_distinct(&[
        VERSIONS_PATH,
        &routes.authentication,
        &routes.registration,
        &routes.current_user,
    ])?;
    let state = Arc::new(FoundationState {
        service,
        auth,
        limits,
    });
    build_router(|| {
        Router::new()
            .route(VERSIONS_PATH, get(foundation_versions))
            .route(&routes.authentication, get(foundation_authentication))
            .route(&routes.registration, post(foundation_register_client))
            .route(&routes.current_user, get(foundation_current_user))
            .with_state(state)
    })
}

/// Build the Documents router covering all thirteen operations.
///
/// Every route is host-defined through [`DocumentRoutes`]. Placeholder OpenAPI
/// paths that contain `server-provided-path` are rejected.
pub fn documents_router(
    service: Arc<dyn DocumentsService>,
    auth: DocumentsAuthPolicies,
    routes: DocumentRoutes,
    limits: ServerLimits,
) -> Result<Router, ConfigError> {
    routes.validate()?;
    limits.validate()?;
    ensure_distinct(&[
        &routes.select_documents,
        &routes.selected_documents,
        &routes.upload_documents,
        &routes.document_download,
        &routes.document_metadata,
        &routes.document_versions,
        &routes.document_version,
        &routes.upload_instructions,
        &routes.upload_file_part,
        &routes.upload_completion,
        &routes.upload_cancellation,
        &routes.query_document_versions,
    ])?;
    let upload_limit = limits.max_upload_body;
    let state = Arc::new(DocumentsState {
        service,
        auth,
        limits,
    });
    build_router(|| {
        Router::new()
            .route(&routes.select_documents, post(select_documents))
            .route(&routes.selected_documents, get(selected_documents))
            .route(&routes.upload_documents, post(upload_documents))
            .route(&routes.document_download, get(download_document))
            .route(&routes.document_metadata, get(document_metadata))
            .route(&routes.document_versions, get(document_versions))
            .route(&routes.document_version, get(document_version))
            .route(&routes.upload_instructions, post(upload_instructions))
            .route(
                &routes.upload_file_part,
                put(upload_file_part_put)
                    .post(upload_file_part_post)
                    .layer(RequestBodyLimitLayer::new(upload_limit)),
            )
            .route(&routes.upload_completion, post(upload_completion))
            .route(&routes.upload_cancellation, post(upload_cancellation))
            .route(
                &routes.query_document_versions,
                post(query_document_versions),
            )
            .with_state(state)
    })
}

fn ensure_distinct(routes: &[&str]) -> Result<(), ConfigError> {
    let mut seen = std::collections::BTreeSet::new();
    for route in routes {
        let shape = route
            .split('/')
            .map(|segment| {
                if segment.starts_with('{') && segment.ends_with('}') {
                    "{}"
                } else {
                    segment
                }
            })
            .collect::<Vec<_>>()
            .join("/");
        if !seen.insert(shape) {
            return Err(ConfigError::new("routes must have distinct match shapes"));
        }
    }
    Ok(())
}

fn build_router(build: impl FnOnce() -> Router) -> Result<Router, ConfigError> {
    catch_unwind(AssertUnwindSafe(build))
        .map_err(|_| ConfigError::new("route templates are malformed or conflicting"))
}

async fn foundation_versions(
    State(state): State<Arc<FoundationState>>,
    params: RawPathParams,
    request: Request<Body>,
) -> Response {
    let context = match build_context(
        &state.auth.versions,
        &params,
        request.uri(),
        request.headers(),
    )
    .await
    {
        Ok(value) => value,
        Err(error) => return error.into_response(),
    };
    json_result(state.service.versions(context).await)
}

async fn foundation_authentication(
    State(state): State<Arc<FoundationState>>,
    params: RawPathParams,
    request: Request<Body>,
) -> Response {
    let context = match build_context(
        &state.auth.authentication,
        &params,
        request.uri(),
        request.headers(),
    )
    .await
    {
        Ok(value) => value,
        Err(error) => return error.into_response(),
    };
    json_result(state.service.authentication(context).await)
}

async fn foundation_register_client(
    State(state): State<Arc<FoundationState>>,
    params: RawPathParams,
    request: Request<Body>,
) -> Response {
    let context = match build_context(
        &state.auth.registration,
        &params,
        request.uri(),
        request.headers(),
    )
    .await
    {
        Ok(value) => value,
        Err(error) => return error.into_response(),
    };
    let body: DynamicClientRegistrationRequest =
        match json_body(request, state.limits.max_json_body).await {
            Ok(value) => value,
            Err(error) => return error.into_response(),
        };
    json_result_with_status(
        state.service.register_client(context, body).await,
        StatusCode::CREATED,
    )
}

async fn foundation_current_user(
    State(state): State<Arc<FoundationState>>,
    params: RawPathParams,
    request: Request<Body>,
) -> Response {
    let context = match build_context(
        &state.auth.current_user,
        &params,
        request.uri(),
        request.headers(),
    )
    .await
    {
        Ok(value) => value,
        Err(error) => return error.into_response(),
    };
    json_result(state.service.current_user(context).await)
}

async fn select_documents(
    State(state): State<Arc<DocumentsState>>,
    params: RawPathParams,
    request: Request<Body>,
) -> Response {
    let context = match build_context(
        &state.auth.select_documents,
        &params,
        request.uri(),
        request.headers(),
    )
    .await
    {
        Ok(value) => value,
        Err(error) => return error.into_response(),
    };
    let body: SelectDocuments = match json_body(request, state.limits.max_json_body).await {
        Ok(value) => value,
        Err(error) => return error.into_response(),
    };
    json_result(state.service.select_documents(context, body).await)
}

async fn selected_documents(
    State(state): State<Arc<DocumentsState>>,
    params: RawPathParams,
    request: Request<Body>,
) -> Response {
    let context = match build_context(
        &state.auth.selected_documents,
        &params,
        request.uri(),
        request.headers(),
    )
    .await
    {
        Ok(value) => value,
        Err(error) => return error.into_response(),
    };
    json_result(state.service.selected_documents(context).await)
}

async fn upload_documents(
    State(state): State<Arc<DocumentsState>>,
    params: RawPathParams,
    request: Request<Body>,
) -> Response {
    let context = match build_context(
        &state.auth.upload_documents,
        &params,
        request.uri(),
        request.headers(),
    )
    .await
    {
        Ok(value) => value,
        Err(error) => return error.into_response(),
    };
    let body: UploadDocuments = match json_body(request, state.limits.max_json_body).await {
        Ok(value) => value,
        Err(error) => return error.into_response(),
    };
    json_result(state.service.upload_documents(context, body).await)
}

async fn download_document(
    State(state): State<Arc<DocumentsState>>,
    params: RawPathParams,
    request: Request<Body>,
) -> Response {
    let context = match build_context(
        &state.auth.download_document,
        &params,
        request.uri(),
        request.headers(),
    )
    .await
    {
        Ok(value) => value,
        Err(error) => return error.into_response(),
    };
    match state.service.download_document(context).await {
        Ok(download) => download_response(download),
        Err(error) => error.into_response(),
    }
}

async fn document_metadata(
    State(state): State<Arc<DocumentsState>>,
    params: RawPathParams,
    request: Request<Body>,
) -> Response {
    let context = match build_context(
        &state.auth.document_metadata,
        &params,
        request.uri(),
        request.headers(),
    )
    .await
    {
        Ok(value) => value,
        Err(error) => return error.into_response(),
    };
    json_result(state.service.document_metadata(context).await)
}

async fn document_versions(
    State(state): State<Arc<DocumentsState>>,
    params: RawPathParams,
    request: Request<Body>,
) -> Response {
    let context = match build_context(
        &state.auth.document_versions,
        &params,
        request.uri(),
        request.headers(),
    )
    .await
    {
        Ok(value) => value,
        Err(error) => return error.into_response(),
    };
    json_result(state.service.document_versions(context).await)
}

async fn document_version(
    State(state): State<Arc<DocumentsState>>,
    params: RawPathParams,
    request: Request<Body>,
) -> Response {
    let context = match build_context(
        &state.auth.document_version,
        &params,
        request.uri(),
        request.headers(),
    )
    .await
    {
        Ok(value) => value,
        Err(error) => return error.into_response(),
    };
    json_result(state.service.document_version(context).await)
}

async fn upload_instructions(
    State(state): State<Arc<DocumentsState>>,
    params: RawPathParams,
    request: Request<Body>,
) -> Response {
    let context = match build_context(
        &state.auth.upload_instructions,
        &params,
        request.uri(),
        request.headers(),
    )
    .await
    {
        Ok(value) => value,
        Err(error) => return error.into_response(),
    };
    let body: UploadFileDetails = match json_body(request, state.limits.max_json_body).await {
        Ok(value) => value,
        Err(error) => return error.into_response(),
    };
    json_result(state.service.upload_instructions(context, body).await)
}

async fn upload_file_part_put(
    State(state): State<Arc<DocumentsState>>,
    params: RawPathParams,
    request: Request<Body>,
) -> Response {
    if !is_octet_stream(request.headers()) {
        return ServiceError::new(
            StatusCode::UNSUPPORTED_MEDIA_TYPE,
            "upload content type must be application/octet-stream",
        )
        .into_response();
    }
    let context = match build_context(
        &state.auth.upload_file_part_put,
        &params,
        request.uri(),
        request.headers(),
    )
    .await
    {
        Ok(value) => value,
        Err(error) => return error.into_response(),
    };
    upload_result(
        state
            .service
            .upload_file_part_put(context, request.into_body())
            .await,
    )
}

async fn upload_file_part_post(
    State(state): State<Arc<DocumentsState>>,
    params: RawPathParams,
    request: Request<Body>,
) -> Response {
    if !is_octet_stream(request.headers()) {
        return ServiceError::new(
            StatusCode::UNSUPPORTED_MEDIA_TYPE,
            "upload content type must be application/octet-stream",
        )
        .into_response();
    }
    let context = match build_context(
        &state.auth.upload_file_part_post,
        &params,
        request.uri(),
        request.headers(),
    )
    .await
    {
        Ok(value) => value,
        Err(error) => return error.into_response(),
    };
    upload_result(
        state
            .service
            .upload_file_part_post(context, request.into_body())
            .await,
    )
}

async fn upload_completion(
    State(state): State<Arc<DocumentsState>>,
    params: RawPathParams,
    request: Request<Body>,
) -> Response {
    let context = match build_context(
        &state.auth.upload_completion,
        &params,
        request.uri(),
        request.headers(),
    )
    .await
    {
        Ok(value) => value,
        Err(error) => return error.into_response(),
    };
    json_result(state.service.upload_completion(context).await)
}

async fn upload_cancellation(
    State(state): State<Arc<DocumentsState>>,
    params: RawPathParams,
    request: Request<Body>,
) -> Response {
    let context = match build_context(
        &state.auth.upload_cancellation,
        &params,
        request.uri(),
        request.headers(),
    )
    .await
    {
        Ok(value) => value,
        Err(error) => return error.into_response(),
    };
    cancellation_result(state.service.upload_cancellation(context).await)
}

async fn query_document_versions(
    State(state): State<Arc<DocumentsState>>,
    params: RawPathParams,
    request: Request<Body>,
) -> Response {
    let if_none_match = request
        .headers()
        .get(header::IF_NONE_MATCH)
        .and_then(|value| value.to_str().ok())
        .map(str::to_owned);
    let context = match build_context(
        &state.auth.query_document_versions,
        &params,
        request.uri(),
        request.headers(),
    )
    .await
    {
        Ok(value) => value,
        Err(error) => return error.into_response(),
    };
    let body: DocumentQuery = match json_body(request, state.limits.max_json_body).await {
        Ok(value) => value,
        Err(error) => return error.into_response(),
    };
    match state
        .service
        .query_document_versions(context, body, if_none_match)
        .await
    {
        Ok(Some(metadata)) => {
            let mut response = Json(metadata.value).into_response();
            if let Some(etag) = metadata.etag {
                response.headers_mut().insert(header::ETAG, etag);
            }
            response
        }
        Ok(None) => StatusCode::NOT_MODIFIED.into_response(),
        Err(error) => error.into_response(),
    }
}

async fn build_context(
    auth: &Arc<dyn AuthPolicy>,
    params: &RawPathParams,
    uri: &axum::http::Uri,
    headers: &HeaderMap,
) -> Result<RequestContext, ServiceError> {
    let mut parameters = HashMap::new();
    for (name, value) in params.iter() {
        parameters.insert(name.to_owned(), value.to_owned());
    }
    let auth_request = AuthRequest::new(uri.clone(), headers.clone());
    Ok(RequestContext {
        auth: auth.authenticate(&auth_request).await?,
        parameters,
        uri: uri.clone(),
        headers: headers.clone(),
    })
}

async fn json_body<T: DeserializeOwned>(
    request: Request<Body>,
    limit: usize,
) -> Result<T, ServiceError> {
    if !is_json(request.headers()) {
        return Err(ServiceError::new(
            StatusCode::UNSUPPORTED_MEDIA_TYPE,
            "request content type must be application/json",
        ));
    }
    if request
        .headers()
        .get(header::CONTENT_LENGTH)
        .and_then(|value| value.to_str().ok())
        .and_then(|value| value.parse::<usize>().ok())
        .is_some_and(|length| length > limit)
    {
        return Err(ServiceError::new(
            StatusCode::PAYLOAD_TOO_LARGE,
            "JSON request body exceeds configured limit",
        ));
    }
    let bytes = to_bytes(request.into_body(), limit).await.map_err(|_| {
        ServiceError::new(
            StatusCode::PAYLOAD_TOO_LARGE,
            "JSON request body exceeds configured limit",
        )
    })?;
    serde_json::from_slice(&bytes)
        .map_err(|_| ServiceError::new(StatusCode::BAD_REQUEST, "invalid JSON request body"))
}

fn is_octet_stream(headers: &HeaderMap) -> bool {
    headers
        .get(header::CONTENT_TYPE)
        .and_then(|value| value.to_str().ok())
        .and_then(|value| value.split(';').next())
        .map(str::trim)
        .is_some_and(|value| value.eq_ignore_ascii_case("application/octet-stream"))
}

fn is_json(headers: &HeaderMap) -> bool {
    headers
        .get(header::CONTENT_TYPE)
        .and_then(|value| value.to_str().ok())
        .and_then(|value| value.split(';').next())
        .map(str::trim)
        .is_some_and(|value| {
            value.eq_ignore_ascii_case("application/json")
                || value
                    .get(value.len().saturating_sub(5)..)
                    .is_some_and(|suffix| suffix.eq_ignore_ascii_case("+json"))
        })
}

fn json_result<T: Serialize>(result: Result<T, ServiceError>) -> Response {
    json_result_with_status(result, StatusCode::OK)
}

fn json_result_with_status<T: Serialize>(
    result: Result<T, ServiceError>,
    status: StatusCode,
) -> Response {
    match result {
        Ok(value) => (status, Json(value)).into_response(),
        Err(error) => error.into_response(),
    }
}

fn upload_result(result: Result<UploadPartResponse, ServiceError>) -> Response {
    match result {
        Ok(value) => {
            let mut response = StatusCode::OK.into_response();
            for (name, header_value) in value.headers.iter() {
                response
                    .headers_mut()
                    .append(name.clone(), header_value.clone());
            }
            response
        }
        Err(error) => error.into_response(),
    }
}

fn cancellation_result(result: Result<UploadCancellationResponse, ServiceError>) -> Response {
    match result {
        Ok(value) => {
            let mut response = StatusCode::NO_CONTENT.into_response();
            for (name, header_value) in value.headers.iter() {
                response
                    .headers_mut()
                    .append(name.clone(), header_value.clone());
            }
            response
        }
        Err(error) => error.into_response(),
    }
}

fn download_response(download: DocumentDownload) -> Response {
    let mut response = Response::new(download.body);
    *response.status_mut() = StatusCode::OK;
    response
        .headers_mut()
        .insert(header::CONTENT_TYPE, download.content_type);
    if let Some(length) = download.content_length {
        if let Ok(value) = length.to_string().parse() {
            response.headers_mut().insert(header::CONTENT_LENGTH, value);
        }
    }
    for (name, header_value) in download.headers.iter() {
        response
            .headers_mut()
            .append(name.clone(), header_value.clone());
    }
    response
}

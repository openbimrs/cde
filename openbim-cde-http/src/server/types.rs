use crate::{ConfigError, ServiceError};
use async_trait::async_trait;
use axum::{
    body::Body,
    http::{HeaderMap, HeaderValue, Uri},
};
use openbim_cde::{
    documents::DocumentQueryResult,
    foundation::{
        AuthenticationDiscovery, DynamicClientRegistrationRequest,
        DynamicClientRegistrationResponse, User, VersionsResponse,
    },
};
use std::{collections::HashMap, fmt, sync::Arc};

/// Identity information supplied by an injected authentication policy.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct AuthContext {
    /// Stable subject identifier, when known.
    pub subject: Option<String>,
}

impl AuthContext {
    /// Anonymous context helper.
    #[must_use]
    pub fn anonymous() -> Self {
        Self::default()
    }

    /// Whether the context has no subject.
    #[must_use]
    pub fn is_anonymous(&self) -> bool {
        self.subject.is_none()
    }
}

/// Per-request context supplied to service methods.
///
/// Header values can contain credentials or signed-upload material, so `Debug`
/// reports names only. Service implementations must treat explicit access to
/// [`Self::headers`] as sensitive.
#[derive(Clone, Default, PartialEq, Eq)]
pub struct RequestContext {
    /// Authentication result.
    pub auth: AuthContext,
    /// Values captured from the host-configured route template.
    pub parameters: HashMap<String, String>,
    /// Original request URI, including its query string.
    ///
    /// Signed URL query parameters are sensitive. Access is explicit and Debug
    /// output never includes URI values.
    pub uri: Uri,
    /// Original request headers.
    pub headers: HeaderMap,
}

impl fmt::Debug for RequestContext {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        let header_names: Vec<_> = self.headers.keys().map(|name| name.as_str()).collect();
        let parameter_names: Vec<_> = self.parameters.keys().map(String::as_str).collect();
        formatter
            .debug_struct("RequestContext")
            .field("auth", &self.auth)
            .field("parameter_names", &parameter_names)
            .field("uri", &"[REDACTED]")
            .field("has_query", &self.uri.query().is_some())
            .field("header_names", &header_names)
            .finish()
    }
}

/// Sensitive request input supplied to an authentication policy.
///
/// URI and header values are available explicitly but remain opaque in `Debug`.
#[derive(Clone, Default, PartialEq, Eq)]
pub struct AuthRequest {
    uri: Uri,
    headers: HeaderMap,
}

impl AuthRequest {
    pub(crate) fn new(uri: Uri, headers: HeaderMap) -> Self {
        Self { uri, headers }
    }

    /// Original request URI, including the query string.
    #[must_use]
    pub fn uri(&self) -> &Uri {
        &self.uri
    }

    /// Original request headers.
    #[must_use]
    pub fn headers(&self) -> &HeaderMap {
        &self.headers
    }
}

impl fmt::Debug for AuthRequest {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        let header_names: Vec<_> = self.headers.keys().map(|name| name.as_str()).collect();
        formatter
            .debug_struct("AuthRequest")
            .field("uri", &"[REDACTED]")
            .field("has_query", &self.uri.query().is_some())
            .field("header_names", &header_names)
            .finish()
    }
}

/// Object-safe asynchronous request authentication policy.
#[async_trait]
pub trait AuthPolicy: Send + Sync {
    /// Authenticate a request or reject it.
    async fn authenticate(&self, request: &AuthRequest) -> Result<AuthContext, ServiceError>;
}

/// Authentication policy that permits every request.
#[derive(Debug, Default, Clone, Copy)]
pub struct AllowAnonymous;

#[async_trait]
impl AuthPolicy for AllowAnonymous {
    async fn authenticate(&self, _request: &AuthRequest) -> Result<AuthContext, ServiceError> {
        Ok(AuthContext::default())
    }
}

/// Per-route Foundation authentication policies.
#[derive(Clone)]
pub struct FoundationAuthPolicies {
    /// Versions discovery policy.
    pub versions: Arc<dyn AuthPolicy>,
    /// Authentication discovery policy.
    pub authentication: Arc<dyn AuthPolicy>,
    /// Dynamic registration policy.
    pub registration: Arc<dyn AuthPolicy>,
    /// Current-user policy.
    pub current_user: Arc<dyn AuthPolicy>,
}

impl FoundationAuthPolicies {
    /// Configure every Foundation route independently.
    #[must_use]
    pub fn new(
        versions: Arc<dyn AuthPolicy>,
        authentication: Arc<dyn AuthPolicy>,
        registration: Arc<dyn AuthPolicy>,
        current_user: Arc<dyn AuthPolicy>,
    ) -> Self {
        Self {
            versions,
            authentication,
            registration,
            current_user,
        }
    }

    /// Apply one policy to all Foundation routes.
    #[must_use]
    pub fn uniform(policy: Arc<dyn AuthPolicy>) -> Self {
        Self::new(policy.clone(), policy.clone(), policy.clone(), policy)
    }

    /// Make discovery and registration public while protecting current-user.
    #[must_use]
    pub fn public_discovery(current_user: Arc<dyn AuthPolicy>) -> Self {
        let public: Arc<dyn AuthPolicy> = Arc::new(AllowAnonymous);
        Self::new(public.clone(), public.clone(), public, current_user)
    }
}

impl fmt::Debug for FoundationAuthPolicies {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("FoundationAuthPolicies { .. }")
    }
}

/// Operation-specific authentication policies for Documents routes.
#[derive(Clone)]
pub struct DocumentsAuthPolicies {
    /// Policy for selecting documents.
    pub select_documents: Arc<dyn AuthPolicy>,
    /// Policy for retrieving the current selection.
    pub selected_documents: Arc<dyn AuthPolicy>,
    /// Policy for initiating document uploads.
    pub upload_documents: Arc<dyn AuthPolicy>,
    /// Policy for downloading document content.
    pub download_document: Arc<dyn AuthPolicy>,
    /// Policy for document metadata.
    pub document_metadata: Arc<dyn AuthPolicy>,
    /// Policy for a document's version collection.
    pub document_versions: Arc<dyn AuthPolicy>,
    /// Policy for one document version.
    pub document_version: Arc<dyn AuthPolicy>,
    /// Policy for obtaining upload instructions.
    pub upload_instructions: Arc<dyn AuthPolicy>,
    /// Policy for PUT upload parts.
    pub upload_file_part_put: Arc<dyn AuthPolicy>,
    /// Policy for POST upload parts.
    pub upload_file_part_post: Arc<dyn AuthPolicy>,
    /// Policy for upload completion.
    pub upload_completion: Arc<dyn AuthPolicy>,
    /// Policy for upload cancellation.
    pub upload_cancellation: Arc<dyn AuthPolicy>,
    /// Policy for document-version queries.
    pub query_document_versions: Arc<dyn AuthPolicy>,
}

impl DocumentsAuthPolicies {
    /// Apply one policy to all Documents routes.
    #[must_use]
    pub fn uniform(policy: Arc<dyn AuthPolicy>) -> Self {
        Self {
            select_documents: policy.clone(),
            selected_documents: policy.clone(),
            upload_documents: policy.clone(),
            download_document: policy.clone(),
            document_metadata: policy.clone(),
            document_versions: policy.clone(),
            document_version: policy.clone(),
            upload_instructions: policy.clone(),
            upload_file_part_put: policy.clone(),
            upload_file_part_post: policy.clone(),
            upload_completion: policy.clone(),
            upload_cancellation: policy.clone(),
            query_document_versions: policy,
        }
    }
}

impl fmt::Debug for DocumentsAuthPolicies {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("DocumentsAuthPolicies { .. }")
    }
}

/// Bounded server request policy.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ServerLimits {
    /// Maximum JSON request bytes accepted by typed endpoints.
    pub max_json_body: usize,
    /// Maximum streamed upload bytes accepted by file-part endpoints.
    pub max_upload_body: usize,
}

impl Default for ServerLimits {
    fn default() -> Self {
        Self {
            max_json_body: 1024 * 1024,
            max_upload_body: 1024 * 1024 * 1024,
        }
    }
}

impl ServerLimits {
    pub(crate) fn validate(&self) -> Result<(), ConfigError> {
        if self.max_json_body == 0 {
            return Err(ConfigError::new("max_json_body must be non-zero"));
        }
        if self.max_upload_body == 0 {
            return Err(ConfigError::new("max_upload_body must be non-zero"));
        }
        Ok(())
    }
}

/// Foundation route configuration.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct FoundationRoutes {
    /// Authentication discovery path.
    pub authentication: String,
    /// Dynamic client registration path advertised by this host.
    pub registration: String,
    /// Current-user path.
    pub current_user: String,
}

impl FoundationRoutes {
    /// Derive Foundation paths from a base path and a host-advertised registration path.
    ///
    /// `base_path` should reflect the API base declared to callers, for example
    /// `/foundation/1.1`. The `authentication` and `current_user` routes are
    /// derived by appending the standardized suffixes. The registration route is
    /// host-defined and passed through verbatim.
    pub fn new(base_path: &str, registration_path: &str) -> Result<Self, ConfigError> {
        let base = validate_base_path(base_path)?;
        let registration = validate_route(registration_path)?.to_owned();
        Ok(Self {
            authentication: format!("{base}auth"),
            registration,
            current_user: format!("{base}current-user"),
        })
    }

    pub(crate) fn validate(&self) -> Result<(), ConfigError> {
        validate_route(&self.authentication)?;
        validate_route(&self.registration)?;
        validate_route(&self.current_user)?;
        Ok(())
    }
}

/// Host-provided Documents route templates.
///
/// No defaults exist because the Documents specification deliberately advertises
/// server-provided URLs. Axum parameters use `{name}` syntax.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DocumentRoutes {
    /// Standard select-documents route.
    pub select_documents: String,
    /// Advertised selected-documents route template.
    pub selected_documents: String,
    /// Standard upload-documents route.
    pub upload_documents: String,
    /// Advertised download route template.
    pub document_download: String,
    /// Advertised metadata route template.
    pub document_metadata: String,
    /// Advertised document-versions route template.
    pub document_versions: String,
    /// Advertised single-version route template.
    pub document_version: String,
    /// Advertised upload-instructions route template.
    pub upload_instructions: String,
    /// Advertised file-part route template, shared by PUT and POST.
    pub upload_file_part: String,
    /// Advertised completion route template.
    pub upload_completion: String,
    /// Advertised cancellation route template.
    pub upload_cancellation: String,
    /// Standard document query route.
    pub query_document_versions: String,
}

impl DocumentRoutes {
    pub(crate) fn validate(&self) -> Result<(), ConfigError> {
        for route in [
            &self.select_documents,
            &self.selected_documents,
            &self.upload_documents,
            &self.document_download,
            &self.document_metadata,
            &self.document_versions,
            &self.document_version,
            &self.upload_instructions,
            &self.upload_file_part,
            &self.upload_completion,
            &self.upload_cancellation,
            &self.query_document_versions,
        ] {
            validate_route(route)?;
            if route.contains("server-provided-path") {
                return Err(ConfigError::new(
                    "placeholder paths from the OpenAPI document are not routable",
                ));
            }
        }
        Ok(())
    }
}

/// JSON response plus protocol metadata.
#[derive(Clone, Debug, PartialEq)]
pub struct ResponseMetadata<T> {
    /// Response value.
    pub value: T,
    /// Optional entity tag.
    pub etag: Option<HeaderValue>,
}

impl<T> ResponseMetadata<T> {
    /// Construct a response without an entity tag.
    #[must_use]
    pub const fn new(value: T) -> Self {
        Self { value, etag: None }
    }

    /// Attach an entity tag to the response.
    #[must_use]
    pub fn with_etag(mut self, etag: HeaderValue) -> Self {
        self.etag = Some(etag);
        self
    }
}

/// Streaming service download response.
pub struct DocumentDownload {
    /// Backpressure-aware response body.
    pub body: Body,
    /// Media type of the document.
    pub content_type: HeaderValue,
    /// Known content length, if available.
    pub content_length: Option<u64>,
    /// Additional safe response headers.
    pub headers: HeaderMap,
}

impl fmt::Debug for DocumentDownload {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("DocumentDownload")
            .field("body", &"<stream>")
            .field("content_type", &self.content_type)
            .field("content_length", &self.content_length)
            .finish_non_exhaustive()
    }
}

/// Headers returned after a successful streamed upload part.
#[derive(Clone, Default)]
pub struct UploadPartResponse {
    /// Response headers.
    pub headers: HeaderMap,
}

impl fmt::Debug for UploadPartResponse {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        let header_names: Vec<_> = self.headers.keys().map(|name| name.as_str()).collect();
        formatter
            .debug_struct("UploadPartResponse")
            .field("status", &"200 OK (fixed)")
            .field("header_names", &header_names)
            .finish()
    }
}

/// Headers returned after successful upload cancellation.
#[derive(Clone, Default)]
pub struct UploadCancellationResponse {
    /// Response headers.
    pub headers: HeaderMap,
}

impl fmt::Debug for UploadCancellationResponse {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        let header_names: Vec<_> = self.headers.keys().map(|name| name.as_str()).collect();
        formatter
            .debug_struct("UploadCancellationResponse")
            .field("status", &"204 No Content (fixed)")
            .field("header_names", &header_names)
            .finish()
    }
}

/// Object-safe asynchronous implementation of all Foundation operations.
#[async_trait]
pub trait FoundationService: Send + Sync {
    /// `GET /foundation/versions`.
    async fn versions(&self, context: RequestContext) -> Result<VersionsResponse, ServiceError>;
    /// Authentication discovery GET.
    async fn authentication(
        &self,
        context: RequestContext,
    ) -> Result<AuthenticationDiscovery, ServiceError>;
    /// Dynamic client registration POST.
    async fn register_client(
        &self,
        context: RequestContext,
        request: DynamicClientRegistrationRequest,
    ) -> Result<DynamicClientRegistrationResponse, ServiceError>;
    /// Current-user GET.
    async fn current_user(&self, context: RequestContext) -> Result<User, ServiceError>;
}

pub(crate) fn validate_route(route: &str) -> Result<&str, ConfigError> {
    if !route.starts_with('/') || route.contains('?') || route.contains('#') || route.contains("//")
    {
        return Err(ConfigError::new(
            "routes must be absolute path templates without query or fragment",
        ));
    }
    Ok(route)
}

fn validate_base_path(base: &str) -> Result<String, ConfigError> {
    validate_route(base)?;
    Ok(if base.ends_with('/') {
        base.to_owned()
    } else {
        format!("{base}/")
    })
}

/// Result of a document-versions query: `None` when the current entity tag was
/// still valid (`If-None-Match` matched), otherwise the current result.
/// Optional query result; `None` means the client ETag matched and the router returns 304.
pub type DocumentQueryServiceResponse = Option<ResponseMetadata<DocumentQueryResult>>;

use crate::{Diagnostic, HttpError};
use base64::{engine::general_purpose::STANDARD, Engine as _};
use bytes::Bytes;
use futures_core::Stream;
use futures_util::{stream, StreamExt, TryStreamExt};
use openbim_cde::{
    documents::{
        DocumentDiscoverySessionInitialization, DocumentMetadata, DocumentQuery,
        DocumentQueryResult, DocumentUploadSessionInitialization, DocumentVersion,
        DocumentVersions, DocumentsToUpload, Headers, HttpMethod, SelectDocuments,
        SelectedDocuments, UploadDocuments, UploadFileDetails, UploadFilePartInstruction,
        SELECT_DOCUMENTS_PATH, UPLOAD_DOCUMENTS_PATH,
    },
    foundation::{
        AuthenticationDiscovery, DynamicClientRegistrationRequest,
        DynamicClientRegistrationResponse, User, VersionsResponse, VERSIONS_PATH,
    },
};
use reqwest::{
    header::{
        HeaderMap, HeaderName, HeaderValue, AUTHORIZATION, CONTENT_LENGTH, CONTENT_TYPE, ETAG,
        HOST, PROXY_AUTHORIZATION, TRANSFER_ENCODING,
    },
    Method, RequestBuilder, Response, StatusCode, Url,
};
use serde::{de::DeserializeOwned, Serialize};
use std::{fmt, pin::Pin, time::Duration};
use zeroize::{Zeroize, ZeroizeOnDrop, Zeroizing};

type UploadPiece = Pin<Box<dyn Stream<Item = Result<Bytes, HttpError>> + Send>>;

/// A streaming upload source.
pub struct UploadSource {
    stream: Pin<Box<dyn Stream<Item = Result<Bytes, HttpError>> + Send + 'static>>,
}

impl UploadSource {
    /// Construct a source containing one in-memory byte sequence.
    #[must_use]
    pub fn from_bytes(bytes: impl Into<Bytes>) -> Self {
        let bytes = bytes.into();
        Self {
            stream: Box::pin(stream::once(async move { Ok(bytes) })),
        }
    }

    /// Construct a source from a fallible asynchronous byte stream.
    pub fn from_stream<S, E>(source: S) -> Self
    where
        S: Stream<Item = Result<Bytes, E>> + Send + 'static,
        E: std::error::Error + Send + Sync + 'static,
    {
        Self {
            stream: Box::pin(source.map_err(|_| HttpError::Transport)),
        }
    }
}

impl fmt::Debug for UploadSource {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("UploadSource([STREAM])")
    }
}

/// A bounded streaming download.
pub struct Download {
    /// Response content type, if supplied.
    pub content_type: Option<String>,
    /// Declared content length, if supplied.
    pub content_length: Option<u64>,
    stream: Pin<Box<dyn Stream<Item = Result<Bytes, HttpError>> + Send + 'static>>,
}

impl Download {
    /// Consume the download and return its bounded byte stream.
    #[must_use]
    pub fn into_stream(
        self,
    ) -> Pin<Box<dyn Stream<Item = Result<Bytes, HttpError>> + Send + 'static>> {
        self.stream
    }
}

impl fmt::Debug for Download {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("Download")
            .field("content_type", &self.content_type)
            .field("content_length", &self.content_length)
            .field("stream", &"[STREAM]")
            .finish()
    }
}

/// Result of a conditional document-version query.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct QueryResponse {
    /// Whether the server returned `304 Not Modified`.
    pub not_modified: bool,
    /// Parsed result for a successful modified response.
    pub result: Option<DocumentQueryResult>,
    /// Response entity tag, if supplied.
    pub etag: Option<String>,
}

/// HTTP authorization material with redacted diagnostics.
#[derive(Clone, PartialEq, Eq, Zeroize, ZeroizeOnDrop)]
pub enum Credential {
    /// OAuth bearer token.
    Bearer(String),
    /// HTTP Basic username and password.
    Basic {
        /// Basic username.
        username: String,
        /// Basic password.
        password: String,
    },
}

impl Credential {
    /// Construct bearer credentials.
    #[must_use]
    pub fn bearer(token: impl Into<String>) -> Self {
        Self::Bearer(token.into())
    }

    /// Construct Basic credentials.
    #[must_use]
    pub fn basic(username: impl Into<String>, password: impl Into<String>) -> Self {
        Self::Basic {
            username: username.into(),
            password: password.into(),
        }
    }

    fn redact(&self, value: &mut Zeroizing<String>) {
        match self {
            Self::Bearer(token) => {
                redact_in_place(value, token);
                let authorization = Zeroizing::new(format!("Bearer {token}"));
                redact_in_place(value, &authorization);
            }
            Self::Basic { username, password } => {
                redact_in_place(value, username);
                redact_in_place(value, password);
                let cleartext = Zeroizing::new(format!("{username}:{password}"));
                let encoded = Zeroizing::new(STANDARD.encode(cleartext.as_bytes()));
                redact_in_place(value, &encoded);
                let authorization = Zeroizing::new(format!("Basic {}", encoded.as_str()));
                redact_in_place(value, &authorization);
            }
        }
    }
}

impl fmt::Debug for Credential {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Bearer(_) => formatter.write_str("Credential::Bearer([REDACTED])"),
            Self::Basic { .. } => formatter.write_str("Credential::Basic([REDACTED])"),
        }
    }
}

/// Secure-by-default builder for [`HttpClient`].
pub struct HttpClientBuilder {
    origin: Url,
    foundation_base: Url,
    documents_base: Url,
    credential: Option<Credential>,
    allow_http_local: bool,
    allow_cross_origin_authorization: bool,
    connect_timeout: Duration,
    request_timeout: Duration,
    max_json_body: usize,
    max_error_body: usize,
    max_download_body: usize,
    user_agent: String,
    proxy: Option<reqwest::Proxy>,
}

impl HttpClientBuilder {
    /// Start a client builder with the server origin and advertised API bases.
    ///
    /// The three URLs must be absolute. API bases may include path prefixes;
    /// query strings, fragments, and embedded URL credentials are rejected.
    pub fn new(
        origin: impl AsRef<str>,
        foundation_base: impl AsRef<str>,
        documents_base: impl AsRef<str>,
    ) -> Result<Self, crate::ConfigError> {
        Ok(Self {
            origin: parse_base(origin.as_ref())?,
            foundation_base: parse_base(foundation_base.as_ref())?,
            documents_base: parse_base(documents_base.as_ref())?,
            credential: None,
            allow_http_local: false,
            allow_cross_origin_authorization: false,
            connect_timeout: Duration::from_secs(10),
            request_timeout: Duration::from_secs(60),
            max_json_body: 2 * 1024 * 1024,
            max_error_body: 16 * 1024,
            max_download_body: 1024 * 1024 * 1024,
            user_agent: format!("openbim-cde-http/{}", env!("CARGO_PKG_VERSION")),
            proxy: None,
        })
    }

    /// Configure authorization credentials.
    #[must_use]
    pub fn credential(mut self, credential: Credential) -> Self {
        self.credential = Some(credential);
        self
    }

    /// Explicitly permit cleartext HTTP to loopback or `localhost` targets.
    ///
    /// Non-local cleartext origins remain prohibited even after this opt-in.
    #[must_use]
    pub fn allow_http(mut self) -> Self {
        self.allow_http_local = true;
        self
    }

    /// Explicitly permit cleartext HTTP for loopback development.
    ///
    /// This descriptive alias is equivalent to [`Self::allow_http`].
    #[must_use]
    pub fn allow_http_for_local_development(self) -> Self {
        self.allow_http()
    }

    /// Explicitly permit authorization on advertised URLs at another origin.
    #[must_use]
    pub fn allow_cross_origin_authorization(mut self) -> Self {
        self.allow_cross_origin_authorization = true;
        self
    }

    /// Set the TCP connect timeout.
    #[must_use]
    pub fn connect_timeout(mut self, timeout: Duration) -> Self {
        self.connect_timeout = timeout;
        self
    }

    /// Set the whole-request timeout.
    #[must_use]
    pub fn request_timeout(mut self, timeout: Duration) -> Self {
        self.request_timeout = timeout;
        self
    }

    /// Set the maximum buffered JSON request or success-response size.
    #[must_use]
    pub fn max_json_body(mut self, bytes: usize) -> Self {
        self.max_json_body = bytes;
        self
    }

    /// Set the maximum buffered HTTP error diagnostic size.
    #[must_use]
    pub fn max_error_body(mut self, bytes: usize) -> Self {
        self.max_error_body = bytes;
        self
    }

    /// Set the maximum streamed document download size.
    #[must_use]
    pub fn max_download_body(mut self, bytes: usize) -> Self {
        self.max_download_body = bytes;
        self
    }

    /// Set the request user agent.
    #[must_use]
    pub fn user_agent(mut self, value: impl Into<String>) -> Self {
        self.user_agent = value.into();
        self
    }

    /// Configure an explicit HTTP proxy.
    ///
    /// Ambient proxy environment variables are ignored. An explicitly configured
    /// proxy is part of the caller's trust boundary and can receive authorization
    /// headers and, for loopback-development HTTP, cleartext traffic.
    pub fn proxy(mut self, url: impl AsRef<str>) -> Result<Self, crate::ConfigError> {
        self.proxy = Some(
            reqwest::Proxy::all(url.as_ref())
                .map_err(|_| crate::ConfigError::new("invalid proxy URL"))?,
        );
        Ok(self)
    }

    /// Build the hardened client. Redirects are always disabled.
    pub fn build(self) -> Result<HttpClient, crate::ConfigError> {
        for url in [&self.origin, &self.foundation_base, &self.documents_base] {
            validate_transport(url, self.allow_http_local)
                .map_err(|error| crate::ConfigError::new(error.to_string()))?;
        }
        if self.max_json_body == 0 || self.max_error_body == 0 || self.max_download_body == 0 {
            return Err(crate::ConfigError::new("body limits must be non-zero"));
        }
        install_default_crypto_provider()?;
        let mut builder = reqwest::Client::builder()
            .redirect(reqwest::redirect::Policy::none())
            .connect_timeout(self.connect_timeout)
            .timeout(self.request_timeout)
            .user_agent(&self.user_agent);
        builder = if let Some(proxy) = self.proxy {
            builder.proxy(proxy)
        } else {
            // Ambient proxy variables are outside the caller's configured trust
            // boundary and could receive authorization, including credentials on
            // the explicit loopback-only cleartext development path.
            builder.no_proxy()
        };
        let inner = builder
            .build()
            .map_err(|_| crate::ConfigError::new("failed to build HTTP client"))?;
        Ok(HttpClient {
            inner,
            origin: self.origin,
            foundation_base: with_trailing_slash(self.foundation_base),
            documents_base: with_trailing_slash(self.documents_base),
            credential: self.credential,
            allow_http_local: self.allow_http_local,
            allow_cross_origin_authorization: self.allow_cross_origin_authorization,
            max_json_body: self.max_json_body,
            max_error_body: self.max_error_body,
            max_download_body: self.max_download_body,
        })
    }
}

impl fmt::Debug for HttpClientBuilder {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("HttpClientBuilder")
            .field("origin", &"[REDACTED URL]")
            .field("foundation_base", &"[REDACTED URL]")
            .field("documents_base", &"[REDACTED URL]")
            .field("credential", &self.credential)
            .field("allow_http_local", &self.allow_http_local)
            .field(
                "allow_cross_origin_authorization",
                &self.allow_cross_origin_authorization,
            )
            .field("connect_timeout", &self.connect_timeout)
            .field("request_timeout", &self.request_timeout)
            .field("max_json_body", &self.max_json_body)
            .field("max_error_body", &self.max_error_body)
            .field("max_download_body", &self.max_download_body)
            .finish_non_exhaustive()
    }
}

/// Hardened asynchronous OpenCDE HTTP client.
pub struct HttpClient {
    inner: reqwest::Client,
    origin: Url,
    foundation_base: Url,
    documents_base: Url,
    credential: Option<Credential>,
    allow_http_local: bool,
    allow_cross_origin_authorization: bool,
    max_json_body: usize,
    max_error_body: usize,
    max_download_body: usize,
}

impl fmt::Debug for HttpClient {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("HttpClient")
            .field("origin", &"[REDACTED URL]")
            .field("foundation_base", &"[REDACTED URL]")
            .field("documents_base", &"[REDACTED URL]")
            .field("credential", &self.credential)
            .finish_non_exhaustive()
    }
}

impl HttpClient {
    /// Discover APIs at the origin-fixed Foundation versions path.
    pub async fn versions(&self) -> Result<VersionsResponse, HttpError> {
        let mut url = self.origin.clone();
        url.set_path(VERSIONS_PATH);
        url.set_query(None);
        url.set_fragment(None);
        self.send_json(self.inner.get(url), StatusCode::OK).await
    }

    /// Fetch Foundation authentication metadata below the configured base.
    pub async fn authentication(&self) -> Result<AuthenticationDiscovery, HttpError> {
        let url = self.standard_url(&self.foundation_base, "auth")?;
        self.send_json(self.inner.get(url), StatusCode::OK).await
    }

    /// Dynamically register at the exact absolute URL advertised by Foundation.
    pub async fn register_client(
        &self,
        advertised_url: &str,
        request: &DynamicClientRegistrationRequest,
    ) -> Result<DynamicClientRegistrationResponse, HttpError> {
        let url = self.advertised_url(advertised_url)?;
        let builder = self.json_request(Method::POST, url, request, false)?;
        self.send_unescaped_json(builder, StatusCode::CREATED).await
    }

    /// Fetch the authenticated Foundation user.
    pub async fn current_user(&self) -> Result<User, HttpError> {
        let url = self.standard_url(&self.foundation_base, "current-user")?;
        let builder = self.authorize(self.inner.get(url), &self.foundation_base, true)?;
        self.send_json(builder, StatusCode::OK).await
    }

    /// Start the standardized Documents selection flow.
    pub async fn select_documents(
        &self,
        request: &SelectDocuments,
    ) -> Result<DocumentDiscoverySessionInitialization, HttpError> {
        let url = self.standard_url(&self.documents_base, SELECT_DOCUMENTS_PATH)?;
        let builder = self.json_request(Method::POST, url, request, true)?;
        self.send_json(builder, StatusCode::OK).await
    }

    /// Fetch selected documents from the exact server-advertised URL.
    pub async fn selected_documents(
        &self,
        advertised_url: &str,
    ) -> Result<SelectedDocuments, HttpError> {
        self.get_advertised_json(advertised_url, true).await
    }

    /// Start the standardized Documents upload flow.
    pub async fn upload_documents(
        &self,
        request: &UploadDocuments,
    ) -> Result<DocumentUploadSessionInitialization, HttpError> {
        let url = self.standard_url(&self.documents_base, UPLOAD_DOCUMENTS_PATH)?;
        let builder = self.json_request(Method::POST, url, request, true)?;
        self.send_json(builder, StatusCode::OK).await
    }

    /// Stream a document from the exact server-advertised URL.
    pub async fn download_document(&self, advertised_url: &str) -> Result<Download, HttpError> {
        let url = self.advertised_url(advertised_url)?;
        let builder = self.authorize(self.inner.get(url.clone()), &url, true)?;
        let response = builder.send().await.map_err(|_| HttpError::Transport)?;
        if response.status() != StatusCode::OK {
            return Err(self.response_error(response, None).await);
        }
        let content_length = response.content_length();
        if content_length.is_some_and(|length| length > self.max_download_body as u64) {
            return Err(HttpError::BodyTooLarge {
                limit: self.max_download_body,
            });
        }
        let content_type = response
            .headers()
            .get(CONTENT_TYPE)
            .and_then(|value| value.to_str().ok())
            .map(str::to_owned);
        let limit = self.max_download_body;
        let stream = response.bytes_stream().scan(0usize, move |seen, item| {
            let next = match item {
                Ok(bytes) => {
                    *seen = seen.saturating_add(bytes.len());
                    if *seen > limit {
                        Err(HttpError::BodyTooLarge { limit })
                    } else {
                        Ok(bytes)
                    }
                }
                Err(_) => Err(HttpError::Transport),
            };
            std::future::ready(Some(next))
        });
        Ok(Download {
            content_type,
            content_length,
            stream: Box::pin(stream),
        })
    }

    /// Fetch document metadata from the exact server-advertised URL.
    pub async fn document_metadata(
        &self,
        advertised_url: &str,
    ) -> Result<DocumentMetadata, HttpError> {
        self.get_advertised_json(advertised_url, true).await
    }

    /// Fetch all versions from the exact server-advertised URL.
    pub async fn document_versions(
        &self,
        advertised_url: &str,
    ) -> Result<DocumentVersions, HttpError> {
        self.get_advertised_json(advertised_url, true).await
    }

    /// Fetch one version from the exact server-advertised URL.
    pub async fn document_version(
        &self,
        advertised_url: &str,
    ) -> Result<DocumentVersion, HttpError> {
        self.get_advertised_json(advertised_url, true).await
    }

    /// Request upload-part instructions from the exact server-advertised URL.
    pub async fn upload_instructions(
        &self,
        advertised_url: &str,
        request: &UploadFileDetails,
    ) -> Result<DocumentsToUpload, HttpError> {
        let url = self.advertised_url(advertised_url)?;
        let builder = self.json_request(Method::POST, url, request, true)?;
        self.send_json(builder, StatusCode::OK).await
    }

    /// Execute a `PUT` upload instruction as a stream.
    pub async fn upload_file_part_put(
        &self,
        instruction: &UploadFilePartInstruction,
        source: UploadSource,
    ) -> Result<(), HttpError> {
        if instruction.http_method != HttpMethod::Put {
            return Err(HttpError::InvalidUrl(
                "instruction method is not PUT".into(),
            ));
        }
        self.upload_file_part(Method::PUT, instruction, source)
            .await
    }

    /// Execute a `POST` upload instruction as a stream.
    pub async fn upload_file_part_post(
        &self,
        instruction: &UploadFilePartInstruction,
        source: UploadSource,
    ) -> Result<(), HttpError> {
        if instruction.http_method != HttpMethod::Post {
            return Err(HttpError::InvalidUrl(
                "instruction method is not POST".into(),
            ));
        }
        self.upload_file_part(Method::POST, instruction, source)
            .await
    }

    /// Signal upload completion at the exact server-advertised URL.
    pub async fn upload_completion(
        &self,
        advertised_url: &str,
    ) -> Result<DocumentVersion, HttpError> {
        let url = self.advertised_url(advertised_url)?;
        let builder = self.authorize(self.inner.post(url.clone()), &url, true)?;
        self.send_json(builder, StatusCode::OK).await
    }

    /// Signal upload cancellation at the exact server-advertised URL.
    pub async fn upload_cancellation(&self, advertised_url: &str) -> Result<(), HttpError> {
        self.empty_advertised_post(advertised_url).await
    }

    /// Query standardized document versions, optionally with `If-None-Match`.
    pub async fn query_document_versions(
        &self,
        request: &DocumentQuery,
        etag: Option<&str>,
    ) -> Result<QueryResponse, HttpError> {
        let url = self.standard_url(&self.documents_base, "document-versions")?;
        let mut builder = self.json_request(Method::POST, url, request, true)?;
        if let Some(etag) = etag {
            let value = HeaderValue::from_str(etag)
                .map_err(|_| HttpError::InvalidHeader("if-none-match".into()))?;
            builder = builder.header(reqwest::header::IF_NONE_MATCH, value);
        }
        let response = builder.send().await.map_err(|_| HttpError::Transport)?;
        let response_etag = response
            .headers()
            .get(ETAG)
            .and_then(|value| value.to_str().ok())
            .map(str::to_owned);
        if response.status() == StatusCode::NOT_MODIFIED {
            return Ok(QueryResponse {
                not_modified: true,
                result: None,
                etag: response_etag,
            });
        }
        let result = self.decode_json_response(response, StatusCode::OK).await?;
        Ok(QueryResponse {
            not_modified: false,
            result: Some(result),
            etag: response_etag,
        })
    }

    async fn get_advertised_json<T: DeserializeOwned>(
        &self,
        advertised_url: &str,
        include_authorization: bool,
    ) -> Result<T, HttpError> {
        let url = self.advertised_url(advertised_url)?;
        let builder = self.authorize(self.inner.get(url.clone()), &url, include_authorization)?;
        self.send_json(builder, StatusCode::OK).await
    }

    async fn empty_advertised_post(&self, advertised_url: &str) -> Result<(), HttpError> {
        let url = self.advertised_url(advertised_url)?;
        let builder = self.authorize(self.inner.post(url.clone()), &url, true)?;
        let response = builder.send().await.map_err(|_| HttpError::Transport)?;
        self.expect_status(response, StatusCode::NO_CONTENT, None)
            .await
    }

    async fn upload_file_part(
        &self,
        method: Method,
        instruction: &UploadFilePartInstruction,
        source: UploadSource,
    ) -> Result<(), HttpError> {
        let url = self.advertised_url(&instruction.url)?;
        let mut builder = self.inner.request(method, url.clone());
        if let Some(headers) = instruction.additional_headers.as_ref() {
            builder = apply_headers(builder, headers)?;
        }
        builder = builder.header(CONTENT_TYPE, "application/octet-stream");
        builder = self.authorize(builder, &url, instruction.include_authorization)?;
        let mut pieces: Vec<UploadPiece> = Vec::new();
        if let Some(framing) = instruction.multipart_form_data.as_ref() {
            let prefix = STANDARD
                .decode(&framing.prefix)
                .map_err(|_| HttpError::InvalidMultipartFraming)?;
            pieces.push(Box::pin(stream::once(
                async move { Ok(Bytes::from(prefix)) },
            )));
        }
        pieces.push(source.stream);
        if let Some(framing) = instruction.multipart_form_data.as_ref() {
            let suffix = STANDARD
                .decode(&framing.suffix)
                .map_err(|_| HttpError::InvalidMultipartFraming)?;
            pieces.push(Box::pin(stream::once(
                async move { Ok(Bytes::from(suffix)) },
            )));
        }
        let body = reqwest::Body::wrap_stream(stream::iter(pieces).flatten());
        let response = builder
            .body(body)
            .send()
            .await
            .map_err(|_| HttpError::Transport)?;
        self.expect_status(
            response,
            StatusCode::OK,
            instruction.additional_headers.as_ref(),
        )
        .await
    }

    fn standard_url(&self, base: &Url, relative: &str) -> Result<Url, HttpError> {
        let relative = relative.trim_start_matches('/');
        let url = base
            .join(relative)
            .map_err(|_| HttpError::InvalidUrl("cannot join API path".into()))?;
        if !same_origin(base, &url) {
            return Err(HttpError::InvalidUrl("joined URL changed origin".into()));
        }
        self.check_transport(&url)?;
        Ok(url)
    }

    fn advertised_url(&self, value: &str) -> Result<Url, HttpError> {
        let url = Url::parse(value)
            .map_err(|_| HttpError::InvalidUrl("advertised URL must be absolute".into()))?;
        if !url.username().is_empty() || url.password().is_some() || url.fragment().is_some() {
            return Err(HttpError::InvalidUrl(
                "advertised URL contains credentials or fragment".into(),
            ));
        }
        self.check_transport(&url)?;
        Ok(url)
    }

    fn check_transport(&self, url: &Url) -> Result<(), HttpError> {
        validate_transport(url, self.allow_http_local)
    }

    fn json_request<T: Serialize + ?Sized>(
        &self,
        method: Method,
        url: Url,
        body: &T,
        include_authorization: bool,
    ) -> Result<RequestBuilder, HttpError> {
        let encoded = serde_json::to_vec(body).map_err(|_| HttpError::Json)?;
        if encoded.len() > self.max_json_body {
            return Err(HttpError::BodyTooLarge {
                limit: self.max_json_body,
            });
        }
        let builder = self
            .inner
            .request(method, url.clone())
            .header(CONTENT_TYPE, "application/json")
            .body(encoded);
        self.authorize(builder, &url, include_authorization)
    }

    fn authorize(
        &self,
        mut builder: RequestBuilder,
        target: &Url,
        include: bool,
    ) -> Result<RequestBuilder, HttpError> {
        if !include {
            return Ok(builder);
        }
        let Some(credential) = &self.credential else {
            return Ok(builder);
        };
        self.check_transport(target)?;
        if !same_origin(&self.origin, target) && !self.allow_cross_origin_authorization {
            return Err(HttpError::CrossOriginAuthorization);
        }
        builder = match credential {
            Credential::Bearer(token) => builder.bearer_auth(token),
            Credential::Basic { username, password } => {
                builder.basic_auth(username, Some(password))
            }
        };
        Ok(builder)
    }

    async fn send_json<T: DeserializeOwned>(
        &self,
        builder: RequestBuilder,
        expected_status: StatusCode,
    ) -> Result<T, HttpError> {
        let response = builder.send().await.map_err(|_| HttpError::Transport)?;
        self.decode_json_response(response, expected_status).await
    }

    async fn send_unescaped_json<T: DeserializeOwned>(
        &self,
        builder: RequestBuilder,
        expected_status: StatusCode,
    ) -> Result<T, HttpError> {
        let response = builder.send().await.map_err(|_| HttpError::Transport)?;
        if response.status() != expected_status {
            return Err(self.response_error(response, None).await);
        }
        validate_json_content_type(response.headers())?;
        let bytes = collect_bounded(response, self.max_json_body).await?;
        // serde_json decodes escapes into a reusable ordinary Vec before invoking
        // a visitor. Registration responses can contain a client secret, so only
        // accept values borrowed directly from this zeroizing response buffer.
        if bytes.contains(&b'\\') {
            return Err(HttpError::Json);
        }
        serde_json::from_slice(&bytes).map_err(|_| HttpError::Json)
    }

    async fn decode_json_response<T: DeserializeOwned>(
        &self,
        response: Response,
        expected_status: StatusCode,
    ) -> Result<T, HttpError> {
        if response.status() != expected_status {
            return Err(self.response_error(response, None).await);
        }
        validate_json_content_type(response.headers())?;
        let bytes = collect_bounded(response, self.max_json_body).await?;
        serde_json::from_slice(&bytes).map_err(|_| HttpError::Json)
    }

    async fn expect_status(
        &self,
        response: Response,
        expected_status: StatusCode,
        extra_headers: Option<&Headers>,
    ) -> Result<(), HttpError> {
        if response.status() == expected_status {
            Ok(())
        } else {
            Err(self.response_error(response, extra_headers).await)
        }
    }

    async fn response_error(
        &self,
        response: Response,
        extra_headers: Option<&Headers>,
    ) -> HttpError {
        let status = response.status().as_u16();
        let url_secrets = response_url_secrets(&response);
        let bytes = collect_truncated(response, self.max_error_body).await;
        let mut diagnostic = Zeroizing::new(String::from_utf8_lossy(&bytes).into_owned());
        self.redact_response(&mut diagnostic, &url_secrets, extra_headers);
        HttpError::Response {
            status,
            diagnostic: Diagnostic::new(diagnostic),
        }
    }

    fn redact_response(
        &self,
        value: &mut Zeroizing<String>,
        url_secrets: &[Zeroizing<String>],
        extra_headers: Option<&Headers>,
    ) {
        if let Some(credential) = &self.credential {
            credential.redact(value);
        }
        for secret in url_secrets {
            redact_in_place(value, secret);
        }
        if let Some(headers) = extra_headers {
            for header in &headers.values {
                redact_in_place(value, &header.value);
            }
        }
    }
}

fn parse_base(value: &str) -> Result<Url, crate::ConfigError> {
    let url = Url::parse(value).map_err(|_| crate::ConfigError::new("URL must be absolute"))?;
    if !matches!(url.scheme(), "https" | "http") {
        return Err(crate::ConfigError::new("URL scheme must be HTTP or HTTPS"));
    }
    if url.host().is_none()
        || !url.username().is_empty()
        || url.password().is_some()
        || url.query().is_some()
        || url.fragment().is_some()
    {
        return Err(crate::ConfigError::new(
            "base URL must not contain credentials, query, or fragment",
        ));
    }
    Ok(url)
}

fn with_trailing_slash(mut url: Url) -> Url {
    if !url.path().ends_with('/') {
        let path = format!("{}/", url.path());
        url.set_path(&path);
    }
    url
}

fn install_default_crypto_provider() -> Result<(), crate::ConfigError> {
    if rustls::crypto::CryptoProvider::get_default().is_none() {
        let _ = rustls::crypto::ring::default_provider().install_default();
    }
    if rustls::crypto::CryptoProvider::get_default().is_none() {
        Err(crate::ConfigError::new(
            "failed to install TLS crypto provider",
        ))
    } else {
        Ok(())
    }
}

fn validate_transport(url: &Url, allow_http_local: bool) -> Result<(), HttpError> {
    if url.scheme() == "https" {
        return Ok(());
    }
    if url.scheme() != "http" {
        return Err(HttpError::InsecureTransport);
    }
    let local = matches!(
        url.host_str(),
        Some("localhost" | "127.0.0.1" | "[::1]" | "::1")
    );
    if allow_http_local && local {
        Ok(())
    } else {
        Err(HttpError::InsecureTransport)
    }
}

fn same_origin(left: &Url, right: &Url) -> bool {
    left.scheme() == right.scheme()
        && left.host_str() == right.host_str()
        && left.port_or_known_default() == right.port_or_known_default()
}

fn validate_json_content_type(headers: &HeaderMap) -> Result<(), HttpError> {
    let value = headers
        .get(CONTENT_TYPE)
        .and_then(|value| value.to_str().ok())
        .ok_or(HttpError::InvalidContentType)?;
    let media_type = value.split(';').next().unwrap_or_default().trim();
    if media_type.eq_ignore_ascii_case("application/json")
        || media_type
            .get(media_type.len().saturating_sub(5)..)
            .is_some_and(|suffix| suffix.eq_ignore_ascii_case("+json"))
    {
        Ok(())
    } else {
        Err(HttpError::InvalidContentType)
    }
}

async fn collect_bounded(
    response: Response,
    limit: usize,
) -> Result<Zeroizing<Vec<u8>>, HttpError> {
    if response
        .content_length()
        .is_some_and(|length| length > limit as u64)
    {
        return Err(HttpError::BodyTooLarge { limit });
    }
    let mut output = Zeroizing::new(Vec::new());
    let mut stream = response.bytes_stream();
    while let Some(chunk) = stream.next().await {
        let chunk = chunk.map_err(|_| HttpError::Transport)?;
        if output.len().saturating_add(chunk.len()) > limit {
            return Err(HttpError::BodyTooLarge { limit });
        }
        output.extend_from_slice(&chunk);
    }
    Ok(output)
}

async fn collect_truncated(response: Response, limit: usize) -> Zeroizing<Vec<u8>> {
    let mut output = Zeroizing::new(Vec::new());
    let mut stream = response.bytes_stream();
    while output.len() < limit {
        let Some(chunk) = stream.next().await else {
            break;
        };
        let Ok(chunk) = chunk else {
            break;
        };
        let remaining = limit - output.len();
        output.extend_from_slice(&chunk[..chunk.len().min(remaining)]);
    }
    output
}

fn response_url_secrets(response: &Response) -> Vec<Zeroizing<String>> {
    let url = response.url();
    let mut secrets = vec![Zeroizing::new(url.as_str().to_owned())];
    if let Some(query) = url.query() {
        secrets.push(Zeroizing::new(query.to_owned()));
        for component in query.split('&') {
            if let Some((_, value)) = component.split_once('=') {
                secrets.push(Zeroizing::new(value.to_owned()));
            }
        }
        for (_, value) in url.query_pairs() {
            secrets.push(Zeroizing::new(value.into_owned()));
        }
    }
    secrets
}

fn redact_in_place(value: &mut Zeroizing<String>, secret: &str) {
    const REDACTED: &str = "[REDACTED]";
    if secret.is_empty() || secret == REDACTED || !value.contains(secret) {
        return;
    }
    *value = Zeroizing::new(value.replace(secret, REDACTED));
}

fn apply_headers(
    mut builder: RequestBuilder,
    headers: &Headers,
) -> Result<RequestBuilder, HttpError> {
    for header in &headers.values {
        let name = HeaderName::from_bytes(header.name.as_bytes())
            .map_err(|_| HttpError::InvalidHeader("invalid name".into()))?;
        if matches!(
            name,
            AUTHORIZATION | PROXY_AUTHORIZATION | HOST | CONTENT_LENGTH | TRANSFER_ENCODING
        ) || name.as_str().eq_ignore_ascii_case("connection")
            || name.as_str().eq_ignore_ascii_case("cookie")
            || name.as_str().eq_ignore_ascii_case("set-cookie")
        {
            return Err(HttpError::InvalidHeader(name.as_str().to_owned()));
        }
        let value = HeaderValue::from_str(&header.value)
            .map_err(|_| HttpError::InvalidHeader(name.as_str().to_owned()))?;
        builder = builder.header(name, value);
    }
    Ok(builder)
}

use crate::{
    ClientSecret, OAuthConfig, RefreshToken, ResourceOwnerPassword, StoreError, StoredToken,
    TokenKey, TokenSet, TokenStore,
};
use async_trait::async_trait;
use base64::{
    engine::general_purpose::{STANDARD, URL_SAFE_NO_PAD},
    Engine as _,
};
use openbim_cde::foundation::{DynamicClientRegistrationRequest, OAuth2Flow};
use rand::{rngs::OsRng, RngCore};
use secrecy::{ExposeSecret, SecretString};
use serde::de::{self, IgnoredAny, MapAccess, Visitor};
use serde::{Deserialize, Deserializer};
use sha2::{Digest, Sha256};
use std::{
    collections::HashSet,
    error::Error,
    fmt,
    sync::{Arc, Mutex},
    time::SystemTime,
};
use subtle::ConstantTimeEq;
use url::{form_urlencoded, Url};
use zeroize::{Zeroize, Zeroizing};

const MAX_OAUTH_RESPONSE_BYTES: usize = 1024 * 1024;
const MAX_OAUTH_REQUEST_BYTES: usize = 1024 * 1024;
const MAX_ENDPOINT_BYTES: usize = 8 * 1024;
const MAX_AUTHORIZATION_URL_BYTES: usize = 16 * 1024;
const MAX_SCOPE_BYTES: usize = 8 * 1024;
const MAX_SCOPE_COUNT: usize = 128;
const MAX_CALLBACK_BYTES: usize = 16 * 1024;
const MAX_CALLBACK_VALUE_BYTES: usize = 4 * 1024;

/// An OAuth HTTP body whose allocation is erased when it leaves the transport boundary.
pub struct SensitiveBytes(Zeroizing<Vec<u8>>);

impl SensitiveBytes {
    /// Takes ownership of bytes that may contain OAuth credentials.
    #[must_use]
    pub fn new(bytes: Vec<u8>) -> Self {
        Self(Zeroizing::new(bytes))
    }

    /// Borrows the bytes for immediate transport or parsing.
    #[must_use]
    pub fn as_bytes(&self) -> &[u8] {
        self.0.as_slice()
    }

    /// Returns the body length without exposing its contents.
    #[must_use]
    pub fn len(&self) -> usize {
        self.0.len()
    }

    /// Returns whether the body is empty.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }
}

impl Zeroize for SensitiveBytes {
    fn zeroize(&mut self) {
        self.0.zeroize();
    }
}

impl zeroize::ZeroizeOnDrop for SensitiveBytes {}

impl AsRef<[u8]> for SensitiveBytes {
    fn as_ref(&self) -> &[u8] {
        self.as_bytes()
    }
}

impl fmt::Debug for SensitiveBytes {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("SensitiveBytes([REDACTED])")
    }
}

/// Credential-bearing HTTP request passed to an injected transport.
pub struct OAuthHttpRequest(http::Request<SensitiveBytes>);

impl OAuthHttpRequest {
    /// Wraps an HTTP request while preventing accidental diagnostic exposure.
    #[must_use]
    pub fn from_http(request: http::Request<SensitiveBytes>) -> Self {
        Self(request)
    }

    /// Returns the request method for transport dispatch.
    #[must_use]
    pub fn method(&self) -> &http::Method {
        self.0.method()
    }

    /// Returns the endpoint for transport dispatch. Callers must not log it.
    #[must_use]
    pub fn uri(&self) -> &http::Uri {
        self.0.uri()
    }

    /// Returns headers for transport dispatch. Callers must not log them.
    #[must_use]
    pub fn headers(&self) -> &http::HeaderMap {
        self.0.headers()
    }

    /// Returns the redacting, zeroizing request body.
    #[must_use]
    pub fn body(&self) -> &SensitiveBytes {
        self.0.body()
    }

    /// Decomposes the request at the transport boundary.
    #[must_use]
    pub fn into_parts(self) -> (http::request::Parts, SensitiveBytes) {
        self.0.into_parts()
    }
}

impl fmt::Debug for OAuthHttpRequest {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("OAuthHttpRequest")
            .field("method", self.method())
            .field("uri", &"[REDACTED]")
            .field("headers", &"[REDACTED]")
            .field("body", &"[REDACTED]")
            .finish()
    }
}

/// Credential-bearing HTTP response returned by an injected transport.
pub struct OAuthHttpResponse(http::Response<SensitiveBytes>);

impl OAuthHttpResponse {
    /// Wraps an HTTP response while preventing accidental diagnostic exposure.
    #[must_use]
    pub fn from_http(response: http::Response<SensitiveBytes>) -> Self {
        Self(response)
    }

    /// Returns the response status.
    #[must_use]
    pub fn status(&self) -> http::StatusCode {
        self.0.status()
    }

    /// Returns headers for protocol parsing. Callers must not log them.
    #[must_use]
    pub fn headers(&self) -> &http::HeaderMap {
        self.0.headers()
    }

    /// Returns the redacting, zeroizing response body.
    #[must_use]
    pub fn body(&self) -> &SensitiveBytes {
        self.0.body()
    }
}

impl fmt::Debug for OAuthHttpResponse {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("OAuthHttpResponse")
            .field("status", &self.status())
            .field("headers", &"[REDACTED]")
            .field("body", &"[REDACTED]")
            .finish()
    }
}

/// Sanitized transport failure. Request data and response bodies are never retained.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TransportError;

impl TransportError {
    /// Creates a categorical transport error. The supplied diagnostic is deliberately discarded.
    #[must_use]
    pub fn new(_message: impl AsRef<str>) -> Self {
        Self
    }
}

impl fmt::Display for TransportError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("OAuth HTTP operation failed")
    }
}

impl Error for TransportError {}

/// Injected async HTTP boundary used for all credential-bearing requests.
// async-trait marks the boxed future `#[must_use]`; clippy flags its own expansion.
#[allow(clippy::double_must_use)]
#[async_trait]
pub trait OAuthHttpTransport: Send + Sync {
    /// Executes one OAuth HTTP request without following redirects.
    ///
    /// Implementations must not log requests and must bound request URI, headers, body, and
    /// responses before copying caller-controlled values. Both bodies erase owned allocations on
    /// drop.
    async fn execute(&self, request: OAuthHttpRequest)
        -> Result<OAuthHttpResponse, TransportError>;
}

/// OAuth client credentials. The client secret is redacted and zeroized on drop.
#[derive(Clone)]
pub struct ClientCredentials {
    client_id: String,
    client_secret: Option<ClientSecret>,
}

impl ClientCredentials {
    /// Creates credentials for a public or confidential client.
    #[must_use]
    pub fn new(client_id: impl Into<String>, client_secret: Option<ClientSecret>) -> Self {
        Self {
            client_id: client_id.into(),
            client_secret,
        }
    }

    /// Returns the non-secret client identifier.
    #[must_use]
    pub fn client_id(&self) -> &str {
        &self.client_id
    }
}

impl fmt::Debug for ClientCredentials {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("ClientCredentials")
            .field("client_id", &self.client_id)
            .field(
                "client_secret",
                &self.client_secret.as_ref().map(|_| "[REDACTED]"),
            )
            .finish()
    }
}

/// Explicit compatibility policy for the deprecated password grant.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LegacyPasswordPolicy {
    /// Reject the password grant (default-safe behavior).
    Disabled,
    /// Permit it, but only if discovery advertised it.
    Enabled,
}

/// One pending authorization request. State and PKCE verifier are secret.
pub struct PendingAuthorization {
    authorization_url: Url,
    state: SecretString,
    pkce_verifier: SecretString,
}

impl PendingAuthorization {
    /// URL to open in the resource owner's user agent.
    #[must_use]
    pub fn authorization_url(&self) -> &Url {
        &self.authorization_url
    }
}

impl fmt::Debug for PendingAuthorization {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("PendingAuthorization")
            .field("authorization_url", &"[REDACTED]")
            .field("state", &"[REDACTED]")
            .field("pkce_verifier", &"[REDACTED]")
            .finish()
    }
}

/// OAuth execution failures. Secrets and raw endpoint data are never included.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum OAuthError {
    /// Configuration or redirect URL was invalid.
    InvalidConfiguration(&'static str),
    /// The callback URL did not exactly bind to the configured redirect URI.
    RedirectMismatch,
    /// The callback parameters were missing, duplicated, mixed, or otherwise malformed.
    InvalidCallback,
    /// The callback state did not match the generated state.
    StateMismatch,
    /// A requested flow was not advertised by discovery.
    FlowNotAdvertised(OAuth2Flow),
    /// Resource Owner Password Credentials was not explicitly enabled.
    LegacyPasswordGrantDisabled,
    /// The transport failed.
    Transport(TransportError),
    /// Authorization server rejected the request. Server-controlled text is discarded.
    EndpointRejected,
    /// Authorization server returned an invalid or unsupported response.
    InvalidTokenResponse,
    /// Dynamic registration returned an invalid response.
    InvalidRegistrationResponse,
    /// A response token type other than Bearer was returned.
    UnsupportedTokenType,
    /// Foundation one-time refresh did not return a distinct replacement token.
    MissingRotatedRefreshToken,
    /// No token exists for the requested store key.
    MissingStoredToken,
    /// Another refresh for this token key is already using its one-time token.
    RefreshInProgress,
    /// The stored token has no refresh token.
    MissingRefreshToken,
    /// Token persistence failed, including CAS conflicts.
    Store(StoreError),
}

impl fmt::Display for OAuthError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidConfiguration(reason) => formatter.write_str(reason),
            Self::RedirectMismatch => formatter.write_str("OAuth callback redirect mismatch"),
            Self::InvalidCallback => formatter.write_str("OAuth callback is invalid"),
            Self::StateMismatch => formatter.write_str("OAuth callback state mismatch"),
            Self::FlowNotAdvertised(flow) => {
                write!(formatter, "OAuth flow {flow:?} was not advertised")
            }
            Self::LegacyPasswordGrantDisabled => {
                formatter.write_str("legacy password grant is disabled")
            }
            Self::Transport(error) => write!(formatter, "OAuth transport failed: {error}"),
            Self::EndpointRejected => formatter.write_str("authorization server rejected request"),
            Self::InvalidTokenResponse => {
                formatter.write_str("authorization server returned an invalid token response")
            }
            Self::InvalidRegistrationResponse => {
                formatter.write_str("dynamic registration returned an invalid response")
            }
            Self::UnsupportedTokenType => {
                formatter.write_str("authorization server returned a non-Bearer token")
            }
            Self::MissingRotatedRefreshToken => {
                formatter.write_str("one-time refresh token was not distinctly rotated")
            }
            Self::MissingStoredToken => formatter.write_str("no stored token exists"),
            Self::RefreshInProgress => {
                formatter.write_str("a refresh is already in progress for this token key")
            }
            Self::MissingRefreshToken => formatter.write_str("stored token has no refresh token"),
            Self::Store(error) => write!(formatter, "token store failed: {error}"),
        }
    }
}

impl Error for OAuthError {}

impl From<StoreError> for OAuthError {
    fn from(value: StoreError) -> Self {
        Self::Store(value)
    }
}

/// Secure executor for discovered OpenCDE OAuth endpoints.
pub struct OAuthExecutor<T> {
    config: OAuthConfig,
    credentials: ClientCredentials,
    redirect_url: Url,
    transport: T,
    refreshing: Arc<Mutex<HashSet<TokenKey>>>,
}

impl<T: Clone> Clone for OAuthExecutor<T> {
    fn clone(&self) -> Self {
        Self {
            config: self.config.clone(),
            credentials: self.credentials.clone(),
            redirect_url: self.redirect_url.clone(),
            transport: self.transport.clone(),
            refreshing: Arc::clone(&self.refreshing),
        }
    }
}

impl<T> OAuthExecutor<T>
where
    T: OAuthHttpTransport,
{
    /// Creates an executor after validating the redirect URL.
    pub fn new(
        config: OAuthConfig,
        credentials: ClientCredentials,
        redirect_url: &str,
        transport: T,
    ) -> Result<Self, OAuthError> {
        if credentials.client_id.trim().is_empty()
            || credentials.client_id.len() > MAX_ENDPOINT_BYTES
            || credentials.client_id.chars().any(char::is_control)
            || credentials.client_secret.as_ref().is_some_and(|secret| {
                secret.expose().is_empty() || secret.expose().len() > MAX_ENDPOINT_BYTES
            })
        {
            return Err(OAuthError::InvalidConfiguration(
                "OAuth client credentials must not be empty",
            ));
        }
        let redirect_url = validate_redirect_url(redirect_url)?;
        Ok(Self {
            config,
            credentials,
            redirect_url,
            transport,
            refreshing: Arc::new(Mutex::new(HashSet::new())),
        })
    }

    /// Starts Authorization Code Grant with mandatory S256 PKCE and random CSRF state.
    pub fn begin_authorization<'a, I>(&self, scopes: I) -> Result<PendingAuthorization, OAuthError>
    where
        I: IntoIterator<Item = &'a str>,
    {
        self.require_flow(OAuth2Flow::AuthorizationCode)?;
        let mut url = self.config.authorization_endpoint().cloned().ok_or(
            OAuthError::InvalidConfiguration("authorization endpoint is missing"),
        )?;
        let validated_scopes = collect_scopes_bounded(scopes)?;
        let scope = validated_scopes.join(" ");

        // Validate all caller input before generating secrets. Once generated,
        // secret values immediately have a zeroizing owner across every fallible path.
        let state = SecretString::from(random_urlsafe_secret());
        let verifier = SecretString::from(random_urlsafe_secret());
        let challenge = URL_SAFE_NO_PAD.encode(Sha256::digest(verifier.expose_secret().as_bytes()));
        {
            let mut query = url.query_pairs_mut();
            query
                .append_pair("response_type", "code")
                .append_pair("client_id", &self.credentials.client_id)
                .append_pair("redirect_uri", self.redirect_url.as_str())
                .append_pair("state", state.expose_secret())
                .append_pair("code_challenge", &challenge)
                .append_pair("code_challenge_method", "S256");
            if !scope.is_empty() {
                query.append_pair("scope", &scope);
            }
        }
        if url.as_str().len() > MAX_AUTHORIZATION_URL_BYTES {
            return Err(OAuthError::InvalidConfiguration(
                "OAuth authorization URL exceeded size limit",
            ));
        }
        Ok(PendingAuthorization {
            authorization_url: url,
            state,
            pkce_verifier: verifier,
        })
    }

    /// Strictly validates the complete callback URL, checks state in constant time, then exchanges
    /// the single authorization code with the pending PKCE verifier.
    pub async fn exchange_authorization_code(
        &self,
        pending: PendingAuthorization,
        callback_url: &str,
        now: SystemTime,
    ) -> Result<TokenSet, OAuthError> {
        if callback_url.len() > MAX_CALLBACK_BYTES || callback_url.contains('#') {
            return Err(OAuthError::InvalidCallback);
        }
        let (callback_base_text, query_text) = callback_url
            .split_once('?')
            .ok_or(OAuthError::InvalidCallback)?;
        if query_text.is_empty() || query_text.contains('?') {
            return Err(OAuthError::InvalidCallback);
        }
        let callback_base =
            Url::parse(callback_base_text).map_err(|_| OAuthError::InvalidCallback)?;
        if !callback_base.username().is_empty() || callback_base.password().is_some() {
            return Err(OAuthError::InvalidCallback);
        }
        if callback_base != self.redirect_url {
            return Err(OAuthError::RedirectMismatch);
        }

        if !strict_form_encoding(query_text) {
            return Err(OAuthError::InvalidCallback);
        }
        let mut code: Option<Zeroizing<String>> = None;
        let mut state: Option<Zeroizing<String>> = None;
        let mut endpoint_error = false;
        for (name, value) in form_urlencoded::parse(query_text.as_bytes()) {
            match name.as_ref() {
                "code"
                    if code.is_none()
                        && !value.is_empty()
                        && value.len() <= MAX_CALLBACK_VALUE_BYTES =>
                {
                    code = Some(Zeroizing::new(value.into_owned()));
                }
                "state"
                    if state.is_none()
                        && !value.is_empty()
                        && value.len() <= MAX_CALLBACK_VALUE_BYTES =>
                {
                    state = Some(Zeroizing::new(value.into_owned()));
                }
                "error"
                    if !endpoint_error
                        && !value.is_empty()
                        && value.len() <= MAX_CALLBACK_VALUE_BYTES =>
                {
                    endpoint_error = true;
                }
                _ => return Err(OAuthError::InvalidCallback),
            }
        }
        let state = state.ok_or(OAuthError::InvalidCallback)?;
        if !constant_time_state_eq(pending.state.expose_secret(), state.as_str()) {
            return Err(OAuthError::StateMismatch);
        }
        if endpoint_error {
            if code.is_some() {
                return Err(OAuthError::InvalidCallback);
            }
            return Err(OAuthError::EndpointRejected);
        }
        let code = code.ok_or(OAuthError::InvalidCallback)?;
        let pairs = [
            ("grant_type", "authorization_code"),
            ("code", code.as_str()),
            ("redirect_uri", self.redirect_url.as_str()),
            ("code_verifier", pending.pkce_verifier.expose_secret()),
        ];
        let response = self.execute_token_form(&pairs, None).await?;
        parse_token_response(response, now, false, None)
    }

    /// Refreshes a stored token and atomically rotates it if its generation is unchanged.
    pub async fn refresh_stored<S>(
        &self,
        store: &S,
        key: &TokenKey,
        now: SystemTime,
    ) -> Result<StoredToken, OAuthError>
    where
        S: TokenStore,
    {
        self.require_flow(OAuth2Flow::AuthorizationCode)?;
        let _refresh_guard = self.acquire_refresh(key)?;
        let stored = store
            .load(key)
            .await?
            .ok_or(OAuthError::MissingStoredToken)?;
        let refresh = stored
            .token()
            .refresh_token()
            .ok_or(OAuthError::MissingRefreshToken)?;
        let pairs = [
            ("grant_type", "refresh_token"),
            ("refresh_token", refresh.expose()),
        ];
        let response = self.execute_token_form(&pairs, None).await?;
        let replacement = parse_token_response(response, now, true, Some(refresh.expose()))?;
        store
            .compare_and_swap(key, Some(stored.generation()), replacement)
            .await
            .map_err(OAuthError::Store)
    }

    /// Executes ROPC only with explicit policy and a matching discovery advertisement.
    pub async fn exchange_password<'a, I>(
        &self,
        username: &str,
        password: &ResourceOwnerPassword,
        policy: LegacyPasswordPolicy,
        scopes: I,
        now: SystemTime,
    ) -> Result<TokenSet, OAuthError>
    where
        I: IntoIterator<Item = &'a str>,
    {
        if policy != LegacyPasswordPolicy::Enabled {
            return Err(OAuthError::LegacyPasswordGrantDisabled);
        }
        self.require_flow(OAuth2Flow::ResourceOwnerPasswordCredentials)?;
        if username.is_empty() {
            return Err(OAuthError::InvalidConfiguration(
                "resource-owner username must not be empty",
            ));
        }
        let scopes = collect_scopes_bounded(scopes)?;
        let scope = scopes.join(" ");
        let mut pairs = vec![
            ("grant_type", "password"),
            ("username", username),
            ("password", password.expose()),
        ];
        if !scope.is_empty() {
            pairs.push(("scope", scope.as_str()));
        }
        let response = self.execute_token_form(&pairs, None).await?;
        parse_token_response(response, now, false, None)
    }

    /// Registers a client using the Foundation dynamic-registration contract.
    pub async fn register_client(
        &self,
        metadata: &DynamicClientRegistrationRequest,
    ) -> Result<ClientCredentials, OAuthError> {
        validate_redirect_url(&metadata.redirect_url)?;
        let registration_redirect = Url::parse(&metadata.redirect_url)
            .map_err(|_| OAuthError::InvalidConfiguration("invalid OAuth redirect URL"))?;
        if registration_redirect != self.redirect_url {
            return Err(OAuthError::InvalidConfiguration(
                "registration redirect URL does not match the executor redirect URL",
            ));
        }
        let endpoint =
            self.config
                .registration_endpoint()
                .ok_or(OAuthError::InvalidConfiguration(
                    "dynamic registration endpoint is missing",
                ))?;
        ensure_registration_metadata_bounded(metadata)?;
        let body = serde_json::to_vec(metadata)
            .map(SensitiveBytes::new)
            .map_err(|_| OAuthError::InvalidConfiguration("invalid registration metadata"))?;
        if body.len() > MAX_OAUTH_REQUEST_BYTES {
            return Err(OAuthError::InvalidConfiguration(
                "registration request exceeded size limit",
            ));
        }
        let request = OAuthHttpRequest::from_http(
            http::Request::builder()
                .method(http::Method::POST)
                .uri(endpoint.as_str())
                .header(http::header::ACCEPT, "application/json")
                .header(http::header::CONTENT_TYPE, "application/json")
                .body(body)
                .map_err(|_| OAuthError::InvalidConfiguration("invalid registration request"))?,
        );
        let response = self
            .transport
            .execute(request)
            .await
            .map_err(OAuthError::Transport)?;
        if response.status() != http::StatusCode::CREATED
            || response.body().len() > MAX_OAUTH_RESPONSE_BYTES
            || !has_json_content_type(response.headers())
            || response.body().as_bytes().contains(&b'\\')
        {
            return Err(OAuthError::InvalidRegistrationResponse);
        }
        let mut deserializer = serde_json::Deserializer::from_slice(response.body().as_bytes());
        let wire = RegistrationResponse::deserialize(&mut deserializer)
            .map_err(|_| OAuthError::InvalidRegistrationResponse)?;
        deserializer
            .end()
            .map_err(|_| OAuthError::InvalidRegistrationResponse)?;
        if wire.client_id.is_empty() || wire.client_secret.0.is_empty() {
            return Err(OAuthError::InvalidRegistrationResponse);
        }
        Ok(ClientCredentials::new(
            wire.client_id,
            Some(ClientSecret::new(wire.client_secret.into_string())),
        ))
    }

    async fn execute_token_form(
        &self,
        grant_pairs: &[(&str, &str)],
        scope: Option<&str>,
    ) -> Result<OAuthHttpResponse, OAuthError> {
        let endpoint = self
            .config
            .token_endpoint()
            .ok_or(OAuthError::InvalidConfiguration(
                "token endpoint is missing",
            ))?;
        let use_basic =
            self.credentials.client_secret.is_some() && self.config.http_basic_supported();
        let mut form_pairs = grant_pairs.to_vec();
        if !use_basic {
            form_pairs.push(("client_id", self.credentials.client_id.as_str()));
            if let Some(secret) = &self.credentials.client_secret {
                form_pairs.push(("client_secret", secret.expose()));
            }
        }
        if let Some(scope) = scope {
            form_pairs.push(("scope", scope));
        }
        ensure_form_pairs_bounded(&form_pairs)?;

        let basic_authorization = match (
            self.credentials.client_secret.as_ref(),
            self.config.http_basic_supported(),
        ) {
            (Some(secret), true) => Some(basic_authorization(
                &self.credentials.client_id,
                secret.expose(),
            )?),
            _ => None,
        };
        let body = {
            let mut serializer = form_urlencoded::Serializer::new(String::new());
            serializer.extend_pairs(form_pairs);
            SensitiveBytes::new(serializer.finish().into_bytes())
        };
        if body.len() > MAX_OAUTH_REQUEST_BYTES {
            return Err(OAuthError::InvalidConfiguration(
                "OAuth token request exceeded size limit",
            ));
        }
        let mut request = http::Request::builder()
            .method(http::Method::POST)
            .uri(endpoint.as_str())
            .header(http::header::ACCEPT, "application/json")
            .header(
                http::header::CONTENT_TYPE,
                "application/x-www-form-urlencoded",
            );
        if let Some(authorization) = basic_authorization {
            request = request.header(http::header::AUTHORIZATION, authorization);
        }
        let request = OAuthHttpRequest::from_http(
            request
                .body(body)
                .map_err(|_| OAuthError::InvalidConfiguration("invalid token request"))?,
        );
        let response = self
            .transport
            .execute(request)
            .await
            .map_err(OAuthError::Transport)?;
        if response.body().len() > MAX_OAUTH_RESPONSE_BYTES {
            return Err(OAuthError::InvalidTokenResponse);
        }
        Ok(response)
    }

    fn acquire_refresh(&self, key: &TokenKey) -> Result<RefreshGuard, OAuthError> {
        let mut refreshing = self
            .refreshing
            .lock()
            .unwrap_or_else(|error| error.into_inner());
        if !refreshing.insert(key.clone()) {
            return Err(OAuthError::RefreshInProgress);
        }
        drop(refreshing);
        Ok(RefreshGuard {
            key: key.clone(),
            refreshing: Arc::clone(&self.refreshing),
        })
    }

    fn require_flow(&self, flow: OAuth2Flow) -> Result<(), OAuthError> {
        if self.config.flows().contains(&flow) {
            Ok(())
        } else {
            Err(OAuthError::FlowNotAdvertised(flow))
        }
    }
}

struct RefreshGuard {
    key: TokenKey,
    refreshing: Arc<Mutex<HashSet<TokenKey>>>,
}

impl Drop for RefreshGuard {
    fn drop(&mut self) {
        self.refreshing
            .lock()
            .unwrap_or_else(|error| error.into_inner())
            .remove(&self.key);
    }
}

fn strict_form_encoding(value: &str) -> bool {
    let bytes = value.as_bytes();
    let mut decoded = Zeroizing::new(Vec::with_capacity(bytes.len()));
    let mut index = 0;
    while index < bytes.len() {
        match bytes[index] {
            b'%' => {
                if index + 2 >= bytes.len() {
                    return false;
                }
                let Some(high) = hex_value(bytes[index + 1]) else {
                    return false;
                };
                let Some(low) = hex_value(bytes[index + 2]) else {
                    return false;
                };
                decoded.push((high << 4) | low);
                index += 3;
            }
            b'+' => {
                decoded.push(b' ');
                index += 1;
            }
            byte => {
                decoded.push(byte);
                index += 1;
            }
        }
    }
    std::str::from_utf8(&decoded).is_ok()
}

fn hex_value(byte: u8) -> Option<u8> {
    match byte {
        b'0'..=b'9' => Some(byte - b'0'),
        b'a'..=b'f' => Some(byte - b'a' + 10),
        b'A'..=b'F' => Some(byte - b'A' + 10),
        _ => None,
    }
}

fn valid_scope_token(scope: &str) -> bool {
    !scope.is_empty()
        && scope.len() <= 256
        && scope.bytes().all(|byte| {
            byte == b'!' || (b'#'..=b'[').contains(&byte) || (b']'..=b'~').contains(&byte)
        })
}

fn collect_scopes_bounded<'a, I>(scopes: I) -> Result<Vec<&'a str>, OAuthError>
where
    I: IntoIterator<Item = &'a str>,
{
    let mut validated = Vec::new();
    let mut bytes = 0usize;
    for scope in scopes {
        if !valid_scope_token(scope) || validated.len() == MAX_SCOPE_COUNT {
            return Err(OAuthError::InvalidConfiguration(
                "invalid OAuth scope token",
            ));
        }
        bytes = bytes
            .checked_add(scope.len())
            .and_then(|total| total.checked_add(usize::from(!validated.is_empty())))
            .ok_or(OAuthError::InvalidConfiguration(
                "OAuth scopes exceeded size limit",
            ))?;
        if bytes > MAX_SCOPE_BYTES {
            return Err(OAuthError::InvalidConfiguration(
                "OAuth scopes exceeded size limit",
            ));
        }
        validated.push(scope);
    }
    Ok(validated)
}

fn ensure_registration_metadata_bounded(
    metadata: &DynamicClientRegistrationRequest,
) -> Result<(), OAuthError> {
    let fields = [
        Some(metadata.client_name.as_str()),
        metadata.client_description.as_ref().map(String::as_str),
        metadata.client_url.as_ref().map(String::as_str),
        Some(metadata.redirect_url.as_str()),
    ];
    let source_bytes = fields
        .into_iter()
        .flatten()
        .try_fold(0usize, |total, value| total.checked_add(value.len()));
    // JSON control characters can expand to a six-byte `\u00XX` escape. Bound
    // that worst case before serde_json allocates the duplicate request buffer.
    let encoded_upper_bound = source_bytes
        .and_then(|bytes| bytes.checked_mul(6))
        .and_then(|bytes| bytes.checked_add(256));
    if encoded_upper_bound.is_some_and(|bytes| bytes <= MAX_OAUTH_REQUEST_BYTES) {
        Ok(())
    } else {
        Err(OAuthError::InvalidConfiguration(
            "registration request exceeded size limit",
        ))
    }
}

fn ensure_form_pairs_bounded(pairs: &[(&str, &str)]) -> Result<(), OAuthError> {
    let source_bytes = pairs.iter().try_fold(0usize, |total, (name, value)| {
        total
            .checked_add(name.len())
            .and_then(|bytes| bytes.checked_add(value.len()))
            .and_then(|bytes| bytes.checked_add(2))
    });
    // Percent encoding expands each byte by at most 3x. Reject before the
    // serializer allocates a credential-bearing form buffer.
    let encoded_upper_bound = source_bytes.and_then(|bytes| bytes.checked_mul(3));
    if encoded_upper_bound.is_some_and(|bytes| bytes <= MAX_OAUTH_REQUEST_BYTES) {
        Ok(())
    } else {
        Err(OAuthError::InvalidConfiguration(
            "OAuth token request exceeded size limit",
        ))
    }
}

fn basic_authorization(
    client_id: &str,
    client_secret: &str,
) -> Result<http::HeaderValue, OAuthError> {
    let encoded_id: String = form_urlencoded::byte_serialize(client_id.as_bytes()).collect();
    let encoded_secret = Zeroizing::new(
        form_urlencoded::byte_serialize(client_secret.as_bytes()).collect::<String>(),
    );
    let mut credential = Zeroizing::new(Vec::with_capacity(
        encoded_id
            .len()
            .saturating_add(encoded_secret.len())
            .saturating_add(1),
    ));
    credential.extend_from_slice(encoded_id.as_bytes());
    credential.push(b':');
    credential.extend_from_slice(encoded_secret.as_bytes());
    let encoded_len = credential
        .len()
        .checked_add(2)
        .and_then(|length| length.checked_div(3))
        .and_then(|length| length.checked_mul(4))
        .ok_or(OAuthError::InvalidConfiguration(
            "OAuth client credentials are too large",
        ))?;
    let mut header = vec![0_u8; 6_usize.saturating_add(encoded_len)];
    header[..6].copy_from_slice(b"Basic ");
    let written = STANDARD
        .encode_slice(credential.as_slice(), &mut header[6..])
        .map_err(|_| OAuthError::InvalidConfiguration("OAuth client credentials are too large"))?;
    header.truncate(6 + written);
    let owned = bytes::Bytes::from_owner(SensitiveBytes::new(header));
    http::HeaderValue::from_maybe_shared(owned)
        .map_err(|_| OAuthError::InvalidConfiguration("invalid OAuth client credentials"))
}

fn validate_redirect_url(value: &str) -> Result<Url, OAuthError> {
    if value.len() > MAX_ENDPOINT_BYTES {
        return Err(OAuthError::InvalidConfiguration(
            "OAuth redirect URL is too large",
        ));
    }
    let url = Url::parse(value)
        .map_err(|_| OAuthError::InvalidConfiguration("invalid OAuth redirect URL"))?;
    if url.host_str().is_none()
        || !url.username().is_empty()
        || url.password().is_some()
        || url.fragment().is_some()
        || url.query().is_some()
    {
        return Err(OAuthError::InvalidConfiguration(
            "redirect URL must be absolute and must not contain userinfo, query, or fragment",
        ));
    }
    let is_https = url.scheme() == "https";
    let is_loopback_http = url.scheme() == "http" && is_loopback_host(&url);
    if !is_https && !is_loopback_http {
        return Err(OAuthError::InvalidConfiguration(
            "redirect URL must use HTTPS or loopback HTTP",
        ));
    }
    Ok(url)
}

fn is_loopback_host(url: &Url) -> bool {
    matches!(
        url.host_str(),
        Some("localhost" | "127.0.0.1" | "[::1]" | "::1")
    )
}

fn random_urlsafe_secret() -> String {
    let mut bytes = [0_u8; 32];
    OsRng.fill_bytes(&mut bytes);
    let result = URL_SAFE_NO_PAD.encode(bytes);
    bytes.zeroize();
    result
}

fn constant_time_state_eq(expected: &str, received: &str) -> bool {
    let mut expected_hash = Sha256::digest(expected.as_bytes());
    let mut received_hash = Sha256::digest(received.as_bytes());
    let equal = bool::from(expected_hash.ct_eq(&received_hash));
    expected_hash.zeroize();
    received_hash.zeroize();
    equal
}

fn constant_time_secret_ne(left: &str, right: &str) -> bool {
    !constant_time_state_eq(left, right)
}

fn has_json_content_type(headers: &http::HeaderMap) -> bool {
    headers
        .get(http::header::CONTENT_TYPE)
        .and_then(|value| value.to_str().ok())
        .and_then(|value| value.split(';').next())
        .is_some_and(|media_type| media_type.trim().eq_ignore_ascii_case("application/json"))
}

fn parse_token_response(
    response: OAuthHttpResponse,
    now: SystemTime,
    require_rotated_refresh: bool,
    previous_refresh: Option<&str>,
) -> Result<TokenSet, OAuthError> {
    if response.status() != http::StatusCode::OK {
        return Err(OAuthError::EndpointRejected);
    }
    if !has_json_content_type(response.headers())
        || response.body().is_empty()
        || response.body().as_bytes().contains(&b'\\')
    {
        // Reject escaped JSON strings: serde_json otherwise retains decoded
        // credential bytes in its non-zeroizing scratch allocation.
        return Err(OAuthError::InvalidTokenResponse);
    }
    let mut deserializer = serde_json::Deserializer::from_slice(response.body().as_bytes());
    let mut wire = TokenResponseWire::deserialize(&mut deserializer)
        .map_err(|_| OAuthError::InvalidTokenResponse)?;
    deserializer
        .end()
        .map_err(|_| OAuthError::InvalidTokenResponse)?;
    if wire.access_token.0.is_empty() {
        return Err(OAuthError::InvalidTokenResponse);
    }
    if !wire.token_type.0.eq_ignore_ascii_case("bearer") {
        return Err(OAuthError::UnsupportedTokenType);
    }
    if require_rotated_refresh {
        let replacement = wire
            .refresh_token
            .as_ref()
            .ok_or(OAuthError::MissingRotatedRefreshToken)?;
        let previous = previous_refresh.ok_or(OAuthError::MissingRotatedRefreshToken)?;
        if replacement.0.is_empty() || !constant_time_secret_ne(previous, &replacement.0) {
            return Err(OAuthError::MissingRotatedRefreshToken);
        }
    }
    let expires_at = wire
        .expires_in
        .map(|seconds| {
            now.checked_add(std::time::Duration::from_secs(seconds))
                .ok_or(OAuthError::InvalidTokenResponse)
        })
        .transpose()?;
    let access = crate::AccessToken::new(wire.access_token.into_string());
    let refresh = wire
        .refresh_token
        .take()
        .map(|secret| RefreshToken::new(secret.into_string()));
    Ok(TokenSet::new(access, refresh, expires_at))
}

struct WireSecret(String);

impl WireSecret {
    fn into_string(mut self) -> String {
        std::mem::take(&mut self.0)
    }
}

impl Drop for WireSecret {
    fn drop(&mut self) {
        self.0.zeroize();
    }
}

impl<'de> Deserialize<'de> for WireSecret {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        struct SecretVisitor;
        impl Visitor<'_> for SecretVisitor {
            type Value = WireSecret;

            fn expecting(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
                formatter.write_str("a secret string")
            }

            fn visit_string<E>(self, value: String) -> Result<Self::Value, E>
            where
                E: de::Error,
            {
                Ok(WireSecret(value))
            }

            fn visit_str<E>(self, value: &str) -> Result<Self::Value, E>
            where
                E: de::Error,
            {
                Ok(WireSecret(value.to_owned()))
            }
        }
        deserializer.deserialize_string(SecretVisitor)
    }
}

struct TokenResponseWire {
    access_token: WireSecret,
    token_type: WireSecret,
    refresh_token: Option<WireSecret>,
    expires_in: Option<u64>,
}

impl<'de> Deserialize<'de> for TokenResponseWire {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        struct TokenVisitor;
        impl<'de> Visitor<'de> for TokenVisitor {
            type Value = TokenResponseWire;

            fn expecting(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
                formatter.write_str("an OAuth token response object")
            }

            fn visit_map<A>(self, mut map: A) -> Result<Self::Value, A::Error>
            where
                A: MapAccess<'de>,
            {
                let mut access_token = None;
                let mut token_type = None;
                let mut refresh_token = None;
                let mut refresh_seen = false;
                let mut expires_in = None;
                let mut expires_seen = false;
                while let Some(key) = map.next_key::<WireSecret>()? {
                    match key.0.as_str() {
                        "access_token" if access_token.is_none() => {
                            access_token = Some(map.next_value()?);
                        }
                        "token_type" if token_type.is_none() => {
                            token_type = Some(map.next_value()?)
                        }
                        "refresh_token" if !refresh_seen => {
                            refresh_seen = true;
                            refresh_token = Some(map.next_value()?);
                        }
                        "expires_in" if !expires_seen => {
                            expires_seen = true;
                            expires_in = Some(map.next_value::<ExpiresIn>()?.0);
                        }
                        "access_token" | "token_type" | "refresh_token" | "expires_in" => {
                            return Err(de::Error::custom("duplicate OAuth response field"));
                        }
                        _ => {
                            let _: IgnoredAny = map.next_value()?;
                        }
                    }
                }
                Ok(TokenResponseWire {
                    access_token: access_token
                        .ok_or_else(|| de::Error::missing_field("access_token"))?,
                    token_type: token_type.ok_or_else(|| de::Error::missing_field("token_type"))?,
                    refresh_token,
                    expires_in,
                })
            }
        }
        deserializer.deserialize_map(TokenVisitor)
    }
}

struct ExpiresIn(u64);

impl<'de> Deserialize<'de> for ExpiresIn {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        struct ExpiresVisitor;
        impl Visitor<'_> for ExpiresVisitor {
            type Value = ExpiresIn;

            fn expecting(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
                formatter.write_str("a non-negative integer or canonical decimal string")
            }

            fn visit_u64<E>(self, value: u64) -> Result<Self::Value, E> {
                Ok(ExpiresIn(value))
            }

            fn visit_str<E>(self, value: &str) -> Result<Self::Value, E>
            where
                E: de::Error,
            {
                if value.is_empty()
                    || !value.bytes().all(|byte| byte.is_ascii_digit())
                    || (value.len() > 1 && value.starts_with('0'))
                {
                    return Err(E::custom("invalid expires_in"));
                }
                value
                    .parse()
                    .map(ExpiresIn)
                    .map_err(|_| E::custom("invalid expires_in"))
            }
        }
        deserializer.deserialize_any(ExpiresVisitor)
    }
}

struct RegistrationResponse {
    client_id: String,
    client_secret: WireSecret,
}

impl<'de> Deserialize<'de> for RegistrationResponse {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        struct RegistrationVisitor;
        impl<'de> Visitor<'de> for RegistrationVisitor {
            type Value = RegistrationResponse;

            fn expecting(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
                formatter.write_str("a dynamic client registration response")
            }

            fn visit_map<A>(self, mut map: A) -> Result<Self::Value, A::Error>
            where
                A: MapAccess<'de>,
            {
                let mut client_id = None;
                let mut client_secret = None;
                while let Some(key) = map.next_key::<WireSecret>()? {
                    match key.0.as_str() {
                        "client_id" if client_id.is_none() => client_id = Some(map.next_value()?),
                        "client_secret" if client_secret.is_none() => {
                            client_secret = Some(map.next_value()?);
                        }
                        "client_id" | "client_secret" => {
                            return Err(de::Error::custom("duplicate registration field"));
                        }
                        _ => {
                            let _: IgnoredAny = map.next_value()?;
                        }
                    }
                }
                Ok(RegistrationResponse {
                    client_id: client_id.ok_or_else(|| de::Error::missing_field("client_id"))?,
                    client_secret: client_secret
                        .ok_or_else(|| de::Error::missing_field("client_secret"))?,
                })
            }
        }
        deserializer.deserialize_map(RegistrationVisitor)
    }
}

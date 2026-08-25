//! Shared contracts from buildingSMART Foundation API 1.1.
//!
//! The upstream repository publishes JSON Schema Draft 3 fragments rather than
//! one OpenAPI document. Requiredness follows the normative README where it is
//! stricter than those fragments (notably `supported_oauth2_flows`).

use core::fmt;
use serde::{Deserialize, Serialize};

/// Fixed discovery path, independent of any advertised Foundation base URL.
pub const VERSIONS_PATH: &str = "/foundation/versions";

/// Foundation API version implemented by these contracts.
pub const VERSION: &str = "1.1";

/// Response from `GET /foundation/versions`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct VersionsResponse {
    /// Advertised OpenCDE API implementations.
    ///
    /// The upstream schema permits null array entries, so the wire model does
    /// too even though useful servers normally return only objects.
    pub versions: Vec<Option<ApiVersion>>,
}

/// One OpenCDE API and version advertised by a server.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ApiVersion {
    /// API family identifier, for example `foundation` or `documents`.
    pub api_id: String,
    /// Family-specific version, for example `1.1`.
    pub version_id: String,
    /// Link to the detailed specification.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub detailed_version: Option<String>,
    /// Optional relocated base URL for this API.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub api_base_url: Option<String>,
}

/// Authentication capabilities returned by the Foundation discovery endpoint.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AuthenticationDiscovery {
    /// OAuth2 authorization endpoint.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub oauth2_auth_url: Option<String>,
    /// OAuth2 token endpoint.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub oauth2_token_url: Option<String>,
    /// Optional dynamic client-registration endpoint.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub oauth2_dynamic_client_reg_url: Option<String>,
    /// Whether HTTP Basic authentication is supported.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub http_basic_supported: Option<bool>,
    /// OAuth2 flows the server supports.
    ///
    /// The prose table marks this field required, but the released Draft 3
    /// schema permits omission and `null`. Wire parsing follows the machine
    /// schema; prose-level validation can reject `None` at an application
    /// boundary.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub supported_oauth2_flows: Option<Vec<OAuth2Flow>>,
}

/// OAuth2 flows allowed by Foundation API 1.1.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum OAuth2Flow {
    /// RFC 6749 §4.1.
    #[serde(rename = "authorization_code_grant")]
    AuthorizationCode,
    /// RFC 6749 §4.2.
    #[serde(rename = "implicit_grant")]
    Implicit,
    /// RFC 6749 §4.3.
    #[serde(rename = "resource_owner_password_credentials_grant")]
    ResourceOwnerPasswordCredentials,
}

/// Dynamic client-registration request.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DynamicClientRegistrationRequest {
    /// Human-readable client name.
    pub client_name: String,
    /// Optional client description.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub client_description: Option<String>,
    /// Optional URL with client information.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub client_url: Option<String>,
    /// Redirect URL after authorization.
    pub redirect_url: String,
}

/// Dynamic client-registration response.
#[derive(Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DynamicClientRegistrationResponse {
    /// Server-issued client identifier.
    pub client_id: String,
    /// Server-issued client secret. Debug output is always redacted.
    pub client_secret: ClientSecret,
}

impl fmt::Debug for DynamicClientRegistrationResponse {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("DynamicClientRegistrationResponse")
            .field("client_id", &self.client_id)
            .field("client_secret", &self.client_secret)
            .finish()
    }
}

/// A client secret whose `Debug` representation cannot disclose its value.
#[derive(Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(transparent)]
pub struct ClientSecret(String);

impl ClientSecret {
    /// Construct a secret from its wire value.
    #[must_use]
    pub fn new(value: impl Into<String>) -> Self {
        Self(value.into())
    }

    /// Explicitly expose the secret for authentication use.
    #[must_use]
    pub fn expose_secret(&self) -> &str {
        &self.0
    }
}

impl fmt::Debug for ClientSecret {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("ClientSecret([REDACTED])")
    }
}

/// Authenticated user returned by the current-user service.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct User {
    /// Server-specific user identifier.
    pub id: String,
    /// Optional display name.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
}

/// Common Foundation error response.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ErrorResponse {
    /// Human-readable error description.
    pub message: String,
}

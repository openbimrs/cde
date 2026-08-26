use openbim_cde::foundation::{AuthenticationDiscovery, OAuth2Flow};
use std::{error::Error, fmt};
use url::Url;

/// Policy for validating authorization-server endpoint URLs.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum EndpointSecurityPolicy {
    /// Require HTTPS for every endpoint.
    #[default]
    HttpsOnly,
    /// Also permit cleartext HTTP on loopback hosts for local tests and development.
    AllowLoopbackHttp,
}

/// Validated executable OAuth configuration derived from Foundation discovery.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OAuthConfig {
    authorization_endpoint: Option<Url>,
    token_endpoint: Option<Url>,
    registration_endpoint: Option<Url>,
    http_basic_supported: bool,
    flows: Vec<OAuth2Flow>,
}

impl OAuthConfig {
    /// Validate discovery metadata using the secure HTTPS-only policy.
    pub fn from_discovery(value: AuthenticationDiscovery) -> Result<Self, DiscoveryError> {
        Self::from_discovery_with_policy(value, EndpointSecurityPolicy::HttpsOnly)
    }

    /// Validate discovery metadata under an explicit endpoint policy.
    pub fn from_discovery_with_policy(
        value: AuthenticationDiscovery,
        policy: EndpointSecurityPolicy,
    ) -> Result<Self, DiscoveryError> {
        let authorization_endpoint =
            parse_endpoint("oauth2_auth_url", value.oauth2_auth_url, policy)?;
        let token_endpoint = parse_endpoint("oauth2_token_url", value.oauth2_token_url, policy)?;
        let registration_endpoint = parse_endpoint(
            "oauth2_dynamic_client_reg_url",
            value.oauth2_dynamic_client_reg_url,
            policy,
        )?;
        let flows = value
            .supported_oauth2_flows
            .ok_or(DiscoveryError::MissingSupportedFlows)?;

        if flows.is_empty() {
            return Err(DiscoveryError::MissingSupportedFlows);
        }
        if flows
            .iter()
            .enumerate()
            .any(|(index, flow)| flows[index + 1..].contains(flow))
        {
            return Err(DiscoveryError::DuplicateSupportedFlow);
        }

        if flows.contains(&OAuth2Flow::AuthorizationCode)
            && (authorization_endpoint.is_none() || token_endpoint.is_none())
        {
            return Err(DiscoveryError::UnpairedAuthorizationEndpoints);
        }
        if flows.contains(&OAuth2Flow::Implicit) && authorization_endpoint.is_none() {
            return Err(DiscoveryError::MissingAuthorizationEndpoint);
        }
        if flows.contains(&OAuth2Flow::ResourceOwnerPasswordCredentials) && token_endpoint.is_none()
        {
            return Err(DiscoveryError::MissingTokenEndpoint);
        }

        Ok(Self {
            authorization_endpoint,
            token_endpoint,
            registration_endpoint,
            http_basic_supported: value.http_basic_supported.unwrap_or(false),
            flows,
        })
    }

    /// Confirm a flow is advertised and executable by this crate.
    pub fn require_executable_flow(&self, flow: OAuth2Flow) -> Result<(), DiscoveryError> {
        if !self.flows.contains(&flow) {
            return Err(DiscoveryError::FlowNotAdvertised(flow));
        }
        if flow == OAuth2Flow::Implicit {
            return Err(DiscoveryError::UnsupportedFlow(flow));
        }
        Ok(())
    }

    /// Validated authorization endpoint, when advertised.
    #[must_use]
    pub fn authorization_endpoint(&self) -> Option<&Url> {
        self.authorization_endpoint.as_ref()
    }

    /// Validated token endpoint, when advertised.
    #[must_use]
    pub fn token_endpoint(&self) -> Option<&Url> {
        self.token_endpoint.as_ref()
    }

    /// Validated dynamic-registration endpoint, when advertised.
    #[must_use]
    pub fn registration_endpoint(&self) -> Option<&Url> {
        self.registration_endpoint.as_ref()
    }

    /// Whether discovery advertises HTTP Basic client authentication.
    #[must_use]
    pub fn http_basic_supported(&self) -> bool {
        self.http_basic_supported
    }

    /// Advertised Foundation OAuth flows.
    #[must_use]
    pub fn flows(&self) -> &[OAuth2Flow] {
        &self.flows
    }
}

impl TryFrom<AuthenticationDiscovery> for OAuthConfig {
    type Error = DiscoveryError;

    fn try_from(value: AuthenticationDiscovery) -> Result<Self, Self::Error> {
        Self::from_discovery(value)
    }
}

fn parse_endpoint(
    field: &'static str,
    value: Option<String>,
    policy: EndpointSecurityPolicy,
) -> Result<Option<Url>, DiscoveryError> {
    let Some(value) = value else { return Ok(None) };
    if value.len() > 8 * 1024 {
        return Err(DiscoveryError::InvalidEndpoint {
            field,
            reason: "endpoint exceeds the size limit".to_owned(),
        });
    }
    let url = Url::parse(&value).map_err(|error| DiscoveryError::InvalidEndpoint {
        field,
        reason: error.to_string(),
    })?;
    if url.host_str().is_none()
        || !url.username().is_empty()
        || url.password().is_some()
        || url.query().is_some()
        || url.fragment().is_some()
    {
        return Err(DiscoveryError::InvalidEndpoint {
            field,
            reason: "endpoint must be absolute and must not contain userinfo, query, or fragment"
                .to_owned(),
        });
    }
    let secure = url.scheme() == "https";
    let allowed_loopback = policy == EndpointSecurityPolicy::AllowLoopbackHttp
        && url.scheme() == "http"
        && matches!(
            url.host_str(),
            Some("localhost" | "127.0.0.1" | "[::1]" | "::1")
        );
    if !secure && !allowed_loopback {
        return Err(DiscoveryError::InsecureEndpoint { field });
    }
    Ok(Some(url))
}

/// Failure while turning Foundation discovery into executable OAuth policy.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DiscoveryError {
    MissingSupportedFlows,
    DuplicateSupportedFlow,
    UnpairedAuthorizationEndpoints,
    MissingAuthorizationEndpoint,
    MissingTokenEndpoint,
    InvalidEndpoint { field: &'static str, reason: String },
    InsecureEndpoint { field: &'static str },
    FlowNotAdvertised(OAuth2Flow),
    UnsupportedFlow(OAuth2Flow),
}

impl fmt::Display for DiscoveryError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::MissingSupportedFlows => {
                formatter.write_str("discovery omitted supported OAuth2 flows")
            }
            Self::DuplicateSupportedFlow => {
                formatter.write_str("discovery repeated a supported OAuth2 flow")
            }
            Self::UnpairedAuthorizationEndpoints => formatter.write_str(
                "authorization-code discovery requires both authorization and token endpoints",
            ),
            Self::MissingAuthorizationEndpoint => {
                formatter.write_str("the advertised flow requires an authorization endpoint")
            }
            Self::MissingTokenEndpoint => {
                formatter.write_str("the advertised flow requires a token endpoint")
            }
            Self::InvalidEndpoint { field, reason } => {
                write!(formatter, "invalid {field}: {reason}")
            }
            Self::InsecureEndpoint { field } => write!(formatter, "{field} must use HTTPS"),
            Self::FlowNotAdvertised(flow) => {
                write!(formatter, "OAuth flow {flow:?} was not advertised")
            }
            Self::UnsupportedFlow(flow) => write!(
                formatter,
                "OAuth flow {flow:?} is recognized but intentionally unsupported"
            ),
        }
    }
}

impl Error for DiscoveryError {}

use openbim_cde::foundation::{AuthenticationDiscovery, OAuth2Flow};
use openbim_cde_oauth::{DiscoveryError, OAuthConfig};

fn discovery(
    auth: Option<&str>,
    token: Option<&str>,
    flows: Option<Vec<OAuth2Flow>>,
) -> AuthenticationDiscovery {
    AuthenticationDiscovery {
        oauth2_auth_url: auth.map(str::to_owned),
        oauth2_token_url: token.map(str::to_owned),
        oauth2_dynamic_client_reg_url: None,
        http_basic_supported: Some(true),
        supported_oauth2_flows: flows,
    }
}

#[test]
fn discovery_requires_paired_authorization_and_token_endpoints() {
    let wire = discovery(
        Some("https://id.example/authorize"),
        None,
        Some(vec![OAuth2Flow::AuthorizationCode]),
    );

    assert_eq!(
        OAuthConfig::try_from(wire).unwrap_err(),
        DiscoveryError::UnpairedAuthorizationEndpoints
    );
}

#[test]
fn discovery_rejects_cleartext_oauth_endpoints() {
    let wire = discovery(
        Some("http://id.example/authorize"),
        Some("http://id.example/token"),
        Some(vec![OAuth2Flow::AuthorizationCode]),
    );

    assert!(matches!(
        OAuthConfig::try_from(wire),
        Err(DiscoveryError::InsecureEndpoint { .. })
    ));
}

#[test]
fn implicit_is_recognized_but_typed_unsupported() {
    let wire = discovery(
        Some("https://id.example/authorize"),
        None,
        Some(vec![OAuth2Flow::Implicit]),
    );
    let config = OAuthConfig::try_from(wire).unwrap();

    assert_eq!(
        config.require_executable_flow(OAuth2Flow::Implicit),
        Err(DiscoveryError::UnsupportedFlow(OAuth2Flow::Implicit))
    );
}

#[test]
fn token_only_password_discovery_is_valid() {
    let wire = discovery(
        None,
        Some("https://id.example/token"),
        Some(vec![OAuth2Flow::ResourceOwnerPasswordCredentials]),
    );
    assert!(OAuthConfig::try_from(wire).is_ok());
}

#[test]
fn every_discovered_endpoint_rejects_query_fragment_and_userinfo() {
    let endpoint_attacks = [
        "https://id.example/authorize?response_type=token",
        "https://id.example/token?grant_type=client_credentials",
        "https://id.example/register?redirect_uri=https%3A%2F%2Fevil.example",
        "https://id.example/token#fragment",
        "https://attacker@id.example/token",
    ];
    for attack in endpoint_attacks {
        let mut wire = discovery(
            Some("https://id.example/authorize"),
            Some("https://id.example/token"),
            Some(vec![OAuth2Flow::AuthorizationCode]),
        );
        if attack.contains("authorize") {
            wire.oauth2_auth_url = Some(attack.into());
        } else if attack.contains("register") {
            wire.oauth2_dynamic_client_reg_url = Some(attack.into());
        } else {
            wire.oauth2_token_url = Some(attack.into());
        }
        assert!(
            matches!(
                OAuthConfig::try_from(wire),
                Err(DiscoveryError::InvalidEndpoint { .. })
            ),
            "accepted polluted endpoint: {attack}"
        );
    }
}

#[test]
fn duplicate_or_empty_flow_advertisements_are_rejected() {
    let empty = discovery(None, None, Some(vec![]));
    assert_eq!(
        OAuthConfig::try_from(empty).unwrap_err(),
        DiscoveryError::MissingSupportedFlows
    );

    let duplicate = discovery(
        Some("https://id.example/authorize"),
        Some("https://id.example/token"),
        Some(vec![
            OAuth2Flow::AuthorizationCode,
            OAuth2Flow::AuthorizationCode,
        ]),
    );
    assert_eq!(
        OAuthConfig::try_from(duplicate).unwrap_err(),
        DiscoveryError::DuplicateSupportedFlow
    );
}

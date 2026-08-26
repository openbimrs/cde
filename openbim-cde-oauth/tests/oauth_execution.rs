use async_trait::async_trait;
use http::{header::CONTENT_TYPE, Response, StatusCode};
use openbim_cde::{
    foundation::{AuthenticationDiscovery, DynamicClientRegistrationRequest, OAuth2Flow},
    OptionalNonNull,
};
use openbim_cde_oauth::{
    AccessToken, ClientCredentials, ClientSecret, LegacyPasswordPolicy, MemoryTokenStore,
    OAuthConfig, OAuthError, OAuthExecutor, OAuthHttpRequest, OAuthHttpResponse,
    OAuthHttpTransport, PendingAuthorization, RefreshToken, ResourceOwnerPassword, SensitiveBytes,
    TokenKey, TokenSet, TokenStore, TransportError,
};
use std::{
    collections::VecDeque,
    sync::{Arc, Mutex},
    time::SystemTime,
};
use tokio::sync::Notify;

#[derive(Clone, Default)]
struct ScriptedTransport {
    responses: Arc<Mutex<VecDeque<Result<OAuthHttpResponse, TransportError>>>>,
    requests: Arc<Mutex<Vec<OAuthHttpRequest>>>,
    started: Option<Arc<Notify>>,
    release: Option<Arc<Notify>>,
}

impl ScriptedTransport {
    fn with_responses(responses: Vec<Result<OAuthHttpResponse, TransportError>>) -> Self {
        Self {
            responses: Arc::new(Mutex::new(responses.into())),
            requests: Arc::default(),
            started: None,
            release: None,
        }
    }

    fn with_blocked_response(
        response: Result<OAuthHttpResponse, TransportError>,
    ) -> (Self, Arc<Notify>, Arc<Notify>) {
        let started = Arc::new(Notify::new());
        let release = Arc::new(Notify::new());
        (
            Self {
                responses: Arc::new(Mutex::new(VecDeque::from([response]))),
                requests: Arc::default(),
                started: Some(Arc::clone(&started)),
                release: Some(Arc::clone(&release)),
            },
            started,
            release,
        )
    }

    fn requests(&self) -> Vec<(String, String)> {
        self.requests
            .lock()
            .unwrap()
            .iter()
            .map(|request| {
                (
                    request.uri().to_string(),
                    String::from_utf8(request.body().as_bytes().to_vec()).unwrap(),
                )
            })
            .collect()
    }

    fn has_authorization_header(&self, expected: &str) -> bool {
        self.requests.lock().unwrap().iter().any(|request| {
            request
                .headers()
                .get(http::header::AUTHORIZATION)
                .is_some_and(|value| value.as_bytes() == expected.as_bytes())
        })
    }
}

#[async_trait]
impl OAuthHttpTransport for ScriptedTransport {
    async fn execute(
        &self,
        request: OAuthHttpRequest,
    ) -> Result<OAuthHttpResponse, TransportError> {
        let should_block = {
            let mut requests = self.requests.lock().unwrap();
            requests.push(request);
            requests.len() == 1
        };
        if should_block {
            if let (Some(started), Some(release)) = (&self.started, &self.release) {
                started.notify_one();
                release.notified().await;
            }
        }
        self.responses
            .lock()
            .unwrap()
            .pop_front()
            .expect("unexpected request")
    }
}

fn token_response(status: StatusCode, json: &str) -> OAuthHttpResponse {
    OAuthHttpResponse::from_http(
        Response::builder()
            .status(status)
            .header(CONTENT_TYPE, "application/json")
            .body(SensitiveBytes::new(json.as_bytes().to_vec()))
            .unwrap(),
    )
}

fn executor(
    flows: Vec<OAuth2Flow>,
    transport: ScriptedTransport,
) -> OAuthExecutor<ScriptedTransport> {
    executor_with_basic(flows, true, transport)
}

fn executor_with_basic(
    flows: Vec<OAuth2Flow>,
    http_basic_supported: bool,
    transport: ScriptedTransport,
) -> OAuthExecutor<ScriptedTransport> {
    let discovery = AuthenticationDiscovery {
        oauth2_auth_url: Some("https://id.example/authorize".into()),
        oauth2_token_url: Some("https://id.example/token".into()),
        oauth2_dynamic_client_reg_url: Some("https://id.example/register".into()),
        http_basic_supported: Some(http_basic_supported),
        supported_oauth2_flows: Some(flows),
    };
    let config = OAuthConfig::from_discovery(discovery).unwrap();
    OAuthExecutor::new(
        config,
        ClientCredentials::new("client-id", Some(ClientSecret::new("client-secret"))),
        "https://client.example/callback",
        transport,
    )
    .unwrap()
}

fn pending_state(pending: &PendingAuthorization) -> String {
    pending
        .authorization_url()
        .query_pairs()
        .find(|(name, _)| name == "state")
        .map(|(_, value)| value.into_owned())
        .unwrap()
}

fn form_pairs(body: &str) -> Vec<(String, String)> {
    url::form_urlencoded::parse(body.as_bytes())
        .into_owned()
        .collect()
}

#[test]
fn oauth_http_debug_redacts_uri_headers_and_bodies() {
    let request = OAuthHttpRequest::from_http(
        http::Request::builder()
            .method(http::Method::POST)
            .uri("https://id.example/token?code=query-secret")
            .header(http::header::AUTHORIZATION, "Basic header-secret")
            .body(SensitiveBytes::new(b"body-secret".to_vec()))
            .unwrap(),
    );
    let response = OAuthHttpResponse::from_http(
        Response::builder()
            .status(StatusCode::OK)
            .header("set-cookie", "session=header-secret")
            .body(SensitiveBytes::new(b"response-secret".to_vec()))
            .unwrap(),
    );

    let diagnostics = format!("{request:?} {response:?}");
    for secret in [
        "query-secret",
        "header-secret",
        "body-secret",
        "response-secret",
        "id.example",
    ] {
        assert!(!diagnostics.contains(secret));
    }
    assert!(diagnostics.contains("[REDACTED]"));
}

#[test]
fn executor_rejects_non_loopback_cleartext_redirects() {
    let discovery = AuthenticationDiscovery {
        oauth2_auth_url: Some("https://id.example/authorize".into()),
        oauth2_token_url: Some("https://id.example/token".into()),
        oauth2_dynamic_client_reg_url: None,
        http_basic_supported: Some(true),
        supported_oauth2_flows: Some(vec![OAuth2Flow::AuthorizationCode]),
    };
    let result = OAuthExecutor::new(
        OAuthConfig::from_discovery(discovery).unwrap(),
        ClientCredentials::new("client-id", None),
        "http://client.example/callback",
        ScriptedTransport::default(),
    );
    assert!(matches!(result, Err(OAuthError::InvalidConfiguration(_))));
}

#[test]
fn authorization_input_and_final_url_are_bounded() {
    let oauth = executor(
        vec![OAuth2Flow::AuthorizationCode],
        ScriptedTransport::default(),
    );
    assert!(matches!(
        oauth.begin_authorization((0..129).map(|_| "s")),
        Err(OAuthError::InvalidConfiguration(_))
    ));
    let max_token = "s".repeat(256);
    assert!(matches!(
        oauth.begin_authorization((0..33).map(|_| max_token.as_str())),
        Err(OAuthError::InvalidConfiguration(_))
    ));
}

#[tokio::test]
async fn authorization_code_always_uses_s256_pkce_and_matching_state() {
    let transport = ScriptedTransport::with_responses(vec![Ok(token_response(
        StatusCode::OK,
        r#"{"access_token":"access","token_type":"bearer","expires_in":"3600","refresh_token":"refresh"}"#,
    ))]);
    let oauth = executor(vec![OAuth2Flow::AuthorizationCode], transport.clone());

    let pending = oauth.begin_authorization(["documents.read"]).unwrap();
    let pairs: Vec<_> = pending
        .authorization_url()
        .query_pairs()
        .into_owned()
        .collect();
    for required in [
        "response_type",
        "client_id",
        "redirect_uri",
        "state",
        "code_challenge",
        "code_challenge_method",
        "scope",
    ] {
        assert_eq!(
            pairs.iter().filter(|(name, _)| name == required).count(),
            1,
            "authorization parameter {required} was not unique"
        );
    }
    assert!(pairs.iter().all(|(name, _)| matches!(
        name.as_str(),
        "response_type"
            | "client_id"
            | "redirect_uri"
            | "state"
            | "code_challenge"
            | "code_challenge_method"
            | "scope"
    )));
    let get = |name: &str| {
        pairs
            .iter()
            .find(|(candidate, _)| candidate == name)
            .map(|(_, value)| value.as_str())
            .unwrap()
    };
    assert_eq!(get("response_type"), "code");
    assert_eq!(get("code_challenge_method"), "S256");
    assert!(!get("code_challenge").is_empty());
    let state = get("state").to_owned();

    let callback = format!("https://client.example/callback?code=code-value&state={state}");
    let token = oauth
        .exchange_authorization_code(pending, &callback, SystemTime::UNIX_EPOCH)
        .await
        .unwrap();
    assert_eq!(token.bearer_token().expose(), "access");
    assert!(token.expires_at().is_some());

    let requests = transport.requests();
    assert_eq!(requests.len(), 1);
    assert!(requests[0].1.contains("grant_type=authorization_code"));
    assert!(requests[0].1.contains("code_verifier="));
    assert!(requests[0].1.contains("redirect_uri="));
}

#[tokio::test]
async fn state_mismatch_is_rejected_before_transport() {
    let transport = ScriptedTransport::default();
    let oauth = executor(vec![OAuth2Flow::AuthorizationCode], transport.clone());
    let pending = oauth.begin_authorization([]).unwrap();

    let error = oauth
        .exchange_authorization_code(
            pending,
            "https://client.example/callback?code=code&state=wrong-state",
            SystemTime::now(),
        )
        .await
        .unwrap_err();
    assert_eq!(error, OAuthError::StateMismatch);
    assert!(transport.requests().is_empty());
}

#[tokio::test]
async fn refresh_rotates_one_time_token_and_cas_updates_store() {
    let transport = ScriptedTransport::with_responses(vec![Ok(token_response(
        StatusCode::OK,
        r#"{"access_token":"new-access","token_type":"bearer","refresh_token":"new-refresh"}"#,
    ))]);
    let oauth = executor(vec![OAuth2Flow::AuthorizationCode], transport.clone());
    let store = MemoryTokenStore::new();
    let key = TokenKey::new("server/user").unwrap();
    store
        .compare_and_swap(
            &key,
            None,
            TokenSet::new(
                AccessToken::new("old-access"),
                Some(RefreshToken::new("old-refresh")),
                None,
            ),
        )
        .await
        .unwrap();

    let updated = oauth
        .refresh_stored(&store, &key, SystemTime::now())
        .await
        .unwrap();
    assert_eq!(updated.generation(), 2);
    assert_eq!(updated.token().bearer_token().expose(), "new-access");
    assert!(transport.requests()[0]
        .1
        .contains("refresh_token=old-refresh"));
}

#[tokio::test]
async fn failed_refresh_preserves_stored_token() {
    let transport = ScriptedTransport::with_responses(vec![Ok(token_response(
        StatusCode::BAD_REQUEST,
        r#"{"error":"invalid_grant"}"#,
    ))]);
    let oauth = executor(vec![OAuth2Flow::AuthorizationCode], transport);
    let store = MemoryTokenStore::new();
    let key = TokenKey::new("server/user").unwrap();
    store
        .compare_and_swap(
            &key,
            None,
            TokenSet::new(
                AccessToken::new("still-valid"),
                Some(RefreshToken::new("one-time")),
                None,
            ),
        )
        .await
        .unwrap();

    assert!(oauth
        .refresh_stored(&store, &key, SystemTime::now())
        .await
        .is_err());
    let unchanged = store.load(&key).await.unwrap().unwrap();
    assert_eq!(unchanged.generation(), 1);
    assert_eq!(unchanged.token().bearer_token().expose(), "still-valid");
}

#[tokio::test]
async fn refresh_without_rotated_refresh_token_is_rejected_and_preserved() {
    let transport = ScriptedTransport::with_responses(vec![Ok(token_response(
        StatusCode::OK,
        r#"{"access_token":"new-access","token_type":"bearer"}"#,
    ))]);
    let oauth = executor(vec![OAuth2Flow::AuthorizationCode], transport);
    let store = MemoryTokenStore::new();
    let key = TokenKey::new("server/user").unwrap();
    store
        .compare_and_swap(
            &key,
            None,
            TokenSet::new(
                AccessToken::new("old-access"),
                Some(RefreshToken::new("one-time")),
                None,
            ),
        )
        .await
        .unwrap();

    let error = oauth
        .refresh_stored(&store, &key, SystemTime::now())
        .await
        .unwrap_err();
    assert_eq!(error, OAuthError::MissingRotatedRefreshToken);
    assert_eq!(store.load(&key).await.unwrap().unwrap().generation(), 1);
}

#[tokio::test]
async fn password_grant_requires_both_discovery_and_explicit_legacy_policy() {
    let transport = ScriptedTransport::default();
    let oauth = executor(
        vec![OAuth2Flow::ResourceOwnerPasswordCredentials],
        transport.clone(),
    );
    let error = oauth
        .exchange_password(
            "alice",
            &ResourceOwnerPassword::new("password"),
            LegacyPasswordPolicy::Disabled,
            [],
            SystemTime::now(),
        )
        .await
        .unwrap_err();
    assert_eq!(error, OAuthError::LegacyPasswordGrantDisabled);
    assert!(transport.requests().is_empty());

    let oauth = executor(vec![OAuth2Flow::AuthorizationCode], transport);
    let error = oauth
        .exchange_password(
            "alice",
            &ResourceOwnerPassword::new("password"),
            LegacyPasswordPolicy::Enabled,
            [],
            SystemTime::now(),
        )
        .await
        .unwrap_err();
    assert_eq!(
        error,
        OAuthError::FlowNotAdvertised(OAuth2Flow::ResourceOwnerPasswordCredentials)
    );
}

#[tokio::test]
async fn registration_and_token_forms_are_preflight_bounded_before_transport() {
    let registration_transport = ScriptedTransport::default();
    let registration_oauth = executor(
        vec![OAuth2Flow::AuthorizationCode],
        registration_transport.clone(),
    );
    let oversized_metadata = DynamicClientRegistrationRequest {
        client_name: "n".repeat(200_000),
        client_description: OptionalNonNull::Missing,
        client_url: OptionalNonNull::Missing,
        redirect_url: "https://client.example/callback".into(),
    };
    assert!(matches!(
        registration_oauth
            .register_client(&oversized_metadata)
            .await,
        Err(OAuthError::InvalidConfiguration(_))
    ));
    assert!(registration_transport.requests().is_empty());

    let password_transport = ScriptedTransport::default();
    let password_oauth = executor(
        vec![OAuth2Flow::ResourceOwnerPasswordCredentials],
        password_transport.clone(),
    );
    let oversized_password = ResourceOwnerPassword::new("p".repeat(400_000));
    assert!(matches!(
        password_oauth
            .exchange_password(
                "user",
                &oversized_password,
                LegacyPasswordPolicy::Enabled,
                std::iter::empty(),
                SystemTime::now(),
            )
            .await,
        Err(OAuthError::InvalidConfiguration(_))
    ));
    assert!(password_transport.requests().is_empty());
}

#[tokio::test]
async fn dynamic_registration_maps_foundation_contracts_and_redacts_credentials() {
    let transport = ScriptedTransport::with_responses(vec![Ok(token_response(
        StatusCode::CREATED,
        r#"{"client_id":"registered-id","client_secret":"registered-secret"}"#,
    ))]);
    let oauth = executor(vec![OAuth2Flow::AuthorizationCode], transport.clone());
    let request = DynamicClientRegistrationRequest {
        client_name: "OpenBIM desktop".into(),
        client_description: OptionalNonNull::Missing,
        client_url: OptionalNonNull::Missing,
        redirect_url: "https://client.example/callback".into(),
    };

    let credentials = oauth.register_client(&request).await.unwrap();
    assert_eq!(credentials.client_id(), "registered-id");
    let debug = format!("{credentials:?}");
    assert!(!debug.contains("registered-secret"));

    let requests = transport.requests();
    assert_eq!(requests[0].0, "https://id.example/register");
    assert!(requests[0].1.contains("OpenBIM desktop"));
    assert!(requests[0].1.contains("redirect_url"));
}

#[tokio::test]
async fn oauth_success_statuses_are_exact() {
    let token_transport = ScriptedTransport::with_responses(vec![Ok(token_response(
        StatusCode::CREATED,
        r#"{"access_token":"access","token_type":"Bearer"}"#,
    ))]);
    let token_oauth = executor(
        vec![OAuth2Flow::ResourceOwnerPasswordCredentials],
        token_transport,
    );
    assert_eq!(
        token_oauth
            .exchange_password(
                "alice",
                &ResourceOwnerPassword::new("password"),
                LegacyPasswordPolicy::Enabled,
                [],
                SystemTime::now(),
            )
            .await
            .unwrap_err(),
        OAuthError::EndpointRejected
    );

    let registration_transport = ScriptedTransport::with_responses(vec![Ok(token_response(
        StatusCode::OK,
        r#"{"client_id":"registered-id","client_secret":"registered-secret"}"#,
    ))]);
    let registration_oauth = executor(vec![OAuth2Flow::AuthorizationCode], registration_transport);
    let metadata = DynamicClientRegistrationRequest {
        client_name: "OpenBIM desktop".into(),
        client_description: OptionalNonNull::Missing,
        client_url: OptionalNonNull::Missing,
        redirect_url: "https://client.example/callback".into(),
    };
    assert_eq!(
        registration_oauth
            .register_client(&metadata)
            .await
            .unwrap_err(),
        OAuthError::InvalidRegistrationResponse
    );
}

#[tokio::test]
async fn callback_is_bound_and_rejects_duplicate_or_extra_parameters() {
    let transport = ScriptedTransport::default();
    let oauth = executor(vec![OAuth2Flow::AuthorizationCode], transport.clone());
    for kind in [
        "duplicate-code",
        "duplicate-state",
        "extra",
        "fragment",
        "malformed-percent",
        "invalid-utf8",
        "origin",
        "path",
    ] {
        let pending = oauth.begin_authorization([]).unwrap();
        let state = pending_state(&pending);
        let callback = match kind {
            "duplicate-code" => {
                format!("https://client.example/callback?code=one&code=two&state={state}")
            }
            "duplicate-state" => {
                format!("https://client.example/callback?code=one&state={state}&state={state}")
            }
            "extra" => {
                format!("https://client.example/callback?code=one&state={state}&scope=admin")
            }
            "fragment" => {
                format!("https://client.example/callback?code=one&state={state}#fragment")
            }
            "malformed-percent" => {
                format!("https://client.example/callback?code=%ZZ&state={state}")
            }
            "invalid-utf8" => {
                format!("https://client.example/callback?code=%FF&state={state}")
            }
            "origin" => format!("https://evil.example/callback?code=one&state={state}"),
            "path" => format!("https://client.example/other?code=one&state={state}"),
            _ => unreachable!(),
        };
        assert!(matches!(
            oauth
                .exchange_authorization_code(pending, &callback, SystemTime::now())
                .await,
            Err(OAuthError::InvalidCallback | OAuthError::RedirectMismatch)
        ));
    }
    assert!(transport.requests().is_empty());
}

#[tokio::test]
async fn callback_error_is_categorical_and_state_checked_first() {
    let transport = ScriptedTransport::default();
    let oauth = executor(vec![OAuth2Flow::AuthorizationCode], transport.clone());
    let pending = oauth.begin_authorization([]).unwrap();
    let state = pending_state(&pending);
    let error = oauth
        .exchange_authorization_code(
            pending,
            &format!("https://client.example/callback?error=server_private_value&state={state}"),
            SystemTime::now(),
        )
        .await
        .unwrap_err();
    assert_eq!(error, OAuthError::EndpointRejected);
    assert!(!format!("{error:?} {error}").contains("server_private_value"));
    assert!(transport.requests().is_empty());
}

#[test]
fn redirect_configuration_rejects_query_fragment_and_userinfo() {
    let discovery = AuthenticationDiscovery {
        oauth2_auth_url: Some("https://id.example/authorize".into()),
        oauth2_token_url: Some("https://id.example/token".into()),
        oauth2_dynamic_client_reg_url: None,
        http_basic_supported: Some(true),
        supported_oauth2_flows: Some(vec![OAuth2Flow::AuthorizationCode]),
    };
    let config = OAuthConfig::from_discovery(discovery).unwrap();
    for redirect in [
        "https://client.example/callback?code=polluted",
        "https://client.example/callback#fragment",
        "https://attacker@client.example/callback",
    ] {
        assert!(matches!(
            OAuthExecutor::new(
                config.clone(),
                ClientCredentials::new("client", None),
                redirect,
                ScriptedTransport::default(),
            ),
            Err(OAuthError::InvalidConfiguration(_))
        ));
    }
}

#[tokio::test]
async fn token_form_has_each_security_parameter_exactly_once() {
    let transport = ScriptedTransport::with_responses(vec![Ok(token_response(
        StatusCode::OK,
        r#"{"access_token":"access","token_type":"Bearer"}"#,
    ))]);
    let oauth = executor(
        vec![OAuth2Flow::ResourceOwnerPasswordCredentials],
        transport.clone(),
    );
    oauth
        .exchange_password(
            "alice",
            &ResourceOwnerPassword::new("password"),
            LegacyPasswordPolicy::Enabled,
            ["documents.read"],
            SystemTime::now(),
        )
        .await
        .unwrap();
    let requests = transport.requests();
    assert_eq!(requests[0].0, "https://id.example/token");
    let pairs = form_pairs(&requests[0].1);
    for required in ["grant_type", "username", "password", "scope"] {
        assert_eq!(pairs.iter().filter(|(name, _)| name == required).count(), 1);
    }
    assert!(!pairs
        .iter()
        .any(|(name, _)| matches!(name.as_str(), "client_id" | "client_secret")));
    assert!(transport.has_authorization_header("Basic Y2xpZW50LWlkOmNsaWVudC1zZWNyZXQ="));
    assert_eq!(
        pairs
            .iter()
            .find(|(name, _)| name == "grant_type")
            .unwrap()
            .1,
        "password"
    );

    let body_transport = ScriptedTransport::with_responses(vec![Ok(token_response(
        StatusCode::OK,
        r#"{"access_token":"access","token_type":"Bearer"}"#,
    ))]);
    let body_oauth = executor_with_basic(
        vec![OAuth2Flow::ResourceOwnerPasswordCredentials],
        false,
        body_transport.clone(),
    );
    body_oauth
        .exchange_password(
            "alice",
            &ResourceOwnerPassword::new("password"),
            LegacyPasswordPolicy::Enabled,
            [],
            SystemTime::now(),
        )
        .await
        .unwrap();
    let body_requests = body_transport.requests();
    let body_pairs = form_pairs(&body_requests[0].1);
    for required in ["client_id", "client_secret"] {
        assert_eq!(
            body_pairs
                .iter()
                .filter(|(name, _)| name == required)
                .count(),
            1
        );
    }
}

#[tokio::test]
async fn duplicate_token_fields_and_secret_server_errors_are_categorical() {
    for (status, body) in [
        (
            StatusCode::OK,
            r#"{"access_token":"first-secret","access_token":"second-secret","token_type":"Bearer"}"#,
        ),
        (
            StatusCode::BAD_REQUEST,
            r#"{"error":"server-secret","error_description":"password=needle"}"#,
        ),
    ] {
        let transport = ScriptedTransport::with_responses(vec![Ok(token_response(status, body))]);
        let oauth = executor(
            vec![OAuth2Flow::ResourceOwnerPasswordCredentials],
            transport,
        );
        let error = oauth
            .exchange_password(
                "alice",
                &ResourceOwnerPassword::new("password"),
                LegacyPasswordPolicy::Enabled,
                [],
                SystemTime::now(),
            )
            .await
            .unwrap_err();
        let diagnostics = format!("{error:?} {error}");
        for needle in [
            "first-secret",
            "second-secret",
            "server-secret",
            "password=needle",
        ] {
            assert!(!diagnostics.contains(needle));
        }
    }
}

#[tokio::test]
async fn token_response_rejects_wrong_content_type_and_expiry_overflow() {
    let wrong_type = OAuthHttpResponse::from_http(
        Response::builder()
            .status(StatusCode::OK)
            .header(CONTENT_TYPE, "text/plain")
            .body(SensitiveBytes::new(
                br#"{"access_token":"secret","token_type":"Bearer"}"#.to_vec(),
            ))
            .unwrap(),
    );
    for response in [
        wrong_type,
        token_response(
            StatusCode::OK,
            r#"{"access_token":"secret","token_type":"Bearer","expires_in":18446744073709551615}"#,
        ),
    ] {
        let transport = ScriptedTransport::with_responses(vec![Ok(response)]);
        let oauth = executor(
            vec![OAuth2Flow::ResourceOwnerPasswordCredentials],
            transport,
        );
        assert_eq!(
            oauth
                .exchange_password(
                    "alice",
                    &ResourceOwnerPassword::new("password"),
                    LegacyPasswordPolicy::Enabled,
                    [],
                    SystemTime::now(),
                )
                .await
                .unwrap_err(),
            OAuthError::InvalidTokenResponse
        );
    }
}

#[tokio::test]
async fn refresh_must_rotate_to_a_distinct_value() {
    let transport = ScriptedTransport::with_responses(vec![Ok(token_response(
        StatusCode::OK,
        r#"{"access_token":"new","token_type":"Bearer","refresh_token":"same"}"#,
    ))]);
    let oauth = executor(vec![OAuth2Flow::AuthorizationCode], transport);
    let store = MemoryTokenStore::new();
    let key = TokenKey::new("rotation").unwrap();
    store
        .compare_and_swap(
            &key,
            None,
            TokenSet::new(
                AccessToken::new("old"),
                Some(RefreshToken::new("same")),
                None,
            ),
        )
        .await
        .unwrap();
    assert_eq!(
        oauth
            .refresh_stored(&store, &key, SystemTime::now())
            .await
            .unwrap_err(),
        OAuthError::MissingRotatedRefreshToken
    );
    assert_eq!(store.load(&key).await.unwrap().unwrap().generation(), 1);
}

#[tokio::test]
async fn registration_rejects_redirect_mismatch_and_duplicate_secret() {
    let response = token_response(
        StatusCode::CREATED,
        r#"{"client_id":"id","client_secret":"one","client_secret":"two"}"#,
    );
    let transport = ScriptedTransport::with_responses(vec![Ok(response)]);
    let oauth = executor(vec![OAuth2Flow::AuthorizationCode], transport.clone());
    let mismatched = DynamicClientRegistrationRequest {
        client_name: "client".into(),
        client_description: OptionalNonNull::Missing,
        client_url: OptionalNonNull::Missing,
        redirect_url: "https://client.example/other".into(),
    };
    assert!(matches!(
        oauth.register_client(&mismatched).await,
        Err(OAuthError::InvalidConfiguration(_))
    ));
    assert!(transport.requests().is_empty());

    let matching = DynamicClientRegistrationRequest {
        redirect_url: "https://client.example/callback".into(),
        ..mismatched
    };
    assert_eq!(
        oauth.register_client(&matching).await.unwrap_err(),
        OAuthError::InvalidRegistrationResponse
    );
}

#[tokio::test]
async fn concurrent_refresh_is_rejected_before_reusing_one_time_token() {
    let (transport, started, release) =
        ScriptedTransport::with_blocked_response(Ok(token_response(
            StatusCode::OK,
            r#"{"access_token":"new-access","token_type":"Bearer","refresh_token":"new-refresh"}"#,
        )));
    let oauth = executor(vec![OAuth2Flow::AuthorizationCode], transport.clone());
    let store = MemoryTokenStore::new();
    let key = TokenKey::new("concurrent-refresh").unwrap();
    store
        .compare_and_swap(
            &key,
            None,
            TokenSet::new(
                AccessToken::new("old-access"),
                Some(RefreshToken::new("old-refresh")),
                None,
            ),
        )
        .await
        .unwrap();

    let first_oauth = oauth.clone();
    let first_store = store.clone();
    let first_key = key.clone();
    let first = tokio::spawn(async move {
        first_oauth
            .refresh_stored(&first_store, &first_key, SystemTime::now())
            .await
    });
    started.notified().await;

    assert_eq!(
        oauth
            .refresh_stored(&store, &key, SystemTime::now())
            .await
            .unwrap_err(),
        OAuthError::RefreshInProgress
    );
    assert_eq!(transport.requests().len(), 1);

    release.notify_one();
    first.await.unwrap().unwrap();
    assert_eq!(store.load(&key).await.unwrap().unwrap().generation(), 2);
}

#[tokio::test]
async fn scope_callback_and_response_size_limits_fail_before_unsafe_use() {
    let transport = ScriptedTransport::default();
    let oauth = executor(
        vec![
            OAuth2Flow::AuthorizationCode,
            OAuth2Flow::ResourceOwnerPasswordCredentials,
        ],
        transport.clone(),
    );
    assert!(matches!(
        oauth.begin_authorization(["documents.read admin"]),
        Err(OAuthError::InvalidConfiguration(_))
    ));
    assert!(matches!(
        oauth
            .exchange_password(
                "alice",
                &ResourceOwnerPassword::new("password"),
                LegacyPasswordPolicy::Enabled,
                ["bad\\scope"],
                SystemTime::now(),
            )
            .await,
        Err(OAuthError::InvalidConfiguration(_))
    ));
    let pending = oauth.begin_authorization([]).unwrap();
    let state = pending_state(&pending);
    let oversized_callback = format!(
        "https://client.example/callback?code={}&state={state}",
        "x".repeat(17 * 1024)
    );
    assert_eq!(
        oauth
            .exchange_authorization_code(pending, &oversized_callback, SystemTime::now())
            .await
            .unwrap_err(),
        OAuthError::InvalidCallback
    );
    assert!(transport.requests().is_empty());

    let oversized = OAuthHttpResponse::from_http(
        Response::builder()
            .status(StatusCode::OK)
            .header(CONTENT_TYPE, "application/json")
            .body(SensitiveBytes::new(vec![b'x'; 1024 * 1024 + 1]))
            .unwrap(),
    );
    let transport = ScriptedTransport::with_responses(vec![Ok(oversized)]);
    let oauth = executor(
        vec![OAuth2Flow::ResourceOwnerPasswordCredentials],
        transport,
    );
    assert_eq!(
        oauth
            .exchange_password(
                "alice",
                &ResourceOwnerPassword::new("password"),
                LegacyPasswordPolicy::Enabled,
                [],
                SystemTime::now(),
            )
            .await
            .unwrap_err(),
        OAuthError::InvalidTokenResponse
    );
}

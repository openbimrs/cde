#![allow(missing_docs)]

use async_trait::async_trait;
use openbim_cde::foundation::{
    AuthenticationDiscovery, ClientSecret, DynamicClientRegistrationRequest,
    DynamicClientRegistrationResponse, User, VersionsResponse,
};
use openbim_cde_http::{
    foundation_router, AllowAnonymous, FoundationAuthPolicies, FoundationRoutes, FoundationService,
    HttpClientBuilder, RequestContext, ServerLimits, ServiceError,
};
use std::sync::Arc;

struct Foundation;

#[async_trait]
impl FoundationService for Foundation {
    async fn versions(&self, _: RequestContext) -> Result<VersionsResponse, ServiceError> {
        Ok(VersionsResponse {
            versions: Vec::new(),
        })
    }

    async fn authentication(
        &self,
        _: RequestContext,
    ) -> Result<AuthenticationDiscovery, ServiceError> {
        Ok(AuthenticationDiscovery {
            oauth2_auth_url: None,
            oauth2_token_url: None,
            oauth2_dynamic_client_reg_url: None,
            http_basic_supported: Some(false),
            supported_oauth2_flows: None,
        })
    }

    async fn register_client(
        &self,
        context: RequestContext,
        request: DynamicClientRegistrationRequest,
    ) -> Result<DynamicClientRegistrationResponse, ServiceError> {
        assert_eq!(context.uri.path(), "/oauth/register");
        assert_eq!(request.client_name, "integration-client");
        Ok(DynamicClientRegistrationResponse {
            client_id: "integration-id".into(),
            client_secret: ClientSecret::new("integration-secret"),
        })
    }

    async fn current_user(&self, _: RequestContext) -> Result<User, ServiceError> {
        Ok(User {
            id: "user".into(),
            name: None,
        })
    }
}

#[tokio::test]
async fn client_and_server_agree_on_foundation_status_and_bodies() {
    let app = foundation_router(
        Arc::new(Foundation),
        FoundationAuthPolicies::public_discovery(Arc::new(AllowAnonymous)),
        FoundationRoutes::new("/foundation/1.1", "/oauth/register").unwrap(),
        ServerLimits::default(),
    )
    .unwrap();
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    let server = tokio::spawn(async move { axum::serve(listener, app).await.unwrap() });
    let base = format!("http://{address}");
    let client = HttpClientBuilder::new(
        &base,
        format!("{base}/foundation/1.1"),
        format!("{base}/documents/1.0"),
    )
    .unwrap()
    .allow_http_for_local_development()
    .build()
    .unwrap();

    assert!(client.versions().await.unwrap().versions.is_empty());
    let request: DynamicClientRegistrationRequest = serde_json::from_value(serde_json::json!({
        "client_name": "integration-client",
        "redirect_url": "https://client.example/callback"
    }))
    .unwrap();
    let registered = client
        .register_client(&format!("{base}/oauth/register"), &request)
        .await
        .unwrap();
    assert_eq!(registered.client_id, "integration-id");
    assert_eq!(
        registered.client_secret.expose_secret(),
        "integration-secret"
    );
    server.abort();
}

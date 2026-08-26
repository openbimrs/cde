//! Secure OAuth execution for buildingSMART openCDE APIs.

mod discovery;
#[cfg(feature = "native-keyring")]
mod native_keyring;
mod oauth;
#[cfg(feature = "reqwest")]
mod reqwest_transport;
mod secrets;
mod store;

pub use discovery::{DiscoveryError, EndpointSecurityPolicy, OAuthConfig};
#[cfg(feature = "native-keyring")]
pub use native_keyring::NativeKeyringTokenStore;
pub use oauth::{
    ClientCredentials, LegacyPasswordPolicy, OAuthError, OAuthExecutor, OAuthHttpRequest,
    OAuthHttpResponse, OAuthHttpTransport, PendingAuthorization, SensitiveBytes, TransportError,
};
#[cfg(feature = "reqwest")]
pub use reqwest_transport::ReqwestOAuthTransport;
pub use secrets::{
    AccessToken, BearerToken, ClientSecret, RefreshToken, ResourceOwnerPassword, TokenSet,
};
pub use store::{MemoryTokenStore, StoreError, StoredToken, TokenKey, TokenStore};

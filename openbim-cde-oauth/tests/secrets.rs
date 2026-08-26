use openbim_cde_oauth::{
    AccessToken, ClientSecret, RefreshToken, ResourceOwnerPassword, SensitiveBytes, TokenSet,
};
use std::time::{Duration, SystemTime};
use zeroize::{Zeroize, ZeroizeOnDrop};

fn assert_zeroize_on_drop<T: ZeroizeOnDrop>() {}

#[test]
fn all_secret_debug_output_is_redacted() {
    let access = AccessToken::new("access-needle");
    let refresh = RefreshToken::new("refresh-needle");
    let client = ClientSecret::new("client-needle");
    let password = ResourceOwnerPassword::new("password-needle");
    let token = TokenSet::new(access.clone(), Some(refresh.clone()), None);

    for (debug, needle) in [
        (format!("{access:?}"), "access-needle"),
        (format!("{refresh:?}"), "refresh-needle"),
        (format!("{client:?}"), "client-needle"),
        (format!("{password:?}"), "password-needle"),
        (format!("{token:?}"), "access-needle"),
    ] {
        assert!(!debug.contains(needle));
        assert!(debug.contains("REDACTED"));
    }
}

#[test]
fn secret_owners_have_compile_time_and_eager_zeroization() {
    assert_zeroize_on_drop::<AccessToken>();
    assert_zeroize_on_drop::<RefreshToken>();
    assert_zeroize_on_drop::<ClientSecret>();
    assert_zeroize_on_drop::<ResourceOwnerPassword>();
    assert_zeroize_on_drop::<SensitiveBytes>();

    let mut bytes = SensitiveBytes::new(b"request-secret".to_vec());
    bytes.zeroize();
    assert!(bytes.is_empty());
    assert_eq!(format!("{bytes:?}"), "SensitiveBytes([REDACTED])");
}

#[test]
fn token_expiry_and_bearer_exposure_are_explicit() {
    let now = SystemTime::UNIX_EPOCH + Duration::from_secs(100);
    let token = TokenSet::new(
        AccessToken::new("bearer-value"),
        Some(RefreshToken::new("single-use")),
        Some(now + Duration::from_secs(60)),
    );

    assert!(!token.is_expired_at(now + Duration::from_secs(59)));
    assert!(token.is_expired_at(now + Duration::from_secs(60)));
    assert_eq!(token.bearer_token().expose(), "bearer-value");
    assert!(token.refresh_is_one_time());
}

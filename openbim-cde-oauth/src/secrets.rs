use secrecy::{ExposeSecret, ExposeSecretMut, SecretString};
use std::{fmt, time::SystemTime};
use zeroize::{Zeroize, ZeroizeOnDrop};

macro_rules! secret_type {
    ($name:ident, $doc:literal) => {
        #[doc = $doc]
        #[derive(Clone)]
        pub struct $name(SecretString);

        impl $name {
            /// Wrap a secret value. The allocation is zeroized when dropped.
            #[must_use]
            pub fn new(value: impl Into<String>) -> Self {
                Self(SecretString::from(value.into()))
            }

            pub(crate) fn expose(&self) -> &str {
                self.0.expose_secret()
            }
        }

        impl Zeroize for $name {
            fn zeroize(&mut self) {
                self.0.expose_secret_mut().zeroize();
            }
        }

        impl Drop for $name {
            fn drop(&mut self) {
                self.zeroize();
            }
        }

        impl ZeroizeOnDrop for $name {}

        impl fmt::Debug for $name {
            fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
                write!(formatter, "{}([REDACTED])", stringify!($name))
            }
        }
    };
}

secret_type!(
    AccessToken,
    "An OAuth access token with redacted diagnostics and zeroizing storage."
);
secret_type!(RefreshToken, "A one-time Foundation OAuth refresh token.");
secret_type!(ClientSecret, "An OAuth confidential-client secret.");
secret_type!(ResourceOwnerPassword, "A legacy resource-owner password.");

/// Explicit, borrow-only access to a bearer token for an Authorization header.
///
/// This type intentionally implements neither `Display` nor `Debug` with the
/// secret value. Call [`BearerToken::expose`] only at the HTTP authorization
/// boundary.
pub struct BearerToken<'a>(&'a str);

impl<'a> BearerToken<'a> {
    /// Expose the bearer token for immediate use in an Authorization header.
    #[must_use]
    pub fn expose(&self) -> &'a str {
        self.0
    }
}

impl fmt::Debug for BearerToken<'_> {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("BearerToken([REDACTED])")
    }
}

/// A token response together with its absolute expiry.
#[derive(Clone)]
pub struct TokenSet {
    access_token: AccessToken,
    refresh_token: Option<RefreshToken>,
    expires_at: Option<SystemTime>,
}

impl TokenSet {
    /// Construct a token set. Foundation refresh tokens are always treated as
    /// one-time credentials and must be rotated after successful use.
    #[must_use]
    pub fn new(
        access_token: AccessToken,
        refresh_token: Option<RefreshToken>,
        expires_at: Option<SystemTime>,
    ) -> Self {
        Self {
            access_token,
            refresh_token,
            expires_at,
        }
    }

    /// Borrow the access token through an explicit-use wrapper.
    #[must_use]
    pub fn bearer_token(&self) -> BearerToken<'_> {
        BearerToken(self.access_token.expose())
    }

    /// Return the absolute expiry instant, if supplied by the authorization server.
    #[must_use]
    pub fn expires_at(&self) -> Option<SystemTime> {
        self.expires_at
    }

    /// Whether the access token is expired at `now`. Tokens without an expiry
    /// are conservatively reported as not known to be expired.
    #[must_use]
    pub fn is_expired_at(&self, now: SystemTime) -> bool {
        self.expires_at.is_some_and(|expiry| now >= expiry)
    }

    /// Whether this token set contains a one-time Foundation refresh token.
    #[must_use]
    pub fn refresh_is_one_time(&self) -> bool {
        self.refresh_token.is_some()
    }

    pub(crate) fn refresh_token(&self) -> Option<&RefreshToken> {
        self.refresh_token.as_ref()
    }
}

impl fmt::Debug for TokenSet {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("TokenSet")
            .field("access_token", &self.access_token)
            .field("refresh_token", &self.refresh_token)
            .field("expires_at", &self.expires_at)
            .finish()
    }
}

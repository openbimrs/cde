use crate::{AccessToken, RefreshToken, StoreError, StoredToken, TokenKey, TokenSet, TokenStore};
use async_trait::async_trait;
use base64::{engine::general_purpose::URL_SAFE_NO_PAD, Engine as _};
use keyring::{Entry, Error as KeyringError};
use serde::{
    de::{Deserializer, Visitor},
    Deserialize, Serialize,
};
use std::{
    fmt,
    sync::{Arc, Mutex, MutexGuard},
    time::{Duration, SystemTime},
};
use zeroize::Zeroizing;

/// Native OS credential-store persistence.
///
/// This implementation never writes a plaintext file. CAS operations are
/// atomic for clones of one store instance in this process. Native keyring APIs
/// do not provide cross-process transactions, so callers must not share a key
/// across independent processes that refresh concurrently.
#[derive(Clone)]
pub struct NativeKeyringTokenStore {
    service: Arc<str>,
    operation_lock: Arc<Mutex<()>>,
}

impl NativeKeyringTokenStore {
    /// Creates a store using `service` as the native credential-store service.
    pub fn new(service: impl Into<String>) -> Result<Self, StoreError> {
        let service = service.into();
        if service.trim().is_empty() || service.len() > 255 || service.chars().any(char::is_control)
        {
            return Err(StoreError::InvalidKey);
        }
        Ok(Self {
            service: Arc::from(service),
            operation_lock: Arc::new(Mutex::new(())),
        })
    }

    fn lock(&self) -> Result<MutexGuard<'_, ()>, StoreError> {
        self.operation_lock
            .lock()
            .map_err(|_| StoreError::LockPoisoned)
    }

    fn entry(&self, key: &TokenKey) -> Result<Entry, StoreError> {
        Entry::new(&self.service, key.as_str()).map_err(sanitize_backend_error)
    }

    fn load_unlocked(&self, key: &TokenKey) -> Result<Option<StoredToken>, StoreError> {
        let entry = self.entry(key)?;
        match entry.get_password() {
            Ok(payload) => {
                let payload = Zeroizing::new(payload);
                decode_stored(payload.as_bytes()).map(Some)
            }
            Err(KeyringError::NoEntry) => Ok(None),
            Err(error) => Err(sanitize_backend_error(error)),
        }
    }
}

#[async_trait]
impl TokenStore for NativeKeyringTokenStore {
    async fn load(&self, key: &TokenKey) -> Result<Option<StoredToken>, StoreError> {
        let _guard = self.lock()?;
        self.load_unlocked(key)
    }

    async fn compare_and_swap(
        &self,
        key: &TokenKey,
        expected: Option<u64>,
        replacement: TokenSet,
    ) -> Result<StoredToken, StoreError> {
        let _guard = self.lock()?;
        let actual_stored = self.load_unlocked(key)?;
        let actual = actual_stored.as_ref().map(StoredToken::generation);
        if actual != expected {
            return Err(StoreError::GenerationConflict { expected, actual });
        }
        let generation = actual
            .unwrap_or(0)
            .checked_add(1)
            .ok_or(StoreError::CorruptEntry)?;
        let stored = StoredToken::new(generation, replacement);
        let payload = encode_stored(&stored)?;
        self.entry(key)?
            .set_password(payload.as_str())
            .map_err(sanitize_backend_error)?;
        Ok(stored)
    }

    async fn delete(&self, key: &TokenKey, expected: u64) -> Result<(), StoreError> {
        let _guard = self.lock()?;
        let actual = self
            .load_unlocked(key)?
            .as_ref()
            .map(StoredToken::generation);
        if actual != Some(expected) {
            return Err(StoreError::GenerationConflict {
                expected: Some(expected),
                actual,
            });
        }
        self.entry(key)?
            .delete_credential()
            .map_err(sanitize_backend_error)
    }
}

const MAX_PERSISTED_TOKEN_BYTES: usize = 1024 * 1024;

#[derive(Deserialize)]
struct PersistedToken {
    version: u8,
    generation: u64,
    access_token: PersistedSecret,
    refresh_token: Option<PersistedSecret>,
    expires_at_seconds: Option<u64>,
    expires_at_nanos: Option<u32>,
}

struct PersistedSecret(Zeroizing<String>);

fn decode_base64_secret_into(
    input: &[u8],
    decoded: &mut Zeroizing<Vec<u8>>,
) -> Result<(), StoreError> {
    URL_SAFE_NO_PAD
        .decode_vec(input, decoded)
        .map_err(|_| StoreError::CorruptEntry)
}

fn decode_utf8_secret(bytes: Vec<u8>) -> Result<Zeroizing<String>, StoreError> {
    match String::from_utf8(bytes) {
        Ok(value) => Ok(Zeroizing::new(value)),
        Err(error) => {
            let _invalid = Zeroizing::new(error.into_bytes());
            Err(StoreError::CorruptEntry)
        }
    }
}

impl PersistedSecret {
    fn decode(&self) -> Result<Zeroizing<String>, StoreError> {
        let mut decoded = Zeroizing::new(Vec::with_capacity(self.0.len()));
        decode_base64_secret_into(self.0.as_bytes(), &mut decoded)?;
        let bytes = std::mem::take(&mut *decoded);
        decode_utf8_secret(bytes)
    }
}

impl<'de> Deserialize<'de> for PersistedSecret {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        struct SecretVisitor;

        impl Visitor<'_> for SecretVisitor {
            type Value = PersistedSecret;

            fn expecting(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
                formatter.write_str("an unescaped credential string")
            }

            fn visit_borrowed_str<E>(self, value: &str) -> Result<Self::Value, E> {
                Ok(PersistedSecret(Zeroizing::new(value.to_owned())))
            }

            fn visit_str<E>(self, _value: &str) -> Result<Self::Value, E>
            where
                E: serde::de::Error,
            {
                Err(E::custom(
                    "copied persisted credential strings are not supported",
                ))
            }

            fn visit_string<E>(self, value: String) -> Result<Self::Value, E> {
                Ok(PersistedSecret(Zeroizing::new(value)))
            }
        }

        deserializer.deserialize_string(SecretVisitor)
    }
}

#[derive(Serialize)]
struct PersistedTokenRef<'a> {
    version: u8,
    generation: u64,
    access_token: &'a str,
    refresh_token: Option<&'a str>,
    expires_at_seconds: Option<u64>,
    expires_at_nanos: Option<u32>,
}

const PERSISTED_JSON_MAX_OVERHEAD: usize = 256;

#[cfg(test)]
std::thread_local! {
    static SECRET_ENCODING_CALLS: std::cell::Cell<usize> = const { std::cell::Cell::new(0) };
}

fn encode_persisted_secret(value: &str) -> Zeroizing<String> {
    #[cfg(test)]
    SECRET_ENCODING_CALLS.with(|calls| calls.set(calls.get() + 1));
    Zeroizing::new(URL_SAFE_NO_PAD.encode(value))
}

fn base64_no_pad_len(bytes: usize) -> Option<usize> {
    let complete = bytes.checked_div(3)?.checked_mul(4)?;
    complete.checked_add(match bytes % 3 {
        0 => 0,
        1 => 2,
        _ => 3,
    })
}

fn preflight_persisted_token(stored: &StoredToken) -> Result<(), StoreError> {
    let mut size = PERSISTED_JSON_MAX_OVERHEAD;
    let access = stored.token().bearer_token().expose().len();
    size = size
        .checked_add(base64_no_pad_len(access).ok_or(StoreError::CorruptEntry)?)
        .ok_or(StoreError::CorruptEntry)?;
    if let Some(refresh) = stored.token().refresh_token() {
        size = size
            .checked_add(base64_no_pad_len(refresh.expose().len()).ok_or(StoreError::CorruptEntry)?)
            .ok_or(StoreError::CorruptEntry)?;
    }
    if size > MAX_PERSISTED_TOKEN_BYTES {
        Err(StoreError::CorruptEntry)
    } else {
        Ok(())
    }
}

fn encode_stored(stored: &StoredToken) -> Result<Zeroizing<String>, StoreError> {
    preflight_persisted_token(stored)?;
    let (expires_at_seconds, expires_at_nanos) = match stored.token().expires_at() {
        Some(expires_at) => {
            let duration = expires_at
                .duration_since(SystemTime::UNIX_EPOCH)
                .map_err(|_| StoreError::CorruptEntry)?;
            (Some(duration.as_secs()), Some(duration.subsec_nanos()))
        }
        None => (None, None),
    };
    let access_token = encode_persisted_secret(stored.token().bearer_token().expose());
    let refresh_token = stored
        .token()
        .refresh_token()
        .map(|token| encode_persisted_secret(token.expose()));
    let wire = PersistedTokenRef {
        version: 1,
        generation: stored.generation(),
        access_token: &access_token,
        refresh_token: refresh_token.as_deref().map(String::as_str),
        expires_at_seconds,
        expires_at_nanos,
    };
    let encoded = serde_json::to_string(&wire)
        .map(Zeroizing::new)
        .map_err(|_| StoreError::CorruptEntry)?;
    if encoded.len() > MAX_PERSISTED_TOKEN_BYTES {
        Err(StoreError::CorruptEntry)
    } else {
        Ok(encoded)
    }
}

fn decode_stored(payload: &[u8]) -> Result<StoredToken, StoreError> {
    // Escaped JSON makes serde_json allocate an ordinary scratch String before
    // handing the decoded value to our zeroizing field visitor.
    if payload.len() > MAX_PERSISTED_TOKEN_BYTES || payload.contains(&b'\\') {
        return Err(StoreError::CorruptEntry);
    }
    let wire: PersistedToken =
        serde_json::from_slice(payload).map_err(|_| StoreError::CorruptEntry)?;
    if wire.version != 1 || wire.generation == 0 || wire.access_token.0.is_empty() {
        return Err(StoreError::CorruptEntry);
    }
    let expires_at = match (wire.expires_at_seconds, wire.expires_at_nanos) {
        (None, None) => None,
        (Some(seconds), Some(nanos)) if nanos < 1_000_000_000 => SystemTime::UNIX_EPOCH
            .checked_add(Duration::new(seconds, nanos))
            .ok_or(StoreError::CorruptEntry)
            .map(Some)?,
        _ => return Err(StoreError::CorruptEntry),
    };
    let mut access_value = wire.access_token.decode()?;
    if access_value.is_empty() {
        return Err(StoreError::CorruptEntry);
    }
    let access_token = AccessToken::new(std::mem::take(&mut *access_value));
    let refresh_token = wire
        .refresh_token
        .as_ref()
        .map(PersistedSecret::decode)
        .transpose()?
        .map(|mut value| RefreshToken::new(std::mem::take(&mut *value)));
    Ok(StoredToken::new(
        wire.generation,
        TokenSet::new(access_token, refresh_token, expires_at),
    ))
}

fn sanitize_backend_error(error: KeyringError) -> StoreError {
    let category = match error {
        KeyringError::NoEntry => "entry not found",
        KeyringError::Ambiguous(_) => "ambiguous credential entry",
        KeyringError::NoStorageAccess(_) => "credential store unavailable",
        KeyringError::PlatformFailure(_) => "native credential operation failed",
        KeyringError::BadEncoding(_) => "credential store returned invalid encoding",
        KeyringError::TooLong(_, _) => "credential exceeds native store limits",
        KeyringError::Invalid(_, _) => "invalid native credential attributes",
        _ => "native credential operation failed",
    };
    StoreError::Backend(category)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn credential_payload_round_trips_without_serde_on_public_secret_types() {
        let stored = StoredToken::new(
            7,
            TokenSet::new(
                AccessToken::new("access\"line\n"),
                Some(RefreshToken::new("refresh\\line")),
                Some(SystemTime::UNIX_EPOCH + Duration::new(42, 3)),
            ),
        );
        let payload = encode_stored(&stored).unwrap();
        let decoded = decode_stored(payload.as_bytes()).unwrap();
        assert_eq!(decoded.generation(), 7);
        assert_eq!(decoded.token().bearer_token().expose(), "access\"line\n");
        assert_eq!(
            decoded.token().refresh_token().map(RefreshToken::expose),
            Some("refresh\\line")
        );
        assert_eq!(
            decoded.token().expires_at(),
            Some(SystemTime::UNIX_EPOCH + Duration::new(42, 3))
        );
    }

    #[test]
    fn credential_payload_rejects_escaped_secret_json() {
        let payload = br#"{"version":1,"generation":1,"access_token":"Y\u0051","refresh_token":null,"expires_at_seconds":null,"expires_at_nanos":null}"#;
        assert!(matches!(
            decode_stored(payload),
            Err(StoreError::CorruptEntry)
        ));
    }

    #[test]
    fn malformed_base64_after_a_valid_secret_prefix_is_rejected() {
        type ZeroizingDecoder = fn(&[u8], &mut Zeroizing<Vec<u8>>) -> Result<(), StoreError>;
        let _typed_decode: ZeroizingDecoder = decode_base64_secret_into;
        let mut partial = Zeroizing::new(Vec::new());
        assert!(matches!(
            decode_base64_secret_into(b"c2VjcmV0c2VjcmV0c2VjcmV0!!", &mut partial),
            Err(StoreError::CorruptEntry)
        ));
        assert!(partial.starts_with(b"secret"));

        let payload = br#"{"version":1,"generation":1,"access_token":"c2VjcmV0!","refresh_token":null,"expires_at_seconds":null,"expires_at_nanos":null}"#;
        assert!(matches!(
            decode_stored(payload),
            Err(StoreError::CorruptEntry)
        ));
    }

    #[test]
    fn persisted_size_is_rejected_before_base64_or_json_encoding() {
        let oversized = "x".repeat(MAX_PERSISTED_TOKEN_BYTES);
        let stored = StoredToken::new(1, TokenSet::new(AccessToken::new(oversized), None, None));
        SECRET_ENCODING_CALLS.with(|calls| calls.set(0));
        assert!(matches!(
            preflight_persisted_token(&stored),
            Err(StoreError::CorruptEntry)
        ));
        assert!(matches!(
            encode_stored(&stored),
            Err(StoreError::CorruptEntry)
        ));
        SECRET_ENCODING_CALLS.with(|calls| assert_eq!(calls.get(), 0));
    }
}

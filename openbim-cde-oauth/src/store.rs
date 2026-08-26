use crate::TokenSet;
use async_trait::async_trait;
use std::{
    collections::HashMap,
    error::Error,
    fmt,
    sync::{Arc, Mutex},
};

/// Stable, non-secret application key used to locate one token set.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct TokenKey(String);

impl TokenKey {
    /// Construct a non-empty token key.
    pub fn new(value: impl Into<String>) -> Result<Self, StoreError> {
        let value = value.into();
        if value.trim().is_empty() || value.len() > 255 || value.chars().any(char::is_control) {
            return Err(StoreError::InvalidKey);
        }
        Ok(Self(value))
    }

    /// Borrow the key for a native credential-store account name.
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

/// A token set read from a store with its optimistic-concurrency generation.
#[derive(Clone, Debug)]
pub struct StoredToken {
    generation: u64,
    token: TokenSet,
}

impl StoredToken {
    pub(crate) fn new(generation: u64, token: TokenSet) -> Self {
        Self { generation, token }
    }

    /// Monotonically increasing generation, starting at one.
    #[must_use]
    pub fn generation(&self) -> u64 {
        self.generation
    }

    /// Borrow the redaction-safe token model.
    #[must_use]
    pub fn token(&self) -> &TokenSet {
        &self.token
    }

    /// Consume the stored wrapper.
    #[must_use]
    pub fn into_token(self) -> TokenSet {
        self.token
    }
}

/// Token-store failure. Diagnostics never include credential values.
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub enum StoreError {
    /// The key was empty.
    InvalidKey,
    /// The expected generation no longer matches the stored generation.
    GenerationConflict {
        /// Generation observed by the caller, or `None` for create-only.
        expected: Option<u64>,
        /// Generation currently stored, or `None` when absent.
        actual: Option<u64>,
    },
    /// A synchronization primitive was poisoned.
    LockPoisoned,
    /// A native credential-store operation failed. The message is sanitized.
    Backend(&'static str),
    /// Persisted credential data was malformed or outside supported ranges.
    CorruptEntry,
}

impl fmt::Display for StoreError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidKey => formatter.write_str("token key must not be empty"),
            Self::GenerationConflict { expected, actual } => {
                write!(
                    formatter,
                    "token generation conflict: expected {expected:?}, actual {actual:?}"
                )
            }
            Self::LockPoisoned => formatter.write_str("token store lock was poisoned"),
            Self::Backend(message) => write!(formatter, "credential store error: {message}"),
            Self::CorruptEntry => formatter.write_str("credential store entry is corrupt"),
        }
    }
}

impl Error for StoreError {}

/// Async optimistic-concurrency token persistence.
///
/// `compare_and_swap` is the only mutation operation. `expected = None` is a
/// create-only operation; `Some(generation)` updates exactly that generation.
#[async_trait]
pub trait TokenStore: Send + Sync {
    /// Load the current token and generation.
    async fn load(&self, key: &TokenKey) -> Result<Option<StoredToken>, StoreError>;

    /// Atomically replace a token only if the generation matches.
    async fn compare_and_swap(
        &self,
        key: &TokenKey,
        expected: Option<u64>,
        replacement: TokenSet,
    ) -> Result<StoredToken, StoreError>;

    /// Delete a token only if the generation matches.
    async fn delete(&self, key: &TokenKey, expected: u64) -> Result<(), StoreError>;
}

/// Process-local token store for tests and ephemeral services.
#[derive(Clone, Default)]
pub struct MemoryTokenStore {
    entries: Arc<Mutex<HashMap<TokenKey, StoredToken>>>,
}

impl MemoryTokenStore {
    /// Construct an empty in-memory store.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }
}

#[async_trait]
impl TokenStore for MemoryTokenStore {
    async fn load(&self, key: &TokenKey) -> Result<Option<StoredToken>, StoreError> {
        self.entries
            .lock()
            .map_err(|_| StoreError::LockPoisoned)
            .map(|entries| entries.get(key).cloned())
    }

    async fn compare_and_swap(
        &self,
        key: &TokenKey,
        expected: Option<u64>,
        replacement: TokenSet,
    ) -> Result<StoredToken, StoreError> {
        let mut entries = self.entries.lock().map_err(|_| StoreError::LockPoisoned)?;
        let actual = entries.get(key).map(StoredToken::generation);
        if actual != expected {
            return Err(StoreError::GenerationConflict { expected, actual });
        }
        let generation = actual
            .unwrap_or(0)
            .checked_add(1)
            .ok_or(StoreError::CorruptEntry)?;
        let stored = StoredToken::new(generation, replacement);
        entries.insert(key.clone(), stored.clone());
        Ok(stored)
    }

    async fn delete(&self, key: &TokenKey, expected: u64) -> Result<(), StoreError> {
        let mut entries = self.entries.lock().map_err(|_| StoreError::LockPoisoned)?;
        let actual = entries.get(key).map(StoredToken::generation);
        if actual != Some(expected) {
            return Err(StoreError::GenerationConflict {
                expected: Some(expected),
                actual,
            });
        }
        entries.remove(key);
        Ok(())
    }
}

use crate::budget::{
    check_raw, check_schema, drop_value_iteratively, exceeded, LimitExceeded, ResourceKind,
    ResourceLimits,
};
use serde_json::{Map, Value};
use sha2::{Digest, Sha256};
use std::{error::Error, fmt};

/// One named, parsed schema source and its verified digest.
#[derive(Debug, Clone)]
pub struct SchemaSource {
    pub(crate) component: String,
    pub(crate) value: Value,
    digest: String,
}

impl SchemaSource {
    /// Parse a JSON schema with default limits and optionally require its raw-byte SHA-256.
    pub fn from_json_bytes(
        component: impl Into<String>,
        bytes: impl AsRef<[u8]>,
        expected_sha256: Option<&str>,
    ) -> Result<Self, SourceError> {
        Self::from_json_bytes_with_limits(
            component,
            bytes,
            expected_sha256,
            ResourceLimits::default(),
        )
    }

    /// Parse a JSON schema under explicit limits and optionally require its raw-byte SHA-256.
    pub fn from_json_bytes_with_limits(
        component: impl Into<String>,
        bytes: impl AsRef<[u8]>,
        expected_sha256: Option<&str>,
        limits: ResourceLimits,
    ) -> Result<Self, SourceError> {
        let component = component.into();
        check_raw(
            component.as_bytes(),
            ResourceKind::SchemaBytes,
            limits.max_schema_bytes,
        )
        .map_err(SourceError::Limit)?;
        let bytes = bytes.as_ref();
        check_raw(bytes, ResourceKind::SchemaBytes, limits.max_schema_bytes)
            .map_err(SourceError::Limit)?;
        let digest = digest(bytes);
        verify_digest(expected_sha256, &digest)?;
        let value = serde_json::from_slice(bytes).map_err(|error| {
            if is_recursion_error(&error.to_string()) {
                SourceError::Limit(exceeded(ResourceKind::SchemaDepth, limits.schema_depth()))
            } else {
                SourceError::Parse {
                    format: "JSON",
                    message: json_error_summary(&error),
                }
            }
        })?;
        Self::finish(component, value, digest, limits)
    }

    /// Accept a JSON value with default limits and optionally require its canonical-JSON SHA-256.
    ///
    /// Object keys are recursively sorted before compact serialization, making
    /// this digest independent of map insertion order. Use [`Self::from_json_bytes`]
    /// when pinning an upstream file's exact bytes.
    pub fn from_value(
        component: impl Into<String>,
        value: Value,
        expected_sha256: Option<&str>,
    ) -> Result<Self, SourceError> {
        Self::from_value_with_limits(component, value, expected_sha256, ResourceLimits::default())
    }

    /// Accept a JSON value under explicit limits and optionally require its canonical digest.
    pub fn from_value_with_limits(
        component: impl Into<String>,
        value: Value,
        expected_sha256: Option<&str>,
        limits: ResourceLimits,
    ) -> Result<Self, SourceError> {
        let component = component.into();
        if let Err(error) = check_raw(
            component.as_bytes(),
            ResourceKind::SchemaBytes,
            limits.max_schema_bytes,
        ) {
            drop_value_iteratively(value);
            return Err(SourceError::Limit(error));
        }
        if let Err(error) = check_schema(&value, limits) {
            drop_value_iteratively(value);
            return Err(SourceError::Limit(error));
        }
        let canonical = canonicalize(&value);
        let bytes = serde_json::to_vec(&canonical).map_err(|error| SourceError::Parse {
            format: "JSON value",
            message: json_error_summary(&error),
        })?;
        let digest = digest(&bytes);
        verify_digest(expected_sha256, &digest)?;
        Self::finish_checked(component, value, digest)
    }

    fn finish(
        component: String,
        value: Value,
        digest: String,
        limits: ResourceLimits,
    ) -> Result<Self, SourceError> {
        if let Err(error) = check_schema(&value, limits) {
            drop_value_iteratively(value);
            return Err(SourceError::Limit(error));
        }
        Self::finish_checked(component, value, digest)
    }

    fn finish_checked(
        component: String,
        value: Value,
        digest: String,
    ) -> Result<Self, SourceError> {
        if component.is_empty() {
            return Err(SourceError::EmptyComponent);
        }
        if !value.is_object() && !value.is_boolean() {
            return Err(SourceError::NotSchema);
        }
        Ok(Self {
            component,
            value,
            digest,
        })
    }

    /// Name used to select this source's compiled validator.
    #[must_use]
    pub fn component(&self) -> &str {
        &self.component
    }

    /// Lowercase hexadecimal SHA-256 of the supplied source representation.
    #[must_use]
    pub fn sha256(&self) -> &str {
        &self.digest
    }
}

// This recursive transform runs only after iterative depth validation has capped
// the input at HARD_MAX_NESTING_DEPTH.
fn canonicalize(value: &Value) -> Value {
    match value {
        Value::Array(values) => Value::Array(values.iter().map(canonicalize).collect()),
        Value::Object(values) => {
            let mut keys: Vec<_> = values.keys().collect();
            keys.sort_unstable();
            let mut result = Map::new();
            for key in keys {
                result.insert(key.clone(), canonicalize(&values[key]));
            }
            Value::Object(result)
        }
        _ => value.clone(),
    }
}

fn digest(bytes: &[u8]) -> String {
    format!("{:x}", Sha256::digest(bytes))
}

pub(crate) fn verify_bytes(bytes: &[u8], expected: Option<&str>) -> Result<String, SourceError> {
    let actual = digest(bytes);
    verify_digest(expected, &actual)?;
    Ok(actual)
}

fn verify_digest(expected: Option<&str>, actual: &str) -> Result<(), SourceError> {
    if let Some(expected) = expected {
        if expected.len() != 64 || !expected.bytes().all(|byte| byte.is_ascii_hexdigit()) {
            let value = "[REDACTED]".to_owned();
            return Err(SourceError::InvalidExpectedHash { value });
        }
        let expected = expected.to_ascii_lowercase();
        if expected != actual {
            return Err(SourceError::HashMismatch {
                expected,
                actual: actual.to_owned(),
            });
        }
    }
    Ok(())
}

pub(crate) fn json_error_summary(error: &serde_json::Error) -> String {
    let category = match error.classify() {
        serde_json::error::Category::Io => "I/O",
        serde_json::error::Category::Syntax => "syntax",
        serde_json::error::Category::Data => "data",
        serde_json::error::Category::Eof => "end-of-input",
    };
    format!(
        "{category} error at line {}, column {}",
        error.line(),
        error.column()
    )
}

pub(crate) fn yaml_error_summary(error: &serde_yaml::Error) -> String {
    match error.location() {
        Some(location) => format!(
            "YAML error at line {}, column {}",
            location.line(),
            location.column()
        ),
        None => "YAML parse error".to_owned(),
    }
}

pub(crate) fn is_recursion_error(message: &str) -> bool {
    message.contains("recursion limit exceeded") || message.contains("recursion limit")
}

/// Failure while parsing or identity-checking a caller-supplied source.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SourceError {
    /// A source exceeded a configured resource budget.
    Limit(LimitExceeded),
    /// The expected digest was not 64 hexadecimal characters.
    InvalidExpectedHash {
        /// Rejected digest.
        value: String,
    },
    /// The source bytes did not match the expected SHA-256.
    HashMismatch {
        /// Caller-supplied digest.
        expected: String,
        /// Computed digest.
        actual: String,
    },
    /// Source syntax could not be parsed.
    Parse {
        /// Input syntax.
        format: &'static str,
        /// Parser diagnostic.
        message: String,
    },
    /// Component names may not be empty.
    EmptyComponent,
    /// A source must be a JSON Schema object or boolean.
    NotSchema,
}

impl fmt::Display for SourceError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Limit(error) => error.fmt(formatter),
            Self::InvalidExpectedHash { value } => {
                write!(formatter, "invalid expected SHA-256 `{value}`")
            }
            Self::HashMismatch { expected, actual } => write!(
                formatter,
                "schema SHA-256 mismatch: expected {expected}, computed {actual}"
            ),
            Self::Parse { format, message } => write!(formatter, "invalid {format}: {message}"),
            Self::EmptyComponent => formatter.write_str("schema component name is empty"),
            Self::NotSchema => formatter.write_str("schema source is not an object or boolean"),
        }
    }
}

impl Error for SourceError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::Limit(error) => Some(error),
            _ => None,
        }
    }
}

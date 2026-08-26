//! Runtime validation for caller-supplied buildingSMART openCDE schemas.
//!
//! The crate validates raw [`serde_json::Value`] instances before typed
//! deserialization. It intentionally ships no Foundation or Documents schema
//! files; applications must supply and, optionally, SHA-256-pin their source
//! corpus at runtime.

#![forbid(unsafe_code)]
#![warn(missing_docs)]

mod budget;
mod normalize;
mod source;
mod validator;

pub use budget::{LimitExceeded, ResourceKind, ResourceLimits, HARD_MAX_NESTING_DEPTH};
pub use source::{SchemaSource, SourceError};
pub use validator::{
    BuildError, DocumentsValidator, FoundationValidator, ValidationFailure, Violation,
};

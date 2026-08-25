//! `openbim-cde` — buildingSMART openCDE API wire contracts.
//!
//! # Implemented scope
//!
//! - Foundation API 1.1 discovery, authentication, client registration, user,
//!   and error payloads.
//! - All 24 named Documents API 1.0 OpenAPI schemas.
//! - Forward-compatible decoding of unknown object fields, as required by the
//!   Foundation API conventions.
//!
//! # Deliberate boundaries
//!
//! This crate contains transport-independent wire models, not an HTTP client,
//! server framework, OAuth implementation, or DIN SPEC 91391 implementation.
//! BCF remains owned by `openbim-bcf`.

#![forbid(unsafe_code)]
#![warn(missing_docs)]

/// Documents API 1.0 wire contracts.
pub mod documents;
/// Foundation API 1.1 shared wire contracts.
pub mod foundation;

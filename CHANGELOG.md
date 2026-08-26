# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

### Added

- Initial `openbim-cde` crate.
- Complete Serde model coverage for the six Foundation API 1.1 JSON schemas.
- Complete Serde model coverage for all 24 Documents API 1.0 OpenAPI components.
- Source revision and SHA-256 evidence lock without redistributing upstream schemas.
- Standalone format, build, test, Clippy, rustdoc, package, and dependency-policy gate.
- Presence-aware optional/non-null wire fields that reject `null` independently of
  their inner type, plus credential-safe `Debug` output.
- Family-specific extension handling, including Documents
  `additionalProperties: false` enforcement.
- Packaged third-party attribution and exact upstream provenance.
- Explicit Cargo package inclusion boundary with duplicate-license drift checking.
- `openbim-cde-http` typed clients and injected Axum adapters for all selected
  Foundation and Documents operations, with bounded streaming bodies.
- `openbim-cde-oauth` Authorization Code/PKCE execution, refresh rotation,
  generation-aware stores, and optional native keyring/Reqwest providers.
- `openbim-cde-validation` runtime evaluation of exact caller-supplied pinned
  Foundation and Documents schemas with version-specific normalization.
- CI checkout and execution of the exact external Foundation/Documents corpus on
  Rust 1.85.0 and current stable.

### Security

- Block crates.io publication pending upstream rights clearance.
- Redact dynamic-registration browser/client URLs, upload URLs, HTTP header values,
  and multipart framing from `Debug`; keep registered client secrets in zeroizing
  wire owners, reject registration JSON escapes before non-zeroizing Serde scratch,
  and keep each non-success body in one bounded, zeroizing explicit diagnostic.
- Require HTTPS by default, disable redirects and ambient proxy discovery, isolate
  credentials by origin, treat only an explicitly configured proxy as trusted, and
  enforce exact success media types/statuses and bounded bodies.
- Require S256 PKCE and state, reject ambiguous callbacks/endpoints, rotate refresh
  tokens with compare-and-swap, zeroize controlled secret allocations, reject
  non-global IANA IPv6 ranges, preflight caller-controlled request and persisted-token
  expansion, decode malformed base64 into zeroizing scratch, enforce request bounds
  again at the production Reqwest boundary, and persist opaque keyring tokens without
  Serde secret scratch strings.
- Bound external schemas and instances by bytes, depth, nodes, references,
  normalized/compiled-wrapper expansion, and retained violations before validation.
- Keep validation diagnostics categorical: structural pointers/keywords are retained;
  rejected instance, reference, and source-version values are not. A zero violation
  budget is enforced before retaining unknown-component failures.

[Unreleased]: https://github.com/openbimrs/cde/commits/main

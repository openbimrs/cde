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

### Security

- Block crates.io publication pending upstream rights clearance.
- Redact dynamic-registration browser/client URLs, upload URLs, HTTP header values,
  and multipart framing from `Debug`.

[Unreleased]: https://github.com/openbimrs/cde/commits/main

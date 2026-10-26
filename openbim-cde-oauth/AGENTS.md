# openbim-cde-oauth contributor context

This crate executes OAuth discovered through buildingSMART Foundation API 1.1.
It depends inward on `../openbim-cde`; never move transport or storage behavior
into the wire-model crate.

## Security invariants

- Authorization Code always uses S256 PKCE and generated CSRF state.
- Verify callback state before any token request.
- Implicit is discoverable but never executable.
- ROPC requires both discovery advertisement and explicit `LegacyPasswordPolicy::Enabled`.
- Never derive Serde traits for public secret/token types or expose a generic secret accessor.
- Preserve stored credentials after any failed exchange; rotate via TokenStore CAS only.
- Do not add plaintext file persistence.
- The reqwest adapter remains HTTPS-only, redirect-free, bounded, and diagnostic-redacted.
- Keep `keyring` exactly at 3.6.3.

## Verification

Use an isolated target directory and run on the pinned toolchain (Rust 1.99.1):

```sh
cargo fmt --check
CARGO_TARGET_DIR=/tmp/openbim-cde-oauth cargo test --all-features
CARGO_TARGET_DIR=/tmp/openbim-cde-oauth cargo clippy --all-features --all-targets -- -D warnings
CARGO_TARGET_DIR=/tmp/openbim-cde-oauth cargo doc --all-features --no-deps
```

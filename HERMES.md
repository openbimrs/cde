# OpenBIM.rs CDE

Canonical repository: <https://github.com/openbimrs/cde>
Integration repository: <https://github.com/openbimrs/openbim>

Read `AGENTS.md` before changing the repository and nested context before
editing the crate. Keep this workspace independently buildable; the integration
repository pins it as `packages/cde`.

## Verification

Run `./scripts/gate.sh`. It is the authoritative local and CI gate.

## Conventions

- Rust 2021, MSRV 1.85, MIT, pure Rust.
- The crate models buildingSMART Foundation API 1.1 and Documents API 1.0.
- DIN SPEC 91391 is research context, not a substitute machine schema.
- Do not vendor copyrighted standards or upstream schemas without a license.

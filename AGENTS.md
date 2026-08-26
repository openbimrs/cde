# OpenBIM.rs CDE repository

This repository owns the OpenBIM.rs Rust wire contracts for the buildingSMART
openCDE API family.

## Map

- `openbim-cde/` — dependency-light wire contracts; read its `AGENTS.md`
- `openbim-cde-http/` — hardened reqwest client and Axum adapters
- `openbim-cde-oauth/` — OAuth execution and secure token stores
- `openbim-cde-validation/` — external-schema normalization and validation
- `docs/` — maintained architecture and source research
- `references/` — source lock and local-only copyrighted standards
- `scripts/gate.sh` — standalone verification gate

## Boundaries

- Implement Foundation and Documents API wire contracts here.
- Keep BCF in `openbim-bcf` and bSDD/Dictionary APIs out of this crate.
- Do not add HTTP transport, OAuth execution, schema engines, or server framework
  dependencies to the wire-model crate. Put those capabilities in sibling
  adapter crates that depend inward on `openbim-cde`.
- Ignore unknown JSON object properties as required by Foundation API 1.1.
- Never commit DIN/ISO/CEN PDFs or upstream schemas without redistribution rights.
- Keep capability claims separated into implemented, conformance-tested, and
  planned.

Run `./scripts/gate.sh`; trust command exit codes.

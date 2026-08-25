# OpenBIM.rs CDE repository

This repository owns the OpenBIM.rs Rust wire contracts for the buildingSMART
openCDE API family.

## Map

- `openbim-cde/` — published crate; read its `AGENTS.md` before editing
- `docs/` — maintained architecture and source research
- `references/` — source lock and local-only copyrighted standards
- `scripts/gate.sh` — standalone verification gate

## Boundaries

- Implement Foundation and Documents API wire contracts here.
- Keep BCF in `openbim-bcf` and bSDD/Dictionary APIs out of this crate.
- Do not add HTTP transport, OAuth execution, or server framework dependencies to
  the wire-model crate.
- Ignore unknown JSON object properties as required by Foundation API 1.1.
- Never commit DIN/ISO/CEN PDFs or upstream schemas without redistribution rights.
- Keep capability claims separated into implemented, conformance-tested, and
  planned.

Run `./scripts/gate.sh`; trust command exit codes.

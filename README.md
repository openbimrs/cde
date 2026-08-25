# OpenBIM.rs CDE

Pure-Rust wire contracts for the buildingSMART openCDE API family.

[![CI](https://github.com/openbimrs/cde/actions/workflows/ci.yml/badge.svg)](https://github.com/openbimrs/cde/actions/workflows/ci.yml)
[![License: MIT](https://img.shields.io/badge/license-MIT-blue.svg)](LICENSE)

## Scope

| Capability | Status |
| --- | --- |
| Foundation API 1.1 payloads | Implemented: all 6 released JSON schemas |
| Documents API 1.0 payloads | Implemented: all 24 named OpenAPI components |
| Serde JSON read/write | Implemented and tested |
| Endpoint discovery constants | Implemented |
| HTTP client/server | Not implemented |
| OAuth execution/token storage | Not implemented |
| Full JSON Schema constraint validation | Not implemented |
| DIN SPEC 91391 interface | Research only; no machine schema found |

The crate is transport-agnostic by design. Applications choose their HTTP,
async, TLS, OAuth, retry, and observability stacks.

## Workspace

```text
openbim-cde/                 published Cargo crate
  src/foundation.rs          Foundation API 1.1 models
  src/documents.rs           Documents API 1.0 models
docs/architecture.md         boundaries and wire-fidelity decisions
docs/schema-audit.md         where the actual schemas are
references/                  source revision/hash evidence only
scripts/gate.sh              complete standalone verification
```

## Quick start

```toml
[dependencies]
openbim-cde = "0.1.0"
```

```rust
use openbim_cde::foundation::VersionsResponse;

let body = r#"{
  "versions": [{"api_id": "documents", "version_id": "1.0"}]
}"#;
let response: VersionsResponse = serde_json::from_str(body)?;
# Ok::<(), serde_json::Error>(())
```

## Schema finding

The archived [`buildingSMART/OpenCDE-API`](https://github.com/buildingSMART/OpenCDE-API)
repository is an old umbrella/draft. Current contracts are split between
[Foundation API 1.1](https://github.com/buildingSMART/foundation-API/tree/release_1_1)
and [Documents API 1.0](https://github.com/buildingSMART/documents-API/tree/release_1_0),
with BCF and Dictionary APIs remaining separate.

The supplied DIN SPEC 91391 PDFs contain no embedded OpenAPI/XSD/JSON attachment.
See the evidence and the DIN §1/§4.2 tension in
[`docs/schema-audit.md`](docs/schema-audit.md).

## Verify

```bash
./scripts/gate.sh
```

The gate checks formatting, all targets, all tests/features, Clippy with warnings
denied, rustdoc with warnings denied, and Cargo packaging.

## Integration

Canonical repository: <https://github.com/openbimrs/cde>

The OpenBIM.rs integration superproject pins this repository at `packages/cde`
as a Git submodule. Family changes land and pass the standalone gate here before
the superproject pin advances.

## License

MIT for original Rust code. Upstream schema files and locally licensed DIN PDFs
are not redistributed by this repository.

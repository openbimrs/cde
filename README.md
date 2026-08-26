# OpenBIM.rs CDE

Pure-Rust wire contracts and hardened runtime adapters for the buildingSMART
openCDE API family.

[![CI](https://github.com/openbimrs/cde/actions/workflows/ci.yml/badge.svg)](https://github.com/openbimrs/cde/actions/workflows/ci.yml)
[![License: MIT](https://img.shields.io/badge/license-MIT-blue.svg)](LICENSE)

## Scope

| Capability | Status |
| --- | --- |
| Foundation API 1.1 payloads | Implemented: all 6 released JSON schemas |
| Documents API 1.0 payloads | Implemented: all 24 named OpenAPI components |
| Serde JSON read/write | Implemented and tested |
| HTTP client/server adapters | Implemented: 4 Foundation and 13 Documents operations |
| OAuth execution/token storage | Implemented: PKCE/state, refresh rotation, opt-in legacy ROPC |
| Pinned runtime schema validation | Implemented for applicable assertions in supplied locked sources |
| Vendor certification / prose-only requirements | Not claimed |
| crates.io publication | Blocked pending upstream rights clearance |
| DIN SPEC 91391 interface | Research only; no machine schema found |

The `openbim-cde` wire crate remains transport-agnostic. Applications opt into
the sibling HTTP, OAuth, and validation layers independently; server services
and authentication policy remain injected by the host application.

## Workspace

```text
openbim-cde/                 dependency-light wire contracts
openbim-cde-http/            hardened Reqwest clients and Axum adapters
openbim-cde-oauth/           OAuth execution and token-store providers
openbim-cde-validation/      bounded external-schema validation
docs/phase-2-architecture.md runtime boundaries and security invariants
docs/schema-audit.md         exact upstream discrepancies and normalization
references/                  source revision/hash evidence only
scripts/gate.sh              complete standalone verification
```

## Quick start

```toml
[dependencies]
openbim-cde = { git = "https://github.com/openbimrs/cde.git" }
serde_json = "1"
```

The package has `publish = false` until the rights status of the upstream API
descriptions is cleared. Pin a Git revision or the OpenBIM.rs submodule gitlink
for reproducible builds.

```rust
use openbim_cde::foundation::VersionsResponse;

let body = r#"{
  "versions": [{"api_id": "documents", "version_id": "1.0"}]
}"#;
let response: VersionsResponse = serde_json::from_str(body)?;
# Ok::<(), serde_json::Error>(())
```

Runtime APIs and their security contracts are documented in the crate READMEs:
[`openbim-cde-http`](openbim-cde-http/README.md),
[`openbim-cde-oauth`](openbim-cde-oauth/README.md), and
[`openbim-cde-validation`](openbim-cde-validation/README.md).

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

Every package manifest uses an explicit inclusion boundary. The gate checks
formatting, locked dependency resolution, all targets/features, Clippy and
rustdoc with warnings denied, the exact four-package workspace, exact per-package
file inventories, `publish = false`, byte-identical licenses, MSRV/edition
metadata, and notices. Adapter packages are compiled against their real path
links; `cargo package --list` audits their archives without pretending the
intentionally unpublished wire dependency exists on crates.io. CI runs
`cargo-deny` and executes validation against clean checkouts of both exact
upstream source revisions.

## Integration

Canonical repository: <https://github.com/openbimrs/cde>

The OpenBIM.rs integration superproject pins this repository at `packages/cde`
as a Git submodule. Family changes land and pass the standalone gate here before
the superproject pin advances.

## License

MIT applies to original Rust code. Upstream schema files and locally licensed
DIN PDFs are not redistributed. The upstream repositories expose no explicit
schema license, so publication remains mechanically disabled pending rights
clearance. See `openbim-cde/THIRD_PARTY_NOTICES.md`.

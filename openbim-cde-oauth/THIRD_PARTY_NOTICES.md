# Third-party notices and source provenance

`openbim-cde-oauth` contains independently authored Rust OAuth execution code.
It does not include copies of buildingSMART schemas, Foundation API source files,
or locally licensed standards.

## Publication status

Foundation API's inspected repository did not contain an explicit license grant
for its machine-readable contracts. Cargo publication remains disabled until
translation/distribution rights are cleared. This notice is attribution, not a
third-party license grant.

## Foundation API 1.1 research baseline

- Source: <https://github.com/buildingSMART/foundation-API/tree/release_1_1>
- Commit: `2c552eae435d575f1df7c11ff9ea19e3d2a324ce`
- Consulted: `README.md`, `OAuth2Examples.md`, `schemas/auth_GET.json`,
  `schemas/dynRegClient_GET.json`, and `schemas/dynRegClient_POST.json`.
- No upstream file is redistributed in this crate.

## Rust dependencies

The crate links to third-party Rust packages under their respective licenses,
including `oauth2`, `secrecy`, `zeroize`, `async-trait`, `serde`, `serde_json`,
`url`, `http`, `base64`, `rand`, `sha2`, and `subtle`. Optional integrations use
`reqwest` with explicit ring-backed `rustls`, and `keyring` 3.6.3 plus their
transitive platform dependencies. See
the resolved `Cargo.lock` and each package's distributed license metadata for
the exact dependency graph and license terms.

The repository AGPL-3.0-or-later license applies only to original OpenBIM.rs contributions to
the extent those contributions are licensable by their authors.

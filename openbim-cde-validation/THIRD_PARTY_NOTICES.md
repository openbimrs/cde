# Third-party notices and source provenance

`openbim-cde-validation` contains independently authored Rust normalization and
validation code. It accepts upstream source documents from callers at runtime;
it does not include copies of buildingSMART JSON/YAML schemas, standards, or
PDFs.

## Publication status

The inspected Foundation API and Documents API repositories did not contain an
explicit license grant for their machine-readable contracts. This package is
therefore marked `publish = false` pending written permission or legal
clearance. This notice is attribution, not a license grant for third-party
material. The repository MIT license applies only to original OpenBIM.rs work.

## Runtime source provenance used by optional integration tests

### Foundation API 1.1

- Source: <https://github.com/buildingSMART/foundation-API/tree/release_1_1>
- Commit: `2c552eae435d575f1df7c11ff9ea19e3d2a324ce`
- `schemas/auth_GET.json`: `28f6ece4cbb180450c35190ca91860209d0b73aaa446982bde0abbbf43dad906`
- `schemas/dynRegClient_GET.json`: `8cfaaa2e205a24cc64502627178304567849862146eab0a26d7e9030503b0968`
- `schemas/dynRegClient_POST.json`: `ba0b5e258bf1ff3d65e91162496ff2c80820fe60de655e982fbda4d03f34aba1`
- `schemas/error.json`: `cec6b1a36818b2b3c5bfc9030e440e5fec0f97cfea263e2f6cf8229b93f24b63`
- `schemas/user_GET.json`: `9fcacd7b5659863908edffb5a2f64a2929b32a7032db4413869e31cf7e55328e`
- `schemas/versions_GET.json`: `38e32d9cc2ce87eba72a583b6ef7fb18e5569191566727627dee9d77a842d38d`

### Documents API 1.0

- Source: <https://github.com/buildingSMART/documents-API/tree/release_1_0>
- Commit: `938bd7636e094c2dc3c65ad6dae846ac3c7b6a9b`
- `swagger.yaml`: `b6d5b5ee5da62648e385d5e468a9589f3ea4b385fabcfd3598b5a8bb668b6bbf`

## Rust dependencies

Direct dependencies are used under their upstream licenses:

- `openbim-cde` — MIT (path dependency)
- `jsonschema` 0.29.1 — MIT; default HTTP/file resolver features disabled
- `sha2` 0.10 — MIT OR Apache-2.0
- `base64` 0.22 — MIT OR Apache-2.0
- `serde` / `serde_json` 1.x — MIT OR Apache-2.0
- `serde_yaml` 0.9 — MIT OR Apache-2.0

Transitive dependency licensing remains represented by Cargo metadata and is
subject to the repository's dependency-audit gate.

# CDE extraction and initial implementation

## Goal

Create `openbimrs/cde` as a standalone Rust workspace, implement the current
buildingSMART openCDE Foundation 1.1 and Documents 1.0 wire contracts, and pin it
as `packages/cde` in `openbimrs/openbim`.

## Constraints

- Preserve the local DIN PDFs but never commit or publish them.
- Do not conflate DIN SPEC 91391-2's conceptual API with the later
  buildingSMART API family.
- Keep BCF and bSDD outside this crate.
- Model extension compatibility required by Foundation 1.1.
- Use exact released upstream revisions and exercise serialization behavior.

## Workstreams

1. Lock upstream source revisions and document the schema finding.
2. Add failing wire-contract tests.
3. Implement Foundation and Documents models.
4. Add standalone docs, CI, package gate, and changelog.
5. Push the child repository and integrate it as a parent submodule.

## Validation

- Observe the initial contract tests fail before implementation.
- Run fmt, build, test, Clippy, rustdoc, and `cargo package` standalone.
- Run the superproject submodule and root integration gates at the exact pin.
- Verify a fresh anonymous recursive clone.

## Rollback

The child repository is independently revertible. Parent integration is one
narrow gitlink/manifest/docs commit and can be reverted without touching other
families.

## Status

- [x] Audit DIN PDFs, legacy archive, and current portfolio repositories.
- [x] Record exact upstream revisions/hashes without vendoring schemas.
- [x] Add failing wire-contract tests, then implement Foundation 1.1 and every
  named Documents 1.0 component.
- [x] Add standalone CI/gate, package metadata, progressive context, and honest
  capability documentation.
- [x] Mutation-probe the upload authorization default test and restore it.
- [ ] Land and push the clean standalone commit.
- [ ] Pin the child as `packages/cde` in the integration superproject and pass
  the root gate.

### Evidence

- `openapi-spec-validator swagger.yaml` — structurally valid OpenAPI 3.0.
- `cargo package -p openbim-cde --allow-dirty` — 10 files; no standards PDFs.
- Standalone format/build/test/Clippy/rustdoc passed; the final clean package
  gate awaits the initial commit because Cargo refuses dirty packaging.

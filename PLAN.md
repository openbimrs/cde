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
- [x] Correct optional/non-null fidelity, enforce Documents object-closure rules,
  and redact credential-bearing `Debug` output.
- [x] Land and push the initial standalone implementation; initial GitHub CI was green.
- [x] Pin the initial child as `packages/cde` in the integration superproject and
  pass the root gate at that revision.
- [x] Push the audit remediation and update/revalidate the parent gitlink.

## Phase 2: executable protocol stack

Architecture and acceptance criteria live in
[`docs/phase-2-architecture.md`](docs/phase-2-architecture.md).

- [x] Audit Foundation/Documents HTTP operations, OAuth semantics, validation
  dialects, dependency options, and crate boundaries.
- [x] Preserve the moved local DIN corpus and mutation-verify a directory-agnostic
  tracked-PDF guard.
- [ ] Add `openbim-cde-http` with hardened client and configurable server adapters.
- [ ] Add `openbim-cde-oauth` with PKCE, refresh rotation, legacy-flow policy,
  memory storage, and optional native keyring persistence.
- [ ] Add `openbim-cde-validation` with external-schema identity checks,
  Draft-3/OpenAPI normalization, and complete raw-payload validation.
- [ ] Mutation-test, independently review, land, and advance the parent gitlink.

### Evidence

- `openapi-spec-validator swagger.yaml` — structurally valid OpenAPI 3.0.
- Current standalone `scripts/gate.sh` — fmt, locked build, 15 tests, Clippy,
  rustdoc, and package verification passed.
- `cargo package -p openbim-cde` — 12 files, 52.5 KiB; attribution included and
  no standards PDFs.
- Parent `scripts/gate.sh` — workspace/feature/isolation/submodule gates passed,
  including `isolated build -p openbim-cde`.

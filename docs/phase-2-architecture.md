# Phase 2: executable OpenCDE protocol stack

**Status:** implementation candidate; release gates and exact-tree review required
**Wire baseline:** `openbim-cde` 0.1, buildingSMART Foundation API 1.1 and Documents API 1.0

## Goal

Add production HTTP client/server adapters, OAuth execution with secure token storage, and complete runtime validation of the pinned released schemas without contaminating the wire-model crate or redistributing upstream schema files.

## Non-negotiable boundaries

- `openbim-cde` remains the dependency-light Serde contract crate.
- New behavior lives in sibling crates and depends inward on `openbim-cde`.
- BCF and bSDD remain separate API families.
- Upstream schemas are supplied by callers at runtime; no DIN, ISO, CEN, Foundation, or Documents source schema is packaged.
- Publication remains disabled until translation/distribution rights are cleared.
- Default HTTP policy requires HTTPS, disables redirects carrying credentials, bounds response/request bodies, validates content types, and redacts secrets.
- Remote schema-reference fetching is disabled by default to prevent SSRF and nondeterministic validation.

## Workspace structure

```text
openbim-cde/             # existing wire contracts only
openbim-cde-http/        # reqwest client + axum server adapters
openbim-cde-oauth/       # OAuth execution and TokenStore implementations
openbim-cde-validation/  # raw JSON schema/OpenAPI validation
```

## HTTP workstream

### Client

- Resolve fixed Foundation discovery paths and discovered API base URLs safely.
- Cover all Foundation methods and every Documents OpenAPI operation.
- Accept Documents URLs advertised by the server; never send literal `server-provided-path-*` placeholders.
- Expose explicit timeout, maximum-body, TLS, proxy, and user-agent policy.
- Ignore ambient proxy environment variables; only an explicit `proxy` builder call
  makes a proxy part of the caller's credential trust boundary.
- Disable redirects by default so bearer tokens, Basic credentials, signed URLs, and upload headers cannot cross origins.
- Reject cleartext credential transport unless a test-only/local-development policy explicitly permits it.
- Retain each non-success body only once in a bounded, zeroizing diagnostic;
  callers must explicitly expose untrusted server text, and no ordinary parsed
  error-message copy is kept.
- Reject JSON escapes in successful dynamic-registration responses before Serde;
  its escape-decoding scratch is not zeroizing.
- Store dynamically registered client secrets in zeroizing wire owners with
  redacted diagnostics.
- Stream document bodies; do not buffer unbounded files.

### Server

- Provide axum routers for the four fixed Foundation operations.
- Provide host-configured Documents route templates and typed handler traits for all operations.
- Validate JSON before handler dispatch and enforce request-body limits.
- Stream upload/download bodies with backpressure.
- Keep authentication as an injected policy; no bundled identity provider.

Documents deliberately publishes server-provided URLs. Router paths are therefore application configuration, not invented global constants.

## OAuth workstream

- Execute Authorization Code Grant with mandatory PKCE and state verification.
- Execute refresh-token rotation and Resource Owner Password Credentials only when explicitly advertised and enabled as a legacy compatibility policy.
- Explicitly reject Implicit and Client Credentials execution; Foundation 1.1 does not advertise Client Credentials, and Implicit cannot meet the required redirect/secret policy.
- Model access, refresh, client, and PKCE secrets with redacted diagnostics and zeroization-capable storage.
- Define an async, generation-aware `TokenStore` so refresh rotation cannot silently overwrite a newer token in one process.
- Ship an in-memory implementation for tests/ephemeral services.
- Ship native credential-store persistence behind an opt-in feature pinned to an MSRV-compatible keyring release; preflight the complete persisted payload before base64/JSON allocation, reject escaped JSON, and decode malformed base64 only into zeroizing scratch.
- Never provide plaintext file persistence.
- Inject bearer authentication into HTTP adapters; OAuth authorization-server/identity-provider implementation is not implied.
- Bound production Reqwest requests before copying: query-free URI up to 8 KiB,
  at most 64 headers, 16 KiB per value, 64 KiB aggregate, and a 1 MiB body.

Foundation 1.1 advertises authorization-code, implicit, and resource-owner-password flows. Implicit is recognized for discovery compatibility but is not executed because it cannot meet the required secret/redirect security properties. This rejection is explicit, typed, and documented.

## Validation workstream

Validation consumes raw JSON plus caller-supplied locked source documents before typed deserialization:

1. Verify optional expected SHA-256 identities.
2. Normalize Foundation Draft-3 property-level requiredness and type arrays into deterministic Draft-4-compatible schemas.
3. Normalize OpenAPI 3.0 `nullable`, component references, formats, and object closure into deterministic JSON Schema form.
4. Reject network/file references outside the supplied in-memory registry.
5. Enforce byte, node, depth, component, reference-expansion, compiled-wrapper,
   and violation budgets before and during normalization/validation.
6. Account and dispose caller-built JSON values iteratively so hostile nesting
   cannot exhaust the native stack.
7. Compile immutable validators only after all structural limits pass, without
   allowing component count to multiply retained schema copies beyond aggregate
   byte/node budgets.
8. Return bounded, stable JSON-pointer/keyword violations with categorical
   messages that never retain rejected instance, reference, or source-version
   values; check the violation budget before retaining even an unknown-component
   violation, so a configured zero remains a hard zero.
9. Deserialize into `openbim-cde` only after raw validation passes.

The validator follows released machine semantics. The known `minLength`-on-array typo is inapplicable to arrays and therefore has no released-schema effect; an optional interoperability policy may enforce the likely intended non-empty array but must label that rule as inferred rather than normative.

“Full” means every applicable assertion in the supplied pinned Foundation fragments and Documents component schemas is evaluated. It does not mean arbitrary remote schemas or prose-only requirements are silently invented.

## Dependency decisions

- `reqwest 0.13`: client with `rustls-no-provider` and an explicit ring provider,
  streaming, and JSON; MSRV 1.85.
- `axum 0.8` and `tower`: server adapters; MSRV below 1.85.
- `oauth2 5.0`: protocol execution with default HTTP client disabled.
- `secrecy`/`zeroize`: explicit secret boundaries.
- `keyring 3.6.3`: optional native persistence; keyring 4.x is rejected because its MSRV is 1.88.
- `jsonschema`: normalized schema execution with remote resolution disabled.
- `sha2`: source identity verification.

All are subject to `cargo-deny`, exact lockfile resolution, default-feature review, and isolated-build checks.

## Verification

- TDD for every public operation and security invariant.
- Local loopback client/server integration tests, including streamed bodies and bounded error responses.
- OAuth tests against a deterministic local token endpoint: state/PKCE, refresh rotation, failure preservation, and redaction.
- Validation tests for every keyword present in the pinned sources plus local differential checks against the exact external corpus.
- Mutation probes for redirect policy, HTTP opt-in, body limits, state verification, token CAS, redaction, remote-ref rejection, requiredness, bounds, patterns, formats, enums, arrays, and object closure.
- The Rust 1.99.1 full gate, cargo-deny, package allowlists, independent exact-tree review, fresh recursive parent clone, and parent CI.

## Rollback

Each adapter crate is independently removable. The wire crate API remains compatible. Parent integration remains a narrow gitlink advance.

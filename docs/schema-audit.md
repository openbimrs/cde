# OpenCDE schema audit

**Audited:** 2026-08-25

## Answer

No single current "OpenCDE schema" exists. The name covers a portfolio, and the
machine-readable contracts are split across repositories.

| Source | Status | Machine contract | Crate treatment |
| --- | --- | --- | --- |
| [OpenCDE-API](https://github.com/buildingSMART/OpenCDE-API) | Archived 2024-03-07; last commit 2022 | Legacy 2019 draft `openapi/openCDE-spec.yaml` | Research only |
| [Foundation API 1.1](https://github.com/buildingSMART/foundation-API/tree/release_1_1) | Current release branch/tag | Six JSON Schema Draft 3 fragments | Implemented |
| [Documents API 1.0](https://github.com/buildingSMART/documents-API/tree/release_1_0) | Current release branch | OpenAPI 3.0 `swagger.yaml`, 24 components | Implemented |
| [BCF API 3.0](https://github.com/buildingSMART/BCF-API/tree/release_3_0) | Separate standard API | Its own schemas | `openbim-bcf` boundary |
| [Dictionary API](https://app.swaggerhub.com/apis/buildingSMART/Dictionaries/v1) | Separate portfolio API | SwaggerHub contract | Future dedicated package |

The Foundation repository itself is the current source family, not the obsolete
umbrella repository. Its default `master` branch has continued to evolve after
the pinned 1.1 release and currently includes PKCE-related discovery prose and
fields, but no newer release branch or tag supersedes `release_1_1`. This
repository therefore claims released Foundation 1.1 compatibility only. The
OAuth adapter requires PKCE as a security policy without presenting unreleased
`master` additions as normative 1.1 wire fields.

Exact commits and SHA-256 hashes are locked in
[`references/upstream-sources.toml`](../references/upstream-sources.toml).
Upstream schemas are not copied into this repository because the inspected
Foundation and Documents repositories contain no explicit redistribution
license. Omitting those source files does not by itself settle whether derived
wire declarations require permission. The Cargo package therefore sets
`publish = false` pending written permission or legal clearance, and ships exact
attribution in `THIRD_PARTY_NOTICES.md`.

## DIN SPEC 91391 is not the missing buildingSMART schema

The locally supplied DIN SPEC 91391-1/-2 PDFs are useful conceptual prior art,
but they do not contain an embedded machine-readable attachment:

- `pdfdetach -list` reports **0 embedded files** for all four German/English PDFs.
- DIN SPEC 91391-2 §1 says its requirements do not describe implementation
  syntax and vendor interfaces may differ syntactically.
- §4.2 nevertheless says the interface is specified in OpenAPI 3.0 format.
- The visible document then defines conceptual metadata and REST operations,
  not a bundled OpenAPI YAML/JSON file.

That internal tension explains why searching the PDFs for an XSD or JSON schema
fails. No overlooked schema is embedded in the purchased PDFs. The DIN contract
also predates and differs from the current buildingSMART Foundation/Documents
portfolio, so reconstructing a purported DIN OpenAPI file from the prose would
create a non-normative invention.

Part 1's editable example metadata sheet may be available as a separate DIN
spreadsheet download, but it is an example/template, not the Part 2 API schema.

## Source quality caveats

The released sources are usable, but they are not internally perfect:

- Foundation's files are JSON Schema Draft 3 fragments. Only `error.json`
  declares `$schema`; required properties use Draft 3's per-property
  `"required": true` form.
- `auth_GET.json` does not mark `supported_oauth2_flows` as required and permits
  `null`, while the Foundation 1.1 parameter table marks it required. Wire
  parsing follows the released machine schema; this crate does not yet claim a
  separate prose-conformance validator.
- Foundation and Documents distinguish absent properties from explicit `null`.
  Optional, non-null properties use the presence-aware `OptionalNonNull<T>`
  wire type; fields whose schemas explicitly allow `null` continue to use
  `Option<T>`.
- Documents marks many objects `additionalProperties: false`, while Foundation
  1.1 §1.8 requires every OpenCDE request/response object to accept and ignore
  extra properties. The modules preserve that family boundary: Foundation
  objects remain extension-tolerant, while Documents objects enforce each
  component's declared policy.
- `UploadFilePartInstruction` combines a closed `LinkData` schema with more
  fields through `allOf`; strict JSON Schema evaluation can reject those added
  fields. The Rust model implements the evident merged payload shape and rejects
  properties outside that merged set.
- `upload_file_parts` uses `minLength` on an array where `minItems` was likely
  intended. This crate does not claim full JSON Schema validation.

These semantic caveats are why the models are reviewed against both schema and
normative prose rather than generated blindly.

## Validation evidence

- Documents `swagger.yaml` at the locked release commit passes
  `openapi-spec-validator` as OpenAPI 3.0.
- A stricter Redocly recommended-policy lint reports missing server/security/4xx
  documentation. Those are quality-policy findings, not structural parse
  failures, and are not silently "fixed" in this crate.
- The Rust tests deserialize every released named component and pin required
  fields, enum rejection, defaults, nullable entries, family-specific extension
  policies, and secret redaction.

## What not to do

- Do not implement the archived draft as if it were Foundation 1.1/Documents 1.0.
- Do not generate one monolithic client combining BCF, Documents, and bSDD.
- Do not publish the DIN PDFs; they are locally licensed and visibly watermarked.
- Do not claim full JSON Schema validation or HTTP/OAuth behavior from Serde
  model coverage alone.

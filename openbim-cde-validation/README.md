# openbim-cde-validation

Runtime JSON Schema validation for caller-supplied buildingSMART openCDE
Foundation API 1.1 fragments and Documents API 1.0 OpenAPI 3.0 documents.

The package contains no upstream schema files. `publish = false` remains in force
pending upstream rights clearance.

## Guarantees

- Optional SHA-256 identity verification happens before schema parsing.
- Raw byte limits are enforced before hashing or deserialization; parsed values
  are measured iteratively before canonical hashing, normalization, or validation.
- Foundation Draft-3 property-level `required` booleans and type arrays are
  normalized deterministically to Draft-4.
- Documents OpenAPI `nullable`, component `$ref`, `format`, object closure, and
  object-inheritance `allOf` are normalized before compilation.
- HTTP(S), file, and unknown component references are rejected before the schema
  engine runs. `jsonschema` is built without its HTTP/file resolver features.
- A validator is compiled once per component and cached immutably.
- Raw `serde_json::Value` validation always precedes typed deserialization.
- Violations are deterministically sorted and identify the component, instance
  JSON Pointer, normalized-schema JSON Pointer, and assertion keyword.
- Local reference graphs are cycle-checked and both schema and instance work is
  bounded by typed, caller-configurable resource limits.

## Usage

```rust
use openbim_cde::foundation::User;
use openbim_cde_validation::{FoundationValidator, SchemaSource};
use serde_json::json;

let source = SchemaSource::from_value(
    "user_GET",
    json!({
        "type": "object",
        "properties": {
            "id": {"type": "string", "required": true},
            "name": {"type": ["string", "null"]}
        }
    }),
    None,
)?;
let validators = FoundationValidator::new([source])?;
let user: User = validators.validate_and_deserialize(
    "user_GET",
    json!({"id": "user-1", "name": null}),
)?;
assert_eq!(user.id, "user-1");
# Ok::<(), Box<dyn std::error::Error>>(())
```

For exact file pinning, use `SchemaSource::from_json_bytes` and
`DocumentsValidator::from_openapi_bytes`; their digest covers the original
bytes. Value-based constructors hash recursively key-sorted compact JSON.

Every byte/value constructor and validation operation has a corresponding
`*_with_limits` entry point accepting `ResourceLimits`. Validators built with
`new_with_limits` retain those instance limits for ordinary `validate` and
`validate_*_and_deserialize` calls. A `LimitExceeded` is returned through the
typed `SourceError::Limit`, `BuildError::Limit`, or `ValidationFailure::Limit`
variant; budget exhaustion is never reported as an ordinary schema violation.

Default limits are 2 MiB / 50,000 nodes / depth 64 for schemas, 64 components,
4,096 references, expansion depth 32, 8 MiB / 250,000 nodes / depth 64 for
instances, 1,024 returned violations, and 1 MiB of retained violation data. Depth settings may be lowered by the
caller and are always capped by `HARD_MAX_NESTING_DEPTH` (96), including when a
larger value is configured. Owned rejected `serde_json::Value` trees are
dismantled iteratively so even caller-built extreme nesting cannot overflow the
stack while the API returns its typed error.

## Released-source anomalies

### `UploadFilePartInstruction` `allOf`

The Documents source extends closed `LinkData` (`additionalProperties: false`)
through `allOf`. Literal JSON Schema intersection semantics would make every
extension field an illegal additional property. OpenAPI uses this shape as
object inheritance. The normalizer resolves local object branches and merges
properties and required sets into one closed object. Conflicting duplicate
properties and non-object branches fail construction rather than guessing.

### `minLength` on an array

The released `DocumentToUpload.upload_file_parts` schema applies `minLength: 1`
to an array. Under JSON Schema, `minLength` applies only to strings, so the
keyword has no effect there. This crate preserves that literal machine meaning:
an empty array is accepted and no inferred `minItems` rule is introduced.
`minLength` still executes for every string location where it is applicable.

## Keyword coverage

The pinned source corpus contains these applicable assertions/applicators:
`type`, `required`, `properties`, `items`, `$ref`, `allOf`, `enum`, `minLength`,
`format`, and `additionalProperties`; OpenAPI `nullable` is translated to a
null union. Synthetic tests execute every item, including `byte`, `date-time`,
`int32`, and `int64` format semantics. `$schema`, `title`, `description`,
`default`, and `example` are annotations rather than instance assertions.

The integration test requires the exact external corpus from
`OPENCDE_REFERENCE_ROOT`, falling back to `/mnt/backup/references/opencde`.
Machines intentionally running synthetic-only tests must opt out explicitly with
`OPENCDE_CORPUS_OPTIONAL=1`; no source file is embedded.

## Security boundary

Only in-document JSON Pointer references of the exact form
`#/components/schemas/<name>` are accepted for Documents input. Foundation
fragments permit only resolvable in-fragment JSON Pointers. No network or file
retrieval is attempted, making schema loading deterministic and SSRF-safe.
Reference cycles are rejected before normalization/compilation. Aggregate budget
checks cover supplied values, normalized output, and the complete set of
per-component compiled Documents wrappers, so component count cannot amplify one
accepted corpus outside its byte/node budget. Violation and deserialization
diagnostics retain structural pointers/keywords but never rejected instance,
reference, or source-version strings. Violation count is checked before retaining
even an unknown-component violation, so `max_violations = 0` is a hard zero.
Borrowed `validate(&Value)` input is walked
iteratively and never passed to `jsonschema` after a budget failure; as with any
borrowed API, destruction of that value remains the caller's responsibility.

## License and provenance

AGPL-3.0-or-later covers the original Rust implementation. Upstream API schemas retain their
respective rights and are never redistributed here. See
[`THIRD_PARTY_NOTICES.md`](THIRD_PARTY_NOTICES.md).

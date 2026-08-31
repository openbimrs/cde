# openbim-cde

Typed, transport-agnostic Rust wire models for the buildingSMART openCDE API
family.

## Example

```rust
use openbim_cde::foundation::VersionsResponse;

let response: VersionsResponse = serde_json::from_str(
    r#"{"versions":[{"api_id":"documents","version_id":"1.0"}]}"#,
)?;
assert_eq!(response.versions[0].as_ref().unwrap().api_id, "documents");
# Ok::<(), serde_json::Error>(())
```

## Modules

- `foundation` — all six Foundation API 1.1 payload schemas and discovery paths.
- `documents` — all 24 named Documents API 1.0 OpenAPI component schemas and
  standardized entry paths.

Required properties are required Rust fields. Optional, non-null properties use
`OptionalNonNull<T>` so explicit JSON `null` is rejected; properties that are
explicitly nullable use `Option<T>`. Closed wire enums are Rust enums. Foundation
objects accept extensions; Documents objects enforce each schema's declared
`additionalProperties` policy.
Client secrets redact their `Debug` representation, zeroize owned values on drop,
and reject copied/escaped deserialization paths that cannot preserve the zeroizing
ownership boundary.

This crate does **not** execute HTTP requests, OAuth flows, file transfers, or
full JSON Schema validation. See the repository
[architecture](https://github.com/openbimrs/cde/blob/main/docs/architecture.md)
and [schema audit](https://github.com/openbimrs/cde/blob/main/docs/schema-audit.md).

## License and publication status

AGPL-3.0-or-later applies to the original Rust implementation. Standards and upstream schemas
retain their respective rights and are not redistributed in this crate. The
package has `publish = false` pending upstream rights clearance; exact source
attribution ships in [`THIRD_PARTY_NOTICES.md`](THIRD_PARTY_NOTICES.md).

# openbim-cde crate

Pure-Rust Serde wire models for buildingSMART Foundation API 1.1 and Documents
API 1.0.

- `foundation` owns shared discovery, authentication metadata, user, and error
  payloads.
- `documents` owns Documents API request/response payloads.
- Preserve the wire vocabulary with `serde(rename_all = "snake_case")`.
- Required schema properties use non-`Option` fields; nullable or optional
  properties use `Option`.
- Do not use `deny_unknown_fields`: Foundation API 1.1 requires clients and
  servers to ignore extension properties.
- Keep URLs and RFC 3339 date-times as strings unless validation is explicit;
  parsing them implicitly would reject otherwise round-trippable payloads.

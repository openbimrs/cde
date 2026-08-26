# openbim-cde-validation crate

Runtime validation adapter for caller-supplied Foundation API 1.1 JSON Schema
fragments and Documents API 1.0 OpenAPI documents.

- Keep all changes inside this directory unless a repository-level integration
  task explicitly says otherwise.
- Never vendor upstream buildingSMART schemas, standards, or PDFs.
- Preserve raw-JSON-before-typed-deserialization ordering.
- Keep remote and file reference resolution disabled; reject unknown references
  during normalization.
- Normalization must be deterministic and fail closed on unsupported/conflicting
  schema constructs.
- Stable violations must retain component, instance pointer, schema pointer, and
  keyword.
- Add a synthetic regression before changing behavior. External-corpus tests may
  load `OPENCDE_REFERENCE_ROOT` but must remain runnable without that checkout.
- Rust 2021, MSRV 1.85, no unsafe code, `publish = false`.
- Run format, test, clippy with `-D warnings`, and rustdoc warnings on both stable
  and Rust 1.85 using targets outside the repository.

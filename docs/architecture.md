# Architecture

## Boundary

`openbim-cde` models the common buildingSMART openCDE Foundation API and the
Documents API. It does not absorb every API in the portfolio:

```text
Foundation 1.1  ─┐
                 ├─ openbim-cde (wire contracts)
Documents 1.0  ──┘
BCF API/XML     ─── openbim-bcf
Dictionary API ─── future dedicated bSDD package
HTTP transport ─── application adapter
```

## Why transport is outside

The standards define payloads and flows, but applications differ in async
runtime, TLS, OAuth token storage, retry policy, proxies, observability, and
server/client role. Keeping `reqwest`, `axum`, and OAuth implementations out of
the core crate makes the contracts usable from any stack and keeps dependency
weight low.

## Wire fidelity rules

- Required fields are non-optional Rust fields.
- Nullable or optional fields use `Option<T>`.
- Foundation's nullable array entries are represented as `Vec<Option<ApiVersion>>`.
- Closed enumerations use Rust enums.
- Date-time, URL, and base64 fields remain strings because the released schemas
  specify their wire representation but not a canonical Rust parser or storage
  policy.
- Unknown object properties are ignored for forward compatibility, following
  Foundation 1.1's compatibility convention.
- OAuth client secrets use a transparent `ClientSecret` newtype with redacted
  `Debug` output.

Serde enforces shape, requiredness, and closed enums. It does not by itself
implement every JSON Schema assertion such as `minLength`; runtime conformance
validation remains explicitly out of scope for 0.1.

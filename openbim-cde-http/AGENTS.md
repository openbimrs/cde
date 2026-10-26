# openbim-cde-http crate

Executable HTTP adapters for `openbim-cde` wire contracts.

- Keep all transport policy explicit and secure by default: HTTPS, no redirects,
  finite timeouts, finite bodies, content-type checks, and redacted diagnostics.
- Fixed Foundation discovery is always origin-relative at `/foundation/versions`.
- Documents hypermedia operations consume exact server-advertised absolute URLs;
  never issue requests to the specification's placeholder paths.
- Keep service traits object-safe and authentication injected.
- JSON requests are bounded before deserialization and service dispatch. Binary
  upload/download bodies stay streaming.
- Every public behavior and security invariant must be test-driven.
- Verify with the crate manifest directly on Rust 1.99.1.

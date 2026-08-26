# openbim-cde-http

Hardened, executable HTTP adapters for the buildingSMART Foundation API 1.1 and
Documents API 1.0 wire models in [`openbim-cde`](../openbim-cde).

The client requires HTTPS by default, disables redirects, bounds all buffered
bodies, validates response content types, redacts credentials, and only sends
authorization to another origin when an explicit policy permits it. Documents
hypermedia methods use exact absolute URLs advertised by the CDE. Non-success
bodies are retained only in one bounded, zeroizing `Diagnostic`; ordinary
callers must invoke `Diagnostic::expose_untrusted` to read server-controlled text.
Dynamic-registration `ClientSecret` values have zeroizing owners. Successful registration
responses containing JSON escapes are rejected before Serde because its escape scratch is
not zeroizing; servers should emit the opaque secret as unescaped UTF-8.

The server exposes configurable Axum routers with injected authentication and
object-safe asynchronous services. A host must configure every Documents route;
there are intentionally no fake defaults based on OpenAPI placeholder paths.
Upload and download payloads remain streaming. Declared uploads above
`ServerLimits::max_upload_body` receive `413 Payload Too Large`; unknown-length
streams are wrapped with the same limit and services must propagate body-stream
errors rather than discarding them. If the process has not selected a rustls
cryptography provider, the client installs the ring provider before building.

This package is not published while upstream schema redistribution and
conformance arrangements are unresolved.

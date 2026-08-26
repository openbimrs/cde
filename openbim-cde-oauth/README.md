# openbim-cde-oauth

Secure OAuth 2 execution and generation-aware token persistence for
buildingSMART openCDE Foundation API 1.1 discovery contracts.

Publication is disabled pending upstream contract translation/distribution
clearance. Rust edition 2021; MSRV 1.85.

## Implemented

- Discovery-to-executable-config URL and flow validation (HTTPS by default).
- Authorization Code Grant with mandatory S256 PKCE, generated CSRF state, and
  constant-time state verification before token exchange.
- Async code exchange and refresh-token rotation through an injected transport.
- Foundation's one-time refresh semantics: a successful refresh must return a
  replacement refresh token, and persistence uses generation CAS.
- ROPC only when Foundation discovery advertises it **and** callers pass
  `LegacyPasswordPolicy::Enabled`.
- Typed rejection of the recognized Implicit Grant.
- Foundation dynamic client-registration request/response mapping.
- Secret access/refresh/client/password, callback code, PKCE verifier, and all
  credential-bearing HTTP/form/JSON buffers use zeroizing owners. Token and
  registration responses are parsed directly into drop-zeroizing secret fields;
  native-keyring payloads base64url-encode opaque token values, reject JSON
  escapes, decode malformed values only into zeroizing scratch, and preflight the
  complete 1 MiB persisted representation before base64 or JSON allocation. Public
  diagnostics are categorical and never retain server values. Public token types
  do not implement Serde serialization.
- Explicit `BearerToken::expose()` boundary for HTTP adapters.
- `MemoryTokenStore` for ephemeral services and tests.
- Optional native OS credential-store persistence (`native-keyring`) using
  exactly keyring 3.6.3. No plaintext file store exists.
- Optional `ReqwestOAuthTransport` (`reqwest`) with `rustls-no-provider` plus an
  explicit ring provider, HTTPS-only requests, redirects and ambient proxies
  disabled, public-address-only DNS, timeout policy, bounded outgoing URI,
  headers and body, bounded response bodies, and sanitized errors.

## Minimal flow

```rust,no_run
use openbim_cde::foundation::AuthenticationDiscovery;
use openbim_cde_oauth::{
    ClientCredentials, OAuthConfig, OAuthExecutor, OAuthHttpTransport,
};

# fn build<T: OAuthHttpTransport>(discovery: AuthenticationDiscovery, transport: T) -> Result<(), Box<dyn std::error::Error>> {
let config = OAuthConfig::from_discovery(discovery)?;
let executor = OAuthExecutor::new(
    config,
    ClientCredentials::new("registered-client", None),
    "https://client.example/oauth/callback",
    transport,
)?;
let pending = executor.begin_authorization(["documents.read"])?;
println!("open {}", pending.authorization_url());
// Pass the complete redirect URL back; origin/path, unique code/state, and CSRF
// state are validated before any token request.
// let tokens = executor.exchange_authorization_code(pending, callback_url, now).await?;
# Ok(())
# }
```

The authorization URL contains state, so treat the URL as sensitive and do not
log it.

## Features

- `native-keyring`: native Apple/Linux/Windows credential storage. Keyring calls
  are synchronous behind the async trait; avoid invoking them on a latency-
  sensitive single-thread executor. CAS is atomic between clones of one store
  instance in one process; native keyring APIs do not provide cross-process
  transactions.
- `reqwest`: a fail-closed public-Internet HTTPS transport. It rejects proxy
  environment variables and any DNS set containing special-use/private addresses.
  For private-network OAuth deployments, deterministic tests, or custom runtimes,
  implement a deliberately scoped `OAuthHttpTransport` instead.

Refresh rotation is serialized for clones of one `OAuthExecutor`, then persisted
with generation CAS. Callers must share that executor (or provide an external
distributed lease) when multiple processes can refresh the same credential.

## Deliberate limitations

This is an OAuth client, not an authorization server or identity provider. It
does not execute Implicit Grant, automatically inject bearer tokens into
arbitrary requests, follow OAuth redirects, persist plaintext files, coordinate
CAS across processes, or fetch discovery itself. The injected transport is a
trusted credential-bearing boundary and must not log request headers/bodies.

Dynamic client registration requires Foundation's exact `201 Created`; OAuth
token/refresh/legacy-password success requires RFC 6749's exact `200 OK`.
Credential-bearing request/response wrappers redact URI, headers, and bodies from
`Debug`. Token responses are capped at 1 MiB. Authorization input is capped at
128 scopes and 8 KiB aggregate scope text; the complete authorization URL is
capped at 16 KiB. Registration metadata and credential-bearing form bodies use
worst-case expansion preflight checks before serializers allocate duplicate
buffers. The production Reqwest adapter independently rejects query-bearing or
oversized request URIs, more than 64 headers, header values above 16 KiB,
aggregate headers above 64 KiB, and request bodies above 1 MiB.

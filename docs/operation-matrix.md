# OpenCDE operation matrix

Source baseline: Foundation API 1.1 `2c552eae…`; Documents API 1.0 `938bd763…`.

## Foundation

| Method | Path | Request | Success |
| --- | --- | --- | --- |
| `GET` | `/foundation/versions` | none | `VersionsResponse` |
| `GET` | configured Foundation 1.1 auth path | none | `AuthenticationDiscovery` |
| `POST` | configured dynamic-registration path | `DynamicClientRegistrationRequest` | `DynamicClientRegistrationResponse` |
| `GET` | configured current-user path | Bearer or advertised Basic auth | `User` |

Only versions has one mandatory global path. The versioned base and service URLs are discovery/configuration inputs.

## Documents

The OpenAPI file intentionally uses server-provided placeholder paths. The server chooses real URLs and the client executes those advertised URLs.

| Method | Upstream operation | Request body | Success body |
| --- | --- | --- | --- |
| `POST` | `/select-documents` | `SelectDocuments` | `DocumentDiscoverySessionInitialization` |
| `GET` | advertised selected-documents URL | none | `SelectedDocuments` |
| `POST` | `/upload-documents` | `UploadDocuments` | `DocumentUploadSessionInitialization` |
| `GET` | advertised document-download URL | none | streamed bytes |
| `GET` | advertised document-metadata URL | none | `DocumentMetadata` |
| `GET` | advertised document-versions URL | none | `DocumentVersions` |
| `GET` | advertised document-version URL | none | `DocumentVersion` |
| `POST` | advertised upload-documents URL | `UploadFileDetails` | `DocumentsToUpload` |
| `PUT` | advertised upload-file-part URL | streamed bytes | empty response |
| `POST` | advertised upload-file-part URL | streamed bytes | empty response |
| `POST` | advertised upload-completion URL | none | `DocumentVersion` |
| `POST` | advertised upload-cancellation URL | none | empty response |
| `POST` | `/document-versions` | `DocumentQuery` | `DocumentQueryResult` or `304` |

## Data-plane execution

Metadata operations return credential-bearing download links and upload-part instructions. The HTTP client must also execute those instructions with the exact method/headers/form data, streamed request/response bodies, bounded diagnostics, redirect isolation, and redacted errors.

## Authentication boundary

All protected metadata operations accept bearer-token injection. OAuth token acquisition/rotation is owned by `openbim-cde-oauth`; an authorization-server implementation is not part of the buildingSMART payload contract.

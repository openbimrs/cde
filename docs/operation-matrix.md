# OpenCDE operation matrix

Source baseline: Foundation API 1.1 `2c552eae…`; Documents API 1.0 `938bd763…`.

## Foundation

| Method | Path | Request | Status | Success body |
| --- | --- | --- | --- | --- |
| `GET` | `/foundation/versions` | none | `200` | `VersionsResponse` |
| `GET` | configured Foundation 1.1 auth path | none | `200` | `AuthenticationDiscovery` |
| `POST` | configured dynamic-registration path | `DynamicClientRegistrationRequest` | `201` | `DynamicClientRegistrationResponse` |
| `GET` | configured current-user path | Bearer or advertised Basic auth | `200` | `User` |

Only versions has one mandatory global path. The versioned base and service URLs are discovery/configuration inputs.

## Documents

The OpenAPI file intentionally uses server-provided placeholder paths. The server chooses real URLs and the client executes those advertised URLs.

| Method | Upstream operation | Request body | Status | Success body |
| --- | --- | --- | --- | --- |
| `POST` | `/select-documents` | `SelectDocuments` | `200` | `DocumentDiscoverySessionInitialization` |
| `GET` | advertised selected-documents URL | none | `200` | `SelectedDocuments` |
| `POST` | `/upload-documents` | `UploadDocuments` | `200` | `DocumentUploadSessionInitialization` |
| `GET` | advertised document-download URL | none | `200` | streamed bytes |
| `GET` | advertised document-metadata URL | none | `200` | `DocumentMetadata` |
| `GET` | advertised document-versions URL | none | `200` | `DocumentVersions` |
| `GET` | advertised document-version URL | none | `200` | `DocumentVersion` |
| `POST` | advertised upload-documents URL | `UploadFileDetails` | `200` | `DocumentsToUpload` |
| `PUT` | advertised upload-file-part URL | streamed bytes | `200` | empty response |
| `POST` | advertised upload-file-part URL | streamed bytes | `200` | empty response |
| `POST` | advertised upload-completion URL | none | `200` | `DocumentVersion` |
| `POST` | advertised upload-cancellation URL | none | `204` | empty response |
| `POST` | `/document-versions` | `DocumentQuery` | `200` | `DocumentQueryResult` |

## Data-plane execution

Metadata operations return credential-bearing download links and upload-part instructions. The HTTP client must also execute those instructions with the exact method/headers/form data, streamed request/response bodies, bounded diagnostics, redirect isolation, and redacted errors.

## Authentication boundary

All protected metadata operations accept bearer-token injection. OAuth token acquisition/rotation is owned by `openbim-cde-oauth`; an authorization-server implementation is not part of the buildingSMART payload contract.

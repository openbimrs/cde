//! Wire contracts from buildingSMART Documents API 1.0.
//!
//! This module models every named schema in the release OpenAPI document. It is
//! intentionally transport-agnostic: callers can use any HTTP stack.

use serde::{Deserialize, Serialize};

/// Standard entry path for the document-selection flow.
pub const SELECT_DOCUMENTS_PATH: &str = "/select-documents";
/// Standard entry path for the document-upload flow.
pub const UPLOAD_DOCUMENTS_PATH: &str = "/upload-documents";
/// Documents API version implemented by these contracts.
pub const VERSION: &str = "1.0";

fn is_false(value: &bool) -> bool {
    !*value
}

/// Metadata associated with one document version.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DocumentMetadata {
    /// Metadata key/value entries.
    pub metadata: Vec<DocumentMetadataEntry>,
}

/// Hypermedia link returned by a CDE.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LinkData {
    /// Absolute or server-defined link URL.
    pub url: String,
}

/// HTTP method permitted for an upload part.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum HttpMethod {
    /// HTTP POST.
    #[serde(rename = "POST")]
    Post,
    /// HTTP PUT.
    #[serde(rename = "PUT")]
    Put,
}

/// Instructions for uploading one inclusive byte range of a file.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct UploadFilePartInstruction {
    /// Upload target inherited from the OpenAPI `LinkData` schema.
    pub url: String,
    /// HTTP method to use.
    pub http_method: HttpMethod,
    /// Additional headers required by the upload target.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub additional_headers: Option<Headers>,
    /// Whether to attach the OpenCDE authorization header.
    #[serde(default, skip_serializing_if = "is_false")]
    pub include_authorization: bool,
    /// Optional multipart framing supplied by the CDE.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub multipart_form_data: Option<MultipartFormData>,
    /// Inclusive, zero-based first byte in this part.
    pub content_range_start: i64,
    /// Inclusive, zero-based final byte in this part.
    pub content_range_end: i64,
}

/// Server-provided multipart/form-data framing bytes, base64 encoded on wire.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct MultipartFormData {
    /// Prefix prepended to the binary part, encoded as OpenAPI `byte`.
    pub prefix: String,
    /// Suffix appended to the binary part, encoded as OpenAPI `byte`.
    pub suffix: String,
}

/// Primitive type declared for a document metadata entry.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum MetadataDataType {
    /// String values.
    #[serde(rename = "string")]
    String,
    /// Boolean values represented as strings by the upstream schema.
    #[serde(rename = "boolean")]
    Boolean,
    /// RFC 3339-style date-time values represented as strings.
    #[serde(rename = "date-time")]
    DateTime,
    /// Date values represented as strings.
    #[serde(rename = "date")]
    Date,
    /// Signed 32-bit integer values represented as strings.
    #[serde(rename = "integer32")]
    Integer32,
    /// Signed 64-bit integer values represented as strings.
    #[serde(rename = "integer64")]
    Integer64,
    /// Numeric values represented as strings.
    #[serde(rename = "number")]
    Number,
    /// URL values represented as strings.
    #[serde(rename = "url")]
    Url,
}

/// One document metadata key and its possibly multi-valued content.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DocumentMetadataEntry {
    /// Metadata property name.
    pub name: String,
    /// Values. Documents API 1.0 represents every primitive as a string.
    pub value: Vec<String>,
    /// Primitive interpretation for `value` entries.
    pub data_type: MetadataDataType,
}

/// All known versions of a document.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DocumentVersions {
    /// Ordered or server-provided document versions.
    pub documents: Vec<DocumentVersion>,
}

/// Documents selected by a user in the CDE UI.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SelectedDocuments {
    /// Opaque CDE UI context, if supplied.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub server_context: Option<String>,
    /// Selected document versions.
    pub documents: Vec<DocumentVersion>,
}

/// One immutable version of a CDE document.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DocumentVersion {
    /// Hypermedia operations associated with this version.
    pub links: DocumentVersionLinks,
    /// Human-readable, CDE-specific version number.
    pub version_number: String,
    /// Machine-orderable version sequence number.
    pub version_index: i32,
    /// Creation timestamp as supplied by the CDE.
    pub creation_date: String,
    /// Human-readable code or title.
    pub title: String,
    /// Backing file information.
    pub file_description: FileDescription,
    /// Stable document identifier used for update queries.
    pub document_id: String,
}

/// Hypermedia operations for a document version.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DocumentVersionLinks {
    /// Self link for this version.
    pub document_version: LinkData,
    /// Metadata retrieval link.
    pub document_version_metadata: LinkData,
    /// File download link.
    pub document_version_download: LinkData,
    /// Link listing all versions of the document.
    pub document_versions: LinkData,
    /// Optional browser UI for document details.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub document_details: Option<LinkData>,
}

/// Basic file information.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct FileDescription {
    /// File name.
    pub name: String,
    /// File size in bytes.
    pub size_in_bytes: i64,
}

/// Result of initiating a document-discovery session.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DocumentDiscoverySessionInitialization {
    /// CDE browser UI where the user selects documents.
    pub select_documents_url: String,
    /// URL lifetime in seconds.
    pub expires_in: i32,
}

/// Request that starts the document-selection flow.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SelectDocuments {
    /// Browser redirect callback.
    pub callback: CallbackLink,
    /// Opaque CDE UI context to restore.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub server_context: Option<String>,
    /// Preferred extensions, including their leading dots.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub supported_file_extensions: Option<Vec<String>>,
}

/// File sizes used to calculate multipart upload instructions.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct UploadFileDetails {
    /// Files participating in the upload session.
    pub files: Vec<UploadFileDetail>,
}

/// Size and session identifier for one local upload file.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct UploadFileDetail {
    /// File size in bytes.
    pub size_in_bytes: i64,
    /// Client-provided identifier within this upload session.
    pub session_file_id: String,
}

/// Request that starts the document-upload flow.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct UploadDocuments {
    /// Browser redirect callback.
    pub callback: CallbackLink,
    /// Opaque CDE UI context to restore.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub server_context: Option<String>,
    /// Local files the user will describe in the CDE UI.
    pub files: Vec<FileToUpload>,
}

/// One local file offered to a CDE upload flow.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct FileToUpload {
    /// File name displayed by the CDE UI.
    pub file_name: String,
    /// Client-provided upload-session identifier.
    pub session_file_id: String,
    /// Existing document identifier when uploading a new version.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub document_id: Option<String>,
}

/// Result of initiating a document-upload session.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DocumentUploadSessionInitialization {
    /// CDE browser UI where the user enters metadata.
    pub upload_ui_url: String,
    /// URL lifetime in seconds.
    pub expires_in: i32,
    /// Maximum supported file size.
    pub max_size_in_bytes: i64,
}

/// Upload instructions returned after CDE metadata entry.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DocumentsToUpload {
    /// Opaque CDE UI context, if supplied.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub server_context: Option<String>,
    /// Per-document upload instructions.
    pub documents_to_upload: Vec<DocumentToUpload>,
}

/// Request sequence for uploading one document.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DocumentToUpload {
    /// Client-provided upload-session identifier.
    pub session_file_id: String,
    /// Ordered upload parts.
    pub upload_file_parts: Vec<UploadFilePartInstruction>,
    /// Operation invoked after all parts succeed.
    pub upload_completion: LinkData,
    /// Operation invoked to cancel the upload.
    pub upload_cancellation: LinkData,
}

/// Browser redirect callback with a finite lifetime.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CallbackLink {
    /// Redirect target.
    pub url: String,
    /// URL lifetime in seconds.
    pub expires_in: i32,
}

/// Additional HTTP headers required by a server-provided operation.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Headers {
    /// Header name/value pairs.
    pub values: Vec<HeaderValue>,
}

/// One HTTP header.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct HeaderValue {
    /// Header field name.
    pub name: String,
    /// Header field value.
    pub value: String,
}

/// Stable document identifiers to query for updates.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DocumentQuery {
    /// Document identifiers to track.
    pub document_ids: Vec<String>,
}

/// Latest versions returned by a document update query.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DocumentQueryResult {
    /// Latest known version for each changed document.
    pub versions: Vec<DocumentVersion>,
}

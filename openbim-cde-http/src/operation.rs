/// A Documents OpenAPI operation inventory entry.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct DocumentOperation {
    /// Literal path key in the released OpenAPI document.
    pub path: &'static str,
    /// Uppercase HTTP method.
    pub method: &'static str,
    /// Released OpenAPI operation identifier.
    pub operation_id: &'static str,
}

impl DocumentOperation {
    /// Construct an inventory entry.
    #[must_use]
    pub const fn new(path: &'static str, method: &'static str, operation_id: &'static str) -> Self {
        Self {
            path,
            method,
            operation_id,
        }
    }
}

/// Exact Documents API 1.0 path/method/operationId inventory.
///
/// Placeholder paths describe server-advertised URLs; they are inventory only
/// and are never used as client defaults or server route defaults.
pub const DOCUMENT_OPERATIONS: [DocumentOperation; 13] = [
    DocumentOperation::new("/select-documents", "POST", "/select-documents"),
    DocumentOperation::new(
        "/server-provided-path-selected-documents-url",
        "GET",
        "/server-provided-path-selected-documents-url",
    ),
    DocumentOperation::new("/upload-documents", "POST", "/upload-documents"),
    DocumentOperation::new(
        "/server-provided-path-document-download",
        "GET",
        "/server-provided-path-document-download",
    ),
    DocumentOperation::new(
        "/server-provided-path-document-metadata",
        "GET",
        "/server-provided-path-document-metadata",
    ),
    DocumentOperation::new(
        "/server-provided-path-document-versions",
        "GET",
        "/server-provided-path-document-versions",
    ),
    DocumentOperation::new(
        "/server-provided-path-document-version",
        "GET",
        "/server-provided-path-document-version",
    ),
    DocumentOperation::new(
        "/server-provided-path-upload-documents-url",
        "POST",
        "/server-provided-path-upload-documents-url",
    ),
    DocumentOperation::new(
        "/server-provided-path-document-upload-file-part",
        "PUT",
        "/server-provided-path-document-upload-file-put",
    ),
    DocumentOperation::new(
        "/server-provided-path-document-upload-file-part",
        "POST",
        "/server-provided-path-document-upload-file-post",
    ),
    DocumentOperation::new(
        "/server-provided-path-document-upload-completion",
        "POST",
        "/server-provided-path-document-upload-completion",
    ),
    DocumentOperation::new(
        "/server-provided-path-document-upload-cancellation",
        "POST",
        "/server-provided-path-document-upload-cancellation",
    ),
    DocumentOperation::new("/document-versions", "POST", "/document-versions"),
];

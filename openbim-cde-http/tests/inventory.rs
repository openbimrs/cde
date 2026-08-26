#![allow(missing_docs)]

use openbim_cde_http::{DocumentOperation, DOCUMENT_OPERATIONS};

#[test]
fn inventory_exactly_matches_documents_openapi_paths_methods_and_ids() {
    let expected = [
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
    assert_eq!(DOCUMENT_OPERATIONS, expected);
}

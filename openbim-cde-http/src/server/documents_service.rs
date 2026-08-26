use super::types::DocumentQueryServiceResponse;
use super::{DocumentDownload, RequestContext, UploadCancellationResponse, UploadPartResponse};
use crate::ServiceError;
use async_trait::async_trait;
use axum::body::Body;
use openbim_cde::documents::{
    DocumentDiscoverySessionInitialization, DocumentMetadata, DocumentQuery,
    DocumentUploadSessionInitialization, DocumentVersion, DocumentVersions, DocumentsToUpload,
    SelectDocuments, SelectedDocuments, UploadDocuments, UploadFileDetails,
};

/// Object-safe asynchronous implementation of all thirteen Documents operations.
#[async_trait]
pub trait DocumentsService: Send + Sync {
    /// Standard select-documents POST.
    async fn select_documents(
        &self,
        context: RequestContext,
        request: SelectDocuments,
    ) -> Result<DocumentDiscoverySessionInitialization, ServiceError>;

    /// Advertised selected-documents GET.
    async fn selected_documents(
        &self,
        context: RequestContext,
    ) -> Result<SelectedDocuments, ServiceError>;

    /// Standard upload-documents POST.
    async fn upload_documents(
        &self,
        context: RequestContext,
        request: UploadDocuments,
    ) -> Result<DocumentUploadSessionInitialization, ServiceError>;

    /// Advertised document download GET.
    async fn download_document(
        &self,
        context: RequestContext,
    ) -> Result<DocumentDownload, ServiceError>;

    /// Advertised metadata GET.
    async fn document_metadata(
        &self,
        context: RequestContext,
    ) -> Result<DocumentMetadata, ServiceError>;

    /// Advertised versions GET.
    async fn document_versions(
        &self,
        context: RequestContext,
    ) -> Result<DocumentVersions, ServiceError>;

    /// Advertised single-version GET.
    async fn document_version(
        &self,
        context: RequestContext,
    ) -> Result<DocumentVersion, ServiceError>;

    /// Advertised upload-instructions POST.
    async fn upload_instructions(
        &self,
        context: RequestContext,
        request: UploadFileDetails,
    ) -> Result<DocumentsToUpload, ServiceError>;

    /// Advertised file-part PUT with a streaming body.
    async fn upload_file_part_put(
        &self,
        context: RequestContext,
        body: Body,
    ) -> Result<UploadPartResponse, ServiceError>;

    /// Advertised file-part POST with a streaming body.
    async fn upload_file_part_post(
        &self,
        context: RequestContext,
        body: Body,
    ) -> Result<UploadPartResponse, ServiceError>;

    /// Advertised upload-completion POST.
    async fn upload_completion(
        &self,
        context: RequestContext,
    ) -> Result<DocumentVersion, ServiceError>;

    /// Advertised upload-cancellation POST.
    async fn upload_cancellation(
        &self,
        context: RequestContext,
    ) -> Result<UploadCancellationResponse, ServiceError>;
    /// Standard document-versions query POST; `None` represents 304 Not Modified.
    async fn query_document_versions(
        &self,
        context: RequestContext,
        request: DocumentQuery,
        if_none_match: Option<String>,
    ) -> Result<DocumentQueryServiceResponse, ServiceError>;
}

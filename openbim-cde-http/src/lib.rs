//! Hardened HTTP adapters for buildingSMART OpenCDE Foundation and Documents APIs.
//!
//! The wire contracts remain in [`openbim_cde`]. This crate supplies transport
//! behavior without embedding an OAuth provider or inventing Documents routes.

#![forbid(unsafe_code)]
#![warn(missing_docs)]

mod client;
mod error;
mod operation;
mod server;

pub use client::{
    Credential, Download, HttpClient, HttpClientBuilder, QueryResponse, UploadSource,
};
pub use error::{ConfigError, Diagnostic, HttpError, ServiceError};
pub use operation::{DocumentOperation, DOCUMENT_OPERATIONS};
pub use server::{
    documents_router, foundation_router, AllowAnonymous, AuthContext, AuthPolicy, AuthRequest,
    DocumentDownload, DocumentQueryServiceResponse, DocumentRoutes, DocumentsAuthPolicies,
    DocumentsService, FoundationAuthPolicies, FoundationRoutes, FoundationService, RequestContext,
    ResponseMetadata, ServerLimits, UploadCancellationResponse, UploadPartResponse,
};

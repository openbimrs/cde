use axum::{
    http::StatusCode,
    response::{IntoResponse, Response},
    Json,
};
use core::fmt;
use openbim_cde::foundation::ErrorResponse;
use zeroize::Zeroizing;

/// Configuration failure detected before a client or router is built.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ConfigError {
    message: String,
}

impl ConfigError {
    pub(crate) fn new(message: impl Into<String>) -> Self {
        Self {
            message: message.into(),
        }
    }
}

impl fmt::Display for ConfigError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.message)
    }
}

impl std::error::Error for ConfigError {}

/// Bounded, scrubbed but still untrusted text from an HTTP error response.
///
/// Formatting is deliberately opaque. Callers must explicitly opt in before
/// exposing server-controlled text to logs, UIs, or telemetry.
#[derive(Clone, PartialEq, Eq)]
pub struct Diagnostic(Zeroizing<String>);

impl Diagnostic {
    pub(crate) fn new(value: Zeroizing<String>) -> Self {
        Self(value)
    }

    /// Explicitly expose redacted, server-controlled diagnostic text.
    ///
    /// The value remains untrusted and can contain terminal escapes, markup,
    /// misleading text, or secrets unknown to this client.
    #[must_use]
    pub fn expose_untrusted(&self) -> &str {
        &self.0
    }
}

impl fmt::Debug for Diagnostic {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("Diagnostic([REDACTED])")
    }
}

impl fmt::Display for Diagnostic {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("[untrusted HTTP diagnostic redacted]")
    }
}

/// A client-side transport or protocol failure.
#[derive(Clone, PartialEq, Eq)]
pub enum HttpError {
    /// A URL was syntactically invalid or unsafe for the requested operation.
    InvalidUrl(String),
    /// Cleartext HTTP was rejected by policy.
    InsecureTransport,
    /// Authorization would have crossed an origin boundary.
    CrossOriginAuthorization,
    /// A server-required upload header was invalid or unsafe.
    InvalidHeader(String),
    /// A successful JSON operation returned another content type.
    InvalidContentType,
    /// A configured request or response body limit was exceeded.
    BodyTooLarge {
        /// Configured maximum number of bytes.
        limit: usize,
    },
    /// A network request failed. Details are intentionally not retained because
    /// request URLs can contain signed query credentials.
    Transport,
    /// A non-success HTTP response.
    Response {
        /// HTTP status code.
        status: u16,
        /// Bounded redacted fallback diagnostic.
        diagnostic: Diagnostic,
    },
    /// JSON encoding or decoding failed.
    Json,
    /// Server-provided multipart framing was not valid base64.
    InvalidMultipartFraming,
}

impl fmt::Debug for HttpError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidUrl(reason) => formatter.debug_tuple("InvalidUrl").field(reason).finish(),
            Self::InsecureTransport => formatter.write_str("InsecureTransport"),
            Self::CrossOriginAuthorization => formatter.write_str("CrossOriginAuthorization"),
            Self::InvalidHeader(name) => {
                formatter.debug_tuple("InvalidHeader").field(name).finish()
            }
            Self::InvalidContentType => formatter.write_str("InvalidContentType"),
            Self::BodyTooLarge { limit } => formatter
                .debug_struct("BodyTooLarge")
                .field("limit", limit)
                .finish(),
            Self::Transport => formatter.write_str("Transport"),
            Self::Response {
                status,
                diagnostic: _,
            } => formatter
                .debug_struct("Response")
                .field("status", status)
                .field("diagnostic", &"[REDACTED]")
                .finish(),
            Self::Json => formatter.write_str("Json"),
            Self::InvalidMultipartFraming => formatter.write_str("InvalidMultipartFraming"),
        }
    }
}

impl fmt::Display for HttpError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidUrl(reason) => write!(formatter, "invalid URL: {reason}"),
            Self::InsecureTransport => formatter.write_str("cleartext HTTP is prohibited"),
            Self::CrossOriginAuthorization => {
                formatter.write_str("authorization across origins is prohibited")
            }
            Self::InvalidHeader(name) => write!(formatter, "unsafe or invalid header: {name}"),
            Self::InvalidContentType => formatter.write_str("invalid success content type"),
            Self::BodyTooLarge { limit } => write!(formatter, "body exceeds {limit} bytes"),
            Self::Transport => formatter.write_str("HTTP transport failed"),
            Self::Response { status, .. } => {
                write!(formatter, "HTTP response failed with {status}")
            }
            Self::Json => formatter.write_str("JSON processing failed"),
            Self::InvalidMultipartFraming => formatter.write_str("invalid multipart framing"),
        }
    }
}

impl std::error::Error for HttpError {}

/// A service-layer error rendered using the Foundation error schema.
#[derive(Clone, PartialEq, Eq)]
pub struct ServiceError {
    status: StatusCode,
    message: String,
}

impl ServiceError {
    /// Construct a service error.
    #[must_use]
    pub fn new(status: StatusCode, message: impl Into<String>) -> Self {
        Self {
            status,
            message: message.into(),
        }
    }

    /// Return the response status.
    #[must_use]
    pub fn status(&self) -> StatusCode {
        self.status
    }
}

impl fmt::Debug for ServiceError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("ServiceError")
            .field("status", &self.status)
            .field("message", &"[REDACTED]")
            .finish()
    }
}

impl fmt::Display for ServiceError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "OpenCDE service failed with {}", self.status)
    }
}

impl std::error::Error for ServiceError {}

impl IntoResponse for ServiceError {
    fn into_response(self) -> Response {
        (
            self.status,
            Json(ErrorResponse {
                message: self.message,
            }),
        )
            .into_response()
    }
}

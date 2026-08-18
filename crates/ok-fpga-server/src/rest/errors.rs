//! # RFC 7807 Problem Details
//!
//! Implements the [RFC 7807](https://tools.ietf.org/html/rfc7807) Problem
//! Details format for HTTP API error responses. All REST error responses
//! use this format for consistency.

use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use axum::Json;
use serde::Serialize;

/// An RFC 7807 Problem Details response body.
///
/// Every REST API error response uses this format. The `type` field is a
/// URI that identifies the problem type, and `detail` contains a
/// human-readable explanation.
///
/// ## Example JSON
///
/// ```json
/// {
///   "type": "about:blank",
///   "title": "Not Found",
///   "status": 404,
///   "detail": "Device 'ABC1234567' not found"
/// }
/// ```
#[derive(Debug, Serialize)]
pub struct ProblemDetail {
    /// A URI reference that identifies the problem type.
    /// Defaults to `"about:blank"` for generic problems.
    #[serde(rename = "type")]
    pub problem_type: String,

    /// A short, human-readable summary of the problem.
    pub title: String,

    /// The HTTP status code for this problem.
    pub status: u16,

    /// A human-readable explanation specific to this occurrence.
    pub detail: String,
}

impl ProblemDetail {
    /// Create a new ProblemDetail with the given status code and detail message.
    ///
    /// The `type` field defaults to `"about:blank"` and `title` is derived
    /// from the status code's canonical reason phrase.
    pub fn new(status: StatusCode, detail: impl Into<String>) -> Self {
        Self {
            problem_type: "about:blank".to_string(),
            title: status.canonical_reason().unwrap_or("Error").to_string(),
            status: status.as_u16(),
            detail: detail.into(),
        }
    }

    /// Create a 404 Not Found problem.
    pub fn not_found(detail: impl Into<String>) -> Self {
        Self::new(StatusCode::NOT_FOUND, detail)
    }

    /// Create a 400 Bad Request problem.
    #[allow(dead_code)] // Available for future handler use.
    pub fn bad_request(detail: impl Into<String>) -> Self {
        Self::new(StatusCode::BAD_REQUEST, detail)
    }

    /// Create a 409 Conflict problem.
    pub fn conflict(detail: impl Into<String>) -> Self {
        Self::new(StatusCode::CONFLICT, detail)
    }

    /// Create a 501 Not Implemented problem.
    ///
    /// Logs a WARN-level message so stub invocations are visible in the
    /// dashboard log viewer (ADR-022 §4).
    pub fn not_implemented(detail: impl Into<String>) -> Self {
        let detail = detail.into();
        tracing::warn!(detail = %detail, "Stub endpoint invoked — returning 501 Not Implemented");
        Self::new(StatusCode::NOT_IMPLEMENTED, detail)
    }

    /// Create a 500 Internal Server Error problem.
    #[allow(dead_code)] // Available for future handler use.
    pub fn internal(detail: impl Into<String>) -> Self {
        Self::new(StatusCode::INTERNAL_SERVER_ERROR, detail)
    }
}

/// Allow ProblemDetail to be returned directly from Axum handlers.
///
/// Sets the `Content-Type` header to `application/problem+json` as
/// required by RFC 7807.
impl IntoResponse for ProblemDetail {
    fn into_response(self) -> Response {
        let status = StatusCode::from_u16(self.status).unwrap_or(StatusCode::INTERNAL_SERVER_ERROR);

        let mut response = (status, Json(self)).into_response();
        response.headers_mut().insert(
            axum::http::header::CONTENT_TYPE,
            "application/problem+json"
                .parse()
                .expect("valid content type"),
        );
        response
    }
}

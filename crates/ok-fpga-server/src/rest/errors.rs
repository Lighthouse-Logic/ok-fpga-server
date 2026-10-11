//! # RFC 7807 Problem Details
//!
//! Implements the [RFC 7807](https://tools.ietf.org/html/rfc7807) Problem
//! Details format for HTTP API error responses. All REST error responses
//! use this format for consistency.

use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use axum::Json;
use serde::Serialize;

use ok_fpga_core::resource::{ResourceConflict, ResourceError};

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
///
/// ## Resource errors (ADR-034, D73)
///
/// Resource 409 / 422 responses may add a `conflict` object that names what
/// clashed, for example
/// `"conflict": {"code": "overlap", "resource_id": "SN:dac",
/// "resource_name": "dac", "item_type": "endpoint", "item": "0x05"}`.
/// It is left out of the JSON when there is nothing to report.
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

    /// Machine-readable resource conflict / validation detail (D73).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub conflict: Option<ResourceConflict>,
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
            conflict: None,
        }
    }

    /// Attach a D73 `conflict` object (builder style: returns `self`).
    pub fn with_conflict(mut self, conflict: ResourceConflict) -> Self {
        self.conflict = Some(conflict);
        self
    }

    /// Convert a core [`ResourceError`]: `validation` → 422, every other
    /// code → 409. The `conflict` object is always attached.
    pub fn from_resource_error(err: ResourceError) -> Self {
        let status = if err.is_validation() {
            StatusCode::UNPROCESSABLE_ENTITY
        } else {
            StatusCode::CONFLICT
        };
        Self::new(status, err.detail).with_conflict(err.conflict)
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

    /// Create a 403 Forbidden problem.
    pub fn forbidden(detail: impl Into<String>) -> Self {
        Self::new(StatusCode::FORBIDDEN, detail)
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

#[cfg(test)]
mod tests {
    use super::*;
    use ok_fpga_core::resource::ConflictCode;

    #[test]
    fn test_problem_detail_conflict_skipped_when_none() {
        let v = serde_json::to_value(ProblemDetail::not_found("x")).unwrap();
        assert!(v.get("conflict").is_none());
    }

    #[test]
    fn test_problem_detail_from_resource_error_maps_status() {
        let v = ProblemDetail::from_resource_error(ResourceError::validation("bad"));
        assert_eq!(v.status, 422);
        let overlap = ResourceError {
            detail: "clash".into(),
            conflict: ResourceConflict::new(ConflictCode::Overlap)
                .with_resource("SN:a", "a")
                .with_endpoint(5),
        };
        let p = ProblemDetail::from_resource_error(overlap);
        assert_eq!(p.status, 409);
        let v = serde_json::to_value(&p).unwrap();
        assert_eq!(v["conflict"]["code"], "overlap");
        assert_eq!(v["conflict"]["item"], "0x05");
    }
}

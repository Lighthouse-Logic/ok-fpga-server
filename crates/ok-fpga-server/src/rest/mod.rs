//! # REST API Server Module
//!
//! This module implements the REST API defined in `api/openapi.yaml`.
//! It is only compiled when the `rest` Cargo feature is enabled.
//!
//! ## Route Modules
//!
//! - [`server`] — Server status, restart, logs, service lifecycle
//! - [`devices`] — Device discovery, info, sensors, settings, SYZYGY
//! - [`classic`] — Classic data port operations (wire/trigger/pipe/register)
//! - [`axi`] — AXI data port operations (Lite/Full/Stream)
//! - [`resources`] — Multi-user resource management
//! - [`users`] — Connected user sessions
//! - [`events`] — Server-Sent Events (SSE) stream
//! - [`heartbeat_middleware`] — Implicit heartbeat on every REST request
//! - [`errors`] — RFC 7807 Problem Details error responses
//!
//! ## Architecture (ADR-004)
//!
//! Each route handler extracts `State<Arc<AppState>>` and delegates to
//! `ok-fpga-core` domain types. Responses use JSON serialization via serde.

pub mod access_middleware;
pub mod axi;
pub mod classic;
pub mod devices;
pub mod errors;
pub mod events;
pub mod heartbeat_middleware;
pub mod managed;
pub mod resources;
pub mod server;
pub mod users;

use std::sync::Arc;

use axum::middleware;
use axum::Router;

use crate::state::AppState;

use axum::http::HeaderMap;
use errors::ProblemDetail;

/// Resolve user identity from the request body and/or the `X-User-Id` header.
///
/// # Precedence (ADR-029)
///
/// 1. **Body `user_id` field** (primary) — self-documenting in OpenAPI, matches
///    the gRPC proto convention.
/// 2. **`X-User-Id` header** (fallback) — used by access control and heartbeat
///    middleware.
/// 3. **`"Unknown"`** — if neither source provides a non-empty value.
///
/// # Mismatch Rejection
///
/// If both the body field and the header are present and non-empty but differ,
/// the function returns a **400 Bad Request** `ProblemDetail`. This prevents
/// subtle bugs where the middleware (which reads the header) and the handler
/// (which reads the body) operate on different user identities.
///
/// # Arguments
///
/// * `body_user_id` — The `user_id` field from the deserialized request body
///   (`None` if the field was absent or the body was empty).
/// * `headers` — The HTTP request headers (used to read `X-User-Id`).
///
/// # Errors
///
/// Returns `Err(ProblemDetail)` with status 400 if both `user_id` and
/// `X-User-Id` are present but have different values.
pub fn resolve_user_id(
    body_user_id: Option<&str>,
    headers: &HeaderMap,
) -> Result<String, ProblemDetail> {
    let body = body_user_id.filter(|s| !s.is_empty());
    let header = headers
        .get("x-user-id")
        .and_then(|v| v.to_str().ok())
        .filter(|s| !s.is_empty());

    // Mismatch check: reject if both are present and different.
    if let (Some(b), Some(h)) = (body, header) {
        if b != h {
            return Err(ProblemDetail::bad_request(format!(
                "user_id body field '{}' does not match X-User-Id header '{}'",
                b, h
            )));
        }
    }

    Ok(body.or(header).unwrap_or("Unknown").to_string())
}

/// Build the complete REST API router.
///
/// Creates a router with all `/api/v1/*` routes nested under the version
/// prefix, wrapped with the implicit heartbeat middleware (ADR-020 §5).
///
/// The heartbeat middleware extracts the `X-User-Id` header from every
/// request and calls `user_sessions.touch()` to update the user's
/// `last_seen_at` timestamp. If the header is absent, the request
/// passes through unchanged.
///
/// # Arguments
///
/// * `state` — Shared application state
pub fn build_router(state: Arc<AppState>) -> Router {
    Router::new()
        .nest("/api/v1", api_v1_routes())
        // Access enforcement MW — on ALL /api/v1/* routes (ADR-024).
        // Three-tier enforcement: identity-only on management/device routes,
        // identity + device-level gating on Classic/AXI data port routes.
        .layer(middleware::from_fn_with_state(
            state.clone(),
            access_middleware::access_enforcement_middleware,
        ))
        // Heartbeat MW — independent liveness concern (ADR-020 §5).
        // Decoupled from access control per ADR-024 design principle 2.
        // Updates user_sessions.last_seen_at on every request that
        // includes the X-User-Id header.
        .layer(middleware::from_fn_with_state(
            state.clone(),
            heartbeat_middleware::heartbeat_middleware,
        ))
        .with_state(state)
}

/// All versioned API routes under `/api/v1`.
fn api_v1_routes() -> Router<Arc<AppState>> {
    Router::new()
        .merge(server::routes())
        .merge(devices::routes())
        .merge(classic::routes())
        .merge(axi::routes())
        .merge(resources::routes())
        .merge(users::routes())
        .merge(events::routes())
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Helper: build a `HeaderMap` with an optional `X-User-Id` header.
    fn headers_with_user(user_id: Option<&str>) -> HeaderMap {
        let mut h = HeaderMap::new();
        if let Some(uid) = user_id {
            h.insert("x-user-id", uid.parse().unwrap());
        }
        h
    }

    #[test]
    fn test_resolve_user_id_body_only() {
        // Body provided, no header → uses body.
        let headers = headers_with_user(None);
        let result = resolve_user_id(Some("alice"), &headers).unwrap();
        assert_eq!(result, "alice");
    }

    #[test]
    fn test_resolve_user_id_header_only() {
        // No body, header set → uses header.
        let headers = headers_with_user(Some("bob"));
        let result = resolve_user_id(None, &headers).unwrap();
        assert_eq!(result, "bob");
    }

    #[test]
    fn test_resolve_user_id_both_match() {
        // Both present, same value → uses that value.
        let headers = headers_with_user(Some("alice"));
        let result = resolve_user_id(Some("alice"), &headers).unwrap();
        assert_eq!(result, "alice");
    }

    #[test]
    fn test_resolve_user_id_both_mismatch_returns_400() {
        // Both present, different values → returns error.
        let headers = headers_with_user(Some("bob"));
        let result = resolve_user_id(Some("alice"), &headers);
        assert!(result.is_err());
        let err = result.unwrap_err();
        assert_eq!(err.status, axum::http::StatusCode::BAD_REQUEST.as_u16());
        assert!(err.detail.contains("does not match"));
    }

    #[test]
    fn test_resolve_user_id_neither_returns_unknown() {
        // Neither body nor header → returns "Unknown".
        let headers = headers_with_user(None);
        let result = resolve_user_id(None, &headers).unwrap();
        assert_eq!(result, "Unknown");
    }

    #[test]
    fn test_resolve_user_id_empty_body_falls_back_to_header() {
        // Empty string body is treated as absent → falls back to header.
        let headers = headers_with_user(Some("bob"));
        let result = resolve_user_id(Some(""), &headers).unwrap();
        assert_eq!(result, "bob");
    }

    #[test]
    fn test_resolve_user_id_empty_header_falls_back_to_body() {
        // Empty header is treated as absent → falls back to body.
        let mut h = HeaderMap::new();
        h.insert("x-user-id", "".parse().unwrap());
        let result = resolve_user_id(Some("alice"), &h).unwrap();
        assert_eq!(result, "alice");
    }
}

//! # Server-Sent Events (SSE) Routes
//!
//! REST endpoint for streaming device and service events.
//! Implements the `/events` path from ADR-004.

use std::sync::Arc;

use axum::routing::get;
use axum::{Json, Router};

use crate::rest::errors::ProblemDetail;
use crate::state::AppState;

/// Build the `/events` route group.
pub fn routes() -> Router<Arc<AppState>> {
    Router::new().route("/events", get(subscribe_events))
}

/// `GET /events` — Subscribe to server-sent events (SSE).
///
/// This will stream device connect/disconnect events, service state
/// changes, and resource state changes as SSE events. The full SSE
/// implementation requires wiring up the broadcast channels from
/// `DeviceManager` and `ServiceManager`.
async fn subscribe_events() -> Result<Json<serde_json::Value>, ProblemDetail> {
    Err(ProblemDetail::not_implemented(
        "SSE event streaming requires broadcast channel integration",
    ))
}

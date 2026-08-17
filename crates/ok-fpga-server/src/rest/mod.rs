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
        // Layer the implicit heartbeat middleware on all /api/v1/* routes.
        // This updates user_sessions.last_seen_at on every request that
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

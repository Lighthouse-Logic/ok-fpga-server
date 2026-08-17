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
//! - [`errors`] — RFC 7807 Problem Details error responses
//!
//! ## Architecture (ADR-004)
//!
//! Each route handler extracts `State<Arc<AppState>>` and delegates to
//! `ok-fpga-core` domain types. Responses use JSON serialization via serde.

pub mod server;
pub mod devices;
pub mod classic;
pub mod axi;
pub mod resources;
pub mod users;
pub mod events;
pub mod errors;
pub mod managed;

use std::sync::Arc;

use axum::Router;

use crate::state::AppState;

/// Build the complete REST API router.
///
/// Creates a router with all `/api/v1/*` routes nested under the version
/// prefix. The router is intended to be served on the REST port (default 8080).
///
/// # Arguments
///
/// * `state` — Shared application state
pub fn build_router(state: Arc<AppState>) -> Router {
    Router::new()
        .nest("/api/v1", api_v1_routes())
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

//! # Resource Routes
//!
//! REST endpoints for multi-user resource management.
//! Implements the `/resources/*` paths from ADR-004.

use std::sync::Arc;

use axum::extract::{Path, State};
use axum::routing::{get, post, delete};
use axum::{Json, Router};
use serde::Serialize;

use crate::rest::errors::ProblemDetail;
use crate::state::AppState;

/// Build the `/resources/*` route group.
pub fn routes() -> Router<Arc<AppState>> {
    Router::new()
        .route("/resources", get(list_resources))
        .route("/resources/{id}/acquire", post(acquire_resource))
        .route("/resources/{id}/release", delete(release_resource))
}

/// Resource list response.
#[derive(Serialize)]
struct ResourceListBody {
    resources: Vec<ResourceSummary>,
}

/// Summary of a resource (used in list responses).
#[derive(Serialize)]
struct ResourceSummary {
    id: String,
    name: String,
    device_serial: String,
    status: String,
}

/// `GET /resources` — List all registered resources across all devices.
async fn list_resources(
    State(state): State<Arc<AppState>>,
) -> Json<ResourceListBody> {
    let resources = state.resource_registry.list();
    let summaries = resources
        .iter()
        .map(|r| ResourceSummary {
            id: r.id.clone(),
            name: r.name.clone(),
            device_serial: r.device_serial.clone(),
            status: format!("{:?}", r.state),
        })
        .collect();

    Json(ResourceListBody {
        resources: summaries,
    })
}

/// `POST /resources/{id}/acquire` — Acquire a resource for the current user.
async fn acquire_resource(
    Path(_id): Path<String>,
) -> Result<Json<serde_json::Value>, ProblemDetail> {
    Err(ProblemDetail::not_implemented(
        "Resource acquisition requires user session integration",
    ))
}

/// `DELETE /resources/{id}/release` — Release a previously acquired resource.
async fn release_resource(
    Path(_id): Path<String>,
) -> Result<Json<serde_json::Value>, ProblemDetail> {
    Err(ProblemDetail::not_implemented(
        "Resource release requires user session integration",
    ))
}

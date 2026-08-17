//! # Resource Routes
//!
//! REST endpoints for multi-user resource management.
//! Implements the `/resources/*` paths from ADR-004 and ADR-016.
//!
//! ## Client-Side Resource Registration (ADR-016)
//!
//! Resources are created by clients via `POST /resources` after programming
//! a bitfile. The server is agnostic to bitfile capabilities — only the
//! client knows what resources the bitfile exposes.

use std::sync::Arc;

use axum::extract::{Path, State};
use axum::http::StatusCode;
use axum::routing::{get, post};
use axum::{Json, Router};
use serde::{Deserialize, Serialize};

use ok_fpga_core::resource::Resource;

use crate::rest::errors::ProblemDetail;
use crate::state::AppState;

/// Build the `/resources/*` route group.
pub fn routes() -> Router<Arc<AppState>> {
    Router::new()
        .route("/resources", get(list_resources).post(create_resource))
        .route(
            "/resources/{id}",
            get(get_resource).delete(delete_resource),
        )
        .route("/resources/{id}/acquire", post(acquire_resource))
        .route("/resources/{id}/release", post(release_resource))
}

// ============================================================================
// Request/Response Types
// ============================================================================

/// Request body for creating a new resource (POST /resources).
#[derive(Deserialize)]
struct CreateResourceRequest {
    /// Human-readable resource name (e.g., "xem7320-classic-counters").
    name: String,
    /// Optional description of the resource.
    description: Option<String>,
    /// Serial of the FPGA that provides this resource.
    device_serial: String,
    /// Resource type identifier (e.g., "classic", "axi", "dsp").
    #[serde(default)]
    #[allow(dead_code)] // Deserialized for future use; stored in resource metadata.
    resource_type: Option<String>,
    /// List of capabilities this resource provides.
    #[serde(default)]
    #[allow(dead_code)] // Deserialized for future use; stored in resource metadata.
    capabilities: Vec<String>,
}

/// Request body for acquiring a resource.
#[derive(Deserialize)]
struct AcquireResourceRequest {
    /// User identifier.
    user_id: String,
    /// Access mode: "shared" or "exclusive".
    mode: String,
}

/// Request body for releasing a resource.
#[derive(Deserialize)]
struct ReleaseResourceRequest {
    /// User identifier.
    user_id: String,
}

/// Resource list response.
#[derive(Serialize)]
struct ResourceListBody {
    resources: Vec<ResourceView>,
}

/// Serializable view of a resource for API responses.
#[derive(Serialize)]
struct ResourceView {
    id: String,
    name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    description: Option<String>,
    device_serial: String,
    state: String,
    holders: Vec<String>,
}

impl From<&Resource> for ResourceView {
    fn from(r: &Resource) -> Self {
        ResourceView {
            id: r.id.clone(),
            name: r.name.clone(),
            description: r.description.clone(),
            device_serial: r.device_serial.clone(),
            state: format!("{}", r.state),
            holders: r.holders.clone(),
        }
    }
}

// ============================================================================
// Route Handlers
// ============================================================================

/// `GET /resources` — List all registered resources across all devices.
async fn list_resources(State(state): State<Arc<AppState>>) -> Json<ResourceListBody> {
    let resources = state.resource_registry.list();
    let views = resources.iter().map(ResourceView::from).collect();
    Json(ResourceListBody { resources: views })
}

/// `GET /resources/{id}` — Get a specific resource by ID.
async fn get_resource(
    Path(id): Path<String>,
    State(state): State<Arc<AppState>>,
) -> Result<Json<ResourceView>, ProblemDetail> {
    match state.resource_registry.get(&id) {
        Some(resource) => Ok(Json(ResourceView::from(&resource))),
        None => Err(ProblemDetail::not_found(&format!(
            "Resource '{}' not found",
            id
        ))),
    }
}

/// `POST /resources` — Create a new resource (client-side registration, ADR-016).
///
/// The client creates resources after programming a bitfile, since the client
/// knows what capabilities the bitfile exposes. The server validates that the
/// referenced device exists and is programmed.
async fn create_resource(
    State(state): State<Arc<AppState>>,
    Json(body): Json<CreateResourceRequest>,
) -> Result<(StatusCode, Json<ResourceView>), ProblemDetail> {
    // Validate the device exists and is programmed.
    let registry = state.device_manager.registry();
    match registry.get(&body.device_serial) {
        Some(dev) => {
            if !dev.is_programmed() {
                return Err(ProblemDetail::conflict(
                    "FPGA is not programmed — program it first with POST /configure",
                ));
            }
        }
        None => {
            return Err(ProblemDetail::not_found(&format!(
                "Device '{}' not found",
                body.device_serial
            )));
        }
    }

    // Generate resource ID: {serial}:{name}
    let resource_id = format!("{}:{}", body.device_serial, body.name);

    let resource = Resource::new(
        resource_id.clone(),
        body.name.clone(),
        body.description.clone(),
        body.device_serial.clone(),
    );

    if state.resource_registry.register(resource) {
        let created = state.resource_registry.get(&resource_id).unwrap();
        Ok((StatusCode::CREATED, Json(ResourceView::from(&created))))
    } else {
        Err(ProblemDetail::conflict(&format!(
            "Resource '{}' already exists",
            resource_id
        )))
    }
}

/// `DELETE /resources/{id}` — Delete a resource definition.
///
/// Returns 409 if the resource is currently held by any user.
/// Returns 204 No Content on success.
async fn delete_resource(
    Path(id): Path<String>,
    State(state): State<Arc<AppState>>,
) -> Result<StatusCode, ProblemDetail> {
    // Check if resource exists and is not currently held.
    match state.resource_registry.get(&id) {
        Some(resource) => {
            if !resource.holders.is_empty() {
                return Err(ProblemDetail::conflict(&format!(
                    "Resource '{}' is currently held by {} user(s) — release it first",
                    id,
                    resource.holders.len()
                )));
            }
        }
        None => {
            return Err(ProblemDetail::not_found(&format!(
                "Resource '{}' not found",
                id
            )));
        }
    }

    state.resource_registry.remove(&id);
    Ok(StatusCode::NO_CONTENT)
}

/// `POST /resources/{id}/acquire` — Acquire a resource for a user.
async fn acquire_resource(
    Path(id): Path<String>,
    State(state): State<Arc<AppState>>,
    Json(body): Json<AcquireResourceRequest>,
) -> Result<Json<ResourceView>, ProblemDetail> {
    let mode = match body.mode.as_str() {
        "shared" => ok_fpga_core::resource::AccessMode::Shared,
        "exclusive" => ok_fpga_core::resource::AccessMode::Exclusive,
        _ => {
            return Err(ProblemDetail::bad_request(
                "mode must be 'shared' or 'exclusive'",
            ))
        }
    };

    state
        .resource_registry
        .acquire(&id, &body.user_id, mode)
        .map_err(|e| ProblemDetail::conflict(&format!("{}", e)))?;

    let resource = state.resource_registry.get(&id).unwrap();
    Ok(Json(ResourceView::from(&resource)))
}

/// `POST /resources/{id}/release` — Release a previously acquired resource.
async fn release_resource(
    Path(id): Path<String>,
    State(state): State<Arc<AppState>>,
    Json(body): Json<ReleaseResourceRequest>,
) -> Result<Json<serde_json::Value>, ProblemDetail> {
    if state.resource_registry.release(&id, &body.user_id) {
        Ok(Json(serde_json::json!({
            "status": "released",
            "resource_id": id,
            "user_id": body.user_id,
        })))
    } else {
        Err(ProblemDetail::not_found(&format!(
            "Resource '{}' not found or user '{}' was not holding it",
            id, body.user_id
        )))
    }
}

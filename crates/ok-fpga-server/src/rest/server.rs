//! # Server Routes
//!
//! REST endpoints for server status, restart, logs, and service lifecycle
//! management. Implements the `/server/*` paths from ADR-004.

use std::sync::Arc;

use axum::extract::State;
use axum::routing::{get, post};
use axum::{Json, Router};
use axum::extract::Path;
use serde::Serialize;

use ok_fpga_core::service::ServiceInfo;

use crate::rest::errors::ProblemDetail;
use crate::state::AppState;

/// Build the `/server/*` route group.
pub fn routes() -> Router<Arc<AppState>> {
    Router::new()
        .route("/server/status", get(get_server_status))
        .route("/server/restart", post(restart_server))
        .route("/server/services", get(list_services))
        .route("/server/services/{name}/start", post(start_service))
        .route("/server/services/{name}/stop", post(stop_service))
        .route("/server/services/{name}/restart", post(restart_service))
}

// ============================================================================
// Response Types
// ============================================================================

/// Server status response body.
#[derive(Serialize)]
struct ServerStatusBody {
    api_version: String,
    uptime_secs: u64,
    device_count: usize,
    services: Vec<ServiceInfo>,
}

/// Service list response body.
#[derive(Serialize)]
struct ServiceListBody {
    services: Vec<ServiceInfo>,
}

/// Simple success response body.
#[derive(Serialize)]
struct StatusResponseBody {
    success: bool,
    message: String,
}

// ============================================================================
// Handlers
// ============================================================================

/// `GET /server/status` — Get server status, uptime, and service states.
async fn get_server_status(
    State(state): State<Arc<AppState>>,
) -> Json<ServerStatusBody> {
    let services = state.service_manager.list();
    let device_count = state.device_manager.device_count();

    Json(ServerStatusBody {
        api_version: state.api_version.to_string(),
        uptime_secs: state.uptime_secs(),
        device_count,
        services,
    })
}

/// `POST /server/restart` — Initiate a graceful server restart.
///
/// Returns 202 Accepted. The actual restart occurs after the response
/// is sent (ADR-010 respond-then-restart pattern).
async fn restart_server() -> (axum::http::StatusCode, Json<StatusResponseBody>) {
    // In a future phase, this will signal the management layer to restart.
    // For now, return 202 with a message explaining the stub.
    (
        axum::http::StatusCode::ACCEPTED,
        Json(StatusResponseBody {
            success: true,
            message: "Server restart initiated. This is currently a stub — \
                      use the management API on port 9090 for full restart support."
                .to_string(),
        }),
    )
}

/// `GET /server/services` — List all manageable services.
async fn list_services(
    State(state): State<Arc<AppState>>,
) -> Json<ServiceListBody> {
    let services = state.service_manager.list();
    Json(ServiceListBody { services })
}

/// `POST /server/services/{name}/start` — Start a stopped service.
async fn start_service(
    State(state): State<Arc<AppState>>,
    Path(name): Path<String>,
) -> Result<Json<ServiceInfo>, ProblemDetail> {
    state
        .service_manager
        .start(&name)
        .await
        .map_err(|e| ProblemDetail::conflict(e.to_string()))?;

    state
        .service_manager
        .get_status(&name)
        .ok_or_else(|| ProblemDetail::not_found(format!("service '{}' not found", name)))
        .map(Json)
}

/// `POST /server/services/{name}/stop` — Stop a running service.
async fn stop_service(
    State(state): State<Arc<AppState>>,
    Path(name): Path<String>,
) -> Result<Json<ServiceInfo>, ProblemDetail> {
    state
        .service_manager
        .stop(&name)
        .await
        .map_err(|e| ProblemDetail::conflict(e.to_string()))?;

    state
        .service_manager
        .get_status(&name)
        .ok_or_else(|| ProblemDetail::not_found(format!("service '{}' not found", name)))
        .map(Json)
}

/// `POST /server/services/{name}/restart` — Restart a service.
async fn restart_service(
    State(state): State<Arc<AppState>>,
    Path(name): Path<String>,
) -> Result<(axum::http::StatusCode, Json<ServiceInfo>), ProblemDetail> {
    state
        .service_manager
        .restart(&name)
        .await
        .map_err(|e| ProblemDetail::conflict(e.to_string()))?;

    let info = state
        .service_manager
        .get_status(&name)
        .ok_or_else(|| ProblemDetail::not_found(format!("service '{}' not found", name)))?;

    Ok((axum::http::StatusCode::ACCEPTED, Json(info)))
}

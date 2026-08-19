//! # Server Routes
//!
//! REST endpoints for server status, restart, logs, and service lifecycle
//! management. Implements the `/server/*` paths from ADR-004.

use std::sync::Arc;

use axum::extract::Path;
use axum::extract::State;
use axum::routing::{get, post, put};
use axum::{Json, Router};
use serde::{Deserialize, Serialize};
use tracing::info;

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
        .route("/heartbeat", post(heartbeat))
        .route("/disconnect", post(disconnect))
}

// ============================================================================
// Response Types
// ============================================================================

/// Server status response body.
#[derive(Serialize)]
struct ServerStatusBody {
    api_version: String,
    description: String,
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
async fn get_server_status(State(state): State<Arc<AppState>>) -> Json<ServerStatusBody> {
    let services = state.service_manager.list();
    let device_count = state.device_manager.device_count();

    // Read description from runtime config. Fall back to "ok-fpga-server"
    // if the config has no description set.
    let description = {
        let cfg = state.config.read().unwrap();
        if cfg.server.description.is_empty() {
            cfg.mdns.instance_name.clone()
        } else {
            cfg.server.description.clone()
        }
    };

    Json(ServerStatusBody {
        api_version: state.api_version.to_string(),
        description,
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
async fn list_services(State(state): State<Arc<AppState>>) -> Json<ServiceListBody> {
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

// ============================================================================
// Heartbeat / Liveness (ADR-020 §5)
// ============================================================================

/// Request body for the heartbeat endpoint.
#[derive(Deserialize)]
struct HeartbeatRequest {
    /// User identifier sending the heartbeat.
    user_id: String,
}

/// Response body for the heartbeat endpoint.
#[derive(Serialize)]
struct HeartbeatResponse {
    /// Server timestamp (ISO 8601) when the ping was received.
    server_time: String,
    /// User ID that was pinged (echoed back for confirmation).
    user_id: String,
}

/// `POST /heartbeat` — User liveness heartbeat (ADR-020 §5).
///
/// Clients should call this periodically to keep their session alive.
/// The server responds with the current server time. In the future, this
/// will update the user session's `last_seen_at` timestamp.
async fn heartbeat(
    State(state): State<Arc<AppState>>,
    Json(body): Json<HeartbeatRequest>,
) -> Result<Json<HeartbeatResponse>, ProblemDetail> {
    if body.user_id.is_empty() {
        return Err(ProblemDetail::bad_request("user_id is required"));
    }

    // Update the user session's last_seen_at timestamp (ADR-020 §5).
    state.user_sessions.touch(&body.user_id);

    let server_time = chrono::Utc::now().to_rfc3339();

    Ok(Json(HeartbeatResponse {
        server_time,
        user_id: body.user_id,
    }))
}

// ============================================================================
// Disconnect
// ============================================================================

/// Request body for the disconnect endpoint.
#[derive(Deserialize)]
struct DisconnectRequest {
    /// User identifier to disconnect.
    user_id: String,
}

/// Response body for the disconnect endpoint.
#[derive(Serialize)]
struct DisconnectResponse {
    /// The user that was disconnected.
    user_id: String,
    /// Number of resources that were released during disconnect.
    released_resources: usize,
    /// Human-readable status message.
    message: String,
}

/// `POST /disconnect` — User-initiated disconnect (Phase 25-Fix-H, F10).
///
/// Releases all resources held by the specified user across all devices,
/// removes the user session, and ceases heartbeat checking. This is the
/// recommended way for clients to end their session cleanly.
///
/// This is preferred over simply letting the heartbeat expire because:
/// - Resources are released immediately (no timeout delay)
/// - The dashboard reflects the change right away
/// - Other clients can acquire those resources without waiting
async fn disconnect(
    State(state): State<Arc<AppState>>,
    Json(body): Json<DisconnectRequest>,
) -> Result<Json<DisconnectResponse>, ProblemDetail> {
    if body.user_id.is_empty() {
        return Err(ProblemDetail::bad_request("user_id is required"));
    }

    // Release all resources held by this user across all devices.
    let released = state.resource_registry.release_all_for_user(&body.user_id);

    // Remove the user session entirely.
    state.user_sessions.remove(&body.user_id);

    info!(
        user_id = %body.user_id,
        released_count = released,
        "User disconnected via POST /disconnect"
    );

    // Broadcast event for dashboard auto-refresh (Phase 25-Fix-H, F9).
    state.broadcast_event(&format!(
        r#"{{"type":"user_disconnected","user_id":"{}"}}"#,
        body.user_id
    ));

    Ok(Json(DisconnectResponse {
        user_id: body.user_id,
        released_resources: released,
        message: "Disconnected successfully".to_string(),
    }))
}

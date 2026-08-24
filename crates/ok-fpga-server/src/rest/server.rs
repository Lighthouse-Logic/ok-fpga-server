//! # Server Routes
//!
//! REST endpoints for server status, restart, logs, and service lifecycle
//! management. Implements the `/server/*` paths from ADR-004.

use std::sync::Arc;

use axum::extract::Path;
use axum::extract::State;
use axum::http::HeaderMap;
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

/// Server status response body — matches OpenAPI `ServerStatus` schema (Phase 37).
#[derive(Serialize)]
struct ServerStatusBody {
    /// ok-fpga-server version (from Cargo.toml).
    server_version: String,
    /// OpenAPI spec version (api/openapi.yaml info.version).
    openapi_version: String,
    /// FrontPanel DLL version string (e.g., "6.1.0"). Null if DLL not loaded.
    frontpanel_version: Option<String>,
    /// Server platform (e.g., "windows-x86_64").
    platform: String,
    /// Server description from TOML config.
    description: String,
    /// Human-readable uptime (e.g., "2h 15m 30s").
    uptime: String,
    /// Uptime in seconds.
    uptime_seconds: u64,
    /// Number of connected FPGA devices.
    connected_devices: usize,
    /// Number of registered resources.
    active_resources: usize,
    /// Status of all managed services.
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
///
/// Returns a `ServerStatus` response matching the OpenAPI schema (Phase 37).
async fn get_server_status(State(state): State<Arc<AppState>>) -> Json<ServerStatusBody> {
    let services = state.service_manager.list();
    let connected_devices = state.device_manager.device_count();
    let active_resources = state.resource_registry.count();

    // Read description from runtime config. Fall back to the mDNS instance name
    // if the config has no description set.
    let description = {
        let cfg = state.config.read().unwrap();
        if cfg.server.description.is_empty() {
            cfg.mdns.instance_name.clone()
        } else {
            cfg.server.description.clone()
        }
    };

    // Get FrontPanel DLL version if loaded.
    let frontpanel_version = state
        .handle_pool
        .as_ref()
        .map(|pool| pool.dll().api_version_string());

    // Build human-readable uptime string.
    let secs = state.uptime_secs();
    let uptime = if secs >= 3600 {
        format!("{}h {}m {}s", secs / 3600, (secs % 3600) / 60, secs % 60)
    } else if secs >= 60 {
        format!("{}m {}s", secs / 60, secs % 60)
    } else {
        format!("{}s", secs)
    };

    // Detect platform at compile time.
    let platform = format!("{}-{}", std::env::consts::OS, std::env::consts::ARCH);

    Json(ServerStatusBody {
        server_version: state.api_version.to_string(),
        openapi_version: "1.0.0".to_string(),
        frontpanel_version,
        platform,
        description,
        uptime,
        uptime_seconds: secs,
        connected_devices,
        active_resources,
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
///
/// `user_id` is optional in the Rust struct (ADR-029): if absent, the handler
/// falls back to the `X-User-Id` header.
#[derive(Deserialize)]
struct HeartbeatRequest {
    /// User identifier sending the heartbeat (optional — falls back to
    /// X-User-Id header).
    user_id: Option<String>,
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
/// The server responds with the current server time. User identity is
/// resolved from the body `user_id` field first, then the `X-User-Id`
/// header (ADR-029).
async fn heartbeat(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Json(body): Json<HeartbeatRequest>,
) -> Result<Json<HeartbeatResponse>, ProblemDetail> {
    // Resolve user identity: body first, then X-User-Id header (ADR-029).
    let user_id = super::resolve_user_id(body.user_id.as_deref(), &headers)?;

    if user_id == "Unknown" {
        return Err(ProblemDetail::bad_request("user_id is required"));
    }

    // Update the user session's last_seen_at timestamp (ADR-020 §5).
    state.user_sessions.touch(&user_id);

    let server_time = chrono::Utc::now().to_rfc3339();

    Ok(Json(HeartbeatResponse {
        server_time,
        user_id,
    }))
}

// ============================================================================
// Disconnect
// ============================================================================

/// Request body for the disconnect endpoint.
///
/// `user_id` is optional in the Rust struct (ADR-029): if absent, the handler
/// falls back to the `X-User-Id` header.
#[derive(Deserialize)]
struct DisconnectRequest {
    /// User identifier to disconnect (optional — falls back to X-User-Id header).
    user_id: Option<String>,
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
///
/// User identity is resolved from the body `user_id` field first, then the
/// `X-User-Id` header (ADR-029).
async fn disconnect(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Json(body): Json<DisconnectRequest>,
) -> Result<Json<DisconnectResponse>, ProblemDetail> {
    // Resolve user identity: body first, then X-User-Id header (ADR-029).
    let user_id = super::resolve_user_id(body.user_id.as_deref(), &headers)?;

    if user_id == "Unknown" {
        return Err(ProblemDetail::bad_request("user_id is required"));
    }

    // Release all resources held by this user across all devices.
    let released = state.resource_registry.release_all_for_user(&user_id);

    // Remove the user session entirely.
    state.user_sessions.remove(&user_id);

    info!(
        user_id = %user_id,
        released_count = released,
        "User disconnected via POST /disconnect"
    );

    // Broadcast DeviceEvent for SSE subscribers (Phase 39, ADR-030).
    let _ = state.device_manager.broadcast(
        ok_fpga_core::manager::DeviceEvent::UserDisconnected {
            user_id: user_id.clone(),
            reason: "explicit".to_string(),
            timestamp: chrono::Utc::now(),
        },
    );

    // Broadcast event for dashboard auto-refresh (Phase 25-Fix-H, F9).
    state.broadcast_event(&format!(
        r#"{{"type":"user_disconnected","user_id":"{}"}}"#,
        user_id
    ));

    Ok(Json(DisconnectResponse {
        user_id,
        released_resources: released,
        message: "Disconnected successfully".to_string(),
    }))
}

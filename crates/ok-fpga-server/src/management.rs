//! # Management Server
//!
//! The management server is the **always-on** HTTP server that runs on the
//! management port (default 9090). It serves:
//!
//! 1. **Management API** (`/mgmt/*`) — service lifecycle, config, shutdown
//! 2. **WebSocket** (`/ws/events`) — real-time event streaming
//! 3. **Web Dashboard** (`/*`) — static SPA files (when `dashboard` feature enabled)
//!
//! The management server is NOT a `ManagedService` — it cannot be stopped
//! or restarted. It runs for the entire lifetime of the process.

use std::sync::Arc;

use axum::extract::ws::{Message, WebSocket, WebSocketUpgrade};
use axum::extract::{Path, State};
use axum::response::IntoResponse;
use axum::routing::{delete, get, post, put};
use axum::{Json, Router};
use serde::{Deserialize, Serialize};
use tokio::sync::broadcast;

use crate::log_broadcast::{self, LogEvent, LogRingBuffer};
use tracing::{info, warn};

use crate::config::ServerConfig;
use crate::state::AppState;

/// Event broadcast channel capacity for WebSocket clients.
#[allow(dead_code)] // Will be used when WS event streaming is implemented.
const WS_EVENT_CHANNEL_CAPACITY: usize = 256;

/// Build the management server router.
///
/// The router includes:
/// - `/mgmt/services` — list all service states
/// - `/mgmt/services/{name}/start` — start a service
/// - `/mgmt/services/{name}/stop` — stop a service
/// - `/mgmt/services/{name}/restart` — restart a service
/// - `/mgmt/devices` — list connected FPGA devices with full info
/// - `/mgmt/resources` — list registered resources with state/holders
/// - `/mgmt/users` — list active user sessions
/// - `/mgmt/config` — get current config (sanitized)
/// - `/mgmt/status` — get server status
/// - `/mgmt/shutdown` — graceful server shutdown
/// - `/ws/events` — WebSocket event stream
/// - `/*` — static dashboard files (when `dashboard` feature enabled)
pub fn build_management_router(
    state: Arc<AppState>,
    config: Arc<ServerConfig>,
    event_tx: broadcast::Sender<String>,
    log_tx: broadcast::Sender<LogEvent>,
    log_ring_buffer: Arc<LogRingBuffer>,
    shutdown_tx: Arc<tokio::sync::Notify>,
) -> Router {
    let mgmt_state = Arc::new(MgmtState {
        app: state,
        config,
        event_tx,
        log_tx,
        log_ring_buffer,
        shutdown_tx,
    });

    let mut router = Router::new()
        // Management API routes
        .route("/mgmt/status", get(mgmt_status))
        .route("/mgmt/services", get(mgmt_list_services))
        .route("/mgmt/services/{name}/start", post(mgmt_start_service))
        .route("/mgmt/services/{name}/stop", post(mgmt_stop_service))
        .route("/mgmt/services/{name}/restart", post(mgmt_restart_service))
        .route("/mgmt/config", get(mgmt_get_config))
        .route("/mgmt/devices", get(mgmt_list_devices))
        .route("/mgmt/devices/{serial}/syzygy", get(mgmt_device_syzygy))
        .route("/mgmt/resources", get(mgmt_list_resources))
        .route("/mgmt/users", get(mgmt_list_users))
        .route("/mgmt/shutdown", post(mgmt_shutdown))
        // Log management endpoints (ADR-022)
        .route("/mgmt/logs/download", get(mgmt_logs_download))
        .route("/mgmt/logs/size", get(mgmt_logs_size))
        .route("/mgmt/logs", delete(mgmt_logs_clear))
        // Settings (Phase 28B)
        .route("/mgmt/settings", get(mgmt_get_settings))
        .route("/mgmt/settings", put(mgmt_put_settings))
        // Catalog (Phase 28D)
        .route("/mgmt/catalog", get(mgmt_get_catalog))
        // WebSocket
        .route("/ws/events", get(ws_events))
        .with_state(mgmt_state);

    // Serve static dashboard files if the `dashboard` feature is compiled.
    #[cfg(feature = "dashboard")]
    {
        router = serve_dashboard(router);
    }

    router
}

/// Internal state for management routes.
///
/// Separates management concerns from the main `AppState` to avoid
/// circular dependencies.
struct MgmtState {
    /// The shared application state.
    app: Arc<AppState>,

    /// The loaded server configuration.
    config: Arc<ServerConfig>,

    /// Broadcast channel for WebSocket event streaming (device/service events).
    event_tx: broadcast::Sender<String>,

    /// Broadcast channel for log events (ADR-022).
    log_tx: broadcast::Sender<LogEvent>,

    /// Ring buffer for log backfill on new WebSocket connections (ADR-022).
    log_ring_buffer: Arc<LogRingBuffer>,

    /// Shutdown signal — notifying this triggers the graceful shutdown
    /// flow in `main()` (stop all services, clear MQTT retained topics,
    /// broadcast SSE ServerShutdown event, then exit).
    shutdown_tx: Arc<tokio::sync::Notify>,
}

// ============================================================================
// Response Types
// ============================================================================

/// Server status response.
#[derive(Serialize)]
struct StatusResponse {
    version: String,
    frontpanel_version: String,
    platform: String,
    uptime_seconds: u64,
    uptime_human: String,
    connected_devices: usize,
    active_resources: usize,
    compiled_features: Vec<String>,
    /// Human-readable server description (Phase 28D).
    description: String,
    /// Server hostname (Phase 28D).
    hostname: String,
}

/// Service info response.
#[derive(Serialize)]
struct ServiceInfoResponse {
    name: String,
    status: String,
    compiled: bool,
    enabled: bool,
}

/// Service operation result.
#[derive(Serialize)]
struct ServiceOpResponse {
    name: String,
    status: String,
    message: String,
}

/// Sanitized config response (hides sensitive values).
#[derive(Serialize)]
struct ConfigResponse {
    management_port: u16,
    log_level: String,
    rest_enabled: bool,
    rest_port: u16,
    grpc_enabled: bool,
    grpc_port: u16,
    mqtt_enabled: bool,
    mqtt_broker_url: String,
    mdns_enabled: bool,
    dashboard_enabled: bool,
}

/// Device information response for the `/mgmt/devices` endpoint.
///
/// Provides a JSON-friendly summary of each connected FPGA device.
/// This is served from the management port (9090) so the dashboard
/// can render device details without cross-port requests to the REST API.
/// Phase 28D: enriched with is_programmed and bitfile info for expandable rows.
#[derive(Serialize)]
struct DeviceInfoResponse {
    /// Hardware serial number (unique per device).
    serial: String,
    /// Product name (e.g., "XEM7320-A75T").
    product_name: String,
    /// Opal Kelly product ID number.
    product_id: i32,
    /// Interface type ("USB 3.0", "PCIe", "Gen3").
    interface: String,
    /// USB speed if applicable.
    usb_speed: String,
    /// Current state ("Connected" or "Programmed").
    state: String,
    /// Firmware major version.
    firmware_major: i32,
    /// Firmware minor version.
    firmware_minor: i32,
    /// User-settable device ID string.
    device_id: String,
    /// Whether the FPGA has been programmed with a bitfile (Phase 28D).
    is_programmed: bool,
    /// Bitfile info, present only when programmed (Phase 28D).
    #[serde(skip_serializing_if = "Option::is_none")]
    bitfile: Option<DeviceBitfileInfo>,
}

/// Bitfile info included in DeviceInfoResponse (Phase 28D).
#[derive(Serialize)]
struct DeviceBitfileInfo {
    filename: String,
    description: String,
    programmed_at: String,
    programmed_by: String,
}

/// Resource information response for the `/mgmt/resources` endpoint.
///
/// Provides a JSON-friendly summary of each registered resource.
/// Used by the dashboard's Resources page (Phase 25-Fix-G, F6).
/// Phase 28D: enriched with description, access_mode, and metadata.
#[derive(Serialize)]
struct ResourceInfoResponse {
    /// Unique resource identifier.
    id: String,
    /// Human-readable resource name.
    name: String,
    /// Serial number of the device that hosts this resource.
    device_serial: String,
    /// Current state (e.g., "Available", "InUseShared", "InUseLocked").
    state: String,
    /// Users currently holding this resource.
    holders: Vec<String>,
    /// Who locked the resource (if any).
    locked_by: Option<String>,
    /// Optional resource description (Phase 28D).
    #[serde(skip_serializing_if = "Option::is_none")]
    description: Option<String>,
    /// Design-level access mode: "shared" or "exclusive" (Phase 28D).
    #[serde(skip_serializing_if = "Option::is_none")]
    access_mode: Option<String>,
    /// Extensible key-value metadata (Phase 28D).
    #[serde(skip_serializing_if = "std::collections::HashMap::is_empty")]
    metadata: std::collections::HashMap<String, String>,
}

/// User session response for the `/mgmt/users` endpoint.
///
/// Provides a JSON-friendly summary of each active user session.
/// Used by the dashboard's Users page (Phase 25-Fix-G, F7).
#[derive(Serialize)]
struct UserSessionResponse {
    /// User identifier.
    user_id: String,
    /// Resource IDs currently held by this user.
    held_resources: Vec<String>,
    /// ISO 8601 timestamp of last activity.
    last_seen_at: String,
}

// ============================================================================
// Management API Handlers
// ============================================================================

/// `GET /mgmt/status` — Get server status.
async fn mgmt_status(State(state): State<Arc<MgmtState>>) -> Json<StatusResponse> {
    let uptime = state.app.uptime_secs();
    let hours = uptime / 3600;
    let mins = (uptime % 3600) / 60;
    let secs = uptime % 60;

    let features = compiled_features();

    // Server description: prefer the configured description, fall back to
    // the mDNS instance name if blank (Phase 28D).
    let description = if state.config.server.description.is_empty() {
        state.config.mdns.instance_name.clone()
    } else {
        state.config.server.description.clone()
    };

    // Server hostname from the OS (Phase 28D).
    let hostname = hostname::get()
        .map(|h| h.to_string_lossy().to_string())
        .unwrap_or_else(|_| "unknown".to_string());

    Json(StatusResponse {
        version: state.app.api_version.to_string(),
        frontpanel_version: "6.1.0".to_string(),
        platform: format!("{}-{}", std::env::consts::OS, std::env::consts::ARCH),
        uptime_seconds: uptime,
        uptime_human: format!("{}h {}m {}s", hours, mins, secs),
        connected_devices: state.app.device_manager.device_count(),
        active_resources: state.app.resource_registry.count(),
        compiled_features: features,
        description,
        hostname,
    })
}

/// `GET /mgmt/devices` — List connected FPGA devices with full info.
///
/// Returns an array of device objects from the `DeviceRegistry`. This
/// endpoint exists so the web dashboard (served on the management port)
/// can display device details without needing to make cross-port requests
/// to the REST API server.
async fn mgmt_list_devices(State(state): State<Arc<MgmtState>>) -> Json<Vec<DeviceInfoResponse>> {
    let devices: Vec<DeviceInfoResponse> = state
        .app
        .device_manager
        .registry()
        .list()
        .iter()
        .map(|d| {
            let bitfile = d.state.bitfile().map(|b| DeviceBitfileInfo {
                filename: b.filename.clone(),
                description: b.description.clone().unwrap_or_default(),
                programmed_at: b.programmed_at.to_rfc3339(),
                programmed_by: b.programmed_by.clone(),
            });
            DeviceInfoResponse {
                serial: d.info.serial.clone(),
                product_name: d.info.product_name.clone(),
                product_id: d.info.product_id,
                interface: d.info.device_interface.to_string(),
                usb_speed: d.info.usb_speed.to_string(),
                state: d.state.to_string(),
                firmware_major: d.info.device_major_version,
                firmware_minor: d.info.device_minor_version,
                device_id: d.info.device_id.clone(),
                is_programmed: d.state.is_programmed(),
                bitfile,
            }
        })
        .collect();
    Json(devices)
}

/// `GET /mgmt/devices/{serial}/syzygy` — Discover SYZYGY ports for a device.
///
/// Mirrors the REST SYZYGY endpoint but on the management port so the
/// dashboard can lazy-load SYZYGY data when expanding a device row.
/// This requires the FrontPanel DLL and an open device handle.
async fn mgmt_device_syzygy(
    Path(serial): Path<String>,
    State(state): State<Arc<MgmtState>>,
) -> Result<Json<Vec<ok_fpga_core::syzygy::SyzygyPort>>, axum::http::StatusCode> {
    // Verify device exists.
    if !state.app.device_manager.registry().contains(&serial) {
        return Err(axum::http::StatusCode::NOT_FOUND);
    }

    // SYZYGY discovery requires the FrontPanel DLL and an active device handle.
    // If the handle pool is unavailable, return an empty array rather than error.
    let pool = match state.app.handle_pool.as_ref() {
        Some(p) => p,
        None => return Ok(Json(Vec::new())),
    };

    // Probe SYZYGY ports using the same algorithm as the REST endpoint.
    let ports = match pool.discover_syzygy_ports(&serial).await {
        Ok(ports) => ports,
        Err(_) => Vec::new(),
    };

    Ok(Json(ports))
}

/// `GET /mgmt/resources` — List all registered resources.
///
/// Returns resource state, holders, and lock info for each resource.
/// Used by the dashboard Resources page (Phase 25-Fix-G, F6).
async fn mgmt_list_resources(State(state): State<Arc<MgmtState>>) -> Json<Vec<ResourceInfoResponse>> {
    let resources: Vec<ResourceInfoResponse> = state
        .app
        .resource_registry
        .list()
        .iter()
        .map(|r| ResourceInfoResponse {
            id: r.id.clone(),
            name: r.name.clone(),
            device_serial: r.device_serial.clone(),
            state: format!("{:?}", r.state),
            holders: r.holders.iter().map(|h| h.user_id.clone()).collect(),
            locked_by: r.locked_by.clone(),
            description: r.description.clone(),
            access_mode: r.access_mode.as_ref().map(|m| format!("{:?}", m).to_lowercase()),
            metadata: r.metadata.clone(),
        })
        .collect();
    Json(resources)
}

/// `GET /mgmt/users` — List all active user sessions.
///
/// Returns user ID, held resources, and last activity timestamp.
/// Used by the dashboard Users page (Phase 25-Fix-G, F7).
async fn mgmt_list_users(State(state): State<Arc<MgmtState>>) -> Json<Vec<UserSessionResponse>> {
    let users: Vec<UserSessionResponse> = state
        .app
        .user_sessions
        .list()
        .iter()
        .map(|u| UserSessionResponse {
            user_id: u.user_id.clone(),
            held_resources: u.held_resource_ids.clone(),
            last_seen_at: u.last_seen_at.to_rfc3339(),
        })
        .collect();
    Json(users)
}

/// `GET /mgmt/services` — List all services and their states.
async fn mgmt_list_services(State(state): State<Arc<MgmtState>>) -> Json<Vec<ServiceInfoResponse>> {
    let services: Vec<ServiceInfoResponse> = state
        .app
        .service_manager
        .list()
        .iter()
        .map(|s| ServiceInfoResponse {
            name: s.name.clone(),
            status: format!("{}", s.status),
            compiled: s.compiled,
            enabled: s.enabled,
        })
        .collect();
    Json(services)
}

/// `POST /mgmt/services/{name}/start` — Start a stopped service.
async fn mgmt_start_service(
    State(state): State<Arc<MgmtState>>,
    Path(name): Path<String>,
) -> Json<ServiceOpResponse> {
    info!(service = %name, "management: start requested");
    match state.app.service_manager.start(&name).await {
        Ok(()) => {
            // Broadcast event.
            let _ = state.event_tx.send(
                serde_json::json!({
                    "type": "service_started",
                    "service": name,
                    "timestamp": chrono::Utc::now().to_rfc3339()
                })
                .to_string(),
            );
            Json(ServiceOpResponse {
                name: name.clone(),
                status: "started".to_string(),
                message: format!("service '{}' started successfully", name),
            })
        }
        Err(e) => Json(ServiceOpResponse {
            name: name.clone(),
            status: "error".to_string(),
            message: format!("{}", e),
        }),
    }
}

/// `POST /mgmt/services/{name}/stop` — Stop a running service.
async fn mgmt_stop_service(
    State(state): State<Arc<MgmtState>>,
    Path(name): Path<String>,
) -> Json<ServiceOpResponse> {
    info!(service = %name, "management: stop requested");
    match state.app.service_manager.stop(&name).await {
        Ok(()) => {
            let _ = state.event_tx.send(
                serde_json::json!({
                    "type": "service_stopped",
                    "service": name,
                    "timestamp": chrono::Utc::now().to_rfc3339()
                })
                .to_string(),
            );
            Json(ServiceOpResponse {
                name: name.clone(),
                status: "stopped".to_string(),
                message: format!("service '{}' stopped successfully", name),
            })
        }
        Err(e) => Json(ServiceOpResponse {
            name: name.clone(),
            status: "error".to_string(),
            message: format!("{}", e),
        }),
    }
}

/// `POST /mgmt/services/{name}/restart` — Restart a service.
async fn mgmt_restart_service(
    State(state): State<Arc<MgmtState>>,
    Path(name): Path<String>,
) -> Json<ServiceOpResponse> {
    info!(service = %name, "management: restart requested");
    // Stop then start.
    if let Err(e) = state.app.service_manager.stop(&name).await {
        return Json(ServiceOpResponse {
            name: name.clone(),
            status: "error".to_string(),
            message: format!("stop failed: {}", e),
        });
    }
    tokio::time::sleep(tokio::time::Duration::from_millis(500)).await;
    match state.app.service_manager.start(&name).await {
        Ok(()) => {
            let _ = state.event_tx.send(
                serde_json::json!({
                    "type": "service_restarted",
                    "service": name,
                    "timestamp": chrono::Utc::now().to_rfc3339()
                })
                .to_string(),
            );
            Json(ServiceOpResponse {
                name: name.clone(),
                status: "restarted".to_string(),
                message: format!("service '{}' restarted successfully", name),
            })
        }
        Err(e) => Json(ServiceOpResponse {
            name: name.clone(),
            status: "error".to_string(),
            message: format!("restart failed: {}", e),
        }),
    }
}

/// `GET /mgmt/config` — Get current config (sanitized).
async fn mgmt_get_config(State(state): State<Arc<MgmtState>>) -> Json<ConfigResponse> {
    let cfg = &state.config;
    Json(ConfigResponse {
        management_port: cfg.server.management_port,
        log_level: cfg.server.log_level.clone(),
        rest_enabled: cfg.rest.enabled,
        rest_port: cfg.rest.port,
        grpc_enabled: cfg.grpc.enabled,
        grpc_port: cfg.grpc.port,
        mqtt_enabled: cfg.mqtt.enabled,
        mqtt_broker_url: cfg.mqtt.broker_url.clone(),
        mdns_enabled: cfg.mdns.enabled,
        dashboard_enabled: cfg.dashboard.enabled,
    })
}

/// `POST /mgmt/shutdown` — Graceful server shutdown (202 Accepted).
async fn mgmt_shutdown(
    State(state): State<Arc<MgmtState>>,
) -> (axum::http::StatusCode, Json<serde_json::Value>) {
    warn!("management: shutdown requested via API");
    let _ = state.event_tx.send(
        serde_json::json!({
            "type": "server_shutdown",
            "timestamp": chrono::Utc::now().to_rfc3339()
        })
        .to_string(),
    );
    // Spawn delayed shutdown so the response is sent first.
    // Notify the shared shutdown signal which `main()` is `select!`-ing on.
    // This triggers the full graceful shutdown path: SSE broadcast,
    // service_manager.stop_all() (including MQTT retained topic cleanup),
    // then clean process exit.
    let shutdown_tx = state.shutdown_tx.clone();
    tokio::spawn(async move {
        tokio::time::sleep(tokio::time::Duration::from_millis(500)).await;
        shutdown_tx.notify_one();
    });

    (
        axum::http::StatusCode::ACCEPTED,
        Json(serde_json::json!({
            "message": "server is shutting down"
        })),
    )
}

// ============================================================================
// WebSocket
// ============================================================================

/// `GET /ws/events` — WebSocket upgrade for real-time event streaming.
async fn ws_events(ws: WebSocketUpgrade, State(state): State<Arc<MgmtState>>) -> impl IntoResponse {
    ws.on_upgrade(move |socket| handle_ws_connection(socket, state))
}

/// Handle a WebSocket connection.
///
/// Subscribes to both the event broadcast channel (device/service events)
/// and the log broadcast channel (tracing events). Supports per-client
/// log level filtering via `set_log_level` control messages.
///
/// ## Protocol (ADR-022)
///
/// Client → Server:
/// ```json
/// { "type": "set_log_level", "level": "DEBUG" }
/// ```
///
/// Server → Client:
/// - Device/service events (from `event_tx`)
/// - Log events (from `log_tx`, filtered by client's level)
async fn handle_ws_connection(mut socket: WebSocket, state: Arc<MgmtState>) {
    let mut event_rx = state.event_tx.subscribe();
    let mut log_rx = state.log_tx.subscribe();

    // Per-client log level filter. Default from config.
    let default_level = &state.config.dashboard.default_log_level;
    let mut client_level_priority =
        log_broadcast::level_priority_from_str(default_level).unwrap_or(2); // INFO

    // Send log backfill from the ring buffer.
    let backfill = state.log_ring_buffer.get_filtered(client_level_priority);
    for event in backfill {
        let json = event.to_ws_json();
        if socket.send(Message::Text(json.into())).await.is_err() {
            return; // Client disconnected during backfill.
        }
    }

    loop {
        tokio::select! {
            // Device/service events (always forwarded, no level filtering).
            event = event_rx.recv() => {
                match event {
                    Ok(msg) => {
                        if socket.send(Message::Text(msg.into())).await.is_err() {
                            break; // Client disconnected.
                        }
                    }
                    Err(broadcast::error::RecvError::Lagged(n)) => {
                        let _ = socket
                            .send(Message::Text(
                                serde_json::json!({
                                    "type": "warning",
                                    "message": format!("missed {} events", n)
                                }).to_string().into()
                            ))
                            .await;
                    }
                    Err(broadcast::error::RecvError::Closed) => break,
                }
            }
            // Log events (filtered by client's level).
            log_event = log_rx.recv() => {
                match log_event {
                    Ok(event) => {
                        if event.level_priority() >= client_level_priority {
                            let json = event.to_ws_json();
                            if socket.send(Message::Text(json.into())).await.is_err() {
                                break;
                            }
                        }
                    }
                    Err(broadcast::error::RecvError::Lagged(_)) => {
                        // Log events are less critical — silently skip.
                    }
                    Err(broadcast::error::RecvError::Closed) => break,
                }
            }
            // Client messages (control messages + ping/pong).
            msg = socket.recv() => {
                match msg {
                    Some(Ok(Message::Close(_))) | None => break,
                    Some(Ok(Message::Ping(data))) => {
                        let _ = socket.send(Message::Pong(data)).await;
                    }
                    Some(Ok(Message::Text(text))) => {
                        // Parse control messages.
                        if let Ok(parsed) = serde_json::from_str::<serde_json::Value>(&text) {
                            if parsed.get("type").and_then(|t| t.as_str()) == Some("set_log_level") {
                                if let Some(level) = parsed.get("level").and_then(|l| l.as_str()) {
                                    if let Some(priority) = log_broadcast::level_priority_from_str(level) {
                                        client_level_priority = priority;
                                        tracing::debug!(
                                            level = %level,
                                            "WebSocket client changed log level"
                                        );
                                    }
                                }
                            }
                        }
                    }
                    _ => {} // Ignore other messages.
                }
            }
        }
    }
}

// ============================================================================
// Log Management Endpoints (ADR-022)
// ============================================================================

/// `GET /mgmt/logs/download` — Download the server log file.
async fn mgmt_logs_download(
    State(state): State<Arc<MgmtState>>,
) -> impl IntoResponse {
    let log_path = &state.config.server.log_file;
    if log_path.is_empty() {
        return (
            axum::http::StatusCode::NOT_FOUND,
            [
                (axum::http::header::CONTENT_TYPE, "text/plain".to_string()),
                (axum::http::header::CONTENT_DISPOSITION, "inline".to_string()),
            ],
            "Log file not configured".to_string(),
        );
    }

    match tokio::fs::read_to_string(log_path).await {
        Ok(content) => (
            axum::http::StatusCode::OK,
            [
                (axum::http::header::CONTENT_TYPE, "text/plain; charset=utf-8".to_string()),
                (axum::http::header::CONTENT_DISPOSITION,
                 "attachment; filename=\"okfpga-server.log\"".to_string()),
            ],
            content,
        ),
        Err(e) => (
            axum::http::StatusCode::INTERNAL_SERVER_ERROR,
            [
                (axum::http::header::CONTENT_TYPE, "text/plain".to_string()),
                (axum::http::header::CONTENT_DISPOSITION, "inline".to_string()),
            ],
            format!("Could not read log file: {}", e),
        ),
    }
}

/// `GET /mgmt/logs/size` — Get the log file size in bytes.
async fn mgmt_logs_size(
    State(state): State<Arc<MgmtState>>,
) -> Json<serde_json::Value> {
    let log_path = &state.config.server.log_file;
    if log_path.is_empty() {
        return Json(serde_json::json!({ "size_bytes": 0, "configured": false }));
    }

    let size = tokio::fs::metadata(log_path)
        .await
        .map(|m| m.len())
        .unwrap_or(0);

    Json(serde_json::json!({
        "size_bytes": size,
        "configured": true,
        "path": log_path,
    }))
}

/// `DELETE /mgmt/logs` — Truncate the log file to zero bytes.
async fn mgmt_logs_clear(
    State(state): State<Arc<MgmtState>>,
) -> impl IntoResponse {
    let log_path = &state.config.server.log_file;
    if log_path.is_empty() {
        return (
            axum::http::StatusCode::NOT_FOUND,
            Json(serde_json::json!({
                "error": "Log file not configured"
            })),
        );
    }

    match tokio::fs::write(log_path, b"").await {
        Ok(()) => {
            info!("log file cleared by dashboard user");
            (
                axum::http::StatusCode::OK,
                Json(serde_json::json!({
                    "status": "cleared",
                    "path": log_path,
                })),
            )
        }
        Err(e) => (
            axum::http::StatusCode::INTERNAL_SERVER_ERROR,
            Json(serde_json::json!({
                "error": format!("Could not clear log file: {}", e)
            })),
        ),
    }
}

// ============================================================================
// Dashboard Static File Serving
// ============================================================================

/// Serve the web dashboard static files at the root path.
///
/// Only compiled when the `dashboard` feature is enabled.
#[cfg(feature = "dashboard")]
fn serve_dashboard(router: Router) -> Router {
    use tower_http::services::ServeDir;

    // Serve from the `web/` directory relative to the working directory.
    // In production this would be `include_dir!` or a configurable path.
    let serve_dir = ServeDir::new("web").append_index_html_on_directories(true);

    router.fallback_service(serve_dir)
}

// ============================================================================
// Helpers
// ============================================================================

/// Get a list of compiled feature names.
#[allow(clippy::vec_init_then_push)] // Can't use vec![] with #[cfg] gates.
fn compiled_features() -> Vec<String> {
    let mut features = Vec::new();
    #[cfg(feature = "rest")]
    features.push("rest".to_string());
    #[cfg(feature = "dashboard")]
    features.push("dashboard".to_string());
    #[cfg(feature = "grpc")]
    features.push("grpc".to_string());
    #[cfg(feature = "mqtt")]
    features.push("mqtt".to_string());
    #[cfg(feature = "mdns")]
    features.push("mdns".to_string());
    features
}

// ============================================================================
// Settings (Phase 28B/28C)
// ============================================================================

/// Response/request body for `/mgmt/settings`.
///
/// The `description` field is always present and editable.
/// The `endpoint_enforcement` field is only included when
/// `[dashboard] expose_settings = true`. When omitted from a PUT request
/// (or when expose_settings is false), the enforcement level is unchanged.
#[derive(Serialize, Deserialize)]
struct SettingsBody {
    /// Human-readable server description — always available.
    description: String,
    /// Endpoint enforcement level: "off", "warn", or "enforce".
    /// Only present when `[dashboard] expose_settings = true`.
    #[serde(skip_serializing_if = "Option::is_none")]
    endpoint_enforcement: Option<String>,
}

/// `GET /mgmt/settings` — Read current runtime settings.
///
/// The server description is always returned. The endpoint enforcement level
/// is only included when `[dashboard] expose_settings = true`.
async fn mgmt_get_settings(
    State(state): State<Arc<MgmtState>>,
) -> Result<Json<SettingsBody>, (axum::http::StatusCode, String)> {
    let cfg = state.app.config.read().unwrap();

    let description = if cfg.server.description.is_empty() {
        cfg.mdns.instance_name.clone()
    } else {
        cfg.server.description.clone()
    };

    let endpoint_enforcement = if cfg.dashboard.expose_settings {
        Some(cfg.access_control.endpoint_enforcement.clone())
    } else {
        None
    };

    Ok(Json(SettingsBody {
        description,
        endpoint_enforcement,
    }))
}

/// `PUT /mgmt/settings` — Update runtime settings and persist to TOML.
///
/// The server description can always be updated. The endpoint enforcement
/// level can only be changed when `[dashboard] expose_settings = true`;
/// if `expose_settings` is false, any `endpoint_enforcement` value in the
/// request body is silently ignored.
async fn mgmt_put_settings(
    State(state): State<Arc<MgmtState>>,
    Json(body): Json<SettingsBody>,
) -> Result<Json<SettingsBody>, (axum::http::StatusCode, String)> {
    let expose_settings = {
        let cfg = state.app.config.read().unwrap();
        cfg.dashboard.expose_settings
    };

    // Validate enforcement value if provided and allowed.
    let new_enforcement = if expose_settings {
        if let Some(ref val) = body.endpoint_enforcement {
            let lower = val.to_lowercase();
            if !matches!(lower.as_str(), "off" | "warn" | "enforce") {
                return Err((
                    axum::http::StatusCode::BAD_REQUEST,
                    format!("Invalid endpoint_enforcement value '{}'", val),
                ));
            }
            Some(lower)
        } else {
            None
        }
    } else {
        None // Silently ignore enforcement changes when expose_settings is off.
    };

    // Apply changes to the in-memory config.
    {
        let mut cfg = state.app.config.write().unwrap();
        cfg.server.description = body.description.clone();
        if let Some(ref enforcement) = new_enforcement {
            cfg.access_control.endpoint_enforcement = enforcement.clone();
            // Phase 32: Also update the runtime enforcement enum so that
            // middleware/handlers see the change immediately, without a
            // server restart.
            state.app.set_enforcement(
                crate::state::EndpointEnforcement::from_config(enforcement),
            );
        }
    }

    // Persist to TOML file.
    if let Some(ref path) = state.app.config_path {
        let cfg = state.app.config.read().unwrap();
        if let Err(e) = crate::config::save_config(&cfg, path) {
            warn!(error = %e, "Failed to persist settings to TOML");
            return Err((
                axum::http::StatusCode::INTERNAL_SERVER_ERROR,
                "Settings applied but could not be saved to disk".to_string(),
            ));
        }
    }

    info!(
        description = %body.description,
        enforcement_changed = new_enforcement.is_some(),
        "Server settings updated via management API"
    );

    // Broadcast event so the dashboard auto-refreshes.
    let _ = state.event_tx.send(
        r#"{"type":"settings_updated"}"#.to_string(),
    );

    // Return the response — enforcement included only if expose_settings is on.
    let cfg = state.app.config.read().unwrap();
    Ok(Json(SettingsBody {
        description: body.description,
        endpoint_enforcement: if expose_settings {
            Some(cfg.access_control.endpoint_enforcement.clone())
        } else {
            None
        },
    }))
}

// ============================================================================
// Catalog (Phase 28D)
// ============================================================================

/// `GET /mgmt/catalog` — Return the SYZYGY/device/bitfile catalog.
///
/// The dashboard fetches this once at init and caches it locally. Used to
/// render "About" links in device, SYZYGY, and bitfile detail rows.
async fn mgmt_get_catalog(
    State(state): State<Arc<MgmtState>>,
) -> Json<crate::config::CatalogConfig> {
    Json(state.config.catalog.clone())
}

// ============================================================================
// Tests
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    use axum::body::Body;
    use axum::http::Request;
    use tower::ServiceExt; // for `oneshot`

    use ok_fpga_core::auth;
    use ok_fpga_core::device::{DeviceInfo, DeviceInterface, UsbSpeed, FpgaVendor, DataportType};
    use ok_fpga_core::manager::DeviceManager;
    use ok_fpga_core::resource::ResourceRegistry;
    use ok_fpga_core::service_manager::ServiceManager;

    use crate::config::ServerConfig;
    use crate::state::AppState;

    /// Build a management router with default state (no devices, no hardware).
    ///
    /// This creates a fully functional router that can be tested via
    /// `Router::oneshot()` — no TCP port is needed.
    fn test_router() -> Router {
        let device_manager = Arc::new(DeviceManager::new());
        let resource_registry = Arc::new(ResourceRegistry::new());
        let service_manager = Arc::new(ServiceManager::new());
        let auth_provider: Arc<dyn auth::AuthProvider> =
            Arc::from(auth::create_provider(
                "none",
                vec![],
                None,
            ));
        let session_manager = Arc::new(auth::SessionManager::new(3600));

        let app_state = Arc::new(AppState::new(
            device_manager,
            resource_registry,
            service_manager,
            None,
            auth_provider,
            session_manager,
            Arc::new(ok_fpga_core::user::UserSessionRegistry::new()),
            crate::state::EndpointEnforcement::Off,
            ServerConfig::default(),
            None,
        ));

        let config = Arc::new(ServerConfig::default());
        let (event_tx, _) = broadcast::channel::<String>(16);
        let (log_tx, _) = broadcast::channel::<LogEvent>(16);
        let log_ring_buffer = Arc::new(LogRingBuffer::new());

        build_management_router(app_state, config, event_tx, log_tx, log_ring_buffer, Arc::new(tokio::sync::Notify::new()))
    }

    /// Build a router with a pre-populated device in the registry.
    fn test_router_with_device() -> Router {
        let device_manager = Arc::new(DeviceManager::new());

        // Add a fake device to the registry.
        let info = DeviceInfo {
            serial: "TEST1234AB".to_string(),
            product_name: "XEM7320-A75T".to_string(),
            device_id: "MyDevice".to_string(),
            product_id: 45,
            device_interface: DeviceInterface::Usb3,
            usb_speed: UsbSpeed::Super,
            device_major_version: 6,
            device_minor_version: 1,
            wire_width: 32,
            trigger_width: 32,
            pipe_width: 32,
            register_address_width: 32,
            register_data_width: 32,
            fpga_vendor: FpgaVendor::Xilinx,
            dataport: DataportType::Legacy,
        };
        device_manager.on_device_added(info);

        let resource_registry = Arc::new(ResourceRegistry::new());
        let service_manager = Arc::new(ServiceManager::new());
        let auth_provider: Arc<dyn auth::AuthProvider> =
            Arc::from(auth::create_provider(
                "none",
                vec![],
                None,
            ));
        let session_manager = Arc::new(auth::SessionManager::new(3600));

        let app_state = Arc::new(AppState::new(
            device_manager,
            resource_registry,
            service_manager,
            None,
            auth_provider,
            session_manager,
            Arc::new(ok_fpga_core::user::UserSessionRegistry::new()),
            crate::state::EndpointEnforcement::Off,
            ServerConfig::default(),
            None,
        ));

        let config = Arc::new(ServerConfig::default());
        let (event_tx, _) = broadcast::channel::<String>(16);
        let (log_tx, _) = broadcast::channel::<LogEvent>(16);
        let log_ring_buffer = Arc::new(LogRingBuffer::new());

        build_management_router(app_state, config, event_tx, log_tx, log_ring_buffer, Arc::new(tokio::sync::Notify::new()))
    }

    /// Helper: parse a JSON response body into a serde_json::Value.
    async fn json_body(response: axum::response::Response) -> serde_json::Value {
        let body = axum::body::to_bytes(response.into_body(), 1024 * 1024)
            .await
            .expect("failed to read body");
        serde_json::from_slice(&body).expect("response is not valid JSON")
    }

    // ------------------------------------------------------------------
    // GET /mgmt/status
    // ------------------------------------------------------------------

    #[tokio::test]
    async fn mgmt_status_returns_expected_shape() {
        let router = test_router();

        let response = router
            .oneshot(
                Request::builder()
                    .uri("/mgmt/status")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(response.status(), 200);

        let body = json_body(response).await;
        // Verify all expected fields are present.
        assert!(body["version"].is_string(), "version field missing");
        assert!(body["frontpanel_version"].is_string());
        assert!(body["platform"].is_string());
        assert!(body["uptime_seconds"].is_number());
        assert!(body["uptime_human"].is_string());
        assert!(body["connected_devices"].is_number());
        assert!(body["active_resources"].is_number());
        assert!(body["compiled_features"].is_array());
    }

    #[tokio::test]
    async fn mgmt_status_shows_zero_devices_when_empty() {
        let router = test_router();

        let response = router
            .oneshot(
                Request::builder()
                    .uri("/mgmt/status")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();

        let body = json_body(response).await;
        assert_eq!(body["connected_devices"], 0);
        assert_eq!(body["active_resources"], 0);
    }

    #[tokio::test]
    async fn mgmt_status_shows_one_device_when_populated() {
        let router = test_router_with_device();

        let response = router
            .oneshot(
                Request::builder()
                    .uri("/mgmt/status")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();

        let body = json_body(response).await;
        assert_eq!(body["connected_devices"], 1);
    }

    // ------------------------------------------------------------------
    // GET /mgmt/devices
    // ------------------------------------------------------------------

    #[tokio::test]
    async fn mgmt_devices_returns_empty_list_when_no_hardware() {
        let router = test_router();

        let response = router
            .oneshot(
                Request::builder()
                    .uri("/mgmt/devices")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(response.status(), 200);

        let body = json_body(response).await;
        let devices = body.as_array().expect("response should be an array");
        assert!(devices.is_empty(), "expected empty device list");
    }

    #[tokio::test]
    async fn mgmt_devices_returns_device_info_when_populated() {
        let router = test_router_with_device();

        let response = router
            .oneshot(
                Request::builder()
                    .uri("/mgmt/devices")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(response.status(), 200);

        let body = json_body(response).await;
        let devices = body.as_array().expect("response should be an array");
        assert_eq!(devices.len(), 1, "expected exactly one device");

        let dev = &devices[0];
        assert_eq!(dev["serial"], "TEST1234AB");
        assert_eq!(dev["product_name"], "XEM7320-A75T");
        assert_eq!(dev["product_id"], 45);
        assert_eq!(dev["interface"], "USB 3.0");
        assert_eq!(dev["device_id"], "MyDevice");
        assert_eq!(dev["firmware_major"], 6);
        assert_eq!(dev["firmware_minor"], 1);
        // State should be "Connected" since no bitfile is loaded.
        assert!(
            dev["state"].as_str().unwrap().contains("Connected"),
            "expected Connected state, got: {}",
            dev["state"]
        );
    }

    // ------------------------------------------------------------------
    // GET /mgmt/services
    // ------------------------------------------------------------------

    #[tokio::test]
    async fn mgmt_services_returns_service_list() {
        let router = test_router();

        let response = router
            .oneshot(
                Request::builder()
                    .uri("/mgmt/services")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(response.status(), 200);

        let body = json_body(response).await;
        let services = body.as_array().expect("response should be an array");
        // The ServiceManager starts with registered services (REST, gRPC, etc.)
        // Even with no features, the manager has entries for known services.
        for svc in services {
            assert!(svc["name"].is_string(), "service missing name");
            assert!(svc["status"].is_string(), "service missing status");
        }
    }

    // ------------------------------------------------------------------
    // GET /mgmt/config
    // ------------------------------------------------------------------

    #[tokio::test]
    async fn mgmt_config_returns_sanitized_config() {
        let router = test_router();

        let response = router
            .oneshot(
                Request::builder()
                    .uri("/mgmt/config")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(response.status(), 200);

        let body = json_body(response).await;
        // Verify expected config fields are present.
        assert!(body["management_port"].is_number());
        assert!(body["log_level"].is_string());
        assert!(body["rest_enabled"].is_boolean());
        assert!(body["rest_port"].is_number());
        assert!(body["grpc_enabled"].is_boolean());
        assert!(body["grpc_port"].is_number());
        assert!(body["mqtt_enabled"].is_boolean());
        assert!(body["mdns_enabled"].is_boolean());
        assert!(body["dashboard_enabled"].is_boolean());
    }

    // ------------------------------------------------------------------
    // 404 for unknown routes
    // ------------------------------------------------------------------

    #[tokio::test]
    async fn unknown_mgmt_route_returns_404() {
        let router = test_router();

        let response = router
            .oneshot(
                Request::builder()
                    .uri("/mgmt/nonexistent")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();

        // Axum returns 404 for unmatched routes.
        assert_eq!(response.status(), 404);
    }

    // ------------------------------------------------------------------
    // GET /mgmt/resources  (Phase 25-Fix-G, F6)
    // ------------------------------------------------------------------

    #[tokio::test]
    async fn mgmt_resources_returns_empty_list() {
        let router = test_router();

        let response = router
            .oneshot(
                Request::builder()
                    .uri("/mgmt/resources")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(response.status(), 200);
        let body = axum::body::to_bytes(response.into_body(), usize::MAX).await.unwrap();
        let resources: Vec<serde_json::Value> = serde_json::from_slice(&body).unwrap();
        assert!(resources.is_empty());
    }

    #[tokio::test]
    async fn mgmt_resources_returns_registered_resource() {
        use ok_fpga_core::resource::{Resource, AccessMode};

        let device_manager = Arc::new(DeviceManager::new());
        let resource_registry = Arc::new(ResourceRegistry::new());
        let service_manager = Arc::new(ServiceManager::new());
        let auth_provider: Arc<dyn auth::AuthProvider> =
            Arc::from(auth::create_provider("none", vec![], None));
        let session_manager = Arc::new(auth::SessionManager::new(3600));

        // Register a resource.
        let resource = Resource::new(
            "res-001".to_string(),
            "Counter Resource".to_string(),
            None,
            "SN001".to_string(),
            AccessMode::Shared,
        );
        resource_registry.register(resource);

        let app_state = Arc::new(AppState::new(
            device_manager,
            resource_registry,
            service_manager,
            None,
            auth_provider,
            session_manager,
            Arc::new(ok_fpga_core::user::UserSessionRegistry::new()),
            crate::state::EndpointEnforcement::Off,
            ServerConfig::default(),
            None,
        ));
        let config = Arc::new(ServerConfig::default());
        let (event_tx, _) = broadcast::channel::<String>(16);
        let (log_tx, _) = broadcast::channel::<LogEvent>(16);
        let log_ring_buffer = Arc::new(LogRingBuffer::new());
        let router = build_management_router(app_state, config, event_tx, log_tx, log_ring_buffer, Arc::new(tokio::sync::Notify::new()));

        let response = router
            .oneshot(
                Request::builder()
                    .uri("/mgmt/resources")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(response.status(), 200);
        let body = axum::body::to_bytes(response.into_body(), usize::MAX).await.unwrap();
        let resources: Vec<serde_json::Value> = serde_json::from_slice(&body).unwrap();
        assert_eq!(resources.len(), 1);
        assert_eq!(resources[0]["id"], "res-001");
        assert_eq!(resources[0]["name"], "Counter Resource");
        assert_eq!(resources[0]["device_serial"], "SN001");
        assert_eq!(resources[0]["state"], "Available");
    }

    // ------------------------------------------------------------------
    // GET /mgmt/users  (Phase 25-Fix-G, F7)
    // ------------------------------------------------------------------

    #[tokio::test]
    async fn mgmt_users_returns_empty_list() {
        let router = test_router();

        let response = router
            .oneshot(
                Request::builder()
                    .uri("/mgmt/users")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(response.status(), 200);
        let body = axum::body::to_bytes(response.into_body(), usize::MAX).await.unwrap();
        let users: Vec<serde_json::Value> = serde_json::from_slice(&body).unwrap();
        assert!(users.is_empty());
    }

    #[tokio::test]
    async fn mgmt_users_returns_active_session() {
        let device_manager = Arc::new(DeviceManager::new());
        let resource_registry = Arc::new(ResourceRegistry::new());
        let service_manager = Arc::new(ServiceManager::new());
        let auth_provider: Arc<dyn auth::AuthProvider> =
            Arc::from(auth::create_provider("none", vec![], None));
        let session_manager = Arc::new(auth::SessionManager::new(3600));
        let user_sessions = Arc::new(ok_fpga_core::user::UserSessionRegistry::new());

        // Touch a user session so it exists.
        user_sessions.touch("alice");

        let app_state = Arc::new(AppState::new(
            device_manager,
            resource_registry,
            service_manager,
            None,
            auth_provider,
            session_manager,
            user_sessions,
            crate::state::EndpointEnforcement::Off,
            ServerConfig::default(),
            None,
        ));
        let config = Arc::new(ServerConfig::default());
        let (event_tx, _) = broadcast::channel::<String>(16);
        let (log_tx, _) = broadcast::channel::<LogEvent>(16);
        let log_ring_buffer = Arc::new(LogRingBuffer::new());
        let router = build_management_router(app_state, config, event_tx, log_tx, log_ring_buffer, Arc::new(tokio::sync::Notify::new()));

        let response = router
            .oneshot(
                Request::builder()
                    .uri("/mgmt/users")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(response.status(), 200);
        let body = axum::body::to_bytes(response.into_body(), usize::MAX).await.unwrap();
        let users: Vec<serde_json::Value> = serde_json::from_slice(&body).unwrap();
        assert_eq!(users.len(), 1);
        assert_eq!(users[0]["user_id"], "alice");
        assert!(users[0]["last_seen_at"].as_str().is_some());
    }
}


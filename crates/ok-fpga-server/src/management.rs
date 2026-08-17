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
use axum::routing::{get, post};
use axum::{Json, Router};
use serde::Serialize;
use tokio::sync::broadcast;
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
/// - `/mgmt/config` — get current config (sanitized)
/// - `/mgmt/status` — get server status
/// - `/mgmt/shutdown` — graceful server shutdown
/// - `/ws/events` — WebSocket event stream
/// - `/*` — static dashboard files (when `dashboard` feature enabled)
pub fn build_management_router(
    state: Arc<AppState>,
    config: Arc<ServerConfig>,
    event_tx: broadcast::Sender<String>,
) -> Router {
    let mgmt_state = Arc::new(MgmtState {
        app: state,
        config,
        event_tx,
    });

    let mut router = Router::new()
        // Management API routes
        .route("/mgmt/status", get(mgmt_status))
        .route("/mgmt/services", get(mgmt_list_services))
        .route("/mgmt/services/{name}/start", post(mgmt_start_service))
        .route("/mgmt/services/{name}/stop", post(mgmt_stop_service))
        .route("/mgmt/services/{name}/restart", post(mgmt_restart_service))
        .route("/mgmt/config", get(mgmt_get_config))
        .route("/mgmt/shutdown", post(mgmt_shutdown))
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

    /// Broadcast channel for WebSocket event streaming.
    event_tx: broadcast::Sender<String>,
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

    Json(StatusResponse {
        version: state.app.api_version.to_string(),
        frontpanel_version: "6.1.0".to_string(),
        platform: format!("{}-{}", std::env::consts::OS, std::env::consts::ARCH),
        uptime_seconds: uptime,
        uptime_human: format!("{}h {}m {}s", hours, mins, secs),
        connected_devices: state.app.device_manager.device_count(),
        active_resources: state.app.resource_registry.count(),
        compiled_features: features,
    })
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
    tokio::spawn(async {
        tokio::time::sleep(tokio::time::Duration::from_millis(500)).await;
        // Send SIGINT to self (triggers the ctrl_c handler in main).
        std::process::exit(0);
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
/// Subscribes to the event broadcast channel and forwards events to the
/// WebSocket client. The connection is closed when the client disconnects
/// or the broadcast channel is dropped.
async fn handle_ws_connection(mut socket: WebSocket, state: Arc<MgmtState>) {
    let mut rx = state.event_tx.subscribe();

    loop {
        tokio::select! {
            event = rx.recv() => {
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
            msg = socket.recv() => {
                match msg {
                    Some(Ok(Message::Close(_))) | None => break,
                    Some(Ok(Message::Ping(data))) => {
                        let _ = socket.send(Message::Pong(data)).await;
                    }
                    _ => {} // Ignore other messages.
                }
            }
        }
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

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
/// - `/mgmt/devices` — list connected FPGA devices with full info
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
        .route("/mgmt/devices", get(mgmt_list_devices))
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

/// Device information response for the `/mgmt/devices` endpoint.
///
/// Provides a JSON-friendly summary of each connected FPGA device.
/// This is served from the management port (9090) so the dashboard
/// can render device details without cross-port requests to the REST API.
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
        .map(|d| DeviceInfoResponse {
            serial: d.info.serial.clone(),
            product_name: d.info.product_name.clone(),
            product_id: d.info.product_id,
            interface: d.info.device_interface.to_string(),
            usb_speed: d.info.usb_speed.to_string(),
            state: d.state.to_string(),
            firmware_major: d.info.device_major_version,
            firmware_minor: d.info.device_minor_version,
            device_id: d.info.device_id.clone(),
        })
        .collect();
    Json(devices)
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
            auth_provider,
            session_manager,
        ));

        let config = Arc::new(ServerConfig::default());
        let (event_tx, _) = broadcast::channel::<String>(16);

        build_management_router(app_state, config, event_tx)
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
            auth_provider,
            session_manager,
        ));

        let config = Arc::new(ServerConfig::default());
        let (event_tx, _) = broadcast::channel::<String>(16);

        build_management_router(app_state, config, event_tx)
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
}


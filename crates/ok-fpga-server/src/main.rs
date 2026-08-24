//! # ok-fpga-server — REST + gRPC Server and Web Dashboard
//!
//! This is the main binary crate for the ok-FPGA-Server. When you run it,
//! it starts a server that:
//!
//! 1. **Loads configuration** from a TOML file (default: `./config.toml`)
//! 2. **Initializes structured logging** via `tracing` with the configured level
//! 3. **Detects connected Opal Kelly FPGAs** via the FrontPanelManager
//!    hot-plug callback system
//! 4. **Starts configured services** (REST, gRPC, MQTT, mDNS) via the
//!    ServiceManager
//! 5. **Serves a web dashboard** on the management port (default 9090)
//!
//! ## Configuration
//!
//! See [`config`] module and the sample `config.toml` at the project root.
//!
//! ## Feature Flags (ADR-010)
//!
//! ```text
//! default = ["rest", "dashboard"]
//!
//! rest      — REST API (Axum)
//! dashboard — Web dashboard on management port
//! grpc      — gRPC API (Tonic)
//! mqtt      — MQTT status publisher
//! mdns      — mDNS service discovery
//! full      — All features enabled
//! ```
//!
//! ## Architecture
//!
//! ```text
//! ┌───────────────────────────────────────────────┐
//! │          Management Port :9090                 │
//! │          (always on, dashboard)                │
//! └───────────────────┬───────────────────────────┘
//!                     │
//! ┌───────────────────┴───────────────────────────┐
//! │              ServiceManager                    │
//! ├─────────────┬──────────────┬──────────────────┤
//! │ REST :8080  │ gRPC :50051  │ MQTT / mDNS      │
//! │ [feature]   │ [feature]    │ [features]        │
//! └──────┬──────┴──────┬───────┴──────────────────┘
//!        └─────────────┴──────┐
//!                             │
//!                ┌────────────┴────────────┐
//!                │      ok-fpga-core       │
//!                │   (domain logic)        │
//!                └────────────┬────────────┘
//!                             │
//!                ┌────────────┴────────────┐
//!                │      ok-fpga-ffi        │
//!                │   (C API bindings)      │
//!                └────────────┬────────────┘
//!                             │
//!                ┌────────────┴────────────┐
//!                │   okFrontPanel DLL      │
//!                └─────────────────────────┘
//! ```

// ============================================================================
// Module declarations
// ============================================================================

/// Server configuration (TOML loading, env overrides, validation).
mod config;

/// Shared application state (device manager, service manager, etc.)
mod state;

/// Management server (always-on, serves dashboard + mgmt API on port 9090).
mod management;

/// Device scanner — loads FrontPanel DLL and enumerates connected FPGAs.
/// Used for both initial scan and periodic polling (ADR-013).
mod scanner;

/// Device handle pool — persistent per-device FFI handles (ADR-018).
/// Provides `DeviceHandlePool` for REST/gRPC handlers to perform FFI calls.
mod handle_pool;

/// Device monitor — FrontPanelManager hot-plug callbacks (ADR-002 Phase 2).
/// NOTE: FrontPanelManager callbacks only fire during initial enumeration on
/// Windows, not for subsequent hot-plug events. The polling approach in
/// `device_poll_loop` is used instead. This module is retained for future
/// investigation and potential cross-platform use.
#[allow(dead_code)]
mod device_monitor;

/// Log broadcast layer — captures tracing events and broadcasts to WebSocket
/// clients with per-client level filtering (ADR-022).
mod log_broadcast;

/// REST API server module — only compiled when `rest` feature is enabled.
#[cfg(feature = "rest")]
mod rest;

/// gRPC API server module — only compiled when `grpc` feature is enabled.
#[cfg(feature = "grpc")]
mod grpc;

/// MQTT status publisher — only compiled when `mqtt` feature is enabled.
#[cfg(feature = "mqtt")]
mod mqtt;

/// mDNS/DNS-SD service discovery — only compiled when `mdns` feature is enabled.
#[cfg(feature = "mdns")]
mod mdns;

/// Background user session sweep task (ADR-020 §5).
mod sweep;

use std::sync::Arc;

use ok_fpga_core::manager::DeviceManager;
use ok_fpga_core::resource::ResourceRegistry;
use ok_fpga_core::service_manager::ServiceManager;
use tracing::{info, warn};

use config::ServerConfig;
use state::AppState;

/// The server version, derived from the workspace Cargo.toml.
const VERSION: &str = env!("CARGO_PKG_VERSION");

/// Entry point for the ok-FPGA-Server.
///
/// 1. Loads configuration from TOML file + env overrides
/// 2. Initializes tracing with the configured log level
/// 3. Creates core domain components
/// 4. Registers and starts services based on config and compiled features
/// 5. Waits for Ctrl+C, then gracefully shuts down
#[tokio::main]
async fn main() -> anyhow::Result<()> {
    // Load configuration (TOML file + env overrides + feature validation).
    let (cfg, config_path) = config::load_config();

    // Initialize tracing (structured logging) with the configured log level.
    // Returns the broadcast handles needed for the dashboard log viewer.
    let (log_tx, log_ring_buffer) = init_tracing(&cfg);

    // Print startup banner.
    print_banner(&cfg);

    info!(
        config_log_level = %cfg.server.log_level,
        management_port = cfg.server.management_port,
        "configuration loaded"
    );

    // Initialize core domain components.
    let device_manager = Arc::new(DeviceManager::new());
    let resource_registry = Arc::new(ResourceRegistry::new());
    let service_manager = Arc::new(ServiceManager::new());

    // Create the event broadcast channel for WebSocket (needed by both
    // device polling and management server).
    let (event_tx, _) = tokio::sync::broadcast::channel::<String>(256);

    // Device discovery: initial scan + persistent handle pool (Phase 19).
    //
    // The FrontPanelManager callback API only fires OnDeviceAdded during
    // initial enumeration (StartMonitoring), not for subsequent hot-plug
    // events on Windows. Instead, we use the reliable DeviceScanner with
    // periodic polling to detect connect/disconnect.
    let handle_pool = run_scanner_with_pool(&device_manager).await;

    // Spawn periodic device polling task (every 2 seconds).
    {
        let poll_dm = device_manager.clone();
        let poll_tx = event_tx.clone();
        let poll_pool = handle_pool.clone();
        tokio::spawn(async move {
            device_poll_loop(poll_dm, poll_tx, poll_pool).await;
        });
        info!("device polling task started (2s interval)");
    }

    // Create the authentication provider from config (ADR-008).
    let auth_provider: Arc<dyn ok_fpga_core::auth::AuthProvider> =
        Arc::from(ok_fpga_core::auth::create_provider(
            &cfg.auth.provider,
            cfg.auth.local.users.clone(),
            Some(cfg.auth.ldap.clone()),
        ));
    info!(
        provider = auth_provider.provider_name(),
        required = auth_provider.is_required(),
        "authentication provider initialized"
    );

    // Create the session manager.
    let session_manager = Arc::new(ok_fpga_core::auth::SessionManager::new(
        cfg.auth.session_expiry_secs,
    ));

    // Create the user session registry for liveness tracking (ADR-020 §5).
    let user_sessions = Arc::new(ok_fpga_core::user::UserSessionRegistry::new());

    // Create shared application state.
    // Parse the endpoint enforcement level from config (ADR-024 §1).
    let enforcement = crate::state::EndpointEnforcement::from_config(
        &cfg.access_control.endpoint_enforcement,
    );
    info!("Endpoint access enforcement: {}", enforcement);

    let mut app_state_inner = AppState::new(
        device_manager.clone(),
        resource_registry.clone(),
        service_manager.clone(),
        handle_pool,
        auth_provider,
        session_manager,
        user_sessions,
        enforcement,
        cfg.clone(),
        config_path,
    );
    // Wire the event broadcast channel so resource/user lifecycle events
    // reach the WebSocket dashboard (Phase 25-Fix-H, F9).
    app_state_inner.event_tx = Some(event_tx.clone());
    let app_state = Arc::new(app_state_inner);

    // Wrap config in Arc (needed by management server and mDNS service).
    let cfg_arc = Arc::new(cfg);

    // Register available services based on compiled features and config.
    register_services(&service_manager, &app_state, &cfg_arc);

    // Log the feature flag status.
    log_feature_status();

    // Start all enabled services.
    service_manager.start_enabled().await;

    // Log the active services.
    for svc_info in service_manager.list() {
        info!(
            service = %svc_info.name,
            status = %svc_info.status,
            compiled = svc_info.compiled,
            enabled = svc_info.enabled,
            "service state"
        );
    }

    // Start the management server (always-on, serves dashboard + mgmt API).
    start_management_server(
        &app_state,
        &cfg_arc,
        event_tx.clone(),
        log_tx,
        log_ring_buffer,
    )
    .await;

    // Start the user session sweep task (auto-releases expired users' resources).
    sweep::spawn_sweep_task(
        app_state.user_sessions.clone(),
        app_state.resource_registry.clone(),
        app_state.device_manager.clone(),
        cfg_arc.server.user_timeout_secs,
        cfg_arc.server.user_sweep_interval_secs,
        Some(event_tx.clone()),
    );

    info!("ok-FPGA-Server is ready. Press Ctrl+C to shut down.");

    // Wait for shutdown signal.
    tokio::signal::ctrl_c().await?;
    info!("shutdown signal received");

    // Broadcast ServerShutdown event for SSE subscribers (Phase 39, ADR-030).
    // This fires *before* stopping services so connected clients receive it.
    let _ = app_state.device_manager.broadcast(
        ok_fpga_core::manager::DeviceEvent::ServerShutdown {
            reason: "signal".to_string(),
            timestamp: chrono::Utc::now(),
        },
    );

    // Gracefully stop all services.
    service_manager.stop_all().await;

    info!("ok-FPGA-Server shut down cleanly.");
    Ok(())
}

/// Perform the initial one-shot device scan AND create the DeviceHandlePool.
///
/// This replaces the old `run_scanner_fallback` — it not only populates the
/// registry but also opens persistent FFI handles for each discovered device.
async fn run_scanner_with_pool(
    device_manager: &Arc<DeviceManager>,
) -> Option<Arc<handle_pool::DeviceHandlePool>> {
    let scanner = match scanner::DeviceScanner::try_new() {
        Some(s) => s,
        None => {
            warn!("FrontPanel DLL not available - running without hardware access");
            return None;
        }
    };

    // Create the handle pool with the same DLL instance the scanner loaded.
    let pool = Arc::new(handle_pool::DeviceHandlePool::new(scanner.dll()));

    let scanned = scanner.scan_devices();
    info!(count = scanned.len(), "initial device scan complete");

    for device_info in scanned {
        let serial = device_info.serial.clone();
        info!(
            serial = %device_info.serial,
            product = %device_info.product_name,
            interface = %device_info.device_interface,
            "discovered FPGA device (initial scan)"
        );
        device_manager.on_device_added(device_info);

        // Open a persistent handle for this device in the pool.
        if let Err(e) = pool.open(&serial).await {
            warn!(
                serial = %serial,
                error = %e,
                "could not open persistent handle — device may have disconnected"
            );
        }
    }

    Some(pool)
}

/// Periodic device polling loop — scans for connected devices every 2 seconds
/// and synchronizes the `DeviceManager` registry with reality.
///
/// ## How it works (Phase 23 rev2)
///
/// 1. **Disconnect detection** (pool-based): For each device already in the
///    handle pool, call `pool.is_alive()` which uses `okFrontPanel_IsOpen()`
///    on the existing handle. This avoids creating a new `okFrontPanelDevices`
///    snapshot which interfered with open handles on Windows.
///
/// 2. **Discovery** (scan-based): Call `scan_serials()` to find physically
///    connected devices. Only devices **not** already in the registry are
///    processed as new connections.
///
/// This two-phase approach avoids the circular dependency that caused the
/// connect/disconnect cycling bug:
/// - Phase 23 rev1 used `scan_serials()` for both disconnect and discovery,
///   but `okFrontPanelDevices_Construct()` on Windows interferes with handles
///   already opened by the pool → device appears absent → disconnect →
///   pool closes handle → next scan finds it → connect → cycle repeats.
async fn device_poll_loop(
    device_manager: Arc<DeviceManager>,
    event_tx: tokio::sync::broadcast::Sender<String>,
    handle_pool: Option<Arc<handle_pool::DeviceHandlePool>>,
) {
    let scanner = match scanner::DeviceScanner::try_new() {
        Some(s) => s,
        None => {
            warn!("device polling: FrontPanel DLL not available, polling disabled");
            return;
        }
    };

    let mut interval = tokio::time::interval(std::time::Duration::from_secs(2));
    // Don't fire immediately — the initial scan already ran.
    interval.tick().await;

    loop {
        interval.tick().await;

        // === Phase 1: Disconnect detection via pool liveness ===
        //
        // For each device in the registry, check if its handle pool entry
        // is still alive (fp_is_open returns true). If not, the device has
        // been physically disconnected.
        if let Some(ref pool) = handle_pool {
            let current_devices = device_manager.registry().list();
            for device in &current_devices {
                let serial = &device.info.serial;

                if !pool.is_alive(serial).await {
                    info!(
                        serial = %serial,
                        "FPGA device disconnected (hot-unplug detected by polling)"
                    );
                    device_manager.on_device_removed(serial);

                    // Close the persistent handle for this device.
                    pool.close(serial).await;

                    // Broadcast WebSocket event.
                    let event = serde_json::json!({
                        "type": "device_disconnected",
                        "serial": serial,
                    });
                    let _ = event_tx.send(event.to_string());
                }
            }
        }

        // === Phase 2: Discovery via scan_serials ===
        //
        // Enumerate physically connected serials. Only process devices
        // that are NOT already in the registry (genuinely new connections).
        let connected_serials = scanner.scan_serials();

        let registry_serials: std::collections::HashSet<String> = device_manager
            .registry()
            .list()
            .iter()
            .map(|d| d.info.serial.clone())
            .collect();

        let new_serials: Vec<&String> = connected_serials
            .iter()
            .filter(|s| !registry_serials.contains(*s))
            .collect();

        if !new_serials.is_empty() {
            // Perform a full scan to get DeviceInfo for the new devices.
            // This opens each device briefly to read okTDeviceInfo.
            let scanned = scanner.scan_devices();

            for device_info in &scanned {
                if new_serials.contains(&&device_info.serial) {
                    info!(
                        serial = %device_info.serial,
                        product = %device_info.product_name,
                        interface = %device_info.device_interface,
                        "FPGA device connected (hot-plug detected by polling)"
                    );
                    device_manager.on_device_added(device_info.clone());

                    // Open a persistent handle for this device in the pool.
                    if let Some(ref pool) = handle_pool {
                        if let Err(e) = pool.open(&device_info.serial).await {
                            warn!(
                                serial = %device_info.serial,
                                error = %e,
                                "could not open persistent handle for hot-plugged device"
                            );
                        }
                    }

                    // Broadcast WebSocket event.
                    let event = serde_json::json!({
                        "type": "device_connected",
                        "serial": device_info.serial,
                        "product": device_info.product_name,
                        "interface": device_info.device_interface.to_string(),
                    });
                    let _ = event_tx.send(event.to_string());
                }
            }
        }
    }
}

/// Start the management server (always-on).
///
/// This server runs for the entire lifetime of the process and cannot be
/// stopped or restarted. It serves:
/// - `/mgmt/*` — management API (service lifecycle, config, shutdown)
/// - `/ws/events` — WebSocket event streaming
/// - `/*` — web dashboard static files (when `dashboard` feature enabled)
async fn start_management_server(
    app_state: &Arc<AppState>,
    config: &Arc<ServerConfig>,
    event_tx: tokio::sync::broadcast::Sender<String>,
    log_tx: tokio::sync::broadcast::Sender<log_broadcast::LogEvent>,
    log_ring_buffer: std::sync::Arc<log_broadcast::LogRingBuffer>,
) {
    let mgmt_addr: std::net::SocketAddr = format!(
        "{}:{}",
        config.server.management_bind, config.server.management_port
    )
    .parse()
    .expect("valid management bind address from config");

    let router = management::build_management_router(
        app_state.clone(),
        config.clone(),
        event_tx,
        log_tx,
        log_ring_buffer,
    );

    let listener = match tokio::net::TcpListener::bind(mgmt_addr).await {
        Ok(l) => l,
        Err(e) => {
            tracing::error!(address = %mgmt_addr, error = %e, "failed to bind management server");
            return;
        }
    };

    info!(
        address = %mgmt_addr,
        "management server started (dashboard + mgmt API)"
    );

    tokio::spawn(async move {
        if let Err(e) = axum::serve(listener, router).await {
            tracing::error!(error = %e, "management server error");
        }
    });
}

/// Initialize the `tracing` subscriber with the configured log level.
///
/// The log level can come from (in priority order):
/// 1. `RUST_LOG` environment variable (standard tracing behavior)
/// 2. `OKFPGA_LOG_LEVEL` environment variable (via config)
/// 3. `server.log_level` in the TOML config
/// 4. Built-in default: `"info"`
///
/// ## Subscriber Stack (ADR-022)
///
/// The subscriber is a layered registry with three layers:
/// 1. **fmt::Layer** — stdout output, filtered by config level
/// 2. **File appender layer** — writes to `server.log_file`, filtered by config level
/// 3. **WebSocketBroadcastLayer** — captures ALL events (TRACE+) for dashboard streaming
///
/// Returns `(log_tx, ring_buffer)` — the broadcast channel and ring buffer
/// needed by the management server's WebSocket handler.
fn init_tracing(
    cfg: &ServerConfig,
) -> (
    tokio::sync::broadcast::Sender<log_broadcast::LogEvent>,
    std::sync::Arc<log_broadcast::LogRingBuffer>,
) {
    use tracing_subscriber::prelude::*;

    // If RUST_LOG is set, use it directly (standard tracing convention).
    // Otherwise, use the config's log_level.
    let filter = tracing_subscriber::EnvFilter::try_from_default_env()
        .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new(&cfg.server.log_level));

    // Layer 1: stdout output (existing behavior).
    let stdout_layer = tracing_subscriber::fmt::layer()
        .with_target(true)
        .with_thread_ids(false)
        .with_file(false)
        .with_line_number(false)
        .with_filter(filter);

    // Layer 2: File appender (ADR-022).
    // Only created if log_file is non-empty.
    let file_layer = if !cfg.server.log_file.is_empty() {
        let log_path = std::path::Path::new(&cfg.server.log_file);

        // Ensure the parent directory exists.
        if let Some(parent) = log_path.parent() {
            if !parent.as_os_str().is_empty() {
                let _ = std::fs::create_dir_all(parent);
            }
        }

        // Open the log file in append mode.
        match std::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(log_path)
        {
            Ok(file) => {
                let file_filter = tracing_subscriber::EnvFilter::try_from_default_env()
                    .unwrap_or_else(|_| {
                        tracing_subscriber::EnvFilter::new(&cfg.server.log_level)
                    });
                Some(
                    tracing_subscriber::fmt::layer()
                        .with_target(true)
                        .with_ansi(false)
                        .with_writer(std::sync::Mutex::new(file))
                        .with_filter(file_filter),
                )
            }
            Err(e) => {
                eprintln!(
                    "WARNING: could not open log file '{}': {}",
                    cfg.server.log_file, e
                );
                None
            }
        }
    } else {
        None
    };

    // Layer 3: WebSocket broadcast (ADR-022).
    // Captures ALL events at TRACE level — filtering is per-client.
    let (broadcast_layer, log_tx, ring_buffer) = log_broadcast::WebSocketBroadcastLayer::new();

    // Compose the subscriber stack.
    tracing_subscriber::registry()
        .with(stdout_layer)
        .with(file_layer)
        .with(broadcast_layer)
        .init();

    (log_tx, ring_buffer)
}

/// Register compiled services with the ServiceManager using config values.
///
/// Each service is registered with:
/// - Its bind address and port from config
/// - Its `enabled` flag from config (only matters if the feature is compiled)
///
/// Services whose Cargo features are NOT compiled cannot be registered.
fn register_services(
    service_manager: &Arc<ServiceManager>,
    app_state: &Arc<AppState>,
    cfg: &Arc<ServerConfig>,
) {
    // --- REST (default feature) ---
    #[cfg(feature = "rest")]
    {
        let rest_addr: std::net::SocketAddr = format!("{}:{}", cfg.rest.bind, cfg.rest.port)
            .parse()
            .expect("valid REST bind address from config");
        let rest_service = Arc::new(rest::managed::RestManagedService::new(
            rest_addr,
            app_state.clone(),
        ));
        service_manager.register(rest_service, cfg.rest.enabled);
        info!(
            bind = %cfg.rest.bind,
            port = cfg.rest.port,
            enabled = cfg.rest.enabled,
            "registered REST service"
        );
    }

    // --- gRPC (opt-in feature) ---
    #[cfg(feature = "grpc")]
    {
        let grpc_addr: std::net::SocketAddr = format!("{}:{}", cfg.grpc.bind, cfg.grpc.port)
            .parse()
            .expect("valid gRPC bind address from config");
        let grpc_service = Arc::new(grpc::managed::GrpcManagedService::new(
            grpc_addr,
            app_state.clone(),
        ));
        service_manager.register(grpc_service, cfg.grpc.enabled);
        info!(
            bind = %cfg.grpc.bind,
            port = cfg.grpc.port,
            enabled = cfg.grpc.enabled,
            "registered gRPC service"
        );
    }

    // --- MQTT (opt-in feature, ADR-011) ---
    #[cfg(feature = "mqtt")]
    {
        let mqtt_service = Arc::new(mqtt::MqttPublisher::new(
            cfg.mqtt.clone(),
            app_state.clone(),
        ));
        service_manager.register(mqtt_service, cfg.mqtt.enabled);
        info!(
            broker = %cfg.mqtt.broker_url,
            topic_prefix = %cfg.mqtt.topic_prefix,
            enabled = cfg.mqtt.enabled,
            "registered MQTT service"
        );
    }

    // --- mDNS (opt-in feature, ADR-012) ---
    #[cfg(feature = "mdns")]
    {
        let mdns_service = Arc::new(mdns::MdnsDiscoveryService::new(
            cfg.clone(),
            app_state.clone(),
        ));
        service_manager.register(mdns_service, cfg.mdns.enabled);
        info!(
            instance_name = %cfg.mdns.instance_name,
            enabled = cfg.mdns.enabled,
            "registered mDNS service"
        );
    }
}

/// Log which features are compiled into this binary.
fn log_feature_status() {
    let features: Vec<&str> = vec![
        #[cfg(feature = "rest")]
        "rest",
        #[cfg(feature = "dashboard")]
        "dashboard",
        #[cfg(feature = "grpc")]
        "grpc",
        #[cfg(feature = "mqtt")]
        "mqtt",
        #[cfg(feature = "mdns")]
        "mdns",
    ];

    if features.is_empty() {
        warn!("no optional features compiled — only management server is available");
    } else {
        info!(features = ?features, "compiled features");
    }
}

/// Print the startup banner to stdout.
fn print_banner(cfg: &ServerConfig) {
    println!("╔══════════════════════════════════════════════════╗");
    println!("║           ok-FPGA-Server v{}                 ║", VERSION);
    println!("║  Opal Kelly FrontPanel 6.1.0 Device Server      ║");
    println!("╠══════════════════════════════════════════════════╣");

    #[cfg(feature = "rest")]
    if cfg.rest.enabled {
        println!(
            "║  REST API:  http://{}:{}/api/v1{} ║",
            cfg.rest.bind,
            cfg.rest.port,
            " ".repeat(
                49_usize.saturating_sub(
                    14 + cfg.rest.bind.len() + format!("{}", cfg.rest.port).len() + 7
                )
            )
        );
    }

    #[cfg(feature = "grpc")]
    if cfg.grpc.enabled {
        println!(
            "║  gRPC:      http://{}:{}{}║",
            cfg.grpc.bind,
            cfg.grpc.port,
            " ".repeat(
                49_usize
                    .saturating_sub(14 + cfg.grpc.bind.len() + format!("{}", cfg.grpc.port).len())
            )
        );
    }

    println!(
        "║  Mgmt:      http://{}:{}{}║",
        cfg.server.management_bind,
        cfg.server.management_port,
        " ".repeat(49_usize.saturating_sub(
            14 + cfg.server.management_bind.len() + format!("{}", cfg.server.management_port).len()
        ))
    );

    println!("╚══════════════════════════════════════════════════╝");
    println!();

    // Suppress unused warning when features are disabled.
    let _ = cfg;
}

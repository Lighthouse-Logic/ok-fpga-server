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

/// REST API server module — only compiled when `rest` feature is enabled.
#[cfg(feature = "rest")]
mod rest;

/// gRPC API server module — only compiled when `grpc` feature is enabled.
#[cfg(feature = "grpc")]
mod grpc;

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
    let cfg = config::load_config();

    // Initialize tracing (structured logging) with the configured log level.
    init_tracing(&cfg);

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

    // Create shared application state.
    let app_state = Arc::new(AppState::new(
        device_manager.clone(),
        resource_registry.clone(),
        service_manager.clone(),
    ));

    // Register available services based on compiled features and config.
    register_services(&service_manager, &app_state, &cfg);

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
    // Create the event broadcast channel for WebSocket.
    let (event_tx, _) = tokio::sync::broadcast::channel::<String>(256);

    // Start the management server (always-on, serves dashboard + mgmt API).
    let cfg_arc = Arc::new(cfg);
    start_management_server(&app_state, &cfg_arc, event_tx.clone()).await;

    info!("ok-FPGA-Server is ready. Press Ctrl+C to shut down.");

    // Wait for shutdown signal.
    tokio::signal::ctrl_c().await?;
    info!("shutdown signal received");

    // Gracefully stop all services.
    service_manager.stop_all().await;

    info!("ok-FPGA-Server shut down cleanly.");
    Ok(())
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
fn init_tracing(cfg: &ServerConfig) {
    // If RUST_LOG is set, use it directly (standard tracing convention).
    // Otherwise, use the config's log_level.
    let filter = tracing_subscriber::EnvFilter::try_from_default_env()
        .unwrap_or_else(|_| {
            tracing_subscriber::EnvFilter::new(&cfg.server.log_level)
        });

    tracing_subscriber::fmt()
        .with_env_filter(filter)
        .with_target(true)
        .with_thread_ids(false)
        .with_file(false)
        .with_line_number(false)
        .init();
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
    cfg: &ServerConfig,
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

    // --- MQTT (Phase 10, opt-in feature) ---
    // #[cfg(feature = "mqtt")]
    // { ... }

    // --- mDNS (Phase 11, opt-in feature) ---
    // #[cfg(feature = "mdns")]
    // { ... }
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
    println!(
        "║           ok-FPGA-Server v{}                 ║",
        VERSION
    );
    println!("║  Opal Kelly FrontPanel 6.1.0 Device Server      ║");
    println!("╠══════════════════════════════════════════════════╣");

    #[cfg(feature = "rest")]
    if cfg.rest.enabled {
        println!(
            "║  REST API:  http://{}:{}/api/v1{} ║",
            cfg.rest.bind,
            cfg.rest.port,
            " ".repeat(49_usize.saturating_sub(
                14 + cfg.rest.bind.len() + format!("{}", cfg.rest.port).len() + 7
            ))
        );
    }

    #[cfg(feature = "grpc")]
    if cfg.grpc.enabled {
        println!(
            "║  gRPC:      http://{}:{}{}║",
            cfg.grpc.bind,
            cfg.grpc.port,
            " ".repeat(49_usize.saturating_sub(
                14 + cfg.grpc.bind.len() + format!("{}", cfg.grpc.port).len()
            ))
        );
    }

    println!(
        "║  Mgmt:      http://{}:{}{}║",
        cfg.server.management_bind,
        cfg.server.management_port,
        " ".repeat(49_usize.saturating_sub(
            14 + cfg.server.management_bind.len()
                + format!("{}", cfg.server.management_port).len()
        ))
    );

    println!("╚══════════════════════════════════════════════════╝");
    println!();

    // Suppress unused warning when features are disabled.
    let _ = cfg;
}

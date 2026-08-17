//! # ok-fpga-server — REST + gRPC Server and Web Dashboard
//!
//! This is the main binary crate for the ok-FPGA-Server. When you run it,
//! it starts a server that:
//!
//! 1. **Loads the FrontPanel DLL** (`okFrontPanel.dll` on Windows,
//!    `libokFrontPanel.so` on Linux) at runtime.
//! 2. **Detects connected Opal Kelly FPGAs** via the FrontPanelManager
//!    hot-plug callback system.
//! 3. **Exposes FPGAs over REST** (Axum, OpenAPI 3.0) and **gRPC** (Tonic, proto3)
//!    depending on which Cargo features are enabled.
//! 4. **Serves a web dashboard** showing device status, resources, logs, and
//!    API traffic in real time.
//!
//! ## Feature Flags (ADR-010)
//!
//! ```text
//! default = ["rest", "dashboard"]
//!
//! rest      — REST API on port 8080 (Axum)
//! dashboard — Web dashboard on management port 9090
//! grpc      — gRPC API on port 50051 (Tonic)
//! mqtt      — MQTT status publisher (Phase 10)
//! mdns      — mDNS service discovery (Phase 11)
//! full      — All features enabled
//! ```
//!
//! ## Configuration
//!
//! The server reads its configuration from a TOML file (default: `config.toml`
//! in the working directory). See Phase 7 of the development roadmap.
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

/// Shared application state (device manager, service manager, etc.)
mod state;

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

use state::AppState;

/// The server version, derived from the workspace Cargo.toml.
const VERSION: &str = env!("CARGO_PKG_VERSION");

/// Entry point for the ok-FPGA-Server.
///
/// Initializes the core domain components, registers services with the
/// ServiceManager based on compiled features and configuration, then
/// starts the management server and enabled services.
#[tokio::main]
async fn main() -> anyhow::Result<()> {
    // Initialize tracing (structured logging).
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("info")),
        )
        .init();

    // Print startup banner.
    print_banner();

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

    // Register available services based on compiled features.
    register_services(&service_manager, &app_state);

    // Log the feature flag status.
    log_feature_status();

    // Start all enabled services.
    service_manager.start_enabled().await;

    // Log the active services.
    for info in service_manager.list() {
        info!(
            service = %info.name,
            status = %info.status,
            compiled = info.compiled,
            enabled = info.enabled,
            "service state"
        );
    }

    info!("ok-FPGA-Server is ready. Press Ctrl+C to shut down.");

    // Wait for shutdown signal.
    tokio::signal::ctrl_c().await?;
    info!("shutdown signal received");

    // Gracefully stop all services.
    service_manager.stop_all().await;

    info!("ok-FPGA-Server shut down cleanly.");
    Ok(())
}

/// Register compiled services with the ServiceManager.
///
/// Services whose Cargo features are enabled are registered. Services whose
/// features are NOT compiled are NOT registered (they can't run in this binary).
/// The `enabled` flag comes from TOML config (Phase 7); for now, all compiled
/// services default to enabled.
fn register_services(service_manager: &Arc<ServiceManager>, app_state: &Arc<AppState>) {
    // --- REST (default feature) ---
    #[cfg(feature = "rest")]
    {
        let rest_addr: std::net::SocketAddr = "0.0.0.0:8080"
            .parse()
            .expect("valid REST bind address");
        let rest_service = Arc::new(rest::managed::RestManagedService::new(
            rest_addr,
            app_state.clone(),
        ));
        service_manager.register(rest_service, true); // enabled by default
    }

    // --- gRPC (opt-in feature) ---
    #[cfg(feature = "grpc")]
    {
        let grpc_addr: std::net::SocketAddr = "0.0.0.0:50051"
            .parse()
            .expect("valid gRPC bind address");
        let grpc_service = Arc::new(grpc::managed::GrpcManagedService::new(
            grpc_addr,
            app_state.clone(),
        ));
        service_manager.register(grpc_service, true); // enabled when compiled
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
fn print_banner() {
    println!("╔══════════════════════════════════════════════════╗");
    println!(
        "║           ok-FPGA-Server v{}                 ║",
        VERSION
    );
    println!("║  Opal Kelly FrontPanel 6.1.0 Device Server      ║");
    println!("╠══════════════════════════════════════════════════╣");

    #[cfg(feature = "rest")]
    println!("║  REST API:  http://localhost:8080/api/v1         ║");

    #[cfg(feature = "grpc")]
    println!("║  gRPC:      http://localhost:50051               ║");

    #[cfg(feature = "dashboard")]
    println!("║  Dashboard: http://localhost:9090                ║");

    println!("╚══════════════════════════════════════════════════╝");
    println!();
}

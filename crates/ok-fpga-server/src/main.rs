//! # ok-fpga-server — REST + gRPC Server and Web Dashboard
//!
//! This is the main binary crate for the ok-FPGA-Server. When you run it,
//! it starts a server that:
//!
//! 1. **Loads the FrontPanel DLL** (`okFrontPanel.dll` on Windows,
//!    `libokFrontPanel.so` on Linux) at runtime.
//! 2. **Detects connected Opal Kelly FPGAs** via the FrontPanelManager
//!    hot-plug callback system.
//! 3. **Exposes FPGAs over REST** (Axum, OpenAPI 3.0) and **gRPC** (Tonic, proto3).
//! 4. **Serves a web dashboard** showing device status, resources, logs, and
//!    API traffic in real time.
//!
//! ## Configuration
//!
//! The server reads its configuration from a TOML file (default: `config.toml`
//! in the working directory). See Phase 7 of the development roadmap.
//!
//! ## Architecture
//!
//! ```text
//! ┌──────────┐  ┌──────────┐  ┌──────────┐
//! │ REST API │  │ gRPC API │  │ Web UI   │
//! │ (Axum)   │  │ (Tonic)  │  │ (Axum)   │
//! └────┬─────┘  └────┬─────┘  └────┬─────┘
//!      └──────────────┴─────────────┘
//!                     │
//!           ┌─────────┴──────────┐
//!           │   ok-fpga-core     │
//!           │ (domain logic)     │
//!           └─────────┬──────────┘
//!                     │
//!           ┌─────────┴──────────┐
//!           │   ok-fpga-ffi      │
//!           │ (C API bindings)   │
//!           └─────────┬──────────┘
//!                     │
//!           ┌─────────┴──────────┐
//!           │  okFrontPanel DLL  │
//!           └────────────────────┘
//! ```

/// The server version, derived from the workspace Cargo.toml.
const VERSION: &str = env!("CARGO_PKG_VERSION");

/// Entry point for the ok-FPGA-Server.
///
/// Currently prints a startup banner and exits. The full server initialization
/// (config loading, DLL loading, device monitoring, REST/gRPC binding) will
/// be implemented in Phases 5-7 of the development roadmap.
fn main() {
    println!("╔══════════════════════════════════════════════════╗");
    println!("║           ok-FPGA-Server v{}                 ║", VERSION);
    println!("║  Opal Kelly FrontPanel 6.1.0 Device Server      ║");
    println!("╠══════════════════════════════════════════════════╣");
    println!("║  REST API:  http://localhost:8080/api/v1         ║");
    println!("║  gRPC:      http://localhost:50051               ║");
    println!("║  Dashboard: http://localhost:8080                ║");
    println!("╚══════════════════════════════════════════════════╝");
    println!();
    println!("  FFI crate:  {}", ok_fpga_core::crate_name());
    println!("  Core crate: {}", ok_fpga_core::crate_name());
    println!();
    println!("  Server is not yet implemented. See docs/development-roadmap.md");
    println!("  for the implementation plan (Phases 5-8).");
}

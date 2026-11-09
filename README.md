# ok-FPGA-Server

A Rust-based server that wraps the [Opal Kelly FrontPanel 6.1.0](https://opalkelly.com/products/frontpanel/) C/C++ API via FFI, exposing connected FPGA devices over REST and gRPC interfaces with a real-time web dashboard.

## Features

- **Device discovery & hot-plug** — automatic detection of connected Opal Kelly FPGAs via `FrontPanelManager`
- **Full FrontPanel 6.1.0 API** — wires, triggers, pipes, registers, AXI, device settings, sensors, PLL, flash, and SYZYGY peripheral discovery
- **Dual API exposure** — OpenAPI 3.0 REST (Axum) and proto3 gRPC (Tonic)
- **Multi-user resource management** — shared/exclusive access control for FPGA resources
- **Web dashboard** — real-time device status, service management, and log streaming
- **MQTT publishing** — publish-only status events to an external MQTT broker
- **mDNS/DNS-SD** — zero-config network discovery via `_okfpga._tcp`
- **Cross-platform** — Windows 11 and Ubuntu 22.04/24.04 LTS

## Supported Hardware

| Board | Interface | SYZYGY |
|-------|-----------|--------|
| XEM7320 (A75T, A200T) | USB3 | ✓ |
| XEM8305 | USB3 / Gen3 | ✓ |
| XEM8310 | USB3 / Gen3 | ✓ |
| XEM8320 | USB3 / Gen3 | ✓ |
| XEM8350 (KU060, KU085, KU115) | USB3 / PCIe | ✓ |
| XEM8370 | USB3 / Gen3 | ✓ |
| HUB1450 (AU10P, AU25P) | USB3 / Gen3 | ✓ |

> USB 2.0 devices are silently skipped and logged.

## Quick Start

### Prerequisites

- **Rust** 1.70+ (2021 edition)
- **libclang** — required by `bindgen` to parse the FrontPanel C header
  - Windows: Install [LLVM](https://releases.llvm.org/) and set `LIBCLANG_PATH=C:\Program Files\LLVM\bin`
  - Ubuntu: `apt install libclang-dev`
- **protoc** — required for gRPC proto compilation (only when building with `--features grpc`)
  - Windows: `winget install Google.Protobuf`
  - Ubuntu: `apt install protobuf-compiler`
- **FrontPanel 6.1.0 SDK** — proprietary Opal Kelly SDK files (`okFrontPanel.h` at build time and `okFrontPanel.dll` / `libokFrontPanel.so` at runtime) are gitignored; copy them into [`FrontPanel-6.1.0/`](FrontPanel-6.1.0/README.md) or set `OK_FRONTPANEL_SDK_DIR`

### Build & Run

```bash
# Build with default features (REST + dashboard):
cargo build

# Build with all features:
cargo build --features full

# Run the server:
cargo run

# Run tests:
cargo test

# Run hardware tests (requires connected FPGA board):
cargo test -p ok-fpga-ffi --test hardware -- --ignored --test-threads=1 --nocapture
```

### Environment Variables

| Variable | Description | Default |
|----------|-------------|---------|
| `LIBCLANG_PATH` | Path to LLVM/libclang bin directory | System PATH |
| `PROTOC` | Path to `protoc` binary | System PATH |
| `OK_FRONTPANEL_SDK_DIR` | Path to Opal Kelly FrontPanel 6.1.0 SDK directory (`okFrontPanel.h` + shared library) | `FrontPanel-6.1.0/` |
| `OKFPGA_CONFIG` | Path to TOML config file | `config.toml` |
| `OKFPGA_DLL_PATH` | Explicit path to `okFrontPanel.dll` / `libokFrontPanel.so` | `<OK_FRONTPANEL_SDK_DIR>/<lib>` or `FrontPanel-6.1.0/<lib>` |

## Feature Flags

Features control which optional server components are compiled in:

| Feature | Default | Description |
|---------|---------|-------------|
| `rest` | ✓ | REST API server (Axum, port 8088) |
| `dashboard` | ✓ | Web dashboard SPA (served on management port 9090) |
| `grpc` | | gRPC API server (Tonic, port 50051) |
| `mqtt` | | MQTT status publisher (requires external broker) |
| `mdns` | | mDNS/DNS-SD service discovery |
| `full` | | All of the above |

```bash
# Build examples:
cargo build                           # REST + dashboard (defaults)
cargo build --features grpc           # REST + dashboard + gRPC
cargo build --features full           # everything
cargo build --no-default-features     # management server only
```

## Configuration

The server reads a TOML configuration file (default: `config.toml`):

```toml
[server]
management_port = 9090       # Always-on management server & dashboard
management_bind = "0.0.0.0"
log_level = "info"           # trace, debug, info, warn, error

[rest]
enabled = true
bind = "0.0.0.0"
port = 8088

[grpc]
enabled = true
bind = "0.0.0.0"
port = 50051

[mqtt]
enabled = false
broker_url = "mqtt://localhost:1883"
publish_interval_secs = 5

[mdns]
enabled = false
instance_name = "okfpga-server"

[dashboard]
enabled = true

[auth]
provider = "none"            # "none", "local", or "ldap"
session_timeout_secs = 3600
```

Configuration priority: defaults → TOML file → `OKFPGA_*` environment variables.

## Architecture

```
┌───────────────────────────────────────────────────────┐
│                    ok-FPGA-Server                      │
│                                                        │
│  ┌────────┐  ┌────────┐  ┌────────┐  ┌────────────┐  │
│  │REST API│  │gRPC API│  │Web UI  │  │Device Mgr  │  │
│  │(Axum)  │  │(Tonic) │  │(Axum)  │  │(FP Manager)│  │
│  └───┬────┘  └───┬────┘  └───┬────┘  └─────┬──────┘  │
│      └───────────┴───────────┴──────────────┘         │
│                         │                              │
│              ┌──────────┴──────────┐                   │
│              │  ok-fpga-core       │                   │
│              │  Domain Layer       │                   │
│              └──────────┬──────────┘                   │
│              ┌──────────┴──────────┐                   │
│              │  ok-fpga-ffi        │                   │
│              │  FFI Bindings       │                   │
│              └──────────┬──────────┘                   │
└─────────────────────────┼──────────────────────────────┘
                          │ DLL/SO
               ┌──────────┴──────────┐
               │ okFrontPanel.dll/.so│
               │ FrontPanel 6.1.0   │
               └──────────┬──────────┘
                          │ USB3 / PCIe / Gen3
               ┌──────────┴──────────┐
               │  Opal Kelly FPGAs   │
               └─────────────────────┘
```

### Crate Structure

| Crate | Purpose |
|-------|---------|
| `ok-fpga-ffi` | Low-level FFI bindings (bindgen + libloading). All `unsafe` confined here. |
| `ok-fpga-core` | Safe Rust domain layer: device lifecycle, resources, auth, sensors, SYZYGY. |
| `ok-fpga-server` | Binary crate: REST/gRPC servers, management API, dashboard, MQTT, mDNS. |

## API Documentation

- **REST API**: OpenAPI 3.0 spec at [`api/openapi.yaml`](api/openapi.yaml)
- **gRPC API**: Proto3 definitions at [`proto/okfpga/v1/okfpga.proto`](proto/okfpga/v1/okfpga.proto)
- **Rust docs**: `cargo doc --no-deps --open`

## Architecture Decision Records

All major design decisions are documented as ADRs in [`docs/architecture/adr/`](docs/architecture/adr/):

| ADR | Title |
|-----|-------|
| [ADR-001](docs/architecture/adr/ADR-001-ffi-strategy.md) | FFI Strategy — libloading + rust-bindgen |
| [ADR-002](docs/architecture/adr/ADR-002-device-lifecycle.md) | Device Lifecycle & Hot-Plug |
| [ADR-003](docs/architecture/adr/ADR-003-data-port-abstraction.md) | Data Port Abstraction — Classic vs AXI |
| [ADR-004](docs/architecture/adr/ADR-004-rest-api-design.md) | REST API Design — OpenAPI 3.0 |
| [ADR-005](docs/architecture/adr/ADR-005-grpc-api-design.md) | gRPC API Design — Proto3 |
| [ADR-006](docs/architecture/adr/ADR-006-resource-management.md) | Multi-User Resource Management |
| [ADR-007](docs/architecture/adr/ADR-007-web-dashboard.md) | Web Dashboard Architecture |
| [ADR-008](docs/architecture/adr/ADR-008-authentication.md) | Authentication — Pluggable Provider |
| [ADR-009](docs/architecture/adr/ADR-009-board-syzygy.md) | Supported Boards & SYZYGY Discovery |
| [ADR-010](docs/architecture/adr/ADR-010-service-manager.md) | Service Manager |
| [ADR-011](docs/architecture/adr/ADR-011-mqtt-publishing.md) | MQTT Status Publishing |
| [ADR-012](docs/architecture/adr/ADR-012-mdns-discovery.md) | mDNS/DNS-SD Service Discovery |
| [ADR-013](docs/architecture/adr/ADR-013-device-scanner.md) | Device Scanner — FFI→DeviceManager Bridge |

## License & Commercial Licensing

Copyright (C) 2026 Great Bay Synergy LLC d/b/a Lighthouse Logic.

**`ok-fpga-server`** is **dual-licensed**:

1. **Open-Source (`GPLv3`)**: Available under the [GNU General Public License v3.0](./LICENSE).
2. **Commercial License**: To integrate `ok-fpga-server` into closed-source software, proprietary test systems, or commercial hardware products without GPLv3 copyleft obligations, please see [`COMMERCIAL-LICENSE.md`](./COMMERCIAL-LICENSE.md) or contact **Lighthouse Logic** at [admin@lighthouselogic.dev](mailto:admin@lighthouselogic.dev) ([https://lighthouselogic.dev](https://lighthouselogic.dev)).

### Contributing
Contributions are welcome! All contributors must agree to our [Contributor License Agreement (`CLA.md`)](./CLA.md) via a one-line Pull Request comment. See [`CONTRIBUTING.md`](./CONTRIBUTING.md) for details.

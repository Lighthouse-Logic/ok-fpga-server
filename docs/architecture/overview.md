# ok-FPGA-Server — Architecture Overview

**Version**: 0.1.0  
**Date**: 2026-08-16  
**Author**: Architecture Team  

---

## 1. Purpose

The **ok-FPGA-Server** is a Rust-based service that wraps the Opal Kelly FrontPanel 6.1.0 C/C++ DLL via FFI and exposes connected FPGA devices over REST and gRPC interfaces. It provides:

- **Device discovery & hot-plug management** — automatic detection of connected/disconnected Opal Kelly FPGAs via `FrontPanelManager`
- **Full FrontPanel 6.1.0 API access** — wire, trigger, pipe, register, AXI, device settings, sensors, PLL, flash, and SYZYGY peripheral discovery
- **Multi-user resource management** — multiple users accessing shared or locked FPGA resources with extensible access control
- **Web dashboard** — real-time status, device info, resource state, traffic monitoring, and log viewing
- **Dual API exposure** — OpenAPI 3.0 REST and proto3 gRPC
- **Cross-platform deployment** — Ubuntu 22.04 LTS, Ubuntu 24.04 LTS, Windows 11

## 2. System Context

```
┌──────────────────────────────────────────────────────────────────┐
│                        ok-FPGA-Server                           │
│                                                                  │
│  ┌──────────┐  ┌──────────┐  ┌──────────┐  ┌───────────────┐   │
│  │ REST API │  │ gRPC API │  │ Web UI   │  │ Device Manager│   │
│  │ (Axum)   │  │ (Tonic)  │  │ (Axum)   │  │ (FP Manager) │   │
│  └────┬─────┘  └────┬─────┘  └────┬─────┘  └───────┬───────┘   │
│       │              │             │                │            │
│       └──────────────┴─────────────┴────────────────┘            │
│                              │                                   │
│                    ┌─────────┴──────────┐                        │
│                    │  Core Domain Layer  │                        │
│                    │  (Resource Mgmt,    │                        │
│                    │   Device State,     │                        │
│                    │   Bitfile Tracking) │                        │
│                    └─────────┬──────────┘                        │
│                              │                                   │
│                    ┌─────────┴──────────┐                        │
│                    │   FFI Layer         │                        │
│                    │   (libloading +     │                        │
│                    │    rust-bindgen)    │                        │
│                    └─────────┬──────────┘                        │
│                              │                                   │
└──────────────────────────────┼───────────────────────────────────┘
                               │ DLL/SO calls
                    ┌──────────┴──────────┐
                    │  okFrontPanel.dll    │
                    │  (or .so on Linux)   │
                    │  FrontPanel 6.1.0    │
                    └──────────┬──────────┘
                               │ USB 3.0 / PCIe / Gen3
                    ┌──────────┴──────────┐
                    │  Opal Kelly FPGAs   │
                    │  (XEM8305, XEM8310, │
                    │   XEM7320, XEM8320, │
                    │   XEM8350, XEM8370, │
                    │   HUB1450)          │
                    └─────────────────────┘
```

## 3. Layer Architecture

### Layer 1: FFI Layer (`ok-fpga-ffi`)

- **Responsibility**: Low-level bindings to the FrontPanel 6.1.0 C API
- **Technology**: `rust-bindgen` for type generation from `okFrontPanel.h`, `libloading` for runtime DLL/SO loading
- **Key design**: All C handles wrapped in Rust RAII types; `unsafe` confined to this layer
- **Platform**: Loads `okFrontPanel.dll` on Windows, `libokFrontPanel.so` on Linux
- **Exclusions**: `WriteI2C`, `ReadI2C`, and any other USB 2.0-only functions are omitted

### Layer 2: Core Domain Layer (`ok-fpga-core`)

- **Responsibility**: Safe Rust abstractions over the FFI layer; device lifecycle; resource management; bitfile tracking
- **Key types**:
  - `DeviceManager` — wraps `FrontPanelManager` callbacks for hot-plug
  - `FpgaDevice` — represents a connected FPGA with its `okTDeviceInfo`, settings, sensors
  - `DataPort` — trait abstraction over `FPGADataPortClassic` and `FPGADataPortAXI`
  - `Resource` — a named, user-defined capability from a programmed FPGA
  - `ResourceState` — `Unavailable | Initializing | Available | InUseShared | InUseLocked`
  - `BitfileRecord` — tracks filename, description, programming timestamp, user

### Layer 3: API Layer

- **REST** (`ok-fpga-rest`): Axum-based, OpenAPI 3.0 spec-first, JSON responses, SSE for events
- **gRPC** (`ok-fpga-grpc`): Tonic-based, proto3 spec-first, server streaming for events
- **Web Dashboard** (`ok-fpga-web`): Axum static file serving + WebSocket/SSE for real-time updates

### Layer 4: Configuration & Infrastructure

- **Config**: TOML file with environment variable overrides
- **Logging**: `tracing` crate with log streaming to web dashboard
- **Auth stubs**: Pluggable trait — no-auth default, local user store stub, LDAP/SSL stub
- **MQTT publishing**: Publish-only status events to external MQTT v5 broker (feature-gated)
- **mDNS discovery**: Zero-config `_okfpga._tcp` service advertising (feature-gated)
- **Service manager**: Dynamic start/stop/restart of optional services via management API

## 4. Supported Hardware

| Board | Product Constant | Interface | SYZYGY Ports |
|-------|-----------------|-----------|-------------|
| XEM8305 | `OK_PRODUCT_XEM8305AU15P` (60) | USB3 / Gen3 | Yes |
| XEM8310 | `OK_PRODUCT_XEM8310AU25P` (57) | USB3 / Gen3 | Yes |
| XEM7320 | `OK_PRODUCT_XEM7320A75T` (45), `OK_PRODUCT_XEM7320A200T` (46) | USB3 | Yes |
| XEM8320 | `OK_PRODUCT_XEM8320AU25P` (56) | USB3 / Gen3 | Yes |
| XEM8350 | `OK_PRODUCT_XEM8350KU060` (49), `KU085` (50), `KU115` (51) | USB3 / PCIe | Yes |
| XEM8370 | `OK_PRODUCT_XEM8370KU11P` (59) | USB3 / Gen3 | Yes |
| HUB1450 | `OK_PRODUCT_SZGHUB1450AU10P` (61), `AU25P` (62) | USB3 / Gen3 | Yes |

Devices with `deviceInterface == OK_INTERFACE_USB2` (1) are silently skipped and logged.

## 5. Key Architectural Decisions

All decisions are recorded as ADRs in `docs/architecture/adr/`:

| ADR | Title | Status |
|-----|-------|--------|
| [ADR-001](adr/ADR-001-ffi-strategy.md) | FFI Strategy — libloading + rust-bindgen | Accepted |
| [ADR-002](adr/ADR-002-device-lifecycle.md) | Device Lifecycle & Hot-Plug | Accepted |
| [ADR-003](adr/ADR-003-data-port-abstraction.md) | Data Port Abstraction — Classic vs AXI | Accepted |
| [ADR-004](adr/ADR-004-rest-api-design.md) | REST API Design — OpenAPI 3.0 | Accepted |
| [ADR-005](adr/ADR-005-grpc-api-design.md) | gRPC API Design — Proto3 | Accepted |
| [ADR-006](adr/ADR-006-resource-management.md) | Multi-User Resource Management | Accepted |
| [ADR-007](adr/ADR-007-web-dashboard.md) | Web Dashboard Architecture | Accepted |
| [ADR-008](adr/ADR-008-authentication.md) | Authentication Stubs — Local & LDAP | Accepted |
| [ADR-009](adr/ADR-009-board-syzygy.md) | Supported Boards & SYZYGY Discovery | Accepted |
| [ADR-010](adr/ADR-010-service-manager.md) | Service Manager — Dynamic Lifecycle Control | Accepted |
| [ADR-011](adr/ADR-011-mqtt-publishing.md) | MQTT Status Publishing | Accepted |
| [ADR-012](adr/ADR-012-mdns-discovery.md) | mDNS/DNS-SD Service Discovery | Accepted |
| [ADR-013](adr/ADR-013-device-scanner.md) | Device Scanner — FFI→DeviceManager Bridge | Accepted |
| [ADR-014](adr/ADR-014-example-clients.md) | Example Client Architecture & E2E Testing | Accepted |

## 6. Technology Stack

| Concern | Technology | Crate |
|---------|-----------|-------|
| Language | Rust (2021 edition) | — |
| Async runtime | Tokio | `tokio` |
| FFI type gen | rust-bindgen | `bindgen` (build dep) |
| DLL loading | Runtime via libloading | `libloading` |
| REST framework | Axum | `axum` |
| gRPC framework | Tonic | `tonic`, `prost` |
| Serialization | serde + JSON | `serde`, `serde_json` |
| Config | TOML | `toml`, `config` |
| Logging | Tracing | `tracing`, `tracing-subscriber` |
| OpenAPI codegen | — | `utoipa` (optional) |
| Proto codegen | protoc + tonic-build | `tonic-build` (build dep) |
| MQTT client | rumqttc | `rumqttc` (optional) |
| mDNS/DNS-SD | mdns-sd | `mdns-sd` (optional) |

## 7. Deployment Model

The server is a single binary that:
1. Loads `okFrontPanel.dll` / `.so` from a configurable path (or system library path)
2. Starts the FrontPanelManager for hot-plug monitoring
3. Binds REST, gRPC, and web dashboard on configurable ports
4. Runs until terminated or restarted via the REST API

Cross-platform packaging:
- **Windows 11**: `.exe` + `okFrontPanel.dll` side-by-side
- **Ubuntu 22.04/24.04**: Binary + `libokFrontPanel.so` (via `LD_LIBRARY_PATH` or install to `/usr/local/lib`)

## 8. References

- [FrontPanel 6.1.0 Header](../FrontPanel-6.1.0/okFrontPanel.h)
- [FP5→FP6 Migration Guide](../migrate-cpp-fp5-to-fp6.md)
- [Project Goals](../ok-FPGA-Server_Project-goals.txt)
- [Opal Kelly FrontPanel API Documentation](https://library.opalkelly.com/library/FrontPanelAPI/namespaceOpalKelly.html)

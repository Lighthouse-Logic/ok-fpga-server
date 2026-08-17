# ADR-014: Example Client Architecture & End-to-End Testing

**Status**: Accepted  
**Date**: 2026-08-17  
**Deciders**: Architecture Team  
**Roadmap Phase**: 16

## Context

The ok-FPGA-Server exposes two API surfaces (REST via OpenAPI 3.0 and gRPC via proto3)
and two infrastructure services (MQTT publishing and mDNS discovery). To validate these
interfaces and provide developers with working reference implementations, the project
needs example clients in multiple languages.

### Requirements

1. **Four example clients**: 2 Rust (REST, gRPC), 2 Python (REST, gRPC)
2. **mDNS discovery**: All clients discover the server via `_okfpga._tcp.local.`
3. **MQTT status publishing**: All clients publish their own connection status
4. **Rigorous testing**: Every REST endpoint and gRPC RPC must be exercised
5. **No real FPGA dependency**: Tests use mock programming and mock resources
6. **Well-commented, educational**: Serve as reference implementations for integrators

## Decision

### Client Architecture

Each client is a standalone project in `client-examples/` with its own build system,
dependencies, and README. Clients are NOT part of the Cargo workspace to keep them
independent and easy to copy as starting points.

```
client-examples/
├── README.md
├── rust-rest-client/        Cargo project (reqwest + mdns-sd + rumqttc)
├── rust-grpc-client/        Cargo project (tonic + mdns-sd + rumqttc)
├── python-rest-client/      Python 3.12+ (httpx + pydantic + zeroconf + paho-mqtt)
└── python-grpc-client/      Python 3.12+ (grpcio + pydantic + zeroconf + paho-mqtt)
```

### Technology Stack

| Component | Rust | Python 3.12+ |
|-----------|------|-------------|
| REST HTTP | `reqwest` | `httpx` (async) |
| gRPC | `tonic` + `prost` | `grpcio` + `grpcio-tools` |
| mDNS discovery | `mdns-sd` | `zeroconf` |
| MQTT publishing | `rumqttc` | `paho-mqtt` |
| Data models | `serde` + Rust structs | `pydantic` v2 |
| Test framework | `cargo test` | `pytest` + `pytest-asyncio` |
| CLI | `clap` | `argparse` |

### Code Generation from API Definitions

Both REST and gRPC clients are built from their respective API definition files:

- **REST clients**: Built against `/api/openapi.yaml` — client models mirror the
  OpenAPI schema definitions. If the OpenAPI spec changes, the client models must
  be updated to match.
- **gRPC clients**: Built against `/proto/okfpga/v1/okfpga.proto` — Rust clients
  use `tonic-build` at compile time; Python clients include a `generate_protos.py`
  script that runs `grpcio-tools` to produce generated stubs in
  `okfpga_grpc_client/generated/`.

### mDNS Discovery (Standalone per Client)

Each client contains its own mDNS discovery module. The logic is identical across
all four clients:

1. Browse for `_okfpga._tcp.local.` services
2. Extract TXT records: `rest`, `grpc`, `mqtt`, `dashboard`, `version`, `devices`
3. Build connection URL from host + port

The discovery code is duplicated (not shared) so each client is a standalone example.
Both Rust files and both Python files include a header comment:

```
// NOTE: This discovery module is identical in rust-rest-client and rust-grpc-client.
// If you modify this file, apply the same changes to the other client.
```

### MQTT Client Topic Structure

Clients publish their status under a separate namespace from the server to avoid
collision. The server publishes to `okfpga/{server_id}/...`; clients publish to
`okfpga-client/{client_id}/...`.

```
okfpga-client/{client_id}/
├── status                              # Heartbeat (periodic, QoS 1)
│                                       #   { "online": true, "uptime_secs": 120,
│                                       #     "client_type": "rust-rest",
│                                       #     "version": "0.1.0" }
├── connections/
│   └── {server_instance}/              # Per-server connection status
│       └── status                      #   { "connected": true,
│                                       #     "server_host": "lab-bench-1.local",
│                                       #     "rest_port": 8088,
│                                       #     "latency_ms": 12 }
└── resources/
    ├── inventory                       # Acquired resource snapshot (on change)
    │                                   #   { "resources": [
    │                                   #     { "id": "...", "state": "InUseLocked" }
    │                                   #   ] }
    └── {resource_id}/
        └── state                       # Per-resource state (on change, QoS 1)
                                        #   { "state": "InUseLocked",
                                        #     "acquired_at": "2026-08-17T..." }
```

### MQTT Broker Handling

MQTT publishing is **optional** in all clients. The MQTT broker address and port
are configured in the client's configuration file. If the broker is unavailable,
the client logs a warning and skips MQTT functionality. Tests that exercise MQTT
are also skipped when the broker is unreachable.

### Testing Strategy

#### Protocol-Layer Testing (No FFI)

The current server routes return stub/mock responses for most FPGA operations
(wire, pipe, trigger, AXI, etc.) because the FFI calls aren't wired through to
real hardware. The example client tests verify the **protocol layer**:

- Correct request/response shapes
- HTTP status codes and gRPC error codes
- Authentication header handling
- Resource state transitions (acquire shared → acquire locked → release)
- WebSocket/SSE event delivery

#### Mock Programming Flow

1. Client discovers server via mDNS (or falls back to `localhost`)
2. Client calls `GET /devices` to find connected (mock) device
3. Client calls `POST /devices/{serial}/configure` → server returns success (mock)
4. Client calls `POST /resources/{id}/acquire` → server returns locked resource
5. Client verifies resource state, then releases

#### Test Runner

- **Rust clients**: Runnable as both `cargo run` (interactive demo) and
  `cargo test` (automated verification)
- **Python clients**: Runnable as both `python -m okfpga_rest_client.examples.demo`
  and `pytest -v` (automated verification)
- **Server requirement**: `cargo run --features full` must be running for all
  client tests. The test documentation makes this explicit.
- **Automated integration**: Tests should start the server process, run the demo,
  verify console output, and tear down.

#### Coverage Matrix

| Category | REST Endpoints | gRPC RPCs |
|----------|---------------|-----------|
| Server | 7 endpoints | 6 RPCs (ServerService) |
| Devices | 10 endpoints | 20 RPCs (DeviceService) |
| Classic data port | 14 endpoints | 15 RPCs (ClassicDataPortService) |
| AXI data port | 10 endpoints | 12 RPCs (AXIDataPortService) |
| Resources | 4 endpoints | (included in DeviceService) |
| Users | 2 endpoints | (included in DeviceService) |
| Events | 1 SSE endpoint | 1 streaming RPC |
| **Total** | **48 endpoints** | **53 RPCs** |

### Python Version

All Python clients target Python `>=3.12`. This version provides:
- Massive interpreter speedups for gRPC stream handling
- Fully stable `grpcio` with pre-compiled binary wheels
- Cleaner generics syntax (PEP 695) for type-annotated data models

## Consequences

### Positive

- **Complete API coverage** — every endpoint and RPC has a working client call + test
- **Developer onboarding** — new integrators can copy a client project and modify
- **Regression safety** — protocol-level changes are caught by client test failures
- **Fleet observability** — MQTT topics enable monitoring of client health and resource usage
- **Spec-driven** — clients are built from the OpenAPI/proto definitions, not ad-hoc

### Negative

- **Maintenance burden** — 4 clients × 2 discovery modules = code to keep in sync
- **Server dependency** — tests require a running server (not pure unit tests)
- **Mock limitations** — cannot verify actual FPGA I/O without real hardware

### Risks

- **OpenAPI/proto drift** — if specs change, clients must be updated. Mitigation:
  client tests will fail immediately, signaling the drift.
- **MQTT broker assumption** — tests skip MQTT if broker unavailable, which could
  mask MQTT bugs. Mitigation: CI should run Mosquitto.

## References

- [OpenAPI 3.0 Spec](../../api/openapi.yaml)
- [Proto3 gRPC Spec](../../proto/okfpga/v1/okfpga.proto)
- [ADR-004 — REST API Design](ADR-004-rest-api-design.md)
- [ADR-005 — gRPC API Design](ADR-005-grpc-api-design.md)
- [ADR-006 — Resource Management](ADR-006-resource-management.md)
- [ADR-011 — MQTT Publishing](ADR-011-mqtt-publishing.md)
- [ADR-012 — mDNS Discovery](ADR-012-mdns-discovery.md)

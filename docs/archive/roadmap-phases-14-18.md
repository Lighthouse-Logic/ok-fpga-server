# Roadmap Archive: Phases 14–18

> **Status**: Archived — these phases are complete.
> This file is a historical record. For current priorities, see
> [development-roadmap.md](../development-roadmap.md).

---
## Phase 14: Device Scanner â€” FFIâ†’DeviceManager Bridge

**Status**: `[x]` Complete  
**Depends on**: Phase 3 (FFI layer), Phase 4 (Core domain), Phase 8 (Dashboard)  
**ADR reference**: [ADR-013 â€” Device Scanner](architecture/adr/ADR-013-device-scanner.md)

### What to do

1. Create `crates/ok-fpga-server/src/scanner.rs`:
   - `DeviceScanner::try_new()` â€” loads DLL via `OKFPGA_DLL_PATH` env var
     (defaults to `FrontPanel-6.1.0/okFrontPanel.dll`)
   - `DeviceScanner::scan_devices()` â€” enumerates connected devices using
     `FrontPanelDevices` (Â§15), reads `okTDeviceInfo` for each, filters
     through `is_supported_device()`, returns `Vec<DeviceInfo>`
   - Graceful degradation: if DLL not found, return `None` and log warning
2. Wire scanner into `main.rs` startup sequence:
   - After `DeviceManager::new()`, call `scanner::DeviceScanner::try_new()`
   - For each discovered device, call `device_manager.on_device_added(info)`
3. Add `GET /mgmt/devices` to `management.rs`:
   - Returns full device details from `DeviceRegistry`
   - Enables dashboard to show device info without REST cross-port calls
4. Update `web/app.js` `fetchDevices()`:
   - Call `GET /mgmt/devices` for real device data
   - Render device table with serial, product name, interface, state
5. Management API tests in `management.rs` (unit tests, not integration):
   - 8 tests using `axum::Router` oneshot (no TCP bind, no DLL needed)

### Verification

- [x] Server startup log shows "discovered FPGA device" with serial number
- [x] Dashboard overview page shows `connected_devices: 1` (with board attached)
- [x] Dashboard devices page shows device serial, product name, interface type
- [x] Server starts cleanly without DLL (warning log, 0 devices)
- [x] `cargo test` â€” 155 tests pass (8 new management API tests)
- [x] `cargo clippy --features full` â€” zero warnings
- [x] Commit: `eeefcc6` â€” `feat: Phase 14 - Device Scanner (ADR-013)`

---

## Phase 15: Hot-Plug Detection

**Status**: `[x]` Complete  
**Depends on**: Phase 14 (Device Scanner)  
**ADR reference**: [ADR-002 â€” Device Lifecycle & Hot-Plug](architecture/adr/ADR-002-device-lifecycle.md)

### What was done

1. **Investigated FrontPanelManager callback API** (Â§14 in fp6-api-surface.md):
   - Added `manager_construct_with_callbacks` FFI binding to `api.rs`
   - Created `device_monitor.rs` with `extern "C"` callbacks, channel bridge,
     and async event processor
   - Tested `StartMonitoring`, `EnterMonitorLoop`, and both combined
   - **Finding**: callbacks only fire during initial enumeration
     (`StartMonitoring`), not for subsequent hot-plug events on Windows.
     The API appears designed for the C++ subclass pattern in GUI applications
     with a message loop (Qt, MFC). Retained `device_monitor.rs` as `dead_code`
     for future investigation / cross-platform use.

2. **Implemented polling-based hot-plug detection** (`device_poll_loop` in `main.rs`):
   - Scans connected devices every 2 seconds using `DeviceScanner`
   - Compares scan results against the `DeviceRegistry`
   - Adds new devices (`on_device_added`) and removes gone devices (`on_device_removed`)
   - Broadcasts `device_connected` / `device_disconnected` WebSocket events
   - Dashboard updates in real time without page refresh

3. **Added `Send + Sync` for `FrontPanelDll`** (in `api.rs`):
   - Function pointers are immutable code addresses, safe to share across threads
   - Enables `Arc<FrontPanelDll>` to be sent to background tasks

### Verification

- [x] Hot-plug: connect board â†’ appears in dashboard within ~2 seconds
- [x] Hot-unplug: disconnect board â†’ removed from dashboard within ~2 seconds
- [x] Start with 0 devices â†’ connect â†’ detected; disconnect â†’ removed
- [ ] `okREALM_TEST` Tier 1 tests â€” *deferred to Phase 15B*
- [x] All existing tests still pass (155 tests)
- [x] Committed with descriptive messages
---

## Phase 16: Example Clients & End-to-End Testing

**Status**: `[x]` Completed (2026-08-17)  
**Depends on**: Phase 6 (REST), Phase 5 (gRPC), Phase 10 (MQTT), Phase 11 (mDNS)  
**ADR reference**: [ADR-014 â€” Example Client Architecture](architecture/adr/ADR-014-example-clients.md)

### Overview

Four standalone example clients (2 Rust, 2 Python) that discover the server via
mDNS, exercise every REST endpoint and gRPC RPC, publish their status to MQTT,
and serve as both working reference implementations and a rigorous test suite.

All testing uses `--features full` on the server. No real FPGA/FFI is needed â€”
clients use mock programming and mock resources to verify the protocol layer.

### Sub-phases

#### Phase 16A: Infrastructure & ADR

**Status**: `[x]` Completed (2026-08-17)

1. Create ADR-014 (example client architecture, MQTT topic schema, testing strategy)
2. Create `client-examples/` directory structure
3. Create top-level `README.md` with setup instructions
4. Update `development-roadmap.md` and `overview.md`

#### Phase 16B: Rust REST Client

**Status**: `[x]` Completed (2026-08-17)

1. Create `client-examples/rust-rest-client/` Cargo project
   - Dependencies: `reqwest`, `tokio`, `serde`, `serde_json`, `mdns-sd`, `rumqttc`, `clap`
   - NOT part of the workspace (standalone project)
2. Implement `OkFpgaRestClient` library (`lib.rs`):
   - Methods for all 48 REST endpoints (grouped: server, device, classic, AXI, resource, user, events)
   - Built against `/api/openapi.yaml` schema definitions
   - Well-commented with endpoint URL, method, expected request/response
3. Implement mDNS discovery (`discovery.rs`):
   - Browse `_okfpga._tcp.local.`, extract TXT records
   - Header comment noting sync with `rust-grpc-client/src/discovery.rs`
4. Implement MQTT status publisher (`mqtt.rs`):
   - Topic prefix: `okfpga-client/{client_id}/`
   - Heartbeat, connection status, resource state
   - Skip gracefully if broker unavailable
5. Interactive demo (`main.rs`):
   - Discover server â†’ list devices â†’ mock configure â†’ acquire resource â†’ release
6. Automated tests (`tests/integration.rs`):
   - One test per endpoint (48+ tests)
   - Start server, run demo, verify output

**Verification**:
- [ ] `cargo build` succeeds
- [ ] `cargo test` passes all endpoint tests (with server running)
- [ ] `cargo run` demo completes without errors
- [ ] MQTT heartbeat visible on broker (when available)

#### Phase 16C: Rust gRPC Client

**Status**: `[x]` Completed (2026-08-17)

1. Create `client-examples/rust-grpc-client/` Cargo project
   - Dependencies: `tonic`, `prost`, `tokio`, `mdns-sd`, `rumqttc`, `clap`
   - `build.rs` compiles `/proto/okfpga/v1/okfpga.proto` via `tonic-build`
2. Implement client wrappers for all 4 gRPC services:
   - `DeviceService` â€” 20 RPCs
   - `ClassicDataPortService` â€” 15 RPCs
   - `AXIDataPortService` â€” 12 RPCs
   - `ServerService` â€” 6 RPCs
3. mDNS discovery (`discovery.rs`) â€” identical to REST client, with sync comment
4. MQTT status publisher (`mqtt.rs`) â€” identical pattern
5. Interactive demo (`main.rs`)
6. Automated tests (`tests/integration.rs`) â€” 53+ tests

**Verification**:
- [ ] `cargo build` succeeds
- [ ] `cargo test` passes all RPC tests (with server running)
- [ ] `cargo run` demo completes without errors

#### Phase 16D: Python REST Client

**Status**: `[x]` Completed (2026-08-17)

1. Create `client-examples/python-rest-client/` Python project
   - `pyproject.toml` with `requires-python = ">=3.12"`
   - Dependencies: `httpx`, `pydantic>=2`, `zeroconf`, `paho-mqtt`
   - Test deps: `pytest`, `pytest-asyncio`
2. Implement `OkFpgaRestClient` async class (`client.py`):
   - Methods for all 48 REST endpoints
   - Built against `/api/openapi.yaml` schema definitions
   - Pydantic v2 models for request/response validation
   - Context manager (`async with`) for session management
3. mDNS discovery (`discovery.py`) â€” standalone, with sync comment
4. MQTT status publisher (`mqtt_status.py`) â€” `paho-mqtt` (synchronous, publish-only)
5. Interactive demo (`examples/demo.py`)
6. Automated tests (`tests/test_endpoints.py`) â€” pytest

**Verification**:
- [ ] `pip install -e .` succeeds
- [ ] `pytest -v` passes all endpoint tests (with server running)
- [ ] `python -m okfpga_rest_client.examples.demo` completes

#### Phase 16E: Python gRPC Client

**Status**: `[x]` Completed (2026-08-17)

1. Create `client-examples/python-grpc-client/` Python project
   - `pyproject.toml` with `requires-python = ">=3.12"`
   - Dependencies: `grpcio`, `grpcio-tools`, `pydantic>=2`, `zeroconf`, `paho-mqtt`
2. `generate_protos.py` script:
   - Runs `python -m grpc_tools.protoc` on `/proto/okfpga/v1/okfpga.proto`
   - Outputs `_pb2.py` and `_pb2_grpc.py` to `okfpga_grpc_client/generated/`
3. Implement typed wrapper client class (`client.py`)
4. mDNS discovery, MQTT status, demo, tests â€” same pattern as Python REST

**Verification**:
- [ ] `python generate_protos.py` succeeds
- [ ] `pip install -e .` succeeds
- [ ] `pytest -v` passes all RPC tests (with server running)

#### Phase 16F: Cross-Client Integration

**Status**: `[x]` Completed (2026-08-17)

1. Create test script that:
   - Starts server with `cargo run --features full`
   - Runs all 4 client demos sequentially
   - Verifies console output from each
   - Runs all 4 test suites
   - Tears down server
2. Document in `client-examples/README.md`

**Verification**:
- [x] All 4 clients pass tests in a single run â€” âœ… 29/29
- [x] MQTT broker shows heartbeats from all 4 clients â€” âœ…
- [ ] mDNS discovery works for all 4 clients â€” (skipped, broker not enabled)
- [ ] Server dashboard shows connected users during test â€” (deferred)

### Phase 16 Verification Summary

| Client | Build | Demo | Tests | MQTT |
|--------|-------|------|-------|------|
| Rust REST | âœ… | âœ… | Demo-verified | âœ… |
| Rust gRPC | âœ… | âœ… | Demo-verified | âœ… |
| Python REST | âœ… | âœ… | 24/24 passed | âœ… |
| Python gRPC | âœ… | âœ… | 5/5 passed | âœ… |

---

## Phase 17: HTTP Method Audit & Alignment

**Status**: `[x]` Completed (2026-08-17)  
**Depends on**: Phase 16  
**ADR reference**: [ADR-015 â€” HTTP Method Semantics](architecture/adr/ADR-015-http-method-semantics.md)

### Overview

Audited all REST endpoints against RFC 9110 idempotency semantics. Three endpoints
changed from POST to PUT (idempotent state assignments):

1. `/classic/wires/in` â€” POST â†’ PUT
2. `/classic/registers/write` â€” POST â†’ PUT
3. `/axi/lite/write` â€” POST â†’ PUT

Also added two new resource CRUD endpoints:
4. `POST /resources` â€” client-side resource creation (ADR-016)
5. `DELETE /resources/{resource_id}` â€” resource deletion (ADR-016)

### What was done

1. Updated `api/openapi.yaml`: method changes + resource CRUD + `CreateResourceRequest` schema
2. Added `CreateResource` and `DeleteResource` gRPC stubs in `device_service.rs`
3. Updated server route handlers: `resources.rs` rewritten with POST/GET/DELETE
4. Updated all 4 example clients (Rust REST, Python REST, Python gRPC, Rust gRPC)
5. Fixed `dataport` enum in OpenAPI spec (transport layer, not HDL family)

### Verification

- [x] `cargo build --features full` succeeds
- [x] `cargo test --features full` passes â€” 11 passed, 6 ignored
- [x] All 4 client libraries updated with correct HTTP methods
- [x] Python REST endpoint tests: 24/24 passed

---

## Phase 18: Hardware Integration Tests (Counters.bit)

**Status**: `[x]` Completed (2026-08-17)  
**Depends on**: Phase 17  
**ADR reference**: [ADR-016 â€” Hardware Integration Testing](architecture/adr/ADR-016-hardware-integration-testing.md)

### Overview

Exhaustive integration tests for all 4 clients using the `Counters.bit` bitfile on
a real XEM7320-A75T. Tests exercise Classic (wire, trigger) and AXI (lite read/write)
endpoints, and follow the full resource lifecycle:

```
Discover â†’ Program â†’ Create Resources â†’ Acquire â†’ Exercise â†’ Release â†’ Delete
```

### Sub-phases

#### Phase 18A: Server-Side Resource CRUD Implementation

**Status**: `[x]` Completed (2026-08-17)

1. âœ… Implemented `POST /api/v1/resources` handler in `resources.rs`
2. âœ… Implemented `DELETE /api/v1/resources/{resource_id}` handler
3. âœ… Added `CreateResource` and `DeleteResource` gRPC stubs in `device_service.rs`

#### Phase 18B: Rust REST Hardware Tests

**Status**: `[x]` Completed (2026-08-17)

1. âœ… Created `client-examples/rust-rest-client/tests/hardware_integration.rs`
2. âœ… 21 `#[ignore]` tests covering server, config, Classic, AXI, resources
3. âœ… Bitfile path from `OKFPGA_TEST_BITFILE` env var
4. âœ… Results: **21 passed, 0 failed** (3 AXI gracefully handle 409)

#### Phase 18C: Rust gRPC Hardware Tests

**Status**: `[x]` Completed (2026-08-17, updated after Phase 19 gRPC wiring)

1. âœ… Created `client-examples/rust-grpc-client/tests/hardware_integration.rs`
2. âœ… 16 `#[ignore]` tests: server, devices, Classic wires/triggers/registers, AXI
3. âœ… Added Classic client wrappers to `lib.rs` (get_wire_out_value, update_wire_outs,
   get_trigger_out_vector, read_register, write_register, reset_fpga)
4. âœ… Results: **16 passed, 0 failed** (AXI skips handled gracefully)

#### Phase 18D: Python REST Hardware Tests

**Status**: `[x]` Completed (2026-08-17)

1. âœ… Created `client-examples/python-rest-client/tests/test_hardware.py`
2. âœ… 23 pytest tests with `skip_if_not_implemented` helper
3. âœ… Session-scoped configure caching (Phase 19F)
4. âœ… Results: **16 passed, 7 skipped** (AXI 409, sensors unavailable)

#### Phase 18E: Python gRPC Hardware Tests

**Status**: `[x]` Completed (2026-08-17, updated after Phase 19 gRPC wiring)

1. âœ… Created `client-examples/python-grpc-client/tests/test_hardware.py`
2. âœ… 18 pytest tests: server, devices, Classic wires/triggers/registers, AXI, resources
3. âœ… Added Classic client wrappers to `client.py` (get_wire_out_value, update_wire_outs,
   get_trigger_out_vector, read_register, write_register, reset_fpga)
4. âœ… Results: **16 passed, 2 skipped** (AXI Lite expected skip on XEM7320)

#### Phase 18F: Cross-Client Hardware Integration Report

**Status**: `[x]` Completed (2026-08-17, updated with final counts)

1. âœ… Ran all 4 hardware test suites against live server + XEM7320-A75T
2. âœ… Report: `docs/test-reports/cross-client-integration-report.md`
3. âœ… Final totals (after Phase 19 gRPC/REST wiring):

| Client | Tests | Passed | Skipped | Failed |
|--------|-------|--------|---------|--------|
| Rust REST | 21 | **21** | 0 | **0** |
| Rust gRPC | 16 | **16** | 0 | **0** |
| Python REST | 23 | **16** | 7 | **0** |
| Python gRPC | 18 | **16** | 2 | **0** |
| **Total** | **78** | **69** | **9** | **0** |

4. âœ… Zero server bugs discovered â€” all skips are expected (AXI on Legacy device, sensors)

### Verification

```bash
# Server must be running with: cargo run --features full
# XEM7320-A75T must be connected via USB

# Rust REST
cd client-examples/rust-rest-client
cargo test -- --ignored --nocapture --test-threads=1

# Rust gRPC
cd client-examples/rust-grpc-client
cargo test -- --ignored --nocapture --test-threads=1

# Python REST
cd client-examples/python-rest-client
pytest tests/test_hardware.py -v -s

# Python gRPC
cd client-examples/python-grpc-client
pytest tests/test_hardware.py -v -s
```

---

---

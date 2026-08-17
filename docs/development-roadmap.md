# ok-FPGA-Server — Development Roadmap

**Version**: 1.0  
**Date**: 2026-08-16  
**Purpose**: Step-by-step development plan that any AI assistant can follow to build this project from scratch. Each phase has explicit inputs, outputs, verification steps, and acceptance criteria.

---

## How to Use This Roadmap

1. **Work one phase at a time**, in order. Do not skip ahead.
2. **Each phase must pass its verification steps** before moving to the next.
3. **Read the referenced ADR** before starting each phase — it contains the design decisions.
4. **Follow the commenting rules** in `.agents/rules/ai-collaboration.md` §8.
5. **Commit after each successful build** per `.agents/rules/ai-collaboration.md` §7.
6. **Update this roadmap** when a phase is completed (change `[ ]` to `[x]` and add the completion date).

---

## Phase 0: Project Scaffolding

**Status**: `[x]` Completed (2026-08-16, `c56145b`)  
**Depends on**: Nothing  
**ADR reference**: [Architecture Overview](architecture/overview.md)

### What to do

1. Initialize a Cargo workspace at the project root with three crates:
   ```
   ok-fpga-server/
   ├── Cargo.toml              ← workspace root
   ├── crates/
   │   ├── ok-fpga-ffi/        ← FFI bindings to okFrontPanel.dll/.so
   │   │   ├── Cargo.toml
   │   │   ├── build.rs        ← rust-bindgen build script (empty stub for now)
   │   │   └── src/
   │   │       └── lib.rs
   │   ├── ok-fpga-core/       ← safe Rust domain logic
   │   │   ├── Cargo.toml
   │   │   └── src/
   │   │       └── lib.rs
   │   └── ok-fpga-server/     ← binary: REST + gRPC + web dashboard
   │       ├── Cargo.toml
   │       └── src/
   │           └── main.rs
   ├── proto/                   ← gRPC proto3 definitions
   │   └── okfpga/
   │       └── v1/
   │           └── okfpga.proto
   ├── api/                     ← OpenAPI spec
   │   └── openapi.yaml
   ├── FrontPanel-6.1.0/        ← existing SDK files (already present)
   ├── docs/                    ← existing architecture docs (already present)
   └── .agents/                 ← existing AI rules (already present)
   ```

2. In the workspace `Cargo.toml`:
   - Set `resolver = "2"`
   - List all three crates as workspace members
   - Define shared workspace dependencies (versions pinned)

3. In each crate's `lib.rs` or `main.rs`:
   - Add a `//!` crate-level doc comment explaining the crate's purpose
   - Add a placeholder `pub fn hello() -> &'static str { "ok-fpga-ffi" }` or equivalent
   - The server `main.rs` should print a startup banner and exit

4. Create a `.gitignore` that excludes `target/`, `*.pdb`, and IDE files.

### Verification

- [x] `cargo build` succeeds with no errors from the workspace root — ✅ 44.77s
- [x] `cargo test` succeeds (even if there are no real tests yet) — ✅ 2/2 passed
- [x] `cargo doc --no-deps` succeeds and shows all three crates with doc comments — ✅ 3 crates documented
- [x] Directory structure matches the layout above — ✅ verified
- [x] Commit: `c56145b` — `chore: initialize Cargo workspace with three crates`

---

## Phase 1: OpenAPI 3.0 Specification

**Status**: `[x]` Completed (2026-08-16, `44c2d0c`)  
**Depends on**: Phase 0  
**ADR reference**: [ADR-004 REST API Design](architecture/adr/ADR-004-rest-api-design.md)

### What to do

Create `api/openapi.yaml` — a complete OpenAPI 3.0.0 specification.

**The file must define ALL of the following** (refer to ADR-004 for the full URL hierarchy):

1. **Info block**: title "ok-FPGA-Server API", version "1.0.0", description, license
2. **Servers**: `http://localhost:8080/api/v1` (configurable)
3. **Paths** — every endpoint from ADR-004:
   - `/server/status` (GET)
   - `/server/restart` (POST)
   - `/server/logs` (GET, SSE stream)
   - `/devices` (GET — list all)
   - `/devices/{serial}` (GET — device info)
   - `/devices/{serial}/sensors` (GET)
   - `/devices/{serial}/settings` (GET — list all settings)
   - `/devices/{serial}/settings/{key}` (GET, PUT)
   - `/devices/{serial}/syzygy` (GET)
   - `/devices/{serial}/configure` (POST — upload bitfile, DELETE — clear)
   - `/devices/{serial}/bitfile` (GET)
   - `/devices/{serial}/dataport` (GET)
   - `/devices/{serial}/classic/wires/in` (POST)
   - `/devices/{serial}/classic/wires/out` (GET)
   - `/devices/{serial}/classic/triggers/in` (POST)
   - `/devices/{serial}/classic/triggers/out` (GET)
   - `/devices/{serial}/classic/pipes/in` (POST)
   - `/devices/{serial}/classic/pipes/out` (GET)
   - `/devices/{serial}/classic/pipes/block-in` (POST)
   - `/devices/{serial}/classic/pipes/block-out` (GET)
   - `/devices/{serial}/classic/registers/read` (POST)
   - `/devices/{serial}/classic/registers/write` (POST)
   - `/devices/{serial}/classic/reset` (POST)
   - `/devices/{serial}/axi/lite/read` (POST)
   - `/devices/{serial}/axi/lite/write` (POST)
   - `/devices/{serial}/axi/lite/reset` (POST)
   - `/devices/{serial}/axi/full/read` (POST)
   - `/devices/{serial}/axi/full/write` (POST)
   - `/devices/{serial}/axi/full/datapath-width` (GET)
   - `/devices/{serial}/axi/full/reset` (POST)
   - `/devices/{serial}/axi/stream/read` (POST)
   - `/devices/{serial}/axi/stream/write` (POST)
   - `/devices/{serial}/axi/stream/datapath-width` (GET)
   - `/devices/{serial}/axi/stream/reset` (POST)
   - `/resources` (GET)
   - `/resources/{resource_id}` (GET)
   - `/resources/{resource_id}/acquire` (POST)
   - `/resources/{resource_id}/release` (POST)
   - `/users` (GET)
   - `/users/{user_id}` (GET)
   - `/events/stream` (GET, SSE)

4. **Schemas** (in `components/schemas`):
   - `DeviceInfo` — maps every field of `okTDeviceInfo` to JSON
   - `DeviceSensor` — maps `okTDeviceSensor`
   - `DeviceSetting` — key-value with type
   - `SyzygyPort` — all SYZYGY DNA fields
   - `BitfileInfo` — filename, description, timestamp, user
   - `Resource` — id, name, state, holders, device_serial
   - `ResourceState` — enum: unavailable, initializing, available, in_use_shared, in_use_locked
   - `UserSession` — user_id, connected_at, held_resources
   - `ServerStatus` — uptime, api_version, frontpanel_version, platform
   - `ErrorResponse` — RFC 7807 Problem Details (type, title, status, detail)
   - `WireInRequest`, `WireOutResponse`, `TriggerInRequest`, `TriggerOutResponse`
   - `PipeInRequest`, `PipeOutResponse`, `RegisterReadRequest`, `RegisterWriteRequest`
   - `RegisterEntry` — address + data (maps `okTRegisterEntry`)
   - `AXILiteReadRequest/Response`, `AXILiteWriteRequest`
   - `AXIFullReadRequest/Response`, `AXIFullWriteRequest`
   - `AXIStreamReadRequest/Response`, `AXIStreamWriteRequest`
   - `ConfigureFPGARequest` — multipart file upload
   - `DeviceEvent` — discriminated union for SSE events
   - All request/response bodies for acquire/release

5. **Parameters**: `serial` (path, string, 10 chars), `key` (path, string), `resource_id` (path, string)

6. **Security schemes** (placeholder): HTTP Basic (for future auth stubs)

### Verification

- [x] YAML is valid (parse with any YAML validator) — ✅ parsed successfully
- [x] OpenAPI is valid: `npx @redocly/cli lint` — ✅ 0 errors, 31 warnings (all acknowledged `operation-4xx-response`)
- [x] Every endpoint from ADR-004 is present — ✅ 41 paths, 43 operations
- [x] Every schema has a `description` field explaining what it represents — ✅ verified by script
- [x] Commit: `44c2d0c` + `7767a71` (lint fix)

---

## Phase 2: Proto3 gRPC Specification

**Status**: `[x]` Completed (2026-08-16, `44c2d0c`)  
**Depends on**: Phase 0  
**ADR reference**: [ADR-005 gRPC API Design](architecture/adr/ADR-005-grpc-api-design.md)

### What to do

Create `proto/okfpga/v1/okfpga.proto` — a complete proto3 service definition.

**The file must define ALL of the following** (refer to ADR-005):

1. **Package**: `okfpga.v1`
2. **Options**: java_multiple_files, java_package, objc_class_prefix (from project goals)
3. **Three services**:
   - `DeviceService` — all RPCs from ADR-005 §1
   - `ClassicDataPortService` — all RPCs from ADR-005 §2
   - `AXIDataPortService` — all RPCs from ADR-005 §3
4. **All message types** from ADR-005:
   - Common: `DeviceRequest`, `StatusResponse`, `ErrorCode`, `ErrorCluster`
   - Device: `DeviceInfoResponse`, `DeviceSensorsResponse`, `DeviceSettingsResponse`
   - Events: `DeviceEvent` (oneof), `DeviceAdded`, `DeviceRemoved`, `BitfileProgrammed`, `ResourceStateChanged`
   - Classic: all wire/trigger/pipe/register request/response messages
   - AXI: all lite/full/stream request/response messages
   - Resources: acquire/release messages
   - Config: `ConfigureFPGARequest`, `ConfigureFPGAChunk` (for client streaming)
5. **Enums**: `DeviceInterface`, `USBSpeed`, `FPGAVendor`, `DataportType`, `ResourceState`, `SensorType`
6. **Comments**: Every message, field, enum, service, and RPC must have a proto comment explaining its purpose.

### Verification

- [x] Proto file compiles — ✅ `protoc 35.1` — exit code 0, no errors
- [x] Every RPC from ADR-005 is present — ✅ 60 RPCs across 3 services verified
- [x] Every message field has a comment — ✅ 0 uncommented fields (212 added in `47e037f`)
- [x] Commit: `44c2d0c` + `47e037f` (field comments)

---

## Phase 3: FFI Layer (`ok-fpga-ffi`)

**Status**: `[x]` Complete  
**Depends on**: Phase 0  
**ADR reference**: [ADR-001 FFI Strategy](architecture/adr/ADR-001-ffi-strategy.md), [FP6 API Surface Catalog](architecture/fp6-api-surface.md)

### What to do

This phase builds the low-level Rust FFI bindings. It has **three sub-phases**:

#### 3A: Build Script & Bindgen Setup

1. Add `bindgen` as a build dependency in `ok-fpga-ffi/Cargo.toml`
2. Create `ok-fpga-ffi/build.rs` that:
   - Points `bindgen` at `FrontPanel-6.1.0/okFrontPanel.h`
   - Defines `okNO_LUA` (exclude Lua scripting API)
   - Defines `OPALKELLY_NO_CXX_TYPES` (exclude C++ class wrappers)
   - Blocklists: `okFrontPanel_WriteI2C`, `okFrontPanel_ReadI2C`, `okFrontPanel_ConstructFromFD`, all `okBuffer_*`, `okScriptValue_*`, `okScriptValues_*`, `okScriptEngine_*`, all `okFrontPanelDLL_*`
   - Generates output to `$OUT_DIR/okfrontpanel_bindings.rs`
3. In `ok-fpga-ffi/src/lib.rs`, include the generated bindings:
   ```rust
   #![allow(non_upper_case_globals, non_camel_case_types, non_snake_case)]
   include!(concat!(env!("OUT_DIR"), "/okfrontpanel_bindings.rs"));
   ```

**Verification 3A**:
- [x] `cargo build -p ok-fpga-ffi` succeeds — ✅ 10.34s
- [x] Generated bindings contain `okFrontPanel_Construct`, `okTDeviceInfo`, `ok_ErrorCode` — ✅ all present (1651 lines)
- [x] Generated bindings do NOT contain `okFrontPanel_WriteI2C` or any Lua/DLL functions — ✅ all absent
- [x] Commit: `494a87b` — `feat(ffi): add rust-bindgen build script for okFrontPanel.h`

#### 3B: Runtime DLL Loading with `libloading`

1. Add `libloading` as a dependency in `ok-fpga-ffi/Cargo.toml`
2. Create `ok-fpga-ffi/src/api.rs` — the function dispatch table:
   - Define a `FrontPanelApi` struct that holds a `libloading::Library` and `Symbol` for every C function listed in the [FP6 API Surface Catalog](architecture/fp6-api-surface.md) (sections 1-15, ~160 functions)
   - Implement `FrontPanelApi::load(dll_path: &Path) -> Result<Self, LoadError>` that:
     - Opens the library with `Library::new()`
     - Resolves every symbol, returning a clear error if any are missing
     - Verifies API version ≥ 6.1.0
   - Implement one Rust method per C function, delegating to the loaded symbol
   - Group methods by subsystem (version, device, config, classic_port, axi_port, settings, sensors, pll, flash, manager, devices)
3. Create `ok-fpga-ffi/src/error.rs` — error types:
   - `FfiError` enum wrapping `ok_ErrorCode` values
   - `impl From<ok_ErrorCode> for FfiError`
   - Human-readable display for each error variant

**Verification 3B**:
- [x] `cargo build -p ok-fpga-ffi` succeeds — ✅ 0 warnings
- [x] `cargo doc -p ok-fpga-ffi --no-deps` shows all public API with doc comments — ✅ 0 warnings
- [x] Commit: `ab650d6` — `feat(ffi): add libloading runtime DLL dispatch table`

#### 3C: Safe RAII Handle Wrappers

1. Create `ok-fpga-ffi/src/handles.rs` — RAII wrappers:
   - `FrontPanelHandle` — wraps `okFrontPanel_HANDLE`, calls `Destruct` on drop
   - `PLL22150Handle` — wraps `okPLL22150_HANDLE`
   - `PLL22393Handle` — wraps `okPLL22393_HANDLE`
   - `DeviceSensorsHandle` — wraps `okDeviceSensors_HANDLE`
   - `DeviceSettingsHandle` — wraps `okDeviceSettings_HANDLE`
   - `DeviceSettingNamesHandle` — wraps `okDeviceSettingNames_HANDLE`
   - Each wrapper stores a reference to `FrontPanelApi` (so it can call destruct)
   - Each wrapper implements `Drop`
   - Non-owning handles (data ports) use a different pattern — they store a raw handle with a lifetime or `PhantomData` marker
2. Write unit tests using `okREALM_TEST` (if possible at this stage) or mock tests

**Verification 3C**:
- [x] `cargo build -p ok-fpga-ffi` succeeds — ✅ 0 warnings
- [x] `cargo test -p ok-fpga-ffi` succeeds — ✅ 9/9 passed
- [x] Every `unsafe` block has a comment explaining why it is sound — ✅ 24/24 audited
- [x] Commit: `b724359` — `feat(ffi): add RAII handle wrappers for all FrontPanel handles`

---

## Phase 4: Core Domain Layer (`ok-fpga-core`)

**Status**: `[x]` Complete  
**Depends on**: Phase 3  
**ADR references**: [ADR-002](architecture/adr/ADR-002-device-lifecycle.md), [ADR-003](architecture/adr/ADR-003-data-port-abstraction.md), [ADR-006](architecture/adr/ADR-006-resource-management.md), [ADR-009](architecture/adr/ADR-009-board-syzygy.md)

### What to do (sub-phases)

#### 4A: Device Types & Board Recognition

1. Create `ok-fpga-core/src/device.rs`:
   - `FpgaDevice` struct: serial, product_name, product_id, device_interface, device info, bitfile record
   - `DeviceState` enum: `Connected`, `Programmed { bitfile: BitfileRecord }`
   - `BitfileRecord` struct: filename, description, programmed_at, programmed_by
   - `SupportedBoard` — recognition logic from ADR-009 (the `SUPPORTED_PRODUCTS` list and `is_supported_device()`)
2. Create `ok-fpga-core/src/syzygy.rs`:
   - `SyzygyPort` struct
   - `discover_syzygy_ports()` function implementing the algorithm from ADR-009
3. Create `ok-fpga-core/src/sensors.rs`:
   - `DeviceSensor` struct (Rust-friendly version of `okTDeviceSensor`)
   - `SensorType` enum
   - `read_sensors()` function

#### 4B: Data Port Abstractions

1. Create `ok-fpga-core/src/dataport.rs`:
   - `DataPort` trait
   - `ClassicDataPort` struct with all 19 methods from ADR-003
   - `AXIDataPort` struct with sub-interfaces
   - `ActiveDataPort` enum

#### 4C: Device Manager & Hot-Plug

1. Create `ok-fpga-core/src/manager.rs`:
   - `DeviceManager` struct wrapping `FrontPanelManager`
   - `DeviceRegistry` (thread-safe HashMap of serial → DeviceState)
   - Event channel (tokio broadcast)
   - `DeviceEvent` enum: `Added`, `Removed`, `Programmed`, `ResourceChanged`
   - USB 2.0 filtering in the `on_device_added` handler

#### 4D: Resource Management

1. Create `ok-fpga-core/src/resource.rs`:
   - `Resource` struct, `ResourceId`, `ResourceState` enum from ADR-006
   - `ResourceAccessPolicy` trait with default implementation
   - `ResourceRegistry` (thread-safe)
   - Acquire/release logic
   - Invalidation on device disconnect/reprogram

### Verification

- [x] `cargo build -p ok-fpga-core` succeeds — ✅ 0 warnings
- [x] `cargo test -p ok-fpga-core` succeeds — ✅ 46/46 passed (40 unit + 6 doc-tests)
- [x] `cargo doc -p ok-fpga-core --no-deps` shows clean documentation — ✅ 0 warnings
- [x] Commit: `5762395` — `feat(core): implement Phase 4 — Core Domain Layer`

---

## Phase 4E: Service Manager

**Status**: `[x]` Completed (2026-08-16, `025de4b`)  
**Depends on**: Phase 4  
**ADR reference**: [ADR-010](architecture/adr/ADR-010-service-manager.md)

### What to do

1. Create `ok-fpga-core/src/service.rs`:
   - `ManagedService` trait (`name()`, `status()`, `start()`, `stop()`, `restart()`)
   - `ServiceStatus` enum: `Running`, `Stopped`, `Starting`, `Stopping`, `Failed`, `NotCompiled`
   - `ServiceError` error type
   - `ServiceEvent` enum: `Started`, `Stopped`, `Failed`
2. Create `ok-fpga-core/src/service_manager.rs`:
   - `ServiceManager` struct with `HashMap<String, Arc<dyn ManagedService>>`
   - `register()`, `start()`, `stop()`, `restart()`, `list()` methods
   - `tokio::broadcast` channel for `ServiceEvent`
   - Shutdown ordering: stop services in reverse-registration order, MQTT last (before management server)

### Verification

- [x] `cargo build -p ok-fpga-core` succeeds — ✅ 0 warnings
- [x] `cargo test -p ok-fpga-core` succeeds — ✅ 84 unit + 10 doc-tests = 94 total, 0 failures
- [x] Commit: `025de4b` — `feat(core): add ServiceManager and ManagedService trait`

---

## Phase 5: gRPC Server (`ok-fpga-server`)

**Status**: `[ ]` Not started  
**Depends on**: Phase 2, Phase 4E  
**ADR references**: [ADR-005](architecture/adr/ADR-005-grpc-api-design.md), [ADR-010](architecture/adr/ADR-010-service-manager.md)  
**Feature flag**: `grpc` (NOT a default feature)

### What to do

1. Add `tonic`, `tonic-build`, `prost` as optional dependencies behind the `grpc` feature flag
2. Create `ok-fpga-server/build.rs` that compiles `proto/okfpga/v1/okfpga.proto`
3. Implement all four services:
   - `DeviceServiceImpl` — delegates to `DeviceManager` and `FpgaDevice`
   - `ClassicDataPortServiceImpl` — delegates to `ClassicDataPort`
   - `AXIDataPortServiceImpl` — delegates to `AXIDataPort`
   - `ServerServiceImpl` — delegates to `ServiceManager` for lifecycle RPCs
4. Implement `WatchDeviceEvents` server streaming
5. Implement the `ManagedService` trait for the gRPC server (start/stop/restart)
6. Gate all gRPC code with `#[cfg(feature = "grpc")]`

### Verification

- [ ] `cargo build -p ok-fpga-server --features grpc` succeeds
- [ ] `cargo build -p ok-fpga-server` succeeds WITHOUT gRPC
- [ ] gRPC reflection or `grpcurl` can list services
- [ ] Commit: `feat(server): implement gRPC services behind feature flag`

---

## Phase 6: REST Server (`ok-fpga-server`)

**Status**: `[ ]` Not started  
**Depends on**: Phase 1, Phase 4E  
**ADR references**: [ADR-004](architecture/adr/ADR-004-rest-api-design.md), [ADR-010](architecture/adr/ADR-010-service-manager.md)  
**Feature flag**: `rest` (default feature)

### What to do

1. Add `axum`, `serde`, `serde_json`, `tower`, `tower-http` as optional dependencies behind the `rest` feature flag
2. Create route modules matching the URL hierarchy from ADR-004:
   - `src/routes/server.rs` — status, restart, logs, **service lifecycle endpoints**
   - `src/routes/devices.rs` — device CRUD, sensors, settings, SYZYGY, configure
   - `src/routes/classic.rs` — wire/trigger/pipe/register operations
   - `src/routes/axi.rs` — AXI lite/full/stream operations
   - `src/routes/resources.rs` — resource list, acquire, release
   - `src/routes/users.rs` — user list
   - `src/routes/events.rs` — SSE event stream
3. Each route handler delegates to `ok-fpga-core` domain types
4. Implement RFC 7807 error responses
5. Implement `ManagedService` trait for the REST server
6. Gate all REST code with `#[cfg(feature = "rest")]`

### Verification

- [ ] `cargo build -p ok-fpga-server --features rest` succeeds
- [ ] `cargo build -p ok-fpga-server --no-default-features` succeeds WITHOUT REST
- [ ] `curl http://localhost:8080/api/v1/server/status` returns valid JSON
- [ ] `curl http://localhost:8080/api/v1/server/services` returns service list
- [ ] Commit: `feat(server): implement REST API routes behind feature flag`

---

## Phase 7: Configuration & Logging

**Status**: `[ ]` Not started  
**Depends on**: Phase 5, Phase 6  
**ADR reference**: [ADR-010](architecture/adr/ADR-010-service-manager.md)

### What to do

1. Add `toml`, `config`, `tracing`, `tracing-subscriber` dependencies
2. Create `src/config.rs`:
   - Top-level config struct with per-service sections:
     ```toml
     [server]
     management_port = 9090
     
     [rest]
     enabled = true
     bind = "0.0.0.0"
     port = 8080
     
     [grpc]
     enabled = true
     bind = "0.0.0.0"
     port = 50051
     
     [mqtt]
     enabled = false
     broker_url = "mqtt://localhost:1883"
     client_id = "okfpga-server-01"
     topic_prefix = "okfpga/server01"
     publish_interval_secs = 5
     
     [mdns]
     enabled = false
     instance_name = "okfpga-lab-bench-1"
     
     [dashboard]
     enabled = true
     ```
   - Environment variable overrides
   - Default config file path
   - Validation: log warning (don't panic) if feature not compiled but `enabled = true`
3. Create a sample `config.toml` at the project root
4. Set up `tracing` with:
   - Console output (stdout)
   - Log level from config
   - Broadcast channel for web dashboard log streaming
5. Wire config loading into `main.rs`
6. Start management server on `management_port` (always on)
7. Register and start enabled services via `ServiceManager`

### Verification

- [ ] Server starts with default config (management port + REST)
- [ ] Server starts with custom config file path via CLI arg or env var
- [ ] Log output appears on stdout with timestamps and levels
- [ ] Non-compiled features log a warning when `enabled = true`
- [ ] Commit: `feat(server): add TOML configuration and tracing`

---

## Phase 8: Web Dashboard

**Status**: `[ ]` Not started  
**Depends on**: Phase 7  
**ADR references**: [ADR-007](architecture/adr/ADR-007-web-dashboard.md), [ADR-010](architecture/adr/ADR-010-service-manager.md)  
**Feature flag**: `dashboard` (default feature)

### What to do

1. Create `web/` directory with static HTML/CSS/JS files
2. Build the SPA pages from ADR-007:
   - Server overview (uptime, version, device count)
   - Device list and detail
   - Resource list
   - User list
   - Log viewer (real-time via WebSocket)
   - **Service management panel** (start/stop/restart services via `/mgmt/services/*`)
3. Serve dashboard on the **management port** (9090), NOT the REST port
4. Create management API routes on the management server:
   - `GET /mgmt/services` — list service status
   - `POST /mgmt/services/{name}/start` — start service
   - `POST /mgmt/services/{name}/stop` — stop service
   - `POST /mgmt/services/{name}/restart` — restart service
   - `GET /mgmt/config` — current config (sanitized)
   - `POST /mgmt/shutdown` — graceful server shutdown
5. Implement WebSocket endpoint (`/ws/events`) on management port
6. Connect the log broadcast channel to WebSocket
7. Gate dashboard static file serving with `#[cfg(feature = "dashboard")]`

### Verification

- [ ] `http://localhost:9090/` serves the dashboard
- [ ] Dashboard shows server status and service states
- [ ] Dashboard can start/stop/restart services
- [ ] Log viewer shows real-time logs
- [ ] `cargo build --no-default-features` succeeds without dashboard
- [ ] Commit: `feat(web): add dashboard SPA on management port`

---

## Phase 9: Authentication Stubs

**Status**: `[ ]` Not started  
**Depends on**: Phase 6  
**ADR reference**: [ADR-008](architecture/adr/ADR-008-authentication.md)

### What to do

1. Create `ok-fpga-core/src/auth.rs`:
   - `AuthProvider` trait
   - `NoAuthProvider` (default)
   - `LocalAuthProvider` stub
   - `LdapAuthProvider` stub with `LdapConfig`
2. Add auth config section to TOML config
3. Create Axum middleware layer for auth checking
4. Add login/logout endpoints for web dashboard

### Verification

- [ ] Server starts in no-auth mode by default
- [ ] Setting `auth.provider = "local"` in config activates local auth (stub returns success)
- [ ] Commit: `feat(auth): add pluggable authentication stubs`

---

## Phase 10: MQTT Publisher

**Status**: `[ ]` Not started  
**Depends on**: Phase 4E, Phase 7  
**ADR reference**: [ADR-011](architecture/adr/ADR-011-mqtt-publishing.md)  
**Feature flag**: `mqtt` (NOT a default feature)

### What to do

1. Add `rumqttc` as an optional dependency behind the `mqtt` feature flag
2. Create `ok-fpga-server/src/mqtt.rs`:
   - `MqttPublisher` struct implementing `ManagedService`
   - Connect to external MQTT broker from TOML config
   - Retry with exponential backoff if broker unavailable
3. Subscribe to `DeviceManager` event channel and publish:
   - `{prefix}/status` — periodic heartbeat (QoS 0)
   - `{prefix}/services/status` — on service state change (QoS 1, retained)
   - `{prefix}/devices/inventory` — on device add/remove (QoS 1, retained)
   - `{prefix}/devices/{serial}/state` — on state change (QoS 1, retained)
   - `{prefix}/devices/{serial}/sensors` — periodic readings (QoS 0)
   - `{prefix}/devices/{serial}/events` — on device events (QoS 1)
   - `{prefix}/resources/inventory` — on resource change (QoS 1, retained)
4. All messages use versioned JSON format from ADR-011
5. Gate all MQTT code with `#[cfg(feature = "mqtt")]`
6. MQTT stops last during shutdown (so it can publish shutdown events)

### Verification

- [ ] `cargo build -p ok-fpga-server --features mqtt` succeeds
- [ ] `cargo build -p ok-fpga-server` succeeds WITHOUT MQTT
- [ ] Server starts without MQTT broker (logs warning, retries)
- [ ] With Mosquitto running, `mosquitto_sub -t "okfpga/#"` receives messages
- [ ] Commit: `feat(mqtt): add MQTT status publisher behind feature flag`

---

## Phase 11: mDNS Service Discovery

**Status**: `[ ]` Not started  
**Depends on**: Phase 4E, Phase 7  
**ADR reference**: [ADR-012](architecture/adr/ADR-012-mdns-discovery.md)  
**Feature flag**: `mdns` (NOT a default feature)

### What to do

1. Add `mdns-sd` as an optional dependency behind the `mdns` feature flag
2. Create `ok-fpga-server/src/mdns.rs`:
   - `MdnsService` struct implementing `ManagedService`
   - Register `_okfpga._tcp` service with TXT records:
     - `version` — API version
     - `rest` — REST port (if enabled)
     - `grpc` — gRPC port (if enabled)
     - `mqtt` — MQTT topic prefix (if enabled)
     - `dashboard` — management port (if enabled)
     - `devices` — connected device count (updated dynamically)
   - Instance name from TOML config (default: `okfpga-{hostname}`)
3. Update TXT records when:
   - Services start/stop (add/remove port keys)
   - Devices connect/disconnect (update `devices` count)
4. Gate all mDNS code with `#[cfg(feature = "mdns")]`

### Verification

- [ ] `cargo build -p ok-fpga-server --features mdns` succeeds
- [ ] `cargo build -p ok-fpga-server` succeeds WITHOUT mDNS
- [ ] `dns-sd -B _okfpga._tcp` (macOS) or `avahi-browse _okfpga._tcp` (Linux) discovers the server
- [ ] TXT records include correct port numbers for enabled services
- [ ] Commit: `feat(mdns): add mDNS service discovery behind feature flag`

---

## Phase 12: Integration Testing

**Status**: `[ ]` Not started  
**Depends on**: All previous phases

### What to do

1. Create `tests/` directory at workspace root
2. Write integration tests using `okREALM_TEST`:
   - Emulate device connection → verify REST `/devices` returns it
   - Emulate device disconnection → verify SSE event fires
   - Test resource acquire/release via REST
   - Test gRPC `WatchDeviceEvents` streaming
3. Write API contract tests:
   - Validate REST responses against the OpenAPI spec
   - Validate gRPC responses against proto definitions
4. Write service lifecycle tests:
   - Start/stop REST via management API
   - Start/stop gRPC via management API
   - Verify mDNS TXT records update on service start/stop
   - Verify MQTT publishes service state changes
5. Write feature flag combination tests:
   - Build with each individual feature
   - Build with `--features full`
   - Build with `--no-default-features`
6. Cross-platform CI setup:
   - GitHub Actions with Windows and Ubuntu runners
   - Test matrix: Ubuntu 22.04, Ubuntu 24.04, Windows 11

### Verification

- [ ] `cargo test` passes all integration tests
- [ ] `cargo build --features full` compiles all features
- [ ] `cargo build --no-default-features` compiles minimal binary
- [ ] CI pipeline is green on both Linux and Windows
- [ ] Commit: `test: add integration tests with okREALM_TEST`

---

## Phase 13: Documentation & Polish

**Status**: `[ ]` Not started  
**Depends on**: All previous phases

### What to do

1. Run `cargo doc --no-deps --open` and review all generated documentation
2. Add a top-level `README.md` with:
   - Project description
   - Quick start guide
   - Build instructions for Windows and Ubuntu
   - Feature flag reference (what each flag enables)
   - Configuration reference (all TOML sections)
   - API documentation links
3. Update `docs/architecture/overview.md` with final architecture
4. Ensure all ADRs have correct status
5. Create `CONTRIBUTING.md` with development setup instructions
6. Final `cargo clippy` and `cargo fmt` pass

### Verification

- [ ] `cargo clippy -- -D warnings` passes
- [ ] `cargo fmt -- --check` passes
- [ ] `README.md` is complete and accurate
- [ ] All doc comments render correctly in `cargo doc`
- [ ] Commit: `docs: finalize project documentation`

---

## Summary of Dependencies

```
Phase 0: Scaffolding
  ├── Phase 1: OpenAPI spec
  ├── Phase 2: Proto3 spec
  └── Phase 3: FFI layer (3A → 3B → 3C)
       └── Phase 4: Core domain (4A → 4B → 4C → 4D)
            └── Phase 4E: ServiceManager
                 ├── Phase 5: gRPC server (needs 2 + 4E) [feature: grpc]
                 ├── Phase 6: REST server (needs 1 + 4E) [feature: rest]
                 │    ├── Phase 7: Config & logging (needs 5 + 6)
                 │    │    ├── Phase 8: Web dashboard (needs 7) [feature: dashboard]
                 │    │    ├── Phase 10: MQTT publisher (needs 4E + 7) [feature: mqtt]
                 │    │    └── Phase 11: mDNS discovery (needs 4E + 7) [feature: mdns]
                 │    └── Phase 9: Auth stubs (needs 6)
                 └── Phase 12: Integration testing (needs all)
                      └── Phase 13: Documentation & polish (needs all)
```

Phases 1 and 2 can be done **in parallel** (both depend only on Phase 0).  
Phases 5 and 6 can be done **in parallel** (both depend on Phase 4E).  
Phases 10 and 11 can be done **in parallel** (both depend on Phase 4E + Phase 7).

### Feature Flag Summary

| Feature | Default | Dependencies Added | Phase |
|---------|---------|-------------------|-------|
| `rest` | ✅ Yes | axum, tower-http | 6 |
| `dashboard` | ✅ Yes | (static files only) | 8 |
| `grpc` | ❌ No | tonic, prost, tonic-build | 5 |
| `mqtt` | ❌ No | rumqttc | 10 |
| `mdns` | ❌ No | mdns-sd | 11 |
| `full` | ❌ No | all of the above | — |

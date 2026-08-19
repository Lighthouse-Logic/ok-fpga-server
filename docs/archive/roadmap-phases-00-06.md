# Roadmap Archive: Phases 0–6

> **Status**: Archived — these phases are complete.
> This file is a historical record. For current priorities, see
> [development-roadmap.md](../development-roadmap.md).

---
## Phase 0: Project Scaffolding

**Status**: `[x]` Completed (2026-08-16, `c56145b`)  
**Depends on**: Nothing  
**ADR reference**: [Architecture Overview](architecture/overview.md)

### What to do

1. Initialize a Cargo workspace at the project root with three crates:
   ```
   ok-fpga-server/
   â”œâ”€â”€ Cargo.toml              â† workspace root
   â”œâ”€â”€ crates/
   â”‚   â”œâ”€â”€ ok-fpga-ffi/        â† FFI bindings to okFrontPanel.dll/.so
   â”‚   â”‚   â”œâ”€â”€ Cargo.toml
   â”‚   â”‚   â”œâ”€â”€ build.rs        â† rust-bindgen build script (empty stub for now)
   â”‚   â”‚   â””â”€â”€ src/
   â”‚   â”‚       â””â”€â”€ lib.rs
   â”‚   â”œâ”€â”€ ok-fpga-core/       â† safe Rust domain logic
   â”‚   â”‚   â”œâ”€â”€ Cargo.toml
   â”‚   â”‚   â””â”€â”€ src/
   â”‚   â”‚       â””â”€â”€ lib.rs
   â”‚   â””â”€â”€ ok-fpga-server/     â† binary: REST + gRPC + web dashboard
   â”‚       â”œâ”€â”€ Cargo.toml
   â”‚       â””â”€â”€ src/
   â”‚           â””â”€â”€ main.rs
   â”œâ”€â”€ proto/                   â† gRPC proto3 definitions
   â”‚   â””â”€â”€ okfpga/
   â”‚       â””â”€â”€ v1/
   â”‚           â””â”€â”€ okfpga.proto
   â”œâ”€â”€ api/                     â† OpenAPI spec
   â”‚   â””â”€â”€ openapi.yaml
   â”œâ”€â”€ FrontPanel-6.1.0/        â† existing SDK files (already present)
   â”œâ”€â”€ docs/                    â† existing architecture docs (already present)
   â””â”€â”€ .agents/                 â† existing AI rules (already present)
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

- [x] `cargo build` succeeds with no errors from the workspace root â€” âœ… 44.77s
- [x] `cargo test` succeeds (even if there are no real tests yet) â€” âœ… 2/2 passed
- [x] `cargo doc --no-deps` succeeds and shows all three crates with doc comments â€” âœ… 3 crates documented
- [x] Directory structure matches the layout above â€” âœ… verified
- [x] Commit: `c56145b` â€” `chore: initialize Cargo workspace with three crates`

---

## Phase 1: OpenAPI 3.0 Specification

**Status**: `[x]` Completed (2026-08-16, `44c2d0c`)  
**Depends on**: Phase 0  
**ADR reference**: [ADR-004 REST API Design](architecture/adr/ADR-004-rest-api-design.md)

### What to do

Create `api/openapi.yaml` â€” a complete OpenAPI 3.0.0 specification.

**The file must define ALL of the following** (refer to ADR-004 for the full URL hierarchy):

1. **Info block**: title "ok-FPGA-Server API", version "1.0.0", description, license
2. **Servers**: `http://localhost:8080/api/v1` (configurable)
3. **Paths** â€” every endpoint from ADR-004:
   - `/server/status` (GET)
   - `/server/restart` (POST)
   - `/server/logs` (GET, SSE stream)
   - `/devices` (GET â€” list all)
   - `/devices/{serial}` (GET â€” device info)
   - `/devices/{serial}/sensors` (GET)
   - `/devices/{serial}/settings` (GET â€” list all settings)
   - `/devices/{serial}/settings/{key}` (GET, PUT)
   - `/devices/{serial}/syzygy` (GET)
   - `/devices/{serial}/configure` (POST â€” upload bitfile, DELETE â€” clear)
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
   - `DeviceInfo` â€” maps every field of `okTDeviceInfo` to JSON
   - `DeviceSensor` â€” maps `okTDeviceSensor`
   - `DeviceSetting` â€” key-value with type
   - `SyzygyPort` â€” all SYZYGY DNA fields
   - `BitfileInfo` â€” filename, description, timestamp, user
   - `Resource` â€” id, name, state, holders, device_serial
   - `ResourceState` â€” enum: unavailable, initializing, available, in_use_shared, in_use_locked
   - `UserSession` â€” user_id, connected_at, held_resources
   - `ServerStatus` â€” uptime, api_version, frontpanel_version, platform
   - `ErrorResponse` â€” RFC 7807 Problem Details (type, title, status, detail)
   - `WireInRequest`, `WireOutResponse`, `TriggerInRequest`, `TriggerOutResponse`
   - `PipeInRequest`, `PipeOutResponse`, `RegisterReadRequest`, `RegisterWriteRequest`
   - `RegisterEntry` â€” address + data (maps `okTRegisterEntry`)
   - `AXILiteReadRequest/Response`, `AXILiteWriteRequest`
   - `AXIFullReadRequest/Response`, `AXIFullWriteRequest`
   - `AXIStreamReadRequest/Response`, `AXIStreamWriteRequest`
   - `ConfigureFPGARequest` â€” multipart file upload
   - `DeviceEvent` â€” discriminated union for SSE events
   - All request/response bodies for acquire/release

5. **Parameters**: `serial` (path, string, 10 chars), `key` (path, string), `resource_id` (path, string)

6. **Security schemes** (placeholder): HTTP Basic (for future auth stubs)

### Verification

- [x] YAML is valid (parse with any YAML validator) â€” âœ… parsed successfully
- [x] OpenAPI is valid: `npx @redocly/cli lint` â€” âœ… 0 errors, 31 warnings (all acknowledged `operation-4xx-response`)
- [x] Every endpoint from ADR-004 is present â€” âœ… 41 paths, 43 operations
- [x] Every schema has a `description` field explaining what it represents â€” âœ… verified by script
- [x] Commit: `44c2d0c` + `7767a71` (lint fix)

---

## Phase 2: Proto3 gRPC Specification

**Status**: `[x]` Completed (2026-08-16, `44c2d0c`)  
**Depends on**: Phase 0  
**ADR reference**: [ADR-005 gRPC API Design](architecture/adr/ADR-005-grpc-api-design.md)

### What to do

Create `proto/okfpga/v1/okfpga.proto` â€” a complete proto3 service definition.

**The file must define ALL of the following** (refer to ADR-005):

1. **Package**: `okfpga.v1`
2. **Options**: java_multiple_files, java_package, objc_class_prefix (from project goals)
3. **Three services**:
   - `DeviceService` â€” all RPCs from ADR-005 Â§1
   - `ClassicDataPortService` â€” all RPCs from ADR-005 Â§2
   - `AXIDataPortService` â€” all RPCs from ADR-005 Â§3
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

- [x] Proto file compiles â€” âœ… `protoc 35.1` â€” exit code 0, no errors
- [x] Every RPC from ADR-005 is present â€” âœ… 60 RPCs across 3 services verified
- [x] Every message field has a comment â€” âœ… 0 uncommented fields (212 added in `47e037f`)
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
- [x] `cargo build -p ok-fpga-ffi` succeeds â€” âœ… 10.34s
- [x] Generated bindings contain `okFrontPanel_Construct`, `okTDeviceInfo`, `ok_ErrorCode` â€” âœ… all present (1651 lines)
- [x] Generated bindings do NOT contain `okFrontPanel_WriteI2C` or any Lua/DLL functions â€” âœ… all absent
- [x] Commit: `494a87b` â€” `feat(ffi): add rust-bindgen build script for okFrontPanel.h`

#### 3B: Runtime DLL Loading with `libloading`

1. Add `libloading` as a dependency in `ok-fpga-ffi/Cargo.toml`
2. Create `ok-fpga-ffi/src/api.rs` â€” the function dispatch table:
   - Define a `FrontPanelApi` struct that holds a `libloading::Library` and `Symbol` for every C function listed in the [FP6 API Surface Catalog](architecture/fp6-api-surface.md) (sections 1-15, ~160 functions)
   - Implement `FrontPanelApi::load(dll_path: &Path) -> Result<Self, LoadError>` that:
     - Opens the library with `Library::new()`
     - Resolves every symbol, returning a clear error if any are missing
     - Verifies API version â‰¥ 6.1.0
   - Implement one Rust method per C function, delegating to the loaded symbol
   - Group methods by subsystem (version, device, config, classic_port, axi_port, settings, sensors, pll, flash, manager, devices)
3. Create `ok-fpga-ffi/src/error.rs` â€” error types:
   - `FfiError` enum wrapping `ok_ErrorCode` values
   - `impl From<ok_ErrorCode> for FfiError`
   - Human-readable display for each error variant

**Verification 3B**:
- [x] `cargo build -p ok-fpga-ffi` succeeds â€” âœ… 0 warnings
- [x] `cargo doc -p ok-fpga-ffi --no-deps` shows all public API with doc comments â€” âœ… 0 warnings
- [x] Commit: `ab650d6` â€” `feat(ffi): add libloading runtime DLL dispatch table`

#### 3C: Safe RAII Handle Wrappers

1. Create `ok-fpga-ffi/src/handles.rs` â€” RAII wrappers:
   - `FrontPanelHandle` â€” wraps `okFrontPanel_HANDLE`, calls `Destruct` on drop
   - `PLL22150Handle` â€” wraps `okPLL22150_HANDLE`
   - `PLL22393Handle` â€” wraps `okPLL22393_HANDLE`
   - `DeviceSensorsHandle` â€” wraps `okDeviceSensors_HANDLE`
   - `DeviceSettingsHandle` â€” wraps `okDeviceSettings_HANDLE`
   - `DeviceSettingNamesHandle` â€” wraps `okDeviceSettingNames_HANDLE`
   - Each wrapper stores a reference to `FrontPanelApi` (so it can call destruct)
   - Each wrapper implements `Drop`
   - Non-owning handles (data ports) use a different pattern â€” they store a raw handle with a lifetime or `PhantomData` marker
2. Write unit tests using `okREALM_TEST` (if possible at this stage) or mock tests

**Verification 3C**:
- [x] `cargo build -p ok-fpga-ffi` succeeds â€” âœ… 0 warnings
- [x] `cargo test -p ok-fpga-ffi` succeeds â€” âœ… 9/9 passed
- [x] Every `unsafe` block has a comment explaining why it is sound â€” âœ… 24/24 audited
- [x] Commit: `b724359` â€” `feat(ffi): add RAII handle wrappers for all FrontPanel handles`

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
   - `SupportedBoard` â€” recognition logic from ADR-009 (the `SUPPORTED_PRODUCTS` list and `is_supported_device()`)
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
   - `DeviceRegistry` (thread-safe HashMap of serial â†’ DeviceState)
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

- [x] `cargo build -p ok-fpga-core` succeeds â€” âœ… 0 warnings
- [x] `cargo test -p ok-fpga-core` succeeds â€” âœ… 46/46 passed (40 unit + 6 doc-tests)
- [x] `cargo doc -p ok-fpga-core --no-deps` shows clean documentation â€” âœ… 0 warnings
- [x] Commit: `5762395` â€” `feat(core): implement Phase 4 â€” Core Domain Layer`

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

- [x] `cargo build -p ok-fpga-core` succeeds â€” âœ… 0 warnings
- [x] `cargo test -p ok-fpga-core` succeeds â€” âœ… 84 unit + 10 doc-tests = 94 total, 0 failures
- [x] Commit: `025de4b` â€” `feat(core): add ServiceManager and ManagedService trait`

---

## Phase 5: gRPC Server (`ok-fpga-server`)

**Status**: `[x]` Completed (2026-08-16, `8edfdce`)  
**Depends on**: Phase 2, Phase 4E  
**ADR references**: [ADR-005](architecture/adr/ADR-005-grpc-api-design.md), [ADR-010](architecture/adr/ADR-010-service-manager.md)  
**Feature flag**: `grpc` (NOT a default feature)

### What to do

1. Add `tonic`, `tonic-build`, `prost` as optional dependencies behind the `grpc` feature flag
2. Create `ok-fpga-server/build.rs` that compiles `proto/okfpga/v1/okfpga.proto`
3. Implement all four services:
   - `DeviceServiceImpl` â€” delegates to `DeviceManager` and `FpgaDevice`
   - `ClassicDataPortServiceImpl` â€” delegates to `ClassicDataPort`
   - `AXIDataPortServiceImpl` â€” delegates to `AXIDataPort`
   - `ServerServiceImpl` â€” delegates to `ServiceManager` for lifecycle RPCs
4. Implement `WatchDeviceEvents` server streaming
5. Implement the `ManagedService` trait for the gRPC server (start/stop/restart)
6. Gate all gRPC code with `#[cfg(feature = "grpc")]`

### Verification

- [x] `cargo build -p ok-fpga-server --features grpc` succeeds â€” âœ…
- [x] `cargo build -p ok-fpga-server` succeeds WITHOUT gRPC â€” âœ…
- [ ] gRPC reflection or `grpcurl` can list services (requires running server)
- [x] Commit: `8edfdce` â€” `feat(server): implement Phase 5 (gRPC) and Phase 6 (REST) API servers`

---

## Phase 6: REST Server (`ok-fpga-server`)

**Status**: `[x]` Completed (2026-08-16, `8edfdce`)  
**Depends on**: Phase 1, Phase 4E  
**ADR references**: [ADR-004](architecture/adr/ADR-004-rest-api-design.md), [ADR-010](architecture/adr/ADR-010-service-manager.md)  
**Feature flag**: `rest` (default feature)

### What to do

1. Add `axum`, `serde`, `serde_json`, `tower`, `tower-http` as optional dependencies behind the `rest` feature flag
2. Create route modules matching the URL hierarchy from ADR-004:
   - `src/routes/server.rs` â€” status, restart, logs, **service lifecycle endpoints**
   - `src/routes/devices.rs` â€” device CRUD, sensors, settings, SYZYGY, configure
   - `src/routes/classic.rs` â€” wire/trigger/pipe/register operations
   - `src/routes/axi.rs` â€” AXI lite/full/stream operations
   - `src/routes/resources.rs` â€” resource list, acquire, release
   - `src/routes/users.rs` â€” user list
   - `src/routes/events.rs` â€” SSE event stream
3. Each route handler delegates to `ok-fpga-core` domain types
4. Implement RFC 7807 error responses
5. Implement `ManagedService` trait for the REST server
6. Gate all REST code with `#[cfg(feature = "rest")]`

### Verification

- [x] `cargo build -p ok-fpga-server --features rest` succeeds â€” âœ…
- [x] `cargo build -p ok-fpga-server --no-default-features` succeeds WITHOUT REST â€” âœ…
- [ ] `curl http://localhost:8080/api/v1/server/status` returns valid JSON (requires running server)
- [ ] `curl http://localhost:8080/api/v1/server/services` returns service list (requires running server)
- [x] Commit: `8edfdce` â€” `feat(server): implement Phase 5 (gRPC) and Phase 6 (REST) API servers`

---

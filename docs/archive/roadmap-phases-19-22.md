# Roadmap Archive: Phases 19–22

> **Status**: Archived — these phases are complete.
> This file is a historical record. For current priorities, see
> [development-roadmap.md](../development-roadmap.md).

---
## Phase 19: FFI Endpoint Wiring

**Status**: `[x]` Completed (2026-08-17)  
**Depends on**: Phase 17, Phase 18A  
**ADR references**: [ADR-017 â€” FFI Endpoint Wiring](architecture/adr/ADR-017-ffi-endpoint-wiring.md), [ADR-018 â€” Device Handle Pool](architecture/adr/ADR-018-device-handle-pool.md), [ADR-019 â€” AXI Interface Architecture](architecture/adr/ADR-019-axi-interface-architecture.md)

### Overview

Wire all 501-returning REST/gRPC endpoint stubs to the existing `FrontPanelDll`
FFI layer via a new `DeviceHandlePool`. After this phase, all hardware integration
tests exercise real FPGA hardware through the server's API.

### Sub-phases

#### Phase 19A: Device Handle Pool Infrastructure

**Status**: `[x]` Completed (2026-08-17)  
**Commit**: `b063467`

1. âœ… Created `crates/ok-fpga-server/src/handle_pool.rs`:
   - `DeviceHandlePool` struct with `open()`, `close()`, `get()`, `configure()` methods
   - `OpenDevice` struct holding raw handle + optional data port handles
   - Per-device `tokio::sync::Mutex` for thread-safe FFI serialization
2. âœ… Added `handle_pool: Arc<DeviceHandlePool>` to `AppState`
3. âœ… Wired into `main.rs` startup: open handles for each discovered device
4. âœ… Wired into `device_poll_loop`: skip "gone" detection for pool-held devices
5. âœ… `unsafe impl Send/Sync for OpenDevice` (raw handles are `!Send` by default)

#### Phase 19B: Configure FPGA + Device Endpoints

**Status**: `[x]` Completed (2026-08-17)  
**Commit**: `b063467`

Wired 4 device-level endpoints:

1. âœ… `POST /devices/{serial}/configure` â†’ `okFrontPanel_ConfigureFPGA` (JSON path)
   or `okFrontPanel_ConfigureFPGAFromMemory` (binary upload)
2. âœ… `GET /devices/{serial}/bitfile` â†’ return stored `BitfileRecord`
3. âœ… `GET /devices/{serial}/dataport` â†’ return cached `okTDeviceInfo.dataport`
4. â­ï¸ `GET /devices/{serial}/sensors` â†’ skipped (XEM7320 has no sensors)
5. â­ï¸ `GET /devices/{serial}/settings` â†’ deferred to Phase 19G (ADR-021)

#### Phase 19C: Classic Data Port Endpoints

**Status**: `[x]` Completed (2026-08-17)  
**Commit**: `b063467`

Wired all Classic handlers in `classic.rs` to FFI:

| Handler | FFI Call(s) | Status |
|---------|------------|--------|
| `set_wire_in` | `SetWireInValue` + `UpdateWireIns` | âœ… |
| `get_wire_out` | `UpdateWireOuts` + `GetWireOutValue` (per-address) | âœ… |
| `get_all_wire_outs` | `UpdateWireOuts` + `GetWireOutValue` (0x20â€“0x3F batch) | âœ… |
| `activate_trigger_in` | `ActivateTriggerIn` | âœ… |
| `is_triggered` | `UpdateTriggerOuts` + `IsTriggered` (per-address) | âœ… |
| `get_all_trigger_outs` | `UpdateTriggerOuts` + `GetTriggerOutVector` (0x60â€“0x7F batch) | âœ… |
| `write_to_pipe_in` | `WriteToPipeIn` | âœ… |
| `read_from_pipe_out` | `ReadFromPipeOut` | âœ… |
| `write_to_block_pipe_in` | `WriteToBlockPipeIn` | âœ… |
| `read_from_block_pipe_out` | `ReadFromBlockPipeOut` | âœ… |
| `read_register` | `ReadRegister` | âœ… |
| `write_register` | `WriteRegister` | âœ… |
| `reset_fpga` | `ResetFPGA` | âœ… |

#### Phase 19D: AXI Data Port Endpoints â€” All Three Sub-Interfaces

**Status**: `[/]` Partially complete â€” requires design fix  
**ADR reference**: [ADR-019 â€” AXI Interface Architecture](architecture/adr/ADR-019-axi-interface-architecture.md)

Per ADR-019, the three AXI sub-interfaces (Lite, Full, Stream) are **independent,
distinct protocols** that must NOT fall back to each other:

| Sub-Interface | Protocol | Addressing | Transfer Semantics |
|---------------|----------|-----------|-------------------|
| **AXI Lite** | AXI4-Lite | Explicit 32/64-bit | Single 32-bit register R/W (+ bulk) |
| **AXI Full** | AXI4 | Explicit 32/64-bit | Burst-capable byte-level memory-mapped |
| **AXI Stream** | AXI4-Stream | **Addressless** | Continuous ordered byte streaming (FIFO) |

**Handle acquisition** (in `acquire_data_ports()`):
1. âœ… Call `GetFPGADataPortAXI()` â€” if `ok_UnsupportedFeature`, AXI not supported
2. âœ… If `ok_NoError`, independently probe all three sub-handles:
   - `GetAXILite()` â†’ `axi_lite` (or `None`)
   - `GetAXIFull()` â†’ `axi_full` (or `None`)
   - `GetAXIStream()` â†’ `axi_stream` (or `None`)

**REST endpoints** (each uses ONLY its own sub-handle, no fallback):

| Endpoint | FFI Call | Status |
|----------|---------|--------|
| `POST /axi/lite/read` | `AXILite::Read` | âš ï¸ Wired but has incorrect Full fallback â€” **must fix** |
| `PUT /axi/lite/write` | `AXILite::Write` | âš ï¸ Wired but has incorrect Full fallback â€” **must fix** |
| `POST /axi/lite/reset` | `AXILite::Reset` | âš ï¸ Same issue â€” **must fix** |
| `GET /axi/full/datapath-width` | `AXIFull::GetDatapathWidthByteCount` | âœ… |
| `POST /axi/full/read` | `AXIFull::Read` | âœ… |
| `POST /axi/full/write` | `AXIFull::Write` | âœ… |
| `POST /axi/full/reset` | `AXIFull::Reset` | âœ… |
| `GET /axi/stream/datapath-width` | `AXIStream::GetDatapathWidths` | â­ï¸ Stub (501) |
| `POST /axi/stream/read` | `AXIStream::Read` | â­ï¸ Stub (501) |
| `POST /axi/stream/write` | `AXIStream::Write` | â­ï¸ Stub (501) |
| `POST /axi/stream/reset` | `AXIStream::Reset` | â­ï¸ Stub (501) |

**Remaining work:**
1. Remove the Liteâ†’Full fallback from `axi_lite_read`, `axi_lite_write`, `axi_lite_reset`
2. Wire AXI Stream endpoints (widths, read, write, reset) to FFI
3. Test on a Gen3-USB device (XEM7320 reports `okDATAPORT_LEGACY`, AXI unsupported)

#### Phase 19E: gRPC Classic Wiring + Client Updates

**Status**: `[x]` Completed (2026-08-17)  
**Commits**: `6153a0d`, `982b8cc`

1. âœ… Wired all 18 Classic `ClassicDataPortService` gRPC stubs to FFI in
   `classic_service.rs` (were returning UNIMPLEMENTED)
2. âœ… Added 6 Classic client wrappers to Rust gRPC client (`lib.rs`)
3. âœ… Added 6 Classic client wrappers to Python gRPC client (`client.py`)
4. âœ… Rewrote Rust gRPC hardware tests (16 tests, all pass)
5. âœ… Rewrote Python gRPC hardware tests (18 tests, 16 pass, 2 AXI skip)
6. âœ… Added `reqwest` dev-dependency for REST configure in Rust gRPC tests

#### Phase 19F: gRPC Resource RPCs

**Status**: `[x]` Completed (2026-08-17)

1. âœ… `CreateResource` â†’ delegates to `ResourceRegistry::create()`
2. âœ… `DeleteResource` â†’ delegates to `ResourceRegistry::remove()`
3. âœ… Replaced `UNIMPLEMENTED` stubs with working implementations

### Hardware Test Results (XEM7320-A75T, Counters.bit)

Final results after all Phase 19 sub-phases completed:

| Client | Tests | Passed | Skipped | Failed |
|--------|-------|--------|---------|--------|
| Rust REST | 21 | **21** | 0 | **0** |
| Rust gRPC | 16 | **16** | 0 | **0** |
| Python REST | 23 | **16** | 7 | **0** |
| Python gRPC | 18 | **16** | 2 | **0** |
| **Total** | **78** | **69** | **9** | **0** |

All 9 skips are expected: AXI (Legacy dataport on XEM7320) and sensors (unavailable).

### Verification

```bash
# Server-side
cargo build --features full
cargo test --features full

# Hardware tests (server running + FPGA connected)
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

#### Phase 19G: Device Settings & SYZYGY FFI Wiring

**Status**: `[x]` Completed (2026-08-17)  
**ADR reference**: [ADR-021 â€” Device Settings FFI Wiring](architecture/adr/ADR-021-settings-ffi-wiring.md)

Wired the three settings REST endpoints, three gRPC RPCs, and the SYZYGY
discovery endpoints from 501 stubs to real FFI calls via the `DeviceHandlePool`.
Settings and SYZYGY data are device-level metadata (not bitfile-dependent) and
do not require FPGA programming.

**REST endpoints:**

| Endpoint | FFI Calls | Status |
|----------|----------|--------|
| `GET /devices/{serial}/settings` | `settings_construct` â†’ `fp_get_device_settings` â†’ `settings_list` â†’ iterate keys â†’ `settings_get_int`/`settings_get_string` â†’ `settings_destruct` | `[x]` Done |
| `GET /devices/{serial}/settings/{key}` | Same populate + single-key read | `[x]` Done |
| `PUT /devices/{serial}/settings/{key}` | Same populate + `settings_set_int`/`settings_set_string` + `settings_save` | `[x]` Done |
| `GET /devices/{serial}/syzygy` | Same populate + probe `SYZYGYn_PORT_STATUS` keys (n=0..15), read DNA fields | `[x]` Done |

**gRPC RPCs:**

| RPC | Status |
|-----|--------|
| `GetDeviceSettings` | `[x]` Done |
| `GetDeviceSetting` | `[x]` Done |
| `SetDeviceSetting` | `[x]` Done |
| `GetSyzygyPorts` | `[x]` Done |

**Client updates:**
- `[x]` Move "Settings" out of "Stub Endpoints (expected 501)" in Rust REST client demo
- `[x]` Add "SYZYGY Ports" section showing structured port data
- `[x]` Move Dataport to "Device Endpoints" (was already live, mislabeled as stub)
- `[x]` Add Bitfile info with explanatory note (409 = not programmed, not a stub)
- `[x]` Clarify Sensors section: 501 with code -15 is correct for XEM7320 (no on-board sensors)

---

## Phase 20: Enhanced Resource Registration

**Status**: `[x]` Completed (2026-08-17)  
**Depends on**: Phase 19  
**ADR reference**: [ADR-020 â€” Client-Side Resource Registration](architecture/adr/ADR-020-client-side-resource-registration.md)

### Overview

Enhance the resource registration schema with access mode constraints, Classic
endpoint declarations, AXI address range declarations, interface type validation,
heartbeat/liveness, and lock/unlock semantics. This phase implements all 7 features
from ADR-020.

### Sub-phases

#### Phase 20A: Enhanced Resource Schema

**Status**: `[x]` Completed (2026-08-17, `c999c08`)

1. Extend `Resource` struct in `ok-fpga-core/src/resource.rs` with:
   - `access_mode: AccessMode` (resource-level, not per-acquisition)
   - `interfaces: InterfaceDeclaration` (classic, axi_lite, axi_full, axi_stream)
   - `endpoints: Option<EndpointDeclaration>` (Classic EP list with per-EP access)
   - `address_ranges: Vec<AddressRange>` (32-bit hex ranges with access mode)
   - `metadata: HashMap<String, String>` (extensible key-value pairs)
2. Update `CreateResourceRequest` in proto3 and OpenAPI to match the full schema
3. Update REST `POST /resources` handler to accept the expanded schema
4. Update gRPC `CreateResource` handler to accept the expanded schema

#### Phase 20B: Registration-Time Validation

**Status**: `[x]` Completed (2026-08-17, `c999c08`)

1. **Interface validation (strict)**: If `axi_lite/full/stream` is declared, call
   `GetFPGADataPortAXI()` and the specific sub-handle getter. Reject if unsupported.
2. **EP type validation**: Verify declared EP `type` matches the FrontPanel address
   range (0x00â€“0x1F=WireIn, etc.). Reject mismatches.
3. **EP overlap check**: Reject if a new resource declares an EP as `exclusive` that
   is already declared `exclusive` by another resource on the same device.
4. **Address range overlap check**: Reject if a new resource's exclusive address
   range overlaps an existing exclusive range on the same device.
5. **Address range validation**: `start <= end`, both valid 32-bit hex.
6. Pipe type distinction: Accept both `pipe_in`/`pipe_out` and `bt_pipe_in`/`bt_pipe_out`
   in the same 0x80â€“0xBF range but never the same address for both types.

#### Phase 20C: Access Mode & Lock/Unlock

**Status**: `[x]` Completed (2026-08-17, `c999c08`, `df59119`)

1. Change acquisition logic: `exclusive` resources always transition to `InUseLocked`
   on acquire (no mode choice). `shared` resources transition to `InUseShared`.
2. Add `POST /resources/{id}/lock` â€” holder of a shared resource can lock it.
   Transitions `InUseShared` â†’ `InUseLocked`. Other holders receive event.
3. Add `POST /resources/{id}/unlock` â€” unlocker must be the locker.
   Transitions `InUseLocked` â†’ `InUseShared`. All holders receive event.
4. Add `LockResource` / `UnlockResource` gRPC RPCs.
5. Add `ResourceLocked` / `ResourceUnlocked` event types to SSE and gRPC streams.
6. Reject lock/unlock on `exclusive` resources (always implicitly locked).

#### Phase 20D: Heartbeat / Liveness

**Status**: `[/]` Partially complete (2026-08-17, `df59119`)

> Explicit heartbeat endpoints implemented. Items 1, 2, 4, 6 deferred to Phase 21.

1. Add `last_seen_at: DateTime<Utc>` to `UserSession`.
2. **Implicit heartbeat**: Middleware/interceptor on all REST and gRPC requests that
   updates `last_seen_at` for the identified user.
3. **Explicit heartbeat**: Add `POST /api/v1/heartbeat` REST endpoint.
   Add `ServerService.Ping` gRPC RPC.
4. **Server sweep task**: Background `tokio::spawn` task every 10 seconds:
   - Find users where `now - last_seen_at > 60 seconds`
   - Auto-release all resources held by expired users
   - Send `ResourceStateChanged` events to remaining holders
5. Update Rust and Python clients with heartbeat helpers.
6. Configure timeout (60s) and sweep interval (10s) in server config.

#### Phase 20E: Resource Persistence Rules

**Status**: `[/]` Partially complete (2026-08-17, `c999c08`)

> Resource deletion on device disconnect implemented. Holder notification deferred to Phase 22.

1. **Survive client disconnect**: Resources remain in registry after the
   registering client disconnects. No change needed (already in-memory).
2. **Delete on device disconnect**: When `DeviceRemoved` event fires, delete
   (not just mark Unavailable) all resources for that serial. Notify holders.
3. **Server-side static resources**: Future work (document in ADR-020).

#### Phase 20F: Client Updates & Documentation

**Status**: `[x]` Completed (2026-08-17, client update commit)

1. Update all 4 example clients to use the enhanced `CreateResource` schema.
2. Add resource registration documentation with full schema examples.
3. Document all endpoint types (including `bt_pipe_in`/`bt_pipe_out`).
4. Add heartbeat usage to client examples.
5. Update hardware integration tests to test lock/unlock flows.

#### Phase 20G: Stub Code for Developers

**Status**: `[ ]` Deferred to Phase 22B

1. Add `ResourceAccessPolicy` trait methods for:
   - Address-range-based access validation (stub, not enforced in v1)
   - EP-based access validation (stub, not enforced in v1)
   - Custom metadata key handlers (stub, developer fills in logic)
2. Document the stub code in `ok-fpga-core/src/resource.rs` with examples.

### Verification

```bash
# Server-side
cargo build --features full
cargo test --features full

# Enhanced resource registration
curl -X POST http://localhost:8088/api/v1/resources \
  -H 'Content-Type: application/json' \
  -d '{
    "name": "counters-classic",
    "device_serial": "22520013CC",
    "access_mode": "shared",
    "interfaces": { "classic": true, "axi_lite": false, "axi_full": false, "axi_stream": false },
    "endpoints": {
      "default_access": "exclusive",
      "entries": [
        { "address": "0x00", "type": "wire_in", "access": "exclusive" },
        { "address": "0x20", "type": "wire_out", "access": "shared" }
      ]
    }
  }'
# Expected: 201 Created

# Lock/unlock flow
curl -X POST http://localhost:8088/api/v1/resources/{id}/acquire -d '{"user_id": "alice"}'
curl -X POST http://localhost:8088/api/v1/resources/{id}/lock -d '{"user_id": "alice"}'
curl -X POST http://localhost:8088/api/v1/resources/{id}/unlock -d '{"user_id": "alice"}'
curl -X POST http://localhost:8088/api/v1/resources/{id}/release -d '{"user_id": "alice"}'

# Heartbeat
curl -X POST http://localhost:8088/api/v1/heartbeat -d '{"user_id": "alice"}'
```

---

## Phase 21: User Session Tracking & Liveness

**Status**: `[x]` Completed (2026-08-17)  
**Depends on**: Phase 20  
**ADR reference**: ADR-020 Â§5 (Heartbeat / Liveness)

### Overview

Implement user session tracking with `last_seen_at` timestamps, implicit heartbeat
middleware on all REST/gRPC requests, and a background sweep task that auto-releases
resources held by expired users.

> [!NOTE]
> When `auth.provider = "none"`, the implicit heartbeat middleware will skip
> silently if no `X-User-Id` header is present (backward compatible). However,
> it is **highly recommended** to always send the `X-User-Id` header even
> when authentication is disabled, or diminished functionality (no liveness
> tracking, no auto-release) will occur.

### Sub-phases

#### Phase 21A: User Session Registry

**Status**: `[x]` Completed (2026-08-17, `62ca90c`)

1. Create a `UserSession` struct with `user_id: String`,
   `last_seen_at: DateTime<Utc>`, and `held_resource_ids: Vec<String>`.
   Location: new `user.rs` module in `ok-fpga-core`, or extend `resource.rs`.
2. Create a `UserSessionRegistry` backed by `RwLock<HashMap<String, UserSession>>`
   with methods:
   - `touch(user_id)` â€” create or update `last_seen_at` to `Utc::now()`
   - `get(user_id)` â†’ `Option<UserSession>`
   - `list()` â†’ `Vec<UserSession>`
   - `remove(user_id)`
   - `expired(timeout: Duration)` â†’ `Vec<UserSession>` (users not seen within timeout)
3. Add `pub user_sessions: Arc<UserSessionRegistry>` to `AppState` in `state.rs`.
4. Update `GET /users` handler in `rest/users.rs` to return real session data
   from the registry instead of the current empty `[]` stub.
5. Update the existing `POST /heartbeat` (REST) and `Ping` (gRPC) handlers
   to call `user_sessions.touch(user_id)`.
6. Update `acquire_resource` and `release_resource` handlers (REST + gRPC)
   to also call `touch()` for implicit user tracking.

#### Phase 21B: Implicit Heartbeat Middleware

**Status**: `[x]` Completed (2026-08-17, `463cbd0`)

1. Create an Axum middleware layer wrapping all `/api/v1/*` routes that:
   - Extracts `user_id` from the `X-User-Id` request header (when `auth.provider = "none"`)
   - Extracts `user_id` from the session cookie (when `auth.provider = "local"` or `"ldap"`)
   - Calls `user_sessions.touch(user_id)` if a user identity is available
   - Skips silently if no user identity is present (backward compatible)
2. Create a gRPC interceptor in the `device_service.rs` module that:
   - Extracts `user_id` from the `x-user-id` metadata key
   - Calls `user_sessions.touch(user_id)` if present
3. Document that `X-User-Id` header is **highly recommended** even when
   `auth.provider = "none"`, or liveness tracking will not function.

#### Phase 21C: Server Sweep Task

**Status**: `[x]` Completed (2026-08-17, `28bdd42`)

1. Add two new fields to `CoreConfig` in `config.rs`:
   ```toml
   [server]
   user_timeout_secs = 60         # Mark user expired after 60s of silence
   user_sweep_interval_secs = 10  # Check for expired users every 10s
   ```
2. Implement the sweep function (spawned via `tokio::spawn` in `main.rs`):
   - Every `user_sweep_interval_secs`, call `user_sessions.expired(timeout)`
   - For each expired user:
     a. List all resources held by the user
     b. Call `resource_registry.release()` for each resource
     c. Broadcast a `DeviceEvent::ResourceChanged` for each released resource
        (Option B â€” handlers broadcast via `DeviceManager`)
     d. Remove the user session from the registry
     e. Log: `warn!("auto-released {} resources for expired user '{}'", count, user_id)`
3. Add unit tests for the sweep logic.

### Verification

```bash
cargo test -p ok-fpga-core -- user_session
cargo test --features full -- heartbeat
cargo test --features full -- sweep_expired

# Manual: send heartbeat, wait 60s, verify resources are auto-released
curl -X POST http://localhost:8088/api/v1/heartbeat \
  -H 'Content-Type: application/json' \
  -d '{"user_id": "alice"}'
```

---

## Phase 22: Event System & Policy Stubs

**Status**: `[x]` Completed (2026-08-17)  
**Depends on**: Phase 20 (Phase 21 is NOT a dependency)  
**ADR reference**: ADR-020 Â§3 (Lock/Unlock events), ADR-004 (SSE), ADR-007 (WebSocket)

### Overview

Wire the existing `DeviceEvent` / `ServiceEvent` broadcast channels to the REST
SSE endpoint and management WebSocket. Add resource event broadcasting from
REST/gRPC handlers (Option B â€” handlers broadcast via `DeviceManager`). Add
stub methods to `ResourceAccessPolicy` for future HDL-specific validation.

### Sub-phases

#### Phase 22A: Event Notifications

**Status**: `[x]` Completed (2026-08-17, `10f1fc6`)

1. **SSE endpoint** â€” Replace the `501 Not Implemented` stub in `rest/events.rs`
   with a real SSE implementation using `axum::response::sse::Sse` and
   `tokio_stream`. Subscribe to `device_manager.subscribe()` and yield events.
2. **WebSocket** â€” Wire the management WebSocket handler (`/ws/events`) in
   `management.rs` to forward `DeviceEvent` and `ServiceEvent` broadcasts
   to connected dashboard clients as JSON messages.
3. **Resource event broadcasting (Option B)** â€” After each successful registry
   mutation in REST/gRPC handlers, broadcast via `device_manager.broadcast()`:

   | Handler | Event | `new_state` |
   |---------|-------|-------------|
   | `acquire_resource` | `ResourceChanged` | `"In Use (Shared)"` or `"In Use (Locked)"` |
   | `release_resource` | `ResourceChanged` | `"Available"` or `"In Use (Shared)"` |
   | `lock_resource` | `ResourceChanged` | `"In Use (Locked)"` |
   | `unlock_resource` | `ResourceChanged` | `"In Use (Shared)"` |
   | `invalidate_device` | `ResourceChanged` | `"Deleted"` (one per resource) |

4. Add integration tests verifying events are received via SSE when resources
   are acquired, released, locked, and unlocked.

#### Phase 22B: ResourceAccessPolicy Trait Stubs

**Status**: `[x]` Completed (2026-08-17, `10f1fc6`)  
(Moved from Phase 20G)

1. Add three new **default-implemented** methods to the `ResourceAccessPolicy`
   trait in `ok-fpga-core/src/resource.rs`:
   - `validate_address_access(resource, user, address) â†’ Result<(), AcquisitionDenied>`
     â€” Stub for HDL-specific AXI address range protection. Default: allow all.
   - `validate_endpoint_access(resource, user, endpoint, ep_type) â†’ Result<(), AcquisitionDenied>`
     â€” Stub for per-Classic-endpoint access control. Default: allow all.
   - `on_metadata_key(resource, key, value) â†’ Result<(), String>`
     â€” Stub for custom metadata key processing during `create_resource`. Default: accept all.
2. All three methods have default no-op implementations â€” `DefaultAccessPolicy`
   does not need to change.
3. Add comprehensive doc comments with examples showing how a developer would
   override each method for an HDL-specific access policy.
4. Add unit tests verifying the default (allow-all) behavior.

### Verification

```bash
# SSE event stream
curl -N http://localhost:8088/api/v1/events
# â†’ Connect a device â†’ expect DeviceAdded event
# â†’ Acquire a resource â†’ expect ResourceChanged event

# Trait stub tests
cargo test -p ok-fpga-core -- validate_address
cargo test -p ok-fpga-core -- validate_endpoint
cargo test -p ok-fpga-core -- on_metadata_key
```

---

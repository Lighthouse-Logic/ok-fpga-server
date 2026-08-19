# Roadmap Archive: Phases 23–25

> **Status**: Archived — these phases are complete.
> This file is a historical record. For current priorities, see
> [development-roadmap.md](../development-roadmap.md).

---
## Phase 23: Disconnect Detection Bug Fix

**Status**: `[x]` Completed (2026-08-18, `d8b8341`)  
**Depends on**: Phase 15, Phase 19 (ADR-018 handle pool)  
**ADR reference**: [ADR-002](architecture/adr/ADR-002-device-lifecycle.md) (addendum),
[ADR-018](architecture/adr/ADR-018-device-handle-pool.md)

### Overview

The `device_poll_loop` in `main.rs` has a bug that prevents device disconnect
detection. When a device is physically unplugged, the dashboard continues to
show it as connected.

### Root Cause

The poll loop (lines 369â€“389 of `main.rs`) checks whether a "gone" device is
still in the handle pool before declaring it disconnected. Since every device
is opened in the pool at startup, and the pool only closes handles when a
disconnect is detected, the check **always** returns true â€” creating a circular
dependency that suppresses all disconnect events.

The `FrontPanelDevices` scanner tries to `devices_open()` each device to read
`okTDeviceInfo`, which fails when the device is already exclusively opened by
the pool. The workaround (skip pool-opened devices) was intended to prevent
false disconnects but instead prevents all disconnects.

### Fix

1. **Add `scan_serials(&self) -> HashSet<String>`** to `scanner.rs` â€” a
   lightweight method that calls `FrontPanelDevices::GetCount()` +
   `GetSerial()` to list all physically connected serial numbers **without
   opening any devices**. This works regardless of whether the handle pool
   has the device open.

2. **Rewrite `device_poll_loop`** in `main.rs`:
   - Use `scan_serials()` for presence detection (replaces `scan_devices()`)
   - Remove the broken `in_pool` check entirely
   - For gone devices: call `on_device_removed()` + `pool.close()`
   - For new devices: call `scan_devices()` for that single serial to get
     full info, then `on_device_added()` + `pool.open()`

### Files Changed

| File | Change |
|------|--------|
| `crates/ok-fpga-server/src/scanner.rs` | Add `scan_serials()` method |
| `crates/ok-fpga-server/src/main.rs` | Rewrite `device_poll_loop` |

### Verification

```bash
# Build check
cargo check --features full

# Unit tests
cargo test --features full --lib

# Manual test:
# 1. Start server with FPGA connected â†’ dashboard shows device
# 2. Disconnect FPGA â†’ dashboard should update within 2s
# 3. Reconnect FPGA â†’ dashboard should show device again
```

---

## Phase 24: Dashboard Log Streaming & Level Filtering (ADR-022)

**Status**: `[x]` Completed (2026-08-18)  
**Depends on**: Phase 8 (web dashboard), Phase 23 (disconnect fix â€” INFO-level events)  
**ADR reference**: [ADR-022](architecture/adr/ADR-022-dashboard-logging.md),
[ADR-007](architecture/adr/ADR-007-web-dashboard.md) Â§77-94

### Overview

Implement the log streaming architecture specified in ADR-022. Server-side
`tracing` events are broadcast to the web dashboard via WebSocket, with
per-client log level filtering. Logs are also persisted to a configurable
file on disk.

### Sub-phases

#### Phase 24A: Server-Side Broadcast Layer

**Status**: `[x]` Completed (2026-08-18, `cb55e75`)

1. **Create `log_broadcast.rs`** â€” a custom `tracing_subscriber::Layer` that:
   - Captures all events at TRACE level
   - Serializes each event to a `LogEvent` struct
   - Sends through a `broadcast::Sender<LogEvent>`
   - Maintains a ring buffer (200 entries) for backfill

2. **Update `init_tracing()`** in `main.rs` â€” compose the subscriber stack:
   - `fmt::Layer` (stdout, filtered by config level)
   - File appender layer (new, filtered by config level)
   - `WebSocketBroadcastLayer` (new, captures at TRACE)

3. **Add TOML config fields**:
   - `server.log_file` â€” log file path (default: `logs/okfpga-server.log`)
   - `dashboard.default_log_level` â€” default log level for dashboard (default: `info`)

#### Phase 24B: WebSocket Per-Client Filtering

**Status**: `[x]` Completed (2026-08-18, `cb55e75`)

1. **Update `handle_ws_connection`** in `management.rs`:
   - Accept `{"type": "set_log_level", "level": "DEBUG"}` control messages
   - Maintain per-client log level filter (default from config)
   - Send backfill from ring buffer on new client connection
   - Filter log events before sending to each client

2. **Add log management endpoints** to the management router:
   - `GET /mgmt/logs/download` â€” download log file as `text/plain`
   - `DELETE /mgmt/logs` â€” truncate log file to zero bytes
   - `GET /mgmt/logs/size` â€” return file size in bytes (JSON)

#### Phase 24C: FFI & Request Instrumentation

**Status**: `[x]` Completed (2026-08-18, `1f30cb1`)

1. **Tower middleware** â€” shared between Axum and Tonic:
   - TRACE: log every request (method, path/RPC, status, latency)
   - DEBUG: periodic summary (request count, error count, avg latency)

2. **Handle pool instrumentation** â€” add `tracing::trace!` around FFI calls:
   - Log function name, arguments, and return code for every FFI invocation
   - Log FFI errors at TRACE with error code description

3. **Stub call logging** â€” ensure all stub/unimplemented handlers emit:
   - WARN: short alert ("GetDeviceSensors: FFI not wired â€” returning 501")
   - TRACE: full request context

#### Phase 24D: Dashboard UI

**Status**: `[x]` Completed (2026-08-18, `784d75b`)

1. **Log level selector** â€” button group in Logs page header:
   `[TRACE] [DEBUG] [INFO] [WARN] [ERROR]`
   - Default: configurable via TOML
   - Sends `set_log_level` WebSocket message on click
   - Active button is visually highlighted

2. **Log management controls**:
   - Clear button: shows file size, calls `DELETE /mgmt/logs` with confirmation
   - Download button: calls `GET /mgmt/logs/download`

3. **CSS classes** â€” add `log-debug` (muted gray) and `log-trace` (very muted, monospace)

### Files Changed

| File | Change | Sub-phase |
|------|--------|-----------|
| `crates/ok-fpga-server/src/log_broadcast.rs` | [NEW] Broadcast layer + ring buffer | 24A |
| `crates/ok-fpga-server/src/main.rs` | Layered tracing subscriber | 24A |
| `crates/ok-fpga-server/src/management.rs` | Per-client WS filtering + log endpoints | 24B |
| `crates/ok-fpga-server/src/handle_pool.rs` | FFI call instrumentation | 24C |
| `web/app.js` | Log level selector + clear/download UI | 24D |
| `web/index.html` | Log level buttons + download button | 24D |
| `web/style.css` | `log-debug` + `log-trace` classes | 24D |

### Verification

```bash
# Build check
cargo check --features full

# Unit tests
cargo test --features full --lib

# Manual verification:
# 1. Open dashboard Logs page â†’ should show INFO+ entries
# 2. Select TRACE â†’ should see FFI calls, REST requests
# 3. Select ERROR â†’ should only see errors
# 4. Perform a REST GET â†’ verify request appears at TRACE level
# 5. Disconnect a device â†’ verify INFO-level entry appears
# 6. Click Clear â†’ log file should be truncated
# 7. Click Download â†’ should download log file
```

---

## Phase 25: GUI Example Clients â€” Interactive Counters.bit Demo (ADR-023)

**Status**: `[/]` Implemented â€” unit tests pending (Phase 25-Fix)  
**Commits**: `b72454a` (8 GUI demos + 4 common libraries), `3eda00b` (docs)  
**Depends on**: Phase 16 (example clients), Phase 18 (hardware integration tests),
Phase 20 (enhanced resource registration), Phase 19 (FFI endpoint wiring)  
**ADR reference**: [ADR-023](architecture/adr/ADR-023-gui-example-clients.md),
[ADR-014](architecture/adr/ADR-014-example-clients.md) Â§GUI Demos,
[ADR-016](architecture/adr/ADR-016-hardware-integration-testing.md) Â§GUI Integration Tests

### Overview

Eight interactive GUI clients demonstrating real-time control of the Counters.bit
hardware test via Classic Data Port endpoints. Each client registers a shared
resource, acquires it, and provides interactive counter control with live polling.

Two GUI frameworks per language provide framework comparison:
- **Rust**: `eframe`/`egui` and `iced`
- **Python**: `tkinter` and `dearpygui`

### Sub-phases

#### Phase 25A: Rust REST GUI (egui) â€” "Rusty Rester"

**Status**: `[x]` Completed (`b72454a`)

1. âœ… Created `client-examples/rust-rest-gui-egui/` Cargo project
2. âœ… Implemented connection panel (Connect â†’ Program â†’ Register â†’ Acquire)
3. âœ… Implemented Counter #1 panel (WireIn RESET1/DISABLE1, WireOut 0x20, TriggerOut 0x60)
4. âœ… Implemented Counter #2 panel (TriggerIn UP2/DOWN2/RESET2, AUTOCOUNT2, WireOut 0x21, TriggerOut 0x61)
5. âœ… Implemented bottom bar (Buttons WireOut 0x22, poll interval selector, activity log)
6. âœ… Implemented resource locking (Lock/Unlock, holder display, Override lockout checkbox)
7. âœ… Implemented shutdown prompt (Release resource? Yes/No)

#### Phase 25B: Rust REST GUI (iced) â€” "Icy Rester"

**Status**: `[x]` Completed (`b72454a`, upgraded to iced 0.14 in `1f71ede`)

Same feature set as 25A, using `iced` 0.14 framework (Elm Architecture).

#### Phase 25C: Rust gRPC GUI (egui) â€” "Rusty Garpac"

**Status**: `[x]` Completed (`b72454a`)

Same feature set as 25A, using gRPC API (`tonic`) instead of REST (`reqwest`).
Uses `tokio::sync::Mutex` for async-safe client access in spawned tasks.

#### Phase 25D: Rust gRPC GUI (iced) â€” "Icy Garpac"

**Status**: `[x]` Completed (`b72454a`, upgraded to iced 0.14 in `1f71ede`)

Same feature set as 25B, using gRPC API. Creates fresh gRPC connection per
operation (iced Messages must be `Debug + Clone`, tonic client is not `Clone`).

#### Phase 25E: Python REST GUI (tkinter) â€” "Pythos Rester"

**Status**: `[x]` Completed (`b72454a`)

1. âœ… Created `client-examples/python-rest-gui-tkinter/` Python application
2. âœ… Implemented connection panel, counter panels, resource locking, shutdown prompt
3. âœ… Uses `requests` (sync) for REST API calls, `tkinter` for GUI

#### Phase 25F: Python REST GUI (dearpygui) â€” "Dear Rester"

**Status**: `[x]` Completed (`b72454a`)

Same feature set as 25E, using `dearpygui` framework with background polling thread.

#### Phase 25G: Python gRPC GUI (tkinter) â€” "Pythos Garpac"

**Status**: `[x]` Completed (`b72454a`)

Same feature set as 25E, using gRPC API (`grpcio`).

#### Phase 25H: Python gRPC GUI (dearpygui) â€” "Dear Garpac"

**Status**: `[x]` Completed (`b72454a`)

Same feature set as 25F, using gRPC API.

### Phase 25-Fix: iced 0.14 Upgrade + Comprehensive Unit Tests

**Status**: `[x]` Completed (2026-08-18, `1f71ede`, `a3f05c0`, `527938b`, `a7b7d9f`)  
**Depends on**: Phase 25Aâ€“25H  
**ADR reference**: [ADR-023 Â§8 Testing Strategy](architecture/adr/ADR-023-gui-example-clients.md)

#### Phase 25-Fix-A: Upgrade iced 0.13 â†’ 0.14

1. Update `iced = "0.13"` â†’ `iced = "0.14"` in Icy Rester and Icy Garpac
2. Migrate to iced 0.14 API (compiler-guided: `Widget::update` takes `Event` by
   reference, `Task::perform` relaxed to `FnOnce`, reactive rendering)
3. Add `iced_test = "0.14"` as dev-dependency for headless GUI testing

#### Phase 25-Fix-B: Common Library Unit Tests

Add comprehensive unit tests to all 4 shared API client libraries using mocks:

| Library | Mock Framework | Tests |
|---------|---------------|------|
| `rust-rest-gui-common` | `mockito` + `tokio` | 24 |
| `rust-grpc-gui-common` | â€” (data model tests) | 14 |
| `python_rest_gui_common` | `responses` | 26 |
| `python_grpc_gui_common` | â€” (data model tests) | 15 |

Each Python package gets a `pyproject.toml` with `[project.optional-dependencies.test]`.

#### Phase 25-Fix-C: GUI Application Unit Tests

| Demo | Framework | Test Approach | Tests |
|------|-----------|--------------|------|
| Rusty Rester (egui) | â€” | *(egui lacks headless harness â€” future refactor)* | â€” |
| Icy Rester (iced) | `#[cfg(test)]` | Elm arch: boot/update/state/lockout/log | 25 |
| Rusty Garpac (egui) | â€” | *(egui lacks headless harness â€” future refactor)* | â€” |
| Icy Garpac (iced) | `#[cfg(test)]` | Elm arch: boot/update/state/lockout/log | 23 |
| Pythos Rester (tkinter) | `unittest.mock` | Mocked tkinter vars, state, phases | 17 |
| Dear Rester (dearpygui) | â€” | Headless state/logic + lockout | 13 |
| Pythos Garpac (tkinter) | â€” | Headless state/logic + lockout | 13 |
| Dear Garpac (dearpygui) | â€” | Headless state/logic + lockout | 12 |

**Total: 182 tests across 10 modules (4 common + 6 GUI apps).**

> **Note**: Rust egui demos do not yet have unit tests. egui does not provide a
> headless test harness comparable to iced's `iced_test`. Extracting testable
> logic from the `eframe::App` trait would require a refactor (tracked separately).

#### Phase 25-Fix-D: Device Selection Dropdown + Disconnect Detection

**Status**: `[x]` Completed (2026-08-18)  
**Depends on**: Phase 25-Fix-C  
**ADR reference**: [ADR-023 Â§2 User-Controlled Startup Sequence](architecture/adr/ADR-023-gui-example-clients.md)

All 8 GUI demos were refactored to implement explicit device selection:

1. **Device Dropdown**: Replaced auto-selection (first device) and listbox/button
   patterns with a proper dropdown widget (`ComboBox`/`pick_list`/`Combobox`).
   Default value: "No device selected".
2. **Gated Programming**: The "Program Bitfile" button is disabled until the user
   explicitly selects a device from the dropdown.
3. **5-second Device Refresh**: After connecting, the device list is polled every
   5 seconds to detect hardware changes.
4. **Disconnect Detection**: If the selected device disappears from the server
   while in the Acquired phase, the Counter Control Panel is automatically closed,
   the app resets to Connected phase, and a disconnect warning dialog is shown.

| Demo | Widget | Refresh Mechanism |
|------|--------|-------------------|
| Rusty Rester (egui) | `egui::ComboBox` | `Instant` timer in `update()` |
| Icy Rester (iced) | `iced::pick_list` | `Subscription::batch` 5s tick |
| Rusty Garpac (egui) | `egui::ComboBox` | `Instant` timer in `update()` |
| Icy Garpac (iced) | `iced::pick_list` | `Subscription::batch` 5s tick |
| Pythos Rester (tkinter) | `ttk.Combobox` | `root.after(5000)` |
| Dear Rester (dearpygui) | `dpg.add_combo` | Thread-based timer |
| Pythos Garpac (tkinter) | `ttk.Combobox` | `root.after(5000)` |
| Dear Garpac (dearpygui) | `dpg.add_combo` | Thread-based timer |

**Tests updated**: Iced connect tests changed to assert `selected_serial == None`
after connect (user must select explicitly). All 48 iced tests pass.

#### Phase 25-Fix-E: Client-Server Contract Alignment

**Status**: `[x]` Completed (2026-08-18, `455d5ba`, `c850110`)  
**Depends on**: Phase 25-Fix-D  
**ADR reference**: [ADR-023 Addendum: Phase 25-Fix-E](architecture/adr/ADR-023-gui-example-clients.md)  
**Rule added**: [ai-collaboration.md Â§6 â€” API Specification Authority](../.agents/rules/ai-collaboration.md)

Manual testing revealed 4 bugs across all 8 GUI demos, all caused by
client-server contract mismatches. Split into two commits:

**Commit 1 â€” Bug Fixes (E1, E2, E5, E6):**

| Sub-phase | Description |
|-----------|-------------|
| E1 | Fix REST `list_devices()` â€” unwrap `{"devices": [...]}` wrapper per OpenAPI spec |
| E2 | Fix mock test responses to match real server format |
| E5 | Generate + commit Python gRPC proto stubs; add `getting-started.md` per demo |
| E6 | Replace `unwrap_or_default()` with error propagation in Rust REST client |
| Server | Expand `DeviceSummary` â†’ full `DeviceInfo` per OpenAPI `DeviceInfo` schema |

**Commit 2 â€” Features (E3, E4):**

| Sub-phase | Description |
|-----------|-------------|
| E3 | Wire gRPC `ConfigureFPGA` + `ConfigureFPGAFromMemory` to FFI `handle_pool` |
| E4 | GUI demos support dual-mode bitfile: server-side path OR client-side streaming |

### Files Created

| File | Description | Sub-phase |
|------|-------------|-----------|
| `client-examples/rust-rest-gui-common/` | [NEW] Shared REST CountersClient (reqwest) | 25A |
| `client-examples/rust-rest-gui-egui/` | [NEW] Rust REST + egui GUI | 25A |
| `client-examples/rust-rest-gui-iced/` | [NEW] Rust REST + iced GUI | 25B |
| `client-examples/rust-grpc-gui-common/` | [NEW] Shared gRPC CountersGrpcClient (tonic) | 25C |
| `client-examples/rust-grpc-gui-egui/` | [NEW] Rust gRPC + egui GUI | 25C |
| `client-examples/rust-grpc-gui-iced/` | [NEW] Rust gRPC + iced GUI | 25D |
| `client-examples/python_rest_gui_common/` | [NEW] Shared REST CountersClient (requests) | 25E |
| `client-examples/python-rest-gui-tkinter/` | [NEW] Python REST + tkinter GUI | 25E |
| `client-examples/python-rest-gui-dearpygui/` | [NEW] Python REST + dearpygui GUI | 25F |
| `client-examples/python_grpc_gui_common/` | [NEW] Shared gRPC CountersGrpcClient (grpcio) | 25G |
| `client-examples/python-grpc-gui-tkinter/` | [NEW] Python gRPC + tkinter GUI | 25G |
| `client-examples/python-grpc-gui-dearpygui/` | [NEW] Python gRPC + dearpygui GUI | 25H |

### Verification

#### Automated Tests (Phase 25-Fix)

```bash
# Rust common libraries â€” unit tests with mocks
cd client-examples/rust-rest-gui-common && cargo test
cd client-examples/rust-grpc-gui-common && cargo test

# Rust GUI demos â€” logic tests + iced_test headless (iced demos)
cd client-examples/rust-rest-gui-egui && cargo test
cd client-examples/rust-rest-gui-iced && cargo test  # includes iced_test
cd client-examples/rust-grpc-gui-egui && cargo test
cd client-examples/rust-grpc-gui-iced && cargo test  # includes iced_test

# Python common libraries
cd client-examples && python -m pytest python_rest_gui_common/ -v
cd client-examples && python -m pytest python_grpc_gui_common/ -v

# Python GUI demos
cd client-examples && python -m pytest python-rest-gui-tkinter/ -v
cd client-examples && python -m pytest python-rest-gui-dearpygui/ -v
cd client-examples && python -m pytest python-grpc-gui-tkinter/ -v
cd client-examples && python -m pytest python-grpc-gui-dearpygui/ -v
```

#### Manual Verification

Each GUI demo should be verified with:

1. **No device**: GUI launches, connection panel functional, buttons correctly disabled
2. **With device**: Full startup sequence â†’ counter control â†’ polling â†’ triggers
3. **Device selection**: Connect â†’ verify dropdown shows devices â†’ select one â†’ Program enabled
4. **Device refresh**: Connect â†’ unplug device â†’ verify dropdown updates within 5s
5. **Disconnect detection**: Acquire â†’ unplug device â†’ verify control panel closes,
   disconnect warning shown, app resets to Connected phase
6. **Multi-user**: Two GUI clients sharing the same resource simultaneously
7. **Lock enforcement**: Lock from one client â†’ verify other client is locked out â†’
   Override checkbox â†’ verify server rejects writes â†’ error displayed in GUI
8. **Timeout test**: Exit without releasing â†’ verify server reclaims resource after
   `user_timeout_secs`

#### Phase 25-Fix-F: Iced GUI + Server Bug Fixes

**Status**: `[x]` Completed (2026-08-18, `56db45b`)  
**Depends on**: Phase 25-Fix-E  
**ADR reference**: [ADR-023 Addendum: Phase 25-Fix-F](architecture/adr/ADR-023-gui-example-clients.md)

Manual testing of the Rust REST iced demo against a live server with one device
revealed 7 bugs. Five are fixed in this phase; two are deferred to Fix-G.

| Bug | Severity | Summary |
|-----|----------|--------|
| F1 | CRITICAL | Counter values always 0 â€” server uses path params for wire-out, spec uses query params |
| F2 | CRITICAL | Triggers never fire â€” same path-vs-query mismatch for trigger-out |
| F3 | HIGH | 409 Conflict on re-register â€” server doesn't clean resources on re-program |
| F4 | MEDIUM | No smart skip when device already programmed with matching resource |
| F5 | LOW | No bitfile mode indicator (server-path vs local-file) |

**Resource cleanup rule**: All resources for a device are cleared whenever the FPGA
is re-programmed (any `configure_fpga` method, by any user).

**Future work**:
- In-use protection â€” prevent re-program when resources are held (shared or exclusive)
- User privilege levels â€” admin vs normal-user; only admin can override in-use protection

**Cross-demo note**: Server fixes (F1, F2, F3) benefit all 8 demos. Client library
fixes (F4) benefit all demos using those libraries. GUI-specific fixes (F3 409-handling,
F4 smart-skip, F5 bitfile toggle) are iced-specific and must be ported to other demos.

#### Phase 25-Fix-G: Dashboard Fixes

**Status**: `[x]` Completed (2026-08-18, `a963be4`)  
**Depends on**: Phase 25-Fix-F  
**ADR reference**: [ADR-023 Addendum: Phase 25-Fix-F](architecture/adr/ADR-023-gui-example-clients.md)

Dashboard-focused phase for bugs deferred from Fix-F.

| Bug | Severity | Summary |
|-----|----------|--------|
| F6 | LOW | Resource panel shows empty table â€” no `/mgmt/resources` endpoint |
| F7 | LOW | No users in dashboard â€” no `/mgmt/users` endpoint |

#### Phase 25-Fix-H: QuitRelease Race, Dashboard Auto-Refresh, User Session Cleanup

**Status**: `[x]` Completed (2026-08-18, `85d7b65..6b27019`)  
**Depends on**: Phase 25-Fix-G  
**ADR reference**: [ADR-023 Addendum: Phase 25-Fix-H](architecture/adr/ADR-023-gui-example-clients.md)

Bugs discovered during manual testing of Fix-G dashboard with the iced GUI demo.

| Bug | Severity | Summary |
|-----|----------|---------|
| F8 | HIGH | QuitRelease races â€” resolved by F10 disconnect endpoint |
| F9 | MEDIUM | Dashboard auto-refresh â€” WebSocket events for resource/user lifecycle |
| F10 | LOW | User session cleanup â€” `POST /disconnect` + `Disconnect` RPC |
| F11 | â€” | Endpoint access enforcement â€” documented as ADR-024 candidate |

**Resource cleanup rule** (unchanged): All resources for a device are cleared whenever
the FPGA is re-programmed (any `configure_fpga` method, by any user).

---

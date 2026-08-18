# ADR-023: GUI Example Clients — Interactive Counters.bit Demo

**Status**: Accepted  
**Date**: 2026-08-18  
**Deciders**: Architecture Team  
**Depends on**: [ADR-014](ADR-014-example-clients.md) (Example Client Architecture),
[ADR-016](ADR-016-hardware-integration-testing.md) (Hardware Integration Testing),
[ADR-020](ADR-020-client-side-resource-registration.md) (Client-Side Resource Registration)

## Context

The existing CLI example clients (Phase 16, ADR-014) exercise every REST/gRPC
endpoint sequentially and exit. They prove the protocol layer works but don't
demonstrate **interactive, real-time FPGA control** — the primary use case for
the ok-FPGA-Server.

A second tier of **GUI-based** example clients is needed to:

1. Demonstrate interactive hardware control using the Classic Data Port API
2. Exercise the resource management lifecycle (register → acquire → lock → release)
3. Showcase multi-user shared access and locking behavior
4. Serve as reference implementations for developers building custom FPGA control GUIs
5. Provide hands-on testing of server-side resource locking enforcement

### Hardware Target: Counters.bit

All GUI demos target the `Counters.bit` example bitfile, which implements two
counters with Classic Data Port endpoints. The bitfile path is configured via
`OKFPGA_TEST_BITFILE`. See [Counters.bit Endpoint Reference](../counters-bit-endpoints.md).

## Decision

### 1. Eight GUI Clients (2 frameworks × 2 protocols × 2 languages)

Each language (Rust, Python) gets **two GUI framework implementations** for
each protocol (REST, gRPC), providing framework comparison and broader
reference coverage:

| # | Client | Framework | Protocol | User Name | Directory |
|---|--------|-----------|----------|-----------|-----------|
| 1 | Rust REST (egui) | `eframe`/`egui` | REST | `Rusty Rester` | `client-examples/rust-rest-gui-egui/` |
| 2 | Rust REST (iced) | `iced` | REST | `Icy Rester` | `client-examples/rust-rest-gui-iced/` |
| 3 | Rust gRPC (egui) | `eframe`/`egui` | gRPC | `Rusty Garpac` | `client-examples/rust-grpc-gui-egui/` |
| 4 | Rust gRPC (iced) | `iced` | gRPC | `Icy Garpac` | `client-examples/rust-grpc-gui-iced/` |
| 5 | Python REST (tkinter) | `tkinter` | REST | `Pythos Rester` | `client-examples/python-rest-gui-tkinter/` |
| 6 | Python REST (dearpygui) | `dearpygui` | REST | `Dear Rester` | `client-examples/python-rest-gui-dearpygui/` |
| 7 | Python gRPC (tkinter) | `tkinter` | gRPC | `Pythos Garpac` | `client-examples/python-grpc-gui-tkinter/` |
| 8 | Python gRPC (dearpygui) | `dearpygui` | gRPC | `Dear Garpac` | `client-examples/python-grpc-gui-dearpygui/` |

#### Framework Rationale

**Rust:**
- **`eframe`/`egui`** — Immediate-mode, pure Rust, single binary, no system
  dependencies. Cross-platform. Best for simple tool UIs.
- **`iced`** (0.14) — Elm-architecture, retained-mode, more polished look. Demonstrates
  a different UI paradigm for Rust developers. Version 0.14 provides the `iced_test`
  headless simulator for comprehensive GUI testing in CI.

**Python:**
- **`tkinter`** — Standard library, no `pip install` needed. Works on every
  Python installation. Fastest to get running.
- **`dearpygui`** — Immediate-mode, GPU-accelerated, modern look. MIT license.
  Demonstrates a performance-oriented Python GUI option.

### 2. User-Controlled Startup Sequence

The GUI launches **first** with a connection panel. The user drives each step
via buttons, allowing inspection of server state between operations:

```
┌─────────────────────────────────────────────────────────┐
│  Counters.bit — [Client Name]                           │
├─────────────────────────────────────────────────────────┤
│  Server: [localhost:9090    ]  [🔍 Discover Servers]     │
│  Status: Not connected                                  │
│                                                         │
│  [1. Connect to Server]     (disabled until server set) │
│  Select Device: [No device selected      ▾]             │
│  [2. Program Bitfile]       (disabled until device sel) │
│  [3. Register Resource]     (disabled until programmed) │
│  [4. Acquire Resource]      (disabled until registered) │
│                                                         │
│  → After step 4, the Counter Control Panel appears      │
└─────────────────────────────────────────────────────────┘
```

| Step | UI Element | Action | API Call |
|------|-----------|--------|---------|
| 0 | Discover Servers | mDNS browse for `_okfpga._tcp.local.` | mDNS query |
| 1 | Connect to Server | Verify server reachable, list devices | `GET /status`, `GET /devices` |
| 1b | Select Device | User picks device from dropdown (default: "No device selected") | — (client-side) |
| 2 | Program Bitfile | Configure FPGA with `OKFPGA_TEST_BITFILE` (requires device selected) | `POST /devices/{s}/configure` |
| 3 | Register Resource | Create shared Counter Resource | `POST /resources` |
| 4 | Acquire Resource | Acquire in shared mode | `POST /resources/{id}/acquire` |

> **Device Refresh**: Once connected, the device list is refreshed every **5 seconds**
> via `GET /devices`. The dropdown updates automatically. If the currently selected
> device disappears from the server while in the Acquired phase, the Counter Control
> Panel is automatically closed, the UI resets to Connected phase, and a disconnect
> warning dialog is shown.

After step 4 completes, the GUI transitions to the interactive Counter Control
Panel.

### 3. Counter Control Panel

```
┌──────────────────────────────────────────────────────────────────┐
│  Counters.bit — Rusty Rester                          [Quit]    │
│  Device: XEM7320-A75T (22520013CC) ▪ Shared ▪ Polling: 500ms   │
│  Resource holders: Rusty Rester, Pythos Garpac                  │
├──────────────────────────────┬───────────────────────────────────┤
│  COUNTER #1 (Auto-count)     │  COUNTER #2 (Manual)             │
│                              │                                  │
│  Value: [0x3A] ████████░░    │  Value: [0x05] █░░░░░░░░░        │
│                              │                                  │
│  [☑ RESET1]  [☐ DISABLE1]   │  [▶ Count Up]  [▼ Count Down]    │
│                              │  [⟲ Reset]     [☐ AUTOCOUNT2]    │
│  Triggers:                   │                                  │
│   COUNT1==0x00: ⚫            │  Triggers:                       │
│   COUNT1==0x80: ⚫            │   COUNT2==0xFF: ⚫                │
│                              │                                  │
├──────────────────────────────┴───────────────────────────────────┤
│  Buttons: [1: ⚫] [2: ⚫] [3: ⚫] [4: ⚫]        Poll: [500ms ▾] │
│  [🔒 Lock Resource]  [☐ Override lockout]                        │
│  Log: SetWireIn(0x00, 0x0000, 0xFFFF) → OK              [Clear] │
└──────────────────────────────────────────────────────────────────┘
```

### 4. Endpoint Mapping

#### Writes (on user interaction)

| UI Action | REST | gRPC |
|-----------|------|------|
| Toggle RESET1/DISABLE1/AUTOCOUNT2 | `PUT /devices/{s}/classic/wires/in` `{endpoint: 0, value: X, mask: 0x07}` | `ClassicDataPortService.SetWireIn` |
| Commit wire changes | `POST /devices/{s}/classic/wires/in/update` | `ClassicDataPortService.UpdateWireIns` |
| Count Up / Count Down / Reset counter2 | `POST /devices/{s}/classic/triggers/in` `{endpoint: 0x40, bit: N}` | `ClassicDataPortService.ActivateTriggerIn` |

#### Reads (on poll timer)

| Data | REST | gRPC |
|------|------|------|
| Refresh wire outputs | `POST /devices/{s}/classic/wires/out/update` | `ClassicDataPortService.UpdateWireOuts` |
| Counter #1 value | `GET /devices/{s}/classic/wires/out/0x20` | `ClassicDataPortService.GetWireOutValue` |
| Counter #2 value | `GET /devices/{s}/classic/wires/out/0x21` | `ClassicDataPortService.GetWireOutValue` |
| Button status | `GET /devices/{s}/classic/wires/out/0x22` | `ClassicDataPortService.GetWireOutValue` |
| Counter #1 triggers | `GET /devices/{s}/classic/triggers/out/0x60` | `ClassicDataPortService.GetTriggerOutVector` |
| Counter #2 triggers | `GET /devices/{s}/classic/triggers/out/0x61` | `ClassicDataPortService.GetTriggerOutVector` |

#### Resource Lifecycle

| Phase | REST | gRPC |
|-------|------|------|
| Register | `POST /resources` (name: "Counter Resource", mode: "Shared") | `DeviceService.CreateResource` |
| Acquire | `POST /resources/{id}/acquire` | `DeviceService.AcquireResource` |
| Lock | `POST /resources/{id}/lock` | `DeviceService.LockResource` |
| Unlock | `POST /resources/{id}/unlock` | `DeviceService.UnlockResource` |
| Release | `POST /resources/{id}/release` | `DeviceService.ReleaseResource` |
| Query holders | `GET /resources/{id}` | `DeviceService.GetResource` |

### 5. Resource Locking & Shared Access

#### Display

The GUI displays all current holders of the shared resource (queried via
`GET /resources/{id}`). This updates on each poll cycle alongside the
counter values.

#### Lock Behavior

- A **Lock Resource** button allows the user to request exclusive control.
- When the resource is locked **by another user**, the GUI:
  1. Disables all control buttons (RESET, DISABLE, Count Up/Down, triggers)
  2. Shows a banner: "Resource locked by [other_user_name]"
  3. Continues polling and displaying counter values (read-only)
- An **Override lockout** checkbox re-enables the controls even when locked
  by another user. This allows testing of the **server-side lock enforcement**:
  the server should reject write operations from a non-lock-holder regardless
  of what the client sends, and the GUI displays the server's error response.

### 6. Shutdown Behavior

On window close, the GUI presents a prompt:

```
╔═══════════════════════════════════════════════╗
║  Release resource before exit?                ║
║                                               ║
║  [Yes — Release]    [No — Leave Held]         ║
╚═══════════════════════════════════════════════╝
```

- **Yes**: Calls `POST /resources/{id}/release`, then exits cleanly.
- **No**: Exits without releasing. This enables testing of the server's
  user timeout and sweep mechanism (ADR-020 §user_timeout_secs) — the
  server should automatically reclaim the resource after the configured
  inactivity period.

The FPGA is **never deprogrammed** on exit — the bitfile remains loaded
for the next client.

### 7. Polling Configuration

Output values (WireOut, TriggerOut, resource holder list) are polled on a
client-side timer. The poll interval is selectable in the GUI:

| Interval | Use Case |
|----------|----------|
| 100ms | Fast counter observation (Counter #1 auto-count) |
| 250ms | Interactive use |
| 500ms | Default — balanced |
| 1000ms | Low traffic |
| 2000ms | Minimal polling |

The interval applies to all reads in a single batch per cycle:
`UpdateWireOuts → GetWireOut × 3 → GetTriggerOut × 2 → GetResource`.

### 8. Testing Strategy

All code follows TDD per [§10 of ai-collaboration.md](../../.agents/rules/ai-collaboration.md).
Tests are organized in three tiers:

#### Tier 1: Common Library Unit Tests (Always Run, No Server/Hardware)

The 4 shared API client libraries contain all protocol logic and are the
highest-value test targets. Each library gets comprehensive unit tests:

| Library | Test File | Mock Framework | Tests | Focus Areas |
|---------|-----------|---------------|-------|-------------|
| `rust-rest-gui-common` | `src/lib.rs` `#[cfg(test)]` | `mockito` + `tokio` | 24 | HTTP request construction, URL normalization, bitmask computation, response parsing, error handling |
| `rust-grpc-gui-common` | `src/lib.rs` `#[cfg(test)]` | — (data model + proto tests) | 14 | Bitmask computation, proto field mapping, `resource_from_response` helper |
| `python_rest_gui_common` | `test_counters_client.py` | `responses` | 26 | URL construction, bitmask computation, HTTP mock request/response, error handling |
| `python_grpc_gui_common` | `test_counters_grpc_client.py` | — (data model tests) | 15 | Data type construction, bitmask logic, field mapping |

Key test categories for common libraries:
- **WireInState bitmask**: `default() == 0`, individual bits (0x01, 0x02, 0x04), all bits (`0x07`)
- **CounterState defaults**: All fields zero/false after construction
- **DeviceInfo / ResourceInfo fields**: Dataclass/struct construction and field access
- **URL normalization**: Trailing slash handling, base path construction
- **Lock detection**: `locked_by` field presence/absence/self-detection
- **HTTP/gRPC request construction**: Correct method, path, body/fields (via mocks)
- **Response parsing**: Mock server responses → typed data structures
- **Error handling**: Server error codes → `ClientError` with correct status

#### Tier 2: GUI Application Tests (Always Run, Headless)

Each GUI demo application has tests for its state machine logic and, where
framework support permits, headless GUI interaction testing.

**Rust iced demos** (Icy Rester, Icy Garpac) test the Elm Architecture
state machine in `#[cfg(test)]` blocks: `boot()` state defaults, `update()`
message handling (sync and result messages), phase transitions, lockout
logic, log management, and serial helper. 25 + 23 = 48 tests.

**Rust egui demos** (Rusty Rester, Rusty Garpac) do not currently have unit
tests. egui does not provide a headless test harness comparable to `iced_test`.
The egui app state is tightly coupled to `eframe::App` and would need
refactoring to extract testable logic. This is tracked for a future phase.

**Python tkinter demos** test state machine logic using mocked `tkinter`
variables. 17 + 13 = 30 tests.

**Python dearpygui demos** test logic layer headlessly (no DearPyGui import
required). 13 + 12 = 25 tests.

| Test Category | What It Verifies | Count |
|---------------|-----------------|-------|
| Phase ordering | `Disconnected → Connected → Programmed → Registered → Acquired` | 10 |
| Lock detection | `is_locked_out()` with no resource, self-locked, other-locked, override | 30 |
| Log management | Max 200 entries, truncation, error vs. normal | 12 |
| Wire state | `WireInState` bitmask: individual bits, combinations, all-set | 30 |
| Message handling *(iced only)* | `update()` with sync and result messages | 36 |
| State defaults | Initial values after `boot()` / construction | 20 |
| URL / client setup | Base URL normalization, user ID header, session setup | 10 |
| HTTP/gRPC mocking *(common libs)* | Mock server responses → typed data structures | 34 |

#### Tier 3: Hardware Integration Tests (Run When FPGA Connected)

These are the existing Phase 18 hardware tests, extended to cover the GUI
client common libraries. They require a running server + connected FPGA and
are marked `#[ignore]` (Rust) or skipped (Python) by default.

#### Test Dependencies

**Rust dev-dependencies:**
```toml
[dev-dependencies]
mockito = "1"        # HTTP mock server (REST common)
iced_test = "0.14"   # Headless GUI simulator (iced demos)
tokio-test = "0.4"   # Async test utilities
```

**Python test dependencies** (in `pyproject.toml`):
```toml
[project.optional-dependencies]
test = [
    "pytest>=8.0",
    "pytest-asyncio>=0.24",
    "responses>=0.25",    # HTTP mock (REST common)
]
```

### 9. Framework Versions

| Framework | Version | MSRV | Key Testing Feature |
|-----------|---------|------|-------------------|
| `eframe`/`egui` | latest | — | No headless test harness; test extracted logic |
| `iced` | **0.14** | Rust 1.88 | `iced_test` headless simulator with `click()`, `find()`, `tap_key()` |
| `tkinter` | stdlib | Python 3.12+ | `unittest.mock` patching |
| `dearpygui` | latest | Python 3.12+ | `unittest.mock` patching (GPU context required for real rendering) |

> **Note on iced version**: iced 0.14 was chosen over 0.13 for its new `iced_test`
> testing facilities (headless simulator), reactive rendering, animation APIs,
> and new built-in widgets (`table`, `grid`, `sensor`). The MSRV of Rust 1.88 is
> satisfied by the project's toolchain (Rust 1.97).

## Consequences

### Positive

- **8 reference implementations** covering 2 languages × 2 protocols × 2 frameworks
- **Interactive hardware validation** — real-time counter control and observation
- **Resource management showcase** — register, acquire, lock, unlock, release,
  timeout all exercised interactively
- **Framework comparison** — developers can compare egui vs iced, tkinter vs dearpygui
- **Server enforcement testing** — override checkbox enables validation of
  server-side lock rejection
- **Comprehensive test coverage** — common libraries tested with HTTP/gRPC mocks,
  iced demos tested with headless GUI simulator, all following TDD (§10)
- **Educational value** — tests demonstrate proper GUI testing patterns for
  each framework, teaching users best practices

### Negative

- **8 clients is significant maintenance** — however, the API client logic is
  identical within each language/protocol pair; only the UI layer differs
- **Requires real hardware** — Counters.bit must be loaded on a physical FPGA
- **Framework dependencies** — egui, iced, dearpygui are external deps (tkinter
  is stdlib)
- **Test dependency overhead** — `mockito`, `iced_test`, `responses` add
  dev-dependency weight, but only affect test builds

## References

- [ADR-014 — Example Client Architecture](ADR-014-example-clients.md)
- [ADR-016 — Hardware Integration Testing](ADR-016-hardware-integration-testing.md)
- [ADR-020 — Client-Side Resource Registration](ADR-020-client-side-resource-registration.md)
- [Counters.bit Endpoint Reference](../counters-bit-endpoints.md)
- [ai-collaboration.md §10 — Testing Strategy](../../.agents/rules/ai-collaboration.md)
- [ai-collaboration.md §6 — API Specification Authority](../../.agents/rules/ai-collaboration.md)
- [iced_test 0.14 documentation](https://docs.rs/iced_test/0.14.0)

---

## Addendum: Phase 25-Fix-E — Client-Server Contract Alignment

**Date**: 2026-08-18  
**Status**: Completed

### Problem

Manual testing of all 8 GUI demos revealed four categories of failures:

| Bug | Symptom | Root Cause | Demos Affected |
|-----|---------|------------|---------------|
| REST response wrapping | 0 devices shown (Rust) or `'str' object has no attribute 'get'` (Python) | `list_devices()` expects flat array `[{...}]`; server returns `{"devices": [{...}]}` per OpenAPI spec | All 4 REST demos |
| gRPC ConfigureFPGA stub | `Unimplemented: ConfigureFPGA requires FFI integration` | gRPC RPC not wired to FFI `handle_pool`; REST endpoint IS wired | 2 Rust gRPC demos |
| Python proto stubs missing | `'NoneType' object has no attribute 'DeviceServiceStub'` | `gen/` directory not generated; `try/except` silently sets modules to `None` | 2 Python gRPC demos |
| Egui dropdown hidden | Select Device dropdown never appears | Consequence of bug #1: `self.devices` always empty | 1 Rust REST egui demo |

All four bugs passed unit tests because the **mock response fixtures did not
match the real server's response format** — a contract-mismatch class of bug.

### Decision

1. **OpenAPI and proto specs are the source of truth** (new rule: `ai-collaboration.md` §6).
   Server implementations and client parsers must conform to the specs.
   When a discrepancy is found, the spec is correct; the code must be fixed.

2. **Server `DeviceSummary` expanded** to return all fields defined in the
   OpenAPI `DeviceInfo` schema (Option B). The list endpoint must return what
   the spec says.

3. **Silent error swallowing removed**: Rust client's `unwrap_or_default()` on
   deserialization replaced with error propagation via `map_err()`.

4. **Dual-mode bitfile configuration**: GUI demos support both:
   - **Server-side path** (`ConfigureFPGA` / JSON `{"bitfile_path": "..."}`):
     for bitfiles hosted on the server machine.
   - **Client-side streaming** (`ConfigureFPGAFromMemory` / `multipart/form-data`):
     for bitfiles on the GUI client's machine, streamed to the server.

5. **Python gRPC proto stubs committed** to the repository. Each demo directory
   gets a `getting-started.md` documenting prerequisites and setup.

6. **Mock fixtures aligned to specs**: All mock-based tests updated to return
   response bodies that exactly match the OpenAPI/proto spec format.

### Files Changed

| Sub-phase | Files | Description |
|-----------|-------|-------------|
| E1 | `rust-rest-gui-common/src/lib.rs`, `python_rest_gui_common/counters_client.py` | Fix `list_devices()` to unwrap `{"devices": [...]}` |
| E2 | `rust-rest-gui-common/src/lib.rs` (tests), `python_rest_gui_common/test_counters_client.py` | Fix mock response fixtures |
| E3 | `crates/ok-fpga-server/src/grpc/device_service.rs` | Wire gRPC `ConfigureFPGA` + `ConfigureFPGAFromMemory` to FFI |
| E4 | All 8 GUI demo files, all 4 common client libraries | Add dual-mode (path vs streaming) bitfile config |
| E5 | `python_grpc_gui_common/gen/`, 8× `getting-started.md` | Commit proto stubs, add per-demo docs |
| E6 | `rust-rest-gui-common/src/lib.rs` | Replace `unwrap_or_default()` with error propagation |
| Server | `crates/ok-fpga-server/src/rest/devices.rs` | Expand `DeviceSummary` to full `DeviceInfo` per OpenAPI spec |
| Rule | `.agents/rules/ai-collaboration.md` | New §6: API Specification Authority |

---

## Addendum: Phase 25-Fix-F — Iced GUI Demo + Server Bug Fixes

**Date**: 2026-08-18  
**Status**: Completed (2026-08-18, `56db45b`)

### Problem

Manual testing of the Rust REST iced demo against a live server with one connected
device revealed 7 bugs spanning the server, client libraries, and the GUI demo.

### Bugs

| ID | Severity | Summary | Root Cause |
|----|----------|---------|------------|
| F1 | CRITICAL | Counter values always read as 0 | Server uses path params `/{address}` for wire-out; OpenAPI spec uses query params `?endpoint=N`. Client follows spec, server doesn't. |
| F2 | CRITICAL | Triggers never fire | Same path-vs-query mismatch for trigger-out endpoints |
| F3 | HIGH | 409 Conflict on re-register | Server doesn't clean resources on re-program; GUI doesn't handle 409 gracefully |
| F4 | MEDIUM | No smart skip for already-programmed devices | GUI requires manual step-through even when device is already ready |
| F5 | LOW | No bitfile mode indicator | GUI doesn't expose server-path vs local-file toggle |
| F6 | LOW | Dashboard resource panel empty | No `/mgmt/resources` endpoint (deferred → Fix-G) |
| F7 | LOW | No users in dashboard | No `/mgmt/users` endpoint (deferred → Fix-G) |

### Decisions

1. **F1/F2 — Server must follow the OpenAPI spec** (per §6 API Specification Authority).
   The `GET /classic/wires/out` and `GET /classic/triggers/out` handlers must accept
   `?endpoint=N` query parameters, not `/{address}` path segments.

2. **F3 — Resource cleanup on re-program**: When any `configure_fpga` method is called
   (REST or gRPC, by any user), the server clears **all** resources for that device.
   The GUI handles 409 by looking up the existing resource and proceeding.

3. **F6/F7 — Deferred to Phase 25-Fix-G** (dashboard-focused phase). The management
   API needs `/mgmt/resources` and `/mgmt/users` endpoints.

4. **Cross-demo applicability**: Server fixes (F1, F2, F3) benefit all 8 demos.
   Client library fixes (F4) benefit all demos using those libraries. GUI-specific
   fixes (F3 409-handling, F4 smart-skip, F5 bitfile toggle) are iced-specific and
   must be ported to the other 7 demos later.

### Future Work

- **In-use protection**: Prevent FPGA re-programming when any resource on that device
  is currently held (shared or exclusive). Return 409 listing active holders.
- **User privilege levels**: Introduce admin vs normal-user roles. Only admin users
  can override in-use protection. Full privilege scheme TBD.


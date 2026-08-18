# ADR-013: Device Scanner — FFI-to-DeviceManager Bridge

**Status**: Superseded by [ADR-018](ADR-018-device-handle-pool.md) ✅  
**Date**: 2026-08-16  
**Deciders**: Architecture Team  

## Context

Phases 1–13 built the full server stack in layers:
- **ok-fpga-ffi**: DLL loading, function pointer dispatch, raw C API access
- **ok-fpga-core**: `DeviceManager` with `DeviceRegistry`, event broadcasting, `DeviceInfo` domain types
- **ok-fpga-server**: REST/gRPC handlers, management API, web dashboard

However, the layers were never **connected at runtime**. The server creates an
empty `DeviceManager` and never populates it with real devices. The hardware
tests work because they call the FFI layer directly, bypassing the server.

The result: the web dashboard always shows `connected_devices: 0`, even when
FPGAs are physically connected and detectable by the FFI layer.

## Decision

### Phase 1: Startup Device Scan (this ADR)

Add a `scanner` module to `ok-fpga-server` that:

1. Loads the FrontPanel DLL via `OKFPGA_DLL_PATH` environment variable
   (defaults to `FrontPanel-6.1.0/okFrontPanel.dll` relative to CWD)
2. Calls `FrontPanelDevices` (§15 in fp6-api-surface.md) to enumerate all
   connected devices — a point-in-time snapshot
3. For each device: opens it, reads `okTDeviceInfo`, converts to `DeviceInfo`,
   applies `is_supported_device()` filtering (skip USB 2.0)
4. Calls `DeviceManager::on_device_added()` for each supported device
5. Closes all device handles after reading info

**Graceful degradation**: If the DLL is not found or fails to load, the server
starts normally with an empty device list and a warning log. This enables
development/testing without hardware.

### Phase 2: FrontPanelManager Callbacks (future, see ADR-002)

ADR-002 specifies `FrontPanelManager` callbacks as the long-term solution for
real-time hot-plug. This will be implemented as a follow-up, wiring:
- `extern "C"` callback → `tokio::sync::mpsc` channel → `DeviceManager` events
- `okREALM_TEST` support for emulated device connections in CI

### DLL Path Resolution

The DLL path follows this priority:
1. `OKFPGA_DLL_PATH` environment variable (already used by hardware tests)
2. Platform-specific default:
   - Windows: `FrontPanel-6.1.0/okFrontPanel.dll` (relative to CWD)
   - Linux: `FrontPanel-6.1.0/libokFrontPanel.so` (relative to CWD)

No TOML config field is added — the env var is sufficient and consistent with
the existing hardware test convention.

### Management API Extension

Add `GET /mgmt/devices` to the management router, returning full device details
from the `DeviceRegistry`. This allows the dashboard (served on port 9090) to
display device info without cross-origin calls to the REST API on port 8080.

## Files Changed

| File | Change |
|------|--------|
| `crates/ok-fpga-server/src/scanner.rs` | [NEW] Device scanner module |
| `crates/ok-fpga-server/src/main.rs` | Wire scanner into startup sequence |
| `crates/ok-fpga-server/src/management.rs` | Add `GET /mgmt/devices` endpoint |
| `web/app.js` | Update `fetchDevices()` to render real device data |
| `crates/ok-fpga-server/tests/management_api.rs` | [NEW] Management API HTTP tests |

## Consequences

### Positive
- **Dashboard shows real devices** — the primary user-facing gap is closed
- **Graceful degradation** — server works without DLL or hardware
- **No config changes** — uses existing `OKFPGA_DLL_PATH` convention
- **Testable without hardware** — management API tests use an empty `DeviceManager`

### Negative
- **No hot-plug** — devices connected after startup are not detected until restart
- **Startup cost** — device enumeration adds ~100ms to startup (opens each device briefly)

### Risks
- **DLL path portability** — relative paths behave differently on Windows vs Linux; mitigated by env var override

## Open Items / Future Work

- [x] ADR-002 Phase 2: Wire `FrontPanelManager` callbacks for real-time hot-plug
      — **Done**: Implemented as polling in `device_poll_loop` (see ADR-002 addendum,
      2026-08-17). Callback approach retained as dead code for future Linux support.
- [x] ADR-002: `okREALM_TEST` integration for Tier 1 emulated tests
      — **Done**: Wired in `device_monitor.rs` (retained, `#[allow(dead_code)]`).
- [x] Dashboard: periodic refresh or WebSocket push for device list updates
      — **Done**: WebSocket `device_connected`/`device_disconnected` events broadcast
      from `device_poll_loop` (ADR-002 addendum, ADR-018 §5).

## Addendum (2026-08-18): Superseded by ADR-018

This ADR described the **Phase 1** startup-only device scan using
`FrontPanelDevices` snapshot enumeration. [ADR-018](ADR-018-device-handle-pool.md)
(Device Handle Pool) fully supersedes it:

- The `DeviceScanner` module (`scanner.rs`) is still used, but the pool now
  owns the `Arc<FrontPanelDll>` and keeps device handles open persistently
  instead of the open-read-close pattern described here.
- The `device_poll_loop` in `main.rs` uses the scanner for periodic polling
  but the lifecycle is managed by the handle pool, not by this ADR's
  one-shot scan model.
- The `GET /mgmt/devices` endpoint from this ADR remains unchanged.

## References

- [ADR-002](ADR-002-device-lifecycle.md) — Device Lifecycle & Hot-Plug
- [ADR-001](ADR-001-ffi-strategy.md) — FFI Strategy
- [ADR-009](ADR-009-board-syzygy.md) — Supported Boards & SYZYGY Discovery
- [FP6 API Surface §15](../fp6-api-surface.md) — FrontPanelDevices Snapshot Enumeration

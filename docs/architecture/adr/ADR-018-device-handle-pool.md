# ADR-018: Device Handle Pool

**Status**: Implemented  
**Date**: 2026-08-17  
**Deciders**: Architecture Team  
**Depends on**: [ADR-001](ADR-001-ffi-strategy.md) (FFI layer),
[ADR-002](ADR-002-device-lifecycle.md) (Device lifecycle)
**Supersedes**: [ADR-013](ADR-013-device-scanner.md) (Device Scanner — startup-only scan)

## Context

The Device Scanner (ADR-013) loads the FrontPanel DLL, opens device handles
briefly during the startup scan, reads `okTDeviceInfo`, and **closes all
handles** before returning. The scanned metadata is stored in the
`DeviceManager` registry (`DeviceInfo` structs), but no live DLL reference
or open device handles persist into the server's request-handling phase.

REST and gRPC handlers need to perform FFI operations (configure FPGA, read
wires, write registers, etc.) on demand. This requires:

1. **Persistent access** to the loaded `FrontPanelDll`
2. **Open device handles** (`okFrontPanel_HANDLE`) for each connected FPGA
3. **Thread-safe serialization** of FFI calls per device

## Decision

### 1. `DeviceHandlePool` — Location and Structure

**Location**: `crates/ok-fpga-server/src/handle_pool.rs`

The pool is server infrastructure (it depends on `FrontPanelDll` from
`ok-fpga-ffi` and `tokio::sync::Mutex`), not domain logic. The core domain
layer (`ok-fpga-core`) remains agnostic to FFI implementation details.

```rust
/// Pool of open FrontPanel device handles, keyed by serial number.
///
/// This is the bridge between the API layer and the FFI layer. It holds
/// one open device handle per connected FPGA, protected by a per-device
/// mutex to serialize FFI calls.
///
/// ## Why a Pool?
///
/// The FrontPanel C API requires an `okFrontPanel_HANDLE` for every
/// operation. Opening/closing handles per-request would be expensive
/// (USB enumeration + open takes ~50ms) and would prevent maintaining
/// state (e.g., wire values, programmed bitfile). The pool keeps handles
/// open for the lifetime of the device connection.
pub struct DeviceHandlePool {
    /// The loaded FrontPanel shared library.
    /// Shared across all handles (function pointers are immutable).
    dll: Arc<FrontPanelDll>,

    /// Open device handles, keyed by serial number.
    /// Each device gets exactly ONE handle, wrapped in a tokio Mutex
    /// to serialize FFI calls.
    devices: RwLock<HashMap<String, Arc<Mutex<OpenDevice>>>>,
}
```

### 2. `OpenDevice` — Per-Device State

Each open device holds its handle plus optional data port handles obtained
after FPGA configuration:

```rust
/// State for a single open FrontPanel device.
///
/// Contains the main device handle and (after FPGA programming) cached
/// data port handles for Classic and AXI operations.
pub struct OpenDevice {
    /// The main FrontPanel device handle (RAII wrapper).
    /// All device-level FFI calls go through this handle.
    handle: FrontPanelHandle,

    /// Classic data port handle, obtained after ConfigureFPGA.
    /// None until the FPGA is programmed with a bitfile that
    /// includes Classic endpoints.
    classic_port: Option<ClassicDataPortHandle>,

    /// AXI data port handle, obtained after ConfigureFPGA.
    /// None until the FPGA is programmed with a bitfile that
    /// includes an okRegisterBridge.
    axi_port: Option<AXIDataPortHandle>,

    /// Whether the FPGA is currently programmed.
    is_programmed: bool,
}
```

### 3. Pool Lifecycle Operations

#### `open(serial: &str)` — Open a device handle

Called when a device is discovered (initial scan or hot-plug).

1. Construct a new `okFrontPanel_HANDLE` via `okFrontPanel_Construct()`
2. Open by serial: `okFrontPanel_OpenBySerial(handle, serial)`
3. If open fails, log warning and return `Err` (device may have disconnected)
4. Wrap in `OpenDevice { handle, classic_port: None, axi_port: None }`
5. Insert into `devices` map under `Arc<Mutex<...>>`

#### `close(serial: &str)` — Close and remove a device handle

Called when a device is removed (hot-unplug detected by polling).

1. Remove entry from `devices` map
2. Drop the `Arc<Mutex<OpenDevice>>` — this triggers `FrontPanelHandle::drop()`
   which calls `okFrontPanel_Destruct()`
3. Any pending `lock()` callers will get an error on their next FFI call

#### `get(serial: &str) -> Option<Arc<Mutex<OpenDevice>>>`

Called by request handlers to obtain a device for FFI operations.

1. Read-lock the `devices` map
2. Return `Arc::clone()` of the entry (if present)
3. Caller then `lock().await` the mutex before making FFI calls

#### `configure(serial: &str, data: &[u8]) -> Result<(), FfiError>`

Called by the `POST /configure` endpoint.

1. Get the device mutex and lock it
2. Call `okFrontPanel_ConfigureFPGAFromMemory(handle, data.as_ptr(), data.len())`
3. If successful:
   - Drop any existing data port handles
   - Obtain `ClassicDataPort` via `okFrontPanel_GetFPGADataPortClassic()`
   - Obtain `AXIDataPort` via `okFrontPanel_GetFPGADataPortAXI()`
   - Cache both in `OpenDevice`
   - Set `is_programmed = true`
4. If failed: return the `ok_ErrorCode` as `FfiError`

#### `configure_from_path(serial: &str, path: &Path) -> Result<(), FfiError>`

Convenience method for local-path configuration:

1. Call `okFrontPanel_ConfigureFPGA(handle, c_path.as_ptr())`
2. Same post-configure data port acquisition as above

### 4. Integration with `AppState`

```rust
pub struct AppState {
    pub device_manager: Arc<DeviceManager>,
    pub resource_registry: Arc<ResourceRegistry>,
    pub service_manager: Arc<ServiceManager>,
    pub handle_pool: Arc<DeviceHandlePool>,    // ← NEW
    pub auth_provider: Arc<dyn AuthProvider>,
    pub session_manager: Arc<SessionManager>,
    pub start_time: Instant,
    pub api_version: &'static str,
}
```

### 5. Integration with `main.rs` Startup

```
1. DeviceScanner::try_new()             → loads DLL
2. scanner.scan_devices()               → enumerates devices
3. DeviceHandlePool::new(scanner.dll()) → creates pool with shared DLL
4. For each scanned device:
   a. device_manager.on_device_added()  → updates registry
   b. handle_pool.open(serial)          → opens persistent handle
5. AppState::new(..., handle_pool)      → passes pool to state
6. device_poll_loop:
   - New device → device_manager.on_device_added() + handle_pool.open()
   - Gone device → device_manager.on_device_removed() + handle_pool.close()
```

### 6. Concurrency Model

```
Device A ──── Mutex A ──── FFI calls for device A (serialized)
Device B ──── Mutex B ──── FFI calls for device B (serialized)
                            ↑ different devices can run in parallel
```

**Why `tokio::sync::Mutex` instead of `std::sync::Mutex`?**

- FFI calls to USB devices can block for 10–100ms (especially pipe and block
  pipe transfers). Using `std::sync::Mutex` would hold a tokio worker thread
  hostage during this time, potentially starving other tasks.
- `tokio::sync::Mutex::lock()` is `.await`-able — it yields the current task
  back to the runtime while waiting for the lock, allowing other tasks to
  make progress on the same worker thread.
- The trade-off: `tokio::sync::Mutex` has higher overhead for uncontended
  locks (involves runtime scheduling). This is acceptable because FFI calls
  are already I/O-bound.

### 7. Handle Invalidation Scenarios

| Event | Pool Action | Data Port Action |
|-------|-------------|-----------------|
| Device discovered | `open(serial)` | None (not programmed yet) |
| FPGA programmed | (already open) | Obtain Classic + AXI ports |
| FPGA reprogrammed | (already open) | Drop old ports, obtain new ones |
| Device disconnected | `close(serial)` | Dropped with device |
| Device reconnected | `open(serial)` | None (needs reprogram) |
| Server shutdown | Drop entire pool | All handles destructed |

## Consequences

### Positive

- **Persistent handles** — no per-request open/close overhead
- **Safe concurrent access** — per-device mutex prevents data races
- **Clean lifecycle** — RAII ensures handles are always properly destructed
- **Async-friendly** — `tokio::sync::Mutex` doesn't block the runtime
- **Separation of concerns** — pool is server infrastructure, core layer
  stays clean

### Negative

- **Memory footprint** — one open handle per device (small, ~few KB each)
- **Handle leak risk** — if `close()` is not called on hot-unplug, the handle
  leaks until server restart. Mitigated by the polling loop always calling
  `close()` for removed devices.
- **Single point of contention** — if many clients send requests to the same
  device simultaneously, they serialize on the device mutex. This is inherent
  to the FrontPanel API's thread safety model and cannot be avoided.

### Future Work

- **Request queuing** — priority queue for FFI calls per device (e.g.,
  configure has higher priority than wire reads)
- **Timeout on lock acquisition** — return 503 if a handler waits too long
  for the device mutex

### Addendum (2026-08-18): Pool Liveness API

The "Handle health checks" future work item has been implemented as part of
the Phase 23 disconnect detection bug fix.

#### `is_alive(serial) → bool`

Calls `okFrontPanel_IsOpen(handle)` on the pool's existing open handle.
Returns `true` if the device is still physically connected, `false` if the
USB device has been unplugged (the handle becomes invalid).

This is the primary mechanism for **disconnect detection** in the
`device_poll_loop` (see [ADR-002 addendum](ADR-002-device-lifecycle.md)).
It replaced the `scan_serials()` approach (rev1) which created a new
`okFrontPanelDevices` snapshot that interfered with open handles on Windows.

#### `list_serials() → Vec<String>`

Returns the serial numbers of all devices currently in the pool. Used by the
poll loop to iterate over known devices for liveness checks.

## References

- [ADR-001 — FFI Strategy](ADR-001-ffi-strategy.md)
- [ADR-002 — Device Lifecycle & Hot-Plug](ADR-002-device-lifecycle.md)
- [ADR-013 — Device Scanner](ADR-013-device-scanner.md)
- [ADR-017 — FFI Endpoint Wiring Strategy](ADR-017-ffi-endpoint-wiring.md)
- [FP6 API Surface Catalog — §3 Device Core](../fp6-api-surface.md)

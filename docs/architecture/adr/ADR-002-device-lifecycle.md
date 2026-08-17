# ADR-002: Device Lifecycle & Hot-Plug Management

**Status**: Accepted  
**Date**: 2026-08-16  
**Deciders**: Architecture Team  

## Context

The ok-FPGA-Server must:

1. Detect all currently connected Opal Kelly FPGAs at startup
2. React to FPGAs being connected while the server is running
3. Properly handle FPGAs being disconnected (clean up resources, notify clients)
4. Filter out USB 2.0 devices (only USB 3.0, PCIe, and Gen3 interfaces are supported)
5. Support `okREALM_TEST` for debug/unit-test scenarios (emulated device connections)

The FrontPanel 6.1.0 C API provides two mechanisms:

- **`FrontPanelDevices`** — point-in-time snapshot of connected devices (`okFrontPanelDevices_Construct`, `GetCount`, `GetSerial`, `Open`)
- **`FrontPanelManager`** — persistent monitor with `OnDeviceAdded`/`OnDeviceRemoved` callbacks (`okFrontPanelManager_ConstructWithCallbacks`, `StartMonitoring`, `StopMonitoring`)

## Decision

### Use `FrontPanelManager` as the primary device lifecycle mechanism

The `FrontPanelManager` C API provides asynchronous callbacks when devices are connected or disconnected. This is the correct choice for a long-running server because:

- It avoids polling (which wastes CPU and introduces latency)
- It provides real-time notification to update the web dashboard and notify API clients
- It integrates with `okREALM_TEST` for unit testing via `okFrontPanelManager_EmulateTestDeviceConnection`

### Architecture

```
┌─────────────────────────────────────────────────┐
│              DeviceManager (Rust)                │
│                                                  │
│  ┌────────────────────┐  ┌───────────────────┐  │
│  │ FrontPanelManager  │  │ Device Registry   │  │
│  │ (C API callbacks)  │──│ HashMap<Serial,   │  │
│  │                    │  │   DeviceState>     │  │
│  └────────┬───────────┘  └───────────────────┘  │
│           │                        │             │
│  OnDeviceAdded(serial)             │             │
│  OnDeviceRemoved(serial)           │             │
│           │                        │             │
│  ┌────────▼───────────┐           │             │
│  │ Event Channel      │───────────┘             │
│  │ (tokio::broadcast) │                         │
│  └────────┬───────────┘                         │
│           │                                      │
└───────────┼──────────────────────────────────────┘
            │
     ┌──────┴──────┬──────────────┐
     ▼             ▼              ▼
  REST SSE    gRPC Stream    Web Dashboard
  (clients)   (clients)      (WebSocket)
```

### Initialization Sequence

1. **Construct manager**: Call `okFrontPanelManager_ConstructWithCallbacks(self, realm, on_added_cb, on_removed_cb)`
   - `realm` = `""` (empty string → `okREALM_LOCAL`) for production
   - `realm` = `"test"` (`okREALM_TEST`) for unit tests
2. **Start monitoring**: Call `okFrontPanelManager_StartMonitoring(handle)`
3. **Initial enumeration**: The manager fires `OnDeviceAdded` for each already-connected device
4. **On `OnDeviceAdded(serial)`**:
   - Open the device via `okFrontPanelManager_Open(handle, serial)` → `okFrontPanel_HANDLE`
   - Call `okFrontPanel_GetDeviceInfoWithSize` to populate `okTDeviceInfo`
   - **Filter**: If `info.deviceInterface == OK_INTERFACE_USB2` (1), log a warning and close the device — do NOT register it
   - Register the device in the `DeviceRegistry` with state `Connected` (but not yet programmed, so no resources)
   - Broadcast `DeviceAdded { serial, product_name, ... }` event
5. **On `OnDeviceRemoved(serial)`**:
   - Mark all resources from this device as `Unavailable`
   - Notify any users holding resources from this device
   - Remove device from registry
   - Broadcast `DeviceRemoved { serial }` event
6. **Shutdown**: Call `okFrontPanelManager_StopMonitoring` then `okFrontPanelManager_Destruct`

### Callback Thread Safety

The FrontPanel DLL calls `OnDeviceAdded` and `OnDeviceRemoved` from an internal thread (not the Rust async runtime). The callbacks must:

1. Be `extern "C"` functions with minimal work
2. Send the serial string through a `std::sync::mpsc::Sender` or `tokio::sync::mpsc::Sender` to the async runtime
3. The async runtime task processes events and updates the `DeviceRegistry` (which is behind an `Arc<RwLock<...>>`)

This avoids holding DLL-internal locks while doing Rust async work.

### Device State (per-device, separate from resource state)

```
Connected (no bitfile)
    │
    ▼ ConfigureFPGA(bitfile)
Programmed (bitfile loaded, resources can be created)
    │
    ▼ Device disconnected
Disconnected (removed from registry)
```

### Testing with `okREALM_TEST`

- Unit tests construct the manager with `realm = "test"`
- Call `okFrontPanelManager_EmulateTestDeviceConnection(serial, true)` to simulate connection
- Call `okFrontPanelManager_EmulateTestDeviceConnection(serial, false)` to simulate disconnection
- This enables full lifecycle testing without physical hardware

## Consequences

### Positive
- **Real-time hot-plug** — zero-latency detection of device connect/disconnect
- **Clean event model** — all state changes flow through a single event channel
- **Testable** — `okREALM_TEST` enables hardware-free CI
- **USB 2.0 filtering at the earliest point** — devices filtered in `OnDeviceAdded` before any state is created

### Negative
- **Callback complexity** — DLL callbacks from a foreign thread require careful synchronization
- **Manager lifecycle** — the `FrontPanelManager` handle must outlive all opened device handles

### Risks
- **Callback ordering** — if the DLL fires `OnDeviceRemoved` before the `OnDeviceAdded` processing completes, the channel-based approach naturally serializes this
- **DLL thread contention** — callbacks must be lightweight; heavy work (device interrogation) happens on the async side

## Addendum (2026-08-16): Phased Implementation

This ADR's full `FrontPanelManager` callback design is being implemented in
two phases:

**Phase 1 (ADR-013)**: Startup-only device scan using `FrontPanelDevices`
snapshot enumeration. No hot-plug — devices connected after server start are
not detected until restart. This was chosen to deliver working device
visibility in the dashboard without the complexity of C callback → Rust
channel wiring.

**Phase 2 (this ADR, future)**: Full `FrontPanelManager` callback integration
with `extern "C"` callbacks → `tokio::sync::mpsc` channel → `DeviceManager`
events. This will also enable `okREALM_TEST` for Tier 1 emulated tests
(see `.agents/rules/ai-collaboration.md` §9.2).

The two phases share the same `DeviceManager`/`DeviceRegistry` infrastructure
(which is already built). Phase 2 replaces Phase 1's one-shot scan with a
persistent monitor.

## References

- [FrontPanelManager C API](../../FrontPanel-6.1.0/okFrontPanel.h) — lines 1271-1306
- [FrontPanelManager C++ class](../../FrontPanel-6.1.0/okFrontPanel.h) — lines 2235-2273
- [okREALM_TEST / okREALM_LOCAL](../../FrontPanel-6.1.0/okFrontPanel.h) — lines 2232-2233
- [EmulateTestDeviceConnection](../../FrontPanel-6.1.0/okFrontPanel.h) — line 1306
- [ADR-013 — Device Scanner](ADR-013-device-scanner.md) — Phase 1 implementation

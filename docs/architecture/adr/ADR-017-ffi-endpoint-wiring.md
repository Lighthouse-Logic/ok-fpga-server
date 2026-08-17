# ADR-017: FFI Endpoint Wiring Strategy

**Status**: Accepted  
**Date**: 2026-08-17  
**Deciders**: Architecture Team  
**Depends on**: [ADR-001](ADR-001-ffi-strategy.md) (FFI layer),
[ADR-013](ADR-013-device-scanner.md) (Device Scanner),
[ADR-018](ADR-018-device-handle-pool.md) (Device Handle Pool)

## Context

Phases 5–6 implemented REST and gRPC API servers with **stub handlers** that
return `501 Not Implemented` for all endpoints requiring FPGA hardware access.
The FFI layer (Phase 3) already loads all ~160 FrontPanel C symbols via
`libloading`, and the Device Scanner (Phase 14) demonstrates working DLL usage
at startup. However, no request-path code calls through to the DLL.

Phase 18 hardware testing confirmed that **20 of 23 Python REST tests skip**
because `POST /configure`, all Classic endpoints, all AXI endpoints, and
`GET /sensors` return 501.

The gap is straightforward: REST/gRPC handlers lack access to the loaded DLL
and open device handles.

## Decision

### 1. Request-Path Call Chain

Every FFI-backed endpoint follows this call chain:

```
HTTP/gRPC Request
      │
      ▼
  Route Handler (axum / tonic)
      │  extracts: serial, request body
      ▼
  AppState.handle_pool.get(serial)
      │  returns: Arc<Mutex<OpenDevice>>
      ▼
  lock device mutex (tokio::sync::Mutex)
      │  ensures: exclusive FFI access per-device
      ▼
  FFI call via FrontPanelDll function pointer
      │  e.g., (dll.classic_set_wire_in_value)(handle, endpoint, value, mask)
      ▼
  Check ok_ErrorCode
      │  map to HTTP status or gRPC status
      ▼
  Return JSON / protobuf response
```

### 2. Error Mapping

FrontPanel `ok_ErrorCode` values map to HTTP/gRPC statuses as follows:

| `ok_ErrorCode` | HTTP Status | gRPC Status | When |
|----------------|-------------|-------------|------|
| `ok_NoError` (0) | 200 OK | OK | Success |
| `ok_DeviceNotOpen` (-2) | 503 Service Unavailable | UNAVAILABLE | Device disconnected mid-request |
| `ok_InvalidParameter` (-4) | 400 Bad Request | INVALID_ARGUMENT | Bad endpoint/address/value |
| `ok_Failed` (-1) | 500 Internal Server Error | INTERNAL | Generic FFI failure |
| `ok_Timeout` (-5) | 504 Gateway Timeout | DEADLINE_EXCEEDED | USB transfer timeout |
| `ok_UnsupportedFeature` (-17) | 501 Not Implemented | UNIMPLEMENTED | Bitfile doesn't support this |
| `ok_FPGANotConfigured` (-23) | 409 Conflict | FAILED_PRECONDITION | FPGA not programmed |

All error responses use RFC 7807 Problem Details (REST) or `google.rpc.Status`
with detail message (gRPC).

### 3. Configure FPGA — Dual Content-Type Support

The `POST /devices/{serial}/configure` endpoint supports two content types:

#### Binary Upload (Remote-Capable)

```http
POST /api/v1/devices/22520013CC/configure
Content-Type: multipart/form-data; boundary=...

--boundary
Content-Disposition: form-data; name="bitfile"; filename="Counters.bit"
Content-Type: application/octet-stream

<binary bitfile data>
--boundary--
```

Uses `okFrontPanel_ConfigureFPGAFromMemory(handle, data.as_ptr(), data.len())`.

#### Local Path (Development Convenience)

```http
POST /api/v1/devices/22520013CC/configure
Content-Type: application/json

{"path": "/path/to/Counters.bit"}
```

Uses `okFrontPanel_ConfigureFPGA(handle, c_path.as_ptr())`.

The server distinguishes by `Content-Type` header. Both methods update the
`DeviceManager` state to `Programmed` and store a `BitfileRecord`.

### 4. Transport Security for Binary Uploads

> **Important**: Binary bitfile uploads over plain HTTP are **not encrypted**.
> In production deployments, bitfile data should be protected in transit.

**Recommended approaches (in order of preference):**

1. **Reverse proxy with TLS** — Place nginx, Caddy, or Traefik in front of
   the server. Zero code changes, automatic cert management via Let's Encrypt
   or internal CA. This is the recommended production deployment model.

2. **Native TLS** (future work) — Add `axum-server` with `rustls` behind a
   `[tls]` config section in `config.toml`:
   ```toml
   [tls]
   enabled = true
   cert_file = "/etc/ssl/certs/okfpga.pem"
   key_file = "/etc/ssl/private/okfpga.key"
   ```

3. **Local-only path mode** — When client and server are on the same machine,
   use the JSON path mode to avoid transferring bitfile data over the network
   entirely.

Native TLS support is deferred to a future phase, consistent with ADR-008's
approach of incrementally adding security features.

### 5. Thread Safety — Per-Device Mutex

The FrontPanel C API is **not thread-safe per device handle**. Two concurrent
calls on the same `okFrontPanel_HANDLE` cause undefined behavior.

The `DeviceHandlePool` (ADR-018) wraps each device handle in a
`tokio::sync::Mutex`. This choice (over `std::sync::Mutex`) is important
because FFI calls to USB devices may block for 10–100ms (especially pipe
transfers), and `tokio::sync::Mutex` yields the async runtime while waiting,
whereas `std::sync::Mutex` would block the entire tokio worker thread.

**Different devices** can be accessed concurrently — each has its own mutex.

### 6. Data Port Handle Lifetime

After `ConfigureFPGA` succeeds:

1. Server obtains `ClassicDataPort` handle via `GetFPGADataPortClassic()`
2. Server obtains `AXIDataPort` handle via `GetFPGADataPortAXI()`
3. Both are cached in the `OpenDevice` struct inside the pool

These handles are **invalidated** when:
- The FPGA is reprogrammed (new `ConfigureFPGA` call) → old handles dropped,
  new ones obtained
- The device is disconnected → entire `OpenDevice` is dropped
- The device is reset → data port handles re-obtained

Classic and AXI data port handles are **borrowed** from the parent
`FrontPanelHandle` (not independently owned), so they must NOT outlive the
parent handle.

## Files Changed

| File | Change |
|------|--------|
| `crates/ok-fpga-server/src/handle_pool.rs` | [NEW] DeviceHandlePool |
| `crates/ok-fpga-server/src/state.rs` | Add `handle_pool` field |
| `crates/ok-fpga-server/src/main.rs` | Wire pool into startup and poll loop |
| `crates/ok-fpga-server/src/rest/devices.rs` | Wire configure, sensors, bitfile, dataport |
| `crates/ok-fpga-server/src/rest/classic.rs` | Wire all 13 Classic handlers |
| `crates/ok-fpga-server/src/rest/axi.rs` | Wire AXI Lite handlers |
| `crates/ok-fpga-server/src/grpc/device_service.rs` | Wire gRPC RPCs + resource CRUD |

## Consequences

### Positive

- **All 501 stubs become functional** — enables full hardware testing
- **Clean separation** — FFI calls confined to `handle_pool.rs` methods, handlers
  remain thin
- **Error transparency** — FrontPanel error codes surface as structured API errors
- **Dual configure mode** — supports both remote binary upload and local path

### Negative

- **Increased unsafe surface** — every FFI call is `unsafe`, though confined to
  `handle_pool.rs`
- **USB latency** — pipe and block pipe operations may take 10–100ms under
  mutex lock, blocking other requests for the same device
- **No native TLS** — binary uploads unencrypted without reverse proxy

### Future Work

- Native TLS support via `rustls` / `axum-server`
- Pipe and block pipe operations with proper binary data handling
- `ConfigureFPGA` progress reporting via SSE/WebSocket

### Addendum A: AXI Sub-Interface Wiring Correction (2026-08-17)

See [ADR-019 — AXI Interface Architecture](ADR-019-axi-interface-architecture.md).

The original "Future Work" section described AXI Full and AXI Stream endpoint
wiring as "lower priority, not needed for Counters.bit testing." This has been
corrected:

1. **AXI Full and AXI Stream endpoints are fully wired** (routes defined in
   `axi.rs`, FFI calls connected). AXI Stream handlers remain stubs (501)
   pending gRPC bidirectional streaming design.

2. **Each AXI sub-interface endpoint uses only its own sub-handle.** An earlier
   implementation incorrectly had AXI Lite read/write falling back to AXI Full.
   This has been identified as a design error — AXI Lite, Full, and Stream are
   independent protocols with different semantics. See ADR-019 §1 for details.

3. **AXI cannot be tested on the XEM7320** (Legacy data port device). AXI testing
   requires a Gen3-USB device such as the XEM8320 or XEM8350.

## References

- [ADR-001 — FFI Strategy](ADR-001-ffi-strategy.md)
- [ADR-003 — Data Port Abstraction](ADR-003-data-port-abstraction.md)
- [ADR-008 — Authentication Stubs](ADR-008-authentication.md)
- [ADR-013 — Device Scanner](ADR-013-device-scanner.md)
- [ADR-018 — Device Handle Pool](ADR-018-device-handle-pool.md)
- [ADR-019 — AXI Interface Architecture](ADR-019-axi-interface-architecture.md)
- [FP6 API Surface Catalog](../fp6-api-surface.md)


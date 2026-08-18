# ADR-022: Dashboard Log Streaming & Level Filtering

**Status**: Accepted  
**Date**: 2026-08-18  
**Deciders**: Architecture Team  
**Depends on**: [ADR-007](ADR-007-web-dashboard.md) (Web Dashboard),
[ADR-002](ADR-002-device-lifecycle.md) (Device Lifecycle)

## Context

[ADR-007 §77-94](ADR-007-web-dashboard.md) specifies the log streaming design:
a custom `tracing` subscriber layer that broadcasts structured JSON log entries
over WebSocket. The dashboard's JavaScript (`app.js`) already has scaffolding
for this — `addLogEntry(level, message, timestamp)`, a log container, clear
button, and auto-scroll checkbox — and it handles `type: "log"` WebSocket
events. However, the server-side subscriber layer that feeds `tracing` logs
into the broadcast channel was never implemented.

Additionally, there is no mechanism for the user to control **which log levels**
appear in the dashboard. In a hardware debugging context, the user may want to
see every FFI call (TRACE) when diagnosing a wiring issue, but only device
connect/disconnect events (INFO) during normal operation.

### Requirements

1. Stream server-side `tracing` log events to the web dashboard in real time
2. Allow the user to select the displayed log level (TRACE, DEBUG, INFO, WARN, ERROR)
3. Persist logs to a file on disk for post-mortem analysis
4. Allow the user to clear and download the log file from the dashboard
5. Make the default dashboard log level configurable via TOML

## Decision

### 1. Server-Side Architecture

#### Tracing Subscriber Stack

The `tracing` subscriber is composed as a layered stack using
`tracing_subscriber::Registry`:

```
┌────────────────────────────────────────────────────────────┐
│                   tracing Subscriber Stack                  │
│                                                             │
│  ┌─────────────────────┐   ┌────────────────────────────┐  │
│  │ fmt::Layer           │   │ WebSocketBroadcastLayer    │  │
│  │ (stdout + file)      │   │ (new module)               │  │
│  │ Filter: config level │   │ Filter: TRACE (capture all)│  │
│  └─────────────────────┘   └─────────┬──────────────────┘  │
│                                       │                     │
└───────────────────────────────────────┼─────────────────────┘
                                        │
                              broadcast::Sender<LogEvent>
                                        │
                         ┌──────────────┼──────────────┐
                         ▼              ▼              ▼
                    WS Client 1    WS Client 2    WS Client N
                    (filter:INFO)  (filter:DEBUG) (filter:TRACE)
```

The `WebSocketBroadcastLayer` captures **all** events at TRACE level (the
most verbose). Filtering happens **per-client** at WebSocket send time based
on the level each client has requested. This avoids reconfiguring the global
tracing subscriber when a dashboard user changes their log level.

#### `LogEvent` Structure

Each captured tracing event is serialized to a `LogEvent` struct and sent
over the broadcast channel:

```rust
pub struct LogEvent {
    /// ISO 8601 timestamp.
    pub timestamp: String,
    /// Log level: TRACE, DEBUG, INFO, WARN, ERROR.
    pub level: tracing::Level,
    /// The tracing target (e.g., `ok_fpga_server::handle_pool`).
    pub target: String,
    /// Human-readable message.
    pub message: String,
    /// Structured fields from the tracing span/event.
    pub fields: HashMap<String, String>,
}
```

Sent over WebSocket as JSON:

```json
{
    "type": "log",
    "timestamp": "2026-08-18T07:12:00.123Z",
    "level": "INFO",
    "target": "ok_fpga_server::scanner",
    "message": "Device 22520013CC connected (XEM7320-A75, USB3)",
    "fields": { "serial": "22520013CC", "product": "XEM7320-A75" }
}
```

#### Ring Buffer for Backfill

A ring buffer (capacity: 200 entries) stores recent log events. When a new
WebSocket client connects, it receives the buffered entries matching its
requested log level before switching to live streaming. This ensures the
Logs page is not empty on initial load.

The ring buffer does **not** load from the disk log file on startup — it
starts empty and fills as the server runs. This keeps startup fast and
avoids reading potentially large log files into memory.

### 2. Log File Persistence

#### File Backend

Logs are also written to a file on disk using a `tracing` file appender
layer. The file path is configurable via TOML:

```toml
[server]
log_level = "info"
log_file = "logs/okfpga-server.log"       # default
```

The log file is **appended to** (not truncated) on each server start. This
allows multiple server sessions to accumulate in the same file.

#### Dashboard Log Management

The management API exposes two endpoints for log file management:

| Endpoint | Method | Description |
|----------|--------|-------------|
| `GET /mgmt/logs/download` | GET | Download the log file as `text/plain` |
| `DELETE /mgmt/logs` | DELETE | Truncate the log file to zero bytes |
| `GET /mgmt/logs/size` | GET | Return the log file size in bytes |

The dashboard's Clear button calls `DELETE /mgmt/logs` (with a confirmation
dialog) and displays the file size on the button label:
`Clear Log (2.4 MB)`.

### 3. Log Level Definitions

| Level | What Gets Logged |
|-------|-----------------|
| **ERROR** | FFI errors that indicate hardware failure, server panics, service bind failures |
| **WARN** | Stub/unimplemented endpoint invocations, degraded operation (DLL not found, broker unreachable) |
| **INFO** | Device connect/disconnect, bitfile programming, service start/stop, user session create/expire |
| **DEBUG** | REST/gRPC request summaries (method, path, status, latency), resource state transitions |
| **TRACE** | Every FFI call with arguments and return codes, full REST/gRPC request details, every stub code invocation |

### 4. Specific Logging Requirements

#### Device Events (INFO)

```
INFO  Device 22520013CC connected (XEM7320-A75, USB3)
INFO  Device 22520013CC disconnected (hot-unplug detected by polling)
INFO  Device 22520013CC programmed with xem7320_counters.bit (by rest-client, 1.2s)
```

#### FFI Errors (TRACE)

Every FFI call that returns a non-zero error code is logged at TRACE level
with the function name, arguments, and the error code with its description:

```
TRACE FFI: okFrontPanel_GetDeviceSensors(22520013CC) → error -15 (NotSupported)
TRACE FFI: okFrontPanel_ConfigureFPGA(22520013CC, "test.bit") → error -3 (FileError)
```

#### Stub/Unimplemented Calls (WARN + TRACE)

When a REST or gRPC handler invokes code that is not yet wired to FFI,
two log entries are emitted:

- **WARN**: Short message visible at INFO+ level — alerts the user
- **TRACE**: Detailed message with the full request context

```
WARN  GetDeviceSensors: FFI not wired — returning 501 Not Implemented
TRACE Stub invocation: GET /api/v1/devices/22520013CC/sensors (serial=22520013CC)
```

#### REST/gRPC Request Tracing

**TRACE level**: Every individual request with method, path/RPC, status, and latency:

```
TRACE REST: GET /api/v1/devices/22520013CC/settings → 200 OK (12ms)
TRACE gRPC: DeviceService.GetDeviceSettings(serial=22520013CC) → OK (8ms)
```

**DEBUG level**: Periodic request summary (aggregated):

```
DEBUG REST: 15 requests in last 5s, 0 errors, avg latency 8ms
DEBUG gRPC: 3 RPCs in last 5s, 0 errors, avg latency 5ms
```

This is implemented as a **Tower middleware** layer shared between the
Axum (REST) and Tonic (gRPC) stacks. The middleware intercepts all requests
and emits a `tracing::trace!` event with the request details.

#### FFI Call Instrumentation

FFI call logging is implemented in the **handle pool** (`handle_pool.rs`),
which is the single call site for all FFI operations. Each public method
(`configure_from_path`, `configure_from_memory`, wire/pipe/trigger operations)
wraps its FFI calls with `tracing::trace!` instrumentation:

```rust
tracing::trace!(
    serial = %serial,
    function = "okFrontPanel_SetWireInValue",
    endpoint = endpoint,
    value = value,
    mask = mask,
    "FFI call"
);
let result = unsafe { (dll.set_wire_in_value)(handle, endpoint, value, mask) };
tracing::trace!(
    serial = %serial,
    function = "okFrontPanel_SetWireInValue",
    result = result,
    "FFI result"
);
```

### 5. Dashboard UI

#### Log Level Selector

A `<select>` dropdown labeled **"Min Level:"** is added to the Logs page
header, allowing the user to select the minimum displayed log level:

```
Min Level: [INFO ▾]    [Clear (2.4 KB)] [⬇ Download] [☑ Auto-scroll]
```

- **Default**: INFO (configurable via `[dashboard] default_log_level` in TOML)
- Changing the selection sends a WebSocket control message:
  `{"type": "set_log_level", "level": "DEBUG"}`
- The server's WebSocket handler updates the per-client filter
- Lower levels include all higher levels (DEBUG shows DEBUG + INFO + WARN + ERROR)
- A tooltip explains the precedence order:
  `"TRACE → DEBUG → INFO → WARN → ERROR (TRACE shows everything, ERROR shows only errors)"`

#### Log Entry Styling

| Level | CSS Class | Visual Treatment |
|-------|-----------|-----------------|
| ERROR | `log-error` | Red text, bold |
| WARN | `log-warn` | Amber/yellow text |
| INFO | `log-info` | Default text color |
| DEBUG | `log-debug` | Muted/gray text |
| TRACE | `log-trace` | Very muted, monospace font |

### 6. TOML Configuration

```toml
[server]
log_level = "info"                    # existing — controls stdout verbosity
log_file = "logs/okfpga-server.log"   # NEW — log file path (default shown)

[dashboard]
default_log_level = "info"            # NEW — default log level shown in dashboard
```

### 7. WebSocket Protocol Extension

The existing WebSocket protocol (`/ws/events`) is extended with two new
message types:

#### Client → Server

```json
{ "type": "set_log_level", "level": "DEBUG" }
```

Sets the per-client log level filter. Only log events at this level or
higher are forwarded to this client.

#### Server → Client

```json
{
    "type": "log",
    "timestamp": "2026-08-18T07:12:00.123Z",
    "level": "INFO",
    "target": "ok_fpga_server::scanner",
    "message": "Device 22520013CC connected",
    "fields": {}
}
```

## Files Changed

| File | Change |
|------|--------|
| `crates/ok-fpga-server/src/log_broadcast.rs` | [NEW] `WebSocketBroadcastLayer`, `LogEvent`, ring buffer |
| `crates/ok-fpga-server/src/main.rs` | Update `init_tracing()` to use layered Registry with broadcast + file layers |
| `crates/ok-fpga-server/src/management.rs` | Add log download/clear/size endpoints; update WS handler for per-client level filtering |
| `crates/ok-fpga-server/src/handle_pool.rs` | Add `tracing::trace!` instrumentation around FFI calls |
| `web/app.js` | Add log level selector, clear/download buttons, DEBUG/TRACE CSS |
| `web/index.html` | Add log level button group and download button to Logs page header |
| `web/style.css` | Add `log-debug` and `log-trace` CSS classes |

## Consequences

### Positive

- **Full observability** — every FFI call, every API request, every state
  transition is visible at TRACE level
- **User-controlled verbosity** — operators see only what they need (INFO
  by default, TRACE when debugging)
- **Post-mortem capability** — log file on disk survives server restarts
- **Zero overhead when not used** — the broadcast layer is essentially free
  when no WebSocket clients are subscribed at TRACE level
- **No new dependencies** — uses `tracing-subscriber` features already in
  the dependency tree

### Negative

- **Disk usage** — log file grows unbounded until manually cleared. Mitigation:
  the dashboard shows file size and provides a clear button.
- **Broadcast overhead at TRACE** — if a dashboard client requests TRACE level,
  every FFI call generates a broadcast message. Mitigated by the broadcast
  channel's bounded capacity (lagged clients are notified and skip messages).

### Risks

- **Sensitive data in logs** — authentication tokens or user credentials could
  appear in TRACE-level request dumps. Mitigation: the Tower middleware must
  redact `Authorization` headers and password fields before logging.

## References

- [ADR-007 — Web Dashboard Architecture](ADR-007-web-dashboard.md) — original log streaming spec (§77-94)
- [ADR-002 — Device Lifecycle](ADR-002-device-lifecycle.md) — device connect/disconnect events
- [ADR-017 — FFI Endpoint Wiring](ADR-017-ffi-endpoint-wiring.md) — FFI call patterns
- [ADR-018 — Device Handle Pool](ADR-018-device-handle-pool.md) — FFI instrumentation location
- [`tracing-subscriber` Layer API](https://docs.rs/tracing-subscriber/latest/tracing_subscriber/layer/trait.Layer.html)
- [`tracing-appender`](https://docs.rs/tracing-appender) — file appender for tracing

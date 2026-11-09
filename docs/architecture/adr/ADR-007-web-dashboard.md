# ADR-007: Web Dashboard Architecture

**Status**: Implemented  
**Date**: 2026-08-16  
**Deciders**: Architecture Team  

## Context

The ok-FPGA-Server must expose a web page showing:

1. Server status and uptime
2. Discovered FPGAs with device info, sensors, settings, SYZYGY ports
3. Which bitfile each FPGA is programmed with (filename, description, when, by whom)
4. Available resources and their states
5. Connected users and their held resources
6. REST and gRPC API traffic metrics
7. Server logs (browsable and real-time streaming)
8. Server restart control

Since the server already runs Axum for REST, the web dashboard is served from the same process.

## Decision

### Embedded SPA served by Axum

The web dashboard is a **single-page application (SPA)** compiled to static HTML/CSS/JS assets and embedded in the Rust binary (via `include_dir` or served from a configurable directory). Axum serves these static files alongside the REST API.

```
┌─────────────────────────────────────────────────┐
│                 Axum Server                      │
│                                                  │
│  /api/v1/*          → REST API handlers         │
│  /ws/events         → WebSocket (real-time)     │
│  /api/v1/events/*   → SSE (REST event stream)   │
│  /*                 → Static file serving (SPA) │
│                                                  │
└─────────────────────────────────────────────────┘
```

### Real-Time Updates

The dashboard receives real-time updates via **WebSocket** (`/ws/events`):

- Device connected/disconnected
- Resource state changes
- Bitfile programming events
- Sensor readings (on-demand refresh or periodic polling)
- Log entries (streamed as they are produced)

The WebSocket carries the same event types as the REST SSE stream and gRPC `WatchDeviceEvents`, ensuring consistency.

### Dashboard Pages / Views

| View | Content | Data Source |
|------|---------|-------------|
| **Server Overview** | Uptime, API version, FrontPanel DLL version, platform, listen addresses | Local state + `okFrontPanel_GetAPIVersionString` |
| **Devices** | List of connected FPGAs with product name, serial, interface type, firmware version | `GET /api/v1/devices/` |
| **Device Detail** | Full `okTDeviceInfo` fields, sensors table, settings key-value pairs, SYZYGY ports, bitfile info | `GET /api/v1/devices/{serial}/*` |
| **Resources** | All resources across all FPGAs, current state, holders | `GET /api/v1/resources/` |
| **Users** | Connected users and their held resources | `GET /api/v1/users/` |
| **API Traffic** | Request counts, latency histograms, error rates for REST and gRPC | Internal metrics (tower middleware) |
| **Logs** | Searchable log history + live streaming tail | `GET /api/v1/server/logs` (SSE) + WebSocket |
| **Server Control** | Restart button | `POST /api/v1/server/restart` |

### Technology Stack

| Concern | Technology |
|---------|-----------|
| SPA Framework | Vanilla HTML/CSS/JS or lightweight framework (e.g., Alpine.js, htmx) |
| Styling | Modern CSS (dark theme, responsive) |
| Real-time | WebSocket (primary) + SSE fallback |
| Charts/Metrics | Lightweight charting (Chart.js or similar) |
| Build | Pre-built assets committed to repo or built via npm in CI |

> **Note**: The SPA frontend technology is intentionally kept lightweight. A full React/Vue/Angular framework is not needed for a status dashboard. The priority is clear, real-time data presentation.

### Logging Integration

The server uses the `tracing` crate for structured logging. A custom `tracing` subscriber layer:

1. Writes logs to the configured log destination (file, stdout)
2. Broadcasts log entries to a `tokio::sync::broadcast` channel
3. The WebSocket handler and SSE endpoint subscribe to this channel
4. The web dashboard receives log entries as structured JSON:

```json
{
  "timestamp": "2026-08-16T17:00:00.123Z",
  "level": "INFO",
  "target": "ok_fpga_server::device_manager",
  "message": "Device ABC1234567 connected (XEM8320-AU25P)",
  "fields": { "serial": "ABC1234567" }
}
```

### API Traffic Metrics

Tower middleware (shared between Axum and Tonic) collects:
- Request count per endpoint
- Response latency (p50, p95, p99)
- Error count by status code / gRPC status
- Bytes transferred

These metrics are exposed via an internal endpoint consumed by the dashboard.

### Authentication Integration

The dashboard respects the authentication stubs (ADR-008):
- **No-auth mode** (default): Dashboard is freely accessible
- **Local auth mode**: Login page → session cookie
- **LDAP auth mode**: Login page → LDAP validation → session cookie

## Consequences

### Positive
- **Zero extra processes** — dashboard served from the same binary as REST/gRPC
- **Real-time** — WebSocket provides sub-second updates for device events and logs
- **Self-contained** — the entire system is a single deployable binary
- **Consistent** — dashboard consumes the same REST API that external clients use

### Negative
- **Frontend maintenance** — even a lightweight SPA requires HTML/CSS/JS maintenance
- **No complex UI** — the lightweight approach limits interactive features (acceptable for a status dashboard)

## References

- [Project Goals — Web Dashboard](../../../ok-FPGA-Server_Project-goals.txt) — line 4
- [Axum static file serving](https://docs.rs/axum/latest/axum/routing/struct.Router.html#method.nest_service)
- [tracing crate](https://docs.rs/tracing)

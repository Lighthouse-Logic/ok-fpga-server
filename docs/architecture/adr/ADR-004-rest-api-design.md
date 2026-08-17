# ADR-004: REST API Design — OpenAPI 3.0

**Status**: Accepted  
**Date**: 2026-08-16  
**Deciders**: Architecture Team  

## Context

The ok-FPGA-Server must expose FPGAs over a REST API conformant to OpenAPI 3.0.0. The REST API must:

1. Mirror the FrontPanel 6.1.0 API surface (device management, data port operations)
2. Support device discovery, hot-plug event notifications, resource management
3. Enable server restart via API
4. Allow bitfile programming and tracking
5. Be spec-first: the OpenAPI YAML is defined before any implementation code

The REST server will be built with **Axum**, gated behind the `rest` Cargo feature flag (ADR-010), and served on a configurable port separate from the management/dashboard port.

## Decision

### URL Hierarchy

```
/api/v1/
├── server/
│   ├── GET    /status              — server status, uptime, API version
│   ├── POST   /restart             — restart the server
│   ├── GET    /logs                — SSE stream of server log entries
│   ├── GET    /services            — list all services and their status
│   ├── POST   /services/{name}/start   — start a stopped service
│   ├── POST   /services/{name}/stop    — stop a running service
│   └── POST   /services/{name}/restart — restart a service (202 Accepted)
│
├── devices/
│   ├── GET    /                    — list all connected FPGAs
│   ├── GET    /{serial}            — device info (okTDeviceInfo fields)
│   ├── GET    /{serial}/sensors    — device sensor readings
│   ├── GET    /{serial}/settings   — device settings key-value pairs
│   ├── GET    /{serial}/settings/{key}  — individual setting
│   ├── PUT    /{serial}/settings/{key}  — update individual setting
│   ├── GET    /{serial}/syzygy     — SYZYGY port discovery (auto-enumerated)
│   ├── POST   /{serial}/configure  — program bitfile to FPGA
│   ├── DELETE /{serial}/configure  — clear FPGA configuration
│   ├── GET    /{serial}/bitfile    — current bitfile info (if programmed)
│   └── GET    /{serial}/dataport   — data port type (classic/axi/none)
│
├── devices/{serial}/classic/       — Classic data port operations
│   ├── PUT    /wires/in            — SetWireInValue (idempotent state assignment)
│   ├── GET    /wires/out           — UpdateWireOuts + GetWireOutValue
│   ├── POST   /triggers/in         — ActivateTriggerIn
│   ├── GET    /triggers/out        — UpdateTriggerOuts + IsTriggered/GetVector
│   ├── POST   /pipes/in            — WriteToPipeIn
│   ├── GET    /pipes/out           — ReadFromPipeOut
│   ├── POST   /pipes/block-in      — WriteToBlockPipeIn
│   ├── GET    /pipes/block-out     — ReadFromBlockPipeOut
│   ├── POST   /registers/read      — ReadRegister(s)
│   ├── PUT    /registers/write     — WriteRegister(s) (idempotent state assignment)
│   └── POST   /reset               — ResetFPGA
│
├── devices/{serial}/axi/           — AXI data port operations
│   ├── lite/
│   │   ├── POST /read              — AXILite Read (single or bulk)
│   │   ├── PUT  /write             — AXILite Write (idempotent state assignment)
│   │   └── POST /reset             — AXILite Reset
│   ├── full/
│   │   ├── POST /read              — AXIFull Read
│   │   ├── POST /write             — AXIFull Write
│   │   ├── GET  /datapath-width    — GetDatapathWidthByteCount
│   │   └── POST /reset             — AXIFull Reset
│   └── stream/
│       ├── POST /read              — AXIStream Read
│       ├── POST /write             — AXIStream Write
│       ├── GET  /datapath-width    — read/write datapath widths
│       └── POST /reset             — AXIStream Reset
│
├── resources/
│   ├── GET    /                    — list all resources across all FPGAs
│   ├── POST   /                    — create a resource (client-defined, ADR-016)
│   ├── GET    /{resource_id}       — resource detail and current state
│   ├── DELETE /{resource_id}       — delete a resource definition (ADR-016)
│   ├── POST   /{resource_id}/acquire  — acquire resource (shared or locked)
│   └── POST   /{resource_id}/release  — release resource
│
├── users/
│   ├── GET    /                    — list connected users
│   └── GET    /{user_id}           — user detail and held resources
│
└── events/
    └── GET    /stream              — SSE stream of device/resource events
```

### Versioning

- API version in URL path: `/api/v1/`
- Version bumped on breaking changes only
- Server status endpoint reports current API version

### Event Streaming

Real-time events use **Server-Sent Events (SSE)** on `GET /api/v1/events/stream`:

```
event: device_added
data: {"serial": "ABC1234567", "product_name": "XEM8320-AU25P"}

event: device_removed
data: {"serial": "ABC1234567"}

event: resource_state_changed
data: {"resource_id": "...", "state": "available", "device_serial": "ABC1234567"}

event: bitfile_programmed
data: {"serial": "ABC1234567", "filename": "design.bit", "user": "admin"}
```

### Request/Response Patterns

- All responses are JSON with `Content-Type: application/json`
- Error responses follow RFC 7807 Problem Details format
- Binary data (pipe reads, bitfile uploads) use `application/octet-stream` or `multipart/form-data`
- Pipe reads return data as base64-encoded JSON or raw bytes depending on `Accept` header

### Example: Device Info Response

```json
{
  "serial": "ABC1234567",
  "device_id": "MyFPGA",
  "product_name": "XEM8320-AU25P",
  "product_id": 56,
  "device_interface": "usb3",
  "usb_speed": "super",
  "device_major_version": 1,
  "device_minor_version": 2,
  "wire_width": 32,
  "trigger_width": 32,
  "pipe_width": 32,
  "fpga_vendor": "xilinx",
  "dataport": "classic",
  "is_front_panel_enabled": true,
  "bitfile": {
    "filename": "design.bit",
    "description": "Signal processing v2.1",
    "programmed_at": "2026-08-16T17:00:00Z",
    "programmed_by": "admin"
  },
  "syzygy_ports": [ ... ]
}
```

### OpenAPI Specification

The full OpenAPI 3.0.0 YAML will be created at `api/openapi.yaml` before any implementation code. It will define:
- All paths, operations, parameters
- All request/response schemas
- All error responses
- Security scheme placeholders (for auth stubs)

## Consequences

### Positive
- **Spec-first** — OpenAPI YAML drives code generation and documentation
- **RESTful** — clean resource-oriented URLs
- **Real-time** — SSE enables efficient hot-plug notifications without polling
- **Comprehensive** — covers the entire FrontPanel API surface

### Negative
- **Large API surface** — ~40+ endpoints reflecting the FrontPanel API breadth
- **Binary data handling** — pipe/block transfers over REST are inherently less efficient than gRPC streaming

## Addendum A: HTTP Method Corrections (2026-08-17)

See [ADR-015](ADR-015-http-method-semantics.md) for full rationale.

Three endpoints changed from POST to PUT based on RFC 9110 idempotency analysis:
- `/classic/wires/in` — POST → **PUT** (idempotent wire state assignment)
- `/classic/registers/write` — POST → **PUT** (idempotent register write)
- `/axi/lite/write` — POST → **PUT** (idempotent AXI Lite register write)

URL hierarchy above has been updated to reflect these changes.

## Addendum B: Client-Side Resource CRUD (2026-08-17)

See [ADR-016](ADR-016-hardware-integration-testing.md) for full rationale.

Two new resource endpoints added:
- `POST /resources` — client creates a resource after programming a bitfile
- `DELETE /resources/{resource_id}` — client removes a resource definition

URL hierarchy above has been updated to reflect these additions.

## References

- [OpenAPI 3.0.0 Specification](https://spec.openapis.org/oas/v3.0.0)
- [RFC 7807 — Problem Details for HTTP APIs](https://tools.ietf.org/html/rfc7807)
- [Server-Sent Events (SSE)](https://html.spec.whatwg.org/multipage/server-sent-events.html)
- [ADR-010 Service Manager](ADR-010-service-manager.md) — feature flags and service lifecycle
- [ADR-015 HTTP Method Semantics](ADR-015-http-method-semantics.md) — POST vs PUT audit
- [ADR-016 Hardware Integration Testing](ADR-016-hardware-integration-testing.md) — resource creation and test matrix

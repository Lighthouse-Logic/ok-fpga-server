# ADR-020: Client-Side Resource Registration

**Status**: Implemented  
**Date**: 2026-08-17  
**Deciders**: Architecture Team  
**Amends**: [ADR-006](ADR-006-resource-management.md) (adds client-side resource creation,
enhanced resource schema, heartbeat/liveness, lock/unlock semantics),
[ADR-004](ADR-004-rest-api-design.md) (adds `POST /resources`, `DELETE /resources/{id}`,
`POST /heartbeat`, lock/unlock endpoints),
[ADR-005](ADR-005-grpc-api-design.md) (adds `CreateResource`, `DeleteResource`,
`LockResource`, `UnlockResource`, `Ping` RPCs)  
**Previously part of**: [ADR-016](ADR-016-hardware-integration-testing.md) (split out)

## Context

ADR-006 defined the resource model but left resource creation as "developer-defined"
without specifying the creation protocol. ADR-016 originally combined hardware
integration testing with a basic resource registration protocol. This ADR extracts
and significantly expands the resource registration design with:

1. **Access mode constraints** declared at registration time
2. **Address range declarations** with overlap validation
3. **Resource persistence** rules (survive client disconnect, deleted on device disconnect)
4. **Heartbeat/liveness** with auto-release of abandoned resources
5. **Interface type declarations** (Classic, AXI-Lite, AXI-Full, AXI-Stream)
6. **Classic endpoint declarations** with per-EP sharing semantics
7. **Extensible metadata** via key-value map

## Decision

### 1. Client-Side Resource Registration Principle

**The server is agnostic to bitfile capabilities.** The client that programs an
FPGA also registers the resources that the bitfile exposes. The server stores,
validates, and enforces the resource schema — but the client provides the schema
because only the client/integrator knows what the HDL bitfile implements.

#### Protocol Flow

```
Client                              Server
  │                                   │
  │  POST /devices/{serial}/configure │
  │  (upload Counters.bit)            │
  │──────────────────────────────────→│  Programs FPGA
  │  200 OK                           │  Returns success
  │←──────────────────────────────────│
  │                                   │
  │  POST /resources                  │
  │  { name, device_serial,           │
  │    access_mode, interfaces,       │
  │    endpoints, address_ranges,     │
  │    metadata }                     │
  │──────────────────────────────────→│  Validates & creates resource
  │  201 Created                      │  State: Available
  │  Location: /resources/{id}        │
  │←──────────────────────────────────│
  │                                   │
  │  POST /resources/{id}/acquire     │
  │  { user_id }                      │
  │──────────────────────────────────→│  Acquires resource
  │  200 OK                           │  State: InUseShared or InUseLocked
  │←──────────────────────────────────│
  │                                   │
  │  (exercise endpoints...)          │
  │                                   │
  │  POST /resources/{id}/lock        │
  │  { user_id }                      │
  │──────────────────────────────────→│  Locks shared resource
  │  200 OK                           │  State: InUseLocked
  │←──────────────────────────────────│
  │                                   │
  │  POST /resources/{id}/unlock      │
  │  { user_id }                      │
  │──────────────────────────────────→│  Unlocks (back to shared)
  │  200 OK                           │  State: InUseShared
  │←──────────────────────────────────│
  │                                   │
  │  POST /resources/{id}/release     │
  │  { user_id }                      │
  │──────────────────────────────────→│  Releases resource
  │  200 OK                           │  State: Available
  │←──────────────────────────────────│
  │                                   │
  │  DELETE /resources/{id}           │
  │──────────────────────────────────→│  Removes resource
  │  204 No Content                   │
  │←──────────────────────────────────│
```

---

### 2. Resource Registration Schema

#### Full JSON Schema

```json
{
  "name": "xem7320-classic-counters",
  "description": "Classic data port counters from Counters.bit",
  "device_serial": "22520013CC",
  "access_mode": "shared",
  "interfaces": {
    "classic": true,
    "axi_lite": false,
    "axi_full": false,
    "axi_stream": false
  },
  "endpoints": {
    "default_access": "exclusive",
    "entries": [
      { "address": "0x00", "type": "wire_in", "access": "exclusive", "description": "Counter control" },
      { "address": "0x20", "type": "wire_out", "access": "shared",    "description": "Counter1 readback" },
      { "address": "0x21", "type": "wire_out", "access": "shared",    "description": "Counter2 readback" },
      { "address": "0x40", "type": "trigger_in", "access": "exclusive", "description": "Counter2 triggers" },
      { "address": "0x60", "type": "trigger_out", "access": "shared",   "description": "Counter1 comparators" }
    ]
  },
  "address_ranges": [
    { "start": "0x44A60000", "end": "0x44A8FFFF", "access": "exclusive" },
    { "start": "0x80000000", "end": "0x8FFFFFFF", "access": "shared" }
  ],
  "metadata": {
    "bitfile_version": "2.1.0",
    "firmware_build": "20260815",
    "project_name": "Counters"
  }
}
```

#### Field Descriptions

| Field | Type | Required | Description |
|-------|------|----------|-------------|
| `name` | string | ✅ | Human-readable resource name. Must be unique per device. |
| `description` | string | ❌ | Optional description of what this resource does. |
| `device_serial` | string | ✅ | Serial of the FPGA providing this resource. Device must be connected and programmed. |
| `access_mode` | enum | ✅ | `"shared"` or `"exclusive"`. See §3. |
| `interfaces` | object | ✅ | Which data port interfaces this resource uses. See §5. |
| `endpoints` | object | ❌ | Classic endpoint declarations. Required if `interfaces.classic == true`. See §6. |
| `address_ranges` | array | ❌ | AXI/register address ranges. See §7. |
| `metadata` | map | ❌ | Extensible key-value pairs. See §8. |

---

### 3. Access Mode Semantics

The `access_mode` field on a resource is a **design constraint** determined by the
FPGA HDL bitfile, not a runtime user choice. It controls how the resource can be
acquired.

| `access_mode` | Meaning | Acquisition Behavior |
|---------------|---------|---------------------|
| `exclusive` | Only one user can use this resource at a time | Acquisition always transitions to `InUseLocked`. Only one holder allowed. |
| `shared` | Multiple users can use this resource concurrently | Acquisition transitions to `InUseShared`. Multiple holders allowed. |

#### Lock / Unlock for Shared Resources

A user who holds a `shared` resource may temporarily **lock** it to prevent other
users from interfering with a critical operation:

```
POST /api/v1/resources/{resource_id}/lock
{ "user_id": "alice" }
```

- **Precondition**: The user must already hold the resource (be in the `holders` list).
- **Effect**: Resource transitions from `InUseShared` → `InUseLocked`.
- **Notification**: All other holders receive an event notification
  (`ResourceLocked`) via SSE/gRPC event stream.
- **Other holders**: Remain in the holders list but cannot perform operations on
  the resource until it is unlocked. They are NOT ejected.

```
POST /api/v1/resources/{resource_id}/unlock
{ "user_id": "alice" }
```

- **Precondition**: The user must be the one who locked the resource.
- **Effect**: Resource transitions from `InUseLocked` → `InUseShared`.
- **Notification**: All holders receive a `ResourceUnlocked` event.

#### Exclusive Resources

Resources declared as `exclusive` are **always locked** when acquired. There is no
separate lock/unlock step. Only one user can hold the resource at a time. The
resource is locked from acquisition until release.

Attempting to lock or unlock an `exclusive` resource returns an error — it is
implicitly always locked by its holder.

---

### 4. Resource Persistence & Device Lifecycle

#### Persistence Rules

- **Resources survive client disconnect**: Once registered, a resource persists in
  the server's registry regardless of whether the registering client is still
  connected. Other clients can discover and use the resource.
- **Resources are deleted on device disconnect**: When an FPGA is physically
  disconnected (USB removed, power lost), **all resources for that serial are
  deleted** from the registry. The device state is unknown after reconnection (it
  may have lost power, been reprogrammed by another tool, etc.).
- **Resources survive server restart**: Future work. Currently, resources are
  in-memory and lost on server restart.
- **Server-side static resources**: Future work. A configuration file or database
  could define persistent resource definitions that are automatically registered
  when a known device+bitfile combination is detected.

#### Event Notifications on Device Disconnect

When resources are deleted due to device disconnect:
1. All holders of affected resources receive a `ResourceStateChanged` event
   (new state: deleted).
2. The resource is removed from the registry.
3. Any heartbeat timers for affected users are cleaned up.

---

### 5. Heartbeat / Liveness

#### Problem

If a client acquires a resource (especially in exclusive or locked mode) and then
crashes, loses network connectivity, or simply abandons the session, the resource
becomes permanently locked — a "resource leak."

#### Design

**Implicit heartbeat (piggyback)**: Any API call from an identified user (`user_id`
header, authentication token, or gRPC metadata) automatically updates that user's
`last_seen_at` timestamp. No explicit heartbeat is needed as long as the client is
actively making API calls.

**Explicit heartbeat (fallback)**: If the client is idle (not making any API calls)
but wants to retain its resource holds, it must send periodic heartbeats:

| Protocol | Endpoint | Interval |
|----------|----------|----------|
| REST | `POST /api/v1/heartbeat` with body `{ "user_id": "alice" }` | ≤ 24 seconds |
| gRPC | `ServerService.Ping(PingRequest { user_id })` → `PingResponse` | ≤ 24 seconds |

**Timing parameters:**

| Parameter | Value | Rationale |
|-----------|-------|-----------|
| Server timeout | 60 seconds | Time without any activity before server auto-releases |
| Client heartbeat interval | ≤ 24 seconds | 2.5× faster than timeout to provide margin |
| Server sweep interval | 10 seconds | Background task checks for expired users |

#### Auto-Release Behavior

When a user's `last_seen_at` exceeds 60 seconds:

1. **All resources held by that user** are released:
   - If the resource was `InUseLocked` and this user was the locker → transitions
     to `InUseShared` (if other holders exist) or `Available` (if no other holders).
   - If the resource was `InUseShared` → user is removed from holders list.
     If no holders remain → transitions to `Available`.
2. **Event notifications**: Other holders (if any) receive a
   `ResourceStateChanged` event.
3. **User session**: The user's session is marked as expired but not deleted (they
   can reconnect and re-acquire resources).

---

### 6. Interface Type Declarations

Resources must declare which data port interfaces they use. At least one interface
must be enabled.

```json
{
  "interfaces": {
    "classic": true,
    "axi_lite": false,
    "axi_full": false,
    "axi_stream": false
  }
}
```

| Field | Description |
|-------|-------------|
| `classic` | Uses Classic FrontPanel endpoints (wires, triggers, pipes) |
| `axi_lite` | Uses AXI4-Lite registered access (CSRs, configuration) |
| `axi_full` | Uses AXI4 burst memory-mapped access (BRAM, DDR) |
| `axi_stream` | Uses AXI4-Stream addressless streaming (DSP, ADC/DAC) |

#### Validation at Registration Time (Strict)

The server validates interface declarations against the connected device's actual
capabilities:

- If `classic: true` → always accepted (all FrontPanel devices support Classic).
- If any AXI interface is `true` → the server calls `GetFPGADataPortAXI()`.
  - If it returns `UnsupportedFeature` → **reject registration** with error:
    `"Device {serial} does not support AXI interfaces (Legacy dataport)"`
  - If it returns `NoError` → the server further validates:
    - `axi_lite: true` → calls `GetAXILite()` and rejects if unavailable.
    - `axi_full: true` → calls `GetAXIFull()` and rejects if unavailable.
    - `axi_stream: true` → calls `GetAXIStream()` and rejects if unavailable.

---

### 7. Classic Endpoint Declarations

When `interfaces.classic == true`, the resource SHOULD declare which Classic
endpoints it uses. If no endpoints are declared, the resource is assumed to use
**all** endpoints in `exclusive` mode.

#### Endpoint Schema

```json
{
  "endpoints": {
    "default_access": "exclusive",
    "entries": [
      { "address": "0x00", "type": "wire_in", "access": "exclusive", "description": "Counter control" },
      { "address": "0x20", "type": "wire_out", "access": "shared",    "description": "Counter1 readback" },
      { "address": "0x40", "type": "trigger_in", "access": "exclusive", "description": "Counter2 triggers" },
      { "address": "0x60", "type": "trigger_out", "access": "shared",   "description": "Counter1 comparators" },
      { "address": "0x80", "type": "pipe_in", "access": "exclusive", "description": "Data upload" },
      { "address": "0x81", "type": "bt_pipe_in", "access": "exclusive", "description": "Throttled upload" },
      { "address": "0xA0", "type": "pipe_out", "access": "exclusive", "description": "Data download" },
      { "address": "0xA1", "type": "bt_pipe_out", "access": "exclusive", "description": "Throttled download" }
    ]
  }
}
```

#### Endpoint Types

| Type | Address Range | FrontPanel Module | Host API |
|------|-------------|-------------------|----------|
| `wire_in` | 0x00–0x1F | `okWireIn` | `SetWireInValue` / `UpdateWireIns` |
| `wire_out` | 0x20–0x3F | `okWireOut` | `UpdateWireOuts` / `GetWireOutValue` |
| `trigger_in` | 0x40–0x5F | `okTriggerIn` | `ActivateTriggerIn` |
| `trigger_out` | 0x60–0x7F | `okTriggerOut` | `UpdateTriggerOuts` / `IsTriggered` |
| `pipe_in` | 0x80–0x9F | `okPipeIn` | `WriteToPipeIn` |
| `pipe_out` | 0xA0–0xBF | `okPipeOut` | `ReadFromPipeOut` |
| `bt_pipe_in` | 0x80–0x9F | `okBTPipeIn` | `WriteToBlockPipeIn` |
| `bt_pipe_out` | 0xA0–0xBF | `okBTPipeOut` | `ReadFromBlockPipeOut` |

> **Note**: Standard Pipes (`pipe_in`/`pipe_out`) and Block-Throttled Pipes
> (`bt_pipe_in`/`bt_pipe_out`) share the same address ranges (0x80–0x9F for input,
> 0xA0–0xBF for output). A given endpoint address is instantiated in the HDL as
> either a standard pipe OR a BT pipe — never both simultaneously at the same address.
> The `type` field distinguishes which kind of pipe is used.
> See [notes-on-Pipes-and-Block-Throttled-Pipes.md](../../notes-on-Pipes-and-Block-Throttled-Pipes.md)
> for details on the differences.

#### Convenience: `default_access`

The `default_access` field applies to any endpoint NOT explicitly listed in
`entries`. This allows declaring only the exceptions:

```json
{
  "endpoints": {
    "default_access": "shared",
    "entries": [
      { "address": "0x00", "type": "wire_in", "access": "exclusive" }
    ]
  }
}
```

In this example, EP 0x00 is exclusive; all other EPs used by this resource default
to shared.

#### Validation at Registration Time

1. **Type must match address range**: If a client registers EP `0x00` as
   `trigger_in`, the server rejects with an error because 0x00 is in the WireIn
   range (0x00–0x1F). Exception: `pipe_in` and `bt_pipe_in` both share the
   0x80–0x9F range, and `pipe_out` and `bt_pipe_out` both share 0xA0–0xBF.

2. **Exclusive EP overlap check**: If Resource A declares EP 0x00 as `exclusive`
   (for the same device), and Resource B also tries to declare EP 0x00 as
   `exclusive`, Resource B's registration is **rejected** with an error:
   `"Endpoint 0x00 is already declared exclusive by resource '{resource_a_name}'"`

3. **Shared EPs may overlap**: Multiple resources on the same device may declare
   the same EP as `shared` without conflict.

4. **No runtime enforcement (v1)**: The server does NOT intercept individual
   `SetWireInValue` / `WriteToPipeIn` calls to check EP ownership. Enforcement is
   at registration time only. Stub code is provided for developers who wish to add
   per-call enforcement in the future.

---

### 8. Address Range Declarations

Address ranges represent FPGA design elements (memory-mapped regions) used by the
resource. They are 32-bit ranges that may be non-contiguous.

```json
{
  "address_ranges": [
    { "start": "0x44A60000", "end": "0x44A8FFFF", "access": "exclusive" },
    { "start": "0x80000000", "end": "0x8FFFFFFF", "access": "shared" }
  ]
}
```

| Field | Type | Description |
|-------|------|-------------|
| `start` | hex string | Start of the address range (inclusive), e.g. `"0x44A60000"` |
| `end` | hex string | End of the address range (inclusive), e.g. `"0x44A8FFFF"` |
| `access` | enum | `"shared"` or `"exclusive"` |

#### Validation

- `start` must be ≤ `end`.
- Both values must be valid 32-bit hex addresses.
- **Exclusive overlap check**: If Resource A on the same device declares range
  `0x44A60000–0x44A8FFFF` as `exclusive`, and Resource B tries to declare a range
  that overlaps, Resource B's registration is rejected.
- **Shared ranges may overlap**: Multiple resources may declare overlapping shared
  ranges without conflict.

#### Runtime Behavior

No per-call address filtering is performed in v1. The address range declaration is
used for:

1. **Registration-time overlap validation** (as described above).
2. **Client-side documentation** — clients can query a resource's address ranges to
   understand what memory regions it occupies.
3. **Server-side stub code** — a `ResourceAccessPolicy` trait method is provided
   for developers who understand their specific FPGA HDL design and wish to add
   address-based access control logic.

> **Future work**: When Gen3-USB (AXI) devices are available, address range
> enforcement at the AXI read/write call level will be implemented and tested.
> This is deferred until hardware is available for validation.

---

### 9. Extensible Metadata

Resources may include arbitrary key-value pairs for information that the server
does not interpret but stores and returns to clients:

```json
{
  "metadata": {
    "bitfile_version": "2.1.0",
    "firmware_build": "20260815",
    "project_name": "Counters",
    "fpga_part": "XC7A75T",
    "hdl_revision": "abc1234"
  }
}
```

In proto3, this maps to `map<string, string> metadata`.

The server provides stub code for any metadata key — if a developer wants to define
server-side behavior triggered by a specific metadata key (e.g., `"auto_configure":
"true"` to auto-load a bitfile), they implement the appropriate handler.

#### Documentation Requirements

All clients registering resources must document:
- Every metadata key they use and its meaning
- The format/schema of each value
- Whether the key is informational or triggers server behavior (via stub code)

---

### 10. REST API Changes

#### New Endpoints

| Method | Path | Description |
|--------|------|-------------|
| `POST` | `/api/v1/resources` | Create a resource (see §2 for schema) |
| `DELETE` | `/api/v1/resources/{resource_id}` | Delete a resource |
| `POST` | `/api/v1/resources/{resource_id}/lock` | Lock a shared resource |
| `POST` | `/api/v1/resources/{resource_id}/unlock` | Unlock a locked resource |
| `POST` | `/api/v1/heartbeat` | Explicit heartbeat (body: `{ "user_id": "..." }`) |

#### Modified Endpoints

| Method | Path | Change |
|--------|------|--------|
| `POST` | `/api/v1/resources/{resource_id}/acquire` | No longer accepts `mode` parameter for exclusive resources. Mode is determined by the resource's `access_mode`. |

#### Validation Responses

| Condition | HTTP Status | Error |
|-----------|-------------|-------|
| Device not connected | `404 Not Found` | `"Device '{serial}' not found"` |
| Device not programmed | `409 Conflict` | `"Device '{serial}' is not programmed"` |
| Name already exists (same device) | `409 Conflict` | `"Resource '{name}' already exists for device '{serial}'"` |
| EP overlap (exclusive) | `409 Conflict` | `"Endpoint 0x{addr} is already declared exclusive by resource '{name}'"` |
| Address range overlap (exclusive) | `409 Conflict` | `"Address range overlaps with exclusive range in resource '{name}'"` |
| AXI not supported | `422 Unprocessable Entity` | `"Device '{serial}' does not support AXI interfaces"` |
| EP type mismatch | `422 Unprocessable Entity` | `"Endpoint 0x{addr} is in the {actual_type} range, not {declared_type}"` |
| Resource currently held | `409 Conflict` | `"Cannot delete resource '{id}' — currently held by {n} user(s)"` |

---

### 11. gRPC API Changes

#### New RPCs

Add to `DeviceService` (resource management):

```protobuf
// Create a new resource for a programmed FPGA.
rpc CreateResource(CreateResourceRequest) returns (ResourceResponse);

// Delete a resource definition.
rpc DeleteResource(ResourceRequest) returns (StatusResponse);

// Lock a shared resource for exclusive use by the requesting user.
rpc LockResource(LockResourceRequest) returns (StatusResponse);

// Unlock a previously locked resource back to shared mode.
rpc UnlockResource(UnlockResourceRequest) returns (StatusResponse);
```

Add to `ServerService`:

```protobuf
// Client liveness ping. Updates the user's last_seen_at timestamp.
rpc Ping(PingRequest) returns (PingResponse);
```

#### New Event Types

```protobuf
message ResourceLocked {
  string resource_id = 1;
  string locked_by = 2;     // user_id of the locker
}

message ResourceUnlocked {
  string resource_id = 1;
  string unlocked_by = 2;   // user_id of the unlocker
}
```

---

## Consequences

### Positive

- **Server remains agnostic** — no bitfile-specific logic in the server
- **Client-driven workflow** — matches real-world usage where integrators know
  their bitfile's capabilities
- **Design-level access control** — `access_mode` is a hardware constraint, not
  a user preference, preventing accidental concurrent access to exclusive resources
- **Liveness safety** — heartbeat + auto-release prevents resource leaks from
  crashed clients
- **Rich resource metadata** — EP declarations, address ranges, and interface types
  provide a complete picture of what a resource uses
- **Extensible** — metadata map and stub code enable future custom logic

### Negative

- **Schema complexity** — the full registration schema is significantly more complex
  than the minimal v1 schema
- **No runtime enforcement (v1)** — per-call EP/address filtering is deferred
- **Client responsibility** — clients must accurately declare their resource schema;
  incorrect declarations can cause conflicts at runtime

### Future Work

- **Per-call EP enforcement** — middleware to intercept wire/trigger/pipe calls and
  verify the caller holds a resource that declares the requested EP
- **Per-call address enforcement** — for AXI devices, intercept read/write calls and
  verify address is within the caller's declared ranges
- **Server-side static resources** — define resources in a TOML/YAML config file
  that are automatically registered when a known device+bitfile is detected
- **Persistent resource storage** — database or file-backed registry that survives
  server restarts

### Amendments to ADR-006

This ADR adds to ADR-006:

- **Resource creation protocol**: Clients create resources via `POST /resources`
  after programming a bitfile.
- **Resource deletion protocol**: Clients delete resources via
  `DELETE /resources/{resource_id}`.
- **Access mode at registration**: The `access_mode` field is set at registration
  time as a hardware design constraint, not at acquisition time.
- **Lock/unlock semantics**: Shared resources can be temporarily locked and unlocked
  by any holder.
- **Heartbeat/liveness**: Users must maintain heartbeats to retain resource holds.
- **Delete on disconnect**: Resources are deleted (not just marked Unavailable)
  when the device is disconnected.

### Amendments to ADR-004

This ADR adds to ADR-004's URL Hierarchy:

- `POST /resources` — create a resource (new)
- `DELETE /resources/{resource_id}` — delete a resource (new)
- `POST /resources/{resource_id}/lock` — lock a shared resource (new)
- `POST /resources/{resource_id}/unlock` — unlock a locked resource (new)
- `POST /heartbeat` — client liveness heartbeat (new)

## References

- [ADR-006 — Multi-User Resource Management](ADR-006-resource-management.md)
- [ADR-004 — REST API Design](ADR-004-rest-api-design.md)
- [ADR-005 — gRPC API Design](ADR-005-grpc-api-design.md)
- [ADR-016 — Hardware Integration Testing](ADR-016-hardware-integration-testing.md)
- [ADR-019 — AXI Interface Architecture](ADR-019-axi-interface-architecture.md)
- [Notes on Pipes and Block-Throttled Pipes](../../notes-on-Pipes-and-Block-Throttled-Pipes.md)

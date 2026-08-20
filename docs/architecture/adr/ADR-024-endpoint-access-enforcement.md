# ADR-024: Endpoint Access Enforcement

**Status**: Implemented  
**Date**: 2026-08-18  
**Deciders**: Architecture Team  
**Depends on**: [ADR-006](ADR-006-resource-management.md) (Resource Management),
[ADR-020](ADR-020-client-side-resource-registration.md) (Client-Side Resource Registration),
[ADR-023](ADR-023-gui-example-clients.md) (GUI Example Clients — F11 origin)

## Context

### The Gap

ADR-020 §7 explicitly states:

> *"No runtime enforcement (v1): The server does NOT intercept individual
> `SetWireInValue` / `WriteToPipeIn` calls to check EP ownership. Enforcement
> is at registration time only."*

This means any user who can reach the server can call **any** Classic endpoint
(WireIn, WireOut, TriggerIn, TriggerOut, PipeIn, PipeOut) or **any** AXI address
on **any** connected device — even if they have not registered, acquired, or locked
a resource. The `X-User-Id` header is used for heartbeats but is not checked
before FPGA operations.

This was identified as **F11** during Phase 25 GUI testing (ADR-023 §25-Fix-H) and
flagged as a future ADR-024 candidate.

### Why It Matters

| Scenario | Risk without enforcement |
|----------|--------------------------|
| User A holds exclusive resource on EP 0x00 (control register). User B calls `SetWireInValue(0x00)` directly. | **Silent data corruption** — B overwrites A's control state without error |
| User A locks a shared resource for a critical multi-step operation. User B fires TriggerIn on the same endpoint. | **Race condition** — B disrupts A's locked operation |
| No user has acquired any resource. An unauthenticated script calls `WriteToPipeIn`. | **Uncontrolled access** — no audit trail, no coordination |
| Device is programmed but no resources registered yet. User calls `GetWireOutValue`. | **Read-only observation** — arguably harmless, but invisible to resource system |

### Existing Infrastructure

The codebase already has substantial groundwork for this:

1. **`ResourceAccessPolicy` trait** (`ok-fpga-core/src/resource.rs`) — has
   `validate_endpoint_access()` and `validate_address_access()` stub methods that
   currently return `Ok(())` for all requests
2. **`DefaultAccessPolicy`** — implements the `can_acquire()` rules but leaves
   endpoint/address stubs as no-ops
3. **`ResourceRegistry`** — already holds the resource definitions including
   endpoint declarations and address ranges
4. **`X-User-Id` / gRPC metadata** — already extracted by the heartbeat middleware
   on every request

---

## Decision

### 1. Enforcement Model: Three Levels

Introduce a server-level configuration setting that controls enforcement behavior:

```toml
# okfpga-server.toml
[access_control]
# "off"     — no enforcement (current behavior, v1 compatible)
# "warn"    — log a warning but allow the operation
# "enforce" — reject unauthorized operations with 403 / PERMISSION_DENIED
endpoint_enforcement = "off"
```

| Level | Behavior | Use Case |
|-------|----------|----------|
| `off` | No checks. Current v1 behavior. User ID is not required. | Development, single-user setups, backward compatibility |
| `warn` | Check passes, but violations are logged at WARN level with user, device, endpoint, and reason. **User ID is required** — requests without `X-User-Id` are logged as violations. | Migration period — lets teams audit violations before enabling enforcement |
| `enforce` | Unauthorized operations are rejected with `403 Forbidden` (REST) or `PERMISSION_DENIED` (gRPC). **User ID is required** — requests without `X-User-Id` are rejected. | Production multi-user environments |

Default: **`off`** — preserves backward compatibility.

**User ID requirement**: When `endpoint_enforcement` is anything other than `off`,
every FPGA operation request **must** include a valid `X-User-Id` header (REST) or
`user_id` metadata (gRPC). Requests without a user identity are treated as
unauthorized.

### 2. Authorization Rule: "Must Hold a Covering Resource"

For any Classic endpoint or AXI address operation, the enforcement middleware checks:

```
Given: user_id, device_serial, endpoint_address (or AXI address)

1. Find all resources on device_serial where user_id ∈ holders
2. For each held resource:
   a. If resource is InUseLocked and user_id ≠ locked_by → DENY
      (A locked resource can ONLY be used by the user who holds the lock.)
   b. Check if the resource's endpoint declarations cover the requested endpoint:
      - Explicit entry with matching address → ALLOW
      - No explicit entry but default_access is defined → ALLOW
      - No endpoint declarations at all (legacy resource) → ALLOW (see §3)
3. If no held resource covers the endpoint → DENY

DENY response:
  REST: 403 Forbidden — "User '{user_id}' does not hold a resource covering
        endpoint 0x{addr:02X} on device '{serial}'"
  gRPC: PERMISSION_DENIED with same detail message
```

### 3. Handling Legacy / Minimal Resources

ADR-020 resources registered without endpoint declarations (e.g., the current GUI
demos pass only `name`, `device_serial`, `access_mode`) are treated as **covering
all endpoints** on that device. This ensures existing clients continue to work
without schema changes.

| Resource has `endpoints`? | Enforcement behavior |
|---------------------------|---------------------|
| Yes, with entries | Check endpoint is in entries or falls under `default_access` |
| Yes, but `entries` is empty | `default_access` applies to all endpoints |
| No `endpoints` field at all | Resource covers all endpoints on the device (legacy mode) |

### 4. Where Enforcement Hooks In

#### Design Principles

1. **Lowest latency critical path** — minimize middleware between Request and
   Handler. Once a command is authorized and initiated, data streams to the
   handler without additional checks (see §5).
2. **Heartbeat decoupled from access control** — the heartbeat middleware
   (`touch()`) is an independent liveness concern (ADR-020 §5). It tracks
   when a user was last active. Access enforcement is a separate, independent
   layer. They do NOT serialize through each other.
3. **Access control on ALL routes** — the access middleware applies to all
   `/api/v1/*` routes, not just Classic/AXI. This ensures any future route
   (e.g., a hypothetical "TechX" data port) is automatically intercepted.

#### REST: Hybrid Enforcement (Middleware + In-Handler)

The access enforcement middleware is a lightweight layer on all `/api/v1/*`
routes. It handles user identity validation and route-level gating. Detailed
endpoint-level checks are performed inside each handler after deserialization.

```
Request ──┬──→ Access MW (user_id + route-level gating) ──→ Handler (endpoint-level check) ──→ Response
          │
          └──→ Heartbeat MW (async touch, independent liveness concern)
```

The access MW and heartbeat MW are **independent layers**. The heartbeat MW
must NOT be in the critical path to the handler.

##### Route Classification — Three Enforcement Tiers

**Tier 1 — Management routes (no `{serial}`):**

Routes: `/server/*`, `/devices` (list all), `/resources/*`, `/users/*`,
`/events/*`, `/disconnect`, `/heartbeat`.

| Level | Behavior |
|-------|----------|
| `off` | Pass through. No checks. |
| `warn` | Validate `X-User-Id` is present. Log warning if absent. Allow. |
| `enforce` | Validate `X-User-Id` is present. Reject with 403 if absent. |

> **Note**: `warn` and `enforce` enable **user identity enforcement only** on
> management routes — no restrictions on which management operations the user
> may perform. Any authenticated user may call any management endpoint.

**Tier 2 — Device-level routes (have `{serial}`, but not data port):**

Routes: `/devices/{serial}` (info), `/devices/{serial}/configure`,
`/devices/{serial}/settings/*`, `/devices/{serial}/sensors`,
`/devices/{serial}/syzygy`.

| Level | Behavior |
|-------|----------|
| `off` | Pass through. |
| `warn` | Validate `X-User-Id` is present. No resource check. |
| `enforce` | Validate `X-User-Id` is present. No resource check. |

> **Note**: Device info, configuration, settings, sensors, and SYZYGY
> operations do not require resource ownership. Per §10, `ConfigureFPGA`
> is explicitly excluded from endpoint enforcement (resources cannot exist
> until the FPGA is programmed). `warn` and `enforce` enable **user identity
> enforcement only** on these routes.

**Tier 3 — Data port routes (have `{serial}` + Classic or AXI path):**

Routes: `/devices/{serial}/classic/**`, `/devices/{serial}/axi/**`.

| Level | Behavior |
|-------|----------|
| `off` | Pass through. |
| `warn` | MW validates `X-User-Id`, checks `check_device_access()`. Logs violation if denied. Passes to handler. Handler checks endpoint coverage, logs if denied, allows. |
| `enforce` | MW validates `X-User-Id`, checks `check_device_access()`. Rejects with 403 if denied. Handler checks endpoint coverage. Rejects with 403 if denied. |

The middleware performs a **device-level gate** — does the user hold ANY
resource on this device? This is a fast, lightweight check that rejects
obviously unauthorized requests before the handler runs.

The handler then performs the **endpoint-level check** — does the user's
resource cover the specific endpoint or address being accessed? This check
requires the deserialized request body (endpoint address from JSON body or
query params), which is only available inside the handler.

##### REST Handler Enforcement Helper

Each Classic and AXI handler calls a shared helper function at the top of
the handler, after extracting the endpoint address from the request:

```rust
/// Check endpoint access for a Classic operation.
/// Called inside each handler after extracting endpoint from query/body.
fn enforce_endpoint(
    state: &AppState,
    user_id: Option<&str>,
    serial: &str,
    endpoint: u8,
    ep_type: &EndpointType,
) -> Result<(), ProblemDetail> { ... }
```

#### gRPC: In-Handler Enforcement

gRPC enforcement is performed inside each RPC method after deserializing the
request message. This matches the REST handler approach — the check happens
at the same logical point (after deserialization, before the FFI call), and
provides the same three-level enforcement behavior.

```
Request ──→ RPC method (deserialize → check_access() → FFI call) ──→ Response
```

Each `ClassicDataPortService` and `AXIDataPortService` RPC method calls a
shared helper:

```rust
fn check_access(
    &self,
    serial: &str,
    user_id: Option<&str>,
    endpoint: u8,
    ep_type: &EndpointType,
) -> Result<(), Status> { ... }
```

User identity is extracted from gRPC metadata (`x-user-id` key). Denied
operations return `Status::permission_denied()` (in `enforce` mode) or log
a warning (in `warn` mode).

> **Note**: The gRPC `Ping` RPC (heartbeat) is in `ServerService`, not in
> `ClassicDataPortService` or `AXIDataPortService`. It is naturally decoupled
> from endpoint access enforcement.

#### Core: New Registry Methods

```rust
impl ResourceRegistry {
    /// Check if a user is authorized to access a specific endpoint on a device.
    ///
    /// Returns Ok(()) if authorized, or Err(reason) if denied.
    pub fn check_endpoint_access(
        &self,
        user_id: &str,
        device_serial: &str,
        endpoint: u8,
        ep_type: &EndpointType,
    ) -> Result<(), AccessDenied> {
        // 1. Find resources on this device held by this user
        // 2. Check endpoint coverage (§2 authorization rule)
        // 3. Delegate to policy.validate_endpoint_access() for custom logic
    }

    /// Same for AXI address access.
    pub fn check_address_access(
        &self,
        user_id: &str,
        device_serial: &str,
        address: u64,
    ) -> Result<(), AccessDenied> { ... }

    /// Device-level check: does the user hold ANY resource on this device?
    /// Used by the REST middleware for Tier 3 early gating.
    pub fn check_device_access(
        &self,
        user_id: &str,
        device_serial: &str,
    ) -> Result<(), AccessDenied> { ... }
}
```
```

### 5. Enforcement Scope: Command-Level, Not Transfer-Level

Enforcement applies at the **command initiation** level, not at the per-byte
transfer level. Once a command is authorized and initiated, it runs to completion
without further per-transfer user checks:

| Operation | Enforcement point | Transfer phase (no further checks) |
|-----------|------------------|-------------------------------------|
| `SetWireInValue` | Before the call | N/A (atomic) |
| `ActivateTriggerIn` | Before the call | N/A (atomic) |
| `WriteToPipeIn` / `WriteToBlockPipeIn` | Before starting the transfer | Bytes stream to FPGA without per-byte checks |
| `ReadFromPipeOut` / `ReadFromBlockPipeOut` | Before starting the transfer | Bytes stream from FPGA without per-byte checks |
| AXI burst read/write | Before initiating the burst | Burst completes without per-word checks |
| AXI Stream | Before starting the stream | Stream runs without per-packet checks |

This ensures that enforcement adds no latency to high-throughput data paths. The
authorization check happens once when the operation begins; the data transfer
itself proceeds at full speed.

### 6. Lock Enforcement

When a shared resource is locked (state `InUseLocked`):

- **Lock holder**: Full access to all endpoints covered by the resource
- **Other holders**: **Denied** — a locked resource can only be used by the user
  who holds the lock. Other holders remain in the `holders` list but their
  operations are rejected until the resource is unlocked
- **Non-holders**: Denied (same as without a lock)

This makes `Lock` semantically meaningful at the FPGA operation level, not just at
the resource metadata level.

### 7. Error Responses

#### REST

```json
HTTP 403 Forbidden
Content-Type: application/problem+json

{
  "type": "https://okfpga.dev/problems/access-denied",
  "title": "Endpoint access denied",
  "status": 403,
  "detail": "User 'Bob' does not hold a resource covering endpoint 0x00 (wire_in) on device '22520013CC'",
  "instance": "/api/v1/devices/22520013CC/classic/wires/in"
}
```

#### gRPC

```
Status: PERMISSION_DENIED
Message: "User 'Bob' does not hold a resource covering endpoint 0x00 (wire_in) on device '22520013CC'"
```

### 8. Dashboard / Observability

When enforcement is `warn` or `enforce`:

- Violations are logged at `WARN` level with structured fields: `user_id`,
  `device_serial`, `endpoint`, `action`, `denied_reason`
- The dashboard log panel already shows server logs — violations will appear
  automatically
- A future dashboard "Access Audit" panel could aggregate these events

### 9. Configuration

```toml
[access_control]
# Enforcement level: "off", "warn", "enforce"
endpoint_enforcement = "off"
```

This is a runtime-reloadable setting (server watches config file). Changing from
`off` → `warn` → `enforce` does not require a restart.

### 10. Note on ConfigureFPGA (Device Programming)

`ConfigureFPGA` (programming the device) does **not** fall under endpoint access
enforcement. Resources cannot exist until the FPGA is programmed, so there is no
resource to hold before programming. Any connected user may program any device.

> **Future work**: When user authority levels are introduced (e.g., Admin vs Normal
> user), the ability to program devices may be restricted by role. This is outside
> the scope of endpoint access enforcement and would be handled by a separate
> role-based authorization system.

---

## Consequences

### Positive

- **Meaningful resource locking** — Lock/Unlock actually prevents other users from
  touching the hardware, not just updating metadata
- **Audit trail** — `warn` mode provides visibility into who accesses what without
  breaking existing workflows
- **Gradual rollout** — `off` → `warn` → `enforce` migration path lets teams adopt
  incrementally
- **Builds on existing code** — `ResourceAccessPolicy` trait,
  `validate_endpoint_access()` stubs, and `ResourceRegistry` are already in place
- **Custom policies** — developers can implement `ResourceAccessPolicy` for
  HDL-specific logic (e.g., some endpoints are always readable)
- **Zero transfer overhead** — enforcement is command-level only; high-throughput
  data paths (pipes, AXI bursts, streams) are unaffected after initial authorization

### Negative

- **Per-command overhead** — every Classic/AXI command initiation gains a registry
  lookup (expected to be negligible with in-memory registry and <100 resources)
- **Client migration** — existing clients that don't acquire resources will break in
  `enforce` mode (mitigated by default `off` and `warn` migration period)
- **Handler modification** — each Classic and AXI handler must call the enforcement
  helper at the top. This is simple (one function call) but requires updating every
  handler. The benefit is that the handler has full deserialized context, avoiding
  the complexity of parsing endpoint addresses from raw HTTP requests in middleware

### Future Work

- **Read-only resource acquisition** — a new `AccessMode::ReadOnly` that allows
  `GetWireOutValue`, `UpdateWireOuts`, `GetTriggerOutVector`, `ReadFromPipeOut` etc.
  but denies all write operations. This gives "observe but don't touch" access
  without full acquire
- ~~**Address range enforcement for Classic endpoints and okRegisterBridge**~~ —
  **Implemented → [ADR-025](ADR-025-classic-address-mapping.md)**.
  Developer-defined custom stubs (`resolve_classic_address()`) allow a developer
  with knowledge of the HDL design to map Classic endpoint operations to
  memory-mapped addresses for enforcement. okRegisterBridge direct checking was
  already wired in Phase 28 (`enforce_address()` in REST/gRPC register handlers).
  See also: [Developer Guide](../../guides/classic-address-mapping.md).
- **Dashboard "Access Audit" panel** — visualize who accessed what
- **Per-resource enforcement override** — allow individual resources to opt out of
  enforcement
- **Role-based device programming** — restrict `ConfigureFPGA` to Admin-level users
  when user authority levels are defined
- **Rate limiting per user** — combine with access control for DoS protection

---

## Amendments

### To ADR-020

- §7 "No runtime enforcement (v1)" is superseded by this ADR's enforcement model
- The `ResourceAccessPolicy::validate_endpoint_access()` and
  `validate_address_access()` stubs become active call sites (not just
  documentation)

### To ADR-004 / ADR-005

- No new endpoints are added — enforcement is middleware-level (REST) and
  in-handler (both REST and gRPC), not API-level
- REST: lightweight access middleware on all `/api/v1/*` routes provides
  user identity enforcement (Tiers 1–2) and device-level gating (Tier 3);
  endpoint-level checks are inside handlers
- gRPC: enforcement is inside each RPC method after deserialization, using
  `x-user-id` metadata for identity
- New error responses: `403 Forbidden` / `PERMISSION_DENIED` on Classic and AXI
  endpoints when `endpoint_enforcement` is `"enforce"`

## References

- [ADR-006 — Multi-User Resource Management](ADR-006-resource-management.md)
- [ADR-020 — Client-Side Resource Registration](ADR-020-client-side-resource-registration.md) (§7: "No runtime enforcement")
- [ADR-023 — GUI Example Clients](ADR-023-gui-example-clients.md) (§F11: origin of this ADR)

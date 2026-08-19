# ADR-024: Endpoint Access Enforcement

**Status**: Accepted  
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

#### REST: Axum Middleware Layer

A new middleware layer sits between the heartbeat middleware and the Classic/AXI
route handlers:

```
Request → Heartbeat MW (extracts user_id) → Access MW (checks resource) → Handler
```

The middleware intercepts routes matching:
- `/devices/{serial}/classic/wires/**`
- `/devices/{serial}/classic/triggers/**`
- `/devices/{serial}/classic/pipes/**`
- `/devices/{serial}/axi/**`

It extracts `{serial}` from the path and `endpoint` from query params or JSON body,
then calls `ResourceRegistry::check_endpoint_access()`.

#### gRPC: Interceptor

A tonic interceptor on `ClassicDataPortService` and `AXIDataPortService` performs
the same check using the `serial` and `endpoint` fields from the request message
and `user_id` from gRPC metadata.

#### Core: New Registry Method

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
        // 2. Check endpoint coverage
        // 3. Delegate to policy.validate_endpoint_access() for custom logic
    }

    /// Same for AXI address access.
    pub fn check_address_access(
        &self,
        user_id: &str,
        device_serial: &str,
        address: u64,
    ) -> Result<(), AccessDenied> { ... }
}
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
- **Complexity** — the middleware must parse endpoint addresses from heterogeneous
  request formats (JSON body, query params, path segments)

### Future Work

- **Read-only resource acquisition** — a new `AccessMode::ReadOnly` that allows
  `GetWireOutValue`, `UpdateWireOuts`, `GetTriggerOutVector`, `ReadFromPipeOut` etc.
  but denies all write operations. This gives "observe but don't touch" access
  without full acquire
- **Address range enforcement for Classic endpoints and okRegisterBridge** —
  ADR-020's `address_ranges` field is not exclusive to AXI (`FPGADataPortAXI`)
  endpoints. Classic (`FPGADataPortClassic`) designs may also expose a
  memory-mapped address space accessed via wires, triggers, and pipes in a
  design-specific manner — or via the `okRegisterBridge` module. Since this
  generic server cannot know the specifics of an arbitrary HDL design, two
  mechanisms should be provided:
  1. **Developer-defined custom stubs** — extend `ResourceAccessPolicy` with a
     hook that lets the developer specify how their Classic FrontPanel endpoints
     map to address ranges (e.g., "WireIn 0x01 selects the address, PipeIn 0x80
     writes data at that address"). The server calls this stub before Classic
     operations, passing the endpoint and the resource's declared address ranges.
  2. **okRegisterBridge direct checking** — when a resource declares that it uses
     `okRegisterBridge`, the server can directly intercept register read/write
     calls and validate the target register address against the resource's
     declared `address_ranges`. If any range is `exclusive`, operations targeting
     addresses within that range are denied for non-holders.
  This enforcement is only necessary when a resource registers an address range
  **and** at least one range is declared `exclusive`.
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

- No new endpoints are added — enforcement is middleware-level, not API-level
- New error responses: `403 Forbidden` / `PERMISSION_DENIED` on Classic and AXI
  endpoints when `endpoint_enforcement` is `"enforce"`

## References

- [ADR-006 — Multi-User Resource Management](ADR-006-resource-management.md)
- [ADR-020 — Client-Side Resource Registration](ADR-020-client-side-resource-registration.md) (§7: "No runtime enforcement")
- [ADR-023 — GUI Example Clients](ADR-023-gui-example-clients.md) (§F11: origin of this ADR)

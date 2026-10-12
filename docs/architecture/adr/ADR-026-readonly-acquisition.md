# ADR-026: Read-Only Resource Acquisition & Private Data

**Date:** 2026-08-20  
**Status:** Accepted ✅ — Implemented (Phase 30, commits `94a4a4b`, `fd46b97`)  
**Extends:** [ADR-024](ADR-024-endpoint-access-enforcement.md), [ADR-025](ADR-025-classic-address-mapping.md)

---

## Context

ADR-024 established endpoint-level access enforcement: when a user operates on a
Classic endpoint or AXI address, the server checks that the user holds a resource
covering that endpoint/address. However, the only acquisition modes are `Shared`
(multiple holders, full read+write) and `Exclusive` (sole holder, full read+write).

There is no way for a user to *observe* a resource's state without having the
ability to *modify* it. This is a common need:

- A monitoring dashboard displaying live counter values
- A supervisor observing a calibration run without risk of interference
- A student observing a lab demo without accidentally resetting the FPGA state

### Related ADRs

- **ADR-024** — endpoint access enforcement pipeline (upstream checks)
- **ADR-025** — `resolve_classic_address()` hook for Classic → address mapping
- **ADR-020** — resource registration with `address_ranges` and `endpoints`

---

## Decision

### 1. New Acquisition Mode: `ReadOnly`

Add `ReadOnly` to the existing `AccessMode` enum:

```rust
pub enum AccessMode {
    Shared,
    Exclusive,
    ReadOnly,  // NEW: observe-only, no writes, no system disturbance
}
```

A `ReadOnly` holder can:
- Read cached wire-out values (`GetWireOutValue`)
- Read cached trigger-out values (`IsTriggered`, `GetTriggerOutVector`)
- Read from pipes (`ReadFromPipeOut`, `ReadFromBlockThrottledPipeOut`)
- Read AXI addresses (if not Private)
- Read register bridge addresses (if not Private)

A `ReadOnly` holder **cannot**:
- Write to any input endpoint (WireIn, TriggerIn, PipeIn, BtPipeIn)
- Call `UpdateWireOuts` or `UpdateTriggerOuts` — these cause the FPGA to latch
  output values, which disturbs the system state
- Write to any AXI address or register bridge address
- Lock a resource
- Access any Private endpoint, address range, or resource

### 2. The Private Concept — Three Levels

"Private" is a boolean flag that restricts ReadOnly access. It operates at three
hierarchical levels:

#### Level 1: Whole Resource

```rust
pub struct Resource {
    // ... existing fields ...
    pub private: bool,  // NEW: if true, ReadOnly acquisition is denied entirely
}
```

If a Resource is marked `private: true`, no user can acquire it as `ReadOnly`.
The acquire attempt returns `AcquisitionDenied::PrivateResource`.

#### Level 2: Per-Endpoint

```rust
pub struct EndpointEntry {
    pub address: u8,
    pub ep_type: EndpointType,
    pub access: AccessMode,
    pub description: Option<String>,
    pub private: bool,  // NEW: if true, ReadOnly holders cannot access this EP
}
```

Even if the resource is not private, individual endpoints can be marked private.
A ReadOnly holder accessing a private endpoint gets `AccessDenied::PrivateDataDenied`.

#### Level 3: Per-Address Range

```rust
pub struct AddressRange {
    pub start: u32,
    pub end: u32,
    pub access: AccessMode,
    pub private: bool,  // NEW: if true, ReadOnly holders cannot read this range
}
```

Private address ranges deny ReadOnly access for:
- Classic endpoints that resolve to the range via `resolve_classic_address()` (ADR-025)
- AXI reads to addresses within the range
- Register bridge reads to addresses within the range

### 3. Holder Tracking

To enforce ReadOnly restrictions at operation time, the server must know *how* each
user acquired the resource. The `holders` field changes from a simple user ID list
to a list of holder entries:

```rust
pub struct HolderEntry {
    pub user_id: UserId,
    pub mode: AccessMode,  // Shared, Exclusive, or ReadOnly
}
```

### 4. Acquisition Rules

| Resource State | Request Mode | Result |
|---------------|-------------|--------|
| Available | ReadOnly | ✅ Allow (unless resource is Private) |
| Available | Shared | ✅ Allow (existing) |
| Available | Exclusive | ✅ Allow (existing) |
| InUseShared | ReadOnly | ✅ Allow (unless resource is Private) |
| InUseShared | Shared | ✅ Allow (existing) |
| InUseShared | Exclusive | ❌ Deny (existing) |
| InUseLocked | ReadOnly | ✅ Allow (ReadOnly doesn't interfere) |
| InUseLocked (Exclusive) | ReadOnly | ✅ Allow (unless resource is Private) |
| InUseLocked (Exclusive) | Shared | ❌ Deny (existing) |

Additional acquisition-time checks:
- If all endpoints in the resource are input-only (WireIn, TriggerIn, PipeIn,
  BtPipeIn), deny ReadOnly acquisition — there is nothing to observe
- If the resource is marked `private: true`, deny ReadOnly acquisition

### 5. Enforcement Pipeline

The enforcement pipeline from ADR-024 is extended with two new check stages
that fire **after** the existing checks pass:

```
Request arrives
  │
  ├── [ADR-024] User has identity? ──────────── NO → deny
  ├── [ADR-024] User holds resource on device? ─ NO → deny
  ├── [ADR-024] Resource locked by other + WRITE? ── YES → deny (skip; reads allowed†)
  ├── [ADR-024] Endpoint covered by resource? ── NO → deny
  ├── [ADR-025] resolve_classic_address() hook
  │     ├── NoMapping → continue
  │     ├── AllowAlways → continue
  │     └── MapsToAddress(addr) → check address ranges
  │
  ├── [ADR-026] Holder mode is ReadOnly?  ← NEW
  │     ├── Endpoint is input? (WireIn, TriggerIn, PipeIn, BtPipeIn) → DENY
  │     ├── Operation is UpdateWireOuts or UpdateTriggerOuts? → DENY
  │     ├── Endpoint is marked private? → DENY
  │     ├── Resolved address range is marked private? → DENY
  │     └── Operation is AXI/register write? → DENY
  │
  └── ALLOW
```

†Note: ReadOnly holders are not blocked by the "locked by other" check for
**read** operations. They ARE blocked if the resource-level `private` flag
is set (checked at acquisition time, not at operation time).

> **Implemented (Phase 33):** The lock bypass described here is now
> implemented in `check_endpoint_access()` and `check_address_access()`.
> Additionally, the bypass was extended to **all holders** (not just
> ReadOnly) — any holder can read output endpoints when a resource is locked
> by another user. Only write operations (input endpoints) are blocked by
> the lock. See ADR-024 §6 amendment.

### 6. Data Freshness for ReadOnly Users

ReadOnly users cannot call `UpdateWireOuts` or `UpdateTriggerOuts` because these
operations disturb the system by causing the FPGA to latch output values. This
means:

- ReadOnly users see **cached** wire-out and trigger-out values
- The cache is refreshed only when a ReadWrite/Exclusive holder calls
  `UpdateWireOuts` or `UpdateTriggerOuts`
- If a ReadOnly user is the sole holder, they will see stale or zero values

This is a deliberate design choice: a truly passive observer should not cause any
bus transactions on the FPGA. The intended use case is monitoring alongside an
active ReadWrite user.

### 7. API Changes

#### OpenAPI (`api/openapi.yaml`)

```yaml
# Acquire endpoint request body
mode:
  type: string
  enum: [shared, exclusive, read_only]
```

Add `private` boolean field (default `false`) to:
- Resource schema
- EndpointEntry schema
- AddressRange schema

#### Proto3 (`proto/okfpga/v1/okfpga.proto`)

```protobuf
enum AccessMode {
  ACCESS_MODE_SHARED = 0;
  ACCESS_MODE_EXCLUSIVE = 1;
  ACCESS_MODE_READ_ONLY = 2;  // NEW
}
```

Add `bool private` field to:
- Resource message
- EndpointEntry message
- AddressRange message

### 8. Error Types

Two new `AccessDenied` variants:

```rust
/// The user acquired in ReadOnly mode but attempted a write operation.
ReadOnlyWriteAttempt {
    user_id: String,
    resource_id: String,
    detail: String,  // e.g., "WireIn 0x01", "UpdateWireOuts", "AXI write 0x44A60000"
}

/// The data is marked private and the user is a ReadOnly holder.
PrivateDataDenied {
    user_id: String,
    resource_id: String,
    detail: String,  // e.g., "endpoint 0x00 (wire_out)", "address range 0x0000–0x00FF"
}
```

One new `AcquisitionDenied` variant:

```rust
/// The resource is marked private — ReadOnly acquisition is denied.
PrivateResource {
    resource_id: String,
}
```

---

## Files Changed

| File | Change |
|------|--------|
| `crates/ok-fpga-core/src/resource.rs` | `ReadOnly` variant, `HolderEntry`, `private` fields, enforcement, 14 new tests |
| `crates/ok-fpga-server/src/management.rs` | `HolderEntry` → string serialization in management handler |
| `crates/ok-fpga-server/src/rest/resources.rs` | Accept `read_only` mode in `parse_access_mode()`, `private: false` default |
| `crates/ok-fpga-server/src/grpc/device_service.rs` | Accept `ACCESS_MODE_READ_ONLY`, `HolderEntry` → string serialization |
| `crates/ok-fpga-server/src/sweep.rs` | Use `has_holder()` instead of `holders.contains()` |
| `api/openapi.yaml` | `read_only` enum value, `access_mode` + `private` fields on `Resource` schema |
| `proto/okfpga/v1/okfpga.proto` | `ACCESS_MODE_READ_ONLY = 2`, `bool private = 7` on `Resource` message |
| `client-examples/rust-rest-gui-common/src/lib.rs` | `acquire_resource_readonly()` method |
| `client-examples/rust-grpc-gui-common/src/lib.rs` | `acquire_resource_readonly()` method |
| `client-examples/python_rest_gui_common/counters_client.py` | `acquire_resource_readonly()` method |
| `client-examples/python_grpc_gui_common/counters_grpc_client.py` | `acquire_resource_readonly()` method |
| All 8 GUI demos (`main.rs` / `main.py`) | "Acquire R/O" button, ReadOnly lockout + banner |

---

## Future Work

- **ReadOnly dashboard indicator** — display a visual badge when a user is in
  ReadOnly mode (e.g., a lock icon with an eye overlay)
- **Auto-refresh for ReadOnly** — a ReadWrite holder could opt into broadcasting
  fresh values to ReadOnly observers via a pub/sub mechanism
- **Per-endpoint read-only override** — allow marking individual output endpoints
  as "always allow ReadOnly" even on a Private resource, for public status
  registers

---

## Addendum — Implementation Notes (2026-08-20, `fd46b97`)

**Commit:** `94a4a4b` (docs), `fd46b97` (code + specs + GUI demos)

### Deviations from Original Design

#### §4 Input-Only Endpoint Guard — Not Implemented

The original design specified:

> If all endpoints in the resource are input-only (WireIn, TriggerIn, PipeIn,
> BtPipeIn), deny ReadOnly acquisition — there is nothing to observe

This check was **not implemented** in `can_acquire()`. The rationale:

1. Resources may also have `address_ranges` with observable data even when all
   Classic endpoints are input-only.
2. A resource with no endpoints at all (possible during early registration) would
   be incorrectly denied.
3. Denying at acquisition time based on endpoint composition is fragile — the
   server can instead rely on operation-time enforcement (`check_endpoint_access()`
   denies ReadOnly writes to input endpoints).

If this guard is desired in the future, add an `all_input_only()` predicate to
`Resource` and call it from `DefaultAccessPolicy::can_acquire()`.

#### §7 API Schemas — EndpointEntry / AddressRange Not in Specs

The original design specified adding `private` to the `EndpointEntry` and
`AddressRange` schemas in the OpenAPI and Proto3 specs. However:

- Neither the OpenAPI spec nor the Proto3 spec defines `EndpointEntry` or
  `AddressRange` as standalone schemas. These are **internal Rust structs** in
  `ok-fpga-core/src/resource.rs`, not externally-facing API types.
- `private` was added to the **`Resource` schema** in both specs (the only
  externally-visible type).
- Per-endpoint and per-address-range privacy is enforced internally by the
  core registry and is not exposed through the API at this time.

#### Files Changed — Corrections

The original Files Changed table listed `rest/classic.rs` and
`grpc/classic_service.rs` as modified. These files were **not** changed in
Phase 30 — ReadOnly enforcement is handled entirely within the core registry's
`check_endpoint_access()` and `check_address_access()` methods, which the
server handlers call. The original table also listed `grpc/resource_service.rs`,
which does not exist; the correct file is `grpc/device_service.rs`.

The corrected table above reflects the actual files modified in commit `fd46b97`.


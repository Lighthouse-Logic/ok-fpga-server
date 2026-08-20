# ADR-025: Developer-Defined Classic Address Mapping

**Status**: Accepted  
**Date**: 2026-08-19  
**Deciders**: Architecture Team  
**Depends on**: [ADR-006](ADR-006-resource-management.md) (Resource Management),
[ADR-020](ADR-020-client-side-resource-registration.md) (Client-Side Resource Registration),
[ADR-024](ADR-024-endpoint-access-enforcement.md) (Endpoint Access Enforcement)

## Context

### The Gap After ADR-024

ADR-024 introduced endpoint access enforcement with two levels:

1. **Classic endpoint numbers** (0x00–0xFF) — `check_endpoint_access()` verifies
   that the user holds a resource covering the requested endpoint
2. **AXI / Register Bridge addresses** — `check_address_access()` verifies that
   the user holds a resource covering the requested memory-mapped address

However, many Classic FPGA designs implement **internal memory-mapped address
spaces** that are accessed indirectly through Classic endpoints. The server sees
only the endpoint number (e.g., "WireIn 0x01") — it has no way to know that the
value being written to that endpoint is actually a 32-bit address targeting a
specific memory region inside the FPGA.

### Real-World Example

Consider a Classic HDL design with an internal memory controller:

```
┌─────────────────────────────────────────────┐
│  FPGA: my_memory_controller.bit             │
│                                             │
│  WireIn 0x01  ──→ Address Register          │
│  WireIn 0x02  ──→ Write Data Register       │
│  TriggerIn 0x40 ──→ Write Strobe            │
│  WireOut 0x20  ←── Read Data Register       │
│                                             │
│  Internal address space:                    │
│    0x0000–0x00FF  DAC control (exclusive)   │
│    0x0100–0x01FF  Status registers (shared) │
│    0x0200–0x02FF  ADC readback (shared)     │
└─────────────────────────────────────────────┘
```

With ADR-024 alone, endpoint enforcement can check whether the user holds a
resource covering WireIn 0x01 — but it cannot know that writing `0x0042` to
WireIn 0x01 targets address `0x0042` in the exclusive DAC control range.

This same pattern applies to any Classic design that creates an internal bus:
- Custom AXI buses not connected directly by FrontPanel
- RAM banks accessed via wire/trigger/pipe sequences
- FIFO controllers with addressable channels
- Any design where Classic endpoints serve as an indirect addressing mechanism

The `okRegisterBridge` module is a specific case of this pattern, but register
bridge operations already have direct `ReadRegister`/`WriteRegister` REST and
gRPC endpoints that call `check_address_access()`. The gap is for **arbitrary
Classic designs** where the developer uses raw wires, triggers, and pipes to
implement their own addressing scheme.

### Key Insight

Only the developer who wrote the HDL knows how Classic endpoints map to
memory-mapped addresses. The server cannot discover this mapping automatically.
A **developer-defined hook** is needed where the developer expresses the mapping
in code that runs inside the server's enforcement pipeline.

---

## Decision

### 1. New Trait Method: `resolve_classic_address()`

Add a method to the existing `ResourceAccessPolicy` trait that the developer
overrides to map Classic endpoint operations to memory-mapped addresses:

```rust
pub enum ClassicAddressResolution {
    /// No address mapping — use standard endpoint-level enforcement.
    NoMapping,
    /// This operation targets the given address — enforce via address ranges.
    MapsToAddress(u64),
    /// Infrastructure endpoint — any holder may use, skip address check.
    AllowAlways,
}

fn resolve_classic_address(
    &self,
    resource: &Resource,
    user: &UserId,
    endpoint: u8,
    ep_type: &EndpointType,
    value: Option<u32>,
) -> ClassicAddressResolution {
    ClassicAddressResolution::NoMapping  // Default: no mapping
}
```

### 2. Firing Order: After Upstream Checks

The hook fires **only after** the existing enforcement pipeline has verified:

1. ✅ User is authenticated
2. ✅ User holds a resource on the device
3. ✅ Resource is not locked by another user (or is locked by this user)
4. ✅ The resource's endpoint declarations cover the requested endpoint

Only then does the server call `resolve_classic_address()`. This means:

- The developer **never** needs to check authentication, acquisition, or lock state
- The developer's only responsibility is: *"given this endpoint and value, what
  address is being targeted?"*
- The server handles all enforcement decisions based on the hook's return value

### 3. Address Range Enforcement After Resolution

When the hook returns `MapsToAddress(addr)`, the server checks the resolved
address against the resource's declared `address_ranges`:

| Address in range? | Range access mode | Resource lock state | Result |
|-------------------|-------------------|---------------------|--------|
| Not in any range | — | — | **DENY** (address not covered) |
| In a Shared range | `Shared` | — | **ALLOW** |
| In an Exclusive range | `Exclusive` | Locked by this user | **ALLOW** |
| In an Exclusive range | `Exclusive` | Not locked / locked by other | **DENY** |

This enables the critical pattern: **shared endpoints gating exclusive address
ranges**. A user needs Shared access to the wire endpoint (to physically set the
address value) while enforcement happens at the _resolved address_ level.

### 4. The `value` Parameter

The `value` parameter provides the data needed for address resolution:

| Endpoint Type | `value` contents | Rationale |
|---------------|------------------|-----------|
| WireIn | `Some(value)` — the u32 being written | Address is typically in the wire value |
| TriggerIn | `Some(mask)` — the trigger mask | Mask identifies which trigger bit fires |
| WireOut | `None` | Read — value not known before FFI call |
| TriggerOut | `None` | Read — value not known before FFI call |
| PipeIn / PipeOut | `None` | Byte stream, not a single address |

For operations where `value` is `None`, the developer's hook can return
`NoMapping` to fall back to standard endpoint enforcement, or use device state
(which the developer's custom policy code has access to) to make a decision.

### 5. Updated Enforcement Flow

```
Client: SetWireInValue(device="ABC123", endpoint=0x01, value=0x0042)
                                    │
                                    ▼
         ┌─────────────────────────────────────────────┐
         │ 1. enforce_endpoint() — handler level       │
         │    Extracts user_id, serial, endpoint, value │
         └──────────────────────┬──────────────────────┘
                                │
                                ▼
         ┌─────────────────────────────────────────────┐
         │ 2. check_endpoint_access()                  │
         │    a. Find resources on device held by user  │
         │    b. Lock check passes                      │
         │    c. Endpoint coverage check passes         │
         └──────────────────────┬──────────────────────┘
                                │ (upstream checks passed)
                                ▼
         ┌─────────────────────────────────────────────┐
         │ 3. policy.resolve_classic_address()          │
         │    Developer's hook: "EP 0x01 + value 0x0042 │
         │    → MapsToAddress(0x0042)"                  │
         └──────────────────────┬──────────────────────┘
                                │
                     ┌──────────┴──────────┐
                     │                     │
                  NoMapping          MapsToAddress(0x0042)
                     │                     │
                     ▼                     ▼
                   Ok(())      ┌───────────────────────┐
                               │ 4. Check 0x0042 vs    │
                               │    resource address   │
                               │    ranges             │
                               └───────────┬───────────┘
                                           │
                                ┌──────────┴──────────┐
                                │                     │
                          In Shared range     In Exclusive range
                                │                     │
                                ▼                     ▼
                              Ok(())        User holds lock?
                                            ┌────┴────┐
                                            │         │
                                           Yes        No
                                            │         │
                                            ▼         ▼
                                          Ok(())    DENY
                                                   (403 / PERMISSION_DENIED)
```

### 6. Backward Compatibility

The default implementation of `resolve_classic_address()` returns `NoMapping`,
which means:

- **Existing code**: All current behavior is preserved. No custom policy
  implements the hook → no address resolution occurs → standard endpoint
  enforcement applies exactly as before.
- **`DefaultAccessPolicy`**: Inherits the default `NoMapping` return — zero
  behavior change.
- **Register Bridge operations**: Continue to use `enforce_address()` /
  `check_address_access()` directly (not through this hook). The hook is for
  Classic endpoint → address mapping, not for operations that already have
  explicit address parameters.

---

## Consequences

### Positive

- **Developer empowerment** — developers with HDL knowledge can express their
  design's addressing scheme without modifying server source code
- **Zero regression** — default `NoMapping` preserves all existing behavior
- **Simplified developer interface** — upstream checks handle auth, acquisition,
  and lock enforcement; the developer's hook is pure address mapping
- **Shared endpoint / exclusive address** — enables fine-grained access control
  where physical endpoints are shared but address ranges are exclusive
- **Reuses existing infrastructure** — `address_ranges` and `AddressRange` types
  from ADR-020 are already on the `Resource` struct

### Negative

- **Per-endpoint `value` overhead** — every Classic endpoint enforcement call now
  passes an `Option<u32>` value. Negligible cost (it's a stack value).
- **Developer effort** — requires the developer to implement a custom
  `ResourceAccessPolicy` and understand their HDL's addressing scheme. Mitigated
  by comprehensive documentation (see Developer Guide).
- **Write-only enforcement** — for WireOut/PipeOut reads where `value` is `None`,
  the hook cannot determine the target address. The developer can use device state
  or return `NoMapping` to allow reads through standard endpoint enforcement.

### Future Work

- **Read-value post-check** — after a WireOut/PipeOut read completes, the server
  could call a post-read hook with the actual value for audit logging
- **Declarative mapping in TOML** — for simple designs (e.g., "WireIn 0x01 is
  always the address register"), allow the mapping to be expressed in config
  rather than code
- **Per-range lock semantics** — instead of requiring a full resource lock for
  exclusive ranges, allow per-range locking (more granular but more complex)

---

## Files Changed

| File | Change |
|------|--------|
| `crates/ok-fpga-core/src/resource.rs` | `ClassicAddressResolution` enum, `resolve_classic_address()` on trait, `ExclusiveAddressRange` on `AccessDenied`, updated `check_endpoint_access()` |
| `crates/ok-fpga-server/src/rest/classic.rs` | `enforce_endpoint()` gains `value: Option<u32>`; each handler passes its value |
| `crates/ok-fpga-server/src/grpc/classic_service.rs` | `check_access()` gains `value: Option<u32>`; each handler passes its value |
| `docs/guides/classic-address-mapping.md` | **[NEW]** Standalone developer guide |
| `docs/architecture/adr/ADR-025-classic-address-mapping.md` | **[NEW]** This ADR |
| `docs/architecture/overview.md` | ADR-025 added to index |
| `docs/architecture/adr/ADR-024-endpoint-access-enforcement.md` | Future Work cross-reference |
| `docs/development-roadmap.md` | Phase 29 promoted to Now |

---

## Developer Guide

See [Classic Address Mapping — Developer Guide](../../guides/classic-address-mapping.md)
for a complete implementation walkthrough with realistic examples.

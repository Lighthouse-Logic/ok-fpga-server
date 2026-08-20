# Roadmap Archive — Phase 29

## Phase 29 — Developer-Defined Classic Address Mapping (ADR-025)

**Status:** ✅ Completed 2026-08-20  
**ADR:** [ADR-025 — Developer-Defined Classic Address Mapping](../architecture/adr/ADR-025-classic-address-mapping.md)  
**Developer Guide:** [Classic Address Mapping](../guides/classic-address-mapping.md)  
**Commits:** `c048279` (docs), `615e854` (code)

### Focus

Provide developer-defined custom stubs (`ResourceAccessPolicy` hooks) that allow a
developer with knowledge of their HDL design to enforce access control on Classic
endpoint operations that indirectly address a memory-mapped space. This enables
shared endpoints (wires, triggers, pipes) to gate exclusive address ranges declared
by resources.

### Context

Many Classic FPGA designs implement internal memory-mapped address spaces accessed
indirectly through Classic endpoints (WireIns carrying addresses, TriggerIns firing
write strobes, etc.). These designs may use custom HDL logic, internal AXI buses,
RAM banks, FIFOs, or the `okRegisterBridge` module. The server cannot know how an
arbitrary HDL design maps endpoints to addresses, so a developer-defined hook is
provided.

### Deliverables

* [x] **ADR-025** — new ADR documenting the `resolve_classic_address()` trait
      method, `ClassicAddressResolution` enum, firing order, and enforcement flow
* [x] **`ClassicAddressResolution` enum** — `NoMapping` | `MapsToAddress(u64)` |
      `AllowAlways`
* [x] **`resolve_classic_address()` on `ResourceAccessPolicy`** — developer
      override with default `NoMapping` (preserves all current behavior)
* [x] **Updated `check_endpoint_access()`** — add `value: Option<u32>`, call hook
      after upstream checks pass, enforce address ranges on `MapsToAddress`
* [x] **`ExclusiveAddressRange` variant on `AccessDenied`** — clear error when a
      user without the lock accesses an exclusive address range
* [x] **REST/gRPC handler updates** — thread `value` through `enforce_endpoint()` /
      `check_access()` from all Classic handlers (6 REST, 10 gRPC)
* [x] **Developer guide** — standalone `docs/guides/classic-address-mapping.md`
      with complete implementation walkthrough, realistic examples, common patterns
      (single address register, upper-bits packing, paged addressing), and FAQ
* [x] **Unit tests** — 9 new tests (181 total): custom policy scenarios,
      shared-endpoint/exclusive-address enforcement, NoMapping backward compat

### Design Decisions

1. **Hook fires after upstream checks** — the developer never needs to deal with
   authentication, resource acquisition, or lock state. Their only job is mapping
   endpoint + value → address.
2. **Three return variants** — `NoMapping` (default, backward-compatible),
   `MapsToAddress(u64)` (triggers address range enforcement), `AllowAlways`
   (infrastructure endpoints like data registers).
3. **Exclusive ranges require lock** — when `MapsToAddress` resolves to an
   Exclusive address range, the user must hold the resource lock. Shared ranges
   allow any holder.
4. **Value threading** — WireIn passes `Some(value)`, TriggerIn passes
   `Some(1 << bit)`, all reads and pipe operations pass `None`.

### Files Changed

| File | Change |
|------|--------|
| `docs/architecture/adr/ADR-025-classic-address-mapping.md` | **[NEW]** ADR |
| `docs/guides/classic-address-mapping.md` | **[NEW]** Developer guide |
| `crates/ok-fpga-core/src/resource.rs` | Enum, trait method, enforcement, tests |
| `crates/ok-fpga-server/src/rest/classic.rs` | `enforce_endpoint()` + 6 handlers |
| `crates/ok-fpga-server/src/grpc/classic_service.rs` | `check_access()` + 10 handlers |
| `docs/architecture/overview.md` | ADR index table (all 25 ADRs) |
| `docs/architecture/adr/ADR-024-endpoint-access-enforcement.md` | Future Work cross-ref |

### Verification

- 181 unit tests passing (9 new ADR-025 tests), 0 failures
- `cargo check` clean (pre-existing warnings only)
- Backward-compatible: `DefaultAccessPolicy` returns `NoMapping`, identical to
  previous behavior

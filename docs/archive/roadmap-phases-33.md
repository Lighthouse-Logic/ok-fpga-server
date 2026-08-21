# Roadmap Archive — Phase 33

## Phase 33 — Read-Aware Lock Enforcement

**Status:** ✅ Completed 2026-08-21  
**Triggered by:** Live testing with Shared + ReadOnly holders during lock  
**Amends:** ADR-024 §6 (Lock Enforcement), ADR-026 §5 (ReadOnly lock bypass)

### Problem

Two issues discovered when client 1 (Shared holder) locks a resource:

1. **Bug — ReadOnly holders blocked from reads during lock (ADR-026 §5 violation):**
   `check_endpoint_access()` unconditionally skipped locked resources without
   checking the holder's access mode or the operation type. ReadOnly holders
   got `AccessDenied` on output endpoints (WireOut, TriggerOut).

2. **Design change — Shared holders should read during lock (ADR-024 §6 amendment):**
   The original ADR-024 §6 said locking blocks ALL operations. Live testing
   showed non-lock-holders need read access for monitoring.

### Root Cause

Lock check in `ok-fpga-core/src/resource.rs` L1548-1560 unconditionally
`continue`d (skipped) any resource locked by another user, regardless of
the operation type or the holder's access mode.

### Fix

Made the lock check read/write-aware:
- **`check_endpoint_access()`**: Uses `ep_type.is_input()` — lock blocks
  write operations (input endpoints), allows read operations (output endpoints)
- **`check_address_access()`**: Added `is_write: bool` parameter — lock blocks
  write address operations, allows read address operations
- Updated all callers in REST (`enforce_address`) and gRPC (`check_address`)

### Clarification: Exclusive ≠ Private

Confirmed that Exclusive resources are "always-locked" (only the sole holder
can write) but NOT "always-private" (ReadOnly observers CAN join unless the
resource is explicitly marked `private: true`).

### Files Changed

| File | Change |
|------|--------|
| `ok-fpga-core/src/resource.rs` | Lock check in `check_endpoint_access()` and `check_address_access()` made read/write-aware |
| `ok-fpga-server/src/rest/classic.rs` | `enforce_address()` gains `is_write` parameter; callers updated |
| `ok-fpga-server/src/grpc/classic_service.rs` | `check_address()` gains `is_write` parameter; callers updated |
| `ADR-024-endpoint-access-enforcement.md` | §6 amended: lock blocks writes only, not reads |
| `ADR-026-readonly-acquisition.md` | §5 pipeline diagram + implementation note added |
| `docs/access_control_overview.md` | New overview document (Access Mode × Lock × Private interaction model) |

### Tests Added

| Test | Verifies |
|------|----------|
| `locked_shared_holder_can_read_output_endpoint` | Shared non-locker reads WireOut/TriggerOut ✅ |
| `locked_shared_holder_cannot_write_input_endpoint` | Shared non-locker writes WireIn ❌ (LockedByOther) |
| `locked_readonly_holder_can_read_output_endpoint` | ReadOnly non-locker reads WireOut ✅ |
| `locked_readonly_holder_denied_private_endpoint` | ReadOnly reads private WireOut ❌ (PrivateDataDenied) |
| `locked_address_read_allowed_for_non_locker` | Address read ✅, address write ❌ |
| `exclusive_resource_allows_readonly_read` | ReadOnly reads WireOut on Exclusive ✅, writes ❌ |

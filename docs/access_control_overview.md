# Access Control Interaction Model

> **Purpose**: A single reference for how **Access Mode**, **Resource Locking**,
> and the **Private** flag interact during endpoint enforcement.
>
> **Source ADRs:**
> [ADR-006](architecture/adr/ADR-006-resource-management.md) · 
> [ADR-020](architecture/adr/ADR-020-client-side-resource-registration.md) · 
> [ADR-024](architecture/adr/ADR-024-endpoint-access-enforcement.md) · 
> [ADR-025](architecture/adr/ADR-025-classic-address-mapping.md) · 
> [ADR-026](architecture/adr/ADR-026-readonly-acquisition.md)

---

## 1. Concepts

### Resource-Level Access Mode (set at registration, ADR-020 §3)

The **resource's** `access_mode` is a hardware design constraint chosen by the
developer when creating the resource. It is **not** a runtime user choice.

| Resource `access_mode` | Meaning |
|------------------------|---------|
| **Shared** | Multiple users may hold the resource simultaneously. Any holder may temporarily **lock** it for uninterrupted access. |
| **Exclusive** | Only one user may hold the resource at a time. The resource is **always implicitly locked** when acquired. Lock/unlock is a no-op error. Exclusive is **effectively always-locked** — only the sole holder can write, but ReadOnly observers may join (unless the resource is marked `private`). |

### Holder-Level Access Mode (chosen at acquisition, ADR-026)

When a user **acquires** a resource, they choose their **holder mode**:

| Holder mode | Meaning |
|-------------|---------|
| **Shared** | Full read + write access to all non-private endpoints/addresses. |
| **ReadOnly** | Observe-only — can read output endpoints but cannot write, cannot call UpdateWireOuts/UpdateTriggerOuts, cannot lock, cannot access Private data. |

> [!IMPORTANT]
> These are two different uses of the word "Shared". The **resource** is
> declared `Shared` (a hardware design fact). A **user** acquires it in either
> `Shared` (read+write) or `ReadOnly` (observe-only) mode. The enum values
> `AccessMode::Shared`, `AccessMode::Exclusive`, and `AccessMode::ReadOnly`
> serve both roles, but their meaning depends on context.

### Resource Locking (ADR-020 §3, ADR-024 §6)

Locking is a **temporary** mechanism for Shared resources only:

| Concept | Description |
|---------|-------------|
| **Lock** | A Shared holder requests exclusive control temporarily. Resource transitions `InUseShared → InUseLocked`. |
| **Unlock** | The lock holder releases the lock. Resource transitions `InUseLocked → InUseShared`. |
| **Exclusive resources** | Always in `InUseLocked` from acquisition to release. Lock/unlock API calls return an error. |
| **ReadOnly holders** | Cannot lock. Attempting to lock returns an error. |

### Private Flag (ADR-026 §2)

"Private" restricts **ReadOnly** access at three hierarchical levels:

| Level | Flag location | Effect |
|-------|---------------|--------|
| **Whole Resource** | `Resource.private` | ReadOnly **acquisition** denied entirely. |
| **Per-Endpoint** | `EndpointEntry.private` | ReadOnly holder gets `PrivateDataDenied` on this endpoint. |
| **Per-Address Range** | `AddressRange.private` | ReadOnly holder gets `PrivateDataDenied` on addresses in this range. |

> [!NOTE]
> **Exclusive ≠ Private.** Although only one user can hold an Exclusive
> resource as a read+write holder, ReadOnly observers CAN join (ADR-026 §4)
> unless the resource is explicitly marked `private: true`. If the developer
> wants no observers on an Exclusive resource, they must set `private: true`
> at registration time. See Section 2 acquisition table.

---

## 2. Acquisition Rules

### Who can acquire what?

| Resource State | Resource Mode | Request Mode | Result |
|----------------|---------------|-------------|--------|
| Available | Shared | Shared | ✅ Allow |
| Available | Shared | ReadOnly | ✅ Allow (unless resource is Private) |
| Available | Exclusive | Exclusive | ✅ Allow → immediately `InUseLocked` |
| Available | Exclusive | Shared | ❌ Deny — Exclusive resources require Exclusive acquisition |
| Available | Exclusive | ReadOnly | ✅ Allow (unless resource is Private — but see note below) |
| InUseShared | Shared | Shared | ✅ Allow (add to holders) |
| InUseShared | Shared | ReadOnly | ✅ Allow (unless resource is Private) |
| InUseShared | Shared | Exclusive | ❌ Deny — already has shared holders |
| InUseLocked | Shared (temp locked) | ReadOnly | ✅ Allow (ReadOnly doesn't interfere with lock) |
| InUseLocked | Shared (temp locked) | Shared | ❌ Deny — locked by another holder |
| InUseLocked | Exclusive | ReadOnly | ✅ Allow (unless resource is Private) |
| InUseLocked | Exclusive | Shared | ❌ Deny — exclusively held |
| Unavailable/Init | — | — | ❌ Deny |

> [!TIP]
> **Exclusive resources allow ReadOnly observers.** ADR-026 §4 says ReadOnly
> can join even a locked/exclusive resource because "ReadOnly doesn't
> interfere." If the developer wants to prevent observation, they mark the
> resource `private: true` at registration time. This is consistent with
> the principle that Exclusive means "always-locked" (write protection), not
> "always-private" (observation denied).

---

## 3. Operation-Time Enforcement

### The Full Pipeline

When a user performs an FPGA operation (e.g., `SetWireInValue`, `GetWireOutValue`),
the enforcement pipeline executes these checks in order:

```
Request arrives (user_id, serial, endpoint, ep_type, value)
  │
  ├── [1] Enforcement active? ──────────────────────── OFF → ALLOW
  │
  ├── [2] User has identity? ───────────────────────── NO → deny (403)
  │       (X-User-Id header / x-user-id metadata)
  │
  ├── [3] User holds resource on this device? ──────── NO → deny
  │       (check_device_access — REST middleware)
  │
  ├── FOR EACH held resource on this device:
  │   │
  │   ├── [4] Resource locked by another user? ─┐
  │   │                                         │
  │   │   ┌─────────────────────────────────────┘
  │   │   │
  │   │   │  CURRENT CODE (Bug):
  │   │   │    YES → skip resource (all operations blocked)
  │   │   │
  │   │   │  PROPOSED FIX (Phase 33):
  │   │   │    YES, and operation is WRITE → skip resource
  │   │   │    YES, and operation is READ:
  │   │   │      Holder mode is ReadOnly → allow (ADR-026 §5)
  │   │   │      Holder mode is Shared → allow (ADR-024 §6 amendment)
  │   │   │
  │   ├── [5] Endpoint covered by resource? ────────── NO → skip, try next
  │   │
  │   ├── [6] ADR-025: resolve_classic_address()
  │   │       NoMapping → continue
  │   │       AllowAlways → continue
  │   │       MapsToAddress(addr) → check address ranges
  │   │         In Shared range → continue
  │   │         In Exclusive range + user holds lock → continue
  │   │         In Exclusive range + no lock → skip resource
  │   │
  │   ├── [7] ADR-026: Holder is ReadOnly?
  │   │       Endpoint is input? → DENY (ReadOnlyWriteAttempt)
  │   │       Operation is UpdateWireOuts/TriggerOuts? → DENY
  │   │       Endpoint is private? → DENY (PrivateDataDenied)
  │   │       Address range is private? → DENY (PrivateDataDenied)
  │   │
  │   └── ALLOW ✅
  │
  └── No resource covered the endpoint → DENY
```

### Classification of Read vs Write Operations

For the lock enforcement fix (step [4] above), operations must be classified:

| Operation | Classification | Rationale |
|-----------|---------------|-----------|
| `SetWireInValue` | **Write** | Drives input to FPGA |
| `UpdateWireIns` | **Write** | Latches all pending WireIn values to FPGA |
| `ActivateTriggerIn` | **Write** | Fires a trigger pulse into FPGA |
| `WriteToPipeIn` | **Write** | Streams data into FPGA |
| `WriteToBlockPipeIn` | **Write** | Streams data into FPGA (throttled) |
| `UpdateWireOuts` | **Write** | Causes FPGA to latch output values (disturbs state) |
| `UpdateTriggerOuts` | **Write** | Causes FPGA to latch trigger outputs (disturbs state) |
| `GetWireOutValue` | **Read** | Returns cached wire value (no bus transaction) |
| `IsTriggered` / `GetTriggerOutVector` | **Read** | Returns cached trigger value |
| `ReadFromPipeOut` | **Read** | Reads data stream from FPGA |
| `ReadFromBlockPipeOut` | **Read** | Reads data stream from FPGA (throttled) |
| AXI read | **Read** | Memory-mapped read |
| AXI write | **Write** | Memory-mapped write |
| Register bridge read | **Read** | Address-based read |
| Register bridge write | **Write** | Address-based write |

> [!IMPORTANT]
> `UpdateWireOuts` and `UpdateTriggerOuts` are classified as **Write** even
> though they "read" output values. They cause the FPGA to **latch** output
> registers, which is a bus transaction that disturbs system state. This is
> why ReadOnly holders cannot call them (ADR-026 §6). For lock enforcement
> purposes they are also writes — a non-lock-holder should not trigger
> bus transactions during a locked critical section.

### The `ep_type.is_input()` Predicate

The existing `EndpointType::is_input()` method already classifies endpoints
as input (write) or output (read):

| `EndpointType` | `is_input()` | Lock: Non-holder allowed? |
|----------------|-------------|--------------------------|
| WireIn | `true` | ❌ Write — blocked |
| TriggerIn | `true` | ❌ Write — blocked |
| PipeIn | `true` | ❌ Write — blocked |
| BtPipeIn | `true` | ❌ Write — blocked |
| WireOut | `false` | ✅ Read — allowed |
| TriggerOut | `false` | ✅ Read — allowed |
| PipeOut | `false` | ✅ Read — allowed |
| BtPipeOut | `false` | ✅ Read — allowed |

**Special case**: `UpdateWireOuts` and `UpdateTriggerOuts` are API-level
operations, not endpoint types. They would need to be caught **before** the
endpoint check (they don't target a specific endpoint) or classified by the
handler before calling `check_endpoint_access()`.

---

## 4. Interaction Summary: Lock × Holder Mode × Private

This is the **proposed** behavior after Phase 33 (current behavior differs
at cells marked 🐛).

### Shared Resource, NOT Locked

| Holder Mode | Input EP (Write) | Output EP (Read) | Private EP (Read) |
|-------------|-------------------|-------------------|--------------------|
| Shared | ✅ Allow | ✅ Allow | ✅ Allow |
| ReadOnly | ❌ Deny (R/O write) | ✅ Allow | ❌ Deny (private) |

### Shared Resource, Locked by Another User

| Holder Mode | Input EP (Write) | Output EP (Read) | Private EP (Read) |
|-------------|-------------------|-------------------|--------------------|
| Lock Holder | ✅ Allow | ✅ Allow | ✅ Allow |
| Shared (non-locker) | ❌ Deny (locked) | ✅ Allow 🐛 | ✅ Allow 🐛 |
| ReadOnly | ❌ Deny (R/O + locked) | ✅ Allow 🐛 | ❌ Deny (private) |

🐛 = **Currently broken** — code unconditionally blocks all operations for
non-lock-holders. Phase 33 fixes this.

### Exclusive Resource (Always Locked by Sole Holder)

| Holder Mode | Input EP (Write) | Output EP (Read) | Private EP (Read) |
|-------------|-------------------|-------------------|--------------------|
| Exclusive Holder | ✅ Allow | ✅ Allow | ✅ Allow |
| ReadOnly (if allowed to join) | ❌ Deny (R/O write) | ❌ Deny 🐛 → ✅ Allow | ❌ Deny (private) |

> [!NOTE]
> For Exclusive resources, the ReadOnly holder row only applies if the
> resource is NOT marked `private: true`. If it IS private, ReadOnly
> acquisition is denied at acquire time and this row is unreachable.
> Exclusive means "always-locked" (write protection), not "always-private"
> (observation denied). If the developer wants no observers, they set
> `private: true` explicitly at registration time.

---

## 5. Consistency Check

| Rule | Source | Current Code | Status |
|------|--------|-------------|--------|
| Shared holders can coexist | ADR-006 | ✅ `can_acquire()` allows multiple Shared | ✅ Correct |
| Exclusive → always locked, one holder | ADR-020 §3 | ✅ `can_acquire()` denies second holder | ✅ Correct |
| Lock blocks other holders (write only) | ADR-024 §6 (proposed amend) | 🐛 Blocks ALL operations | **Fix in Phase 33** |
| ReadOnly bypasses lock for reads | ADR-026 §5 | 🐛 Not implemented | **Fix in Phase 33** |
| ReadOnly denied on input endpoints | ADR-026 §5 | ✅ `is_input()` check | ✅ Correct |
| ReadOnly denied on private endpoints | ADR-026 §2 | ✅ `entry.private` check | ✅ Correct |
| ReadOnly denied on private resources (acquire) | ADR-026 §2 | ✅ `can_acquire()` checks `resource.private` | ✅ Correct |
| ReadOnly cannot lock | ADR-026 | ✅ Lock method rejects ReadOnly holders | ✅ Correct |
| ReadOnly cannot call UpdateWireOuts | ADR-026 §6 | ✅ Denied by `is_input()` on synthetic check | ✅ Correct |
| Exclusive = always-locked, not always-private | ADR-020 §3, ADR-026 §4 | ✅ `can_acquire()` allows ReadOnly on Exclusive | ✅ Correct |
| Exclusive address ranges require lock | ADR-025 §3 | ✅ Address range check | ✅ Correct |

---

## 6. Phase 33 Scope

### Must Fix (Bug)
1. **ReadOnly lock bypass** — Implement ADR-026 §5: ReadOnly holders reading
   output endpoints should not be blocked by the lock check.

### Must Fix (Approved Design Change)
2. **Shared holder read during lock** — Amend ADR-024 §6: non-lock-holding
   Shared holders should be able to **read** (output endpoints) but not
   **write** (input endpoints) when the resource is locked.

### Files to Modify
| File | Change |
|------|---------|
| `ok-fpga-core/src/resource.rs` | `check_endpoint_access()` lock check → read/write-aware |
| `ok-fpga-core/src/resource.rs` | `check_address_access()` same lock logic |
| `ADR-024-endpoint-access-enforcement.md` §6 | Amend: lock blocks writes only, not reads |
| `ADR-026-readonly-acquisition.md` | Note: lock bypass now implemented |
| `docs/development-roadmap.md` | Update Phase 33 description |

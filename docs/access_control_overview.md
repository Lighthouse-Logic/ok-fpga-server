# Access Control Interaction Model

> **Purpose**: A single reference for how **Access Mode**, **Resource Locking & Cross-Resource Blocking**, and the **Private** flag interact during registration, acquisition, and operation-time enforcement.
>
> **Authoritative ADR:**
> [ADR-034 (Resource Model Consolidation)](architecture/adr/ADR-034-resource-model-consolidation.md)
>
> **Historical Context ADRs:**
> [ADR-006](architecture/adr/ADR-006-resource-management.md) ·
> [ADR-020](architecture/adr/ADR-020-client-side-resource-registration.md) ·
> [ADR-024](architecture/adr/ADR-024-endpoint-access-enforcement.md) ·
> [ADR-025](architecture/adr/ADR-025-classic-address-mapping.md) ·
> [ADR-026](architecture/adr/ADR-026-readonly-acquisition.md) ·
> [ADR-029](architecture/adr/ADR-029-user-identification-consistency.md)

---

## 1. Core Concepts

### Resource-Level Access Mode (set at registration, [ADR-034 §11](architecture/adr/ADR-034-resource-model-consolidation.md))

The **resource's** `access_mode` is a hardware design constraint declared when registering the resource. Only `shared` and `exclusive` are valid at registration time (registering a resource with `access_mode: "read_only"` is rejected with `400 Bad Request` / `INVALID_ARGUMENT`).

| Resource `access_mode` | Meaning |
|------------------------|---------|
| **`shared`** | Multiple users may hold the resource simultaneously in `shared` or `read_only` mode. Any `shared` holder may temporarily **lock** the resource (`in_use_shared → in_use_locked`) for uninterrupted write access. |
| **`exclusive`** | At most one `exclusive` (read+write) holder may hold the resource at a time. When acquired in `exclusive` mode, the resource transitions directly to `in_use_locked` (`locked_by = Some(user_id)`). `read_only` observers may also join an `exclusive` resource (unless `private: true`). Explicit `lock` / `unlock` calls on an `exclusive` resource return `409 Conflict` (`FAILED_PRECONDITION`). |

### Holder-Level Access Mode (chosen at acquisition, [ADR-034 §8 & §11](architecture/adr/ADR-034-resource-model-consolidation.md))

When a user **acquires** a resource, they specify their **holder mode** (`shared`, `exclusive`, or `read_only`):

| Holder mode | Meaning |
|-------------|---------|
| **`shared`** | Valid on `shared` resources. Grants full read + write access to all covered endpoints and address ranges (including `private: true` items), subject to lock and cross-resource blocking rules. |
| **`exclusive`** | Valid on `exclusive` resources. Grants sole read + write ownership (`in_use_locked`, `locked_by = Some(user_id)`), including `private: true` items. |
| **`read_only`** | Valid on both `shared` and `exclusive` resources (unless `resource.private == true`). Observe-only: may read covered non-private output endpoints/addresses, **may** call `UpdateWireOuts`, and **may** call `UpdateTriggerOuts` (unless any resource on the device declares a `private: true` `TriggerOut` endpoint, per `D75 F2`). Cannot write, cannot call device/AXI resets (`check_reset_access`), cannot lock, and cannot read `private: true` items. |

> [!IMPORTANT]
> The word `shared` appears at two levels: a **resource** declared `access_mode: "shared"` allows multiple holders, while a **holder** acquiring it in `mode: "shared"` requests read+write access (as opposed to `mode: "read_only"`).

### Resource Locking & Cross-Resource Blocking ([ADR-034 §11 & §12](architecture/adr/ADR-034-resource-model-consolidation.md))

| Concept | Description |
|---------|-------------|
| **Lock (`lock_resource`)** | A `shared` holder of a `shared` resource requests temporary exclusive write control. Resource transitions `in_use_shared → in_use_locked` (`locked_by = Some(user_id)`). Repeat-lock by the current lock owner is idempotent (`L1`). |
| **Unlock (`unlock_resource`)** | The lock owner releases the lock. Resource transitions `in_use_locked → in_use_shared` (`locked_by = None`). Releasing a locked resource auto-unlocks it (`L3`). |
| **Exclusive resources** | If acquired first by a `read_only` observer, an `exclusive` resource enters `in_use_shared` (`locked_by = None`, `D25b`). When an `exclusive` holder acquires it, it enters `in_use_locked` (`locked_by = Some(user_id)`, `D25c`). Releasing the `exclusive` holder while `read_only` observers remain transitions it back to `in_use_shared` (`D25d`). |
| **Cross-Resource Blocking (`BlockedByOther`, `B1`–`B4`, `D35`)** | When two registered `shared` resources `R` and `S` on the same device share any endpoint or overlapping address range (`R1`), and user `V` locks `R` (or holds `R` while `R` is `in_use_locked` by `V`), resource `S` is dynamically **blocked** for any other user `U != V`. Blocking is computed dynamically (not stored as a wire `ResourceState`) and blocks writes on `S` (`AccessDenied::BlockedByOther`) unless `U` covers the target item via another unblocked resource. |

### The `private` Flag ([ADR-034 §10](architecture/adr/ADR-034-resource-model-consolidation.md))

`private: true` restricts **`read_only`** access at three hierarchical levels (`shared` and `exclusive` holders are never restricted by `private`):

| Level | Flag location | Effect on `read_only` callers |
|-------|---------------|-------------------------------|
| **Whole Resource** | `Resource.private` | `read_only` **acquisition** is denied with `409 Conflict` (`conflict.code = "private"`). |
| **Per-Endpoint** | `EndpointRange.private` | `read_only` holder receives `403 Forbidden` (`PERMISSION_DENIED` / `PrivateDataDenied`) on that endpoint. Additionally, if any resource on the device covers a `private: true` `TriggerOut` endpoint, `read_only` holders are denied `UpdateTriggerOuts` and trigger reads with `update=true` (`D75 F2`). |
| **Per-Address Range** | `AddressRange.private` | `read_only` holder receives `403 Forbidden` (`PERMISSION_DENIED` / `PrivateDataDenied`) on addresses within that range. |

---

## 2. Acquisition Rules ([ADR-034 §11](architecture/adr/ADR-034-resource-model-consolidation.md))

| Current `ResourceState` | Resource `access_mode` | Requested Holder `mode` | Result & State Transition |
|-------------------------|------------------------|-------------------------|---------------------------|
| `available` | `shared` | `shared` | ✅ Allow → `in_use_shared` |
| `available` | `shared` | `read_only` | ✅ Allow → `in_use_shared` (or ❌ `409` `conflict.code = "private"` if `resource.private`) |
| `available` | `shared` | `exclusive` | ❌ Deny (`409` `conflict.code = "mode_mismatch"`) |
| `available` | `exclusive` | `exclusive` | ✅ Allow → `in_use_locked` (`locked_by = Some(user_id)`) |
| `available` | `exclusive` | `read_only` | ✅ Allow → `in_use_shared` (`locked_by = None`, `D25b`; or ❌ `409` `"private"` if `resource.private`) |
| `available` | `exclusive` | `shared` | ❌ Deny (`409` `conflict.code = "mode_mismatch"`) |
| `in_use_shared` | `shared` | `shared` | ✅ Allow → remains `in_use_shared` |
| `in_use_shared` | `shared` | `read_only` | ✅ Allow → remains `in_use_shared` (unless `resource.private`) |
| `in_use_shared` | `shared` | `exclusive` | ❌ Deny (`409` `conflict.code = "mode_mismatch"`) |
| `in_use_shared` (held by `read_only` observers) | `exclusive` | `exclusive` | ✅ Allow → transitions to `in_use_locked` (`locked_by = Some(user_id)`), retaining `read_only` observers (`D25c`) |
| `in_use_shared` (held by `read_only` observers) | `exclusive` | `read_only` | ✅ Allow → remains `in_use_shared` (unless `resource.private`) |
| `in_use_locked` | `shared` (temp locked) | `read_only` | ✅ Allow → remains `in_use_locked` (unless `resource.private`) |
| `in_use_locked` | `shared` (temp locked) | `shared` | ❌ Deny (`409` — locked by another holder) |
| `in_use_locked` | `exclusive` | `read_only` | ✅ Allow → remains `in_use_locked` (unless `resource.private`) |
| `in_use_locked` | `exclusive` | `exclusive` or `shared` | ❌ Deny (`409` — already exclusively held / mode mismatch) |
| Any held state | Any | Repeat acquire by same `user_id` with **same** `mode` | ✅ Allow — idempotent no-op (`A1`), no duplicate holder entry |
| Any held state | Any | Repeat acquire by same `user_id` with **different** `mode` | ❌ Deny (`409` `conflict.code = "mode_mismatch"` — release first, then re-acquire) |
| `unavailable` / `initializing` | Any | Any | ❌ Deny (`409` / `FAILED_PRECONDITION`) |

---

## 3. Operation-Time Enforcement Pipeline ([ADR-034 §13](architecture/adr/ADR-034-resource-model-consolidation.md))

### 3.1 Endpoint & Address-Span Pipeline (`check_endpoint_access` / `check_address_span_access`)

When a caller performs an FPGA endpoint or register/AXI address operation, `ResourceRegistry` executes the following pipeline in [`crates/ok-fpga-core/src/resource.rs`](../crates/ok-fpga-core/src/resource.rs):

```
Request arrives (user_id, serial, target endpoint or [addr..addr+len), is_write)
  │
  ├── [1] Enforcement active (`endpoint_enforcement == "enforce"`)?
  │       └── OFF → ALLOW ✅
  │
  ├── [2] Caller provided non-empty identity (`X-User-Id` header / `x-user-id` metadata)?
  │       └── NO  → DENY: 400 Bad Request / INVALID_ARGUMENT (ADR-029, ADR-034 §13)
  │
  ├── [3] User holds at least one resource on `serial`?
  │       └── NO  → DENY: 403 Forbidden / PERMISSION_DENIED (NoResourceOnDevice)
  │
  ├── [4] Evaluate each resource `R` held by `user_id` on `serial`:
  │   │
  │   ├── [4a] Does `R` cover the target endpoint (or for spans, does the union
  │   │        of eligible held resources cover every byte in `[addr..addr+len)`)?
  │   │        (Classic address-mapping resources also evaluate `resolve_classic_address`;
  │   │         note that `AddressRange.access_mode` inside a `shared` resource is
  │   │         advisory per S-14 / D44 / D46 and does NOT require a server lock.)
  │   │        └── NO → skip `R`, try next held resource
  │   │
  │   ├── [4b] Is this a WRITE operation?
  │   │        ├── Holder mode on `R` is `read_only`?
  │   │        │     → record `ReadOnlyWriteAttempt`, skip `R`
  │   │        ├── `R` is locked by another user (`locked_by != Some(user_id)`)?
  │   │        │     → record `ResourceLocked`, skip `R`
  │   │        ├── `R` is blocked by another user's lock on an overlapping
  │   │        │   resource on the same device (`is_resource_blocked_for_user`)?
  │   │        │     → record `BlockedByOther`, skip `R`
  │   │        └── Otherwise → ALLOW WRITE ✅
  │   │
  │   └── [4c] Otherwise this is a READ operation:
  │            (Foreign locks and cross-resource blocks do NOT block reads!)
  │            ├── Holder mode on `R` is `shared` or `exclusive`?
  │            │     → ALLOW READ ✅ (even on `private: true` items)
  │            └── Holder mode on `R` is `read_only`?
  │                  ├── Target endpoint or address range is `private: true`?
  │                  │     → record `PrivateDataDenied`, skip `R`
  │                  └── Not private → ALLOW READ ✅
  │
  └── [5] No held resource granted access → DENY (403 Forbidden / PERMISSION_DENIED)
          Precedence of denial reason:
          `ResourceLocked` > `BlockedByOther` > `ReadOnlyWriteAttempt` > `PrivateDataDenied` > `EndpointNotCovered` / `AddressNotCovered`
```

### 3.2 Device-Wide Operation Helpers

Not all FrontPanel operations target a single endpoint or memory address. [`crates/ok-fpga-core/src/resource.rs`](../crates/ok-fpga-core/src/resource.rs) provides three dedicated helpers for device-wide operations:

| Helper in `resource.rs` | Operations Checked | Enforcement Rule |
|-------------------------|--------------------|------------------|
| **`check_trigger_out_update_access(serial, user_id)`** | `UpdateTriggerOuts`, and `IsTriggered` / `GetTriggerOutVector` when `update=true` | Allowed if the caller holds at least one `shared` or `exclusive` resource on `serial`. If the caller holds **only** `read_only` resources on `serial`, allowed **unless** any registered resource on `serial` covers a `TriggerOut` endpoint with `private: true` (`D75 F2` — because latching hardware `TriggerOut` registers clears single-shot triggers on the FPGA). |
| **`check_reset_access(serial, user_id)`** | `ResetFPGA`, `AXILiteReset`, `AXIFullReset`, `AXIStreamReset` | Requires the caller to hold at least one `shared` or `exclusive` resource on `serial` that is neither locked by another user (`ResourceLocked`) nor blocked by an overlapping lock (`BlockedByOther`). Callers holding only `read_only` access are denied with `ReadOnlyWriteAttempt` (`D75 F1`). |
| **`check_device_write_access(serial, user_id)`** | Standalone `UpdateWireIns`, `AXIStreamWrite` | Requires the caller to hold at least one `shared` or `exclusive` resource on `serial` that is neither locked by another user (`ResourceLocked`) nor blocked by an overlapping lock (`BlockedByOther`). |

### 3.3 Classification of Read vs Write Operations ([ADR-034 §8 & §13](architecture/adr/ADR-034-resource-model-consolidation.md))

| Operation | Classification | Enforcement Entry Point |
|-----------|---------------|-------------------------|
| `SetWireInValue` | **Write** | `check_endpoint_access(WireIn)` |
| `UpdateWireIns` (standalone) | **Write** | `check_device_write_access` |
| `ActivateTriggerIn` | **Write** | `check_endpoint_access(TriggerIn)` |
| `WriteToPipeIn` / `WriteToBlockPipeIn` | **Write** | `check_endpoint_access(PipeIn / BtPipeIn)` |
| `UpdateWireOuts` (standalone or `update=true`) | **Read** (`D42`) | Device holder check (`check_device_access`); allowed for `read_only` holders |
| `GetWireOutValue` | **Read** | `check_endpoint_access(WireOut)` |
| `UpdateTriggerOuts` (standalone) | **Read** (`D42`, `D75 F2`) | `check_trigger_out_update_access` |
| `IsTriggered` / `GetTriggerOutVector` | **Read** (`D75 F2` if `update=true`) | `check_endpoint_access(TriggerOut)` + `check_trigger_out_update_access` when `update=true` |
| `ReadFromPipeOut` / `ReadFromBlockPipeOut` | **Read** | `check_endpoint_access(PipeOut / BtPipeOut)` |
| Classic `ReadRegister` / `ReadRegisters` | **Read** | `check_address_access(..., is_write = false)` |
| Classic `WriteRegister` / `WriteRegisters` | **Write** | `check_address_access(..., is_write = true)` |
| `AXILiteRead` / `AXIFullRead` | **Read** | `check_address_span_access(..., is_write = false)` |
| `AXILiteWrite` / `AXIFullWrite` | **Write** | `check_address_span_access(..., is_write = true)` |
| `AXIStreamRead` | **Read** | Device holder check (`check_device_access`) |
| `AXIStreamWrite` | **Write** | `check_device_write_access` |
| `ResetFPGA`, `AXILiteReset`, `AXIFullReset`, `AXIStreamReset` | **Write (Reset)** (`D75 F1`) | `check_reset_access` |

---

## 4. Interaction Summary: Lock × Blocking × Holder Mode × Private

### 4.1 `shared` Resource — Unlocked & Unblocked

| Holder Mode | Input / Write Op | Non-Private Output / Read Op | `private: true` Output / Read Op |
|-------------|------------------|------------------------------|----------------------------------|
| `shared` | ✅ Allow | ✅ Allow | ✅ Allow |
| `read_only` | ❌ Deny (`ReadOnlyWriteAttempt`) | ✅ Allow (subject to `D75 F2` for `UpdateTriggerOuts`) | ❌ Deny (`PrivateDataDenied`) |

### 4.2 `shared` Resource — Locked by Another User OR Blocked by Overlapping Lock

| Holder Mode | Input / Write Op | Non-Private Output / Read Op | `private: true` Output / Read Op |
|-------------|------------------|------------------------------|----------------------------------|
| Lock Holder (`locked_by == Some(user_id)`) | ✅ Allow | ✅ Allow | ✅ Allow |
| `shared` (non-locker or blocked via `R1` overlap) | ❌ Deny (`ResourceLocked` / `BlockedByOther`) | ✅ Allow | ✅ Allow |
| `read_only` | ❌ Deny (`ReadOnlyWriteAttempt`) | ✅ Allow (subject to `D75 F2` for `UpdateTriggerOuts`) | ❌ Deny (`PrivateDataDenied`) |

### 4.3 `exclusive` Resource

| Holder Mode | Input / Write Op | Non-Private Output / Read Op | `private: true` Output / Read Op |
|-------------|------------------|------------------------------|----------------------------------|
| `exclusive` Holder (`locked_by == Some(user_id)`) | ✅ Allow | ✅ Allow | ✅ Allow |
| `read_only` Observer (when `resource.private == false`) | ❌ Deny (`ReadOnlyWriteAttempt`) | ✅ Allow (subject to `D75 F2` for `UpdateTriggerOuts`) | ❌ Deny (`PrivateDataDenied`) |

---

## 5. Implementation Reference

All access-control rules above are implemented and verified across Tier 1 (`okREALM_TEST`), Tier 2 (live hardware), and Tier 3 (mock FFI) test suites:

| Component | File | Responsibilities |
|-----------|------|------------------|
| **Core Registry & Enforcement** | [`crates/ok-fpga-core/src/resource.rs`](../crates/ok-fpga-core/src/resource.rs) | `ResourceRegistry`, `acquire_resource`, `lock_resource`, `unlock_resource`, `is_resource_blocked_for_user`, `check_endpoint_access`, `check_address_access`, `check_address_span_access`, `check_reset_access`, `check_trigger_out_update_access`, `check_device_write_access` |
| **REST Handlers & Middleware** | [`crates/ok-fpga-server/src/rest/access_middleware.rs`](../crates/ok-fpga-server/src/rest/access_middleware.rs), [`classic.rs`](../crates/ok-fpga-server/src/rest/classic.rs), [`axi.rs`](../crates/ok-fpga-server/src/rest/axi.rs), [`resources.rs`](../crates/ok-fpga-server/src/rest/resources.rs) | HTTP `X-User-Id` extraction (`400 Bad Request` when missing), RFC 7807 `ProblemDetail` (`403 Forbidden` / `409 Conflict` with structured `conflict` payloads) |
| **gRPC Services** | [`crates/ok-fpga-server/src/grpc/classic_service.rs`](../crates/ok-fpga-server/src/grpc/classic_service.rs), [`axi_service.rs`](../crates/ok-fpga-server/src/grpc/axi_service.rs), [`device_service.rs`](../crates/ok-fpga-server/src/grpc/device_service.rs), [`resource_mapping.rs`](../crates/ok-fpga-server/src/grpc/resource_mapping.rs) | gRPC `x-user-id` metadata extraction (`INVALID_ARGUMENT` when missing), `PERMISSION_DENIED` / `FAILED_PRECONDITION` / `ALREADY_EXISTS` mapping |

# Resource Management — Findings & Recommendations (DRAFT rev 6)

> **Basis:** ADR-003, 004, 005, 006, 015, 016, 017, 018, 020, 021, 023, 024,
> 025, 026, 029; the owner decisions of 2026-10-10 (§1); the DualPort-Transceiver
> `axibridge-access-guide-v2.md`; `api/openapi.yaml` and
> `proto/okfpga/v1/okfpga.proto` (definitions only); local Strata second
> opinion (§8). No code was reviewed.
> Companion: [resource-management-developer-guide.md](resource-management-developer-guide.md).
> **Status:** Draft rev 6, 2026-10-10. Nothing is decided until the owner
> approves. No ADR, spec or code has been changed. The roadmap RBAC item was
> widened (D31, D47).

---

## 1. Owner decisions (2026-10-10)

### 1.1 Round 1–2 (D1–D17)

| ID | From | Decision |
|---|---|---|
| **D1** | Q-A | A lock applies to the **entire** resource. Overlapping **shared** declarations are allowed. Locking a resource already locked by someone else is denied. |
| **D2** | Q-B | A user acquires a given resource at most once (no duplicate holder entries). |
| **D3** | Q-C | Shared resources: many holders. Exclusive: one. ReadOnly for read-only users; Private flags at L1/L2/L3. |
| **D4** | — | This branch's ADR-025 stub implements the DualPort-Transceiver bridge. Bridge EPs are shared. WI 0x00 = AXI write address, WI 0x01 = AXI read address. |
| **D5** | Q1 | **Blocking (simplest rule):** if any shared item (EP or shared range) of resource S is also declared by a resource R that another user has locked, **S as a whole** is "locked by another user" for its holders until R is unlocked. |
| **D6** | Q2 | Shared items are allowed **only in shared resources**. An exclusive resource that declares a shared EP, a shared range or `default_access: shared` is rejected at registration. |
| **D7** | Q3 | A second acquire of the same resource by the same user (same mode) returns **idempotent 200**. |
| **D8** | Q4 | **First registration wins.** A later registration that conflicts is denied with a meaningful error. |
| **D9** | Q5 | The server does not dictate how resources are used; the HDL designer / software architect designs resources within the rules. The entity that programs the FPGA registers its resources. Alice writing Bob's exclusive range is denied, even when neither resource is locked. |
| **D10** | Q6 | ~~A per-EP `access: exclusive` inside a shared resource means "writable only by the locker"~~ **Corrected by D44:** "writable only by the **holders** of the declaring resource". |
| **D11** | Q7 | Expose `private` on EP entries and address ranges in the registration API. |
| **D12** | Q8 | The server stays FPGA-agnostic. ReadOnly does not apply to ADR-025 developer-defined classic address ranges; it applies to FPGADataPortAXI and okRegisterBridge ranges. Future work: server-side sequences for multi-step transactions and FPGA authentication. |
| **D13** | Q9 | The bridge register map is in `client/python-dualport-client/docs/` (now `axibridge-access-guide-v2.md`, D30). |
| **D14** | Q10 | The Authenticator EPs are **shared in both** port resources. Whichever port is acquired first authenticates. |
| **D15** | Q11 | (a) A classic resource with no `endpoints` claims **all EPs as exclusive** (now "unspecified-classic", D45). (b) Any user may reprogram; reprogramming unlocks all, releases all, deletes all resources, then writes the bitfile. (c) Anyone may delete a resource until RBAC exists. (d) ADR-029 is implemented; its status is stale. (e) The enforcement default becomes **`enforce`**. (f) ADR-020 §3 gets ReadOnly behaviour; ADR-026 §4 is correct (allow unless private). |
| **D16** | Q12 | A read of a **non-private** item is permitted, including during someone else's lock. The guide gets a read/write permission matrix. A consolidated ADR may supersede the older resource ADRs. |
| **D17** | Q13 | No server restriction on reset EPs. Any holder of a resource that is not locked by another user (and not blocked) may write any WireIn the resource declares. Future RBAC *may* restrict this. |

### 1.2 Round 3 (D18–D35)

| ID | From | Decision |
|---|---|---|
| **D18** | Q1/N1 | Blocking is **non-transitive** (rule B4). |
| **D19** | Q2/N2 | A blocked resource is blocked **as a whole**: no writes at all through it. |
| **D20** | Q3/N3 | A **multi-step** transaction on a shared resource must run under the lock; a single-step one needs no lock. Until server-side atomic transactions exist, every bridge transaction is multi-step. *Clarified by D46: this is a client obligation; the server never requires a lock.* |
| **D21** | Q4/N4 | **No state-change classification.** Reads (incl. PipeOut/BTPipeOut) are simply allowed per D16. A design that other users' reads could harm must declare the resource **exclusive** (and/or `private`). |
| **D22** | Q5/N5 | A `private` mismatch between overlapping declarations gives **409** to the later registration (first wins). |
| **D23** | Q6/N6 | An unspecified-classic resource registered first blocks all later classic resources on the device. |
| **D24** | N7 | Authentication runs under the lock. |
| **D25** | Q7/N8 | ReadOnly with exclusive resources: (a) exclusive holder + ReadOnly acquire → allowed unless private, stays `InUseLocked`; (b) only ReadOnly holders → `InUseShared`; (c) exclusive acquire while ReadOnly holders exist → allowed → `InUseLocked`. |
| **D26** | N9 | Reprogramming is a **forced override** at every enforcement level, REST and gRPC alike. **All** holders on the device are informed, whatever their state. Future RBAC will restrict who may reprogram. |
| **D27** | Q8/N10 | All clients must comply. `off` / `warn` are **debugging aids** only; the default is `enforce`. |
| **D28** | Q9/N11 | Same-mode repeat acquire = 200 (D7). A **different-mode** acquire is a change, and is **denied** with "release, then acquire". |
| **D29** | Q10/N12 | ReadOnly holders are denied ADR-025 mapped access. |
| **D30** | Q11/N13 | DualPort-Transceiver uses only one-shot writes, burst writes (PipeIn 0x80) and burst reads (PipeOut 0xA0), per `axibridge-access-guide-v2.md`. Authenticated status = AXI Status Register 2 bit 7 (module base + 0x08). Field list in guide §6.1. |
| **D31** | Q12/N14 | Widen the RBAC Future-work item. **Done** (D47). |
| **D32** | Q13 | **One consolidated superseding ADR** (§4). |
| **D33** | Q14/C1 | `access_mode` is **required** at registration; a missing value returns a meaningful error. |
| **D34** | Q15 | ~~Server-side declarative sequences first~~ *Revised by D40.* |
| **D35** | Q16 | A user who holds both port resources may lock one and write the other's exclusive ranges. *With D44/D46 no lock is needed for the write itself.* |

### 1.3 Round 4 (D36–D47)

| ID | From | Decision |
|---|---|---|
| **D36** | S1 | Spec fix approved for Phase 44: **remove** (not deprecate) `resource_type` and `capabilities` (no existing users); add `access_mode` (required), `interfaces`, `endpoints`, `address_ranges` (`name`, `private`), `metadata`, `private`. Applies to OpenAPI and, by parity, proto (P1). |
| **D37** | S2 | Approved: `Resource.access_mode` = declared mode (`shared` / `exclusive`); `holders[]` = `{user_id, mode}`; add `locked_by` and the declaration fields. |
| **D38** | S3 | Approved: 422 for validation; structured error body naming the conflicting resource / item or the locking resource. |
| **D39** | S4 | The draft port JSON resources are corrected as part of C-SFPP-1. |
| **D40** | §5 | When server-side sequences come due, the **embedded engine (option C)** will likely be explored first (extensibility; server-held keys for authentication). Decided at that phase. |
| **D41** | — | The client project is renamed from the codename "dualport" to **DualPort-Transceiver** (`python-DualPort-Transceiver-client`); folders and the repo later. Documentation references updated. |
| **D42** | N16 | `UpdateWireOuts` / `UpdateTriggerOuts` are **reads**: FrontPanel 6.1.0 calls that latch the outputs, and must precede the read. |
| **D43** | N17 | A different-mode acquire returns **409**. |
| **D44** | N18 | **D10 corrected:** an exclusive item (per-EP `access: exclusive`, or an exclusive range) in a shared resource is **writable by any holder** of the declaring resource, not only the locker. No lock is required. |
| **D45** | N19 | **Legacy = Classic.** A Classic resource may use okRegisterBridge, but cannot use direct AXI. |
| **D46** | N20 | The server **never requires a lock to write**. Multi-step writes and reads need the client to lock; single-step ones on any item do not. A user who knows she is alone (e.g. single-resource bitfile) carries no extra burden. Server-side atomic sequences are future work. |
| **D47** | D31 | Roadmap RBAC Future-work item widened (`docs/development-roadmap.md`). |

---

## 2. Status of earlier items

### 2.1 Contradictions (⚠C) and gaps (❓G)

All C- and G-items are resolved except **G18** (`AllowAlways` must not skip
lock/ReadOnly/blocking checks; wording only, consolidated ADR §10). G13 is
now closed by D43; G15 by D44. Full history: rev 5 (commit `8b6ae7c`).

### 2.2 Round-2/3 gaps N1–N20

| ID | Status |
|---|---|
| N1–N15 | Resolved in rev 5 (D18–D31) |
| N16 | **Resolved:** D42 |
| N17 | **Resolved:** D43 |
| N18 | **Resolved:** D44 (proposal "exclusive needs the lock" rejected) |
| N19 | **Partly resolved:** D45. Remaining: which ranges an unspecified-classic resource claims (§3.2 N19) |
| N20 | **Resolved:** D46 |

---

## 3. Rules, open gaps and discrepancies

### 3.1 Normative rules (proposed for the consolidated ADR)

- **R1 (registration, D6, D8, D22, D33, D45).** `access_mode` is required.
  An exclusive resource contains only exclusive items. A new declaration
  that overlaps an existing one is allowed only if **both** are shared items
  in shared resources and agree on type and `private`. Otherwise 409, naming
  the existing resource and the conflicting item. An unspecified-classic
  resource claims all classic EPs exclusively. AXI interfaces on an
  `OK_DATAPORT_LEGACY` device → 422.
- **B1 (blocking, D5, D19).** Resource S is **blocked for user U** if another
  user V ≠ U holds the lock on a resource R ≠ S, and S and R declare at
  least one common shared item. S is also unusable for U if S itself is
  locked by V. A blocked or foreign-locked resource gives U **no write
  rights** through it. Reads of non-private items stay allowed (D16, D21).
- **B2 (write authorisation, D44, D46).** A write by U to item X is allowed
  if U holds at least one resource that covers X, is not ReadOnly, and is
  neither locked by another user nor blocked; then the ADR-025 address check
  ("address in one of that resource's ranges", D9) applies. **No lock is
  required.** Each call is checked against the lock state at that moment.
- **B3 (lock denial).** `lock` on S is denied (409) if S is locked or blocked
  for the caller. A user's own lock never blocks that user (D35).
- **B4 (non-transitive, D18).**
- **L1 (client obligation, D20, D24, D46).** Multi-step operations (writes
  or reads, incl. authentication and every bridge transaction) run under the
  client's lock when other users may hold overlapping resources. The server
  cannot detect or enforce this.
- **A1 (acquire, D7, D28, D43).** Same mode → 200. Different mode → 409,
  "release, then acquire".
- **E1 (meaning of exclusive, D44).** An exclusive item belongs to exactly
  one resource. It is not a mutex among that resource's holders; one-writer
  guarantees come from an **exclusive resource** or from client locking.

### 3.2 Open gaps (round 4, with Strata)

| ID | Issue | Proposal |
|---|---|---|
| **N19** | **Ranges of an unspecified-classic resource.** D45 says it may use okRegisterBridge. | It claims **only the ranges it declares**. Claiming "all register addresses" by default would block every other resource's register ranges. |
| **N21** | **Name for the "no `endpoints` field" resource.** ADR-024 §3 calls it "legacy resource", which clashes with `OK_DATAPORT_LEGACY` (ADR-003) and with D45. | Rename to **"unspecified-classic resource"** in the consolidated ADR (Strata suggestion). |
| **N22** | **Mixed interfaces.** ADR-020 §6 lets `interfaces` be a list, so one resource on a Gen3 device could list `classic` and `axi_lite`. D45 says a Classic resource cannot use direct AXI. | Either (a) a resource is Classic **or** AXI, never both (simplest; consistent with D45), or (b) mixing is allowed on Gen3 devices and D45 applies only to `OK_DATAPORT_LEGACY` devices. Proposed: **(a)**. |
| **N23** | **Register-only classic resource.** A resource that only declares okRegisterBridge ranges and omits `endpoints` would become unspecified-classic and claim every EP. | Require an explicit empty `endpoints` object (`{"entries": []}`) to mean "no EPs"; omission keeps D15a. |
| **N24** | **`ResetFPGA` RPC / `POST …/classic/reset`.** Device-wide, disturbs every user; not covered by D17 (reset EPs) or D26 (reprogram). | Treat as a **write** needing a usable (not foreign-locked, not blocked) resource on the device; does **not** clear resources. Candidate for RBAC (D47). |
| **N25** | **Private visibility.** OpenAPI `Resource.private` and proto `Resource.private` say private resources are "only visible to holders with Shared or Exclusive access". ADR-026 only denies ReadOnly **acquisition**; it says nothing about hiding resources from listings. | Keep listings visible to all (needed to acquire at all); `private` restricts ReadOnly acquisition and reads only. Fix both spec comments (S6, P13). |

Also noted, no question needed (Strata):
- **Self-interleaving.** A user holding both ports can corrupt her own bridge
  transactions; the server does not protect a user from herself (guide §6.2).
- **Lock between unlocked steps.** If Bob locks between two unlocked calls by
  Alice, her next write is denied mid-sequence. This is why L1 exists
  (guide §4.3).

### 3.3 OpenAPI discrepancies (⚠S)

These are **discrepancies between `api/openapi.yaml` and the ADRs / owner
decisions**. S1–S3 are approved for Phase 44 (D36–D38); the spec itself is
edited only at implementation time. S5–S6 need permission.

| ID | Location | Discrepancy | Status |
|---|---|---|---|
| **S1** | `CreateResourceRequest` (~L2042) | Only `name`, `device_serial`, `description`, `resource_type`, `capabilities`. | **Approved** (D36): remove `resource_type` / `capabilities`; add `access_mode` (required), `interfaces`, `endpoints` (`default_access`, `entries[]`: `address`, `type`, `access`, `private`, `description`), `address_ranges[]` (`name`, `start`, `end`, `access`, `private`), `metadata`, `private` |
| **S2** | `Resource` (~L2065) | `access_mode` enum incl. `read_only`, described as "current access mode if acquired"; `holders` is `string[]`; no `locked_by` or declaration fields. | **Approved** (D37) |
| **S3** | `createResource`, acquire, lock | Only 400/404/409 documented; no structured conflict body. | **Approved** (D38) |
| **S5** | `/events` (~L1605), `DeviceEvent` (~L2178) | No `ResourceLocked` / `ResourceUnlocked` events, although ADR-020 §3 requires them and clients wait for `ResourceUnlocked` (guide §4.3). `ResourceChanged` covers "acquired, released, invalidated" only. No event says *why* resources vanished on reprogram (ADR-023 mentions a dashboard `resources_cleared` event that is not in the spec). | Proposed: add `ResourceLocked` / `ResourceUnlocked` (`resource_id`, `user_id`) and a reprogram/clear reason on `ResourceDeleted`. **Needs permission** |
| **S6** | `Resource.private` description | "Private resources are only visible to holders…" (N25). | Proposed fix with N25. **Needs permission** |
| **S4** | port-A/B JSON (not a spec) | Descriptions vs v2 guide: WO 0x20, 0x22/0x23, 0x31, 0x29; unused 0xA1 / large mode. | **Approved for C-SFPP-1** (D39) |

### 3.4 Proto discrepancies (⚠P) — `proto/okfpga/v1/okfpga.proto`, definition only

Audited this round (D36–D38 apply "by parity"). Line numbers approximate.

| ID | Location | Discrepancy | Proposed fix |
|---|---|---|---|
| **P1** | `CreateResourceRequest` (L902) | Same gaps as S1; comment cites ADR-016, not ADR-020. | As D36: remove fields 4–5 (`reserved 4, 5;` so the numbers are never reused), add the declaration messages (`EndpointDeclaration`, `EndpointEntry`, `AddressRange`), `metadata` map, `private` |
| **P2** | `Resource` (L229) | No `access_mode`; `holders` is `repeated string`; no declaration fields. Has `locked_by` (OpenAPI lacks it). | As D37: `ResourceAccessMode access_mode`, `repeated Holder holders` (`user_id`, `mode`) — changing field 6's type needs a new field number |
| **P3** | `AccessMode` enum (L114) | `ACCESS_MODE_SHARED = 0` is the proto3 default, so an omitted mode silently means "shared". Contradicts D33 for registration. | Separate `ResourceAccessMode { UNSPECIFIED = 0; SHARED; EXCLUSIVE; }` for registration (UNSPECIFIED → INVALID_ARGUMENT); give holder `AccessMode` an `UNSPECIFIED = 0` too |
| **P4** | `AccessMode` comment (L117–120) | "ReadOnly holders cannot call UpdateWireOuts, fire triggers…" contradicts D42; "fire triggers" is ambiguous. | "cannot write input EPs or call ActivateTriggerIn; may call UpdateWireOuts / UpdateTriggerOuts (reads)" |
| **P5** | `UserSession` (L249) | `held_resource_ids` has no holder mode. | `repeated Holding` (`resource_id`, `mode`, `locked`) |
| **P6** | `DeviceEvent` (L280) | Only DeviceAdded, DeviceRemoved, BitfileProgrammed, ResourceStateChanged. Missing ADR-020 §11's `ResourceLocked` / `ResourceUnlocked`, and REST's ResourceCreated, ResourceDeleted, UserConnected, UserDisconnected, ServerShutdown. Names differ from REST (BitfileProgrammed vs DeviceProgrammed; ResourceStateChanged vs ResourceChanged). `ResourceState` has no "deleted". | Event parity with REST plus S5's lock events; one name per event across both specs |
| **P7** | Errors (`ErrorCluster`, L27) | Resource RPCs return `ErrorCluster` (FrontPanel error codes). No defined mapping to gRPC status codes (PERMISSION_DENIED / FAILED_PRECONDITION / ALREADY_EXISTS / INVALID_ARGUMENT) and no structured detail for D38. | Document the status-code mapping per RPC; add an error-detail message (conflicting resource / item, locking resource + user, required action) |
| **P8** | Identity | Data-port requests (e.g. `SetWireInValueRequest` L524) and `ResourceRequest` (create/delete), `DeviceRequest` (`ClearFPGAConfiguration`, `ResetFPGA`) have no `user_id`. The `x-user-id` metadata key (ADR-024 §4, ADR-029) is not documented anywhere in the proto. Under `enforce` this is the only way to identify. | Document `x-user-id` as required metadata in the file header and each service comment (no field changes needed) |
| **P9** | `AXIDataPortService` comment (L1191) | "AXI Lite, Full, and Stream operations **via okRegisterBridge**" conflates direct AXI (FPGADataPortAXI) with the Classic register bridge (`ReadRegister` / `WriteRegister`, ClassicDataPortService). Contradicts D45. | "…via FPGADataPortAXI (FP-Gen3 devices only)"; mention okRegisterBridge under the Classic register RPCs |
| **P10** | `LockResource` / `UnlockResource` (L1093) | Return `ResourceResponse`; ADR-020 §11 says `StatusResponse` and a separate `UnlockResourceRequest`. | Keep the proto (it is authoritative and more useful); consolidated ADR supersedes ADR-020 §11 |
| **P11** | `DeleteResource` comment (L1083) | "Returns an error if held" — true for explicit delete, but reprogram / clear delete held resources (D15b, D26). | Add "configure / clear force-delete regardless (see …)" |
| **P12** | `ClearFPGAConfiguration` (L1021) | Not described as a forced cleanup like ConfigureFPGA, and carries no user id. | Same forced-override semantics and notifications as reprogramming (D26); identity via P8 |
| **P13** | `Resource.private` comment (L240) | Visibility claim not in ADR-026 (N25). | Fix with N25 |
| **P14** | Address width | Register RPCs use `uint32` addresses, AXI RPCs `uint64`; ADR-020 §8 ranges are 32-bit. | Keep ranges 32-bit (all supported devices); state that AXI addresses above 32 bits cannot be covered by a range and are denied under `enforce` |
| **P15** | Non-resource (noted only) | `GetServerStatus` exists in DeviceService and ServerService with different types; ServerService RPCs take `StatusResponse` as their **request** type. | Out of scope; separate cleanup item |

### 3.5 ADR text superseded by D42–D46 (for the consolidated ADR)

Verified against the files:

| ADR | Text | Superseded by |
|---|---|---|
| ADR-025 §3 table | "In an Exclusive range \| Locked by this user → ALLOW"; "Not locked / locked by other → DENY" | D44: holder of the declaring resource, resource usable → ALLOW |
| ADR-025 §3 | "critical pattern: shared endpoints gating exclusive address ranges" (implies lock protection) | D44 (pattern stays; protection = ownership, not lock) |
| ADR-025 §5 flow (L191) | "In Exclusive range → User holds lock?" | B2 |
| ADR-025 Future Work (L250) | "Per-range lock semantics — instead of requiring a full resource lock for exclusive ranges" | Premise gone (D44) |
| ADR-024 §2 (L92) | "A locked resource can ONLY be used by the user who holds the lock." | B1: writes only; reads stay allowed (D16) |
| ADR-024 §6 (L314) | "state-disturbing operations (UpdateWireOuts, UpdateTriggerOuts)" denied to other holders | D42 |
| ADR-020 §3 (L171) | "Other holders … cannot perform operations on the resource until it is unlocked." | B1 (writes only) |
| ADR-020 §7 | No endpoints → "all endpoints in exclusive mode" | Keep; meaning of exclusive per D44; name per N21 |
| ADR-020 §11 | Lock/Unlock return `StatusResponse` | Proto (P10) |

### 3.6 Reprogramming event order (D15b, D26 — proposal)

Under the device lock, atomically, for each resource on the device:
`ResourceUnlocked` (if locked) → release every holder → `ResourceDeleted`
(reason: reprogram / clear); then write the bitfile (or clear) and emit the
device event. All holders receive the events whatever their state.
Requires S5 / P6.

---

## 4. Consolidated ADR (decided, D32)

Working title: **"Resource Model, Registration, Acquisition, Locking and
Enforcement"**, next free number. It supersedes the sections below; the old
ADRs stay in the log, marked *Superseded by ADR-0NN §x*.

| § | Content | Supersedes / absorbs |
|---|---|---|
| 1 | Status, scope, authority (specs remain authoritative; S/P changes per D36–D38) | — |
| 2 | Terminology: resource, item, holder, holder mode, lock, blocked, overlap, private, exclusive item (E1), single-/multi-step, unspecified-classic (N21), Classic vs AXI (D45), ADR-025 mapped vs register / direct address | — |
| 3 | Resource model and registration schema | ADR-006 Resource Model; ADR-020 §1, §2, §6, §9; ADR-026 §2 |
| 4 | Declaration rules R1, unspecified-classic (N19, N23), interfaces (N22) | ADR-020 §7, §8, §10; ADR-024 §3 |
| 5 | Acquisition A1, ReadOnly states (D25) | ADR-006 Acquisition; ADR-020 §3 (part); ADR-026 §3, §4, §7 |
| 6 | Locking B1–B4, client obligation L1, events | ADR-020 §3, §11; ADR-024 §6 |
| 7 | Lifecycle: release, disconnect, heartbeat, device disconnect, reprogram / clear (D26, §3.6), delete (D15c), ResetFPGA (N24) | ADR-006 Lifecycle; ADR-020 §4, §5, §10; ADR-023 Fix-F; ADR-024 §10 |
| 8 | Reads vs writes (D16, D21, D42) and the permission matrix (D44) | ADR-024 §6; ADR-026 §1, §5 |
| 9 | Enforcement levels (D27), route tiers, pipeline (B2), errors and gRPC status mapping (D38, P7), identity metadata (P8) | ADR-024 §1, §2, §4, §5, §7, §9; ADR-026 §8 |
| 10 | Address enforcement: ADR-025 hook (compiled), register / direct AXI, ReadOnly (D12, D29), `AllowAlways` (G18) | ADR-025 §2, §3, §5, Future Work |
| 11 | Identity | ADR-029 (mark Accepted/Implemented) |
| 12 | Consequences, test requirements | — |
| 13 | Open items: RBAC (D47), server-side sequences (D40) | — |

---

## 5. Future work: server-side sequences (D12, D40)

**Today:** ADR-025 hooks are compiled Rust. A DualPort one-shot write is
about 5 REST calls and a burst read about 6–7; clients lock around them
(L1).

| Option | What | Recompile? | Pros | Cons |
|---|---|---|---|---|
| A. Built-in DualPort ops | `axi_write`, `axi_read`, `authenticate` endpoints | Yes | Atomic, simple | Server becomes DualPort-aware: violates D12 |
| B. Declarative sequences | Named JSON sequences (set/update wires, trigger, poll, pipe I/O, parameters) run under the device mutex after one permission check | No | Generic, auditable, one round trip | Limited logic; auth maths needs built-in operators |
| **C. Embedded engine (Rhai, Lua, WASM) — explore first (D40)** | Scripts from a server directory or the API, run under the device mutex after one permission check (B2) | No | Fully flexible; keys can stay server-side for authentication | Sandbox, limits, security review, new dependency |
| D. Compiled trait hooks | ADR-025 style | Yes | Fast, type-safe | Per-bitfile logic built into the server |

A sequence/script call counts as **single-step**, so L1's lock obligation
falls away for it. Details are decided when the phase comes due.

---

## 6. Impact on the drafted plans (all on hold until approval)

- **Server Phase 44:** consolidated ADR (§4). Spec work S1–S3 and the proto
  equivalents P1–P9, P11–P14 (approved by D36–D38 for OpenAPI; proto
  parity needs your confirmation, Q4); S5/S6 if approved. R1 with 409/422
  bodies. B1–B4 without any lock requirement for writes (D44, D46). A1.
  Reads per D42. Forced reprogram/clear cleanup (§3.6). Default `enforce`.
  DualPort ADR-025 stub. ADR-029 status. Sequences stay Future (D40).
- **Roadmap:** RBAC item widened (D47). The Phase 44 draft text in
  `docs/development-roadmap.md` still reflects the older model (declare-once,
  ADR-034/035, sequence endpoint) and needs a rewrite after approval.
- **LabVIEW C-17:** handle 403 (blocked / foreign lock / ReadOnly), 409
  (lock denied, registration conflict, different-mode acquire), 422. Send a
  user id everywhere. Lock around multi-step operations.
- **DualPort-Transceiver C-SFPP-1 (D41):** two shared port resources, bridge /
  Authenticator / GTY EPs shared, own ranges exclusive, `access_mode`
  explicit. Fix the JSON descriptions (D39). Lock around authentication,
  every bridge transaction and the Status Register 2 check. Never read
  PipeOut 0xA0 outside its own locked transaction. Handle the reprogram
  events. Folder rename to `client/python-DualPort-Transceiver-client/` pending (Q6).

Next step after approval: a code review against this model.

---

## 7. Clarifying questions

1. **N19 / N23:** an unspecified-classic resource claims only the ranges it
   declares, and a register-only resource sends an explicit empty
   `endpoints` object?
2. **N21:** rename "legacy resource" to "unspecified-classic resource"?
3. **N22:** a resource is either Classic or AXI, never both (proposed), or
   may a Gen3 resource mix them?
4. **Proto parity:** do D36–D38 also authorise the matching proto changes
   (P1–P9, P11–P14), including `reserved` field numbers and a new
   `ResourceAccessMode` enum with `UNSPECIFIED = 0`?
5. **S5 / S6, N24, N25:** may I plan the lock events and private-visibility
   fix, and treat `ResetFPGA` as a write needing a usable resource?
6. **Rename (D41):** rename the folder to `client/python-DualPort-Transceiver-client/`
   now or later? Import package `dualport_client` (a `.` is not valid in a Python
   import name), distribution `okfpga-DualPort-Transceiver-client`? Rename the git
   branch too?

---

## 8. Local Strata second opinion

Job `job_1c030374` (inputs: both docs rev 5, the proto, ADR-003/020/024/
025/026/029, the owner's round-4 answers; max_tokens 32768, reasoning
16384). Claims were verified against the files before adoption.

**Adopted:**
- ADR text contradicted by D44/D46 → §3.5 (quotes checked; one quote Strata
  attributed to ADR-025 was actually from the guide, dropped).
- "Exclusive ≠ one writer" and "blocking overrides holder rights" → E1,
  guide §2.1, §4.3, §5.3.
- Lock between unlocked steps; self-interleaving → §3.2 notes.
- Proto audit → P1–P14 (P3 default-enum issue, P4, P6 parity, P7 errors,
  P8 identity, P9 okRegisterBridge wording, P10–P14). Non-resource items
  → P15.
- Name "unspecified-classic" → N21.

**Differs:**
- Strata says an unspecified-classic resource "does not imply absence of
  AXI or register bridge". D45 says a Classic resource cannot use direct
  AXI, so this is not adopted; the mixed-interface case is N22.
- Strata did not see the register-only hole (N23), the `ResetFPGA` gap
  (N24, partly raised) or the private-visibility mismatch (N25); added here.

**Quality note:** after about 50 findings the output degenerated into
repetitive filler (sections 3.49–3.200, e.g. "ServiceInfo LabVIEW client
status"); that part was ignored.

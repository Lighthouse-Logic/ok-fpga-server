# Resource Management — Findings & Recommendations (DRAFT rev 7)

> **Basis:** ADR-003, 004, 005, 006, 015, 016, 017, 018, 020, 021, 023, 024,
> 025, 026, 029; the owner decisions of 2026-10-10 (§1); the DualPort-Transceiver
> `axibridge-access-guide-v2.md`; `api/openapi.yaml` and
> `proto/okfpga/v1/okfpga.proto` (definitions only); local Strata second
> opinion (§8). No code was reviewed.
> Companion: [resource-management-developer-guide.md](resource-management-developer-guide.md).
> **Status:** Draft rev 7, 2026-10-10. No ADR, spec or code has been changed.
> The server roadmap Phase 44 has been rewritten to this model (§6).

> [!IMPORTANT]
> **Model in one sentence (D1, D8, D48):** shared items declared by shared
> resources **may overlap**; every other overlap is denied to the later
> registration (first wins). There is **no "declare-once" rule**.

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
| **D13** | Q9 | The bridge register map is in `client/python-DualPort-Transceiver-client/docs/` (now `axibridge-access-guide-v2.md`, D30). |
| **D14** | Q10 | The Authenticator EPs are **shared in both** port resources. Whichever port is acquired first authenticates. |
| **D15** | Q11 | (a) A classic resource with no `endpoints` claims **all EPs as exclusive** ("unspecified-classic", D53). (b) Any user may reprogram; reprogramming unlocks all, releases all, deletes all resources, then writes the bitfile. (c) Anyone may delete a resource until RBAC exists. (d) ADR-029 is implemented; its status is stale. (e) The enforcement default becomes **`enforce`**. (f) ADR-020 §3 gets ReadOnly behaviour; ADR-026 §4 is correct (allow unless private). |
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
| **D33** | Q14/C1 | `access_mode` is **required** at registration; a missing value returns a meaningful error. *See N27: the round-5 wording may amend this.* |
| **D34** | Q15 | ~~Server-side declarative sequences first~~ *Revised by D40.* |
| **D35** | Q16 | A user who holds both port resources may lock one and write the other's exclusive ranges. *With D44/D46 no lock is needed for the write itself.* |

### 1.3 Round 4 (D36–D47)

| ID | From | Decision |
|---|---|---|
| **D36** | S1 | Spec fix approved for Phase 44: **remove** (not deprecate) `resource_type` and `capabilities` (no existing users); add `access_mode`, `interfaces`, `endpoints`, `address_ranges` (`name`, `private`), `metadata`, `private`. Applies to OpenAPI and, by parity, proto (P1, D59). |
| **D37** | S2 | Approved: `Resource.access_mode` = declared mode (`shared` / `exclusive`); `holders[]` = `{user_id, mode}`; add `locked_by` and the declaration fields. |
| **D38** | S3 | Approved: 422 for validation; structured error body naming the conflicting resource / item or the locking resource. |
| **D39** | S4 | The draft port JSON resources are corrected as part of C-SFPP-1. |
| **D40** | §5 | When server-side sequences come due, the **embedded engine (option C)** will likely be explored first (extensibility; server-held keys for authentication). Decided at that phase. |
| **D41** | — | The client project is renamed from the codename "dualport" to **DualPort-Transceiver**. **Done** (D60): folder `client/python-DualPort-Transceiver-client/`, branch `feature/python-DualPort-Transceiver-client`, package `dualport_client`. |
| **D42** | N16 | `UpdateWireOuts` / `UpdateTriggerOuts` are **reads**: FrontPanel 6.1.0 calls that latch the outputs, and must precede the read. |
| **D43** | N17 | A different-mode acquire returns **409**. |
| **D44** | N18 | **D10 corrected:** an exclusive item (per-EP `access: exclusive`, or an exclusive range) in a shared resource is **writable by any holder** of the declaring resource, not only the locker. No lock is required. |
| **D45** | N19 | ~~Legacy = Classic.~~ **Corrected by D50.** A Classic resource may use okRegisterBridge, but cannot use direct AXI. |
| **D46** | N20 | The server **never requires a lock to write**. Multi-step writes and reads need the client to lock; single-step ones on any item do not. A user who knows she is alone (e.g. single-resource bitfile) carries no extra burden. Server-side atomic sequences are future work. |
| **D47** | D31 | Roadmap RBAC Future-work item widened (`docs/development-roadmap.md`). |

### 1.4 Round 5 (D48–D60)

| ID | From | Decision |
|---|---|---|
| **D48** | roadmap 44.0 | **Declare-once is rejected.** Shared declarations in shared resources may overlap (D1, D8, R1). All declare-once text is removed from the server roadmap (done in this rev). |
| **D49** | — | **"Legacy" means only the dataport `OK_DATAPORT_LEGACY`** (ADR-003, "Important: `okTDeviceInfo.dataport` vs HDL Endpoint Availability"). All current FPGAs report LEGACY. `OK_DATAPORT_UNKNOWN` needs no support. Future boards will report `OK_DATAPORT_AXI` and need new HDL, bitfiles and resources. A resource is never called "legacy". |
| **D50** | D45 | Linking Legacy and Classic was a mistake and is removed. True statement: **a Classic resource may use okRegisterBridge, but cannot use direct AXI.** |
| **D51** | G18 | `AllowAlways` skips **only** the address-range check, never the identity, holder, ReadOnly, lock or blocking checks. |
| **D52** | N19 | An okRegisterBridge resource claims **only the ranges it declares**. |
| **D53** | N21 | No resource is called "legacy" (D49). Working name for a classic resource with no `endpoints` field: **"unspecified-classic resource"** (confirmation asked, §7 Q6). |
| **D54** | N22 | A resource **may list both** `classic` and `axi_*` interfaces (ADR-003). AXI interfaces are still rejected (422) on an `OK_DATAPORT_LEGACY` device, i.e. on every device today. |
| **D55** | N23 | An **explicit empty** `endpoints` object (`{"entries": []}`) means "no EPs"; omitting `endpoints` keeps D15a. Documented in guide §2.4. |
| **D56** | N24 | **Rejected:** no special rule for `ResetFPGA` / `POST …/classic/reset`. It is allowed per the current rules; future RBAC may change this. It does not clear resources. (Exact gate wording: §7 Q5.) |
| **D57** | N25, S6 | Approved: `private` does not hide resources from listings; it restricts ReadOnly acquisition and reads only. Fix the OpenAPI and proto comments (S6, P13). |
| **D58** | S5 | Approved: add `ResourceLocked` / `ResourceUnlocked` events (`resource_id`, `user_id`) and a reprogram/clear reason on `ResourceDeleted` (OpenAPI; proto by parity, P6). |
| **D59** | Q4, P1–P3 | **No `UNSPECIFIED` enum value** in OpenAPI or proto, anywhere. P1, P2, P3 agreed. "Unspecified" is only a transient condition when the resource JSON omits `access_mode`; the established rules then set the mode. It is never a spec state. (How the rules set it: N27. Field numbers 4–5: §7 Q4.) |
| **D60** | D41 | Rename done now: folder, branch, package `dualport_client`, distribution `okfpga-DualPort-Transceiver-client`. |

---

## 2. Status of earlier items

### 2.1 Contradictions (⚠C) and gaps (❓G)

All C- and G-items are resolved. G18 closed by D51. Full history: rev 5
(commit `8b6ae7c`).

### 2.2 Gaps N1–N25

| ID | Status |
|---|---|
| N1–N15 | Resolved in rev 5 (D18–D31) |
| N16–N18, N20 | Resolved: D42, D43, D44, D46 |
| N19 | **Resolved:** D52 |
| N21 | **Resolved:** D53 (name to confirm) |
| N22 | **Resolved:** D54 |
| N23 | **Resolved:** D55 |
| N24 | **Resolved:** D56 (gate wording, §7 Q5) |
| N25 | **Resolved:** D57 |

---

## 3. Rules, open gaps and discrepancies

### 3.1 Normative rules (proposed for ADR-034)

- **R1 (registration, D6, D8, D22, D33, D48, D54).** `access_mode` per N27.
  An exclusive resource contains only exclusive items. A new declaration
  that overlaps an existing one is allowed only if **both** are shared items
  in shared resources and agree on type and `private`. Otherwise 409, naming
  the existing resource and the conflicting item. An unspecified-classic
  resource claims all classic EPs exclusively; an explicit empty `endpoints`
  object claims none (D55). Address ranges claim only what is declared
  (D52). AXI interfaces on an `OK_DATAPORT_LEGACY` device → 422.
- **B1 (blocking, D5, D19).** Resource S is **blocked for user U** if another
  user V ≠ U holds the lock on a resource R ≠ S, and S and R declare at
  least one common shared item. S is also unusable for U if S itself is
  locked by V. A blocked or foreign-locked resource gives U **no write
  rights** through it. Reads of non-private items stay allowed (D16, D21).
- **B2 (write authorisation, D44, D46, D51).** A write by U to item X is
  allowed if U holds at least one resource that covers X, is not ReadOnly,
  and is neither locked by another user nor blocked; then the ADR-025
  address check ("address in one of **that** resource's ranges", D9)
  applies. `AllowAlways` skips only that address check. **No lock is
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
- **T1 (terminology, D49, D50).** "Legacy" = `OK_DATAPORT_LEGACY` only.
  "Classic" = FrontPanel classic EPs (+ okRegisterBridge). "Direct AXI" =
  FPGADataPortAXI, which needs an `OK_DATAPORT_AXI` device.

### 3.2 Open gaps (round 5, with Strata)

| ID | Issue | Options / proposal |
|---|---|---|
| **N26** | **Meaning of `endpoints.default_access`.** ADR-020 §7: it applies to "any endpoint NOT explicitly listed". ADR-024 §3 table: empty `entries` → `default_access` applies to **all** EPs. That contradicts D55 when `default_access` is present. Also, an omitted per-entry `access` defaults to `exclusive` today (roadmap S-24). | **(a) Strata — declaration shorthand:** `default_access` claims **every unlisted classic EP** with that access. `{entries: []}` without `default_access` = no EPs (D55). A `shared` default overlaps every other shared resource, so any lock elsewhere blocks it (D5); an `exclusive` default takes every unlisted EP and stops later classic resources (like D23). Conflict errors say whether the clash came from an entry or the default. **(b) Mine — default value only:** `default_access` is just the `access` used by listed entries that omit `access`; a resource covers **only its listed entries**. Simpler, no accidental "claim everything", and fixes S-24's silent exclusive default. ADR-020 §7 and ADR-024 §3 are superseded in ADR-034. **Recommended: (b).** |
| **N27** | **Missing `access_mode` (D33 vs D59 wording).** D33: required, missing → meaningful error. D59: "the established rules set it appropriately (e.g. Shared)". | **(1) Strict:** registration requires it (422). **(2) Default:** missing → `shared` (supersedes D33; a forgotten field silently creates a resource that overlaps and can be blocked). **(3) Hybrid (recommended):** registration strict (D33); acquire `mode` **optional** — omitted = the resource's declared mode, `read_only` is opt-in, a conflicting mode → 409. Proto: presence via proto3 `optional`, no UNSPECIFIED (D59). |
| **N28** | **Dataport enum.** OpenAPI `DeviceInfo.dataport` (~L1780): `[unknown, classic, axi]`; `GET …/dataport` (~L593): `[unknown, legacy, axi]`; proto `DataportType`: `UNKNOWN=0, LEGACY=1, AXI=2`. Conflicts with D49 and D59. | Public values `legacy`, `axi` only, in both OpenAPI places. Proto `DATAPORT_LEGACY = 0; DATAPORT_AXI = 1;` (wire-value change; acceptable since no external users). A FrontPanel `OK_DATAPORT_UNKNOWN` report is mapped to `legacy` with a logged warning (all current devices are LEGACY), or treated as an error — owner to choose. Fixes old roadmap S-10 / SD-05. **Needs spec permission.** |
| **N29** | **ADR-003 wording.** ADR-003 L13 ("AXI Endpoints (`FPGADataPortAXI`) … via `okRegisterBridge`"), L37 ("AXI path: `okRegisterBridge`…") and L174–179 ("AXI register bridge", "(AXI path)") call okRegisterBridge an AXI path. Conflicts with D50 (Classic may use okRegisterBridge; direct AXI is FPGADataPortAXI). | Do not edit ADR-003; ADR-034 §2 terminology states the correct meaning and marks those lines as superseded wording. Same fix as P9 in the proto. |

Also noted, no question needed (Strata):
- **Self-interleaving.** A user holding both ports can corrupt her own bridge
  transactions; the server does not protect a user from herself (guide §6.2).
- **Lock between unlocked steps.** If Bob locks between two unlocked calls by
  Alice, her next write is denied mid-sequence. This is why L1 exists
  (guide §4.3).
- **A bridge-EP-only resource** never reaches another resource's ranges:
  the ADR-025 address check uses the ranges of the **same** usable covering
  resource, and an empty range list → deny (D9; old roadmap S-20).

### 3.3 OpenAPI discrepancies (⚠S)

These are **discrepancies between `api/openapi.yaml` and the ADRs / owner
decisions**. All content is approved; the spec itself is edited only at
implementation time, with an explicit go-ahead per edit (ai-collaboration
§2, §4).

| ID | Location | Discrepancy | Status |
|---|---|---|---|
| **S1** | `CreateResourceRequest` (~L2042) | Only `name`, `device_serial`, `description`, `resource_type`, `capabilities`. | **Approved** (D36): remove `resource_type` / `capabilities`; add `access_mode`, `interfaces`, `endpoints` (`default_access` per N26, `entries[]`: `address`, `type`, `access`, `private`, `description`), `address_ranges[]` (`name`, `start`, `end`, `access`, `private`, `description`), `metadata`, `private` |
| **S2** | `Resource` (~L2065) | `access_mode` enum incl. `read_only`, described as "current access mode if acquired"; `holders` is `string[]`; no `locked_by` or declaration fields. | **Approved** (D37) |
| **S3** | `createResource`, acquire, lock | Only 400/404/409 documented; no structured conflict body. | **Approved** (D38) |
| **S4** | port-A/B JSON (not a spec) | Descriptions vs v2 guide: WO 0x20, 0x22/0x23, 0x31, 0x29; unused 0xA1 / large mode. | **Approved for C-SFPP-1** (D39) |
| **S5** | `/events` (~L1605), `DeviceEvent` (~L2178) | No `ResourceLocked` / `ResourceUnlocked`; no reason on resource deletion at reprogram. | **Approved** (D58) |
| **S6** | `Resource.private` description | "Private resources are only visible to holders…" | **Approved** (D57) |
| **S7** | `DeviceInfo.dataport`, `GET …/dataport` | `unknown`, `classic` values; two different enums (N28). | Proposed per N28. **Needs permission** |
| **S8** | Acquire request `mode` (~L1263) | Required. | Optional per N27 option 3, if chosen. **Needs permission** |

### 3.4 Proto discrepancies (⚠P) — `proto/okfpga/v1/okfpga.proto`, definition only

Line numbers approximate. **No enum gets an `UNSPECIFIED` value (D59).**
Presence is detected with proto3 `optional` fields.

| ID | Location | Discrepancy | Fix / status |
|---|---|---|---|
| **P1** | `CreateResourceRequest` (L902) | Same gaps as S1; comment cites ADR-016, not ADR-020. | **Approved** (D59): remove fields 4–5, add `EndpointDeclaration`, `EndpointEntry`, `AddressRange`, `metadata`, `private`. Mark 4–5 `reserved` or reuse them: §7 Q4 |
| **P2** | `Resource` (L229) | No `access_mode`; `holders` is `repeated string`; no declaration fields. Has `locked_by` (OpenAPI lacks it). | **Approved** (D59): `ResourceAccessMode access_mode`, `repeated Holder holders` (`user_id`, `mode`) under a new field number, declaration fields |
| **P3** | `AccessMode` enum (L114) | `ACCESS_MODE_SHARED = 0` is the proto3 default, so an omitted mode silently means "shared". | **Approved, revised** (D59): no UNSPECIFIED. Registration: `ResourceAccessMode { RESOURCE_ACCESS_MODE_SHARED = 0; RESOURCE_ACCESS_MODE_EXCLUSIVE = 1; }` on an `optional` field, so absence is detectable and handled per N27. Acquire `mode` also `optional` (N27 option 3) |
| **P4** | `AccessMode` comment (L117–120) | "ReadOnly holders cannot call UpdateWireOuts, fire triggers…" contradicts D42. | "cannot write input EPs or call ActivateTriggerIn; may call UpdateWireOuts / UpdateTriggerOuts (reads)" |
| **P5** | `UserSession` (L249) | `held_resource_ids` has no holder mode. | `repeated Holding` (`resource_id`, `mode`, `locked`) |
| **P6** | `DeviceEvent` (L280) | Missing lock events and REST event parity; names differ (BitfileProgrammed vs DeviceProgrammed; ResourceStateChanged vs ResourceChanged). | Event parity with REST plus D58 lock events and delete reason; one name per event |
| **P7** | Errors (`ErrorCluster`, L27) | No mapping to gRPC status codes, no structured detail for D38. | Document status codes per RPC; add an error-detail message |
| **P8** | Identity | `x-user-id` metadata key is not documented; most messages have no `user_id`. | Document `x-user-id` as required metadata (no field changes) |
| **P9** | `AXIDataPortService` comment (L1191) | "AXI … operations **via okRegisterBridge**" conflates direct AXI with the Classic register bridge (D50, N29). | "…via FPGADataPortAXI (`OK_DATAPORT_AXI` devices only)"; mention okRegisterBridge under the Classic register RPCs |
| **P10** | `LockResource` / `UnlockResource` (L1093) | Return `ResourceResponse`; ADR-020 §11 says `StatusResponse`. | Keep the proto; ADR-034 supersedes ADR-020 §11 |
| **P11** | `DeleteResource` comment (L1083) | "Returns an error if held" — reprogram / clear delete held resources (D15b, D26). | Add the forced-delete note |
| **P12** | `ClearFPGAConfiguration` (L1021) | Not described as a forced cleanup. | Same semantics and notifications as reprogramming (D26) |
| **P13** | `Resource.private` comment (L240) | Visibility claim not in ADR-026. | **Approved** (D57) |
| **P14** | Address width | Register RPCs `uint32`, AXI RPCs `uint64`; ranges are 32-bit. | Keep ranges 32-bit; AXI addresses above 32 bits cannot be covered and are denied under `enforce` |
| **P15** | Non-resource (noted only) | `GetServerStatus` duplicated; ServerService RPCs take `StatusResponse` as request. | Out of scope |
| **P16** | `DataportType` (L85) | `DATAPORT_UNKNOWN = 0` (D49, D59). | Per N28: `DATAPORT_LEGACY = 0; DATAPORT_AXI = 1;` **Needs permission** |

### 3.5 ADR text superseded (for ADR-034)

Verified against the files:

| ADR | Text | Superseded by |
|---|---|---|
| ADR-025 §3 table | "In an Exclusive range \| Locked by this user → ALLOW"; "Not locked / locked by other → DENY" | D44: holder of the declaring resource, resource usable → ALLOW |
| ADR-025 §3 | "critical pattern: shared endpoints gating exclusive address ranges" (implies lock protection) | D44 (pattern stays; protection = ownership, not lock) |
| ADR-025 §5 flow (L191) | "In Exclusive range → User holds lock?" | B2 |
| ADR-025 Future Work (L250) | "Per-range lock semantics…" | Premise gone (D44) |
| ADR-024 §2 (L92) | "A locked resource can ONLY be used by the user who holds the lock." | B1: writes only; reads stay allowed (D16) |
| ADR-024 §3 | "legacy resource"; empty `entries` → `default_access` for all EPs | D53 (name); D55 and N26 (meaning) |
| ADR-024 §6 (L314) | "state-disturbing operations (UpdateWireOuts, UpdateTriggerOuts)" | D42 |
| ADR-020 §3 (L171) | "Other holders … cannot perform operations on the resource until it is unlocked." | B1 (writes only) |
| ADR-020 §7 | No endpoints → "all endpoints in exclusive mode"; `default_access` applies to unlisted EPs | Keep the first (D15a, name D53); `default_access` per N26 |
| ADR-020 §11 | Lock/Unlock return `StatusResponse` | Proto (P10) |
| ADR-003 L13, L37, L174–179 | okRegisterBridge described as the "AXI path" | D50, T1 (N29) |

### 3.6 Reprogramming event order (D15b, D26, D58)

Under the device lock, atomically, for each resource on the device:
`ResourceUnlocked` (if locked) → release every holder → `ResourceDeleted`
(reason: `reprogram` / `clear`); then write the bitfile (or clear) and emit
the device event. All holders receive the events whatever their state.
`ResetFPGA` does **not** clear resources (D56).

---

## 4. Consolidated ADR (decided, D32): ADR-034

`docs/architecture/adr/ADR-034-resource-model-consolidation.md`, title
**"Resource Model, Registration, Acquisition, Locking and Enforcement"**
(ADR-001–033 exist). It supersedes the sections below; the old ADRs stay in
the log, marked *Superseded by ADR-034 §x*. It is added to
`docs/architecture/overview.md` in the same commit as the first code it
documents (roadmap 44-A).

| § | Content | Supersedes / absorbs |
|---|---|---|
| 1 | Status, scope, authority (specs remain authoritative; S/P changes per D36–D38, D57–D59) | — |
| 2 | Terminology (T1): resource, item, holder, holder mode, lock, blocked, overlap, private, exclusive item (E1), single-/multi-step, unspecified-classic (D53), legacy = dataport only (D49), Classic vs direct AXI (D50), ADR-025 mapped vs register / direct address | ADR-003 wording (N29) |
| 3 | Resource model and registration schema | ADR-006 Resource Model; ADR-020 §1, §2, §6, §9; ADR-026 §2 |
| 4 | Declaration rules R1, unspecified-classic, empty `endpoints` (D55), `default_access` (N26), ranges (D52), interfaces (D54) | ADR-020 §7, §8, §10; ADR-024 §3 |
| 5 | Acquisition A1, optional acquire mode (N27), ReadOnly states (D25) | ADR-006 Acquisition; ADR-020 §3 (part); ADR-026 §3, §4, §7 |
| 6 | Locking B1–B4, client obligation L1, events (D58) | ADR-020 §3, §11; ADR-024 §6 |
| 7 | Lifecycle: release, disconnect, heartbeat, device disconnect, reprogram / clear (D26, §3.6), delete (D15c), ResetFPGA (D56) | ADR-006 Lifecycle; ADR-020 §4, §5, §10; ADR-023 Fix-F; ADR-024 §10 |
| 8 | Reads vs writes (D16, D21, D42) and the permission matrix (D44) | ADR-024 §6; ADR-026 §1, §5 |
| 9 | Enforcement levels (D27), route tiers, pipeline (B2), errors and gRPC status mapping (D38, P7), identity metadata (P8) | ADR-024 §1, §2, §4, §5, §7, §9; ADR-026 §8 |
| 10 | Address enforcement: ADR-025 hook (compiled), register / direct AXI, ReadOnly (D12, D29), `AllowAlways` (D51) | ADR-025 §2, §3, §5, Future Work |
| 11 | Identity | ADR-029 (mark Accepted/Implemented) |
| 12 | Dataport mapping (N28) | ADR-003 (dataport enum only) |
| 13 | Consequences, test requirements | — |
| 14 | Open items: RBAC (D47), server-side sequences (D40), per-holder trigger cache (§7 Q7), address carriers | — |

---

## 5. Future work: server-side sequences (D12, D40)

**Today:** ADR-025 hooks are compiled Rust. A DualPort one-shot write is
about 5 REST calls and a burst read about 6–7; clients lock around them
(L1).

| Option | What | Recompile? | Pros | Cons |
|---|---|---|---|---|
| A. Built-in DualPort ops | `axi_write`, `axi_read`, `authenticate` endpoints | Yes | Atomic, simple | Server becomes DualPort-aware: violates D12 |
| B. Declarative sequences | Named JSON sequences run under the device mutex after one permission check | No | Generic, auditable, one round trip | Limited logic; auth maths needs built-in operators |
| **C. Embedded engine (Rhai, Lua, WASM) — explore first (D40)** | Scripts from a server directory or the API, run under the device mutex after one permission check (B2) | No | Fully flexible; keys can stay server-side for authentication | Sandbox, limits, security review, new dependency |
| D. Compiled trait hooks | ADR-025 style | Yes | Fast, type-safe | Per-bitfile logic built into the server |

A sequence/script call counts as **single-step**, so L1's lock obligation
falls away for it. Details are decided when the phase comes due. The old
roadmap `classic/sequence` endpoint proposal is moved to Later.

---

## 6. Impact on the drafted plans

- **Server Phase 44: rewritten** in
  [development-roadmap.md](development-roadmap.md) as "Resource Model
  Consolidation & Enforcement Alignment", sub-phases 44-A … 44-J, ADR-034
  only, all declare-once text removed (D48). Every old S/SD/Q44 item is
  kept, revised, moved to Later or dropped (Strata classification, §8).
  Still **Draft** until you approve it.
- **LabVIEW C-17** (`docs/client/client-development-roadmap.md`,
  uncommitted draft): still describes declare-once in places. Needs the
  same correction (§7 Q9). Handle 403 (blocked / foreign lock / ReadOnly),
  409 (lock denied, registration conflict, different-mode acquire), 422.
  Send a user id everywhere. Lock around multi-step operations.
- **DualPort-Transceiver C-SFPP-1** (`client/python-DualPort-Transceiver-client/`): two
  shared port resources, bridge / Authenticator / GTY EPs shared, own
  ranges exclusive, `access_mode` explicit. Fix the JSON descriptions (D39).
  Lock around authentication, every bridge transaction and the Status
  Register 2 check. Handle the reprogram events. Its roadmap (L4, H-08
  L117, L141, L147, L173, L213, L216) and ADR-C-SFPP-001 (L90–91) still
  mention declare-once (§7 Q9).

---

## 7. Clarifying questions

1. **N26 `default_access`:** (a) shorthand that claims every unlisted EP, or
   (b) only the default `access` for listed entries that omit it
   (recommended)? With (b), should an entry with neither `access` nor a
   `default_access` be a 422 (recommended) instead of today's silent
   `exclusive`?
2. **N27 missing `access_mode`:** (1) strict 422, (2) default to `shared`
   (supersedes D33), or (3) strict at registration + optional acquire mode
   that defaults to the declared mode (recommended)?
3. **N28 dataport:** public enum `legacy` / `axi` only, proto renumbered to
   `DATAPORT_LEGACY = 0, DATAPORT_AXI = 1`? A FrontPanel `UNKNOWN` report:
   map to `legacy` + warning, or error? May I plan the S7 / P16 spec edits?
4. **P1 field numbers:** "removing the reserved 4, 5 numbers" — should
   fields 4 and 5 be declared `reserved 4, 5;` so they are never reused
   (recommended, standard proto practice), or freely reused by the new
   fields?
5. **D56 ResetFPGA gate:** "allowed per the current rules" = (a) the tier-3
   data-port gate only (caller identified and holds **any** resource on the
   device, recommended reading of your answer), or (b) caller holds at least
   one **usable, non-ReadOnly** resource (Strata)?
6. **D53 name:** confirm "unspecified-classic resource".
7. **S-16 per-holder trigger cache:** move to Later? It adds per-holder
   state, which D21 avoids; designs where another holder's
   `UpdateTriggerOuts` hurts should use an exclusive resource or the lock.
8. **Metadata (old Q44-10):** `map<string,string>` (proto parity; the
   DualPort JSON `associated_cores` array must become a string), or any JSON
   value in REST?
9. **Client docs:** may I correct the declare-once text in the LabVIEW C-17
   draft, the DualPort-Transceiver client roadmap and ADR-C-SFPP-001 next round?

---

## 8. Local Strata second opinion

Job `job_d25276ee` (inputs over 64k tokens: both docs rev 6, the roadmap,
the proto, ADR-003/020/024/025/026/029, the round-5 answers; max_tokens
98304, reasoning 32768, per the owner). An earlier job with a smaller token
budget (`job_9e1f1aa6`) was discarded. Claims were checked against the files
before adoption. The answer was complete, with no degeneration.

**Adopted:**
- `default_access` hole → N26 (Strata found ADR-020 §7 vs ADR-024 §3 vs
  D55).
- Options for missing `access_mode`, with `optional` presence and no
  UNSPECIFIED → N27, P3.
- Dataport enum proposal → N28, S7, P16.
- Address check on the **same** covering resource's ranges → §3.2 note.
- Classification of every old roadmap item and the Phase 44 sub-phase
  structure, test names and acceptance criteria → roadmap Phase 44.

**Differs:**
- N26: Strata recommends option (a); I recommend (b) as simpler, with no
  accidental claim of every EP.
- D56: Strata reads the ResetFPGA gate as "a usable non-ReadOnly resource";
  your answer reads more like the plain tier-3 gate. Asked (Q5).
- Strata did not raise the `reserved` field-number ambiguity (Q4) or the
  ADR-003 L13 occurrence (N29); added here.

# Resource Management — Findings & Recommendations (DRAFT rev 9)

> **Basis:** ADR-003, 004, 005, 006, 015, 016, 017, 018, 020, 021, 023, 024,
> 025, 026, 029; the owner decisions of 2026-10-10 (§1); the DualPort-Transceiver
> `axibridge-access-guide-v2.md`; `api/openapi.yaml` and
> `proto/okfpga/v1/okfpga.proto` (definitions only); local Strata second
> opinion (§8). No code was reviewed.
> Companion: [resource-management-developer-guide.md](resource-management-developer-guide.md).
> **Status:** Draft rev 9, 2026-10-10. No ADR, spec or code has been changed.
> The server roadmap Phase 44 follows this model (§6).

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
| **D15** | Q11 | (a) A classic resource with no `endpoints` claims **all EPs as exclusive** (it is simply an exclusive resource, D68). (b) Any user may reprogram; reprogramming unlocks all, releases all, deletes all resources, then writes the bitfile. (c) Anyone may delete a resource until RBAC exists. (d) ADR-029 is implemented; its status is stale. (e) The enforcement default becomes **`enforce`**. (f) ADR-020 §3 gets ReadOnly behaviour; ADR-026 §4 is correct (allow unless private). |
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
| **D23** | Q6/N6 | A resource without `endpoints` (D68) registered first blocks all later classic resources on the device. |
| **D24** | N7 | Authentication runs under the lock. |
| **D25** | Q7/N8 | ReadOnly with exclusive resources: (a) exclusive holder + ReadOnly acquire → allowed unless private, stays `InUseLocked`; (b) only ReadOnly holders → `InUseShared`; (c) exclusive acquire while ReadOnly holders exist → allowed → `InUseLocked`. |
| **D26** | N9 | Reprogramming is a **forced override** at every enforcement level, REST and gRPC alike. **All** holders on the device are informed, whatever their state. Future RBAC will restrict who may reprogram. |
| **D27** | Q8/N10 | All clients must comply. `off` / `warn` are **debugging aids** only; the default is `enforce`. |
| **D28** | Q9/N11 | Same-mode repeat acquire = 200 (D7). A **different-mode** acquire is a change, and is **denied** with "release, then acquire". |
| **D29** | Q10/N12 | ReadOnly holders are denied ADR-025 mapped access. |
| **D30** | Q11/N13 | DualPort-Transceiver uses only one-shot writes, burst writes (PipeIn 0x80) and burst reads (PipeOut 0xA0), per `axibridge-access-guide-v2.md`. Authenticated status = AXI Status Register 2 bit 7 (module base + 0x08). Field list in guide §6.1. |
| **D31** | Q12/N14 | Widen the RBAC Future-work item. **Done** (D47). |
| **D32** | Q13 | **One consolidated superseding ADR** (§4). |
| **D33** | Q14/C1 | `access_mode` is **required** at registration; a missing value returns a meaningful error. *Exception: a resource without `endpoints` (D68).* |
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
| **D53** | N21 | No resource is called "legacy" (D49). ~~Working name "unspecified-classic resource"~~ *Superseded by D68: no special name.* |
| **D54** | N22 | A resource **may list both** `classic` and `axi_*` interfaces (ADR-003). AXI interfaces are still rejected (422) on an `OK_DATAPORT_LEGACY` device, i.e. on every device today. |
| **D55** | N23 | An **explicit empty** `endpoints` object (`{"entries": []}`) means "no EPs"; omitting `endpoints` keeps D15a. Documented in guide §2.4. |
| **D56** | N24 | **Rejected:** no special rule for `ResetFPGA` / `POST …/classic/reset`. It is allowed per the current rules; future RBAC may change this. It does not clear resources. (Exact gate wording: §7 Q5.) |
| **D57** | N25, S6 | Approved: `private` does not hide resources from listings; it restricts ReadOnly acquisition and reads only. Fix the OpenAPI and proto comments (S6, P13). |
| **D58** | S5 | Approved: add `ResourceLocked` / `ResourceUnlocked` events (`resource_id`, `user_id`) and a reprogram/clear reason on `ResourceDeleted` (OpenAPI; proto by parity, P6). |
| **D59** | Q4, P1–P3 | **No `UNSPECIFIED` enum value** in OpenAPI or proto, anywhere. P1, P2, P3 agreed. "Unspecified" is only a transient condition when the resource JSON omits `access_mode`; the established rules then set the mode. It is never a spec state. (How the rules set it: D62, D68. Field numbers: D65.) |
| **D60** | D41 | Rename done now: folder, branch, package `dualport_client`, distribution `okfpga-DualPort-Transceiver-client`. |

### 1.5 Round 6 (D61–D72)

| ID | From | Decision |
|---|---|---|
| **D61** | N26 | **`default_access` is only the default `access` for listed entries that omit `access`.** A resource covers **only its listed entries**; `default_access` never claims unlisted EPs. With an empty `entries` list it has no effect. An entry with neither `access` nor `default_access` → 422 (replaces today's silent `exclusive`). Rule A unchanged: an exclusive resource may not give `default_access: shared`. |
| **D62** | N27 | **Hybrid.** Registration requires `access_mode` (422 if missing, D33), except as D68 says. The acquire `mode` is **optional**: omitted = the resource's declared mode; `read_only` is opt-in; a conflicting mode → 409 (D43). Proto: `optional` fields, no UNSPECIFIED (D59). |
| **D63** | N28 | Dataport enum is **`legacy` / `axi`** only, in both OpenAPI places; proto `DATAPORT_LEGACY = 0, DATAPORT_AXI = 1`. If FrontPanel reports `OK_DATAPORT_UNKNOWN`, the server reports an **error** (no mapping to `legacy`). **Spec change permission granted** (applied with the code in 44-H). Listing behaviour: N30. |
| **D64** | N29 | Agreed: ADR-034 terminology supersedes ADR-003's "okRegisterBridge = AXI path" wording. |
| **D65** | Q4 | Proto `CreateResourceRequest` field numbers 4 and 5 are **reused** by new fields (no `reserved`); nothing deployed is affected. Mapping: N31. |
| **D66** | Q5 | `ResetFPGA` uses the **tier-3 data-port gate only**: caller identified and holding any resource on the device. |
| **D67** | Q7 | ~~**Never** a per-holder TriggerOut cache (old S-16), now or later.~~ **Reopened 2026-10-11 (Q-F):** still no per-holder state, but a device-wide, non-standard **TriggerOut event log** (client-held sequence numbers) is a 🔵 Later item in the roadmap. Until then the server stays agnostic and clients manage how and when triggers are read. |
| **D68** | Q6 | *Revised by D77 (Pipe/BTPipe not claimed).* A resource JSON **without an `endpoints` field** is simply an **exclusive resource** that claims every classic EP (ADR-020 §7): a quick-and-dirty resource. No special name. `access_mode` may be omitted and is then set to `exclusive`; `access_mode: exclusive` is accepted; `access_mode: shared` → 422. It claims only the ranges it declares (D52), blocks later classic resources (D23), and ReadOnly may join unless private (D25). |
| **D69** | Q8 | `metadata` stays **`map<string,string>`** (text values only; proto unchanged). Lists go into one string (DualPort `associated_cores` → comma-separated). The server's web dashboard shows each resource's metadata. |
| **D70** | N30 | `OK_DATAPORT_UNKNOWN` is **not supported**. A device that reports it is **left out** of device listings and treated as not present: single-device calls return 404 / gRPC `NOT_FOUND`, and nothing can be registered on it. The dataport enums have no "unknown" value. |
| **D71** | N31 | Proto `CreateResourceRequest`: field 4 = `optional ResourceAccessMode access_mode`, field 5 = `repeated string interfaces`; the other new fields take 6+. |
| **D72** | N32 | **422** for every error visible from the request alone (rule A, a resource without `endpoints` declared `shared`, missing fields, type / address mismatch, entry without `access`, non-string metadata, `axi_*` on a legacy device). **409** only for clashes with existing state (overlap with another resource, duplicate name, lock denied, different-mode acquire). |

### 1.6 Phase 44 planning (D73–D74)

| # | Item | Decision |
|---|------|----------|
| **D73** | Q44-13 | Error body: RFC 7807 `ProblemDetail` plus an optional `conflict` object `{code, resource_id, resource_name, item_type, item}`; `code` ∈ `overlap`, `locked`, `blocked`, `mode_mismatch`, `validation`; `item_type` ∈ `endpoint`, `address_range`. gRPC: 409 → `FAILED_PRECONDITION`, 422 → `INVALID_ARGUMENT`, 404 → `NOT_FOUND`; the same fields in a `ResourceConflictDetail` message in the status details. |
| **D74** | Q44-14 | The OpenAPI 409 "resources in use" responses on configure / clear are removed (forced cleanup, D26); done in 44-F. |

### 1.7 Post-44-A: ReadOnly and Opal Kelly read semantics (D75–D77, 2026-10-11)

These supersede the 2026-10-10/11 wording of D16, D21, D42 and D44 where
they differ. Spec text was changed in version 0.3.0; enforcement of F1, F2
and F4 and the D77 coverage change land in 44-E.

| # | Item | Decision |
|---|------|----------|
| **D75** | F1, F2, F4 | **ReadOnly rule.** A ReadOnly holder may **read** every covered item that is not `private`: WireOuts and `UpdateWireOuts`; TriggerOuts (`IsTriggered`, `GetTriggerOutVector`, `UpdateTriggerOuts`); PipeOut / BTPipeOut; `ReadRegister(s)`; AXI reads. It may **never write**: WireIn / `UpdateWireIns`, `ActivateTriggerIn`, PipeIn / BTPipeIn, register and AXI writes. **F1:** ReadOnly is denied `ResetFPGA` (AXI Lite/Full/Stream resets are treated as writes too). **F2:** `UpdateTriggerOuts` (and `update=true` on trigger reads) is denied to ReadOnly when **any** resource on the device declares a `private` TriggerOut; snapshot reads are checked per EP (`private` → deny). **Design rule (documented, not enforced):** a ReadOnly action should not change user-logic state or consume data other holders rely on (TriggerOut latches, Pipe/BTPipe FIFOs, clear-on-read registers); any item where a ReadOnly read could cause an unwanted side-effect **should be marked `private`**. The HDL architect decides. **F4:** `warn_readonly_side_effects = true` in `config.toml` (env override): when on, the server logs `warn!` at registration if ReadOnly holders could join a resource with a non-private PipeOut / BTPipeOut / TriggerOut or a non-private range. Shared / exclusive holders are unrestricted (D42 still applies to them). |
| **D76** | Q-E, F3, Q-B, Q-G | **Opal Kelly read semantics.** Reads match FrontPanel exactly: `GetWireOutValue`, `IsTriggered`, `GetTriggerOutVector` read only the device's **snapshot**, shared by all clients (one handle per device); only `UpdateWireOuts` / `UpdateTriggerOuts` touch the FPGA. An optional `update` flag (default false; REST `?update=true`, proto `optional bool update`) runs Update + read atomically under the device lock. The standalone update routes / RPCs stay. **Whole-bank reads are removed** (never approved); `endpoint` is required (422 if missing). **Known limitation (Q-G):** a client cannot tell whether other clients ran `UpdateTriggerOuts` (or `UpdateWireOuts`) since its own last read, so a trigger can be cleared by someone else, or several updates may have happened between two of its reads. Only the future TriggerOut event log (D67 reopened) can fix this. |
| **D77** | Q-C (a) | **D68 revised.** Omitted `endpoints` = an exclusive resource claiming every **Wire and Trigger** EP (0x00–0x7F) only. Pipe / BTPipe EPs (0x80–0xBF) must be declared explicitly, because the server cannot tell a Pipe from a BTPipe at the same address. D23 still applies. |

---

## 2. Status of earlier items

### 2.1 Contradictions (⚠C) and gaps (❓G)

All C- and G-items are resolved. G18 closed by D51. Full history: rev 5
(commit `8b6ae7c`).

### 2.2 Gaps N1–N29

| ID | Status |
|---|---|
| N1–N15 | Resolved in rev 5 (D18–D31) |
| N16–N18, N20 | Resolved: D42, D43, D44, D46 |
| N19 | **Resolved:** D52 |
| N21 | **Resolved:** D53, superseded by D68 |
| N22 | **Resolved:** D54 |
| N23 | **Resolved:** D55 |
| N24 | **Resolved:** D56 (gate wording, §7 Q5) |
| N25 | **Resolved:** D57 |
| N26 | **Resolved:** D61 |
| N27 | **Resolved:** D62, D68 |
| N28 | **Resolved:** D63, D70 |
| N30–N32 | **Resolved:** D70, D71, D72 |
| N29 | **Resolved:** D64 |

---

## 3. Rules, open gaps and discrepancies

### 3.1 Normative rules (proposed for ADR-034)

- **R1 (registration, D6, D8, D22, D33, D48, D54, D61, D68).**
  `access_mode` is required, except that a resource without `endpoints` is
  exclusive (D68). An exclusive resource contains only exclusive items. A
  resource covers only its listed entries (D61). A new declaration that
  overlaps an existing one is allowed only if **both** are shared items in
  shared resources and agree on type and `private`. Otherwise 409, naming
  the existing resource and the conflicting item. A resource without
  `endpoints` claims all classic EPs exclusively; an explicit empty
  `endpoints` object claims none (D55). Address ranges claim only what is
  declared (D52). AXI interfaces on an `OK_DATAPORT_LEGACY` device → 422.
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

### 3.2 Open gaps

None. N30–N32 were resolved by D70–D72.

Also noted, no question needed:
- **Self-interleaving.** A user holding both ports can corrupt her own bridge
  transactions; the server does not protect a user from herself (guide §6.2).
- **Lock between unlocked steps.** If Bob locks between two unlocked calls by
  Alice, her next write is denied mid-sequence. This is why L1 exists
  (guide §4.3).
- **A bridge-EP-only resource** never reaches another resource's ranges:
  the ADR-025 address check uses the ranges of the **same** usable covering
  resource, and an empty range list → deny (D9). So a DualPort port
  resource must declare the shared bridge EPs **and** its own exclusive
  ranges, as guide §6.1 already shows. The old C-SFPP roadmap resource set
  (H-08, ports with no EPs) does not work and is corrected next round.
- **gRPC registration before P1.** Today's proto `CreateResourceRequest`
  has no `endpoints` field, so under D68 every gRPC registration would claim
  every classic EP. 44-B therefore lands after P1 (44-A), and gRPC uses the
  same validator as REST.

### 3.3 OpenAPI discrepancies (⚠S)

Discrepancies between `api/openapi.yaml` and the ADRs / owner decisions.
All content is approved; the spec is edited at implementation time, with an
explicit go-ahead per edit (ai-collaboration §2, §4). D63 grants that
permission for the dataport change.

| ID | Location | Discrepancy | Status |
|---|---|---|---|
| **S1** | `CreateResourceRequest` (~L2042) | Only `name`, `device_serial`, `description`, `resource_type`, `capabilities`. | **Approved** (D36, D61, D62, D68, D69): remove `resource_type` / `capabilities`; add `access_mode` (required unless `endpoints` is omitted), `interfaces`, `endpoints` (`default_access`, `entries[]`: `address`, `type`, `access`, `private`, `description`), `address_ranges[]` (`name`, `start`, `end`, `access`, `private`, `description`), `metadata` (string values only), `private` |
| **S2** | `Resource` (~L2065) | `access_mode` enum incl. `read_only`; `holders` is `string[]`; no `locked_by` or declaration fields. | **Approved** (D37) |
| **S3** | `createResource`, acquire, lock | Only 400/404/409 documented; no structured conflict body. | **Approved** (D38; 409/422 split per D72) |
| **S4** | port-A/B JSON (not a spec) | Descriptions vs v2 guide; metadata array (D69). | **Approved for C-SFPP-1** (D39, D69) |
| **S5** | `/events`, `DeviceEvent` | No lock events; no delete reason. | **Approved** (D58) |
| **S6** | `Resource.private` description | Visibility wording. | **Approved** (D57) |
| **S7** | `DeviceInfo.dataport` (~L1778), `GET …/dataport` (~L591) | `unknown`, `classic`; two different enums. | **Approved** (D63, D70): `legacy` / `axi` only; an UNKNOWN device is not listed (404) |
| **S8** | Acquire request `mode` (~L1263) | Required. | **Approved** (D62): optional; omitted = declared mode |

### 3.4 Proto discrepancies (⚠P) — `proto/okfpga/v1/okfpga.proto`, definition only

Line numbers approximate. **No enum gets an `UNSPECIFIED` value (D59).**
Presence is detected with proto3 `optional` fields.

| ID | Location | Discrepancy | Fix / status |
|---|---|---|---|
| **P1** | `CreateResourceRequest` (L902) | Same gaps as S1; comment cites ADR-016, not ADR-020. | **Approved** (D59, D65): fields 4–5 **reused** (D71: 4 = `access_mode`, 5 = `interfaces`), declaration messages `EndpointDeclaration`, `EndpointEntry`, `AddressRange`, `map<string,string> metadata` (D69), `private` |
| **P2** | `Resource` (L229) | No `access_mode`; `holders` is `repeated string`; no declarations. | **Approved** (D59): `ResourceAccessMode access_mode`, `repeated Holder holders` under a new field number, declarations |
| **P3** | `AccessMode` enum (L114) | `ACCESS_MODE_SHARED = 0` is the default, so an omitted mode silently means "shared". | **Approved** (D59, D62): `ResourceAccessMode { RESOURCE_ACCESS_MODE_SHARED = 0; RESOURCE_ACCESS_MODE_EXCLUSIVE = 1; }` on an `optional` registration field (absent → D68 or 422); acquire `mode` `optional` (absent → declared mode) |
| **P4** | `AccessMode` comment (L117–120) | Contradicts D42. | "cannot write input EPs or call ActivateTriggerIn; may call UpdateWireOuts / UpdateTriggerOuts (reads)" |
| **P5** | `UserSession` (L249) | No holder mode. | `repeated Holding` (`resource_id`, `mode`, `locked`) |
| **P6** | `DeviceEvent` (L280) | Missing lock events and REST parity. | Parity plus D58 |
| **P7** | Errors (`ErrorCluster`, L27) | No gRPC status mapping or structured detail. | Document status codes per RPC; error-detail message |
| **P8** | Identity | `x-user-id` metadata undocumented. | Document as required metadata |
| **P9** | `AXIDataPortService` comment (L1191) | "via okRegisterBridge" (D50, D64). | "…via FPGADataPortAXI (`OK_DATAPORT_AXI` devices only)" |
| **P10** | `LockResource` / `UnlockResource` (L1093) | Return `ResourceResponse`; ADR-020 §11 says `StatusResponse`. | Keep the proto |
| **P11** | `DeleteResource` comment (L1083) | "Returns an error if held". | Add the forced-delete note |
| **P12** | `ClearFPGAConfiguration` (L1021) | Not a forced cleanup. | D26 semantics |
| **P13** | `Resource.private` comment (L240) | Visibility claim. | **Approved** (D57) |
| **P14** | Address width | `uint32` vs `uint64`; ranges 32-bit. | AXI addresses above 32 bits cannot be covered and are denied under `enforce` |
| **P15** | Non-resource (noted only) | `GetServerStatus` duplicated. | Out of scope |
| **P16** | `DataportType` (L85) | `DATAPORT_UNKNOWN = 0`. | **Approved** (D63): `DATAPORT_LEGACY = 0; DATAPORT_AXI = 1;` an UNKNOWN device is not present (D70) |

### 3.5 ADR text superseded (for ADR-034)

| ADR | Text | Superseded by |
|---|---|---|
| ADR-025 §3 table | "In an Exclusive range \| Locked by this user → ALLOW"; "Not locked / locked by other → DENY" | D44 |
| ADR-025 §3 | "critical pattern: shared endpoints gating exclusive address ranges" (implies lock protection) | D44 (pattern stays; protection = ownership) |
| ADR-025 §5 flow (L191) | "In Exclusive range → User holds lock?" | B2 |
| ADR-025 Future Work (L250) | "Per-range lock semantics…" | Premise gone (D44) |
| ADR-024 §2 (L92) | "A locked resource can ONLY be used by the user who holds the lock." | B1 (writes only) |
| ADR-024 §2 (~L112–124) | "No explicit entry but `default_access` is defined → ALLOW" | D61: coverage = listed entries only |
| ADR-024 §3 | "legacy resource"; empty `entries` → `default_access` for all EPs | D68 (no special name); D55, D61 (empty = no EPs) |
| ADR-024 §6 (L314) | "state-disturbing operations (UpdateWireOuts, UpdateTriggerOuts)" | D42 |
| ADR-020 §3 (L171) | "Other holders … cannot perform operations…" | B1 (writes only) |
| ADR-020 §7 | No endpoints → "all endpoints in exclusive mode" | **Kept** (D68), plus `access_mode` defaulting |
| ADR-020 §7 | `default_access` applies to "any endpoint NOT explicitly listed" | D61 |
| ADR-020 §11 | Lock/Unlock return `StatusResponse` | Proto (P10) |
| ADR-003 L13, L37, L174–179 | okRegisterBridge described as the "AXI path" | D50, D64, T1 |

### 3.6 Reprogramming event order (D15b, D26, D58)

Under the device lock, atomically, for each resource on the device:
`ResourceUnlocked` (if locked) → release every holder → `ResourceDeleted`
(reason: `reprogram` / `clear`); then write the bitfile (or clear) and emit
the device event. All holders receive the events whatever their state.
`ResetFPGA` does **not** clear resources (D56, D66).

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
| 1 | Status, scope, authority (specs remain authoritative; S/P changes per D36–D38, D57–D59, D63) | — |
| 2 | Terminology (T1): resource, item, holder, holder mode, lock, blocked, overlap, private, exclusive item (E1), single-/multi-step, legacy = dataport only (D49), Classic vs direct AXI (D50), ADR-025 mapped vs register / direct address | ADR-003 wording (D64) |
| 3 | Resource model and registration schema, metadata (D69) | ADR-006 Resource Model; ADR-020 §1, §2, §6, §9; ADR-026 §2 |
| 4 | Declaration rules R1, omitted / empty `endpoints` (D55, D68), `default_access` (D61), ranges (D52), interfaces (D54), 409 vs 422 (D72) | ADR-020 §7, §8, §10; ADR-024 §3 |
| 5 | Acquisition A1, optional acquire mode (D62), ReadOnly states (D25) | ADR-006 Acquisition; ADR-020 §3 (part); ADR-026 §3, §4, §7 |
| 6 | Locking B1–B4, client obligation L1, events (D58) | ADR-020 §3, §11; ADR-024 §6 |
| 7 | Lifecycle: release, disconnect, heartbeat, device disconnect, reprogram / clear (D26, §3.6), delete (D15c), ResetFPGA (D56, D66) | ADR-006 Lifecycle; ADR-020 §4, §5, §10; ADR-023 Fix-F; ADR-024 §10 |
| 8 | Reads vs writes (D16, D21, D42), permission matrix (D44), triggers are the clients' business (D67) | ADR-024 §6; ADR-026 §1, §5 |
| 9 | Enforcement levels (D27), route tiers, pipeline (B2), errors and gRPC status mapping (D38, P7), identity metadata (P8) | ADR-024 §1, §2, §4, §5, §7, §9; ADR-026 §8 |
| 10 | Address enforcement: ADR-025 hook (compiled), register / direct AXI, ReadOnly (D12, D29), `AllowAlways` (D51) | ADR-025 §2, §3, §5, Future Work |
| 11 | Identity | ADR-029 (mark Accepted/Implemented) |
| 12 | Dataport mapping (D63, D70) | ADR-003 (dataport enum only) |
| 13 | Consequences, test requirements | — |
| 14 | Open items: RBAC (D47), server-side sequences (D40), address carriers | — |

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
falls away for it. Details are decided when the phase comes due.

---

## 6. Impact on the drafted plans

- **Server Phase 44** ([development-roadmap.md](development-roadmap.md)):
  updated to D61–D72. The ❓ markers for N26–N32, Q4, Q5, Q7, Q8 are gone;
  the per-holder trigger cache is removed everywhere, including Later
  (D67); a dashboard metadata item is added (D69). **Promoted to Now**
  2026-10-10 (rev 4: REST and gRPC together, 44-I folded into the other
  sub-phases, file lists and tests per sub-phase). Remaining questions: §7.
- **Client docs, next round** (agreed): LabVIEW C-17 draft
  (`docs/client/client-development-roadmap.md`, uncommitted),
  `client/python-DualPort-Transceiver-client/docs/client-sfpp-development-roadmap.md`,
  `ADR-C-SFPP-001`, plus `docs/client/hdl-resource-mapping-guide.md` (SD-08,
  owned by C-17). Strata listed about 45 passages with line numbers and
  one-line fixes (job `job_40c1d604`, §B and §C.2), to be checked and
  applied then. Main themes: declare-once, lock needed for exclusive items,
  `default_access` claiming EPs, the per-holder trigger cache, the ADR-035
  sequence endpoint, port resources without bridge EPs, metadata arrays.
- **DualPort-Transceiver C-SFPP-1:** two shared port resources, each declaring the
  shared bridge / Authenticator / GTY EPs **and** its own exclusive ranges
  (guide §6.1); `access_mode` explicit; `associated_cores` as one string
  (D69); lock around authentication and every bridge transaction.

---

## 7. Clarifying questions

No resource-model questions remain. Q44-3 (pipes → octet-stream) and Q44-5
(all REST and gRPC changes together) were answered on 2026-10-10. The
remaining implementation questions were answered the same day: Q44-13
(D73), Q44-14 (D74); go given for 44-A.

---

## 8. Local Strata second opinion

Job `job_40c1d604` (inputs about 75k tokens: both docs rev 7, the roadmap,
ADR-020, ADR-024, the proto, the LabVIEW and DualPort client roadmaps,
ADR-C-SFPP-001, the HDL mapping guide, the port-A JSON; max_tokens 98304,
reasoning 32768, `async_mode`). Two earlier submissions failed with an MCP
EOF before the IDE reload. Claims were checked against the files before
adoption. Output complete, no degeneration.

**Adopted:**
- ADR-024 §2 "no entry but `default_access` → ALLOW" also falls with D61
  (§3.5).
- `default_access` with empty `entries` is inert; entry without `access`
  and no default → 422 (D61).
- Where the UNKNOWN error surfaces, and the listing hole → N30 (resolved by
  D70: such a device is not listed).
- Field-number mapping → N31 (D71).
- Bridge-EP-only port resources cannot reach their ranges; gRPC
  registration must wait for P1 → §3.2 notes.
- Passage lists for the client docs, the HDL mapping guide and Phase 44
  (§6; Phase 44 applied now).

**Differs:**
- Strata kept `access_mode` required even when `endpoints` is omitted. You
  chose that it may be omitted there (D68).
- Strata offered "silently override `shared` to exclusive" as an option for
  `shared` + no `endpoints`; rejected (D68: 422).
- Strata asked whether port resources should declare both bridge EPs and
  ranges. That is already the guide §6.1 design (D4, D14), so no question.

# Resource Management — Owner Decision Log (D1–D78) & Pointer to ADR-034

> **Status:** Consolidated into [ADR-034 — Resource Model, Registration, Acquisition, Locking and Enforcement](architecture/adr/ADR-034-resource-model-consolidation.md) and the [API Specification PDF](API-Specification/ok-fpga-server-api-specification.pdf) (Phase 44-J, 2026-11-05).
> **Companion pointer:** [resource-management-developer-guide.md](resource-management-developer-guide.md)
> **Archive:** [archive/roadmap-phases-44.md](archive/roadmap-phases-44.md)

> [!IMPORTANT]
> **Model in one sentence (`D1`, `D8`, `D48`):** shared items declared by shared
> resources **may overlap**; every other overlap is denied to the later
> registration (first wins). There is **no "declare-once" rule**.
>
> All normative rules (`R1`, `B1`–`B4`, `L1`, `A1`, `E1`, `T1`), OpenAPI (`S1`–`S8`)
> and Proto (`P1`–`P16`) alignments, and ADR supersession details are now
> canonically specified in **[ADR-034](architecture/adr/ADR-034-resource-model-consolidation.md)**.
> This document retains the chronological owner decision log (`D1–D78`) and the
> server-side sequence design options (`D12`, `D40`) referenced from
> [development-roadmap.md](development-roadmap.md) (`🔵 Later`).

---

## 1. Owner decisions (2026-10-10)

### 1.1 Round 1–2 (D1–D17)

| ID | From | Decision |
|---|---|---|
| **D1** | Q-A | A lock applies to the **entire** resource. Overlapping **shared** declarations are allowed. Locking a resource already locked by someone else is denied. |
| **D2** | Q-B | A user acquires a given resource at most once (no duplicate holder entries). |
| **D3** | Q-C | Shared resources: many holders. Exclusive: one. ReadOnly for read-only users; Private flags at L1/L2/L3. |
| **D4** | — | Custom `ResourceAccessPolicy` implementations can map shared Classic `WireIn` endpoints (e.g. `0x00` write address, `0x01` read address) to 32-bit target addresses. |
| **D5** | Q1 | **Blocking (simplest rule):** if any shared item (EP or shared range) of resource S is also declared by a resource R that another user has locked, **S as a whole** is "locked by another user" for its holders until R is unlocked. |
| **D6** | Q2 | Shared items are allowed **only in shared resources**. An exclusive resource that declares a shared EP, a shared range or `default_access: shared` is rejected at registration. |
| **D7** | Q3 | A second acquire of the same resource by the same user (same mode) returns **idempotent 200**. |
| **D8** | Q4 | **First registration wins.** A later registration that conflicts is denied with a meaningful error. |
| **D9** | Q5 | The server does not dictate how resources are used; the HDL designer / software architect designs resources within the rules. The entity that programs the FPGA registers its resources. Alice writing Bob's exclusive range is denied, even when neither resource is locked. |
| **D10** | Q6 | ~~A per-EP `access: exclusive` inside a shared resource means "writable only by the locker"~~ **Corrected by D44:** "writable only by the **holders** of the declaring resource". |
| **D11** | Q7 | Expose `private` on EP entries and address ranges in the registration API. |
| **D12** | Q8 | The server stays FPGA-agnostic. ReadOnly does not apply to ADR-025 developer-defined classic address ranges; it applies to FPGADataPortAXI and okRegisterBridge ranges. Future work: server-side sequences for multi-step transactions and device initialization. |
| **D13** | Q9 | Bridge register maps are defined by the HDL design documentation. |
| **D14** | Q10 | Shared device-level supervisor endpoints are declared as **shared in both** port resources. Whichever port is acquired first initializes the device. |
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
| **D30** | Q11/N13 | The worked dual-port bridge design uses three transaction patterns: one-shot writes, burst writes (PipeIn 0x80) and burst reads (PipeOut 0xA0), with module status read from Status Register 2 (module base + 0x08). |
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
| **D39** | S4 | Draft port JSON resource declarations are maintained in downstream client repositories. |
| **D40** | §5 | When server-side sequences come due, the **embedded engine (option C)** will likely be explored first (extensibility; server-held secrets for device initialization). Decided at that phase. |
| **D41** | — | Downstream client packages live in their own repositories outside the server repository. |
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
| **D60** | D41 | Downstream client packages extracted to standalone repositories. |

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
| **D69** | Q8 | `metadata` stays **`map<string,string>`** (text values only; proto unchanged). Lists go into one string (e.g., `associated_cores` → comma-separated). The server's web dashboard shows each resource's metadata. |
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
| **D78** | Strata 0.3.0 #1 | **IsTriggered `mask` defaults to 0** on REST and gRPC (0.3.1). With no mask `triggered` is always false, forcing the client to set the bits it cares about. An all-ones default could look as if it works while reporting `triggered` for an unrelated bit. `vector` always carries the full 32 bits. |

---

## 2. Consolidated Specification Pointer

All pre-implementation gap tables (`C1–C7`, `G1–G18`, `N1–N32`), OpenAPI/proto discrepancy tables (`S1–S8`, `P1–P16`), and normative rule drafts (`R1`, `B1–B4`, `L1`, `A1`, `E1`, `T1`) were implemented across Phases `44-A` through `44-J` and consolidated into:

- **[ADR-034 — Resource Model, Registration, Acquisition, Locking and Enforcement](architecture/adr/ADR-034-resource-model-consolidation.md)**
- **[API Specification (`ok-fpga-server-api-specification.pdf`)](API-Specification/ok-fpga-server-api-specification.pdf)**
- **[Phase 44 Archive (`docs/archive/roadmap-phases-44.md`)](archive/roadmap-phases-44.md)**

---

## 5. Future work: server-side sequences (D12, D40)

**Today:** ADR-025 hooks are compiled Rust. A multi-step bridge one-shot write is
about 5 REST calls and a burst read about 6–7; clients lock around them
(`L1`).

| Option | What | Recompile? | Pros | Cons |
|---|---|---|---|---|
| A. Built-in design-specific ops | Custom `axi_write`, `axi_read`, `initialize` endpoints | Yes | Atomic, simple | Server becomes design-specific: violates D12 |
| B. Declarative sequences | Named JSON sequences run under the device mutex after one permission check | No | Generic, auditable, one round trip | Limited logic; handshake maths needs built-in operators |
| **C. Embedded engine (Rhai, Lua, WASM) — explore first (D40)** | Scripts from a server directory or the API, run under the device mutex after one permission check (B2) | No | Fully flexible; keys can stay server-side for device initialization | Sandbox, limits, security review, new dependency |
| D. Compiled trait hooks | ADR-025 style | Yes | Fast, type-safe | Per-bitfile logic built into the server |

A sequence/script call counts as **single-step**, so `L1`'s lock obligation
falls away for it. Details are decided when the phase comes due (see
[development-roadmap.md](development-roadmap.md) `🔵 Later`).

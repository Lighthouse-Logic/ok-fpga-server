# Resource Management — Findings & Recommendations (DRAFT rev 5)

> **Basis:** ADR-004, 005, 006, 015, 016, 017, 018, 020, 021, 023, 024, 025,
> 026, 029; the owner decisions of 2026-10-10 (§1); the DualPort-Transceiver
> `axibridge-access-guide-v2.md`; `api/openapi.yaml` (schemas only, for the
> S-items); local Strata second opinion (§8). No code was reviewed.
> Companion: [resource-management-developer-guide.md](resource-management-developer-guide.md).
> **Status:** Draft rev 5, 2026-10-10. Nothing is decided until the owner
> approves. No ADR, spec, roadmap or code has been changed.

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
| **D10** | Q6 | A per-EP `access: exclusive` inside a shared resource means "writable only by the locker" (like ADR-025 ranges). |
| **D11** | Q7 | Expose `private` on EP entries and address ranges in the registration API (spec change; permission needed at implementation time). |
| **D12** | Q8 | The server stays FPGA-agnostic. ReadOnly does not apply to ADR-025 developer-defined classic address ranges; it applies to FPGADataPortAXI and okRegisterBridge ranges. Future work: server-side sequences for multi-step transactions and FPGA authentication. |
| **D13** | Q9 | The bridge register map is in `client/python-dualport-client/docs/` (now `axibridge-access-guide-v2.md`, D30). |
| **D14** | Q10 | The Authenticator EPs are **shared in both** port resources. Whichever port is acquired first authenticates. |
| **D15** | Q11 | (a) A legacy resource with no `endpoints` claims **all EPs as exclusive**. (b) Any user may reprogram; reprogramming unlocks all, releases all, deletes all resources, then writes the bitfile. (c) Anyone may delete a resource until RBAC exists. (d) ADR-029 is implemented; its status is stale. (e) The enforcement default becomes **`enforce`**. (f) ADR-020 §3 gets ReadOnly behaviour; ADR-026 §4 is correct (allow unless private). |
| **D16** | Q12 | A read of a **non-private** item is permitted, including during someone else's lock. The guide gets a read/write permission matrix. A consolidated ADR may supersede the older resource ADRs. |
| **D17** | Q13 | No server restriction on reset EPs. Any holder of a resource that is not locked by another user (and not blocked) may write any WireIn the resource declares. Future RBAC *may* restrict this. |

### 1.2 Round 3 (D18–D35)

| ID | From | Decision |
|---|---|---|
| **D18** | Q1/N1 | Blocking is **non-transitive** (rule B4). |
| **D19** | Q2/N2 | A blocked resource is blocked **as a whole**: no writes at all through it, including its own exclusive ranges and direct AXI/okRegisterBridge paths. |
| **D20** | Q3/N3 | A **multi-step** transaction on a shared resource must run under the lock. A single-step (atomic) write to a shared item needs no lock. Until server-side atomic transactions exist, **every bridge transaction is multi-step**. |
| **D21** | Q4/N4 | **No state-change classification.** Reads (incl. PipeOut/BTPipeOut) are simply allowed per D16. A design that other users' reads could harm must declare the resource **exclusive** (and/or `private`). The guide gets an example (guide §5.4). |
| **D22** | Q5/N5 | `private` is part of an overlapping declaration: a mismatch gives **409** to the later registration (first wins). |
| **D23** | Q6/N6 | A legacy resource registered first blocks all later classic resources on the device. Intended. |
| **D24** | N7 | Authentication runs under the lock. |
| **D25** | Q7/N8 | ReadOnly with exclusive resources: proposals (a), (b), (c) accepted (§3.2 N8). |
| **D26** | N9 | Reprogramming is a **forced override** at every enforcement level, REST and gRPC alike. **All** holders on the device are informed, whatever their state (holding, locked, blocked, ReadOnly). Future RBAC will restrict who may reprogram. |
| **D27** | Q8/N10 | All clients must comply. `off` / `warn` exist only as **debugging aids** for non-compliant clients; the default is `enforce`. |
| **D28** | Q9/N11 | Same-mode repeat acquire = 200 (D7). A **different-mode** acquire is not a repeat but a change: it is **denied** with a message telling the client to release, then acquire. |
| **D29** | Q10/N12 | ReadOnly holders are denied ADR-025 mapped access. |
| **D30** | Q11/N13 | DualPort-Transceiver uses only **one-shot writes**, **burst writes** (PipeIn 0x80) and **burst reads** (PipeOut 0xA0), per `axibridge-access-guide-v2.md` (pipes §6, workflows §7). Authenticated status = **AXI Status Register 2, bit 7** (module base + 0x08). Field list in §3.4. |
| **D31** | Q12/N14 | Widen the RBAC Future-work item in the roadmap. *Edit deferred (no other documents this round).* |
| **D32** | Q13 | **One consolidated superseding ADR** (§4). |
| **D33** | Q14/C1 | `access_mode` is **required** at registration; a missing value returns a meaningful error. |
| **D34** | Q15 | Server-side **declarative sequences first** (future work, §5). |
| **D35** | Q16 | A user who holds both port resources may lock one and write the other's exclusive ranges: she owns both, and her own lock never blocks her. (Which lock is required: N20.) |

---

## 2. Status of earlier items

### 2.1 Contradictions (⚠C) and gaps (❓G)

| ID | Issue | Status |
|---|---|---|
| C1 | `access_mode` required vs default `shared` | **Resolved:** D33 (required). Spec still lacks the field (S1) |
| C2 | Accepted acquire `mode` values | D3: `read_only` accepted at acquire. Wording only |
| C3 | `InUseLocked` vs ReadOnly may join | **Resolved:** D15f, D25 |
| C4 | No `endpoints`: "all exclusive" vs "covers all" | **Resolved:** D15a, D23 |
| C5 / G2 / G7 | Lock reach across overlapping resources | **Resolved:** D1, D5, D18 |
| C6 | Order-dependent result | **Resolved:** D5 (B2) |
| C7, C9, C10, C12 | Stale ADR text | Fold into the consolidated ADR (D32) |
| C8 | Reprogramming while held | **Resolved:** D15b, D26 |
| C11 | L2/L3 Private not in the API | **Resolved:** D11 (spec change pending, S1) |
| G1 | Exclusive vs shared EP overlap | **Resolved:** D8 |
| G3 | Per-EP `access` has no runtime meaning | **Resolved:** D10 |
| G4 | State after ReadOnly acquire | **Resolved:** D25 |
| G5 | Explicit release by the locker | → `InUseShared` if other holders remain, else `Available` (unchanged) |
| G8 | Heartbeat needs identity in `off` mode | Moot: `off` is a debug aid (D27) |
| G9 | Who may delete | **Resolved:** D15c |
| G10 | ADR-029 status Proposed | **Resolved:** D15d (status update in the consolidated ADR) |
| G11 | Enforcement default `off` | **Resolved:** D15e, D27 |
| G12 | ReadOnly on exclusive | **Resolved:** D15f, D25 |
| G13 | Second acquire | **Resolved:** D7, D28 (status code: N17) |
| G14 | Bridge reads need writes; ReadOnly can't | **Resolved:** D12, D29 |
| G15 | Range ownership inside one shared resource | **Resolved:** D9, D35 |
| G16 | Multi-call bridge transactions | **Resolved:** D20 (lock). Sequences are future work (D34) |
| G17 | Exclusive resource with shared items | **Resolved:** D6 |
| G18 | `AllowAlways` must not skip lock/ReadOnly checks | Open, wording only (consolidated ADR §10) |
| G19 | Shared reset EPs | **Resolved:** D17 |

### 2.2 Round-2 gaps N1–N15

| ID | Issue | Status |
|---|---|---|
| N1 | Blocking transitivity | **Resolved:** D18 (B4) |
| N2 | Blocked = whole resource | **Resolved:** D19 |
| N3 | Writing your own exclusive range needs the lock | **Resolved for multi-step:** D20. Single-step writes to *exclusive* items: N18 |
| N4 | "State-changing reads" | **Resolved, proposal withdrawn:** D21. No classification; reads are reads; declare exclusive/private if reads could harm. Update\*Outs wording: N16 |
| N5 | `private` agreement on overlap | **Resolved:** D22 |
| N6 | Legacy resource blocks later classic resources | **Resolved:** D23 |
| N7 | Authentication under lock | **Resolved:** D24. The status check is itself a bridge read, so it also runs under the lock (§3.4) |
| N8 | ReadOnly + exclusive states | **Resolved:** D25: (a) exclusive holder + ReadOnly acquire → allowed unless private, stays `InUseLocked`; (b) only ReadOnly holders → `InUseShared`; (c) exclusive acquire while ReadOnly holders exist → allowed → `InUseLocked`. ReadOnly holders never count against "exclusive" |
| N9 | Reprogramming forced cleanup | **Resolved:** D26. Event order: §3.5 |
| N10 | Default `enforce` migration | **Resolved:** D27. All clients must comply; migration order still matters for Phase 44 sequencing (§6) |
| N11 | Different-mode repeat acquire | **Resolved:** D28 (status code: N17) |
| N12 | ReadOnly vs ADR-025 mapped access | **Resolved:** D29 |
| N13 | Register-map discrepancies | **Resolved:** D30 (v2 guide is authoritative). JSON fixes: S4 |
| N14 | RBAC scope | **Resolved:** D31. Roadmap edit pending (§6) |
| N15 | ADR-025 hooks are compiled | Noted. Changing a hook requires recompiling the server |

---

## 3. Rules, new gaps and discrepancies

### 3.1 Proposed normative rules

- **R1 (registration, D6, D8, D22, D33).** `access_mode` is required. An
  exclusive resource contains only exclusive items. A new declaration that
  overlaps an existing one is allowed only if **both** are shared items in
  shared resources and agree on type and `private`. Otherwise 409, naming
  the existing resource and the conflicting item. A legacy resource claims
  all EPs exclusively (D15a).
- **B1 (blocking, D5, D19).** Resource S is **blocked for user U** if another
  user V ≠ U holds the lock on a resource R ≠ S, and S and R declare at
  least one common shared item. S is also unusable for U if S itself is
  locked by V. A blocked or foreign-locked resource gives U **no write
  rights** through it, for any of its items. Reads of non-private items
  stay allowed (D16, D21).
- **B2 (write authorisation).** A write by U to item X is allowed if U holds
  at least one resource that covers X, is not ReadOnly, and is neither
  locked by another user nor blocked; then the ADR-025 / D10 checks apply.
  Because every resource that shares one of R's items is blocked, R's items
  can only be written by the locker. No order dependence.
- **B3 (lock denial).** `lock` on S is denied (409) if S is locked or blocked
  for the caller. A user's own lock never blocks that user (D35).
- **B4 (non-transitive, D18).** If R is locked, S overlaps R and T overlaps S
  but not R, then T is not blocked.
- **L1 (when to lock, D20, D24).** Multi-step transactions on a shared
  resource, including authentication and every bridge transaction, run
  under the client's lock. A single-step write to a **shared** item needs no
  lock. (Exclusive items: N18.)
- **A1 (acquire, D7, D28).** Same user, same resource, same mode → 200
  (idempotent). Same user, different mode → denied, "release, then
  acquire" (status code N17).

### 3.2 Open gaps (round 3, with Strata)

| ID | Issue | Proposal |
|---|---|---|
| **N16** | **`UpdateWireOuts` / `UpdateTriggerOuts`.** ADR-024 §6 and ADR-026 §1 list them as write-class. They only latch outputs, and REST `GET /wires/out` performs `UpdateWireOuts` + `GetWireOutValue` internally (ADR-004). Under D21 "reads are reads". | Treat both as **reads** (allowed during another user's lock and for ReadOnly, unless private). The consolidated ADR supersedes the ADR-024 §6 / ADR-026 §1 wording. |
| **N17** | **Status code for a different-mode acquire (D28).** | **409 Conflict**, body: "already held as `shared`; release, then acquire as `read_only`". It is a conflict with the caller's existing hold, not a permission failure. |
| **N18** | **Single-step write to an exclusive item.** D20 frees single-step writes on *shared* items from the lock. ADR-025 §3 and D10 make exclusive items "writable only by the locker". | Keep D10: exclusive items (per-EP `access: exclusive` or exclusive ranges in a shared resource) always need the lock, single-step or not. In an **exclusive resource** the sole holder is implicitly the locker, so no explicit lock is needed. |
| **N19** | **Legacy resources and address ranges.** D15a covers EPs. | A legacy resource claims **no** address ranges (it has no ADR-025 map). Later resources may still not declare EPs, so this is only relevant for direct AXI / okRegisterBridge ranges. |
| **N20** | **D35: which lock lets Alice write `port-b`'s exclusive ranges?** She holds both ports and locks `port-a`. | Under N18/ADR-025 §3 she must lock **`port-b`** (or both). Her lock on `port-a` does not block her `port-b`, but does not grant exclusive-write rights on it either. Alternative: any lock she holds on an overlapping resource suffices (more permissive, harder to state). |

### 3.3 Spec discrepancies (⚠S) — `api/openapi.yaml` vs ADRs/decisions

These are **discrepancies between the spec and the ADRs / owner decisions**.
Per the collaboration rules the spec stays authoritative and is **not
changed without owner permission**. Until permission is given, the server
implementation must match the spec, and the features below cannot be
exposed.

| ID | Location | Discrepancy | Proposed spec change |
|---|---|---|---|
| **S1** | `CreateResourceRequest` (~line 2042) | Has only `name`, `device_serial`, `description`, `resource_type`, `capabilities`; `required: [name, device_serial]`. Lacks `access_mode` (required, D33), `interfaces`, `endpoints` (`default_access`, `entries[]`: `address`, `type`, `access`, `private`, `description`), `address_ranges[]` (`start`, `end`, `access`, `private`, `name`), `metadata`, resource-level `private` (D11). `resource_type` / `capabilities` are not in ADR-020. | Add the ADR-020 / D11 / D33 fields; add `access_mode` to `required`; decide whether `resource_type` / `capabilities` stay (informational) or are deprecated. |
| **S2** | `Resource` (~line 2065) | `access_mode` enum `[shared, exclusive, read_only]` described as the "current access mode if acquired": conflates the resource's declared mode with a holder's mode. `holders` is `string[]`; ADR-026 requires entries with `user_id` + `mode`. Lacks `locked_by`, `interfaces`, `endpoints`, `address_ranges`, `metadata`. | `access_mode` = declared mode (`shared` / `exclusive`); `holders[]` = `{user_id, mode}`; add `locked_by` and the declaration fields. |
| **S3** | `createResource` responses; acquire/lock errors | Documents only 400/404/409. ADR-020 §10 uses 422 for validation. No documented error body for registration conflicts (resource + item), different-mode acquire (N17), or "blocked by lock on R" (B1). | Add 422 (or fold into 400, owner's choice) and a structured error body naming the conflicting resource/item or locking resource. |

The same audit is needed for `proto/okfpga/v1/okfpga.proto` before
implementation (not reviewed this round).

### 3.4 DualPort register map (⚠S4, D30)

`axibridge-access-guide-v2.md` is authoritative. Status Register 2 (module
base + 0x08):

| Module | Bits |
|---|---|
| Core and Trigger | Authenticated [7], Current State [4:1], txTriggerArmed [0] |
| Core only | Core Ready [8], txControlState [14:12], last_non_idle_address [31:16] |
| Trigger only | Trigger Armed [9], Triggered [10] |

Status Register 3 (+0x0C, Core) = capture's final block address pointer.
Reading these is a **burst read** through PipeOut 0xA0, i.e. a multi-step
bridge transaction under the client's own port lock (D20). ReadOnly users
cannot do it (D29).

**S4 — port JSON descriptions to fix (C-SFPP-1 work, not a spec item):**
WO 0x20 = bresp/rresp (JSON: "Status / Command Done"); WO 0x22/0x23 = TX/RX
timeout flags (JSON: "Read Data Low/High"); WO 0x31 = read counter (JSON:
"General Status"); WO 0x29 "Global Authenticated Status" is not the
authoritative status (Status Register 2 bit 7 is); BTPipeOut 0xA1 and large
mode (WI 0x04 count, TI 0x44 bit 1) are unused in 1.1. The v2 guide does not
cover the Authenticator / GTY / DRP EPs, so their JSON descriptions cannot be
verified from it.

### 3.5 Reprogramming event order (D15b, D26 — proposal)

Under the device lock, atomically, for each resource on the device:
`ResourceUnlocked` (if locked) → release every holder → `ResourceStateChanged
(deleted)`; then write the bitfile and emit the device event. All holders
receive the events whatever their state. Acquire/lock calls cannot
interleave. Confirm with the code review (ADR-023 Fix-F already clears
resources).

---

## 4. Consolidated ADR (decided, D32)

Working title: **"Resource Model, Registration, Acquisition, Locking and
Enforcement"**, next free number. It supersedes the sections below; the old
ADRs stay in the log, marked *Superseded by ADR-0NN §x*.

| § | Content | Supersedes / absorbs |
|---|---|---|
| 1 | Status, scope, authority (spec remains authoritative; S1–S3 need permission) | — |
| 2 | Terminology: resource, item, holder, holder mode, lock, blocked, overlap, private, single-/multi-step transaction, ADR-025 mapped vs direct address | — |
| 3 | Resource model and registration schema (`access_mode` required, `private` on EPs/ranges) | ADR-006 Resource Model; ADR-020 §1, §2, §6, §9; ADR-026 §2 |
| 4 | Declaration rules R1 (D6, D8, D22, D33), legacy = all EPs exclusive (D15a, D23, N19) | ADR-020 §7, §8; ADR-024 §3 |
| 5 | Acquisition A1 (D7, D28, N17), ReadOnly states (D25) | ADR-006 Acquisition; ADR-020 §3 (part); ADR-026 §3, §4, §7 |
| 6 | Locking B1–B4, L1 (D20, N18), events, exclusive = implicit lock | ADR-020 §3; ADR-024 §6 |
| 7 | Lifecycle: release, disconnect, heartbeat, device disconnect, reprogramming (D26, §3.5), delete (D15c) | ADR-006 Lifecycle; ADR-020 §4, §5, §10; ADR-023 Fix-F; ADR-024 §10 |
| 8 | Reads vs writes (D16, D21, N16) and the permission matrix | ADR-024 §6; ADR-026 §1, §5 |
| 9 | Enforcement levels, default `enforce`, `off`/`warn` as debug aids (D27), route tiers, pipeline, errors (S3) | ADR-024 §1, §2, §4, §5, §7, §9; ADR-026 §8 |
| 10 | Address enforcement: ADR-025 hook (compiled), direct AXI / okRegisterBridge, ReadOnly applicability (D12, D29), `AllowAlways` limits (G18) | ADR-025 §2, §3, §5 |
| 11 | Identity | ADR-029 (mark Accepted/Implemented) |
| 12 | Consequences, test requirements | — |
| 13 | Open items: RBAC (widened, D31), server-side sequences (D34), per-range leases | — |

---

## 5. Future work: server-side sequences (D12, D34)

**Today:** ADR-025 hooks are compiled Rust. A DualPort one-shot write is
about 5 REST calls and a burst read about 6–7; the client locks around them
(D20).

| Option | What | Recompile? | Pros | Cons |
|---|---|---|---|---|
| A. Built-in DualPort ops | `axi_write`, `axi_read`, `authenticate` endpoints | Yes | Atomic, simple | Server becomes DualPort-aware: violates D12 |
| **B. Declarative sequences (chosen first, D34)** | A named JSON sequence (set/update wires, trigger, poll WireOut with timeout, pipe read/write, `${param}`, return values), registered with the resources. The server runs it under the device mutex after one permission check against the caller's resources (B2, N18). | **No** | Generic, auditable, one round trip, becomes the "server-side atomic transaction" D20 anticipates | Schema + versioning; limited logic; auth maths needs a few built-in operators |
| C. Embedded engine (Rhai, Lua, WASM) | Scripts from a server directory or the API | No | Fully flexible; keys can stay server-side | Sandbox, limits, security review, new dependency |
| D. Compiled trait hooks | ADR-025 style | Yes | Fast, type-safe | Per-bitfile logic built into the server |

Path: **B first**, C later for authentication offload. Once B exists, a
sequence counts as single-step, so D20's lock requirement relaxes for it.

---

## 6. Impact on the drafted plans (all on hold until approval)

- **Server Phase 44:** consolidated ADR (§4). R1 with meaningful 409/422
  bodies. B1–B4, L1/N18. A1 incl. different-mode denial. Reads per D21/N16.
  Legacy = all EPs exclusive. Forced reprogram cleanup (§3.5). Default
  `enforce`. Spec changes S1–S3 (+ proto audit) **only with permission**.
  DualPort ADR-025 stub. ADR-029 status. Sequences stay Future (D34).
- **Roadmap:** widen the RBAC Future-work item (`docs/development-roadmap.md`
  line 330) to cover reprogramming, resource deletion and possibly reset EPs
  (D31). Deferred until the owner allows other documents to be edited.
- **LabVIEW C-17:** handle 403 (blocked / foreign lock / ReadOnly) and 409
  (lock denied, registration conflict, different-mode acquire). Send a user
  id everywhere. Lock around multi-step transactions.
- **dualport C-SFPP-1:** two shared port resources, bridge / Authenticator
  / GTY EPs shared, own ranges exclusive, `access_mode` explicit. Fix the
  JSON descriptions (S4). Lock around authentication, every bridge
  transaction and the Status Register 2 check. Never read PipeOut 0xA0
  outside its own locked transaction. Handle the reprogram events.

Next step after approval: a code review against this model.

---

## 7. Clarifying questions

1. **N16:** treat `UpdateWireOuts` / `UpdateTriggerOuts` as reads,
   superseding ADR-024 §6 / ADR-026 §1?
2. **N17:** a different-mode acquire returns **409**?
3. **N18:** single-step writes to **exclusive** items (in a shared resource)
   still require the lock?
4. **N19:** a legacy resource claims no address ranges?
5. **N20:** to write `port-b`'s exclusive ranges, must Alice lock `port-b`
   (proposed), or is her lock on `port-a` enough?
6. **S1–S3:** may the spec changes be planned for Phase 44 (applied only
   at implementation, with your go-ahead)? Keep or deprecate
   `resource_type` / `capabilities`? 422 or 400 for validation errors?
7. **D31:** may I widen the roadmap RBAC item now, or after these two
   documents are approved?

---

## 8. Local Strata second opinion

Job `job_f652958a` (rev 5 inputs: both docs rev 4, the owner's round-3
answers, ADR-020/024/025/026, `axibridge-access-guide-v2.md`, the port-A
JSON, `api/openapi.yaml` resource schemas). Claims were verified against the
files before adoption.

**Adopted:**
- Q3 vs D10 reading → N18.
- Update\*Outs are listed as write-class in ADR-024 §6 / ADR-026 §1 → N16.
- 409 for a different-mode acquire → N17.
- D35 lock question → N20.
- Legacy resources and ranges → N19.
- The spec gaps → S1–S3, each checked against `openapi.yaml`.
- The JSON vs v2 guide differences → S4.
- Reprogram event order → §3.5.
- The authentication check is a bridge read under the port's own lock;
  ReadOnly cannot do it (§3.4).

**Reversed from rev 4:** the "state-changing reads" classification (old N4,
also proposed by Strata in rev 4) is withdrawn per D21.

**Differs / deferred:** Strata suggests future atomic reads/writes on
exclusive ranges; deferred to the sequences work (§5). A raw PipeOut read by
a ReadOnly holder is allowed (not ADR-025 mapped access); Strata agrees.

**Earlier rounds:** rev 1 and rev 3 runs agreed with D1 and identified
G14–G17; the rev 4 run (`job_66b22b65`) produced N1–N13.

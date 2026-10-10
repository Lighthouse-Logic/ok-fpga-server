# Resource Management — Findings & Recommendations (DRAFT rev 4)

> **Basis:** ADR-004, 005, 006, 015, 016, 017, 018, 020, 021, 023, 024, 025,
> 026, 029; the owner decisions of 2026-10-10 (§1); the DualPort-Transceiver
> `axibridge-access-guide.md`; local Strata second opinion (§8). No code was
> reviewed.
> Companion: [resource-management-developer-guide.md](resource-management-developer-guide.md).
> **Status:** Draft rev 4, 2026-10-10. Nothing is decided until the owner
> approves. No ADR, spec, roadmap or code has been changed.

---

## 1. Owner decisions (2026-10-10)

| ID | From | Decision |
|---|---|---|
| **D1** | Q-A | A lock applies to the **entire** resource. Overlapping **shared** declarations are allowed. Locking a resource already locked by someone else is denied. |
| **D2** | Q-B | A user acquires a given resource at most once (no duplicate holder entries). |
| **D3** | Q-C | Shared resources: many holders. Exclusive: one. ReadOnly for read-only users; Private flags at L1/L2/L3. |
| **D4** | — | This branch's ADR-025 stub implements the DualPort-Transceiver bridge. Bridge EPs are shared. WI 0x00 = AXI write address, WI 0x01 = AXI read address. |
| **D5** | Q1 | **Blocking (simplest rule):** if any shared item (EP or shared range) of resource S is also declared by a resource R that another user has locked, **S as a whole** is "locked by another user" for its holders until R is unlocked. |
| **D6** | Q2 | Shared items are allowed **only in shared resources**. An exclusive resource that declares a shared EP, a shared range or `default_access: shared` is rejected at registration. |
| **D7** | Q3 | A second acquire of the same resource by the same user returns **idempotent 200**. |
| **D8** | Q4 | **First registration wins.** The first resource to declare an EP/range defines it. A later registration that conflicts is denied with a meaningful error. |
| **D9** | Q5 | The server does not dictate how resources are used. The HDL designer / software architect designs resources within the rules. The entity that programs the FPGA registers its resources. Alice writing Bob's exclusive range is denied, even when neither resource is locked. |
| **D10** | Q6 | A per-EP `access: exclusive` inside a shared resource means "writable only by the locker" (like ADR-025 ranges). |
| **D11** | Q7 | Expose `private` on EP entries and address ranges in the registration API (a spec change; permission is still needed at implementation time). |
| **D12** | Q8 | The server stays FPGA-agnostic. **ReadOnly does not apply to ADR-025 developer-defined classic address ranges.** It applies to FPGADataPortAXI and okRegisterBridge ranges. Future work: server-side scripts/sequences for multi-step transactions and FPGA authentication. |
| **D13** | Q9 | The bridge register map is in `client/python-dualport-client/docs/axibridge-access-guide.md`. |
| **D14** | Q10 | The Authenticator EPs are **shared in both** port resources. Whichever port is acquired first authenticates. |
| **D15** | Q11 | (a) A legacy resource with no `endpoints` claims **all EPs as exclusive** (closes the ADR-024 §3 gap). (b) Any user may reprogram; reprogramming unlocks all, releases all, deletes all resources, then writes the bitfile. (c) Anyone may delete a resource until RBAC exists. (d) ADR-029 is implemented; its status is stale. (e) The enforcement default becomes **`enforce`**. (f) ADR-020 gets an addendum extending §3 with ReadOnly behaviour; ADR-026 §4 is correct (allow unless private). |
| **D16** | Q12 | A read of a **non-private** item is permitted, including during someone else's lock. The guide gets a read/write permission matrix. A new consolidated ADR may supersede the older resource ADRs. |
| **D17** | Q13 | No server restriction on reset EPs. Any holder of a resource that is not locked by another user may write any WireIn the resource declares. Future RBAC *may* restrict this. |

---

## 2. Status of earlier contradictions (⚠C) and gaps (❓G)

| ID | Issue | Status |
|---|---|---|
| C1 | `access_mode` required vs default `shared` | Open (Q14) |
| C2 | Accepted acquire `mode` values | D3: `read_only` accepted at acquire. Wording only |
| C3 | `InUseLocked`: "no one may acquire" vs ReadOnly may join | **Resolved:** D15f (ADR-020 addendum) |
| C4 | No `endpoints`: "all exclusive" vs "covers all" | **Resolved:** D15a. It was a gap, not a contradiction |
| C5 / G2 / G7 | Lock reach across overlapping resources | **Resolved:** D1, D5 |
| C6 | Order-dependent result | **Resolved:** D5 (§3.1 rule B1) |
| C7, C9, C10, C12 | Stale ADR text | Fold into the consolidated ADR (§4) |
| C8 | Reprogramming while held | **Resolved:** D15b. Details in N9 |
| C11 | L2/L3 Private not in the API | **Resolved:** D11 (spec change pending) |
| G1 | Exclusive vs shared EP overlap | **Resolved:** D8 |
| G3 | Per-EP `access` has no runtime meaning | **Resolved:** D10 |
| G4 | State after ReadOnly acquire | Open (N8) |
| G5 | Explicit release by the locker | Presumably → `InUseShared` (unchanged) |
| G8 | Heartbeat needs identity in `off` mode | Largely moot under D15e |
| G9 | Who may delete | **Resolved:** D15c |
| G10 | ADR-029 status Proposed | **Resolved:** D15d (ADR status update needed) |
| G11 | Enforcement default `off` | **Resolved:** D15e (migration, N10) |
| G12 | ReadOnly on exclusive | **Resolved:** D15f (state rules N8) |
| G13 | Second acquire | **Resolved:** D7 (mode change, N11) |
| G14 | Bridge reads need writes; ReadOnly can't | **Resolved by scope:** D12. ReadOnly is not offered for ADR-025 ranges |
| G15 | Range ownership inside one shared resource | **Resolved:** D9. Ownership = which resource declares the range (N3) |
| G16 | Multi-call bridge transactions | Client takes the lock. Server-side sequences are future work (§5) |
| G17 | Exclusive resource with shared items | **Resolved:** D6 (cannot exist) |
| G18 | `AllowAlways` must not skip lock/ReadOnly checks | Open, wording |
| G19 | Shared reset EPs | **Resolved:** D17 (design responsibility) |

---

## 3. Rules that follow, and new gaps

### 3.1 Proposed normative rules

- **R1 (registration, D6/D8).** An exclusive resource contains only
  exclusive items. A new declaration that overlaps an existing one is
  allowed only if **both** are shared items in shared resources (and agree
  on type and `private`, N5). Otherwise 409, naming the existing resource
  and the conflicting item.
- **B1 (blocking, D5).** Resource S is **blocked for user U** if another
  user V ≠ U holds the lock on a resource R ≠ S, and S and R declare at
  least one common shared item. S is also unusable for U if S itself is
  locked by V. A blocked or foreign-locked resource gives U **no write
  rights** through it, for any of its items. Reads of non-private items
  stay allowed (D16).
- **B2 (write authorisation).** A write by U to item X is allowed if U holds
  at least one resource that covers X, is not ReadOnly, and is neither
  locked by another user nor blocked (then ADR-025 / D10 checks). Every
  resource that declares one of R's items is blocked, so R's items can never
  be written by anyone but the locker. That removes the old order
  dependence (C6).
- **B3 (lock denial).** `lock` on S is denied (409) if S is locked or blocked
  for the caller. Acquiring an **exclusive** resource cannot be blocked: by
  R1 it shares nothing with other resources.
- **B4 (non-transitive).** Blocking is **not** transitive. If R is locked,
  S overlaps R and T overlaps S but not R, then T is not blocked: none of
  R's items is in T (§3.2 N1).

### 3.2 New gaps (with Strata)

| ID | Issue | Proposal |
|---|---|---|
| **N1** | **D5 transitivity.** R locked; S overlaps R on item B; T overlaps S on C but not R. | B4: T is not blocked. C stays writable through T, but not through S. Simple, and matches "the lock reserves R's items; S is blocked because it touches them". Confirm (Q1). |
| **N2** | **D5 scope.** A blocked S is unusable for writes as a **whole**, including S's own exclusive ranges and any direct AXI/okRegisterBridge path to them. | That is what "whole resource" means. For DualPort this matches the single shared bridge. Documented in the guide (§4.3). Confirm (Q2). |
| **N3** | **Writing your own exclusive range needs the lock** (ADR-025 §3, D10). D9's wording ("denied even when neither resource is locked") suggests unlocked writes to your *own* range happen. Under D5, Alice's lock on port-a then blocks Bob's port-b for every bridge transaction she makes. | Keep ADR-025 §3 (lock required). The bridge needs serialising anyway: a transaction is several calls on shared EPs. **Q3.** |
| **N4** | **State-changing "reads".** `UpdateWireOuts`/`UpdateTriggerOuts` latch outputs. `PipeOut`/`BTPipeOut` **drain the FIFO**: Bob reading PipeOut 0xA0 during Alice's bridge read steals her data (guide §5, §6.3). ADR-024 §6 lists PipeOut/BTPipeOut as reads allowed during a lock. | Classify Update*Outs and PipeOut/BTPipeOut as **state-changing**, i.e. write-class for locking and ReadOnly. Only cached WireOut/TriggerOut values and direct AXI/okRegisterBridge reads count as "reads" under D16. **Q4.** |
| **N5** | **D8 + D11: `private` on overlapping shared declarations.** Port-a declares 0x20 shared non-private; port-b declares it shared **private**. | Treat `private` as part of the declaration: a mismatch is a conflict (409), and the first registration wins. Q5. |
| **N6** | **Legacy resources (D15a + D8).** A legacy resource registered first claims every EP exclusively, so **every later classic resource on that device is denied**. | Accept, and document loudly (guide §2.4). Q6 if this was not intended. |
| **N7** | **Authentication is multi-step on shared EPs** (WI 0x17, TI 0x4C, WO 0x29–0x2C, TO 0x60–0x63). Interleaving corrupts it. | The client locks its port around authentication. Under D5 that blocks the other port briefly, which is correct. Authenticated status is device-level hardware state. Checking it needs `UpdateWireOuts` (N4), so it waits while another user holds a lock. |
| **N8** | **ReadOnly on an exclusive resource (D15f).** Exact states: (a) exclusive holder present + ReadOnly acquire → allowed unless private, state stays `InUseLocked`; (b) only ReadOnly holders → state? (c) exclusive acquire while ReadOnly holders exist? | Proposal: ReadOnly holders never count against "exclusive". (b) state `InUseShared` (holders present, not locked). (c) allowed → `InUseLocked`. **Q7.** |
| **N9** | **Reprogramming forced cleanup (D15b).** Any user can force-unlock / release / delete other users' holds. | Specify: applies at every enforcement level and to REST and gRPC alike. It bypasses the "409 if held" delete rule. It emits `ResourceUnlocked`, then release, then `ResourceStateChanged(deleted)` per resource. It runs atomically under the device lock, so acquire/lock calls cannot race it. (ADR-023 Fix-F already clears resources.) |
| **N10** | **Default `enforce` (D15e)** breaks clients that send no user id (GUI demos, LabVIEW, tests). | Migration note in the consolidated ADR: client updates first, then flip the default. Q8 (timing). |
| **N11** | **D7 details.** A repeat acquire with a **different mode** (Shared ↔ ReadOnly)? | 200 with the current holder entry; the mode is unchanged. Changing mode = release + acquire. Q9. |
| **N12** | **D12 meaning.** "ReadOnly does not apply to ADR-025 ranges" = a ReadOnly holder cannot use ADR-025 mapped access at all (it needs input-EP writes). ReadOnly works for cached EP values and direct AXI/okRegisterBridge reads. | Documented in the guide. Confirm (Q10). |
| **N13** | **Register-map discrepancies**, `axibridge-access-guide.md` vs port-a/port-b JSON `description`s: WO 0x20 (bresp/rresp vs "Status / Command Done"); WO 0x22/0x23 (TX/RX timeout flags vs "Read Data Low/High"); WO 0x31 (read counter vs "General Status"); auth status WO **0x25** `Authenticator_1` vs **0x29** `Authenticator_0`; guide §5 omits BTPipeOut 0xA1 (used by TI 0x44 bit 1); the guide doesn't list WO 0x25–0x28 in the JSON, nor 0x2A–0x2C. Read data comes back through **PipeOut 0xA0**, not WireOuts. | Fix the JSON descriptions (C-SFPP-1 work). **Q11** (authoritative source; 0x25 vs 0x29). |
| **N14** | **RBAC scope.** The roadmap Future-work item (line 330) exists, but only restricts `ConfigureFPGA`. D15c and D17 point to resource deletion and maybe reset EPs as well. | Widen the roadmap item's wording. Not edited (outside this document's scope). Q12. |
| **N15** | **ADR-025 hooks are compiled.** `resolve_classic_address()` is a method on the Rust `ResourceAccessPolicy` trait in `ok-fpga-core` (ADR-025 §1, Files). | Adding or changing a hook **requires recompiling** the server. This answers the D12 question; see §5. |

---

## 4. Proposed consolidated ADR (D16)

Working title: **"Resource Model, Registration, Acquisition, Locking and
Enforcement"**. It would be the next free number; the dropped declare-once
"ADR-034" draft is retired. It supersedes the listed sections; the old ADRs
stay in the log, marked *Superseded by ADR-0NN §x*.

| § | Content | Supersedes / absorbs |
|---|---|---|
| 1 | Status, scope, authority (spec remains authoritative) | — |
| 2 | Terminology: resource, item, holder, holder mode, lock, blocked, overlap, private, passive read, state-changing op, ADR-025 mapped vs direct address | — |
| 3 | Resource model and registration schema (incl. `private` on EPs/ranges, D11) | ADR-006 Resource Model; ADR-020 §1, §2, §6, §9; ADR-026 §2 |
| 4 | Declaration rules: R1, first-wins (D8), D6, private agreement (N5), legacy = all exclusive (D15a) | ADR-020 §7, §8; ADR-024 §3 |
| 5 | Acquisition: shared / exclusive / ReadOnly, idempotent (D7), state table incl. N8 | ADR-006 Acquisition; ADR-020 §3 (part); ADR-026 §3, §4, §7 |
| 6 | Locking: preconditions, B1–B4, events, exclusive = implicit lock | ADR-020 §3; ADR-024 §6 |
| 7 | Lifecycle: release, disconnect, heartbeat/auto-release, device disconnect, reprogramming (D15b, N9), delete (D15c) | ADR-006 Lifecycle; ADR-020 §4, §5, §10; ADR-023 Fix-F (F3); ADR-024 §10 |
| 8 | Operation classes (N4) and the read/write permission matrix | ADR-024 §6; ADR-026 §1, §5 |
| 9 | Enforcement: levels, default `enforce` + migration (D15e, N10), route tiers, pipeline, errors | ADR-024 §1, §2, §4, §5, §7, §9; ADR-026 §8 |
| 10 | Address enforcement: ADR-025 hook (compiled, N15), direct AXI / okRegisterBridge, ReadOnly applicability (D12), `AllowAlways` limits (G18) | ADR-025 §2, §3, §5; ADR-024 §6 amendment |
| 11 | Identity | ADR-029 (mark Accepted/Implemented) |
| 12 | Consequences, test requirements | — |
| 13 | Open items: RBAC, server-side sequences, per-range leases (ADR-025 Future Work) | — |

Alternative, if the owner prefers fewer moves: amend ADR-020 (ReadOnly
addendum, D6, D8), ADR-024 (B1–B4, N4, legacy, default) and ADR-026 (L2/L3 in
the API) instead. The amendments would be scattered across three ADRs that
are already heavily amended (Q13).

---

## 5. Future work: server-side sequences (D12)

**Today:** ADR-025 hooks are compiled Rust (N15). A DualPort AXI write is
3–5 REST calls, and a read is about 6. The client must lock around them.

| Option | What | Recompile? | Pros | Cons |
|---|---|---|---|---|
| A. Built-in DualPort ops | `axi_write`, `axi_read`, `authenticate` endpoints | Yes | Atomic, simple for clients | Server becomes DualPort-aware: violates the agnostic principle |
| **B. Declarative sequences** | Client (or the registering client, per bitfile) uploads a named JSON sequence: set/update wires, trigger, poll WireOut with timeout, pipe read/write, `${param}` substitution, return values. The server runs it under the device mutex, after one permission check against the caller's resources. | **No** | Generic, auditable, sandboxed by construction, one round trip, works for LabVIEW/Python/gRPC | Needs a schema and versioning; limited logic; challenge-response maths needs a few built-in operators |
| C. Embedded engine (Rhai, Lua, WASM) | Scripts loaded from a server directory or registered via API | No | Fully flexible (e.g. Authenticator XOR maths, server-held keys) | Sandbox, resource limits, security review, new dependency |
| D. Compiled trait hooks | ADR-025 style | Yes | Fast, type-safe | Per-bitfile logic is built into the server |

Sketch of the recommended path: **B first** (closest to the planned ADR-035
"sequence" endpoint). Sequences are registered alongside resources and
checked like ordinary calls (B2, R1). **C later** for secure authentication
offload, where keys never leave the server. A needs no separate work if B
exists. Q15.

---

## 6. Impact on the drafted plans (all on hold until approval)

- **Server Phase 44:** consolidated ADR (§4). Registration checks R1, D6, D8
  and N5 with meaningful 409 bodies. Blocking B1–B4. Idempotent acquire (D7).
  Operation classes (N4). Legacy = all exclusive. Forced cleanup on
  reprogram (N9). Default `enforce` + migration. `private` on EPs/ranges
  (spec change → permission). DualPort ADR-025 stub. ADR-029 status. RBAC
  wording (N14). Server-side sequences stay Future (§5).
- **LabVIEW C-17:** handle 403 (blocked/foreign lock) and 409 (lock denied,
  registration conflict). Send a user id everywhere (default `enforce`).
- **dualport C-SFPP-1:** two shared port resources: bridge, Authenticator
  and GTY EPs shared; own ranges exclusive. Fix the JSON descriptions (N13).
  Lock around authentication and every bridge transaction. Check
  authenticated status before use. Retry on 409.

Next step after approval: a code review against this model.

---

## 7. Clarifying questions

1. **N1:** blocking is non-transitive (T is not blocked)?
2. **N2:** a blocked resource loses write access to its own exclusive
   ranges too, including direct AXI/okRegisterBridge paths?
3. **N3:** does writing your **own** exclusive range still require your
   lock (ADR-025 §3, D10)? If yes, every DualPort bridge transaction locks
   the port and briefly blocks the other port.
4. **N4:** treat `UpdateWireOuts`/`UpdateTriggerOuts` and
   `PipeOut`/`BTPipeOut` as state-changing (denied during another user's
   lock, and for ReadOnly)? ADR-024 §6 currently allows PipeOut reads during
   a lock.
5. **N5:** must overlapping shared declarations agree on `private`?
6. **N6:** confirm that a legacy resource registered first blocks all later
   classic resources on the device.
7. **N8:** ReadOnly with exclusive resources: proposed (a)/(b)/(c) OK?
8. **N10:** flip the default to `enforce` in Phase 44, or after the clients
   are migrated?
9. **N11:** repeat acquire with a different mode: 200 with the mode
   unchanged?
10. **N12:** ReadOnly holders are simply denied ADR-025 mapped access?
11. **N13:** which is authoritative, `axibridge-access-guide.md` or the
    JSONs? Is authenticated status WO 0x25 or 0x29?
12. **N14:** widen the roadmap RBAC item to cover resource deletion and
    (maybe) reset EPs? May I edit the roadmap?
13. **§4:** one consolidated superseding ADR, or amendments to
    ADR-020/024/026?
14. **C1:** is `access_mode` required at registration, or defaulted to
    `shared` (ADR-004 Add. F / ADR-005 Add. C)?
15. **§5:** declarative sequences first, embedded engine later?
16. **D9 ownership:** if Alice acquires **both** ports, may she lock
    `port-b` and write its exclusive ranges? (Assumed yes: ownership follows
    the acquisition, and keeping one application per port is the client
    architect's choice.)

---

## 8. Local Strata second opinion

Job `job_66b22b65`. Inputs: ADR-006/020/024/025/026, both docs (rev 3.1),
`axibridge-access-guide.md` and the port-A JSON, plus the owner's answers
(max_tokens 49152, reasoning 16384). Claims were verified before adoption.

**Adopted:** D5 transitivity hole (N1); whole-resource scope incl. direct
AXI (N2); the D9 vs ADR-025 §3 tension (N3); destructive pipe reads (N4,
checked against guide §5/§6.3); private agreement (N5); legacy blocking
(N6); authentication under lock (N7); ReadOnly + exclusive states (N8);
reprogramming details (N9); enforce migration (N10); acquire-mode change
(N11); D12 wording (N12); all register-map discrepancies (N13, each checked
against the files); the consolidated-ADR outline (merged into §4).

**Differs:**
- Strata recommends built-in atomic DualPort bridge/auth operations
  (option A) as the first step. That conflicts with the agnostic principle
  (D12), so this document prefers declarative sequences (B).
- Strata flags D17 (unrestricted resets) as unsafe. The owner has decided
  it is the architect's responsibility; recorded as such.
- Strata suggested a user could hold S and T and write the same item
  through one but not the other. Under B2 a write needs only one usable
  covering resource, and every resource declaring a locked item is blocked,
  so this is consistent.

**Earlier rounds:** rev 1 and rev 3 runs agreed with D1 (device-wide lock
reach) and identified G14–G17, now resolved by D5, D6, D9 and D12.

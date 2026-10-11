# ok-FPGA-Server — Development Roadmap

> **Status:** Living Document
> **Last Updated:** 2026-10-10 (Phase 44 updated to decisions D61–D69 — draft, not approved)
> **Architecture Decisions:** See [docs/architecture/overview.md](architecture/overview.md)
> **Archive:** Completed phases are in [docs/archive/](archive/)

---

## 📜 Roadmap Rules

1. **Three horizons** — The roadmap always maintains three sections: **Now**
   (active work), **Next** (planned milestones), and **Later** (future
   horizons). None of these sections may be dropped, even if empty.
2. **One "Now" at a time** — Only one phase should be in "Now" at any given
   time.
3. **Living document** — Update this file whenever plans change, phases
   complete, or new features are identified.
4. **Archive completed phases** — When a phase is completed, its detailed
   description must be moved to a file in `docs/archive/`. Follow these
   guidelines:
   - **Append** to the most recent archive file if the last 3 phases in
     that file are related to the newly completed phase (same feature area,
     same ADR family, or part of the same initiative).
   - **Create a new archive file** if the newly completed phase is
     substantially different from the phases in the most recent archive
     (e.g., infrastructure vs feature work, different ADR family).
   - **Consider upcoming work** — if the Now and Next phases are related
     to the newly completed phase, prefer creating a new archive that can
     later absorb those phases too, avoiding frequent file splits.
   - Archive files are named `roadmap-phases-NN-MM.md` where NN–MM is the
     range of phases contained.
   - Update the completed phases table in this file to link to the archive.
5. **No auto-promotion** — When a "Now" phase completes, do **not**
   automatically promote the first "Next" item to "Now". The user must
   explicitly request promotion.

---

## 🧭 Horizon Overview

| Horizon | Focus Area | ADR | Status |
|---------|-----------|-----|--------|
| **Phase 34** | Client UX & Device Metadata Improvements | ADR-004 (F), ADR-005 (C) | ✅ Completed |
| **Phase 35** | Client Demo Bug Fixes & README Update | — | ✅ Completed |
| **Phase 36** | Configure FPGA API Alignment | ADR-028 | ✅ [Archived](archive/roadmap-phases-36.md) |
| **Phase 37** | API Response Compliance Audit | — | ✅ [Archived](archive/roadmap-phases-37.md) |
| **Phase 38** | User Identification Consistency | ADR-029 | ✅ [Archived](archive/roadmap-phases-38.md) |
| **Phase 39** | SSE Event Expansion | ADR-030 | ✅ [Archived](archive/roadmap-phases-39.md) |
| **Phase 40** | MQTT Discovery, Report-by-Exception, Hostname Identity | ADR-011, ADR-031, ADR-032 | ✅ [Archived](archive/roadmap-phases-40-43.md) |
| **Phase 41** | Singleton Instance Guard & Startup Safety | ADR-033 | ✅ [Archived](archive/roadmap-phases-40-43.md) |
| **Phase 42** | mDNS Hostname Identity Alignment | — | ✅ [Archived](archive/roadmap-phases-40-43.md) |
| **Phase 43** | mDNS Stale Registration Cleanup & Hostname Sanitization | — | ✅ [Archived](archive/roadmap-phases-40-43.md) |
| **Phase 44** | Resource Model Consolidation & Enforcement Alignment | ADR-034 (proposed; supersedes parts of ADR-006/020/024/025/026) | 📝 Draft (Next) |

---

## 🟢 Now

*(Empty — no phase is currently in progress.)*

---

## 🟡 Next

### Phase 44 (DRAFT) — Resource Model Consolidation & Enforcement Alignment

> **Status:** 📝 Draft rev 3 (2026-10-10). **Not approved.** Rewritten from
> the 2026-10-08 issue list to the owner-agreed resource model. Promotion to
> Now needs explicit approval of this plan and answers to the open questions
> in 44.7.
> **Model source:** [resource-management-recommendations.md](resource-management-recommendations.md)
> (decisions D1–D69, rules R1/B1–B4/L1/A1/E1/T1) and
> [resource-management-developer-guide.md](resource-management-developer-guide.md).
> **Origin:** `python-DualPort-Transceiver-client` design review, 2026-10-08 (dual-port
> SFP+ DualPort design on the XEM8320).
> **Related plans:**
> [LabVIEW client Phase C-17](client/client-development-roadmap.md) ·
> [DualPort-Transceiver client Phase C-SFPP-1](../client/python-DualPort-Transceiver-client/docs/client-sfpp-development-roadmap.md)

> **Spec rule (ai-collaboration §2, §4):** sub-phases marked 🔒 change
> `api/openapi.yaml` and/or `proto/okfpga/v1/okfpga.proto`. The owner has
> approved the *content* listed (D36–D38, D57–D59, D61–D69; dataport permission D63); each spec file edit
> still gets an explicit go-ahead when the sub-phase starts. Items marked
> ❓ wait for an answer in 44.7. Until a spec is changed, it is treated as
> correct and the implementation is what gets fixed.

#### 44.0 The model being implemented (summary)

- **Overlap (R1, D1, D8, D48):** several resources on one device **may**
  declare the same EP or overlapping ranges when every overlap is a
  **shared item in a shared resource on both sides**, with the same type
  and `private` flag. Every other overlap is denied to the later
  registration (first wins), with a structured 409.
- **Exclusive resources contain only exclusive items** (D6). An exclusive
  item belongs to one resource and is writable by any non-ReadOnly holder of
  it (D44, E1).
- **Locks** cover the whole resource. A lock on R **blocks** every other
  resource that shares an item with R, for every user except the locker
  (B1, D5, D19). Blocking is non-transitive (D18).
- **The server never requires a lock to write** (D46). A write is allowed
  through any held, non-ReadOnly, usable (not foreign-locked, not blocked)
  resource that covers the item, then the address check runs against **that**
  resource's ranges (B2, D9). Multi-step operations are locked by the client
  (L1).
- **Reads** of non-private items are allowed to every holder, also during
  a foreign lock. `UpdateWireOuts` / `UpdateTriggerOuts` are reads (D16,
  D21, D42).
- **Reprogram / clear** is a forced override: unlock, release, delete,
  notify everyone, then program (D15b, D26). `ResetFPGA` keeps resources
  (D56).
- **Terminology (T1, D49, D50):** "legacy" is only the dataport
  `OK_DATAPORT_LEGACY` (every supported device today). A resource is never
  "legacy". A Classic resource may use okRegisterBridge, never direct AXI.
- **Enforcement default `enforce`** (D27); a user id is required.

#### 44.1 ADR

One new ADR, **ADR-034 — Resource Model, Registration, Acquisition, Locking
and Enforcement** (`docs/architecture/adr/ADR-034-resource-model-consolidation.md`;
outline in recommendations §4). It absorbs or supersedes ADR-006 (resource
model, acquisition, lifecycle), ADR-020 §1–§3 and §6–§11, ADR-024 §1–§7, §9,
§10, ADR-025 §2, §3, §5 and Future Work, ADR-026 §1–§5, §7, §8, and the
ADR-003 okRegisterBridge wording (N29); ADR-029 is marked
Accepted/Implemented. The old ADRs stay in the log, marked *Superseded by
ADR-034 §x*. ADR-034 is written in 44-A and added to
[architecture/overview.md](architecture/overview.md) in the **same commit**
as the code it documents; later sub-phases amend it with their commit
hashes. Required sections: Date, Commit hash, Status, Context,
Decision/Rationale, Files changed, Open items.

#### 44.2 Issues carried forward (from the 2026-10-08 review, revised)

Evidence references are from the 2026-10-08 code review; re-verify line
numbers when the sub-phase starts.

**Spec ↔ implementation**

| ID | Issue (evidence) | Resolution | Sub-phase |
|----|------------------|------------|-----------|
| S-01 🔒 | `CreateResourceRequest` spec has `resource_type` / `capabilities` only ([openapi.yaml:2042](../api/openapi.yaml)); the implementation needs `access_mode`, `interfaces`, `endpoints`, `address_ranges`, `metadata`, `private` (`rest/resources.rs`) | Spec to the declaration schema (D36, sketch 44.6); `resource_type` / `capabilities` **removed**; missing `access_mode` → 422, except a resource without `endpoints` (exclusive, D62, D68); metadata text-only (D69) | 44-A |
| S-02 🔒 | `Resource` response lacks `locked_by`, `metadata`, declarations; `holders` is `string[]`; `access_mode` includes `read_only` | `access_mode` = declared mode, `holders[] = {user_id, mode}`, `locked_by`, declarations (D37), in spec **and** `ResourceView` | 44-A |
| S-03 🔒 | REST ignores the acquire `mode` when the resource has a registered `access_mode` (`rest/resources.rs` `acquire_resource`); gRPC honours it (`grpc/device_service.rs`) | A1: same mode → 200, different mode → 409 "release, then acquire" (D7, D28, D43). Acquire `mode` optional, default = declared mode, `read_only` opt-in (D62). REST and gRPC identical | 44-C |
| S-04 🔒 | gRPC `RegisterResource` maps `resource_type` → `access_mode`; no declarations | Proto parity with S-01 (P1) | 44-A, 44-B |
| S-05 | Pipe and block-pipe routes are POST JSON/base64 (`rest/classic.rs:653–830`); spec is octet-stream | Fix to spec (Q44-3); update the Rust/Python example clients and LabVIEW client in the same phase | 44-G |
| S-06 | Register bulk routes take a single `{address, value}`; spec uses `addresses[]` / `entries[]` with `data` | Fix impl to spec | 44-G |
| S-07 | `WireInRequest.auto_update` missing (`rest/classic.rs:90`); `UpdateWireIns` always called | Implement `auto_update` | 44-G |
| S-08 🔒 | Undocumented routes: `wires/in/update`, `wires/out/update`, `triggers/out/update`, whole-bank `GET wires/out` / `triggers/out` | Document them; the two output updates are **reads** (D42), `wires/in/update` is a write | 44-A (spec), 44-G |
| S-09 | State / mode strings serialised as Display/Debug text (`"In Use (Shared)"`, `"InUseShared"`, `"Shared"`) | Serialise the spec enum strings everywhere (REST, SSE) | 44-G |
| S-10 🔒 | `dataport` enum: `[unknown, classic, axi]` ([openapi.yaml:1780](../api/openapi.yaml)) vs `[unknown, legacy, axi]` ([openapi.yaml:593](../api/openapi.yaml)) vs proto `UNKNOWN/LEGACY/AXI` | `legacy` / `axi` only; proto `DATAPORT_LEGACY = 0, DATAPORT_AXI = 1`; FrontPanel UNKNOWN → error (D63; listing ❓N30) | 44-H |

**Resource model and enforcement**

| ID | Issue (evidence) | Resolution | Sub-phase |
|----|------------------|------------|-----------|
| S-19 | Registration only rejects exclusive-vs-exclusive overlaps (`rest/resources.rs:388–428`) | Implement **R1**: shared-in-shared overlaps **allowed**; shared-vs-exclusive, exclusive-vs-anything, shared item in an exclusive resource, type or `private` mismatch → 409 naming the existing resource and item (D6, D8, D22, D38). Same code path for gRPC | 44-B |
| S-11 | A lock does not affect other resources that share an EP (F-2) | Implement **blocking B1–B4** (D5, D18, D19): a lock on R blocks every resource that shares an item with R, for all users except the locker | 44-D |
| S-13 | `endpoint_covered` is true whenever `endpoints` is present, so holding any resource grants every EP; a held resource locked by someone else falls through to the next held resource (`ok-fpga-core/src/resource.rs:1520–1600`) | Coverage = listed entries only (D61), or every classic EP for a resource without `endpoints` (D68). The "try every held resource" loop is **correct** under B2, but each candidate must pass the ReadOnly, foreign-lock **and blocking** checks; blocking closes the bypass | 44-E |
| S-14 | AXI `check_address_access` and the Classic ADR-025 path disagree on exclusive ranges (`resource.rs:1768+`) | One rule on both paths: no lock required (D44, D46); coverage, ReadOnly, foreign lock, blocking, and the same-resource range check | 44-E |
| S-12 | Enforcement off by default (`config.rs:463`, repo `config.toml:72`) | Default `enforce` (D27); `off` / `warn` are debugging aids | 44-E |
| S-15 | Block pipes, `classic/reset`, `*/update`, batch reads skip enforcement (`rest/classic.rs`) | Enforce all data-port routes. Updates of outputs = reads (D42); reset = no special rule, tier-3 data-port gate only (D56, D66) | 44-E |
| S-18 | `resolve_user_id` falls back to `"Unknown"` (`rest/mod.rs`) | Under `warn` / `enforce`, a missing identity is rejected (D27) | 44-E |
| S-20 | ADR-025 `MapsToAddress` returns Ok when the covering resource has no ranges (`resource.rs`, ADR-025 branch) | Address must be in the ranges of the **same** usable covering resource; empty ranges → deny (D9, B2) | 44-E |
| S-22 | Exclusive acquire sets `InUseLocked` without `locked_by`; range checks then deny the sole holder | Writes no longer check `locked_by` (D44, D46). Show the implicit lock (`locked_by` = holder) for display only. Verify with a test | 44-C |
| S-23 | `private` cannot be set over REST; `create_resource` hard-codes `private: false` (`rest/resources.rs:30–95, :355, :384`) | Add and store `private` on resource, EP entries and ranges (D11) | 44-B |
| S-24 | `metadata` is `HashMap<String,String>` (non-string values fail the request); range `name` / `description` dropped; omitted `access` defaults to exclusive (`rest/resources.rs:40, :87–95`) | Store range `name` / `description`; metadata stays text-only `map<string,string>` (D69), so a non-string value is a 422 with a clear message; entry without `access` and without `default_access` → 422 (D61) | 44-B |
| S-17 | `configure` neither checks nor invalidates resources (`rest/devices.rs` `configure_fpga`) | Forced cleanup per D26 for configure **and** clear, REST and gRPC: unlock → release → delete (reason) → program; notify every holder | 44-F |

> **Why the Rust GUI demos appear to obey locks:** both `rust-rest-gui-iced`
> and `rust-grpc-gui-iced` grey out write controls client-side when
> `locked_by != me`, and both act on the **same** resource. With
> `endpoint_enforcement = "off"` the server is not involved.

**Documentation**

| ID | Doc | Resolution | Sub-phase |
|----|-----|------------|-----------|
| SD-01 🔒 | `api/openapi.yaml:23` port 8080 | 8088 | 44-A |
| SD-02 | [architecture/overview.md](architecture/overview.md): duplicated ADR table, missing ADR-033, stale statuses, wrong crate names | Clean up; add ADR-034 | 44-A, 44-J |
| SD-03 | Access-control docs / ADR-024 | Rewrite for the overlap + blocking model, B2 and the read/write matrix (guide §5.3) | 44-J |
| SD-04 | WireIn read-back (F-8) | Document: FrontPanel has no WireIn read-back; clients cache written values | 44-J |
| SD-05 | `dataport` meaning | `legacy` = `OK_DATAPORT_LEGACY` transport only; dataport does not decide whether classic EPs exist | 44-H |
| SD-06 | [guides/python-rest-client-guide.md](guides/python-rest-client-guide.md) §5 | Align configure flow with ADR-028 | 44-J |
| SD-07 | [ADR-020](architecture/adr/ADR-020-client-side-resource-registration.md) §7–§8, Errors table | Marked superseded by ADR-034 §3–§4 (shared overlap **kept**; validation rules per R1; stale "no runtime enforcement (v1)") | 44-A |
| SD-08 | `docs/client/hdl-resource-mapping-guide.md` §4, §6 rule 3 | Already teaches shared overlap; align with R1, omitted `endpoints` = exclusive resource (D68), empty `endpoints` (D55), `default_access` (D61), blocking; fix the "16-bit wire data" remark (wire width is device-dependent). Owned by LabVIEW C-17 | 44-J |
| SD-09 | `rest/resources.rs` `create_resource` doc comment | Interface validation: `classic` always; `axi_*` only on `OK_DATAPORT_AXI` devices; mixing allowed (D54) | 44-B |

#### 44.3 Sub-phases

Each sub-phase: tests written with the code (TDD), `cargo test` green, one
commit (`feat:` / `fix:` / `docs:`), version bump in `Cargo.toml` (and
`pyproject.toml` where a Python client changes) in the same commit, ADR-034
amended. Test names follow `test_{function}_{scenario}_{outcome}`.
Tier 1 = `okREALM_TEST` (CI), Tier 3 = FFI-trait mocks, Tier 2 = hardware
(`#[ignore]`, run only when you approve).

**44-A Contract and ADR-034 🔒**
- ADR-034 (first version) + overview.md entry; SD-01, SD-02, SD-07.
- OpenAPI: S1 (S-01), S2 (S-02), S3 structured 409/422 bodies, S5 lock
  events + delete reason (D58), S6 private wording (D57), S-08 routes.
- Proto: P1 (fields 4–5 **reused**, D65; proposed `access_mode = 4`,
  `interfaces = 5`, ❓N31), P2, P3 (`optional`, **no UNSPECIFIED**, D59,
  D62), P4, P6, P8, P13.
- Core domain types: declarations, holder `{user_id, mode}`, declared vs
  holder mode, structured conflict errors.
- Tests (Tier 3): `test_create_resource_request_full_schema_roundtrips`,
  `test_resource_view_holders_include_mode`,
  `test_conflict_error_body_names_resource_and_item`.
- Commit: `feat: ADR-034 resource contract and declaration types`.

**44-B Registration (R1)**
- Overlap rules (S-19), rule A (exclusive → only exclusive items), first
  wins, `private` / type mismatch (D22), resource without `endpoints` = exclusive, all classic EPs (D15a, D23, D68),
  explicit empty `endpoints` = no EPs (D55), ranges claim only what is
  declared (D52), interfaces (SD-09, D54: `axi_*` → 422 on
  `OK_DATAPORT_LEGACY`), `default_access` = default `access` only (D61), missing `access_mode` → 422
  except without `endpoints` (D62, D68), 409 vs 422 (❓N32), `private`
  fields (S-23), range name/description and text-only metadata (S-24, D69). gRPC `RegisterResource` uses the same validator (S-04).
  The web dashboard resource view shows each resource's metadata
  key/value pairs (D69; verify what it shows today).
- Tests (Tier 3): `test_create_resource_shared_overlap_allowed`,
  `test_create_resource_shared_vs_exclusive_overlap_denied`,
  `test_create_resource_exclusive_with_shared_item_denied`,
  `test_create_resource_private_mismatch_denied`,
  `test_create_resource_no_endpoints_blocks_later_classic`,
  `test_create_resource_no_endpoints_no_access_mode_exclusive`,
  `test_create_resource_no_endpoints_shared_rejected`,
  `test_create_resource_default_access_does_not_claim_unlisted`,
  `test_create_resource_entry_without_access_rejected`,
  `test_create_resource_metadata_non_string_rejected`,
  `test_create_resource_empty_endpoints_claims_none`,
  `test_create_resource_axi_on_legacy_dataport_rejected`,
  `test_create_resource_missing_access_mode_rejected`. Tier 1: registration conflicts on an `okREALM_TEST` device.
- Commit: `feat: enforce resource registration overlap rules (R1)`.

**44-C Acquisition (A1)**
- Holder entries with mode; same mode → 200; different mode → 409 (S-03);
  optional acquire mode, omitted = declared mode (D62); ReadOnly rules incl. exclusive resources
  (D25); implicit lock display for exclusive resources (S-22).
- Tests (Tier 3): `test_acquire_same_mode_idempotent`,
  `test_acquire_different_mode_conflict`,
  `test_acquire_omitted_mode_uses_declared_mode`,
  `test_acquire_readonly_private_denied`,
  `test_acquire_exclusive_with_readonly_holders_allowed`,
  `test_acquire_exclusive_holder_writes_without_lock_allowed`. Tier 1:
  acquire/release flow.
- Commit: `feat: resource acquisition holder-mode rules (A1)`.

**44-D Locking and blocking (B1–B4) + events**
- Lock preconditions (holder, not ReadOnly), 409 if locked or blocked for
  the caller (B3), whole-resource lock, blocking computation (B1, B4),
  own lock never blocks self (D35). Emit `ResourceLocked` /
  `ResourceUnlocked` to holders of the resource **and** of the resources it
  blocks.
- Tests (Tier 3): `test_lock_blocked_resource_conflict`,
  `test_blocking_shared_item_blocks_other_resource`,
  `test_blocking_non_transitive`, `test_blocking_own_lock_not_blocked`,
  `test_unlock_notifies_blocked_holders`. Tier 1: two users, two
  overlapping shared resources.
- Commit: `feat: resource lock blocking and lock events (B1-B4)`.

**44-E Enforcement pipeline (B2)**
- Default `enforce` (S-12), identity required (S-18), route tiers and every
  data-port route enforced (S-15), read/write classes (D42), candidate
  filtering (ReadOnly, foreign lock, blocked; S-13), no lock requirement
  (D44, D46, S-14), same-resource address check with empty ranges → deny
  (S-20), `AllowAlways` skips only the address check (D51), ReadOnly denied
  ADR-025 mapped access (D29), private reads, ResetFPGA via the data-port
  gate only (D56, D66), AXI addresses above 32 bits denied (P14).
- Tests (Tier 3): `test_enforce_write_usable_holder_allowed`,
  `test_enforce_write_foreign_lock_denied`,
  `test_enforce_write_blocked_denied`,
  `test_enforce_read_during_foreign_lock_allowed`,
  `test_enforce_update_wire_outs_readonly_allowed`,
  `test_enforce_mapped_address_other_resource_range_denied`,
  `test_enforce_mapped_address_empty_ranges_denied`,
  `test_enforce_allow_always_still_checks_blocking`,
  `test_enforce_missing_identity_rejected`. Tier 1: enforcement matrix on
  `okREALM_TEST`. Tier 2 (approval): DualPort bridge with two users.
- Commit: `feat: align endpoint enforcement with overlap and blocking (B2)`.

**44-F Reprogram and clear cleanup (D26)**
- `ConfigureFPGA`, `ConfigureFPGAFromMemory`, `ClearFPGAConfiguration`
  (REST and gRPC): under the device lock unlock all → release all → delete
  all (`ResourceDeleted` reason `reprogram` / `clear`) → program / clear;
  every holder notified. `ResetFPGA` does not clear (D56). P11, P12
  comments.
- Tests (Tier 3): `test_configure_fpga_held_resources_force_deleted`,
  `test_clear_fpga_notifies_all_holders`,
  `test_reset_fpga_resources_retained`. Tier 1: reprogram cleanup.
- Commit: `fix: forced resource cleanup on configure and clear`.

**44-G Classic data-path spec alignment**
- S-05, S-06, S-07, S-08 (implementation), S-09; example clients (Rust,
  Python) and LabVIEW client updated in the same phase (version bumps).
- Tests: Tier 3 route request/response shapes; Tier 1 pipe and register
  round-trips; Tier 2 (approval) pipe throughput.
- Commits: one per item (`fix: align classic pipe routes with OpenAPI`, …).

**44-H Dataport normalisation 🔒 (D63; permission granted)**
- S-10, SD-05, S7, P16: public `legacy` / `axi`; `okEDataport` mapping;
  `OK_DATAPORT_UNKNOWN` → error: single-device REST calls 500
  problem+json, gRPC `ErrorCluster` error, `axi_*` registration 422;
  device listing per ❓N30; client models.
- Tests (Tier 3): `test_dataport_legacy_maps_to_legacy`,
  `test_dataport_axi_maps_to_axi`, `test_dataport_unknown_returns_error`.
- Commit: `fix: normalise dataport enum to legacy and axi`.

**44-I Remaining proto alignment 🔒**
- P5 (holdings with mode), P7 (gRPC status mapping + error detail), P9
  (direct AXI vs okRegisterBridge wording), P10 (keep proto lock
  responses), P14 (address width note).
- Tests (Tier 3): `test_grpc_conflict_maps_to_failed_precondition` (and the
  other mappings), `test_user_session_holdings_include_mode`.
- Commit: `feat: align proto sessions, errors and comments`.

**44-J Documentation**
- SD-03, SD-04, SD-06, SD-08; final ADR-034 status; the two
  resource-management docs reduced to pointers into ADR-034.
- Commit: `docs: consolidate resource model documentation`.

#### 44.4 Acceptance criteria

1. Shared items in shared resources may overlap; every other overlap → 409
   naming the existing resource and item.
2. Exclusive resources with shared items are rejected.
3. `default_access` only fills in a missing `access` on listed entries; it
   never claims unlisted EPs.
4. Omitted `endpoints` = exclusive resource claiming every classic EP
   (`access_mode` optional there, `shared` → 422); `{"entries": []}` = no
   EPs.
5. Resources claim only the ranges they declare.
6. `axi_*` interfaces are rejected on `OK_DATAPORT_LEGACY` devices; classic
   and AXI may be combined on `OK_DATAPORT_AXI` devices.
7. Missing `access_mode` → 422 (except item 4); acquire `mode` optional;
   no `UNSPECIFIED` value in either
   spec.
8. Acquire: same mode 200, different mode 409.
9. A lock blocks every overlapping resource for everyone but the locker;
   blocking is non-transitive.
10. No write requires a lock; foreign-locked or blocked resources give no
    write rights; reads of non-private items stay allowed.
11. `UpdateWireOuts` / `UpdateTriggerOuts` are reads.
12. ReadOnly cannot use ADR-025 mapped access.
13. Address checks use the same covering resource's ranges; empty → deny.
14. `AllowAlways` skips only the address check.
15. Configure / clear force-delete all resources and notify all holders;
    reset does not.
16. Default enforcement is `enforce`; identity is required.
17. Lock / unlock / delete-reason events exist in REST and gRPC.
18. Dataport enum is `legacy` / `axi` only.
19. OpenAPI, proto and implementation agree; REST and gRPC behave the same.
20. ADR-034 is committed with the code it documents and listed in
    overview.md.

#### 44.5 Moved to Later (see 🔵 Later)

- **S-21 declarative address carriers** (old Q44-9): the compiled ADR-025
  DualPort stub stays (D4).
- **Classic sequence endpoint** (old 44.3 proposal): replaced by
  the server-side sequences future work, embedded engine first (D40).
- **WireIn shadow read-back** (old Q44-6).
- **Atomic multi-resource acquire** (old Q44-12).

Dropped: the 2026-10-08 ADR proposals for this phase, old Q44-7
(superseded by D48, D68) and old S-16, the per-holder TriggerOut cache
(**rejected permanently**, D67: clients manage trigger access). ADR-034 now means the consolidated ADR (44.1).

#### 44.6 F-1 spec sketch (for S-01/S-02 — not applied)

```yaml
CreateResourceRequest:
  required: [name, device_serial, interfaces]   # access_mode required when endpoints present (D62, D68)
  properties:
    name: {type: string}
    description: {type: string}
    device_serial: {type: string}
    access_mode: {type: string, enum: [shared, exclusive]}
    private: {type: boolean, default: false}
    interfaces:
      type: array
      items: {type: string, enum: [classic, axi_lite, axi_full, axi_stream]}
    endpoints:                     # omitted = exclusive, all classic EPs (D68); {entries: []} = no EPs
      type: object
      properties:
        default_access: {type: string, enum: [shared, exclusive]}   # default for entries without access (D61)
        entries:
          type: array
          items:
            type: object
            required: [address, type]    # access required unless default_access is set (D61)
            properties:
              address: {type: string, pattern: '^0x[0-9A-Fa-f]{2}$'}
              type: {type: string, enum: [wire_in, wire_out, trigger_in, trigger_out,
                                          pipe_in, pipe_out, bt_pipe_in, bt_pipe_out]}
              access: {type: string, enum: [shared, exclusive]}
              private: {type: boolean}
              description: {type: string}
    address_ranges:
      type: array
      items:
        type: object
        required: [start, end, access]
        properties:
          name: {type: string}
          start: {type: string}
          end: {type: string}
          access: {type: string, enum: [shared, exclusive]}
          private: {type: boolean}
          description: {type: string}
    metadata: {type: object, additionalProperties: {type: string}}   # text only (D69)
# Resource (response): access_mode (declared), holders[{user_id, mode}],
#   locked_by, interfaces, endpoints, address_ranges, metadata, private
# 409 = invalid overlap (R1), lock denied, different-mode acquire
# 422 = validation (missing field, type/address mismatch, axi_* on legacy dataport)
```

#### 44.7 Open questions

Kept from the 2026-10-08 list:
- **Q44-1** Spec edits: content approved (D36–D38, D57–D59, D61–D69); confirm each
  🔒 sub-phase at its start.
- **Q44-3** Pipes (S-05): fix the implementation to the spec's octet-stream
  form (recommended) or change the spec to JSON/base64?
- **Q44-5** REST and gRPC in the same sub-phases (recommended)?

New (recommendations §7):
- **N30** `OK_DATAPORT_UNKNOWN` in the device list: show the device without
  a `dataport` value (recommended), leave it out, or add an error field?
- **N31** Reuse field 4 for `access_mode` and 5 for `interfaces`?
- **N32** 422 for every error visible from the request alone, 409 only for
  clashes with existing state?

Resolved: Q44-2 (D28, D43, D62), Q44-4 (D27), Q44-8 (D61), Q44-10 (D69),
Q44-11 (D50, D54); round 6: N26 (D61), N27 (D62, D68), N28 (D63),
Q4 (D65), Q5 (D66), Q7 (D67, rejected), Q8 (D69). Dropped: Q44-7. Moved to Later: Q44-6, Q44-9, Q44-12.

---

## 🔵 Later: Future Horizons

Features in this section represent uncommitted research or long-term capabilities:

* **gRPC AXI FFI wiring** — all 14 AXI RPCs in `AXIDataPortService` (Lite, Full,
  Stream) are `UNIMPLEMENTED` stubs. REST AXI is fully wired, but gRPC was not
  connected because no hardware with `FPGADataPortAXI` (Gen3-USB /
  `OK_DATAPORT_AXI`) is available for testing. Implement and test when a
  Gen3-USB device is accessible.

* **okREALM_TEST simulation mode** — explore FrontPanel's `okREALM_TEST`
  capability for testing without a physical device. Would enable CI test
  coverage in environments with no hardware access.

* **Hardware integration verification pass** — verify all server endpoints
  against a live server with connected hardware. Includes gRPC reflection,
  REST status/services endpoints, mDNS discovery, and running all 4 example
  client libraries (Rust REST, Rust gRPC, Python REST, Python gRPC) end-to-end.

* **Server-side static resources (bitfile-aware auto-registration)** — imbue
  the server with knowledge of specific bitfiles' capabilities so that when a
  known bitfile is programmed, the server automatically creates the appropriate
  resources without client involvement.

* **In-use protection on re-program** — *superseded by D15b/D26:*
  reprogramming is a forced override that unlocks, releases and deletes all
  resources and notifies every holder (Phase 44-F). Restricting **who** may
  reprogram is part of **Role-based access control** below.

* **Read-only resource acquisition** — **Completed in Phase 30.**
  See [ADR-026](architecture/adr/ADR-026-readonly-acquisition.md) and
  [archive](archive/roadmap-phases-30.md).

* **Native TLS termination & certificate-based identity** — embedded `rustls` for
  secure lab deployments without a reverse proxy. When mutual TLS (mTLS) is enabled,
  user identity can be extracted from the client certificate's CN/SAN fields,
  providing cryptographically verified identity without requiring a login flow.
  The `resolve_user_id()` chain (ADR-029) is designed to be extensible: a future
  TLS phase would add certificate identity as the highest-priority source
  (cert CN → body `user_id` → `X-User-Id` header → "Unknown"). This applies to
  both REST (HTTPS) and gRPC (gRPC-TLS channel credentials). See ADR-029 §Future
  Considerations for details.

* **Persistent resource definitions** — save resource registrations to disk so
  they survive server restarts. This would have to do a quick state save of 
  Users, Resources, connected devices and all info about the device (is it
  programmed, with what bitfile and description, by what user) and this would 
  have to be reloaded and cleared after a "restart" (add that option to the 
  Service Management page on the Web Dashboard?)  - There's no benefit to 
  keeping a persistent save file of the "last bitfile programed, last users
  connected" when the server shutsdown and then reload that on next startup, as
  that could be hours or days apart, the connected FPGA could be reprogrammed, etc.
  This is only useful if the server needs to restart (gracefully and quickly)
  Should this info be always written in case a crash/restart occurs. Is that
  likely with RUST? 


* **Role-based access control** — Admin vs Normal user authority levels,
  tied to authenticated identity (mTLS / LDAP, replacing self-asserted
  `X-User-Id`). Scope (resource-management decision D31, see
  [resource-management-recommendations.md](resource-management-recommendations.md)):
  - restrict `ConfigureFPGA` / `ConfigureFPGAFromMemory` / clear
    configuration, which today is a forced override that unlocks, releases
    and deletes every resource on the device (D15b, D26);
  - restrict resource registration and deletion (today: anyone, D15c);
  - optionally restrict writes to reset EPs (today: any holder of an
    unblocked resource, D17);
  - restrict server management (service start/stop/restart, server
    restart).

* **Cross-platform CI** — GitHub Actions matrix for Windows + Ubuntu.

* **Cross-platform packaging** — Windows service installer, systemd unit file,
  Docker image.

* **FPGA dynamic partial reconfiguration** — safe remote flashing with hardware
  fallback recovery.

* **Resource-model follow-ups (moved from Phase 44 draft)** — server-side
  sequences / transactions (embedded engine explored first, D40; replaces
  the old `classic/sequence` endpoint proposal); declarative address carriers for bridge
  designs (old S-21); WireIn shadow read-back (old Q44-6); atomic
  multi-resource acquire (old Q44-12). See
  [resource-management-recommendations.md](resource-management-recommendations.md) §5.

* **Rust egui demo unit tests** — egui does not provide a built-in test harness
  like iced's `iced_test`; implementing headless UI tests will require
  significant effort.

* **mDNS browser unit tests** — deferred from Phase 28A. Test the
  `mdns_browser` module in each of the 4 common libraries (Rust + Python,
  REST + gRPC) with mock service announcements where possible. Also test
  `[server] description` config default/override and TXT record generation
  with the description field.

* **mDNS dynamic bitfile announcements** — update the mDNS TXT records to
  include the currently loaded bitfile description (or "Not Programmed" if the
  device has no bitfile loaded). This would allow GUI clients to see at a
  glance what each discovered server is running before connecting. Requires
  re-announcing the mDNS service whenever the FPGA is programmed or reset.

---

## 📦 Completed Phases

All completed phases are archived in [`docs/archive/`](archive/) with full
implementation details, verification steps, and acceptance criteria.

| Phase | Title | Date | Key ADRs | Archive |
|-------|-------|------|----------|---------|
| 0 | Project Scaffolding | 2026-08-16 | — | [phases 0–6](archive/roadmap-phases-00-06.md) |
| 1 | OpenAPI 3.0 Specification | 2026-08-16 | ADR-004 | [phases 0–6](archive/roadmap-phases-00-06.md) |
| 2 | Proto3 gRPC Specification | 2026-08-16 | ADR-005 | [phases 0–6](archive/roadmap-phases-00-06.md) |
| 3 | FFI Layer (`ok-fpga-ffi`) | 2026-08-16 | ADR-001 | [phases 0–6](archive/roadmap-phases-00-06.md) |
| 4 | Core Domain (`ok-fpga-core`) | 2026-08-16 | ADR-002, 003, 006 | [phases 0–6](archive/roadmap-phases-00-06.md) |
| 4E | Service Manager | 2026-08-16 | ADR-010 | [phases 0–6](archive/roadmap-phases-00-06.md) |
| 5 | gRPC Server | 2026-08-16 | ADR-005 | [phases 0–6](archive/roadmap-phases-00-06.md) |
| 6 | REST Server | 2026-08-16 | ADR-004 | [phases 0–6](archive/roadmap-phases-00-06.md) |
| 7 | Configuration & Logging | 2026-08-16 | — | [phases 7–13](archive/roadmap-phases-07-13.md) |
| 8 | Web Dashboard | 2026-08-16 | ADR-007 | [phases 7–13](archive/roadmap-phases-07-13.md) |
| 9 | Authentication Stubs | 2026-08-16 | ADR-008 | [phases 7–13](archive/roadmap-phases-07-13.md) |
| 10 | MQTT Publisher | 2026-08-16 | ADR-011 | [phases 7–13](archive/roadmap-phases-07-13.md) |
| 11 | mDNS Discovery | 2026-08-16 | ADR-012 | [phases 7–13](archive/roadmap-phases-07-13.md) |
| 12 | Integration Testing | 2026-08-16 | — | [phases 7–13](archive/roadmap-phases-07-13.md) |
| 13 | Documentation & Polish | 2026-08-16 | — | [phases 7–13](archive/roadmap-phases-07-13.md) |
| 14 | Device Scanner | 2026-08-17 | ADR-013→018 | [phases 14–18](archive/roadmap-phases-14-18.md) |
| 15 | Hot-Plug Detection | 2026-08-17 | ADR-002 | [phases 14–18](archive/roadmap-phases-14-18.md) |
| 16 | Example Clients & E2E | 2026-08-17 | ADR-014 | [phases 14–18](archive/roadmap-phases-14-18.md) |
| 17 | HTTP Method Audit | 2026-08-17 | ADR-015 | [phases 14–18](archive/roadmap-phases-14-18.md) |
| 18 | Hardware Integration Tests | 2026-08-17 | ADR-016 | [phases 14–18](archive/roadmap-phases-14-18.md) |
| 19 | FFI Endpoint Wiring | 2026-08-17 | ADR-017, 018, 019 | [phases 19–22](archive/roadmap-phases-19-22.md) |
| 20 | Enhanced Resource Registration | 2026-08-17 | ADR-020 | [phases 19–22](archive/roadmap-phases-19-22.md) |
| 21 | User Session Tracking | 2026-08-17 | ADR-020 | [phases 19–22](archive/roadmap-phases-19-22.md) |
| 22 | Event System & Policy Stubs | 2026-08-17 | — | [phases 19–22](archive/roadmap-phases-19-22.md) |
| 23 | Disconnect Detection Fix | 2026-08-18 | — | [phases 23–25](archive/roadmap-phases-23-25.md) |
| 24 | Dashboard Log Streaming | 2026-08-18 | ADR-022 | [phases 23–25](archive/roadmap-phases-23-25.md) |
| 25 | GUI Example Clients | 2026-08-18 | ADR-023 | [phases 23–25](archive/roadmap-phases-23-25.md) |
| 26 | Endpoint Access Enforcement | 2026-08-19 | ADR-024 | [phases 26–27](archive/roadmap-phases-26-27.md) |
| 27 | OpenAPI Spec Gaps | 2026-08-19 | — | [phases 26–27](archive/roadmap-phases-26-27.md) |
| 28 | mDNS-Powered GUI Discovery + Dashboard Settings | 2026-08-19 | — | [phase 28](archive/roadmap-phases-28.md) |
| 28C | Settings Lockdown | 2026-08-19 | — | [phase 28](archive/roadmap-phases-28.md) |
| 28D | Dashboard Reliability, Expanded Views & SYZYGY Board Catalog | 2026-08-19 | — | [phase 28](archive/roadmap-phases-28.md) |
| 29 | Developer-Defined Classic Address Mapping | 2026-08-20 | ADR-025 | [phase 29](archive/roadmap-phases-29.md) |
| 30 | Read-Only Resource Acquisition & Private Data | 2026-08-20 | ADR-026 | [phase 30](archive/roadmap-phases-30.md) |
| 31 | OS-Native mDNS Service Registration | 2026-08-21 | ADR-027 | [phase 31](archive/roadmap-phases-31.md) |
| 32 | Bug Fixes: Users Page, Settings, gRPC Identity | 2026-08-21 | — | [phase 32](archive/roadmap-phases-32.md) |
| 33 | Read-Aware Lock Enforcement | 2026-08-21 | ADR-024, 026 | [phase 33](archive/roadmap-phases-33.md) |
| 34 | Client UX & Device Metadata Improvements | 2026-08-21 | ADR-004 (F), ADR-005 (C) | [phase 34](archive/roadmap-phases-34.md) |
| 35 | Client Demo Bug Fixes & README Update | 2026-08-21 | — | [phase 35](archive/roadmap-phases-35.md) |
| 36 | Configure FPGA API Alignment | 2026-08-23 | ADR-028 | [phase 36](archive/roadmap-phases-36.md) |
| 37 | API Response Compliance Audit | 2026-08-23 | — | [phase 37](archive/roadmap-phases-37.md) |
| 38 | User Identification Consistency | 2026-08-23 | ADR-029 | [phase 38](archive/roadmap-phases-38.md) |
| 39 | SSE Event Expansion | 2026-08-24 | ADR-030 | [phase 39](archive/roadmap-phases-39.md) |
| 40 | MQTT Discovery, Report-by-Exception, Hostname Identity | 2026-08-26 | ADR-011(B), ADR-031, ADR-032 | [phases 40–43](archive/roadmap-phases-40-43.md) |
| 41 | Singleton Instance Guard & Startup Safety | 2026-08-26 | ADR-033 | [phases 40–43](archive/roadmap-phases-40-43.md) |
| 42 | mDNS Hostname Identity Alignment | 2026-08-27 | — | [phases 40–43](archive/roadmap-phases-40-43.md) |
| 43 | mDNS Stale Registration Cleanup & Hostname Sanitization | 2026-09-19 | — | [phases 40–43](archive/roadmap-phases-40-43.md) |

---

## Feature Flag Summary

| Feature | Default | Dependencies Added | Phase |
|---------|---------|-------------------|-------|
| `rest` | ✅ Yes | axum, tower-http | 6 |
| `dashboard` | ✅ Yes | (static files only) | 8 |
| `grpc` | ❌ No | tonic, prost, tonic-build | 5 |
| `mqtt` | ❌ No | rumqttc | 10 |
| `mdns` | ❌ No | dns-sd-native + mdns-sd (fallback) | 11, 31 |
| `full` | ❌ No | all of the above | — |

# ok-FPGA-Server — Development Roadmap

> **Status:** Living Document
> **Last Updated:** 2026-10-12 (44-B archived, 0.3.3; 44-C next for planning)
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
| **Phase 44** | Resource Model Consolidation & Enforcement Alignment | ADR-034 (supersedes parts of ADR-003/006/020/024/025/026) | 🚧 Now — 44-A, 44-B ✅ [Archived](archive/roadmap-phases-44.md); 44-C next |

---

## 🟢 Now

### Phase 44 — Resource Model Consolidation & Enforcement Alignment

> **Status:** 🚧 Now — rev 4 (2026-10-10). Promoted by the owner on
> 2026-10-10. Open items in 44.7 must be answered before the sub-phase
> that needs them starts (Q44-13 blocks 44-A).
> **Model source:** [resource-management-recommendations.md](resource-management-recommendations.md)
> (decisions D1–D78, rules R1/B1–B4/L1/A1/E1/T1, spec items S1–S8 in §3.1,
> proto items P1–P16 in §3.2) and
> [resource-management-developer-guide.md](resource-management-developer-guide.md).
> D-numbers below are for traceability; each bullet states the rule itself.
> **Origin:** `python-DualPort-Transceiver-client` design review, 2026-10-08 (dual-port
> SFP+ DualPort design on the XEM8320).
> **Related plans:**
> [LabVIEW client Phase C-17](client/client-development-roadmap.md) ·
> [DualPort-Transceiver client Phase C-SFPP-1](../client/python-DualPort-Transceiver-client/docs/client-sfpp-development-roadmap.md)

> **Spec rule (ai-collaboration §2, §4):** sub-phases marked 🔒 change
> `api/openapi.yaml` and/or `proto/okfpga/v1/okfpga.proto`. The owner has
> approved the *content* listed here (D36–D38, D57–D59, D61–D72; dataport
> D63). Each 🔒 sub-phase still gets an explicit "go" from the owner when it
> starts. Until a spec is changed, it is treated as correct and the
> implementation is what gets fixed.

> **REST + gRPC together (Q44-5):** every sub-phase changes REST **and**
> gRPC in the same commit, with tests on both. There is no REST-first or
> gRPC-only sub-phase. gRPC tests need the `grpc` feature: every sub-phase
> runs `cargo test --features full` as well as `cargo test`.

> **Versioning:** the workspace version (root `Cargo.toml`, currently
> `0.3.3`) went to `0.2.0` in 44-A (breaking API contract), to `0.3.0`
> with the post-44-A read-semantics change (breaking: whole-bank reads
> removed, snapshot reads) and to `0.3.1` for the IsTriggered mask default
> (D78), and to `0.3.2` for the 24 s heartbeat text and the shipped
> `config.toml` setting `endpoint_enforcement = "enforce"` (code default
> stays `off` until 44-E), and to `0.3.3` for 44-B (registration rules,
> conflict `items`). It gets a patch bump in every later sub-phase.
> Python packages are bumped in the commit that changes them.

> **API Specification (2026-10-11):** the developer-facing spec
> [docs/API-Specification/](API-Specification/) (Typst; PDF
> `ok-fpga-server-api-specification.pdf`) specifies the Resource Model and
> Access Enforcement in full. Every sub-phase updates the affected chapters
> and **Appendix A (implementation status)** in the same commit, and
> rebuilds the PDF (`typst compile main.typ ok-fpga-server-api-specification.pdf`).
> Visible text never cites ADRs / D-numbers / phases; those go in `// ref:`
> comments in the `.typ` sources.

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
  D21, D42). They behave exactly as Opal Kelly FrontPanel: Get/Is calls
  read the shared device snapshot, only Update calls touch the FPGA, an
  optional `update` flag combines both, and every read names one EP (D76).
- **ReadOnly** holders read every non-private item, never write, are
  denied `ResetFPGA`, and are denied `UpdateTriggerOuts` when any resource
  on the device declares a private TriggerOut. Items whose read has a
  side-effect should be marked `private` (D75).
- **Registration fields:** `access_mode` is required (422 if missing),
  except for a resource with no `endpoints` field, which is an exclusive
  resource claiming every Wire and Trigger EP, 0x00–0x7F (`access_mode` may
  be omitted; `shared` → 422) (D62, D68, D77). `{"entries": []}` claims no
  EPs (D55).
  `default_access` only fills in a missing `access` on listed entries
  (D61). Metadata is text only (D69).
- **Errors:** 422 for anything wrong in the request itself; 409 only for a
  clash with existing state (overlap, lock, blocking, different-mode
  acquire) (D72).
- **Reprogram / clear** is a forced override: unlock, release, delete,
  notify everyone, then program (D15b, D26). `ResetFPGA` keeps resources
  (D56) and needs only the tier-3 data-port gate (D66).
- **Terminology (T1, D49, D50, D64):** "legacy" is only the dataport
  `OK_DATAPORT_LEGACY`. A resource is never "legacy". A Classic resource may
  use okRegisterBridge, never direct AXI. Direct AXI needs
  `OK_DATAPORT_AXI`.
- **Dataport (D63, D70):** public values `legacy` / `axi` only. A device
  reporting `OK_DATAPORT_UNKNOWN` is unsupported: not listed, 404 /
  `NOT_FOUND` on single-device calls.
- **Enforcement default `enforce`** (D27); a user id is required.

#### 44.1 ADR

One new ADR, **ADR-034 — Resource Model, Registration, Acquisition, Locking
and Enforcement** (`docs/architecture/adr/ADR-034-resource-model-consolidation.md`;
outline in recommendations §4). It absorbs or supersedes ADR-006 (resource
model, acquisition, lifecycle), ADR-020 §1–§3 and §6–§11, ADR-024 §1–§7, §9,
§10, ADR-025 §2, §3, §5 and Future Work, ADR-026 §1–§5, §7, §8, and the
ADR-003 wording that calls okRegisterBridge "the AXI path" (replaced by:
Classic resources may use okRegisterBridge; direct AXI requires
`OK_DATAPORT_AXI`). ADR-029 is marked Accepted/Implemented. The old ADRs stay
in the log; each superseded section gets a line *Superseded by ADR-034 §x*.
ADR-034 is written in 44-A and added to
[architecture/overview.md](architecture/overview.md) in the **same commit**
as the code it documents; every later sub-phase amends ADR-034 (decision
text, Files changed, commit hash) in its own commit. Required sections:
Date, Commit hash, Status, Context, Decision/Rationale, Files changed, Open
items.

#### 44.2 Issues carried forward (from the 2026-10-08 review, revised)

Evidence re-checked on 2026-10-10 against the working tree. References to
`ok-fpga-core/src/resource.rs`, `rest/devices.rs` and `grpc/*.rs` are from
the 2026-10-08 review; the implementing agent opens the file and confirms
the behaviour (not the line number) before changing it.

**Spec ↔ implementation**

| ID | Issue (evidence) | Resolution | Sub-phase |
|----|------------------|------------|-----------|
| S-01 🔒 | `CreateResourceRequest` spec has `resource_type` / `capabilities` only ([openapi.yaml:2042](../api/openapi.yaml)); the implementation needs `access_mode`, `interfaces`, `endpoints`, `address_ranges`, `metadata`, `private` (`rest/resources.rs`) | Spec to the declaration schema (recs S1, sketch 44.6); `resource_type` / `capabilities` **removed**; `access_mode` required except for a resource without `endpoints` (D62, D68); metadata string values only (D69) | 44-A |
| S-02 🔒 | `Resource` response ([openapi.yaml:2065](../api/openapi.yaml)) lacks `locked_by`, `metadata`, declarations; `holders` is `string[]`; `access_mode` includes `read_only` | `access_mode` = declared mode (`shared` / `exclusive`), `holders[] = {user_id, mode}`, `locked_by`, `interfaces`, `endpoints`, `address_ranges`, `metadata`, `private` (recs S2, D37), in the spec **and** `ResourceView` (`rest/resources.rs:182`) **and** proto `Resource` (P2) | 44-A |
| S-03 🔒 | REST `acquire_resource` (`rest/resources.rs:523`) uses the registered `access_mode` and ignores the request `mode`; gRPC honours it (`grpc/device_service.rs`) | A1: same mode → 200, different mode → 409 "release, then acquire" (D7, D28, D43). Acquire `mode` optional in both specs (recs S8, P3): omitted = declared mode; `read_only` is opt-in (D62). REST and gRPC identical | 44-A (spec), 44-C |
| S-04 🔒 | gRPC `RegisterResource` maps `resource_type` → `access_mode`; no declarations | Proto `CreateResourceRequest` parity with S-01 (P1: field 4 = `optional ResourceAccessMode access_mode`, field 5 = `repeated string interfaces`, D65, D71); same validator as REST | 44-A, 44-B |
| S-05 | Pipe and block-pipe routes take POST JSON/base64 (`rest/classic.rs:653`, `:693`, `:739`, `:778`; `PipeRequest` at `:136`); spec is `application/octet-stream` ([openapi.yaml:703–826](../api/openapi.yaml)) | **Decided (Q44-3):** server to the spec's octet-stream request and response bodies; update the Rust/Python example clients and the LabVIEW client in the same commit. gRPC already uses `bytes` (no change; parity test only) | 44-G |
| S-06 | Register routes take a single `{address, value}` (`RegisterRequest`, `rest/classic.rs:129`); spec uses `addresses[]` / `entries[]` with `data` ([openapi.yaml:827–890](../api/openapi.yaml)) | Fix impl to spec. gRPC already has `ReadRegisters` / `WriteRegisters` (no change; parity test only) | 44-G |
| S-07 | `WireInRequest.auto_update` missing (`rest/classic.rs:90`); `UpdateWireIns` always called (`:436`) | Implement `auto_update` as in the spec ([openapi.yaml:1897](../api/openapi.yaml)) and as gRPC already does (`grpc/classic_service.rs:296`) | 44-G |
| S-08 🔒 | Undocumented routes: `wires/in/update`, `wires/out/update`, `triggers/out/update` (`rest/classic.rs:28–49`), whole-bank `GET wires/out` / `triggers/out` (no `endpoint` query) | Document the three update routes in OpenAPI; the two output updates are **reads** (D42), `wires/in/update` is a write. **Whole-bank reads: removed in 0.3.0** (never approved; D76) — `endpoint` is required | 44-A (spec), 0.3.0 (removal), 44-E (enforcement) |
| S-09 | State / mode strings serialised as Display/Debug text (`"In Use (Shared)"`, `"InUseShared"`, `"Shared"`) in `ResourceView` and SSE payloads (`rest/resources.rs`) | Serialise the spec enum strings everywhere (REST, SSE, dashboard): states `unavailable` / `initializing` / `available` / `in_use_shared` / `in_use_locked`, declared modes `shared` / `exclusive`, holder modes `shared` / `exclusive` / `read_only`, exactly as listed in `api/openapi.yaml` | 44-A |
| S-10 🔒 | `dataport` enum: `[unknown, classic, axi]` ([openapi.yaml:1780](../api/openapi.yaml)) vs `[unknown, legacy, axi]` ([openapi.yaml:593](../api/openapi.yaml)) vs proto `UNKNOWN/LEGACY/AXI` | `legacy` / `axi` only (recs S7); proto `DATAPORT_LEGACY = 0, DATAPORT_AXI = 1` (P16); a device reporting UNKNOWN is filtered out internally and never reaches the public enums (D63, D70) | 44-H |

**Resource model and enforcement**

| ID | Issue (evidence) | Resolution | Sub-phase |
|----|------------------|------------|-----------|
| S-19 | Registration only rejects exclusive-vs-exclusive overlaps (`rest/resources.rs:388–428`) | Implement **R1**: shared-in-shared overlaps **allowed**; shared-vs-exclusive, exclusive-vs-anything, shared item in an exclusive resource, type or `private` mismatch → 409 naming the existing resource and item (D6, D8, D22, D38). Same code path for gRPC | 44-B |
| S-11 | A lock on one resource does not block another resource that shares an EP or range with it | Implement **blocking B1–B4** (D5, D18, D19): a lock on R blocks every resource that shares an item with R, for all users except the locker | 44-D |
| S-13 | `endpoint_covered` is true whenever `endpoints` is present, so holding any resource grants every EP; a held resource locked by someone else falls through to the next held resource (`ok-fpga-core/src/resource.rs`, `endpoint_covered` and its caller) | Coverage = listed entries only (D61), or every Wire and Trigger EP (0x00–0x7F) for a resource without `endpoints` (D77). The "try every held resource" loop is **correct** under B2, but each candidate must pass the ReadOnly, foreign-lock **and blocking** checks | 44-E |
| S-14 | AXI `check_address_access` and the Classic ADR-025 path disagree on exclusive ranges (`ok-fpga-core/src/resource.rs`) | One rule on both paths: no lock required (D44, D46); coverage, ReadOnly, foreign lock, blocking, and the same-resource range check | 44-E |
| S-12 | Enforcement off by default (`config.rs:463`, repo `config.toml:72`) | Default `enforce` in both places (D27); `off` / `warn` stay as debugging aids | 44-E |
| S-15 | Block pipes, `classic/reset` (`rest/classic.rs:878`), `*/update` skip enforcement (`rest/classic.rs`) | Enforce every data-port route, REST (`rest/classic.rs`, `rest/axi.rs`) and gRPC (`grpc/classic_service.rs`, `grpc/axi_service.rs`). Output updates = reads (D42); reset = tier-3 data-port gate plus a non-ReadOnly hold (D56, D66, D75 F1). (Whole-bank reads were removed in 0.3.0.) | 44-E |
| S-18 | `resolve_user_id` falls back to `"Unknown"` (`rest/mod.rs:92`, test at `:191–194`) | Under `warn` / `enforce`, a missing identity is rejected (D27), REST and gRPC (`x-user-id` metadata, P8) | 44-E |
| S-20 | ADR-025 `MapsToAddress` returns Ok when the covering resource has no ranges (`ok-fpga-core/src/resource.rs`, ADR-025 branch) | Address must be in the ranges of the **same** usable covering resource; empty ranges → deny (D9, B2) | 44-E |
| S-22 | Exclusive acquire sets `InUseLocked` without `locked_by`; range checks then deny the sole holder (`ok-fpga-core/src/resource.rs`) | Writes no longer check `locked_by` (D44, D46). Report the implicit lock (`locked_by` = holder) for display only | 44-C |
| S-23 | `private` cannot be set over REST; `create_resource` hard-codes `private: false` (`rest/resources.rs:355`, `:384`) | Accept and store `private` on resource, EP entries and ranges (D11) | 44-B |
| S-24 | `metadata` is `HashMap<String,String>` (`rest/resources.rs:69`), so a non-string value fails deserialisation with a generic error; range `name` / `description` dropped; omitted `access` defaults to exclusive (`default_exclusive`, `:88–94`, `:108`, `:122`) | Store range `name` / `description`; metadata stays text-only (D69) with a clear 422 for non-string values; remove `default_exclusive`: entry without `access` and without `default_access` → 422 (D61) | 44-B |
| S-17 | `configure` neither checks nor invalidates resources (`rest/devices.rs` `configure_fpga`) | Forced cleanup per D26 for configure **and** clear, REST and gRPC: unlock → release → delete (reason) → program; notify every holder | 44-F |

> **Why the Rust GUI demos appear to obey locks:** both `rust-rest-gui-iced`
> and `rust-grpc-gui-iced` grey out write controls client-side when
> `locked_by != me`, and both act on the **same** resource. With
> `endpoint_enforcement = "off"` the server is not involved.

**Documentation**

| ID | Doc | Resolution | Sub-phase |
|----|-----|------------|-----------|
| SD-01 🔒 | `api/openapi.yaml:23` port 8080 | Change to 8088 | 44-A |
| SD-02 | [architecture/overview.md](architecture/overview.md) | Remove the duplicated ADR table, add ADR-033 and ADR-034, correct the statuses, correct the crate names (`ok-fpga-ffi`, `ok-fpga-core`, `ok-fpga-server`) | 44-A |
| SD-03 | Access-control docs / ADR-024 | Mark ADR-024 sections superseded; move the read/write matrix (guide §5.3) and B2 pipeline into ADR-034 | 44-J |
| SD-04 | WireIn read-back | Document in ADR-034 and the guide: FrontPanel has no WireIn read-back; clients cache the values they wrote | 44-J |
| SD-05 | `dataport` meaning | Spec and proto descriptions: `legacy` = `OK_DATAPORT_LEGACY` transport only; dataport does not decide whether classic EPs exist | 44-H |
| SD-06 | [guides/python-rest-client-guide.md](guides/python-rest-client-guide.md) §5 | Replace the configure example with ADR-028's two routes: `POST /devices/{serial}/configure` (multipart `bitfile`) and `POST /devices/{serial}/configure_from_server_path` (JSON `bitfile_path`), both with `user_id` + `X-User-Id` | 44-J |
| SD-07 | [ADR-020](architecture/adr/ADR-020-client-side-resource-registration.md) §7–§8, Errors table | Mark superseded by ADR-034 §3–§4 (shared overlap **kept**; validation per R1; remove "no runtime enforcement (v1)") | 44-A |
| SD-08 | `docs/client/hdl-resource-mapping-guide.md` §4, §6 rule 3 | Owned by LabVIEW C-17; the server phase only checks it states: shared items in shared resources may overlap; exclusive resources hold only exclusive items; omitted `endpoints` = exclusive resource claiming all classic EPs; `{"entries": []}` = no EPs; `default_access` only fills a missing `access`; a lock blocks overlapping resources; wire width is device-dependent (not "16-bit") | 44-J |
| SD-09 | `rest/resources.rs` `create_resource` doc comment | Interface validation: `classic` always; `axi_*` only on `OK_DATAPORT_AXI` devices; mixing allowed (D54) | 44-B |

#### 44.3 Sub-phases

Each sub-phase: tests written with the code (TDD); `cargo test` and
`cargo test --features full` green; one commit (`feat:` / `fix:` / `docs:`)
with the version bump; ADR-034 amended. Test names follow
`test_{function}_{scenario}_{outcome}`. Tier 1 = `okREALM_TEST` (CI),
Tier 3 = FFI-trait mocks, Tier 2 = hardware (`#[ignore]`, run only with
owner approval). REST and gRPC tests in the same sub-phase (Q44-5).

Order and dependencies: 44-A → 44-B → 44-C → 44-D → 44-E → 44-F; 44-G and
44-H need only 44-A and may run after 44-B; 44-J is last. 44-B validates
`axi_*` interfaces against the device's internal FFI dataport value
(`ok-fpga-core/src/dataport.rs`), so it does not wait for 44-H.

**44-A Contract, wire formats and ADR-034 🔒 — ✅ Done (0.2.0 → 0.3.1)**
- Archived with the post-44-A read-semantics change (D75–D77, 0.3.0) and
  the IsTriggered mask default (D78, 0.3.1):
  [archive/roadmap-phases-44.md](archive/roadmap-phases-44.md).
- Commits `c5e4f79`, `5de5ab6`, `d88e5c5` (+ doc commits). Carried
  forward: F1 / F2 / F4 enforcement and D77 coverage → 44-E; trigger-bit
  validation → 44-E; TriggerOut event log → 🔵 Later.

**44-B Registration (R1) — ✅ Done (0.3.3)**
- Archived: [archive/roadmap-phases-44.md](archive/roadmap-phases-44.md)
  (owner decisions Q1–Q5, rules, tests as implemented).
- Commits `8a9e928` (0.3.2, heartbeat text / shipped enforce), `dcbda14`
  (44-B). ADR-034 §10. Carried forward: self-overlap "private wins" and
  D77 coverage enforcement → 44-E; registration vs unplug / reprogram
  (Q5) → 44-F.

**44-C Acquisition (A1)**
- Files: `ok-fpga-core/src/resource.rs`, `rest/resources.rs`,
  `grpc/device_service.rs`.
- Holder entries with mode; same mode → 200; different mode → 409 "release,
  then acquire" (S-03); acquire `mode` optional, omitted = declared mode,
  `read_only` opt-in (D62); ReadOnly rules incl. exclusive resources (D25);
  implicit lock shown as `locked_by` = holder for exclusive resources,
  display only (S-22).
- Tests (Tier 3): `test_acquire_same_mode_idempotent`,
  `test_acquire_different_mode_conflict`,
  `test_acquire_omitted_mode_uses_declared_mode`,
  `test_acquire_readonly_private_denied`,
  `test_acquire_exclusive_with_readonly_holders_allowed`,
  `test_acquire_exclusive_holder_writes_without_lock_allowed`,
  `test_grpc_acquire_different_mode_failed_precondition` (status per
  Q44-13). Tier 1: acquire/release flow.
- Commit: `feat: resource acquisition holder-mode rules (A1)`.

**44-D Locking and blocking (B1–B4) + events**
- Files: `ok-fpga-core/src/resource.rs`, `rest/resources.rs`,
  `rest/events.rs`, `grpc/device_service.rs`.
- Lock preconditions (holder, not ReadOnly); 409 if locked or blocked for
  the caller (B3); whole-resource lock; blocking computation (B1, B4); own
  lock never blocks self (D35). Emit `resource_locked` /
  `resource_unlocked` (SSE and gRPC `DeviceEvent`) to holders of the
  resource **and** of the resources it blocks.
- Implement gRPC `WatchDeviceEvents` (currently `UNIMPLEMENTED`) so gRPC
  clients receive the same events as SSE (scope note in 44-A).
  Test: `test_grpc_watch_device_events_streams_resource_events`.
- Tests (Tier 3): `test_lock_blocked_resource_conflict`,
  `test_blocking_shared_item_blocks_other_resource`,
  `test_blocking_non_transitive`, `test_blocking_own_lock_not_blocked`,
  `test_unlock_notifies_blocked_holders`, `test_grpc_lock_events_emitted`.
  Tier 1: two users, two overlapping shared resources.
- Commit: `feat: resource lock blocking and lock events (B1-B4)`.

**44-E Enforcement pipeline (B2)**
- Files: `config.rs`, `config.toml`, `rest/mod.rs`, `rest/access_middleware.rs`,
  `rest/classic.rs`, `rest/axi.rs`, `grpc/classic_service.rs`,
  `grpc/axi_service.rs`, `grpc/mod.rs` (identity), `ok-fpga-core/src/resource.rs`.
- Default `enforce` (S-12); identity required under `warn` / `enforce`
  (S-18); every data-port route enforced incl. block pipes and `*/update`
  (S-15); read/write classes (D42); candidate filtering
  (ReadOnly, foreign lock, blocked; S-13); no lock requirement (D44, D46,
  S-14); same-resource address check, empty ranges → deny (S-20);
  `AllowAlways` skips only the address check (D51); ReadOnly denied ADR-025
  mapped access (D29); private items readable only by holders (D16, D57);
  ResetFPGA via the tier-3 data-port gate (D56, D66); AXI addresses
  above 32 bits denied under `enforce` and noted in the proto (P14).
- ReadOnly rule (D75): **F1** ReadOnly-only callers denied `ResetFPGA` and
  AXI Lite / Full / Stream resets; **F2** `UpdateTriggerOuts` and trigger
  reads with `update=true` denied to ReadOnly-only callers when any
  resource on the device declares a private TriggerOut; snapshot reads
  checked per EP. **F4** new setting `warn_readonly_side_effects`
  (`[access_control]` in `config.toml`, default `true`, env override
  `OKFPGA_WARN_READONLY_SIDE_EFFECTS`): `warn!` at registration when
  ReadOnly holders could join a resource with a non-private PipeOut /
  BTPipeOut / TriggerOut or a non-private range.
- Coverage (D77): a resource without `endpoints` covers 0x00–0x7F only;
  update `declaration.rs` docs and the overlap check accordingly.
- Validate `ActivateTriggerIn` `bit` (0–31) before computing the mask:
  422 / `INVALID_ARGUMENT` (today `1u32 << bit` panics in debug builds;
  Strata 0.3.0 review). Test `test_activate_trigger_in_bit_out_of_range_returns_422`.
- Tests (Tier 3): `test_enforce_write_usable_holder_allowed`,
  `test_enforce_write_foreign_lock_denied`,
  `test_enforce_write_blocked_denied`,
  `test_enforce_read_during_foreign_lock_allowed`,
  `test_enforce_private_read_non_holder_denied`,
  `test_enforce_update_wire_outs_readonly_allowed`,
  `test_enforce_update_trigger_outs_readonly_allowed`,
  `test_enforce_update_trigger_outs_readonly_private_trigger_denied`,
  `test_enforce_trigger_read_update_readonly_private_trigger_denied`,
  `test_enforce_block_pipe_write_enforced`,
  `test_enforce_mapped_address_other_resource_range_denied`,
  `test_enforce_mapped_address_empty_ranges_denied`,
  `test_enforce_mapped_address_readonly_denied`,
  `test_enforce_allow_always_still_checks_blocking`,
  `test_enforce_axi_address_above_32_bits_denied`,
  `test_reset_fpga_readonly_only_denied`,
  `test_reset_fpga_non_readonly_holder_allowed`,
  `test_coverage_no_endpoints_excludes_pipes`,
  `test_warn_readonly_side_effects_logs_on_registration`,
  `test_warn_readonly_side_effects_disabled_no_log`,
  `test_access_control_config_default_is_enforce`,
  `test_resolve_user_id_missing_identity_rejected` (replaces the
  `"Unknown"` assertion at `rest/mod.rs:191–194`),
  `test_grpc_enforce_write_blocked_denied`. Tier 1: enforcement matrix on
  `okREALM_TEST`. Tier 2 (approval): DualPort bridge with two users.
- Commit: `feat: align endpoint enforcement with overlap and blocking (B2)`.

**44-F Reprogram and clear cleanup (D26) 🔒**
- Files: `rest/devices.rs`, `grpc/device_service.rs`,
  `ok-fpga-core/src/resource.rs`.
- `ConfigureFPGA`, `ConfigureFPGAFromServerPath` / `ConfigureFPGAFromMemory`,
  `ClearFPGAConfiguration` (REST and gRPC): under the device lock unlock all
  → release all → delete all (`resource_deleted` reason `reprogram` /
  `clear`) → program / clear; every holder notified. Remove the OpenAPI 409
  "Device has resources in use" responses on configure / clear (Q44-14). `ResetFPGA` does not clear (D56). Proto comments on
  `DeleteResource` (P11) and `ClearFPGAConfiguration` (P12).
- **Registration vs device lifecycle (owner, 2026-10-11, from 44-B Q5).**
  `POST /resources` / `RegisterResource` checks the device registry
  (exists, programmed) and later inserts into the resource registry under a
  different lock, so an unplug or reprogram can land in between.
  - *Owner:* a resource is tied to its device serial; if the device is
    gone, the registration must go away too. A watchdog may take a few
    seconds — that is acceptable. Today hot-unplug already deletes the
    device's resources (`resource_registry.invalidate_device` in
    `device_monitor.rs`), but a registration that lands *after* that
    cleanup survives as an orphan. Fix: a periodic sweep (reuse the user
    sweep in `sweep.rs`) deletes resources whose device is absent or
    unprogrammed, reason `device_removed`, with events.
  - *Owner:* for reprogram it matters only if a resource is tied to
    identifying details of the bitfile. Today it is not: configure calls
    `remove_for_device` *after* programming, so a registration racing a
    fast second configure can end up attached to the second bitfile.
    Proposal: the device records a *configuration generation* (counter
    bumped on every configure / clear, plus bitfile name and hash);
    `create_resource` captures it at lookup and `try_register` rejects
    (409) if it changed. Optional, for discussion: a resource may declare
    the bitfiles it is valid for (e.g. regex
    `.*rgmii.*xem7320.*enetab.*\.bit`, owner's example) so the watchdog can
    flag or delete resources whose device runs a non-matching bitfile.
    The declaration field is a spec change (🔒, needs design discussion).
  - Tests: `test_sweep_orphan_resource_device_absent_deleted`,
    `test_create_resource_configure_generation_changed_conflict`.
- Tests (Tier 3): `test_configure_fpga_held_resources_force_deleted`,
  `test_clear_fpga_notifies_all_holders`,
  `test_reset_fpga_resources_retained`,
  `test_grpc_configure_fpga_force_deletes_resources`. Tier 1: reprogram
  cleanup.
- Commit: `fix: forced resource cleanup on configure and clear`.

**44-G Classic data-path spec alignment**
- Files: `rest/classic.rs`; `client-examples/` (Rust and Python REST
  clients); `client/python-labview-client/` and the LabVIEW VIs that call
  pipes / registers.
- S-05 pipes and block pipes to `application/octet-stream` (Q44-3); S-06
  register `addresses[]` / `entries[]`; S-07 `auto_update`. gRPC already
  matches (bytes, bulk register RPCs, `auto_update`): parity tests only.
- Client update for the 44-A resource contract (scope note in 44-A):
  `client-examples/` and `client/python-labview-client/` move to the new
  `CreateResourceRequest` / `Resource` shapes, holder objects, spec
  state strings and the optional acquire `mode`. Own commit:
  `fix: update example clients to the ADR-034 resource contract`.
- Tests: Tier 3 `test_pipe_in_octet_stream_body_accepted`,
  `test_pipe_out_returns_octet_stream`,
  `test_registers_read_bulk_addresses`, `test_registers_write_bulk_entries`,
  `test_set_wire_in_auto_update_false_skips_update`,
  `test_grpc_classic_pipe_register_parity`; Tier 1 pipe and register
  round-trips; Tier 2 (approval) pipe throughput.
- Commits: one per item (`fix: align classic pipe routes with OpenAPI`, …),
  each with the client updates it requires.

**44-H Dataport normalisation 🔒 (D63; permission granted)**
- Files: `api/openapi.yaml` (`:593`, `:1780`), `proto/okfpga/v1/okfpga.proto`
  (`DataportType`, `AXIDataPortService` comment), `ok-fpga-core/src/dataport.rs`,
  `ok-fpga-core/src/device.rs` / `manager.rs` (device listing),
  `rest/devices.rs`, `grpc/device_service.rs`, client models in
  `client-examples/` and `client/python-labview-client/`.
- S-10, SD-05: public `legacy` / `axi`; proto `DATAPORT_LEGACY = 0,
  DATAPORT_AXI = 1` (P16); a device whose FFI dataport is
  `OK_DATAPORT_UNKNOWN` is dropped from listings and gives 404 /
  `NOT_FOUND` on single-device calls (D70); `AXIDataPortService` comment:
  "direct AXI via FPGADataPortAXI (`OK_DATAPORT_AXI` devices only)" (P9).
- Tests (Tier 3): `test_dataport_legacy_maps_to_legacy`,
  `test_dataport_axi_maps_to_axi`, `test_list_devices_unknown_dataport_omitted`,
  `test_get_device_unknown_dataport_not_found`,
  `test_grpc_get_device_unknown_dataport_not_found`.
- Commit: `fix: normalise dataport enum to legacy and axi`.

**44-J Documentation**
- SD-03, SD-04, SD-06, SD-08 check; ADR-034 status → Accepted/Implemented
  with all commit hashes; superseded markers in ADR-006/020/024/025/026/003;
  the two resource-management docs reduced to a pointer into ADR-034 plus
  the decision log.
- Check: every relative link in the changed docs resolves (manual or a
  link checker if one is available).
- Commit: `docs: consolidate resource model documentation`.

(Old 44-I "remaining proto alignment" is gone: under Q44-5 its items moved
into the sub-phase that changes the matching behaviour — P5, P7, P10 → 44-A;
P14 → 44-E; P9 → 44-H.)

#### 44.4 Acceptance criteria

1. Shared items in shared resources may overlap; every other overlap → 409
   naming the existing resource and item.
2. Exclusive resources with shared items are rejected (422).
3. `default_access` only fills in a missing `access` on listed entries; it
   never claims unlisted EPs.
4. Omitted `endpoints` = exclusive resource claiming every classic EP
   (`access_mode` optional there, `shared` → 422); `{"entries": []}` = no
   EPs.
5. Resources claim only the ranges they declare.
6. `axi_*` interfaces are rejected on `OK_DATAPORT_LEGACY` devices; classic
   and AXI may be combined on `OK_DATAPORT_AXI` devices.
7. Missing `access_mode` → 422 (except item 4); acquire `mode` optional;
   no `UNSPECIFIED` value in either spec.
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
18. Dataport enum is `legacy` / `axi` only; UNKNOWN devices are not listed.
19. OpenAPI, proto and implementation agree; REST and gRPC behave the same
    (a gRPC test exists beside each REST test).
20. Pipes use `application/octet-stream`; register routes use
    `addresses[]` / `entries[]`; the example and LabVIEW clients work
    against them.
21. ADR-034 is committed with the code it documents and listed in
    overview.md.

#### 44.5 Moved to Later (see 🔵 Later)

- **S-21 declarative address carriers** (old Q44-9): the compiled ADR-025
  DualPort stub stays (D4).
- **Classic sequence endpoint** (old 44.3 proposal): replaced by
  the server-side sequences future work, embedded engine first (D40).
- **WireIn shadow read-back** (old Q44-6).
- **Atomic multi-resource acquire** (old Q44-12).
- **TriggerOut event log** (D67 reopened 2026-10-11, Q-F).

Dropped: the 2026-10-08 ADR proposals for this phase, old Q44-7
(superseded by D48, D68) and old S-16, the **per-holder** TriggerOut cache
(still rejected: no per-holder state, D67; the device-wide event log above
is a different design). ADR-034 now means the consolidated ADR (44.1).

#### 44.6 F-1 spec sketch (for S-01/S-02 — applied in 44-A)

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
    endpoints:                     # omitted = exclusive, all Wire/Trigger EPs 0x00-0x7F (D77); {entries: []} = no EPs
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
# 409 = clash with existing state: invalid overlap (R1), locked / blocked, different-mode acquire
# 422 = request error: missing field, rule A, type/address mismatch, axi_* on legacy dataport,
#       shared without endpoints, non-string metadata (D72)
# Error body shape: Q44-13
```

#### 44.7 Open questions

- **Q44-1** Spec edits: content approved (D36–D38, D57–D59, D61–D72);
  confirm each 🔒 sub-phase (44-A, 44-F, 44-H) at its start. **44-A: go
  given 2026-10-10.**
- **Q44-13 — decided 2026-10-10 (D73):** keep RFC 7807 `ProblemDetail`
  ([openapi.yaml:1717](../api/openapi.yaml)) and add an optional `conflict`
  object `{code, resource_id, resource_name, item_type, item}` where `code`
  ∈ `overlap`, `locked`, `blocked`, `mode_mismatch`, `validation`, and
  `item_type` ∈ `endpoint`, `address_range`. gRPC: 409 →
  `FAILED_PRECONDITION`, 422 → `INVALID_ARGUMENT`, 404 → `NOT_FOUND`, with
  the same fields in a `ResourceConflictDetail` proto message carried in
  the status details.
- **Q44-14 — decided 2026-10-10 (D74):** 44-F removes the OpenAPI 409
  "resources in use" responses on configure / clear (forced cleanup, D26).

Resolved: Q44-2 (D28, D43, D62), Q44-3 (octet-stream, 2026-10-10), Q44-4
(D27), Q44-5 (REST and gRPC together, 2026-10-10), Q44-8 (D61), Q44-10
(D69), Q44-11 (D50, D54); round 6: N26 (D61), N27 (D62, D68), N28 (D63),
Q4 (D65), Q5 (D66), Q7 (D67, rejected; reopened as the Later event log),
Q8 (D69), N30 (D70), N31 (D71), N32 (D72); post-44-A (2026-10-11): F1, F2,
F4 (D75), F3, Q-B, Q-E, Q-G (D76), Q-C (D77), Q-F (D67 reopened → Later),
Q-H (one 0.3.0 commit, enforcement in 44-E, spec permission granted), Q-I
(accepted). Dropped: Q44-7. Moved to Later: Q44-6, Q44-9, Q44-12.

---

## 🟡 Next

*(Empty — no phase is planned next.)*

---

## 🔵 Later: Future Horizons

Features in this section represent uncommitted research or long-term capabilities:

* **API Specification — expand the overview chapters.** Section 2 of
  [docs/API-Specification/](API-Specification/) is an overview only. Expand
  each part into a full specification (every operation, parameter, error,
  rationale, Opal Kelly citation), as already done for the Resource Model
  (§3) and Access Enforcement (§4): server/management, identity and auth,
  devices (sensors, settings, SYZYGY, PLL, flash), FPGA configuration,
  Classic data port (wires, triggers, pipes, block pipes, register bridge),
  AXI data port, events (SSE / gRPC / MQTT), dashboard, mDNS. Existing
  section numbers must not change (append sub-sections only).
* **OpenAPI / proto cite API Specification section numbers.** After the
  spec chapters exist, change `api/openapi.yaml` and
  `proto/okfpga/v1/okfpga.proto` descriptions to refer to the required
  behaviour by spec section number (e.g. "see API Specification §4.4.4")
  instead of ADR / D-numbers, keeping inline notes short. 🔒 Needs owner
  permission for the spec change.

* **TriggerOut event log (non-standard Opal Kelly extension)** — D67
  reopened 2026-10-11 (Q-F). Needs a design round and owner approval
  before any work; nothing below is decided except where marked.

  **Why.** With one FrontPanel handle per device (ADR-017/018), every client
  shares one TriggerOut snapshot (D76). `UpdateTriggerOuts` reads **and
  clears** the latches of the whole device, so one client's update can
  consume a trigger another client was waiting for, and no client can tell
  whether, or how many, updates happened since its own last read (the
  known limitation documented for Q-G in the guide §5.4 and ADR-034 §9).

  **Opal Kelly references (what the server must keep matching, D76).**
  - FrontPanel API, `FPGADataPortClassic`:
    https://library.opalkelly.com/library/FrontPanelAPI/classOpalKelly_1_1FPGADataPortClassic.html
  - C header `FrontPanel-6.1.0/.../okFrontPanel.h:1095–1103`
    (`okFrontPanel_UpdateTriggerOuts(okFrontPanel_HANDLE)`,
    `okFrontPanel_IsTriggered(handle, ep, mask)`,
    `okFrontPanel_GetTriggerOutVector(handle, ep)`): `UpdateTriggerOuts`
    takes **only the handle**, no endpoint.
  - `UpdateTriggerOuts`: "query the XEM to determine if any TriggerOuts
    have been activated since the last call."
  - `IsTriggered`: "find out if a particular bit (or bits) on a particular
    TriggerOut endpoint has triggered since the last call to
    UpdateTriggerOuts()" (quote corrected 2026-10-11 against the live page).
  - `GetTriggerOutVector`: "retrieve the full vector of values for a
    particular TriggerOut endpoint" (from the last update).
  - HDL endpoint ports (FrontPanel HDL library): `okWireOut` has only
    `okHE` / `okEH` / `ep_addr` / `ep_datain` — **no read strobe**, so a
    WireOut read is invisible to user logic. `okTriggerOut` has inputs
    `ep_clk` / `ep_trigger`; the latch lives inside the Opal Kelly endpoint
    and is cleared by `UpdateTriggerOuts`. `okPipeOut` / `okBTPipeOut`
    drive an `ep_read` output (FIFO drain). `okRegisterBridge` drives
    `ep_read` (designs may clear on read).

  **Scenario (owner, 2026-10-11).** One FPGA with two network ports. Alice
  holds `port-a` and watches TriggerOut A; Bob holds `port-b` and watches
  TriggerOut B. When Alice calls `UpdateTriggerOuts` she also clears B's
  latch, so Bob may never see his trigger. If Bob could ask "were there
  any B events **newer than my last read**?" before issuing his own
  update, he would know that a trigger happened even though Alice's update
  consumed it.

  **Proposal (best current outline).**
  - Every `UpdateTriggerOuts` the server runs (from any client, the
    `update=true` flag, or a poller) gets a device-wide **sequence number**
    and a **timestamp**.
  - The server appends each **non-zero** TriggerOut EP vector from that
    update to a **bounded, device-wide log**: `{seq, timestamp, ep,
    vector}`.
  - Clients query "events for EP x since seq N" and get the matching
    entries plus the latest seq. Each client **keeps its own N**.
  - **No per-holder server state** (decided; the old per-holder cache S-16
    stays rejected). The server never tracks who has seen what.
  - Clearly labelled **non-standard Opal Kelly**: separate routes / RPCs
    (e.g. `GET …/classic/triggers/out/events?endpoint=0x61&since=N` and a
    `GetTriggerOutEvents` RPC). The standard routes keep exact FrontPanel
    semantics (D76).
  - The log is cleared on reprogram / clear / disconnect, with the
    sequence restarting (or an epoch id) so stale N values are detected.

  **Open points (need owner decisions).**
  - **Log size**: fixed entry count vs time window; per device vs per EP;
    configurable in `config.toml`.
  - **Overflow reporting**: when entries older than N were dropped, the
    reply must say so (e.g. `overflow: true`, `oldest_seq`), so a client
    knows it may have missed events.
  - **`private` / ReadOnly**: a ReadOnly holder must not see events of a
    private TriggerOut EP; whether a ReadOnly holder may read the log at
    all when a private TriggerOut exists (F2 analogy), and whether
    non-holders of the covering resource may read events for that EP.
  - Whether `UpdateWireOuts` gets a matching "last update seq/timestamp"
    so clients can at least detect that someone else refreshed WireOuts.

  **Server-side poller (option).** A per-device, opt-in background task
  calls `UpdateTriggerOuts` every *T* ms and feeds the log, so no event is
  lost between client polls. Trade-offs: latency (≤ *T*) vs USB/PCIe bus
  load and FFI-mutex contention with client traffic; it must take the
  device lock like any other call; it changes what clients see from the
  standard routes (the snapshot is refreshed by the poller), so it must be
  explicitly enabled per device (config or API) and documented. Interaction
  with `update=true` and F2 (a private TriggerOut) must be defined.

  **gRPC streaming (option).** A server-streaming RPC (e.g.
  `WatchTriggerOutEvents(serial, endpoints[], since_seq)`) that replays
  log entries after `since_seq` and then pushes new ones as the poller or
  any update produces them. It maps onto the `WatchDeviceEvents` fan-out
  pattern (44-D, broadcast channel per device); REST could offer the same
  via SSE on `/events`. Slow consumers get the overflow marker instead of
  unbounded buffering.

  **Related design-specific hooks.** This is one of several server-side
  extensions that particular bitfiles need beyond plain FrontPanel calls,
  e.g. the DualPort-Transceiver FrontPanel_to_AXI bridge and custom AXI address
  handling (ADR-025 `ResourceAccessPolicy`), and server-side sequences
  (44.5, D12, D40). A common extension mechanism should be considered
  together with this item.

* **Configurable MQTT test broker host** (follow-up, 2026-10-11) —
  `crates/ok-fpga-server/tests/mqtt_integration.rs` hardcodes
  `127.0.0.1:1883`, so 3 tests fail with "connection refused" when no local
  Mosquitto runs (the lab broker is `127.0.0.1`). Read the
  host and port from environment variables (e.g. `OKFPGA_TEST_MQTT_HOST`,
  `OKFPGA_TEST_MQTT_PORT`, defaulting to `127.0.0.1:1883`), document them in
  the test module header, and decide whether the tests are skipped (or
  `#[ignore]`d) when the broker is unreachable so `cargo test --features full`
  stays green on machines without a broker. Use unique topic prefixes per run
  so tests against a shared broker do not collide.

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

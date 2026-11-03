# ADR-034: Resource Model Consolidation

**Status**: Accepted — 44-A implemented incl. post-44-A read semantics (0.3.0) and mask default (0.3.1);
44-A, 44-B, 44-C and 44-D archived; 44-B implemented (0.3.3); 44-C implemented (0.3.4); 44-D implemented (0.3.5); 44-E … 44-J pending
**Date**: 2026-10-11 (amended 2026-11-02)
**Commit hash**: 44-A `d3b19a6`; 0.3.0 read semantics (D75–D77) `dcb5e5a`;
0.3.1 IsTriggered mask default (D78) `6e11f9b`; 0.3.2 heartbeat text and
shipped enforcement `2e2f7a3`; 44-B registration rules (0.3.3) `1d95a6f`;
44-C acquisition rules (0.3.4) `2167914`; 44-D locking, blocking & events (0.3.5) `e562705`
**Deciders**: Project owner, Architecture Team
**Phase**: 44 ([development-roadmap.md](../../development-roadmap.md) §44)
**Supersedes (in part)**: ADR-020 §7, §8 and the Validation Responses table
(more sections are superseded by later sub-phases, see roadmap §44.1)

## Context

The resource contract had drifted apart in three places:

- The OpenAPI `CreateResourceRequest` still described `resource_type` /
  `capabilities`, while the server parsed `access_mode`, `interfaces`,
  `endpoints`, `address_ranges`, `metadata` and `private` (S-01).
- The `Resource` response returned holders as plain strings and states as
  Rust `Debug` text such as `"InUseShared"` (S-02, S-09).
- gRPC mapped `resource_type` onto `access_mode` and carried no declarations
  (S-04). Conflicts came back as free text, so clients could not tell which
  resource or item blocked them.

The design is recorded in
[resource-management-developer-guide.md](../../resource-management-developer-guide.md)
and [resource-management-recommendations.md](../../resource-management-recommendations.md)
(decisions D1–D78). This ADR is the single place where accepted decisions
become the server contract. Each Phase 44 sub-phase amends it.

## Decision

### §1 Declared mode vs holder mode

- A resource has a **declared mode** (`access_mode`): `shared` or
  `exclusive`. It describes the resource, not a user.
- Each holder has a **holder mode**: `shared`, `exclusive` or `read_only`.
  Holders are returned as `{user_id, mode}` objects.
- The declared-mode enum has no `read_only` value and, in proto, no
  `UNSPECIFIED` value. Proto uses `optional ResourceAccessMode`, so "not
  given" is a missing field, not a magic zero.

### §2 Wire formats (S-09)

States and modes are always sent as the spec's lowercase strings, on REST,
SSE, the dashboard and gRPC:

| Kind | Values |
|------|--------|
| State | `unavailable`, `initializing`, `available`, `in_use_shared`, `in_use_locked` |
| Declared mode | `shared`, `exclusive` |
| Holder mode | `shared`, `exclusive`, `read_only` |
| Delete reason | `client`, `reprogram`, `clear` |

SSE events carry the event name in `event:` and the variant object itself
in `data:` (no `{"VariantName": {...}}` wrapper).

### §3 Declaration schema

`CreateResourceRequest` (REST and gRPC) carries `name`, `device_serial`,
`description`, `access_mode`, `interfaces`, `endpoints`, `address_ranges`,
`metadata` (string values only, D69) and `private`. `resource_type` and
`capabilities` are removed. Presence rules handled in 44-A (D61, D62, D68):

- **No `endpoints` field** → the resource is exclusive. `access_mode` may
  be omitted (it becomes `exclusive`); `shared` is rejected (422). Coverage
  (D77, revises D68): every **Wire and Trigger** endpoint (0x00–0x7F) only.
  Pipe / BTPipe endpoints (0x80–0xBF) must be declared explicitly, because
  the server cannot tell a Pipe from a BTPipe at the same address. D23
  still applies (it blocks later classic resources). *Today the server
  still claims all 0x00–0xBF; the D77 coverage change lands in 44-E.*
- **`endpoints` present** → `access_mode` is required (422 if missing).
- **Each entry** needs its own `access` or an `endpoints.default_access`;
  `default_access` only fills a missing `access`. Neither → 422.
- `{"entries": []}` means "no classic endpoints".

The full R1 overlap rules, Rule A and the interface/dataport check are
implemented in 44-B (0.3.3), see §10. 44-A kept the existing
exclusive-overlap check, reported as a structured conflict.

### §4 Structured errors (Q44-13)

- **REST**: 409 and 422 responses use `ProblemDetail`. When another
  resource or user is the cause, `conflict` names it:
  `{code, resource_id, resource_name, item_type, item, items}`
  (`code` ∈ `overlap`, `locked`, `blocked`, `mode_mismatch`, `private`,
  `validation`; `blocked` is emitted from 44-D).
  A duplicate name is a 409 without `conflict`.
- Malformed JSON or bad enum values are 422 (not 400); the create route no
  longer returns 400.
- 201 responses carry `Location: /api/v1/resources/{id}`.
- **gRPC**: REST 409 maps to `FAILED_PRECONDITION` (including a duplicate
  name), 404 to `NOT_FOUND`, 422 to `INVALID_ARGUMENT` (table in
  `okfpga.proto`, `DeviceService` comment). Details travel in the standard
  `google.rpc.Status` as a `ResourceConflictDetail`
  (`type.googleapis.com/okFPGAService.v1.ResourceConflictDetail`).

### §5 Events (recs S5, D58)

- New events: `resource_locked`, `resource_unlocked`.
- `resource_deleted` gains `reason` (`client` now; `reprogram` / `clear`
  in 44-F).
- Proto `DeviceEvent` gains the matching messages. The gRPC
  `WatchDeviceEvents` stream itself is implemented in 44-D.

### §6 Users and holdings (P5)

`UserSession` gains `holdings: [{resource_id, mode, locked}]`, on REST
`/users`, the management API and gRPC `ListUsers` (now implemented).

### §7 Undocumented routes (S-08)

`wires/in/update` (write), `wires/out/update` and `triggers/out/update`
(reads, D42) are now documented. Enforcement for them comes in 44-E.

*Amended in 0.3.0 (D76):* the whole-bank `GET wires/out` /
`triggers/out` (no `endpoint` query), documented in 44-A, were never an
approved feature and are **removed**. `endpoint` is now required.

### §8 ReadOnly holders (D75, 0.3.0)

- A ReadOnly holder may **read** every covered item that is not
  `private`: WireOuts and `UpdateWireOuts`; TriggerOuts (`IsTriggered`,
  `GetTriggerOutVector`, `UpdateTriggerOuts`); PipeOut / BTPipeOut;
  `ReadRegister(s)`; AXI reads.
- It may **never write**: WireIn / `UpdateWireIns`, `ActivateTriggerIn`,
  PipeIn / BTPipeIn, register and AXI writes.
- **F1**: ReadOnly is denied `ResetFPGA`. AXI Lite / Full / Stream resets
  are treated as writes as well.
- **F2**: `UpdateTriggerOuts` (and `update=true` on a trigger read) is
  denied to ReadOnly when **any** resource on the device declares a
  `private` TriggerOut, because `UpdateTriggerOuts` reads and clears the
  latches of the whole device. Snapshot reads (`update=false`) are checked
  per endpoint (`private` → deny).
- **Design rule (documented, not enforced)**: a ReadOnly action should not
  change user-logic state or consume endpoint data other holders rely on.
  In Opal Kelly HDL `okWireOut` has no read strobe, but `okTriggerOut`
  latches are cleared by `UpdateTriggerOuts`, `okPipeOut` /
  `okBTPipeOut` drive `ep_read` (FIFO drain), and `okRegisterBridge` drives
  `ep_read` (possible clear-on-read). Any item where a ReadOnly read could
  cause an unwanted side-effect **should be marked `private`**. The HDL
  architect decides; this is stated in the OpenAPI and proto `private`
  descriptions.
- **F4**: `warn_readonly_side_effects` (bool, default `true`, in
  `config.toml`, env override) makes the server log `warn!` at
  registration when ReadOnly holders could join a resource with a
  non-private PipeOut / BTPipeOut / TriggerOut or a non-private range.
- Shared and exclusive holders are unrestricted by this section; D42 still
  applies to them.
- 0.3.0 changes the spec text only. Enforcement of F1 and F2 and the F4
  setting land in 44-E.

### §9 Opal Kelly read semantics (D76, 0.3.0)

The server opens one FrontPanel handle per device. Its reads now behave
exactly as the FrontPanel API
([FPGADataPortClassic](https://library.opalkelly.com/library/FrontPanelAPI/classOpalKelly_1_1FPGADataPortClassic.html),
`okFrontPanel.h`):

- `GetWireOutValue`, `IsTriggered` and `GetTriggerOutVector` read only the
  device **snapshot**, which all clients share.
- Only `UpdateWireOuts` / `UpdateTriggerOuts` touch the FPGA.
  `UpdateTriggerOuts` takes only the handle, so it reads and clears every
  TriggerOut latch of the device.
- Optional `update` flag (default `false`): REST `?update=true`, proto
  `optional bool update` on `GetWireValueRequest` (field 3, ignored by
  `GetWireInValue`), `IsTriggeredRequest` (field 4) and
  `TriggerOutVectorRequest` (field 3). The server then runs the Update call
  and the read atomically under the device lock.
- The standalone update routes / RPCs stay.
- Every read names one endpoint (422 if `endpoint` is missing).
- **IsTriggered mask defaults to 0 (D78, 0.3.1)**, on REST (`mask` query
  parameter) and gRPC (`IsTriggeredRequest.mask`, plain `uint32`, so
  "not set" is already 0). With no mask, `triggered` is always `false`; the
  client must name exactly the bits it cares about. An all-ones default
  would make a careless client look correct while also reporting triggers
  on unrelated bits. `vector` / `GetTriggerOutVector` always return the
  full 32-bit vector. *Discrepancy resolved:* before 0.3.1 REST and OpenAPI
  defaulted to `0xFFFFFFFF` while proto defaulted to 0.
- **Known limitation (Q-G)**: a client cannot tell whether other clients
  ran an Update since its own last read, or how many. A trigger can be
  consumed by another client's `UpdateTriggerOuts`. Only the future
  TriggerOut event log (roadmap 🔵 Later, D67 reopened) can fix this.

**Discrepancy fixed**: the proto already described `GetWireOutValue` as
"from the last UpdateWireOuts", but the server always called
`UpdateWireOuts` / `UpdateTriggerOuts` before every read. The
implementation now follows the spec.

The logic lives in one helper, `snapshot::read_snapshot(update, refresh,
read)`, which REST and gRPC both call, so the two APIs cannot drift apart.

### §10 Registration rules (44-B, 0.3.3)

REST and gRPC share `declaration::check_overlap` and the helpers below, so
the two APIs cannot drift apart.

- **Rule A** (422): an exclusive resource may contain only exclusive items:
  no shared entry, no shared range, no `default_access: shared` (even with
  `entries: []`).
- **R1** (409): first registration wins. An overlap is allowed only where
  both items are shared items in shared resources, with the same endpoint
  type and the same item-level `private` flag (ranges: both shared, same
  `private`).
- **Q3 — item-level `private` only.** Resource-level `private` (which denies
  ReadOnly acquisition) is not compared.
- **Q1 — self-overlap.** Ranges inside one declaration may overlap (201).
  `access_control.warn_range_self_overlap` (default `true`, env
  `OKFPGA_WARN_RANGE_SELF_OVERLAP`) logs a `warn!` per overlapping pair
  (`warnings.rs`). Where such ranges differ in `private`, the private one
  applies to the shared addresses; enforced with 44-E.
- **Duplicate endpoint addresses** (422): an address may appear once per
  declaration, even `pipe_in` vs `bt_pipe_in`.
- **Strict REST address syntax** (422): `^0x[0-9A-Fa-f]{2}$`
  (`parse_ep_address`); `5`, `0x5`, `0x005`, `0X05` are rejected.
- **Q4 — conflict `items`.** `ResourceConflict.items` (REST) /
  `ResourceConflictDetail.items = 6` (proto) lists every item that clashes
  with the named resource: endpoints ascending, then that resource's ranges
  in declaration order; `item` is the first entry. `detail` ends with
  "(N conflicting items in total)" when N > 1. Wildcard vs wildcard lists
  all 192 endpoints.
- **Deterministic choice**: existing resources are checked in `id` order
  (the registry is a `HashMap`), so the lowest id is named.
- **Check order on create**: build (422) → device lookup (404) → interfaces
  vs data port (422) → programmed (409) → self-overlap warning → R1 (409).
  A declaration that can never be valid on a device is 422 even while the
  device is unprogrammed.
- **Wildcard coverage** is still 0x00–0xBF; D77 (0x00–0x7F) lands in 44-E.
- **Q5 — registration vs device lifecycle** (unplug / reprogram during or
  after registration) is deferred to 44-F (roadmap).

### §11 Acquisition rules (44-C, 0.3.4)

REST `POST /resources/{id}/acquire` and gRPC `DeviceService.AcquireResource`
both delegate to `ResourceRegistry::acquire` and `DefaultAccessPolicy::can_acquire`:

- **S-03 fixed**: on REST, `mode` resolves as
  `requested.or(resource.access_mode).unwrap_or(AccessMode::Shared)`, so an
  explicit `mode` (such as `read_only` on a `shared` or `exclusive`
  resource) is honored rather than overridden by `resource.access_mode`.
- **Repeat acquire by an existing holder (A1, D2, D7, D28, D43)**:
  - Same mode → 200 / `OK` (`AcquireOutcome::AlreadyHeld`), idempotent: no
    duplicate `HolderEntry`, no state change, no `ResourceChanged` event
    (while still touching `user_sessions`).
  - Different mode → 409 / `FAILED_PRECONDITION` (`HolderModeMismatch`,
    `conflict.code = "mode_mismatch"`, "release first, then acquire").
- **Declared vs requested mode (D62)**: a non-`read_only` request whose
  `mode` differs from `resource.access_mode` (e.g. `shared` on an
  `exclusive` resource or `exclusive` on a `shared` resource) is rejected
  with 409 / `FAILED_PRECONDITION` (`DeclaredModeMismatch`,
  `conflict.code = "mode_mismatch"`).
- **Private resource ReadOnly denial (owner decision Q2)**: requesting
  `read_only` on a resource with `private: true` returns 409 /
  `FAILED_PRECONDITION` with `conflict.code = "private"`, naming the
  resource (`resource_id`, `resource_name`).
- **ReadOnly and Exclusive transitions (D25a/b/c)**:
  - `D25a`: `read_only` on a held `exclusive` resource (`in_use_locked`) is
    allowed (unless `private`); state stays `in_use_locked`.
  - `D25b`: `read_only` on an `available` `exclusive` resource transitions
    it to `in_use_shared` (`locked_by = None`).
  - `D25c`: `exclusive` acquisition of an `exclusive` resource in
    `in_use_shared` (held only by `read_only` observers) succeeds,
    transitioning to `in_use_locked` with `locked_by = Some(user_id)` while
    retaining the `read_only` holders. When the `exclusive` holder later
    releases and `read_only` holders remain, the resource transitions back
    to `in_use_shared`.
- **Implicit lock on Exclusive resources (S-22, owner decision Q1 Option A)**:
  acquiring in `exclusive` mode sets `locked_by = Some(user_id)` so the
  holder is shown as lock owner and passes `ExclusiveAddressRange` checks
  without calling `lock()`. Removing the `locked_by` check for exclusive
  address ranges on shared resources (`S-14`) remains scoped to 44-E.

#### 12. Locking, blocking and events (0.3.5, 44-D)

`ResourceRegistry::lock` and `ResourceRegistry::unlock` enforce the B1, B3,
B4 and D35 rules and return structured `LockOutcome` / `LockError` types so
REST and gRPC share the exact same logic:

- **Blocking (B1, B4, D35)**: resource `S` is blocked for user `U` when
  another resource `R` on the same device (`R.id != S.id`) is locked by
  another user `V` (`V != U`, so a user's own lock never blocks self, D35)
  and `R` declares at least one common item (a Classic endpoint address or
  an overlapping 32-bit `AddressRange`) with `S`. Blocking is direct overlap
  only — not transitive (B4).
- **Lock denial (B3, owner decision 44-D Q1)**:
  - If `S` itself is locked by another user, `lock` returns 409 /
    `FAILED_PRECONDITION` with `conflict = { code: "locked", resource_id: S.id, resource_name: S.name }`.
  - If `S` is blocked for the caller by `R` (checked in deterministic
    ascending `R.id` order), `lock` returns 409 / `FAILED_PRECONDITION` with
    `conflict = { code: "blocked", resource_id: R.id, resource_name: R.name }`
    (no item fields needed for `blocked`).
- **Idempotent repeat lock (owner decision 44-D Q2)**: if the caller already
  holds the lock on `S`, calling `lock` again returns
  `Ok(LockOutcome::AlreadyLockedBySelf)` (200 / `OK` affirming the lock,
  touching the user session, and suppressing duplicate `ResourceLocked` /
  `ResourceChanged` events).
- **Implicit unlock event completeness (owner decision 44-D Q3)**: when a
  holder of a shared-resource lock releases via `/release`, `ReleaseResource`,
  `/disconnect`, `Disconnect` or session timeout sweep without calling
  `/unlock` first, `ResourceUnlocked` is broadcast before `ResourceChanged`
  so holders of blocked resources are always notified.
- **gRPC `WatchDeviceEvents` fan-out**: subscribes to
  `DeviceManager::subscribe()`, converts all 11 `DeviceEvent` variants via
  `device_event_to_proto`, and streams them over `ReceiverStream`.

#### 13. Enforcement pipeline, coverage, and registration warnings (0.3.6, 44-E)

Phase 44-E aligns runtime data-port enforcement across REST and gRPC with
the resource declaration, overlap, and blocking model (`B2`, `S-12..S-20`,
`D42`, `D44`, `D51`, `D56`, `D61`, `D66`, `D75`, `D77`, `P14`):

- **Default enforcement & identity (`S-12`, `S-18`, owner decision Q3a)**:
  `AccessControlConfig::default().endpoint_enforcement` is now `"enforce"`,
  and `resolve_user_id` rejects missing identity (`400 Bad Request`) instead
  of falling back to `"Unknown"`. Anonymous `/configure` and
  `/configure_from_server_path` requests are permitted only when
  `endpoint_enforcement == "off"`.
- **Wildcard endpoint coverage (`D77`, owner decision Q2)**:
  `WILDCARD_EP_LAST` is narrowed from `0xBF` to `0x7F` so a Classic resource
  that omits `endpoints` (`None`) claims only Wire and Trigger endpoints
  (`0x00–0x7F`) and leaves Pipe/BTPipe (`0x80–0xBF`) available to other
  resources on the same device. Registration logs `warn!` messages via
  `warnings::warn_wildcard_endpoints` when `endpoints` is omitted or when an
  `endpoints: None` resource and a Pipe-declaring resource coexist on the
  same device.
- **Candidate filtering & blocking (`B2`, `S-13`, `S-14`, `D44`, `D51`)**:
  for write operations (`SetWireInValue`, `UpdateWireIns`,
  `ActivateTriggerIn`, `PipeIn`/`BtPipeIn` writes, `WriteRegister(s)`, AXI
  writes, `ResetFPGA`, AXI resets), candidate resources held only as
  `ReadOnly`, locked by another user (`locked_by != Some(user_id)`), or
  blocked by another user's lock on an overlapping resource (`B2`) are
  filtered out before checking endpoint/address coverage. Exclusive address
  ranges inside a `Shared` resource no longer require the caller to hold a
  resource lock (`S-14`, `D44`), and `AccessRule::AllowAlways` skips only the
  address-range check on the same candidate resource while still enforcing
  holder mode, foreign lock, and blocking (`D51`).
- **Same-resource address span checking & generic bridge policy (`S-20`, `P14`, owner decisions Q1, Q2, Q4)**:
  `Resource::check_address_span` checks that the full requested byte span
  `[start, start + byte_len - 1]` falls within at least one declared 32-bit
  `AddressRange` on the **same** candidate resource (`byte_len = 1` for
  Classic `okRegisterBridge` and `MapsToAddress`, `byte_len = 4` for AXI
  Lite and 4-byte REST AXI Full, `byte_len = length` for gRPC AXI Full /
  Bulk). When self-overlapping ranges in one resource disagree on `private`,
  the private one wins (`44-B Q1`). AXI addresses `> 0xFFFF_FFFF` cannot
  match any 32-bit `AddressRange` and are denied under `enforce` (`P14`).
  `ClassicAddressBridgePolicy` provides a documented, generic
  non-DualPort-specific `ResourceAccessPolicy` reference implementation for
  Classic WireIn-bridged register spaces.
- **ReadOnly hardware side-effect protections (`D56`, `D66`, `D75 F1, F2, F4`)**:
  - **F1**: `ResetFPGA` and AXI Lite / Full / Stream resets use
    `check_reset_access`, denying callers who hold only `ReadOnly` access on
    the device (`AccessDenied::ReadOnlyHolder`) or whose held resources are
    all foreign-locked or blocked.
  - **F2**: `UpdateTriggerOuts` and trigger reads with `update=true` use
    `check_trigger_out_update_access`, denying `ReadOnly`-only callers when
    any registered resource on the device covers a `private: true`
    `TriggerOut` (or is a `private: true` resource with omitted `endpoints`).
  - **F4**: `access_control.warn_readonly_side_effects` (default `true`, env
    `OKFPGA_WARN_READONLY_SIDE_EFFECTS`) logs a registration warning when
    `ReadOnly` holders can join a resource (`resource.private == false`) that
    exposes a non-private `TriggerOut`, `PipeOut`, `BtPipeOut`, or
    `AddressRange`.
- **`ActivateTriggerIn` bit validation**: both REST and gRPC validate
  `(0..=31).contains(&req.bit)` before computing `1u32 << req.bit` and before
  enforcement, returning `422 Unprocessable Entity` / `INVALID_ARGUMENT`.

### Rationale

One schema shared by REST, gRPC and the dashboard removes the guessing that
caused S-01 … S-09. Structured conflicts let clients say "endpoint 0x05 is
held by resource *counter-ctrl* (user *alice*)" without parsing text.
Keeping declared and holder modes separate is what makes `read_only` an
opt-in holder mode (D62) instead of a resource type.

## Files changed (44-A)

- Spec: `api/openapi.yaml` (incl. SD-01 port 8088),
  `proto/okfpga/v1/okfpga.proto`.
- Core: `crates/ok-fpga-core/src/resource.rs`,
  `crates/ok-fpga-core/src/declaration.rs` (new),
  `crates/ok-fpga-core/src/manager.rs`, `crates/ok-fpga-core/src/lib.rs`.
- Server: `crates/ok-fpga-server/src/views.rs` (new),
  `rest/resources.rs`, `rest/events.rs`, `rest/users.rs`, `rest/errors.rs`,
  `rest/classic.rs` (Linux `c_long` cast fix), `grpc/resource_mapping.rs`
  (new), `grpc/device_service.rs`, `grpc/mod.rs`, `management.rs`,
  `sweep.rs`, `main.rs`.
- Dashboard: `web/app.js`.
- Docs: this ADR, `docs/architecture/overview.md`, ADR-020 superseded
  markers, `docs/development-roadmap.md`.

## Files changed (0.3.0, D75–D77)

- Spec: `api/openapi.yaml` (single-endpoint reads, `update` parameter,
  422s, ReadOnly and `private` text, D77), `proto/okfpga/v1/okfpga.proto`
  (`update` fields, RPC and `private` comments, D77).
- Server: `crates/ok-fpga-server/src/snapshot.rs` (new),
  `rest/classic.rs`, `grpc/classic_service.rs`, `main.rs`.
- Version: `Cargo.toml`, `Cargo.lock` (0.3.0).
- Docs: this ADR, `docs/resource-management-developer-guide.md`,
  `docs/resource-management-recommendations.md`,
  `docs/development-roadmap.md`.

## Files changed (0.3.1, D78)

- Spec: `api/openapi.yaml` (`mask` default 0, `triggered` description),
  `proto/okfpga/v1/okfpga.proto` (`IsTriggeredRequest.mask` comment).
- Server: `crates/ok-fpga-server/src/rest/classic.rs` (`TriggerOutQuery`
  mask default 0, tests).
- Version: `Cargo.toml`, `Cargo.lock` (0.3.1).
- Docs: this ADR, the developer guide, the recommendations, the roadmap.

## Files changed (0.3.2, heartbeat text and shipped enforcement)

Owner permission 2026-10-11 (API Specification review):

- Spec: `api/openapi.yaml` `/heartbeat` now says "at least every 24
  seconds while idle" (was "recommended: every 30 seconds"), matching
  ADR-020 §5; `proto/okfpga/v1/okfpga.proto` `Ping` comment says the same.
- Config: repo `config.toml` ships `endpoint_enforcement = "enforce"`. The
  built-in default in `config.rs` stays `"off"` until 44-E, which also
  extends enforcement to every data-port route.
- Version: `Cargo.toml`, `Cargo.lock` (0.3.2).
- Docs: this ADR, `docs/API-Specification/` (Appendix A, ref comments),
  `docs/development-roadmap.md`.

## Files changed (0.3.3, 44-B)

Owner permission 2026-10-11 for the spec text changes (Q2):

- Spec: `api/openapi.yaml` (`ResourceConflict.items`, `address_ranges` and
  `entries` descriptions); `proto/okfpga/v1/okfpga.proto`
  (`ResourceConflictDetail.items = 6`, comments).
- Core: `crates/ok-fpga-core/src/declaration.rs` (Rule A, R1, strict
  address parsing, duplicate check, data-port and self-overlap helpers),
  `crates/ok-fpga-core/src/resource.rs` (`ResourceConflict.items`,
  `with_items`).
- Server: `crates/ok-fpga-server/src/rest/resources.rs`,
  `grpc/device_service.rs`, `grpc/resource_mapping.rs`, `config.rs`
  (`warn_range_self_overlap`), `warnings.rs` (new), `main.rs`.
- Config: `config.toml`.
- Version: `Cargo.toml`, `Cargo.lock` (0.3.3).
- Docs: this ADR, `docs/API-Specification/`, `docs/development-roadmap.md`.

## Files changed (0.3.4, 44-C)

Owner permission 2026-11-02 for the `ConflictCode::Private` (`"private"`)
spec addition (Q2) and S-22 scoping (Q1 Option A):

- Spec: `api/openapi.yaml` (`ResourceConflict.code` adds `private`,
  `/resources/{resource_id}/acquire` 409 description);
  `proto/okfpga/v1/okfpga.proto` (`ResourceConflictDetail.code` and
  `DeviceService` comments add `private`).
- Core: `crates/ok-fpga-core/src/resource.rs` (`ConflictCode::Private`,
  `AcquireOutcome`, `HolderModeMismatch`, `DeclaredModeMismatch`,
  `DefaultAccessPolicy::can_acquire`, `ResourceRegistry::acquire` /
  `release` / `release_all_for_user`, `check_endpoint_access`),
  `crates/ok-fpga-core/tests/device_resource_integration.rs`.
- Server: `crates/ok-fpga-server/src/rest/resources.rs` (S-03 mode
  precedence, idempotent event suppression, Tier 3 tests),
  `crates/ok-fpga-server/src/grpc/device_service.rs` (session touch on
  acquire/release, idempotent event suppression, Tier 3 tests).
- Version: `Cargo.toml`, `Cargo.lock` (0.3.4).
- Docs: this ADR, `docs/API-Specification/`, `docs/archive/roadmap-phases-44.md`,
  `docs/development-roadmap.md`.

## Files changed (0.3.5, 44-D)

Owner decisions 2026-11-02 (`blocked` conflict fields Q1, idempotent repeat
lock Q2, implicit unlock events Q3):

- Core: `crates/ok-fpga-core/src/resource.rs` (`Resource::shares_item_with`,
  `LockOutcome`, `LockError`, `ReleasedResource`, `find_blocking_resource`,
  `ResourceRegistry::blocking_conflict_for` / `lock` / `unlock` /
  `release_with_info` / `release_all_for_user_with_info`, unit tests),
  `crates/ok-fpga-core/tests/device_resource_integration.rs`.
- Server: `crates/ok-fpga-server/src/rest/resources.rs`,
  `crates/ok-fpga-server/src/rest/server.rs`,
  `crates/ok-fpga-server/src/sweep.rs`,
  `crates/ok-fpga-server/src/grpc/resource_mapping.rs` (`lock_error_status`,
  `device_event_to_proto`),
  `crates/ok-fpga-server/src/grpc/device_service.rs` (`watch_device_events`,
  `lock_resource`, `unlock_resource`, `release_resource`, `disconnect`, Tier 3 tests).
- Version: `Cargo.toml`, `Cargo.lock` (0.3.5).
- Docs: this ADR, `docs/API-Specification/`, `docs/archive/roadmap-phases-44.md`,
  `docs/development-roadmap.md`.

## Files changed (0.3.6, 44-E)

Owner decisions 2026-11-03 (`P14` proto comment & Later warning schema Q1,
omitted `endpoints` Pipe coexistence & generic `ClassicAddressBridgePolicy`
Q2, default `"enforce"` & Later unit test migration Q3a/Q3b, full
`[start, start + length - 1]` span checking Q4, Tier 2 `XEM7320` hardware
test Q5):

- Spec: `proto/okfpga/v1/okfpga.proto` (`P14` comment on AXI 64-bit request
  addresses vs 32-bit `AddressRange`).
- Core: `crates/ok-fpga-core/src/declaration.rs` (`WILDCARD_EP_LAST = 0x7F`,
  `D77`, unit tests),
  `crates/ok-fpga-core/src/resource.rs` (`AccessDenied::BlockedByOther`,
  `Resource::check_endpoint_coverage`, `Resource::check_address_span`,
  `ClassicAddressBridgePolicy`, `check_endpoint_access`,
  `check_address_access`, `check_address_span_access`, `check_reset_access`,
  `check_device_write_access`, `check_trigger_out_update_access`, 14 unit
  tests),
  `crates/ok-fpga-core/tests/device_resource_integration.rs`
  (`test_two_users_classic_bridge_enforcement_and_blocking`,
  `test_hardware_xem7320_counters_enforcement_pipeline`).
- Server: `crates/ok-fpga-server/src/config.rs` (`endpoint_enforcement`
  default `"enforce"`, `warn_readonly_side_effects`, env overrides, tests),
  `crates/ok-fpga-server/src/warnings.rs` (`readonly_side_effects_messages`,
  `wildcard_endpoint_messages`, unit tests),
  `crates/ok-fpga-server/src/rest/mod.rs` (`resolve_user_id` rejects missing
  identity, `S-18`),
  `crates/ok-fpga-server/src/rest/devices.rs`,
  `crates/ok-fpga-server/src/rest/resources.rs`,
  `crates/ok-fpga-server/src/rest/classic.rs` (`enforce_with_user`, `bit`
  validation, `update_wire_ins`/`outs`, `update_trigger_outs`,
  `get_trigger_out`, block pipes, `reset_fpga`, Tier 3 tests),
  `crates/ok-fpga-server/src/rest/axi.rs` (enforcement on all 11 AXI routes),
  `crates/ok-fpga-server/src/grpc/device_service.rs`,
  `crates/ok-fpga-server/src/grpc/classic_service.rs`,
  `crates/ok-fpga-server/src/grpc/axi_service.rs`.
- Config: `config.toml` (`warn_readonly_side_effects = true`).
- Version: `Cargo.toml`, `Cargo.lock` (0.3.6).
- Docs: this ADR, `docs/API-Specification/`, `docs/archive/roadmap-phases-44.md`,
  `docs/development-roadmap.md`.

## Open items

- ~~**44-B**: R1 overlap rules, Rule A, interface/dataport check, `private`
  storage; tighten endpoint address parsing to the spec pattern
  `^0x[0-9A-Fa-f]{2}$` (today `5` and `0x5` are accepted); reject the same
  endpoint address listed twice in one declaration (Strata 44-A review).~~
  **Done in 0.3.3** (§10).
- ~~**44-C**: REST acquire still lets the declared `access_mode` override the
  requested `mode` (S-03); A1 rules for REST and gRPC.~~
  **Done in 0.3.4** (§11).
- ~~**44-D**: lock blocking B1–B4; gRPC `WatchDeviceEvents` fan-out.~~
  **Done in 0.3.5** (§12).
- ~~**44-E**: endpoint coverage per D61 and D77 (omitted `endpoints` =
  0x00–0x7F only), enforcement on all data-port routes, enforcement on by
  default; ReadOnly F1 (`ResetFPGA`, AXI resets) and F2 (private
  TriggerOut blocks `UpdateTriggerOuts` / `update=true`) enforcement;
  `warn_readonly_side_effects` setting and warning (F4).~~
  **Done in 0.3.6** (§13).
- **Later**: TriggerOut event log (D67 reopened, roadmap 🔵 Later).
- REST `?update=abc` is rejected by axum's query extractor with a plain-text
  400, not a 422 ProblemDetail (minor).
- Pre-existing, found by the 0.3.0 Strata review (not changed in 0.3.0):
  - ~~Spec discrepancy: proto `IsTriggeredRequest.mask` default 0 vs
    OpenAPI `0xFFFFFFFF`.~~ **Resolved in 0.3.1 (D78): default 0 on both.**
  - ~~`ActivateTriggerIn` computes `1u32 << bit` before validating `bit`
    (`rest/classic.rs`, `grpc/classic_service.rs`); `bit` outside 0–31
    panics in debug builds. Validate (422 / `INVALID_ARGUMENT`) in 44-E.~~
    **Resolved in 0.3.6 (44-E).**
- **44-F**: forced cleanup on configure / clear; `reprogram` / `clear`
  delete reasons; registration vs device lifecycle (orphan sweep on unplug,
  configuration-generation check, optional bitfile name pattern), see
  roadmap 44-F.
- **44-G**: example clients and `client/python-labview-client` are out of
  step with this contract until then.
- **Spec ↔ implementation discrepancies (spec unchanged)**:
  - Acquire `user_id` is `required` in the spec; the server still accepts
    the ADR-029 `X-User-Id` fallback. gRPC acquire requires `user_id`.
  - REST `/users` returns `last_seen_at` / `count`; the spec says
    `connected_at`. gRPC fills `connected_at` with the last-seen time.
  - REST `GET /users/{user_id}` and gRPC `GetUser` are not implemented.
- `tests/mqtt_integration.rs` hardcodes `127.0.0.1:1883`; it fails without
  a local broker. Proposal: make the broker host configurable.

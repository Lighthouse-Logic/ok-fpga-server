# ADR-034: Resource Model Consolidation

**Status**: Accepted — Implemented (Phase 44: 44-A through 44-J & Phase 45 archived)
**Date**: 2026-10-11 (amended 2026-11-06)
**Commit hash**: 44-A `d3b19a6`; 0.3.0 read semantics (D75–D77) `dcb5e5a`;
0.3.1 IsTriggered mask default (D78) `6e11f9b`; 0.3.2 heartbeat text and
shipped enforcement `2e2f7a3`; 44-B registration rules (0.3.3) `1d95a6f`;
44-C acquisition rules (0.3.4) `2167914`; 44-D locking, blocking & events (0.3.5) `e562705`;
44-E enforcement pipeline (0.3.6) `7c39e74`; 44-F reprogram & clear cleanup (0.3.7) `5a1f025`;
44-G classic data-path alignment & example clients (0.3.8) `d59921e`, `96c0ed7`;
44-H dataport normalisation (0.3.9) `f1961c6`;
44-J documentation consolidation `5ea5e78`;
Phase 45 explicit registration warnings (0.3.10) pending commit
**Deciders**: Project owner, Architecture Team
**Phase**: 44 & 45 ([archive/roadmap-phases-44.md](../../archive/roadmap-phases-44.md))
**Supersedes (in part)**:
- [ADR-003](ADR-003-data-port-abstraction.md) (wording calling `okRegisterBridge` "the AXI path" and `OK_DATAPORT_UNKNOWN` exposure; see §13, §15)
- [ADR-006](ADR-006-resource-management.md) (Resource Model, Resource State Model, Resource Registry, Acquisition Protocol, and FPGA ↔ Resource Lifecycle; see §1–§6, §10–§14)
- [ADR-020](ADR-020-client-side-resource-registration.md) (§1–§3 and §6–§11; see §1–§6, §10–§14)
- [ADR-024](ADR-024-endpoint-access-enforcement.md) (§1–§7, §9, §10; see §8, §11, §13)
- [ADR-025](ADR-025-classic-address-mapping.md) (§2, §3, §5, and Future Work; see §13)
- [ADR-026](ADR-026-readonly-acquisition.md) (§1–§5, §7, §8; see §1, §8, §11, §13)

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

The historical design exploration and decision log (`D1–D78`) are recorded in
[resource-management-recommendations.md](../../resource-management-recommendations.md)
and [resource-management-developer-guide.md](../../resource-management-developer-guide.md).
This ADR and [docs/API-Specification/](../../API-Specification/) are the
normative server contract.

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
| Delete reason | `client`, `reprogram`, `clear`, `device_removed` |

SSE events carry the event name in `event:` and the variant object itself
in `data:` (no `{"VariantName": {...}}` wrapper).

### §3 Declaration schema

`CreateResourceRequest` (REST and gRPC) carries `name`, `device_serial`,
`description`, `access_mode`, `interfaces`, `endpoints`, `address_ranges`,
`metadata` (string values only, D69) and `private`. `resource_type` and
`capabilities` are removed. Presence rules (D61, D62, D68, D77):

- **No `endpoints` field** → the resource is exclusive. `access_mode` may
  be omitted (it becomes `exclusive`); `shared` is rejected (422). Coverage
  (D77, revises D68, enforced in 44-E): every **Wire and Trigger** endpoint
  (`0x00–0x7F`) only. Pipe / BTPipe endpoints (`0x80–0xBF`) must be declared
  explicitly, because the server cannot tell a Pipe from a BTPipe at the same
  address. D23 still applies (it blocks later classic resources that claim
  Wire or Trigger endpoints).
- **`endpoints` present** → `access_mode` is required (422 if missing).
- **Each entry** needs its own `access` or an `endpoints.default_access`;
  `default_access` only fills a missing `access`. Neither → 422.
- `{"entries": []}` means "no classic endpoints".

The full R1 overlap rules, Rule A and the interface/dataport check are
implemented in 44-B (0.3.3), see §10.

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
- `resource_deleted` gains `reason` (`client`, `reprogram`, `clear`,
  `device_removed`; §14).
- Proto `DeviceEvent` gains the matching messages, and the gRPC
  `WatchDeviceEvents` stream fans out all 11 event variants (§12).

### §6 Users and holdings (P5)

`UserSession` gains `holdings: [{resource_id, mode, locked}]`, on REST
`/users`, the management API and gRPC `ListUsers` (now implemented).

### §7 Undocumented routes (S-08)

`wires/in/update` (write), `wires/out/update` and `triggers/out/update`
(reads, D42) are documented in `api/openapi.yaml` and enforced in `rest/classic.rs` (§13).

*Amended in 0.3.0 (D76):* the whole-bank `GET wires/out` /
`triggers/out` (no `endpoint` query), documented in 44-A, were never an
approved feature and are **removed**. `endpoint` is now required.

### §8 ReadOnly holders (D75, 0.3.0 / 0.3.6)

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
- Spec text landed in 0.3.0; runtime enforcement of F1 and F2 and the F4
  setting landed in 44-E (0.3.6, §13).

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
  applies to the shared addresses; enforced in 44-E (§13).
- **Duplicate endpoint addresses** (422): an address may appear once per
  declaration, even `pipe_in` vs `bt_pipe_in`.
- **Strict REST address syntax** (422): `^0x[0-9A-Fa-f]{2}$`
  (`parse_ep_address`); `5`, `0x5`, `0x005`, `0X05` are rejected.
- **Q4 — conflict `items`.** `ResourceConflict.items` (REST) /
  `ResourceConflictDetail.items = 6` (proto) lists every item that clashes
  with the named resource: endpoints ascending, then that resource's ranges
  in declaration order; `item` is the first entry. `detail` ends with
  "(N conflicting items in total)" when N > 1. Wildcard vs wildcard lists
  all 128 Wire/Trigger endpoints (`0x00–0x7F`, D77).
- **Deterministic choice**: existing resources are checked in `id` order
  (the registry is a `HashMap`), so the lowest id is named.
- **Check order on create**: build (422) → device lookup (404) → interfaces
  vs data port (422) → programmed (409) → self-overlap warning → R1 (409).
  A declaration that can never be valid on a device is 422 even while the
  device is unprogrammed.
- **Wildcard coverage (D77)** is `0x00–0x7F` (Wire and Trigger endpoints; §13).
- **Q5 — registration vs device lifecycle** (unplug / reprogram during or
  after registration) is implemented in 44-F (§14).

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

### §12 Locking, blocking and events (44-D, 0.3.5)

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

### §13 Enforcement pipeline, permission matrix, coverage, and registration warnings (44-E, 0.3.6; SD-03)

Phase 44-E aligns runtime data-port enforcement across REST and gRPC with
the resource declaration, overlap, and blocking model (`B2`, `S-12..S-20`,
`D42`, `D44`, `D51`, `D56`, `D61`, `D66`, `D75`, `D77`, `P14`, `SD-03`):

- **Default enforcement & identity (`S-12`, `S-18`, owner decision Q3a)**:
  `AccessControlConfig::default().endpoint_enforcement` is `"enforce"`
  (`"off"` and `"warn"` exist only as debugging aids), and `resolve_user_id`
  rejects missing identity (`400 Bad Request`) instead of falling back to
  `"Unknown"`. Anonymous `/configure` and `/configure_from_server_path`
  requests are permitted only when `endpoint_enforcement == "off"`.
- **Route tiers**:
  | Tier | Routes / RPCs | Check |
  |---|---|---|
  | 1 Management | `/server/*`, `/devices`, `/resources/*`, `/users/*`, `/events/*`, `/disconnect`, `/heartbeat`; `ServerService`, management `DeviceService` RPCs | user id |
  | 2 Device | `/devices/{serial}`, `/configure*`, `/settings/*`, `/sensors`, `/syzygy`; device-info / configure / sensor / settings RPCs | user id |
  | 3 Data port | `/devices/{serial}/classic/**`, `/devices/{serial}/axi/**`; `ClassicDataPortService`, `AXIDataPortService` | user id + holds ≥1 resource on device + permission matrix & B2 pipeline |
- **Read vs. Write operation classes (`D16`, `D21`, `D42`, `D44`, `D75`, `D76`)**:
  | Class | Operations |
  |---|---|
  | **Read** | `UpdateWireOuts`, `GetWireOutValue`, `UpdateTriggerOuts`, `IsTriggered`, `GetTriggerOutVector`, `ReadFromPipeOut`, `ReadFromBlockPipeOut`; `ReadRegister(s)`; direct AXI reads (`AXILite.Read*`, `AXIFull.Read*`, `AXIStream.Read`) |
  | **Write** | `SetWireInValue`, `UpdateWireIns`, `ActivateTriggerIn`, `WriteToPipeIn`, `WriteToBlockPipeIn`; `WriteRegister(s)`; direct AXI writes (`AXILite.Write*`, `AXIFull.Write*`, `AXIStream.Write`); `ResetFPGA` and AXI Lite / Full / Stream resets (`D75 F1`). An `ADR-025` hook may map a Classic write to a 32-bit address, which is then checked against the same candidate resource's `address_ranges` |
- **Normative Write Permission Matrix (`SD-03`, `D6`, `D44`, `D46`)** (`U` = requesting user; `R` = a resource `U` holds on the device; a write is allowed if **at least one** held resource `R` allows it; "usable" = not locked by another user and not blocked per §12):
  | `U`'s hold on `R` | Shared EP | Exclusive EP | Shared range (`okRegisterBridge`, `ADR-025`, or AXI) | Exclusive range | Item **not** declared by `R` |
  |---|---|---|---|---|---|
  | None | ❌ | ❌ | ❌ | ❌ | ❌ |
  | `Shared` holder, `R` usable (unlocked, or locked by `U`) | ✅ | ✅ | ✅ | ✅ | ❌ |
  | `Shared` holder, `R` locked by another user | ❌ | ❌ | ❌ | ❌ | ❌ |
  | `Shared` holder, `R` **blocked** by another user's lock on an overlapping resource | ❌ | ❌ | ❌ | ❌ | ❌ |
  | `Exclusive` holder (`R` cannot be foreign-locked or blocked under `R1`) | — (`Rule A`) | ✅ | — (`Rule A`) | ✅ | ❌ |
  | `ReadOnly` holder | ❌ | ❌ | ❌ | ❌ | ❌ |
  - A lock is **never** required by the server to write an exclusive item or range inside a `Shared` resource (`S-14`, `D44`, `D46`); multi-step sequences are coordinated by clients using resource locks.
  - `private` has no effect on writes (it restricts only `ReadOnly`, which cannot write).
- **Normative Read Permission Matrix (`SD-03`, `D16`, `D21`, `D29`, `D42`, `D75`)**:
  | `U`'s hold on `R` | Read, non-private (`private: false`) | Read, `private: true` | `ADR-025` mapped access (requires Classic writes) |
  |---|---|---|---|
  | None | ❌ | ❌ | ❌ |
  | `Shared` / `Exclusive` holder (any lock state, incl. foreign-locked or blocked) | ✅ | ✅ | follows the **Write** table |
  | `ReadOnly` holder | ✅ (`UpdateTriggerOuts` / trigger `update=true` ❌ if any resource on the device declares a `private: true` `TriggerOut`, `D75 F2`) | ❌ | ❌ (`D12`, `D29`) |
- **Ordered 7-Step `B2` Data-Port Enforcement Pipeline (`SD-03`, `B2`)**:
  ```text
  1. User identified?                                       (S-18; 400 / 403 / PERMISSION_DENIED if missing)
  2. User holds >= 1 resource on the device?                (Tier-3 device gate; 403 / PERMISSION_DENIED if none)
  3. ResetFPGA / AXI reset (no item):
       caller must hold >= 1 resource on the device that is NOT ReadOnly,
       NOT foreign-locked, and NOT blocked                  (D56, D66, D75 F1)
  4. UpdateTriggerOuts / trigger read with update=true:
       caller must hold >= 1 covering resource (or any resource for standalone
       UpdateTriggerOuts); if all held resources on the device are ReadOnly and
       ANY resource on the device covers a private TriggerOut -> deny 403 (D75 F2)
  5. Candidate resources = resources on the device held by U:
       READ:  keep candidates that cover the EP or full address span
              [start, start + byte_len - 1]; allow if >= 1 candidate is held as
              Shared/Exclusive, or held as ReadOnly and the item is not private
              (for self-overlapping ranges, private: true wins, 44-B Q1)
       WRITE: filter out candidates held as ReadOnly, locked by another user
              (locked_by != Some(U)), or blocked (B1/B2); keep remaining
              candidates that cover the EP / address span; none left -> deny 403
  6. WRITE via a Classic EP on a remaining candidate R:
       policy.resolve_classic_address(R, U, ep, ep_type, value):
         - NoMapping / AllowAlways -> allow (AllowAlways skips only the address
           check on R, never steps 1-5, D51)
         - MapsToAddress(a) -> allow iff a is in one of R's own AddressRanges
           (same candidate R, S-20, D9, D44)
  7. WRITE / READ via okRegisterBridge or direct AXI address:
       allow iff the full byte span [start, start + byte_len - 1] falls within
       at least one 32-bit AddressRange of the same candidate R (S-20, P14)
  ```
  - Checked **once per command** at initiation; bulk / pipe / stream byte transfers run without per-byte checks.
- **Wildcard endpoint coverage (`D77`, owner decision Q2)**:
  `WILDCARD_EP_LAST` is narrowed from `0xBF` to `0x7F` so a Classic resource
  that omits `endpoints` (`None`) claims only Wire and Trigger endpoints
  (`0x00–0x7F`) and leaves Pipe/BTPipe (`0x80–0xBF`) available to other
  resources on the same device. Registration logs `warn!` messages via
  `warnings::warn_wildcard_endpoints` when `endpoints` is omitted or when an
  `endpoints: None` resource and a Pipe-declaring resource coexist on the
  same device.
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

### §14 Reprogram and clear forced resource cleanup (44-F, 0.3.7)

- **Forced cleanup on `ConfigureFPGA*` and `ClearFPGAConfiguration` (`D26`, `Q44-14`, `D74`, `P11`, `P12`, owner decisions `q1`, `q3a`, `q3b`)**:
  - `POST /devices/{serial}/configure`, `POST /devices/{serial}/configure_from_server_path`,
    `DELETE /devices/{serial}/configure`, and gRPC `ConfigureFPGA`,
    `ConfigureFPGAFromMemory`, and `ClearFPGAConfiguration` are unconditional
    forced overrides at every enforcement level (`off`, `warn`, `enforce`).
    The `409 Conflict` ("Device has resources in use") responses are removed
    from `api/openapi.yaml` (`q1`).
  - **Pre-FFI cleanup and failure state (`q3a`)**: after validating request
    inputs (device exists, caller identity valid when `enforcement != Off`,
    bitfile bytes/path present), the server transitions the device to
    `DeviceState::Connected` (unprogrammed, bumping `config_generation`),
    clears cached data port handles on `OpenDevice`, and runs
    `AppState::force_cleanup_device_resources` **before** calling into FFI:
    1. For every shared resource on the device with an active lock
       (`access_mode == Some(Shared)` and `locked_by == Some(user)`), emit
       `DeviceEvent::ResourceUnlocked` and dashboard WS `resource_unlocked`.
    2. Release all holders and notify `ResourceAccessPolicy::on_resource_invalidated`.
    3. Remove all resources on the device from `ResourceRegistry` and emit
       `DeviceEvent::ResourceDeleted { reason }` (`DeleteReason::Reprogram` or
       `DeleteReason::Clear`) and dashboard WS `resource_deleted` /
       `resources_cleared`.
    If the FFI bitfile transfer fails mid-way, the device remains in
    `DeviceState::Connected` (unprogrammed) with no stale resources or data
    ports (`q3a`).
  - **Tier 3 in-memory fallback (`q3b`)**: when `AppState.handle_pool` is
    `None` (Tier 3 tests without `libokFrontPanel`), `ConfigureFPGA*` and
    `ClearFPGAConfiguration` still perform the full `D26` forced cleanup,
    update `DeviceState` (`Programmed` / `Connected`), and broadcast events in
    memory.
  - **`ResetFPGA` retains resources (`D27`)**: `POST /devices/{serial}/classic/reset`
    and `ClassicDataPortService.ResetFPGA` reset logic inside the running
    bitfile without changing the loaded bitfile, so registered resources and
    holders are retained.
- **Hot-unplug & orphan resource sweep (`DeleteReason::DeviceRemoved`, owner decision `q2`)**:
  - Added `"device_removed"` (`DELETE_REASON_DEVICE_REMOVED = 3` in
    `okfpga.proto`, `DeleteReason::DeviceRemoved` in `ok-fpga-core`) to
    `DeleteReason` in `api/openapi.yaml`, `okfpga.proto`, `ok-fpga-core`, and
    `resource-model.typ` (`q2` Option A).
  - Hot-unplug in `main.rs` calls `force_delete_for_device(serial)` and
    `broadcast_force_deleted_resources(..., DeleteReason::DeviceRemoved)` so
    holders receive `ResourceUnlocked` (if locked) + `ResourceDeleted { reason: "device_removed" }`.
  - `sweep_orphan_resources` in `sweep.rs` runs on every sweep tick and
    force-deletes any resource whose device is absent from `DeviceRegistry`
    (`DeleteReason::DeviceRemoved`) or no longer programmed
    (`DeleteReason::Clear`).
- **Configuration generation check vs bitfile-hash/regex matching (`44-B Q5`, owner decision `q4`)**:
  - **Rejected & dropped (not deferred to Later)**: the proposal from `44-B Q5`
    to allow optional bitfile-matching regexes or hashes on resource
    declarations was evaluated in `44-F q4` and **permanently dropped**.
    Owner rationale: FPGA bitfile hashes (MD5/SHA) change even when a design
    is rebuilt with zero RTL code changes (due to synthesis timestamps and
    placement/routing seeds), and bitfile filenames are not authoritative.
  - Instead, `FpgaDevice` carries a monotonic `config_generation: u64` counter
    bumped on every `DeviceRegistry::update_state` call. `create_resource`
    (REST and gRPC) captures `expected_gen = dev.config_generation` at device
    lookup and re-verifies `dev.is_programmed() && dev.config_generation == expected_gen`
    inside `ResourceRegistry::try_register_with_precondition` while holding
    the `ResourceRegistry` write lock. Combined with `D26` forced cleanup on
    every `ConfigureFPGA*` and `ClearFPGAConfiguration`, this guarantees that
    resources only exist for the exact configuration session in which they
    were registered.
- **Proprietary FrontPanel SDK shared libraries (`q5`)**:
  - Added `FrontPanel-6.1.0/*.so`, `*.so.*`, and `*.dylib` to `.gitignore` so
    locally installed `libokFrontPanel.so` binaries are never committed, and
    added a **🔵 Later** roadmap item to purge tracked proprietary Opal Kelly
    FrontPanel SDK files from git.

### §15 Dataport normalisation (44-H, 0.3.9)

Phase 44-H normalises the physical host transport data port across REST,
gRPC, core types, and clients (`D63`, `D70`, `S-10`, `SD-05`, `P9`, `P16`):

- **Public values `legacy` and `axi` only (`D63`, `S-10`, `P16`)**:
  - OpenAPI `GET /devices/{serial}/dataport` and `DeviceInfo.properties.dataport`
    use `enum: [legacy, axi]` (`SD-05`).
  - Protobuf `enum DataportType` is `DATAPORT_LEGACY = 0; DATAPORT_AXI = 1;`
    (`P16`), and `AXIDataPortService` comments state that direct AXI runs via
    `FPGADataPortAXI` on `OK_DATAPORT_AXI` devices only (`P9`, `D50`, `D64`).
  - `DataportType::as_str()` and `Display` return `"legacy"` / `"axi"` so
    both `GET /devices` and `GET /devices/{serial}/dataport` return lowercase
    canonical strings.
  - gRPC `DeviceServiceImpl::dataport_to_proto` explicitly maps
    `DataportType::Legacy` → `DATAPORT_LEGACY` (`0`) and `DataportType::Axi`
    → `DATAPORT_AXI` (`1`), and `GetDataportType` is wired to `DeviceRegistry`.
- **Unsupported `OK_DATAPORT_UNKNOWN` filtering (`D70`)**:
  - Devices reporting `OK_DATAPORT_UNKNOWN` (`0`) are skipped during FFI
    enumeration (`scanner.rs`, `device_monitor.rs`), omitted from
    `DeviceRegistry::list()` / `count()`, and return `404` / `NOT_FOUND` on
    single-device lookups (`DeviceRegistry::get()`, `contains()`).

### §16 WireIn read-back and host-side shadow values (SD-04, D16)

- **No hardware `WireIn` read-back in FrontPanel**: Opal Kelly's `okWireIn`
  HDL module is strictly host-to-FPGA (`ep_dataout` only); there is no
  hardware read-back path from the FPGA fabric for `WireIn` endpoints.
- **Host-side staging cache only**: FrontPanel's `GetWireInValue` reads only
  the host-side staging buffer maintained inside the single `okCFrontPanel`
  handle from prior `SetWireInValue` calls (whether or not `UpdateWireIns`
  has been called). REST does not expose a `WireIn` read route.
- **Client guidance**: clients that need to remember what they wrote to a
  `WireIn` endpoint should cache the written values client-side, or place the
  `WireIn` in an `exclusive` resource (or coordinate multi-step updates under
  a `shared` resource lock) when multiple clients share a `WireIn`.
  An optional server-side `WireIn` shadow read-back endpoint is tracked as a
  **🔵 Later** research item (`Q44-6`).

### §17 Explicit registration warnings in CreateResource responses (Phase 45, 0.3.10)

Owner decisions 2026-11-06 (`q1` spec edit permission, `q2` Option A1 REST
`Resource.warnings`, `q3` gRPC `ResourceResponse.warnings = 3`, `q4`
post-registration evaluation + `warn_wildcard_endpoints` toggle, `q5` client
updates):

- **REST (`api/openapi.yaml` & `ResourceView`)**:
  - `components/schemas/Resource` defines an optional `readOnly: true` field
    `warnings: array of string`.
  - `ResourceView` carries `#[serde(skip_serializing_if = "Vec::is_empty")] pub warnings: Vec<String>`,
    populated on `201 Created` from `POST /resources` via
    `ResourceView::from_with_warnings(&created, warnings)` when at least one
    enabled registration warning fires, and omitted from serialized JSON when
    empty or on `GET /resources`, `GET /resources/{id}`, acquire, lock, and
    unlock responses.
- **gRPC (`proto/okfpga/v1/okfpga.proto` & `DeviceServiceImpl`)**:
  - `ResourceResponse` defines `repeated string warnings = 3;`, populated by
    `CreateResource` and empty (`Vec::new()`) on `GetResource`,
    `AcquireResource`, `LockResource`, and `UnlockResource`.
- **Unified post-registration warning evaluation (`warnings.rs` & `config.rs`)**:
  - Added `warn_wildcard_endpoints: bool` (default `true`, env override
    `OKFPGA_WARN_WILDCARD_ENDPOINTS`) to `[access_control]` alongside
    `warn_range_self_overlap` and `warn_readonly_side_effects`.
  - All three warning generators are evaluated and logged via `tracing::warn!`
    *after* `try_register_with_precondition` succeeds via
    `collect_and_log_registration_warnings` in deterministic order:
    (1) `self_overlap_messages`, (2) `readonly_side_effects_messages`, and
    (3) `wildcard_endpoint_messages` (with existing device resources sorted by
    `id`). Failed registrations (`404` / `409` / `422`) never emit false-alarm
    server log warnings.

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

## Files changed (0.3.7, 44-F)

Owner decisions 2026-11-03 (spec/proto permission `q1`, `DeleteReason::DeviceRemoved`
`q2` Option A, pre-FFI forced cleanup & `Connected` state on FFI failure `q3a`,
Tier 3 in-memory cleanup fallback `q3b`, rejection/dropping of bitfile
regex/hash matching on resource declarations `q4`, `.gitignore` for
`FrontPanel-6.1.0/*.so*` & Later SDK cleanup `q5`):

- Gitignore: `.gitignore` (`FrontPanel-6.1.0/*.so`, `*.so.*`, `*.dylib`).
- Spec: `api/openapi.yaml` (removed `409` from `POST /configure`,
  `DELETE /configure`, and `POST /configure_from_server_path`; added
  `device_removed` to `ResourceDeletedEvent.reason`),
  `proto/okfpga/v1/okfpga.proto` (`DELETE_REASON_DEVICE_REMOVED = 3`, `P11`,
  `P12`, `D26` comments).
- Core: `crates/ok-fpga-core/src/device.rs` (`FpgaDevice.config_generation`),
  `crates/ok-fpga-core/src/manager.rs` (`DeleteReason::DeviceRemoved`,
  `DeviceRegistry::update_state` generation bump, unit test),
  `crates/ok-fpga-core/src/resource.rs` (`ForceDeletedResource`,
  `RegisterError::PreconditionFailed`, `try_register_with_precondition`,
  `force_delete_for_device`, `force_delete_resource`),
  `crates/ok-fpga-core/tests/device_resource_integration.rs`
  (`test_emulated_reprogram_and_clear_force_delete_resources`, live hardware
  `test_hardware_xem7320_counters_enforcement_pipeline` step 5).
- Server: `crates/ok-fpga-server/src/state.rs`
  (`AppState::force_cleanup_device_resources`,
  `broadcast_force_deleted_resources`),
  `crates/ok-fpga-server/src/handle_pool.rs` (`OpenDevice::clear_data_ports`,
  `DeviceHandlePool::clear_configuration`),
  `crates/ok-fpga-server/src/rest/devices.rs` (`DELETE /devices/{serial}/configure`
  wired to `clear_fpga_configuration`, `D26`/`q3a`/`q3b` in `configure_fpga*`,
  Tier 3 unit tests),
  `crates/ok-fpga-server/src/rest/resources.rs` (`create_resource` generation
  precondition check, Tier 3 unit test),
  `crates/ok-fpga-server/src/grpc/device_service.rs` (`configure_fpga`,
  `configure_fpga_from_memory`, `clear_fpga_configuration`, `create_resource`
  generation precondition check, Tier 3 unit tests),
  `crates/ok-fpga-server/src/grpc/resource_mapping.rs`
  (`DeleteReason::DeviceRemoved` mapping),
  `crates/ok-fpga-server/src/sweep.rs` (`sweep_orphan_resources`, Tier 3 unit
  test),
  `crates/ok-fpga-server/src/main.rs` (hot-unplug forced cleanup + events).
- Version: `Cargo.toml`, `Cargo.lock` (0.3.7).
- Docs: this ADR, `docs/API-Specification/`, `docs/archive/roadmap-phases-44.md`,
  `docs/development-roadmap.md`.

## Files changed (0.3.8, 44-G)

- Server:
  - `crates/ok-fpga-server/src/rest/classic.rs` (`S-05` `application/octet-stream`
    pipe and block-pipe routes with `GET /pipes/out` and `GET /pipes/block-out`,
    `S-06` bulk register `addresses[]` / `entries[]` with pre-FFI batch address
    enforcement, `S-07` `WireInRequest.auto_update`, Tier 3 unit tests),
  - `crates/ok-fpga-server/src/grpc/classic_service.rs` (`length >= 0` and
    `block_size > 0` validation, `test_grpc_classic_pipe_register_parity`),
  - `crates/ok-fpga-core/tests/device_resource_integration.rs`
    (`test_emulated_pipe_and_register_round_trip_with_ok_realm_test`,
    `test_hardware_xem7320_wire_in_auto_update_semantics`).
- Clients:
  - `client/python-labview-client/` (`0.2.0` → `0.2.1`; `Holder` and updated
    `Resource` models, optional `acquire_resource` `mode`, unit tests),
  - `client-examples/rust-rest-client/`, `client-examples/python-rest-client/`
    (`0.1.0` → `0.1.1`), `client-examples/rust-grpc-client/`,
    `client-examples/python-grpc-client/` (`0.1.0` → `0.1.1`, regenerated
    protobuf stubs), `client-examples/rust-rest-gui-common/`,
    `client-examples/rust-grpc-gui-common/`,
    `client-examples/python_rest_gui_common/` (`0.1.0` → `0.1.1`),
    `client-examples/python_grpc_gui_common/` (`0.1.0` → `0.1.1`, regenerated
    protobuf stubs).
- Version: `Cargo.toml`, `Cargo.lock` (0.3.8).
- Docs: this ADR, `docs/API-Specification/`, `docs/archive/roadmap-phases-44.md`,
  `docs/development-roadmap.md`, `docs/architecture/overview.md`.

## Files changed (0.3.9, 44-H)

Owner permission 2026-11-05 for the `api/openapi.yaml` and
`proto/okfpga/v1/okfpga.proto` dataport normalisation changes (`D63`, `D70`,
`S-10`, `SD-05`, `P9`, `P16`):

- Spec: `api/openapi.yaml` (`GET /devices/{serial}/dataport` and
  `DeviceInfo.properties.dataport` normalised to `enum: [legacy, axi]` with
  `SD-05` descriptions),
  `proto/okfpga/v1/okfpga.proto` (`enum DataportType` `DATAPORT_LEGACY = 0;
  DATAPORT_AXI = 1;`, `GetDataportType` and `AXIDataPortService` comments).
- Core: `crates/ok-fpga-core/src/device.rs` (`DataportType::as_str`,
  `DataportType::is_supported`, lowercase `Display`, unit tests),
  `crates/ok-fpga-core/src/manager.rs` (`DeviceRegistry` `D70` filtering on
  `get`, `list`, `count`, `contains`, `on_device_added`, unit test),
  `crates/ok-fpga-core/src/declaration.rs` (updated test assertion).
- Server: `crates/ok-fpga-server/src/scanner.rs`,
  `crates/ok-fpga-server/src/device_monitor.rs`,
  `crates/ok-fpga-server/src/rest/devices.rs` (`as_str()`, Tier 3 tests
  `test_dataport_legacy_maps_to_legacy`, `test_dataport_axi_maps_to_axi`,
  `test_list_devices_unknown_dataport_omitted`,
  `test_get_device_unknown_dataport_not_found`),
  `crates/ok-fpga-server/src/grpc/device_service.rs` (`dataport_to_proto`,
  `get_dataport_type` implementation, Tier 3 tests
  `test_grpc_dataport_legacy_and_axi_mapping`,
  `test_grpc_get_device_unknown_dataport_not_found`).
- Clients: `client/python-labview-client/` (`0.2.1` → `0.2.2`),
  `client-examples/python-rest-client/` (`0.1.1` → `0.1.2`),
  `client-examples/python-grpc-client/` (`0.1.1` → `0.1.2`, regenerated
  protobuf stubs),
  `client-examples/python_grpc_gui_common/` (`0.1.1` → `0.1.2`, regenerated
  protobuf stubs).
- Version: `Cargo.toml`, `Cargo.lock` (0.3.9).
- Docs: this ADR, `docs/API-Specification/`, `docs/archive/roadmap-phases-44.md`,
  `docs/development-roadmap.md`, `docs/architecture/overview.md`.

## Files changed (44-J)

- Docs:
  - `docs/architecture/adr/ADR-034-resource-model-consolidation.md` (consolidated
    `SD-03` read/write permission matrix and 7-step `B2` pipeline into §13,
    added `SD-04` `WireIn` read-back §16, updated status and `Supersedes`
    header),
  - `docs/architecture/adr/ADR-003-data-port-abstraction.md`,
    `docs/architecture/adr/ADR-006-resource-management.md`,
    `docs/architecture/adr/ADR-020-client-side-resource-registration.md`,
    `docs/architecture/adr/ADR-024-endpoint-access-enforcement.md`,
    `docs/architecture/adr/ADR-025-classic-address-mapping.md`,
    `docs/architecture/adr/ADR-026-readonly-acquisition.md`,
    `docs/architecture/adr/ADR-028-configure-fpga-api-alignment.md`
    (added section-level *Superseded by ADR-034* markers),
  - `docs/architecture/adr/ADR-029-user-identification-consistency.md`,
    `docs/architecture/adr/ADR-030-sse-event-expansion.md` (updated status from
    `Proposed` to `Accepted — Implemented`),
  - `docs/architecture/overview.md` (synchronised ADR index statuses and fixed
    relative links in §9),
  - `docs/guides/python-rest-client-guide.md` (`SD-06` `ADR-028` configure
    routes + `D26` forced cleanup note, `ADR-034` resource/snapshot alignment,
    relative link fixes),
  - `docs/client/hdl-resource-mapping-guide.md` (`SD-08` registration check-order
    and structured conflict clarifications),
  - `docs/resource-management-developer-guide.md`,
    `docs/resource-management-recommendations.md` (reduced to pointers into
    `ADR-034` and `docs/API-Specification/` while preserving the `D1–D78`
    decision log),
  - `docs/API-Specification/` (`main.typ`, `chapters/appendix-status.typ`, and
    rebuilt `ok-fpga-server-api-specification.pdf`),
  - `docs/archive/roadmap-phases-44.md`, `docs/development-roadmap.md`
    (archived Phase 44-J and collapsed Phase 44 in the roadmap).

## Files changed (0.3.10, Phase 45)

Owner decisions 2026-11-06 (`q1`–`q5`):

- Spec: `api/openapi.yaml` (`POST /resources` 201 description and
  `Resource.properties.warnings`),
  `proto/okfpga/v1/okfpga.proto` (`ResourceResponse.warnings = 3`).
- Server & Config: `config.toml` (`warn_wildcard_endpoints = true`),
  `crates/ok-fpga-server/src/config.rs` (`warn_wildcard_endpoints`,
  `OKFPGA_WARN_WILDCARD_ENDPOINTS`, unit test),
  `crates/ok-fpga-server/src/warnings.rs` (`RegistrationWarningToggles`,
  `collect_and_log_registration_warnings`, unit test),
  `crates/ok-fpga-server/src/views.rs` (`ResourceView.warnings`,
  `ResourceView::from_with_warnings`, unit test),
  `crates/ok-fpga-server/src/rest/resources.rs` (`create_resource` post-registration
  warning collection, Tier 3 unit tests),
  `crates/ok-fpga-server/src/grpc/device_service.rs` (`create_resource`
  post-registration warning collection, Tier 3 unit test).
- Clients: `client/python-labview-client/` (`0.2.2` → `0.2.3`; `Resource.warnings`),
  `client-examples/rust-rest-gui-common/` (`ResourceInfo.warnings`),
  `client-examples/rust-grpc-gui-common/` (`ResourceInfo.warnings`),
  `client-examples/python_rest_gui_common/` (`0.1.1` → `0.1.2`; `ResourceInfo.warnings`),
  `client-examples/python_grpc_gui_common/` (`0.1.2` → `0.1.3`; `ResourceInfo.warnings`,
  regenerated protobuf stubs),
  `client-examples/python-grpc-client/` (`0.1.2` → `0.1.3`, regenerated protobuf stubs).
- Version: `Cargo.toml`, `Cargo.lock` (0.3.10).
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
- ~~**44-F**: forced cleanup on configure / clear; `reprogram` / `clear` /
  `device_removed` delete reasons; registration vs device lifecycle (orphan
  sweep on unplug, configuration-generation check; optional bitfile regex/hash
  matching evaluated and rejected/dropped per `q4`).~~
  **Done in 0.3.7** (§14).
- ~~**44-G**: example clients and `client/python-labview-client` are out of
  step with this contract until then.~~ **Done in 0.3.8**.
- ~~**44-H**: dataport normalisation (`legacy` / `axi` only, `D63`, `D70`,
  `S-10`, `SD-05`, `P9`, `P16`).~~ **Done in 0.3.9** (§15).
- ~~**44-J**: documentation consolidation (`SD-03`, `SD-04`, `SD-06`, `SD-08`,
  ADR superseded/status markers, working-doc pointers, roadmap collapse).~~
  **Done** (§13, §16).
- **Later**: TriggerOut event log (D67 reopened, roadmap 🔵 Later).
- **Spec ↔ implementation discrepancies (spec unchanged)**:
  - Acquire `user_id` is `required` in the spec; the server still accepts
    the ADR-029 `X-User-Id` fallback. gRPC acquire requires `user_id`.
  - REST `/users` returns `last_seen_at` / `count`; the spec says
    `connected_at`. gRPC fills `connected_at` with the last-seen time.
  - REST `GET /users/{user_id}` and gRPC `GetUser` are not implemented.
- `tests/mqtt_integration.rs` hardcodes `127.0.0.1:1883`; it fails without
  a local broker. Proposal: make the broker host configurable.

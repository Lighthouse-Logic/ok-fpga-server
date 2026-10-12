# ADR-034: Resource Model Consolidation

**Status**: Accepted — 44-A implemented incl. post-44-A read semantics (0.3.0) and mask default (0.3.1);
44-A archived; 44-B implemented (0.3.3); 44-C … 44-J pending
**Date**: 2026-10-11
**Commit hash**: 44-A `c5e4f79`; 0.3.0 read semantics (D75–D77) `5de5ab6`;
0.3.1 IsTriggered mask default (D78) `d88e5c5`
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
  (`code` ∈ `overlap`, `locked`, `blocked`, `mode_mismatch`, `validation`;
  `blocked` and `mode_mismatch` are emitted from 44-D / 44-C).
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

## Open items

- ~~**44-B**: R1 overlap rules, Rule A, interface/dataport check, `private`
  storage; tighten endpoint address parsing to the spec pattern
  `^0x[0-9A-Fa-f]{2}$` (today `5` and `0x5` are accepted); reject the same
  endpoint address listed twice in one declaration (Strata 44-A review).~~
  **Done in 0.3.3** (§10).
- **44-C**: REST acquire still lets the declared `access_mode` override the
  requested `mode` (S-03); A1 rules for REST and gRPC.
- **44-D**: lock blocking B1–B4; gRPC `WatchDeviceEvents` fan-out.
- **44-E**: endpoint coverage per D61 and D77 (omitted `endpoints` =
  0x00–0x7F only), enforcement on all data-port routes, enforcement on by
  default; ReadOnly F1 (`ResetFPGA`, AXI resets) and F2 (private
  TriggerOut blocks `UpdateTriggerOuts` / `update=true`) enforcement;
  `warn_readonly_side_effects` setting and warning (F4).
- **Later**: TriggerOut event log (D67 reopened, roadmap 🔵 Later).
- REST `?update=abc` is rejected by axum's query extractor with a plain-text
  400, not a 422 ProblemDetail (minor).
- Pre-existing, found by the 0.3.0 Strata review (not changed in 0.3.0):
  - ~~Spec discrepancy: proto `IsTriggeredRequest.mask` default 0 vs
    OpenAPI `0xFFFFFFFF`.~~ **Resolved in 0.3.1 (D78): default 0 on both.**
  - `ActivateTriggerIn` computes `1u32 << bit` before validating `bit`
    (`rest/classic.rs`, `grpc/classic_service.rs`); `bit` outside 0–31
    panics in debug builds. Validate (422 / `INVALID_ARGUMENT`) in 44-E.
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

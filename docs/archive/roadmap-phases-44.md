# Archive — Phase 44: Resource Model Consolidation & Enforcement Alignment

> **Archived:** 2026-10-11 (sub-phases are appended here as they complete)
> **Source:** [development-roadmap.md](../development-roadmap.md) §44
> **ADR:** [ADR-034](../architecture/adr/ADR-034-resource-model-consolidation.md)
> **Decisions:** [resource-management-recommendations.md](../resource-management-recommendations.md) (D1–D78)

Phase 44 is still 🚧 Now. Each completed sub-phase is moved here from the
roadmap; the S-, SD-, P- item tables and the open sub-phases stay in the
roadmap until the whole phase completes.

---

## 44-A — Contract, wire formats and ADR-034 (0.2.0 → 0.3.1)

**Commits:** `d3b19a6` (44-A, 0.2.0), `656d1f6` (ADR hash), `422d714`
(MQTT follow-up item), `dcb5e5a` (post-44-A read semantics D75–D77,
0.3.0), `f414b8b` (ADR hash), `6e11f9b` (IsTriggered mask default D78,
0.3.1).

**Verification:** `cargo test --workspace` and `cargo test --workspace
--features full` green, except the 3 `tests/mqtt_integration.rs` tests that
need a local Mosquitto broker (follow-up in the roadmap, Later). Strata
second-opinion reviews for 44-A and 0.3.0 (findings verified; valid ones
applied, pre-existing ones moved to 44-E / ADR-034 open items).

**44-A Contract, wire formats and ADR-034 🔒**
- Files: `api/openapi.yaml`, `proto/okfpga/v1/okfpga.proto`,
  `crates/ok-fpga-core/src/resource.rs`, `crates/ok-fpga-server/src/rest/resources.rs`,
  `rest/errors.rs`, `rest/events.rs`, `grpc/device_service.rs`, `web/app.js`,
  ADR-034, `docs/architecture/overview.md`, ADR-020.
- OpenAPI: `CreateResourceRequest` per sketch 44.6 (S-01); `Resource` per
  S-02; acquire `mode` optional (S-03 / recs S8); structured 409 / 422
  bodies per Q44-13 (recs S3, D38, D72); `resource_locked` /
  `resource_unlocked` events and a `reason` (`client`, `reprogram`,
  `clear`) on `resource_deleted` (recs S5, D58); `private` = access rule,
  not visibility (recs S6, D57); the S-08 routes; SD-01.
- Proto: `CreateResourceRequest` field 4 = `optional ResourceAccessMode
  access_mode`, field 5 = `repeated string interfaces`, plus
  `EndpointDeclaration`, `EndpointEntry`, `AddressRange`,
  `map<string,string> metadata`, `private` (P1, D65, D71);
  `Resource.access_mode`, `repeated Holder holders` under a new field
  number, declarations (P2); `ResourceAccessMode { SHARED = 0; EXCLUSIVE =
  1; }` used as `optional`, no UNSPECIFIED; acquire `mode` `optional` (P3,
  D59, D62); `AccessMode` comment: ReadOnly "cannot write input EPs or call
  ActivateTriggerIn; may call UpdateWireOuts / UpdateTriggerOuts (reads)"
  (P4); `UserSession` holdings `{resource_id, mode, locked}` (P5); lock
  events + delete reason (P6); gRPC status mapping table and error-detail
  message per Q44-13 (P7); `x-user-id` documented as required metadata
  (P8); `LockResource` / `UnlockResource` keep returning `ResourceResponse`
  (P10); `private` comment (P13).
- Code: REST and gRPC request/response types parse and return the new
  shapes (validation rules come in 44-B); core types for declarations,
  holder `{user_id, mode}`, declared vs holder mode, structured conflict
  error; spec enum strings for state / mode in `ResourceView`, SSE and gRPC
  (S-09); `web/app.js` resources page renders holders as `user (mode)`,
  compares `state` with the new enum strings, and shows endpoints, ranges
  and metadata in the detail panel (metadata display already exists at
  `web/app.js:410–417`); users page shows holdings with mode.
- Docs: ADR-034 first version; overview.md (SD-02); ADR-020 superseded
  markers (SD-07).
- Scope notes (2026-10-10): the presence rules D61 (`access` or
  `default_access`), D62 (`access_mode` required with `endpoints`) and D68
  (no `endpoints` = exclusive, all classic EPs) move here from 44-B, because
  the new optional types need them; their 44-B tests move with them. The
  example clients (`client-examples/`) and `client/python-labview-client/`
  are **not** updated here and stay out of step with the new resource
  contract until 44-G. gRPC gets only the new `DeviceEvent` messages here;
  the `WatchDeviceEvents` stream is implemented in 44-D.
- Tests (Tier 3): `test_create_resource_request_full_schema_roundtrips`,
  `test_resource_view_holders_include_mode`,
  `test_resource_view_state_uses_spec_enum_strings`,
  `test_conflict_error_body_names_resource_and_item`,
  `test_grpc_register_resource_request_full_schema_roundtrips`,
  `test_grpc_user_session_holdings_include_mode`. Manual check: dashboard
  resources and users pages with one shared and one exclusive resource.
- Commit: `feat: ADR-034 resource contract and declaration types` (0.2.0).
  **Done:** `d3b19a6`.

**Post-44-A: Opal Kelly read semantics and ReadOnly rule (D75–D77) 🔒 — 0.3.0**
- Owner decisions 2026-10-11 (F1–F4, Q-B, Q-C, Q-E, Q-G, Q-H, Q-I); spec
  change permission granted.
- Files: `api/openapi.yaml`, `proto/okfpga/v1/okfpga.proto`,
  `crates/ok-fpga-server/src/snapshot.rs` (new), `rest/classic.rs`,
  `grpc/classic_service.rs`, `main.rs`, ADR-034, the developer guide, the
  recommendations, the roadmap, `Cargo.toml` / `Cargo.lock`.
- Reads follow FrontPanel exactly (D76): `GetWireOutValue`, `IsTriggered`,
  `GetTriggerOutVector` read the shared snapshot; only Update calls touch
  the FPGA; optional `update` flag (REST `?update=true`, proto
  `optional bool update`) runs Update + read under the device lock.
  Whole-bank reads removed; `endpoint` required (422).
- Spec text: ReadOnly rule (D75), `private` side-effect guidance, D77
  coverage of omitted `endpoints`, ResetFPGA denied to ReadOnly.
- Tests (Tier 3): `test_read_snapshot_without_update_skips_refresh`,
  `test_read_snapshot_with_update_refreshes_before_read`,
  `test_read_snapshot_refresh_error_skips_read`,
  `test_get_wire_out_missing_endpoint_returns_422`,
  `test_get_trigger_out_missing_endpoint_returns_422`,
  `test_get_wire_out_endpoint_with_update_passes_validation`,
  `test_get_wire_out_endpoint_without_update_passes_validation`,
  `test_get_trigger_out_endpoint_with_update_passes_validation`,
  `test_get_trigger_out_endpoint_without_update_passes_validation`.
- Enforcement of F1 / F2, the D77 coverage change and the F4 warning come
  in 44-E.
- Commit: `feat: Opal Kelly snapshot reads, no whole-bank reads, ReadOnly
  rule D75-D77` (0.3.0). **Done:** `dcb5e5a` (+ `f414b8b` ADR hash).
- Follow-up D78 (0.3.1): IsTriggered `mask` defaults to 0 on REST and gRPC
  (was `0xFFFFFFFF` on REST / OpenAPI). Tests
  `test_trigger_out_query_mask_omitted_defaults_to_zero`,
  `test_trigger_out_query_mask_given_is_used`. Commit `fix: IsTriggered mask
  defaults to 0 (D78, 0.3.1)`. **Done:** `6e11f9b`.

---

## 44-B — Registration (R1) (0.3.2 → 0.3.3)

**Commits:** `2e2f7a3` (0.3.2: heartbeat ≤ 24 s text in OpenAPI / proto,
shipped `config.toml` with `endpoint_enforcement = "enforce"`; done
between 44-A and 44-B), `1d95a6f` (44-B, 0.3.3).

**Verification:** `cargo test` and `cargo test --features full` green
(core 240, server 157 with `full`), except the 3 `tests/mqtt_integration.rs`
tests that need a local Mosquitto broker. Strata drafted the core tests and
the docs, and reviewed the final diff (no bugs; 8 of 10 suggested test gaps
adopted, 2 rejected as non-observable / redundant).

**44-B Registration (R1)**
- Files: `api/openapi.yaml`, `proto/okfpga/v1/okfpga.proto`,
  `crates/ok-fpga-core/src/declaration.rs`, `resource.rs`,
  `crates/ok-fpga-server/src/rest/resources.rs`, `grpc/device_service.rs`,
  `grpc/resource_mapping.rs`, `config.rs`, `warnings.rs` (new), `main.rs`,
  `config.toml`, ADR-034 (§10), `docs/API-Specification/` (registration
  rules, errors, Appendix A), `Cargo.toml` / `Cargo.lock`.
- Rules, REST and gRPC through the same core functions
  (`declaration::build_resource`, `check_overlap`,
  `check_interfaces_for_dataport`, `self_overlapping_ranges`):
  - R1 (S-19): first registration wins; overlap allowed only for shared
    items in shared resources with the same EP type and the same
    item-level `private` → otherwise 409 naming resource and item (D22).
  - Rule A: an exclusive resource may contain only exclusive items (no
    shared entry / range, no `default_access: shared`) → 422.
  - Duplicate EP address in one declaration (even Pipe vs BTPipe) → 422.
  - Strict REST EP address `^0x[0-9A-Fa-f]{2}$` (`parse_ep_address`) → 422.
  - `axi_*` interfaces on a non-AXI data port → 422, checked **before** the
    "device programmed" 409 (SD-09).
  - Existing resources checked in `id` order (deterministic report).
  - No-`endpoints` wildcard still claims 0x00–0xBF (D77 narrowing → 44-E).
- Owner decisions (2026-10-11, 44-B review):
  - **Q1** Overlapping ranges within one declaration allowed; `warn!` when
    `access_control.warn_range_self_overlap` (default `true`, env
    `OKFPGA_WARN_RANGE_SELF_OVERLAP`); private wins on shared addresses
    (enforced in 44-E).
  - **Q2** Spec text permission (OpenAPI, proto, recommendations, ADR,
    Typst spec).
  - **Q3** Only item-level `private` is compared; resource-level `private`
    is not.
  - **Q4** `ResourceConflict.items` / `ResourceConflictDetail.items = 6`:
    every item clashing with the named resource (endpoints ascending, then
    its ranges in declaration order); `item` = first entry; detail text
    "(N conflicting items in total)".
  - **Q5** Registration vs unplug / reprogram → carried to 44-F (roadmap).
- Tests as implemented (Tier 3; names differ from the plan, which listed
  route-level names):
  - core `declaration.rs`: `test_build_resource_exclusive_with_*_rejected`,
    `test_build_resource_shared_with_exclusive_items_accepted`,
    `test_build_resource_duplicate_endpoint_address_rejected`,
    `test_build_resource_duplicate_address_pipe_vs_btpipe_rejected`,
    `test_parse_ep_address_strict_pattern`,
    `test_self_overlapping_ranges_*`, `test_check_interfaces_for_dataport_*`,
    `test_check_overlap_*` (shared/exclusive matrix, private and type
    mismatch, resource-level private not compared, wildcard cases incl. 192
    / 193 items, ranges incl. inclusive boundary and adjacency, multiple
    items, deterministic lowest id).
  - REST `rest/resources.rs`: `test_create_resource_axi_on_legacy_dataport_rejected`,
    `test_create_resource_unprogrammed_legacy_with_axi_rejected_422`,
    `test_create_resource_unprogrammed_classic_returns_409`,
    `test_create_resource_endpoint_address_not_two_hex_digits_rejected`,
    `test_create_resource_duplicate_endpoint_address_rejected`,
    `test_create_resource_exclusive_with_shared_item_rejected`,
    `test_create_resource_shared_overlap_allowed`,
    `test_create_resource_no_endpoints_blocks_later_classic`,
    `test_create_resource_self_overlapping_ranges_accepted`,
    `test_create_resource_overlap_items_include_endpoints_and_ranges`.
  - gRPC `grpc/device_service.rs`: `test_grpc_create_resource_*` for
    overlap detail with `items`, Rule A, AXI on Classic (programmed and
    unprogrammed), duplicate endpoint, self-overlap accepted, multi-item
    conflict.
  - config: `test_access_control_warn_range_self_overlap_omitted_defaults_true`.
  - Not done: Tier 1 `okREALM_TEST` registration-conflict test (the route
    tests use the in-memory registry; no FFI involved in registration).
- Commit: `feat: enforce resource registration overlap rules (R1) (0.3.3)`.
  **Done:** `1d95a6f`.

---

## 44-C — Acquisition (A1) (0.3.4)

**Owner decisions (2026-11-02):**
- **Q1 (Exclusive range lock check — `S-22` vs `S-14`):** Option A — keep
  44-C scoped to `S-22` (setting `locked_by = Some(user_id)` on `Exclusive`
  resource acquisition so exclusive holders pass `ExclusiveAddressRange`
  checks without calling `lock()`). Removing `locked_by` checks on
  shared-resource exclusive ranges (`S-14`) stays in 44-E.
- **Q2 (`PrivateResource` conflict code):** expand `ConflictCode` with
  `Private` (`"private"`) so `read_only` acquisition denied on a resource
  with `private: true` returns 409 / `FAILED_PRECONDITION` with
  `conflict = { code: "private", resource_id, resource_name }`. Updated
  `api/openapi.yaml` and `proto/okfpga/v1/okfpga.proto`.

**44-C Acquisition (A1)**
- Files: `api/openapi.yaml`, `proto/okfpga/v1/okfpga.proto`,
  `crates/ok-fpga-core/src/resource.rs`,
  `crates/ok-fpga-core/tests/device_resource_integration.rs`,
  `crates/ok-fpga-server/src/rest/resources.rs`,
  `crates/ok-fpga-server/src/grpc/device_service.rs`,
  `docs/API-Specification/` (+ rebuilt PDF).
- Implemented:
  - REST `acquire_resource` mode precedence (`S-03`):
    `requested.or(resource.access_mode).unwrap_or(AccessMode::Shared)`.
  - Repeat acquire by existing holder (`A1`, `D2`, `D7`, `D28`, `D43`):
    same mode returns `Ok(AcquireOutcome::AlreadyHeld)` (idempotent 200 /
    `OK`, no duplicate `HolderEntry`, no `ResourceChanged` event, touches
    `user_sessions`); different mode returns `HolderModeMismatch` → 409 /
    `FAILED_PRECONDITION` (`conflict.code = "mode_mismatch"`, "release
    first, then acquire").
  - Declared vs requested mode (`D62`): non-`read_only` request differing
    from `resource.access_mode` returns `DeclaredModeMismatch` → 409 /
    `FAILED_PRECONDITION` (`conflict.code = "mode_mismatch"`).
  - Private resource `read_only` denial (Q2): `PrivateResource` maps to
    `ConflictCode::Private` (`"private"`).
  - `ReadOnly` + `Exclusive` transitions (`D25a/b/c`): `Exclusive` acquire
    allowed when current holders of an `Exclusive` resource are all
    `ReadOnly`; state transitions to `InUseLocked` with
    `locked_by = Some(user_id)` (`S-22`); releasing the `Exclusive` holder
    while `ReadOnly` holders remain transitions back to `InUseShared`.
  - `check_endpoint_access` checks `is_readonly` write denial before the
    `InUseLocked` skip so `ReadOnly` holders on `Exclusive` resources still
    receive `ReadOnlyWriteAttempt`.
- Tests:
  - Core `resource.rs`: `test_acquire_same_mode_idempotent`,
    `test_acquire_different_mode_conflict`,
    `test_acquire_declared_mode_mismatch_conflict`,
    `test_acquire_readonly_private_denied`,
    `test_acquire_exclusive_with_readonly_holders_allowed`,
    `test_acquire_exclusive_holder_writes_without_lock_allowed`.
  - REST `rest/resources.rs`: `test_acquire_same_mode_idempotent`,
    `test_acquire_different_mode_conflict`,
    `test_acquire_omitted_mode_uses_declared_mode`,
    `test_acquire_readonly_private_denied`,
    `test_acquire_exclusive_with_readonly_holders_allowed`.
  - gRPC `grpc/device_service.rs`: `test_grpc_acquire_same_mode_idempotent`,
    `test_grpc_acquire_different_mode_failed_precondition`,
    `test_grpc_acquire_readonly_private_denied`,
    `test_grpc_acquire_exclusive_with_readonly_holders_allowed`.
- Commit: `feat: resource acquisition holder-mode rules (A1) (0.3.4)`.
  **Done:** `2167914`.

---

## 44-D — Locking and blocking (B1–B4) + events (0.3.5)

**Owner decisions (2026-11-02):**
- **Q1 (`ConflictCode::Blocked` fields):** populate only `code: "blocked"`,
  `resource_id: R.id`, and `resource_name: R.name` (no `item_type` / `item` /
  `items`).
- **Q2 (Repeat lock by current lock holder):** idempotent success
  (`Ok(LockOutcome::AlreadyLockedBySelf)`) — returns 200 / `OK` affirming the
  caller's lock, touches `user_sessions`, and suppresses duplicate
  `ResourceLocked` / `ResourceChanged` events.
- **Q3 (Implicit unlock events on release / disconnect / sweep):** whenever a
  user holding a shared-resource lock releases via `/release`,
  `ReleaseResource`, `/disconnect`, `Disconnect`, or session timeout sweep
  without calling `/unlock` first, broadcast `DeviceEvent::ResourceUnlocked`
  (and dashboard `resource_unlocked`) before `DeviceEvent::ResourceChanged`.

**44-D Locking and blocking (B1–B4) + events**
- Files: `crates/ok-fpga-core/src/resource.rs`,
  `crates/ok-fpga-core/tests/device_resource_integration.rs`,
  `crates/ok-fpga-server/src/rest/resources.rs`,
  `crates/ok-fpga-server/src/rest/server.rs`,
  `crates/ok-fpga-server/src/sweep.rs`,
  `crates/ok-fpga-server/src/grpc/resource_mapping.rs`,
  `crates/ok-fpga-server/src/grpc/device_service.rs`,
  `docs/API-Specification/`, `Cargo.toml` / `Cargo.lock`.
- Implemented:
  - `Resource::shares_item_with`, `find_blocking_resource`, and
    `ResourceRegistry::blocking_conflict_for` (`B1`, `B4`, `D35`): checks
    other resources on the same device in deterministic ascending `id` order
    that are locked by a different user (`V != U`, so own lock never blocks
    self, `D35`) and share at least one Classic endpoint address or
    overlapping `AddressRange` (`B4`: non-transitive).
  - Structured `LockOutcome` (`Locked`, `AlreadyLockedBySelf`) and
    `LockError` (`NotFound`, `PreconditionFailed`, `Conflict`) returned by
    `ResourceRegistry::lock` and `unlock` (`B3`, `D73`).
  - `ReleasedResource` (`release_with_info`, `release_all_for_user_with_info`)
    so REST/gRPC `release`, `disconnect`, and `sweep` broadcast
    `ResourceUnlocked` when a shared-resource lock ends implicitly (Q3), and
    gRPC `Disconnect` also broadcasts `UserDisconnected` (`reason: "explicit"`).
  - gRPC `WatchDeviceEvents` (`device_event_to_proto` + `ReceiverStream` over
    `device_manager.subscribe()`), streaming all 11 `DeviceEvent` variants.
- Tests:
  - Core `resource.rs`: `test_blocking_shared_item_blocks_other_resource`,
    `test_blocking_non_transitive`, `test_blocking_own_lock_not_blocked`,
    `test_lock_repeat_by_same_user_is_idempotent`.
  - Tier 1 `device_resource_integration.rs`:
    `test_two_users_two_overlapping_shared_resources_lock_blocking`.
  - REST `rest/resources.rs`: `test_lock_blocked_resource_conflict`,
    `test_unlock_notifies_blocked_holders`.
  - gRPC `grpc/device_service.rs`: `test_grpc_lock_events_emitted`,
    `test_grpc_watch_device_events_streams_resource_events`.
- Commit: `feat: resource lock blocking and lock events (B1-B4) (0.3.5)`.
  **Done:** `e562705`.

---

## 44-E — Enforcement pipeline (B2) (0.3.6)

**Owner decisions (2026-11-03):**
- **Q1 (`P14` proto comment & surfacing registration warnings):**
  - Permission granted to add the `P14` comment to `proto/okfpga/v1/okfpga.proto`
    explaining that AXI RPCs use `uint64 address` to match the FrontPanel C API,
    whereas `AddressRange` declarations are 32-bit (`uint32`), so addresses
    `> 0xFFFF_FFFF` cannot match any declared range under `enforce`.
  - Option B selected for surfacing registration warnings to callers: do not
    stuff warnings into an undocumented HTTP header; log them via
    `tracing::warn!` in 44-E and add a **Later** roadmap item to define an
    explicit `warnings: list[string]` field on `CreateResource` responses in
    `api/openapi.yaml` and `proto/okfpga/v1/okfpga.proto`.
- **Q2 (Omitted `endpoints: None` vs. Pipe-only resources & generic bridge policy):**
  - Narrow `WILDCARD_EP_LAST` to `0x7F` (`D77`): omitted `endpoints` claims
    `0x00..=0x7F` (Wire & Trigger endpoints only) and allows a second resource
    on the same device if that second resource declares only Pipe/BTPipe
    endpoints (`0x80..=0xBF`).
  - Log `tracing::warn!` messages when a Classic resource omits `endpoints`
    (explaining the `0x00–0x7F` assumption, Pipe/BTPipe availability, and
    `"endpoints": {"entries": []}` for zero endpoints, `D59`) and when an
    `endpoints: None` resource and a Pipe-declaring resource coexist on the
    same device.
  - Provide and document a generic, non-DualPort-specific
    `ClassicAddressBridgePolicy` reference implementation in
    `crates/ok-fpga-core/src/resource.rs`.
- **Q3a & Q3b (`endpoint_enforcement` default & unit test migration):**
  - Default `access_control.endpoint_enforcement` to `"enforce"` (`S-12`) and
    reject missing identity in `resolve_user_id` (`S-18`); allow anonymous
    `/configure` and `/configure_from_server_path` only when
    `endpoint_enforcement == "off"`.
  - Add a **Later** roadmap item to migrate older Tier 3 route unit tests from
    `EndpointEnforcement::Off` to `EndpointEnforcement::Enforce`.
- **Q4 (Full address span `[start, start + length - 1]` checking):**
  - Check the full byte span `[start, start + byte_len - 1]` against a single
    declared `AddressRange` on the candidate resource (`byte_len = 1` for
    Classic `okRegisterBridge` and `MapsToAddress`, `byte_len = 4` for AXI Lite
    and 4-byte REST AXI Full, `byte_len = length` for gRPC AXI Full / Bulk).
- **Q5 (Tier 2 Hardware Testing on the hardware test host):**
  - Add `test_hardware_xem7320_counters_enforcement_pipeline` (`#[ignore]`) in
    `crates/ok-fpga-core/tests/device_resource_integration.rs` targeting the
    connected `XEM7320-A75T` boards with `Counters.bit`.

**44-E Enforcement pipeline (B2)**
- Files: `proto/okfpga/v1/okfpga.proto`,
  `crates/ok-fpga-core/src/declaration.rs`,
  `crates/ok-fpga-core/src/resource.rs`,
  `crates/ok-fpga-core/tests/device_resource_integration.rs`,
  `crates/ok-fpga-server/src/config.rs`,
  `crates/ok-fpga-server/src/warnings.rs`,
  `crates/ok-fpga-server/src/rest/mod.rs`,
  `crates/ok-fpga-server/src/rest/devices.rs`,
  `crates/ok-fpga-server/src/rest/resources.rs`,
  `crates/ok-fpga-server/src/rest/classic.rs`,
  `crates/ok-fpga-server/src/rest/axi.rs`,
  `crates/ok-fpga-server/src/grpc/device_service.rs`,
  `crates/ok-fpga-server/src/grpc/classic_service.rs`,
  `crates/ok-fpga-server/src/grpc/axi_service.rs`,
  `config.toml`, `docs/API-Specification/`, `Cargo.toml` / `Cargo.lock`.
- Tests:
  - Core `declaration.rs`: `test_check_overlap_wildcard_allows_pipe_only_resource`,
    `test_check_overlap_wildcard_blocks_trigger_denied`.
  - Core `resource.rs`: 14 Phase 44-E unit tests covering write candidate
    filtering, foreign lock, blocking (`B2`), read during foreign lock,
    private endpoint/range reads, same-resource address checking (`S-20`),
    self-overlap `private` tie-breaking, `AllowAlways` with blocking (`D51`),
    `ReadOnly` mapped-address denial (`D29`), `ResetFPGA` (`D56`, `D66`,
    `D75 F1`), `UpdateTriggerOuts` private TriggerOut denial (`D75 F2`),
    full-span AXI & 64-bit address checking (`Q4`, `P14`), and
    `ClassicAddressBridgePolicy` (`Q2`).
  - Core `device_resource_integration.rs`:
    `test_two_users_classic_bridge_enforcement_and_blocking`,
    `test_hardware_xem7320_counters_enforcement_pipeline` (`#[ignore]`).
  - Server `config.rs`, `warnings.rs`, `rest/mod.rs`, `rest/classic.rs`:
    `test_access_control_default_endpoint_enforcement_is_enforce`,
    `test_access_control_warn_readonly_side_effects_omitted_defaults_true`,
    `test_readonly_side_effects_messages_*`,
    `test_wildcard_endpoint_messages_warns_on_omitted_endpoints_and_pipe_coexistence`,
    `test_resolve_user_id_missing_identity_rejected`,
    `test_activate_trigger_in_invalid_bit_returns_422`,
    `test_reset_fpga_readonly_holder_returns_403`,
    `test_reset_fpga_shared_holder_passes_enforcement`,
    `test_axi_reset_readonly_holder_returns_403`,
    `test_update_trigger_outs_readonly_denied_when_device_has_private_trigger_out`,
    `test_update_trigger_outs_readonly_allowed_when_no_private_trigger_out`,
    `test_block_pipe_routes_enforce_endpoint_access`,
    `test_axi_lite_read_write_enforce_address_access`.
- Commit: `feat: align endpoint enforcement with overlap and blocking (B2) (0.3.6)`.
  **Done:** `7c39e74`.

---

## 44-F — Reprogram and clear cleanup (D26) (0.3.7)

**Owner decisions (2026-11-03):**
- **Q1 (Spec & proto permission — `Q44-14`, `P11`, `P12`):** permission granted
  to edit `api/openapi.yaml` and `proto/okfpga/v1/okfpga.proto`.
- **Q2 (`DeleteReason` on device unplug / orphan sweep):** Option A — add
  `"device_removed"` (`DELETE_REASON_DEVICE_REMOVED = 3` in proto,
  `DeleteReason::DeviceRemoved` in Rust) to `DeleteReason` in OpenAPI, proto,
  `ok-fpga-core`, and `resource-model.typ`.
- **Q3a & Q3b (Forced cleanup ordering, FFI failure state, and Tier 3 fallback):**
  - **Q3a:** After validating request inputs (device exists, caller identity
    valid, bitfile data/path exists), transition the device to
    `DeviceState::Connected` (unprogrammed, bumping `config_generation`) and
    run `D26` forced cleanup (`ResourceUnlocked` if locked → release holders →
    `ResourceDeleted` with `reprogram` / `clear`) **before** the FFI hardware
    call, so that if the FFI transfer fails mid-transfer, the device remains in
    `Connected` (unprogrammed) state with no stale resources or data ports.
  - **Q3b:** When `state.handle_pool` is `None` (Tier 3 tests without
    `libokFrontPanel`), `configure_fpga*` and `clear_fpga_configuration` /
    `ClearFPGAConfiguration` still perform the full `D26` forced cleanup,
    update `DeviceState` (`Programmed` / `Connected`), and broadcast events in
    memory.
- **Q4 (Optional bitfile-matching regex / hash on resource declarations from 44-B Q5):**
  - **Rejected and dropped (NOT moved to Later).** A resource can never be
    reliably validated against a bitfile hash because bitfile hashes change
    even when rebuilt with zero code changes (timestamps/seeds), and filenames
    are not authoritative. Instead, the monotonic per-device
    `config_generation` counter (checked atomically during `create_resource`)
    plus `D26` forced cleanup on every `ConfigureFPGA*` and
    `ClearFPGAConfiguration` guarantees resources only live for the
    configuration session in which they were registered.
- **Q5 (`FrontPanel-6.1.0/libokFrontPanel.so` & proprietary SDK files in git):**
  - Added `FrontPanel-6.1.0/*.so`, `*.so.*`, and `*.dylib` to `.gitignore`, and
    added a **🔵 Later** roadmap task to remove tracked proprietary Opal Kelly
    FrontPanel IP/SDK files from git and document external SDK installation.

**44-F Reprogram and clear cleanup (D26)**
- Files: `.gitignore`, `api/openapi.yaml`, `proto/okfpga/v1/okfpga.proto`,
  `crates/ok-fpga-core/src/device.rs`, `crates/ok-fpga-core/src/manager.rs`,
  `crates/ok-fpga-core/src/resource.rs`,
  `crates/ok-fpga-core/tests/device_resource_integration.rs`,
  `crates/ok-fpga-server/src/state.rs`,
  `crates/ok-fpga-server/src/handle_pool.rs`,
  `crates/ok-fpga-server/src/rest/devices.rs`,
  `crates/ok-fpga-server/src/rest/resources.rs`,
  `crates/ok-fpga-server/src/grpc/device_service.rs`,
  `crates/ok-fpga-server/src/grpc/resource_mapping.rs`,
  `crates/ok-fpga-server/src/sweep.rs`,
  `crates/ok-fpga-server/src/main.rs`,
  `docs/API-Specification/`, `Cargo.toml` / `Cargo.lock`.
- Implemented:
  - `FpgaDevice::config_generation` (`u64`) bumped on every
    `DeviceRegistry::update_state` call.
  - `ResourceRegistry::try_register_with_precondition` verifying inside the
    resource-registry write lock that the target device still exists, is
    programmed, and matches `expected_gen` (`RegisterError::PreconditionFailed`
    → 409 / `FAILED_PRECONDITION`).
  - `ResourceRegistry::force_delete_for_device`, `force_delete_resource`, and
    `ForceDeletedResource` returning unlocked-user and released-holders info.
  - `AppState::force_cleanup_device_resources` and
    `broadcast_force_deleted_resources` emitting `ResourceUnlocked` (if a
    shared resource was locked) before `ResourceDeleted { reason }` across SSE,
    dashboard WebSocket, and `WatchDeviceEvents`.
  - REST `DELETE /devices/{serial}/configure` (`clear_fpga_configuration`),
    `POST /devices/{serial}/configure`,
    `POST /devices/{serial}/configure_from_server_path`, and gRPC
    `ConfigureFPGA`, `ConfigureFPGAFromMemory`, and `ClearFPGAConfiguration`
    wired with `D26` forced cleanup before FFI (`Q3a`), Tier 3 fallback
    (`Q3b`), `OpenDevice::clear_data_ports()`, and `DeviceHandlePool::clear_configuration`.
  - `sweep_orphan_resources` in `sweep.rs` and hot-unplug in `main.rs` emitting
    `DeleteReason::DeviceRemoved` (`"device_removed"`).
- Tests:
  - Core `manager.rs`: `test_delete_reason_serializes_snake_case` (incl. `device_removed`).
  - Core `device_resource_integration.rs`:
    `test_emulated_reprogram_and_clear_force_delete_resources` (Tier 1),
    `test_hardware_xem7320_counters_enforcement_pipeline` step 5 (Tier 2 live
    `XEM7320-A75T` hardware).
  - Server `rest/devices.rs`: `test_configure_fpga_held_resources_force_deleted`,
    `test_clear_fpga_notifies_all_holders`,
    `test_reset_fpga_resources_retained`.
  - Server `rest/resources.rs`:
    `test_create_resource_configure_generation_changed_conflict`.
  - Server `grpc/device_service.rs`:
    `test_grpc_configure_fpga_force_deletes_resources`,
    `test_grpc_clear_fpga_notifies_all_holders`.
  - Server `sweep.rs`: `test_sweep_orphan_resource_device_absent_deleted`.
- Commit: `fix: forced resource cleanup on configure and clear (0.3.7)`.
  **Done:** `5a1f025`.

---

## 44-G — Classic data-path spec alignment & example clients (0.3.8)

**44-G Classic data-path spec alignment (`S-05`, `S-06`, `S-07`) & example clients**
- Files:
  - Server: `crates/ok-fpga-server/src/rest/classic.rs`,
    `crates/ok-fpga-server/src/grpc/classic_service.rs`,
    `crates/ok-fpga-core/tests/device_resource_integration.rs`,
    `Cargo.toml` / `Cargo.lock`.
  - Clients: `client/python-labview-client/` (`0.2.0` → `0.2.1`),
    `client-examples/rust-rest-client/`,
    `client-examples/python-rest-client/` (`0.1.0` → `0.1.1`),
    `client-examples/rust-grpc-client/`,
    `client-examples/python-grpc-client/` (`0.1.0` → `0.1.1`, regenerated protobuf stubs),
    `client-examples/rust-rest-gui-common/`,
    `client-examples/rust-grpc-gui-common/`,
    `client-examples/python_rest_gui_common/` (`0.1.0` → `0.1.1`),
    `client-examples/python_grpc_gui_common/` (`0.1.0` → `0.1.1`, regenerated protobuf stubs).
  - Docs: `docs/API-Specification/`, `docs/architecture/adr/ADR-034-resource-model-consolidation.md`,
    `docs/architecture/overview.md`, `docs/development-roadmap.md`.
- Implemented:
  - **S-05 (`application/octet-stream` Pipes & Block Pipes):**
    - Changed `POST /devices/{serial}/classic/pipes/out` and
      `POST /devices/{serial}/classic/pipes/block-out` to `GET` per
      `api/openapi.yaml`.
    - Implemented `PipeInQuery`, `PipeOutQuery`, `BlockPipeInQuery`, and
      `BlockPipeOutQuery` with RFC 7807 `422` validation when `endpoint`,
      `length`, or `block_size` is missing, `length < 0`, or `block_size <= 0`.
    - Accepted `axum::body::Bytes` (`application/octet-stream`) on
      `write_to_pipe_in` and `write_to_block_pipe_in`, and returned raw
      `application/octet-stream` bytes on `read_from_pipe_out` and
      `read_from_block_pipe_out`.
    - Added matching `length >= 0` and `block_size > 0` validation in gRPC
      `ClassicDataPortServiceImpl`.
  - **S-06 (Bulk Registers):**
    - Replaced single-register REST structs with `RegisterReadRequest { addresses: Vec<u32> }`,
      `RegisterEntry { address: u32, data: u32 }`,
      `RegisterReadResponse { entries: Vec<RegisterEntry> }`, and
      `RegisterWriteRequest { entries: Vec<RegisterEntry> }`.
    - Enforced `enforce_address` across every address in the batch before
      acquiring the device lock or touching hardware (matching gRPC
      `read_registers` / `write_registers`).
  - **S-07 (`WireInRequest.auto_update`):**
    - Added `#[serde(default = "default_auto_update")] auto_update: bool`
      (default `true`) to `WireInRequest`; `set_wire_in` only calls
      `classic_update_wire_ins` when `req.auto_update` is `true`.
  - **ADR-034 Client Alignment:**
    - Updated `client/python-labview-client/` (`Holder`, `Resource`, optional
      `acquire_resource` `mode`, `read_registers` / `write_registers`,
      `write_pipe_in` / `read_pipe_out` `application/octet-stream`,
      `set_wire_in(auto_update=...)`) and all 8 `client-examples/` packages
      (Rust/Python REST, gRPC, and GUI common libraries) to the ADR-034
      resource contract (`CreateResourceRequest`, `Holder` objects,
      `ResourceState` strings, single-endpoint `WireOut`/`TriggerOut` reads,
      and regenerated Python protobuf stubs).
- Tests:
  - Server `rest/classic.rs`: `test_pipe_in_octet_stream_and_missing_endpoint_422`,
    `test_pipe_out_get_octet_stream_and_negative_length_422`,
    `test_block_pipe_in_out_octet_stream_and_validation_422`,
    `test_register_bulk_read_and_write_enforcement_atomicity`,
    `test_wire_in_auto_update_default_true_and_explicit_false`,
    `test_block_pipe_routes_enforce_endpoint_access`.
  - Server `grpc/classic_service.rs`: `test_grpc_classic_pipe_register_parity`.
  - Core `device_resource_integration.rs`:
    `test_emulated_pipe_and_register_round_trip_with_ok_realm_test` (Tier 1),
    `test_hardware_xem7320_wire_in_auto_update_semantics` (Tier 2 live
    `XEM7320-A75T` hardware on the hardware test host).
  - Clients: `rust-rest-client`, `rust-grpc-client`, `rust-rest-gui-common`,
    `rust-grpc-gui-common` (`cargo test`), `python-labview-client` (149 passed),
    `python_rest_gui_common` (36 passed), `python_grpc_gui_common` (15 passed).
- Commits:
  - `fix: align classic pipe, register, and wire_in routes with OpenAPI (0.3.8)` — **Done:** `d59921e`.
  - `fix: update example clients to the ADR-034 resource contract` — **Done:** `96c0ed7`.

---

## 44-H — Dataport normalisation 🔒 (0.3.9)

**44-H Dataport normalisation (`D63`, `D70`, `S-10`, `SD-05`, `P9`, `P16`)**
- Files:
  - Spec: `api/openapi.yaml`, `proto/okfpga/v1/okfpga.proto`.
  - Core: `crates/ok-fpga-core/src/device.rs`, `crates/ok-fpga-core/src/manager.rs`,
    `crates/ok-fpga-core/src/declaration.rs`.
  - Server: `crates/ok-fpga-server/src/scanner.rs`,
    `crates/ok-fpga-server/src/device_monitor.rs`,
    `crates/ok-fpga-server/src/rest/devices.rs`,
    `crates/ok-fpga-server/src/grpc/device_service.rs`,
    `Cargo.toml` / `Cargo.lock`.
  - Clients: `client/python-labview-client/` (`0.2.1` → `0.2.2`),
    `client-examples/python-rest-client/` (`0.1.1` → `0.1.2`),
    `client-examples/python-grpc-client/` (`0.1.1` → `0.1.2`, regenerated protobuf stubs),
    `client-examples/python_grpc_gui_common/` (`0.1.1` → `0.1.2`, regenerated protobuf stubs).
  - Docs: `docs/API-Specification/`, `docs/architecture/adr/ADR-034-resource-model-consolidation.md`,
    `docs/architecture/overview.md`, `docs/development-roadmap.md`.
- Implemented:
  - **S-10 & SD-05 (OpenAPI):** Normalised `GET /devices/{serial}/dataport`
    and `DeviceInfo.properties.dataport` to `enum: [legacy, axi]` with `SD-05`
    descriptions clarifying `OK_DATAPORT_LEGACY` vs `OK_DATAPORT_AXI` and that
    Classic endpoints exist on all supported devices.
  - **P9 & P16 (Protobuf):** Updated `enum DataportType` to
    `DATAPORT_LEGACY = 0; DATAPORT_AXI = 1;` (`P16`), updated `GetDataportType`
    comment, and updated `AXIDataPortService` service comment (`P9`) to state
    that direct AXI runs via `FPGADataPortAXI` (`OK_DATAPORT_AXI` devices only).
  - **D70 (`OK_DATAPORT_UNKNOWN` Filtering):** Added `DataportType::as_str()`
    and `DataportType::is_supported()`; filtered `DataportType::Unknown` during
    FFI enumeration (`scanner.rs`, `device_monitor.rs`) and in
    `DeviceRegistry` (`get`, `list`, `count`, `contains`, `on_device_added`).
  - **REST & gRPC Parity:** Fixed `DataportType::Display` and
    `GET /devices/{serial}/dataport` to return `"legacy"` / `"axi"`; added
    `DeviceServiceImpl::dataport_to_proto` and implemented gRPC
    `GetDataportType`.
- Tests:
  - Core `device.rs`: `test_dataport_legacy_maps_to_legacy`,
    `test_dataport_axi_maps_to_axi`, `test_dataport_unknown_is_unsupported`.
  - Core `manager.rs`: `test_device_registry_unknown_dataport_omitted_and_not_found`.
  - Server `rest/devices.rs`: `test_dataport_legacy_maps_to_legacy`,
    `test_dataport_axi_maps_to_axi`, `test_list_devices_unknown_dataport_omitted`,
    `test_get_device_unknown_dataport_not_found`.
  - Server `grpc/device_service.rs`: `test_grpc_dataport_legacy_and_axi_mapping`,
    `test_grpc_get_device_unknown_dataport_not_found`.
- Commit: `fix: normalise dataport enum to legacy and axi (0.3.9)` — **Done:** `f1961c6`.

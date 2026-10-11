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

**Commits:** `c5e4f79` (44-A, 0.2.0), `9389a32` (ADR hash), `2433a68`
(MQTT follow-up item), `5de5ab6` (post-44-A read semantics D75–D77,
0.3.0), `cc04972` (ADR hash), `d88e5c5` (IsTriggered mask default D78,
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
  **Done:** `c5e4f79`.

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
  rule D75-D77` (0.3.0). **Done:** `5de5ab6` (+ `cc04972` ADR hash).
- Follow-up D78 (0.3.1): IsTriggered `mask` defaults to 0 on REST and gRPC
  (was `0xFFFFFFFF` on REST / OpenAPI). Tests
  `test_trigger_out_query_mask_omitted_defaults_to_zero`,
  `test_trigger_out_query_mask_given_is_used`. Commit `fix: IsTriggered mask
  defaults to 0 (D78, 0.3.1)`. **Done:** `d88e5c5`.

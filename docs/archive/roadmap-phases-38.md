# Archive — Phase 38: User Identification Consistency

> **Archived:** 2026-08-23
> **Commit:** `8507b4a`
> **ADR:** [ADR-029 — User Identification Consistency](../architecture/adr/ADR-029-user-identification-consistency.md)

---

## Phase 38 — User Identification Consistency (`user_id` vs `X-User-Id`)

**Goal:** Cross-cutting resolution of `user_id` across all 8 REST endpoints that
accept it in their request bodies, aligning the server so that identity is
read from body first, `X-User-Id` header as fallback, with mismatch rejection.

**Deliverables:**

- Shared `resolve_user_id()` function in `rest/mod.rs` — body → header → "Unknown",
  returns 400 if both are present but differ.
- Updated 6 handlers (acquire, release, lock, unlock, heartbeat, disconnect) with
  `Option<String>` user_id and `HeaderMap` extraction.
- Removed local `resolve_user_id()` from `devices.rs`, now imports shared version.
- OpenAPI spec updated with fallback/mismatch documentation on all 8 endpoints.
- Python REST client updated with `_post_with_user()` helper and `disconnect()` method.
- 7 new unit tests for `resolve_user_id()` (body-only, header-only, match, mismatch,
  neither, empty-body, empty-header).
- All 52 server unit tests + 164 core tests pass.
- Hardware verified on XEM7320 (heartbeat match/fallback/mismatch, disconnect).

**Files changed:**
- `crates/ok-fpga-server/src/rest/mod.rs` — shared `resolve_user_id()` + 7 tests
- `crates/ok-fpga-server/src/rest/devices.rs` — removed local, imports shared
- `crates/ok-fpga-server/src/rest/resources.rs` — 4 handlers updated
- `crates/ok-fpga-server/src/rest/server.rs` — 2 handlers updated
- `api/openapi.yaml` — 8 endpoint descriptions updated
- `client-examples/python-rest-client/okfpga_rest_client/client.py` — `X-User-Id` header + `disconnect()`
- `docs/architecture/adr/ADR-029-user-identification-consistency.md` — created
- `docs/architecture/overview.md` — ADR-029 added to index
- `docs/development-roadmap.md` — Phase 38 references, TLS item expanded

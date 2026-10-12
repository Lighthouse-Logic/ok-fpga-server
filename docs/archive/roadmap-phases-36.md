# Archived Roadmap Phases 36

> Archived from `docs/development-roadmap.md` on 2026-08-23.

---

## Phase 36 — Configure FPGA API Alignment ✅

**Goal:** Split the configure endpoint into two REST paths, implement proper multipart
parsing, and align the server response with the OpenAPI spec.

**ADR:** [ADR-028](../architecture/adr/ADR-028-configure-fpga-api-alignment.md)

**Deliverables:**

| # | Task | Notes |
|---|------|-------|
| 1 | Add `POST /devices/{serial}/configure_from_server_path` route | JSON body `{"bitfile_path": "..."}`, calls `ConfigureFPGA` from disk |
| 2 | Refactor `/devices/{serial}/configure` handler | Replace raw-bytes hack with proper `axum::extract::Multipart` parsing |
| 3 | Extract `filename`, `description` from multipart form fields | Currently `description` is hard-coded to `None` |
| 4 | Fix response to return `BitfileInfo` | Replace `ConfigureResponse {status, serial, message}` |
| 5 | Update server doc comment on `configure_fpga` handler | Fix `"path"` → `"bitfile_path"` and describe both endpoints |
| 6 | Update Rust GUI clients | `configure_fpga()` → `/configure_from_server_path`; `configure_fpga_from_file()` → true multipart |
| 7 | Update integration tests | Verify both endpoints work against running server |

**Exit Criteria:**
- Both endpoints return `BitfileInfo` per OpenAPI spec ✅
- Server parses true `multipart/form-data` (no raw-bytes hack) ✅
- Rust GUI clients work with both modes (ServerPath / LocalFile) ✅
- `cargo test` passes, `cargo build` succeeds ✅

**Commits:** `fc0d77d` (server + Rust GUI client), `2959752` (ADR-028 status), `c0709a7` (all REST client addendum)

**Addendum:** All REST client examples updated (Python common, Python
standalone, Rust standalone) to use `/configure_from_server_path` for
server-side path mode and `multipart/form-data` for client upload mode.

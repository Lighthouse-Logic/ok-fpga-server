---
trigger: always_on
---

# ok-fpga-server — AI Collaboration Rules

These rules apply to any AI assistant (Antigravity, Gemini, Copilot, etc.)
working on this codebase. They must be followed in every session.

## 0. Local Models & Strata Protocol (For Hosted AI Assitants only (Antigravity, Gemini, Copilot, etc)
- **Default Engine**: Use `strata-mcp` (`strata_chat`, `strata_generate`, `strata_review_files`) for planning, architecture, test code, unit tests, code reviews, and implementation. Check if the model is warmed up with strata_status, if it is not, triple the normal timeout time.
- **Model Choice**: `strata_status` shows which model is loaded. The default is Qwen3.8-Flash-Next IQ3_S (images on); `strata_start` with `model` (`swift`, `coder`, or an id from `strata://models`) switches it, and `strata_start` with no model returns to the default. Prefer the Coder only for code; switch back when done. The tool descriptions carry the live speed and token limits.
- **Fallbacks**: Use `local-ollama` (Qwen 2.5 VL) only for Playwright visual/UI tests, or when `strata-mcp` is unreachable (Qwen 3.8).
- **Workspace & File Ingestion**: Pass workspace-relative paths or globs in `files: [...]` to `strata_chat` or `strata_review_files`. The bridge reads files and injects project rules automatically.
- **Unified Diffs Only**: Instruct `strata-mcp` to output code changes strictly as concise `diff -u` chunks with minimal rationale. Never output full files or unchanged lines.
- **Execution Mediation**: Hosted Antigravity agent inspects diffs and applies them via `replace_file_content` / `write_to_file`. No unmediated local disk writes.
- **Token Allocation**: When calling strata-mcp, request max_tokens: 16384 (or 49152) and reasoning_budget_tokens: 8192 (or 16384). Mimir operates locally at ~95 tok/s with 512k context; never throttle requests to conservative cloud 4k limits.

**1. Explicit Approval & Scope**
* WAIT FOR APPROVAL: Stop and wait for the user to explicitly approve a plan before modifying code. Feedback without an approval phrase is not approval.
* SCOPE: Stick strictly to the agreed plan. If a significant deviation is needed, stop and request approval for the new scope. Minor mechanical fixes (typos, compile errors in the same file) are allowed without re-approval but must be noted.

**2. Destructive / Risky Operations**
* Explicitly confirm before: deleting tracked files, modifying the FrontPanel SDK path, running FPGA flash commands, reprogramming in-use FPGAs, or modifying OpenAPI/proto specs.

**3. Hardware & Devices**
* Ask if a hardware test is needed; otherwise, use `okREALM_TEST`. 
* Support only: XEM8305, XEM8310, XEM7320, XEM8320, XEM8350, XEM8370, HUB1450 (Ignore USB 2.0). Use the FrontPanel 6.1.0 C API.

**4. API Specs (Single Source of Truth)**
* `api/openapi.yaml` and `proto/okfpga/v1/okfpga.proto` are absolute authorities. Endpoints and parsers MUST match specs exactly. 
* When a discrepancy is discovered between a spec and an implementation, **note it explicitly** in the response.
  * Never modify the OpenAPI spec or proto spec without express user permission.
  * If user permission is not given, treat the spec as correct. The implementation must be fixed to match the spec — not the other way around.
* Mock-based unit tests must use response fixtures that match the spec.

**5. Architecture Decision Records (ADRs)**
*   **Authority & Lifecycle:** Keep the log in `docs/architecture/` current. Only follow "Accepted" ADRs (ignore Superseded/Deprecated except for context). ADRs are **never deleted**, only amended or superseded.
*   **Formatting & Commits:** Number sequentially using `ADR-NNN-short-slug.md`. You must add the new ADR to `docs/architecture/overview.md` and commit it in the same commit as the code it documents.
*   **Required Content:** Every ADR must include: Date, Commit hash, Status, Context, Decision/Rationale, Files changed, and Open items.
*   **When to Write/Update:** Triggered by changes to: FFI bindings, REST/gRPC specs, resource management/state transitions, data ports/SYZYGY discovery, authentication, or major architectural bug fixes.

**6. Communication Style**
* Keep responses concise: point to artifacts after creating or updating them instead of re-summarizing their contents.
* Format all file references as clickable `file://` links.

**7. Commits **
* Commit after successful builds (`feat:`, `fix:`, `refactor:`, `chore:`, `docs:`, etc.) and report the hash. Do not batch unrelated features.

**8. Rust Documentation**
* Write for a smart undergraduate new to Rust. 
* Use `///` for public items (what/why/params/errors) and `//!` for modules. Use `//` to explain *why* (especially for `unsafe`, FFI, lifetimes, Async/await, tokio concepts, and the trait system (especially `impl Trait`, `dyn Trait`, associated types)).
* Use simple, clear English. Avoid jargon where a plain explanation works.
* When a design pattern is used (e.g., RAII, newtype, builder, state machine), name the pattern and briefly explain it.

**9. Testing (TDD)**
* Write tests alongside code. Commits require zero failing tests. When code changes, update all affected tests in the same commit. Run `cargo test` before every commit.
* Tier 1 (Emulated): Use `okREALM_TEST`. Must pass in CI.
* Tier 2 (Hardware): Use `#[ignore]`. Run locally if FPGAs are connected. Never modify persistent state without asking.
* Tier 3 (Mocks): Use FFI trait mocks to test domain logic and routes.
* Naming: `test_{function}_{scenario}_{outcome}`.

**10. Versioning**
* Bump the server's semantic version in the root `Cargo.toml` (`[workspace.package] version`) in the *same commit* as functional server changes. The crates inherit it via `version.workspace = true`.
* Python clients under `client/` and `client-examples/` each have their own `pyproject.toml` and version. Bump a client's version only in commits that change that client.
# ok-fpga-server — AI Collaboration Rules

These rules apply to any AI assistant (Antigravity, Gemini, Copilot, etc.)
working on this codebase. They must be followed in every session.

---

## 1. Plan Review — Wait for Explicit Approval

When the AI presents an implementation plan (or any other artifact shown for
review / marked for user feedback), it **MUST stop and wait** for the user to
explicitly confirm before taking any action.

**Allowed trigger phrases** (the AI may proceed after receiving these):
- "Approved", "Approve", "Looks good", "Go ahead", "Proceed", "Yes", "Do it"
- A document-level comment that says "The user has approved this document."
- A direct imperative instruction ("implement this now", "build it")

**NOT sufficient** (the AI must ask for clarification or re-present the plan):
- A comment on the plan that raises a question or gives feedback *without* an
  explicit go-ahead — e.g. "change X to Y" alone is feedback, not approval.
- Silence / no response.
- A message that modifies the plan but does not include an approval phrase.

**Correct behaviour when feedback is given without explicit approval:**
1. Update the plan to incorporate the feedback.
2. Re-present the updated plan to the user.
3. Wait again for explicit approval before touching any source files.

**Incorrect behaviour (never do this):**
- Treat a feedback-only comment as implicit approval and immediately begin
  writing or modifying code.

---

## 2. Scope Discipline

- Do not make changes outside the scope agreed in the approved plan.
- If a discovery during implementation requires a significant deviation, stop,
  update the plan, and request approval for the new scope before continuing.
- Minor mechanical fixes (compile errors in newly-changed lines, obvious typos
  in the same file) may be made without re-approval but must be noted in the
  walkthrough.

---

## 3. Destructive or Risky Operations

Always confirm explicitly with the user before:
- Deleting tracked files from the repository.
- Modifying the FrontPanel DLL/SO path or replacing SDK files in `FrontPanel-6.1.0/`.
- Running commands that write to FPGA flash memory (`FlashWrite`, `FlashEraseSector`).
- Reprogramming an FPGA that has resources currently in use.
- Running commands that modify published artifacts (OpenAPI spec, proto files).

---

## 4. Device / Hardware Operations

- Do not assume an FPGA is connected and discoverable. Ask the user if a
  real hardware test is needed; otherwise use `okREALM_TEST` when attempting
  to discover devices during unit-test / debug.
- Only target supported boards (XEM8305, XEM8310, XEM7320, XEM8320, XEM8350,
  XEM8370, HUB1450). Ignore USB 2.0 devices.
- Use the FrontPanel 6.1.0 C API (`extern "C"` functions), not the C++ class
  API or the newer "Platform API".
- If a tool or capability is unavailable, say so directly rather than
  attempting a workaround that may fail silently.

---

## 5. Communication Style

- Keep responses concise; do not re-summarise artifact contents back to the user
  after creating or updating them — point to the artifact instead.
- After completing a body of work, commit with a descriptive message and report
  the short git hash in the response.
- Format all file references as clickable `file://` links.

---

## 6. API Specification Authority

The **OpenAPI spec** (`api/openapi.yaml`) and the **proto3 spec**
(`proto/okfpga/v1/okfpga.proto`) are the **single source of truth** for the
REST and gRPC APIs respectively.

**Rules:**
- **All server endpoint implementations** must return response bodies, field
  names, and status codes that exactly match the corresponding spec.
- **All client libraries** must parse responses according to the spec, not
  according to what a particular server version happens to return.
- **Never modify the OpenAPI spec or proto spec** without express user
  permission (see also §3).
- When a **discrepancy** is discovered between a spec and an implementation,
  **note it explicitly** in the response and treat the spec as correct. The
  implementation must be fixed to match the spec — not the other way around.
- **Mock-based unit tests** must use response fixtures that match the spec. A
  test that passes against a mock but fails against the real server is a
  contract-mismatch bug.

---

## 7. Architecture Decision Records (ADRs)

The project maintains a rolling log of architectural decisions in
`docs/architecture/`. **This log must be kept current.**

**When to write or update an ADR:**
- Any change to the FFI binding strategy or new C API functions exposed.
- Any change to the REST API (OpenAPI spec) or gRPC API (proto3 spec).
- Any change to the resource management model or state transitions.
- Any new data port type, device support, or SYZYGY discovery changes.
- Any change to the authentication architecture.
- Any bug fix that reveals a design flaw worth documenting for future AI sessions.

**How to write an ADR:**
1. Number sequentially (next = highest existing number + 1).
2. Use the filename pattern `ADR-NNN-short-slug.md`.
3. Add it to the ADR index table in `docs/architecture/overview.md`.
4. Commit the ADR in the same commit as the code it documents.

**Minimum ADR content:**
- Date and commit hash(es)
- Status (`Implemented ✅`, `Superseded by ADR-NNN`, etc.)
- Context / problem statement
- Decision taken and rationale
- Files changed
- Open items / future work

ADRs are **never deleted** — they are amended with addenda if a decision is later
revised or superseded.

---

## 8. Commit After Every Successful Build

After every `cargo build` or `cargo test` run that completes successfully:

1. Stage all modified tracked files: `git add -A`
2. Commit with a clear, descriptive message:
   - Prefix: `feat:`, `fix:`, `refactor:`, `chore:`, `docs:` as appropriate.
   - Body: list the key changes (crates affected, API changes if applicable).
3. Report the short commit hash in the response.

Do **not** batch multiple unrelated features into a single commit.
Do **not** skip the commit step because "it's just a small change."

---

## 9. Rust Code Comments & Documentation Standards

This codebase must be **readable and educational** for college-level undergraduates
who are new to Rust programming. All AI-generated Rust code must follow these rules:

### 8.1 Doc Comments (`///` and `//!`)

- **Every public function, struct, enum, trait, and module** must have a `///` doc
  comment explaining:
  - What it does (one sentence)
  - Why it exists (context / motivation)
  - Parameters and return values
  - Any error conditions
  - A short example where helpful

- **Every module** (`mod.rs` or top of file) must have a `//!` module-level doc
  comment explaining the module's purpose and how it fits into the larger system.

- **Every crate** (`lib.rs` or `main.rs`) must have a `//!` crate-level doc comment
  explaining the crate's role in the workspace.

### 8.2 Inline Comments (`//`)

- Use inline comments to explain **why**, not **what**. The code should be readable
  enough that *what* is obvious; comments should explain the reasoning.
- **Always** comment:
  - `unsafe` blocks — explain exactly why the unsafe code is sound.
  - FFI calls — explain what the C function does and what the handle/pointer
    ownership rules are.
  - Non-obvious Rust idioms (lifetimes, trait bounds, `PhantomData`, pin/unpin,
    etc.) — explain in plain English what the construct achieves.
  - Error handling decisions — why a particular error is mapped, propagated,
    or ignored.
  - Magic numbers and bitmasks — what each value means.

### 8.3 Audience Awareness

- Assume the reader is a **smart undergraduate** who knows basic programming
  but is **new to Rust**. Do not assume familiarity with:
  - Ownership and borrowing rules (explain when non-obvious)
  - The `unsafe` keyword and what it means
  - FFI patterns (`extern "C"`, raw pointers, `repr(C)`)
  - Async/await and tokio concepts
  - The trait system (especially `impl Trait`, `dyn Trait`, associated types)
- Use **simple, clear English**. Avoid jargon where a plain explanation works.
- When a design pattern is used (e.g., RAII, newtype, builder, state machine),
  name the pattern and briefly explain it.

### 8.4 Keeping Documentation Current

- When code is modified, **update all affected doc comments in the same commit**.
- Never leave stale doc comments that describe old behaviour.
- Run `cargo doc --no-deps` periodically and verify the output makes sense.
- The development roadmap (`docs/development-roadmap.md`) must be updated when
  phases are completed or when plans change.

---

## 10. Testing Strategy & Test-Driven Development

All code must be developed following a **Test-Driven Development (TDD)** philosophy.
Tests are not an afterthought — they are written *before or alongside* the code they
verify. Every commit must pass `cargo test` with zero failures.

### 9.1 Test Organization

- **Unit tests**: Live in `#[cfg(test)] mod tests { ... }` blocks at the bottom of
  the same file they test. This keeps tests close to the code they verify.
- **Integration tests**: Live in `tests/` at the workspace root. These test
  cross-crate interactions (e.g., FFI → core → server).
- **Hardware tests**: Live in `tests/` with the `#[ignore]` attribute by default.
  They are run explicitly with `cargo test -- --ignored` when hardware is available.

### 9.2 Three Testing Tiers

This project has three tiers of tests, each serving a different purpose:

#### Tier 1: Emulated Tests (`okREALM_TEST`) — Always Run

These tests use the FrontPanel `okREALM_TEST` realm to simulate device connections
without any physical hardware. They **must always pass** on every platform (Ubuntu
22.04, Ubuntu 24.04, Windows 11) and are the primary CI gate.

Use `okREALM_TEST` for:
- Device manager lifecycle testing (connect, disconnect, re-connect)
- Event channel verification (DeviceAdded, DeviceRemoved events)
- USB 2.0 filtering (emulate a USB 2.0 device → verify it is skipped)
- Resource state machine transitions (acquire, release, invalidate on disconnect)
- Hot-plug callback thread safety

```rust
// Example: emulate a device connection in a test
#[test]
fn test_device_manager_detects_emulated_device() {
    // Construct manager with okREALM_TEST
    // Call okFrontPanelManager_EmulateTestDeviceConnection("TEST0001", true)
    // Assert DeviceAdded event is received
    // Call okFrontPanelManager_EmulateTestDeviceConnection("TEST0001", false)
    // Assert DeviceRemoved event is received
}
```

#### Tier 2: Real Hardware Tests — Run When FPGAs Are Connected

These tests use **real, physically connected FPGAs**. They are marked with
`#[ignore]` so they do not run in CI by default, but **must be run whenever a
connected FPGA is discovered** during development or manual testing.

**Discovery rule**: Once basic FPGA discovery is confirmed working (Phase 3-4),
any test session where `cargo test` detects one or more connected supported FPGAs
should automatically run the hardware test suite. This can be implemented as:
- A test helper that checks `FrontPanelDevices` for connected devices at the start
  of each hardware test and skips (rather than fails) if none are found.
- A `--features hardware-tests` flag that enables the hardware tier.

Use real hardware tests for:
- Actual device info readback (verify `okTDeviceInfo` fields match the real board)
- Sensor readings (verify voltage, temperature sensors return plausible values)
- Device settings read/write round-trip
- SYZYGY port discovery on boards with SYZYGY peripherals attached
- Bitfile programming (using a known-good test bitfile)
- Wire, trigger, and pipe I/O (with a programmed FPGA running a test gateware)
- PLL configuration read/write
- Data port type detection (Classic vs AXI)

```rust
// Example: read real device info
#[test]
#[ignore] // Only runs with: cargo test -- --ignored
fn test_real_device_info_readback() {
    // Enumerate devices with FrontPanelDevices (okREALM_LOCAL)
    // Skip if no supported devices found
    // Open first device
    // Read okTDeviceInfo
    // Assert serial is non-empty, productID is in SUPPORTED_PRODUCTS
    // Assert deviceInterface is NOT USB2
}
```

**Important**: Real hardware tests must **never modify persistent device state**
(e.g., flash writes, EEPROM PLL config changes) without explicit user confirmation.
Read-only operations (sensors, settings, device info) are always safe.

#### Tier 3: Mock-Based Unit Tests — Always Run

These tests use mock/stub implementations of the FFI layer to test the core domain
logic and server route handlers in isolation, without any DLL or hardware dependency.

Use mocks for:
- `ok-fpga-core` domain logic (resource acquisition, state machine, event routing)
- `ok-fpga-server` route handler logic (request parsing, response formatting, error mapping)
- Authentication provider logic (NoAuth, Local, LDAP stubs)
- Configuration parsing and validation

The FFI crate exposes traits that the core crate depends on, enabling mock
implementations in tests:
```rust
// The core crate depends on a trait, not on the concrete FFI struct
pub trait FrontPanelApi: Send + Sync {
    fn get_device_info(&self, handle: DeviceHandle) -> Result<DeviceInfo, FfiError>;
    // ...
}

// In tests, use a mock:
struct MockFrontPanelApi { /* ... */ }
impl FrontPanelApi for MockFrontPanelApi { /* ... */ }
```

### 9.3 Test Naming Convention

Use descriptive names following the pattern:
```
test_{function_or_module}_{scenario}_{expected_outcome}
```

Examples:
- `test_device_manager_usb2_device_is_skipped`
- `test_resource_acquire_when_locked_returns_error`
- `test_syzygy_discovery_with_no_peripherals_returns_empty_list`
- `test_real_device_sensors_return_plausible_values`

### 9.4 Rules

- **Tests first**: Write or update tests *before or alongside* production code.
- **Zero broken tests**: Never bypass, disable, or `#[allow]` a failing test to
  force a build to pass. Fix the code or fix the test.
- **Continuous alignment**: When code changes, update all affected tests in the
  same commit.
- **`cargo test` before every commit**: Per §8, a commit requires a passing build.
  `cargo test` (which includes `cargo build`) is the minimum bar.

---
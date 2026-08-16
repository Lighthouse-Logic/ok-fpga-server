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

## 6. Architecture Decision Records (ADRs)

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

## 7. Commit After Every Successful Build

After every `cargo build` or `cargo test` run that completes successfully:

1. Stage all modified tracked files: `git add -A`
2. Commit with a clear, descriptive message:
   - Prefix: `feat:`, `fix:`, `refactor:`, `chore:`, `docs:` as appropriate.
   - Body: list the key changes (crates affected, API changes if applicable).
3. Report the short commit hash in the response.

Do **not** batch multiple unrelated features into a single commit.
Do **not** skip the commit step because "it's just a small change."

---

## 8. Rust Code Comments & Documentation Standards

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
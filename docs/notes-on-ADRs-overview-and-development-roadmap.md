For project specification and documentation clarity, we must improve **Intent (Roadmap)** and **Settled Architecture (ADRs)**:

* **Roadmaps** are forward-looking artifacts designed to communicate **momentum, phases, and priorities**. When they retain obsolete, stale implementation notes, they lose their scannability and primary purpose.
* **ADRs (Architecture Decision Records)** are immutable records of **why specific technical paths were chosen, superseded, or amended**.
* **Changelogs / Git History** are historical records of **what actually landed**.

---

### Best Practices for Refactoring `development-roadmap.md`

#### 1. Split the Stale History into an `archive/` or `CHANGELOG.md`

If you love having the narrative timeline of how the project evolved:

* **Option A: The Append-Only Changelog / Project Log:** Convert completed roadmap phases into high-level milestone releases in a `CHANGELOG.md` or `docs/history/milestones.md`.
* **Option B: Roadmap Archive:** Move completed early phases (e.g., *Phase 0: Initial Scaffolding*, *Phase 1: Proof of Concept*) into `docs/roadmap-archive.md`.
* Keep `development-roadmap.md` strictly focused on **Current Phase**, **Next Phase**, and **Future Horizons (Backlog/Epics)**.
Recommend: Option B, in a /docs/archive folder, with the roadmap storing only a limited number of logically grouped (in-order) phases to keep each individual file size small.   Start a CHANGELOG.md only when the first official release is issued.

#### 2. Establish a "Source of Truth" Boundary (Link, Don't Duplicate)

When a roadmap item involves a major technical pivot or settled architectural design, **strip technical details from the roadmap and link directly to the ADR**.

```markdown
<!-- Bad: Duplicating stale technical design in the roadmap -->
- [x] Phase 2: Host Interface
  - Implement custom IPC over shared memory using lockless ring buffers... (Stale: later replaced by gRPC)

<!-- Good: Clean roadmap with ADR linkage -->
- [x] Phase 2: Host Interface & Telemetry Bridge
  - Status: Completed (v0.4.0)
  - Architecture: See `ADR-0004: gRPC Transport Migration`

```

#### 3. Shift from Low-Level Tasks to High-Level Outcomes/Epics

A common anti-pattern in long-running projects is treating the roadmap as a giant task backlog (like a local Jira board in markdown).

* **Roadmaps should track Epics/Capabilities:** *"Add native TLS termination for testbench nodes"*.
* **Issue trackers / PRs track Tasks:** *"Update `config.toml` schema"*, *"Integrate `rustls-pki-types`"*.
* **ADRs track Technical Mechanics:** *"Why Rustls was selected over OpenSSL"*.

---


### Recommended Document Hierarchy

| Document | Primary Question Answered | Direction | Lifecycle |
| --- | --- | --- | --- |
| **`development-roadmap.md`** | *"Where are we heading next and what are current priorities?"* | Future / Active | **Living Document** (Trimmed regularly) |
| **`docs/adr/overview.md`** | *"What is the current status and index of all architectural decisions?"* | Current / Ground Truth | **Living Index** (Updated as ADRs are added/superseded) |
| **`docs/adr/NNNN-*.md`** | *"Why did we make this specific technical architectural decision?"* | Context / Rationale | **Immutable** (Amended only via new ADRs)(*1) |
| **`CHANGELOG.md`** | *"What features/fixes landed in which release?"* | Past | **Append-Only** |
| **`docs/archive/`** | *"What were our early prototype roadmaps/explorations?"* | Historical Archive | **Read-Only / Archived** |

> **(*1 NOTE:** ADRs can be updated to correct errors, update Status (e.g., changing "Active" to "Superseded by..."), add/update "Depends on:" information (pointers to other ADRs), or update References (if including pointers to other ADRs), but no other update of existing ADRs is allowed. Refer to section 7 of /.agents/rules/ai-collaboration.md )

---

### Practical Step-by-Step Clean Up

1. **Prune & Snapshot:** Copy all completed milestones prior to your current active cycle into an `archive/` note or your release changelog.
2. **Back-link Completed Milestones:** For remaining completed phases, collapse them into a single-line summary with a reference to the relevant ADRs.
3. **Format by Horizons:** Structure the active roadmap into 3 clear buckets:
* **Now (Active Horizon):** The current focus with explicit milestone targets.
* **Next (Planned Horizon):** Well-defined epics scheduled immediately after the active milestone.
* **Later / Horizons (Exploratory):** Broad aspirational features or uncommitted research topics.
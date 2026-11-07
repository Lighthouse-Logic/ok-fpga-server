# Archive — Phase 0: Project Scaffolding & Documentation Framework

> **Archived:** 2026-01-15
> **Source:** [development-roadmap.md](../development-roadmap.md) §0
> **ADR:** [ADR-000](../architecture/adr/ADR-000-template.md) (template)

*This is a worked example of the archive format. Replace or delete it once
real phases start landing here.*

---

## 0-A — Documentation skeleton 🔒

**Commits:** `0000aaa` (scaffold), `0000bbb` (rules customization).

**Verification:** `scaffold_project_docs.sh --check` green; roadmap horizons
populated; ADR index in overview.md present.

**0-A items**
- [x] S-01: Install docs scaffolding (rules, roadmap, ADR template, archive).
- [x] S-02: Customize AI collaboration rules (sources of truth, hardware
  risks, test commands).
- [x] S-03: Seed roadmap Phase 1 with acceptance criteria.

**Notes:** Placeholder `{{HARDWARE_RISKS}}` was replaced with the project's
actual risky-operation list; the sandbox hook was left installed and verified
with a synthetic out-of-bounds write (blocked as expected).

# Roadmap Archive

Completed phases move here from [`docs/development-roadmap.md`](../development-roadmap.md)
to keep the roadmap scannable. One file per phase — `roadmap-phases-NN.md` —
or grouped ranges for small early phases (`roadmap-phases-00-06.md`).

## Header pattern (required)

```markdown
# Archive — Phase NN: [Title]

> **Archived:** YYYY-MM-DD
> **Source:** [development-roadmap.md](../development-roadmap.md) §NN
> **ADR:** `ADR-NNN` (`../architecture/adr/ADR-NNN-slug.md`)
```

## Rules

1. Copy the phase's item tables, verification notes, and commit hashes
   **verbatim** from the roadmap at the moment of completion.
2. After archiving, the roadmap keeps only a one-line summary + ADR links
   in its 📦 Completed Phases section.
3. Archive files are **read-only** afterwards — corrections go in new ADRs.
4. Start a `CHANGELOG.md` only at the first official release; until then the
   archive is the project log.

See [example-archived-phase.md](example-archived-phase.md) for a worked example.

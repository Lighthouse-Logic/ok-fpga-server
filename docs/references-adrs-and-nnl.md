# ADRs and Now–Next–Later: Where These Ideas Actually Come From

> Referenced brief for the documentation scaffolding (system-blueprint SDR-0024).
> **The short version:** every mechanism in this framework was developed for
> human project teams, decades before AI coding agents existed. Agentic
> tooling inherits them because they were already good engineering practice —
> not the other way around.

---

## 1. Architecture Decision Records (ADRs)

**What it is:** a short, numbered, immutable document capturing one
architecturally significant decision with its context and consequences;
superseded, never deleted.

- The canonical write-up is **Michael Nygard, "Documenting Architecture
  Decisions" (Cognitect blog, 2011-11-15)** [1]: records for decisions that
  "affect the structure, non-functional characteristics, dependencies,
  interfaces, or construction techniques," stored in the repository as
  `doc/arch/adr-NNN.md`, numbered "sequentially and monotonically," with
  reversed decisions "kept around, but marked as superseded." The format
  (Title / Context / Decision / Status / Consequences) is modeled on
  Christopher Alexander's pattern documents — "the characteristic balancing
  of forces." Cognitect had used it in client projects since **early August
  2011**.
- The academic concept is older. Wikipedia's "Architectural decision"
  article [2] traces design-rationale research to **Perry & Woolf's 1998**
  definition of software architecture, then to a **2004 Groningen workshop**
  on architectural decisions and Architectural Knowledge Management, with
  **Tyree & Akerman, "Architecture decisions: demystifying architecture"
  (IBM Systems Journal, 2004)** among the early publications; a Springer
  handbook (2009) and a systematic mapping study (2013) consolidated the
  field.
- The decision-record lineage is older still: the ADR community repository
  (17k+ stars) [3] lists **Issue-Based Information Systems — designed by
  Horst Riehl in 1965**, popularized as dialogue mapping by Conklin (2006) —
  plus Questions/Options/Criteria and Decision Representation Languages as
  the intellectual ancestors of the ADR.
- Industry adoption long predates AI assistants: the **ThoughtWorks
  Technology Radar** blipped "Lightweight Architecture Decision Records" in
  **November 2016 (Trial)** and moved it to **Adopt in November 2017**,
  recommending storage in source control so the record "remains in sync with
  the code itself" [4]. Markdown ADR (MADR) conventions followed [5].

## 2. Now / Next / Later (NNL)

**What it is:** a three-bucket roadmap keeping intent scannable — Now =
committed active work, Next = scoped near-term, Later = exploratory backlog.

- Product School's reference article (2024, updated 2025) [6] defines the
  "Now-Next-Later roadmap" (also "Now-Next-Future" / "Now-Then-Later") as
  "a strategic framework used in product management to categorize tasks and
  initiatives into three time-focused segments," valued for re-prioritization
  speed and stakeholder communication — and explicitly warns against it
  where granular scheduling is required.
- Atlassian's agile-roadmap guidance [7] — mainstream product practice well
  before agentic AI — prescribes the same hygiene this scaffolding encodes:
  roadmaps track outcomes, not "detailed requirements… weighing down the
  roadmap"; review them on a cadence; balance short-term tactics against
  long-term strategy to avoid "thrashing, staleness, and nearsightedness."
- The three-bucket horizon idea descends from 1990s corporate strategy:
  **McKinsey's "three horizons of growth" (Baghai, Coley & White, *The
  Alchemy of Growth*, 1999)**, adapted into the futures-studies **Three
  Horizons framework in 2006** (Hodgson, Curry, Leicester, Sharpe, Lyon &
  Fazey; formalized in *Ecology & Society*, 2016) and used today by
  organizations including UNDP and UNICEF [8].

## 3. The point: these predate agentic AI

| Year | Development | AI era? |
| --- | --- | --- |
| 1965 | Riehl's IBIS decision mapping (ADR ancestor) | — |
| 1998 | Perry & Woolf place rationale in architecture's definition | — |
| 1999 | Baghai/Coley/White: three horizons of growth | — |
| 2004 | Tyree & Akerman; Groningen architectural-knowledge workshop | — |
| 2011 | Nygard publishes the ADR format as used today | — |
| 2016–17 | ThoughtWorks Radar moves ADRs Trial → **Adopt** | — |
| 2010s | Now/Next/Later becomes standard product-management practice | — |
| 2021+ | LLM code assistants; 2023+ agentic coding tools | ← all of the above already existed |
| 2024+ | ADR tooling *for* agents appears: ADR Guard (CI gate), Decision Guardian, kgai decision logs, machine-readable DRF [3] | the pre-AI artifact being adopted by AI, not invented for it |

Why the conventions transfer to agents so cleanly: ADRs externalize the
*why* that survives team turnover — and model turnover; NNL bounds
commitment without a Jira-sized backlog; the archive protocol keeps living
documents scannable. An agent required to write an ADR before a technical
pivot is governed by exactly the discipline that governs a new human hire.

---

## References

1. Nygard, M. (2011-11-15). *Documenting Architecture Decisions.* Cognitect
   blog. <https://cognitect.com/blog/2011-11-15/documenting-architecture-decisions>
   (work released under a CC0 waiver)
2. Wikipedia. *Architectural decision.* <https://en.wikipedia.org/wiki/Architectural_decision>
   (cites Perry/Woolf 1998; Tyree & Akerman 2004; Babar, Dingsøyr, Lago &
   van Vliet, *Software Architecture Knowledge Management*, Springer 2009)
3. architecture-decision-record (GitHub organization, est. Joel Parker Henderson).
   *Architecture decision record (ADR)* curated repository.
   <https://github.com/joelparkerhenderson/architecture-decision-record>
4. Thoughtworks. *Lightweight Architecture Decision Records.* Technology
   Radar (published 2016-11-07; Adopt ring 2017-11 through 2018-05).
   <https://www.thoughtworks.com/radar/techniques/lightweight-architecture-decision-records>
5. *Markdown Architectural Decision Records (MADR).* <https://github.com/adr/madr>
6. Villaumbrosia, C. (2024-05-09, updated 2025-03-24). *Curves Ahead:
   Navigating Change with Now-Next-Later Roadmap.* Product School.
   <https://productschool.com/blog/product-strategy/now-next-later-roadmap>
7. Atlassian. *Agile roadmaps: Build, share, use, and evolve.*
   <https://www.atlassian.com/agile/product-management/roadmaps>
8. Wikipedia. *Three Horizons.* <https://en.wikipedia.org/wiki/Three_Horizons>
   (3H created 2006, adapted from McKinsey's three horizons of growth;
   Sharpe et al., *Ecology and Society* 21(2), 2016; Baghai, Coley & White,
   *The Alchemy of Growth*, Broadway Books, 1999)
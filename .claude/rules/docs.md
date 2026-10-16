---
paths:
  - "docs/architecture.md"
  - "docs/tech-debt.md"
---

# docs/architecture.md and docs/tech-debt.md

Both change only through reviewed pull requests, together with the code they
describe; the `liaison` decides on them (`docs/agents.md`).

## docs/architecture.md

- How the project works and why, as the code on `main` is now: crate
  boundaries, the core/client boundary, the HTTP API, the public library API,
  the vault format and path rules, the sync protocol, cross-cutting schemes.
- Sufficient: an agent that starts cold can place a change correctly from
  this document and the rule files, without re-deriving the design from the
  code.
- Minimal: no detail the code already shows (member lists, signatures,
  file-by-file descriptions), no history, no plans (`docs/roadmap.md`),
  nothing a rule file already states: refer to it.
- Clean: each fact stated once, nothing outdated or contradictory. A change
  rewrites or removes what it made outdated; it does not only append.

## docs/tech-debt.md

- Problems whose fix is a task of its own: what is simplified or postponed.
  Not here: blocked tasks, open questions (`docs/questions.md`), ideas and
  plans (`docs/roadmap.md`).
- A developer adds an entry in the pull request of the task that met the
  problem; the pull request that resolves an entry removes it.
- One bullet per problem, under the section of its area: **what** (bold, with
  the files or modules) - why so - the risk or cost while it stays - how to
  close it.
- Sufficient: an agent that starts cold can take the entry as a task.
  Minimal: no history, no author or date, nothing the code or a rule file
  already states. Clean: each problem has one entry, and an entry that a
  change made outdated is rewritten or removed.

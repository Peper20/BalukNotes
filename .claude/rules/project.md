# baluk notes - rules for the agent

A notes app on Typst: a Rust core builds a vault of `.typ` files into HTML and
PDF, a Svelte client shows the notes.

## Where to find what

| What | Where |
|---|---|
| what it is, installation, `notes` commands | `README.md` (Russian copy - `README.ru.md`) |
| how it works and why | `docs/architecture.md` |
| what it does: behaviour specs with requirement ids | `docs/specs/<area>.md` (format - the `sdd` skill) |
| the plan and the user's decisions on it | `docs/roadmap.md` |
| tech debt: what is simplified, the risk, how to close it | `docs/tech-debt.md` |
| open questions to the user | `docs/questions.md` |
| the development pipeline: agents, the mod, CI, review | the kit `pipeline/` (`pipeline/docs/README.md`); this project's side - `docs/agents.md`, `.claude/pipeline.json` |
| how to write notes | `docs/writing.md` (= `notes docs writing`) |
| the storage server on a VPS: service, nginx | `docs/server.md` |
| research measurements | `docs/research/` |
| the layout library API | `baluk/README.md` (= `notes docs library`) |
| fonts and their licenses | `fonts/README.md` |

Directory rules - `.claude/rules/<name>.md`, Claude Code loads them itself when
working with files of the directory (the `paths` field): `crates` (core,
server, CLI), `app` (client), `baluk` (editing the library), `skills`
(skills), `tools` (checks, screenshots, cloud environment), `tests-vault` (the
fixture vault), `vendor` (copies of foreign crates), `docs` (the architecture
and tech debt documents). Need the rules of another
directory - read the file.

## Essentials

- Rendering comes first: a note in HTML looks like its PDF.
- Note files are the source of truth; the app does not change them silently.
- Optimization is not a goal now (user's decision): do not optimize small
  things (< 1 MB, tens of ms). Know the weak spots and record them as tech
  debt.
- The user's vaults are in the data directory (`notes info`): copies of their
  notes, they may be edited. Never touch the originals
  (`~/Documents/abstract/...`).
- How to make a feature (interface look, behavior, names) - decide yourself
  and do it: the user sees the result while testing and asks for a fix if they
  disagree. A question to the user (`docs/questions.md`) has a high bar: only
  what is expensive to redo or cannot be undone (architecture, security, data
  loss, actions on the user's machine or server) and has no obvious answer -
  then a default option + the question. New features - only on request.

## Iteration

- While working - the tests the change needs; the full `tools/check.sh` runs
  in CI on every push to a pull request (`docs/agents.md`).
- Changed the public library API, the `note`/`book` templates
  (`notes-core::new_note`), `notes` commands or path rules (`NoteId`) - in the
  same change fix `docs/writing.md` and the `/baluk-note` skill (`SKILL.md`,
  `examples/`, `reference.md`; what the tests check -
  `.claude/rules/skills.md`), then `tools/install.sh`.
- At the end: mark what is done in the roadmap; tech debt is added and closed
  in the pull requests.

## Pipeline

`lead` (the main session) -> the pipeline mod (`pipeline/`, code) ->
`developer-1`, `developer-2`, `developer-3` -> CI -> `reviewer`; definitions -
`pipeline/claude/agents/` (linked into `.claude/agents/`), how it works -
`pipeline/docs/README.md` and `docs/agents.md`. The repository is on a local
Forgejo server (`origin`, the `forge` skill), GitHub is its mirror. Code and the
documents that change with it reach `main` only through a reviewed pull request;
the `lead` owns the architecture. The lead commits `docs/roadmap.md`,
`docs/questions.md`, `docs/agents.md`, the design decisions in
`docs/architecture.md` and `docs/tech-debt.md` that no code yet follows, and
`.claude/rules/`, `.claude/pipeline.json`, the kit's agents, skills and
documents (`pipeline/claude/`, `pipeline/docs/`, `pipeline/config.md`) straight
to `main`. Permission settings (`.claude/settings*.json`) are changed only by
the user.

## Language

- English: code, comments, logs, messages, docs (`.claude/rules/`, `docs/`,
  README), skill texts, commit messages and PR titles and descriptions.
- Russian: the client interface, interface texts in the core
  (`.claude/rules/crates.md`), library captions (`i18n.typ`) and theme
  titles, `README.ru.md` (a copy of the root README, they link to each other;
  edit both).

## Documents

- Plain Markdown without typography: `"`, `-`, `->`, `...` instead of curly
  quotes, dashes, arrows and ellipses.
- Every fact in one place, the others refer to it. What is done is described
  as it works, without history (that is in git).
- Measure size with `tiktoken` (the command - `.claude/rules/skills.md`): do
  not bloat.
- This file is only a map and general rules, details are in the directory
  rules (`.claude/rules/`). Agree edits of this file with the user.

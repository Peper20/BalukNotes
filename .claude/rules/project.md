# baluk notes - rules for the agent

A notes app on Typst: a Rust core builds a vault of `.typ` files into HTML and
PDF, a Svelte client shows the notes.

## Where to find what

| What | Where |
|---|---|
| what it is, installation, `notes` commands | `README.md` (Russian copy - `README.ru.md`) |
| how it works and why | `docs/architecture.md` |
| the plan and the user's decisions on it | `docs/roadmap.md` |
| tech debt: what is simplified, the risk, how to close it | `docs/tech-debt.md` |
| open questions to the user | `docs/questions.md` |
| how to write notes | `docs/writing.md` (= `notes docs writing`) |
| research measurements | `docs/research/` |
| the layout library API | `baluk/README.md` (= `notes docs library`) |
| fonts and their licenses | `fonts/README.md` |

Directory rules - `.claude/rules/<name>.md`, Claude Code loads them itself when
working with files of the directory (the `paths` field): `crates` (core,
server, CLI), `app` (client), `baluk` (editing the library), `skills`
(skills), `tools` (checks, screenshots, cloud environment), `tests-vault` (the
fixture vault), `vendor` (copies of foreign crates). Need the rules of another
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
- Do not decide disputable things (interface look, behavior for the user, the
  public library API) without the user: a default option + a question in
  `docs/questions.md`.

## Iteration

- Before a commit and a PR - `tools/check.sh`.
- Changed the public library API, the `note`/`book` templates
  (`notes-core::new_note`), `notes` commands or path rules (`NoteId`) - in the
  same change fix `docs/writing.md` and the `/baluk-note` skill (`SKILL.md`,
  `examples/`, `reference.md`; what the tests check -
  `.claude/rules/skills.md`), then `tools/install.sh`.
- At the end: mark what is done in the roadmap, add tech debt, delete what is
  closed.

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

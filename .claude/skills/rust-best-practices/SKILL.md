---
name: rust-best-practices
description: >
  Rust rules for the baluk notes workspace (crates/): Apollo GraphQL's Rust best practices handbook,
  adapted to this project. Load before writing, editing or reviewing any .rs file here:
  (1) new Rust code or functions, (2) reviewing or refactoring, (3) borrowing vs cloning,
  (4) error handling, (5) tests, (6) comments and docs.
license: MIT
compatibility: Rust 1.92+ (edition 2024), Cargo workspace
metadata:
  author: apollographql (adapted for baluk notes)
  version: "1.1.2-baluk.1"
allowed-tools: Bash(cargo:*) Bash(rustc:*) Bash(rustfmt:*) Bash(clippy:*) Read Write Edit Glob Grep
---

# Rust Best Practices (baluk notes)

Based on Apollo GraphQL's [Rust Best Practices Handbook](https://github.com/apollographql/rust-best-practices)
(MIT, `LICENSE`). The chapters in `references/` are the handbook text with broken examples fixed; this
file adapts it to the project. Where they disagree, the order is: `crates/README.md` (project layout and rules) > "Project rules"
below > the handbook.

Before editing, read `crates/README.md` (modules, `NoteId`, `trait Storage`, cache keys, test layout). Read
handbook chapters only when the task touches their topic.

## Project rules (override the handbook)

### Panics: no `unwrap`/`expect` outside tests (handbook 4.2)
- Restructure instead: `let ... else`, `?` with an error, a constructor that cannot fail, a type that makes the
  case impossible.
- Only what the code itself guarantees and cannot express in types may panic: `expect("<the invariant>")` under
  `#[expect(clippy::expect_used, reason = "...")]`.
- Anything from the user, the disk, the network or Typst is not an invariant: return `Result`.
- The code base is being brought to this rule (`docs/roadmap.md`, "Приоритетное"): older code may still have
  bare `expect`; do not add new ones.
- Note compile errors are not `Err`: they are `Diagnostic`s in `NotePage` (`crates/README.md`).

### Errors
- Libraries: `thiserror` (`notes_core::Error`); binaries (`notes-typst`, `notes-app`): `anyhow` or a message
  string - as the handbook says. HTTP errors go through `notes-server/src/error.rs`.

### Lints
- `[workspace.lints]`: clippy `all` + `pedantic` as warnings, `unsafe_code = "forbid"`; `tools/check.sh` denies warnings.
- Silence a lint locally with `#[expect(clippy::x, reason = "...")]`, never without a reason (handbook 2.4).
  Older code has `#[allow(..., reason)]`: switch it to `expect` when you touch it.
- Full check: `tools/check.sh`. Quick: `cargo clippy --workspace --all-targets --features notes-core/measure
  -- -D warnings`. Not `--all-features`: features `ts` and `measure` are for type export and measurements, and
  `notes-app` needs WebKitGTK.
- `rustfmt.toml`: `max_width = 120`. Stable rustfmt: no `group_imports`/`imports_granularity` (handbook 1.7
  needs nightly).
- `rust-version = "1.92"`: no newer std features (`assert_matches!` from handbook 5.4 is 1.96).

### Performance: correct and clear first
- User decision: optimization is not a goal now. Do not micro-optimize (under ~1 MB, tens of ms); a known weak
  spot goes to `docs/tech-debt.md` with numbers.
- Borrowing over cloning stays the default idiom (handbook 1.1), for clarity, not speed. Do not contort code to
  save a clone of a small value (handbook 3.2 is advice, not a rule here).
- Measuring: feature `measure` (heavy tests), `RUST_LOG=notes_core=debug`, write-ups in `docs/research/`. No
  flamegraph or criterion setup unless asked.

### Tests
- Unit tests next to the code; integration tests in `crates/*/tests/it/` (one binary per crate, new file = a
  module in `main.rs`). Fixture vault: `tests/vault`; core layer tests without Typst
  (`pages::tests::Setup`, `MemStorage`).
- Handbook 5.1 ("one assertion per test", long `should_..._when_...` names) does not apply: a test here checks
  one behaviour with several cases in a row (table style), and its short name says the behaviour
  (`filters`, `center_stays`). Typst compilation is slow, so cases share one setup.
- No new test dependencies (`rstest`, `insta`) without need: snapshots are `tests/snapshots/`.

### Language
- Target (user decision, `docs/roadmap.md` "Потом"): the whole project in English - identifiers, comments,
  docs, logs, error and CLI messages, `docs/`, READMEs, `CLAUDE.md`. Russian stays only in the client UI (and a
  Russian copy of the root README).
- The move is one separate task (message texts are checked by tests). Until then, follow the language of the
  file you edit, so each file stays in one language; identifiers are English already.

### Comments and docs
- `//!` at the top of a module says how it works and why; `///` on items says what they are. Comments explain
  why, briefly (handbook 1.6 and 8.2 agree).
- Long design reasoning lives in `docs/architecture.md` (the project's ADRs); comments point to it
  (`architecture §1`) instead of repeating it. Each fact in one place.
- No `TODO` in code (handbook 8.6 wants an issue): record the debt in `docs/tech-debt.md`.
- `missing_docs` is not enabled (crates are internal, `publish = false`); `missing_errors_doc` and
  `missing_panics_doc` are allowed.
- Describe how the code works now, not its history (git keeps history).

### Concurrency
- Core: `parking_lot` locks. With `std::sync::Mutex`, handle poisoning explicitly
  (`unwrap_or_else(PoisonError::into_inner)`).
- Async server: blocking work (Typst, disk) in `spawn_blocking` (`notes-server/src/error.rs`).

## Handbook quick reference (still applies)

### Borrowing & ownership
- Prefer `&T` over `.clone()` unless ownership is needed; `&str` / `&[T]` in parameters.
- Small `Copy` types (≤24 bytes) by value; `Cow<'_, T>` when ownership is ambiguous.

### Errors
- `?` over match chains; map errors with context at the boundary.

### Iterators
- Iterators over manual index loops; no intermediate `.collect()` just to iterate again. A plain `for` is fine
  when it reads better (handbook 1.5).

### Functions
- Extract on the third repetition or when a block has a name of its own; do not extract a wrong abstraction
  (handbook 1.8). Test code: readability beats DRY.

### Generics & dispatch
- Generics (static dispatch) by default; `dyn Trait` for heterogeneous collections and plugin-like registries.

### Type state
- Use it when invalid states would otherwise be easy to reach at runtime; not for simple flags (handbook 7.5).

## Handbook chapters

- [1 - Coding styles and idioms](references/chapter_01.md): borrowing vs cloning, Copy, Option/Result,
  iterators, comments, extracting functions
- [2 - Clippy and linting](references/chapter_02.md)
- [3 - Performance mindset](references/chapter_03.md) - see "Performance" above
- [4 - Error handling](references/chapter_04.md) - see "Panics" above
- [5 - Automated testing](references/chapter_05.md) - see "Tests" above
- [6 - Generics and dispatch](references/chapter_06.md)
- [7 - Type state pattern](references/chapter_07.md)
- [8 - Comments vs documentation](references/chapter_08.md) - see "Comments and docs" above
- [9 - Understanding pointers](references/chapter_09.md): Send/Sync, `Arc`, `Mutex`, `OnceLock`

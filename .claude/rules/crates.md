---
paths:
  - "crates/**"
  - "Cargo.toml"
  - "clippy.toml"
---

# crates - core, server, CLI

How it works and why - `docs/architecture.md`; here - code rules of `crates/`.

| Crate | What |
|---|---|
| `notes-core` | the core without async or HTTP: vault, Typst compiling, HTML processing, cache, warm-up, links, graph, search, settings |
| `notes-server` | HTTP API (axum) and serving the client |
| `notes-assets` | the client `app/dist` for the server |
| `notes` | the thin `notes` command: calls the parts from its own folder, no dependencies (architecture §1) |
| `notes-typst` | the part that builds with Typst: every command except those of other parts |
| `notes-app` | the window (Tauri 2) without Typst: the `notes://` scheme -> the core over a Unix socket; needs WebKitGTK |

Logic lives in the core, the other crates are thin wrappers. A new app part is
a binary `notes-<name>` and a line in `notes::PARTS` (its commands); shared
`notes-typst` flags with a value - `notes::VALUE_FLAGS` (a test checks it).

The app icon has one source, `app/public/assets/icon.svg` (site favicon, menu -
`tools/install.sh`); the Tauri window needs a PNG, after editing the SVG:
`rsvg-convert -w 256 -h 256 app/public/assets/icon.svg -o
crates/notes-app/icons/icon.png`.

General Rust rules - the Claude Code skill `rust-best-practices`
(`.claude/skills/`, the `apollographql/skills` handbook, MIT, with fixed
examples; the skill is generic, it knows nothing about the project). The rules
of this file override the skill. Without the skill loaded (after a context
compaction - loaded again) the hook `.claude/hooks/require-rust-skill.py`
(`.claude/settings.json`) blocks editing `.rs` (Edit, Write and writes from
Bash).

Where the project differs from the skill:

- **Panics**: as in the skill - no `unwrap`/`expect` outside tests (lints
  `clippy::unwrap_used`/`expect_used`); `expect` on an invariant the code
  guarantees itself - under `#[expect(clippy::expect_used, reason = "...")]`.
  Allowed in tests: `#[test]` and `#[cfg(test)]` - `clippy.toml`, helpers of
  end-to-end tests - `#![allow(...)]` on the whole test crate (a crate rule,
  not a stub). A Typst panic during a build is a build error of the note
  (`world.rs`, `panic_error`).
- **Performance**: its chapters are advice, the rule is
  `.claude/rules/project.md` (do not optimize small things, weak spots go to
  `docs/tech-debt.md`). Measurements - the `measure` feature,
  `RUST_LOG=notes_core=debug`, reports - `docs/research/`.
- **Tests**: a test is one behavior but several cases in a row (Typst builds
  are slow, setup is shared), a short name (`filters`); `rstest` and `insta`
  are not used - snapshots live in `tests/snapshots/`.
- **Comments**: long rationale goes to `docs/architecture.md`, the code has a
  reference (`architecture §1`); no `TODO` in code - debt goes to
  `docs/tech-debt.md`; `missing_docs` is off (the crates are internal).
- **Language**: code, comments, logs, API errors and `notes` output are in
  English (with the tests of message texts). Interface texts in the core
  (setting labels in `settings.rs`, the name `new_note::UNTITLED`, the group
  `vault_graph::ROOT_GROUP`, the broken link hint in `passes/links.rs`, the
  anchor `"раздел"` in `render::slug`) are Russian.
- **Threads**: in the core - `parking_lot`; for `std::sync::Mutex` poisoning
  is explicit (`unwrap_or_else(PoisonError::into_inner)`); in the server
  blocking work (Typst, disk) goes to `spawn_blocking`
  (`notes-server/src/error.rs`).
- **Checks**: `tools/check.sh`; clippy without `--all-features` (the `ts` and
  `measure` features are for exporting types and measurements, `notes-app`
  needs WebKitGTK). `rust-version = "1.92"` - do not use newer std APIs.

- **Server** - a module per API area (`vaults`, `notes`, `graph`, `search`,
  `settings`, `assets`, `fonts`, `events`) with its own `routes()`; shared -
  `AppState` (`lib.rs`), `error.rs`, the token - `auth.rs`. A new area - a
  module + `merge`. The vault API is under `/api/vaults/{vault}/...`
  (`s.vault(name)` - an open vault); what is shared by all (settings, themes,
  fonts) - without a vault. A response is a named struct
  (`notes-server/src/api.rs` or a core type), not `json!`: the client types
  are exported from it (`.claude/rules/app.md`).
- **Typst is pinned `=0.15.1`** (the HTML export is experimental, the
  `html.*` API changes). Updating is a separate task: tests, a visual check,
  moving or removing the crate copies in `vendor/` (`.claude/rules/vendor.md`).
- Core errors - `notes_core::Error` (thiserror); note compile errors are not
  errors but `Diagnostic` in `NotePage`.
- Vault files only through `trait Storage`. Core layers - architecture §7;
  layer tests without Typst (`pages::tests::Setup`: a fake build +
  `MemStorage`).
- Performance settings are device settings (`device.*`, `SettingDef::device`,
  defaults by `settings::Platform`); the core applies them on the fly
  (`Notes::apply_device`), not as constants.
- **The disk cache** is valid while the rendering code has not changed: the tag
  is a hash of the core sources (`crates/notes-core/build.rs`). A module that
  runs after the cache can be listed in `AFTER_CACHE` there; one affecting raw
  rendering - never. Changed `Record` or `Rendered` - bump `cache::FORMAT`.
- Cache versions and keys only via `version::StableHasher`, not
  `DefaultHasher`.
- Note paths only via `NoteId` (checks `..`, internal `_`/`.`).
- **The note page** is a chain of passes: `render.rs` + `passes/` (over the
  typst-html tree and over text) -> cache -> `finish.rs` (for settings:
  `figures.rs`, `frames.rs`). A new pass - a module and a line in a list
  (`passes::TREE`/`TEXT`, `finish::FINISH`; after the cache - also
  `AFTER_CACHE`). A book chapter - `book.rs`. Pass time and size -
  `RUST_LOG=notes_core=debug` ("pass", "figures").
- Vault data for notes `/_vault/<prefix>/...` - a provider in the registry
  `vault_data.rs`.
- Layout fonts are embedded (`fonts/README.md`); the browser gets the theme
  fonts (main and fallback) as WOFF2 parts (`webfonts.rs`). Changed the part
  encoding or updated `fontcull` - bump `webfonts::ENCODER`.
- Lints - `[workspace.lints]` (clippy pedantic); silence locally,
  `#[expect(..., reason = "...")]`.
- Tests: unit tests next to the code, end-to-end - `crates/*/tests/it/` (one
  binary per crate: a shared core on the fixture - `common.rs`; a new file is a
  module in `main.rs`, not a separate binary; heavy measurements - the
  `measure` feature) on `tests/vault`.

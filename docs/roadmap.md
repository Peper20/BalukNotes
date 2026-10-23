# Plan

Marks: `[x]` done, `[~]` in progress, `[ ]` not started. What is done - one
line (how it works - `docs/architecture.md`), open questions -
`docs/questions.md`.

## Priority

- [x] The server releases memory after builds (user's decision): a release
      after 5 s of idle, `comemo` frees the accelerators, the `notes`
      allocator is jemalloc. `notes serve` idle after warming `tests/vault` -
      361 MB (633 with glibc, ~1.17 GB before the release); numbers -
      `docs/research/E5.md`
- [x] Autostart of `notes serve`: `notes service install | remove | status` -
      a systemd user service (no root, at login, restart after a crash, log -
      journald); `tools/install.sh` restarts it
- [x] Rust per the `rust-best-practices` skill (user's decision): all code in
      `crates/` follows the skill and the project rules
      (`.claude/rules/crates.md`): panics - lints
      `clippy::unwrap_used`/`expect_used`, `expect` outside tests - under
      `#[expect]`, a Typst panic during a build - a build error of the note;
      `#[allow]` -> `#[expect]`
- [x] The whole project in English except the client interface (user's
      decision): code, comments, logs and messages, `docs/`, all README and
      the rules `.claude/rules/`, commit messages and PRs; the root README
      also has a Russian copy (`README.ru.md`). Models read the code and
      texts: English is cheaper in tokens, the skill does not quote Russian
      strings. There is no separate output mode for models: normal output is
      short (one problem - one line), structure - `--json`

## M0. The layout library in HTML

- [x] `baluk/`: HTML and PDF branches for every block, the public API in
      English only through `lib.typ` (a test snapshots the names)
- [x] Themes `classic` and `night`
- [x] The note language `lang:` and own words `words:`; dictionaries `ru` and
      `en` with the same keys (test)

## M1. Core and CLI

- [x] A compiler over the vault: virtual `/_baluk/`, embedded fonts, packages
      from the cache or network; note and book
- [x] A chain of HTML passes; figures - one SVG for all themes, shared glyphs,
      rounding
- [x] Memory and disk cache, warm-up, parallel builds, a watcher
- [x] Link index, backlinks, graph, full-text search
- [x] `notes` from any folder: `serve`, `new`, `list`, `tags`, `check`, `pdf`, `png`,
      `docs`, `info`, `vaults`; installation - `tools/install.sh`
- [x] The `/baluk-note` skill: everything needed is in the skill (workflow,
      reference, samples for every public name), only `notes` commands
- [x] Anchors with symbols: `C++`, `C#`, `a=b` stay in the `id`
- [x] Book properties: the root `main.typ` (`book.with` - shared by all
      chapters), a chapter's own - `chapter.with` (title, tags, label);
      chapter tags in `notes list`/`tags`, on the tag page and in the graph
      filter
- [x] `notes rename`: as in the app (title, file name, `#see` links), the
      skill uses it
- [x] `notes png`: the PDF pages as PNG (`typst-render` of the same version as
      Typst, `--pages`, `--dpi`) - for models that do not read PDF (user's
      decision); the skill checks the look with it
- [x] The browser also gets the theme fallback fonts (New Computer Modern,
      DejaVu Sans Mono): a glyph missing from the main font looks as in the
      PDF; parts only with the page's glyphs (NCM converted to TrueType)

## M2. The client `app/`

- [x] Tree, tabs, history, the Ctrl+K palette and Ctrl+O switching, search
      (also in a book), tags, link previews, contents, "Ссылаются сюда",
      hotkeys, reading mode, settings by the core schema, a narrow screen
- [x] Updates on server events; `refresh.mode`
- [x] Book chapters ahead of time, with a check of the book version
- [x] Several vaults (a choice screen), deleting notes to the trash, titles
      instead of file names
- [x] Ctrl+F in a note and a book: where to search - the chapter, the note or
      book, the whole vault (Tab); a second Ctrl+F - the browser search
- [x] A vault switcher at the bottom of the sidebar (as in Obsidian); rename
      and delete the open vault (to the system trash, with confirmation)
- [x] Vault settings `<vault>/.baluk/settings.json` over the shared ones;
      a change from the interface - for the vault, theme and font size - for
      all ("only for this vault" - optional); the settings window shows where
      a value comes from
- [x] Server connection: a "нет связи" mark in the top bar (the note stays,
      it comes back by itself), no change polling
- [x] Formulas in a title - as source text ("Ряд sum 1/n^2") in the tree,
      tabs and graph; "Ссылаются сюда" - the section heading as in the
      contents
- [x] What the book shares (title, root tags) at the top of the contents in
      any chapter
- [x] Delete a folder from the tree (right click, to the trash with
      everything in it); the theme from the first frame (no white flash);
      switching notes without an empty frame; 3D rotation - the near side
      follows the pointer
- [x] Empty folders in the tree; rename a note, book or folder (right click):
      the title, the file name from it, `#see` links to it - with a plan in
      the dialog (which notes get fixed)
- [x] 3D on a phone: a vertical swipe scrolls the page, one started sideways
      rotates
- [x] The top bar and tabs are one pinned panel (shared background, no gap)
- [x] A note preview on hover and in the tree (the card is to the right of
      the row); the tree menu (right click) names at the top what it is for
- [x] A folder page (`/f/<path>`, user's decision): subfolders and notes, a
      link to the folder graph (`/graph?folder=`, notes with subfolders); the
      path in the top bar - links to folders
- [x] `__NOTES_API__` is only the address (`base`): who may use the server -
      the session cookie (M4)

## M3. Graph and interactivity

- [x] The entry to the vault graph: an icon at the bottom of the sidebar, next
      to "Главная"; the icon is an own "star" of 4 vertices (user's choice)
- [x] Dragging a graph node: a pointer outside the graph does not select page
      text (the graph gesture cancels the default mouse action)
- [x] The `/graph` page and the live graph; a graph in a note `#vault-graph`
- [x] `interactive-plot`, `interactive-surface`, `frames`
- [x] The core layout is Barnes-Hut, 5-10 times faster (`docs/research/E6.md`;
      the look is approved by the user); Ctrl+click on a node and a link - a
      background tab
- [x] A book on the graph: the "книги главами" checkbox on `/graph` - the book
      root and chapters as a cluster around it; a link into a section goes to
      its chapter. The look (user's choice): a chapter is a hollow circle
      smaller than any note, the link is solid and pale; the checkbox is in
      the filter row; not yet on the home page and in `#vault-graph`
- [x] Graph forces are sliders (user's decision): folders, repulsion, center,
      links (the core layout, `Forces` in the filter), neighbors while
      dragging (client physics); a slide-out panel at the side of the graph
      (user's choice; hidden by default, Esc hides it), settings `graph.*`
      for all vaults

## The core on the device, the server is storage

User's decision (architecture §1, §9). The priority is **desktop** (M5): notes
are written on a computer (Claude Code), other devices almost always read. The
desktop works without a server too, sync joins with M4. Research E4 (WASM) -
before M7 starts.

## Device settings - before M5-M6

- [x] The group "Это устройство" (`device.*`): warm-up (the whole vault / only
      the open one / none; a phone - none), simultaneous builds (2), Typst
      memory (10 builds), pages in memory (64 MB), disk cache (512 MB), the
      lifetime of a foreign cache (14 days); a gentle warm-up
- [ ] Phone defaults (memory 32 MB, disk 256 MB, Typst memory 3) are guessed:
      tune them by measurement in M6

## M4. Storage server and sign-in

- [x] Sign-in instead of the shared token (architecture §9): login +
      password (user's decision), accounts - `notes users`; sessions 30 days
      without requests (`--session-days`), token hashes on disk; a pause after
      a wrong password; the sign-in screen on 401 and "Выйти" in the vault
      menu; `notes serve --auth` (required on a non-localhost address), the
      window needs no sign-in
- [x] **A vault from the server is untrusted**: HTML is sanitized by an
      allowlist (allowed tags and attributes), `on*`, dangerous URLs and SVG
      `foreignObject`/`use` pointing outside are removed. The client is served
      with a CSP: `script-src 'self'`, `object-src 'none'`, `base-uri 'none'`,
      `img-src 'self' data: blob:`, `style-src 'self' 'unsafe-inline'`,
      `font-src 'self'`, `connect-src 'self'`, `frame-ancestors 'self'`. The
      Vite dev server is not involved. `notes check` warns with one line if
      something was removed; notes have no JS of their own
- [x] Typst packages - a whitelist with versions (`notes-core::packages`):
      another package is a build error; beyond the list - the device setting
      `device.packages` with a warning (user's decision). Review and extend
      the list regularly: the beauty of notes matters more; the same rules
      apply to sync
- [x] A server without Typst, `notes-hub` (architecture §9): accounts,
      sessions, vault files with versions and history, change long polling;
      the light crate `notes-store` (data directory rules, vault names,
      accounts, sessions, the sync engine)
- [x] The rest of the core split: storage and its watcher, the vault list,
      note paths, the platform and the sync preference are in `notes-store`
      (`notes-device` without Typst); the light commands (`docs`, `vaults`,
      `sync`, `service`) run in the thin `notes`. The settings schema stays in
      the core: it is built from the library themes
- [x] Sync: a copy on the device, file versions, changes on events; who wins
      a conflict - `device.sync_prefer` (the computer writes first, the others
      receive - user's decision); `notes sync`, a background worker in the
      core, the API for the client
- [x] Sync in the app settings (the section "Синхронизация"): the server
      address and sign-in, which vaults are synced (a vault only on the
      server - download), the state and the conflicts of the last round
- [x] A guard against mass deletion in sync: a round that deletes a large
      part of the vault waits for `notes sync confirm` or the buttons in the
      settings; `restore` brings the files back (architecture §9)
- [x] An edit lock (one writer at a time) - groundwork only: a write names
      the version it replaces (`notes_store::sync::Base`, the `x-base` header)
- [x] HTTPS - the reverse proxy (user's decision: nginx is already on the
      VPS); how to deploy - `docs/server.md`
- [x] One-command update of the hub on the VPS: `tools/deploy-hub.sh` (the VPS
      builds, a root helper installs; `docs/server.md`, "Updating")
- [x] The hub on the VPS (`docs/server.md`): the API on a subdomain, the
      main name shows the front page `landing/` (user's decision; a first
      version, to polish)

## M5. Desktop - Tauri with the core inside

- [x] Research E7 (`docs/research/E7.md`): WebKitGTK renders notes like
      Chromium (small differences - below), the interface through the
      `notes://` scheme and the same `Router` works fully; SSE through the
      scheme is impossible
- [x] Parts (user's decision, architecture §1): the thin `notes` calls
      `notes-typst` from its folder and passes the version; light commands
      run in `notes` itself
- [x] Events by long polling instead of SSE (one way for the browser and the
      window); `notes serve --socket` - a Unix socket without a token, the
      service listens on it too
- [x] `notes-app`: a Tauri 2 window without Typst, its own address scheme ->
      the core socket; the core is already running (service) - connect, if
      not - start its own (closing the window and SIGTERM stop it); external
      links and PDF go to system programs
- [x] Desktop instead of the browser (user's decision): the app is the
      window - the menu entry or `notes app [--vault <name> [<note>]]`; README,
      `docs/writing.md`, `notes new` and the skill point to it; `notes serve`
      and `notes service` stay for the browser version and self-hosting
      (`tools/install.sh` does not install the service)
- [x] The skill without the service (user's decision): it tells the user the
      `notes app` command, looks at the PNG pages and, with a browser tool,
      at the page of its own temporary `notes serve`
- [x] Windows (user's decision): one app, a second launch goes to it; the
      setting "another vault - in this window or a new one" (this one by
      default); right click on a vault - both choices; Ctrl+click - a new
      window; a vault already open in another window - switch to it
- [x] A `WebKitWebProcess` crash when closing the window (user's task): the
      window kills its WebKit process before exit (tech debt "WebKitGTK in
      the window")
- [x] Page jitter in the window (touchpad, screen scale 1.25): the document
      does not scroll, the page column does (`lib/scroll.ts`) - panels and
      the contents stay in place
- [x] The window opens maximized or fullscreen if it was so when closed
      (user's task; `tauri-plugin-window-state`, size and position are not
      remembered)
- [x] WebKitGTK differences (E7): the gap after an operator with limits or
      scripts (∑, ∫, lim) - the pass `passes/operators.rs` puts it after the
      whole construct for every engine; interface `<select>` and the graph
      checkbox have their own look, the same in both engines; `baluk.css`
      goes through lightningcss with prefixes; no elastic page bounce
- [x] Installation - `tools/install.sh` (parts, a menu entry and icon), later
      a PKGBUILD (user's decision)
- [x] The app icon (user's choice of 7 variants): an italic "b" and lines of
      text; it is also the site favicon
- [x] A vault copy + sync, the server address and sign-in in settings (M4)

## M6. Android - Tauri with the core inside

- [ ] The same app on Android; tabs and the graph on a phone (tapping a node -
      the user decides)
- [ ] Speed on a phone: first the theme being read, the second after showing
      (E1); the warm-up does not disturb reading

## M7. Browser - the core in WASM

- [x] Research E4 (PR #14): `notes-core` for `wasm32` (no threads, `notify`, `std::fs` -
      `Storage` in OPFS/IndexedDB), build time and memory of the demo and a
      large book in Chromium and on a phone, the weight of fonts and packages;
      report - `docs/research/E4.md`
- [x] The core in a Web Worker, the API transport - messages to the worker:
      `notes-wasm` serves the `notes-server` `Router` (PR #21), the client's
      worker transport (PR #22), the vault and settings kept in OPFS (PR #23)
- [x] Sync from the hub in the worker (PR #28): the `notes-store` sync engine
      over synchronous `XMLHttpRequest`, the state and the removed files in
      OPFS
- [x] The hub serves the browser version (`notes hub serve --web`, PR #24):
      the client, the module, the packages, the theme CSS and the fonts as
      static files built on the computer (`tools/web-bundle.sh`); sign-in and
      the vault list go to the hub, the worker runs the tab's vault (PR #26)
- [x] Fonts fetched apart from the module (PR #30): the module 13.4 -> 8.0 MB
      brotli, the fonts are files the browser caches for good
- [x] Restart the worker after a panic or above a memory threshold (PR #29)

## The development cycle as a kit

User's decisions (2026-10-23): the development cycle becomes a kit that will
later move to its own repository; it is made less tied to this project, and a
project adapts it with a thin layer of its own. Order: after `e2e-parallel`
is merged, one task at a time.

- [~] Test speed: cargo-test binaries at the same time (PR #33), e2e on
      several Playwright workers with a server each (PR #32); heavy steps use
      about 5/6 of the CPUs (all at 1-2) with a raised nice
- [ ] The kit in `pipeline/`, a Claude Code plugin: the agents, the skills
      (`dev-workflow`, `forge`, `sdd`), the mod, the generic scripts (targeted
      tests by changed paths, the full check runner with its locks, CPU cap,
      nice, step table and CI summary, a speed measurement script), the
      Forgejo part (workflow template, runner setup, agent accounts),
      templates and the cycle's docs. No project details in it
- [ ] The project layer: `.claude/pipeline.toml` (full check steps, the map
      of paths to targeted tests, the forge address, what runs after a merge,
      where the documents are); the project's own steps stay in `tools/`, its
      code rules in `.claude/rules/`
- [ ] Spec-driven development, the spec-anchored level: living behaviour
      specs `docs/specs/<area>.md` with stable requirement ids; a task's brief
      carries the spec delta, the developer applies it in the same pull
      request as the code, the reviewer checks against the ids; specs grow
      as areas are touched, plus a first task for sync, paths and the HTTP
      API. The brief itself is not kept. Own light Markdown format
- [ ] The user approves tasks (or autonomous work) in the chat; the full
      descriptions are there to read at will
- [~] Code skills: a language-neutral `code-quality` skill (comments must
      prove they are needed; up to 1000-1500 tokens) preloaded in every
      developer and the reviewer; the Rust skill rewritten for agents,
      portable, no emoji (done, PR #34). Then one mechanism in the mod: an edit of a file
      whose language has a skill in `.claude/pipeline.toml` needs that skill
      loaded (replaces `.claude/hooks/require-rust-skill.py`)
- [ ] The main agent is renamed from `liaison` to `lead`
- [ ] The mod: a cancelled check (a machine shutdown) is re-run and costs no
      round; at start it reconciles dead agents, their unsaved work and their
      worktrees. It does not pile up tasks: `pipeline_status` shows the live
      tasks, the last 3 merged ones and a count of the rest (all with a
      parameter); a merged task is dropped from the mod's state after 14
      days, a dropped one right after its cleanup; a blocked one stays and is
      marked stale after 14 days
- [ ] What makes the shared `target/` reach its 30 GB limit
- [ ] A task that measures has a budget (a number of runs, or stop at the
      target); such tasks run one after another, not side by side
- [ ] After `e2e-parallel`: re-wrap the long lines in `docs/tech-debt.md`
      ("tools/check.sh is sequential") and `docs/architecture.md`, and drop
      the history parenthesis from `.claude/rules/tools.md` "CPU cap"

## Later

- Spec-driven development (user's plan): every task is written to a file in
  the repository (spec, acceptance criteria, files) that the developer reads;
  the liaison writes the specs, the briefs point to them. How the files are
  laid out and when they are removed - to decide when it starts

- Public notes (to think through): a note opens by a link for everyone, in the
  full client (not a static site); it is visible which ones are public. Who
  builds HTML for a reader without the app - to decide (a server without
  Typst; the owner's core or WASM at the reader). Download - PDF
- Many users: accounts, a vault per user, a database behind `Storage`
- Version history, backups, algorithm animation, running code, encryption,
  iOS
- An alphabet of file names (user's decision; the mechanism is renaming from
  the interface, `notes-core::rename`): allowed characters - Latin, Cyrillic,
  some special characters; some special characters in a file name are
  replaced by words, but stay as they are in the title. Which characters and
  which words - decide with the user; length up to 80 letters and "Name 2"
  for a taken one - as now. The alphabet is a rule for all paths (user's
  decision): together with it - a one-time migration, existing files and
  folders of vaults with other characters are renamed by the same mechanism
  as renaming (`#see` links are rewritten, with a list and confirmation);
  after it `NoteId` accepts no other names. The fixtures `tests/vault/Имена/`
  (`C++ и C#`, `50% готово`) - redo under the rule
- Real formulas in titles (as in the contents) in the tree, tabs, graph - if
  needed: requires built titles of all notes
- Build limits (time and memory limits: a hanging note is an error, not a
  hanging device) - postponed (user's decision: no option was liked). Typst
  cannot interrupt a build; considered: checks inside Typst (copies of two of
  its crates), building in a separate process (the Typst cache is lost, not
  possible on Android and in WASM), a watchdog without Typst changes (the
  build keeps burning CPU, overspent memory - a server restart). For now
  Typst's built-in limits cover it: `while` - 10 000 steps, call depth - 80
- A built-in editor - very low priority

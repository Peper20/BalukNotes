# Tech debt

What is postponed or simplified: **what** - why so - the risk - how to close
it. Extended at the end of every iteration, closed items are deleted.

## Rendering

- **Figure processing parses SVG text** (`figures.rs`, `frames.rs`), not a
  tree: SVG exists only as `typst-svg` output, the code relies on its format
  (tag by tag, relative path commands `m l h v c q a Z`). Anything unclear is
  left as is, but a new Typst version may silently turn off the compression.
  How to close: on updates watch the test of the number of merged figures;
  ideally - our own frame output.
- **The `data-k-figs` tag is a hash of the color table**: a note body without
  the `<style>` from `styles` is left without figure colors. The client must
  always insert `styles` together with `body`.
- **Every theme is a full compilation** (in parallel): twice the CPU work. By
  E1 95-99 % of the time is figures, they have to be built per theme anyway;
  "text once" is not done. On a phone parallelism will not help - a lazy
  second theme (roadmap, M6).
- **The first build of a large book takes seconds** ("Матан" 7.4 s in
  release, almost all of it CeTZ figures). After a restart - the disk cache;
  after a chapter edit the Typst memo provides unchanged figures (0.05-0.17 s
  instead of 2.6-6.8 s, E2). Slow remains an edit while the server was not
  running, and after 10 builds of other notes (`comemo::evict(10)` in
  `world.rs`). How to close: evict the memo more gently (by memory, keep the
  open note longer), the number of builds as a device setting.
- **Book chapters are cut from the HTML text** (`book.rs`): there is no tree
  after figure processing. The code relies on `typst-html` closing tags and
  escaping `<`; anything unclear - the whole book.
- **Browser search and printing see one chapter of a book**: Ctrl+F is the
  palette search (by book, chapter, vault), the browser one is a second
  Ctrl+F; printing - via PDF.
- **The heading HTML for the contents is cut from the page text**
  (`passes/heading_html.rs`): `typst-html` does not serialize a single element.
  The code relies on the form `<hN ... id="...">`; links and figures in a
  heading become text; if the format changes the contents silently fall back
  to text. How to close: subtree serialization if Typst exposes it.
- **Brackets in formulas are a heuristic**: `stretchy="false"` if there are
  no fractions, roots, limits, matrices between them (`passes/fences.rs`,
  `TALL`). Something tall not on the list gets unstretched brackets. How to
  close: extend the list; ideally - a fix in Typst.
- **The gap after an operator with limits is moved by a pass**
  (`passes/operators.rs`): WebKit keeps the `rspace` of the base `mo` between
  the operator and its limits, so the pass zeroes it and adds an `mspace`
  after the construct (also in Chromium - the look does not change). Only
  `em` spaces and parents with a free child count (`math`, `mrow`, `mtd`,
  `msqrt`); the large operator list is in the pass. Left in WebKit: `∬`/`∮`
  display glyphs are ~8 px narrower, stretchy brackets and matrices differ in
  width (display formulas shift by 4-5 px); number inputs in settings have a
  spin button.
- **Anchor separators are a list** (`render::SEPARATORS`: spaces and common
  punctuation); the rest stays in the `id`, including rare punctuation and
  formula symbols in a heading. How to close: extend the list.
- **The class `k-h` is used by both baluk headings and the `#h` spacing**
  (`span.k-h`): spacing rules apply only to `span.k-h`. Rename the spacing all
  at once (library, CSS, snapshots).
- **`h`/`v` in HTML are only absolute**: `pt`, `em` and a sum become an empty
  `span.k-h`/`div.k-v`; `1fr` and percents disappear with a warning,
  `weak: true` is ignored. The fallback recognizes formulas by the theme's
  math font: an own math font - and `quad` becomes a `span` inside MathML.
- **The typst-html tree changes after introspection** (typst#7951): links
  inside moved figure variants resolve against the base document. There are no
  links inside figures yet.
- **Passes are three lists** (`passes::TREE`, `passes::TEXT`,
  `finish::FINISH`): dependencies between them are not checked (there are
  none now); every tree pass is a separate traversal (fractions of a
  millisecond).
- **Shared frame parts are only whole SVG subtrees** longer than 96 bytes,
  seen twice in a group. Over the network there is almost no gain (gzip
  compressed the repeats), the gain is in memory and browser parsing.

## The `baluk` library

- **Layout dictionaries are only `ru` and `en`**: another language - own
  `words:`, missing words are English. `notes check` sees only a literal
  `lang: "..."` in the template call. How to close: dictionaries as vault
  files (`/_words/de.typ`) if there are many languages.
- **Argument names are not in the API snapshot** (`baluk-api.txt`): Typst does
  not expose them. The skill reference test checks them partly (below).
- **Old names are not supported**: notes on the former Russian API (`#рис`,
  `холст`, `#см`...) do not build, the original notes use it; import is a
  separate tool (user's decision).
- **Interactive figure formula parsing is written twice** (Typst and TS): the
  cross-check catches a divergence only on the formulas in the set. How to
  close: Typst in WASM or points from the server - more expensive; for now -
  extend the set.
- **The formula subset is narrow**: no `if`, `calc.max/min`, own functions,
  parametric curves. A whole figure depending on a parameter - `frames`.
- **The default y/z range comes from the frame at default values**: with
  another value the curve leaves the area. Give `y:`/`z:` explicitly.
- **3D is an orthographic projection and the painter's algorithm by face
  centers**: overlap glitches on strongly curved surfaces, no axes with ticks.
  How to close: a z-buffer in WebGL.
- **Frame size is not aligned automatically**: without an invisible frame
  frames of different sizes shift. How to close: measure frames (`measure`)
  and pad - but a shift of the origin cannot be fixed this way.
- **Frames in PDF are one row without wrapping**: wide ones go past the
  margin. The limit is 60 frames (do not raise until needed - user's
  decision). How to close: a wrapping grid (`columns:`).

## Fonts

- **The math font is not subset** (New Computer Modern Math, CFF with `MATH`):
  `fontcull` handles neither CFF nor the `MATH` closure. Whole it is 669 KB,
  the heaviest file of a page; a subset would be 192 KB (E3). How to close: a
  subset in fontations/klippa when available, or fontTools-cut parts in the
  repository (check stretchy brackets in Chromium, WebKit, Gecko). Not done
  now: optimization is not a goal, the core serves the font locally.
- **Font parts follow character sets, not the page's characters**: Latin is
  loaded whole (~40 KB); kerning between characters of different parts is
  lost. How to close: a subset by the page's characters.
- **Two copies of brotli** (8 - `tower-http` and our CFF encoder, 9 -
  `ttf2woff2`) and a second copy of fontations in `fontcull`: a longer build
  from scratch. How to close: a shared brotli version; klippa directly.
- **The font part cache** (`<data>/cache/fonts`, ~3.6 MB) is cleaned by the
  server at startup (`Fonts::prune_web`): unneeded parts older than the
  foreign cache lifetime (another build shares the directory). The version
  `webfonts::ENCODER` is bumped by hand. The first start on a new machine -
  ~15 s of CPU in the background.
- **Fonts from collections (`.ttc`) are not served.** Not needed now.
- **New Computer Modern glyphs are wider than their advance** (☼, circled
  letters: the drawing of Ⓐ spans -219 to 825 with an advance of 613): they
  overlap neighbors both in PDF and in HTML - that is how the font is made
  (user: fine for fallbacks).
- **The CFF text font in the browser is rebuilt**
  (`webfonts::cff_to_truetype`): cubic curves -> quadratic with 0.5 font unit
  precision, no hints - indistinguishable on screen, but it is not the same
  file as in the PDF. The conversion runs on every server start (~30 ms per
  style); the font role (formulas or text) comes from the theme lists, not
  from the file. How to close: not needed while Chrome rejects the CFF of New
  Computer Modern; check on Typst updates.
- **Layout fonts are embedded whole** (4.6 MB) from Debian packages (small
  differences from the JetBrains release are possible); embedded families
  cannot be replaced by system ones (only `--font-path`). How to close:
  official releases and new snapshots - with the user.

## Core and server

- **The rendering code tag is computed from the core sources** (`build.rs`):
  an edit of any `src/` file outside `AFTER_CACHE` (even a comment) means a
  full warm-up. The fonts in the tag are families and the number of styles,
  not the content.
- **The embedded library is not part of note versions** (its files have no
  path on disk): its hash is in the cache tag, a new library - everything
  anew.
- **Only `notes serve` cleans the disk cache** (when the warm-up starts); new
  limits apply from the next cleaning.
- **Device settings live in the same `settings.json`**, the `device` flag
  tells them apart: sync (M4) must take it into account. Phone defaults are
  guessed (roadmap).
- **Changing figure settings reads the raw rendering from disk** ("Матан" -
  ~3 MB of JSON, tens of ms). Debug and release builds (and the installed
  `notes`) share the data directory and overwrite each other's cache - an
  extra warm-up.
- **Server memory idle after warm-up is ~360 MB** (`tests/vault`, the target
  <~300 MB; `docs/research/E5.md`): ~72 MB live on the heap, plus the page
  cache (up to 64 MB), font parts and what jemalloc keeps in its arenas. The
  warm-up peak is ~1.2 GB (two builds at once), on "Демо" (15 notes, figures)
  - ~2 GB, ~520 MB after the pass. On Windows (MSVC) there is no jemalloc -
  memory there is as with glibc. How to close: measure if it gets in the way.
- **The "Папки" graph force was tuned on a synthetic graph** (300 nodes,
  `vault_graph::tests::forces_do_what_they_say`): there is no real vault with
  many folders to check on. How to close: tune `CLUSTER_PULL` and
  `GROUP_REPEL` when such a vault appears.
- **Parallel builds use a pool of file caches** (`world::Stores`): memory up
  to "number of builds" times more. `comemo::evict(N)` after a build cleans
  the other memo too; a large warm-up pass (`evict(0)`) - also the memo of
  the open note.
- **The warm-up and the user share the CPU**: the warm-up thread runs with
  `nice` 10 (Linux, Android; `rustix`), but the Typst layout pool is shared,
  without lowering; on other OSes the priority does not change. New notes are
  ordered by source size.
- **An interrupted build continues** (Typst cannot cancel): leave a book that
  is building - it finishes. How to close: build cancellation.
- **The watcher exists only in the server and only for the directory**: the
  CLI walks the vault. OS events get lost (inotify overflow, network drives,
  a file in a just created folder), so the index and the warm-up walk the
  vault at least every 10 minutes (`graph::MAX_AGE`, `warm::RESCAN`); a
  broken watcher (overflow - `Rescan`) is turned off, the events response is
  `watching: false`, the client stops waiting and refreshes by a button (no
  polling). The library directory on disk is watched as a second source
  (`Changes::also`), its failure is a warning; a library edit makes all notes
  stale. After a change the index is walked whole, not by the paths from the
  event. For tens of thousands of notes - update by paths.
- **Computed links come from the last successful build** (`Record.links`):
  until a rebuild deleted links are still visible; an unbuilt note gives only
  literal `#see("path")`.
- **The version of a note with `#vault-graph` is the graph response**: the
  version check builds the index and takes the layout (from the cache); a link
  edit in a large vault means a new layout already during the version check.
- **Vault data for notes - the providers graph and titles**
  (`vault_data.rs`): the default fingerprint is a hash of the response, a
  cheap freshness token is set by the provider itself (`DataProvider::token`).
  The graph filter is in the file name; the name `_vault` in the vault root is
  taken.
- **PDF and PNG on every request without a cache** ("Матан" ~2 s): a plain
  `typst compile` cannot build the vault. `notes png` compiles the whole note
  even for one page (`--pages`), and the pages are rendered one after another.
- HTML is sanitized (see the architecture): dangerous tags and attributes are
  removed, the CSP forbids inline/eval scripts. Left: refine the allowlist as
  blocks are added to the library, make sure fonts and themes do not need
  `style-src` without `unsafe-inline`.
- `<style>` and `style=` in a note can restyle the whole app UI (spoofing:
  fake dialogs, covering buttons): sanitization cuts XSS but does not limit
  the style scope to the note. How to close: note style isolation
  (scope/shadow/rewriting selectors).
- DOM clobbering via `id`/`name`: a note can set names that conflict with
  global document properties. How to close: the client must not rely on
  `window.<id>` and `document.<name>`, only explicit
  `querySelector`/`getElementById` in its container.
- **No CORS**: `config.ts` has `base`, but the server rejects a foreign
  `Origin`. The core talks to the storage server, not the page - CORS is
  needed only if a page from another origin calls the server itself.
- **Events for the client are "something changed"**: every tab checks the note
  version and the list on any edit (two cheap requests); a tab is one
  connection (HTTP/1.1 has a limit of ~6 per server). For many users - events
  per note.
- **The package whitelist is only CeTZ 0.4.2 and oxifmt 1.0.0** (what the
  library pulls in): the rest - via `device.packages`. How to close: review
  the list when notes need a package. Changing the allowed ones rebuilds all
  notes with the library (it reads CeTZ).
- **A vault closes after 10 minutes without requests** (`IDLE_CLOSE`) if it
  is not active and has no event stream: reopening is cheap (fonts and themes
  are shared), but pages then come from disk, not memory. The period is a
  constant, not a setting.
- **The active (warmed) vault is the open one or the one that last hinted the
  warm-up**: two tabs with different vaults pull the warm-up back and forth.
  What is skipped is built on request.
- **Deleting goes only to the system trash**: there is none on Android and in
  WASM (an error); restoring is only from the Dolphin trash; the folder of the
  last deleted note stays (empty - in the tree). With sync (M4) a deletion
  must become a versioned change, otherwise the note comes back from the
  server. How to close: a vault trash `.trash/`.

## Sign-in and sync

- **Accounts are files** (`users.json`, `sessions.json` in the data
  directory): fine for a few users; a `notes users` command and a running
  server may save `sessions.json` at the same moment (a tiny window). A
  session's "last used" on disk lags up to an hour. The pause after a wrong
  password lives in memory (a restart resets it); one wrong password makes
  that login wait ~1 s even with the right one. How to close: a KV database
  with several users.
- **Sign-in in the client is only the form**: a password is changed from the
  console (`notes users passwd`), no "remember me". A client pointed at
  another origin (`__NOTES_API__.base`) cannot sign in: the server has no
  CORS with credentials. `--session-days` is not passed to `notes service
  install`.
- **The hub trusts `X-Forwarded-Proto` and `X-Forwarded-Host`**: right behind
  a reverse proxy on localhost, wrong if the port is open to the network (the
  `Origin` check compares host and port, not the scheme).
- **All accounts of `notes serve --auth` see the same vaults** (one data
  directory); only the hub keeps vaults per account.
- **Hub vaults**: tombstones and `history/` are kept forever, there is no API
  to read the history; open vaults are never closed, the vault list opens
  (rescans) all of them; a file edited on the server by hand is noticed on the
  next start or on a request for that path; `notes users remove` keeps the
  vault files. A file is held whole in memory (limit 64 MiB).
- **Sync sees a change by size and time, then by hash**: an edit that keeps
  both is missed until the next one. Paths are compared byte by byte (case,
  Unicode forms). A failed file operation on the device fails the whole
  round.
- **A deletion is a change like any other**: emptying a linked vault folder by
  hand deletes the files on the server and on other devices (the server keeps
  them in `history/`, a device - 30 days in `sync/removed/`). There is no "too
  many deletions - ask first" guard.
- **Rename or delete of a linked vault in the app unlinks it**: the server
  copy stays under the old name, there is no rename on the server and no
  re-link. Unlinking from the console while `notes serve` runs - its worker
  stops on the next round or state request.
- **Sync in settings shows words only**: a round error is the core's English
  line recognised by its start in `lib/sync.ts` (`roundError`; a new error
  text there needs a rule, otherwise "Не удалось синхронизировать" and the
  line under it), no progress of a large download, conflicts are the file
  names of the last round; the state is read while the settings window is open. The "plain
  http" warning repeats the core's rule in `lib/sync.ts`.
- **Extra sync rounds**: every pulled file and every server echo of an own
  upload wakes the worker for one more cheap round. `.baluk/settings.json`
  (a hidden path for the watcher) goes with the next round, not at once.
  Without a file watcher - a round every 60 s (a constant).
- **`notes-server` pulls the HTTP client** (`ureq`, native TLS) through
  `notes-device`; the hub binary alone builds without it
  (`--no-default-features`).

## Graph

- **The core layout takes ~60 ms per 1000 nodes** (release, first time;
  repeated - cache), measurements - `docs/research/E6.md`. The area per node is
  a constant for an average label: a graph of long labels is cramped
  (spreading - no more than 80 passes), of short ones - roomy. Folders cluster
  because nodes are ordered by path: the initial layout goes in order, a
  "toward own folder" force is not needed.
- **Client physics are springs to the core layout, not core forces**:
  otherwise an untouched graph would drift. A dragged node pushes away only
  those it ran into. The grid (from 200 nodes) has a cell sized by the largest
  node: one long label brings back an almost full scan.
- **Moved nodes are not remembered**: a filter change or reload - the layout
  anew; a button to restore it exists only on `/graph`. Whether to remember -
  the user's taste.
- **Label width is by the number of characters**: the layout is the same
  everywhere, gaps at wide letters are inexact; on a phone labels may overlap;
  the Typst picture of a large graph makes labels tiny.
- **Tapping a node on a phone opens the note at once**, neighbors highlight
  only on hover. The user decides (M6).
- **Without motion (`prefers-reduced-motion`)** neighbors do not follow,
  transitions are instant.
- **Books as chapters only on `/graph`**: the home page and `#vault-graph` in
  a note have no checkbox (user's decision: not needed yet). A chapter vertex
  is `<book>/.<number>`: insert a chapter - the numbers of the following ones
  shift (the chapter's neighbors in the address are another chapter). A link
  the contents do not see (inside `#let`, a computed path) comes from the book
  root. The stiffness of the "book - chapter" spring (`TIGHT_SPRING`) is tuned
  by eye on "Демо" with an added 5-chapter book.

## Client

- **Opened "Ответы" are matched by the section anchor and the `summary` text**
  (`lib/details.ts`), among equal ones - by index: a new block above an opened
  one with the same text opens instead of it.
- **Fuzzy search is greedy** (`lib/fuzzy.ts`): a query letter jumps to the
  start of the next word; if that fails, a second pass takes the first
  occurrences, but the pick is not the best one by score (no full
  backtracking).
- **"Нет связи" depends on a server answer** (`api.onReach`): an error
  response (502 of a reverse proxy, M4) counts as connected. The probe request
  is `GET /api/vaults` every 3 s while disconnected.
- **Only the open vault can be renamed and deleted**: another one - open it
  first. Tabs and reading positions move only in this browser; the disk cache
  is tied to the vault path - after a rename the notes are rebuilt, the
  former cache is deleted as foreign (`device.foreign_days`).
- **Key hints hide on `(pointer: coarse)`**: a tablet with a keyboard does not
  get them either.
- **On a phone tabs only scroll** - no swipe and no list. For Android (M6) -
  rethink.
- **The first-frame theme is browser memory** (`k-theme@<vault>`): a new
  browser or a vault with its own theme opened for the first time - the first
  frame is in the last shown (or system) theme, then it switches.
- **3D by finger: tilting only by a gesture started sideways** (one started
  vertically scrolls the page, `touch-action: pan-y`); no two-finger rotation.
  Decided so (the user left the choice to the agent): scrolling does not get
  stuck on the figure, no extra frames or handles. Other options - a handle
  strip, a tap that "activates" the figure, two-finger rotation - if it turns
  out inconvenient on a phone.
- **Renaming is not a transaction** (`notes-core::rename`): files change one
  by one, a failure in the middle leaves part of the edits. Only literal
  `#see("path")` are rewritten: computed paths and `#import`/`#include` of
  other files by an absolute path are not.
- **Tabs and reading positions of the previous client version** (keys without
  a vault) are not carried over.
- **Another vault - a page reload** (in this window) or a new window (tab) -
  the setting `vaults.open`.

## Vault and tools

- **A title is source text** (`outline.rs`): a formula is shown as source
  (`[Ряд $sum 1/n^2$]` -> "Ряд sum 1/n^2", user's decision), anything computed
  in `title: [...]` is skipped. How to close: title HTML for the client (in
  "Later").
- **The `id` of a section with a formula in the index differs from the page**
  (`search::section_ids`): on the page the slug comes from MathML text
  (`Пространство-ℝ𝑛-...`), in the index - from the source. Search leads into
  such a section without scrolling to it; "Ссылаются сюда" finds the heading
  by the link anchor (`id` or the page slug), otherwise shows the text. How to
  close: section `id`s from the built page (cache) in the index.
- **Chapter tags are only literal** (`outline.rs`): `chapter.with(tags: ...)`
  is parsed from the source, as `note.with`; tags from a variable or computed
  ones are not seen by the index (they are on the page). A link preview into a
  chapter shows the book root tags, not the chapter's. How to close: tags of
  the built page per chapter (`ul.k-chapter-tags`) and a per-chapter preview.
- **A folder title is only `_folder.toml`**: it is not read in a book folder;
  there is no `notes` command for it (the interface - renaming).
- **The link index labels a `#see` without text by the file name** (text for
  search), while the page uses the title: a search by link text finds the file
  name. How to close: substitute titles when building the index (a second
  pass).
- **A file name from a title is not Unicode-normalized**: "й" of two code
  points (macOS) and of one are different names. How to close: NFC on
  creation and comparison.
- **`/_vault/title/...` without a watcher walks the vault** for every title
  (`notes check`: 10 links - 10 `stat` walks, tens of ms). How to close: a
  walk cache for the duration of a command.
- **User vaults are outside git and have no copies**: losing the directory is
  losing the notes. How to close: a git repository for the vault or sync (M4).
- **The e2e warm-up adds 20 s to a run** (`e2e/global-setup.ts`) for the sake
  of short waits; `01-switch` checks a repeated long build, not the first.
- **`tools/check.sh` is sequential** (~3.5 min with e2e): `cargo test` ~35 s
  (crate binaries run in turn, the longest is the core unit tests, ~13 s),
  e2e ~80 s (20 s of it is the warm-up). Rebuilding tests after a core edit
  takes ~3 s (end-to-end tests are one binary per crate), the release in
  `install.sh` ~35 s. A new dependency or feature changes the feature set of
  shared crates - the Typst stack is rebuilt in all profiles (minutes, once).
  What else could speed it up: e2e already has a warm-up and one worker (a
  shared server).
- **The installed `notes` and skill are a snapshot at `tools/install.sh`
  time**: without reinstalling they lag behind the repository; an open window
  (its core) and a `notes serve` started by hand run the old version until a
  restart. How to close: an app with updates (PKGBUILD).
- **Parts compare only the version number** (`notes::check_version`, the
  workspace version): two builds of one version from different commits are
  not told apart. While `tools/install.sh` installs the parts together - no
  problem; with releases (PKGBUILD) - bump the version.
- **The core socket is Unix only** (`notes serve --socket`): on Windows the
  window needs a named pipe or a port with a token. The socket of a crashed
  server stays as a file until the next start (which removes it if nobody
  answers).
- **The `notes-app` window is Linux only**: a Unix socket, `xdg-open` for
  links and PDF. A window killed by SIGKILL leaves its core running (the next
  window connects to it); the core crashed with the window open - 502
  answers ("нет связи"), the window does not start a new core. A PDF with a
  build error is only a line in the window log. The page memory (tabs,
  reading positions) of the window is its own, not the browser's.
- **One app for all launches** (`notes-app` `instance.rs`): a second launch
  with another `--data` or `--vault <dir>` is handed to the running app, which
  ignores those flags (its core does not serve that vault - an error page);
  the launch waits up to 40 s for the answer while the first app starts its
  core. Which vault a window shows comes from its address: only real
  navigations are intercepted, not the client's `pushState` (the client does
  not switch vaults that way). Taking the focus on Wayland needs the
  activation token of the launcher (`XDG_ACTIVATION_TOKEN`): a launch from a
  terminal without it only asks for attention. Maximized/fullscreen is
  remembered per window label (`main`, `w2`...).
- **WebKitGTK in the window: memory and a crash on exit.** For the user, after
  11 minutes of work the window processes reached 1.7 GB (walking all "Демо"
  notes - WebKit ~550 MB without growth, 40 drags of a graph node - without
  growth). `WebKitWebProcess`, exiting by itself, crashed in `exit()`: the
  destructor of `libEGL.so.1` (libglvnd; both Mesa and NVIDIA are loaded)
  tears down EGL while the WebKit render thread is still in `libEGL_mesa`
  (WebKitGTK 2.52.6) - the system writes the dump for about a minute, the
  machine lags. Workaround: the window kills its WebKit process before exit
  (`notes-app` `quit`, SIGKILL from WebKit) - so `exit()` never runs there
  (the user closed the window - no crash report). In a test compositor
  (`kwin_wayland --virtual`) the crash does not reproduce; such a compositor
  only inside `dbus-run-session`: on the user's bus it turns off KWin global
  shortcuts (Alt+Tab) after exit. The WebKit bug remains: a report to WebKit
  if it can be reproduced.
- **`glib 0.18` with a vulnerability (Dependabot #1)**: it comes via the
  window (`tauri` -> `wry`/`tao` -> `gtk 0.18`); the fix is in `glib 0.20`,
  and `gtk 0.18` is the last GTK3 binding, Tauri 2 on Linux uses it. No
  dependency calls the vulnerable `VariantStrIter`
  (`Variant::array_iter_str`). How to close: update when Tauri leaves GTK3;
  until then recheck on Tauri updates (`cargo tree -i glib`).
- **Autostart of `notes serve` is only a systemd user service** (`notes
  service`, for self-hosting; the window does not need it): no macOS or
  Windows; the service lives while the user is logged in (without `loginctl
  enable-linger`). The unit runs the binary `install` was called from (a
  debug one - with a warning).
- **The `comemo` copy with a patch** (`.claude/rules/vendor.md`): on a Typst
  update - move the patch or remove the copy.
- **The `notes` config file has one key** (`data`, an unknown one is an
  error).
- **The hint "the path starts with the vault directory name"** in `notes new`
  is a heuristic on the first segment; it does not catch other path mistakes.
- **`target/` is cleaned whole** (`check.sh`, over 30 GB): after a cleaning
  the first check builds everything again (~10 min). How to close: remove only
  old variants (`cargo-sweep`) if full rebuilds get in the way.

## The `/baluk-note` skill

- **The skill looks at the page through a browser tool** on its own temporary
  `notes serve` (port 8439, next to the window's core: they share the disk
  cache without locking, as the CLI always did); without a browser tool - only
  the PNG pages. Stopping that server (`kill <PID>`) is not in
  `allowed-tools` - Claude Code may ask the user.
- **`writing.md` and `SKILL.md` are checked against the library only
  partly**: the test checks only `#name` calls; names without `#` and the
  meaning of rules go stale unnoticed. Of the `SKILL.md` snippets only the
  "Common calls" block is built; the header in the "The file" section is
  rewritten from `new_note` by hand.
- **The reference (`reference.md`) is checked only by named parameters**: the
  test finds `#let name(...)` in `baluk/` as text and compares names and
  defaults; positional arguments, descriptions and allowed strings are caught
  only by building the samples. Signatures are described twice:
  `baluk/README.md` and `reference.md`. How to close: generate `reference.md`
  from `///` comments at installation.
- **Content rules live in two places**: `docs/writing.md` and `SKILL.md`
  ("Writing a good note"). Edit a rule - edit both. How to close: one source,
  `SKILL.md` refers to `notes docs writing` or includes it at installation.
- **The skill is made for Claude Code**: `allowed-tools`,
  `disable-model-invocation`, "Base directory" are its concepts; other shells
  (opencode) read `SKILL.md` as text, the path to `examples/` comes from "Base
  directory" if the shell writes it.

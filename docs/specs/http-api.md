# HTTP API of the local server

What `notes serve` answers: the vault API under `/api/vaults/{vault}/...`, the
shared settings, themes and fonts, the client files, the sign-in of the TCP
address and the sync of this device. Code: `crates/notes-server/src/`. The
sign-in routes themselves and the storage server's `/api/sync/...` are
specified in `docs/specs/sync.md`; note ids and vault names in
`docs/specs/paths.md`. The browser build answers the same API without the
client files, sign-in and device sync.

Next id: API-45

## Conventions

- **API-1** An error answered by a handler is JSON `{ "error": text, "errors":
  [] }` with the status: 400 for an invalid note id, vault name, setting or
  rename; 404 for an unknown vault, note or folder; 409 for a vault that
  already exists; 500 for anything else (the text is the error message). The
  `errors` list carries compile diagnostics only in the 422 of a PDF (API-23).
- **API-2** A request body or query string that the framework cannot read is
  refused with a plain-text body, not the JSON of API-1: 415 for a body
  without the JSON content type, 400 for broken JSON or a missing query
  parameter, 422 for JSON of the wrong shape.
- **API-3** A note id in a URL (`notes`, `version`, `links`, `preview`, `pdf`,
  the `note` parameter of `search`) follows the id rules (`docs/specs/paths.md`,
  PATH-2 to PATH-4): an invalid id is 400 (`..%2Fx`, `_baluk/lib`), a valid id
  of a note that does not exist is 404.
- **API-4** A vault name in a URL that is not a vault of the server is 404,
  also when it breaks the name rules (`..%2Fx`); the message lists the vaults
  that exist. A vault is opened by the first request that names it.

## Vaults

- **API-5** `GET /api/vaults` answers 200 `{ "vaults": [names, sorted],
  "can_create": bool }`. There is no default vault, and a data directory
  without vaults gives an empty list and creates nothing.
- **API-6** `POST /api/vaults` with `{ "name" }` creates an empty vault and
  answers 201 with the list of API-5. An invalid name is 400, a name that
  exists is 409.
- **API-7** `PATCH /api/vaults/{vault}` with `{ "name" }` renames the vault and
  answers 200 with the list. The old name is 404 from then on, the notes and
  the vault's own settings are under the new name. An unknown vault is 404, an
  invalid new name 400, a name of another vault 409.
- **API-8** `DELETE /api/vaults/{vault}` moves the whole vault folder to the
  system trash and answers 200 with the list. A second delete is 404.
- **API-9** A server started on one folder (`notes serve --vault <path>`) lists
  that vault only with `can_create` false, answers 403 to `POST`, `PATCH` and
  `DELETE` on `/api/vaults`, and 404 for any other vault name.

## Notes

- **API-10** `GET .../notes` answers the notes and books: an array of `{ id,
  kind ("note" or "book"), name, folder, title, tags, chapters }`. The title is
  the one in the file, otherwise the name; for a book the tags are those of its
  `main.typ` and `chapters` lists the chapters that have tags of their own.
- **API-11** `GET .../folders` answers `[{ path, title }]` sorted by path: the
  vault folders of PATH-9, with the title of `_folder.toml`, otherwise (also
  when that file is broken) the folder name. A book is not a folder.
- **API-12** `GET .../notes/{id}` answers 200 with the page: `{ id, kind,
  version, rendered, errors, warnings, book }`. A note that does not compile
  is still 200: `errors` lists the diagnostics and `rendered` holds the last
  good rendering, or null if there was none.
- **API-13** `GET .../notes/{id}?chapter=N` or `?anchor=X` on a book answers
  one chapter in `rendered` and the contents of the whole book in `book`; the
  `version` and the headings are those of the whole book, an unknown anchor
  gives the first chapter (with the title page). Anything that is not a book is
  answered whole, with or without these parameters.
- **API-14** `DELETE .../notes/{id}` moves a note's file, or a book's whole
  folder, to the trash and answers 204. The note list is up to date at once. An
  unknown note is 404.
- **API-15** `DELETE .../folders/{path}` moves a directory of the vault with
  everything in it to the trash and answers 204. A path that is not a directory
  is 404, an invalid path 400.
- **API-16** `POST .../rename` with `{ kind ("note" or "folder"), id, title,
  apply (default false) }` answers 200 with the plan `{ kind, from, to, title,
  links: [{ note, count }] }`: the new path (PATH-18, PATH-19) and which other
  notes get their `#see` links rewritten. Without `apply` nothing changes; with
  it the file or folder is moved, the title is set and the links are rewritten.
  A blank title or a folder that is a book is 400.
- **API-17** `GET .../version/{id}` answers `{ "version" }`, the version of the
  note as in its page, without building a note that is already built.
- **API-18** `GET .../links/{id}` answers `{ outgoing: [{ target, anchor,
  exists }], backlinks: [{ from, anchor, ... }] }`. An unknown note is 404.
- **API-19** `GET .../graph` answers `{ nodes, edges }` for the whole vault; a
  link target that is not a note is a node with a null `kind`.
- **API-20** `POST .../graph/layout` with a filter answers the filtered nodes
  with their coordinates and groups, and the `center` note.
- **API-21** `GET .../search?q=&limit=&note=` answers the sections that match,
  best first, at most `limit` (default 30, at most 200). With `note` only that
  note or book is searched, every section in text order; an unknown note is
  404.
- **API-22** `GET .../preview/{id}?anchor=` answers the title, the tags and the
  start of the text of the note, or of the section the anchor names (`heading`
  is null when it names none). An unknown note is 404.
- **API-23** `GET .../pdf/{id}?theme=` answers 200 `application/pdf` with
  `Content-Disposition: inline; filename*=UTF-8''<last segment>.pdf`
  (percent-encoded). The default theme is the first one. An unknown theme is
  400, a note that does not compile is 422 with its diagnostics in `errors`.
- **API-24** `POST .../warm` with `{ "ids": [..] }` answers 204 and makes the
  vault the active one; ids that are invalid or unknown are skipped.

## Events

- **API-25** `GET .../events` without `after` answers at once `{ watching, seq,
  changes: [] }`: whether the server watches the files and the number of the
  last change.
- **API-26** `GET .../events?after=N` answers at once with the changes after
  `N` (each `{ seq, paths }`, the changed vault files) if there are any. If not
  and the server watches the files, it waits for the first change and answers
  with it, or answers with no changes after 25 seconds, or when the server
  stops.
- **API-27** An `after` ahead of the last change (the server restarted) or
  older than the 64 changes the server keeps is answered with one change with
  the latest `seq` and empty `paths`: "check everything".
- **API-28** When the server cannot watch the files, or loses changes,
  `watching` is false, the waiting requests are answered with "check
  everything", and later requests are answered at once.

## Settings, themes and fonts

- **API-29** `GET /api/settings` answers `{ schema, values }`. `PUT
  /api/settings` with an object of changes answers 200 with all the values; an
  unknown key or a value the schema does not accept is 400.
- **API-30** `GET .../settings` answers `{ schema, values, shared, own }`:
  `values` is `shared` with `own` on top. `PUT .../settings` sets own values
  and answers the same; a `null` value removes the own one, so the shared one
  shows again; a device setting (`device.*`) is 400.
- **API-31** When the vault's settings file cannot be read, `GET` and `PUT
  .../settings` carry `problem` (path, message, line, column); changes are kept
  in memory and the file is not overwritten.
- **API-32** `GET /api/themes` answers the themes `[{ name, title, dark }]`,
  `GET /api/themes.css` their CSS variables and `GET /api/fonts.css` the
  `@font-face` rules of the design fonts, one per part with a `unicode-range`.
  `GET /fonts/{family}/{style}/{part}.woff2` answers 200 `font/woff2`; an
  unknown family or part is 404. None of them has a vault in the path.

## Client and transport

- **API-33** `GET /`, `/v/{vault}`, `/v/{vault}/n/{id}`, `/v/{vault}/f/{path}`,
  `/v/{vault}/graph`, `/v/{vault}/tags[/{tag}]` and the same without
  `/v/{vault}` (`/n/{id}`, `/graph`, `/tags[/{tag}]`) answer the same client
  page, with a `Content-Security-Policy` and `Cache-Control: no-cache`.
  When the client is not built, they answer 503 with a page that says how to
  build it.
- **API-34** `GET /assets/{path}` answers a file of the client build, or 404. A
  file with a hash in its name is `public, max-age=31536000, immutable`, any
  other `no-cache`.
- **API-35** Responses are compressed with brotli or gzip, as the request's
  `Accept-Encoding` allows, except WOFF2 fonts.

## Sign-in of the TCP address

- **API-36** With sign-in on (`notes serve --auth`) a request on the TCP
  address needs a session, else it is 401 `{ "error": "sign-in required",
  "errors": [] }`. Open without one: `GET` and `HEAD` of the client pages of
  API-33, of `/assets/...` and `/fonts/...`, of `/api/themes`,
  `/api/themes.css` and `/api/fonts.css`; and the sign-in routes
  (`docs/specs/sync.md`, SYNC-39 to SYNC-45). Any other method needs a session.
- **API-37** Without sign-in `GET /api/session` answers 204 and `POST
  /api/login` is 404.
- **API-38** A server that listens on TCP and on a Unix socket needs the
  session on TCP only; the socket is never asked for one.

## Sync of this device

- **API-39** `GET /api/device/sync` answers the status: `{ server, login,
  signed_in, server_error, session_ended, vaults }`. `vaults` is every vault of
  this device, every linked one and (when the storage server answers within a
  few seconds) every vault of the account on it, sorted by name, each `{ name,
  local, remote, linked, state, last_sync, error, report, held }`. `remote` is
  null when the server was not reached, and `server_error` says why. `state` is
  one of `idle`, `syncing`, `error`, `sign-in`, `offline`, `held`.
- **API-40** `POST /api/device/sync/login` with `{ server, login, password }`
  answers 200 `{ server, login, insecure }` and saves the account
  (`docs/specs/sync.md`, SYNC-29). A wrong login or password is 422, never 401;
  an invalid address 400; a server that cannot be reached 502.
- **API-41** `POST /api/device/sync/logout` answers 204; linked vaults stay
  linked.
- **API-42** `POST .../vaults/{vault}/link`, `/now`, `/confirm` and `/restore`
  run what `docs/specs/sync.md` describes (SYNC-31, SYNC-15, SYNC-26, SYNC-27)
  and answer 200 with the report `{ uploaded, downloaded, removed_local,
  removed_remote, conflicts, skipped }`. `POST .../unlink` answers 204.
- **API-43** The errors of these routes are never 401. 409: not signed in, the
  session ended, a round for the vault that is still running after a minute's
  wait, the vault not linked or not on the server, a round held (SYNC-24),
  nothing held to confirm. 404: a
  vault that exists neither on the device nor on the server. 400: an invalid
  vault name. 502: the storage server cannot be reached or refuses.
- **API-44** A server that runs without device sync answers every
  `/api/device/sync...` route with 404 "this server runs without vault sync".
  `notes serve` runs with it.

# Sync

How a device's copy of a vault follows the copy on the storage server (the
hub), and how the hub signs users in. The sync works on file versions, there is
no merge of contents. Code: `crates/notes-store/src/sync/` (the rules and the
engine), `crates/notes-hub/` (the server API and sign-in),
`crates/notes-device/` (the device side). The routes of the local server that
drive the device side are in `docs/specs/http-api.md`.

Next id: SYNC-46

## Hub: vaults

- **SYNC-1** Every `/api/sync/...` route needs a session; without a valid one
  the answer is 401 `{ "error": "sign-in required", "errors": [] }`.
- **SYNC-2** An account sees only its own vaults: the same vault name in two
  accounts is two separate vaults.
- **SYNC-3** `GET /api/sync/vaults` answers the account's vaults `[{ name, seq
  }]` sorted by name. `PUT /api/sync/vaults/{vault}` creates an empty vault and
  answers 201 `{ name, seq: 0 }`, or 200 if it exists. An invalid vault name
  (`docs/specs/paths.md`, PATH-20) is 400. Reading, writing or listing changes
  of a vault that does not exist is 404 and does not create it.

## Hub: files and versions

- **SYNC-4** A file's version is the SHA-256 of its content, in lowercase hex.
  Every change of a file (new content, deletion) gets the next change number
  `seq` of the vault, counted from 1 (a new vault is at 0). A deleted file
  stays as an entry with a null `hash` and size 0, a tombstone, for good.
  Entries are `{ path, hash, size, seq }`.
- **SYNC-5** `GET .../{vault}/files/{path}` answers 200 with the bytes
  (`application/octet-stream`) and the headers `X-Hash` (the version) and
  `X-Seq`. A deleted file or one the server never had is 404.
- **SYNC-6** `PUT .../{vault}/files/{path}` writes the request body. The header
  `X-Base` is required and says which version is replaced: `any` (no check),
  `absent` (the file must not exist, a tombstone counts as not existing) or a
  64-digit lowercase hex hash (exactly that version). When the base matches,
  the answer is 200 with the new entry. When it does not, the answer is 409
  `{ "current": entry }` (`null` for a path the server never knew) and nothing
  changes. A missing or malformed `X-Base` is 400. Content identical to the
  current version is not a new version: the answer is 200 with the current
  entry and no number is spent, whatever the base.
- **SYNC-7** `DELETE .../{vault}/files/{path}` with `X-Base` checks the base as
  in SYNC-6 and answers 200 with the tombstone entry, or 409 with the current
  entry. Deleting a file that is already gone, with `absent` or `any`, is a
  success that changes nothing.
- **SYNC-8** Content that a write replaced or a delete removed is moved to
  `history/<seq>/<path>` in the vault's folder on the server, so nothing a
  device pushed is lost there. No route reads the history.
- **SYNC-9** A synced path is relative to the vault, `/`-separated, UTF-8, at
  most 1024 bytes with every segment at most 255 bytes, with no empty, `.` or
  `..` segment, no backslash and no control character. Not synced: anything in
  a `.git` or `.trash` folder (at any depth), a name ending in `.tmp`,
  `.DS_Store`, `Thumbs.db`, and everything in the `.baluk/` folder at the
  vault root except `.baluk/settings.json` (a `.baluk` folder deeper down is
  synced). Names starting with `_` are synced (`_folder.toml`).
  The hub answers 400 to a read, write or delete of any other path; a device
  never lists or sends such a path, and ignores such an entry from the server.
- **SYNC-10** A path cannot be a file and a folder at once: writing `dir` while
  `dir/a.typ` is a live file, or `dir/a.typ/b.typ` while `dir/a.typ` is one, is
  400. After the file is deleted the name is free.
- **SYNC-11** A file is at most 64 MiB. A larger write is 413 and writes
  nothing, and so is a body far over the limit. A device does not send a larger
  local file, does not take a larger entry from the server, and does not treat
  either as a deletion; both are listed in the round's `skipped`.
- **SYNC-12** `GET .../{vault}/changes?after=N` answers `{ seq, entries }`: the
  vault's current `seq` and the entries whose `seq` is greater than `N` (zero
  when omitted), ordered by `seq`, one per path (its latest version, tombstones
  included).
- **SYNC-13** With `wait=S` (seconds, at most 30) and no entry after `N`,
  `changes` waits for a write to that vault and answers with it; after `S`
  seconds, or when the server stops, it answers with no entries. If there are
  entries it answers at once.
- **SYNC-14** The hub compares the files in its folder with its records when it
  opens a vault (on the first request after the server starts) and, for one
  path, when that path is read, written or deleted. A file edited by hand in the
  folder gets a new `seq` then, a file removed by hand becomes a tombstone, a
  file only touched does not change. `changes` alone does not look at the
  disk.

## Device: a round

- **SYNC-15** A device keeps a full copy of the vault and the version of every
  file as of its last round. A round compares, per file, that record with the
  device's file and the server's. Changed only on the server: downloaded, or
  the local file removed when the server deleted it. Changed only on the
  device: uploaded with the recorded version as base (`absent` for a new file),
  or deleted on the server with that base. Changed the same way on both sides
  (same content, or both deleted): recorded, nothing transferred. A file that
  was only touched is not a change, and a round reads no file whose size and
  modification time match the record.
- **SYNC-16** A file changed differently on both sides, an edit against a
  deletion included, is a conflict, listed in the round's `conflicts` and
  settled by the device setting `device.sync_prefer`. With `local` the device's
  version is pushed without a base check (or its deletion is) and the server
  keeps the replaced version in its history. With `remote` the server's
  version is taken (or the local file removed) and the device's version is
  moved aside as in SYNC-19. The default is `local` on a computer and `remote`
  on a phone.
- **SYNC-17** When the server refuses an upload or a delete with 409 because
  the file changed there meanwhile, the same round settles the file again as a
  conflict with the server's current version (SYNC-16); the same content on
  both sides is no conflict.
- **SYNC-18** The first round of a vault uses the same rules with an empty
  record: a file equal on both sides is recorded, a file on one side only is
  copied to the other, a file that differs is a conflict. A file deleted
  locally before the first round is downloaded again.
- **SYNC-19** A local file that a round replaces or removes is first moved to
  `<data>/sync/removed/<vault>/<unix time>/<path>`, never deleted. Entries
  older than 30 days are deleted when the app starts.
- **SYNC-20** A local file that changed after the round scanned it is neither
  overwritten nor removed: its path is listed in `skipped` and the next round
  settles it, as a conflict if the server changed it too.
- **SYNC-21** A round that fails midway (the network, a crash) keeps what it
  already did: the next round finishes the work and uploads nothing twice, and
  at every point each file is on at least one side.
- **SYNC-22** When the server's change number is lower than the device's (the
  server was restored from a backup), the device compares with everything the
  server has, keeps its own files and uploads what the server no longer has.
- **SYNC-23** The report of a round counts `uploaded`, `downloaded`,
  `removed_local` and `removed_remote` files and lists the `conflicts` and the
  `skipped` paths.

## Device: the guard against mass deletion

- **SYNC-24** A round plans everything before it changes anything. When the
  plan deletes on the server, or removes from the device, more than 10 files
  and more than 20 % of the files the vault had at the last round, or all of
  them when there are at least two, the round is held: nothing is uploaded,
  downloaded or deleted and the server number does not move. The two sides are
  checked separately. The round ends with the error `sync of "<vault>" is
  paused: N of M files are gone from this device and would be deleted on the
  server` (or `... are gone from the server and would be removed from this
  device`) followed by what to type to go on.
- **SYNC-25** A held round is recorded in the vault's status: `held` has the
  `side` (`server` for files gone from the device, `device` for files gone from
  the server), the `count` and the `total`, and the state is `held`. Every later
  round that plans the same deletions is held again.
- **SYNC-26** `confirm` runs a round that lets through exactly the held set
  (the same side and the same paths). If more or other files are gone by then,
  the round is held again. With nothing held it fails with `nothing of
  "<vault>" waits for a confirmation`.
- **SYNC-27** `restore` runs a round that downloads the files missing on the
  device instead of deleting them on the server. It does not help files gone
  from the server: that round is held again, and the way back is to unlink the
  vault and link it again.
- **SYNC-28** The worker of a held vault stays in the state `held`, looks again
  after 30 seconds (at once after a `confirm` or `restore` made through it),
  does not back off, and writes the hold to the log once per set of files.
  While it waits, the server's changes are not applied.

## Device: account and linking

- **SYNC-29** Signing in takes the server address, a login and a password. An
  address without a scheme means `https://`; `http://` is accepted and marked
  `insecure` unless the host is `localhost` or a loopback address; a trailing
  `/` is dropped. Another scheme, no host, user info, a query, a fragment or
  whitespace is refused. The login saves `{ server, login, token }` in
  `<data>/sync/account.json`, readable by the user only; the password is never
  stored, and a new login replaces the account. A wrong login or password is
  the error `wrong login or password`.
- **SYNC-30** Signing out ends the session on the server as far as it can and
  removes the account file in any case, also when the server cannot be
  reached. Linked vaults stay linked.
- **SYNC-31** Linking a vault runs its first round (SYNC-18) and, when it is
  only on the device, creates it on the server first; when it is only on the
  server, creates the folder `<data>/vaults/<vault>` and downloads into it;
  when it is on both sides, merges. A vault that is nowhere is the error
  `vault "<vault>" exists neither on this device nor on the server`. If the
  first round fails, the vault stays unlinked. Linking a linked vault runs a
  round.
- **SYNC-32** Unlinking forgets the record and the last result of the vault.
  Files stay on both sides and the server keeps its copy. A round for a vault
  that is not linked fails with `vault "<vault>" is not linked: notes sync link
  --vault "<vault>"` and does not link it.
- **SYNC-33** One round runs for a vault at a time, across processes. A second
  one waits up to 60 seconds for the first and then fails with `sync of
  "<vault>" is already running`.
- **SYNC-34** When the server refuses the saved session, a round fails with
  `the session ended: sign in again: notes sync login <server> --login
  <login>`, the status has `session_ended`, and the vault stays linked. The
  worker stops in the state `sign-in`; after a new login it starts again and
  pushes the pending edits.
- **SYNC-35** When the server cannot be reached, a round fails with `cannot
  reach the server <server>: <detail>` and changes nothing on the device. The
  worker's state is `offline` and it retries after 5 seconds, doubling up to 5
  minutes; any other failed round is retried the same way with the state
  `error`. A successful round ends the series.
- **SYNC-36** A running worker (one per linked vault, while the device is signed
  in and `notes serve` runs) runs a round at start, 2 seconds after the last
  change to a synced file, and when the server reports changes the device does
  not have (a long poll, no polling by the clock). When the folder cannot be
  watched it also runs one every 60 seconds. Stopping it does not wait for the
  long poll.
- **SYNC-37** A file that a round writes into the vault is an ordinary file
  change: the vault events of the local server report it (API-25 to API-28).
- **SYNC-38** Renaming or deleting a linked vault stops its worker first, then
  unlinks it, since its folder is no longer where the record says; the server
  keeps its copy under the old name. A rename or delete that fails leaves the
  vault linked and its worker running.

## Sign-in (the hub and `notes serve --auth`)

- **SYNC-39** `POST /api/login` with `{ "login", "password" }` answers 200 `{
  "login", "token" }` and sets the cookie `notes_session=<token>; Path=/;
  HttpOnly; SameSite=Strict; Max-Age=<session lifetime in seconds>`, with
  `; Secure` when the first value of `X-Forwarded-Proto` is `https`. The login
  is not case sensitive and is answered in lowercase; the token is 43
  characters.
- **SYNC-40** A session is accepted from the `notes_session` cookie or from
  `Authorization: Bearer <token>`, never from the URL. `GET /api/session`
  answers `{ "login" }` for a valid session and 401 otherwise.
- **SYNC-41** An unknown login and a wrong password both give 401 `wrong login
  or password` with no cookie. A body that is not `{login, password}` is 400,
  one over 8 KiB is 413.
- **SYNC-42** After a wrong password the same login (not case sensitive) must
  wait before the next attempt, even a right one: 1 second after the first
  failure, then 2, 4, up to 60 seconds. An attempt during the wait is 429 with
  `Retry-After` in seconds. A right password clears the count, the count starts
  over 15 minutes after the last failure, other logins are not slowed, and more
  than 20 failures across all logins within a minute slow everyone down. An
  attempt counts as a failure until its password has been verified.
- **SYNC-43** A session lives 30 days after its last use (`--session-days`),
  and every use extends it. `POST /api/logout` ends the sessions the request
  presents, clears the cookie (`Max-Age=0`) and answers 204, also without a
  session; the user's other sessions stay.
- **SYNC-44** Accounts and sessions changed by `notes users` (a new account, a
  new password, a removed user) apply to the running server without a restart;
  a changed password or a removed user ends all of that user's sessions.
- **SYNC-45** A request that is not `GET`, `HEAD` or `OPTIONS` and carries an
  `Origin` whose host differs from the `X-Forwarded-Host` (its first value) or
  `Host` of the request is 403, also `POST /api/login`. A request without
  `Origin` passes.

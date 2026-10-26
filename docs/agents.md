# The development pipeline of baluk notes

The development cycle (roles, the path of a task, rounds, the mod) is the kit in
`pipeline/`: `pipeline/docs/README.md`. This is this project's side of it: the
server, the accounts, how it is launched, the limits, and what the agents need
to know about the project. The project layer of the kit is
`.claude/pipeline.json` (reference: `pipeline/config.md`); the kit's agents and
skills are linked into `.claude/agents` and `.claude/skills` by
`pipeline/setup.sh`.

## Roles here

The roles and models are in the kit's README. Here: the `liaison` keeps
`docs/roadmap.md`, `docs/questions.md`, `docs/tech-debt.md`, this file and
`.claude/rules/`; the mod runs `tools/install.sh` after a merge that touches
`crates/`, `app/`, `baluk/` or `skills/` (`afterMerge`). A task goes to
`developer-1` when it follows an existing pattern (a CLI command, an endpoint,
a client component, a library block, a tool), to `developer-2` when it spans
several crates or the core and the client, or touches the sync protocol, builds
and caches, memory, concurrency or Typst internals.

## Isolation

- Worktrees build into the `target/` of the main checkout
  (`tools/lib/worktree-target.sh`): a build from scratch is ~8 min and tens of
  GB, and the dependencies built once serve every worktree. `tools/check.sh`
  prunes the stale units of our own crates and cleans the dir over 30 GB
  itself, under the lock of heavy work (`.claude/rules/tools.md`, "Checks"). A
  build by hand in the main checkout uses other rustflags, so it builds its own
  copy of the dependencies.
- The heavy steps of all runs (`tools/check.sh`, `tools/cargo.sh`, the builds in
  `tools/install.sh` and `tools/test-env.sh`) take turns on one lock,
  `~/.cache/baluk-notes/heavy.lock`, and each run is capped at `CHECK_JOBS`
  CPUs (default n - round(n/6) of n CPUs, all of 1-2;
  `tools/lib/cpu-cap.sh`). Measurements are run the same
  way: `nice -n 2`, within the cap. The test server of an agent takes its own
  port (`PORT=... tools/test-env.sh`).
- An agent never opens a window on the user's display: anything that needs a
  window (the `notes-app` window, WebKitGTK) runs inside a headless gamescope
  (`tools/shot-webkit.py`, `tools/app-smoke.sh`; rules - `.claude/rules/tools.md`,
  "No windows on the user's display"). gamescope exits 0 even when the command
  fails: check the command's own result; if gamescope fails or is missing,
  report it, never fall back to a visible window.
- Agents do not touch the user's notes, the installed `notes` service beyond
  `tools/install.sh`, or the VPS: `tools/deploy-hub.sh` runs only on the
  user's word.

## Server

The repository is on a local Forgejo server, `http://localhost:3000`,
`ibaluk/baluk-notes`, reachable only from `localhost`; the remote `origin`.
Agents use `git` and `tea` (commands - the `forge` skill). The server shares
the accounts with the `nexus` project:

| Account | `tea` login | Used by | For |
|---|---|---|---|
| `ibaluk` | `owner` | the user, every `git push`, the mod's merge | owner, administrator |
| `agent-dev` | `dev` | developers, `tools/check-pr.sh` | opens pull requests, comments the check |
| `agent-reviewer` | `reviewer` | `reviewer` | approves or requests changes |

Separate accounts make the review formal: the server does not let an account
approve its own pull request. Protection of `main`: a merge needs an approval
of `agent-reviewer` and the status `ci / full-check (pull_request)`, a new
commit makes the approval stale; `agent-dev` and `ibaluk` may merge; only
`ibaluk` pushes to `main` directly. All pushes go as `ibaluk`, so the ban on
developers pushing to `main` rests on the agents' rules.

CI: the runner checks every push to a pull request
(`.forgejo/workflows/check.yml` -> `tools/check-pr.sh`): the full
`tools/check.sh` at the head commit (`.claude/rules/tools.md`, "CI").

GitHub (`github.com/Peper20/BalukNotes`, the remote `github`) is the public
mirror: Forgejo pushes every change of the repository there (a push mirror
over SSH with a deploy key). The cloud environment of Claude Code works from
it.

In a Claude Code cloud session there is no such server. The first step of the
session, `tools/cloud-setup.sh` (`.claude/rules/tools.md`, "Cloud
environment"), starts a throwaway Forgejo on `127.0.0.1:3000` inside the
session with the same accounts, `tea` logins and protection of `main`, and
pushes the clone's `main` into it; `origin` is then the cloud Forgejo and
`github` the clone's own remote, so the rules, the `forge` skill and the agent
definitions work unchanged. Nothing in the session writes to GitHub `main`: at
the end `tools/cloud-finish.sh` pushes the cloud `main` to a GitHub branch (by
default the `claude/...` branch the session started on) and prints the link;
the user opens the pull request there. The server dies with the session:
anything not sent by `cloud-finish.sh`, open pull requests included, is lost.

## Launch

```sh
claude --plugin-dir pipeline
```

From the project root. `"agent": "liaison"` in `.claude/settings.json` makes the
main session the `liaison`, locally and in a cloud session (the cloud session
reads the file from the clone; the model of a cloud session is chosen in its
interface, not by the agent definition). `--plugin-dir` loads the pipeline mod
(`pipeline/docs/README.md`, "The mod"); the runner of CI is a user service
(`tools/ci-runner.sh status`). `tools/pipeline` is a link to `pipeline/`, kept
for sessions started with the old path.

## Known limits

- The runner is a user service without lingering (the user's decision): it
  stops when the user logs out, and the jobs queued meanwhile run at the next
  login.
- A cloud session has no runner, so the mod would wait for a status that
  never comes: the pipeline does not run in a cloud session now.
- The limits of the kit (`pipeline/docs/README.md`, "Known limits") apply:
  `cargo`, `npm`, `git`, `tea` and `tools/*.sh` must be allowed in advance (or
  the session runs in auto mode).

## Project specifics for the agents

What the kit's agents and skills defer to. Read it first.

**Rules.** `.claude/rules/project.md` (the map and the Essentials), the rule
file of every directory you touch (`crates.md`, `app.md`, `baluk.md`,
`skills.md`, `tools.md`, ...), and `.claude/rules/docs.md` for
`docs/architecture.md` and `docs/tech-debt.md` (the format of an entry).

**Language.** The `liaison` speaks Russian with the user. Code, comments, logs,
docs, commit messages, pull requests and briefs are English (`.claude/rules/project.md`,
"Language").

**Tests and the full check.** Run the crate's tests with
`tools/cargo.sh test -p <crate> [filter]`, `npm test` in `app/`,
`tools/test-libs.sh` for `tools/`. Run `cargo` by hand only as
`tools/cargo.sh <arguments>`: it builds into the main checkout's `target/`,
shared by all worktrees (`tools/lib/worktree-target.sh`), with the settings of
the checks, and freshens the sources when another checkout built there last. A
plain `cargo` builds from scratch in the worktree or reuses another checkout's
artifacts of our crates. Waiting on another checkout's lock is normal. The full
check is `tools/check.sh`; CI runs it, agents do not. e2e runs only inside it:
it listens on the ports 8600-8649, so a `playwright` run by hand breaks the
checks running on this machine.

**Look by eye.** For changes to the library, CSS or client, look as
`.claude/rules/tools.md` ("Look by eye") says. Start the test server on your own
port (`PORT=<8500-8599> tools/test-env.sh`), not 8432: other agents may be using
it. Stop it by its port.

**The liaison's areas.** The liaison owns the architecture: crate boundaries,
the core/client boundary, the HTTP API, the public library API, the vault
format and path rules (`NoteId`), the sync protocol, cross-cutting schemes
(builds and caches, memory, concurrency). The Essentials of
`.claude/rules/project.md` bind its designs.

**What goes with a change.** A change to the public library API, the
`note`/`book` templates, `notes` commands or path rules also changes
`docs/writing.md` and the `/baluk-note` skill, then `tools/install.sh`
(`.claude/rules/project.md`, "Iteration"). Developers do not touch
`docs/roadmap.md`, `docs/questions.md`, `docs/agents.md`, `.claude/agents/`,
`.claude/skills/` or `pipeline/claude/agents/`, `pipeline/claude/skills/`, `pipeline/docs/`
unless the brief says so.

**Documents the liaison commits itself**, straight to `main`: `docs/roadmap.md`,
`docs/questions.md` (open questions to the user; the "Look yourself" section
is what it reminds the user of at the start of a session), `docs/agents.md`,
the design decisions in `docs/architecture.md` and `docs/tech-debt.md` that no
code yet follows, `.claude/rules/`, `.claude/pipeline.json`, and the kit's
agents, skills and documents (`pipeline/claude/agents/`, `pipeline/claude/skills/`,
`pipeline/docs/`, `pipeline/config.md`). The code of the mod and the scripts
reach `main` through pull requests. Permission settings
(`.claude/settings*.json`) are the user's.

**Review.** Besides the kit's checklist, the Essentials of
`.claude/rules/project.md`: a note in HTML looks like its PDF; the app does not
change note files silently; the user's notes are never touched. Sensitive
places: the hub trusting client input, unsafe paths (escaping the vault).

**Cloud session.** The `liaison` runs `tools/cloud-setup.sh` as the very first
step, before reading or delegating anything, and `tools/cloud-finish.sh` at the
end, once every pull request is merged or blocked; then it gives the user the
branch it prints.

**The user's word.** An update of the server on the VPS (`tools/deploy-hub.sh`,
`docs/server.md`) runs only on the user's word; the `liaison` runs it itself
from the main checkout. Commands that need root are the user's to run. The
user's own notes are not touched.

**Pull requests** open at `http://localhost:3000/ibaluk/baluk-notes/pulls/<number>`.

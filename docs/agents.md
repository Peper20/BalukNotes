# The development pipeline

How development of baluk notes runs: who does what, how a task gets from a
request to `main`, where the settings are. The detailed rules of each role are
in its definition (`.claude/agents/`); this document is the map.

## Roles

| Who | Model | Does | Does not |
|---|---|---|---|
| `liaison` | Opus, medium | Talks to the user in Russian, owns the architecture and the specification (`docs/architecture.md`), decides on the tech debt, splits requests into tasks, picks the developer of each and starts them through the pipeline mod, answers developers' questions, keeps `docs/roadmap.md`, `docs/questions.md`, `docs/tech-debt.md`, this file and `.claude/rules/`, `.claude/agents/`, `.claude/skills/`, reports | Write code, merge |
| pipeline mod | code, no model | Starts developers and reviewers, follows the CI status and the review on the server, returns failures and findings to the developer, counts rounds, hands over to `developer-3`, blocks, merges, cleans up, runs `tools/install.sh`, reports to the liaison (`tools/pipeline/`, `.claude/rules/tools.md`, "Pipeline mod") | Decide anything a rule below does not fix |
| `developer-1` | Haiku, xhigh | Routine, well-described, small tasks along an existing pattern | Merge, design the architecture |
| `developer-2` | Sonnet, high | Complex tasks: several crates or the core with the client, sync, builds and caches, memory, concurrency, Typst internals | Merge, design the architecture |
| `developer-3` | Opus, medium | The hardest: elusive bugs, investigations, tasks another developer could not finish, resumed blocked tasks | Merge, design the architecture |
| `reviewer` | Sonnet, high | Reads the change against its passed CI check, runs targeted tests, approves or requests changes | Edit files |

The user talks only to the `liaison`. Agents talk to each other in English.

## The path of a task

1. **Request.** The user describes the task to the `liaison`; it makes the goal
   and acceptance criteria clear (how a feature looks is decided, not asked -
   `.claude/rules/project.md`, "Essentials").
2. **Design and tasks.** The `liaison` designs the change, decides which tech
   debt is done now, works out the change to `docs/architecture.md`, splits
   the request into tasks (one per coherent change) and picks the developer
   of each.
3. **Start.** For each task the `liaison` calls `pipeline_start` with a
   self-contained brief (the developer has not seen the conversation); tasks
   that depend on others wait with `after`. Independent tasks run in parallel.
4. **Development.** The developer starts cold in its own worktree with the
   design fixed in its brief, works on the branch `task/<slug>`, runs the
   targeted tests, opens a pull request and ends with a `PIPELINE:` line (the
   `dev-workflow` skill). A question or a report that needs the `liaison`
   first (architecture additions, deferred items) goes to it; it answers with
   `pipeline_send`.
5. **CI.** The runner checks every push to a pull request
   (`.forgejo/workflows/check.yml` -> `tools/check-pr.sh`): the full
   `tools/check.sh` at the head commit, as the commit status `ci / full-check
   (pull_request)` and a comment with the step table. A failure goes back to
   the developer as a round.
6. **Review.** On a `success` status the mod starts a `reviewer`; it reads the
   pull request in its own worktree, runs targeted tests and gives the verdict
   on the server. Blocking findings go back to the same developer as a round.
7. **Merge.** An approval and a `success` status at the same head: the mod
   squashes the pull request into `main`, deletes the branch, removes the
   worktrees, runs `tools/install.sh` when the diff touches what it installs,
   and reports to the `liaison`, which marks the roadmap and reports to the
   user in Russian.

Rounds and the end of a task:

- A round is one return of the pull request to a developer with a failed
  check or blocking findings. Each developer gets two; then a new
  `developer-3` takes the task over on the same branch and pull request.
  After `developer-3`'s two rounds the task is blocked.
- A blocked task is still wanted but cannot be finished now: its pull request
  stays open and its branch stays on the server; the `liaison` brings it to
  the user, and `pipeline_send` resumes it on a new `developer-3`.
- A dropped task is discarded for good (its pull request closed, its branch
  deleted), only on the user's word.

The `liaison` owns the architecture (`docs/architecture.md`); developers apply
its changes to the document in the same pull request as the code. Tech debt
(`docs/tech-debt.md`) entries change in the pull requests of the code: a
developer may defer a large unforeseen problem with an entry, the `liaison`
decides whether it waits. Requirements to both documents - `.claude/rules/docs.md`.

## Isolation

- Each developer and reviewer works in its own git worktree in
  `.claude/worktrees/` (not in git), on its own branch. The `liaison` and the
  pipeline mod work in the main checkout.
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
  CPUs (default 9, `tools/lib/cpu-cap.sh`). Measurements are run the same
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
claude --plugin-dir tools/pipeline
```

From the project root. `"agent": "liaison"` in `.claude/settings.json` makes the
main session the `liaison`, locally and in a cloud session (the cloud session
reads the file from the clone; the model of a cloud session is chosen in its
interface, not by the agent definition). `--plugin-dir` loads the pipeline mod
(`.claude/rules/tools.md`, "Pipeline mod"); the runner of CI is a user service
(`tools/ci-runner.sh status`).

## Known limits

- Agents run in the background and cannot ask for a permission: `cargo`,
  `npm`, `git`, `tea` and `tools/*.sh` must be allowed in advance (or the
  session runs in auto mode).
- The mod lives in the liaison's session: with the session closed CI still
  checks pushes, but no reviewer starts and nothing merges until the session
  is back (the mod reloads its tasks).
- The runner is a user service: it stops when the user logs out, unless
  lingering is enabled (`loginctl enable-linger`, the user's decision).
- The mod's API is early access and may change with a Claude Code update.
- A cloud session has no runner, so the mod would wait for a status that
  never comes: the pipeline does not run in a cloud session now.

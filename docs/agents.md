# The development pipeline

How development of baluk notes runs: who does what, how a task gets from a
request to `main`, where the settings are. The detailed rules of each role are
in its definition (`.claude/agents/`); this document is the map.

## Roles

| Agent | Model | Does | Does not |
|---|---|---|---|
| `liaison` | Opus, medium | Talks to the user in Russian, owns the architecture and the specification (`docs/architecture.md`), decides on the tech debt, splits requests into tasks and picks the developer of each, writes the orchestrator's brief in English, keeps `docs/roadmap.md`, `docs/questions.md`, `docs/tech-debt.md`, this file and `.claude/rules/`, `.claude/agents/`, `.claude/skills/`, reports | Write code, run developers, merge |
| `orchestrator` | Sonnet, medium | Runs the pipeline: starts the tasks of the brief, waits for them, runs the review, merges approved pull requests, runs the full check of each pull request (`tools/check-pr.sh`), cleans up, runs `tools/install.sh`, relays questions to the liaison | Design, split or change tasks, decide on the tech debt, edit files |
| `developer-1` | Haiku, xhigh | Routine, well-described, small tasks along an existing pattern | Merge, design the architecture |
| `developer-2` | Sonnet, high | Complex tasks: several crates or the core with the client, sync, builds and caches, memory, concurrency, Typst internals | Merge, design the architecture |
| `developer-3` | Opus, medium | The hardest: elusive bugs, investigations, tasks another developer could not finish, resumed blocked tasks | Merge, design the architecture |
| `reviewer` | Sonnet, high | Reads the change against its passed full check, runs targeted tests, approves or requests changes | Edit files |

The user talks only to the `liaison`. Agents talk to each other in English.

## The path of a task

1. **Request.** The user describes the task to the `liaison`; it makes the goal
   and acceptance criteria clear (how a feature looks is decided, not asked -
   `.claude/rules/project.md`, "Essentials").
2. **Design and tasks.** The `liaison` designs the change, decides which tech
   debt is done now, works out the change to `docs/architecture.md`, splits
   the request into tasks (one per coherent change) and picks the developer
   of each.
3. **Brief.** Goal, context, design, tasks (each with its developer, files,
   tech debt entries, architecture change and acceptance criteria), out of
   scope. The brief is self-contained: the `orchestrator` and the developers
   have not seen the conversation. The `orchestrator` runs independent tasks
   in parallel.
4. **Development.** A developer starts cold in its own worktree with the
   design fixed in its brief, works on the branch `task/<slug>`, runs
   `tools/check.sh --fast` and opens a pull request (the `dev-workflow` skill).
5. **Full check.** The `orchestrator` runs `tools/check-pr.sh` on the head of
   the pull request: the full `tools/check.sh` in a worktree of that commit,
   the result as a pull request comment. A failure goes back to the developer
   as a round; the review starts only on a passed head.
6. **Review.** The `reviewer` reads the pull request in its own worktree,
   relies on its passed full check, runs targeted tests and gives the verdict.
   Blocking findings go back to the same developer, and the head is checked
   again before the next review; at most two rounds per developer, then
   `developer-3`. A task that cannot be finished is blocked (its pull request
   stays open) by the `orchestrator`; dropped (deleted everywhere) only by the
   user.
7. **Merge.** The `orchestrator` squashes an approved pull request into
   `main`, deletes the branch, removes worktrees, and runs `tools/install.sh`
   when the request changed what it installs.
8. **Report.** The `liaison` reports to the user in Russian: blocked tasks and
   tasks that took `developer-3` two rounds first, then the results, the
   tech debt, what to look at.

The `liaison` owns the architecture (`docs/architecture.md`); developers apply
its changes to the document in the same pull request as the code. Tech debt
(`docs/tech-debt.md`) entries change in the pull requests of the code: a
developer may defer a large unforeseen problem with an entry, the `liaison`
decides whether it waits (the `orchestrator` relays). Requirements to both documents - `.claude/rules/docs.md`.

## Isolation

- Each developer and reviewer works in its own git worktree in
  `.claude/worktrees/` (not in git), on its own branch. The `liaison` and the
  `orchestrator` work in the main checkout.
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
| `ibaluk` | `owner` | the user, every `git push` | owner, administrator |
| `agent-dev` | `dev` | `orchestrator`, developers | opens and merges pull requests |
| `agent-reviewer` | `reviewer` | `reviewer` | approves or requests changes |

Separate accounts make the review formal: the server does not let an account
approve its own pull request. Protection of `main`: a merge needs an approval
of `agent-reviewer`, a new commit makes it stale; `agent-dev` merges; only
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
claude
```

From the project root. `"agent": "liaison"` in `.claude/settings.json` makes the
main session the `liaison`, locally and in a cloud session (the cloud session
reads the file from the clone; the model of a cloud session is chosen in its
interface, not by the agent definition). `claude --agent <name>` overrides it.

## Known limits

- Agents run in the background and cannot ask for a permission: `cargo`,
  `npm`, `git`, `tea` and `tools/*.sh` must be allowed in advance (or the
  session runs in auto mode).
- No CI: the orchestrator runs the full check (`tools/check-pr.sh`) on this
  machine, between the developer and the review.
- Rare cases are not written into the rules: the `orchestrator` handles them
  on the spot and names them in the report.

# The development pipeline

How development runs with the kit: who does what, how a task gets from a
request to `main`, what the mod does. The detailed rules of each role are in
its definition (`claude/agents/`); this document is the map. The project's own
side
- its server, accounts, commands, limits - is its pipeline page, named in
`.claude/pipeline.json` (`config.md`).

## What the kit needs from a project

- A git repository with `main` and a Forgejo server as `origin`; the `tea`
  client with the logins `dev` (opens pull requests, comments), `reviewer`
  (reviews) and `owner` (pushes, merges).
- A full check of the project, run by CI on every push to a pull request and
  published as the combined commit status of the head; with the kit's runner
  ("Checks") it only supplies its steps.
- `.claude/pipeline.json` (`config.md`).
- A pipeline page (`guide`) with the section "Project specifics for the
  agents": the commands for tests, builds and the full check, the places to
  look by eye, the documents the liaison owns, the areas of the architecture,
  what no agent touches. The agents read it first; the mod's prompts name it.
- Optionally a rules map (`map`) and rule files per directory.
- `setup.sh` links the kit's agents and skills (`claude/agents`,
  `claude/skills`, outside the folders a plugin loads by itself, so each is
  listed once) into `.claude/agents` and
  `.claude/skills` of the project (Claude Code finds them there by their
  plain names, which the mod uses to start them); the session is launched
  with `claude --plugin-dir <kit>`.

## Roles

| Who | Model | Does | Does not |
|---|---|---|---|
| `liaison` | Opus, medium | Talks to the user, owns the architecture and the specification, decides on the tech debt, splits requests into tasks, picks the developer of each and starts them through the pipeline mod, answers developers' questions, keeps the roadmap, the questions, the pipeline's own documents, reports | Write code, merge |
| pipeline mod | code, no model | Starts developers and reviewers, follows the CI status and the review on the server, returns failures and findings to the developer, counts rounds, hands over to `developer-3`, blocks, merges, cleans up, runs the command after a merge, reports to the liaison | Decide anything a rule below does not fix |
| `developer-1` | Haiku, xhigh | Routine, well-described, small tasks along an existing pattern | Merge, design the architecture |
| `developer-2` | Sonnet, high | Complex tasks across modules or layers, protocols, builds and caches, memory, concurrency | Merge, design the architecture |
| `developer-3` | Opus, medium | The hardest: elusive bugs, investigations, tasks another developer could not finish, resumed blocked tasks | Merge, design the architecture |
| `reviewer` | Sonnet, high | Reads the change against its passed CI check (no re-run of its tests), approves or requests changes | Edit files |

The user talks only to the `liaison`. Agents talk to each other in English.

## The path of a task

1. **Request.** The user describes the task to the `liaison`; it makes the goal
   and acceptance criteria clear.
2. **Design and tasks.** The `liaison` designs the change, decides which tech
   debt is done now, works out the change to the architecture document, splits
   the request into tasks (one per coherent change) and picks the developer of
   each.
3. **Start.** For each task the `liaison` calls `pipeline_start` with a
   self-contained brief (the developer has not seen the conversation); tasks
   that depend on others wait with `after`. Independent tasks run in parallel.
4. **Development.** The developer starts cold in its own worktree with the
   design fixed in its brief, works on the branch `task/<slug>`, runs the
   targeted tests, opens a pull request and ends with a `PIPELINE:` line (the
   `dev-workflow` skill). A question or a report that needs the `liaison`
   first (architecture additions, deferred items) goes to it; it answers with
   `pipeline_send`.
5. **CI.** The project's CI checks every push to a pull request: the full check
   at the head commit, as the commit status and a comment with the step table.
   A failure goes back to the developer as a round.
6. **Review.** On a `success` status the mod starts a `reviewer`; it reads the
   pull request in its own worktree, runs only what CI cannot show and gives
   the verdict on the server. Blocking findings go back to the same developer
   as a round.
7. **Merge.** An approval and a `success` status at the same head: the mod
   squashes the pull request into `main`, deletes the branch, removes the
   worktrees, runs the `afterMerge` command of the project when the diff
   touches its paths, and reports to the `liaison`, which marks the roadmap and
   reports to the user.

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

The `liaison` owns the architecture; developers apply its changes to the
architecture document in the same pull request as the code. Tech debt entries
change in the pull requests of the code: a developer may defer a large
unforeseen problem with an entry, the `liaison` decides whether it waits.

## Isolation

- Each developer and reviewer works in its own git worktree in
  `.claude/worktrees/` (not in git), on its own branch. The `liaison` and the
  pipeline mod work in the main checkout.
- What worktrees share (build directories, caches, locks, ports, CPU) and how
  a run keeps from breaking another is the project's to say, on its pipeline
  page. The agents' rule is to use only the project's own commands for it.
- An agent never opens a window on the user's display, never touches the
  user's data, and never deploys: the pipeline page lists what is the user's
  to run.

## Checks

`scripts/` is the generic part of the project's checks (bash; the config is
read with `jq` or `python3`); the project keeps its steps and the settings of
its tools (`config.md`: `check`, `tests`). `scripts/test-scripts.sh` tests them.

- **The full check**: `scripts/check.sh <arguments>` runs at nice 2 and ionice
  5 and calls `steps "<arguments>"` of the steps file (`check.steps`), so the
  project parses its own options and prints its own help. The file runs
  commands with `step <name> <command...>` (under the lock of heavy work),
  `step_unlocked` (for a command that takes the lock itself: it is not
  reentrant), `skip <name> <reason>` and `last_ok`; `$logs` is the log folder.
  Optional hooks: `before_steps` (once, before the first step, e.g. pruning a
  shared build dir) and `on_heavy_acquire`. All steps run; the exit code is
  non-zero if one failed. The output: a table (step, result, seconds) on stdout
  and in `$logs/table.txt`, and `$logs/summary.md`: the same with the time
  spent waiting for the lock per step (not in the step's time), a total row,
  the wall time, and the last 25 lines of each failed step's log in a
  collapsed block. When `GITHUB_STEP_SUMMARY` is set, as Forgejo Actions does,
  the summary is appended to it: the job page shows the table.
- **The CPU cap and the lock** (`scripts/lib/cap.sh`, sourced by the runner
  and by the project's own scripts): `CHECK_JOBS`, the CPUs the scripts may
  keep busy (all of a machine with 1-2 CPUs, otherwise n - round(n/6);
  `CHECK_JOBS` of the environment wins, capped at the CPUs of the machine),
  and one lock (`check.heavyLock`) for the heavy work of all runs: `heavy
  <command...>`, or `heavy_acquire` and `heavy_release` (`HEAVY_WAIT` is the
  seconds waited). The project's `check.env` file turns the number into the
  settings of its tools.
- **Targeted tests**: `scripts/test.sh [--dry-run] [paths...]` maps the files
  changed against `origin/main` (or the paths given) to the commands of `tests`,
  runs each distinct command once under the cap and at low priority, prints a
  table and exits non-zero on a failure. It does not hold the lock itself:
  the project's commands take it where they need it. What needs fixed ports
  or the whole project (end-to-end tests) is left to the full check.
- **Measuring**: `scripts/measure.sh <runs> -- <command...>` runs a command N
  times under the lock, the cap and nice 2 and prints per run the wall seconds,
  the average and peak busy CPUs of the machine (`/proc/stat`) and the exit
  code, then min, median and max. A speed claim in a pull request names the
  command, so that a reviewer can run the same line. A task that measures has a
  budget (runs, time) in its brief; without one, a single demonstration run.
- **The mod's own check**: `scripts/lib/mod-check.sh` (`pipeline_mod_check`,
  for the steps file): validate, the mod's tests and `tsc`, with the engine
  types copied into a linked worktree.

## Forgejo

The server side of the cycle: the check of every pull request run by CI, the
runner, and a throwaway server for a cloud session. The scripts are in
`scripts/`, the workflow template in `forgejo/`; the project's values are in
`.claude/pipeline.json` (`config.md`: `forge`, `ci`, `check`).

### CI

Every push to a pull request into `main` (opened, synchronize, reopened) runs
the full check as a commit status. The project copies `forgejo/check.yml` to
`.forgejo/workflows/check.yml` and sets `runs-on` to its runner (`ci.runner`).
The one job `full-check` runs `$PIPELINE_MAIN/pipeline/scripts/check-pr.sh
<number>` of the main checkout, so the check builds into the shared build
dirs and takes the shared locks. The job result is the exit code of the
script: 0 success, anything else failure. A readable status of a head is the
combined status of its commit, `tea api --login dev
'/repos/{owner}/{repo}/commits/<sha>/status'`: `state` is `pending`,
`success`, `failure` or `error`.

`scripts/check-pr.sh <number>` (CI; by hand from any checkout) fetches
`pull/<number>/head`, checks it out detached in the worktree of the main
checkout `.claude/worktrees/check-pr-<number>` and runs the worktree's own
`scripts/check.sh` there. A later run reuses the worktree: ignored output
such as `node_modules` stays, the paths of `ci.staleOutput` are removed when
the head changed, tracked files the check changed are restored. The result is
the comment as `dev` on the pull request, which starts with exactly `Full
check passed at <sha>.` or `Full check failed at <sha>: <steps>.` (the mod
reads these lines), then the step table. The whole output is `run.log` in the
worktree's folder of the logs (`check.logs`). Exit 0 passed, 1 failed, 2 could
not run; the last line of its output is `check-pr: exit <code>` (the end to
wait for). A second run for the same pull request while one is going exits 2
(a lock `check-pr-<number>.lock` next to the lock of heavy work, which
`check.sh` takes step by step). It prints the failed steps and the absolute
paths of their logs.

The runner is a Forgejo Runner (version and sha256 pinned in
`scripts/ci-runner.sh`) on the project's machine: a repository runner
`ci.runner`, label `<runner>:host` (the job runs on the host, no Docker),
capacity 1, so the jobs queue and the check of a newer head starts when the
older one ends. It runs as the user service `forgejo-runner.service` (while
the user is logged in), `Nice=2`. `ci-runner.sh install` sets it up
(idempotent; no root), `uninstall` removes it, `status` shows the unit and the
runner as Forgejo sees it. The server is `forge.url`, the registration goes
through the `owner` login of `tea`, the job finds the main checkout in
`PIPELINE_MAIN` of the runner's environment (the checkout the script belongs
to). `ci.tools` lists the programs the project's check calls; `install` warns
about those missing from the runner's PATH.

- Config and the token of the runner: `~/.config/forgejo-runner/config.yml`
  (0600); the unit: `~/.config/systemd/user/forgejo-runner.service`; the
  working dirs of jobs: `~/.local/share/forgejo-runner/`; the binary:
  `~/.local/bin/forgejo-runner`. `install` writes the first two: edit the
  script, not them.
- Logs: the runner's - `journalctl --user -u forgejo-runner`; a job's - in
  Forgejo, on the run of the pull request (the list through the API: `tea api
  --login dev '/repos/{owner}/{repo}/actions/runs'`).
- The job runs code of the pull request with the rights of the user (host
  execution, no sandbox), as the agents do by hand. A change of the workflow
  in a pull request takes effect in that pull request's own run, but the
  script it runs is the main checkout's.

### Cloud session

A Claude Code cloud session clones a GitHub mirror, runs as root, has a small
disk and no Forgejo, and has no CI runner, so the mod does not run there. The
rest of the cycle works with a throwaway Forgejo started inside the session.

- **`scripts/cloud-setup.sh`**, the first step of the session (idempotent;
  needs `git`, `curl`, `sha256sum`, `awk` and `jq` or `python3` for the
  config; `--install` only downloads, `--stop` stops the server). It installs
  the pinned releases of Forgejo and `tea` (versions and sha256 in the script;
  a mismatch stops it), initialises Forgejo on `127.0.0.1:$FORGEJO_PORT`
  (default 3000) with SQLite, no registration, no email, no SSH, data in
  `$FORGEJO_HOME` (default `~/.cache/<project name as a word>/forgejo`; random
  passwords in its `credentials`, 0600). It creates the account of the owner
  of `forge.repo` (admin) and `agent-dev`, `agent-reviewer`, the `tea` logins
  `owner`, `dev`, `reviewer`, the repository `forge.repo` (squash merges
  only), pushes the clone's `main` into it (a shallow clone is completed
  first) and protects `main` as the project's server does: a merge needs an
  approval of `agent-reviewer`, a new commit makes it stale, only the owner
  pushes. `origin` becomes the cloud Forgejo and `github` the clone's former
  `origin`; git signs in as the owner through a helper that reads
  `credentials`. From then on `git`, `tea` and the `forge` skill work as
  locally.
- It refuses, changing nothing, when the port is taken by another program,
  when `$FORGEJO_HOME` is not its own (no `.pipeline-cloud-forgejo`), when
  `origin` is already the project's Forgejo repository or `github` exists, or
  when the `tea` config has `owner`, `dev` or `reviewer` that the script did
  not make for this server: it never touches the user's own Forgejo. On the
  user's machine `origin` and the logins have exactly the URL of the default
  port, so a rerun is accepted only with the proof in `$FORGEJO_HOME`
  (`remotes-done`, `tea-logins`) that the script itself arranged them.
  Nothing is mirrored to GitHub, and nothing writes to GitHub `main`.
- An environment setup script of the cloud session (root, cached in a
  snapshot) may run `scripts/cloud-setup.sh --install` to cache the two
  downloads (~150 MB); the network policy must allow `codeberg.org` (Forgejo)
  and `dl.gitea.com` (`tea`), both without redirects.
- **`scripts/cloud-finish.sh [--branch NAME]`**, the last step, once every pull
  request is merged or blocked: fetches `main` of the cloud Forgejo and pushes
  it to a GitHub branch: `--branch`, else `$CLOUD_BRANCH`, else the `claude/...`
  branch the session started on (recorded by the setup), else `cloud/<date>`.
  It refuses `main`, never forces, reports a rejected push (the branch has
  commits the session lacks) with the ways out, warns about pull requests
  still open on the server (they are not sent) and prints the link for the
  user to open a pull request from. The server dies with the session: what is
  not sent is lost.

## The mod

`hooks/` is a Claude Code plugin (a "mod") that the liaison's session loads. No
model decides a routine step. It starts developers and reviewers as subagents
(they show in the session window), reacts to the commit status and the review
on the server, counts rounds, merges, cleans up and reports to the liaison.

- **Load**, with no edit of `.claude/settings*.json`: from the project root
  `claude --agent liaison --plugin-dir <kit>` (an absolute path from
  elsewhere), or `CLAUDE_CODE_PLUGIN_DIRS=<absolute path>` where no flag can
  be given. The mod keeps working over a restart: its tasks are in `$.store`
  (key `tasks`, so in the user's Claude Code state, not in the repository).
- **The project layer**: `.claude/pipeline.json`, read at session start
  (`config.md`). A missing or invalid file is reported once to the liaison, and
  the mod starts no task: the tools refuse with the reason, and a call after
  the file is fixed reads it anew.
- **Tools of the liaison** (`mcp__pipeline__<name>`): `pipeline_start { slug,
  developer, brief, after? }` (the developer gets the brief, the branch
  `task/<slug>`, the project and its pages to read first, and the contract:
  targeted tests only, push, open the pull request, end with `PIPELINE: pr=<n>`,
  `PIPELINE: pr=<n> liaison` or `PIPELINE: question`), `pipeline_status`,
  `pipeline_send { slug, text }` (an answer to a developer; resumes the task),
  `pipeline_stop { slug?, reason }`.
- **Stages** of a task: waiting (for `after`), developing, liaison (waits for
  `pipeline_send`), checking (the status of the head, polled every 60 s),
  reviewing, merging, merged, blocked. Rounds, hand-over to `developer-3` and
  the block follow "The path of a task"; the merge only with an approval of
  `reviewer` and a `success` status at the same head commit (a head approved
  while its status was still pending is merged when the status passes, with no
  second review; a new head clears the approval), squash as `owner`; a merge
  that fails for any reason but a conflict or a moved head goes to the
  liaison, never retried by the mod. Then cleanup (the worktrees and branches
  of its agents, the local `task/<slug>` when it equals the merged head, the
  check worktree) and the `afterMerge` command. The mod never pushes to `main`,
  never forces a push, never deploys.
- **View**: the pane `/pipeline` (a row per task: stage, pull request, status,
  developer, rounds, last event) and the status line under the prompt, the
  counts by stage (`1 developing · 2 checking`; the engine puts the plugin's
  name before it).
- **Code**: `hooks/machine.ts` decides (state, event and config in, state and
  actions out; no clock, no server), `hooks/driver.ts` carries the actions out
  and feeds results back as events, `hooks/forge.ts` holds every `tea` and
  `git` call, `hooks/config.ts` reads the project layer, `hooks/register.tsx`
  only wires events, timers and `$` to them. Tests (`tests/*.test.ts`) use a
  fake server and fake ports (`tests/helpers.ts`); none talks to the server.
- **Checks**: `claude plugin validate <kit>`, `claude plugin test <kit>` and
  `tsc -p <kit>`. The types of the engine (`.claude-plugin/types/`, not in the
  repository) are laid by Claude Code when it loads the mod; a `tsc` run needs
  them.

## Known limits

- Agents run in the background and cannot ask for a permission: the commands
  of the project must be allowed in advance (or the session runs in auto mode).
- The mod lives in the liaison's session: with the session closed CI still
  checks pushes, but no reviewer starts and nothing merges until the session
  is back (the mod reloads its tasks).
- The mod's API is early access and may change with a Claude Code update.

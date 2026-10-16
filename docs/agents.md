# The development pipeline

How development of baluk notes runs: who does what, how a task gets from a
request to `main`, where the settings are. The detailed rules of each role are
in its definition (`.claude/agents/`); this document is the map.

## Roles

| Agent | Model | Does | Does not |
|---|---|---|---|
| `liaison` | the session's | Talks to the user in Russian, makes the goal and acceptance criteria clear, keeps `docs/roadmap.md`, `docs/questions.md`, this file and `.claude/rules/`, `.claude/agents/`, `.claude/skills/`, writes the orchestrator's brief in English, reports | Write code, edit `docs/architecture.md` or `docs/tech-debt.md`, run developers, merge |
| `orchestrator` | Sonnet, medium | Owns the architecture and designs what the request needs, decides on the tech debt, splits the request into tasks, picks the developer, runs the review, merges approved pull requests, cleans up, runs `tools/install.sh` | Edit files |
| `developer` | Sonnet, medium | Simple tasks in one area along an existing pattern | Merge, design the architecture |
| `developer-senior` | Sonnet, high | Complex tasks: several crates or the core with the client, sync, builds and caches, memory, concurrency, Typst internals | Merge, design the architecture |
| `developer-expert` | Opus, medium | The hardest: elusive bugs, investigations, tasks another developer could not finish, resumed blocked tasks | Merge, design the architecture |
| `reviewer` | Sonnet, medium | Builds the pull request, runs `tools/check.sh`, reads the change, approves or requests changes | Edit files |

The user talks only to the `liaison`. Agents talk to each other in English.

## The path of a task

1. **Request.** The user describes the task to the `liaison`; it makes the goal
   and acceptance criteria clear (how a feature looks is decided, not asked -
   `.claude/rules/project.md`, "Essentials") and takes the related tech debt
   into the brief.
2. **Brief.** Goal, context, acceptance criteria, out of scope. The brief is
   self-contained: the `orchestrator` has not seen the conversation.
3. **Plan.** The `orchestrator` designs the change, decides which tech debt is
   done now and splits the request into tasks: one task per coherent change,
   independent ones in parallel.
4. **Development.** A developer starts cold in its own worktree with the
   design fixed in its brief, works on the branch `task/<slug>`, runs
   `tools/check.sh` and opens a pull request (the `dev-workflow` skill).
5. **Review.** The `reviewer` builds and checks the pull request in its own
   worktree and gives the verdict. Blocking findings go back to the same
   developer; at most two rounds per developer, then `developer-expert`. A
   task that cannot be finished is blocked (its pull request stays open) by
   the `orchestrator`; dropped (deleted everywhere) only by the user.
6. **Merge.** The `orchestrator` squashes an approved pull request into
   `main`, deletes the branch, removes worktrees, and runs `tools/install.sh`
   when the request changed what it installs.
7. **Report.** The `liaison` reports to the user in Russian: blocked tasks and
   tasks that took `developer-expert` two rounds first, then the results, the
   tech debt, what to look at.

The `orchestrator` owns the architecture (`docs/architecture.md`); developers
apply its changes to the document in the same pull request as the code. Tech
debt (`docs/tech-debt.md`) changes only in pull requests: a developer may defer
a large unforeseen problem with an entry, the `orchestrator` decides whether it
waits. Requirements to both documents - `.claude/rules/docs.md`.

## Isolation

- Each developer and reviewer works in its own git worktree in
  `.claude/worktrees/` (not in git), on its own branch. The `liaison` and the
  `orchestrator` work in the main checkout.
- Worktrees build into one shared dir `~/.cache/baluk-notes/worktree-target`
  (`tools/lib/worktree-target.sh`): a build from scratch is ~8 min and tens of
  GB. The `orchestrator` removes it between requests when it is over 30 GB.
  The main checkout keeps its own `target/`.
- e2e listens on fixed ports: parallel runs take turns on a lock. The test
  server of an agent takes its own port (`PORT=... tools/test-env.sh`).
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

## Launch

```sh
claude --agent liaison
```

From the project root. A plain `claude` session is an assistant outside the
pipeline: it follows the project rules and does not run the roles.

## Known limits

- Agents run in the background and cannot ask for a permission: `cargo`,
  `npm`, `git`, `tea` and `tools/*.sh` must be allowed in advance (or the
  session runs in auto mode).
- No CI: the agents run `tools/check.sh` themselves.
- Rare cases are not written into the rules: the `orchestrator` handles them
  on the spot and names them in the report.

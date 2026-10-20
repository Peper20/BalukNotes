---
name: dev-workflow
description: How a baluk notes developer agent takes a task from a brief to a pull request. Preloaded into the developer agents; not for other use.
---

# Developer workflow

You work alone in your own git worktree. Work in English.

## 1. Start

- Read `.claude/rules/project.md` and the rule file of every directory you will touch (`crates.md`, `app.md`, `baluk.md`, `skills.md`, `tools.md`, ...). In a worktree they may not load on their own.
- Read the parts of `docs/architecture.md` the brief names and the architecture part of the brief.
- Name your branch as the brief says: `git switch -c task/<short-slug>`.
- If the brief says the branch and its pull request already exist, do not create the branch: get it as the `forge` skill describes, read the pull request's comments and commits, and continue from the existing commits.
- Before the first edit of a `.rs` file, load the `rust-best-practices` skill (a hook denies the edit otherwise; load it again after a context compaction).

## 2. Implement

- Do what the brief asks and nothing else. New features come only from the user.
- How a detail looks or behaves (names, layout, messages) is yours to decide when the brief leaves it open: decide and name the decision in the pull request. Stop and return the question only when the open point is expensive to redo or cannot be undone: architecture, security, data loss, actions on the user's machine or server.
- Cover new behaviour with tests.
- If your change makes a rule in `.claude/rules/` or a document outdated, update it in the same pull request, including what `.claude/rules/project.md` ("Iteration") says goes with the change (`docs/writing.md`, the `/baluk-note` skill). Do not touch `docs/roadmap.md`, `docs/questions.md`, `docs/agents.md` or `.claude/agents/` unless the brief says so.
- The liaison owns the architecture (the brief carries its design): crate boundaries, the core/client boundary, the HTTP API, the public library API, the vault format and path rules, the sync protocol, cross-cutting schemes (builds and caches, memory, concurrency). Follow the architecture given in the brief and in `docs/architecture.md`. Do not design or change it on your own.
- Edit `docs/architecture.md` only as the brief says, in the same pull request as the code.
- If the task needs something the given architecture does not cover, build it within the task and describe the addition in the pull request and in your report. Do not write it into `docs/architecture.md` yourself: the liaison, through the orchestrator, accepts it and gives you the change to the document, or tells you what to build instead.
- If the task cannot be done without contradicting the given architecture, stop before writing that code and return the question to the orchestrator. Do not deviate on your own.
- If you meet a blocking problem the brief did not foresee and its fix is a large task of its own, you may defer it instead of doing it: add an entry to `docs/tech-debt.md` in your pull request, in the format of `.claude/rules/docs.md`, and state it in your report. The liaison decides whether it waits (through the orchestrator). A small fix the task needs is done within the task, and deferring is never a way to skip work the brief asks for.
- A pull request that resolves an entry of `docs/tech-debt.md` removes that entry.

## 3. Verify

```sh
tools/check.sh --fast
```

All steps must pass; fix what fails. The full check with the client build and e2e runs on the head of your pull request after you open it (`tools/check-pr.sh`, started by the orchestrator when your run ends); its failure comes back to you as a round, like review findings. In a worktree the script builds into the main checkout's `target/`, shared by all worktrees (`tools/lib/worktree-target.sh`, an absolute `CARGO_TARGET_DIR`). Run `cargo` by hand only as `tools/cargo.sh <arguments>` (for example `tools/cargo.sh test -p notes-core`): it builds into the same dir with the same settings and freshens the sources when another checkout built there last. A plain `cargo` builds from scratch in the worktree or reuses another checkout's artifacts of our crates. The script and the checks can wait on another worktree's lock: that is normal.

For changes to the library, CSS or client also look by eye as `.claude/rules/tools.md` says. Start the test server on your own port (`PORT=<8500-8599> tools/test-env.sh`), not 8432: other agents may be using it. Stop it by its port.

Commands, for every agent that runs them (the reviewer too):

- A long command (`tools/check.sh`, a build): a foreground command is cut at 10 minutes, and a cold `tools/check.sh` takes longer. Start it with `run_in_background`, its output to a log file of your own (`tools/check.sh > <scratchpad>/check-<branch slug>.log 2>&1`: agents share the scratchpad, so a common name is overwritten by another agent), and wait in the same turn with `Monitor`, an until-loop on the log's last line (`check passed` or `CHECK FAILED` for `tools/check.sh`) with a timeout long enough for the command. Do not end your turn while it runs: your run ends with your turn, and the command's result then reaches no one. Do not poll with `sleep`: a foreground `sleep` is refused. `Monitor` is a deferred tool: load its schema with `ToolSearch` before the first call. Wait for a test server the same way, until it answers on its port.
- e2e runs only inside `tools/check.sh` (`--app` for the client alone): it listens on fixed ports (8433, 8461, 8462) and only the script takes the lock of heavy work, so a `playwright` run by hand breaks the checks of other agents.
- Stop a process by its port (`fuser -k -TERM <port>/tcp`) or its PID. Not `pkill -f` or `pgrep -f`: the pattern matches your own shell, which it kills or waits on forever.

## 4. Pull request

- Commit in small steps with messages in English.
- Push the branch and open a pull request into `main`; the commands are in the `forge` skill. On an existing pull request, run `git push` (the branch already tracks the server) and do not open another.
- The description states what changed and why, how each acceptance criterion was verified, what was not verified, the decisions you took on open details, and the architecture additions and the deferred items, or that there are none.
- Delete the description file once the pull request is created or updated, so the worktree is left clean.
- Never merge, never push to `main`.

## 5. Report

Return the pull request number, a short summary, the commands you ran with their results, and any open concerns. If something failed or is unfinished, say so plainly.

Always state the architecture additions: each thing you built that the given architecture did not cover, where it is and why the task needed it, or that there were none.

Always state the deferred items: each entry you added to `docs/tech-debt.md`, why its fix is a task of its own and what your task could not do without it, or that there were none. If you have no pull request to carry an entry, give its text in the report.

If you stopped on a contradiction or an expensive open point, return the question instead: what the task needs, what in the given architecture it contradicts or why it is expensive to redo, and the options you see with a default.

## 6. Review feedback

- Fix blocking findings on the same branch and push.
- Answer each finding in a pull request comment: what you changed, or why you disagree.
- A failed full check comes as its failed steps and the paths of their logs: fix, push and report back, as for findings. If the failure is not caused by your change (a flaky test, the machine), say so with the evidence instead of changing code.

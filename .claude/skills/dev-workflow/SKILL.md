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
- The orchestrator owns the architecture: crate boundaries, the core/client boundary, the HTTP API, the public library API, the vault format and path rules, the sync protocol, cross-cutting schemes (builds and caches, memory, concurrency). Follow the architecture given in the brief and in `docs/architecture.md`. Do not design or change it on your own.
- Edit `docs/architecture.md` only as the brief or the orchestrator says, in the same pull request as the code.
- If the task needs something the given architecture does not cover, build it within the task and describe the addition in the pull request and in your report. Do not write it into `docs/architecture.md` yourself: the orchestrator accepts it and gives you the change to the document, or tells you what to build instead.
- If the task cannot be done without contradicting the given architecture, stop before writing that code and return the question to the orchestrator. Do not deviate on your own.
- If you meet a blocking problem the brief did not foresee and its fix is a large task of its own, you may defer it instead of doing it: add an entry to `docs/tech-debt.md` in your pull request, in the format of `.claude/rules/docs.md`, and state it in your report. The orchestrator decides whether it waits. A small fix the task needs is done within the task, and deferring is never a way to skip work the brief asks for.
- A pull request that resolves an entry of `docs/tech-debt.md` removes that entry.

## 3. Verify

```sh
tools/check.sh
```

All steps must pass; fix what fails. In a worktree the script builds into a target dir shared by all worktrees (`tools/lib/worktree-target.sh`); prefix your own `cargo` commands with the same `CARGO_TARGET_DIR=~/.cache/baluk-notes/worktree-target`, or they start a build from scratch. A first build there can wait on another worktree's lock: that is normal.

For changes to the library, CSS or client also look by eye as `.claude/rules/tools.md` says. Start the test server on your own port (`PORT=<8500-8599> tools/test-env.sh`), not 8432: other agents may be using it. Stop it by its port.

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

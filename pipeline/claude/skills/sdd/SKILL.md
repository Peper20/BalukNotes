---
name: sdd
description: The behaviour specs of the project - their format, how a brief changes them and how a pull request and its review follow them. Preloaded into the developers and the reviewer; the liaison reads it before writing a brief that changes behaviour.
---

# Behaviour specs

A spec says what the system does, observably, area by area. The architecture document says how it is built; the code and the tests say the rest. A spec is kept true for the code on `main`: it changes in the same pull request as the behaviour.

The specs are in the folder the guide names, by default `docs/specs/`, one file per area (`sync.md`, `paths.md`). An area gets a spec when a task first touches its behaviour; there is no rush to cover the rest.

## Format

```markdown
# Sync

What the area covers, in one or two sentences, and where its code is.

Next id: SYNC-8

## Upload

- **SYNC-1** When a client pushes a file whose base version is not the
  server's current one, the server refuses it with a conflict and changes
  nothing.
- **SYNC-2** ...
```

- One requirement per bullet: an id in bold, then the behaviour as one or two sentences of what a user, a client or a caller can observe: inputs, outputs, errors, limits. Not how it is built, no names of internal functions or types unless they are the public API.
- Ids are `<AREA>-<n>`, stable for the life of the requirement and never reused; `Next id` is the next free number. A changed requirement keeps its id; a removed one is deleted (git keeps it), its number stays used.
- Groups (`##`) by topic, not by task. Plain Markdown, the project's rules for documents apply: each fact once, no history, no plans.

## The spec delta of a brief

A brief that adds, changes or removes behaviour carries the exact change, under **Work**:

```
Spec delta, docs/specs/sync.md:
- add SYNC-8 (group Upload): <text>
- change SYNC-2 to: <text>
- remove SYNC-5
```

The liaison writes it; a brief whose behaviour is not in a spec yet may create the file with the requirements the task touches. A brief that changes no behaviour says "Spec delta: none".

## Developer

- Apply the delta word for word in the pull request with the code. Do not change other requirements.
- Each added or changed requirement has a test that would fail without it. The pull request description lists them: `SYNC-8 - <test name>`, or why a requirement has no automatic test.
- If the code contradicts a requirement outside the delta, or the delta cannot hold as written, do not fix the spec on your own: report it to the liaison (the `[liaison]` ending of the developer workflow).

## Reviewer

- The spec after the pull request says what the code does: every requirement of the delta holds in the code and has its test; no behaviour changed that the delta does not name.
- A mismatch between the code and a requirement of the delta, or a missing test for one, is a blocking finding. A contradiction with a requirement outside the delta is a finding for the liaison: name the id.

---
name: forge
description: Commands for the project's Forgejo server - pull requests, reviews and merging. Preloaded into the agents that use the server; not for other use.
---

# Forge

The project is hosted on a local Forgejo server (`http://localhost:3000`, repository `ibaluk/baluk-notes`; the remote `origin`). The command line client is `tea`. Use `git` for everything git can do and `tea` for the rest. The remote `github` is the public mirror: the server pushes `main` there itself, agents never push to it.

Two accounts, chosen with `--login`. The login is a `tea` name; the server user is shown in reviews and API output:

- `dev` (server user `agent-dev`) opens pull requests: the developers.
- `reviewer` (server user `agent-reviewer`) reviews: the reviewer only.

The accounts are shared with another project on the same server. Pushes go as the repository owner.

| Task | Command |
|---|---|
| Push a branch | `git push -u origin <branch>` |
| Get an existing remote branch | `git fetch origin <branch>`, then `git switch <branch>`: creates the local branch tracking `origin/<branch>` |
| Open a pull request | `tea pulls create --login dev --base main --head <branch> --title "<title>" --description-file <file>`, with the description in a file in your worktree that you do not commit |
| Find a pull request number | `tea pulls --login dev --output simple --fields index,title` |
| Read a pull request with its comments and reviews | `tea pulls --login dev --comments --output json --fields index <number>` |
| Read its reviews only | `tea api --login dev '/repos/{owner}/{repo}/pulls/<number>/reviews'` |
| Check whether it is merged, get its head commit | `tea pulls --login dev --output json --fields index <number>`: `hasMerged` and `headSha` |
| Get the code of a pull request | `git fetch origin pull/<number>/head`, then `git checkout --detach FETCH_HEAD` |
| See its diff | `git fetch origin main`, then `git diff origin/main...HEAD` in that checkout |
| Comment | `tea comment --login <login> <number> '<text>'` |
| Approve | `tea pulls approve --login reviewer <number> '<text>'` |
| Request changes | `tea pulls reject --login reviewer <number> '<text>'` |
| Merge | `tea pulls merge --login dev --style squash <number>` |
| Close without merging | `tea pulls close --login dev <number>` |
| Delete a merged or dropped branch | `git push origin --delete <branch>` |

- Run `git` and `tea` from the root of your own worktree, one command per Bash call. The permission check refuses `git -C` and `cd` into another checkout, `$(...)` or a loop around `git`, and a script that calls `git`.
- Text for a comment, a review or a description: one single-quoted argument with real line breaks (a blank line starts a paragraph, `- ` a bullet). `$'...'` and `$(...)` are refused, and backticks inside double quotes are executed. Write an apostrophe as `'\''`.
- The permission check refuses `tea` text that contains a backtick and the word "git" together, and text fed on stdin (heredoc or pipe) that contains "git" at all. Reword it. A description with such text goes in the file.
- After a review, read it back with the reviews command and check the line breaks are there.
- `tea pulls merge` prints nothing on success. Confirm with `hasMerged`; `headSha` stays correct after the merge and after the branch is deleted. For a bare value: `tea api --login dev '/repos/{owner}/{repo}/pulls/<number>' | jq -r .head.sha` (or `.merged`).
- Plain `tea pulls` output carries terminal escape codes that `NO_COLOR=1` does not remove. Use `--output json` or `--output simple`; `tea comment list` flattens line breaks.
- `main` is protected: a merge needs the approval of `reviewer`, and a new commit makes an earlier review stale.
- Right after a push or an approval the server may answer a merge with "not mergeable" while it is still checking. Wait a few seconds and retry before treating it as a real conflict.
- Do not delete a branch before its pull request is merged or deliberately closed: deleting the branch closes the pull request.
- After a pull request is closed and its branch deleted, `headSha` still shows the last commit.

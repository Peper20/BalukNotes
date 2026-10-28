# The project layer: `.claude/pipeline.json`

The kit knows nothing of the project it runs in. What differs from project to
project is in this file at the project's `.claude/pipeline.json`, and in the
pipeline page it names. A shell reads it with `jq` (or `python3`); the mod
parses it natively (`hooks/config.ts`) at session start. Unknown keys are
ignored. A missing or invalid file is reported once in the lead's session, and
the mod starts no task until it is fixed (the next call of a pipeline tool reads
it anew).

| Key | Type | Default | Meaning |
|---|---|---|---|
| `project` | string | `the project` | The project's name in the texts the mod sends to agents |
| `guide` | string | none | Path of the project's pipeline page. The agents read its section "Project specifics for the agents" first |
| `map` | string | none | Path of the project's map of rules, read first with the guide |
| `forge` | object | required | `{ "url": "http://...", "repo": "owner/name" }`: the Forgejo server; the mod builds the pull request address `<url>/<repo>/pulls/<n>` from it; `ci-runner.sh` registers the runner at `url`, and the cloud Forgejo (`cloud-setup.sh`) is a repository `repo` whose owner account is the owner of `repo` |
| `afterMerge` | object | none (nothing runs) | The command run in the main checkout after a merge that changed the project's files |
| `afterMerge.paths` | list of strings | `[]` | Path prefixes (`"src/"`): a merge that changes a file under one runs the command. Empty: any change |
| `afterMerge.command` | list of strings | required with `afterMerge` | The program and its arguments, no shell |
| `afterMerge.timeoutSeconds` | number | `600` | The command is cut after this long |
| `check` | object | none | The full check run by `scripts/check.sh`; the keys below |
| `check.steps` | string | required by the runner | Path of the steps file (against the project root): it defines the function `steps`, which gets the runner's arguments |
| `check.env` | string | none | Path of a file that `scripts/lib/cap.sh` sources once `CHECK_JOBS` is set: it exports the settings of the project's tools (`below <n>` is `CHECK_JOBS` - n, at least 1) and may define `on_heavy_acquire <changed>`, called under the lock of heavy work (`changed` is 1 for the first holder after another checkout) |
| `check.logs` | string | `.pipeline/check` | The folder of the step logs, `table.txt` and `summary.md`, against the checkout root (keep it out of git). The mod names it to a developer whose check failed |
| `check.heavyLock` | string | `~/.cache/<project name as a word>/heavy.lock` | The lock that makes the heavy work of all runs on the machine take turns; `~/` is the home |
| `ci` | object | none | The CI runner and the check of a pull request (`scripts/ci-runner.sh`, `scripts/check-pr.sh`); the keys below |
| `ci.runner` | string | required by `ci-runner.sh` | The name of the repository runner; its label is `<runner>:host`. The workflow's `runs-on` is this label's name |
| `ci.tools` | string | none | Programs, space-separated, that the project's check calls: `ci-runner.sh install` warns about those missing from the PATH of the runner (it always checks `git`, `tea`, `python3`, `flock`) |
| `ci.staleOutput` | string | none | Paths (against the checkout root), space-separated, that `check-pr.sh` removes from the check worktree when it moves to another head: build output that would be taken for the new head's |
| `tests` | list of objects | none | The rules of `scripts/test.sh`: `{ "match": "<extended regex on a changed path>", "run": "<command>" }`; `$1`..`$9` in the command are the groups of the match |
| `skills` | list of objects | none (nothing is guarded) | The code skills the guard requires before an edit (`docs/README.md`, "The mod"): `{ "match": "<regular expression on a path>", "skill": "<skill name>", "rules": "<path>" }`. `match` is a JavaScript regular expression tried on the path, against the project root, of every file an edit writes (a file in an agent's worktree, `.claude/worktrees/<name>/`, against that worktree, so a pattern means the same in every checkout); `skill` is the skill to load; `rules` (optional) is the project's rules for the language, named in the refusal. A bad regular expression is a config error. The guard reads this key itself, so it works when the rest of the file is unusable |
| `targetedTests` | string | none | The hint on running the targeted tests that the developer contract shows, `(<hint>)` after "the targeted tests your change needs" |

The pipeline page named by `guide` has a section "Project specifics for the
agents". The kit's agents and skills defer to it for everything that is not the
same in every project: the commands for tests, builds and the full check, the
places to look by eye, the user's language, the documents the lead owns and
commits itself, the areas of the architecture, the project's rules for new
features and questions, deployments and what no agent touches.

Example:

```json
{
  "project": "widgets",
  "guide": "docs/pipeline.md",
  "map": "docs/rules.md",
  "forge": { "url": "http://forge.example.com", "repo": "acme/widgets" },
  "afterMerge": {
    "paths": ["src/", "lib/"],
    "command": ["scripts/install.sh"],
    "timeoutSeconds": 600
  },
  "check": {
    "steps": "scripts/check-steps.sh",
    "env": "scripts/cap-env.sh",
    "logs": "out/check"
  },
  "ci": { "runner": "widgets-host", "tools": "make node", "staleOutput": "web/dist" },
  "tests": [
    { "match": "^src/([^/]+)/", "run": "make test T=$1" }
  ],
  "skills": [
    { "match": "\\.rs$", "skill": "rust", "rules": "docs/rust.md" }
  ],
  "targetedTests": "pipeline/scripts/test.sh"
}
```

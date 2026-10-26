# The project layer: `.claude/pipeline.json`

The kit knows nothing of the project it runs in. What differs from project to
project is in this file at the project's `.claude/pipeline.json`, and in the
pipeline page it names. A shell reads it with `jq`; the mod parses it natively
(`hooks/config.ts`) at session start. Unknown keys are ignored. A missing or
invalid file is reported once in the liaison's session, and the mod starts no
task until it is fixed (the next call of a pipeline tool reads it anew).

| Key | Type | Default | Meaning |
|---|---|---|---|
| `project` | string | `the project` | The project's name in the texts the mod sends to agents |
| `guide` | string | none | Path of the project's pipeline page. The agents read its section "Project specifics for the agents" first |
| `map` | string | none | Path of the project's map of rules, read first with the guide |
| `forge` | object | required | `{ "url": "http://...", "repo": "owner/name" }`: the Forgejo server; the mod builds the pull request address `<url>/<repo>/pulls/<n>` from it |
| `afterMerge` | object | none (nothing runs) | The command run in the main checkout after a merge that changed the project's files |
| `afterMerge.paths` | list of strings | `[]` | Path prefixes (`"src/"`): a merge that changes a file under one runs the command. Empty: any change |
| `afterMerge.command` | list of strings | required with `afterMerge` | The program and its arguments, no shell |
| `afterMerge.timeoutSeconds` | number | `600` | The command is cut after this long |
| `targetedTests` | string | none | The hint on running the targeted tests that the developer contract shows, `(<hint>)` after "the targeted tests your change needs" |

The pipeline page named by `guide` has a section "Project specifics for the
agents". The kit's agents and skills defer to it for everything that is not
the same in every project: the commands for tests, builds and the full check,
the places to look by eye, the user's language, the documents the liaison owns
and commits itself, the areas of the architecture, the project's rules for
new features and questions, deployments and what no agent touches.

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
  "targetedTests": "make test T=<name>"
}
```

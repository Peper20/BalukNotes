---
paths:
  - "skills/**"
---

# BalukNotes skills

Claude Code skills installed together with the program: `tools/install.sh`
puts each into `~/.claude/skills/<name>` in one version with `notes`. This
file - the principles of any skill.

| Skill | What it does |
|---|---|
| `baluk-note/` | `/baluk-note`: a new note or book, a chapter, an edit, a rename |

## Who we write for

- **The reader is a model**, mostly a strong one (Sonnet, Opus); a weak one
  must cope too, its context starts at ~64k tokens. So rules come **with
  reasons** (with a reason a strong model handles an unforeseen case), and
  samples and the reference back up a weak one.
- **Strict - only the process**: paths, commands, checks, the file header,
  boundaries. Content is free: a sample is a starting point, not a limit.
- A calm tone: no NEVER, caps or extra emphasis - they cause overcaution
  (extra questions, dropping reasonable initiative).

## Self-sufficiency

- The skill is called **from any folder**, usually the one with the sources
  (lectures, problems). Everything needed is in the skill directory:
  `SKILL.md`, samples, the reference; BalukNotes sources and online docs are
  not needed, and the skill says so directly.
- Work goes only through the **public `notes` command** and editing note
  files. A capability is missing - first a CLI command, then the skill.
- `allowed-tools` - only the needed `notes` commands and reading files.

## User data

- A vault holds the user's own notes. The skill works **only in the chosen
  vault**. There is no default vault: not named - ask (even if there is one
  vault); the skill itself does not create vaults.
- Only what the task is about is created and changed; a reason to change
  something else (a link in another note, a typo) - ask first.
- The user's sources are read only, nothing is created in their folders.
  Temporary files (PDF, screenshots) go to the session's temporary folder.

## Language and style

- Texts for the model (`SKILL.md`, the reference, samples) are **in English**:
  fewer tokens and less ambiguity.
- At the start and at the end of `SKILL.md` - a point about language: the
  answer and the note are in the language of the request (for an edit - the
  note's language); a Russian request - a Russian note, although the
  instructions are English.
- Quote `notes` and library messages **verbatim**: the model looks for them in
  the output.
- **Plain Markdown** without typography: `"`, `-`, `->`, `...`. Tables -
  where things are worse without them.
- For people - only `argument-hint` (a hint in the interface), it is in
  Russian.

## Tokens

- Measure size with `tiktoken` (`o200k_base`) before and after an edit -
  approximate (Claude has its own tokenizer), but fast and local:

  ```sh
  python3 -c 'import sys, tiktoken; e = tiktoken.get_encoding("o200k_base"); [print(len(e.encode(open(f).read())), f) for f in sys.argv[1:]]' skills/baluk-note/SKILL.md
  ```

- No filler or repeats, but **reasons for rules and completeness matter more
  than savings**.
- Do not write token counts into the skill text: the model does not need them
  and they go stale.
- Large and not always needed content goes into a separate file that
  `SKILL.md` asks to read when needed.

## Skill layout

```
<name>/
  SKILL.md       the workflow, rules with reasons, common mistakes
  reference.md   full signatures and allowed values (if there is an API)
  examples/      samples: whole files that build without errors
```

`SKILL.md`:
- the header: `name`, `description` (in English), `disable-model-invocation:
  true` (the user calls the skill), `argument-hint`, `allowed-tools`;
- the user's request is `$ARGUMENTS` in `<request>` tags (on separate lines)
  at the start and once more at the end: the model separates the task from the
  instructions, and the task is not lost in a long prompt. Do not write `$`
  with a digit: the shell replaces `$0` with a word of the call;
- the progress header (goal, plan, what next) is written by the model per the
  skill, not by the `notes new` template (user's decision);
- order: language, context (what the program is, where the result goes),
  choosing the task, steps, reference (paths, syntax, CLI errors), quality
  rules, common calls, an index of samples.

Together the samples use **every** public library name; comments say what a
call does and why (parameters are in the reference). The model reads them in
full before planning, to know what the library can do.

## Checks

`tools/check.sh --rust`:

- the `baluk-note` step: `notes new` templates, all samples and the "Common
  calls" block of `SKILL.md` build in a temporary vault without errors,
  warnings or broken links;
- `cargo test -p notes-core --test it library::`: `#name` calls in `SKILL.md`
  are public names; every name is in the samples; the reference matches the
  sources in named parameters and defaults; there is `<request>` with
  `$ARGUMENTS` and no `$digit`; the skill texts have no Cyrillic or typography
  (except inline `code` and `argument-hint`); the skill size in characters
  grew by no more than 10 % over `tests/snapshots/skill-size.txt` (an
  intended growth - `UPDATE_SNAPSHOTS=1 cargo test -p notes-core --test
  library`).

## A new skill

1. A directory `skills/<name>/` with `SKILL.md` per the sections above.
2. Installation - in `tools/install.sh` (replace entirely with a copy, as
   `baluk-note`).
3. Building the samples - a step in `tools/check.sh`, text checks - a test
   next to `library.rs`.
4. A row in the table above.

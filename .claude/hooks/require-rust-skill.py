#!/usr/bin/env python3
"""PreToolUse hook: Rust edits only after the rust-best-practices skill.

An edit is Edit/Write/MultiEdit of a .rs file or a Bash command that writes
to a .rs file: one part of the command (between ;, &&, ||, | and new lines)
both writes (sed -i, perl -i, tee, a write from python) and names a .rs file,
or the command redirects into one (> file.rs). A part that only mentions a .rs
file while writing another file (a document about the code) is not an edit;
nor is a file outside the project (a scratch .rs in /tmp). The
skill counts as loaded when the transcript has a Skill call with it or the
/rust-best-practices command after the last context compaction: a compaction
summarizes the skill text away, so it is loaded again in full. A subagent
(agent_id) has its own context and its own transcript, next to the session's
(transcript_path): <session>/subagents/agent-<id>.jsonl - the skill has to be
loaded there, the session's load does not count. Otherwise the edit is denied
with an explanation. A failure of the hook itself never blocks the edit.
"""

import json
import os
import re
import sys

SKILL = "rust-best-practices"

# Signs of writing a file in one part of a shell command (reading - grep, sed -n, cat - is not).
WRITES = re.compile(
    r"sed\s+(-[a-zA-Z]*\s+)*-[a-zA-Z]*i"
    r"|perl\s+-[a-zA-Z]*i"
    r"|\btee\b"
    r"|\.write\(|write_text\(|open\([^)]*['\"][wa]"
)

# A redirect into a .rs file: a write by itself.
REDIRECT = re.compile(r">>?\s*['\"]?([^\s'\"|;&<>]*\.rs)\b")

# A .rs path in a command.
RUST_PATH = re.compile(r"([^\s'\"|;&<>()=,]*\.rs)\b")

# Where one part of a shell command ends.
PARTS = re.compile(r"&&|\|\||[;|\n]")

PROJECT = os.path.realpath(os.environ.get("CLAUDE_PROJECT_DIR") or os.getcwd())


def in_project(path: str, cwd: str) -> bool:
    """Whether the path (relative to the command's cwd) is inside the project."""
    full = os.path.realpath(os.path.join(cwd, os.path.expanduser(path)))
    return full == PROJECT or full.startswith(PROJECT + os.sep)


def bash_writes_rust(command: str, cwd: str) -> bool:
    if any(in_project(path, cwd) for path in REDIRECT.findall(command)):
        return True
    return any(
        WRITES.search(part) and any(in_project(path, cwd) for path in RUST_PATH.findall(part))
        for part in PARTS.split(command)
    )


def touches_rust(tool: str, data: dict, cwd: str) -> bool:
    if tool in ("Edit", "Write", "MultiEdit"):
        path = str(data.get("file_path", ""))
        return path.endswith(".rs") and in_project(path, cwd)
    if tool == "Bash":
        return bash_writes_rust(str(data.get("command", "")), cwd)
    return False


def loads_skill(line: str) -> bool:
    if SKILL not in line:
        return False
    if '"name":"Skill"' in line and f'"skill":"{SKILL}"' in line:
        return True
    return f"<command-name>/{SKILL}</command-name>" in line


def skill_loaded(transcript: str) -> bool:
    """The skill is loaded since the last compaction (or since the start)."""
    loaded = False
    with open(transcript, encoding="utf-8", errors="replace") as f:
        for line in f:
            if COMPACTION in line:
                loaded = False
            elif loads_skill(line):
                loaded = True
    return loaded


def own_transcript(event: dict) -> str | None:
    """The transcript of whoever makes the call: the subagent's, else the session's."""
    session = event.get("transcript_path")
    agent = str(event.get("agent_id") or "")
    if not session or not re.fullmatch(r"[\w-]+", agent):
        return session
    path = os.path.join(session.removesuffix(".jsonl"), "subagents", f"agent-{agent}.jsonl")
    return path if os.path.isfile(path) else session


def main() -> None:
    event = json.load(sys.stdin)
    cwd = str(event.get("cwd") or os.getcwd())
    if not touches_rust(event.get("tool_name", ""), event.get("tool_input") or {}, cwd):
        return
    transcript = own_transcript(event)
    if not transcript or skill_loaded(transcript):
        return
    print(
        json.dumps(
            {
                "hookSpecificOutput": {
                    "hookEventName": "PreToolUse",
                    "permissionDecision": "deny",
                    "permissionDecisionReason": (
                        f"Rust edits need the {SKILL} skill loaded (again after a context compaction): "
                        f'call Skill "{SKILL}" and read .claude/rules/crates.md (project rules), then retry the edit.'
                    ),
                }
            }
        )
    )


if __name__ == "__main__":
    try:
        main()
    except Exception:  # noqa: BLE001 - a hook failure must not stop the work
        pass

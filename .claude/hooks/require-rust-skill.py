#!/usr/bin/env python3
"""PreToolUse hook: Rust edits only after the rust-best-practices skill.

An edit is Edit/Write/MultiEdit of a .rs file or a Bash command that writes
to a .rs file (sed -i, perl -i, > file.rs, tee, a write from python). The
skill counts as loaded when the session transcript (transcript_path) has a
Skill call with it or the /rust-best-practices command after the last context
compaction: a compaction summarizes the skill text away, so it is loaded again
in full. Otherwise the edit is denied with an explanation. A failure of the
hook itself never blocks the edit.
"""

import json
import re
import sys

SKILL = "rust-best-practices"

# Signs of writing a file in a shell command (reading - grep, sed -n, cat - is not).
WRITES = re.compile(
    r"sed\s+(-[a-zA-Z]*\s+)*-[a-zA-Z]*i"
    r"|perl\s+-[a-zA-Z]*i"
    r"|>>?\s*['\"]?[^\s'\"|;&]*\.rs\b"
    r"|\btee\b"
    r"|\.write\(|write_text\(|open\([^)]*['\"][wa]"
)

# A context compaction in the transcript (a system record).
COMPACTION = '"subtype":"compact_boundary"'


def touches_rust(tool: str, data: dict) -> bool:
    if tool in ("Edit", "Write", "MultiEdit"):
        return str(data.get("file_path", "")).endswith(".rs")
    if tool == "Bash":
        command = str(data.get("command", ""))
        return re.search(r"\.rs\b", command) is not None and WRITES.search(command) is not None
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


def main() -> None:
    event = json.load(sys.stdin)
    if not touches_rust(event.get("tool_name", ""), event.get("tool_input") or {}):
        return
    transcript = event.get("transcript_path")
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
                        f'call Skill "{SKILL}" and read crates/README.md (project rules), then retry the edit.'
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

#!/usr/bin/env python3
"""Хук PreToolUse: правка .rs - только после навыка rust-best-practices.

Правка - Edit/Write/MultiEdit файла .rs или команда Bash, которая пишет в .rs
(sed -i, perl -i, > файл.rs, tee, запись из python). Навык загружен - в
журнале сессии (transcript_path) есть вызов Skill с ним или команда
/rust-best-practices. Нет - правка запрещается с объяснением. Сбой самого
хука правку не останавливает.
"""

import json
import re
import sys

SKILL = "rust-best-practices"

# Признаки записи в файл в команде оболочки (чтение - grep, sed -n, cat - не они).
WRITES = re.compile(
    r"sed\s+(-[a-zA-Z]*\s+)*-[a-zA-Z]*i"
    r"|perl\s+-[a-zA-Z]*i"
    r"|>>?\s*['\"]?[^\s'\"|;&]*\.rs\b"
    r"|\btee\b"
    r"|\.write\(|write_text\(|open\([^)]*['\"][wa]"
)


def touches_rust(tool: str, data: dict) -> bool:
    if tool in ("Edit", "Write", "MultiEdit"):
        return str(data.get("file_path", "")).endswith(".rs")
    if tool == "Bash":
        command = str(data.get("command", ""))
        return re.search(r"\.rs\b", command) is not None and WRITES.search(command) is not None
    return False


def skill_loaded(transcript: str) -> bool:
    with open(transcript, encoding="utf-8", errors="replace") as f:
        for line in f:
            if SKILL not in line:
                continue
            if '"name":"Skill"' in line and f'"skill":"{SKILL}"' in line:
                return True
            if f"<command-name>/{SKILL}</command-name>" in line:
                return True
    return False


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
                        f"Правка Rust - только после навыка {SKILL}: вызовите Skill "
                        f'"{SKILL}" и прочитайте crates/README.md (правила проекта), затем повторите правку.'
                    ),
                }
            },
            ensure_ascii=False,
        )
    )


if __name__ == "__main__":
    try:
        main()
    except Exception:  # noqa: BLE001 - сбой хука не должен останавливать работу
        pass

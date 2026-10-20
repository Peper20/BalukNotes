#!/usr/bin/env python3
"""Sizes of the sections of a .wasm module (raw and brotli), for E4.

Usage: sections.py module.wasm
"""

import shutil
import subprocess
import sys

NAMES = {0: "custom", 1: "type", 2: "import", 3: "function", 4: "table", 5: "memory", 6: "global",
         7: "export", 8: "start", 9: "element", 10: "code", 11: "data", 12: "datacount"}


def leb(data, pos):
    result, shift = 0, 0
    while True:
        byte = data[pos]
        pos += 1
        result |= (byte & 0x7F) << shift
        shift += 7
        if byte < 0x80:
            return result, pos


def brotli_size(data):
    if not shutil.which("brotli"):
        return None
    out = subprocess.run(["brotli", "-c", "-q", "11"], input=data, capture_output=True, check=True).stdout
    return len(out)


def main():
    data = open(sys.argv[1], "rb").read()
    pos = 8
    rows = []
    while pos < len(data):
        kind = data[pos]
        size, body = leb(data, pos + 1)
        name = NAMES.get(kind, str(kind))
        if kind == 0:
            n, p = leb(data, body)
            name = "custom:" + data[p:p + n].decode(errors="replace")
        rows.append((name, data[body:body + size]))
        pos = body + size
    total_br = brotli_size(data)
    print(f"{'section':<28}{'raw MB':>10}{'brotli MB':>12}")
    for name, chunk in rows:
        if len(chunk) < 10000:
            continue
        br = brotli_size(chunk)
        print(f"{name:<28}{len(chunk) / 1e6:>10.2f}{(br or 0) / 1e6:>12.2f}")
    print(f"{'total':<28}{len(data) / 1e6:>10.2f}{(total_br or 0) / 1e6:>12.2f}")


if __name__ == "__main__":
    main()

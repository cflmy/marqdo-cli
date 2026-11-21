#!/usr/bin/env python3
"""Emit GitHub check annotations from a cargo-test log.

Native jobs (Windows/macOS) and the Linux gate tee `cargo test --workspace` to
a file. Public check-run annotations are then the only failure detail visible
without repository-admin log access. ANSI colour codes are stripped first so
anchored patterns still match.
"""

import re
import sys

MAX_ANNOTATIONS = 60
ANSI = re.compile(r"\x1b\[[0-9;]*m")


def escape(text: str) -> str:
    return text.replace("%", "%25").replace("\r", "%0D").replace("\n", "%0A")


def interesting_lines(lines: list[str]) -> list[tuple[int, str]]:
    selected: list[tuple[int, str]] = []
    seen: set[int] = set()

    def add(index: int) -> None:
        if 0 <= index < len(lines) and index not in seen:
            seen.add(index)
            selected.append((index, lines[index]))

    for index, line in enumerate(lines):
        if "panicked at" in line:
            for offset in range(-2, 19):
                add(index + offset)
        elif line.startswith("failures:") or line.startswith("---- "):
            for offset in range(0, 6):
                add(index + offset)
        elif re.match(r"^error(\[|:)", line):
            for offset in range(-1, 7):
                add(index + offset)
        elif "test " in line and line.endswith("FAILED"):
            add(index)
    return sorted(selected)


def main(argv: list[str]) -> int:
    if len(argv) < 2:
        print("usage: annotate_workspace_tests.py <log> [title]", file=sys.stderr)
        return 2
    log = sys.argv[1]
    title = sys.argv[2] if len(argv) > 2 else "Workspace tests"
    try:
        text = open(log, encoding="utf-8", errors="replace").read()
    except OSError as error:
        print(f"::error title={title}::cannot read test log: {error}")
        return 0
    lines = ANSI.sub("", text).splitlines()
    for _, line in interesting_lines(lines)[:MAX_ANNOTATIONS]:
        print(f"::error title={title}::{escape(line)}")
    return 0


if __name__ == "__main__":
    raise SystemExit(main(sys.argv))

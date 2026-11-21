#!/usr/bin/env python3
"""Review aid for the RFC-LANG-0002 migration (NOT a parser).

Checks migrated `.mq.md` files for common mistakes:
  * unclosed/unmatched if/else/while/end inside an executable segment;
  * `if`/`while`/`return` without an expression; `else`/`end` with trailing text;
  * semicolon-separated statements in one payload;
  * unknown `sem.*` codes in negative expected JSON.

It intentionally ignores Knowledge sections and does not try to be a
normative validator; the Rust frontend migration must provide the real
validation. Usage: python3 tools/dev-only/check-mq-syntax.py <dir>...
"""
from __future__ import annotations
import json
import re
import sys
from pathlib import Path

KNOWN_SEM = {
    "sem.invalid_mode_marker", "sem.empty_unit_name", "sem.duplicate_unit_name", "sem.exec_without_function",
    "sem.payload_nested", "sem.empty_payload", "sem.syntax", "sem.unmatched_else",
    "sem.duplicate_else", "sem.unmatched_end", "sem.unclosed_control",
    "sem.signature_position", "sem.unsupported_version_directive",
    "sem.no_entry_function", "sem.inferred_signature",
}
HEADING = re.compile(r"^(#{1,6})\s+(.*)$")
MODE = re.compile(r"^@(exec|knowledge)\b\s*(.*)$")


def split_semicolons(text: str) -> list[str]:
    """Split a payload on `;` outside text literals and backtick spans."""
    parts, current = [], []
    in_text = in_code = False
    escaped = False
    for ch in text:
        if escaped:
            current.append(ch)
            escaped = False
            continue
        if ch == "\\" and in_text:
            current.append(ch)
            escaped = True
            continue
        if ch == '"' and not in_code:
            in_text = not in_text
        elif ch == "`" and not in_text:
            in_code = not in_code
        if ch == ";" and not in_text and not in_code:
            parts.append("".join(current))
            current = []
            continue
        current.append(ch)
    parts.append("".join(current))
    return parts


def payload_of(line: str):
    s = line.strip()
    while True:
        n = re.sub(r"^(?:>|[-+*]\s+|\d+[.)]\s+)", "", s).strip()
        if n == s:
            break
        s = n
    if len(s) >= 4 and s.startswith("**") and s.endswith("**") and "**" not in s[2:-2]:
        return s[2:-2]
    if len(s) >= 2 and s.startswith("*") and s.endswith("*") and not s.startswith("**") and "*" not in s[1:-1]:
        return s[1:-1]
    m = re.match(r"^(`+)(.*)\1$", s, re.S)
    if m:
        return m.group(2)
    return None


def check(path: Path) -> list[str]:
    # AST fixtures pin the native Markdown profile, not the language mapping;
    # several intentionally contain empty/invalid carriers as raw Markdown.
    if "conformance/ast" in path.as_posix():
        return []
    # Negative fixtures intentionally contain invalid declarations and broken
    # control stacks; their expected diagnostics are asserted by the Rust
    # negative harness, so this review aid only checks them structurally.
    expected_invalid = "tests/negative" in path.as_posix()
    errors: list[str] = []
    stack: list[tuple[str, int]] = []
    headings: list[tuple[int, str | None, int]] = []  # level, explicit mode
    segment = "root"

    def effective(level: int) -> str:
        for hl, mode, _ln in reversed(headings):
            if hl < level:
                if mode:
                    return mode
                # continue to a higher explicit ancestor
        return "knowledge"

    def close_segment(line: int) -> None:
        nonlocal stack
        if stack:
            kind, op = stack[-1]
            errors.append(f"{path}:{line}: unclosed {kind} from line {op} in {segment}")
            stack = []

    for i, raw in enumerate(path.read_text(encoding="utf-8").splitlines(), 1):
        m = HEADING.match(raw)
        if m:
            close_segment(i)
            level, body = len(m.group(1)), m.group(2)
            mm = MODE.match(body)
            mode = mm.group(1) if mm else None
            name = mm.group(2).strip() if mm else body.strip()
            if mm and not name and not expected_invalid:
                errors.append(f"{path}:{i}: mode without heading name")
            if not mm and re.match(r"^@[A-Za-z]", name) and not expected_invalid:
                errors.append(f"{path}:{i}: unknown mode marker {name.split()[0]}")
            while headings and headings[-1][0] >= level:
                headings.pop()
            headings.append((level, mode, i))
            segment = f"H{level} {name}"
            continue
        text = payload_of(raw)
        if text is None:
            continue
        text = text.strip()
        if not text:
            errors.append(f"{path}:{i}: empty executable payload")
            continue
        owning = False
        for hl, _mode, _ln in reversed(headings):
            if hl == 2:
                owning = effective(2) == "exec"
                break
        if not owning:
            continue
        if len(split_semicolons(text)) > 1:
            errors.append(f"{path}:{i}: semicolon-separated statements in one payload: {text!r}")
        first = text.split(None, 1)[0]
        if first in ("if", "while"):
            if len(text.split()) == 1:
                errors.append(f"{path}:{i}: {first} without expression")
            stack.append((first, i))
        elif first == "else":
            if text != "else":
                errors.append(f"{path}:{i}: else with trailing tokens")
            if not stack or not stack[-1][0].startswith("if"):
                errors.append(f"{path}:{i}: unmatched else")
            elif stack[-1][0] == "if_else":
                errors.append(f"{path}:{i}: duplicate else")
            else:
                stack[-1] = ("if_else", stack[-1][1])
        elif first == "end":
            if text != "end":
                errors.append(f"{path}:{i}: end with trailing tokens")
            if not stack:
                errors.append(f"{path}:{i}: unmatched end")
            else:
                stack.pop()
        elif first == "return" and len(text.split()) == 1:
            errors.append(f"{path}:{i}: return without expression")
        elif first == "params" and not re.match(r"^params\s*\(.*\)$", text, re.S):
            errors.append(f"{path}:{i}: malformed params")
    close_segment(len(headings) + 1)
    return errors


def main(argv: list[str]) -> int:
    roots = [Path(a) for a in argv] or [Path("tests"), Path("examples")]
    failures: list[str] = []
    count = 0
    for root in roots:
        paths = [root] if root.is_file() else sorted(root.rglob("*"))
        for path in paths:
            if path.name.endswith(".mq.md"):
                count += 1
                failures.extend(check(path))
            elif path.suffix == ".json" and "negative" in path.as_posix():
                try:
                    data = json.loads(path.read_text(encoding="utf-8"))
                except Exception as error:  # noqa: BLE001 - review aid
                    failures.append(f"{path}: invalid JSON: {error}")
                    continue
                for code in data.get("codes", []):
                    if code.startswith("sem.") and code not in KNOWN_SEM:
                        failures.append(f"{path}: unknown sem code {code}")
    for failure in failures:
        print(failure)
    print(f"checked {count} .mq.md files, {len(failures)} issue(s)")
    return 1 if failures else 0


if __name__ == "__main__":
    sys.exit(main(sys.argv[1:]))

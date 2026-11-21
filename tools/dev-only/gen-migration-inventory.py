#!/usr/bin/env python3
"""Generate docs/migration/semantic-rewrite-inventory.tsv.

The inventory classifies every git-tracked file for the RFC-LANG-0002
single-syntax migration. It is a repository audit aid, not a compiler and not
a substitute for `cargo test` or the real frontend.
"""
from __future__ import annotations
import re
import subprocess
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parents[2]
OLD_MARKERS = re.compile(
    r"^##\s+[^@\s].*$|^\*[^*\n].*\*$|^\*\*[^*\n].*\*\*$|"
    r"markdown\.inert_executable|markdown\.invalid_construct|"
    r"marqdo_semantics|--legacy|map_v1",
    re.M,
)
NEW_MARKERS = re.compile(r"@exec|@knowledge")
FRONTEND = {
    "crates/marqdo-frontend/src/units.rs",
    "crates/marqdo-frontend/src/map.rs",
    "crates/marqdo-frontend/src/payload.rs",
    "crates/marqdo-frontend/src/signature.rs",
    "crates/marqdo-frontend/src/analyze.rs",
    "crates/marqdo-expression/src/lexer.rs",
    "crates/marqdo-expression/src/parser.rs",
    "crates/marqdo-expression/src/ast.rs",
    "crates/marqdo-ir/src/lib.rs",
}


def tracked() -> list[str]:
    """Tracked files plus untracked-but-not-ignored files.

    The migration adds new fixtures/docs before they are committed; an inventory
    that only reads `git ls-files` would silently omit them.
    """
    listed = subprocess.run(
        ["git", "ls-files"], cwd=ROOT, check=True, text=True, capture_output=True
    ).stdout.splitlines()
    untracked = subprocess.run(
        ["git", "ls-files", "--others", "--exclude-standard"],
        cwd=ROOT,
        check=True,
        text=True,
        capture_output=True,
    ).stdout.splitlines()
    return sorted({line for line in listed + untracked if line})


def deleted() -> set[str]:
    out = subprocess.run(
        ["git", "ls-files", "--deleted"], cwd=ROOT, check=True, text=True, capture_output=True
    ).stdout
    return {line for line in out.splitlines() if line}


def changed() -> set[str]:
    out = subprocess.run(
        ["git", "status", "--porcelain"], cwd=ROOT, check=True, text=True, capture_output=True
    ).stdout
    result = set()
    for line in out.splitlines():
        if len(line) > 3:
            result.add(line[3:].strip())
    return result


def classify(path: str, changed_set: set[str]) -> tuple[str, str, str, str, str, str, str]:
    """Return category, active, old_semantics, action, owner, verification, status."""
    p = Path(path)
    text = ""
    full = ROOT / path
    if full.is_file() and full.stat().st_size < 2_000_000:
        try:
            text = full.read_text(encoding="utf-8")
        except UnicodeDecodeError:
            text = ""
    is_changed = path in changed_set

    if path == "docs/adr/0052-unified-markdown-semantics.md":
        return ("decision", "yes", "old mapping", "adopt current language decision", "docs", "review", "migrated")
    if path == "docs/adr/README.md":
        return ("decision", "yes", "n/a", "index current decisions/history", "docs", "review", "migrated")
    if path == "docs/evidence/2026-11-21-semantic-docs-migration.md":
        return ("evidence", "yes", "old syntax", "record migration evidence", "docs", "docs_consistency", "migrated")
    if path.startswith("docs/adr/"):
        return ("historical", "no", "decision at its date", "banner if syntax-role", "docs", "review", "historical")
    if path.startswith("docs/history/") or path.startswith("docs/evidence/"):
        return ("historical", "no", "archived", "banner only", "docs", "review", "historical")
    if path.startswith("docs/migration/"):
        return ("migration", "yes", "n/a", "maintain inventory/guide", "docs", "review", "migrated")
    if path.startswith("docs/architecture/RFC-LANG-0002"):
        return ("architecture", "yes", "old mapping", "adopt as sole authority", "docs", "review", "migrated")
    if path.startswith("docs/architecture/Marqdo_CLI_Latest"):
        return ("architecture", "no", "review input", "historical banner", "docs", "review", "historical")
    if path.startswith("docs/architecture/"):
        return ("architecture", "yes", "status/index", "update to RFC-LANG-0002", "docs", "docs_consistency", "migrated")
    if path.startswith("docs/MARQDO_COMPUTE"):
        return ("architecture", "yes", "compute RFC", "migrate examples / mark historical spelling", "docs", "review", "migrated")
    if path.startswith("docs/"):
        if is_changed:
            return ("tutorial", "yes", "old mapping", "rewrite + historical banner where needed", "docs", "docs_consistency", "migrated")
        return ("tutorial", "yes", "none", "reviewed", "docs", "review", "reviewed")
    if path.startswith("spec/"):
        return ("spec", "yes", "old mapping", "in-place rewrite", "language", "cargo test / review", "migrated")
    if path.endswith(".mq.md"):
        if path.startswith("stdlib/"):
            return ("stdlib", "yes", "old syntax", "migrate with frontend + digest", "stdlib", "stdlib_product", "pending-implementation")
        if path.startswith("tests/conformance/ast/"):
            return ("test", "yes", "Markdown AST only", "keep AST baseline", "tests", "conformance_ast", "reviewed")
        if path.startswith("tests/negative/"):
            return ("test", "yes", "old negative shape", "rebuild for sem.* diagnostics", "tests", "negative", "migrated")
        if path.startswith("examples/"):
            return ("example", "yes", "old syntax", "migrate source", "examples", "examples_r6 / run", "migrated")
        if NEW_MARKERS.search(text):
            return ("test", "yes", "old syntax", "migrate source", "tests", "conformance/negative", "migrated")
        return ("test", "yes", "old syntax", "needs migration", "tests", "review", "pending")
    if path.startswith("tests/"):
        if is_changed:
            return ("test", "yes", "old syntax", "migrate source/expectations", "tests", "negative/properties", "migrated")
        return ("test", "yes", "harness", "review", "tests", "review", "reviewed")
    if path.startswith("crates/"):
        if path in FRONTEND:
            return ("rust-source", "yes", "old mapping", "implement RFC-LANG-0002", "language", "cargo test", "pending-implementation")
        if "/tests/" in path or path.endswith("/tests.rs"):
            return ("rust-test", "yes", "embedded old syntax", "migrate embedded .mq.md strings", "language", "cargo test", "migrated" if is_changed else "reviewed")
        if is_changed:
            return ("rust-source", "yes", "embedded text/help", "migrate embedded .mq.md text only", "language", "cargo test", "migrated")
        return ("rust-source", "yes", "not syntax-bearing", "review", "language", "cargo test", "reviewed")
    if path.startswith("stdlib/"):
        return ("stdlib", "yes", "old syntax", "migrate with frontend + digest", "stdlib", "stdlib_product", "pending-implementation")
    if path.startswith("tools/"):
        return ("tooling", "yes", "migration aid", "review", "docs", "review", "reviewed")
    if path.startswith("examples/"):
        return ("example", "yes", "old syntax", "review", "examples", "run", "reviewed")
    if path in {"README.md", "COMPATIBILITY.md", "CHANGELOG.md", "AGENTS.md"}:
        return ("tutorial", "yes", "old mapping", "rewrite", "docs", "review", "migrated")
    return ("non-semantic", "no", "none", "none", "-", "n/a", "reviewed")


def main() -> int:
    changed_set = changed()
    deleted_set = deleted()
    rows = []
    for path in tracked():
        if path in deleted_set:
            continue
        rows.append((path, *classify(path, changed_set)))
    rows.sort(key=lambda row: row[0])
    out = ROOT / "docs/migration/semantic-rewrite-inventory.tsv"
    with out.open("w", encoding="utf-8", newline="\n") as handle:
        handle.write(
            "path\tcategory\tactive\told_semantics\taction\towner\tverification\tstatus\n"
        )
        for row in rows:
            handle.write("\t".join(row) + "\n")
    counts: dict[str, int] = {}
    for row in rows:
        counts[row[-1]] = counts.get(row[-1], 0) + 1
    print(f"wrote {out} ({len(rows)} rows)")
    for status, count in sorted(counts.items()):
        print(f"  {status}: {count}")
    return 0


if __name__ == "__main__":
    sys.exit(main())

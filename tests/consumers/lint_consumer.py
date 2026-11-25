#!/usr/bin/env python3
"""Lint-like independent consumer of public references and diagnostics.

Reports unresolved/ambiguous/out-of-scope links with their exact module and
source span. It never reads a private AST/IR and never fixes a reference.

usage: lint_consumer.py ANALYSIS.json
"""
import json
import sys


def line_col(source, span):
    prefix = source.encode("utf-8")[: span["start"]].decode("utf-8")
    line = prefix.count("\n") + 1
    column = len(prefix.rsplit("\n", 1)[-1]) + 1
    return line, column


def main(argv):
    if len(argv) != 2:
        raise SystemExit("usage: lint_consumer.py ANALYSIS.json")
    with open(argv[1], encoding="utf-8") as handle:
        analysis = json.load(handle)
    if analysis.get("schema") != "marqdo.analysis":
        raise SystemExit("lint: unexpected analysis schema")
    modules = {module["id"]: module for module in analysis["modules"]}
    problems = 0
    for module in analysis["modules"]:
        source = module["source"]
        for reference in module["references"]:
            state = reference["state"]
            if state in ("resolved", "external"):
                continue
            line, column = line_col(source, reference["span"])
            print(
                f"lint: {reference['url']} state={state} "
                f"{module['relative_path']}:{line}:{column}"
            )
            if state in ("ambiguous",):
                print("lint: candidates=" + ",".join(reference["candidates"]))
            problems += 1
    for diagnostic in analysis["diagnostics"]:
        if diagnostic["severity"] != "error":
            continue
        raise SystemExit(
            "lint: a successful artifact must not contain an error diagnostic"
        )
    for diagnostic in analysis["diagnostics"]:
        module = modules[diagnostic["module"]]
        span = diagnostic["span"]
        location = module["relative_path"]
        if span is not None:
            line, column = line_col(module["source"], span)
            location = f"{location}:{line}:{column}"
        print(f"lint: {diagnostic['code']} {location}")
    print(f"lint: unresolved={problems}")
    return 0


if __name__ == "__main__":
    sys.exit(main(sys.argv))

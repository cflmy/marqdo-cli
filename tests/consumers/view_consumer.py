#!/usr/bin/env python3
"""View-like independent consumer of the public Analysis artifact.

Uses only the JSON document: section ranges, source text and function headings.
Never imports a Marqdo/Rust crate and never executes the document.

usage: view_consumer.py ANALYSIS.json
"""
import json
import sys


def utf8_slice(source, span):
    data = source.encode("utf-8")
    return data[span["start"] : span["end"]].decode("utf-8")


def main(argv):
    if len(argv) != 2:
        raise SystemExit("usage: view_consumer.py ANALYSIS.json")
    with open(argv[1], encoding="utf-8") as handle:
        analysis = json.load(handle)
    if analysis.get("schema") != "marqdo.analysis":
        raise SystemExit("view: unexpected analysis schema")
    rendered = 0
    for module in analysis["modules"]:
        source = module["source"]
        units = module["units"]
        if not units:
            raise SystemExit("view: module has no units")
        by_id = {unit["id"]: unit for unit in units}
        for unit in units:
            parent = unit.get("parent_id")
            if parent is not None and parent not in by_id:
                raise SystemExit("view: dangling unit parent")
            if unit["span"]["start"] > unit["span"]["end"]:
                raise SystemExit("view: reversed section span")
        for unit in units:
            if unit["kind"] != "function":
                continue
            section = utf8_slice(source, unit["span"])
            heading = utf8_slice(source, unit["heading_span"])
            if not heading.strip():
                raise SystemExit("view: empty function heading")
            if not section.startswith(heading):
                raise SystemExit("view: function section does not start at its heading")
            label = "/".join(unit["path"])
            print(f"view: {module['id']}#{label} [{len(section)} bytes]")
            rendered += 1
    if not rendered:
        raise SystemExit("view: no callable section to render")
    print(f"view: rendered={rendered}")
    return 0


if __name__ == "__main__":
    sys.exit(main(sys.argv))

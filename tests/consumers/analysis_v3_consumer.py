#!/usr/bin/env python3
"""Read loop transfers from public Analysis JSON using only Python."""
import json
import pathlib
import sys


def visit(body, loop_span=None):
    for statement in body:
        kind = statement["kind"]
        if kind == "loop":
            yield from visit(statement["body"], statement["span"])
        elif kind == "branch":
            for arm in statement["arms"]:
                yield from visit(arm["body"], loop_span)
        elif kind in ("break", "continue"):
            if loop_span is None:
                raise SystemExit(f"{kind} outside a projected loop")
            span = statement["span"]
            if not 0 <= loop_span["start"] < span["start"] < span["end"]:
                raise SystemExit(f"invalid {kind} carrier span")
            yield kind, loop_span["start"]
        elif kind not in ("assign", "expression", "return"):
            raise SystemExit(f"unsupported statement kind {kind}")


def main():
    if len(sys.argv) != 2:
        raise SystemExit("usage: analysis_v3_consumer.py ANALYSIS.json")
    analysis = json.loads(pathlib.Path(sys.argv[1]).read_text(encoding="utf-8"))
    if analysis.get("schema") != "marqdo.analysis" or analysis.get("schema_version") != 3:
        raise SystemExit("unsupported Analysis schema version")
    transfers = [
        transfer
        for module in analysis["modules"]
        for function in module["functions"]
        for transfer in visit(function["body"])
    ]
    if not transfers:
        raise SystemExit("missing loop transfers")
    print(f"transfers={len(transfers)} nearest_loops={len({target for _, target in transfers})}")


if __name__ == "__main__":
    main()

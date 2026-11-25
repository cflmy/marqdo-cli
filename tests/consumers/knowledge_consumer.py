#!/usr/bin/env python3
"""Knowledge-like independent consumer of several explicit program entries.

Each entry is a separate `marqdo check --emit` artifact with its own project
snapshot. This fixture deduplicates modules by canonical id plus source digest,
keeps the real entry context, and reports links the loaded catalog did not
verify. It never scans a directory and never asks the CLI to build a combined
program.

usage: knowledge_consumer.py ANALYSIS... 
"""
import json
import sys


def main(argv):
    if len(argv) < 2:
        raise SystemExit("usage: knowledge_consumer.py ANALYSIS...")
    entries = 0
    modules = {}
    unverified = 0
    projects = set()
    for path in argv[1:]:
        with open(path, encoding="utf-8") as handle:
            analysis = json.load(handle)
        if analysis.get("schema") != "marqdo.analysis":
            raise SystemExit(f"knowledge: unexpected schema in {path}")
        project = analysis["project"]
        project_key = (
            project["root_id"],
            project["entry_module_id"],
            project.get("manifest_digest", ""),
            project.get("lock_digest", ""),
        )
        if project_key in projects:
            raise SystemExit("knowledge: duplicate entry contribution")
        projects.add(project_key)
        entries += 1
        module_ids = {module["id"] for module in analysis["modules"]}
        if project["entry_module_id"] not in module_ids:
            raise SystemExit("knowledge: entry is not in modules")
        for module in analysis["modules"]:
            key = (module["id"], module["source_digest"])
            existing = modules.get(key)
            if existing is not None and existing != module["source"]:
                raise SystemExit("knowledge: same id/digest carries different source")
            modules[key] = module["source"]
            for reference in module["references"]:
                if reference["state"] == "not_loaded":
                    unverified += 1
    print(f"knowledge: entries={entries} modules={len(modules)}")
    print(f"knowledge: not_loaded={unverified}")
    return 0


if __name__ == "__main__":
    sys.exit(main(sys.argv))

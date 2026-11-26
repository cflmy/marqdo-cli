#!/usr/bin/env python3
"""Dependency-direction gate for the Engine / Host Contract / Native Host split.

Issue #13 / ADR-0071 replace the old "fixed crate count and fixed paths" check
with a reviewed responsibility graph. This script reads Cargo's actual resolved
metadata (not a grep over manifests) and enforces:

* the Engine crates reach no native host, sandbox, foreign, plugin-loader,
  worker, package or CLI dependency over normal/build edges;
* ``marqdo-host-contract`` depends only on the neutral value/span types;
* the native host never compiles against runtime/frontend/IR;
* the plugin ABI crate carries no native loading (``libloading``) and the SDK
  carries no loader/worker/host edge;
* the independent embedding consumer resolves the same neutral graph;
* every workspace crate is in the reviewed allowlist below, with a stated
  responsibility. Adding a crate requires editing that list in the same change.

Dev-dependencies are reported and checked separately; they are never allowed to
stand in for the real Engine build. The companion
``check_engine_boundary.py`` builds and runs the real consumer and CLI.

Usage:
    python3 tools/ci/check_dependency_boundaries.py [--offline]
"""

from __future__ import annotations

import argparse
import json
import shutil
import subprocess
import sys
from pathlib import Path

REPO_ROOT = Path(__file__).resolve().parents[2]
CONSUMER_MANIFEST = REPO_ROOT / "tests" / "embedding" / "Cargo.toml"

# Reviewed responsibility allowlist. A new workspace crate must be added here
# (with its responsibility and owner layer) in the same change that adds it.
WORKSPACE_CRATES = {
    "marqdo-span": "engine: UTF-8 position and diagnostic base types",
    "marqdo-value": "engine: Value, RuntimeError and the one Value Wire encoding",
    "marqdo-expression": "engine: expression grammar",
    "marqdo-markdown": "engine: the only Markdown profile and AST",
    "marqdo-ir": "engine: IR, function identity and builtin inventory",
    "marqdo-frontend": "engine: document mapping, signatures and static binding",
    "marqdo-runtime": "engine: evaluator, control flow and Memory",
    "marqdo-host-contract": "engine: platform-neutral Host trait and runtime observation facts",
    "marqdo-host": "native host: capabilities, resources, OS adapters and trusted extension execution",
    "marqdo-sandbox": "native host support: OS isolation probe and strict launch",
    "marqdo-foreign": "native host support: controlled one-shot foreign kernels (F-A)",
    "marqdo-plugin-abi": "public contract: C ABI types, wire envelope and protocol errors",
    "marqdo-worker-ipc": "shared plumbing: one bounded worker frame transport",
    "marqdo-provider-worker": "shipped worker: provider helper process",
    "marqdo-package": "product support: project manifest, lock and dependency resolution",
    "marqdo-cli": "composition root: arguments, prepare, commands, product projection",
}

ENGINE_CRATES = [
    "marqdo-span",
    "marqdo-value",
    "marqdo-expression",
    "marqdo-markdown",
    "marqdo-ir",
    "marqdo-frontend",
    "marqdo-runtime",
    "marqdo-host-contract",
]

FORBIDDEN_FOR_ENGINE = {
    "marqdo-host",
    "marqdo-sandbox",
    "marqdo-foreign",
    "marqdo-plugin-abi",
    "marqdo-plugin-sdk",
    "marqdo-provider-worker",
    "marqdo-worker-ipc",
    "marqdo-package",
    "marqdo-cli",
    "libloading",
    "rustls",
    "rustls-native-certs",
    "flate2",
    "regex",
    "glob",
    "getrandom",
}

# Normal/build edges only; "dev" is reported but never used to justify a build.
BUILD_KINDS = {None, "build"}


class GateError(Exception):
    pass


def run_metadata(manifest: Path | None, offline: bool) -> dict:
    command = [cargo(), "metadata", "--format-version", "1", "--locked"]
    if manifest is not None:
        command += ["--manifest-path", str(manifest)]
    if offline:
        command += ["--offline"]
    result = subprocess.run(
        command, cwd=REPO_ROOT, capture_output=True, text=True, check=False
    )
    if result.returncode != 0:
        raise GateError(f"`{' '.join(command)}` failed:\n{result.stderr}")
    return json.loads(result.stdout)


def cargo() -> str:
    path = shutil.which("cargo")
    if path is None:
        raise GateError("cargo is not on PATH")
    return path


def package_names(metadata: dict) -> dict[str, dict]:
    return {package["name"]: package for package in metadata["packages"]}


def workspace_package_names(metadata: dict) -> set[str]:
    members = set(metadata.get("workspace_members", []))
    return {
        package["name"]
        for package in metadata["packages"]
        if package["id"] in members
    }


def dependency_graph(metadata: dict) -> dict[str, dict[str, set[str]]]:
    """Map package name -> {dependency name: {normal, build, dev}}."""
    names_by_id = {package["id"]: package["name"] for package in metadata["packages"]}
    graph: dict[str, dict[str, set[str]]] = {name: {} for name in names_by_id.values()}
    for node in metadata.get("resolve", {}).get("nodes", []):
        source = names_by_id.get(node["id"])
        if source is None:
            continue
        for dependency in node.get("deps", []):
            target = names_by_id.get(dependency.get("pkg"))
            if target is None:
                continue
            kinds = {
                kind.get("kind")
                for kind in dependency.get("dep_kinds", [{"kind": None}])
            }
            graph[source].setdefault(target, set()).update(kinds)
    return graph


def reachable(graph: dict[str, dict[str, set[str]]], root: str, kinds: set) -> dict[str, str]:
    """Return reachable package -> first path from root over selected edge kinds."""
    seen: dict[str, str] = {root: root}
    queue = [root]
    while queue:
        current = queue.pop()
        for target, edge_kinds in graph.get(current, {}).items():
            if not (edge_kinds & kinds):
                continue
            if target in seen:
                continue
            seen[target] = f"{seen[current]} -> {target}"
            queue.append(target)
    return seen


def direct_build_dependencies(graph: dict[str, dict[str, set[str]]], root: str) -> set[str]:
    return {
        target
        for target, edge_kinds in graph.get(root, {}).items()
        if edge_kinds & BUILD_KINDS
    }


def check_workspace(metadata: dict) -> list[str]:
    problems: list[str] = []
    names = package_names(metadata)
    members = workspace_package_names(metadata)

    stale = sorted(set(WORKSPACE_CRATES) - members)
    if stale:
        problems.append(
            "reviewed crate allowlist names crates that are not workspace members: "
            + ", ".join(stale)
        )
    undocumented = sorted(members - set(WORKSPACE_CRATES))
    if undocumented:
        problems.append(
            "workspace members missing a reviewed responsibility in "
            "tools/ci/check_dependency_boundaries.py: " + ", ".join(undocumented)
        )

    graph = dependency_graph(metadata)
    for crate in ENGINE_CRATES:
        if crate not in graph:
            continue
        closure = reachable(graph, crate, BUILD_KINDS)
        for forbidden in sorted(FORBIDDEN_FOR_ENGINE & set(closure)):
            problems.append(
                f"engine crate `{crate}` reaches forbidden dependency `{forbidden}`: "
                f"{closure[forbidden]}"
            )

    contract = "marqdo-host-contract"
    if contract in graph:
        allowed = {"marqdo-span", "marqdo-value"}
        direct = direct_build_dependencies(graph, contract)
        unexpected = sorted(direct - allowed)
        if unexpected:
            problems.append(
                "marqdo-host-contract must depend only on neutral span/value types; got: "
                + ", ".join(unexpected)
            )

    host = "marqdo-host"
    if host in graph:
        closure = reachable(graph, host, BUILD_KINDS)
        for forbidden in (
            "marqdo-runtime",
            "marqdo-frontend",
            "marqdo-ir",
            "marqdo-expression",
            "marqdo-markdown",
            "marqdo-cli",
        ):
            if forbidden in closure:
                problems.append(
                    f"native host `marqdo-host` must not depend back on `{forbidden}`: "
                    f"{closure[forbidden]}"
                )

    abi = "marqdo-plugin-abi"
    if abi in graph:
        direct = direct_build_dependencies(graph, abi)
        for forbidden in ("libloading", "marqdo-worker-ipc", "marqdo-host", "marqdo-sandbox"):
            if forbidden in direct:
                problems.append(
                    f"public contract `marqdo-plugin-abi` must not carry execution-side "
                    f"dependency `{forbidden}`"
                )

    # ADR-0072: the SDK is an independent repository. The CLI workspace must
    # not carry it again; a new member here would be an undocumented layer and
    # would resurrect a second contract-consumer copy.
    if "marqdo-plugin-sdk" in members:
        problems.append(
            "marqdo-plugin-sdk must not be a CLI workspace member; the SDK is the "
            "independent repository pinned to a contract revision (ADR-0072)"
        )

    # CLT Tool / package identity is a product concern; the CLI is the only
    # crate allowed to combine engine + native host + package.
    cli = "marqdo-cli"
    if cli in graph:
        direct = direct_build_dependencies(graph, cli)
        for required in ("marqdo-runtime", "marqdo-host", "marqdo-host-contract"):
            if required not in direct:
                problems.append(
                    f"CLI composition root must depend on `{required}` directly"
                )

    return problems


def check_consumer(metadata: dict) -> list[str]:
    problems: list[str] = []
    graph = dependency_graph(metadata)
    package = next(
        (
            item
            for item in metadata["packages"]
            if item["name"] == "marqdo-embedding-consumer"
        ),
        None,
    )
    if package is None:
        return ["embedding consumer package was not resolved"]

    names = package_names(metadata)
    if "marqdo-host" in names or "marqdo-cli" in names or "marqdo-sandbox" in names:
        problems.append(
            "independent embedding consumer resolved a native host, sandbox or CLI package"
        )

    closure = reachable(graph, "marqdo-runtime", BUILD_KINDS)
    for forbidden in sorted(FORBIDDEN_FOR_ENGINE & set(closure)):
        problems.append(
            f"runtime in the independent consumer reaches forbidden dependency "
            f"`{forbidden}`: {closure[forbidden]}"
        )

    direct = direct_build_dependencies(graph, "marqdo-embedding-consumer")
    allowed_direct = {
        "marqdo-frontend",
        "marqdo-host-contract",
        "marqdo-ir",
        "marqdo-runtime",
        "marqdo-span",
        "marqdo-value",
        "serde_json",
        "serde",
    }
    unexpected = sorted(direct - allowed_direct)
    if unexpected:
        problems.append(
            "embedding consumer has unexpected direct dependencies: "
            + ", ".join(unexpected)
        )
    for required in ("marqdo-runtime", "marqdo-host-contract", "marqdo-frontend"):
        if required not in direct:
            problems.append(
                f"embedding consumer must depend directly on `{required}`"
            )
    return problems


def evaluate(workspace_metadata: dict, consumer_metadata: dict) -> list[str]:
    problems = check_workspace(workspace_metadata)
    problems += check_consumer(consumer_metadata)
    return problems


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument(
        "--offline",
        action="store_true",
        help="pass --offline to cargo metadata (used on hosts with a warm cache)",
    )
    arguments = parser.parse_args()
    try:
        workspace = run_metadata(None, arguments.offline)
        consumer = run_metadata(CONSUMER_MANIFEST, arguments.offline)
    except GateError as error:
        print(f"FAIL dependency-boundaries: {error}", file=sys.stderr)
        return 1
    problems = evaluate(workspace, consumer)
    if problems:
        for problem in problems:
            print(f"FAIL dependency-boundaries: {problem}", file=sys.stderr)
        return 1
    print("PASS dependency-boundaries")
    return 0


if __name__ == "__main__":
    sys.exit(main())

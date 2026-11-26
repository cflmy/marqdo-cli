#!/usr/bin/env python3
"""Negative-case tests for the dependency-direction gate (issue #13 G03).

The architecture gate must actually reject violations, not just accept the
current tree. These synthetic Cargo-metadata graphs exercise each forbidden edge
and the reviewed-allowlist rule.
"""

from __future__ import annotations

import sys
import unittest
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parent))

import check_dependency_boundaries as gate  # noqa: E402


def package_id(name: str) -> str:
    return f"path+file:///repo#{name}@0.0.0"


def metadata(
    edges: dict[str, list[str]],
    *,
    members: set[str] | None = None,
    extra_packages: set[str] | None = None,
) -> dict:
    names = set(edges)
    for targets in edges.values():
        names.update(targets)
    names.update(extra_packages or set())
    packages = [
        {"id": package_id(name), "name": name, "version": "0.0.0"}
        for name in sorted(names)
    ]
    nodes = []
    for source, targets in edges.items():
        nodes.append(
            {
                "id": package_id(source),
                "deps": [
                    {
                        "name": target,
                        "pkg": package_id(target),
                        "dep_kinds": [{"kind": None, "target": None}],
                    }
                    for target in targets
                ],
            }
        )
    if members is None:
        members = set(gate.WORKSPACE_CRATES)
    return {
        "packages": packages,
        "workspace_members": [package_id(name) for name in members],
        "resolve": {"nodes": nodes},
    }


def valid_workspace_edges() -> dict[str, list[str]]:
    edges = {name: [] for name in gate.WORKSPACE_CRATES}
    edges["marqdo-value"] = ["marqdo-span"]
    edges["marqdo-span"] = []
    edges["marqdo-expression"] = ["marqdo-span", "marqdo-value"]
    edges["marqdo-markdown"] = ["marqdo-span"]
    edges["marqdo-ir"] = ["marqdo-span", "marqdo-value"]
    edges["marqdo-frontend"] = [
        "marqdo-markdown",
        "marqdo-expression",
        "marqdo-ir",
        "marqdo-value",
        "marqdo-span",
    ]
    edges["marqdo-host-contract"] = ["marqdo-value", "marqdo-span"]
    edges["marqdo-runtime"] = [
        "marqdo-host-contract",
        "marqdo-ir",
        "marqdo-expression",
        "marqdo-value",
        "marqdo-span",
    ]
    edges["marqdo-plugin-abi"] = ["marqdo-value"]
    edges["marqdo-worker-ipc"] = []
    edges["marqdo-sandbox"] = []
    edges["marqdo-foreign"] = ["marqdo-worker-ipc"]
    edges["marqdo-provider-worker"] = ["marqdo-worker-ipc"]
    edges["marqdo-package"] = ["marqdo-value"]
    edges["marqdo-host"] = [
        "marqdo-host-contract",
        "marqdo-plugin-abi",
        "marqdo-foreign",
        "marqdo-sandbox",
        "marqdo-worker-ipc",
    ]
    edges["marqdo-cli"] = [
        "marqdo-runtime",
        "marqdo-host-contract",
        "marqdo-host",
        "marqdo-frontend",
        "marqdo-package",
    ]
    return edges


def valid_consumer_edges() -> dict[str, list[str]]:
    return {
        "marqdo-embedding-consumer": [
            "marqdo-runtime",
            "marqdo-host-contract",
            "marqdo-frontend",
            "marqdo-ir",
            "marqdo-span",
            "marqdo-value",
            "serde_json",
        ],
        "marqdo-runtime": [
            "marqdo-host-contract",
            "marqdo-ir",
            "marqdo-expression",
            "marqdo-value",
            "marqdo-span",
        ],
        "marqdo-host-contract": ["marqdo-value", "marqdo-span"],
        "marqdo-frontend": ["marqdo-ir", "marqdo-value"],
        "marqdo-ir": ["marqdo-value"],
        "marqdo-value": ["marqdo-span"],
        "marqdo-span": [],
        "serde_json": [],
    }


class DependencyBoundaryTests(unittest.TestCase):
    def assert_rejected(self, workspace_edges_edit, needle: str) -> None:
        edges = valid_workspace_edges()
        workspace_edges_edit(edges)
        workspace = metadata(edges)
        consumer = metadata(valid_consumer_edges(), members={"marqdo-embedding-consumer"})
        problems = gate.evaluate(workspace, consumer)
        self.assertTrue(
            any(needle in problem for problem in problems),
            f"expected a problem containing {needle!r}, got {problems}",
        )

    def test_current_shape_is_accepted(self) -> None:
        workspace = metadata(valid_workspace_edges())
        consumer = metadata(valid_consumer_edges(), members={"marqdo-embedding-consumer"})
        self.assertEqual(gate.evaluate(workspace, consumer), [])

    def test_runtime_to_native_host_is_rejected(self) -> None:
        self.assert_rejected(
            lambda edges: edges["marqdo-runtime"].append("marqdo-host"),
            "forbidden dependency `marqdo-host`",
        )

    def test_engine_to_sandbox_is_rejected(self) -> None:
        self.assert_rejected(
            lambda edges: edges["marqdo-runtime"].append("marqdo-sandbox"),
            "forbidden dependency `marqdo-sandbox`",
        )

    def test_engine_to_libloading_is_rejected(self) -> None:
        self.assert_rejected(
            lambda edges: edges["marqdo-runtime"].append("libloading"),
            "forbidden dependency `libloading`",
        )

    def test_native_host_back_edge_is_rejected(self) -> None:
        self.assert_rejected(
            lambda edges: edges["marqdo-host"].append("marqdo-runtime"),
            "must not depend back on `marqdo-runtime`",
        )

    def test_contract_extra_dependency_is_rejected(self) -> None:
        self.assert_rejected(
            lambda edges: edges["marqdo-host-contract"].append("serde_json"),
            "must depend only on neutral span/value types",
        )

    def test_abi_native_loader_is_rejected(self) -> None:
        self.assert_rejected(
            lambda edges: edges["marqdo-plugin-abi"].append("libloading"),
            "must not carry execution-side dependency `libloading`",
        )

    def test_sdk_reappearing_as_workspace_member_is_rejected(self) -> None:
        edges = valid_workspace_edges()
        edges["marqdo-plugin-sdk"] = ["marqdo-plugin-abi"]
        workspace = metadata(edges, members=set(gate.WORKSPACE_CRATES) | {"marqdo-plugin-sdk"})
        consumer = metadata(valid_consumer_edges(), members={"marqdo-embedding-consumer"})
        problems = gate.evaluate(workspace, consumer)
        self.assertTrue(
            any("must not be a CLI workspace member" in problem for problem in problems),
            problems,
        )

    def test_undocumented_workspace_crate_is_rejected(self) -> None:
        edges = valid_workspace_edges()
        edges["marqdo-unreviewed"] = []
        workspace = metadata(edges, members=set(gate.WORKSPACE_CRATES) | {"marqdo-unreviewed"})
        consumer = metadata(valid_consumer_edges(), members={"marqdo-embedding-consumer"})
        problems = gate.evaluate(workspace, consumer)
        self.assertTrue(
            any("missing a reviewed responsibility" in problem for problem in problems),
            problems,
        )

    def test_cli_without_contract_is_rejected(self) -> None:
        self.assert_rejected(
            lambda edges: edges["marqdo-cli"].remove("marqdo-host-contract"),
            "CLI composition root must depend on `marqdo-host-contract`",
        )

    def test_consumer_native_host_is_rejected(self) -> None:
        edges = valid_consumer_edges()
        edges["marqdo-embedding-consumer"].append("marqdo-host")
        consumer = metadata(edges, members={"marqdo-embedding-consumer"})
        workspace = metadata(valid_workspace_edges())
        problems = gate.evaluate(workspace, consumer)
        self.assertTrue(
            any("resolved a native host" in problem for problem in problems), problems
        )


if __name__ == "__main__":
    unittest.main()

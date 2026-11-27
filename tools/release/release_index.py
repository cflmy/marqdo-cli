#!/usr/bin/env python3
"""Generate the ecosystem release index for a cold-tested CLI archive."""

from __future__ import annotations

import argparse
import json
import re
import tarfile
import urllib.request
from pathlib import Path

from package import sha256_file

REPOSITORY = "cflmy/marqdo-cli"
FORGE = "https://git.cflmy.cn"
CI = "https://ci.cflmy.cn"
LIMIT = 1024 * 1024
SHA = re.compile(r"[0-9a-f]{40}")
COMPONENT_ID = re.compile(r"[a-z][a-z0-9-]*")
REQUIRED_STEPS = {"contract", "test", "independent-build", "minimum-rust", "build"}


def read_json(url: str) -> dict:
    with urllib.request.urlopen(url, timeout=20) as response:
        data = response.read(LIMIT + 1)
        if response.geturl() != url or len(data) > LIMIT:
            raise ValueError("unexpected CI or forge response")
    return json.loads(data)


def verify_ci(source_sha: str, number: int) -> dict:
    if number < 1:
        raise ValueError("pipeline number must be positive")
    forge = read_json(f"{FORGE}/api/v1/repos/{REPOSITORY}")
    repo = read_json(f"{CI}/api/repos/lookup/{REPOSITORY}")
    repo_id = repo.get("id")
    if (type(forge.get("id")) is not int or type(repo_id) is not int or repo_id < 1
            or repo.get("full_name") != REPOSITORY
            or repo.get("forge_remote_id") != str(forge["id"])):
        raise ValueError("Woodpecker does not identify the canonical CLI repository")
    pipeline = read_json(f"{CI}/api/repos/{repo_id}/pipelines/{number}")
    successful = {
        step["name"]
        for workflow in pipeline.get("workflows", [])
        if workflow.get("name") == "main" and workflow.get("state") == "success"
        for step in workflow.get("children", [])
        if step.get("state") == "success" and step.get("type") == "commands"
    }
    if (pipeline.get("number") != number or pipeline.get("commit") != source_sha
            or pipeline.get("status") != "success"
            or pipeline.get("event") not in {"push", "pull_request"}
            or not REQUIRED_STEPS <= successful):
        raise ValueError("pipeline did not pass the CLI gates for the archive's exact SHA")
    return {"pipeline_number": number, "url": f"{CI}/repos/{repo_id}/pipeline/{number}",
            "source_sha": source_sha}


def archive_info(archive: Path) -> dict:
    top = archive.name.removesuffix(".tar.gz")
    names = {f"{top}/RELEASE_INFO.json", f"{top}/SOURCE_SHA"}
    metadata = {}
    # Read the two fixed paths without extracting the archive.
    with tarfile.open(archive, "r|gz") as product:
        for member in product:
            if member.name not in names:
                continue
            if member.name in metadata or not member.isfile() or member.size > LIMIT:
                raise ValueError("invalid archive metadata entry")
            with product.extractfile(member) as file:
                metadata[member.name] = file.read()
    if metadata.keys() != names:
        raise ValueError("archive is missing release metadata")
    info = json.loads(metadata[f"{top}/RELEASE_INFO.json"])
    sha = metadata[f"{top}/SOURCE_SHA"].decode("ascii").strip()
    if (info.get("schema") != "marqdo.release-info" or info.get("schema_version") != 1
            or info.get("product") != "marqdo-cli" or not SHA.fullmatch(sha)
            or info.get("source_sha") != sha or info.get("build_profile") != "release"
            or archive.name != f"marqdo-{info['product_version']}-{info['target_triple']}.tar.gz"):
        raise ValueError("archive identity disagrees with RELEASE_INFO or SOURCE_SHA")
    return info


def bundle_dependencies(info: dict, receipt: dict) -> list[dict]:
    bundle = info["stdlib"]
    source = bundle.get("source", {})
    if (source.get("kind") != "ecosystem" or source.get("repository") != "cflmy/marqdo"
            or not SHA.fullmatch(source.get("commit", ""))
            or receipt.get("root_commit") != source["commit"]
            or any(receipt.get(field) != bundle[field]
                   for field in ("name", "version", "tree_digest", "aliases_digest"))):
        raise ValueError("producer receipt mismatches the Bundle in the archive")
    components = receipt["components"]
    if not isinstance(components, list) or not 1 <= len(components) <= 256:
        raise ValueError("Bundle receipt must identify its contributing components")
    dependencies = {}
    for component in components:
        name, revision = component["id"], component["revision"]
        if not COMPONENT_ID.fullmatch(name) or name == "cli" or not SHA.fullmatch(revision) or name in dependencies:
            raise ValueError("invalid or duplicate Bundle component identity")
        dependencies[name] = revision
    return [{"id": name, "source_sha": sha} for name, sha in sorted(dependencies.items())]


def evidence(archive: Path, receipt: dict, number: int) -> dict:
    info = archive_info(archive)
    dependencies = bundle_dependencies(info, receipt)
    checksum = archive.with_name(archive.name + ".sha256")
    digest = sha256_file(archive)
    if checksum.stat().st_size > LIMIT or checksum.read_text().split() != [digest, archive.name]:
        raise ValueError("archive checksum disagrees with the release asset")
    return {
        "component_id": "cli", "repository": REPOSITORY, "tag": f"v{info['product_version']}",
        "source_sha": info["source_sha"], "ci": verify_ci(info["source_sha"], number),
        "assets": [
            {"name": path.name, "target": info["target_triple"], "size": path.stat().st_size,
             "sha256": sha}
            for path, sha in ((archive, digest), (checksum, sha256_file(checksum)))
        ],
        "dependencies": dependencies,
    }


def main() -> None:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--archive", type=Path, required=True)
    parser.add_argument("--bundle-receipt", type=Path, required=True)
    parser.add_argument("--pipeline", type=int, required=True)
    args = parser.parse_args()
    try:
        with args.bundle_receipt.open("rb") as file:
            data = file.read(LIMIT + 1)
        if len(data) > LIMIT:
            raise ValueError("Bundle receipt exceeds size limit")
        index = evidence(args.archive, json.loads(data), args.pipeline)
    except (OSError, ValueError, TypeError, KeyError, AttributeError, tarfile.TarError) as error:
        parser.exit(1, f"release-index: {error}\n")
    print(json.dumps(index, indent=2))


if __name__ == "__main__":
    main()

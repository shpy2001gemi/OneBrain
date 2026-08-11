#!/usr/bin/env python3
"""Generate the deterministic SPDX 2.3 SBOM for a Base v1 candidate lane."""

from __future__ import annotations

import argparse
import base64
import hashlib
import json
import re
import sys
import tomllib
from pathlib import Path


class SbomError(RuntimeError):
    """The dependency graph cannot produce a closed deterministic SBOM."""


_BINDING_FIELDS = {
    "format",
    "release_request_digest",
    "qualification_session_id",
    "candidate_commit",
    "candidate_tree",
    "candidate_semantic_digest",
    "target_triple",
    "toolchain_digest",
    "created_utc",
}
_HEX_32 = re.compile(r"^[0-9a-f]{64}$")
_GIT_ID = re.compile(r"^(?:[0-9a-f]{40}|[0-9a-f]{64})$")


def _canonical(value: object) -> bytes:
    return json.dumps(value, sort_keys=True, separators=(",", ":"), ensure_ascii=True).encode()


def _object(value: object, label: str) -> dict[str, object]:
    if not isinstance(value, dict):
        raise SbomError(f"{label} must be an object")
    return value


def _validate_binding(value: object) -> dict[str, object]:
    binding = _object(value, "candidate binding")
    if set(binding) != _BINDING_FIELDS:
        missing = sorted(_BINDING_FIELDS - set(binding))
        extra = sorted(set(binding) - _BINDING_FIELDS)
        raise SbomError(f"candidate binding fields drift; missing={missing}, extra={extra}")
    if binding["format"] != "onebrain/base-v1-candidate-binding/1":
        raise SbomError("candidate binding format is unsupported")
    for field in (
        "release_request_digest",
        "qualification_session_id",
        "candidate_semantic_digest",
        "toolchain_digest",
    ):
        if not isinstance(binding[field], str) or not _HEX_32.fullmatch(binding[field]):
            raise SbomError(f"candidate binding {field} must be lowercase 32-byte hex")
    for field in ("candidate_commit", "candidate_tree"):
        if not isinstance(binding[field], str) or not _GIT_ID.fullmatch(binding[field]):
            raise SbomError(f"candidate binding {field} must match the repository object format")
    for field in ("target_triple", "created_utc"):
        if not isinstance(binding[field], str) or not binding[field] or not binding[field].isascii():
            raise SbomError(f"candidate binding {field} must be non-empty ASCII")
    return dict(binding)


def _spdx_id(ecosystem: str, locator: str) -> str:
    digest = hashlib.sha256(locator.encode("utf-8")).hexdigest()[:24]
    return f"SPDXRef-Package-{ecosystem}-{digest}"


def _cargo_lock_packages(cargo_lock: str) -> dict[tuple[str, str, str | None], dict[str, object]]:
    try:
        parsed = tomllib.loads(cargo_lock)
    except tomllib.TOMLDecodeError as error:
        raise SbomError(f"Cargo.lock is invalid TOML: {error}") from error
    result: dict[tuple[str, str, str | None], dict[str, object]] = {}
    for raw in parsed.get("package", []):
        package = _object(raw, "Cargo.lock package")
        key = (package.get("name"), package.get("version"), package.get("source"))
        if not all(isinstance(part, str) for part in key[:2]):
            raise SbomError("Cargo.lock package name/version is missing")
        if key in result:
            raise SbomError(f"duplicate Cargo.lock package: {key[0]} {key[1]}")
        result[key] = package
    return result


def _cargo_packages(
    metadata: object, cargo_lock: str
) -> tuple[list[dict[str, object]], list[dict[str, str]], set[str]]:
    root = _object(metadata, "cargo metadata")
    raw_packages = root.get("packages")
    resolve = _object(root.get("resolve"), "cargo resolve")
    nodes = resolve.get("nodes")
    if not isinstance(raw_packages, list) or not isinstance(nodes, list):
        raise SbomError("cargo metadata packages/resolve.nodes must be arrays")
    locks = _cargo_lock_packages(cargo_lock)
    by_id: dict[str, dict[str, object]] = {}
    result: list[dict[str, object]] = []
    spdx_by_id: dict[str, str] = {}
    for raw in raw_packages:
        package = _object(raw, "cargo package")
        package_id = package.get("id")
        name = package.get("name")
        version = package.get("version")
        license_value = package.get("license")
        source = package.get("source")
        if not all(isinstance(value, str) and value for value in (package_id, name, version)):
            raise SbomError("cargo package id/name/version is missing")
        if package_id in by_id:
            raise SbomError(f"duplicate cargo package id: {package_id}")
        if not isinstance(license_value, str) or not license_value:
            raise SbomError(f"cargo package license is missing: {name} {version}")
        by_id[package_id] = package
        locator = f"{package_id}"
        spdx = _spdx_id("cargo", locator)
        spdx_by_id[package_id] = spdx
        entry: dict[str, object] = {
            "SPDXID": spdx,
            "name": name,
            "versionInfo": version,
            "downloadLocation": source if isinstance(source, str) else "NOASSERTION",
            "filesAnalyzed": False,
            "licenseConcluded": license_value,
            "licenseDeclared": license_value,
            "supplier": "NOASSERTION",
            "externalRefs": [{
                "referenceCategory": "PACKAGE-MANAGER",
                "referenceType": "purl",
                "referenceLocator": f"pkg:cargo/{name}@{version}",
            }],
        }
        if isinstance(source, str):
            locked = locks.get((name, version, source))
            checksum = locked.get("checksum") if locked else None
            if not isinstance(checksum, str) or not _HEX_32.fullmatch(checksum):
                raise SbomError(f"cargo registry checksum is missing or invalid: {name} {version}")
            entry["checksums"] = [{"algorithm": "SHA256", "checksumValue": checksum}]
        result.append(entry)
    relationships: list[dict[str, str]] = []
    seen_nodes: set[str] = set()
    for raw in nodes:
        node = _object(raw, "cargo resolve node")
        node_id = node.get("id")
        dependencies = node.get("dependencies")
        if not isinstance(node_id, str) or node_id not in by_id or not isinstance(dependencies, list):
            raise SbomError("cargo resolve node is invalid or references a missing package")
        if node_id in seen_nodes:
            raise SbomError(f"duplicate cargo resolve node: {node_id}")
        seen_nodes.add(node_id)
        for dependency in dependencies:
            if not isinstance(dependency, str) or dependency not in by_id:
                raise SbomError(f"missing cargo dependency package: {dependency}")
            relationships.append({
                "spdxElementId": spdx_by_id[node_id],
                "relationshipType": "DEPENDS_ON",
                "relatedSpdxElement": spdx_by_id[dependency],
            })
    if seen_nodes != set(by_id):
        raise SbomError("cargo resolve graph omits one or more packages")
    if isinstance(resolve.get("root"), str):
        roots = {resolve["root"]}
    else:
        workspace_members = root.get("workspace_members")
        if not isinstance(workspace_members, list) or not workspace_members:
            raise SbomError("virtual cargo workspace has no workspace_members")
        roots = set(workspace_members)
        if not roots <= set(by_id):
            raise SbomError("cargo workspace_members references a missing package")
    return result, relationships, {spdx_by_id[item] for item in roots if item in spdx_by_id}


def _integrity_checksum(integrity: object, label: str) -> dict[str, str]:
    if not isinstance(integrity, str) or "-" not in integrity:
        raise SbomError(f"npm integrity checksum is missing: {label}")
    algorithm, encoded = integrity.split("-", 1)
    known = {"sha256": "SHA256", "sha384": "SHA384", "sha512": "SHA512"}
    if algorithm not in known:
        raise SbomError(f"npm integrity algorithm is unsupported: {label}")
    try:
        checksum = base64.b64decode(encoded, validate=True).hex()
    except (ValueError, base64.binascii.Error) as error:
        raise SbomError(f"npm integrity checksum is invalid: {label}") from error
    return {"algorithm": known[algorithm], "checksumValue": checksum}


def _npm_packages(
    npm_lock: object,
) -> tuple[list[dict[str, object]], list[dict[str, str]], set[str]]:
    lock = _object(npm_lock, "npm lock")
    if lock.get("lockfileVersion") != 3:
        raise SbomError("npm lockfileVersion must be 3")
    raw_packages = _object(lock.get("packages"), "npm packages")
    result: list[dict[str, object]] = []
    by_name: dict[str, tuple[str, str]] = {}
    roots: set[str] = set()
    root_dependencies = _object(raw_packages.get("", {}), "npm root package").get("dependencies", {})
    for path, raw in raw_packages.items():
        if path == "":
            continue
        package = _object(raw, f"npm package {path}")
        name = package.get("name")
        if not isinstance(name, str) or not name:
            name = path.rsplit("node_modules/", 1)[-1]
        version = package.get("version")
        license_value = package.get("license")
        if not isinstance(version, str) or not version:
            raise SbomError(f"npm package version is missing: {path}")
        if not isinstance(license_value, str) or not license_value:
            raise SbomError(f"npm package license is missing: {name} {version}")
        locator = f"{name}@{version}"
        if name in by_name or locator in {item[0] for item in by_name.values()}:
            raise SbomError(f"duplicate npm package locator: {locator}")
        spdx = _spdx_id("npm", locator)
        by_name[name] = (locator, spdx)
        if name in root_dependencies:
            roots.add(spdx)
        result.append({
            "SPDXID": spdx,
            "name": name,
            "versionInfo": version,
            "downloadLocation": package.get("resolved", "NOASSERTION"),
            "filesAnalyzed": False,
            "licenseConcluded": license_value,
            "licenseDeclared": license_value,
            "supplier": "NOASSERTION",
            "checksums": [_integrity_checksum(package.get("integrity"), locator)],
            "externalRefs": [{
                "referenceCategory": "PACKAGE-MANAGER",
                "referenceType": "purl",
                "referenceLocator": f"pkg:npm/{name}@{version}",
            }],
        })
    relationships: list[dict[str, str]] = []
    for path, raw in raw_packages.items():
        if path == "":
            continue
        package = _object(raw, f"npm package {path}")
        name = package.get("name") or path.rsplit("node_modules/", 1)[-1]
        dependencies = _object(package.get("dependencies", {}), f"npm dependencies {name}")
        for dependency in dependencies:
            if dependency not in by_name:
                raise SbomError(f"missing npm dependency package: {name} -> {dependency}")
            relationships.append({
                "spdxElementId": by_name[name][1],
                "relationshipType": "DEPENDS_ON",
                "relatedSpdxElement": by_name[dependency][1],
            })
    for dependency in root_dependencies:
        if dependency not in by_name:
            raise SbomError(f"missing npm root dependency package: {dependency}")
    return result, relationships, roots


def generate_spdx(
    cargo_metadata: object,
    cargo_lock: str,
    npm_lock: object,
    candidate_binding: object,
) -> dict[str, object]:
    """Return canonical-content SPDX data; caller controls only authenticated inputs."""
    binding = _validate_binding(candidate_binding)
    cargo_packages, cargo_edges, cargo_roots = _cargo_packages(cargo_metadata, cargo_lock)
    npm_packages, npm_edges, npm_roots = _npm_packages(npm_lock)
    packages = sorted(cargo_packages + npm_packages, key=lambda item: item["SPDXID"])
    if len({item["SPDXID"] for item in packages}) != len(packages):
        raise SbomError("duplicate SPDX package identifier")
    document_id = "SPDXRef-DOCUMENT"
    relationships = cargo_edges + npm_edges + [
        {"spdxElementId": document_id, "relationshipType": "DESCRIBES", "relatedSpdxElement": root}
        for root in sorted(cargo_roots | npm_roots)
    ]
    relationships.sort(key=lambda item: (
        item["spdxElementId"], item["relationshipType"], item["relatedSpdxElement"]
    ))
    binding_digest = hashlib.sha256(_canonical(binding)).hexdigest()
    return {
        "SPDXID": document_id,
        "spdxVersion": "SPDX-2.3",
        "dataLicense": "CC0-1.0",
        "name": "OneBrain-Base-v1-candidate",
        "documentNamespace": f"https://onebrain.invalid/spdx/base-v1/{binding_digest}",
        "creationInfo": {
            "created": binding["created_utc"],
            "creators": ["Tool: onebrain-base-v1-sbom/1"],
            "licenseListVersion": "3.26",
        },
        "onebrainCandidateBinding": binding,
        "packages": packages,
        "relationships": relationships,
    }


def main(argv: list[str] | None = None) -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--cargo-metadata", type=Path, required=True)
    parser.add_argument("--cargo-lock", type=Path, required=True)
    parser.add_argument("--npm-lock", type=Path, required=True)
    parser.add_argument("--binding", type=Path, required=True)
    parser.add_argument("--output", type=Path, required=True)
    args = parser.parse_args(argv)
    try:
        document = generate_spdx(
            json.loads(args.cargo_metadata.read_text(encoding="utf-8")),
            args.cargo_lock.read_text(encoding="utf-8"),
            json.loads(args.npm_lock.read_text(encoding="utf-8")),
            json.loads(args.binding.read_text(encoding="utf-8")),
        )
        payload = json.dumps(document, sort_keys=True, separators=(",", ":"), ensure_ascii=True) + "\n"
        args.output.parent.mkdir(parents=True, exist_ok=True)
        with args.output.open("x", encoding="utf-8", newline="\n") as stream:
            stream.write(payload)
    except (OSError, json.JSONDecodeError, SbomError) as error:
        print(f"Base SBOM generation failed: {error}", file=sys.stderr)
        return 1
    return 0


if __name__ == "__main__":
    raise SystemExit(main())

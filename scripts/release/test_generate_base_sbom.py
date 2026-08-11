#!/usr/bin/env python3
"""Tests for deterministic Base v1 SPDX generation."""

from __future__ import annotations

import copy
import unittest

from scripts.release.generate_base_sbom import SbomError, generate_spdx


SHA256_A = "11" * 32
SHA256_B = "22" * 32


class BaseSbomTests(unittest.TestCase):
    def setUp(self) -> None:
        app = "path+file:///candidate/src/app#onebrain-app@1.0.0"
        dep = "registry+https://github.com/rust-lang/crates.io-index#serde@1.0.219"
        self.metadata = {
            "packages": [
                {
                    "id": app,
                    "name": "onebrain-app",
                    "version": "1.0.0",
                    "license": "Apache-2.0",
                    "source": None,
                    "manifest_path": "/candidate/src/app/Cargo.toml",
                },
                {
                    "id": dep,
                    "name": "serde",
                    "version": "1.0.219",
                    "license": "MIT OR Apache-2.0",
                    "source": "registry+https://github.com/rust-lang/crates.io-index",
                    "manifest_path": "/cargo/registry/serde/Cargo.toml",
                },
            ],
            "resolve": {
                "root": app,
                "nodes": [
                    {"id": app, "dependencies": [dep]},
                    {"id": dep, "dependencies": []},
                ],
            },
        }
        self.cargo_lock = f'''version = 4

[[package]]
name = "onebrain-app"
version = "1.0.0"

[[package]]
name = "serde"
version = "1.0.219"
source = "registry+https://github.com/rust-lang/crates.io-index"
checksum = "{SHA256_A}"
'''
        self.npm_lock = {
            "name": "OneBrain",
            "lockfileVersion": 3,
            "packages": {
                "": {"dependencies": {"marked": "18.0.5"}},
                "node_modules/marked": {
                    "name": "marked",
                    "version": "18.0.5",
                    "resolved": "https://registry.npmjs.org/marked/-/marked-18.0.5.tgz",
                    "integrity": "sha512-AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA==",
                    "license": "MIT",
                    "dependencies": {},
                },
            },
        }
        self.binding = {
            "format": "onebrain/base-v1-candidate-binding/1",
            "release_request_digest": "31" * 32,
            "qualification_session_id": "32" * 32,
            "candidate_commit": "a" * 40,
            "candidate_tree": "b" * 40,
            "candidate_semantic_digest": "33" * 32,
            "target_triple": "x86_64-unknown-linux-gnu",
            "toolchain_digest": "34" * 32,
            "created_utc": "2026-08-11T00:00:00Z",
        }

    def test_deterministic_packages_checksums_edges_and_binding(self) -> None:
        first = generate_spdx(self.metadata, self.cargo_lock, self.npm_lock, self.binding)
        reordered = copy.deepcopy(self.metadata)
        reordered["packages"].reverse()
        reordered["resolve"]["nodes"].reverse()
        second = generate_spdx(reordered, self.cargo_lock, self.npm_lock, self.binding)
        self.assertEqual(first, second)
        self.assertEqual(first["spdxVersion"], "SPDX-2.3")
        self.assertEqual(first["onebrainCandidateBinding"], self.binding)
        packages = {package["name"]: package for package in first["packages"]}
        self.assertRegex(packages["serde"]["SPDXID"], r"^SPDXRef-Package-cargo-[0-9a-f]{24}$")
        self.assertEqual(packages["serde"]["checksums"], [{"algorithm": "SHA256", "checksumValue": SHA256_A}])
        self.assertEqual(packages["marked"]["licenseConcluded"], "MIT")
        self.assertEqual(packages["marked"]["checksums"][0]["algorithm"], "SHA512")
        edges = {(edge["spdxElementId"], edge["relationshipType"], edge["relatedSpdxElement"])
                 for edge in first["relationships"]}
        self.assertIn((packages["onebrain-app"]["SPDXID"], "DEPENDS_ON", packages["serde"]["SPDXID"]), edges)
        self.assertIn((first["SPDXID"], "DESCRIBES", packages["marked"]["SPDXID"]), edges)

    def test_rejects_missing_dependency_and_duplicate_package(self) -> None:
        missing = copy.deepcopy(self.metadata)
        missing["packages"].pop()
        with self.assertRaisesRegex(SbomError, "missing cargo dependency"):
            generate_spdx(missing, self.cargo_lock, self.npm_lock, self.binding)
        duplicate = copy.deepcopy(self.metadata)
        duplicate["packages"].append(copy.deepcopy(duplicate["packages"][0]))
        with self.assertRaisesRegex(SbomError, "duplicate cargo package"):
            generate_spdx(duplicate, self.cargo_lock, self.npm_lock, self.binding)

    def test_rejects_missing_registry_checksum_and_duplicate_npm_locator(self) -> None:
        no_checksum = self.cargo_lock.replace(f'checksum = "{SHA256_A}"\n', "")
        with self.assertRaisesRegex(SbomError, "checksum"):
            generate_spdx(self.metadata, no_checksum, self.npm_lock, self.binding)
        duplicate = copy.deepcopy(self.npm_lock)
        duplicate["packages"]["vendor/marked"] = copy.deepcopy(
            duplicate["packages"]["node_modules/marked"]
        )
        with self.assertRaisesRegex(SbomError, "duplicate npm package"):
            generate_spdx(self.metadata, self.cargo_lock, duplicate, self.binding)

    def test_rejects_incomplete_or_malformed_binding(self) -> None:
        for field in ("release_request_digest", "candidate_tree", "target_triple"):
            with self.subTest(field=field):
                binding = dict(self.binding)
                del binding[field]
                with self.assertRaisesRegex(SbomError, field):
                    generate_spdx(self.metadata, self.cargo_lock, self.npm_lock, binding)

    def test_virtual_workspace_members_are_document_roots(self) -> None:
        metadata = copy.deepcopy(self.metadata)
        metadata["workspace_members"] = [metadata["packages"][0]["id"]]
        metadata["resolve"]["root"] = None
        document = generate_spdx(metadata, self.cargo_lock, self.npm_lock, self.binding)
        app = next(package for package in document["packages"] if package["name"] == "onebrain-app")
        self.assertIn({
            "spdxElementId": "SPDXRef-DOCUMENT",
            "relationshipType": "DESCRIBES",
            "relatedSpdxElement": app["SPDXID"],
        }, document["relationships"])


if __name__ == "__main__":
    unittest.main()

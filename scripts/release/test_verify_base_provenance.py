#!/usr/bin/env python3
"""Tests for the Base v1 candidate provenance gate."""

from __future__ import annotations

import copy
import hashlib
import json
import subprocess
import tempfile
import unittest
from pathlib import Path

import blake3

from scripts.release.verify_base_provenance import (
    REVIEWED_AUDIT_ITEMS,
    ProvenanceError,
    verify_provenance,
)


CHECKOUT_SHA = "fbc6f3992d24b796d5a048ff273f7fcc4a7b6c09"
UPLOAD_SHA = "ea165f8d65b6e75b540449e92b4886f43607fa02"
ACTION_ALLOWLIST = {
    "actions/checkout": CHECKOUT_SHA,
    "actions/setup-python": "ece7cb06caefa5fff74198d8649806c4678c61a1",
    "actions/setup-node": "a0853c24544627f65ddf259abe73b1d18a591444",
    "dart-lang/setup-dart": "65eb853c7ba17dde3be364c3d2858773e7144260",
    "actions/upload-artifact": UPLOAD_SHA,
    "actions/download-artifact": "634f93cb2916e3fdff6788551b99b062d0335ce0",
}


def b3(path: Path) -> str:
    return blake3.blake3(path.read_bytes()).hexdigest()


class BaseProvenanceTests(unittest.TestCase):
    def setUp(self) -> None:
        self.temp = tempfile.TemporaryDirectory()
        self.root = Path(self.temp.name) / "repo"
        self.evidence = Path(self.temp.name) / "evidence"
        self.root.mkdir()
        self.evidence.mkdir()
        subprocess.run(["git", "init", "-q"], cwd=self.root, check=True)
        subprocess.run(["git", "config", "user.email", "ci@example.invalid"], cwd=self.root, check=True)
        subprocess.run(["git", "config", "user.name", "CI"], cwd=self.root, check=True)
        (self.root / ".gitignore").write_text("ignored-output/\n", encoding="utf-8")
        (self.root / "source.txt").write_text("candidate\n", encoding="utf-8")
        self.workflow = self.root / "candidate.yml"
        self.workflow = self.root / ".github" / "workflows" / "base-v1-candidate.yml"
        self.workflow.parent.mkdir(parents=True)
        self.workflow.write_text(
            "steps:\n"
            f"  - uses: actions/checkout@{CHECKOUT_SHA}\n"
            f"  - uses: actions/upload-artifact@{UPLOAD_SHA}\n",
            encoding="utf-8",
        )
        subprocess.run(["git", "add", "."], cwd=self.root, check=True)
        subprocess.run(["git", "commit", "-qm", "candidate"], cwd=self.root, check=True)
        commit = subprocess.check_output(["git", "rev-parse", "HEAD"], cwd=self.root, text=True).strip()
        tree = subprocess.check_output(["git", "rev-parse", "HEAD^{tree}"], cwd=self.root, text=True).strip()
        request = "41" * 32
        session = "42" * 32
        semantic = "43" * 32
        self.executable = self.evidence / "onebrain"
        self.executable.write_bytes(b"binary")
        self.compiler = self.evidence / "rustc-vV.txt"
        self.compiler.write_text("rustc 1.91.0\nhost: test\n", encoding="utf-8")
        self.cargo_audit = self.evidence / "cargo-audit.json"
        self.cargo_audit.write_text('{"vulnerabilities":{"list":[]}}\n', encoding="utf-8")
        self.npm_audit = self.evidence / "npm-audit.json"
        self.npm_audit.write_text('{"vulnerabilities":{}}\n', encoding="utf-8")
        common = {
            "qualification_mode": "release",
            "release_request_digest": request,
            "qualification_session_id": session,
            "candidate_commit": commit,
            "candidate_tree": tree,
            "candidate_semantic_digest": semantic,
            "executable_path": str(self.executable),
            "executable_blake3": b3(self.executable),
            "compiler_path": str(self.compiler),
            "compiler_blake3": b3(self.compiler),
            "workflow_sha256": hashlib.sha256(self.workflow.read_bytes()).hexdigest(),
            "raw_audits": {
                "cargo": {"path": str(self.cargo_audit), "blake3": b3(self.cargo_audit)},
                "npm": {"path": str(self.npm_audit), "blake3": b3(self.npm_audit)},
            },
            "audit_items": [],
        }
        triples = {
            "linux": "x86_64-unknown-linux-gnu",
            "windows": "x86_64-pc-windows-msvc",
            "macos": "aarch64-apple-darwin",
        }
        self.sboms: dict[str, Path] = {}
        toolchains = {os_name: f"{index + 5:02x}" * 32 for index, os_name in enumerate(triples)}
        for os_name, target in triples.items():
            sbom = self.evidence / f"sbom-{os_name}.spdx.json"
            sbom.write_text(json.dumps({
                "spdxVersion": "SPDX-2.3",
                "onebrainCandidateBinding": {
                    "format": "onebrain/base-v1-candidate-binding/1",
                    "release_request_digest": request,
                    "qualification_session_id": session,
                    "candidate_commit": commit,
                    "candidate_tree": tree,
                    "candidate_semantic_digest": semantic,
                    "target_triple": target,
                    "toolchain_digest": toolchains[os_name],
                    "created_utc": "2026-08-11T00:00:00Z",
                },
            }, sort_keys=True, separators=(",", ":")) + "\n", encoding="utf-8")
            self.sboms[os_name] = sbom
        self.bundle = {
            "format": "onebrain/base-v1-provenance/1",
            "qualification_mode": "release",
            "release_request_digest": request,
            "qualification_session_id": session,
            "candidate_commit": commit,
            "candidate_tree": tree,
            "candidate_semantic_digest": semantic,
            "workflow_path": str(self.workflow),
            "action_allowlist": ACTION_ALLOWLIST,
            "lanes": [
                {
                    **common,
                    "os": os_name,
                    "target_triple": triple,
                    "toolchain_digest": toolchains[os_name],
                    "runner_image": f"github-{os_name}@20260811.1",
                    "artifact_tuple_digest": f"{index + 8:02x}" * 32,
                    "sbom_path": str(self.sboms[os_name]),
                    "sbom_blake3": b3(self.sboms[os_name]),
                }
                for index, (os_name, triple) in enumerate(triples.items())
            ],
        }

    def tearDown(self) -> None:
        self.temp.cleanup()

    def test_accepts_exact_three_os_candidate(self) -> None:
        receipt = verify_provenance(self.bundle, self.root)
        self.assertTrue(receipt["verified"])
        self.assertEqual(receipt["qualification_mode"], "release")
        self.assertEqual(receipt["candidate_semantic_digest"], "43" * 32)
        self.assertEqual(set(receipt["lane_receipts"]), {"linux", "windows", "macos"})

    def test_rejects_mixed_request_session_commit_or_tree(self) -> None:
        for field, value in (
            ("release_request_digest", "91" * 32),
            ("qualification_session_id", "92" * 32),
            ("candidate_commit", "a" * 40),
            ("candidate_tree", "b" * 40),
            ("qualification_mode", "prequalification"),
        ):
            with self.subTest(field=field):
                bundle = copy.deepcopy(self.bundle)
                bundle["lanes"][1][field] = value
                with self.assertRaisesRegex(ProvenanceError, field):
                    verify_provenance(bundle, self.root)

    def test_rejects_open_bundle_or_wrong_workflow_path(self) -> None:
        bundle = copy.deepcopy(self.bundle)
        bundle["qualified"] = True
        with self.assertRaisesRegex(ProvenanceError, "fields are not closed"):
            verify_provenance(bundle, self.root)
        bundle = copy.deepcopy(self.bundle)
        bundle["workflow_path"] = str(self.root / "source.txt")
        with self.assertRaisesRegex(ProvenanceError, "candidate-owned"):
            verify_provenance(bundle, self.root)

    def test_rejects_dirty_tracked_untracked_and_ignored_source(self) -> None:
        mutations = (
            (self.root / "source.txt", "changed\n"),
            (self.root / "untracked.txt", "new\n"),
            (self.root / "ignored-output" / "cache.bin", "ignored\n"),
        )
        for path, value in mutations:
            with self.subTest(path=path.name):
                if path.parent != self.root:
                    path.parent.mkdir(exist_ok=True)
                path.write_text(value, encoding="utf-8")
                with self.assertRaisesRegex(ProvenanceError, "source tree"):
                    verify_provenance(self.bundle, self.root)
                subprocess.run(["git", "clean", "-fdx"], cwd=self.root, check=True, stdout=subprocess.DEVNULL)
                subprocess.run(["git", "restore", "."], cwd=self.root, check=True)

    def test_rejects_mutable_unknown_or_unlisted_action(self) -> None:
        for uses in ("actions/checkout@v5", "evil/unknown@" + "c" * 40):
            with self.subTest(uses=uses):
                self.workflow.write_text(f"steps:\n  - uses: {uses}\n", encoding="utf-8")
                bundle = copy.deepcopy(self.bundle)
                digest = hashlib.sha256(self.workflow.read_bytes()).hexdigest()
                for lane in bundle["lanes"]:
                    lane["workflow_sha256"] = digest
                with self.assertRaisesRegex(ProvenanceError, "action"):
                    verify_provenance(bundle, self.root)
        self.workflow.write_text("steps:\n  - uses: evil/unknown@" + "c" * 40 + "\n", encoding="utf-8")
        bundle = copy.deepcopy(self.bundle)
        bundle["action_allowlist"]["evil/unknown"] = "c" * 40
        digest = hashlib.sha256(self.workflow.read_bytes()).hexdigest()
        for lane in bundle["lanes"]:
            lane["workflow_sha256"] = digest
        with self.assertRaisesRegex(ProvenanceError, "allowlist"):
            verify_provenance(bundle, self.root)

    def test_rejects_artifact_digest_missing_lane_or_copied_target_tuple(self) -> None:
        mismatch = copy.deepcopy(self.bundle)
        mismatch["lanes"][0]["executable_blake3"] = "d0" * 32
        with self.assertRaisesRegex(ProvenanceError, "executable"):
            verify_provenance(mismatch, self.root)
        mismatch = copy.deepcopy(self.bundle)
        mismatch["lanes"][2]["sbom_blake3"] = "d1" * 32
        with self.assertRaisesRegex(ProvenanceError, "SBOM"):
            verify_provenance(mismatch, self.root)
        missing = copy.deepcopy(self.bundle)
        missing["lanes"].pop()
        with self.assertRaisesRegex(ProvenanceError, "OS lanes"):
            verify_provenance(missing, self.root)
        copied = copy.deepcopy(self.bundle)
        copied["lanes"][1]["artifact_tuple_digest"] = copied["lanes"][0]["artifact_tuple_digest"]
        with self.assertRaisesRegex(ProvenanceError, "artifact tuple"):
            verify_provenance(copied, self.root)

    def test_rejects_wrong_sbom_binding_or_compiler_bytes(self) -> None:
        sbom = self.sboms["linux"]
        original = sbom.read_text(encoding="utf-8")
        value = json.loads(original)
        value["onebrainCandidateBinding"]["candidate_semantic_digest"] = "e1" * 32
        sbom.write_text(json.dumps(value, sort_keys=True, separators=(",", ":")) + "\n", encoding="utf-8")
        bundle = copy.deepcopy(self.bundle)
        bundle["lanes"][0]["sbom_blake3"] = b3(sbom)
        with self.assertRaisesRegex(ProvenanceError, "SBOM candidate binding"):
            verify_provenance(bundle, self.root)
        sbom.write_text(original, encoding="utf-8")
        self.compiler.write_text("altered compiler\n", encoding="utf-8")
        with self.assertRaisesRegex(ProvenanceError, "compiler"):
            verify_provenance(self.bundle, self.root)

    def test_rejects_untriaged_p0_or_p1(self) -> None:
        for severity in ("P0", "P1"):
            with self.subTest(severity=severity):
                bundle = copy.deepcopy(self.bundle)
                bundle["lanes"][0]["audit_items"] = [
                    {"id": "RUSTSEC-TEST", "severity": severity, "triage": "untriaged"}
                ]
                with self.assertRaisesRegex(ProvenanceError, "untriaged"):
                    verify_provenance(bundle, self.root)

    def test_rejects_advisory_omitted_from_triage(self) -> None:
        self.cargo_audit.write_text(
            '{"vulnerabilities":{"list":[{"advisory":{"id":"RUSTSEC-2099-0001"}}]}}\n',
            encoding="utf-8",
        )
        bundle = copy.deepcopy(self.bundle)
        digest = b3(self.cargo_audit)
        for lane in bundle["lanes"]:
            lane["raw_audits"]["cargo"]["blake3"] = digest
        with self.assertRaisesRegex(ProvenanceError, "missing triage"):
            verify_provenance(bundle, self.root)

    def test_accepts_exact_reviewed_non_base_advisory(self) -> None:
        advisory = "RUSTSEC-2026-0221"
        self.cargo_audit.write_text(json.dumps({
            "vulnerabilities": {"list": []},
            "warnings": {"unsound": [{"advisory": {"id": advisory}}]},
        }) + "\n", encoding="utf-8")
        bundle = copy.deepcopy(self.bundle)
        digest = b3(self.cargo_audit)
        for lane in bundle["lanes"]:
            lane["raw_audits"]["cargo"]["blake3"] = digest
            lane["audit_items"] = [REVIEWED_AUDIT_ITEMS[advisory]]
        self.assertTrue(verify_provenance(bundle, self.root)["verified"])


if __name__ == "__main__":
    unittest.main()

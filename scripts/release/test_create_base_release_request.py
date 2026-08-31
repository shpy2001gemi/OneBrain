#!/usr/bin/env python3
"""Tests for immutable Base release-request creation."""

from __future__ import annotations

import json
import subprocess
import sys
import tempfile
import unittest
from datetime import datetime, timedelta, timezone
from pathlib import Path

import blake3

from scripts.release.create_base_release_request import (
    ReleaseRequestCreationError,
    create_release_request,
)
from scripts.release.verify_base_release_request import (
    FROZEN_APPROVER_POLICY,
    ReleaseRequestError,
    _verify_authenticated_tooling,
    canonical_json,
)


ROOT = Path(__file__).resolve().parents[2]


class CreateBaseReleaseRequestTests(unittest.TestCase):
    def setUp(self) -> None:
        self.temp = tempfile.TemporaryDirectory()
        self.root = Path(self.temp.name) / "repo"
        self.output = Path(self.temp.name) / "requests"
        self.root.mkdir()
        subprocess.run(["git", "init", "-q"], cwd=self.root, check=True)
        subprocess.run(["git", "config", "user.name", "Test"], cwd=self.root, check=True)
        subprocess.run(["git", "config", "user.email", "test@example.invalid"], cwd=self.root, check=True)
        (self.root / "candidate.txt").write_text("candidate\n", encoding="utf-8")
        self.profile = self.root / "src/test-vectors/vnext/base-v1-freeze-v1.json"
        self.vector = self.root / "src/test-vectors/vnext/base-v1-release-signers-v1.json"
        self.history = self.root / "src/test-vectors/vnext/base-v1-runtime-interface-history-v1.json"
        for path in (self.profile, self.vector, self.history):
            path.parent.mkdir(parents=True, exist_ok=True)
        self.profile.write_bytes(canonical_json({"profile": 1}))
        self.vector.write_bytes((ROOT / "src/test-vectors/vnext/base-v1-release-signers-v1.json").read_bytes())
        self.history.write_bytes(canonical_json({"history_chain": {"root_sha256": "44" * 32}}))
        self.policy = self.vector
        relative_tools = {
            "qualifier": "scripts/base/qualify_base.py",
            "request": "scripts/release/create_base_release_request.py",
            "clean_worktree": "scripts/release/prepare_clean_candidate.py",
            "release_wrapper": "scripts/release/create_verified_base_release.py",
            "verifier": "scripts/release/verify_base_release_request.py",
        }
        self.tooling = {}
        for name, relative in relative_tools.items():
            path = self.root / relative
            path.parent.mkdir(parents=True, exist_ok=True)
            path.write_text(name, encoding="utf-8")
            self.tooling[name] = path
        self.tooling["signer_policy"] = self.vector
        subprocess.run(["git", "add", "."], cwd=self.root, check=True)
        subprocess.run(["git", "commit", "-qm", "candidate"], cwd=self.root, check=True)
        self.reference = {
            "target_triple": "x86_64-unknown-linux-gnu",
            "rust_toolchain_digest": "51" * 32,
            "runner_image_digest": "52" * 32,
            "probe_blake3": "53" * 32,
            "probe_signature": "54" * 32,
            "probe_signer_fingerprint": "55" * 32,
            "probe_signer_public_key": "56" * 32,
            "executable_blake3": "57" * 32,
            "python_executable_blake3": "58" * 32,
            "gpg_executable_blake3": "59" * 32,
        }
        self.registry = {
            "candidate_semantic_digest": "61" * 32,
            "artifact_tuple_digest": "62" * 32,
            "release_aggregate_root": "63" * 32,
            "registry_generation": 1,
            "payload_artifacts_blake3": {
                "OBR:concepts.obr": "64" * 32,
                "LABEL_INDEX:concepts.obr.labels.idx": "65" * 32,
                "CCID_INDEX:concepts.obr.ccids.idx": "66" * 32,
                "MANIFEST:concepts.obr.manifest.json": "67" * 32,
                "SPDX_SBOM:sbom.spdx.json": "68" * 32,
            },
            "release_stamp_blake3": "69" * 32,
            "registry_trust_policy_digest": "6a" * 32,
            "registry_signer_fingerprint": "6b" * 32,
            "ccid_inputs_blake3": {
                "old_input": "71" * 32,
                "old_obr": "72" * 32,
                "old_manifest": "73" * 32,
                "candidate_input": "74" * 32,
                "candidate_obr": "75" * 32,
                "candidate_manifest": "76" * 32,
            },
        }
        self.created = datetime(2026, 8, 27, 6, tzinfo=timezone.utc)

    def tearDown(self) -> None:
        self.temp.cleanup()

    def create(self, **overrides):
        values = {
            "candidate_root": self.root,
            "output_root": self.output,
            "approver_policy_path": self.policy,
            "signer_fingerprint": FROZEN_APPROVER_POLICY["signers"][0]["fingerprint"],
            "evidence_root_uri": "file:///external/base-v1/evidence",
            "required_targets": {
                "x86_64-unknown-linux-gnu": "81" * 32,
                "x86_64-pc-windows-msvc": "82" * 32,
                "aarch64-apple-darwin": "83" * 32,
            },
            "production_profile_path": self.profile,
            "production_vector_path": self.vector,
            "append_only_idl_history_path": self.history,
            "candidate_tooling": self.tooling,
            "registry_candidate": self.registry,
            "reference_environment": self.reference,
            "created_utc": self.created,
            "expires_utc": self.created + timedelta(hours=168),
            "session_id": "91" * 32,
            "sign_detached": lambda payload, fingerprint: b"signature:" + bytes.fromhex(fingerprint),
            "verify_detached": lambda payload, signature, fingerprint: signature == b"signature:" + bytes.fromhex(fingerprint),
            "resume": False,
        }
        values.update(overrides)
        return create_release_request(**values)

    def test_creates_canonical_content_addressed_immutable_request(self) -> None:
        result = self.create()
        request_bytes = result.request_path.read_bytes()
        request = json.loads(request_bytes)
        self.assertEqual(request_bytes, canonical_json(request))
        self.assertEqual(result.request_digest, blake3.blake3(request_bytes).hexdigest())
        self.assertEqual(result.request_path.parent.name, result.request_digest)
        self.assertEqual(request["qualification_session_id"], "91" * 32)
        self.assertEqual(set(request["required_targets"]), {
            "x86_64-unknown-linux-gnu", "x86_64-pc-windows-msvc", "aarch64-apple-darwin"
        })
        expected_tools = {name: blake3.blake3(path.read_bytes()).hexdigest() for name, path in self.tooling.items()}
        self.assertEqual(request["candidate_tooling_blake3"], expected_tools)

    def test_resume_requires_byte_and_signature_exactness(self) -> None:
        result = self.create()
        resumed = self.create(
            resume=True,
            resume_request_path=result.request_path,
            created_utc=self.created + timedelta(days=1),
            expires_utc=self.created + timedelta(days=8),
        )
        self.assertEqual(resumed, result)
        with self.assertRaisesRegex(ReleaseRequestCreationError, "explicit prior"):
            self.create(resume=True)
        result.signature_path.write_bytes(b"foreign")
        with self.assertRaisesRegex(ReleaseRequestCreationError, "signature"):
            self.create(resume=True, resume_request_path=result.request_path)

    def test_never_overwrites_and_preserves_failed_attempt(self) -> None:
        def fail_sign(_payload, _fingerprint):
            raise RuntimeError("signer unavailable")
        with self.assertRaisesRegex(ReleaseRequestCreationError, "sign"):
            self.create(sign_detached=fail_sign)
        attempts = list(self.output.glob("*/request.json"))
        self.assertEqual(len(attempts), 1)
        self.assertFalse((attempts[0].parent / "request.json.asc").exists())
        with self.assertRaises(ReleaseRequestCreationError):
            self.create()

    def test_rejects_wrong_signer_targets_expiry_or_dirty_candidate(self) -> None:
        cases = {
            "signer": {"signer_fingerprint": "A" * 40},
            "targets": {"required_targets": {"x86_64-unknown-linux-gnu": "81" * 32}},
            "expiry": {"expires_utc": self.created},
        }
        for name, overrides in cases.items():
            with self.subTest(name=name), self.assertRaises(ReleaseRequestCreationError):
                self.create(**overrides)
        (self.root / "candidate.txt").write_text("dirty\n", encoding="utf-8")
        with self.assertRaisesRegex(ReleaseRequestCreationError, "dirty"):
            self.create()

    def test_rejects_validity_too_short_for_exact_72_hour_soak(self) -> None:
        """A 24-hour request cannot authorize a 72-hour qualifying soak."""
        with self.assertRaisesRegex(ReleaseRequestCreationError, "validity"):
            self.create(expires_utc=self.created + timedelta(hours=24))

    def test_rejects_external_or_untracked_candidate_contract_paths(self) -> None:
        """Signed digests must come from the exact candidate-owned canonical paths."""
        external = Path(self.temp.name) / "external-qualifier.py"
        external.write_text("qualifier", encoding="utf-8")
        tooling = dict(self.tooling)
        tooling["qualifier"] = external
        with self.assertRaisesRegex(ReleaseRequestCreationError, "canonical|candidate|tracked"):
            self.create(candidate_tooling=tooling)

    def test_verifier_measures_every_candidate_tool_before_execution(self) -> None:
        """Skipping any signed candidate tool measurement is a security bug."""
        runtime = self.tooling["request"]
        runtime_digest = blake3.blake3(runtime.read_bytes()).hexdigest()
        self.reference["python_executable_blake3"] = runtime_digest
        self.reference["gpg_executable_blake3"] = runtime_digest
        result = self.create()
        request = json.loads(result.request_path.read_bytes())
        _verify_authenticated_tooling(
            request,
            self.vector.read_bytes(),
            python_path=runtime,
            gpg_path=runtime,
            verifier_path=self.tooling["verifier"],
            candidate_root=self.root,
        )
        self.tooling["clean_worktree"].write_text("mutated", encoding="utf-8")
        with self.assertRaisesRegex(ReleaseRequestError, "tooling|candidate"):
            _verify_authenticated_tooling(
                request,
                self.vector.read_bytes(),
                python_path=runtime,
                gpg_path=runtime,
                verifier_path=self.tooling["verifier"],
                candidate_root=self.root,
            )

    def test_task28_scripts_run_by_absolute_path_and_expose_exact_cli_contract(self) -> None:
        """Direct-file imports or Task28 argument drift must fail this contract."""
        contracts = {
            ROOT / "scripts/release/create_base_release_request.py": (
                "--new-attempt", "--candidate-commit", "--output-root", "--signer-policy",
                "--verify", "--print", "--resume",
            ),
            ROOT / "scripts/release/prepare_clean_candidate.py": (
                "--source-root", "--release-request", "--signature", "--signer-policy",
                "--read-only", "--verify-only", "--candidate-root",
            ),
            ROOT / "scripts/base/qualify_base.py": (
                "--release-request", "--release-request-signature", "--evidence-root",
                "--output-generation-root", "--ready-output", "--verify-ready",
            ),
            ROOT / "scripts/release/create_verified_base_release.py": (
                "--release-request", "--release-request-signature", "--manifest-ready",
                "--release-envelope-root", "--release-ready-output", "--signer-policy",
                "--signer-role", "--tag",
            ),
        }
        outside = Path(self.temp.name) / "outside-cwd"
        outside.mkdir()
        for script, options in contracts.items():
            with self.subTest(script=script.name):
                completed = subprocess.run(
                    [sys.executable, str(script), "--help"],
                    cwd=outside,
                    capture_output=True,
                    text=True,
                    check=False,
                )
                self.assertEqual(completed.returncode, 0, completed.stderr)
                for option in options:
                    self.assertIn(option, completed.stdout)

    def test_task28_request_print_cli_emits_only_value_and_rejects_extra_mode_args(self) -> None:
        request = {
            "format": "onebrain/base-v1-release-request/2",
            "usage": "base-release-request",
            "qualification_session_id": "ab" * 32,
            "candidate": {"commit": "1" * 40, "tree": "2" * 40, "object_format": "sha1"},
            "qualification_approver_fingerprint": "A" * 40,
            "trust_policy_digest": "3" * 64,
            "required_targets": {
                "linux": "x86_64-unknown-linux-gnu",
                "windows": "x86_64-pc-windows-msvc",
                "macos": "aarch64-apple-darwin",
            },
            "production_profile_blake3": "4" * 64,
            "production_vector_blake3": "5" * 64,
            "append_only_idl_history_root": "6" * 64,
            "created_utc": "2026-08-11T00:00:00Z",
            "expires_utc": "2026-08-18T00:00:00Z",
            "evidence_root_uri": "file:///external/evidence",
            "candidate_tooling_blake3": {
                name: f"{index:x}" * 64
                for index, name in enumerate(
                    (
                        "qualifier", "request", "clean_worktree", "release_wrapper",
                        "verifier", "signer_policy",
                    ),
                    start=7,
                )
            },
        }
        path = Path(self.temp.name) / "task28-request.json"
        path.write_bytes(canonical_json(request))
        script = ROOT / "scripts/release/create_base_release_request.py"
        outside = Path(self.temp.name) / "print-outside"
        outside.mkdir()
        completed = subprocess.run(
            [sys.executable, str(script), "--print", "qualification_session_id", "--request", str(path)],
            cwd=outside,
            capture_output=True,
            text=True,
            check=False,
        )
        self.assertEqual(completed.returncode, 0, completed.stderr)
        self.assertEqual(completed.stdout, "ab" * 32 + "\n")
        self.assertEqual(completed.stderr, "")
        rejected = subprocess.run(
            [
                sys.executable, str(script), "--print", "qualification_session_id",
                "--request", str(path), "--signature", str(path),
            ],
            cwd=outside,
            capture_output=True,
            text=True,
            check=False,
        )
        self.assertNotEqual(rejected.returncode, 0)

    def test_task28_nonhelp_cli_paths_fail_closed_from_external_cwd(self) -> None:
        """Absolute-path CLIs must execute real validation branches outside the repo."""
        outside = Path(self.temp.name) / "behavior-outside"
        outside.mkdir()
        missing = outside / "missing.json"
        cases = (
            (
                ROOT / "scripts/release/prepare_clean_candidate.py",
                ["--verify-only"],
                "release request verification arguments are incomplete",
            ),
            (
                ROOT / "scripts/base/qualify_base.py",
                ["--verify-ready", str(missing), "--release-request", str(missing)],
                "manifest ready pointer is unreadable",
            ),
            (
                ROOT / "scripts/release/create_verified_base_release.py",
                [
                    "--release-request", str(missing),
                    "--release-request-signature", str(missing),
                    "--manifest-ready", str(missing),
                    "--release-envelope-root", str(outside / "envelopes"),
                    "--release-ready-output", str(outside / "release.ready.json"),
                    "--signer-policy", str(missing),
                    "--signer-role", "base-release",
                    "--tag", "base-v1.0.0",
                ],
                "release request",
            ),
        )
        for script, arguments, expected in cases:
            with self.subTest(script=script.name):
                completed = subprocess.run(
                    [sys.executable, str(script), *arguments],
                    cwd=outside,
                    capture_output=True,
                    text=True,
                    check=False,
                )
                self.assertEqual(completed.returncode, 2)
                self.assertIn(expected, completed.stderr)
                self.assertNotIn("ModuleNotFoundError", completed.stderr)
                self.assertNotIn("Traceback", completed.stderr)


if __name__ == "__main__":
    unittest.main()

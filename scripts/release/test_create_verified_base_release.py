#!/usr/bin/env python3
"""Atomic publication tests for the verified Base v1 release wrapper."""

from __future__ import annotations

import base64
import copy
import hashlib
import json
import os
import subprocess
import tempfile
import unittest
from unittest.mock import patch
from datetime import datetime, timedelta, timezone
from pathlib import Path

import blake3

from scripts.base.qualify_base import QualificationInputs
from scripts.release.create_verified_base_release import (
    BaseReleasePublicationError,
    _create_or_exact,
    _signature_verified,
    publish_verified_base_release,
    publish_verified_base_release_for_test_nonproduction,
)
from scripts.release.verify_base_release_request import canonical_json
from scripts.release.prepare_clean_candidate import PreparedCandidate, prepared_candidate_receipt


FINGERPRINT = "F9DDAFB46FB6603E14B21B4DB0D9DBF23DBE8ED2"


class VerifiedBaseReleaseTests(unittest.TestCase):
    def setUp(self) -> None:
        self.temp = tempfile.TemporaryDirectory()
        self.repo = Path(self.temp.name) / "repo"
        self.output = Path(self.temp.name) / "release"
        self.repo.mkdir()
        subprocess.run(["git", "init", "-q"], cwd=self.repo, check=True)
        subprocess.run(["git", "config", "user.name", "Release Test"], cwd=self.repo, check=True)
        subprocess.run(["git", "config", "user.email", "release@example.invalid"], cwd=self.repo, check=True)
        (self.repo / "source.txt").write_text("candidate\n", encoding="utf-8")
        subprocess.run(["git", "add", "."], cwd=self.repo, check=True)
        subprocess.run(["git", "commit", "-qm", "candidate"], cwd=self.repo, check=True)
        self.commit = subprocess.check_output(["git", "rev-parse", "HEAD"], cwd=self.repo, text=True).strip()
        self.tree = subprocess.check_output(["git", "rev-parse", "HEAD^{tree}"], cwd=self.repo, text=True).strip()
        self.request = {
            "format": "onebrain/base-v1-release-request/1",
            "candidate": {"commit": self.commit, "tree": self.tree, "object_format": "sha1"},
            "qualification_session_id": "11" * 32,
        }
        self.request_path = Path(self.temp.name) / "request.json"
        self.request_path.write_bytes(canonical_json(self.request))
        self.request_digest = blake3.blake3(self.request_path.read_bytes()).hexdigest()
        self.manifest = {
            "format": "onebrain/base-v1-evidence-manifest/1",
            "qualification_tier": "nonproduction-test",
            "release_request_digest": self.request_digest,
            "qualification_session_id": "11" * 32,
            "candidate": {
                "commit": self.commit,
                "tree": self.tree,
                "object_format": "sha1",
                "semantic_digest": "22" * 32,
            },
            "qualified": True,
        }
        self.manifest_path = Path(self.temp.name) / "manifest.json"
        self.manifest_path.write_bytes(canonical_json(self.manifest))
        self.signers_path = Path(self.temp.name) / "signers.json"
        source = Path(__file__).resolve().parents[2] / "src/test-vectors/vnext/base-v1-release-signers-v1.json"
        self.signers_path.write_bytes(source.read_bytes())

    def tearDown(self) -> None:
        self.temp.cleanup()

    @staticmethod
    def sign(payload: bytes, usage: str, fingerprint: str) -> bytes:
        return b"SIG\0" + fingerprint.encode() + b"\0" + usage.encode() + b"\0" + hashlib.sha256(payload).digest()

    @staticmethod
    def verify(payload: bytes, signature: bytes, usage: str, fingerprint: str) -> bool:
        return signature == VerifiedBaseReleaseTests.sign(payload, usage, fingerprint)

    def publish(self, **overrides):
        values = {
            "repository": self.repo,
            "manifest_path": self.manifest_path,
            "request_path": self.request_path,
            "output_root": self.output,
            "signer_profile_path": self.signers_path,
            "signer_fingerprint": FINGERPRINT,
            "sign": self.sign,
            "verify": self.verify,
            "expected_manifest_digest": blake3.blake3(self.manifest_path.read_bytes()).hexdigest(),
            "failure_hook": None,
        }
        values.update(overrides)
        return publish_verified_base_release_for_test_nonproduction(
            verified_request_digest=self.request_digest,
            **values,
        )

    def tag_ref(self) -> str | None:
        completed = subprocess.run(
            ["git", "-C", str(self.repo), "rev-parse", "--verify", "refs/tags/base-v1.0.0"],
            capture_output=True, text=True, check=False,
        )
        return completed.stdout.strip() if completed.returncode == 0 else None

    def test_publishes_verified_envelope_pointer_then_single_cas_tag(self) -> None:
        result = self.publish()
        self.assertEqual(result.status, "Published")
        self.assertEqual(self.tag_ref(), result.tag_object)
        pointer = json.loads(result.ready_pointer.read_bytes())
        self.assertEqual(pointer["release_ready"]["manifest_digest"], result.manifest_digest)
        self.assertEqual(pointer["release_ready"]["release_request_digest"], self.request_digest)
        self.assertEqual(pointer["release_ready_digest"], blake3.blake3(canonical_json(pointer["release_ready"])).hexdigest())
        self.assertTrue(result.receipt_path.is_file())

    def test_task28_layout_publishes_only_content_addressed_detached_signature(self) -> None:
        envelope_root = Path(self.temp.name) / "release-envelopes"
        ready_output = Path(self.temp.name) / "release.ready.json"
        result = self.publish(
            release_envelope_root=envelope_root,
            release_ready_output=ready_output,
        )
        self.assertEqual(result.ready_pointer, ready_output)
        generation = envelope_root / result.manifest_digest / result.envelope_digest
        self.assertEqual({item.name for item in generation.iterdir()}, {"manifest.json.asc"})
        signature = (generation / "manifest.json.asc").read_bytes()
        self.assertEqual(blake3.blake3(signature).hexdigest(), result.envelope_digest)
        self.assertEqual(
            self.publish(
                release_envelope_root=envelope_root,
                release_ready_output=ready_output,
            ).status,
            "AlreadyPublished",
        )

    def test_release_ready_precedes_tag_object_and_retry_reconciles_interruption(self) -> None:
        ready_output = Path(self.temp.name) / "release.ready.json"

        def interrupt(point: str):
            if point == "after-ready-before-object":
                raise RuntimeError(point)

        with self.assertRaisesRegex(BaseReleasePublicationError, "after-ready-before-object"):
            self.publish(release_ready_output=ready_output, failure_hook=interrupt)
        self.assertTrue(ready_output.is_file())
        self.assertIsNone(self.tag_ref())
        result = self.publish(release_ready_output=ready_output)
        self.assertEqual(result.status, "Published")
        self.assertEqual(self.tag_ref(), result.tag_object)

    def test_task28_envelope_generation_rejects_preexisting_extra_file(self) -> None:
        envelope_root = Path(self.temp.name) / "strict-envelopes"
        manifest_digest = blake3.blake3(self.manifest_path.read_bytes()).hexdigest()
        signature = self.sign(self.manifest_path.read_bytes(), "base-evidence-manifest", FINGERPRINT)
        signature_digest = blake3.blake3(signature).hexdigest()
        generation = envelope_root / manifest_digest / signature_digest
        generation.mkdir(parents=True)
        (generation / "foreign.tmp").write_bytes(b"foreign")
        with self.assertRaisesRegex(BaseReleasePublicationError, "extra|generation|file"):
            self.publish(
                release_envelope_root=envelope_root,
                release_ready_output=Path(self.temp.name) / "strict.ready.json",
            )
        self.assertIsNone(self.tag_ref())

    def test_windows_directory_flush_is_not_a_noop(self) -> None:
        calls: list[Path] = []
        with patch(
            "scripts.release.create_verified_base_release.os.name", "nt"
        ), patch(
            "scripts.release.create_verified_base_release._flush_windows_directory",
            side_effect=lambda path: calls.append(path),
            create=True,
        ):
            from scripts.release.create_verified_base_release import _fsync_directory

            _fsync_directory(self.output)
        self.assertEqual(calls, [self.output])

    def test_windows_directory_flush_failure_is_fail_closed_and_injectable(self) -> None:
        from scripts.release.create_verified_base_release import _fsync_directory

        def fail(_path: Path) -> None:
            raise OSError("durability unavailable")

        with self.assertRaisesRegex(OSError, "durability unavailable"):
            _fsync_directory(
                self.output,
                platform_name="nt",
                windows_flusher=fail,
            )

    def test_exact_retry_is_idempotent_but_foreign_existing_ref_fails(self) -> None:
        first = self.publish()
        second = self.publish()
        self.assertEqual(second.status, "AlreadyPublished")
        self.assertEqual(second.tag_object, first.tag_object)
        other = Path(self.temp.name) / "other"
        subprocess.run(["git", "clone", "-q", str(self.repo), str(other)], check=True)
        with self.assertRaisesRegex(BaseReleasePublicationError, "foreign|existing"):
            self.publish(repository=other, output_root=Path(self.temp.name) / "other-output")

    def test_retry_rejects_same_signer_foreign_tag_and_forged_ready(self) -> None:
        """A valid signer cannot bless stale tag bytes from another candidate."""
        first = self.publish()
        subprocess.run(
            ["git", "-C", str(self.repo), "update-ref", "-d", "refs/tags/base-v1.0.0"],
            check=True,
        )
        pointer = json.loads(first.ready_pointer.read_bytes())
        ready = pointer["release_ready"]
        unsigned = base64.b64decode(ready["tag_unsigned_base64"])
        foreign_tree = "f" * len(self.tree)
        forged_unsigned = unsigned.replace(
            f"Candidate-tree: {self.tree}".encode(),
            f"Candidate-tree: {foreign_tree}".encode(),
        )
        forged_signature = self.sign(forged_unsigned, "base-release-tag", FINGERPRINT)
        forged_object = subprocess.check_output(
            ["git", "-C", str(self.repo), "hash-object", "-t", "tag", "-w", "--stdin"],
            input=forged_unsigned + forged_signature,
        ).decode().strip()
        ready.update(
            {
                "target_tree": foreign_tree,
                "tag_object": forged_object,
                "tag_unsigned_base64": base64.b64encode(forged_unsigned).decode(),
                "tag_signature_base64": base64.b64encode(forged_signature).decode(),
            }
        )
        pointer["release_ready_digest"] = blake3.blake3(canonical_json(ready)).hexdigest()
        first.ready_pointer.write_bytes(canonical_json(pointer))
        with self.assertRaisesRegex(BaseReleasePublicationError, "stale|foreign"):
            self.publish()

    def test_failure_boundaries_never_publish_unverified_ref_and_recover_after_cas(self) -> None:
        for boundary in ("before-envelope-readiness", "before-object-write", "before-cas"):
            with self.subTest(boundary=boundary):
                repo = Path(self.temp.name) / f"repo-{boundary}"
                subprocess.run(["git", "clone", "-q", str(self.repo), str(repo)], check=True)
                output = Path(self.temp.name) / f"out-{boundary}"
                def fail(point: str, expected=boundary):
                    if point == expected:
                        raise RuntimeError(point)
                with self.assertRaisesRegex(BaseReleasePublicationError, boundary):
                    self.publish(repository=repo, output_root=output, failure_hook=fail)
                completed = subprocess.run(["git", "-C", str(repo), "rev-parse", "--verify", "refs/tags/base-v1.0.0"], capture_output=True, check=False)
                self.assertNotEqual(completed.returncode, 0)

        def fail_after(point: str):
            if point == "after-cas-receipt-fsync":
                raise RuntimeError(point)
        with self.assertRaisesRegex(BaseReleasePublicationError, "after-cas"):
            self.publish(failure_hook=fail_after)
        self.assertIsNotNone(self.tag_ref())
        self.assertEqual(self.publish().status, "AlreadyPublished")

    def test_rejects_wrong_valid_key_role_stale_digest_or_mixed_manifest(self) -> None:
        wrong_key_sign = lambda payload, usage, _fingerprint: self.sign(payload, usage, "A" * 40)
        cases = {
            "wrong-key": {"sign": wrong_key_sign},
            "stale-digest": {"expected_manifest_digest": "ff" * 32},
        }
        for name, overrides in cases.items():
            with self.subTest(name=name), self.assertRaises(BaseReleasePublicationError):
                self.publish(output_root=Path(self.temp.name) / name, **overrides)
        profile = json.loads(self.signers_path.read_bytes())
        release_policy = next(row for row in profile["policies"] if row["policy"]["role"] == "base-release")
        release_policy["policy"]["role"] = "qualification-approver"
        wrong_role = Path(self.temp.name) / "wrong-role.json"
        wrong_role.write_bytes(canonical_json(profile))
        with self.assertRaisesRegex(BaseReleasePublicationError, "policy|role"):
            self.publish(output_root=Path(self.temp.name) / "wrong-role-out", signer_profile_path=wrong_role)
        manifest = copy.deepcopy(self.manifest)
        manifest["candidate"]["commit"] = "9" * 40
        mixed = Path(self.temp.name) / "mixed.json"
        mixed.write_bytes(canonical_json(manifest))
        with self.assertRaisesRegex(BaseReleasePublicationError, "candidate"):
            self.publish(
                output_root=Path(self.temp.name) / "mixed-out",
                manifest_path=mixed,
                expected_manifest_digest=blake3.blake3(mixed.read_bytes()).hexdigest(),
            )

    def test_production_requalifies_and_rejects_nonproduction_before_signing(self) -> None:
        calls = []

        def forbidden_sign(*_args):
            calls.append("sign")
            return b"must-not-sign"

        with self.assertRaisesRegex(BaseReleasePublicationError, "qualifier|production"):
            publish_verified_base_release(
                qualification_inputs=QualificationInputs(
                    document={}, evidence_bytes={}, freeze_profile={}
                ),
                verified_request_digest=self.request_digest,
                repository=self.repo,
                manifest_path=self.manifest_path,
                request_path=self.request_path,
                output_root=Path(self.temp.name) / "production-reject",
                signer_profile_path=self.signers_path,
                signer_fingerprint=FINGERPRINT,
                sign=forbidden_sign,
                verify=self.verify,
                expected_manifest_digest=blake3.blake3(
                    self.manifest_path.read_bytes()
                ).hexdigest(),
            )
        self.assertEqual(calls, [])

    def test_production_finalizer_rejects_post_qualification_mutation_before_signing(self) -> None:
        calls: list[str] = []
        production_manifest = copy.deepcopy(self.manifest)
        production_manifest["qualification_tier"] = "production"
        production_path = Path(self.temp.name) / "production-manifest.json"
        production_path.write_bytes(canonical_json(production_manifest))
        output = Path(self.temp.name) / "external-output"
        output.mkdir()
        receipt = prepared_candidate_receipt(
            PreparedCandidate(
                worktree=self.repo,
                commit=self.commit,
                tree=self.tree,
                object_format="sha1",
                request_digest=self.request_digest,
                qualification_session_id="11" * 32,
                environment={"CARGO_TARGET_DIR": str(output)},
                tracked_blake3={
                    "source.txt": blake3.blake3((self.repo / "source.txt").read_bytes()).hexdigest()
                },
            )
        )
        receipt_path = Path(self.temp.name) / "prepared-candidate.json"
        receipt_path.write_bytes(
            json.dumps(receipt, sort_keys=True, separators=(",", ":")).encode()
        )
        (self.repo / "source.txt").write_text("mutated after qualification\n", encoding="utf-8")

        def forbidden_sign(*_args):
            calls.append("sign")
            return b"must-not-sign"

        with patch(
            "scripts.release.create_verified_base_release.qualify_base",
            return_value=production_manifest,
        ), self.assertRaisesRegex(BaseReleasePublicationError, "final filesystem|finalizer"):
            publish_verified_base_release(
                qualification_inputs=QualificationInputs({}, {}, {}),
                verified_request_digest=self.request_digest,
                prepared_candidate_receipt=receipt_path,
                prepared_candidate_receipt_digest=blake3.blake3(
                    receipt_path.read_bytes()
                ).hexdigest(),
                candidate_root=self.repo,
                repository=self.repo,
                manifest_path=production_path,
                request_path=self.request_path,
                output_root=self.output,
                signer_profile_path=self.signers_path,
                signer_fingerprint=FINGERPRINT,
                sign=forbidden_sign,
                verify=self.verify,
                expected_manifest_digest=blake3.blake3(production_path.read_bytes()).hexdigest(),
            )
        self.assertEqual(calls, [])

    def test_production_rejects_clean_decoy_receipt_for_dirty_actual_candidate(self) -> None:
        decoy = Path(self.temp.name) / "clean-decoy"
        subprocess.run(["git", "clone", "-q", str(self.repo), str(decoy)], check=True)
        output = Path(self.temp.name) / "decoy-output"
        output.mkdir()
        request = {
            **self.request,
            "created_utc": "2026-08-11T05:00:00Z",
            "expires_utc": "2026-08-18T05:00:00Z",
        }
        request_path = Path(self.temp.name) / "decoy-request.json"
        request_path.write_bytes(canonical_json(request))
        request_digest = blake3.blake3(request_path.read_bytes()).hexdigest()
        manifest = {
            **copy.deepcopy(self.manifest),
            "qualification_tier": "production",
            "release_request_digest": request_digest,
        }
        manifest_path = Path(self.temp.name) / "decoy-manifest.json"
        manifest_path.write_bytes(canonical_json(manifest))
        receipt = prepared_candidate_receipt(
            PreparedCandidate(
                worktree=decoy,
                commit=self.commit,
                tree=self.tree,
                object_format="sha1",
                request_digest=request_digest,
                qualification_session_id="11" * 32,
                environment={"CARGO_TARGET_DIR": str(output)},
                tracked_blake3={
                    "source.txt": blake3.blake3((decoy / "source.txt").read_bytes()).hexdigest()
                },
            )
        )
        receipt_path = Path(self.temp.name) / "decoy-prepared.json"
        receipt_path.write_bytes(json.dumps(receipt, sort_keys=True, separators=(",", ":")).encode())
        receipt_digest = blake3.blake3(receipt_path.read_bytes()).hexdigest()
        (self.repo / "source.txt").write_text("dirty actual candidate\n", encoding="utf-8")
        calls: list[str] = []

        def forbidden_sign(*_args):
            calls.append("sign")
            return b"must-not-sign"

        with patch(
            "scripts.release.create_verified_base_release.qualify_base", return_value=manifest
        ), self.assertRaisesRegex(BaseReleasePublicationError, "candidate root|receipt worktree"):
            publish_verified_base_release(
                qualification_inputs=QualificationInputs({}, {}, {}),
                verified_request_digest=request_digest,
                prepared_candidate_receipt=receipt_path,
                prepared_candidate_receipt_digest=receipt_digest,
                candidate_root=self.repo,
                repository=self.repo,
                manifest_path=manifest_path,
                request_path=request_path,
                output_root=self.output,
                signer_profile_path=self.signers_path,
                signer_fingerprint=FINGERPRINT,
                sign=forbidden_sign,
                verify=self.verify,
                expected_manifest_digest=blake3.blake3(manifest_path.read_bytes()).hexdigest(),
            )
        self.assertEqual(calls, [])

    def test_production_signature_time_must_be_inside_request_and_signer_interval(self) -> None:
        """Accepting a timeless or out-of-window VALIDSIG is a release bug."""
        start = datetime(2026, 8, 11, tzinfo=timezone.utc)
        interval = (start, start + timedelta(hours=168))
        arguments = (b"payload", b"signature", "usage", FINGERPRINT)
        self.assertFalse(_signature_verified(lambda *_: True, *arguments, interval))
        self.assertFalse(
            _signature_verified(lambda *_: start - timedelta(seconds=1), *arguments, interval)
        )
        self.assertTrue(
            _signature_verified(lambda *_: start + timedelta(hours=72), *arguments, interval)
        )
        self.assertFalse(_signature_verified(lambda *_: interval[1], *arguments, interval))

    def test_release_publication_links_durable_bytes_before_ready_directory_fsync(self) -> None:
        root = Path(self.temp.name) / "durability"
        root.mkdir()
        path = root / "release.ready.json"
        events: list[str] = []
        real_link = os.link

        def observed_link(source, destination):
            self.assertTrue(Path(source).is_file())
            self.assertFalse(Path(destination).exists())
            events.append("link")
            return real_link(source, destination)

        with patch(
            "scripts.release.create_verified_base_release.os.link",
            side_effect=observed_link,
        ), patch(
            "scripts.release.create_verified_base_release._fsync_directory",
            side_effect=lambda _path: events.append("fsync"),
        ):
            _create_or_exact(path, b"ready")
        self.assertEqual(events, ["link", "fsync"])
        self.assertEqual(path.read_bytes(), b"ready")


if __name__ == "__main__":
    unittest.main()

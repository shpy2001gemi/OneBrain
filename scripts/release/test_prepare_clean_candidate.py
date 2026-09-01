#!/usr/bin/env python3
"""Tests for detached, immutable Base candidate preparation."""

from __future__ import annotations

import os
import json
import stat
import subprocess
import tempfile
import unittest
from pathlib import Path

from scripts.release.prepare_clean_candidate import (
    CleanCandidateError,
    finalize_prepared_candidate,
    finalize_prepared_candidate_receipt,
    prepared_candidate_receipt,
    prepare_clean_candidate,
)
from scripts.release.verify_base_release_request import VerifiedQualificationContextV1


class PrepareCleanCandidateTests(unittest.TestCase):
    def setUp(self) -> None:
        self.temp = tempfile.TemporaryDirectory()
        self.source = Path(self.temp.name) / "source"
        self.external = Path(self.temp.name) / "external"
        self.source.mkdir()
        self.external.mkdir()
        subprocess.run(["git", "init", "-q"], cwd=self.source, check=True)
        subprocess.run(["git", "config", "user.name", "Test"], cwd=self.source, check=True)
        subprocess.run(["git", "config", "user.email", "test@example.invalid"], cwd=self.source, check=True)
        (self.source / ".gitignore").write_text("ignored/\ntarget/\n", encoding="utf-8")
        (self.source / "source.txt").write_text("immutable\n", encoding="utf-8")
        subprocess.run(["git", "add", "."], cwd=self.source, check=True)
        subprocess.run(["git", "commit", "-qm", "candidate"], cwd=self.source, check=True)
        self.commit = subprocess.check_output(["git", "rev-parse", "HEAD"], cwd=self.source, text=True).strip()
        self.tree = subprocess.check_output(["git", "rev-parse", "HEAD^{tree}"], cwd=self.source, text=True).strip()
        self.context = VerifiedQualificationContextV1(
            request_digest="11" * 32,
            signer_fingerprint="A" * 40,
            trust_policy_digest="22" * 32,
            run_context={
                "format": "onebrain/qualification-run-context/1",
                "variant": "Release",
                "release_request_digest": "11" * 32,
                "qualification_session_id": "33" * 32,
                "candidate_commit": self.commit,
                "candidate_tree": self.tree,
            },
            bindings={},
            tooling_blake3={},
            production=True,
        )
        self.request = Path(self.temp.name) / "request.json"
        self.signature = Path(self.temp.name) / "request.json.asc"
        self.policy = Path(self.temp.name) / "policy.json"
        for path in (self.request, self.signature, self.policy):
            path.write_bytes(path.name.encode())

    def tearDown(self) -> None:
        self.temp.cleanup()

    def prepare(self, **overrides):
        values = {
            "source_root": self.source,
            "external_root": self.external,
            "request_path": self.request,
            "signature_path": self.signature,
            "policy_path": self.policy,
            "gpg_home": self.external,
            "verify_request": lambda *_args: self.context,
            "after_checkout": None,
        }
        values.update(overrides)
        return prepare_clean_candidate(**values)

    def test_creates_exact_detached_read_only_candidate_and_external_outputs(self) -> None:
        prepared = self.prepare()
        self.assertFalse(str(prepared.worktree).startswith(str(self.source)))
        self.assertEqual(
            subprocess.check_output(["git", "-C", str(prepared.worktree), "rev-parse", "HEAD"], text=True).strip(),
            self.commit,
        )
        self.assertEqual(prepared.commit, self.commit)
        self.assertEqual(prepared.tree, self.tree)
        mode = (prepared.worktree / "source.txt").stat().st_mode
        self.assertFalse(mode & stat.S_IWUSR)
        for value in prepared.environment.values():
            self.assertTrue(str(Path(value)).startswith(str(self.external)))
            self.assertFalse(str(Path(value)).startswith(str(prepared.worktree)))

    def test_verifies_request_before_creating_worktree(self) -> None:
        calls = []
        def reject(*_args):
            calls.append("verify")
            raise RuntimeError("bad request")
        with self.assertRaisesRegex(CleanCandidateError, "release request"):
            self.prepare(verify_request=reject)
        self.assertEqual(calls, ["verify"])
        self.assertEqual(list(self.external.iterdir()), [])

    def test_rejects_dirty_untracked_ignored_or_generated_candidate_files(self) -> None:
        mutations = {
            "dirty": lambda root: (root / "source.txt").write_text("changed\n", encoding="utf-8"),
            "untracked": lambda root: (root / "untracked.txt").write_text("new\n", encoding="utf-8"),
            "ignored": lambda root: ((root / "ignored").mkdir(), (root / "ignored/cache").write_text("x", encoding="utf-8")),
            "generated": lambda root: ((root / "target").mkdir(), (root / "target/generated").write_text("x", encoding="utf-8")),
        }
        for name, mutate in mutations.items():
            with self.subTest(name=name), self.assertRaises(CleanCandidateError):
                isolated = self.external / name
                isolated.mkdir()
                self.prepare(external_root=isolated, after_checkout=mutate)

    def test_rejects_source_dirt_and_mixed_commit_or_tree(self) -> None:
        (self.source / "untracked.txt").write_text("x", encoding="utf-8")
        with self.assertRaisesRegex(CleanCandidateError, "source worktree"):
            self.prepare()
        (self.source / "untracked.txt").unlink()
        for field in ("candidate_commit", "candidate_tree"):
            context = VerifiedQualificationContextV1(
                **{**self.context.__dict__, "run_context": {**self.context.run_context, field: "9" * 40}}
            )
            with self.subTest(field=field), self.assertRaises(CleanCandidateError):
                self.prepare(verify_request=lambda *_args, context=context: context)

    def test_source_bootstrap_allows_only_exact_ignored_request_attempt(self) -> None:
        request_root = self.source / "target" / "base-v1" / "release-requests" / "digest"
        request_root.mkdir(parents=True)
        request = request_root / "release-request.json"
        signature = request_root / "release-request.json.asc"
        request.write_bytes(b"request")
        signature.write_bytes(b"signature")
        prepared = self.prepare(allowed_source_artifacts=(request, signature))
        self.assertEqual(prepared.commit, self.commit)

        extra = self.source / "target" / "unexpected.cache"
        extra.write_bytes(b"unexpected")
        with self.assertRaisesRegex(CleanCandidateError, "source worktree"):
            self.prepare(allowed_source_artifacts=(request, signature))

    def test_source_bootstrap_accepts_verified_request_artifacts_outside_source(self) -> None:
        prepared = self.prepare(
            allowed_source_artifacts=(self.request, self.signature)
        )
        self.assertEqual(prepared.commit, self.commit)
        self.assertFalse(self.request.is_relative_to(self.source))

    def test_finalizer_detects_post_return_candidate_mutation(self) -> None:
        """Dropping the post-run integrity check must make this test fail."""
        prepared = self.prepare()
        source = prepared.worktree / "source.txt"
        source.chmod(source.stat().st_mode | stat.S_IWUSR)
        source.write_text("post-run mutation\n", encoding="utf-8")
        with self.assertRaisesRegex(CleanCandidateError, "post-run|integrity|candidate"):
            finalize_prepared_candidate(prepared)

    def test_persisted_receipt_drives_exact_post_run_finalization(self) -> None:
        """Removing the cross-process lifecycle receipt must make this fail."""
        prepared = self.prepare()
        receipt = self.external / "prepared.json"
        receipt.write_bytes(
            json.dumps(
                prepared_candidate_receipt(prepared),
                sort_keys=True,
                separators=(",", ":"),
            ).encode("utf-8")
        )
        finalized = finalize_prepared_candidate_receipt(receipt)
        self.assertEqual(finalized.status, "post-run-integrity-verified")
        self.assertEqual(finalized.release_request_digest, self.context.request_digest)


if __name__ == "__main__":
    unittest.main()

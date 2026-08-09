"""Regression tests for the closed signed Base release-request verifier."""

from __future__ import annotations

import json
import os
import shutil
import subprocess
import sys
import tempfile
import unittest
from datetime import datetime, timedelta, timezone
from pathlib import Path

import blake3

SCRIPT_DIR = Path(__file__).resolve().parent
if str(SCRIPT_DIR) not in sys.path:
    sys.path.insert(0, str(SCRIPT_DIR))

from verify_base_release_request import (  # noqa: E402
    APPROVER_POLICY_DIGEST,
    APPROVER_POLICY_DIGEST_CONTEXT,
    FROZEN_APPROVER_POLICY,
    ReleaseRequestError,
    canonical_json,
    verify_release_request,
)


def _gpg() -> str:
    value = shutil.which("gpg")
    if value:
        return value
    bundled = Path(r"C:\Program Files\Git\usr\bin\gpg.exe")
    if bundled.is_file():
        return str(bundled)
    raise unittest.SkipTest("GPG is required for detached-signature tests")


def _run_gpg(home: Path, *arguments: str) -> subprocess.CompletedProcess[str]:
    bash = Path(r"C:\Program Files\Git\usr\bin\bash.exe")
    if bash.is_file():
        # Git's gpg.exe locates gpg-agent through the MSYS /usr mount.  Launch
        # key-generation/signing calls under Git Bash while keeping all state
        # inside this test's isolated home.
        return subprocess.run(
            [
                str(bash), "-lc",
                'home=$(cygpath -u "$1"); shift; exec /usr/bin/gpg --homedir "$home" --batch --no-tty --pinentry-mode loopback "$@"',
                "task20-gpg", str(home), *arguments,
            ],
            check=True,
            capture_output=True,
            text=True,
        )
    return subprocess.run(
        [_gpg(), "--homedir", str(home), "--batch", "--no-tty", "--pinentry-mode", "loopback", *arguments],
        check=True,
        capture_output=True,
        text=True,
    )


class SignedReleaseRequestTests(unittest.TestCase):
    def setUp(self) -> None:
        self.temporary = tempfile.TemporaryDirectory()
        self.root = Path(self.temporary.name)
        self.gpg_home = self.root / "gnupg"
        self.gpg_home.mkdir()
        _run_gpg(
            self.gpg_home,
            "--passphrase",
            "",
            "--quick-generate-key",
            "Task20 Ephemeral <task20@example.invalid>",
            "ed25519",
            "sign",
            "1d",
        )
        listing = _run_gpg(self.gpg_home, "--with-colons", "--list-keys").stdout
        self.fingerprint = next(
            line.split(":")[9]
            for line in listing.splitlines()
            if line.startswith("fpr:")
        )
        # Export again in binary mode so packet hashing is byte-exact.
        binary = subprocess.run(
            [_gpg(), "--homedir", str(self.gpg_home), "--batch", "--export", self.fingerprint],
            check=True,
            capture_output=True,
        ).stdout
        now = datetime.now(timezone.utc).replace(microsecond=0)
        self.policy = {
            "algorithm": "OpenPGP-Ed25519",
            "allowed_usages": ["base-release-request"],
            "format": "onebrain/base-v1-qualification-approver-policy/1",
            "role": "qualification-approver",
            "signers": [{
                "created_utc": (now - timedelta(minutes=5)).isoformat().replace("+00:00", "Z"),
                "expires_utc": (now + timedelta(hours=12)).isoformat().replace("+00:00", "Z"),
                "fingerprint": self.fingerprint,
                "key_id": self.fingerprint[-16:],
                "public_key_packet_blake3": blake3.blake3(binary).hexdigest(),
            }],
            "valid_unlisted_signature": "reject",
            "verification": {
                "fingerprint_source": "gpg-status-fd-VALIDSIG-full-primary-fingerprint",
                "trust_model": "explicit-allowlist",
            },
        }
        self.policy_digest = blake3.blake3(
            canonical_json(self.policy),
            derive_key_context=APPROVER_POLICY_DIGEST_CONTEXT,
        ).hexdigest()
        self.request = {
            "format": "onebrain/base-v1-release-request/1",
            "usage": "base-release-request",
            "qualification_session_id": "42" * 32,
            "candidate": {
                "commit": "11" * 20,
                "tree": "22" * 20,
                "object_format": "sha1",
            },
            "qualification_approver_fingerprint": self.fingerprint,
            "trust_policy_digest": self.policy_digest,
            "required_targets": {
                "x86_64-unknown-linux-gnu": "31" * 32,
                "x86_64-pc-windows-msvc": "32" * 32,
                "aarch64-apple-darwin": "33" * 32,
            },
            "production_profile_blake3": "44" * 32,
            "production_vector_blake3": "45" * 32,
            "append_only_idl_history_root": "46" * 32,
            "created_utc": now.isoformat().replace("+00:00", "Z"),
            "expires_utc": (now + timedelta(hours=1)).isoformat().replace("+00:00", "Z"),
            "evidence_root_uri": "file:///qualification/evidence/session-42",
            "candidate_tooling_blake3": {
                "qualifier": "51" * 32,
                "request": "52" * 32,
                "clean_worktree": "53" * 32,
                "release_wrapper": "54" * 32,
                "verifier": "55" * 32,
                "signer_policy": "56" * 32,
            },
            "registry_candidate": {
                "candidate_semantic_digest": "61" * 32,
                "artifact_tuple_digest": "62" * 32,
                "release_aggregate_root": "63" * 32,
                "registry_generation": 7,
                "payload_artifacts_blake3": {
                    "OBR:concepts.obr": "71" * 32,
                    "LABEL_INDEX:concepts.obr.labels.idx": "72" * 32,
                    "CCID_INDEX:concepts.obr.ccids.idx": "73" * 32,
                    "MANIFEST:concepts.obr.manifest.json": "74" * 32,
                    "SPDX_SBOM:sbom.spdx.json": "75" * 32,
                },
                "release_stamp_blake3": "76" * 32,
                "registry_trust_policy_digest": "77" * 32,
                "registry_signer_fingerprint": "78" * 32,
                "ccid_inputs_blake3": {
                    "old_input": "91" * 32,
                    "old_obr": "92" * 32,
                    "old_manifest": "93" * 32,
                    "candidate_input": "94" * 32,
                    "candidate_obr": "95" * 32,
                    "candidate_manifest": "96" * 32,
                },
            },
            "reference_environment": {
                "target_triple": "x86_64-unknown-linux-gnu",
                "rust_toolchain_digest": "81" * 32,
                "runner_image_digest": "82" * 32,
                "probe_blake3": "83" * 32,
                "probe_signature": "84" * 32,
                "probe_signer_fingerprint": "85" * 32,
                "probe_signer_public_key": "87" * 32,
                "executable_blake3": "86" * 32,
            },
        }

    def tearDown(self) -> None:
        self.temporary.cleanup()

    def _signed_paths(self) -> tuple[Path, Path, Path]:
        request_path = self.root / "request.json"
        signature_path = self.root / "request.json.asc"
        policy_path = self.root / "policy.json"
        request_path.write_bytes(canonical_json(self.request))
        policy_path.write_bytes(canonical_json(self.policy))
        signature_path.unlink(missing_ok=True)
        _run_gpg(
            self.gpg_home,
            "--local-user",
            self.fingerprint,
            "--detach-sign",
            "--output",
            str(signature_path),
            str(request_path),
        )
        return request_path, signature_path, policy_path

    def test_frozen_policy_digest_recomputes_from_exact_canonical_preimage(self) -> None:
        digest = blake3.blake3(
            canonical_json(FROZEN_APPROVER_POLICY),
            derive_key_context=APPROVER_POLICY_DIGEST_CONTEXT,
        ).hexdigest()
        self.assertEqual(digest, APPROVER_POLICY_DIGEST)

    def test_valid_signature_yields_closed_context_derived_from_request(self) -> None:
        request_path, signature_path, policy_path = self._signed_paths()
        verified = verify_release_request(
            request_path,
            signature_path,
            policy_path,
            self.gpg_home,
            gpg_executable=Path(_gpg()),
            production=False,
        )
        self.assertFalse(verified.production)
        self.assertEqual(verified.run_context["candidate_commit"], self.request["candidate"]["commit"])
        self.assertEqual(verified.bindings["release_request_digest"], blake3.blake3(request_path.read_bytes()).hexdigest())
        self.assertEqual(verified.bindings["candidate_payload_artifacts_blake3"], self.request["registry_candidate"]["payload_artifacts_blake3"])

    def test_tamper_unlisted_expired_and_unknown_field_fail_closed(self) -> None:
        request_path, signature_path, policy_path = self._signed_paths()
        request_path.write_bytes(canonical_json({**self.request, "qualification_session_id": "99" * 32}))
        with self.assertRaisesRegex(ReleaseRequestError, "signature"):
            verify_release_request(request_path, signature_path, policy_path, self.gpg_home, gpg_executable=Path(_gpg()), production=False)

        unlisted = json.loads(policy_path.read_text())
        unlisted["signers"][0]["fingerprint"] = "A" * 40
        unlisted["signers"][0]["key_id"] = "A" * 16
        policy_path.write_bytes(canonical_json(unlisted))
        unlisted_request = dict(self.request)
        unlisted_request["qualification_approver_fingerprint"] = "A" * 40
        unlisted_request["trust_policy_digest"] = blake3.blake3(
            canonical_json(unlisted), derive_key_context=APPROVER_POLICY_DIGEST_CONTEXT
        ).hexdigest()
        request_path.write_bytes(canonical_json(unlisted_request))
        signature_path.unlink()
        _run_gpg(self.gpg_home, "--local-user", self.fingerprint, "--detach-sign", "--output", str(signature_path), str(request_path))
        with self.assertRaisesRegex(ReleaseRequestError, "allowlist"):
            verify_release_request(request_path, signature_path, policy_path, self.gpg_home, gpg_executable=Path(_gpg()), production=False)

        request_path, signature_path, policy_path = self._signed_paths()
        expired = dict(self.request)
        expired["expires_utc"] = "2020-01-01T00:00:00Z"
        request_path.write_bytes(canonical_json(expired))
        signature_path.unlink()
        _run_gpg(self.gpg_home, "--local-user", self.fingerprint, "--detach-sign", "--output", str(signature_path), str(request_path))
        with self.assertRaisesRegex(ReleaseRequestError, "validity|expired"):
            verify_release_request(request_path, signature_path, policy_path, self.gpg_home, gpg_executable=Path(_gpg()), production=False)

        request_path, signature_path, policy_path = self._signed_paths()
        extended = {**self.request, "qualified": True}
        request_path.write_bytes(canonical_json(extended))
        signature_path.unlink()
        _run_gpg(self.gpg_home, "--local-user", self.fingerprint, "--detach-sign", "--output", str(signature_path), str(request_path))
        with self.assertRaisesRegex(ReleaseRequestError, "closed"):
            verify_release_request(request_path, signature_path, policy_path, self.gpg_home, gpg_executable=Path(_gpg()), production=False)


if __name__ == "__main__":
    unittest.main()

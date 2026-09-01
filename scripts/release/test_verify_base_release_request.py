"""Regression tests for the closed signed Base release-request verifier."""

from __future__ import annotations

import json
import inspect
import os
import shutil
import subprocess
import sys
import tempfile
import unittest
from unittest.mock import patch
from datetime import datetime, timedelta, timezone
from pathlib import Path

import blake3
from cryptography.hazmat.primitives.asymmetric.ed25519 import Ed25519PrivateKey

SCRIPT_DIR = Path(__file__).resolve().parent
if str(SCRIPT_DIR) not in sys.path:
    sys.path.insert(0, str(SCRIPT_DIR))
CONCEPT_DIR = SCRIPT_DIR.parent / "concept_registry"
if str(CONCEPT_DIR) not in sys.path:
    sys.path.insert(0, str(CONCEPT_DIR))

from verify_base_release_request import (  # noqa: E402
    APPROVER_POLICY_DIGEST,
    APPROVER_POLICY_DIGEST_CONTEXT,
    FROZEN_APPROVER_POLICY,
    ReleaseRequestError,
    canonical_json,
    canonical_compatibility_tuple_bytes,
    bind_task28_registry_measurements,
    verify_release_request,
    verify_release_request_for_test_nonproduction,
    verify_registry_candidate_measurements,
    verify_task28_release_request_for_test_nonproduction,
    python_executable_path,
    _verify_authenticated_tooling,
)
from build_obr import build  # noqa: E402
from config import SOURCE_WIKIDATA  # noqa: E402
from production_qualification import signer_fingerprint, trust_policy_digest  # noqa: E402
import release_cycle_qualification as release_cycle_module  # noqa: E402
from release_cycle_qualification import (  # noqa: E402
    REQUIRED_STEPS,
    CycleError,
    _latest_state,
    _stamp,
    run_release_cycle_for_test_nonproduction,
)
from ccid_stability_qualification import (  # noqa: E402
    qualify_ccid_stability_from_signed_request_for_test_nonproduction,
)
from resource_qualification import (  # noqa: E402
    QualificationError,
    create_verified_resource_receipt_for_test_nonproduction,
)
from production_qualification import _aggregate_reports_for_test_nonproduction  # noqa: E402


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
                "python_executable_blake3": blake3.blake3(python_executable_path().read_bytes()).hexdigest(),
                "gpg_executable_blake3": blake3.blake3(Path(_gpg()).read_bytes()).hexdigest(),
            },
        }

    def tearDown(self) -> None:
        self.temporary.cleanup()

    def _compatibility_tuple(self, commit: str, target: str, toolchain_digest: str) -> dict[str, object]:
        vector = json.loads(
            (SCRIPT_DIR.parents[1] / "src/test-vectors/vnext/base-v1-compatibility-v1.json").read_text(
                encoding="utf-8"
            )
        )
        value = json.loads(json.dumps(vector["baseline"]))
        value["base_commit"] = {"kind": "sha1" if len(commit) == 40 else "sha256", "hex": commit}
        value["target_triple"] = target
        value["toolchain"] = {"kind": "known", "hex": toolchain_digest}
        return value

    def _signed_paths(self) -> tuple[Path, Path, Path]:
        request_path = self.root / "request.json"
        signature_path = self.root / "request.json.asc"
        policy_path = self.root / "policy.json"
        policy_bytes = canonical_json(self.policy)
        self.request["candidate_tooling_blake3"]["verifier"] = blake3.blake3(
            (SCRIPT_DIR / "verify_base_release_request.py").read_bytes()
        ).hexdigest()
        self.request["candidate_tooling_blake3"]["signer_policy"] = blake3.blake3(policy_bytes).hexdigest()
        request_path.write_bytes(canonical_json(self.request))
        policy_path.write_bytes(policy_bytes)
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

    def test_approved_compatibility_tuple_bytes_recompute_semantic_and_artifact_digests(self) -> None:
        vector = json.loads(
            (SCRIPT_DIR.parents[1] / "src/test-vectors/vnext/base-v1-compatibility-v1.json").read_text(
                encoding="utf-8"
            )
        )
        semantic = canonical_compatibility_tuple_bytes(vector["baseline"], include_artifact_fields=False)
        artifact = canonical_compatibility_tuple_bytes(vector["baseline"], include_artifact_fields=True)
        self.assertEqual(
            blake3.blake3(semantic, derive_key_context="onebrain:base:candidate-semantic:1\0").hexdigest(),
            vector["golden_digests"]["candidate_semantic"],
        )
        self.assertEqual(
            blake3.blake3(artifact, derive_key_context="onebrain:base:artifact-tuple:1\0").hexdigest(),
            vector["golden_digests"]["artifact_tuple"],
        )

    def test_production_verifier_api_has_no_executable_or_policy_mode_injection(self) -> None:
        parameters = inspect.signature(verify_release_request).parameters
        self.assertNotIn("gpg_executable", parameters)
        self.assertNotIn("production", parameters)

    def test_explicit_nonproduction_cli_can_never_return_production_context(self) -> None:
        request_path, signature_path, policy_path = self._signed_paths()
        result = subprocess.run(
            [
                sys.executable,
                str(SCRIPT_DIR / "verify_base_release_request.py"),
                "--request", str(request_path),
                "--signature", str(signature_path),
                "--policy", str(policy_path),
                "--gpg-home", str(self.gpg_home),
                "--test-nonproduction-gpg", _gpg(),
            ],
            capture_output=True,
            text=True,
            encoding="utf-8",
            errors="replace",
            check=False,
        )
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertFalse(json.loads(result.stdout)["production"])

    def test_task28_v2_context_excludes_future_registry_results_then_extends_without_override(self) -> None:
        now = datetime.now(timezone.utc).replace(microsecond=0)
        self.policy["signers"][0]["expires_utc"] = (
            now + timedelta(days=8)
        ).isoformat().replace("+00:00", "Z")
        self.policy_digest = blake3.blake3(
            canonical_json(self.policy),
            derive_key_context=APPROVER_POLICY_DIGEST_CONTEXT,
        ).hexdigest()
        self.request = {
            key: value
            for key, value in self.request.items()
            if key not in {"registry_candidate", "reference_environment"}
        }
        self.request.update({
            "format": "onebrain/base-v1-release-request/2",
            "trust_policy_digest": self.policy_digest,
            "required_targets": {
                "linux": "x86_64-unknown-linux-gnu",
                "windows": "x86_64-pc-windows-msvc",
                "macos": "aarch64-apple-darwin",
            },
            "created_utc": now.isoformat().replace("+00:00", "Z"),
            "expires_utc": (now + timedelta(hours=168)).isoformat().replace("+00:00", "Z"),
        })
        request_path, signature_path, policy_path = self._signed_paths()
        verified = verify_task28_release_request_for_test_nonproduction(
            request_path,
            signature_path,
            policy_path,
            gpg_home=self.gpg_home,
            gpg_executable=Path(_gpg()),
            now=now,
        )
        self.assertEqual(verified.as_dict()["format"], "onebrain/verified-qualification-context/2")
        self.assertNotIn("release_aggregate_root", verified.bindings)
        registry = {
            "candidate_semantic_digest": "61" * 32,
            "artifact_tuple_digest": "62" * 32,
            "release_aggregate_root": "63" * 32,
            "registry_generation": 7,
            "candidate_payload_artifacts_blake3": {
                name: f"{index:x}" * 64
                for index, name in enumerate(sorted({
                    "OBR:concepts.obr", "LABEL_INDEX:concepts.obr.labels.idx",
                    "CCID_INDEX:concepts.obr.ccids.idx", "MANIFEST:concepts.obr.manifest.json",
                    "SPDX_SBOM:sbom.spdx.json",
                }), start=1)
            },
            "release_stamp_blake3": "76" * 32,
            "trust_policy_digest": "77" * 32,
            "signer_fingerprint": "78" * 32,
            "ccid_inputs_blake3": {
                name: f"{index:x}" * 64
                for index, name in enumerate((
                    "old_input", "old_obr", "old_manifest", "candidate_input",
                    "candidate_obr", "candidate_manifest",
                ), start=1)
            },
            "probe_blake3": "81" * 32,
            "probe_signature": "82" * 32,
            "probe_signer_fingerprint": "83" * 32,
            "probe_signer_public_key": "84" * 32,
            "executable_blake3": "85" * 32,
            "rust_toolchain_digest": "86" * 32,
            "runner_image_digest": "87" * 32,
            "target_triple": "x86_64-unknown-linux-gnu",
        }
        bound = bind_task28_registry_measurements(verified, registry)
        self.assertEqual(bound.bindings["release_aggregate_root"], "63" * 32)
        mutated = dict(registry)
        mutated["release_request_digest"] = verified.request_digest
        with self.assertRaisesRegex(ReleaseRequestError, "fields are not closed"):
            bind_task28_registry_measurements(verified, mutated)

    def test_authenticated_runtime_tooling_identity_rejects_python_and_gpg_mutations(self) -> None:
        for field, message in (
            ("python_executable_blake3", "Python executable"),
            ("gpg_executable_blake3", "GPG executable"),
        ):
            with self.subTest(field=field):
                self.request["reference_environment"][field] = "00" * 32
                request_path, signature_path, policy_path = self._signed_paths()
                with self.assertRaisesRegex(ReleaseRequestError, message):
                    verify_release_request_for_test_nonproduction(
                        request_path, signature_path, policy_path, self.gpg_home,
                        gpg_executable=Path(_gpg()),
                    )
                self.request["reference_environment"][field] = blake3.blake3(
                    (python_executable_path() if field.startswith("python") else Path(_gpg())).read_bytes()
                ).hexdigest()

    def test_authenticated_verifier_tooling_identity_rejects_altered_bytes(self) -> None:
        request_path, signature_path, policy_path = self._signed_paths()
        self.request["candidate_tooling_blake3"]["verifier"] = "00" * 32
        request_path.write_bytes(canonical_json(self.request))
        signature_path.unlink()
        _run_gpg(
            self.gpg_home, "--local-user", self.fingerprint, "--detach-sign",
            "--output", str(signature_path), str(request_path),
        )
        with self.assertRaisesRegex(ReleaseRequestError, "verifier tooling"):
            verify_release_request_for_test_nonproduction(
                request_path, signature_path, policy_path, self.gpg_home,
                gpg_executable=Path(_gpg()),
            )

    def test_actual_python_verifier_and_gpg_byte_mutations_fail_measurement(self) -> None:
        _request_path, _signature_path, policy_path = self._signed_paths()
        paths = {
            "python_path": python_executable_path(),
            "gpg_path": Path(_gpg()),
            "verifier_path": SCRIPT_DIR / "verify_base_release_request.py",
        }
        for parameter, source in paths.items():
            with self.subTest(parameter=parameter):
                altered = self.root / f"altered-{source.name}"
                shutil.copyfile(source, altered)
                with altered.open("ab") as handle:
                    handle.write(b"altered")
                arguments = dict(paths)
                arguments[parameter] = altered
                with self.assertRaisesRegex(ReleaseRequestError, "digest mismatch"):
                    _verify_authenticated_tooling(
                        self.request, policy_path.read_bytes(), **arguments
                    )

    @unittest.skipUnless(
        os.environ.get("ONEBRAIN_REGISTRY_FAILURE_QUALIFICATION")
        and os.environ.get("ONEBRAIN_REGISTRY_RELEASE_OPS"),
        "set compiled failure and release-operation bridges",
    )
    def test_compiled_failure_harness_accepts_only_measured_signed_release_path(self) -> None:
        input_path = self.root / "concepts.jsonl"
        input_path.write_text(
            json.dumps({
                "source": SOURCE_WIKIDATA,
                "ext_id": 283,
                "category": 7,
                "name": "water",
                "canonical_form": "wd:Q283",
                "labels": {"en": "water"},
            }) + "\n",
            encoding="utf-8",
        )
        obr = self.root / "concepts.obr"
        build(input_path, obr)
        manifest = json.loads(Path(f"{obr}.manifest.json").read_text(encoding="utf-8"))
        sbom = self.root / "sbom.spdx.json"
        sbom.write_text(json.dumps({"spdxVersion": "SPDX-2.3", "dataLicense": "CC0-1.0", "packages": []}), encoding="utf-8")
        sources_path = self.root / "sources.json"
        sources = []
        for index, name in enumerate(("chebi", "geonames", "ncbi", "wikidata", "wordnet"), start=1):
            source = manifest["sources"][name]
            sources.append({
                "name": name,
                "snapshot_id": source["snapshot_id"],
                "source_uri": source["source_uri"],
                "license": source["license"],
                "snapshot_blake3": blake3.blake3(f"snapshot-{index}".encode()).hexdigest(),
                "download_blake3": blake3.blake3(f"download-{index}".encode()).hexdigest(),
            })
        sources_path.write_text(json.dumps(sources), encoding="utf-8")
        receipt_key = Ed25519PrivateKey.from_private_bytes(bytes([19]) * 32)
        private_key = self.root / "receipt.key"
        private_key.write_text(bytes([19] * 32).hex(), encoding="utf-8")
        receipt_public = receipt_key.public_key().public_bytes_raw()
        receipt_policy = {
            "algorithm": "Ed25519",
            "allowed_usages": ["registry-release-stamp", "registry-qualification-receipt"],
            "format": "onebrain/concept-registry-trust-policy/1",
            "signers": [{
                "fingerprint_algorithm": "blake3-derive-key-v1",
                "fingerprint_context": "onebrain:concept-registry:signer-fingerprint:1",
                "fingerprint_hex": signer_fingerprint(receipt_public),
                "public_key_hex": receipt_public.hex(),
            }],
        }
        receipt_policy_path = self.root / "registry-policy.json"
        receipt_policy_path.write_text(json.dumps(receipt_policy), encoding="utf-8")
        registry_root = self.root / "installed-registry"
        release_id = "qualification-candidate"
        bridge = os.environ["ONEBRAIN_REGISTRY_RELEASE_OPS"]
        for arguments in (
            ["package", registry_root, obr, sbom, sources_path, release_id, private_key],
            ["activate", registry_root, release_id, receipt_public.hex()],
        ):
            result = subprocess.run(
                [bridge, *map(str, arguments)], capture_output=True, text=True,
                encoding="utf-8", errors="replace", check=False,
            )
            self.assertEqual(result.returncode, 0, result.stderr)
        stamp_path = registry_root / "releases" / release_id / "release.stamp.json"
        stamp = json.loads(stamp_path.read_text(encoding="utf-8"))
        state_path = sorted((registry_root / "state").glob("state-*.json"))[-1]
        state = json.loads(state_path.read_text(encoding="utf-8"))
        semantic = self.root / "semantic.json"
        profile = self.root / "profile.json"
        vector = self.root / "vector.json"
        profile.write_text(json.dumps({"profile": 1}), encoding="utf-8")
        vector.write_text(json.dumps({"vector": 1}), encoding="utf-8")
        idl = self.root / "idl-root.txt"
        idl.write_text("46" * 32, encoding="utf-8")
        repo = SCRIPT_DIR.parents[1]
        commit = subprocess.run(["git", "-C", repo, "rev-parse", "HEAD"], check=True, capture_output=True, text=True).stdout.strip()
        tree = subprocess.run(["git", "-C", repo, "rev-parse", "HEAD^{tree}"], check=True, capture_output=True, text=True).stdout.strip()
        target = "x86_64-pc-windows-msvc"
        artifact_tuple = "62" * 32
        self.request["candidate"] = {"commit": commit, "tree": tree, "object_format": "sha1"}
        self.request["required_targets"] = {target: artifact_tuple}
        self.request["production_profile_blake3"] = blake3.blake3(canonical_json(json.loads(profile.read_text()))).hexdigest()
        self.request["production_vector_blake3"] = blake3.blake3(canonical_json(json.loads(vector.read_text()))).hexdigest()
        self.request["append_only_idl_history_root"] = idl.read_text()
        self.request["registry_candidate"].update({
            "candidate_semantic_digest": self.request["registry_candidate"]["candidate_semantic_digest"],
            "artifact_tuple_digest": artifact_tuple,
            "release_aggregate_root": stamp["artifact_root"],
            "registry_generation": state["generation"],
            "payload_artifacts_blake3": {
                f"{item['role']}:{item['relative_path']}": item["blake3"]
                for item in stamp["artifacts"]
            },
            "release_stamp_blake3": blake3.blake3(stamp_path.read_bytes()).hexdigest(),
            "registry_trust_policy_digest": trust_policy_digest(receipt_policy),
            "registry_signer_fingerprint": signer_fingerprint(receipt_public),
        })
        failure_executable = Path(os.environ["ONEBRAIN_REGISTRY_FAILURE_QUALIFICATION"])
        executable_digest = blake3.blake3(failure_executable.read_bytes()).hexdigest()
        probe_signature = self.root / "failure-probe.sig"
        probe_message = b"onebrain:concept-registry-probe:1\0" + blake3.blake3(failure_executable.read_bytes()).digest()
        probe_signature.write_text(receipt_key.sign(probe_message).hex(), encoding="ascii")
        toolchain = self.root / "failure-toolchain.txt"
        runner = self.root / "failure-runner.txt"
        toolchain.write_text("rustc fixture", encoding="utf-8")
        runner.write_text("runner fixture", encoding="utf-8")
        compatibility_tuple = self._compatibility_tuple(
            commit, target, blake3.blake3(toolchain.read_bytes()).hexdigest()
        )
        semantic.write_bytes(canonical_json(compatibility_tuple))
        semantic_digest = blake3.blake3(
            canonical_compatibility_tuple_bytes(compatibility_tuple, include_artifact_fields=False),
            derive_key_context="onebrain:base:candidate-semantic:1\0",
        ).hexdigest()
        artifact_tuple = blake3.blake3(
            canonical_compatibility_tuple_bytes(compatibility_tuple, include_artifact_fields=True),
            derive_key_context="onebrain:base:artifact-tuple:1\0",
        ).hexdigest()
        self.request["required_targets"] = {target: artifact_tuple}
        self.request["registry_candidate"]["candidate_semantic_digest"] = semantic_digest
        self.request["registry_candidate"]["artifact_tuple_digest"] = artifact_tuple
        self.request["reference_environment"].update({
            "target_triple": target,
            "probe_blake3": executable_digest,
            "executable_blake3": executable_digest,
            "probe_signature": blake3.blake3(probe_signature.read_bytes()).hexdigest(),
            "probe_signer_fingerprint": signer_fingerprint(receipt_public),
            "probe_signer_public_key": receipt_public.hex(),
            "rust_toolchain_digest": blake3.blake3(toolchain.read_bytes()).hexdigest(),
            "runner_image_digest": blake3.blake3(runner.read_bytes()).hexdigest(),
        })
        request_path, signature_path, approver_policy_path = self._signed_paths()
        output = self.root / "failure-receipt.json"
        command = [
            os.environ["ONEBRAIN_REGISTRY_FAILURE_QUALIFICATION"],
            "--test-release-nonproduction",
            str(self.root / "work"), str(obr), str(sbom), str(sources_path), str(private_key),
            str(request_path), str(signature_path), str(approver_policy_path), str(self.gpg_home),
            sys.executable, _gpg(), shutil.which("git") or "git",
            str(registry_root), release_id, str(repo), str(semantic), str(profile), str(vector),
            str(idl), target, str(failure_executable), str(probe_signature), str(toolchain),
            str(runner), str(receipt_policy_path), str(output),
        ]
        result = subprocess.run(command, capture_output=True, text=True, encoding="utf-8", errors="replace", timeout=90, check=False)
        self.assertEqual(result.returncode, 0, result.stderr)
        payload = json.loads(output.read_text(encoding="utf-8"))["payload"]
        self.assertTrue(payload["base_candidate_bound"])
        self.assertFalse(payload["production_qualified"])
        self.assertEqual(payload["release_request_digest"], blake3.blake3(request_path.read_bytes()).hexdigest())

    @unittest.skipUnless(
        os.environ.get("ONEBRAIN_REGISTRY_RELEASE_OPS")
        and os.environ.get("ONEBRAIN_REGISTRY_FAILURE_QUALIFICATION")
        and os.environ.get("ONEBRAIN_REGISTRY_GENERATION_QUALIFICATION"),
        "set compiled release, failure, and generation bridges",
    )
    def test_first_party_nine_step_cycle_inspects_real_state_and_signed_inputs(self) -> None:
        old_input = self.root / "old.jsonl"
        candidate_input = self.root / "candidate.jsonl"
        water = {
            "source": SOURCE_WIKIDATA, "ext_id": 283, "category": 7,
            "name": "water", "canonical_form": "wd:Q283", "labels": {"en": "water"},
        }
        fire = {
            "source": SOURCE_WIKIDATA, "ext_id": 3196, "category": 7,
            "name": "fire", "canonical_form": "wd:Q3196", "labels": {"en": "fire"},
        }
        old_input.write_text(json.dumps(water) + "\n", encoding="utf-8")
        candidate_input.write_text(json.dumps(water) + "\n" + json.dumps(fire) + "\n", encoding="utf-8")
        old_obr = self.root / "old.obr"
        candidate_obr = self.root / "candidate.obr"
        build(old_input, old_obr)
        build(candidate_input, candidate_obr)
        old_manifest = Path(f"{old_obr}.manifest.json")
        candidate_manifest = Path(f"{candidate_obr}.manifest.json")
        old_sbom = self.root / "old.spdx.json"
        candidate_sbom = self.root / "candidate.spdx.json"
        old_sbom.write_text(json.dumps({"spdxVersion": "SPDX-2.3", "dataLicense": "CC0-1.0", "packages": []}), encoding="utf-8")
        candidate_sbom.write_text(json.dumps({"spdxVersion": "SPDX-2.3", "dataLicense": "CC0-1.0", "packages": [{"name": "candidate"}]}), encoding="utf-8")
        manifest = json.loads(candidate_manifest.read_text(encoding="utf-8"))
        sources = []
        for index, name in enumerate(("chebi", "geonames", "ncbi", "wikidata", "wordnet"), start=1):
            source = manifest["sources"][name]
            sources.append({
                "name": name, "snapshot_id": source["snapshot_id"],
                "source_uri": source["source_uri"], "license": source["license"],
                "snapshot_blake3": blake3.blake3(f"snapshot-{index}".encode()).hexdigest(),
                "download_blake3": blake3.blake3(f"download-{index}".encode()).hexdigest(),
            })
        sources_path = self.root / "sources.json"
        sources_path.write_text(json.dumps(sources), encoding="utf-8")
        signing_key = Ed25519PrivateKey.from_private_bytes(bytes([29]) * 32)
        private_key = self.root / "registry.key"
        private_key.write_text(bytes([29] * 32).hex(), encoding="utf-8")
        public = signing_key.public_key().public_bytes_raw()
        receipt_policy = {
            "algorithm": "Ed25519",
            "allowed_usages": ["registry-release-stamp", "registry-qualification-receipt"],
            "format": "onebrain/concept-registry-trust-policy/1",
            "signers": [{
                "fingerprint_algorithm": "blake3-derive-key-v1",
                "fingerprint_context": "onebrain:concept-registry:signer-fingerprint:1",
                "fingerprint_hex": signer_fingerprint(public),
                "public_key_hex": public.hex(),
            }],
        }
        aggregate_profile = {
            "format": "onebrain/concept-registry-production-qualification/1",
            "qualification_receipt_envelope": {
                "format": "onebrain/concept-registry-qualification-receipt/1",
                "usage": "registry-qualification-receipt",
                "closed_receipt_kinds": [
                    "resource-qualification", "failure-qualification", "generation-swap",
                    "ccid-stability", "signed-release-cycle", "production-aggregate",
                ],
            },
            "trust_policy": {
                "digest_hex": trust_policy_digest(receipt_policy),
                "policy": receipt_policy,
            },
        }
        bridge = Path(os.environ["ONEBRAIN_REGISTRY_RELEASE_OPS"])
        measured = self.root / "measured"
        candidate_release = "candidate-v2"
        for arguments in (
            ["package", measured, old_obr, old_sbom, sources_path, "stable-v1", private_key],
            ["activate", measured, "stable-v1", public.hex()],
            ["package", measured, candidate_obr, candidate_sbom, sources_path, candidate_release, private_key],
            ["activate", measured, candidate_release, public.hex()],
            ["rollback", measured, public.hex()],
            ["activate", measured, candidate_release, public.hex()],
        ):
            result = subprocess.run(
                [str(bridge), *map(str, arguments)],
                capture_output=True, text=True, encoding="utf-8", errors="replace", check=False,
            )
            self.assertEqual(result.returncode, 0, result.stderr)
        stamp_path = measured / "releases" / candidate_release / "release.stamp.json"
        candidate_state = sorted((measured / "state").glob("state-*.json"))[-1]
        stamp = json.loads(stamp_path.read_text(encoding="utf-8"))
        target = "x86_64-pc-windows-msvc"
        artifact_tuple = "62" * 32
        repo = SCRIPT_DIR.parents[1]
        self.request["candidate"] = {
            "commit": subprocess.run(["git", "-C", repo, "rev-parse", "HEAD"], check=True, capture_output=True, text=True).stdout.strip(),
            "tree": subprocess.run(["git", "-C", repo, "rev-parse", "HEAD^{tree}"], check=True, capture_output=True, text=True).stdout.strip(),
            "object_format": "sha1",
        }
        profile_path = self.root / "production-profile.json"
        vector_path = self.root / "production-vector.json"
        idl_path = self.root / "idl-root.txt"
        semantic_path = self.root / "semantic.json"
        profile_path.write_bytes(canonical_json(aggregate_profile))
        vector_path.write_bytes(canonical_json({"vector": "small-real-producer-fixture"}))
        idl_path.write_text("46" * 32, encoding="ascii")
        self.request["production_profile_blake3"] = blake3.blake3(profile_path.read_bytes()).hexdigest()
        self.request["production_vector_blake3"] = blake3.blake3(vector_path.read_bytes()).hexdigest()
        self.request["append_only_idl_history_root"] = idl_path.read_text(encoding="ascii")
        self.request["required_targets"] = {target: artifact_tuple}
        self.request["registry_candidate"].update({
            "artifact_tuple_digest": artifact_tuple,
            "release_aggregate_root": stamp["artifact_root"],
            "registry_generation": 4,
            "payload_artifacts_blake3": {
                f"{item['role']}:{item['relative_path']}": item["blake3"] for item in stamp["artifacts"]
            },
            "release_stamp_blake3": blake3.blake3(stamp_path.read_bytes()).hexdigest(),
            "registry_trust_policy_digest": trust_policy_digest(receipt_policy),
            "registry_signer_fingerprint": signer_fingerprint(public),
            "ccid_inputs_blake3": {
                "old_input": blake3.blake3(old_input.read_bytes()).hexdigest(),
                "old_obr": blake3.blake3(old_obr.read_bytes()).hexdigest(),
                "old_manifest": blake3.blake3(old_manifest.read_bytes()).hexdigest(),
                "candidate_input": blake3.blake3(candidate_input.read_bytes()).hexdigest(),
                "candidate_obr": blake3.blake3(candidate_obr.read_bytes()).hexdigest(),
                "candidate_manifest": blake3.blake3(candidate_manifest.read_bytes()).hexdigest(),
            },
        })
        self.request["reference_environment"]["target_triple"] = target
        failure_executable = Path(os.environ["ONEBRAIN_REGISTRY_FAILURE_QUALIFICATION"])
        executable_digest = blake3.blake3(failure_executable.read_bytes()).hexdigest()
        probe_signature_path = self.root / "probe.sig"
        probe_message = b"onebrain:concept-registry-probe:1\0" + blake3.blake3(failure_executable.read_bytes()).digest()
        probe_signature_path.write_text(signing_key.sign(probe_message).hex(), encoding="ascii")
        toolchain_path = self.root / "rust-toolchain.txt"
        runner_path = self.root / "runner-image.txt"
        toolchain_path.write_text("rustc fixture", encoding="utf-8")
        runner_path.write_text("runner fixture", encoding="utf-8")
        compatibility_tuple = self._compatibility_tuple(
            self.request["candidate"]["commit"], target,
            blake3.blake3(toolchain_path.read_bytes()).hexdigest(),
        )
        semantic_path.write_bytes(canonical_json(compatibility_tuple))
        semantic_digest = blake3.blake3(
            canonical_compatibility_tuple_bytes(compatibility_tuple, include_artifact_fields=False),
            derive_key_context="onebrain:base:candidate-semantic:1\0",
        ).hexdigest()
        artifact_tuple = blake3.blake3(
            canonical_compatibility_tuple_bytes(compatibility_tuple, include_artifact_fields=True),
            derive_key_context="onebrain:base:artifact-tuple:1\0",
        ).hexdigest()
        self.request["required_targets"] = {target: artifact_tuple}
        self.request["registry_candidate"]["candidate_semantic_digest"] = semantic_digest
        self.request["registry_candidate"]["artifact_tuple_digest"] = artifact_tuple
        self.request["reference_environment"].update({
            "probe_blake3": executable_digest,
            "executable_blake3": executable_digest,
            "probe_signature": blake3.blake3(probe_signature_path.read_bytes()).hexdigest(),
            "probe_signer_fingerprint": signer_fingerprint(public),
            "probe_signer_public_key": public.hex(),
            "rust_toolchain_digest": blake3.blake3(toolchain_path.read_bytes()).hexdigest(),
            "runner_image_digest": blake3.blake3(runner_path.read_bytes()).hexdigest(),
        })
        tooling_paths: dict[str, Path] = {}
        for name in ("qualifier", "request", "clean_worktree"):
            path = self.root / f"{name}.tool"
            path.write_text(f"{name} fixture", encoding="utf-8")
            tooling_paths[name] = path
            self.request["candidate_tooling_blake3"][name] = blake3.blake3(path.read_bytes()).hexdigest()
        tooling_paths["release_wrapper"] = bridge
        self.request["candidate_tooling_blake3"]["release_wrapper"] = blake3.blake3(bridge.read_bytes()).hexdigest()
        request_path, signature_path, approver_policy_path = self._signed_paths()
        tooling_paths.update({
            "verifier": SCRIPT_DIR / "verify_base_release_request.py",
            "signer_policy": approver_policy_path,
        })
        def run_cycle(
            target_registry: Path,
            old_release_id: str = "stable-v1",
        ) -> dict[str, object]:
            return run_release_cycle_for_test_nonproduction(
                request_path, signature_path, approver_policy_path, self.gpg_home,
                gpg_executable=Path(_gpg()), bridge=bridge,
                candidate_root=repo, git_executable=Path(shutil.which("git") or "git"),
                candidate_semantic_evidence=semantic_path,
                production_profile=profile_path, production_vector=vector_path,
                append_only_idl_history=idl_path, candidate_tooling=tooling_paths,
                probe=failure_executable, probe_signature=probe_signature_path,
                executable=failure_executable, rust_toolchain_evidence=toolchain_path,
                runner_image_evidence=runner_path, target_triple=target,
                candidate_release_stamp=stamp_path, candidate_state=candidate_state,
                registry_root=target_registry,
                old_input=old_input, old_obr=old_obr, old_manifest=old_manifest, old_sbom=old_sbom,
                candidate_input=candidate_input, candidate_obr=candidate_obr,
                candidate_manifest=candidate_manifest, candidate_sbom=candidate_sbom,
                sources=sources_path, old_release_id=old_release_id, candidate_release_id=candidate_release,
                query_label="water", release_private_key=private_key,
                release_public_key=public.hex(), signing_key=signing_key, receipt_policy=receipt_policy,
            )

        semantic_original = semantic_path.read_bytes()
        semantic_mutation = json.loads(semantic_original)
        semantic_mutation["feature_set_digest"] = "ff" * 32
        staged_sbom = stamp_path.parent / "sbom.spdx.json"
        staged_sbom_original = staged_sbom.read_bytes()
        state_original = candidate_state.read_bytes()
        state_mutation = json.loads(state_original)
        state_original_value = json.loads(state_original)
        state_mutation["previous_release"] = "unbound-previous-v9"
        state_view = {
            field: state_mutation[field]
            for field in ("profile", "generation", "active_release", "previous_release")
        }
        state_mutation["state_root"] = blake3.blake3(
            b"onebrain:concept-registry-state:1\0"
            + json.dumps(state_view, ensure_ascii=False, separators=(",", ":")).encode("utf-8")
        ).hexdigest()
        state_mutation_bytes = state_original.replace(
            json.dumps(state_original_value["previous_release"]).encode("ascii"),
            json.dumps(state_mutation["previous_release"]).encode("ascii"),
            1,
        ).replace(
            state_original_value["state_root"].encode("ascii"),
            state_mutation["state_root"].encode("ascii"),
            1,
        )
        qualifier_tool = tooling_paths["qualifier"]
        qualifier_original = qualifier_tool.read_bytes()
        mismatch_cases = (
            ("semantic", semantic_path, semantic_original, canonical_json(semantic_mutation)),
            ("payload", staged_sbom, staged_sbom_original, staged_sbom_original + b" "),
            (
                "state",
                candidate_state,
                state_original,
                state_mutation_bytes,
            ),
            ("tool", qualifier_tool, qualifier_original, qualifier_original + b"tamper"),
        )

        def assert_zero_operation_rejection(
            name: str,
            old_release_id: str = "stable-v1",
        ) -> CycleError:
            rejected_registry = self.root / f"preflight-rejected-{name}"
            receipt_before_rejection = None
            caught = None
            with (
                patch.object(
                    release_cycle_module,
                    "_execute_bridge",
                    wraps=release_cycle_module._execute_bridge,
                ) as execute_bridge,
                patch.object(
                    release_cycle_module,
                    "_query_obr",
                    wraps=release_cycle_module._query_obr,
                ) as query_obr,
                patch.object(
                    release_cycle_module,
                    "generate_report",
                    wraps=release_cycle_module.generate_report,
                ) as ccid_diff,
            ):
                try:
                    receipt_before_rejection = run_cycle(rejected_registry, old_release_id)
                except CycleError as error:
                    caught = error
                operation_calls = execute_bridge.call_count
                query_calls = query_obr.call_count
                ccid_calls = ccid_diff.call_count
            self.assertIsNotNone(caught, "candidate mismatch must fail closed")
            self.assertIsNone(receipt_before_rejection)
            self.assertEqual(operation_calls, 0, "candidate mismatch must run zero first-party operations")
            self.assertEqual(query_calls, 0)
            self.assertEqual(ccid_calls, 0)
            self.assertFalse(
                rejected_registry.exists(),
                "candidate mismatch must produce no operation state/output",
            )
            return caught

        for name, path, original, mutation in mismatch_cases:
            with self.subTest(preoperation_candidate_mismatch=name):
                path.write_bytes(mutation)
                try:
                    mismatch_error = assert_zero_operation_rejection(name)
                    if name == "state":
                        self.assertIn("state differs", str(mismatch_error))
                finally:
                    path.write_bytes(original)

        self_state_mutation = json.loads(state_original)
        original_previous_release = self_state_mutation["previous_release"]
        original_state_root = self_state_mutation["state_root"]
        self_state_mutation["previous_release"] = candidate_release
        self_state_view = {
            field: self_state_mutation[field]
            for field in ("profile", "generation", "active_release", "previous_release")
        }
        self_state_mutation["state_root"] = blake3.blake3(
            b"onebrain:concept-registry-state:1\0"
            + json.dumps(
                self_state_view,
                ensure_ascii=False,
                separators=(",", ":"),
            ).encode("utf-8")
        ).hexdigest()
        candidate_state.write_bytes(
            state_original.replace(
                json.dumps(original_previous_release).encode("ascii"),
                json.dumps(candidate_release).encode("ascii"),
                1,
            ).replace(
                original_state_root.encode("ascii"),
                self_state_mutation["state_root"].encode("ascii"),
                1,
            )
        )
        try:
            with self.subTest(preoperation_candidate_mismatch="same-release-identity"):
                identity_error = assert_zero_operation_rejection(
                    "same-release-identity", candidate_release
                )
                self.assertIn("distinct", str(identity_error))
        finally:
            candidate_state.write_bytes(state_original)

        stamp_original = stamp_path.read_bytes()
        bad_stamp = json.loads(stamp_original)
        original_source_root = bad_stamp["source_root"]
        original_stamp_signature = bad_stamp["signature"]
        bad_stamp["source_root"] = "00" * 32
        unsigned_bad_stamp = dict(bad_stamp)
        unsigned_bad_stamp["signature"] = ""
        bad_stamp["signature"] = signing_key.sign(
            b"onebrain:concept-registry-release-stamp:1\0"
            + blake3.blake3(
                json.dumps(
                    unsigned_bad_stamp,
                    ensure_ascii=False,
                    separators=(",", ":"),
                ).encode("utf-8")
            ).digest()
        ).hex()
        stamp_path.write_bytes(
            stamp_original.replace(
                original_source_root.encode("ascii"),
                bad_stamp["source_root"].encode("ascii"),
                1,
            ).replace(
                original_stamp_signature.encode("ascii"),
                bad_stamp["signature"].encode("ascii"),
                1,
            )
        )
        self.request["registry_candidate"]["release_stamp_blake3"] = blake3.blake3(
            stamp_path.read_bytes()
        ).hexdigest()
        request_path, signature_path, approver_policy_path = self._signed_paths()
        try:
            with self.subTest(preoperation_candidate_mismatch="resigned-source-root"):
                source_root_error = assert_zero_operation_rejection("resigned-source-root")
                self.assertIn("source root", str(source_root_error))
        finally:
            stamp_path.write_bytes(stamp_original)
            self.request["registry_candidate"]["release_stamp_blake3"] = blake3.blake3(
                stamp_original
            ).hexdigest()
            request_path, signature_path, approver_policy_path = self._signed_paths()

        frozen_registry = self.root / "frozen-candidate-provenance"
        first_operation = True
        real_execute_bridge = release_cycle_module._execute_bridge

        def mutate_staged_locators_after_preflight(
            operation_bridge: Path,
            operation: str,
            arguments: list[str],
        ) -> list[str]:
            nonlocal first_operation
            if first_operation:
                first_operation = False
                stamp_path.write_bytes(stamp_original + b"post-preflight")
                candidate_state.write_bytes(state_original + b"post-preflight")
            return real_execute_bridge(operation_bridge, operation, arguments)

        try:
            with patch.object(
                release_cycle_module,
                "_execute_bridge",
                side_effect=mutate_staged_locators_after_preflight,
            ):
                frozen_receipt = run_cycle(frozen_registry)
        finally:
            stamp_path.write_bytes(stamp_original)
            candidate_state.write_bytes(state_original)
        frozen_command = frozen_receipt["payload"]["command"]
        self.assertIn(
            f"--candidate-release-stamp={stamp_path.name}@blake3:"
            f"{blake3.blake3(stamp_original).hexdigest()}",
            frozen_command,
        )
        self.assertIn(
            f"--candidate-state={candidate_state.name}@blake3:"
            f"{blake3.blake3(state_original).hexdigest()}",
            frozen_command,
        )

        cycle_registry = self.root / "cycle-registry"
        receipt = run_cycle(cycle_registry)
        payload = receipt["payload"]
        self.assertEqual([step["step"] for step in payload["steps"]], list(REQUIRED_STEPS))
        self.assertTrue(all(payload["exit_oracles"].values()))
        self.assertEqual(payload["registry_generation"], 4)
        self.assertEqual(payload["command_blake3"], blake3.blake3(canonical_json(payload["command"])).hexdigest())
        self.assertNotIn(str(private_key), json.dumps(payload["command"], sort_keys=True))
        bridge_identity = f"{bridge.name}@blake3:{blake3.blake3(bridge.read_bytes()).hexdigest()}"
        file_identity = lambda path: f"{path.name}@blake3:{blake3.blake3(path.read_bytes()).hexdigest()}"
        expected_steps = {
            "package": ["package", bridge_identity, "package", str(cycle_registry), file_identity(old_obr), file_identity(old_sbom), file_identity(sources_path), "stable-v1", "<external-private-key-redacted>"],
            "verify": ["verify", bridge_identity, "verify", str(cycle_registry), "stable-v1", public.hex()],
            "activate": ["activate", bridge_identity, "activate", str(cycle_registry), "stable-v1", public.hex()],
            "query": ["query", "internal-obr-query", f"--obr={file_identity(old_obr)}", "--label=water"],
            "build-new-signed-generation": ["build-new-signed-generation", bridge_identity, "package", str(cycle_registry), file_identity(candidate_obr), file_identity(candidate_sbom), file_identity(sources_path), candidate_release, "<external-private-key-redacted>"],
            "ccid-diff": [
                "ccid_stability_diff.py",
                f"--old-input={file_identity(old_input)}", f"--old-obr={file_identity(old_obr)}",
                f"--old-manifest={file_identity(old_manifest)}", f"--candidate-input={file_identity(candidate_input)}",
                f"--candidate-obr={file_identity(candidate_obr)}", f"--candidate-manifest={file_identity(candidate_manifest)}",
            ],
            "activate-new": ["activate-new", bridge_identity, "activate", str(cycle_registry), candidate_release, public.hex()],
            "rollback": ["rollback", bridge_identity, "rollback", str(cycle_registry), public.hex()],
            "reactivate-new": ["reactivate-new", bridge_identity, "activate", str(cycle_registry), candidate_release, public.hex()],
        }
        self.assertEqual(payload["step_commands"], expected_steps)
        self.assertEqual(
            payload["step_command_blake3"],
            {name: blake3.blake3(canonical_json(command)).hexdigest() for name, command in expected_steps.items()},
        )
        expected_step_digests = {
            name: blake3.blake3(canonical_json(command)).hexdigest()
            for name, command in expected_steps.items()
        }
        expected_cycle_invocation = [
            "release_cycle_qualification.py",
            f"--release-request-digest={blake3.blake3(request_path.read_bytes()).hexdigest()}",
            f"--bridge-blake3={blake3.blake3(bridge.read_bytes()).hexdigest()}",
            f"--old-input-blake3={blake3.blake3(old_input.read_bytes()).hexdigest()}",
            f"--candidate-input-blake3={blake3.blake3(candidate_input.read_bytes()).hexdigest()}",
            f"--old-obr={file_identity(old_obr)}", f"--old-manifest={file_identity(old_manifest)}",
            f"--old-sbom={file_identity(old_sbom)}", f"--candidate-obr={file_identity(candidate_obr)}",
            f"--candidate-manifest={file_identity(candidate_manifest)}", f"--candidate-sbom={file_identity(candidate_sbom)}",
            f"--sources={file_identity(sources_path)}", "--old-release-id=stable-v1",
            f"--candidate-release-id={candidate_release}", "--query-label=water", f"--target-triple={target}",
            f"--semantic-tuple={file_identity(semantic_path)}", f"--production-profile={file_identity(profile_path)}",
            f"--production-vector={file_identity(vector_path)}", f"--idl-history={file_identity(idl_path)}",
            f"--probe={file_identity(failure_executable)}", f"--probe-signature={file_identity(probe_signature_path)}",
            f"--executable={file_identity(failure_executable)}", f"--rust-toolchain={file_identity(toolchain_path)}",
            f"--runner-image={file_identity(runner_path)}",
            f"--candidate-release-stamp={file_identity(stamp_path)}",
            f"--candidate-state={file_identity(candidate_state)}",
            *[f"--candidate-tool-{name}={file_identity(path)}" for name, path in sorted(tooling_paths.items())],
            *[f"--step-{name}-blake3={expected_step_digests[name]}" for name in REQUIRED_STEPS],
            "--release-private-key=<external-redacted>", "--gpg-home=<redacted>",
            "--receipt-signer=<external-redacted>",
        ]
        self.assertEqual(payload["command"], expected_cycle_invocation)
        self.assertEqual(payload["command_blake3"], blake3.blake3(canonical_json(expected_cycle_invocation)).hexdigest())

        verified = verify_release_request_for_test_nonproduction(
            request_path, signature_path, approver_policy_path, self.gpg_home,
            gpg_executable=Path(_gpg()),
        )
        installed_release = cycle_registry / "releases" / candidate_release
        payload_artifacts = {
            name: installed_release / name.split(":", 1)[1]
            for name in self.request["registry_candidate"]["payload_artifacts_blake3"]
        }
        resource_receipts = []
        labels_file = self.root / "qualification-labels.txt"
        labels_file.write_text("water\nfire\n", encoding="utf-8")
        for resource_profile in ("cold-cache", "low-ram", "ssd", "hdd"):
            resource_receipts.append(create_verified_resource_receipt_for_test_nonproduction(
                {
                    "qualification_profile": resource_profile,
                    "budget_profile": "ci-small-fixture-v1",
                    "cache_strategy_requested": "auto",
                    "limits": {"timeout_seconds": 60},
                    "qualified": True,
                    "exit_oracles": {"small_fixture_producer_completed": True},
                },
                verified,
                git_executable=Path(shutil.which("git") or "git"),
                labels_file=labels_file, cache_strategy="auto",
                budget_profile="ci-small-fixture-v1", timeout_seconds=60,
                candidate_root=repo,
                registry_root=cycle_registry,
                release_id=candidate_release,
                candidate_semantic_evidence=semantic_path,
                production_profile=profile_path,
                production_vector=vector_path,
                append_only_idl_history=idl_path,
                candidate_tooling=tooling_paths,
                payload_artifacts=payload_artifacts,
                release_stamp=installed_release / "release.stamp.json",
                probe=failure_executable,
                probe_signature=probe_signature_path,
                executable=failure_executable,
                rust_toolchain_evidence=toolchain_path,
                runner_image_evidence=runner_path,
                target_triple=target,
                signing_key=signing_key,
                policy=receipt_policy,
            ))
        for resource_profile, resource_receipt in zip(("cold-cache", "low-ram", "ssd", "hdd"), resource_receipts):
            expected_invocation = [
                "resource_qualification.py",
                f"--profile={resource_profile}",
                f"--labels-file={labels_file.name}@blake3:{blake3.blake3(labels_file.read_bytes()).hexdigest()}",
                "--cache-strategy=auto",
                "--budget-profile=ci-small-fixture-v1",
                "--timeout-seconds=60",
                f"--release-request-digest={verified.request_digest}",
                f"--candidate-tree={verified.run_context['candidate_tree']}",
                f"--release-id={candidate_release}",
                *[
                    f"--payload-{name}={path.name}@blake3:{blake3.blake3(path.read_bytes()).hexdigest()}"
                    for name, path in sorted(payload_artifacts.items())
                ],
                f"--release-stamp=release.stamp.json@blake3:{blake3.blake3((installed_release / 'release.stamp.json').read_bytes()).hexdigest()}",
                f"--probe={failure_executable.name}@blake3:{blake3.blake3(failure_executable.read_bytes()).hexdigest()}",
                f"--probe-signature={probe_signature_path.name}@blake3:{blake3.blake3(probe_signature_path.read_bytes()).hexdigest()}",
                f"--executable={failure_executable.name}@blake3:{blake3.blake3(failure_executable.read_bytes()).hexdigest()}",
                f"--production-profile={profile_path.name}@blake3:{blake3.blake3(profile_path.read_bytes()).hexdigest()}",
                f"--production-vector={vector_path.name}@blake3:{blake3.blake3(vector_path.read_bytes()).hexdigest()}",
                f"--idl-history={idl_path.name}@blake3:{blake3.blake3(idl_path.read_bytes()).hexdigest()}",
                f"--rust-toolchain={toolchain_path.name}@blake3:{blake3.blake3(toolchain_path.read_bytes()).hexdigest()}",
                f"--runner-image={runner_path.name}@blake3:{blake3.blake3(runner_path.read_bytes()).hexdigest()}",
                *[
                    f"--candidate-tool-{name}={path.name}@blake3:{blake3.blake3(path.read_bytes()).hexdigest()}"
                    for name, path in sorted(tooling_paths.items())
                ],
                f"--target-triple={target}",
                "--gpg-home=<redacted>",
                "--receipt-signer=<external-redacted>",
            ]
            self.assertEqual(resource_receipt["payload"]["command"], expected_invocation)
            self.assertEqual(
                resource_receipt["payload"]["command_blake3"],
                blake3.blake3(canonical_json(expected_invocation)).hexdigest(),
            )
            self.assertNotEqual(
                resource_receipt["payload"]["command_blake3"],
                blake3.blake3(canonical_json(expected_invocation[:-1])).hexdigest(),
            )
        ccid_receipt = qualify_ccid_stability_from_signed_request_for_test_nonproduction(
            request_path, signature_path, approver_policy_path, self.gpg_home,
            old_input, old_obr, old_manifest, candidate_input, candidate_obr, candidate_manifest,
            sample_limit=20, work_dir=self.root / "ccid-work",
            signing_key=signing_key, receipt_policy=receipt_policy,
            gpg_executable=Path(_gpg()),
        )
        failure_output = self.root / "failure-real-receipt.json"
        failure_command = [
            os.environ["ONEBRAIN_REGISTRY_FAILURE_QUALIFICATION"], "--test-release-nonproduction",
            str(self.root / "failure-work"), str(candidate_obr), str(candidate_sbom), str(sources_path), str(private_key),
            str(request_path), str(signature_path), str(approver_policy_path), str(self.gpg_home),
            sys.executable, _gpg(), shutil.which("git") or "git",
            str(cycle_registry), candidate_release, str(repo), str(semantic_path), str(profile_path),
            str(vector_path), str(idl_path), target, str(failure_executable), str(probe_signature_path),
            str(toolchain_path), str(runner_path), str(self.root / "receipt-policy.json"), str(failure_output),
        ]
        (self.root / "receipt-policy.json").write_bytes(canonical_json(receipt_policy))
        runner_path.write_text("tampered runner", encoding="utf-8")
        rejected_environment = subprocess.run(
            failure_command, capture_output=True, text=True, encoding="utf-8", errors="replace", timeout=90, check=False,
        )
        self.assertNotEqual(rejected_environment.returncode, 0)
        self.assertIn("reference environment", rejected_environment.stderr)
        runner_path.write_text("runner fixture", encoding="utf-8")

        tuple_original = semantic_path.read_bytes()
        tuple_with_unknown_field = json.loads(tuple_original)
        tuple_with_unknown_field["unknown_compatibility_field"] = "known-fields-unchanged"
        semantic_path.write_bytes(canonical_json(tuple_with_unknown_field))
        rejected_failure_work = self.root / "unknown-tuple-failure-work"
        rejected_failure_output = self.root / "unknown-tuple-failure-receipt.json"
        rejected_failure_command = list(failure_command)
        rejected_failure_command[2] = str(rejected_failure_work)
        rejected_failure_command[-1] = str(rejected_failure_output)
        rejected_failure = subprocess.run(
            rejected_failure_command,
            capture_output=True, text=True, encoding="utf-8", errors="replace",
            timeout=90, check=False,
        )
        self.assertNotEqual(rejected_failure.returncode, 0)
        self.assertIn("fields are not closed", rejected_failure.stderr)
        self.assertFalse(rejected_failure_output.exists())
        self.assertFalse(
            rejected_failure_work.exists(),
            "unknown tuple field must be rejected before failure work-root creation",
        )
        semantic_path.write_bytes(tuple_original)

        failure_result = subprocess.run(
            failure_command, capture_output=True, text=True, encoding="utf-8", errors="replace", timeout=90, check=False,
        )
        self.assertEqual(failure_result.returncode, 0, failure_result.stderr)
        failure_receipt = json.loads(failure_output.read_text(encoding="utf-8"))

        generation_registry = self.root / "generation-registry"
        for arguments in (
            ["package", generation_registry, old_obr, old_sbom, sources_path, "stable-v1", private_key],
            ["activate", generation_registry, "stable-v1", public.hex()],
            ["package", generation_registry, candidate_obr, candidate_sbom, sources_path, candidate_release, private_key],
        ):
            result = subprocess.run([str(bridge), *map(str, arguments)], capture_output=True, text=True, encoding="utf-8", errors="replace", check=False)
            self.assertEqual(result.returncode, 0, result.stderr)
        generation_output = self.root / "generation-real-receipt.json"
        generation_command = [
            os.environ["ONEBRAIN_REGISTRY_GENERATION_QUALIFICATION"], "--test-release-nonproduction",
            str(generation_registry), public.hex(), "stable-v1", candidate_release, "water",
            str(request_path), str(signature_path), str(approver_policy_path), str(self.gpg_home),
            sys.executable, _gpg(), shutil.which("git") or "git",
            str(repo), str(semantic_path), str(profile_path), str(vector_path), str(idl_path),
            str(failure_executable), str(probe_signature_path), str(toolchain_path), str(runner_path), target,
            str(self.root / "receipt-policy.json"), str(private_key), str(generation_output),
        ]

        semantic_path.write_bytes(canonical_json(tuple_with_unknown_field))
        generation_before = {
            path.relative_to(generation_registry).as_posix(): path.read_bytes()
            for path in generation_registry.rglob("*")
            if path.is_file()
        }
        rejected_generation_output = self.root / "unknown-tuple-generation-receipt.json"
        rejected_generation_command = list(generation_command)
        rejected_generation_command[-1] = str(rejected_generation_output)
        rejected_generation = subprocess.run(
            rejected_generation_command,
            capture_output=True, text=True, encoding="utf-8", errors="replace",
            timeout=90, check=False,
        )
        self.assertNotEqual(rejected_generation.returncode, 0)
        self.assertIn("fields are not closed", rejected_generation.stderr)
        self.assertFalse(rejected_generation_output.exists())
        self.assertEqual(
            {
                path.relative_to(generation_registry).as_posix(): path.read_bytes()
                for path in generation_registry.rglob("*")
                if path.is_file()
            },
            generation_before,
            "unknown tuple field must be rejected before generation swap state mutation",
        )
        semantic_path.write_bytes(tuple_original)

        generation_result = subprocess.run(
            generation_command, capture_output=True, text=True, encoding="utf-8", errors="replace", timeout=90, check=False,
        )
        self.assertEqual(generation_result.returncode, 0, generation_result.stderr)
        generation_receipt = json.loads(generation_output.read_text(encoding="utf-8"))

        components = [*resource_receipts, failure_receipt, generation_receipt, ccid_receipt, receipt]
        for component in components:
            emitted = component["payload"]
            self.assertEqual(
                emitted["command_blake3"],
                blake3.blake3(canonical_json(emitted["command"])).hexdigest(),
            )
            self.assertNotEqual(
                emitted["command_blake3"],
                blake3.blake3(canonical_json([*emitted["command"], "--omitted"])).hexdigest(),
            )
            self.assertNotIn(str(private_key), json.dumps(emitted["command"], sort_keys=True))
            serialized = json.dumps(emitted["command"], sort_keys=True)
            self.assertIn(verified.request_digest, serialized)
            if component["receipt_kind"] != "ccid-stability":
                self.assertIn(target, serialized)
        aggregate = _aggregate_reports_for_test_nonproduction(
            components,
            verified.run_context, aggregate_profile, signing_key,
        )
        self.assertFalse(aggregate["payload"]["registry_production_qualified"])
        self.assertEqual(
            {item["receipt_kind"] for item in components},
            {"resource-qualification", "failure-qualification", "generation-swap", "ccid-stability", "signed-release-cycle"},
        )
        semantic_original = semantic_path.read_bytes()
        semantic_mutation = json.loads(semantic_original)
        semantic_mutation["feature_set_digest"] = "ff" * 32
        semantic_path.write_bytes(canonical_json(semantic_mutation))
        with self.assertRaisesRegex(QualificationError, "semantic digest"):
            create_verified_resource_receipt_for_test_nonproduction(
                {"qualification_profile": "cold-cache", "budget_profile": "ci-small-fixture-v1", "cache_strategy_requested": "auto", "limits": {"timeout_seconds": 60}, "qualified": True, "exit_oracles": {"completed": True}},
                verified, git_executable=Path(shutil.which("git") or "git"), labels_file=labels_file,
                cache_strategy="auto", budget_profile="ci-small-fixture-v1", timeout_seconds=60,
                candidate_root=repo, registry_root=cycle_registry, release_id=candidate_release,
                candidate_semantic_evidence=semantic_path, production_profile=profile_path,
                production_vector=vector_path, append_only_idl_history=idl_path,
                candidate_tooling=tooling_paths, payload_artifacts=payload_artifacts,
                release_stamp=installed_release / "release.stamp.json", probe=failure_executable,
                probe_signature=probe_signature_path, executable=failure_executable,
                rust_toolchain_evidence=toolchain_path, runner_image_evidence=runner_path,
                target_triple=target, signing_key=signing_key, policy=receipt_policy,
            )
        artifact_mutation = json.loads(semantic_original)
        artifact_mutation["target_triple"] = "aarch64-apple-darwin"
        semantic_path.write_bytes(canonical_json(artifact_mutation))
        with self.assertRaisesRegex(QualificationError, "artifact tuple target/toolchain"):
            create_verified_resource_receipt_for_test_nonproduction(
                {"qualification_profile": "cold-cache", "budget_profile": "ci-small-fixture-v1", "cache_strategy_requested": "auto", "limits": {"timeout_seconds": 60}, "qualified": True, "exit_oracles": {"completed": True}},
                verified, git_executable=Path(shutil.which("git") or "git"), labels_file=labels_file,
                cache_strategy="auto", budget_profile="ci-small-fixture-v1", timeout_seconds=60,
                candidate_root=repo, registry_root=cycle_registry, release_id=candidate_release,
                candidate_semantic_evidence=semantic_path, production_profile=profile_path,
                production_vector=vector_path, append_only_idl_history=idl_path,
                candidate_tooling=tooling_paths, payload_artifacts=payload_artifacts,
                release_stamp=installed_release / "release.stamp.json", probe=failure_executable,
                probe_signature=probe_signature_path, executable=failure_executable,
                rust_toolchain_evidence=toolchain_path, runner_image_evidence=runner_path,
                target_triple=target, signing_key=signing_key, policy=receipt_policy,
            )
        semantic_path.write_bytes(semantic_original)
        installed_obr = installed_release / "concepts.obr"
        installed_obr_original = installed_obr.read_bytes()
        installed_obr.write_bytes(installed_obr_original + b"tamper")
        with self.assertRaisesRegex(CycleError, "payload bytes differ"):
            _stamp(cycle_registry, candidate_release)
        installed_obr.write_bytes(installed_obr_original)
        state_path = sorted((cycle_registry / "state").glob("state-*.json"))[-1]
        state_original = state_path.read_bytes()
        state_mutation = json.loads(state_original)
        state_mutation["active_release"] = "stable-v1"
        state_path.write_bytes(canonical_json(state_mutation))
        with self.assertRaisesRegex(CycleError, "state root"):
            _latest_state(cycle_registry)
        state_path.write_bytes(state_original)
        candidate_input.write_text(candidate_input.read_text(encoding="utf-8") + json.dumps({**fire, "ext_id": 999}) + "\n", encoding="utf-8")
        with self.assertRaisesRegex(CycleError, "signed request"):
            run_release_cycle_for_test_nonproduction(
                request_path, signature_path, approver_policy_path, self.gpg_home,
                gpg_executable=Path(_gpg()), bridge=bridge,
                candidate_root=repo, git_executable=Path(shutil.which("git") or "git"),
                candidate_semantic_evidence=semantic_path,
                production_profile=profile_path, production_vector=vector_path,
                append_only_idl_history=idl_path, candidate_tooling=tooling_paths,
                probe=failure_executable, probe_signature=probe_signature_path,
                executable=failure_executable, rust_toolchain_evidence=toolchain_path,
                runner_image_evidence=runner_path, target_triple=target,
                candidate_release_stamp=stamp_path, candidate_state=candidate_state,
                registry_root=self.root / "tampered-cycle",
                old_input=old_input, old_obr=old_obr, old_manifest=old_manifest, old_sbom=old_sbom,
                candidate_input=candidate_input, candidate_obr=candidate_obr,
                candidate_manifest=candidate_manifest, candidate_sbom=candidate_sbom,
                sources=sources_path, old_release_id="stable-v1", candidate_release_id=candidate_release,
                query_label="water", release_private_key=private_key,
                release_public_key=public.hex(), signing_key=signing_key, receipt_policy=receipt_policy,
            )

    def test_generation_producer_does_not_accept_caller_verifier_or_python(self) -> None:
        source = (
            SCRIPT_DIR.parents[1]
            / "src" / "onebrain-node" / "examples"
            / "concept_registry_production_qualification.rs"
        ).read_text(encoding="utf-8")
        self.assertNotIn("REQUEST_VERIFIER", source)
        self.assertNotIn('std::env::var("PYTHON")', source)

    def test_resource_production_cli_has_no_caller_selected_gpg(self) -> None:
        source = (CONCEPT_DIR / "resource_qualification.py").read_text(encoding="utf-8")
        self.assertNotIn('parser.add_argument("--gpg"', source)
        self.assertNotIn("gpg_executable=args.gpg", source)

    def test_candidate_measurement_api_requires_git_state_release_state_and_semantic_evidence(self) -> None:
        parameters = inspect.signature(verify_registry_candidate_measurements).parameters
        for field in (
            "candidate_root", "registry_root", "release_id",
            "candidate_semantic_evidence", "production_profile",
            "production_vector", "append_only_idl_history",
        ):
            with self.subTest(field=field):
                self.assertIn(field, parameters)

    def test_failure_producer_has_no_raw_release_context_or_binding_arguments(self) -> None:
        source = (
            SCRIPT_DIR.parents[1] / "src" / "ku-core" / "examples"
            / "concept_registry_failure_qualification.rs"
        ).read_text(encoding="utf-8")
        self.assertNotIn("RUN_CONTEXT_JSON", source)
        self.assertNotIn("RELEASE_BINDING_JSON", source)

    def test_all_release_producers_bind_full_sanitized_command_provenance(self) -> None:
        sources = {
            "resource": (SCRIPT_DIR.parent / "concept_registry" / "resource_qualification.py").read_text(encoding="utf-8"),
            "ccid": (SCRIPT_DIR.parent / "concept_registry" / "ccid_stability_qualification.py").read_text(encoding="utf-8"),
            "generation": (SCRIPT_DIR.parents[1] / "src" / "onebrain-node" / "examples" / "concept_registry_production_qualification.rs").read_text(encoding="utf-8"),
            "failure": (SCRIPT_DIR.parents[1] / "src" / "ku-core" / "examples" / "concept_registry_failure_qualification.rs").read_text(encoding="utf-8"),
            "cycle": (SCRIPT_DIR.parent / "concept_registry" / "release_cycle_qualification.py").read_text(encoding="utf-8"),
        }
        for name, source in sources.items():
            with self.subTest(producer=name):
                self.assertIn("command_blake3", source)
                self.assertIn("release_request_digest", source)
                self.assertNotIn('"generation-swap"\n        ]', source)
                self.assertNotIn('"truncated-index", "disk-shortage"', source)

    def test_python_producers_do_not_accept_caller_command_provenance(self) -> None:
        for producer in (
            create_verified_resource_receipt_for_test_nonproduction,
            qualify_ccid_stability_from_signed_request_for_test_nonproduction,
        ):
            with self.subTest(producer=producer.__name__):
                self.assertNotIn("invocation", inspect.signature(producer).parameters)

    def test_resource_receipt_api_requires_every_behavior_affecting_option(self) -> None:
        parameters = inspect.signature(
            create_verified_resource_receipt_for_test_nonproduction
        ).parameters
        for name in ("labels_file", "cache_strategy", "budget_profile", "timeout_seconds"):
            with self.subTest(name=name):
                self.assertIn(name, parameters)

    def test_valid_signature_yields_closed_context_derived_from_request(self) -> None:
        request_path, signature_path, policy_path = self._signed_paths()
        verified = verify_release_request_for_test_nonproduction(
            request_path,
            signature_path,
            policy_path,
            self.gpg_home,
            gpg_executable=Path(_gpg()),
        )
        self.assertFalse(verified.production)
        self.assertEqual(verified.run_context["candidate_commit"], self.request["candidate"]["commit"])
        self.assertEqual(verified.bindings["release_request_digest"], blake3.blake3(request_path.read_bytes()).hexdigest())
        self.assertEqual(verified.bindings["candidate_payload_artifacts_blake3"], self.request["registry_candidate"]["payload_artifacts_blake3"])

    def test_tamper_unlisted_expired_and_unknown_field_fail_closed(self) -> None:
        request_path, signature_path, policy_path = self._signed_paths()
        request_path.write_bytes(canonical_json({**self.request, "qualification_session_id": "99" * 32}))
        with self.assertRaisesRegex(ReleaseRequestError, "signature"):
            verify_release_request_for_test_nonproduction(request_path, signature_path, policy_path, self.gpg_home, gpg_executable=Path(_gpg()))

        unlisted = json.loads(policy_path.read_text())
        unlisted["signers"][0]["fingerprint"] = "A" * 40
        unlisted["signers"][0]["key_id"] = "A" * 16
        policy_path.write_bytes(canonical_json(unlisted))
        unlisted_request = json.loads(json.dumps(self.request))
        unlisted_request["qualification_approver_fingerprint"] = "A" * 40
        unlisted_request["trust_policy_digest"] = blake3.blake3(
            canonical_json(unlisted), derive_key_context=APPROVER_POLICY_DIGEST_CONTEXT
        ).hexdigest()
        unlisted_request["candidate_tooling_blake3"]["signer_policy"] = blake3.blake3(
            canonical_json(unlisted)
        ).hexdigest()
        request_path.write_bytes(canonical_json(unlisted_request))
        signature_path.unlink()
        _run_gpg(self.gpg_home, "--local-user", self.fingerprint, "--detach-sign", "--output", str(signature_path), str(request_path))
        with self.assertRaisesRegex(ReleaseRequestError, "allowlist"):
            verify_release_request_for_test_nonproduction(request_path, signature_path, policy_path, self.gpg_home, gpg_executable=Path(_gpg()))

        request_path, signature_path, policy_path = self._signed_paths()
        expired = dict(self.request)
        expired["expires_utc"] = "2020-01-01T00:00:00Z"
        request_path.write_bytes(canonical_json(expired))
        signature_path.unlink()
        _run_gpg(self.gpg_home, "--local-user", self.fingerprint, "--detach-sign", "--output", str(signature_path), str(request_path))
        with self.assertRaisesRegex(ReleaseRequestError, "validity|expired"):
            verify_release_request_for_test_nonproduction(request_path, signature_path, policy_path, self.gpg_home, gpg_executable=Path(_gpg()))

        request_path, signature_path, policy_path = self._signed_paths()
        extended = {**self.request, "qualified": True}
        request_path.write_bytes(canonical_json(extended))
        signature_path.unlink()
        _run_gpg(self.gpg_home, "--local-user", self.fingerprint, "--detach-sign", "--output", str(signature_path), str(request_path))
        with self.assertRaisesRegex(ReleaseRequestError, "closed"):
            verify_release_request_for_test_nonproduction(request_path, signature_path, policy_path, self.gpg_home, gpg_executable=Path(_gpg()))

    def test_ccid_producer_verifies_signed_request_before_exact_input_diff(self) -> None:
        from build_obr import build
        from ccid_stability_qualification import (
            qualify_ccid_stability_from_signed_request_for_test_nonproduction,
        )
        from config import SOURCE_WIKIDATA
        from production_qualification import signer_fingerprint, trust_policy_digest

        receipt_key = Ed25519PrivateKey.from_private_bytes(bytes([49]) * 32)
        receipt_public = receipt_key.public_key().public_bytes_raw()
        receipt_policy = {
            "algorithm": "Ed25519",
            "allowed_usages": ["registry-qualification-receipt"],
            "format": "onebrain/concept-registry-trust-policy/1",
            "signers": [{
                "fingerprint_algorithm": "blake3-derive-key-v1",
                "fingerprint_context": "onebrain:concept-registry:signer-fingerprint:1",
                "fingerprint_hex": signer_fingerprint(receipt_public),
                "public_key_hex": receipt_public.hex(),
            }],
        }
        triples = []
        for name in ("old", "candidate"):
            directory = self.root / name
            merged = directory / "merged"
            merged.mkdir(parents=True)
            input_path = merged / "concepts_deduped.jsonl"
            input_path.write_text(json.dumps({
                "source": SOURCE_WIKIDATA,
                "ext_id": 42,
                "category": 7,
                "name": "Douglas Adams",
                "canonical_form": "wd:42",
                "labels": {"en": "Douglas Adams"},
            }) + "\n", encoding="utf-8")
            obr_path = directory / "concepts.obr"
            build(input_path, obr_path)
            triples.append((input_path, obr_path, Path(f"{obr_path}.manifest.json")))
        names = ("old_input", "old_obr", "old_manifest", "candidate_input", "candidate_obr", "candidate_manifest")
        measured = {
            name: blake3.blake3(path.read_bytes()).hexdigest()
            for name, path in zip(names, (*triples[0], *triples[1]), strict=True)
        }
        self.request["registry_candidate"]["ccid_inputs_blake3"] = measured
        self.request["registry_candidate"]["payload_artifacts_blake3"]["OBR:concepts.obr"] = measured["candidate_obr"]
        self.request["registry_candidate"]["payload_artifacts_blake3"]["MANIFEST:concepts.obr.manifest.json"] = measured["candidate_manifest"]
        self.request["registry_candidate"]["registry_trust_policy_digest"] = trust_policy_digest(receipt_policy)
        self.request["registry_candidate"]["registry_signer_fingerprint"] = signer_fingerprint(receipt_public)
        verifier_path = SCRIPT_DIR / "verify_base_release_request.py"
        self.request["candidate_tooling_blake3"]["verifier"] = blake3.blake3(verifier_path.read_bytes()).hexdigest()
        request_path, signature_path, policy_path = self._signed_paths()
        receipt = qualify_ccid_stability_from_signed_request_for_test_nonproduction(
            request_path, signature_path, policy_path, self.gpg_home,
            *triples[0], *triples[1],
            sample_limit=10, work_dir=self.root / "ccid-work",
            signing_key=receipt_key, receipt_policy=receipt_policy,
            gpg_executable=Path(_gpg()),
        )
        self.assertTrue(receipt["payload"]["result"])
        triples[1][0].write_text(triples[1][0].read_text() + "\n", encoding="utf-8")
        with self.assertRaisesRegex(RuntimeError, "do not match"):
            qualify_ccid_stability_from_signed_request_for_test_nonproduction(
                request_path, signature_path, policy_path, self.gpg_home,
                *triples[0], *triples[1],
                sample_limit=10, work_dir=self.root / "ccid-work",
                signing_key=receipt_key, receipt_policy=receipt_policy,
                gpg_executable=Path(_gpg()),
            )


if __name__ == "__main__":
    unittest.main()

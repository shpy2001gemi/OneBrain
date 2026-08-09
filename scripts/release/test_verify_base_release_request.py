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
    verify_release_request,
    verify_release_request_for_test_nonproduction,
    verify_registry_candidate_measurements,
)
from build_obr import build  # noqa: E402
from config import SOURCE_WIKIDATA  # noqa: E402
from production_qualification import signer_fingerprint, trust_policy_digest  # noqa: E402
from release_cycle_qualification import (  # noqa: E402
    REQUIRED_STEPS,
    CycleError,
    run_release_cycle_for_test_nonproduction,
)
from ccid_stability_qualification import (  # noqa: E402
    qualify_ccid_stability_from_signed_request_for_test_nonproduction,
)
from resource_qualification import (  # noqa: E402
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
            },
        }

    def tearDown(self) -> None:
        self.temporary.cleanup()

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
        semantic = self.root / "semantic.txt"
        semantic.write_text("61" * 32, encoding="utf-8")
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
            "candidate_semantic_digest": semantic.read_text(),
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
        result = subprocess.run(
            [str(bridge), "package", str(measured), str(candidate_obr), str(candidate_sbom), str(sources_path), candidate_release, str(private_key)],
            capture_output=True, text=True, encoding="utf-8", errors="replace", check=False,
        )
        self.assertEqual(result.returncode, 0, result.stderr)
        stamp_path = measured / "releases" / candidate_release / "release.stamp.json"
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
        semantic_path = self.root / "semantic.txt"
        profile_path.write_bytes(canonical_json(aggregate_profile))
        vector_path.write_bytes(canonical_json({"vector": "small-real-producer-fixture"}))
        idl_path.write_text("46" * 32, encoding="ascii")
        semantic_path.write_text(self.request["registry_candidate"]["candidate_semantic_digest"], encoding="ascii")
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
        for name in ("qualifier", "request", "clean_worktree", "release_wrapper"):
            path = self.root / f"{name}.tool"
            path.write_text(f"{name} fixture", encoding="utf-8")
            tooling_paths[name] = path
            self.request["candidate_tooling_blake3"][name] = blake3.blake3(path.read_bytes()).hexdigest()
        request_path, signature_path, approver_policy_path = self._signed_paths()
        tooling_paths.update({
            "verifier": SCRIPT_DIR / "verify_base_release_request.py",
            "signer_policy": approver_policy_path,
        })
        cycle_registry = self.root / "cycle-registry"
        receipt = run_release_cycle_for_test_nonproduction(
            request_path, signature_path, approver_policy_path, self.gpg_home,
            gpg_executable=Path(_gpg()), bridge=bridge,
            registry_root=cycle_registry,
            old_input=old_input, old_obr=old_obr, old_manifest=old_manifest, old_sbom=old_sbom,
            candidate_input=candidate_input, candidate_obr=candidate_obr,
            candidate_manifest=candidate_manifest, candidate_sbom=candidate_sbom,
            sources=sources_path, old_release_id="stable-v1", candidate_release_id=candidate_release,
            query_label="water", release_private_key=private_key,
            release_public_key=public.hex(), signing_key=signing_key, receipt_policy=receipt_policy,
        )
        payload = receipt["payload"]
        self.assertEqual([step["step"] for step in payload["steps"]], list(REQUIRED_STEPS))
        self.assertTrue(all(payload["exit_oracles"].values()))
        self.assertEqual(payload["registry_generation"], 4)
        self.assertEqual(payload["command_blake3"], blake3.blake3(canonical_json(payload["command"])).hexdigest())
        self.assertNotIn(str(private_key), json.dumps(payload["command"], sort_keys=True))

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
        for resource_profile in ("cold-cache", "low-ram", "ssd", "hdd"):
            resource_receipts.append(create_verified_resource_receipt_for_test_nonproduction(
                {
                    "qualification_profile": resource_profile,
                    "qualified": True,
                    "exit_oracles": {"small_fixture_producer_completed": True},
                },
                verified,
                git_executable=Path(shutil.which("git") or "git"),
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
        candidate_input.write_text(candidate_input.read_text(encoding="utf-8") + json.dumps({**fire, "ext_id": 999}) + "\n", encoding="utf-8")
        with self.assertRaisesRegex(CycleError, "signed request"):
            run_release_cycle_for_test_nonproduction(
                request_path, signature_path, approver_policy_path, self.gpg_home,
                gpg_executable=Path(_gpg()), bridge=bridge,
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

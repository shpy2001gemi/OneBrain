"""Tests for the complete signed Concept Registry release-cycle harness."""

from __future__ import annotations

import json
import sys
import tempfile
import unittest
from pathlib import Path

from cryptography.hazmat.primitives.asymmetric.ed25519 import Ed25519PrivateKey

SCRIPT_DIR = Path(__file__).resolve().parent
if str(SCRIPT_DIR) not in sys.path:
    sys.path.insert(0, str(SCRIPT_DIR))

from production_qualification import signer_fingerprint, trust_policy_digest
from release_cycle_qualification import CycleError, REQUIRED_STEPS, run_release_cycle


class ReleaseCycleQualificationTests(unittest.TestCase):
    def setUp(self) -> None:
        self.key = Ed25519PrivateKey.from_private_bytes(bytes([41]) * 32)
        public = self.key.public_key().public_bytes_raw()
        self.policy = {
            "algorithm": "Ed25519",
            "allowed_usages": [
                "registry-release-stamp",
                "registry-qualification-receipt",
            ],
            "format": "onebrain/concept-registry-trust-policy/1",
            "signers": [
                {
                    "fingerprint_algorithm": "blake3-derive-key-v1",
                    "fingerprint_context": "onebrain:concept-registry:signer-fingerprint:1",
                    "fingerprint_hex": signer_fingerprint(public),
                    "public_key_hex": public.hex(),
                }
            ],
        }
        self.old_root = "10" * 32
        self.new_root = "20" * 32
        self.context = {
            "format": "onebrain/qualification-run-context/1",
            "variant": "Release",
            "release_request_digest": "30" * 32,
            "qualification_session_id": "cycle-session",
            "candidate_commit": "40" * 20,
            "candidate_tree": "50" * 20,
        }
        self.binding = {
            "candidate_semantic_digest": "60" * 32,
            "artifact_tuple_digest": "70" * 32,
            "release_aggregate_root": self.new_root,
            "registry_generation": 4,
            "production_profile_blake3": "80" * 32,
            "trust_policy_digest": trust_policy_digest(self.policy),
            "signer_fingerprint": signer_fingerprint(public),
            "probe_blake3": "90" * 32,
            "executable_blake3": "a0" * 32,
            "candidate_payload_artifacts_blake3": {
                "OBR:concepts.obr": "a1" * 32,
                "LABEL_INDEX:concepts.obr.labels.idx": "a2" * 32,
                "CCID_INDEX:concepts.obr.ccids.idx": "a3" * 32,
                "MANIFEST:concepts.obr.manifest.json": "a4" * 32,
                "SPDX_SBOM:sbom.spdx.json": "a5" * 32,
            },
            "release_stamp_blake3": "b0" * 32,
        }

    def _helper(self, root: Path) -> Path:
        helper = root / "step.py"
        helper.write_text(
            "import json,sys\n"
            "print(json.dumps({'step':sys.argv[1], 'result':sys.argv[2]=='true', "
            "'observed_release_root':sys.argv[3], 'registry_generation':int(sys.argv[4])}))\n",
            encoding="utf-8",
        )
        return helper

    def _plan(self, root: Path) -> dict[str, object]:
        helper = self._helper(root)
        generations = {
            "package": 0,
            "verify": 0,
            "activate": 1,
            "query": 1,
            "build-new-signed-generation": 1,
            "ccid-diff": 1,
            "activate-new": 2,
            "rollback": 3,
            "reactivate-new": 4,
        }
        old_steps = {"package", "verify", "activate", "query", "rollback"}
        return {
            "previous_release_aggregate_root": self.old_root,
            "steps": [
                {
                    "name": name,
                    "command": [
                        sys.executable,
                        str(helper),
                        name,
                        "true",
                        self.old_root if name in old_steps else self.new_root,
                        str(generations[name]),
                    ],
                }
                for name in REQUIRED_STEPS
            ],
        }

    def test_complete_cycle_executes_every_step_and_signs_exact_final_root(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            receipt = run_release_cycle(
                self._plan(Path(directory)), self.context, self.binding, self.key, self.policy
            )
        payload = receipt["payload"]
        self.assertEqual(receipt["receipt_kind"], "signed-release-cycle")
        self.assertTrue(payload["result"])
        self.assertEqual(payload["release_aggregate_root"], self.new_root)
        self.assertEqual(payload["registry_generation"], 4)
        self.assertEqual([step["step"] for step in payload["steps"]], list(REQUIRED_STEPS))
        self.assertTrue(all(payload["exit_oracles"].values()))

    def test_missing_duplicate_false_and_quarterly_update_substitute_fail_closed(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            plans = []
            missing = self._plan(root)
            missing["steps"].pop()
            plans.append((missing, "exactly once"))
            duplicate = self._plan(root)
            duplicate["steps"][-1] = duplicate["steps"][-2]
            plans.append((duplicate, "exactly once"))
            false = self._plan(root)
            false["steps"][5]["command"][3] = "false"
            plans.append((false, "ccid-diff"))
            substitute = self._plan(root)
            substitute["steps"][0]["command"][1] = "quarterly_update.py"
            plans.append((substitute, "quarterly_update.py"))
            for plan, message in plans:
                with self.subTest(message=message):
                    with self.assertRaisesRegex(CycleError, message):
                        run_release_cycle(plan, self.context, self.binding, self.key, self.policy)


if __name__ == "__main__":
    unittest.main()

"""Tests for the pure signed Registry production aggregate verifier."""

from __future__ import annotations

import copy
import json
import sys
import unittest
from pathlib import Path

from cryptography.hazmat.primitives.asymmetric.ed25519 import Ed25519PrivateKey

SCRIPT_DIR = Path(__file__).resolve().parent
if str(SCRIPT_DIR) not in sys.path:
    sys.path.insert(0, str(SCRIPT_DIR))

from production_qualification import (
    AggregationError,
    _aggregate_reports_for_test_nonproduction,
    aggregate_reports,
    canonical_json,
    create_signed_receipt,
    signer_fingerprint,
    trust_policy_digest,
)


ROOT = "11" * 32
PROFILE_DIGEST = "22" * 32
REQUEST = "33" * 32
SESSION = "session-2026-08-09"
COMMIT = "44" * 20
TREE = "55" * 20


class ProductionQualificationTests(unittest.TestCase):
    def setUp(self) -> None:
        self.key = Ed25519PrivateKey.from_private_bytes(bytes([37]) * 32)
        public = self.key.public_key().public_bytes_raw().hex()
        fingerprint = signer_fingerprint(bytes.fromhex(public))
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
                    "fingerprint_hex": fingerprint,
                    "public_key_hex": public,
                }
            ],
        }
        self.policy_digest = trust_policy_digest(self.policy)
        self.profile = {
            "format": "onebrain/concept-registry-production-qualification/1",
            "qualification_receipt_envelope": {
                "format": "onebrain/concept-registry-qualification-receipt/1",
                "usage": "registry-qualification-receipt",
                "closed_receipt_kinds": [
                    "resource-qualification",
                    "failure-qualification",
                    "generation-swap",
                    "ccid-stability",
                    "signed-release-cycle",
                    "production-aggregate",
                ],
            },
            "trust_policy": {
                "digest_hex": self.policy_digest,
                "policy": self.policy,
            },
        }
        self.profile_digest = __import__("blake3").blake3(
            canonical_json(self.profile)
        ).hexdigest()
        self.context = {
            "format": "onebrain/qualification-run-context/1",
            "variant": "Release",
            "release_request_digest": REQUEST,
            "qualification_session_id": SESSION,
            "candidate_commit": COMMIT,
            "candidate_tree": TREE,
        }

    def payload(self, kind: str, resource_profile: str | None = None) -> dict[str, object]:
        payload: dict[str, object] = {
            "qualification_context_variant": "Release",
            "release_request_digest": REQUEST,
            "qualification_session_id": SESSION,
            "candidate_commit": COMMIT,
            "candidate_tree": TREE,
            "candidate_semantic_digest": "66" * 32,
            "artifact_tuple_digest": "77" * 32,
            "base_candidate_bound": True,
            "release_aggregate_root": ROOT,
            "registry_generation": 9,
            "production_profile_blake3": self.profile_digest,
            "trust_policy_digest": self.policy_digest,
            "signer_fingerprint": self.policy["signers"][0]["fingerprint_hex"],
            "probe_blake3": "88" * 32,
            "executable_blake3": "99" * 32,
            "candidate_payload_artifacts_blake3": {
                "CCID_INDEX:concepts.obr.ccids.idx": "a1" * 32,
                "LABEL_INDEX:concepts.obr.labels.idx": "a2" * 32,
                "MANIFEST:concepts.obr.manifest.json": "a3" * 32,
                "OBR:concepts.obr": "a4" * 32,
                "SPDX_SBOM:sbom.spdx.json": "a5" * 32,
            },
            "release_stamp_blake3": "aa" * 32,
            "command": [kind, "--release"],
            "result": True,
            "exit_oracles": {"completed": True, "exact_root": True},
            "limitations": ["Registry-only subgate; never BASE-GATE-V1"],
        }
        if resource_profile is not None:
            payload["qualification_profile"] = resource_profile
        return payload

    def receipt(self, kind: str, resource_profile: str | None = None) -> dict[str, object]:
        return create_signed_receipt(
            kind,
            self.payload(kind, resource_profile),
            self.key,
            self.policy,
        )

    def reports(self) -> list[dict[str, object]]:
        return [
            *(self.receipt("resource-qualification", value) for value in ("cold-cache", "low-ram", "ssd", "hdd")),
            self.receipt("failure-qualification"),
            self.receipt("generation-swap"),
            self.receipt("ccid-stability"),
            self.receipt("signed-release-cycle"),
        ]

    def aggregate(self, reports: list[dict[str, object]] | None = None) -> dict[str, object]:
        return _aggregate_reports_for_test_nonproduction(
            self.reports() if reports is None else reports,
            self.context,
            self.profile,
            self.key,
        )

    def resign(self, receipt: dict[str, object]) -> dict[str, object]:
        return create_signed_receipt(
            str(receipt["receipt_kind"]),
            receipt["payload"],
            self.key,
            self.policy,
        )

    def test_nonproduction_helper_verifies_components_but_cannot_claim_subgate(self) -> None:
        receipt = self.aggregate()
        self.assertFalse(receipt["payload"]["registry_production_qualified"])
        self.assertFalse(receipt["payload"]["base_gate_v1"])
        self.assertEqual(receipt["payload"]["release_aggregate_root"], ROOT)
        self.assertEqual(receipt["receipt_kind"], "production-aggregate")

    def test_public_production_aggregator_rejects_ephemeral_profile_and_signer(self) -> None:
        with self.assertRaisesRegex(AggregationError, "frozen"):
            aggregate_reports(self.reports(), self.context, self.profile, self.key)

    def test_report_profile_root_false_result_duplicate_and_fixture_fail_closed(self) -> None:
        mutations = []
        profile = self.reports()
        profile[0]["payload"]["production_profile_blake3"] = PROFILE_DIGEST
        profile[0] = self.resign(profile[0])
        mutations.append((profile, "profile"))
        root = self.reports()
        root[1]["payload"]["release_aggregate_root"] = "00" * 32
        root[1] = self.resign(root[1])
        mutations.append((root, "release_aggregate_root"))
        false = self.reports()
        false[2]["payload"]["result"] = False
        false[2] = self.resign(false[2])
        mutations.append((false, "result"))
        duplicate = self.reports()
        duplicate[-1] = copy.deepcopy(duplicate[-2])
        mutations.append((duplicate, "duplicate"))
        fixture = self.reports()
        fixture[3]["payload"]["qualification_context_variant"] = "Fixture"
        fixture[3] = self.resign(fixture[3])
        mutations.append((fixture, "context"))
        for reports, message in mutations:
            with self.subTest(message=message):
                with self.assertRaisesRegex(AggregationError, message):
                    self.aggregate(reports)

    def test_prequalification_missing_wrong_and_mixed_request_or_session_fail_closed(self) -> None:
        prequalification = {
            "format": "onebrain/qualification-run-context/1",
            "variant": "Prequalification",
            "closure_digest": "ab" * 32,
        }
        with self.assertRaisesRegex(AggregationError, "Release"):
            _aggregate_reports_for_test_nonproduction(self.reports(), prequalification, self.profile, self.key)

        for field, value in (
            ("release_request_digest", None),
            ("release_request_digest", "00" * 32),
            ("qualification_session_id", "wrong-session"),
        ):
            reports = self.reports()
            if value is None:
                del reports[0]["payload"][field]
            else:
                reports[0]["payload"][field] = value
            reports[0] = self.resign(reports[0])
            with self.subTest(field=field, value=value):
                with self.assertRaisesRegex(AggregationError, field):
                    self.aggregate(reports)

    def test_tampering_after_signature_is_rejected(self) -> None:
        reports = self.reports()
        reports[0]["payload"]["registry_generation"] = 10
        with self.assertRaisesRegex(AggregationError, "signature"):
            self.aggregate(reports)


if __name__ == "__main__":
    unittest.main()

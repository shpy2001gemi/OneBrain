#!/usr/bin/env python3
"""Mutation tests for the pure Base v1 evidence qualifier."""

from __future__ import annotations

import base64
import copy
import hashlib
import json
import os
import subprocess
import sys
import tempfile
import unittest
from pathlib import Path
from unittest.mock import patch

import blake3
from cryptography.hazmat.primitives.asymmetric.ed25519 import Ed25519PrivateKey

from scripts.base.qualify_base import (
    BaseQualificationError,
    QualificationInputs,
    _atomic_create_or_exact,
    canonical_json,
    qualify_base,
    qualify_base_for_test_nonproduction,
)
from scripts.release.verify_base_release_request import canonical_compatibility_tuple_bytes


ROOT = Path(__file__).resolve().parents[2]
TARGETS = {
    "linux": "x86_64-unknown-linux-gnu",
    "windows": "x86_64-pc-windows-msvc",
    "macos": "aarch64-apple-darwin",
}
BUILDER_IDS = {
    "x86_64-unknown-linux-gnu": "https://onebrain.dev/builders/base-v1/linux-release-runner/v1",
    "x86_64-pc-windows-msvc": "https://onebrain.dev/builders/base-v1/windows-release-runner/v1",
    "aarch64-apple-darwin": "https://onebrain.dev/builders/base-v1/macos-release-runner/v1",
}
GATES = (
    "contract-validators",
    "canonical-and-negative-vectors",
    "three-os-build-matrix",
    "blob-and-derived-index-integrity",
    "archive-recovery-and-kill-windows",
    "authoritative-transaction-boundaries",
    "cross-language-and-n-minus-one-conformance",
    "fresh-production-registry",
    "fresh-multi-host-p5",
    "fresh-exact-candidate-72h-soak",
    "dependency-security-and-sbom",
    "product-default-and-release-documents",
)
SIGNED_GATES = {
    "fresh-production-registry": "registry-production-aggregator",
    "fresh-multi-host-p5": "p5-orchestrator",
    "fresh-exact-candidate-72h-soak": "soak-aggregator",
}
SIGNATURE_DOMAIN = b"onebrain:base-v1:child-evidence-reference:1\0"
EVIDENCE_APPROVAL_DOMAIN = b"onebrain:base-v1:evidence-receipt-approval:1\0"
EVIDENCE_APPROVER_FINGERPRINT_CONTEXT = (
    "onebrain:base-v1:evidence-approver-fingerprint:1"
)
EVIDENCE_APPROVER_POLICY_CONTEXT = "onebrain:base-v1:evidence-approver-policy:1"


def digest_bytes(value: bytes) -> str:
    return blake3.blake3(value).hexdigest()


def tuple_digest(value: dict[str, object], *, artifact: bool) -> str:
    context = (
        "onebrain:base:artifact-tuple:1\0"
        if artifact
        else "onebrain:base:candidate-semantic:1\0"
    )
    return blake3.blake3(
        canonical_compatibility_tuple_bytes(value, include_artifact_fields=artifact),
        derive_key_context=context,
    ).hexdigest()


class BaseQualifierTests(unittest.TestCase):
    def setUp(self) -> None:
        self.profile = json.loads(
            (ROOT / "src/test-vectors/vnext/base-v1-freeze-v1.json").read_text(
                encoding="utf-8"
            )
        )
        self.request = "11" * 32
        self.request_created = "2026-08-10T00:00:00Z"
        self.request_expires = "2026-08-18T00:00:00Z"
        self.session = "22" * 32
        self.commit = "3" * 40
        self.tree = "4" * 40
        self.raw: dict[str, bytes] = {}
        self.private_keys: dict[str, Ed25519PrivateKey] = {}
        child_policies: dict[str, dict[str, str]] = {}
        for index, (gate, role) in enumerate(SIGNED_GATES.items(), start=1):
            key = Ed25519PrivateKey.generate()
            public = key.public_key().public_bytes_raw().hex()
            fingerprint = blake3.blake3(
                bytes.fromhex(public),
                derive_key_context="onebrain:base-v1:test-child-signer:1",
            ).hexdigest()
            policy = f"{index + 80:02x}" * 32
            self.private_keys[role] = key
            child_policies[gate] = {
                "role": role,
                "public_key_hex": public,
                "fingerprint_context": "onebrain:base-v1:test-child-signer:1",
                "fingerprint_hex": fingerprint,
                "trust_policy_digest": policy,
            }
        self.profile["child_evidence_policies"] = child_policies
        self.evidence_approval_key = Ed25519PrivateKey.generate()
        evidence_public = self.evidence_approval_key.public_key().public_bytes_raw().hex()
        evidence_fingerprint = blake3.blake3(
            bytes.fromhex(evidence_public),
            derive_key_context=EVIDENCE_APPROVER_FINGERPRINT_CONTEXT,
        ).hexdigest()
        evidence_policy = {
            "algorithm": "Ed25519",
            "allowed_usages": [
                "gate-receipt-approval",
                "target-receipt-approval",
            ],
            "format": "onebrain/base-v1-evidence-approver-policy/1",
            "role": "base-evidence-approver",
            "signature_domain": "onebrain:base-v1:evidence-receipt-approval:1",
            "signers": [
                {
                    "created_utc": "2026-08-09T00:00:00Z",
                    "expires_utc": "2026-08-19T00:00:00Z",
                    "fingerprint_context": EVIDENCE_APPROVER_FINGERPRINT_CONTEXT,
                    "fingerprint_hex": evidence_fingerprint,
                    "public_key_hex": evidence_public,
                }
            ],
            "valid_unlisted_signature": "reject",
        }
        self.profile["base_evidence_approver_policy"] = {
            "status": "test-only-ephemeral-approved",
            "trust_policy_context": EVIDENCE_APPROVER_POLICY_CONTEXT,
            "trust_policy_digest": blake3.blake3(
                canonical_json(evidence_policy),
                derive_key_context=EVIDENCE_APPROVER_POLICY_CONTEXT,
            ).hexdigest(),
            "policy": evidence_policy,
        }
        base_tuple = {
            "base_version": {"major": 1, "minor": 0, "patch": 0, "prerelease": None},
            "base_commit": {"kind": "sha1", "hex": self.commit},
            "canonical_schema_digest": "31" * 32,
            "domain_registry_digest": "32" * 32,
            "resource_registry_digest": "33" * 32,
            "storage_schema": 1,
            "archive_profile": {"major": 2, "minor": 0},
            "migration_profile": {"major": 1, "minor": 0},
            "registry_profile": {"major": 1, "minor": 0},
            "registry_profile_digest": "34" * 32,
            "wire_session": {"major": 1, "minor": 0},
            "product_api": {"major": 1, "minor": 1},
            "c_abi": {"major": 1, "minor": 0},
            "feature_set_digest": "35" * 32,
            "target_triple": TARGETS["linux"],
            "toolchain": {"kind": "known", "hex": "36" * 32},
        }
        self.semantic = tuple_digest(base_tuple, artifact=False)
        tuples: dict[str, dict[str, object]] = {}
        artifacts: dict[str, str] = {}
        receipts: list[dict[str, object]] = []
        for index, (os_name, target) in enumerate(TARGETS.items(), start=1):
            value = copy.deepcopy(base_tuple)
            value["target_triple"] = target
            value["toolchain"] = {"kind": "known", "hex": f"{index + 60:02x}" * 32}
            artifact = tuple_digest(value, artifact=True)
            tuples[target] = value
            artifacts[target] = artifact
            binary_id = f"target-{os_name}-binary"
            binary = f"{binary_id}-bytes".encode()
            self.raw[binary_id] = binary
            binary_digest = digest_bytes(binary)
            binary_sha1 = hashlib.sha1(binary).hexdigest()
            binary_sha256 = hashlib.sha256(binary).hexdigest()
            package_verification_code = hashlib.sha1(
                binary_sha1.encode("ascii")
            ).hexdigest()
            contract = self.profile["target_check_contracts"][target][0]
            sbom_id = f"target-{os_name}-sbom"
            sbom = canonical_json({
                "spdxVersion": "SPDX-2.3",
                "SPDXID": "SPDXRef-DOCUMENT",
                "dataLicense": "CC0-1.0",
                "name": f"OneBrain Base v1 {target}",
                "documentNamespace": f"https://onebrain.dev/spdx/base-v1/{os_name}",
                "creationInfo": {
                    "created": "2026-08-11T00:00:00Z",
                    "creators": ["Tool: onebrain-base-v1-sbom-generator"],
                    "licenseListVersion": "3.26",
                },
                "documentDescribes": ["SPDXRef-Package-OneBrainBase"],
                "packages": [{
                    "SPDXID": "SPDXRef-Package-OneBrainBase",
                    "name": "onebrain-base",
                    "versionInfo": "1.0.0",
                    "downloadLocation": "NOASSERTION",
                    "filesAnalyzed": True,
                    "packageVerificationCode": {
                        "packageVerificationCodeValue": package_verification_code,
                    },
                    "licenseConcluded": "NOASSERTION",
                    "licenseDeclared": "NOASSERTION",
                    "copyrightText": "NOASSERTION",
                }],
                "files": [{
                    "SPDXID": "SPDXRef-BaseBinary",
                    "fileName": f"onebrain-base-{target}",
                    "checksums": [
                        {"algorithm": "SHA1", "checksumValue": binary_sha1},
                        {"algorithm": "SHA256", "checksumValue": binary_sha256},
                    ],
                    "licenseConcluded": "NOASSERTION",
                    "copyrightText": "NOASSERTION",
                }],
                "relationships": [
                    {
                        "spdxElementId": "SPDXRef-DOCUMENT",
                        "relationshipType": "DESCRIBES",
                        "relatedSpdxElement": "SPDXRef-Package-OneBrainBase",
                    },
                    {
                        "spdxElementId": "SPDXRef-Package-OneBrainBase",
                        "relationshipType": "CONTAINS",
                        "relatedSpdxElement": "SPDXRef-BaseBinary",
                    },
                ],
                "annotations": [
                    {"annotationType": "OTHER", "annotator": "Tool: onebrain-base-v1-qualifier", "annotationDate": "2026-08-11T00:00:01Z", "comment": f"onebrain:target-triple:{target}"},
                    {"annotationType": "OTHER", "annotator": "Tool: onebrain-base-v1-qualifier", "annotationDate": "2026-08-11T00:00:01Z", "comment": f"onebrain:artifact-tuple-blake3:{artifact}"},
                ],
            })
            self.raw[sbom_id] = sbom
            provenance_id = f"target-{os_name}-provenance"
            provenance = canonical_json({
                "_type": "https://in-toto.io/Statement/v1",
                "subject": [{"name": f"onebrain-base-{target}", "digest": {"sha256": binary_sha256}}],
                "predicateType": "https://slsa.dev/provenance/v1",
                "predicate": {
                    "buildDefinition": {
                        "buildType": "https://onebrain.dev/base-v1/build/v1",
                        "externalParameters": {
                            "target_triple": target,
                            "artifact_tuple_blake3": artifact,
                            "sbom_blake3": digest_bytes(sbom),
                        },
                        "internalParameters": {
                            "candidate_commit": self.commit,
                            "candidate_tree": self.tree,
                        },
                        "resolvedDependencies": [{
                            "uri": "git+https://onebrain.invalid/OneBrain",
                            "digest": {"gitCommit": self.commit, "gitTree": self.tree},
                        }],
                    },
                    "runDetails": {
                        "builder": {"id": contract["builder_id"]},
                        "metadata": {"invocationId": self.session, "startedOn": "2026-08-11T00:00:00Z", "finishedOn": "2026-08-11T00:00:01Z"},
                    },
                },
            })
            self.raw[provenance_id] = provenance
            stdout_id = f"target-{os_name}-job-stdout"
            stderr_id = f"target-{os_name}-job-stderr"
            assertions = []
            for assertion_id in contract["required_assertion_ids"]:
                assertion_evidence_id = f"target-{os_name}-assertion-{assertion_id}"
                self.raw[assertion_evidence_id] = f"measured:{target}:{assertion_id}".encode()
                assertions.append({
                    "id": assertion_id,
                    "passed": True,
                    "evidence_id": assertion_evidence_id,
                    "evidence_blake3": digest_bytes(self.raw[assertion_evidence_id]),
                })
            assertions.sort(key=lambda row: row["id"])
            target_bindings = {
                "release_request_digest": self.request,
                "qualification_session_id": self.session,
                "candidate_commit": self.commit,
                "candidate_tree": self.tree,
                "candidate_semantic_digest": self.semantic,
                "artifact_tuple_digests": artifacts,
                "registry_root": "51" * 32,
                "p5_root": "52" * 32,
                "soak_root": "53" * 32,
            }
            self.raw[stdout_id] = canonical_json({
                "format": "onebrain/base-v1-check-result/1",
                "check": contract["name"],
                "status": "passed",
                "bindings": target_bindings,
                "assertions": assertions,
                "assertion_root": digest_bytes(canonical_json(assertions)),
            })
            self.raw[stderr_id] = b""
            command = contract["command"]
            receipt_id = f"target-{os_name}-receipt"
            receipt_bytes = canonical_json({
                "format": "onebrain/base-v1-target-receipt/1",
                "os": os_name,
                "target_triple": target,
                "bindings": target_bindings,
                "artifact_tuple_digest": artifact,
                "binary": {"evidence_id": binary_id, "blake3": binary_digest},
                "sbom": {"evidence_id": sbom_id, "blake3": digest_bytes(sbom)},
                "provenance": {"evidence_id": provenance_id, "blake3": digest_bytes(provenance)},
                "checks": [{
                    "name": "target-job",
                    "command": command,
                    "command_blake3": digest_bytes(canonical_json(command)),
                    "exit_code": 0,
                    "stdout_evidence_id": stdout_id,
                    "stdout_blake3": digest_bytes(self.raw[stdout_id]),
                    "stderr_evidence_id": stderr_id,
                    "stderr_blake3": digest_bytes(self.raw[stderr_id]),
                    "runner": {
                        "format": "onebrain/base-v1-runner-provenance/1",
                        "kind": contract["runner_kind"],
                        "identity": contract["runner_identity"],
                        "candidate_commit": self.commit,
                        "candidate_tree": self.tree,
                        "command_blake3": digest_bytes(canonical_json(command)),
                        "invocation_id": f"{index + 90:02x}" * 32,
                    },
                }],
            })
            self.raw[receipt_id] = receipt_bytes
            receipts.append(
                {
                    "os": os_name,
                    "target_triple": target,
                    "receipt_evidence_id": receipt_id,
                    "receipt_blake3": digest_bytes(receipt_bytes),
                }
            )

        # All target receipts bind the same completed three-target artifact map.
        for reference in receipts:
            receipt_id = reference["receipt_evidence_id"]
            receipt = json.loads(self.raw[receipt_id])
            receipt["bindings"]["artifact_tuple_digests"] = artifacts
            check = receipt["checks"][0]
            stdout = json.loads(self.raw[check["stdout_evidence_id"]])
            stdout["bindings"]["artifact_tuple_digests"] = artifacts
            self.raw[check["stdout_evidence_id"]] = canonical_json(stdout)
            check["stdout_blake3"] = digest_bytes(self.raw[check["stdout_evidence_id"]])
            self.raw[receipt_id] = canonical_json(receipt)
            reference["receipt_blake3"] = digest_bytes(self.raw[receipt_id])

        roots = {"registry": "51" * 32, "p5": "52" * 32, "soak": "53" * 32}
        gates: list[dict[str, object]] = []
        signatures: list[dict[str, str]] = []
        for gate in GATES:
            evidence_id = f"gate-{gate}"
            details: dict[str, object] = {}
            if gate == "dependency-security-and-sbom":
                details["security_lanes"] = ["cargo-audit", "cargo-deny", "npm-audit"]
            stdout_id = f"gate-{gate}-stdout"
            stderr_id = f"gate-{gate}-stderr"
            contract = self.profile["gate_check_contracts"][gate][0]
            assertions = []
            for assertion_id in contract["required_assertion_ids"]:
                assertion_evidence_id = f"gate-{gate}-assertion-{assertion_id}"
                self.raw[assertion_evidence_id] = f"measured:{gate}:{assertion_id}".encode()
                assertions.append({
                    "id": assertion_id,
                    "passed": True,
                    "evidence_id": assertion_evidence_id,
                    "evidence_blake3": digest_bytes(self.raw[assertion_evidence_id]),
                })
            assertions.sort(key=lambda row: row["id"])
            gate_bindings = {
                "release_request_digest": self.request,
                "qualification_session_id": self.session,
                "candidate_commit": self.commit,
                "candidate_tree": self.tree,
                "candidate_semantic_digest": self.semantic,
                "artifact_tuple_digests": artifacts,
                "registry_root": roots["registry"],
                "p5_root": roots["p5"],
                "soak_root": roots["soak"],
            }
            self.raw[stdout_id] = canonical_json({
                "format": "onebrain/base-v1-check-result/1",
                "check": contract["name"],
                "status": "passed",
                "bindings": gate_bindings,
                "assertions": assertions,
                "assertion_root": digest_bytes(canonical_json(assertions)),
            })
            self.raw[stderr_id] = b""
            command = contract["command"]
            machine_receipt = {
                "format": "onebrain/base-v1-gate-receipt/1",
                "gate": gate,
                "bindings": gate_bindings,
                "fresh": gate in SIGNED_GATES,
                "carry_forward": False,
                "checks": [{
                    "name": contract["name"],
                    "command": command,
                    "command_blake3": digest_bytes(canonical_json(command)),
                    "exit_code": 0,
                    "stdout_evidence_id": stdout_id,
                    "stdout_blake3": digest_bytes(self.raw[stdout_id]),
                    "stderr_evidence_id": stderr_id,
                    "stderr_blake3": digest_bytes(self.raw[stderr_id]),
                    "runner": {
                        "format": "onebrain/base-v1-runner-provenance/1",
                        "kind": contract["runner_kind"],
                        "identity": contract["runner_identity"],
                        "candidate_commit": self.commit,
                        "candidate_tree": self.tree,
                        "command_blake3": digest_bytes(canonical_json(command)),
                        "invocation_id": digest_bytes(f"invocation:{gate}".encode()),
                    },
                }],
                "details": details,
                "derived_root": roots[{
                    "fresh-production-registry": "registry",
                    "fresh-multi-host-p5": "p5",
                    "fresh-exact-candidate-72h-soak": "soak",
                }[gate]] if gate in SIGNED_GATES else None,
            }
            content = canonical_json(machine_receipt)
            self.raw[evidence_id] = content
            record = {
                "gate": gate,
                "receipt_evidence_id": evidence_id,
                "receipt_blake3": digest_bytes(content),
            }
            gates.append(record)
            if gate in SIGNED_GATES:
                role = SIGNED_GATES[gate]
                policy = child_policies[gate]
                unsigned = {
                    "format": "onebrain/base-v1-child-evidence-reference/1",
                    "gate": gate,
                    "evidence_id": evidence_id,
                    "evidence_blake3": record["receipt_blake3"],
                    "release_request_digest": self.request,
                    "qualification_session_id": self.session,
                    "candidate_commit": self.commit,
                    "candidate_tree": self.tree,
                    "candidate_semantic_digest": self.semantic,
                    "artifact_tuple_digests": artifacts,
                    "registry_root": roots["registry"],
                    "p5_root": roots["p5"],
                    "soak_root": roots["soak"],
                    "role": role,
                    "signer_fingerprint": policy["fingerprint_hex"],
                    "trust_policy_digest": policy["trust_policy_digest"],
                    "fresh": True,
                    "carry_forward": False,
                    "checks_blake3": digest_bytes(canonical_json(machine_receipt["checks"])),
                    "derived_root": machine_receipt["derived_root"],
                }
                signature = self.private_keys[role].sign(
                    SIGNATURE_DOMAIN + blake3.blake3(canonical_json(unsigned)).digest()
                )
                signatures.append(
                    {**unsigned, "signature": base64.b64encode(signature).decode("ascii")}
                )

        documents = {}
        for name in ("migration", "rollback", "changelog"):
            evidence_id = f"document-{name}"
            content = f"{name}-document".encode()
            self.raw[evidence_id] = content
            documents[name] = {
                "evidence_id": evidence_id,
                "blake3": digest_bytes(content),
            }

        evidence_signatures = []
        approval_policy_record = self.profile["base_evidence_approver_policy"]
        approval_policy = approval_policy_record["policy"]
        approval_signer = approval_policy["signers"][0]
        approval_items = [
            ("target", reference["target_triple"], reference)
            for reference in receipts
        ] + [
            ("gate", reference["gate"], reference)
            for reference in gates
            if reference["gate"] not in SIGNED_GATES
        ]
        for kind, identity, reference in approval_items:
            unsigned = {
                "format": "onebrain/base-v1-evidence-receipt-approval/1",
                "kind": kind,
                "identity": identity,
                "receipt_evidence_id": reference["receipt_evidence_id"],
                "receipt_blake3": reference["receipt_blake3"],
                "role": approval_policy["role"],
                "signer_fingerprint": approval_signer["fingerprint_hex"],
                "trust_policy_digest": approval_policy_record["trust_policy_digest"],
            }
            evidence_signatures.append({
                **unsigned,
                "signature": base64.b64encode(
                    self.evidence_approval_key.sign(
                        EVIDENCE_APPROVAL_DOMAIN
                        + blake3.blake3(canonical_json(unsigned)).digest()
                    )
                ).decode("ascii"),
            })
        evidence_signatures.sort(key=lambda row: (row["kind"], row["identity"]))

        self.document = {
            "format": "onebrain/base-v1-qualification-input/1",
            "release_request_digest": self.request,
            "release_request_created_utc": self.request_created,
            "release_request_expires_utc": self.request_expires,
            "qualification_session_id": self.session,
            "candidate": {
                "commit": self.commit,
                "tree": self.tree,
                "object_format": "sha1",
                "semantic_digest": self.semantic,
            },
            "compatibility": {
                "per_target_tuples": tuples,
                "per_target_artifact_digests": artifacts,
                "schema_digest": "31" * 32,
                "domain_registry_digest": "32" * 32,
                "resource_registry_digest": "33" * 32,
                "storage_schema_version": 1,
                "archive_profile_version": {"major": 2, "minor": 0},
                "migration_profile_version": {"major": 1, "minor": 0},
                "registry_profile_version": {"major": 1, "minor": 0},
                "registry_profile_digest": "34" * 32,
                "wire_session_version": {"major": 1, "minor": 0},
                "product_api_version": {"major": 1, "minor": 1},
                "c_abi_version": {"major": 1, "minor": 0},
                "feature_set_digest": "35" * 32,
            },
            "feature_matrix": {
                "base_default": True,
                "legacy_default": False,
                "network_default": False,
                "network_kill_switch_verified": True,
            },
            "target_receipts": receipts,
            "gate_evidence": gates,
            "child_signatures": signatures,
            "evidence_signatures": evidence_signatures,
            "roots": roots,
            "documents": documents,
            "limitations": ["No Base v2 semantics are implied."],
        }

    def qualify(self, document: dict[str, object] | None = None, *, raw=None, profile=None):
        return qualify_base_for_test_nonproduction(
            QualificationInputs(
                document=copy.deepcopy(document or self.document),
                evidence_bytes=dict(self.raw if raw is None else raw),
                freeze_profile=copy.deepcopy(profile or self.profile),
            )
        )

    def mutate_gate_receipt(self, document, raw, gate, mutate) -> None:
        reference = next(row for row in document["gate_evidence"] if row["gate"] == gate)
        evidence_id = reference["receipt_evidence_id"]
        receipt = json.loads(raw[evidence_id])
        mutate(receipt)
        raw[evidence_id] = canonical_json(receipt)
        reference["receipt_blake3"] = digest_bytes(raw[evidence_id])
        if gate in SIGNED_GATES:
            envelope = next(row for row in document["child_signatures"] if row["gate"] == gate)
            envelope["evidence_blake3"] = reference["receipt_blake3"]
            envelope["fresh"] = receipt["fresh"]
            envelope["carry_forward"] = receipt["carry_forward"]
            envelope["checks_blake3"] = digest_bytes(canonical_json(receipt["checks"]))
            unsigned = {key: value for key, value in envelope.items() if key != "signature"}
            envelope["signature"] = base64.b64encode(
                self.private_keys[envelope["role"]].sign(
                    SIGNATURE_DOMAIN + blake3.blake3(canonical_json(unsigned)).digest()
                )
            ).decode("ascii")

    @staticmethod
    def mutate_target_receipt(document, raw, index, mutate) -> None:
        reference = document["target_receipts"][index]
        evidence_id = reference["receipt_evidence_id"]
        receipt = json.loads(raw[evidence_id])
        mutate(receipt)
        raw[evidence_id] = canonical_json(receipt)
        reference["receipt_blake3"] = digest_bytes(raw[evidence_id])

    def test_derives_qualified_manifest_and_never_accepts_input_claim(self) -> None:
        manifest = self.qualify()
        self.assertTrue(manifest["qualified"])
        self.assertEqual(manifest["candidate"]["semantic_digest"], self.semantic)
        self.assertNotIn("evidence_bytes", manifest)
        claimed = copy.deepcopy(self.document)
        claimed["qualified"] = True
        with self.assertRaisesRegex(BaseQualificationError, "closed|qualified"):
            self.qualify(claimed)

    def test_removing_or_falsifying_every_gate_fails(self) -> None:
        for gate in GATES:
            with self.subTest(gate=gate, mutation="missing"):
                value = copy.deepcopy(self.document)
                value["gate_evidence"] = [row for row in value["gate_evidence"] if row["gate"] != gate]
                with self.assertRaises(BaseQualificationError):
                    self.qualify(value)
            with self.subTest(gate=gate, mutation="failed"):
                value = copy.deepcopy(self.document)
                raw = dict(self.raw)
                self.mutate_gate_receipt(
                    value, raw, gate,
                    lambda receipt: receipt["checks"][0].__setitem__("exit_code", 1),
                )
                with self.assertRaises(BaseQualificationError):
                    self.qualify(value, raw=raw)

    def test_rejects_altered_evidence_and_mixed_bindings(self) -> None:
        mutations = (
            ("qualification_session_id", "91" * 32),
            ("candidate_commit", "9" * 40),
            ("candidate_tree", "8" * 40),
            ("candidate_semantic_digest", "92" * 32),
            ("registry_root", "93" * 32),
        )
        for field, replacement in mutations:
            with self.subTest(field=field):
                value = copy.deepcopy(self.document)
                raw = dict(self.raw)
                self.mutate_gate_receipt(
                    value, raw, GATES[0],
                    lambda receipt, field=field, replacement=replacement: receipt["bindings"].__setitem__(field, replacement),
                )
                with self.assertRaises(BaseQualificationError):
                    self.qualify(value, raw=raw)
        raw = dict(self.raw)
        raw["gate-contract-validators"] += b"tampered"
        with self.assertRaisesRegex(BaseQualificationError, "evidence"):
            self.qualify(raw=raw)

    def test_rejects_duplicate_missing_or_cross_target_receipts(self) -> None:
        for mutation in ("duplicate", "missing", "swap", "mixed-session"):
            with self.subTest(mutation=mutation):
                value = copy.deepcopy(self.document)
                if mutation == "duplicate":
                    value["target_receipts"][1] = copy.deepcopy(value["target_receipts"][0])
                elif mutation == "missing":
                    value["target_receipts"].pop()
                else:
                    if mutation == "swap":
                        raw = dict(self.raw)
                        first = json.loads(raw[value["target_receipts"][0]["receipt_evidence_id"]])
                        second = json.loads(raw[value["target_receipts"][1]["receipt_evidence_id"]])
                        self.mutate_target_receipt(
                            value, raw, 0,
                            lambda receipt: receipt.__setitem__("artifact_tuple_digest", second["artifact_tuple_digest"]),
                        )
                    else:
                        raw = dict(self.raw)
                        self.mutate_target_receipt(
                            value, raw, 0,
                            lambda receipt: receipt["bindings"].__setitem__("qualification_session_id", "ef" * 32),
                        )
                with self.assertRaises(BaseQualificationError):
                    self.qualify(value, raw=locals().get("raw", self.raw))

    def test_rejects_binary_sbom_or_provenance_mismatch(self) -> None:
        for kind in ("binary", "sbom", "provenance"):
            with self.subTest(kind=kind):
                value = copy.deepcopy(self.document)
                raw = dict(self.raw)
                self.mutate_target_receipt(
                    value, raw, 0,
                    lambda receipt, kind=kind: receipt[kind].__setitem__("blake3", "aa" * 32),
                )
                with self.assertRaises(BaseQualificationError):
                    self.qualify(value, raw=raw)

    def test_rejects_missing_or_wrong_spdx_package_verification_code(self) -> None:
        for kind in ("missing", "wrong"):
            with self.subTest(kind=kind):
                value = copy.deepcopy(self.document)
                raw = dict(self.raw)
                reference = value["target_receipts"][0]
                receipt_id = reference["receipt_evidence_id"]
                receipt = json.loads(raw[receipt_id])
                evidence_id = receipt["sbom"]["evidence_id"]
                sbom = json.loads(raw[evidence_id])
                package = sbom["packages"][0]
                if kind == "missing":
                    package.pop("packageVerificationCode")
                else:
                    package["packageVerificationCode"][
                        "packageVerificationCodeValue"
                    ] = "0" * 40
                raw[evidence_id] = canonical_json(sbom)
                receipt["sbom"]["blake3"] = digest_bytes(raw[evidence_id])
                raw[receipt_id] = canonical_json(receipt)
                reference["receipt_blake3"] = digest_bytes(raw[receipt_id])
                with self.assertRaisesRegex(
                    BaseQualificationError, "SPDX package verification code"
                ):
                    self.qualify(value, raw=raw)

    def test_rejects_invalid_bare_or_cross_target_slsa_builder_uri(self) -> None:
        mutations = {
            "invalid": "https://[broken",
            "bare": "onebrain-linux-release-runner",
            "cross-target": BUILDER_IDS["x86_64-pc-windows-msvc"],
        }
        for kind, builder_id in mutations.items():
            with self.subTest(kind=kind):
                value = copy.deepcopy(self.document)
                raw = dict(self.raw)
                reference = value["target_receipts"][0]
                receipt_id = reference["receipt_evidence_id"]
                receipt = json.loads(raw[receipt_id])
                evidence_id = receipt["provenance"]["evidence_id"]
                provenance = json.loads(raw[evidence_id])
                provenance["predicate"]["runDetails"]["builder"]["id"] = builder_id
                raw[evidence_id] = canonical_json(provenance)
                receipt["provenance"]["blake3"] = digest_bytes(raw[evidence_id])
                raw[receipt_id] = canonical_json(receipt)
                reference["receipt_blake3"] = digest_bytes(raw[receipt_id])
                with self.assertRaisesRegex(BaseQualificationError, "SLSA builder"):
                    self.qualify(value, raw=raw)

    def test_rejects_malformed_spdx_and_slsa_subject_or_time_mismatch(self) -> None:
        for kind in (
            "spdx-missing-creation", "spdx-bad-algorithm", "spdx-relationship",
            "spdx-creation-time", "provenance-subject", "provenance-time",
            "provenance-reversed-time",
        ):
            with self.subTest(kind=kind):
                value = copy.deepcopy(self.document)
                raw = dict(self.raw)
                reference = value["target_receipts"][0]
                receipt_id = reference["receipt_evidence_id"]
                receipt = json.loads(raw[receipt_id])
                is_spdx = kind.startswith("spdx")
                evidence_id = receipt["sbom" if is_spdx else "provenance"]["evidence_id"]
                artifact = json.loads(raw[evidence_id])
                if kind == "spdx-missing-creation":
                    artifact.pop("creationInfo")
                    label = "SBOM|SPDX"
                elif kind == "spdx-bad-algorithm":
                    artifact["files"][0]["checksums"][0]["algorithm"] = "BLAKE3"
                    label = "SBOM|SPDX"
                elif kind == "spdx-relationship":
                    artifact["relationships"][1]["relatedSpdxElement"] = "SPDXRef-Decoy"
                    label = "SBOM|SPDX"
                elif kind == "spdx-creation-time":
                    artifact["creationInfo"]["created"] = "2026-08-19T00:00:00Z"
                    label = "SBOM|SPDX"
                elif kind == "provenance-subject":
                    artifact["subject"][0]["digest"]["sha256"] = "aa" * 32
                    label = "provenance"
                elif kind == "provenance-time":
                    artifact["predicate"]["runDetails"]["metadata"]["startedOn"] = (
                        "2026-08-19T00:00:00Z"
                    )
                    label = "provenance"
                else:
                    artifact["predicate"]["runDetails"]["metadata"]["finishedOn"] = (
                        "2026-08-10T23:59:59Z"
                    )
                    label = "provenance"
                raw[evidence_id] = canonical_json(artifact)
                receipt["sbom" if is_spdx else "provenance"]["blake3"] = digest_bytes(
                    raw[evidence_id]
                )
                raw[receipt_id] = canonical_json(receipt)
                reference["receipt_blake3"] = digest_bytes(raw[receipt_id])
                with self.assertRaisesRegex(BaseQualificationError, label):
                    self.qualify(value, raw=raw)

    def test_signed_gate_roots_come_from_verified_receipts(self) -> None:
        value = copy.deepcopy(self.document)
        value["roots"]["registry"] = "ab" * 32
        with self.assertRaisesRegex(BaseQualificationError, "derived|root|binding"):
            self.qualify(value)

    def test_ready_publication_links_exact_bytes_before_directory_fsync(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            path = Path(directory) / "manifest.ready.json"
            events: list[str] = []
            real_link = os.link

            def observed_link(source, destination):
                self.assertTrue(Path(source).is_file())
                self.assertFalse(Path(destination).exists())
                events.append("link")
                return real_link(source, destination)

            with patch(
                "scripts.base.qualify_base.os.link", side_effect=observed_link
            ), patch(
                "scripts.base.qualify_base._fsync_directory",
                side_effect=lambda _path: events.append("fsync"),
            ):
                _atomic_create_or_exact(path, b"ready")
            self.assertEqual(events, ["link", "fsync"])
            self.assertEqual(path.read_bytes(), b"ready")
            self.assertEqual(list(path.parent.glob("*.tmp")), [])
            with self.assertRaisesRegex(BaseQualificationError, "collision"):
                _atomic_create_or_exact(path, b"foreign")

    def test_verify_ready_cli_runs_by_absolute_path_outside_repository(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            request = root / "release-request.json"
            request.write_bytes(canonical_json({"request": "exact"}))
            manifest = {
                "format": "onebrain/base-v1-evidence-manifest/1",
                "candidate": {"commit": "1" * 40, "tree": "2" * 40},
            }
            manifest_bytes = canonical_json(manifest)
            digest = digest_bytes(manifest_bytes)
            generation = root / digest
            generation.mkdir()
            (generation / "manifest.json").write_bytes(manifest_bytes)
            (generation / "manifest.blake3").write_text(digest + "\n", encoding="ascii")
            ready = {
                "format": "onebrain/base-v1-manifest-ready/1",
                "manifest_digest": digest,
                "generation": str(generation),
                "release_request_digest": digest_bytes(request.read_bytes()),
                "qualification_session_id": "3" * 64,
                "candidate": manifest["candidate"],
                "candidate_root": str(root),
                "prepared_candidate_receipt": str(root / "prepared-candidate.json"),
                "prepared_candidate_receipt_blake3": "",
            }
            prepared = root / "prepared-candidate.json"
            prepared.write_bytes(canonical_json({"fixture": "prepared-candidate"}))
            ready["prepared_candidate_receipt_blake3"] = digest_bytes(
                prepared.read_bytes()
            )
            pointer = {
                "format": "onebrain/base-v1-manifest-ready-pointer/1",
                "ready_blake3": digest_bytes(canonical_json(ready)),
                "ready": ready,
            }
            ready_path = root / "manifest.ready.json"
            ready_path.write_bytes(canonical_json(pointer))
            outside = root / "outside"
            outside.mkdir()
            completed = subprocess.run(
                [
                    sys.executable,
                    str(ROOT / "scripts/base/qualify_base.py"),
                    "--verify-ready",
                    str(ready_path),
                    "--release-request",
                    str(request),
                ],
                cwd=outside,
                capture_output=True,
                text=True,
                check=False,
            )
            self.assertEqual(completed.returncode, 0, completed.stderr)
            self.assertEqual(completed.stdout, "")

    def test_rejects_unsupported_profile_carry_forward_or_missing_security_lane(self) -> None:
        profile = copy.deepcopy(self.profile)
        profile["profile_id"] = "UNAPPROVED"
        with self.assertRaisesRegex(BaseQualificationError, "profile"):
            self.qualify(profile=profile)
        production_profile = copy.deepcopy(self.profile)
        production_profile["base_evidence_approver_policy"]["status"] = "owner-approved"
        with self.assertRaisesRegex(BaseQualificationError, "profile digest"):
            qualify_base(
                QualificationInputs(
                    document=copy.deepcopy(self.document),
                    evidence_bytes=dict(self.raw),
                    freeze_profile=production_profile,
                )
            )
        for gate in ("fresh-production-registry", "fresh-exact-candidate-72h-soak"):
            value = copy.deepcopy(self.document)
            raw = dict(self.raw)
            self.mutate_gate_receipt(
                value, raw, gate,
                lambda receipt: receipt.__setitem__("carry_forward", True),
            )
            with self.assertRaisesRegex(BaseQualificationError, "carry-forward"):
                self.qualify(value, raw=raw)
        value = copy.deepcopy(self.document)
        raw = dict(self.raw)
        self.mutate_gate_receipt(
            value, raw, "dependency-security-and-sbom",
            lambda receipt: receipt["details"]["security_lanes"].pop(),
        )
        with self.assertRaisesRegex(BaseQualificationError, "security"):
            self.qualify(value, raw=raw)

    def test_approved_production_evidence_approver_policy_passes_profile_validation(self) -> None:
        """Rejecting the owner-approved canonical policy is a production blocker."""
        frozen = json.loads(
            (ROOT / "src/test-vectors/vnext/base-v1-freeze-v1.json").read_text(
                encoding="utf-8"
            )
        )
        with self.assertRaisesRegex(BaseQualificationError, "qualification input fields"):
            qualify_base(QualificationInputs({}, {}, frozen))

    def test_rejects_evidence_approver_policy_derivation_and_contract_mutations(self) -> None:
        """A wrong key, digest, domain, usage, or interval must not authorize receipts."""
        mutations = {
            "public key": lambda record: record["policy"]["signers"][0].__setitem__(
                "public_key_hex", "00" * 32
            ),
            "fingerprint": lambda record: record["policy"]["signers"][0].__setitem__(
                "fingerprint_hex", "00" * 32
            ),
            "fingerprint context": lambda record: record["policy"]["signers"][0].__setitem__(
                "fingerprint_context", "onebrain:base-v1:wrong-fingerprint:1"
            ),
            "trust digest": lambda record: record.__setitem__(
                "trust_policy_digest", "00" * 32
            ),
            "usage": lambda record: record["policy"].__setitem__(
                "allowed_usages", ["target-receipt-approval"]
            ),
            "validity": lambda record: record["policy"]["signers"][0].__setitem__(
                "expires_utc", record["policy"]["signers"][0]["created_utc"]
            ),
        }
        for label, mutate in mutations.items():
            with self.subTest(label=label):
                profile = copy.deepcopy(self.profile)
                mutate(profile["base_evidence_approver_policy"])
                with self.assertRaisesRegex(
                    BaseQualificationError,
                    "evidence approver|fingerprint|trust|policy|usage|validity",
                ):
                    self.qualify(profile=profile)

    def test_rejects_candidate_request_outside_evidence_approver_validity(self) -> None:
        """Accepting candidate evidence outside signer validity is a trust bypass."""
        profile = copy.deepcopy(self.profile)
        policy_record = profile["base_evidence_approver_policy"]
        policy_record["policy"]["signers"][0]["expires_utc"] = (
            "2026-08-17T23:59:59Z"
        )
        policy_record["trust_policy_digest"] = blake3.blake3(
            canonical_json(policy_record["policy"]),
            derive_key_context=EVIDENCE_APPROVER_POLICY_CONTEXT,
        ).hexdigest()
        with self.assertRaisesRegex(BaseQualificationError, "validity"):
            self.qualify(profile=profile)

    def test_rejects_valid_signature_from_unlisted_or_wrong_role_key(self) -> None:
        for mutation in ("unlisted", "wrong-role"):
            with self.subTest(mutation=mutation):
                value = copy.deepcopy(self.document)
                envelope = value["child_signatures"][0]
                unsigned = {key: item for key, item in envelope.items() if key != "signature"}
                if mutation == "unlisted":
                    key = Ed25519PrivateKey.generate()
                else:
                    other_role = next(role for role in self.private_keys if role != envelope["role"])
                    key = self.private_keys[other_role]
                envelope["signature"] = base64.b64encode(
                    key.sign(SIGNATURE_DOMAIN + blake3.blake3(canonical_json(unsigned)).digest())
                ).decode("ascii")
                with self.assertRaisesRegex(BaseQualificationError, "signature"):
                    self.qualify(value)

    def test_rejects_duplicate_gate_and_noncanonical_tuple_artifact_binding(self) -> None:
        value = copy.deepcopy(self.document)
        value["gate_evidence"][1] = copy.deepcopy(value["gate_evidence"][0])
        with self.assertRaises(BaseQualificationError):
            self.qualify(value)
        value = copy.deepcopy(self.document)
        target = TARGETS["linux"]
        value["compatibility"]["per_target_tuples"][target]["feature_set_digest"] = "ff" * 32
        with self.assertRaisesRegex(BaseQualificationError, "semantic|artifact"):
            self.qualify(value)

    def test_rejects_opaque_caller_authored_gate_result_even_when_rehashed(self) -> None:
        """Removing the machine receipt parser must make this test fail."""
        value = copy.deepcopy(self.document)
        raw = dict(self.raw)
        row = value["gate_evidence"][0]
        raw[row["receipt_evidence_id"]] = b"caller-authored: result=pass"
        row["receipt_blake3"] = digest_bytes(raw[row["receipt_evidence_id"]])
        with self.assertRaisesRegex(BaseQualificationError, "receipt|canonical|machine"):
            self.qualify(value, raw=raw)

    def test_rejects_opaque_target_artifacts_as_a_target_pass_claim(self) -> None:
        """Trusting outer target result instead of a parsed receipt is a bug."""
        value = copy.deepcopy(self.document)
        raw = dict(self.raw)
        reference = value["target_receipts"][0]
        receipt_id = reference["receipt_evidence_id"]
        receipt = json.loads(raw[receipt_id])
        for kind in ("binary", "sbom", "provenance"):
            evidence_id = receipt[kind]["evidence_id"]
            raw[evidence_id] = f"caller-authored-{kind}".encode()
            receipt[kind]["blake3"] = digest_bytes(raw[evidence_id])
        raw[receipt_id] = canonical_json(receipt)
        reference["receipt_blake3"] = digest_bytes(raw[receipt_id])
        with self.assertRaisesRegex(BaseQualificationError, "receipt|canonical|machine"):
            self.qualify(value, raw=raw)

    def test_rejects_fabricated_zero_exit_for_unexecuted_unknown_command(self) -> None:
        """An asserted exit=0 for an unfrozen command and empty output is not evidence."""
        value = copy.deepcopy(self.document)
        raw = dict(self.raw)

        def fabricate(receipt):
            check = receipt["checks"][0]
            check["command"] = ["definitely-not-executed"]
            check["command_blake3"] = digest_bytes(canonical_json(check["command"]))
            raw[check["stdout_evidence_id"]] = b""
            check["stdout_blake3"] = digest_bytes(b"")
            raw[check["stderr_evidence_id"]] = b""
            check["stderr_blake3"] = digest_bytes(b"")
            check["exit_code"] = 0

        self.mutate_gate_receipt(value, raw, "contract-validators", fabricate)
        with self.assertRaisesRegex(BaseQualificationError, "frozen|command|output|oracle"):
            self.qualify(value, raw=raw)

    def test_every_unsigned_gate_and_target_requires_evidence_approver_signature(self) -> None:
        value = copy.deepcopy(self.document)
        value["evidence_signatures"].pop()
        with self.assertRaisesRegex(BaseQualificationError, "evidence approver|signature"):
            self.qualify(value)

    def test_rehashing_fabricated_output_without_approver_resignature_fails(self) -> None:
        value = copy.deepcopy(self.document)
        raw = dict(self.raw)
        reference = next(
            row for row in value["gate_evidence"] if row["gate"] == "contract-validators"
        )
        receipt = json.loads(raw[reference["receipt_evidence_id"]])
        check = receipt["checks"][0]
        stdout = json.loads(raw[check["stdout_evidence_id"]])
        assertion = stdout["assertions"][0]
        raw[assertion["evidence_id"]] = b"caller-fabricated-but-rehashed"
        assertion["evidence_blake3"] = digest_bytes(raw[assertion["evidence_id"]])
        stdout["assertion_root"] = digest_bytes(canonical_json(stdout["assertions"]))
        raw[check["stdout_evidence_id"]] = canonical_json(stdout)
        check["stdout_blake3"] = digest_bytes(raw[check["stdout_evidence_id"]])
        raw[reference["receipt_evidence_id"]] = canonical_json(receipt)
        reference["receipt_blake3"] = digest_bytes(raw[reference["receipt_evidence_id"]])
        with self.assertRaisesRegex(BaseQualificationError, "evidence approver|signature"):
            self.qualify(value, raw=raw)


if __name__ == "__main__":
    unittest.main()

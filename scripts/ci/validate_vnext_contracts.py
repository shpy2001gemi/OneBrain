#!/usr/bin/env python3
"""Dependency-free structural checks for OneBrain vNext contracts."""

from __future__ import annotations

import hashlib
import ast
import json
import os
import re
import subprocess
import sys
import tomllib
from datetime import datetime, timezone
from pathlib import Path
from urllib.parse import urlsplit

import blake3


ROOT = Path(__file__).resolve().parents[2]
PLAN = ROOT / "docs/research/ONEBRAIN_FOUNDATION_IMPLEMENTATION_PLAN_V7_1.md"
VNEXT = ROOT / "docs/specs/vnext"
TRACEABILITY = VNEXT / "TRACEABILITY_MATRIX_V1.md"
VOCABULARY = VNEXT / "NORMATIVE_VOCABULARY_V1.md"
NEGATIVE_ASSERTIONS = VNEXT / "negative_assertions.yaml"
NORMATIVE_COVERAGE = VNEXT / "normative_coverage.json"
VECTORS = ROOT / "src/test-vectors/vnext/foundation/canonical-v1.json"
IDENTITY_OBJECT_VECTORS = (
    ROOT / "src/test-vectors/vnext/foundation/identity-object-v1.json"
)
FEED_EVENT_VECTORS = ROOT / "src/test-vectors/vnext/foundation/feed-event-v1.json"
PRODUCT_PROFILE = (
    ROOT / "src/test-vectors/vnext/product-integration-profile-v1.json"
)
PRIVATE_WS_PROFILE = (
    ROOT / "src/test-vectors/vnext/private-websocket-profile-v1.json"
)
VNEXT_CLI_PROFILE = ROOT / "src/test-vectors/vnext/vnext-cli-profile-v1.json"
VNEXT_DESKTOP_WEB_UX_PROFILE = (
    ROOT / "src/test-vectors/vnext/vnext-desktop-web-ux-profile-v1.json"
)
DR_M5_BASELINE_PROFILE = ROOT / "src/test-vectors/vnext/dr-m5-baseline-v1.json"
DR_M5_RESOURCE_PROFILE = (
    ROOT / "src/test-vectors/vnext/dr-m5-resource-admission-v1.json"
)
DR_M5_OBSERVABILITY_PROFILE = (
    ROOT / "src/test-vectors/vnext/dr-m5-observability-v1.json"
)
DR_M5_CRASH_HARNESS_PROFILE = (
    ROOT / "src/test-vectors/vnext/dr-m5-crash-harness-v1.json"
)
DR_M5_CHAOS_FUZZ_PROFILE = (
    ROOT / "src/test-vectors/vnext/dr-m5-chaos-fuzz-v1.json"
)
DR_M5_OPERATIONAL_COMPACTION_PROFILE = (
    ROOT / "src/test-vectors/vnext/dr-m5-operational-compaction-v1.json"
)
DR_M5_MIXED_ROLLBACK_PROFILE = (
    ROOT / "src/test-vectors/vnext/dr-m5-mixed-rollback-v1.json"
)
DR_M5_SOAK_RELEASE_PROFILE = (
    ROOT / "src/test-vectors/vnext/dr-m5-soak-release-v1.json"
)
P5_CANARY_PREFLIGHT_PROFILE = (
    ROOT / "src/test-vectors/vnext/p5-canary-preflight-v1.json"
)
P5_OPERATIONS_PREFLIGHT_PROFILE = (
    ROOT / "src/test-vectors/vnext/p5-operations-preflight-v1.json"
)
P5_MULTI_HOST_PRODUCTION_PROFILE = (
    ROOT / "src/test-vectors/vnext/p5-multi-host-production-qualification-v1.json"
)
BASE_V1_EXACT_CANDIDATE_SOAK_PROFILE = (
    ROOT / "src/test-vectors/vnext/base-v1-exact-candidate-soak-v1.json"
)
CONCEPT_REGISTRY_OPERATIONS_PROFILE = (
    ROOT / "src/test-vectors/vnext/concept-registry-operations-v1.json"
)
CONCEPT_REGISTRY_PRODUCTION_QUALIFICATION_PROFILE = (
    ROOT
    / "src/test-vectors/vnext/concept-registry-production-qualification-v1.json"
)
BASE_V1_AUTHORITY_RECOVERY_PROFILE = (
    ROOT / "src/test-vectors/vnext/base-v1-authority-recovery-v1.json"
)
BASE_V1_STORAGE_INTEGRITY_PROFILE = (
    ROOT / "src/test-vectors/vnext/base-v1-storage-integrity-v1.json"
)
BASE_V1_DERIVED_PROJECTION_PROFILE = (
    ROOT / "src/test-vectors/vnext/base-v1-derived-projection-v1.json"
)
BASE_V1_ARCHIVE_PROFILE = ROOT / "src/test-vectors/vnext/base-v1-archive-v1.json"
BASE_V1_RUNTIME_INTERFACE_PROFILE = (
    ROOT / "src/test-vectors/vnext/base-v1-runtime-interface-v1.json"
)
BASE_V1_RUNTIME_INTERFACE_HISTORY = (
    ROOT / "src/test-vectors/vnext/base-v1-runtime-interface-history-v1.json"
)
BASE_V1_COMPATIBILITY_PROFILE = (
    ROOT / "src/test-vectors/vnext/base-v1-compatibility-v1.json"
)
BASE_V1_FREEZE_PROFILE = ROOT / "src/test-vectors/vnext/base-v1-freeze-v1.json"
BASE_V1_RELEASE_SIGNERS = (
    ROOT / "src/test-vectors/vnext/base-v1-release-signers-v1.json"
)
BASE_V1_FREEZE_DOCUMENT = VNEXT / "BASE_V1_FREEZE_AND_EVIDENCE_PROFILE.md"
DR_M5_TRANSACTION_INVENTORY = (
    VNEXT / "DISTRIBUTED_RUNTIME_TRANSACTION_BOUNDARY_INVENTORY_V1.md"
)
VNEXT_FOUNDATION_WORKFLOW = ROOT / ".github/workflows/vnext-foundation.yml"
VNEXT_SOAK_WORKFLOW = ROOT / ".github/workflows/vnext-soak.yml"
VNEXT_MACOS_SOAK_WORKFLOW = (
    ROOT / ".github/workflows/vnext-soak-macos-arm64.yml"
)
VNEXT_SOAK_RUNNER_SCRIPT = ROOT / "scripts/runner/onebrain-soak-runner.sh"
VNEXT_SOAK_RUNNER_GUIDE = (
    ROOT / "docs/operations/ONEBRAIN_SOAK_RUNNER_GUIDE_V1.md"
)
VNEXT_MACOS_SOAK_RUNNER_GUIDE = (
    ROOT / "docs/operations/ONEBRAIN_SOAK_RUNNER_MAC_M2_GUIDE_V1.md"
)
CONCEPT_REGISTRY_PRODUCTION_WORKFLOW = (
    ROOT / ".github/workflows/concept-registry-production.yml"
)
BASE_V1_P5_PRODUCTION_WORKFLOW = (
    ROOT / ".github/workflows/vnext-p5-production-canary.yml"
)
BASE_V1_CANDIDATE_WORKFLOW = ROOT / ".github/workflows/base-v1-candidate.yml"
CONCEPT_REGISTRY_RUNNER_SCRIPT = (
    ROOT / "scripts/runner/onebrain-registry-runner.sh"
)
CONCEPT_REGISTRY_RUNNER_GUIDE = (
    ROOT / "docs/operations/ONEBRAIN_REGISTRY_RUNNER_GUIDE_V1.md"
)

TASK_ROW = re.compile(r"^\|\s*\[[ x~]\]\s*`([A-Z][A-Z0-9]*-\d{3})`")
TASK_ID = re.compile(r"(?<!ADR-)(?<!NEG-)\b[A-Z][A-Z0-9]*-\d{3}\b")
ADR_ID = re.compile(r"\bADR-[A-Z0-9]+-\d{3}-\d{2}\b")
NEGATIVE_ID = re.compile(r"\bNEG-[A-Z0-9-]+\b")
MARKDOWN_LINK = re.compile(r"\[[^\]]*\]\(([^)]+)\)")
NORMATIVE_KEYWORD = re.compile(r"\b(?:MUST|SHOULD)\b")


class ContractError(RuntimeError):
    pass


def read(path: Path) -> str:
    try:
        return path.read_text(encoding="utf-8")
    except OSError as error:
        raise ContractError(f"cannot read {path.relative_to(ROOT)}: {error}") from error


def plan_tasks() -> tuple[set[str], dict[str, set[str]]]:
    definitions: list[str] = []
    dependencies: dict[str, set[str]] = {}
    for line in read(PLAN).splitlines():
        match = TASK_ROW.match(line)
        if not match:
            continue
        task = match.group(1)
        definitions.append(task)
        cells = line.split("|")
        dependency_cell = cells[4] if len(cells) > 4 else ""
        dependencies[task] = set(TASK_ID.findall(dependency_cell))

    unique = set(definitions)
    if len(unique) != len(definitions):
        duplicates = sorted(task for task in unique if definitions.count(task) > 1)
        raise ContractError(f"duplicate plan task definitions: {duplicates}")
    if len(unique) < 99:
        raise ContractError(f"expected at least 99 plan tasks, found {len(unique)}")

    undefined = sorted(
        dependency
        for task_dependencies in dependencies.values()
        for dependency in task_dependencies
        if dependency not in unique
    )
    if undefined:
        raise ContractError(f"undefined task dependencies: {undefined}")
    assert_acyclic(dependencies)
    return unique, dependencies


def assert_acyclic(graph: dict[str, set[str]]) -> None:
    visiting: set[str] = set()
    visited: set[str] = set()

    def visit(task: str, path: list[str]) -> None:
        if task in visiting:
            cycle = " -> ".join(path + [task])
            raise ContractError(f"task dependency cycle: {cycle}")
        if task in visited:
            return
        visiting.add(task)
        for dependency in graph.get(task, set()):
            visit(dependency, path + [task])
        visiting.remove(task)
        visited.add(task)

    for task in graph:
        visit(task, [])


def validate_traceability(tasks: set[str]) -> int:
    text = read(TRACEABILITY)
    referenced_tasks = set(TASK_ID.findall(text))
    undefined = sorted(referenced_tasks - tasks)
    if undefined:
        raise ContractError(f"traceability references undefined tasks: {undefined}")
    adrs = set(ADR_ID.findall(text))
    if len(adrs) < 18:
        raise ContractError(f"expected at least 18 traced ADRs, found {len(adrs)}")
    return len(adrs)


def validate_base_v1_authority_recovery(
    profile: dict[str, object] | None = None,
) -> tuple[int, int]:
    if profile is None:
        try:
            profile = json.loads(read(BASE_V1_AUTHORITY_RECOVERY_PROFILE))
        except json.JSONDecodeError as error:
            raise ContractError(f"invalid Base v1 authority profile JSON: {error}") from error
    if not isinstance(profile, dict):
        raise ContractError("Base v1 authority profile must be an object")

    expected_top_level = {
        "format": "onebrain/base-v1-authority-recovery/1",
        "canonical_write_path": "vnext-object-event-feed",
        "legacy_boundary": "explicit-read-only-migration",
        "recovery_profile": "encrypted-recovery-package-v1",
        "archive_profiles": ["password-argon2id-v1", "recovery-key-v1"],
        "registry_required_states": ["registry-dependent-encoding", "ready-offline"],
        "network_default_active_lane_count": 0,
        "delete_semantics": "event-or-local-retention-never-history-rewrite",
        "authority_order": [
            "distributed-runtime-plan",
            "mobile-architecture-constraints",
            "base-v1-authority-recovery-profile",
            "product-projections",
        ],
        "recovery_decision": {
            "selected": "encrypted-recovery-package-v1",
            "rejected": ["mnemonic-derivation", "bip39-shaped-placeholder"],
        },
    }
    for field, expected in expected_top_level.items():
        if profile.get(field) != expected:
            raise ContractError(f"unexpected Base v1 {field}")

    expected_archive_crypto = {
        "password_argon2id_v1": {
            "algorithm": "argon2id",
            "memory_kib": 65536,
            "iterations": 3,
            "parallelism": 1,
            "salt_bytes": 16,
            "output_bytes": 32,
            "domain": "onebrain:base-v1:archive:password-argon2id-v1",
        },
        "recovery_key_v1": {
            "key_bytes": 32,
            "derivation": "blake3-derive-key",
            "separately_verified": True,
            "domain": "onebrain:base-v1:archive:recovery-key-v1",
        },
        "aead": "xchacha20-poly1305",
        "nonce_bytes": 24,
        "manifest": "encrypted-and-authenticated",
        "profiles_use_distinct_domains": True,
    }
    archive_crypto = profile.get("archive_crypto")
    if not isinstance(archive_crypto, dict):
        raise ContractError("invalid Base v1 archive crypto")
    if archive_crypto.get("password_argon2id_v1") != expected_archive_crypto[
        "password_argon2id_v1"
    ]:
        raise ContractError("unexpected Base v1 password Argon2id parameters")
    if archive_crypto != expected_archive_crypto:
        raise ContractError("unexpected Base v1 archive crypto")

    expected_archive_scope = {
        "included": [
            "canonical-object-event-feed",
            "owned-original-blobs",
            "private-vault",
            "quarantine",
            "correctness-journals",
            "pending-outbox",
            "migration-state",
            "interpretation-configuration",
            "permitted-recovery-metadata",
            "signed-authority-high-water-metadata",
        ],
        "excluded_rebuildable_or_refetchable": [
            "derived-indexes",
            "concept-registry-bytes",
            "local-model-bytes",
            "remote-media-cache",
            "stale-delivery-caches",
        ],
        "restore_activation": (
            "verify-entire-archive-stage-new-generation-parity-health-then-atomic-switch"
        ),
    }
    archive_scope = profile.get("archive_scope")
    if archive_scope != expected_archive_scope:
        raise ContractError("unexpected Base v1 archive scope")

    signer_recovery = profile.get("signer_recovery")
    expected_domains = {
        "node_transport": "onebrain:base-v1:recovery:node-transport:1",
        "actor_root": "onebrain:base-v1:recovery:actor-root:1",
        "feed_author": "onebrain:base-v1:recovery:feed-author:1",
    }
    if not isinstance(signer_recovery, dict) or set(signer_recovery) != set(
        expected_domains
    ):
        raise ContractError("unexpected Base v1 signer recovery domains")
    actual_domains: list[str] = []
    for signer, domain in expected_domains.items():
        policy = signer_recovery.get(signer)
        if not isinstance(policy, dict) or policy.get("domain") != domain:
            raise ContractError("unexpected Base v1 signer recovery domains")
        if policy.get("non_exportable_unavailable") != "ReprovisionRequired":
            raise ContractError("unexpected Base v1 non-exportable signer disposition")
        if set(policy) != {"domain", "non_exportable_unavailable"}:
            raise ContractError("unexpected Base v1 signer recovery policy")
        actual_domains.append(domain)
    if len(set(actual_domains)) != len(actual_domains):
        raise ContractError("Base v1 signer recovery domains must be distinct")

    expected_registry_policy = {
        "bootstrap_limited_without_active_release": True,
        "missing_exact_release": "fail-closed",
        "registry_dependent_encoding_requires_exact_release": True,
        "ready_offline_requires_exact_release": True,
    }
    if profile.get("registry_policy") != expected_registry_policy:
        raise ContractError("unexpected Base v1 Registry policy")

    allowed_fields = set(expected_top_level) | {
        "archive_crypto",
        "archive_scope",
        "signer_recovery",
        "registry_policy",
    }
    if set(profile) != allowed_fields:
        raise ContractError("unexpected Base v1 authority profile fields")
    return len(expected_domains), len(expected_archive_scope["included"])


def validate_base_v1_storage_integrity(
    profile: dict[str, object] | None = None,
    projection: dict[str, object] | None = None,
) -> tuple[int, int]:
    if profile is None:
        try:
            profile = json.loads(read(BASE_V1_STORAGE_INTEGRITY_PROFILE))
        except json.JSONDecodeError as error:
            raise ContractError(f"invalid Base v1 storage profile JSON: {error}") from error
    if projection is None:
        try:
            projection = json.loads(read(BASE_V1_DERIVED_PROJECTION_PROFILE))
        except json.JSONDecodeError as error:
            raise ContractError(
                f"invalid Base v1 derived projection JSON: {error}"
            ) from error
    if not isinstance(profile, dict) or not isinstance(projection, dict):
        raise ContractError("Base v1 storage profiles must be objects")

    expected_boundaries = [
        "full-cid-blob-layout",
        "full-read-blob-integrity",
        "bounded-capacity-admission",
        "journaled-filesystem-commit",
        "same-transaction-secondary-indexes",
        "generation-swapped-derived-projections",
        "character-safe-preview",
        "exact-canonical-import-export",
        "owned-blob-reference-authority",
        "closed-storage-owner-mapping",
    ]
    if (
        profile.get("format") != "onebrain/base-v1-storage-integrity/1"
        or profile.get("authoritative_boundaries") != expected_boundaries
    ):
        raise ContractError("unexpected Base v1 storage integrity profile")

    expected_blob_layout = {
        "cid_bytes": 34,
        "cid_hex_characters": 68,
        "hex_case": "lower",
        "relative_path": (
            "v2/<digest-byte-0-hex>/<digest-byte-1-hex>/"
            "<full-68-lower-hex-cid>"
        ),
        "shard_source": "two-leading-digest-bytes-not-version-or-type",
        "short_cid_use": "display-only",
    }
    if profile.get("blob_layout") != expected_blob_layout:
        raise ContractError("unexpected Base v1 blob layout")

    expected_read_integrity = {
        "required_checks": [
            "declared-type",
            "declared-length",
            "each-chunk-blake3",
            "full-payload-blake3",
            "full-typed-cid",
        ],
        "check_before_returning_bytes": True,
        "legacy_missing_chunk_hash": "typed-migration-required",
    }
    if profile.get("blob_read_integrity") != expected_read_integrity:
        raise ContractError("unexpected Base v1 blob read integrity")

    expected_capacity = {
        "chunk_max_bytes": 262144,
        "per_object_max_bytes": 104857600,
        "total_quota_bytes": "required-configured-nonzero-u64",
        "free_space_reserve_bytes": "required-configured-nonzero-u64",
        "accounting": "unique-owned-physical-bytes",
        "arithmetic": "checked-add-and-subtract-reject-overflow-underflow",
        "admission_order": "limits-and-space-before-write-side-effect",
    }
    if profile.get("capacity_admission") != expected_capacity:
        raise ContractError("unexpected Base v1 capacity admission")

    expected_filesystem_commit = {
        "protocol": (
            "durable-intent-stage-fsync-atomic-publish-metadata-commit-cleanup"
        ),
        "reopen": "reconcile-every-nonterminal-intent-idempotently",
        "unsafe_overwrite": False,
    }
    if profile.get("filesystem_commit") != expected_filesystem_commit:
        raise ContractError("unexpected Base v1 filesystem commit")

    expected_secondary = {
        "class": "same-redb-transaction",
        "required_indexes": [
            "feed-inception-by-feed-id",
            "authority-event-by-principal-frontier",
        ],
        "parity": "canonical-write-and-index-row-commit-or-abort-together",
    }
    if profile.get("secondary_index_policy") != expected_secondary:
        raise ContractError("unexpected Base v1 secondary index policy")

    expected_derived = {
        "stores": ["graph", "search", "retriever"],
        "class": "disposable-generation-swapped",
        "row_binding": [
            "source-root",
            "mapping-id",
            "reducer-version",
            "index-root",
            "projection-root",
        ],
        "corrupt_reopen": "mark-dirty-rebuild-from-validated-canonical-records",
        "publication_failure_state": (
            "canonical-available-derived-degraded-dirty-generation"
        ),
        "parity_operations": ["create", "update", "delete", "rebuild"],
        "empty_projection": (
            "allowed-only-when-source-kind-coverage-proves-no-output"
        ),
        "archive_bytes": "excluded-rebuildable",
    }
    if profile.get("derived_store_policy") != expected_derived:
        raise ContractError("unexpected Base v1 derived store policy")

    expected_preview = {
        "input": "validated-utf8",
        "truncation_unit": "unicode-scalar-value",
        "maximum_scalars": 80,
        "invalid_utf8": "reject",
        "byte_slice": "forbidden",
    }
    if profile.get("text_preview") != expected_preview:
        raise ContractError("unexpected Base v1 text preview")

    expected_exchange = {
        "export": "exact-validated-canonical-bytes-with-full-typed-reference",
        "import": (
            "decode-validate-reencode-byte-equality-and-recompute-cid-before-commit"
        ),
        "round_trip": "same-bytes-same-cid-same-type-same-length",
        "partial_success": "explicit-per-record-result-never-silent",
    }
    if profile.get("canonical_exchange") != expected_exchange:
        raise ContractError("unexpected Base v1 canonical exchange")

    expected_blob_reference = {
        "format": "onebrain/owned-blob-reference/1",
        "authority_source": "validated-vnext-object-event-bytes-only",
        "owner": "full-typed-ObjectReference",
        "required_fields": [
            "owner",
            "blob-cid",
            "role",
            "retention-state",
        ],
        "roles": ["owned-original", "attachment", "source-artifact"],
        "retention_states": ["live", "terminal-retain", "terminal-release"],
        "terminal_event_binding": (
            "terminal-states-require-full-validated-EventCID-live-state-omits"
        ),
        "terminal_event_semantics": (
            "validated-owner-terminal-event-reduces-retention-never-rewrites-history"
        ),
        "legacy_ku_metadata": "read-only-migration-evidence-never-authority",
    }
    if profile.get("owned_blob_reference") != expected_blob_reference:
        raise ContractError("unexpected Base v1 owned blob reference")

    owner_names = [
        "canonical",
        "vault",
        "quarantine",
        "blob",
        "pending_blob_intent",
        "source_capture_intent",
        "reconciliation",
        "inventory",
        "outbox",
        "provenance",
        "private_kql",
        "private_pomv",
        "operational",
        "rollout",
        "optional_network",
        "migration",
        "base_operations",
        "interpretation_config",
        "identity",
        "registry_metadata",
        "derived_index",
        "retriever_projection",
    ]
    expected_owner_rows = []
    for code, name in enumerate(owner_names, start=1):
        encoded = code.to_bytes(2, "big").hex()
        expected_owner_rows.append(
            {
                "name": name,
                "code_u16": code,
                "code_hex": f"0x{code:04X}",
                "base_storage_owner": name,
                "archive_owner": name,
                "base_storage_owner_bytes": encoded,
                "archive_owner_bytes": encoded,
            }
        )
    expected_owner_table = {
        "encoding": "big-endian-u16",
        "base_storage_owner_type": "BaseStorageOwnerId",
        "archive_owner_type": "ArchiveOwner",
        "conversion_owner": "onebrain-node-adapter-only",
        "reserved": ["0x0000", "0x0017..0xFFFF"],
        "unknown_reserved_reused": "fail-closed",
        "owners": expected_owner_rows,
    }
    if profile.get("storage_owner_table") != expected_owner_table:
        raise ContractError("unexpected Base v1 storage owner table")

    expected_phases = [
        "before_begin_write",
        "after_begin_write_before_mutation",
        "after_mutation_before_commit",
        "after_commit_before_next_side_effect",
        "after_next_side_effect_before_ack",
    ]
    if profile.get("failpoint_phases") != expected_phases:
        raise ContractError("unexpected Base v1 failpoint phases")
    expected_tx_boundaries = [
        "TX-BLOB-001",
        "TX-IDX-001",
        "TX-ARCH-001",
        "TX-RESTORE-001",
        "TX-RECOVERY-001",
    ]
    if profile.get("transaction_boundaries") != expected_tx_boundaries:
        raise ContractError("unexpected Base v1 transaction boundaries")
    inventory = read(DR_M5_TRANSACTION_INVENTORY)
    for boundary_id in expected_tx_boundaries:
        if f"| `{boundary_id}` |" not in inventory:
            raise ContractError(f"Base v1 transaction inventory lacks {boundary_id}")
    for index, phase in enumerate(expected_phases, start=1):
        if f"{index}. `{phase}`" not in inventory:
            raise ContractError(f"Base v1 failpoint phase missing: {phase}")

    expected_crash_oracle = {
        "runner": "child-process-real-files-kill-reopen",
        "every_boundary_every_phase": True,
        "reopen_outcome": "exact-pre-state-or-exact-post-state-never-partial",
        "expected_oracle_location": "outside-store-under-test",
        "recorded_fields": [
            "boundary-id",
            "phase",
            "process-exit",
            "restart-result",
            "oracle-digest",
        ],
    }
    if profile.get("crash_oracle") != expected_crash_oracle:
        raise ContractError("unexpected Base v1 crash oracle")

    expected_negative_oracles = [
        "short-cid-path-rejected",
        "missing-full-read-hash-rejected",
        "missing-total-quota-rejected",
        "best-effort-without-dirty-generation-rejected",
        "unknown-or-vacuous-projection-rejected",
        "legacy-blob-reference-authority-rejected",
        "corrupt-retriever-rebuilds-with-canonical-startup-available",
        "missing-update-delete-parity-rejected",
        "byte-sliced-utf8-rejected",
    ]
    if profile.get("negative_oracles") != expected_negative_oracles:
        raise ContractError("unexpected Base v1 negative oracles")
    if set(profile) != {
        "format",
        "authoritative_boundaries",
        "blob_layout",
        "blob_read_integrity",
        "capacity_admission",
        "filesystem_commit",
        "secondary_index_policy",
        "derived_store_policy",
        "text_preview",
        "canonical_exchange",
        "owned_blob_reference",
        "storage_owner_table",
        "failpoint_phases",
        "transaction_boundaries",
        "crash_oracle",
        "negative_oracles",
    }:
        raise ContractError("unexpected Base v1 storage profile fields")

    expected_projection_header = {
        "format": "onebrain/base-v1-derived-projection/1",
        "accepted_record_families": [
            "object",
            "event",
            "feed-inception",
            "authority-event",
        ],
        "mapping_reducer_id": "base-v1-derived-projection-reducer/1",
        "projection_root_domain": "onebrain:base-v1:derived-projection-root:1",
        "row_binding": [
            "source-root",
            "canonical-record-reference",
            "mapping-id",
            "reducer-version",
            "output-key",
            "output-value",
            "index-root",
        ],
        "branch_handling": (
            "retain-all-canonical-branches-no-winner-from-count-order-or-score"
        ),
        "tombstone_handling": (
            "apply-validated-terminal-reducer-never-rewrite-canonical-history"
        ),
        "unknown_kind_exclusion": (
            "canonical-opaque-or-quarantine-by-criticality-no-derived-row"
        ),
        "empty_projection_rule": (
            "coverage-must-name-every-input-and-prove-zero-output-per-mapping"
        ),
    }
    for field, expected in expected_projection_header.items():
        if projection.get(field) != expected:
            raise ContractError(f"unexpected Base v1 projection {field}")

    object_kinds = [
        "legacy-evidence",
        "semantic-kernel",
        "receptor-definition",
        "assembly-manifest",
        "knowledge-affordance",
        "mapping-envelope",
        "query-definition",
        "capability-definition",
        "implementation-manifest",
        "conformance-fixture",
        "receptor-claim-envelope",
        "receptor-resolution-action",
        "use-evidence",
        "derivation-evidence",
        "encoding-attempt",
        "fidelity-policy",
        "encoding-fidelity-attestation",
        "sanitized-public-problem",
        "outcome-observation",
        "benefit-evidence",
        "exploration-policy",
        "source-artifact",
        "observation-event-payload",
    ]
    object_exclusions = [
        "never-authority-or-blob-retention-source",
        "none-after-schema-and-disclosure-validation",
        "none-after-schema-and-disclosure-validation",
        "none-after-schema-and-disclosure-validation",
        "none-after-schema-and-disclosure-validation",
        "inactive-without-materialization-and-adoption",
        "private-vault-only-never-public-index",
        "projection-never-grants-capability",
        "projection-never-authorizes-execution",
        "never-production-authority",
        "claim-is-not-resolution",
        "consume-reduced-resolution-view-not-raw-action",
        "does-not-prove-benefit-truth-or-reward",
        "does-not-prove-benefit-truth-or-reward",
        "attempt-is-not-success-or-authority",
        "policy-relative-not-global-truth",
        "attestation-does-not-rewrite-source",
        "never-reconstruct-private-source",
        "observation-is-not-benefit-or-truth",
        "conflicts-coexist-no-score-authority",
        "policy-cannot-change-eligibility",
        "private-or-disclosure-scoped-content-only",
        "payload-needs-validated-event-for-exercise-view",
    ]
    schema_registry = read(ROOT / "src/ku-core/src/foundation/schema_registry.rs")
    object_registry_match = re.search(
        r"(?s)pub const OBJECT_KINDS_V1:.*?= &\[(.*?)\];", schema_registry
    )
    event_registry_match = re.search(
        r"(?s)pub const EVENT_TYPES_V1:.*?= &\[(.*?)\];", schema_registry
    )
    object_constants = {
        name: int(value)
        for name, value in re.findall(
            r"pub const (OBJECT_KIND_[A-Z0-9_]+): u64 = (\d+);",
            schema_registry,
        )
    }
    event_constants = {
        name: int(value)
        for name, value in re.findall(
            r"pub const (EVENT_TYPE_[A-Z0-9_]+): u64 = (\d+);",
            schema_registry,
        )
    }
    if not object_registry_match or not event_registry_match:
        raise ContractError("Base v1 projection cannot read the schema registry")
    source_object_rows = [
        (object_constants.get(constant), name)
        for constant, name in re.findall(
            r'id:\s*(OBJECT_KIND_[A-Z0-9_]+),\s*name:\s*"([^"]+)"',
            object_registry_match.group(1),
        )
    ]
    source_event_rows = [
        (event_constants.get(constant), name)
        for constant, name in re.findall(
            r'id:\s*(EVENT_TYPE_[A-Z0-9_]+),\s*name:\s*"([^"]+)"',
            event_registry_match.group(1),
        )
    ]
    if source_object_rows != list(enumerate(object_kinds, start=1)):
        raise ContractError("Base v1 object mapping drifts from the schema registry")
    expected_object_rows = []
    for object_id, (kind, exclusion) in enumerate(
        zip(object_kinds, object_exclusions, strict=True), start=1
    ):
        expected_object_rows.append(
            {
                "id": object_id,
                "kind": kind,
                "mapping_id": f"base-v1/object/{object_id}-{kind}/1",
                "reducer_version": 1,
                "graph_key": "object-reference",
                "graph_output": "validated-declared-object-references",
                "search_key": "object-reference",
                "search_output": (
                    "schema-declared-normalized-text-subject-to-disclosure"
                ),
                "exclusion": exclusion,
            }
        )
    if projection.get("object_mappings") != expected_object_rows:
        raise ContractError("unexpected Base v1 object mapping")

    event_kinds = [
        "receptor-resolution",
        "use-evidence",
        "derivation-evidence",
        "encoding-fidelity-attestation",
        "outcome-observation",
        "benefit-evidence",
        "observation",
    ]
    event_exclusions = [
        "retain-branches-use-frozen-resolution-reducer",
        "retrieval-or-exposure-is-not-use",
        "does-not-prove-benefit-truth-or-reward",
        "attestation-is-policy-and-frontier-relative",
        "observation-is-not-benefit-or-truth",
        "conflicts-coexist-no-score-authority",
        "private-context-never-public-projection",
    ]
    if source_event_rows != list(enumerate(event_kinds, start=1)):
        raise ContractError("Base v1 event mapping drifts from the schema registry")
    expected_event_rows = []
    for event_id, (kind, exclusion) in enumerate(
        zip(event_kinds, event_exclusions, strict=True), start=1
    ):
        expected_event_rows.append(
            {
                "id": event_id,
                "kind": kind,
                "mapping_id": f"base-v1/event/{event_id}-{kind}/1",
                "reducer_version": 1,
                "graph_key": "event-cid",
                "graph_output": "payload-object-and-causal-references",
                "search_key": "event-cid",
                "search_output": (
                    "schema-declared-normalized-text-subject-to-disclosure"
                ),
                "exclusion": exclusion,
            }
        )
    if projection.get("event_mappings") != expected_event_rows:
        raise ContractError("unexpected Base v1 event mapping")
    if set(projection) != set(expected_projection_header) | {
        "object_mappings",
        "event_mappings",
    }:
        raise ContractError("unexpected Base v1 projection fields")

    return len(expected_boundaries), len(expected_negative_oracles)


def validate_base_v1_archive() -> tuple[int, int]:
    try:
        profile = json.loads(read(BASE_V1_ARCHIVE_PROFILE))
    except json.JSONDecodeError as error:
        raise ContractError(f"invalid Base v1 archive JSON: {error}") from error
    if profile.get("format") != "onebrain/base-v1-archive/1":
        raise ContractError("unexpected Base v1 archive format")
    if profile.get("profile") != "OBARV002":
        raise ContractError("Base v1 archive must emit OBARV002")
    expected_domains = {
        "entry_id": "onebrain:base:archive-entry-id:1",
        "entry_root": "onebrain:base:archive-entry-root:1",
        "manifest": "onebrain:base:archive-manifest:1\\0",
        "high_water": "onebrain:base:archive-high-water:1\\0",
        "dataset": "onebrain:base:archive-dataset:1\\0",
    }
    if profile.get("domains") != expected_domains:
        raise ContractError("Base v1 archive domain drift")
    if profile.get("limits") != {
        "logical_key_bytes": 256,
        "manifest_entries": 1_000_000,
        "dataset_bytes": 16 * 1024 * 1024 * 1024,
    }:
        raise ContractError("Base v1 archive limit drift")
    owners = profile.get("owner_codes")
    if not isinstance(owners, dict) or list(owners.values()) != list(range(1, 23)):
        raise ContractError("Base v1 archive owner table drift")
    kinds = profile.get("entry_kinds")
    if not isinstance(kinds, list) or len(kinds) != 24 or len(set(kinds)) != 24:
        raise ContractError("Base v1 archive entry-kind set drift")
    required = profile.get("required_metadata")
    if required != [
        "AuthorityHighWater",
        "MigrationState",
        "InterpretationConfig",
        "RegistryHighWater",
        "SignerRecoveryPolicy",
    ]:
        raise ContractError("Base v1 archive required metadata drift")
    if profile.get("stable_entry_id_vector") != {
        "kind": "CanonicalObject",
        "owner": 1,
        "namespace": 1,
        "logical_key_utf8": "object-01",
        "entry_id_blake3_hex": "b83be45eda7ce7bcdbc3e6f9f0eeccfe4febdd7f471b4240c92046b73bf7210d",
    }:
        raise ContractError("Base v1 archive stable entry-ID vector drift")
    if profile.get("restore_gate") != [
        "canonical_schema_digest",
        "domain_registry_digest",
        "resource_registry_digest",
        "storage_schema_version",
        "archive_profile",
        "migration_profile",
    ]:
        raise ContractError("Base v1 archive portable restore gate drift")
    if profile.get("non_exportable_signer_restore") != "reprovision_required":
        raise ContractError("Base v1 signer restore policy drift")
    expected_base_operations = {
        "owner": "base_operations",
        "entry_kind": "BaseOperationRecord",
        "boundary": "TX-BASE-OPS-001",
        "states": [
            "reserved",
            "prepared",
            "confirming",
            "committed",
            "canceled",
            "failed",
            "unknown_outcome",
        ],
        "generation_bindings": ["process_generation", "dataset_generation"],
        "migration_bindings": [
            "vector_id",
            "vector_blake3",
            "trust_policy_digest",
        ],
        "restore_nonterminal": "unknown_outcome_reconcile_required_never_replay",
        "activation_receipt_plane": "non-switched-control-and-selected-generation",
        "authority_journal": {
            "tables": [
                "management_grant",
                "management_handle",
                "archive_capability",
                "signer_provision",
            ],
            "restart_disposition": "revoked_never_reactivated",
            "archive_disposition": "revoked_evidence_only",
        },
    }
    if profile.get("base_operation_records") != expected_base_operations:
        raise ContractError("Base operation archive contract drift")
    inventory = read(DR_M5_TRANSACTION_INVENTORY)
    if "| `TX-BASE-OPS-001` |" not in inventory:
        raise ContractError("Base operation transaction boundary is absent")
    operation_store = read(ROOT / "src/onebrain-node/src/base_operation_store.rs")
    runtime = read(ROOT / "src/onebrain-node/src/base_runtime.rs")
    for phase in [
        "before_begin_write",
        "after_begin_write_before_mutation",
        "after_mutation_before_commit",
        "after_commit_before_next_side_effect",
        "after_next_side_effect_before_ack",
    ]:
        if phase not in operation_store:
            raise ContractError(f"Base operation failpoint phase is absent: {phase}")
    for needle in [
        "ProcessGenerationLease::allocate",
        "reserve_operation",
        "begin_confirm",
        "mark_unknown",
        "register_authority",
        "transition_authority",
        "BaseAuthorityStateV1::Revoked",
        "register_signer_provision",
        "complete_signer_reprovision",
        "signer_request_matches",
        "import_activation_receipt",
        "restore_archive_for_base",
        "MigrationVectorBindingV1",
    ]:
        if needle not in operation_store and needle not in runtime:
            raise ContractError(f"Base runtime implementation evidence missing: {needle}")
    return len(kinds), len(required)


def _closed_discriminators(
    rows: object, namespace: str
) -> dict[int, dict[str, object]]:
    if not isinstance(rows, list) or not rows:
        raise ContractError(f"closed discriminator inventory is absent: {namespace}")
    by_id: dict[int, dict[str, object]] = {}
    names: set[str] = set()
    for row in rows:
        if not isinstance(row, dict):
            raise ContractError(f"invalid closed discriminator: {namespace}")
        identifier = row.get("id")
        name = row.get("name")
        if (
            not isinstance(identifier, int)
            or identifier <= 0
            or not isinstance(name, str)
            or not name
            or identifier in by_id
            or name in names
        ):
            raise ContractError(
                f"closed discriminator ID/name is missing, duplicated, or reused: {namespace}"
            )
        by_id[identifier] = row
        names.add(name)
    return by_id


def _runtime_history_root(entries: list[dict[str, object]]) -> str:
    digest = bytes(32)
    domain = b"onebrain/base-v1-runtime-interface-history/1\x00"
    for entry in entries:
        canonical = json.dumps(
            entry, sort_keys=True, separators=(",", ":"), ensure_ascii=False
        ).encode("utf-8")
        digest = hashlib.sha256(domain + digest + canonical).digest()
    return digest.hex()


def _runtime_live_discriminators(
    profile: dict[str, object],
) -> set[tuple[str, int, str]]:
    sources = {
        "request": profile.get("requests"),
        "response": profile.get("responses"),
        "error": profile.get("errors"),
        "command": profile.get("command_kinds"),
        "topic": profile.get("topic_kinds"),
        "operation": profile.get("operations"),
    }
    live: set[tuple[str, int, str]] = set()
    for namespace, rows in sources.items():
        for identifier, row in _closed_discriminators(rows, namespace).items():
            live.add((namespace, identifier, str(row["name"])))
    definitions = profile.get("type_definitions")
    if not isinstance(definitions, dict):
        raise ContractError("runtime type definition inventory is absent")
    for name, definition in definitions.items():
        if (
            isinstance(name, str)
            and isinstance(definition, dict)
            and definition.get("kind") == "enum"
            and isinstance(definition.get("variants"), list)
        ):
            namespace = f"type:{name}"
            for identifier, row in _closed_discriminators(
                definition["variants"], namespace
            ).items():
                live.add((namespace, identifier, str(row["name"])))
    return live


def _validate_runtime_baseline(
    profile: dict[str, object],
    history: dict[str, object],
    baseline_profile: dict[str, object],
    baseline_history: dict[str, object],
) -> None:
    current_entries = history.get("entries")
    baseline_entries = baseline_history.get("entries")
    if not isinstance(current_entries, list) or not isinstance(baseline_entries, list):
        raise ContractError("baseline history is absent")
    if current_entries[: len(baseline_entries)] != baseline_entries:
        raise ContractError("baseline history is not an immutable prefix")

    current_version = profile.get("profile_version")
    baseline_version = baseline_profile.get("profile_version")
    if not isinstance(current_version, dict) or not isinstance(baseline_version, dict):
        raise ContractError("baseline profile version is absent")
    if current_version.get("major") != baseline_version.get("major"):
        return
    current_minor = current_version.get("minor")
    baseline_minor = baseline_version.get("minor")
    if (
        not isinstance(current_minor, int)
        or not isinstance(baseline_minor, int)
        or current_minor < baseline_minor
    ):
        raise ContractError("Base runtime profile minor regressed")
    if len(current_entries) > len(baseline_entries) and current_minor <= baseline_minor:
        raise ContractError("additive discriminator history requires a profile minor bump")

    for field, namespace in (
        ("requests", "request"),
        ("responses", "response"),
        ("errors", "error"),
        ("command_kinds", "command"),
        ("topic_kinds", "topic"),
        ("operations", "operation"),
    ):
        current = _closed_discriminators(profile.get(field), namespace)
        baseline = _closed_discriminators(baseline_profile.get(field), namespace)
        for identifier, old_row in baseline.items():
            new_row = current.get(identifier)
            if new_row is None or new_row.get("name") != old_row.get("name"):
                raise ContractError(
                    f"breaking-major discriminator removal/retype: {namespace}/{identifier}"
                )
            if namespace == "operation" and new_row != old_row:
                raise ContractError(
                    f"breaking-major operation ownership/type change: {identifier}"
                )

    current_limits = profile.get("limits")
    baseline_limits = baseline_profile.get("limits")
    if not isinstance(current_limits, dict) or not isinstance(baseline_limits, dict):
        raise ContractError("baseline limit inventory is absent")
    for name, old_value in baseline_limits.items():
        new_value = current_limits.get(name)
        if isinstance(old_value, int) and (
            not isinstance(new_value, int) or new_value > old_value
        ):
            raise ContractError(f"bound widening is a breaking-major change: {name}")

    baseline_scalars = {
        row.get("name"): row
        for row in baseline_profile.get("scalar_types", [])
        if isinstance(row, dict)
    }
    current_scalars = {
        row.get("name"): row
        for row in profile.get("scalar_types", [])
        if isinstance(row, dict)
    }
    for name, old_row in baseline_scalars.items():
        new_row = current_scalars.get(name)
        if new_row is None or new_row.get("ownership") != old_row.get("ownership"):
            raise ContractError(f"breaking-major ownership change: {name}")
        for bound_name in ("exact_bytes", "max_bytes", "max_items"):
            old_bound = old_row.get(bound_name)
            new_bound = new_row.get(bound_name) if new_row is not None else None
            if isinstance(old_bound, int) and (
                not isinstance(new_bound, int)
                or bound_name == "exact_bytes"
                and new_bound != old_bound
                or bound_name != "exact_bytes"
                and new_bound > old_bound
            ):
                raise ContractError(
                    f"bound widening is a breaking-major change: {name}/{bound_name}"
                )

    baseline_definitions = baseline_profile.get("type_definitions")
    current_definitions = profile.get("type_definitions")
    if not isinstance(baseline_definitions, dict) or not isinstance(
        current_definitions, dict
    ):
        raise ContractError("baseline type definition inventory is absent")
    for name, old_definition in baseline_definitions.items():
        new_definition = current_definitions.get(name)
        if not isinstance(old_definition, dict) or not isinstance(
            new_definition, dict
        ) or new_definition.get("kind") != old_definition.get("kind"):
            raise ContractError(f"breaking-major type removal/retype: {name}")
        kind = old_definition.get("kind")
        if kind == "struct":
            old_fields = _closed_discriminators(
                old_definition.get("fields"), f"baseline/type/{name}/field"
            )
            new_fields = _closed_discriminators(
                new_definition.get("fields"), f"type/{name}/field"
            )
            for identifier, old_field in old_fields.items():
                new_field = new_fields.get(identifier)
                if (
                    new_field is None
                    or new_field.get("name") != old_field.get("name")
                    or new_field.get("type") != old_field.get("type")
                    or new_field.get("ownership") != old_field.get("ownership")
                    or old_field.get("required") is False
                    and new_field.get("required") is True
                ):
                    raise ContractError(
                        f"breaking-major field retype/optionality/ownership: {name}/{identifier}"
                    )
                for bound_name in ("exact_bytes", "max_bytes", "max_value"):
                    old_bound = old_field.get(bound_name)
                    new_bound = new_field.get(bound_name)
                    if isinstance(old_bound, int) and (
                        not isinstance(new_bound, int)
                        or bound_name == "exact_bytes"
                        and new_bound != old_bound
                        or bound_name != "exact_bytes"
                        and new_bound > old_bound
                    ):
                        raise ContractError(
                            f"bound widening is a breaking-major change: {name}/{identifier}/{bound_name}"
                        )
        elif kind == "enum" and "variants" in old_definition:
            old_variants = _closed_discriminators(
                old_definition.get("variants"), f"baseline/type/{name}"
            )
            new_variants = _closed_discriminators(
                new_definition.get("variants"), f"type/{name}"
            )
            for identifier, old_variant in old_variants.items():
                new_variant = new_variants.get(identifier)
                if (
                    new_variant is None
                    or new_variant.get("name") != old_variant.get("name")
                    or new_variant.get("payload") != old_variant.get("payload")
                ):
                    raise ContractError(
                        f"breaking-major enum removal/retype: {name}/{identifier}"
                    )
        elif kind in {"newtype", "opaque_registry_id"}:
            if new_definition.get("wire") != old_definition.get("wire") or new_definition.get(
                "ownership"
            ) != old_definition.get("ownership"):
                raise ContractError(f"breaking-major newtype/ownership change: {name}")
            for bound_name in ("exact_bytes", "max_bytes"):
                old_bound = old_definition.get(bound_name)
                new_bound = new_definition.get(bound_name)
                if isinstance(old_bound, int) and (
                    not isinstance(new_bound, int)
                    or bound_name == "exact_bytes"
                    and new_bound != old_bound
                    or bound_name != "exact_bytes"
                    and new_bound > old_bound
                ):
                    raise ContractError(
                        f"bound widening is a breaking-major change: {name}/{bound_name}"
                    )


def validate_base_v1_runtime_baseline_receipt(
    receipt: dict[str, object],
    *,
    resolved_commit: str,
    resolved_tree: str,
    baseline_idl_bytes: bytes,
    baseline_history_bytes: bytes,
    candidate_is_descendant: bool,
) -> tuple[dict[str, object], dict[str, object]]:
    required = {
        "format",
        "ref",
        "commit_sha1",
        "tree_sha1",
        "idl_sha256",
        "history_chain_root_sha256",
    }
    if set(receipt) != required or receipt.get("format") != (
        "onebrain/base-v1-idl-baseline-receipt/1"
    ) or receipt.get("ref") != "refs/heads/base-v1-idl-baseline":
        raise ContractError("invalid Base runtime baseline receipt")
    sha1 = re.compile(r"[0-9a-f]{40}")
    sha256 = re.compile(r"[0-9a-f]{64}")
    if not sha1.fullmatch(str(receipt.get("commit_sha1", ""))) or not sha1.fullmatch(
        str(receipt.get("tree_sha1", ""))
    ) or not sha256.fullmatch(str(receipt.get("idl_sha256", ""))) or not sha256.fullmatch(
        str(receipt.get("history_chain_root_sha256", ""))
    ):
        raise ContractError("invalid Base runtime baseline receipt digest")
    if receipt.get("commit_sha1") != resolved_commit:
        raise ContractError("Base runtime baseline ref moved from receipt commit")
    if receipt.get("tree_sha1") != resolved_tree:
        raise ContractError("Base runtime baseline tree digest mismatch")
    if not candidate_is_descendant:
        raise ContractError("Base runtime baseline is not a candidate ancestor")
    if receipt.get("idl_sha256") != hashlib.sha256(baseline_idl_bytes).hexdigest():
        raise ContractError("Base runtime baseline IDL digest mismatch")
    try:
        baseline_profile = json.loads(baseline_idl_bytes.decode("utf-8"))
        baseline_history = json.loads(baseline_history_bytes.decode("utf-8"))
    except (UnicodeDecodeError, json.JSONDecodeError) as error:
        raise ContractError(f"invalid Base runtime baseline payload: {error}") from error
    chain = baseline_history.get("history_chain")
    if not isinstance(chain, dict) or receipt.get(
        "history_chain_root_sha256"
    ) != chain.get("root_sha256") or not isinstance(
        baseline_history.get("entries"), list
    ) or chain.get("root_sha256") != _runtime_history_root(
        baseline_history["entries"]
    ):
        raise ContractError("Base runtime baseline history digest mismatch")
    return baseline_profile, baseline_history


def load_base_v1_runtime_baseline(
    receipt_path: Path,
    *,
    candidate_ref: str = "HEAD",
) -> tuple[dict[str, object], dict[str, object]]:
    try:
        receipt = json.loads(receipt_path.read_text(encoding="utf-8"))
    except (OSError, json.JSONDecodeError) as error:
        raise ContractError(f"cannot load Base runtime baseline receipt: {error}") from error

    def git_bytes(*arguments: str) -> bytes:
        try:
            return subprocess.run(
                ["git", *arguments],
                cwd=ROOT,
                check=True,
                capture_output=True,
            ).stdout
        except (OSError, subprocess.CalledProcessError) as error:
            raise ContractError(
                f"cannot load protected Base runtime baseline with git {' '.join(arguments)}"
            ) from error

    baseline_ref = "refs/heads/base-v1-idl-baseline"
    resolved_commit = git_bytes("rev-parse", f"{baseline_ref}^{{commit}}").decode(
        "ascii"
    ).strip()
    resolved_tree = git_bytes("rev-parse", f"{resolved_commit}^{{tree}}").decode(
        "ascii"
    ).strip()
    ancestor = subprocess.run(
        ["git", "merge-base", "--is-ancestor", resolved_commit, candidate_ref],
        cwd=ROOT,
        check=False,
        capture_output=True,
    ).returncode == 0
    idl_path = "src/test-vectors/vnext/base-v1-runtime-interface-v1.json"
    history_path = (
        "src/test-vectors/vnext/base-v1-runtime-interface-history-v1.json"
    )
    baseline_profile, baseline_history = validate_base_v1_runtime_baseline_receipt(
        receipt,
        resolved_commit=resolved_commit,
        resolved_tree=resolved_tree,
        baseline_idl_bytes=git_bytes("show", f"{resolved_commit}:{idl_path}"),
        baseline_history_bytes=git_bytes("show", f"{resolved_commit}:{history_path}"),
        candidate_is_descendant=ancestor,
    )
    return baseline_profile, baseline_history


def validate_base_v1_runtime_interface(
    profile: dict[str, object] | None = None,
    history: dict[str, object] | None = None,
    *,
    baseline_profile: dict[str, object] | None = None,
    baseline_history: dict[str, object] | None = None,
) -> tuple[int, int, int]:
    if profile is None:
        try:
            profile = json.loads(read(BASE_V1_RUNTIME_INTERFACE_PROFILE))
        except json.JSONDecodeError as error:
            raise ContractError(f"invalid Base runtime interface JSON: {error}") from error
    if history is None:
        try:
            history = json.loads(read(BASE_V1_RUNTIME_INTERFACE_HISTORY))
        except json.JSONDecodeError as error:
            raise ContractError(f"invalid Base runtime history JSON: {error}") from error

    if profile.get("format") != "onebrain/base-v1-runtime-interface/1":
        raise ContractError("unexpected Base runtime interface format")
    if profile.get("profile_id") != "BASE_V1_RUNTIME_INTERFACE_V1":
        raise ContractError("unexpected Base runtime interface ID")
    version = profile.get("profile_version")
    product_version = profile.get("product_api_profile")
    if (
        not isinstance(version, dict)
        or version.get("major") != 1
        or not isinstance(version.get("minor"), int)
        or not 0 <= version["minor"] <= 65535
    ):
        raise ContractError("unexpected Base runtime interface version")
    if product_version != {"major": 1, "minor": 1}:
        raise ContractError("Base product API minor was not bumped additively")

    baseline_contract = profile.get("baseline_contract")
    if baseline_contract != {
        "ref": "refs/heads/base-v1-idl-baseline",
        "receipt_format": "onebrain/base-v1-idl-baseline-receipt/1",
        "required_fields": [
            "format",
            "ref",
            "commit_sha1",
            "tree_sha1",
            "idl_sha256",
            "history_chain_root_sha256",
        ],
        "load": "git_show_exact_commit",
        "candidate_relation": "baseline_is_ancestor",
        "missing_or_moved": "fail_closed",
    }:
        raise ContractError("Base runtime protected baseline contract drift")

    if (baseline_profile is None) != (baseline_history is None):
        raise ContractError("baseline profile/history must be supplied together")
    if baseline_profile is not None and baseline_history is not None:
        _validate_runtime_baseline(
            profile, history, baseline_profile, baseline_history
        )

    limits = profile.get("limits")
    if not isinstance(limits, dict):
        raise ContractError("runtime payload bound inventory is absent")
    required_limits = {
        "max_payload_bytes": 1048576,
        "max_continuation_bytes": 4096,
        "max_continuation_encoded_chars": 5462,
        "max_string_bytes": 4096,
        "max_limitations": 64,
        "max_limitation_bytes": 128,
        "max_capabilities": 64,
        "max_resource_budget_dimensions": 16,
        "max_query_items": 256,
        "max_subscription_batch_items": 256,
        "max_event_payload_bytes": 65536,
        "max_archive_chunk_bytes": 1048576,
        "max_archive_total_bytes": 1099511627776,
        "max_management_scopes": 16,
        "max_active_operations": 1024,
    }
    if limits.get("max_payload_bytes") != required_limits["max_payload_bytes"]:
        raise ContractError("runtime payload bound drift")
    if any(limits.get(name) != value for name, value in required_limits.items()):
        if not isinstance(limits.get("max_continuation_bytes"), int) or limits.get(
            "max_continuation_bytes", 0
        ) <= 0:
            raise ContractError("opaque continuation bound drift")
        raise ContractError("runtime resource bound drift")

    scalar_types = profile.get("scalar_types")
    if not isinstance(scalar_types, list) or not scalar_types:
        raise ContractError("runtime scalar type inventory is absent")
    forbidden = {
        "raw_path",
        "runtime_handle",
        "store_handle",
        "private_key",
        "authority_implementation",
        "borrowed_reader",
        "borrowed_writer",
        "unbounded_string",
    }
    for scalar in scalar_types:
        if not isinstance(scalar, dict):
            raise ContractError("invalid runtime scalar type")
        wire = scalar.get("wire")
        if wire in forbidden:
            raise ContractError(f"forbidden exposure in machine IDL: {wire}")
        if isinstance(wire, str) and (
            "utf8" in wire
            or wire
            in {"ascii_token", "bounded_bytes", "opaque_bytes", "secret_bytes"}
        ):
            if not isinstance(scalar.get("max_bytes", scalar.get("exact_bytes")), int):
                raise ContractError("unbounded string/bytes type in machine IDL")

    required_common = {
        "profile_major",
        "profile_minor",
        "process_generation",
        "dataset_generation",
        "request_id",
        "operation_id",
        "idempotency_key",
        "lifecycle",
        "coverage",
        "limitations",
        "retryable",
        "resource_budget",
        "payload_discriminator",
        "compatibility_digest",
    }
    if set(profile.get("common_cross_projection_fields", [])) != required_common:
        raise ContractError("cross-projection common field inventory drift")

    definitions = profile.get("type_definitions")
    required_definition_names = {
        "BaseOperationId",
        "BaseOperationReservationId",
        "BaseIdempotencyKey",
        "BaseOpaqueContinuation",
        "BaseCommandV1",
        "ArchiveCredentialKindV1",
        "BoundedSecretIngressV1",
        "BaseManagementGrantV1",
        "BaseRequestV1",
        "BaseManagementRequestV1",
        "BaseErrorCodeV1",
    }
    if not isinstance(definitions, dict) or not required_definition_names <= set(
        definitions
    ):
        raise ContractError("generator-ready core type inventory drift")
    scalar_names = {
        row.get("name") for row in scalar_types if isinstance(row, dict)
    }
    permitted_references = set(definitions) | scalar_names | {
        "u8",
        "u16",
        "u32",
        "u64",
        "bool",
        "SecretBytes",
    }
    permitted_ownership = {
        "value",
        "owned",
        "service_handle",
        "management_handle",
        "host_principal",
        "zeroizing_one_way_ingress",
    }
    for name, definition in definitions.items():
        if not isinstance(name, str) or not isinstance(definition, dict):
            raise ContractError("invalid generator-ready type definition")
        kind = definition.get("kind")
        if kind == "struct":
            fields = _closed_discriminators(
                definition.get("fields"), f"type/{name}/field"
            )
            for field in fields.values():
                if field.get("type") not in permitted_references:
                    raise ContractError(
                        f"generator-ready type has unresolved reference: {name}"
                    )
                if field.get("required") not in {True, False} or field.get(
                    "ownership"
                ) not in permitted_ownership:
                    raise ContractError(
                        f"generator-ready field optionality/ownership drift: {name}"
                    )
        elif kind == "enum":
            if definition.get("closed") is not True:
                raise ContractError(f"generator-ready enum is not closed: {name}")
            if "variants_from" not in definition:
                variants = _closed_discriminators(
                    definition.get("variants"), f"type/{name}"
                )
                for variant in variants.values():
                    payload = variant.get("payload")
                    if payload is not None and payload not in permitted_references:
                        raise ContractError(
                            f"generator-ready enum has unresolved payload: {name}"
                        )
        elif kind in {"newtype", "opaque_registry_id"}:
            bound = definition.get("exact_bytes", definition.get("max_bytes"))
            if not isinstance(bound, int) or bound <= 0 or definition.get(
                "ownership"
            ) not in permitted_ownership:
                raise ContractError(f"generator-ready newtype is unbounded: {name}")
        else:
            raise ContractError(f"unsupported generator-ready type kind: {name}")
    for name in (
        "BaseOperationId",
        "BaseOperationReservationId",
        "BaseIdempotencyKey",
    ):
        if definitions.get(name) != {
            "kind": "newtype",
            "wire": "fixed_bytes",
            "exact_bytes": 32,
            "ownership": "owned",
        }:
            raise ContractError(f"generator-ready fixed ID type drift: {name}")
    if definitions.get("BaseOpaqueContinuation") != {
        "kind": "newtype",
        "wire": "bounded_bytes",
        "max_bytes": 4096,
        "ownership": "owned",
        "constructor": "private_checked",
    }:
        raise ContractError("generator-ready private continuation type drift")
    command_definition = definitions.get("BaseCommandV1")
    credential_definition = definitions.get("ArchiveCredentialKindV1")
    request_definition = definitions.get("BaseRequestV1")
    management_request_definition = definitions.get("BaseManagementRequestV1")
    for definition, name in (
        (command_definition, "BaseCommandV1"),
        (credential_definition, "ArchiveCredentialKindV1"),
        (request_definition, "BaseRequestV1"),
        (management_request_definition, "BaseManagementRequestV1"),
    ):
        if (
            not isinstance(definition, dict)
            or definition.get("kind") != "enum"
            or definition.get("closed") is not True
        ):
            raise ContractError(f"generator-ready closed enum drift: {name}")
        _closed_discriminators(definition.get("variants"), f"type/{name}")
    command_variants = _closed_discriminators(
        command_definition.get("variants"), "type/BaseCommandV1"
    )
    if {
        identifier: (row.get("name"), row.get("payload"))
        for identifier, row in command_variants.items()
    } != {
        1: ("ExistingLocalCommand", "BaseLocalCommandV1"),
        2: ("CreateArchive", "CreateArchiveCommandV1"),
        3: ("RestoreArchive", "RestoreArchiveCommandV1"),
    }:
        raise ContractError("generator-ready Base command variants drift")
    credential_variants = _closed_discriminators(
        credential_definition.get("variants"), "type/ArchiveCredentialKindV1"
    )
    if {identifier: row.get("name") for identifier, row in credential_variants.items()} != {
        1: "Password",
        2: "RecoveryKey",
    }:
        raise ContractError("generator-ready archive credential variants drift")
    secret_definition = definitions.get("BoundedSecretIngressV1")
    if not isinstance(secret_definition, dict) or secret_definition.get(
        "response_serializable"
    ) is not False or secret_definition.get("loggable") is not False:
        raise ContractError("generator-ready secret ingress firewall drift")
    secret_fields = _closed_discriminators(
        secret_definition.get("fields"), "type/BoundedSecretIngressV1/field"
    )
    if secret_fields.get(2, {}).get("max_bytes") != 1024 or secret_fields.get(
        2, {}
    ).get("ownership") != "zeroizing_one_way_ingress":
        raise ContractError("generator-ready secret ingress bound/custody drift")
    if definitions.get("BaseManagementGrantV1") != {
        "kind": "opaque_registry_id",
        "exact_bytes": 32,
        "ownership": "host_principal",
        "constructible_by_service": False,
    }:
        raise ContractError("generator-ready management grant type drift")
    expected_request_variants = {
        3: "Status",
        5: "Query",
        6: "ReserveOperation",
        7: "Prepare",
        8: "Confirm",
        9: "Cancel",
        10: "Reconcile",
        11: "Subscribe",
        12: "PollEvents",
        13: "CloseSubscription",
        14: "Drain",
        15: "Close",
    }
    if {
        identifier: row.get("name")
        for identifier, row in _closed_discriminators(
            request_definition.get("variants"), "type/BaseRequestV1"
        ).items()
    } != expected_request_variants:
        raise ContractError("generator-ready Base request variants drift")
    expected_management_variants = {
        102: "ArchiveSourceBegin",
        103: "ArchiveSourcePush",
        104: "ArchiveSourceSeal",
        105: "ArchiveSinkBegin",
        106: "ArchiveSinkRead",
        107: "ArchiveSinkCommit",
        108: "ArchiveSecretRegister",
        109: "ArchiveCapabilityAbort",
        110: "ArchiveCapabilityDestroy",
        111: "CompleteSignerReprovision",
        112: "Close",
    }
    if {
        identifier: row.get("name")
        for identifier, row in _closed_discriminators(
            management_request_definition.get("variants"),
            "type/BaseManagementRequestV1",
        ).items()
    } != expected_management_variants:
        raise ContractError("generator-ready Base management request variants drift")
    if definitions.get("BaseErrorCodeV1") != {
        "kind": "enum",
        "repr": "u16",
        "closed": True,
        "variants_from": "errors",
    }:
        raise ContractError("generator-ready Base error declaration drift")

    requests = _closed_discriminators(profile.get("requests"), "request")
    responses = _closed_discriminators(profile.get("responses"), "response")
    errors = _closed_discriminators(profile.get("errors"), "error")
    commands = _closed_discriminators(profile.get("command_kinds"), "command")
    topics = _closed_discriminators(profile.get("topic_kinds"), "topic")
    operations = _closed_discriminators(profile.get("operations"), "operation")

    expected_operations = {
        1: "open",
        2: "negotiate",
        3: "status",
        4: "snapshot",
        5: "query",
        6: "reserve_operation",
        7: "prepare",
        8: "confirm",
        9: "cancel",
        10: "reconcile",
        11: "subscribe",
        12: "poll_events",
        13: "close_subscription",
        14: "drain",
        15: "close",
        101: "management.open",
        102: "management.archive_source_begin",
        103: "management.archive_source_push_chunk",
        104: "management.archive_source_seal",
        105: "management.archive_sink_begin",
        106: "management.archive_sink_read_chunk",
        107: "management.archive_sink_commit",
        108: "management.archive_secret_register",
        109: "management.archive_capability_abort",
        110: "management.archive_capability_destroy",
        111: "management.complete_signer_reprovision",
        112: "management.close",
    }
    if {identifier: row["name"] for identifier, row in operations.items()} != expected_operations:
        raise ContractError("Base runtime operation inventory drift")
    if set(requests) != set(operations) or set(responses) != set(operations):
        raise ContractError("request/response discriminator inventory drift")
    for identifier, operation in operations.items():
        if operation.get("request_id") != identifier or operation.get(
            "response_id"
        ) != identifier:
            raise ContractError("operation request/response projection drift")

    if {row["name"] for row in commands.values()} != {
        "ExistingLocalCommand",
        "CreateArchive",
        "RestoreArchive",
    }:
        raise ContractError("required archive command discriminator is absent")
    if {row["name"] for row in topics.values()} != {
        "RuntimeStatus",
        "OperationReceipts",
        "QueryResults",
        "ArchiveProgress",
        "Compatibility",
    }:
        raise ContractError("subscription topic vocabulary drift")
    expected_errors = {
        1: "InvalidRequest",
        2: "NotFound",
        3: "Conflict",
        4: "Expired",
        5: "RateLimited",
        6: "CapabilityDisabled",
        7: "DependencyUnavailable",
        8: "IncompatibleProfile",
        9: "ResourceExhausted",
        10: "CorruptState",
        11: "ReprovisionRequired",
        12: "UnknownOutcome",
        13: "InternalError",
    }
    if {identifier: row["name"] for identifier, row in errors.items()} != expected_errors:
        raise ContractError("closed discriminator error inventory drift")
    for error in errors.values():
        if error.get("retryable") is True and error.get(
            "reconcile_before_retry"
        ) is not True:
            raise ContractError("retryable error lacks reconcile requirement")

    protocol = profile.get("operation_protocol")
    if not isinstance(protocol, dict):
        raise ContractError("operation protocol is absent")
    if protocol.get("durable_order") != [
        "reserve_operation",
        "management_capability_registration",
        "prepare",
        "confirm_or_cancel",
        "reconcile",
    ] or protocol.get("prepare_requires_reservation") is not True:
        raise ContractError("durable reserve-before-capability flow drift")
    if protocol.get("confirm_requires_idempotency_key") is not True:
        raise ContractError("confirm idempotency requirement drift")
    if protocol.get("retry_requires_reconcile") is not True or protocol.get(
        "unknown_outcome_requires_reconcile"
    ) is not True:
        raise ContractError("retry must reconcile unknown outcome")
    if set(protocol.get("states", [])) != {
        "reserved",
        "prepared",
        "confirming",
        "committed",
        "canceled",
        "failed",
        "unknown_outcome",
    } or set(protocol.get("transitions", [])) != {
        "reserved->prepared",
        "reserved->canceled",
        "prepared->confirming",
        "prepared->canceled",
        "confirming->committed",
        "confirming->failed",
        "confirming->unknown_outcome",
        "unknown_outcome->committed",
        "unknown_outcome->failed",
    }:
        raise ContractError("asynchronous operation state machine drift")

    generation = profile.get("generation_fence")
    if not isinstance(generation, dict) or set(
        generation.get("required_fields", [])
    ) != {"process_generation", "dataset_generation"} or generation.get(
        "checked_on_every_operation"
    ) is not True:
        raise ContractError("process/dataset generation fence drift")

    subscriptions = profile.get("subscriptions")
    if not isinstance(subscriptions, dict):
        raise ContractError("subscription contract is absent")
    if set(subscriptions.get("required_operations", [])) != {
        "subscribe",
        "poll_events",
        "close_subscription",
    } or subscriptions.get("handle_ownership") != "owned_by_service_session":
        raise ContractError("subscription ownership/close contract drift")
    if subscriptions.get("max_batch_items") != 256 or subscriptions.get(
        "max_event_payload_bytes"
    ) != 65536:
        raise ContractError("bounded subscription batch drift")
    if subscriptions.get("cursor_rule") != "strictly_monotonic_non_regressing":
        raise ContractError("subscription cursor regression rule drift")
    if subscriptions.get("gap_response") != (
        "typed_resync_required_with_earliest_available_cursor"
    ):
        raise ContractError("retention gap lacks explicit resync response")
    if subscriptions.get("slow_consumer") != (
        "bounded_buffer_then_typed_disconnect_with_resync"
    ):
        raise ContractError("slow-consumer backpressure behavior drift")

    management = profile.get("management")
    if not isinstance(management, dict):
        raise ContractError("management contract is absent")
    required_grant_bindings = {
        "principal_id",
        "exact_scopes",
        "process_generation",
        "dataset_generation",
        "expires_at",
        "revocation_epoch",
    }
    if set(management.get("grant_bindings", [])) != required_grant_bindings or management.get(
        "open_consumes_grant"
    ) is not True or management.get("ordinary_service_can_mint_grant") is not False:
        raise ContractError("management grant principal/scope binding drift")
    required_management = {
        name for name in expected_operations.values() if name.startswith("management.")
    }
    if set(management.get("required_operations", [])) != required_management:
        if "management.complete_signer_reprovision" not in management.get(
            "required_operations", []
        ):
            raise ContractError("management reprovision lifecycle drift")
        raise ContractError("management operation lifecycle drift")

    archive = profile.get("archive_capabilities")
    if not isinstance(archive, dict):
        raise ContractError("archive capability contract is absent")
    if set(archive.get("ownership_binding", [])) != {
        "management_handle",
        "principal_id",
        "operation_id",
        "process_generation",
        "dataset_generation",
        "capability_kind",
    }:
        raise ContractError("archive capability ownership is ambiguous")
    if archive.get("max_chunk_bytes") != 1048576 or archive.get(
        "max_total_bytes"
    ) != 1099511627776:
        raise ContractError("bounded archive chunk/total drift")
    required_archive_lifecycle = {
        "management.archive_source_begin",
        "management.archive_source_push_chunk",
        "management.archive_source_seal",
        "management.archive_sink_begin",
        "management.archive_sink_read_chunk",
        "management.archive_sink_commit",
        "management.archive_secret_register",
        "management.archive_capability_abort",
        "management.archive_capability_destroy",
    }
    if set(archive.get("lifecycle_operations", [])) != required_archive_lifecycle:
        raise ContractError("archive lifecycle register/seal/commit/abort/destroy drift")
    if (
        archive.get("opaque_handles_only") is not True
        or archive.get("reserve_before_registration") is not True
        or archive.get("secret_custody") != "zeroizing_non_exportable"
        or archive.get("terminal_reuse") != "reject"
        or archive.get("drop_behavior") != "abort_then_destroy"
        or set(archive.get("source_states", []))
        != {"registered", "streaming", "sealed", "consumed", "aborted", "destroyed"}
        or set(archive.get("sink_states", []))
        != {"registered", "streaming", "committed", "aborted", "destroyed"}
        or set(archive.get("secret_states", []))
        != {"registered", "consumed", "aborted", "destroyed"}
    ):
        raise ContractError("archive capability state/custody drift")

    lifecycle = profile.get("runtime_lifecycle")
    if not isinstance(lifecycle, dict) or lifecycle.get(
        "drain_blocks_new_operations"
    ) is not True or lifecycle.get("drain_preserves_poll_cancel_reconcile") is not True:
        raise ContractError("runtime drain behavior drift")
    if lifecycle.get("close_requires_drain") is not True or lifecycle.get(
        "management_close_is_explicit"
    ) is not True:
        raise ContractError("runtime close authority drift")

    projections = profile.get("projection_rules")
    if not isinstance(projections, dict) or projections.get("source") != (
        "machine_idl_only"
    ) or projections.get("generated") is not True or projections.get(
        "handwritten_declarations_allowed"
    ) is not False:
        raise ContractError("projection source must be generated machine IDL only")
    targets = projections.get("targets")
    if not isinstance(targets, list):
        raise ContractError("projection target inventory is absent")
    targets_by_name = {
        row.get("name"): row for row in targets if isinstance(row, dict)
    }
    if set(targets_by_name) != {"rust", "typescript", "dart", "c_abi"}:
        raise ContractError("projection target inventory drift")
    if targets_by_name["c_abi"].get("struct_size_required") is not True:
        raise ContractError("C ABI structs require struct_size")
    mappings = projections.get("operation_mapping")
    if not isinstance(mappings, list) or {
        row.get("operation") for row in mappings if isinstance(row, dict)
    } != set(expected_operations.values()):
        raise ContractError("exact operation projection mapping drift")
    if set(projections.get("forbidden_exposures", [])) != forbidden:
        raise ContractError("forbidden exposure inventory drift")
    mapping_by_operation = {
        row.get("operation"): row for row in mappings if isinstance(row, dict)
    }

    def camel(name: str) -> str:
        head, *tail = name.split("_")
        return head + "".join(part[:1].upper() + part[1:] for part in tail)

    for operation_name, row in mapping_by_operation.items():
        if not isinstance(operation_name, str):
            raise ContractError("invalid operation projection mapping")
        if operation_name in {"management.open", "management.close"}:
            suffix = operation_name.replace(".", "_")
        else:
            suffix = operation_name.removeprefix("management.")
        rust_name = suffix
        expected_mapping = {
            "operation": operation_name,
            "rust": rust_name,
            "typescript": camel(rust_name),
            "dart": camel(rust_name),
            "c_abi": f"ob_base_{rust_name}_v1",
        }
        if row != expected_mapping:
            raise ContractError(f"exact projection name drift: {operation_name}")

    if history.get("format") != "onebrain/base-v1-runtime-interface-history/1" or history.get(
        "profile_id"
    ) != "BASE_V1_RUNTIME_INTERFACE_V1" or history.get("append_only") is not True:
        raise ContractError("unexpected runtime discriminator history format")
    entries = history.get("entries")
    if not isinstance(entries, list) or not entries:
        raise ContractError("runtime discriminator history is absent")
    seen_ids: dict[tuple[str, int], str] = {}
    seen_names: dict[tuple[str, str], int] = {}
    active: set[tuple[str, int, str]] = set()
    for expected_sequence, entry in enumerate(entries, start=1):
        if not isinstance(entry, dict) or entry.get("sequence") != expected_sequence:
            raise ContractError("runtime history sequence is not append-only")
        namespace = entry.get("namespace")
        identifier = entry.get("id")
        name = entry.get("name")
        state = entry.get("state")
        if (
            not (
                namespace
                in {"request", "response", "error", "command", "topic", "operation"}
                or isinstance(namespace, str)
                and namespace.startswith("type:")
            )
            or not isinstance(identifier, int)
            or identifier <= 0
            or not isinstance(name, str)
            or state not in {"active", "tombstone"}
        ):
            raise ContractError("invalid runtime history entry")
        id_key = (namespace, identifier)
        name_key = (namespace, name)
        prior_name = seen_ids.get(id_key)
        prior_id = seen_names.get(name_key)
        if (prior_name is not None and prior_name != name) or (
            prior_id is not None and prior_id != identifier
        ):
            raise ContractError("runtime history discriminator reuse detected")
        if prior_name is not None and state != "tombstone":
            raise ContractError("runtime history active discriminator was rewritten")
        seen_ids[id_key] = name
        seen_names[name_key] = identifier
        key = (str(namespace), identifier, name)
        if state == "active":
            active.add(key)
        else:
            active.discard(key)
    if active != _runtime_live_discriminators(profile):
        raise ContractError("runtime discriminator history coverage drift")
    chain = history.get("history_chain")
    if not isinstance(chain, dict) or chain.get("algorithm") != "sha256-chain-v1" or chain.get(
        "canonicalization"
    ) != "json-sort-keys-utf8-no-whitespace" or chain.get(
        "root_sha256"
    ) != _runtime_history_root(entries):
        raise ContractError("runtime discriminator history chain root mismatch")

    return len(operations), len(topics), len(errors)


def validate_base_v1_compatibility(
    profile: dict[str, object] | None = None,
    runtime_profile: dict[str, object] | None = None,
) -> int:
    if profile is None:
        try:
            profile = json.loads(read(BASE_V1_COMPATIBILITY_PROFILE))
        except json.JSONDecodeError as error:
            raise ContractError(f"invalid Base compatibility JSON: {error}") from error
    if profile.get("format") != "onebrain/base-v1-compatibility/1" or profile.get(
        "profile_id"
    ) != "BASE_V1_COMPATIBILITY_V1":
        raise ContractError("unexpected Base compatibility profile")
    if profile.get("domains") != {
        "candidate_semantic": "onebrain:base:candidate-semantic:1\\0",
        "artifact_tuple": "onebrain:base:artifact-tuple:1\\0",
    }:
        raise ContractError("Base compatibility digest domain drift")
    if profile.get("identity_inputs") != {
        "source_commit": "ONEBRAIN_BASE_COMMIT",
        "toolchain_digest": "ONEBRAIN_TOOLCHAIN_DIGEST",
        "missing_or_malformed": "typed_unknown",
    }:
        raise ContractError("Base compatibility build identity contract drift")

    tuple_fields = [
        "base_version",
        "base_commit",
        "canonical_schema_digest",
        "domain_registry_digest",
        "resource_registry_digest",
        "storage_schema",
        "archive_profile",
        "migration_profile",
        "registry_profile",
        "registry_profile_digest",
        "wire_session",
        "product_api",
        "c_abi",
        "feature_set_digest",
        "target_triple",
        "toolchain",
    ]
    candidate_fields = tuple_fields[:-2]
    if profile.get("tuple_fields") != tuple_fields or profile.get(
        "candidate_fields"
    ) != candidate_fields or profile.get("artifact_only_fields") != [
        "target_triple",
        "toolchain",
    ]:
        raise ContractError("Base compatibility tuple field order drift")

    if runtime_profile is None:
        try:
            runtime_profile = json.loads(read(BASE_V1_RUNTIME_INTERFACE_PROFILE))
        except json.JSONDecodeError as error:
            raise ContractError(f"invalid Base runtime interface JSON: {error}") from error
    runtime = runtime_profile
    runtime_compatibility = runtime.get("compatibility")
    if not isinstance(runtime_compatibility, dict) or runtime_compatibility.get(
        "candidate_fields"
    ) != candidate_fields or runtime_compatibility.get("artifact_only_fields") != [
        "target_triple",
        "toolchain",
    ] or runtime_compatibility.get(
        "qualification_participates_in_digest"
    ) is not False or runtime_compatibility.get(
        "qualification_is_external"
    ) is not True:
        raise ContractError("Base compatibility IDL binding drift")

    scalar_rows = runtime.get("scalar_types")
    if not isinstance(scalar_rows, list):
        raise ContractError("Base compatibility scalar declarations are absent")
    scalars = {
        row.get("name"): row for row in scalar_rows if isinstance(row, dict)
    }
    expected_scalars = {
        "BasePrerelease": {
            "name": "BasePrerelease",
            "wire": "ascii_token",
            "max_bytes": 32,
            "ownership": "owned",
        },
        "TargetTriple": {
            "name": "TargetTriple",
            "wire": "ascii_token",
            "max_bytes": 96,
            "ownership": "owned",
        },
        "MigrationVectorIdV1": {
            "name": "MigrationVectorIdV1",
            "wire": "ascii_token",
            "max_bytes": 64,
            "ownership": "owned",
        },
        "BaseCapabilitySet": {
            "name": "BaseCapabilitySet",
            "wire": "bounded_set",
            "max_items": 64,
            "ownership": "owned",
        },
        "StorageSchemaVersion": {
            "name": "StorageSchemaVersion",
            "wire": "u32",
            "ownership": "value",
        },
    }
    if any(scalars.get(name) != row for name, row in expected_scalars.items()):
        raise ContractError("Base compatibility scalar declaration drift")

    definitions = runtime.get("type_definitions")
    if not isinstance(definitions, dict):
        raise ContractError("Base compatibility type declarations are absent")

    def fields(name: str) -> list[tuple[object, ...]]:
        definition = definitions.get(name)
        if not isinstance(definition, dict) or definition.get("kind") != "struct":
            raise ContractError(f"Base compatibility struct declaration drift: {name}")
        rows = definition.get("fields")
        if not isinstance(rows, list):
            raise ContractError(f"Base compatibility field declaration drift: {name}")
        return [
            (
                row.get("id"),
                row.get("name"),
                row.get("type"),
                row.get("required"),
                row.get("ownership"),
                row.get("max_value"),
            )
            for row in rows
            if isinstance(row, dict)
        ]

    expected_struct_fields = {
        "ProfileVersion": [
            (1, "major", "u16", True, "value", None),
            (2, "minor", "u16", True, "value", None),
        ],
        "BaseReleaseVersion": [
            (1, "major", "u16", True, "value", None),
            (2, "minor", "u16", True, "value", None),
            (3, "patch", "u16", True, "value", None),
            (4, "prerelease", "BasePrerelease", False, "owned", None),
        ],
        "BaseQualifiedEvidence": [
            (1, "candidate_commit", "SourceCommitId", True, "owned", None),
            (
                2,
                "candidate_semantic_digest",
                "CompatibilityDigestV1",
                True,
                "owned",
                None,
            ),
            (3, "evidence_blake3", "CompatibilityDigestV1", True, "owned", None),
        ],
        "ArchiveRestorePolicyV1": [
            (
                1,
                "canonical_schema_digest",
                "CompatibilityDigestV1",
                True,
                "owned",
                None,
            ),
            (
                2,
                "domain_registry_digest",
                "CompatibilityDigestV1",
                True,
                "owned",
                None,
            ),
            (
                3,
                "resource_registry_digest",
                "CompatibilityDigestV1",
                True,
                "owned",
                None,
            ),
            (4, "storage_schema", "StorageSchemaVersion", True, "value", None),
            (5, "archive_profile", "ProfileVersion", True, "value", None),
            (6, "migration_profile", "ProfileVersion", True, "value", None),
            (7, "max_dataset_bytes", "u64", True, "value", 17_179_869_184),
        ],
        "BaseCompatibilityTuple": [
            (1, "base_version", "BaseReleaseVersion", True, "owned", None),
            (2, "base_commit", "SourceCommitIdentity", True, "owned", None),
            (
                3,
                "canonical_schema_digest",
                "CompatibilityDigestV1",
                True,
                "owned",
                None,
            ),
            (
                4,
                "domain_registry_digest",
                "CompatibilityDigestV1",
                True,
                "owned",
                None,
            ),
            (
                5,
                "resource_registry_digest",
                "CompatibilityDigestV1",
                True,
                "owned",
                None,
            ),
            (6, "storage_schema", "StorageSchemaVersion", True, "value", None),
            (7, "archive_profile", "ProfileVersion", True, "value", None),
            (8, "migration_profile", "ProfileVersion", True, "value", None),
            (9, "registry_profile", "ProfileVersion", True, "value", None),
            (
                10,
                "registry_profile_digest",
                "CompatibilityDigestV1",
                True,
                "owned",
                None,
            ),
            (11, "wire_session", "ProfileVersion", True, "value", None),
            (12, "product_api", "ProfileVersion", True, "value", None),
            (13, "c_abi", "ProfileVersion", True, "value", None),
            (
                14,
                "feature_set_digest",
                "CompatibilityDigestV1",
                True,
                "owned",
                None,
            ),
            (15, "target_triple", "TargetTriple", True, "owned", None),
            (16, "toolchain", "ToolchainIdentity", True, "owned", None),
        ],
        "BaseVersionStatus": [
            (1, "compatibility", "BaseCompatibilityTuple", True, "owned", None),
            (
                2,
                "candidate_semantic_digest",
                "CompatibilityDigestV1",
                True,
                "owned",
                None,
            ),
            (
                3,
                "artifact_tuple_digest",
                "CompatibilityDigestV1",
                True,
                "owned",
                None,
            ),
            (4, "qualification", "BaseQualificationState", True, "owned", None),
        ],
        "NegotiatedVersions": [
            (1, "base_minor", "u16", True, "value", None),
            (2, "wire_session_minor", "u16", True, "value", None),
            (3, "product_api_minor", "u16", True, "value", None),
            (4, "c_abi_minor", "u16", True, "value", None),
        ],
        "MigrationVectorBindingV1": [
            (1, "vector_id", "MigrationVectorIdV1", True, "owned", None),
            (2, "vector_blake3", "CompatibilityDigestV1", True, "owned", None),
            (
                3,
                "trust_policy_digest",
                "CompatibilityDigestV1",
                True,
                "owned",
                None,
            ),
        ],
        "BaseCompatibilityPolicy": [
            (1, "current", "BaseCompatibilityTuple", True, "owned", None),
            (2, "minimum_additive", "NegotiatedVersions", True, "owned", None),
            (3, "archive_restore", "ArchiveRestorePolicyV1", True, "owned", None),
        ],
        "BaseCapabilityRequirements": [
            (1, "supported", "BaseCapabilitySet", True, "owned", None),
            (2, "required", "BaseCapabilitySet", True, "owned", None),
        ],
        "BaseCompatibleNegotiationV1": [
            (1, "versions", "NegotiatedVersions", True, "owned", None),
            (2, "capabilities", "BaseCapabilitySet", True, "owned", None),
        ],
        "BaseMigrationRequiredNegotiationV1": [
            (1, "from", "BaseReleaseVersion", True, "owned", None),
            (2, "to", "BaseReleaseVersion", True, "owned", None),
            (3, "vector", "MigrationVectorBindingV1", True, "owned", None),
        ],
    }
    for name, expected in expected_struct_fields.items():
        if fields(name) != expected:
            raise ContractError(f"Base compatibility field declaration drift: {name}")

    for name, expected in {
        "SourceCommitSha1": {
            "kind": "newtype",
            "wire": "fixed_bytes",
            "exact_bytes": 20,
            "ownership": "owned",
        },
        "SourceCommitSha256": {
            "kind": "newtype",
            "wire": "fixed_bytes",
            "exact_bytes": 32,
            "ownership": "owned",
        },
        "ToolchainDigest": {
            "kind": "newtype",
            "wire": "fixed_bytes",
            "exact_bytes": 32,
            "ownership": "owned",
        },
    }.items():
        if definitions.get(name) != expected:
            raise ContractError(f"Base compatibility identity declaration drift: {name}")

    expected_enums = {
        "SourceCommitId": (
            "u8",
            [(1, "Sha1", "SourceCommitSha1"), (2, "Sha256", "SourceCommitSha256")],
        ),
        "SourceCommitIdentity": (
            "u8",
            [(1, "Known", "SourceCommitId"), (2, "Unknown", None)],
        ),
        "ToolchainIdentity": (
            "u8",
            [(1, "Known", "ToolchainDigest"), (2, "Unknown", None)],
        ),
        "BaseQualificationState": (
            "u8",
            [
                (1, "Unqualified", None),
                (2, "Qualified", "BaseQualifiedEvidence"),
            ],
        ),
        "BaseNegotiationOutcome": (
            "u8",
            [
                (1, "Compatible", "BaseCompatibleNegotiationV1"),
                (2, "MigrationRequired", "BaseMigrationRequiredNegotiationV1"),
                (3, "Incompatible", "BaseCompatibilityError"),
            ],
        ),
    }
    for name, (expected_repr, expected) in expected_enums.items():
        definition = definitions.get(name)
        variants = definition.get("variants") if isinstance(definition, dict) else None
        actual = [
            (row.get("id"), row.get("name"), row.get("payload"))
            for row in variants
            if isinstance(row, dict)
        ] if isinstance(variants, list) else []
        if not isinstance(definition, dict) or definition.get(
            "kind"
        ) != "enum" or definition.get("closed") is not True or definition.get(
            "repr"
        ) != expected_repr or actual != expected:
            raise ContractError(f"Base compatibility enum declaration drift: {name}")
    error_definition = definitions.get("BaseCompatibilityError")
    error_variants = (
        error_definition.get("variants")
        if isinstance(error_definition, dict)
        else None
    )
    expected_error_names = [
        "BaseMajorMismatch",
        "BaseMinorBelowMinimum",
        "CanonicalSchemaMismatch",
        "DomainRegistryMismatch",
        "ResourceRegistryMismatch",
        "RegistryProfileMismatch",
        "RegistryProfileDigestMismatch",
        "WireSessionMajorMismatch",
        "WireSessionMinorBelowMinimum",
        "ProductApiMajorMismatch",
        "ProductApiMinorBelowMinimum",
        "CAbiMajorMismatch",
        "CAbiMinorBelowMinimum",
        "MigrationVectorRequired",
        "MissingRequiredCapability",
        "InvalidPolicy",
    ]
    if not isinstance(error_definition, dict) or error_definition.get(
        "kind"
    ) != "enum" or error_definition.get("closed") is not True or error_definition.get(
        "repr"
    ) != "u16" or not isinstance(error_variants, list) or [
        (row.get("id"), row.get("name"), row.get("payload"))
        for row in error_variants
        if isinstance(row, dict)
    ] != [
        (identifier, name, None)
        for identifier, name in enumerate(expected_error_names, start=1)
    ]:
        raise ContractError("Base compatibility error declaration drift")

    baseline = profile.get("baseline")
    if not isinstance(baseline, dict) or list(baseline) != tuple_fields:
        raise ContractError("Base compatibility baseline tuple drift")
    release = baseline.get("base_version")
    if not isinstance(release, dict) or set(release) != {
        "major",
        "minor",
        "patch",
        "prerelease",
    } or any(
        not isinstance(release.get(field), int)
        for field in ("major", "minor", "patch")
    ):
        raise ContractError("Base compatibility release version drift")

    digest_fields = {
        "canonical_schema_digest",
        "domain_registry_digest",
        "resource_registry_digest",
        "registry_profile_digest",
        "feature_set_digest",
    }
    digest_pattern = re.compile(r"[0-9a-f]{64}")
    for field in digest_fields:
        if not digest_pattern.fullmatch(str(baseline.get(field, ""))):
            raise ContractError(f"Base compatibility digest field drift: {field}")
    commit = baseline.get("base_commit")
    toolchain = baseline.get("toolchain")
    if not isinstance(commit, dict) or commit.get("kind") != "sha1" or not re.fullmatch(
        r"[0-9a-f]{40}", str(commit.get("hex", ""))
    ) or not isinstance(toolchain, dict) or toolchain.get(
        "kind"
    ) != "known" or not digest_pattern.fullmatch(str(toolchain.get("hex", ""))):
        raise ContractError("Base compatibility known identity vector drift")
    if not isinstance(baseline.get("storage_schema"), int) or not isinstance(
        baseline.get("target_triple"), str
    ) or not 0 < len(baseline["target_triple"].encode("ascii")) <= 96:
        raise ContractError("Base compatibility storage/target vector drift")
    for field in ("archive_profile", "migration_profile", "registry_profile", "wire_session", "product_api", "c_abi"):
        value = baseline.get(field)
        if not isinstance(value, dict) or set(value) != {"major", "minor"} or any(
            not isinstance(value.get(part), int) for part in ("major", "minor")
        ):
            raise ContractError(f"Base compatibility profile version drift: {field}")

    minimum = profile.get("minimum_additive")
    if not isinstance(minimum, dict) or set(minimum) != {
        "base_minor",
        "wire_session_minor",
        "product_api_minor",
        "c_abi_minor",
    } or any(not isinstance(value, int) for value in minimum.values()):
        raise ContractError("Base compatibility independent minor floors drift")
    archive_restore = profile.get("archive_restore")
    if archive_restore != {"max_dataset_bytes": 17_179_869_184}:
        raise ContractError("Base compatibility archive limit drift")

    capabilities = profile.get("capabilities")
    if not isinstance(capabilities, dict):
        raise ContractError("Base compatibility capability vectors are absent")
    for side in ("local", "peer"):
        offer = capabilities.get(side)
        if not isinstance(offer, dict) or set(offer) != {"supported", "required"}:
            raise ContractError("Base compatibility capability offer drift")
        supported = offer.get("supported")
        required = offer.get("required")
        if not isinstance(supported, list) or not isinstance(required, list) or (
            supported != sorted(set(supported))
            or required != sorted(set(required))
            or not set(required) <= set(supported)
            or len(supported) > 64
        ):
            raise ContractError("Base compatibility capability bounds drift")
    if capabilities.get("expected_intersection") != [1, 2]:
        raise ContractError("Base compatibility capability intersection drift")

    vector = profile.get("migration_vector")
    if not isinstance(vector, dict) or set(vector) != {
        "vector_id",
        "vector_blake3",
        "trust_policy_digest",
    } or not isinstance(vector.get("vector_id"), str) or not 0 < len(
        vector["vector_id"].encode("ascii")
    ) <= 64 or not digest_pattern.fullmatch(str(vector.get("vector_blake3", ""))) or not digest_pattern.fullmatch(
        str(vector.get("trust_policy_digest", ""))
    ):
        raise ContractError("Base compatibility migration binding drift")

    golden = profile.get("golden_digests")
    if not isinstance(golden, dict) or set(golden) != {
        "candidate_semantic",
        "artifact_tuple",
    } or any(
        not digest_pattern.fullmatch(str(golden.get(field, "")))
        for field in golden
    ):
        raise ContractError("Base compatibility golden digest drift")

    cases = profile.get("cases")
    if not isinstance(cases, list) or len(cases) != 34:
        raise ContractError("Base compatibility vector count drift")
    expected_outcomes = {
        "exact": "compatible",
        "base-major": "incompatible:BaseMajorMismatch",
        "base-minor": "compatible",
        "base-minor-below-floor": "incompatible:BaseMinorBelowMinimum",
        "base-patch": "compatible",
        "base-prerelease": "compatible",
        "commit-known": "compatible",
        "commit-unknown": "compatible",
        "canonical-schema": "incompatible:CanonicalSchemaMismatch",
        "domain-registry": "incompatible:DomainRegistryMismatch",
        "resource-registry": "incompatible:ResourceRegistryMismatch",
        "storage-with-vector": "migration_required",
        "storage-without-vector": "incompatible:MigrationVectorRequired",
        "archive-profile": "migration_required",
        "archive-profile-without-vector": "incompatible:MigrationVectorRequired",
        "migration-profile": "migration_required",
        "migration-profile-without-vector": "incompatible:MigrationVectorRequired",
        "registry-profile": "incompatible:RegistryProfileMismatch",
        "registry-profile-digest": "incompatible:RegistryProfileDigestMismatch",
        "wire-major": "incompatible:WireSessionMajorMismatch",
        "wire-minor": "compatible",
        "wire-minor-below-floor": "incompatible:WireSessionMinorBelowMinimum",
        "product-major": "incompatible:ProductApiMajorMismatch",
        "product-minor": "compatible",
        "product-minor-below-floor": "incompatible:ProductApiMinorBelowMinimum",
        "c-abi-major": "incompatible:CAbiMajorMismatch",
        "c-abi-minor": "compatible",
        "c-abi-minor-below-floor": "incompatible:CAbiMinorBelowMinimum",
        "optional-feature": "compatible",
        "required-feature": "incompatible:MissingRequiredCapability",
        "target": "compatible",
        "toolchain-known": "compatible",
        "toolchain-unknown": "compatible",
        "commit-toolchain-unknown": "compatible",
    }
    by_id: dict[str, dict[str, object]] = {}
    for case in cases:
        if not isinstance(case, dict) or set(case) != {
            "id",
            "field",
            "change",
            "migration_vector",
            "outcome",
            "semantic_digest_changed",
            "artifact_digest_changed",
            "qualification",
        } or not isinstance(case.get("id"), str) or case["id"] in by_id:
            raise ContractError("Base compatibility case schema/ID drift")
        by_id[case["id"]] = case
    if set(by_id) != set(expected_outcomes) or any(
        by_id[identifier].get("outcome") != outcome
        for identifier, outcome in expected_outcomes.items()
    ):
        raise ContractError("Base compatibility decision vector drift")
    covered = {
        str(case.get("field")).split(".", 1)[0].split("+", 1)[0]
        for case in cases
    }
    if not set(tuple_fields) <= covered | {"all"}:
        raise ContractError("Base compatibility tuple field coverage drift")
    if by_id["target"].get("semantic_digest_changed") is not False or by_id[
        "target"
    ].get("artifact_digest_changed") is not True or by_id[
        "toolchain-known"
    ].get("semantic_digest_changed") is not False or by_id[
        "toolchain-known"
    ].get("artifact_digest_changed") is not True:
        raise ContractError("Base artifact-only digest separation drift")
    for identifier in ("commit-unknown", "toolchain-unknown", "commit-toolchain-unknown"):
        if by_id[identifier].get("qualification") != "unqualified":
            raise ContractError("unknown build identity must remain unqualified")
    return len(cases)


def validate_base_v1_freeze() -> int:
    try:
        profile = json.loads(read(BASE_V1_FREEZE_PROFILE))
        signers = json.loads(read(BASE_V1_RELEASE_SIGNERS))
    except json.JSONDecodeError as error:
        raise ContractError(f"invalid Base v1 freeze JSON: {error}") from error
    if profile.get("format") != "onebrain/base-v1-freeze/1" or profile.get(
        "profile_id"
    ) != "BASE_V1_FREEZE_AND_EVIDENCE_PROFILE_V1":
        raise ContractError("unexpected Base v1 freeze profile")
    candidate = profile.get("candidate")
    if candidate != {
        "version": "1.0.0",
        "qualification_state_before_task_28": "Unqualified",
        "only_eligible_commit": "task-27-commit",
        "task_25_is_ancestor_checkpoint_only": True,
        "tag": "base-v1.0.0",
        "tag_must_be_absent_before_verified_atomic_publication": True,
    }:
        raise ContractError("Base v1 candidate freeze drift")
    targets = profile.get("targets")
    if targets != [
        "x86_64-unknown-linux-gnu",
        "x86_64-pc-windows-msvc",
        "aarch64-apple-darwin",
    ]:
        raise ContractError("Base v1 exact target map drift")
    gates = profile.get("base_gate_v1")
    expected_gates = {
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
    }
    if not isinstance(gates, list) or len(gates) != 12 or set(gates) != expected_gates:
        raise ContractError("BASE-GATE-V1 gate set drift")
    child = profile.get("child_evidence_policies")
    expected_child = {
        "fresh-production-registry": {
            "role": "registry-production-aggregator",
            "public_key_hex": "bef8e2b9d8ae7a38b3753a7d756a39c20948f128a66ca71ed04799e7a5d5177c",
            "fingerprint_context": "onebrain:concept-registry:signer-fingerprint:1",
            "fingerprint_hex": "dcc09574ac53ec8b95585cad5e2e88cbdfbe44841ad46b3709f73c989b4316d4",
            "trust_policy_digest": "e0a2551a39823c3f2cb088defe60484c8a33ffe0f3aab9df9493b52557ab55fe",
        },
        "fresh-multi-host-p5": {
            "role": "p5-orchestrator",
            "public_key_hex": "cce7da80b255ed3a67a8414f79e700bb0fdc4944abe3793d9c23e8ca1699fc27",
            "fingerprint_context": "onebrain:p5:evidence-signer-fingerprint:1",
            "fingerprint_hex": "6d018ba3d7224bc5a415a54c226f81db1139d950aedf0ef5dfb9b9da441b01ca",
            "trust_policy_digest": "deac187c74148dbeb9db4c29590b862121cff44506be2efc79f30d688868987b",
        },
        "fresh-exact-candidate-72h-soak": {
            "role": "soak-aggregator",
            "public_key_hex": "888cf37977b179b78aff9045a0ce599cd090172d38ec04e4d462cf70eee454b3",
            "fingerprint_context": "onebrain:base-v1:soak-evidence-signer-fingerprint:1",
            "fingerprint_hex": "8ab8e70864bd2258042dc4e5d18d271680df2566092317363b1064b6f1fa2ae9",
            "trust_policy_digest": "f2ef9e95575c47a25a1809ba580b70eac1413bc5d147f9f021987c393ba778d6",
        },
    }
    if child != expected_child:
        raise ContractError("Base v1 child evidence signer policy drift")
    evidence_approver = profile.get("base_evidence_approver_policy")
    evidence_policy_context = "onebrain:base-v1:evidence-approver-policy:1"
    evidence_fingerprint_context = (
        "onebrain:base-v1:evidence-approver-fingerprint:1"
    )
    evidence_approver_fields = {
        "status", "trust_policy_context", "trust_policy_digest", "policy",
    }
    evidence_policy_fields = {
        "algorithm", "allowed_usages", "format", "role", "signature_domain",
        "signers", "valid_unlisted_signature",
    }
    evidence_signer_fields = {
        "created_utc", "expires_utc", "fingerprint_context",
        "fingerprint_hex", "public_key_hex",
    }
    if (
        not isinstance(evidence_approver, dict)
        or set(evidence_approver) != evidence_approver_fields
        or evidence_approver.get("status") != "owner-approved"
        or evidence_approver.get("trust_policy_context")
        != evidence_policy_context
    ):
        raise ContractError("Base v1 evidence approver policy fields drift")
    evidence_policy = evidence_approver.get("policy")
    if (
        not isinstance(evidence_policy, dict)
        or set(evidence_policy) != evidence_policy_fields
        or evidence_policy.get("algorithm") != "Ed25519"
        or evidence_policy.get("allowed_usages")
        != ["gate-receipt-approval", "target-receipt-approval"]
        or evidence_policy.get("format")
        != "onebrain/base-v1-evidence-approver-policy/1"
        or evidence_policy.get("role") != "base-evidence-approver"
        or evidence_policy.get("signature_domain")
        != "onebrain:base-v1:evidence-receipt-approval:1"
        or evidence_policy.get("valid_unlisted_signature") != "reject"
    ):
        raise ContractError("Base v1 evidence approver public policy drift")
    evidence_signers = evidence_policy.get("signers")
    if (
        not isinstance(evidence_signers, list)
        or len(evidence_signers) != 1
        or not isinstance(evidence_signers[0], dict)
        or set(evidence_signers[0]) != evidence_signer_fields
    ):
        raise ContractError("Base v1 evidence approver signer allowlist drift")
    evidence_signer = evidence_signers[0]
    public_key = evidence_signer.get("public_key_hex")
    fingerprint = evidence_signer.get("fingerprint_hex")
    trust_digest = evidence_approver.get("trust_policy_digest")
    if (
        evidence_signer.get("fingerprint_context")
        != evidence_fingerprint_context
        or not all(
            isinstance(value, str) and re.fullmatch(r"[0-9a-f]{64}", value)
            for value in (public_key, fingerprint, trust_digest)
        )
    ):
        raise ContractError("approved Base v1 evidence approver values are invalid")
    measured_fingerprint = blake3.blake3(
        bytes.fromhex(public_key),
        derive_key_context=evidence_fingerprint_context,
    ).hexdigest()
    if measured_fingerprint != fingerprint:
        raise ContractError("Base v1 evidence approver fingerprint does not derive")
    canonical_evidence_policy = json.dumps(
        evidence_policy, ensure_ascii=False, sort_keys=True, separators=(",", ":")
    ).encode("utf-8")
    measured_trust_digest = blake3.blake3(
        canonical_evidence_policy,
        derive_key_context=evidence_policy_context,
    ).hexdigest()
    if measured_trust_digest != trust_digest:
        raise ContractError("Base v1 evidence approver trust policy does not derive")
    try:
        created_utc = datetime.fromisoformat(
            evidence_signer["created_utc"].removesuffix("Z") + "+00:00"
        )
        expires_utc = datetime.fromisoformat(
            evidence_signer["expires_utc"].removesuffix("Z") + "+00:00"
        )
    except (TypeError, ValueError) as error:
        raise ContractError(
            "Base v1 evidence approver validity is invalid"
        ) from error
    if (
        not str(evidence_signer["created_utc"]).endswith("Z")
        or not str(evidence_signer["expires_utc"]).endswith("Z")
        or created_utc.microsecond
        or expires_utc.microsecond
        or created_utc.tzinfo != timezone.utc
        or expires_utc.tzinfo != timezone.utc
        or created_utc >= expires_utc
        or not (created_utc <= datetime.now(timezone.utc) < expires_utc)
    ):
        raise ContractError("Base v1 evidence approver is not currently valid")
    if signers.get("format") != "onebrain/base-v1-release-signers/1" or signers.get(
        "owner_approval"
    ) != {
        "status": "owner-approved",
        "approved_utc": "2026-08-11",
        "sample_or_default_keys_allowed": False,
    }:
        raise ContractError("Base v1 release signer approval drift")
    policies = signers.get("policies")
    if not isinstance(policies, list) or len(policies) != 3:
        raise ContractError("Base v1 release signer role count drift")
    by_role = {row.get("policy", {}).get("role"): row for row in policies}
    expected_policies = {
        "qualification-approver": (
            "CB3FF16A1A2C8B017B5D83DF59DC9C079E00928B",
            "2e7cc2dacafad658ab5fe4e1536a4b92590f788c9c9e5a450d123930d65cfbd6",
            ["base-release-request"],
        ),
        "base-release": (
            "F9DDAFB46FB6603E14B21B4DB0D9DBF23DBE8ED2",
            "443534ac4f583368cc5e07b1c4dbddf1ac66c63eba32bcf9e565b07f07a80d88",
            ["base-evidence-manifest", "base-release-tag"],
        ),
    }
    if set(by_role) != {*expected_policies, "base-evidence-approver"}:
        raise ContractError("Base v1 release signer roles drift")
    for role, (fingerprint, digest, usages) in expected_policies.items():
        row = by_role[role]
        policy = row.get("policy", {})
        if (
            policy.get("algorithm") != "OpenPGP-Ed25519"
            or policy.get("allowed_usages") != usages
            or policy.get("signers", [{}])[0].get("fingerprint") != fingerprint
            or row.get("digest", {}).get("expected_hex") != digest
        ):
            raise ContractError(f"Base v1 {role} signer policy drift")
    evidence_policy_row = by_role["base-evidence-approver"]
    if (
        evidence_policy_row.get("policy") != evidence_policy
        or evidence_policy_row.get("digest")
        != {
            "algorithm": "BLAKE3 derive-key",
            "context": evidence_policy_context,
            "expected_hex": trust_digest,
        }
    ):
        raise ContractError("Base v1 evidence approver vector policy drift")
    source = read(ROOT / "src/onebrain-base-contract/src/compatibility.rs")
    runtime = read(ROOT / "src/onebrain-node/src/base_runtime.rs")
    qualifier = read(ROOT / "scripts/base/qualify_base.py")
    frozen_binding = None
    for statement in ast.parse(qualifier).body:
        if (
            isinstance(statement, ast.Assign)
            and any(isinstance(target, ast.Name) and target.id == "FROZEN_PROFILE_BLAKE3" for target in statement.targets)
            and isinstance(statement.value, ast.Constant)
            and isinstance(statement.value.value, str)
        ):
            frozen_binding = statement.value.value
    canonical_profile = json.dumps(
        profile, ensure_ascii=False, sort_keys=True, separators=(",", ":")
    ).encode("utf-8")
    measured_profile = blake3.blake3(canonical_profile).hexdigest()
    if frozen_binding != measured_profile:
        raise ContractError("Base v1 frozen-profile digest binding drift")
    if profile.get("release_publication", {}).get("request_validity_hours") != 168:
        raise ContractError("Base v1 release request validity drift")
    if profile.get("machine_receipts") != {
        "gate_format": "onebrain/base-v1-gate-receipt/1",
        "target_format": "onebrain/base-v1-target-receipt/1",
        "outer_result_claim_allowed": False,
        "pass_derivation": "closed-frozen-check-contract-and-substantive-output-oracle",
        "command_and_output_hashes_required": True,
        "runner_provenance_required": True,
        "substantive_output_oracle_required": True,
        "target_binary_sbom_provenance_relation_required": True,
        "spdx_package_verification_code": (
            "sha1-of-concatenated-sorted-analyzed-file-sha1-no-excludes"
        ),
        "slsa_builder_id": "absolute-TypeURI-distinct-from-runner_identity",
    }:
        raise ContractError("Base v1 machine receipt contract drift")
    gate_contracts = profile.get("gate_check_contracts")
    target_contracts = profile.get("target_check_contracts")
    if not isinstance(gate_contracts, dict) or set(gate_contracts) != expected_gates:
        raise ContractError("Base v1 frozen gate check contract set drift")
    if not isinstance(target_contracts, dict) or set(target_contracts) != set(targets):
        raise ContractError("Base v1 frozen target check contract set drift")
    expected_builder_ids = {
        "x86_64-unknown-linux-gnu": (
            "https://onebrain.dev/builders/base-v1/linux-release-runner/v1"
        ),
        "x86_64-pc-windows-msvc": (
            "https://onebrain.dev/builders/base-v1/windows-release-runner/v1"
        ),
        "aarch64-apple-darwin": (
            "https://onebrain.dev/builders/base-v1/macos-release-runner/v1"
        ),
    }
    for owner, contracts in {**gate_contracts, **target_contracts}.items():
        if not isinstance(contracts, list) or not contracts:
            raise ContractError(f"Base v1 {owner} has no frozen substantive checks")
        names: set[str] = set()
        is_target_contract = owner in target_contracts
        expected_contract_fields = {
            "name", "command", "runner_kind", "runner_identity",
            "required_assertion_ids",
        }
        if is_target_contract:
            expected_contract_fields.add("builder_id")
        for contract in contracts:
            if not isinstance(contract, dict) or set(contract) != expected_contract_fields:
                raise ContractError(f"Base v1 {owner} check contract fields drift")
            name = contract.get("name")
            command = contract.get("command")
            assertions = contract.get("required_assertion_ids")
            if (
                not isinstance(name, str)
                or not name
                or name in names
                or not isinstance(command, list)
                or not command
                or not all(isinstance(argument, str) and argument for argument in command)
                or contract.get("runner_kind") != "candidate-bound-runner"
                or not isinstance(contract.get("runner_identity"), str)
                or not contract["runner_identity"]
                or not isinstance(assertions, list)
                or not assertions
                or len(assertions) != len(set(assertions))
                or not all(isinstance(assertion, str) and assertion for assertion in assertions)
            ):
                raise ContractError(f"Base v1 {owner} check contract is not substantive")
            if is_target_contract:
                builder_id = contract.get("builder_id")
                try:
                    parsed_builder_id = urlsplit(builder_id)
                    builder_port = parsed_builder_id.port
                except (TypeError, ValueError) as error:
                    raise ContractError(
                        f"Base v1 {owner} SLSA builder ID is not an absolute TypeURI"
                    ) from error
                if (
                    builder_port is not None and builder_port <= 0
                    or not isinstance(builder_id, str)
                    or any(character.isspace() for character in builder_id)
                    or parsed_builder_id.scheme != "https"
                    or parsed_builder_id.netloc != "onebrain.dev"
                    or builder_id != expected_builder_ids.get(owner)
                    or builder_id == contract.get("runner_identity")
                ):
                    raise ContractError(
                        f"Base v1 {owner} SLSA builder ID is not target-frozen"
                    )
            names.add(name)
    if "pub const BASE_V1_RELEASE_VERSION" not in source or not all(
        needle in source for needle in ("major: 1", "minor: 0", "patch: 0", "prerelease: None")
    ) or "base_version: BASE_V1_RELEASE_VERSION" not in runtime:
        raise ContractError("compiled Base v1.0.0 candidate version drift")
    for path in (
        BASE_V1_FREEZE_DOCUMENT,
        ROOT / "docs/security/BASE_V1_RELEASE_SIGNER_POLICY.md",
        ROOT / "docs/operations/ONEBRAIN_BASE_V1_MIGRATION_GUIDE.md",
        ROOT / "docs/operations/ONEBRAIN_BASE_V1_ROLLBACK_GUIDE.md",
        ROOT / "docs/operations/ONEBRAIN_BASE_V1_CHANGELOG.md",
        ROOT / "scripts/base/qualify_base.py",
        ROOT / "scripts/release/create_base_release_request.py",
        ROOT / "scripts/release/prepare_clean_candidate.py",
        ROOT / "scripts/release/create_verified_base_release.py",
    ):
        if not path.is_file():
            raise ContractError(f"Base v1 freeze artifact missing: {path.relative_to(ROOT)}")
    return len(gates)


def validate_negative_assertions() -> int:
    yaml_text = read(NEGATIVE_ASSERTIONS)
    ids = re.findall(r"(?m)^\s*-\s+id:\s*([A-Z0-9-]+)\s*$", yaml_text)
    if len(ids) != len(set(ids)):
        raise ContractError("duplicate negative assertion IDs")
    if len(ids) < 37:
        raise ContractError(f"expected at least 37 negative assertions, found {len(ids)}")
    vocabulary_ids = set(NEGATIVE_ID.findall(read(VOCABULARY)))
    missing = sorted(set(ids) - vocabulary_ids)
    if missing:
        raise ContractError(f"negative assertions missing from vocabulary: {missing}")
    return len(ids)


def validate_vectors() -> tuple[int, int, int, int]:
    try:
        vectors = json.loads(read(VECTORS))
    except json.JSONDecodeError as error:
        raise ContractError(f"invalid foundation vector JSON: {error}") from error
    if vectors.get("format") != "onebrain/foundation-vectors/1":
        raise ContractError("unexpected foundation vector format")
    if vectors.get("canonical_profile") != "onebrain/canonical/1":
        raise ContractError("unexpected canonical profile")

    sections = (
        "valid_cbor",
        "invalid_cbor",
        "normalized_text",
        "domain_digests",
        "envelopes",
        "signatures",
    )
    ids: list[str] = []
    for section in sections:
        rows = vectors.get(section)
        if not isinstance(rows, list) or not rows:
            raise ContractError(f"vector section {section} must be a non-empty list")
        ids.extend(row.get("id", "") for row in rows if isinstance(row, dict))
    if any(not vector_id for vector_id in ids):
        raise ContractError("foundation vector without an ID")
    if len(ids) != len(set(ids)):
        raise ContractError("duplicate foundation vector IDs")

    domains = [row.get("domain") for row in vectors["domain_digests"]]
    if len(domains) != 21 or len(set(domains)) != 21:
        raise ContractError("foundation vectors must cover 21 unique reserved domains")

    try:
        schema_vectors = json.loads(read(IDENTITY_OBJECT_VECTORS))
    except json.JSONDecodeError as error:
        raise ContractError(f"invalid identity-object vector JSON: {error}") from error
    if schema_vectors.get("format") != "onebrain/schema-vectors/1":
        raise ContractError("unexpected schema vector format")
    if schema_vectors.get("schema_profile") != "onebrain/identity-object/1":
        raise ContractError("unexpected identity-object profile")
    identities = schema_vectors.get("identities", [])
    objects = schema_vectors.get("objects", [])
    schema_ids = [row.get("id", "") for row in identities + objects]
    if len(identities) < 5 or len(objects) < 3:
        raise ContractError("identity-object vectors lack required coverage")
    if any(not vector_id for vector_id in schema_ids) or len(schema_ids) != len(
        set(schema_ids)
    ):
        raise ContractError("missing or duplicate identity-object vector IDs")
    collision_pair = identities[:2]
    if len(collision_pair) != 2:
        raise ContractError("missing full-width collision pair")
    left = bytes.fromhex(collision_pair[0]["raw_hex"])
    right = bytes.fromhex(collision_pair[1]["raw_hex"])
    if left[:8] != right[:8] or left == right:
        raise ContractError("identity collision pair must share only its 64-bit prefix")

    try:
        event_vectors = json.loads(read(FEED_EVENT_VECTORS))
    except json.JSONDecodeError as error:
        raise ContractError(f"invalid feed-event vector JSON: {error}") from error
    if event_vectors.get("format") != "onebrain/schema-vectors/1":
        raise ContractError("unexpected feed-event vector format")
    if event_vectors.get("schema_profile") != "onebrain/feed-event/1":
        raise ContractError("unexpected feed-event profile")
    feeds = event_vectors.get("feed_inceptions", [])
    events = event_vectors.get("events", [])
    event_ids = [row.get("id", "") for row in feeds + events]
    if len(feeds) < 1 or len(events) < 3:
        raise ContractError("feed-event vectors lack required coverage")
    if any(not vector_id for vector_id in event_ids) or len(event_ids) != len(
        set(event_ids)
    ):
        raise ContractError("missing or duplicate feed-event vector IDs")
    if not any(row.get("error") == "SIGNATURE_INVALID" for row in events):
        raise ContractError("feed-event vectors must include signature rejection")
    if not any(row.get("opaque") is True for row in events):
        raise ContractError("feed-event vectors must include opaque event semantics")
    return len(ids), len(domains), len(schema_ids), len(event_ids)


def validate_product_integration_profile(
    profile: dict[str, object] | None = None,
) -> tuple[int, int]:
    if profile is None:
        try:
            profile = json.loads(read(PRODUCT_PROFILE))
        except json.JSONDecodeError as error:
            raise ContractError(
                f"invalid product integration profile JSON: {error}"
            ) from error

    if profile.get("format") != "onebrain/vnext-product-integration-profile/1":
        raise ContractError("unexpected product integration profile format")
    if profile.get("profile_id") != "VNEXT_PRODUCT_INTEGRATION_PROFILE_V1":
        raise ContractError("unexpected product integration profile ID")
    if (
        profile.get("version") != 1
        or profile.get("profile_major") != 1
        or profile.get("profile_minor") != 1
        or profile.get("base_path") != "/api/vnext"
    ):
        raise ContractError("unexpected product integration version/base path")

    wire = profile.get("wire")
    if not isinstance(wire, dict):
        raise ContractError("product integration profile lacks wire rules")
    cid = wire.get("cid")
    expected_cid = {
        "encoding": "lowercase_hex",
        "decoded_bytes": 32,
        "encoded_chars": 64,
        "prefix": "",
        "typed": True,
    }
    if cid != expected_cid:
        raise ContractError("product API CID encoding drift")
    continuation = wire.get("continuation")
    if not isinstance(continuation, dict) or any(
        continuation.get(key) != value
        for key, value in {
            "encoding": "base64url_no_pad",
            "prefix": "obc1.",
            "opaque": True,
            "context_bound": True,
            "max_chars": 2048,
        }.items()
    ):
        raise ContractError("product API continuation contract drift")

    envelope = profile.get("envelope")
    if not isinstance(envelope, dict):
        raise ContractError("product integration profile lacks envelope rules")
    expected_envelope = {
        "success_required": {"ok", "profile", "data", "meta"},
        "error_required": {"ok", "profile", "error", "meta"},
        "meta_required": {"lifecycle", "coverage", "limitations", "continuation"},
        "lifecycle_states": {"disabled", "requested", "active", "degraded"},
        "coverage_states": {"local_only", "partial"},
        "work_states": {"pending", "deferred", "quarantined", "conflict"},
    }
    for field, expected in expected_envelope.items():
        value = envelope.get(field)
        if not isinstance(value, list) or set(value) != expected:
            raise ContractError(f"product envelope field drift: {field}")

    errors = profile.get("errors")
    expected_errors = {
        ("invalid_request", 400, False),
        ("not_found", 404, False),
        ("conflict", 409, False),
        ("expired", 410, False),
        ("rate_limited", 429, True),
        ("capability_disabled", 503, False),
        ("dependency_unavailable", 503, True),
        ("internal_error", 500, False),
    }
    if not isinstance(errors, list):
        raise ContractError("product integration profile lacks error semantics")
    actual_errors = {
        (row.get("code"), row.get("http_status"), row.get("retryable"))
        for row in errors
        if isinstance(row, dict)
    }
    if actual_errors != expected_errors:
        raise ContractError("product error code/status/retryability drift")

    dtos = profile.get("dtos")
    if not isinstance(dtos, dict) or not dtos:
        raise ContractError("product integration profile lacks DTO definitions")
    for name, dto in dtos.items():
        if not isinstance(name, str) or not isinstance(dto, dict):
            raise ContractError("invalid product DTO definition")
        required = dto.get("required")
        if not isinstance(required, list) or not required:
            raise ContractError(f"product DTO lacks required fields: {name}")
        if len(required) != len(set(required)):
            raise ContractError(f"duplicate required field in product DTO: {name}")

    field_types = profile.get("dto_field_types")
    if not isinstance(field_types, dict) or set(field_types) != set(dtos):
        raise ContractError("product DTO type inventory drift")
    for name, fields in field_types.items():
        if (
            not isinstance(fields, dict)
            or set(fields) != set(dtos[name]["required"])
            or any(not isinstance(value, str) or not value for value in fields.values())
        ):
            raise ContractError(f"product DTO field-type drift: {name}")

    expected_endpoints = {
        ("GET", "/api/vnext/workflow"),
        ("GET", "/api/vnext/workflow/{stage}"),
        ("POST", "/api/vnext/kql/needs/prepare"),
        ("POST", "/api/vnext/kql/needs"),
        ("GET", "/api/vnext/kql/needs"),
        ("GET", "/api/vnext/kql/needs/{id}"),
        ("GET", "/api/vnext/kql/needs/{id}/matches"),
        ("POST", "/api/vnext/kql/needs/{id}/scan"),
        ("DELETE", "/api/vnext/kql/needs/{id}"),
        ("POST", "/api/vnext/pomv/public-use/prepare"),
        ("POST", "/api/vnext/pomv/public-use/confirm"),
        ("GET", "/api/vnext/pomv/publications/{id}"),
        ("GET", "/api/vnext/pomv/views/{target}"),
        ("POST", "/api/vnext/base/negotiate"),
        ("GET", "/api/vnext/runtime/status"),
    }
    endpoints = profile.get("endpoints")
    if not isinstance(endpoints, list):
        raise ContractError("product integration profile lacks endpoints")
    actual_endpoints: set[tuple[str, str]] = set()
    for endpoint in endpoints:
        if not isinstance(endpoint, dict):
            raise ContractError("invalid product endpoint row")
        method = endpoint.get("method")
        path = endpoint.get("path")
        status = endpoint.get("status")
        visibility = endpoint.get("visibility")
        request = endpoint.get("request")
        response = endpoint.get("response")
        if not isinstance(method, str) or not isinstance(path, str):
            raise ContractError("product endpoint lacks method/path")
        if not path.startswith("/api/vnext/"):
            raise ContractError(f"vNext endpoint escaped additive namespace: {path}")
        key = (method, path)
        if key in actual_endpoints:
            raise ContractError(f"duplicate product endpoint: {method} {path}")
        actual_endpoints.add(key)
        if status not in {"implemented_read_only", "reserved"}:
            raise ContractError(f"invalid product endpoint status: {method} {path}")
        expected_visibility = (
            "authenticated_local_private"
            if path.startswith("/api/vnext/kql/needs")
            or path.startswith("/api/vnext/pomv/public-use")
            else "authenticated_local"
        )
        if visibility != expected_visibility:
            raise ContractError(
                f"product endpoint visibility drift: {method} {path}"
            )
        if request is not None and request not in dtos:
            raise ContractError(f"undefined request DTO for {method} {path}: {request}")
        if not isinstance(response, str) or response not in dtos:
            raise ContractError(f"undefined response DTO for {method} {path}: {response}")
    if actual_endpoints != expected_endpoints:
        raise ContractError(
            "product endpoint inventory drift; "
            f"missing={sorted(expected_endpoints - actual_endpoints)}, "
            f"extra={sorted(actual_endpoints - expected_endpoints)}"
        )

    forbidden_client = profile.get("client_supplied_forbidden_fields")
    forbidden_response = profile.get("response_forbidden_fields")
    non_exportable = profile.get("non_exportable_fields")
    if (
        not isinstance(forbidden_client, list)
        or not isinstance(forbidden_response, list)
        or not isinstance(non_exportable, list)
    ):
        raise ContractError("product profile lacks field firewalls")
    client_fields = set(forbidden_client)
    response_fields = set(forbidden_response)
    for endpoint in endpoints:
        request_name = endpoint.get("request")
        if isinstance(request_name, str):
            request_fields = set(dtos[request_name]["required"])
            if request_fields & client_fields:
                raise ContractError(
                    f"client can supply authority/secret field through {request_name}"
                )
        response_name = endpoint["response"]
        output_fields = set(dtos[response_name]["required"])
        if output_fields & response_fields:
            raise ContractError(f"private field leaks through {response_name}")
    if set(non_exportable) != {
        "local_query",
        "standing_need_id",
        "query_definition_cid",
        "single_use_receipt",
    }:
        raise ContractError("non-exportable product field inventory drift")

    legacy = profile.get("legacy_surfaces")
    expected_legacy = {
        ("/api/kql", "legacy_local_kql", True),
        ("/api/watch", "legacy_watch", True),
        ("pomv", "legacy_local_pomv_scalar_v1", True),
        ("pomv_breakdown", "legacy_local_pomv_breakdown_v1", True),
    }
    if not isinstance(legacy, list):
        raise ContractError("product profile lacks legacy boundary inventory")
    actual_legacy = {
        (row.get("path"), row.get("meaning"), row.get("unchanged"))
        for row in legacy
        if isinstance(row, dict)
    }
    if actual_legacy != expected_legacy:
        raise ContractError("legacy product meaning changed or inventory drifted")

    firewalls = profile.get("semantic_firewalls")
    required_firewalls = {
        "proposal_executable",
        "proposal_materializes_mapping",
        "proposal_grants_authority",
        "pomv_establishes_truth",
        "pomv_establishes_benefit",
        "pomv_authorizes_reward",
        "pomv_claims_global_completion",
        "zero_results_claim_global_absence",
    }
    if (
        not isinstance(firewalls, dict)
        or set(firewalls) != required_firewalls
        or any(value is not False for value in firewalls.values())
    ):
        raise ContractError("product semantic firewall must remain fail-closed")

    match_fields = set(dtos["QuarantinedMatchV1"]["required"])
    metabolic_fields = set(dtos["MetabolicEvidenceViewV1"]["required"])
    if not {"state", "executable", "limitations"} <= match_fields:
        raise ContractError("quarantined match DTO lost non-executable fields")
    if (
        field_types["QuarantinedMatchV1"]["state"] != "literal:quarantined"
        or field_types["QuarantinedMatchV1"]["executable"] != "literal:false"
    ):
        raise ContractError("quarantined match DTO weakened its literal types")
    if not {
        "establishes_truth",
        "establishes_benefit",
        "authorizes_reward",
        "claims_global_completion",
        "limitations",
    } <= metabolic_fields:
        raise ContractError("metabolic view DTO lost semantic firewall fields")
    if any(
        field_types["MetabolicEvidenceViewV1"][field] != "literal:false"
        for field in {
            "establishes_truth",
            "establishes_benefit",
            "authorizes_reward",
            "claims_global_completion",
        }
    ):
        raise ContractError("metabolic view DTO weakened its literal false types")

    return len(endpoints), len(dtos)


def validate_private_websocket_profile(
    profile: dict[str, object] | None = None,
) -> tuple[int, int]:
    if profile is None:
        try:
            profile = json.loads(read(PRIVATE_WS_PROFILE))
        except json.JSONDecodeError as error:
            raise ContractError(
                f"invalid private WebSocket profile JSON: {error}"
            ) from error

    if profile.get("format") != "onebrain/vnext-private-websocket-profile/1":
        raise ContractError("unexpected private WebSocket profile format")
    if profile.get("profile_id") != "VNEXT_PRIVATE_WEBSOCKET_PROFILE_V1":
        raise ContractError("unexpected private WebSocket profile ID")
    if profile.get("version") != 1:
        raise ContractError("unexpected private WebSocket profile version")

    expected_routes = {
        (
            "POST",
            "/api/vnext/ws/tickets",
            "bearer",
            "VNextWsTicketRequestV1",
            "VNextWsTicketV1",
        ),
        (
            "GET",
            "/api/vnext/ws",
            "single_use_ticket",
            "VNextWsTicketQuery",
            "websocket_upgrade",
        ),
    }
    routes = profile.get("routes")
    if not isinstance(routes, list):
        raise ContractError("private WebSocket profile lacks routes")
    actual_routes = {
        (
            row.get("method"),
            row.get("path"),
            row.get("authentication"),
            row.get("request"),
            row.get("response"),
        )
        for row in routes
        if isinstance(row, dict)
    }
    if actual_routes != expected_routes:
        raise ContractError("private WebSocket route inventory drift")

    expected_ticket = {
        "encoding": "base64url_no_pad",
        "prefix": "obw1.",
        "decoded_bytes": 32,
        "single_use": True,
        "ticket_ttl_seconds": 30,
        "session_ttl_seconds": 900,
        "subscription_immutable": True,
    }
    if profile.get("ticket") != expected_ticket:
        raise ContractError("private WebSocket ticket contract drift")

    expected_topics = {"matches", "publications", "views", "runtime"}
    topics = profile.get("topics")
    if not isinstance(topics, list) or set(topics) != expected_topics:
        raise ContractError("private WebSocket topic inventory drift")

    expected_envelope = {
        "profile",
        "event_type",
        "sequence",
        "timestamp",
        "lifecycle",
        "coverage",
        "limitations",
        "data",
    }
    envelope = profile.get("event_envelope_required")
    if not isinstance(envelope, list) or set(envelope) != expected_envelope:
        raise ContractError("private WebSocket event envelope drift")

    expected_events = {
        "subscription_ready",
        "bounded_match_available",
        "publication_queued",
        "publication_delivered",
        "publication_deferred",
        "view_revision",
        "view_conflict",
        "lane_active",
        "lane_disabled",
        "lane_degraded",
    }
    events = profile.get("event_types")
    if not isinstance(events, list) or set(events) != expected_events:
        raise ContractError("private WebSocket event inventory drift")

    expected_limits = {
        "max_topics": 4,
        "max_pending_tickets": 128,
        "max_active_sessions": 64,
        "event_queue_capacity": 32,
        "max_client_message_bytes": 4096,
    }
    if profile.get("limits") != expected_limits:
        raise ContractError("private WebSocket bounded limits drift")

    expected_publication_states = {
        "queued": "pending",
        "delivered": "delivered",
        "deferred": "deferred",
        "delivered_requires_durable_authenticated_acknowledgement": True,
    }
    if profile.get("publication_states") != expected_publication_states:
        raise ContractError("private WebSocket publication state drift")

    expected_non_exportable = {
        "local_query",
        "standing_need_id",
        "query_definition_cid",
        "private_target",
        "proposal_cid",
        "single_use_receipt",
        "client_session",
        "ticket",
    }
    non_exportable = profile.get("event_non_exportable_fields")
    if not isinstance(non_exportable, list) or set(non_exportable) != expected_non_exportable:
        raise ContractError("private WebSocket non-exportable field inventory drift")

    expected_firewalls = {
        "cross_client_delivery",
        "event_is_authority",
        "event_materializes_mapping",
        "event_creates_use_evidence",
        "event_establishes_truth",
        "event_establishes_benefit",
        "event_authorizes_reward",
        "event_claims_global_completion",
        "slow_client_blocks_runtime",
    }
    firewalls = profile.get("semantic_firewalls")
    if (
        not isinstance(firewalls, dict)
        or set(firewalls) != expected_firewalls
        or any(value is not False for value in firewalls.values())
    ):
        raise ContractError("private WebSocket semantic firewall must remain fail-closed")

    return len(events), len(topics)


def validate_vnext_cli_profile(
    profile: dict[str, object] | None = None,
) -> int:
    if profile is None:
        try:
            profile = json.loads(read(VNEXT_CLI_PROFILE))
        except json.JSONDecodeError as error:
            raise ContractError(f"invalid vNext CLI profile JSON: {error}") from error

    if profile.get("format") != "onebrain/vnext-cli-profile/1":
        raise ContractError("unexpected vNext CLI profile format")
    if profile.get("profile_id") != "VNEXT_CLI_PROFILE_V1":
        raise ContractError("unexpected vNext CLI profile ID")
    if profile.get("version") != 1:
        raise ContractError("unexpected vNext CLI profile version")

    expected_commands = {
        "need prepare",
        "need activate",
        "need list",
        "need scan",
        "need matches",
        "need retire",
        "pomv use prepare",
        "pomv use confirm",
        "pomv use status",
        "pomv view",
        "vnext status",
    }
    commands = profile.get("commands")
    if not isinstance(commands, list) or set(commands) != expected_commands:
        raise ContractError("vNext CLI command inventory drift")

    transport = profile.get("transport")
    if transport != {
        "contract": "VNEXT_PRODUCT_INTEGRATION_PROFILE_V1",
        "authentication": "bearer",
        "default_api_url": "http://127.0.0.1:4280",
        "token_environment": "ONEBRAIN_API_TOKEN",
    }:
        raise ContractError("vNext CLI transport contract drift")

    need = profile.get("need")
    expected_need = {
        "scope": "one_hop",
        "raw_query_local_only": True,
        "exact_replay_identity": True,
        "match_state": "quarantined",
        "match_executable": False,
        "zero_result_claims_global_absence": False,
        "continuation_opaque": True,
    }
    if need != expected_need:
        raise ContractError("vNext CLI Need firewall drift")

    public_use = profile.get("public_use")
    if not isinstance(public_use, dict):
        raise ContractError("vNext CLI profile lacks Public Use contract")
    expected_preview = {
        "canonical_payload_preview",
        "exact_target",
        "exact_recipient",
        "selector_cid",
        "namespace",
        "disclosure",
        "intent_cid",
        "expires_at",
    }
    if (
        public_use.get("prepare_creates_evidence") is not False
        or public_use.get("prepare_requires_public_permanent_acknowledgement")
        is not True
        or set(public_use.get("preview_required", [])) != expected_preview
        or public_use.get("confirmation_requires_exact_typed_intent") is not True
        or public_use.get("yes_bypass") is not False
        or public_use.get("single_use_receipt_exported") is not False
        or public_use.get("exact_replay_identity") is not True
        or public_use.get("delivery_acknowledgement_inferred") is not False
    ):
        raise ContractError("vNext CLI Public Use contract drift")

    signer = profile.get("feed_signer")
    if not isinstance(signer, dict):
        raise ContractError("vNext CLI profile lacks Feed signer contract")
    if (
        signer.get("selection_explicit") is not True
        or set(signer.get("providers", [])) != {"none", "development-file"}
        or signer.get("development_file_requires_opt_in") is not True
        or signer.get("development_file_warning") is not True
        or signer.get("development_file_production_custody") is not False
        or signer.get("fallback_on_failure") is not False
    ):
        raise ContractError("vNext CLI Feed signer contract drift")

    expected_false = {
        "establishes_truth": False,
        "establishes_benefit": False,
        "authorizes_reward": False,
        "claims_global_completion": False,
        "conflict_displays_authorized": False,
    }
    if profile.get("view_firewalls") != expected_false:
        raise ContractError("vNext CLI view firewall drift")

    expected_status = {
        "compiled",
        "requested",
        "active",
        "kill_switch",
        "signer_ready",
        "lifecycle",
        "coverage",
        "limitations",
    }
    status_fields = profile.get("status_fields")
    if not isinstance(status_fields, list) or set(status_fields) != expected_status:
        raise ContractError("vNext CLI status field inventory drift")

    legacy = profile.get("legacy")
    if legacy != {
        "kql_reinterpreted": False,
        "pomv_scalar_reinterpreted": False,
        "status_reinterpreted": False,
    }:
        raise ContractError("vNext CLI legacy isolation drift")

    return len(commands)


def validate_vnext_desktop_web_ux_profile(
    profile: dict[str, object] | None = None,
) -> int:
    if profile is None:
        try:
            profile = json.loads(read(VNEXT_DESKTOP_WEB_UX_PROFILE))
        except json.JSONDecodeError as error:
            raise ContractError(
                f"invalid vNext Desktop/Web UX profile JSON: {error}"
            ) from error

    if profile.get("format") != "onebrain/vnext-desktop-web-ux-profile/1":
        raise ContractError("unexpected vNext Desktop/Web UX profile format")
    if profile.get("profile_id") != "VNEXT_DESKTOP_WEB_UX_PROFILE_V1":
        raise ContractError("unexpected vNext Desktop/Web UX profile ID")
    if profile.get("version") != 1:
        raise ContractError("unexpected vNext Desktop/Web UX profile version")

    discovery = profile.get("discovery")
    expected_match_fields = {
        "responder_scope",
        "selector_cid",
        "assessed_frontier",
        "limitations",
        "continuation",
    }
    if (
        not isinstance(discovery, dict)
        or discovery.get("local_kql_contacts_peers") is not False
        or discovery.get("one_hop_is_separate_surface") is not True
        or discovery.get("one_hop_scope") != "one_hop"
        or discovery.get("zero_result_claims_global_absence") is not False
        or discovery.get("match_label") != "quarantined proposal"
        or discovery.get("match_executable") is not False
        or set(discovery.get("required_match_fields", [])) != expected_match_fields
    ):
        raise ContractError("vNext Desktop/Web discovery firewall drift")

    pomv = profile.get("pomv")
    expected_view_fields = {
        "policy_cid",
        "assessed_frontier",
        "revision",
        "conflicts",
        "coverage",
        "limitations",
    }
    if (
        not isinstance(pomv, dict)
        or pomv.get("legacy_scalar_is_separate") is not True
        or pomv.get("view_load_creates_use_evidence") is not False
        or pomv.get("publication_lookup_creates_use_evidence") is not False
        or pomv.get("conflict_displays_authorized") is not False
        or set(pomv.get("required_view_fields", [])) != expected_view_fields
    ):
        raise ContractError("vNext Desktop/Web PoMV firewall drift")

    public_use = profile.get("public_use")
    expected_preview = {
        "canonical_payload_preview",
        "exact_target",
        "exact_recipient",
        "selector_cid",
        "namespace",
        "disclosure",
        "intent_cid",
        "idempotency_key",
        "expires_at",
    }
    if (
        not isinstance(public_use, dict)
        or public_use.get("prepare_creates_use_evidence") is not False
        or public_use.get("public_permanent_acknowledgement_required") is not True
        or public_use.get("exact_typed_intent_required") is not True
        or public_use.get("receipt_derived_after_exact_match") is not True
        or public_use.get("receipt_exported_to_ui") is not False
        or set(public_use.get("required_preview_fields", [])) != expected_preview
        or set(public_use.get("publication_states_visible", []))
        != {"pending", "deferred"}
        or public_use.get("delivery_acknowledgement_inferred") is not False
    ):
        raise ContractError("vNext Desktop/Web Public Use firewall drift")

    expected_status = {
        "compiled",
        "requested",
        "active",
        "kill_switch",
        "signer_ready",
        "lifecycle",
        "coverage",
        "limitations",
    }
    if set(profile.get("settings_fields", [])) != expected_status:
        raise ContractError("vNext Desktop/Web status field inventory drift")

    if profile.get("desktop") != {
        "quit_graceful_shutdown": True,
        "restart_graceful_shutdown": True,
        "restart_rebuilds_process": True,
        "tray_quit_bypasses_shutdown": False,
    }:
        raise ContractError("vNext Desktop lifecycle firewall drift")

    compatibility = profile.get("compatibility")
    if (
        not isinstance(compatibility, dict)
        or any(value is not False for value in compatibility.values())
        or set(compatibility)
        != {
            "legacy_kql_reinterpreted",
            "legacy_pomv_reinterpreted",
            "private_websocket_cross_client_delivery",
            "api_cli_replay_identity_changed",
        }
    ):
        raise ContractError("vNext Desktop/Web compatibility firewall drift")

    vectors = profile.get("receipt_vectors")
    if (
        not isinstance(vectors, list)
        or len(vectors) != 2
        or any(
            not isinstance(row, dict)
            or not re.fullmatch(r"[0-9a-f]{64}", str(row.get("intent_cid", "")))
            or not re.fullmatch(
                r"obc1\.[A-Za-z0-9_-]{43}", str(row.get("receipt", ""))
            )
            for row in vectors
        )
    ):
        raise ContractError("vNext Desktop/Web receipt vector drift")

    source_contract = {
        "src/onebrain-web/src/pages/Discovery.tsx": (
            "Local KQL",
            "One-hop discovery",
        ),
        "src/onebrain-web/src/pages/OneHopDiscovery.tsx": (
            "quarantined proposal",
            "Responder scope",
            "Assessed frontier",
            "Coverage outside this assessed",
        ),
        "src/onebrain-web/src/pages/Pomv.tsx": (
            "Legacy local scalar",
            "vNext Evidence View / Public Use",
        ),
        "src/onebrain-web/src/pages/VNextPomv.tsx": (
            "Exact canonical payload bytes",
            "Type the exact intent CID to confirm",
            "outbox /",
            "UNRESOLVED CONFLICT — not Authorized",
        ),
        "src/onebrain-web/src/api/client.ts": (
            '"single_use_receipt":"[REDACTED]"',
        ),
        "src/onebrain-web/src/pages/Settings.tsx": (
            "Compiled",
            "Requested",
            "Active",
            "Kill switch",
            "Signer ready",
        ),
        "src/onebrain-desktop/src/commands.rs": (
            "shutdown_network().await",
            "shutdown_node(state.node.get().cloned()).await;\n    app.restart()",
            "shutdown_node(state.node.get().cloned()).await;\n    app.exit(0)",
        ),
        "src/onebrain-desktop/src/tray.rs": (
            "crate::commands::shutdown_node(node).await;\n                    app.exit(0)",
        ),
    }
    for relative, needles in source_contract.items():
        text = read(ROOT / relative)
        for needle in needles:
            if needle not in text:
                raise ContractError(
                    f"vNext Desktop/Web implementation evidence missing: "
                    f"{relative}: {needle}"
                )

    return len(vectors)


def validate_vnext_dr_m5_baseline(
    profile: dict[str, object] | None = None,
) -> tuple[int, int]:
    if profile is None:
        try:
            profile = json.loads(read(DR_M5_BASELINE_PROFILE))
        except json.JSONDecodeError as error:
            raise ContractError(f"invalid DR-M5 baseline JSON: {error}") from error

    if (
        profile.get("format") != "onebrain/dr-m5-baseline/1"
        or profile.get("profile_id")
        != "DISTRIBUTED_RUNTIME_HARDENING_BASELINE_V1"
        or profile.get("version") != 1
    ):
        raise ContractError("unexpected DR-M5 baseline profile")

    if profile.get("runtime_change_globs") != ["src/**"]:
        raise ContractError("DR-M5 runtime path trigger drift")

    expected_steps = {
        "Compile product entrypoints with the runtime feature",
        "M2 authenticated OBP-RP over real QUIC",
        "M3 private-vault-rehydrated distributed KQL over real peers",
        "M4 strong Public Use consent and metabolic view over real peers",
        "P1.5 authenticated route and local authority boundary",
        "P2.1 node-owned product runtime aggregate",
        "P2.2 independent product lanes and hard budgets",
        "P2.3 ordered lifecycle and partial-start rollback",
        "P2.4 durable selector/type incremental processing",
        "P2.5 cloneable service handles and concurrency fence",
        "P3.1 authenticated REST API contract and real-runtime flow",
        "P3.2 private WebSocket isolation and backpressure",
        "P3.3 authenticated CLI contract and real-runtime replay",
        "Node-owned real QUIC lifecycle",
    }
    gate = profile.get("real_quic_gate")
    required_steps = gate.get("required_steps") if isinstance(gate, dict) else None
    if (
        not isinstance(gate, dict)
        or gate.get("workflow_path") != ".github/workflows/vnext-foundation.yml"
        or gate.get("job_id") != "vnext-network-runtime"
        or not isinstance(gate.get("timeout_minutes"), int)
        or not 0 < gate["timeout_minutes"] <= 45
        or not isinstance(required_steps, list)
        or any(not isinstance(step, str) for step in required_steps)
        or set(required_steps) != expected_steps
    ):
        raise ContractError("DR-M5 real-QUIC gate drift")

    expected_phases = [
        "before_begin_write",
        "after_begin_write_before_mutation",
        "after_mutation_before_commit",
        "after_commit_before_next_side_effect",
        "after_next_side_effect_before_ack",
    ]
    if profile.get("failpoint_phases") != expected_phases:
        raise ContractError("DR-M5 failpoint phase vocabulary drift")

    expected_fields = {
        "accepted_object_cids",
        "accepted_event_cids",
        "selector_inventory_roots",
        "reconciliation_journals",
        "pending_outbox",
        "authority_decisions",
        "private_need_records",
        "distributed_kql_matches",
        "prepared_public_use",
        "public_use_publications",
        "metabolic_views",
    }
    oracle = profile.get("invariant_oracle")
    oracle_fields = oracle.get("fields") if isinstance(oracle, dict) else None
    if (
        not isinstance(oracle, dict)
        or oracle.get("format") != "onebrain/dr-m5-oracle/1"
        or oracle.get("canonicalization")
        != "json-sort-keys-no-whitespace-utf8"
        or oracle.get("digest_algorithm") != "sha256"
        or not isinstance(oracle_fields, list)
        or any(not isinstance(field, str) for field in oracle_fields)
        or set(oracle_fields) != expected_fields
    ):
        raise ContractError("DR-M5 invariant oracle drift")

    expected_boundaries = {
        "TX-PUSE-000",
        "TX-PUSE-001",
        "TX-PUSE-002",
        "TX-OUT-001",
        "TX-OUT-002",
        "TX-JRN-001",
        "TX-VAL-001",
        "TX-INV-001",
        "TX-AUTH-001",
        "TX-KQL-000",
        "TX-KQL-001",
        "TX-POMV-001",
        "TX-POMV-002",
    }
    boundaries = profile.get("transaction_boundaries")
    if not isinstance(boundaries, list):
        raise ContractError("DR-M5 transaction boundary inventory missing")
    boundary_ids: list[str] = []
    covered_oracle_fields: set[str] = set()
    for boundary in boundaries:
        if not isinstance(boundary, dict):
            raise ContractError("invalid DR-M5 transaction boundary row")
        boundary_id = boundary.get("id")
        owner = boundary.get("durable_owner")
        components = boundary.get("oracle_components")
        if (
            not isinstance(boundary_id, str)
            or not isinstance(owner, str)
            or not owner.strip()
            or not isinstance(components, list)
            or not components
            or any(not isinstance(component, str) for component in components)
            or any(component not in expected_fields for component in components)
        ):
            raise ContractError("invalid DR-M5 transaction boundary contract")
        boundary_ids.append(boundary_id)
        covered_oracle_fields.update(components)
    if (
        set(boundary_ids) != expected_boundaries
        or len(boundary_ids) != len(set(boundary_ids))
    ):
        raise ContractError("DR-M5 transaction boundary ID drift")
    if covered_oracle_fields != expected_fields:
        raise ContractError("DR-M5 transaction boundaries do not cover the oracle")

    specimen = profile.get("empty_oracle_specimen")
    if not isinstance(specimen, dict) or not isinstance(specimen.get("snapshot"), dict):
        raise ContractError("DR-M5 empty oracle specimen missing")
    snapshot = specimen["snapshot"]
    if (
        snapshot.get("format") != "onebrain/dr-m5-oracle/1"
        or snapshot.get("version") != 1
        or set(snapshot) != expected_fields | {"format", "version"}
        or any(snapshot.get(field) != [] for field in expected_fields)
    ):
        raise ContractError("DR-M5 empty oracle snapshot drift")
    canonical = json.dumps(
        snapshot, ensure_ascii=False, separators=(",", ":"), sort_keys=True
    ).encode("utf-8")
    if specimen.get("sha256") != hashlib.sha256(canonical).hexdigest():
        raise ContractError("DR-M5 empty oracle digest drift")

    inventory = read(DR_M5_TRANSACTION_INVENTORY)
    for boundary_id in expected_boundaries:
        if f"| `{boundary_id}` |" not in inventory:
            raise ContractError(
                f"DR-M5 boundary missing from transaction inventory: {boundary_id}"
            )
    for index, phase in enumerate(expected_phases, start=1):
        if f"{index}. `{phase}`" not in inventory:
            raise ContractError(f"DR-M5 failpoint phase missing from inventory: {phase}")

    workflow = read(VNEXT_FOUNDATION_WORKFLOW)
    if workflow.count('- "src/**"') < 2:
        raise ContractError("DR-M5 src/** trigger missing from PR or push workflow")
    job_match = re.search(
        r"(?ms)^  vnext-network-runtime:\s*$"
        r"(?P<body>.*?)(?=^  [A-Za-z0-9_-]+:\s*$|\Z)",
        workflow,
    )
    if not job_match:
        raise ContractError("DR-M5 real-QUIC workflow job missing")
    job = job_match.group("body")
    timeout_match = re.search(r"(?m)^\s+timeout-minutes:\s*(\d+)\s*$", job)
    if (
        not timeout_match
        or int(timeout_match.group(1)) != gate["timeout_minutes"]
        or any(f"- name: {step}" not in job for step in expected_steps)
    ):
        raise ContractError("DR-M5 real-QUIC workflow contract drift")
    if "python -m unittest scripts.ci.test_validate_vnext_dr_m5_baseline" not in workflow:
        raise ContractError("DR-M5 baseline mutation tests missing from CI")

    return len(boundary_ids), len(expected_fields)


def validate_vnext_dr_m5_resource_admission(
    profile: dict[str, object] | None = None,
) -> tuple[int, int, int]:
    if profile is None:
        try:
            profile = json.loads(read(DR_M5_RESOURCE_PROFILE))
        except json.JSONDecodeError as error:
            raise ContractError(f"invalid M5-01 resource profile JSON: {error}") from error

    if (
        profile.get("format") != "onebrain/dr-m5-resource-admission/1"
        or profile.get("profile_id") != "UNIFIED_RESOURCE_ADMISSION_AND_FAIRNESS_V1"
        or profile.get("version") != 1
        or profile.get("claims_network_completion") is not False
    ):
        raise ContractError("unexpected M5-01 resource admission profile")

    expected_pipeline = [
        "stream_read",
        "frame",
        "protocol",
        "journal",
        "application",
    ]
    if profile.get("pipeline") != expected_pipeline:
        raise ContractError("M5-01 ordered admission pipeline drift")

    expected_lanes = [
        {
            "id": "session_control",
            "max_bytes": 262_144,
            "prefix_checked_before_allocation": False,
        },
        {
            "id": "carrier_frame",
            "max_bytes": 4_194_304,
            "prefix_checked_before_allocation": True,
        },
        {
            "id": "protocol_payload",
            "max_bytes": 1_048_576,
            "prefix_checked_before_allocation": False,
        },
    ]
    if profile.get("allocation_lanes") != expected_lanes:
        raise ContractError("M5-01 allocation lane contract drift")

    expected_quotas = {
        "handshakes_global": 128,
        "handshakes_per_ip": 8,
        "sessions_global": 64,
        "sessions_per_ip": 8,
        "sessions_per_node_id": 4,
        "contexts_per_session": 64,
        "replay_entries": 65_536,
        "rate_window_seconds": 60,
        "records_per_session": 4_096,
        "bytes_per_session": 16_777_216,
        "work_per_session": 1_000_000,
        "records_per_node_id_window": 8_192,
        "bytes_per_node_id_window": 16_777_216,
        "work_per_node_id_window": 2_000_000,
        "per_ip_window_derivation": "per_node_id_window_times_sessions_per_ip",
        "global_window_derivation": "per_node_id_window_times_sessions_global",
    }
    if profile.get("default_quotas") != expected_quotas:
        raise ContractError("M5-01 identity or rate quota drift")

    expected_state_bounds = {
        "proposal_quarantine_records": 65_536,
        "accepted_records": 65_536,
        "verified_quarantine_records": 65_536,
        "inventory_records": 65_536,
        "provenance_observations": 262_144,
        "typed_provenance_records": 65_536,
        "typed_provenance_prefixes": 65_536,
        "source_peers_per_record": 64,
        "distributed_kql_matches": 65_536,
        "outbox_records": 65_536,
        "outbox_tombstones": 65_536,
        "storage_soft_watermark_bytes": 536_870_912,
        "storage_hard_watermark_bytes": 1_073_741_824,
    }
    if profile.get("bounded_durable_state") != expected_state_bounds:
        raise ContractError("M5-01 durable-state bound drift")

    scans = profile.get("incremental_scans")
    if scans != {
        "index": "selector_kind_type_prefix",
        "cursor": "monotonic_sequence",
        "max_page_records": 4_096,
        "consumers": ["distributed_kql", "distributed_pomv"],
    }:
        raise ContractError("M5-01 incremental scan contract drift")

    outbox = profile.get("outbox")
    if outbox != {
        "states": ["Pending", "Acknowledged", "DeadLetter", "RetryExhausted"],
        "fair_cursor": "persisted_round_robin",
        "transport_counter": "transport_attempts",
        "validation_counter": "validation_retries",
        "terminal_order": "terminal_sequence",
        "terminal_compaction": "tombstone_before_payload_delete",
        "max_records_inspected_per_quantum": 65_536,
    }:
        raise ContractError("M5-01 fair outbox contract drift")

    expected_oracles = [
        "flood_peer_bounded_with_constant_controller_overhead",
        "healthy_peer_progresses_within_finite_quanta",
        "retry_exhausted_prefix_cannot_starve_pending_work",
    ]
    if profile.get("exit_oracles") != expected_oracles:
        raise ContractError("M5-01 exit oracle drift")

    source_contract = {
        "src/ku-net/src/vnext_resource_gate.rs": (
            "pub fn admit_length_prefix",
            "pub struct RuntimeAdmissionController",
            "pub fn try_begin_handshake",
            "pub fn begin_record",
            "fn rejected_flood_does_not_grow_identity_maps",
        ),
        "src/ku-net/src/transport.rs": ("recv_length_prefixed_uni", "read_exact(&mut prefix)"),
        "src/ku-net/src/vnext_session.rs": (
            "DEFAULT_SESSION_REPLAY_ENTRIES",
            "pub fn with_capacity",
        ),
        "src/onebrain-node/src/vnext_outbox.rs": (
            "DeadLetter",
            "RetryExhausted",
            "pending_fair",
            "transport_attempts",
            "validation_retries",
            "compact_terminal",
            "exhausted_first_page_cannot_starve_healthy_pending_work",
        ),
        "src/onebrain-node/src/vnext_record_provenance.rs": (
            "MAX_TYPED_DELTA_PAGE_RECORDS",
            "MAX_SOURCE_PEERS_PER_RECORD",
            "typed_delta",
        ),
        "src/ku-core/src/foundation/storage.rs": (
            "MAX_VERIFIED_ACCEPTED_RECORDS",
            "MAX_VERIFIED_QUARANTINE_RECORDS",
        ),
    }
    for relative, needles in source_contract.items():
        text = read(ROOT / relative)
        for needle in needles:
            if needle not in text:
                raise ContractError(
                    f"M5-01 implementation evidence missing: {relative}: {needle}"
                )

    spec = read(VNEXT / "UNIFIED_RESOURCE_ADMISSION_AND_FAIRNESS_V1.md")
    if "dr-m5-resource-admission-v1.json" not in spec:
        raise ContractError("M5-01 normative profile is not linked to machine contract")
    workflow = read(VNEXT_FOUNDATION_WORKFLOW)
    if (
        "- name: M5.1 unified resource admission and fair outbox" not in workflow
        or "python -m unittest scripts.ci.test_validate_vnext_dr_m5_resource_admission"
        not in workflow
    ):
        raise ContractError("M5-01 CI acceptance gate missing")

    return len(expected_lanes), len(expected_state_bounds), len(expected_oracles)


def validate_vnext_dr_m5_observability(
    profile: dict[str, object] | None = None,
) -> tuple[int, int, int]:
    if profile is None:
        try:
            profile = json.loads(read(DR_M5_OBSERVABILITY_PROFILE))
        except json.JSONDecodeError as error:
            raise ContractError(f"invalid M5-02 observability profile JSON: {error}") from error

    if (
        profile.get("format") != "onebrain/dr-m5-observability/1"
        or profile.get("profile_id") != "STRUCTURED_OBSERVABILITY_PROFILE_V1"
        or profile.get("version") != 1
        or profile.get("profile_major") != 1
        or profile.get("claims_network_completion") is not False
    ):
        raise ContractError("unexpected M5-02 observability profile")

    expected_reasons = [
        "ACCEPTED_NEW",
        "ALREADY_PRESENT",
        "REPLAYED",
        "DEFERRED_MISSING_DEPENDENCY",
        "DEFERRED_BUDGET",
        "QUARANTINED_INVALID",
        "REJECTED_CONTEXT_BINDING",
        "REJECTED_SELECTOR",
        "REJECTED_LENGTH",
        "REJECTED_CONTENT_CID",
        "REJECTED_SINK",
        "REJECTED_AUTHORITY",
        "REJECTED_STORAGE",
        "REJECTED_RATE_LIMIT",
        "REJECTED_REPLAY",
        "REJECTED_SESSION",
        "REJECTED_PROTOCOL",
        "JOURNAL_FAILURE",
        "OUTBOX_RETRY_EXHAUSTED",
        "POMV_IDENTITY_CONFLICT",
        "REGISTRY_FALLBACK",
        "TRANSPORT_FAILURE",
    ]
    if profile.get("reason_codes") != expected_reasons:
        raise ContractError("M5-02 typed reason-code inventory drift")

    expected_outcomes = [
        "accepted_new",
        "already_present",
        "replayed",
        "deferred",
        "quarantined",
        "rejected",
    ]
    if profile.get("outcome_counters") != expected_outcomes:
        raise ContractError("M5-02 outcome counter inventory drift")

    max_u64 = 18_446_744_073_709_551_615
    resources = profile.get("resource_metrics")
    if resources != {
        "counters": ["admitted_bytes", "admitted_work_units", "rate_limited"],
        "record_bytes_inclusive_upper_bounds": [
            64,
            1_024,
            4_096,
            16_384,
            65_536,
            262_144,
            1_048_576,
            max_u64,
        ],
        "work_units_inclusive_upper_bounds": [1, 2, 4, 8, 16, 64, 256, max_u64],
    }:
        raise ContractError("M5-02 resource metric or finite bucket drift")

    expected_gauges = [
        "active_journals",
        "pending_outbox",
        "retry_exhausted_outbox",
        "oldest_pending_outbox_age_seconds",
    ]
    if profile.get("runtime_gauges") != expected_gauges:
        raise ContractError("M5-02 runtime gauge inventory drift")
    if profile.get("journal_age_seconds_inclusive_upper_bounds") != [
        0,
        1,
        5,
        30,
        60,
        300,
        900,
        max_u64,
    ]:
        raise ContractError("M5-02 journal age bucket drift")

    reconciliation = profile.get("reconciliation")
    if reconciliation != {
        "counters": [
            "selector_scans",
            "partial_selector_scans",
            "assessed_frontier_items",
        ],
        "latest_gauge": "latest_lag_records",
        "lag_records_inclusive_upper_bounds": [0, 1, 2, 4, 8, 16, 64, max_u64],
    }:
        raise ContractError("M5-02 reconciliation metric drift")
    if profile.get("pomv") != ["identity_conflicts", "latest_view_revision"]:
        raise ContractError("M5-02 PoMV metric drift")
    if profile.get("registry_states") != [
        "UNKNOWN",
        "DISABLED",
        "LOADED",
        "FALLBACK_V1",
    ]:
        raise ContractError("M5-02 registry-state inventory drift")

    logging = profile.get("structured_logging")
    if logging != {
        "target": "onebrain::vnext::observability",
        "required_fields": ["reason_code", "count", "bytes", "work_units"],
        "free_form_identity_fields": False,
        "swallowed_adversarial_errors": False,
    }:
        raise ContractError("M5-02 structured logging contract drift")

    expected_forbidden_labels = [
        "node_id",
        "peer_id",
        "selector",
        "feed_id",
        "object_cid",
        "event_cid",
        "standing_need_id",
        "private_need",
        "local_query",
    ]
    privacy = profile.get("privacy")
    if privacy != {
        "forbidden_metric_labels": expected_forbidden_labels,
        "contains_high_cardinality_labels": False,
        "contains_private_need_labels": False,
    }:
        raise ContractError("M5-02 privacy label firewall drift")

    if profile.get("operator_snapshot") != {
        "rest_method": "GET",
        "rest_path": "/api/vnext/runtime/status",
        "field": "observability",
        "authenticated_local": True,
        "claims_network_completion": False,
    }:
        raise ContractError("M5-02 operator snapshot contract drift")

    expected_oracles = [
        "every_adversarial_outcome_has_one_typed_reason_transition",
        "exact_counter_transitions_are_reproducible",
        "operator_snapshot_contains_no_private_or_high_cardinality_labels",
        "status_never_claims_network_completeness",
    ]
    if profile.get("exit_oracles") != expected_oracles:
        raise ContractError("M5-02 exit oracle drift")

    source_contract = {
        "src/onebrain-node/src/vnext_observability.rs": (
            "pub const REASON_CODE_COUNT: usize = 22",
            "pub fn record_count",
            "pub fn begin_journal",
            "pub fn observe_outbox",
            "pub fn observe_selector_coverage",
            "serialized_snapshot_has_no_identity_selector_or_private_need_label_surface",
        ),
        "src/onebrain-node/src/vnext_outbox.rs": (
            "const MAGIC_V3",
            "pub fn stats",
            "oldest_pending_age_seconds",
            "legacy_v1_and_v2_intents_decode_with_unknown_age",
        ),
        "src/onebrain-node/src/vnext_network_runtime.rs": (
            "observable_resource_admission_error",
            "payload_reject_reason",
            "admission_and_payload_failures_map_to_stable_low_cardinality_reasons",
            "two_runtime_listeners_authenticate_and_reject_unvalidated_payload_bytes",
        ),
        "src/onebrain-node/src/vnext_product_runtime.rs": (
            "observe_selector_coverage",
            "observe_pomv",
            "observe_registry_state",
        ),
        "src/onebrain-api/src/vnext_api.rs": (
            "pub observability: onebrain_node::VNextObservabilitySnapshot",
            'body["data"]["observability"]["contains_high_cardinality_labels"]',
        ),
    }
    for relative, needles in source_contract.items():
        text = read(ROOT / relative)
        for needle in needles:
            if needle not in text:
                raise ContractError(
                    f"M5-02 implementation evidence missing: {relative}: {needle}"
                )

    if "Err(_)" in read(ROOT / "src/onebrain-node/src/vnext_network_runtime.rs"):
        raise ContractError("M5-02 network runtime still swallows an untyped error")
    spec = read(VNEXT / "STRUCTURED_OBSERVABILITY_PROFILE_V1.md")
    if "dr-m5-observability-v1.json" not in spec:
        raise ContractError("M5-02 normative profile is not linked to machine contract")
    workflow = read(VNEXT_FOUNDATION_WORKFLOW)
    if (
        "- name: M5.2 typed observability and operator snapshot" not in workflow
        or "python -m unittest scripts.ci.test_validate_vnext_dr_m5_observability"
        not in workflow
    ):
        raise ContractError("M5-02 CI acceptance gate missing")

    return len(expected_reasons), len(expected_gauges), len(expected_oracles)


def validate_vnext_dr_m5_crash_harness(
    profile: dict[str, object] | None = None,
) -> tuple[int, int, int, int]:
    if profile is None:
        try:
            profile = json.loads(read(DR_M5_CRASH_HARNESS_PROFILE))
        except json.JSONDecodeError as error:
            raise ContractError(f"invalid M5-03 crash-harness profile JSON: {error}") from error

    if (
        profile.get("format") != "onebrain/dr-m5-crash-harness/1"
        or profile.get("profile_id") != "REAL_REDB_PROCESS_CRASH_HARNESS_V1"
        or profile.get("version") != 1
    ):
        raise ContractError("unexpected M5-03 crash-harness profile")

    feature = profile.get("feature")
    if feature != {
        "name": "vnext-crash-harness",
        "default_enabled": False,
        "kill_switch_env": "ONEBRAIN_DR_M5_FAILPOINTS_ENABLED",
        "kill_switch_value": "1",
        "requires_exact_failpoint": True,
        "requires_fsynced_marker": True,
        "requires_per_case_token": True,
    }:
        raise ContractError("M5-03 default-off failpoint firewall drift")

    process_kill = profile.get("process_kill")
    if process_kill != {
        "worker": "vnext_crash_harness::tests::dr_m5_process_kill_worker",
        "parent_action": "wait_for_fsynced_marker_then_kill_child",
        "marker_timeout_seconds": 10,
        "child_success_is_failure": True,
        "restart_uses_open_not_create": True,
    }:
        raise ContractError("M5-03 child-process kill contract drift")

    expected_boundaries = [
        "TX-PUSE-000",
        "TX-PUSE-001",
        "TX-PUSE-002",
        "TX-OUT-001",
        "TX-OUT-002",
        "TX-JRN-001",
        "TX-VAL-001",
        "TX-INV-001",
        "TX-AUTH-001",
        "TX-KQL-000",
        "TX-KQL-001",
        "TX-POMV-001",
        "TX-POMV-002",
    ]
    if profile.get("boundaries") != expected_boundaries:
        raise ContractError("M5-03 boundary inventory drift")
    expected_phases = [
        "before_begin_write",
        "after_begin_write_before_mutation",
        "after_mutation_before_commit",
        "after_commit_before_next_side_effect",
        "after_next_side_effect_before_ack",
    ]
    if profile.get("phases") != expected_phases:
        raise ContractError("M5-03 failpoint phase inventory drift")
    expected_case_count = len(expected_boundaries) * len(expected_phases)
    if profile.get("required_process_kill_cases") != expected_case_count:
        raise ContractError("M5-03 process-kill case count drift")

    expected_oracle_fields = [
        "accepted_object_cids",
        "accepted_event_cids",
        "selector_inventory_roots",
        "reconciliation_journals",
        "pending_outbox",
        "authority_decisions",
        "private_need_records",
        "distributed_kql_matches",
        "prepared_public_use",
        "public_use_publications",
        "metabolic_views",
    ]
    oracle = profile.get("oracle")
    if oracle != {
        "format": "onebrain/dr-m5-oracle/1",
        "canonicalization": "json-sort-keys-no-whitespace-utf8",
        "digest_algorithm": "sha256",
        "fields": expected_oracle_fields,
        "expected_complete_fixture_sha256": (
            "9c312d251b2347c65149f16fd6a55327cd962ee8d5806bb5bcb642648d9c4aeb"
        ),
    }:
        raise ContractError("M5-03 recovery oracle or digest drift")

    expected_case_fields = [
        "boundary",
        "phase",
        "child_exit",
        "restart_result",
        "oracle_sha256",
        "canonical_rows",
        "side_effect_rows",
        "ack_rows",
        "exact_replay_digest",
    ]
    report = profile.get("report")
    if report != {
        "format": "onebrain/dr-m5-crash-report/1",
        "required_case_fields": expected_case_fields,
        "expected_sha256": (
            "9457130a211e12924c5e6322631a0b6c8ac811de90f67c435a2fd0ed11ed4dcd"
        ),
        "claims_network_completion": False,
    }:
        raise ContractError("M5-03 crash report schema, digest, or claim drift")

    expected_faults = ["disk_full", "read_only", "corrupt_store", "truncated_store"]
    if profile.get("storage_faults") != expected_faults:
        raise ContractError("M5-03 storage-fault inventory drift")
    expected_invariants = [
        "accepted_and_pending_state_is_not_lost",
        "replay_does_not_add_rows_or_change_oracle_digest",
        "authority_decision_remains_fail_closed_without_amplification",
        "corrupt_or_truncated_store_fails_explicitly_without_recreation",
    ]
    if profile.get("recovery_invariants") != expected_invariants:
        raise ContractError("M5-03 recovery invariant inventory drift")

    expected_hook_paths = {
        "TX-PUSE-000": "src/onebrain-node/src/vnext_distributed_pomv.rs",
        "TX-PUSE-001": "src/onebrain-node/src/vnext_distributed_pomv.rs",
        "TX-PUSE-002": "src/onebrain-node/src/vnext_distributed_pomv.rs",
        "TX-OUT-001": "src/onebrain-node/src/vnext_outbox.rs",
        "TX-OUT-002": "src/onebrain-node/src/vnext_outbox.rs",
        "TX-JRN-001": "src/ku-net/src/vnext_reconciliation_journal.rs",
        "TX-VAL-001": "src/ku-core/src/foundation/storage.rs",
        "TX-INV-001": "src/ku-net/src/vnext_inventory_forest.rs",
        "TX-AUTH-001": "src/ku-core/src/foundation/storage.rs",
        "TX-KQL-000": "src/ku-kql/src/vnext_private_need.rs",
        "TX-KQL-001": "src/onebrain-node/src/vnext_distributed_kql.rs",
        "TX-POMV-001": "src/onebrain-node/src/vnext_distributed_pomv.rs",
        "TX-POMV-002": "src/onebrain-node/src/vnext_distributed_pomv.rs",
    }
    hooks = profile.get("owner_hooks")
    if not isinstance(hooks, list) or len(hooks) != len(expected_hook_paths):
        raise ContractError("M5-03 owner-hook inventory drift")
    observed_hooks: dict[str, str] = {}
    for hook in hooks:
        if not isinstance(hook, dict):
            raise ContractError("M5-03 invalid owner-hook row")
        boundary = hook.get("boundary")
        path = hook.get("path")
        if not isinstance(boundary, str) or not isinstance(path, str):
            raise ContractError("M5-03 invalid owner-hook binding")
        if boundary in observed_hooks:
            raise ContractError(f"M5-03 duplicate owner hook: {boundary}")
        observed_hooks[boundary] = path
    if observed_hooks != expected_hook_paths:
        raise ContractError("M5-03 owner-hook path binding drift")
    for boundary, relative in observed_hooks.items():
        source = read(ROOT / relative)
        if boundary not in source:
            raise ContractError(f"M5-03 owner hook missing: {relative}: {boundary}")
        for phase in expected_phases:
            if phase not in source:
                raise ContractError(f"M5-03 phase hook missing: {relative}: {phase}")

    failpoint_source = read(ROOT / "src/ku-core/src/foundation/dr_m5_failpoint.rs")
    for needle in (
        "file.sync_all()",
        ".create_new(true)",
        "ONEBRAIN_DR_M5_FAILPOINTS_ENABLED",
        "ONEBRAIN_DR_M5_TOKEN",
    ):
        if needle not in failpoint_source:
            raise ContractError(f"M5-03 authenticated failpoint evidence missing: {needle}")
    harness_source = read(ROOT / "src/onebrain-node/src/vnext_crash_harness.rs")
    for needle in (
        "child.kill()",
        "Database::open(path)",
        "child_process_kill_matrix_recovers_exactly_once_with_stable_oracle",
        "disk_full_and_read_only_faults_are_explicit_and_non_mutating",
        "corrupt_and_truncated_store_fail_explicitly_without_recreation",
        "DR_M5_CRASH_REPORT_SHA256",
        "claims_network_completion: false",
    ):
        if needle not in harness_source:
            raise ContractError(f"M5-03 harness implementation evidence missing: {needle}")

    node_manifest = read(ROOT / "src/onebrain-node/Cargo.toml")
    if (
        'default = ["base-v1"]' not in node_manifest
        or "vnext-crash-harness = [" not in node_manifest
        or '"ku-core/dr-m5-crash-harness"' not in node_manifest
        or '"ku-net/dr-m5-crash-harness"' not in node_manifest
        or '"ku-kql/dr-m5-crash-harness"' not in node_manifest
    ):
        raise ContractError("M5-03 feature firewall wiring missing")

    inventory = read(DR_M5_TRANSACTION_INVENTORY)
    if "M5-03 process-kill coverage complete" not in inventory:
        raise ContractError("M5-03 transaction inventory status is stale")
    spec = read(VNEXT / "REAL_REDB_PROCESS_CRASH_HARNESS_V1.md")
    if "dr-m5-crash-harness-v1.json" not in spec:
        raise ContractError("M5-03 normative profile is not linked to machine contract")
    workflow = read(VNEXT_FOUNDATION_WORKFLOW)
    if (
        "- name: M5.3 real Redb process-kill crash matrix" not in workflow
        or "python -m unittest scripts.ci.test_validate_vnext_dr_m5_crash_harness"
        not in workflow
        or "--features vnext-crash-harness" not in workflow
    ):
        raise ContractError("M5-03 CI acceptance gate missing")

    return (
        len(expected_boundaries),
        len(expected_phases),
        expected_case_count,
        len(expected_faults),
    )


def validate_vnext_dr_m5_chaos_fuzz(
    profile: dict[str, object] | None = None,
) -> tuple[int, int, int, int, int]:
    if profile is None:
        try:
            profile = json.loads(read(DR_M5_CHAOS_FUZZ_PROFILE))
        except json.JSONDecodeError as error:
            raise ContractError(f"invalid M5-04 profile JSON: {error}") from error
    if (
        profile.get("format") != "onebrain/dr-m5-chaos-fuzz/1"
        or profile.get("profile_id") != "CHAOS_AND_FUZZ_PROFILE_V1"
        or profile.get("version") != 1
    ):
        raise ContractError("M5-04 profile identity drift")

    feature = profile.get("feature")
    if feature != {"name": "vnext-chaos-harness", "default_enabled": False}:
        raise ContractError("M5-04 feature firewall drift")

    expected_chaos = [
        "drop",
        "duplicate",
        "delay",
        "reorder",
        "disconnect",
        "partition_reunion",
        "slow_reader_writer",
    ]
    real_quic = profile.get("real_quic")
    if not isinstance(real_quic, dict) or (
        real_quic.get("scenarios") != expected_chaos
        or real_quic.get("timeout_seconds") != 15
        or real_quic.get("authenticated") is not True
        or real_quic.get("reconnect_reauthenticates") is not True
        or real_quic.get("fair_redelivery") is not True
    ):
        raise ContractError("M5-04 real-QUIC scenario contract drift")

    expected_floods = [
        "pre_auth",
        "authenticated_sessions",
        "contexts_manifests",
        "unique_invalid_cids",
        "slowloris",
    ]
    flood = profile.get("flood")
    if not isinstance(flood, dict) or (
        flood.get("scenarios") != expected_floods
        or flood.get("pre_auth_attempts") != 20_000
        or flood.get("authenticated_session_attempts") != 1_024
        or flood.get("context_manifest_attempts") != 1_024
        or flood.get("context_limit") != 8
        or flood.get("unique_invalid_cids") != 4_096
        or flood.get("slowloris_deadline_ms") != 75
    ):
        raise ContractError("M5-04 flood bound drift")

    trace = profile.get("property_trace")
    expected_oracle = (
        "a93a054ece2eabd5afacaaa21a233137a1987c82d646a6e1138598dc225c5a53"
    )
    if not isinstance(trace, dict) or (
        trace.get("seeds") != 64
        or trace.get("steps_per_seed") != 4_096
        or trace.get("record_count") != 64
        or trace.get("oracle_algorithm") != "blake3"
        or trace.get("expected_oracle_blake3") != expected_oracle
        or trace.get("claims_network_completion") is not False
        or trace.get("grants_authority") is not False
    ):
        raise ContractError("M5-04 long-trace oracle drift")

    expected_targets = [
        "canonical_codec",
        "session_reconciliation_codec",
        "carrier_frame",
        "journal_token_snapshot",
        "domain_records",
        "legacy_adapter",
    ]
    fuzz = profile.get("fuzz")
    expected_corpus_digest = (
        "465d554e235738511b69e37c33c0b5e6fcccbc09f8b30e010d7d3eac916c66fd"
    )
    if not isinstance(fuzz, dict) or (
        fuzz.get("cargo_fuzz_version") != "0.13.2"
        or fuzz.get("libfuzzer_sys_version") != "0.4.13"
        or fuzz.get("max_input_bytes") != 4_096
        or fuzz.get("targets") != expected_targets
        or fuzz.get("pr_corpus_seeds_per_target") != 3
        or fuzz.get("required_pr_corpus_cases") != 18
        or fuzz.get("corpus_manifest_sha256") != expected_corpus_digest
    ):
        raise ContractError("M5-04 fuzz target/corpus contract drift")

    nightly = profile.get("nightly")
    if not isinstance(nightly, dict) or nightly != {
        "workflow": ".github/workflows/vnext-fuzz-nightly.yml",
        "rust_toolchain": "nightly-2026-07-20",
        "max_total_time_seconds_per_target": 60,
        "timeout_seconds_per_input": 10,
        "matrix_targets": 6,
        "artifact_retention_days": 14,
    }:
        raise ContractError("M5-04 nightly budget drift")

    expected_exit = [
        "zero_panic_oom_hang_privacy_or_invariant_failure",
        "bounded_state_under_flood",
        "fair_redelivery_same_oracle_root",
        "pr_corpus_smoke_all_targets",
        "versioned_nightly_budget",
    ]
    if profile.get("exit_oracles") != expected_exit:
        raise ContractError("M5-04 exit oracle drift")

    chaos_source = read(ROOT / "src/ku-net/src/vnext_chaos.rs")
    for needle in (
        "real_quic_drop_duplicate_delay_reorder_disconnect_and_reunion_converge",
        "floods_and_slowloris_remain_bounded_without_state_amplification",
        "long_delivery_traces_converge_to_one_oracle_under_fair_redelivery",
        "private-standing-need-must-not-cross-chaos-wire",
        "accepted.extend(source)",
        "claims_network_completion: false",
    ):
        if needle not in chaos_source:
            raise ContractError(f"M5-04 chaos implementation evidence missing: {needle}")

    shared_target_source = read(
        ROOT / "src/onebrain-node/src/vnext_fuzz_targets.rs"
    )
    for needle in (
        "decode_canonical",
        "decode_session_message",
        "decode_reconciliation_message",
        "QuicRecordAdapter::decode",
        "fuzz_decode_journal_token_and_snapshot",
        "decode_knowledge_object",
        "decode_knowledge_event",
        "decode_feed_inception",
        "decode_actor_delegation",
        "decode_actor_revocation",
        "UseEvidencePayload::from_validated_object",
        "legacy::parse_peer_message",
        "assert!(!adapter.grants_vnext_authority())",
    ):
        if needle not in shared_target_source:
            raise ContractError(f"M5-04 parser target evidence missing: {needle}")
    journal_source = read(
        ROOT / "src/ku-net/src/vnext_reconciliation_journal.rs"
    )
    if "validate_token_against(&projection, &token, token_key)" not in journal_source:
        raise ContractError("M5-04 journal token validator fuzz evidence missing")

    fuzz_manifest = read(ROOT / "src/fuzz/Cargo.toml")
    if (
        'libfuzzer-sys = "=0.4.13"' not in fuzz_manifest
        or 'features = ["vnext-chaos-harness"]' not in fuzz_manifest
    ):
        raise ContractError("M5-04 cargo-fuzz manifest/version wiring drift")
    fuzz_lock = read(ROOT / "src/fuzz/Cargo.lock")
    if not re.search(
        r'(?m)^name = "libfuzzer-sys"\nversion = "0\.4\.13"$',
        fuzz_lock,
    ):
        raise ContractError("M5-04 cargo-fuzz lockfile version drift")
    for target in expected_targets:
        relative = f"fuzz_targets/{target}.rs"
        if f'name = "{target}"' not in fuzz_manifest or relative not in fuzz_manifest:
            raise ContractError(f"M5-04 fuzz manifest target missing: {target}")
        target_source = read(ROOT / "src/fuzz" / relative)
        if (
            f'run_target("{target}", data)' not in target_source
            or "MAX_FUZZ_INPUT_BYTES" not in target_source
        ):
            raise ContractError(f"M5-04 fuzz target wrapper drift: {target}")

    corpus_root = ROOT / "src/fuzz/corpus"
    attributes = read(ROOT / ".gitattributes")
    if "src/fuzz/corpus/** -text" not in attributes:
        raise ContractError("M5-04 corpus byte-preservation attribute missing")
    digest = hashlib.sha256()
    digest.update(b"onebrain:dr-m5:fuzz-corpus:1\0")
    corpus_cases = 0

    def digest_field(value: bytes) -> None:
        digest.update(len(value).to_bytes(8, "big"))
        digest.update(value)

    for target in expected_targets:
        digest_field(target.encode("utf-8"))
        directory = corpus_root / target
        if not directory.is_dir():
            raise ContractError(f"M5-04 corpus directory missing: {target}")
        files = sorted(path for path in directory.iterdir() if path.is_file())
        if len(files) != 3:
            raise ContractError(f"M5-04 corpus seed count drift: {target}")
        for path in files:
            data = path.read_bytes()
            if not data or len(data) > 4_096:
                raise ContractError(f"M5-04 corpus seed bound drift: {path.name}")
            digest_field(path.name.encode("utf-8"))
            digest_field(data)
            corpus_cases += 1
    if digest.hexdigest() != expected_corpus_digest:
        raise ContractError("M5-04 corpus manifest digest drift")

    nightly_source = read(ROOT / ".github/workflows/vnext-fuzz-nightly.yml")
    for needle in (
        "RUSTUP_TOOLCHAIN: nightly-2026-07-20",
        'rustup toolchain install "$RUSTUP_TOOLCHAIN" --profile minimal --no-self-update',
        "cargo install cargo-fuzz --version 0.13.2 --locked",
        "-max_total_time=60 -timeout=10 -max_len=4096",
        "retention-days: 14",
    ):
        if needle not in nightly_source:
            raise ContractError(f"M5-04 nightly workflow evidence missing: {needle}")
    for target in expected_targets:
        if f"- {target}" not in nightly_source:
            raise ContractError(f"M5-04 nightly target missing: {target}")

    node_manifest = read(ROOT / "src/onebrain-node/Cargo.toml")
    if (
        'default = ["base-v1"]' not in node_manifest
        or "vnext-chaos-harness = [" not in node_manifest
        or '"ku-net/dr-m5-chaos-harness"' not in node_manifest
        or 'required-features = ["vnext-chaos-harness"]' not in node_manifest
    ):
        raise ContractError("M5-04 feature firewall wiring missing")
    spec = read(VNEXT / "CHAOS_AND_FUZZ_PROFILE_V1.md")
    if "dr-m5-chaos-fuzz-v1.json" not in spec:
        raise ContractError("M5-04 normative profile is not linked to machine contract")
    workflow = read(VNEXT_FOUNDATION_WORKFLOW)
    for needle in (
        "python -m unittest scripts.ci.test_validate_vnext_dr_m5_chaos_fuzz",
        "- name: M5.4 real-QUIC chaos and adversarial flood",
        "- name: M5.4 deterministic parser corpus smoke",
        "--features vnext-chaos-harness",
    ):
        if needle not in workflow:
            raise ContractError(f"M5-04 PR acceptance gate missing: {needle}")

    return (
        len(expected_chaos),
        len(expected_floods),
        len(expected_targets),
        corpus_cases,
        len(expected_exit),
    )


def validate_vnext_dr_m5_operational_compaction(
    profile: dict[str, object] | None = None,
) -> tuple[int, int, int, int, int]:
    if profile is None:
        try:
            profile = json.loads(read(DR_M5_OPERATIONAL_COMPACTION_PROFILE))
        except json.JSONDecodeError as error:
            raise ContractError(f"invalid M5-05 profile JSON: {error}") from error
    if (
        profile.get("format") != "onebrain/dr-m5-operational-compaction/1"
        or profile.get("profile_id") != "OPERATIONAL_COMPACTION_PROFILE_V1"
        or profile.get("version") != 1
    ):
        raise ContractError("M5-05 profile identity drift")

    feature = profile.get("feature")
    if feature != {
        "name": "vnext-compaction-harness",
        "default_enabled": False,
    }:
        raise ContractError("M5-05 feature firewall drift")

    kill_switch = profile.get("kill_switch")
    if kill_switch != {
        "default_enabled": False,
        "generation_fenced": True,
        "stale_permit_commits": False,
    }:
        raise ContractError("M5-05 kill-switch generation fence drift")

    journal = profile.get("journal")
    if journal != {
        "allowed_compactable_states": ["completed", "superseded"],
        "protected_states": [
            "pending",
            "retrying",
            "inflight",
            "missing_dependency",
        ],
        "audit_identity": "full_manifest_digest",
        "exact_canonical_length_required": True,
        "semantic_root_unchanged": True,
    }:
        raise ContractError("M5-05 journal eligibility/protection drift")

    outbox = profile.get("outbox")
    if outbox != {
        "terminal_states": [
            "acknowledged",
            "dead_letter",
            "retry_exhausted",
        ],
        "protected_states": ["pending"],
        "audit_before_delete": True,
        "audit_and_delete_atomic": True,
        "payload_digest": "blake3",
        "max_tombstones": 65_536,
        "physical_disk_decrease_required": True,
    }:
        raise ContractError("M5-05 outbox audit-before-delete contract drift")

    bounded = profile.get("bounded_evidence")
    expected_overflow_fields = [
        "dropped_records",
        "dropped_bytes",
        "chain_root",
        "last_dropped_id",
    ]
    if bounded != {
        "lanes": ["quarantine", "provenance"],
        "max_records_per_lane": 4_096,
        "max_record_bytes": 1_048_576,
        "overflow_fields": expected_overflow_fields,
        "retry_last_overflow_idempotent": True,
    }:
        raise ContractError("M5-05 bounded evidence/overflow contract drift")

    derived = profile.get("derived_snapshots")
    frozen_fixture = {
        "rows": [
            {"key_hex": "01", "value_byte": 1, "value_length": 32},
            {"key_hex": "02", "value_byte": 2, "value_length": 64},
            {"key_hex": "03", "value_byte": 3, "value_length": 96},
        ],
        "kql_source_root_blake3": (
            "0ca08333f6db371de7674d19cb99db26df952b72a87ca6ee37226a9bf0872910"
        ),
        "kql_projection_root_blake3": (
            "230a443d1bd69814e05fb2a2173c4d895556b262b39586204120fa48d8442194"
        ),
        "pomv_source_root_blake3": (
            "7fbcf8ee16d00a0c31391f45dcf0c424387c53dafa2ea77d0a4a37a5f799f689"
        ),
        "pomv_projection_root_blake3": (
            "73f25199ee54961a10dc3585ed28d8fc08e1be432ced37f1f0b3a92582ccc571"
        ),
    }
    if derived != {
        "lanes": ["kql", "pomv"],
        "reducer_version": 1,
        "max_rows": 65_536,
        "max_snapshot_bytes": 16_777_216,
        "canonical_exact_restore": True,
        "fixture": frozen_fixture,
    }:
        raise ContractError("M5-05 derived snapshot/root contract drift")

    expected_boundaries = [
        "TX-CMP-JRN-001",
        "TX-CMP-OUT-001",
        "TX-CMP-QAR-001",
        "TX-CMP-PRV-001",
        "TX-CMP-IDX-001",
    ]
    expected_phases = [
        "before_begin_write",
        "after_begin_write_before_mutation",
        "after_mutation_before_commit",
        "after_commit_before_next_side_effect",
        "after_next_side_effect_before_ack",
    ]
    process_kill = profile.get("process_kill")
    expected_case_count = len(expected_boundaries) * len(expected_phases)
    if not isinstance(process_kill, dict) or (
        process_kill.get("boundaries") != expected_boundaries
        or process_kill.get("phases") != expected_phases
        or process_kill.get("required_process_kill_cases") != expected_case_count
        or process_kill.get("real_redb_reopen") is not True
        or process_kill.get("retry_after_reopen") is not True
    ):
        raise ContractError("M5-05 process-kill matrix drift")

    expected_exit = [
        "exact_root_after_every_crash",
        "pending_work_continues",
        "semantic_result_unchanged",
        "logical_payload_bytes_decrease",
        "physical_disk_bytes_decrease",
    ]
    if profile.get("exit_oracles") != expected_exit:
        raise ContractError("M5-05 exit oracle drift")

    switch_source = read(
        ROOT / "src/ku-core/src/foundation/operational_compaction.rs"
    )
    for needle in (
        "pub struct OperationalCompactionSwitch",
        "AtomicBool::new(false)",
        ".fetch_add(1, Ordering::AcqRel)",
        "pub struct OperationalCompactionPermit",
        "pub fn run_if_current",
        "switch_is_default_off_and_stale_permits_never_revive",
    ):
        if needle not in switch_source:
            raise ContractError(
                f"M5-05 kill-switch implementation evidence missing: {needle}"
            )

    journal_source = read(
        ROOT / "src/ku-net/src/vnext_reconciliation_journal.rs"
    )
    for needle in (
        "compact_completed_manifests",
        "accepted.canonical_length == entry.canonical_length",
        "next.compacted_manifests.insert(*digest)",
        "store_compaction_atomically",
        "m5_05_journal_process_kill_matrix_restores_exact_root",
    ):
        if needle not in journal_source:
            raise ContractError(
                f"M5-05 journal implementation evidence missing: {needle}"
            )

    outbox_source = read(ROOT / "src/onebrain-node/src/vnext_outbox.rs")
    for needle in (
        "pub struct OutboundAuditTombstone",
        "if intent.state.is_terminal()",
        "let mut tombstones",
        "outbox.remove",
        "physical_reclaim_reduces_disk_after_terminal_payload_compaction",
        "m5_05_outbox_process_kill_matrix_restores_exact_root",
    ):
        if needle not in outbox_source:
            raise ContractError(
                f"M5-05 outbox implementation evidence missing: {needle}"
            )

    operational_source = read(
        ROOT / "src/onebrain-node/src/vnext_operational_compaction.rs"
    )
    for needle in (
        "MAX_OPERATIONAL_EVIDENCE_RECORDS: u64 = 4_096",
        "MAX_OPERATIONAL_EVIDENCE_BYTES: usize = 1_048_576",
        "previous.last_dropped_id == id",
        "MAX_DERIVED_SNAPSHOT_ROWS: usize = 65_536",
        "MAX_DERIVED_SNAPSHOT_BYTES: usize = 16 * 1_048_576",
        "frozen_kql_and_pomv_snapshot_roots_match_profile",
        "m5_05_operational_process_kill_matrix_restores_exact_root",
    ):
        if needle not in operational_source:
            raise ContractError(
                f"M5-05 operational implementation evidence missing: {needle}"
            )
    for boundary in expected_boundaries[2:]:
        if boundary not in operational_source:
            raise ContractError(
                f"M5-05 operational boundary evidence missing: {boundary}"
            )

    node_manifest = read(ROOT / "src/onebrain-node/Cargo.toml")
    if (
        'default = ["base-v1"]' not in node_manifest
        or "vnext-compaction-harness = [" not in node_manifest
        or '"ku-core/dr-m5-crash-harness"' not in node_manifest
        or '"ku-net/dr-m5-crash-harness"' not in node_manifest
    ):
        raise ContractError("M5-05 feature firewall wiring missing")

    inventory = read(DR_M5_TRANSACTION_INVENTORY)
    for boundary in expected_boundaries:
        if boundary not in inventory:
            raise ContractError(
                f"M5-05 transaction inventory missing boundary: {boundary}"
            )
    spec = read(VNEXT / "OPERATIONAL_COMPACTION_PROFILE_V1.md")
    if "dr-m5-operational-compaction-v1.json" not in spec:
        raise ContractError("M5-05 normative profile is not linked to machine contract")
    workflow = read(VNEXT_FOUNDATION_WORKFLOW)
    for needle in (
        "python -m unittest scripts.ci.test_validate_vnext_dr_m5_operational_compaction",
        "- name: M5.5 operational compaction and process-kill recovery",
        "--features persist,dr-m5-crash-harness",
        "--features vnext-compaction-harness",
    ):
        if needle not in workflow:
            raise ContractError(f"M5-05 PR acceptance gate missing: {needle}")

    return (
        len(expected_boundaries),
        len(expected_phases),
        expected_case_count,
        len(derived["lanes"]),
        len(expected_exit),
    )


def validate_vnext_dr_m5_mixed_rollback(
    profile: dict[str, object] | None = None,
) -> tuple[int, int, int, int]:
    if profile is None:
        try:
            profile = json.loads(read(DR_M5_MIXED_ROLLBACK_PROFILE))
        except json.JSONDecodeError as error:
            raise ContractError(f"invalid M5-06 profile JSON: {error}") from error
    if (
        profile.get("format") != "onebrain/dr-m5-mixed-rollback/1"
        or profile.get("profile_id") != "MIXED_VERSION_RUNTIME_ROLLBACK_PROFILE_V1"
        or profile.get("version") != 1
    ):
        raise ContractError("M5-06 profile identity drift")

    if profile.get("transports") != {
        "simultaneous": True,
        "legacy": "tcp-json-v1",
        "vnext": "authenticated-quic-obp-rp-v1",
        "real_loopback_required": True,
    }:
        raise ContractError("M5-06 simultaneous real-transport contract drift")

    legacy = profile.get("legacy_n_minus_one")
    if not isinstance(legacy, dict) or (
        legacy.get("fixture_kind") != "frozen-wire-corpus"
        or legacy.get("source_release") != "onebrain-legacy-tcp-json-v1"
        or legacy.get("byte_exact_reserialization") is not True
        or legacy.get("vnext_authority") is not False
    ):
        raise ContractError("M5-06 N-1 fixture contract drift")
    corpus = legacy.get("corpus")
    if not isinstance(corpus, list) or len(corpus) != 3:
        raise ContractError("M5-06 N-1 corpus coverage drift")
    ids: set[str] = set()
    for row in corpus:
        if not isinstance(row, dict):
            raise ContractError("M5-06 N-1 corpus row is not an object")
        fixture_id = row.get("id")
        payload_hex = row.get("payload_hex")
        framed_hex = row.get("framed_hex")
        if (
            not isinstance(fixture_id, str)
            or fixture_id in ids
            or not isinstance(payload_hex, str)
            or not isinstance(framed_hex, str)
        ):
            raise ContractError("M5-06 N-1 corpus identity/hex drift")
        ids.add(fixture_id)
        try:
            payload = bytes.fromhex(payload_hex)
            frame = bytes.fromhex(framed_hex)
        except ValueError as error:
            raise ContractError("M5-06 N-1 corpus contains invalid hex") from error
        if len(frame) < 4 or int.from_bytes(frame[:4], "big") != len(payload):
            raise ContractError("M5-06 N-1 frame length drift")
        if frame[4:] != payload:
            raise ContractError("M5-06 N-1 frame payload drift")
        try:
            decoded = json.loads(payload)
        except (UnicodeDecodeError, json.JSONDecodeError) as error:
            raise ContractError("M5-06 N-1 payload is not frozen JSON") from error
        if not isinstance(decoded, dict) or len(decoded) != 1:
            raise ContractError("M5-06 N-1 legacy message shape drift")

    expected_lanes = [
        "network",
        "distributed_kql_one_hop",
        "public_use_evidence_publish",
        "distributed_pomv_view",
    ]
    generation = profile.get("runtime_generation_fence")
    if generation != {
        "lanes": expected_lanes,
        "startup_config_may_disable": True,
        "startup_config_may_reenable": False,
        "explicit_reenable_required": True,
        "kill_is_idempotent": True,
        "reenable_advances_generation": True,
        "session_rule": "existing-session-must-recheck-before-each-record",
        "inflight_rule": "operation-past-generation-check-may-drain",
    }:
        raise ContractError("M5-06 runtime generation fence drift")

    expected_preserved = [
        "raw",
        "journal",
        "outbox",
        "quarantine",
        "provenance",
        "wallet",
        "obt",
    ]
    rollback = profile.get("rollback")
    if rollback != {
        "atomic_all_lane_disable": True,
        "preserves": expected_preserved,
        "stale_config_reenable": False,
        "legacy_local_offline_available": True,
        "changes_wallet_state": False,
        "changes_obt_state": False,
    }:
        raise ContractError("M5-06 rollback preservation contract drift")

    expected_phases = [
        "before_begin_write",
        "after_begin_write_before_mutation",
        "after_mutation_before_commit",
        "after_commit_before_next_side_effect",
        "after_next_side_effect_before_ack",
    ]
    process_kill = profile.get("process_kill")
    if process_kill != {
        "boundary": "TX-ROL-001",
        "phases": expected_phases,
        "required_process_kill_cases": 5,
        "real_redb_reopen": True,
        "retry_after_reopen": True,
    }:
        raise ContractError("M5-06 process-kill matrix drift")

    expected_exit = [
        "no_new_session_after_network_kill",
        "no_new_publish_after_publication_kill",
        "independent_product_lane_fences",
        "stale_config_cannot_reenable",
        "rollback_preserves_durable_evidence",
        "legacy_and_vnext_real_transports_coexist",
        "wallet_and_obt_unchanged",
    ]
    if profile.get("exit_oracles") != expected_exit:
        raise ContractError("M5-06 exit oracle drift")

    rollout_source = read(
        ROOT / "src/onebrain-node/src/vnext_runtime_rollout.rs"
    )
    for needle in (
        'TableDefinition::new("vnext_runtime_rollout_v1")',
        'const TX_RUNTIME_ROLLBACK: &str = "TX-ROL-001"',
        "Startup may apply a",
        "pub fn kill(",
        "pub fn reenable(",
        "pub fn rollback(",
        "m5_06_runtime_rollback_process_kill_matrix_recovers_exact_generation",
    ):
        if needle not in rollout_source:
            raise ContractError(
                f"M5-06 rollout implementation evidence missing: {needle}"
            )

    product_source = read(
        ROOT / "src/onebrain-node/src/vnext_product_runtime.rs"
    )
    for needle in (
        "kill_runtime_lane",
        "reenable_runtime_lane",
        "rollback_runtime",
        "runtime_kill_rollback_restart_and_explicit_reenable_use_real_quic",
        "VNextRuntimeLane::DistributedKql",
        "VNextRuntimeLane::PublicUseEvidencePublish",
        "VNextRuntimeLane::DistributedPomvView",
    ):
        if needle not in product_source:
            raise ContractError(
                f"M5-06 product fence evidence missing: {needle}"
            )

    network_source = read(
        ROOT / "src/onebrain-node/src/vnext_network_runtime.rs"
    )
    for needle in (
        "network generation changed",
        "generation.is_current()",
        'connection.close("OBP-RP runtime generation fenced")',
    ):
        if needle not in network_source:
            raise ContractError(
                f"M5-06 network generation evidence missing: {needle}"
            )

    mixed_source = read(
        ROOT / "src/onebrain-node/src/vnext_mixed_conformance.rs"
    )
    if "verify_frozen_n_minus_one_wire_corpus" not in mixed_source:
        raise ContractError("M5-06 frozen N-1 executable evidence missing")
    node_test = read(
        ROOT / "src/onebrain-node/tests/vnext_node_runtime.rs"
    )
    if (
        "legacy_tcp_and_vnext_quic_exchange_concurrently_on_real_loopback"
        not in node_test
    ):
        raise ContractError("M5-06 simultaneous transport evidence missing")

    inventory = read(DR_M5_TRANSACTION_INVENTORY)
    if "TX-ROL-001" not in inventory:
        raise ContractError("M5-06 transaction inventory missing TX-ROL-001")
    spec = read(VNEXT / "MIXED_VERSION_RUNTIME_ROLLBACK_PROFILE_V1.md")
    if "dr-m5-mixed-rollback-v1.json" not in spec:
        raise ContractError("M5-06 normative profile is not linked to machine contract")
    workflow = read(VNEXT_FOUNDATION_WORKFLOW)
    for needle in (
        "python -m unittest scripts.ci.test_validate_vnext_dr_m5_mixed_rollback",
        "- name: M5.6 mixed-version runtime rollback and process-kill recovery",
        "--features vnext-crash-harness",
        "--lib vnext_runtime_rollout",
    ):
        if needle not in workflow:
            raise ContractError(f"M5-06 PR acceptance gate missing: {needle}")

    return len(expected_lanes), len(corpus), len(expected_phases), len(expected_exit)


def validate_vnext_soak_runner_kit(
    runner_script: str | None = None,
    runner_guide: str | None = None,
    soak_workflow: str | None = None,
) -> tuple[int, int, int]:
    if runner_script is None:
        runner_script = read(VNEXT_SOAK_RUNNER_SCRIPT)
    if runner_guide is None:
        runner_guide = read(VNEXT_SOAK_RUNNER_GUIDE)
    if soak_workflow is None:
        soak_workflow = read(VNEXT_SOAK_WORKFLOW)

    script_needles = (
        "require_non_root",
        "sha256sum --check --status",
        "--labels \"$RUNNER_LABELS\"",
        "--ephemeral",
        "setup_runner ephemeral",
        "setup-run",
        "./config.sh remove --token",
        "resolve_path \"$RUNNER_HOME\"",
        "realpath -m \"$path\"",
        'rm -rf -- "$RUNNER_HOME"',
        "No inbound firewall port is required",
        "run_privileged",
        "command_exists dnf",
        "command_exists yum",
        "require_supported_distribution",
        "Darwin/arm64",
        'RUNNER_ASSET_ID="osx-arm64"',
        'DEFAULT_RUNNER_LABELS="onebrain-soak-macos-arm64"',
        "shasum -a 256",
        "caffeinate -dimsu",
        "brew install python@3.13 cmake pkgconf",
        '[[ "$HOST_KIND" == "linux-x64" ]] || return 0',
        '[[ "$HOST_KIND" == "macos-arm64" ]] || return 0',
    )
    for needle in script_needles:
        if needle not in runner_script:
            raise ContractError(f"M5-07 portable runner safety missing: {needle}")
    for forbidden in (
        "RUNNER_ALLOW_RUNASROOT",
        'rm -rf -- "$HOME"',
        'rm -rf "$HOME"',
        "sudo ufw",
        "--no-default-labels",
        "--disableupdate",
        'DEFAULT_RUNNER_HOME="${HOME}/Library/Application Support',
    ):
        if forbidden in runner_script:
            raise ContractError(
                f"M5-07 portable runner contains forbidden behavior: {forbidden}"
            )

    guide_needles = (
        "Không cần mở TCP/UDP inbound",
        "sudo ufw allow out 443/tcp",
        "setup-run",
        "pre-release-72h",
        "uninstall",
        "repo public",
        "không chứa SSH key",
        "Rocky/Alma/RHEL",
        "CentOS 7 không được hỗ trợ",
    )
    for needle in guide_needles:
        if needle not in runner_guide:
            raise ContractError(f"M5-07 portable runner guide missing: {needle}")

    workflow_needles = (
        "permissions:\n  contents: read",
        "github.ref == 'refs/heads/main'",
        "runs-on: [self-hosted, linux, x64, onebrain-soak]",
        "save-if: false",
    )
    for needle in workflow_needles:
        if needle not in soak_workflow:
            raise ContractError(
                f"M5-07 self-hosted workflow safety missing: {needle}"
            )
    if "pull_request:" in soak_workflow:
        raise ContractError("M5-07 self-hosted workflow must not run on pull requests")

    return len(script_needles), len(guide_needles), len(workflow_needles)


def validate_concept_registry_runner_kit(
    runner_script: str | None = None,
    runner_guide: str | None = None,
    production_workflow: str | None = None,
    foundation_workflow: str | None = None,
) -> tuple[int, int, int, int]:
    if runner_script is None:
        runner_script = read(CONCEPT_REGISTRY_RUNNER_SCRIPT)
    if runner_guide is None:
        runner_guide = read(CONCEPT_REGISTRY_RUNNER_GUIDE)
    if production_workflow is None:
        production_workflow = read(CONCEPT_REGISTRY_PRODUCTION_WORKFLOW)
    if foundation_workflow is None:
        foundation_workflow = read(VNEXT_FOUNDATION_WORKFLOW)

    runner_needles = (
        'readonly RUNNER_FORMAT="onebrain/concept-registry-runner/1"',
        'readonly TARGET_TRIPLE="x86_64-unknown-linux-gnu"',
        '[[ "$QUALIFICATION_MODE" == "prequalification" || "$QUALIFICATION_MODE" == "release" ]]',
        "verify_base_release_request.py",
        "/usr/bin/python3",
        "/usr/bin/gpg",
        '"previous/input.jsonl"',
        '"previous/concepts.obr"',
        '"previous/release.stamp.json"',
        '"previous/state.json"',
        '"candidate/input.jsonl"',
        '"candidate/concepts.obr"',
        '"candidate/release.stamp.json"',
        '"candidate/state.json"',
        '"environment/runner-image.json"',
        '"environment/rust-toolchain.json"',
        '"environment/registry_probe.sig"',
        '"environment/registry-trust-policy.json"',
        "onebrain:concept-registry-closure:1\\0",
        'readonly REGISTRY_CLOSURE_DIGEST_FILE=',
        "fixture fallback is forbidden",
        "ONEBRAIN_REGISTRY_PRIVATE_KEY_FILE:?external",
        '"base_candidate_bound": False',
        '"registry_production_qualified": False',
        "ccid_stability_diff.py",
        "concept_registry_failure_qualification",
        "concept_registry_production_qualification",
        "release_cycle_qualification",
        "production_qualification.py",
        "raw_report_blake3",
        "_verify_receipt",
        "STAMP_SIGNATURE_DOMAIN",
        'readonly CANDIDATE_RELEASE_WRAPPER_TOOL="${RELEASE_OPS}"',
    )
    for needle in runner_needles:
        if needle not in runner_script:
            if "QUALIFICATION_MODE" in needle:
                raise ContractError(
                    "Concept Registry runner closed qualification mode is missing"
                )
            if "release.stamp.json" in needle or "closure" in needle:
                raise ContractError(
                    f"Concept Registry runner closure input is missing: {needle}"
                )
            if "verify_base_release_request" in needle:
                raise ContractError(
                    "Concept Registry runner signed release request verification is missing"
                )
            if "fixture fallback" in needle:
                raise ContractError(
                    "Concept Registry runner fixture fallback fence is missing"
                )
            if "PRIVATE_KEY" in needle:
                raise ContractError(
                    "Concept Registry runner external signing key fence is missing"
                )
            if "base_candidate_bound" in needle or "registry_production" in needle:
                raise ContractError(
                    "Concept Registry runner non-production summary fence is missing"
                )
            if needle == "_verify_receipt":
                raise ContractError(
                    "Concept Registry prequalification receipt signature verification is missing"
                )
            if needle == "STAMP_SIGNATURE_DOMAIN":
                raise ContractError(
                    "Concept Registry staged release signature verification is missing"
                )
            if "CANDIDATE_RELEASE_WRAPPER_TOOL" in needle:
                raise ContractError(
                    "Concept Registry release-cycle wrapper is not the fixed candidate binary"
                )
            raise ContractError(f"Concept Registry runner contract missing: {needle}")
    if "ONEBRAIN_REGISTRY_CLOSURE_DIGEST" in runner_script:
        raise ContractError("Concept Registry runner permits a closure override")
    for forbidden in (
        "--candidate-root)",
        "--previous-root)",
        "--release-request-digest)",
        "--qualification-session-id)",
        "--candidate-commit)",
        "--candidate-tree)",
        "ci-small-fixture-v1",
        "target/private-key.hex",
    ):
        if forbidden in runner_script:
            raise ContractError(
                f"Concept Registry runner contains forbidden override/fallback: {forbidden}"
            )

    guide_needles = (
        "Task 21 prequalification is not `BASE-GATE-V1`",
        "registry_production_qualified=true",
        "fixture-only",
        "Never commit measured reports",
        "ONEBRAIN_REGISTRY_PRIVATE_KEY_FILE",
        "ONEBRAIN_QUALIFICATION_GPG_HOME",
        "x86_64-unknown-linux-gnu",
        "onebrain-registry-image-v1",
        "onebrain-registry-cold-cache",
        "onebrain-registry-low-ram",
        "onebrain-registry-ssd",
        "onebrain-registry-hdd",
        "onebrain-registry-controller",
        "2,200,000,000",
        "registry_closure_digest",
        "90 days",
        "raw report",
        '"base_candidate_bound": false',
        '"registry_production_qualified": false',
        "Task 28",
    )
    for needle in guide_needles:
        if needle not in runner_guide:
            raise ContractError(
                f"Concept Registry operations guide missing: {needle}"
            )

    workflow_needles = (
        "  workflow_dispatch:",
        "permissions:\n  contents: read",
        "qualification_mode:",
        "onebrain-registry-image-v1",
        "onebrain-registry-cold-cache",
        "onebrain-registry-low-ram",
        "onebrain-registry-ssd",
        "onebrain-registry-hdd",
        "onebrain-registry-controller",
        "actions/upload-artifact@v4",
        "retention-days: 90",
        "scripts/runner/onebrain-registry-runner.sh",
    )
    for needle in workflow_needles:
        if needle not in production_workflow:
            if "onebrain-registry" in needle:
                raise ContractError(
                    f"Concept Registry immutable runner labels missing: {needle}"
                )
            if "retention-days" in needle:
                raise ContractError("Concept Registry raw report retention is missing")
            raise ContractError(
                f"Concept Registry production workflow missing: {needle}"
            )
    for forbidden_trigger in (
        "  pull_request:",
        "  push:",
        "  schedule:",
        "  workflow_call:",
    ):
        if forbidden_trigger in production_workflow:
            raise ContractError(
                "Concept Registry production workflow must remain manual-only"
            )
    for forbidden_identity in (
        "release_request_digest:",
        "qualification_session_id:",
        "candidate_commit:",
        "candidate_tree:",
        "runner_label:",
    ):
        if forbidden_identity in production_workflow and forbidden_identity != "runner_label:":
            raise ContractError(
                "Concept Registry workflow exposes a release identity override"
            )
    if "${{ inputs.runner_label }}" in production_workflow:
        raise ContractError(
            "Concept Registry workflow immutable runner labels are caller-controlled"
        )

    foundation_needles = (
        "ONEBRAIN_REGISTRY_EVIDENCE_TIER: fixture",
        "Build and verify a small Concept Registry fixture",
        "python -m unittest scripts.ci.test_validate_concept_registry_runner",
        "bash -n scripts/runner/onebrain-registry-runner.sh",
    )
    for needle in foundation_needles:
        if needle not in foundation_workflow:
            raise ContractError(
                f"Concept Registry fixture-only foundation lane missing: {needle}"
            )
    return (
        len(runner_needles),
        len(guide_needles),
        len(workflow_needles),
        len(foundation_needles),
    )


def validate_vnext_macos_soak_runner_kit(
    runner_script: str | None = None,
    runner_guide: str | None = None,
    soak_workflow: str | None = None,
    foundation_workflow: str | None = None,
) -> tuple[int, int, int, int]:
    if runner_script is None:
        runner_script = read(VNEXT_SOAK_RUNNER_SCRIPT)
    if runner_guide is None:
        runner_guide = read(VNEXT_MACOS_SOAK_RUNNER_GUIDE)
    if soak_workflow is None:
        soak_workflow = read(VNEXT_MACOS_SOAK_WORKFLOW)
    if foundation_workflow is None:
        foundation_workflow = read(VNEXT_FOUNDATION_WORKFLOW)

    script_needles = (
        "Darwin/arm64",
        'RUNNER_ASSET_ID="osx-arm64"',
        'DEFAULT_RUNNER_HOME="${HOME}/onebrain-actions-runner"',
        'DEFAULT_RUNNER_LABELS="onebrain-soak-macos-arm64"',
        "shasum -a 256",
        "caffeinate -dimsu",
        "brew install python@3.13 cmake pkgconf",
        "macOS purge target must be below HOME",
        "Cannot write runner parent directory",
        '[[ "$HOST_KIND" == "linux-x64" ]] || return 0',
        '[[ "$HOST_KIND" == "macos-arm64" ]] || return 0',
    )
    for needle in script_needles:
        if needle not in runner_script:
            raise ContractError(f"M5-07 macOS runner safety missing: {needle}")

    guide_needles = (
        "Không cần mở TCP/UDP inbound",
        "macOS ARM64 (Darwin/arm64)",
        "actions-runner-osx-arm64",
        "onebrain-soak-macos-arm64",
        "caffeinate",
        "pre-release-72h",
        "uninstall",
        "~/onebrain-actions-runner",
    )
    for needle in guide_needles:
        if needle not in runner_guide:
            raise ContractError(f"M5-07 macOS runner guide missing: {needle}")

    workflow_needles = (
        "permissions:\n  contents: read",
        "workflow_dispatch:",
        "github.ref == 'refs/heads/main'",
        "runs-on: [self-hosted, macOS, ARM64, onebrain-soak-macos-arm64]",
        "timeout-minutes: 4440",
        "caffeinate -dimsu",
        "actions/upload-artifact@v4",
        "save-if: false",
    )
    for needle in workflow_needles:
        if needle not in soak_workflow:
            raise ContractError(
                f"M5-07 macOS self-hosted workflow safety missing: {needle}"
            )
    for forbidden in ("pull_request:", "schedule:"):
        if forbidden in soak_workflow:
            raise ContractError(
                f"M5-07 macOS workflow must remain manual-only: {forbidden}"
            )

    foundation_needles = (
        "runs-on: macos-15",
        'test "$(uname -m)" = "arm64"',
        "m5_07_macos_proc_metrics_are_available",
        "m5_07_release_smoke_uses_real_quic_fsync_and_all_fault_cycles",
        "python -m unittest scripts.ci.test_validate_vnext_macos_soak_runner_kit",
    )
    for needle in foundation_needles:
        if needle not in foundation_workflow:
            raise ContractError(
                f"M5-07 hosted macOS acceptance lane missing: {needle}"
            )

    return (
        len(script_needles),
        len(guide_needles),
        len(workflow_needles),
        len(foundation_needles),
    )


def validate_vnext_dr_m5_soak_release(
    profile: dict[str, object] | None = None,
) -> tuple[int, int, int, int, int]:
    if profile is None:
        try:
            profile = json.loads(read(DR_M5_SOAK_RELEASE_PROFILE))
        except json.JSONDecodeError as error:
            raise ContractError(f"invalid M5-07 profile JSON: {error}") from error
    if (
        profile.get("format") != "onebrain/dr-m5-soak-release/1"
        or profile.get("profile_id") != "SOAK_PERFORMANCE_RELEASE_GATE_PROFILE_V1"
        or profile.get("version") != 1
    ):
        raise ContractError("M5-07 profile identity drift")

    if profile.get("build") != {
        "cargo_profile": "release",
        "transport": "authenticated-real-quic-loopback",
        "long_runner": ["self-hosted", "linux", "x64", "onebrain-soak"],
        "github_hosted_job_limit_hours": 6,
    }:
        raise ContractError("M5-07 release build/runner contract drift")

    run_profiles = profile.get("run_profiles")
    expected_profiles = {
        "smoke": {
            "minimum_elapsed_seconds": 0,
            "minimum_fault_cycles": 3,
            "release_qualifying": False,
        },
        "nightly-24h": {
            "minimum_elapsed_seconds": 86_400,
            "minimum_fault_cycles": 3,
            "release_qualifying": False,
        },
        "pre-release-72h": {
            "minimum_elapsed_seconds": 259_200,
            "minimum_fault_cycles": 3,
            "release_qualifying": True,
        },
    }
    if run_profiles != expected_profiles:
        raise ContractError("M5-07 duration qualification drift")

    expected_latency = {
        "quic_authenticated_connect": {
            "p50": 500_000,
            "p95": 1_000_000,
            "p99": 2_000_000,
        },
        "fsync_4k": {
            "p50": 100_000,
            "p95": 500_000,
            "p99": 2_000_000,
        },
        "kql_incremental_scan_max": 250_000,
        "pomv_incremental_scan_max": 250_000,
    }
    if profile.get("latency_budgets_micros") != expected_latency:
        raise ContractError("M5-07 latency percentile budget drift")

    expected_growth = {
        "rss_bytes": {
            "hard_cap": 536_870_912,
            "max_growth": 134_217_728,
            "max_positive_slope_per_cycle": 8_388_608,
        },
        "disk_bytes": {
            "hard_cap": 536_870_912,
            "max_growth": 33_554_432,
            "max_positive_slope_per_cycle": 2_097_152,
        },
        "task_count": {
            "hard_cap": 512,
            "max_growth": 16,
            "max_positive_slope_per_cycle": 4,
        },
    }
    if profile.get("growth_budgets") != expected_growth:
        raise ContractError("M5-07 resource growth budget drift")

    expected_lanes = ["distributed-kql-one-hop", "distributed-pomv-view"]
    if profile.get("incremental_scan") != {
        "lanes": expected_lanes,
        "max_records_per_scan": 64,
        "drained_scan_records": 0,
        "durable_selector_type_cursor": True,
    }:
        raise ContractError("M5-07 incremental scan contract drift")

    expected_faults = [
        "slow-peer",
        "bounded-session-flood",
        "partition-reunion",
    ]
    if profile.get("fault_cycle") != expected_faults:
        raise ContractError("M5-07 fault cycle coverage drift")

    expected_signals = [
        "quic-latency-percentiles",
        "fsync-latency-percentiles",
        "rss-growth-and-slope",
        "disk-growth-and-slope",
        "task-growth-and-leak",
        "incremental-scan-budget",
        "runtime-bounded-counters",
        "rollback-reason-codes",
    ]
    if profile.get("operator_signals") != expected_signals:
        raise ContractError("M5-07 operator signal coverage drift")

    expected_exit = [
        "no-unbounded-memory-disk-task-slope",
        "no-task-or-session-leak",
        "m3-fair-redelivery-root-preserved",
        "m4-no-truth-benefit-wallet-obt-amplification",
        "operator-can-detect-and-rollback",
        "short-run-cannot-claim-long-soak",
        "pre-release-requires-72-real-hours",
    ]
    if profile.get("exit_oracles") != expected_exit:
        raise ContractError("M5-07 exit oracle drift")

    source = read(ROOT / "src/onebrain-node/src/vnext_soak_release.rs")
    for needle in (
        'pub const SOAK_RELEASE_PROFILE: &str = "onebrain/dr-m5-soak-release/1"',
        "pub async fn run_soak_release(",
        "measure_quic_connects(",
        "measure_fsync(",
        "measure_incremental_scans(",
        "run_slow_peer_cycle(",
        "run_bounded_flood_cycle(",
        "run_partition_reunion_cycle(",
        "DURATION_OR_CYCLE_EVIDENCE_INCOMPLETE",
        "m5_07_release_smoke_uses_real_quic_fsync_and_all_fault_cycles",
        "pub host_os: String",
        "pub host_arch: String",
        "proc_pidinfo(",
        "m5_07_macos_proc_metrics_are_available",
    ):
        if needle not in source:
            raise ContractError(f"M5-07 implementation evidence missing: {needle}")

    cargo = read(ROOT / "src/onebrain-node/Cargo.toml")
    for needle in (
        'name = "dr_m5_soak_release"',
        'required-features = ["vnext-soak-harness"]',
        "vnext-soak-harness = [",
        '"ku-net/dr-m5-chaos-harness"',
    ):
        if needle not in cargo:
            raise ContractError(f"M5-07 Cargo gate missing: {needle}")

    foundation = read(VNEXT_FOUNDATION_WORKFLOW)
    for needle in (
        "python -m unittest scripts.ci.test_validate_vnext_dr_m5_soak_release",
        "- name: M5.7 optimized real-QUIC soak and performance release smoke",
        "cargo test --locked --release -p onebrain-node",
        "--features vnext-soak-harness",
        "--profile smoke",
    ):
        if needle not in foundation:
            raise ContractError(f"M5-07 PR acceptance gate missing: {needle}")

    soak_workflow = read(VNEXT_SOAK_WORKFLOW)
    for needle in (
        'cron: "41 1 * * *"',
        "runs-on: [self-hosted, linux, x64, onebrain-soak]",
        "timeout-minutes: 4440",
        "nightly-24h",
        "pre-release-72h",
        "actions/upload-artifact@v4",
    ):
        if needle not in soak_workflow:
            raise ContractError(f"M5-07 long-soak workflow missing: {needle}")

    validate_vnext_soak_runner_kit(soak_workflow=soak_workflow)
    validate_vnext_macos_soak_runner_kit(
        soak_workflow=read(VNEXT_MACOS_SOAK_WORKFLOW),
        foundation_workflow=foundation,
    )

    spec = read(VNEXT / "SOAK_PERFORMANCE_RELEASE_GATE_PROFILE_V1.md")
    if "dr-m5-soak-release-v1.json" not in spec:
        raise ContractError("M5-07 normative profile is not linked to machine contract")

    return (
        len(expected_profiles),
        len(expected_latency),
        len(expected_growth),
        len(expected_faults),
        len(expected_exit),
    )


def validate_vnext_p5_multi_host(
    profile: dict[str, object] | None = None,
) -> tuple[int, int, int, int, int]:
    if profile is None:
        try:
            profile = json.loads(read(P5_MULTI_HOST_PRODUCTION_PROFILE))
        except json.JSONDecodeError as error:
            raise ContractError(f"invalid P5 multi-host profile JSON: {error}") from error
    if (
        profile.get("format")
        != "onebrain/p5-multi-host-production-qualification/1"
        or profile.get("profile_id")
        != "P5_MULTI_HOST_PRODUCTION_QUALIFICATION_PROFILE_V1"
        or profile.get("version") != 1
    ):
        raise ContractError("P5 multi-host profile identity drift")

    expected_scope = {
        "physical_host_count": 3,
        "logical_node_count": 3,
        "target_triple": "x86_64-unknown-linux-gnu",
        "transport": "authenticated-real-quic",
        "control_transport": "ssh-stdio-control-only",
        "portability_qualifying": False,
        "single_host_preflight_may_qualify": False,
    }
    if profile.get("scope") != expected_scope:
        raise ContractError("P5 multi-host scope drift")

    expected_reference = {
        "identity_source": "verified-task28-request-v2-plus-compiled-agent-and-measured-registry-candidate",
        "required_pinned_fields": [
            "candidate_commit",
            "candidate_tree",
            "candidate_semantic_digest",
            "linux_artifact_tuple_digest",
            "toolchain_digest",
            "runner_bundle_manifest_digest",
            "agent_binary_digest",
            "agent_signature_digest",
            "registry_root",
            "profile_digest",
        ],
        "cross_host_byte_equality": [
            "target_triple",
            "candidate_commit",
            "candidate_tree",
            "linux_artifact_tuple_digest",
            "toolchain_digest",
            "runner_bundle_manifest_digest",
            "agent_binary_digest",
            "agent_signature_digest",
            "registry_root",
            "profile_digest",
        ],
        "byte_identical_cryptographically_signed_release_agent": True,
        "producer_override": False,
    }
    if profile.get("reference_environment") != expected_reference:
        raise ContractError("P5 reference environment drift")

    expected_hosts = [
        {
            "physical_host_id": "host-a",
            "receipt_role": "p5-host:host-a",
            "durable_root_slot": "p5-host-a-root",
            "principal_slot": "p5-host-a-principal",
        },
        {
            "physical_host_id": "host-b",
            "receipt_role": "p5-host:host-b",
            "durable_root_slot": "p5-host-b-root",
            "principal_slot": "p5-host-b-principal",
        },
        {
            "physical_host_id": "host-c",
            "receipt_role": "p5-host:host-c",
            "durable_root_slot": "p5-host-c-root",
            "principal_slot": "p5-host-c-principal",
        },
    ]
    topology = profile.get("topology")
    if not isinstance(topology, dict):
        raise ContractError("P5 multi-host topology missing")
    hosts = topology.get("hosts")
    if not isinstance(hosts, list) or len(hosts) != 3:
        raise ContractError("P5 multi-host topology requires three hosts")
    roots = [host.get("durable_root_slot") for host in hosts if isinstance(host, dict)]
    principals = [host.get("principal_slot") for host in hosts if isinstance(host, dict)]
    if len(set(roots)) != 3 or len(set(principals)) != 3:
        raise ContractError("P5 multi-host topology root/principal reuse")
    if topology != {
        "hosts": expected_hosts,
        "ring": ["host-a->host-b", "host-b->host-c", "host-c->host-a"],
        "independent_durable_roots": 3,
        "independent_principals": 3,
        "shared_root_or_principal_policy": "reject",
        "accepted_placement_authorities": [
            "provider-signed-placement",
            "bare-metal-lease-inventory",
            "owner-signed-out-of-band-provider-verification",
        ],
        "owner_attestation_binding": "signed-inventory-plus-exact-host-and-placement-evidence-sha256",
    }:
        raise ContractError("P5 multi-host topology drift")

    expected_inventory = {
        "required_host_fields": [
            "physical_host_id",
            "runner_identity",
            "ssh_host_key_algorithm",
            "ssh_host_key_fingerprint",
            "observed_ssh_host_key_fingerprint",
            "receipt_role",
            "receipt_signer_fingerprint",
            "durable_root_locator",
            "expected_principal",
            "ssh_port",
            "physical_machine_fingerprint",
            "host_evidence_sha256",
            "placement_evidence_sha256",
        ],
        "required_orchestrator_fields": [
            "runner_identity",
            "receipt_role",
            "receipt_signer_fingerprint",
        ],
        "ssh_host_key_pin_required": True,
        "duplicate_host_runner_root_principal_or_key_policy": "reject",
    }
    if profile.get("inventory") != expected_inventory:
        raise ContractError("P5 multi-host inventory drift")

    if profile.get("control_plane") != {
        "ssh_use": "control-only",
        "application_bytes_over_ssh": False,
        "bounded_json_stdio": True,
        "signed_agent_receipt_required": True,
        "command_sequence_monotonic": True,
        "replay_or_stale_command_policy": "reject",
    }:
        raise ContractError("P5 multi-host control plane drift")

    if profile.get("fault_proxy") != {
        "default_enabled": False,
        "changes_delivery_conditions_only": True,
        "may_validate_or_create_knowledge": False,
        "may_claim_authority_truth_completion_reward_or_wallet": False,
    }:
        raise ContractError("P5 multi-host fault proxy authority drift")

    expected_faults = [
        "partition",
        "drop",
        "reorder",
        "duplicate",
        "restart",
        "address-change",
        "seed-outage",
        "signer-outage",
        "disk-pressure",
        "slow-peer",
        "base-obarv002-archive-restore",
        "rollback",
        "explicit-re-enable",
    ]
    if profile.get("fault_matrix") != expected_faults:
        raise ContractError("P5 multi-host fault matrix drift")

    if profile.get("archive_restore") != {
        "production_profile": "OBARV002",
        "preflight_profile": "onebrain/p5-offline-backup/1",
        "preflight_profile_unchanged": True,
        "preflight_profile_may_qualify": False,
        "restore_target": "new-dataset-generation",
        "activation": "verify-parity-health-then-atomic-switch",
    }:
        raise ContractError("P5 multi-host archive boundary drift")

    expected_roles = [
        "p5-host:host-a",
        "p5-host:host-b",
        "p5-host:host-c",
        "p5-orchestrator",
    ]
    expected_role_bindings = [
        {
            "role": "p5-host:host-a",
            "public_key_hex": "aca5c9fcdd081df1611245fce93bf906bf80de3c8e032f342d435a8070808fdd",
            "fingerprint_hex": "b3e1630cc673e711b90a494fe26d6ad413382f299f83913a006e175916002474",
        },
        {
            "role": "p5-host:host-b",
            "public_key_hex": "deadb04f785432147f18e6dcd53b802a3fcca4071bd77eb82f29a96a9b5edbbb",
            "fingerprint_hex": "72167d8e93c6b28dd2ba6684d818b457d8547bd8e44235795b8427d9dd27fff7",
        },
        {
            "role": "p5-host:host-c",
            "public_key_hex": "fb075ebeedd80680987165d2e7c32d3595dc421fcd057cdbc60a15f9dbeab67d",
            "fingerprint_hex": "c63b2b4d4ab09b5a49e42b3c547c04d6e7aa81cc72423ed8f7ef70c254afedfa",
        },
        {
            "role": "p5-orchestrator",
            "public_key_hex": "cce7da80b255ed3a67a8414f79e700bb0fdc4944abe3793d9c23e8ca1699fc27",
            "fingerprint_hex": "6d018ba3d7224bc5a415a54c226f81db1139d950aedf0ef5dfb9b9da441b01ca",
        },
    ]
    trust = profile.get("trust_policy")
    if not isinstance(trust, dict):
        raise ContractError("P5 multi-host trust policy missing")
    policy = trust.get("policy")
    if not isinstance(policy, dict):
        raise ContractError("P5 multi-host trust policy bytes missing")
    role_bindings = policy.get("role_bindings")
    if not isinstance(role_bindings, list) or [
        row.get("role") for row in role_bindings if isinstance(row, dict)
    ] != expected_roles:
        raise ContractError("P5 multi-host signer role drift")
    public_keys = [
        row.get("public_key_hex") for row in role_bindings if isinstance(row, dict)
    ]
    fingerprints = [
        row.get("fingerprint_hex") for row in role_bindings if isinstance(row, dict)
    ]
    if len(set(public_keys)) != 4 or len(set(fingerprints)) != 4:
        raise ContractError("P5 multi-host cross-host key reuse")
    if trust != {
        "canonicalization": "utf8-json-sorted-keys-no-whitespace",
        "digest_algorithm": "BLAKE3-derive-key-v1",
        "digest_context": "onebrain:p5:trust-policy:1",
        "digest_hex": "9aa666fedfc1ee3ee76cf814b9027d06dc1e243a0937001f8df10e996bf7572d",
        "fingerprint_algorithm": "BLAKE3-derive-key-v1",
        "fingerprint_context": "onebrain:p5:evidence-signer-fingerprint:1",
        "policy": {
            "algorithm": "Ed25519",
            "allowed_usages": [
                "p5-host-receipt",
                "p5-orchestrator-aggregate",
                "p5-release-agent-signature",
            ],
            "format": "onebrain/p5-multi-host-trust-policy/1",
            "role_bindings": expected_role_bindings,
        },
        "valid_unlisted_signature": "reject",
        "wrong_role_signature": "reject",
        "cross_host_key_reuse": "reject",
    }:
        raise ContractError("P5 multi-host trust policy drift")

    expected_receipt_bindings = [
        "role",
        "physical_host_id",
        "release_request_digest",
        "qualification_session_id",
        "candidate_commit",
        "candidate_tree",
        "candidate_semantic_digest",
        "linux_artifact_tuple_digest",
        "toolchain_digest",
        "runner_bundle_manifest_digest",
        "agent_binary_digest",
        "agent_signature_digest",
        "registry_root",
        "profile_digest",
        "trust_policy_digest",
        "runner_identity",
        "ssh_host_key_fingerprint",
        "physical_machine_fingerprint",
        "host_evidence_sha256",
        "placement_evidence_sha256",
        "command_sequence",
        "command",
        "fault_id",
        "before_roots",
        "after_roots",
        "resource_observation",
        "result",
        "limitations",
    ]
    expected_root_fields = [
        "canonical_root",
        "journal_root",
        "outbox_root",
        "operational_root",
    ]
    if profile.get("child_receipt") != {
        "format": "onebrain/p5-multi-host-child-receipt/1",
        "signature_domain": "onebrain:p5:multi-host-child-receipt:1\\0",
        "canonicalization": "utf8-json-sorted-keys-no-whitespace",
        "unknown_field_policy": "reject",
        "binding_match": "exact-verified-signed-release-request",
        "missing_or_wrong_binding_policy": "reject",
        "required_bindings": expected_receipt_bindings,
        "required_root_fields": expected_root_fields,
    }:
        raise ContractError("P5 multi-host child receipt drift")

    identical_bindings = [
        "release_request_digest",
        "qualification_session_id",
        "candidate_commit",
        "candidate_tree",
        "candidate_semantic_digest",
        "linux_artifact_tuple_digest",
        "toolchain_digest",
        "runner_bundle_manifest_digest",
        "agent_binary_digest",
        "agent_signature_digest",
        "registry_root",
        "profile_digest",
        "trust_policy_digest",
    ]
    if profile.get("aggregate") != {
        "format": "onebrain/p5-multi-host-production-aggregate/1",
        "signer_role": "p5-orchestrator",
        "signature_domain": "onebrain:p5:multi-host-production-aggregate:1\\0",
        "root_algorithm": "BLAKE3",
        "root_domain": "onebrain:p5:multi-host-child-receipt-root:1\\0",
        "root_order": "physical-host-id-then-fault-order-then-command-sequence",
        "root_inputs": ["canonical-ordered-child-receipt-bytes"],
        "root_excludes": ["aggregate_report", "aggregate_signature"],
        "identical_child_bindings": identical_bindings,
        "mixed_binding_policy": "reject",
        "minimum_distinct_physical_hosts": 3,
        "derive_multi_host_qualified_from_verified_evidence": True,
        "input_boolean_trusted": False,
    }:
        raise ContractError("P5 multi-host aggregate drift")

    expected_bounds = {
        "max_peak_rss_bytes_per_host": 1073741824,
        "max_durable_growth_bytes_per_host": 4294967296,
        "max_task_count_per_host": 256,
        "max_active_sessions_per_host": 16,
        "max_control_message_bytes": 1048576,
        "max_fault_duration_ms": 300000,
        "max_reunion_ms": 60000,
        "max_quiescence_ms": 30000,
    }
    if profile.get("resource_bounds") != expected_bounds:
        raise ContractError("P5 multi-host resource bound drift")

    expected_exit = [
        "durable-reunion-idempotency",
        "principal-preserved-per-host",
        "canonical-root-preserved-or-exactly-advanced",
        "journal-root-reconciled",
        "outbox-root-reconciled",
        "operational-root-reconciled",
        "zero-active-session-after-quiescence",
        "memory-disk-and-task-bounds-hold",
        "local-kql-works-with-all-network-lanes-off",
        "zero-truth-authority-completion-reward-wallet-amplification",
    ]
    if profile.get("exit_oracles") != expected_exit:
        raise ContractError("P5 multi-host exit oracle drift")

    if profile.get("preflight_boundary") != {
        "single_host_profiles": [
            "onebrain/p5-canary-preflight/1",
            "onebrain/p5-operations-preflight/1",
        ],
        "single_host_multi_host_qualified": False,
        "three_process_single_host_multi_host_qualified": False,
        "preflight_receipt_evidence_tier": "prequalification",
        "production_receipt_evidence_tier": "production-reference",
    }:
        raise ContractError("P5 multi-host preflight boundary drift")

    if profile.get("qualification_state") != {
        "contract_frozen": True,
        "measured_evidence_committed": False,
        "multi_host_qualified": False,
        "portability_qualified": False,
        "registry_production_qualified": False,
        "base_gate_v1_qualified": False,
    }:
        raise ContractError("P5 multi-host qualification state drift")

    if profile.get("required_limitations") != [
        "aggregate-qualification-is-orchestrator-owned",
        "base-gate-v1-not-claimed",
        "receipt-is-evidence-not-authority",
        "real-quic-ring-and-fault-injection-pending",
        "registry-candidate-bytes-bound-without-full-profile-qualification",
        "registry-production-qualification-not-claimed",
        "registry-production-resource-profiles-pending",
    ]:
        raise ContractError("P5 multi-host limitations drift")
    if profile.get("registry_candidate_binding") != {
        "format": "onebrain/p5-registry-candidate-binding/1",
        "files": [
            "concepts.obr",
            "concepts.obr.ccids.idx",
            "concepts.obr.labels.idx",
            "concepts.obr.manifest.json",
            "concepts.obr.verification.json",
        ],
        "root_domain": "onebrain:p5:registry-candidate-binding:1\\0",
        "full_registry_profile_required_for_p5_subgate": False,
        "registry_production_qualified": False,
        "base_gate_v1_qualified": False,
    }:
        raise ContractError("P5 Registry candidate binding drift")

    spec = read(VNEXT / "P5_MULTI_HOST_PRODUCTION_QUALIFICATION_PROFILE_V1.md")
    for needle in (
        "p5-multi-host-production-qualification-v1.json",
        "x86_64-unknown-linux-gnu",
        "OBARV002",
        "onebrain/p5-offline-backup/1",
        "multi_host_qualified=false",
    ):
        if needle not in spec:
            raise ContractError(f"P5 multi-host normative profile missing: {needle}")
    for preflight_name in (
        "P5_CANARY_PREFLIGHT_PROFILE_V1.md",
        "P5_OPERATIONS_PREFLIGHT_PROFILE_V1.md",
    ):
        preflight = read(VNEXT / preflight_name)
        if "P5_MULTI_HOST_PRODUCTION_QUALIFICATION_PROFILE_V1.md" not in preflight:
            raise ContractError(f"P5 preflight production boundary missing: {preflight_name}")

    return (
        expected_scope["physical_host_count"],
        len(expected_hosts),
        len(expected_faults),
        len(expected_exit),
        len(expected_role_bindings),
    )


def validate_base_v1_exact_candidate_soak(
    profile: dict[str, object] | None = None,
    workflow: str | None = None,
) -> tuple[int, int, int, int]:
    if profile is None:
        try:
            profile = json.loads(read(BASE_V1_EXACT_CANDIDATE_SOAK_PROFILE))
        except json.JSONDecodeError as error:
            raise ContractError(f"invalid Base v1 exact-candidate soak JSON: {error}") from error
    if (
        profile.get("format") != "onebrain/base-v1-exact-candidate-soak/1"
        or profile.get("profile_id") != "BASE_V1_EXACT_CANDIDATE_SOAK_PROFILE_V1"
        or profile.get("version") != 1
    ):
        raise ContractError("Base v1 exact-candidate soak identity drift")

    expected_scope = {
        "minimum_uninterrupted_elapsed_seconds": 259200,
        "minimum_distinct_physical_runners": 3,
        "target_triple": "x86_64-unknown-linux-gnu",
        "cargo_profile": "release",
        "transport": "authenticated-real-quic",
        "candidate_source": "verified-signed-task-28-release-request",
        "only_eligible_candidate": "exact-task-27-commit-and-tree",
        "task_25_is_ancestor_checkpoint_only": True,
        "fresh_run_required": True,
        "prior_m5_07_may_qualify": False,
        "synthetic_unchanged_closure_may_qualify": False,
    }
    if profile.get("scope") != expected_scope:
        raise ContractError("Base v1 exact-candidate soak scope drift")

    required_bindings = [
        "release_request_digest",
        "qualification_session_id",
        "candidate_commit",
        "candidate_tree",
        "candidate_semantic_digest",
        "frozen_target_artifact_digest",
        "registry_root",
        "p5_aggregate_root",
        "executable_blake3",
        "sbom_blake3",
        "provenance_blake3",
        "runner_image_digest",
        "trust_policy_digest",
    ]
    reference_pins = required_bindings[:-1]
    reference = profile.get("reference_environment")
    if reference != {
        "identity_source": "verified-signed-release-request",
        "producer_override": False,
        "required_pinned_fields": reference_pins,
        "cross_runner_byte_equality": reference_pins[2:],
        "identical_release_executable_hash_required": True,
    }:
        raise ContractError("Base v1 exact-candidate reference identity drift")

    expected_runners = [
        {
            "runner_id": "runner-a",
            "role": "soak-runner:runner-a",
            "required_labels": [
                "self-hosted",
                "linux",
                "x64",
                "onebrain-soak",
                "onebrain-soak-a",
            ],
        },
        {
            "runner_id": "runner-b",
            "role": "soak-runner:runner-b",
            "required_labels": [
                "self-hosted",
                "linux",
                "x64",
                "onebrain-soak",
                "onebrain-soak-b",
            ],
        },
        {
            "runner_id": "runner-c",
            "role": "soak-runner:runner-c",
            "required_labels": [
                "self-hosted",
                "linux",
                "x64",
                "onebrain-soak",
                "onebrain-soak-c",
            ],
        },
    ]
    if profile.get("runners") != expected_runners:
        raise ContractError("Base v1 exact-candidate soak runner topology drift")

    expected_roles = [
        {
            "role": "soak-runner:runner-a",
            "public_key_hex": "f6dcfda9ff046728bd9ffec69f38db909f6198e46b3eb6c208411c3fef95fd27",
            "fingerprint_hex": "af9ec4df16d41ab7700c3428f12430c95c8ada4d2c0ca5ac8353af42fcb755ad",
        },
        {
            "role": "soak-runner:runner-b",
            "public_key_hex": "9b415457ea3f9a794670c55387d4742bd6105dea6f95780c6cf6c3d9ae7c4907",
            "fingerprint_hex": "160ce310b3c99f1f30d21f3ad2206b638cb391bfe057058155a2355998a1e08f",
        },
        {
            "role": "soak-runner:runner-c",
            "public_key_hex": "d4295546c6818dacf38758d75f70867ea06a50f12c8800bcae532f76e737ac9e",
            "fingerprint_hex": "2069e547629b1551c505becebf4da5cabe6c61b92b9465aba328fb8258065ae1",
        },
        {
            "role": "soak-aggregator",
            "public_key_hex": "888cf37977b179b78aff9045a0ce599cd090172d38ec04e4d462cf70eee454b3",
            "fingerprint_hex": "8ab8e70864bd2258042dc4e5d18d271680df2566092317363b1064b6f1fa2ae9",
        },
    ]
    trust = profile.get("trust_policy")
    if not isinstance(trust, dict):
        raise ContractError("Base v1 exact-candidate soak trust policy missing")
    expected_trust_without_approval = {
        "canonicalization": "utf8-json-sorted-keys-no-whitespace",
        "digest_algorithm": "BLAKE3-derive-key-v1",
        "digest_context": "onebrain:base-v1:soak-trust-policy:1",
        "digest_hex": "f2ef9e95575c47a25a1809ba580b70eac1413bc5d147f9f021987c393ba778d6",
        "fingerprint_algorithm": "BLAKE3-derive-key-v1",
        "fingerprint_context": "onebrain:base-v1:soak-evidence-signer-fingerprint:1",
        "policy": {
            "algorithm": "Ed25519",
            "allowed_usages": [
                "base-v1-soak-child-receipt",
                "base-v1-soak-aggregate",
            ],
            "format": "onebrain/base-v1-exact-candidate-soak-trust-policy/1",
            "role_bindings": expected_roles,
        },
        "valid_unlisted_signature": "reject",
        "wrong_or_cross_runner_role": "reject",
        "changed_trust_policy": "reject",
    }
    actual_without_approval = dict(trust)
    approval = actual_without_approval.pop("owner_approval", None)
    if actual_without_approval != expected_trust_without_approval:
        raise ContractError("Base v1 exact-candidate soak trust-policy drift")
    if approval not in (
        {"status": "pending-owner-approval", "approved_utc": None},
        {"status": "owner-approved", "approved_utc": "2026-08-11"},
    ):
        raise ContractError("Base v1 exact-candidate soak owner approval state drift")
    public_keys = {row["public_key_hex"] for row in expected_roles}
    fingerprints = {row["fingerprint_hex"] for row in expected_roles}
    if len(public_keys) != 4 or len(fingerprints) != 4:
        raise ContractError("Base v1 exact-candidate soak reuses a signer across roles")

    payload_fields = [
        "role",
        "runner_id",
        "runner_identity",
        "interval_sequence",
        "receipt_kind",
        "monotonic_start_ns",
        "monotonic_end_ns",
        "command",
        "result",
        "limitations",
    ]
    if profile.get("child_receipt") != {
        "format": "onebrain/base-v1-exact-candidate-soak-child-receipt/1",
        "signature_domain": "onebrain:base-v1:exact-candidate-soak-child-receipt:1\\0",
        "canonicalization": "utf8-json-sorted-keys-no-whitespace",
        "unknown_field_policy": "reject",
        "receipt_kinds": ["interval", "fault"],
        "required_bindings": required_bindings,
        "required_payload_fields": payload_fields,
    }:
        raise ContractError("Base v1 exact-candidate soak child-receipt drift")
    if profile.get("aggregate") != {
        "format": "onebrain/base-v1-exact-candidate-soak-aggregate/1",
        "signer_role": "soak-aggregator",
        "signature_domain": "onebrain:base-v1:exact-candidate-soak-aggregate:1\\0",
        "root_algorithm": "BLAKE3",
        "root_domain": "onebrain:base-v1:exact-candidate-soak-child-root:1\\0",
        "root_order": "runner-id-then-monotonic-start-then-sequence-then-receipt-kind",
        "root_inputs": ["canonical-ordered-interval-and-fault-child-receipt-bytes"],
        "root_excludes": ["aggregate_report", "aggregate_signature"],
        "mixed_binding_policy": "reject",
        "input_qualification_boolean_trusted": False,
    }:
        raise ContractError("Base v1 exact-candidate soak aggregate-root drift")
    if profile.get("fault_cycle") != [
        "slow-peer",
        "bounded-session-flood",
        "partition-reunion",
    ]:
        raise ContractError("Base v1 exact-candidate soak fault-cycle drift")
    exit_oracles = profile.get("exit_oracles")
    if not isinstance(exit_oracles, list) or len(exit_oracles) != 13:
        raise ContractError("Base v1 exact-candidate soak exit-oracle drift")
    if profile.get("carry_forward") != {
        "analyzer_purpose": "staleness-demonstration-only",
        "analytically_reusable_when_closure_unchanged": True,
        "base_v1_reusable": False,
        "fresh_task_28_soak_required": True,
    }:
        raise ContractError("Base v1 exact-candidate carry-forward boundary drift")
    expected_state = {
        "contract_frozen": approval["status"] == "owner-approved",
        "measured_evidence_committed": False,
        "soak_qualified": False,
        "production_qualified": False,
    }
    if profile.get("qualification_state") != expected_state:
        raise ContractError("Base v1 exact-candidate qualification state drift")

    if workflow is None:
        workflow = read(BASE_V1_P5_PRODUCTION_WORKFLOW)
    markers = (
        "workflow_dispatch:",
        "if: github.ref == 'refs/heads/main'",
        "verify_base_release_request.py",
        "ref: ${{ needs.verify-exact-release-request.outputs.candidate_commit }}",
        "git rev-parse HEAD^{tree}",
        "compare-release-executable-hashes",
        "retain-signed-raw-receipts",
        "p5-multi-host-aggregate",
        "base-v1-exact-candidate-soak-aggregate",
        "timeout-minutes: 4440",
        "pre-release-72h",
    )
    for marker in markers:
        if marker not in workflow:
            raise ContractError(f"Base v1 production canary workflow missing: {marker}")
    for forbidden in ("pull_request:", "schedule:", "candidate_commit:\n        description:"):
        if forbidden in workflow:
            raise ContractError(f"Base v1 production canary workflow exposes: {forbidden}")

    analyzer = read(ROOT / "scripts/release/validate_evidence_carry_forward.py")
    for marker in (
        "def _verify_p5_aggregate(",
        'parser.add_argument("--p5-aggregate", type=Path, required=True)',
        'parser.add_argument("--executable", type=Path, required=True)',
        '"SPDX_SBOM:sbom.spdx.json"',
        'fresh exact-candidate soak evidence is incomplete',
    ):
        if marker not in analyzer:
            raise ContractError(f"Base v1 evidence analyzer missing: {marker}")
    for forbidden in (
        'parser.add_argument("--p5-aggregate-root"',
        'parser.add_argument("--executable-blake3"',
        'parser.add_argument("--sbom-blake3"',
        'parser.add_argument("--provenance-blake3"',
    ):
        if forbidden in analyzer:
            raise ContractError(f"Base v1 evidence analyzer accepts override: {forbidden}")

    spec = read(VNEXT / "BASE_V1_EXACT_CANDIDATE_SOAK_PROFILE.md")
    for marker in (
        "base-v1-exact-candidate-soak-v1.json",
        "fresh, uninterrupted 259,200-second soak",
        "Task 27",
        "Task 28",
        "fresh_soak_required=true",
        "production_qualified=false",
    ):
        if marker not in spec:
            raise ContractError(f"Base v1 exact-candidate soak spec missing: {marker}")
    return (len(expected_runners), len(expected_roles), 2, len(exit_oracles))


def validate_vnext_p5_canary_preflight(
    profile: dict[str, object] | None = None,
) -> tuple[int, int, int, int, int]:
    if profile is None:
        try:
            profile = json.loads(read(P5_CANARY_PREFLIGHT_PROFILE))
        except json.JSONDecodeError as error:
            raise ContractError(f"invalid P5-01 canary profile JSON: {error}") from error
    if (
        profile.get("format") != "onebrain/p5-canary-preflight/1"
        or profile.get("profile_id") != "P5_CANARY_PREFLIGHT_PROFILE_V1"
        or profile.get("version") != 1
    ):
        raise ContractError("P5-01 canary profile identity drift")

    scope = profile.get("scope")
    if scope != {
        "host_count": 1,
        "logical_node_count": 3,
        "transport": "authenticated-real-quic-loopback",
        "production_canary_qualifying": False,
    }:
        raise ContractError("P5-01 canary scope drift")

    topology = profile.get("topology")
    expected_topology = {
        "independent_durable_directories": 3,
        "independent_principals": 3,
        "ring_deliveries": 3,
        "minimum_authenticated_route_observations": 6,
    }
    if topology != expected_topology:
        raise ContractError("P5-01 canary topology drift")

    expected_faults = [
        "old-route-partition",
        "durable-node-restart",
        "authenticated-route-address-change",
        "reunion-idempotent-replay",
    ]
    if profile.get("fault_drills") != expected_faults:
        raise ContractError("P5-01 canary fault drill drift")

    expected_exit = [
        "three-distinct-principals",
        "durable-principal-survives-restart",
        "route-generation-advances",
        "replayed-feed-has-one-durable-branch",
        "zero-active-session-after-quiescence",
        "no-wallet-or-obt-side-effect",
        "no-authority-or-network-completion-claim",
        "nonempty-operator-directory-fails-closed",
    ]
    if profile.get("exit_oracles") != expected_exit:
        raise ContractError("P5-01 canary exit oracle drift")

    if profile.get("production_gate") != {
        "requires_pre_release_72h": True,
        "requires_multi_host_canary": True,
        "preflight_can_open_release": False,
    }:
        raise ContractError("P5-01 canary production gate drift")

    source = read(ROOT / "src/onebrain-node/src/vnext_canary_operations.rs")
    for needle in (
        'pub const P5_CANARY_PREFLIGHT_PROFILE: &str = "onebrain/p5-canary-preflight/1"',
        "pub async fn run_p5_canary_preflight(",
        "P5_CANARY_NODE_COUNT: usize = 3",
        "P5_CANARY_RING_DELIVERIES: usize = 3",
        "P5_CANARY_ROUTE_OBSERVATIONS: usize = 6",
        "partition_rejected_old_route",
        "restarted_principal_stable",
        "route_generation_advanced",
        "durable_feed_branches_after_replay",
        "production_canary_qualified: false",
        "p5_01_three_node_real_quic_partition_restart_route_change_reunion",
        "p5_01_nonempty_node_directory_fails_before_runtime_start",
    ):
        if needle not in source:
            raise ContractError(f"P5-01 implementation evidence missing: {needle}")

    cargo = read(ROOT / "src/onebrain-node/Cargo.toml")
    for needle in (
        'name = "p5_canary_preflight"',
        'required-features = ["vnext-canary-harness"]',
        'vnext-canary-harness = ["vnext-network-runtime"]',
    ):
        if needle not in cargo:
            raise ContractError(f"P5-01 Cargo gate missing: {needle}")

    workflow = read(VNEXT_FOUNDATION_WORKFLOW)
    for needle in (
        "python -m unittest scripts.ci.test_validate_vnext_p5_canary_preflight",
        "- name: P5.1 three-node authenticated QUIC canary preflight",
        "--features vnext-canary-harness",
        "--example p5_canary_preflight",
    ):
        if needle not in workflow:
            raise ContractError(f"P5-01 PR acceptance gate missing: {needle}")

    spec = read(VNEXT / "P5_CANARY_PREFLIGHT_PROFILE_V1.md")
    if "p5-canary-preflight-v1.json" not in spec:
        raise ContractError("P5-01 normative profile is not linked to machine contract")

    return (
        expected_topology["independent_principals"],
        expected_topology["ring_deliveries"],
        expected_topology["minimum_authenticated_route_observations"],
        len(expected_faults),
        len(expected_exit),
    )


def validate_vnext_p5_operations_preflight(
    profile: dict[str, object] | None = None,
) -> tuple[int, int, int, int, int, int, int]:
    if profile is None:
        try:
            profile = json.loads(read(P5_OPERATIONS_PREFLIGHT_PROFILE))
        except json.JSONDecodeError as error:
            raise ContractError(
                f"invalid P5-02..P5-06 operations profile JSON: {error}"
            ) from error
    if (
        profile.get("format") != "onebrain/p5-operations-preflight/1"
        or profile.get("profile_id") != "P5_OPERATIONS_PREFLIGHT_PROFILE_V1"
        or profile.get("version") != 1
    ):
        raise ContractError("P5-02..P5-06 operations profile identity drift")

    if profile.get("scope") != {
        "host_count": 1,
        "uses_authenticated_real_quic": True,
        "consumes_pre_release_72h_evidence": False,
        "multi_host_canary_qualifying": False,
        "production_canary_qualifying": False,
    }:
        raise ContractError("P5-02..P5-06 operations scope drift")

    expected_faults = [
        "session-signer-unavailable-before-durable-side-effect",
        "storage-hard-watermark-rejects-payload",
        "slow-authenticated-peer-does-not-block-healthy-peer",
    ]
    expected_fault_exit = [
        "zero-signer-outage-durable-files",
        "rejected-storage-reason-visible",
        "zero-durable-feed-branch-under-disk-pressure",
        "healthy-peer-progress-within-5000ms",
        "zero-active-session-after-quiescence",
        "no-wallet-obt-authority-or-completion-amplification",
    ]
    fault_drills = profile.get("p5_02_fault_drills")
    if not isinstance(fault_drills, dict):
        raise ContractError("P5-02 fault drill contract missing")
    if fault_drills.get("faults") != expected_faults:
        raise ContractError("P5-02 fault drill drift")
    if fault_drills.get("exit_oracles") != expected_fault_exit:
        raise ContractError("P5-02 exit oracle drift")

    expected_durable_files = [
        "vnext_identity.key",
        "vnext_verified.redb",
        "vnext_reconciliation.redb",
        "vnext_inventory.redb",
        "vnext_record_provenance.redb",
        "vnext_outbox.redb",
        "vnext_operational_compaction.redb",
    ]
    expected_integrity = [
        "sorted-relative-path-manifest",
        "per-file-length-and-blake3",
        "domain-separated-aggregate-root",
        "fsync-each-copied-file",
        "reject-symlink-and-non-regular-entry",
        "corruption-fails-before-restore-target-creation",
    ]
    expected_restore_exit = [
        "principal-preserved",
        "one-raw-feed-branch-preserved",
        "journal-bytes-preserved",
        "pending-outbox-preserved",
        "quarantine-and-provenance-preserved",
        "operational-root-preserved",
    ]
    backup = profile.get("p5_03_backup_restore")
    if not isinstance(backup, dict):
        raise ContractError("P5-03 backup/restore contract missing")
    if backup.get("archive_profile") != "onebrain/p5-offline-backup/1":
        raise ContractError("P5-03 archive profile drift")
    if backup.get("required_durable_files") != expected_durable_files:
        raise ContractError("P5-03 durable file set drift")
    if backup.get("integrity") != expected_integrity:
        raise ContractError("P5-03 integrity gate drift")
    if backup.get("restore_oracles") != expected_restore_exit:
        raise ContractError("P5-03 restore oracle drift")

    expected_lanes = [
        "network",
        "distributed_kql_one_hop",
        "public_use_evidence_publish",
        "distributed_pomv_view",
    ]
    expected_rollback_sequence = [
        "run",
        "atomic-rollback",
        "network-fenced",
        "shutdown",
        "durable-state-inspection",
        "restart-with-stale-enabled-config",
        "explicit-reenable-each-lane",
        "authenticated-real-quic-reconnect",
    ]
    expected_preserved = [
        "principal",
        "raw-feed",
        "journal",
        "pending-outbox",
        "quarantine",
        "operational-root",
    ]
    rollback = profile.get("p5_04_rollback_reenable")
    if not isinstance(rollback, dict):
        raise ContractError("P5-04 rollback/re-enable contract missing")
    if rollback.get("lanes") != expected_lanes:
        raise ContractError("P5-04 lane set drift")
    if rollback.get("sequence") != expected_rollback_sequence:
        raise ContractError("P5-04 rollback sequence drift")
    if rollback.get("preserved") != expected_preserved:
        raise ContractError("P5-04 preservation oracle drift")

    expected_rollout = {
        "default_feature_flag_count": 12,
        "runtime_lane_count": 4,
        "stale_config_may_enable": False,
        "explicit_generation_advancing_reenable_required": True,
        "default_config_effective_lanes_after_reopen": 0,
        "local_kql_round_trip_with_network_off": True,
    }
    if profile.get("p5_05_default_off_rollout") != expected_rollout:
        raise ContractError("P5-05 default-off rollout drift")

    expected_signals = [
        "startup-phase-count",
        "signer-health",
        "registry-health",
        "lane-generation-and-state",
        "authenticated-route-and-session-count",
        "journal-count",
        "outbox-count-and-age",
        "quarantine-and-provenance-count",
        "storage-bytes-and-pressure",
        "finite-incident-and-action-codes",
    ]
    expected_incidents = [
        "SIGNER_UNAVAILABLE",
        "REGISTRY_CORRUPT",
        "STORAGE_SOFT_WATERMARK",
        "STORAGE_REJECTED",
        "PENDING_OUTBOX",
        "RETRY_EXHAUSTED_OUTBOX",
        "ACTIVE_JOURNAL",
        "QUARANTINE_PRESENT",
        "LANE_FENCED",
        "ROLLBACK_ACTIVE",
    ]
    dashboard = profile.get("p5_06_operator_dashboard")
    if not isinstance(dashboard, dict):
        raise ContractError("P5-06 operator dashboard contract missing")
    if dashboard.get("profile") != "onebrain/p5-operator-dashboard/1":
        raise ContractError("P5-06 dashboard profile drift")
    if dashboard.get("signals") != expected_signals:
        raise ContractError("P5-06 signal set drift")
    if dashboard.get("incident_codes") != expected_incidents:
        raise ContractError("P5-06 incident code drift")
    if dashboard.get("privacy") != {
        "contains_node_id": False,
        "contains_selector": False,
        "contains_private_need": False,
        "contains_free_form_label": False,
    }:
        raise ContractError("P5-06 privacy boundary drift")

    expected_external_gates = [
        "pinned-pre-release-72h-artifact",
        "multi-host-production-canary",
        "operator-approved-production-rollout",
    ]
    if profile.get("remaining_external_gates") != expected_external_gates:
        raise ContractError("P5-02..P5-06 external gate drift")

    source = read(ROOT / "src/onebrain-node/src/vnext_p5_operations.rs")
    for needle in (
        'pub const P5_OPERATIONS_PREFLIGHT_PROFILE: &str = "onebrain/p5-operations-preflight/1"',
        "pub async fn run_p5_operations_preflight(",
        "start_canary_harness(",
        "VNextReasonCode::RejectedStorage",
        "create_offline_backup(",
        "restore_offline_backup(",
        "corrupt_archive_failed_before_restore",
        "rollback_runtime()",
        "local_kql_round_trip_with_network_off",
        "pub fn build_operator_dashboard(",
        "production_canary_qualified: false",
        "p5_02_through_p5_06_operational_preflight_passes_without_72h",
    ):
        if needle not in source:
            raise ContractError(
                f"P5-02..P5-06 implementation evidence missing: {needle}"
            )

    network_source = read(ROOT / "src/onebrain-node/src/vnext_network_runtime.rs")
    if "pub(crate) async fn start_canary_harness(" not in network_source:
        raise ContractError("P5 operations gated network harness missing")

    cargo = read(ROOT / "src/onebrain-node/Cargo.toml")
    for needle in (
        'name = "p5_operations_preflight"',
        'required-features = ["vnext-canary-harness"]',
        'vnext-canary-harness = ["vnext-network-runtime"]',
    ):
        if needle not in cargo:
            raise ContractError(f"P5 operations Cargo gate missing: {needle}")

    workflow = read(VNEXT_FOUNDATION_WORKFLOW)
    for needle in (
        "python -m unittest scripts.ci.test_validate_vnext_p5_operations_preflight",
        "- name: P5.2-P5.6 operational fault backup rollback and dashboard preflight",
        "--example p5_operations_preflight",
    ):
        if needle not in workflow:
            raise ContractError(f"P5 operations PR acceptance gate missing: {needle}")

    spec = read(VNEXT / "P5_OPERATIONS_PREFLIGHT_PROFILE_V1.md")
    if "p5-operations-preflight-v1.json" not in spec:
        raise ContractError("P5 operations profile is not linked to machine contract")

    return (
        len(expected_faults),
        len(expected_durable_files),
        len(expected_lanes),
        expected_rollout["default_feature_flag_count"],
        len(expected_signals),
        len(expected_incidents),
        len(expected_external_gates),
    )


def validate_concept_registry_production_qualification(
    profile: dict[str, object] | None = None,
) -> tuple[int, int, int, int]:
    if profile is None:
        try:
            profile = json.loads(
                read(CONCEPT_REGISTRY_PRODUCTION_QUALIFICATION_PROFILE)
            )
        except (OSError, json.JSONDecodeError) as error:
            raise ContractError(
                "invalid Concept Registry production qualification profile JSON: "
                f"{error}"
            ) from error

    if (
        profile.get("format")
        != "onebrain/concept-registry-production-qualification/1"
        or profile.get("profile_id")
        != "CONCEPT_REGISTRY_PRODUCTION_QUALIFICATION_PROFILE_V1"
        or profile.get("version") != 1
    ):
        raise ContractError(
            "Concept Registry production qualification profile identity drift"
        )
    if profile.get("profile_digest") != {
        "algorithm": "BLAKE3",
        "canonicalization": "utf8-json-sorted-keys-no-whitespace",
        "input": "complete-profile-object-without-an-embedded-digest-value",
        "receipt_field": "production_profile_blake3",
    }:
        raise ContractError("Concept Registry production profile digest drift")

    release = profile.get("release_package")
    expected_artifacts = [
        ["OBR", "concepts.obr"],
        ["LABEL_INDEX", "concepts.obr.labels.idx"],
        ["CCID_INDEX", "concepts.obr.ccids.idx"],
        ["MANIFEST", "concepts.obr.manifest.json"],
        ["SPDX_SBOM", "sbom.spdx.json"],
    ]
    if not isinstance(release, dict):
        raise ContractError("Concept Registry production release package missing")
    if release.get("payload_artifacts") != expected_artifacts:
        raise ContractError("Concept Registry production artifact set drift")
    if release.get("verification_stamp") != {
        "filename": "release.stamp.json",
        "signature_algorithm": "Ed25519",
        "signature_domain_hex": (
            "6f6e65627261696e3a636f6e636570742d72656769737472792d"
            "72656c656173652d7374616d703a3100"
        ),
        "signature_message": (
            "domain-bytes-then-blake3-of-serde-json-struct-order-unsigned-stamp"
        ),
        "unsigned_transform": "clone-then-set-signature-to-empty-string",
        "json_encoding": "serde-json-compact-rust-struct-field-order-utf8",
        "signed_fields": [
            "profile",
            "release_id",
            "builder_version",
            "dedup_policy_version",
            "artifacts",
            "artifact_root",
            "sources",
            "source_root",
            "distribution",
            "signer_public_key",
            "signature",
        ],
        "signature_field_value_during_message": "",
        "activation_metadata_fields": ["release_id"],
    }:
        raise ContractError("Concept Registry production verification stamp drift")
    if release.get("obr_size_bytes") != {
        "artifact": "concepts.obr",
        "minimum": 2_200_000_000,
        "maximum": 2_500_000_000,
    }:
        raise ContractError("Concept Registry production OBR size bounds drift")
    if release.get("aggregate_root") != {
        "algorithm": "BLAKE3",
        "domain_hex": (
            "6f6e65627261696e3a636f6e636570742d72656769737472792d"
            "6172746966616374733a3100"
        ),
        "order": "role-then-relative-path-bytewise",
        "fields": [
            "role",
            "relative_path",
            "exact_length_u64_be",
            "blake3_hex",
        ],
        "string_framing": "u64-be-length-then-utf8-bytes",
        "digest_encoding": "lowercase-hex-ascii",
        "includes_verification_stamp": False,
    }:
        raise ContractError("Concept Registry production aggregate root drift")
    if (
        release.get("root_match_policy")
        != "stamp-reports-and-aggregate-must-match-exactly"
    ):
        raise ContractError("Concept Registry production aggregate root match drift")

    resources = profile.get("resource_profiles")
    if not isinstance(resources, dict) or set(resources) != {
        "cold-cache",
        "low-ram",
        "ssd",
        "hdd",
    }:
        raise ContractError("Concept Registry production resource profile set drift")
    expected_budgets = {
        "cold-cache": (180_000, 250_000, 536_870_912, None),
        "low-ram": (300_000, 500_000, 268_435_456, 3_221_225_472),
        "ssd": (120_000, 100_000, 536_870_912, None),
        "hdd": (300_000, 750_000, 536_870_912, None),
    }
    for name, expected in expected_budgets.items():
        row = resources.get(name)
        if not isinstance(row, dict):
            raise ContractError(
                f"Concept Registry production resource budget missing: {name}"
            )
        actual = (
            row.get("max_ready_ms"),
            row.get("max_lookup_p95_us"),
            row.get("max_peak_rss_bytes"),
            row.get("address_space_limit_bytes"),
        )
        if actual != expected:
            raise ContractError(
                f"Concept Registry production resource budget drift: {name}"
            )
    if resources["cold-cache"].get("cache_evidence") != [
        "linux-posix-fadvise-dontneed",
        "vmtouch-evict",
    ]:
        raise ContractError("Concept Registry cold-cache evidence drift")
    if resources["low-ram"].get("enforcement_evidence") != ["linux-rlimit-as"]:
        raise ContractError("Concept Registry low-RAM enforcement evidence drift")
    if resources["ssd"].get("storage_evidence") != [
        "linux-findmnt-source-and-fstype",
        "linux-sysfs-block-device",
        "linux-sysfs-rotational-equals-0",
    ]:
        raise ContractError("Concept Registry SSD storage evidence drift")
    if resources["hdd"].get("storage_evidence") != [
        "linux-findmnt-source-and-fstype",
        "linux-sysfs-block-device",
        "linux-sysfs-rotational-equals-1",
    ]:
        raise ContractError("Concept Registry HDD storage evidence drift")

    if profile.get("reference_environment") != {
        "target_triple": "x86_64-unknown-linux-gnu",
        "identity_source": "verified-signed-release-request",
        "required_pinned_fields": [
            "rust_toolchain_digest",
            "runner_image_digest",
            "probe_blake3",
            "probe_signature",
            "probe_signer_fingerprint",
            "python_executable_blake3",
            "gpg_executable_blake3",
        ],
        "cross_host_equality": [
            "target_triple",
            "rust_toolchain_digest",
            "runner_image_digest",
            "probe_blake3",
            "python_executable_blake3",
            "gpg_executable_blake3",
        ],
        "producer_override": False,
        "portability_collectors": ["windows-preflight", "macos-preflight"],
        "portability_collectors_are_production_reference": False,
    }:
        raise ContractError("Concept Registry production reference environment drift")

    if profile.get("trust_policy") != {
        "canonicalization": "utf8-json-sorted-keys-no-whitespace",
        "digest_algorithm": "BLAKE3-derive-key-v1",
        "digest_context": "onebrain:concept-registry:trust-policy:1",
        "digest_hex": (
            "e0a2551a39823c3f2cb088defe60484c8a33ffe0f3aab9df9493b52557ab55fe"
        ),
        "policy": {
            "algorithm": "Ed25519",
            "allowed_usages": [
                "registry-release-stamp",
                "registry-qualification-receipt",
            ],
            "format": "onebrain/concept-registry-trust-policy/1",
            "signers": [
                {
                    "fingerprint_algorithm": "blake3-derive-key-v1",
                    "fingerprint_context": (
                        "onebrain:concept-registry:signer-fingerprint:1"
                    ),
                    "fingerprint_hex": (
                        "dcc09574ac53ec8b95585cad5e2e88cbdfbe44841ad46b3709f73c989b4316d4"
                    ),
                    "public_key_hex": (
                        "bef8e2b9d8ae7a38b3753a7d756a39c20948f128a66ca71ed04799e7a5d5177c"
                    ),
                }
            ],
        },
        "valid_unlisted_signature": "reject",
        "verification_required_for": [
            "release.stamp.json",
            "every-registry-evidence-receipt",
        ],
    }:
        raise ContractError("Concept Registry production signer trust policy drift")

    if profile.get("qualification_receipt_envelope") != {
        "format": "onebrain/concept-registry-qualification-receipt/1",
        "signature_algorithm": "Ed25519",
        "signature_domain_hex": (
            "6f6e65627261696e3a636f6e636570742d72656769737472792d"
            "7175616c696669636174696f6e2d726563656970743a3100"
        ),
        "signature_message": (
            "domain-bytes-then-blake3-of-canonical-unsigned-envelope"
        ),
        "canonicalization": "utf8-json-sorted-keys-no-whitespace",
        "unsigned_transform": "clone-then-set-signature-to-empty-string",
        "envelope_fields": [
            "format",
            "receipt_kind",
            "usage",
            "payload",
            "signer_public_key",
            "signer_fingerprint",
            "trust_policy_digest",
            "signature",
        ],
        "signature_field_value_during_message": "",
        "usage": "registry-qualification-receipt",
        "closed_receipt_kinds": [
            "resource-qualification",
            "failure-qualification",
            "generation-swap",
            "ccid-stability",
            "signed-release-cycle",
            "production-aggregate",
        ],
        "payload_binding_sets": {
            "common": [
                "qualification_context_variant",
                "release_aggregate_root",
                "registry_generation",
                "production_profile_blake3",
                "trust_policy_digest",
                "signer_fingerprint",
                "probe_blake3",
                "executable_blake3",
                "candidate_payload_artifacts_blake3",
                "release_stamp_blake3",
                "command",
                "result",
                "exit_oracles",
                "limitations",
                "evidence_tier",
            ],
            "prequalification": {
                "required": ["closure_digest", "base_candidate_bound", "evidence_tier"],
                "base_candidate_bound": False,
                "evidence_tier": "prequalification",
                "forbidden": [
                    "release_request_digest",
                    "qualification_session_id",
                    "candidate_commit",
                    "candidate_tree",
                ],
            },
            "release": {
                "required": [
                    "release_request_digest",
                    "qualification_session_id",
                    "candidate_commit",
                    "candidate_tree",
                    "candidate_semantic_digest",
                    "artifact_tuple_digest",
                    "base_candidate_bound",
                    "evidence_tier",
                ],
                "base_candidate_bound": True,
                "production_evidence_tier": "production-reference",
                "nonproduction_test_evidence_tier": "nonproduction-test",
            },
        },
        "unknown_field_policy": "reject",
        "valid_unlisted_signature": "reject",
    }:
        raise ContractError(
            "Concept Registry production qualification receipt envelope drift"
        )

    context = profile.get("qualification_run_context")
    if not isinstance(context, dict):
        raise ContractError("Concept Registry qualification run context missing")
    if (
        context.get("format") != "onebrain/qualification-run-context/1"
        or context.get("closed_variants") != ["Prequalification", "Release"]
        or context.get("prequalification")
        != {
            "required_fields": ["closure_digest"],
            "base_candidate_bound": False,
            "production_aggregate_allowed": False,
        }
    ):
        raise ContractError("Concept Registry prequalification context drift")
    if context.get("release") != {
        "required_fields": [
            "release_request_digest",
            "qualification_session_id",
            "candidate_commit",
            "candidate_tree",
        ],
        "verified_signed_request_required": True,
        "request_match": "exact",
        "producer_override": False,
        "missing_context_policy": "reject",
        "mixed_context_policy": "reject",
        "base_candidate_bound": True,
        "production_aggregate_allowed": True,
    }:
        raise ContractError("Concept Registry release context drift")

    if profile.get("evidence_classes") != {
        "fixture": {
            "production_eligible": False,
            "base_candidate_bound": False,
        },
        "prequalification": {
            "production_eligible": False,
            "base_candidate_bound": False,
        },
        "release": {
            "production_eligible": True,
            "base_candidate_bound": True,
            "fresh_reports_required": True,
        },
    }:
        raise ContractError("Concept Registry production evidence classification drift")

    if profile.get("production_report_binding") != {
        "identical_fields": [
            "release_request_digest",
            "qualification_session_id",
            "candidate_commit",
            "candidate_tree",
            "candidate_semantic_digest",
            "artifact_tuple_digest",
            "release_aggregate_root",
            "registry_generation",
            "production_profile_blake3",
            "trust_policy_digest",
            "signer_fingerprint",
            "probe_blake3",
            "executable_blake3",
            "candidate_payload_artifacts_blake3",
            "release_stamp_blake3",
            "evidence_tier",
        ],
        "component_mismatch_policy": "reject",
        "carry_forward_allowed_for_base_v1": False,
        "registry_subgate_only": True,
    }:
        raise ContractError("Concept Registry production report binding drift")

    expected_failure_gates = [
        "truncated-index",
        "disk-shortage",
        "update-interruption-process-kill",
        "live-reader-generation-swap",
        "rollback",
        "ccid-stability",
        "signed-release-cycle",
    ]
    if profile.get("failure_gates") != expected_failure_gates:
        raise ContractError("Concept Registry production failure gate drift")

    if profile.get("signed_release_cycle") != {
        "accepted_harnesses": ["release_cycle_qualification.py"],
        "rejected_substitutes": ["quarterly_update.py"],
        "required_steps": [
            "package",
            "verify",
            "activate",
            "query",
            "build-new-signed-generation",
            "ccid-diff",
            "activate-new",
            "rollback",
            "reactivate-new",
        ],
        "complete_cycle_required": True,
        "signed_receipt_required": True,
    }:
        raise ContractError("Concept Registry signed release cycle drift")

    if profile.get("qualification_state") != {
        "contract_frozen": True,
        "measured_evidence_committed": False,
        "production_qualified": False,
    }:
        raise ContractError("Concept Registry qualification state drift")

    spec = read(
        VNEXT / "CONCEPT_REGISTRY_PRODUCTION_QUALIFICATION_PROFILE_V1.md"
    )
    if "concept-registry-production-qualification-v1.json" not in spec:
        raise ContractError(
            "Concept Registry production profile is not linked to machine contract"
        )
    workflow = read(VNEXT_FOUNDATION_WORKFLOW)
    if (
        "python -m unittest "
        "scripts.ci.test_validate_concept_registry_production_qualification"
        not in workflow
    ):
        raise ContractError(
            "Concept Registry production negative validator CI gate missing"
        )
    return (
        len(expected_artifacts),
        len(expected_budgets),
        len(expected_failure_gates),
        len(profile["trust_policy"]["policy"]["signers"]),
    )


def validate_concept_registry_operations(
    profile: dict[str, object] | None = None,
) -> tuple[int, int, int, int]:
    if profile is None:
        try:
            profile = json.loads(read(CONCEPT_REGISTRY_OPERATIONS_PROFILE))
        except (OSError, json.JSONDecodeError) as error:
            raise ContractError(
                f"invalid Concept Registry operations profile JSON: {error}"
            ) from error

    if (
        profile.get("format") != "onebrain/concept-registry-operations/1"
        or profile.get("profile_id") != "CONCEPT_REGISTRY_OPERATIONS_PROFILE_V1"
        or profile.get("version") != 1
    ):
        raise ContractError("Concept Registry operations profile identity drift")

    release = profile.get("release_package")
    expected_artifacts = [
        ["OBR", "concepts.obr"],
        ["LABEL_INDEX", "concepts.obr.labels.idx"],
        ["CCID_INDEX", "concepts.obr.ccids.idx"],
        ["MANIFEST", "concepts.obr.manifest.json"],
        ["SPDX_SBOM", "sbom.spdx.json"],
    ]
    if not isinstance(release, dict):
        raise ContractError("Concept Registry release package contract missing")
    if release.get("artifacts") != expected_artifacts:
        raise ContractError("Concept Registry exact artifact set drift")
    if {
        "publication": release.get("publication"),
        "overwrite_existing_release": release.get("overwrite_existing_release"),
        "verification_stamp": release.get("verification_stamp"),
        "signature": release.get("signature"),
        "hash": release.get("hash"),
        "exact_file_set": release.get("exact_file_set"),
    } != {
        "publication": "unique-staging-then-atomic-rename",
        "overwrite_existing_release": False,
        "verification_stamp": "release.stamp.json",
        "signature": "Ed25519",
        "hash": "BLAKE3",
        "exact_file_set": True,
    }:
        raise ContractError("Concept Registry release package security drift")
    if release.get("capacity_preflight") != {
        "filesystem": "releases-directory-filesystem",
        "before_staging": True,
        "source_bytes": "exact-five-source-artifact-lengths",
        "metadata_reserve_bytes": 4_194_304,
        "safety_margin_bytes": 67_108_864,
        "insufficient_space_error": "InsufficientSpace",
        "no_staging_or_state_side_effect": True,
    }:
        raise ContractError("Concept Registry capacity preflight drift")

    provenance = profile.get("provenance")
    expected_sources = ["chebi", "geonames", "ncbi", "wikidata", "wordnet"]
    expected_source_fields = [
        "snapshot_id",
        "source_uri",
        "license",
        "snapshot_blake3",
        "download_blake3",
    ]
    if not isinstance(provenance, dict):
        raise ContractError("Concept Registry provenance contract missing")
    if provenance.get("required_sources") != expected_sources:
        raise ContractError("Concept Registry source set drift")
    if provenance.get("required_source_fields") != expected_source_fields:
        raise ContractError("Concept Registry source provenance drift")
    if provenance.get("signed_fields") != [
        "builder_version",
        "dedup_policy_version",
        "artifact_root",
        "source_root",
        "distribution",
    ]:
        raise ContractError("Concept Registry signed provenance drift")

    activation = profile.get("activation")
    if not isinstance(activation, dict):
        raise ContractError("Concept Registry activation contract missing")
    if {
        "publication": activation.get("publication"),
        "old_new_coexist": activation.get("old_new_coexist"),
        "rollback": activation.get("rollback"),
        "ignore_invalid_or_truncated_newest_state": activation.get(
            "ignore_invalid_or_truncated_newest_state"
        ),
        "interrupted_staging_preserves_active": activation.get(
            "interrupted_staging_preserves_active"
        ),
    } != {
        "publication": "append-only-create-new",
        "old_new_coexist": True,
        "rollback": "append-previous-as-new-generation",
        "ignore_invalid_or_truncated_newest_state": True,
        "interrupted_staging_preserves_active": True,
    }:
        raise ContractError("Concept Registry activation/rollback drift")

    runtime = profile.get("runtime")
    if not isinstance(runtime, dict) or {
        "verify_signature_and_all_artifact_hashes_before_open": runtime.get(
            "verify_signature_and_all_artifact_hashes_before_open"
        ),
        "signed_release_verification_cache": runtime.get(
            "signed_release_verification_cache"
        ),
        "required_mode_fallback": runtime.get("required_mode_fallback"),
        "status_fields": runtime.get("status_fields"),
    } != {
        "verify_signature_and_all_artifact_hashes_before_open": True,
        "signed_release_verification_cache": False,
        "required_mode_fallback": False,
        "status_fields": ["release_id", "release_generation"],
    }:
        raise ContractError("Concept Registry runtime fail-closed policy drift")

    distribution = profile.get("distribution")
    if not isinstance(distribution, dict) or {
        "policy": distribution.get("policy"),
        "obp_artifact_gossip": distribution.get("obp_artifact_gossip"),
        "allowed": distribution.get("allowed"),
    } != {
        "policy": "MIRROR_OR_OFFLINE_ONLY_NO_OBP_GOSSIP",
        "obp_artifact_gossip": False,
        "allowed": ["content-addressed-chunks", "mirrors", "offline-media"],
    }:
        raise ContractError("Concept Registry distribution boundary drift")

    ccid_diff = profile.get("ccid_stability_diff")
    if not isinstance(ccid_diff, dict) or ccid_diff != {
        "profile": "onebrain/ccid-stability-diff/1",
        "algorithm": "actual-obr-ccid-by-stable-source-identity-v1",
        "compares_actual_obr_ccids": True,
        "stable_identity_source": "exact-builder-input-jsonl",
        "join": "disk-backed-sqlite",
        "memory_bounded": True,
        "exit_oracles": [
            "has_stable_source_identity_overlap",
            "all_stable_source_identities_keep_ccid",
            "old_release_has_no_ccid_collision",
            "new_release_has_no_ccid_collision",
        ],
    }:
        raise ContractError("Concept Registry CCID stability contract drift")

    resource_qualification = profile.get("resource_qualification")
    if not isinstance(resource_qualification, dict) or resource_qualification != {
        "profile": "onebrain/concept-registry-resource-qualification/1",
        "probe_profile": "onebrain/concept-registry-probe/1",
        "implemented_profiles": ["cold-cache", "low-ram"],
        "fresh_process": True,
        "verification_cache": "uncached",
        "lookup_cache_capacity": 0,
        "labels_source": "external-bounded-file",
        "evidence_publication": "atomic-json-replace",
        "cold_cache_strategies": [
            "linux-posix-fadvise-dontneed",
            "vmtouch-evict",
        ],
        "low_ram_enforcement": ["linux-rlimit-as"],
        "rss_collectors": [
            "linux-proc-vmhwm",
            "macos-ps-rss",
            "windows-psapi-peak-working-set",
        ],
        "budget_profiles": {
            "ci-small-fixture-v1": {
                "max_ready_ms": 60_000,
                "max_p95_us": 1_000_000,
                "max_peak_rss_bytes": 268_435_456,
                "address_space_limit_bytes": 536_870_912,
            },
            "cold-cache-production-v1": {
                "max_ready_ms": 180_000,
                "max_p95_us": 250_000,
                "max_peak_rss_bytes": 536_870_912,
                "address_space_limit_bytes": None,
            },
            "low-ram-production-v1": {
                "max_ready_ms": 300_000,
                "max_p95_us": 500_000,
                "max_peak_rss_bytes": 268_435_456,
                "address_space_limit_bytes": 3_221_225_472,
            },
        },
        "ci_scope": "small-fixture-contract-only",
        "full_registry_evidence_required": True,
    }:
        raise ContractError("Concept Registry resource qualification contract drift")

    failure_qualification = profile.get("failure_qualification")
    if not isinstance(failure_qualification, dict) or failure_qualification != {
        "profile": "onebrain/concept-registry-failure-qualification/1",
        "implemented_profiles": ["truncated-index", "disk-shortage"],
        "truncated_artifacts": [
            "concepts.obr.labels.idx",
            "concepts.obr.ccids.idx",
        ],
        "disk_capacity_source": "filesystem-containing-releases-directory",
        "fault_injection_scope": "concept-registry-failure-harness-feature-only",
        "evidence_publication": "atomic-json-rename",
        "active_release_preserved": True,
        "full_registry_evidence_required": True,
        "production_qualified_by_ci_fixture": False,
        "exit_oracles": [
            "truncated_label_index_rejected",
            "truncated_ccid_index_rejected",
            "disk_shortage_rejected_before_publication",
            "disk_shortage_left_no_final_release",
            "disk_shortage_left_no_staging_directory",
            "active_release_survived_every_failure",
        ],
    }:
        raise ContractError("Concept Registry failure qualification contract drift")

    drills = profile.get("implemented_failure_drills")
    remaining = profile.get("remaining_qualification_gates")
    if not isinstance(drills, list) or len(drills) != 11:
        raise ContractError("Concept Registry implemented failure drills drift")
    if profile.get("production_qualification") != {
        "profile": "onebrain/concept-registry-production-qualification/1",
        "machine_contract": "concept-registry-production-qualification-v1.json",
        "status": "contract-frozen-evidence-open",
        "completion_claimed": False,
    }:
        raise ContractError("Concept Registry production qualification link drift")
    if remaining != ["CONCEPT_REGISTRY_PRODUCTION_QUALIFICATION_PROFILE_V1"]:
        raise ContractError("Concept Registry remaining qualification gates drift")

    release_source = read(ROOT / "src/ku-core/src/concept_registry_release.rs")
    for needle in [
        'pub const CONCEPT_REGISTRY_RELEASE_PROFILE: &str = "onebrain/concept-registry-release/1"',
        "pub fn package_concept_registry_release(",
        "pub fn concept_registry_release_capacity(",
        "pub fn verify_concept_registry_release(",
        "pub fn activate_concept_registry_release(",
        "pub fn rollback_concept_registry_release(",
        'distribution: "MIRROR_OR_OFFLINE_ONLY_NO_OBP_GOSSIP".to_owned()',
        "load_and_validate_manifest_uncached",
        "ConceptRegistryReleaseError::InsufficientSpace",
        "StagingDirectoryGuard::new",
    ]:
        if needle not in release_source:
            raise ContractError(
                f"Concept Registry release implementation evidence missing: {needle}"
            )

    runtime_source = read(ROOT / "src/onebrain-node/src/concept_registry_runtime.rs")
    for needle in [
        "resolve_active_concept_registry_release",
        "pub release_id: Option<String>",
        "pub release_generation: Option<u64>",
        "required_mode_loads_only_the_verified_active_release_without_cache_mutation",
        "required_release_mode_never_falls_back_when_activation_is_missing",
    ]:
        if needle not in runtime_source:
            raise ContractError(
                f"Concept Registry runtime evidence missing: {needle}"
            )

    failure_source = read(
        ROOT / "src/ku-core/examples/concept_registry_failure_qualification.rs"
    )
    for needle in [
        'const PROFILE: &str = "onebrain/concept-registry-failure-qualification/1"',
        "package_concept_registry_release_with_capacity_for_drill",
        '"concepts.obr.labels.idx"',
        '"concepts.obr.ccids.idx"',
        '"production_qualified": false',
        "write_report_atomic",
    ]:
        if needle not in failure_source:
            raise ContractError(
                f"Concept Registry failure qualification evidence missing: {needle}"
            )

    ccid_diff_source = read(
        ROOT / "scripts/concept_registry/ccid_stability_diff.py"
    )
    for needle in [
        'PROFILE = "onebrain/ccid-stability-diff/1"',
        'ALGORITHM = "actual-obr-ccid-by-stable-source-identity-v1"',
        "def _ingest_pair(",
        "sqlite3.connect(database_path)",
        "WHERE old.ccid != new.ccid",
        "stable_count > 0",
    ]:
        if needle not in ccid_diff_source:
            raise ContractError(
                f"Concept Registry CCID stability evidence missing: {needle}"
            )

    resource_source = read(
        ROOT / "scripts/concept_registry/resource_qualification.py"
    )
    for needle in [
        'PROFILE = "onebrain/concept-registry-resource-qualification/1"',
        'PROBE_PROFILE = "onebrain/concept-registry-probe/1"',
        '"cold-cache-production-v1"',
        '"low-ram-production-v1"',
        "os.posix_fadvise",
        "resource.RLIMIT_AS",
        "application_lookup_cache_is_disabled",
        "targeted_cache_eviction_request_completed",
        "hard_address_space_limit_applied",
        "os.replace(temporary_path, path)",
    ]:
        if needle not in resource_source:
            raise ContractError(
                f"Concept Registry resource qualification evidence missing: {needle}"
            )

    probe_source = read(ROOT / "src/ku-core/examples/registry_probe.rs")
    for needle in [
        'const PROBE_PROFILE: &str = "onebrain/concept-registry-probe/1"',
        "load_and_validate_manifest_uncached",
        'value == "--labels-file"',
        'value == "--cache-capacity"',
        'value == "--verification-cache"',
        "sampled_from_obr",
    ]:
        if needle not in probe_source:
            raise ContractError(
                f"Concept Registry resource probe evidence missing: {needle}"
            )

    workflow = read(VNEXT_FOUNDATION_WORKFLOW)
    for needle in [
        "python -m unittest scripts.ci.test_validate_concept_registry_operations",
        "python -m unittest scripts.concept_registry.test_ccid_stability_diff",
        "python -m unittest scripts.concept_registry.test_resource_qualification",
        "python -m unittest scripts.concept_registry.test_failure_qualification",
        "--example registry_probe",
        "--example concept_registry_failure_qualification",
        "Concept Registry signed release and atomic activation",
        "--example concept_registry_release",
    ]:
        if needle not in workflow:
            raise ContractError(
                f"Concept Registry CI acceptance gate missing: {needle}"
            )

    spec = read(VNEXT / "CONCEPT_REGISTRY_OPERATIONS_PROFILE_V1.md")
    if "concept-registry-operations-v1.json" not in spec:
        raise ContractError(
            "Concept Registry normative profile is not linked to machine contract"
        )
    return (len(expected_artifacts), len(expected_sources), len(drills), len(remaining))


def validate_markdown_links() -> int:
    files = sorted(VNEXT.rglob("*.md")) + [PLAN]
    checked = 0
    for markdown in files:
        text = read(markdown)
        if text.count("```") % 2:
            raise ContractError(f"unbalanced code fence in {markdown.relative_to(ROOT)}")
        for raw_target in MARKDOWN_LINK.findall(text):
            target = raw_target.strip().strip("<>").split("#", 1)[0]
            if not target or re.match(r"^(?:https?://|mailto:)", target):
                continue
            path = (markdown.parent / target).resolve()
            if not path.exists():
                raise ContractError(
                    f"broken link in {markdown.relative_to(ROOT)}: {raw_target}"
                )
            checked += 1
    return checked


def validate_normative_coverage() -> int:
    try:
        manifest = json.loads(read(NORMATIVE_COVERAGE))
    except json.JSONDecodeError as error:
        raise ContractError(f"invalid normative coverage JSON: {error}") from error
    if manifest.get("format") != "onebrain/normative-coverage/1":
        raise ContractError("unexpected normative coverage format")

    actual: dict[str, int] = {}
    for markdown in sorted(VNEXT.rglob("*.md")):
        count = sum(
            1 for line in read(markdown).splitlines() if NORMATIVE_KEYWORD.search(line)
        )
        if count:
            actual[markdown.relative_to(ROOT).as_posix()] = count

    rows = manifest.get("files")
    if not isinstance(rows, list) or not rows:
        raise ContractError("normative coverage files must be a non-empty list")
    declared: dict[str, int] = {}
    for row in rows:
        if not isinstance(row, dict):
            raise ContractError("invalid normative coverage row")
        path = row.get("path")
        expected = row.get("expected_statement_lines")
        evidence = row.get("evidence")
        rationale = row.get("rationale")
        if not isinstance(path, str) or not isinstance(expected, int) or expected <= 0:
            raise ContractError("invalid normative coverage path/count")
        if path in declared:
            raise ContractError(f"duplicate normative coverage path: {path}")
        if not isinstance(rationale, str) or not rationale.strip():
            raise ContractError(f"normative coverage lacks rationale: {path}")
        if not isinstance(evidence, list) or not evidence:
            raise ContractError(f"normative coverage lacks evidence: {path}")
        for item in evidence:
            if not isinstance(item, dict):
                raise ContractError(f"invalid normative evidence row: {path}")
            evidence_path = item.get("path")
            needle = item.get("needle")
            if not isinstance(evidence_path, str) or not isinstance(needle, str) or not needle:
                raise ContractError(f"invalid normative evidence reference: {path}")
            target = ROOT / evidence_path
            if not target.is_file():
                raise ContractError(f"missing normative evidence file: {evidence_path}")
            if needle not in read(target):
                raise ContractError(
                    f"normative evidence needle missing in {evidence_path}: {needle}"
                )
        declared[path] = expected

    if set(actual) != set(declared):
        missing = sorted(set(actual) - set(declared))
        stale = sorted(set(declared) - set(actual))
        raise ContractError(
            f"normative coverage file mismatch; missing={missing}, stale={stale}"
        )
    drift = sorted(
        (path, actual[path], declared[path])
        for path in actual
        if actual[path] != declared[path]
    )
    if drift:
        raise ContractError(f"normative statement coverage count drift: {drift}")
    return sum(actual.values())


def validate_base_v1_packaging() -> int:
    """Validate Base-default features and fail-closed legacy/harness fences."""

    packages = {
        "onebrain-node": ROOT / "src/onebrain-node/Cargo.toml",
        "onebrain-api": ROOT / "src/onebrain-api/Cargo.toml",
        "onebrain-cli": ROOT / "src/onebrain-cli/Cargo.toml",
    }
    for name, path in packages.items():
        document = tomllib.loads(path.read_text(encoding="utf-8"))
        features = document.get("features", {})
        if features.get("default") != ["base-v1"]:
            raise ContractError(f"{name} must default to base-v1 only")
        if "base-v1" not in features or "legacy-read-compat" not in features:
            raise ContractError(f"{name} is missing Base/legacy feature declarations")
        if "base-v1" in features["legacy-read-compat"]:
            raise ContractError(
                f"{name} legacy-read-compat must not silently auto-enable base-v1"
            )
    feature_guards = {
        ROOT / "src/onebrain-node/src/lib.rs": "legacy-read-compat requires base-v1",
        ROOT / "src/onebrain-api/src/lib.rs": "legacy-read-compat requires base-v1",
        ROOT / "src/onebrain-cli/src/main.rs": "legacy-read-compat requires base-v1",
    }
    for path, guard in feature_guards.items():
        if guard not in path.read_text(encoding="utf-8"):
            raise ContractError(f"missing forbidden-combination guard in {path}")
    cli_archive = (ROOT / "src/onebrain-cli/src/cli/data.rs").read_text(
        encoding="utf-8"
    )
    for forbidden in ("node.create_backup(", "node.restore_backup("):
        if forbidden in cli_archive:
            raise ContractError(
                f"CLI archive path bypasses the Base scoped facade: {forbidden}"
            )
    api_archive = (ROOT / "src/onebrain-api/src/handlers.rs").read_text(
        encoding="utf-8"
    )
    for forbidden in ("node.create_backup(", "node.restore_backup("):
        if forbidden in api_archive:
            raise ContractError(
                f"API archive path bypasses the Base scoped facade: {forbidden}"
            )
    for required in (
        "issue_base_management_grant",
        "ArchiveSinkBegin",
        "ArchiveSourceBegin",
        "ArchiveCapabilityHandleV1",
        "management.close().await",
    ):
        if required not in cli_archive:
            raise ContractError(f"CLI Base archive lifecycle is missing {required}")
    abi = tomllib.loads(
        (ROOT / "src/onebrain-base-abi/Cargo.toml").read_text(encoding="utf-8")
    )
    if abi.get("lib", {}).get("crate-type") != ["cdylib", "staticlib", "rlib"]:
        raise ContractError("onebrain-base-abi crate types drifted")
    node_features = tomllib.loads(
        (ROOT / "src/onebrain-node/Cargo.toml").read_text(encoding="utf-8")
    )["features"]
    for forbidden in (
        "vnext-canary-harness",
        "vnext-crash-harness",
        "vnext-chaos-harness",
        "vnext-compaction-harness",
        "vnext-soak-harness",
    ):
        if forbidden in node_features["default"] or forbidden in node_features["base-v1"]:
            raise ContractError(f"production Base feature includes {forbidden}")
    workflow = (ROOT / ".github/workflows/vnext-foundation.yml").read_text(
        encoding="utf-8"
    )
    for marker in (
        "base-v1-projections",
        "Run the Base v1 integrated root and tuple gate",
        "-p onebrain-node --test base_gate_integration",
        "-p onebrain-base-contract --test cross_consumer_tuple",
        "-p onebrain-api --test base_contract",
        "-p onebrain-cli --test version",
        "--no-default-features --features base-v1",
        "--no-default-features --features base-v1,legacy-read-compat",
        "onebrain-api/vnext-network-runtime,onebrain-cli/vnext-network-runtime",
        "validate_base_abi_header.py",
        "dart pub get --enforce-lockfile",
        "install_base_v1_cbindgen.ps1",
        "ONEBRAIN_BASE_CBINDGEN",
    ):
        if marker not in workflow:
            raise ContractError(f"Base packaging workflow is missing marker: {marker}")
    if "--skip-tool-verification" in workflow:
        raise ContractError("Base ABI CI may not bypass the pinned executable hash")
    integration_test = read(
        ROOT / "src/onebrain-node/tests/base_gate_integration.rs"
    )
    for marker in (
        "BaseIntegrationReceipt",
        "canonical_root_before_restart",
        "canonical_root_after_restart",
        "archive_restore_root",
        "registry_release_root",
        "default_active_network_lanes",
        "legacy_write_enabled",
    ):
        if marker not in integration_test:
            raise ContractError(f"Base integration fixture is missing marker: {marker}")
    cross_consumer = read(
        ROOT / "src/onebrain-base-contract/tests/cross_consumer_tuple.rs"
    )
    for marker in (
        "candidate_semantic_digest",
        "artifact_tuple_digest",
        "generated TypeScript",
        "generated Dart",
        "Axum API",
        "CLI verbose version",
        "C ABI",
    ):
        if marker not in cross_consumer:
            raise ContractError(f"Base cross-consumer gate is missing marker: {marker}")
    return len(packages) + 1


def validate_base_v1_candidate_workflow(
    workflow: str | None = None,
    runbook: str | None = None,
) -> tuple[int, int]:
    """Validate the closed Task 26 workflow without executing qualification."""
    if workflow is None:
        workflow = read(BASE_V1_CANDIDATE_WORKFLOW)
    if runbook is None:
        runbook = read(ROOT / "docs/operations/ONEBRAIN_BASE_V1_CANDIDATE_RUNBOOK.md")

    required_actions = {
        "actions/checkout": "fbc6f3992d24b796d5a048ff273f7fcc4a7b6c09",
        "actions/setup-python": "ece7cb06caefa5fff74198d8649806c4678c61a1",
        "actions/setup-node": "a0853c24544627f65ddf259abe73b1d18a591444",
        "dart-lang/setup-dart": "65eb853c7ba17dde3be364c3d2858773e7144260",
        "actions/upload-artifact": "ea165f8d65b6e75b540449e92b4886f43607fa02",
        "actions/download-artifact": "634f93cb2916e3fdff6788551b99b062d0335ce0",
    }
    required_action_counts = {
        "actions/checkout": 3,
        "actions/setup-python": 3,
        "actions/setup-node": 1,
        "dart-lang/setup-dart": 1,
        "actions/upload-artifact": 2,
        "actions/download-artifact": 2,
    }
    references = re.findall(
        r"^\s*(?:-\s+)?uses:\s*([^\s#]+)", workflow, re.MULTILINE
    )
    seen_actions: set[str] = set()
    for reference in references:
        if reference.startswith("./"):
            continue
        if "@" not in reference:
            raise ContractError(f"Base candidate action has no revision: {reference}")
        action, revision = reference.rsplit("@", 1)
        if not re.fullmatch(r"[0-9a-f]{40}", revision):
            raise ContractError(f"Base candidate action is not pinned by full SHA: {reference}")
        if required_actions.get(action) != revision:
            raise ContractError(f"Base candidate action is unknown or unreviewed: {reference}")
        seen_actions.add(action)
    if seen_actions != set(required_actions):
        raise ContractError(
            f"Base candidate reviewed action set drift: {sorted(seen_actions)}"
        )
    for action, count in required_action_counts.items():
        reference = f"{action}@{required_actions[action]}"
        if references.count(reference) != count:
            raise ContractError(
                f"Base candidate action occurrence drift: {action}"
            )
    for action, revision in required_actions.items():
        if f"`{action}`" not in runbook or f"`{revision}`" not in runbook:
            raise ContractError(f"Base candidate action mapping missing from runbook: {action}")

    dispatch = workflow.split("workflow_dispatch:", 1)
    if len(dispatch) != 2:
        raise ContractError("Base candidate workflow_dispatch is missing")
    dispatch_inputs = dispatch[1].split("\npermissions:", 1)[0]
    for forbidden in (
        "release_request_digest",
        "qualification_session_id",
        "candidate_commit",
        "candidate_tree",
        "candidate_semantic_digest",
        "artifact_tuple_digest",
    ):
        if re.search(rf"^\s{{6}}{forbidden}:\s*$", dispatch_inputs, re.MULTILINE):
            raise ContractError(f"Base candidate workflow exposes identity input: {forbidden}")

    required_markers = (
        "pull_request:",
        "push:",
        "qualification_mode:",
        "signed_request_run_id:",
        "base-v1-signed-release-request",
        "approver-public-key.gpg",
        "approver-policy.json",
        "verify_base_release_request.py",
        "needs.verify-candidate-identity.outputs.candidate_commit",
        "needs.verify-candidate-identity.outputs.candidate_tree",
        "ubuntu-24.04",
        "windows-2025",
        "macos-15",
        "python-version: '3.13'",
        "--only-binary=:all: --require-hashes",
        "x86_64-unknown-linux-gnu",
        "x86_64-pc-windows-msvc",
        "aarch64-apple-darwin",
        "linux_artifact_tuple_digest",
        "windows_artifact_tuple_digest",
        "macos_artifact_tuple_digest",
        "EXPECTED_ARTIFACT_TUPLE",
        "cargo fmt --all --manifest-path src/Cargo.toml -- --check",
        "cargo check --workspace --locked --manifest-path src/Cargo.toml",
        "cargo clippy --workspace --all-targets --locked --manifest-path src/Cargo.toml -- -D warnings",
        "cargo test --workspace --locked --manifest-path src/Cargo.toml",
        "onebrain-node/vnext-network-runtime,onebrain-api/vnext-network-runtime,onebrain-cli/vnext-network-runtime",
        "python scripts/ci/validate_vnext_contracts.py",
        "python scripts/ci/validate_mobile_build_contracts.py",
        "python scripts/base/generate_contract.py --check",
        "npm test --prefix src/onebrain-base-contract/conformance/typescript",
        "dart pub get --enforce-lockfile",
        "dart analyze",
        "dart test",
        "-p onebrain-archive",
        "--test archive_roundtrip --test dataset_generation_failpoints",
        "test_validate_concept_registry_operations",
        "test_validate_vnext_p5_canary_preflight",
        "test_validate_vnext_p5_operations_preflight",
        "--no-default-features --features base-v1",
        "cargo audit --file src/Cargo.lock --json",
        "cargo install --locked --version 0.22.2 cargo-audit",
        "npm audit --package-lock-only --json",
        "generate_base_sbom.py",
        "verify_base_provenance.py",
        "--ignored=matching",
        "compression-level: 0",
        "overwrite: false",
        "retention-days: 90",
        "CARGO_TARGET_DIR=$env:CARGO_TARGET_DIR",
        "ONEBRAIN_BASE_V1_IDL_BASELINE_RECEIPT",
        "BASE_V1_IDL_BASELINE_RECEIPT=$env:BASE_V1_IDL_BASELINE_RECEIPT",
        "refs/heads/base-v1-idl-baseline:refs/heads/base-v1-idl-baseline",
        "rustc-vV.txt",
        "ImageVersion",
        "'qualification_mode':os.environ['QUALIFICATION_MODE']",
    )
    for marker in required_markers:
        if marker not in workflow:
            raise ContractError(f"Base candidate workflow is missing marker: {marker}")
    for marker, count in (
        ("python-version: '3.13'", 3),
        ("--only-binary=:all: --require-hashes", 3),
    ):
        if workflow.count(marker) != count:
            raise ContractError(f"Base candidate workflow marker count drift: {marker}")
    for forbidden in (
        "permissions:\n  contents: write",
        "continue-on-error: true",
        "overwrite: true",
        "actions/checkout@v",
        "actions/setup-python@v",
        "actions/upload-artifact@v",
        "actions/download-artifact@v",
    ):
        if forbidden in workflow:
            raise ContractError(f"Base candidate workflow contains forbidden marker: {forbidden}")
    return (3, len(required_actions))


def main() -> int:
    try:
        tasks, _ = plan_tasks()
        adrs = validate_traceability(tasks)
        base_signer_domains, base_archive_classes = (
            validate_base_v1_authority_recovery()
        )
        base_storage_boundaries, base_storage_negative_oracles = (
            validate_base_v1_storage_integrity()
        )
        base_archive_kinds, base_archive_required = validate_base_v1_archive()
        baseline_receipt = os.environ.get("BASE_V1_IDL_BASELINE_RECEIPT")
        if baseline_receipt:
            baseline_profile, baseline_history = load_base_v1_runtime_baseline(
                Path(baseline_receipt)
            )
            (
                base_runtime_operations,
                base_runtime_topics,
                base_runtime_errors,
            ) = validate_base_v1_runtime_interface(
                baseline_profile=baseline_profile,
                baseline_history=baseline_history,
            )
        else:
            (
                base_runtime_operations,
                base_runtime_topics,
                base_runtime_errors,
            ) = validate_base_v1_runtime_interface()
        base_compatibility_vectors = validate_base_v1_compatibility()
        base_freeze_gates = validate_base_v1_freeze()
        base_packaging_surfaces = validate_base_v1_packaging()
        base_candidate_os_lanes, base_candidate_actions = (
            validate_base_v1_candidate_workflow()
        )
        assertions = validate_negative_assertions()
        vector_count, domains, schema_vectors, event_vectors = validate_vectors()
        product_endpoints, product_dtos = validate_product_integration_profile()
        ws_events, ws_topics = validate_private_websocket_profile()
        cli_commands = validate_vnext_cli_profile()
        ux_receipt_vectors = validate_vnext_desktop_web_ux_profile()
        dr_m5_boundaries, dr_m5_oracle_fields = validate_vnext_dr_m5_baseline()
        m5_resource_lanes, m5_state_bounds, m5_exit_oracles = (
            validate_vnext_dr_m5_resource_admission()
        )
        m5_reason_codes, m5_runtime_gauges, m5_observability_oracles = (
            validate_vnext_dr_m5_observability()
        )
        (
            m5_crash_boundaries,
            m5_crash_phases,
            m5_crash_cases,
            m5_crash_faults,
        ) = validate_vnext_dr_m5_crash_harness()
        (
            m5_chaos_scenarios,
            m5_flood_scenarios,
            m5_fuzz_targets,
            m5_corpus_cases,
            m5_chaos_exit_oracles,
        ) = validate_vnext_dr_m5_chaos_fuzz()
        (
            m5_compaction_boundaries,
            m5_compaction_phases,
            m5_compaction_cases,
            m5_compaction_derived_lanes,
            m5_compaction_exit_oracles,
        ) = validate_vnext_dr_m5_operational_compaction()
        (
            m5_rollback_lanes,
            m5_n_minus_one_fixtures,
            m5_rollback_phases,
            m5_rollback_exit_oracles,
        ) = validate_vnext_dr_m5_mixed_rollback()
        (
            m5_soak_profiles,
            m5_performance_metrics,
            m5_growth_metrics,
            m5_fault_cycles,
            m5_soak_exit_oracles,
        ) = validate_vnext_dr_m5_soak_release()
        (
            p5_physical_hosts,
            p5_inventory_hosts,
            p5_production_faults,
            p5_production_exit_oracles,
            p5_evidence_roles,
        ) = validate_vnext_p5_multi_host()
        (
            base_soak_runners,
            base_soak_roles,
            base_soak_receipt_kinds,
            base_soak_exit_oracles,
        ) = validate_base_v1_exact_candidate_soak()
        (
            p5_canary_nodes,
            p5_ring_deliveries,
            p5_route_observations,
            p5_fault_drills,
            p5_exit_oracles,
        ) = validate_vnext_p5_canary_preflight()
        (
            p5_operations_faults,
            p5_durable_files,
            p5_rollout_lanes,
            p5_feature_flags,
            p5_dashboard_signals,
            p5_incident_codes,
            p5_external_gates,
        ) = validate_vnext_p5_operations_preflight()
        (
            registry_release_artifacts,
            registry_sources,
            registry_failure_drills,
            registry_remaining_gates,
        ) = validate_concept_registry_operations()
        (
            registry_production_artifacts,
            registry_production_resources,
            registry_production_failure_gates,
            registry_production_signers,
        ) = validate_concept_registry_production_qualification()
        (
            registry_runner_checks,
            registry_runner_guide_checks,
            registry_runner_workflow_checks,
            registry_runner_fixture_checks,
        ) = validate_concept_registry_runner_kit()
        links = validate_markdown_links()
        normative_lines = validate_normative_coverage()
    except ContractError as error:
        print(f"vNext contract validation failed: {error}", file=sys.stderr)
        return 1

    print(
        "vNext contracts OK: "
        f"{len(tasks)} tasks, {adrs} ADRs, {assertions} negative assertions, "
        f"{vector_count} foundation vectors/{domains} domains, "
        f"{base_signer_domains} Base signer domains/{base_archive_classes} archive classes, "
        f"{base_storage_boundaries} Base storage boundaries/"
        f"{base_storage_negative_oracles} negative oracles, "
        f"{base_archive_kinds} Base archive kinds/"
        f"{base_archive_required} required metadata, "
        f"{base_runtime_operations} Base runtime operations/"
        f"{base_runtime_topics} topics/{base_runtime_errors} errors, "
        f"{base_compatibility_vectors} Base compatibility vectors/"
        f"{base_freeze_gates} Base freeze gates/"
        f"{base_packaging_surfaces} packaging surfaces, "
        f"{base_candidate_os_lanes} Base candidate OS lanes/"
        f"{base_candidate_actions} pinned actions, "
        f"{schema_vectors} identity-object vectors, "
        f"{event_vectors} feed-event vectors, {normative_lines} normative lines, "
        f"{product_endpoints} product endpoints/{product_dtos} DTOs, "
        f"{ws_events} private-WS events/{ws_topics} topics, "
        f"{cli_commands} vNext CLI commands, "
        f"{ux_receipt_vectors} Desktop/Web receipt vectors, "
        f"{dr_m5_boundaries} DR-M5 boundaries/{dr_m5_oracle_fields} oracle fields, "
        f"{m5_resource_lanes} M5-01 lanes/{m5_state_bounds} state bounds/"
        f"{m5_exit_oracles} exit oracles, "
        f"{m5_reason_codes} M5-02 reasons/{m5_runtime_gauges} gauges/"
        f"{m5_observability_oracles} exit oracles, "
        f"{m5_crash_boundaries} M5-03 boundaries/{m5_crash_phases} phases/"
        f"{m5_crash_cases} process kills/{m5_crash_faults} storage faults, "
        f"{m5_chaos_scenarios} M5-04 chaos/{m5_flood_scenarios} floods/"
        f"{m5_fuzz_targets} fuzz targets/{m5_corpus_cases} corpus cases/"
        f"{m5_chaos_exit_oracles} exit oracles, "
        f"{m5_compaction_boundaries} M5-05 boundaries/"
        f"{m5_compaction_phases} phases/{m5_compaction_cases} process kills/"
        f"{m5_compaction_derived_lanes} derived lanes/"
        f"{m5_compaction_exit_oracles} exit oracles, "
        f"{m5_rollback_lanes} M5-06 lanes/{m5_n_minus_one_fixtures} N-1 fixtures/"
        f"{m5_rollback_phases} process-kill phases/"
        f"{m5_rollback_exit_oracles} exit oracles, "
        f"{m5_soak_profiles} M5-07 profiles/{m5_performance_metrics} performance metrics/"
        f"{m5_growth_metrics} growth metrics/{m5_fault_cycles} fault cycles/"
        f"{m5_soak_exit_oracles} exit oracles, "
        f"{p5_physical_hosts} P5 production hosts/{p5_inventory_hosts} inventory hosts/"
        f"{p5_production_faults} production faults/{p5_production_exit_oracles} exit oracles/"
        f"{p5_evidence_roles} evidence roles, "
        f"{base_soak_runners} Base exact-soak runners/{base_soak_roles} roles/"
        f"{base_soak_receipt_kinds} receipt kinds/{base_soak_exit_oracles} exit oracles, "
        f"{p5_canary_nodes} P5-01 nodes/{p5_ring_deliveries} ring deliveries/"
        f"{p5_route_observations} route observations/{p5_fault_drills} fault drills/"
        f"{p5_exit_oracles} exit oracles, "
        f"{p5_operations_faults} P5-02 faults/{p5_durable_files} P5-03 durable files/"
        f"{p5_rollout_lanes} P5-04 lanes/{p5_feature_flags} P5-05 default-off flags/"
        f"{p5_dashboard_signals} P5-06 signals/{p5_incident_codes} incident codes/"
        f"{p5_external_gates} external gates, "
        f"{registry_release_artifacts} registry artifacts/{registry_sources} sources/"
        f"{registry_failure_drills} failure drills/{registry_remaining_gates} remaining gates, "
        f"{registry_production_artifacts} production registry payloads/"
        f"{registry_production_resources} resource profiles/"
        f"{registry_production_failure_gates} failure gates/"
        f"{registry_production_signers} approved signer, "
        f"{registry_runner_checks} Registry runner checks/"
        f"{registry_runner_guide_checks} guide checks/"
        f"{registry_runner_workflow_checks} workflow checks/"
        f"{registry_runner_fixture_checks} fixture checks, "
        f"{links} local links"
    )
    return 0


if __name__ == "__main__":
    raise SystemExit(main())

#!/usr/bin/env python3
"""Validate the OneBrain vNext 10-pillar architecture & cross-pillar contract."""

from __future__ import annotations

import json
import sys
from pathlib import Path
from typing import Any

if __package__:
    from .vnext_contract_common import ContractError, ROOT, read
else:
    SCRIPT_DIR = Path(__file__).resolve().parent
    if str(SCRIPT_DIR) not in sys.path:
        sys.path.insert(0, str(SCRIPT_DIR))
    from vnext_contract_common import ContractError, ROOT, read  # type: ignore[no-redef]

TEN_PILLAR_CONTRACT = ROOT / "src/test-vectors/vnext/ten-pillar-architecture-v1.json"
EXPECTED_PILLAR_IDS = [f"P{i}" for i in range(1, 11)]
EXPECTED_FIREWALL_IDS = [f"FW-{i:02d}" for i in range(1, 11)]


def _read_spec(rel_path: str, spec_overrides: dict[str, str] | None) -> str:
    if spec_overrides and rel_path in spec_overrides:
        return spec_overrides[rel_path]
    path = ROOT / rel_path
    if not path.is_file():
        raise ContractError(f"missing 10-pillar spec file: {rel_path}")
    return read(path)


def validate_ten_pillar_contracts(
    contract_data: dict[str, Any] | None = None,
    spec_overrides: dict[str, str] | None = None,
) -> tuple[int, int, int, int]:
    """Verify 10-pillar spec closure, firewalls, acyclicity, and honest reachability."""

    if contract_data is None:
        try:
            contract = json.loads(read(TEN_PILLAR_CONTRACT))
        except json.JSONDecodeError as error:
            raise ContractError(f"invalid 10-pillar contract JSON: {error}") from error
    else:
        contract = contract_data

    if contract.get("format") != "onebrain/ten-pillar-architecture/1":
        raise ContractError("unexpected 10-pillar contract format")
    if contract.get("profile_id") != "ARCH-10P-001":
        raise ContractError("unexpected 10-pillar contract profile_id")
    if contract.get("reachability") != "spec-only (CI-checked)":
        raise ContractError("10-pillar contract must declare spec-only (CI-checked) reachability")

    master_spec_rel = contract.get("master_spec")
    if master_spec_rel != "docs/specs/vnext/TEN_PILLAR_ARCHITECTURE_AND_INTERACTION_MATRIX_V1.md":
        raise ContractError("unexpected master_spec path in 10-pillar contract")
    master_text = _read_spec(master_spec_rel, spec_overrides)
    for marker in (
        "ARCH-10P-001",
        "spec-only (CI-checked)",
        "ten-pillar-architecture-v1.json",
    ):
        if marker not in master_text:
            raise ContractError(f"master 10-pillar spec missing marker: {marker}")

    pillars = contract.get("pillars")
    if not isinstance(pillars, list) or [p.get("pillar_id") for p in pillars] != EXPECTED_PILLAR_IDS:
        raise ContractError("10-pillar contract must list P1..P10 in canonical order")

    seen_profiles: set[str] = set()
    seen_specs: set[str] = set()
    pillar_by_id: dict[str, dict[str, Any]] = {}
    rank_by_id: dict[str, int] = {}

    for row in pillars:
        pid = row["pillar_id"]
        profile_id = row.get("profile_id")
        gap_spec = row.get("gap_closing_spec")
        rank = row.get("acyclic_plane_rank")
        confluence = row.get("confluence_class")
        headings = row.get("required_headings")
        core_specs = row.get("core_foundation_specs")
        domains = row.get("fnd001_domains")

        if not isinstance(profile_id, str) or not profile_id or profile_id in seen_profiles:
            raise ContractError(f"invalid or duplicate pillar profile_id for {pid}")
        if not isinstance(gap_spec, str) or not gap_spec.startswith("docs/specs/vnext/") or gap_spec in seen_specs:
            raise ContractError(f"invalid or duplicate gap_closing_spec for {pid}")
        if not isinstance(domains, list) or not domains:
            raise ContractError(f"pillar {pid} must declare non-empty fnd001_domains")
        if not isinstance(core_specs, list) or not core_specs:
            raise ContractError(f"pillar {pid} must declare non-empty core_foundation_specs")
        if not isinstance(headings, list) or len(headings) < 3:
            raise ContractError(f"pillar {pid} must declare at least 3 required_headings")
        if not isinstance(rank, int):
            raise ContractError(f"pillar {pid} acyclic_plane_rank must be an integer")

        if pid == "P5":
            if confluence != "post_commit_non_i_confluent_isolated" or rank != 6:
                raise ContractError(
                    "P5 must be isolated on post_commit_non_i_confluent_isolated at rank 6"
                )
        else:
            if confluence != "validated_union_i_confluent" or not (1 <= rank <= 5):
                raise ContractError(
                    f"{pid} must use validated_union_i_confluent at plane rank 1..5"
                )

        spec_text = _read_spec(gap_spec, spec_overrides)
        if profile_id not in spec_text:
            raise ContractError(f"{gap_spec} missing profile_id {profile_id}")
        if "spec-only (CI-checked)" not in spec_text:
            raise ContractError(f"{gap_spec} must state honest spec-only (CI-checked) reachability")
        for heading in headings:
            if heading not in spec_text:
                raise ContractError(f"{gap_spec} missing required heading: {heading}")
        gap_basename = Path(gap_spec).name
        if gap_basename not in master_text:
            raise ContractError(f"master 10-pillar spec does not link {gap_basename}")

        for core_rel in core_specs:
            _read_spec(core_rel, spec_overrides)
            core_basename = Path(core_rel).name
            if core_basename not in master_text:
                raise ContractError(
                    f"master 10-pillar spec does not reference foundation contract {core_basename} for {pid}"
                )

        seen_profiles.add(profile_id)
        seen_specs.add(gap_spec)
        pillar_by_id[pid] = row
        rank_by_id[pid] = rank

    firewalls = contract.get("firewalls")
    if not isinstance(firewalls, list) or [fw.get("id") for fw in firewalls] != EXPECTED_FIREWALL_IDS:
        raise ContractError("10-pillar contract must declare FW-01..FW-10 in canonical order")

    for fw in firewalls:
        fw_id = fw["id"]
        src_pillars = fw.get("source_pillars")
        dst_pillars = fw.get("target_pillars")
        enforced_specs = fw.get("enforced_in_specs")
        if (
            not isinstance(src_pillars, list)
            or not src_pillars
            or any(p not in pillar_by_id for p in src_pillars)
        ):
            raise ContractError(f"{fw_id} has invalid source_pillars")
        if (
            not isinstance(dst_pillars, list)
            or not dst_pillars
            or any(p not in pillar_by_id for p in dst_pillars)
        ):
            raise ContractError(f"{fw_id} has invalid target_pillars")
        if not isinstance(enforced_specs, list) or master_spec_rel not in enforced_specs or len(enforced_specs) < 2:
            raise ContractError(f"{fw_id} must be enforced in master_spec and >=1 pillar spec")
        if fw_id not in master_text:
            raise ContractError(f"master 10-pillar spec missing firewall {fw_id}")
        pillar_mentions = 0
        for spec_rel in enforced_specs:
            text = _read_spec(spec_rel, spec_overrides)
            if spec_rel != master_spec_rel and fw_id in text:
                pillar_mentions += 1
        if pillar_mentions == 0:
            raise ContractError(f"{fw_id} is not anchored in any enforced pillar spec")

    edges = contract.get("canonical_authority_dag_edges")
    if not isinstance(edges, list) or not edges:
        raise ContractError("canonical_authority_dag_edges must be non-empty")

    adjacency: dict[str, list[str]] = {pid: [] for pid in EXPECTED_PILLAR_IDS}
    indegree: dict[str, int] = {pid: 0 for pid in EXPECTED_PILLAR_IDS}
    seen_pairs: set[tuple[str, str]] = set()

    for edge in edges:
        u = edge.get("from")
        v = edge.get("to")
        rel = edge.get("relation")
        if u not in pillar_by_id or v not in pillar_by_id or not isinstance(rel, str) or not rel:
            raise ContractError(f"invalid authority edge: {edge}")
        if u == v:
            raise ContractError(f"self-loop authority edge forbidden: {u}")
        if (u, v) in seen_pairs:
            raise ContractError(f"duplicate authority edge: {u} -> {v}")
        if u == "P5":
            raise ContractError(f"reverse P5 economic authority edge forbidden: P5 -> {v}")
        if u == "P7" and v in {"P1", "P4", "P8"}:
            raise ContractError(
                f"reverse P7 derived-view authority edge forbidden: P7 -> {v}"
            )
        if rank_by_id[u] > rank_by_id[v]:
            raise ContractError(
                f"acyclic plane rank violation on {u}({rank_by_id[u]}) -> {v}({rank_by_id[v]})"
            )
        seen_pairs.add((u, v))
        adjacency[u].append(v)
        indegree[v] += 1

    queue = [pid for pid in EXPECTED_PILLAR_IDS if indegree[pid] == 0]
    visited = 0
    while queue:
        current = queue.pop(0)
        visited += 1
        for nxt in adjacency[current]:
            indegree[nxt] -= 1
            if indegree[nxt] == 0:
                queue.append(nxt)
    if visited != len(EXPECTED_PILLAR_IDS):
        raise ContractError("authority cycle detected in canonical_authority_dag_edges")

    assertions = contract.get("additive_negative_assertions")
    if not isinstance(assertions, list) or len(assertions) < 10:
        raise ContractError("10-pillar contract must define additive_negative_assertions")
    seen_neg_ids: set[str] = set()
    for item in assertions:
        neg_id = item.get("id")
        pillar = item.get("pillar")
        spec_rel = item.get("spec")
        statement = item.get("statement")
        if (
            not isinstance(neg_id, str)
            or not neg_id.startswith("NEG-")
            or neg_id in seen_neg_ids
        ):
            raise ContractError(f"invalid or duplicate additive negative assertion id: {neg_id}")
        if pillar not in pillar_by_id:
            raise ContractError(f"negative assertion {neg_id} cites unknown pillar {pillar}")
        if spec_rel != pillar_by_id[pillar]["gap_closing_spec"]:
            raise ContractError(
                f"negative assertion {neg_id} must point to {pillar}'s gap_closing_spec"
            )
        if not isinstance(statement, str) or "MUST" not in statement:
            raise ContractError(f"negative assertion {neg_id} must carry a normative MUST clause")
        _read_spec(spec_rel, spec_overrides)
        seen_neg_ids.add(neg_id)

    return (len(pillars), len(firewalls), len(edges), len(assertions))


def main() -> int:
    try:
        pillars, firewalls, edges, assertions = validate_ten_pillar_contracts()
    except ContractError as error:
        print(f"10-pillar contract validation failed: {error}", file=sys.stderr)
        return 1
    print(
        "10-pillar contracts OK: "
        f"{pillars} pillars, {firewalls} firewalls, {edges} acyclic authority edges, "
        f"{assertions} additive negative assertions"
    )
    return 0


if __name__ == "__main__":
    raise SystemExit(main())

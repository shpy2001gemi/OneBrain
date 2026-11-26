from __future__ import annotations

import copy
import json
import unittest

from scripts.ci.validate_ten_pillar_contracts import (
    TEN_PILLAR_CONTRACT,
    validate_ten_pillar_contracts,
)
from scripts.ci.vnext_contract_common import ContractError, ROOT


class TenPillarContractTests(unittest.TestCase):
    def setUp(self) -> None:
        self.contract = json.loads(TEN_PILLAR_CONTRACT.read_text(encoding="utf-8"))

    def test_frozen_ten_pillar_contract_passes(self) -> None:
        self.assertEqual(
            validate_ten_pillar_contracts(self.contract),
            (10, 10, 15, 12),
        )

    def test_missing_pillar_fails_closed(self) -> None:
        mutated = copy.deepcopy(self.contract)
        mutated["pillars"] = [p for p in mutated["pillars"] if p["pillar_id"] != "P9"]
        with self.assertRaisesRegex(ContractError, "P1\\.\\.P10"):
            validate_ten_pillar_contracts(mutated)

    def test_reverse_p5_authority_edge_fails_closed(self) -> None:
        mutated = copy.deepcopy(self.contract)
        mutated["canonical_authority_dag_edges"].append(
            {"from": "P5", "to": "P1", "relation": "illegal_token_gated_truth"}
        )
        with self.assertRaisesRegex(ContractError, "reverse P5 economic authority"):
            validate_ten_pillar_contracts(mutated)

    def test_reverse_p7_derived_authority_edge_fails_closed(self) -> None:
        mutated = copy.deepcopy(self.contract)
        mutated["canonical_authority_dag_edges"].append(
            {"from": "P7", "to": "P1", "relation": "illegal_graph_rewrite"}
        )
        with self.assertRaisesRegex(ContractError, "reverse P7 derived-view authority"):
            validate_ten_pillar_contracts(mutated)

    def test_authority_cycle_at_equal_rank_fails_closed(self) -> None:
        mutated = copy.deepcopy(self.contract)
        mutated["canonical_authority_dag_edges"].append(
            {"from": "P2", "to": "P8", "relation": "equal_rank_cycle_with_p8_to_p2"}
        )
        with self.assertRaisesRegex(ContractError, "authority cycle detected"):
            validate_ten_pillar_contracts(mutated)

    def test_p5_confluence_must_remain_isolated(self) -> None:
        mutated = copy.deepcopy(self.contract)
        for p in mutated["pillars"]:
            if p["pillar_id"] == "P5":
                p["confluence_class"] = "validated_union_i_confluent"
        with self.assertRaisesRegex(ContractError, "post_commit_non_i_confluent_isolated"):
            validate_ten_pillar_contracts(mutated)

    def test_missing_required_heading_in_pillar_spec_fails_closed(self) -> None:
        p9_rel = "docs/specs/vnext/P9_BCI_NEURAL_INTENT_AND_SAFETY_PROFILE_V1.md"
        p9_text = (ROOT / p9_rel).read_text(encoding="utf-8").replace(
            "## 4. `SomaticImmediate/1` Authority & Zero-Latency Revocation (`REV-001` Extension)",
            "## 4. Weakened Revocation Section",
        )
        with self.assertRaisesRegex(ContractError, "missing required heading"):
            validate_ten_pillar_contracts(self.contract, spec_overrides={p9_rel: p9_text})

    def test_dishonest_reachability_claim_fails_closed(self) -> None:
        p6_rel = "docs/specs/vnext/P6_REMOTE_COGNITION_AND_PERSONAL_MEDIATOR_PROFILE_V1.md"
        p6_text = (ROOT / p6_rel).read_text(encoding="utf-8").replace(
            "spec-only (CI-checked)",
            "default-build",
        )
        with self.assertRaisesRegex(ContractError, "honest spec-only"):
            validate_ten_pillar_contracts(self.contract, spec_overrides={p6_rel: p6_text})


if __name__ == "__main__":
    unittest.main()

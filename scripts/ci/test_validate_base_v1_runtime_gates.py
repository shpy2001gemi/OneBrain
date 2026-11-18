from __future__ import annotations

import unittest
from pathlib import Path
from unittest import mock

from scripts.ci import validate_vnext_contracts
from scripts.ci.validate_vnext_contracts import (
    ContractError,
    validate_base_v1_archive,
    validate_base_v1_freeze,
    validate_base_v1_packaging,
)


def mutate_workflow(test: unittest.TestCase, mutate):
    real_read = validate_vnext_contracts.read

    def read_mutated(candidate):
        text = real_read(candidate)
        if candidate == validate_vnext_contracts.VNEXT_FOUNDATION_WORKFLOW:
            mutated = mutate(text)
            test.assertNotEqual(mutated, text)
            return mutated
        return text

    return mock.patch.object(validate_vnext_contracts, "read", side_effect=read_mutated)


class BaseV1RuntimeGateTests(unittest.TestCase):
    """ARCH-13: Base runtime evidence is CI-run Rust tests, not source needles."""

    def test_archive_validator_accepts_the_current_tree(self) -> None:
        validate_base_v1_archive()

    def test_node_library_tests_must_run_in_the_integrated_base_gate(self) -> None:
        command = validate_vnext_contracts.BASE_NODE_LIBRARY_TEST_COMMAND
        with mutate_workflow(self, lambda text: command.sub("true", text)):
            with self.assertRaisesRegex(ContractError, "Base runtime library test gate"):
                validate_base_v1_archive()

    def test_failpoint_phase_test_exists(self) -> None:
        source = (
            validate_vnext_contracts.ROOT / "src/onebrain-node/src/base_operation_store.rs"
        ).read_text(encoding="utf-8")
        self.assertIn("fn tx_base_ops_001_hooks_every_frozen_failpoint_phase", source)


    def test_freeze_and_packaging_validators_accept_the_current_tree(self) -> None:
        validate_base_v1_freeze()
        validate_base_v1_packaging()

    def test_base_contract_library_tests_must_run_in_the_integrated_gate(self) -> None:
        command = validate_vnext_contracts.BASE_CONTRACT_LIBRARY_TEST_COMMAND
        with mutate_workflow(self, lambda text: command.sub("true", text)):
            with self.assertRaisesRegex(ContractError, "candidate version test gate"):
                validate_base_v1_freeze()

    def test_packaging_matrix_must_prove_each_forbidden_combination_fails(self) -> None:
        for check in validate_vnext_contracts.BASE_FORBIDDEN_FEATURE_CHECKS:
            with self.subTest(check=check.pattern):
                with mutate_workflow(self, lambda text, check=check: check.sub("true\n", text)):
                    with self.assertRaisesRegex(ContractError, "forbidden combination"):
                        validate_base_v1_packaging()

    def test_cli_archive_facade_round_trip_test_exists(self) -> None:
        root = validate_vnext_contracts.ROOT
        integration = (root / "src/onebrain-node/tests/base_gate_integration.rs").read_text(
            encoding="utf-8"
        )
        self.assertIn(
            "fn cli_base_archive_file_facade_round_trips_through_scoped_management",
            integration,
        )

    def test_cli_archive_must_route_through_the_tested_facade(self) -> None:
        cli = validate_vnext_contracts.ROOT / "src/onebrain-cli/src/cli/data.rs"
        real_read_text = Path.read_text

        def read_text(path: Path, *args, **kwargs) -> str:
            text = real_read_text(path, *args, **kwargs)
            if path == cli:
                return text.replace("restore_base_archive_file(", "restore_inline(")
            return text

        with mock.patch.object(Path, "read_text", read_text):
            with self.assertRaisesRegex(ContractError, "bypasses the tested facade"):
                validate_base_v1_packaging()

    def test_release_version_tests_exist(self) -> None:
        root = validate_vnext_contracts.ROOT
        contract = (root / "src/onebrain-base-contract/src/compatibility.rs").read_text(
            encoding="utf-8"
        )
        runtime = (root / "src/onebrain-node/src/base_runtime.rs").read_text(encoding="utf-8")
        self.assertIn("fn base_v1_release_version_is_the_frozen_1_2_0_candidate", contract)
        self.assertIn("fn compiled_runtime_tuple_reports_the_base_release_version", runtime)

if __name__ == "__main__":
    unittest.main()

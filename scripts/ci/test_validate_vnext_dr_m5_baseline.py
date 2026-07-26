from __future__ import annotations

import copy
import json
import unittest

from scripts.ci.validate_vnext_contracts import (
    ContractError,
    DR_M5_BASELINE_PROFILE,
    validate_vnext_dr_m5_baseline,
)


def frozen_profile() -> dict[str, object]:
    return json.loads(DR_M5_BASELINE_PROFILE.read_text(encoding="utf-8"))


class VNextDrM5BaselineTests(unittest.TestCase):
    def test_frozen_profile_is_accepted(self) -> None:
        self.assertEqual(validate_vnext_dr_m5_baseline(frozen_profile()), (13, 11))

    def test_runtime_change_cannot_escape_real_quic_gate(self) -> None:
        profile = copy.deepcopy(frozen_profile())
        profile["runtime_change_globs"] = ["src/ku-net/**"]
        with self.assertRaisesRegex(ContractError, "runtime path"):
            validate_vnext_dr_m5_baseline(profile)

    def test_real_quic_gate_cannot_be_unbounded(self) -> None:
        profile = copy.deepcopy(frozen_profile())
        profile["real_quic_gate"]["timeout_minutes"] = 0
        with self.assertRaisesRegex(ContractError, "real-QUIC"):
            validate_vnext_dr_m5_baseline(profile)

    def test_transaction_boundary_cannot_disappear(self) -> None:
        profile = copy.deepcopy(frozen_profile())
        profile["transaction_boundaries"].pop()
        with self.assertRaisesRegex(ContractError, "boundary ID"):
            validate_vnext_dr_m5_baseline(profile)

    def test_oracle_field_cannot_disappear(self) -> None:
        profile = copy.deepcopy(frozen_profile())
        profile["invariant_oracle"]["fields"].pop()
        with self.assertRaisesRegex(ContractError, "invariant oracle"):
            validate_vnext_dr_m5_baseline(profile)

    def test_frozen_oracle_digest_cannot_drift(self) -> None:
        profile = copy.deepcopy(frozen_profile())
        profile["empty_oracle_specimen"]["sha256"] = "00" * 32
        with self.assertRaisesRegex(ContractError, "digest"):
            validate_vnext_dr_m5_baseline(profile)

    def test_failpoint_phase_cannot_disappear(self) -> None:
        profile = copy.deepcopy(frozen_profile())
        profile["failpoint_phases"].pop()
        with self.assertRaisesRegex(ContractError, "failpoint phase"):
            validate_vnext_dr_m5_baseline(profile)


if __name__ == "__main__":
    unittest.main()

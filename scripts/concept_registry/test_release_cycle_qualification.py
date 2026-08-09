"""Tests for the complete signed Concept Registry release-cycle harness."""

from __future__ import annotations

import inspect
import json
import sys
import tempfile
import unittest
from pathlib import Path

import blake3

SCRIPT_DIR = Path(__file__).resolve().parent
if str(SCRIPT_DIR) not in sys.path:
    sys.path.insert(0, str(SCRIPT_DIR))

from release_cycle_qualification import CycleError, _latest_state, run_release_cycle


class ReleaseCycleQualificationTests(unittest.TestCase):
    def test_latest_state_rejects_boolean_generation_before_release_lookup(self) -> None:
        with tempfile.TemporaryDirectory() as temporary:
            registry_root = Path(temporary)
            state_dir = registry_root / "state"
            state_dir.mkdir()
            state_view = {
                "profile": "onebrain/concept-registry-release-state/1",
                "generation": True,
                "active_release": "candidate-v2",
                "previous_release": None,
            }
            state = {
                **state_view,
                "state_root": blake3.blake3(
                    b"onebrain:concept-registry-state:1\0"
                    + json.dumps(
                        state_view, ensure_ascii=False, separators=(",", ":")
                    ).encode("utf-8")
                ).hexdigest(),
            }
            (state_dir / "state-00000000000000000001.json").write_text(
                json.dumps(state), encoding="utf-8"
            )
            with self.assertRaisesRegex(CycleError, "not append-only"):
                _latest_state(registry_root)

    def test_release_cycle_api_cannot_accept_caller_step_plan_or_commands(self) -> None:
        parameters = inspect.signature(run_release_cycle).parameters
        self.assertNotIn("plan", parameters)
        self.assertNotIn("commands", parameters)
        self.assertNotIn("candidate_root", parameters)
        source = (SCRIPT_DIR / "release_cycle_qualification.py").read_text(encoding="utf-8")
        self.assertIn("subprocess.Popen", source)
        self.assertIn("_verify_cycle_candidate", source)


if __name__ == "__main__":
    unittest.main()

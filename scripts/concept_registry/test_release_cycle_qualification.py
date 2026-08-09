"""Tests for the complete signed Concept Registry release-cycle harness."""

from __future__ import annotations

import inspect
import sys
import unittest
from pathlib import Path

SCRIPT_DIR = Path(__file__).resolve().parent
if str(SCRIPT_DIR) not in sys.path:
    sys.path.insert(0, str(SCRIPT_DIR))

from release_cycle_qualification import run_release_cycle


class ReleaseCycleQualificationTests(unittest.TestCase):
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

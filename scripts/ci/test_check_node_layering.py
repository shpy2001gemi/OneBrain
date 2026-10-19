"""Unit tests for the ARCH-09 layering ratchet (no cargo invocation)."""
from __future__ import annotations

import unittest

try:
    from scripts.ci.check_node_layering import (
        KNOWN_BYPASSES,
        NODE_LIB,
        SURFACES,
        evaluate,
        ungated_harness_modules,
    )
except ImportError:  # pragma: no cover - direct script-directory execution
    from check_node_layering import (
        KNOWN_BYPASSES,
        NODE_LIB,
        SURFACES,
        evaluate,
        ungated_harness_modules,
    )


def current_edges() -> dict[str, set[str]]:
    return {s: set(SURFACES[s]) & {"onebrain-node"} | KNOWN_BYPASSES.get(s, set()) for s in SURFACES}


class LayeringRatchetTests(unittest.TestCase):
    def test_current_known_state_passes(self) -> None:
        self.assertEqual(evaluate({"onebrain-node", "ku-core", "tokio"}, current_edges()), [])

    def test_node_depending_on_host_shell_fails(self) -> None:
        failures = evaluate({"onebrain-node", "axum"}, current_edges())
        self.assertEqual(len(failures), 1)
        self.assertIn("axum", failures[0])

    def test_new_surface_bypass_fails(self) -> None:
        edges = current_edges()
        edges["onebrain-api"].add("ku-net")
        failures = evaluate({"onebrain-node"}, edges)
        self.assertEqual(len(failures), 1)
        self.assertIn("ku-net", failures[0])

    def test_removed_bypass_requires_tightening_the_list(self) -> None:
        edges = current_edges()
        edges["onebrain-cli"].discard("ku-core")
        failures = evaluate({"onebrain-node"}, edges)
        self.assertEqual(len(failures), 1)
        self.assertIn("KNOWN_BYPASSES", failures[0])

    def test_harness_modules_must_be_feature_gated(self) -> None:
        lib = (
            '#[cfg(feature = "vnext-sim-harness")]\r\n'
            "pub mod vnext_m5_benchmark;\r\n"
            "pub mod vnext_m6_model;\r\n"
            "#[cfg(test)]\n"
            "pub mod vnext_scale_simulation;\n"
            "pub mod vnext_status;\n"
        )
        self.assertEqual(ungated_harness_modules(lib), ["vnext_m6_model"])

    def test_repository_lib_keeps_harness_out_of_default_core(self) -> None:
        self.assertEqual(ungated_harness_modules(NODE_LIB.read_text(encoding="utf-8")), [])


if __name__ == "__main__":
    unittest.main()

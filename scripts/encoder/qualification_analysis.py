"""Metadata-only operational reduction. No semantic grader or qualification.

The independently reviewed label/rubric/FID lane is intentionally not replaced
by encoder status. Missing and failed repeats stay in the planned denominator.
"""
from collections import Counter, defaultdict
import json
import math

from scripts.encoder.qualification_attempt_ledger import PROFILE, TERMINALS, encode, plan_rows
from scripts.encoder.qualification_package_audit import require, sha256


def nearest_rank(values, percent):
    if not values:
        return None
    return sorted(values)[max(0, math.ceil(len(values) * percent / 100) - 1)]


def reduce_operations(plan, snapshot):
    """Full planned denominators; timings are recorded reports, not measurements here."""
    expected = dict(plan_rows(plan))
    require(snapshot.get("kind") == "ku-enc-003-attempt-ledger-snapshot/1" and
            snapshot.get("plan_sha256") == sha256(encode(plan).encode()), "analysis_plan_mismatch")
    require(snapshot.get("planned_rows") == len(expected), "analysis_denominator_mismatch")
    actual = snapshot.get("rows")
    require(isinstance(actual, list) and len(actual) == len(expected), "analysis_rows_missing")
    seen = set()
    groups = defaultdict(list)
    for row in actual:
        key = row["id"]
        require(key in expected and key not in seen, "analysis_duplicate_or_unknown_row")
        binding = expected[key]
        require(row["binding"] == encode(binding), "analysis_row_binding_mismatch")
        require(row["state"] in TERMINALS | {"pending", "active"}, "analysis_unknown_state")
        for field in ("calls", "input_tokens", "output_tokens", "work"):
            require(type(row[field]) is int and row[field] >= 0, "analysis_counter_invalid")
        require(row["elapsed_ms"] is None or (type(row["elapsed_ms"]) is int and row["elapsed_ms"] >= 0),
                "analysis_elapsed_invalid")
        seen.add(key)
        groups[(binding["tuple_sha256"], binding["language"])].append((binding, row))
    result = []
    for (tuple_id, language), rows in sorted(groups.items()):
        sources = defaultdict(list)
        for binding, row in rows:
            sources[binding["source_id_sha256"]].append(row)
        outcomes = Counter(row["state"] for _, row in rows)
        successful_times = [row["elapsed_ms"] for _, row in rows
                            if row["state"] == "reported_complete" and row["elapsed_ms"] is not None]
        finished_times = [row["elapsed_ms"] for _, row in rows
                          if row["state"] in TERMINALS and row["elapsed_ms"] is not None]
        result.append({
            "tuple_sha256": tuple_id, "language": language,
            "source_rows": len(sources), "independent_source_count": None,
            "planned_repeat_rows": len(rows), "outcomes": dict(sorted(outcomes.items())),
            "sources_all_repeats_reported_complete": sum(all(r["state"] == "reported_complete" for r in rs)
                                                         for rs in sources.values()),
            "sources_with_failure_or_unfinished_repeat": sum(any(r["state"] != "reported_complete" for r in rs)
                                                             for rs in sources.values()),
            "charged": {field: sum(row[field] for _, row in rows)
                        for field in ("calls", "input_tokens", "output_tokens", "work")},
            "reported_complete_latency_ms": {"n": len(successful_times),
                "p50": nearest_rank(successful_times, 50), "p95": nearest_rank(successful_times, 95)},
            "all_terminal_latency_ms": {"n": len(finished_times),
                "p50": nearest_rank(finished_times, 50), "p95": nearest_rank(finished_times, 95)},
            "missing_terminal_latency_rows": sum(r["state"] in TERMINALS and r["elapsed_ms"] is None
                                                  for _, r in rows),
            "semantic_precision": None, "semantic_recall": None,
            "semantic_agreement": None, "raw_cid_agreement": None,
            "quality_gate_evaluated": False, "latency_gate_evaluated": False,
        })
    return {"kind": "ku-enc-003-operational-reduction/1", "model_qualified": False,
            "execution_admitted": False, "plan_sha256": snapshot["plan_sha256"],
            "thresholds": PROFILE["qualification"], "groups": result,
            "limits": ["Encoder completion is not independently acceptable canonical KU",
                       "Source rows are not verified independent provenance groups",
                       "Latencies require collector evidence and declared cold/warm conditions",
                       "Independent labels, alternatives, FID and source-level analysis remain required"]}

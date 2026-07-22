#!/usr/bin/env python3
"""Dependency-free structural checks for OneBrain vNext contracts."""

from __future__ import annotations

import json
import re
import sys
from pathlib import Path


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
    if len(domains) != 20 or len(set(domains)) != 20:
        raise ContractError("foundation vectors must cover 20 unique reserved domains")

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


def main() -> int:
    try:
        tasks, _ = plan_tasks()
        adrs = validate_traceability(tasks)
        assertions = validate_negative_assertions()
        vector_count, domains, schema_vectors, event_vectors = validate_vectors()
        links = validate_markdown_links()
        normative_lines = validate_normative_coverage()
    except ContractError as error:
        print(f"vNext contract validation failed: {error}", file=sys.stderr)
        return 1

    print(
        "vNext contracts OK: "
        f"{len(tasks)} tasks, {adrs} ADRs, {assertions} negative assertions, "
        f"{vector_count} foundation vectors/{domains} domains, "
        f"{schema_vectors} identity-object vectors, "
        f"{event_vectors} feed-event vectors, {normative_lines} normative lines, "
        f"{links} local links"
    )
    return 0


if __name__ == "__main__":
    raise SystemExit(main())

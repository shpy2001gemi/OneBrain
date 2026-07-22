# M5 Multi-Objective Benchmark Profile v1

> **Task:** `QA-002`  
> **Status:** Complete  
> **Depends on:** `KQL-009`, `KQL-010`, `KQL-011`, `KQL-012`, `OBKG-001`, `AI-002`, `AI-003`, `AI-004`

## 1. Purpose

This profile defines the reproducible M5 evaluation harness for gap filling,
Assembly utility, hard constraints, long-tail exposure, privacy, Companion
consent and model ablation.

It deliberately rejects a single weighted “OneBrain score.” Retrieval quality
must never compensate for a hard violation, privacy leak, exploration starvation
or unauthorized side effect.

## 2. Metric vector

Each `M5BenchmarkReport` exposes independently:

| Dimension | Numerator / evidence | Denominator / failure set |
|---|---|---|
| GapFillRecall | expected fixture fragments discovered | all expected fixture fragments; missed full IDs retained |
| UsefulAssemblyPrecision | selected proposals useful for that fixture task | all selected proposals |
| Hard constraint safety | — | selected proposal IDs carrying required hard violations |
| Long-tail exposure | eligible long-tail candidates presented | all eligible long-tail candidates; starved IDs retained |
| Privacy leakage | forbidden patterns found | leaking probe IDs and match count |
| Companion consent | — | side-effect attempts made before the required ready gate |
| Model validity | symbolic disposition per MappingKernelCID | common on/off Mapping drift list |

“Useful for fixture task” is a bounded benchmark oracle, not truth, global
quality, benefit evidence or OBT value.

Fractions use exact rational arithmetic. An empty denominator remains undefined
and fails a positive threshold instead of being reported as perfect.

## 3. Reproducibility

Every case and variant uses full-width commitments. Inputs are canonicalized by
case ID; exact set members are sorted/deduplicated and conflicting duplicate
cases fail closed. The report root binds:

- variant commitment and model-enabled flag;
- every metric fraction;
- missed/starved/leaking/violating IDs;
- hard-violation IDs; and
- Mapping validity vector.

Reordering the same corpus produces the same complete report and root.

The root is a commitment to the report vector, not a claim that the corpus is
complete or representative of all OneBrain partitions.

## 4. Baseline and ablation

`M5BenchmarkRunner::compare` reports baseline and ablation values side by side:

- GapFillRecall;
- UsefulAssemblyPrecision;
- long-tail exposure;
- number of common MappingKernelCIDs; and
- exact common Mapping validity drift IDs.

Model-off may lower recall or change candidate ordering. A common Mapping must
retain the same symbolic disposition, matching the AI-002 firewall invariant.
The comparison contains no weighted aggregate score.

## 5. Gate vector

Thresholds are separate exact ratios for recall, useful precision and long-tail
exposure. Safety gates are separate booleans:

- no selected required hard violation;
- no privacy leakage; and
- no Companion consent-boundary violation.

Consumers must display/report this vector. They must not collapse failures into
an average that allows a high retrieval metric to hide an unsafe dimension.

## 6. Privacy and consent probes

Privacy probes scan serialized route/disclosure output for explicitly supplied
forbidden byte patterns such as private Need fragments, raw observation bytes
or stable IDs. This is a deterministic regression detector, not a proof against
all inference or traffic analysis.

Consent cases distinguish local reads, network send, share/publish and
materialization. A side-effect attempt before `ReadyForExplicitExecutor` is a
violation; a recommendation with no attempt is not. The Companion itself should
therefore record zero attempts.

## 7. Boundaries

The benchmark does not:

- define truth or globally useful knowledge;
- make fixture labels network authority;
- prove anonymity from zero byte-pattern matches;
- treat non-presentation as non-use;
- allow recall/precision to offset safety failure;
- convert results into PoMV or OBT rewards; or
- claim global completeness.

## 8. Executable evidence

Five tests prove:

- a clean run reports exact independent metrics and a full passing gate vector;
- model-off may reduce recall while common Mapping validity stays fixed;
- hard violation, privacy leakage and consent violation fail independently even
  when recall/precision remain high;
- high useful precision cannot hide long-tail starvation; and
- source/case reordering reproduces the identical report root.

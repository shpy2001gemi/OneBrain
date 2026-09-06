# KU-ENC-003 qualification checkpoint

Date: 2026-09-06. Branch: `codex/ku-enc-003-model-qualification`.
Starting main: `ca1d4c22f61864a5bde53edf5c399e6ffdbc1943`, clean and equal to
freshly fetched `origin/main` before branch creation.

**Qualification is blocked, not completed. No model/profile is qualified.**
The owner has no identified holdout/evaluator and requested source examples and
evaluation instructions. The [Vietnamese data guide](KU_ENC_003_DATA_GUIDE.vi.md)
answers that request. Its illustrative examples are development material and
must never be relabeled blind evidence.

## Delivered and measured

- [Read-only preflight runner](../../../../scripts/encoder/qualification_preflight.py)
  checks every frozen bundle artifact against its recorded SHA-256, snapshots
  existing thresholds/resource profiles and binds native extraction/compiler,
  node bridge, adapter, lockfile and runner bytes.
- Explicit local Ollama library tags are resolved to manifests; every config/
  weight/projector/template/license/parameter layer present is streamed through
  SHA-256 and checked against its declared size. No downloads occur. This is
  artifact integrity against a local manifest, not publisher authenticity,
  license approval, activation approval or proof of what a server loaded.
- Only literal-loopback `/api/version` and `/api/show` metadata are queried,
  with proxies/redirects disabled and bounded responses. Detailed local output
  records model metadata, template hashes, backend-reported version and client
  executable hash. A client hash does not identify the actual worker build.
- The run inspected existing Qwen and Gemma artifacts successfully, with five
  verified local layers per selected tag. Ollama reported version 0.33.3.
  This establishes availability of two candidate families, **not execution**.
- Detailed inventory is stored outside Git at
  `C:/Users/shpy2/.codex/ku-enc-003-evidence/preflight-20260906.json`.
  It contains private exact model/build/device bindings and is not public
  capability telemetry. The script refuses to overwrite evidence or save it
  in the repository. Later runs need a new output filename.
- **Inference calls: 0. Holdout runs: 0. Qualified tuples: 0.** No raw private
  source is read or sent by this inventory. Local metadata inventory is not
  full inference privacy, worker-stop or OOM evidence.

## Missing gates and evidence boundaries

The entry requirement in
[framework §7](../../../specs/vnext/KU_EXTRACTION_FRAMEWORK_PROFILE_V1.md#7-conformance-and-model-qualification)
is: “KU-ENC-003 must lock a blind holdout before any model run”.
The public 48-case mapping corpus and guide examples cannot satisfy this.
The owner explicitly reported that they do not yet know the source dataset and
asked for examples and how to evaluate it. No new threshold or authority rule
has been inferred to bypass that missing prerequisite.

| Task acceptance item | Checkpoint status |
|---|---|
| Complete reproducible qualification runner/report | Partial: artifact preflight only; not an inference, grading or qualification-publishing runner. |
| Source split/labels/evaluator independence | Missing; guide and collection template delivered. No holdout commitment or blind transcript fabricated. |
| Two model families, repetitions, raw-text agreement | Two families have verified local artifacts; no inference or agreement numbers. |
| No-LLM baseline | Existing finite rule remains available; native regression is conformance only, not a measured holdout/resource baseline. |
| vi/en quality, coverage, abstention, confidence intervals | Unmeasured; no gold labels or independent assessments substituted. |
| Same-SEM bytes/CIDs and alternatives | Existing native compiler regression exercised; no new target qualification or measured raw-text CID claim. |
| Real source custody and Registry root | No qualification run authority installed. Corpus synthetic metadata cannot authorize production. |
| Tokenizer, chat wrapper and schema overhead | Metadata pinned, executable exact tokenizer/accounting unavailable in this harness. Embedded model metadata alone is insufficient. |
| Backend schema capability probes | Unmeasured; supported keywords cannot be asserted from `/show`. |
| Cold/warm latency, tokens, stages, peak RAM/KV | Unmeasured; metadata/blob byte sizes are not latency or peak memory. |
| Cancellation, OOM, process death | Actual inference worker behavior unqualified; shared server is not terminated by this inventory. Managed worker may be required. |
| Constrained-memory host | Unqualified; no OS-enforced worker envelope or measured constrained host run. |
| Legacy tool/v2 ablations | Pending isolated read-only harness; no legacy writer/tool path invoked. |
| Physical mobile | Unqualified; MOB-06/MOB-07 ownership unchanged, no mobile files or evidence modified. |
| Qualification manifests | None issued. Failed/missing evidence must not become a passing tuple. |

Thresholds are copied unchanged from the accepted machine profile. This snapshot
is **not** a complete preregistration: an immutable split, reviewed acceptable
interpretations, evaluator/FID bindings, tuple/configuration and statistical
analysis plan still need to be locked together before inference. No truth,
majority winner, cognitive independence or delegated/reward claim is made.

The current node surface still uses one whole bounded source and abstains on
units without authenticated metadata and ambiguous selections without host
review authority. Unsupported/omitted features remain in complete-source
denominators. This task does not fix or expand that accepted implementation.

## Reproduce this checkpoint

From repository root, with the same artifacts already installed locally:

```text
python -m scripts.encoder.qualification_preflight --model qwen3:1.7b --model gemma4:12b --output C:/Users/shpy2/.codex/ku-enc-003-evidence/preflight-NEW.json
python -m unittest scripts.encoder.test_qualification_preflight scripts.encoder.test_contract
python -m scripts.encoder.generate_bundle --check
python scripts/ci/validate_vnext_contracts.py
```

The inventory never offers pull/generate/chat routes. A successful command exit
means inventory collection succeeded; the report always says unqualified.
Missing/tampered artifacts fail before a successful report is written. This
does not close TOCTOU for a future run: reverify artifacts and actual server
model bindings at that run's admission boundary.

Validation at this checkpoint:

- 27 Python tests pass: nine new inventory integrity/route-denial cases and
  18 existing encoder contract cases. Synthetic test blobs are explicitly not
  real weights or model evidence.
- Eight generated bundle artifacts unchanged; global vNext validator passes.
- `cargo test --locked -q -p ku-encoder --lib extraction::` from `src`:
  16 pass. Diff and local file-link checks pass for the five changed handoff
  documents. Native regressions are not measured model/resource qualification.

## Resume

Continue this same branch. First obtain independent source/label custody and
reviewer provenance using the guide. Finish the locked run/analysis plan and
an authenticated host harness, exact tokenizer, backend build binding and
worker lifecycle/resource controls. Then run real model and no-LLM lanes,
conformance/adversarial probes, repeated holdout, isolated ablations and fault
drills. Record every failure and publish only measured passing tuples.

Keep the task pointer on KU-ENC-003. Do not mark Review/Merged, advance to
KU-API-001, change rollout or merge/delete this branch without the applicable
owner instruction. No new authority approval is needed for unchanged
KU-ENC-001/002 contracts; the blocker is missing evaluation inputs/evidence.

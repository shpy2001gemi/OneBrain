# KU-ENC-003 input and harness preparation — 2026-09-27

This is a source-inspection and harness integration map, not an executable
qualification runner, locked run package or measured model result. Task state
remains in [PROGRESS](../PROGRESS.md): Blocked, `model_qualified=false`.

Later [offline preparation](KU_ENC_003_OFFLINE_PREPARATION.md) completed the
required reading, implemented a preparation-only audit with synthetic tests
and verified the recovered Registry natively. The full execution/grading
harness and final lock remain incomplete; use that report for current next work.

Subsequent [autonomous input recovery](KU_ENC_003_INPUT_RECOVERY.md) found both
workbooks and local Registry/host artifacts. The missing-location requests in
this historical preparation map are superseded. Private intake commitments and
an unlocked run draft exist; all 200 reviewer fields are blank. Continue the
agent-owned technical work from the recovery report.

## Actual checkpoint

Work continued in the original tree on `codex/ku-enc-003-handoff`, preserving
the five modified tracked handoff files and untracked resume map found at entry.
HEAD, local main and origin/main resolve to
`689efd546e7c4f8e41c9f9f9ff3243b63c979908`. These are local ref observations;
the preceding handoff's remote verification was not repeated in this turn.
D-043 merge `6449446` is an ancestor of HEAD. The retained Desktop branch is
`bf3edb1ef247a74fc9fc8ed6b85c6516e213861b`; qualification remains
`4a8f29dde9803355f6251d6f266f40e3aa43c958`.

The four historical files in the [resume map](KU_ENC_003_RESUME.md) were read
with `git show`. No historical ledger or runner was restored over the current
tree. The historical private inventory still exists; its contents, installed
weights, worker binaries and host service were not inspected or revalidated.
No private corpus was opened. No model/worker, holdout, remote host, tuning,
mobile, Git publication or branch-lifecycle action was performed.

## Current-candidate compatibility map

The September 6 missing-input list is historical. Some runtime mechanisms now
exist, but their presence is not qualification evidence for this candidate.

| Component inspected | Current behavior | Qualification harness work still required |
|---|---|---|
| Historical `qualification_preflight.py` at `4a8f29d` | Hashes bundle, selected native sources and local model layers; queries only loopback metadata; always unqualified. | Preserve as historical inventory. It has no locked-package admission, inference, independent grading, resource collector or qualification publisher. Its file selection is not a complete current build/host manifest. |
| [Shared provider](../../../../src/ku-encoder/src/extraction/provider.rs) | `ExtractionProvider` plus exact `ExtractionTokenizer`; bounded Candidate transport and separate draft-task methods. | Supply and verify each concrete tokenizer/backend tuple. A manifest field alone cannot prove actual loaded artifacts or worker termination. |
| [Managed provider](../../../../src/ku-encoder/src/extraction/managed_ollama.rs) | Windows-only `qwen3:*`; verifies/locks model layers, executable and selected CPU runtime files; 8–32 GiB admission, raw prompt, actual prompt-token equality check. | Rehash installed artifacts at future admission, bind supply-chain provenance and actual loaded worker/build. No second admitted model family exists in this adapter. Do not widen it by substituting tags. |
| [Qwen tokenizer](../../../../src/ku-encoder/src/extraction/qwen_tokenizer.rs) | Builds tokenizer from verified GGUF metadata; requires Qwen3 / gpt2 / qwen2 preprocessing and no BOS. | Execute exact tokenizer/chat-wrapper/schema overhead checks for the locked tuple after entry gates. This implementation cannot stand in for Gemma tokenization. |
| [Worker](../../../../src/ku-encoder/src/extraction/ollama_worker.rs) | Own Windows Job Object, process/job memory caps and kill-on-close; non-Windows fails unavailable. Drop polls for up to two seconds. | Observe actual inference descendants stopping, cancellation latency, late callbacks, OOM/process death and peak worker plus host memory. Drop code or a server-only shutdown probe is not measured in-flight cancellation. |
| Managed provider `generate` | Starts an owned worker for each call; `keep_alive=0`; kills it when the call ends. Reserves 4 GiB for host residency and passes the remainder to the worker. | Declare cache/load conditions explicitly. A repeated call is not automatically warm resident-model evidence. Report an unavailable warm lane honestly; measure host memory instead of treating the reservation as a measurement. |
| [Workflow](../../../../src/ku-encoder/src/extraction/workflow.rs) | `new` uses fixed no-LLM/constrained/standard ceilings. `new_experimental_ollama` selects the 600-second experimental override and enters the implementation commitment. | Use the intended fixed profile in an isolated qualification host and bind the chosen constructor/policy. Do not relabel experimental timing as standard/constrained qualification. |
| [Node bridge](../../../../src/onebrain-node/src/ku_extraction.rs) | `SharedKuExtractionInputs`, principal/source custody and signed Registry lease; one whole source, at most 8,192 characters, unique complete Registry matches. | Install actual consent/custody, Registry and private journal. Preserve unsupported unit/review/multi-chunk limits in the complete-source denominator. Do not promote synthetic fixture authority. |
| [Experimental host inputs](../../../../src/onebrain-node/src/ku_ollama.rs) | `OllamaKuInputs::new` uses `new_experimental_ollama` even though the node input profile is named `standard`; also exposes review-draft providers. | Bind the actual execution policy, not just the profile label. Leave the running user host alone; assemble qualification separately after inputs are locked. |
| [Development chat probe](../../../../src/ku-encoder/examples/review_chat_probe.rs) | DraftJob transport; reserves 6,144 input tokens without an exact preflight tokenizer; reports `canonical_ku=false`. | Cannot become a second-family qualification runner merely by supplying a holdout path. Requires exact tokenizer, authenticated host admission, worker controls and independent grading. |
| [Development extraction probe](../../../../src/ku-encoder/examples/extraction_probe.rs) | Explicit no-Registry development authority and private traces. | Not an authenticated source/Registry qualification host. Keep public development results separate from blind evaluation. |

Private review/selection drafts and canonical Candidate/SEM preparation are
different outputs. The run package must name which one is measured. A draft
`ready`/`draft_extracted` result cannot count as complete validated KU; pending
canonical lowering and independent verification cannot be supplied by a report
label. Keep D-024's deferred quality work deferred; no semantic tuning is part
of this preparation.

## Input request — metadata only

The owner-reported 100 VI + 100 EN workbooks still have no supplied local
references or verified custody/reviewer/lock evidence in this continuation.
The owner was asked for those references, without source text or labels in chat.
The following is a concrete intake checklist, not a new protocol/schema or
an assertion that a blank package is admissible.

| Required input | What to supply privately | Verification before inference |
|---|---|---|
| Corpus location and custody | Local workbook/export paths or metadata-only manifest; custodian; allowed local reader/purpose; provenance and processing permission. | Custodian checks exact source bytes, export/transformation record and access scope; developer chat receives metadata only. Keep labels in a separate grading area. |
| Split and independence | Source/group identifiers, language/feature counts, development/holdout split commitment and exposure history. | Group near duplicates, translations and template variants; no development examples in holdout. At least 100 independent sources per language, three repeats and ten sources per claimed feature, plus unsupported/ambiguous/injection/multi-chunk sets. |
| Independent reviewer package | Reviewer identities/roles and language competence, label/rubric/acceptable-alternative commitments, provenance and independence evidence; applicable FID policy and evaluator plan. | Labels fixed before candidate outputs; no model self-grading or different model names treated as group evidence. Review exposure history and evidence categories. |
| Source and Registry authority | Admissible SourceArtifact custody/grants/revocation bindings; exact signed Registry generation and trust configuration; declared feature scope. | Validate actual source and Registry authority in the host; missing unit metadata or review authority stays unresolved. Do not paste keys into chat. |
| Model and device tuples | Two real families' local artifacts and exact tokenizer/template/quantization/runtime/build references, provenance/license decision; OS/device/memory envelope and no-LLM/constrained lanes. | Locally verify bytes and actual worker identity. Reuse no September 6 artifact/version claim as current evidence. No downloads or automatic off-device fallback. |
| Immutable run and analysis package | Exact candidate/build/harness, all above commitments, original threshold bundle, inference options, order/seeds/repeats, cold/warm definitions, failure policy, source-level confidence-interval method and reviewer acknowledgement of the package digest. | Lock together before any model run, including schema probes, warm-up or ablations. Refuse missing, mismatched or changed bindings; retain old package and attempts. A prepared draft is not a lock. |

Before-run reviewer evidence establishes custody, provenance, independence,
preregistered labels and the applicable FID plan. Actual publisher/external
attempts and signed post-run attestations are later outputs, not artifacts that
can be fabricated before inference. FID-002 must enforce commit-before-reveal;
FID-001's default requires publisher plus at least two external blind attempts
and two evidenced administrative-principal + pipeline/model-lineage groups.
FID-003 assessment stays relative to policy/source/candidate/frontier. Two
spreadsheet reviewers alone do not close those contracts.

## Harness assembly sequence

### Admission test matrix prepared in the follow-up continuation

The following cases are a preparation checklist derived from framework sections
2, 4, 5, 6 and 7 and task 23. They are not implemented tests, a new contract or
passing evidence. Exercise them with public synthetic metadata and a provider
spy before introducing private inputs. A successful synthetic admission proves
only the plumbing; it must never issue a qualification manifest.

| Scenario | Required observable outcome in the future harness |
|---|---|
| Missing source/split/label/rubric/evaluator commitment or reviewer acknowledgement | Reject admission before model load or dispatch; identify the missing binding without source text or labels in diagnostics. |
| Workbook supplied but custody, processing permission or exposure history absent | Keep input unverified; do not infer independence from row count, language or filename. |
| A development source, translation or near-duplicate group crosses the holdout boundary | Reject the proposed split; retain the exposure record and require an untouched admissible split. |
| Source, labels, thresholds, Registry, model, tokenizer, template, harness or analysis bytes differ from the locked package | Reject the stale package; never silently regenerate commitments and continue under the old reviewer acknowledgement. |
| Inference requested for a schema probe, warm-up or legacy ablation before the package is locked | Zero provider calls; the same entry gate applies to these model calls. |
| Experimental host reports profile `standard` while selecting the 600-second constructor | Do not admit it as the fixed 120-second standard qualification lane; bind the actual constructor and execution policy. |
| Model tag resolves but exact tokenizer or actual worker build cannot be verified | Leave that tuple unqualified; neither metadata inventory nor a client binary hash substitutes for these bindings. |
| Source grant revoked, Registry trust fails, or authenticated unit/review authority is unavailable | No unauthorized preparation; record the unresolved/failure outcome against the complete source rather than dropping it. |
| Timeout, cancellation, OOM, process death or late callback occurs | Preserve the source/repeat row and charged budgets; no late success or silent automatic resampling. Actual worker-stop/resource evidence still requires a later admitted run. |
| One source has three repeats and one fails | Keep three attempt outcomes but one independent source for source-level statistics; report complete denominators and separate successful-intersection diagnostics. |
| A private draft is ready, or multiple valid interpretations produce different CIDs | Do not count draft readiness as validated canonical KU or choose a majority truth; retain alternatives and separate semantic from raw-CID agreement. |
| Repeated managed calls each start and terminate a worker | Record actual load/cache conditions; do not label repetition as resident-model warm evidence. |

Before implementing this matrix, finish the full task-23 required read set,
including corpus/schema and authority profiles. This follow-up reread the
handoff, historical reports/runner/tests, framework and machine thresholds;
it did not implement a qualification runner or claim that complete read gate.

### Minimal metadata reply for the custodian

The immediate external dependency can be resolved incrementally. Supply only
references and reading scope in chat; keep workbook contents, expected meanings,
labels and private evidence inside the custodian/evaluator storage:

```text
VI workbook/export reference:
EN workbook/export reference:
Custodian and permitted metadata reader/purpose:
Split/grouping/exposure-history record reference:
Independent reviewer/provenance and label/rubric commitment reference:
Source grant and signed Registry reference:
Existing run-package reference and reviewer acknowledgement (or: not yet made):
```

An absent final run package need not block offline harness development, but it
blocks every qualification model call. The engineer must first finish the
harness/build, tuple inventory and analysis bindings; the custodian/reviewer
then locks and acknowledges that exact package. Do not ask the owner to supply
measurements or post-run attestations before a run exists. A path alone does not
authorize printing its contents into development prompts.

At this follow-up checkpoint no new private references have been supplied.
The original tree still has HEAD/main/origin-main local refs at `689efd5`,
Desktop at `bf3edb1` and qualification at `4a8f29d`; remote refs were not queried.
The seven incoming modified/new handoff files were preserved. No private
inventory or holdout was opened, and no worker or inference was started.
Fresh follow-up checks passed: aggregate vNext validator, tracked whitespace,
198 local file links across seven changed/new Markdown files, and unchanged
tracked `src/scripts/docs/specs`. The encoder unit suite and generated-bundle
command in the earlier checkpoint below were not separately rerun here.

### Execution sequence after intake

1. Complete task 23's required corpus/schema and authority read set before
   implementation. This turn inspected the framework/profile, KU-ENC-001/002
   reports, research evaluation design, CAP-001/CAP-002, FID-001/002/003 and
   MOB-06/supply-chain boundary sections; it did not reread every public corpus
   row or implement a runner. Mobile sections are boundary context only.
2. Resolve the metadata request above without exposing blind content to the
   development process. Custodian/evaluator retain source and label access
   separately. Public handoff keeps status and code references only.
3. Build a local, isolated harness around the shared workflow and authenticated
   node ports. Validate admission/rejection with public synthetic fixtures only;
   do not call an inference endpoint before the complete package is locked.
   Provide exact-tokenizer and managed-worker integration for each family.
4. Wire private immutable per-source/per-repeat records before dispatch. Bind
   attempts, compiler and prepared output separately; collect actual tokens,
   calls, stage/end-to-end times, work, peak memory and terminal failures. Crash,
   timeout, OOM, cancellation and unsupported results retain rows. Keep validated
   alternate bytes and both generic and normalized-semantic identity measures.
5. Bind evaluator/reducer versions and preregistered source-level statistics.
   Report precision/recall and agreement per language/feature, full denominators
   plus successful-intersection diagnostics. No majority winner, silent case
   removal or aggregate score hiding a weak feature.
6. Lock the final artifact/build/analysis package and reviewer evidence, then
   verify it at admission. Only then execute two-family repeated holdout,
   no-LLM baseline, bounded read-only legacy ablations and resource/fault lanes.
   Publish a qualification manifest only for a measured passing tuple; otherwise
   preserve the rejected/unqualified result and manual/no-LLM path.

Unchanged machine gates: 100% conformance and same-SEM identity; zero critical
safety failures, budget overruns or successful late callbacks; complete-source
precision >=98%, recall >=90%, unsupported abstention >=99%, repeat agreement
>=95%, cross-model agreement >=90%; p95 complete latency <=30 seconds
constrained / <=120 seconds standard. No-LLM has zero calls and its 30-second
ceiling. No thresholds or feature scope were changed.

## Reproducible local checks

Observed file SHA-256 at this checkpoint (contract inputs, not a run lock):

- `profile.json`: `56379d7bf6a69554801ce87bea75d5c384c600a190bcd1dc5643f8c70de0541f`.
- `bundle.manifest.json`: `737d5cdc2e0d6e96b3823e1f5386d79d5fde1c922792c1858857a5104a909ba1`.

Executed from repository root on 2026-09-27:

| Check | Actual result |
|---|---|
| `python -m unittest scripts.encoder.test_contract` | PASS, 18 tests; public synthetic conformance only. |
| `python -m scripts.encoder.generate_bundle --check` | PASS, eight generated artifacts verified. |
| `python scripts/ci/validate_vnext_contracts.py` | PASS, including 48 encoder cases/two jobs and 529 contract links. |
| `git diff --quiet HEAD -- src scripts docs/specs` | PASS; tracked implementation and specification trees unchanged. |
| Handoff local-file links / `git diff --check` | PASS; 197 links across seven changed/new Markdown files, no whitespace errors in tracked diffs. |

No Rust runtime suite, inference benchmark, actual worker fault drill or model
artifact inventory was rerun. The executable qualification harness remains to
be built; this map prepares its integration and admission work without claiming
readiness. Missing input references and independent lock evidence remain the
immediate owner/custodian dependency.

KU-QA-001/INT-KU-OBP-001 acceptance, model/platform/NAT limits, OBP default-off,
rollback/legacy data and staging/runs/durable state remain unchanged. Linux P5
qualification applies only to `c453f3e`. D-010 still requires separate direction
for merging or deleting retained branches; all handoff edits remain local.

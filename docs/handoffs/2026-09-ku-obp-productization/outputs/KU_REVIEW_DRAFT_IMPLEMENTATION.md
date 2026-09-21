# Private review drafts: implementation and development evidence

Owner approved the staged draft and background-job proposal on 2026-09-08.
This follow-up is on `codex/ku-web-001-workflow` and is not a model qualification.
The [profile](../../../specs/vnext/KU_REVIEW_DRAFT_PROFILE_V1.md) defines its scope.

## Implemented surface

- `ku-encoder::extraction::review_draft`: exact quoted semantic claims, separate
  qualifiers and quantities, bounded review edits, retained revisions and source
  coverage checks. No model-generated cryptographic IDs or UTF-8 arithmetic.
- Managed Ollama uses the same admitted artifacts, tokenizer, owned CPU worker,
  closed structured output, empty Qwen thinking prefix and cancellation ownership
  as the existing encoder. Each call has 600 seconds, each job 1800 charged seconds.
- Node-owned encrypted checkpoints share the existing Vault/journal. Start
  commits source consent and queues a job before returning. Poll/list only read.
  A single inference queue limits concurrency; up to 16 jobs may wait/run.
- Explicit cancel fences late results. Process shutdown interrupts work. Restart
  preserves drafts and charged calls/time; explicit resume uses the recorded job
  even when the original Base receipt has become `unknown_outcome`. It does not
  replay or authorize canonical preparation, save or publication.
- `KuReviewDraft.tsx` shows subject, relation, arguments, qualifiers, number/unit/
  counted entity, source evidence, remaining issues and earlier versions. It
  reopens recent jobs and keeps their content after a failed progress read.

The component uses `review_drafts_available`. The installed Ollama adapter now
advertises it after the inspected Qwen3 8b development gate described below.
The rebuilt local host was verified end-to-end with both original sources.
Existing canonical Candidate encoding is preserved. Frequency/count semantics
still need a reviewed lowering plan before a draft can become a complete KU.

## Development runs and limitations

Only the owner's two public development sentences and explicit development
variants were used. Private qualification holdouts were not accessed. Expectations
remain outside model requests. Raw reports, including exact prompts and responses,
are private in `OneBrainLocal/development-reports`; no source was published.

The Python constrained run `20260908-staged-constrained-final-qwen8b.jsonl`
produced correct subject/predicate/qualifiers/quantity details for both originals:
water 124.76 seconds (review emitted an empty edit), car 93.06 seconds (draft_ready).
These were loopback prompt experiments, not proof of the production composition.

The first actual ManagedOllama + DraftJob run is
`20260908-native-review-qwen8b.jsonl`. It exposed additional weaknesses:

| Development case | Seconds | Recorded result | Inspected limitation |
|---|---:|---|---|
| Water, usually, temperature, near sea level | 182.18 | needs_review | Initial draft correct; unnecessary number pass added `Nước` as counted entity; reviewer repeated inert edits |
| Personal car, usually four wheels | 143.00 | draft_ready | Subject, relation, frequency, `4` and counted entity `bánh` retained |
| Negation and contrast | 125.18 | needs_review | Predicate duplicated negation; reviewer tried moving it to the other subject; rejected |
| Conditional statement | 160.70 | needs_review | Condition retained, but subject misplaced and relation target nonexistent; repair rejected |
| Three-claim composition | 128.11 | needs_review | Draft had duplicated negation and an extra contrast edge; number pass proposed a number for the wrong claim |

Changes made in response: skip number analysis when numbers already have complete
proposed kinds; flag simultaneous unit/count assignment as ambiguous; ignore only
provably inert edits while retaining strict stale-value checks; provide generic
condition/contrast guidance using different development examples. Do not describe
the preceding table as a pass.

The post-change actual ManagedOllama rerun
`20260908-native-review-v2-qwen8b.jsonl` passed all five inspected development
meanings and returned `draft_ready`, with two calls per case:

| Case | Seconds |
|---|---:|
| Water | 163.43 |
| Car | 123.18 |
| Negation/contrast | 126.60 |
| Conditional | 114.24 |
| Three-claim composition | 170.12 |

The composition retained reciprocal contrast links between the lamp and fan.
These express the same symmetric contrast; they are redundant, not two different
facts. The post-run assessor was corrected to compare contrast edge sets while
keeping causal/conditional direction strict. This correction has a negative test
for a wrong endpoint; it does not modify model output or runtime acceptance.
Raw evidence still includes the reciprocal links. Evidence spans can remain
broader than the narrowest claim, another draft-quality limitation.

`scripts/encoder/assess_development_draft.py` checks subjects, relations, arguments,
qualifiers, quantities and links against public development expectations stored
separately from model input. Mechanical readiness is reported separately. These
five cases are not a broad semantic benchmark or factual verification.

The same native runner and prompts with Qwen3 1.7b produced `needs_review` on all
five cases: 61.87 / 57.74 / 53.06 / 51.97 / 68.56 seconds in the same order.
It omitted an argument preposition, added an incorrect location, confused roles
and negation, misplaced the conditional subject and failed composition grounding.
The prior drafts remained inspectable. This is evidence that the current prompt
does **not** yet generalize well to the smaller model; the feature does not label
that model as qualified or silently replace it with 8b. Report:
`20260908-native-review-v2-qwen1_7b.jsonl`.

## Verification checkpoint

- 13 Python development-probe/semantic-assessor tests passed.
- 13 Rust review-draft tests passed, including job-wide claim bounds. The broader
  extraction suite passed 38 tests with one opt-in real-worker test skipped before
  the last claim-bound test was added.
- Five actual API integration tests passed: background return/cancel fencing,
  operation cancellation, consent/session/auth guards, restart read without model,
  and interrupted resume with charged budget preserved.
- API regression: 18 passed, one opt-in canonical real-inference test skipped.
- Web production build passed. Keyboard tests were updated for the new disclosure
  that keeps the older direct Candidate workflow available below the draft flow.
- Final Web suite: 19 passed, including accessibility/keyboard and draft recovery.
- KU encoder and aggregate vNext contract validators passed.

Installed-model support is still Qwen3 via the admitted adapter; the
shared task/provider interface supports additional adapters but does not pretend
arbitrary model architectures are already admitted. Independent factual or
cross-model verification and canonical lowering remain separate follow-up work.

## Local usage

Open `http://127.0.0.1:4280/ku`. Use **Bản nháp tri thức · xử lý nền** at the top:
select the installed model, enter source, consent, and choose **Tạo bản nháp nền**.
The page can be closed while work runs. Recent jobs reopen recorded progress;
**Kiểm tra tiến độ** reads it without rerunning AI. **Dừng công việc** is explicit
cancellation. Interrupted jobs expose **Tiếp tục phần còn lại**.

The previous direct Candidate workflow is under **Encode trực tiếp sang KU ·
luồng thử nghiệm cũ**. It retains its existing representational limitations.
The new draft interface deliberately has no Save KU or Share button because
the canonical lowering for all retained draft constructs is not implemented.

Exact semantic instructions are in `docs/specs/vnext/ku-review-draft-v1/`:
`draft.vi.txt`, `review.vi.txt`, `numbers.vi.txt` and corresponding schemas.
Actual request prompts/schema/model pins are retained in native development
reports. Qwen thinking is disabled through the admitted empty-think raw template.

Browser verification on the rebuilt host confirmed the new interface, source
details and partial semantic claims during the review stage. Reload restored the
most recent job without a model call. Original local Registry/Vault data and keys
were reused; no replacement Registry or synthetic source governance was installed.

Final real API report: `20260908-review-host-originals.jsonl`, produced by
`scripts/encoder/probe_review_host.py` against the actual owner host/Registry.

| Original source | Start response | Completed draft | Calls | Inspected meaning |
|---|---:|---:|---:|---|
| Water | 0.085 s | 170.95 s | 2 | Pass |
| Car | 0.071 s | 126.06 s | 2 | Pass |

Both returned `draft_ready`. Repeating the identical start and reading progress
left calls/revisions unchanged. Both appeared in recent jobs. The saved KU list
was unchanged; no canonical prepare/save/share request was sent. These two private
jobs remain readable on the Web. Host startup health reported HTTP 200, Registry
ready, local encoder ready and `model_qualified:false`.

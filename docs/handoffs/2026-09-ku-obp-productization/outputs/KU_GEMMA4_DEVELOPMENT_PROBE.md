# Gemma 4 development comparison

Owner requested trying Gemma 4; local runs started on 2026-09-09. This is a
development experiment with the five existing public cases, not qualification or
activation of a new Web model adapter.

## Method

`src/ku-encoder/examples/review_chat_probe.rs` runs the actual Rust `DraftJob`
scheduler, quote validation, conditional numbers/supplement stages, bounded edits
and retained revisions. It uses the same `draft.vi.txt`, `review.vi.txt`,
`numbers.vi.txt` and ordered, source-constrained schemas as the Qwen3 native run.
Only the transport changes: Ollama `/api/chat` renders the installed model's chat
template. No Qwen special tokens are sent to Gemma. Human assessment expectations
are never included in a model request.

Both installed models run sequentially on an isolated loopback Ollama 0.33.3
server, CPU only. Request options are `think:false`, `temperature:0`, `seed:1`,
`num_ctx:8192`, `num_predict:2048`, `keep_alive:0`. There is a 600-second call
limit and the same 1800-second job budget as the draft workflow. Wall times include
model loading, draft generation, any repair/review calls and host checks.

The development transport reserves the full 6144 input-token allowance per call
and checks Ollama's reported token counts after each response. It does **not**
implement exact Gemma tokenizer preflight and is not a production provider.
Reports explicitly retain this limitation, model digest/template/version, exact
HTTP request bodies, raw output, timing/token counters and returned thinking
character counts. The live Web provider admission is unchanged.

## Evidence

Private JSONL reports are in `OneBrainLocal/development-reports`. Semantic checks
use `scripts/encoder/assess_development_draft.py` and the existing separate public
expectations. Both host readiness and inspected meaning are reported: neither
alone establishes factual truth, broad model reliability or canonical KU support.

E4B report: `20260909-native-chat-gemma4-e4b.jsonl`.
Installed digest: `c6eb396dbd5992bbe3f5cdb947e8bbc0ee413d7c17e2beaae69f5d569cf982eb`.
All five jobs used two calls, all ten responses returned zero thinking characters.

| Case | Seconds | Host state | Inspected meaning |
|---|---:|---|---|
| Water, usually, temperature, location | 62.24 | needs_review | Missing `ở` before `100oC`; other roles and number/unit correct; review repeated the unchanged argument |
| Personal car, usually four wheels | 60.79 | draft_ready | Pass |
| Negation and contrast | 70.38 | draft_ready | Pass |
| Conditional statement | 59.89 | draft_ready | Pass |
| Three-claim composition | 79.55 | draft_ready | Fail: fan contrast points to sensor (index 0), instead of lamp (index 1); connector also includes comma |

E4B preserved the three individual claims in the composition but linked the wrong
pair. This is a concrete counterexample to treating schema/quote validation or a
self-review with no findings as semantic approval. Existing draft readiness does
not prove relation endpoints are correct. The development assessor detected the
error independently of the runtime's ready state.

12B report: `20260909-native-chat-gemma4-12b.jsonl`.
Installed digest: `4eb23ef187e2c5462566d6a1d3bbbc2f1346d0b4327cbb66d58fffbcc9b2b05c`.

| Case | Seconds | Host state | Inspected meaning |
|---|---:|---|---|
| Water, usually, temperature, location | 169.88 | draft_ready | Pass, including `ở 100oC` |
| Personal car, usually four wheels | 162.53 | draft_ready | Pass |
| Negation and contrast | 190.82 | draft_ready | Pass; redundant reciprocal contrast links connect the correct lamp/fan pair |
| Conditional statement | 156.71 | draft_ready | Pass; rain remains a condition of slippery road |
| Three-claim composition | 218.95 | draft_ready | Pass; fan contrast correctly points to lamp (index 1) |

All five 12B cases completed with two calls each and passed the separate
development meaning checks. The water case used two
calls with zero returned thinking characters: about 90.76 seconds prompt
evaluation, 48.57 seconds generation and 30.33 seconds model loading. Turning
thinking off does not eliminate those costs. CPU wall times are observations on
this machine, not controlled cross-hardware model rankings.

| Installed model | Meaning checks passed | Mechanically ready | Mean seconds per source |
|---|---:|---:|---:|
| Gemma 4 E4B | 3/5 | 4/5 | 66.57 |
| Gemma 4 12B | 5/5 | 5/5 | 179.78 |

Combined assessment: `20260909-gemma4-assessment.json`. All 20 actual requests
had `think:false`; all 20 replies had zero thinking characters and no transport,
timeout or JSON/schema failure. Maximum reported input/output tokens were
1116/260 for E4B and 1120/406 for 12B. Successful syntax did not prevent the E4B
semantic defects described above. The Gemma prompts were not tuned between runs.

This is one run per case on five known short development sources. It does not
establish performance on long knowledge documents, repeated-run reliability,
factual accuracy or independent cross-model verification. Earlier Qwen3 results
remain in [the draft implementation report](KU_REVIEW_DRAFT_IMPLEMENTATION.md);
they were recorded in a previous session, not rerun concurrently here.

## Verification

- `cargo build -p ku-encoder --example review_chat_probe` passed.
- The new example passed `rustfmt --check` and completed all ten actual model jobs.
- The existing separate semantic assessor was run over both complete reports,
  with manual inspection of all final claims and relation endpoints.
- Read-only live-host check passed after inference: Web HTTP 200, Registry ready,
  local encoder ready, admitted Web catalog still `qwen3:8b`, unqualified.
- Aggregate vNext contract validator passed. Git whitespace check passed with
  Windows CRLF recognized as line endings; no broad line-ending rewrite was made.
- The dedicated probe server was stopped after checking its executable, listener
  ownership and empty model list. The original Ollama and Web host remain running.

## Implications for follow-up work

This experiment preserves a common task contract across model families; it does
not select a universal best model. The E4B composition failure motivates testing
source-quoted relation endpoints that the host resolves, instead of asking the
model to maintain fragile statement indices. That change still needs ambiguity
checks and independent semantic review: matching a quote alone cannot prove the
model chose the right relationship. This is a proposed next experiment, not an
implemented schema change or permission to promote drafts to canonical KU.

Production Gemma support also needs an admitted template/tokenizer/resource and
cancellation adapter. The development HTTP transport is evidence for prompt and
workflow portability only. Longer documents, mixed models and independent
verifiers require separate coverage beyond these five short development cases.

## Reproduction

From the `src` workspace, build with
`cargo build -p ku-encoder --example review_chat_probe`. Run the executable with
four arguments: development cases JSON, a **new** private JSONL report path,
installed model name, and the dedicated loopback port. The tool refuses to
overwrite reports or download missing models. Assess the resulting report with
`python scripts/encoder/assess_development_draft.py REPORT.jsonl` from repo root.

This probe creates no source admission, Registry entry, canonical KU, save or
publication request. It leaves the running Web host and existing private jobs
untouched.

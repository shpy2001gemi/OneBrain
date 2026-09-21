# Shared semantic selection: implementation and development evidence

Owner approval: 2026-09-09, D-023 follow-up. The owner requires consistent behavior
on every platform. The [shared architecture contract](../../../specs/vnext/KU_SEMANTIC_SELECTION_PROFILE_V1.md)
is linked from the extraction framework, review-draft profile, editor profile and
vNext index. This update does not implement mobile or alter canonical KU/SEM.

## Implementation

- `ku-encoder::extraction::semantic_selection` supplies common numeric syntax
  hints, sparse schemas/prompts, deterministic full-draft assembly, quote/evidence
  anchoring, source-quoted link resolution and checked targeted repairs.
- `DraftJob::new_selection` binds `ku-semantic-selection/1.0` and its executable
  commitment. It reuses durable reservations, limits, revisions and interruption
  handling. Legacy jobs retain their original fields/state and remain readable.
- `Select` returns only populated semantic choices. Host code fills all structural
  fields, parses numeric values and maps relation targets. Raw choices/proposals
  are retained alongside complete drafts; failed repairs preserve prior versions.
- Repairs bind one claim and declared fields. The model cannot choose a claim
  index, arbitrary field path or old-value precondition. Invalid/unhelpful patches
  stop with `needs_review`; they do not trigger full regeneration.
- Node review-start now creates selection jobs. The UI presents `draft_extracted`
  as unassessed, distinct from legacy `draft_ready`. No independent verifier is
  configured by this change; the API and UI explicitly report that fact.

Clients render the node result; they do not duplicate prompts or assembly logic.
Model adapters retain their existing tokenizer, memory and cancellation duties.
Gemma's development chat probe is still not a production adapter.

## Verification checkpoint

- Encoder extraction tests: 51 passed, one opt-in worker test skipped; the
  expanded selection-only suite then passed all 14 tests (two additional cases
  cover cross-window claim accounting and inert structural patches).
- Five API tests passed, including interrupted targeted repair with prior revision
  and charged budget preserved, cancel fencing, read/repeated-start without
  inference, consent/auth/session guards and no KU save.
- Web tests: 20 passed. Production Web build passed.
- One negative repair test exposed argument deletion hidden by overlapping numeric
  coverage. The implementation now rejects that loss, and the negative test passes.

## Model experiments

Private raw reports are under `OneBrainLocal/development-reports`. Earlier failed
iterations remain retained and are described below. Final September 20 activation and comparison results are recorded below;
the September 9 entries are historical development iterations.

The first E4B selection run completed in 42.77/40.42/44.26/25.35/49.14 seconds
for water/car/contrast/condition/composition. Only the conditional case passed
the separate meaning checks. Water put location among arguments; the car repair
called the counted entity a unit; the contrast omitted the lamp claim; composition
duplicated negation inside its predicate. This run is an unsuccessful development
iteration, not an activation gate or evidence that reduced output preserves quality.
Report: `20260909-selection-v1-gemma4-e4b.jsonl`.

The follow-up prompt retains sparse fields but explicitly requires populated
semantic content and complete claims. Added generic count and two-claim examples
are different from the test sources. The raw failed run remains retained; no
assessment expectations or host acceptance rules were weakened to count it as pass.

E4B second iteration (`20260909-selection-v2-gemma4-e4b.jsonl`) passed car,
negation/contrast and condition. Water still omitted an argument preposition and
its repair attempted unrelated fields (rejected); composition had an extra self
link and a no-op repair (rejected). Times were 59.08/49.63/35.37/29.18/63.14 s.
The decoder has since been restricted to the host-declared repair fields, on
both short and long windows, with a negative conformance test. The host's
independent rejection of undeclared fields remains in place.

Qwen's first sparse run (`20260909-selection-v3-qwen8b.jsonl`) passed 4/5:
water omitted `thường`. A shared lexical cue router now requests only the
frequency field when the uncovered cue falls within exactly one grounded claim.
The router does not insert a semantic value. Tests cover an unrelated second
clause whose qualifier must not be assigned to the first claim.

### Qwen3 8b, September 9 native development run

`20260909-selection-v4-qwen8b.jsonl` uses the production managed Qwen adapter,
CPU only, thinking disabled, the same five cases and unchanged separate assessor.
All five meanings passed, including exact role coverage and contrast endpoints.

| Source | Seconds | AI calls | Inspected result |
|---|---:|---:|---|
| Water / usually / temperature / location | 79.12 | 2 | Pass; second call adds only `frequency: ["thường"]` |
| Personal car / usually / four wheels | 46.48 | 1 | Pass; `bánh` is a counted entity |
| Negation and contrast | 48.91 | 1 | Pass; fan contrasts with lamp |
| Conditional statement | 45.77 | 1 | Pass; rain remains a condition, not a fact asserted to have occurred |
| Three-claim composition | 66.51 | 1 | Pass; fan contrasts with lamp, not sensor |

Compared with `20260908-native-review-v2-qwen8b.jsonl`, mean first-call latency
fell from 70.21 to 51.99 seconds (26.0%). Mean first response size fell from
617.2 to 250 UTF-8 bytes (59.5%); these are bytes, not output token counts.
Mean first input tokens stayed similar, 1134 versus 1146.4: semantic instructions
were retained. Total calls fell from ten to six and total input tokens from
10,741 to 6,308. Mean total source latency was 57.36 versus 139.51 seconds.
The total reduction includes removal of mandatory self-review; the new result is
explicitly unassessed and is not an equivalent verified workload becoming faster.
Provider integrity initialization occurs before these per-source timers.

### Gemma 4 E4B, September 9 development chat run

`20260909-selection-v4-gemma4-e4b.jsonl` passed 3/5 meanings with eight calls.
The same source/prompt/schema and targeted repair rules are used; no per-model
semantic implementation is introduced.

| Source | Seconds | AI calls | Host state | Inspected meaning |
|---|---:|---:|---|---|
| Water | 55.32 | 2 | draft_extracted | Fail: repair put the whole source into `arguments` |
| Car | 50.66 | 2 | draft_extracted | Pass; repair supplied the counted quantity |
| Negation and contrast | 35.87 | 1 | draft_extracted | Pass |
| Conditional statement | 30.51 | 1 | draft_extracted | Pass |
| Three-claim composition | 70.39 | 2 | needs_review | Fail: extra self-link; unchanged repair was rejected |

Mean E4B total latency was 48.55 seconds versus 66.57 previously. First-call
latency was 35.74 versus 39.94 seconds; mean first output tokens were 75.6 versus
149.2 (49.3% fewer), with essentially unchanged input, 1109.4 versus 1109.2 tokens.
There were zero returned thinking characters in all eight new calls.

The water repair preserves source coverage but chooses the wrong semantic span.
This is an explicit counterexample to treating mechanical repair acceptance as
semantic verification. The composition keeps all three useful claims and the
valid fan-to-lamp link; the extra rejected lamp-to-itself selection remains visible
as an issue and in retained raw proposals. Neither failure is counted as a pass.

### Inspecting the actual model instructions

The shared [selection prompt](../../../specs/vnext/ku-semantic-selection-v1/selection.vi.txt)
retains role, qualifier, condition, relation and multi-claim instructions. The
[repair prompt](../../../specs/vnext/ku-semantic-selection-v1/repair.vi.txt) only
permits host-declared fields. Exact populated requests and raw replies are in
the private trace reports.

For the car source, Qwen returned these semantic choices (object key order and
whitespace omitted here):

```json
{"claims":[{"subject":"Xe oto cá nhân","predicate":"có","arguments":["4 bánh"],"frequency":["thường"],"quantities":[{"quote":"4 bánh","counted_entity":"bánh"}]}]}
```

The host derives the `4` numeric quote, checks exact numeric syntax, constructs
the evidence span, fills unused arrays and produces the complete retained draft.
The model still makes semantic selections using a small JSON envelope. Replacing
that envelope with free text would still require an equally unambiguous parser;
JSON itself is not the expensive semantic or bookkeeping task being removed.

The exact same five public development cases and separate semantic assessor are
used. `KU_SELECTION=1` selects the new flow in the existing native/chat probes.
No model gets the human expected answer. Gemma chat requests use `think:false`,
CPU, 8192 context, 2048 output, temperature zero, seed one and unload after each
call, as in the previous baseline.

Latency comparisons must separate the smaller extraction payload from elimination
of mandatory same-model review: selection jobs are explicitly **unassessed**.
An independent verifier would consume additional work. Structural readiness must
never conceal semantic failures; inspect roles, quantities and link endpoints.


## September 20 continuation: mechanically constrained repair choices

The September 9 12B report (`20260909-selection-v4-gemma4-12b.jsonl`)
passed 4/5 meanings, with times 77.71/78.18/73.91/69.67/143.03 seconds.
Composition retained the correct three claims but proposed an extra lamp self-link,
then repeated that proposal during repair. The host retained `needs_review`.
Mean first-call latency was 78.59 versus 120.31 seconds in the legacy run;
mean first output tokens were 52.8 versus 245.4. All six calls returned zero
thinking characters. The failed case remains a failure despite faster output.

The shared repair planner now supplies finite `CHOICES`:

- Missing-preposition argument repair: original arguments and exact adjacent
  source extensions; whole-sentence replacement is rejected by both decoder and
  host validation.
- Link repair: evidence quotes that map uniquely to other retained claims;
  self and ambiguous targets are excluded. The model still chooses relationship
  and target, removes an invalid link, or returns unresolved.
- Short-window quote enumeration intersects these restrictions instead of
  overwriting them. Long-window repair retains the same restrictions.

This change does not select the nearest/first target or silently add semantic
relations. A new negative test covers whole-sentence argument repair and another
checks constrained links on short and long sources. The architecture contract
records the same behavior for every platform.

Validation on September 20: all 15 selection tests, five API background-job
tests, twenty Web tests, production Web build and aggregate vNext contract
validation passed. Host `ku_local_web` was rebuilt and started with the existing
signed Registry, dataset and keys. Read-only startup checks passed; three legacy
`draft_ready` jobs remained readable. No model downloads or provider admission
changes were made.

Ollama on this machine is now 0.34.2; September 9 runs used 0.33.3. Cross-date
latency comparisons are descriptive, not controlled benchmarks. New Web API jobs
use the current shared executor and retain explicit semantic/factual unassessed
status. The two owner examples passed: water 75.04 s, car 42.61 s. Full Qwen rerun passed all five separate meaning checks. Report:
`20260920-selection-v5-web-qwen8b.jsonl`.

| Case | Seconds | Calls |
|---|---:|---:|
| Water | 75.04 | 2 |
| Car | 42.61 | 1 |
| Negation / contrast | 44.68 | 1 |
| Condition | 40.59 | 1 |
| Composition | 58.66 | 1 |

Mean time: 52.32 seconds. Admission took 0.050–0.073 seconds. Every completed
job was read and started again with its original idempotency key: calls and
revisions stayed unchanged; each appeared in recent jobs; the saved KU list
was unchanged. All five report semantic/factual verification as unassessed.
The browser was reloaded and visibly showed all three composition claims,
lamp-only negation and the fan contrast pointing to claim 2 (lamp).

The shared executable commitment is
`53cf766901d9e84b76902ea1d528d20618372cee9953174e76861db93d10f5c8`.
Both Gemma comparisons completed on this same commitment; see results below.


### September 20 E4B outcome

`20260920-selection-v5-gemma4-e4b.jsonl`: **4/5** meanings passed; eight calls,
mean 43.19 seconds, zero returned thinking characters. Times:
49.05/47.80/32.99/26.17/59.94 seconds.

Water remains `needs_review`: it omits `ở` and its bounded repair returns the
same argument, so the host rejects a no-op. It can no longer put the entire
source into `arguments` and claim mechanical completion. Composition now passes:
the link repair chooses `quạt chạy` from valid alternatives; both reciprocal
contrast links connect lamp and fan, with no link to the sensor or self. The
model, not a host semantic rule, selected this endpoint. Raw initial and repaired
choices remain in the report. The unchanged first extraction still makes the
same errors; this is evidence for a bounded repair improvement, not proof the
model understands arbitrary text reliably.


### September 20 12B outcome and completion

`20260920-selection-v5-gemma4-12b.jsonl`: **5/5** meanings passed; six calls,
mean 78.58 seconds, zero returned thinking characters. Times:
69.04/68.99/69.36/62.91/122.59 seconds. Composition takes two calls: the retained
initial self-link is repaired to the fan clause, while all original semantic
roles, quantities and other claims remain intact. The first four cases each use
one call. There were no transport, timeout or schema failures in this run.

| Current development path | Meaning checks | Mean seconds | Calls for five sources |
|---|---:|---:|---:|
| Qwen3 8b, actual Web API | 5/5 | 52.32 | 6 |
| Gemma 4 E4B, development adapter | 4/5 | 43.19 | 8 |
| Gemma 4 12B, development adapter | 5/5 | 78.58 | 6 |

All use the same shared selection and repair executor. Gemma uses its installed
chat template with `think:false`; Qwen uses its admitted raw template with closed
thinking prefix. This table describes one run on five known short development
sources. It does not qualify a model, establish long-document accuracy or prove
that the independent verification stage is implemented. Gemma remains a
headless development adapter; the Web model catalog still admits Qwen3 8b only.

Private final assessment: `20260920-selection-final-assessment.json`, including
report hashes and per-case findings. Raw requests/replies and rejected proposals
are retained in the reports. The dedicated test server had already exited when
work resumed; the user's default Ollama was left untouched. The local Web host
was restarted after the app interruption, using the existing Registry and keys.
No KU save, publication, reward, commit, push or merge was performed.

The approved shared architecture/implementation change is ready for review.
Remaining separate work: independent semantic verification, canonical KU lowering,
broader blind/long-document qualification and production admission of additional
model adapters. Future platform clients must consume this common executor;
no mobile implementation is included.


Final restart check passed: Web HTTP 200, signed Registry ready, local encoder
ready and admitted catalog `qwen3:8b`. All five new jobs preserved their completed
state, exact revisions and call counts after restart; three legacy jobs remained
readable. No inference was triggered by these reads. Private evidence:
`20260920-selection-restart-check.json`. Final aggregate contract and whitespace
checks passed. The local host remains running at `http://127.0.0.1:4280/ku`.

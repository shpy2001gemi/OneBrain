# Shared private semantic selection v2 — KU-SEM-001

Owner approved the [representation proposal](../../handoffs/2026-09-ku-obp-productization/outputs/KU_SEM_001_REPRESENTATION_PROPOSAL.md)
on 2026-09-20. Profile `ku-semantic-selection/2.0` produces
`ku-semantic-draft/2.0`; implementation and development evidence are separate.
This contract adds no canonical SEM primitive or KU publication authority.

## Representation

The v1 exact-quote subject, predicate, argument and qualifier fields remain a
source view. Structured alternatives replace the interpretation of the matching
argument: the group quote MUST equal one retained argument. Its ordered branches
have exact local surface selectors, optionally with a borrowed source selector.
Borrowing is `reconstructed`, never an explicit expanded quote. The group cue
represents OR; exclusivity is unspecified unless the model proposes otherwise.
Non-default exclusivity requires review; host syntax is not semantic proof.
Nested groups or qualifier scope smaller than a claim remain unresolved in this
initial bounded surface. Do not flatten unsupported nesting into independent facts.

`ellipses` annotates a retained subject/argument with its local surface and borrowed
source. `references` selects a local cue and a term/group/claim target. Host resolves
only a unique target of that kind, retaining a proposed semantic status. Reference
dependency cycles are invalid; symmetric contrast links remain allowed.
`comparisons` retains property, cue, and an explicit, implicit-candidate or
unspecified target. Unspecified forbids a target; other variants require one.
Implicit candidates are inferred and require review. An unspecified target also
requires review but may faithfully represent the source. Property must be the
claim predicate; comparison subject and temporal/modal scope belong to that claim.

Selectors contain `quote` plus optional exact `within`. Host computes UTF-8
start/end offsets relative to the current source window; window start maps these
to the consented full source. A selector must resolve exactly once. No nearest
occurrence fallback; no expanded text becomes evidence. Role and cue spans count
toward coverage; borrowed/target spans are separate provenance and do not cover
other claims. Ambiguous evidence never covers every matching occurrence.
The output retains raw structured choices, host IDs, source spans and resolved
targets. Clients must render the structured interpretation, including uncertainty,
instead of presenting a flat argument as the complete meaning.

## Quantity and repair

Numeric annotations require one complete source numeric token accepted by the
exact parser. Dynamic grammar forbids quantities when no supported token exists;
host validation applies on every window length. Number words and unsupported
numeric forms stay quoted with unresolved status; absence of digits proves nothing
about the absence of numeric meaning. Model prompts retain that responsibility.
Host detects unsupported digit syntax and missing supported numeric annotations.
Common VI/EN number-word cues trigger review only; homonyms can be false positives,
and unlisted languages/forms remain the extractor/reviewer's responsibility.
No numeric value is inferred from these lexical hints. OR/comparison cues lacking
structure and predicate/qualifier overlap likewise raise issues without assigning
semantic roles. Borrowed text in an alternative must occur in another branch of
that group; broader ellipsis uses its separate field or remains unresolved.
Known conjunction/contrast cues (`và`, `and`, `nhưng`, `but`) inside an OR group
raise an incompatible-cue issue. This is a negative guard, not a multilingual
parser or proof that any other cue expresses disjunction.

Repairs are claim/revision bound with field allow-lists. A repair cannot remove
previous grounded roles/branches/qualifiers, change provenance to explicit without
source, increase uncovered byte positions or add issues. Structured fields are
atomic. One extraction and at most two repairs per window; no-op stops. Raw
proposals and prior revisions survive rejection. Host does not invent references,
comparison targets or causal edges. Unresolved semantic issues need human or later
independent review, not automatic whole-source regeneration.

Predicate/qualifier overlap may dispatch a predicate-only repair. Decoder choices
retain the original predicate or remove one already-selected leading/trailing
qualifier at a whitespace boundary. Every candidate is a contiguous exact source
substring; the qualifier stays in its original role. The model chooses; the host
does not apply this trim automatically. The host independently enforces the same
finite choices and full revalidation, including no-op and coverage-loss rejection.

## Lifecycle, limits and compatibility

Use the existing durable reservations, cancel/restart fencing and consent boundary.
Any unresolved item or host issue produces `needs_review`. Clear assembly yields
`draft_extracted` with both verifications `unassessed`; never canonical readiness.
Retain 8192 source bytes, 16 windows, 64 claims, 32 calls, 6144 input/2048 output
tokens per call, 600 seconds/call, 1800 charged seconds/job, and 1 MiB checkpoints.
Count at most 256 structured nodes per job, 16 branches/group, expression depth
at most 8 (the current nonrecursive wire surface is stricter). Enforce before
assembly; resource failure preserves previous revisions.

V1 records remain readable, without conversion or budget resets. Changing shared
lifecycle code changes its executable commitment; old commitments are not aliases
for new code. Resume is denied unless the exact executor commitment is available.
New output includes its profile. Unknown profiles are displayed as unsupported,
without interpreting their fields as v1. Existing canonical/save endpoints and
model admission remain unchanged. Production activation requires separately
inspected headless evidence; a successful mechanical test is insufficient.

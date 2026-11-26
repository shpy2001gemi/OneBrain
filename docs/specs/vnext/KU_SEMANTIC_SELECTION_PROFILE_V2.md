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
previous grounded roles/branches/qualifiers (sole exception: D-064 below), change provenance to explicit without
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

Borrowed text in an OR group that is not taken from another branch — it resolves
in no other branch (`borrowed_outside_other_branch`) or not uniquely inside the
group (`borrowed_ambiguous`, e.g. a predicate outside the group) — may dispatch an
`alternatives`-only repair (D-060). Per flagged branch the choices are: omit
`borrowed`, keeping only the exact branch surface, or one whitespace-bounded token
run of another branch surface that resolves uniquely inside that branch. Group
and branch counts, surfaces, quote/cue/exclusivity and every other reconstruction
are unchanged. Dropping a reconstruction leaves an explicit, source-quoted surface;
it does not assert that the shorter branch is the intended meaning.

Branch scope or ordering failures in an OR group (`branch_scope_or_order`) may
likewise dispatch an `alternatives`-only repair when finite local choices exist
(D-061). If all branches already resolve uniquely and disjointly inside the group
quote outside its cue, only reordering those existing branches (with their own
reconstructions intact) into source byte order is permitted. Otherwise, only
branches whose surface fails unique resolution inside the group quote, overlaps
the cue, or overlaps another branch may replace `surface` with one unique
whitespace-bounded token run inside the group quote outside the cue (and outside
fixed sibling spans); repaired groups satisfy left-to-right non-overlapping order,
while group/branch counts, quote/cue/exclusivity and unflagged surfaces or
reconstructions remain unchanged. Groups without a unique local token run or an
unambiguous cue stay unrouted in `needs_review`.

An OR group whose cue is a known conjunction/contrast cue (`incompatible_or_cue`)
may dispatch an `alternatives`-only repair whose only finite choice is omitting
that whole group (D-064). This is the sole exception to keeping previous branches:
the group's branches only carried an OR operator the cue does not support, and the
exact group quote stays the claim's bound argument. Kept groups stay byte-identical
and in order; a relabelled cue, partial branch removal or substituted group is
rejected, and full revalidation (no new issues, no coverage loss) still applies.
Omission asserts no conjunction or contrast operator. Groups whose quote also
contains an OR cue, whose quote is not a bound argument, which do not resolve
uniquely, or which contain a resolved reference target stay unrouted.

Comparison binding errors may dispatch a `comparisons`-only repair (D-065). A
comparison flagged `property_binding` whose property and the claim predicate are
contiguous whitespace-bounded token extensions of one another may only be rebound
to the exact claim predicate. A comparison flagged `unexpected_target` (a target
emitted with `target_kind: unspecified`) may only omit that ungrounded target; the
`target_unspecified` issue stays visible. Comparison count, cues and target kinds
are fixed, no target is added, substituted or marked explicit, and full
revalidation still applies. Unrelated properties stay unrouted.

A comparison flagged `target_absent_or_ambiguous` (an explicit or
implicit-candidate target that does not resolve uniquely in its scope) may join
that `comparisons`-only repair (D-067). Its only finite choice is downgrading to
`target_kind: unspecified` with the unresolved target omitted; the resulting
`target_unspecified` issue is the sole new issue revalidation admits and stays
visible for review. No target is added, substituted or re-resolved, no other kind
is chosen, unflagged cues stay fixed, and a target cannot be dropped without the
downgrade.

A comparison flagged `explicit_target_scope` (an explicit target that resolves
in the claim scope but lies outside the claim's evidence span or inside a term
owned by another claim) may join that `comparisons`-only repair (D-069) with the
same single choice: downgrading to `target_kind: unspecified` with that target
omitted, admitting only `target_unspecified` as a new issue. The target is not
re-scoped, substituted or rebound, other claims and in-scope comparisons stay
unchanged, and full revalidation still applies.

A comparison flagged `cue_ambiguous` (`cue` does not resolve uniquely in the
claim scope) may join that `comparisons`-only repair when finite local choices
exist (D-070). A flagged comparison may only narrow `cue` to one contiguous
whitespace-bounded token run of the emitted `cue` itself that resolves uniquely
in the claim scope. Unflagged comparison cues, comparison count, properties,
target kinds and targets stay unchanged, no token outside the emitted `cue` or
sub-word slice is accepted, and full revalidation (no new issues, no coverage
loss) applies. Cues with no unique local token run (such as an absent cue or a
repeated single-token cue) stay unrouted in `needs_review`.

An OR group flagged `cue_ambiguous` (`cue` does not resolve uniquely inside
`group.quote`) may dispatch an `alternatives`-only repair when finite local
choices exist (D-071). A flagged group may only narrow `cue` to one contiguous
whitespace-bounded token run of the emitted `cue` itself that resolves uniquely
inside `group.quote` and overlaps no located branch span. Unflagged group cues,
group count, quotes, exclusivity, branches and borrowed text stay unchanged, no
token outside the emitted `cue`, branch token or sub-word slice is accepted, and
full revalidation (no new issues, no coverage loss) applies. Groups without a
unique local token run (such as an absent cue or a repeated single-token cue)
stay unrouted in `needs_review`.

An OR group flagged `argument_binding` (`group.quote` equals none of the claim's
`arguments`) whose emitted `quote`, `cue`, ordered disjoint `branches` and any
`borrowed` reconstructions resolve cleanly may dispatch an `alternatives`-only
repair when finite local choices exist (D-073), and an OR group flagged
`group_ambiguous` (`group.quote` fails unique resolution in the claim scope, with
`exclusivity: unspecified` and no incompatible conjunction/contrast cue) may
likewise join that repair (D-074). In either case a flagged group may only rebind
`quote` to an exact bound `argument` of the same claim that is a contiguous
whitespace-bounded token extension of the emitted `quote`, is not already bound
by another group of the claim, and preserves unique `cue`, branch and `borrowed`
resolution with no new issue or coverage loss. Unflagged group quotes, group
count, cues, exclusivity, branches and borrowed text stay unchanged, no unbound
quote or sub-word slice is accepted, and full revalidation applies. Groups
without a valid bound-argument rebind stay unrouted in `needs_review`.

A reference flagged `cue_ambiguous` (`via` does not resolve uniquely in the claim
scope) may dispatch a `references`-only repair when finite local choices exist
(D-072). A flagged reference may only narrow `via.quote` to one contiguous
whitespace-bounded token run of the emitted `via.quote` itself that resolves
uniquely in the same scope (any emitted `via.within` kept) and overlaps no
resolved `to` span. Unflagged `via` selectors, reference count, `to` selectors and
target kinds stay unchanged, no token outside the emitted `via.quote`, target
token or sub-word slice is accepted, and full revalidation (no new issues, no
coverage loss) applies. References without a unique local token run (such as an
absent cue or a repeated single-token cue) stay unrouted in `needs_review`.

A reference flagged `target_absent_ambiguous_or_self` (`to` does not resolve to
one unique addressable target of `target_kind` in another claim) may join that
`references`-only repair when finite local choices exist (D-074). A flagged
reference may only narrow `to.quote` to one contiguous whitespace-bounded token
run of the emitted `to.quote` itself that resolves uniquely (any emitted
`to.within` kept), overlaps no resolved `via` span, matches a single addressable
target of the emitted `target_kind` owned by another claim (any group or branch
target must belong to a bound, structurally valid OR group), and creates no
reference `cycle`. Unflagged `to` selectors, reference count, unflagged `via`
selectors and target kinds stay unchanged, no token outside the emitted
`to.quote`, self-target, unbound/malformed group target, cyclic target or
sub-word slice is accepted, and full revalidation applies. References without a
valid cross-claim target token run stay unrouted in `needs_review`.

An ellipsis whose surface is neither the claim subject nor one of its arguments
(`role_binding`) may dispatch an `ellipses`-only repair (D-066). A flagged
ellipsis may only be omitted, or have its surface rebound to an exact bound
subject/argument that is a contiguous whitespace-bounded token extension of the
emitted surface and resolves uniquely in the claim scope outside a uniquely
resolved borrowed span. Borrowed text is fixed; unflagged ellipses stay
byte-identical and in order; no role, borrowed text or ellipsis is added, and
full revalidation still applies. Omission keeps the argument as an explicit quote
and asserts no reconstruction. A bound ellipsis flagged `self_reference` (its
surface resolves to the very span it borrows) may join that `ellipses`-only
repair (D-068), and a bound ellipsis flagged `absent_or_ambiguous` (`surface` or
`borrowed` fails unique resolution while its bound `subject`/`argument` quote
resolves uniquely in the claim scope) likewise joins that repair (D-074). In
both cases the only finite choice is omitting that ellipsis; the bound
subject/argument stays an explicit quote, no surface, `within` scope or borrowed
text is substituted, and full revalidation still applies. Bound ellipses whose
own role quote does not resolve uniquely in the claim scope stay unrouted.

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

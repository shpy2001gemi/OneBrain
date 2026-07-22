# OneBrain vNext — KQL Complement Planner Profile v1

> **Task:** `KQL-003`  
> **Status:** Normative local-runtime contract — frozen 2026-07-20  
> **Code:** [`ku-kql::vnext_planner`](../../../src/ku-kql/src/vnext_planner.rs)

## 1. Independent candidate channels

`ComplementPlanner` coordinates bounded `CandidateGenerator` implementations
for exact typed index, structural, opposition, long-tail and local-AI channels.
Channels have unique identities and receive the same exact QueryRun/Work
SelectorCID boundary plus their own continuation token.

An empty page means only that one channel produced zero candidates for that
page. It does not terminate the run, suppress later channels or establish a
no-match conclusion.

## 2. Budget ownership

PlannerBudget separately caps generated candidates, validations, accepted
proposals and work units. A generator receives remaining budget and must not
return a page that exceeds it or labels candidates as another channel. Such a
page is rejected as a contract violation rather than silently truncated.

Validation and portfolio insertion consume separate counters. Exact duplicate
ProposalIDs are deduplicated without multiplying accepted-proposal usage.

## 3. Cancellation and continuation

Cancellation is cooperative and deterministic at channel and candidate
boundaries. Budget exhaustion or cancellation returns existing proposals plus:

- per-channel opaque continuation tokens;
- candidate IDs generated but not yet validated;
- exact usage counters and examined-channel list;
- outcome `PARTIAL_BUDGET` or `CANCELLED`.

No partial exit is rewritten to empty/no-match, and prior continuation tokens
are preserved when cancellation happens before a channel resumes.

## 4. Proposal portfolio

The portfolio stores validated BindingProposals by ProposalID in deterministic
order. It preserves every exact score component, constraint state and proposal
disposition. The profile exposes iteration and disposition views but no scalar
winner or automatic materialize/adopt operation.

Hard-violation preservation remains owned by the KQL Proposal Profile; the
planner may retain blocked proposals for explanation while later action policy
refuses them.

## 5. Acceptance evidence

- An empty exact-index page does not prevent a later structural proposal.
- Exhausting validation budget returns one accepted proposal, the unvalidated
  candidate ID and the generator continuation.
- Cancellation preserves a pre-existing long-tail continuation.
- Two proposals with opposing two-component scores coexist in the portfolio;
  neither is collapsed into a canonical scalar winner.

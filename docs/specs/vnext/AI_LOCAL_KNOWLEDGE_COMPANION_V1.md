# AI Local Knowledge Companion Profile v1

> **Task:** `AI-004`  
> **Status:** Complete  
> **Depends on:** `KQL-005`, `AI-003`, `SEC-001`

## 1. Purpose

This profile turns local context and AI-003 observation provenance into a
private query, durable StandingNeed candidates and bounded recommendations. It
is an orchestration layer in `onebrain-node`, not a new KU/KQL authority.

```text
local context + observation EventCIDs/proposal commitments
  → LOCAL_ONLY NeedIR + QueryDefinition
  → LOCAL_ONLY StandingNeeds
  → recommendations: local fetch / optional network fetch / share / materialize
  → explicit existing policy, consent, permit and executor boundaries
```

The Companion never performs the final action.

## 2. Private context and NeedIR

`CompanionContext` carries full-width Receptor references, desired roles, goal,
local context, observed frontier and only AI-003 EventCID/proposal commitments—
not raw observation bytes.

Planning validates the existing KQL query contract and constructs a
`QueryDefinition` whose full `KnowledgeNeedIr` is `LOCAL_ONLY`. One StandingNeed
is derived per Receptor using the exact query ObjectCID, selector CID, watch
policy and observed frontier. Need and StandingNeed are local state; the plan
has no inventory/publication method.

## 3. Deterministic local recommendation budget

Policy freezes selector/budget, query/exploration/watch/materialization policy
references, disclosure policy, route/share purposes and a hard recommendation
count. Recommendations are emitted in deterministic priority order:

1. local private fetch;
2. optional network-fetch proposal;
3. symbolically candidate-only Mapping materialization prompts; and
4. sanitized/disclosure-gated knowledge-share prompts.

The hard count cannot be expanded by model output, provider count or network
availability. Blocked/expired Mapping proposals are not recommended for
materialization.

Recommendations are local UI/planning artifacts. Every recommendation reports
`performs_side_effect = false` and `is_authority_record = false`.

## 4. Offline-first behavior

The fully functional baseline requires no cloud, network, peer or seed server.
Without a network adapter it still returns:

- private query and QueryDefinition CID;
- StandingNeeds;
- local-fetch recommendation;
- eligible local materialization prompts; and
- share prompts with their policy/consent status.

`operates_offline = true` is invariant, not a fallback error state.

## 5. KQL-012 is an optional proposal compiler

`OptionalCompanionMultipathPlanner` is a local compiler interface. It receives
only query CID, selector, bounded budget, exact RouteMinimal scope commitment
and consent commitment. It returns a `MultipathQueryPlan`; it has no network
send API.

The adapter is called only when SEC-001 authorizes RouteMinimal under:

- the exact Companion disclosure policy;
- expected route purpose;
- context/selector/mode-bound scope commitment;
- unexpired consent; and
- local tick.

If the adapter is absent, offline planning succeeds. If policy or consent is
missing/invalid, a blocked recommendation may be shown but the adapter is not
called and no route packet plan is produced.

Even an authorized compiled plan still requires an explicit external executor.

## 6. Share and materialization gates

Each share subject gets its own scope commitment over context, selector, mode
and full object reference. Consent for one object cannot authorize another.
Share recommendation status distinguishes policy-disabled, consent-required,
invalid/expired and ready-for-explicit-executor states.

Raw `source-artifact` kind `22` and `observation-event-payload` kind `23` are
forbidden as direct share subjects. A separate encoding/fidelity/sanitization
workflow must produce a shareable knowledge artifact first.

Mapping recommendation accepts only a non-expired, hard-violation-free
`BindingProposal`. It always reports
`ExplicitMaterializationAuthorityRequired`; the Companion has no mapping backend
or materializer and cannot adopt the Mapping.

## 7. Boundaries

The profile does not:

- serialize full NeedIR or raw observation to the network;
- call cloud inference or require online availability;
- send a compiled KQL-012 packet;
- publish/share without exact policy, purpose, subject scope and consent;
- materialize or adopt a Mapping;
- treat a recommendation as Use, benefit, truth, PoMV or OBT;
- infer global completeness from local results; or
- introduce a Core DNA Gene or execution opcode.

## 8. Executable evidence

Six tests prove:

- the full private-query/StandingNeed/local recommendation path works with no
  network adapter and performs no side effect;
- exact authorized RouteMinimal consent compiles a KQL-012 plan but never sends;
- missing route consent prevents adapter invocation;
- SourceArtifact cannot be recommended for direct sharing;
- missing versus exact subject-scoped share consent changes only the guard and
  never publishes; and
- the recommendation count is a deterministic hard cap.

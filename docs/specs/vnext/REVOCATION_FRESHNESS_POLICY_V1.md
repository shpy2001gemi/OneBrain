# Revocation Freshness Policy v1

> **Task:** `REV-001`  
> **Status:** Complete  
> **Depends on:** `FEED-002`, `CAP-004`

## 1. Purpose

Revocation freshness answers one local question: “Is the authority evidence I
have fresh enough for this action under this named policy?” It does not answer
whether an actor, feed, permit, partition or the OneBrain network is globally
“live”.

Every positive result is relative to exact authority scopes, accepted local
frontiers, a policy profile and a local monotonic evaluation tick.

## 2. Risk tiers and action floor

| Tier | Typical action | Freshness behavior |
|---|---|---|
| `R0` | read immutable knowledge | never freshness-gated |
| `R1` | local reasoning or proposal | never freshness-gated |
| `R2` | route/public network exchange | exact scoped evidence within R2 window |
| `R3` | negotiated disclosure, remote cognition, reversible effect | exact scoped evidence within R3 window |
| `R4` | delegation, irreversible or safety-critical effect | exact scoped evidence within R4 window |

Each action has a minimum tier. Declaring a safety-critical or remote action as
R0/R1 is rejected before the no-gate rule is considered. Therefore offline
local reading/reasoning remains usable without creating a downgrade path for
external authority.

## 3. Authority observation

`AuthorityFreshnessObservation` contains:

- exact FeedID or PermitCID scope;
- `AUTHORIZED_RELATIVE`, `REVOKED_RELATIVE`, `EXPIRED` or
  `STALE_OR_UNRESOLVED`;
- the accepted EventCID frontier where available; and
- a local monotonic observation tick.

The observation tick is local-private runtime state. It is not signed,
replicated or interpreted as network time. `from_feed` consumes the FEED-002
key-state reducer; `from_permit` consumes the CAP-004 permit view.

R2–R4 checks name the complete required-scope set. Missing, extra, duplicate or
wrong-scope evidence fails closed. Revoked evidence denies relative to its
frontier; unknown or stale evidence requests refresh. Neither outcome deletes
knowledge or declares an authority invalid outside that frontier.

## 4. TerrestrialInteractive/1

This named profile interprets caller ticks as local monotonic seconds and uses:

| Tier | Maximum local age |
|---|---:|
| `R2` | 3600 seconds |
| `R3` | 300 seconds |
| `R4` | 60 seconds |

These are an interactive deployment policy, not protocol constants for Mars,
delay-tolerant networks or isolated communities. Another node is not required
to share the same clock origin or profile.

## 5. TaskSpecificDtn/1

A DTN profile is never selected automatically from geography or connectivity.
It must be explicitly authorized against a current CAP-004 Permit and binds:

- non-zero profile and task commitments;
- one exact non-local action;
- PermitCID and its authority frontier;
- monotonic non-increasing R2/R3/R4 windows;
- an expiry contained within the Permit lifetime; and
- a PermitExecutionScope containing the task commitment.

At evaluation, action and task must match exactly, the profile and Permit must
remain active, and the same exact authority-scope evidence is still required.
Thus a longer DTN window is a bounded task decision, not a reusable “Earth is
offline” exception or global authority.

## 6. Boundaries

Freshness authorization does not:

- establish truth, correctness or encoding fidelity;
- make a KU wrong, unavailable or deletable;
- turn provider reachability into authority;
- prove absence of a newer unseen revocation;
- publish, execute, adopt or materialize anything;
- create reward or OBT; or
- introduce a Core DNA Gene or execution opcode.

## 7. Executable evidence

Five tests prove:

- R0/R1 local actions proceed without freshness evidence;
- terrestrial R2/R3/R4 enforce 3600/300/60-second local windows and exact
  scopes;
- revoked/unknown remain frontier-relative denial/refresh, never global
  liveness;
- risk understatement cannot bypass a gate; and
- a CAP-004-authorized DTN profile accepts only its exact task/action/window,
  while the same old evidence under `TerrestrialInteractive/1` requires refresh.

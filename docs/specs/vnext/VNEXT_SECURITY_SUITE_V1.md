# OneBrain vNext — Security Suite v1

> **Task:** `QA-005`  
> **Status:** Complete  
> **Executable gate:** [`onebrain-node::vnext_security_suite`](../../../src/onebrain-node/src/vnext_security_suite.rs)

## 1. Security outcome

The release gate executes six adversarial probes. A probe passes only when the
attack is rejected and the attempted path grants no semantic authority,
completion, fidelity or adoption.

| Probe | Adversarial case | Required outcome |
|---|---|---|
| `SESSION_TRANSCRIPT_REPLAY` | signed transcript mutation, capability insertion and authenticated-session replay | signature/transcript checks reject mutation; replay guard accepts once |
| `MERKLE_RIBLT_FALLBACK` | modified checkpoint inclusion sibling and speculative RIBLT enablement | Merkle proof fails; RIBLT remains default-off and cannot bypass its dependencies |
| `PARSER_EXPANSION_BOMB` | oversized canonical input, excessive compressed input, expanded output and expansion ratio | reject before unbounded parse/allocation/decompression |
| `PERMIT_TASK_REPLAY` | exact permit replay, exact task replay and same-ID task mutation | permit replay is idempotent; task replay/mutation never executes the backend |
| `SYBIL_CORRELATION` | 100 device/feed identities under one evidenced administrator and model pipeline | one independence group, never 100 fidelity votes |
| `PRIVATE_NEED_TAINT` | absent consent plus rare exact route token derived from private Need material | absent consent rejects; rare token is suppressed with local-private audit only |

## 2. Runtime resource boundary

`ku-net::vnext_resource_gate` is the codec-independent admission boundary for
compressed carrier payloads. It checks three independent ceilings before a
decompressor may allocate output:

- received compressed bytes;
- declared or codec-derived expanded bytes;
- maximum expansion ratio.

`CONTROL_V1` currently admits at most 1 MiB compressed, 4 MiB expanded and a
64:1 expansion ratio. These are versioned implementation limits, not claims
about knowledge value. Passing the gate establishes only resource admission;
the output must still stop at the admitted ceiling and pass canonical profile
validation.

## 3. Remote cognitive replay boundary

Remote task handlers use `TypedCognitiveExecutor::execute_once`. A local
`CognitiveTaskReplayGuard` binds each `task_id` to a deterministic commitment
over all execution-relevant fields. Exact replay returns `TaskReplay`; reuse of
the ID with different valid execution semantics returns
`TaskIdentityConflict`. Both outcomes occur before backend work. The guard does
not turn task identity into feed authority and does not publish or materialize
the result.

## 4. Fidelity and privacy interpretation

Sybil resistance counts evidenced correlation groups, not nodes, devices or
self-claimed independence. This is encoding-fidelity evidence only: it checks
whether a KU was encoded as described and does not vote that a KU is true or
false.

Private Need material remains local by default. Consent is explicit,
scope-bound and expiring; it is never inferred from connectivity or prior
activity. Generalization or suppression happens before route disclosure, and
the detailed taint audit remains local-private.

## 5. Evidence and limits

The executable gate is deterministic and contains no network-wide oracle. It
does not claim exhaustive cryptographic review, global Sybil elimination,
global completion, or safety of a future RIBLT implementation. Future codecs
must integrate the runtime expansion gate at their allocation boundary and add
codec-specific streaming/fuzz cases without weakening this profile.


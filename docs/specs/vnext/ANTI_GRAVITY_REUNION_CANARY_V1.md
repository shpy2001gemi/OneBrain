# Anti-Gravity Reunion Canary v1

> **Task:** `QA-001`  
> **Status:** Complete  
> **Gate:** M3 Anti-Gravity Reunion Slice

## 1. What this canary proves

The canary composes the vNext foundation contracts into one bounded offline-first scenario. Component A holds a private Receptor, QueryDefinition and StandingNeed. Component B creates two public KnowledgeAffordances while disconnected. Reconnection transfers only selector-eligible public objects; matching occurs locally at A and produces a private proposal.

“Anti-Gravity” remains the fictional narrative inherited from the corpus. Passing this canary makes no scientific truth claim and does not assert that any proposed mapping is useful, beneficial or correct.

## 2. Deterministic trace oracle

`onebrain-node::DeterministicReunionTrace` is a local canary oracle. It records a canonical set of phase, full-width subject CID and phase-scoped outcome code. Its digest excludes:

- wall-clock time and arrival order;
- bridge, carrier, route and seed identity;
- private NeedIR, StandingNeed, Receptor/Assembly/User identifiers;
- popularity, ranking, truth and benefit scores; and
- OBT or reward state.

Consequently one, two and five bridge paths yield the same semantic delivery and canary trace digest. Duplicate trace entries are idempotent. The oracle never claims network-global completeness.

## 3. Machine-checked exit matrix

| §15.4 | Executable evidence |
|---:|---|
| 1 | Test topology creates no seed, coordinator or quorum; the carrier/inbox explicitly grants no authority. |
| 2 | A's private query/StandingNeed and B's public Affordances are constructed before any reconciliation session. |
| 3 | One memory bridge, two file-bundle bridge paths and five memory bridge paths converge on the same sorted validated CID set and semantic/trace digests. |
| 4 | A same-CID/different-bytes payload variant is rejected by length/content binding and cannot overwrite the two validated store entries. |
| 5 | Public selector admits only `Public` Affordance objects; a LocalOnly selector fails; canonical outbound records are checked against private Query/Receptor/Assembly/StandingNeed identifiers. |
| 6 | A newly validated remote Affordance triggers exactly one active local StandingNeed and yields a proposal with explicit correspondence and satisfied typed-constraint observation. |
| 7 | Proposal quarantine is non-executable; importing it remains quarantine-to-quarantine; resolution stays `Open` after proposal and after materialization. |
| 8 | Authorized adoption with partial assessment produces `PartiallySatisfied`; a causally later satisfied candidate produces `SatisfiedRelative`. |
| 9 | Bundle, materialization command, resolution event and Use evidence are each replayed 1,000 times without duplicate durable side effects. |
| 10 | A concurrent `REOPEN` branch remains beside the satisfied adoption branch and the view becomes `Concurrent`. |
| 11 | The result batch validates an assessed frontier, `Partial` status, explicit limitation and continuation, and denies global completeness. |
| 12 | After sessions are dropped, accepted public object bytes remain independently decodable from the validated local store. |
| 13 | The complete suite has no OBT dependency; the trace oracle explicitly reports `requires_obt() == false`. |
| 14 | Selector, namespace and budget are reconciliation-context-bound; selector tampering fails context validation and private storage class compilation fails. |

## 4. Crash, partition and replay behavior

The StandingNeed is closed and reopened through the Redb backend before reunion. The reconciliation session is closed and reopened from a shared atomic journal before a 1,000-copy replay. Accepted payload identity survives restart; path-local invalid observations are not promoted into semantic authority.

Both components remain useful as local stores before reunion. After reunion, removing the carrier does not revoke accepted KU, materialized local Mapping or locally assessed Resolution evidence.

## 5. Authority boundaries

The only automated transition caused by reunion is remote-public-object admission followed by local proposal generation. Proposal import does not authorize materialization. Materialization does not authorize adoption. Signed Use evidence is deduplicated by EventCID and explicitly establishes neither truth, benefit nor reward.

No KU is classified as “wrong.” Corrupt or mismatched bytes fail encoding/content fidelity; the knowledge object itself is not assigned a global truth verdict.

## 6. Executable locations

- Trace oracle: `src/onebrain-node/src/vnext_reunion_canary.rs`
- Cross-pillar suite: `src/onebrain-node/tests/anti_gravity_reunion.rs`
- Reunion join: `src/ku-kql/src/vnext_reunion.rs`
- Frozen narrative corpus: `docs/specs/vnext/corpus/anti_gravity_v1.yaml`

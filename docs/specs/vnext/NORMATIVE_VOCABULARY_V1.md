# OneBrain vNext — Normative Vocabulary and Negative Assertions v1

> **Task:** `FND-002`  
> **Status:** Normative  
> **Depends on:** [Field Ownership Matrix v1](FIELD_OWNERSHIP_MATRIX_V1.md)  
> **Machine-readable registry:** [negative_assertions.yaml](negative_assertions.yaml)

## 1. Normative language

The key words **MUST**, **MUST NOT**, **SHOULD**, **SHOULD NOT** and **MAY** are requirements for vNext contracts. Vietnamese prose has the same force when it uses `bắt buộc`, `không được`, `nên`, `không nên` and `có thể`.

No unqualified term in this document creates network-wide truth, finality, completeness or authority.

## 2. Required qualifiers

| Ambiguous term | Canonical meaning | Required qualifiers | Forbidden interpretation |
|---|---|---|---|
| `valid` | Artifact passed a named structural/cryptographic/resource validator. | validator/schema/profile version | Proposition is true or useful. |
| `invalid` | Canonical bytes/CID/signature/schema/resource rule failed under a named validator. | failure code + validator version | “The knowledge is wrong.” |
| `verified` | A named verification procedure produced evidence. | what was verified, policy/tool/version, frontier | Universal truth/correctness. |
| `fidelity` | Encoding represents a named source with stated limitations. | source CID, encoding CID, policy, evidence frontier | Truth of the source proposition. |
| `complete` | A finite named operation has no known remaining work relative to its stated boundary. | boundary/selector, frontier/root, budget, operator/profile | Whole network or all human knowledge was searched/synchronized. |
| `global` | Reserved for quoted legacy input or ordinary non-protocol prose. | legacy adapter context when protocol-visible | Canonical query/sync scope. |
| `independent` | Not a canonical boolean. Use evidenced correlation dimensions/groups. | evidence dimensions, strengths and policy | Different NodeID/IP/model label proves independence. |
| `expired` | A lease/permit/run is unusable under a named local observation/policy. | record CID, generation, observation/frontier, profile | Knowledge disappeared or became false. |
| `adopted` | An authorized placement-scoped resolution event references a Mapping. | assembly lineage, revision, placement, event/frontier | Mapping was merely retrieved, ranked, stored or published. |
| `satisfied` | Acceptance profile is met relative to a placement, evidence frontier and policy. | placement, policy, frontier, status | Receptor is globally/permanently closed. |
| `published` | Object/event is placed in a stated disclosure namespace/store/feed. | namespace/disclosure class and publisher feed | Available everywhere, adopted, true or high-value. |
| `available` | A provider/path was observed and probed or has a usable lease under local policy. | provider/lease, observation time, reachability view | Provider is complete, authoritative or permanent. |
| `trusted` | A named policy assigns confidence/authority for a specific action. | policy, action/effect, evidence/frontier | General epistemic truth or permanent node rank. |
| `used` | A signed Use/Derivation event records a causal/exercise role. | event identity, task/context, signer/frontier | Benefit was proven. |
| `benefit` | Outcome/benefit evidence exists under a named attribution policy. | outcome, affected context, evidence, limitations, policy | Token reward is owed or the KU is true. |
| `deleted` | Local bytes were evicted/crypto-erased under local retention policy. | storage class, policy, audit/anchor | Global deletion or semantic nonexistence. |
| `island` | Informal observation of a temporarily connected component. | reachability view/interval if used analytically | Protocol identity, epoch, leader or authority. |
| `final` | Prohibited for open-network knowledge/query/sync state unless naming a closed finite proof domain. | closed domain and proof | Irreversible network-wide consensus. |

## 3. Canonical state vocabulary

### 3.1 Receptor ResolutionView

Canonical derived states are:

- `OPEN`
- `PARTIALLY_SATISFIED`
- `SATISFIED_RELATIVE`
- `WAIVED`
- `DEFERRED`
- `CONCURRENT`

`CLOSED`, `COMPLETE` and `FINAL` are not Receptor state tokens.

Canonical resolution event kinds are:

- `ADOPT_BINDING`
- `REVOKE_ADOPTION`
- `WAIVE`
- `REOPEN`
- `DEFER`

Event kinds and derived states MUST NOT share one enum.

### 3.2 Query/run completion

Canonical terminal reasons for one scoped run are:

- `SATISFIED_RELATIVE`
- `EXHAUSTED_RELATIVE`
- `BUDGET`
- `DEADLINE`
- `CANCELLED`
- `ERROR`

Every result MUST include or reference its `CoverageStatement`. A zero-result response is never evidence of global absence.

### 3.3 Reachability

`Standalone`, `ComponentReachable` and `PathLimited` MAY be local derived display modes. They are not protocol entities. The source `ReachabilityView` includes observation interval, peer digest, selector frontiers, carrier paths, budgets and limitations.

There is no canonical `IslandID`, `IslandEpoch`, island leader or island consensus status.

### 3.4 Encoding fidelity

Canonical assessment states are scoped, for example:

- `SELF_ATTESTED`
- `PARTIALLY_CORROBORATED`
- `FIDELITY_CORROBORATED_RELATIVE`

`FULL` is not a canonical vNext state. It may appear only as quoted legacy input parsed by `LegacyAdapter` and downgraded to `LegacyEncodingClaim`.

### 3.5 Query scope

`GLOBAL` is not a canonical vNext scope. Use a named boundary such as:

- local validated store;
- explicit feed set;
- selector-scoped reachable peers;
- reachable best effort with frontier and limitations;
- a closed fixture or archive manifest.

## 4. Knowledge and truth language

OneBrain stores and connects claims, observations, models, experiences, procedures, counterclaims, refutations and questions. It does not assign an existence-level boolean `KU_CORRECT/KU_WRONG`.

The following distinctions are mandatory:

| Question | Correct owner |
|---|---|
| Are the bytes canonical and does CID match? | artifact validator |
| Is the signature/authority valid at an observed frontier? | crypto/authorization evaluator |
| Does an encoding faithfully represent its source? | fidelity evidence/assessment |
| Does a claim fit this task/context? | KQL constraint/Mapping policy |
| Was it used? | signed Use/Derivation evidence |
| Did it produce benefit? | Outcome/Benefit evidence + attribution policy |
| Is the proposition true? | Not a single network reducer; preserve evidence and opposition. |

A refutation, failed approach or minority claim may be valuable by helping a task, exposing a boundary, producing a counterexample or enabling a later derivation.

## 5. Operation-specific language

### 5.1 Materialize versus adopt

- `BindingProposal` means an ephemeral candidate in Quarantine/runtime state.
- `MaterializeMappingCommand` means storing validated MappingKernel + MappingEnvelope at a durable boundary.
- `PUBLISH` means exposing a stored Mapping under a disclosure policy.
- `ADOPT_BINDING` means an authorized resolution event for one exact Assembly Placement.
- Adoption does not automatically imply `SATISFIED_RELATIVE`; the reducer may return `PARTIALLY_SATISFIED` or `CONCURRENT`.

Retrieval, delivery, exposure, ranking, model score and validation alone MUST NOT be called materialization or adoption.

### 5.2 Reconciliation

Allowed language:

> Session `S` reconciled selector `X` with peer set `P` to inventory roots/frontiers `R` under budget `B`.

Forbidden language:

> This node is fully synchronized with OneBrain.

Probabilistic structures may say `likely equal` only as a routing/fast-path hint. Completion requires the exact deterministic contract for the named selector/session.

### 5.3 Provider and lease

`ProviderLease expired` means the record is no longer usable for current routing under the named local policy. It does not remove the provider principal, delete the KU, revoke custody or prove the provider lacks the content.

Replay of the same ProviderLease CID MUST NOT renew it. Only a valid higher generation may renew the tuple.

### 5.4 Checkpoint and GC

`covered by checkpoint` requires inclusion, consistency and reducer-effect proof. Missing proof yields `UNRESOLVED`, not covered.

`evicted locally` is preferred over `deleted` for public immutable KU payload. A later peer may reintroduce the same CID.

## 6. Legacy aliases

| Legacy input | vNext inbound interpretation | vNext outbound rule |
|---|---|---|
| `GLOBAL=5` | reachable best effort with unknown/partial coverage and legacy limitation | Only an explicitly negotiated legacy adapter may emit legacy query syntax; response remains scoped/partial. |
| `FULL=3` | `LegacyEncodingClaim` with original wire provenance | MUST NOT emit `FULL=3`; legacy outbound encoding status is at most `PART=2`. |

Legacy aliases MUST NOT appear in canonical vNext enums, canonical serialized objects, reducer states or new API contracts.

## 7. Negative assertions

Each assertion has a stable ID mirrored in `negative_assertions.yaml`.

### 7.1 Knowledge/object assertions

- `NEG-KU-001`: CID match MUST NOT be described as truth verification.
- `NEG-KU-002`: Lack of UseEvent MUST NOT delete, reject or label a KU as wrong.
- `NEG-KU-003`: Popularity, trust, route distance or OBT balance MUST NOT be a KQL eligibility gate.
- `NEG-KU-004`: Opposing or refuted knowledge MUST NOT be removed merely because another branch is preferred.

### 7.2 Receptor/Mapping assertions

- `NEG-REC-001`: ReceptorDefinition MUST NOT contain current resolution, budget, rank or candidate list.
- `NEG-REC-002`: Resolution MUST NOT be addressed only by ReceptorDefinitionCID; exact assembly revision and placement are required.
- `NEG-MAP-001`: Retrieval/ranking/delivery MUST NOT materialize a Mapping.
- `NEG-MAP-002`: Materialization/publish MUST NOT adopt a Mapping into an Assembly.
- `NEG-MAP-003`: `ADOPT_BINDING` MUST NOT force `SATISFIED_RELATIVE` when acceptance is only partial or concurrent.
- `NEG-MAP-004`: A BindingProposal MUST NOT create an active OBKG edge or tool/profile side effect.

### 7.3 KQL/privacy assertions

- `NEG-KQL-001`: Full private KnowledgeNeedIR MUST NOT be an OBP payload.
- `NEG-KQL-002`: Zero results MUST NOT be expressed as global absence.
- `NEG-KQL-003`: `GLOBAL` MUST NOT be a canonical vNext query scope.
- `NEG-KQL-004`: Private ClaimEnvelope/StandingNeed MUST NOT enter outbound public inventory.
- `NEG-KQL-005`: RouteNeedSketch MUST NOT carry stable Receptor/Assembly/Need/User/Node identity or raw KQL.

### 7.4 Network/identity assertions

- `NEG-NET-001`: NodeID MUST NOT be truncated to `u64` for identity, clock, ACK, watch or evidence deduplication.
- `NEG-NET-002`: Seed, bridge, relay or provider MUST NOT gain content/author authority by forwarding data.
- `NEG-NET-003`: There MUST NOT be a canonical IslandID, IslandEpoch, island leader or global component authority.
- `NEG-NET-004`: Bloom/XOR/RIBLT output MUST NOT establish completion without exact root verification/fallback.
- `NEG-NET-005`: Transport handshake MUST NOT link every namespace-scoped feed by default.
- `NEG-NET-006`: Same CID with different bytes MUST NOT overwrite accepted local bytes.

### 7.5 Fidelity/capability assertions

- `NEG-FID-001`: Different NodeID/IP/model label MUST NOT be counted as attester independence.
- `NEG-FID-002`: Encoding fidelity MUST NOT be described as proposition truth.
- `NEG-FID-003`: `FULL` MUST NOT be a canonical vNext fidelity state.
- `NEG-CAP-001`: CapabilityOffer/conformance MUST NOT grant authority.
- `NEG-CAP-002`: Remote result verification MUST NOT automatically materialize, publish, adopt or execute it.
- `NEG-CAP-003`: A child delegation MUST NOT expand the parent's effects, purpose, budget, lifetime or onward-delegation right.

### 7.6 Provider/checkpoint/retention assertions

- `NEG-PRO-001`: Replaying the same lease CID MUST NOT reset local lease age.
- `NEG-PRO-002`: Provider DHT response MUST NOT claim a complete provider set after sampling/eviction.
- `NEG-PRO-003`: Provider availability MUST NOT imply custody, correctness or authority.
- `NEG-GC-001`: A checkpoint MUST NOT suppress an event without required proof.
- `NEG-GC-002`: Missing proof/head MUST NOT be reduced as add-wins, remove-wins or covered; it is unresolved.
- `NEG-GC-003`: Local payload eviction MUST NOT be represented as global semantic deletion.

### 7.7 PoMV/OBT assertions

- `NEG-POMV-001`: QueryHit, retrieval or exposure MUST NOT count as Use.
- `NEG-POMV-002`: UseEvent alone MUST NOT be described as proven benefit.
- `NEG-POMV-003`: PoMV/Benefit assessment MUST NOT vote on proposition truth.
- `NEG-OBT-001`: OBT failure, balance or reward policy MUST NOT block KU publish, preservation, KQL, reconciliation, Mapping adoption or fidelity evidence.

## 8. Conformance expectations

Until `FND-004/FND-005` provide executable vectors and runners, review uses these manual gates:

1. public enums contain no unqualified `GLOBAL`, `FULL`, `CLOSED`, `FINAL`, `INDEPENDENT` or bare `COMPLETE`;
2. API/status prose includes required boundary/policy/frontier qualifiers;
3. semantic object schemas contain no availability or authority fields;
4. availability records contain no truth/fidelity/adoption effects;
5. every negative assertion maps to at least one future invalid vector/property/security test in the traceability matrix.

## 9. Acceptance checklist

- [x] `complete`, `global`, `independent`, `expired`, `verified` and `adopted` have scoped definitions.
- [x] Artifact validity, encoding fidelity, task applicability, use, benefit and proposition truth are separated.
- [x] Canonical Receptor, query and fidelity tokens exclude legacy/global-finality aliases.
- [x] Negative assertions cover semantic, network, privacy, authority, provider, GC, PoMV and OBT boundaries.
- [x] The registry uses stable IDs suitable for future automation.


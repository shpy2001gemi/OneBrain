# OBP Reconciliation Protocol Profile v1

> **Task:** `OBP-003`  
> **Capability:** `obp/reconcile/1`  
> **Schema:** `reconciliation-message` (`7`), major `1`  
> **Status:** Normative vNext foundation contract

## 1. Purpose and boundary

This profile defines the canonical message contract used by two authenticated OneBrain nodes to compare one bounded inventory selector. It covers hello/profile, selector offer, inventory summary, diff ranges, manifest, receipt, progress, abort and resume records.

It does not define transport, routing, payload acceptance, semantic adoption, feed authority, knowledge truth, network-wide completeness or reward. Those remain separate layers. In particular:

- a manifest announces candidate immutable bytes; it is not a KU mapping or adoption;
- a `ValidatedStored` receipt only reports protocol/storage handling;
- `SelectorComplete` means the current exchange has no known remaining work under its exact selector, frontier, summary method and budget;
- no message proves global closure, benefit, truth or OBT entitlement;
- an isolated component can reconcile the knowledge it can reach without a seed, leader or global membership view.

## 2. Negotiation

The session MUST first negotiate the stable capability ID derived from the canonical UTF-8 name `obp/reconcile/1`. Reconciliation Hello MUST carry:

- profile family `0x4f425052` (`OBPR`), major `1`, minor `0`;
- the exact negotiated capability ID;
- a fresh reconciliation nonce.

Profile or capability mismatch is rejected. Legacy TCP/JSON message types cannot express this schema and cannot be upgraded implicitly.

## 3. Immutable reconciliation context

Every message carries the same `ReconciliationContext` and its domain-separated binding digest:

| Field | Meaning |
|---|---|
| `authenticated_transcript` | Full transcript hash from the authenticated session. |
| `selector` | Full-width content-addressed SelectorCID. |
| `namespace` | Hiding NamespaceCommitment; never plaintext namespace. |
| `disclosure` | Exact disclosure class for this exchange. `LocalOnly` is forbidden on the wire. |
| `summary_method` | Correctness summary method; v1 requires `RadixForest256V1`. |
| `budget` | Per-exchange ceilings for summary nodes, diff ranges, manifests and individual payload bytes. |
| `resume_mode` | Either disabled or `BoundTokenV1`. |

The binding digest is `ManifestCID(canonical(context))`. A peer MUST compare the decoded context with the locally negotiated expected context. Recomputing a digest after changing selector, namespace, disclosure, method, budget or resume mode does not make the message acceptable because expected-context comparison still fails.

This binding is channel/session scope, not knowledge authority.

## 4. Message families

| Wire ID | Message | Required role |
|---:|---|---|
| 20 | `Hello` | Confirm profile, capability and exchange nonce. |
| 21 | `SelectorOffer` | Offer selector-scoped inventory root, canonical lane set and optional known checkpoint frontier. |
| 22 | `InventorySummary` | Send bounded radix summary nodes and selector-local leaf count. |
| 23 | `Diff` | Identify deterministic divergent lane/prefix ranges. |
| 24 | `Manifest` | Announce full CID, kind and canonical length before any payload. |
| 25 | `Receipt` | Report validated-storage, already-present, invalid or deferred-budget handling. |
| 26 | `Progress` | Report local phase/count/remaining upper bound and optional continuation token; manifest-batch completion remains distinct from selector completion. |
| 27 | `Abort` | Stop with a typed code, retry flag and progress commitment without free-text leakage. |
| 28 | `Resume` | Continue at the exact next sequence using a bound token. |

The authoritative inventory lanes are `Object`, `Event` and `MappingKernel`; all identifiers remain full width. Summary nodes and diff ranges use canonical 0–256-bit prefixes. Unused suffix bits MUST be zero.

## 5. Resume contract

`BoundTokenV1` contains:

- the exact reconciliation binding digest;
- a checkpoint/progress digest;
- the exact next sequence;
- 32 opaque bytes owned by the future persisted session implementation.

A token is invalid if resume was not negotiated, its binding differs, or a Resume message sequence does not equal `next_sequence`. The token itself grants no authority and promises no durable state. Crash-safe journaling, MAC/key ownership and retry behavior are owned by `OBP-005`.

## 6. Resource and canonicalization rules

- Canonical restricted CBOR is mandatory; decode then re-encode MUST reproduce identical bytes.
- Overall parser allocation is bounded by `manifest/1` (4 MiB).
- Control messages are at most 256 KiB.
- Inventory summary is at most 1 MiB.
- Manifest and receipt are at most 4 MiB.
- Summary nodes, diff ranges and manifest/receipt entries are each capped at 65,536 and additionally narrowed by the negotiated budget.
- Individual declared payload length is non-zero and at most 1 MiB, additionally narrowed by `budget.max_payload_bytes`.
- Set-like arrays are strictly sorted and duplicate-free.
- Unknown enum/wire IDs, non-canonical prefixes, zero/excess budgets and unbound tokens are rejected.

Optional RIBLT or future summary accelerators require a new negotiated capability and cannot replace radix-forest correctness.

## 7. Privacy, partitions and completion

Private StandingNeeds, ClaimEnvelopes, local AI traces and Vault objects MUST NOT enter this schema. Only a disclosure-compatible selector projection may be reconciled. A one-way or delayed carrier can preserve these exact canonical messages; lack of response remains `unknown`, never `false`.

After partition reunion, either component may resume or start a fresh selector-scoped reconciliation. Multiple bridges may repeat messages; this schema gives them stable identities and bindings, while deterministic dedup/convergence is owned by `OBP-004` and `OBP-006`.

## 8. Executable evidence

- Codec and negative tests: `src/onebrain-protocol/src/reconciliation_codec.rs`
- Logical types and stable wire IDs: `src/onebrain-protocol/src/types.rs`
- Frozen Hello vector: `src/test-vectors/vnext/obp/reconcile-v1.json`
- CI: `.github/workflows/vnext-foundation.yml`

The tests cover all nine message families, canonical round trips, context-field tampering, binding/token/sequence tampering, resource limits, canonical prefixes/order, the `LocalOnly` firewall and negative authority/global-completion assertions.

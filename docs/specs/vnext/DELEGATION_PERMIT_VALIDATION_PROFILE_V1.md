# Delegation Permit Validation Profile v1

> **Task:** `CAP-004`  
> **Status:** Complete  
> **Depends on:** `CAP-001`, `FEED-002`

## 1. Purpose

Capability availability is not authority. This profile defines the first executable authority gate: a canonical permit must be signed by a feed authorized for its issuer Actor at the caller's accepted key-state frontier, and every child permit must be no more permissive than its admitted parent.

`foundation::capability_permit` has no Offer, trust-score or conformance-report input. Consequently none of those claims can be converted into permission by this validator.

## 2. Signed issuer claim

`SignedDelegationPermit` binds the complete CAP-001 permit body and signer FeedID under the domain-separated Permit signature context. Authentication requires:

- canonical body and wrapper round trip;
- a valid Ed25519 signature from the exact validated FeedInception;
- an `AuthorizedRelative` decision from the caller's local `KeyStateReducer`; and
- equality between the actor in that decision and the permit issuer.

Missing delegation evidence is `IssuerAuthorityUnresolved`, not false or globally invalid. A locally revoked feed is quarantined. The resulting `AuthenticatedDelegationPermit` is still only an authenticated claim and grants no authority until parent attenuation and lease admission succeed.

## 3. Root and child admission

A root permit has no parent and is admitted from its authenticated issuer claim. A child must reference a parent already admitted in the same local permit view. The child issuer must equal the parent executor, and the parent must explicitly allow onward delegation.

The child is admitted only if all dimensions satisfy:

| Dimension | Attenuation rule |
|---|---|
| Capability Definition | exact equality |
| Input commitments | child set is a subset of parent set |
| Allowed effect classes | child set is a subset of parent set |
| Purpose | exact equality in v1; no unproven purpose hierarchy |
| Budget | component-wise `child <= parent` |
| Lifetime | child interval is contained in parent interval |
| Onward delegation | cannot be enabled unless the parent enables it |
| Retention | equality or conservative bottom, as described below |

This is effect-set intersection by admission: a delegator must encode the already-intersected child set; the validator never silently adds an effect from either side.

## 4. Retention v1 fail-closed rule

CAP-001 v1 contains `DeleteAfterTask`, `RetainUntilExpiry` and `NoTraining`. Duration and training are orthogonal, so these three values do not form a sound total ordering and cannot represent every intersection.

Profile v1 therefore treats `DeleteAfterTask` as a conservative bottom and the other two values as incomparable. A child may retain the exact parent rule or attenuate to `DeleteAfterTask`; any other transition is rejected. A later version should replace this with a product policy such as `{retention_ceiling, training_permission}` rather than inventing an unsafe ordering.

## 5. Replay, time and partitions

PermitCID is derived from the canonical body, which includes a non-zero issuer nonce. Exact replay is idempotent. Reusing the same issuer nonce for a different PermitCID is rejected. Parent absence is unresolved, so a disconnected node can retain the child claim and retry after receiving its parent.

Lease checks use an explicit caller-supplied local monotonic tick; the permit claims no Earth time or global live-state oracle. `authority_at` returns only frontier-relative authority and becomes expired at `expires_at`.

## 6. Interpretation boundary

An admitted permit authorizes only the encoded capability scope. It does not establish:

- correctness, truth or encoding fidelity of any KU;
- current provider availability;
- attester independence;
- automatic publication, adoption or durable materialization outside its effect set; or
- benefit, value, reward or OBT entitlement.

## 7. Executable evidence

Tests prove:

- exact signature/feed/issuer authority binding at an accepted frontier;
- strict child attenuation and deterministic local expiry;
- fail-closed rejection for capability, input, effect, purpose, budget, retention and lifetime expansion;
- onward-delegation denial and unresolved-parent behavior; and
- exact permit replay idempotence.

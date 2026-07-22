# Provider Lease and Retirement Profile v1

> **Task:** `DHT-001`  
> **Status:** Complete  
> **Depends on:** `FEED-001`, `FEED-002`, `REV-001`

## 1. Purpose

Provider records are signed availability hints for decentralized discovery.
They replace arrival-order overwrite with an immutable multi-provider,
multi-generation reducer that continues to work through partitions and reunion.

They do not establish knowledge identity, correctness, custody, completeness or
global reachability.

## 2. ProviderTuple

The operational identity is:

```text
ProviderTuple(index_key, provider_principal, offer_kind)
```

`provider_principal` is a full FeedID or ActorID. `offer_kind` is typed as
KnowledgeObject, Assembly, Capability, QueryMailbox or CheckpointArchive. Two
providers for one index therefore occupy different tuples and cannot overwrite
each other.

## 3. Signed ProviderLease

Schema ID `5` carries a canonical signed lease with:

- exact tuple;
- SelectorRoot or ContentRoot subject;
- coarse capability classes and bounded endpoint references;
- advisory issued time;
- bounded local-observation duration;
- positive generation; and
- exact FEED-002 key-state frontier reference.

The signature is domain-separated as `provider-lease/1`; its full canonical
bytes produce LeaseCID. Feed principals must equal the signer FeedID. Actor
principals require that signer feed to be authorized for the exact actor at the
referenced local frontier. Unresolved, revoked-relative or wrong-frontier
signers fail closed.

Advisory issued time is metadata only. It is not a synchronized expiry clock.

## 4. Local LeaseObservation

`LeaseObservationStore.first_seen_monotonic(LeaseCID)` is local-private runtime
state. The first observation starts the lease duration. Re-observing the same
CID is an idempotent replay and never changes first-seen time. The value has no
signed/network encoder and must not be copied as provider authority.

Only a valid higher-generation lease has a new CID and may renew the tuple.

## 5. Signed ProviderRetire

Retirement is a separately signed `provider-retire/1` event-domain record with:

- exact ProviderTuple;
- positive `retire_through_generation`;
- exact key-state frontier; and
- non-zero replay nonce.

The reducer retains all exact records at the maximum retirement floor,
including same-floor conflicts. Every lease generation at or below the floor
is suppressed even if the retirement arrived first. Old lease replay cannot
resurrect it. A later valid generation above the floor may advertise
availability again.

## 6. ProviderLeaseMap

For each tuple, the reducer retains every LeaseCID grouped by generation and
every retirement EventCID grouped by floor. Merge is immutable union:

- lower generation remains historical;
- same-generation different CIDs remain explicit conflicts;
- the maximum observed generation is the only current generation considered;
- expiry at high water never falls back to an older generation; and
- retirement floor is the exact maximum, not probabilistic evidence.

Active lookup additionally requires a local first-seen observation whose
exclusive duration has not elapsed. Lookup across an index returns all active
provider tuples in deterministic order.

## 7. Boundaries

Provider availability does not:

- prove that content is present, correct or useful;
- delete or revoke a KU when a lease expires/retires;
- establish global absence when no provider is visible;
- grant execution, feed or knowledge authority;
- create benefit, reward or OBT; or
- introduce a Core DNA Gene or execution opcode.

No active lease means only “no currently usable hint in this local view”.

## 8. Executable evidence

Five tests prove:

- two providers under one index never overwrite;
- same-generation lease conflicts remain present without arrival winner;
- exact replay never renews local age;
- retire-before-lease and same-floor conflicts preserve an exact high-water
  floor with no resurrection; and
- revoked-relative or wrong-frontier signers cannot create availability.

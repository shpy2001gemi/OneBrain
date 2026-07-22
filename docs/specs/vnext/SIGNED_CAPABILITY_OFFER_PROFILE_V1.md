# Signed Capability Offer Profile v1

> **Task:** `CAP-003`  
> **Status:** Complete  
> **Depends on:** `CAP-001`, `CAP-002`, `FEED-002`

## 1. Purpose

A Capability Offer is a short-lived, signed availability claim: a provider says that one implementation (or coarse implementation class) may currently serve one Capability Definition under bounded privacy and resource modes. It is neither a delegation permit nor evidence that the capability is correct, trustworthy, independent or useful.

`foundation::capability_offer` binds the complete CAP-001 offer body, signer feed and signature into canonical bytes and a domain-separated `LeaseCid`. The reducer remains deterministic across partitions, replay and reunion.

## 2. Signer and provider binding

Profile v1 accepts only a `Feed` provider whose FeedID exactly equals the validated FeedInception signer. A different feed is rejected. An `Actor` provider fails closed until a later profile supplies and validates an explicit delegated-feed proof; an actor label alone cannot authorize a feed.

The Ed25519 signature covers:

- provider principal;
- CapabilityDefinition ObjectCID;
- manifest ObjectCID or coarse implementation-class CCID;
- privacy modes and coarse resource buckets;
- self-claimed correlation hint and route/carrier handles; and
- `not_before`, `expires_at` and monotonic generation.

Decoding validates canonical form, key/signature, signer/provider equality and the full-body round trip before returning a `ValidatedCapabilityOffer`.

## 3. Identity and expiry

The stable reducer identity is `(provider, capability_definition, implementation_selector)`. Route handles and the self-claimed correlation hint are mutable availability metadata, not identity, authority or attester-independence evidence.

Lease evaluation uses an explicit local monotonic tick supplied by the caller. The wire object does not claim global time. A lease is active only when `not_before <= local_tick < expires_at`.

## 4. Generation high-water reducer

For each stable identity, the reducer retains every validated record grouped by generation and OfferCID:

1. a generation higher than the current high-water advances it;
2. a lower generation is retained as stale history but can never become active again;
3. distinct records at the same high-water generation are retained as a conflict set; and
4. an exact CID replay is idempotent.

Only records at the highest generation are eligible for the active view. Therefore expiry of a newer generation does not resurrect an older, longer-lived offer. Conflicts converge independently of arrival order; the reducer never invents an arrival-order winner.

This rule is partition-safe: isolated networks can retain and use locally active offers, then merge all signed generations and conflicts when connectivity returns without a central clock or authority.

## 5. Interpretation boundary

A validated or active Offer proves only that the signer made the encoded availability claim. It does not establish:

- permission or delegated authority to run a task;
- semantic/scientific correctness or encoding fidelity;
- correlation independence, merely because a hint differs;
- conformance beyond separately referenced evidence;
- adoption, publication, materialization or side-effect permission; or
- benefit, value, reward or OBT entitlement.

Remote execution still requires a separately validated, attenuated Permit (`CAP-004`).

## 6. Executable evidence

Tests prove:

- signature and LeaseCID bind the complete canonical offer;
- signer feed must exactly match the provider feed;
- a replayed stale generation cannot resurrect after the high-water offer expires;
- same-generation conflicts converge without arrival-order selection; and
- validated offers grant neither authority nor fidelity-group status.

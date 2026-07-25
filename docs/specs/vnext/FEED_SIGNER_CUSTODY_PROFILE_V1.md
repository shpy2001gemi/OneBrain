# OneBrain vNext - Feed Event Signer Custody Profile v1

> **Work package:** `DR-P1.2`
> **Status:** Executable production boundary - frozen 2026-07-26
> **Code:** `ku_core::foundation::{FeedEventSigner, ProvenFeedEventSigner}`

## 1. Scope and identity separation

This profile separates feed-event signing from private-key storage. A caller
MAY implement `FeedEventSigner` with an operating-system keystore, HSM,
hardware token, or remote signer. OneBrain core and product runtimes receive
only the public key and signing operation.

The following identity domains remain independent:

| Domain | Key owner | Signing boundary | Authority conveyed |
|---|---|---|---|
| Transport `NodeId` | Node operator | `SessionIdentitySigner` | Session authentication only |
| `ActorId` | Actor/recovery authority | Actor-root proof flow | Delegation root only |
| `FeedId` | One namespace-scoped feed generation | `FeedEventSigner` | Event authorship only |

A transport-authenticated node does not gain feed or Actor authority. A feed
signature does not grant capability authority, establish truth, or authorize
reward. Implementations MUST NOT reuse a NodeID or Actor root signer as the
feed signer.

## 2. Custody boundary

The complete boundary is:

```text
public_key() -> Ed25519PublicKey
sign_feed_event(message) -> Ed25519Signature | unavailable
```

Private-key export, seed export, path discovery, key-file fallback, signer
selection, and authority decisions are not part of the interface.

`ed25519_dalek::SigningKey` implements this boundary only as a compatibility
software signer for tests and local development. Merely using that adapter is
not evidence of production or non-exportable custody. Production runtimes MUST
inject one custody-backed implementation and MUST surface its failure without
consulting a file key or alternate signer.

## 3. Proof of possession

`ProvenFeedEventSigner::prove_for_public_key` performs these operations in
order:

1. Read the advertised public key.
2. Compare it to the exact `FeedInception.feed_public_key`.
3. Parse it as an Ed25519 public key.
4. Ask the same signer to sign the domain-separated challenge
   `onebrain:vnext:feed-event-signer-possession:1\0 || public_key`.
5. Verify the proof under that public key.

Public-key mismatch therefore fails before a sign operation. Invalid key,
unavailable signer, or invalid proof returns a stable failure and produces no
proof handle. There is no retry against another signer.

Every later event signature produced through the proof handle is verified
again. This catches a remote or hardware signer that becomes unavailable,
changes key, or returns a malformed/wrong signature after initialization.

## 4. Event construction ordering

`KnowledgeEventEnvelope::sign` proves the signer and delegates to
`sign_with_proven_signer`. The core event boundary MUST perform:

1. bounded field validation;
2. exact `author_feed` binding;
3. exact signer-public-key to `FeedInception.feed_public_key` binding;
4. unsigned canonical encoding;
5. domain-separated signing;
6. returned-signature verification.

The public-key checks occur before canonical encoding. A malformed event
paired with a wrong signer therefore returns
`EVENT_AUTHOR_KEY_MISMATCH`, not a later canonical error.

Callers that may persist, publish, enqueue, invoke an adapter, or perform
another external side effect MUST create the proof handle at the beginning of
the operation and pass it to `sign_with_proven_signer`.

## 5. Integrated side-effect boundaries

- `LocalObservationIntake::ingest` proves the feed signer before writing raw
  source, payload, or event bytes to the Private Vault and before invoking the
  observation adapter.
- `PublicUseEvidencePublisher::publish_confirmed` proves the feed signer before
  opening a write transaction, allocating a feed sequence, or storing a
  publication.

An invalid request or denied consent may fail before proof because neither
path performs a side effect. Once proof begins, any signer failure is terminal
for that operation and cannot trigger a fallback.

## 6. Stable failures

| Code | Meaning |
|---|---|
| `FEED_SIGNER_PUBLIC_KEY_INVALID` | Advertised bytes are not an Ed25519 public key |
| `FEED_SIGNER_PUBLIC_KEY_MISMATCH` | Advertised key differs from the expected feed key |
| `FEED_SIGNER_UNAVAILABLE` | The selected signer failed the requested operation |
| `FEED_SIGNER_PROOF_INVALID` | Possession challenge signature did not verify |
| `FEED_SIGNER_SIGNATURE_INVALID` | A later event signature did not verify |
| `EVENT_AUTHOR_FEED_MISMATCH` | Event FeedID differs from the validated inception |
| `EVENT_AUTHOR_KEY_MISMATCH` | Event signer differs from the validated inception |

Failure strings returned by an external signer are not copied into the stable
contract. They may contain provider-specific or sensitive custody details.

## 7. Executable evidence

- Valid proof and event signatures are verified under the advertised feed key.
- Public-key mismatch fails before the signer is called.
- Wrong proof and unavailable remote signer fail closed after exactly one
  selected-signer call.
- Wrong signer wins over a deliberately later canonical duplicate error,
  proving feed/public-key binding precedes canonical encoding.
- Wrong signer leaves the Private Vault empty; retry with the correct signer
  stores every artifact for the first time.
- Wrong signer creates no Public UseEvidence publication and consumes no feed
  sequence; retry with the correct signer starts at sequence zero.
- Existing feed-event canonical bytes and verification rules remain unchanged.

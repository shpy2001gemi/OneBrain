# OneBrain vNext — Authenticated Session Profile v1

> **Task:** `NET-001`  
> **Status:** Executable session contract — frozen 2026-07-20  
> **Schema owner:** [`onebrain-protocol::session_codec`](../../../src/onebrain-protocol/src/session_codec.rs)  
> **Verification:** [`ku-net::vnext_session`](../../../src/ku-net/src/vnext_session.rs)

## 1. Three-message handshake

The v1 session handshake is canonical `Hello → Welcome → Finish`:

- `Hello` binds the carrier-provided 256-bit transport/channel binding,
  initiator nonce, full NodeID, Ed25519 public key, ordered profile preferences,
  capability set and optional selective feed-proof references.
- `Welcome` binds the exact signed Hello transcript, a distinct responder nonce,
  responder principal, strongest common profile, exact capability intersection
  and optional responder feed proofs.
- `Finish` signs the complete Hello+Welcome transcript with the initiator key.

All three wire records and signature preimages are owned by
`onebrain-protocol`; carriers do not create parallel enums or IDs.

## 2. Principal and transcript binding

The session NodeID is a full 256-bit role-typed identity derived with a
domain-separated hash of the Ed25519 public key. Verification rejects a key that
does not reproduce the claimed principal. Signatures cover canonical unsigned
records; the transport binding and both nonces are therefore inside the signed
transcript.

The transport binding must come from an authenticated carrier exporter or an
equivalent channel transcript. A different binding, key, nonce relationship or
transcript is rejected. The executable runtime now performs this exchange over
real QUIC/TLS 1.3 listeners. The in-memory harness remains a deterministic test
carrier; the old TCP/JSON demo remains isolated under `legacy` and is not a
vNext authenticated carrier.

## 3. Negotiation and downgrade defense

The initiator orders profiles strongest/preferred first. The responder must
select the first common profile. Verification recomputes that result and the
exact canonical capability intersection. Even a correctly signed Welcome is
rejected if it selects a weaker common profile or strips/adds capabilities.

## 4. Namespace-private feed disclosure

The default Hello and Welcome contain zero feed proofs. A feed appears only as
a capability-scoped `SelectiveFeedProof` carrying exact FeedID, namespace
commitment and immutable proof reference, and only if its capability is in the
negotiated set.

The handshake signature binds that selective reference but does not replace
feed-proof validation or grant actor/content authority. `SessionFeedEvidence`
has an executable `grants_authority() == false` invariant. Unrelated
namespace-scoped feeds are not disclosed or linkable through the default
handshake.

## 5. Replay and seed independence

The authenticated session ID binds full transcript, transport binding and
responder principal. `SessionReplayGuard` accepts it once and rejects replay.
No seed, rendezvous server, relay or wall-clock claim participates in session
authority; a seed can at most help peers discover an address before this
handshake.

## 6. Executable evidence

Tests cover the complete handshake, full-width distinct principals, exact
profile/capability negotiation, transport MITM, key mismatch, transcript
tampering, nonce reuse, signed downgrade, signed capability stripping, replay,
default namespace unlinkability and non-authoritative selective feed evidence.

## 7. Private-key custody

Handshake construction depends on the
[`SessionIdentitySigner`](NODE_IDENTITY_KEY_CUSTODY_PROFILE_V1.md) boundary,
not on exportable private-key bytes. Production embedders can inject an OS
keystore, HSM, hardware token, or remote signer. The signer must prove
possession of its advertised Ed25519 public key before the runtime creates
storage or binds a listener; failure is terminal for startup and never falls
back to another identity.

# Progressive Disclosure Capsule v1

> **Task:** `SEC-003`  
> **Status:** Complete  
> **Depends on:** `SEC-002`, `CAP-004`

## 1. Purpose

`DisclosureCapsuleV1` progressively reveals authorized private material to one
recipient. It is an operational encrypted message, not a KnowledgeObject and
not a publication path.

The caller supplies a 256-bit session key obtained from an authenticated
recipient negotiation such as NET-001. This profile deliberately does not
invent a second key-exchange protocol. Keys are zeroized on drop and must come
from a CSPRNG-backed session or equivalent recipient key agreement.

## 2. CAP-004 authority binding

Before a session exists, `AuthorizedDisclosureSession::authorize` asks the
local `PermitValidator` to validate all authority dimensions used through the
requested ceiling:

- exact capability definition and private input commitment;
- exact purpose;
- one distinct effect class for each permitted disclosure stage;
- recipient equal to the validated Permit executor;
- byte/work/depth budget and retention; and
- session lifetime contained within Permit `not_before..expires_at`.

The resulting scope commitment binds the complete local request. It grants no
authority after Permit or session expiry, and every seal/open rechecks the
current local permit frontier and exclusive TTL.

## 3. Affordance-first state machine

The only valid progression is:

```text
AffordanceSketch
  -> ConstraintSketch
  -> EvidenceReferences
  -> FullNegotiatedPayload
```

The first payload is a typed coarse `AffordanceSketch`: capability classes,
input/output role classes, resource bucket and limitations. It has no provider,
manifest or private source field.

Every later stage requires a fresh one-time request nonce on both sender and
recipient state. The nonce is committed into authenticated associated data.
The recipient must register the same approval before opening the next capsule;
possession of a session key alone does not bypass this step. Stage skipping,
repetition, reordering, approval reuse and ceiling expansion fail closed.

Either side may cancel its local session. A cancelled sender cannot seal and a
cancelled recipient inbox cannot open. A network cancellation message and
durable restart restoration remain later runtime profiles; v1 does not claim
global cancellation.

## 4. Encryption and wire boundary

Payloads are encoded canonically, zero-padded inside the ciphertext, then sealed
with XChaCha20-Poly1305. Stage plaintext sizes are fixed at 512, 1024, 2048 and
4096 bytes; the 16-byte AEAD tag is additional. Nonce reuse in one sender
session is rejected.

Authenticated cleartext binds profile, random session ID, PermitCID, stage,
sequence, exclusive lifetime, scope/approval commitments and nonce. Exact
recipient ActorID and purpose CCID are represented only by session-keyed,
session-specific bindings; neither stable value appears directly on the wire.
Ciphertext authentication therefore binds recipient, purpose, Permit scope,
TTL and stage without publishing the private payload.

## 5. Inbound checks

The inbox performs, in order:

1. bounded canonical decode;
2. active CAP-004 Permit/session validation;
3. keyed recipient and purpose binding checks;
4. exact scope/lifetime/ceiling checks;
5. capsule replay and sequence/stage checks;
6. recipient-side approval check; and
7. AEAD authentication, fixed-padding validation and typed-stage binding.

A capsule is marked seen only after successful authenticated decryption, so an
invalid ciphertext cannot consume its identity. Exact replay never reopens or
renews it.

## 6. Boundaries

This profile does not:

- make a route sketch or public object private retroactively;
- prove transport unlinkability or hide packet timing/stage size;
- let a sender exceed consent, Permit, purpose, TTL or disclosure ceiling;
- publish, materialize, adopt or execute the decrypted content;
- classify a KU as true, false or wrong;
- create benefit, reward or OBT; or
- introduce a Core DNA Gene or execution opcode.

## 7. Executable evidence

Five tests prove:

- AffordanceSketch is first and both sides require a fresh approval for every
  later stage;
- wrong recipient, wrong key, expiry and replay are rejected;
- disclosure ceiling and sender/recipient cancellation stop progression;
- purpose, input and lifetime cannot expand the validated Permit; and
- full private payload bytes and stable recipient ActorID do not appear in the
  capsule while encrypted stage padding remains fixed.

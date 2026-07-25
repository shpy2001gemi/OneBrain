# OneBrain vNext - Node Identity Key Custody Profile v1

> **Task:** `NET-002 / M2 key custody`
> **Status:** Executable production boundary - frozen 2026-07-25
> **Code:** `ku-net::vnext_session::SessionIdentitySigner`,
> `onebrain-node::vnext_network_runtime`

## 1. Scope

This profile separates the OBP-RP NodeID signing operation from private-key
storage. A production node may use an operating-system keystore, hardware
security module, hardware token, or remote signer without giving OneBrain the
private Ed25519 key bytes.

This is transport identity custody. Actor root keys, feed keys, capability
permits, OBT authority, and user recovery policy are separate domains and must
not be loaded into the network runtime merely because they also use signatures.

## 2. Signer boundary

The runtime receives only two operations:

```text
public_key() -> Ed25519PublicKey
sign_session_message(message) -> Ed25519Signature
```

The caller-owned signer is shared by authenticated Hello/Welcome/Finish
sessions and by the domain-separated derivation used to protect peer-bound
resume tokens. Private-key export is not part of the interface.

Before creating a data directory, opening a database, or binding a listener,
`start_with_signer` asks the signer to sign a fixed domain-separated
proof-of-possession challenge and verifies that signature against the advertised
public key. An invalid public key, unavailable signer, or mismatched signature
fails startup. There is no fallback to another identity.

`OneBrainNode::set_vnext_identity_signer` exposes the same boundary to embedded
deployments before `start_network`. The resulting NodeID is derived from the
external public key exactly as it is for the built-in signer.

## 3. Compatibility file signer

`VNextNetworkRuntime::start` retains a local `vnext_identity.key` signer for
development and compatibility. It creates the file atomically and uses
owner-only permissions where the operating system supports them.

The file signer is not a claim of hardware-backed production custody.
Production operators requiring non-exportable keys must inject a signer and
must verify that no `vnext_identity.key` is created.

## 4. Restart, resume, and rotation

An external signer must return deterministic, standards-conformant Ed25519
signatures. Reusing the same signer across restart preserves the NodeID and the
peer-bound resume-token key while each new QUIC transcript remains unique.
Resume-token key material is derived from a domain-separated signature over the
ordered initiator/responder NodeIDs; that signature is never transmitted.

Changing the signer public key intentionally changes the NodeID. Existing
sessions and resume tokens then fail closed, and peers must update the expected
NodeID before sending durable outbox work. Automatic NodeID key rotation and
peer-route migration are not defined by v1.

If the external signer becomes unavailable after startup, a new handshake or
resume-key operation fails that session. The runtime does not substitute the
file signer, reuse a stale signature, or claim availability.

## 5. Actor-root separation

`ActorRootDelegation/1` is verified from its public proof bytes. The OBP-RP
runtime does not generate, persist, or request the Actor root private key.
Compromise or rotation procedures for an Actor identity therefore remain
separate from transport NodeID custody.

## 6. Executable evidence

- A caller-owned signer completes real QUIC mutual authentication, receiver
  restart, durable journal reopening, and `PeerBoundTokenV2` resume without
  creating `vnext_identity.key`.
- The restarted receiver preserves its NodeID and re-derives the same
  peer-bound token key through the external signer.
- A signer whose advertised key does not verify its proof-of-possession
  signature fails before the requested data directory exists.
- The built-in file identity remains restart-stable and corrupt/truncated key
  files fail explicitly.

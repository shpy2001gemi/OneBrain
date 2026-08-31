# OneBrain P5 Multi-Host Production Qualification Profile v2

> **Profile ID:** `P5_MULTI_HOST_PRODUCTION_QUALIFICATION_PROFILE_V2`
> **Machine vector:** [`../../../src/test-vectors/vnext/p5-multi-host-production-qualification-v2.json`](../../../src/test-vectors/vnext/p5-multi-host-production-qualification-v2.json)
> **V1 preservation:** [P5 multi-host profile v1](P5_MULTI_HOST_PRODUCTION_QUALIFICATION_PROFILE_V1.md)

## 1. Additive scope

V2 is additive and does not reinterpret V1. It qualifies the production
Reachability Manager, outbound-first relay routing and alternate-relay resume on
three physical Linux reference hosts. It MUST NOT promote unexecuted Windows,
macOS, Android, iOS or browser lanes.

The ring is `host-a -> host-b -> host-c -> host-a`. Every edge MUST
authenticate its expected NodeID. For the owner-approved NAT-independent lane,
every edge MUST be relay-class and every node MUST originate its connections;
no node-side inbound mapping, public listener or direct candidate is required.
An all-direct or partially direct ring rejects this lane. Direct/hole-punched
qualification remains a separate future profile and cannot be inferred from
relay evidence.

## 2. Candidate and authority roots

The immutable candidate commit, tree, bundle manifest and compiled binding
MUST match across inventory, run request, installed generation, child
receipts, operation receipts and aggregate. The unchanged Base release request
keeps its existing OpenPGP qualification authority.

P5 uses a separate owner-approved `p5-run-approver` Ed25519 policy and domain.
The P5 key MUST NOT expand or impersonate the frozen Base evidence/release
signer roles. Controller application signing and OpenSSH authentication use
distinct keys.

`P5InventoryV2` embeds the bounded canonical public probe set, topology
attestation and provider-evidence bundle. Their digests and status form
`P5EvidenceAuthorityV2` and MUST be repeated exactly in every public child,
admin-operation, finalization and aggregate receipt.

For this evidence set, provider status is
`owner-telephone-verified-provider-document-pending`. That pending status MUST
remain explicit even when the owner-signed topology attestation allows the
`production-reference` tier. Only three typed provider documents may derive
`provider-document-verified` in a future request.

## 3. Signed controller and child roots

`P5SignedControlFrameV2` binds format, Base/P5 request digests, inventory and
evidence authority, session/host, monotonic sequence, issued/expiry, command
kind, canonical command digest, controller public identity and signature.
Replay or stale sequence rejects before a command side effect.

`P5ChildReceiptV2` binds the full control-frame digest, exact request/session,
host/signer, candidate/bundle/profile/vector/allowlist digests, evidence
authority, command kind/result, embedded route/fault/checkpoint evidence,
resource counters, timestamps and signature. Unknown command-specific fields
reject; absent required evidence cannot be represented by free text.

`P5MultiHostAggregateV2` embeds or digest-binds every verified child receipt,
the ring/path evidence, all fault evidence, limitations enum, candidate and
evidence authority. Qualification booleans are derived only after all inputs
verify. Aggregate report bytes and aggregate signature are excluded from the
root preimage to avoid self-reference.

## 4. Route and failover evidence

Before marker traffic, a manager-owned journal snapshot MUST contain canonical
selected and alternate relay associations plus both peers' signed
reservations at each relay. At least two distinct usable relay paths must
exist for the faulted edge.

The selected relay must produce typed `RelayUnavailable`. The alternate relay
MUST differ, use its pre-failure reservation pair, produce a fresh outer
connection and transport binding, authenticate a fresh session, and resume the
exact acknowledged durable checkpoint. Same-relay reuse, stale binding,
stale session, checkpoint substitution and replay reject.

Public route evidence contains no IP, DNS name or interface. Restricted raw
evidence may retain actual session-scoped sockets required to prove a fault
target, represented publicly only by canonical digest and typed projection.

## 5. Fault and admin boundary

The thirteen required faults are frozen in the vector. Every fault has
before/during/after observations and the applicable root, resource and
recovery oracle. Observe-only commands and handcrafted receipts cannot
qualify.

The only admin actions are `prepare-session`, `cleanup-session`, `observe`,
`apply` and `clear`. Prepare/cleanup use null fault/phase; otherwise the exact
map is `observe=before`, `apply=during`, `clear=after`.

Every admin frame MUST bind request, session, host, operation ID, action,
fault/phase, issued/expiry, parameters or dual-signed dynamic target, and
controller signature. The helper persists the replay key before mutation and
MUST reject every reused mutation after process or host restart.

The signed operation receipt includes the admin-request digest, parameters or
target digest, post-action typed observation, bounded captured evidence-object
digests and host receipt signature. Captured operation stdout/stderr exclude
the response envelope itself, preventing digest self-reference.

## 6. Lifecycle and privileged execution

Bootstrap verifies the Base request/signature, P5 request/signature, policies,
inventory, bundle and proposed session config. It may install only a
create-new session config and bootstrap response; it MUST NOT start units,
create a namespace, change firewall/NAT/sysctl state or emit a qualification
receipt.

A separately signed `prepare-session` creates the bounded execution boundary,
starts the required signer/agent services and obtains the post-state receipt.
Cleanup is two phase: signed cleanup preserves the receipt signer/session long
enough to sign and fsync its final observation; a separately signed finalizer
then removes the remaining signer/session state.

All control and recovery binaries execute from the immutable candidate
generation path. The tested application activation symlink may move during a
rollback fault without replacing the P5 control boundary.

## 7. Evidence transport and privacy

Admin and agent responses use bounded canonical envelopes containing the
signed receipt plus exact digest-bound raw objects. The controller verifies
each object before create-new persistence. Partial, reordered, duplicated,
oversized or digest-mismatched responses reject.

Raw evidence is packaged separately with randomized
HPKE-X25519/HKDF-SHA256/ChaCha20-Poly1305 encryption to the request-bound
recipient key. Public CI artifacts contain only the privacy-safe aggregate and
verification receipt; private endpoints, interfaces and kernel output MUST
NOT leak into public strings or arbitrary limitations.

## 8. Qualification derivation

All thirteen faults, ten exit oracles, three authenticated hosts, the exact
relay-only path policy, distinct failover, exact checkpoint resume, resource bounds, privacy
scan and signatures are mandatory. Missing evidence is failure, not a skip.

The final tier is `production-reference` with explicit limitations
`provider-document-pending`, `non-linux-platform-lanes-pending` and
`mobile-carrier-mailbox-pending`. No individual limitation may be omitted or
rewritten as an unbounded operator string.

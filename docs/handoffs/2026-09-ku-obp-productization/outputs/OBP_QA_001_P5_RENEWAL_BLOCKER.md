# P5 approved renewal and exact-candidate blocker — 2026-09-23

Continuation: a [local correction is now tested](OBP_QA_001_RELAY_RENEWAL_FIX.md).
It is uncommitted and not deployed. This record preserves the original `fc65f08`
blocker and preparation evidence; the corrected bytes need a new immutable
candidate and freshly verified exact-candidate authority before remote execution.

The owner approved all prepared proposals, including the September 23–30 P5
policy renewal with the existing key/role and scoped execution on the same
three hosts. Do not ask for that approval again.

## Completed admission preparation

External WSL controller root:
`/home/shpy2/.onebrain/obp-qa-fc65f08-20260923`.

- Clean exact-candidate checkout `candidate/`: `fc65f08dede54d9e50731449776a5b2171d257ad`,
  tree `289c293c113302f84f7101f0fae68461aefd7691`.
- Approved `p5-run-approval-policy.json`: existing identity/domain, new approved
  interval only; old policy remains intact.
- Fresh signed and verified Base request:
  `release-requests/685c00516f85f46271a1888c643d7b73f74014715c8243907d7b28f5dc80ac74/release-request.json`.
- Fresh Linux default CLI built offline/locked from the exact candidate. Its
  measured verbose status produced `candidate-semantic-evidence.json`, with
  both semantic and artifact tuple digests independently recomputed.
- `prebuilt-registry-binding.json` and `prebuilt-registry-verified.json` created
  by canonical tools using the existing allowlisted Registry key and remeasured
  `onebrain_data` files. No Registry source extraction or model work occurred.
- Bundle 02 includes seven historical unit templates and the original operational
  verifier, each checked against the recovered bundle's SHA-256/BLAKE3 manifest.
  Template provenance is included; executable bytes are from the new build.
  Canonical bundle verifier passed. Manifest SHA-256/generation:
  `ee74e996f08c1ddab278531c3338e2f6c05061c099f5fd0b5d59a2ba7bc2f9fe`;
  manifest BLAKE3:
  `ae44b1bf140cc4c348e70eee220a9f01ba3f0cfd9ef64e8cf524f41fab4f50e5`.
- Bundle 02 uploaded to `~/onebrain-p5-staging/obp-qa-fc65f08-20260923-02` on
  all three hosts and verified there. Runner-a's newly extracted root inherited
  group-write mode and initially failed verification; changing only that staging
  root to 0755 made the unchanged artifact pass. Failed attempt retained.

## Confirmed blocker: existing relay descriptor renewal

Both live relay candidate descriptors were fetched read-only and matched their
archived bytes exactly. Their expiry values are `1788319236` and `1788319238`
(September 2); neither can enter a fresh qualifying inventory.

The exact candidate's relay CLI has no descriptor renewal command.
`src/onebrain-relay/src/runtime.rs` exports by first writing the output file,
then `create_new(DescriptorFloor, "candidate-v1", ...)`. The durable state
implementation rejects an existing key. Descriptor construction always sets
`previous_descriptor_blake3: None`, so changing the configured sequence alone
cannot produce the required contiguous same-key chain.

An offline reproduction with the shipped binary and a new isolated fixture key
confirmed first export exit 0, second export exit 1 (`OBP_RELAY_RUNTIME: State`).
The second output file nevertheless exists; it must not be treated as an admitted
descriptor. No fixture private key was copied into the repository or bundle.

Restricted measured results:
`target/obp-qa-001/p5-remote-02/existing-descriptors.json` and
`descriptor-renewal-repro.json`. The fixture remains under the external controller
root `descriptor-renewal-repro-01/`.

The archived reset script would move the complete REDb database out of service
and initialize empty state. That does not preserve active replay/nonce/reservation
floors or produce a contiguous descriptor chain, so it was not run. Old host keys,
state, units, current selectors and active relay services remain unchanged.

## Required correction before execution

Implement and review a relay lifecycle renewal operation that atomically advances
the descriptor chain while preserving replay/control/reservation state, verifies
previous digest and sequence, and handles output-publication failure safely.
Add regression coverage for reopen/replay, concurrent or failed renewal, and
fresh remote possession probes. Existing activation must remain bound to the
new descriptor and verified probes.

This requires a new immutable runtime candidate and rebuilt/signed bindings;
uncommitted runtime edits cannot qualify `fc65f08`. No runtime fix, Git
commit/push/merge, reset, key rotation, network fault or production P5 dispatch
was performed in this preparation. P5 request/inventory were not fabricated in
the absence of fresh descriptors. OBP-QA-001 remains Blocked; the six earlier
remote loopback preflights remain preflight, not qualification.

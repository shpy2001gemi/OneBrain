# P5 Outbound-First Preflight Profile V2

> **Status:** implementation profile  
> **Qualification tier:** `production-reference` only after the complete gate

## Boundary

This profile exercises three authenticated OBP nodes on three physical hosts.
It does not require inbound NAT configuration. A path is one of `direct`,
`hole-punched`, `relay-udp`, or `relay-tcp-443`; direct-class is the first two
and relay-class is the last two. Relays are permissionless, descriptor-key
pinned carriers. They are not identity, discovery, ordering, or truth
authorities.

The qualifying ring is exactly `host-a -> host-b -> host-c -> host-a`. It must
contain at least one direct-class and one relay-class path. Simulation, socat,
WireGuard, single-host, preflight-only, and observe-only runs cannot qualify.

## Fail-closed evidence

Every controller command and child/admin receipt is request-, inventory-,
session-, host-, and sequence-bound. The controller starts all three pinned
OpenSSH bridges concurrently. A completed valid receipt is stored create-new
and durably synced before any sibling future error is evaluated. Timeout,
missing/duplicate/late receipt, signature failure, or cleanup failure makes the
aggregate nonqualifying while retaining verified partial evidence.

The selected-relay drill qualifies only when both selected and alternate relay
reservations predate the failure, relay identities differ, the replacement
transport binding and authenticated session are fresh, and the resumed durable
checkpoint is exactly the acknowledged intent/root set at sequence + 1. The
thirteen inherited real faults remain mandatory; selected-relay shutdown is an
additional V2 route drill.

Provider evidence status is explicitly repeated in inventory, every receipt,
aggregate, and verification receipt. The current accepted value is
`owner-telephone-verified-provider-document-pending`; this does not claim
provider-document verification.

## Controller isolation

The Ed25519 application key, P5 run-approver key, X25519 raw-evidence recipient,
and OpenSSH identity are distinct. Host-key verification uses a generated,
inventory-bound known-hosts file with global files, DNS verification, key
updates, agents, certificates, passwords, keyboard interaction, GSSAPI, and
host-based authentication disabled. SSH forced commands use immutable
candidate-generation paths.

Plaintext `p5/raw/` is private controller evidence and is never a public CI
artifact. Public output contains only privacy-safe authority/request/aggregate
bytes and the encrypted raw archive envelope. Qualification booleans are
derived from verified evidence and are never accepted as caller claims.

## Reproduction

```text
python -m unittest scripts.runner.test_onebrain_p5_multi_host_v2 -v
python -m unittest scripts.runner.test_onebrain_p5_multi_host -v
python -m unittest scripts.release.test_validate_evidence_carry_forward -v
python -m unittest scripts.ci.test_validate_vnext_p5_multi_host -v
python scripts/ci/validate_vnext_contracts.py
```


# P5 descriptor history admission v1

Owner accepted the OBP-QA-001 successor-history correction on 2026-09-23 (D-037).
This additive input profile preserves existing descriptor CBOR and probe V1.
It does not relax live possession, current freshness or existing durable floors.

## Closed input and limits

Probe request format 2 retains all V1 fields and requires
`descriptor_history_hex`: an ordered array of canonical lowercase hex predecessor
descriptors, excluding `candidate_descriptor_hex`. Format 1 forbids this field.
Format-2 probe receipts use format 3 and repeat that exact history; format-1
requests continue emitting format-2 receipts without the extra field.
The existing 65,536-byte request ceiling remains. Each chain, including terminal,
has at most 16 descriptors and 8,192 decoded bytes. Empty predecessors are valid
only for a sequence-1 terminal without a predecessor.

P5 diagnose/ensure-reservations commands optionally carry
`relay_descriptor_histories`, a closed map from each exact terminal hex in
`relay_descriptors` to its predecessor array. If present the key set must match
the terminal set exactly, with two or three relays; the same per-chain bounds
apply. Absence retains V1 behavior. The controller derives this map from matching
probe receipts in the signed inventory. Sources for one terminal must agree on
every history byte. Inventory/evidence-authority and command digests bind history
through existing request/receipt signatures; no free-standing unsigned floor is
accepted. No new network protocol object or application endpoint is introduced.

## Shared cryptographic validation and atomic floors

The shared reachability core validates the complete chain before changing state:
canonical decoding, each signature and derived NodeID, sequence 1 anchor with no
predecessor, contiguous sequence and exact predecessor digest, identical key,
endpoints, transports, protocol versions and capacity policy. Issue times cannot
decrease; expiry must strictly advance. Each object retains the existing bounded
validity interval. Terminal freshness is checked at current time. Historical
objects may be expired; they are verified history only, never dial targets or
sources of live reservations/authority. No historical timestamp is passed as the
current admission time.

A typed verified chain may bridge an empty floor or an exact matching ancestor
floor contained within that complete chain. A digest-only floor cannot be the
first anchor because it does not prove unchanged descriptor configuration. Any
known floor ahead of, outside, or conflicting with the chain rejects.
Exact terminal recovery is allowed only via this explicit verified-history input
and still requires a new live proof. Ordinary admission's replay behavior is
unchanged. No new floor is committed until every terminal possession proof
passes. The store checks the current floor and writes the terminal in one lock
or immediate-durability transaction; races/failure cannot install partial history.
Other sequence, nonce and reservation state is preserved.

The P5 agent retains descriptor floors in its existing session-scoped durable
replay database, shared with its admission service. A restart reconstructs only
verified inputs and repeats possession; it cannot restore a live session.
Probe processes remain bounded one-shot observations; their ephemeral floors do
not claim restart persistence. They use the same chain validator and admission.

The V2 controller exposes `run-happy-case` for signed bootstrap, live matrix,
authenticated ring, markers, shutdown, cleanup and finalization without fault
execution or a qualification aggregate. `run` retains the complete P5 fault gate.

## Required evidence

Cover cold start at sequence 2+, expired historical entries, bad signatures/keys,
forks/gaps/config changes, byte/count overflow, future/expired terminals,
equal-terminal explicit recovery, durable restart, stale concurrent updates and
failed possession without floor advancement. Real happy-case evidence requires
four fresh cross-host probes, full exact-candidate V2 admission, three-host
authenticated relay ring and cleanup. Happy-case success alone is nonqualifying;
all existing production fault/oracle and separate consumer-network gates remain.

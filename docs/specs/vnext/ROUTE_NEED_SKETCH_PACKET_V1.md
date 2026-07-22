# RouteNeedSketch Packet v1

> **Task:** `SEC-002`  
> **Status:** Complete  
> **Depends on:** `SEC-001`, `NET-001`

## 1. Purpose

`RouteNeedSketchV1` is a short-lived operational routing packet derived from an
authorized `ROUTE_MINIMAL` projection. It helps locate peers likely to answer a
private need without serializing the private NeedIR or raw KQL.

The packet is scoped to an authenticated transport session where available, but
does not grant feed, knowledge or capability authority.

## 2. Canonical packet

The canonical `control/1` map has major version `1` and exactly these fields:

| Key | Field | Contract |
|---:|---|---|
| `0` | profile major | exactly `1` |
| `1` | sketch ID | non-zero, per-packet entropy |
| `2` | one-time reply capability | non-zero and not reused in the same local run |
| `3` | coarse route token | exactly one allowlisted class/code pair |
| `4` | response budget bucket | non-zero coarse bucket |
| `5` | expiry evaluations | non-zero receiver-relative lifetime |
| `6` | hop budget | non-zero |
| `7` | padding class | `1`, `2` or `3` |
| `8` | replay nonce | non-zero, per-packet entropy |
| `9` | salted disclosure commitment | commits private run/definition without revealing either |
| `10` | padding | zero-only canonical padding |

Padding classes produce exact network lengths of 512, 1024 and 2048 bytes.
The decoder rejects a size/class mismatch or non-zero padding, preventing the
padding field from becoming an application-controlled covert payload.

There is no schema field for raw KQL, QueryDefinitionCID, run ID, private source
reference, exact constraint/conjunction, or stable Receptor, Assembly, Need,
User or Node identity.

## 3. Multipath and entropy rules

One local QueryRun may emit at most three packets. Each packet has exactly one
coarse token. Within the run the compiler rejects reuse of:

- sketch ID;
- one-time reply capability;
- replay nonce; or
- commitment salt.

Consequently the salted commitment also changes per packet. These constraints
remove deliberate stable correlation fields; they do not prove unlinkability
against timing, transport metadata or a colluding observer.

The token must already satisfy estimated support `>=64`. SEC-001 must replace a
rarer value with an allowlisted supported ancestor or suppress transmission.
The support estimate itself is not sent as evidence and is not a truth claim.

## 4. Replay and expiry

`RoutePacketReplayGuard` validates canonical bytes before admission and stores
the domain-separated packet digest under its replay nonce. First acceptance
sets an exclusive receiver-local expiry:

```text
expires_at = first_seen_local_evaluation + expiry_evaluations
```

An exact replay is rejected, including before expiry. A different packet under
the same nonce is a collision and is rejected. Replaying the packet never
refreshes first-seen time or expiry. After expiry the entry remains a replay
tombstone until local policy replaces the bounded guard; capacity exhaustion
fails closed.

This is deliberately independent of synchronized wall clocks. A peer cannot
prove how long a packet was withheld before its first observation, so v1 does
not claim a network-global TTL. Durable restart restoration is a runtime concern
for a later persisted inbox profile.

## 5. Boundaries

The packet does not:

- reveal or publish the private need;
- prove anonymity, support population or global reachability;
- authorize an encrypted capsule or downstream side effect;
- classify any KU as true, false or wrong;
- create reward or OBT; or
- introduce a Core DNA Gene or execution opcode.

## 6. Executable evidence

Tests prove:

- three packets have one token each, distinct reply keys and distinct salted
  commitments, while a fourth is rejected;
- reuse of any entropy bundle is rejected;
- all support values below `64` are rejected and supplied private byte strings
  are absent from the packet;
- exact packet sizes and zero-only padding are enforced; and
- replay is rejected without renewing receiver-relative expiry.

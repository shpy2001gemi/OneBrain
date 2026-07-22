# OneBrain vNext — Identity and Knowledge Object Profile v1

> **Tasks:** `IDN-001`, `OBJ-001`, `OBJ-002`  
> **Status:** Normative — frozen 2026-07-20  
> **Code:** [`ku-core::foundation::{identity, object, schema_registry}`](../../../src/ku-core/src/foundation)  
> **Vectors:** [`identity-object-v1.json`](../../../src/test-vectors/vnext/foundation/identity-object-v1.json)

## 1. Identity roles

`NodeId`, `DeviceId`, `ActorId` and `FeedId` are separate Rust/wire types,
each exactly 32 bytes. They are not aliases and have no semantic conversion to
`u64`. A log/UI may render a short prefix, but a prefix is never a key, ACK
identity, clock actor, watch owner, evidence deduplication key or index key.

| Type | Owns | MUST NOT own |
|---|---|---|
| `NodeId` | authenticated network session/routing identity | actor authority, feed sequence, attester independence |
| `DeviceId` | actor-authorized device/key identity | transport endpoint or namespace-scoped feed |
| `ActorId` | pseudonymous principal/persona identity | actor-global sequence or network route |
| `FeedId` | namespace/generation-scoped single-writer feed | actor-global history or transport identity |

The exact `FeedId` derivation, inception, delegation and rotation rules belong
to `IDN-002`. `IDN-001` only freezes its full-width typed representation.

`CrdtDot<K> = (TypedIdentity<K>, counter)` retains the whole identity. The
provided `FullWidthClock<K>` is valid only inside a bounded schema/selector/feed
scope. It MUST NOT become a global vector across every OneBrain actor.

Legacy `ku-net::identity::NodeId` and legacy `u64` clocks remain isolated until
their owning migration task replaces each path. They are not accepted as vNext
canonical identities.

## 2. Generic Knowledge Object envelope

The root uses `onebrain/canonical/1` and schema ID `1`:

```text
root = {
  0: 1,                    // schema_id: knowledge-object-envelope
  1: 1,                    // schema_major
  2: 0,                    // schema_minor
  3: body,
  4: extensions?,
  5: critical_extensions?
}
```

The body is:

| Key | Field | Rule |
|---:|---|---|
| `0` | `object_kind` | Stable unsigned registry ID. Unknown kind is opaque, not invalid. |
| `1` | `kind_major` | Incompatible semantic change increments major. |
| `2` | `kind_minor` | Backward-compatible extension revision. |
| `3` | `disclosure_class` | `0 PUBLIC`, `1 NEGOTIATED_ENCRYPTED`, `2 ROUTE_MINIMAL`, `3 LOCAL_ONLY`. |
| `4` | `references` | Set-like array of typed `{0: reference_kind, 1: cid_bytes32}` entries; canonical-key sorted and unique. |
| `5` | `payload` | Kind-owned canonical value. Unknown kinds MUST NOT project/execute it. |
| `6` | `limits` | Optional `{0: max_total_nodes, 1: max_depth}`; may narrow but never exceed the parent resource profile. |

Generic objects hash the exact accepted root bytes with domain `object/1`.
Narrow object families may allocate a narrower domain in the domain registry;
they do not silently reuse a generic `ObjectCid` under different bytes.

An unknown `object_kind` that passes canonical/schema/resource/CID validation is
stored only as bounded opaque bytes under local quota. Original bytes are the
forwarding source. Unknown base fields, schema major or critical extension are
rejected/quarantined for semantic use. Unknown non-critical extensions are
preserved byte-for-byte.

Maximum references in profile v1 are `4,096`, in addition to the parent
`object/1` byte/depth/node limits. Duplicate references are invalid.

## 3. Schema registry v1

Schema IDs:

| ID | Name | Owner task |
|---:|---|---|
| `1` | knowledge-object-envelope | `OBJ-002` |
| `2` | feed-inception | `IDN-002` |
| `3` | knowledge-event-envelope | `EVT-001` |
| `4` | feed-checkpoint | `CHK-001` |
| `5` | provider-lease | `DHT-001` |
| `6` | delegation-permit | `CAP-003` |
| `7` | reconciliation-message | `OBP-003` |
| `8` | manifest | `OBJ-002`/`OBS-001` |

Generic object kinds:

| ID | Name |
|---:|---|
| `1` | legacy-evidence |
| `2` | semantic-kernel |
| `3` | receptor-definition |
| `4` | assembly-manifest |
| `5` | knowledge-affordance |
| `6` | mapping-envelope |
| `7` | query-definition |
| `8` | capability-definition |
| `9` | implementation-manifest |
| `10` | conformance-fixture (test-only) |
| `11` | receptor-claim-envelope |
| `12` | receptor-resolution-action |
| `13` | use-evidence |
| `14` | derivation-evidence |
| `15` | encoding-attempt |
| `16` | fidelity-policy |
| `17` | encoding-fidelity-attestation |
| `18` | sanitized-public-problem |
| `19` | outcome-observation |
| `20` | benefit-evidence |
| `21` | exploration-policy |
| `22` | source-artifact |
| `23` | observation-event-payload |

Event type allocations:

| ID | Name |
|---:|---|
| `1` | receptor-resolution |
| `2` | use-evidence |
| `3` | derivation-evidence |
| `4` | encoding-fidelity-attestation |
| `5` | outcome-observation |
| `6` | benefit-evidence |
| `7` | observation |

Allocations are append-only within v1. A deleted/renamed schema or kind leaves a
reserved tombstone; its number is never reused. Test-only kind `10` MUST NOT be
published as production knowledge; later production allocations continue after
it without reusing or renumbering the test tombstone.

## 4. Acceptance evidence

- Two NodeIds sharing the exact first 64 bits remain distinct through canonical
  bytes, ACK/watch collections, full-width sync clock and ordered indexes.
- Role-typed identity values round-trip without truncation.
- Known objects round-trip with the same CID and original bytes.
- Unknown kinds are opaque and preserve original bytes.
- Unknown critical extensions cannot reach semantic execution.
- References are sorted/deduplicated and declared limits cannot widen profile.
- The same frozen vectors pass `ku-core`, `onebrain-protocol` and `ku-net`.

# OneBrain vNext — Protocol Codec and Legacy Isolation Profile v1

> **Task:** `PROTO-001`  
> **Status:** Executable protocol contract — frozen 2026-07-20  
> **Code:** [`onebrain-protocol`](../../../src/onebrain-protocol/src)

## 1. Ownership

`onebrain-protocol::types` is the only owner of vNext logical message variants
and stable wire IDs. `onebrain-protocol::codec` is the only canonical codec.
Carrier code receives `VNextMessage`; it must not define a parallel enum or
reinterpret IDs.

The initial v1 set separates manifest from payload for ObjectCID and EventCID
records and binds every message to a full-width SelectorCID. All identifiers
remain typed 256-bit values.

## 2. Canonical codec

The root binds schema ID, major/minor, wire ID and body under the shared
restricted canonical CBOR profile. Decode performs canonical re-encoding and
rejects unknown wire IDs, unsupported major versions, malformed full-width
identifiers and resource-limit violations.

Payload messages recompute the domain-separated ObjectCID/EventCID before they
are returned. This check is byte identity only; semantic validation and
validate-before-persist still belong to the validated store.

## 3. Legacy isolation

TCP/JSON demo messages live only in `onebrain-protocol::legacy`. Their enums
contain no vNext object, event or selector variant. The vNext logical types do
not implement Serde serialization and therefore cannot enter the legacy generic
JSON writer.

Inbound legacy parsing returns both the parsed message and exact original
bytes. No normalization, inferred full-width ID, vNext conversion or semantic
upgrade occurs in the parser.

Top-level re-exports preserve current seed/demo compatibility while keeping the
implementation visibly under the legacy module.

## 4. Executable evidence

Tests prove deterministic vNext payloads, canonical round-trip, CID-to-payload
binding, unknown wire/resource-cap rejection, exact legacy byte preservation and
legacy rejection of vNext CBOR. The protocol crate also runs the shared
foundation conformance vectors.


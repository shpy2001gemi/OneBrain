# OneBrain vNext — Hybrid Inventory Forest Profile v1

> **Task:** `OBP-002`  
> **Status:** Executable reconciliation source contract — frozen 2026-07-20  
> **Code:** [`ku-net::vnext_inventory_forest`](../../../src/ku-net/src/vnext_inventory_forest.rs)

## 1. Scope and root

Each `HybridInventoryForest` belongs to one exact SelectorCID. It maintains
separate sparse 256-bit radix lanes for ObjectCID, EventCID, MappingKernelCID,
FeedInception CID and AuthorityEvent CID bytes. A leaf binds record kind, full
CID and canonical byte length. Same
kind/CID with a different length is a collision, never an overwrite.

The forest root binds the SelectorCID, all five lane roots and canonical feed
prefix summaries. It is independent of insertion order and reproduces after a
canonical snapshot/restore.

This is availability inventory, not semantic truth, acceptance or authority.

## 2. CID ranges and divergence

`InventoryRange` is an exact bit prefix from 0 to 256 bits; bits after the
prefix are masked. Range summaries report a deterministic subtree root and
record count. Comparing two forests under the same SelectorCID returns the
first exact divergent child prefix in stable record-kind/bit order.

Different selectors are incomparable and rejected. Feed-prefix divergence uses
the complete Event lane sentinel because feed sequence/head space is causal,
not a fake CID prefix.

## 3. Feed-prefix inventory and checkpoints

Each feed summary binds full FeedID, covered sequence, head EventCID and
`checkpoint_frontier_refs[]`. Multiple heads at the same sequence are retained
as separate records; arrival order does not choose a winner.

The checkpoint field exists from v1 and may be empty. Exact root equality cannot
establish selector-relative completion while any referenced checkpoint is
unknown. Even a fully assessed selector never claims global completion.

## 4. Semantic shards

Semantic shard hints bind a source root, projection root and index version, but
are derived/rebuildable only. Adding or deleting them does not change the
authoritative inventory root, and snapshots intentionally omit them. They can
accelerate KQL recall but cannot establish reconciliation completion.

## 5. Executable evidence

Tests cover insertion-order/root stability, canonical restart, exact divergent
prefix, CID collision rejection, semantic-shard root isolation and unknown
checkpoint completion blocking.

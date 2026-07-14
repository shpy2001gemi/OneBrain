# STUB_TRACKING.md — OneBrain Node Stub Methods

> **Purpose**: Track all stub methods in `onebrain-node/src/node.rs` that need
> full implementation. These stubs were created to unblock CLI development
> while the underlying subsystems are being built.
>
> **Created**: 2026-07-14
> **Last Updated**: 2026-07-14

## Stub Methods

### Knowledge Management

| Method | Current Behavior | Full Implementation Needed | Priority |
|--------|-----------------|---------------------------|----------|
| `deprecate_ku(cid)` | Checks KU exists, prints warning | Update Epigenetics layer, broadcast deprecation notice to P2P | Medium |
| `encode_draft(text)` | Calls `encode_and_store` (still broadcasts) | Separate DraftStore, encrypted local, no broadcast, separate lifecycle | High |
| `encode_with_attachments(text, files)` | Stores blobs + encodes text separately | Insert MediaRef instructions into KU, atomic encode+attach, rollback on failure | High |

### Social & Discovery

| Method | Current Behavior | Full Implementation Needed | Priority |
|--------|-----------------|---------------------------|----------|
| `follow_node(node_id)` | No-op with validation | Persist in follow store, subscribe to PubSub topic | Medium |
| `unfollow_node(node_id)` | No-op with validation | Remove from follow store, unsubscribe from PubSub | Medium |
| `following_list()` | Returns empty `Vec` | Read from persistent follow store | Medium |
| `get_peer_profile(node_id)` | Looks up connected peers for basic info | Fetch full profile from P2P/DHT, include EigenTrust score | Low |

### Multi-Device

| Method | Current Behavior | Full Implementation Needed | Priority |
|--------|-----------------|---------------------------|----------|
| `list_devices()` | Returns current device only | Read device group from identity store, sync across group | Medium |
| `sync_status()` | Returns "up-to-date" placeholder | Read from VectorClock-based SyncManager | Medium |

### Blob Storage Extensions

| Method | Current Behavior | Full Implementation Needed | Priority |
|--------|-----------------|---------------------------|----------|
| `pin_blob(cid)` | Checks blob exists, no-op | Update `BlobMeta.pinned = true` in BlobStorage | High |
| `unpin_blob(cid)` | Checks blob exists, no-op | Update `BlobMeta.pinned = false` in BlobStorage | High |

### Bulk Operations & Tags

| Method | Current Behavior | Full Implementation Needed | Priority |
|--------|-----------------|---------------------------|----------|
| `bulk_delete(gene, before)` | Iterates and deletes matching KUs (works but slow) | Storage-level batch delete, index cleanup | Low |
| `add_tag(cid, tag)` | Checks KU exists, no-op | Update KU Epigenetics layer, persist tags | High |
| `remove_tag(cid, tag)` | Checks KU exists, no-op | Update KU Epigenetics layer, remove tag | High |
| `list_all_tags()` | Returns empty `Vec` | Scan Epigenetics of all KUs, build tag index | Medium |
| `pin_ku(cid)` | Checks KU exists, no-op | Persist pin state in identity-level store, sync across devices | Medium |
| `unpin_ku(cid)` | Checks KU exists, no-op | Remove pin state from identity-level store | Medium |
| `pinned_kus()` | Returns empty `Vec` | Read from pin store (identity-level) | Medium |

### Watch (Standing Queries)

| Method | Current Behavior | Full Implementation Needed | Priority |
|--------|-----------------|---------------------------|----------|
| `create_watch(kql)` | Generates random ID, no-op | Parse KQL WATCH query, register in WatchManager, hook into KuReceived events | Low |
| `list_watches()` | Returns empty `Vec` | Read from WatchManager | Low |
| `delete_watch(id)` | Validates ID, no-op | Remove from WatchManager, unhook event listener | Low |

## Implementation Order (Suggested)

1. **Phase 1** (High priority): `pin_blob`/`unpin_blob`, `add_tag`/`remove_tag` — simple storage updates
2. **Phase 2** (High priority): `encode_draft`, `encode_with_attachments` — need DraftStore + MediaRef
3. **Phase 3** (Medium): `follow_node`/`unfollow_node`/`following_list` — need FollowStore
4. **Phase 4** (Medium): `list_devices`/`sync_status` — need SyncManager integration
5. **Phase 5** (Low): Watch queries, peer profile, bulk operations — need subsystem integration

# STUB_TRACKING.md — OneBrain Node Legacy Placeholder Methods

> **Purpose**: Track legacy `OneBrainNode` methods in `onebrain-node/src/node.rs`
> whose behavior is still a placeholder or keeps state only in process memory.
> They serve the legacy `/api/*` pages and CLI; the vNext KU workspace
> (`/api/vnext/ku`, Base runtime) does not use them.
>
> **Created**: 2026-07-14
> **Last Updated**: 2026-10-19 (ARCH-05, D-051): refreshed against `main` `117acea`.
> The 2026-07-14 table listed no-op stubs; most now keep state in process-memory
> `HashMap`/`Vec` fields, which **are lost on every restart** even though older code
> comments called them "persistent".

## Current state

| Method(s) | Current behavior (2026-10-19) | Persistence | Remaining gap |
|---|---|---|---|
| `deprecate_ku(cid)`, `is_deprecated` | Inserts into `deprecated_kus` set; `get_ku` reflects it | Process memory only | No durable epigenetics write; no P2P notice |
| `save_draft`, `list_drafts`, `get_draft`, `update_draft`, `delete_draft` | Drafts kept in `drafts` map | Process memory only | Lost on restart; vNext KU drafts (`/api/vnext/ku`) are the durable path |
| `publish_draft(id)` | Calls legacy `encode_and_store`, then drops the draft | Legacy KU store | Uses the legacy encoder, not the vNext KU workspace |
| `encode_with_attachments(text, files)` | Stores blobs, encodes text, then links blobs by reference; link failures are only logged | Blob + legacy KU store | Not atomic; no rollback on failure |
| `follow_node`, `unfollow_node`, `following_list` | Kept in `following` list | Process memory only | No follow store, no PubSub subscription |
| `get_peer_profile(node_id)` | Substring match over connected peers or follow list; fixed trust 0.5/0.6/0.7 and tier `Leaf` | — | Placeholder trust/tier values |
| `get_feed(limit)` | Returns `trending_kus(limit)`; follows are not used | — | No follow boost |
| `list_devices`, `register_device`, `unregister_device`, `sync_status` | Device list in memory; current device always `up-to-date` | Process memory only | No identity device group or real sync |
| `pin_blob`, `unpin_blob` | `BlobStorage::set_pinned` (`try_lock`; `Storage busy` under contention) | Durable (blob store) | Contention returns an error instead of waiting |
| `bulk_delete(gene, before)` | Lists up to 10,000 KUs and deletes one by one | Legacy KU store | Silently capped at 10,000; no batch delete/index cleanup |
| `add_tag`, `remove_tag`, `list_all_tags`, `get_ku_tags` | Tags in `ku_tags` map | Process memory only | Lost on restart |
| `pin_ku`, `unpin_ku`, `pinned_kus` | Pins in `pinned_kus` set | Process memory only | Lost on restart |
| `create_watch`, `list_watches`, `delete_watch`, `evaluate_watches` | Watch list in memory; evaluation is a case-insensitive substring match of the raw query text against preview/gene type, not KQL | Process memory only | Lost on restart; not KQL. Text KQL `WATCH` is rejected by `/api/kql` (ARCH-04) |

## Direction

- D-051 freezes the legacy surface: fix honesty bugs, do not extend it. ARCH-06
  routes or labels the legacy Encode/Explorer pages; durable user features belong
  in the vNext KU workspace rather than these in-memory maps.
- If a legacy method stays user-visible, its UI should label the state as
  session-only until it is persisted or removed.

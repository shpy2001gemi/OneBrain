# OneBrain vNext — Checkpoint Compaction and Local GC Profile v1

> **Tasks:** `CHK-003`, `CHK-004`, `CHK-005`, `CHK-006`  
> **Status:** Normative implementation profile  
> **Code:** [`foundation::checkpoint_compaction`](../../../src/ku-core/src/foundation/checkpoint_compaction.rs)

## 1. Exact high-water anchors

`ExactHighWaterAnchors` stores separate lanes for provider lease generation,
provider retirement floor, permit generation, key generation and feed
checkpoint position. Each `(lane, subject)` retains:

- the exact maximum observed value; and
- every full-width record CID conflicting at that maximum.

Merge is immutable max-plus-union. A lower arrival is retained by its durable
source namespace but cannot lower the compact anchor or resurrect an older
lease/permit/key/checkpoint state. Floors are never Bloom-filter or
probabilistic-GC entries. The canonical anchor root is the
`retirement_floor_root` committed by CHK-001.

These anchors summarize reducer high-water only. They are not a second reducer,
global retirement decision or delete instruction.

## 2. Shadow checkpoint and dry-run compaction

`ShadowCompactionPlanner` requires:

- CHK-002 `AUTHORIZED_RELATIVE` suppression assessment;
- exact anchor-root equality;
- live/rebuilt view-root parity;
- an enabled local shadow kill switch; and
- an exact checkpoint inclusion proof for every non-derived candidate's
  covering event.

Authority anchors, checkpoint anchors and quarantine evidence are protected.
The result contains the exact candidates, archive manifest, audit root and
`deletion_performed = false`. A single missing/mismatched proof blocks the whole
plan instead of silently creating a partial destructive set.

## 3. Archive and custody

`ArchiveManifest` sorts and commits each record kind, CID, storage class, byte
length and bytes digest plus the checkpoint and anchor root. A signed custody
receipt binds the manifest, entry root, anchor root, custodian feed and exact
key-state frontier. The receipt signer must be authorized relative to that
frontier.

A receipt proves only this bounded custody statement. It does not prove KU
truth, usefulness, global durability or reward.

## 4. Restore drill

The restore drill passes only when:

1. the shadow plan still has view parity and has never deleted;
2. the exact archive manifest contains every candidate unchanged;
3. the exact high-water anchor root is present;
4. the validated custody receipt binds that archive and anchor root; and
5. a `CheckpointRestoreRebuilder` actually replays the exact checkpoint,
   archive, anchors and its owned retained/later events, producing the original
   live view root. A caller-asserted "restored root" is not accepted.

Missing archive, custody, anchor or root parity returns a typed failure and
`must_retain_payloads() == true`.

## 5. Local retention and destructive gate

Default policy keeps canonical events, canonical objects, private sources,
authority/checkpoint anchors and quarantine. Only rebuildable derived caches
are evictable by default.

Changing a class to checkpoint/archive eviction is a local operator policy.
Private-source deletion additionally requires exact per-record consent.
`ShadowCompactionPlan`, `RestoreDrillReport` and `ApprovedLocalEviction` have
private construction boundaries, so downstream crates cannot fabricate a
passing report or deletion capability. An `ApprovedLocalEviction` exists only
after:

- operator kill switch enabled;
- shadow soak passed;
- successful restore drill;
- non-empty operator recovery path; and
- class policy/consent approval.

The backend contract persists the eviction audit before the first local delete.
It has no broadcast/global-delete operation.

## 6. Executable evidence

Five tests prove:

- high-water merge is commutative, retains same-water conflicts and rejects
  resurrection by old arrivals;
- shadow planning emits a full manifest without deletion;
- missing proof or view parity blocks candidate creation;
- restore requires exact archive, custody, anchors and view root;
- local deletion requires every gate and writes audit before delete.

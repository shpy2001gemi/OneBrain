# OneBrain vNext — Feed Checkpoint and Proof Profile v1

> **Tasks:** `CHK-001`, `CHK-002`  
> **Status:** Normative implementation profile  
> **Code:** [`foundation::checkpoint`](../../../src/ku-core/src/foundation/checkpoint.rs)

## 1. Boundary

A feed checkpoint is a signed, immutable claim about one feed prefix at one
named reducer and key-state frontier. Signature validity authenticates its
producer; it is not sufficient suppression authority and never authorizes
payload deletion.

There is no global checkpoint, final feed head or network-wide causal-stability
oracle. A partition may continue from older local evidence and reevaluate a
checkpoint after reunion.

## 2. Canonical signed record

Schema `feed-checkpoint` (`4`), major `1`, uses the reserved `checkpoint/1` CID
and signature domain. Its body binds:

- full `FeedID` and inclusive `covered_sequence`;
- deterministic Merkle `covered_root`;
- final reducer `state_cid` and exact `reducer_version`;
- `last_event_cid`;
- optional `previous_checkpoint_cid`;
- exact `retirement_floor_root` for CHK-003 anchors;
- exact key-state frontier and optional key-state root;
- optional archive manifest reference; and
- non-zero replay-separating nonce.

The signer must be the same feed and key as the validated FeedInception.
Changing any field changes the signature and CheckpointCID.

## 3. History and Merkle proofs

Each `CheckpointHistoryWitness` chunk contains at most 65,536 exact leaves.
The first chunk begins at sequence zero; every later chunk begins at exactly
`previous.covered_sequence + 1`. Thus the cap bounds per-checkpoint work rather
than the lifetime of a feed. Each leaf is constructed from an already validated
signed event and commits:

- feed and feed-local sequence;
- EventCID;
- the event's canonical causal-parent set;
- reducer state before and after the event; and
- reducer-effect commitment.

Leaves must be contiguous, remain on one feed, link each position to the exact
prior EventCID and form an exact state transition chain. The chunk Merkle tree
uses domain-separated leaf and node hashes. Odd nodes are duplicated
deterministically; inclusion proofs are bounded to 64 siblings. An extension
root additionally commits the prior CheckpointCID/root, prior and new sequence
bounds and the new chunk root.

An inclusion proof covers one exact EventCID, not every event at the same feed
position. Therefore a valid checkpoint cannot suppress a previously unseen
fork merely because its sequence is below the checkpoint high-water mark.

## 4. Consistency and effect validation

Append consistency requires all of the following:

1. current `previous_checkpoint_cid` equals the exact prior CheckpointCID;
2. feed identity is unchanged and the new chunk starts at prior sequence + 1;
3. the extension binding carries the exact prior root;
4. the new first leaf links the prior last EventCID/state CID; and
5. the current extension root equals the signed checkpoint root.

Reducer effects are checked through a named implementation of
`CheckpointEffectVerifier`. Model output or a root match cannot replace this
check. Missing history, key state or effect verifier remains explicitly
unresolved. A mismatched history/effect proof is rejected.

## 5. Suppression assessment

`AUTHORIZED_RELATIVE` is emitted only when:

- the checkpoint signature/feed binding was validated;
- key-state frontier and exact state root match a reducer-produced
  `KeyStateCheckpointProof`;
- the feed is authorized relative to that same frontier;
- the history witness reproduces every checkpoint commitment; and
- every reducer effect validates under the exact reducer version.

For an extension chunk, the caller must additionally present the unforgeable
suppression token of the exact previous CheckpointCID and pass consistency
validation. Missing it yields `UnresolvedPreviousCheckpoint`.

Successful assessment carries a privately constructed
`ValidatedCheckpointSuppression` bound to the exact CheckpointCID/covered
position; downstream crates cannot manufacture an authorized enum value. Even
this token returns no deletion authority. Deletion is a separate CHK-004…006
local-policy workflow.

## 6. Conflict and partition behavior

`CheckpointRegister` retains every CheckpointCID. Same feed/position with
different covered roots produces a deterministic `CheckpointConflictProof`
containing all CIDs and roots; there is no arrival-order winner. Multiple
records with the same root are retained as parallel evidence without inventing
a conflict.

Missing proof/head is not a statement that a KU or event is wrong. Bytes remain
available for later reconciliation and proof reevaluation.

## 7. Executable evidence

Five tests prove:

- signature alone grants no suppression and no deletion;
- exact inclusion proof tampering is rejected;
- current history proves prior-prefix consistency;
- same-position/different-root records produce a branch-preserving conflict;
- foreign or stale key-state evidence cannot authorize suppression.

//! Rebuildable single-writer feed projection over validated signed events.

use std::collections::{BTreeMap, BTreeSet};
use std::ops::Bound::{Excluded, Unbounded};

use super::content_id::EventCid;
use super::event::ValidatedKnowledgeEvent;
use super::identity::FeedId;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FeedInsertOutcome {
    Inserted,
    ExactReplay,
    EquivocationObserved,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct SequenceRange {
    pub first: u64,
    pub last: u64,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct FeedEquivocationProof {
    pub feed_id: FeedId,
    pub sequence: u64,
    pub event_cids: Vec<EventCid>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct FeedSuccessorProof {
    pub feed_id: FeedId,
    pub predecessor: EventCid,
    pub successor: EventCid,
    pub successor_sequence: u64,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct UnresolvedFeedPosition {
    pub sequence: u64,
    pub event_cids: Vec<EventCid>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct FeedProjection {
    pub feed_id: FeedId,
    pub contiguous_through: Option<u64>,
    pub contiguous_tips: Vec<EventCid>,
    pub gaps: Vec<SequenceRange>,
    pub unresolved_consistency: Vec<UnresolvedFeedPosition>,
    pub equivocations: Vec<FeedEquivocationProof>,
    pub successor_proofs: Vec<FeedSuccessorProof>,
}

impl FeedProjection {
    pub fn is_complete_through(&self, sequence: u64) -> bool {
        self.contiguous_through.is_some_and(|head| head >= sequence)
    }
}

#[derive(Default)]
pub struct ValidatedFeedStore {
    feeds: BTreeMap<FeedId, BTreeMap<u64, BTreeMap<[u8; 32], ValidatedKnowledgeEvent>>>,
}

impl ValidatedFeedStore {
    pub fn insert(&mut self, event: ValidatedKnowledgeEvent) -> FeedInsertOutcome {
        let feed_id = event.signed.event.author_feed;
        let sequence = event.signed.event.author_sequence;
        let cid = event.cid().into_bytes();
        let position = self
            .feeds
            .entry(feed_id)
            .or_default()
            .entry(sequence)
            .or_default();
        if position.contains_key(&cid) {
            return FeedInsertOutcome::ExactReplay;
        }
        let equivocation = !position.is_empty();
        position.insert(cid, event);
        if equivocation {
            FeedInsertOutcome::EquivocationObserved
        } else {
            FeedInsertOutcome::Inserted
        }
    }

    pub fn event(&self, cid: EventCid) -> Option<&ValidatedKnowledgeEvent> {
        self.feeds
            .values()
            .flat_map(BTreeMap::values)
            .find_map(|position| position.get(cid.as_bytes()))
    }

    pub fn projection(&self, feed_id: FeedId) -> FeedProjection {
        let Some(positions) = self.feeds.get(&feed_id) else {
            return FeedProjection {
                feed_id,
                contiguous_through: None,
                contiguous_tips: Vec::new(),
                gaps: Vec::new(),
                unresolved_consistency: Vec::new(),
                equivocations: Vec::new(),
                successor_proofs: Vec::new(),
            };
        };

        let gaps = missing_ranges(positions);
        let mut reachable: BTreeSet<[u8; 32]> = BTreeSet::new();
        let mut contiguous_through = None;
        let mut unresolved_consistency = Vec::new();
        let mut successor_proofs = Vec::new();

        let mut expected_sequence = 0u64;
        for (&sequence, events) in positions {
            if sequence != expected_sequence {
                break;
            }
            if sequence == 0 {
                reachable.extend(events.keys().copied());
                contiguous_through = Some(0);
                expected_sequence = 1;
                continue;
            }

            let previous = reachable;
            let mut next_reachable = BTreeSet::new();
            for (cid, event) in events {
                let linked: Vec<_> = event
                    .signed
                    .event
                    .causal_parents
                    .iter()
                    .filter(|parent| previous.contains(parent.as_bytes()))
                    .copied()
                    .collect();
                if !linked.is_empty() {
                    next_reachable.insert(*cid);
                    successor_proofs.extend(linked.into_iter().map(|predecessor| {
                        FeedSuccessorProof {
                            feed_id,
                            predecessor,
                            successor: event.cid(),
                            successor_sequence: sequence,
                        }
                    }));
                }
            }
            if next_reachable.is_empty() {
                unresolved_consistency.push(UnresolvedFeedPosition {
                    sequence,
                    event_cids: sorted_cids(events.keys().copied()),
                });
                reachable = BTreeSet::new();
                break;
            }
            reachable = next_reachable;
            contiguous_through = Some(sequence);
            let Some(next) = sequence.checked_add(1) else {
                break;
            };
            expected_sequence = next;
        }

        if let Some(head) = contiguous_through {
            for (&sequence, events) in positions.range((Excluded(head), Unbounded)) {
                if !unresolved_consistency
                    .iter()
                    .any(|position| position.sequence == sequence)
                {
                    unresolved_consistency.push(UnresolvedFeedPosition {
                        sequence,
                        event_cids: sorted_cids(events.keys().copied()),
                    });
                }
            }
        } else {
            unresolved_consistency.extend(positions.iter().map(|(&sequence, events)| {
                UnresolvedFeedPosition {
                    sequence,
                    event_cids: sorted_cids(events.keys().copied()),
                }
            }));
        }

        let equivocations = positions
            .iter()
            .filter(|(_, events)| events.len() > 1)
            .map(|(&sequence, events)| FeedEquivocationProof {
                feed_id,
                sequence,
                event_cids: sorted_cids(events.keys().copied()),
            })
            .collect();
        successor_proofs.sort_by(|left, right| {
            left.successor_sequence
                .cmp(&right.successor_sequence)
                .then_with(|| {
                    left.predecessor
                        .as_bytes()
                        .cmp(right.predecessor.as_bytes())
                })
                .then_with(|| left.successor.as_bytes().cmp(right.successor.as_bytes()))
        });

        FeedProjection {
            feed_id,
            contiguous_through,
            contiguous_tips: sorted_cids(reachable),
            gaps,
            unresolved_consistency,
            equivocations,
            successor_proofs,
        }
    }
}

fn sorted_cids(cids: impl IntoIterator<Item = [u8; 32]>) -> Vec<EventCid> {
    cids.into_iter().map(EventCid::from_bytes).collect()
}

fn missing_ranges(
    positions: &BTreeMap<u64, BTreeMap<[u8; 32], ValidatedKnowledgeEvent>>,
) -> Vec<SequenceRange> {
    let mut ranges = Vec::new();
    let mut expected = 0u64;
    for &sequence in positions.keys() {
        if sequence > expected {
            ranges.push(SequenceRange {
                first: expected,
                last: sequence - 1,
            });
        }
        let Some(next) = sequence.checked_add(1) else {
            break;
        };
        expected = next;
    }
    ranges
}

#[cfg(test)]
mod tests {
    use ed25519_dalek::SigningKey;

    use super::*;
    use crate::foundation::{
        decode_feed_inception, decode_knowledge_event, DisclosureClass, EventType, FeedInception,
        KnowledgeEventEnvelope, NamespaceCommitment,
    };

    const KNOWN_EVENT: EventType = EventType(1);

    fn author() -> (SigningKey, super::super::feed::ValidatedFeedInception) {
        let key = SigningKey::from_bytes(&[1; 32]);
        let feed = FeedInception::new(
            *key.verifying_key().as_bytes(),
            NamespaceCommitment::derive(b"feed-store", [2; 32]).unwrap(),
            0,
            super::super::identity::DeviceId::from_bytes([3; 32]),
        )
        .sign(&key)
        .unwrap();
        (key, decode_feed_inception(&feed.encode().unwrap()).unwrap())
    }

    fn event(
        key: &SigningKey,
        author: &super::super::feed::ValidatedFeedInception,
        sequence: u64,
        marker: u8,
        parents: Vec<EventCid>,
    ) -> ValidatedKnowledgeEvent {
        let mut event = KnowledgeEventEnvelope::new(
            KNOWN_EVENT,
            author.feed_id,
            sequence,
            DisclosureClass::Public,
            [marker; 32],
        );
        event.causal_parents = parents;
        let (bytes, _) = event.sign(author, key).unwrap().encode().unwrap();
        decode_knowledge_event(&bytes, author, &[KNOWN_EVENT]).unwrap()
    }

    #[test]
    fn gap_and_out_of_order_event_resolve_after_predecessor_arrives() {
        let (key, author) = author();
        let first = event(&key, &author, 0, 10, vec![]);
        let second = event(&key, &author, 1, 11, vec![first.cid()]);
        let mut store = ValidatedFeedStore::default();
        store.insert(second);
        let before = store.projection(author.feed_id);
        assert_eq!(before.contiguous_through, None);
        assert_eq!(before.gaps, vec![SequenceRange { first: 0, last: 0 }]);
        assert_eq!(before.unresolved_consistency.len(), 1);

        store.insert(first);
        let after = store.projection(author.feed_id);
        assert_eq!(after.contiguous_through, Some(1));
        assert!(after.gaps.is_empty());
        assert!(after.unresolved_consistency.is_empty());
        assert_eq!(after.successor_proofs.len(), 1);
    }

    #[test]
    fn same_position_different_event_keeps_both_as_equivocation_proof() {
        let (key, author) = author();
        let left = event(&key, &author, 0, 10, vec![]);
        let right = event(&key, &author, 0, 11, vec![]);
        let left_cid = left.cid();
        let right_cid = right.cid();
        let mut store = ValidatedFeedStore::default();
        assert_eq!(store.insert(left), FeedInsertOutcome::Inserted);
        assert_eq!(store.insert(right), FeedInsertOutcome::EquivocationObserved);
        let projection = store.projection(author.feed_id);
        assert_eq!(projection.equivocations.len(), 1);
        assert_eq!(projection.equivocations[0].event_cids.len(), 2);
        assert!(store.event(left_cid).is_some());
        assert!(store.event(right_cid).is_some());
    }

    #[test]
    fn exact_replay_is_idempotent() {
        let (key, author) = author();
        let first = event(&key, &author, 0, 10, vec![]);
        let mut store = ValidatedFeedStore::default();
        assert_eq!(store.insert(first.clone()), FeedInsertOutcome::Inserted);
        assert_eq!(store.insert(first), FeedInsertOutcome::ExactReplay);
        assert!(store.projection(author.feed_id).equivocations.is_empty());
    }

    #[test]
    fn missing_consistency_is_unresolved_not_an_accusation() {
        let (key, author) = author();
        let first = event(&key, &author, 0, 10, vec![]);
        let unlinked = event(&key, &author, 1, 11, vec![EventCid::from_bytes([99; 32])]);
        let mut store = ValidatedFeedStore::default();
        store.insert(first);
        store.insert(unlinked);
        let projection = store.projection(author.feed_id);
        assert_eq!(projection.contiguous_through, Some(0));
        assert_eq!(projection.unresolved_consistency.len(), 1);
        assert!(projection.equivocations.is_empty());
    }

    #[test]
    fn insertion_order_converges_to_the_same_projection() {
        let (key, author) = author();
        let first = event(&key, &author, 0, 10, vec![]);
        let second = event(&key, &author, 1, 11, vec![first.cid()]);
        let alternate = event(&key, &author, 1, 12, vec![first.cid()]);
        let mut left = ValidatedFeedStore::default();
        let mut right = ValidatedFeedStore::default();
        for item in [first.clone(), second.clone(), alternate.clone()] {
            left.insert(item);
        }
        for item in [alternate, second, first] {
            right.insert(item);
        }
        assert_eq!(
            left.projection(author.feed_id),
            right.projection(author.feed_id)
        );
    }

    #[test]
    fn sparse_max_sequence_does_not_expand_into_an_unbounded_loop() {
        let (key, author) = author();
        let sparse = event(&key, &author, u64::MAX, 10, vec![]);
        let mut store = ValidatedFeedStore::default();
        store.insert(sparse);
        let projection = store.projection(author.feed_id);
        assert_eq!(
            projection.gaps,
            vec![SequenceRange {
                first: 0,
                last: u64::MAX - 1,
            }]
        );
        assert_eq!(projection.unresolved_consistency.len(), 1);
    }
}

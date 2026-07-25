//! Bounded M4 runtime for explicit Public UseEvidence exchange.
//!
//! The sender uses a transactional logical outbox. The receiver rebuilds a
//! policy/frontier-relative metabolic view from typed, signed, durable records.
//! Nothing in this module changes wallet or OBT state.

#![cfg(feature = "vnext-network-runtime")]

use std::collections::{BTreeMap, BTreeSet};
use std::net::SocketAddr;
use std::path::Path;

use ed25519_dalek::SigningKey;
use ku_core::foundation::{
    decode_feed_inception, decode_knowledge_event, decode_knowledge_object, event_author_feed,
    AssessedExerciseEvidence, DisclosureClass, EventCid, ExerciseAuthority, ExerciseEvidence,
    FeedAuthorityDecision, FeedId, KnowledgeEventEnvelope, KnownObjectKind,
    MetabolicEvidenceFrontier, MetabolicEvidenceReducer, MetabolicEvidenceView,
    MetabolicViewPolicy, NamespaceCommitment, NodeId, ObjectCid, ObjectReference, ObjectSemantics,
    ResourceProfile, SelectorCid, UseEvidencePayload, ValidatedKnowledgeEvent,
    ValidatedUseEvidenceEvent, USE_EVIDENCE_EVENT_TYPE, USE_EVIDENCE_KIND,
};
use onebrain_protocol::ReconcileManifestKind;
use redb::{Database, ReadableTable, ReadableTableMetadata, TableDefinition};
use serde::{Deserialize, Serialize};
use thiserror::Error;

use crate::vnext_network_runtime::{VNextNetworkRuntime, VNextNetworkRuntimeError};
use crate::vnext_outbox::{OutboundTransferIntent, OutboxEnqueueOutcome};

const PUBLICATIONS: TableDefinition<&[u8], &[u8]> =
    TableDefinition::new("vnext_public_use_publications_v1");
const FEED_HEADS: TableDefinition<&[u8], &[u8]> =
    TableDefinition::new("vnext_public_use_feed_heads_v1");
const USE_IDENTITIES: TableDefinition<&[u8], &[u8]> =
    TableDefinition::new("vnext_received_use_identities_v1");
const VIEW_HEADS: TableDefinition<&[u8], &[u8]> =
    TableDefinition::new("vnext_distributed_pomv_view_heads_v1");
const PUBLICATION_SCHEMA: u64 = 1;
const PUBLICATION_KEY_BYTES: usize = 64;
const USE_IDENTITY_BYTES: usize = 72;
const VIEW_LINEAGE_KEY_BYTES: usize = 80;
const VIEW_HEAD_VALUE_BYTES: usize = 73;
const MAX_PUBLICATIONS: u64 = 65_536;
const MAX_FLUSH_BATCH: usize = 4_096;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ExplicitUseConfirmation {
    commitment: [u8; 32],
}

impl ExplicitUseConfirmation {
    pub fn new(commitment: [u8; 32]) -> Result<Self, DistributedPomvError> {
        if commitment == [0; 32] {
            return Err(DistributedPomvError::ConfirmationRequired);
        }
        Ok(Self { commitment })
    }

    pub const fn commitment(self) -> [u8; 32] {
        self.commitment
    }
}

#[derive(Clone, Debug)]
pub struct PublishPublicUseEvidenceRequest {
    pub payload: UseEvidencePayload,
    pub expected_peer: NodeId,
    pub last_known_addr: SocketAddr,
    pub selector: SelectorCid,
    pub namespace: NamespaceCommitment,
    pub idempotency_key: [u8; 32],
    pub confirmation: ExplicitUseConfirmation,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PublicUsePublishOutcome {
    Stored,
    ExactReplay,
    RouteUpdated,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PublicUseEvidencePublication {
    pub publication_id: [u8; 32],
    pub author_feed: FeedId,
    pub author_sequence: u64,
    pub idempotency_key: [u8; 32],
    pub feed_bytes: Vec<u8>,
    pub object_bytes: Vec<u8>,
    pub object_cid: ObjectCid,
    pub event_bytes: Vec<u8>,
    pub event_cid: EventCid,
    pub expected_peer: NodeId,
    pub last_known_addr: SocketAddr,
    pub selector: SelectorCid,
    pub namespace: NamespaceCommitment,
}

impl PublicUseEvidencePublication {
    pub fn transfer_intents(&self) -> Result<Vec<OutboundTransferIntent>, DistributedPomvError> {
        [
            (ReconcileManifestKind::FeedInception, &self.feed_bytes),
            (ReconcileManifestKind::Object, &self.object_bytes),
            (ReconcileManifestKind::Event, &self.event_bytes),
        ]
        .into_iter()
        .map(|(kind, bytes)| {
            OutboundTransferIntent::new(
                self.expected_peer,
                self.last_known_addr,
                self.selector,
                self.namespace,
                DisclosureClass::Public,
                kind,
                bytes.clone(),
            )
            .map_err(|error| DistributedPomvError::Outbox(error.to_string()))
        })
        .collect()
    }
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct PublicUseFlushReport {
    pub scanned_publications: u64,
    pub exported_publications: u64,
    pub added_intents: u64,
    pub existing_intents: u64,
    pub route_updated_intents: u64,
}

#[derive(Serialize, Deserialize)]
struct StoredPublication {
    schema: u64,
    publication_id: [u8; 32],
    author_feed: [u8; 32],
    author_sequence: u64,
    idempotency_key: [u8; 32],
    confirmation_commitment: [u8; 32],
    expected_peer: [u8; 32],
    last_known_addr: String,
    selector: [u8; 32],
    namespace: [u8; 32],
    feed_bytes: Vec<u8>,
    object_bytes: Vec<u8>,
    event_bytes: Vec<u8>,
    exported_to_network_outbox: bool,
}

#[derive(Serialize, Deserialize)]
struct StoredFeedHead {
    next_sequence: u64,
    last_event_cid: Option<[u8; 32]>,
}

pub struct PublicUseEvidencePublisher {
    database: Database,
}

impl PublicUseEvidencePublisher {
    pub fn open(data_dir: &Path) -> Result<Self, DistributedPomvError> {
        std::fs::create_dir_all(data_dir)?;
        let database =
            Database::create(data_dir.join("vnext_public_use_sender.redb")).map_err(storage)?;
        let write = database.begin_write().map_err(storage)?;
        {
            write.open_table(PUBLICATIONS).map_err(storage)?;
            write.open_table(FEED_HEADS).map_err(storage)?;
        }
        write.commit().map_err(storage)?;
        Ok(Self { database })
    }

    pub fn publish_confirmed(
        &self,
        request: &PublishPublicUseEvidenceRequest,
        author: &ku_core::foundation::ValidatedFeedInception,
        signing_key: &SigningKey,
    ) -> Result<(PublicUseEvidencePublication, PublicUsePublishOutcome), DistributedPomvError> {
        if request.idempotency_key == [0; 32]
            || request.expected_peer.as_bytes() == &[0; 32]
            || request.confirmation.commitment == [0; 32]
        {
            return Err(DistributedPomvError::InvalidPublishRequest);
        }
        let (object_bytes, object_cid) = request
            .payload
            .to_knowledge_object(DisclosureClass::Public)
            .map_err(|error| DistributedPomvError::Evidence(format!("{error:?}")))?
            .encode(ResourceProfile::ObjectV1)
            .map_err(|error| DistributedPomvError::Evidence(error.to_string()))?;
        let feed_bytes = author.original_bytes().to_vec();
        let key = publication_key(author.feed_id, request.idempotency_key);

        let write = self.database.begin_write().map_err(storage)?;
        let outcome;
        let stored;
        {
            let mut publications = write.open_table(PUBLICATIONS).map_err(storage)?;
            let existing = publications
                .get(key.as_slice())
                .map_err(storage)?
                .map(|value| value.value().to_vec());
            if let Some(bytes) = existing {
                let mut replay = decode_stored_publication(&bytes)?;
                validate_replay(&replay, request, author, &feed_bytes, &object_bytes)?;
                let requested_addr = request.last_known_addr.to_string();
                if replay.last_known_addr == requested_addr {
                    outcome = PublicUsePublishOutcome::ExactReplay;
                } else {
                    replay.last_known_addr = requested_addr;
                    replay.exported_to_network_outbox = false;
                    let encoded = encode_stored_publication(&replay)?;
                    publications
                        .insert(key.as_slice(), encoded.as_slice())
                        .map_err(storage)?;
                    outcome = PublicUsePublishOutcome::RouteUpdated;
                }
                stored = replay;
            } else {
                if publications.len().map_err(storage)? >= MAX_PUBLICATIONS {
                    return Err(DistributedPomvError::PublicationLimit);
                }
                let mut heads = write.open_table(FEED_HEADS).map_err(storage)?;
                let head = heads
                    .get(author.feed_id.as_bytes().as_slice())
                    .map_err(storage)?
                    .map(|value| serde_json::from_slice::<StoredFeedHead>(value.value()))
                    .transpose()
                    .map_err(codec)?
                    .unwrap_or(StoredFeedHead {
                        next_sequence: 0,
                        last_event_cid: None,
                    });
                let mut event = KnowledgeEventEnvelope::new(
                    USE_EVIDENCE_EVENT_TYPE,
                    author.feed_id,
                    head.next_sequence,
                    DisclosureClass::Public,
                    request.idempotency_key,
                );
                event.payload_refs = vec![ObjectReference::new(0, object_cid.into_bytes())];
                event.causal_parents = head
                    .last_event_cid
                    .map(EventCid::from_bytes)
                    .into_iter()
                    .collect();
                let (event_bytes, event_cid) = event
                    .sign(author, signing_key)
                    .map_err(|error| DistributedPomvError::Evidence(error.to_string()))?
                    .encode()
                    .map_err(|error| DistributedPomvError::Evidence(error.to_string()))?;
                let publication_id = publication_id(
                    author.feed_id,
                    request.idempotency_key,
                    event_cid,
                    request.expected_peer,
                    request.selector,
                    request.namespace,
                );
                let value = StoredPublication {
                    schema: PUBLICATION_SCHEMA,
                    publication_id,
                    author_feed: *author.feed_id.as_bytes(),
                    author_sequence: head.next_sequence,
                    idempotency_key: request.idempotency_key,
                    confirmation_commitment: request.confirmation.commitment,
                    expected_peer: *request.expected_peer.as_bytes(),
                    last_known_addr: request.last_known_addr.to_string(),
                    selector: *request.selector.as_bytes(),
                    namespace: *request.namespace.as_bytes(),
                    feed_bytes,
                    object_bytes,
                    event_bytes,
                    exported_to_network_outbox: false,
                };
                validate_stored_publication(&value)?;
                let encoded = encode_stored_publication(&value)?;
                publications
                    .insert(key.as_slice(), encoded.as_slice())
                    .map_err(storage)?;
                let next = head
                    .next_sequence
                    .checked_add(1)
                    .ok_or(DistributedPomvError::SequenceExhausted)?;
                let head = StoredFeedHead {
                    next_sequence: next,
                    last_event_cid: Some(event_cid.into_bytes()),
                };
                let encoded_head = serde_json::to_vec(&head).map_err(codec)?;
                heads
                    .insert(
                        author.feed_id.as_bytes().as_slice(),
                        encoded_head.as_slice(),
                    )
                    .map_err(storage)?;
                stored = value;
                outcome = PublicUsePublishOutcome::Stored;
            }
        }
        write.commit().map_err(storage)?;
        Ok((publication_from_stored(&stored)?, outcome))
    }

    pub fn pending_publication_count(&self) -> Result<u64, DistributedPomvError> {
        let read = self.database.begin_read().map_err(storage)?;
        let table = read.open_table(PUBLICATIONS).map_err(storage)?;
        let mut pending = 0u64;
        for entry in table.iter().map_err(storage)? {
            let (_, value) = entry.map_err(storage)?;
            if !decode_stored_publication(value.value())?.exported_to_network_outbox {
                pending = pending.saturating_add(1);
            }
        }
        Ok(pending)
    }

    pub fn flush_pending(
        &self,
        network: &VNextNetworkRuntime,
        limit: usize,
    ) -> Result<PublicUseFlushReport, DistributedPomvError> {
        if limit == 0 || limit > MAX_FLUSH_BATCH {
            return Err(DistributedPomvError::InvalidLimit);
        }
        let read = self.database.begin_read().map_err(storage)?;
        let table = read.open_table(PUBLICATIONS).map_err(storage)?;
        let mut pending = Vec::<([u8; PUBLICATION_KEY_BYTES], StoredPublication)>::new();
        for entry in table.iter().map_err(storage)? {
            let (key, value) = entry.map_err(storage)?;
            let stored = decode_stored_publication(value.value())?;
            if !stored.exported_to_network_outbox {
                let key: [u8; PUBLICATION_KEY_BYTES] = key
                    .value()
                    .try_into()
                    .map_err(|_| DistributedPomvError::CorruptPublication)?;
                pending.push((key, stored));
                if pending.len() == limit {
                    break;
                }
            }
        }
        drop(table);
        drop(read);

        let mut report = PublicUseFlushReport::default();
        for (key, stored) in pending {
            report.scanned_publications = report.scanned_publications.saturating_add(1);
            let publication = publication_from_stored(&stored)?;
            for intent in publication.transfer_intents()? {
                match network.enqueue_outbound(&intent)? {
                    OutboxEnqueueOutcome::Added => {
                        report.added_intents = report.added_intents.saturating_add(1)
                    }
                    OutboxEnqueueOutcome::Existing => {
                        report.existing_intents = report.existing_intents.saturating_add(1)
                    }
                    OutboxEnqueueOutcome::RouteUpdated => {
                        report.route_updated_intents =
                            report.route_updated_intents.saturating_add(1)
                    }
                }
            }
            self.mark_exported(key)?;
            report.exported_publications = report.exported_publications.saturating_add(1);
        }
        Ok(report)
    }

    fn mark_exported(&self, key: [u8; PUBLICATION_KEY_BYTES]) -> Result<(), DistributedPomvError> {
        let write = self.database.begin_write().map_err(storage)?;
        {
            let mut table = write.open_table(PUBLICATIONS).map_err(storage)?;
            let bytes = table
                .get(key.as_slice())
                .map_err(storage)?
                .map(|value| value.value().to_vec())
                .ok_or(DistributedPomvError::CorruptPublication)?;
            let mut stored = decode_stored_publication(&bytes)?;
            stored.exported_to_network_outbox = true;
            let encoded = encode_stored_publication(&stored)?;
            table
                .insert(key.as_slice(), encoded.as_slice())
                .map_err(storage)?;
        }
        write.commit().map_err(storage)
    }
}

fn publication_key(feed: FeedId, idempotency_key: [u8; 32]) -> [u8; PUBLICATION_KEY_BYTES] {
    let mut key = [0u8; PUBLICATION_KEY_BYTES];
    key[..32].copy_from_slice(feed.as_bytes());
    key[32..].copy_from_slice(&idempotency_key);
    key
}

fn publication_id(
    feed: FeedId,
    idempotency_key: [u8; 32],
    event: EventCid,
    peer: NodeId,
    selector: SelectorCid,
    namespace: NamespaceCommitment,
) -> [u8; 32] {
    let mut hasher = blake3::Hasher::new();
    hasher.update(b"onebrain:vnext:public-use-publication:1\0");
    hasher.update(feed.as_bytes());
    hasher.update(&idempotency_key);
    hasher.update(event.as_bytes());
    hasher.update(peer.as_bytes());
    hasher.update(selector.as_bytes());
    hasher.update(namespace.as_bytes());
    *hasher.finalize().as_bytes()
}

fn encode_stored_publication(value: &StoredPublication) -> Result<Vec<u8>, DistributedPomvError> {
    serde_json::to_vec(value).map_err(codec)
}

fn decode_stored_publication(bytes: &[u8]) -> Result<StoredPublication, DistributedPomvError> {
    let value = serde_json::from_slice(bytes).map_err(codec)?;
    validate_stored_publication(&value)?;
    Ok(value)
}

fn validate_stored_publication(value: &StoredPublication) -> Result<(), DistributedPomvError> {
    if value.schema != PUBLICATION_SCHEMA
        || value.idempotency_key == [0; 32]
        || value.confirmation_commitment == [0; 32]
        || value.expected_peer == [0; 32]
    {
        return Err(DistributedPomvError::CorruptPublication);
    }
    let feed = decode_feed_inception(&value.feed_bytes)
        .map_err(|_| DistributedPomvError::CorruptPublication)?;
    if feed.feed_id.as_bytes() != &value.author_feed {
        return Err(DistributedPomvError::CorruptPublication);
    }
    let object = decode_knowledge_object(
        &value.object_bytes,
        ResourceProfile::ObjectV1,
        &[KnownObjectKind::new(USE_EVIDENCE_KIND, 1)],
        &[],
    )
    .map_err(|_| DistributedPomvError::CorruptPublication)?;
    if object.disclosure() != DisclosureClass::Public
        || UseEvidencePayload::from_validated_object(&object).is_err()
    {
        return Err(DistributedPomvError::CorruptPublication);
    }
    let event = decode_knowledge_event(&value.event_bytes, &feed, &[USE_EVIDENCE_EVENT_TYPE])
        .map_err(|_| DistributedPomvError::CorruptPublication)?;
    if event.signed.event.author_sequence != value.author_sequence
        || event.signed.event.idempotency_key != value.idempotency_key
        || ValidatedUseEvidenceEvent::bind(&event, &object).is_err()
    {
        return Err(DistributedPomvError::CorruptPublication);
    }
    let peer = NodeId::from_bytes(value.expected_peer);
    let selector = SelectorCid::from_bytes(value.selector);
    let namespace = NamespaceCommitment::from_bytes(value.namespace);
    let expected_id = publication_id(
        feed.feed_id,
        value.idempotency_key,
        event.cid(),
        peer,
        selector,
        namespace,
    );
    if expected_id != value.publication_id || value.last_known_addr.parse::<SocketAddr>().is_err() {
        return Err(DistributedPomvError::CorruptPublication);
    }
    Ok(())
}

fn validate_replay(
    stored: &StoredPublication,
    request: &PublishPublicUseEvidenceRequest,
    author: &ku_core::foundation::ValidatedFeedInception,
    feed_bytes: &[u8],
    object_bytes: &[u8],
) -> Result<(), DistributedPomvError> {
    if stored.author_feed != *author.feed_id.as_bytes()
        || stored.idempotency_key != request.idempotency_key
        || stored.confirmation_commitment != request.confirmation.commitment
        || stored.expected_peer != *request.expected_peer.as_bytes()
        || stored.selector != *request.selector.as_bytes()
        || stored.namespace != *request.namespace.as_bytes()
        || stored.feed_bytes != feed_bytes
        || stored.object_bytes != object_bytes
    {
        return Err(DistributedPomvError::IdempotencyConflict);
    }
    Ok(())
}

fn publication_from_stored(
    stored: &StoredPublication,
) -> Result<PublicUseEvidencePublication, DistributedPomvError> {
    let feed = decode_feed_inception(&stored.feed_bytes)
        .map_err(|_| DistributedPomvError::CorruptPublication)?;
    let object = decode_knowledge_object(
        &stored.object_bytes,
        ResourceProfile::ObjectV1,
        &[KnownObjectKind::new(USE_EVIDENCE_KIND, 1)],
        &[],
    )
    .map_err(|_| DistributedPomvError::CorruptPublication)?;
    let event = decode_knowledge_event(&stored.event_bytes, &feed, &[USE_EVIDENCE_EVENT_TYPE])
        .map_err(|_| DistributedPomvError::CorruptPublication)?;
    Ok(PublicUseEvidencePublication {
        publication_id: stored.publication_id,
        author_feed: feed.feed_id,
        author_sequence: stored.author_sequence,
        idempotency_key: stored.idempotency_key,
        feed_bytes: stored.feed_bytes.clone(),
        object_bytes: stored.object_bytes.clone(),
        object_cid: object.cid(),
        event_bytes: stored.event_bytes.clone(),
        event_cid: event.cid(),
        expected_peer: NodeId::from_bytes(stored.expected_peer),
        last_known_addr: stored
            .last_known_addr
            .parse()
            .map_err(|_| DistributedPomvError::CorruptPublication)?,
        selector: SelectorCid::from_bytes(stored.selector),
        namespace: NamespaceCommitment::from_bytes(stored.namespace),
    })
}

const MAX_IDENTITY_VARIANTS: usize = 64;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum IdentityObserveOutcome {
    Added,
    ExactReplay,
    ConflictObserved,
}

struct DurableUseIdentityIndex {
    database: Database,
}

impl DurableUseIdentityIndex {
    fn open(path: &Path) -> Result<Self, DistributedPomvError> {
        let database = Database::create(path).map_err(storage)?;
        let write = database.begin_write().map_err(storage)?;
        {
            write.open_table(USE_IDENTITIES).map_err(storage)?;
            write.open_table(VIEW_HEADS).map_err(storage)?;
        }
        write.commit().map_err(storage)?;
        Ok(Self { database })
    }

    fn observe(
        &self,
        identity: [u8; USE_IDENTITY_BYTES],
        event: EventCid,
    ) -> Result<IdentityObserveOutcome, DistributedPomvError> {
        let write = self.database.begin_write().map_err(storage)?;
        let outcome;
        {
            let mut table = write.open_table(USE_IDENTITIES).map_err(storage)?;
            let existing = table
                .get(identity.as_slice())
                .map_err(storage)?
                .map(|value| value.value().to_vec());
            let (mut overflowed, mut cids) = existing
                .as_deref()
                .map(decode_identity_value)
                .transpose()?
                .unwrap_or((false, Vec::new()));
            if cids.binary_search(&event.into_bytes()).is_ok() {
                outcome = IdentityObserveOutcome::ExactReplay;
            } else {
                if cids.len() < MAX_IDENTITY_VARIANTS {
                    cids.push(event.into_bytes());
                    cids.sort();
                } else {
                    overflowed = true;
                }
                outcome = if cids.len() == 1 && !overflowed {
                    IdentityObserveOutcome::Added
                } else {
                    IdentityObserveOutcome::ConflictObserved
                };
                let encoded = encode_identity_value(overflowed, &cids);
                table
                    .insert(identity.as_slice(), encoded.as_slice())
                    .map_err(storage)?;
            }
        }
        write.commit().map_err(storage)?;
        Ok(outcome)
    }

    fn state(
        &self,
        identity: [u8; USE_IDENTITY_BYTES],
    ) -> Result<(bool, Vec<[u8; 32]>), DistributedPomvError> {
        let read = self.database.begin_read().map_err(storage)?;
        let table = read.open_table(USE_IDENTITIES).map_err(storage)?;
        table
            .get(identity.as_slice())
            .map_err(storage)?
            .map(|value| decode_identity_value(value.value()))
            .transpose()
            .map(|value| value.unwrap_or((false, Vec::new())))
    }

    fn apply_view_lineage(
        &self,
        target: &ObjectReference,
        policy: &ObjectReference,
        root: [u8; 32],
    ) -> Result<(u64, Option<[u8; 32]>), DistributedPomvError> {
        let key = view_lineage_key(target, policy);
        let write = self.database.begin_write().map_err(storage)?;
        let result;
        {
            let mut table = write.open_table(VIEW_HEADS).map_err(storage)?;
            let existing = table
                .get(key.as_slice())
                .map_err(storage)?
                .map(|value| decode_view_head(value.value()))
                .transpose()?;
            let head = match existing {
                Some(head) if head.root == root => head,
                Some(head) => StoredViewHead {
                    revision: head
                        .revision
                        .checked_add(1)
                        .ok_or(DistributedPomvError::ViewRevisionExhausted)?,
                    root,
                    previous: Some(head.root),
                },
                None => StoredViewHead {
                    revision: 1,
                    root,
                    previous: None,
                },
            };
            let encoded = encode_view_head(head);
            table
                .insert(key.as_slice(), encoded.as_slice())
                .map_err(storage)?;
            result = (head.revision, head.previous);
        }
        write.commit().map_err(storage)?;
        Ok(result)
    }
}

#[derive(Clone, Copy)]
struct StoredViewHead {
    revision: u64,
    root: [u8; 32],
    previous: Option<[u8; 32]>,
}

fn view_lineage_key(
    target: &ObjectReference,
    policy: &ObjectReference,
) -> [u8; VIEW_LINEAGE_KEY_BYTES] {
    let mut key = [0u8; VIEW_LINEAGE_KEY_BYTES];
    key[..8].copy_from_slice(&target.reference_kind.to_be_bytes());
    key[8..40].copy_from_slice(&target.cid);
    key[40..48].copy_from_slice(&policy.reference_kind.to_be_bytes());
    key[48..].copy_from_slice(&policy.cid);
    key
}

fn encode_view_head(head: StoredViewHead) -> [u8; VIEW_HEAD_VALUE_BYTES] {
    let mut value = [0u8; VIEW_HEAD_VALUE_BYTES];
    value[..8].copy_from_slice(&head.revision.to_be_bytes());
    value[8..40].copy_from_slice(&head.root);
    if let Some(previous) = head.previous {
        value[40] = 1;
        value[41..].copy_from_slice(&previous);
    }
    value
}

fn decode_view_head(value: &[u8]) -> Result<StoredViewHead, DistributedPomvError> {
    if value.len() != VIEW_HEAD_VALUE_BYTES || !matches!(value[40], 0 | 1) {
        return Err(DistributedPomvError::IdentityIndexCorrupt);
    }
    let revision = u64::from_be_bytes(
        value[..8]
            .try_into()
            .map_err(|_| DistributedPomvError::IdentityIndexCorrupt)?,
    );
    let root = value[8..40]
        .try_into()
        .map_err(|_| DistributedPomvError::IdentityIndexCorrupt)?;
    let previous = (value[40] == 1)
        .then(|| {
            value[41..]
                .try_into()
                .map_err(|_| DistributedPomvError::IdentityIndexCorrupt)
        })
        .transpose()?;
    if revision == 0 || root == [0; 32] || (value[40] == 0 && value[41..] != [0; 32]) {
        return Err(DistributedPomvError::IdentityIndexCorrupt);
    }
    Ok(StoredViewHead {
        revision,
        root,
        previous,
    })
}

fn encode_identity_value(overflowed: bool, cids: &[[u8; 32]]) -> Vec<u8> {
    let mut value = Vec::with_capacity(1 + cids.len() * 32);
    value.push(u8::from(overflowed));
    for cid in cids {
        value.extend_from_slice(cid);
    }
    value
}

fn decode_identity_value(value: &[u8]) -> Result<(bool, Vec<[u8; 32]>), DistributedPomvError> {
    if value.is_empty() || !matches!(value[0], 0 | 1) || (value.len() - 1) % 32 != 0 {
        return Err(DistributedPomvError::IdentityIndexCorrupt);
    }
    let mut cids = value[1..]
        .chunks_exact(32)
        .map(|bytes| {
            bytes
                .try_into()
                .map_err(|_| DistributedPomvError::IdentityIndexCorrupt)
        })
        .collect::<Result<Vec<[u8; 32]>, _>>()?;
    if cids.is_empty() || cids.len() > MAX_IDENTITY_VARIANTS {
        return Err(DistributedPomvError::IdentityIndexCorrupt);
    }
    let original = cids.clone();
    cids.sort();
    cids.dedup();
    if cids != original {
        return Err(DistributedPomvError::IdentityIndexCorrupt);
    }
    Ok((value[0] == 1, cids))
}

fn use_identity(feed: FeedId, idempotency_key: [u8; 32]) -> [u8; USE_IDENTITY_BYTES] {
    let mut key = [0u8; USE_IDENTITY_BYTES];
    key[..32].copy_from_slice(feed.as_bytes());
    key[32..40].copy_from_slice(&USE_EVIDENCE_EVENT_TYPE.0.to_be_bytes());
    key[40..].copy_from_slice(&idempotency_key);
    key
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DistributedUseEvidenceObservation {
    pub event_cid: EventCid,
    pub payload_object: ObjectCid,
    pub author_feed: FeedId,
    pub author_sequence: u64,
    pub idempotency_key: [u8; 32],
    pub source_peers: Vec<NodeId>,
    pub authority: ExerciseAuthority,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DistributedPomvReport {
    pub view: MetabolicEvidenceView,
    pub observations: Vec<DistributedUseEvidenceObservation>,
    pub newly_indexed_events: u64,
    pub replayed_index_events: u64,
    pub ignored_non_use_records: u64,
    pub invalid_or_unbound_records: u64,
    pub idempotency_conflicts: u64,
    pub claims_truth: bool,
    pub claims_benefit: bool,
    pub changes_wallet_state: bool,
    pub changes_obt_state: bool,
    pub claims_network_completion: bool,
}

pub struct DistributedPomvRuntime {
    identities: DurableUseIdentityIndex,
    max_records: usize,
}

impl DistributedPomvRuntime {
    pub fn open(data_dir: &Path, max_records: usize) -> Result<Self, DistributedPomvError> {
        std::fs::create_dir_all(data_dir)?;
        // Validate the configured capacity at startup.
        MetabolicEvidenceReducer::new(max_records)
            .map_err(|error| DistributedPomvError::Metabolic(format!("{error:?}")))?;
        Ok(Self {
            identities: DurableUseIdentityIndex::open(
                &data_dir.join("vnext_distributed_pomv.redb"),
            )?,
            max_records,
        })
    }

    pub const fn changes_wallet_state(&self) -> bool {
        false
    }

    pub const fn changes_obt_state(&self) -> bool {
        false
    }

    pub fn materialize_public_use_view(
        &self,
        network: &VNextNetworkRuntime,
        selector: SelectorCid,
        target: ObjectReference,
        policy: &MetabolicViewPolicy,
        authority_frontier: EventCid,
    ) -> Result<DistributedPomvReport, DistributedPomvError> {
        let mut objects = BTreeMap::new();
        let mut ignored_non_use_records = 0u64;
        let mut invalid_or_unbound_records = 0u64;
        for bytes in network.accepted_object_bytes()? {
            let validated = match decode_knowledge_object(
                &bytes,
                ResourceProfile::ObjectV1,
                &[KnownObjectKind::new(USE_EVIDENCE_KIND, 1)],
                &[],
            ) {
                Ok(value) => value,
                Err(_) => {
                    invalid_or_unbound_records = invalid_or_unbound_records.saturating_add(1);
                    continue;
                }
            };
            if !matches!(
                validated.semantics(),
                ObjectSemantics::Known(envelope)
                    if envelope.kind == USE_EVIDENCE_KIND
                        && envelope.disclosure == DisclosureClass::Public
            ) {
                ignored_non_use_records = ignored_non_use_records.saturating_add(1);
                continue;
            }
            if UseEvidencePayload::from_validated_object(&validated).is_err() {
                invalid_or_unbound_records = invalid_or_unbound_records.saturating_add(1);
                continue;
            }
            objects.insert(validated.cid().into_bytes(), validated);
        }

        let mut candidates = Vec::new();
        let mut newly_indexed_events = 0u64;
        let mut replayed_index_events = 0u64;
        for bytes in network.accepted_event_bytes()? {
            let feed_id = match event_author_feed(&bytes) {
                Ok(feed) => feed,
                Err(_) => {
                    invalid_or_unbound_records = invalid_or_unbound_records.saturating_add(1);
                    continue;
                }
            };
            let branches = network.feed_inception_branches(feed_id)?;
            let decisions = network.feed_authority_at(feed_id, authority_frontier)?;
            let mut event = None::<ValidatedKnowledgeEvent>;
            let mut verifying_branches = Vec::new();
            for (index, branch) in branches.iter().enumerate() {
                let Ok(validated) =
                    decode_knowledge_event(&bytes, branch, &[USE_EVIDENCE_EVENT_TYPE])
                else {
                    continue;
                };
                if validated.signed.event.event_type != USE_EVIDENCE_EVENT_TYPE
                    || validated.signed.event.disclosure != DisclosureClass::Public
                {
                    continue;
                }
                verifying_branches.push(index);
                event.get_or_insert(validated);
            }
            let Some(event) = event else {
                ignored_non_use_records = ignored_non_use_records.saturating_add(1);
                continue;
            };
            let Some(payload_ref) = event.signed.event.payload_refs.first() else {
                invalid_or_unbound_records = invalid_or_unbound_records.saturating_add(1);
                continue;
            };
            let Some(payload_object) = objects.get(&payload_ref.cid) else {
                invalid_or_unbound_records = invalid_or_unbound_records.saturating_add(1);
                continue;
            };
            let bound = match ValidatedUseEvidenceEvent::bind(&event, payload_object) {
                Ok(value) => value,
                Err(_) => {
                    invalid_or_unbound_records = invalid_or_unbound_records.saturating_add(1);
                    continue;
                }
            };
            let identity = use_identity(feed_id, event.signed.event.idempotency_key);
            match self.identities.observe(identity, event.cid())? {
                IdentityObserveOutcome::Added | IdentityObserveOutcome::ConflictObserved => {
                    newly_indexed_events = newly_indexed_events.saturating_add(1)
                }
                IdentityObserveOutcome::ExactReplay => {
                    replayed_index_events = replayed_index_events.saturating_add(1)
                }
            }
            let source_peers = network.record_source_peers(
                ReconcileManifestKind::Event,
                event.cid().into_bytes(),
                selector,
            )?;
            if source_peers.is_empty() {
                continue;
            }
            let authority = assess_branch_authority(&verifying_branches, &decisions);
            candidates.push(UseCandidate {
                identity,
                evidence: bound,
                idempotency_key: event.signed.event.idempotency_key,
                source_peers,
                authority,
            });
        }
        candidates.sort_by_key(|candidate| candidate.evidence.event_cid().into_bytes());

        let mut reducer = MetabolicEvidenceReducer::new(self.max_records)
            .map_err(|error| DistributedPomvError::Metabolic(format!("{error:?}")))?;
        let mut observations = Vec::new();
        let mut conflicting_identities = BTreeSet::new();
        for candidate in candidates {
            let (overflowed, variants) = self.identities.state(candidate.identity)?;
            if overflowed
                || variants.len() != 1
                || variants[0] != candidate.evidence.event_cid().into_bytes()
            {
                conflicting_identities.insert(candidate.identity);
                continue;
            }
            reducer.record(AssessedExerciseEvidence {
                evidence: ExerciseEvidence::Use(candidate.evidence.clone()),
                authority: candidate.authority,
            });
            observations.push(DistributedUseEvidenceObservation {
                event_cid: candidate.evidence.event_cid(),
                payload_object: candidate.evidence.payload_object_cid(),
                author_feed: candidate.evidence.author_feed(),
                author_sequence: candidate.evidence.author_sequence(),
                idempotency_key: candidate.idempotency_key,
                source_peers: candidate.source_peers,
                authority: candidate.authority,
            });
        }
        observations.sort_by_key(|observation| observation.event_cid.into_bytes());

        let mut positions = BTreeMap::new();
        for observation in &observations {
            if let Some(position) = network
                .feed_projection(observation.author_feed)?
                .contiguous_through
            {
                positions.insert(observation.author_feed, position);
            }
        }
        let frontier = MetabolicEvidenceFrontier::new(authority_frontier.into_bytes(), positions)
            .map_err(|error| DistributedPomvError::Metabolic(format!("{error:?}")))?;
        let mut view = reducer
            .materialize(target, policy, &frontier)
            .map_err(|error| DistributedPomvError::Metabolic(format!("{error:?}")))?;
        let (revision, previous_view_root) = self.identities.apply_view_lineage(
            &view.target,
            &view.policy.policy_ref,
            view.view_root,
        )?;
        view.revision = revision;
        view.previous_view_root = previous_view_root;
        Ok(DistributedPomvReport {
            view,
            observations,
            newly_indexed_events,
            replayed_index_events,
            ignored_non_use_records,
            invalid_or_unbound_records,
            idempotency_conflicts: conflicting_identities.len() as u64,
            claims_truth: false,
            claims_benefit: false,
            changes_wallet_state: false,
            changes_obt_state: false,
            claims_network_completion: false,
        })
    }
}

struct UseCandidate {
    identity: [u8; USE_IDENTITY_BYTES],
    evidence: ValidatedUseEvidenceEvent,
    idempotency_key: [u8; 32],
    source_peers: Vec<NodeId>,
    authority: ExerciseAuthority,
}

fn assess_branch_authority(
    verifying_branches: &[usize],
    decisions: &[FeedAuthorityDecision],
) -> ExerciseAuthority {
    let applicable = verifying_branches
        .iter()
        .filter_map(|index| decisions.get(*index))
        .collect::<Vec<_>>();
    if applicable
        .iter()
        .any(|decision| matches!(decision, FeedAuthorityDecision::AuthorizedRelative { .. }))
    {
        ExerciseAuthority::Authorized
    } else if applicable.iter().any(|decision| {
        matches!(
            decision,
            FeedAuthorityDecision::QuarantinedRevokedRelative { .. }
        )
    }) {
        ExerciseAuthority::Unauthorized
    } else {
        ExerciseAuthority::Unresolved
    }
}

fn storage(error: impl std::fmt::Display) -> DistributedPomvError {
    DistributedPomvError::Storage(error.to_string())
}

fn codec(error: impl std::fmt::Display) -> DistributedPomvError {
    DistributedPomvError::Codec(error.to_string())
}

#[derive(Debug, Error)]
pub enum DistributedPomvError {
    #[error("explicit use confirmation is required")]
    ConfirmationRequired,
    #[error("public UseEvidence publish request is invalid")]
    InvalidPublishRequest,
    #[error("public UseEvidence publication limit reached")]
    PublicationLimit,
    #[error("public UseEvidence feed sequence exhausted")]
    SequenceExhausted,
    #[error("distributed PoMV view revision exhausted")]
    ViewRevisionExhausted,
    #[error("public UseEvidence idempotency key conflicts with durable content")]
    IdempotencyConflict,
    #[error("public UseEvidence publication is corrupt")]
    CorruptPublication,
    #[error("received UseEvidence identity index is corrupt")]
    IdentityIndexCorrupt,
    #[error("public UseEvidence batch limit is invalid")]
    InvalidLimit,
    #[error("public UseEvidence codec failed: {0}")]
    Codec(String),
    #[error("public UseEvidence storage failed: {0}")]
    Storage(String),
    #[error("public UseEvidence validation failed: {0}")]
    Evidence(String),
    #[error("public UseEvidence network outbox failed: {0}")]
    Outbox(String),
    #[error("metabolic evidence projection failed: {0}")]
    Metabolic(String),
    #[error("vNext network runtime failed: {0}")]
    Network(#[from] VNextNetworkRuntimeError),
    #[error("public UseEvidence filesystem operation failed: {0}")]
    Io(#[from] std::io::Error),
}

#[cfg(test)]
mod tests {
    use std::time::Duration;

    use ku_core::foundation::{
        ActorId, ActorRevocation, ActorRootDelegation, ConceptCcid, DeviceId, FeedInception,
        MetabolicViewLimitation, ReservedDomain, UnresolvedAuthorityReason, UseMode,
    };

    use super::*;
    use crate::vnext_config::VNextNetworkPolicy;
    use crate::vnext_outbox::OutboundIntentState;

    fn reference(byte: u8) -> ObjectReference {
        ObjectReference::new(0, [byte; 32])
    }

    fn payload(target: ObjectReference, policy: ObjectReference, marker: u8) -> UseEvidencePayload {
        UseEvidencePayload {
            subjects: vec![target],
            mode: UseMode::Application,
            actor_class: ConceptCcid::from_bytes([marker; 16]),
            task_context_commitment: [marker.wrapping_add(1); 32],
            causal_role: ConceptCcid::from_bytes([marker.wrapping_add(2); 16]),
            assembly: None,
            mapping: None,
            outcome_observation: None,
            use_policy: policy,
            observed_frontier: [marker.wrapping_add(3); 32],
        }
    }

    fn plain_feed(
        seed: u8,
    ) -> (
        SigningKey,
        Vec<u8>,
        ku_core::foundation::ValidatedFeedInception,
    ) {
        let key = SigningKey::from_bytes(&[seed; 32]);
        let bytes = FeedInception::new(
            *key.verifying_key().as_bytes(),
            NamespaceCommitment::derive(b"pomv-plain-feed", [seed.wrapping_add(1); 32]).unwrap(),
            0,
            DeviceId::from_bytes([seed.wrapping_add(2); 32]),
        )
        .sign(&key)
        .unwrap()
        .encode()
        .unwrap();
        let feed = decode_feed_inception(&bytes).unwrap();
        (key, bytes, feed)
    }

    struct AuthorizedFeedFixture {
        root_bytes: Vec<u8>,
        root_cid: EventCid,
        actor: ActorId,
        feed_key: SigningKey,
        feed: ku_core::foundation::ValidatedFeedInception,
        device: DeviceId,
    }

    fn authorized_feed() -> AuthorizedFeedFixture {
        let root_key = SigningKey::from_bytes(&[0x31; 32]);
        let feed_key = SigningKey::from_bytes(&[0x32; 32]);
        let namespace = NamespaceCommitment::derive(b"pomv-authorized-feed", [0x33; 32]).unwrap();
        let device = DeviceId::from_bytes([0x34; 32]);
        let mut feed =
            FeedInception::new(*feed_key.verifying_key().as_bytes(), namespace, 0, device);
        let feed_id = feed.feed_id().unwrap();
        let root_bytes = ActorRootDelegation::new(
            *root_key.verifying_key().as_bytes(),
            feed_id,
            device,
            Some(namespace),
            0,
            0,
        )
        .unwrap()
        .sign(&root_key)
        .unwrap()
        .encode()
        .unwrap();
        let root_cid = EventCid::from_bytes(ReservedDomain::AuthorityEvent.digest(&root_bytes));
        let root = ku_core::foundation::decode_actor_root_delegation(&root_bytes).unwrap();
        feed.actor_delegation_ref = Some(root_cid.into_bytes());
        let feed_bytes = feed.sign(&feed_key).unwrap().encode().unwrap();
        let feed = decode_feed_inception(&feed_bytes).unwrap();
        AuthorizedFeedFixture {
            root_bytes,
            root_cid,
            actor: root.signed.delegation.actor,
            feed_key,
            feed,
            device,
        }
    }

    fn publish_request(
        receiver: &VNextNetworkRuntime,
        selector: SelectorCid,
        namespace: NamespaceCommitment,
        target: ObjectReference,
        policy: ObjectReference,
        idempotency_key: [u8; 32],
    ) -> PublishPublicUseEvidenceRequest {
        PublishPublicUseEvidenceRequest {
            payload: payload(target, policy, 0x41),
            expected_peer: NodeId::from_bytes(receiver.status().principal),
            last_known_addr: receiver.local_addr(),
            selector,
            namespace,
            idempotency_key,
            confirmation: ExplicitUseConfirmation::new([0x42; 32]).unwrap(),
        }
    }

    #[test]
    fn publisher_transaction_is_idempotent_restart_safe_and_sequence_linked() {
        assert!(matches!(
            ExplicitUseConfirmation::new([0; 32]),
            Err(DistributedPomvError::ConfirmationRequired)
        ));
        let directory = tempfile::tempdir().unwrap();
        let (key, _feed_bytes, feed) = plain_feed(0x21);
        let target = reference(0x22);
        let policy = reference(0x23);
        let request = PublishPublicUseEvidenceRequest {
            payload: payload(target, policy, 0x24),
            expected_peer: NodeId::from_bytes([0x25; 32]),
            last_known_addr: "127.0.0.1:32001".parse().unwrap(),
            selector: SelectorCid::from_bytes([0x26; 32]),
            namespace: NamespaceCommitment::from_bytes([0x27; 32]),
            idempotency_key: [0x28; 32],
            confirmation: ExplicitUseConfirmation::new([0x29; 32]).unwrap(),
        };
        let first;
        {
            let publisher = PublicUseEvidencePublisher::open(directory.path()).unwrap();
            let (publication, outcome) =
                publisher.publish_confirmed(&request, &feed, &key).unwrap();
            assert_eq!(outcome, PublicUsePublishOutcome::Stored);
            assert_eq!(publication.author_sequence, 0);
            assert_eq!(publisher.pending_publication_count().unwrap(), 1);
            let (replay, outcome) = publisher.publish_confirmed(&request, &feed, &key).unwrap();
            assert_eq!(outcome, PublicUsePublishOutcome::ExactReplay);
            assert_eq!(replay, publication);

            let mut conflict = request.clone();
            conflict.payload.task_context_commitment = [0x2A; 32];
            assert!(matches!(
                publisher.publish_confirmed(&conflict, &feed, &key),
                Err(DistributedPomvError::IdempotencyConflict)
            ));

            let mut second_request = request.clone();
            second_request.idempotency_key = [0x2B; 32];
            let (second, outcome) = publisher
                .publish_confirmed(&second_request, &feed, &key)
                .unwrap();
            assert_eq!(outcome, PublicUsePublishOutcome::Stored);
            assert_eq!(second.author_sequence, 1);
            let second_event =
                decode_knowledge_event(&second.event_bytes, &feed, &[USE_EVIDENCE_EVENT_TYPE])
                    .unwrap();
            assert_eq!(
                second_event.signed.event.causal_parents,
                vec![publication.event_cid]
            );
            assert_eq!(publisher.pending_publication_count().unwrap(), 2);
            first = publication;
        }
        let reopened = PublicUseEvidencePublisher::open(directory.path()).unwrap();
        let (replay, outcome) = reopened.publish_confirmed(&request, &feed, &key).unwrap();
        assert_eq!(outcome, PublicUsePublishOutcome::ExactReplay);
        assert_eq!(replay, first);
        assert_eq!(reopened.pending_publication_count().unwrap(), 2);
    }

    #[test]
    fn received_identity_index_preserves_conflict_across_restart() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("identities.redb");
        let identity = use_identity(FeedId::from_bytes([1; 32]), [2; 32]);
        {
            let index = DurableUseIdentityIndex::open(&path).unwrap();
            assert_eq!(
                index
                    .observe(identity, EventCid::from_bytes([3; 32]))
                    .unwrap(),
                IdentityObserveOutcome::Added
            );
            assert_eq!(
                index
                    .observe(identity, EventCid::from_bytes([3; 32]))
                    .unwrap(),
                IdentityObserveOutcome::ExactReplay
            );
            assert_eq!(
                index
                    .observe(identity, EventCid::from_bytes([4; 32]))
                    .unwrap(),
                IdentityObserveOutcome::ConflictObserved
            );
        }
        let reopened = DurableUseIdentityIndex::open(&path).unwrap();
        assert_eq!(
            reopened.state(identity).unwrap(),
            (false, vec![[3; 32], [4; 32]])
        );
    }

    #[test]
    fn authority_mapping_never_promotes_unresolved_or_revoked_branches() {
        let frontier = EventCid::from_bytes([5; 32]);
        let actor = ActorId::from_bytes([6; 32]);
        let unresolved = FeedAuthorityDecision::StaleOrUnresolved {
            reason: UnresolvedAuthorityReason::MissingAcceptedGrant,
            frontier,
        };
        let revoked = FeedAuthorityDecision::QuarantinedRevokedRelative {
            actor,
            revocation: EventCid::from_bytes([7; 32]),
            frontier,
        };
        assert_eq!(
            assess_branch_authority(&[0], &[unresolved]),
            ExerciseAuthority::Unresolved
        );
        assert_eq!(
            assess_branch_authority(&[0], &[revoked]),
            ExerciseAuthority::Unauthorized
        );
        assert_eq!(
            assess_branch_authority(&[], &[]),
            ExerciseAuthority::Unresolved
        );
    }

    fn enqueue_records(
        sender: &VNextNetworkRuntime,
        receiver: &VNextNetworkRuntime,
        selector: SelectorCid,
        namespace: NamespaceCommitment,
        records: impl IntoIterator<Item = (ReconcileManifestKind, Vec<u8>)>,
    ) -> Vec<OutboundTransferIntent> {
        let peer = NodeId::from_bytes(receiver.status().principal);
        records
            .into_iter()
            .map(|(kind, bytes)| {
                let intent = OutboundTransferIntent::new(
                    peer,
                    receiver.local_addr(),
                    selector,
                    namespace,
                    DisclosureClass::Public,
                    kind,
                    bytes,
                )
                .unwrap();
                sender.enqueue_outbound(&intent).unwrap();
                intent
            })
            .collect()
    }

    async fn wait_acknowledged(runtime: &VNextNetworkRuntime, intents: &[OutboundTransferIntent]) {
        tokio::time::timeout(Duration::from_secs(10), async {
            loop {
                if intents.iter().all(|intent| {
                    runtime
                        .outbound_intent(&intent.id)
                        .unwrap()
                        .is_some_and(|stored| stored.state == OutboundIntentState::Acknowledged)
                }) {
                    break;
                }
                tokio::time::sleep(Duration::from_millis(10)).await;
            }
        })
        .await
        .unwrap();
    }

    async fn wait_event_sources(
        receiver: &VNextNetworkRuntime,
        event: EventCid,
        selector: SelectorCid,
        expected: usize,
    ) {
        tokio::time::timeout(Duration::from_secs(10), async {
            loop {
                if receiver
                    .record_source_peers(ReconcileManifestKind::Event, event.into_bytes(), selector)
                    .unwrap()
                    .len()
                    == expected
                {
                    break;
                }
                tokio::time::sleep(Duration::from_millis(10)).await;
            }
        })
        .await
        .unwrap();
    }

    #[tokio::test]
    async fn public_use_over_one_two_five_paths_is_counted_once_and_survives_restart() {
        let sender_dir = tempfile::tempdir().unwrap();
        let receiver_dir = tempfile::tempdir().unwrap();
        let mut sender = VNextNetworkRuntime::start(
            sender_dir.path(),
            "127.0.0.1:0".parse().unwrap(),
            VNextNetworkPolicy::default(),
        )
        .await
        .unwrap();
        let mut receiver = VNextNetworkRuntime::start(
            receiver_dir.path(),
            "127.0.0.1:0".parse().unwrap(),
            VNextNetworkPolicy::default(),
        )
        .await
        .unwrap();
        let fixture = authorized_feed();
        let selector = SelectorCid::from_bytes([0x51; 32]);
        let namespace = NamespaceCommitment::derive(b"distributed-pomv-m4", [0x52; 32]).unwrap();
        let target = reference(0x53);
        let evidence_policy = reference(0x54);
        let view_policy = MetabolicViewPolicy {
            policy_ref: reference(0x55),
            accepted_evidence_policies: vec![evidence_policy.clone()],
            recent_event_horizon: 16,
        };

        let root_intents = enqueue_records(
            &sender,
            &receiver,
            selector,
            namespace,
            [(
                ReconcileManifestKind::AuthorityEvent,
                fixture.root_bytes.clone(),
            )],
        );
        wait_acknowledged(&sender, &root_intents).await;

        let publisher = PublicUseEvidencePublisher::open(sender_dir.path()).unwrap();
        let request = publish_request(
            &receiver,
            selector,
            namespace,
            target.clone(),
            evidence_policy.clone(),
            [0x56; 32],
        );
        let (publication, outcome) = publisher
            .publish_confirmed(&request, &fixture.feed, &fixture.feed_key)
            .unwrap();
        assert_eq!(outcome, PublicUsePublishOutcome::Stored);
        drop(publisher);
        let publisher = PublicUseEvidencePublisher::open(sender_dir.path()).unwrap();
        let (restart_replay, outcome) = publisher
            .publish_confirmed(&request, &fixture.feed, &fixture.feed_key)
            .unwrap();
        assert_eq!(outcome, PublicUsePublishOutcome::ExactReplay);
        assert_eq!(restart_replay, publication);
        let flush = publisher.flush_pending(&sender, 8).unwrap();
        assert_eq!(flush.exported_publications, 1);
        assert_eq!(flush.added_intents, 3);
        assert_eq!(publisher.pending_publication_count().unwrap(), 0);
        let publication_intents = publication.transfer_intents().unwrap();
        wait_acknowledged(&sender, &publication_intents).await;
        wait_event_sources(&receiver, publication.event_cid, selector, 1).await;

        let pomv = DistributedPomvRuntime::open(receiver_dir.path(), 1_024).unwrap();
        let one_path = pomv
            .materialize_public_use_view(
                &receiver,
                selector,
                target.clone(),
                &view_policy,
                fixture.root_cid,
            )
            .unwrap();
        assert_eq!(
            one_path.view.cumulative_event_ids,
            vec![publication.event_cid]
        );
        assert_eq!(one_path.observations.len(), 1);
        assert_eq!(
            one_path.observations[0].authority,
            ExerciseAuthority::Authorized
        );
        assert_eq!(one_path.observations[0].source_peers.len(), 1);
        assert_eq!(one_path.view.revision, 1);
        assert_eq!(one_path.view.previous_view_root, None);
        let one_path_root = one_path.view.view_root;
        assert!(!one_path.claims_truth);
        assert!(!one_path.claims_benefit);
        assert!(!one_path.changes_wallet_state);
        assert!(!one_path.changes_obt_state);
        assert!(!one_path.claims_network_completion);
        assert!(!pomv.changes_wallet_state());
        assert!(!pomv.changes_obt_state());

        let mut bridges = Vec::new();
        for expected_paths in 2..=5 {
            let directory = tempfile::tempdir().unwrap();
            let runtime = VNextNetworkRuntime::start(
                directory.path(),
                "127.0.0.1:0".parse().unwrap(),
                VNextNetworkPolicy::default(),
            )
            .await
            .unwrap();
            let intents = enqueue_records(
                &runtime,
                &receiver,
                selector,
                namespace,
                [
                    (
                        ReconcileManifestKind::FeedInception,
                        publication.feed_bytes.clone(),
                    ),
                    (
                        ReconcileManifestKind::Object,
                        publication.object_bytes.clone(),
                    ),
                    (
                        ReconcileManifestKind::Event,
                        publication.event_bytes.clone(),
                    ),
                ],
            );
            wait_acknowledged(&runtime, &intents).await;
            wait_event_sources(&receiver, publication.event_cid, selector, expected_paths).await;
            let report = pomv
                .materialize_public_use_view(
                    &receiver,
                    selector,
                    target.clone(),
                    &view_policy,
                    fixture.root_cid,
                )
                .unwrap();
            assert_eq!(
                report.view.cumulative_event_ids,
                vec![publication.event_cid]
            );
            assert_eq!(report.observations[0].source_peers.len(), expected_paths);
            assert_eq!(report.view.view_root, one_path_root);
            assert_eq!(report.view.revision, 1);
            bridges.push((directory, runtime));
        }

        // A separately signed but undelegated feed stays unresolved and cannot
        // increase cumulative evidence.
        let (unknown_key, unknown_feed_bytes, unknown_feed) = plain_feed(0x61);
        let unknown_object = payload(target.clone(), evidence_policy.clone(), 0x62)
            .to_knowledge_object(DisclosureClass::Public)
            .unwrap();
        let (unknown_object_bytes, unknown_object_cid) =
            unknown_object.encode(ResourceProfile::ObjectV1).unwrap();
        let mut unknown_event = KnowledgeEventEnvelope::new(
            USE_EVIDENCE_EVENT_TYPE,
            unknown_feed.feed_id,
            0,
            DisclosureClass::Public,
            [0x63; 32],
        );
        unknown_event.payload_refs = vec![ObjectReference::new(0, unknown_object_cid.into_bytes())];
        let (unknown_event_bytes, _unknown_event_cid) = unknown_event
            .sign(&unknown_feed, &unknown_key)
            .unwrap()
            .encode()
            .unwrap();
        let unknown_intents = enqueue_records(
            &sender,
            &receiver,
            selector,
            namespace,
            [
                (ReconcileManifestKind::FeedInception, unknown_feed_bytes),
                (ReconcileManifestKind::Object, unknown_object_bytes),
                (ReconcileManifestKind::Event, unknown_event_bytes),
            ],
        );
        wait_acknowledged(&sender, &unknown_intents).await;
        let with_unknown = pomv
            .materialize_public_use_view(
                &receiver,
                selector,
                target.clone(),
                &view_policy,
                fixture.root_cid,
            )
            .unwrap();
        assert_eq!(
            with_unknown.view.cumulative_event_ids,
            vec![publication.event_cid]
        );
        assert!(with_unknown
            .observations
            .iter()
            .any(|observation| observation.authority == ExerciseAuthority::Unresolved));
        assert!(with_unknown
            .view
            .limitations
            .contains(&MetabolicViewLimitation::AuthorityUnresolved));
        assert_eq!(with_unknown.view.revision, 2);
        assert_eq!(with_unknown.view.previous_view_root, Some(one_path_root));
        let restart_root = with_unknown.view.view_root;

        for (_, bridge) in &mut bridges {
            bridge.shutdown().await;
        }
        sender.shutdown().await;
        drop(pomv);
        receiver.shutdown().await;
        drop(receiver);

        let mut restarted = VNextNetworkRuntime::start(
            receiver_dir.path(),
            "127.0.0.1:0".parse().unwrap(),
            VNextNetworkPolicy::default(),
        )
        .await
        .unwrap();
        let reopened = DistributedPomvRuntime::open(receiver_dir.path(), 1_024).unwrap();
        let after_restart = reopened
            .materialize_public_use_view(
                &restarted,
                selector,
                target.clone(),
                &view_policy,
                fixture.root_cid,
            )
            .unwrap();
        assert_eq!(after_restart.view.view_root, restart_root);
        assert_eq!(after_restart.view.revision, 2);
        assert_eq!(after_restart.view.previous_view_root, Some(one_path_root));
        assert_eq!(
            after_restart.view.cumulative_event_ids,
            vec![publication.event_cid]
        );
        assert_eq!(
            after_restart
                .observations
                .iter()
                .find(|observation| observation.event_cid == publication.event_cid)
                .unwrap()
                .source_peers
                .len(),
            5
        );

        let control_dir = tempfile::tempdir().unwrap();
        let mut control_sender = VNextNetworkRuntime::start(
            control_dir.path(),
            "127.0.0.1:0".parse().unwrap(),
            VNextNetworkPolicy::default(),
        )
        .await
        .unwrap();
        let revocation_bytes = ActorRevocation::new(
            fixture.actor,
            fixture.root_cid,
            fixture.device,
            0,
            fixture.root_cid,
            fixture.feed.feed_id,
        )
        .unwrap()
        .sign(&fixture.feed, &fixture.feed_key)
        .unwrap()
        .encode()
        .unwrap();
        let revocation_cid =
            EventCid::from_bytes(ReservedDomain::AuthorityEvent.digest(&revocation_bytes));
        let revocation_intents = enqueue_records(
            &control_sender,
            &restarted,
            selector,
            namespace,
            [(ReconcileManifestKind::AuthorityEvent, revocation_bytes)],
        );
        wait_acknowledged(&control_sender, &revocation_intents).await;
        assert_eq!(
            restarted
                .feed_authority_at(fixture.feed.feed_id, revocation_cid)
                .unwrap()[0]
                .code(),
            "QUARANTINED_REVOKED_RELATIVE"
        );
        let revoked = reopened
            .materialize_public_use_view(
                &restarted,
                selector,
                target.clone(),
                &view_policy,
                revocation_cid,
            )
            .unwrap();
        assert!(revoked.view.cumulative_event_ids.is_empty());
        assert!(revoked.observations.iter().any(|observation| {
            observation.event_cid == publication.event_cid
                && observation.authority == ExerciseAuthority::Unauthorized
        }));
        assert!(revoked
            .view
            .limitations
            .contains(&MetabolicViewLimitation::UnauthorizedEvidenceExcluded));
        assert_eq!(revoked.view.revision, 3);
        assert_eq!(revoked.view.previous_view_root, Some(restart_root));

        // An adversarial second EventCID with the same feed/type/idempotency is
        // retained as a conflict and neither variant can double count.
        let conflict_object = payload(target.clone(), evidence_policy, 0x71)
            .to_knowledge_object(DisclosureClass::Public)
            .unwrap();
        let (conflict_object_bytes, conflict_object_cid) =
            conflict_object.encode(ResourceProfile::ObjectV1).unwrap();
        let mut conflict_event = KnowledgeEventEnvelope::new(
            USE_EVIDENCE_EVENT_TYPE,
            fixture.feed.feed_id,
            1,
            DisclosureClass::Public,
            publication.idempotency_key,
        );
        conflict_event.payload_refs =
            vec![ObjectReference::new(0, conflict_object_cid.into_bytes())];
        conflict_event.causal_parents = vec![publication.event_cid];
        let (conflict_event_bytes, _conflict_event_cid) = conflict_event
            .sign(&fixture.feed, &fixture.feed_key)
            .unwrap()
            .encode()
            .unwrap();
        let conflict_intents = enqueue_records(
            &control_sender,
            &restarted,
            selector,
            namespace,
            [
                (ReconcileManifestKind::Object, conflict_object_bytes),
                (ReconcileManifestKind::Event, conflict_event_bytes),
            ],
        );
        wait_acknowledged(&control_sender, &conflict_intents).await;
        let conflict = reopened
            .materialize_public_use_view(
                &restarted,
                selector,
                target,
                &view_policy,
                fixture.root_cid,
            )
            .unwrap();
        assert_eq!(conflict.idempotency_conflicts, 1);
        assert!(conflict.view.cumulative_event_ids.is_empty());
        assert!(!conflict.changes_wallet_state);
        assert!(!conflict.changes_obt_state);

        control_sender.shutdown().await;
        restarted.shutdown().await;
    }
}

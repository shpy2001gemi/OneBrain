//! Bounded M4 runtime for explicit Public UseEvidence exchange.
//!
//! The sender uses a transactional logical outbox. The receiver rebuilds a
//! policy/frontier-relative metabolic view from typed, signed, durable records.
//! Nothing in this module changes wallet or OBT state.

#![cfg(feature = "vnext-network-runtime")]

use std::collections::{BTreeMap, BTreeSet};
use std::path::Path;
use std::sync::Arc;
use std::time::{SystemTime, UNIX_EPOCH};

use ku_core::foundation::{
    decode_feed_inception, decode_knowledge_event, decode_knowledge_object, event_author_feed,
    AssessedExerciseEvidence, DisclosureClass, EventCid, ExerciseAuthority, ExerciseEvidence,
    FeedAuthorityDecision, FeedEventSigner, FeedId, KnowledgeEventEnvelope, KnownObjectKind,
    MetabolicEvidenceFrontier, MetabolicEvidenceReducer, MetabolicEvidenceView,
    NamespaceCommitment, NodeId, ObjectCid, ObjectReference, ObjectSemantics,
    ProvenFeedEventSigner, ResourceProfile, SelectorCid, UseEvidencePayload,
    ValidatedKnowledgeEvent, ValidatedUseEvidenceEvent, USE_EVIDENCE_EVENT_TYPE, USE_EVIDENCE_KIND,
};
use onebrain_protocol::ReconcileManifestKind;
use rand::rngs::OsRng;
use rand::RngCore;
use redb::{Database, ReadableTable, ReadableTableMetadata, TableDefinition};
use serde::{Deserialize, Serialize};
use thiserror::Error;

use crate::vnext_network_runtime::{VNextNetworkRuntime, VNextNetworkRuntimeError};
use crate::vnext_outbox::{OutboundTransferIntent, OutboxEnqueueOutcome};
use crate::vnext_route_authority::{
    AuthenticatedRouteOrigin, AuthorityFrontierResolution, LocalPolicyRegistry,
    LocalPolicyRegistryError, LocalPolicyVersion,
};

const PUBLICATIONS: TableDefinition<&[u8], &[u8]> =
    TableDefinition::new("vnext_public_use_publications_v1");
const FEED_HEADS: TableDefinition<&[u8], &[u8]> =
    TableDefinition::new("vnext_public_use_feed_heads_v1");
const PREPARED_PUBLIC_USE: TableDefinition<&[u8], &[u8]> =
    TableDefinition::new("vnext_prepared_public_use_v1");
const PREPARED_BY_OPERATION: TableDefinition<&[u8], &[u8]> =
    TableDefinition::new("vnext_prepared_public_use_by_operation_v1");
const USE_IDENTITIES: TableDefinition<&[u8], &[u8]> =
    TableDefinition::new("vnext_received_use_identities_v1");
const VIEW_HEADS: TableDefinition<&[u8], &[u8]> =
    TableDefinition::new("vnext_distributed_pomv_view_heads_v1");
const PUBLICATION_SCHEMA: u64 = 3;
const LEGACY_PUBLICATION_SCHEMA_WITH_CALLER_ROUTE: u64 = 2;
const PREPARED_PUBLIC_USE_SCHEMA: u64 = 1;
const PUBLICATION_KEY_BYTES: usize = 64;
const USE_IDENTITY_BYTES: usize = 72;
const VIEW_LINEAGE_KEY_BYTES: usize = 80;
const VIEW_HEAD_VALUE_BYTES: usize = 73;
const MAX_PUBLICATIONS: u64 = 65_536;
const MAX_PREPARED_PUBLIC_USE_INTENTS: u64 = 65_536;
const MAX_FLUSH_BATCH: usize = 4_096;
pub const MAX_PUBLIC_USE_CONSENT_TTL_SECONDS: u64 = 15 * 60;

#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct PublicUseIntentCid([u8; 32]);

impl PublicUseIntentCid {
    pub const fn from_bytes(bytes: [u8; 32]) -> Self {
        Self(bytes)
    }

    pub const fn as_bytes(&self) -> &[u8; 32] {
        &self.0
    }

    pub const fn into_bytes(self) -> [u8; 32] {
        self.0
    }
}

impl std::fmt::Debug for PublicUseIntentCid {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "PublicUseIntentCid({self})")
    }
}

impl std::fmt::Display for PublicUseIntentCid {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        for byte in self.0 {
            write!(f, "{byte:02x}")?;
        }
        Ok(())
    }
}

#[derive(Clone, Debug)]
pub struct PreparePublicUseEvidenceRequest {
    pub payload: UseEvidencePayload,
    pub exact_target: ObjectReference,
    pub expected_peer: NodeId,
    pub selector: SelectorCid,
    pub namespace: NamespaceCommitment,
    pub disclosure: DisclosureClass,
    pub idempotency_key: [u8; 32],
    pub expires_at: u64,
}

#[derive(Clone, Copy, PartialEq, Eq)]
struct SingleUseConsentReceipt([u8; 32]);

impl std::fmt::Debug for SingleUseConsentReceipt {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("SingleUseConsentReceipt([REDACTED])")
    }
}

#[derive(PartialEq, Eq)]
pub struct PreparedPublicUseIntent {
    pub intent_cid: PublicUseIntentCid,
    pub canonical_payload_preview: Vec<u8>,
    pub exact_target: ObjectReference,
    pub exact_recipient: NodeId,
    pub selector: SelectorCid,
    pub namespace: NamespaceCommitment,
    pub disclosure: DisclosureClass,
    pub idempotency_key: [u8; 32],
    pub expires_at: u64,
    receipt: SingleUseConsentReceipt,
}

impl PreparedPublicUseIntent {
    /// Convert the locally reviewed preparation into a typed confirmation.
    ///
    /// The receipt stays private and has no byte-export API. Consuming this
    /// capability is the runtime boundary that a product UI must place behind
    /// an explicit user action.
    pub fn confirm(self) -> ConfirmPublicUseEvidenceRequest {
        ConfirmPublicUseEvidenceRequest {
            intent_cid: self.intent_cid,
            receipt: self.receipt,
        }
    }
}

impl std::fmt::Debug for PreparedPublicUseIntent {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("PreparedPublicUseIntent")
            .field("intent_cid", &self.intent_cid)
            .field("canonical_payload_preview", &self.canonical_payload_preview)
            .field("exact_target", &self.exact_target)
            .field("exact_recipient", &self.exact_recipient)
            .field("selector", &self.selector)
            .field("namespace", &self.namespace)
            .field("disclosure", &self.disclosure)
            .field("idempotency_key", &self.idempotency_key)
            .field("expires_at", &self.expires_at)
            .field("receipt", &"[REDACTED]")
            .finish()
    }
}

pub struct ConfirmPublicUseEvidenceRequest {
    pub intent_cid: PublicUseIntentCid,
    receipt: SingleUseConsentReceipt,
}

impl std::fmt::Debug for ConfirmPublicUseEvidenceRequest {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ConfirmPublicUseEvidenceRequest")
            .field("intent_cid", &self.intent_cid)
            .field("receipt", &"[REDACTED]")
            .finish()
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PublicUsePublishOutcome {
    Stored,
    ExactReplay,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PublicUseEvidencePublication {
    pub publication_id: [u8; 32],
    pub intent_cid: PublicUseIntentCid,
    pub author_feed: FeedId,
    pub author_sequence: u64,
    pub idempotency_key: [u8; 32],
    pub feed_bytes: Vec<u8>,
    pub object_bytes: Vec<u8>,
    pub object_cid: ObjectCid,
    pub event_bytes: Vec<u8>,
    pub event_cid: EventCid,
    pub expected_peer: NodeId,
    pub selector: SelectorCid,
    pub namespace: NamespaceCommitment,
}

impl PublicUseEvidencePublication {
    pub fn transfer_intents(
        &self,
        network: &VNextNetworkRuntime,
    ) -> Result<Vec<OutboundTransferIntent>, DistributedPomvError> {
        let route = network
            .authenticated_route(self.expected_peer)?
            .ok_or(DistributedPomvError::AuthenticatedRouteUnavailable)?;
        if route.origin != AuthenticatedRouteOrigin::OutboundResponder {
            return Err(DistributedPomvError::AuthenticatedRouteUnavailable);
        }
        [
            (ReconcileManifestKind::FeedInception, &self.feed_bytes),
            (ReconcileManifestKind::Object, &self.object_bytes),
            (ReconcileManifestKind::Event, &self.event_bytes),
        ]
        .into_iter()
        .map(|(kind, bytes)| {
            OutboundTransferIntent::new(
                self.expected_peer,
                route.addr,
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
    intent_cid: [u8; 32],
    author_feed: [u8; 32],
    author_sequence: u64,
    idempotency_key: [u8; 32],
    receipt_commitment: [u8; 32],
    expected_peer: [u8; 32],
    #[serde(rename = "last_known_addr", default, skip_serializing)]
    legacy_caller_route: Option<String>,
    selector: [u8; 32],
    namespace: [u8; 32],
    feed_bytes: Vec<u8>,
    object_bytes: Vec<u8>,
    event_bytes: Vec<u8>,
    exported_to_network_outbox: bool,
}

#[derive(Clone, Serialize, Deserialize)]
struct StoredPreparedPublicUse {
    schema: u64,
    intent_cid: [u8; 32],
    author_feed: [u8; 32],
    canonical_payload_preview: Vec<u8>,
    exact_target_kind: u64,
    exact_target_cid: [u8; 32],
    exact_recipient: [u8; 32],
    selector: [u8; 32],
    namespace: [u8; 32],
    disclosure: u64,
    idempotency_key: [u8; 32],
    expires_at: u64,
    receipt_commitment: [u8; 32],
    consumed: bool,
}

#[derive(Serialize, Deserialize)]
struct StoredFeedHead {
    next_sequence: u64,
    last_event_cid: Option<[u8; 32]>,
}

pub struct PublicUseEvidencePublisher {
    database: Database,
    consent_clock: Arc<dyn PublicUseConsentClock>,
}

trait PublicUseConsentClock: Send + Sync {
    fn now_unix_seconds(&self) -> Result<u64, DistributedPomvError>;
}

struct SystemPublicUseConsentClock;

impl PublicUseConsentClock for SystemPublicUseConsentClock {
    fn now_unix_seconds(&self) -> Result<u64, DistributedPomvError> {
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|duration| duration.as_secs())
            .map_err(|_| DistributedPomvError::ConsentClockUnavailable)
    }
}

impl PublicUseEvidencePublisher {
    pub fn open(data_dir: &Path) -> Result<Self, DistributedPomvError> {
        Self::open_with_clock(data_dir, Arc::new(SystemPublicUseConsentClock))
    }

    fn open_with_clock(
        data_dir: &Path,
        consent_clock: Arc<dyn PublicUseConsentClock>,
    ) -> Result<Self, DistributedPomvError> {
        std::fs::create_dir_all(data_dir)?;
        let database =
            Database::create(data_dir.join("vnext_public_use_sender.redb")).map_err(storage)?;
        let write = database.begin_write().map_err(storage)?;
        {
            write.open_table(PUBLICATIONS).map_err(storage)?;
            write.open_table(FEED_HEADS).map_err(storage)?;
            write.open_table(PREPARED_PUBLIC_USE).map_err(storage)?;
            write.open_table(PREPARED_BY_OPERATION).map_err(storage)?;
        }
        write.commit().map_err(storage)?;
        Ok(Self {
            database,
            consent_clock,
        })
    }

    pub fn prepare_public_use(
        &self,
        request: &PreparePublicUseEvidenceRequest,
        author: &ku_core::foundation::ValidatedFeedInception,
    ) -> Result<PreparedPublicUseIntent, DistributedPomvError> {
        let now = self.consent_clock.now_unix_seconds()?;
        validate_prepare_request(request, now)?;
        if !request.payload.subjects.contains(&request.exact_target) {
            return Err(DistributedPomvError::ConsentTargetMismatch);
        }
        let (object_bytes, _) = request
            .payload
            .to_knowledge_object(request.disclosure)
            .map_err(|error| DistributedPomvError::Evidence(format!("{error:?}")))?
            .encode(ResourceProfile::ObjectV1)
            .map_err(|error| DistributedPomvError::Evidence(error.to_string()))?;
        let intent_cid = public_use_intent_cid(
            author.feed_id,
            &object_bytes,
            &request.exact_target,
            request.expected_peer,
            request.selector,
            request.namespace,
            request.disclosure,
            request.idempotency_key,
            request.expires_at,
        );
        let receipt = generate_single_use_receipt()?;
        let receipt_commitment = consent_receipt_commitment(intent_cid, receipt);
        let operation_key = publication_key(author.feed_id, request.idempotency_key);
        let mut stored = StoredPreparedPublicUse {
            schema: PREPARED_PUBLIC_USE_SCHEMA,
            intent_cid,
            author_feed: *author.feed_id.as_bytes(),
            canonical_payload_preview: object_bytes,
            exact_target_kind: request.exact_target.reference_kind,
            exact_target_cid: request.exact_target.cid,
            exact_recipient: *request.expected_peer.as_bytes(),
            selector: *request.selector.as_bytes(),
            namespace: *request.namespace.as_bytes(),
            disclosure: request.disclosure as u64,
            idempotency_key: request.idempotency_key,
            expires_at: request.expires_at,
            receipt_commitment,
            consumed: false,
        };
        validate_stored_prepared_public_use(&stored)?;

        let write = self.database.begin_write().map_err(storage)?;
        {
            let mut intents = write.open_table(PREPARED_PUBLIC_USE).map_err(storage)?;
            let mut operations = write.open_table(PREPARED_BY_OPERATION).map_err(storage)?;
            let existing = {
                let value = operations.get(operation_key.as_slice()).map_err(storage)?;
                value.map(|value| value.value().to_vec())
            };
            if let Some(existing) = existing {
                let existing_intent: [u8; 32] = existing
                    .try_into()
                    .map_err(|_| DistributedPomvError::CorruptPreparedConsent)?;
                if existing_intent != intent_cid {
                    return Err(DistributedPomvError::IdempotencyConflict);
                }
                let bytes = intents
                    .get(intent_cid.as_slice())
                    .map_err(storage)?
                    .map(|value| value.value().to_vec())
                    .ok_or(DistributedPomvError::CorruptPreparedConsent)?;
                let mut replay = decode_stored_prepared_public_use(&bytes)?;
                if replay.consumed {
                    return Err(DistributedPomvError::ConsentAlreadyConfirmed);
                }
                replay.receipt_commitment = receipt_commitment;
                validate_stored_prepared_public_use(&replay)?;
                let encoded = encode_stored_prepared_public_use(&replay)?;
                intents
                    .insert(intent_cid.as_slice(), encoded.as_slice())
                    .map_err(storage)?;
                stored = replay;
            } else {
                if intents.len().map_err(storage)? >= MAX_PREPARED_PUBLIC_USE_INTENTS {
                    return Err(DistributedPomvError::ConsentPreparationLimit);
                }
                let encoded = encode_stored_prepared_public_use(&stored)?;
                intents
                    .insert(intent_cid.as_slice(), encoded.as_slice())
                    .map_err(storage)?;
                operations
                    .insert(operation_key.as_slice(), intent_cid.as_slice())
                    .map_err(storage)?;
            }
        }
        write.commit().map_err(storage)?;
        prepared_public_use_from_stored(&stored, receipt)
    }

    pub fn publish_confirmed(
        &self,
        request: &ConfirmPublicUseEvidenceRequest,
        author: &ku_core::foundation::ValidatedFeedInception,
        signer: &dyn FeedEventSigner,
    ) -> Result<(PublicUseEvidencePublication, PublicUsePublishOutcome), DistributedPomvError> {
        let now = self.consent_clock.now_unix_seconds()?;
        let prepared = self.load_prepared_public_use(request.intent_cid)?;
        validate_confirmation(&prepared, request, author, now)?;
        let signer = ProvenFeedEventSigner::prove_for_public_key(
            signer,
            author.signed.inception.feed_public_key,
        )
        .map_err(|error| DistributedPomvError::Evidence(error.to_string()))?;
        let object = decode_knowledge_object(
            &prepared.canonical_payload_preview,
            ResourceProfile::ObjectV1,
            &[KnownObjectKind::new(USE_EVIDENCE_KIND, 1)],
            &[],
        )
        .map_err(|_| DistributedPomvError::CorruptPreparedConsent)?;
        let object_cid = object.cid();
        let object_bytes = prepared.canonical_payload_preview.clone();
        let feed_bytes = author.original_bytes().to_vec();
        let key = publication_key(author.feed_id, prepared.idempotency_key);

        let write = self.database.begin_write().map_err(storage)?;
        let outcome;
        let stored;
        {
            let mut intents = write.open_table(PREPARED_PUBLIC_USE).map_err(storage)?;
            let prepared_bytes = intents
                .get(request.intent_cid.as_bytes().as_slice())
                .map_err(storage)?
                .map(|value| value.value().to_vec())
                .ok_or(DistributedPomvError::ConsentIntentNotFound)?;
            let mut prepared = decode_stored_prepared_public_use(&prepared_bytes)?;
            validate_confirmation(&prepared, request, author, now)?;
            let mut publications = write.open_table(PUBLICATIONS).map_err(storage)?;
            let existing = publications
                .get(key.as_slice())
                .map_err(storage)?
                .map(|value| value.value().to_vec());
            if let Some(bytes) = existing {
                let replay = decode_stored_publication(&bytes)?;
                validate_publication_against_prepared(
                    &replay,
                    &prepared,
                    author,
                    &feed_bytes,
                    &object_bytes,
                )?;
                if !prepared.consumed {
                    return Err(DistributedPomvError::CorruptPreparedConsent);
                }
                outcome = PublicUsePublishOutcome::ExactReplay;
                stored = replay;
            } else {
                if prepared.consumed {
                    return Err(DistributedPomvError::CorruptPreparedConsent);
                }
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
                    prepared.idempotency_key,
                );
                event.payload_refs = vec![ObjectReference::new(0, object_cid.into_bytes())];
                event.causal_parents = head
                    .last_event_cid
                    .map(EventCid::from_bytes)
                    .into_iter()
                    .collect();
                let (event_bytes, event_cid) = event
                    .sign_with_proven_signer(author, &signer)
                    .map_err(|error| DistributedPomvError::Evidence(error.to_string()))?
                    .encode()
                    .map_err(|error| DistributedPomvError::Evidence(error.to_string()))?;
                let publication_id = publication_id(
                    prepared.intent_cid,
                    author.feed_id,
                    prepared.idempotency_key,
                    event_cid,
                    NodeId::from_bytes(prepared.exact_recipient),
                    SelectorCid::from_bytes(prepared.selector),
                    NamespaceCommitment::from_bytes(prepared.namespace),
                );
                let value = StoredPublication {
                    schema: PUBLICATION_SCHEMA,
                    publication_id,
                    intent_cid: prepared.intent_cid,
                    author_feed: *author.feed_id.as_bytes(),
                    author_sequence: head.next_sequence,
                    idempotency_key: prepared.idempotency_key,
                    receipt_commitment: prepared.receipt_commitment,
                    expected_peer: prepared.exact_recipient,
                    legacy_caller_route: None,
                    selector: prepared.selector,
                    namespace: prepared.namespace,
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
                prepared.consumed = true;
                let encoded_prepared = encode_stored_prepared_public_use(&prepared)?;
                intents
                    .insert(prepared.intent_cid.as_slice(), encoded_prepared.as_slice())
                    .map_err(storage)?;
                stored = value;
                outcome = PublicUsePublishOutcome::Stored;
            }
        }
        write.commit().map_err(storage)?;
        Ok((publication_from_stored(&stored)?, outcome))
    }

    fn load_prepared_public_use(
        &self,
        intent_cid: PublicUseIntentCid,
    ) -> Result<StoredPreparedPublicUse, DistributedPomvError> {
        let read = self.database.begin_read().map_err(storage)?;
        let table = read.open_table(PREPARED_PUBLIC_USE).map_err(storage)?;
        let bytes = table
            .get(intent_cid.as_bytes().as_slice())
            .map_err(storage)?
            .map(|value| value.value().to_vec())
            .ok_or(DistributedPomvError::ConsentIntentNotFound)?;
        decode_stored_prepared_public_use(&bytes)
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
            for intent in publication.transfer_intents(network)? {
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

fn validate_prepare_request(
    request: &PreparePublicUseEvidenceRequest,
    now: u64,
) -> Result<(), DistributedPomvError> {
    if request.idempotency_key == [0; 32]
        || request.exact_target.cid == [0; 32]
        || request.expected_peer.as_bytes() == &[0; 32]
        || request.selector.as_bytes() == &[0; 32]
        || request.namespace.as_bytes() == &[0; 32]
        || request.disclosure != DisclosureClass::Public
    {
        return Err(DistributedPomvError::InvalidPublishRequest);
    }
    if request.expires_at <= now {
        return Err(DistributedPomvError::ConsentExpired);
    }
    if request.expires_at - now > MAX_PUBLIC_USE_CONSENT_TTL_SECONDS {
        return Err(DistributedPomvError::ConsentExpiryTooFar);
    }
    Ok(())
}

#[allow(clippy::too_many_arguments)]
fn public_use_intent_cid(
    author_feed: FeedId,
    canonical_payload_preview: &[u8],
    exact_target: &ObjectReference,
    exact_recipient: NodeId,
    selector: SelectorCid,
    namespace: NamespaceCommitment,
    disclosure: DisclosureClass,
    idempotency_key: [u8; 32],
    expires_at: u64,
) -> [u8; 32] {
    let mut hasher = blake3::Hasher::new();
    hasher.update(b"onebrain:vnext:public-use-consent-intent:1\0");
    hasher.update(author_feed.as_bytes());
    hasher.update(&(canonical_payload_preview.len() as u64).to_be_bytes());
    hasher.update(canonical_payload_preview);
    hasher.update(&exact_target.reference_kind.to_be_bytes());
    hasher.update(&exact_target.cid);
    hasher.update(exact_recipient.as_bytes());
    hasher.update(selector.as_bytes());
    hasher.update(namespace.as_bytes());
    hasher.update(&(disclosure as u64).to_be_bytes());
    hasher.update(&idempotency_key);
    hasher.update(&expires_at.to_be_bytes());
    *hasher.finalize().as_bytes()
}

fn generate_single_use_receipt() -> Result<SingleUseConsentReceipt, DistributedPomvError> {
    for _ in 0..2 {
        let mut receipt = [0u8; 32];
        OsRng
            .try_fill_bytes(&mut receipt)
            .map_err(|_| DistributedPomvError::ConsentEntropyUnavailable)?;
        if receipt != [0; 32] {
            return Ok(SingleUseConsentReceipt(receipt));
        }
    }
    Err(DistributedPomvError::ConsentEntropyUnavailable)
}

fn consent_receipt_commitment(intent_cid: [u8; 32], receipt: SingleUseConsentReceipt) -> [u8; 32] {
    let mut hasher = blake3::Hasher::new();
    hasher.update(b"onebrain:vnext:public-use-consent-receipt:1\0");
    hasher.update(&intent_cid);
    hasher.update(&receipt.0);
    *hasher.finalize().as_bytes()
}

fn receipt_commitments_match(claimed: [u8; 32], expected: [u8; 32]) -> bool {
    claimed
        .into_iter()
        .zip(expected)
        .fold(0u8, |difference, (left, right)| difference | (left ^ right))
        == 0
}

fn encode_stored_prepared_public_use(
    value: &StoredPreparedPublicUse,
) -> Result<Vec<u8>, DistributedPomvError> {
    serde_json::to_vec(value).map_err(codec)
}

fn decode_stored_prepared_public_use(
    bytes: &[u8],
) -> Result<StoredPreparedPublicUse, DistributedPomvError> {
    let value = serde_json::from_slice(bytes).map_err(codec)?;
    validate_stored_prepared_public_use(&value)?;
    Ok(value)
}

fn validate_stored_prepared_public_use(
    value: &StoredPreparedPublicUse,
) -> Result<(), DistributedPomvError> {
    if value.schema != PREPARED_PUBLIC_USE_SCHEMA
        || value.intent_cid == [0; 32]
        || value.author_feed == [0; 32]
        || value.exact_target_cid == [0; 32]
        || value.exact_recipient == [0; 32]
        || value.selector == [0; 32]
        || value.namespace == [0; 32]
        || value.disclosure != DisclosureClass::Public as u64
        || value.idempotency_key == [0; 32]
        || value.expires_at == 0
        || value.receipt_commitment == [0; 32]
    {
        return Err(DistributedPomvError::CorruptPreparedConsent);
    }
    let object = decode_knowledge_object(
        &value.canonical_payload_preview,
        ResourceProfile::ObjectV1,
        &[KnownObjectKind::new(USE_EVIDENCE_KIND, 1)],
        &[],
    )
    .map_err(|_| DistributedPomvError::CorruptPreparedConsent)?;
    if object.disclosure() != DisclosureClass::Public {
        return Err(DistributedPomvError::CorruptPreparedConsent);
    }
    let payload = UseEvidencePayload::from_validated_object(&object)
        .map_err(|_| DistributedPomvError::CorruptPreparedConsent)?;
    let exact_target = ObjectReference::new(value.exact_target_kind, value.exact_target_cid);
    if !payload.subjects.contains(&exact_target) {
        return Err(DistributedPomvError::CorruptPreparedConsent);
    }
    let expected = public_use_intent_cid(
        FeedId::from_bytes(value.author_feed),
        &value.canonical_payload_preview,
        &exact_target,
        NodeId::from_bytes(value.exact_recipient),
        SelectorCid::from_bytes(value.selector),
        NamespaceCommitment::from_bytes(value.namespace),
        DisclosureClass::Public,
        value.idempotency_key,
        value.expires_at,
    );
    if expected != value.intent_cid {
        return Err(DistributedPomvError::CorruptPreparedConsent);
    }
    Ok(())
}

fn prepared_public_use_from_stored(
    stored: &StoredPreparedPublicUse,
    receipt: SingleUseConsentReceipt,
) -> Result<PreparedPublicUseIntent, DistributedPomvError> {
    validate_stored_prepared_public_use(stored)?;
    if stored.consumed
        || !receipt_commitments_match(
            consent_receipt_commitment(stored.intent_cid, receipt),
            stored.receipt_commitment,
        )
    {
        return Err(DistributedPomvError::CorruptPreparedConsent);
    }
    Ok(PreparedPublicUseIntent {
        intent_cid: PublicUseIntentCid::from_bytes(stored.intent_cid),
        canonical_payload_preview: stored.canonical_payload_preview.clone(),
        exact_target: ObjectReference::new(stored.exact_target_kind, stored.exact_target_cid),
        exact_recipient: NodeId::from_bytes(stored.exact_recipient),
        selector: SelectorCid::from_bytes(stored.selector),
        namespace: NamespaceCommitment::from_bytes(stored.namespace),
        disclosure: DisclosureClass::Public,
        idempotency_key: stored.idempotency_key,
        expires_at: stored.expires_at,
        receipt,
    })
}

fn validate_confirmation(
    stored: &StoredPreparedPublicUse,
    request: &ConfirmPublicUseEvidenceRequest,
    author: &ku_core::foundation::ValidatedFeedInception,
    now: u64,
) -> Result<(), DistributedPomvError> {
    validate_stored_prepared_public_use(stored)?;
    if stored.intent_cid != request.intent_cid.into_bytes()
        || stored.author_feed != *author.feed_id.as_bytes()
    {
        return Err(DistributedPomvError::ConsentIntentMismatch);
    }
    if now >= stored.expires_at {
        return Err(DistributedPomvError::ConsentExpired);
    }
    if !receipt_commitments_match(
        consent_receipt_commitment(request.intent_cid.into_bytes(), request.receipt),
        stored.receipt_commitment,
    ) {
        return Err(DistributedPomvError::ConsentReceiptInvalid);
    }
    Ok(())
}

fn publication_id(
    intent_cid: [u8; 32],
    feed: FeedId,
    idempotency_key: [u8; 32],
    event: EventCid,
    peer: NodeId,
    selector: SelectorCid,
    namespace: NamespaceCommitment,
) -> [u8; 32] {
    let mut hasher = blake3::Hasher::new();
    hasher.update(b"onebrain:vnext:public-use-publication:1\0");
    hasher.update(&intent_cid);
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
    let mut value: StoredPublication = serde_json::from_slice(bytes).map_err(codec)?;
    if value.schema == LEGACY_PUBLICATION_SCHEMA_WITH_CALLER_ROUTE {
        value.schema = PUBLICATION_SCHEMA;
        value.legacy_caller_route = None;
        value.exported_to_network_outbox = false;
    }
    validate_stored_publication(&value)?;
    Ok(value)
}

fn validate_stored_publication(value: &StoredPublication) -> Result<(), DistributedPomvError> {
    if value.schema != PUBLICATION_SCHEMA
        || value.intent_cid == [0; 32]
        || value.idempotency_key == [0; 32]
        || value.receipt_commitment == [0; 32]
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
        value.intent_cid,
        feed.feed_id,
        value.idempotency_key,
        event.cid(),
        peer,
        selector,
        namespace,
    );
    if expected_id != value.publication_id {
        return Err(DistributedPomvError::CorruptPublication);
    }
    Ok(())
}

fn validate_publication_against_prepared(
    stored: &StoredPublication,
    prepared: &StoredPreparedPublicUse,
    author: &ku_core::foundation::ValidatedFeedInception,
    feed_bytes: &[u8],
    object_bytes: &[u8],
) -> Result<(), DistributedPomvError> {
    if stored.intent_cid != prepared.intent_cid
        || stored.author_feed != *author.feed_id.as_bytes()
        || stored.idempotency_key != prepared.idempotency_key
        || stored.receipt_commitment != prepared.receipt_commitment
        || stored.expected_peer != prepared.exact_recipient
        || stored.selector != prepared.selector
        || stored.namespace != prepared.namespace
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
        intent_cid: PublicUseIntentCid::from_bytes(stored.intent_cid),
        author_feed: feed.feed_id,
        author_sequence: stored.author_sequence,
        idempotency_key: stored.idempotency_key,
        feed_bytes: stored.feed_bytes.clone(),
        object_bytes: stored.object_bytes.clone(),
        object_cid: object.cid(),
        event_bytes: stored.event_bytes.clone(),
        event_cid: event.cid(),
        expected_peer: NodeId::from_bytes(stored.expected_peer),
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
    max_view_records: usize,
    policies: LocalPolicyRegistry,
}

impl DistributedPomvRuntime {
    pub fn open(
        data_dir: &Path,
        max_records: usize,
        policies: LocalPolicyRegistry,
    ) -> Result<Self, DistributedPomvError> {
        Self::open_with_limits(data_dir, max_records, max_records, policies)
    }

    pub fn open_with_limits(
        data_dir: &Path,
        max_records: usize,
        max_view_records: usize,
        policies: LocalPolicyRegistry,
    ) -> Result<Self, DistributedPomvError> {
        std::fs::create_dir_all(data_dir)?;
        // Validate the configured capacity at startup.
        MetabolicEvidenceReducer::new(max_records)
            .map_err(|error| DistributedPomvError::Metabolic(format!("{error:?}")))?;
        if max_view_records == 0 || max_view_records > max_records {
            return Err(DistributedPomvError::InvalidViewLimit);
        }
        Ok(Self {
            identities: DurableUseIdentityIndex::open(
                &data_dir.join("vnext_distributed_pomv.redb"),
            )?,
            max_records,
            max_view_records,
            policies,
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
        policy_version: LocalPolicyVersion,
    ) -> Result<DistributedPomvReport, DistributedPomvError> {
        let policy = self
            .policies
            .resolve(policy_version)
            .ok_or(DistributedPomvError::PolicyVersionNotAllowed)?
            .clone();
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
        let mut authority_resolutions = BTreeMap::<FeedId, AuthorityFrontierResolution>::new();
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
            if let std::collections::btree_map::Entry::Vacant(entry) =
                authority_resolutions.entry(feed_id)
            {
                entry.insert(network.resolve_feed_authority(feed_id)?);
            }
            let decisions = match authority_resolutions.get(&feed_id) {
                Some(AuthorityFrontierResolution::Resolved { decisions, .. }) => decisions.clone(),
                Some(
                    AuthorityFrontierResolution::Missing
                    | AuthorityFrontierResolution::Ambiguous { .. },
                )
                | None => Vec::new(),
            };
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
            if observations.len() >= self.max_view_records {
                return Err(DistributedPomvError::ViewLimitExceeded);
            }
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
        let frontier = MetabolicEvidenceFrontier::new(
            local_authority_frontier_digest(&authority_resolutions),
            positions,
        )
        .map_err(|error| DistributedPomvError::Metabolic(format!("{error:?}")))?;
        let mut view = reducer
            .materialize(target, &policy, &frontier)
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

fn local_authority_frontier_digest(
    resolutions: &BTreeMap<FeedId, AuthorityFrontierResolution>,
) -> [u8; 32] {
    let mut hasher = blake3::Hasher::new();
    hasher.update(b"onebrain:vnext:validated-local-authority-frontier:1\0");
    hasher.update(&(resolutions.len() as u64).to_be_bytes());
    for (feed, resolution) in resolutions {
        hasher.update(feed.as_bytes());
        match resolution {
            AuthorityFrontierResolution::Resolved { frontier, .. } => {
                hasher.update(&[1]);
                hasher.update(frontier.as_bytes());
            }
            AuthorityFrontierResolution::Missing => {
                hasher.update(&[0]);
            }
            AuthorityFrontierResolution::Ambiguous { frontiers } => {
                hasher.update(&[2]);
                hasher.update(&(frontiers.len() as u64).to_be_bytes());
                for frontier in frontiers {
                    hasher.update(frontier.as_bytes());
                }
            }
        }
    }
    *hasher.finalize().as_bytes()
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
    #[error("public UseEvidence publish request is invalid")]
    InvalidPublishRequest,
    #[error("prepared Public Use consent has expired")]
    ConsentExpired,
    #[error("prepared Public Use consent expiry exceeds the allowed window")]
    ConsentExpiryTooFar,
    #[error("prepared Public Use target is not present in the canonical payload")]
    ConsentTargetMismatch,
    #[error("prepared Public Use intent was not found")]
    ConsentIntentNotFound,
    #[error("prepared Public Use intent does not match this author or confirmation")]
    ConsentIntentMismatch,
    #[error("single-use Public Use consent receipt is invalid")]
    ConsentReceiptInvalid,
    #[error("prepared Public Use intent was already confirmed")]
    ConsentAlreadyConfirmed,
    #[error("prepared Public Use consent limit reached")]
    ConsentPreparationLimit,
    #[error("prepared Public Use consent state is corrupt")]
    CorruptPreparedConsent,
    #[error("Public Use consent clock is unavailable")]
    ConsentClockUnavailable,
    #[error("Public Use consent receipt entropy is unavailable")]
    ConsentEntropyUnavailable,
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
    #[error("distributed PoMV view limit is invalid")]
    InvalidViewLimit,
    #[error("distributed PoMV view record limit reached")]
    ViewLimitExceeded,
    #[error("authenticated route for the exact recipient is unavailable")]
    AuthenticatedRouteUnavailable,
    #[error("requested local policy version is not allow-listed")]
    PolicyVersionNotAllowed,
    #[error("local policy registry failed: {0}")]
    PolicyRegistry(#[from] LocalPolicyRegistryError),
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
    use std::sync::atomic::{AtomicU64, Ordering};
    use std::time::Duration;

    use ed25519_dalek::SigningKey;
    use ku_core::foundation::{
        ActorId, ActorRevocation, ActorRootDelegation, ConceptCcid, DeviceId, FeedInception,
        MetabolicViewLimitation, MetabolicViewPolicy, ReservedDomain, UnresolvedAuthorityReason,
        UseMode,
    };

    use super::*;
    use crate::vnext_config::VNextNetworkPolicy;
    use crate::vnext_outbox::OutboundIntentState;

    struct TestConsentClock(AtomicU64);

    impl TestConsentClock {
        fn new(now: u64) -> Self {
            Self(AtomicU64::new(now))
        }

        fn set(&self, now: u64) {
            self.0.store(now, Ordering::SeqCst);
        }
    }

    impl PublicUseConsentClock for TestConsentClock {
        fn now_unix_seconds(&self) -> Result<u64, DistributedPomvError> {
            Ok(self.0.load(Ordering::SeqCst))
        }
    }

    #[test]
    fn pomv_record_and_view_limits_fail_closed_at_startup() {
        let directory = tempfile::tempdir().unwrap();
        let version = LocalPolicyVersion::new(1).unwrap();
        let policy = MetabolicViewPolicy {
            policy_ref: ObjectReference::new(0, [0x61; 32]),
            accepted_evidence_policies: vec![ObjectReference::new(0, [0x62; 32])],
            recent_event_horizon: 64,
        };
        let registry = LocalPolicyRegistry::new([(version, policy)]).unwrap();
        assert!(matches!(
            DistributedPomvRuntime::open_with_limits(directory.path(), 8, 0, registry.clone()),
            Err(DistributedPomvError::InvalidViewLimit)
        ));
        assert!(matches!(
            DistributedPomvRuntime::open_with_limits(directory.path(), 8, 9, registry),
            Err(DistributedPomvError::InvalidViewLimit)
        ));
    }

    fn test_publisher(path: &Path, clock: Arc<TestConsentClock>) -> PublicUseEvidencePublisher {
        PublicUseEvidencePublisher::open_with_clock(path, clock).unwrap()
    }

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

    fn prepare_request(
        receiver: &VNextNetworkRuntime,
        selector: SelectorCid,
        namespace: NamespaceCommitment,
        target: ObjectReference,
        policy: ObjectReference,
        idempotency_key: [u8; 32],
        expires_at: u64,
    ) -> PreparePublicUseEvidenceRequest {
        PreparePublicUseEvidenceRequest {
            payload: payload(target.clone(), policy, 0x41),
            exact_target: target,
            expected_peer: NodeId::from_bytes(receiver.status().principal),
            selector,
            namespace,
            disclosure: DisclosureClass::Public,
            idempotency_key,
            expires_at,
        }
    }

    #[test]
    fn publisher_transaction_is_idempotent_restart_safe_and_sequence_linked() {
        let directory = tempfile::tempdir().unwrap();
        let clock = Arc::new(TestConsentClock::new(1_000));
        let (key, _feed_bytes, feed) = plain_feed(0x21);
        let target = reference(0x22);
        let policy = reference(0x23);
        let request = PreparePublicUseEvidenceRequest {
            payload: payload(target.clone(), policy, 0x24),
            exact_target: target,
            expected_peer: NodeId::from_bytes([0x25; 32]),
            selector: SelectorCid::from_bytes([0x26; 32]),
            namespace: NamespaceCommitment::from_bytes([0x27; 32]),
            disclosure: DisclosureClass::Public,
            idempotency_key: [0x28; 32],
            expires_at: 1_600,
        };
        let first;
        let confirmation;
        {
            let publisher = test_publisher(directory.path(), clock.clone());
            let prepared = publisher.prepare_public_use(&request, &feed).unwrap();
            assert_eq!(prepared.exact_target, request.exact_target);
            assert_eq!(prepared.exact_recipient, request.expected_peer);
            assert_eq!(prepared.selector, request.selector);
            assert_eq!(prepared.namespace, request.namespace);
            assert_eq!(prepared.disclosure, DisclosureClass::Public);
            assert_eq!(prepared.idempotency_key, request.idempotency_key);
            assert_eq!(prepared.expires_at, request.expires_at);
            let preview = decode_knowledge_object(
                &prepared.canonical_payload_preview,
                ResourceProfile::ObjectV1,
                &[KnownObjectKind::new(USE_EVIDENCE_KIND, 1)],
                &[],
            )
            .unwrap();
            assert_eq!(preview.disclosure(), DisclosureClass::Public);
            confirmation = prepared.confirm();
            let (publication, outcome) = publisher
                .publish_confirmed(&confirmation, &feed, &key)
                .unwrap();
            assert_eq!(outcome, PublicUsePublishOutcome::Stored);
            assert_eq!(publication.intent_cid, confirmation.intent_cid);
            assert_eq!(publication.author_sequence, 0);
            assert_eq!(publisher.pending_publication_count().unwrap(), 1);
            let (replay, outcome) = publisher
                .publish_confirmed(&confirmation, &feed, &key)
                .unwrap();
            assert_eq!(outcome, PublicUsePublishOutcome::ExactReplay);
            assert_eq!(replay, publication);
            let publication_key = publication_key(feed.feed_id, request.idempotency_key);
            let legacy_bytes = {
                let read = publisher.database.begin_read().unwrap();
                let publications = read.open_table(PUBLICATIONS).unwrap();
                publications
                    .get(publication_key.as_slice())
                    .unwrap()
                    .unwrap()
                    .value()
                    .to_vec()
            };
            let mut legacy: serde_json::Value = serde_json::from_slice(&legacy_bytes).unwrap();
            legacy["schema"] = serde_json::Value::from(LEGACY_PUBLICATION_SCHEMA_WITH_CALLER_ROUTE);
            legacy["last_known_addr"] = serde_json::Value::from("127.0.0.1:9");
            legacy["exported_to_network_outbox"] = serde_json::Value::from(true);
            let migrated =
                decode_stored_publication(&serde_json::to_vec(&legacy).unwrap()).unwrap();
            assert_eq!(migrated.schema, PUBLICATION_SCHEMA);
            assert_eq!(migrated.legacy_caller_route, None);
            assert!(!migrated.exported_to_network_outbox);

            let mut conflict = request.clone();
            conflict.payload.task_context_commitment = [0x2A; 32];
            assert!(matches!(
                publisher.prepare_public_use(&conflict, &feed),
                Err(DistributedPomvError::IdempotencyConflict)
            ));

            let mut second_request = request.clone();
            second_request.idempotency_key = [0x2B; 32];
            let second_confirmation = publisher
                .prepare_public_use(&second_request, &feed)
                .unwrap()
                .confirm();
            let (second, outcome) = publisher
                .publish_confirmed(&second_confirmation, &feed, &key)
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
        let reopened = test_publisher(directory.path(), clock);
        let (replay, outcome) = reopened
            .publish_confirmed(&confirmation, &feed, &key)
            .unwrap();
        assert_eq!(outcome, PublicUsePublishOutcome::ExactReplay);
        assert_eq!(replay, first);
        assert_eq!(reopened.pending_publication_count().unwrap(), 2);
    }

    #[test]
    fn signer_mismatch_fails_before_publication_side_effects() {
        let directory = tempfile::tempdir().unwrap();
        let clock = Arc::new(TestConsentClock::new(1_000));
        let (key, _feed_bytes, feed) = plain_feed(0x21);
        let wrong_key = SigningKey::from_bytes(&[0x31; 32]);
        let target = reference(0x22);
        let request = PreparePublicUseEvidenceRequest {
            payload: payload(target.clone(), reference(0x23), 0x24),
            exact_target: target,
            expected_peer: NodeId::from_bytes([0x25; 32]),
            selector: SelectorCid::from_bytes([0x26; 32]),
            namespace: NamespaceCommitment::from_bytes([0x27; 32]),
            disclosure: DisclosureClass::Public,
            idempotency_key: [0x28; 32],
            expires_at: 1_600,
        };
        let publisher = test_publisher(directory.path(), clock);
        let confirmation = publisher
            .prepare_public_use(&request, &feed)
            .unwrap()
            .confirm();

        assert!(matches!(
            publisher.publish_confirmed(&confirmation, &feed, &wrong_key),
            Err(DistributedPomvError::Evidence(ref error))
                if error == "FEED_SIGNER_PUBLIC_KEY_MISMATCH"
        ));
        assert_eq!(publisher.pending_publication_count().unwrap(), 0);

        let (publication, outcome) = publisher
            .publish_confirmed(&confirmation, &feed, &key)
            .unwrap();
        assert_eq!(outcome, PublicUsePublishOutcome::Stored);
        assert_eq!(publication.author_sequence, 0);
        assert_eq!(publisher.pending_publication_count().unwrap(), 1);
    }

    #[test]
    fn arbitrary_receipt_intent_swap_and_expiry_fail_closed() {
        let directory = tempfile::tempdir().unwrap();
        let clock = Arc::new(TestConsentClock::new(1_000));
        let (key, _feed_bytes, feed) = plain_feed(0x21);
        let target = reference(0x22);
        let base = PreparePublicUseEvidenceRequest {
            payload: payload(target.clone(), reference(0x23), 0x24),
            exact_target: target,
            expected_peer: NodeId::from_bytes([0x25; 32]),
            selector: SelectorCid::from_bytes([0x26; 32]),
            namespace: NamespaceCommitment::from_bytes([0x27; 32]),
            disclosure: DisclosureClass::Public,
            idempotency_key: [0x28; 32],
            expires_at: 1_100,
        };
        let publisher = test_publisher(directory.path(), clock.clone());
        let first = publisher.prepare_public_use(&base, &feed).unwrap();
        let first_intent = first.intent_cid;
        let confirmation = first.confirm();
        let forged = ConfirmPublicUseEvidenceRequest {
            intent_cid: first_intent,
            receipt: SingleUseConsentReceipt([0xAA; 32]),
        };
        assert!(matches!(
            publisher.publish_confirmed(&forged, &feed, &key),
            Err(DistributedPomvError::ConsentReceiptInvalid)
        ));
        let (_, _, wrong_author) = plain_feed(0x31);
        assert!(matches!(
            publisher.publish_confirmed(&confirmation, &wrong_author, &key),
            Err(DistributedPomvError::ConsentIntentMismatch)
        ));

        let mut second_request = base.clone();
        second_request.idempotency_key = [0x29; 32];
        let second = publisher
            .prepare_public_use(&second_request, &feed)
            .unwrap();
        let swapped = ConfirmPublicUseEvidenceRequest {
            intent_cid: second.intent_cid,
            receipt: confirmation.receipt,
        };
        assert!(matches!(
            publisher.publish_confirmed(&swapped, &feed, &key),
            Err(DistributedPomvError::ConsentReceiptInvalid)
        ));
        assert_eq!(publisher.pending_publication_count().unwrap(), 0);

        drop(publisher);
        clock.set(1_100);
        let reopened = test_publisher(directory.path(), clock);
        assert!(matches!(
            reopened.publish_confirmed(&confirmation, &feed, &key),
            Err(DistributedPomvError::ConsentExpired)
        ));
        assert_eq!(reopened.pending_publication_count().unwrap(), 0);
    }

    #[test]
    fn prepare_rejects_wrong_target_disclosure_and_unbounded_expiry() {
        let directory = tempfile::tempdir().unwrap();
        let clock = Arc::new(TestConsentClock::new(1_000));
        let (_, _feed_bytes, feed) = plain_feed(0x21);
        let target = reference(0x22);
        let mut request = PreparePublicUseEvidenceRequest {
            payload: payload(target.clone(), reference(0x23), 0x24),
            exact_target: reference(0x99),
            expected_peer: NodeId::from_bytes([0x25; 32]),
            selector: SelectorCid::from_bytes([0x26; 32]),
            namespace: NamespaceCommitment::from_bytes([0x27; 32]),
            disclosure: DisclosureClass::Public,
            idempotency_key: [0x28; 32],
            expires_at: 1_100,
        };
        let publisher = test_publisher(directory.path(), clock);
        assert!(matches!(
            publisher.prepare_public_use(&request, &feed),
            Err(DistributedPomvError::ConsentTargetMismatch)
        ));

        request.exact_target = target;
        request.disclosure = DisclosureClass::LocalOnly;
        assert!(matches!(
            publisher.prepare_public_use(&request, &feed),
            Err(DistributedPomvError::InvalidPublishRequest)
        ));

        request.disclosure = DisclosureClass::Public;
        request.expires_at = 1_000;
        assert!(matches!(
            publisher.prepare_public_use(&request, &feed),
            Err(DistributedPomvError::ConsentExpired)
        ));

        request.expires_at = 1_000 + MAX_PUBLIC_USE_CONSENT_TTL_SECONDS + 1;
        assert!(matches!(
            publisher.prepare_public_use(&request, &feed),
            Err(DistributedPomvError::ConsentExpiryTooFar)
        ));

        request.expires_at = 1_100;
        assert!(publisher.prepare_public_use(&request, &feed).is_ok());
    }

    #[test]
    fn prepared_and_confirmation_debug_output_redacts_receipt() {
        let directory = tempfile::tempdir().unwrap();
        let clock = Arc::new(TestConsentClock::new(1_000));
        let (_, _feed_bytes, feed) = plain_feed(0x21);
        let target = reference(0x22);
        let request = PreparePublicUseEvidenceRequest {
            payload: payload(target.clone(), reference(0x23), 0x24),
            exact_target: target,
            expected_peer: NodeId::from_bytes([0x25; 32]),
            selector: SelectorCid::from_bytes([0x26; 32]),
            namespace: NamespaceCommitment::from_bytes([0x27; 32]),
            disclosure: DisclosureClass::Public,
            idempotency_key: [0x28; 32],
            expires_at: 1_100,
        };
        let publisher = test_publisher(directory.path(), clock);
        let prepared = publisher.prepare_public_use(&request, &feed).unwrap();
        let receipt_debug = format!("{:?}", prepared.receipt.0);
        let prepared_debug = format!("{prepared:?}");
        assert!(prepared_debug.contains("[REDACTED]"));
        assert!(!prepared_debug.contains(&receipt_debug));

        let confirmation = prepared.confirm();
        let confirmation_debug = format!("{confirmation:?}");
        assert!(confirmation_debug.contains("[REDACTED]"));
        assert!(!confirmation_debug.contains(&receipt_debug));
    }

    #[test]
    fn reprepare_rotates_receipt_and_confirmation_has_one_side_effect() {
        let directory = tempfile::tempdir().unwrap();
        let clock = Arc::new(TestConsentClock::new(1_000));
        let (key, _feed_bytes, feed) = plain_feed(0x21);
        let target = reference(0x22);
        let request = PreparePublicUseEvidenceRequest {
            payload: payload(target.clone(), reference(0x23), 0x24),
            exact_target: target,
            expected_peer: NodeId::from_bytes([0x25; 32]),
            selector: SelectorCid::from_bytes([0x26; 32]),
            namespace: NamespaceCommitment::from_bytes([0x27; 32]),
            disclosure: DisclosureClass::Public,
            idempotency_key: [0x28; 32],
            expires_at: 1_600,
        };
        let publisher = test_publisher(directory.path(), clock);
        let old_confirmation = publisher
            .prepare_public_use(&request, &feed)
            .unwrap()
            .confirm();
        let current_confirmation = publisher
            .prepare_public_use(&request, &feed)
            .unwrap()
            .confirm();
        assert_eq!(old_confirmation.intent_cid, current_confirmation.intent_cid);
        assert!(matches!(
            publisher.publish_confirmed(&old_confirmation, &feed, &key),
            Err(DistributedPomvError::ConsentReceiptInvalid)
        ));

        let (publication, outcome) = publisher
            .publish_confirmed(&current_confirmation, &feed, &key)
            .unwrap();
        assert_eq!(outcome, PublicUsePublishOutcome::Stored);
        let (replay, outcome) = publisher
            .publish_confirmed(&current_confirmation, &feed, &key)
            .unwrap();
        assert_eq!(outcome, PublicUsePublishOutcome::ExactReplay);
        assert_eq!(replay, publication);
        assert_eq!(publisher.pending_publication_count().unwrap(), 1);
        assert!(matches!(
            publisher.prepare_public_use(&request, &feed),
            Err(DistributedPomvError::ConsentAlreadyConfirmed)
        ));
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
        let policy_version = LocalPolicyVersion::new(1).unwrap();

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
        let expires_at = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_secs()
            + 600;
        let preparation = prepare_request(
            &receiver,
            selector,
            namespace,
            target.clone(),
            evidence_policy.clone(),
            [0x56; 32],
            expires_at,
        );
        let request = publisher
            .prepare_public_use(&preparation, &fixture.feed)
            .unwrap()
            .confirm();
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
        let unrouted_dir = tempfile::tempdir().unwrap();
        let mut unrouted = VNextNetworkRuntime::start(
            unrouted_dir.path(),
            "127.0.0.1:0".parse().unwrap(),
            VNextNetworkPolicy::default(),
        )
        .await
        .unwrap();
        assert!(matches!(
            publication.transfer_intents(&unrouted),
            Err(DistributedPomvError::AuthenticatedRouteUnavailable)
        ));
        unrouted.shutdown().await;
        let publication_intents = publication.transfer_intents(&sender).unwrap();
        wait_acknowledged(&sender, &publication_intents).await;
        wait_event_sources(&receiver, publication.event_cid, selector, 1).await;

        let pomv = DistributedPomvRuntime::open(
            receiver_dir.path(),
            1_024,
            LocalPolicyRegistry::new([(policy_version, view_policy.clone())]).unwrap(),
        )
        .unwrap();
        assert!(matches!(
            pomv.materialize_public_use_view(
                &receiver,
                selector,
                target.clone(),
                LocalPolicyVersion::new(2).unwrap(),
            ),
            Err(DistributedPomvError::PolicyVersionNotAllowed)
        ));
        let one_path = pomv
            .materialize_public_use_view(&receiver, selector, target.clone(), policy_version)
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
                .materialize_public_use_view(&receiver, selector, target.clone(), policy_version)
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
            .materialize_public_use_view(&receiver, selector, target.clone(), policy_version)
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
        let reopened = DistributedPomvRuntime::open(
            receiver_dir.path(),
            1_024,
            LocalPolicyRegistry::new([(policy_version, view_policy.clone())]).unwrap(),
        )
        .unwrap();
        let after_restart = reopened
            .materialize_public_use_view(&restarted, selector, target.clone(), policy_version)
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
            .materialize_public_use_view(&restarted, selector, target.clone(), policy_version)
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
            .materialize_public_use_view(&restarted, selector, target, policy_version)
            .unwrap();
        assert_eq!(conflict.idempotency_conflicts, 1);
        assert!(conflict.view.cumulative_event_ids.is_empty());
        assert!(!conflict.changes_wallet_state);
        assert!(!conflict.changes_obt_state);

        control_sender.shutdown().await;
        restarted.shutdown().await;
    }
}

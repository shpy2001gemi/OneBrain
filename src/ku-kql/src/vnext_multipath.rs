//! Private multipath query coordination and encrypted StandingNeed mailbox.
//!
//! Route packets remain unlinkable-by-schema operational artifacts. Local
//! union uses canonical content identity, while encrypted notification state is
//! private and exactly-once per StandingNeed/match/rule tuple.

use std::collections::{BTreeMap, BTreeSet};

use chacha20poly1305::aead::{Aead, KeyInit, Payload};
use chacha20poly1305::{XChaCha20Poly1305, XNonce};
use ku_core::foundation::{
    canonicalize_set_by_key, decode_canonical, encode_canonical, CanonicalValue, EventCid,
    MappingKernelCid, ObjectReference, ResourceProfile,
};
use zeroize::Zeroize;

use crate::vnext_disclosure_capsule::OpenedDisclosure;
use crate::vnext_query::MAX_ROUTE_SKETCHES_PER_RUN;
use crate::vnext_query_view::QueryArtifactRef;
use crate::vnext_route_packet::{decode_route_need_sketch_v1, RoutePacketError};
use crate::vnext_standing_need::{
    StandingNeed, StandingNeedError, StandingNeedId, StandingNeedState,
};

pub const MAX_MULTIPATH_RESULTS: usize = 1_000_000;
pub const MAX_MULTIPATH_REPLY_RESULTS: usize = 16_384;
pub const MAX_MAILBOX_NOTIFICATIONS: usize = 1_000_000;
pub const MAX_MAILBOX_CIPHERTEXT_BYTES: usize = 1_048_576;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct MultipathBranchPacket {
    /// Random local route handle. It is never serialized into the packet.
    pub local_path_commitment: [u8; 32],
    pub packet_bytes: Vec<u8>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct PlannedBranch {
    local_path_commitment: [u8; 32],
    packet_bytes: Vec<u8>,
    reply_capability: [u8; 32],
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct MultipathQueryPlan {
    branches: Vec<PlannedBranch>,
}

impl MultipathQueryPlan {
    pub fn new(branches: Vec<MultipathBranchPacket>) -> Result<Self, MultipathError> {
        if branches.is_empty() || branches.len() > usize::from(MAX_ROUTE_SKETCHES_PER_RUN) {
            return Err(MultipathError::BranchLimit);
        }
        let mut path_ids = BTreeSet::new();
        let mut sketch_ids = BTreeSet::new();
        let mut reply_capabilities = BTreeSet::new();
        let mut replay_nonces = BTreeSet::new();
        let mut salted_commitments = BTreeSet::new();
        let mut planned = Vec::with_capacity(branches.len());
        for branch in branches {
            if branch.local_path_commitment == [0; 32]
                || !path_ids.insert(branch.local_path_commitment)
            {
                return Err(MultipathError::DuplicateBranchIdentity);
            }
            let decoded = decode_route_need_sketch_v1(&branch.packet_bytes)?;
            if decoded.coarse_token_count() != 1
                || !sketch_ids.insert(decoded.sketch_id)
                || !reply_capabilities.insert(decoded.one_time_reply_capability)
                || !replay_nonces.insert(decoded.replay_nonce)
                || !salted_commitments.insert(decoded.salted_disclosure_commitment)
            {
                return Err(MultipathError::LinkableRouteEntropy);
            }
            planned.push(PlannedBranch {
                local_path_commitment: branch.local_path_commitment,
                packet_bytes: branch.packet_bytes,
                reply_capability: decoded.one_time_reply_capability,
            });
        }
        planned.sort_by_key(|branch| branch.local_path_commitment);
        Ok(Self { branches: planned })
    }

    pub fn outbound_packets(&self) -> impl Iterator<Item = &[u8]> {
        self.branches
            .iter()
            .map(|branch| branch.packet_bytes.as_slice())
    }

    pub fn branch_count(&self) -> usize {
        self.branches.len()
    }

    pub const fn contains_plaintext_correlation_field(&self) -> bool {
        false
    }

    pub const fn claims_transport_unlinkability(&self) -> bool {
        false
    }
}

/// Unforgeable-at-this-API receipt created only from a capsule accepted and
/// opened by the SEC-003 inbox. It prevents the multipath coordinator from
/// accidentally growing a raw/plaintext reply bypass.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct OpenedCapsuleReceipt {
    capsule_id: [u8; 32],
}

impl OpenedCapsuleReceipt {
    pub fn from_opened(opened: &OpenedDisclosure) -> Result<Self, MultipathError> {
        if opened.capsule_id == [0; 32] {
            return Err(MultipathError::InvalidReply);
        }
        Ok(Self {
            capsule_id: opened.capsule_id,
        })
    }

    pub const fn capsule_id(&self) -> [u8; 32] {
        self.capsule_id
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct OpenedMultipathReply {
    one_time_reply_capability: [u8; 32],
    opened_capsule: OpenedCapsuleReceipt,
    results: Vec<QueryArtifactRef>,
}

impl OpenedMultipathReply {
    pub fn new(
        one_time_reply_capability: [u8; 32],
        opened_capsule: OpenedCapsuleReceipt,
        results: Vec<QueryArtifactRef>,
    ) -> Self {
        Self {
            one_time_reply_capability,
            opened_capsule,
            results,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PathUnavailableReason {
    Dropped,
    TimedOutLocally,
    CarrierUnavailable,
    SuspectedEclipse,
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct BranchState {
    branch: PlannedBranch,
    response_commitment: Option<[u8; 32]>,
    unavailable_observation: Option<PathUnavailableReason>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MultipathReplyOutcome {
    Added { new_artifacts: usize },
    DeduplicatedOnly,
    ExactReplay,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct MultipathCoverage {
    pub planned_paths: usize,
    pub responded_paths: usize,
    pub unresolved_path_commitments: Vec<[u8; 32]>,
    pub unavailable_observations: Vec<([u8; 32], PathUnavailableReason)>,
}

impl MultipathCoverage {
    pub const fn is_globally_complete(&self) -> bool {
        false
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct MultipathUnionView {
    pub results: Vec<QueryArtifactRef>,
    pub result_set_root: [u8; 32],
    pub coverage: MultipathCoverage,
}

impl MultipathUnionView {
    pub const fn source_count_affects_rank(&self) -> bool {
        false
    }

    pub const fn is_execution_authority(&self) -> bool {
        false
    }
}

#[derive(Clone)]
pub struct MultipathCoordinator {
    max_results: usize,
    branches: BTreeMap<[u8; 32], BranchState>,
    results: BTreeMap<ArtifactIdentity, QueryArtifactRef>,
}

impl MultipathCoordinator {
    pub fn new(plan: MultipathQueryPlan, max_results: usize) -> Result<Self, MultipathError> {
        if max_results == 0 || max_results > MAX_MULTIPATH_RESULTS {
            return Err(MultipathError::InvalidCapacity);
        }
        let branches = plan
            .branches
            .into_iter()
            .map(|branch| {
                (
                    branch.reply_capability,
                    BranchState {
                        branch,
                        response_commitment: None,
                        unavailable_observation: None,
                    },
                )
            })
            .collect();
        Ok(Self {
            max_results,
            branches,
            results: BTreeMap::new(),
        })
    }

    pub fn accept_opened_reply(
        &mut self,
        reply: OpenedMultipathReply,
    ) -> Result<MultipathReplyOutcome, MultipathError> {
        let mut next = self.clone();
        let outcome = next.accept_opened_reply_in_place(reply)?;
        *self = next;
        Ok(outcome)
    }

    fn accept_opened_reply_in_place(
        &mut self,
        mut reply: OpenedMultipathReply,
    ) -> Result<MultipathReplyOutcome, MultipathError> {
        if reply.one_time_reply_capability == [0; 32]
            || reply.opened_capsule.capsule_id() == [0; 32]
            || reply.results.len() > MAX_MULTIPATH_REPLY_RESULTS
        {
            return Err(MultipathError::InvalidReply);
        }
        reply.results.sort_by_key(artifact_identity);
        let identities = reply
            .results
            .iter()
            .map(artifact_identity)
            .collect::<BTreeSet<_>>();
        if identities.len() != reply.results.len()
            || identities.iter().any(|identity| identity.cid == [0; 32])
        {
            return Err(MultipathError::DuplicateReplyResult);
        }
        let response_commitment = opened_reply_commitment(&reply)?;
        let branch = self
            .branches
            .get_mut(&reply.one_time_reply_capability)
            .ok_or(MultipathError::UnknownReplyCapability)?;
        if let Some(existing) = branch.response_commitment {
            return if existing == response_commitment {
                Ok(MultipathReplyOutcome::ExactReplay)
            } else {
                Err(MultipathError::ReplyCapabilityConsumed)
            };
        }

        let mut new_artifacts = 0usize;
        for artifact in &reply.results {
            let identity = artifact_identity(artifact);
            match self.results.get(&identity) {
                Some(existing) if existing != artifact => {
                    return Err(MultipathError::ArtifactMetadataConflict)
                }
                Some(_) => {}
                None => new_artifacts += 1,
            }
        }
        if self.results.len().saturating_add(new_artifacts) > self.max_results {
            return Err(MultipathError::ResultCapacityReached);
        }
        for artifact in reply.results {
            self.results
                .entry(artifact_identity(&artifact))
                .or_insert(artifact);
        }
        branch.response_commitment = Some(response_commitment);
        Ok(if new_artifacts == 0 {
            MultipathReplyOutcome::DeduplicatedOnly
        } else {
            MultipathReplyOutcome::Added { new_artifacts }
        })
    }

    pub fn observe_path_unavailable(
        &mut self,
        local_path_commitment: [u8; 32],
        reason: PathUnavailableReason,
    ) -> Result<(), MultipathError> {
        let branch = self
            .branches
            .values_mut()
            .find(|state| state.branch.local_path_commitment == local_path_commitment)
            .ok_or(MultipathError::UnknownPath)?;
        branch.unavailable_observation = Some(reason);
        Ok(())
    }

    pub fn view(&self) -> Result<MultipathUnionView, MultipathError> {
        let results = self.results.values().cloned().collect::<Vec<_>>();
        let result_set_root = digest_value(b"multipath-result-set", &artifact_set(&results)?)?;
        let mut unresolved_path_commitments = Vec::new();
        let mut unavailable_observations = Vec::new();
        let mut responded_paths = 0usize;
        for branch in self.branches.values() {
            if branch.response_commitment.is_some() {
                responded_paths += 1;
            } else {
                unresolved_path_commitments.push(branch.branch.local_path_commitment);
            }
            if let Some(reason) = branch.unavailable_observation {
                unavailable_observations.push((branch.branch.local_path_commitment, reason));
            }
        }
        unresolved_path_commitments.sort_unstable();
        unavailable_observations.sort_by_key(|(path, _)| *path);
        Ok(MultipathUnionView {
            results,
            result_set_root,
            coverage: MultipathCoverage {
                planned_paths: self.branches.len(),
                responded_paths,
                unresolved_path_commitments,
                unavailable_observations,
            },
        })
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
struct ArtifactIdentity {
    domain: u8,
    cid: [u8; 32],
}

fn artifact_identity(artifact: &QueryArtifactRef) -> ArtifactIdentity {
    match artifact {
        QueryArtifactRef::Object(reference) => ArtifactIdentity {
            domain: 0,
            cid: reference.cid,
        },
        QueryArtifactRef::Mapping(mapping) => ArtifactIdentity {
            domain: 1,
            cid: mapping.into_bytes(),
        },
        QueryArtifactRef::Event(event) => ArtifactIdentity {
            domain: 2,
            cid: event.into_bytes(),
        },
    }
}

fn artifact_value(artifact: &QueryArtifactRef) -> CanonicalValue {
    match artifact {
        QueryArtifactRef::Object(reference) => CanonicalValue::Map(vec![
            (0, CanonicalValue::Unsigned(0)),
            (1, CanonicalValue::Unsigned(reference.reference_kind)),
            (2, CanonicalValue::Bytes(reference.cid.to_vec())),
        ]),
        QueryArtifactRef::Mapping(mapping) => CanonicalValue::Map(vec![
            (0, CanonicalValue::Unsigned(1)),
            (2, CanonicalValue::Bytes(mapping.as_bytes().to_vec())),
        ]),
        QueryArtifactRef::Event(event) => CanonicalValue::Map(vec![
            (0, CanonicalValue::Unsigned(2)),
            (2, CanonicalValue::Bytes(event.as_bytes().to_vec())),
        ]),
    }
}

fn artifact_set(artifacts: &[QueryArtifactRef]) -> Result<CanonicalValue, MultipathError> {
    let members = artifacts
        .iter()
        .map(artifact_value)
        .map(|value| (value.clone(), value))
        .collect();
    Ok(CanonicalValue::Array(canonicalize_set_by_key(
        members,
        ResourceProfile::ObjectV1,
    )?))
}

fn opened_reply_commitment(reply: &OpenedMultipathReply) -> Result<[u8; 32], MultipathError> {
    digest_value(
        b"opened-multipath-reply",
        &CanonicalValue::Map(vec![
            (
                0,
                CanonicalValue::Bytes(reply.one_time_reply_capability.to_vec()),
            ),
            (
                1,
                CanonicalValue::Bytes(reply.opened_capsule.capsule_id().to_vec()),
            ),
            (2, artifact_set(&reply.results)?),
        ]),
    )
}

pub struct StandingNeedMailboxKey([u8; 32]);

impl StandingNeedMailboxKey {
    pub const fn from_bytes(bytes: [u8; 32]) -> Self {
        Self(bytes)
    }
}

impl Drop for StandingNeedMailboxKey {
    fn drop(&mut self) {
        self.0.zeroize();
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct StandingNeedNotification {
    pub standing_need_id: StandingNeedId,
    pub matches: Vec<QueryArtifactRef>,
    pub match_rule_commitment: [u8; 32],
    pub first_query_revision: [u8; 32],
}

impl StandingNeedNotification {
    pub fn new(
        standing_need: &StandingNeed,
        matches: Vec<QueryArtifactRef>,
        match_rule_commitment: [u8; 32],
        first_query_revision: [u8; 32],
    ) -> Result<Self, MultipathError> {
        standing_need.validate()?;
        if standing_need.state != StandingNeedState::Active {
            return Err(MultipathError::StandingNeedInactive);
        }
        let notification = Self {
            standing_need_id: standing_need.id()?,
            matches,
            match_rule_commitment,
            first_query_revision,
        };
        notification.validate()?;
        Ok(notification)
    }

    fn validate(&self) -> Result<(), MultipathError> {
        if self.matches.is_empty()
            || self.matches.len() > MAX_MULTIPATH_REPLY_RESULTS
            || self.match_rule_commitment == [0; 32]
            || self.first_query_revision == [0; 32]
        {
            return Err(MultipathError::InvalidNotification);
        }
        let identities = self
            .matches
            .iter()
            .map(artifact_identity)
            .collect::<BTreeSet<_>>();
        if identities.len() != self.matches.len()
            || identities.iter().any(|identity| identity.cid == [0; 32])
        {
            return Err(MultipathError::InvalidNotification);
        }
        Ok(())
    }

    /// Stable local exactly-once key excludes QueryView revision so the same
    /// match reappearing in a later revision does not notify twice.
    pub fn notification_id(&self) -> Result<[u8; 32], MultipathError> {
        self.validate()?;
        digest_value(
            b"standing-need-notification-id",
            &CanonicalValue::Map(vec![
                (
                    0,
                    CanonicalValue::Bytes(self.standing_need_id.as_bytes().to_vec()),
                ),
                (1, artifact_set(&self.matches)?),
                (
                    2,
                    CanonicalValue::Bytes(self.match_rule_commitment.to_vec()),
                ),
            ]),
        )
    }

    fn canonical_bytes(&self) -> Result<Vec<u8>, MultipathError> {
        self.validate()?;
        Ok(encode_canonical(
            &CanonicalValue::Map(vec![
                (0, CanonicalValue::Unsigned(1)),
                (
                    1,
                    CanonicalValue::Bytes(self.standing_need_id.as_bytes().to_vec()),
                ),
                (2, artifact_set(&self.matches)?),
                (
                    3,
                    CanonicalValue::Bytes(self.match_rule_commitment.to_vec()),
                ),
                (4, CanonicalValue::Bytes(self.first_query_revision.to_vec())),
                (
                    5,
                    CanonicalValue::Unsigned(
                        ku_core::foundation::DisclosureClass::LocalOnly as u64,
                    ),
                ),
            ]),
            ResourceProfile::ObjectV1,
        )?)
    }

    fn decode(bytes: &[u8]) -> Result<Self, MultipathError> {
        let value = decode_canonical(bytes, ResourceProfile::ObjectV1)?;
        let CanonicalValue::Map(map) = &value else {
            return Err(MultipathError::InvalidNotification);
        };
        if unsigned(map, 0)? != 1
            || unsigned(map, 5)? != ku_core::foundation::DisclosureClass::LocalOnly as u64
        {
            return Err(MultipathError::InvalidNotification);
        }
        let notification = Self {
            standing_need_id: StandingNeedId::from_bytes(bytes32(map, 1)?),
            matches: parse_artifact_set(required(map, 2)?)?,
            match_rule_commitment: bytes32(map, 3)?,
            first_query_revision: bytes32(map, 4)?,
        };
        notification.validate()?;
        if notification.canonical_bytes()? != bytes {
            return Err(MultipathError::NonCanonicalNotification);
        }
        Ok(notification)
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct SealedNotification {
    nonce: [u8; 24],
    ciphertext: Vec<u8>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct StandingNeedMailboxSnapshot {
    entries: BTreeMap<[u8; 32], SealedNotification>,
    delivered: BTreeSet<[u8; 32]>,
    used_nonces: BTreeSet<[u8; 24]>,
    capacity: usize,
}

impl StandingNeedMailboxSnapshot {
    pub const fn is_local_private(&self) -> bool {
        true
    }

    pub fn sealed_bytes(&self) -> impl Iterator<Item = &[u8]> {
        self.entries
            .values()
            .map(|entry| entry.ciphertext.as_slice())
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MailboxEnqueueOutcome {
    Enqueued,
    ExactReplayPending,
    AlreadyDelivered,
}

pub struct EncryptedStandingNeedMailbox {
    snapshot: StandingNeedMailboxSnapshot,
}

impl EncryptedStandingNeedMailbox {
    pub fn new(capacity: usize) -> Result<Self, MultipathError> {
        if capacity == 0 || capacity > MAX_MAILBOX_NOTIFICATIONS {
            return Err(MultipathError::InvalidCapacity);
        }
        Ok(Self {
            snapshot: StandingNeedMailboxSnapshot {
                entries: BTreeMap::new(),
                delivered: BTreeSet::new(),
                used_nonces: BTreeSet::new(),
                capacity,
            },
        })
    }

    pub fn from_snapshot(snapshot: StandingNeedMailboxSnapshot) -> Result<Self, MultipathError> {
        if snapshot.capacity == 0
            || snapshot.capacity > MAX_MAILBOX_NOTIFICATIONS
            || snapshot.entries.len() > snapshot.capacity
            || !snapshot
                .delivered
                .is_subset(&snapshot.entries.keys().copied().collect())
            || snapshot.entries.values().any(|entry| {
                entry.nonce == [0; 24]
                    || entry.ciphertext.is_empty()
                    || entry.ciphertext.len() > MAX_MAILBOX_CIPHERTEXT_BYTES
                    || !snapshot.used_nonces.contains(&entry.nonce)
            })
        {
            return Err(MultipathError::InvalidMailboxSnapshot);
        }
        Ok(Self { snapshot })
    }

    pub fn enqueue(
        &mut self,
        notification: &StandingNeedNotification,
        nonce: [u8; 24],
        key: &StandingNeedMailboxKey,
    ) -> Result<MailboxEnqueueOutcome, MultipathError> {
        let notification_id = notification.notification_id()?;
        if self.snapshot.entries.contains_key(&notification_id) {
            return Ok(if self.snapshot.delivered.contains(&notification_id) {
                MailboxEnqueueOutcome::AlreadyDelivered
            } else {
                MailboxEnqueueOutcome::ExactReplayPending
            });
        }
        if self.snapshot.entries.len() >= self.snapshot.capacity {
            return Err(MultipathError::MailboxCapacityReached);
        }
        if nonce == [0; 24] || !self.snapshot.used_nonces.insert(nonce) {
            return Err(MultipathError::MailboxNonceReuse);
        }
        let plaintext = notification.canonical_bytes()?;
        let aad = mailbox_aad(notification_id);
        let cipher = XChaCha20Poly1305::new((&key.0).into());
        let nonce_ref = XNonce::try_from(&nonce[..]).map_err(|_| MultipathError::MailboxCrypto)?;
        let ciphertext = cipher
            .encrypt(
                &nonce_ref,
                Payload {
                    msg: &plaintext,
                    aad: &aad,
                },
            )
            .map_err(|_| MultipathError::MailboxCrypto)?;
        if ciphertext.len() > MAX_MAILBOX_CIPHERTEXT_BYTES {
            return Err(MultipathError::MailboxCapacityReached);
        }
        self.snapshot
            .entries
            .insert(notification_id, SealedNotification { nonce, ciphertext });
        Ok(MailboxEnqueueOutcome::Enqueued)
    }

    pub fn claim_next(
        &mut self,
        key: &StandingNeedMailboxKey,
    ) -> Result<Option<StandingNeedNotification>, MultipathError> {
        let Some((notification_id, entry)) = self
            .snapshot
            .entries
            .iter()
            .find(|(id, _)| !self.snapshot.delivered.contains(*id))
            .map(|(id, entry)| (*id, entry.clone()))
        else {
            return Ok(None);
        };
        let cipher = XChaCha20Poly1305::new((&key.0).into());
        let nonce_ref =
            XNonce::try_from(&entry.nonce[..]).map_err(|_| MultipathError::MailboxCrypto)?;
        let plaintext = cipher
            .decrypt(
                &nonce_ref,
                Payload {
                    msg: &entry.ciphertext,
                    aad: &mailbox_aad(notification_id),
                },
            )
            .map_err(|_| MultipathError::MailboxCrypto)?;
        let notification = StandingNeedNotification::decode(&plaintext)?;
        if notification.notification_id()? != notification_id {
            return Err(MultipathError::MailboxIdentityMismatch);
        }
        self.snapshot.delivered.insert(notification_id);
        Ok(Some(notification))
    }

    pub fn snapshot(&self) -> StandingNeedMailboxSnapshot {
        self.snapshot.clone()
    }

    pub const fn is_local_private(&self) -> bool {
        true
    }

    pub const fn is_publication_path(&self) -> bool {
        false
    }
}

fn mailbox_aad(notification_id: [u8; 32]) -> Vec<u8> {
    let mut aad = b"onebrain:vnext:standing-need-mailbox:1\0".to_vec();
    aad.extend_from_slice(&notification_id);
    aad
}

fn parse_artifact_set(value: &CanonicalValue) -> Result<Vec<QueryArtifactRef>, MultipathError> {
    let CanonicalValue::Array(values) = value else {
        return Err(MultipathError::InvalidNotification);
    };
    values.iter().map(parse_artifact).collect()
}

fn parse_artifact(value: &CanonicalValue) -> Result<QueryArtifactRef, MultipathError> {
    let CanonicalValue::Map(map) = value else {
        return Err(MultipathError::InvalidNotification);
    };
    match unsigned(map, 0)? {
        0 => Ok(QueryArtifactRef::Object(ObjectReference::new(
            unsigned(map, 1)?,
            bytes32(map, 2)?,
        ))),
        1 => Ok(QueryArtifactRef::Mapping(MappingKernelCid::from_bytes(
            bytes32(map, 2)?,
        ))),
        2 => Ok(QueryArtifactRef::Event(EventCid::from_bytes(bytes32(
            map, 2,
        )?))),
        _ => Err(MultipathError::InvalidNotification),
    }
}

fn required(map: &[(u64, CanonicalValue)], key: u64) -> Result<&CanonicalValue, MultipathError> {
    map.iter()
        .find_map(|(candidate, value)| (*candidate == key).then_some(value))
        .ok_or(MultipathError::InvalidNotification)
}

fn unsigned(map: &[(u64, CanonicalValue)], key: u64) -> Result<u64, MultipathError> {
    match required(map, key)? {
        CanonicalValue::Unsigned(value) => Ok(*value),
        _ => Err(MultipathError::InvalidNotification),
    }
}

fn bytes32(map: &[(u64, CanonicalValue)], key: u64) -> Result<[u8; 32], MultipathError> {
    match required(map, key)? {
        CanonicalValue::Bytes(bytes) if bytes.len() == 32 => {
            let mut output = [0u8; 32];
            output.copy_from_slice(bytes);
            Ok(output)
        }
        _ => Err(MultipathError::InvalidNotification),
    }
}

fn digest_value(label: &[u8], value: &CanonicalValue) -> Result<[u8; 32], MultipathError> {
    let bytes = encode_canonical(value, ResourceProfile::ObjectV1)?;
    Ok(digest_bytes(label, &bytes))
}

fn digest_bytes(label: &[u8], bytes: &[u8]) -> [u8; 32] {
    let mut hasher = blake3::Hasher::new();
    hasher.update(b"onebrain:vnext:kql-multipath:1\0");
    hasher.update(label);
    hasher.update(&[0]);
    hasher.update(&(bytes.len() as u64).to_be_bytes());
    hasher.update(bytes);
    *hasher.finalize().as_bytes()
}

#[derive(Debug, PartialEq, Eq)]
pub enum MultipathError {
    Canonical(ku_core::foundation::CanonicalError),
    Route(RoutePacketError),
    StandingNeed(StandingNeedError),
    BranchLimit,
    DuplicateBranchIdentity,
    LinkableRouteEntropy,
    InvalidCapacity,
    InvalidReply,
    DuplicateReplyResult,
    UnknownReplyCapability,
    ReplyCapabilityConsumed,
    ArtifactMetadataConflict,
    ResultCapacityReached,
    UnknownPath,
    StandingNeedInactive,
    InvalidNotification,
    NonCanonicalNotification,
    InvalidMailboxSnapshot,
    MailboxCapacityReached,
    MailboxNonceReuse,
    MailboxCrypto,
    MailboxIdentityMismatch,
}

impl From<ku_core::foundation::CanonicalError> for MultipathError {
    fn from(error: ku_core::foundation::CanonicalError) -> Self {
        Self::Canonical(error)
    }
}

impl From<RoutePacketError> for MultipathError {
    fn from(error: RoutePacketError) -> Self {
        Self::Route(error)
    }
}

impl From<StandingNeedError> for MultipathError {
    fn from(error: StandingNeedError) -> Self {
        Self::StandingNeed(error)
    }
}

#[cfg(test)]
mod tests {
    use ku_core::foundation::{public_knowledge_exchange_fixture_v1, ObjectCid};

    use super::*;
    use crate::vnext_query::{
        CoarseRouteToken, CoarseRouteTokenClass, DisclosureCompiler, QueryRun, RouteSketchEntropy,
        MIN_ROUTE_TOKEN_SUPPORT,
    };

    fn run() -> QueryRun {
        QueryRun::new(
            [0xab; 32],
            ObjectCid::from_bytes([0xaa; 32]),
            public_knowledge_exchange_fixture_v1(),
        )
        .unwrap()
    }

    fn route_plan(count: u8) -> MultipathQueryPlan {
        let run = run();
        let mut compiler = DisclosureCompiler::default();
        let branches = (0..count)
            .map(|offset| {
                let base = 10 + offset * 10;
                let sketch = compiler
                    .compile_route_minimal(
                        &run,
                        CoarseRouteToken {
                            class: CoarseRouteTokenClass::CoarseRole,
                            allowlisted_code: 100 + u16::from(offset),
                        },
                        MIN_ROUTE_TOKEN_SUPPORT,
                        1,
                        20,
                        3,
                        1,
                        RouteSketchEntropy {
                            sketch_id: [base; 32],
                            one_time_reply_capability: [base + 1; 32],
                            replay_nonce: [base + 2; 32],
                            commitment_salt: [base + 3; 32],
                        },
                    )
                    .unwrap();
                MultipathBranchPacket {
                    local_path_commitment: [base + 4; 32],
                    packet_bytes: sketch.network_bytes().unwrap(),
                }
            })
            .collect();
        MultipathQueryPlan::new(branches).unwrap()
    }

    fn reply(capability: u8, capsule: u8, results: &[u8]) -> OpenedMultipathReply {
        OpenedMultipathReply::new(
            [capability; 32],
            OpenedCapsuleReceipt {
                capsule_id: [capsule; 32],
            },
            results
                .iter()
                .map(|result| QueryArtifactRef::Object(ObjectReference::new(2, [*result; 32])))
                .collect(),
        )
    }

    fn standing_need() -> StandingNeed {
        StandingNeed::new_local(
            ObjectReference::new(3, [1; 32]),
            ObjectCid::from_bytes([2; 32]),
            ku_core::foundation::SelectorCid::from_bytes([3; 32]),
            ObjectReference::new(21, [4; 32]),
            [5; 32],
        )
    }

    #[test]
    fn three_packets_are_schema_unlinkable_and_private_ids_absent() {
        let plan = route_plan(3);
        assert_eq!(plan.branch_count(), 3);
        assert!(!plan.contains_plaintext_correlation_field());
        assert!(!plan.claims_transport_unlinkability());
        let packets = plan.outbound_packets().collect::<Vec<_>>();
        for packet in packets {
            let decoded = decode_route_need_sketch_v1(packet).unwrap();
            assert_eq!(decoded.coarse_token_count(), 1);
            assert!(!packet.windows(32).any(|window| window == [0xaa; 32]));
            assert!(!packet.windows(32).any(|window| window == [0xab; 32]));
        }
    }

    #[test]
    fn reordered_paths_produce_same_canonical_union() {
        let plan = route_plan(3);
        let mut left = MultipathCoordinator::new(plan.clone(), 10).unwrap();
        let mut right = MultipathCoordinator::new(plan, 10).unwrap();
        let replies = [reply(11, 1, &[1, 2]), reply(21, 2, &[2, 3])];
        for item in replies.iter().cloned() {
            left.accept_opened_reply(item).unwrap();
        }
        for item in replies.iter().rev().cloned() {
            right.accept_opened_reply(item).unwrap();
        }
        assert_eq!(left.view().unwrap(), right.view().unwrap());
        assert_eq!(left.view().unwrap().results.len(), 3);
    }

    #[test]
    fn eclipse_or_drop_remains_partial_without_disabling_local_results() {
        let plan = route_plan(3);
        let mut coordinator = MultipathCoordinator::new(plan, 10).unwrap();
        coordinator
            .observe_path_unavailable([24; 32], PathUnavailableReason::SuspectedEclipse)
            .unwrap();
        coordinator.accept_opened_reply(reply(11, 1, &[7])).unwrap();
        let view = coordinator.view().unwrap();
        assert_eq!(view.results.len(), 1);
        assert_eq!(view.coverage.responded_paths, 1);
        assert_eq!(view.coverage.unresolved_path_commitments.len(), 2);
        assert!(!view.coverage.is_globally_complete());
        assert!(!view.is_execution_authority());
    }

    #[test]
    fn replay_and_cross_path_same_cid_never_boost_union() {
        let plan = route_plan(2);
        let mut coordinator = MultipathCoordinator::new(plan, 10).unwrap();
        let first = reply(11, 1, &[9]);
        assert_eq!(
            coordinator.accept_opened_reply(first.clone()).unwrap(),
            MultipathReplyOutcome::Added { new_artifacts: 1 }
        );
        assert_eq!(
            coordinator.accept_opened_reply(first).unwrap(),
            MultipathReplyOutcome::ExactReplay
        );
        assert_eq!(
            coordinator.accept_opened_reply(reply(21, 2, &[9])).unwrap(),
            MultipathReplyOutcome::DeduplicatedOnly
        );
        let view = coordinator.view().unwrap();
        assert_eq!(view.results.len(), 1);
        assert!(!view.source_count_affects_rank());
    }

    #[test]
    fn mailbox_is_encrypted_restart_safe_and_exactly_once() {
        let need = standing_need();
        let notification = StandingNeedNotification::new(
            &need,
            vec![QueryArtifactRef::Object(ObjectReference::new(2, [9; 32]))],
            [10; 32],
            [11; 32],
        )
        .unwrap();
        let need_id = need.id().unwrap();
        let key = StandingNeedMailboxKey::from_bytes([12; 32]);
        let wrong = StandingNeedMailboxKey::from_bytes([13; 32]);
        let mut mailbox = EncryptedStandingNeedMailbox::new(10).unwrap();
        assert_eq!(
            mailbox.enqueue(&notification, [14; 24], &key).unwrap(),
            MailboxEnqueueOutcome::Enqueued
        );
        assert!(mailbox
            .snapshot()
            .sealed_bytes()
            .all(|ciphertext| !ciphertext
                .windows(32)
                .any(|window| window == need_id.as_bytes())));
        assert_eq!(
            mailbox.claim_next(&wrong).unwrap_err(),
            MultipathError::MailboxCrypto
        );
        let snapshot = mailbox.snapshot();
        let mut reopened = EncryptedStandingNeedMailbox::from_snapshot(snapshot).unwrap();
        assert_eq!(
            reopened.claim_next(&key).unwrap(),
            Some(notification.clone())
        );
        assert_eq!(reopened.claim_next(&key).unwrap(), None);
        assert_eq!(
            reopened.enqueue(&notification, [15; 24], &key).unwrap(),
            MailboxEnqueueOutcome::AlreadyDelivered
        );
    }

    #[test]
    fn later_query_revision_of_same_match_does_not_notify_again() {
        let need = standing_need();
        let first = StandingNeedNotification::new(
            &need,
            vec![QueryArtifactRef::Object(ObjectReference::new(2, [9; 32]))],
            [10; 32],
            [11; 32],
        )
        .unwrap();
        let later = StandingNeedNotification::new(
            &need,
            vec![QueryArtifactRef::Object(ObjectReference::new(2, [9; 32]))],
            [10; 32],
            [99; 32],
        )
        .unwrap();
        assert_eq!(
            first.notification_id().unwrap(),
            later.notification_id().unwrap()
        );
        let key = StandingNeedMailboxKey::from_bytes([12; 32]);
        let mut mailbox = EncryptedStandingNeedMailbox::new(10).unwrap();
        mailbox.enqueue(&first, [1; 24], &key).unwrap();
        assert_eq!(
            mailbox.enqueue(&later, [2; 24], &key).unwrap(),
            MailboxEnqueueOutcome::ExactReplayPending
        );
        assert_eq!(mailbox.claim_next(&key).unwrap(), Some(first));
        assert_eq!(mailbox.claim_next(&key).unwrap(), None);
    }
}

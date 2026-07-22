//! Exact high-water anchors, shadow compaction, restore drills and local GC gates.
//!
//! All decisions in this module are local. A checkpoint may suppress covered
//! payloads from one local read path after proof validation, but it never
//! creates a network-wide deletion or completeness claim.

use std::collections::{BTreeMap, BTreeSet};
use std::fmt;

use ed25519_dalek::{Signature, Signer, SigningKey, Verifier, VerifyingKey};

use super::authority::FeedAuthorityDecision;
use super::canonical::{encode_canonical, CanonicalError, CanonicalValue, ResourceProfile};
use super::checkpoint::{
    MerkleInclusionProof, ValidatedCheckpointSuppression, ValidatedFeedCheckpoint,
};
use super::content_id::{signature_message, CheckpointCid, ManifestCid, ReservedDomain};
use super::feed::ValidatedFeedInception;
use super::identity::FeedId;
use super::key_state::KeyStateCheckpointProof;

pub const COMPACTION_PROFILE_MAJOR: u64 = 1;
pub const COMPACTION_PROFILE_MINOR: u64 = 0;
pub const MAX_HIGH_WATER_ANCHORS: usize = 65_536;
pub const MAX_COMPACTION_RECORDS: usize = 65_536;

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
#[repr(u8)]
pub enum HighWaterLane {
    ProviderLeaseGeneration = 0,
    ProviderRetirementFloor = 1,
    PermitGeneration = 2,
    KeyGeneration = 3,
    FeedCheckpointPosition = 4,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ExactHighWaterEntry {
    pub lane: HighWaterLane,
    pub subject: [u8; 32],
    pub high_water: u64,
    pub record_ids: BTreeSet<[u8; 32]>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum HighWaterObserveOutcome {
    Initialized,
    Advanced,
    ConflictAtHighWaterRetained,
    ExactReplay,
    BelowHighWaterInactive,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct ExactHighWaterAnchors {
    entries: BTreeMap<(HighWaterLane, [u8; 32]), ExactHighWaterEntry>,
}

impl ExactHighWaterAnchors {
    pub fn observe(
        &mut self,
        lane: HighWaterLane,
        subject: [u8; 32],
        high_water: u64,
        record_id: [u8; 32],
    ) -> Result<HighWaterObserveOutcome, CompactionError> {
        if subject == [0; 32] || record_id == [0; 32] {
            return Err(CompactionError::InvalidAnchor);
        }
        let key = (lane, subject);
        let Some(existing) = self.entries.get_mut(&key) else {
            if self.entries.len() == MAX_HIGH_WATER_ANCHORS {
                return Err(CompactionError::ResourceLimit);
            }
            self.entries.insert(
                key,
                ExactHighWaterEntry {
                    lane,
                    subject,
                    high_water,
                    record_ids: BTreeSet::from([record_id]),
                },
            );
            return Ok(HighWaterObserveOutcome::Initialized);
        };
        if high_water < existing.high_water {
            return Ok(HighWaterObserveOutcome::BelowHighWaterInactive);
        }
        if high_water > existing.high_water {
            existing.high_water = high_water;
            existing.record_ids.clear();
            existing.record_ids.insert(record_id);
            return Ok(HighWaterObserveOutcome::Advanced);
        }
        if !existing.record_ids.insert(record_id) {
            Ok(HighWaterObserveOutcome::ExactReplay)
        } else {
            Ok(HighWaterObserveOutcome::ConflictAtHighWaterRetained)
        }
    }

    pub fn merge(&mut self, other: &Self) -> Result<(), CompactionError> {
        for entry in other.entries.values() {
            for record_id in &entry.record_ids {
                self.observe(entry.lane, entry.subject, entry.high_water, *record_id)?;
            }
        }
        Ok(())
    }

    pub fn entry(&self, lane: HighWaterLane, subject: [u8; 32]) -> Option<&ExactHighWaterEntry> {
        self.entries.get(&(lane, subject))
    }

    pub fn entries(&self) -> impl Iterator<Item = &ExactHighWaterEntry> {
        self.entries.values()
    }

    pub fn root(&self) -> Result<[u8; 32], CompactionError> {
        let entries = self
            .entries
            .values()
            .map(|entry| {
                CanonicalValue::Map(vec![
                    (0, CanonicalValue::Unsigned(entry.lane as u64)),
                    (1, CanonicalValue::Bytes(entry.subject.to_vec())),
                    (2, CanonicalValue::Unsigned(entry.high_water)),
                    (
                        3,
                        CanonicalValue::Array(
                            entry
                                .record_ids
                                .iter()
                                .map(|id| CanonicalValue::Bytes(id.to_vec()))
                                .collect(),
                        ),
                    ),
                ])
            })
            .collect();
        digest_value(
            b"exact-high-water-anchors/1",
            &CanonicalValue::Map(vec![
                (0, CanonicalValue::Unsigned(COMPACTION_PROFILE_MAJOR)),
                (1, CanonicalValue::Array(entries)),
            ]),
        )
    }

    pub const fn is_global_retirement_or_delete(&self) -> bool {
        false
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub struct PayloadRecordId {
    pub record_kind: u64,
    pub cid: [u8; 32],
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
#[repr(u8)]
pub enum PayloadClass {
    CanonicalEvent = 0,
    CanonicalObject = 1,
    PrivateSource = 2,
    DerivedCache = 3,
    AuthorityAnchor = 4,
    CheckpointAnchor = 5,
    QuarantineEvidence = 6,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PayloadDescriptor {
    pub id: PayloadRecordId,
    pub class: PayloadClass,
    pub byte_len: u64,
    pub bytes_digest: [u8; 32],
}

impl PayloadDescriptor {
    fn validate(&self) -> Result<(), CompactionError> {
        if self.id.cid == [0; 32] || self.byte_len == 0 || self.bytes_digest == [0; 32] {
            return Err(CompactionError::InvalidPayload);
        }
        Ok(())
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ProofedPayload {
    pub descriptor: PayloadDescriptor,
    /// The event whose accepted reducer effect covers this payload.
    pub covering_event: Option<[u8; 32]>,
    pub inclusion_proof: Option<MerkleInclusionProof>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ArchiveEntry {
    pub descriptor: PayloadDescriptor,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ArchiveManifest {
    pub checkpoint_cid: CheckpointCid,
    pub anchors_root: [u8; 32],
    pub entries_root: [u8; 32],
    pub entries: Vec<ArchiveEntry>,
    pub manifest_cid: ManifestCid,
}

impl ArchiveManifest {
    pub fn new(
        checkpoint_cid: CheckpointCid,
        anchors_root: [u8; 32],
        mut entries: Vec<ArchiveEntry>,
    ) -> Result<Self, CompactionError> {
        if anchors_root == [0; 32] || entries.len() > MAX_COMPACTION_RECORDS {
            return Err(CompactionError::InvalidArchive);
        }
        entries.sort_by_key(|entry| entry.descriptor.id);
        for entry in &entries {
            entry.descriptor.validate()?;
        }
        if entries
            .windows(2)
            .any(|pair| pair[0].descriptor.id == pair[1].descriptor.id)
        {
            return Err(CompactionError::InvalidArchive);
        }
        let entry_values = entries
            .iter()
            .map(|entry| payload_value(&entry.descriptor))
            .collect::<Vec<_>>();
        let entries_root = digest_value(
            b"archive-entry-set/1",
            &CanonicalValue::Array(entry_values.clone()),
        )?;
        let value = CanonicalValue::Map(vec![
            (0, CanonicalValue::Unsigned(COMPACTION_PROFILE_MAJOR)),
            (1, CanonicalValue::Bytes(checkpoint_cid.as_bytes().to_vec())),
            (2, CanonicalValue::Bytes(anchors_root.to_vec())),
            (3, CanonicalValue::Bytes(entries_root.to_vec())),
            (4, CanonicalValue::Array(entry_values)),
        ]);
        let bytes = encode_canonical(&value, ResourceProfile::ManifestV1)?;
        let manifest_cid = ManifestCid::compute(ReservedDomain::Manifest, &bytes)
            .map_err(|_| CompactionError::InvalidArchive)?;
        Ok(Self {
            checkpoint_cid,
            anchors_root,
            entries_root,
            entries,
            manifest_cid,
        })
    }

    pub fn contains_exact(&self, descriptor: &PayloadDescriptor) -> bool {
        self.entries
            .binary_search_by_key(&descriptor.id, |entry| entry.descriptor.id)
            .ok()
            .is_some_and(|index| self.entries[index].descriptor == *descriptor)
    }

    pub fn is_self_consistent(&self) -> bool {
        Self::new(self.checkpoint_cid, self.anchors_root, self.entries.clone())
            .is_ok_and(|rebuilt| rebuilt == *self)
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CustodyReceiptBody {
    pub archive_manifest: ManifestCid,
    pub entries_root: [u8; 32],
    pub anchors_root: [u8; 32],
    pub custodian_feed: FeedId,
    pub key_state_frontier: super::content_id::EventCid,
    pub nonce: [u8; 32],
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SignedCustodyReceipt {
    pub body: CustodyReceiptBody,
    pub signature: [u8; 64],
}

impl SignedCustodyReceipt {
    pub fn sign(
        archive: &ArchiveManifest,
        custodian: &ValidatedFeedInception,
        key_state: &KeyStateCheckpointProof,
        nonce: [u8; 32],
        signing_key: &SigningKey,
    ) -> Result<Self, CompactionError> {
        if nonce == [0; 32]
            || signing_key.verifying_key().as_bytes() != &custodian.signed.inception.feed_public_key
            || key_state.subject_feed != custodian.feed_id
            || key_state.frontier.as_bytes() == &[0; 32]
        {
            return Err(CompactionError::InvalidCustody);
        }
        let body = CustodyReceiptBody {
            archive_manifest: archive.manifest_cid,
            entries_root: archive.entries_root,
            anchors_root: archive.anchors_root,
            custodian_feed: custodian.feed_id,
            key_state_frontier: key_state.frontier,
            nonce,
        };
        let bytes = custody_body_bytes(&body)?;
        let message = signature_message(ReservedDomain::Manifest, &bytes)
            .map_err(|_| CompactionError::InvalidCustody)?;
        Ok(Self {
            body,
            signature: signing_key.sign(&message).to_bytes(),
        })
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ValidatedCustodyReceipt {
    body: CustodyReceiptBody,
    receipt_cid: ManifestCid,
}

impl ValidatedCustodyReceipt {
    pub fn body(&self) -> &CustodyReceiptBody {
        &self.body
    }

    pub const fn receipt_cid(&self) -> ManifestCid {
        self.receipt_cid
    }
}

pub fn validate_custody_receipt(
    signed: &SignedCustodyReceipt,
    custodian: &ValidatedFeedInception,
    key_state: &KeyStateCheckpointProof,
) -> Result<ValidatedCustodyReceipt, CompactionError> {
    if signed.body.custodian_feed != custodian.feed_id
        || key_state.subject_feed != custodian.feed_id
        || signed.body.key_state_frontier != key_state.frontier
        || !matches!(
            key_state.decision,
            FeedAuthorityDecision::AuthorizedRelative { frontier, .. } if frontier == key_state.frontier
        )
    {
        return Err(CompactionError::InvalidCustody);
    }
    let body_bytes = custody_body_bytes(&signed.body)?;
    let message = signature_message(ReservedDomain::Manifest, &body_bytes)
        .map_err(|_| CompactionError::InvalidCustody)?;
    let key = VerifyingKey::from_bytes(&custodian.signed.inception.feed_public_key)
        .map_err(|_| CompactionError::InvalidCustody)?;
    key.verify(&message, &Signature::from_bytes(&signed.signature))
        .map_err(|_| CompactionError::InvalidCustody)?;
    let signed_value = CanonicalValue::Map(vec![
        (0, CanonicalValue::Bytes(body_bytes)),
        (1, CanonicalValue::Bytes(signed.signature.to_vec())),
    ]);
    let signed_bytes = encode_canonical(&signed_value, ResourceProfile::ControlV1)?;
    Ok(ValidatedCustodyReceipt {
        body: signed.body.clone(),
        receipt_cid: ManifestCid::compute(ReservedDomain::Manifest, &signed_bytes)
            .map_err(|_| CompactionError::InvalidCustody)?,
    })
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ShadowBlockReason {
    KillSwitch,
    SuppressionNotAuthorized,
    AnchorRootMismatch,
    ViewParityMismatch,
    MissingCoverageProof,
    ProtectedClass,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ShadowCompactionPlan {
    checkpoint_cid: CheckpointCid,
    anchors_root: [u8; 32],
    live_view_root: [u8; 32],
    rebuilt_view_root: [u8; 32],
    candidates: Vec<PayloadDescriptor>,
    archive_manifest: ArchiveManifest,
    audit_root: [u8; 32],
    deletion_performed: bool,
}

impl ShadowCompactionPlan {
    pub const fn is_dry_run(&self) -> bool {
        !self.deletion_performed
    }

    pub const fn checkpoint_cid(&self) -> CheckpointCid {
        self.checkpoint_cid
    }

    pub const fn anchors_root(&self) -> [u8; 32] {
        self.anchors_root
    }

    pub const fn live_view_root(&self) -> [u8; 32] {
        self.live_view_root
    }

    pub const fn rebuilt_view_root(&self) -> [u8; 32] {
        self.rebuilt_view_root
    }

    pub fn candidates(&self) -> &[PayloadDescriptor] {
        &self.candidates
    }

    pub fn archive_manifest(&self) -> &ArchiveManifest {
        &self.archive_manifest
    }

    pub const fn audit_root(&self) -> [u8; 32] {
        self.audit_root
    }
}

pub struct ShadowCompactionPlanner;

impl ShadowCompactionPlanner {
    #[allow(clippy::too_many_arguments)]
    pub fn plan(
        checkpoint: &ValidatedFeedCheckpoint,
        suppression: &ValidatedCheckpointSuppression,
        anchors: &ExactHighWaterAnchors,
        payloads: Vec<ProofedPayload>,
        live_view_root: [u8; 32],
        rebuilt_view_root: [u8; 32],
        kill_switch_allows_shadow: bool,
    ) -> Result<ShadowCompactionPlan, ShadowBlockReason> {
        if !kill_switch_allows_shadow {
            return Err(ShadowBlockReason::KillSwitch);
        }
        if suppression.checkpoint_cid() != checkpoint.checkpoint_cid
            || suppression.covered_sequence() != checkpoint.signed.body.covered_sequence
        {
            return Err(ShadowBlockReason::SuppressionNotAuthorized);
        }
        let anchors_root = anchors
            .root()
            .map_err(|_| ShadowBlockReason::AnchorRootMismatch)?;
        if anchors_root != checkpoint.signed.body.retirement_floor_root {
            return Err(ShadowBlockReason::AnchorRootMismatch);
        }
        if live_view_root == [0; 32]
            || live_view_root != rebuilt_view_root
            || rebuilt_view_root == [0; 32]
        {
            return Err(ShadowBlockReason::ViewParityMismatch);
        }
        if payloads.len() > MAX_COMPACTION_RECORDS {
            return Err(ShadowBlockReason::MissingCoverageProof);
        }
        let mut candidates = Vec::new();
        for payload in payloads {
            payload
                .descriptor
                .validate()
                .map_err(|_| ShadowBlockReason::MissingCoverageProof)?;
            if matches!(
                payload.descriptor.class,
                PayloadClass::AuthorityAnchor
                    | PayloadClass::CheckpointAnchor
                    | PayloadClass::QuarantineEvidence
            ) {
                return Err(ShadowBlockReason::ProtectedClass);
            }
            if payload.descriptor.class != PayloadClass::DerivedCache {
                let proof = payload
                    .inclusion_proof
                    .as_ref()
                    .ok_or(ShadowBlockReason::MissingCoverageProof)?;
                proof
                    .validate(checkpoint)
                    .map_err(|_| ShadowBlockReason::MissingCoverageProof)?;
                if payload.covering_event != Some(*proof.leaf.event_cid().as_bytes()) {
                    return Err(ShadowBlockReason::MissingCoverageProof);
                }
            }
            candidates.push(payload.descriptor);
        }
        candidates.sort_by_key(|descriptor| descriptor.id);
        if candidates.windows(2).any(|pair| pair[0].id == pair[1].id) {
            return Err(ShadowBlockReason::MissingCoverageProof);
        }
        let archive_manifest = ArchiveManifest::new(
            checkpoint.checkpoint_cid,
            anchors_root,
            candidates
                .iter()
                .cloned()
                .map(|descriptor| ArchiveEntry { descriptor })
                .collect(),
        )
        .map_err(|_| ShadowBlockReason::MissingCoverageProof)?;
        let audit_root = shadow_audit_root(
            checkpoint.checkpoint_cid,
            anchors_root,
            live_view_root,
            &candidates,
        )
        .map_err(|_| ShadowBlockReason::MissingCoverageProof)?;
        Ok(ShadowCompactionPlan {
            checkpoint_cid: checkpoint.checkpoint_cid,
            anchors_root,
            live_view_root,
            rebuilt_view_root,
            candidates,
            archive_manifest,
            audit_root,
            deletion_performed: false,
        })
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RestoreDrillFailure {
    ShadowParityMissing,
    CheckpointMissingOrChanged,
    ArchiveMissingOrChanged,
    CustodyMissingOrChanged,
    AnchorMissingOrChanged,
    RebuildRootMismatch,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RestoreDrillReport {
    checkpoint_cid: CheckpointCid,
    shadow_audit_root: [u8; 32],
    archive_manifest_cid: ManifestCid,
    restored_view_root: [u8; 32],
    verified_record_ids: Vec<PayloadRecordId>,
    custody_receipt: Option<ManifestCid>,
    failure: Option<RestoreDrillFailure>,
    report_root: [u8; 32],
}

impl RestoreDrillReport {
    pub const fn passed(&self) -> bool {
        self.failure.is_none()
    }

    pub const fn must_retain_payloads(&self) -> bool {
        !self.passed()
    }

    pub const fn checkpoint_cid(&self) -> CheckpointCid {
        self.checkpoint_cid
    }

    pub const fn restored_view_root(&self) -> [u8; 32] {
        self.restored_view_root
    }

    pub const fn shadow_audit_root(&self) -> [u8; 32] {
        self.shadow_audit_root
    }

    pub const fn archive_manifest_cid(&self) -> ManifestCid {
        self.archive_manifest_cid
    }

    pub fn verified_record_ids(&self) -> &[PayloadRecordId] {
        &self.verified_record_ids
    }

    pub const fn custody_receipt(&self) -> Option<ManifestCid> {
        self.custody_receipt
    }

    pub const fn failure(&self) -> Option<RestoreDrillFailure> {
        self.failure
    }

    pub const fn report_root(&self) -> [u8; 32] {
        self.report_root
    }
}

pub struct RestoreDrill;

/// Runtime-owned deterministic rebuild adapter. Implementations must replay
/// the exact checkpoint state plus archive and all retained/later events; the
/// drill never accepts a caller-asserted root directly.
pub trait CheckpointRestoreRebuilder {
    fn rebuild_from_checkpoint_archive_and_later_events(
        &self,
        checkpoint: &ValidatedFeedCheckpoint,
        archive: &ArchiveManifest,
        anchors: &ExactHighWaterAnchors,
    ) -> Option<[u8; 32]>;
}

impl RestoreDrill {
    pub fn run(
        plan: &ShadowCompactionPlan,
        checkpoint: Option<&ValidatedFeedCheckpoint>,
        archive: Option<&ArchiveManifest>,
        custody: Option<&ValidatedCustodyReceipt>,
        anchors: Option<&ExactHighWaterAnchors>,
        rebuilder: Option<&dyn CheckpointRestoreRebuilder>,
    ) -> RestoreDrillReport {
        let mut verified = Vec::new();
        let mut receipt_id = None;
        let mut restored_view_root = [0; 32];
        let failure = if !shadow_plan_integrity(plan) {
            Some(RestoreDrillFailure::ShadowParityMissing)
        } else if checkpoint.is_none_or(|value| value.checkpoint_cid != plan.checkpoint_cid) {
            Some(RestoreDrillFailure::CheckpointMissingOrChanged)
        } else if archive.is_none_or(|manifest| {
            !manifest.is_self_consistent()
                || manifest.manifest_cid != plan.archive_manifest.manifest_cid
                || plan
                    .candidates
                    .iter()
                    .any(|descriptor| !manifest.contains_exact(descriptor))
        }) {
            Some(RestoreDrillFailure::ArchiveMissingOrChanged)
        } else if anchors
            .and_then(|value| value.root().ok())
            .is_none_or(|root| root != plan.anchors_root)
        {
            Some(RestoreDrillFailure::AnchorMissingOrChanged)
        } else if custody.is_none_or(|receipt| {
            receipt.body.archive_manifest != plan.archive_manifest.manifest_cid
                || receipt.body.entries_root != plan.archive_manifest.entries_root
                || receipt.body.anchors_root != plan.anchors_root
        }) {
            Some(RestoreDrillFailure::CustodyMissingOrChanged)
        } else {
            let rebuilt = checkpoint
                .zip(archive)
                .zip(anchors)
                .zip(rebuilder)
                .and_then(|(((checkpoint, archive), anchors), rebuilder)| {
                    rebuilder.rebuild_from_checkpoint_archive_and_later_events(
                        checkpoint, archive, anchors,
                    )
                });
            match rebuilt {
                Some(root) if root == plan.live_view_root => {
                    restored_view_root = root;
                    verified.extend(plan.candidates.iter().map(|descriptor| descriptor.id));
                    receipt_id = custody.map(|receipt| receipt.receipt_cid);
                    None
                }
                Some(root) => {
                    restored_view_root = root;
                    Some(RestoreDrillFailure::RebuildRootMismatch)
                }
                None => Some(RestoreDrillFailure::RebuildRootMismatch),
            }
        };
        let report_root = restore_report_root(
            plan.checkpoint_cid,
            plan.audit_root,
            plan.archive_manifest.manifest_cid,
            restored_view_root,
            &verified,
            receipt_id,
            failure,
        )
        .unwrap_or([0; 32]);
        RestoreDrillReport {
            checkpoint_cid: plan.checkpoint_cid,
            shadow_audit_root: plan.audit_root,
            archive_manifest_cid: plan.archive_manifest.manifest_cid,
            restored_view_root,
            verified_record_ids: verified,
            custody_receipt: receipt_id,
            failure,
            report_root,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RetentionAction {
    Keep,
    EvictAfterCheckpointArchive,
    EvictRebuildableCache,
    UserAuthorizedPrivateDelete,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct LocalRetentionPolicy {
    rules: BTreeMap<PayloadClass, RetentionAction>,
}

impl Default for LocalRetentionPolicy {
    fn default() -> Self {
        Self {
            rules: BTreeMap::from([
                (PayloadClass::CanonicalEvent, RetentionAction::Keep),
                (PayloadClass::CanonicalObject, RetentionAction::Keep),
                (PayloadClass::PrivateSource, RetentionAction::Keep),
                (
                    PayloadClass::DerivedCache,
                    RetentionAction::EvictRebuildableCache,
                ),
                (PayloadClass::AuthorityAnchor, RetentionAction::Keep),
                (PayloadClass::CheckpointAnchor, RetentionAction::Keep),
                (PayloadClass::QuarantineEvidence, RetentionAction::Keep),
            ]),
        }
    }
}

impl LocalRetentionPolicy {
    pub fn set(&mut self, class: PayloadClass, action: RetentionAction) {
        self.rules.insert(class, action);
    }

    pub fn action(&self, class: PayloadClass) -> RetentionAction {
        self.rules
            .get(&class)
            .copied()
            .unwrap_or(RetentionAction::Keep)
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct LocalGcGate {
    pub operator_enabled: bool,
    pub shadow_soak_passed: bool,
    pub recovery_path: String,
    pub private_delete_consents: BTreeSet<PayloadRecordId>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum GcBlockReason {
    OperatorKillSwitch,
    ShadowSoakIncomplete,
    RestoreDrillFailed,
    RecoveryPathMissing,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ApprovedLocalEviction {
    checkpoint_cid: CheckpointCid,
    record_ids: Vec<PayloadRecordId>,
    recovery_path: String,
    audit_root: [u8; 32],
}

impl ApprovedLocalEviction {
    pub const fn checkpoint_cid(&self) -> CheckpointCid {
        self.checkpoint_cid
    }

    pub fn record_ids(&self) -> &[PayloadRecordId] {
        &self.record_ids
    }

    pub fn recovery_path(&self) -> &str {
        &self.recovery_path
    }

    pub const fn audit_root(&self) -> [u8; 32] {
        self.audit_root
    }
}

pub struct LocalEvictionCoordinator;

impl LocalEvictionCoordinator {
    pub fn approve(
        plan: &ShadowCompactionPlan,
        drill: &RestoreDrillReport,
        policy: &LocalRetentionPolicy,
        gate: &LocalGcGate,
    ) -> Result<ApprovedLocalEviction, GcBlockReason> {
        if !gate.operator_enabled {
            return Err(GcBlockReason::OperatorKillSwitch);
        }
        if !gate.shadow_soak_passed {
            return Err(GcBlockReason::ShadowSoakIncomplete);
        }
        if !shadow_plan_integrity(plan)
            || !drill.passed()
            || drill.checkpoint_cid != plan.checkpoint_cid
            || drill.shadow_audit_root != plan.audit_root
            || drill.archive_manifest_cid != plan.archive_manifest.manifest_cid
        {
            return Err(GcBlockReason::RestoreDrillFailed);
        }
        if gate.recovery_path.trim().is_empty() {
            return Err(GcBlockReason::RecoveryPathMissing);
        }
        let mut record_ids = Vec::new();
        for descriptor in &plan.candidates {
            let approved = match policy.action(descriptor.class) {
                RetentionAction::Keep => false,
                RetentionAction::EvictAfterCheckpointArchive => true,
                RetentionAction::EvictRebuildableCache => {
                    descriptor.class == PayloadClass::DerivedCache
                }
                RetentionAction::UserAuthorizedPrivateDelete => {
                    descriptor.class == PayloadClass::PrivateSource
                        && gate.private_delete_consents.contains(&descriptor.id)
                }
            };
            if approved {
                record_ids.push(descriptor.id);
            }
        }
        record_ids.sort();
        let audit_root = eviction_audit_root(
            plan.checkpoint_cid,
            plan.audit_root,
            drill.report_root,
            &record_ids,
            &gate.recovery_path,
        )
        .map_err(|_| GcBlockReason::RecoveryPathMissing)?;
        Ok(ApprovedLocalEviction {
            checkpoint_cid: plan.checkpoint_cid,
            record_ids,
            recovery_path: gate.recovery_path.clone(),
            audit_root,
        })
    }
}

pub trait LocalPayloadBackend {
    type Error;

    fn persist_eviction_audit(
        &mut self,
        approval: &ApprovedLocalEviction,
    ) -> Result<(), Self::Error>;
    fn delete_local_payload(&mut self, record: PayloadRecordId) -> Result<(), Self::Error>;
}

pub fn execute_local_eviction<B: LocalPayloadBackend>(
    backend: &mut B,
    approval: &ApprovedLocalEviction,
) -> Result<usize, B::Error> {
    backend.persist_eviction_audit(approval)?;
    for record in &approval.record_ids {
        backend.delete_local_payload(*record)?;
    }
    Ok(approval.record_ids.len())
}

fn shadow_plan_integrity(plan: &ShadowCompactionPlan) -> bool {
    plan.is_dry_run()
        && plan.live_view_root != [0; 32]
        && plan.live_view_root == plan.rebuilt_view_root
        && plan.archive_manifest.is_self_consistent()
        && plan.archive_manifest.checkpoint_cid == plan.checkpoint_cid
        && plan.archive_manifest.anchors_root == plan.anchors_root
        && plan
            .candidates
            .iter()
            .all(|descriptor| plan.archive_manifest.contains_exact(descriptor))
        && shadow_audit_root(
            plan.checkpoint_cid,
            plan.anchors_root,
            plan.live_view_root,
            &plan.candidates,
        )
        .is_ok_and(|root| root == plan.audit_root)
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum CompactionError {
    Canonical(CanonicalError),
    InvalidAnchor,
    InvalidPayload,
    InvalidArchive,
    InvalidCustody,
    ResourceLimit,
}

impl CompactionError {
    pub const fn code(&self) -> &'static str {
        match self {
            Self::Canonical(_) => "COMPACTION_CANONICAL",
            Self::InvalidAnchor => "COMPACTION_ANCHOR",
            Self::InvalidPayload => "COMPACTION_PAYLOAD",
            Self::InvalidArchive => "COMPACTION_ARCHIVE",
            Self::InvalidCustody => "COMPACTION_CUSTODY",
            Self::ResourceLimit => "COMPACTION_RESOURCE_LIMIT",
        }
    }
}

impl From<CanonicalError> for CompactionError {
    fn from(value: CanonicalError) -> Self {
        Self::Canonical(value)
    }
}

impl fmt::Display for CompactionError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(self.code())
    }
}

impl std::error::Error for CompactionError {}

fn payload_value(descriptor: &PayloadDescriptor) -> CanonicalValue {
    CanonicalValue::Map(vec![
        (0, CanonicalValue::Unsigned(descriptor.id.record_kind)),
        (1, CanonicalValue::Bytes(descriptor.id.cid.to_vec())),
        (2, CanonicalValue::Unsigned(descriptor.class as u64)),
        (3, CanonicalValue::Unsigned(descriptor.byte_len)),
        (4, CanonicalValue::Bytes(descriptor.bytes_digest.to_vec())),
    ])
}

fn custody_body_bytes(body: &CustodyReceiptBody) -> Result<Vec<u8>, CompactionError> {
    if body.entries_root == [0; 32] || body.anchors_root == [0; 32] || body.nonce == [0; 32] {
        return Err(CompactionError::InvalidCustody);
    }
    Ok(encode_canonical(
        &CanonicalValue::Map(vec![
            (0, CanonicalValue::Unsigned(COMPACTION_PROFILE_MAJOR)),
            (
                1,
                CanonicalValue::Bytes(body.archive_manifest.as_bytes().to_vec()),
            ),
            (2, CanonicalValue::Bytes(body.entries_root.to_vec())),
            (3, CanonicalValue::Bytes(body.anchors_root.to_vec())),
            (
                4,
                CanonicalValue::Bytes(body.custodian_feed.as_bytes().to_vec()),
            ),
            (
                5,
                CanonicalValue::Bytes(body.key_state_frontier.as_bytes().to_vec()),
            ),
            (6, CanonicalValue::Bytes(body.nonce.to_vec())),
        ]),
        ResourceProfile::ControlV1,
    )?)
}

fn shadow_audit_root(
    checkpoint: CheckpointCid,
    anchors_root: [u8; 32],
    live_view_root: [u8; 32],
    candidates: &[PayloadDescriptor],
) -> Result<[u8; 32], CompactionError> {
    digest_value(
        b"shadow-compaction-audit/1",
        &CanonicalValue::Map(vec![
            (0, CanonicalValue::Bytes(checkpoint.as_bytes().to_vec())),
            (1, CanonicalValue::Bytes(anchors_root.to_vec())),
            (2, CanonicalValue::Bytes(live_view_root.to_vec())),
            (
                3,
                CanonicalValue::Array(candidates.iter().map(payload_value).collect()),
            ),
            (4, CanonicalValue::Bool(false)),
        ]),
    )
}

fn restore_report_root(
    checkpoint: CheckpointCid,
    shadow_audit_root: [u8; 32],
    archive_manifest_cid: ManifestCid,
    restored_view_root: [u8; 32],
    verified: &[PayloadRecordId],
    receipt: Option<ManifestCid>,
    failure: Option<RestoreDrillFailure>,
) -> Result<[u8; 32], CompactionError> {
    digest_value(
        b"restore-drill-report/1",
        &CanonicalValue::Map(vec![
            (0, CanonicalValue::Bytes(checkpoint.as_bytes().to_vec())),
            (1, CanonicalValue::Bytes(shadow_audit_root.to_vec())),
            (
                2,
                CanonicalValue::Bytes(archive_manifest_cid.as_bytes().to_vec()),
            ),
            (3, CanonicalValue::Bytes(restored_view_root.to_vec())),
            (
                4,
                CanonicalValue::Array(
                    verified
                        .iter()
                        .map(|record| {
                            CanonicalValue::Map(vec![
                                (0, CanonicalValue::Unsigned(record.record_kind)),
                                (1, CanonicalValue::Bytes(record.cid.to_vec())),
                            ])
                        })
                        .collect(),
                ),
            ),
            (
                5,
                receipt
                    .map(|value| CanonicalValue::Bytes(value.as_bytes().to_vec()))
                    .unwrap_or(CanonicalValue::Null),
            ),
            (
                6,
                failure
                    .map(|value| CanonicalValue::Unsigned(value as u64))
                    .unwrap_or(CanonicalValue::Null),
            ),
        ]),
    )
}

fn eviction_audit_root(
    checkpoint: CheckpointCid,
    shadow_root: [u8; 32],
    restore_root: [u8; 32],
    record_ids: &[PayloadRecordId],
    recovery_path: &str,
) -> Result<[u8; 32], CompactionError> {
    digest_value(
        b"local-eviction-audit/1",
        &CanonicalValue::Map(vec![
            (0, CanonicalValue::Bytes(checkpoint.as_bytes().to_vec())),
            (1, CanonicalValue::Bytes(shadow_root.to_vec())),
            (2, CanonicalValue::Bytes(restore_root.to_vec())),
            (
                3,
                CanonicalValue::Array(
                    record_ids
                        .iter()
                        .map(|record| {
                            CanonicalValue::Map(vec![
                                (0, CanonicalValue::Unsigned(record.record_kind)),
                                (1, CanonicalValue::Bytes(record.cid.to_vec())),
                            ])
                        })
                        .collect(),
                ),
            ),
            (4, CanonicalValue::Text(recovery_path.to_owned())),
        ]),
    )
}

fn digest_value(domain: &[u8], value: &CanonicalValue) -> Result<[u8; 32], CompactionError> {
    let bytes = encode_canonical(value, ResourceProfile::ManifestV1)?;
    let mut hasher = blake3::Hasher::new();
    hasher.update(b"onebrain:vnext:");
    hasher.update(domain);
    hasher.update(&[0]);
    hasher.update(&bytes);
    Ok(*hasher.finalize().as_bytes())
}

#[cfg(test)]
mod tests {
    use ed25519_dalek::SigningKey;

    use super::*;
    use crate::foundation::{
        assess_checkpoint_suppression, decode_feed_checkpoint, decode_feed_inception,
        decode_knowledge_event, ActorId, CheckpointEffectVerifier, CheckpointHistoryWitness,
        CheckpointLeaf, DelegationGrant, DeviceId, DisclosureClass, EventCid, EventType,
        FeedCheckpointBody, FeedInception, KeyStateReducer, KnowledgeEventEnvelope,
        NamespaceCommitment, ScopedDelegation, SignedFeedCheckpoint,
    };

    const EVENT: EventType = EventType(1);
    const REDUCER: [u8; 32] = [61; 32];

    struct Effects;

    impl CheckpointEffectVerifier for Effects {
        fn verify_effect(&self, reducer_version: [u8; 32], leaf: &CheckpointLeaf) -> bool {
            reducer_version == REDUCER && leaf.effect_root() == [leaf.sequence() as u8 + 50; 32]
        }
    }

    struct Fixture {
        key: SigningKey,
        feed: ValidatedFeedInception,
        state: KeyStateReducer,
        witness: CheckpointHistoryWitness,
        anchors: ExactHighWaterAnchors,
        checkpoint: ValidatedFeedCheckpoint,
    }

    fn fixture() -> Fixture {
        let key = SigningKey::from_bytes(&[31; 32]);
        let device = DeviceId::from_bytes([32; 32]);
        let delegation = EventCid::from_bytes([33; 32]);
        let mut inception = FeedInception::new(
            *key.verifying_key().as_bytes(),
            NamespaceCommitment::derive(b"compaction", [34; 32]).unwrap(),
            0,
            device,
        );
        inception.actor_delegation_ref = Some(*delegation.as_bytes());
        let feed = decode_feed_inception(&inception.sign(&key).unwrap().encode().unwrap()).unwrap();
        let mut state = KeyStateReducer::new(EventCid::from_bytes([35; 32]));
        state.accept_root(ScopedDelegation {
            grant: DelegationGrant {
                actor: ActorId::from_bytes([36; 32]),
                device,
                delegation_ref: delegation,
                namespace_commitment: None,
                first_generation: 0,
                last_generation: 5,
                proof: EventCid::from_bytes([37; 32]),
            },
            parent_delegation_ref: None,
        });
        let mut leaves = Vec::new();
        let mut parent = None;
        for sequence in 0..2u64 {
            let mut envelope = KnowledgeEventEnvelope::new(
                EVENT,
                feed.feed_id,
                sequence,
                DisclosureClass::Public,
                [sequence as u8 + 40; 32],
            );
            envelope.causal_parents = parent.into_iter().collect();
            let (bytes, _) = envelope.sign(&feed, &key).unwrap().encode().unwrap();
            let event = decode_knowledge_event(&bytes, &feed, &[EVENT]).unwrap();
            parent = Some(event.cid());
            leaves.push(
                CheckpointLeaf::from_validated_event(
                    &event,
                    ManifestCid::from_bytes([sequence as u8 + 70; 32]),
                    ManifestCid::from_bytes([sequence as u8 + 71; 32]),
                    [sequence as u8 + 50; 32],
                )
                .unwrap(),
            );
        }
        let witness = CheckpointHistoryWitness::new(leaves).unwrap();
        let mut anchors = ExactHighWaterAnchors::default();
        anchors
            .observe(
                HighWaterLane::ProviderLeaseGeneration,
                [80; 32],
                4,
                [81; 32],
            )
            .unwrap();
        anchors
            .observe(
                HighWaterLane::ProviderRetirementFloor,
                [80; 32],
                3,
                [82; 32],
            )
            .unwrap();
        anchors
            .observe(HighWaterLane::PermitGeneration, [83; 32], 2, [84; 32])
            .unwrap();
        anchors
            .observe(HighWaterLane::KeyGeneration, [85; 32], 1, [86; 32])
            .unwrap();
        let body = FeedCheckpointBody::from_witness(
            &witness,
            REDUCER,
            None,
            anchors.root().unwrap(),
            &state.checkpoint_proof(&feed),
            None,
            [87; 32],
        )
        .unwrap();
        let signed = SignedFeedCheckpoint::sign(body, &feed, &key).unwrap();
        let checkpoint = decode_feed_checkpoint(&signed.encode().unwrap(), &feed).unwrap();
        Fixture {
            key,
            feed,
            state,
            witness,
            anchors,
            checkpoint,
        }
    }

    fn payload(fixture: &Fixture, sequence: u64, class: PayloadClass) -> ProofedPayload {
        let proof = fixture.witness.inclusion_proof(sequence).unwrap();
        ProofedPayload {
            descriptor: PayloadDescriptor {
                id: PayloadRecordId {
                    record_kind: class as u64 + 1,
                    cid: if class == PayloadClass::CanonicalEvent {
                        *proof.leaf.event_cid().as_bytes()
                    } else {
                        [sequence as u8 + 100; 32]
                    },
                },
                class,
                byte_len: 128,
                bytes_digest: [sequence as u8 + 110; 32],
            },
            covering_event: Some(*proof.leaf.event_cid().as_bytes()),
            inclusion_proof: Some(proof),
        }
    }

    fn shadow(fixture: &Fixture) -> ShadowCompactionPlan {
        let suppression = assess_checkpoint_suppression(
            &fixture.checkpoint,
            &fixture.state.checkpoint_proof(&fixture.feed),
            Some(&fixture.witness),
            Some(&Effects),
        );
        let suppression = suppression.authority().unwrap();
        ShadowCompactionPlanner::plan(
            &fixture.checkpoint,
            suppression,
            &fixture.anchors,
            vec![
                payload(fixture, 0, PayloadClass::CanonicalEvent),
                payload(fixture, 1, PayloadClass::CanonicalObject),
            ],
            [120; 32],
            [120; 32],
            true,
        )
        .unwrap()
    }

    struct StaticRebuilder([u8; 32]);

    impl CheckpointRestoreRebuilder for StaticRebuilder {
        fn rebuild_from_checkpoint_archive_and_later_events(
            &self,
            _checkpoint: &ValidatedFeedCheckpoint,
            _archive: &ArchiveManifest,
            _anchors: &ExactHighWaterAnchors,
        ) -> Option<[u8; 32]> {
            Some(self.0)
        }
    }

    #[test]
    fn qa006_high_water_merge_is_commutative_associative_idempotent_and_monotonic() {
        let mut left = ExactHighWaterAnchors::default();
        left.observe(HighWaterLane::PermitGeneration, [1; 32], 5, [2; 32])
            .unwrap();
        let mut right = ExactHighWaterAnchors::default();
        right
            .observe(HighWaterLane::PermitGeneration, [1; 32], 5, [3; 32])
            .unwrap();
        right
            .observe(HighWaterLane::PermitGeneration, [1; 32], 4, [4; 32])
            .unwrap();
        let mut third = ExactHighWaterAnchors::default();
        third
            .observe(HighWaterLane::ProviderRetirementFloor, [5; 32], 8, [6; 32])
            .unwrap();

        let mut a = left.clone();
        a.merge(&right).unwrap();
        let mut b = right.clone();
        b.merge(&left).unwrap();
        assert_eq!(a.root().unwrap(), b.root().unwrap());

        let mut left_associated = a.clone();
        left_associated.merge(&third).unwrap();
        let mut right_third = right;
        right_third.merge(&third).unwrap();
        let mut right_associated = left.clone();
        right_associated.merge(&right_third).unwrap();
        assert_eq!(
            left_associated.root().unwrap(),
            right_associated.root().unwrap()
        );
        let stable_root = left_associated.root().unwrap();
        let replay = left_associated.clone();
        left_associated.merge(&replay).unwrap();
        assert_eq!(left_associated.root().unwrap(), stable_root);

        let entry = a
            .entry(HighWaterLane::PermitGeneration, [1; 32])
            .unwrap()
            .clone();
        assert_eq!(entry.high_water, 5);
        assert_eq!(entry.record_ids, BTreeSet::from([[2; 32], [3; 32]]));
        assert_eq!(
            a.observe(HighWaterLane::PermitGeneration, [1; 32], 1, [9; 32])
                .unwrap(),
            HighWaterObserveOutcome::BelowHighWaterInactive
        );
        assert_eq!(
            a.entry(HighWaterLane::PermitGeneration, [1; 32]).unwrap(),
            &entry
        );
    }

    #[test]
    fn shadow_plan_has_full_manifest_but_never_deletes() {
        let fixture = fixture();
        let plan = shadow(&fixture);
        assert!(plan.is_dry_run());
        assert!(!plan.deletion_performed);
        assert_eq!(plan.candidates.len(), 2);
        assert_eq!(plan.archive_manifest.entries.len(), 2);
    }

    #[test]
    fn missing_proof_anchor_or_parity_fails_before_candidate_creation() {
        let fixture = fixture();
        let suppression = assess_checkpoint_suppression(
            &fixture.checkpoint,
            &fixture.state.checkpoint_proof(&fixture.feed),
            Some(&fixture.witness),
            Some(&Effects),
        );
        let suppression = suppression.authority().unwrap();
        let mut uncovered = payload(&fixture, 0, PayloadClass::CanonicalEvent);
        uncovered.inclusion_proof = None;
        assert_eq!(
            ShadowCompactionPlanner::plan(
                &fixture.checkpoint,
                suppression,
                &fixture.anchors,
                vec![uncovered],
                [1; 32],
                [1; 32],
                true,
            ),
            Err(ShadowBlockReason::MissingCoverageProof)
        );
        assert_eq!(
            ShadowCompactionPlanner::plan(
                &fixture.checkpoint,
                suppression,
                &fixture.anchors,
                vec![],
                [1; 32],
                [2; 32],
                true,
            ),
            Err(ShadowBlockReason::ViewParityMismatch)
        );
    }

    #[test]
    fn restore_drill_requires_archive_custody_anchors_and_exact_view_root() {
        let fixture = fixture();
        let plan = shadow(&fixture);
        let mut forged_plan = plan.clone();
        forged_plan.audit_root[0] ^= 1;
        let forged = RestoreDrill::run(
            &forged_plan,
            Some(&fixture.checkpoint),
            Some(&forged_plan.archive_manifest),
            None,
            Some(&fixture.anchors),
            Some(&StaticRebuilder([120; 32])),
        );
        assert_eq!(
            forged.failure(),
            Some(RestoreDrillFailure::ShadowParityMissing)
        );
        let missing = RestoreDrill::run(&plan, Some(&fixture.checkpoint), None, None, None, None);
        assert!(missing.must_retain_payloads());
        assert_eq!(
            missing.failure,
            Some(RestoreDrillFailure::ArchiveMissingOrChanged)
        );

        let key_proof = fixture.state.checkpoint_proof(&fixture.feed);
        let mut wrong_subject = key_proof;
        wrong_subject.subject_feed = FeedId::from_bytes([123; 32]);
        assert_eq!(
            SignedCustodyReceipt::sign(
                &plan.archive_manifest,
                &fixture.feed,
                &wrong_subject,
                [121; 32],
                &fixture.key,
            )
            .unwrap_err(),
            CompactionError::InvalidCustody
        );
        let signed_receipt = SignedCustodyReceipt::sign(
            &plan.archive_manifest,
            &fixture.feed,
            &key_proof,
            [121; 32],
            &fixture.key,
        )
        .unwrap();
        let receipt = validate_custody_receipt(&signed_receipt, &fixture.feed, &key_proof).unwrap();
        let wrong_rebuild = RestoreDrill::run(
            &plan,
            Some(&fixture.checkpoint),
            Some(&plan.archive_manifest),
            Some(&receipt),
            Some(&fixture.anchors),
            Some(&StaticRebuilder([119; 32])),
        );
        assert_eq!(
            wrong_rebuild.failure(),
            Some(RestoreDrillFailure::RebuildRootMismatch)
        );
        let passed = RestoreDrill::run(
            &plan,
            Some(&fixture.checkpoint),
            Some(&plan.archive_manifest),
            Some(&receipt),
            Some(&fixture.anchors),
            Some(&StaticRebuilder([120; 32])),
        );
        assert!(passed.passed());
        assert_eq!(passed.verified_record_ids.len(), 2);
    }

    #[derive(Default)]
    struct MemoryBackend {
        audit_persisted: bool,
        deleted: Vec<PayloadRecordId>,
    }

    impl LocalPayloadBackend for MemoryBackend {
        type Error = ();

        fn persist_eviction_audit(
            &mut self,
            _approval: &ApprovedLocalEviction,
        ) -> Result<(), Self::Error> {
            self.audit_persisted = true;
            Ok(())
        }

        fn delete_local_payload(&mut self, record: PayloadRecordId) -> Result<(), Self::Error> {
            assert!(self.audit_persisted);
            self.deleted.push(record);
            Ok(())
        }
    }

    #[test]
    fn local_gc_needs_restore_soak_policy_kill_switch_and_audit_first() {
        let fixture = fixture();
        let plan = shadow(&fixture);
        let key_proof = fixture.state.checkpoint_proof(&fixture.feed);
        let receipt = validate_custody_receipt(
            &SignedCustodyReceipt::sign(
                &plan.archive_manifest,
                &fixture.feed,
                &key_proof,
                [122; 32],
                &fixture.key,
            )
            .unwrap(),
            &fixture.feed,
            &key_proof,
        )
        .unwrap();
        let drill = RestoreDrill::run(
            &plan,
            Some(&fixture.checkpoint),
            Some(&plan.archive_manifest),
            Some(&receipt),
            Some(&fixture.anchors),
            Some(&StaticRebuilder([120; 32])),
        );
        let mut policy = LocalRetentionPolicy::default();
        policy.set(
            PayloadClass::CanonicalEvent,
            RetentionAction::EvictAfterCheckpointArchive,
        );
        let disabled = LocalGcGate {
            operator_enabled: false,
            shadow_soak_passed: true,
            recovery_path: "archive://local/m6".into(),
            private_delete_consents: BTreeSet::new(),
        };
        assert_eq!(
            LocalEvictionCoordinator::approve(&plan, &drill, &policy, &disabled),
            Err(GcBlockReason::OperatorKillSwitch)
        );
        let enabled = LocalGcGate {
            operator_enabled: true,
            ..disabled
        };
        let suppression = assess_checkpoint_suppression(
            &fixture.checkpoint,
            &fixture.state.checkpoint_proof(&fixture.feed),
            Some(&fixture.witness),
            Some(&Effects),
        );
        let suppression = suppression.authority().unwrap();
        let different_plan = ShadowCompactionPlanner::plan(
            &fixture.checkpoint,
            suppression,
            &fixture.anchors,
            vec![payload(&fixture, 0, PayloadClass::CanonicalEvent)],
            [120; 32],
            [120; 32],
            true,
        )
        .unwrap();
        assert_eq!(
            LocalEvictionCoordinator::approve(&different_plan, &drill, &policy, &enabled),
            Err(GcBlockReason::RestoreDrillFailed)
        );
        let approval = LocalEvictionCoordinator::approve(&plan, &drill, &policy, &enabled).unwrap();
        assert_eq!(approval.record_ids.len(), 1);
        let mut backend = MemoryBackend::default();
        assert_eq!(execute_local_eviction(&mut backend, &approval).unwrap(), 1);
        assert!(backend.audit_persisted);
        assert_eq!(backend.deleted, approval.record_ids);
    }
}

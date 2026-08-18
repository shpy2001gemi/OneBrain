//! Bounded, multi-source relay discovery.
//!
//! Transport sources only carry bytes. A relay becomes visible only after the
//! Task 3 identity/signature/DNS checks and live proof-of-possession complete.

use std::collections::{BTreeMap, BTreeSet};
use std::future::Future;
use std::pin::Pin;
use std::sync::{Arc, Mutex};
use std::time::Instant;

use ed25519_dalek::{Signature, Verifier, VerifyingKey};
use ku_core::foundation::NodeId;
use onebrain_protocol::{
    decode_reachability_object, reachability_signing_bytes, ReachabilityAdvertisementV1,
    ReachabilityObjectV1, ReachabilitySignatureRoleV1, RelayPossessionChallengeV1,
    RelayPossessionProofV1,
};
use rand::rngs::OsRng;
use rand::RngCore;

use crate::vnext_reachability_crypto::{
    KnownPeerIdentity, PendingRelayDescriptorAdmission, PreparedRelayDescriptorAdmission,
    ReachabilityAdmission, ReachabilityAdmissionPreparer, ReachabilityDialValidator,
    ReachabilityLockFreeDialValidation, ReachabilityLockFreePreparation,
    ReachabilityRecordAdmission, RelayAdmissionError, ValidatedPossessionDialEndpoint,
    ValidatedRelayDescriptor,
};
use crate::vnext_session::principal_node_id;

pub const MANUAL_RELAY_PREFIX: &str = "onebrain://relay/v1/";
pub const MANUAL_PEER_PREFIX: &str = "onebrain://peer/v1/";

pub type ReachabilityFuture<'a, T> = Pin<Box<dyn Future<Output = T> + Send + 'a>>;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SourceBudget {
    pub max_records: usize,
    pub max_bytes: usize,
    pub max_signature_checks: usize,
    pub deadline: Instant,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct SourceBudgetState {
    pub records: usize,
    pub bytes: usize,
    pub signature_checks: usize,
    pub exhausted: bool,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum RelayDiscoveryLimitation {
    SourceKeyLimit,
    RecordLimit,
    ByteLimit,
    SignatureLimit,
    Deadline,
    NoBootstrapReachable,
    PoisonedSource,
    SessionNotLive,
    StateUnavailable,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct RelayDiscoveryDelta {
    pub admitted: Vec<NodeId>,
    pub refreshed: Vec<NodeId>,
    pub rejected: usize,
    pub limitations: Vec<RelayDiscoveryLimitation>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RelayDiscoveryPolicy {
    pub max_source_keys: usize,
    pub max_records_per_source: usize,
    pub max_total_records: usize,
    pub max_bytes_per_source: usize,
    pub max_signature_checks: usize,
    pub max_probe_concurrency: usize,
}

impl Default for RelayDiscoveryPolicy {
    fn default() -> Self {
        Self {
            max_source_keys: 8,
            max_records_per_source: 64,
            max_total_records: 256,
            max_bytes_per_source: 1_048_576,
            max_signature_checks: 64,
            max_probe_concurrency: 4,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub struct RelayDiscoverySourceId(pub [u8; 32]);

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct VerifiedAuthenticatedSessionSource {
    peer: NodeId,
    session_id: [u8; 32],
    transport_binding_digest: [u8; 32],
    handshake_digest: [u8; 32],
}

impl VerifiedAuthenticatedSessionSource {
    #[allow(dead_code)]
    pub(crate) fn from_verified_route(
        peer: NodeId,
        session_id: [u8; 32],
        transport_binding_digest: [u8; 32],
        handshake_digest: [u8; 32],
    ) -> Self {
        Self {
            peer,
            session_id,
            transport_binding_digest,
            handshake_digest,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct LiveSessionLease {
    lease_id: [u8; 32],
    peer: NodeId,
    session_id: [u8; 32],
    transport_binding_digest: [u8; 32],
}

impl LiveSessionLease {
    pub fn authenticated_pex(self) -> AuthenticatedPexSource {
        AuthenticatedPexSource { lease: self }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AuthenticatedPexSource {
    lease: LiveSessionLease,
}

pub trait AuthenticatedSessionRegistry: Send + Sync {
    fn register(
        &self,
        source: &VerifiedAuthenticatedSessionSource,
    ) -> Result<LiveSessionLease, RelayDiscoveryLimitation>;
    fn is_live(&self, lease: &LiveSessionLease, now: u64) -> bool;
    fn revoke(&self, lease: LiveSessionLease);
}

#[derive(Default)]
pub struct InMemoryAuthenticatedSessionRegistry {
    leases: Mutex<BTreeMap<[u8; 32], VerifiedAuthenticatedSessionSource>>,
}

impl AuthenticatedSessionRegistry for InMemoryAuthenticatedSessionRegistry {
    fn register(
        &self,
        source: &VerifiedAuthenticatedSessionSource,
    ) -> Result<LiveSessionLease, RelayDiscoveryLimitation> {
        let mut nonce = [0; 32];
        OsRng.fill_bytes(&mut nonce);
        let mut preimage = Vec::with_capacity(128);
        preimage.extend_from_slice(source.peer.as_bytes());
        preimage.extend_from_slice(&source.session_id);
        preimage.extend_from_slice(&source.transport_binding_digest);
        preimage.extend_from_slice(&source.handshake_digest);
        preimage.extend_from_slice(&nonce);
        let lease_id = *blake3::hash(&preimage).as_bytes();
        self.leases
            .lock()
            .map_err(|_| RelayDiscoveryLimitation::StateUnavailable)?
            .insert(lease_id, source.clone());
        Ok(LiveSessionLease {
            lease_id,
            peer: source.peer,
            session_id: source.session_id,
            transport_binding_digest: source.transport_binding_digest,
        })
    }

    fn is_live(&self, lease: &LiveSessionLease, _now: u64) -> bool {
        self.leases
            .lock()
            .ok()
            .and_then(|leases| leases.get(&lease.lease_id).cloned())
            .is_some_and(|source| {
                source.peer == lease.peer
                    && source.session_id == lease.session_id
                    && source.transport_binding_digest == lease.transport_binding_digest
            })
    }

    fn revoke(&self, lease: LiveSessionLease) {
        if let Ok(mut leases) = self.leases.lock() {
            leases.remove(&lease.lease_id);
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum RelayDiscoverySource {
    Rendezvous { relay: NodeId },
    AuthenticatedPeerExchange(AuthenticatedPexSource),
    BootstrapManifest { source_id: [u8; 32] },
    ManualRelayInvitation,
}

impl RelayDiscoverySource {
    pub fn rendezvous(relay: NodeId) -> Self {
        Self::Rendezvous { relay }
    }

    pub fn bootstrap_manifest(source_id: [u8; 32]) -> Self {
        Self::BootstrapManifest { source_id }
    }

    pub fn manual_relay() -> Self {
        Self::ManualRelayInvitation
    }

    pub fn authenticated_pex(source: AuthenticatedPexSource) -> Self {
        Self::AuthenticatedPeerExchange(source)
    }

    fn source_id(&self) -> RelayDiscoverySourceId {
        let mut preimage = Vec::with_capacity(65);
        match self {
            Self::Rendezvous { relay } => {
                preimage.push(1);
                preimage.extend_from_slice(relay.as_bytes());
            }
            Self::AuthenticatedPeerExchange(source) => {
                preimage.push(2);
                preimage.extend_from_slice(&source.lease.lease_id);
            }
            Self::BootstrapManifest { source_id } => {
                preimage.push(3);
                preimage.extend_from_slice(source_id);
            }
            Self::ManualRelayInvitation => preimage.push(4),
        }
        RelayDiscoverySourceId(*blake3::hash(&preimage).as_bytes())
    }

    fn live_lease(&self) -> Option<&LiveSessionLease> {
        match self {
            Self::AuthenticatedPeerExchange(source) => Some(&source.lease),
            _ => None,
        }
    }
}

#[derive(Debug, PartialEq, Eq)]
pub struct RelayPreparationPermit {
    source: RelayDiscoverySourceId,
    source_kind: RelayDiscoverySource,
    record_lengths: Vec<usize>,
    records: usize,
    bytes: usize,
    signature_checks: usize,
    dns_jobs: usize,
    permit_id: [u8; 32],
}

pub struct RelayDiscoveryPreparer {
    admission: Arc<ReachabilityAdmissionPreparer>,
    dial_validator: Arc<ReachabilityDialValidator>,
}

impl RelayDiscoveryPreparer {
    pub fn new(
        admission: Arc<ReachabilityAdmissionPreparer>,
        dial_validator: Arc<ReachabilityDialValidator>,
    ) -> Self {
        Self {
            admission,
            dial_validator,
        }
    }

    pub fn prepare_records<'a>(
        &'a self,
        permit: &'a RelayPreparationPermit,
        records: &'a [Vec<u8>],
        now: u64,
        deadline: Instant,
    ) -> ReachabilityFuture<
        'a,
        Result<Vec<PreparedRelayDescriptorAdmission>, RelayDiscoveryLimitation>,
    > {
        Box::pin(async move {
            if Instant::now() > deadline
                || records.len() != permit.records
                || records
                    .iter()
                    .zip(&permit.record_lengths)
                    .any(|(record, expected)| record.len() != *expected)
            {
                return Err(RelayDiscoveryLimitation::PoisonedSource);
            }
            let mut prepared = Vec::with_capacity(records.len());
            for record in records {
                let value = self
                    .admission
                    .prepare_descriptor(record, now, deadline)
                    .await
                    .map_err(map_admission_error)?;
                prepared.push(value);
            }
            Ok(prepared)
        })
    }

    pub fn prepare_possession<'a>(
        &'a self,
        mut staged: StagedRelayAdmission,
        deadline: Instant,
    ) -> ReachabilityFuture<'a, Result<StagedRelayAdmission, RelayDiscoveryLimitation>> {
        Box::pin(async move {
            let mut dials = Vec::with_capacity(staged.pending.challenges().len());
            for endpoint_index in 0..staged.pending.challenges().len() {
                dials.push(
                    self.dial_validator
                        .validate_possession_dial(&staged.pending, endpoint_index, deadline)
                        .await
                        .map_err(map_admission_error)?,
                );
            }
            staged.possession_dials = dials;
            Ok(staged)
        })
    }
}

pub struct StagedRelayAdmission {
    pending: PendingRelayDescriptorAdmission,
    source: RelayDiscoverySourceId,
    charged_records: usize,
    charged_bytes: usize,
    staged_at: u64,
    stage_id: [u8; 32],
    possession_dials: Vec<ValidatedPossessionDialEndpoint>,
}

impl StagedRelayAdmission {
    pub fn challenges(&self) -> &[RelayPossessionChallengeV1] {
        self.pending.challenges()
    }

    pub fn possession_dials(&self) -> &[ValidatedPossessionDialEndpoint] {
        &self.possession_dials
    }
}

pub trait RelayPossessionClient: Send + Sync {
    fn prove<'a>(
        &'a self,
        staged: &'a StagedRelayAdmission,
        deadline: Instant,
    ) -> ReachabilityFuture<'a, Result<Vec<RelayPossessionProofV1>, RelayDiscoveryLimitation>>;
}

#[derive(Clone, Debug)]
struct PermitLedger {
    source: RelayDiscoverySourceId,
    records: usize,
    bytes: usize,
    signature_checks: usize,
    dns_jobs: usize,
}

pub struct RelayDiscovery {
    policy: RelayDiscoveryPolicy,
    admission: ReachabilityAdmission,
    sessions: Arc<dyn AuthenticatedSessionRegistry>,
    sources: BTreeMap<RelayDiscoverySourceId, SourceBudgetState>,
    relays: BTreeMap<NodeId, ValidatedRelayDescriptor>,
    total_records: usize,
    signature_checks: usize,
    active_dns_jobs: usize,
    permits: BTreeMap<[u8; 32], PermitLedger>,
    staged: BTreeSet<[u8; 32]>,
    next_permit: u64,
}

impl RelayDiscovery {
    pub fn new(
        policy: RelayDiscoveryPolicy,
        admission: ReachabilityAdmission,
        sessions: Arc<dyn AuthenticatedSessionRegistry>,
    ) -> Self {
        Self {
            policy,
            admission,
            sessions,
            sources: BTreeMap::new(),
            relays: BTreeMap::new(),
            total_records: 0,
            signature_checks: 0,
            active_dns_jobs: 0,
            permits: BTreeMap::new(),
            staged: BTreeSet::new(),
            next_permit: 0,
        }
    }

    fn source_is_live(&self, source: &RelayDiscoverySource, now: u64) -> bool {
        source
            .live_lease()
            .is_none_or(|lease| self.sessions.is_live(lease, now))
    }
}

pub trait VerifiedRelayDiscovery {
    fn reserve_preparation(
        &mut self,
        source: RelayDiscoverySource,
        record_lengths: &[usize],
        now: u64,
    ) -> Result<RelayPreparationPermit, RelayDiscoveryLimitation>;
    fn stage_prepared(
        &mut self,
        permit: RelayPreparationPermit,
        prepared: Vec<PreparedRelayDescriptorAdmission>,
        now: u64,
    ) -> Result<Vec<StagedRelayAdmission>, RelayDiscoveryLimitation>;
    fn abort_preparation(
        &mut self,
        permit: RelayPreparationPermit,
        now: u64,
    ) -> Result<(), RelayDiscoveryLimitation>;
    fn commit_descriptor(
        &mut self,
        staged: StagedRelayAdmission,
        proofs: &[RelayPossessionProofV1],
        now: u64,
    ) -> Result<RelayDiscoveryDelta, RelayDiscoveryLimitation>;
    fn abort_descriptor(
        &mut self,
        staged: StagedRelayAdmission,
        now: u64,
    ) -> Result<(), RelayDiscoveryLimitation>;
    fn verified_relays(&self) -> impl Iterator<Item = &ValidatedRelayDescriptor>;
}

impl VerifiedRelayDiscovery for RelayDiscovery {
    fn reserve_preparation(
        &mut self,
        source: RelayDiscoverySource,
        record_lengths: &[usize],
        now: u64,
    ) -> Result<RelayPreparationPermit, RelayDiscoveryLimitation> {
        if record_lengths.is_empty() || !self.source_is_live(&source, now) {
            return Err(if record_lengths.is_empty() {
                RelayDiscoveryLimitation::RecordLimit
            } else {
                RelayDiscoveryLimitation::SessionNotLive
            });
        }
        let source_id = source.source_id();
        if !self.sources.contains_key(&source_id)
            && self.sources.len() >= self.policy.max_source_keys
        {
            return Err(RelayDiscoveryLimitation::SourceKeyLimit);
        }
        let records = record_lengths.len();
        let bytes = record_lengths.iter().try_fold(0_usize, |total, value| {
            total
                .checked_add(*value)
                .ok_or(RelayDiscoveryLimitation::ByteLimit)
        })?;
        let signature_checks = records;
        let current = self.sources.get(&source_id).cloned().unwrap_or_default();
        if records > self.policy.max_records_per_source
            || current.records.saturating_add(records) > self.policy.max_records_per_source
            || self.total_records.saturating_add(records) > self.policy.max_total_records
        {
            return Err(RelayDiscoveryLimitation::RecordLimit);
        }
        if bytes > self.policy.max_bytes_per_source
            || current.bytes.saturating_add(bytes) > self.policy.max_bytes_per_source
        {
            return Err(RelayDiscoveryLimitation::ByteLimit);
        }
        if current.signature_checks.saturating_add(signature_checks)
            > self.policy.max_signature_checks
        {
            return Err(RelayDiscoveryLimitation::SignatureLimit);
        }
        let dns_jobs = usize::from(records > 0);
        if self.active_dns_jobs.saturating_add(dns_jobs) > self.policy.max_probe_concurrency {
            return Err(RelayDiscoveryLimitation::Deadline);
        }
        let state = self.sources.entry(source_id).or_default();
        state.records += records;
        state.bytes += bytes;
        state.signature_checks += signature_checks;
        state.exhausted = state.records == self.policy.max_records_per_source
            || state.bytes == self.policy.max_bytes_per_source
            || state.signature_checks == self.policy.max_signature_checks;
        self.total_records += records;
        self.signature_checks += signature_checks;
        self.active_dns_jobs += dns_jobs;
        self.next_permit = self.next_permit.wrapping_add(1);
        let mut permit_preimage = Vec::with_capacity(48);
        permit_preimage.extend_from_slice(&source_id.0);
        permit_preimage.extend_from_slice(&self.next_permit.to_be_bytes());
        permit_preimage.extend_from_slice(&now.to_be_bytes());
        let permit_id = *blake3::hash(&permit_preimage).as_bytes();
        self.permits.insert(
            permit_id,
            PermitLedger {
                source: source_id,
                records,
                bytes,
                signature_checks,
                dns_jobs,
            },
        );
        Ok(RelayPreparationPermit {
            source: source_id,
            source_kind: source,
            record_lengths: record_lengths.to_vec(),
            records,
            bytes,
            signature_checks,
            dns_jobs,
            permit_id,
        })
    }

    fn stage_prepared(
        &mut self,
        permit: RelayPreparationPermit,
        prepared: Vec<PreparedRelayDescriptorAdmission>,
        now: u64,
    ) -> Result<Vec<StagedRelayAdmission>, RelayDiscoveryLimitation> {
        let ledger = self
            .permits
            .remove(&permit.permit_id)
            .ok_or(RelayDiscoveryLimitation::PoisonedSource)?;
        self.active_dns_jobs = self.active_dns_jobs.saturating_sub(ledger.dns_jobs);
        if ledger.source != permit.source
            || ledger.records != permit.records
            || ledger.bytes != permit.bytes
            || ledger.signature_checks != permit.signature_checks
            || ledger.dns_jobs != permit.dns_jobs
            || prepared.len() != permit.records
            || !self.source_is_live(&permit.source_kind, now)
        {
            return Err(RelayDiscoveryLimitation::PoisonedSource);
        }
        let mut digests = BTreeSet::new();
        if prepared
            .iter()
            .any(|value| !digests.insert(*value.digest()))
        {
            return Err(RelayDiscoveryLimitation::PoisonedSource);
        }
        let mut output: Vec<StagedRelayAdmission> = Vec::with_capacity(prepared.len());
        for value in prepared {
            let pending =
                match self
                    .admission
                    .register_prepared_descriptor(value, permit.source.0, now)
                {
                    Ok(value) => value,
                    Err(error) => {
                        for prior in output {
                            let _ = self.admission.abort_descriptor_admission(prior.pending);
                        }
                        return Err(map_admission_error(error));
                    }
                };
            let mut stage_preimage = Vec::with_capacity(64);
            stage_preimage.extend_from_slice(pending.digest());
            stage_preimage.extend_from_slice(&permit.permit_id);
            let stage_id = *blake3::hash(&stage_preimage).as_bytes();
            self.staged.insert(stage_id);
            output.push(StagedRelayAdmission {
                pending,
                source: permit.source,
                charged_records: 1,
                charged_bytes: permit.bytes / permit.records,
                staged_at: now,
                stage_id,
                possession_dials: Vec::new(),
            });
        }
        Ok(output)
    }

    fn abort_preparation(
        &mut self,
        permit: RelayPreparationPermit,
        _now: u64,
    ) -> Result<(), RelayDiscoveryLimitation> {
        if let Some(ledger) = self.permits.remove(&permit.permit_id) {
            if ledger.source != permit.source {
                return Err(RelayDiscoveryLimitation::PoisonedSource);
            }
            self.active_dns_jobs = self.active_dns_jobs.saturating_sub(ledger.dns_jobs);
        }
        Ok(())
    }

    fn commit_descriptor(
        &mut self,
        staged: StagedRelayAdmission,
        proofs: &[RelayPossessionProofV1],
        now: u64,
    ) -> Result<RelayDiscoveryDelta, RelayDiscoveryLimitation> {
        if staged.possession_dials.len() != staged.pending.challenges().len()
            || !self.staged.remove(&staged.stage_id)
        {
            return Err(RelayDiscoveryLimitation::PoisonedSource);
        }
        let _accounting = (
            staged.source,
            staged.charged_records,
            staged.charged_bytes,
            staged.staged_at,
        );
        let validated = self
            .admission
            .complete_descriptor_admission(staged.pending, proofs, now)
            .map_err(map_admission_error)?;
        let node = validated.canonical().relay_node_id;
        let refreshed = self.relays.insert(node, validated).is_some();
        Ok(if refreshed {
            RelayDiscoveryDelta {
                refreshed: vec![node],
                ..RelayDiscoveryDelta::default()
            }
        } else {
            RelayDiscoveryDelta {
                admitted: vec![node],
                ..RelayDiscoveryDelta::default()
            }
        })
    }

    fn abort_descriptor(
        &mut self,
        staged: StagedRelayAdmission,
        _now: u64,
    ) -> Result<(), RelayDiscoveryLimitation> {
        if self.staged.remove(&staged.stage_id) {
            self.admission
                .abort_descriptor_admission(staged.pending)
                .map_err(map_admission_error)?;
        }
        Ok(())
    }

    fn verified_relays(&self) -> impl Iterator<Item = &ValidatedRelayDescriptor> {
        self.relays.values()
    }
}

pub struct ManualPeerInvitation {
    identity: KnownPeerIdentity,
    advertisement: ReachabilityAdvertisementV1,
    canonical_advertisement: Vec<u8>,
}

impl ManualPeerInvitation {
    pub fn identity(&self) -> &KnownPeerIdentity {
        &self.identity
    }

    pub fn advertisement(&self) -> &ReachabilityAdvertisementV1 {
        &self.advertisement
    }

    pub fn canonical_advertisement(&self) -> &[u8] {
        &self.canonical_advertisement
    }
}

pub fn encode_manual_relay_invitation(
    canonical_descriptor: &[u8],
) -> Result<String, RelayDiscoveryLimitation> {
    let object = decode_reachability_object(canonical_descriptor)
        .map_err(|_| RelayDiscoveryLimitation::PoisonedSource)?;
    if !matches!(object, ReachabilityObjectV1::RelayDescriptor(_)) {
        return Err(RelayDiscoveryLimitation::PoisonedSource);
    }
    Ok(format!(
        "{MANUAL_RELAY_PREFIX}{}",
        base64url_encode(canonical_descriptor)
    ))
}

pub fn decode_manual_relay_invitation(
    invitation: &str,
) -> Result<Vec<u8>, RelayDiscoveryLimitation> {
    let encoded = invitation
        .strip_prefix(MANUAL_RELAY_PREFIX)
        .ok_or(RelayDiscoveryLimitation::PoisonedSource)?;
    let bytes = base64url_decode(encoded)?;
    let object =
        decode_reachability_object(&bytes).map_err(|_| RelayDiscoveryLimitation::PoisonedSource)?;
    if !matches!(object, ReachabilityObjectV1::RelayDescriptor(_)) {
        return Err(RelayDiscoveryLimitation::PoisonedSource);
    }
    Ok(bytes)
}

pub fn encode_manual_peer_invitation(
    identity: &KnownPeerIdentity,
    canonical_advertisement: &[u8],
) -> Result<String, RelayDiscoveryLimitation> {
    identity
        .validate()
        .map_err(|_| RelayDiscoveryLimitation::PoisonedSource)?;
    let object = decode_reachability_object(canonical_advertisement)
        .map_err(|_| RelayDiscoveryLimitation::PoisonedSource)?;
    let ReachabilityObjectV1::Advertisement(advertisement) = object else {
        return Err(RelayDiscoveryLimitation::PoisonedSource);
    };
    if advertisement.target_node_id != identity.node_id {
        return Err(RelayDiscoveryLimitation::PoisonedSource);
    }
    verify_manual_advertisement_signature(&advertisement, identity.public_key)?;
    let length = u32::try_from(canonical_advertisement.len())
        .map_err(|_| RelayDiscoveryLimitation::ByteLimit)?;
    let mut envelope = Vec::with_capacity(36 + canonical_advertisement.len());
    envelope.extend_from_slice(&identity.public_key);
    envelope.extend_from_slice(&length.to_be_bytes());
    envelope.extend_from_slice(canonical_advertisement);
    Ok(format!(
        "{MANUAL_PEER_PREFIX}{}",
        base64url_encode(&envelope)
    ))
}

pub fn decode_manual_peer_invitation(
    invitation: &str,
) -> Result<ManualPeerInvitation, RelayDiscoveryLimitation> {
    let encoded = invitation
        .strip_prefix(MANUAL_PEER_PREFIX)
        .ok_or(RelayDiscoveryLimitation::PoisonedSource)?;
    let envelope = base64url_decode(encoded)?;
    if envelope.len() < 36 {
        return Err(RelayDiscoveryLimitation::PoisonedSource);
    }
    let public_key: [u8; 32] = envelope[..32]
        .try_into()
        .map_err(|_| RelayDiscoveryLimitation::PoisonedSource)?;
    let length = u32::from_be_bytes(
        envelope[32..36]
            .try_into()
            .map_err(|_| RelayDiscoveryLimitation::PoisonedSource)?,
    ) as usize;
    if envelope.len() != 36 + length {
        return Err(RelayDiscoveryLimitation::PoisonedSource);
    }
    let canonical = envelope[36..].to_vec();
    let object = decode_reachability_object(&canonical)
        .map_err(|_| RelayDiscoveryLimitation::PoisonedSource)?;
    let ReachabilityObjectV1::Advertisement(advertisement) = object else {
        return Err(RelayDiscoveryLimitation::PoisonedSource);
    };
    let identity = KnownPeerIdentity {
        node_id: principal_node_id(&public_key),
        public_key,
    };
    if advertisement.target_node_id != identity.node_id {
        return Err(RelayDiscoveryLimitation::PoisonedSource);
    }
    verify_manual_advertisement_signature(&advertisement, identity.public_key)?;
    Ok(ManualPeerInvitation {
        identity,
        advertisement,
        canonical_advertisement: canonical,
    })
}

fn verify_manual_advertisement_signature(
    advertisement: &ReachabilityAdvertisementV1,
    public_key: [u8; 32],
) -> Result<(), RelayDiscoveryLimitation> {
    let key = VerifyingKey::from_bytes(&public_key)
        .map_err(|_| RelayDiscoveryLimitation::PoisonedSource)?;
    let object = ReachabilityObjectV1::Advertisement(advertisement.clone());
    let preimage =
        reachability_signing_bytes(&object, ReachabilitySignatureRoleV1::AdvertisementTarget)
            .map_err(|_| RelayDiscoveryLimitation::PoisonedSource)?;
    key.verify(
        &preimage,
        &Signature::from_bytes(&advertisement.target_signature),
    )
    .map_err(|_| RelayDiscoveryLimitation::PoisonedSource)
}

fn map_admission_error(error: RelayAdmissionError) -> RelayDiscoveryLimitation {
    match error {
        RelayAdmissionError::BudgetExceeded => RelayDiscoveryLimitation::RecordLimit,
        RelayAdmissionError::DnsResolutionFailed => RelayDiscoveryLimitation::NoBootstrapReachable,
        RelayAdmissionError::StateUnavailable => RelayDiscoveryLimitation::StateUnavailable,
        _ => RelayDiscoveryLimitation::PoisonedSource,
    }
}

fn base64url_encode(bytes: &[u8]) -> String {
    const ALPHABET: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789-_";
    let mut output = String::with_capacity((bytes.len() * 4).div_ceil(3));
    for chunk in bytes.chunks(3) {
        let value = ((chunk[0] as u32) << 16)
            | ((chunk.get(1).copied().unwrap_or(0) as u32) << 8)
            | chunk.get(2).copied().unwrap_or(0) as u32;
        output.push(ALPHABET[((value >> 18) & 63) as usize] as char);
        output.push(ALPHABET[((value >> 12) & 63) as usize] as char);
        if chunk.len() > 1 {
            output.push(ALPHABET[((value >> 6) & 63) as usize] as char);
        }
        if chunk.len() > 2 {
            output.push(ALPHABET[(value & 63) as usize] as char);
        }
    }
    output
}

fn base64url_decode(value: &str) -> Result<Vec<u8>, RelayDiscoveryLimitation> {
    if value.is_empty() || value.contains('=') || !value.is_ascii() || value.len() % 4 == 1 {
        return Err(RelayDiscoveryLimitation::PoisonedSource);
    }
    let mut output = Vec::with_capacity(value.len() * 3 / 4);
    for chunk in value.as_bytes().chunks(4) {
        let mut decoded = [0_u8; 4];
        for (index, byte) in chunk.iter().enumerate() {
            decoded[index] = base64url_value(*byte)?;
        }
        let packed = ((decoded[0] as u32) << 18)
            | ((decoded[1] as u32) << 12)
            | ((decoded[2] as u32) << 6)
            | decoded[3] as u32;
        output.push((packed >> 16) as u8);
        if chunk.len() > 2 {
            output.push((packed >> 8) as u8);
        }
        if chunk.len() > 3 {
            output.push(packed as u8);
        }
        if chunk.len() == 2 && decoded[1] & 0x0f != 0 || chunk.len() == 3 && decoded[2] & 0x03 != 0
        {
            return Err(RelayDiscoveryLimitation::PoisonedSource);
        }
    }
    Ok(output)
}

fn base64url_value(value: u8) -> Result<u8, RelayDiscoveryLimitation> {
    match value {
        b'A'..=b'Z' => Ok(value - b'A'),
        b'a'..=b'z' => Ok(value - b'a' + 26),
        b'0'..=b'9' => Ok(value - b'0' + 52),
        b'-' => Ok(62),
        b'_' => Ok(63),
        _ => Err(RelayDiscoveryLimitation::PoisonedSource),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::vnext_reachability_crypto::{
        InMemoryReachabilityReplayStore, ReachabilityAdmission,
    };

    #[test]
    fn base64url_round_trip_and_noncanonical_tail_reject() {
        for bytes in [b"a".as_slice(), b"ab", b"abc", b"OneBrain"] {
            assert_eq!(base64url_decode(&base64url_encode(bytes)).unwrap(), bytes);
        }
        assert!(base64url_decode("AB").is_err());
    }

    #[test]
    fn authenticated_pex_lease_must_still_be_live_at_reservation() {
        let registry = Arc::new(InMemoryAuthenticatedSessionRegistry::default());
        let verified = VerifiedAuthenticatedSessionSource::from_verified_route(
            principal_node_id(&[41; 32]),
            [42; 32],
            [43; 32],
            [44; 32],
        );
        let lease = registry.register(&verified).unwrap();
        let source = RelayDiscoverySource::authenticated_pex(lease.clone().authenticated_pex());
        registry.revoke(lease);
        let replay = Arc::new(InMemoryReachabilityReplayStore::default());
        let admission = ReachabilityAdmission::new(replay);
        let mut discovery =
            RelayDiscovery::new(RelayDiscoveryPolicy::default(), admission, registry);
        assert_eq!(
            discovery.reserve_preparation(source, &[1], 1),
            Err(RelayDiscoveryLimitation::SessionNotLive)
        );
    }
}

//! Closed P5 V2 control and evidence contracts.
//!
//! This module deliberately contains data and validation only.  Privileged
//! host mutation is performed by the separately authenticated admin boundary;
//! the long-lived node agent can only consume the receipts produced there.

use ku_core::foundation::NodeId;
use onebrain_protocol::{
    ReachabilityEndpointV1, RelayAssociationV1, RelayReservationV1, RouteFailureCodeV1,
    RouteReceiptV1,
};
use std::path::Path;

use crate::vnext_outbox::DurableCheckpointV1;
use crate::vnext_p5_signer_provider::{DurableSequenceCursor, P5SignerError};

pub const P5_V2_FORMAT: u64 = 2;
pub const MAX_P5_HOST_ID_BYTES: usize = 128;
pub const MAX_P5_RAW_OBJECTS: usize = 64;
pub const MAX_P5_RAW_OBJECT_BYTES: usize = 1_048_576;
pub const MAX_P5_RESPONSE_BYTES: usize = 4_194_304;
pub const MAX_P5_CHILD_RECEIPTS: usize = 4_096;
pub const MAX_P5_ROUTE_EVIDENCE: usize = 64;
pub const MIN_P5_RESERVATIONS: usize = 4;
pub const MAX_P5_RESERVATIONS: usize = 6;

pub const CONTROL_SIGNING_DOMAIN: &[u8] = b"onebrain/p5/signed-control-frame/v2";
pub const CHILD_RECEIPT_SIGNING_DOMAIN: &[u8] = b"onebrain/p5/child-receipt/v2";
pub const AGGREGATE_SIGNING_DOMAIN: &[u8] = b"onebrain/p5/multi-host-aggregate/v2";

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum P5ProviderEvidenceStatusV2 {
    OwnerTelephoneVerifiedProviderDocumentPending,
    ProviderDocumentVerified,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum P5QualificationTierV2 {
    ProductionReference,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum P5RawArchiveEncryptionV2 {
    HpkeX25519HkdfSha256ChaCha20Poly1305,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct P5EvidenceAuthorityV2 {
    pub inventory_blake3: [u8; 32],
    pub public_probe_set_blake3: [u8; 32],
    pub topology_attestation_blake3: [u8; 32],
    pub provider_evidence_blake3: [u8; 32],
    pub provider_evidence_status: P5ProviderEvidenceStatusV2,
    pub qualification_tier: P5QualificationTierV2,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum P5ControlCommandV2 {
    StartReachability,
    EnsureReservations,
    PublishAdvertisement,
    WaitBarrier {
        barrier: [u8; 32],
    },
    ConnectExpected {
        peer: NodeId,
        advertisement_blake3: [u8; 32],
    },
    DeliverMarker {
        marker: [u8; 32],
    },
    RecordCheckpoint,
    PrepareFaultTarget {
        operation_id: [u8; 32],
        fault: P5FaultKindV2,
    },
    MeasureFaultBoundary {
        admin_frame: P5SignedAdminFrameV2,
        admin_response: Box<P5AdminResponseV2>,
    },
    ReconnectExpected {
        peer: NodeId,
    },
    Shutdown,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum P5FaultKindV2 {
    Partition,
    Drop,
    Reorder,
    Duplicate,
    Restart,
    AddressChange,
    SeedOutage,
    SignerOutage,
    DiskPressure,
    SlowPeer,
    BaseObarv002ArchiveRestore,
    Rollback,
    ExplicitReEnable,
    SelectedRelayShutdown,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum P5FaultPhaseV2 {
    Before,
    During,
    After,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum P5AdminActionV2 {
    PrepareSession,
    CleanupSession,
    Observe,
    Apply,
    Clear,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum P5FaultResultV2 {
    ObservedExpectedEffect,
    Recovered,
    RejectedUnsafeOperation,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct P5RootSetV2 {
    pub canonical_root: [u8; 32],
    pub journal_root: [u8; 32],
    pub outbox_root: [u8; 32],
    pub operational_root: [u8; 32],
}

#[derive(Clone, Debug, PartialEq, Eq, Default)]
pub struct P5ResourceObservationV2 {
    pub peak_rss_bytes: u64,
    pub durable_growth_bytes: u64,
    pub task_count: u64,
    pub active_sessions: u64,
    pub max_control_message_bytes: u64,
    pub fault_duration_ms: u64,
    pub reunion_ms: u64,
    pub quiescence_ms: u64,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum P5ServiceStateV2 {
    Active,
    Inactive,
    Failed,
    Missing,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct P5UnitPairStateV2 {
    pub service: P5ServiceStateV2,
    pub socket: P5ServiceStateV2,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct P5FaultTargetDraftV2 {
    pub request_digest: [u8; 32],
    pub session_id: [u8; 32],
    pub host_id: String,
    pub operation_id: [u8; 32],
    pub fault: P5FaultKindV2,
    pub peer_endpoints: Vec<ReachabilityEndpointV1>,
    pub peer_endpoint_set_blake3: [u8; 32],
    pub selected_relay: Option<NodeId>,
    pub route_receipt_blake3: [u8; 32],
    pub issued_at: u64,
    pub expires_at: u64,
    pub host_target_public_key: [u8; 32],
    pub host_target_signature: [u8; 64],
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct P5FaultTargetV2 {
    pub draft: P5FaultTargetDraftV2,
    pub selected_relay_host_id: Option<String>,
    pub inventory_blake3: [u8; 32],
    pub controller_public_key: [u8; 32],
    pub controller_signature: [u8; 64],
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct P5AdminRequestV2 {
    pub format: u64,
    pub request_digest: [u8; 32],
    pub evidence_authority: P5EvidenceAuthorityV2,
    pub session_id: [u8; 32],
    pub host_id: String,
    pub operation_id: [u8; 32],
    pub action: P5AdminActionV2,
    pub fault: Option<P5FaultKindV2>,
    pub phase: Option<P5FaultPhaseV2>,
    pub issued_at: u64,
    pub expires_at: u64,
    pub parameters_digest: [u8; 32],
    pub controller_signature: [u8; 64],
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct P5SignedAdminFrameV2 {
    pub request: P5AdminRequestV2,
    pub target: Option<P5FaultTargetV2>,
    pub canonical_blake3: [u8; 32],
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct P5SessionConfigV2 {
    pub format: u64,
    pub release_request_blake3: [u8; 32],
    pub release_signature_blake3: [u8; 32],
    pub base_release_policy_blake3: [u8; 32],
    pub p5_request_blake3: [u8; 32],
    pub p5_signature_blake3: [u8; 32],
    pub p5_approval_policy_blake3: [u8; 32],
    pub evidence_authority: P5EvidenceAuthorityV2,
    pub controller_application_public_key: [u8; 32],
    pub controller_ssh_key_sha256: [u8; 32],
    pub host_id: String,
    pub candidate_commit: [u8; 20],
    pub candidate_tree: [u8; 20],
    pub bundle_manifest_blake3: [u8; 32],
    pub profile_blake3: [u8; 32],
    pub vector_blake3: [u8; 32],
    pub allowlist_blake3: [u8; 32],
    pub identity_public_key: [u8; 32],
    pub receipt_public_key: [u8; 32],
    pub session_id: [u8; 32],
    pub issued_at: u64,
    pub expires_at: u64,
    pub config_blake3: [u8; 32],
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct P5SignedControlFrameV2 {
    pub format: u64,
    pub request_digest: [u8; 32],
    pub evidence_authority: P5EvidenceAuthorityV2,
    pub session_id: [u8; 32],
    pub host_id: String,
    pub sequence: u64,
    pub issued_at: u64,
    pub expires_at: u64,
    pub command: P5ControlCommandV2,
    pub command_blake3: [u8; 32],
    pub frame_blake3: [u8; 32],
    pub controller_public_key: [u8; 32],
    pub controller_signature: [u8; 64],
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct P5BootstrapAdminFrameV2 {
    pub format: u64,
    pub release_request: Vec<u8>,
    pub release_signature: Vec<u8>,
    pub base_release_policy: Vec<u8>,
    pub base_verifier_public_keyring: Vec<u8>,
    pub p5_request: Vec<u8>,
    pub p5_signature: [u8; 64],
    pub p5_approval_policy: Vec<u8>,
    pub inventory: Vec<u8>,
    pub bundle_manifest_digest: [u8; 32],
    pub proposed_session_config: P5SessionConfigV2,
    pub host_id: String,
    pub operation_id: [u8; 32],
    pub issued_at: u64,
    pub expires_at: u64,
    pub controller_signature: [u8; 64],
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct P5BootstrapResponseV2 {
    pub format: u64,
    pub request_digest: [u8; 32],
    pub evidence_authority: P5EvidenceAuthorityV2,
    pub session_id: [u8; 32],
    pub host_id: String,
    pub operation_id: [u8; 32],
    pub installed_config_blake3: [u8; 32],
    pub units_changed: bool,
    pub network_changed: bool,
    pub finished_at: u64,
    pub response_blake3: [u8; 32],
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct P5FinalizeSessionV2 {
    pub format: u64,
    pub request_digest: [u8; 32],
    pub evidence_authority: P5EvidenceAuthorityV2,
    pub session_id: [u8; 32],
    pub host_id: String,
    pub cleanup_receipt_blake3: [u8; 32],
    pub operation_id: [u8; 32],
    pub issued_at: u64,
    pub expires_at: u64,
    pub controller_signature: [u8; 64],
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct P5FinalizationResponseV2 {
    pub format: u64,
    pub request_digest: [u8; 32],
    pub evidence_authority: P5EvidenceAuthorityV2,
    pub session_id: [u8; 32],
    pub host_id: String,
    pub cleanup_receipt_blake3: [u8; 32],
    pub signer_stopped: bool,
    pub session_config_removed: bool,
    pub finished_at: u64,
    pub response_blake3: [u8; 32],
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct P5BeforeObservationV2 {
    pub namespace_inode: Option<u64>,
    pub agent_pid: Option<u64>,
    pub agent_namespace_inode: Option<u64>,
    pub peer_endpoint_set_blake3: Option<[u8; 32]>,
    pub address_blake3: Option<[u8; 32]>,
    pub network_epoch: Option<u64>,
    pub candidates_blake3: Option<[u8; 32]>,
    pub signer_sequence: Option<u64>,
    pub root_filesystem_free_bytes: Option<u64>,
    pub generation: Option<u64>,
    pub state_root: Option<[u8; 32]>,
    pub relay_descriptor_sequence: Option<u64>,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct P5DuringObservationV2 {
    pub namespace_inode: Option<u64>,
    pub agent_pid: Option<u64>,
    pub agent_namespace_inode: Option<u64>,
    pub qdisc_canonical_blake3: Option<[u8; 32]>,
    pub nft_ruleset_canonical_blake3: Option<[u8; 32]>,
    pub matched_packets: Option<u64>,
    pub peer_endpoint_set_blake3: Option<[u8; 32]>,
    pub address_blake3: Option<[u8; 32]>,
    pub network_epoch: Option<u64>,
    pub candidates_blake3: Option<[u8; 32]>,
    pub failed_signing_request_blake3: Option<[u8; 32]>,
    pub signer_listener_fd_count: Option<u64>,
    pub root_filesystem_free_bytes: Option<u64>,
    pub fault_mount_free_bytes: Option<u64>,
    pub generation: Option<u64>,
    pub state_root: Option<[u8; 32]>,
    pub network_enabled: Option<bool>,
    pub continuity_receipt_blake3: Option<[u8; 32]>,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct P5AfterObservationV2 {
    pub namespace_inode: Option<u64>,
    pub agent_pid: Option<u64>,
    pub agent_namespace_inode: Option<u64>,
    pub accepted_sequence: Option<u64>,
    pub replay_rejected: Option<bool>,
    pub restored_address_blake3: Option<[u8; 32]>,
    pub network_epoch: Option<u64>,
    pub candidates_blake3: Option<[u8; 32]>,
    pub signer_sequence: Option<u64>,
    pub root_filesystem_free_bytes: Option<u64>,
    pub fault_mount_present: Option<bool>,
    pub generation: Option<u64>,
    pub state_root: Option<[u8; 32]>,
    pub archive_root: Option<[u8; 32]>,
    pub network_enabled: Option<bool>,
    pub relay_descriptor_sequence: Option<u64>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum P5FaultSpecificObservationV2 {
    Lifecycle {
        namespace_inode: u64,
        agent_namespace_inode: Option<u64>,
    },
    Before(P5BeforeObservationV2),
    During(P5DuringObservationV2),
    After(P5AfterObservationV2),
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct P5OperationObservationV2 {
    pub namespace_present: bool,
    pub agent_pid: Option<u64>,
    pub agent_units: P5UnitPairStateV2,
    pub identity_signer_units: P5UnitPairStateV2,
    pub receipt_signer_units: P5UnitPairStateV2,
    pub relay_service: P5ServiceStateV2,
    pub root_filesystem_free_bytes: u64,
    pub active_generation: u64,
    pub archive_root: [u8; 32],
    pub fault_specific: P5FaultSpecificObservationV2,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum P5RawEvidenceKindV2 {
    Stdout,
    Stderr,
    FaultTarget,
    NftRuleset,
    QdiscState,
    UnitState,
    NamespaceState,
    EndpointProbe,
    Checkpoint,
    ReservationSnapshot,
    RelayDescriptorChain,
    LifecycleState,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct P5RawEvidenceObjectV2 {
    pub format: u64,
    pub kind: P5RawEvidenceKindV2,
    pub canonical_blake3: [u8; 32],
    pub bytes: Vec<u8>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct P5EncryptedRawArchiveV2 {
    pub format: u64,
    pub scheme: P5RawArchiveEncryptionV2,
    pub recipient_public_key_blake3: [u8; 32],
    pub hpke_encapsulated_key: [u8; 32],
    pub aad_blake3: [u8; 32],
    pub plaintext_manifest_blake3: [u8; 32],
    pub ciphertext: Vec<u8>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct P5OperationReceiptV2 {
    pub request_digest: [u8; 32],
    pub evidence_authority: P5EvidenceAuthorityV2,
    pub session_id: [u8; 32],
    pub admin_request_digest: [u8; 32],
    pub parameters_digest: [u8; 32],
    pub allowlist_digest: [u8; 32],
    pub operation_id: [u8; 32],
    pub host_id: String,
    pub action: P5AdminActionV2,
    pub fault: Option<P5FaultKindV2>,
    pub phase: Option<P5FaultPhaseV2>,
    pub started_at: u64,
    pub finished_at: u64,
    pub exit_code: i32,
    pub raw_object_digests: Vec<[u8; 32]>,
    pub operation_stdout_blake3: [u8; 32],
    pub operation_stderr_blake3: [u8; 32],
    pub observation: P5OperationObservationV2,
    pub operation_public_key: [u8; 32],
    pub operation_signature: [u8; 64],
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct P5AdminResponseV2 {
    pub format: u64,
    pub receipt: P5OperationReceiptV2,
    pub raw_objects: Vec<P5RawEvidenceObjectV2>,
    pub response_blake3: [u8; 32],
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct P5FaultEvidenceV2 {
    pub fault: P5FaultKindV2,
    pub before_roots: P5RootSetV2,
    pub during_roots: P5RootSetV2,
    pub after_roots: P5RootSetV2,
    pub resource_observation: P5ResourceObservationV2,
    pub operation_receipts: [P5OperationReceiptV2; 3],
    pub result: P5FaultResultV2,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct P5RingEdgeV2 {
    pub from: NodeId,
    pub to: NodeId,
    pub marker: [u8; 32],
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RouteFailureEvidenceV2 {
    pub selected_relay: NodeId,
    pub failure_code: RouteFailureCodeV1,
    pub alternate_relay: NodeId,
    pub alternate_association: RelayAssociationV1,
    pub resumed_route_receipt: RouteReceiptV1,
    pub prior_session_id: [u8; 32],
    pub resumed_session_id: [u8; 32],
    pub prior_binding: [u8; 32],
    pub resumed_binding: [u8; 32],
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct P5ReservationSnapshotV2 {
    pub captured_at: u64,
    pub reservations: Vec<RelayReservationV1>,
    pub selected_association: RelayAssociationV1,
    pub canonical_blake3: [u8; 32],
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct P5RouteEvidenceV2 {
    pub edge: P5RingEdgeV2,
    pub route_receipt: RouteReceiptV1,
    pub reservation_snapshot: Option<P5ReservationSnapshotV2>,
    pub acknowledged_checkpoint: DurableCheckpointV1,
    pub failure: Option<RouteFailureEvidenceV2>,
    pub resumed_checkpoint: Option<DurableCheckpointV1>,
    pub faults: Vec<P5FaultEvidenceV2>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum P5ChildResultV2 {
    Applied,
    Observed,
    Reconnected,
    Rejected,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct P5ChildReceiptV2 {
    pub format: u64,
    pub request_digest: [u8; 32],
    pub evidence_authority: P5EvidenceAuthorityV2,
    pub session_id: [u8; 32],
    pub inventory_blake3: [u8; 32],
    pub host_id: String,
    pub command_sequence: u64,
    pub control_frame_blake3: [u8; 32],
    pub command_blake3: [u8; 32],
    pub result: P5ChildResultV2,
    pub root_set: P5RootSetV2,
    pub resource_observation: P5ResourceObservationV2,
    pub route_evidence: Option<P5RouteEvidenceV2>,
    pub operation_receipt_blake3: Option<[u8; 32]>,
    pub raw_object_digests: Vec<[u8; 32]>,
    pub issued_at: u64,
    pub signer_public_key: [u8; 32],
    pub receipt_blake3: [u8; 32],
    pub signature: [u8; 64],
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct P5QualificationDerivationV2 {
    pub all_expected_peers: bool,
    pub mixed_path_classes: bool,
    pub relay_only_path_classes: bool,
    pub all_real_faults: bool,
    pub selected_relay_failed: bool,
    pub alternate_pre_reserved: bool,
    pub fresh_reauthentication: bool,
    pub exact_checkpoint_resume: bool,
    pub resource_bounds: bool,
    pub cleanup_complete: bool,
    pub multi_host_qualified: bool,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum P5LimitationCodeV2 {
    ProviderEvidencePending,
    TopologyOwnerAttested,
    RelayDiversityNotProviderDiversity,
    MobileDeferred,
    PublicFleetOperationsDeferred,
    SystemdManagerProbeUnavailable,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct P5MultiHostAggregateV2 {
    pub format: u64,
    pub request_digest: [u8; 32],
    pub evidence_authority: P5EvidenceAuthorityV2,
    pub session_id: [u8; 32],
    pub profile_blake3: [u8; 32],
    pub vector_blake3: [u8; 32],
    pub allowlist_blake3: [u8; 32],
    pub controller_public_key: [u8; 32],
    pub child_receipts: Vec<P5ChildReceiptV2>,
    pub routes: Vec<P5RouteEvidenceV2>,
    pub bootstrap_response_digests: Vec<[u8; 32]>,
    pub finalization_response_digests: Vec<[u8; 32]>,
    pub raw_manifest_blake3: [u8; 32],
    pub qualification: P5QualificationDerivationV2,
    pub limitations: Vec<P5LimitationCodeV2>,
    pub aggregate_blake3: [u8; 32],
    pub controller_signature: [u8; 64],
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct P5VerificationReceiptV2 {
    pub format: u64,
    pub request_digest: [u8; 32],
    pub evidence_authority: P5EvidenceAuthorityV2,
    pub session_id: [u8; 32],
    pub aggregate_blake3: [u8; 32],
    pub raw_manifest_blake3: [u8; 32],
    pub verified_child_receipts: u64,
    pub verified_raw_objects: u64,
    pub multi_host_qualified: bool,
    pub limitations: Vec<P5LimitationCodeV2>,
    pub verifier_implementation_blake3: [u8; 32],
    pub verified_at: u64,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum P5V2ValidationError {
    WrongFormat,
    EmptyDigest,
    InvalidHost,
    Expired,
    InvalidActionPhase,
    InvalidFaultTarget,
    RawObjectLimit,
    RawObjectOrder,
    RawObjectDigest,
    RawObjectSetMismatch,
    ReservationCount,
    FailureEvidenceShape,
    QualificationMismatch,
    EvidenceAuthorityMismatch,
    CollectionLimit,
    Replay,
}

/// Durable, request/session-bound admission gate used by the long-lived agent.
/// The cursor is advanced before a command is returned to the executor.
pub struct P5AgentCommandGateV2 {
    request_digest: [u8; 32],
    evidence_authority: P5EvidenceAuthorityV2,
    session_id: [u8; 32],
    host_id: String,
    cursor: DurableSequenceCursor,
}

impl P5AgentCommandGateV2 {
    pub fn open(
        cursor_path: &Path,
        request_digest: [u8; 32],
        evidence_authority: P5EvidenceAuthorityV2,
        session_id: [u8; 32],
        host_id: String,
    ) -> Result<Self, P5V2ValidationError> {
        evidence_authority.validate()?;
        validate_host(&host_id)?;
        let mut binding = blake3::Hasher::new();
        binding.update(b"onebrain/p5/agent-command-cursor/v2\0");
        binding.update(&request_digest);
        binding.update(&evidence_authority.inventory_blake3);
        binding.update(&session_id);
        binding.update(host_id.as_bytes());
        let cursor = DurableSequenceCursor::open(cursor_path, *binding.finalize().as_bytes())
            .map_err(map_cursor_error)?;
        Ok(Self {
            request_digest,
            evidence_authority,
            session_id,
            host_id,
            cursor,
        })
    }

    pub fn admit(
        &self,
        frame: &P5SignedControlFrameV2,
        now: u64,
    ) -> Result<(), P5V2ValidationError> {
        frame.validate(now)?;
        if frame.request_digest != self.request_digest
            || frame.evidence_authority != self.evidence_authority
            || frame.session_id != self.session_id
            || frame.host_id != self.host_id
        {
            return Err(P5V2ValidationError::EvidenceAuthorityMismatch);
        }
        self.cursor
            .advance(frame.sequence)
            .map_err(map_cursor_error)
    }
}

fn map_cursor_error(error: P5SignerError) -> P5V2ValidationError {
    match error {
        P5SignerError::Replay => P5V2ValidationError::Replay,
        _ => P5V2ValidationError::EvidenceAuthorityMismatch,
    }
}

impl P5EvidenceAuthorityV2 {
    pub fn validate(&self) -> Result<(), P5V2ValidationError> {
        if [
            self.inventory_blake3,
            self.public_probe_set_blake3,
            self.topology_attestation_blake3,
            self.provider_evidence_blake3,
        ]
        .iter()
        .any(|digest| *digest == [0; 32])
        {
            return Err(P5V2ValidationError::EmptyDigest);
        }
        Ok(())
    }
}

pub fn validate_admin_action_phase(
    action: P5AdminActionV2,
    fault: Option<P5FaultKindV2>,
    phase: Option<P5FaultPhaseV2>,
) -> Result<(), P5V2ValidationError> {
    let valid = match action {
        P5AdminActionV2::PrepareSession | P5AdminActionV2::CleanupSession => {
            fault.is_none() && phase.is_none()
        }
        P5AdminActionV2::Observe => fault.is_some() && phase == Some(P5FaultPhaseV2::Before),
        P5AdminActionV2::Apply => fault.is_some() && phase == Some(P5FaultPhaseV2::During),
        P5AdminActionV2::Clear => fault.is_some() && phase == Some(P5FaultPhaseV2::After),
    };
    if valid {
        Ok(())
    } else {
        Err(P5V2ValidationError::InvalidActionPhase)
    }
}

impl P5AdminRequestV2 {
    pub fn validate(&self, now: u64) -> Result<(), P5V2ValidationError> {
        if self.format != P5_V2_FORMAT {
            return Err(P5V2ValidationError::WrongFormat);
        }
        validate_host(&self.host_id)?;
        self.evidence_authority.validate()?;
        if self.issued_at > now || now > self.expires_at {
            return Err(P5V2ValidationError::Expired);
        }
        validate_admin_action_phase(self.action, self.fault, self.phase)
    }
}

impl P5SignedControlFrameV2 {
    pub fn validate(&self, now: u64) -> Result<(), P5V2ValidationError> {
        if self.format != P5_V2_FORMAT {
            return Err(P5V2ValidationError::WrongFormat);
        }
        validate_host(&self.host_id)?;
        self.evidence_authority.validate()?;
        if self.sequence == 0 || self.command_blake3 == [0; 32] || self.frame_blake3 == [0; 32] {
            return Err(P5V2ValidationError::EmptyDigest);
        }
        if self.issued_at > now || now > self.expires_at {
            return Err(P5V2ValidationError::Expired);
        }
        Ok(())
    }
}

impl P5AdminResponseV2 {
    pub fn validate(&self) -> Result<(), P5V2ValidationError> {
        if self.format != P5_V2_FORMAT {
            return Err(P5V2ValidationError::WrongFormat);
        }
        if self.raw_objects.len() > MAX_P5_RAW_OBJECTS {
            return Err(P5V2ValidationError::RawObjectLimit);
        }
        let mut total = 0usize;
        let mut previous = None;
        let mut digests = Vec::with_capacity(self.raw_objects.len());
        for object in &self.raw_objects {
            if object.format != P5_V2_FORMAT {
                return Err(P5V2ValidationError::WrongFormat);
            }
            total = total
                .checked_add(object.bytes.len())
                .ok_or(P5V2ValidationError::RawObjectLimit)?;
            if object.bytes.len() > MAX_P5_RAW_OBJECT_BYTES || total > MAX_P5_RESPONSE_BYTES {
                return Err(P5V2ValidationError::RawObjectLimit);
            }
            let computed = *blake3::hash(&object.bytes).as_bytes();
            if computed != object.canonical_blake3 {
                return Err(P5V2ValidationError::RawObjectDigest);
            }
            let key = (object.kind, object.canonical_blake3);
            if previous.as_ref().is_some_and(|prior| prior >= &key) {
                return Err(P5V2ValidationError::RawObjectOrder);
            }
            previous = Some(key);
            digests.push(object.canonical_blake3);
        }
        if digests != self.receipt.raw_object_digests {
            return Err(P5V2ValidationError::RawObjectSetMismatch);
        }
        validate_admin_action_phase(self.receipt.action, self.receipt.fault, self.receipt.phase)
    }
}

impl P5FaultEvidenceV2 {
    pub fn validate(&self) -> Result<(), P5V2ValidationError> {
        let expected = [
            P5FaultPhaseV2::Before,
            P5FaultPhaseV2::During,
            P5FaultPhaseV2::After,
        ];
        for (receipt, phase) in self.operation_receipts.iter().zip(expected) {
            if receipt.fault != Some(self.fault) || receipt.phase != Some(phase) {
                return Err(P5V2ValidationError::InvalidActionPhase);
            }
            validate_admin_action_phase(receipt.action, receipt.fault, receipt.phase)?;
        }
        Ok(())
    }
}

impl P5ReservationSnapshotV2 {
    pub fn validate_shape(&self) -> Result<(), P5V2ValidationError> {
        if !(MIN_P5_RESERVATIONS..=MAX_P5_RESERVATIONS).contains(&self.reservations.len()) {
            return Err(P5V2ValidationError::ReservationCount);
        }
        let ids: std::collections::BTreeSet<_> = self
            .reservations
            .iter()
            .map(|reservation| reservation.reservation_id)
            .collect();
        if ids.len() != self.reservations.len()
            || !ids.contains(&self.selected_association.initiator_reservation_id)
            || !ids.contains(&self.selected_association.target_reservation_id)
        {
            return Err(P5V2ValidationError::ReservationCount);
        }
        Ok(())
    }
}

impl P5RouteEvidenceV2 {
    pub fn validate_shape(&self) -> Result<(), P5V2ValidationError> {
        let faulted = self.failure.is_some();
        if faulted != self.reservation_snapshot.is_some()
            || faulted != self.resumed_checkpoint.is_some()
        {
            return Err(P5V2ValidationError::FailureEvidenceShape);
        }
        if let Some(snapshot) = &self.reservation_snapshot {
            snapshot.validate_shape()?;
        }
        Ok(())
    }
}

impl P5QualificationDerivationV2 {
    pub fn derive(
        all_expected_peers: bool,
        mixed_path_classes: bool,
        relay_only_path_classes: bool,
        all_real_faults: bool,
        selected_relay_failed: bool,
        alternate_pre_reserved: bool,
        fresh_reauthentication: bool,
        exact_checkpoint_resume: bool,
        resource_bounds: bool,
        cleanup_complete: bool,
    ) -> Self {
        let multi_host_qualified = all_expected_peers
            && relay_only_path_classes
            && all_real_faults
            && selected_relay_failed
            && alternate_pre_reserved
            && fresh_reauthentication
            && exact_checkpoint_resume
            && resource_bounds
            && cleanup_complete;
        Self {
            all_expected_peers,
            mixed_path_classes,
            relay_only_path_classes,
            all_real_faults,
            selected_relay_failed,
            alternate_pre_reserved,
            fresh_reauthentication,
            exact_checkpoint_resume,
            resource_bounds,
            cleanup_complete,
            multi_host_qualified,
        }
    }

    pub fn validate(&self) -> Result<(), P5V2ValidationError> {
        let derived = Self::derive(
            self.all_expected_peers,
            self.mixed_path_classes,
            self.relay_only_path_classes,
            self.all_real_faults,
            self.selected_relay_failed,
            self.alternate_pre_reserved,
            self.fresh_reauthentication,
            self.exact_checkpoint_resume,
            self.resource_bounds,
            self.cleanup_complete,
        );
        if self.multi_host_qualified == derived.multi_host_qualified {
            Ok(())
        } else {
            Err(P5V2ValidationError::QualificationMismatch)
        }
    }
}

impl P5MultiHostAggregateV2 {
    pub fn validate_shape(&self) -> Result<(), P5V2ValidationError> {
        if self.format != P5_V2_FORMAT {
            return Err(P5V2ValidationError::WrongFormat);
        }
        self.evidence_authority.validate()?;
        self.qualification.validate()?;
        if self.child_receipts.len() > MAX_P5_CHILD_RECEIPTS
            || self.routes.len() > MAX_P5_ROUTE_EVIDENCE
        {
            return Err(P5V2ValidationError::CollectionLimit);
        }
        if self
            .child_receipts
            .iter()
            .any(|r| r.evidence_authority != self.evidence_authority)
            || self
                .child_receipts
                .iter()
                .any(|r| r.inventory_blake3 != self.evidence_authority.inventory_blake3)
        {
            return Err(P5V2ValidationError::EvidenceAuthorityMismatch);
        }
        for route in &self.routes {
            route.validate_shape()?;
        }
        Ok(())
    }
}

fn validate_host(host: &str) -> Result<(), P5V2ValidationError> {
    if host.is_empty()
        || host.len() > MAX_P5_HOST_ID_BYTES
        || !host.is_ascii()
        || !host
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'-' | b'_'))
    {
        Err(P5V2ValidationError::InvalidHost)
    } else {
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn authority() -> P5EvidenceAuthorityV2 {
        P5EvidenceAuthorityV2 {
            inventory_blake3: [1; 32],
            public_probe_set_blake3: [2; 32],
            topology_attestation_blake3: [3; 32],
            provider_evidence_blake3: [4; 32],
            provider_evidence_status:
                P5ProviderEvidenceStatusV2::OwnerTelephoneVerifiedProviderDocumentPending,
            qualification_tier: P5QualificationTierV2::ProductionReference,
        }
    }

    #[test]
    fn vnext_p5_multi_host_v2_fault_set_is_exact_v1_superset() {
        let faults = [
            P5FaultKindV2::Partition,
            P5FaultKindV2::Drop,
            P5FaultKindV2::Reorder,
            P5FaultKindV2::Duplicate,
            P5FaultKindV2::Restart,
            P5FaultKindV2::AddressChange,
            P5FaultKindV2::SeedOutage,
            P5FaultKindV2::SignerOutage,
            P5FaultKindV2::DiskPressure,
            P5FaultKindV2::SlowPeer,
            P5FaultKindV2::BaseObarv002ArchiveRestore,
            P5FaultKindV2::Rollback,
            P5FaultKindV2::ExplicitReEnable,
            P5FaultKindV2::SelectedRelayShutdown,
        ];
        assert_eq!(faults.len(), 14);
        assert_eq!(
            faults
                .iter()
                .collect::<std::collections::BTreeSet<_>>()
                .len(),
            14
        );
    }

    #[test]
    fn vnext_p5_multi_host_v2_admin_action_phase_is_closed() {
        assert!(validate_admin_action_phase(P5AdminActionV2::PrepareSession, None, None).is_ok());
        assert!(validate_admin_action_phase(
            P5AdminActionV2::Observe,
            Some(P5FaultKindV2::Drop),
            Some(P5FaultPhaseV2::Before)
        )
        .is_ok());
        assert_eq!(
            validate_admin_action_phase(
                P5AdminActionV2::Apply,
                Some(P5FaultKindV2::Drop),
                Some(P5FaultPhaseV2::Before)
            ),
            Err(P5V2ValidationError::InvalidActionPhase)
        );
        assert_eq!(
            validate_admin_action_phase(
                P5AdminActionV2::CleanupSession,
                Some(P5FaultKindV2::Drop),
                None
            ),
            Err(P5V2ValidationError::InvalidActionPhase)
        );
    }

    #[test]
    fn vnext_p5_multi_host_v2_qualification_is_derived_not_caller_authored() {
        let mut q = P5QualificationDerivationV2::derive(
            true, false, true, true, true, true, true, true, true, true,
        );
        assert!(q.multi_host_qualified && q.validate().is_ok());
        q.cleanup_complete = false;
        assert_eq!(
            q.validate(),
            Err(P5V2ValidationError::QualificationMismatch)
        );
    }

    #[test]
    fn vnext_p5_multi_host_v2_raw_response_requires_sorted_complete_digest_bound_objects() {
        let bytes = b"operation output".to_vec();
        let digest = *blake3::hash(&bytes).as_bytes();
        let receipt = sample_receipt(vec![digest]);
        let mut response = P5AdminResponseV2 {
            format: 2,
            receipt,
            raw_objects: vec![P5RawEvidenceObjectV2 {
                format: 2,
                kind: P5RawEvidenceKindV2::Stdout,
                canonical_blake3: digest,
                bytes,
            }],
            response_blake3: [9; 32],
        };
        assert!(response.validate().is_ok());
        response.raw_objects[0].bytes.push(0);
        assert_eq!(
            response.validate(),
            Err(P5V2ValidationError::RawObjectDigest)
        );
    }

    #[test]
    fn vnext_p5_multi_host_v2_receipt_preserves_pending_provider_status_explicitly() {
        let receipt = sample_receipt(Vec::new());
        assert_eq!(
            receipt.evidence_authority.provider_evidence_status,
            P5ProviderEvidenceStatusV2::OwnerTelephoneVerifiedProviderDocumentPending
        );
        assert_ne!(receipt.evidence_authority.provider_evidence_blake3, [0; 32]);
    }

    #[test]
    fn vnext_p5_multi_host_v2_agent_cursor_rejects_replay_after_restart() {
        let temp = tempfile::tempdir().unwrap();
        let path = temp.path().join("agent-cursor");
        let gate =
            P5AgentCommandGateV2::open(&path, [5; 32], authority(), [6; 32], "runner-a".into())
                .unwrap();
        let frame = P5SignedControlFrameV2 {
            format: 2,
            request_digest: [5; 32],
            evidence_authority: authority(),
            session_id: [6; 32],
            host_id: "runner-a".into(),
            sequence: 1,
            issued_at: 10,
            expires_at: 20,
            command: P5ControlCommandV2::StartReachability,
            command_blake3: [7; 32],
            frame_blake3: [8; 32],
            controller_public_key: [9; 32],
            controller_signature: [10; 64],
        };
        gate.admit(&frame, 15).unwrap();
        let reopened =
            P5AgentCommandGateV2::open(&path, [5; 32], authority(), [6; 32], "runner-a".into())
                .unwrap();
        assert_eq!(reopened.admit(&frame, 15), Err(P5V2ValidationError::Replay));
    }

    fn sample_receipt(raw_object_digests: Vec<[u8; 32]>) -> P5OperationReceiptV2 {
        let inactive = P5UnitPairStateV2 {
            service: P5ServiceStateV2::Inactive,
            socket: P5ServiceStateV2::Inactive,
        };
        P5OperationReceiptV2 {
            request_digest: [5; 32],
            evidence_authority: authority(),
            session_id: [6; 32],
            admin_request_digest: [7; 32],
            parameters_digest: [8; 32],
            allowlist_digest: [9; 32],
            operation_id: [10; 32],
            host_id: "runner-a".into(),
            action: P5AdminActionV2::Observe,
            fault: Some(P5FaultKindV2::Drop),
            phase: Some(P5FaultPhaseV2::Before),
            started_at: 1,
            finished_at: 2,
            exit_code: 0,
            raw_object_digests,
            operation_stdout_blake3: [11; 32],
            operation_stderr_blake3: [12; 32],
            observation: P5OperationObservationV2 {
                namespace_present: true,
                agent_pid: Some(1),
                agent_units: inactive.clone(),
                identity_signer_units: inactive.clone(),
                receipt_signer_units: inactive,
                relay_service: P5ServiceStateV2::Active,
                root_filesystem_free_bytes: 1,
                active_generation: 1,
                archive_root: [13; 32],
                fault_specific: P5FaultSpecificObservationV2::Before(
                    P5BeforeObservationV2::default(),
                ),
            },
            operation_public_key: [14; 32],
            operation_signature: [15; 64],
        }
    }
}

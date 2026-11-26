//! Shared multi-pillar deterministic CBOR wire envelopes (`<= 1 MiB` frame guard)
//! for Waves 3–7 (`P3` Discovery, `P6` Remote Cognition, `P5` OBT Ledger, and
//! `P10`/`P7` Personal Mesh & OBKG FedR).

use ku_core::foundation::{
    decode_canonical, encode_canonical, ActorId, CanonicalError, CanonicalValue, DeviceId,
    EventCid, FeedId, LeaseCid, NodeId, ObjectCid, PermitCid, ResourceProfile, SelectorCid,
};

use crate::session_codec::{session_capability_from_name, SessionCodecError};
use crate::types::SessionCapability;

pub const PILLAR_WIRE_SCHEMA_MAJOR: u64 = 1;
pub const MAX_PILLAR_WIRE_FRAME_BYTES: usize = 1_048_576; // 1 MiB hard frame guard
pub const MAX_WATCH_TTL_SECS: u32 = 3_600;
pub const MAX_DELEGATED_JOB_LEASE_TTL_MS: u32 = 300_000;
pub const ROUTE_NEED_SKETCH_BUCKET_SIZES: [usize; 3] = [512, 1_024, 2_048];

pub const CAPABILITY_PROVIDER_DIRECTORY_V1: &str = "obp/provider-directory/1";
pub const CAPABILITY_PRIVATE_DISCOVERY_V1: &str = "obp/private-discovery/1";
pub const CAPABILITY_SELECTOR_WATCH_V1: &str = "obp/selector-watch/1";
pub const CAPABILITY_COGNITION_V1: &str = "obp/cognition/1";
pub const CAPABILITY_OBT_LEDGER_V1: &str = "obp/obt-ledger/1";
pub const CAPABILITY_PERSONAL_MESH_V1: &str = "obp/personal-mesh/1";
pub const CAPABILITY_OBKG_FEDR_V1: &str = "obp/obkg-fedr/1";

pub fn provider_directory_capability() -> Result<SessionCapability, SessionCodecError> {
    session_capability_from_name(CAPABILITY_PROVIDER_DIRECTORY_V1)
}

pub fn private_discovery_capability() -> Result<SessionCapability, SessionCodecError> {
    session_capability_from_name(CAPABILITY_PRIVATE_DISCOVERY_V1)
}

pub fn selector_watch_capability() -> Result<SessionCapability, SessionCodecError> {
    session_capability_from_name(CAPABILITY_SELECTOR_WATCH_V1)
}

pub fn cognition_capability() -> Result<SessionCapability, SessionCodecError> {
    session_capability_from_name(CAPABILITY_COGNITION_V1)
}

pub fn obt_ledger_capability() -> Result<SessionCapability, SessionCodecError> {
    session_capability_from_name(CAPABILITY_OBT_LEDGER_V1)
}

pub fn personal_mesh_capability() -> Result<SessionCapability, SessionCodecError> {
    session_capability_from_name(CAPABILITY_PERSONAL_MESH_V1)
}

pub fn obkg_fedr_capability() -> Result<SessionCapability, SessionCodecError> {
    session_capability_from_name(CAPABILITY_OBKG_FEDR_V1)
}

pub mod pillar_family_id {
    pub const DISCOVERY: u64 = 1;
    pub const COGNITION: u64 = 2;
    pub const OBT_LEDGER: u64 = 3;
    pub const PERSONAL_MESH: u64 = 4;
}

// ─── 1. P3 Discovery Wire Family ─────────────────────────────────────────────

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(u64)]
pub enum DisclosureCapsuleStageWireV1 {
    AffordanceSketch = 1,
    ConstraintSketch = 2,
    EvidenceReferences = 3,
    FullNegotiatedPayload = 4,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(u64)]
pub enum CapsuleCancelReasonV1 {
    UserAborted = 1,
    PermitRevokedOrExpired = 2,
    PolicyViolation = 3,
    BudgetExhausted = 4,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CapsuleCancelV1 {
    pub session_id: [u8; 32],
    pub permit_cid: PermitCid,
    pub reason: CapsuleCancelReasonV1,
    pub signer_public_key: [u8; 32],
    pub signer_proof: [u8; 64],
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct WatchRootHintV1 {
    pub selector_cid: SelectorCid,
    pub new_root: [u8; 32],
    pub ttl_secs: u32,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum DiscoveryWireMessageV1 {
    ProviderLeasePublish {
        dht_key: [u8; 32],
        lease_cid: LeaseCid,
        signed_lease_bytes: Vec<u8>,
    },
    ProviderRetirePublish {
        dht_key: [u8; 32],
        retire_cid: ObjectCid,
        signed_retire_bytes: Vec<u8>,
    },
    ProviderDirectoryLookup {
        dht_key: [u8; 32],
        selector_cid: SelectorCid,
        max_leases: u16,
    },
    ProviderDirectoryResponse {
        dht_key: [u8; 32],
        signed_leases: Vec<Vec<u8>>,
        signed_retires: Vec<Vec<u8>>,
        sampled_coverage: bool,
    },
    RouteNeedSketchPacket {
        packet_tag_blake3: [u8; 32],
        bucket_padded_bytes: Vec<u8>,
        expires_at_unix_s: u64,
    },
    RouteNeedSketchReply {
        packet_tag_blake3: [u8; 32],
        sealed_reply_bytes: Vec<u8>,
    },
    DisclosureCapsuleFrame {
        session_id: [u8; 32],
        permit_cid: PermitCid,
        stage: DisclosureCapsuleStageWireV1,
        ephemeral_x25519_pk: [u8; 32],
        sealed_stage_bytes: Vec<u8>,
    },
    CapsuleCancel(CapsuleCancelV1),
    WatchSubscribe {
        selector_cid: SelectorCid,
        requested_ttl_secs: u32,
    },
    WatchRootHint(WatchRootHintV1),
}

impl DiscoveryWireMessageV1 {
    pub const fn grants_use_evidence_or_adoption(&self) -> bool {
        false
    }
}

// ─── 2. P6 Remote Cognition & D-013 Delegated Work Wire Family ───────────────

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(u64)]
pub enum RemoteTaskCancelReasonV1 {
    RequesterCancelled = 1,
    RuntimeGateDisabled = 2,
    PermitExpiredOrRevoked = 3,
    SomaticImmediateAbort = 4,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RemoteCognitiveRequestV1 {
    pub task_nonce: [u8; 32],
    pub offer_cid: ObjectCid,
    pub permit_cid: PermitCid,
    pub signed_permit_bytes: Vec<u8>,
    pub input_commitment_cid: ObjectCid,
    pub disclosure_capsule_bytes: Vec<u8>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RemoteTaskCancelV1 {
    pub task_nonce: [u8; 32],
    pub permit_cid: PermitCid,
    pub reason: RemoteTaskCancelReasonV1,
    pub requester_public_key: [u8; 32],
    pub requester_signature: [u8; 64],
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RemoteCognitiveResponseV1 {
    pub task_nonce: [u8; 32],
    pub permit_cid: PermitCid,
    pub manifest_cid: ObjectCid,
    pub input_commitment_cid: ObjectCid,
    pub output_commitment_cid: ObjectCid,
    pub execution_record_bytes: Vec<u8>,
    pub quarantined_output_capsule: Vec<u8>,
    pub is_executable: bool,
    pub cancelled: bool,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DelegatedJobLeaseWireV1 {
    pub job_id: [u8; 32],
    pub worker_node_id: NodeId,
    pub permit_cid: PermitCid,
    pub lease_ttl_ms: u32,
    pub issued_at_unix_ms: u64,
    pub requester_signature: [u8; 64],
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct BlindVerifyCommitWireV1 {
    pub job_id: [u8; 32],
    pub verifier_feed_id: FeedId,
    pub source_span_cid: ObjectCid,
    pub attempt_semantic_cid_salted_hash: [u8; 32],
    pub correlation_declaration_bytes: Vec<u8>,
    pub verifier_signature: [u8; 64],
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct BlindVerifyRevealWireV1 {
    pub job_id: [u8; 32],
    pub commitment_ack_digest: [u8; 32],
    pub reveal_salt: [u8; 32],
    pub encoding_attempt_bytes: Vec<u8>,
    pub fidelity_attestation_bytes: Vec<u8>,
    pub verifier_signature: [u8; 64],
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum CognitionWireMessageV1 {
    Request(RemoteCognitiveRequestV1),
    Cancel(RemoteTaskCancelV1),
    Response(RemoteCognitiveResponseV1),
    DelegatedJobLease(DelegatedJobLeaseWireV1),
    BlindVerifyCommit(BlindVerifyCommitWireV1),
    BlindVerifyReveal(BlindVerifyRevealWireV1),
}

impl CognitionWireMessageV1 {
    pub const fn grants_validated_store_authority(&self) -> bool {
        false
    }
}

// ─── 3. P5 OBT Ledger Wire Family (`obp/obt-ledger/1`, FW-07) ────────────────

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(u64)]
pub enum ObtRewardClaimKindWireV1 {
    AcceptedWorkD014 = 1,
    BenefitClosureM7 = 2,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(u64)]
pub enum ObtSettlementStateWireV1 {
    ProvisionalPartitionLocal = 1,
    FinalInDomain = 2,
    FrozenEquivocated = 3,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ObtLedgerWireMessageV1 {
    RewardClaimSubmit {
        claim_kind: ObtRewardClaimKindWireV1,
        scoped_nullifier: [u8; 32],
        reward_policy_cid: ObjectCid,
        amount_quanta: u128,
        canonical_claim_bytes: Vec<u8>,
    },
    PendingMintAuditVote {
        pending_mint_cid: [u8; 32],
        auditor_feed_id: FeedId,
        vrf_proof: Vec<u8>,
        verdict_approve: bool,
        opposing_evidence_cid: Option<EventCid>,
        signature: [u8; 64],
    },
    AccountBlockPush {
        actor_id: ActorId,
        sequence: u64,
        prev_block_cid: Option<[u8; 32]>,
        balance_after_quanta: u128,
        scoped_nullifier: Option<[u8; 32]>,
        settlement_state: ObtSettlementStateWireV1,
        canonical_block_bytes: Vec<u8>,
        signer_feed_id: FeedId,
        signature: [u8; 64],
    },
    AccountChainSyncRequest {
        actor_id: ActorId,
        from_sequence: u64,
        max_blocks: u16,
    },
    AccountEquivocationWarrant {
        actor_id: ActorId,
        sequence: u64,
        first_block_bytes: Vec<u8>,
        second_block_bytes: Vec<u8>,
    },
    SettlementDomainCheckpoint {
        domain_id: [u8; 32],
        epoch_sequence: u64,
        checkpoint_root: [u8; 32],
        canonical_checkpoint_bytes: Vec<u8>,
    },
}

impl ObtLedgerWireMessageV1 {
    pub const fn grants_knowledge_plane_authority(&self) -> bool {
        false
    }
}

// ─── 4. P10 Personal Mesh (`obp/personal-mesh/1`) & P7 `obp/obkg-fedr/1` ─────

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PersonalMeshEpochKeyWrapV1 {
    pub actor_id: ActorId,
    pub mesh_epoch: u64,
    pub target_device_id: DeviceId,
    pub ephemeral_x25519_pk: [u8; 32],
    pub wrapped_epoch_dek: Vec<u8>,
    pub sender_feed_id: FeedId,
    pub signature: [u8; 64],
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum PersonalMeshWireMessageV1 {
    PairInit {
        actor_id: ActorId,
        initiator_device_id: DeviceId,
        pake_message: Vec<u8>,
        session_nonce: [u8; 32],
    },
    PairConfirm {
        actor_id: ActorId,
        new_device_id: DeviceId,
        new_feed_id: FeedId,
        new_device_hpke_pubkey: [u8; 32],
        mesh_epoch: u64,
        sas_commitment: [u8; 32],
        signed_delegation_bytes: Vec<u8>,
    },
    EpochKeyWrap(PersonalMeshEpochKeyWrapV1),
    VaultEncryptedDelta {
        actor_id: ActorId,
        mesh_epoch: u64,
        record_id: [u8; 32],
        nonce: [u8; 24],
        ciphertext: Vec<u8>,
        sender_feed_id: FeedId,
        signature: [u8; 64],
    },
    DeviceRevocationEpochRotation {
        actor_id: ActorId,
        revoked_device_id: DeviceId,
        new_mesh_epoch: u64,
        signed_actor_revocation_bytes: Vec<u8>,
        wrapped_keys_for_remaining: Vec<PersonalMeshEpochKeyWrapV1>,
    },
    ObkgFedrMatrixExchange {
        registry_release_root: [u8; 32],
        embedding_space_spec_cid: ObjectCid,
        l2_clip_bound_bps: u32,
        matrix_payload_bytes: Vec<u8>,
        quarantine_only: bool,
    },
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum PillarWireEnvelopeV1 {
    Discovery(DiscoveryWireMessageV1),
    Cognition(CognitionWireMessageV1),
    ObtLedger(ObtLedgerWireMessageV1),
    PersonalMesh(PersonalMeshWireMessageV1),
}

// ─── Canonical CBOR Codecs (`<= 1 MiB` frame guard) ──────────────────────────

pub fn encode_discovery_wire(msg: &DiscoveryWireMessageV1) -> Result<Vec<u8>, PillarWireError> {
    encode_pillar_envelope(&PillarWireEnvelopeV1::Discovery(msg.clone()))
}

pub fn decode_discovery_wire(bytes: &[u8]) -> Result<DiscoveryWireMessageV1, PillarWireError> {
    match decode_pillar_envelope(bytes)? {
        PillarWireEnvelopeV1::Discovery(m) => Ok(m),
        _ => Err(PillarWireError::WrongFamily),
    }
}

pub fn encode_cognition_wire(msg: &CognitionWireMessageV1) -> Result<Vec<u8>, PillarWireError> {
    encode_pillar_envelope(&PillarWireEnvelopeV1::Cognition(msg.clone()))
}

pub fn decode_cognition_wire(bytes: &[u8]) -> Result<CognitionWireMessageV1, PillarWireError> {
    match decode_pillar_envelope(bytes)? {
        PillarWireEnvelopeV1::Cognition(m) => Ok(m),
        _ => Err(PillarWireError::WrongFamily),
    }
}

pub fn encode_obt_ledger_wire(msg: &ObtLedgerWireMessageV1) -> Result<Vec<u8>, PillarWireError> {
    encode_pillar_envelope(&PillarWireEnvelopeV1::ObtLedger(msg.clone()))
}

pub fn decode_obt_ledger_wire(bytes: &[u8]) -> Result<ObtLedgerWireMessageV1, PillarWireError> {
    match decode_pillar_envelope(bytes)? {
        PillarWireEnvelopeV1::ObtLedger(m) => Ok(m),
        _ => Err(PillarWireError::WrongFamily),
    }
}

pub fn encode_personal_mesh_wire(
    msg: &PersonalMeshWireMessageV1,
) -> Result<Vec<u8>, PillarWireError> {
    encode_pillar_envelope(&PillarWireEnvelopeV1::PersonalMesh(msg.clone()))
}

pub fn decode_personal_mesh_wire(
    bytes: &[u8],
) -> Result<PersonalMeshWireMessageV1, PillarWireError> {
    match decode_pillar_envelope(bytes)? {
        PillarWireEnvelopeV1::PersonalMesh(m) => Ok(m),
        _ => Err(PillarWireError::WrongFamily),
    }
}

pub fn encode_pillar_envelope(envelope: &PillarWireEnvelopeV1) -> Result<Vec<u8>, PillarWireError> {
    validate_envelope(envelope)?;
    let (family, variant, body) = match envelope {
        PillarWireEnvelopeV1::Discovery(msg) => {
            let (v, b) = discovery_value(msg);
            (pillar_family_id::DISCOVERY, v, b)
        }
        PillarWireEnvelopeV1::Cognition(msg) => {
            let (v, b) = cognition_value(msg);
            (pillar_family_id::COGNITION, v, b)
        }
        PillarWireEnvelopeV1::ObtLedger(msg) => {
            let (v, b) = obt_value(msg);
            (pillar_family_id::OBT_LEDGER, v, b)
        }
        PillarWireEnvelopeV1::PersonalMesh(msg) => {
            let (v, b) = personal_mesh_value(msg);
            (pillar_family_id::PERSONAL_MESH, v, b)
        }
    };
    let bytes = encode_canonical(
        &CanonicalValue::Map(vec![
            (0, CanonicalValue::Unsigned(PILLAR_WIRE_SCHEMA_MAJOR)),
            (1, CanonicalValue::Unsigned(family)),
            (2, CanonicalValue::Unsigned(variant)),
            (3, body),
        ]),
        ResourceProfile::ObjectV1,
    )?;
    if bytes.len() > MAX_PILLAR_WIRE_FRAME_BYTES {
        return Err(PillarWireError::FrameLimit);
    }
    Ok(bytes)
}

pub fn decode_pillar_envelope(bytes: &[u8]) -> Result<PillarWireEnvelopeV1, PillarWireError> {
    if bytes.is_empty() || bytes.len() > MAX_PILLAR_WIRE_FRAME_BYTES {
        return Err(PillarWireError::FrameLimit);
    }
    let root_val = decode_canonical(bytes, ResourceProfile::ObjectV1)?;
    let root = as_map(&root_val, "pillar.root")?;
    if unsigned(root, 0, "pillar.major")? != PILLAR_WIRE_SCHEMA_MAJOR {
        return Err(PillarWireError::UnsupportedMajor);
    }
    let family = unsigned(root, 1, "pillar.family")?;
    let variant = unsigned(root, 2, "pillar.variant")?;
    let body = as_map(required(root, 3, "pillar.body")?, "pillar.body")?;
    let parsed = match family {
        pillar_family_id::DISCOVERY => {
            PillarWireEnvelopeV1::Discovery(parse_discovery(variant, body)?)
        }
        pillar_family_id::COGNITION => {
            PillarWireEnvelopeV1::Cognition(parse_cognition(variant, body)?)
        }
        pillar_family_id::OBT_LEDGER => PillarWireEnvelopeV1::ObtLedger(parse_obt(variant, body)?),
        pillar_family_id::PERSONAL_MESH => {
            PillarWireEnvelopeV1::PersonalMesh(parse_personal_mesh(variant, body)?)
        }
        other => return Err(PillarWireError::UnknownFamily(other)),
    };
    validate_envelope(&parsed)?;
    if encode_pillar_envelope(&parsed)? != bytes {
        return Err(PillarWireError::NonCanonical);
    }
    Ok(parsed)
}

fn validate_envelope(envelope: &PillarWireEnvelopeV1) -> Result<(), PillarWireError> {
    match envelope {
        PillarWireEnvelopeV1::Discovery(msg) => match msg {
            DiscoveryWireMessageV1::ProviderLeasePublish {
                signed_lease_bytes, ..
            }
            | DiscoveryWireMessageV1::ProviderRetirePublish {
                signed_retire_bytes: signed_lease_bytes,
                ..
            } => {
                if signed_lease_bytes.is_empty() {
                    return Err(PillarWireError::InvalidField("discovery.empty_record"));
                }
            }
            DiscoveryWireMessageV1::ProviderDirectoryLookup { max_leases, .. } => {
                if *max_leases == 0 || *max_leases > 64 {
                    return Err(PillarWireError::ResourceLimit);
                }
            }
            DiscoveryWireMessageV1::ProviderDirectoryResponse {
                signed_leases,
                signed_retires,
                sampled_coverage,
                ..
            } => {
                if !sampled_coverage {
                    return Err(PillarWireError::InvalidField(
                        "discovery.sampled_coverage_required",
                    ));
                }
                if signed_leases.len() > 64 || signed_retires.len() > 64 {
                    return Err(PillarWireError::ResourceLimit);
                }
            }
            DiscoveryWireMessageV1::RouteNeedSketchPacket {
                bucket_padded_bytes,
                ..
            } => {
                if !ROUTE_NEED_SKETCH_BUCKET_SIZES.contains(&bucket_padded_bytes.len()) {
                    return Err(PillarWireError::InvalidField("discovery.sketch_bucket"));
                }
            }
            DiscoveryWireMessageV1::RouteNeedSketchReply {
                sealed_reply_bytes, ..
            } => {
                if sealed_reply_bytes.is_empty() {
                    return Err(PillarWireError::InvalidField("discovery.empty_reply"));
                }
            }
            DiscoveryWireMessageV1::DisclosureCapsuleFrame {
                session_id,
                sealed_stage_bytes,
                ..
            } => {
                if *session_id == [0; 32] || sealed_stage_bytes.is_empty() {
                    return Err(PillarWireError::InvalidField("discovery.capsule_frame"));
                }
            }
            DiscoveryWireMessageV1::CapsuleCancel(cancel) => {
                if cancel.session_id == [0; 32]
                    || cancel.signer_public_key == [0; 32]
                    || cancel.signer_proof == [0; 64]
                {
                    return Err(PillarWireError::InvalidField("discovery.capsule_cancel"));
                }
            }
            DiscoveryWireMessageV1::WatchSubscribe {
                requested_ttl_secs, ..
            } => {
                if *requested_ttl_secs == 0 || *requested_ttl_secs > MAX_WATCH_TTL_SECS {
                    return Err(PillarWireError::WatchTtlExceeded);
                }
            }
            DiscoveryWireMessageV1::WatchRootHint(hint) => {
                if hint.ttl_secs == 0 || hint.ttl_secs > MAX_WATCH_TTL_SECS {
                    return Err(PillarWireError::WatchTtlExceeded);
                }
            }
        },
        PillarWireEnvelopeV1::Cognition(msg) => match msg {
            CognitionWireMessageV1::Request(req) => {
                if req.task_nonce == [0; 32]
                    || req.signed_permit_bytes.is_empty()
                    || req.disclosure_capsule_bytes.is_empty()
                {
                    return Err(PillarWireError::InvalidField("cognition.request"));
                }
            }
            CognitionWireMessageV1::Cancel(cancel) => {
                if cancel.task_nonce == [0; 32]
                    || cancel.requester_public_key == [0; 32]
                    || cancel.requester_signature == [0; 64]
                {
                    return Err(PillarWireError::InvalidField("cognition.cancel"));
                }
            }
            CognitionWireMessageV1::Response(resp) => {
                if resp.is_executable {
                    return Err(PillarWireError::ExecutableRemoteOutputForbidden);
                }
                if resp.task_nonce == [0; 32] || resp.execution_record_bytes.is_empty() {
                    return Err(PillarWireError::InvalidField("cognition.response"));
                }
            }
            CognitionWireMessageV1::DelegatedJobLease(lease) => {
                if lease.job_id == [0; 32]
                    || lease.lease_ttl_ms == 0
                    || lease.lease_ttl_ms > MAX_DELEGATED_JOB_LEASE_TTL_MS
                    || lease.requester_signature == [0; 64]
                {
                    return Err(PillarWireError::InvalidField("cognition.job_lease"));
                }
            }
            CognitionWireMessageV1::BlindVerifyCommit(commit) => {
                if commit.job_id == [0; 32]
                    || commit.attempt_semantic_cid_salted_hash == [0; 32]
                    || commit.verifier_signature == [0; 64]
                {
                    return Err(PillarWireError::InvalidField("cognition.blind_commit"));
                }
            }
            CognitionWireMessageV1::BlindVerifyReveal(reveal) => {
                if reveal.job_id == [0; 32]
                    || reveal.commitment_ack_digest == [0; 32]
                    || reveal.encoding_attempt_bytes.is_empty()
                    || reveal.fidelity_attestation_bytes.is_empty()
                    || reveal.verifier_signature == [0; 64]
                {
                    return Err(PillarWireError::InvalidField("cognition.blind_reveal"));
                }
            }
        },
        PillarWireEnvelopeV1::ObtLedger(msg) => match msg {
            ObtLedgerWireMessageV1::RewardClaimSubmit {
                scoped_nullifier,
                amount_quanta,
                canonical_claim_bytes,
                ..
            } => {
                if *scoped_nullifier == [0; 32]
                    || *amount_quanta == 0
                    || canonical_claim_bytes.is_empty()
                {
                    return Err(PillarWireError::InvalidField("obt.claim"));
                }
            }
            ObtLedgerWireMessageV1::PendingMintAuditVote {
                pending_mint_cid,
                vrf_proof,
                signature,
                ..
            } => {
                if *pending_mint_cid == [0; 32] || vrf_proof.is_empty() || *signature == [0; 64] {
                    return Err(PillarWireError::InvalidField("obt.audit_vote"));
                }
            }
            ObtLedgerWireMessageV1::AccountBlockPush {
                canonical_block_bytes,
                signature,
                ..
            } => {
                if canonical_block_bytes.is_empty() || *signature == [0; 64] {
                    return Err(PillarWireError::InvalidField("obt.account_block"));
                }
            }
            ObtLedgerWireMessageV1::AccountChainSyncRequest { max_blocks, .. } => {
                if *max_blocks == 0 || *max_blocks > 512 {
                    return Err(PillarWireError::ResourceLimit);
                }
            }
            ObtLedgerWireMessageV1::AccountEquivocationWarrant {
                first_block_bytes,
                second_block_bytes,
                ..
            } => {
                if first_block_bytes.is_empty()
                    || second_block_bytes.is_empty()
                    || first_block_bytes == second_block_bytes
                {
                    return Err(PillarWireError::InvalidField("obt.warrant"));
                }
            }
            ObtLedgerWireMessageV1::SettlementDomainCheckpoint {
                domain_id,
                checkpoint_root,
                canonical_checkpoint_bytes,
                ..
            } => {
                if *domain_id == [0; 32]
                    || *checkpoint_root == [0; 32]
                    || canonical_checkpoint_bytes.is_empty()
                {
                    return Err(PillarWireError::InvalidField("obt.checkpoint"));
                }
            }
        },
        PillarWireEnvelopeV1::PersonalMesh(msg) => match msg {
            PersonalMeshWireMessageV1::PairInit {
                pake_message,
                session_nonce,
                ..
            } => {
                if pake_message.is_empty() || *session_nonce == [0; 32] {
                    return Err(PillarWireError::InvalidField("mesh.pair_init"));
                }
            }
            PersonalMeshWireMessageV1::PairConfirm {
                new_device_hpke_pubkey,
                sas_commitment,
                signed_delegation_bytes,
                ..
            } => {
                if *new_device_hpke_pubkey == [0; 32]
                    || *sas_commitment == [0; 32]
                    || signed_delegation_bytes.is_empty()
                {
                    return Err(PillarWireError::InvalidField("mesh.pair_confirm"));
                }
            }
            PersonalMeshWireMessageV1::EpochKeyWrap(wrap) => validate_key_wrap(wrap)?,
            PersonalMeshWireMessageV1::VaultEncryptedDelta {
                record_id,
                ciphertext,
                signature,
                ..
            } => {
                if *record_id == [0; 32] || ciphertext.is_empty() || *signature == [0; 64] {
                    return Err(PillarWireError::InvalidField("mesh.vault_delta"));
                }
            }
            PersonalMeshWireMessageV1::DeviceRevocationEpochRotation {
                new_mesh_epoch,
                signed_actor_revocation_bytes,
                wrapped_keys_for_remaining,
                ..
            } => {
                if *new_mesh_epoch == 0
                    || signed_actor_revocation_bytes.is_empty()
                    || wrapped_keys_for_remaining.len() > 64
                {
                    return Err(PillarWireError::InvalidField("mesh.revocation"));
                }
                for wrap in wrapped_keys_for_remaining {
                    validate_key_wrap(wrap)?;
                }
            }
            PersonalMeshWireMessageV1::ObkgFedrMatrixExchange {
                registry_release_root,
                l2_clip_bound_bps,
                matrix_payload_bytes,
                quarantine_only,
                ..
            } => {
                if !quarantine_only {
                    return Err(PillarWireError::InvalidField("fedr.quarantine_only"));
                }
                if *registry_release_root == [0; 32]
                    || *l2_clip_bound_bps == 0
                    || matrix_payload_bytes.is_empty()
                {
                    return Err(PillarWireError::InvalidField("fedr.matrix"));
                }
            }
        },
    }
    Ok(())
}

fn validate_key_wrap(wrap: &PersonalMeshEpochKeyWrapV1) -> Result<(), PillarWireError> {
    if wrap.ephemeral_x25519_pk == [0; 32]
        || wrap.wrapped_epoch_dek.is_empty()
        || wrap.signature == [0; 64]
    {
        return Err(PillarWireError::InvalidField("mesh.key_wrap"));
    }
    Ok(())
}

fn discovery_value(msg: &DiscoveryWireMessageV1) -> (u64, CanonicalValue) {
    match msg {
        DiscoveryWireMessageV1::ProviderLeasePublish {
            dht_key,
            lease_cid,
            signed_lease_bytes,
        } => (
            1,
            CanonicalValue::Map(vec![
                (0, b(dht_key)),
                (1, b(lease_cid.as_bytes())),
                (2, b(signed_lease_bytes)),
            ]),
        ),
        DiscoveryWireMessageV1::ProviderRetirePublish {
            dht_key,
            retire_cid,
            signed_retire_bytes,
        } => (
            2,
            CanonicalValue::Map(vec![
                (0, b(dht_key)),
                (1, b(retire_cid.as_bytes())),
                (2, b(signed_retire_bytes)),
            ]),
        ),
        DiscoveryWireMessageV1::ProviderDirectoryLookup {
            dht_key,
            selector_cid,
            max_leases,
        } => (
            3,
            CanonicalValue::Map(vec![
                (0, b(dht_key)),
                (1, b(selector_cid.as_bytes())),
                (2, u(u64::from(*max_leases))),
            ]),
        ),
        DiscoveryWireMessageV1::ProviderDirectoryResponse {
            dht_key,
            signed_leases,
            signed_retires,
            sampled_coverage,
        } => (
            4,
            CanonicalValue::Map(vec![
                (0, b(dht_key)),
                (
                    1,
                    CanonicalValue::Array(signed_leases.iter().map(|x| b(x)).collect()),
                ),
                (
                    2,
                    CanonicalValue::Array(signed_retires.iter().map(|x| b(x)).collect()),
                ),
                (3, CanonicalValue::Bool(*sampled_coverage)),
            ]),
        ),
        DiscoveryWireMessageV1::RouteNeedSketchPacket {
            packet_tag_blake3,
            bucket_padded_bytes,
            expires_at_unix_s,
        } => (
            5,
            CanonicalValue::Map(vec![
                (0, b(packet_tag_blake3)),
                (1, b(bucket_padded_bytes)),
                (2, u(*expires_at_unix_s)),
            ]),
        ),
        DiscoveryWireMessageV1::RouteNeedSketchReply {
            packet_tag_blake3,
            sealed_reply_bytes,
        } => (
            6,
            CanonicalValue::Map(vec![(0, b(packet_tag_blake3)), (1, b(sealed_reply_bytes))]),
        ),
        DiscoveryWireMessageV1::DisclosureCapsuleFrame {
            session_id,
            permit_cid,
            stage,
            ephemeral_x25519_pk,
            sealed_stage_bytes,
        } => (
            7,
            CanonicalValue::Map(vec![
                (0, b(session_id)),
                (1, b(permit_cid.as_bytes())),
                (2, u(*stage as u64)),
                (3, b(ephemeral_x25519_pk)),
                (4, b(sealed_stage_bytes)),
            ]),
        ),
        DiscoveryWireMessageV1::CapsuleCancel(c) => (
            8,
            CanonicalValue::Map(vec![
                (0, b(&c.session_id)),
                (1, b(c.permit_cid.as_bytes())),
                (2, u(c.reason as u64)),
                (3, b(&c.signer_public_key)),
                (4, b(&c.signer_proof)),
            ]),
        ),
        DiscoveryWireMessageV1::WatchSubscribe {
            selector_cid,
            requested_ttl_secs,
        } => (
            9,
            CanonicalValue::Map(vec![
                (0, b(selector_cid.as_bytes())),
                (1, u(u64::from(*requested_ttl_secs))),
            ]),
        ),
        DiscoveryWireMessageV1::WatchRootHint(hint) => (
            10,
            CanonicalValue::Map(vec![
                (0, b(hint.selector_cid.as_bytes())),
                (1, b(&hint.new_root)),
                (2, u(u64::from(hint.ttl_secs))),
            ]),
        ),
    }
}

fn parse_discovery(
    variant: u64,
    m: &[(u64, CanonicalValue)],
) -> Result<DiscoveryWireMessageV1, PillarWireError> {
    match variant {
        1 => Ok(DiscoveryWireMessageV1::ProviderLeasePublish {
            dht_key: bytes32(m, 0, "disc.dht_key")?,
            lease_cid: LeaseCid::from_bytes(bytes32(m, 1, "disc.lease_cid")?),
            signed_lease_bytes: byte_string(m, 2, "disc.lease_bytes")?.to_vec(),
        }),
        2 => Ok(DiscoveryWireMessageV1::ProviderRetirePublish {
            dht_key: bytes32(m, 0, "disc.dht_key")?,
            retire_cid: ObjectCid::from_bytes(bytes32(m, 1, "disc.retire_cid")?),
            signed_retire_bytes: byte_string(m, 2, "disc.retire_bytes")?.to_vec(),
        }),
        3 => Ok(DiscoveryWireMessageV1::ProviderDirectoryLookup {
            dht_key: bytes32(m, 0, "disc.dht_key")?,
            selector_cid: SelectorCid::from_bytes(bytes32(m, 1, "disc.selector")?),
            max_leases: u16_at(m, 2, "disc.max_leases")?,
        }),
        4 => Ok(DiscoveryWireMessageV1::ProviderDirectoryResponse {
            dht_key: bytes32(m, 0, "disc.dht_key")?,
            signed_leases: byte_vec_array(m, 1, "disc.leases")?,
            signed_retires: byte_vec_array(m, 2, "disc.retires")?,
            sampled_coverage: boolean(m, 3, "disc.sampled")?,
        }),
        5 => Ok(DiscoveryWireMessageV1::RouteNeedSketchPacket {
            packet_tag_blake3: bytes32(m, 0, "disc.tag")?,
            bucket_padded_bytes: byte_string(m, 1, "disc.sketch")?.to_vec(),
            expires_at_unix_s: unsigned(m, 2, "disc.expires")?,
        }),
        6 => Ok(DiscoveryWireMessageV1::RouteNeedSketchReply {
            packet_tag_blake3: bytes32(m, 0, "disc.tag")?,
            sealed_reply_bytes: byte_string(m, 1, "disc.reply")?.to_vec(),
        }),
        7 => {
            let stage = match unsigned(m, 2, "disc.stage")? {
                1 => DisclosureCapsuleStageWireV1::AffordanceSketch,
                2 => DisclosureCapsuleStageWireV1::ConstraintSketch,
                3 => DisclosureCapsuleStageWireV1::EvidenceReferences,
                4 => DisclosureCapsuleStageWireV1::FullNegotiatedPayload,
                _ => return Err(PillarWireError::InvalidField("disc.stage")),
            };
            Ok(DiscoveryWireMessageV1::DisclosureCapsuleFrame {
                session_id: bytes32(m, 0, "disc.session")?,
                permit_cid: PermitCid::from_bytes(bytes32(m, 1, "disc.permit")?),
                stage,
                ephemeral_x25519_pk: bytes32(m, 3, "disc.epk")?,
                sealed_stage_bytes: byte_string(m, 4, "disc.stage_bytes")?.to_vec(),
            })
        }
        8 => {
            let reason = match unsigned(m, 2, "disc.cancel_reason")? {
                1 => CapsuleCancelReasonV1::UserAborted,
                2 => CapsuleCancelReasonV1::PermitRevokedOrExpired,
                3 => CapsuleCancelReasonV1::PolicyViolation,
                4 => CapsuleCancelReasonV1::BudgetExhausted,
                _ => return Err(PillarWireError::InvalidField("disc.cancel_reason")),
            };
            Ok(DiscoveryWireMessageV1::CapsuleCancel(CapsuleCancelV1 {
                session_id: bytes32(m, 0, "disc.session")?,
                permit_cid: PermitCid::from_bytes(bytes32(m, 1, "disc.permit")?),
                reason,
                signer_public_key: bytes32(m, 3, "disc.signer_pk")?,
                signer_proof: bytes64(m, 4, "disc.signer_proof")?,
            }))
        }
        9 => Ok(DiscoveryWireMessageV1::WatchSubscribe {
            selector_cid: SelectorCid::from_bytes(bytes32(m, 0, "disc.selector")?),
            requested_ttl_secs: u32_at(m, 1, "disc.ttl")?,
        }),
        10 => Ok(DiscoveryWireMessageV1::WatchRootHint(WatchRootHintV1 {
            selector_cid: SelectorCid::from_bytes(bytes32(m, 0, "disc.selector")?),
            new_root: bytes32(m, 1, "disc.new_root")?,
            ttl_secs: u32_at(m, 2, "disc.ttl")?,
        })),
        other => Err(PillarWireError::UnknownVariant(other)),
    }
}

fn cognition_value(msg: &CognitionWireMessageV1) -> (u64, CanonicalValue) {
    match msg {
        CognitionWireMessageV1::Request(r) => (
            1,
            CanonicalValue::Map(vec![
                (0, b(&r.task_nonce)),
                (1, b(r.offer_cid.as_bytes())),
                (2, b(r.permit_cid.as_bytes())),
                (3, b(&r.signed_permit_bytes)),
                (4, b(r.input_commitment_cid.as_bytes())),
                (5, b(&r.disclosure_capsule_bytes)),
            ]),
        ),
        CognitionWireMessageV1::Cancel(c) => (
            2,
            CanonicalValue::Map(vec![
                (0, b(&c.task_nonce)),
                (1, b(c.permit_cid.as_bytes())),
                (2, u(c.reason as u64)),
                (3, b(&c.requester_public_key)),
                (4, b(&c.requester_signature)),
            ]),
        ),
        CognitionWireMessageV1::Response(r) => (
            3,
            CanonicalValue::Map(vec![
                (0, b(&r.task_nonce)),
                (1, b(r.permit_cid.as_bytes())),
                (2, b(r.manifest_cid.as_bytes())),
                (3, b(r.input_commitment_cid.as_bytes())),
                (4, b(r.output_commitment_cid.as_bytes())),
                (5, b(&r.execution_record_bytes)),
                (6, b(&r.quarantined_output_capsule)),
                (7, CanonicalValue::Bool(r.is_executable)),
                (8, CanonicalValue::Bool(r.cancelled)),
            ]),
        ),
        CognitionWireMessageV1::DelegatedJobLease(l) => (
            4,
            CanonicalValue::Map(vec![
                (0, b(&l.job_id)),
                (1, b(l.worker_node_id.as_bytes())),
                (2, b(l.permit_cid.as_bytes())),
                (3, u(u64::from(l.lease_ttl_ms))),
                (4, u(l.issued_at_unix_ms)),
                (5, b(&l.requester_signature)),
            ]),
        ),
        CognitionWireMessageV1::BlindVerifyCommit(c) => (
            5,
            CanonicalValue::Map(vec![
                (0, b(&c.job_id)),
                (1, b(c.verifier_feed_id.as_bytes())),
                (2, b(c.source_span_cid.as_bytes())),
                (3, b(&c.attempt_semantic_cid_salted_hash)),
                (4, b(&c.correlation_declaration_bytes)),
                (5, b(&c.verifier_signature)),
            ]),
        ),
        CognitionWireMessageV1::BlindVerifyReveal(r) => (
            6,
            CanonicalValue::Map(vec![
                (0, b(&r.job_id)),
                (1, b(&r.commitment_ack_digest)),
                (2, b(&r.reveal_salt)),
                (3, b(&r.encoding_attempt_bytes)),
                (4, b(&r.fidelity_attestation_bytes)),
                (5, b(&r.verifier_signature)),
            ]),
        ),
    }
}

fn parse_cognition(
    variant: u64,
    m: &[(u64, CanonicalValue)],
) -> Result<CognitionWireMessageV1, PillarWireError> {
    match variant {
        1 => Ok(CognitionWireMessageV1::Request(RemoteCognitiveRequestV1 {
            task_nonce: bytes32(m, 0, "cog.nonce")?,
            offer_cid: ObjectCid::from_bytes(bytes32(m, 1, "cog.offer")?),
            permit_cid: PermitCid::from_bytes(bytes32(m, 2, "cog.permit")?),
            signed_permit_bytes: byte_string(m, 3, "cog.permit_bytes")?.to_vec(),
            input_commitment_cid: ObjectCid::from_bytes(bytes32(m, 4, "cog.input")?),
            disclosure_capsule_bytes: byte_string(m, 5, "cog.capsule")?.to_vec(),
        })),
        2 => {
            let reason = match unsigned(m, 2, "cog.cancel_reason")? {
                1 => RemoteTaskCancelReasonV1::RequesterCancelled,
                2 => RemoteTaskCancelReasonV1::RuntimeGateDisabled,
                3 => RemoteTaskCancelReasonV1::PermitExpiredOrRevoked,
                4 => RemoteTaskCancelReasonV1::SomaticImmediateAbort,
                _ => return Err(PillarWireError::InvalidField("cog.cancel_reason")),
            };
            Ok(CognitionWireMessageV1::Cancel(RemoteTaskCancelV1 {
                task_nonce: bytes32(m, 0, "cog.nonce")?,
                permit_cid: PermitCid::from_bytes(bytes32(m, 1, "cog.permit")?),
                reason,
                requester_public_key: bytes32(m, 3, "cog.req_pk")?,
                requester_signature: bytes64(m, 4, "cog.req_sig")?,
            }))
        }
        3 => Ok(CognitionWireMessageV1::Response(
            RemoteCognitiveResponseV1 {
                task_nonce: bytes32(m, 0, "cog.nonce")?,
                permit_cid: PermitCid::from_bytes(bytes32(m, 1, "cog.permit")?),
                manifest_cid: ObjectCid::from_bytes(bytes32(m, 2, "cog.manifest")?),
                input_commitment_cid: ObjectCid::from_bytes(bytes32(m, 3, "cog.input")?),
                output_commitment_cid: ObjectCid::from_bytes(bytes32(m, 4, "cog.output")?),
                execution_record_bytes: byte_string(m, 5, "cog.exec_record")?.to_vec(),
                quarantined_output_capsule: byte_string(m, 6, "cog.out_capsule")?.to_vec(),
                is_executable: boolean(m, 7, "cog.exec_flag")?,
                cancelled: boolean(m, 8, "cog.cancelled")?,
            },
        )),
        4 => Ok(CognitionWireMessageV1::DelegatedJobLease(
            DelegatedJobLeaseWireV1 {
                job_id: bytes32(m, 0, "cog.job_id")?,
                worker_node_id: NodeId::from_bytes(bytes32(m, 1, "cog.worker")?),
                permit_cid: PermitCid::from_bytes(bytes32(m, 2, "cog.permit")?),
                lease_ttl_ms: u32_at(m, 3, "cog.ttl_ms")?,
                issued_at_unix_ms: unsigned(m, 4, "cog.issued_ms")?,
                requester_signature: bytes64(m, 5, "cog.sig")?,
            },
        )),
        5 => Ok(CognitionWireMessageV1::BlindVerifyCommit(
            BlindVerifyCommitWireV1 {
                job_id: bytes32(m, 0, "cog.job_id")?,
                verifier_feed_id: FeedId::from_bytes(bytes32(m, 1, "cog.feed")?),
                source_span_cid: ObjectCid::from_bytes(bytes32(m, 2, "cog.span")?),
                attempt_semantic_cid_salted_hash: bytes32(m, 3, "cog.hash")?,
                correlation_declaration_bytes: byte_string(m, 4, "cog.corr")?.to_vec(),
                verifier_signature: bytes64(m, 5, "cog.sig")?,
            },
        )),
        6 => Ok(CognitionWireMessageV1::BlindVerifyReveal(
            BlindVerifyRevealWireV1 {
                job_id: bytes32(m, 0, "cog.job_id")?,
                commitment_ack_digest: bytes32(m, 1, "cog.ack")?,
                reveal_salt: bytes32(m, 2, "cog.salt")?,
                encoding_attempt_bytes: byte_string(m, 3, "cog.attempt")?.to_vec(),
                fidelity_attestation_bytes: byte_string(m, 4, "cog.attestation")?.to_vec(),
                verifier_signature: bytes64(m, 5, "cog.sig")?,
            },
        )),
        other => Err(PillarWireError::UnknownVariant(other)),
    }
}

fn obt_value(msg: &ObtLedgerWireMessageV1) -> (u64, CanonicalValue) {
    match msg {
        ObtLedgerWireMessageV1::RewardClaimSubmit {
            claim_kind,
            scoped_nullifier,
            reward_policy_cid,
            amount_quanta,
            canonical_claim_bytes,
        } => (
            1,
            CanonicalValue::Map(vec![
                (0, u(*claim_kind as u64)),
                (1, b(scoped_nullifier)),
                (2, b(reward_policy_cid.as_bytes())),
                (3, b(&amount_quanta.to_be_bytes())),
                (4, b(canonical_claim_bytes)),
            ]),
        ),
        ObtLedgerWireMessageV1::PendingMintAuditVote {
            pending_mint_cid,
            auditor_feed_id,
            vrf_proof,
            verdict_approve,
            opposing_evidence_cid,
            signature,
        } => (
            2,
            CanonicalValue::Map(vec![
                (0, b(pending_mint_cid)),
                (1, b(auditor_feed_id.as_bytes())),
                (2, b(vrf_proof)),
                (3, CanonicalValue::Bool(*verdict_approve)),
                (
                    4,
                    opposing_evidence_cid.map_or(CanonicalValue::Null, |c| b(c.as_bytes())),
                ),
                (5, b(signature)),
            ]),
        ),
        ObtLedgerWireMessageV1::AccountBlockPush {
            actor_id,
            sequence,
            prev_block_cid,
            balance_after_quanta,
            scoped_nullifier,
            settlement_state,
            canonical_block_bytes,
            signer_feed_id,
            signature,
        } => (
            3,
            CanonicalValue::Map(vec![
                (0, b(actor_id.as_bytes())),
                (1, u(*sequence)),
                (2, opt_b32(*prev_block_cid)),
                (3, b(&balance_after_quanta.to_be_bytes())),
                (4, opt_b32(*scoped_nullifier)),
                (5, u(*settlement_state as u64)),
                (6, b(canonical_block_bytes)),
                (7, b(signer_feed_id.as_bytes())),
                (8, b(signature)),
            ]),
        ),
        ObtLedgerWireMessageV1::AccountChainSyncRequest {
            actor_id,
            from_sequence,
            max_blocks,
        } => (
            4,
            CanonicalValue::Map(vec![
                (0, b(actor_id.as_bytes())),
                (1, u(*from_sequence)),
                (2, u(u64::from(*max_blocks))),
            ]),
        ),
        ObtLedgerWireMessageV1::AccountEquivocationWarrant {
            actor_id,
            sequence,
            first_block_bytes,
            second_block_bytes,
        } => (
            5,
            CanonicalValue::Map(vec![
                (0, b(actor_id.as_bytes())),
                (1, u(*sequence)),
                (2, b(first_block_bytes)),
                (3, b(second_block_bytes)),
            ]),
        ),
        ObtLedgerWireMessageV1::SettlementDomainCheckpoint {
            domain_id,
            epoch_sequence,
            checkpoint_root,
            canonical_checkpoint_bytes,
        } => (
            6,
            CanonicalValue::Map(vec![
                (0, b(domain_id)),
                (1, u(*epoch_sequence)),
                (2, b(checkpoint_root)),
                (3, b(canonical_checkpoint_bytes)),
            ]),
        ),
    }
}

fn parse_obt(
    variant: u64,
    m: &[(u64, CanonicalValue)],
) -> Result<ObtLedgerWireMessageV1, PillarWireError> {
    match variant {
        1 => {
            let claim_kind = match unsigned(m, 0, "obt.claim_kind")? {
                1 => ObtRewardClaimKindWireV1::AcceptedWorkD014,
                2 => ObtRewardClaimKindWireV1::BenefitClosureM7,
                _ => return Err(PillarWireError::InvalidField("obt.claim_kind")),
            };
            Ok(ObtLedgerWireMessageV1::RewardClaimSubmit {
                claim_kind,
                scoped_nullifier: bytes32(m, 1, "obt.nullifier")?,
                reward_policy_cid: ObjectCid::from_bytes(bytes32(m, 2, "obt.policy")?),
                amount_quanta: u128_at(m, 3, "obt.amount")?,
                canonical_claim_bytes: byte_string(m, 4, "obt.claim_bytes")?.to_vec(),
            })
        }
        2 => Ok(ObtLedgerWireMessageV1::PendingMintAuditVote {
            pending_mint_cid: bytes32(m, 0, "obt.pending_cid")?,
            auditor_feed_id: FeedId::from_bytes(bytes32(m, 1, "obt.feed")?),
            vrf_proof: byte_string(m, 2, "obt.vrf")?.to_vec(),
            verdict_approve: boolean(m, 3, "obt.verdict")?,
            opposing_evidence_cid: optional_bytes32(
                required(m, 4, "obt.opposing")?,
                "obt.opposing",
            )?
            .map(EventCid::from_bytes),
            signature: bytes64(m, 5, "obt.sig")?,
        }),
        3 => {
            let settlement_state = match unsigned(m, 5, "obt.settlement")? {
                1 => ObtSettlementStateWireV1::ProvisionalPartitionLocal,
                2 => ObtSettlementStateWireV1::FinalInDomain,
                3 => ObtSettlementStateWireV1::FrozenEquivocated,
                _ => return Err(PillarWireError::InvalidField("obt.settlement")),
            };
            Ok(ObtLedgerWireMessageV1::AccountBlockPush {
                actor_id: ActorId::from_bytes(bytes32(m, 0, "obt.actor")?),
                sequence: unsigned(m, 1, "obt.seq")?,
                prev_block_cid: optional_bytes32(required(m, 2, "obt.prev")?, "obt.prev")?,
                balance_after_quanta: u128_at(m, 3, "obt.balance")?,
                scoped_nullifier: optional_bytes32(
                    required(m, 4, "obt.nullifier")?,
                    "obt.nullifier",
                )?,
                settlement_state,
                canonical_block_bytes: byte_string(m, 6, "obt.block")?.to_vec(),
                signer_feed_id: FeedId::from_bytes(bytes32(m, 7, "obt.feed")?),
                signature: bytes64(m, 8, "obt.sig")?,
            })
        }
        4 => Ok(ObtLedgerWireMessageV1::AccountChainSyncRequest {
            actor_id: ActorId::from_bytes(bytes32(m, 0, "obt.actor")?),
            from_sequence: unsigned(m, 1, "obt.from_seq")?,
            max_blocks: u16_at(m, 2, "obt.max_blocks")?,
        }),
        5 => Ok(ObtLedgerWireMessageV1::AccountEquivocationWarrant {
            actor_id: ActorId::from_bytes(bytes32(m, 0, "obt.actor")?),
            sequence: unsigned(m, 1, "obt.seq")?,
            first_block_bytes: byte_string(m, 2, "obt.first")?.to_vec(),
            second_block_bytes: byte_string(m, 3, "obt.second")?.to_vec(),
        }),
        6 => Ok(ObtLedgerWireMessageV1::SettlementDomainCheckpoint {
            domain_id: bytes32(m, 0, "obt.domain")?,
            epoch_sequence: unsigned(m, 1, "obt.epoch")?,
            checkpoint_root: bytes32(m, 2, "obt.root")?,
            canonical_checkpoint_bytes: byte_string(m, 3, "obt.checkpoint")?.to_vec(),
        }),
        other => Err(PillarWireError::UnknownVariant(other)),
    }
}

fn personal_mesh_value(msg: &PersonalMeshWireMessageV1) -> (u64, CanonicalValue) {
    match msg {
        PersonalMeshWireMessageV1::PairInit {
            actor_id,
            initiator_device_id,
            pake_message,
            session_nonce,
        } => (
            1,
            CanonicalValue::Map(vec![
                (0, b(actor_id.as_bytes())),
                (1, b(initiator_device_id.as_bytes())),
                (2, b(pake_message)),
                (3, b(session_nonce)),
            ]),
        ),
        PersonalMeshWireMessageV1::PairConfirm {
            actor_id,
            new_device_id,
            new_feed_id,
            new_device_hpke_pubkey,
            mesh_epoch,
            sas_commitment,
            signed_delegation_bytes,
        } => (
            2,
            CanonicalValue::Map(vec![
                (0, b(actor_id.as_bytes())),
                (1, b(new_device_id.as_bytes())),
                (2, b(new_feed_id.as_bytes())),
                (3, b(new_device_hpke_pubkey)),
                (4, u(*mesh_epoch)),
                (5, b(sas_commitment)),
                (6, b(signed_delegation_bytes)),
            ]),
        ),
        PersonalMeshWireMessageV1::EpochKeyWrap(w) => (3, key_wrap_value(w)),
        PersonalMeshWireMessageV1::VaultEncryptedDelta {
            actor_id,
            mesh_epoch,
            record_id,
            nonce,
            ciphertext,
            sender_feed_id,
            signature,
        } => (
            4,
            CanonicalValue::Map(vec![
                (0, b(actor_id.as_bytes())),
                (1, u(*mesh_epoch)),
                (2, b(record_id)),
                (3, b(nonce)),
                (4, b(ciphertext)),
                (5, b(sender_feed_id.as_bytes())),
                (6, b(signature)),
            ]),
        ),
        PersonalMeshWireMessageV1::DeviceRevocationEpochRotation {
            actor_id,
            revoked_device_id,
            new_mesh_epoch,
            signed_actor_revocation_bytes,
            wrapped_keys_for_remaining,
        } => (
            5,
            CanonicalValue::Map(vec![
                (0, b(actor_id.as_bytes())),
                (1, b(revoked_device_id.as_bytes())),
                (2, u(*new_mesh_epoch)),
                (3, b(signed_actor_revocation_bytes)),
                (
                    4,
                    CanonicalValue::Array(
                        wrapped_keys_for_remaining
                            .iter()
                            .map(key_wrap_value)
                            .collect(),
                    ),
                ),
            ]),
        ),
        PersonalMeshWireMessageV1::ObkgFedrMatrixExchange {
            registry_release_root,
            embedding_space_spec_cid,
            l2_clip_bound_bps,
            matrix_payload_bytes,
            quarantine_only,
        } => (
            6,
            CanonicalValue::Map(vec![
                (0, b(registry_release_root)),
                (1, b(embedding_space_spec_cid.as_bytes())),
                (2, u(u64::from(*l2_clip_bound_bps))),
                (3, b(matrix_payload_bytes)),
                (4, CanonicalValue::Bool(*quarantine_only)),
            ]),
        ),
    }
}

fn key_wrap_value(w: &PersonalMeshEpochKeyWrapV1) -> CanonicalValue {
    CanonicalValue::Map(vec![
        (0, b(w.actor_id.as_bytes())),
        (1, u(w.mesh_epoch)),
        (2, b(w.target_device_id.as_bytes())),
        (3, b(&w.ephemeral_x25519_pk)),
        (4, b(&w.wrapped_epoch_dek)),
        (5, b(w.sender_feed_id.as_bytes())),
        (6, b(&w.signature)),
    ])
}

fn parse_key_wrap(
    m: &[(u64, CanonicalValue)],
) -> Result<PersonalMeshEpochKeyWrapV1, PillarWireError> {
    Ok(PersonalMeshEpochKeyWrapV1 {
        actor_id: ActorId::from_bytes(bytes32(m, 0, "mesh.actor")?),
        mesh_epoch: unsigned(m, 1, "mesh.epoch")?,
        target_device_id: DeviceId::from_bytes(bytes32(m, 2, "mesh.target_dev")?),
        ephemeral_x25519_pk: bytes32(m, 3, "mesh.epk")?,
        wrapped_epoch_dek: byte_string(m, 4, "mesh.dek")?.to_vec(),
        sender_feed_id: FeedId::from_bytes(bytes32(m, 5, "mesh.feed")?),
        signature: bytes64(m, 6, "mesh.sig")?,
    })
}

fn parse_personal_mesh(
    variant: u64,
    m: &[(u64, CanonicalValue)],
) -> Result<PersonalMeshWireMessageV1, PillarWireError> {
    match variant {
        1 => Ok(PersonalMeshWireMessageV1::PairInit {
            actor_id: ActorId::from_bytes(bytes32(m, 0, "mesh.actor")?),
            initiator_device_id: DeviceId::from_bytes(bytes32(m, 1, "mesh.dev")?),
            pake_message: byte_string(m, 2, "mesh.pake")?.to_vec(),
            session_nonce: bytes32(m, 3, "mesh.nonce")?,
        }),
        2 => Ok(PersonalMeshWireMessageV1::PairConfirm {
            actor_id: ActorId::from_bytes(bytes32(m, 0, "mesh.actor")?),
            new_device_id: DeviceId::from_bytes(bytes32(m, 1, "mesh.dev")?),
            new_feed_id: FeedId::from_bytes(bytes32(m, 2, "mesh.feed")?),
            new_device_hpke_pubkey: bytes32(m, 3, "mesh.hpke")?,
            mesh_epoch: unsigned(m, 4, "mesh.epoch")?,
            sas_commitment: bytes32(m, 5, "mesh.sas")?,
            signed_delegation_bytes: byte_string(m, 6, "mesh.delegation")?.to_vec(),
        }),
        3 => Ok(PersonalMeshWireMessageV1::EpochKeyWrap(parse_key_wrap(m)?)),
        4 => Ok(PersonalMeshWireMessageV1::VaultEncryptedDelta {
            actor_id: ActorId::from_bytes(bytes32(m, 0, "mesh.actor")?),
            mesh_epoch: unsigned(m, 1, "mesh.epoch")?,
            record_id: bytes32(m, 2, "mesh.record")?,
            nonce: bytes24(m, 3, "mesh.nonce")?,
            ciphertext: byte_string(m, 4, "mesh.ciphertext")?.to_vec(),
            sender_feed_id: FeedId::from_bytes(bytes32(m, 5, "mesh.feed")?),
            signature: bytes64(m, 6, "mesh.sig")?,
        }),
        5 => {
            let wraps = array(m, 4, "mesh.wraps")?
                .iter()
                .map(|v| parse_key_wrap(as_map(v, "mesh.wrap")?))
                .collect::<Result<Vec<_>, _>>()?;
            Ok(PersonalMeshWireMessageV1::DeviceRevocationEpochRotation {
                actor_id: ActorId::from_bytes(bytes32(m, 0, "mesh.actor")?),
                revoked_device_id: DeviceId::from_bytes(bytes32(m, 1, "mesh.revoked")?),
                new_mesh_epoch: unsigned(m, 2, "mesh.new_epoch")?,
                signed_actor_revocation_bytes: byte_string(m, 3, "mesh.revocation")?.to_vec(),
                wrapped_keys_for_remaining: wraps,
            })
        }
        6 => Ok(PersonalMeshWireMessageV1::ObkgFedrMatrixExchange {
            registry_release_root: bytes32(m, 0, "fedr.reg_root")?,
            embedding_space_spec_cid: ObjectCid::from_bytes(bytes32(m, 1, "fedr.spec")?),
            l2_clip_bound_bps: u32_at(m, 2, "fedr.clip")?,
            matrix_payload_bytes: byte_string(m, 3, "fedr.matrix")?.to_vec(),
            quarantine_only: boolean(m, 4, "fedr.quarantine")?,
        }),
        other => Err(PillarWireError::UnknownVariant(other)),
    }
}

fn u(v: u64) -> CanonicalValue {
    CanonicalValue::Unsigned(v)
}

fn b(v: &[u8]) -> CanonicalValue {
    CanonicalValue::Bytes(v.to_vec())
}

fn opt_b32(v: Option<[u8; 32]>) -> CanonicalValue {
    v.map_or(CanonicalValue::Null, |arr| {
        CanonicalValue::Bytes(arr.to_vec())
    })
}

fn as_map<'a>(
    v: &'a CanonicalValue,
    field: &'static str,
) -> Result<&'a [(u64, CanonicalValue)], PillarWireError> {
    match v {
        CanonicalValue::Map(m) => Ok(m),
        _ => Err(PillarWireError::InvalidField(field)),
    }
}

fn required<'a>(
    m: &'a [(u64, CanonicalValue)],
    key: u64,
    field: &'static str,
) -> Result<&'a CanonicalValue, PillarWireError> {
    m.iter()
        .find_map(|(k, v)| (*k == key).then_some(v))
        .ok_or(PillarWireError::InvalidField(field))
}

fn unsigned(
    m: &[(u64, CanonicalValue)],
    key: u64,
    field: &'static str,
) -> Result<u64, PillarWireError> {
    match required(m, key, field)? {
        CanonicalValue::Unsigned(v) => Ok(*v),
        _ => Err(PillarWireError::InvalidField(field)),
    }
}

fn u16_at(
    m: &[(u64, CanonicalValue)],
    key: u64,
    field: &'static str,
) -> Result<u16, PillarWireError> {
    u16::try_from(unsigned(m, key, field)?).map_err(|_| PillarWireError::ResourceLimit)
}

fn u32_at(
    m: &[(u64, CanonicalValue)],
    key: u64,
    field: &'static str,
) -> Result<u32, PillarWireError> {
    u32::try_from(unsigned(m, key, field)?).map_err(|_| PillarWireError::ResourceLimit)
}

fn u128_at(
    m: &[(u64, CanonicalValue)],
    key: u64,
    field: &'static str,
) -> Result<u128, PillarWireError> {
    let bytes = byte_string(m, key, field)?;
    let arr: [u8; 16] = bytes
        .try_into()
        .map_err(|_| PillarWireError::InvalidField(field))?;
    Ok(u128::from_be_bytes(arr))
}

fn boolean(
    m: &[(u64, CanonicalValue)],
    key: u64,
    field: &'static str,
) -> Result<bool, PillarWireError> {
    match required(m, key, field)? {
        CanonicalValue::Bool(v) => Ok(*v),
        _ => Err(PillarWireError::InvalidField(field)),
    }
}

fn array<'a>(
    m: &'a [(u64, CanonicalValue)],
    key: u64,
    field: &'static str,
) -> Result<&'a [CanonicalValue], PillarWireError> {
    match required(m, key, field)? {
        CanonicalValue::Array(v) => Ok(v),
        _ => Err(PillarWireError::InvalidField(field)),
    }
}

fn byte_vec_array(
    m: &[(u64, CanonicalValue)],
    key: u64,
    field: &'static str,
) -> Result<Vec<Vec<u8>>, PillarWireError> {
    array(m, key, field)?
        .iter()
        .map(|v| match v {
            CanonicalValue::Bytes(b) => Ok(b.clone()),
            _ => Err(PillarWireError::InvalidField(field)),
        })
        .collect()
}

fn byte_string<'a>(
    m: &'a [(u64, CanonicalValue)],
    key: u64,
    field: &'static str,
) -> Result<&'a [u8], PillarWireError> {
    match required(m, key, field)? {
        CanonicalValue::Bytes(b) => Ok(b),
        _ => Err(PillarWireError::InvalidField(field)),
    }
}

fn bytes24(
    m: &[(u64, CanonicalValue)],
    key: u64,
    field: &'static str,
) -> Result<[u8; 24], PillarWireError> {
    let b = byte_string(m, key, field)?;
    b.try_into()
        .map_err(|_| PillarWireError::InvalidField(field))
}

fn bytes32(
    m: &[(u64, CanonicalValue)],
    key: u64,
    field: &'static str,
) -> Result<[u8; 32], PillarWireError> {
    let b = byte_string(m, key, field)?;
    b.try_into()
        .map_err(|_| PillarWireError::InvalidField(field))
}

fn bytes64(
    m: &[(u64, CanonicalValue)],
    key: u64,
    field: &'static str,
) -> Result<[u8; 64], PillarWireError> {
    let b = byte_string(m, key, field)?;
    b.try_into()
        .map_err(|_| PillarWireError::InvalidField(field))
}

fn optional_bytes32(
    v: &CanonicalValue,
    field: &'static str,
) -> Result<Option<[u8; 32]>, PillarWireError> {
    match v {
        CanonicalValue::Null => Ok(None),
        CanonicalValue::Bytes(b) if b.len() == 32 => {
            let mut out = [0u8; 32];
            out.copy_from_slice(b);
            Ok(Some(out))
        }
        _ => Err(PillarWireError::InvalidField(field)),
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum PillarWireError {
    Canonical(CanonicalError),
    UnsupportedMajor,
    UnknownFamily(u64),
    UnknownVariant(u64),
    WrongFamily,
    InvalidField(&'static str),
    WatchTtlExceeded,
    ExecutableRemoteOutputForbidden,
    ResourceLimit,
    FrameLimit,
    NonCanonical,
}

impl From<CanonicalError> for PillarWireError {
    fn from(err: CanonicalError) -> Self {
        Self::Canonical(err)
    }
}

impl std::fmt::Display for PillarWireError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "OBP_PILLAR_WIRE: {self:?}")
    }
}

impl std::error::Error for PillarWireError {}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn all_four_pillar_wire_families_round_trip_and_enforce_firewalls() {
        let hint = DiscoveryWireMessageV1::WatchRootHint(WatchRootHintV1 {
            selector_cid: SelectorCid::from_bytes([1; 32]),
            new_root: [2; 32],
            ttl_secs: 3_600,
        });
        assert_eq!(
            decode_discovery_wire(&encode_discovery_wire(&hint).unwrap()).unwrap(),
            hint
        );
        let over_ttl = DiscoveryWireMessageV1::WatchRootHint(WatchRootHintV1 {
            selector_cid: SelectorCid::from_bytes([1; 32]),
            new_root: [2; 32],
            ttl_secs: 3_601,
        });
        assert_eq!(
            encode_discovery_wire(&over_ttl).unwrap_err(),
            PillarWireError::WatchTtlExceeded
        );

        let cancel = DiscoveryWireMessageV1::CapsuleCancel(CapsuleCancelV1 {
            session_id: [3; 32],
            permit_cid: PermitCid::from_bytes([4; 32]),
            reason: CapsuleCancelReasonV1::UserAborted,
            signer_public_key: [5; 32],
            signer_proof: [6; 64],
        });
        assert_eq!(
            decode_discovery_wire(&encode_discovery_wire(&cancel).unwrap()).unwrap(),
            cancel
        );

        let mut cog_resp = RemoteCognitiveResponseV1 {
            task_nonce: [7; 32],
            permit_cid: PermitCid::from_bytes([8; 32]),
            manifest_cid: ObjectCid::from_bytes([9; 32]),
            input_commitment_cid: ObjectCid::from_bytes([10; 32]),
            output_commitment_cid: ObjectCid::from_bytes([11; 32]),
            execution_record_bytes: vec![0xa1],
            quarantined_output_capsule: vec![0xb2],
            is_executable: true,
            cancelled: false,
        };
        assert_eq!(
            encode_cognition_wire(&CognitionWireMessageV1::Response(cog_resp.clone())).unwrap_err(),
            PillarWireError::ExecutableRemoteOutputForbidden
        );
        cog_resp.is_executable = false;
        let cog_msg = CognitionWireMessageV1::Response(cog_resp);
        assert_eq!(
            decode_cognition_wire(&encode_cognition_wire(&cog_msg).unwrap()).unwrap(),
            cog_msg
        );

        let obt = ObtLedgerWireMessageV1::RewardClaimSubmit {
            claim_kind: ObtRewardClaimKindWireV1::AcceptedWorkD014,
            scoped_nullifier: [12; 32],
            reward_policy_cid: ObjectCid::from_bytes([13; 32]),
            amount_quanta: 1_000_000_000_000u128,
            canonical_claim_bytes: vec![1, 2, 3],
        };
        assert!(!obt.grants_knowledge_plane_authority());
        assert_eq!(
            decode_obt_ledger_wire(&encode_obt_ledger_wire(&obt).unwrap()).unwrap(),
            obt
        );

        let mesh = PersonalMeshWireMessageV1::ObkgFedrMatrixExchange {
            registry_release_root: [14; 32],
            embedding_space_spec_cid: ObjectCid::from_bytes([15; 32]),
            l2_clip_bound_bps: 1_000,
            matrix_payload_bytes: vec![9, 9, 9],
            quarantine_only: true,
        };
        assert_eq!(
            decode_personal_mesh_wire(&encode_personal_mesh_wire(&mesh).unwrap()).unwrap(),
            mesh
        );
    }
}

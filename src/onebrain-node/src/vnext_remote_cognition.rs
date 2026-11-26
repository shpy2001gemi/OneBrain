//! Pillar 6 `RUN-003` Negotiated Remote Cognitive Execution (`obp/cognition/1`).
//!
//! Enforces:
//! - Independent default-off `remote_cognition_requester` and `remote_cognition_executor`
//!   feature flags with independent kill switches (`VNextExtendedFeatureConfig`).
//! - `CAP-003` signed `CapabilityOffer` reduction (`CapabilityOfferReducer`).
//! - `CAP-004` `R3` non-delegable (`!onward_delegation`) `RetentionRule::DeleteAfterTask`
//!   permit verification (`authenticate_delegation_permit` + `PermitValidator`).
//! - `DisclosureCapsule` input commitment binding + authenticated `RemoteTaskCancelV1`.
//! - `TypedCognitiveExecutor::execute_once` + durable lazy-opened `CognitiveTaskReplayGuard`
//!   persisted in `<data_dir>/vnext_cognition.redb` (never created on default startup).
//! - Remote responses land STRICTLY in non-executable `VaultQuarantineRecord`
//!   (`is_executable() == false`), NEVER in `ValidatedStore`.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use ed25519_dalek::{Signature, Verifier as _, VerifyingKey};
use ku_ai::vnext_executor::{
    cognitive_input_commitment, cognitive_output_commitment, cognitive_task_commitment,
    CancellationToken, CognitiveExecutionError, CognitiveExecutionPolicy, CognitiveExecutionResult,
    CognitiveTask, CognitiveTaskReplayGuard, TypedCapabilityBackend, TypedCognitiveExecutor,
};
use ku_core::foundation::{
    authenticate_delegation_permit, decode_capability_offer, signature_message,
    AuthenticatedDelegationPermit, CapabilityOfferApplyOutcome, CapabilityOfferError,
    CapabilityOfferReducer, KeyStateReducer, ObjectCid, PermitCid, PermitValidator, ReservedDomain,
    RetentionRule, StoredRecordKind, ValidatedCapabilityOffer, ValidatedFeedInception,
    ValidatedVaultRestorePort, VaultQuarantineRecord,
};
use onebrain_protocol::pillar_wire::{
    decode_cognition_wire, encode_cognition_wire, CognitionWireMessageV1, PillarWireError,
    RemoteCognitiveRequestV1, RemoteCognitiveResponseV1, RemoteTaskCancelReasonV1,
    RemoteTaskCancelV1,
};
use redb::{Database, ReadableTable, ReadableTableMetadata, TableDefinition};
use thiserror::Error;

use crate::vnext_extended_config::{VNextExtendedFeature, VNextExtendedFeatureConfig};

const COGNITION_REPLAY_TABLE: TableDefinition<&[u8], &[u8]> =
    TableDefinition::new("vnext_cognition_task_replay_v1");
pub const COGNITION_DB_FILENAME: &str = "vnext_cognition.redb";
pub const MAX_COGNITION_REPLAY_TOMBSTONES: u64 = 65_536;
pub const REMOTE_COGNITION_QUARANTINE_REASON: &str =
    "RUN-003:remote-cognition-output-quarantine-v1";
const CANCEL_SIG_DOMAIN: &[u8] = b"onebrain:vnext:run003-task-cancel:1\0";

#[derive(Debug, Error)]
pub enum RemoteCognitionError {
    #[error("remote_cognition_requester feature is disabled or kill-switched")]
    RequesterDisabled,
    #[error("remote_cognition_executor feature is disabled or kill-switched")]
    ExecutorDisabled,
    #[error("capability offer validation failed: {0:?}")]
    Offer(CapabilityOfferError),
    #[error("R3 remote cognition permit invalid: {0}")]
    PermitInvalid(String),
    #[error("R3 remote cognition permit must be non-delegable (`onward_delegation == false`)")]
    OnwardDelegationForbidden,
    #[error("R3 remote cognition permit must enforce `RetentionRule::DeleteAfterTask`")]
    DeleteAfterTaskRequired,
    #[error("remote cognitive execution failed: {0:?}")]
    Execution(CognitiveExecutionError),
    #[error("remote cognitive task replay detected across durable guard")]
    DurableTaskReplay,
    #[error("remote cognitive task identity conflict detected across durable guard")]
    DurableTaskConflict,
    #[error("remote cognitive replay capacity exhausted")]
    ReplayCapacityExhausted,
    #[error("remote cognitive wire error: {0:?}")]
    Wire(PillarWireError),
    #[error("remote cognitive cancel signature verification failed")]
    InvalidCancelSignature,
    #[error("remote cognitive response failed verification: {0}")]
    InvalidResponse(&'static str),
    #[error("remote cognitive storage error: {0}")]
    Storage(String),
}

impl From<CapabilityOfferError> for RemoteCognitionError {
    fn from(error: CapabilityOfferError) -> Self {
        Self::Offer(error)
    }
}

impl From<CognitiveExecutionError> for RemoteCognitionError {
    fn from(error: CognitiveExecutionError) -> Self {
        Self::Execution(error)
    }
}

impl From<PillarWireError> for RemoteCognitionError {
    fn from(error: PillarWireError) -> Self {
        Self::Wire(error)
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct PendingOutboundRequest {
    task_nonce: [u8; 32],
    permit_cid: PermitCid,
    offer_cid: ObjectCid,
    expected_manifest_cid: ObjectCid,
    input_commitment_cid: ObjectCid,
}

/// Signing preimage for `RemoteTaskCancelV1`.
pub fn remote_task_cancel_signing_preimage(
    task_nonce: [u8; 32],
    permit_cid: PermitCid,
    reason: RemoteTaskCancelReasonV1,
) -> Vec<u8> {
    let mut raw = Vec::with_capacity(CANCEL_SIG_DOMAIN.len() + 72);
    raw.extend_from_slice(CANCEL_SIG_DOMAIN);
    raw.extend_from_slice(&task_nonce);
    raw.extend_from_slice(permit_cid.as_bytes());
    raw.extend_from_slice(&(reason as u64).to_be_bytes());
    signature_message(ReservedDomain::Event, &raw).expect("non-empty cancel preimage")
}

/// Default host `CognitiveExecutionPolicy` using deterministic limitation CCIDs.
pub const fn default_cognitive_execution_policy() -> CognitiveExecutionPolicy {
    use ku_core::foundation::ConceptCcid;
    CognitiveExecutionPolicy {
        cancelled_limitation: ConceptCcid::from_bytes([0xE1; 16]),
        deadline_limitation: ConceptCcid::from_bytes([0xE2; 16]),
        resource_limitation: ConceptCcid::from_bytes([0xE3; 16]),
        backend_failure_limitation: ConceptCcid::from_bytes([0xE4; 16]),
        backend_protocol_limitation: ConceptCcid::from_bytes([0xE5; 16]),
    }
}

/// Host coordinator for `RUN-003` `obp/cognition/1` requester and executor roles.
pub struct RemoteCognitionRuntime {
    data_dir: PathBuf,
    db: Option<Database>,
    offer_reducer: CapabilityOfferReducer,
    permits: PermitValidator,
    replay_guard: CognitiveTaskReplayGuard,
    executor: TypedCognitiveExecutor,
    active_cancellations: BTreeMap<[u8; 32], (PermitCid, [u8; 32], CancellationToken)>,
    pending_outbound: BTreeMap<[u8; 32], PendingOutboundRequest>,
}

impl RemoteCognitionRuntime {
    /// Construct a lazy runtime; `<data_dir>/vnext_cognition.redb` is NOT created
    /// until an executor replay check or explicit persistence call runs.
    pub fn new_lazy(data_dir: impl AsRef<Path>, policy: CognitiveExecutionPolicy) -> Self {
        Self {
            data_dir: data_dir.as_ref().to_path_buf(),
            db: None,
            offer_reducer: CapabilityOfferReducer::default(),
            permits: PermitValidator::default(),
            replay_guard: CognitiveTaskReplayGuard::default(),
            executor: TypedCognitiveExecutor::new(policy),
            active_cancellations: BTreeMap::new(),
            pending_outbound: BTreeMap::new(),
        }
    }

    pub fn db_path(&self) -> PathBuf {
        self.data_dir.join(COGNITION_DB_FILENAME)
    }

    pub const fn grants_validated_store_authority(&self) -> bool {
        false
    }

    fn ensure_db(&mut self) -> Result<&Database, RemoteCognitionError> {
        if self.db.is_none() {
            std::fs::create_dir_all(&self.data_dir)
                .map_err(|e| RemoteCognitionError::Storage(e.to_string()))?;
            let db = Database::create(self.db_path())
                .map_err(|e| RemoteCognitionError::Storage(e.to_string()))?;
            let write = db
                .begin_write()
                .map_err(|e| RemoteCognitionError::Storage(e.to_string()))?;
            {
                write
                    .open_table(COGNITION_REPLAY_TABLE)
                    .map_err(|e| RemoteCognitionError::Storage(e.to_string()))?;
            }
            write
                .commit()
                .map_err(|e| RemoteCognitionError::Storage(e.to_string()))?;
            self.db = Some(db);
        }
        Ok(self.db.as_ref().expect("opened above"))
    }

    /// Export durable `(task_id, task_commitment)` replay floors for `0x000F` archive snapshots.
    pub fn durable_replay_entries(
        &mut self,
    ) -> Result<Vec<([u8; 32], [u8; 32])>, RemoteCognitionError> {
        if self.db.is_none() && !self.db_path().exists() {
            return Ok(Vec::new());
        }
        let db = self.ensure_db()?;
        let read = db
            .begin_read()
            .map_err(|e| RemoteCognitionError::Storage(e.to_string()))?;
        let table = read
            .open_table(COGNITION_REPLAY_TABLE)
            .map_err(|e| RemoteCognitionError::Storage(e.to_string()))?;
        let mut out = Vec::new();
        for item in table
            .iter()
            .map_err(|e| RemoteCognitionError::Storage(e.to_string()))?
        {
            let (k, v) = item.map_err(|e| RemoteCognitionError::Storage(e.to_string()))?;
            let task_id: [u8; 32] = k
                .value()
                .try_into()
                .map_err(|_| RemoteCognitionError::Storage("corrupt task_id".into()))?;
            let commitment: [u8; 32] = v
                .value()
                .try_into()
                .map_err(|_| RemoteCognitionError::Storage("corrupt task_commitment".into()))?;
            out.push((task_id, commitment));
        }
        Ok(out)
    }

    /// Restore a `(task_id, task_commitment)` replay floor from an `0x000F` archive.
    pub fn restore_replay_entry(
        &mut self,
        task_id: [u8; 32],
        commitment: [u8; 32],
    ) -> Result<(), RemoteCognitionError> {
        let db = self.ensure_db()?;
        let write = db
            .begin_write()
            .map_err(|e| RemoteCognitionError::Storage(e.to_string()))?;
        {
            let mut table = write
                .open_table(COGNITION_REPLAY_TABLE)
                .map_err(|e| RemoteCognitionError::Storage(e.to_string()))?;
            let existing = table
                .get(task_id.as_slice())
                .map_err(|e| RemoteCognitionError::Storage(e.to_string()))?
                .map(|g| g.value().to_vec());
            if let Some(existing_bytes) = existing {
                if existing_bytes.as_slice() != commitment.as_slice() {
                    return Err(RemoteCognitionError::DurableTaskConflict);
                }
            } else {
                table
                    .insert(task_id.as_slice(), commitment.as_slice())
                    .map_err(|e| RemoteCognitionError::Storage(e.to_string()))?;
            }
        }
        write
            .commit()
            .map_err(|e| RemoteCognitionError::Storage(e.to_string()))
    }

    // ─── `CAP-003` Offers & `CAP-004` `R3` Permits ───────────────────────────

    pub fn register_capability_offer(
        &mut self,
        signed_offer_bytes: &[u8],
        signer_feed: &ValidatedFeedInception,
        _key_state: &KeyStateReducer,
    ) -> Result<(ValidatedCapabilityOffer, CapabilityOfferApplyOutcome), RemoteCognitionError> {
        let validated = decode_capability_offer(signed_offer_bytes, signer_feed)?;
        let outcome = self.offer_reducer.apply(validated.clone());
        Ok((validated, outcome))
    }

    /// Verify and submit a `CAP-004` `R3` delegation permit, strictly enforcing
    /// `!onward_delegation` and `RetentionRule::DeleteAfterTask`.
    pub fn verify_and_register_r3_permit(
        &mut self,
        signed_permit_bytes: &[u8],
        issuer_feed: &ValidatedFeedInception,
        key_state: &KeyStateReducer,
        local_tick: u64,
    ) -> Result<AuthenticatedDelegationPermit, RemoteCognitionError> {
        let permit = authenticate_delegation_permit(signed_permit_bytes, issuer_feed, key_state)
            .map_err(|e| RemoteCognitionError::PermitInvalid(format!("{e:?}")))?;
        if permit.body.onward_delegation {
            return Err(RemoteCognitionError::OnwardDelegationForbidden);
        }
        if permit.body.retention != RetentionRule::DeleteAfterTask {
            return Err(RemoteCognitionError::DeleteAfterTaskRequired);
        }
        self.permits
            .submit(permit.clone(), local_tick)
            .map_err(|e| RemoteCognitionError::PermitInvalid(format!("{e:?}")))?;
        Ok(permit)
    }

    pub fn permits(&self) -> &PermitValidator {
        &self.permits
    }

    // ─── Requester Role (`remote_cognition_requester`) ───────────────────────

    #[allow(clippy::too_many_arguments)]
    pub fn prepare_remote_request(
        &mut self,
        extended: &VNextExtendedFeatureConfig,
        task_nonce: [u8; 32],
        offer_cid: ObjectCid,
        expected_manifest_cid: ObjectCid,
        signed_permit_bytes: Vec<u8>,
        issuer_feed: &ValidatedFeedInception,
        key_state: &KeyStateReducer,
        local_tick: u64,
        raw_input_payload: &[u8],
        disclosure_capsule_bytes: Vec<u8>,
    ) -> Result<(RemoteCognitiveRequestV1, Vec<u8>), RemoteCognitionError> {
        if !extended.is_active(VNextExtendedFeature::RemoteCognitionRequester) {
            return Err(RemoteCognitionError::RequesterDisabled);
        }
        let permit = self.verify_and_register_r3_permit(
            &signed_permit_bytes,
            issuer_feed,
            key_state,
            local_tick,
        )?;
        let input_commitment = cognitive_input_commitment(raw_input_payload);
        if !permit.body.input_commitments.contains(&input_commitment) {
            return Err(RemoteCognitionError::PermitInvalid(
                "input_commitment not authorized by R3 permit".into(),
            ));
        }
        let input_commitment_cid = ObjectCid::from_bytes(input_commitment);
        let request = RemoteCognitiveRequestV1 {
            task_nonce,
            offer_cid,
            permit_cid: permit.permit_id,
            signed_permit_bytes,
            input_commitment_cid,
            disclosure_capsule_bytes,
        };
        let wire_bytes = encode_cognition_wire(&CognitionWireMessageV1::Request(request.clone()))?;
        self.pending_outbound.insert(
            task_nonce,
            PendingOutboundRequest {
                task_nonce,
                permit_cid: permit.permit_id,
                offer_cid,
                expected_manifest_cid,
                input_commitment_cid,
            },
        );
        Ok((request, wire_bytes))
    }

    /// Verify a `RemoteCognitiveResponseV1` and stage its payload strictly into a
    /// non-executable `VaultQuarantineRecord` (`is_executable() == false`).
    pub fn ingest_remote_response(
        &mut self,
        extended: &VNextExtendedFeatureConfig,
        response: &RemoteCognitiveResponseV1,
    ) -> Result<VaultQuarantineRecord, RemoteCognitionError> {
        if !extended.is_active(VNextExtendedFeature::RemoteCognitionRequester) {
            return Err(RemoteCognitionError::RequesterDisabled);
        }
        if response.is_executable {
            return Err(RemoteCognitionError::InvalidResponse(
                "executable_flag_must_be_false",
            ));
        }
        let pending = self
            .pending_outbound
            .remove(&response.task_nonce)
            .ok_or(RemoteCognitionError::InvalidResponse("unknown_task_nonce"))?;
        if response.permit_cid != pending.permit_cid
            || response.manifest_cid != pending.expected_manifest_cid
            || response.input_commitment_cid != pending.input_commitment_cid
        {
            return Err(RemoteCognitionError::InvalidResponse(
                "permit_manifest_or_input_mismatch",
            ));
        }
        let computed_out = cognitive_output_commitment(&response.quarantined_output_capsule);
        if response.output_commitment_cid != ObjectCid::from_bytes(computed_out) {
            return Err(RemoteCognitionError::InvalidResponse(
                "output_commitment_mismatch",
            ));
        }

        let claimed_cid =
            *ObjectCid::compute(ReservedDomain::Object, &response.quarantined_output_capsule)
                .map_err(|_| RemoteCognitionError::InvalidResponse("empty_or_oversized_capsule"))?
                .as_bytes();
        let mut q_hasher = blake3::Hasher::new();
        q_hasher.update(b"onebrain:vnext:vault-portable-quarantine:1\0");
        q_hasher.update(&[StoredRecordKind::Object as u8]);
        q_hasher.update(&claimed_cid);
        q_hasher.update(&(REMOTE_COGNITION_QUARANTINE_REASON.len() as u64).to_be_bytes());
        q_hasher.update(REMOTE_COGNITION_QUARANTINE_REASON.as_bytes());
        q_hasher.update(&response.quarantined_output_capsule);
        let quarantine = VaultQuarantineRecord {
            quarantine_id: *q_hasher.finalize().as_bytes(),
            record_kind: StoredRecordKind::Object,
            claimed_cid,
            reason_code: REMOTE_COGNITION_QUARANTINE_REASON.to_string(),
            plaintext: response.quarantined_output_capsule.clone(),
        };
        debug_assert!(!quarantine.is_executable());
        Ok(quarantine)
    }

    pub fn ingest_remote_response_into_vault<V: ValidatedVaultRestorePort>(
        &mut self,
        extended: &VNextExtendedFeatureConfig,
        response: &RemoteCognitiveResponseV1,
        vault: &V,
    ) -> Result<VaultQuarantineRecord, RemoteCognitionError> {
        let quarantine = self.ingest_remote_response(extended, response)?;
        vault
            .restore_vault_quarantine(&quarantine)
            .map_err(|e| RemoteCognitionError::Storage(format!("{e:?}")))?;
        Ok(quarantine)
    }

    // ─── Executor Role (`remote_cognition_executor`) ─────────────────────────

    pub fn register_active_cancellation_token(
        &mut self,
        task_nonce: [u8; 32],
        permit_cid: PermitCid,
        requester_public_key: [u8; 32],
    ) -> CancellationToken {
        let token = CancellationToken::new();
        self.active_cancellations.insert(
            task_nonce,
            (permit_cid, requester_public_key, token.clone()),
        );
        token
    }

    pub fn apply_remote_cancel(
        &mut self,
        cancel: &RemoteTaskCancelV1,
    ) -> Result<bool, RemoteCognitionError> {
        let Some((expected_permit, expected_pk, token)) =
            self.active_cancellations.get(&cancel.task_nonce)
        else {
            return Ok(false);
        };
        if *expected_permit != cancel.permit_cid || *expected_pk != cancel.requester_public_key {
            return Err(RemoteCognitionError::InvalidCancelSignature);
        }
        let vk = VerifyingKey::from_bytes(&cancel.requester_public_key)
            .map_err(|_| RemoteCognitionError::InvalidCancelSignature)?;
        let preimage = remote_task_cancel_signing_preimage(
            cancel.task_nonce,
            cancel.permit_cid,
            cancel.reason,
        );
        vk.verify(
            &preimage,
            &Signature::from_bytes(&cancel.requester_signature),
        )
        .map_err(|_| RemoteCognitionError::InvalidCancelSignature)?;
        token.cancel();
        Ok(true)
    }

    #[allow(clippy::too_many_arguments)]
    pub fn execute_remote_request<B: TypedCapabilityBackend>(
        &mut self,
        extended: &VNextExtendedFeatureConfig,
        backend: &mut B,
        request: &RemoteCognitiveRequestV1,
        issuer_feed: &ValidatedFeedInception,
        key_state: &KeyStateReducer,
        mut task: CognitiveTask,
        started_at: u64,
    ) -> Result<(CognitiveExecutionResult, RemoteCognitiveResponseV1, Vec<u8>), RemoteCognitionError>
    {
        if !extended.is_active(VNextExtendedFeature::RemoteCognitionExecutor) {
            return Err(RemoteCognitionError::ExecutorDisabled);
        }
        let permit = self.verify_and_register_r3_permit(
            &request.signed_permit_bytes,
            issuer_feed,
            key_state,
            started_at,
        )?;
        if permit.permit_id != request.permit_cid || task.permit_id != request.permit_cid {
            return Err(RemoteCognitionError::PermitInvalid(
                "permit_cid mismatch".into(),
            ));
        }
        task.task_id = request.task_nonce;
        task.retention = RetentionRule::DeleteAfterTask;

        // Check durable replay guard before invoking TypedCognitiveExecutor.
        let commitment = cognitive_task_commitment(&task);
        {
            let db = self.ensure_db()?;
            let write = db
                .begin_write()
                .map_err(|e| RemoteCognitionError::Storage(e.to_string()))?;
            {
                let mut table = write
                    .open_table(COGNITION_REPLAY_TABLE)
                    .map_err(|e| RemoteCognitionError::Storage(e.to_string()))?;
                if let Some(existing) = table
                    .get(task.task_id.as_slice())
                    .map_err(|e| RemoteCognitionError::Storage(e.to_string()))?
                {
                    if existing.value() == commitment.as_slice() {
                        return Err(RemoteCognitionError::DurableTaskReplay);
                    }
                    return Err(RemoteCognitionError::DurableTaskConflict);
                }
                if table
                    .len()
                    .map_err(|e| RemoteCognitionError::Storage(e.to_string()))?
                    >= MAX_COGNITION_REPLAY_TOMBSTONES
                {
                    return Err(RemoteCognitionError::ReplayCapacityExhausted);
                }
                table
                    .insert(task.task_id.as_slice(), commitment.as_slice())
                    .map_err(|e| RemoteCognitionError::Storage(e.to_string()))?;
            }
            write
                .commit()
                .map_err(|e| RemoteCognitionError::Storage(e.to_string()))?;
        }

        let token = self
            .active_cancellations
            .get(&request.task_nonce)
            .map(|(_, _, t)| t.clone())
            .unwrap_or_else(|| {
                self.register_active_cancellation_token(
                    request.task_nonce,
                    request.permit_cid,
                    issuer_feed.signed.inception.feed_public_key,
                )
            });

        let exec = self.executor.execute_once(
            backend,
            &self.permits,
            &mut self.replay_guard,
            task.clone(),
            started_at,
            &token,
        )?;
        self.active_cancellations.remove(&request.task_nonce);

        let record_val = exec
            .record
            .canonical_body()
            .map_err(|e| RemoteCognitionError::PermitInvalid(format!("{e:?}")))?;
        let execution_record_bytes = ku_core::foundation::encode_canonical(
            &record_val,
            ku_core::foundation::ResourceProfile::ObjectV1,
        )
        .map_err(|e| RemoteCognitionError::PermitInvalid(format!("{e:?}")))?;

        let payload_out = if exec.output.is_empty() {
            b"cancelled-or-empty".to_vec()
        } else {
            exec.output.clone()
        };
        let output_commitment_cid =
            ObjectCid::from_bytes(cognitive_output_commitment(&payload_out));
        let response = RemoteCognitiveResponseV1 {
            task_nonce: request.task_nonce,
            permit_cid: request.permit_cid,
            manifest_cid: task.implementation_manifest,
            input_commitment_cid: request.input_commitment_cid,
            output_commitment_cid,
            execution_record_bytes,
            quarantined_output_capsule: payload_out,
            is_executable: false,
            cancelled: exec.termination == ku_ai::vnext_executor::CognitiveTermination::Cancelled,
        };
        let wire = encode_cognition_wire(&CognitionWireMessageV1::Response(response.clone()))?;
        Ok((exec, response, wire))
    }

    pub fn decode_wire_frame(frame: &[u8]) -> Result<CognitionWireMessageV1, RemoteCognitionError> {
        decode_cognition_wire(frame).map_err(Into::into)
    }
}

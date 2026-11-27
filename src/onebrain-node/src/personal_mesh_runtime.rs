//! Pillar 10 Personal Mesh Runtime (`P10` §2–§3, `FW-04`, `FW-10`, `NEG-P10-001`).
//!
//! Node-owned durable coordinator for same-actor Personal Mesh pairing (2-round SPAKE2+),
//! principal isolation (`ActorId`, `DeviceId`, `FeedId`, `NodeId` pairwise distinct; zero
//! `FeedEventSigner` secret export), per-device X25519 + HKDF-SHA256 + XChaCha20-Poly1305
//! `PersonalMeshEpochDek` wrapping, forward-secret revocation epoch rotation
//! (`mesh_epoch -> mesh_epoch + 1`), and encrypted record sync.
//!
//! Lazily opens `<data_dir>/vnext_personal_mesh.redb` ONLY when explicitly invoked so
//! default `OneBrainNode` / `VNextProductRuntime` startup creates zero extra `.redb` files.

use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};

use ku_core::foundation::{
    decode_canonical, encode_canonical, ActorId, CanonicalValue, DeviceId,
    EncryptedPersonalMeshRecord, EventCid, FeedId, NodeId, PersonalMeshEpochDek,
    PersonalMeshEpochState, PersonalMeshError, PersonalMeshMemberBindingV1, PersonalMeshPrincipals,
    PersonalMeshRecordClass, ResourceProfile, Spake2PlusAwaitingConfirmation,
    Spake2PlusEstablished, Spake2PlusParticipant, Spake2PlusRole, Spake2PlusRound1Message,
    Spake2PlusRound2Confirmation, ValidatedActorRevocation, WrappedPersonalMeshEpochDek,
};
use redb::{Database, TableDefinition};
use thiserror::Error;

use crate::vnext_extended_config::{VNextExtendedFeature, VNextExtendedFeatureConfig};

const MESH_STATE_TABLE: TableDefinition<&[u8], &[u8]> =
    TableDefinition::new("vnext_personal_mesh_state_v1");
const MESH_RECORDS_TABLE: TableDefinition<&[u8], &[u8]> =
    TableDefinition::new("vnext_personal_mesh_records_v1");
const MESH_STATE_KEY: &[u8] = b"active_epoch_state_v1";
const MESH_SNAPSHOT_MAJOR: u64 = 1;

pub const PERSONAL_MESH_DB_FILENAME: &str = "vnext_personal_mesh.redb";

#[derive(Debug, Error)]
pub enum PersonalMeshRuntimeError {
    #[error("personal_mesh_v1 feature is disabled or kill-switched")]
    FeatureDisabled,
    #[error("personal mesh foundation error: {0:?}")]
    Foundation(PersonalMeshError),
    #[error("personal mesh storage error: {0}")]
    Storage(String),
    #[error("personal mesh pairing session not found")]
    PairingSessionNotFound,
    #[error("personal mesh pairing session role mismatch")]
    PairingRoleMismatch,
    #[error("personal mesh is not initialized")]
    MeshNotInitialized,
    #[error("personal mesh epoch DEK is not unlocked on this device for epoch {0}")]
    EpochDekLocked(u64),
    #[error(
        "personal mesh record epoch {record_epoch} does not match active epoch {active_epoch}"
    )]
    EpochMismatch {
        record_epoch: u64,
        active_epoch: u64,
    },
    #[error("personal mesh snapshot encoding is corrupt")]
    CorruptSnapshot,
}

impl From<PersonalMeshError> for PersonalMeshRuntimeError {
    fn from(error: PersonalMeshError) -> Self {
        Self::Foundation(error)
    }
}

enum PairingSessionStage {
    InitiatorRound1(Spake2PlusParticipant),
    AwaitingConfirm(Spake2PlusAwaitingConfirmation),
}

/// Summary of an active SPAKE2+ pairing handshake after Round 1 exchange.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PairingChallengeView {
    pub session_id: [u8; 32],
    pub outbound_confirmation: Spake2PlusRound2Confirmation,
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct PersistedMeshSummary {
    actor_id: ActorId,
    mesh_epoch: u64,
    active_devices: Vec<DeviceId>,
    revoked_devices: BTreeSet<DeviceId>,
    wrapped_deks: BTreeMap<DeviceId, WrappedPersonalMeshEpochDek>,
}

/// Durable lazy-opened Personal Mesh manager.
pub struct PersonalMeshRuntime {
    data_dir: PathBuf,
    db: Option<Database>,
    state: Option<PersonalMeshEpochState>,
    persisted_summary: Option<PersistedMeshSummary>,
    unlocked_deks: BTreeMap<u64, PersonalMeshEpochDek>,
    pairing_sessions: BTreeMap<[u8; 32], PairingSessionStage>,
}

impl PersonalMeshRuntime {
    /// Construct a lazy handle; no `.redb` file is touched until a mutating or
    /// query method explicitly requests durable state.
    pub fn new_lazy(data_dir: impl AsRef<Path>) -> Self {
        Self {
            data_dir: data_dir.as_ref().to_path_buf(),
            db: None,
            state: None,
            persisted_summary: None,
            unlocked_deks: BTreeMap::new(),
            pairing_sessions: BTreeMap::new(),
        }
    }

    /// Open and hydrate any persisted Personal Mesh state immediately.
    pub fn open(data_dir: impl AsRef<Path>) -> Result<Self, PersonalMeshRuntimeError> {
        let mut runtime = Self::new_lazy(data_dir);
        runtime.ensure_open()?;
        Ok(runtime)
    }

    pub fn db_path(&self) -> PathBuf {
        self.data_dir.join(PERSONAL_MESH_DB_FILENAME)
    }

    pub const fn exports_feed_signer_secret(&self) -> bool {
        false
    }

    pub fn ensure_enabled(
        extended: &VNextExtendedFeatureConfig,
    ) -> Result<(), PersonalMeshRuntimeError> {
        if extended.is_active(VNextExtendedFeature::PersonalMeshV1) {
            Ok(())
        } else {
            Err(PersonalMeshRuntimeError::FeatureDisabled)
        }
    }

    fn ensure_open(&mut self) -> Result<&Database, PersonalMeshRuntimeError> {
        if self.db.is_none() {
            std::fs::create_dir_all(&self.data_dir)
                .map_err(|e| PersonalMeshRuntimeError::Storage(e.to_string()))?;
            let db = Database::create(self.db_path())
                .map_err(|e| PersonalMeshRuntimeError::Storage(e.to_string()))?;
            let write = db
                .begin_write()
                .map_err(|e| PersonalMeshRuntimeError::Storage(e.to_string()))?;
            {
                write
                    .open_table(MESH_STATE_TABLE)
                    .map_err(|e| PersonalMeshRuntimeError::Storage(e.to_string()))?;
                write
                    .open_table(MESH_RECORDS_TABLE)
                    .map_err(|e| PersonalMeshRuntimeError::Storage(e.to_string()))?;
            }
            write
                .commit()
                .map_err(|e| PersonalMeshRuntimeError::Storage(e.to_string()))?;

            let loaded = {
                let read = db
                    .begin_read()
                    .map_err(|e| PersonalMeshRuntimeError::Storage(e.to_string()))?;
                let table = read
                    .open_table(MESH_STATE_TABLE)
                    .map_err(|e| PersonalMeshRuntimeError::Storage(e.to_string()))?;
                let maybe_bytes = table
                    .get(MESH_STATE_KEY)
                    .map_err(|e| PersonalMeshRuntimeError::Storage(e.to_string()))?
                    .map(|g| g.value().to_vec());
                maybe_bytes
                    .as_deref()
                    .map(decode_epoch_state_snapshot)
                    .transpose()?
            };
            self.persisted_summary = loaded;
            self.db = Some(db);
        }
        Ok(self.db.as_ref().expect("initialized above"))
    }

    fn persist_state(&mut self) -> Result<(), PersonalMeshRuntimeError> {
        let Some(snapshot) = self.state.as_ref() else {
            return Ok(());
        };
        let summary = summary_from_state(snapshot, self.persisted_summary.as_ref());
        let encoded = encode_epoch_state_snapshot(&summary)?;
        self.persisted_summary = Some(summary);
        let db = self.ensure_open()?;
        let write = db
            .begin_write()
            .map_err(|e| PersonalMeshRuntimeError::Storage(e.to_string()))?;
        {
            let mut table = write
                .open_table(MESH_STATE_TABLE)
                .map_err(|e| PersonalMeshRuntimeError::Storage(e.to_string()))?;
            table
                .insert(MESH_STATE_KEY, encoded.as_slice())
                .map_err(|e| PersonalMeshRuntimeError::Storage(e.to_string()))?;
        }
        write
            .commit()
            .map_err(|e| PersonalMeshRuntimeError::Storage(e.to_string()))
    }

    // ─── SPAKE2+ Pairing Lifecycle ───────────────────────────────────────────

    pub fn pair_start(
        &mut self,
        session_id: [u8; 32],
        pairing_code: &[u8],
        initiator_principals: &PersonalMeshPrincipals,
        responder_principals: &PersonalMeshPrincipals,
        ephemeral_seed: [u8; 32],
    ) -> Result<Spake2PlusRound1Message, PersonalMeshRuntimeError> {
        if session_id == [0; 32] {
            return Err(PersonalMeshError::InvalidIdentity("zero_session_id").into());
        }
        let (participant, round1) = Spake2PlusParticipant::start(
            Spake2PlusRole::Initiator,
            pairing_code,
            initiator_principals,
            responder_principals,
            ephemeral_seed,
        )?;
        self.pairing_sessions.insert(
            session_id,
            PairingSessionStage::InitiatorRound1(participant),
        );
        Ok(round1)
    }

    pub fn pair_respond(
        &mut self,
        session_id: [u8; 32],
        pairing_code: &[u8],
        initiator_principals: &PersonalMeshPrincipals,
        responder_principals: &PersonalMeshPrincipals,
        ephemeral_seed: [u8; 32],
        initiator_round1: &Spake2PlusRound1Message,
    ) -> Result<(Spake2PlusRound1Message, PairingChallengeView), PersonalMeshRuntimeError> {
        if session_id == [0; 32] {
            return Err(PersonalMeshError::InvalidIdentity("zero_session_id").into());
        }
        let (participant, responder_round1) = Spake2PlusParticipant::start(
            Spake2PlusRole::Responder,
            pairing_code,
            initiator_principals,
            responder_principals,
            ephemeral_seed,
        )?;
        let awaiting = participant.process_round1(initiator_round1)?;
        let outbound_confirmation = awaiting.outbound_confirmation();
        self.pairing_sessions
            .insert(session_id, PairingSessionStage::AwaitingConfirm(awaiting));
        Ok((
            responder_round1,
            PairingChallengeView {
                session_id,
                outbound_confirmation,
            },
        ))
    }

    pub fn pair_initiator_process_round1(
        &mut self,
        session_id: [u8; 32],
        responder_round1: &Spake2PlusRound1Message,
    ) -> Result<PairingChallengeView, PersonalMeshRuntimeError> {
        let stage = self
            .pairing_sessions
            .remove(&session_id)
            .ok_or(PersonalMeshRuntimeError::PairingSessionNotFound)?;
        let PairingSessionStage::InitiatorRound1(participant) = stage else {
            return Err(PersonalMeshRuntimeError::PairingRoleMismatch);
        };
        let awaiting = participant.process_round1(responder_round1)?;
        let outbound_confirmation = awaiting.outbound_confirmation();
        self.pairing_sessions
            .insert(session_id, PairingSessionStage::AwaitingConfirm(awaiting));
        Ok(PairingChallengeView {
            session_id,
            outbound_confirmation,
        })
    }

    pub fn pair_confirm(
        &mut self,
        session_id: [u8; 32],
        peer_confirmation: &Spake2PlusRound2Confirmation,
    ) -> Result<Spake2PlusEstablished, PersonalMeshRuntimeError> {
        let stage = self
            .pairing_sessions
            .remove(&session_id)
            .ok_or(PersonalMeshRuntimeError::PairingSessionNotFound)?;
        let PairingSessionStage::AwaitingConfirm(awaiting) = stage else {
            return Err(PersonalMeshRuntimeError::PairingRoleMismatch);
        };
        awaiting
            .verify_peer_confirmation(peer_confirmation)
            .map_err(Into::into)
    }

    // ─── Epoch State, Device Roster & Revocation Rotation ────────────────────

    pub fn initialize_mesh(
        &mut self,
        actor_id: ActorId,
        initial_epoch: u64,
        members: Vec<PersonalMeshMemberBindingV1>,
        initial_dek: PersonalMeshEpochDek,
        wrap_material_seed: [u8; 32],
    ) -> Result<u64, PersonalMeshRuntimeError> {
        let epoch_state = PersonalMeshEpochState::initialize(
            actor_id,
            initial_epoch,
            members,
            &initial_dek,
            wrap_material_seed,
        )?;
        let epoch = epoch_state.mesh_epoch();
        self.unlocked_deks.insert(epoch, initial_dek);
        self.state = Some(epoch_state);
        self.persist_state()?;
        Ok(epoch)
    }

    pub fn unlock_epoch_for_device(
        &mut self,
        device_id: DeviceId,
        device_x25519_secret: &[u8; 32],
    ) -> Result<u64, PersonalMeshRuntimeError> {
        if self.state.is_none() && self.persisted_summary.is_none() && self.db_path().exists() {
            let _ = self.ensure_open()?;
        }
        let (actor_id, wrapped) = if let Some(state) = self.state.as_ref() {
            let w = state
                .wrapped_dek_for(&device_id)
                .cloned()
                .ok_or(PersonalMeshError::UnknownOrRevokedDevice(device_id))?;
            (state.actor_id(), w)
        } else if let Some(summary) = self.persisted_summary.as_ref() {
            let w = summary
                .wrapped_deks
                .get(&device_id)
                .cloned()
                .ok_or(PersonalMeshError::UnknownOrRevokedDevice(device_id))?;
            (summary.actor_id, w)
        } else {
            return Err(PersonalMeshRuntimeError::MeshNotInitialized);
        };
        let dek = PersonalMeshEpochDek::unwrap_for_device(
            &wrapped,
            actor_id,
            device_id,
            device_x25519_secret,
        )?;
        let epoch = dek.epoch();
        self.unlocked_deks.insert(epoch, dek);
        Ok(epoch)
    }

    pub fn mesh_epoch(&mut self) -> Result<Option<u64>, PersonalMeshRuntimeError> {
        if self.state.is_none() && self.persisted_summary.is_none() && self.db_path().exists() {
            let _ = self.ensure_open()?;
        }
        Ok(self
            .state
            .as_ref()
            .map(PersonalMeshEpochState::mesh_epoch)
            .or_else(|| self.persisted_summary.as_ref().map(|s| s.mesh_epoch)))
    }

    pub fn devices(&mut self) -> Result<Vec<DeviceId>, PersonalMeshRuntimeError> {
        if self.state.is_none() && self.persisted_summary.is_none() && self.db_path().exists() {
            let _ = self.ensure_open()?;
        }
        if let Some(state) = self.state.as_ref() {
            return Ok(state.active_devices());
        }
        Ok(self
            .persisted_summary
            .as_ref()
            .map(|s| s.active_devices.clone())
            .unwrap_or_default())
    }

    pub fn is_device_revoked(
        &mut self,
        device_id: &DeviceId,
    ) -> Result<bool, PersonalMeshRuntimeError> {
        if self.state.is_none() && self.persisted_summary.is_none() && self.db_path().exists() {
            let _ = self.ensure_open()?;
        }
        if let Some(state) = self.state.as_ref() {
            return Ok(state.is_device_revoked(device_id));
        }
        Ok(self
            .persisted_summary
            .as_ref()
            .is_some_and(|s| s.revoked_devices.contains(device_id)))
    }

    pub fn wrapped_dek_for(
        &mut self,
        device_id: &DeviceId,
    ) -> Result<Option<WrappedPersonalMeshEpochDek>, PersonalMeshRuntimeError> {
        if self.state.is_none() && self.persisted_summary.is_none() && self.db_path().exists() {
            let _ = self.ensure_open()?;
        }
        if let Some(state) = self.state.as_ref() {
            return Ok(state.wrapped_dek_for(device_id).cloned());
        }
        Ok(self
            .persisted_summary
            .as_ref()
            .and_then(|s| s.wrapped_deks.get(device_id).cloned()))
    }

    /// Apply a `ValidatedActorRevocation`, rotating `mesh_epoch -> mesh_epoch + 1` and
    /// wrapping a fresh `PersonalMeshEpochDek` ONLY for remaining non-revoked devices.
    pub fn revoke_device(
        &mut self,
        revocation: &ValidatedActorRevocation,
        fresh_dek_material: [u8; 32],
        wrap_material_seed: [u8; 32],
    ) -> Result<u64, PersonalMeshRuntimeError> {
        if self.state.is_none() {
            let _ = self.ensure_open()?;
        }
        let state = self
            .state
            .as_mut()
            .ok_or(PersonalMeshRuntimeError::MeshNotInitialized)?;
        let fresh_dek =
            state.apply_revocation(revocation, fresh_dek_material, wrap_material_seed)?;
        let next_epoch = fresh_dek.epoch();
        self.unlocked_deks.insert(next_epoch, fresh_dek);
        self.persist_state()?;
        Ok(next_epoch)
    }

    // ─── Encrypted Record Export & Import (`FW-04`, `FW-10`) ─────────────────

    #[allow(clippy::type_complexity)]
    pub fn export_encrypted_records(
        &mut self,
        items: &[(PersonalMeshRecordClass, [u8; 32], [u8; 24], Vec<u8>)],
    ) -> Result<Vec<EncryptedPersonalMeshRecord>, PersonalMeshRuntimeError> {
        if self.state.is_none() && self.persisted_summary.is_none() && self.db_path().exists() {
            let _ = self.ensure_open()?;
        }
        let (actor_id, active_epoch) = if let Some(state) = self.state.as_ref() {
            (state.actor_id(), state.mesh_epoch())
        } else if let Some(summary) = self.persisted_summary.as_ref() {
            (summary.actor_id, summary.mesh_epoch)
        } else {
            return Err(PersonalMeshRuntimeError::MeshNotInitialized);
        };
        let dek = self
            .unlocked_deks
            .get(&active_epoch)
            .cloned()
            .ok_or(PersonalMeshRuntimeError::EpochDekLocked(active_epoch))?;

        let mut sealed_records = Vec::with_capacity(items.len());
        for (record_class, record_id, nonce, plaintext) in items {
            let encrypted =
                dek.seal_record(actor_id, *record_class, *record_id, *nonce, plaintext)?;
            sealed_records.push(encrypted);
        }

        let db = self.ensure_open()?;
        let write = db
            .begin_write()
            .map_err(|e| PersonalMeshRuntimeError::Storage(e.to_string()))?;
        {
            let mut table = write
                .open_table(MESH_RECORDS_TABLE)
                .map_err(|e| PersonalMeshRuntimeError::Storage(e.to_string()))?;
            for rec in &sealed_records {
                let encoded = rec.encode_bytes()?;
                let key = record_storage_key(rec.epoch, rec.record_class, rec.record_id);
                table
                    .insert(key.as_slice(), encoded.as_slice())
                    .map_err(|e| PersonalMeshRuntimeError::Storage(e.to_string()))?;
            }
        }
        write
            .commit()
            .map_err(|e| PersonalMeshRuntimeError::Storage(e.to_string()))?;
        Ok(sealed_records)
    }

    #[allow(clippy::type_complexity)]
    pub fn import_encrypted_records(
        &mut self,
        records: &[EncryptedPersonalMeshRecord],
    ) -> Result<Vec<(PersonalMeshRecordClass, [u8; 32], Vec<u8>)>, PersonalMeshRuntimeError> {
        if self.state.is_none() && self.persisted_summary.is_none() && self.db_path().exists() {
            let _ = self.ensure_open()?;
        }
        let (actor_id, active_epoch) = if let Some(state) = self.state.as_ref() {
            (state.actor_id(), state.mesh_epoch())
        } else if let Some(summary) = self.persisted_summary.as_ref() {
            (summary.actor_id, summary.mesh_epoch)
        } else {
            return Err(PersonalMeshRuntimeError::MeshNotInitialized);
        };

        let mut opened = Vec::with_capacity(records.len());
        for rec in records {
            rec.record_class.enforce_sync_allowed()?;
            if rec.epoch != active_epoch {
                return Err(PersonalMeshRuntimeError::EpochMismatch {
                    record_epoch: rec.epoch,
                    active_epoch,
                });
            }
            let dek = self
                .unlocked_deks
                .get(&rec.epoch)
                .ok_or(PersonalMeshRuntimeError::EpochDekLocked(rec.epoch))?;
            let plaintext = dek.open_record(actor_id, rec)?;
            opened.push((rec.record_class, rec.record_id, plaintext));
        }

        let db = self.ensure_open()?;
        let write = db
            .begin_write()
            .map_err(|e| PersonalMeshRuntimeError::Storage(e.to_string()))?;
        {
            let mut table = write
                .open_table(MESH_RECORDS_TABLE)
                .map_err(|e| PersonalMeshRuntimeError::Storage(e.to_string()))?;
            for rec in records {
                let encoded = rec.encode_bytes()?;
                let key = record_storage_key(rec.epoch, rec.record_class, rec.record_id);
                table
                    .insert(key.as_slice(), encoded.as_slice())
                    .map_err(|e| PersonalMeshRuntimeError::Storage(e.to_string()))?;
            }
        }
        write
            .commit()
            .map_err(|e| PersonalMeshRuntimeError::Storage(e.to_string()))?;
        Ok(opened)
    }
}

fn record_storage_key(
    epoch: u64,
    record_class: PersonalMeshRecordClass,
    record_id: [u8; 32],
) -> [u8; 48] {
    let mut out = [0u8; 48];
    out[..8].copy_from_slice(&epoch.to_be_bytes());
    out[8..16].copy_from_slice(&(record_class as u64).to_be_bytes());
    out[16..].copy_from_slice(&record_id);
    out
}

fn summary_from_state(
    state: &PersonalMeshEpochState,
    prev: Option<&PersistedMeshSummary>,
) -> PersistedMeshSummary {
    let active_devices = state.active_devices();
    let mut revoked_devices = prev.map(|p| p.revoked_devices.clone()).unwrap_or_default();
    if let Some(prev_sum) = prev {
        for dev in &prev_sum.active_devices {
            if state.is_device_revoked(dev) {
                revoked_devices.insert(*dev);
            }
        }
    }
    let mut wrapped_deks = BTreeMap::new();
    for dev in &active_devices {
        if let Some(w) = state.wrapped_dek_for(dev) {
            wrapped_deks.insert(*dev, w.clone());
        }
    }
    PersistedMeshSummary {
        actor_id: state.actor_id(),
        mesh_epoch: state.mesh_epoch(),
        active_devices,
        revoked_devices,
        wrapped_deks,
    }
}

fn encode_epoch_state_snapshot(
    summary: &PersistedMeshSummary,
) -> Result<Vec<u8>, PersonalMeshRuntimeError> {
    let active_vals = summary
        .active_devices
        .iter()
        .map(|d| CanonicalValue::Bytes(d.as_bytes().to_vec()))
        .collect();
    let revoked_vals = summary
        .revoked_devices
        .iter()
        .map(|d| CanonicalValue::Bytes(d.as_bytes().to_vec()))
        .collect();
    let wrapped_entries = summary
        .wrapped_deks
        .iter()
        .map(|(dev, w)| {
            CanonicalValue::Map(vec![
                (0, CanonicalValue::Bytes(dev.as_bytes().to_vec())),
                (1, CanonicalValue::Unsigned(w.epoch)),
                (2, CanonicalValue::Bytes(w.ephemeral_x25519_public.to_vec())),
                (3, CanonicalValue::Bytes(w.nonce.to_vec())),
                (4, CanonicalValue::Bytes(w.ciphertext.clone())),
            ])
        })
        .collect();
    let root = CanonicalValue::Map(vec![
        (0, CanonicalValue::Unsigned(MESH_SNAPSHOT_MAJOR)),
        (
            1,
            CanonicalValue::Bytes(summary.actor_id.as_bytes().to_vec()),
        ),
        (2, CanonicalValue::Unsigned(summary.mesh_epoch)),
        (3, CanonicalValue::Array(active_vals)),
        (4, CanonicalValue::Array(revoked_vals)),
        (5, CanonicalValue::Array(wrapped_entries)),
    ]);
    encode_canonical(&root, ResourceProfile::ObjectV1)
        .map_err(|_| PersonalMeshRuntimeError::CorruptSnapshot)
}

fn decode_epoch_state_snapshot(
    bytes: &[u8],
) -> Result<PersistedMeshSummary, PersonalMeshRuntimeError> {
    let value = decode_canonical(bytes, ResourceProfile::ObjectV1)
        .map_err(|_| PersonalMeshRuntimeError::CorruptSnapshot)?;
    let CanonicalValue::Map(fields) = value else {
        return Err(PersonalMeshRuntimeError::CorruptSnapshot);
    };
    if fields.len() != 6
        || fields
            .iter()
            .enumerate()
            .any(|(idx, (k, _))| *k != idx as u64)
    {
        return Err(PersonalMeshRuntimeError::CorruptSnapshot);
    }
    let CanonicalValue::Unsigned(MESH_SNAPSHOT_MAJOR) = fields[0].1 else {
        return Err(PersonalMeshRuntimeError::CorruptSnapshot);
    };
    let actor_id = ActorId::from_bytes(mesh_bytes::<32>(&fields[1].1)?);
    let CanonicalValue::Unsigned(mesh_epoch) = fields[2].1 else {
        return Err(PersonalMeshRuntimeError::CorruptSnapshot);
    };
    let CanonicalValue::Array(active_arr) = &fields[3].1 else {
        return Err(PersonalMeshRuntimeError::CorruptSnapshot);
    };
    let mut active_devices = Vec::with_capacity(active_arr.len());
    for item in active_arr {
        active_devices.push(DeviceId::from_bytes(mesh_bytes::<32>(item)?));
    }
    let CanonicalValue::Array(revoked_arr) = &fields[4].1 else {
        return Err(PersonalMeshRuntimeError::CorruptSnapshot);
    };
    let mut revoked_devices = BTreeSet::new();
    for item in revoked_arr {
        revoked_devices.insert(DeviceId::from_bytes(mesh_bytes::<32>(item)?));
    }
    let CanonicalValue::Array(wrapped_arr) = &fields[5].1 else {
        return Err(PersonalMeshRuntimeError::CorruptSnapshot);
    };
    let mut wrapped_deks = BTreeMap::new();
    for item in wrapped_arr {
        let CanonicalValue::Map(wfields) = item else {
            return Err(PersonalMeshRuntimeError::CorruptSnapshot);
        };
        if wfields.len() != 5 {
            return Err(PersonalMeshRuntimeError::CorruptSnapshot);
        }
        let recipient_device = DeviceId::from_bytes(mesh_bytes::<32>(&wfields[0].1)?);
        let CanonicalValue::Unsigned(epoch) = wfields[1].1 else {
            return Err(PersonalMeshRuntimeError::CorruptSnapshot);
        };
        let ephemeral_x25519_public = mesh_bytes::<32>(&wfields[2].1)?;
        let nonce = mesh_bytes::<24>(&wfields[3].1)?;
        let CanonicalValue::Bytes(ciphertext) = &wfields[4].1 else {
            return Err(PersonalMeshRuntimeError::CorruptSnapshot);
        };
        wrapped_deks.insert(
            recipient_device,
            WrappedPersonalMeshEpochDek {
                epoch,
                recipient_device,
                ephemeral_x25519_public,
                nonce,
                ciphertext: ciphertext.clone(),
            },
        );
    }
    Ok(PersistedMeshSummary {
        actor_id,
        mesh_epoch,
        active_devices,
        revoked_devices,
        wrapped_deks,
    })
}

fn mesh_bytes<const N: usize>(value: &CanonicalValue) -> Result<[u8; N], PersonalMeshRuntimeError> {
    let CanonicalValue::Bytes(bytes) = value else {
        return Err(PersonalMeshRuntimeError::CorruptSnapshot);
    };
    bytes
        .as_slice()
        .try_into()
        .map_err(|_| PersonalMeshRuntimeError::CorruptSnapshot)
}

#[allow(dead_code)]
fn _assert_principal_types(_: FeedId, _: NodeId, _: EventCid) {}

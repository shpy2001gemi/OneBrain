//! Durable Redb persistence for `onebrain-obt` (`§6`).
//!
//! Persists reward policies, `vnext_reward_notice_outbox_v2`, quarantined notices,
//! `ShadowClaimEvaluation` diagnostics, `ScopedNullifier`s, epoch cap counters,
//! `PendingMintRecord`s, `FinalizedMintCertificate`s, `AccountBlockV1` chains + settlement
//! states, `AccountEquivocationWarrant`s, and `SettlementDomainCheckpoint`s.

use std::path::Path;

use ku_core::foundation::{ActorId, CanonicalError, ObjectCid};
use redb::{Database, ReadableTable, TableDefinition};
use thiserror::Error;

use crate::account_chain::{
    AccountChainEngine, AccountEquivocationWarrant, SettlementDomainCheckpoint, TrackedAccountBlock,
};
use crate::claims::EpochCapTracker;
use crate::firewall_in::{
    FirewallSinkBackend, ObtEconomicStatus, ObtNoticeV2, ObtRuntimeMode, QuarantinedNotice,
    ShadowClaimEvaluation,
};
use crate::mint_pipeline::{FinalizedMintCertificate, MintPipeline, PendingMintRecord};
use crate::nullifier::ScopedNullifier;
use crate::policy::{PolicyError, RewardPolicyV1};
use crate::units::ObtAmount;

const OBT_META_TABLE: TableDefinition<&[u8], &[u8]> = TableDefinition::new("vnext_obt_meta");
const OBT_POLICIES_TABLE: TableDefinition<&[u8], &[u8]> =
    TableDefinition::new("vnext_obt_policies");
const OBT_NOTICE_OUTBOX_V2_TABLE: TableDefinition<&[u8], &[u8]> =
    TableDefinition::new("vnext_reward_notice_outbox_v2");
const OBT_NOTICE_QUARANTINE_TABLE: TableDefinition<&[u8], &[u8]> =
    TableDefinition::new("vnext_obt_notice_quarantine");
const OBT_SHADOW_EVALS_TABLE: TableDefinition<&[u8], &[u8]> =
    TableDefinition::new("vnext_obt_shadow_evaluations");
const OBT_NULLIFIERS_TABLE: TableDefinition<&[u8], &[u8]> =
    TableDefinition::new("vnext_obt_nullifiers");
const OBT_EPOCH_ACTOR_CAPS_TABLE: TableDefinition<&[u8], &[u8]> =
    TableDefinition::new("vnext_obt_epoch_actor_caps");
const OBT_EPOCH_GROUP_CAPS_TABLE: TableDefinition<&[u8], &[u8]> =
    TableDefinition::new("vnext_obt_epoch_group_caps");
const OBT_PENDING_MINTS_TABLE: TableDefinition<&[u8], &[u8]> =
    TableDefinition::new("vnext_obt_pending_mints");
const OBT_FINALIZED_MINTS_TABLE: TableDefinition<&[u8], &[u8]> =
    TableDefinition::new("vnext_obt_finalized_mints");
const OBT_ACCOUNT_BLOCKS_TABLE: TableDefinition<&[u8], &[u8]> =
    TableDefinition::new("vnext_obt_account_blocks");
const OBT_CANCELLED_SENDS_TABLE: TableDefinition<&[u8], &[u8]> =
    TableDefinition::new("vnext_obt_cancelled_sends");
const OBT_WARRANTS_TABLE: TableDefinition<&[u8], &[u8]> =
    TableDefinition::new("vnext_obt_equivocation_warrants");
const OBT_CHECKPOINTS_TABLE: TableDefinition<&[u8], &[u8]> =
    TableDefinition::new("vnext_obt_settlement_checkpoints");

const META_KEY_MODE: &[u8] = b"runtime_mode";
const META_KEY_ECON_STATUS: &[u8] = b"economic_status";

/// Storage errors returned by `RedbObtStore`.
#[derive(Debug, Error)]
pub enum ObtStoreError {
    #[error("OBT_STORE_REDB: {0}")]
    Redb(String),
    #[error("OBT_STORE_CANONICAL: {0}")]
    Canonical(#[from] CanonicalError),
    #[error("OBT_STORE_POLICY: {0}")]
    Policy(#[from] PolicyError),
    #[error("OBT_STORE_CORRUPT_ROW: {0}")]
    CorruptRow(&'static str),
}

/// Durable Redb store for isolated `onebrain-obt` state.
pub struct RedbObtStore {
    db: Database,
}

impl RedbObtStore {
    /// Open or initialize all P5 Redb tables at `path`.
    pub fn open(path: &Path) -> Result<Self, ObtStoreError> {
        let db = Database::create(path).map_err(|e| ObtStoreError::Redb(e.to_string()))?;
        let write = db
            .begin_write()
            .map_err(|e| ObtStoreError::Redb(e.to_string()))?;
        {
            write
                .open_table(OBT_META_TABLE)
                .map_err(|e| ObtStoreError::Redb(e.to_string()))?;
            write
                .open_table(OBT_POLICIES_TABLE)
                .map_err(|e| ObtStoreError::Redb(e.to_string()))?;
            write
                .open_table(OBT_NOTICE_OUTBOX_V2_TABLE)
                .map_err(|e| ObtStoreError::Redb(e.to_string()))?;
            write
                .open_table(OBT_NOTICE_QUARANTINE_TABLE)
                .map_err(|e| ObtStoreError::Redb(e.to_string()))?;
            write
                .open_table(OBT_SHADOW_EVALS_TABLE)
                .map_err(|e| ObtStoreError::Redb(e.to_string()))?;
            write
                .open_table(OBT_NULLIFIERS_TABLE)
                .map_err(|e| ObtStoreError::Redb(e.to_string()))?;
            write
                .open_table(OBT_EPOCH_ACTOR_CAPS_TABLE)
                .map_err(|e| ObtStoreError::Redb(e.to_string()))?;
            write
                .open_table(OBT_EPOCH_GROUP_CAPS_TABLE)
                .map_err(|e| ObtStoreError::Redb(e.to_string()))?;
            write
                .open_table(OBT_PENDING_MINTS_TABLE)
                .map_err(|e| ObtStoreError::Redb(e.to_string()))?;
            write
                .open_table(OBT_FINALIZED_MINTS_TABLE)
                .map_err(|e| ObtStoreError::Redb(e.to_string()))?;
            write
                .open_table(OBT_ACCOUNT_BLOCKS_TABLE)
                .map_err(|e| ObtStoreError::Redb(e.to_string()))?;
            write
                .open_table(OBT_CANCELLED_SENDS_TABLE)
                .map_err(|e| ObtStoreError::Redb(e.to_string()))?;
            write
                .open_table(OBT_WARRANTS_TABLE)
                .map_err(|e| ObtStoreError::Redb(e.to_string()))?;
            write
                .open_table(OBT_CHECKPOINTS_TABLE)
                .map_err(|e| ObtStoreError::Redb(e.to_string()))?;
        }
        write
            .commit()
            .map_err(|e| ObtStoreError::Redb(e.to_string()))?;
        Ok(Self { db })
    }

    pub fn set_runtime_mode(&self, mode: ObtRuntimeMode) -> Result<(), ObtStoreError> {
        let write = self
            .db
            .begin_write()
            .map_err(|e| ObtStoreError::Redb(e.to_string()))?;
        {
            let mut table = write
                .open_table(OBT_META_TABLE)
                .map_err(|e| ObtStoreError::Redb(e.to_string()))?;
            table
                .insert(META_KEY_MODE, &(mode as u64).to_be_bytes()[..])
                .map_err(|e| ObtStoreError::Redb(e.to_string()))?;
        }
        write
            .commit()
            .map_err(|e| ObtStoreError::Redb(e.to_string()))
    }

    pub fn get_runtime_mode(&self) -> Result<ObtRuntimeMode, ObtStoreError> {
        let read = self
            .db
            .begin_read()
            .map_err(|e| ObtStoreError::Redb(e.to_string()))?;
        let table = read
            .open_table(OBT_META_TABLE)
            .map_err(|e| ObtStoreError::Redb(e.to_string()))?;
        let Some(guard) = table
            .get(META_KEY_MODE)
            .map_err(|e| ObtStoreError::Redb(e.to_string()))?
        else {
            return Ok(ObtRuntimeMode::Disabled);
        };
        let bytes: [u8; 8] = guard
            .value()
            .try_into()
            .map_err(|_| ObtStoreError::CorruptRow("runtime_mode"))?;
        ObtRuntimeMode::from_code(u64::from_be_bytes(bytes))
            .ok_or(ObtStoreError::CorruptRow("runtime_mode_code"))
    }

    pub fn set_economic_status(&self, status: ObtEconomicStatus) -> Result<(), ObtStoreError> {
        let write = self
            .db
            .begin_write()
            .map_err(|e| ObtStoreError::Redb(e.to_string()))?;
        {
            let mut table = write
                .open_table(OBT_META_TABLE)
                .map_err(|e| ObtStoreError::Redb(e.to_string()))?;
            table
                .insert(META_KEY_ECON_STATUS, &(status as u64).to_be_bytes()[..])
                .map_err(|e| ObtStoreError::Redb(e.to_string()))?;
        }
        write
            .commit()
            .map_err(|e| ObtStoreError::Redb(e.to_string()))
    }

    pub fn get_economic_status(&self) -> Result<ObtEconomicStatus, ObtStoreError> {
        let read = self
            .db
            .begin_read()
            .map_err(|e| ObtStoreError::Redb(e.to_string()))?;
        let table = read
            .open_table(OBT_META_TABLE)
            .map_err(|e| ObtStoreError::Redb(e.to_string()))?;
        let Some(guard) = table
            .get(META_KEY_ECON_STATUS)
            .map_err(|e| ObtStoreError::Redb(e.to_string()))?
        else {
            return Ok(ObtEconomicStatus::SimulatedNonEconomic);
        };
        let bytes: [u8; 8] = guard
            .value()
            .try_into()
            .map_err(|_| ObtStoreError::CorruptRow("economic_status"))?;
        ObtEconomicStatus::from_code(u64::from_be_bytes(bytes))
            .ok_or(ObtStoreError::CorruptRow("economic_status_code"))
    }

    pub fn put_policy(&self, policy: &RewardPolicyV1) -> Result<ObjectCid, ObtStoreError> {
        let bytes = policy.encode_canonical_bytes()?;
        let cid = policy.policy_cid();
        let write = self
            .db
            .begin_write()
            .map_err(|e| ObtStoreError::Redb(e.to_string()))?;
        {
            let mut table = write
                .open_table(OBT_POLICIES_TABLE)
                .map_err(|e| ObtStoreError::Redb(e.to_string()))?;
            table
                .insert(cid.as_bytes().as_slice(), bytes.as_slice())
                .map_err(|e| ObtStoreError::Redb(e.to_string()))?;
        }
        write
            .commit()
            .map_err(|e| ObtStoreError::Redb(e.to_string()))?;
        Ok(cid)
    }

    pub fn get_policy(&self, cid: ObjectCid) -> Result<Option<RewardPolicyV1>, ObtStoreError> {
        let read = self
            .db
            .begin_read()
            .map_err(|e| ObtStoreError::Redb(e.to_string()))?;
        let table = read
            .open_table(OBT_POLICIES_TABLE)
            .map_err(|e| ObtStoreError::Redb(e.to_string()))?;
        let Some(guard) = table
            .get(cid.as_bytes().as_slice())
            .map_err(|e| ObtStoreError::Redb(e.to_string()))?
        else {
            return Ok(None);
        };
        let policy = RewardPolicyV1::decode_canonical_bytes(guard.value())?;
        if policy.policy_cid() != cid {
            return Err(ObtStoreError::CorruptRow("policy_cid_mismatch"));
        }
        Ok(Some(policy))
    }

    pub fn put_nullifier(
        &self,
        nullifier: ScopedNullifier,
        tag: u8,
    ) -> Result<bool, ObtStoreError> {
        let write = self
            .db
            .begin_write()
            .map_err(|e| ObtStoreError::Redb(e.to_string()))?;
        let inserted;
        {
            let mut table = write
                .open_table(OBT_NULLIFIERS_TABLE)
                .map_err(|e| ObtStoreError::Redb(e.to_string()))?;
            if table
                .get(nullifier.as_bytes().as_slice())
                .map_err(|e| ObtStoreError::Redb(e.to_string()))?
                .is_some()
            {
                inserted = false;
            } else {
                table
                    .insert(nullifier.as_bytes().as_slice(), &[tag][..])
                    .map_err(|e| ObtStoreError::Redb(e.to_string()))?;
                inserted = true;
            }
        }
        write
            .commit()
            .map_err(|e| ObtStoreError::Redb(e.to_string()))?;
        Ok(inserted)
    }

    pub fn has_nullifier(&self, nullifier: ScopedNullifier) -> Result<bool, ObtStoreError> {
        let read = self
            .db
            .begin_read()
            .map_err(|e| ObtStoreError::Redb(e.to_string()))?;
        let table = read
            .open_table(OBT_NULLIFIERS_TABLE)
            .map_err(|e| ObtStoreError::Redb(e.to_string()))?;
        Ok(table
            .get(nullifier.as_bytes().as_slice())
            .map_err(|e| ObtStoreError::Redb(e.to_string()))?
            .is_some())
    }

    pub fn list_shadow_evaluations(&self) -> Result<Vec<ShadowClaimEvaluation>, ObtStoreError> {
        let read = self
            .db
            .begin_read()
            .map_err(|e| ObtStoreError::Redb(e.to_string()))?;
        let table = read
            .open_table(OBT_SHADOW_EVALS_TABLE)
            .map_err(|e| ObtStoreError::Redb(e.to_string()))?;
        let mut out = Vec::new();
        for entry in table
            .iter()
            .map_err(|e| ObtStoreError::Redb(e.to_string()))?
        {
            let (_, val) = entry.map_err(|e| ObtStoreError::Redb(e.to_string()))?;
            out.push(ShadowClaimEvaluation::decode_canonical_bytes(val.value())?);
        }
        Ok(out)
    }

    pub fn persist_epoch_caps(&self, tracker: &EpochCapTracker) -> Result<(), ObtStoreError> {
        let write = self
            .db
            .begin_write()
            .map_err(|e| ObtStoreError::Redb(e.to_string()))?;
        {
            let mut actor_tbl = write
                .open_table(OBT_EPOCH_ACTOR_CAPS_TABLE)
                .map_err(|e| ObtStoreError::Redb(e.to_string()))?;
            for (&(epoch, actor), amt) in &tracker.actor_totals {
                let mut key = [0u8; 40];
                key[..8].copy_from_slice(&epoch.to_be_bytes());
                key[8..].copy_from_slice(actor.as_bytes());
                actor_tbl
                    .insert(&key[..], &amt.0.to_be_bytes()[..])
                    .map_err(|e| ObtStoreError::Redb(e.to_string()))?;
            }

            let mut group_tbl = write
                .open_table(OBT_EPOCH_GROUP_CAPS_TABLE)
                .map_err(|e| ObtStoreError::Redb(e.to_string()))?;
            for (&(epoch, group), amt) in &tracker.group_totals {
                let mut key = [0u8; 40];
                key[..8].copy_from_slice(&epoch.to_be_bytes());
                key[8..].copy_from_slice(&group);
                group_tbl
                    .insert(&key[..], &amt.0.to_be_bytes()[..])
                    .map_err(|e| ObtStoreError::Redb(e.to_string()))?;
            }

            let mut null_tbl = write
                .open_table(OBT_NULLIFIERS_TABLE)
                .map_err(|e| ObtStoreError::Redb(e.to_string()))?;
            for nullifier in &tracker.consumed_nullifiers {
                null_tbl
                    .insert(nullifier.as_bytes().as_slice(), &[1u8][..])
                    .map_err(|e| ObtStoreError::Redb(e.to_string()))?;
            }
        }
        write
            .commit()
            .map_err(|e| ObtStoreError::Redb(e.to_string()))
    }

    pub fn load_epoch_caps(&self) -> Result<EpochCapTracker, ObtStoreError> {
        let read = self
            .db
            .begin_read()
            .map_err(|e| ObtStoreError::Redb(e.to_string()))?;
        let mut tracker = EpochCapTracker::default();

        let actor_tbl = read
            .open_table(OBT_EPOCH_ACTOR_CAPS_TABLE)
            .map_err(|e| ObtStoreError::Redb(e.to_string()))?;
        for entry in actor_tbl
            .iter()
            .map_err(|e| ObtStoreError::Redb(e.to_string()))?
        {
            let (k, v) = entry.map_err(|e| ObtStoreError::Redb(e.to_string()))?;
            let kb: [u8; 40] = k
                .value()
                .try_into()
                .map_err(|_| ObtStoreError::CorruptRow("epoch_actor_key"))?;
            let vb: [u8; 16] = v
                .value()
                .try_into()
                .map_err(|_| ObtStoreError::CorruptRow("epoch_actor_val"))?;
            let epoch = u64::from_be_bytes(kb[..8].try_into().unwrap());
            let actor = ActorId::from_bytes(kb[8..].try_into().unwrap());
            tracker
                .actor_totals
                .insert((epoch, actor), ObtAmount(u128::from_be_bytes(vb)));
        }

        let group_tbl = read
            .open_table(OBT_EPOCH_GROUP_CAPS_TABLE)
            .map_err(|e| ObtStoreError::Redb(e.to_string()))?;
        for entry in group_tbl
            .iter()
            .map_err(|e| ObtStoreError::Redb(e.to_string()))?
        {
            let (k, v) = entry.map_err(|e| ObtStoreError::Redb(e.to_string()))?;
            let kb: [u8; 40] = k
                .value()
                .try_into()
                .map_err(|_| ObtStoreError::CorruptRow("epoch_group_key"))?;
            let vb: [u8; 16] = v
                .value()
                .try_into()
                .map_err(|_| ObtStoreError::CorruptRow("epoch_group_val"))?;
            let epoch = u64::from_be_bytes(kb[..8].try_into().unwrap());
            let group: [u8; 32] = kb[8..].try_into().unwrap();
            tracker
                .group_totals
                .insert((epoch, group), ObtAmount(u128::from_be_bytes(vb)));
        }

        let null_tbl = read
            .open_table(OBT_NULLIFIERS_TABLE)
            .map_err(|e| ObtStoreError::Redb(e.to_string()))?;
        for entry in null_tbl
            .iter()
            .map_err(|e| ObtStoreError::Redb(e.to_string()))?
        {
            let (k, _) = entry.map_err(|e| ObtStoreError::Redb(e.to_string()))?;
            let nb: [u8; 32] = k
                .value()
                .try_into()
                .map_err(|_| ObtStoreError::CorruptRow("nullifier_key"))?;
            tracker
                .consumed_nullifiers
                .insert(ScopedNullifier::from_bytes(nb));
        }
        Ok(tracker)
    }

    pub fn persist_mint_pipeline(&self, pipeline: &MintPipeline) -> Result<(), ObtStoreError> {
        let write = self
            .db
            .begin_write()
            .map_err(|e| ObtStoreError::Redb(e.to_string()))?;
        {
            let mut pending_tbl = write
                .open_table(OBT_PENDING_MINTS_TABLE)
                .map_err(|e| ObtStoreError::Redb(e.to_string()))?;
            for (cid, rec) in &pipeline.pending_mints {
                let bytes = rec.encode_canonical_bytes()?;
                pending_tbl
                    .insert(&cid[..], bytes.as_slice())
                    .map_err(|e| ObtStoreError::Redb(e.to_string()))?;
            }

            let mut final_tbl = write
                .open_table(OBT_FINALIZED_MINTS_TABLE)
                .map_err(|e| ObtStoreError::Redb(e.to_string()))?;
            for (cid, cert) in &pipeline.finalized_mints {
                let bytes = cert.encode_canonical_bytes()?;
                final_tbl
                    .insert(&cid[..], bytes.as_slice())
                    .map_err(|e| ObtStoreError::Redb(e.to_string()))?;
            }
        }
        write
            .commit()
            .map_err(|e| ObtStoreError::Redb(e.to_string()))
    }

    pub fn load_mint_pipeline(&self) -> Result<MintPipeline, ObtStoreError> {
        let read = self
            .db
            .begin_read()
            .map_err(|e| ObtStoreError::Redb(e.to_string()))?;
        let mut pipeline = MintPipeline::new();

        let pending_tbl = read
            .open_table(OBT_PENDING_MINTS_TABLE)
            .map_err(|e| ObtStoreError::Redb(e.to_string()))?;
        for entry in pending_tbl
            .iter()
            .map_err(|e| ObtStoreError::Redb(e.to_string()))?
        {
            let (_, val) = entry.map_err(|e| ObtStoreError::Redb(e.to_string()))?;
            let rec = PendingMintRecord::decode_canonical_bytes(val.value())?;
            pipeline.pending_mints.insert(rec.pending_mint_cid, rec);
        }

        let final_tbl = read
            .open_table(OBT_FINALIZED_MINTS_TABLE)
            .map_err(|e| ObtStoreError::Redb(e.to_string()))?;
        for entry in final_tbl
            .iter()
            .map_err(|e| ObtStoreError::Redb(e.to_string()))?
        {
            let (_, val) = entry.map_err(|e| ObtStoreError::Redb(e.to_string()))?;
            let cert = FinalizedMintCertificate::decode_canonical_bytes(val.value())?;
            pipeline.finalized_mints.insert(cert.certificate_cid, cert);
        }
        Ok(pipeline)
    }

    pub fn persist_account_chain(&self, engine: &AccountChainEngine) -> Result<(), ObtStoreError> {
        let write = self
            .db
            .begin_write()
            .map_err(|e| ObtStoreError::Redb(e.to_string()))?;
        {
            let mut blk_tbl = write
                .open_table(OBT_ACCOUNT_BLOCKS_TABLE)
                .map_err(|e| ObtStoreError::Redb(e.to_string()))?;
            for (&(actor, seq), branches) in &engine.chains {
                for tracked in branches {
                    let mut key = [0u8; 72];
                    key[..32].copy_from_slice(actor.as_bytes());
                    key[32..40].copy_from_slice(&seq.to_be_bytes());
                    key[40..].copy_from_slice(&tracked.block_cid);
                    let bytes = tracked.encode_canonical_bytes()?;
                    blk_tbl
                        .insert(&key[..], bytes.as_slice())
                        .map_err(|e| ObtStoreError::Redb(e.to_string()))?;
                }
            }

            let mut null_tbl = write
                .open_table(OBT_NULLIFIERS_TABLE)
                .map_err(|e| ObtStoreError::Redb(e.to_string()))?;
            for nullifier in &engine.consumed_nullifiers {
                null_tbl
                    .insert(nullifier.as_bytes().as_slice(), &[2u8][..])
                    .map_err(|e| ObtStoreError::Redb(e.to_string()))?;
            }

            let mut cancel_tbl = write
                .open_table(OBT_CANCELLED_SENDS_TABLE)
                .map_err(|e| ObtStoreError::Redb(e.to_string()))?;
            for send_cid in &engine.cancelled_sends {
                cancel_tbl
                    .insert(&send_cid[..], &[1u8][..])
                    .map_err(|e| ObtStoreError::Redb(e.to_string()))?;
            }

            let mut warrant_tbl = write
                .open_table(OBT_WARRANTS_TABLE)
                .map_err(|e| ObtStoreError::Redb(e.to_string()))?;
            for (cid, warrant) in &engine.warrants {
                let bytes = warrant.encode_canonical_bytes()?;
                warrant_tbl
                    .insert(&cid[..], bytes.as_slice())
                    .map_err(|e| ObtStoreError::Redb(e.to_string()))?;
            }

            let mut cp_tbl = write
                .open_table(OBT_CHECKPOINTS_TABLE)
                .map_err(|e| ObtStoreError::Redb(e.to_string()))?;
            for (&(domain_id, cp_seq), cp) in &engine.checkpoints {
                let mut key = [0u8; 40];
                key[..32].copy_from_slice(&domain_id);
                key[32..].copy_from_slice(&cp_seq.to_be_bytes());
                let bytes = cp.encode_canonical_bytes()?;
                cp_tbl
                    .insert(&key[..], bytes.as_slice())
                    .map_err(|e| ObtStoreError::Redb(e.to_string()))?;
            }
        }
        write
            .commit()
            .map_err(|e| ObtStoreError::Redb(e.to_string()))
    }

    pub fn load_account_chain(&self) -> Result<AccountChainEngine, ObtStoreError> {
        let read = self
            .db
            .begin_read()
            .map_err(|e| ObtStoreError::Redb(e.to_string()))?;
        let mut engine = AccountChainEngine::new();

        let blk_tbl = read
            .open_table(OBT_ACCOUNT_BLOCKS_TABLE)
            .map_err(|e| ObtStoreError::Redb(e.to_string()))?;
        for entry in blk_tbl
            .iter()
            .map_err(|e| ObtStoreError::Redb(e.to_string()))?
        {
            let (_, val) = entry.map_err(|e| ObtStoreError::Redb(e.to_string()))?;
            let tracked = TrackedAccountBlock::decode_canonical_bytes(val.value())?;
            let actor = tracked.block.account;
            let seq = tracked.block.seq;
            engine.blocks_by_cid.insert(tracked.block_cid, (actor, seq));
            if let Some(n) = tracked.block.nullifier {
                engine.consumed_nullifiers.insert(n);
            }
            engine.chains.entry((actor, seq)).or_default().push(tracked);
        }

        let cancel_tbl = read
            .open_table(OBT_CANCELLED_SENDS_TABLE)
            .map_err(|e| ObtStoreError::Redb(e.to_string()))?;
        for entry in cancel_tbl
            .iter()
            .map_err(|e| ObtStoreError::Redb(e.to_string()))?
        {
            let (k, _) = entry.map_err(|e| ObtStoreError::Redb(e.to_string()))?;
            let cid: [u8; 32] = k
                .value()
                .try_into()
                .map_err(|_| ObtStoreError::CorruptRow("cancelled_send"))?;
            engine.cancelled_sends.insert(cid);
        }

        let warrant_tbl = read
            .open_table(OBT_WARRANTS_TABLE)
            .map_err(|e| ObtStoreError::Redb(e.to_string()))?;
        for entry in warrant_tbl
            .iter()
            .map_err(|e| ObtStoreError::Redb(e.to_string()))?
        {
            let (_, val) = entry.map_err(|e| ObtStoreError::Redb(e.to_string()))?;
            let warrant = AccountEquivocationWarrant::decode_canonical_bytes(val.value())?;
            engine.warrants.insert(warrant.warrant_cid, warrant);
        }

        let cp_tbl = read
            .open_table(OBT_CHECKPOINTS_TABLE)
            .map_err(|e| ObtStoreError::Redb(e.to_string()))?;
        for entry in cp_tbl
            .iter()
            .map_err(|e| ObtStoreError::Redb(e.to_string()))?
        {
            let (_, val) = entry.map_err(|e| ObtStoreError::Redb(e.to_string()))?;
            let cp = SettlementDomainCheckpoint::decode_canonical_bytes(val.value())?;
            engine
                .checkpoints
                .insert((cp.domain_id, cp.checkpoint_seq), cp);
        }

        Ok(engine)
    }
}

impl FirewallSinkBackend for RedbObtStore {
    fn put_outbox_notice(&mut self, notice: &ObtNoticeV2) -> Result<bool, String> {
        let id = notice.notice_id();
        let bytes = notice.encode_canonical_bytes().map_err(|e| e.to_string())?;
        let write = self.db.begin_write().map_err(|e| e.to_string())?;
        let inserted;
        {
            let mut tbl = write
                .open_table(OBT_NOTICE_OUTBOX_V2_TABLE)
                .map_err(|e| e.to_string())?;
            if tbl.get(&id[..]).map_err(|e| e.to_string())?.is_some() {
                inserted = false;
            } else {
                tbl.insert(&id[..], bytes.as_slice())
                    .map_err(|e| e.to_string())?;
                inserted = true;
            }
        }
        write.commit().map_err(|e| e.to_string())?;
        Ok(inserted)
    }

    fn remove_outbox_notice(&mut self, notice_id: &[u8; 32]) -> Result<(), String> {
        let write = self.db.begin_write().map_err(|e| e.to_string())?;
        {
            let mut tbl = write
                .open_table(OBT_NOTICE_OUTBOX_V2_TABLE)
                .map_err(|e| e.to_string())?;
            tbl.remove(&notice_id[..]).map_err(|e| e.to_string())?;
        }
        write.commit().map_err(|e| e.to_string())
    }

    fn put_shadow_evaluation(&mut self, eval: &ShadowClaimEvaluation) -> Result<(), String> {
        let bytes = eval.encode_canonical_bytes().map_err(|e| e.to_string())?;
        let write = self.db.begin_write().map_err(|e| e.to_string())?;
        {
            let mut tbl = write
                .open_table(OBT_SHADOW_EVALS_TABLE)
                .map_err(|e| e.to_string())?;
            tbl.insert(&eval.evaluation_id[..], bytes.as_slice())
                .map_err(|e| e.to_string())?;
        }
        write.commit().map_err(|e| e.to_string())
    }

    fn put_pending_mint_from_firewall(&mut self, mint: &PendingMintRecord) -> Result<(), String> {
        let bytes = mint.encode_canonical_bytes().map_err(|e| e.to_string())?;
        let write = self.db.begin_write().map_err(|e| e.to_string())?;
        {
            let mut tbl = write
                .open_table(OBT_PENDING_MINTS_TABLE)
                .map_err(|e| e.to_string())?;
            tbl.insert(&mint.pending_mint_cid[..], bytes.as_slice())
                .map_err(|e| e.to_string())?;
        }
        write.commit().map_err(|e| e.to_string())
    }

    fn put_quarantined_notice(&mut self, quarantined: &QuarantinedNotice) -> Result<(), String> {
        let write = self.db.begin_write().map_err(|e| e.to_string())?;
        {
            let mut tbl = write
                .open_table(OBT_NOTICE_QUARANTINE_TABLE)
                .map_err(|e| e.to_string())?;
            tbl.insert(&quarantined.notice_id[..], quarantined.reason.as_bytes())
                .map_err(|e| e.to_string())?;
        }
        write.commit().map_err(|e| e.to_string())
    }
}

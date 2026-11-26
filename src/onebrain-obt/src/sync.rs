//! Bounded canonical CBOR wire codecs and idempotent delta reconciliation for `obp/obt-ledger/1` (`§2.1`, `§5.2–5.3`).
//!
//! Disconnected offline islands accumulate `PendingMintRecord`s, `FinalizedMintCertificate`s,
//! and `ProvisionalPartitionLocal` `AccountBlockV1`s with zero offline slashing (`NEG-P5-002`),
//! then reconcile idempotently and converge to `SettlementState::FinalInDomain` upon reunion.

use std::collections::{BTreeMap, BTreeSet};

use ku_core::foundation::{
    ActorId, CanonicalError, CanonicalErrorKind, CanonicalValue, FeedId, KeyStateReducer,
    ResourceProfile, ValidatedFeedInception,
};
use thiserror::Error;

use crate::account_chain::{
    AccountBlockOp, AccountBlockV1, AccountChainEngine, AccountChainError,
    AccountEquivocationWarrant, BlockInsertOutcome, SettlementDomainCheckpoint,
};
use crate::codec::{
    decode_value, encode_actor, encode_value, expect_actor, expect_array, expect_bytes_exact,
    expect_map, expect_u64, req_field,
};
use crate::firewall_in::ObtFeatureFlags;
use crate::mint_pipeline::{FinalizedMintCertificate, MintPipeline, PendingMintRecord};

/// Negotiated capability protocol identifier (`§2.1`).
pub const OBT_LEDGER_PROTOCOL_ID: &str = "obp/obt-ledger/1";

pub const MAX_SYNC_PENDING_MINTS: usize = 512;
pub const MAX_SYNC_FINALIZED_MINTS: usize = 512;
pub const MAX_SYNC_ACCOUNT_BLOCKS: usize = 1_024;
pub const MAX_SYNC_WARRANTS: usize = 256;
pub const MAX_SYNC_CHECKPOINTS: usize = 128;

/// Errors returned by `ObtLedgerSyncEngine`.
#[derive(Clone, Debug, PartialEq, Eq, Error)]
pub enum ObtSyncError {
    #[error("OBT_SYNC_DISABLED: obp/obt-ledger/1 is disabled or kill-switched")]
    ProtocolDisabled,
    #[error("OBT_SYNC_CANONICAL: {0}")]
    Canonical(#[from] CanonicalError),
    #[error("OBT_SYNC_BATCH_LIMIT_EXCEEDED: {0}")]
    BatchLimitExceeded(&'static str),
    #[error(
        "OBT_SYNC_MISSING_SIGNER_FEED: feed inception for {0} not present in SyncAuthorityContext"
    )]
    MissingSignerFeed(FeedId),
    #[error("OBT_SYNC_CHAIN: {0}")]
    AccountChain(#[from] AccountChainError),
}

/// Compact `Have` summary advertised by a peer opening an `obp/obt-ledger/1` reconciliation round.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct ObtLedgerSyncSummary {
    pub account_block_cids: Vec<[u8; 32]>,
    pub pending_mint_cids: Vec<[u8; 32]>,
    pub finalized_mint_cids: Vec<[u8; 32]>,
    pub warrant_cids: Vec<[u8; 32]>,
    pub checkpoints: Vec<([u8; 32], u64)>,
}

impl ObtLedgerSyncSummary {
    pub fn to_canonical_value(&self) -> CanonicalValue {
        let mut blks = self.account_block_cids.clone();
        blks.sort();
        blks.dedup();
        let mut pms = self.pending_mint_cids.clone();
        pms.sort();
        pms.dedup();
        let mut fms = self.finalized_mint_cids.clone();
        fms.sort();
        fms.dedup();
        let mut ws = self.warrant_cids.clone();
        ws.sort();
        ws.dedup();
        let mut cps = self.checkpoints.clone();
        cps.sort();
        cps.dedup();

        CanonicalValue::Map(vec![
            (
                0,
                CanonicalValue::Array(
                    blks.into_iter()
                        .map(|c| CanonicalValue::Bytes(c.to_vec()))
                        .collect(),
                ),
            ),
            (
                1,
                CanonicalValue::Array(
                    pms.into_iter()
                        .map(|c| CanonicalValue::Bytes(c.to_vec()))
                        .collect(),
                ),
            ),
            (
                2,
                CanonicalValue::Array(
                    fms.into_iter()
                        .map(|c| CanonicalValue::Bytes(c.to_vec()))
                        .collect(),
                ),
            ),
            (
                3,
                CanonicalValue::Array(
                    ws.into_iter()
                        .map(|c| CanonicalValue::Bytes(c.to_vec()))
                        .collect(),
                ),
            ),
            (
                4,
                CanonicalValue::Array(
                    cps.into_iter()
                        .map(|(dom, seq)| {
                            CanonicalValue::Array(vec![
                                CanonicalValue::Bytes(dom.to_vec()),
                                CanonicalValue::Unsigned(seq),
                            ])
                        })
                        .collect(),
                ),
            ),
        ])
    }

    pub fn from_canonical_value(value: &CanonicalValue) -> Result<Self, ObtSyncError> {
        let entries = expect_map(value, &[0, 1, 2, 3, 4])?;
        let parse_cids =
            |v: &CanonicalValue, limit: usize| -> Result<Vec<[u8; 32]>, ObtSyncError> {
                let arr = expect_array(v)?;
                if arr.len() > limit {
                    return Err(ObtSyncError::BatchLimitExceeded("summary_cids"));
                }
                arr.iter()
                    .map(|item| expect_bytes_exact::<32>(item).map_err(ObtSyncError::Canonical))
                    .collect()
            };
        let cp_arr = expect_array(req_field(entries, 4)?)?;
        if cp_arr.len() > MAX_SYNC_CHECKPOINTS {
            return Err(ObtSyncError::BatchLimitExceeded("summary_checkpoints"));
        }
        let mut checkpoints = Vec::with_capacity(cp_arr.len());
        for item in cp_arr {
            let pair = expect_array(item)?;
            if pair.len() != 2 {
                return Err(ObtSyncError::Canonical(CanonicalError::new(
                    CanonicalErrorKind::ForbiddenType,
                    0,
                )));
            }
            checkpoints.push((expect_bytes_exact::<32>(&pair[0])?, expect_u64(&pair[1])?));
        }

        Ok(Self {
            account_block_cids: parse_cids(req_field(entries, 0)?, MAX_SYNC_ACCOUNT_BLOCKS)?,
            pending_mint_cids: parse_cids(req_field(entries, 1)?, MAX_SYNC_PENDING_MINTS)?,
            finalized_mint_cids: parse_cids(req_field(entries, 2)?, MAX_SYNC_FINALIZED_MINTS)?,
            warrant_cids: parse_cids(req_field(entries, 3)?, MAX_SYNC_WARRANTS)?,
            checkpoints,
        })
    }
}

/// Bounded delta payload exchanged over `obp/obt-ledger/1`.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct ObtLedgerDeltaBatch {
    pub pending_mints: Vec<PendingMintRecord>,
    pub finalized_mints: Vec<FinalizedMintCertificate>,
    pub account_blocks: Vec<AccountBlockV1>,
    pub equivocation_warrants: Vec<AccountEquivocationWarrant>,
    pub checkpoints: Vec<SettlementDomainCheckpoint>,
}

impl ObtLedgerDeltaBatch {
    pub fn validate_bounds(&self) -> Result<(), ObtSyncError> {
        if self.pending_mints.len() > MAX_SYNC_PENDING_MINTS {
            return Err(ObtSyncError::BatchLimitExceeded("pending_mints"));
        }
        if self.finalized_mints.len() > MAX_SYNC_FINALIZED_MINTS {
            return Err(ObtSyncError::BatchLimitExceeded("finalized_mints"));
        }
        if self.account_blocks.len() > MAX_SYNC_ACCOUNT_BLOCKS {
            return Err(ObtSyncError::BatchLimitExceeded("account_blocks"));
        }
        if self.equivocation_warrants.len() > MAX_SYNC_WARRANTS {
            return Err(ObtSyncError::BatchLimitExceeded("equivocation_warrants"));
        }
        if self.checkpoints.len() > MAX_SYNC_CHECKPOINTS {
            return Err(ObtSyncError::BatchLimitExceeded("checkpoints"));
        }
        Ok(())
    }

    pub fn to_canonical_value(&self) -> CanonicalValue {
        CanonicalValue::Map(vec![
            (
                0,
                CanonicalValue::Array(
                    self.pending_mints
                        .iter()
                        .map(PendingMintRecord::to_canonical_value)
                        .collect(),
                ),
            ),
            (
                1,
                CanonicalValue::Array(
                    self.finalized_mints
                        .iter()
                        .map(FinalizedMintCertificate::to_canonical_value)
                        .collect(),
                ),
            ),
            (
                2,
                CanonicalValue::Array(
                    self.account_blocks
                        .iter()
                        .map(AccountBlockV1::to_canonical_value)
                        .collect(),
                ),
            ),
            (
                3,
                CanonicalValue::Array(
                    self.equivocation_warrants
                        .iter()
                        .map(AccountEquivocationWarrant::to_canonical_value)
                        .collect(),
                ),
            ),
            (
                4,
                CanonicalValue::Array(
                    self.checkpoints
                        .iter()
                        .map(SettlementDomainCheckpoint::to_canonical_value)
                        .collect(),
                ),
            ),
        ])
    }

    pub fn from_canonical_value(value: &CanonicalValue) -> Result<Self, ObtSyncError> {
        let entries = expect_map(value, &[0, 1, 2, 3, 4])?;

        let pm_arr = expect_array(req_field(entries, 0)?)?;
        if pm_arr.len() > MAX_SYNC_PENDING_MINTS {
            return Err(ObtSyncError::BatchLimitExceeded("pending_mints"));
        }
        let mut pending_mints = Vec::with_capacity(pm_arr.len());
        for item in pm_arr {
            let bytes = encode_value(item, ResourceProfile::ControlV1)?;
            pending_mints.push(PendingMintRecord::decode_canonical_bytes(&bytes)?);
        }

        let fm_arr = expect_array(req_field(entries, 1)?)?;
        if fm_arr.len() > MAX_SYNC_FINALIZED_MINTS {
            return Err(ObtSyncError::BatchLimitExceeded("finalized_mints"));
        }
        let mut finalized_mints = Vec::with_capacity(fm_arr.len());
        for item in fm_arr {
            let bytes = encode_value(item, ResourceProfile::ControlV1)?;
            finalized_mints.push(FinalizedMintCertificate::decode_canonical_bytes(&bytes)?);
        }

        let blk_arr = expect_array(req_field(entries, 2)?)?;
        if blk_arr.len() > MAX_SYNC_ACCOUNT_BLOCKS {
            return Err(ObtSyncError::BatchLimitExceeded("account_blocks"));
        }
        let mut account_blocks = Vec::with_capacity(blk_arr.len());
        for item in blk_arr {
            let bytes = encode_value(item, ResourceProfile::ControlV1)?;
            account_blocks.push(AccountBlockV1::decode_canonical_bytes(&bytes)?);
        }

        let w_arr = expect_array(req_field(entries, 3)?)?;
        if w_arr.len() > MAX_SYNC_WARRANTS {
            return Err(ObtSyncError::BatchLimitExceeded("equivocation_warrants"));
        }
        let mut equivocation_warrants = Vec::with_capacity(w_arr.len());
        for item in w_arr {
            let bytes = encode_value(item, ResourceProfile::ObjectV1)?;
            equivocation_warrants.push(AccountEquivocationWarrant::decode_canonical_bytes(&bytes)?);
        }

        let cp_arr = expect_array(req_field(entries, 4)?)?;
        if cp_arr.len() > MAX_SYNC_CHECKPOINTS {
            return Err(ObtSyncError::BatchLimitExceeded("checkpoints"));
        }
        let mut checkpoints = Vec::with_capacity(cp_arr.len());
        for item in cp_arr {
            let bytes = encode_value(item, ResourceProfile::ManifestV1)?;
            checkpoints.push(SettlementDomainCheckpoint::decode_canonical_bytes(&bytes)?);
        }

        Ok(Self {
            pending_mints,
            finalized_mints,
            account_blocks,
            equivocation_warrants,
            checkpoints,
        })
    }
}

/// Wire frame envelope for `obp/obt-ledger/1`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ObtLedgerWireMessage {
    HaveSummary(ObtLedgerSyncSummary),
    DeltaBatch(ObtLedgerDeltaBatch),
}

impl ObtLedgerWireMessage {
    pub fn encode_wire_bytes(&self) -> Result<Vec<u8>, ObtSyncError> {
        let value = match self {
            Self::HaveSummary(summary) => CanonicalValue::Map(vec![
                (0, CanonicalValue::Text(OBT_LEDGER_PROTOCOL_ID.to_owned())),
                (1, CanonicalValue::Unsigned(0)),
                (2, summary.to_canonical_value()),
            ]),
            Self::DeltaBatch(batch) => {
                batch.validate_bounds()?;
                CanonicalValue::Map(vec![
                    (0, CanonicalValue::Text(OBT_LEDGER_PROTOCOL_ID.to_owned())),
                    (1, CanonicalValue::Unsigned(1)),
                    (2, batch.to_canonical_value()),
                ])
            }
        };
        Ok(encode_value(&value, ResourceProfile::ManifestV1)?)
    }

    pub fn decode_wire_bytes(bytes: &[u8]) -> Result<Self, ObtSyncError> {
        let value = decode_value(bytes, ResourceProfile::ManifestV1)?;
        let entries = expect_map(&value, &[0, 1, 2])?;
        let CanonicalValue::Text(proto) = req_field(entries, 0)? else {
            return Err(ObtSyncError::Canonical(CanonicalError::new(
                CanonicalErrorKind::ForbiddenType,
                0,
            )));
        };
        if proto != OBT_LEDGER_PROTOCOL_ID {
            return Err(ObtSyncError::Canonical(CanonicalError::new(
                CanonicalErrorKind::UnknownField,
                0,
            )));
        }
        let _ = (encode_actor, expect_actor);
        match expect_u64(req_field(entries, 1)?)? {
            0 => Ok(Self::HaveSummary(
                ObtLedgerSyncSummary::from_canonical_value(req_field(entries, 2)?)?,
            )),
            1 => Ok(Self::DeltaBatch(ObtLedgerDeltaBatch::from_canonical_value(
                req_field(entries, 2)?,
            )?)),
            _ => Err(ObtSyncError::Canonical(CanonicalError::new(
                CanonicalErrorKind::ForbiddenType,
                0,
            ))),
        }
    }
}

/// Authority verification context supplied at reconciliation time.
pub struct SyncAuthorityContext<'a> {
    pub feeds: &'a BTreeMap<FeedId, ValidatedFeedInception>,
    pub key_state: &'a KeyStateReducer,
    pub authority_observed_at_secs: u64,
    pub current_local_secs: u64,
    pub accept_provisional_receives: bool,
}

/// Result summary of applying an `ObtLedgerDeltaBatch`.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct ObtSyncApplyReport {
    pub pending_mints_added: usize,
    pub finalized_mints_added: usize,
    pub blocks_accepted: usize,
    pub blocks_already_present: usize,
    pub warrants_recorded: usize,
    pub checkpoints_applied: usize,
}

/// Idempotent delta reconciliation engine for `obp/obt-ledger/1`.
pub struct ObtLedgerSyncEngine {
    pub flags: ObtFeatureFlags,
    pub mint_pipeline: MintPipeline,
    pub account_chain: AccountChainEngine,
}

impl ObtLedgerSyncEngine {
    pub fn new(
        flags: ObtFeatureFlags,
        mint_pipeline: MintPipeline,
        account_chain: AccountChainEngine,
    ) -> Self {
        Self {
            flags,
            mint_pipeline,
            account_chain,
        }
    }

    fn ensure_enabled(&self) -> Result<(), ObtSyncError> {
        if !self.flags.ledger_sync_allowed() {
            Err(ObtSyncError::ProtocolDisabled)
        } else {
            Ok(())
        }
    }

    pub fn build_summary(&self) -> Result<ObtLedgerSyncSummary, ObtSyncError> {
        self.ensure_enabled()?;
        Ok(ObtLedgerSyncSummary {
            account_block_cids: self.account_chain.blocks_by_cid.keys().copied().collect(),
            pending_mint_cids: self.mint_pipeline.pending_mints.keys().copied().collect(),
            finalized_mint_cids: self.mint_pipeline.finalized_mints.keys().copied().collect(),
            warrant_cids: self.account_chain.warrants.keys().copied().collect(),
            checkpoints: self.account_chain.checkpoints.keys().copied().collect(),
        })
    }

    pub fn compute_delta_for_peer(
        &self,
        peer_summary: &ObtLedgerSyncSummary,
    ) -> Result<ObtLedgerDeltaBatch, ObtSyncError> {
        self.ensure_enabled()?;
        let peer_blocks: BTreeSet<[u8; 32]> =
            peer_summary.account_block_cids.iter().copied().collect();
        let peer_pms: BTreeSet<[u8; 32]> = peer_summary.pending_mint_cids.iter().copied().collect();
        let peer_fms: BTreeSet<[u8; 32]> =
            peer_summary.finalized_mint_cids.iter().copied().collect();
        let peer_warrants: BTreeSet<[u8; 32]> = peer_summary.warrant_cids.iter().copied().collect();
        let peer_cps: BTreeSet<([u8; 32], u64)> =
            peer_summary.checkpoints.iter().copied().collect();

        let pending_mints = self
            .mint_pipeline
            .pending_mints
            .iter()
            .filter(|(cid, _)| !peer_pms.contains(*cid))
            .take(MAX_SYNC_PENDING_MINTS)
            .map(|(_, rec)| rec.clone())
            .collect();

        let finalized_mints = self
            .mint_pipeline
            .finalized_mints
            .iter()
            .filter(|(cid, _)| !peer_fms.contains(*cid))
            .take(MAX_SYNC_FINALIZED_MINTS)
            .map(|(_, cert)| cert.clone())
            .collect();

        // Emit missing account blocks sorted so non-receives (Open/Credit/Send) precede ReceiveTransfer,
        // and lower `seq` precedes higher `seq`, enabling single-pass causal replay across accounts!
        let mut missing_blocks: Vec<AccountBlockV1> = self
            .account_chain
            .chains
            .values()
            .flat_map(|branches| branches.iter())
            .filter(|tracked| !peer_blocks.contains(&tracked.block_cid))
            .map(|tracked| tracked.block.clone())
            .collect();
        missing_blocks.sort_by_key(causal_block_order_key);
        missing_blocks.truncate(MAX_SYNC_ACCOUNT_BLOCKS);

        let equivocation_warrants = self
            .account_chain
            .warrants
            .iter()
            .filter(|(cid, _)| !peer_warrants.contains(*cid))
            .take(MAX_SYNC_WARRANTS)
            .map(|(_, w)| w.clone())
            .collect();

        let checkpoints = self
            .account_chain
            .checkpoints
            .iter()
            .filter(|(k, _)| !peer_cps.contains(*k))
            .take(MAX_SYNC_CHECKPOINTS)
            .map(|(_, cp)| cp.clone())
            .collect();

        Ok(ObtLedgerDeltaBatch {
            pending_mints,
            finalized_mints,
            account_blocks: missing_blocks,
            equivocation_warrants,
            checkpoints,
        })
    }

    /// Apply an incoming `ObtLedgerDeltaBatch` idempotently.
    pub fn apply_delta(
        &mut self,
        batch: &ObtLedgerDeltaBatch,
        ctx: &SyncAuthorityContext<'_>,
    ) -> Result<ObtSyncApplyReport, ObtSyncError> {
        self.ensure_enabled()?;
        batch.validate_bounds()?;
        let mut report = ObtSyncApplyReport::default();

        for pm in &batch.pending_mints {
            if let std::collections::btree_map::Entry::Vacant(slot) =
                self.mint_pipeline.pending_mints.entry(pm.pending_mint_cid)
            {
                slot.insert(pm.clone());
                report.pending_mints_added += 1;
            }
        }

        for cert in &batch.finalized_mints {
            cert.verify_self_contained()
                .map_err(AccountChainError::MintCertificate)?;
            if let std::collections::btree_map::Entry::Vacant(slot) = self
                .mint_pipeline
                .finalized_mints
                .entry(cert.certificate_cid)
            {
                slot.insert(cert.clone());
                report.finalized_mints_added += 1;
            }
        }

        // Topological multi-pass application over `batch.account_blocks` so arbitrary cross-account
        // `SendTransfer -> ReceiveTransfer` chains apply cleanly regardless of wire order.
        let mut remaining: Vec<AccountBlockV1> = batch.account_blocks.clone();
        remaining.sort_by_key(causal_block_order_key);

        loop {
            if remaining.is_empty() {
                break;
            }
            let mut next_round = Vec::new();
            let mut progressed = false;
            let mut last_err: Option<ObtSyncError> = None;

            for blk in remaining {
                let feed = ctx
                    .feeds
                    .get(&blk.signer_feed)
                    .ok_or(ObtSyncError::MissingSignerFeed(blk.signer_feed))?;
                let had_warrants_before = self.account_chain.warrants.len();
                match self.account_chain.ingest_synced_block(
                    blk.clone(),
                    feed,
                    ctx.key_state,
                    ctx.authority_observed_at_secs,
                    ctx.current_local_secs,
                    ctx.accept_provisional_receives,
                ) {
                    Ok(BlockInsertOutcome::AcceptedProvisional { .. }) => {
                        report.blocks_accepted += 1;
                        progressed = true;
                    }
                    Ok(BlockInsertOutcome::AlreadyPresent { .. }) => {
                        report.blocks_already_present += 1;
                        progressed = true;
                    }
                    Ok(BlockInsertOutcome::EquivocationFrozen { .. }) => {
                        report.blocks_accepted += 1;
                        if self.account_chain.warrants.len() > had_warrants_before {
                            report.warrants_recorded += 1;
                        }
                        progressed = true;
                    }
                    Err(
                        err @ (AccountChainError::InvalidSequence { .. }
                        | AccountChainError::UnknownOrMismatchedSendBlock),
                    ) => {
                        last_err = Some(ObtSyncError::AccountChain(err));
                        next_round.push(blk);
                    }
                    Err(err) => return Err(ObtSyncError::AccountChain(err)),
                }
            }

            if !progressed {
                return Err(last_err.expect("non-empty unprogressed round has error"));
            }
            remaining = next_round;
        }

        for warrant in &batch.equivocation_warrants {
            let existed = self
                .account_chain
                .warrants
                .contains_key(&warrant.warrant_cid);
            self.account_chain
                .ingest_equivocation_warrant(warrant.clone())?;
            if !existed {
                report.warrants_recorded += 1;
            }
        }

        let mut cps = batch.checkpoints.clone();
        cps.sort_by_key(|cp| (cp.domain_id, cp.checkpoint_seq));
        for cp in cps {
            let key = (cp.domain_id, cp.checkpoint_seq);
            let existed = self.account_chain.checkpoints.contains_key(&key);
            self.account_chain.apply_domain_checkpoint(cp)?;
            if !existed {
                report.checkpoints_applied += 1;
            }
        }

        Ok(report)
    }
}

fn causal_block_order_key(blk: &AccountBlockV1) -> (u64, u8, ActorId, [u8; 32]) {
    let op_phase = match blk.op {
        AccountBlockOp::OpenAccount { .. } => 0,
        AccountBlockOp::CreditFinalizedMint { .. } => 1,
        AccountBlockOp::SendTransfer { .. } => 2,
        AccountBlockOp::ReceiveTransfer { .. } => 3,
        AccountBlockOp::CancelExpiredSend { .. } => 4,
    };
    (blk.seq, op_phase, blk.account, blk.block_cid())
}

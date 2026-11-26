#![forbid(unsafe_code)]
#![deny(
    clippy::float_arithmetic,
    clippy::float_cmp,
    clippy::cast_precision_loss
)]
//! Isolated Pillar 5 OBT dual-track benefit and accepted-work economy (`P5-OBT-ECONOMY-VNEXT-001`).
//!
//! Enforces strict one-way post-commit isolation (`FW-07`, `NEG-OBT-001`),
//! integer-only 128-bit atomic accounting with 256-bit intermediate budget math (`FND-003`),
//! geography- and identity-neutral reward policies (`§3.3`), dual-track issuance
//! (`D-014` Track A accepted work with evidenced-mismatch symmetry + `M7` Track B
//! attributed benefit), pre-finality challenge windows with non-grindable VRF audit
//! selection (`§5.1`), and CAP-honest `ActorId` account chains (`§5.2–5.3`).

pub mod account_chain;
pub mod claims;
pub(crate) mod codec;
pub mod firewall_in;
pub mod mint_pipeline;
pub mod nullifier;
pub mod policy;
pub mod store;
pub mod sync;
pub mod units;

pub use account_chain::{
    AccountBlockOp, AccountBlockV1, AccountChainEngine, AccountChainError,
    AccountEquivocationWarrant, BlockInsertOutcome, SettlementDomainCheckpoint,
    SettlementDomainManifest, SettlementDomainValidator, SettlementState, ValidatorCheckpointSig,
    ACCOUNT_BLOCK_DOMAIN, ACCOUNT_WARRANT_DOMAIN, EFFECT_OBT_LEDGER_SIGN,
    PROVISIONAL_SETTLEMENT_UI_BADGE, R4_LIVE_FRESHNESS_MAX_AGE_SECS, SETTLEMENT_CHECKPOINT_DOMAIN,
};
pub use claims::{
    extract_4d_correlation_group, AttributedPayout, AttributionGraph, BenefitClosureState,
    BenefitDistribution, BenefitRewardClaimV1, BlindCommitRevealProof, ClaimError,
    ClaimEvaluationOutcome, ClaimEvaluator, ContributorRole, CorrelationGroupId,
    DelegatedWorkJobProof, DerivationContributorNode, EpochCapTracker, OutcomeObservationSummary,
    SemanticMismatchKind, UseOrDerivationHop, VerifierGroupAttestation, WorkRewardClaimV1,
    WorkVerdict,
};
pub use firewall_in::{
    AcceptedWorkKind, FirewallDrainReport, FirewallDropReason, FirewallSinkBackend,
    InMemoryFirewallSink, NoticeEvaluationCandidate, NoticeIngestOutcome, ObtEconomicStatus,
    ObtFeatureFlags, ObtFirewallConsumer, ObtNoticeV2, ObtRuntimeMode, QuarantinedNotice,
    ShadowClaimEvaluation,
};
pub use mint_pipeline::{
    select_challenge_auditors, AuditCoSignature, ChallengeDispute, FinalizedMintCertificate,
    MintLifecycleState, MintPipeline, MintPipelineError, PendingMintRecord, PendingMintSource,
    AUDIT_VRF_DOMAIN, FINALIZED_MINT_DOMAIN, PENDING_MINT_DOMAIN,
};
pub use nullifier::{
    ScopedNullifier, WorkRoleSlot, BENEFIT_NULLIFIER_DOMAIN, WORK_NULLIFIER_DOMAIN,
};
pub use policy::{
    CausalSupportTier, CorroborationTier, EvidenceQualityTier, PolicyError, RewardPolicyV1,
    WorkClass, WorkClassWeights, MAX_RESOLVED_CONSTRAINTS_CAP, MAX_VERIFIED_STATEMENTS_CAP,
    REWARD_POLICY_DOMAIN,
};
pub use store::{ObtStoreError, RedbObtStore};
pub use sync::{
    ObtLedgerDeltaBatch, ObtLedgerSyncEngine, ObtLedgerSyncSummary, ObtLedgerWireMessage,
    ObtSyncApplyReport, ObtSyncError, SyncAuthorityContext, OBT_LEDGER_PROTOCOL_ID,
};
pub use units::{
    compute_benefit_budget, ObtAmount, ObtMathError, BPS_DENOMINATOR, MAX_FACTOR_BPS,
    QUANTA_PER_OBT, U256,
};

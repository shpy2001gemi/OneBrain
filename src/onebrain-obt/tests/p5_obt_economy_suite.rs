#![forbid(unsafe_code)]
#![deny(
    clippy::float_arithmetic,
    clippy::float_cmp,
    clippy::cast_precision_loss
)]

use std::collections::BTreeMap;

use ed25519_dalek::SigningKey;
use ku_core::foundation::{
    decode_feed_inception, AcceptedRevocation, ActorId, AttributionStatus, ConceptCcid,
    CorrelationDimension, CorrelationDimensionEvidence, CorrelationEvidence, DelegationGrant,
    DeviceId, EventCid, EvidenceLimitation, EvidenceStrength, FeedInception, KeyStateApplyOutcome,
    KeyStateReducer, NamespaceCommitment, ObjectCid, OutcomeValence, PermitCid, ScopedDelegation,
    ScopedRevocation, ValidatedFeedInception,
};
use onebrain_obt::{
    compute_benefit_budget, select_challenge_auditors, AcceptedWorkKind, AccountBlockOp,
    AccountBlockV1, AccountChainEngine, AccountChainError, AttributionGraph, AuditCoSignature,
    BenefitClosureState, BenefitRewardClaimV1, BlindCommitRevealProof, BlockInsertOutcome,
    CausalSupportTier, ChallengeDispute, ClaimError, ClaimEvaluationOutcome, ClaimEvaluator,
    ContributorRole, DelegatedWorkJobProof, DerivationContributorNode, EpochCapTracker,
    FirewallDropReason, InMemoryFirewallSink, MintLifecycleState, MintPipeline, MintPipelineError,
    NoticeEvaluationCandidate, NoticeIngestOutcome, ObtAmount, ObtEconomicStatus, ObtFeatureFlags,
    ObtFirewallConsumer, ObtLedgerSyncEngine, ObtLedgerWireMessage, ObtMathError, ObtNoticeV2,
    ObtRuntimeMode, ObtSyncError, OutcomeObservationSummary, PendingMintRecord, PendingMintSource,
    RedbObtStore, RewardPolicyV1, ScopedNullifier, SemanticMismatchKind,
    SettlementDomainCheckpoint, SettlementDomainManifest, SettlementDomainValidator,
    SettlementState, SyncAuthorityContext, UseOrDerivationHop, VerifierGroupAttestation, WorkClass,
    WorkRewardClaimV1, WorkRoleSlot, WorkVerdict, BPS_DENOMINATOR, MAX_FACTOR_BPS,
    OBT_LEDGER_PROTOCOL_ID, PROVISIONAL_SETTLEMENT_UI_BADGE, QUANTA_PER_OBT,
};

struct ActorFixture {
    actor: ActorId,
    device: DeviceId,
    signing_key: SigningKey,
    delegation_ref: EventCid,
    feed: ValidatedFeedInception,
}

fn make_actor_fixture(seed: u8, key_state: &mut KeyStateReducer) -> ActorFixture {
    let actor = ActorId::from_bytes([seed; 32]);
    let device = DeviceId::from_bytes([seed.wrapping_add(100); 32]);
    let signing_key = SigningKey::from_bytes(&[seed.wrapping_add(50); 32]);
    let delegation_ref = EventCid::from_bytes([seed.wrapping_add(150); 32]);
    let ns = NamespaceCommitment::derive(b"obt-p5-test", [seed; 32]).unwrap();
    let mut inception = FeedInception::new(*signing_key.verifying_key().as_bytes(), ns, 0, device);
    inception.actor_delegation_ref = Some(*delegation_ref.as_bytes());
    let signed = inception.sign(&signing_key).unwrap();
    let feed = decode_feed_inception(&signed.encode().unwrap()).unwrap();

    let outcome = key_state.accept_root(ScopedDelegation {
        grant: DelegationGrant {
            actor,
            device,
            subject_feed: feed.feed_id,
            delegation_ref,
            namespace_commitment: None,
            first_generation: 0,
            last_generation: 10,
            proof: delegation_ref,
        },
        parent_delegation_ref: None,
    });
    assert!(matches!(
        outcome,
        KeyStateApplyOutcome::Accepted | KeyStateApplyOutcome::AlreadyPresent
    ));

    ActorFixture {
        actor,
        device,
        signing_key,
        delegation_ref,
        feed,
    }
}

fn make_4d_correlation(
    admin_seed: u8,
    model_seed: u8,
    env_seed: u8,
    source_seed: u8,
    strength: EvidenceStrength,
) -> CorrelationEvidence {
    CorrelationEvidence {
        dimensions: vec![
            CorrelationDimensionEvidence {
                dimension: CorrelationDimension::AdministrativePrincipal,
                value_commitment: (strength != EvidenceStrength::Unknown)
                    .then_some([admin_seed; 32]),
                strength,
                evidence_refs: vec![],
            },
            CorrelationDimensionEvidence {
                dimension: CorrelationDimension::PipelineModelLineage,
                value_commitment: (strength != EvidenceStrength::Unknown)
                    .then_some([model_seed; 32]),
                strength,
                evidence_refs: vec![],
            },
            CorrelationDimensionEvidence {
                dimension: CorrelationDimension::ExecutionEnvironment,
                value_commitment: (strength != EvidenceStrength::Unknown).then_some([env_seed; 32]),
                strength,
                evidence_refs: vec![],
            },
            CorrelationDimensionEvidence {
                dimension: CorrelationDimension::SourceAcquisitionOrDerivation,
                value_commitment: (strength != EvidenceStrength::Unknown)
                    .then_some([source_seed; 32]),
                strength,
                evidence_refs: vec![],
            },
        ],
    }
}

fn sample_work_claim(
    policy_cid: ObjectCid,
    verdict: WorkVerdict,
    stmts: u32,
    constraints: u32,
) -> WorkRewardClaimV1 {
    WorkRewardClaimV1 {
        worker_actor_id: ActorId::from_bytes([10; 32]),
        work_class: WorkClass::BlindFidelityVerify,
        role_slot: WorkRoleSlot::blind_verifier(0),
        epoch_index: 1,
        job_proof: DelegatedWorkJobProof {
            job_id: [21; 32],
            requester_actor_id: ActorId::from_bytes([99; 32]),
            permit_cid: PermitCid::from_bytes([22; 32]),
            source_artifact_cid: ObjectCid::from_bytes([23; 32]),
            compiler_profile_cid: ObjectCid::from_bytes([24; 32]),
        },
        blind_proof: BlindCommitRevealProof::new_committed(
            ObjectCid::from_bytes([25; 32]),
            [26; 32],
            [27; 32],
        ),
        verdict,
        verified_statement_count: stmts,
        resolved_constraint_count: constraints,
        worker_correlation: make_4d_correlation(10, 10, 10, 10, EvidenceStrength::CryptoBound),
        verifier_attestations: vec![
            VerifierGroupAttestation {
                attestation_event_cid: EventCid::from_bytes([31; 32]),
                verifier_actor_id: ActorId::from_bytes([11; 32]),
                correlation_evidence: make_4d_correlation(
                    11,
                    21,
                    31,
                    41,
                    EvidenceStrength::CryptoBound,
                ),
                verdict,
            },
            VerifierGroupAttestation {
                attestation_event_cid: EventCid::from_bytes([32; 32]),
                verifier_actor_id: ActorId::from_bytes([12; 32]),
                correlation_evidence: make_4d_correlation(
                    12,
                    22,
                    32,
                    42,
                    EvidenceStrength::ExternallyAttested,
                ),
                verdict,
            },
        ],
        reward_policy_cid: policy_cid,
        evidence_frontier_root: [44; 32],
    }
}

fn sample_benefit_claim(
    policy_cid: ObjectCid,
    outcome_case_id: [u8; 32],
    task_ctx: [u8; 32],
) -> BenefitRewardClaimV1 {
    let graph = AttributionGraph::new(vec![
        (
            ContributorRole::KuAuthor,
            ActorId::from_bytes([1; 32]),
            5_000,
        ),
        (
            ContributorRole::MappingAuthor,
            ActorId::from_bytes([2; 32]),
            2_000,
        ),
        (
            ContributorRole::TrackAEncoder,
            ActorId::from_bytes([3; 32]),
            1_500,
        ),
        (
            ContributorRole::ProviderLeaseServer,
            ActorId::from_bytes([4; 32]),
            1_500,
        ),
    ])
    .unwrap();

    BenefitRewardClaimV1 {
        is_bare_ku_creation: false,
        contains_exposure_or_rag_telemetry: false,
        benefit_event_cid: Some(EventCid::from_bytes([60; 32])),
        outcome_case_id,
        task_context_commitment: task_ctx,
        outcome_class: ConceptCcid::from_bytes([7; 16]),
        use_or_derivation_hops: vec![UseOrDerivationHop {
            event_cid: EventCid::from_bytes([61; 32]),
            task_context_commitment: task_ctx,
            input_object_cids: vec![ObjectCid::from_bytes([70; 32])],
            derived_output_cid: Some(ObjectCid::from_bytes([71; 32])),
        }],
        outcome_observations: vec![
            OutcomeObservationSummary {
                observation_object_cid: ObjectCid::from_bytes([62; 32]),
                task_context_commitment: task_ctx,
                valence: OutcomeValence::Beneficial,
                has_measurement_evidence: true,
                has_baseline: true,
                limitations: vec![],
            },
            OutcomeObservationSummary {
                observation_object_cid: ObjectCid::from_bytes([63; 32]),
                task_context_commitment: task_ctx,
                valence: OutcomeValence::Beneficial,
                has_measurement_evidence: true,
                has_baseline: true,
                limitations: vec![],
            },
        ],
        closure_state: BenefitClosureState::SupportedUncontested,
        assessed_valence: OutcomeValence::Beneficial,
        attribution_status: AttributionStatus::Supported,
        has_counterfactual_evidence: true,
        gain_bucket: 3,
        causal_tier: CausalSupportTier::MultiHopDerivationSupported,
        witness_correlations: vec![
            make_4d_correlation(81, 81, 81, 81, EvidenceStrength::CryptoBound),
            make_4d_correlation(82, 82, 82, 82, EvidenceStrength::ExternallyAttested),
        ],
        attribution_graph: graph,
        epoch_index: 1,
        reward_policy_cid: policy_cid,
        evidence_frontier_root: [88; 32],
    }
}

#[test]
fn u256_budget_and_share_conservation_invariants_hold_across_extremes() {
    // Sweep prime budgets and uneven basis-point splits to prove exact conservation:
    // sum(floor(B * s_k / 10_000)) + unminted_dust == B and sum <= B always.
    let graph = AttributionGraph::new(vec![
        (
            ContributorRole::KuAuthor,
            ActorId::from_bytes([1; 32]),
            3_333,
        ),
        (
            ContributorRole::MappingAuthor,
            ActorId::from_bytes([2; 32]),
            3_333,
        ),
        (
            ContributorRole::AssemblyAuthor,
            ActorId::from_bytes([3; 32]),
            1_667,
        ),
        (
            ContributorRole::TrackAEncoder,
            ActorId::from_bytes([4; 32]),
            833,
        ),
        (
            ContributorRole::ProviderLeaseServer,
            ActorId::from_bytes([5; 32]),
            833,
        ),
    ])
    .unwrap();
    assert_eq!(graph.total_shares_bps().unwrap(), 9_999);

    for raw_budget in [
        1u128,
        7,
        9_999,
        10_001,
        999_999_937,
        QUANTA_PER_OBT * 1_000_000 + 13,
        u128::MAX / 32,
    ] {
        let budget = ObtAmount(raw_budget);
        let dist = graph.distribute(budget).unwrap();
        let recomputed_sum = dist
            .payouts
            .iter()
            .fold(ObtAmount::ZERO, |acc, p| acc.checked_add(p.amount).unwrap());
        assert_eq!(recomputed_sum, dist.total_allocated);
        assert!(dist.total_allocated <= budget);
        assert_eq!(
            dist.total_allocated
                .checked_add(dist.unminted_dust)
                .unwrap(),
            budget
        );
    }

    // Reject shares summing > 10_000 bps.
    assert_eq!(
        AttributionGraph::new(vec![
            (
                ContributorRole::KuAuthor,
                ActorId::from_bytes([1; 32]),
                6_000
            ),
            (
                ContributorRole::MappingAuthor,
                ActorId::from_bytes([2; 32]),
                4_001,
            ),
        ]),
        Err(ClaimError::Math(ObtMathError::InvalidShareTotal {
            total_bps: 10_001
        }))
    );

    // Reject any budget multiplier > MAX_FACTOR_BPS (20_000).
    assert_eq!(
        compute_benefit_budget(
            ObtAmount(QUANTA_PER_OBT),
            10_000,
            MAX_FACTOR_BPS + 1,
            10_000,
            10_000
        ),
        Err(ObtMathError::FactorOutOfBounds {
            factor_bps: 20_001,
            max_bps: 20_000
        })
    );
    assert_eq!(BPS_DENOMINATOR, 10_000);
}

#[test]
fn track_a_evidenced_mismatch_symmetry_sybil_resistance_and_epoch_caps() {
    let mut policy = RewardPolicyV1::default_v1();
    policy.per_actor_epoch_ceiling = ObtAmount(5 * QUANTA_PER_OBT);
    policy.per_correlation_group_epoch_ceiling = ObtAmount(8 * QUANTA_PER_OBT);
    let policy_cid = policy.policy_cid();

    // 1. D-014 Anti-Rubber-Stamping: FaithfulAlignment and EvidencedSemanticMismatch
    // compute the EXACT SAME ObtAmount!
    let faithful = sample_work_claim(policy_cid, WorkVerdict::FaithfulAlignment, 12, 20);
    let mismatch = sample_work_claim(
        policy_cid,
        WorkVerdict::EvidencedSemanticMismatch {
            span_quote_digest: [0xAB; 32],
            mismatch_kind: SemanticMismatchKind::NegationOrModalityFlip,
        },
        12,
        20,
    );

    let mut tracker_a = EpochCapTracker::default();
    let mut tracker_b = EpochCapTracker::default();
    let out_faithful =
        ClaimEvaluator::evaluate_work_claim(&faithful, &policy, &mut tracker_a).unwrap();
    let out_mismatch =
        ClaimEvaluator::evaluate_work_claim(&mismatch, &policy, &mut tracker_b).unwrap();

    let ClaimEvaluationOutcome::WorkApproved {
        amount: amt_faithful,
        ..
    } = out_faithful
    else {
        panic!("expected WorkApproved");
    };
    let ClaimEvaluationOutcome::WorkApproved {
        amount: amt_mismatch,
        ..
    } = out_mismatch
    else {
        panic!("expected WorkApproved");
    };
    assert_eq!(amt_faithful, amt_mismatch);

    // 2. Structural complexity caps at 32 statements and 64 constraints (zero UTF-8 byte-length input).
    let at_cap = sample_work_claim(policy_cid, WorkVerdict::FaithfulAlignment, 32, 64);
    let padded_beyond_cap =
        sample_work_claim(policy_cid, WorkVerdict::FaithfulAlignment, 9_999, 9_999);
    assert_eq!(
        ClaimEvaluator::compute_work_amount(&at_cap, &policy).unwrap(),
        ClaimEvaluator::compute_work_amount(&padded_beyond_cap, &policy).unwrap()
    );

    // 3. Reject replay of the same ScopedNullifier::for_work.
    assert_eq!(
        ClaimEvaluator::evaluate_work_claim(&faithful, &policy, &mut tracker_a),
        Err(ClaimError::NullifierReplay(faithful.nullifier()))
    );

    // 4. Reject SelfClaimed / Unknown verifier correlation evidence.
    let mut self_claimed = sample_work_claim(policy_cid, WorkVerdict::FaithfulAlignment, 4, 4);
    self_claimed.job_proof.source_artifact_cid = ObjectCid::from_bytes([0xCC; 32]);
    self_claimed.verifier_attestations[1].correlation_evidence =
        make_4d_correlation(12, 22, 32, 42, EvidenceStrength::SelfClaimed);
    assert_eq!(
        ClaimEvaluator::evaluate_work_claim(&self_claimed, &policy, &mut tracker_a),
        Err(ClaimError::InsufficientDistinctVerifierGroups {
            found: 1,
            required: 2
        })
    );

    // 5. Reject Sybil verifiers sharing the same AdministrativePrincipal commitment.
    let mut sybil_same_admin = sample_work_claim(policy_cid, WorkVerdict::FaithfulAlignment, 4, 4);
    sybil_same_admin.job_proof.source_artifact_cid = ObjectCid::from_bytes([0xCD; 32]);
    sybil_same_admin.verifier_attestations[1].correlation_evidence =
        make_4d_correlation(11, 99, 99, 99, EvidenceStrength::CryptoBound);
    assert_eq!(
        ClaimEvaluator::evaluate_work_claim(&sybil_same_admin, &policy, &mut tracker_a),
        Err(ClaimError::InsufficientDistinctVerifierGroups {
            found: 1,
            required: 2
        })
    );

    // 6. Reject uncommitted blind reveal (commit_preceded_reveal == false).
    let mut bad_blind = sample_work_claim(policy_cid, WorkVerdict::FaithfulAlignment, 4, 4);
    bad_blind.job_proof.source_artifact_cid = ObjectCid::from_bytes([0xCE; 32]);
    bad_blind.blind_proof.commit_preceded_reveal = false;
    assert_eq!(
        ClaimEvaluator::evaluate_work_claim(&bad_blind, &policy, &mut tracker_a),
        Err(ClaimError::InvalidBlindCommitReveal)
    );

    // 7. Enforce per-ActorId epoch cap across distinct artifacts in the same epoch.
    let mut cap_tracker = EpochCapTracker::default();
    for artifact_byte in 1u8..=10u8 {
        let mut c = sample_work_claim(policy_cid, WorkVerdict::FaithfulAlignment, 32, 64);
        c.job_proof.source_artifact_cid = ObjectCid::from_bytes([artifact_byte; 32]);
        let res = ClaimEvaluator::evaluate_work_claim(&c, &policy, &mut cap_tracker);
        if artifact_byte <= 1 {
            assert!(res.is_ok());
        } else {
            // Each max-complexity verify claim is 0.1 + 32*0.04 + 64*0.02 = 2.66 OBT;
            // 2 * 2.66 = 5.32 OBT > 5.0 OBT per_actor_epoch_ceiling!
            assert_eq!(
                res,
                Err(ClaimError::ActorEpochCapExceeded {
                    epoch: 1,
                    actor: c.worker_actor_id
                })
            );
            break;
        }
    }
    assert!(
        cap_tracker.actor_issued(1, ActorId::from_bytes([10; 32]))
            <= policy.per_actor_epoch_ceiling
    );
}

#[test]
fn track_b_neg_p5_003_hard_rejects_and_multi_case_retroactive_ku_rewards() {
    let policy = RewardPolicyV1::default_v1();
    let policy_cid = policy.policy_cid();
    let mut tracker = EpochCapTracker::default();

    // 1. Reject bare KU creation.
    let mut bare_ku = sample_benefit_claim(policy_cid, [1; 32], [2; 32]);
    bare_ku.is_bare_ku_creation = true;
    assert_eq!(
        ClaimEvaluator::evaluate_benefit_claim(&bare_ku, &policy, &mut tracker),
        Err(ClaimError::BareKuCreationForbidden)
    );

    // 2. Reject query / RAG exposure telemetry.
    let mut exposure = sample_benefit_claim(policy_cid, [1; 32], [2; 32]);
    exposure.contains_exposure_or_rag_telemetry = true;
    assert_eq!(
        ClaimEvaluator::evaluate_benefit_claim(&exposure, &policy, &mut tracker),
        Err(ClaimError::ExposureTelemetryForbidden)
    );

    // 3. Reject standalone UseEvidence without OutcomeObservation.
    let mut standalone_use = sample_benefit_claim(policy_cid, [1; 32], [2; 32]);
    standalone_use.outcome_observations.clear();
    assert_eq!(
        ClaimEvaluator::evaluate_benefit_claim(&standalone_use, &policy, &mut tracker),
        Err(ClaimError::StandaloneUseWithoutOutcome)
    );

    // 4. Reject contested or harmful closures.
    let mut contested = sample_benefit_claim(policy_cid, [1; 32], [2; 32]);
    contested.closure_state = BenefitClosureState::ContestedOrMixed;
    assert_eq!(
        ClaimEvaluator::evaluate_benefit_claim(&contested, &policy, &mut tracker),
        Err(ClaimError::ContestedOrHarmfulClosure)
    );

    let mut harmful_branch = sample_benefit_claim(policy_cid, [1; 32], [2; 32]);
    harmful_branch.outcome_observations[1].valence = OutcomeValence::Harmful;
    assert_eq!(
        ClaimEvaluator::evaluate_benefit_claim(&harmful_branch, &policy, &mut tracker),
        Err(ClaimError::ContestedOrHarmfulClosure)
    );

    // 5. Reject broken task_context_commitment continuity.
    let mut broken_ctx = sample_benefit_claim(policy_cid, [1; 32], [2; 32]);
    broken_ctx.use_or_derivation_hops[0].task_context_commitment = [99; 32];
    assert_eq!(
        ClaimEvaluator::evaluate_benefit_claim(&broken_ctx, &policy, &mut tracker),
        Err(ClaimError::BrokenTaskContextContinuity)
    );

    // 5b. Self-reported closure yields 0 under default policy (`self_reported_micro_ceiling == 0`),
    // and clamps to `self_reported_micro_ceiling` when configured.
    let mut self_rep = sample_benefit_claim(policy_cid, [0x99; 32], [2; 32]);
    self_rep.witness_correlations.clear();
    self_rep.outcome_observations[0]
        .limitations
        .push(EvidenceLimitation::SelfReported);
    assert_eq!(
        ClaimEvaluator::evaluate_benefit_claim(&self_rep, &policy, &mut tracker),
        Err(ClaimError::ZeroRewardBudget)
    );

    // 6. Hop-attenuated Derivation DAG synthesis rejects cycles and attenuates deeper ancestors.
    let cyclic_nodes = vec![
        DerivationContributorNode {
            object_cid: ObjectCid::from_bytes([1; 32]),
            role: ContributorRole::KuAuthor,
            actor: ActorId::from_bytes([1; 32]),
            hop_distance: 0,
            parent_objects: vec![ObjectCid::from_bytes([2; 32])],
            base_weight_units: 100,
        },
        DerivationContributorNode {
            object_cid: ObjectCid::from_bytes([2; 32]),
            role: ContributorRole::KuAuthor,
            actor: ActorId::from_bytes([2; 32]),
            hop_distance: 1,
            parent_objects: vec![ObjectCid::from_bytes([1; 32])],
            base_weight_units: 100,
        },
    ];
    assert_eq!(
        AttributionGraph::from_causal_lineage(&cyclic_nodes, 7_000),
        Err(ClaimError::DerivationCycleDetected)
    );

    let acyclic_nodes = vec![
        DerivationContributorNode {
            object_cid: ObjectCid::from_bytes([10; 32]),
            role: ContributorRole::AssemblyAuthor,
            actor: ActorId::from_bytes([1; 32]),
            hop_distance: 0,
            parent_objects: vec![ObjectCid::from_bytes([11; 32])],
            base_weight_units: 1_000,
        },
        DerivationContributorNode {
            object_cid: ObjectCid::from_bytes([11; 32]),
            role: ContributorRole::KuAuthor,
            actor: ActorId::from_bytes([2; 32]),
            hop_distance: 2,
            parent_objects: vec![],
            base_weight_units: 1_000,
        },
    ];
    let lineage_graph = AttributionGraph::from_causal_lineage(&acyclic_nodes, 7_000).unwrap();
    // Direct hop=0 contributor gets strictly larger bps than hop=2 attenuated ancestor (1.0 vs 0.49).
    let direct_bps = lineage_graph
        .shares_bps
        .iter()
        .find(|(_, a, _)| *a == ActorId::from_bytes([1; 32]))
        .unwrap()
        .2;
    let ancestor_bps = lineage_graph
        .shares_bps
        .iter()
        .find(|(_, a, _)| *a == ActorId::from_bytes([2; 32]))
        .unwrap()
        .2;
    assert!(direct_bps > ancestor_bps);
    assert!(lineage_graph.total_shares_bps().unwrap() <= 10_000);

    // 7. Valid BenefitRewardClaimV1 succeeds once, blocks replay on same OutcomeCaseId,
    // AND allows the SAME KU to earn a new BenefitBudget on a distinct later OutcomeCaseId!
    let case_1 = sample_benefit_claim(policy_cid, [0xA1; 32], [0xB1; 32]);
    let res_1 = ClaimEvaluator::evaluate_benefit_claim(&case_1, &policy, &mut tracker).unwrap();
    assert!(matches!(
        res_1,
        ClaimEvaluationOutcome::BenefitApproved { .. }
    ));

    // Replay / claim-split of case_1 fails on ScopedNullifier::for_benefit.
    assert_eq!(
        ClaimEvaluator::evaluate_benefit_claim(&case_1, &policy, &mut tracker),
        Err(ClaimError::NullifierReplay(case_1.nullifier()))
    );

    // Years later, the exact same KU (`[70; 32]`) contributes to distinct `OutcomeCaseId` `[0xA2; 32]`:
    let case_2 = sample_benefit_claim(policy_cid, [0xA2; 32], [0xB2; 32]);
    let res_2 = ClaimEvaluator::evaluate_benefit_claim(&case_2, &policy, &mut tracker).unwrap();
    assert!(matches!(
        res_2,
        ClaimEvaluationOutcome::BenefitApproved { .. }
    ));
}

#[test]
fn firewall_shadow_evaluate_and_backpressure_never_mutate_balances_or_fail_callers() {
    let policy = RewardPolicyV1::default_v1();
    let policy_cid = policy.policy_cid();
    let default_flags = ObtFeatureFlags::default();
    assert!(!default_flags.reward_evidence_export);
    assert!(!default_flags.obt_ledger_v1);
    assert_eq!(
        ObtEconomicStatus::default().as_str(),
        "simulated_non_economic"
    );

    let notice_work = ObtNoticeV2::AcceptedWork {
        work_record_cid: ObjectCid::from_bytes([1; 32]),
        attestation_event_cid: EventCid::from_bytes([2; 32]),
        work_kind: AcceptedWorkKind::DelegatedEncode,
        evidence_frontier_root: [3; 32],
        policy_cid,
    };
    let notice_benefit = ObtNoticeV2::BenefitClosure {
        benefit_event_cid: EventCid::from_bytes([4; 32]),
        outcome_case_id: [5; 32],
        evidence_frontier_root: [6; 32],
        policy_cid,
    };
    assert!(!notice_work.contains_mint_or_balance_authority());

    // 1. Disabled runtime drops safely.
    let mut consumer = ObtFirewallConsumer::new(ObtRuntimeMode::Disabled, default_flags, 2, 1);
    assert_eq!(
        consumer.ingest_notice(notice_work.clone(), None),
        NoticeIngestOutcome::DroppedFirewallSafe {
            reason: FirewallDropReason::RuntimeDisabled
        }
    );

    // 2. ShadowEvaluate with export enabled: bounded backpressure + duplicate deduplication + DB error safety.
    let flags_on = ObtFeatureFlags {
        reward_evidence_export: true,
        reward_export_kill_switch: false,
        obt_ledger_v1: false,
        obt_ledger_kill_switch: false,
    };
    let mut shadow_consumer =
        ObtFirewallConsumer::new(ObtRuntimeMode::ShadowEvaluate, flags_on, 2, 1);
    let mut sink = InMemoryFirewallSink {
        inject_failure: true,
        ..Default::default()
    };
    assert_eq!(
        shadow_consumer.ingest_notice(notice_work.clone(), Some(&mut sink)),
        NoticeIngestOutcome::DroppedFirewallSafe {
            reason: FirewallDropReason::BackendUnavailableOrCorrupt
        }
    );

    sink.inject_failure = false;
    assert!(matches!(
        shadow_consumer.ingest_notice(notice_work.clone(), Some(&mut sink)),
        NoticeIngestOutcome::Enqueued { .. }
    ));
    assert!(matches!(
        shadow_consumer.ingest_notice(notice_work.clone(), Some(&mut sink)),
        NoticeIngestOutcome::DuplicateIgnored { .. }
    ));
    assert!(matches!(
        shadow_consumer.ingest_notice(notice_benefit.clone(), Some(&mut sink)),
        NoticeIngestOutcome::Enqueued { .. }
    ));

    // Third distinct notice hits max_queue_len == 2 backpressure without failing caller!
    let extra_notice = ObtNoticeV2::BenefitClosure {
        benefit_event_cid: EventCid::from_bytes([9; 32]),
        outcome_case_id: [10; 32],
        evidence_frontier_root: [11; 32],
        policy_cid,
    };
    assert_eq!(
        shadow_consumer.ingest_notice(extra_notice, Some(&mut sink)),
        NoticeIngestOutcome::DroppedFirewallSafe {
            reason: FirewallDropReason::QueueBackpressureFull
        }
    );

    // 3. Draining under ShadowEvaluate persists ShadowClaimEvaluation diagnostics ONLY
    // and creates ZERO PendingMintRecords or AccountBlockV1s even if pending_mints_if_active is non-empty!
    let dummy_pending = PendingMintRecord::new(
        PendingMintSource::WorkClaim {
            claim_cid: [77; 32],
        },
        ActorId::from_bytes([10; 32]),
        ObtAmount(QUANTA_PER_OBT),
        ScopedNullifier::from_bytes([88; 32]),
        ScopedNullifier::from_bytes([88; 32]),
        &policy,
        10,
    );
    let report = shadow_consumer.drain(&mut sink, |notice, econ_status| {
        let shadow = onebrain_obt::ShadowClaimEvaluation::new(
            notice.notice_id(),
            policy_cid,
            [3; 32],
            ScopedNullifier::from_bytes([88; 32]),
            true,
            ObtAmount(QUANTA_PER_OBT),
            "CANDIDATE_OK",
            econ_status,
        );
        Ok(NoticeEvaluationCandidate {
            shadow,
            pending_mints_if_active: vec![dummy_pending.clone()],
        })
    });
    assert_eq!(report.drained_notices, 2);
    assert_eq!(report.shadow_evaluations_persisted, 2);
    assert_eq!(report.pending_mints_created, 0);
    assert_eq!(report.account_blocks_created, 0);
    assert!(sink.pending_mints.is_empty());
    assert_eq!(sink.shadow_evaluations.len(), 2);
    assert!(sink.outbox.is_empty());
}

#[test]
fn mint_pipeline_vrf_selection_dispute_voiding_and_quorum_finalization() {
    let policy = RewardPolicyV1::default_v1();
    let policy_cid = policy.policy_cid();
    let mut key_state = KeyStateReducer::new(EventCid::from_bytes([1; 32]));

    let worker = make_actor_fixture(1, &mut key_state);
    let challenger = make_actor_fixture(2, &mut key_state);
    let aud1 = make_actor_fixture(3, &mut key_state);
    let aud2 = make_actor_fixture(4, &mut key_state);
    let aud3 = make_actor_fixture(5, &mut key_state);

    let mut feeds = BTreeMap::new();
    for f in [&worker, &challenger, &aud1, &aud2, &aud3] {
        feeds.insert(f.feed.feed_id, f.feed.clone());
    }

    let p1 = PermitCid::from_bytes([31; 32]);
    let p2 = PermitCid::from_bytes([32; 32]);
    let p3 = PermitCid::from_bytes([33; 32]);
    let p3_stuffed = PermitCid::from_bytes([34; 32]);

    // Auditor VRF selection is permutation-invariant and immune to single-ActorId permit stuffing.
    let eval_list_a = vec![
        (aud1.actor, p1),
        (aud2.actor, p2),
        (aud3.actor, p3),
        (aud3.actor, p3_stuffed),
    ];
    let eval_list_b = vec![
        (aud3.actor, p3_stuffed),
        (aud2.actor, p2),
        (aud1.actor, p1),
        (aud3.actor, p3),
    ];
    let entropy = [0x5A; 32];
    let claim_cid_sample = [0x9B; 32];
    let sel_a = select_challenge_auditors(&entropy, &claim_cid_sample, &eval_list_a, 2);
    let sel_b = select_challenge_auditors(&entropy, &claim_cid_sample, &eval_list_b, 2);
    assert_eq!(sel_a, sel_b);
    assert_eq!(sel_a.len(), 2);
    assert_ne!(sel_a[0].0, sel_a[1].0);

    // Stage two PendingMintRecords: one will be disputed, one will complete uncontested.
    let mut pipeline = MintPipeline::new();
    let mut claim_tracker = EpochCapTracker::default();
    let mut work_1 = sample_work_claim(policy_cid, WorkVerdict::FaithfulAlignment, 8, 8);
    work_1.worker_actor_id = worker.actor;
    let out_1 = ClaimEvaluator::evaluate_work_claim(&work_1, &policy, &mut claim_tracker).unwrap();
    let staged_1 = pipeline.stage_approved_claim(&out_1, &policy, 100)[0].clone();
    assert!(!staged_1.is_spendable());
    assert!(!staged_1.is_transferable());

    let mut work_2 = sample_work_claim(policy_cid, WorkVerdict::FaithfulAlignment, 10, 10);
    work_2.worker_actor_id = worker.actor;
    work_2.job_proof.source_artifact_cid = ObjectCid::from_bytes([0xEE; 32]);
    let out_2 = ClaimEvaluator::evaluate_work_claim(&work_2, &policy, &mut claim_tracker).unwrap();
    let staged_2 = pipeline.stage_approved_claim(&out_2, &policy, 100)[0].clone();

    // Dispute staged_1 during challenge window -> VoidedOrFrozen (zero clawback needed!).
    pipeline
        .submit_challenge_dispute(
            staged_1.pending_mint_cid,
            ChallengeDispute::InvalidWorkProof {
                proof_cid: [0x77; 32],
                reason_code: "FALSIFIED_SPAN_QUOTE".to_owned(),
            },
            &challenger.feed,
            &key_state,
            105,
        )
        .unwrap();
    assert!(matches!(
        pipeline.pending_mints[&staged_1.pending_mint_cid].state,
        MintLifecycleState::VoidedOrFrozen { .. }
    ));

    // Attempting to finalize voided staged_1 fails closed even after window closes.
    assert_eq!(
        pipeline.finalize_mint(
            staged_1.pending_mint_cid,
            entropy,
            &eval_list_a,
            2,
            vec![],
            &feeds,
            &key_state,
            200
        ),
        Err(MintPipelineError::AlreadyVoidedOrFrozen)
    );

    // Attempting to finalize staged_2 before closes_at_frontier_seq (116) fails closed.
    assert_eq!(
        pipeline.finalize_mint(
            staged_2.pending_mint_cid,
            entropy,
            &eval_list_a,
            2,
            vec![],
            &feeds,
            &key_state,
            115
        ),
        Err(MintPipelineError::ChallengeWindowStillOpen {
            current: 115,
            closes_at: 116
        })
    );

    // Sign with the VRF-selected 2-of-2 committee for staged_2 and finalize at frontier 116.
    let claim_cid_2 = staged_2.source.claim_cid();
    let committee = select_challenge_auditors(&entropy, &claim_cid_2, &eval_list_a, 2);
    let actor_map: BTreeMap<ActorId, &ActorFixture> = [
        (aud1.actor, &aud1),
        (aud2.actor, &aud2),
        (aud3.actor, &aud3),
    ]
    .into_iter()
    .collect();

    let mut cosigs = Vec::new();
    for (sel_actor, sel_permit) in &committee {
        let fx = actor_map[sel_actor];
        cosigs.push(
            AuditCoSignature::sign(
                fx.actor,
                *sel_permit,
                &fx.feed,
                &fx.signing_key,
                staged_2.pending_mint_cid,
                claim_cid_2,
                entropy,
            )
            .unwrap(),
        );
    }

    let cert = pipeline
        .finalize_mint(
            staged_2.pending_mint_cid,
            entropy,
            &eval_list_a,
            2,
            cosigs,
            &feeds,
            &key_state,
            116,
        )
        .unwrap();
    assert!(cert.verify_self_contained().is_ok());
    assert_eq!(cert.beneficiary, worker.actor);
}

#[test]
fn cap_honest_account_chain_r4_rotation_equivocation_freeze_redb_and_island_sync() {
    let policy = RewardPolicyV1::default_v1();
    let policy_cid = policy.policy_cid();
    let frontier = EventCid::from_bytes([9; 32]);
    let mut key_state = KeyStateReducer::new(frontier);

    let alice = make_actor_fixture(10, &mut key_state);
    let bob = make_actor_fixture(20, &mut key_state);
    let carol = make_actor_fixture(30, &mut key_state);
    let aud1 = make_actor_fixture(41, &mut key_state);
    let aud2 = make_actor_fixture(42, &mut key_state);
    let val1 = make_actor_fixture(51, &mut key_state);
    let val2 = make_actor_fixture(52, &mut key_state);
    let val3 = make_actor_fixture(53, &mut key_state);
    let val4 = make_actor_fixture(54, &mut key_state);

    let mut feeds = BTreeMap::new();
    for fx in [
        &alice, &bob, &carol, &aud1, &aud2, &val1, &val2, &val3, &val4,
    ] {
        feeds.insert(fx.feed.feed_id, fx.feed.clone());
    }

    // Create a finalized mint certificate for Alice so she starts with minted OBT.
    let mut claim_tracker = EpochCapTracker::default();
    let mut work = sample_work_claim(policy_cid, WorkVerdict::FaithfulAlignment, 20, 20);
    work.worker_actor_id = alice.actor;
    let approved = ClaimEvaluator::evaluate_work_claim(&work, &policy, &mut claim_tracker).unwrap();
    let mut mint_pipe = MintPipeline::new();
    let pending = mint_pipe.stage_approved_claim(&approved, &policy, 1)[0].clone();
    let entropy = [0x33; 32];
    let evaluators = vec![
        (aud1.actor, PermitCid::from_bytes([1; 32])),
        (aud2.actor, PermitCid::from_bytes([2; 32])),
    ];
    let cosig1 = AuditCoSignature::sign(
        aud1.actor,
        PermitCid::from_bytes([1; 32]),
        &aud1.feed,
        &aud1.signing_key,
        pending.pending_mint_cid,
        pending.source.claim_cid(),
        entropy,
    )
    .unwrap();
    let cosig2 = AuditCoSignature::sign(
        aud2.actor,
        PermitCid::from_bytes([2; 32]),
        &aud2.feed,
        &aud2.signing_key,
        pending.pending_mint_cid,
        pending.source.claim_cid(),
        entropy,
    )
    .unwrap();
    let alice_mint_cert = mint_pipe
        .finalize_mint(
            pending.pending_mint_cid,
            entropy,
            &evaluators,
            2,
            vec![cosig1, cosig2],
            &feeds,
            &key_state,
            20,
        )
        .unwrap();

    // Configure 4-validator BFT settlement domain (f = 1 => quorum >= 2f + 1 = 3).
    let domain_id = [0xD1; 32];
    let manifest = SettlementDomainManifest {
        domain_id,
        fault_weight_f: 1,
        validators: vec![
            SettlementDomainValidator {
                validator_actor: val1.actor,
                validator_feed: val1.feed.feed_id,
                public_key: val1.feed.signed.inception.feed_public_key,
                weight: 1,
            },
            SettlementDomainValidator {
                validator_actor: val2.actor,
                validator_feed: val2.feed.feed_id,
                public_key: val2.feed.signed.inception.feed_public_key,
                weight: 1,
            },
            SettlementDomainValidator {
                validator_actor: val3.actor,
                validator_feed: val3.feed.feed_id,
                public_key: val3.feed.signed.inception.feed_public_key,
                weight: 1,
            },
            SettlementDomainValidator {
                validator_actor: val4.actor,
                validator_feed: val4.feed.feed_id,
                public_key: val4.feed.signed.inception.feed_public_key,
                weight: 1,
            },
        ],
    };

    let mut chain = AccountChainEngine::new();
    chain.register_domain_manifest(manifest.clone()).unwrap();

    // 1. Stale R4 authority observation (> 60s) is rejected!
    let alice_b0 = AccountBlockV1::sign(
        alice.actor,
        0,
        None,
        AccountBlockOp::OpenAccount {
            initial_policy_cid: policy_cid,
        },
        0,
        None,
        &alice.feed,
        frontier,
        &alice.signing_key,
    )
    .unwrap();
    assert_eq!(
        chain.submit_block(
            alice_b0.clone(),
            &alice.feed,
            &key_state,
            1_000,
            1_061,
            false
        ),
        Err(AccountChainError::StaleOrDeniedR4Freshness)
    );

    // Within 60s R4 window succeeds as ProvisionalPartitionLocal with mandatory UI badge.
    let out_b0 = chain
        .submit_block(
            alice_b0.clone(),
            &alice.feed,
            &key_state,
            1_000,
            1_060,
            false,
        )
        .unwrap();
    let alice_b0_cid = alice_b0.block_cid();
    assert_eq!(
        out_b0,
        BlockInsertOutcome::AcceptedProvisional {
            block_cid: alice_b0_cid
        }
    );
    assert_eq!(
        chain.available_provisional_balance(alice.actor),
        Some((ObtAmount::ZERO, PROVISIONAL_SETTLEMENT_UI_BADGE))
    );

    // 2. Key rotation on Alice's ActorId: authorize a new FeedId on Alice's ActorId and sign seq = 1!
    let alice_rotated_key = SigningKey::from_bytes(&[0xEE; 32]);
    let rot_delegation_ref = EventCid::from_bytes([0xEF; 32]);
    let mut rot_inception = FeedInception::new(
        *alice_rotated_key.verifying_key().as_bytes(),
        NamespaceCommitment::derive(b"obt-p5-rot", [10; 32]).unwrap(),
        1,
        alice.device,
    );
    rot_inception.actor_delegation_ref = Some(*rot_delegation_ref.as_bytes());
    let alice_rot_feed = decode_feed_inception(
        &rot_inception
            .sign(&alice_rotated_key)
            .unwrap()
            .encode()
            .unwrap(),
    )
    .unwrap();
    feeds.insert(alice_rot_feed.feed_id, alice_rot_feed.clone());
    assert_eq!(
        key_state.submit_child(ScopedDelegation {
            grant: DelegationGrant {
                actor: alice.actor,
                device: alice.device,
                subject_feed: alice_rot_feed.feed_id,
                delegation_ref: rot_delegation_ref,
                namespace_commitment: None,
                first_generation: 0,
                last_generation: 5,
                proof: rot_delegation_ref,
            },
            parent_delegation_ref: Some(alice.delegation_ref),
        }),
        KeyStateApplyOutcome::Accepted
    );

    let minted_quanta = alice_mint_cert.amount.quanta();
    let alice_b1 = AccountBlockV1::sign(
        alice.actor,
        1,
        Some(alice_b0_cid),
        AccountBlockOp::CreditFinalizedMint {
            certificate: Box::new(alice_mint_cert.clone()),
        },
        minted_quanta,
        Some(alice_mint_cert.credit_nullifier),
        &alice_rot_feed,
        frontier,
        &alice_rotated_key,
    )
    .unwrap();
    let alice_b1_cid = alice_b1.block_cid();
    chain
        .submit_block(
            alice_b1.clone(),
            &alice_rot_feed,
            &key_state,
            1_050,
            1_060,
            false,
        )
        .unwrap();

    // Finalize Alice seq 0..=1 via >= 2f + 1 (3 of 4) SettlementDomainCheckpoint!
    let mut cp1 = SettlementDomainCheckpoint {
        domain_id,
        checkpoint_seq: 1,
        finalized_heads: vec![(alice.actor, 1, alice_b1_cid)],
        nullifier_set_root: [0xAA; 32],
        validator_signatures: vec![],
    };
    // Only 2 signatures (< 3 required) fails quorum check.
    cp1.validator_signatures = vec![
        cp1.sign_validator(val1.actor, &val1.feed, &val1.signing_key)
            .unwrap(),
        cp1.sign_validator(val2.actor, &val2.feed, &val2.signing_key)
            .unwrap(),
    ];
    assert_eq!(
        chain.apply_domain_checkpoint(cp1.clone()),
        Err(AccountChainError::InsufficientCheckpointQuorum {
            signed_weight: 2,
            required_weight: 3
        })
    );
    // Add 3rd validator signature -> reaches 2f + 1 = 3!
    cp1.validator_signatures.push(
        cp1.sign_validator(val3.actor, &val3.feed, &val3.signing_key)
            .unwrap(),
    );
    chain.apply_domain_checkpoint(cp1.clone()).unwrap();
    assert_eq!(chain.settled_balance(alice.actor), ObtAmount(minted_quanta));
    assert!(chain
        .get_tracked_block(&alice_b1_cid)
        .unwrap()
        .settlement
        .is_final_in_domain());

    // 3. Adversarial Double-Spend Fork at Alice seq = 2 across Partition A and Partition B:
    // Alice sends 500_000_000 quanta to Bob on branch A, AND sends 500_000_000 quanta to Carol on branch B!
    let send_amt = ObtAmount(500_000_000);
    let alice_b2_to_bob = AccountBlockV1::sign(
        alice.actor,
        2,
        Some(alice_b1_cid),
        AccountBlockOp::SendTransfer {
            recipient: bob.actor,
            amount: send_amt,
            expires_at_frontier_seq: None,
            memo_commitment: None,
        },
        minted_quanta - send_amt.quanta(),
        None,
        &alice_rot_feed,
        frontier,
        &alice_rotated_key,
    )
    .unwrap();
    let send_bob_cid = alice_b2_to_bob.block_cid();
    chain
        .submit_block(
            alice_b2_to_bob.clone(),
            &alice_rot_feed,
            &key_state,
            1_050,
            1_060,
            false,
        )
        .unwrap();

    // Bob opens his account and tries to receive WITHOUT opting into provisional risk -> rejected!
    let bob_b0 = AccountBlockV1::sign(
        bob.actor,
        0,
        None,
        AccountBlockOp::OpenAccount {
            initial_policy_cid: policy_cid,
        },
        0,
        None,
        &bob.feed,
        frontier,
        &bob.signing_key,
    )
    .unwrap();
    chain
        .submit_block(bob_b0.clone(), &bob.feed, &key_state, 1_050, 1_060, false)
        .unwrap();

    let bob_recv_nullifier = ScopedNullifier::for_receive_transfer(send_bob_cid, bob.actor);
    let bob_b1_recv = AccountBlockV1::sign(
        bob.actor,
        1,
        Some(bob_b0.block_cid()),
        AccountBlockOp::ReceiveTransfer {
            send_block_cid: send_bob_cid,
            sender: alice.actor,
            amount: send_amt,
        },
        send_amt.quanta(),
        Some(bob_recv_nullifier),
        &bob.feed,
        frontier,
        &bob.signing_key,
    )
    .unwrap();
    assert_eq!(
        chain.submit_block(
            bob_b1_recv.clone(),
            &bob.feed,
            &key_state,
            1_050,
            1_060,
            false
        ),
        Err(AccountChainError::ProvisionalOptInRequired)
    );
    // Bob opts into provisional credit -> accepted provisionally.
    chain
        .submit_block(
            bob_b1_recv.clone(),
            &bob.feed,
            &key_state,
            1_050,
            1_060,
            true,
        )
        .unwrap();

    // Now conflicting branch B from Alice at seq = 2 arrives!
    let alice_b2_to_carol = AccountBlockV1::sign(
        alice.actor,
        2,
        Some(alice_b1_cid),
        AccountBlockOp::SendTransfer {
            recipient: carol.actor,
            amount: send_amt,
            expires_at_frontier_seq: None,
            memo_commitment: None,
        },
        minted_quanta - send_amt.quanta(),
        None,
        &alice_rot_feed,
        frontier,
        &alice_rotated_key,
    )
    .unwrap();
    let fork_outcome = chain
        .submit_block(
            alice_b2_to_carol.clone(),
            &alice_rot_feed,
            &key_state,
            1_050,
            1_060,
            false,
        )
        .unwrap();
    let BlockInsertOutcome::EquivocationFrozen { warrant } = fork_outcome else {
        panic!("expected EquivocationFrozen");
    };
    assert!(warrant.verify().is_ok());

    // Both Alice seq=2 branches AND Bob's unfinalized downstream ReceiveTransfer are now FrozenEquivocated,
    // while Alice's already FinalInDomain blocks (seq 0..=1) remain completely untouched!
    assert!(chain
        .get_tracked_block(&send_bob_cid)
        .unwrap()
        .settlement
        .is_frozen());
    assert!(chain
        .get_tracked_block(&alice_b2_to_carol.block_cid())
        .unwrap()
        .settlement
        .is_frozen());
    assert!(chain
        .get_tracked_block(&bob_b1_recv.block_cid())
        .unwrap()
        .settlement
        .is_frozen());
    assert!(chain
        .get_tracked_block(&alice_b1_cid)
        .unwrap()
        .settlement
        .is_final_in_domain());
    assert_eq!(chain.settled_balance(alice.actor), ObtAmount(minted_quanta));

    // 4. Offline Island Carol + Durable RedbObtStore + `obp/obt-ledger/1` Reunion Convergence:
    // Carol operates on a disconnected offline island (zero slashing!), opens an account, credits her own mint,
    // sends with expiry + cancels after expiry, and later syncs via `ObtLedgerSyncEngine`!
    let mut carol_work = sample_work_claim(policy_cid, WorkVerdict::FaithfulAlignment, 10, 10);
    carol_work.worker_actor_id = carol.actor;
    carol_work.job_proof.source_artifact_cid = ObjectCid::from_bytes([0x91; 32]);
    let carol_approved =
        ClaimEvaluator::evaluate_work_claim(&carol_work, &policy, &mut claim_tracker).unwrap();
    let carol_pending = mint_pipe.stage_approved_claim(&carol_approved, &policy, 50)[0].clone();
    let carol_cosig1 = AuditCoSignature::sign(
        aud1.actor,
        PermitCid::from_bytes([1; 32]),
        &aud1.feed,
        &aud1.signing_key,
        carol_pending.pending_mint_cid,
        carol_pending.source.claim_cid(),
        entropy,
    )
    .unwrap();
    let carol_cosig2 = AuditCoSignature::sign(
        aud2.actor,
        PermitCid::from_bytes([2; 32]),
        &aud2.feed,
        &aud2.signing_key,
        carol_pending.pending_mint_cid,
        carol_pending.source.claim_cid(),
        entropy,
    )
    .unwrap();
    let carol_cert = mint_pipe
        .finalize_mint(
            carol_pending.pending_mint_cid,
            entropy,
            &evaluators,
            2,
            vec![carol_cosig1, carol_cosig2],
            &feeds,
            &key_state,
            70,
        )
        .unwrap();

    let mut island_chain = AccountChainEngine::new();
    island_chain
        .register_domain_manifest(manifest.clone())
        .unwrap();
    let carol_b0 = AccountBlockV1::sign(
        carol.actor,
        0,
        None,
        AccountBlockOp::OpenAccount {
            initial_policy_cid: policy_cid,
        },
        0,
        None,
        &carol.feed,
        frontier,
        &carol.signing_key,
    )
    .unwrap();
    island_chain
        .submit_block(
            carol_b0.clone(),
            &carol.feed,
            &key_state,
            50_000,
            50_010,
            false,
        )
        .unwrap();

    let carol_mint_q = carol_cert.amount.quanta();
    let carol_b1 = AccountBlockV1::sign(
        carol.actor,
        1,
        Some(carol_b0.block_cid()),
        AccountBlockOp::CreditFinalizedMint {
            certificate: Box::new(carol_cert.clone()),
        },
        carol_mint_q,
        Some(carol_cert.credit_nullifier),
        &carol.feed,
        frontier,
        &carol.signing_key,
    )
    .unwrap();
    island_chain
        .submit_block(
            carol_b1.clone(),
            &carol.feed,
            &key_state,
            50_000,
            50_010,
            false,
        )
        .unwrap();

    // Carol creates an expiring SendTransfer (expires_at_frontier_seq = 200) and then CancelExpiredSend at 205.
    let exp_send_amt = ObtAmount(100_000_000);
    let carol_b2_send = AccountBlockV1::sign(
        carol.actor,
        2,
        Some(carol_b1.block_cid()),
        AccountBlockOp::SendTransfer {
            recipient: bob.actor,
            amount: exp_send_amt,
            expires_at_frontier_seq: Some(200),
            memo_commitment: Some([0x12; 32]),
        },
        carol_mint_q - exp_send_amt.quanta(),
        None,
        &carol.feed,
        frontier,
        &carol.signing_key,
    )
    .unwrap();
    island_chain
        .submit_block(
            carol_b2_send.clone(),
            &carol.feed,
            &key_state,
            50_000,
            50_010,
            false,
        )
        .unwrap();

    let carol_b3_cancel = AccountBlockV1::sign(
        carol.actor,
        3,
        Some(carol_b2_send.block_cid()),
        AccountBlockOp::CancelExpiredSend {
            send_block_cid: carol_b2_send.block_cid(),
            refund_amount: exp_send_amt,
            observed_frontier_seq: 205,
        },
        carol_mint_q,
        None,
        &carol.feed,
        frontier,
        &carol.signing_key,
    )
    .unwrap();
    island_chain
        .submit_block(
            carol_b3_cancel.clone(),
            &carol.feed,
            &key_state,
            50_000,
            50_010,
            false,
        )
        .unwrap();

    // Persist everything to RedbObtStore, close, and reload from disk!
    let tmp = tempfile::tempdir().unwrap();
    let db_path = tmp.path().join("obt_p5.redb");
    {
        let store = RedbObtStore::open(&db_path).unwrap();
        store
            .set_runtime_mode(ObtRuntimeMode::ActiveDomainFinal)
            .unwrap();
        store
            .set_economic_status(ObtEconomicStatus::SimulatedNonEconomic)
            .unwrap();
        store.put_policy(&policy).unwrap();
        store.persist_epoch_caps(&claim_tracker).unwrap();
        store.persist_mint_pipeline(&mint_pipe).unwrap();
        store.persist_account_chain(&chain).unwrap();
    }
    let reopened = RedbObtStore::open(&db_path).unwrap();
    assert_eq!(
        reopened.get_runtime_mode().unwrap(),
        ObtRuntimeMode::ActiveDomainFinal
    );
    assert_eq!(
        reopened.get_economic_status().unwrap(),
        ObtEconomicStatus::SimulatedNonEconomic
    );
    assert_eq!(reopened.get_policy(policy_cid).unwrap(), Some(policy));
    let loaded_caps = reopened.load_epoch_caps().unwrap();
    assert_eq!(loaded_caps.actor_totals, claim_tracker.actor_totals);
    let loaded_mints = reopened.load_mint_pipeline().unwrap();
    assert_eq!(loaded_mints.finalized_mints.len(), 2);
    let mut loaded_main_chain = reopened.load_account_chain().unwrap();
    loaded_main_chain
        .register_domain_manifest(manifest.clone())
        .unwrap();
    assert_eq!(loaded_main_chain.warrants.len(), 1);

    // Sync via `obp/obt-ledger/1` between Mainland (`loaded_main_chain`) and Offline Island (`island_chain`):
    assert_eq!(OBT_LEDGER_PROTOCOL_ID, "obp/obt-ledger/1");

    // Default-off check: disabled `obt_ledger_v1` rejects wire sync cleanly.
    let disabled_sync = ObtLedgerSyncEngine::new(
        ObtFeatureFlags::default(),
        loaded_mints.clone(),
        loaded_main_chain.clone(),
    );
    assert_eq!(
        disabled_sync.build_summary(),
        Err(ObtSyncError::ProtocolDisabled)
    );

    let sync_flags = ObtFeatureFlags {
        reward_evidence_export: true,
        reward_export_kill_switch: false,
        obt_ledger_v1: true,
        obt_ledger_kill_switch: false,
    };
    let mut mainland_sync = ObtLedgerSyncEngine::new(sync_flags, loaded_mints, loaded_main_chain);
    let mut island_sync = ObtLedgerSyncEngine::new(sync_flags, mint_pipe, island_chain);

    // Exchange CBOR wire messages in both directions!
    let main_summary_bytes =
        ObtLedgerWireMessage::HaveSummary(mainland_sync.build_summary().unwrap())
            .encode_wire_bytes()
            .unwrap();
    let ObtLedgerWireMessage::HaveSummary(decoded_main_summary) =
        ObtLedgerWireMessage::decode_wire_bytes(&main_summary_bytes).unwrap()
    else {
        panic!("expected HaveSummary");
    };

    let delta_to_main = island_sync
        .compute_delta_for_peer(&decoded_main_summary)
        .unwrap();
    let delta_wire_bytes = ObtLedgerWireMessage::DeltaBatch(delta_to_main)
        .encode_wire_bytes()
        .unwrap();
    let ObtLedgerWireMessage::DeltaBatch(decoded_delta_to_main) =
        ObtLedgerWireMessage::decode_wire_bytes(&delta_wire_bytes).unwrap()
    else {
        panic!("expected DeltaBatch");
    };

    let auth_ctx = SyncAuthorityContext {
        feeds: &feeds,
        key_state: &key_state,
        authority_observed_at_secs: 100_000,
        current_local_secs: 100_010,
        accept_provisional_receives: true,
    };
    let report_1 = mainland_sync
        .apply_delta(&decoded_delta_to_main, &auth_ctx)
        .unwrap();
    assert_eq!(report_1.blocks_accepted, 4);

    // Re-applying the exact same delta batch is 100% idempotent!
    let report_replay = mainland_sync
        .apply_delta(&decoded_delta_to_main, &auth_ctx)
        .unwrap();
    assert_eq!(report_replay.blocks_accepted, 0);
    assert_eq!(report_replay.blocks_already_present, 4);

    // Sync mainland -> island so island learns Alice's finalized blocks + equivocation warrant,
    // then issue checkpoint 2 finalizing Carol's reunited island chain (seq 0..=3) to FinalInDomain!
    let island_summary = island_sync.build_summary().unwrap();
    let delta_to_island = mainland_sync
        .compute_delta_for_peer(&island_summary)
        .unwrap();
    island_sync
        .apply_delta(&delta_to_island, &auth_ctx)
        .unwrap();
    assert_eq!(island_sync.account_chain.warrants.len(), 1);

    let mut cp2 = SettlementDomainCheckpoint {
        domain_id,
        checkpoint_seq: 2,
        finalized_heads: vec![
            (alice.actor, 1, alice_b1_cid),
            (carol.actor, 3, carol_b3_cancel.block_cid()),
        ],
        nullifier_set_root: [0xBB; 32],
        validator_signatures: vec![],
    };
    cp2.validator_signatures = vec![
        cp2.sign_validator(val1.actor, &val1.feed, &val1.signing_key)
            .unwrap(),
        cp2.sign_validator(val2.actor, &val2.feed, &val2.signing_key)
            .unwrap(),
        cp2.sign_validator(val4.actor, &val4.feed, &val4.signing_key)
            .unwrap(),
    ];
    mainland_sync
        .account_chain
        .apply_domain_checkpoint(cp2)
        .unwrap();
    let final_delta = mainland_sync
        .compute_delta_for_peer(&island_sync.build_summary().unwrap())
        .unwrap();
    island_sync.apply_delta(&final_delta, &auth_ctx).unwrap();

    // Both nodes converged to identical FinalInDomain settled balances for Alice and Carol!
    assert_eq!(
        mainland_sync.account_chain.settled_balance(carol.actor),
        ObtAmount(carol_mint_q)
    );
    assert_eq!(
        island_sync.account_chain.settled_balance(carol.actor),
        ObtAmount(carol_mint_q)
    );
    assert_eq!(
        island_sync
            .account_chain
            .get_tracked_block(&carol_b3_cancel.block_cid())
            .unwrap()
            .settlement,
        SettlementState::FinalInDomain {
            domain_id,
            checkpoint: mainland_sync.account_chain.checkpoints[&(domain_id, 2)].clone(),
        }
    );

    // Finally verify revocation of Alice's rotated feed blocks any future block signing!
    assert_eq!(
        key_state.submit_revocation(ScopedRevocation {
            revocation: AcceptedRevocation {
                actor: alice.actor,
                device: alice.device,
                delegation_ref: rot_delegation_ref,
                revoked_from_generation: 0,
                proof: EventCid::from_bytes([0xFE; 32]),
            },
            authorized_by: alice.delegation_ref,
        }),
        KeyStateApplyOutcome::Accepted
    );
    assert_eq!(
        AccountChainEngine::verify_r4_authority(
            &alice_b1,
            &alice_rot_feed,
            &key_state,
            100_000,
            100_005,
        ),
        Err(AccountChainError::UnauthorizedFeedForActor)
    );
}

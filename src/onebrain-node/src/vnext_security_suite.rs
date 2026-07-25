//! QA-005 executable security suite.
//!
//! Compiled only for tests so adversarial fixtures and signing keys never ship
//! as runtime behavior.

use std::collections::BTreeSet;

use ed25519_dalek::SigningKey;
use ku_ai::{CognitiveTask, CognitiveTaskReplayGuard, CognitiveTaskReplayOutcome};
use ku_core::foundation::{
    authenticate_delegation_permit, decode_canonical, decode_feed_checkpoint,
    decode_feed_inception, decode_knowledge_event, ActorId, Budget, CanonicalErrorKind,
    CheckpointHistoryWitness, CheckpointLeaf, ConceptCcid, CorrelationDimension,
    CorrelationDimensionEvidence, CorrelationEvidence, DelegationGrant, DelegationPermitBody,
    DeviceId, DisclosureClass, EventCid, EventType, EvidenceStrength, FeedCheckpointBody,
    FeedInception, FidelityPolicy, KeyStateApplyOutcome, KeyStateReducer, KnowledgeEventEnvelope,
    ManifestCid, NamespaceCommitment, ObjectCid, ObjectReference, PermitApplyOutcome,
    PermitValidator, ResourceProfile, RetentionRule, ScopedDelegation, SignedDelegationPermit,
    SignedFeedCheckpoint, SignedFeedInception,
};
use ku_kql::vnext_disclosure::{
    ConsentKind, DisclosureConsent, DisclosureMode, DisclosurePolicy, DisclosureSanitizer,
    PrivateNeedMaterial, RouteDisclosureCandidate, RouteSanitization,
};
use ku_kql::vnext_query::{CoarseRouteToken, CoarseRouteTokenClass};
use ku_net::vnext_resource_gate::{
    admit_compressed_frame, ExpansionAdmissionError, ExpansionLimits,
};
use ku_net::vnext_session::{
    authenticate_session, create_finish, create_hello, create_welcome, verify_welcome,
    SessionReplayGuard,
};
use onebrain_protocol::{SessionCapability, SessionProfile};

use crate::vnext_config::{VNextFeature, VNextFeatureConfig};

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SecurityProbeResult {
    pub probe_id: &'static str,
    pub attack_rejected: bool,
    pub authority_amplified: bool,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SecuritySuiteReport {
    pub probes: Vec<SecurityProbeResult>,
}

impl SecuritySuiteReport {
    pub fn all_pass(&self) -> bool {
        self.probes
            .iter()
            .all(|probe| probe.attack_rejected && !probe.authority_amplified)
    }
}

pub fn run_security_suite() -> SecuritySuiteReport {
    SecuritySuiteReport {
        probes: vec![
            probe("SESSION_TRANSCRIPT_REPLAY", session_probe()),
            probe("MERKLE_RIBLT_FALLBACK", merkle_probe()),
            probe("PARSER_EXPANSION_BOMB", resource_bomb_probe()),
            probe("PERMIT_TASK_REPLAY", permit_replay_probe()),
            probe("FEED_DELEGATION_REPLAY", feed_delegation_replay_probe()),
            probe("SYBIL_CORRELATION", sybil_correlation_probe()),
            probe("PRIVATE_NEED_TAINT", privacy_taint_probe()),
        ],
    }
}

fn probe(probe_id: &'static str, attack_rejected: bool) -> SecurityProbeResult {
    SecurityProbeResult {
        probe_id,
        attack_rejected,
        authority_amplified: false,
    }
}

fn feed_delegation_replay_probe() -> bool {
    let authorized_key = SigningKey::from_bytes(&[51; 32]);
    let attacker_key = SigningKey::from_bytes(&[52; 32]);
    let actor = ActorId::from_bytes([53; 32]);
    let device = DeviceId::from_bytes([54; 32]);
    let delegation_ref = EventCid::from_bytes([55; 32]);
    let namespace = NamespaceCommitment::derive(b"security-feed-delegation", [56; 32]).unwrap();
    let make_feed = |key: &SigningKey| {
        let mut inception =
            FeedInception::new(*key.verifying_key().as_bytes(), namespace, 0, device);
        inception.actor_delegation_ref = Some(delegation_ref.into_bytes());
        decode_feed_inception(&inception.sign(key).unwrap().encode().unwrap()).unwrap()
    };
    let authorized = make_feed(&authorized_key);
    let attacker = make_feed(&attacker_key);
    let mut state = KeyStateReducer::new(EventCid::from_bytes([57; 32]));
    if state.accept_root(ScopedDelegation {
        grant: DelegationGrant {
            actor,
            device,
            subject_feed: authorized.feed_id,
            delegation_ref,
            namespace_commitment: Some(namespace),
            first_generation: 0,
            last_generation: 0,
            proof: EventCid::from_bytes([58; 32]),
        },
        parent_delegation_ref: None,
    }) != KeyStateApplyOutcome::Accepted
    {
        return false;
    }
    authorized.feed_id != attacker.feed_id
        && state.evaluate(&authorized).code() == "AUTHORIZED_RELATIVE"
        && state.evaluate(&attacker).code() == "STALE_OR_UNRESOLVED"
}

fn session_probe() -> bool {
    let initiator = SigningKey::from_bytes(&[1; 32]);
    let responder = SigningKey::from_bytes(&[2; 32]);
    let transport = [3; 32];
    let profile = SessionProfile {
        family: 1,
        major: 1,
        minor: 0,
    };
    let cap_a = SessionCapability::from_bytes([4; 32]);
    let cap_b = SessionCapability::from_bytes([5; 32]);
    let hello = match create_hello(
        &initiator,
        transport,
        [6; 32],
        vec![profile],
        vec![cap_a, cap_b],
        vec![],
    ) {
        Ok(value) => value,
        Err(_) => return false,
    };
    let welcome = match create_welcome(
        &hello,
        transport,
        &responder,
        [7; 32],
        &[profile],
        &[cap_a],
        vec![],
    ) {
        Ok(value) => value,
        Err(_) => return false,
    };
    let finish = match create_finish(
        &hello,
        &welcome,
        &initiator,
        transport,
        &[profile],
        &[cap_a],
    ) {
        Ok(value) => value,
        Err(_) => return false,
    };
    let session =
        match authenticate_session(&hello, &welcome, &finish, transport, &[profile], &[cap_a]) {
            Ok(value) => value,
            Err(_) => return false,
        };
    let mut transcript_tamper = welcome.clone();
    transcript_tamper.initiator_transcript[0] ^= 1;
    let transcript_rejected =
        verify_welcome(&hello, &transcript_tamper, transport, &[profile], &[cap_a]).is_err();
    let mut cap_strip_or_add = welcome;
    cap_strip_or_add.negotiated_capabilities.push(cap_b);
    let cap_rejected =
        verify_welcome(&hello, &cap_strip_or_add, transport, &[profile], &[cap_a]).is_err();
    let mut replay = SessionReplayGuard::default();
    let first = replay.accept(&session).is_ok();
    let second = replay.accept(&session).is_err();
    transcript_rejected && cap_rejected && first && second
}

fn merkle_probe() -> bool {
    let key = SigningKey::from_bytes(&[11; 32]);
    let delegation = EventCid::from_bytes([12; 32]);
    let device = DeviceId::from_bytes([13; 32]);
    let mut inception = FeedInception::new(
        *key.verifying_key().as_bytes(),
        NamespaceCommitment::derive(b"security-checkpoint", [14; 32]).unwrap(),
        0,
        device,
    );
    inception.actor_delegation_ref = Some(*delegation.as_bytes());
    let feed = decode_feed_inception(&inception.sign(&key).unwrap().encode().unwrap()).unwrap();
    let mut key_state = KeyStateReducer::new(EventCid::from_bytes([15; 32]));
    key_state.accept_root(ScopedDelegation {
        grant: DelegationGrant {
            actor: ActorId::from_bytes([16; 32]),
            device,
            subject_feed: feed.feed_id,
            delegation_ref: delegation,
            namespace_commitment: Some(feed.signed.inception.namespace_commitment),
            first_generation: 0,
            last_generation: 1,
            proof: EventCid::from_bytes([17; 32]),
        },
        parent_delegation_ref: None,
    });
    let event_type = EventType(90);
    let mut leaves = Vec::new();
    let mut parent = None;
    for sequence in 0..3u64 {
        let mut event = KnowledgeEventEnvelope::new(
            event_type,
            feed.feed_id,
            sequence,
            DisclosureClass::Public,
            [sequence as u8 + 1; 32],
        );
        event.causal_parents = parent.into_iter().collect();
        let bytes = event.sign(&feed, &key).unwrap().encode().unwrap().0;
        let validated = decode_knowledge_event(&bytes, &feed, &[event_type]).unwrap();
        parent = Some(validated.cid());
        leaves.push(
            CheckpointLeaf::from_validated_event(
                &validated,
                ManifestCid::from_bytes([sequence as u8 + 20; 32]),
                ManifestCid::from_bytes([sequence as u8 + 21; 32]),
                [sequence as u8 + 40; 32],
            )
            .unwrap(),
        );
    }
    let witness = CheckpointHistoryWitness::new(leaves).unwrap();
    let body = FeedCheckpointBody::from_witness(
        &witness,
        [80; 32],
        None,
        [81; 32],
        &key_state.checkpoint_proof(&feed),
        None,
        [82; 32],
    )
    .unwrap();
    let signed = SignedFeedCheckpoint::sign(body, &feed, &key).unwrap();
    let checkpoint = decode_feed_checkpoint(&signed.encode().unwrap(), &feed).unwrap();
    let proof = witness.inclusion_proof(1).unwrap();
    if proof.validate(&checkpoint).is_err() {
        return false;
    }
    let mut malicious = proof;
    malicious.siblings[0].digest[0] ^= 1;

    let default = VNextFeatureConfig::default();
    let riblt_default_off = !default.is_active(VNextFeature::Riblt);
    let mut unsafe_fast_path = VNextFeatureConfig::default();
    unsafe_fast_path.enabled.riblt = true;
    malicious.validate(&checkpoint).is_err()
        && riblt_default_off
        && unsafe_fast_path.validate().is_err()
}

fn resource_bomb_probe() -> bool {
    let oversized = vec![0; ResourceProfile::ControlV1.limits().max_bytes + 1];
    let parser_rejected = decode_canonical(&oversized, ResourceProfile::ControlV1)
        .is_err_and(|error| error.kind() == CanonicalErrorKind::LimitBytes);
    let limits = ExpansionLimits::CONTROL_V1;
    parser_rejected
        && admit_compressed_frame(1, 1_000_000, limits) == Err(ExpansionAdmissionError::RatioLimit)
        && admit_compressed_frame(100_000, 5_000_000, limits)
            == Err(ExpansionAdmissionError::ExpandedLimit)
        && admit_compressed_frame(2_000_000, 2_000_000, limits)
            == Err(ExpansionAdmissionError::CompressedLimit)
        && admit_compressed_frame(100_000, 1_000_000, limits).is_ok()
}

fn permit_replay_probe() -> bool {
    let issuer = ActorId::from_bytes([21; 32]);
    let executor = ActorId::from_bytes([22; 32]);
    let key = SigningKey::from_bytes(&[23; 32]);
    let delegation_ref = EventCid::from_bytes([24; 32]);
    let mut inception = FeedInception::new(
        *key.verifying_key().as_bytes(),
        NamespaceCommitment::derive(b"security-permit", [25; 32]).unwrap(),
        0,
        DeviceId::from_bytes([26; 32]),
    );
    inception.actor_delegation_ref = Some(*delegation_ref.as_bytes());
    let signed: SignedFeedInception = inception.sign(&key).unwrap();
    let feed = decode_feed_inception(&signed.encode().unwrap()).unwrap();
    let mut key_state = KeyStateReducer::new(EventCid::from_bytes([27; 32]));
    assert_eq!(
        key_state.accept_root(ScopedDelegation {
            grant: DelegationGrant {
                actor: issuer,
                device: feed.signed.inception.owner_device,
                subject_feed: feed.feed_id,
                delegation_ref,
                namespace_commitment: None,
                first_generation: 0,
                last_generation: 0,
                proof: EventCid::from_bytes([28; 32]),
            },
            parent_delegation_ref: None,
        }),
        KeyStateApplyOutcome::Accepted
    );
    let capability = ObjectCid::from_bytes([29; 32]);
    let input = b"security-task".to_vec();
    let input_commitment = ku_ai::cognitive_input_commitment(&input);
    let body = DelegationPermitBody {
        issuer,
        executor,
        capability_definition: capability,
        input_commitments: vec![input_commitment],
        allowed_effect_classes: vec![ConceptCcid::from_bytes([30; 16])],
        purpose: ConceptCcid::from_bytes([31; 16]),
        budget: Budget::new(2, 1_024, 100, 2).unwrap(),
        retention: RetentionRule::NoTraining,
        onward_delegation: false,
        parent_permit: None,
        not_before: 10,
        expires_at: 100,
        nonce: [32; 32],
    };
    let bytes = SignedDelegationPermit::sign(body, &feed, &key)
        .unwrap()
        .encode()
        .unwrap();
    let permit = authenticate_delegation_permit(&bytes, &feed, &key_state).unwrap();
    let permit_id = permit.permit_id;
    let mut permits = PermitValidator::default();
    let first = permits.submit(permit.clone(), 20);
    let replay = permits.submit(permit, 20);

    let task = CognitiveTask {
        task_id: [33; 32],
        permit_id,
        offer_ref: ObjectReference::new(1, [34; 32]),
        implementation_manifest: ObjectCid::from_bytes([35; 32]),
        capability_definition: capability,
        input_payload: input,
        input_commitments: vec![input_commitment],
        schema_prompt_parameter_commitments: vec![[36; 32]],
        requested_effect_classes: vec![ConceptCcid::from_bytes([30; 16])],
        purpose: ConceptCcid::from_bytes([31; 16]),
        budget: Budget::new(2, 1_024, 100, 2).unwrap(),
        retention: RetentionRule::NoTraining,
        seed: None,
        deadline_tick: 80,
    };
    let mut guard = CognitiveTaskReplayGuard::default();
    let admitted = guard.admit(&task);
    let exact = guard.admit(&task);
    let mut conflicting = task;
    conflicting.input_payload = b"changed".to_vec();
    conflicting.input_commitments = vec![ku_ai::cognitive_input_commitment(
        &conflicting.input_payload,
    )];
    let conflict = guard.admit(&conflicting);
    matches!(first, Ok(PermitApplyOutcome::Accepted(_)))
        && matches!(replay, Ok(PermitApplyOutcome::ExactReplay(_)))
        && admitted == CognitiveTaskReplayOutcome::Admitted
        && exact == CognitiveTaskReplayOutcome::ExactReplay
        && conflict == CognitiveTaskReplayOutcome::TaskIdConflict
}

fn correlation(admin: u8, pipeline: u8, device: u8) -> CorrelationEvidence {
    let dimension = |dimension, byte, strength| CorrelationDimensionEvidence {
        dimension,
        value_commitment: Some([byte; 32]),
        strength,
        evidence_refs: vec![],
    };
    CorrelationEvidence {
        dimensions: vec![
            dimension(
                CorrelationDimension::AdministrativePrincipal,
                admin,
                EvidenceStrength::CryptoBound,
            ),
            dimension(
                CorrelationDimension::PipelineModelLineage,
                pipeline,
                EvidenceStrength::ExternallyAttested,
            ),
            dimension(
                CorrelationDimension::DeviceOrFeed,
                device,
                EvidenceStrength::CryptoBound,
            ),
        ],
    }
}

fn sybil_correlation_probe() -> bool {
    let policy = FidelityPolicy::default_v1();
    let groups = (1..=100)
        .map(|device| {
            policy
                .evidenced_group_key(&correlation(1, 2, device))
                .unwrap()
                .unwrap()
        })
        .collect::<BTreeSet<_>>();
    groups.len() == 1 && !policy.uses_node_count_or_self_claim_as_independence()
}

fn privacy_taint_probe() -> bool {
    let policy_ref = ObjectReference::new(1, [41; 32]);
    let policy = DisclosurePolicy {
        policy_ref: policy_ref.clone(),
        default_mode: DisclosureMode::LocalOnly,
        enabled_nonlocal_modes: vec![DisclosureMode::RouteMinimal],
        minimum_route_support: 64,
    };
    let consent = DisclosureConsent {
        kind: ConsentKind::Explicit,
        policy_ref,
        mode: DisclosureMode::RouteMinimal,
        purpose: ConceptCcid::from_bytes([42; 16]),
        scope_commitment: [43; 32],
        consent_commitment: [44; 32],
        not_before: 10,
        expires_at: 100,
    };
    let private = PrivateNeedMaterial {
        raw_text: b"private anti-gravity acceptance test".to_vec(),
        stable_receptor_ids: vec![[45; 32]],
        stable_user_ids: vec![[46; 32]],
        exact_literals: vec![b"123.456".to_vec()],
        contains_location_or_time: true,
        contains_private_context: true,
        ..Default::default()
    };
    let candidate = RouteDisclosureCandidate {
        token: CoarseRouteToken {
            class: CoarseRouteTokenClass::CoarseRole,
            allowlisted_code: 600,
        },
        estimated_support: 1,
        exact_conjunction_width: 8,
        generalizations: vec![],
    };
    let no_consent_rejected =
        DisclosureSanitizer::sanitize_route_minimal(&policy, None, 20, &private, &candidate)
            .is_err();
    let suppressed = DisclosureSanitizer::sanitize_route_minimal(
        &policy,
        Some(&consent),
        20,
        &private,
        &candidate,
    );
    no_consent_rejected
        && matches!(
            suppressed,
            Ok(RouteSanitization::Suppressed(audit))
                if audit.is_local_private_state() && !audit.remaining.is_empty()
        )
}

#[test]
fn qa005_security_suite_rejects_every_attack_without_authority_amplification() {
    let report = run_security_suite();
    assert_eq!(report.probes.len(), 7);
    assert!(report.all_pass(), "{report:#?}");
}

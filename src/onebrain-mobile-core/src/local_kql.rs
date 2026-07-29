use ed25519_dalek::{Signature, Verifier, VerifyingKey};
use ku_core::{
    core_dna::{CoreDna, CoreDnaHeader, Instruction},
    foundation::{
        public_knowledge_exchange_fixture_v1, Budget, ConceptCcid, DisclosureClass,
        ObjectReference, SemanticFrameSet,
    },
    Epigenetics, KuRuntime,
};
use ku_kql::{
    executor::LocalExecutor,
    parser::parse_query,
    vnext_planner::{
        CancellationToken, CandidateGenerator, CandidatePage, CandidateRequest, CandidateSeed,
        CandidateValidation, ComplementPlanner, PlannerBudget, PlannerContinuation, PlannerError,
        PlannerOutcome, ProposalValidator,
    },
    vnext_query::{KnowledgeNeedIr, QueryChannel, QueryDefinition, QueryRun, QueryWorkItem},
};
use serde::Deserialize;

use crate::{MobileCoreError, ResourceBudgets};

const SIGNED_FIXTURE: &str = include_str!("../fixtures/signed_local_kql_smoke_v1.json");

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct SignedKqlFixture {
    format: String,
    payload: KqlFixturePayload,
    public_key_hex: String,
    signature_hex: String,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct KqlFixturePayload {
    query: String,
    concept: u64,
    predicate: u64,
    object: u64,
    certainty: u16,
    trust_score: u16,
    expected_rows: usize,
    private_planner: PrivatePlannerFixture,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct PrivatePlannerFixture {
    run_id_byte: u8,
    work_id_byte: u8,
    receptor_byte: u8,
    desired_role_byte: u8,
    query_policy_byte: u8,
    exploration_policy_byte: u8,
    candidate_byte: u8,
    max_candidates: u64,
    max_validations: u64,
    max_proposals: u64,
    max_work_units: u64,
    expected_examined_channels: usize,
    expected_rejected_candidates: usize,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct LocalKqlSmoke {
    pub signature_verified: bool,
    pub query_scope_local: bool,
    pub rows: usize,
    pub private_planner_verified: bool,
}

pub fn run_signed_local_kql_smoke(
    budgets: &ResourceBudgets,
) -> Result<LocalKqlSmoke, MobileCoreError> {
    let fixture: SignedKqlFixture = serde_json::from_str(SIGNED_FIXTURE)?;
    if fixture.format != "onebrain.mobile.kql-fixture/1" {
        return Err(MobileCoreError::SignedFixture(
            "unexpected fixture format".into(),
        ));
    }
    if fixture.payload.expected_rows > budgets.max_local_kql_results {
        return Err(MobileCoreError::BudgetExceeded(
            "fixture result count exceeds max_local_kql_results".into(),
        ));
    }
    let message = canonical_fixture_message(&fixture);
    let public_key_bytes = decode_fixed_hex::<32>(&fixture.public_key_hex, "public key")?;
    let signature_bytes = decode_fixed_hex::<64>(&fixture.signature_hex, "signature")?;
    let verifying_key = VerifyingKey::from_bytes(&public_key_bytes)
        .map_err(|error| MobileCoreError::SignedFixture(error.to_string()))?;
    let signature = Signature::from_bytes(&signature_bytes);
    verifying_key
        .verify(message.as_bytes(), &signature)
        .map_err(|error| MobileCoreError::SignedFixture(error.to_string()))?;

    let query_scope_local = fixture.payload.query.contains("SCOPE LOCAL");
    if !query_scope_local {
        return Err(MobileCoreError::SignedFixture(
            "fixture query must explicitly use SCOPE LOCAL".into(),
        ));
    }
    let dna = CoreDna {
        header: CoreDnaHeader {
            version: 2,
            gene_type: 0,
            has_concept_table: false,
        },
        concept_table: Vec::new(),
        instructions: vec![
            Instruction::Triple {
                s: fixture.payload.concept,
                p: fixture.payload.predicate,
                o: fixture.payload.object,
            },
            Instruction::Certainty {
                level: fixture.payload.certainty,
            },
        ],
    };
    let ku = KuRuntime::from_dna(dna)
        .map_err(|error| MobileCoreError::LocalKql(error.to_string()))?
        .with_epigenetics(Epigenetics::with_trust(fixture.payload.trust_score, 5000));
    let query = parse_query(&fixture.payload.query)
        .map_err(|error| MobileCoreError::LocalKql(error.to_string()))?;
    let mut executor = LocalExecutor::new();
    executor.insert(ku);
    let result = executor
        .execute(&query)
        .map_err(|error| MobileCoreError::LocalKql(error.to_string()))?;
    if result.total_count != fixture.payload.expected_rows {
        return Err(MobileCoreError::LocalKql(format!(
            "expected {} rows, observed {}",
            fixture.payload.expected_rows, result.total_count
        )));
    }
    run_private_planner_smoke(&fixture.payload.private_planner)?;
    Ok(LocalKqlSmoke {
        signature_verified: true,
        query_scope_local,
        rows: result.total_count,
        private_planner_verified: true,
    })
}

fn canonical_fixture_message(fixture: &SignedKqlFixture) -> String {
    format!(
        "{}\n{}\n{}\n{}\n{}\n{}\n{}\n{}\n{}\n{}\n{}\n{}\n{}\n{}\n{}\n{}\n{}\n{}\n{}\n{}\n{}\n",
        fixture.format,
        fixture.payload.query,
        fixture.payload.concept,
        fixture.payload.predicate,
        fixture.payload.object,
        fixture.payload.certainty,
        fixture.payload.trust_score,
        fixture.payload.expected_rows,
        fixture.payload.private_planner.run_id_byte,
        fixture.payload.private_planner.work_id_byte,
        fixture.payload.private_planner.receptor_byte,
        fixture.payload.private_planner.desired_role_byte,
        fixture.payload.private_planner.query_policy_byte,
        fixture.payload.private_planner.exploration_policy_byte,
        fixture.payload.private_planner.candidate_byte,
        fixture.payload.private_planner.max_candidates,
        fixture.payload.private_planner.max_validations,
        fixture.payload.private_planner.max_proposals,
        fixture.payload.private_planner.max_work_units,
        fixture.payload.private_planner.expected_examined_channels,
        fixture.payload.private_planner.expected_rejected_candidates,
    )
}

fn run_private_planner_smoke(fixture: &PrivatePlannerFixture) -> Result<(), MobileCoreError> {
    let reference = |byte: u8| ObjectReference::new(0, [byte; 32]);
    let definition = QueryDefinition {
        need: KnowledgeNeedIr {
            receptor_definitions: vec![reference(fixture.receptor_byte)],
            desired_roles: vec![ConceptCcid::from_bytes([fixture.desired_role_byte; 16])],
            goal: SemanticFrameSet {
                statements: Vec::new(),
            },
            local_context: SemanticFrameSet {
                statements: Vec::new(),
            },
            privacy: DisclosureClass::LocalOnly,
        },
        query_policy: reference(fixture.query_policy_byte),
        exploration_policy: reference(fixture.exploration_policy_byte),
    };
    let private_bytes = definition
        .private_canonical_bytes()
        .map_err(planner_failure)?;
    let decoded =
        QueryDefinition::from_private_canonical_bytes(&private_bytes).map_err(planner_failure)?;
    if decoded != definition || decoded.need.privacy != DisclosureClass::LocalOnly {
        return Err(MobileCoreError::LocalKql(
            "private planner definition did not round-trip as LocalOnly".into(),
        ));
    }
    let run = QueryRun::new(
        [fixture.run_id_byte; 32],
        decoded.private_cid().map_err(planner_failure)?,
        public_knowledge_exchange_fixture_v1(),
    )
    .map_err(planner_failure)?;
    let work = QueryWorkItem {
        work_id: [fixture.work_id_byte; 32],
        run_id: *run.run_id(),
        channel: QueryChannel::Structural,
        boundary: run.selector_cid().map_err(planner_failure)?,
        budget: Budget::new(fixture.max_candidates, 1_000_000, fixture.max_work_units, 1)
            .map_err(planner_failure)?,
        continuation: None,
    };
    let candidate = CandidateSeed {
        candidate_id: [fixture.candidate_byte; 32],
        candidate_objects: vec![reference(fixture.candidate_byte)],
        channel: QueryChannel::Structural,
    };
    let mut generator = SignedFixtureGenerator { candidate };
    let mut generators: Vec<&mut dyn CandidateGenerator> = vec![&mut generator];
    let result = ComplementPlanner::new(PlannerBudget {
        max_candidates: fixture.max_candidates,
        max_validations: fixture.max_validations,
        max_proposals: fixture.max_proposals,
        max_work_units: fixture.max_work_units,
    })
    .map_err(planner_failure)?
    .run(
        &run,
        &work,
        &mut generators,
        &mut SignedFixtureValidator,
        PlannerContinuation::default(),
        &CancellationToken::default(),
    )
    .map_err(planner_failure)?;
    if result.outcome != PlannerOutcome::CompleteForCurrentChannelPages
        || result.examined_channels.len() != fixture.expected_examined_channels
        || result.rejected_candidates.len() != fixture.expected_rejected_candidates
        || !result.portfolio.is_empty()
    {
        return Err(MobileCoreError::LocalKql(
            "signed private planner outcome did not match the fixture".into(),
        ));
    }
    Ok(())
}

struct SignedFixtureGenerator {
    candidate: CandidateSeed,
}

impl CandidateGenerator for SignedFixtureGenerator {
    fn channel(&self) -> QueryChannel {
        QueryChannel::Structural
    }

    fn generate(&mut self, _request: CandidateRequest) -> Result<CandidatePage, PlannerError> {
        Ok(CandidatePage {
            candidates: vec![self.candidate.clone()],
            consumed_work_units: 1,
            continuation: None,
        })
    }
}

struct SignedFixtureValidator;

impl ProposalValidator for SignedFixtureValidator {
    fn validate(
        &mut self,
        _candidate: &CandidateSeed,
    ) -> Result<CandidateValidation, PlannerError> {
        Ok(CandidateValidation::Rejected {
            reason: "signed_fixture_probe",
        })
    }
}

fn planner_failure(error: impl std::fmt::Debug) -> MobileCoreError {
    MobileCoreError::LocalKql(format!("private planner smoke: {error:?}"))
}

fn decode_fixed_hex<const N: usize>(value: &str, label: &str) -> Result<[u8; N], MobileCoreError> {
    let bytes =
        hex::decode(value).map_err(|error| MobileCoreError::SignedFixture(error.to_string()))?;
    bytes.try_into().map_err(|_| {
        MobileCoreError::SignedFixture(format!("{label} must contain exactly {N} bytes"))
    })
}

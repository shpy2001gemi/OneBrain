//! Typed, permit-gated local cognitive execution.
//!
//! This API is deliberately not a chat API. Backends receive a capability,
//! committed typed input and bounded continuation state. The executor owns
//! permit scope checks, logical deadlines, cancellation boundaries, resource
//! ceilings and immutable provenance construction.

use std::collections::{BTreeMap, BTreeSet};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

use ku_core::foundation::{
    Budget, CapabilityError, CapabilityExecutionRecordBody, CapabilityExecutionState, ConceptCcid,
    ObjectCid, ObjectReference, PermitCid, PermitExecutionScope, PermitValidationError,
    PermitValidator, RetentionRule,
};

pub const COGNITIVE_OUTPUT_COMMITMENT_REFERENCE_KIND: u64 = 1;
pub const MAX_COGNITIVE_INPUT_BYTES: usize = 16 * 1_048_576;
pub const MAX_CONTINUATION_BYTES: usize = 1_048_576;

#[derive(Clone, Default, Debug)]
pub struct CancellationToken {
    cancelled: Arc<AtomicBool>,
}

impl CancellationToken {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn cancel(&self) {
        self.cancelled.store(true, Ordering::Release);
    }

    pub fn is_cancelled(&self) -> bool {
        self.cancelled.load(Ordering::Acquire)
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CognitiveTask {
    pub task_id: [u8; 32],
    pub permit_id: PermitCid,
    pub offer_ref: ObjectReference,
    pub implementation_manifest: ObjectCid,
    pub capability_definition: ObjectCid,
    pub input_payload: Vec<u8>,
    pub input_commitments: Vec<[u8; 32]>,
    pub schema_prompt_parameter_commitments: Vec<[u8; 32]>,
    pub requested_effect_classes: Vec<ConceptCcid>,
    pub purpose: ConceptCcid,
    pub budget: Budget,
    pub retention: RetentionRule,
    pub seed: Option<[u8; 32]>,
    /// Exclusive logical local tick. No wall-clock/global-time claim is made.
    pub deadline_tick: u64,
}

impl CognitiveTask {
    fn permit_scope(&self) -> PermitExecutionScope {
        PermitExecutionScope {
            capability_definition: self.capability_definition,
            input_commitments: self.input_commitments.clone(),
            requested_effect_classes: self.requested_effect_classes.clone(),
            purpose: self.purpose,
            budget: self.budget,
            retention: self.retention,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct CognitiveStepBudget {
    pub remaining_output_records: u64,
    pub remaining_output_bytes: u64,
    pub remaining_work_units: u64,
    pub remaining_steps: u32,
}

pub struct CognitiveStepRequest<'a> {
    pub task_id: [u8; 32],
    pub capability_definition: ObjectCid,
    pub input_payload: &'a [u8],
    pub input_commitments: &'a [[u8; 32]],
    pub schema_prompt_parameter_commitments: &'a [[u8; 32]],
    pub seed: Option<[u8; 32]>,
    pub continuation: Option<&'a [u8]>,
    pub budget: CognitiveStepBudget,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CognitiveStep {
    pub output_fragment: Vec<u8>,
    /// `None` marks completion; `Some` requests another bounded step.
    pub continuation: Option<Vec<u8>>,
    pub work_units: u64,
    pub elapsed_ticks: u64,
    pub limitations: Vec<ConceptCcid>,
}

pub trait TypedCapabilityBackend {
    fn execute_step(&mut self, request: CognitiveStepRequest<'_>) -> Result<CognitiveStep, String>;
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CognitiveTermination {
    Completed,
    Cancelled,
    DeadlineExceeded,
    ResourceExceeded,
    BackendFailed,
    BackendProtocolViolation,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct CognitiveExecutionPolicy {
    pub cancelled_limitation: ConceptCcid,
    pub deadline_limitation: ConceptCcid,
    pub resource_limitation: ConceptCcid,
    pub backend_failure_limitation: ConceptCcid,
    pub backend_protocol_limitation: ConceptCcid,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CognitiveExecutionResult {
    pub output: Vec<u8>,
    pub output_commitment: [u8; 32],
    pub termination: CognitiveTermination,
    pub consumed_work_units: u64,
    pub completed_steps: u32,
    pub backend_error_commitment: Option<[u8; 32]>,
    pub record: CapabilityExecutionRecordBody,
}

impl CognitiveExecutionResult {
    pub fn is_partial(&self) -> bool {
        self.termination != CognitiveTermination::Completed && !self.output.is_empty()
    }

    pub const fn establishes_correctness(&self) -> bool {
        false
    }

    pub const fn materializes_output(&self) -> bool {
        false
    }

    pub const fn publishes_output(&self) -> bool {
        false
    }
}

pub struct TypedCognitiveExecutor {
    policy: CognitiveExecutionPolicy,
}

impl TypedCognitiveExecutor {
    pub const fn new(policy: CognitiveExecutionPolicy) -> Self {
        Self { policy }
    }

    /// Permit-gated execution with an additional local task replay boundary.
    /// Remote task handlers MUST use this entry point rather than `execute`.
    pub fn execute_once<B: TypedCapabilityBackend>(
        &self,
        backend: &mut B,
        permits: &PermitValidator,
        replay_guard: &mut CognitiveTaskReplayGuard,
        task: CognitiveTask,
        started_at: u64,
        cancellation: &CancellationToken,
    ) -> Result<CognitiveExecutionResult, CognitiveExecutionError> {
        validate_task(&task, started_at)?;
        let permit = permits.authorize_scope(task.permit_id, started_at, &task.permit_scope())?;
        if task.deadline_tick > permit.body.expires_at {
            return Err(CognitiveExecutionError::DeadlineExceedsPermit);
        }
        match replay_guard.admit(&task) {
            CognitiveTaskReplayOutcome::Admitted => {
                self.execute(backend, permits, task, started_at, cancellation)
            }
            CognitiveTaskReplayOutcome::ExactReplay => Err(CognitiveExecutionError::TaskReplay),
            CognitiveTaskReplayOutcome::TaskIdConflict => {
                Err(CognitiveExecutionError::TaskIdentityConflict)
            }
        }
    }

    pub fn execute<B: TypedCapabilityBackend>(
        &self,
        backend: &mut B,
        permits: &PermitValidator,
        task: CognitiveTask,
        started_at: u64,
        cancellation: &CancellationToken,
    ) -> Result<CognitiveExecutionResult, CognitiveExecutionError> {
        validate_task(&task, started_at)?;
        let permit = permits.authorize_scope(task.permit_id, started_at, &task.permit_scope())?;
        if task.deadline_tick > permit.body.expires_at {
            return Err(CognitiveExecutionError::DeadlineExceedsPermit);
        }

        let mut trace = blake3::Hasher::new();
        trace.update(b"onebrain:vnext:cognitive-execution-trace:1\0");
        trace.update(&task.task_id);
        trace.update(&started_at.to_be_bytes());

        let mut output = Vec::new();
        let mut continuation: Option<Vec<u8>> = None;
        let mut work_units = 0_u64;
        let mut records = 0_u64;
        let mut steps = 0_u32;
        let mut tick = started_at;
        let mut limitations = BTreeSet::new();
        let mut backend_error_commitment = None;

        let termination = loop {
            // Cancellation is observed only at a step boundary. A step that
            // already completed is either wholly admitted or wholly rejected.
            if cancellation.is_cancelled() {
                break CognitiveTermination::Cancelled;
            }
            if tick >= task.deadline_tick {
                break CognitiveTermination::DeadlineExceeded;
            }
            if steps >= task.budget.max_depth {
                break CognitiveTermination::ResourceExceeded;
            }

            let request = CognitiveStepRequest {
                task_id: task.task_id,
                capability_definition: task.capability_definition,
                input_payload: &task.input_payload,
                input_commitments: &task.input_commitments,
                schema_prompt_parameter_commitments: &task.schema_prompt_parameter_commitments,
                seed: task.seed,
                continuation: continuation.as_deref(),
                budget: CognitiveStepBudget {
                    remaining_output_records: task.budget.max_records - records,
                    remaining_output_bytes: task.budget.max_bytes
                        - u64::try_from(output.len()).unwrap_or(u64::MAX),
                    remaining_work_units: task.budget.max_work_units - work_units,
                    remaining_steps: task.budget.max_depth - steps,
                },
            };
            let step = match backend.execute_step(request) {
                Ok(step) => step,
                Err(error) => {
                    let commitment = private_error_commitment(error.as_bytes());
                    backend_error_commitment = Some(commitment);
                    trace.update(b"backend-error");
                    trace.update(&commitment);
                    break CognitiveTermination::BackendFailed;
                }
            };
            if step.elapsed_ticks == 0
                || step
                    .continuation
                    .as_ref()
                    .is_some_and(|value| value.len() > MAX_CONTINUATION_BYTES)
            {
                trace.update(b"backend-protocol");
                break CognitiveTermination::BackendProtocolViolation;
            }
            let Some(next_tick) = tick.checked_add(step.elapsed_ticks) else {
                trace.update(b"deadline-overflow");
                tick = task.deadline_tick;
                break CognitiveTermination::DeadlineExceeded;
            };
            // The deadline is exclusive. A fragment completing after it is not
            // admitted into the partial result.
            if next_tick > task.deadline_tick {
                trace.update(b"deadline-crossed");
                tick = task.deadline_tick;
                break CognitiveTermination::DeadlineExceeded;
            }
            let fragment_bytes = u64::try_from(step.output_fragment.len()).unwrap_or(u64::MAX);
            if records == task.budget.max_records
                || fragment_bytes > task.budget.max_bytes.saturating_sub(output.len() as u64)
                || step.work_units > task.budget.max_work_units.saturating_sub(work_units)
            {
                trace.update(b"resource-exceeded");
                break CognitiveTermination::ResourceExceeded;
            }

            let fragment_commitment = cognitive_output_commitment(&step.output_fragment);
            trace.update(b"step");
            trace.update(&fragment_commitment);
            trace.update(&step.work_units.to_be_bytes());
            trace.update(&step.elapsed_ticks.to_be_bytes());
            output.extend_from_slice(&step.output_fragment);
            work_units += step.work_units;
            records += 1;
            steps += 1;
            tick = next_tick;
            limitations.extend(step.limitations);
            continuation = step.continuation;
            if continuation.is_none() {
                break CognitiveTermination::Completed;
            }
        };

        if let Some(limitation) = self.termination_limitation(termination) {
            limitations.insert(limitation);
        }
        let output_commitment = cognitive_output_commitment(&output);
        trace.update(&[termination as u8]);
        trace.update(&output_commitment);
        trace.update(&tick.to_be_bytes());
        trace.update(&work_units.to_be_bytes());
        let log_digest = *trace.finalize().as_bytes();
        let state = match termination {
            CognitiveTermination::Completed => CapabilityExecutionState::Completed,
            CognitiveTermination::Cancelled => CapabilityExecutionState::Cancelled,
            CognitiveTermination::DeadlineExceeded | CognitiveTermination::ResourceExceeded
                if !output.is_empty() =>
            {
                CapabilityExecutionState::Partial
            }
            CognitiveTermination::DeadlineExceeded
            | CognitiveTermination::ResourceExceeded
            | CognitiveTermination::BackendFailed
            | CognitiveTermination::BackendProtocolViolation => CapabilityExecutionState::Failed,
        };
        let record = CapabilityExecutionRecordBody {
            task_id: task.task_id,
            offer_ref: task.offer_ref,
            implementation_manifest: task.implementation_manifest,
            input_commitments: task.input_commitments,
            schema_prompt_parameter_commitments: task.schema_prompt_parameter_commitments,
            output_refs_or_commitments: vec![ObjectReference::new(
                COGNITIVE_OUTPUT_COMMITMENT_REFERENCE_KIND,
                output_commitment,
            )],
            state,
            started_at,
            finished_at: tick,
            limitations: limitations.into_iter().collect(),
            log_digest,
            optional_attestation: None,
            retention_claim: task.retention,
        };
        record.canonical_body()?;
        Ok(CognitiveExecutionResult {
            output,
            output_commitment,
            termination,
            consumed_work_units: work_units,
            completed_steps: steps,
            backend_error_commitment,
            record,
        })
    }

    fn termination_limitation(&self, termination: CognitiveTermination) -> Option<ConceptCcid> {
        match termination {
            CognitiveTermination::Completed => None,
            CognitiveTermination::Cancelled => Some(self.policy.cancelled_limitation),
            CognitiveTermination::DeadlineExceeded => Some(self.policy.deadline_limitation),
            CognitiveTermination::ResourceExceeded => Some(self.policy.resource_limitation),
            CognitiveTermination::BackendFailed => Some(self.policy.backend_failure_limitation),
            CognitiveTermination::BackendProtocolViolation => {
                Some(self.policy.backend_protocol_limitation)
            }
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CognitiveTaskReplayOutcome {
    Admitted,
    ExactReplay,
    TaskIdConflict,
}

/// Local replay guard. It binds a task ID to every execution-relevant field;
/// exact replay and same-ID mutation are both rejected before backend work.
#[derive(Default)]
pub struct CognitiveTaskReplayGuard {
    admitted: BTreeMap<[u8; 32], [u8; 32]>,
}

impl CognitiveTaskReplayGuard {
    pub fn admit(&mut self, task: &CognitiveTask) -> CognitiveTaskReplayOutcome {
        let commitment = cognitive_task_commitment(task);
        match self.admitted.get(&task.task_id) {
            Some(existing) if existing == &commitment => CognitiveTaskReplayOutcome::ExactReplay,
            Some(_) => CognitiveTaskReplayOutcome::TaskIdConflict,
            None => {
                self.admitted.insert(task.task_id, commitment);
                CognitiveTaskReplayOutcome::Admitted
            }
        }
    }

    pub fn contains(&self, task_id: &[u8; 32]) -> bool {
        self.admitted.contains_key(task_id)
    }
}

pub fn cognitive_input_commitment(input: &[u8]) -> [u8; 32] {
    domain_commitment(b"onebrain:vnext:cognitive-input:1\0", input)
}

pub fn cognitive_output_commitment(output: &[u8]) -> [u8; 32] {
    domain_commitment(b"onebrain:vnext:cognitive-output:1\0", output)
}

pub fn cognitive_task_commitment(task: &CognitiveTask) -> [u8; 32] {
    let mut input_commitments = task.input_commitments.clone();
    input_commitments.sort();
    let mut prompt_commitments = task.schema_prompt_parameter_commitments.clone();
    prompt_commitments.sort();
    let mut effects = task.requested_effect_classes.clone();
    effects.sort_by(|left, right| left.as_bytes().cmp(right.as_bytes()));
    let mut hasher = blake3::Hasher::new();
    hasher.update(b"onebrain:vnext:cognitive-task:1\0");
    hasher.update(&task.task_id);
    hasher.update(task.permit_id.as_bytes());
    hasher.update(&task.offer_ref.reference_kind.to_be_bytes());
    hasher.update(&task.offer_ref.cid);
    hasher.update(task.implementation_manifest.as_bytes());
    hasher.update(task.capability_definition.as_bytes());
    hasher.update(&cognitive_input_commitment(&task.input_payload));
    for commitment in input_commitments {
        hasher.update(&commitment);
    }
    for commitment in prompt_commitments {
        hasher.update(&commitment);
    }
    for effect in effects {
        hasher.update(effect.as_bytes());
    }
    hasher.update(task.purpose.as_bytes());
    hasher.update(&task.budget.max_records.to_be_bytes());
    hasher.update(&task.budget.max_bytes.to_be_bytes());
    hasher.update(&task.budget.max_work_units.to_be_bytes());
    hasher.update(&task.budget.max_depth.to_be_bytes());
    hasher.update(&(task.retention as u64).to_be_bytes());
    match task.seed {
        Some(seed) => {
            hasher.update(&[1]);
            hasher.update(&seed);
        }
        None => {
            hasher.update(&[0]);
        }
    }
    hasher.update(&task.deadline_tick.to_be_bytes());
    *hasher.finalize().as_bytes()
}

fn private_error_commitment(error: &[u8]) -> [u8; 32] {
    domain_commitment(b"onebrain:vnext:cognitive-private-error:1\0", error)
}

fn domain_commitment(domain: &[u8], value: &[u8]) -> [u8; 32] {
    let mut hasher = blake3::Hasher::new();
    hasher.update(domain);
    hasher.update(&(value.len() as u64).to_be_bytes());
    hasher.update(value);
    *hasher.finalize().as_bytes()
}

fn validate_task(task: &CognitiveTask, started_at: u64) -> Result<(), CognitiveExecutionError> {
    if task.task_id == [0; 32]
        || task.implementation_manifest.as_bytes() == &[0; 32]
        || task.input_payload.is_empty()
        || task.input_payload.len() > MAX_COGNITIVE_INPUT_BYTES
        || task.input_commitments.is_empty()
        || task.requested_effect_classes.is_empty()
        || task.deadline_tick <= started_at
    {
        return Err(CognitiveExecutionError::InvalidTask);
    }
    Budget::new(
        task.budget.max_records,
        task.budget.max_bytes,
        task.budget.max_work_units,
        task.budget.max_depth,
    )
    .map_err(|_| CognitiveExecutionError::InvalidTask)?;
    if !task
        .input_commitments
        .contains(&cognitive_input_commitment(&task.input_payload))
        || has_duplicates(&task.input_commitments)
        || has_duplicates(&task.schema_prompt_parameter_commitments)
        || has_duplicates(&task.requested_effect_classes)
    {
        return Err(CognitiveExecutionError::InvalidTask);
    }
    Ok(())
}

fn has_duplicates<T: Ord + Copy>(values: &[T]) -> bool {
    values.iter().copied().collect::<BTreeSet<_>>().len() != values.len()
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum CognitiveExecutionError {
    InvalidTask,
    DeadlineExceedsPermit,
    TaskReplay,
    TaskIdentityConflict,
    Permit(PermitValidationError),
    Capability(CapabilityError),
}

impl From<PermitValidationError> for CognitiveExecutionError {
    fn from(error: PermitValidationError) -> Self {
        Self::Permit(error)
    }
}

impl From<CapabilityError> for CognitiveExecutionError {
    fn from(error: CapabilityError) -> Self {
        Self::Capability(error)
    }
}

#[cfg(test)]
mod tests {
    use ed25519_dalek::SigningKey;
    use ku_core::foundation::{
        authenticate_delegation_permit, decode_feed_inception, ActorId, DelegationGrant,
        DelegationPermitBody, DeviceId, EventCid, FeedInception, KeyStateApplyOutcome,
        KeyStateReducer, NamespaceCommitment, ScopedDelegation, SignedDelegationPermit,
        SignedFeedInception,
    };

    use super::*;

    fn concept(byte: u8) -> ConceptCcid {
        ConceptCcid::from_bytes([byte; 16])
    }

    fn setup() -> (PermitValidator, CognitiveTask) {
        let actor = ActorId::from_bytes([1; 32]);
        let key = SigningKey::from_bytes(&[2; 32]);
        let delegation_ref = EventCid::from_bytes([3; 32]);
        let mut inception = FeedInception::new(
            *key.verifying_key().as_bytes(),
            NamespaceCommitment::derive(b"cognitive-executor", [4; 32]).unwrap(),
            0,
            DeviceId::from_bytes([5; 32]),
        );
        inception.actor_delegation_ref = Some(delegation_ref.into_bytes());
        let signed: SignedFeedInception = inception.sign(&key).unwrap();
        let feed = decode_feed_inception(&signed.encode().unwrap()).unwrap();
        let mut key_state = KeyStateReducer::new(EventCid::from_bytes([6; 32]));
        assert_eq!(
            key_state.accept_root(ScopedDelegation {
                grant: DelegationGrant {
                    actor,
                    device: feed.signed.inception.owner_device,
                    subject_feed: feed.feed_id,
                    delegation_ref,
                    namespace_commitment: None,
                    first_generation: 0,
                    last_generation: 0,
                    proof: EventCid::from_bytes([7; 32]),
                },
                parent_delegation_ref: None,
            }),
            KeyStateApplyOutcome::Accepted
        );
        let payload = b"typed-input-v1".to_vec();
        let input_commitment = cognitive_input_commitment(&payload);
        let capability = ObjectCid::from_bytes([8; 32]);
        let effect = concept(9);
        let purpose = concept(10);
        let budget = Budget::new(4, 1024, 100, 4).unwrap();
        let body = DelegationPermitBody {
            issuer: actor,
            executor: actor,
            capability_definition: capability,
            input_commitments: vec![input_commitment],
            allowed_effect_classes: vec![effect],
            purpose,
            budget,
            retention: RetentionRule::NoTraining,
            onward_delegation: false,
            parent_permit: None,
            not_before: 10,
            expires_at: 100,
            nonce: [11; 32],
        };
        let bytes = SignedDelegationPermit::sign(body, &feed, &key)
            .unwrap()
            .encode()
            .unwrap();
        let authenticated = authenticate_delegation_permit(&bytes, &feed, &key_state).unwrap();
        let permit_id = authenticated.permit_id;
        let mut permits = PermitValidator::default();
        permits.submit(authenticated, 10).unwrap();
        let task = CognitiveTask {
            task_id: [12; 32],
            permit_id,
            offer_ref: ObjectReference::new(2, [13; 32]),
            implementation_manifest: ObjectCid::from_bytes([14; 32]),
            capability_definition: capability,
            input_payload: payload,
            input_commitments: vec![input_commitment],
            schema_prompt_parameter_commitments: vec![[15; 32]],
            requested_effect_classes: vec![effect],
            purpose,
            budget,
            retention: RetentionRule::NoTraining,
            seed: Some([16; 32]),
            deadline_tick: 80,
        };
        (permits, task)
    }

    fn policy() -> CognitiveExecutionPolicy {
        CognitiveExecutionPolicy {
            cancelled_limitation: concept(20),
            deadline_limitation: concept(21),
            resource_limitation: concept(22),
            backend_failure_limitation: concept(23),
            backend_protocol_limitation: concept(24),
        }
    }

    struct SequenceBackend {
        steps: Vec<CognitiveStep>,
        calls: usize,
    }

    impl TypedCapabilityBackend for SequenceBackend {
        fn execute_step(
            &mut self,
            _request: CognitiveStepRequest<'_>,
        ) -> Result<CognitiveStep, String> {
            let step = self
                .steps
                .get(self.calls)
                .cloned()
                .ok_or_else(|| "missing step".to_owned())?;
            self.calls += 1;
            Ok(step)
        }
    }

    #[test]
    fn completed_execution_is_typed_bounded_provenance_only() {
        let (permits, task) = setup();
        let mut backend = SequenceBackend {
            steps: vec![CognitiveStep {
                output_fragment: b"result".to_vec(),
                continuation: None,
                work_units: 7,
                elapsed_ticks: 2,
                limitations: vec![concept(30)],
            }],
            calls: 0,
        };
        let result = TypedCognitiveExecutor::new(policy())
            .execute(&mut backend, &permits, task, 20, &CancellationToken::new())
            .unwrap();
        assert_eq!(result.termination, CognitiveTermination::Completed);
        assert_eq!(result.record.state, CapabilityExecutionState::Completed);
        assert_eq!(
            result.output_commitment,
            cognitive_output_commitment(b"result")
        );
        assert!(!result.establishes_correctness());
        assert!(!result.materializes_output());
        assert!(!result.publishes_output());
    }

    #[test]
    fn pre_cancel_is_deterministic_and_never_calls_backend() {
        let (permits, task) = setup();
        let mut backend = SequenceBackend {
            steps: vec![],
            calls: 0,
        };
        let token = CancellationToken::new();
        token.cancel();
        let result = TypedCognitiveExecutor::new(policy())
            .execute(&mut backend, &permits, task, 20, &token)
            .unwrap();
        assert_eq!(backend.calls, 0);
        assert_eq!(result.termination, CognitiveTermination::Cancelled);
        assert_eq!(result.record.state, CapabilityExecutionState::Cancelled);
    }

    #[test]
    fn cancellation_at_next_step_boundary_preserves_committed_partial_output() {
        struct CancellingBackend {
            token: CancellationToken,
            calls: usize,
        }
        impl TypedCapabilityBackend for CancellingBackend {
            fn execute_step(
                &mut self,
                _request: CognitiveStepRequest<'_>,
            ) -> Result<CognitiveStep, String> {
                self.calls += 1;
                self.token.cancel();
                Ok(CognitiveStep {
                    output_fragment: b"accepted-before-boundary".to_vec(),
                    continuation: Some(b"next".to_vec()),
                    work_units: 1,
                    elapsed_ticks: 1,
                    limitations: vec![],
                })
            }
        }

        let (permits, task) = setup();
        let token = CancellationToken::new();
        let mut backend = CancellingBackend {
            token: token.clone(),
            calls: 0,
        };
        let result = TypedCognitiveExecutor::new(policy())
            .execute(&mut backend, &permits, task, 20, &token)
            .unwrap();
        assert_eq!(backend.calls, 1);
        assert_eq!(result.termination, CognitiveTermination::Cancelled);
        assert!(result.is_partial());
        assert_eq!(result.record.state, CapabilityExecutionState::Cancelled);
        assert_eq!(result.output, b"accepted-before-boundary");
    }

    #[test]
    fn crossing_deadline_discards_late_fragment_reproducibly() {
        let (permits, mut task) = setup();
        task.deadline_tick = 22;
        let run = || SequenceBackend {
            steps: vec![CognitiveStep {
                output_fragment: b"late".to_vec(),
                continuation: None,
                work_units: 1,
                elapsed_ticks: 3,
                limitations: vec![],
            }],
            calls: 0,
        };
        let executor = TypedCognitiveExecutor::new(policy());
        let mut left_backend = run();
        let left = executor
            .execute(
                &mut left_backend,
                &permits,
                task.clone(),
                20,
                &CancellationToken::new(),
            )
            .unwrap();
        let mut right_backend = run();
        let right = executor
            .execute(
                &mut right_backend,
                &permits,
                task,
                20,
                &CancellationToken::new(),
            )
            .unwrap();
        assert_eq!(left, right);
        assert_eq!(left.termination, CognitiveTermination::DeadlineExceeded);
        assert!(left.output.is_empty());
        assert_eq!(left.record.finished_at, 22);
    }

    #[test]
    fn step_ceiling_returns_committed_partial_result() {
        let (permits, mut task) = setup();
        task.budget = Budget::new(4, 1024, 100, 1).unwrap();
        let mut backend = SequenceBackend {
            steps: vec![CognitiveStep {
                output_fragment: b"partial".to_vec(),
                continuation: Some(b"continue".to_vec()),
                work_units: 5,
                elapsed_ticks: 1,
                limitations: vec![],
            }],
            calls: 0,
        };
        let result = TypedCognitiveExecutor::new(policy())
            .execute(&mut backend, &permits, task, 20, &CancellationToken::new())
            .unwrap();
        assert_eq!(result.termination, CognitiveTermination::ResourceExceeded);
        assert!(result.is_partial());
        assert_eq!(result.record.state, CapabilityExecutionState::Partial);
        assert_eq!(
            result.output_commitment,
            cognitive_output_commitment(b"partial")
        );
    }

    #[test]
    fn expanded_task_scope_is_rejected_before_backend() {
        let (permits, mut task) = setup();
        task.requested_effect_classes.push(concept(99));
        let mut backend = SequenceBackend {
            steps: vec![],
            calls: 0,
        };
        assert_eq!(
            TypedCognitiveExecutor::new(policy()).execute(
                &mut backend,
                &permits,
                task,
                20,
                &CancellationToken::new(),
            ),
            Err(CognitiveExecutionError::Permit(
                PermitValidationError::EffectExpansion
            ))
        );
        assert_eq!(backend.calls, 0);
    }

    #[test]
    fn execute_once_rejects_exact_replay_and_same_id_mutation_before_backend() {
        let (permits, task) = setup();
        let step = CognitiveStep {
            output_fragment: b"first-result".to_vec(),
            continuation: None,
            work_units: 1,
            elapsed_ticks: 1,
            limitations: vec![],
        };
        let mut backend = SequenceBackend {
            steps: vec![step],
            calls: 0,
        };
        let mut replay_guard = CognitiveTaskReplayGuard::default();
        let executor = TypedCognitiveExecutor::new(policy());

        let first = executor
            .execute_once(
                &mut backend,
                &permits,
                &mut replay_guard,
                task.clone(),
                20,
                &CancellationToken::new(),
            )
            .unwrap();
        assert_eq!(first.termination, CognitiveTermination::Completed);
        assert_eq!(backend.calls, 1);

        assert_eq!(
            executor.execute_once(
                &mut backend,
                &permits,
                &mut replay_guard,
                task.clone(),
                20,
                &CancellationToken::new(),
            ),
            Err(CognitiveExecutionError::TaskReplay)
        );
        assert_eq!(backend.calls, 1);

        let mut conflicting = task;
        // A seed change remains permit-valid but changes deterministic
        // execution semantics, so the same task ID must not be reused.
        conflicting.seed = Some([99; 32]);
        assert_eq!(
            executor.execute_once(
                &mut backend,
                &permits,
                &mut replay_guard,
                conflicting,
                20,
                &CancellationToken::new(),
            ),
            Err(CognitiveExecutionError::TaskIdentityConflict)
        );
        assert_eq!(backend.calls, 1);
    }
}

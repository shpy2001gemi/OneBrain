//! Revisioned, late-result-safe QueryView and propensity-aware local learner.
//!
//! Canonical artifacts deduplicate independently of route/source multiplicity.
//! Exposure is private attention telemetry and remains distinct from validated
//! Use evidence.

use std::collections::{BTreeMap, BTreeSet};

use ku_core::foundation::{
    encode_canonical, CoverageStatement, EventCid, ExactRatio, ExposureKind, ExposureObservation,
    InventoryError, MappingKernelCid, ObjectReference, ResourceProfile, SemanticError,
    ValidatedUseEvidenceEvent,
};

use crate::vnext_exploration::{PrivateSelectionRecord, SelectionPropensity};

pub const MAX_QUERY_VIEW_BATCHES: usize = 65_536;
pub const MAX_QUERY_VIEW_RESULTS: usize = 1_000_000;
pub const MAX_QUERY_BATCH_RESULTS: usize = 16_384;
pub const MAX_QUERY_DONE_RECEIPTS: usize = 65_536;
pub const MAX_LEARNING_OBSERVATIONS: usize = 1_000_000;

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum QueryArtifactRef {
    Object(ObjectReference),
    Mapping(MappingKernelCid),
    Event(EventCid),
}

impl QueryArtifactRef {
    fn key(&self) -> ArtifactKey {
        match self {
            Self::Object(reference) => ArtifactKey {
                domain: 0,
                cid: reference.cid,
            },
            Self::Mapping(mapping) => ArtifactKey {
                domain: 1,
                cid: mapping.into_bytes(),
            },
            Self::Event(event) => ArtifactKey {
                domain: 2,
                cid: event.into_bytes(),
            },
        }
    }

    fn to_value(&self) -> ku_core::foundation::CanonicalValue {
        use ku_core::foundation::CanonicalValue;
        match self {
            Self::Object(reference) => CanonicalValue::Map(vec![
                (0, CanonicalValue::Unsigned(0)),
                (1, CanonicalValue::Unsigned(reference.reference_kind)),
                (2, CanonicalValue::Bytes(reference.cid.to_vec())),
            ]),
            Self::Mapping(mapping) => CanonicalValue::Map(vec![
                (0, CanonicalValue::Unsigned(1)),
                (2, CanonicalValue::Bytes(mapping.as_bytes().to_vec())),
            ]),
            Self::Event(event) => CanonicalValue::Map(vec![
                (0, CanonicalValue::Unsigned(2)),
                (2, CanonicalValue::Bytes(event.as_bytes().to_vec())),
            ]),
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
struct ArtifactKey {
    domain: u8,
    cid: [u8; 32],
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct QueryBatchEvidence {
    pub run_id: [u8; 32],
    pub work_id: [u8; 32],
    pub results: Vec<QueryArtifactRef>,
    pub coverage: CoverageStatement,
}

impl QueryBatchEvidence {
    pub fn validate(&self) -> Result<(), QueryViewError> {
        if self.run_id == [0; 32]
            || self.work_id == [0; 32]
            || self.results.len() > MAX_QUERY_BATCH_RESULTS
        {
            return Err(QueryViewError::InvalidBatch);
        }
        self.coverage.validate()?;
        let mut identities = BTreeSet::new();
        for result in &self.results {
            if result.key().cid == [0; 32] || !identities.insert(result.key()) {
                return Err(QueryViewError::DuplicateBatchResult);
            }
        }
        Ok(())
    }

    pub fn batch_commitment(&self) -> Result<[u8; 32], QueryViewError> {
        self.validate()?;
        let value = ku_core::foundation::CanonicalValue::Map(vec![
            (
                0,
                ku_core::foundation::CanonicalValue::Bytes(self.run_id.to_vec()),
            ),
            (
                1,
                ku_core::foundation::CanonicalValue::Bytes(self.work_id.to_vec()),
            ),
            (2, canonical_artifact_set(&self.results)?),
            (3, self.coverage.canonical_value()?),
        ]);
        digest_value(b"query-batch", &value)
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct QueryDoneReceipt {
    pub run_id: [u8; 32],
    pub work_id: [u8; 32],
    pub assessed_frontier: [u8; 32],
    pub continuation: Option<[u8; 32]>,
}

impl QueryDoneReceipt {
    pub fn receipt_commitment(&self) -> Result<[u8; 32], QueryViewError> {
        if self.run_id == [0; 32] || self.work_id == [0; 32] || self.assessed_frontier == [0; 32] {
            return Err(QueryViewError::InvalidReceipt);
        }
        let mut fields = vec![
            (
                0,
                ku_core::foundation::CanonicalValue::Bytes(self.run_id.to_vec()),
            ),
            (
                1,
                ku_core::foundation::CanonicalValue::Bytes(self.work_id.to_vec()),
            ),
            (
                2,
                ku_core::foundation::CanonicalValue::Bytes(self.assessed_frontier.to_vec()),
            ),
        ];
        if let Some(continuation) = self.continuation {
            fields.push((
                3,
                ku_core::foundation::CanonicalValue::Bytes(continuation.to_vec()),
            ));
        }
        digest_value(
            b"query-done-receipt",
            &ku_core::foundation::CanonicalValue::Map(fields),
        )
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum QueryRevisionReason {
    BatchAdded,
    LateBatchAdded,
    ProvenanceOnlyBatchAdded,
    LateProvenanceOnlyBatchAdded,
    DoneReceiptAdded,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum QueryViewStatus {
    Open,
    WorkReceiptsObserved,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct QueryViewResult {
    pub artifact: QueryArtifactRef,
    /// Commitment to all accepted batch occurrences; deliberately not a rank
    /// feature and deliberately exposes no source-count scalar.
    pub occurrence_root: [u8; 32],
}

impl QueryViewResult {
    pub const fn source_count_affects_rank(&self) -> bool {
        false
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct QueryView {
    pub run_id: [u8; 32],
    pub revision: u64,
    pub revision_cid: [u8; 32],
    pub parent_revision: Option<[u8; 32]>,
    pub reason: QueryRevisionReason,
    pub status: QueryViewStatus,
    pub results: Vec<QueryViewResult>,
    pub batch_set_root: [u8; 32],
    pub receipt_set_root: [u8; 32],
    pub coverage_root: [u8; 32],
    pub coverage: Vec<CoverageStatement>,
}

impl QueryView {
    pub const fn is_globally_complete(&self) -> bool {
        false
    }

    pub const fn is_execution_authority(&self) -> bool {
        false
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum QueryBatchApplyOutcome {
    Added,
    LateAdded,
    ProvenanceOnlyAdded,
    LateProvenanceOnlyAdded,
    ExactReplay,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum QueryReceiptApplyOutcome {
    Added,
    ExactReplay,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct QueryBatchApply {
    pub outcome: QueryBatchApplyOutcome,
    pub view: Option<QueryView>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct QueryReceiptApply {
    pub outcome: QueryReceiptApplyOutcome,
    pub view: Option<QueryView>,
}

#[derive(Clone)]
struct ResultAccumulator {
    artifact: QueryArtifactRef,
    batch_commitments: BTreeSet<[u8; 32]>,
}

#[derive(Clone)]
pub struct QueryViewReducer {
    run_id: [u8; 32],
    max_batches: usize,
    max_results: usize,
    batches: BTreeMap<[u8; 32], QueryBatchEvidence>,
    receipts: BTreeMap<[u8; 32], QueryDoneReceipt>,
    results: BTreeMap<ArtifactKey, ResultAccumulator>,
    head: Option<(u64, [u8; 32])>,
}

impl QueryViewReducer {
    pub fn new(
        run_id: [u8; 32],
        max_batches: usize,
        max_results: usize,
    ) -> Result<Self, QueryViewError> {
        if run_id == [0; 32]
            || max_batches == 0
            || max_batches > MAX_QUERY_VIEW_BATCHES
            || max_results == 0
            || max_results > MAX_QUERY_VIEW_RESULTS
        {
            return Err(QueryViewError::InvalidCapacity);
        }
        Ok(Self {
            run_id,
            max_batches,
            max_results,
            batches: BTreeMap::new(),
            receipts: BTreeMap::new(),
            results: BTreeMap::new(),
            head: None,
        })
    }

    pub fn ingest_batch(
        &mut self,
        batch: QueryBatchEvidence,
    ) -> Result<QueryBatchApply, QueryViewError> {
        let mut next = self.clone();
        let applied = next.ingest_batch_in_place(batch)?;
        *self = next;
        Ok(applied)
    }

    fn ingest_batch_in_place(
        &mut self,
        mut batch: QueryBatchEvidence,
    ) -> Result<QueryBatchApply, QueryViewError> {
        batch.validate()?;
        if batch.run_id != self.run_id {
            return Err(QueryViewError::RunMismatch);
        }
        batch.results.sort_by_key(QueryArtifactRef::key);
        let batch_commitment = batch.batch_commitment()?;
        if let Some(existing) = self.batches.get(&batch_commitment) {
            return if existing == &batch {
                Ok(QueryBatchApply {
                    outcome: QueryBatchApplyOutcome::ExactReplay,
                    view: None,
                })
            } else {
                Err(QueryViewError::BatchCommitmentConflict)
            };
        }
        if self.batches.len() >= self.max_batches {
            return Err(QueryViewError::BatchCapacityReached);
        }

        let mut new_results = 0usize;
        for artifact in &batch.results {
            match self.results.get(&artifact.key()) {
                Some(existing) if existing.artifact != *artifact => {
                    return Err(QueryViewError::ArtifactMetadataConflict)
                }
                Some(_) => {}
                None => new_results += 1,
            }
        }
        if self.results.len().saturating_add(new_results) > self.max_results {
            return Err(QueryViewError::ResultCapacityReached);
        }
        let late = self
            .receipts
            .values()
            .any(|receipt| receipt.work_id == batch.work_id);
        for artifact in &batch.results {
            self.results
                .entry(artifact.key())
                .or_insert_with(|| ResultAccumulator {
                    artifact: artifact.clone(),
                    batch_commitments: BTreeSet::new(),
                })
                .batch_commitments
                .insert(batch_commitment);
        }
        self.batches.insert(batch_commitment, batch);

        let (outcome, reason) = match (late, new_results == 0) {
            (false, false) => (
                QueryBatchApplyOutcome::Added,
                QueryRevisionReason::BatchAdded,
            ),
            (true, false) => (
                QueryBatchApplyOutcome::LateAdded,
                QueryRevisionReason::LateBatchAdded,
            ),
            (false, true) => (
                QueryBatchApplyOutcome::ProvenanceOnlyAdded,
                QueryRevisionReason::ProvenanceOnlyBatchAdded,
            ),
            (true, true) => (
                QueryBatchApplyOutcome::LateProvenanceOnlyAdded,
                QueryRevisionReason::LateProvenanceOnlyBatchAdded,
            ),
        };
        Ok(QueryBatchApply {
            outcome,
            view: Some(self.materialize(reason)?),
        })
    }

    pub fn record_done(
        &mut self,
        receipt: QueryDoneReceipt,
    ) -> Result<QueryReceiptApply, QueryViewError> {
        let mut next = self.clone();
        let applied = next.record_done_in_place(receipt)?;
        *self = next;
        Ok(applied)
    }

    fn record_done_in_place(
        &mut self,
        receipt: QueryDoneReceipt,
    ) -> Result<QueryReceiptApply, QueryViewError> {
        if receipt.run_id != self.run_id {
            return Err(QueryViewError::RunMismatch);
        }
        let commitment = receipt.receipt_commitment()?;
        if let Some(existing) = self.receipts.get(&commitment) {
            return if existing == &receipt {
                Ok(QueryReceiptApply {
                    outcome: QueryReceiptApplyOutcome::ExactReplay,
                    view: None,
                })
            } else {
                Err(QueryViewError::ReceiptCommitmentConflict)
            };
        }
        if self.receipts.len() >= MAX_QUERY_DONE_RECEIPTS {
            return Err(QueryViewError::ReceiptCapacityReached);
        }
        self.receipts.insert(commitment, receipt);
        Ok(QueryReceiptApply {
            outcome: QueryReceiptApplyOutcome::Added,
            view: Some(self.materialize(QueryRevisionReason::DoneReceiptAdded)?),
        })
    }

    fn materialize(&mut self, reason: QueryRevisionReason) -> Result<QueryView, QueryViewError> {
        let (revision, parent_revision) = match self.head {
            Some((revision, root)) => (revision.saturating_add(1), Some(root)),
            None => (0, None),
        };
        let mut results = Vec::with_capacity(self.results.len());
        for accumulator in self.results.values() {
            results.push(QueryViewResult {
                artifact: accumulator.artifact.clone(),
                occurrence_root: digest_bytes_set(
                    b"query-result-occurrences",
                    accumulator.batch_commitments.iter().copied(),
                )?,
            });
        }
        let batch_set_root = digest_bytes_set(b"query-view-batches", self.batches.keys().copied())?;
        let receipt_set_root =
            digest_bytes_set(b"query-view-receipts", self.receipts.keys().copied())?;
        let coverage = self
            .batches
            .values()
            .map(|batch| batch.coverage.clone())
            .collect::<Vec<_>>();
        let coverage_root = digest_value(
            b"query-view-coverage",
            &ku_core::foundation::CanonicalValue::Array(
                coverage
                    .iter()
                    .map(CoverageStatement::canonical_value)
                    .collect::<Result<Vec<_>, _>>()?,
            ),
        )?;
        let status = if self.receipts.is_empty() {
            QueryViewStatus::Open
        } else {
            QueryViewStatus::WorkReceiptsObserved
        };
        let revision_cid = digest_value(
            b"query-view-revision",
            &query_view_value(QueryViewCommitmentInput {
                run_id: self.run_id,
                revision,
                parent: parent_revision,
                reason,
                status,
                results: &results,
                batch_root: batch_set_root,
                receipt_root: receipt_set_root,
                coverage_root,
            })?,
        )?;
        self.head = Some((revision, revision_cid));
        Ok(QueryView {
            run_id: self.run_id,
            revision,
            revision_cid,
            parent_revision,
            reason,
            status,
            results,
            batch_set_root,
            receipt_set_root,
            coverage_root,
            coverage,
        })
    }
}

struct QueryViewCommitmentInput<'a> {
    run_id: [u8; 32],
    revision: u64,
    parent: Option<[u8; 32]>,
    reason: QueryRevisionReason,
    status: QueryViewStatus,
    results: &'a [QueryViewResult],
    batch_root: [u8; 32],
    receipt_root: [u8; 32],
    coverage_root: [u8; 32],
}

fn query_view_value(
    input: QueryViewCommitmentInput<'_>,
) -> Result<ku_core::foundation::CanonicalValue, QueryViewError> {
    use ku_core::foundation::CanonicalValue;
    let mut fields = vec![
        (0, CanonicalValue::Unsigned(1)),
        (1, CanonicalValue::Bytes(input.run_id.to_vec())),
        (2, CanonicalValue::Unsigned(input.revision)),
        (
            4,
            CanonicalValue::Unsigned(revision_reason_tag(input.reason)),
        ),
        (
            5,
            CanonicalValue::Unsigned(match input.status {
                QueryViewStatus::Open => 0,
                QueryViewStatus::WorkReceiptsObserved => 1,
            }),
        ),
        (
            6,
            CanonicalValue::Array(
                input
                    .results
                    .iter()
                    .map(|result| {
                        CanonicalValue::Map(vec![
                            (0, result.artifact.to_value()),
                            (1, CanonicalValue::Bytes(result.occurrence_root.to_vec())),
                        ])
                    })
                    .collect(),
            ),
        ),
        (7, CanonicalValue::Bytes(input.batch_root.to_vec())),
        (8, CanonicalValue::Bytes(input.receipt_root.to_vec())),
        (9, CanonicalValue::Bytes(input.coverage_root.to_vec())),
    ];
    if let Some(parent) = input.parent {
        fields.push((3, CanonicalValue::Bytes(parent.to_vec())));
        fields.sort_by_key(|(key, _)| *key);
    }
    Ok(CanonicalValue::Map(fields))
}

fn revision_reason_tag(reason: QueryRevisionReason) -> u64 {
    match reason {
        QueryRevisionReason::BatchAdded => 0,
        QueryRevisionReason::LateBatchAdded => 1,
        QueryRevisionReason::ProvenanceOnlyBatchAdded => 2,
        QueryRevisionReason::LateProvenanceOnlyBatchAdded => 3,
        QueryRevisionReason::DoneReceiptAdded => 4,
    }
}

fn canonical_artifact_set(
    artifacts: &[QueryArtifactRef],
) -> Result<ku_core::foundation::CanonicalValue, QueryViewError> {
    let members = artifacts
        .iter()
        .map(QueryArtifactRef::to_value)
        .map(|value| (value.clone(), value))
        .collect();
    Ok(ku_core::foundation::CanonicalValue::Array(
        ku_core::foundation::canonicalize_set_by_key(members, ResourceProfile::ObjectV1)?,
    ))
}

fn digest_bytes_set(
    label: &[u8],
    values: impl IntoIterator<Item = [u8; 32]>,
) -> Result<[u8; 32], QueryViewError> {
    let values = values
        .into_iter()
        .map(|value| ku_core::foundation::CanonicalValue::Bytes(value.to_vec()))
        .collect::<Vec<_>>();
    digest_value(label, &ku_core::foundation::CanonicalValue::Array(values))
}

fn digest_value(
    label: &[u8],
    value: &ku_core::foundation::CanonicalValue,
) -> Result<[u8; 32], QueryViewError> {
    let bytes = encode_canonical(value, ResourceProfile::ObjectV1)?;
    let mut hasher = blake3::Hasher::new();
    hasher.update(b"onebrain:vnext:kql-query-view:1\0");
    hasher.update(label);
    hasher.update(&[0]);
    hasher.update(&bytes);
    Ok(*hasher.finalize().as_bytes())
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PresentedFeedback {
    NotApplicable,
    NoObservedResponse,
    Engaged,
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct LearnedExposureRecord {
    observation: ExposureObservation,
    feedback: PresentedFeedback,
    selection: Option<PrivateSelectionRecord>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct LearnedCandidateSignal {
    pub target: ObjectReference,
    pub weighted_presentations: ExactRatio,
    pub weighted_engagements: ExactRatio,
    pub validated_use_event_ids: Vec<EventCid>,
}

impl LearnedCandidateSignal {
    pub fn engagement_rate(&self) -> Result<Option<ExactRatio>, QueryViewError> {
        if self.weighted_presentations.numerator() == 0 {
            Ok(None)
        } else {
            Ok(Some(
                self.weighted_engagements
                    .checked_div(self.weighted_presentations)?,
            ))
        }
    }

    pub const fn is_eligibility_authority(&self) -> bool {
        false
    }

    pub const fn establishes_benefit(&self) -> bool {
        false
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LearnerRecordOutcome {
    AddedAttentionOnly,
    AddedPresented,
    ExactReplay,
    ConflictingObservationRejected,
    CapacityReached,
}

#[derive(Clone)]
pub struct PropensityAwareLearner {
    max_observations: usize,
    observations: BTreeMap<[u8; 32], LearnedExposureRecord>,
    signals: BTreeMap<(u64, [u8; 32]), LearnedCandidateSignal>,
    use_events: BTreeSet<[u8; 32]>,
}

impl PropensityAwareLearner {
    pub fn new(max_observations: usize) -> Result<Self, QueryViewError> {
        if max_observations == 0 || max_observations > MAX_LEARNING_OBSERVATIONS {
            return Err(QueryViewError::InvalidCapacity);
        }
        Ok(Self {
            max_observations,
            observations: BTreeMap::new(),
            signals: BTreeMap::new(),
            use_events: BTreeSet::new(),
        })
    }

    pub fn record_exposure(
        &mut self,
        observation: ExposureObservation,
        feedback: PresentedFeedback,
        selection: Option<&PrivateSelectionRecord>,
    ) -> Result<LearnerRecordOutcome, QueryViewError> {
        let mut next = self.clone();
        let outcome = next.record_exposure_in_place(observation, feedback, selection)?;
        *self = next;
        Ok(outcome)
    }

    fn record_exposure_in_place(
        &mut self,
        observation: ExposureObservation,
        feedback: PresentedFeedback,
        selection: Option<&PrivateSelectionRecord>,
    ) -> Result<LearnerRecordOutcome, QueryViewError> {
        if observation.observation_id == [0; 32]
            || observation.target.cid == [0; 32]
            || observation.private_context_commitment == [0; 32]
        {
            return Err(QueryViewError::InvalidExposure);
        }
        match observation.kind {
            ExposureKind::QueryHit | ExposureKind::Retrieval => {
                if feedback != PresentedFeedback::NotApplicable || selection.is_some() {
                    return Err(QueryViewError::AttentionIsNotPresentation);
                }
            }
            ExposureKind::Presented => {
                let selection = selection.ok_or(QueryViewError::MissingPropensity)?;
                if feedback == PresentedFeedback::NotApplicable
                    || selection.candidate_id != observation.target.cid
                {
                    return Err(QueryViewError::ExposureSelectionMismatch);
                }
            }
        }
        let record = LearnedExposureRecord {
            observation: observation.clone(),
            feedback,
            selection: selection.cloned(),
        };
        if let Some(existing) = self.observations.get(&observation.observation_id) {
            return Ok(if existing == &record {
                LearnerRecordOutcome::ExactReplay
            } else {
                LearnerRecordOutcome::ConflictingObservationRejected
            });
        }
        if self.observations.len() >= self.max_observations {
            return Ok(LearnerRecordOutcome::CapacityReached);
        }
        self.observations.insert(observation.observation_id, record);
        if observation.kind != ExposureKind::Presented {
            return Ok(LearnerRecordOutcome::AddedAttentionOnly);
        }

        let selection = selection.expect("presented exposure checked above");
        let inverse = inverse_propensity(selection.propensity)?;
        let key = (observation.target.reference_kind, observation.target.cid);
        let signal = self
            .signals
            .entry(key)
            .or_insert_with(|| LearnedCandidateSignal {
                target: observation.target,
                weighted_presentations: ExactRatio::integer(0),
                weighted_engagements: ExactRatio::integer(0),
                validated_use_event_ids: Vec::new(),
            });
        signal.weighted_presentations = signal.weighted_presentations.checked_add(inverse)?;
        if feedback == PresentedFeedback::Engaged {
            signal.weighted_engagements = signal.weighted_engagements.checked_add(inverse)?;
        }
        Ok(LearnerRecordOutcome::AddedPresented)
    }

    /// Use enters only through a cryptographically validated UseEvent and is
    /// retained separately from exposure-weighted attention.
    pub fn record_validated_use(
        &mut self,
        event: &ValidatedUseEvidenceEvent,
    ) -> Result<bool, QueryViewError> {
        let event_id = event.event_cid().into_bytes();
        if !self.use_events.insert(event_id) {
            return Ok(false);
        }
        for subject in &event.payload().subjects {
            let signal = self
                .signals
                .entry((subject.reference_kind, subject.cid))
                .or_insert_with(|| LearnedCandidateSignal {
                    target: subject.clone(),
                    weighted_presentations: ExactRatio::integer(0),
                    weighted_engagements: ExactRatio::integer(0),
                    validated_use_event_ids: Vec::new(),
                });
            signal.validated_use_event_ids.push(event.event_cid());
            signal
                .validated_use_event_ids
                .sort_by_key(|event| *event.as_bytes());
        }
        Ok(true)
    }

    pub fn signal_for(&self, target: &ObjectReference) -> Option<&LearnedCandidateSignal> {
        self.signals.get(&(target.reference_kind, target.cid))
    }

    pub const fn is_local_private(&self) -> bool {
        true
    }

    pub const fn can_change_eligibility(&self) -> bool {
        false
    }
}

fn inverse_propensity(propensity: SelectionPropensity) -> Result<ExactRatio, QueryViewError> {
    if propensity.numerator == 0
        || propensity.denominator == 0
        || propensity.numerator > propensity.denominator
    {
        return Err(QueryViewError::InvalidPropensity);
    }
    let numerator =
        i64::try_from(propensity.denominator).map_err(|_| QueryViewError::InvalidPropensity)?;
    Ok(ExactRatio::new(numerator, propensity.numerator)?)
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum QueryViewError {
    Canonical(ku_core::foundation::CanonicalError),
    Inventory(InventoryError),
    Semantic(SemanticError),
    InvalidCapacity,
    InvalidBatch,
    InvalidReceipt,
    RunMismatch,
    DuplicateBatchResult,
    BatchCommitmentConflict,
    ReceiptCommitmentConflict,
    ArtifactMetadataConflict,
    BatchCapacityReached,
    ReceiptCapacityReached,
    ResultCapacityReached,
    InvalidExposure,
    AttentionIsNotPresentation,
    MissingPropensity,
    ExposureSelectionMismatch,
    InvalidPropensity,
}

impl From<ku_core::foundation::CanonicalError> for QueryViewError {
    fn from(error: ku_core::foundation::CanonicalError) -> Self {
        Self::Canonical(error)
    }
}

impl From<InventoryError> for QueryViewError {
    fn from(error: InventoryError) -> Self {
        Self::Inventory(error)
    }
}

impl From<SemanticError> for QueryViewError {
    fn from(error: SemanticError) -> Self {
        Self::Semantic(error)
    }
}

#[cfg(test)]
mod tests {
    use ed25519_dalek::SigningKey;
    use ku_core::foundation::{
        decode_feed_inception, decode_knowledge_event, decode_knowledge_object, ConceptCcid,
        CoverageBasis, CoverageLimitation, CoverageStatus, DeviceId, DisclosureClass,
        FeedInception, KnowledgeEventEnvelope, KnownObjectKind, NamespaceCommitment,
        SignedFeedInception, UseEvidencePayload, UseMode, USE_EVIDENCE_EVENT_TYPE,
        USE_EVIDENCE_KIND,
    };

    use super::*;
    use crate::vnext_exploration::{SelectedLane, SelectionReason};

    fn coverage(byte: u8) -> CoverageStatement {
        CoverageStatement {
            selector: ku_core::foundation::SelectorCid::from_bytes([byte; 32]),
            assessed_frontier: vec![EventCid::from_bytes([byte + 1; 32])],
            basis: CoverageBasis::Sampled,
            status: CoverageStatus::Partial,
            returned_records: 1,
            returned_bytes: 32,
            continuation: Some([byte + 2; 32]),
            limitations: vec![CoverageLimitation::FrontierIncomplete],
        }
    }

    fn batch(run: u8, work: u8, result: u8, coverage_id: u8) -> QueryBatchEvidence {
        QueryBatchEvidence {
            run_id: [run; 32],
            work_id: [work; 32],
            results: vec![QueryArtifactRef::Object(ObjectReference::new(
                2,
                [result; 32],
            ))],
            coverage: coverage(coverage_id),
        }
    }

    fn selection(candidate: u8, numerator: u64, denominator: u64) -> PrivateSelectionRecord {
        PrivateSelectionRecord {
            selection_ordinal: 1,
            candidate_id: [candidate; 32],
            lane: SelectedLane::Exploit,
            reason: SelectionReason::RandomExploit,
            propensity: SelectionPropensity {
                numerator,
                denominator,
            },
            policy_cid: ku_core::foundation::ObjectCid::from_bytes([9; 32]),
            frontier_digest: [8; 32],
            rng_counter_start: 0,
        }
    }

    fn exposure(id: u8, target: u8, kind: ExposureKind) -> ExposureObservation {
        ExposureObservation {
            observation_id: [id; 32],
            target: ObjectReference::new(2, [target; 32]),
            kind,
            local_sequence: u64::from(id),
            private_context_commitment: [7; 32],
        }
    }

    #[test]
    fn late_batch_after_done_creates_child_revision() {
        let mut reducer = QueryViewReducer::new([1; 32], 10, 10).unwrap();
        let done = reducer
            .record_done(QueryDoneReceipt {
                run_id: [1; 32],
                work_id: [2; 32],
                assessed_frontier: [3; 32],
                continuation: None,
            })
            .unwrap()
            .view
            .unwrap();
        let late = reducer.ingest_batch(batch(1, 2, 4, 10)).unwrap();
        let view = late.view.unwrap();
        assert_eq!(late.outcome, QueryBatchApplyOutcome::LateAdded);
        assert_eq!(view.revision, 1);
        assert_eq!(view.parent_revision, Some(done.revision_cid));
        assert_eq!(view.results.len(), 1);
        assert!(!view.is_globally_complete());
    }

    #[test]
    fn canonical_cid_dedup_and_replays_never_boost_rank() {
        let mut reducer = QueryViewReducer::new([1; 32], 128, 10).unwrap();
        let first_batch = batch(1, 2, 9, 10);
        let first = reducer.ingest_batch(first_batch.clone()).unwrap();
        assert_eq!(first.outcome, QueryBatchApplyOutcome::Added);
        assert_eq!(
            reducer.ingest_batch(first_batch).unwrap().outcome,
            QueryBatchApplyOutcome::ExactReplay
        );
        let first_root = first.view.unwrap().results[0].occurrence_root;
        let mut last = None;
        for work in 3..=102u8 {
            last = reducer.ingest_batch(batch(1, work, 9, work)).unwrap().view;
        }
        let view = last.unwrap();
        assert_eq!(view.results.len(), 1);
        assert_ne!(view.results[0].occurrence_root, first_root);
        assert!(!view.results[0].source_count_affects_rank());
    }

    #[test]
    fn same_object_cid_with_conflicting_kind_metadata_is_rejected_atomically() {
        let mut reducer = QueryViewReducer::new([1; 32], 10, 10).unwrap();
        reducer.ingest_batch(batch(1, 2, 9, 10)).unwrap();
        let mut conflicting = batch(1, 3, 9, 11);
        conflicting.results[0] = QueryArtifactRef::Object(ObjectReference::new(99, [9; 32]));
        assert_eq!(
            reducer.ingest_batch(conflicting).unwrap_err(),
            QueryViewError::ArtifactMetadataConflict
        );
        let replay = reducer.ingest_batch(batch(1, 2, 9, 10)).unwrap();
        assert_eq!(replay.outcome, QueryBatchApplyOutcome::ExactReplay);
    }

    #[test]
    fn query_hit_and_retrieval_are_attention_not_negative_or_use() {
        let mut learner = PropensityAwareLearner::new(10).unwrap();
        for (id, kind) in [(1, ExposureKind::QueryHit), (2, ExposureKind::Retrieval)] {
            assert_eq!(
                learner
                    .record_exposure(
                        exposure(id, 5, kind),
                        PresentedFeedback::NotApplicable,
                        None
                    )
                    .unwrap(),
                LearnerRecordOutcome::AddedAttentionOnly
            );
        }
        assert!(learner
            .signal_for(&ObjectReference::new(2, [5; 32]))
            .is_none());
        assert!(learner.is_local_private());
        assert!(!learner.can_change_eligibility());
    }

    #[test]
    fn presented_feedback_is_inverse_propensity_weighted_and_replay_safe() {
        let mut learner = PropensityAwareLearner::new(10).unwrap();
        let first = exposure(1, 5, ExposureKind::Presented);
        let half = selection(5, 1, 2);
        learner
            .record_exposure(
                first.clone(),
                PresentedFeedback::NoObservedResponse,
                Some(&half),
            )
            .unwrap();
        assert_eq!(
            learner
                .record_exposure(first, PresentedFeedback::NoObservedResponse, Some(&half),)
                .unwrap(),
            LearnerRecordOutcome::ExactReplay
        );
        learner
            .record_exposure(
                exposure(2, 5, ExposureKind::Presented),
                PresentedFeedback::Engaged,
                Some(&selection(5, 1, 4)),
            )
            .unwrap();
        let signal = learner
            .signal_for(&ObjectReference::new(2, [5; 32]))
            .unwrap();
        assert_eq!(signal.weighted_presentations, ExactRatio::integer(6));
        assert_eq!(signal.weighted_engagements, ExactRatio::integer(4));
        assert_eq!(
            signal.engagement_rate().unwrap(),
            Some(ExactRatio::new(2, 3).unwrap())
        );
        assert!(signal.validated_use_event_ids.is_empty());
    }

    #[test]
    fn invalid_propensity_does_not_consume_observation_id() {
        let mut learner = PropensityAwareLearner::new(10).unwrap();
        let observation = exposure(1, 5, ExposureKind::Presented);
        assert_eq!(
            learner
                .record_exposure(
                    observation.clone(),
                    PresentedFeedback::Engaged,
                    Some(&selection(5, 2, 1)),
                )
                .unwrap_err(),
            QueryViewError::InvalidPropensity
        );
        assert_eq!(
            learner
                .record_exposure(
                    observation,
                    PresentedFeedback::Engaged,
                    Some(&selection(5, 1, 2)),
                )
                .unwrap(),
            LearnerRecordOutcome::AddedPresented
        );
    }

    fn validated_use_event() -> ValidatedUseEvidenceEvent {
        let key = SigningKey::from_bytes(&[7; 32]);
        let inception = FeedInception::new(
            *key.verifying_key().as_bytes(),
            NamespaceCommitment::derive(b"query-view-use-test", [8; 32]).unwrap(),
            0,
            DeviceId::from_bytes([9; 32]),
        );
        let signed: SignedFeedInception = inception.sign(&key).unwrap();
        let author = decode_feed_inception(&signed.encode().unwrap()).unwrap();
        let payload = UseEvidencePayload {
            subjects: vec![ObjectReference::new(2, [5; 32])],
            mode: UseMode::ComparedOrOpposed,
            actor_class: ConceptCcid::from_bytes([2; 16]),
            task_context_commitment: [3; 32],
            causal_role: ConceptCcid::from_bytes([4; 16]),
            assembly: None,
            mapping: None,
            outcome_observation: None,
            use_policy: ObjectReference::new(0, [7; 32]),
            observed_frontier: [8; 32],
        };
        let object = payload
            .to_knowledge_object(DisclosureClass::LocalOnly)
            .unwrap();
        let (object_bytes, object_cid) = object.encode(ResourceProfile::ObjectV1).unwrap();
        let object = decode_knowledge_object(
            &object_bytes,
            ResourceProfile::ObjectV1,
            &[KnownObjectKind::new(USE_EVIDENCE_KIND, 1)],
            &[],
        )
        .unwrap();
        let mut event = KnowledgeEventEnvelope::new(
            USE_EVIDENCE_EVENT_TYPE,
            author.feed_id,
            0,
            DisclosureClass::LocalOnly,
            [30; 32],
        );
        event.payload_refs = vec![ObjectReference::new(0, object_cid.into_bytes())];
        let event_bytes = event.sign(&author, &key).unwrap().encode().unwrap().0;
        let event =
            decode_knowledge_event(&event_bytes, &author, &[USE_EVIDENCE_EVENT_TYPE]).unwrap();
        ValidatedUseEvidenceEvent::bind(&event, &object).unwrap()
    }

    #[test]
    fn validated_use_is_separate_and_event_cid_deduplicated() {
        let mut learner = PropensityAwareLearner::new(10).unwrap();
        let event = validated_use_event();
        assert!(learner.record_validated_use(&event).unwrap());
        assert!(!learner.record_validated_use(&event).unwrap());
        let signal = learner
            .signal_for(&ObjectReference::new(2, [5; 32]))
            .unwrap();
        assert_eq!(signal.validated_use_event_ids, vec![event.event_cid()]);
        assert_eq!(signal.weighted_presentations, ExactRatio::integer(0));
        assert!(!signal.establishes_benefit());
    }
}

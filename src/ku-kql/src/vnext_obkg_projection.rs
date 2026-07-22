//! Disposable OBKG projections over vNext canonical sources.
//!
//! This module consumes already validated objects, the KQL-005 minimal view
//! sources, candidate proposals and signed causal exercise evidence. It never
//! reduces resolution events itself and owns no canonical knowledge state.

use std::collections::{BTreeMap, BTreeSet};

use ku_core::foundation::resolution::RESOLUTION_REDUCER_VERSION;
use ku_core::foundation::{
    encode_canonical, AssessedExerciseEvidence, DisclosureClass, EventCid, ExerciseAuthority,
    ExerciseEvidence, FeedId, MappingKernelCid, ObjectCid, ObjectReference, ObjectSemantics,
    ResourceProfile, SelectorCid, ValidatedKnowledgeObject, KNOWLEDGE_AFFORDANCE_KIND,
    RECEPTOR_DEFINITION_KIND,
};

use crate::vnext_proposal::{BindingProposal, ProposalDisposition, ProposalError, ProposalId};
use crate::vnext_standing_need::{
    MappingViewRecord, MinimalKnowledgeViews, ReceptorResolutionProjection, StandingNeed,
    StandingNeedError, StandingNeedId,
};

pub const OBKG_PROJECTION_REDUCER_VERSION: u64 = 1;
pub const OBKG_PROJECTION_INDEX_VERSION: u64 = 1;
pub const MAX_OBKG_PROJECTION_SOURCES: usize = 1_000_000;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ObkgSourceFrontier {
    pub selector: SelectorCid,
    pub authority_frontier: [u8; 32],
    pub accepted_feed_positions: BTreeMap<FeedId, u64>,
}

impl ObkgSourceFrontier {
    pub fn new(
        selector: SelectorCid,
        authority_frontier: [u8; 32],
        positions: impl IntoIterator<Item = (FeedId, u64)>,
    ) -> Result<Self, ObkgProjectionError> {
        if selector.as_bytes() == &[0; 32] || authority_frontier == [0; 32] {
            return Err(ObkgProjectionError::InvalidFrontier);
        }
        let mut accepted_feed_positions = BTreeMap::new();
        for (feed, sequence) in positions {
            if feed.as_bytes() == &[0; 32]
                || accepted_feed_positions.insert(feed, sequence).is_some()
            {
                return Err(ObkgProjectionError::InvalidFrontier);
            }
        }
        if accepted_feed_positions.len() > MAX_OBKG_PROJECTION_SOURCES {
            return Err(ObkgProjectionError::Limit);
        }
        Ok(Self {
            selector,
            authority_frontier,
            accepted_feed_positions,
        })
    }

    pub fn root(&self) -> [u8; 32] {
        let mut hasher = projection_hasher(b"source-frontier");
        hasher.update(self.selector.as_bytes());
        hasher.update(&self.authority_frontier);
        for (feed, sequence) in &self.accepted_feed_positions {
            hasher.update(feed.as_bytes());
            hasher.update(&sequence.to_be_bytes());
        }
        *hasher.finalize().as_bytes()
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ObkgModelVersion {
    DeterministicNoModel,
    Learned(ObjectReference),
}

impl ObkgModelVersion {
    fn validate(&self) -> Result<(), ObkgProjectionError> {
        match self {
            Self::DeterministicNoModel => Ok(()),
            Self::Learned(reference) if reference.cid != [0; 32] => Ok(()),
            Self::Learned(_) => Err(ObkgProjectionError::InvalidModelVersion),
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ObkgProjectionConfig {
    pub source_frontier: ObkgSourceFrontier,
    pub model_version: ObkgModelVersion,
    pub current_evaluation: u64,
}

pub struct ObkgProjectionSources<'a> {
    pub objects: &'a [ValidatedKnowledgeObject],
    pub standing_needs: &'a [StandingNeed],
    pub resolutions: &'a [ReceptorResolutionProjection],
    pub materialized_mappings: &'a [MappingViewRecord],
    pub candidate_proposals: &'a [BindingProposal],
    pub exercises: &'a [AssessedExerciseEvidence],
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ProjectedKnowledgeObject {
    pub object: ObjectReference,
    pub disclosure: DisclosureClass,
    pub schema_major: u64,
    pub schema_minor: u64,
    pub references: Vec<ObjectReference>,
    pub payload_commitment: [u8; 32],
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ReceptorProjectionRecord {
    pub object: ProjectedKnowledgeObject,
    pub standing_needs: Vec<StandingNeedId>,
    pub resolutions: Vec<ProjectedResolution>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub struct ProjectedResolutionTarget {
    pub assembly_lineage: [u8; 32],
    pub assembly_revision: [u8; 32],
    pub placement: [u8; 32],
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub struct ProjectedResolution {
    pub target: ProjectedResolutionTarget,
    pub state: u8,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct MappingProjectionRecord {
    pub kernel: MappingKernelCid,
    pub materialized: Option<MappingViewRecord>,
    pub candidate_proposals: Vec<(ProposalId, ProposalDisposition)>,
    pub adopted_targets: Vec<ProjectedResolutionTarget>,
}

impl MappingProjectionRecord {
    pub fn is_active_edge(&self) -> bool {
        self.materialized.is_some() && !self.adopted_targets.is_empty()
    }

    pub fn is_candidate_only(&self) -> bool {
        self.materialized.is_none()
            && self.adopted_targets.is_empty()
            && !self.candidate_proposals.is_empty()
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ExerciseProjectionKind {
    Use,
    Derivation,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ExerciseProjectionRecord {
    pub event: EventCid,
    pub kind: ExerciseProjectionKind,
    pub authority: ExerciseAuthority,
    pub payload_object: ObjectCid,
    pub inputs: Vec<ObjectReference>,
    pub outputs: Vec<ObjectReference>,
    pub mapping: Option<MappingKernelCid>,
    pub observed_frontier: [u8; 32],
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ObkgProjectionSnapshot {
    pub source_frontier_root: [u8; 32],
    pub source_root: [u8; 32],
    pub projection_root: [u8; 32],
    pub reducer_version: u64,
    pub resolution_reducer_version: u64,
    pub index_version: u64,
    pub model_version: ObkgModelVersion,
}

#[derive(Clone, Debug)]
pub struct ObkgProjection {
    receptors: BTreeMap<(u64, [u8; 32]), ReceptorProjectionRecord>,
    affordances: BTreeMap<(u64, [u8; 32]), ProjectedKnowledgeObject>,
    mappings: BTreeMap<[u8; 32], MappingProjectionRecord>,
    exercises: BTreeMap<[u8; 32], ExerciseProjectionRecord>,
    exercise_by_object: BTreeMap<(u64, [u8; 32]), BTreeSet<[u8; 32]>>,
    snapshot: ObkgProjectionSnapshot,
}

impl ObkgProjection {
    pub fn rebuild(
        config: ObkgProjectionConfig,
        sources: ObkgProjectionSources<'_>,
    ) -> Result<Self, ObkgProjectionError> {
        config.model_version.validate()?;
        enforce_limits(&sources)?;
        for resolution in sources.resolutions {
            if resolution.view.reducer_version != RESOLUTION_REDUCER_VERSION {
                return Err(ObkgProjectionError::ResolutionReducerVersionMismatch);
            }
            if resolution.view.assessed_frontier == [0; 32] {
                return Err(ObkgProjectionError::InvalidFrontier);
            }
        }

        // This call is the KQL-005 projection boundary. OBKG consumes its
        // output and already-reduced ResolutionViews; it never replays actions.
        let minimal = MinimalKnowledgeViews::rebuild(
            sources.standing_needs,
            sources.resolutions,
            sources.materialized_mappings,
        )?;
        let minimal_snapshot = minimal
            .snapshot()
            .ok_or(ObkgProjectionError::MissingMinimalSnapshot)?;

        let mut receptors = BTreeMap::new();
        let mut affordances = BTreeMap::new();
        for object in sources.objects {
            let projected = project_object(object)?;
            let key = reference_key(&projected.object);
            match projected.object.reference_kind {
                kind if kind == RECEPTOR_DEFINITION_KIND.0 => {
                    let mut resolutions = minimal
                        .resolutions_for(&projected.object)
                        .into_iter()
                        .map(|(target, state)| ProjectedResolution {
                            target: target.into(),
                            state: resolution_state_code(state),
                        })
                        .collect::<Vec<_>>();
                    resolutions.sort_unstable();
                    let record = ReceptorProjectionRecord {
                        standing_needs: minimal.standing_needs_for(&projected.object),
                        resolutions,
                        object: projected,
                    };
                    insert_exact(&mut receptors, key, record)?;
                }
                kind if kind == KNOWLEDGE_AFFORDANCE_KIND.0 => {
                    insert_exact(&mut affordances, key, projected)?;
                }
                _ => return Err(ObkgProjectionError::UnsupportedObjectKind),
            }
        }

        let mut mappings = BTreeMap::<[u8; 32], MappingProjectionRecord>::new();
        for mapping in sources.materialized_mappings {
            let key = mapping.kernel.into_bytes();
            let record = mappings
                .entry(key)
                .or_insert_with(|| MappingProjectionRecord {
                    kernel: mapping.kernel,
                    materialized: None,
                    candidate_proposals: Vec::new(),
                    adopted_targets: Vec::new(),
                });
            match &record.materialized {
                Some(existing) if existing != mapping => {
                    return Err(ObkgProjectionError::ConflictingSource)
                }
                Some(_) => {}
                None => record.materialized = Some(mapping.clone()),
            }
        }

        for proposal in sources.candidate_proposals {
            proposal.validate()?;
            let kernel = proposal.kernel_id()?;
            let record =
                mappings
                    .entry(kernel.into_bytes())
                    .or_insert_with(|| MappingProjectionRecord {
                        kernel,
                        materialized: None,
                        candidate_proposals: Vec::new(),
                        adopted_targets: Vec::new(),
                    });
            record.candidate_proposals.push((
                proposal.proposal_id()?,
                proposal.disposition(config.current_evaluation),
            ));
        }

        for resolution in sources.resolutions {
            let target = ProjectedResolutionTarget::from(resolution.view.target);
            for branch in &resolution.view.branches {
                for kernel in &branch.active_bindings {
                    let Some(record) = mappings.get_mut(kernel.as_bytes()) else {
                        return Err(ObkgProjectionError::AdoptionWithoutMaterializedMapping);
                    };
                    if record.materialized.is_none() {
                        return Err(ObkgProjectionError::AdoptionWithoutMaterializedMapping);
                    }
                    record.adopted_targets.push(target);
                }
            }
        }
        for record in mappings.values_mut() {
            record.candidate_proposals.sort_by_key(|(id, disposition)| {
                (*id.as_bytes(), proposal_disposition_code(*disposition))
            });
            record.candidate_proposals.dedup();
            record.adopted_targets.sort_unstable();
            record.adopted_targets.dedup();
        }

        let mut exercises = BTreeMap::new();
        let mut exercise_by_object = BTreeMap::<_, BTreeSet<_>>::new();
        for assessed in sources.exercises {
            let record = project_exercise(assessed);
            let event_key = record.event.into_bytes();
            for object in record.inputs.iter().chain(&record.outputs) {
                exercise_by_object
                    .entry(reference_key(object))
                    .or_default()
                    .insert(event_key);
            }
            insert_exact(&mut exercises, event_key, record)?;
        }

        let source_frontier_root = config.source_frontier.root();
        let source_root = source_root(
            &config,
            sources,
            minimal_snapshot.source_root,
            minimal_snapshot.projection_root,
        )?;
        let projection_root =
            projection_root(&config, &receptors, &affordances, &mappings, &exercises);
        Ok(Self {
            receptors,
            affordances,
            mappings,
            exercises,
            exercise_by_object,
            snapshot: ObkgProjectionSnapshot {
                source_frontier_root,
                source_root,
                projection_root,
                reducer_version: OBKG_PROJECTION_REDUCER_VERSION,
                resolution_reducer_version: RESOLUTION_REDUCER_VERSION,
                index_version: OBKG_PROJECTION_INDEX_VERSION,
                model_version: config.model_version,
            },
        })
    }

    pub const fn snapshot(&self) -> &ObkgProjectionSnapshot {
        &self.snapshot
    }

    pub fn receptor(&self, reference: &ObjectReference) -> Option<&ReceptorProjectionRecord> {
        self.receptors.get(&reference_key(reference))
    }

    pub fn affordance(&self, reference: &ObjectReference) -> Option<&ProjectedKnowledgeObject> {
        self.affordances.get(&reference_key(reference))
    }

    pub fn mapping(&self, kernel: MappingKernelCid) -> Option<&MappingProjectionRecord> {
        self.mappings.get(kernel.as_bytes())
    }

    pub fn exercise(&self, event: EventCid) -> Option<&ExerciseProjectionRecord> {
        self.exercises.get(event.as_bytes())
    }

    pub fn authorized_exercise_events_for(&self, object: &ObjectReference) -> Vec<EventCid> {
        self.exercise_by_object
            .get(&reference_key(object))
            .into_iter()
            .flatten()
            .filter_map(|event| {
                (self.exercises[event].authority == ExerciseAuthority::Authorized)
                    .then_some(EventCid::from_bytes(*event))
            })
            .collect()
    }

    pub fn active_mapping_edge_count(&self) -> usize {
        self.mappings
            .values()
            .map(|record| {
                if record.materialized.is_some() {
                    record.adopted_targets.len()
                } else {
                    0
                }
            })
            .sum()
    }

    pub const fn accepts_exposure_telemetry(&self) -> bool {
        false
    }

    pub const fn is_source_of_record(&self) -> bool {
        false
    }

    pub const fn is_resolution_reducer(&self) -> bool {
        false
    }
}

impl From<ku_core::foundation::ResolutionTarget> for ProjectedResolutionTarget {
    fn from(target: ku_core::foundation::ResolutionTarget) -> Self {
        Self {
            assembly_lineage: *target.assembly_lineage.as_bytes(),
            assembly_revision: target.assembly_revision.into_bytes(),
            placement: *target.placement.as_bytes(),
        }
    }
}

fn enforce_limits(sources: &ObkgProjectionSources<'_>) -> Result<(), ObkgProjectionError> {
    if [
        sources.objects.len(),
        sources.standing_needs.len(),
        sources.resolutions.len(),
        sources.materialized_mappings.len(),
        sources.candidate_proposals.len(),
        sources.exercises.len(),
    ]
    .into_iter()
    .any(|len| len > MAX_OBKG_PROJECTION_SOURCES)
    {
        Err(ObkgProjectionError::Limit)
    } else {
        Ok(())
    }
}

fn project_object(
    object: &ValidatedKnowledgeObject,
) -> Result<ProjectedKnowledgeObject, ObkgProjectionError> {
    let ObjectSemantics::Known(envelope) = object.semantics() else {
        return Err(ObkgProjectionError::OpaqueObject);
    };
    if !matches!(
        envelope.kind,
        kind if kind == RECEPTOR_DEFINITION_KIND || kind == KNOWLEDGE_AFFORDANCE_KIND
    ) {
        return Err(ObkgProjectionError::UnsupportedObjectKind);
    }
    let payload = encode_canonical(&envelope.payload, ResourceProfile::ObjectV1)?;
    let mut references = envelope.references.clone();
    references.sort_by_key(reference_key);
    references.dedup_by_key(|reference| reference_key(reference));
    Ok(ProjectedKnowledgeObject {
        object: ObjectReference::new(envelope.kind.0, object.cid().into_bytes()),
        disclosure: envelope.disclosure,
        schema_major: envelope.kind_version.major,
        schema_minor: envelope.kind_version.minor,
        references,
        payload_commitment: digest_bytes(b"object-payload", &payload),
    })
}

fn project_exercise(assessed: &AssessedExerciseEvidence) -> ExerciseProjectionRecord {
    let (kind, payload_object, mut inputs, mut outputs, mapping, observed_frontier) =
        match &assessed.evidence {
            ExerciseEvidence::Use(event) => (
                ExerciseProjectionKind::Use,
                event.payload_object_cid(),
                event.payload().subjects.clone(),
                Vec::new(),
                event.payload().mapping,
                event.payload().observed_frontier,
            ),
            ExerciseEvidence::Derivation(event) => (
                ExerciseProjectionKind::Derivation,
                event.payload_object_cid(),
                event
                    .payload()
                    .inputs
                    .iter()
                    .map(|input| input.input.clone())
                    .collect(),
                vec![event.payload().output.clone()],
                None,
                event.payload().observed_frontier,
            ),
        };
    inputs.sort_by_key(reference_key);
    inputs.dedup_by_key(|reference| reference_key(reference));
    outputs.sort_by_key(reference_key);
    outputs.dedup_by_key(|reference| reference_key(reference));
    ExerciseProjectionRecord {
        event: assessed.evidence.event_cid(),
        kind,
        authority: assessed.authority,
        payload_object,
        inputs,
        outputs,
        mapping,
        observed_frontier,
    }
}

fn source_root(
    config: &ObkgProjectionConfig,
    sources: ObkgProjectionSources<'_>,
    minimal_source_root: [u8; 32],
    minimal_projection_root: [u8; 32],
) -> Result<[u8; 32], ObkgProjectionError> {
    let mut hasher = projection_hasher(b"sources");
    hasher.update(&config.source_frontier.root());
    hasher.update(&minimal_source_root);
    hasher.update(&minimal_projection_root);
    hash_model_version(&mut hasher, &config.model_version);

    let mut objects = sources
        .objects
        .iter()
        .map(|object| object.cid().into_bytes())
        .collect::<Vec<_>>();
    objects.sort_unstable();
    objects.dedup();
    for object in objects {
        hasher.update(&object);
    }

    let mut resolutions = sources.resolutions.iter().collect::<Vec<_>>();
    resolutions.sort_by_key(|resolution| {
        (
            *resolution.view.target.assembly_lineage.as_bytes(),
            resolution.view.target.assembly_revision.into_bytes(),
            *resolution.view.target.placement.as_bytes(),
            reference_key(&resolution.receptor_definition),
        )
    });
    for resolution in resolutions {
        hash_resolution_source(&mut hasher, resolution);
    }

    let mut proposals = sources
        .candidate_proposals
        .iter()
        .map(BindingProposal::proposal_id)
        .collect::<Result<Vec<_>, _>>()?;
    proposals.sort_by_key(|id| *id.as_bytes());
    proposals.dedup();
    for proposal in proposals {
        hasher.update(proposal.as_bytes());
    }

    let mut exercises = sources.exercises.iter().collect::<Vec<_>>();
    exercises.sort_by_key(|record| record.evidence.event_cid().into_bytes());
    for exercise in exercises {
        hasher.update(exercise.evidence.event_cid().as_bytes());
        hasher.update(&[authority_code(exercise.authority)]);
    }
    Ok(*hasher.finalize().as_bytes())
}

fn projection_root(
    config: &ObkgProjectionConfig,
    receptors: &BTreeMap<(u64, [u8; 32]), ReceptorProjectionRecord>,
    affordances: &BTreeMap<(u64, [u8; 32]), ProjectedKnowledgeObject>,
    mappings: &BTreeMap<[u8; 32], MappingProjectionRecord>,
    exercises: &BTreeMap<[u8; 32], ExerciseProjectionRecord>,
) -> [u8; 32] {
    let mut hasher = projection_hasher(b"projection");
    hasher.update(&OBKG_PROJECTION_REDUCER_VERSION.to_be_bytes());
    hasher.update(&RESOLUTION_REDUCER_VERSION.to_be_bytes());
    hasher.update(&OBKG_PROJECTION_INDEX_VERSION.to_be_bytes());
    hash_model_version(&mut hasher, &config.model_version);
    for record in receptors.values() {
        hash_projected_object(&mut hasher, &record.object);
        for need in &record.standing_needs {
            hasher.update(need.as_bytes());
        }
        for resolution in &record.resolutions {
            hash_projected_target(&mut hasher, resolution.target);
            hasher.update(&[resolution.state]);
        }
    }
    for record in affordances.values() {
        hash_projected_object(&mut hasher, record);
    }
    for record in mappings.values() {
        hasher.update(record.kernel.as_bytes());
        if let Some(materialized) = &record.materialized {
            hasher.update(&[1]);
            hasher.update(materialized.envelope.as_bytes());
            hasher.update(&[materialized.disclosure as u8]);
        } else {
            hasher.update(&[0]);
        }
        for (proposal, disposition) in &record.candidate_proposals {
            hasher.update(proposal.as_bytes());
            hasher.update(&[proposal_disposition_code(*disposition)]);
        }
        for target in &record.adopted_targets {
            hash_projected_target(&mut hasher, *target);
        }
    }
    for record in exercises.values() {
        hasher.update(record.event.as_bytes());
        hasher.update(&[exercise_kind_code(record.kind)]);
        hasher.update(&[authority_code(record.authority)]);
        hasher.update(record.payload_object.as_bytes());
        for input in &record.inputs {
            hash_reference(&mut hasher, input);
        }
        for output in &record.outputs {
            hash_reference(&mut hasher, output);
        }
        if let Some(mapping) = record.mapping {
            hasher.update(mapping.as_bytes());
        }
        hasher.update(&record.observed_frontier);
    }
    *hasher.finalize().as_bytes()
}

fn hash_resolution_source(hasher: &mut blake3::Hasher, resolution: &ReceptorResolutionProjection) {
    hash_reference(hasher, &resolution.receptor_definition);
    hash_projected_target(hasher, resolution.view.target.into());
    hasher.update(&[resolution_state_code(resolution.view.state)]);
    hash_reference(hasher, &resolution.view.resolution_policy);
    hasher.update(&resolution.view.assessed_frontier);
    hasher.update(&resolution.view.reducer_version.to_be_bytes());
    let mut branches = resolution.view.branches.iter().collect::<Vec<_>>();
    branches.sort_by_key(|branch| branch.tip_event.into_bytes());
    for branch in branches {
        hasher.update(branch.tip_event.as_bytes());
        hasher.update(&[resolution_state_code(branch.state)]);
        let mut mappings = branch.active_bindings.clone();
        mappings.sort_by_key(|mapping| mapping.into_bytes());
        mappings.dedup();
        for mapping in mappings {
            hasher.update(mapping.as_bytes());
        }
    }
    let mut unresolved = resolution.view.unresolved_events.clone();
    unresolved.sort_by_key(|event| event.into_bytes());
    unresolved.dedup();
    for event in unresolved {
        hasher.update(event.as_bytes());
    }
}

fn hash_projected_object(hasher: &mut blake3::Hasher, object: &ProjectedKnowledgeObject) {
    hash_reference(hasher, &object.object);
    hasher.update(&[object.disclosure as u8]);
    hasher.update(&object.schema_major.to_be_bytes());
    hasher.update(&object.schema_minor.to_be_bytes());
    hasher.update(&object.payload_commitment);
    for reference in &object.references {
        hash_reference(hasher, reference);
    }
}

fn hash_projected_target(hasher: &mut blake3::Hasher, target: ProjectedResolutionTarget) {
    hasher.update(&target.assembly_lineage);
    hasher.update(&target.assembly_revision);
    hasher.update(&target.placement);
}

fn hash_model_version(hasher: &mut blake3::Hasher, model: &ObkgModelVersion) {
    match model {
        ObkgModelVersion::DeterministicNoModel => {
            hasher.update(&[0]);
        }
        ObkgModelVersion::Learned(reference) => {
            hasher.update(&[1]);
            hash_reference(hasher, reference);
        }
    }
}

fn hash_reference(hasher: &mut blake3::Hasher, reference: &ObjectReference) {
    hasher.update(&reference.reference_kind.to_be_bytes());
    hasher.update(&reference.cid);
}

fn reference_key(reference: &ObjectReference) -> (u64, [u8; 32]) {
    (reference.reference_kind, reference.cid)
}

fn insert_exact<K: Ord, V: PartialEq>(
    map: &mut BTreeMap<K, V>,
    key: K,
    value: V,
) -> Result<(), ObkgProjectionError> {
    match map.get(&key) {
        Some(existing) if existing != &value => Err(ObkgProjectionError::ConflictingSource),
        Some(_) => Ok(()),
        None => {
            map.insert(key, value);
            Ok(())
        }
    }
}

fn resolution_state_code(state: ku_core::foundation::ResolutionState) -> u8 {
    match state {
        ku_core::foundation::ResolutionState::Open => 0,
        ku_core::foundation::ResolutionState::PartiallySatisfied => 1,
        ku_core::foundation::ResolutionState::SatisfiedRelative => 2,
        ku_core::foundation::ResolutionState::Waived => 3,
        ku_core::foundation::ResolutionState::Deferred => 4,
        ku_core::foundation::ResolutionState::Concurrent => 5,
    }
}

fn proposal_disposition_code(disposition: ProposalDisposition) -> u8 {
    match disposition {
        ProposalDisposition::CandidateOnly => 0,
        ProposalDisposition::BlockedHardViolation => 1,
        ProposalDisposition::Expired => 2,
    }
}

fn authority_code(authority: ExerciseAuthority) -> u8 {
    match authority {
        ExerciseAuthority::Authorized => 0,
        ExerciseAuthority::Unauthorized => 1,
        ExerciseAuthority::Unresolved => 2,
    }
}

fn exercise_kind_code(kind: ExerciseProjectionKind) -> u8 {
    match kind {
        ExerciseProjectionKind::Use => 0,
        ExerciseProjectionKind::Derivation => 1,
    }
}

fn projection_hasher(label: &[u8]) -> blake3::Hasher {
    let mut hasher = blake3::Hasher::new();
    hasher.update(b"onebrain:vnext:obkg-projection:1\0");
    hasher.update(label);
    hasher.update(&[0]);
    hasher
}

fn digest_bytes(label: &[u8], bytes: &[u8]) -> [u8; 32] {
    let mut hasher = projection_hasher(label);
    hasher.update(&(bytes.len() as u64).to_be_bytes());
    hasher.update(bytes);
    *hasher.finalize().as_bytes()
}

#[derive(Debug, PartialEq, Eq)]
pub enum ObkgProjectionError {
    Canonical(ku_core::foundation::CanonicalError),
    StandingNeed(StandingNeedError),
    Proposal(ProposalError),
    InvalidFrontier,
    InvalidModelVersion,
    Limit,
    ResolutionReducerVersionMismatch,
    MissingMinimalSnapshot,
    UnsupportedObjectKind,
    OpaqueObject,
    ConflictingSource,
    AdoptionWithoutMaterializedMapping,
}

impl From<ku_core::foundation::CanonicalError> for ObkgProjectionError {
    fn from(error: ku_core::foundation::CanonicalError) -> Self {
        Self::Canonical(error)
    }
}

impl From<StandingNeedError> for ObkgProjectionError {
    fn from(error: StandingNeedError) -> Self {
        Self::StandingNeed(error)
    }
}

impl From<ProposalError> for ObkgProjectionError {
    fn from(error: ProposalError) -> Self {
        Self::Proposal(error)
    }
}

#[cfg(test)]
mod tests {
    use ed25519_dalek::SigningKey;
    use ku_core::foundation::{
        decode_feed_inception, decode_knowledge_event, decode_knowledge_object, AssemblyLineageId,
        CanonicalValue, ConceptCcid, DerivationEvidencePayload, DerivationInput, DeviceId,
        ExerciseEvidence, FeedInception, KnowledgeEventEnvelope, KnowledgeObjectEnvelope,
        KnownObjectKind, MappingEnvelope, MappingKernel, NamespaceCommitment, ObjectKind,
        PlacementId, ResolutionBranch, ResolutionState, ResolutionTarget, ResolutionView,
        SchemaVersion, SemanticFrameSet, SignedFeedInception, UseEvidencePayload, UseMode,
        ValidatedDerivationEvidenceEvent, ValidatedUseEvidenceEvent,
        DERIVATION_EVIDENCE_EVENT_TYPE, DERIVATION_EVIDENCE_KIND, USE_EVIDENCE_EVENT_TYPE,
        USE_EVIDENCE_KIND,
    };

    use super::*;
    use crate::vnext_proposal::ProposalExpiry;

    fn reference(kind: u64, byte: u8) -> ObjectReference {
        ObjectReference::new(kind, [byte; 32])
    }

    fn frontier() -> ObkgSourceFrontier {
        ObkgSourceFrontier::new(
            SelectorCid::from_bytes([1; 32]),
            [2; 32],
            [(FeedId::from_bytes([3; 32]), 7)],
        )
        .unwrap()
    }

    fn config() -> ObkgProjectionConfig {
        ObkgProjectionConfig {
            source_frontier: frontier(),
            model_version: ObkgModelVersion::DeterministicNoModel,
            current_evaluation: 10,
        }
    }

    fn validated_object(kind: ObjectKind, byte: u8) -> ValidatedKnowledgeObject {
        let mut object = KnowledgeObjectEnvelope::new(
            kind,
            SchemaVersion::new(1, 0),
            DisclosureClass::Public,
            CanonicalValue::Map(vec![(0, CanonicalValue::Unsigned(1))]),
        );
        object.references = vec![reference(0, byte + 1)];
        let (bytes, _) = object.encode(ResourceProfile::ObjectV1).unwrap();
        decode_knowledge_object(
            &bytes,
            ResourceProfile::ObjectV1,
            &[KnownObjectKind::new(kind, 1)],
            &[],
        )
        .unwrap()
    }

    fn proposal() -> BindingProposal {
        let mapping_kernel = MappingKernel {
            source_objects: vec![reference(0, 10)],
            target_objects: vec![reference(0, 11)],
            correspondences: Vec::new(),
            assumptions: SemanticFrameSet {
                statements: Vec::new(),
            },
            constraint_regions: Vec::new(),
            unmapped_regions: Vec::new(),
        };
        let kernel = mapping_kernel.cid().unwrap();
        BindingProposal {
            mapping_kernel,
            proposed_envelope: MappingEnvelope {
                kernel,
                generator: reference(0, 12),
                derivation_rule: None,
                evidence: Vec::new(),
                source_event: None,
            },
            candidate_objects: vec![reference(0, 10), reference(0, 11)],
            index_commitment: None,
            model_commitment: None,
            rule_commitment: None,
            scores: Vec::new(),
            constraints: Vec::new(),
            expiry: ProposalExpiry {
                created_at_evaluation: 10,
                expires_after_evaluations: 10,
                source_frontier: EventCid::from_bytes([13; 32]),
            },
            privacy: DisclosureClass::LocalOnly,
        }
    }

    fn target() -> ResolutionTarget {
        ResolutionTarget {
            assembly_lineage: AssemblyLineageId::from_bytes([20; 32]),
            assembly_revision: ObjectCid::from_bytes([21; 32]),
            placement: PlacementId::from_bytes([22; 32]),
        }
    }

    fn resolution(
        receptor: ObjectReference,
        kernel: Option<MappingKernelCid>,
    ) -> ReceptorResolutionProjection {
        ReceptorResolutionProjection {
            receptor_definition: receptor,
            view: ResolutionView {
                target: target(),
                state: if kernel.is_some() {
                    ResolutionState::SatisfiedRelative
                } else {
                    ResolutionState::Open
                },
                resolution_policy: reference(0, 23),
                assessed_frontier: [24; 32],
                reducer_version: RESOLUTION_REDUCER_VERSION,
                branches: kernel
                    .map(|mapping| {
                        vec![ResolutionBranch {
                            tip_event: EventCid::from_bytes([25; 32]),
                            state: ResolutionState::SatisfiedRelative,
                            active_bindings: vec![mapping],
                        }]
                    })
                    .unwrap_or_default(),
                unresolved_events: Vec::new(),
            },
        }
    }

    fn empty_sources<'a>(
        objects: &'a [ValidatedKnowledgeObject],
        resolutions: &'a [ReceptorResolutionProjection],
        mappings: &'a [MappingViewRecord],
        proposals: &'a [BindingProposal],
        exercises: &'a [AssessedExerciseEvidence],
    ) -> ObkgProjectionSources<'a> {
        ObkgProjectionSources {
            objects,
            standing_needs: &[],
            resolutions,
            materialized_mappings: mappings,
            candidate_proposals: proposals,
            exercises,
        }
    }

    fn validated_exercises() -> Vec<AssessedExerciseEvidence> {
        let key = SigningKey::from_bytes(&[31; 32]);
        let inception = FeedInception::new(
            *key.verifying_key().as_bytes(),
            NamespaceCommitment::derive(b"obkg-projection-test", [32; 32]).unwrap(),
            0,
            DeviceId::from_bytes([33; 32]),
        );
        let signed: SignedFeedInception = inception.sign(&key).unwrap();
        let author = decode_feed_inception(&signed.encode().unwrap()).unwrap();

        let use_payload = UseEvidencePayload {
            subjects: vec![reference(0, 40)],
            mode: UseMode::Application,
            actor_class: ConceptCcid::from_bytes([41; 16]),
            task_context_commitment: [42; 32],
            causal_role: ConceptCcid::from_bytes([43; 16]),
            assembly: None,
            mapping: None,
            outcome_observation: None,
            use_policy: reference(0, 44),
            observed_frontier: [45; 32],
        };
        let use_object = use_payload
            .to_knowledge_object(DisclosureClass::LocalOnly)
            .unwrap();
        let (use_bytes, use_cid) = use_object.encode(ResourceProfile::ObjectV1).unwrap();
        let use_object = decode_knowledge_object(
            &use_bytes,
            ResourceProfile::ObjectV1,
            &[KnownObjectKind::new(USE_EVIDENCE_KIND, 1)],
            &[],
        )
        .unwrap();
        let mut use_event = KnowledgeEventEnvelope::new(
            USE_EVIDENCE_EVENT_TYPE,
            author.feed_id,
            0,
            DisclosureClass::LocalOnly,
            [46; 32],
        );
        use_event.payload_refs = vec![ObjectReference::new(0, use_cid.into_bytes())];
        let use_bytes = use_event.sign(&author, &key).unwrap().encode().unwrap().0;
        let use_event =
            decode_knowledge_event(&use_bytes, &author, &[USE_EVIDENCE_EVENT_TYPE]).unwrap();
        let use_event = ValidatedUseEvidenceEvent::bind(&use_event, &use_object).unwrap();

        let derivation_payload = DerivationEvidencePayload {
            inputs: vec![DerivationInput {
                input: reference(0, 40),
                causal_role: ConceptCcid::from_bytes([47; 16]),
            }],
            output: reference(0, 48),
            derivation_rule: reference(0, 49),
            task_context_commitment: [50; 32],
            derivation_policy: reference(0, 51),
            observed_frontier: [52; 32],
        };
        let derivation_object = derivation_payload
            .to_knowledge_object(DisclosureClass::LocalOnly)
            .unwrap();
        let (derivation_bytes, derivation_cid) =
            derivation_object.encode(ResourceProfile::ObjectV1).unwrap();
        let derivation_object = decode_knowledge_object(
            &derivation_bytes,
            ResourceProfile::ObjectV1,
            &[KnownObjectKind::new(DERIVATION_EVIDENCE_KIND, 1)],
            &[],
        )
        .unwrap();
        let mut derivation_event = KnowledgeEventEnvelope::new(
            DERIVATION_EVIDENCE_EVENT_TYPE,
            author.feed_id,
            1,
            DisclosureClass::LocalOnly,
            [53; 32],
        );
        derivation_event.payload_refs = vec![ObjectReference::new(0, derivation_cid.into_bytes())];
        let derivation_bytes = derivation_event
            .sign(&author, &key)
            .unwrap()
            .encode()
            .unwrap()
            .0;
        let derivation_event = decode_knowledge_event(
            &derivation_bytes,
            &author,
            &[DERIVATION_EVIDENCE_EVENT_TYPE],
        )
        .unwrap();
        let derivation_event =
            ValidatedDerivationEvidenceEvent::bind(&derivation_event, &derivation_object).unwrap();

        vec![
            AssessedExerciseEvidence {
                evidence: ExerciseEvidence::Use(use_event),
                authority: ExerciseAuthority::Authorized,
            },
            AssessedExerciseEvidence {
                evidence: ExerciseEvidence::Derivation(derivation_event),
                authority: ExerciseAuthority::Authorized,
            },
        ]
    }

    #[test]
    fn disposable_projection_rebuilds_to_same_roots_after_delete() {
        let receptor = validated_object(RECEPTOR_DEFINITION_KIND, 60);
        let affordance = validated_object(KNOWLEDGE_AFFORDANCE_KIND, 61);
        let mut objects = vec![receptor, affordance];
        let first =
            ObkgProjection::rebuild(config(), empty_sources(&objects, &[], &[], &[], &[])).unwrap();
        let expected = first.snapshot().clone();
        drop(first);
        objects.reverse();
        let rebuilt =
            ObkgProjection::rebuild(config(), empty_sources(&objects, &[], &[], &[], &[])).unwrap();
        assert_eq!(rebuilt.snapshot(), &expected);
        assert_eq!(rebuilt.snapshot().reducer_version, 1);
        assert_eq!(rebuilt.snapshot().index_version, 1);
        assert!(!rebuilt.is_source_of_record());
        assert!(!rebuilt.is_resolution_reducer());
    }

    #[test]
    fn proposal_and_materialization_are_not_active_before_adoption() {
        let proposal = proposal();
        let kernel = proposal.kernel_id().unwrap();
        let proposals = vec![proposal];
        let candidate =
            ObkgProjection::rebuild(config(), empty_sources(&[], &[], &[], &proposals, &[]))
                .unwrap();
        assert!(candidate.mapping(kernel).unwrap().is_candidate_only());
        assert_eq!(candidate.active_mapping_edge_count(), 0);

        let materialized = vec![MappingViewRecord {
            kernel,
            envelope: ObjectCid::from_bytes([70; 32]),
            disclosure: DisclosureClass::LocalOnly,
        }];
        let pinned = ObkgProjection::rebuild(
            config(),
            empty_sources(&[], &[], &materialized, &proposals, &[]),
        )
        .unwrap();
        assert!(!pinned.mapping(kernel).unwrap().is_active_edge());

        let resolutions = vec![resolution(
            reference(RECEPTOR_DEFINITION_KIND.0, 60),
            Some(kernel),
        )];
        let adopted = ObkgProjection::rebuild(
            config(),
            empty_sources(&[], &resolutions, &materialized, &proposals, &[]),
        )
        .unwrap();
        assert!(adopted.mapping(kernel).unwrap().is_active_edge());
        assert_eq!(adopted.active_mapping_edge_count(), 1);
    }

    #[test]
    fn adoption_without_materialized_mapping_is_rejected() {
        let kernel = proposal().kernel_id().unwrap();
        let resolutions = vec![resolution(
            reference(RECEPTOR_DEFINITION_KIND.0, 60),
            Some(kernel),
        )];
        assert_eq!(
            ObkgProjection::rebuild(config(), empty_sources(&[], &resolutions, &[], &[], &[]))
                .unwrap_err(),
            ObkgProjectionError::AdoptionWithoutMaterializedMapping
        );
    }

    #[test]
    fn use_view_accepts_only_validated_causal_exercise_lane() {
        let exercises = validated_exercises();
        let use_id = exercises[0].evidence.event_cid();
        let derivation_id = exercises[1].evidence.event_cid();
        let projection =
            ObkgProjection::rebuild(config(), empty_sources(&[], &[], &[], &[], &exercises))
                .unwrap();
        assert_eq!(
            projection.exercise(use_id).unwrap().kind,
            ExerciseProjectionKind::Use
        );
        assert_eq!(
            projection.exercise(derivation_id).unwrap().kind,
            ExerciseProjectionKind::Derivation
        );
        assert_eq!(
            projection.authorized_exercise_events_for(&reference(0, 40)),
            vec![use_id, derivation_id]
        );
        assert!(!projection.accepts_exposure_telemetry());
    }

    #[test]
    fn foreign_resolution_reducer_version_fails_closed() {
        let mut resolution = resolution(reference(RECEPTOR_DEFINITION_KIND.0, 60), None);
        resolution.view.reducer_version += 1;
        assert_eq!(
            ObkgProjection::rebuild(config(), empty_sources(&[], &[resolution], &[], &[], &[]))
                .unwrap_err(),
            ObkgProjectionError::ResolutionReducerVersionMismatch
        );
    }
}

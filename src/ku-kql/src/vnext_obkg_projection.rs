//! Disposable OBKG projections over vNext canonical sources.
//!
//! This module consumes already validated objects, the KQL-005 minimal view
//! sources, candidate proposals and signed causal exercise evidence. It never
//! reduces resolution events itself and owns no canonical knowledge state.

use std::collections::{BTreeMap, BTreeSet};

use ku_core::concept_lifecycle::{
    AuthoringConceptResolution, ConceptLifecycleRecord, ConceptLifecycleTable, ConceptRetireReason,
};
use ku_core::foundation::assembly::{ASSEMBLY_PROFILE_MAJOR, ASSEMBLY_PROFILE_MINOR};
use ku_core::foundation::resolution::RESOLUTION_REDUCER_VERSION;
use ku_core::foundation::{
    encode_canonical, project_virtual_sfs_affordance, AffordanceError, AssemblyError,
    AssemblyLineageId, AssessedExerciseEvidence, BenefitClosureState, BenefitClosureView,
    CanonicalValue, CheckpointCid, ConceptCcid, ConstraintEvaluation, DimensionVector,
    DisclosureClass, EventCid, ExerciseAuthority, ExerciseEvidence, FeedId, FidelityAssessment,
    FrontierAssemblyManifest, KnowledgeAffordance, LiteralValue, MappingKernelCid,
    MetabolicEvidenceView, ObjectCid, ObjectReference, ObjectSemantics, PlacementId,
    ReceptorCardinality, ReceptorDefinition, ReceptorError, ReceptorPlacement, ResolutionState,
    ResourceProfile, SelectorCid, SemanticError, SemanticFrameSet, StatementId, TermRef,
    ValidatedKnowledgeObject, ASSEMBLY_MANIFEST_KIND, FIDELITY_PROFILE_MAJOR,
    KNOWLEDGE_AFFORDANCE_KIND, RECEPTOR_DEFINITION_KIND, SEMANTIC_KERNEL_OBJECT_KIND,
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
    Semantic(SemanticError),
    Receptor(ReceptorError),
    Affordance(AffordanceError),
    Assembly(AssemblyError),
    StandingNeed(StandingNeedError),
    Proposal(ProposalError),
    InvalidFrontier,
    InvalidRegistryReleaseRoot,
    InvalidPolicyReference,
    InvalidModelVersion,
    Limit,
    ResolutionReducerVersionMismatch,
    MissingMinimalSnapshot,
    UnsupportedObjectKind,
    OpaqueObject,
    ConflictingSource,
    AdoptionWithoutMaterializedMapping,
    /// `NEG-P7-001`: Historical inspection MUST bind to cryptographic frontiers, never wall-clock timestamps.
    WallClockHistoryForbidden,
    FrontierSnapshotNotFound,
}

impl From<ku_core::foundation::CanonicalError> for ObkgProjectionError {
    fn from(error: ku_core::foundation::CanonicalError) -> Self {
        Self::Canonical(error)
    }
}

impl From<SemanticError> for ObkgProjectionError {
    fn from(error: SemanticError) -> Self {
        Self::Semantic(error)
    }
}

impl From<ReceptorError> for ObkgProjectionError {
    fn from(error: ReceptorError) -> Self {
        Self::Receptor(error)
    }
}

impl From<AffordanceError> for ObkgProjectionError {
    fn from(error: AffordanceError) -> Self {
        Self::Affordance(error)
    }
}

impl From<AssemblyError> for ObkgProjectionError {
    fn from(error: AssemblyError) -> Self {
        Self::Assembly(error)
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

// ═══════════════════════════════════════════════════════════════════════════
// OBKG-002 Unified 5-Plane Disposable Projection (`P7-OBKG-UNIFIED-001` §2)
// ═══════════════════════════════════════════════════════════════════════════

pub const OBKG_MULTI_PLANE_REDUCER_VERSION: u64 = 2;
pub const RETIRED_CONCEPT_ANNOTATION: &str = "[retired_concept]";

/// Authorized private-vault overlay KU entry supplied to `OBKG-002` pure projection (`FW-09`).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AuthorizedVaultSemanticOverlay {
    pub object: ObjectReference,
    pub semantic: SemanticFrameSet,
}

/// Configuration for `OBKG-002` 5-plane projection (`P7` §2.1).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ObkgMultiPlaneConfig {
    pub source_frontier: ObkgSourceFrontier,
    pub checkpoint_anchor: Option<CheckpointCid>,
    pub resolution_policy_ref: ObjectReference,
    pub fidelity_policy_ref: ObjectReference,
    pub metabolic_policy_ref: ObjectReference,
    pub registry_release_root: [u8; 32],
    pub model_version: ObkgModelVersion,
    pub current_evaluation: u64,
}

impl ObkgMultiPlaneConfig {
    fn validate(&self) -> Result<(), ObkgProjectionError> {
        self.model_version.validate()?;
        if self.registry_release_root == [0; 32] {
            return Err(ObkgProjectionError::InvalidRegistryReleaseRoot);
        }
        if self.resolution_policy_ref.cid == [0; 32]
            || self.fidelity_policy_ref.cid == [0; 32]
            || self.metabolic_policy_ref.cid == [0; 32]
        {
            return Err(ObkgProjectionError::InvalidPolicyReference);
        }
        Ok(())
    }
}

/// Complete read-only source bundle for `OBKG-002` 5-plane projection.
pub struct ObkgMultiPlaneSources<'a> {
    pub objects: &'a [ValidatedKnowledgeObject],
    pub authorized_vault_kus: &'a [AuthorizedVaultSemanticOverlay],
    pub lifecycle_table: Option<&'a ConceptLifecycleTable>,
    pub standing_needs: &'a [StandingNeed],
    pub resolutions: &'a [ReceptorResolutionProjection],
    pub materialized_mappings: &'a [MappingViewRecord],
    pub candidate_proposals: &'a [BindingProposal],
    pub metabolic_views: &'a [MetabolicEvidenceView],
    pub benefit_closures: &'a [BenefitClosureView],
    pub fidelity_assessments: &'a [FidelityAssessment],
    pub exercises: &'a [AssessedExerciseEvidence],
}

// ─── Plane 1: Semantic Statement Plane ──────────────────────────────────────

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ProjectedStatementHyperedge {
    pub ku_object: ObjectReference,
    pub disclosure: DisclosureClass,
    pub statement_id: StatementId,
    pub predicate: ConceptCcid,
    pub canonical_predicate: ConceptCcid,
    pub concept_terms: Vec<ConceptCcid>,
    pub units: Vec<ConceptCcid>,
    pub dimensions: Vec<DimensionVector>,
    pub retired_concepts: Vec<(ConceptCcid, ConceptRetireReason)>,
    pub redirect_provenance: Vec<(ConceptCcid, ConceptCcid)>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ProjectedConceptNode {
    pub ccid: ConceptCcid,
    pub canonical_ccid: ConceptCcid,
    pub equivalence_class: BTreeSet<ConceptCcid>,
    pub retired_reason: Option<ConceptRetireReason>,
    pub annotations: Vec<&'static str>,
    pub referencing_kus: BTreeSet<(u64, [u8; 32])>,
}

impl ProjectedConceptNode {
    pub fn is_retired(&self) -> bool {
        self.retired_reason.is_some()
    }
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct SemanticStatementPlane {
    pub ku_payload_commitments: BTreeMap<(u64, [u8; 32]), [u8; 32]>,
    pub hyperedges: Vec<ProjectedStatementHyperedge>,
    pub concepts: BTreeMap<ConceptCcid, ProjectedConceptNode>,
}

// ─── Plane 2: Gap & Affordance Plane ────────────────────────────────────────

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub struct AssemblyHypernodeKey {
    pub lineage_id: [u8; 32],
    pub revision: u64,
    pub placement_id: [u8; 32],
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AssemblyPlacementHypernode {
    pub key: AssemblyHypernodeKey,
    pub manifest_object: ObjectReference,
    pub receptor_definition: ObjectReference,
    pub cardinality: ReceptorCardinality,
    pub required: bool,
    pub constraint_annotations: Vec<ConstraintEvaluation>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ReceptorPortNode {
    pub object: ObjectReference,
    pub role: ConceptCcid,
    pub expected_types: Vec<ConceptCcid>,
    pub cardinality: ReceptorCardinality,
    pub standing_needs: Vec<StandingNeedId>,
    pub constraint_annotations: Vec<ConstraintEvaluation>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AffordanceOfferNode {
    pub object: ObjectReference,
    pub offered_roles: Vec<ConceptCcid>,
    pub is_virtual_sfs: bool,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct GapAndAffordancePlane {
    pub assembly_hypernodes: BTreeMap<AssemblyHypernodeKey, AssemblyPlacementHypernode>,
    pub receptor_ports: BTreeMap<(u64, [u8; 32]), ReceptorPortNode>,
    pub explicit_affordances: BTreeMap<(u64, [u8; 32]), AffordanceOfferNode>,
    pub virtual_sfs_affordances: BTreeMap<(u64, [u8; 32]), AffordanceOfferNode>,
}

// ─── Plane 3: Three-Lane Mapping & Resolution Plane ─────────────────────────

/// Lane A (`LOCAL_ONLY`, `KQL-013`): Quarantined non-executable proposals.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct QuarantinedProposalOverlay {
    pub proposal_id: ProposalId,
    pub kernel: MappingKernelCid,
    pub disposition: ProposalDisposition,
}

impl QuarantinedProposalOverlay {
    pub const fn is_active_edge(&self) -> bool {
        false
    }
}

/// Lane B (`KU-006`): Materialized mapping kernel not yet adopted at the frontier.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct MaterializedInactiveEdge {
    pub kernel: MappingKernelCid,
    pub envelope: ObjectCid,
    pub disclosure: DisclosureClass,
}

impl MaterializedInactiveEdge {
    pub const fn is_active_edge(&self) -> bool {
        false
    }
}

/// Lane C (`KU-006` + `KU-007`): Materialized mapping adopted by an authorized resolution event.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AdoptedActiveRelativeEdge {
    pub kernel: MappingKernelCid,
    pub envelope: ObjectCid,
    pub disclosure: DisclosureClass,
    pub target: ProjectedResolutionTarget,
    pub target_resolution_state: ResolutionState,
}

impl AdoptedActiveRelativeEdge {
    pub const fn is_active_edge(&self) -> bool {
        true
    }
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct ThreeLaneMappingPlane {
    pub quarantined_proposals: BTreeMap<[u8; 32], QuarantinedProposalOverlay>,
    pub materialized_inactive: BTreeMap<[u8; 32], MaterializedInactiveEdge>,
    pub adopted_active: Vec<AdoptedActiveRelativeEdge>,
    pub target_states: BTreeMap<ProjectedResolutionTarget, ResolutionState>,
}

impl ThreeLaneMappingPlane {
    pub fn is_kernel_active(&self, kernel: MappingKernelCid) -> bool {
        self.adopted_active.iter().any(|edge| edge.kernel == kernel)
    }

    pub fn active_bindings_for_target(
        &self,
        target: &ProjectedResolutionTarget,
    ) -> Vec<&AdoptedActiveRelativeEdge> {
        self.adopted_active
            .iter()
            .filter(|edge| &edge.target == target)
            .collect()
    }
}

// ─── Plane 4: Evidence & Fidelity Overlay Plane ─────────────────────────────

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub struct EvidenceOverlayKey {
    pub policy_cid: [u8; 32],
    pub frontier_root: [u8; 32],
    pub reducer_version: u64,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct EvidenceAndFidelityOverlayPlane {
    pub metabolic_views: BTreeMap<(EvidenceOverlayKey, (u64, [u8; 32])), MetabolicEvidenceView>,
    pub benefit_closures: BTreeMap<(EvidenceOverlayKey, [u8; 32]), BenefitClosureView>,
    pub fidelity_assessments: BTreeMap<(EvidenceOverlayKey, (u64, [u8; 32])), FidelityAssessment>,
    pub zero_use_kus: BTreeSet<(u64, [u8; 32])>,
    pub contested_outcome_cases: BTreeSet<[u8; 32]>,
}

// ─── Plane 5: Frontier-Anchored Historical Plane & Snapshot ─────────────────

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ObkgHistoricalSelector {
    AtFrontier(ObkgSourceFrontier),
    AtCheckpoint(CheckpointCid),
    /// Explicitly modeled so callers passing legacy `AT TIME <unix_ts>` fail closed with
    /// [`ObkgProjectionError::WallClockHistoryForbidden`] (`NEG-P7-001`).
    AtWallClockTimestamp(u64),
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct ObkgFrontierDiff {
    pub from_frontier_root: [u8; 32],
    pub to_frontier_root: [u8; 32],
    pub added_kus: Vec<(u64, [u8; 32])>,
    pub removed_kus: Vec<(u64, [u8; 32])>,
    pub added_assembly_hypernodes: Vec<AssemblyHypernodeKey>,
    pub removed_assembly_hypernodes: Vec<AssemblyHypernodeKey>,
    pub newly_adopted_edges: Vec<(MappingKernelCid, ProjectedResolutionTarget)>,
    pub deactivated_edges: Vec<(MappingKernelCid, ProjectedResolutionTarget)>,
    pub newly_materialized_inactive: Vec<MappingKernelCid>,
    pub benefit_state_changes: Vec<(
        [u8; 32],
        Option<BenefitClosureState>,
        Option<BenefitClosureState>,
    )>,
}

/// Immutable, pure-functional 5-plane OBKG snapshot (`OBKG-002`, `FW-09`).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ObkgMultiPlaneSnapshot {
    pub config: ObkgMultiPlaneConfig,
    pub plane1_semantic: SemanticStatementPlane,
    pub plane2_gaps_affordances: GapAndAffordancePlane,
    pub plane3_mappings: ThreeLaneMappingPlane,
    pub plane4_evidence_fidelity: EvidenceAndFidelityOverlayPlane,
    commitment_cid: [u8; 32],
}

impl ObkgMultiPlaneSnapshot {
    pub const fn snapshot_commitment_cid(&self) -> [u8; 32] {
        self.commitment_cid
    }

    pub const fn holds_canonical_truth(&self) -> bool {
        false
    }

    pub const fn emits_obt_rewards(&self) -> bool {
        false
    }
}

/// Multi-plane disposable OBKG coordinator retaining frontier-anchored snapshots (`Plane 5`)
/// while holding zero canonical truth (`FW-09`).
#[derive(Clone, Debug, Default)]
pub struct ObkgMultiPlaneProjection {
    by_frontier_root: BTreeMap<[u8; 32], ObkgMultiPlaneSnapshot>,
    by_checkpoint: BTreeMap<[u8; 32], [u8; 32]>,
    latest_frontier_root: Option<[u8; 32]>,
}

impl ObkgMultiPlaneProjection {
    pub fn project(
        config: ObkgMultiPlaneConfig,
        sources: ObkgMultiPlaneSources<'_>,
    ) -> Result<ObkgMultiPlaneSnapshot, ObkgProjectionError> {
        config.validate()?;
        if [
            sources.objects.len(),
            sources.authorized_vault_kus.len(),
            sources.standing_needs.len(),
            sources.resolutions.len(),
            sources.materialized_mappings.len(),
            sources.candidate_proposals.len(),
            sources.metabolic_views.len(),
            sources.benefit_closures.len(),
            sources.fidelity_assessments.len(),
            sources.exercises.len(),
        ]
        .into_iter()
        .any(|len| len > MAX_OBKG_PROJECTION_SOURCES)
        {
            return Err(ObkgProjectionError::Limit);
        }

        for resolution in sources.resolutions {
            if resolution.view.reducer_version != RESOLUTION_REDUCER_VERSION {
                return Err(ObkgProjectionError::ResolutionReducerVersionMismatch);
            }
            if resolution.view.assessed_frontier == [0; 32] {
                return Err(ObkgProjectionError::InvalidFrontier);
            }
        }

        let minimal = MinimalKnowledgeViews::rebuild(
            sources.standing_needs,
            sources.resolutions,
            sources.materialized_mappings,
        )?;

        let mut plane1 = SemanticStatementPlane::default();
        let mut plane2 = GapAndAffordancePlane::default();
        let eq_classes = sources
            .lifecycle_table
            .map(ConceptLifecycleTable::equivalence_classes)
            .unwrap_or_default();

        // Project validated canonical objects (kinds 2, 3, 4, 5)
        let mut sorted_objects: Vec<&ValidatedKnowledgeObject> = sources.objects.iter().collect();
        sorted_objects.sort_by_key(|obj| obj.cid().into_bytes());
        sorted_objects.dedup_by_key(|obj| obj.cid().into_bytes());

        for object in sorted_objects {
            let ObjectSemantics::Known(envelope) = object.semantics() else {
                return Err(ObkgProjectionError::OpaqueObject);
            };
            let obj_ref = ObjectReference::new(envelope.kind.0, object.cid().into_bytes());
            let key = reference_key(&obj_ref);
            let payload_bytes = encode_canonical(&envelope.payload, ResourceProfile::ObjectV1)?;
            let payload_commitment = digest_bytes(b"mp-object-payload", &payload_bytes);

            match envelope.kind {
                k if k == SEMANTIC_KERNEL_OBJECT_KIND => {
                    let semantic = SemanticFrameSet::from_canonical_value(&envelope.payload)?;
                    project_semantic_ku_into_planes(
                        &mut plane1,
                        &mut plane2,
                        obj_ref,
                        envelope.disclosure,
                        payload_commitment,
                        &semantic,
                        sources.lifecycle_table,
                        &eq_classes,
                    )?;
                }
                k if k == RECEPTOR_DEFINITION_KIND => {
                    let def = ReceptorDefinition::from_canonical_payload(&envelope.payload)?;
                    let annotations = def
                        .hard_constraints
                        .iter()
                        .map(|_| ConstraintEvaluation::Unknown)
                        .collect();
                    let port = ReceptorPortNode {
                        standing_needs: minimal.standing_needs_for(&obj_ref),
                        role: def.role,
                        expected_types: def.expected_types,
                        cardinality: def.cardinality,
                        object: obj_ref,
                        constraint_annotations: annotations,
                    };
                    insert_exact(&mut plane2.receptor_ports, key, port)?;
                }
                k if k == ASSEMBLY_MANIFEST_KIND => {
                    let manifest = decode_assembly_manifest(&envelope.payload)?;
                    for placement in &manifest.placements {
                        let hkey = AssemblyHypernodeKey {
                            lineage_id: *manifest.lineage.as_bytes(),
                            revision: manifest.revision,
                            placement_id: *placement.placement_id.as_bytes(),
                        };
                        let annotations = placement
                            .local_context
                            .statements
                            .iter()
                            .flat_map(|s| s.constraints.iter())
                            .map(|_| ConstraintEvaluation::Unknown)
                            .collect();
                        let hypernode = AssemblyPlacementHypernode {
                            key: hkey,
                            manifest_object: obj_ref.clone(),
                            receptor_definition: placement.receptor_definition.clone(),
                            cardinality: placement.cardinality,
                            required: placement.required,
                            constraint_annotations: annotations,
                        };
                        insert_exact(&mut plane2.assembly_hypernodes, hkey, hypernode)?;
                    }
                }
                k if k == KNOWLEDGE_AFFORDANCE_KIND => {
                    let aff = KnowledgeAffordance::from_validated_object(object)?;
                    insert_exact(
                        &mut plane2.explicit_affordances,
                        key,
                        AffordanceOfferNode {
                            object: obj_ref,
                            offered_roles: aff.offered_roles,
                            is_virtual_sfs: false,
                        },
                    )?;
                }
                _ => return Err(ObkgProjectionError::UnsupportedObjectKind),
            }
        }

        // Project authorized PrivateVault overlay KUs (Plane 1 + Plane 2 virtual affordances)
        let mut vault_kus: Vec<&AuthorizedVaultSemanticOverlay> =
            sources.authorized_vault_kus.iter().collect();
        vault_kus.sort_by_key(|v| reference_key(&v.object));
        vault_kus.dedup_by_key(|v| reference_key(&v.object));
        for overlay in vault_kus {
            let payload_bytes = overlay.semantic.canonical_bytes()?;
            let payload_commitment = digest_bytes(b"mp-vault-ku-payload", &payload_bytes);
            project_semantic_ku_into_planes(
                &mut plane1,
                &mut plane2,
                overlay.object.clone(),
                DisclosureClass::LocalOnly,
                payload_commitment,
                &overlay.semantic,
                sources.lifecycle_table,
                &eq_classes,
            )?;
        }

        plane1.hyperedges.sort_by_key(|edge| {
            (
                reference_key(&edge.ku_object),
                edge.statement_id.0,
                edge.predicate,
            )
        });

        // Plane 3: Three-Lane Mapping & Resolution Plane
        let mut plane3 = ThreeLaneMappingPlane::default();
        let mut materialized_by_kernel = BTreeMap::<[u8; 32], MappingViewRecord>::new();
        for mapping in sources.materialized_mappings {
            insert_exact(
                &mut materialized_by_kernel,
                mapping.kernel.into_bytes(),
                mapping.clone(),
            )?;
        }

        for proposal in sources.candidate_proposals {
            proposal.validate()?;
            let pid = proposal.proposal_id()?;
            let overlay = QuarantinedProposalOverlay {
                proposal_id: pid,
                kernel: proposal.kernel_id()?,
                disposition: proposal.disposition(config.current_evaluation),
            };
            insert_exact(&mut plane3.quarantined_proposals, *pid.as_bytes(), overlay)?;
        }

        let mut adopted_kernels = BTreeSet::<[u8; 32]>::new();
        let mut adopted_edges = Vec::new();
        for resolution in sources.resolutions {
            let target = ProjectedResolutionTarget::from(resolution.view.target);
            plane3.target_states.insert(target, resolution.view.state);
            for branch in &resolution.view.branches {
                for kernel in &branch.active_bindings {
                    let Some(mat) = materialized_by_kernel.get(kernel.as_bytes()) else {
                        return Err(ObkgProjectionError::AdoptionWithoutMaterializedMapping);
                    };
                    adopted_kernels.insert(kernel.into_bytes());
                    adopted_edges.push(AdoptedActiveRelativeEdge {
                        kernel: *kernel,
                        envelope: mat.envelope,
                        disclosure: mat.disclosure,
                        target,
                        target_resolution_state: resolution.view.state,
                    });
                }
            }
        }
        adopted_edges.sort_by_key(|edge| (edge.kernel.into_bytes(), edge.target));
        adopted_edges.dedup_by_key(|edge| (edge.kernel.into_bytes(), edge.target));
        plane3.adopted_active = adopted_edges;

        for (kernel_bytes, mat) in &materialized_by_kernel {
            if !adopted_kernels.contains(kernel_bytes) {
                plane3.materialized_inactive.insert(
                    *kernel_bytes,
                    MaterializedInactiveEdge {
                        kernel: mat.kernel,
                        envelope: mat.envelope,
                        disclosure: mat.disclosure,
                    },
                );
            }
        }

        // Plane 4: Evidence & Fidelity Overlay Plane
        let mut plane4 = EvidenceAndFidelityOverlayPlane::default();
        let mut exercised_kus = BTreeSet::<(u64, [u8; 32])>::new();
        for assessed in sources.exercises {
            if assessed.authority == ExerciseAuthority::Authorized {
                match &assessed.evidence {
                    ExerciseEvidence::Use(ev) => {
                        for subj in &ev.payload().subjects {
                            exercised_kus.insert(reference_key(subj));
                        }
                    }
                    ExerciseEvidence::Derivation(ev) => {
                        for inp in &ev.payload().inputs {
                            exercised_kus.insert(reference_key(&inp.input));
                        }
                        exercised_kus.insert(reference_key(&ev.payload().output));
                    }
                }
            }
        }

        for view in sources.metabolic_views {
            if !view.cumulative_event_ids.is_empty() {
                exercised_kus.insert(reference_key(&view.target));
            }
            let key = EvidenceOverlayKey {
                policy_cid: view.policy.policy_ref.cid,
                frontier_root: view.frontier_root,
                reducer_version: view.schema_major,
            };
            plane4
                .metabolic_views
                .insert((key, reference_key(&view.target)), view.clone());
        }

        for ku_key in plane1.ku_payload_commitments.keys() {
            if !exercised_kus.contains(ku_key) {
                plane4.zero_use_kus.insert(*ku_key);
            }
        }

        let frontier_root = config.source_frontier.root();
        for closure in sources.benefit_closures {
            if matches!(
                closure.state,
                BenefitClosureState::ContestedOrMixed | BenefitClosureState::ObservedNonBeneficial
            ) {
                plane4
                    .contested_outcome_cases
                    .insert(*closure.case_id.as_bytes());
            }
            let key = EvidenceOverlayKey {
                policy_cid: config.metabolic_policy_ref.cid,
                frontier_root,
                reducer_version: 1,
            };
            plane4
                .benefit_closures
                .insert((key, *closure.case_id.as_bytes()), closure.clone());
        }

        for assessment in sources.fidelity_assessments {
            let key = EvidenceOverlayKey {
                policy_cid: assessment.policy_ref.cid,
                frontier_root: *assessment.assessed_frontier.as_bytes(),
                reducer_version: FIDELITY_PROFILE_MAJOR,
            };
            plane4.fidelity_assessments.insert(
                (key, reference_key(&assessment.encoding_artifact)),
                assessment.clone(),
            );
        }

        let commitment_cid =
            compute_multi_plane_commitment(&config, &plane1, &plane2, &plane3, &plane4);
        Ok(ObkgMultiPlaneSnapshot {
            config,
            plane1_semantic: plane1,
            plane2_gaps_affordances: plane2,
            plane3_mappings: plane3,
            plane4_evidence_fidelity: plane4,
            commitment_cid,
        })
    }

    pub fn record_snapshot(&mut self, snapshot: ObkgMultiPlaneSnapshot) -> [u8; 32] {
        let root = snapshot.config.source_frontier.root();
        if let Some(cp) = snapshot.config.checkpoint_anchor {
            self.by_checkpoint.insert(cp.into_bytes(), root);
        }
        self.latest_frontier_root = Some(root);
        self.by_frontier_root.insert(root, snapshot);
        root
    }

    /// Discard all in-memory/cached projection state (`FW-09`).
    pub fn clear_disposable_caches(&mut self) {
        *self = Self::default();
    }

    /// Inspect the graph at a cryptographic `ObkgSourceFrontier` (`Plane 5`).
    pub fn at_frontier(
        &self,
        frontier: &ObkgSourceFrontier,
    ) -> Result<&ObkgMultiPlaneSnapshot, ObkgProjectionError> {
        self.by_frontier_root
            .get(&frontier.root())
            .ok_or(ObkgProjectionError::FrontierSnapshotNotFound)
    }

    /// Inspect via a typed [`ObkgHistoricalSelector`]; wall-clock queries fail closed with
    /// [`ObkgProjectionError::WallClockHistoryForbidden`] (`NEG-P7-001`).
    pub fn inspect_historical(
        &self,
        selector: &ObkgHistoricalSelector,
    ) -> Result<&ObkgMultiPlaneSnapshot, ObkgProjectionError> {
        match selector {
            ObkgHistoricalSelector::AtWallClockTimestamp(_) => {
                Err(ObkgProjectionError::WallClockHistoryForbidden)
            }
            ObkgHistoricalSelector::AtFrontier(frontier) => self.at_frontier(frontier),
            ObkgHistoricalSelector::AtCheckpoint(cp) => {
                let root = self
                    .by_checkpoint
                    .get(cp.as_bytes())
                    .ok_or(ObkgProjectionError::FrontierSnapshotNotFound)?;
                self.by_frontier_root
                    .get(root)
                    .ok_or(ObkgProjectionError::FrontierSnapshotNotFound)
            }
        }
    }

    /// Explicit wall-clock rejection entry point (`NEG-P7-001`).
    pub fn at_wall_clock_timestamp(
        &self,
        _unix_timestamp_s: u64,
    ) -> Result<&ObkgMultiPlaneSnapshot, ObkgProjectionError> {
        Err(ObkgProjectionError::WallClockHistoryForbidden)
    }

    /// Compute the deterministic structural delta between two frontier snapshots (`DIFF FRONTIERS`).
    pub fn diff_frontiers(
        snap_a: &ObkgMultiPlaneSnapshot,
        snap_b: &ObkgMultiPlaneSnapshot,
    ) -> ObkgFrontierDiff {
        let kus_a: BTreeSet<_> = snap_a
            .plane1_semantic
            .ku_payload_commitments
            .keys()
            .copied()
            .collect();
        let kus_b: BTreeSet<_> = snap_b
            .plane1_semantic
            .ku_payload_commitments
            .keys()
            .copied()
            .collect();

        let hyper_a: BTreeSet<_> = snap_a
            .plane2_gaps_affordances
            .assembly_hypernodes
            .keys()
            .copied()
            .collect();
        let hyper_b: BTreeSet<_> = snap_b
            .plane2_gaps_affordances
            .assembly_hypernodes
            .keys()
            .copied()
            .collect();

        let active_a: BTreeSet<([u8; 32], ProjectedResolutionTarget)> = snap_a
            .plane3_mappings
            .adopted_active
            .iter()
            .map(|e| (e.kernel.into_bytes(), e.target))
            .collect();
        let active_b: BTreeSet<([u8; 32], ProjectedResolutionTarget)> = snap_b
            .plane3_mappings
            .adopted_active
            .iter()
            .map(|e| (e.kernel.into_bytes(), e.target))
            .collect();

        let inactive_a: BTreeSet<[u8; 32]> = snap_a
            .plane3_mappings
            .materialized_inactive
            .values()
            .map(|e| e.kernel.into_bytes())
            .collect();
        let inactive_b: BTreeSet<[u8; 32]> = snap_b
            .plane3_mappings
            .materialized_inactive
            .values()
            .map(|e| e.kernel.into_bytes())
            .collect();

        let cases_a: BTreeMap<[u8; 32], BenefitClosureState> = snap_a
            .plane4_evidence_fidelity
            .benefit_closures
            .iter()
            .map(|((_, case_id), view)| (*case_id, view.state))
            .collect();
        let cases_b: BTreeMap<[u8; 32], BenefitClosureState> = snap_b
            .plane4_evidence_fidelity
            .benefit_closures
            .iter()
            .map(|((_, case_id), view)| (*case_id, view.state))
            .collect();
        let all_cases: BTreeSet<[u8; 32]> = cases_a.keys().chain(cases_b.keys()).copied().collect();
        let mut benefit_state_changes = Vec::new();
        for case_id in all_cases {
            let st_a = cases_a.get(&case_id).copied();
            let st_b = cases_b.get(&case_id).copied();
            if st_a != st_b {
                benefit_state_changes.push((case_id, st_a, st_b));
            }
        }

        ObkgFrontierDiff {
            from_frontier_root: snap_a.config.source_frontier.root(),
            to_frontier_root: snap_b.config.source_frontier.root(),
            added_kus: kus_b.difference(&kus_a).copied().collect(),
            removed_kus: kus_a.difference(&kus_b).copied().collect(),
            added_assembly_hypernodes: hyper_b.difference(&hyper_a).copied().collect(),
            removed_assembly_hypernodes: hyper_a.difference(&hyper_b).copied().collect(),
            newly_adopted_edges: active_b
                .difference(&active_a)
                .map(|&(k, t)| (MappingKernelCid::from_bytes(k), t))
                .collect(),
            deactivated_edges: active_a
                .difference(&active_b)
                .map(|&(k, t)| (MappingKernelCid::from_bytes(k), t))
                .collect(),
            newly_materialized_inactive: inactive_b
                .difference(&inactive_a)
                .map(|&k| MappingKernelCid::from_bytes(k))
                .collect(),
            benefit_state_changes,
        }
    }
}

#[allow(clippy::too_many_arguments)]
fn project_semantic_ku_into_planes(
    plane1: &mut SemanticStatementPlane,
    plane2: &mut GapAndAffordancePlane,
    ku_object: ObjectReference,
    disclosure: DisclosureClass,
    payload_commitment: [u8; 32],
    semantic: &SemanticFrameSet,
    lifecycle_table: Option<&ConceptLifecycleTable>,
    eq_classes: &BTreeMap<ConceptCcid, BTreeSet<ConceptCcid>>,
) -> Result<(), ObkgProjectionError> {
    let ku_key = reference_key(&ku_object);
    insert_exact(
        &mut plane1.ku_payload_commitments,
        ku_key,
        payload_commitment,
    )?;

    if !semantic.statements.is_empty() {
        let virtual_aff = project_virtual_sfs_affordance(ku_object.clone(), semantic)?;
        plane2.virtual_sfs_affordances.insert(
            ku_key,
            AffordanceOfferNode {
                object: ku_object.clone(),
                offered_roles: virtual_aff.offered_roles,
                is_virtual_sfs: true,
            },
        );
    }

    for stmt in &semantic.statements {
        let mut concept_terms = Vec::new();
        let mut units = Vec::new();
        let mut dimensions = Vec::new();
        let mut touched_ccids = BTreeSet::from([stmt.operator_or_predicate]);

        for arg in &stmt.arguments {
            match arg {
                TermRef::Concept(c) => {
                    concept_terms.push(*c);
                    touched_ccids.insert(*c);
                }
                TermRef::Literal(LiteralValue::Quantity(q)) => {
                    units.push(q.source_unit.unit);
                    dimensions.push(q.source_unit.dimension);
                    touched_ccids.insert(q.source_unit.unit);
                }
                _ => {}
            }
        }
        for term in [
            stmt.qualifiers.time.as_ref(),
            stmt.qualifiers.location.as_ref(),
            stmt.qualifiers.perspective.as_ref(),
        ]
        .into_iter()
        .flatten()
        {
            if let TermRef::Concept(c) = term {
                touched_ccids.insert(*c);
            }
        }

        let mut retired_concepts = Vec::new();
        let mut redirect_provenance = Vec::new();

        for &ccid in &touched_ccids {
            let (canonical_ccid, retired_reason) = resolve_node_lifecycle(ccid, lifecycle_table);
            if canonical_ccid != ccid {
                redirect_provenance.push((ccid, canonical_ccid));
            }
            if let Some(reason) = retired_reason {
                retired_concepts.push((ccid, reason));
            }
            let equivalence_class = eq_classes
                .get(&canonical_ccid)
                .cloned()
                .unwrap_or_else(|| BTreeSet::from([canonical_ccid]));
            let node = plane1.concepts.entry(ccid).or_insert_with(|| {
                let mut annotations = Vec::new();
                if retired_reason.is_some() {
                    annotations.push(RETIRED_CONCEPT_ANNOTATION);
                }
                ProjectedConceptNode {
                    ccid,
                    canonical_ccid,
                    equivalence_class,
                    retired_reason,
                    annotations,
                    referencing_kus: BTreeSet::new(),
                }
            });
            node.referencing_kus.insert(ku_key);
        }

        let (canonical_predicate, _) =
            resolve_node_lifecycle(stmt.operator_or_predicate, lifecycle_table);
        concept_terms.sort();
        concept_terms.dedup();
        units.sort();
        units.dedup();
        dimensions.sort();
        dimensions.dedup();
        retired_concepts.sort_by_key(|(c, _)| *c);
        redirect_provenance.sort();

        plane1.hyperedges.push(ProjectedStatementHyperedge {
            ku_object: ku_object.clone(),
            disclosure,
            statement_id: stmt.statement_id,
            predicate: stmt.operator_or_predicate,
            canonical_predicate,
            concept_terms,
            units,
            dimensions,
            retired_concepts,
            redirect_provenance,
        });
    }

    Ok(())
}

fn resolve_node_lifecycle(
    ccid: ConceptCcid,
    lifecycle_table: Option<&ConceptLifecycleTable>,
) -> (ConceptCcid, Option<ConceptRetireReason>) {
    let Some(table) = lifecycle_table else {
        return (ccid, None);
    };
    match table.resolve_for_authoring(ccid) {
        AuthoringConceptResolution::Canonical(c) => (c, None),
        AuthoringConceptResolution::Redirected { to, .. } => {
            let retired = match table.get(&to) {
                Some(ConceptLifecycleRecord::Retire(r)) => Some(r.reason_code),
                _ => None,
            };
            (to, retired)
        }
        AuthoringConceptResolution::Retired { ccid: c, reason } => (c, Some(reason)),
        AuthoringConceptResolution::NeedsSplitDisambiguation { parent, .. } => (parent, None),
    }
}

fn decode_assembly_manifest(
    value: &CanonicalValue,
) -> Result<FrontierAssemblyManifest, ObkgProjectionError> {
    let CanonicalValue::Map(fields) = value else {
        return Err(ObkgProjectionError::UnsupportedObjectKind);
    };
    let req = |k: u64| -> Result<&CanonicalValue, ObkgProjectionError> {
        fields
            .iter()
            .find_map(|(ck, v)| (*ck == k).then_some(v))
            .ok_or(ObkgProjectionError::UnsupportedObjectKind)
    };
    let u64_of = |v: &CanonicalValue| -> Result<u64, ObkgProjectionError> {
        match v {
            CanonicalValue::Unsigned(x) => Ok(*x),
            _ => Err(ObkgProjectionError::UnsupportedObjectKind),
        }
    };
    let bytes32_of = |v: &CanonicalValue| -> Result<[u8; 32], ObkgProjectionError> {
        let CanonicalValue::Bytes(b) = v else {
            return Err(ObkgProjectionError::UnsupportedObjectKind);
        };
        b.as_slice()
            .try_into()
            .map_err(|_| ObkgProjectionError::UnsupportedObjectKind)
    };

    let obj_ref_of = |v: &CanonicalValue| -> Result<ObjectReference, ObkgProjectionError> {
        let CanonicalValue::Map(m) = v else {
            return Err(ObkgProjectionError::UnsupportedObjectKind);
        };
        if m.len() != 2 || m[0].0 != 0 || m[1].0 != 1 {
            return Err(ObkgProjectionError::UnsupportedObjectKind);
        }
        Ok(ObjectReference::new(u64_of(&m[0].1)?, bytes32_of(&m[1].1)?))
    };

    if u64_of(req(0)?)? != ASSEMBLY_PROFILE_MAJOR || u64_of(req(1)?)? != ASSEMBLY_PROFILE_MINOR {
        return Err(ObkgProjectionError::UnsupportedObjectKind);
    }
    let lineage = AssemblyLineageId::from_bytes(bytes32_of(req(2)?)?);
    let revision = u64_of(req(3)?)?;
    let CanonicalValue::Array(src_vals) = req(4)? else {
        return Err(ObkgProjectionError::UnsupportedObjectKind);
    };
    let source_objects = src_vals
        .iter()
        .map(&obj_ref_of)
        .collect::<Result<Vec<_>, _>>()?;

    let CanonicalValue::Array(pl_vals) = req(5)? else {
        return Err(ObkgProjectionError::UnsupportedObjectKind);
    };
    let mut placements = Vec::with_capacity(pl_vals.len());
    for pl_val in pl_vals {
        let CanonicalValue::Map(pmap) = pl_val else {
            return Err(ObkgProjectionError::UnsupportedObjectKind);
        };
        let preq = |k: u64| -> Result<&CanonicalValue, ObkgProjectionError> {
            pmap.iter()
                .find_map(|(ck, v)| (*ck == k).then_some(v))
                .ok_or(ObkgProjectionError::UnsupportedObjectKind)
        };
        let placement_id = PlacementId::from_bytes(bytes32_of(preq(0)?)?);
        let receptor_definition = obj_ref_of(preq(1)?)?;
        let CanonicalValue::Map(cmap) = preq(2)? else {
            return Err(ObkgProjectionError::UnsupportedObjectKind);
        };
        let min_val = cmap
            .iter()
            .find_map(|(k, v)| (*k == 0).then_some(v))
            .ok_or(ObkgProjectionError::UnsupportedObjectKind)?;
        let minimum = u32::try_from(u64_of(min_val)?)
            .map_err(|_| ObkgProjectionError::UnsupportedObjectKind)?;
        let maximum = cmap
            .iter()
            .find_map(|(k, v)| (*k == 1).then_some(v))
            .map(u64_of)
            .transpose()?
            .map(u32::try_from)
            .transpose()
            .map_err(|_| ObkgProjectionError::UnsupportedObjectKind)?;
        let CanonicalValue::Bool(required) = preq(3)? else {
            return Err(ObkgProjectionError::UnsupportedObjectKind);
        };
        let local_context = SemanticFrameSet::from_canonical_value(preq(4)?)?;
        let resolution_policy_override = pmap
            .iter()
            .find_map(|(k, v)| (*k == 5).then_some(v))
            .map(&obj_ref_of)
            .transpose()?;
        placements.push(ReceptorPlacement {
            placement_id,
            receptor_definition,
            cardinality: ReceptorCardinality::new(minimum, maximum)?,
            required: *required,
            local_context,
            resolution_policy_override,
        });
    }

    let default_resolution_policy = obj_ref_of(req(6)?)?;
    let predecessor = fields
        .iter()
        .find_map(|(k, v)| (*k == 7).then_some(v))
        .map(&obj_ref_of)
        .transpose()?;

    let manifest = FrontierAssemblyManifest {
        lineage,
        revision,
        predecessor,
        source_objects,
        placements,
        default_resolution_policy,
    };
    if manifest.canonical_payload()? != *value {
        return Err(ObkgProjectionError::UnsupportedObjectKind);
    }
    Ok(manifest)
}

fn compute_multi_plane_commitment(
    config: &ObkgMultiPlaneConfig,
    plane1: &SemanticStatementPlane,
    plane2: &GapAndAffordancePlane,
    plane3: &ThreeLaneMappingPlane,
    plane4: &EvidenceAndFidelityOverlayPlane,
) -> [u8; 32] {
    let mut hasher = blake3::Hasher::new();
    hasher.update(b"onebrain:vnext:obkg-multi-plane-snapshot:1\0");
    hasher.update(&OBKG_MULTI_PLANE_REDUCER_VERSION.to_be_bytes());
    hasher.update(&config.source_frontier.root());
    if let Some(cp) = config.checkpoint_anchor {
        hasher.update(&[1]);
        hasher.update(cp.as_bytes());
    } else {
        hasher.update(&[0]);
    }
    hash_reference(&mut hasher, &config.resolution_policy_ref);
    hash_reference(&mut hasher, &config.fidelity_policy_ref);
    hash_reference(&mut hasher, &config.metabolic_policy_ref);
    hasher.update(&config.registry_release_root);
    hash_model_version(&mut hasher, &config.model_version);

    // Plane 1
    hasher.update(&(plane1.ku_payload_commitments.len() as u64).to_be_bytes());
    for ((kind, cid), payload_commit) in &plane1.ku_payload_commitments {
        hasher.update(&kind.to_be_bytes());
        hasher.update(cid);
        hasher.update(payload_commit);
    }
    hasher.update(&(plane1.concepts.len() as u64).to_be_bytes());
    for (ccid, node) in &plane1.concepts {
        hasher.update(ccid.as_bytes());
        hasher.update(node.canonical_ccid.as_bytes());
        if let Some(reason) = node.retired_reason {
            hasher.update(&[1, reason as u8]);
        } else {
            hasher.update(&[0]);
        }
        hasher.update(&(node.equivalence_class.len() as u64).to_be_bytes());
        for member in &node.equivalence_class {
            hasher.update(member.as_bytes());
        }
    }

    // Plane 2
    hasher.update(&(plane2.assembly_hypernodes.len() as u64).to_be_bytes());
    for (hkey, hnode) in &plane2.assembly_hypernodes {
        hasher.update(&hkey.lineage_id);
        hasher.update(&hkey.revision.to_be_bytes());
        hasher.update(&hkey.placement_id);
        hash_reference(&mut hasher, &hnode.manifest_object);
        hash_reference(&mut hasher, &hnode.receptor_definition);
        hasher.update(&hnode.cardinality.minimum.to_be_bytes());
        hasher.update(&hnode.cardinality.maximum.unwrap_or(u32::MAX).to_be_bytes());
        hasher.update(&[u8::from(hnode.required)]);
    }
    hasher.update(&(plane2.receptor_ports.len() as u64).to_be_bytes());
    for port in plane2.receptor_ports.values() {
        hash_reference(&mut hasher, &port.object);
        hasher.update(port.role.as_bytes());
        for need in &port.standing_needs {
            hasher.update(need.as_bytes());
        }
    }
    hasher.update(&(plane2.explicit_affordances.len() as u64).to_be_bytes());
    for aff in plane2.explicit_affordances.values() {
        hash_reference(&mut hasher, &aff.object);
    }
    hasher.update(&(plane2.virtual_sfs_affordances.len() as u64).to_be_bytes());
    for aff in plane2.virtual_sfs_affordances.values() {
        hash_reference(&mut hasher, &aff.object);
    }

    // Plane 3
    hasher.update(&(plane3.quarantined_proposals.len() as u64).to_be_bytes());
    for prop in plane3.quarantined_proposals.values() {
        hasher.update(prop.proposal_id.as_bytes());
        hasher.update(prop.kernel.as_bytes());
        hasher.update(&[proposal_disposition_code(prop.disposition)]);
    }
    hasher.update(&(plane3.materialized_inactive.len() as u64).to_be_bytes());
    for edge in plane3.materialized_inactive.values() {
        hasher.update(edge.kernel.as_bytes());
        hasher.update(edge.envelope.as_bytes());
    }
    hasher.update(&(plane3.adopted_active.len() as u64).to_be_bytes());
    for edge in &plane3.adopted_active {
        hasher.update(edge.kernel.as_bytes());
        hasher.update(edge.envelope.as_bytes());
        hash_projected_target(&mut hasher, edge.target);
        hasher.update(&[resolution_state_code(edge.target_resolution_state)]);
    }

    // Plane 4
    hasher.update(&(plane4.metabolic_views.len() as u64).to_be_bytes());
    for ((okey, (tkind, tcid)), view) in &plane4.metabolic_views {
        hasher.update(&okey.policy_cid);
        hasher.update(&okey.frontier_root);
        hasher.update(&okey.reducer_version.to_be_bytes());
        hasher.update(&tkind.to_be_bytes());
        hasher.update(tcid);
        hasher.update(&view.view_root);
    }
    hasher.update(&(plane4.benefit_closures.len() as u64).to_be_bytes());
    for ((okey, case_id), closure) in &plane4.benefit_closures {
        hasher.update(&okey.policy_cid);
        hasher.update(&okey.frontier_root);
        hasher.update(case_id);
        hasher.update(&(closure.state as u64).to_be_bytes());
    }
    hasher.update(&(plane4.fidelity_assessments.len() as u64).to_be_bytes());
    for ((okey, (tkind, tcid)), fid) in &plane4.fidelity_assessments {
        hasher.update(&okey.policy_cid);
        hasher.update(&okey.frontier_root);
        hasher.update(&tkind.to_be_bytes());
        hasher.update(tcid);
        hasher.update(&fid.accepted_attestation_set_root);
        hasher.update(&(fid.status as u64).to_be_bytes());
    }
    hasher.update(&(plane4.zero_use_kus.len() as u64).to_be_bytes());
    for (tkind, tcid) in &plane4.zero_use_kus {
        hasher.update(&tkind.to_be_bytes());
        hasher.update(tcid);
    }

    *hasher.finalize().as_bytes()
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

    #[test]
    fn obkg_002_five_planes_rebuild_deterministically_and_forbid_wall_clock_history() {
        use ku_core::concept_lifecycle::{ConceptRedirect, ConceptRetire};
        use ku_core::foundation::{StatementFrame, StatementQualifiers};

        let old_ccid = ConceptCcid::from_bytes([10; 16]);
        let canon_ccid = ConceptCcid::from_bytes([11; 16]);
        let retired_ccid = ConceptCcid::from_bytes([12; 16]);
        let pred_ccid = ConceptCcid::from_bytes([13; 16]);

        let lifecycle = ConceptLifecycleTable::from_records(vec![
            ConceptLifecycleRecord::Redirect(ConceptRedirect {
                deprecated_ccid: old_ccid,
                canonical_ccid: canon_ccid,
                since_release_seq: 1,
                provenance_ref: "wd:Q10->wd:Q11".to_string(),
            }),
            ConceptLifecycleRecord::Retire(ConceptRetire {
                retired_ccid,
                reason_code: ConceptRetireReason::PolicyObsoleted,
                since_release_seq: 2,
            }),
        ])
        .unwrap();

        let sfs = SemanticFrameSet {
            statements: vec![StatementFrame {
                statement_id: StatementId(0),
                operator_or_predicate: pred_ccid,
                arguments: vec![TermRef::Concept(old_ccid), TermRef::Concept(retired_ccid)],
                constraints: Vec::new(),
                qualifiers: StatementQualifiers::default(),
            }],
        };
        let (sfs_bytes, _) = sfs
            .to_knowledge_object(DisclosureClass::Public)
            .unwrap()
            .encode(ResourceProfile::ObjectV1)
            .unwrap();
        let validated_sfs = decode_knowledge_object(
            &sfs_bytes,
            ResourceProfile::ObjectV1,
            &[KnownObjectKind::new(SEMANTIC_KERNEL_OBJECT_KIND, 1)],
            &[],
        )
        .unwrap();

        let manifest = FrontierAssemblyManifest {
            lineage: AssemblyLineageId::from_bytes([30; 32]),
            revision: 0,
            predecessor: None,
            source_objects: vec![ObjectReference::new(
                SEMANTIC_KERNEL_OBJECT_KIND.0,
                validated_sfs.cid().into_bytes(),
            )],
            placements: vec![ReceptorPlacement {
                placement_id: PlacementId::from_bytes([31; 32]),
                receptor_definition: reference(RECEPTOR_DEFINITION_KIND.0, 60),
                cardinality: ReceptorCardinality::new(1, Some(2)).unwrap(),
                required: true,
                local_context: SemanticFrameSet {
                    statements: Vec::new(),
                },
                resolution_policy_override: None,
            }],
            default_resolution_policy: reference(0, 70),
        };
        let (man_bytes, _) = manifest
            .to_knowledge_object(DisclosureClass::Public)
            .unwrap()
            .encode(ResourceProfile::ObjectV1)
            .unwrap();
        let validated_manifest = decode_knowledge_object(
            &man_bytes,
            ResourceProfile::ObjectV1,
            &[KnownObjectKind::new(ASSEMBLY_MANIFEST_KIND, 1)],
            &[],
        )
        .unwrap();

        let prop = proposal();
        let k1 = prop.kernel_id().unwrap();
        let k2 = MappingKernelCid::from_bytes([91; 32]);
        let k3_inactive = MappingKernelCid::from_bytes([92; 32]);
        let materialized = vec![
            MappingViewRecord {
                kernel: k1,
                envelope: ObjectCid::from_bytes([81; 32]),
                disclosure: DisclosureClass::LocalOnly,
            },
            MappingViewRecord {
                kernel: k2,
                envelope: ObjectCid::from_bytes([82; 32]),
                disclosure: DisclosureClass::Public,
            },
            MappingViewRecord {
                kernel: k3_inactive,
                envelope: ObjectCid::from_bytes([83; 32]),
                disclosure: DisclosureClass::Public,
            },
        ];

        let target = ResolutionTarget {
            assembly_lineage: AssemblyLineageId::from_bytes([30; 32]),
            assembly_revision: validated_manifest.cid(),
            placement: PlacementId::from_bytes([31; 32]),
        };
        let concurrent_res = ReceptorResolutionProjection {
            receptor_definition: reference(RECEPTOR_DEFINITION_KIND.0, 60),
            view: ResolutionView {
                target,
                state: ResolutionState::Concurrent,
                resolution_policy: reference(0, 70),
                assessed_frontier: [71; 32],
                reducer_version: RESOLUTION_REDUCER_VERSION,
                branches: vec![
                    ResolutionBranch {
                        tip_event: EventCid::from_bytes([72; 32]),
                        state: ResolutionState::SatisfiedRelative,
                        active_bindings: vec![k1],
                    },
                    ResolutionBranch {
                        tip_event: EventCid::from_bytes([73; 32]),
                        state: ResolutionState::SatisfiedRelative,
                        active_bindings: vec![k2],
                    },
                ],
                unresolved_events: Vec::new(),
            },
        };

        let frontier_a =
            ObkgSourceFrontier::new(SelectorCid::from_bytes([1; 32]), [2; 32], []).unwrap();
        let frontier_b =
            ObkgSourceFrontier::new(SelectorCid::from_bytes([1; 32]), [3; 32], []).unwrap();

        let mp_config_a = ObkgMultiPlaneConfig {
            source_frontier: frontier_a.clone(),
            checkpoint_anchor: None,
            resolution_policy_ref: reference(0, 70),
            fidelity_policy_ref: reference(0, 74),
            metabolic_policy_ref: reference(0, 75),
            registry_release_root: [0xaa; 32],
            model_version: ObkgModelVersion::DeterministicNoModel,
            current_evaluation: 5,
        };
        let snap_a = ObkgMultiPlaneProjection::project(
            mp_config_a.clone(),
            ObkgMultiPlaneSources {
                objects: &[validated_sfs.clone()],
                authorized_vault_kus: &[],
                lifecycle_table: Some(&lifecycle),
                standing_needs: &[],
                resolutions: &[],
                materialized_mappings: &[materialized[2].clone()],
                candidate_proposals: std::slice::from_ref(&prop),
                metabolic_views: &[],
                benefit_closures: &[],
                fidelity_assessments: &[],
                exercises: &[],
            },
        )
        .unwrap();

        // Plane 1: retired concept annotated, redirect tracked, zero-use KU preserved in Plane 4!
        assert!(snap_a.plane1_semantic.concepts[&retired_ccid]
            .annotations
            .contains(&RETIRED_CONCEPT_ANNOTATION));
        assert_eq!(
            snap_a.plane1_semantic.concepts[&old_ccid].canonical_ccid,
            canon_ccid
        );
        assert_eq!(snap_a.plane4_evidence_fidelity.zero_use_kus.len(), 1);
        assert_eq!(
            snap_a.plane2_gaps_affordances.virtual_sfs_affordances.len(),
            1
        );

        // Plane 3 + Plane 2 at Frontier B with AssemblyManifest and Concurrent active bindings
        let mp_config_b = ObkgMultiPlaneConfig {
            source_frontier: frontier_b.clone(),
            ..mp_config_a.clone()
        };
        let objects_b_fwd = [validated_sfs.clone(), validated_manifest.clone()];
        let objects_b_rev = [validated_manifest, validated_sfs];
        let snap_b = ObkgMultiPlaneProjection::project(
            mp_config_b.clone(),
            ObkgMultiPlaneSources {
                objects: &objects_b_fwd,
                authorized_vault_kus: &[],
                lifecycle_table: Some(&lifecycle),
                standing_needs: &[],
                resolutions: std::slice::from_ref(&concurrent_res),
                materialized_mappings: &materialized,
                candidate_proposals: std::slice::from_ref(&prop),
                metabolic_views: &[],
                benefit_closures: &[],
                fidelity_assessments: &[],
                exercises: &[],
            },
        )
        .unwrap();
        let snap_b_rebuilt = ObkgMultiPlaneProjection::project(
            mp_config_b,
            ObkgMultiPlaneSources {
                objects: &objects_b_rev,
                authorized_vault_kus: &[],
                lifecycle_table: Some(&lifecycle),
                standing_needs: &[],
                resolutions: &[concurrent_res],
                materialized_mappings: &materialized,
                candidate_proposals: &[prop],
                metabolic_views: &[],
                benefit_closures: &[],
                fidelity_assessments: &[],
                exercises: &[],
            },
        )
        .unwrap();

        // FW-09: Pure functional rebuildability yields identical SnapshotCommitmentCid!
        assert_eq!(
            snap_b.snapshot_commitment_cid(),
            snap_b_rebuilt.snapshot_commitment_cid()
        );
        assert_eq!(snap_b.plane3_mappings.adopted_active.len(), 2);
        assert!(snap_b
            .plane3_mappings
            .adopted_active
            .iter()
            .all(|e| e.is_active_edge()));
        assert!(!snap_b.plane3_mappings.is_kernel_active(k3_inactive));

        // Plane 5: Frontier lookup, diff, and NEG-P7-001 wall-clock rejection
        let mut store = ObkgMultiPlaneProjection::default();
        store.record_snapshot(snap_a.clone());
        store.record_snapshot(snap_b.clone());
        assert_eq!(
            store
                .at_frontier(&frontier_a)
                .unwrap()
                .snapshot_commitment_cid(),
            snap_a.snapshot_commitment_cid()
        );
        assert_eq!(
            store.at_wall_clock_timestamp(1_700_000_000).unwrap_err(),
            ObkgProjectionError::WallClockHistoryForbidden
        );
        assert_eq!(
            store
                .inspect_historical(&ObkgHistoricalSelector::AtWallClockTimestamp(1_700_000_000))
                .unwrap_err(),
            ObkgProjectionError::WallClockHistoryForbidden
        );

        let diff = ObkgMultiPlaneProjection::diff_frontiers(&snap_a, &snap_b);
        assert_eq!(diff.added_assembly_hypernodes.len(), 1);
        assert_eq!(diff.newly_adopted_edges.len(), 2);

        store.clear_disposable_caches();
        assert_eq!(
            store.at_frontier(&frontier_b).unwrap_err(),
            ObkgProjectionError::FrontierSnapshotNotFound
        );
    }
}

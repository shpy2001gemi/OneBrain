//! Frontier-relative receptor resolution over signed, causally ordered events.

use std::collections::{BTreeMap, BTreeSet, HashMap, HashSet};

use super::assembly::{AssemblyLineageId, PlacementId};
use super::canonical::{canonicalize_set_by_key, CanonicalValue, ResourceProfile};
use super::content_id::{EventCid, MappingKernelCid, ObjectCid};
use super::event::{EventType, ValidatedKnowledgeEvent};
use super::object::{
    DisclosureClass, KnowledgeObjectEnvelope, ObjectError, ObjectKind, ObjectReference,
    ObjectSemantics, SchemaVersion, ValidatedKnowledgeObject,
};
use super::schema_registry::{
    EVENT_TYPE_RECEPTOR_RESOLUTION, OBJECT_KIND_RECEPTOR_RESOLUTION_ACTION,
};

pub const RECEPTOR_RESOLUTION_EVENT_TYPE: EventType = EventType(EVENT_TYPE_RECEPTOR_RESOLUTION);
pub const RECEPTOR_RESOLUTION_ACTION_KIND: ObjectKind =
    ObjectKind(OBJECT_KIND_RECEPTOR_RESOLUTION_ACTION);
pub const RESOLUTION_PROFILE_MAJOR: u64 = 1;
pub const RESOLUTION_PROFILE_MINOR: u64 = 0;
pub const RESOLUTION_REDUCER_VERSION: u64 = 1;
pub const MAX_RESOLUTION_EVIDENCE: usize = 4_096;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct ResolutionTarget {
    pub assembly_lineage: AssemblyLineageId,
    pub assembly_revision: ObjectCid,
    pub placement: PlacementId,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ResolutionAction {
    AdoptBinding { mapping: MappingKernelCid },
    RevokeAdoption { adoption_event: EventCid },
    Waive,
    Reopen,
    Defer,
}

impl ResolutionAction {
    const fn code(self) -> u64 {
        match self {
            Self::AdoptBinding { .. } => 0,
            Self::RevokeAdoption { .. } => 1,
            Self::Waive => 2,
            Self::Reopen => 3,
            Self::Defer => 4,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ResolutionActionPayload {
    pub target: ResolutionTarget,
    pub action: ResolutionAction,
    pub receptor_claim: Option<ObjectReference>,
    pub acceptance_evidence: Vec<ObjectReference>,
    pub resolution_policy: ObjectReference,
    pub observed_frontier: [u8; 32],
}

impl ResolutionActionPayload {
    pub fn canonical_payload(&self) -> Result<CanonicalValue, ResolutionError> {
        if self.acceptance_evidence.len() > MAX_RESOLUTION_EVIDENCE {
            return Err(ResolutionError::Limit);
        }
        let evidence = self
            .acceptance_evidence
            .iter()
            .map(ObjectReference::to_value)
            .collect::<Vec<_>>();
        let evidence = canonicalize_set_by_key(
            evidence
                .into_iter()
                .map(|value| (value.clone(), value))
                .collect(),
            ResourceProfile::ObjectV1,
        )?;
        let mut fields = vec![
            (0, CanonicalValue::Unsigned(RESOLUTION_PROFILE_MAJOR)),
            (1, CanonicalValue::Unsigned(RESOLUTION_PROFILE_MINOR)),
            (
                2,
                CanonicalValue::Bytes(self.target.assembly_lineage.as_bytes().to_vec()),
            ),
            (
                3,
                CanonicalValue::Bytes(self.target.assembly_revision.as_bytes().to_vec()),
            ),
            (
                4,
                CanonicalValue::Bytes(self.target.placement.as_bytes().to_vec()),
            ),
            (5, CanonicalValue::Unsigned(self.action.code())),
            (8, CanonicalValue::Array(evidence)),
            (9, self.resolution_policy.to_value()),
            (10, CanonicalValue::Bytes(self.observed_frontier.to_vec())),
        ];
        match self.action {
            ResolutionAction::AdoptBinding { mapping } => {
                fields.push((6, CanonicalValue::Bytes(mapping.as_bytes().to_vec())));
            }
            ResolutionAction::RevokeAdoption { adoption_event } => {
                fields.push((7, CanonicalValue::Bytes(adoption_event.as_bytes().to_vec())));
            }
            ResolutionAction::Waive | ResolutionAction::Reopen | ResolutionAction::Defer => {}
        }
        if let Some(claim) = &self.receptor_claim {
            fields.push((11, claim.to_value()));
        }
        fields.sort_by_key(|(key, _)| *key);
        Ok(CanonicalValue::Map(fields))
    }

    pub fn to_knowledge_object(
        &self,
        disclosure: DisclosureClass,
    ) -> Result<KnowledgeObjectEnvelope, ResolutionError> {
        let mut object = KnowledgeObjectEnvelope::new(
            RECEPTOR_RESOLUTION_ACTION_KIND,
            SchemaVersion::new(RESOLUTION_PROFILE_MAJOR, RESOLUTION_PROFILE_MINOR),
            disclosure,
            self.canonical_payload()?,
        );
        object.references = self.acceptance_evidence.clone();
        object.references.push(self.resolution_policy.clone());
        if let Some(claim) = &self.receptor_claim {
            object.references.push(claim.clone());
        }
        Ok(object)
    }

    fn from_object(object: &ValidatedKnowledgeObject) -> Result<Self, ResolutionError> {
        let envelope = match object.semantics() {
            ObjectSemantics::Known(envelope)
                if envelope.kind == RECEPTOR_RESOLUTION_ACTION_KIND
                    && envelope.kind_version.major == RESOLUTION_PROFILE_MAJOR =>
            {
                envelope
            }
            _ => return Err(ResolutionError::WrongPayloadKind),
        };
        let map = value_map(&envelope.payload, "resolution.payload")?;
        if value_unsigned(map, 0, "resolution.major")? != RESOLUTION_PROFILE_MAJOR {
            return Err(ResolutionError::WrongPayloadVersion);
        }
        let target = ResolutionTarget {
            assembly_lineage: AssemblyLineageId::from_bytes(value_bytes32(
                map,
                2,
                "resolution.lineage",
            )?),
            assembly_revision: ObjectCid::from_bytes(value_bytes32(map, 3, "resolution.revision")?),
            placement: PlacementId::from_bytes(value_bytes32(map, 4, "resolution.placement")?),
        };
        let action = match value_unsigned(map, 5, "resolution.action")? {
            0 => ResolutionAction::AdoptBinding {
                mapping: MappingKernelCid::from_bytes(value_bytes32(map, 6, "resolution.mapping")?),
            },
            1 => ResolutionAction::RevokeAdoption {
                adoption_event: EventCid::from_bytes(value_bytes32(
                    map,
                    7,
                    "resolution.adoption_event",
                )?),
            },
            2 => ResolutionAction::Waive,
            3 => ResolutionAction::Reopen,
            4 => ResolutionAction::Defer,
            _ => return Err(ResolutionError::InvalidField("resolution.action")),
        };
        let evidence = value_array(map, 8, "resolution.evidence")?
            .iter()
            .map(ObjectReference::from_value)
            .collect::<Result<Vec<_>, _>>()?;
        let resolution_policy =
            ObjectReference::from_value(value_required(map, 9, "resolution.policy")?)?;
        let receptor_claim = value_optional(map, 11)
            .map(ObjectReference::from_value)
            .transpose()?;
        let payload = Self {
            target,
            action,
            receptor_claim,
            acceptance_evidence: evidence,
            resolution_policy,
            observed_frontier: value_bytes32(map, 10, "resolution.frontier")?,
        };
        // Rebuilding also checks set uniqueness/order and action field shape.
        if payload.canonical_payload()? != envelope.payload {
            return Err(ResolutionError::NonCanonicalPayload);
        }
        let (_, expected_cid) = payload
            .to_knowledge_object(envelope.disclosure)?
            .encode(ResourceProfile::ObjectV1)?;
        if expected_cid != object.cid() {
            return Err(ResolutionError::NonCanonicalPayload);
        }
        Ok(payload)
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ValidatedResolutionEvent {
    event_cid: EventCid,
    action_object_cid: ObjectCid,
    payload: ResolutionActionPayload,
    causal_parents: Vec<EventCid>,
}

impl ValidatedResolutionEvent {
    pub fn bind(
        event: &ValidatedKnowledgeEvent,
        action_object: &ValidatedKnowledgeObject,
    ) -> Result<Self, ResolutionError> {
        if event.signed.event.event_type != RECEPTOR_RESOLUTION_EVENT_TYPE {
            return Err(ResolutionError::WrongEventType);
        }
        let expected = ObjectReference::new(0, action_object.cid().into_bytes());
        if event.signed.event.payload_refs != [expected] {
            return Err(ResolutionError::PayloadReferenceMismatch);
        }
        if event.signed.event.disclosure != action_object.disclosure() {
            return Err(ResolutionError::DisclosureMismatch);
        }
        Ok(Self {
            event_cid: event.cid(),
            action_object_cid: action_object.cid(),
            payload: ResolutionActionPayload::from_object(action_object)?,
            causal_parents: event.signed.event.causal_parents.clone(),
        })
    }

    pub const fn event_cid(&self) -> EventCid {
        self.event_cid
    }

    pub const fn action_object_cid(&self) -> ObjectCid {
        self.action_object_cid
    }

    pub const fn payload(&self) -> &ResolutionActionPayload {
        &self.payload
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ResolutionAuthority {
    Authorized,
    Unauthorized,
    Unresolved,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum BindingAcceptance {
    Rejected,
    Partial,
    Satisfied,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AssessedResolutionEvent {
    event: ValidatedResolutionEvent,
    authority: ResolutionAuthority,
    acceptance: Option<BindingAcceptance>,
}

pub trait MaterializedMappingLookup {
    fn contains_materialized_mapping(&self, mapping: MappingKernelCid) -> Result<bool, String>;
}

pub fn assess_resolution_event<L: MaterializedMappingLookup>(
    event: ValidatedResolutionEvent,
    authority: ResolutionAuthority,
    acceptance: Option<BindingAcceptance>,
    mappings: &L,
) -> Result<AssessedResolutionEvent, ResolutionError> {
    match event.payload.action {
        ResolutionAction::AdoptBinding { mapping }
            if authority == ResolutionAuthority::Authorized =>
        {
            if acceptance.is_none() {
                return Err(ResolutionError::MissingAcceptanceAssessment);
            }
            if !mappings
                .contains_materialized_mapping(mapping)
                .map_err(ResolutionError::MappingLookup)?
            {
                return Err(ResolutionError::MappingNotMaterialized);
            }
        }
        ResolutionAction::AdoptBinding { .. } => {}
        _ if acceptance.is_some() => return Err(ResolutionError::UnexpectedAcceptanceAssessment),
        _ => {}
    }
    Ok(AssessedResolutionEvent {
        event,
        authority,
        acceptance,
    })
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ResolutionApplyOutcome {
    Added,
    Reassessed,
    ExactReplay,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum ResolutionState {
    Open,
    PartiallySatisfied,
    SatisfiedRelative,
    Waived,
    Deferred,
    Concurrent,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ResolutionBranch {
    pub tip_event: EventCid,
    pub state: ResolutionState,
    pub active_bindings: Vec<MappingKernelCid>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ResolutionView {
    pub target: ResolutionTarget,
    pub state: ResolutionState,
    pub resolution_policy: ObjectReference,
    pub assessed_frontier: [u8; 32],
    pub reducer_version: u64,
    pub branches: Vec<ResolutionBranch>,
    pub unresolved_events: Vec<EventCid>,
}

pub struct ResolutionReducer {
    target: ResolutionTarget,
    policy: ObjectReference,
    assessed_frontier: [u8; 32],
    records: BTreeMap<[u8; 32], AssessedResolutionEvent>,
}

impl ResolutionReducer {
    pub fn new(
        target: ResolutionTarget,
        policy: ObjectReference,
        assessed_frontier: [u8; 32],
    ) -> Self {
        Self {
            target,
            policy,
            assessed_frontier,
            records: BTreeMap::new(),
        }
    }

    pub fn apply(
        &mut self,
        record: AssessedResolutionEvent,
    ) -> Result<ResolutionApplyOutcome, ResolutionError> {
        if record.event.payload.target != self.target {
            return Err(ResolutionError::TargetMismatch);
        }
        if record.event.payload.resolution_policy != self.policy {
            return Err(ResolutionError::PolicyMismatch);
        }
        if matches!(
            record.event.payload.action,
            ResolutionAction::AdoptBinding { .. }
        ) && record.authority == ResolutionAuthority::Authorized
            && record.acceptance.is_none()
        {
            return Err(ResolutionError::MissingAcceptanceAssessment);
        }
        let key = record.event.event_cid.into_bytes();
        match self.records.get(&key) {
            Some(existing) if existing == &record => Ok(ResolutionApplyOutcome::ExactReplay),
            Some(_) => {
                self.records.insert(key, record);
                Ok(ResolutionApplyOutcome::Reassessed)
            }
            None => {
                self.records.insert(key, record);
                Ok(ResolutionApplyOutcome::Added)
            }
        }
    }

    pub fn set_assessed_frontier(&mut self, frontier: [u8; 32]) {
        self.assessed_frontier = frontier;
    }

    pub fn view(&self) -> Result<ResolutionView, ResolutionError> {
        let active: BTreeMap<_, _> = self
            .records
            .iter()
            .filter(|(_, record)| record.authority == ResolutionAuthority::Authorized)
            .map(|(cid, record)| (*cid, record))
            .collect();
        let mut parent_ids = BTreeSet::new();
        for record in active.values() {
            for parent in &record.event.causal_parents {
                if active.contains_key(parent.as_bytes()) {
                    parent_ids.insert(*parent.as_bytes());
                }
            }
        }
        let tips = active
            .keys()
            .filter(|cid| !parent_ids.contains(*cid))
            .copied()
            .collect::<Vec<_>>();
        let mut memo = HashMap::new();
        let mut branches = Vec::new();
        for tip in tips {
            let states = self.branch_states(tip, &active, &mut memo, &mut HashSet::new())?;
            for state in states {
                branches.push(ResolutionBranch {
                    tip_event: EventCid::from_bytes(tip),
                    state: state.resolution_state(),
                    active_bindings: state
                        .adoptions
                        .values()
                        .map(|binding| MappingKernelCid::from_bytes(binding.mapping))
                        .collect(),
                });
            }
        }
        branches.sort_by(|left, right| {
            left.tip_event
                .as_bytes()
                .cmp(right.tip_event.as_bytes())
                .then_with(|| state_code(left.state).cmp(&state_code(right.state)))
        });
        let state = if branches.is_empty() {
            ResolutionState::Open
        } else {
            let distinct = branches
                .iter()
                .map(|branch| (branch.state, binding_bytes(&branch.active_bindings)))
                .collect::<BTreeSet<_>>();
            if distinct.len() > 1 {
                ResolutionState::Concurrent
            } else {
                branches[0].state
            }
        };
        let unresolved_events = self
            .records
            .values()
            .filter(|record| record.authority == ResolutionAuthority::Unresolved)
            .map(|record| record.event.event_cid)
            .collect();
        Ok(ResolutionView {
            target: self.target,
            state,
            resolution_policy: self.policy.clone(),
            assessed_frontier: self.assessed_frontier,
            reducer_version: RESOLUTION_REDUCER_VERSION,
            branches,
            unresolved_events,
        })
    }

    fn branch_states(
        &self,
        event_id: [u8; 32],
        active: &BTreeMap<[u8; 32], &AssessedResolutionEvent>,
        memo: &mut HashMap<[u8; 32], BTreeSet<BranchState>>,
        visiting: &mut HashSet<[u8; 32]>,
    ) -> Result<BTreeSet<BranchState>, ResolutionError> {
        if let Some(cached) = memo.get(&event_id) {
            return Ok(cached.clone());
        }
        if !visiting.insert(event_id) {
            return Err(ResolutionError::CausalCycle);
        }
        let record = active[&event_id];
        let mut parents = BTreeSet::new();
        for parent in &record.event.causal_parents {
            if active.contains_key(parent.as_bytes()) {
                parents.extend(self.branch_states(*parent.as_bytes(), active, memo, visiting)?);
            }
        }
        if parents.is_empty() {
            parents.insert(BranchState::default());
        }
        let states: BTreeSet<BranchState> = parents
            .into_iter()
            .map(|state| state.apply(event_id, record))
            .collect();
        visiting.remove(&event_id);
        memo.insert(event_id, states.clone());
        Ok(states)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
enum BranchMode {
    Normal,
    Waived,
    Deferred,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
struct ActiveBinding {
    mapping: [u8; 32],
    acceptance: BindingAcceptance,
}

#[derive(Clone, Debug, Default, PartialEq, Eq, PartialOrd, Ord)]
struct BranchState {
    mode: BranchMode,
    adoptions: BTreeMap<[u8; 32], ActiveBinding>,
}

impl Default for BranchMode {
    fn default() -> Self {
        Self::Normal
    }
}

impl BranchState {
    fn apply(mut self, event_id: [u8; 32], record: &AssessedResolutionEvent) -> Self {
        match record.event.payload.action {
            ResolutionAction::AdoptBinding { mapping } => {
                self.mode = BranchMode::Normal;
                self.adoptions.insert(
                    event_id,
                    ActiveBinding {
                        mapping: mapping.into_bytes(),
                        acceptance: record
                            .acceptance
                            .expect("authorized adoption was validated before reduction"),
                    },
                );
            }
            ResolutionAction::RevokeAdoption { adoption_event } => {
                self.adoptions.remove(adoption_event.as_bytes());
            }
            ResolutionAction::Waive => self.mode = BranchMode::Waived,
            ResolutionAction::Reopen => {
                self.mode = BranchMode::Normal;
                self.adoptions.clear();
            }
            ResolutionAction::Defer => self.mode = BranchMode::Deferred,
        }
        self
    }

    fn resolution_state(&self) -> ResolutionState {
        match self.mode {
            BranchMode::Waived => ResolutionState::Waived,
            BranchMode::Deferred => ResolutionState::Deferred,
            BranchMode::Normal => {
                if self
                    .adoptions
                    .values()
                    .any(|binding| binding.acceptance == BindingAcceptance::Satisfied)
                {
                    ResolutionState::SatisfiedRelative
                } else if self
                    .adoptions
                    .values()
                    .any(|binding| binding.acceptance == BindingAcceptance::Partial)
                {
                    ResolutionState::PartiallySatisfied
                } else {
                    ResolutionState::Open
                }
            }
        }
    }
}

fn state_code(state: ResolutionState) -> u8 {
    match state {
        ResolutionState::Open => 0,
        ResolutionState::PartiallySatisfied => 1,
        ResolutionState::SatisfiedRelative => 2,
        ResolutionState::Waived => 3,
        ResolutionState::Deferred => 4,
        ResolutionState::Concurrent => 5,
    }
}

fn binding_bytes(bindings: &[MappingKernelCid]) -> Vec<[u8; 32]> {
    bindings
        .iter()
        .map(|binding| binding.into_bytes())
        .collect()
}

fn value_map<'a>(
    value: &'a CanonicalValue,
    field: &'static str,
) -> Result<&'a [(u64, CanonicalValue)], ResolutionError> {
    match value {
        CanonicalValue::Map(map) => Ok(map),
        _ => Err(ResolutionError::InvalidField(field)),
    }
}

fn value_required<'a>(
    map: &'a [(u64, CanonicalValue)],
    key: u64,
    field: &'static str,
) -> Result<&'a CanonicalValue, ResolutionError> {
    value_optional(map, key).ok_or(ResolutionError::InvalidField(field))
}

fn value_optional(map: &[(u64, CanonicalValue)], key: u64) -> Option<&CanonicalValue> {
    map.iter()
        .find_map(|(candidate, value)| (*candidate == key).then_some(value))
}

fn value_unsigned(
    map: &[(u64, CanonicalValue)],
    key: u64,
    field: &'static str,
) -> Result<u64, ResolutionError> {
    match value_required(map, key, field)? {
        CanonicalValue::Unsigned(value) => Ok(*value),
        _ => Err(ResolutionError::InvalidField(field)),
    }
}

fn value_bytes32(
    map: &[(u64, CanonicalValue)],
    key: u64,
    field: &'static str,
) -> Result<[u8; 32], ResolutionError> {
    match value_required(map, key, field)? {
        CanonicalValue::Bytes(bytes) if bytes.len() == 32 => {
            let mut output = [0; 32];
            output.copy_from_slice(bytes);
            Ok(output)
        }
        _ => Err(ResolutionError::InvalidField(field)),
    }
}

fn value_array<'a>(
    map: &'a [(u64, CanonicalValue)],
    key: u64,
    field: &'static str,
) -> Result<&'a [CanonicalValue], ResolutionError> {
    match value_required(map, key, field)? {
        CanonicalValue::Array(values) => Ok(values),
        _ => Err(ResolutionError::InvalidField(field)),
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ResolutionError {
    Canonical(super::canonical::CanonicalError),
    Object(ObjectError),
    InvalidField(&'static str),
    Limit,
    WrongPayloadKind,
    WrongPayloadVersion,
    NonCanonicalPayload,
    WrongEventType,
    PayloadReferenceMismatch,
    DisclosureMismatch,
    TargetMismatch,
    PolicyMismatch,
    MissingAcceptanceAssessment,
    UnexpectedAcceptanceAssessment,
    MappingNotMaterialized,
    MappingLookup(String),
    CausalCycle,
}

impl From<super::canonical::CanonicalError> for ResolutionError {
    fn from(error: super::canonical::CanonicalError) -> Self {
        Self::Canonical(error)
    }
}

impl From<ObjectError> for ResolutionError {
    fn from(error: ObjectError) -> Self {
        Self::Object(error)
    }
}

#[cfg(test)]
mod tests {
    use ed25519_dalek::SigningKey;

    use super::*;
    use crate::foundation::{
        decode_feed_inception, decode_knowledge_event, decode_knowledge_object, FeedInception,
        KnowledgeEventEnvelope, KnownObjectKind, NamespaceCommitment, SignedFeedInception,
    };

    fn reference(byte: u8) -> ObjectReference {
        ObjectReference::new(0, [byte; 32])
    }

    fn target(byte: u8) -> ResolutionTarget {
        ResolutionTarget {
            assembly_lineage: AssemblyLineageId::from_bytes([byte; 32]),
            assembly_revision: ObjectCid::from_bytes([byte + 1; 32]),
            placement: PlacementId::from_bytes([byte + 2; 32]),
        }
    }

    fn author() -> (SigningKey, crate::foundation::ValidatedFeedInception) {
        let key = SigningKey::from_bytes(&[7; 32]);
        let inception = FeedInception::new(
            *key.verifying_key().as_bytes(),
            NamespaceCommitment::derive(b"resolution-test", [8; 32]).unwrap(),
            0,
            crate::foundation::DeviceId::from_bytes([9; 32]),
        );
        let signed: SignedFeedInception = inception.sign(&key).unwrap();
        (
            key,
            decode_feed_inception(&signed.encode().unwrap()).unwrap(),
        )
    }

    fn resolution_event(
        sequence: u64,
        id: u8,
        action: ResolutionAction,
        parents: Vec<EventCid>,
    ) -> ValidatedResolutionEvent {
        let payload = ResolutionActionPayload {
            target: target(1),
            action,
            receptor_claim: Some(reference(20)),
            acceptance_evidence: vec![reference(21)],
            resolution_policy: reference(22),
            observed_frontier: [23; 32],
        };
        let object = payload
            .to_knowledge_object(DisclosureClass::Public)
            .unwrap();
        let (object_bytes, object_cid) = object.encode(ResourceProfile::ObjectV1).unwrap();
        let object = decode_knowledge_object(
            &object_bytes,
            ResourceProfile::ObjectV1,
            &[KnownObjectKind::new(
                RECEPTOR_RESOLUTION_ACTION_KIND,
                RESOLUTION_PROFILE_MAJOR,
            )],
            &[],
        )
        .unwrap();
        let (key, author) = author();
        let mut event = KnowledgeEventEnvelope::new(
            RECEPTOR_RESOLUTION_EVENT_TYPE,
            author.feed_id,
            sequence,
            DisclosureClass::Public,
            [id; 32],
        );
        event.payload_refs = vec![ObjectReference::new(0, object_cid.into_bytes())];
        event.causal_parents = parents;
        let event_bytes = event.sign(&author, &key).unwrap().encode().unwrap().0;
        let event =
            decode_knowledge_event(&event_bytes, &author, &[RECEPTOR_RESOLUTION_EVENT_TYPE])
                .unwrap();
        ValidatedResolutionEvent::bind(&event, &object).unwrap()
    }

    fn assessed(
        event: ValidatedResolutionEvent,
        authority: ResolutionAuthority,
        acceptance: Option<BindingAcceptance>,
    ) -> AssessedResolutionEvent {
        struct Lookup;
        impl MaterializedMappingLookup for Lookup {
            fn contains_materialized_mapping(
                &self,
                _mapping: MappingKernelCid,
            ) -> Result<bool, String> {
                Ok(true)
            }
        }
        assess_resolution_event(event, authority, acceptance, &Lookup).unwrap()
    }

    fn reducer() -> ResolutionReducer {
        ResolutionReducer::new(target(1), reference(22), [23; 32])
    }

    #[test]
    fn signed_event_binds_exact_action_object_and_disclosure() {
        let event = resolution_event(
            0,
            1,
            ResolutionAction::AdoptBinding {
                mapping: MappingKernelCid::from_bytes([30; 32]),
            },
            Vec::new(),
        );
        assert_eq!(event.payload().target, target(1));
        assert_eq!(event.payload().resolution_policy, reference(22));
    }

    #[test]
    fn qa006_unauthorized_and_unresolved_events_do_not_change_authoritative_state() {
        let mut reducer = reducer();
        let event = resolution_event(0, 1, ResolutionAction::Waive, Vec::new());
        reducer
            .apply(assessed(event, ResolutionAuthority::Unauthorized, None))
            .unwrap();
        assert_eq!(reducer.view().unwrap().state, ResolutionState::Open);

        let pending = resolution_event(1, 2, ResolutionAction::Defer, Vec::new());
        reducer
            .apply(assessed(pending, ResolutionAuthority::Unresolved, None))
            .unwrap();
        let view = reducer.view().unwrap();
        assert_eq!(view.state, ResolutionState::Open);
        assert_eq!(view.unresolved_events.len(), 1);
    }

    #[test]
    fn adoption_state_is_relative_to_acceptance_assessment() {
        let mapping = MappingKernelCid::from_bytes([30; 32]);
        let partial =
            resolution_event(0, 1, ResolutionAction::AdoptBinding { mapping }, Vec::new());
        let mut partial_reducer = reducer();
        partial_reducer
            .apply(assessed(
                partial,
                ResolutionAuthority::Authorized,
                Some(BindingAcceptance::Partial),
            ))
            .unwrap();
        assert_eq!(
            partial_reducer.view().unwrap().state,
            ResolutionState::PartiallySatisfied
        );

        let satisfied =
            resolution_event(0, 2, ResolutionAction::AdoptBinding { mapping }, Vec::new());
        let mut satisfied_reducer = reducer();
        let record = assessed(
            satisfied,
            ResolutionAuthority::Authorized,
            Some(BindingAcceptance::Satisfied),
        );
        assert_eq!(
            satisfied_reducer.apply(record.clone()).unwrap(),
            ResolutionApplyOutcome::Added
        );
        assert_eq!(
            satisfied_reducer.apply(record).unwrap(),
            ResolutionApplyOutcome::ExactReplay
        );
        assert_eq!(
            satisfied_reducer.view().unwrap().state,
            ResolutionState::SatisfiedRelative
        );
    }

    #[test]
    fn concurrent_adopt_and_reopen_preserve_both_branches() {
        let base = resolution_event(0, 1, ResolutionAction::Defer, Vec::new());
        let base_id = base.event_cid();
        let adopt = resolution_event(
            1,
            2,
            ResolutionAction::AdoptBinding {
                mapping: MappingKernelCid::from_bytes([30; 32]),
            },
            vec![base_id],
        );
        let reopen = resolution_event(2, 3, ResolutionAction::Reopen, vec![base_id]);
        let mut reducer = reducer();
        reducer
            .apply(assessed(base, ResolutionAuthority::Authorized, None))
            .unwrap();
        reducer
            .apply(assessed(
                adopt,
                ResolutionAuthority::Authorized,
                Some(BindingAcceptance::Satisfied),
            ))
            .unwrap();
        reducer
            .apply(assessed(reopen, ResolutionAuthority::Authorized, None))
            .unwrap();
        let view = reducer.view().unwrap();
        assert_eq!(view.state, ResolutionState::Concurrent);
        assert_eq!(view.branches.len(), 2);
        assert!(view
            .branches
            .iter()
            .any(|branch| branch.state == ResolutionState::SatisfiedRelative));
        assert!(view
            .branches
            .iter()
            .any(|branch| branch.state == ResolutionState::Open));
    }

    #[test]
    fn qa006_resolution_reducer_trace_permutations_produce_the_same_view() {
        let base = resolution_event(0, 1, ResolutionAction::Defer, Vec::new());
        let base_id = base.event_cid();
        let adopt = resolution_event(
            1,
            2,
            ResolutionAction::AdoptBinding {
                mapping: MappingKernelCid::from_bytes([30; 32]),
            },
            vec![base_id],
        );
        let reopen = resolution_event(2, 3, ResolutionAction::Reopen, vec![base_id]);
        let records = [
            assessed(base, ResolutionAuthority::Authorized, None),
            assessed(
                adopt,
                ResolutionAuthority::Authorized,
                Some(BindingAcceptance::Satisfied),
            ),
            assessed(reopen, ResolutionAuthority::Authorized, None),
        ];
        let orders = [
            [0, 1, 2],
            [0, 2, 1],
            [1, 0, 2],
            [1, 2, 0],
            [2, 0, 1],
            [2, 1, 0],
        ];
        let mut expected = None;
        for order in orders {
            let mut reducer = reducer();
            for index in order {
                assert_eq!(
                    reducer.apply(records[index].clone()).unwrap(),
                    ResolutionApplyOutcome::Added
                );
            }
            assert_eq!(
                reducer.apply(records[order[0]].clone()).unwrap(),
                ResolutionApplyOutcome::ExactReplay
            );
            let view = reducer.view().unwrap();
            assert_eq!(view.state, ResolutionState::Concurrent);
            match &expected {
                Some(expected) => assert_eq!(&view, expected),
                None => expected = Some(view),
            }
        }
    }

    #[test]
    fn causal_revocation_removes_only_the_named_adoption() {
        let adoption = resolution_event(
            0,
            1,
            ResolutionAction::AdoptBinding {
                mapping: MappingKernelCid::from_bytes([30; 32]),
            },
            Vec::new(),
        );
        let adoption_id = adoption.event_cid();
        let revoke = resolution_event(
            1,
            2,
            ResolutionAction::RevokeAdoption {
                adoption_event: adoption_id,
            },
            vec![adoption_id],
        );
        let mut reducer = reducer();
        reducer
            .apply(assessed(
                adoption,
                ResolutionAuthority::Authorized,
                Some(BindingAcceptance::Satisfied),
            ))
            .unwrap();
        reducer
            .apply(assessed(revoke, ResolutionAuthority::Authorized, None))
            .unwrap();
        assert_eq!(reducer.view().unwrap().state, ResolutionState::Open);
    }

    #[test]
    fn authorized_adoption_requires_a_durable_mapping() {
        struct Missing;
        impl MaterializedMappingLookup for Missing {
            fn contains_materialized_mapping(
                &self,
                _mapping: MappingKernelCid,
            ) -> Result<bool, String> {
                Ok(false)
            }
        }
        let adoption = resolution_event(
            0,
            1,
            ResolutionAction::AdoptBinding {
                mapping: MappingKernelCid::from_bytes([30; 32]),
            },
            Vec::new(),
        );
        assert_eq!(
            assess_resolution_event(
                adoption,
                ResolutionAuthority::Authorized,
                Some(BindingAcceptance::Satisfied),
                &Missing,
            )
            .unwrap_err(),
            ResolutionError::MappingNotMaterialized
        );
    }
}

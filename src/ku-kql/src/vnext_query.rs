//! Local/private query contracts and scoped result/coverage boundaries.

use std::collections::{BTreeMap, BTreeSet};

use ku_core::foundation::schema_registry::OBJECT_KIND_QUERY_DEFINITION;
use ku_core::foundation::{
    canonicalize_set_by_key, decode_knowledge_object, encode_canonical, Budget, CanonicalValue,
    ConceptCcid, CoverageStatement, CoverageStatus, DisclosureClass, EventCid,
    KnowledgeObjectEnvelope, KnownObjectKind, MappingKernelCid, ObjectCid, ObjectKind,
    ObjectReference, ObjectSemantics, ResourceProfile, SchemaVersion, Selector, SelectorCid,
    SemanticError, SemanticFrameSet,
};

pub const QUERY_DEFINITION_KIND: ObjectKind = ObjectKind(OBJECT_KIND_QUERY_DEFINITION);
pub const QUERY_PROFILE_MAJOR: u64 = 1;
pub const QUERY_PROFILE_MINOR: u64 = 0;
pub const MAX_QUERY_MEMBERS: usize = 16_384;
pub const MIN_ROUTE_TOKEN_SUPPORT: u64 = 64;
pub const MAX_ROUTE_SKETCHES_PER_RUN: u8 = 3;
pub const ROUTE_PACKET_PADDED_BYTES_V1: &[usize] = &[512, 1024, 2048];

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct KnowledgeNeedIr {
    pub receptor_definitions: Vec<ObjectReference>,
    pub desired_roles: Vec<ConceptCcid>,
    pub goal: SemanticFrameSet,
    pub local_context: SemanticFrameSet,
    pub privacy: DisclosureClass,
}

impl KnowledgeNeedIr {
    pub fn validate(&self) -> Result<(), QueryContractError> {
        if !matches!(
            self.privacy,
            DisclosureClass::LocalOnly | DisclosureClass::NegotiatedEncrypted
        ) {
            return Err(QueryContractError::FullNeedMustRemainPrivate);
        }
        if self.receptor_definitions.is_empty()
            || self.desired_roles.is_empty()
            || self.receptor_definitions.len() > MAX_QUERY_MEMBERS
            || self.desired_roles.len() > MAX_QUERY_MEMBERS
        {
            return Err(QueryContractError::Limit);
        }
        canonical_reference_set(&self.receptor_definitions)?;
        canonical_role_set(&self.desired_roles)?;
        self.goal.canonical_value()?;
        self.local_context.canonical_value()?;
        Ok(())
    }

    fn canonical_value(&self) -> Result<CanonicalValue, QueryContractError> {
        self.validate()?;
        Ok(CanonicalValue::Map(vec![
            (0, canonical_reference_set(&self.receptor_definitions)?),
            (1, canonical_role_set(&self.desired_roles)?),
            (2, self.goal.canonical_value()?),
            (3, self.local_context.canonical_value()?),
            (4, CanonicalValue::Unsigned(self.privacy as u64)),
        ]))
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct QueryDefinition {
    pub need: KnowledgeNeedIr,
    pub query_policy: ObjectReference,
    pub exploration_policy: ObjectReference,
}

impl QueryDefinition {
    pub fn to_private_knowledge_object(
        &self,
    ) -> Result<KnowledgeObjectEnvelope, QueryContractError> {
        self.need.validate()?;
        let payload = CanonicalValue::Map(vec![
            (0, CanonicalValue::Unsigned(QUERY_PROFILE_MAJOR)),
            (1, CanonicalValue::Unsigned(QUERY_PROFILE_MINOR)),
            (2, self.need.canonical_value()?),
            (3, reference_value(&self.query_policy)),
            (4, reference_value(&self.exploration_policy)),
        ]);
        let mut object = KnowledgeObjectEnvelope::new(
            QUERY_DEFINITION_KIND,
            SchemaVersion::new(QUERY_PROFILE_MAJOR, QUERY_PROFILE_MINOR),
            self.need.privacy,
            payload,
        );
        object.references = self.need.receptor_definitions.clone();
        object.references.push(self.query_policy.clone());
        object.references.push(self.exploration_policy.clone());
        Ok(object)
    }

    pub fn private_cid(&self) -> Result<ObjectCid, QueryContractError> {
        Ok(self
            .to_private_knowledge_object()?
            .encode(ResourceProfile::ObjectV1)?
            .1)
    }

    pub fn private_canonical_bytes(&self) -> Result<Vec<u8>, QueryContractError> {
        Ok(self
            .to_private_knowledge_object()?
            .encode(ResourceProfile::ObjectV1)?
            .0)
    }

    /// Strict inverse used only after an encrypted Private Vault record has
    /// authenticated successfully.
    pub fn from_private_canonical_bytes(bytes: &[u8]) -> Result<Self, QueryContractError> {
        let validated = decode_knowledge_object(
            bytes,
            ResourceProfile::ObjectV1,
            &[KnownObjectKind::new(
                QUERY_DEFINITION_KIND,
                QUERY_PROFILE_MAJOR,
            )],
            &[],
        )?;
        let ObjectSemantics::Known(envelope) = validated.semantics() else {
            return Err(QueryContractError::InvalidPrivateDefinition);
        };
        if envelope.kind != QUERY_DEFINITION_KIND
            || envelope.kind_version.major != QUERY_PROFILE_MAJOR
            || envelope.kind_version.minor != QUERY_PROFILE_MINOR
            || !matches!(
                envelope.disclosure,
                DisclosureClass::LocalOnly | DisclosureClass::NegotiatedEncrypted
            )
        {
            return Err(QueryContractError::InvalidPrivateDefinition);
        }
        let payload = query_map(&envelope.payload)?;
        if query_unsigned(query_required(payload, 0)?)? != QUERY_PROFILE_MAJOR
            || query_unsigned(query_required(payload, 1)?)? != QUERY_PROFILE_MINOR
        {
            return Err(QueryContractError::InvalidPrivateDefinition);
        }
        let need_map = query_map(query_required(payload, 2)?)?;
        let privacy = match query_unsigned(query_required(need_map, 4)?)? {
            1 => DisclosureClass::NegotiatedEncrypted,
            3 => DisclosureClass::LocalOnly,
            _ => return Err(QueryContractError::FullNeedMustRemainPrivate),
        };
        let definition = Self {
            need: KnowledgeNeedIr {
                receptor_definitions: parse_query_references(query_required(need_map, 0)?)?,
                desired_roles: parse_query_roles(query_required(need_map, 1)?)?,
                goal: SemanticFrameSet::from_canonical_value(query_required(need_map, 2)?)?,
                local_context: SemanticFrameSet::from_canonical_value(query_required(
                    need_map, 3,
                )?)?,
                privacy,
            },
            query_policy: parse_query_reference(query_required(payload, 3)?)?,
            exploration_policy: parse_query_reference(query_required(payload, 4)?)?,
        };
        definition.need.validate()?;
        if definition.private_canonical_bytes()? != bytes {
            return Err(QueryContractError::InvalidPrivateDefinition);
        }
        Ok(definition)
    }
}

fn query_map(value: &CanonicalValue) -> Result<&[(u64, CanonicalValue)], QueryContractError> {
    match value {
        CanonicalValue::Map(map) => Ok(map),
        _ => Err(QueryContractError::InvalidPrivateDefinition),
    }
}

fn query_array(value: &CanonicalValue) -> Result<&[CanonicalValue], QueryContractError> {
    match value {
        CanonicalValue::Array(values) => Ok(values),
        _ => Err(QueryContractError::InvalidPrivateDefinition),
    }
}

fn query_required(
    map: &[(u64, CanonicalValue)],
    key: u64,
) -> Result<&CanonicalValue, QueryContractError> {
    map.iter()
        .find_map(|(candidate, value)| (*candidate == key).then_some(value))
        .ok_or(QueryContractError::InvalidPrivateDefinition)
}

fn query_unsigned(value: &CanonicalValue) -> Result<u64, QueryContractError> {
    match value {
        CanonicalValue::Unsigned(value) => Ok(*value),
        _ => Err(QueryContractError::InvalidPrivateDefinition),
    }
}

fn parse_query_reference(value: &CanonicalValue) -> Result<ObjectReference, QueryContractError> {
    let map = query_map(value)?;
    let reference_kind = query_unsigned(query_required(map, 0)?)?;
    let CanonicalValue::Bytes(cid) = query_required(map, 1)? else {
        return Err(QueryContractError::InvalidPrivateDefinition);
    };
    Ok(ObjectReference::new(
        reference_kind,
        cid.as_slice()
            .try_into()
            .map_err(|_| QueryContractError::InvalidPrivateDefinition)?,
    ))
}

fn parse_query_references(
    value: &CanonicalValue,
) -> Result<Vec<ObjectReference>, QueryContractError> {
    query_array(value)?
        .iter()
        .map(parse_query_reference)
        .collect()
}

fn parse_query_roles(value: &CanonicalValue) -> Result<Vec<ConceptCcid>, QueryContractError> {
    query_array(value)?
        .iter()
        .map(|value| {
            let CanonicalValue::Bytes(bytes) = value else {
                return Err(QueryContractError::InvalidPrivateDefinition);
            };
            Ok(ConceptCcid::from_bytes(
                bytes
                    .as_slice()
                    .try_into()
                    .map_err(|_| QueryContractError::InvalidPrivateDefinition)?,
            ))
        })
        .collect()
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum QueryRunState {
    Planned,
    Running,
    Partial,
    CompletedWithinBoundary,
    Cancelled,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct QueryRun {
    run_id: [u8; 32],
    definition: ObjectCid,
    pub selector: Selector,
    pub state: QueryRunState,
}

impl QueryRun {
    pub fn new(
        run_id: [u8; 32],
        definition: ObjectCid,
        selector: Selector,
    ) -> Result<Self, QueryContractError> {
        if run_id == [0; 32] {
            return Err(QueryContractError::InvalidRunId);
        }
        selector.validate()?;
        Ok(Self {
            run_id,
            definition,
            selector,
            state: QueryRunState::Planned,
        })
    }

    pub const fn run_id(&self) -> &[u8; 32] {
        &self.run_id
    }

    pub const fn definition(&self) -> ObjectCid {
        self.definition
    }

    pub fn selector_cid(&self) -> Result<SelectorCid, QueryContractError> {
        self.selector.cid().map_err(Into::into)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum QueryChannel {
    ExactTypedIndex,
    Structural,
    Opposition,
    LongTail,
    LocalAi,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct QueryWorkItem {
    pub work_id: [u8; 32],
    pub run_id: [u8; 32],
    pub channel: QueryChannel,
    pub boundary: SelectorCid,
    pub budget: Budget,
    pub continuation: Option<[u8; 32]>,
}

impl QueryWorkItem {
    pub fn validate_for(&self, run: &QueryRun) -> Result<(), QueryContractError> {
        if self.work_id == [0; 32] || self.run_id != *run.run_id() {
            return Err(QueryContractError::RunMismatch);
        }
        if self.boundary != run.selector_cid()? {
            return Err(QueryContractError::BoundaryMismatch);
        }
        if self.budget.max_records > run.selector.budget.max_records
            || self.budget.max_bytes > run.selector.budget.max_bytes
            || self.budget.max_work_units > run.selector.budget.max_work_units
            || self.budget.max_depth > run.selector.budget.max_depth
        {
            return Err(QueryContractError::BudgetExpansion);
        }
        Ok(())
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum QueryResultRef {
    Object(ObjectReference),
    Mapping(MappingKernelCid),
    Event(EventCid),
}

impl QueryResultRef {
    fn to_value(&self) -> CanonicalValue {
        match self {
            Self::Object(reference) => CanonicalValue::Map(vec![
                (0, CanonicalValue::Unsigned(0)),
                (1, reference_value(reference)),
            ]),
            Self::Mapping(mapping) => CanonicalValue::Map(vec![
                (0, CanonicalValue::Unsigned(1)),
                (1, CanonicalValue::Bytes(mapping.as_bytes().to_vec())),
            ]),
            Self::Event(event) => CanonicalValue::Map(vec![
                (0, CanonicalValue::Unsigned(2)),
                (1, CanonicalValue::Bytes(event.as_bytes().to_vec())),
            ]),
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum QueryLimitation {
    BudgetExhausted,
    PathLimited,
    FrontierIncomplete,
    ChannelUnavailable,
    Cancelled,
    ValidationPending,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct QueryResultBatch {
    pub run_id: [u8; 32],
    pub work_id: [u8; 32],
    pub boundary: SelectorCid,
    pub results: Vec<QueryResultRef>,
    pub coverage: CoverageStatement,
    pub limitations: Vec<QueryLimitation>,
    pub continuation: Option<[u8; 32]>,
}

impl QueryResultBatch {
    pub fn validate_for(
        &self,
        run: &QueryRun,
        work: &QueryWorkItem,
    ) -> Result<(), QueryContractError> {
        work.validate_for(run)?;
        if self.run_id != *run.run_id() || self.work_id != work.work_id {
            return Err(QueryContractError::RunMismatch);
        }
        if self.boundary != work.boundary || self.coverage.selector != self.boundary {
            return Err(QueryContractError::BoundaryMismatch);
        }
        self.coverage.validate()?;
        if self.results.len() > work.budget.max_records as usize
            || self.results.len() as u64 > self.coverage.returned_records
            || self.coverage.returned_bytes > work.budget.max_bytes
        {
            return Err(QueryContractError::BudgetExpansion);
        }
        canonical_result_set(&self.results)?;
        unique_limitations(&self.limitations)?;
        if self.coverage.status == CoverageStatus::Partial
            && self.continuation.is_none()
            && self.limitations.is_empty()
        {
            return Err(QueryContractError::UnqualifiedPartialResult);
        }
        if self.coverage.is_complete_within_selector() && self.continuation.is_some() {
            return Err(QueryContractError::InvalidContinuation);
        }
        Ok(())
    }

    pub const fn is_globally_complete(&self) -> bool {
        false
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum QueryWorkOutcome {
    CompletedWithinBoundary,
    Partial,
    Cancelled,
    Failed,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct QueryReceipt {
    pub run_id: [u8; 32],
    pub work_id: [u8; 32],
    pub boundary: SelectorCid,
    pub assessed_frontier: Vec<EventCid>,
    pub examined_records: u64,
    pub examined_bytes: u64,
    pub returned_records: u64,
    pub outcome: QueryWorkOutcome,
    pub limitations: Vec<QueryLimitation>,
    pub continuation: Option<[u8; 32]>,
}

impl QueryReceipt {
    pub fn validate_for(
        &self,
        run: &QueryRun,
        work: &QueryWorkItem,
    ) -> Result<(), QueryContractError> {
        work.validate_for(run)?;
        if self.run_id != *run.run_id()
            || self.work_id != work.work_id
            || self.boundary != work.boundary
        {
            return Err(QueryContractError::RunMismatch);
        }
        if self.assessed_frontier.len() > MAX_QUERY_MEMBERS
            || self.examined_records > work.budget.max_records
            || self.examined_bytes > work.budget.max_bytes
            || self.returned_records > self.examined_records
        {
            return Err(QueryContractError::BudgetExpansion);
        }
        unique_event_ids(&self.assessed_frontier)?;
        unique_limitations(&self.limitations)?;
        if matches!(
            self.outcome,
            QueryWorkOutcome::Partial | QueryWorkOutcome::Cancelled | QueryWorkOutcome::Failed
        ) && self.continuation.is_none()
            && self.limitations.is_empty()
        {
            return Err(QueryContractError::UnqualifiedPartialResult);
        }
        Ok(())
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CoarseRouteTokenClass {
    ObjectClass,
    CapabilityClass,
    DimensionClass,
    OperatorFamily,
    CoarseRole,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct CoarseRouteToken {
    pub class: CoarseRouteTokenClass,
    pub allowlisted_code: u16,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct RouteSketchEntropy {
    pub sketch_id: [u8; 32],
    pub one_time_reply_capability: [u8; 32],
    pub replay_nonce: [u8; 32],
    pub commitment_salt: [u8; 32],
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RouteNeedSketch {
    sketch_id: [u8; 32],
    reply_capability: [u8; 32],
    token: CoarseRouteToken,
    response_budget_bucket: u8,
    expiry_evaluations: u32,
    hop_budget: u8,
    padding_class: u8,
    replay_nonce: [u8; 32],
    salted_disclosure_commitment: [u8; 32],
}

impl RouteNeedSketch {
    pub fn network_bytes(&self) -> Result<Vec<u8>, QueryContractError> {
        let target = route_padding_target(self.padding_class)
            .ok_or(QueryContractError::InvalidRouteSketch)?;
        for padding_len in 0..=target {
            let bytes = encode_canonical(
                &self.canonical_value(padding_len),
                ResourceProfile::ControlV1,
            )?;
            if bytes.len() == target {
                return Ok(bytes);
            }
            if bytes.len() > target {
                break;
            }
        }
        Err(QueryContractError::InvalidRouteSketch)
    }

    fn canonical_value(&self, padding_len: usize) -> CanonicalValue {
        CanonicalValue::Map(vec![
            (0, CanonicalValue::Unsigned(1)),
            (1, CanonicalValue::Bytes(self.sketch_id.to_vec())),
            (2, CanonicalValue::Bytes(self.reply_capability.to_vec())),
            (
                3,
                CanonicalValue::Map(vec![
                    (
                        0,
                        CanonicalValue::Unsigned(match self.token.class {
                            CoarseRouteTokenClass::ObjectClass => 0,
                            CoarseRouteTokenClass::CapabilityClass => 1,
                            CoarseRouteTokenClass::DimensionClass => 2,
                            CoarseRouteTokenClass::OperatorFamily => 3,
                            CoarseRouteTokenClass::CoarseRole => 4,
                        }),
                    ),
                    (
                        1,
                        CanonicalValue::Unsigned(u64::from(self.token.allowlisted_code)),
                    ),
                ]),
            ),
            (
                4,
                CanonicalValue::Unsigned(u64::from(self.response_budget_bucket)),
            ),
            (
                5,
                CanonicalValue::Unsigned(u64::from(self.expiry_evaluations)),
            ),
            (6, CanonicalValue::Unsigned(u64::from(self.hop_budget))),
            (7, CanonicalValue::Unsigned(u64::from(self.padding_class))),
            (8, CanonicalValue::Bytes(self.replay_nonce.to_vec())),
            (
                9,
                CanonicalValue::Bytes(self.salted_disclosure_commitment.to_vec()),
            ),
            (10, CanonicalValue::Bytes(vec![0; padding_len])),
        ])
    }
}

#[derive(Default)]
struct DisclosureRunState {
    emitted: u8,
    sketch_ids: BTreeSet<[u8; 32]>,
    reply_capabilities: BTreeSet<[u8; 32]>,
    replay_nonces: BTreeSet<[u8; 32]>,
    commitment_salts: BTreeSet<[u8; 32]>,
}

#[derive(Default)]
pub struct DisclosureCompiler {
    runs: BTreeMap<[u8; 32], DisclosureRunState>,
}

impl DisclosureCompiler {
    #[allow(clippy::too_many_arguments)]
    pub fn compile_route_minimal(
        &mut self,
        run: &QueryRun,
        token: CoarseRouteToken,
        estimated_support: u64,
        response_budget_bucket: u8,
        expiry_evaluations: u32,
        hop_budget: u8,
        padding_class: u8,
        entropy: RouteSketchEntropy,
    ) -> Result<RouteNeedSketch, QueryContractError> {
        if estimated_support < MIN_ROUTE_TOKEN_SUPPORT {
            return Err(QueryContractError::RouteTokenTooRare);
        }
        if token.allowlisted_code == 0
            || response_budget_bucket == 0
            || expiry_evaluations == 0
            || hop_budget == 0
            || route_padding_target(padding_class).is_none()
            || entropy.sketch_id == [0; 32]
            || entropy.one_time_reply_capability == [0; 32]
            || entropy.replay_nonce == [0; 32]
            || entropy.commitment_salt == [0; 32]
        {
            return Err(QueryContractError::InvalidRouteSketch);
        }
        let state = self.runs.entry(*run.run_id()).or_default();
        if state.emitted >= MAX_ROUTE_SKETCHES_PER_RUN {
            return Err(QueryContractError::RoutePacketLimit);
        }
        if state.sketch_ids.contains(&entropy.sketch_id)
            || state
                .reply_capabilities
                .contains(&entropy.one_time_reply_capability)
            || state.replay_nonces.contains(&entropy.replay_nonce)
            || state.commitment_salts.contains(&entropy.commitment_salt)
        {
            return Err(QueryContractError::RouteEntropyReuse);
        }
        let mut hasher = blake3::Hasher::new();
        hasher.update(b"onebrain:vnext:route-need-disclosure:1\0");
        hasher.update(&entropy.commitment_salt);
        hasher.update(run.definition().as_bytes());
        hasher.update(run.run_id());
        let commitment = *hasher.finalize().as_bytes();
        state.emitted += 1;
        state.sketch_ids.insert(entropy.sketch_id);
        state
            .reply_capabilities
            .insert(entropy.one_time_reply_capability);
        state.replay_nonces.insert(entropy.replay_nonce);
        state.commitment_salts.insert(entropy.commitment_salt);
        Ok(RouteNeedSketch {
            sketch_id: entropy.sketch_id,
            reply_capability: entropy.one_time_reply_capability,
            token,
            response_budget_bucket,
            expiry_evaluations,
            hop_budget,
            padding_class,
            replay_nonce: entropy.replay_nonce,
            salted_disclosure_commitment: commitment,
        })
    }
}

fn reference_value(reference: &ObjectReference) -> CanonicalValue {
    CanonicalValue::Map(vec![
        (0, CanonicalValue::Unsigned(reference.reference_kind)),
        (1, CanonicalValue::Bytes(reference.cid.to_vec())),
    ])
}

fn canonical_reference_set(
    values: &[ObjectReference],
) -> Result<CanonicalValue, QueryContractError> {
    canonical_set(values.iter().map(reference_value).collect())
}

fn canonical_role_set(values: &[ConceptCcid]) -> Result<CanonicalValue, QueryContractError> {
    canonical_set(
        values
            .iter()
            .map(|role| CanonicalValue::Bytes(role.as_bytes().to_vec()))
            .collect(),
    )
}

fn canonical_result_set(values: &[QueryResultRef]) -> Result<CanonicalValue, QueryContractError> {
    canonical_set(values.iter().map(QueryResultRef::to_value).collect())
}

fn canonical_set(values: Vec<CanonicalValue>) -> Result<CanonicalValue, QueryContractError> {
    if values.len() > MAX_QUERY_MEMBERS {
        return Err(QueryContractError::Limit);
    }
    let values = values
        .into_iter()
        .map(|value| (value.clone(), value))
        .collect();
    Ok(CanonicalValue::Array(canonicalize_set_by_key(
        values,
        ResourceProfile::ObjectV1,
    )?))
}

fn unique_limitations(values: &[QueryLimitation]) -> Result<(), QueryContractError> {
    let unique = values.iter().copied().collect::<BTreeSet<_>>();
    if unique.len() == values.len() {
        Ok(())
    } else {
        Err(QueryContractError::DuplicateMember)
    }
}

fn unique_event_ids(values: &[EventCid]) -> Result<(), QueryContractError> {
    let unique = values
        .iter()
        .map(|event| *event.as_bytes())
        .collect::<BTreeSet<_>>();
    if unique.len() == values.len() {
        Ok(())
    } else {
        Err(QueryContractError::DuplicateMember)
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum QueryContractError {
    Canonical(ku_core::foundation::CanonicalError),
    Object(ku_core::foundation::ObjectError),
    Semantic(SemanticError),
    Inventory(ku_core::foundation::InventoryError),
    FullNeedMustRemainPrivate,
    Limit,
    InvalidRunId,
    RunMismatch,
    BoundaryMismatch,
    BudgetExpansion,
    DuplicateMember,
    UnqualifiedPartialResult,
    InvalidContinuation,
    RouteTokenTooRare,
    InvalidRouteSketch,
    RoutePacketLimit,
    RouteEntropyReuse,
    InvalidPrivateDefinition,
}

pub const fn route_padding_target(padding_class: u8) -> Option<usize> {
    match padding_class {
        1 => Some(ROUTE_PACKET_PADDED_BYTES_V1[0]),
        2 => Some(ROUTE_PACKET_PADDED_BYTES_V1[1]),
        3 => Some(ROUTE_PACKET_PADDED_BYTES_V1[2]),
        _ => None,
    }
}

impl From<ku_core::foundation::CanonicalError> for QueryContractError {
    fn from(error: ku_core::foundation::CanonicalError) -> Self {
        Self::Canonical(error)
    }
}

impl From<ku_core::foundation::ObjectError> for QueryContractError {
    fn from(error: ku_core::foundation::ObjectError) -> Self {
        Self::Object(error)
    }
}

impl From<SemanticError> for QueryContractError {
    fn from(error: SemanticError) -> Self {
        Self::Semantic(error)
    }
}

impl From<ku_core::foundation::InventoryError> for QueryContractError {
    fn from(error: ku_core::foundation::InventoryError) -> Self {
        Self::Inventory(error)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use ku_core::foundation::{
        public_knowledge_exchange_fixture_v1, CarrierKind, CarrierProfile, CoverageBasis,
        CoverageLimitation, InventoryError,
    };

    fn reference(byte: u8) -> ObjectReference {
        ObjectReference::new(0, [byte; 32])
    }

    fn definition(privacy: DisclosureClass) -> QueryDefinition {
        QueryDefinition {
            need: KnowledgeNeedIr {
                receptor_definitions: vec![reference(1)],
                desired_roles: vec![ConceptCcid::from_bytes([2; 16])],
                goal: SemanticFrameSet {
                    statements: Vec::new(),
                },
                local_context: SemanticFrameSet {
                    statements: Vec::new(),
                },
                privacy,
            },
            query_policy: reference(3),
            exploration_policy: reference(4),
        }
    }

    fn run() -> QueryRun {
        let definition = definition(DisclosureClass::LocalOnly);
        QueryRun::new(
            [5; 32],
            definition.private_cid().unwrap(),
            public_knowledge_exchange_fixture_v1(),
        )
        .unwrap()
    }

    fn work(run: &QueryRun) -> QueryWorkItem {
        QueryWorkItem {
            work_id: [6; 32],
            run_id: *run.run_id(),
            channel: QueryChannel::ExactTypedIndex,
            boundary: run.selector_cid().unwrap(),
            budget: Budget::new(100, 1_000_000, 10_000, 8).unwrap(),
            continuation: None,
        }
    }

    #[test]
    fn full_need_and_query_definition_cannot_be_public_or_route_minimal() {
        for disclosure in [DisclosureClass::Public, DisclosureClass::RouteMinimal] {
            assert_eq!(
                definition(disclosure)
                    .to_private_knowledge_object()
                    .unwrap_err(),
                QueryContractError::FullNeedMustRemainPrivate
            );
        }
        let private = definition(DisclosureClass::LocalOnly)
            .to_private_knowledge_object()
            .unwrap();
        assert_eq!(private.disclosure, DisclosureClass::LocalOnly);
    }

    #[test]
    fn work_budget_and_boundary_can_only_narrow_the_run() {
        let run = run();
        let mut expanded = work(&run);
        expanded.validate_for(&run).unwrap();
        expanded.budget.max_records = run.selector.budget.max_records + 1;
        assert_eq!(
            expanded.validate_for(&run).unwrap_err(),
            QueryContractError::BudgetExpansion
        );
        let mut wrong = work(&run);
        wrong.boundary = SelectorCid::from_bytes([9; 32]);
        assert_eq!(
            wrong.validate_for(&run).unwrap_err(),
            QueryContractError::BoundaryMismatch
        );
    }

    #[test]
    fn every_batch_is_scoped_and_partial_requires_limitation_or_continuation() {
        let run = run();
        let work = work(&run);
        let mut batch = QueryResultBatch {
            run_id: *run.run_id(),
            work_id: work.work_id,
            boundary: work.boundary,
            results: Vec::new(),
            coverage: CoverageStatement {
                selector: work.boundary,
                assessed_frontier: run.selector.frontier.clone(),
                basis: CoverageBasis::Sampled,
                status: CoverageStatus::Partial,
                returned_records: 0,
                returned_bytes: 0,
                continuation: None,
                limitations: vec![CoverageLimitation::PathLimited],
            },
            limitations: Vec::new(),
            continuation: None,
        };
        assert_eq!(
            batch.validate_for(&run, &work).unwrap_err(),
            QueryContractError::UnqualifiedPartialResult
        );
        batch.limitations = vec![QueryLimitation::PathLimited];
        batch.validate_for(&run, &work).unwrap();
        assert!(!batch.is_globally_complete());
    }

    #[test]
    fn exact_zero_result_is_only_complete_within_the_selector() {
        let run = run();
        let work = work(&run);
        let batch = QueryResultBatch {
            run_id: *run.run_id(),
            work_id: work.work_id,
            boundary: work.boundary,
            results: Vec::new(),
            coverage: CoverageStatement {
                selector: work.boundary,
                assessed_frontier: run.selector.frontier.clone(),
                basis: CoverageBasis::ExactInventory,
                status: CoverageStatus::CompleteWithinSelector,
                returned_records: 0,
                returned_bytes: 0,
                continuation: None,
                limitations: Vec::new(),
            },
            limitations: Vec::new(),
            continuation: None,
        };
        batch.validate_for(&run, &work).unwrap();
        assert!(batch.coverage.is_complete_within_selector());
        assert!(!batch.is_globally_complete());
    }

    #[test]
    fn only_disclosure_compiler_can_construct_bounded_route_sketches() {
        let run = run();
        let mut compiler = DisclosureCompiler::default();
        let token = CoarseRouteToken {
            class: CoarseRouteTokenClass::CoarseRole,
            allowlisted_code: 7,
        };
        let entropy = |byte| RouteSketchEntropy {
            sketch_id: [byte; 32],
            one_time_reply_capability: [byte + 1; 32],
            replay_nonce: [byte + 2; 32],
            commitment_salt: [byte + 3; 32],
        };
        assert_eq!(
            compiler
                .compile_route_minimal(&run, token, 63, 1, 10, 3, 1, entropy(10))
                .unwrap_err(),
            QueryContractError::RouteTokenTooRare
        );
        for byte in 20..23 {
            let sketch = compiler
                .compile_route_minimal(&run, token, 64, 1, 10, 3, 1, entropy(byte))
                .unwrap();
            let bytes = sketch.network_bytes().unwrap();
            assert!(!bytes
                .windows(32)
                .any(|window| window == run.definition().as_bytes()));
            assert!(!bytes.windows(32).any(|window| window == run.run_id()));
        }
        assert_eq!(
            compiler
                .compile_route_minimal(&run, token, 64, 1, 10, 3, 1, entropy(30))
                .unwrap_err(),
            QueryContractError::RoutePacketLimit
        );
    }

    #[test]
    fn receipt_cannot_expand_budget_or_hide_partial_limits() {
        let run = run();
        let work = work(&run);
        let mut receipt = QueryReceipt {
            run_id: *run.run_id(),
            work_id: work.work_id,
            boundary: work.boundary,
            assessed_frontier: run.selector.frontier.clone(),
            examined_records: 1,
            examined_bytes: 100,
            returned_records: 0,
            outcome: QueryWorkOutcome::Partial,
            limitations: Vec::new(),
            continuation: None,
        };
        assert_eq!(
            receipt.validate_for(&run, &work).unwrap_err(),
            QueryContractError::UnqualifiedPartialResult
        );
        receipt.limitations = vec![QueryLimitation::ValidationPending];
        receipt.validate_for(&run, &work).unwrap();
        receipt.examined_records = work.budget.max_records + 1;
        assert_eq!(
            receipt.validate_for(&run, &work).unwrap_err(),
            QueryContractError::BudgetExpansion
        );

        // Keep imports honest: private selector rejection stays owned by INV-001.
        let mut selector = public_knowledge_exchange_fixture_v1();
        selector.disclosure_classes = vec![DisclosureClass::LocalOnly];
        assert_eq!(
            selector.validate().unwrap_err(),
            InventoryError::PrivateStorageClass
        );
        let _carrier = CarrierProfile {
            kind: CarrierKind::InMemory,
            max_frame_bytes: 1024,
            max_bundle_bytes: 1024,
            store_carry_forward: false,
            bidirectional: true,
        };
    }
}

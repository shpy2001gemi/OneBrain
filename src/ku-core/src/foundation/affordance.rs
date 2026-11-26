//! Explicit, provenance-preserving Knowledge Affordance objects.

use std::collections::{BTreeMap, BTreeSet};

use super::canonical::{canonicalize_set_by_key, CanonicalValue, ResourceProfile};
use super::object::{
    DisclosureClass, KnowledgeObjectEnvelope, ObjectError, ObjectKind, ObjectReference,
    ObjectSemantics, SchemaVersion, ValidatedKnowledgeObject,
};
use super::receptor::StatementLocator;
use super::schema_registry::OBJECT_KIND_KNOWLEDGE_AFFORDANCE;
use super::semantic::{
    ConceptCcid, ConstraintExpression, LiteralValue, Modality, SemanticError, SemanticFrameSet,
    StatementFrame, StatementId, StatementQualifiers, TermRef, TypedConstraint,
};

pub const KNOWLEDGE_AFFORDANCE_KIND: ObjectKind = ObjectKind(OBJECT_KIND_KNOWLEDGE_AFFORDANCE);
pub const AFFORDANCE_PROFILE_MAJOR: u64 = 1;
pub const AFFORDANCE_PROFILE_MINOR: u64 = 0;
pub const MAX_AFFORDANCE_MEMBERS: usize = 4_096;
pub const VIRTUAL_SFS_AFFORDANCE_PROFILE: &str = "virtual-sfs-affordance/1";

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AcceptedInput {
    pub receptor_definition: ObjectReference,
    pub role: ConceptCcid,
    pub required: bool,
}

impl AcceptedInput {
    fn to_value(&self) -> CanonicalValue {
        CanonicalValue::Map(vec![
            (0, self.receptor_definition.to_value()),
            (1, CanonicalValue::Bytes(self.role.as_bytes().to_vec())),
            (2, CanonicalValue::Bool(self.required)),
        ])
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum AffordanceOrigin {
    Explicit {
        claims: Vec<StatementLocator>,
    },
    Derived {
        derivation_engine: ObjectReference,
        derivation_rule: ObjectReference,
        inputs: Vec<ObjectReference>,
    },
}

impl AffordanceOrigin {
    fn to_value(&self) -> Result<CanonicalValue, AffordanceError> {
        match self {
            Self::Explicit { claims } => {
                if claims.len() > MAX_AFFORDANCE_MEMBERS {
                    return Err(AffordanceError::Limit);
                }
                let members = claims
                    .iter()
                    .map(|claim| {
                        let value = claim.to_value();
                        (value.clone(), value)
                    })
                    .collect();
                Ok(CanonicalValue::Map(vec![
                    (0, CanonicalValue::Unsigned(0)),
                    (
                        1,
                        CanonicalValue::Array(canonicalize_set_by_key(
                            members,
                            ResourceProfile::ObjectV1,
                        )?),
                    ),
                ]))
            }
            Self::Derived {
                derivation_engine,
                derivation_rule,
                inputs,
            } => Ok(CanonicalValue::Map(vec![
                (0, CanonicalValue::Unsigned(1)),
                (1, derivation_engine.to_value()),
                (2, derivation_rule.to_value()),
                (3, canonical_reference_set(inputs)?),
            ])),
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AffordanceSemantics {
    pub preconditions: SemanticFrameSet,
    pub outputs: SemanticFrameSet,
    pub effects: SemanticFrameSet,
    pub properties: SemanticFrameSet,
    pub invariants: SemanticFrameSet,
    pub operating_conditions: SemanticFrameSet,
    pub limits: SemanticFrameSet,
}

impl AffordanceSemantics {
    fn to_value(&self) -> Result<CanonicalValue, AffordanceError> {
        Ok(CanonicalValue::Map(vec![
            (0, self.preconditions.canonical_value()?),
            (1, self.outputs.canonical_value()?),
            (2, self.effects.canonical_value()?),
            (3, self.properties.canonical_value()?),
            (4, self.invariants.canonical_value()?),
            (5, self.operating_conditions.canonical_value()?),
            (6, self.limits.canonical_value()?),
        ]))
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct KnowledgeAffordance {
    pub sources: Vec<ObjectReference>,
    pub offered_roles: Vec<ConceptCcid>,
    pub accepted_inputs: Vec<AcceptedInput>,
    pub semantics: AffordanceSemantics,
    pub abstraction_patterns: Vec<SemanticFrameSet>,
    pub origin: AffordanceOrigin,
}

impl KnowledgeAffordance {
    /// Exact declared support only. Embeddings/rankers cannot add a role here.
    pub fn supports_role(&self, role: ConceptCcid) -> bool {
        self.offered_roles.contains(&role)
    }

    /// Recover the typed affordance only from a validated, known v1 object.
    /// Unknown fields, alternate set order, and non-canonical semantic frames
    /// are rejected by the final canonical round trip.
    pub fn from_validated_object(
        validated: &ValidatedKnowledgeObject,
    ) -> Result<Self, AffordanceError> {
        let ObjectSemantics::Known(envelope) = validated.semantics() else {
            return Err(AffordanceError::InvalidField("affordance.object"));
        };
        if envelope.kind != KNOWLEDGE_AFFORDANCE_KIND {
            return Err(AffordanceError::InvalidField("affordance.kind"));
        }
        if envelope.kind_version.major != AFFORDANCE_PROFILE_MAJOR
            || envelope.kind_version.minor != AFFORDANCE_PROFILE_MINOR
        {
            return Err(AffordanceError::UnsupportedVersion);
        }
        Self::from_canonical_payload(&envelope.payload)
    }

    pub fn from_canonical_payload(value: &CanonicalValue) -> Result<Self, AffordanceError> {
        let map = affordance_map(value, "affordance")?;
        if affordance_unsigned(
            affordance_required(map, 0, "affordance.major")?,
            "affordance.major",
        )? != AFFORDANCE_PROFILE_MAJOR
            || affordance_unsigned(
                affordance_required(map, 1, "affordance.minor")?,
                "affordance.minor",
            )? != AFFORDANCE_PROFILE_MINOR
        {
            return Err(AffordanceError::UnsupportedVersion);
        }
        let sources = parse_reference_array(
            affordance_required(map, 2, "affordance.sources")?,
            "affordance.sources",
        )?;
        let offered_roles = affordance_array(
            affordance_required(map, 3, "affordance.roles")?,
            "affordance.roles",
        )?
        .iter()
        .map(|value| affordance_ccid(value, "affordance.role"))
        .collect::<Result<Vec<_>, _>>()?;
        let accepted_inputs = affordance_array(
            affordance_required(map, 4, "affordance.inputs")?,
            "affordance.inputs",
        )?
        .iter()
        .map(parse_accepted_input)
        .collect::<Result<Vec<_>, _>>()?;
        let semantics =
            parse_affordance_semantics(affordance_required(map, 5, "affordance.semantics")?)?;
        let abstraction_patterns = affordance_array(
            affordance_required(map, 6, "affordance.patterns")?,
            "affordance.patterns",
        )?
        .iter()
        .map(SemanticFrameSet::from_canonical_value)
        .collect::<Result<Vec<_>, _>>()?;
        let origin = parse_affordance_origin(affordance_required(map, 7, "affordance.origin")?)?;
        let decoded = Self {
            sources,
            offered_roles,
            accepted_inputs,
            semantics,
            abstraction_patterns,
            origin,
        };
        if decoded.canonical_payload()? != *value {
            return Err(AffordanceError::NonCanonicalValue);
        }
        Ok(decoded)
    }

    pub fn canonical_payload(&self) -> Result<CanonicalValue, AffordanceError> {
        Ok(CanonicalValue::Map(vec![
            (0, CanonicalValue::Unsigned(AFFORDANCE_PROFILE_MAJOR)),
            (1, CanonicalValue::Unsigned(AFFORDANCE_PROFILE_MINOR)),
            (2, canonical_reference_set(&self.sources)?),
            (3, canonical_ccid_set(&self.offered_roles)?),
            (4, canonical_input_set(&self.accepted_inputs)?),
            (5, self.semantics.to_value()?),
            (6, canonical_pattern_set(&self.abstraction_patterns)?),
            (7, self.origin.to_value()?),
        ]))
    }

    pub fn to_knowledge_object(
        &self,
        disclosure: DisclosureClass,
    ) -> Result<KnowledgeObjectEnvelope, AffordanceError> {
        Ok(KnowledgeObjectEnvelope::new(
            KNOWLEDGE_AFFORDANCE_KIND,
            SchemaVersion::new(AFFORDANCE_PROFILE_MAJOR, AFFORDANCE_PROFILE_MINOR),
            disclosure,
            self.canonical_payload()?,
        ))
    }
}

fn parse_accepted_input(value: &CanonicalValue) -> Result<AcceptedInput, AffordanceError> {
    let map = affordance_map(value, "affordance.input")?;
    Ok(AcceptedInput {
        receptor_definition: ObjectReference::from_value(affordance_required(
            map,
            0,
            "affordance.input.receptor",
        )?)?,
        role: affordance_ccid(
            affordance_required(map, 1, "affordance.input.role")?,
            "affordance.input.role",
        )?,
        required: affordance_bool(
            affordance_required(map, 2, "affordance.input.required")?,
            "affordance.input.required",
        )?,
    })
}

fn parse_affordance_semantics(
    value: &CanonicalValue,
) -> Result<AffordanceSemantics, AffordanceError> {
    let map = affordance_map(value, "affordance.semantics")?;
    Ok(AffordanceSemantics {
        preconditions: SemanticFrameSet::from_canonical_value(affordance_required(
            map,
            0,
            "affordance.preconditions",
        )?)?,
        outputs: SemanticFrameSet::from_canonical_value(affordance_required(
            map,
            1,
            "affordance.outputs",
        )?)?,
        effects: SemanticFrameSet::from_canonical_value(affordance_required(
            map,
            2,
            "affordance.effects",
        )?)?,
        properties: SemanticFrameSet::from_canonical_value(affordance_required(
            map,
            3,
            "affordance.properties",
        )?)?,
        invariants: SemanticFrameSet::from_canonical_value(affordance_required(
            map,
            4,
            "affordance.invariants",
        )?)?,
        operating_conditions: SemanticFrameSet::from_canonical_value(affordance_required(
            map,
            5,
            "affordance.operating_conditions",
        )?)?,
        limits: SemanticFrameSet::from_canonical_value(affordance_required(
            map,
            6,
            "affordance.limits",
        )?)?,
    })
}

fn parse_affordance_origin(value: &CanonicalValue) -> Result<AffordanceOrigin, AffordanceError> {
    let map = affordance_map(value, "affordance.origin")?;
    match affordance_unsigned(
        affordance_required(map, 0, "affordance.origin.kind")?,
        "affordance.origin.kind",
    )? {
        0 => {
            let claims = affordance_array(
                affordance_required(map, 1, "affordance.origin.claims")?,
                "affordance.origin.claims",
            )?
            .iter()
            .map(parse_statement_locator)
            .collect::<Result<Vec<_>, _>>()?;
            Ok(AffordanceOrigin::Explicit { claims })
        }
        1 => Ok(AffordanceOrigin::Derived {
            derivation_engine: ObjectReference::from_value(affordance_required(
                map,
                1,
                "affordance.origin.engine",
            )?)?,
            derivation_rule: ObjectReference::from_value(affordance_required(
                map,
                2,
                "affordance.origin.rule",
            )?)?,
            inputs: parse_reference_array(
                affordance_required(map, 3, "affordance.origin.inputs")?,
                "affordance.origin.inputs",
            )?,
        }),
        _ => Err(AffordanceError::InvalidField("affordance.origin.kind")),
    }
}

fn parse_statement_locator(value: &CanonicalValue) -> Result<StatementLocator, AffordanceError> {
    let map = affordance_map(value, "statement_locator")?;
    Ok(StatementLocator {
        object: ObjectReference::from_value(affordance_required(
            map,
            0,
            "statement_locator.object",
        )?)?,
        statement_index: u32::try_from(affordance_unsigned(
            affordance_required(map, 1, "statement_locator.index")?,
            "statement_locator.index",
        )?)
        .map_err(|_| AffordanceError::InvalidField("statement_locator.index"))?,
    })
}

fn parse_reference_array(
    value: &CanonicalValue,
    field: &'static str,
) -> Result<Vec<ObjectReference>, AffordanceError> {
    affordance_array(value, field)?
        .iter()
        .map(|value| ObjectReference::from_value(value).map_err(Into::into))
        .collect()
}

fn affordance_map<'a>(
    value: &'a CanonicalValue,
    field: &'static str,
) -> Result<&'a [(u64, CanonicalValue)], AffordanceError> {
    match value {
        CanonicalValue::Map(values) => Ok(values),
        _ => Err(AffordanceError::InvalidField(field)),
    }
}

fn affordance_array<'a>(
    value: &'a CanonicalValue,
    field: &'static str,
) -> Result<&'a [CanonicalValue], AffordanceError> {
    match value {
        CanonicalValue::Array(values) if values.len() <= MAX_AFFORDANCE_MEMBERS => Ok(values),
        _ => Err(AffordanceError::InvalidField(field)),
    }
}

fn affordance_required<'a>(
    map: &'a [(u64, CanonicalValue)],
    key: u64,
    field: &'static str,
) -> Result<&'a CanonicalValue, AffordanceError> {
    map.iter()
        .find_map(|(candidate, value)| (*candidate == key).then_some(value))
        .ok_or(AffordanceError::InvalidField(field))
}

fn affordance_unsigned(
    value: &CanonicalValue,
    field: &'static str,
) -> Result<u64, AffordanceError> {
    match value {
        CanonicalValue::Unsigned(value) => Ok(*value),
        _ => Err(AffordanceError::InvalidField(field)),
    }
}

fn affordance_bool(value: &CanonicalValue, field: &'static str) -> Result<bool, AffordanceError> {
    match value {
        CanonicalValue::Bool(value) => Ok(*value),
        _ => Err(AffordanceError::InvalidField(field)),
    }
}

fn affordance_ccid(
    value: &CanonicalValue,
    field: &'static str,
) -> Result<ConceptCcid, AffordanceError> {
    let CanonicalValue::Bytes(bytes) = value else {
        return Err(AffordanceError::InvalidField(field));
    };
    let bytes = bytes
        .as_slice()
        .try_into()
        .map_err(|_| AffordanceError::InvalidField(field))?;
    Ok(ConceptCcid::from_bytes(bytes))
}

fn canonical_reference_set(values: &[ObjectReference]) -> Result<CanonicalValue, AffordanceError> {
    canonical_set(values.iter().map(ObjectReference::to_value).collect())
}

fn canonical_ccid_set(values: &[ConceptCcid]) -> Result<CanonicalValue, AffordanceError> {
    canonical_set(
        values
            .iter()
            .map(|value| CanonicalValue::Bytes(value.as_bytes().to_vec()))
            .collect(),
    )
}

fn canonical_input_set(values: &[AcceptedInput]) -> Result<CanonicalValue, AffordanceError> {
    canonical_set(values.iter().map(AcceptedInput::to_value).collect())
}

fn canonical_pattern_set(values: &[SemanticFrameSet]) -> Result<CanonicalValue, AffordanceError> {
    canonical_set(
        values
            .iter()
            .map(SemanticFrameSet::canonical_value)
            .collect::<Result<Vec<_>, _>>()?,
    )
}

fn canonical_set(values: Vec<CanonicalValue>) -> Result<CanonicalValue, AffordanceError> {
    if values.len() > MAX_AFFORDANCE_MEMBERS {
        return Err(AffordanceError::Limit);
    }
    let members = values
        .into_iter()
        .map(|value| (value.clone(), value))
        .collect();
    Ok(CanonicalValue::Array(canonicalize_set_by_key(
        members,
        ResourceProfile::ObjectV1,
    )?))
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum AffordanceError {
    Semantic(SemanticError),
    Object(ObjectError),
    Limit,
    InvalidField(&'static str),
    UnsupportedVersion,
    NonCanonicalValue,
}

impl From<SemanticError> for AffordanceError {
    fn from(error: SemanticError) -> Self {
        Self::Semantic(error)
    }
}

impl From<ObjectError> for AffordanceError {
    fn from(error: ObjectError) -> Self {
        Self::Object(error)
    }
}

impl From<super::canonical::CanonicalError> for AffordanceError {
    fn from(error: super::canonical::CanonicalError) -> Self {
        Self::Semantic(SemanticError::Canonical(error))
    }
}

/// Deterministic virtual `KnowledgeAffordance` projection (`virtual-sfs-affordance/1`)
/// over a saved Knowledge Unit's `SemanticFrameSet` (consumed by P3 active discovery
/// and composition engines when an explicit `KnowledgeAffordance` object has not yet
/// been authored).
///
/// Extracts:
/// - deduplicated statement `operator_or_predicate` CCIDs as `offered_roles`,
/// - `QuantityLiteral` / `DimensionVector` / `Range` statements as `semantics.properties`
///   (attaching explicit `ConstraintExpression::Dimension` constraints for quantity terms),
/// - non-default `StatementQualifiers` statements as `semantics.operating_conditions`.
pub fn project_virtual_sfs_affordance(
    ku_ref: ObjectReference,
    semantic: &SemanticFrameSet,
) -> Result<KnowledgeAffordance, AffordanceError> {
    if ku_ref.cid == [0; 32] || semantic.statements.is_empty() {
        return Err(AffordanceError::InvalidField(
            "virtual_sfs_affordance.input",
        ));
    }

    let mut stripped = semantic.clone();
    for stmt in &mut stripped.statements {
        stmt.qualifiers.source_spans.clear();
    }
    let normalized = stripped.alpha_normalized()?;

    let mut role_set = BTreeSet::new();
    let mut property_ids = BTreeSet::new();
    let mut condition_ids = BTreeSet::new();

    for stmt in &normalized.statements {
        role_set.insert(stmt.operator_or_predicate);
        if statement_has_typed_property(stmt) {
            property_ids.insert(stmt.statement_id);
        }
        if statement_has_operating_condition(&stmt.qualifiers) {
            condition_ids.insert(stmt.statement_id);
        }
    }

    let properties = build_projected_subset(&normalized, &property_ids, true)?;
    let operating_conditions = build_projected_subset(&normalized, &condition_ids, false)?;
    let empty = SemanticFrameSet {
        statements: Vec::new(),
    };

    let affordance = KnowledgeAffordance {
        sources: vec![ku_ref.clone()],
        offered_roles: role_set.into_iter().collect(),
        accepted_inputs: Vec::new(),
        semantics: AffordanceSemantics {
            preconditions: empty.clone(),
            outputs: normalized,
            effects: empty.clone(),
            properties,
            invariants: empty.clone(),
            operating_conditions,
            limits: empty,
        },
        abstraction_patterns: Vec::new(),
        origin: AffordanceOrigin::Derived {
            derivation_engine: virtual_affordance_ref(
                b"onebrain:vnext:virtual-sfs-affordance:engine:1\0",
            ),
            derivation_rule: virtual_affordance_ref(
                b"onebrain:vnext:virtual-sfs-affordance:rule:1\0",
            ),
            inputs: vec![ku_ref],
        },
    };
    affordance.canonical_payload()?;
    Ok(affordance)
}

fn virtual_affordance_ref(domain: &[u8]) -> ObjectReference {
    let mut hasher = blake3::Hasher::new();
    hasher.update(domain);
    hasher.update(VIRTUAL_SFS_AFFORDANCE_PROFILE.as_bytes());
    ObjectReference::new(0, *hasher.finalize().as_bytes())
}

fn term_has_quantity(term: &TermRef) -> bool {
    matches!(term, TermRef::Literal(LiteralValue::Quantity(_)))
}

fn constraint_has_quantity_or_dimension(constraint: &TypedConstraint) -> bool {
    match &constraint.expression {
        ConstraintExpression::Compare { left, right, .. } => {
            term_has_quantity(left) || term_has_quantity(right)
        }
        ConstraintExpression::Dimension { .. } | ConstraintExpression::Range { .. } => true,
    }
}

fn statement_has_typed_property(stmt: &StatementFrame) -> bool {
    stmt.arguments.iter().any(term_has_quantity)
        || stmt
            .constraints
            .iter()
            .any(constraint_has_quantity_or_dimension)
        || stmt.qualifiers.tolerance.is_some()
}

fn statement_has_operating_condition(qualifiers: &StatementQualifiers) -> bool {
    qualifiers.negated
        || qualifiers.modality != Modality::Asserted
        || qualifiers.condition.is_some()
        || qualifiers.time.is_some()
        || qualifiers.location.is_some()
        || qualifiers.perspective.is_some()
        || qualifiers.tolerance.is_some()
}

fn build_projected_subset(
    normalized: &SemanticFrameSet,
    initial_ids: &BTreeSet<StatementId>,
    attach_dimension_constraints: bool,
) -> Result<SemanticFrameSet, AffordanceError> {
    if initial_ids.is_empty() {
        return Ok(SemanticFrameSet {
            statements: Vec::new(),
        });
    }
    let stmt_map: BTreeMap<StatementId, &StatementFrame> = normalized
        .statements
        .iter()
        .map(|s| (s.statement_id, s))
        .collect();

    // Transitively include any referenced `qualifiers.condition` statements so
    // `SemanticFrameSet::alpha_normalized()` never fails on dangling statement IDs.
    let mut included = initial_ids.clone();
    let mut queue: Vec<StatementId> = initial_ids.iter().copied().collect();
    while let Some(id) = queue.pop() {
        if let Some(stmt) = stmt_map.get(&id) {
            if let Some(cond_id) = stmt.qualifiers.condition {
                if included.insert(cond_id) {
                    queue.push(cond_id);
                }
            }
        }
    }

    let id_remap: BTreeMap<StatementId, StatementId> = included
        .iter()
        .copied()
        .enumerate()
        .map(|(new_idx, old_id)| (old_id, StatementId(new_idx as u32)))
        .collect();

    let mut statements = Vec::with_capacity(included.len());
    for old_id in &included {
        let Some(orig) = stmt_map.get(old_id) else {
            continue;
        };
        let mut cloned = (*orig).clone();
        cloned.statement_id = id_remap[old_id];
        if let Some(cond_id) = cloned.qualifiers.condition {
            cloned.qualifiers.condition = id_remap.get(&cond_id).copied();
        }
        if attach_dimension_constraints && initial_ids.contains(old_id) {
            let mut extra = Vec::new();
            for arg in &cloned.arguments {
                if let TermRef::Literal(LiteralValue::Quantity(q)) = arg {
                    let dim_constraint = TypedConstraint {
                        expression: ConstraintExpression::Dimension {
                            term: arg.clone(),
                            expected: q.source_unit.dimension,
                        },
                        required: true,
                    };
                    if !cloned.constraints.contains(&dim_constraint)
                        && !extra.contains(&dim_constraint)
                    {
                        extra.push(dim_constraint);
                    }
                }
            }
            cloned.constraints.extend(extra);
        }
        statements.push(cloned);
    }

    Ok(SemanticFrameSet { statements }.alpha_normalized()?)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::foundation::{
        decode_knowledge_object, ConstraintExpression, DimensionVector, ExactRatio,
        KnownObjectKind, QuantityLiteral, SourceSpan, StatementFrame, StatementId,
        StatementQualifiers, TermRef, TypedConstraint, UnitRef,
    };

    fn concept(byte: u8) -> ConceptCcid {
        ConceptCcid::from_bytes([byte; 16])
    }

    fn reference(byte: u8) -> ObjectReference {
        ObjectReference::new(0, [byte; 32])
    }

    fn frames(marker: u8) -> SemanticFrameSet {
        SemanticFrameSet {
            statements: vec![StatementFrame {
                statement_id: StatementId(marker as u32),
                operator_or_predicate: concept(marker),
                arguments: vec![TermRef::Concept(concept(marker + 1))],
                constraints: vec![TypedConstraint {
                    expression: ConstraintExpression::Compare {
                        left: TermRef::Concept(concept(marker + 1)),
                        operator: super::super::semantic::ComparisonOperator::Equal,
                        right: TermRef::Concept(concept(marker + 2)),
                    },
                    required: true,
                }],
                qualifiers: StatementQualifiers::default(),
            }],
        }
    }

    fn affordance(sources: Vec<ObjectReference>, roles: Vec<ConceptCcid>) -> KnowledgeAffordance {
        KnowledgeAffordance {
            sources,
            offered_roles: roles,
            accepted_inputs: vec![AcceptedInput {
                receptor_definition: reference(9),
                role: concept(8),
                required: true,
            }],
            semantics: AffordanceSemantics {
                preconditions: frames(10),
                outputs: frames(20),
                effects: frames(30),
                properties: frames(40),
                invariants: frames(50),
                operating_conditions: frames(60),
                limits: frames(70),
            },
            abstraction_patterns: vec![frames(80)],
            origin: AffordanceOrigin::Explicit {
                claims: vec![StatementLocator {
                    object: reference(1),
                    statement_index: 0,
                }],
            },
        }
    }

    #[test]
    fn set_insertion_order_does_not_change_affordance_cid() {
        let left = affordance(
            vec![reference(2), reference(1)],
            vec![concept(2), concept(1)],
        )
        .to_knowledge_object(DisclosureClass::Public)
        .unwrap()
        .encode(ResourceProfile::ObjectV1)
        .unwrap();
        let right = affordance(
            vec![reference(1), reference(2)],
            vec![concept(1), concept(2)],
        )
        .to_knowledge_object(DisclosureClass::Public)
        .unwrap()
        .encode(ResourceProfile::ObjectV1)
        .unwrap();
        assert_eq!(left, right);
    }

    #[test]
    fn derived_origin_retains_engine_rule_and_inputs_in_identity() {
        let explicit = affordance(vec![reference(1)], vec![concept(1)])
            .to_knowledge_object(DisclosureClass::Public)
            .unwrap()
            .encode(ResourceProfile::ObjectV1)
            .unwrap();
        let mut derived = affordance(vec![reference(1)], vec![concept(1)]);
        derived.origin = AffordanceOrigin::Derived {
            derivation_engine: reference(6),
            derivation_rule: reference(7),
            inputs: vec![reference(1)],
        };
        let derived = derived
            .to_knowledge_object(DisclosureClass::Public)
            .unwrap()
            .encode(ResourceProfile::ObjectV1)
            .unwrap();
        assert_ne!(explicit.1, derived.1);
    }

    #[test]
    fn only_explicitly_offered_role_is_supported() {
        let affordance = affordance(vec![reference(1)], vec![concept(1)]);
        assert!(affordance.supports_role(concept(1)));
        assert!(!affordance.supports_role(concept(2)));
    }

    #[test]
    fn duplicate_source_is_rejected_instead_of_inflating_provenance() {
        let error = affordance(vec![reference(1), reference(1)], vec![concept(1)])
            .canonical_payload()
            .unwrap_err();
        assert!(matches!(error, AffordanceError::Semantic(_)));
    }

    #[test]
    fn validated_public_affordance_round_trips_into_typed_semantics() {
        let original = affordance(vec![reference(1)], vec![concept(1)]);
        let object = original
            .to_knowledge_object(DisclosureClass::Public)
            .unwrap();
        let (bytes, _) = object.encode(ResourceProfile::ObjectV1).unwrap();
        let validated = decode_knowledge_object(
            &bytes,
            ResourceProfile::ObjectV1,
            &[KnownObjectKind::new(KNOWLEDGE_AFFORDANCE_KIND, 1)],
            &[],
        )
        .unwrap();
        let decoded = KnowledgeAffordance::from_validated_object(&validated).unwrap();
        assert_eq!(
            decoded.canonical_payload().unwrap(),
            original.canonical_payload().unwrap()
        );

        let mut noncanonical = original.canonical_payload().unwrap();
        let CanonicalValue::Map(fields) = &mut noncanonical else {
            panic!("affordance payload must be a map");
        };
        fields.push((99, CanonicalValue::Null));
        assert_eq!(
            KnowledgeAffordance::from_canonical_payload(&noncanonical).unwrap_err(),
            AffordanceError::NonCanonicalValue
        );
    }

    #[test]
    fn project_virtual_sfs_affordance_extracts_roles_typed_properties_and_operating_conditions() {
        let ku = reference(42);
        let qty = QuantityLiteral {
            value: ExactRatio::new(100, 1).unwrap(),
            source_unit: UnitRef {
                unit: concept(6),
                dimension: DimensionVector::new([0, 0, 0, 0, 1, 0, 0]),
                scale_to_base: ExactRatio::new(1, 1).unwrap(),
                offset_to_base: ExactRatio::new(27315, 100).unwrap(),
            },
        };
        let cond_stmt = StatementFrame {
            statement_id: StatementId(10),
            operator_or_predicate: concept(3),
            arguments: vec![TermRef::Concept(concept(4))],
            constraints: vec![],
            qualifiers: StatementQualifiers {
                source_spans: vec![SourceSpan {
                    source: reference(1),
                    start: 0,
                    end: 12,
                }],
                ..StatementQualifiers::default()
            },
        };
        let main_stmt = StatementFrame {
            statement_id: StatementId(20),
            operator_or_predicate: concept(2),
            arguments: vec![
                TermRef::Concept(concept(1)),
                TermRef::Literal(LiteralValue::Quantity(qty)),
            ],
            constraints: vec![],
            qualifiers: StatementQualifiers {
                condition: Some(StatementId(10)),
                source_spans: vec![SourceSpan {
                    source: reference(1),
                    start: 13,
                    end: 35,
                }],
                ..StatementQualifiers::default()
            },
        };
        let sfs = SemanticFrameSet {
            statements: vec![cond_stmt, main_stmt],
        };

        let projected = project_virtual_sfs_affordance(ku.clone(), &sfs).unwrap();
        assert!(projected.supports_role(concept(2)));
        assert!(projected.supports_role(concept(3)));
        assert_eq!(projected.sources, vec![ku.clone()]);
        assert_eq!(projected.semantics.properties.statements.len(), 2);
        assert!(projected.semantics.properties.statements.iter().any(|s| s
            .constraints
            .iter()
            .any(|c| matches!(
                c.expression,
                ConstraintExpression::Dimension { expected, .. }
                if expected == DimensionVector::new([0, 0, 0, 0, 1, 0, 0])
            ))));
        assert_eq!(projected.semantics.operating_conditions.statements.len(), 2);
        assert!(matches!(
            &projected.origin,
            AffordanceOrigin::Derived { inputs, .. } if inputs == &vec![ku]
        ));
    }
}

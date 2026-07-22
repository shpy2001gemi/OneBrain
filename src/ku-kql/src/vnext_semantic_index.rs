//! Rebuildable semantic postings over immutable Receptor/Affordance sources.

use std::collections::{BTreeMap, BTreeSet};

use ku_core::foundation::{
    AffordanceSemantics, ComparisonOperator, ConceptCcid, ConstraintExpression, DimensionVector,
    KnowledgeAffordance, LiteralValue, Modality, ObjectCid, QuantityLiteral, ReceptorDefinition,
    SemanticFrameSet, StatementFrame, TermRef, TypedConstraint,
};

pub const SEMANTIC_INDEX_REDUCER_VERSION: u64 = 1;

#[derive(Clone, Copy)]
pub enum SemanticIndexSource<'a> {
    Receptor {
        cid: ObjectCid,
        definition: &'a ReceptorDefinition,
    },
    Affordance {
        cid: ObjectCid,
        affordance: &'a KnowledgeAffordance,
    },
}

impl SemanticIndexSource<'_> {
    fn cid(self) -> ObjectCid {
        match self {
            Self::Receptor { cid, .. } | Self::Affordance { cid, .. } => cid,
        }
    }

    fn kind_code(self) -> u8 {
        match self {
            Self::Receptor { .. } => 0,
            Self::Affordance { .. } => 1,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct SemanticIndexSnapshot {
    pub source_root: [u8; 32],
    pub projection_root: [u8; 32],
    pub reducer_version: u64,
    pub source_count: u64,
}

#[derive(Clone, Default)]
pub struct RebuildableSemanticIndex {
    receptor_roles: BTreeMap<[u8; 16], BTreeSet<[u8; 32]>>,
    affordance_roles: BTreeMap<[u8; 16], BTreeSet<[u8; 32]>>,
    concepts: BTreeMap<[u8; 16], BTreeSet<[u8; 32]>>,
    predicates: BTreeMap<[u8; 16], BTreeSet<[u8; 32]>>,
    comparison_operators: BTreeMap<u64, BTreeSet<[u8; 32]>>,
    units: BTreeMap<[u8; 16], BTreeSet<[u8; 32]>>,
    dimensions: BTreeMap<[i8; 7], BTreeSet<[u8; 32]>>,
    relation_signatures: BTreeMap<[u8; 32], BTreeSet<[u8; 32]>>,
    snapshot: SemanticIndexSnapshot,
}

impl Default for SemanticIndexSnapshot {
    fn default() -> Self {
        Self {
            source_root: [0; 32],
            projection_root: [0; 32],
            reducer_version: SEMANTIC_INDEX_REDUCER_VERSION,
            source_count: 0,
        }
    }
}

impl RebuildableSemanticIndex {
    pub fn rebuild(sources: &[SemanticIndexSource<'_>]) -> Self {
        let mut index = Self::default();
        let mut source_keys = BTreeSet::new();
        for source in sources {
            let cid = source.cid().into_bytes();
            source_keys.insert((source.kind_code(), cid));
            match source {
                SemanticIndexSource::Receptor { definition, .. } => {
                    insert(&mut index.receptor_roles, *definition.role.as_bytes(), cid);
                    collect_concept(&mut index, definition.role, cid);
                    for expected in &definition.expected_types {
                        collect_concept(&mut index, *expected, cid);
                    }
                    for evidence in &definition.acceptance.required_evidence_kinds {
                        collect_concept(&mut index, *evidence, cid);
                    }
                    for constraint in &definition.hard_constraints {
                        collect_constraint(&mut index, constraint, cid);
                    }
                }
                SemanticIndexSource::Affordance { affordance, .. } => {
                    for role in &affordance.offered_roles {
                        insert(&mut index.affordance_roles, *role.as_bytes(), cid);
                        collect_concept(&mut index, *role, cid);
                    }
                    for input in &affordance.accepted_inputs {
                        collect_concept(&mut index, input.role, cid);
                    }
                    collect_affordance_semantics(&mut index, &affordance.semantics, cid);
                    for pattern in &affordance.abstraction_patterns {
                        collect_frames(&mut index, pattern, cid);
                    }
                }
            }
        }
        index.snapshot = SemanticIndexSnapshot {
            source_root: digest_source_keys(&source_keys),
            projection_root: index.projection_root(),
            reducer_version: SEMANTIC_INDEX_REDUCER_VERSION,
            source_count: source_keys.len() as u64,
        };
        index
    }

    pub const fn snapshot(&self) -> SemanticIndexSnapshot {
        self.snapshot
    }

    pub fn receptors_for_role(&self, role: ConceptCcid) -> Vec<ObjectCid> {
        postings(&self.receptor_roles, role.as_bytes())
    }

    pub fn affordances_for_role(&self, role: ConceptCcid) -> Vec<ObjectCid> {
        postings(&self.affordance_roles, role.as_bytes())
    }

    pub fn objects_for_concept(&self, concept: ConceptCcid) -> Vec<ObjectCid> {
        postings(&self.concepts, concept.as_bytes())
    }

    pub fn objects_for_predicate(&self, predicate: ConceptCcid) -> Vec<ObjectCid> {
        postings(&self.predicates, predicate.as_bytes())
    }

    pub fn objects_for_comparison(&self, operator: ComparisonOperator) -> Vec<ObjectCid> {
        postings(&self.comparison_operators, &(operator as u64))
    }

    pub fn objects_for_unit(&self, unit: ConceptCcid) -> Vec<ObjectCid> {
        postings(&self.units, unit.as_bytes())
    }

    pub fn objects_for_dimension(&self, dimension: DimensionVector) -> Vec<ObjectCid> {
        postings(&self.dimensions, &dimension.exponents())
    }

    pub fn objects_for_relation_signature(&self, signature: [u8; 32]) -> Vec<ObjectCid> {
        postings(&self.relation_signatures, &signature)
    }

    pub fn clear_derived(&mut self) {
        *self = Self::default();
    }

    fn projection_root(&self) -> [u8; 32] {
        let mut hasher = blake3::Hasher::new();
        hasher.update(b"onebrain:vnext:semantic-index-projection:1\0");
        hash_postings(&mut hasher, 0, &self.receptor_roles, |key| key.to_vec());
        hash_postings(&mut hasher, 1, &self.affordance_roles, |key| key.to_vec());
        hash_postings(&mut hasher, 2, &self.concepts, |key| key.to_vec());
        hash_postings(&mut hasher, 3, &self.predicates, |key| key.to_vec());
        hash_postings(&mut hasher, 4, &self.comparison_operators, |key| {
            key.to_be_bytes().to_vec()
        });
        hash_postings(&mut hasher, 5, &self.units, |key| key.to_vec());
        hash_postings(&mut hasher, 6, &self.dimensions, |key| {
            key.iter()
                .map(|value| (*value as i16 + 128) as u8)
                .collect()
        });
        hash_postings(&mut hasher, 7, &self.relation_signatures, |key| {
            key.to_vec()
        });
        *hasher.finalize().as_bytes()
    }
}

fn collect_affordance_semantics(
    index: &mut RebuildableSemanticIndex,
    semantics: &AffordanceSemantics,
    cid: [u8; 32],
) {
    for frames in [
        &semantics.preconditions,
        &semantics.outputs,
        &semantics.effects,
        &semantics.properties,
        &semantics.invariants,
        &semantics.operating_conditions,
        &semantics.limits,
    ] {
        collect_frames(index, frames, cid);
    }
}

fn collect_frames(index: &mut RebuildableSemanticIndex, frames: &SemanticFrameSet, cid: [u8; 32]) {
    for statement in &frames.statements {
        collect_statement(index, statement, cid);
    }
}

fn collect_statement(
    index: &mut RebuildableSemanticIndex,
    statement: &StatementFrame,
    cid: [u8; 32],
) {
    collect_concept(index, statement.operator_or_predicate, cid);
    insert(
        &mut index.predicates,
        *statement.operator_or_predicate.as_bytes(),
        cid,
    );
    insert(
        &mut index.relation_signatures,
        relation_signature(statement),
        cid,
    );
    for argument in &statement.arguments {
        collect_term(index, argument, cid);
    }
    for constraint in &statement.constraints {
        collect_constraint(index, constraint, cid);
    }
    for term in [
        statement.qualifiers.time.as_ref(),
        statement.qualifiers.location.as_ref(),
        statement.qualifiers.perspective.as_ref(),
    ]
    .into_iter()
    .flatten()
    {
        collect_term(index, term, cid);
    }
    if let Some(tolerance) = &statement.qualifiers.tolerance {
        collect_quantity(index, tolerance, cid);
    }
}

fn collect_constraint(
    index: &mut RebuildableSemanticIndex,
    constraint: &TypedConstraint,
    cid: [u8; 32],
) {
    match &constraint.expression {
        ConstraintExpression::Compare {
            left,
            operator,
            right,
        } => {
            insert(&mut index.comparison_operators, *operator as u64, cid);
            collect_term(index, left, cid);
            collect_term(index, right, cid);
        }
        ConstraintExpression::Dimension { term, expected } => {
            insert(&mut index.dimensions, expected.exponents(), cid);
            collect_term(index, term, cid);
        }
        ConstraintExpression::Range {
            term, lower, upper, ..
        } => {
            collect_term(index, term, cid);
            collect_quantity(index, lower, cid);
            collect_quantity(index, upper, cid);
        }
    }
}

fn collect_term(index: &mut RebuildableSemanticIndex, term: &TermRef, cid: [u8; 32]) {
    match term {
        TermRef::Concept(concept) => collect_concept(index, *concept, cid),
        TermRef::Variable {
            type_constraint: Some(concept),
            ..
        }
        | TermRef::Receptor {
            expected_type: Some(concept),
            ..
        } => collect_concept(index, *concept, cid),
        TermRef::Literal(LiteralValue::Quantity(quantity)) => {
            collect_quantity(index, quantity, cid)
        }
        TermRef::Variable { .. }
        | TermRef::Literal(_)
        | TermRef::Statement(_)
        | TermRef::KnowledgeObject(_)
        | TermRef::Receptor { .. } => {}
    }
}

fn collect_quantity(
    index: &mut RebuildableSemanticIndex,
    quantity: &QuantityLiteral,
    cid: [u8; 32],
) {
    collect_concept(index, quantity.source_unit.unit, cid);
    insert(&mut index.units, *quantity.source_unit.unit.as_bytes(), cid);
    insert(
        &mut index.dimensions,
        quantity.source_unit.dimension.exponents(),
        cid,
    );
}

fn collect_concept(index: &mut RebuildableSemanticIndex, concept: ConceptCcid, cid: [u8; 32]) {
    insert(&mut index.concepts, *concept.as_bytes(), cid);
}

fn relation_signature(statement: &StatementFrame) -> [u8; 32] {
    let mut hasher = blake3::Hasher::new();
    hasher.update(b"onebrain:vnext:relation-signature:1\0");
    hasher.update(statement.operator_or_predicate.as_bytes());
    hasher.update(&(statement.arguments.len() as u64).to_be_bytes());
    hasher.update(&[u8::from(statement.qualifiers.negated)]);
    hasher.update(&[modality_code(statement.qualifiers.modality)]);
    for argument in &statement.arguments {
        hasher.update(&[term_shape(argument)]);
    }
    *hasher.finalize().as_bytes()
}

fn modality_code(modality: Modality) -> u8 {
    match modality {
        Modality::Asserted => 0,
        Modality::Observed => 1,
        Modality::Reported => 2,
        Modality::Possible => 3,
        Modality::Necessary => 4,
        Modality::Desired => 5,
    }
}

fn term_shape(term: &TermRef) -> u8 {
    match term {
        TermRef::Concept(_) => 0,
        TermRef::Variable { .. } => 1,
        TermRef::Literal(LiteralValue::Boolean(_)) => 2,
        TermRef::Literal(LiteralValue::Text(_)) => 3,
        TermRef::Literal(LiteralValue::Quantity(_)) => 4,
        TermRef::Literal(LiteralValue::Bytes(_)) => 5,
        TermRef::Statement(_) => 6,
        TermRef::KnowledgeObject(_) => 7,
        TermRef::Receptor { .. } => 8,
    }
}

fn insert<K: Ord>(map: &mut BTreeMap<K, BTreeSet<[u8; 32]>>, key: K, cid: [u8; 32]) {
    map.entry(key).or_default().insert(cid);
}

fn postings<K: Ord>(map: &BTreeMap<K, BTreeSet<[u8; 32]>>, key: &K) -> Vec<ObjectCid> {
    map.get(key)
        .into_iter()
        .flatten()
        .copied()
        .map(ObjectCid::from_bytes)
        .collect()
}

fn digest_source_keys(source_keys: &BTreeSet<(u8, [u8; 32])>) -> [u8; 32] {
    let mut hasher = blake3::Hasher::new();
    hasher.update(b"onebrain:vnext:semantic-index-sources:1\0");
    for (kind, cid) in source_keys {
        hasher.update(&[*kind]);
        hasher.update(cid);
    }
    *hasher.finalize().as_bytes()
}

fn hash_postings<K: Ord>(
    hasher: &mut blake3::Hasher,
    namespace: u8,
    map: &BTreeMap<K, BTreeSet<[u8; 32]>>,
    key_bytes: impl Fn(&K) -> Vec<u8>,
) {
    hasher.update(&[namespace]);
    hasher.update(&(map.len() as u64).to_be_bytes());
    for (key, postings) in map {
        let key = key_bytes(key);
        hasher.update(&(key.len() as u64).to_be_bytes());
        hasher.update(&key);
        hasher.update(&(postings.len() as u64).to_be_bytes());
        for cid in postings {
            hasher.update(cid);
        }
    }
}

#[cfg(test)]
mod tests {
    use ku_core::foundation::{
        AcceptedInput, AffordanceOrigin, DisclosureClass, ExactRatio, ObjectReference,
        ReceptorAcceptanceProfile, ReceptorCardinality, ReceptorOrigin, ResourceProfile,
        StatementId, StatementLocator, StatementQualifiers, UnitRef, UnknownConstraintPolicy,
    };

    use super::*;

    fn concept(byte: u8) -> ConceptCcid {
        ConceptCcid::from_bytes([byte; 16])
    }

    fn reference(byte: u8) -> ObjectReference {
        ObjectReference::new(0, [byte; 32])
    }

    fn frame() -> StatementFrame {
        let unit = UnitRef::coherent(concept(9), DimensionVector::LENGTH);
        StatementFrame {
            statement_id: StatementId(1),
            operator_or_predicate: concept(3),
            arguments: vec![
                TermRef::Concept(concept(4)),
                TermRef::Literal(LiteralValue::Quantity(QuantityLiteral {
                    value: ExactRatio::integer(2),
                    source_unit: unit.clone(),
                })),
            ],
            constraints: vec![TypedConstraint {
                expression: ConstraintExpression::Range {
                    term: TermRef::Concept(concept(4)),
                    lower: QuantityLiteral {
                        value: ExactRatio::integer(1),
                        source_unit: unit.clone(),
                    },
                    upper: QuantityLiteral {
                        value: ExactRatio::integer(3),
                        source_unit: unit,
                    },
                    include_lower: true,
                    include_upper: true,
                },
                required: true,
            }],
            qualifiers: StatementQualifiers::default(),
        }
    }

    fn receptor() -> (ObjectCid, ReceptorDefinition) {
        let definition = ReceptorDefinition {
            role: concept(1),
            expected_types: vec![concept(2)],
            hard_constraints: vec![TypedConstraint {
                expression: ConstraintExpression::Compare {
                    left: TermRef::Concept(concept(2)),
                    operator: ComparisonOperator::GreaterThan,
                    right: TermRef::Concept(concept(5)),
                },
                required: true,
            }],
            cardinality: ReceptorCardinality::new(1, Some(1)).unwrap(),
            origin: ReceptorOrigin::Declared {
                source: StatementLocator {
                    object: reference(10),
                    statement_index: 0,
                },
            },
            acceptance: ReceptorAcceptanceProfile {
                policy: reference(11),
                required_evidence_kinds: vec![concept(6)],
                unknown_constraint_policy: UnknownConstraintPolicy::KeepUnresolved,
            },
        };
        let cid = definition
            .to_knowledge_object(DisclosureClass::Public)
            .unwrap()
            .encode(ResourceProfile::ObjectV1)
            .unwrap()
            .1;
        (cid, definition)
    }

    fn affordance() -> (ObjectCid, KnowledgeAffordance) {
        let empty = SemanticFrameSet {
            statements: Vec::new(),
        };
        let frames = SemanticFrameSet {
            statements: vec![frame()],
        };
        let affordance = KnowledgeAffordance {
            sources: vec![reference(20)],
            offered_roles: vec![concept(1)],
            accepted_inputs: vec![AcceptedInput {
                receptor_definition: reference(21),
                role: concept(7),
                required: true,
            }],
            semantics: AffordanceSemantics {
                preconditions: empty.clone(),
                outputs: frames,
                effects: empty.clone(),
                properties: empty.clone(),
                invariants: empty.clone(),
                operating_conditions: empty.clone(),
                limits: empty,
            },
            abstraction_patterns: Vec::new(),
            origin: AffordanceOrigin::Explicit {
                claims: vec![StatementLocator {
                    object: reference(20),
                    statement_index: 0,
                }],
            },
        };
        let cid = affordance
            .to_knowledge_object(DisclosureClass::Public)
            .unwrap()
            .encode(ResourceProfile::ObjectV1)
            .unwrap()
            .1;
        (cid, affordance)
    }

    #[test]
    fn role_ccid_operator_unit_dimension_and_relation_postings_are_derived() {
        let (receptor_cid, receptor) = receptor();
        let (affordance_cid, affordance) = affordance();
        let sources = [
            SemanticIndexSource::Receptor {
                cid: receptor_cid,
                definition: &receptor,
            },
            SemanticIndexSource::Affordance {
                cid: affordance_cid,
                affordance: &affordance,
            },
        ];
        let index = RebuildableSemanticIndex::rebuild(&sources);
        assert_eq!(index.receptors_for_role(concept(1)), vec![receptor_cid]);
        assert_eq!(index.affordances_for_role(concept(1)), vec![affordance_cid]);
        assert!(index
            .objects_for_concept(concept(4))
            .contains(&affordance_cid));
        assert_eq!(
            index.objects_for_predicate(concept(3)),
            vec![affordance_cid]
        );
        assert_eq!(index.objects_for_unit(concept(9)), vec![affordance_cid]);
        assert_eq!(
            index.objects_for_dimension(DimensionVector::LENGTH),
            vec![affordance_cid]
        );
        assert_eq!(
            index.objects_for_comparison(ComparisonOperator::GreaterThan),
            vec![receptor_cid]
        );
        let signature = relation_signature(&frame());
        assert_eq!(
            index.objects_for_relation_signature(signature),
            vec![affordance_cid]
        );
    }

    #[test]
    fn rebuild_is_order_and_restart_stable() {
        let (receptor_cid, receptor) = receptor();
        let (affordance_cid, affordance) = affordance();
        let forward = [
            SemanticIndexSource::Receptor {
                cid: receptor_cid,
                definition: &receptor,
            },
            SemanticIndexSource::Affordance {
                cid: affordance_cid,
                affordance: &affordance,
            },
        ];
        let reverse = [forward[1], forward[0]];
        let first = RebuildableSemanticIndex::rebuild(&forward).snapshot();
        let restarted = RebuildableSemanticIndex::rebuild(&reverse).snapshot();
        assert_eq!(first, restarted);
        assert_ne!(first.source_root, [0; 32]);
        assert_ne!(first.projection_root, [0; 32]);
    }

    #[test]
    fn clearing_index_does_not_mutate_source_objects_and_rebuild_restores_root() {
        let (receptor_cid, receptor) = receptor();
        let source = [SemanticIndexSource::Receptor {
            cid: receptor_cid,
            definition: &receptor,
        }];
        let mut index = RebuildableSemanticIndex::rebuild(&source);
        let root = index.snapshot();
        index.clear_derived();
        assert_eq!(index.snapshot().source_count, 0);
        assert_eq!(receptor.role, concept(1));
        assert_eq!(RebuildableSemanticIndex::rebuild(&source).snapshot(), root);
    }
}

//! Rebuildable multi-channel structural signatures for analogy candidate recall.
//!
//! Vocabulary-sensitive CCID-role signatures coexist with vocabulary-neutral
//! FBS, operator, graph and unit/dimension structure. All outputs are recall
//! hints; exact typed validation remains authoritative for action eligibility.

use std::collections::{BTreeMap, BTreeSet};

use ku_core::foundation::{
    AffordanceSemantics, ComparisonOperator, ConstraintExpression, DimensionVector, ExactRatio,
    KnowledgeAffordance, LiteralValue, Modality, ObjectCid, QuantityLiteral, ReceptorDefinition,
    SemanticError, SemanticFrameSet, StatementFrame, TermRef, TypedConstraint, UnitRef,
};

pub const STRUCTURAL_SIGNATURE_VERSION: u64 = 1;
pub const MAX_SIGNATURES_PER_OBJECT: usize = 65_536;

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
#[repr(u8)]
pub enum StructuralSignatureKind {
    CcidRole = 0,
    FunctionBehaviorStructure = 1,
    OperatorAst = 2,
    GraphShingle = 3,
    Dimension = 4,
    UnitSemantics = 5,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum VocabularySensitivity {
    VocabularyIndependent,
    ExactCcid,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
#[repr(u8)]
pub enum FbsLayer {
    Function = 0,
    Behavior = 1,
    Structure = 2,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
#[repr(u8)]
pub enum SemanticBucket {
    ReceptorConstraint = 0,
    Preconditions = 1,
    Outputs = 2,
    Effects = 3,
    Properties = 4,
    Invariants = 5,
    OperatingConditions = 6,
    Limits = 7,
    Abstraction = 8,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum StructuralRegion {
    ReceptorRole,
    ReceptorExpectedType(u32),
    ReceptorConstraint(u32),
    OfferedRole(u32),
    AcceptedInput(u32),
    FrameSet {
        bucket: SemanticBucket,
    },
    Statement {
        bucket: SemanticBucket,
        statement: u32,
    },
    Constraint {
        bucket: SemanticBucket,
        statement: u32,
        constraint: u32,
    },
    Quantity {
        bucket: SemanticBucket,
        statement: u32,
        ordinal: u32,
    },
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub struct StructuralSignature {
    pub kind: StructuralSignatureKind,
    pub sensitivity: VocabularySensitivity,
    pub digest: [u8; 32],
    pub region: StructuralRegion,
}

impl StructuralSignature {
    pub const fn is_action_authority(&self) -> bool {
        false
    }
}

#[derive(Clone, Copy)]
pub enum StructuralSignatureSource<'a> {
    Receptor {
        cid: ObjectCid,
        definition: &'a ReceptorDefinition,
    },
    Affordance {
        cid: ObjectCid,
        affordance: &'a KnowledgeAffordance,
    },
}

impl StructuralSignatureSource<'_> {
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
pub struct StructuralSignatureSnapshot {
    pub source_root: [u8; 32],
    pub projection_root: [u8; 32],
    pub reducer_version: u64,
    pub source_count: u64,
    pub signature_count: u64,
}

impl Default for StructuralSignatureSnapshot {
    fn default() -> Self {
        Self {
            source_root: [0; 32],
            projection_root: [0; 32],
            reducer_version: STRUCTURAL_SIGNATURE_VERSION,
            source_count: 0,
            signature_count: 0,
        }
    }
}

#[derive(Clone, Default)]
pub struct StructuralSignatureIndex {
    by_object: BTreeMap<[u8; 32], BTreeSet<StructuralSignature>>,
    postings: BTreeMap<(StructuralSignatureKind, [u8; 32]), BTreeSet<[u8; 32]>>,
    snapshot: StructuralSignatureSnapshot,
}

impl StructuralSignatureIndex {
    pub fn rebuild(
        sources: &[StructuralSignatureSource<'_>],
    ) -> Result<Self, StructuralSignatureError> {
        let mut index = Self::default();
        let mut source_keys = BTreeSet::new();
        for source in sources {
            let cid = source.cid().into_bytes();
            source_keys.insert((source.kind_code(), cid));
            let signatures = extract_signatures(*source)?;
            if signatures.len() > MAX_SIGNATURES_PER_OBJECT {
                return Err(StructuralSignatureError::Limit);
            }
            for signature in signatures {
                index
                    .postings
                    .entry((signature.kind, signature.digest))
                    .or_default()
                    .insert(cid);
                index.by_object.entry(cid).or_default().insert(signature);
            }
        }
        let signature_count = index
            .by_object
            .values()
            .map(|signatures| signatures.len() as u64)
            .sum();
        index.snapshot = StructuralSignatureSnapshot {
            source_root: source_root(&source_keys),
            projection_root: index.projection_root(),
            reducer_version: STRUCTURAL_SIGNATURE_VERSION,
            source_count: source_keys.len() as u64,
            signature_count,
        };
        Ok(index)
    }

    pub const fn snapshot(&self) -> StructuralSignatureSnapshot {
        self.snapshot
    }

    pub fn signatures_for(&self, object: ObjectCid) -> Vec<StructuralSignature> {
        self.by_object
            .get(object.as_bytes())
            .into_iter()
            .flatten()
            .copied()
            .collect()
    }

    pub fn objects_for(&self, kind: StructuralSignatureKind, digest: [u8; 32]) -> Vec<ObjectCid> {
        self.postings
            .get(&(kind, digest))
            .into_iter()
            .flatten()
            .copied()
            .map(ObjectCid::from_bytes)
            .collect()
    }

    pub fn clear_derived(&mut self) {
        *self = Self::default();
    }

    fn projection_root(&self) -> [u8; 32] {
        let mut hasher = blake3::Hasher::new();
        hasher.update(b"onebrain:vnext:structural-signature-index:1\0");
        for (object, signatures) in &self.by_object {
            hasher.update(object);
            hasher.update(&(signatures.len() as u64).to_be_bytes());
            for signature in signatures {
                hash_signature(&mut hasher, signature);
            }
        }
        *hasher.finalize().as_bytes()
    }
}

fn extract_signatures(
    source: StructuralSignatureSource<'_>,
) -> Result<BTreeSet<StructuralSignature>, StructuralSignatureError> {
    let mut output = BTreeSet::new();
    match source {
        StructuralSignatureSource::Receptor { definition, .. } => {
            add_ccid_role(
                &mut output,
                0,
                definition.role.as_bytes(),
                StructuralRegion::ReceptorRole,
            );
            for (index, expected) in definition.expected_types.iter().enumerate() {
                add_ccid_role(
                    &mut output,
                    1,
                    expected.as_bytes(),
                    StructuralRegion::ReceptorExpectedType(index_u32(index)?),
                );
            }
            for (index, constraint) in definition.hard_constraints.iter().enumerate() {
                let region = StructuralRegion::ReceptorConstraint(index_u32(index)?);
                add_operator_signature(&mut output, constraint, region);
                collect_constraint_units(
                    &mut output,
                    constraint,
                    SemanticBucket::ReceptorConstraint,
                    0,
                    index_u32(index)?,
                );
            }
        }
        StructuralSignatureSource::Affordance { affordance, .. } => {
            for (index, role) in affordance.offered_roles.iter().enumerate() {
                add_ccid_role(
                    &mut output,
                    2,
                    role.as_bytes(),
                    StructuralRegion::OfferedRole(index_u32(index)?),
                );
            }
            for (index, input) in affordance.accepted_inputs.iter().enumerate() {
                add_ccid_role(
                    &mut output,
                    if input.required { 3 } else { 4 },
                    input.role.as_bytes(),
                    StructuralRegion::AcceptedInput(index_u32(index)?),
                );
            }
            collect_affordance(&mut output, &affordance.semantics)?;
            for pattern in &affordance.abstraction_patterns {
                collect_frames(
                    &mut output,
                    FbsLayer::Function,
                    SemanticBucket::Abstraction,
                    pattern,
                )?;
            }
        }
    }
    Ok(output)
}

fn collect_affordance(
    output: &mut BTreeSet<StructuralSignature>,
    semantics: &AffordanceSemantics,
) -> Result<(), StructuralSignatureError> {
    for (layer, bucket, frames) in [
        (
            FbsLayer::Behavior,
            SemanticBucket::Preconditions,
            &semantics.preconditions,
        ),
        (
            FbsLayer::Function,
            SemanticBucket::Outputs,
            &semantics.outputs,
        ),
        (
            FbsLayer::Behavior,
            SemanticBucket::Effects,
            &semantics.effects,
        ),
        (
            FbsLayer::Structure,
            SemanticBucket::Properties,
            &semantics.properties,
        ),
        (
            FbsLayer::Structure,
            SemanticBucket::Invariants,
            &semantics.invariants,
        ),
        (
            FbsLayer::Behavior,
            SemanticBucket::OperatingConditions,
            &semantics.operating_conditions,
        ),
        (
            FbsLayer::Structure,
            SemanticBucket::Limits,
            &semantics.limits,
        ),
    ] {
        collect_frames(output, layer, bucket, frames)?;
    }
    Ok(())
}

fn collect_frames(
    output: &mut BTreeSet<StructuralSignature>,
    layer: FbsLayer,
    bucket: SemanticBucket,
    frames: &SemanticFrameSet,
) -> Result<(), StructuralSignatureError> {
    if frames.statements.is_empty() {
        return Ok(());
    }
    let frames = frames.alpha_normalized()?;
    let skeletons = frames
        .statements
        .iter()
        .map(statement_skeleton)
        .collect::<Vec<_>>();
    let mut fbs_bytes = vec![layer as u8, bucket as u8];
    for skeleton in &skeletons {
        fbs_bytes.extend_from_slice(skeleton);
    }
    output.insert(StructuralSignature {
        kind: StructuralSignatureKind::FunctionBehaviorStructure,
        sensitivity: VocabularySensitivity::VocabularyIndependent,
        digest: digest(b"fbs", &fbs_bytes),
        region: StructuralRegion::FrameSet { bucket },
    });
    for (statement_index, statement) in frames.statements.iter().enumerate() {
        let statement_index = index_u32(statement_index)?;
        let region = StructuralRegion::Statement {
            bucket,
            statement: statement_index,
        };
        output.insert(StructuralSignature {
            kind: StructuralSignatureKind::GraphShingle,
            sensitivity: VocabularySensitivity::VocabularyIndependent,
            digest: graph_shingle(statement, &frames.statements),
            region,
        });
        for (constraint_index, constraint) in statement.constraints.iter().enumerate() {
            let constraint_index = index_u32(constraint_index)?;
            let region = StructuralRegion::Constraint {
                bucket,
                statement: statement_index,
                constraint: constraint_index,
            };
            add_operator_signature(output, constraint, region);
            collect_constraint_units(
                output,
                constraint,
                bucket,
                statement_index,
                constraint_index,
            );
        }
        let mut quantity_ordinal = 0u32;
        for term in &statement.arguments {
            collect_term_units(output, term, bucket, statement_index, &mut quantity_ordinal);
        }
        for term in [
            statement.qualifiers.time.as_ref(),
            statement.qualifiers.location.as_ref(),
            statement.qualifiers.perspective.as_ref(),
        ]
        .into_iter()
        .flatten()
        {
            collect_term_units(output, term, bucket, statement_index, &mut quantity_ordinal);
        }
        if let Some(tolerance) = &statement.qualifiers.tolerance {
            add_quantity_signatures(output, tolerance, bucket, statement_index, quantity_ordinal);
        }
    }
    Ok(())
}

fn add_ccid_role(
    output: &mut BTreeSet<StructuralSignature>,
    role_class: u8,
    ccid: &[u8; 16],
    region: StructuralRegion,
) {
    let mut bytes = vec![role_class];
    bytes.extend_from_slice(ccid);
    output.insert(StructuralSignature {
        kind: StructuralSignatureKind::CcidRole,
        sensitivity: VocabularySensitivity::ExactCcid,
        digest: digest(b"ccid-role", &bytes),
        region,
    });
}

fn add_operator_signature(
    output: &mut BTreeSet<StructuralSignature>,
    constraint: &TypedConstraint,
    region: StructuralRegion,
) {
    output.insert(StructuralSignature {
        kind: StructuralSignatureKind::OperatorAst,
        sensitivity: VocabularySensitivity::VocabularyIndependent,
        digest: digest(b"operator-ast", &constraint_skeleton(constraint)),
        region,
    });
}

fn collect_constraint_units(
    output: &mut BTreeSet<StructuralSignature>,
    constraint: &TypedConstraint,
    bucket: SemanticBucket,
    statement: u32,
    ordinal_seed: u32,
) {
    let mut ordinal = ordinal_seed.saturating_mul(4);
    match &constraint.expression {
        ConstraintExpression::Compare { left, right, .. } => {
            collect_term_units(output, left, bucket, statement, &mut ordinal);
            collect_term_units(output, right, bucket, statement, &mut ordinal);
        }
        ConstraintExpression::Dimension { term, expected } => {
            add_dimension_signature(
                output,
                *expected,
                StructuralRegion::Quantity {
                    bucket,
                    statement,
                    ordinal,
                },
            );
            ordinal = ordinal.saturating_add(1);
            collect_term_units(output, term, bucket, statement, &mut ordinal);
        }
        ConstraintExpression::Range {
            term, lower, upper, ..
        } => {
            collect_term_units(output, term, bucket, statement, &mut ordinal);
            add_quantity_signatures(output, lower, bucket, statement, ordinal);
            ordinal = ordinal.saturating_add(1);
            add_quantity_signatures(output, upper, bucket, statement, ordinal);
        }
    }
}

fn collect_term_units(
    output: &mut BTreeSet<StructuralSignature>,
    term: &TermRef,
    bucket: SemanticBucket,
    statement: u32,
    ordinal: &mut u32,
) {
    if let TermRef::Literal(LiteralValue::Quantity(quantity)) = term {
        add_quantity_signatures(output, quantity, bucket, statement, *ordinal);
        *ordinal = ordinal.saturating_add(1);
    }
}

fn add_quantity_signatures(
    output: &mut BTreeSet<StructuralSignature>,
    quantity: &QuantityLiteral,
    bucket: SemanticBucket,
    statement: u32,
    ordinal: u32,
) {
    let region = StructuralRegion::Quantity {
        bucket,
        statement,
        ordinal,
    };
    add_dimension_signature(output, quantity.source_unit.dimension, region);
    output.insert(StructuralSignature {
        kind: StructuralSignatureKind::UnitSemantics,
        sensitivity: VocabularySensitivity::VocabularyIndependent,
        digest: digest(b"unit-semantics", &unit_semantics(&quantity.source_unit)),
        region,
    });
}

fn add_dimension_signature(
    output: &mut BTreeSet<StructuralSignature>,
    dimension: DimensionVector,
    region: StructuralRegion,
) {
    output.insert(StructuralSignature {
        kind: StructuralSignatureKind::Dimension,
        sensitivity: VocabularySensitivity::VocabularyIndependent,
        digest: digest(b"dimension", &dimension_bytes(dimension)),
        region,
    });
}

fn statement_skeleton(statement: &StatementFrame) -> [u8; 32] {
    let mut bytes = vec![
        statement.arguments.len().min(u8::MAX as usize) as u8,
        u8::from(statement.qualifiers.negated),
        modality_code(statement.qualifiers.modality),
        u8::from(statement.qualifiers.condition.is_some()),
        u8::from(statement.qualifiers.time.is_some()),
        u8::from(statement.qualifiers.location.is_some()),
        u8::from(statement.qualifiers.perspective.is_some()),
        u8::from(statement.qualifiers.tolerance.is_some()),
    ];
    bytes.extend(statement.arguments.iter().map(term_shape));
    let mut constraints = statement
        .constraints
        .iter()
        .map(constraint_skeleton)
        .map(|bytes| digest(b"constraint-shape", &bytes))
        .collect::<Vec<_>>();
    constraints.sort();
    for constraint in constraints {
        bytes.extend_from_slice(&constraint);
    }
    digest(b"statement-skeleton", &bytes)
}

fn graph_shingle(statement: &StatementFrame, all: &[StatementFrame]) -> [u8; 32] {
    let mut bytes = statement_skeleton(statement).to_vec();
    for (argument, term) in statement.arguments.iter().enumerate() {
        if let TermRef::Statement(target) = term {
            bytes.extend_from_slice(&(argument as u64).to_be_bytes());
            if let Some(target) = all
                .iter()
                .find(|candidate| candidate.statement_id == *target)
            {
                bytes.extend_from_slice(&statement_skeleton(target));
            } else {
                bytes.extend_from_slice(&[0; 32]);
            }
        }
    }
    digest(b"graph-shingle", &bytes)
}

fn constraint_skeleton(constraint: &TypedConstraint) -> Vec<u8> {
    let mut bytes = vec![u8::from(constraint.required)];
    match &constraint.expression {
        ConstraintExpression::Compare {
            left,
            operator,
            right,
        } => {
            bytes.extend_from_slice(&[
                0,
                comparison_code(*operator),
                term_shape(left),
                term_shape(right),
            ]);
        }
        ConstraintExpression::Dimension { term, expected } => {
            bytes.extend_from_slice(&[1, term_shape(term)]);
            bytes.extend_from_slice(&dimension_bytes(*expected));
        }
        ConstraintExpression::Range {
            term,
            lower,
            upper,
            include_lower,
            include_upper,
        } => {
            bytes.extend_from_slice(&[
                2,
                term_shape(term),
                u8::from(*include_lower),
                u8::from(*include_upper),
            ]);
            bytes.extend_from_slice(&quantity_semantics(lower));
            bytes.extend_from_slice(&quantity_semantics(upper));
        }
    }
    bytes
}

fn quantity_semantics(quantity: &QuantityLiteral) -> Vec<u8> {
    let mut bytes = ratio_bytes(quantity.value);
    bytes.extend_from_slice(&unit_semantics(&quantity.source_unit));
    bytes
}

fn unit_semantics(unit: &UnitRef) -> Vec<u8> {
    let mut bytes = dimension_bytes(unit.dimension).to_vec();
    bytes.extend_from_slice(&ratio_bytes(unit.scale_to_base));
    bytes.extend_from_slice(&ratio_bytes(unit.offset_to_base));
    bytes
}

fn ratio_bytes(ratio: ExactRatio) -> Vec<u8> {
    let mut bytes = ratio.numerator().to_be_bytes().to_vec();
    bytes.extend_from_slice(&ratio.denominator().to_be_bytes());
    bytes
}

fn dimension_bytes(dimension: DimensionVector) -> [u8; 7] {
    dimension
        .exponents()
        .map(|exponent| (i16::from(exponent) + 128) as u8)
}

fn term_shape(term: &TermRef) -> u8 {
    match term {
        TermRef::Concept(_) => 0,
        TermRef::Variable {
            type_constraint: None,
            ..
        } => 1,
        TermRef::Variable {
            type_constraint: Some(_),
            ..
        } => 2,
        TermRef::Literal(LiteralValue::Boolean(_)) => 3,
        TermRef::Literal(LiteralValue::Text(_)) => 4,
        TermRef::Literal(LiteralValue::Quantity(_)) => 5,
        TermRef::Literal(LiteralValue::Bytes(_)) => 6,
        TermRef::Statement(_) => 7,
        TermRef::KnowledgeObject(_) => 8,
        TermRef::Receptor {
            expected_type: None,
            ..
        } => 9,
        TermRef::Receptor {
            expected_type: Some(_),
            ..
        } => 10,
    }
}

fn comparison_code(operator: ComparisonOperator) -> u8 {
    operator as u8
}

fn modality_code(modality: Modality) -> u8 {
    modality as u8
}

fn digest(label: &[u8], bytes: &[u8]) -> [u8; 32] {
    let mut hasher = blake3::Hasher::new();
    hasher.update(b"onebrain:vnext:structural-signature:1\0");
    hasher.update(label);
    hasher.update(&[0]);
    hasher.update(bytes);
    *hasher.finalize().as_bytes()
}

fn source_root(sources: &BTreeSet<(u8, [u8; 32])>) -> [u8; 32] {
    let mut hasher = blake3::Hasher::new();
    hasher.update(b"onebrain:vnext:structural-signature-sources:1\0");
    for (kind, cid) in sources {
        hasher.update(&[*kind]);
        hasher.update(cid);
    }
    *hasher.finalize().as_bytes()
}

fn hash_signature(hasher: &mut blake3::Hasher, signature: &StructuralSignature) {
    hasher.update(&[signature.kind as u8]);
    hasher.update(&[match signature.sensitivity {
        VocabularySensitivity::VocabularyIndependent => 0,
        VocabularySensitivity::ExactCcid => 1,
    }]);
    hasher.update(&signature.digest);
    hasher.update(&region_bytes(signature.region));
}

fn region_bytes(region: StructuralRegion) -> Vec<u8> {
    match region {
        StructuralRegion::ReceptorRole => vec![0],
        StructuralRegion::ReceptorExpectedType(index) => indexed_region(1, &[index]),
        StructuralRegion::ReceptorConstraint(index) => indexed_region(2, &[index]),
        StructuralRegion::OfferedRole(index) => indexed_region(3, &[index]),
        StructuralRegion::AcceptedInput(index) => indexed_region(4, &[index]),
        StructuralRegion::FrameSet { bucket } => vec![5, bucket as u8],
        StructuralRegion::Statement { bucket, statement } => {
            let mut bytes = vec![6, bucket as u8];
            bytes.extend_from_slice(&statement.to_be_bytes());
            bytes
        }
        StructuralRegion::Constraint {
            bucket,
            statement,
            constraint,
        } => {
            let mut bytes = vec![7, bucket as u8];
            bytes.extend_from_slice(&statement.to_be_bytes());
            bytes.extend_from_slice(&constraint.to_be_bytes());
            bytes
        }
        StructuralRegion::Quantity {
            bucket,
            statement,
            ordinal,
        } => {
            let mut bytes = vec![8, bucket as u8];
            bytes.extend_from_slice(&statement.to_be_bytes());
            bytes.extend_from_slice(&ordinal.to_be_bytes());
            bytes
        }
    }
}

fn indexed_region(code: u8, indexes: &[u32]) -> Vec<u8> {
    let mut bytes = vec![code];
    for index in indexes {
        bytes.extend_from_slice(&index.to_be_bytes());
    }
    bytes
}

fn index_u32(index: usize) -> Result<u32, StructuralSignatureError> {
    u32::try_from(index).map_err(|_| StructuralSignatureError::Limit)
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum StructuralSignatureError {
    Semantic(SemanticError),
    Limit,
}

impl From<SemanticError> for StructuralSignatureError {
    fn from(error: SemanticError) -> Self {
        Self::Semantic(error)
    }
}

#[cfg(test)]
mod tests {
    use ku_core::foundation::{
        AcceptedInput, AffordanceOrigin, ConceptCcid, LiteralValue, ObjectReference,
        QuantityLiteral, ReceptorAcceptanceProfile, ReceptorCardinality, ReceptorOrigin,
        StatementId, StatementLocator, StatementQualifiers, UnknownConstraintPolicy,
    };

    use super::*;

    fn concept(byte: u8) -> ConceptCcid {
        ConceptCcid::from_bytes([byte; 16])
    }

    fn reference(byte: u8) -> ObjectReference {
        ObjectReference::new(0, [byte; 32])
    }

    fn empty() -> SemanticFrameSet {
        SemanticFrameSet {
            statements: Vec::new(),
        }
    }

    fn statement(vocabulary: u8, reversed: bool) -> StatementFrame {
        let quantity = TermRef::Literal(LiteralValue::Quantity(QuantityLiteral {
            value: ExactRatio::integer(2),
            source_unit: UnitRef::coherent(
                concept(vocabulary.wrapping_add(3)),
                DimensionVector::LENGTH,
            ),
        }));
        let mut arguments = vec![
            TermRef::Concept(concept(vocabulary.wrapping_add(2))),
            quantity,
        ];
        if reversed {
            arguments.reverse();
        }
        StatementFrame {
            statement_id: StatementId(10),
            operator_or_predicate: concept(vocabulary.wrapping_add(1)),
            arguments,
            constraints: vec![TypedConstraint {
                expression: ConstraintExpression::Dimension {
                    term: TermRef::Variable {
                        id: ku_core::foundation::VariableId(9),
                        type_constraint: Some(concept(vocabulary.wrapping_add(4))),
                    },
                    expected: DimensionVector::LENGTH,
                },
                required: true,
            }],
            qualifiers: StatementQualifiers::default(),
        }
    }

    fn affordance(vocabulary: u8, reversed: bool) -> KnowledgeAffordance {
        let empty = empty();
        KnowledgeAffordance {
            sources: vec![reference(1)],
            offered_roles: vec![concept(vocabulary)],
            accepted_inputs: vec![AcceptedInput {
                receptor_definition: reference(2),
                role: concept(vocabulary.wrapping_add(5)),
                required: true,
            }],
            semantics: AffordanceSemantics {
                preconditions: empty.clone(),
                outputs: SemanticFrameSet {
                    statements: vec![statement(vocabulary, reversed)],
                },
                effects: empty.clone(),
                properties: empty.clone(),
                invariants: empty.clone(),
                operating_conditions: empty.clone(),
                limits: empty,
            },
            abstraction_patterns: Vec::new(),
            origin: AffordanceOrigin::Explicit {
                claims: vec![StatementLocator {
                    object: reference(1),
                    statement_index: 0,
                }],
            },
        }
    }

    fn receptor() -> ReceptorDefinition {
        ReceptorDefinition {
            role: concept(30),
            expected_types: vec![concept(31)],
            hard_constraints: vec![TypedConstraint {
                expression: ConstraintExpression::Compare {
                    left: TermRef::Concept(concept(32)),
                    operator: ComparisonOperator::Equal,
                    right: TermRef::Concept(concept(33)),
                },
                required: true,
            }],
            cardinality: ReceptorCardinality::new(1, Some(1)).unwrap(),
            origin: ReceptorOrigin::Declared {
                source: StatementLocator {
                    object: reference(34),
                    statement_index: 0,
                },
            },
            acceptance: ReceptorAcceptanceProfile {
                policy: reference(35),
                required_evidence_kinds: Vec::new(),
                unknown_constraint_policy: UnknownConstraintPolicy::KeepUnresolved,
            },
        }
    }

    fn independent(
        signatures: &[StructuralSignature],
    ) -> BTreeSet<(StructuralSignatureKind, [u8; 32])> {
        signatures
            .iter()
            .filter(|signature| {
                signature.sensitivity == VocabularySensitivity::VocabularyIndependent
            })
            .map(|signature| (signature.kind, signature.digest))
            .collect()
    }

    #[test]
    fn rebuild_is_deterministic_across_source_order_and_restart() {
        let receptor = receptor();
        let affordance = affordance(40, false);
        let a = StructuralSignatureSource::Receptor {
            cid: ObjectCid::from_bytes([1; 32]),
            definition: &receptor,
        };
        let b = StructuralSignatureSource::Affordance {
            cid: ObjectCid::from_bytes([2; 32]),
            affordance: &affordance,
        };
        let forward = StructuralSignatureIndex::rebuild(&[a, b]).unwrap();
        let reverse = StructuralSignatureIndex::rebuild(&[b, a]).unwrap();
        assert_eq!(forward.snapshot(), reverse.snapshot());
        assert!(forward.snapshot().signature_count > 0);
    }

    #[test]
    fn vocabulary_rename_preserves_relational_fbs_operator_graph_and_unit_structure() {
        let first = affordance(50, false);
        let renamed = affordance(90, false);
        let first_index =
            StructuralSignatureIndex::rebuild(&[StructuralSignatureSource::Affordance {
                cid: ObjectCid::from_bytes([3; 32]),
                affordance: &first,
            }])
            .unwrap();
        let renamed_index =
            StructuralSignatureIndex::rebuild(&[StructuralSignatureSource::Affordance {
                cid: ObjectCid::from_bytes([4; 32]),
                affordance: &renamed,
            }])
            .unwrap();
        let first_signatures = first_index.signatures_for(ObjectCid::from_bytes([3; 32]));
        let renamed_signatures = renamed_index.signatures_for(ObjectCid::from_bytes([4; 32]));
        assert_eq!(
            independent(&first_signatures),
            independent(&renamed_signatures)
        );
        let first_exact = first_signatures
            .iter()
            .filter(|signature| signature.sensitivity == VocabularySensitivity::ExactCcid)
            .map(|signature| signature.digest)
            .collect::<BTreeSet<_>>();
        let renamed_exact = renamed_signatures
            .iter()
            .filter(|signature| signature.sensitivity == VocabularySensitivity::ExactCcid)
            .map(|signature| signature.digest)
            .collect::<BTreeSet<_>>();
        assert_ne!(first_exact, renamed_exact);
    }

    #[test]
    fn argument_direction_changes_graph_shingle_but_not_dimension() {
        let normal = affordance(60, false);
        let reversed = affordance(60, true);
        let left = StructuralSignatureIndex::rebuild(&[StructuralSignatureSource::Affordance {
            cid: ObjectCid::from_bytes([5; 32]),
            affordance: &normal,
        }])
        .unwrap();
        let right = StructuralSignatureIndex::rebuild(&[StructuralSignatureSource::Affordance {
            cid: ObjectCid::from_bytes([6; 32]),
            affordance: &reversed,
        }])
        .unwrap();
        let select = |index: &StructuralSignatureIndex, cid, kind| {
            index
                .signatures_for(ObjectCid::from_bytes([cid; 32]))
                .into_iter()
                .filter(|signature| signature.kind == kind)
                .map(|signature| signature.digest)
                .collect::<BTreeSet<_>>()
        };
        assert_ne!(
            select(&left, 5, StructuralSignatureKind::GraphShingle),
            select(&right, 6, StructuralSignatureKind::GraphShingle)
        );
        assert_eq!(
            select(&left, 5, StructuralSignatureKind::Dimension),
            select(&right, 6, StructuralSignatureKind::Dimension)
        );
    }

    #[test]
    fn signature_postings_are_candidate_hints_not_action_authority() {
        let affordance = affordance(70, false);
        let cid = ObjectCid::from_bytes([7; 32]);
        let index = StructuralSignatureIndex::rebuild(&[StructuralSignatureSource::Affordance {
            cid,
            affordance: &affordance,
        }])
        .unwrap();
        let signature = index
            .signatures_for(cid)
            .into_iter()
            .find(|signature| signature.kind == StructuralSignatureKind::OperatorAst)
            .unwrap();
        assert_eq!(
            index.objects_for(signature.kind, signature.digest),
            vec![cid]
        );
        assert!(!signature.is_action_authority());
    }

    #[test]
    fn clearing_and_rebuilding_only_changes_derived_state() {
        let affordance = affordance(80, false);
        let source = StructuralSignatureSource::Affordance {
            cid: ObjectCid::from_bytes([8; 32]),
            affordance: &affordance,
        };
        let mut index = StructuralSignatureIndex::rebuild(&[source]).unwrap();
        let expected = index.snapshot();
        let source_payload = affordance.canonical_payload().unwrap();
        index.clear_derived();
        assert_eq!(index.snapshot().source_count, 0);
        index = StructuralSignatureIndex::rebuild(&[source]).unwrap();
        assert_eq!(index.snapshot(), expected);
        assert_eq!(affordance.canonical_payload().unwrap(), source_payload);
    }
}

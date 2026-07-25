//! Network-safe semantic primitives for KU vNext.
//!
//! This is an object payload IR, not a second Core DNA wire format. Core DNA
//! may continue using local compressed `ConceptId`; this module can only express
//! concepts as full CCIDs.

use std::cmp::Ordering;
use std::collections::{BTreeMap, BTreeSet};
use std::fmt;

use serde::de::{Error as DeError, SeqAccess, Visitor};
use serde::{Deserialize, Deserializer, Serialize, Serializer};

use super::canonical::{
    encode_canonical, CanonicalError, CanonicalValue, NormalizedText, ResourceProfile,
};
use super::object::{
    DisclosureClass, KnowledgeObjectEnvelope, ObjectError, ObjectKind, ObjectReference,
    SchemaVersion,
};

pub const SEMANTIC_KERNEL_OBJECT_KIND: ObjectKind = ObjectKind(2);
pub const SEMANTIC_PROFILE_MAJOR: u64 = 1;
pub const SEMANTIC_PROFILE_MINOR: u64 = 0;
pub const MAX_STATEMENTS: usize = 4_096;
pub const MAX_ARGUMENTS_PER_STATEMENT: usize = 1_024;
pub const MAX_CONSTRAINTS_PER_STATEMENT: usize = 1_024;

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct ConceptCcid([u8; 16]);

impl ConceptCcid {
    pub const fn from_bytes(bytes: [u8; 16]) -> Self {
        Self(bytes)
    }

    pub const fn as_bytes(&self) -> &[u8; 16] {
        &self.0
    }

    pub fn to_hex(self) -> String {
        self.0.iter().map(|byte| format!("{byte:02x}")).collect()
    }
}

impl fmt::Display for ConceptCcid {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.to_hex())
    }
}

impl Serialize for ConceptCcid {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        serializer.serialize_bytes(&self.0)
    }
}

impl<'de> Deserialize<'de> for ConceptCcid {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        struct ConceptCcidVisitor;

        impl<'de> Visitor<'de> for ConceptCcidVisitor {
            type Value = ConceptCcid;

            fn expecting(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
                formatter.write_str("exactly 16 CCID bytes")
            }

            fn visit_bytes<E>(self, value: &[u8]) -> Result<Self::Value, E>
            where
                E: DeError,
            {
                let bytes: [u8; 16] = value
                    .try_into()
                    .map_err(|_| E::invalid_length(value.len(), &self))?;
                Ok(ConceptCcid::from_bytes(bytes))
            }

            fn visit_byte_buf<E>(self, value: Vec<u8>) -> Result<Self::Value, E>
            where
                E: DeError,
            {
                self.visit_bytes(&value)
            }

            fn visit_seq<A>(self, mut sequence: A) -> Result<Self::Value, A::Error>
            where
                A: SeqAccess<'de>,
            {
                let mut bytes = [0u8; 16];
                for (index, byte) in bytes.iter_mut().enumerate() {
                    *byte = sequence
                        .next_element()?
                        .ok_or_else(|| A::Error::invalid_length(index, &self))?;
                }
                if sequence.next_element::<u8>()?.is_some() {
                    return Err(A::Error::invalid_length(17, &self));
                }
                Ok(ConceptCcid::from_bytes(bytes))
            }
        }

        deserializer.deserialize_bytes(ConceptCcidVisitor)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct VariableId(pub u32);

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct StatementId(pub u32);

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct ReceptorSlotId(pub u32);

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ExactRatio {
    numerator: i64,
    denominator: u64,
}

impl ExactRatio {
    pub fn new(numerator: i64, denominator: u64) -> Result<Self, SemanticError> {
        if denominator == 0 {
            return Err(SemanticError::ZeroDenominator);
        }
        if numerator == 0 {
            return Ok(Self {
                numerator: 0,
                denominator: 1,
            });
        }
        let divisor = gcd(numerator.unsigned_abs(), denominator);
        let reduced_numerator = i128::from(numerator) / i128::from(divisor);
        Ok(Self {
            numerator: i64::try_from(reduced_numerator).map_err(|_| SemanticError::Overflow)?,
            denominator: denominator / divisor,
        })
    }

    pub const fn integer(value: i64) -> Self {
        Self {
            numerator: value,
            denominator: 1,
        }
    }

    pub const fn numerator(self) -> i64 {
        self.numerator
    }

    pub const fn denominator(self) -> u64 {
        self.denominator
    }

    pub fn checked_mul(self, other: Self) -> Result<Self, SemanticError> {
        let left_cancel = gcd(self.numerator.unsigned_abs(), other.denominator);
        let right_cancel = gcd(other.numerator.unsigned_abs(), self.denominator);
        let left_num = i64::try_from(i128::from(self.numerator) / i128::from(left_cancel))
            .map_err(|_| SemanticError::Overflow)?;
        let right_num = i64::try_from(i128::from(other.numerator) / i128::from(right_cancel))
            .map_err(|_| SemanticError::Overflow)?;
        let left_den = self.denominator / right_cancel;
        let right_den = other.denominator / left_cancel;
        Self::new(
            left_num
                .checked_mul(right_num)
                .ok_or(SemanticError::Overflow)?,
            left_den
                .checked_mul(right_den)
                .ok_or(SemanticError::Overflow)?,
        )
    }

    pub fn checked_add(self, other: Self) -> Result<Self, SemanticError> {
        let common = gcd(self.denominator, other.denominator);
        let left_scale = other.denominator / common;
        let right_scale = self.denominator / common;
        let numerator = i128::from(self.numerator) * i128::from(left_scale)
            + i128::from(other.numerator) * i128::from(right_scale);
        let numerator = i64::try_from(numerator).map_err(|_| SemanticError::Overflow)?;
        let denominator = self
            .denominator
            .checked_mul(left_scale)
            .ok_or(SemanticError::Overflow)?;
        Self::new(numerator, denominator)
    }

    pub fn checked_div(self, other: Self) -> Result<Self, SemanticError> {
        if other.numerator == 0 {
            return Err(SemanticError::ZeroDenominator);
        }
        let numerator_cancel = gcd(
            self.numerator.unsigned_abs(),
            other.numerator.unsigned_abs(),
        );
        let denominator_cancel = gcd(other.denominator, self.denominator);
        let left_numerator = i128::from(self.numerator) / i128::from(numerator_cancel);
        let right_denominator = i128::from(other.denominator / denominator_cancel);
        let sign = if other.numerator < 0 { -1i128 } else { 1i128 };
        let numerator = left_numerator
            .checked_mul(right_denominator)
            .and_then(|value| value.checked_mul(sign))
            .ok_or(SemanticError::Overflow)?;
        let left_denominator = self.denominator / denominator_cancel;
        let right_numerator = other.numerator.unsigned_abs() / numerator_cancel;
        let denominator = left_denominator
            .checked_mul(right_numerator)
            .ok_or(SemanticError::Overflow)?;
        Self::new(
            i64::try_from(numerator).map_err(|_| SemanticError::Overflow)?,
            denominator,
        )
    }

    pub fn checked_cmp(self, other: Self) -> Result<Ordering, SemanticError> {
        let common = gcd(self.denominator, other.denominator);
        let left_scale = other.denominator / common;
        let right_scale = self.denominator / common;
        let left = i128::from(self.numerator) * i128::from(left_scale);
        let right = i128::from(other.numerator) * i128::from(right_scale);
        Ok(left.cmp(&right))
    }

    pub(crate) fn to_value(self) -> CanonicalValue {
        CanonicalValue::Map(vec![
            (
                0,
                CanonicalValue::Bytes(self.numerator.to_be_bytes().to_vec()),
            ),
            (
                1,
                CanonicalValue::Bytes(self.denominator.to_be_bytes().to_vec()),
            ),
        ])
    }
}

fn gcd(mut left: u64, mut right: u64) -> u64 {
    while right != 0 {
        let remainder = left % right;
        left = right;
        right = remainder;
    }
    left
}

/// SI base order: length, mass, time, electric current, temperature,
/// amount of substance, luminous intensity.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct DimensionVector([i8; 7]);

impl DimensionVector {
    pub const DIMENSIONLESS: Self = Self([0; 7]);
    pub const LENGTH: Self = Self([1, 0, 0, 0, 0, 0, 0]);
    pub const MASS: Self = Self([0, 1, 0, 0, 0, 0, 0]);
    pub const TIME: Self = Self([0, 0, 1, 0, 0, 0, 0]);
    pub const TEMPERATURE: Self = Self([0, 0, 0, 0, 1, 0, 0]);

    pub const fn new(exponents: [i8; 7]) -> Self {
        Self(exponents)
    }

    pub const fn exponents(self) -> [i8; 7] {
        self.0
    }

    pub fn checked_mul(self, other: Self) -> Result<Self, SemanticError> {
        let mut output = [0i8; 7];
        for (index, value) in output.iter_mut().enumerate() {
            *value = self.0[index]
                .checked_add(other.0[index])
                .ok_or(SemanticError::DimensionOverflow)?;
        }
        Ok(Self(output))
    }

    pub fn checked_div(self, other: Self) -> Result<Self, SemanticError> {
        let mut output = [0i8; 7];
        for (index, value) in output.iter_mut().enumerate() {
            *value = self.0[index]
                .checked_sub(other.0[index])
                .ok_or(SemanticError::DimensionOverflow)?;
        }
        Ok(Self(output))
    }

    pub(crate) fn to_value(self) -> CanonicalValue {
        CanonicalValue::Array(
            self.0
                .iter()
                .map(|exponent| CanonicalValue::Unsigned((*exponent as i16 + 128) as u64))
                .collect(),
        )
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct UnitRef {
    pub unit: ConceptCcid,
    pub dimension: DimensionVector,
    /// `base = source * scale_to_base + offset_to_base`.
    pub scale_to_base: ExactRatio,
    pub offset_to_base: ExactRatio,
}

impl UnitRef {
    pub fn coherent(unit: ConceptCcid, dimension: DimensionVector) -> Self {
        Self {
            unit,
            dimension,
            scale_to_base: ExactRatio::integer(1),
            offset_to_base: ExactRatio::integer(0),
        }
    }

    fn to_value(&self) -> CanonicalValue {
        CanonicalValue::Map(vec![
            (0, CanonicalValue::Bytes(self.unit.0.to_vec())),
            (1, self.dimension.to_value()),
            (2, self.scale_to_base.to_value()),
            (3, self.offset_to_base.to_value()),
        ])
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct QuantityLiteral {
    pub value: ExactRatio,
    pub source_unit: UnitRef,
}

impl QuantityLiteral {
    pub fn to_base_value(&self) -> Result<ExactRatio, SemanticError> {
        self.value
            .checked_mul(self.source_unit.scale_to_base)?
            .checked_add(self.source_unit.offset_to_base)
    }

    pub fn compare(&self, other: &Self) -> Result<Ordering, SemanticError> {
        if self.source_unit.dimension != other.source_unit.dimension {
            return Err(SemanticError::DimensionMismatch);
        }
        self.to_base_value()?.checked_cmp(other.to_base_value()?)
    }

    fn to_value(&self) -> CanonicalValue {
        CanonicalValue::Map(vec![
            (0, self.value.to_value()),
            (1, self.source_unit.to_value()),
        ])
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum LiteralValue {
    Boolean(bool),
    Text(NormalizedText),
    Quantity(QuantityLiteral),
    Bytes(Vec<u8>),
}

impl LiteralValue {
    fn to_value(&self) -> CanonicalValue {
        match self {
            Self::Boolean(value) => CanonicalValue::Map(vec![
                (0, CanonicalValue::Unsigned(0)),
                (1, CanonicalValue::Bool(*value)),
            ]),
            Self::Text(value) => CanonicalValue::Map(vec![
                (0, CanonicalValue::Unsigned(1)),
                (1, CanonicalValue::Text(value.as_str().to_string())),
            ]),
            Self::Quantity(value) => CanonicalValue::Map(vec![
                (0, CanonicalValue::Unsigned(2)),
                (1, value.to_value()),
            ]),
            Self::Bytes(value) => CanonicalValue::Map(vec![
                (0, CanonicalValue::Unsigned(3)),
                (1, CanonicalValue::Bytes(value.clone())),
            ]),
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum TermRef {
    Concept(ConceptCcid),
    Variable {
        id: VariableId,
        type_constraint: Option<ConceptCcid>,
    },
    Literal(LiteralValue),
    Statement(StatementId),
    KnowledgeObject(ObjectReference),
    Receptor {
        slot: ReceptorSlotId,
        expected_type: Option<ConceptCcid>,
    },
}

impl TermRef {
    pub(crate) fn to_value(&self) -> CanonicalValue {
        match self {
            Self::Concept(concept) => CanonicalValue::Map(vec![
                (0, CanonicalValue::Unsigned(0)),
                (1, CanonicalValue::Bytes(concept.0.to_vec())),
            ]),
            Self::Variable {
                id,
                type_constraint,
            } => {
                let mut value = vec![
                    (0, CanonicalValue::Unsigned(1)),
                    (1, CanonicalValue::Unsigned(id.0 as u64)),
                ];
                if let Some(concept) = type_constraint {
                    value.push((2, CanonicalValue::Bytes(concept.0.to_vec())));
                }
                CanonicalValue::Map(value)
            }
            Self::Literal(literal) => CanonicalValue::Map(vec![
                (0, CanonicalValue::Unsigned(2)),
                (1, literal.to_value()),
            ]),
            Self::Statement(statement) => CanonicalValue::Map(vec![
                (0, CanonicalValue::Unsigned(3)),
                (1, CanonicalValue::Unsigned(statement.0 as u64)),
            ]),
            Self::KnowledgeObject(reference) => CanonicalValue::Map(vec![
                (0, CanonicalValue::Unsigned(4)),
                (1, reference.to_value()),
            ]),
            Self::Receptor {
                slot,
                expected_type,
            } => {
                let mut value = vec![
                    (0, CanonicalValue::Unsigned(5)),
                    (1, CanonicalValue::Unsigned(slot.0 as u64)),
                ];
                if let Some(concept) = expected_type {
                    value.push((2, CanonicalValue::Bytes(concept.0.to_vec())));
                }
                CanonicalValue::Map(value)
            }
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(u64)]
pub enum ComparisonOperator {
    Equal = 0,
    NotEqual = 1,
    LessThan = 2,
    LessThanOrEqual = 3,
    GreaterThan = 4,
    GreaterThanOrEqual = 5,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ConstraintExpression {
    Compare {
        left: TermRef,
        operator: ComparisonOperator,
        right: TermRef,
    },
    Dimension {
        term: TermRef,
        expected: DimensionVector,
    },
    Range {
        term: TermRef,
        lower: QuantityLiteral,
        upper: QuantityLiteral,
        include_lower: bool,
        include_upper: bool,
    },
}

impl ConstraintExpression {
    fn to_value(&self) -> CanonicalValue {
        match self {
            Self::Compare {
                left,
                operator,
                right,
            } => CanonicalValue::Map(vec![
                (0, CanonicalValue::Unsigned(0)),
                (1, left.to_value()),
                (2, CanonicalValue::Unsigned(*operator as u64)),
                (3, right.to_value()),
            ]),
            Self::Dimension { term, expected } => CanonicalValue::Map(vec![
                (0, CanonicalValue::Unsigned(1)),
                (1, term.to_value()),
                (2, expected.to_value()),
            ]),
            Self::Range {
                term,
                lower,
                upper,
                include_lower,
                include_upper,
            } => CanonicalValue::Map(vec![
                (0, CanonicalValue::Unsigned(2)),
                (1, term.to_value()),
                (2, lower.to_value()),
                (3, upper.to_value()),
                (4, CanonicalValue::Bool(*include_lower)),
                (5, CanonicalValue::Bool(*include_upper)),
            ]),
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ConstraintEvaluation {
    Satisfied,
    Violated,
    Unknown,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TypedConstraint {
    pub expression: ConstraintExpression,
    pub required: bool,
}

impl TypedConstraint {
    pub(crate) fn to_value(&self) -> CanonicalValue {
        CanonicalValue::Map(vec![
            (0, self.expression.to_value()),
            (1, CanonicalValue::Bool(self.required)),
        ])
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(u64)]
pub enum Modality {
    Asserted = 0,
    Observed = 1,
    Reported = 2,
    Possible = 3,
    Necessary = 4,
    Desired = 5,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SourceSpan {
    pub source: ObjectReference,
    pub start: u64,
    pub end: u64,
}

impl SourceSpan {
    fn to_value(&self) -> CanonicalValue {
        CanonicalValue::Map(vec![
            (0, self.source.to_value()),
            (1, CanonicalValue::Unsigned(self.start)),
            (2, CanonicalValue::Unsigned(self.end)),
        ])
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct StatementQualifiers {
    pub negated: bool,
    pub modality: Modality,
    pub condition: Option<StatementId>,
    pub time: Option<TermRef>,
    pub location: Option<TermRef>,
    pub perspective: Option<TermRef>,
    pub tolerance: Option<QuantityLiteral>,
    pub source_spans: Vec<SourceSpan>,
}

impl Default for StatementQualifiers {
    fn default() -> Self {
        Self {
            negated: false,
            modality: Modality::Asserted,
            condition: None,
            time: None,
            location: None,
            perspective: None,
            tolerance: None,
            source_spans: Vec::new(),
        }
    }
}

impl StatementQualifiers {
    fn to_value(&self) -> CanonicalValue {
        let mut fields = vec![
            (0, CanonicalValue::Bool(self.negated)),
            (1, CanonicalValue::Unsigned(self.modality as u64)),
        ];
        if let Some(condition) = self.condition {
            fields.push((2, CanonicalValue::Unsigned(condition.0 as u64)));
        }
        if let Some(time) = &self.time {
            fields.push((3, time.to_value()));
        }
        if let Some(location) = &self.location {
            fields.push((4, location.to_value()));
        }
        if let Some(perspective) = &self.perspective {
            fields.push((5, perspective.to_value()));
        }
        if let Some(tolerance) = &self.tolerance {
            fields.push((6, tolerance.to_value()));
        }
        if !self.source_spans.is_empty() {
            fields.push((
                7,
                CanonicalValue::Array(self.source_spans.iter().map(SourceSpan::to_value).collect()),
            ));
        }
        CanonicalValue::Map(fields)
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct StatementFrame {
    pub statement_id: StatementId,
    pub operator_or_predicate: ConceptCcid,
    pub arguments: Vec<TermRef>,
    pub constraints: Vec<TypedConstraint>,
    pub qualifiers: StatementQualifiers,
}

impl StatementFrame {
    fn to_value(&self) -> CanonicalValue {
        CanonicalValue::Map(vec![
            (0, CanonicalValue::Unsigned(self.statement_id.0 as u64)),
            (
                1,
                CanonicalValue::Bytes(self.operator_or_predicate.0.to_vec()),
            ),
            (
                2,
                CanonicalValue::Array(self.arguments.iter().map(TermRef::to_value).collect()),
            ),
            (
                3,
                CanonicalValue::Array(
                    self.constraints
                        .iter()
                        .map(TypedConstraint::to_value)
                        .collect(),
                ),
            ),
            (4, self.qualifiers.to_value()),
        ])
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SemanticFrameSet {
    pub statements: Vec<StatementFrame>,
}

impl SemanticFrameSet {
    /// Decode the frozen semantic profile from an already canonical value.
    /// The final round-trip check rejects unknown fields, non-normalized IDs,
    /// unreduced ratios, and every other alternate representation.
    pub fn from_canonical_value(value: &CanonicalValue) -> Result<Self, SemanticError> {
        let map = semantic_map(value, "semantic")?;
        if semantic_unsigned(
            semantic_required(map, 0, "semantic.major")?,
            "semantic.major",
        )? != SEMANTIC_PROFILE_MAJOR
            || semantic_unsigned(
                semantic_required(map, 1, "semantic.minor")?,
                "semantic.minor",
            )? != SEMANTIC_PROFILE_MINOR
        {
            return Err(SemanticError::InvalidField("semantic.version"));
        }
        let statements = semantic_array(
            semantic_required(map, 2, "semantic.statements")?,
            "semantic.statements",
        )?
        .iter()
        .map(parse_statement_frame)
        .collect::<Result<Vec<_>, _>>()?;
        let decoded = Self { statements };
        decoded.validate_shape()?;
        if decoded.canonical_value()? != *value {
            return Err(SemanticError::NonCanonicalValue);
        }
        Ok(decoded)
    }

    pub fn alpha_normalized(&self) -> Result<Self, SemanticError> {
        self.validate_shape()?;
        let statement_map: BTreeMap<_, _> = self
            .statements
            .iter()
            .enumerate()
            .map(|(index, statement)| {
                Ok((
                    statement.statement_id,
                    StatementId(u32::try_from(index).map_err(|_| SemanticError::LimitStatements)?),
                ))
            })
            .collect::<Result<_, SemanticError>>()?;

        let mut variable_types = BTreeMap::<VariableId, Option<ConceptCcid>>::new();
        for statement in &self.statements {
            visit_statement_terms(statement, &mut |term| {
                if let TermRef::Variable {
                    id,
                    type_constraint,
                } = term
                {
                    match variable_types.get(id).copied().flatten() {
                        Some(existing)
                            if Some(existing) != *type_constraint && type_constraint.is_some() =>
                        {
                            return Err(SemanticError::VariableTypeConflict(*id));
                        }
                        _ => {
                            if type_constraint.is_some() {
                                variable_types.insert(*id, *type_constraint);
                            } else {
                                variable_types.entry(*id).or_insert(None);
                            }
                        }
                    }
                }
                Ok(())
            })?;
        }

        let mut variable_map = BTreeMap::new();
        let mut receptor_map = BTreeMap::new();
        let mut statements = Vec::with_capacity(self.statements.len());
        for statement in &self.statements {
            let mut normalized = statement.clone();
            normalized.statement_id = statement_map[&statement.statement_id];
            for argument in &mut normalized.arguments {
                normalize_term(
                    argument,
                    &statement_map,
                    &variable_types,
                    &mut variable_map,
                    &mut receptor_map,
                )?;
            }
            for constraint in &mut normalized.constraints {
                normalize_constraint(
                    &mut constraint.expression,
                    &statement_map,
                    &variable_types,
                    &mut variable_map,
                    &mut receptor_map,
                )?;
            }
            if let Some(condition) = normalized.qualifiers.condition.as_mut() {
                *condition = *statement_map
                    .get(condition)
                    .ok_or(SemanticError::UnknownStatementReference(*condition))?;
            }
            for term in [
                &mut normalized.qualifiers.time,
                &mut normalized.qualifiers.location,
                &mut normalized.qualifiers.perspective,
            ] {
                if let Some(term) = term {
                    normalize_term(
                        term,
                        &statement_map,
                        &variable_types,
                        &mut variable_map,
                        &mut receptor_map,
                    )?;
                }
            }
            statements.push(normalized);
        }
        Ok(Self { statements })
    }

    pub fn canonical_value(&self) -> Result<CanonicalValue, SemanticError> {
        let normalized = self.alpha_normalized()?;
        Ok(CanonicalValue::Map(vec![
            (0, CanonicalValue::Unsigned(SEMANTIC_PROFILE_MAJOR)),
            (1, CanonicalValue::Unsigned(SEMANTIC_PROFILE_MINOR)),
            (
                2,
                CanonicalValue::Array(
                    normalized
                        .statements
                        .iter()
                        .map(StatementFrame::to_value)
                        .collect(),
                ),
            ),
        ]))
    }

    pub fn canonical_bytes(&self) -> Result<Vec<u8>, SemanticError> {
        encode_canonical(&self.canonical_value()?, ResourceProfile::ObjectV1).map_err(Into::into)
    }

    pub fn to_knowledge_object(
        &self,
        disclosure: DisclosureClass,
    ) -> Result<KnowledgeObjectEnvelope, SemanticError> {
        Ok(KnowledgeObjectEnvelope::new(
            SEMANTIC_KERNEL_OBJECT_KIND,
            SchemaVersion::new(SEMANTIC_PROFILE_MAJOR, SEMANTIC_PROFILE_MINOR),
            disclosure,
            self.canonical_value()?,
        ))
    }

    fn validate_shape(&self) -> Result<(), SemanticError> {
        if self.statements.len() > MAX_STATEMENTS {
            return Err(SemanticError::LimitStatements);
        }
        let mut ids = BTreeSet::new();
        for statement in &self.statements {
            if !ids.insert(statement.statement_id) {
                return Err(SemanticError::DuplicateStatementId(statement.statement_id));
            }
            if statement.arguments.len() > MAX_ARGUMENTS_PER_STATEMENT {
                return Err(SemanticError::LimitArguments);
            }
            if statement.constraints.len() > MAX_CONSTRAINTS_PER_STATEMENT {
                return Err(SemanticError::LimitConstraints);
            }
            if statement
                .qualifiers
                .source_spans
                .iter()
                .any(|span| span.start > span.end)
            {
                return Err(SemanticError::InvalidSourceSpan);
            }
        }
        Ok(())
    }
}

fn parse_statement_frame(value: &CanonicalValue) -> Result<StatementFrame, SemanticError> {
    let map = semantic_map(value, "statement")?;
    let statement_id = semantic_u32(semantic_required(map, 0, "statement.id")?, "statement.id")?;
    let operator_or_predicate = semantic_ccid(
        semantic_required(map, 1, "statement.operator")?,
        "statement.operator",
    )?;
    let arguments = semantic_array(
        semantic_required(map, 2, "statement.arguments")?,
        "statement.arguments",
    )?
    .iter()
    .map(parse_term)
    .collect::<Result<Vec<_>, _>>()?;
    let constraints = semantic_array(
        semantic_required(map, 3, "statement.constraints")?,
        "statement.constraints",
    )?
    .iter()
    .map(parse_typed_constraint)
    .collect::<Result<Vec<_>, _>>()?;
    let qualifiers =
        parse_statement_qualifiers(semantic_required(map, 4, "statement.qualifiers")?)?;
    Ok(StatementFrame {
        statement_id: StatementId(statement_id),
        operator_or_predicate,
        arguments,
        constraints,
        qualifiers,
    })
}

fn parse_term(value: &CanonicalValue) -> Result<TermRef, SemanticError> {
    let map = semantic_map(value, "term")?;
    match semantic_unsigned(semantic_required(map, 0, "term.kind")?, "term.kind")? {
        0 => Ok(TermRef::Concept(semantic_ccid(
            semantic_required(map, 1, "term.concept")?,
            "term.concept",
        )?)),
        1 => Ok(TermRef::Variable {
            id: VariableId(semantic_u32(
                semantic_required(map, 1, "term.variable")?,
                "term.variable",
            )?),
            type_constraint: semantic_optional(map, 2)
                .map(|value| semantic_ccid(value, "term.type_constraint"))
                .transpose()?,
        }),
        2 => Ok(TermRef::Literal(parse_literal(semantic_required(
            map,
            1,
            "term.literal",
        )?)?)),
        3 => Ok(TermRef::Statement(StatementId(semantic_u32(
            semantic_required(map, 1, "term.statement")?,
            "term.statement",
        )?))),
        4 => Ok(TermRef::KnowledgeObject(ObjectReference::from_value(
            semantic_required(map, 1, "term.object")?,
        )?)),
        5 => Ok(TermRef::Receptor {
            slot: ReceptorSlotId(semantic_u32(
                semantic_required(map, 1, "term.receptor")?,
                "term.receptor",
            )?),
            expected_type: semantic_optional(map, 2)
                .map(|value| semantic_ccid(value, "term.expected_type"))
                .transpose()?,
        }),
        _ => Err(SemanticError::InvalidField("term.kind")),
    }
}

fn parse_literal(value: &CanonicalValue) -> Result<LiteralValue, SemanticError> {
    let map = semantic_map(value, "literal")?;
    let body = semantic_required(map, 1, "literal.value")?;
    match semantic_unsigned(semantic_required(map, 0, "literal.kind")?, "literal.kind")? {
        0 => Ok(LiteralValue::Boolean(semantic_bool(
            body,
            "literal.boolean",
        )?)),
        1 => match body {
            CanonicalValue::Text(text) => {
                Ok(LiteralValue::Text(NormalizedText::new(text.clone())?))
            }
            _ => Err(SemanticError::InvalidField("literal.text")),
        },
        2 => Ok(LiteralValue::Quantity(parse_quantity(body)?)),
        3 => match body {
            CanonicalValue::Bytes(bytes) => Ok(LiteralValue::Bytes(bytes.clone())),
            _ => Err(SemanticError::InvalidField("literal.bytes")),
        },
        _ => Err(SemanticError::InvalidField("literal.kind")),
    }
}

fn parse_quantity(value: &CanonicalValue) -> Result<QuantityLiteral, SemanticError> {
    let map = semantic_map(value, "quantity")?;
    Ok(QuantityLiteral {
        value: parse_ratio(semantic_required(map, 0, "quantity.value")?)?,
        source_unit: parse_unit(semantic_required(map, 1, "quantity.unit")?)?,
    })
}

fn parse_unit(value: &CanonicalValue) -> Result<UnitRef, SemanticError> {
    let map = semantic_map(value, "unit")?;
    Ok(UnitRef {
        unit: semantic_ccid(semantic_required(map, 0, "unit.ccid")?, "unit.ccid")?,
        dimension: parse_dimension(semantic_required(map, 1, "unit.dimension")?)?,
        scale_to_base: parse_ratio(semantic_required(map, 2, "unit.scale")?)?,
        offset_to_base: parse_ratio(semantic_required(map, 3, "unit.offset")?)?,
    })
}

fn parse_ratio(value: &CanonicalValue) -> Result<ExactRatio, SemanticError> {
    let map = semantic_map(value, "ratio")?;
    let numerator = semantic_fixed_bytes::<8>(
        semantic_required(map, 0, "ratio.numerator")?,
        "ratio.numerator",
    )?;
    let denominator = semantic_fixed_bytes::<8>(
        semantic_required(map, 1, "ratio.denominator")?,
        "ratio.denominator",
    )?;
    ExactRatio::new(
        i64::from_be_bytes(numerator),
        u64::from_be_bytes(denominator),
    )
}

fn parse_dimension(value: &CanonicalValue) -> Result<DimensionVector, SemanticError> {
    let values = semantic_array(value, "dimension")?;
    if values.len() != 7 {
        return Err(SemanticError::InvalidField("dimension"));
    }
    let mut exponents = [0i8; 7];
    for (index, value) in values.iter().enumerate() {
        let encoded = semantic_unsigned(value, "dimension.exponent")?;
        if encoded > 255 {
            return Err(SemanticError::InvalidField("dimension.exponent"));
        }
        exponents[index] = (encoded as i16 - 128) as i8;
    }
    Ok(DimensionVector::new(exponents))
}

fn parse_typed_constraint(value: &CanonicalValue) -> Result<TypedConstraint, SemanticError> {
    let map = semantic_map(value, "constraint")?;
    Ok(TypedConstraint {
        expression: parse_constraint_expression(semantic_required(
            map,
            0,
            "constraint.expression",
        )?)?,
        required: semantic_bool(
            semantic_required(map, 1, "constraint.required")?,
            "constraint.required",
        )?,
    })
}

fn parse_constraint_expression(
    value: &CanonicalValue,
) -> Result<ConstraintExpression, SemanticError> {
    let map = semantic_map(value, "constraint.expression")?;
    match semantic_unsigned(
        semantic_required(map, 0, "constraint.expression.kind")?,
        "constraint.expression.kind",
    )? {
        0 => {
            let operator = match semantic_unsigned(
                semantic_required(map, 2, "constraint.operator")?,
                "constraint.operator",
            )? {
                0 => ComparisonOperator::Equal,
                1 => ComparisonOperator::NotEqual,
                2 => ComparisonOperator::LessThan,
                3 => ComparisonOperator::LessThanOrEqual,
                4 => ComparisonOperator::GreaterThan,
                5 => ComparisonOperator::GreaterThanOrEqual,
                _ => return Err(SemanticError::InvalidField("constraint.operator")),
            };
            Ok(ConstraintExpression::Compare {
                left: parse_term(semantic_required(map, 1, "constraint.left")?)?,
                operator,
                right: parse_term(semantic_required(map, 3, "constraint.right")?)?,
            })
        }
        1 => Ok(ConstraintExpression::Dimension {
            term: parse_term(semantic_required(map, 1, "constraint.term")?)?,
            expected: parse_dimension(semantic_required(map, 2, "constraint.dimension")?)?,
        }),
        2 => Ok(ConstraintExpression::Range {
            term: parse_term(semantic_required(map, 1, "constraint.term")?)?,
            lower: parse_quantity(semantic_required(map, 2, "constraint.lower")?)?,
            upper: parse_quantity(semantic_required(map, 3, "constraint.upper")?)?,
            include_lower: semantic_bool(
                semantic_required(map, 4, "constraint.include_lower")?,
                "constraint.include_lower",
            )?,
            include_upper: semantic_bool(
                semantic_required(map, 5, "constraint.include_upper")?,
                "constraint.include_upper",
            )?,
        }),
        _ => Err(SemanticError::InvalidField("constraint.expression.kind")),
    }
}

fn parse_statement_qualifiers(
    value: &CanonicalValue,
) -> Result<StatementQualifiers, SemanticError> {
    let map = semantic_map(value, "qualifiers")?;
    let modality = match semantic_unsigned(
        semantic_required(map, 1, "qualifiers.modality")?,
        "qualifiers.modality",
    )? {
        0 => Modality::Asserted,
        1 => Modality::Observed,
        2 => Modality::Reported,
        3 => Modality::Possible,
        4 => Modality::Necessary,
        5 => Modality::Desired,
        _ => return Err(SemanticError::InvalidField("qualifiers.modality")),
    };
    let source_spans = semantic_optional(map, 7)
        .map(|value| {
            semantic_array(value, "qualifiers.source_spans")?
                .iter()
                .map(parse_source_span)
                .collect::<Result<Vec<_>, _>>()
        })
        .transpose()?
        .unwrap_or_default();
    Ok(StatementQualifiers {
        negated: semantic_bool(
            semantic_required(map, 0, "qualifiers.negated")?,
            "qualifiers.negated",
        )?,
        modality,
        condition: semantic_optional(map, 2)
            .map(|value| semantic_u32(value, "qualifiers.condition").map(StatementId))
            .transpose()?,
        time: semantic_optional(map, 3).map(parse_term).transpose()?,
        location: semantic_optional(map, 4).map(parse_term).transpose()?,
        perspective: semantic_optional(map, 5).map(parse_term).transpose()?,
        tolerance: semantic_optional(map, 6).map(parse_quantity).transpose()?,
        source_spans,
    })
}

fn parse_source_span(value: &CanonicalValue) -> Result<SourceSpan, SemanticError> {
    let map = semantic_map(value, "source_span")?;
    Ok(SourceSpan {
        source: ObjectReference::from_value(semantic_required(map, 0, "source_span.source")?)?,
        start: semantic_unsigned(
            semantic_required(map, 1, "source_span.start")?,
            "source_span.start",
        )?,
        end: semantic_unsigned(
            semantic_required(map, 2, "source_span.end")?,
            "source_span.end",
        )?,
    })
}

fn semantic_map<'a>(
    value: &'a CanonicalValue,
    field: &'static str,
) -> Result<&'a [(u64, CanonicalValue)], SemanticError> {
    match value {
        CanonicalValue::Map(values) => Ok(values),
        _ => Err(SemanticError::InvalidField(field)),
    }
}

fn semantic_array<'a>(
    value: &'a CanonicalValue,
    field: &'static str,
) -> Result<&'a [CanonicalValue], SemanticError> {
    match value {
        CanonicalValue::Array(values) => Ok(values),
        _ => Err(SemanticError::InvalidField(field)),
    }
}

fn semantic_required<'a>(
    map: &'a [(u64, CanonicalValue)],
    key: u64,
    field: &'static str,
) -> Result<&'a CanonicalValue, SemanticError> {
    semantic_optional(map, key).ok_or(SemanticError::InvalidField(field))
}

fn semantic_optional(map: &[(u64, CanonicalValue)], key: u64) -> Option<&CanonicalValue> {
    map.iter()
        .find_map(|(candidate, value)| (*candidate == key).then_some(value))
}

fn semantic_unsigned(value: &CanonicalValue, field: &'static str) -> Result<u64, SemanticError> {
    match value {
        CanonicalValue::Unsigned(value) => Ok(*value),
        _ => Err(SemanticError::InvalidField(field)),
    }
}

fn semantic_u32(value: &CanonicalValue, field: &'static str) -> Result<u32, SemanticError> {
    u32::try_from(semantic_unsigned(value, field)?).map_err(|_| SemanticError::InvalidField(field))
}

fn semantic_bool(value: &CanonicalValue, field: &'static str) -> Result<bool, SemanticError> {
    match value {
        CanonicalValue::Bool(value) => Ok(*value),
        _ => Err(SemanticError::InvalidField(field)),
    }
}

fn semantic_ccid(
    value: &CanonicalValue,
    field: &'static str,
) -> Result<ConceptCcid, SemanticError> {
    Ok(ConceptCcid::from_bytes(semantic_fixed_bytes::<16>(
        value, field,
    )?))
}

fn semantic_fixed_bytes<const N: usize>(
    value: &CanonicalValue,
    field: &'static str,
) -> Result<[u8; N], SemanticError> {
    let CanonicalValue::Bytes(bytes) = value else {
        return Err(SemanticError::InvalidField(field));
    };
    bytes
        .as_slice()
        .try_into()
        .map_err(|_| SemanticError::InvalidField(field))
}

fn visit_statement_terms(
    statement: &StatementFrame,
    visitor: &mut impl FnMut(&TermRef) -> Result<(), SemanticError>,
) -> Result<(), SemanticError> {
    for term in &statement.arguments {
        visitor(term)?;
    }
    for constraint in &statement.constraints {
        match &constraint.expression {
            ConstraintExpression::Compare { left, right, .. } => {
                visitor(left)?;
                visitor(right)?;
            }
            ConstraintExpression::Dimension { term, .. }
            | ConstraintExpression::Range { term, .. } => visitor(term)?,
        }
    }
    for term in [
        &statement.qualifiers.time,
        &statement.qualifiers.location,
        &statement.qualifiers.perspective,
    ] {
        if let Some(term) = term {
            visitor(term)?;
        }
    }
    Ok(())
}

fn normalize_constraint(
    expression: &mut ConstraintExpression,
    statements: &BTreeMap<StatementId, StatementId>,
    variable_types: &BTreeMap<VariableId, Option<ConceptCcid>>,
    variables: &mut BTreeMap<VariableId, VariableId>,
    receptors: &mut BTreeMap<ReceptorSlotId, ReceptorSlotId>,
) -> Result<(), SemanticError> {
    match expression {
        ConstraintExpression::Compare { left, right, .. } => {
            normalize_term(left, statements, variable_types, variables, receptors)?;
            normalize_term(right, statements, variable_types, variables, receptors)
        }
        ConstraintExpression::Dimension { term, .. } | ConstraintExpression::Range { term, .. } => {
            normalize_term(term, statements, variable_types, variables, receptors)
        }
    }
}

fn normalize_term(
    term: &mut TermRef,
    statements: &BTreeMap<StatementId, StatementId>,
    variable_types: &BTreeMap<VariableId, Option<ConceptCcid>>,
    variables: &mut BTreeMap<VariableId, VariableId>,
    receptors: &mut BTreeMap<ReceptorSlotId, ReceptorSlotId>,
) -> Result<(), SemanticError> {
    match term {
        TermRef::Variable {
            id,
            type_constraint,
        } => {
            let next = VariableId(
                u32::try_from(variables.len()).map_err(|_| SemanticError::LimitVariables)?,
            );
            let original = *id;
            *id = *variables.entry(original).or_insert(next);
            *type_constraint = variable_types.get(&original).copied().flatten();
        }
        TermRef::Statement(id) => {
            *id = *statements
                .get(id)
                .ok_or(SemanticError::UnknownStatementReference(*id))?;
        }
        TermRef::Receptor { slot, .. } => {
            let next = ReceptorSlotId(
                u32::try_from(receptors.len()).map_err(|_| SemanticError::LimitReceptors)?,
            );
            *slot = *receptors.entry(*slot).or_insert(next);
        }
        TermRef::Concept(_) | TermRef::Literal(_) | TermRef::KnowledgeObject(_) => {}
    }
    Ok(())
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum SemanticError {
    Canonical(CanonicalError),
    Object(ObjectError),
    ZeroDenominator,
    Overflow,
    DimensionOverflow,
    DimensionMismatch,
    LimitStatements,
    LimitArguments,
    LimitConstraints,
    LimitVariables,
    LimitReceptors,
    DuplicateStatementId(StatementId),
    UnknownStatementReference(StatementId),
    VariableTypeConflict(VariableId),
    InvalidSourceSpan,
    InvalidField(&'static str),
    NonCanonicalValue,
}

impl From<CanonicalError> for SemanticError {
    fn from(error: CanonicalError) -> Self {
        Self::Canonical(error)
    }
}

impl From<ObjectError> for SemanticError {
    fn from(error: ObjectError) -> Self {
        Self::Object(error)
    }
}

impl fmt::Display for SemanticError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Canonical(error) => write!(f, "{error}"),
            Self::Object(error) => write!(f, "{error}"),
            other => write!(f, "SEMANTIC_IR: {other:?}"),
        }
    }
}

impl std::error::Error for SemanticError {}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::foundation::decode_knowledge_object;

    fn concept(byte: u8) -> ConceptCcid {
        ConceptCcid::from_bytes([byte; 16])
    }

    fn variable(id: u32, type_byte: u8) -> TermRef {
        TermRef::Variable {
            id: VariableId(id),
            type_constraint: Some(concept(type_byte)),
        }
    }

    fn frame(variable_id: u32, statement_id: u32) -> SemanticFrameSet {
        SemanticFrameSet {
            statements: vec![StatementFrame {
                statement_id: StatementId(statement_id),
                operator_or_predicate: concept(1),
                arguments: vec![
                    variable(variable_id, 2),
                    TermRef::Concept(concept(3)),
                    variable(variable_id, 2),
                ],
                constraints: vec![TypedConstraint {
                    expression: ConstraintExpression::Dimension {
                        term: variable(variable_id, 2),
                        expected: DimensionVector::MASS,
                    },
                    required: true,
                }],
                qualifiers: StatementQualifiers::default(),
            }],
        }
    }

    #[test]
    fn alpha_renaming_variable_and_statement_ids_preserves_bytes() {
        let left = frame(7, 90).canonical_bytes().unwrap();
        let right = frame(900, 3).canonical_bytes().unwrap();
        assert_eq!(left, right);
    }

    #[test]
    fn concepts_are_full_ccid_bytes_not_local_u64_operands() {
        let value = frame(7, 90).canonical_value().unwrap();
        let CanonicalValue::Map(root) = value else {
            panic!("semantic root must be a map");
        };
        let CanonicalValue::Array(statements) = &root[2].1 else {
            panic!("statements must be an array");
        };
        let CanonicalValue::Map(statement) = &statements[0] else {
            panic!("statement must be a map");
        };
        assert!(matches!(&statement[1].1, CanonicalValue::Bytes(bytes) if bytes.len() == 16));
    }

    #[test]
    fn unit_dimension_and_affine_conversion_are_exact() {
        let celsius = UnitRef {
            unit: concept(10),
            dimension: DimensionVector::TEMPERATURE,
            scale_to_base: ExactRatio::integer(1),
            offset_to_base: ExactRatio::new(27_315, 100).unwrap(),
        };
        let kelvin = UnitRef::coherent(concept(11), DimensionVector::TEMPERATURE);
        let zero_c = QuantityLiteral {
            value: ExactRatio::integer(0),
            source_unit: celsius,
        };
        let freezing_k = QuantityLiteral {
            value: ExactRatio::new(27_315, 100).unwrap(),
            source_unit: kelvin,
        };
        assert_eq!(zero_c.compare(&freezing_k).unwrap(), Ordering::Equal);

        let velocity = DimensionVector::LENGTH
            .checked_div(DimensionVector::TIME)
            .unwrap();
        assert_eq!(velocity.exponents(), [1, 0, -1, 0, 0, 0, 0]);
    }

    #[test]
    fn exact_ratio_division_reduces_sign_and_rejects_zero() {
        assert_eq!(
            ExactRatio::new(2, 3)
                .unwrap()
                .checked_div(ExactRatio::new(4, 5).unwrap())
                .unwrap(),
            ExactRatio::new(5, 6).unwrap()
        );
        assert_eq!(
            ExactRatio::new(2, 3)
                .unwrap()
                .checked_div(ExactRatio::new(-4, 5).unwrap())
                .unwrap(),
            ExactRatio::new(-5, 6).unwrap()
        );
        assert_eq!(
            ExactRatio::integer(1)
                .checked_div(ExactRatio::integer(0))
                .unwrap_err(),
            SemanticError::ZeroDenominator
        );
    }

    #[test]
    fn incompatible_dimensions_are_unknown_to_comparison_not_false() {
        let mass = QuantityLiteral {
            value: ExactRatio::integer(1),
            source_unit: UnitRef::coherent(concept(10), DimensionVector::MASS),
        };
        let length = QuantityLiteral {
            value: ExactRatio::integer(1),
            source_unit: UnitRef::coherent(concept(11), DimensionVector::LENGTH),
        };
        assert_eq!(mass.compare(&length), Err(SemanticError::DimensionMismatch));
        let disposition = mass
            .compare(&length)
            .map(|_| ConstraintEvaluation::Satisfied)
            .unwrap_or(ConstraintEvaluation::Unknown);
        assert_eq!(disposition, ConstraintEvaluation::Unknown);
    }

    #[test]
    fn semantic_frame_wraps_as_generic_immutable_object() {
        let object = frame(7, 90)
            .to_knowledge_object(DisclosureClass::Public)
            .unwrap();
        let (bytes, cid) = object.encode(ResourceProfile::ObjectV1).unwrap();
        let decoded = decode_knowledge_object(
            &bytes,
            ResourceProfile::ObjectV1,
            &[super::super::object::KnownObjectKind::new(
                SEMANTIC_KERNEL_OBJECT_KIND,
                SEMANTIC_PROFILE_MAJOR,
            )],
            &[],
        )
        .unwrap();
        assert_eq!(decoded.cid(), cid);
        assert!(!decoded.is_opaque());
    }

    #[test]
    fn variable_type_conflict_and_unknown_statement_are_rejected() {
        let mut conflict = frame(7, 90);
        conflict.statements[0].arguments.push(variable(7, 99));
        assert!(matches!(
            conflict.alpha_normalized(),
            Err(SemanticError::VariableTypeConflict(VariableId(7)))
        ));

        let mut unknown = frame(7, 90);
        unknown.statements[0].qualifiers.condition = Some(StatementId(404));
        assert!(matches!(
            unknown.alpha_normalized(),
            Err(SemanticError::UnknownStatementReference(StatementId(404)))
        ));
    }
}

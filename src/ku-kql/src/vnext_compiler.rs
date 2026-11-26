//! Deterministic Bounded `FIND -> KnowledgeNeedIr` Semantic Compiler (`KQL-CMP-001`).
//!
//! Eliminates hash-derived synthetic CCIDs and `SelectorCid`s (`P3-KQL-COMPILER-M6A-001` §2):
//! - Rejects non-semantic runtime, legacy, and economic fields fail-closed with
//!   [`NON_SEMANTIC_FIELD_ERROR_CODE`].
//! - Rejects IEEE-754 floating-point literals (`f64` / scientific float notation) and lowers
//!   exact decimal or rational strings into reduced [`ExactRatio`] + SI [`UnitRef`].
//! - Resolves every concept, predicate, and unit against a pinned [`ConceptLookup`] and
//!   `registry_release_root`, applying `P1` [`AuthoringConceptResolution`] (`ConceptRedirect`,
//!   split disambiguation, retirement) without ever hashing raw strings to invent CCIDs.
//! - Applies deterministic commutative constraint sorting + [`SemanticFrameSet::alpha_normalized`]
//!   so queries differing only in whitespace, variable names (`?a` vs `?x`), or commutative
//!   constraint order compile to byte-identical `ReceptorDefinition` and `QueryDefinition` CIDs.
//! - Enforces the `SelectorCid` allow-list firewall (`NEG-KQL-001`, `NEG-KQL-005`) and zeroizes
//!   raw query buffers (`Zeroize`) upon constructing [`CompiledPrivateNeed`].

use std::collections::{BTreeMap, BTreeSet};

use ku_core::concept_lifecycle::{AuthoringConceptResolution, ConceptRetireReason};
use ku_core::concept_registry::{ConceptCategory, ConceptLookup, ResolveResult, ResolvedConcept};
use ku_core::foundation::{
    encode_canonical, ComparisonOperator, ConceptCcid, ConstraintExpression, DimensionVector,
    DisclosureClass, EventCid, ExactRatio, LiteralValue, Modality, NormalizedText, ObjectCid,
    ObjectReference, QuantityLiteral, ReceptorAcceptanceProfile, ReceptorCardinality,
    ReceptorDefinition, ReceptorError, ReceptorOrigin, ResourceProfile, SelectorCid, SemanticError,
    SemanticFrameSet, StatementFrame, StatementId, StatementQualifiers, TermRef, TypedConstraint,
    UnitRef, UnknownConstraintPolicy, VariableId, RECEPTOR_DEFINITION_KIND,
};
use zeroize::{Zeroize, Zeroizing};

use crate::ast::{
    CompOp, Condition, EdgeDirection, FieldPath, FindQuery, NodeLabel, Property, Query, ReturnExpr,
    Scope, Value,
};
use crate::parser::parse_query;
use crate::vnext_matcher::MatcherMetricConcepts;
use crate::vnext_private_need::{PrivateNeedBundle, PrivateNeedError};
use crate::vnext_query::{KnowledgeNeedIr, QueryContractError, QueryDefinition};
use crate::vnext_reunion::LocalNeedTarget;
use crate::vnext_standing_need::StandingNeed;

pub const COMPILER_PROFILE_ID: &str = "kql-need-compiler/1";
pub const MAX_COMPILED_STATEMENTS: usize = 8;
pub const MAX_OPEN_SLOTS: usize = 8;
pub const MAX_CONSTRAINTS: usize = 32;
pub const MAX_DISAMBIGUATION_CANDIDATES: usize = 16;
pub const NON_SEMANTIC_FIELD_ERROR_CODE: &str = "non_semantic_field_forbidden_in_need_ir";

const FORBIDDEN_NON_SEMANTIC_FIELDS: &[&str] = &[
    "pomv",
    "trust_score",
    "trust",
    "metabolic_rate",
    "obt_balance",
    "obt",
    "peer_count",
    "wire_size",
    "encoding_status",
    "gene_type",
    "certainty",
    "difficulty",
    "confidence",
    "verification_level",
    "corroboration_count",
    "challenge_count",
    "error_susceptibility",
    "bond_count",
    "epistemic_status",
    "evidence_type",
    "prediction_score",
    "entropy_at_creation",
    "survival_score",
    "synaptic_centrality",
    "niche_fitness",
    "concept_table_size",
    "instruction_count",
    "has_triple",
    "has_step",
    "epi",
    "expression",
    "cid",
];

/// Record of a `P1` `ConceptRedirect` applied during authoring/query compilation.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub struct LifecycleRedirectApplied {
    pub deprecated_ccid: ConceptCcid,
    pub canonical_ccid: ConceptCcid,
}

/// Auditable provenance recorded alongside a compiled private need (`KQL-CMP-001` §2.2–2.3).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct NeedCompilerProvenance {
    pub compiler_profile: &'static str,
    pub registry_release_root: [u8; 32],
    pub lifecycle_redirects_applied: Vec<LifecycleRedirectApplied>,
}

/// Output of `KqlNeedCompiler`, ready for `RedbPrivateNeedVault` sealing and local matching.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CompiledPrivateNeed {
    pub bundle: PrivateNeedBundle,
    pub receptor_cid: ObjectCid,
    pub query_cid: ObjectCid,
    pub provenance: NeedCompilerProvenance,
    pub limitations: Vec<&'static str>,
}

impl CompiledPrivateNeed {
    pub fn omits_hash_derived_limitation(&self) -> bool {
        !self
            .limitations
            .contains(&"semantic_mapping_is_hash_derived_not_compiled")
    }
}

/// Deterministic SI unit specification used by the host compiler.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct SiUnitSpec {
    pub dimension: DimensionVector,
    pub scale_to_base: ExactRatio,
    pub offset_to_base: ExactRatio,
}

/// Host SI unit transform catalog mapping unit symbols/names or canonical unit CCIDs to
/// exact rational affine transforms (`base = source * scale_to_base + offset_to_base`).
#[derive(Clone, Debug, Default)]
pub struct SiUnitCatalog {
    by_label: BTreeMap<String, SiUnitSpec>,
    by_ccid: BTreeMap<ConceptCcid, SiUnitSpec>,
}

impl SiUnitCatalog {
    /// Standard SI base, prefixed, and affine units (`m`, `km`, `mm`, `kg`, `g`, `s`, `ms`,
    /// `K`, `degC`/`°C`, `Pa`, `kPa`, `J`, `W`, etc.) using exact integer/rational ratios.
    pub fn standard() -> Self {
        let mut catalog = Self::default();
        let int = ExactRatio::integer;
        let ratio = |n: i64, d: u64| ExactRatio::new(n, d).expect("valid constant SI ratio");

        // Length (base: meter)
        for &(names, scale) in &[
            (&["m", "meter", "metre"][..], int(1)),
            (&["km", "kilometer", "kilometre"][..], int(1_000)),
            (&["cm", "centimeter", "centimetre"][..], ratio(1, 100)),
            (&["mm", "millimeter", "millimetre"][..], ratio(1, 1_000)),
            (&["um", "micrometer", "micrometre"][..], ratio(1, 1_000_000)),
            (
                &["nm", "nanometer", "nanometre"][..],
                ratio(1, 1_000_000_000),
            ),
        ] {
            catalog.insert_labels(names, DimensionVector::LENGTH, scale, int(0));
        }

        // Mass (base: kilogram)
        for &(names, scale) in &[
            (&["kg", "kilogram"][..], int(1)),
            (&["g", "gram"][..], ratio(1, 1_000)),
            (&["mg", "milligram"][..], ratio(1, 1_000_000)),
        ] {
            catalog.insert_labels(names, DimensionVector::MASS, scale, int(0));
        }

        // Time (base: second)
        for &(names, scale) in &[
            (&["s", "sec", "second"][..], int(1)),
            (&["ms", "millisecond"][..], ratio(1, 1_000)),
            (&["min", "minute"][..], int(60)),
            (&["h", "hr", "hour"][..], int(3_600)),
        ] {
            catalog.insert_labels(names, DimensionVector::TIME, scale, int(0));
        }

        // Temperature (base: kelvin; degC: T_K = T_C * 1 + 273.15 = T_C + 27315/100)
        catalog.insert_labels(
            &["k", "kelvin"],
            DimensionVector::TEMPERATURE,
            int(1),
            int(0),
        );
        catalog.insert_labels(
            &["degc", "°c", "celsius", "degree celsius", "c"],
            DimensionVector::TEMPERATURE,
            int(1),
            ratio(27_315, 100),
        );
        catalog.insert_labels(
            &["degf", "°f", "fahrenheit", "degree fahrenheit"],
            DimensionVector::TEMPERATURE,
            ratio(5, 9),
            ratio(45_967, 180),
        );

        // Electric current (base: ampere)
        let current = DimensionVector::new([0, 0, 0, 1, 0, 0, 0]);
        catalog.insert_labels(&["a", "ampere"], current, int(1), int(0));
        catalog.insert_labels(&["ma", "milliampere"], current, ratio(1, 1_000), int(0));

        // Amount of substance (base: mole)
        let amount = DimensionVector::new([0, 0, 0, 0, 0, 1, 0]);
        catalog.insert_labels(&["mol", "mole"], amount, int(1), int(0));
        catalog.insert_labels(&["mmol", "millimole"], amount, ratio(1, 1_000), int(0));

        // Pressure (Pa = kg * m^-1 * s^-2)
        let pressure = DimensionVector::new([-1, 1, -2, 0, 0, 0, 0]);
        catalog.insert_labels(&["pa", "pascal"], pressure, int(1), int(0));
        catalog.insert_labels(&["kpa", "kilopascal"], pressure, int(1_000), int(0));
        catalog.insert_labels(&["bar"], pressure, int(100_000), int(0));
        catalog.insert_labels(&["atm", "atmosphere"], pressure, int(101_325), int(0));

        // Energy (J = kg * m^2 * s^-2) & Power (W = kg * m^2 * s^-3)
        let energy = DimensionVector::new([2, 1, -2, 0, 0, 0, 0]);
        catalog.insert_labels(&["j", "joule"], energy, int(1), int(0));
        catalog.insert_labels(&["kj", "kilojoule"], energy, int(1_000), int(0));
        let power = DimensionVector::new([2, 1, -3, 0, 0, 0, 0]);
        catalog.insert_labels(&["w", "watt"], power, int(1), int(0));
        catalog.insert_labels(&["kw", "kilowatt"], power, int(1_000), int(0));

        // Velocity (m * s^-1)
        let velocity = DimensionVector::new([1, 0, -1, 0, 0, 0, 0]);
        catalog.insert_labels(&["m/s", "meter per second"], velocity, int(1), int(0));
        catalog.insert_labels(
            &["km/h", "kilometer per hour"],
            velocity,
            ratio(5, 18),
            int(0),
        );

        // Dimensionless
        catalog.insert_labels(
            &["1", "dimensionless", "ratio"],
            DimensionVector::DIMENSIONLESS,
            int(1),
            int(0),
        );
        catalog.insert_labels(
            &["%", "percent"],
            DimensionVector::DIMENSIONLESS,
            ratio(1, 100),
            int(0),
        );
        catalog.insert_labels(
            &["ppm"],
            DimensionVector::DIMENSIONLESS,
            ratio(1, 1_000_000),
            int(0),
        );

        catalog
    }

    pub fn register_ccid(&mut self, unit_ccid: ConceptCcid, spec: SiUnitSpec) {
        self.by_ccid.insert(unit_ccid, spec);
    }

    pub fn register_label(&mut self, label: impl Into<String>, spec: SiUnitSpec) {
        self.by_label
            .insert(label.into().trim().to_lowercase(), spec);
    }

    fn insert_labels(
        &mut self,
        labels: &[&str],
        dimension: DimensionVector,
        scale_to_base: ExactRatio,
        offset_to_base: ExactRatio,
    ) {
        let spec = SiUnitSpec {
            dimension,
            scale_to_base,
            offset_to_base,
        };
        for label in labels {
            self.by_label.insert((*label).to_lowercase(), spec);
        }
    }

    pub fn lookup(
        &self,
        ccid: ConceptCcid,
        canonical_name: &str,
        raw_token: &str,
    ) -> Option<SiUnitSpec> {
        if let Some(spec) = self.by_ccid.get(&ccid).copied() {
            return Some(spec);
        }
        let canonical_key = canonical_name.trim().to_lowercase();
        if let Some(spec) = self.by_label.get(&canonical_key).copied() {
            return Some(spec);
        }
        let raw_key = raw_token.trim().to_lowercase();
        self.by_label.get(&raw_key).copied()
    }
}

/// Pinned execution environment and policy anchors for compiling a `KnowledgeNeedIr`.
#[derive(Clone)]
pub struct NeedCompileContext<'a> {
    pub registry: &'a dyn ConceptLookup,
    pub registry_release_root: [u8; 32],
    pub unit_catalog: &'a SiUnitCatalog,
    pub allowed_public_selectors: &'a [SelectorCid],
    pub selector: SelectorCid,
    pub query_policy: ObjectReference,
    pub exploration_policy: ObjectReference,
    pub watch_policy: ObjectReference,
    pub acceptance_policy: ObjectReference,
    pub observed_frontier: [u8; 32],
    pub source_frontier: EventCid,
    pub generator: ObjectReference,
    pub metrics: MatcherMetricConcepts,
    pub unmapped_reason: ConceptCcid,
    pub created_at_evaluation: u64,
    pub expires_after_evaluations: u64,
}

/// Quantity input before exact rational + SI resolution.
#[derive(Clone, Debug, PartialEq)]
pub enum NeedQuantityDraftV1 {
    /// Exact decimal (`"12.5"`) or exact rational (`"25/2"`), paired with a unit label or 32-hex CCID.
    Exact { number: String, unit: String },
    /// Explicit IEEE-754 float trap so callers forwarding untyped JSON numbers fail closed.
    FloatForbidden(f64),
}

/// Term draft inside `StructuredNeedCompileRequestV1`.
#[derive(Clone, Debug, PartialEq)]
pub enum NeedTermDraftV1 {
    /// Concept label or 32-hex `ConceptCcid` resolved via `ConceptLookup`.
    ConceptToken(String),
    /// Open variable binder (`"?x"`, `"x"`, etc.), alpha-normalized deterministically.
    OpenSlot {
        name: String,
        type_constraint: Option<String>,
    },
    /// Exact quantity literal.
    Quantity(NeedQuantityDraftV1),
    /// Exact normalized UTF-8 literal.
    Text(String),
    /// Boolean literal.
    Boolean(bool),
}

/// Typed constraint draft inside `StructuredNeedCompileRequestV1`.
#[derive(Clone, Debug, PartialEq)]
pub enum NeedConstraintDraftV1 {
    Compare {
        left: NeedTermDraftV1,
        operator: ComparisonOperator,
        right: NeedTermDraftV1,
        required: bool,
    },
    Range {
        term: NeedTermDraftV1,
        lower: NeedQuantityDraftV1,
        upper: NeedQuantityDraftV1,
        include_lower: bool,
        include_upper: bool,
        required: bool,
    },
    Dimension {
        term: NeedTermDraftV1,
        expected: DimensionVector,
        required: bool,
    },
}

/// Statement qualifiers draft inside `StructuredNeedCompileRequestV1`.
#[derive(Clone, Debug, PartialEq)]
pub struct NeedQualifiersDraftV1 {
    pub negated: bool,
    pub modality: Modality,
    pub condition_statement_index: Option<u32>,
    pub time: Option<NeedTermDraftV1>,
    pub location: Option<NeedTermDraftV1>,
    pub perspective: Option<NeedTermDraftV1>,
    pub tolerance: Option<NeedQuantityDraftV1>,
}

impl Default for NeedQualifiersDraftV1 {
    fn default() -> Self {
        Self {
            negated: false,
            modality: Modality::Asserted,
            condition_statement_index: None,
            time: None,
            location: None,
            perspective: None,
            tolerance: None,
        }
    }
}

/// Single semantic statement draft inside `StructuredNeedCompileRequestV1`.
#[derive(Clone, Debug, PartialEq)]
pub struct NeedStatementDraftV1 {
    pub predicate: String,
    pub arguments: Vec<NeedTermDraftV1>,
    pub constraints: Vec<NeedConstraintDraftV1>,
    pub qualifiers: NeedQualifiersDraftV1,
}

/// Structured, schema-validated compile request (`KQL-CMP-001` §2.1).
#[derive(Clone, Debug, PartialEq)]
pub struct StructuredNeedCompileRequestV1 {
    pub target_role: Option<String>,
    pub expected_types: Vec<String>,
    pub statements: Vec<NeedStatementDraftV1>,
    pub local_context: Vec<NeedStatementDraftV1>,
    pub hard_constraints: Vec<NeedConstraintDraftV1>,
    pub unknown_constraint_policy: UnknownConstraintPolicy,
    pub cardinality: ReceptorCardinality,
}

impl StructuredNeedCompileRequestV1 {
    pub fn single_statement(statement: NeedStatementDraftV1) -> Self {
        Self {
            target_role: None,
            expected_types: Vec::new(),
            statements: vec![statement],
            local_context: Vec::new(),
            hard_constraints: Vec::new(),
            unknown_constraint_policy: UnknownConstraintPolicy::KeepUnresolved,
            cardinality: ReceptorCardinality {
                minimum: 1,
                maximum: None,
            },
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq, thiserror::Error)]
pub enum NeedCompileError {
    #[error("non-semantic field forbidden in NeedIR: {0}")]
    NonSemanticFieldForbidden(&'static str),
    #[error("IEEE-754 floating-point literal forbidden in NeedIR")]
    FloatLiteralForbidden,
    #[error("pinned registry_release_root must be non-zero")]
    InvalidRegistryReleaseRoot,
    #[error("concept registry unavailable: {0}")]
    RegistryUnavailable(String),
    #[error("unresolved semantic term '{token}'")]
    UnresolvedTerm { token: String },
    #[error("ambiguous semantic term '{token}' requires explicit disambiguation")]
    AmbiguousTerm {
        token: String,
        candidates: Vec<(ConceptCcid, String)>,
    },
    #[error("concept '{token}' split upstream and requires disambiguation")]
    SplitDisambiguationRequired {
        token: String,
        parent: ConceptCcid,
        disambiguation_role: ConceptCcid,
        candidates: Vec<(ConceptCcid, String)>,
    },
    #[error("concept '{token}' ({ccid}) is retired ({reason:?})")]
    RetiredConcept {
        token: String,
        ccid: ConceptCcid,
        reason: ConceptRetireReason,
    },
    #[error("invalid or non-unit token '{token}'")]
    InvalidUnit { token: String },
    #[error("invalid exact decimal/rational quantity '{raw}'")]
    InvalidQuantity { raw: String },
    #[error("exact rational quantity overflow")]
    QuantityOverflow,
    #[error("compiled statement count {count} exceeds limit {max}")]
    StatementLimitExceeded { count: usize, max: usize },
    #[error("open variable slot count {count} exceeds limit {max}")]
    OpenSlotLimitExceeded { count: usize, max: usize },
    #[error("compiled constraint count {count} exceeds limit {max}")]
    ConstraintLimitExceeded { count: usize, max: usize },
    #[error("selector is not in the local public allow-list")]
    UnapprovedSelector(SelectorCid),
    #[error("unsupported KQL query form for NeedIR compilation: {0}")]
    UnsupportedQueryForm(String),
    #[error("KQL parse error: {0}")]
    Parse(String),
    #[error("compiled need must contain at least one statement")]
    EmptyNeed,
    #[error("semantic validation error: {0:?}")]
    Semantic(SemanticError),
    #[error("receptor validation error: {0:?}")]
    Receptor(ReceptorError),
    #[error("query contract error: {0:?}")]
    QueryContract(QueryContractError),
    #[error("private need bundle validation error: {0}")]
    PrivateNeed(String),
}

impl From<SemanticError> for NeedCompileError {
    fn from(error: SemanticError) -> Self {
        Self::Semantic(error)
    }
}

impl From<ReceptorError> for NeedCompileError {
    fn from(error: ReceptorError) -> Self {
        Self::Receptor(error)
    }
}

impl From<QueryContractError> for NeedCompileError {
    fn from(error: QueryContractError) -> Self {
        Self::QueryContract(error)
    }
}

impl From<PrivateNeedError> for NeedCompileError {
    fn from(error: PrivateNeedError) -> Self {
        Self::PrivateNeed(format!("{error:?}"))
    }
}

/// Parse an exact decimal (`"-12.5"`, `"273.15"`) or rational (`"3/4"`) string into a reduced
/// [`ExactRatio`] without ever touching IEEE-754 floating-point arithmetic.
pub fn parse_exact_decimal_or_rational(raw: &str) -> Result<ExactRatio, NeedCompileError> {
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        return Err(NeedCompileError::InvalidQuantity {
            raw: raw.to_string(),
        });
    }
    let lower = trimmed.to_ascii_lowercase();
    if lower == "nan"
        || lower == "inf"
        || lower == "+inf"
        || lower == "-inf"
        || lower == "infinity"
        || lower == "+infinity"
        || lower == "-infinity"
        || lower.contains('e')
    {
        return Err(NeedCompileError::FloatLiteralForbidden);
    }

    if let Some((num_str, den_str)) = trimmed.split_once('/') {
        if den_str.contains('/') || num_str.contains('.') || den_str.contains('.') {
            return Err(NeedCompileError::InvalidQuantity {
                raw: raw.to_string(),
            });
        }
        let numerator: i64 =
            num_str
                .trim()
                .parse()
                .map_err(|_| NeedCompileError::InvalidQuantity {
                    raw: raw.to_string(),
                })?;
        let denominator: u64 =
            den_str
                .trim()
                .parse()
                .map_err(|_| NeedCompileError::InvalidQuantity {
                    raw: raw.to_string(),
                })?;
        if denominator == 0 {
            return Err(NeedCompileError::InvalidQuantity {
                raw: raw.to_string(),
            });
        }
        return ExactRatio::new(numerator, denominator)
            .map_err(|_| NeedCompileError::QuantityOverflow);
    }

    if let Some((int_part, frac_part)) = trimmed.split_once('.') {
        if frac_part.contains('.') || frac_part.is_empty() {
            return Err(NeedCompileError::InvalidQuantity {
                raw: raw.to_string(),
            });
        }
        if !frac_part.chars().all(|ch| ch.is_ascii_digit()) {
            return Err(NeedCompileError::InvalidQuantity {
                raw: raw.to_string(),
            });
        }
        let negative = int_part.starts_with('-');
        let unsigned_int_str = int_part
            .strip_prefix('-')
            .or_else(|| int_part.strip_prefix('+'))
            .unwrap_or(int_part);
        if unsigned_int_str.is_empty() || !unsigned_int_str.chars().all(|ch| ch.is_ascii_digit()) {
            return Err(NeedCompileError::InvalidQuantity {
                raw: raw.to_string(),
            });
        }
        if frac_part.len() > 18 {
            return Err(NeedCompileError::QuantityOverflow);
        }
        let scale = 10_u64
            .checked_pow(frac_part.len() as u32)
            .ok_or(NeedCompileError::QuantityOverflow)?;
        let whole: u128 = unsigned_int_str
            .parse()
            .map_err(|_| NeedCompileError::QuantityOverflow)?;
        let frac: u128 = frac_part
            .parse()
            .map_err(|_| NeedCompileError::QuantityOverflow)?;
        let combined = whole
            .checked_mul(u128::from(scale))
            .and_then(|v| v.checked_add(frac))
            .ok_or(NeedCompileError::QuantityOverflow)?;
        let signed: i128 = if negative {
            -(i128::try_from(combined).map_err(|_| NeedCompileError::QuantityOverflow)?)
        } else {
            i128::try_from(combined).map_err(|_| NeedCompileError::QuantityOverflow)?
        };
        let numerator = i64::try_from(signed).map_err(|_| NeedCompileError::QuantityOverflow)?;
        return ExactRatio::new(numerator, scale).map_err(|_| NeedCompileError::QuantityOverflow);
    }

    let integer: i64 = trimmed
        .parse()
        .map_err(|_| NeedCompileError::InvalidQuantity {
            raw: raw.to_string(),
        })?;
    Ok(ExactRatio::integer(integer))
}

/// Deterministic `FIND -> KnowledgeNeedIr` semantic compiler (`KQL-CMP-001`).
pub struct KqlNeedCompiler;

impl KqlNeedCompiler {
    /// Compile a textual `FIND` KQL string while zeroizing the caller-owned query buffer
    /// immediately once lowered.
    pub fn compile_kql_in_place(
        raw_kql: &mut String,
        ctx: &NeedCompileContext<'_>,
    ) -> Result<CompiledPrivateNeed, NeedCompileError> {
        let owned = Zeroizing::new(std::mem::take(raw_kql));
        raw_kql.zeroize();
        let structured = lower_textual_find_to_structured(&owned)?;
        drop(owned);
        Self::compile_structured(&structured, ctx)
    }

    /// Compile a textual `FIND` KQL slice using an internal `Zeroizing<String>` scratch buffer.
    pub fn compile_kql(
        raw_kql: &str,
        ctx: &NeedCompileContext<'_>,
    ) -> Result<CompiledPrivateNeed, NeedCompileError> {
        let mut scratch = String::from(raw_kql);
        Self::compile_kql_in_place(&mut scratch, ctx)
    }

    /// Compile a typed [`StructuredNeedCompileRequestV1`] against a pinned `ConceptLookup`
    /// and allow-listed public `SelectorCid`.
    pub fn compile_structured(
        request: &StructuredNeedCompileRequestV1,
        ctx: &NeedCompileContext<'_>,
    ) -> Result<CompiledPrivateNeed, NeedCompileError> {
        validate_context(ctx)?;

        if request.statements.is_empty() {
            return Err(NeedCompileError::EmptyNeed);
        }
        if request.statements.len() > MAX_COMPILED_STATEMENTS {
            return Err(NeedCompileError::StatementLimitExceeded {
                count: request.statements.len(),
                max: MAX_COMPILED_STATEMENTS,
            });
        }
        if request.local_context.len() > MAX_COMPILED_STATEMENTS {
            return Err(NeedCompileError::StatementLimitExceeded {
                count: request.local_context.len(),
                max: MAX_COMPILED_STATEMENTS,
            });
        }

        let total_constraints = request.hard_constraints.len()
            + request
                .statements
                .iter()
                .map(|s| s.constraints.len())
                .sum::<usize>()
            + request
                .local_context
                .iter()
                .map(|s| s.constraints.len())
                .sum::<usize>();
        if total_constraints > MAX_CONSTRAINTS {
            return Err(NeedCompileError::ConstraintLimitExceeded {
                count: total_constraints,
                max: MAX_CONSTRAINTS,
            });
        }

        let mut resolver = PinnedConceptResolver {
            registry: ctx.registry,
            unit_catalog: ctx.unit_catalog,
            redirects: BTreeSet::new(),
            slot_names: BTreeMap::new(),
        };

        let mut goal_statements = Vec::with_capacity(request.statements.len());
        for (idx, draft) in request.statements.iter().enumerate() {
            goal_statements.push(resolver.compile_statement(idx as u32, draft)?);
        }

        let mut context_statements = Vec::with_capacity(request.local_context.len());
        for (idx, draft) in request.local_context.iter().enumerate() {
            context_statements.push(resolver.compile_statement(idx as u32, draft)?);
        }

        let mut hard_constraints = Vec::with_capacity(request.hard_constraints.len());
        for draft in &request.hard_constraints {
            hard_constraints.push(resolver.compile_constraint(draft)?);
        }

        if resolver.slot_names.len() > MAX_OPEN_SLOTS {
            return Err(NeedCompileError::OpenSlotLimitExceeded {
                count: resolver.slot_names.len(),
                max: MAX_OPEN_SLOTS,
            });
        }

        let expected_types = request
            .expected_types
            .iter()
            .map(|token| resolver.resolve_concept_token(token))
            .collect::<Result<Vec<_>, _>>()?;

        let goal = canonicalize_frame_set(SemanticFrameSet {
            statements: goal_statements,
        })?;
        let local_context = canonicalize_frame_set(SemanticFrameSet {
            statements: context_statements,
        })?;

        let primary_role = match &request.target_role {
            Some(role_token) => resolver.resolve_concept_token(role_token)?,
            None => goal.statements[0].operator_or_predicate,
        };

        let mut desired_roles: Vec<ConceptCcid> = goal
            .statements
            .iter()
            .map(|stmt| stmt.operator_or_predicate)
            .collect();
        if !desired_roles.contains(&primary_role) {
            desired_roles.push(primary_role);
        }
        desired_roles.sort();
        desired_roles.dedup();

        // Collect all statement-level constraints + explicit hard constraints into a single
        // canonically ordered constraint list on ReceptorDefinition so commutative constraint
        // permutations produce byte-identical ReceptorDefinition CIDs.
        let mut all_receptor_constraints = hard_constraints;
        for stmt in &goal.statements {
            for c in &stmt.constraints {
                if !all_receptor_constraints.contains(c) {
                    all_receptor_constraints.push(c.clone());
                }
            }
        }
        if all_receptor_constraints.len() > MAX_CONSTRAINTS {
            return Err(NeedCompileError::ConstraintLimitExceeded {
                count: all_receptor_constraints.len(),
                max: MAX_CONSTRAINTS,
            });
        }
        all_receptor_constraints =
            canonicalize_receptor_constraints(primary_role, &all_receptor_constraints)?;

        let derivation_rule = compiler_rule_reference();
        let receptor = ReceptorDefinition {
            role: primary_role,
            expected_types,
            hard_constraints: all_receptor_constraints,
            cardinality: request.cardinality,
            origin: ReceptorOrigin::Derived {
                derivation_rule: derivation_rule.clone(),
                inputs: Vec::new(),
            },
            acceptance: ReceptorAcceptanceProfile {
                policy: ctx.acceptance_policy.clone(),
                required_evidence_kinds: Vec::new(),
                unknown_constraint_policy: request.unknown_constraint_policy,
            },
        };
        let (_, receptor_cid) = receptor
            .to_knowledge_object(DisclosureClass::LocalOnly)?
            .encode(ResourceProfile::ObjectV1)
            .map_err(QueryContractError::from)?;
        let receptor_reference =
            ObjectReference::new(RECEPTOR_DEFINITION_KIND.0, receptor_cid.into_bytes());

        let query_definition = QueryDefinition {
            need: KnowledgeNeedIr {
                receptor_definitions: vec![receptor_reference.clone()],
                desired_roles,
                goal: goal.clone(),
                local_context: local_context.clone(),
                privacy: DisclosureClass::LocalOnly,
            },
            query_policy: ctx.query_policy.clone(),
            exploration_policy: ctx.exploration_policy.clone(),
        };
        let query_cid = query_definition.private_cid()?;

        // StandingNeed.selector is bound strictly to the allow-listed public SelectorCid
        // (never derived from query bytes: NEG-KQL-001, NEG-KQL-005).
        let standing_need = StandingNeed::new_local(
            receptor_reference,
            query_cid,
            ctx.selector,
            ctx.watch_policy.clone(),
            ctx.observed_frontier,
        );

        let bundle = PrivateNeedBundle {
            query_definition,
            target: LocalNeedTarget {
                need: standing_need,
                receptor,
                required_semantics: goal,
                local_context,
                generator: ctx.generator.clone(),
                derivation_rule: Some(derivation_rule.clone()),
                evidence: Vec::new(),
                index_commitment: None,
                rule_commitment: Some(derivation_rule),
                metrics: ctx.metrics,
                unmapped_reason: ctx.unmapped_reason,
                source_frontier: ctx.source_frontier,
                created_at_evaluation: ctx.created_at_evaluation,
                expires_after_evaluations: ctx.expires_after_evaluations,
            },
        };
        bundle.validate()?;

        Ok(CompiledPrivateNeed {
            bundle,
            receptor_cid,
            query_cid,
            provenance: NeedCompilerProvenance {
                compiler_profile: COMPILER_PROFILE_ID,
                registry_release_root: ctx.registry_release_root,
                lifecycle_redirects_applied: resolver.redirects.into_iter().collect(),
            },
            limitations: Vec::new(),
        })
    }
}

fn validate_context(ctx: &NeedCompileContext<'_>) -> Result<(), NeedCompileError> {
    if ctx.registry_release_root == [0; 32] {
        return Err(NeedCompileError::InvalidRegistryReleaseRoot);
    }
    if ctx.selector.as_bytes() == &[0; 32] || !ctx.allowed_public_selectors.contains(&ctx.selector)
    {
        return Err(NeedCompileError::UnapprovedSelector(ctx.selector));
    }
    Ok(())
}

pub fn compiler_rule_reference() -> ObjectReference {
    let mut hasher = blake3::Hasher::new();
    hasher.update(b"onebrain:vnext:kql-need-compiler:rule:1\0");
    hasher.update(COMPILER_PROFILE_ID.as_bytes());
    ObjectReference::new(0, *hasher.finalize().as_bytes())
}

struct PinnedConceptResolver<'a> {
    registry: &'a dyn ConceptLookup,
    unit_catalog: &'a SiUnitCatalog,
    redirects: BTreeSet<LifecycleRedirectApplied>,
    slot_names: BTreeMap<String, VariableId>,
}

impl PinnedConceptResolver<'_> {
    fn intern_slot(&mut self, raw_name: &str) -> Result<VariableId, NeedCompileError> {
        let normalized = raw_name
            .trim()
            .strip_prefix('?')
            .or_else(|| raw_name.trim().strip_prefix("__slot_"))
            .unwrap_or(raw_name.trim())
            .to_string();
        if normalized.is_empty() {
            return Err(NeedCompileError::UnsupportedQueryForm(
                "empty open slot variable name".to_string(),
            ));
        }
        if let Some(&existing) = self.slot_names.get(&normalized) {
            return Ok(existing);
        }
        let next_id = VariableId(self.slot_names.len() as u32);
        self.slot_names.insert(normalized, next_id);
        if self.slot_names.len() > MAX_OPEN_SLOTS {
            return Err(NeedCompileError::OpenSlotLimitExceeded {
                count: self.slot_names.len(),
                max: MAX_OPEN_SLOTS,
            });
        }
        Ok(next_id)
    }

    fn resolve_concept_with_entry(
        &mut self,
        raw_token: &str,
    ) -> Result<(ConceptCcid, ResolvedConcept), NeedCompileError> {
        let token = raw_token.trim();
        if token.is_empty() {
            return Err(NeedCompileError::UnresolvedTerm {
                token: raw_token.to_string(),
            });
        }
        if is_forbidden_non_semantic_field(token) {
            return Err(NeedCompileError::NonSemanticFieldForbidden(
                NON_SEMANTIC_FIELD_ERROR_CODE,
            ));
        }

        let resolved = if let Some(parsed_ccid) = parse_ccid_hex_token(token) {
            let raw_bytes = *parsed_ccid.as_bytes();
            self.registry
                .describe_checked(&raw_bytes)
                .map_err(|err| NeedCompileError::RegistryUnavailable(err.to_string()))?
                .ok_or_else(|| NeedCompileError::UnresolvedTerm {
                    token: token.to_string(),
                })?
        } else {
            match self
                .registry
                .resolve_checked(token)
                .map_err(|err| NeedCompileError::RegistryUnavailable(err.to_string()))?
            {
                ResolveResult::Found(concept) => concept,
                ResolveResult::Ambiguous(matches) => {
                    let candidates = format_candidates(matches);
                    return Err(NeedCompileError::AmbiguousTerm {
                        token: token.to_string(),
                        candidates,
                    });
                }
                ResolveResult::Fuzzy(fuzzy) => {
                    return Err(NeedCompileError::AmbiguousTerm {
                        token: token.to_string(),
                        candidates: vec![(
                            ConceptCcid::from_bytes(fuzzy.ccid),
                            fuzzy.canonical_name,
                        )],
                    });
                }
                ResolveResult::NotFound => {
                    return Err(NeedCompileError::UnresolvedTerm {
                        token: token.to_string(),
                    });
                }
            }
        };

        let initial_ccid = ConceptCcid::from_bytes(resolved.ccid);
        match self.registry.resolve_for_authoring(initial_ccid) {
            AuthoringConceptResolution::Canonical(canonical) => Ok((canonical, resolved)),
            AuthoringConceptResolution::Redirected { from, to } => {
                self.redirects.insert(LifecycleRedirectApplied {
                    deprecated_ccid: from,
                    canonical_ccid: to,
                });
                let final_entry = self
                    .registry
                    .describe_checked(to.as_bytes())
                    .ok()
                    .flatten()
                    .unwrap_or(resolved);
                Ok((to, final_entry))
            }
            AuthoringConceptResolution::NeedsSplitDisambiguation {
                parent,
                branches,
                role,
            } => {
                let mut candidates = Vec::new();
                for branch in branches.into_iter().take(MAX_DISAMBIGUATION_CANDIDATES) {
                    let label = self
                        .registry
                        .describe_checked(branch.as_bytes())
                        .ok()
                        .flatten()
                        .map(|entry| entry.canonical_name)
                        .unwrap_or_else(|| branch.to_hex());
                    candidates.push((branch, label));
                }
                Err(NeedCompileError::SplitDisambiguationRequired {
                    token: token.to_string(),
                    parent,
                    disambiguation_role: role,
                    candidates,
                })
            }
            AuthoringConceptResolution::Retired { ccid, reason } => {
                Err(NeedCompileError::RetiredConcept {
                    token: token.to_string(),
                    ccid,
                    reason,
                })
            }
        }
    }

    fn resolve_concept_token(&mut self, token: &str) -> Result<ConceptCcid, NeedCompileError> {
        self.resolve_concept_with_entry(token).map(|(ccid, _)| ccid)
    }

    fn resolve_quantity(
        &mut self,
        draft: &NeedQuantityDraftV1,
    ) -> Result<QuantityLiteral, NeedCompileError> {
        match draft {
            NeedQuantityDraftV1::FloatForbidden(_) => Err(NeedCompileError::FloatLiteralForbidden),
            NeedQuantityDraftV1::Exact { number, unit } => {
                let value = parse_exact_decimal_or_rational(number)?;
                let (unit_ccid, entry) = self.resolve_concept_with_entry(unit)?;
                if entry.category != ConceptCategory::Unit {
                    return Err(NeedCompileError::InvalidUnit {
                        token: unit.clone(),
                    });
                }
                let si_spec = self
                    .unit_catalog
                    .lookup(unit_ccid, &entry.canonical_name, unit)
                    .ok_or_else(|| NeedCompileError::InvalidUnit {
                        token: unit.clone(),
                    })?;
                Ok(QuantityLiteral {
                    value,
                    source_unit: UnitRef {
                        unit: unit_ccid,
                        dimension: si_spec.dimension,
                        scale_to_base: si_spec.scale_to_base,
                        offset_to_base: si_spec.offset_to_base,
                    },
                })
            }
        }
    }

    fn compile_term(&mut self, draft: &NeedTermDraftV1) -> Result<TermRef, NeedCompileError> {
        match draft {
            NeedTermDraftV1::ConceptToken(token) => {
                if let Some(slot_name) = token.trim().strip_prefix('?') {
                    let id = self.intern_slot(slot_name)?;
                    Ok(TermRef::Variable {
                        id,
                        type_constraint: None,
                    })
                } else {
                    Ok(TermRef::Concept(self.resolve_concept_token(token)?))
                }
            }
            NeedTermDraftV1::OpenSlot {
                name,
                type_constraint,
            } => {
                let id = self.intern_slot(name)?;
                let constraint = type_constraint
                    .as_deref()
                    .map(|tok| self.resolve_concept_token(tok))
                    .transpose()?;
                Ok(TermRef::Variable {
                    id,
                    type_constraint: constraint,
                })
            }
            NeedTermDraftV1::Quantity(q) => Ok(TermRef::Literal(LiteralValue::Quantity(
                self.resolve_quantity(q)?,
            ))),
            NeedTermDraftV1::Text(text) => {
                let normalized =
                    NormalizedText::new(text.clone()).map_err(SemanticError::Canonical)?;
                Ok(TermRef::Literal(LiteralValue::Text(normalized)))
            }
            NeedTermDraftV1::Boolean(flag) => Ok(TermRef::Literal(LiteralValue::Boolean(*flag))),
        }
    }

    fn compile_constraint(
        &mut self,
        draft: &NeedConstraintDraftV1,
    ) -> Result<TypedConstraint, NeedCompileError> {
        match draft {
            NeedConstraintDraftV1::Compare {
                left,
                operator,
                right,
                required,
            } => {
                let left_term = self.compile_term(left)?;
                let right_term = self.compile_term(right)?;
                let (norm_left, norm_op, norm_right) =
                    canonicalize_comparison_orientation(left_term, *operator, right_term);
                Ok(TypedConstraint {
                    expression: ConstraintExpression::Compare {
                        left: norm_left,
                        operator: norm_op,
                        right: norm_right,
                    },
                    required: *required,
                })
            }
            NeedConstraintDraftV1::Range {
                term,
                lower,
                upper,
                include_lower,
                include_upper,
                required,
            } => Ok(TypedConstraint {
                expression: ConstraintExpression::Range {
                    term: self.compile_term(term)?,
                    lower: self.resolve_quantity(lower)?,
                    upper: self.resolve_quantity(upper)?,
                    include_lower: *include_lower,
                    include_upper: *include_upper,
                },
                required: *required,
            }),
            NeedConstraintDraftV1::Dimension {
                term,
                expected,
                required,
            } => Ok(TypedConstraint {
                expression: ConstraintExpression::Dimension {
                    term: self.compile_term(term)?,
                    expected: *expected,
                },
                required: *required,
            }),
        }
    }

    fn compile_statement(
        &mut self,
        statement_id: u32,
        draft: &NeedStatementDraftV1,
    ) -> Result<StatementFrame, NeedCompileError> {
        let predicate = self.resolve_concept_token(&draft.predicate)?;
        let arguments = draft
            .arguments
            .iter()
            .map(|arg| self.compile_term(arg))
            .collect::<Result<Vec<_>, _>>()?;
        let constraints = draft
            .constraints
            .iter()
            .map(|c| self.compile_constraint(c))
            .collect::<Result<Vec<_>, _>>()?;
        let qualifiers = StatementQualifiers {
            negated: draft.qualifiers.negated,
            modality: draft.qualifiers.modality,
            condition: draft.qualifiers.condition_statement_index.map(StatementId),
            time: draft
                .qualifiers
                .time
                .as_ref()
                .map(|t| self.compile_term(t))
                .transpose()?,
            location: draft
                .qualifiers
                .location
                .as_ref()
                .map(|t| self.compile_term(t))
                .transpose()?,
            perspective: draft
                .qualifiers
                .perspective
                .as_ref()
                .map(|t| self.compile_term(t))
                .transpose()?,
            tolerance: draft
                .qualifiers
                .tolerance
                .as_ref()
                .map(|q| self.resolve_quantity(q))
                .transpose()?,
            source_spans: Vec::new(),
        };
        Ok(StatementFrame {
            statement_id: StatementId(statement_id),
            operator_or_predicate: predicate,
            arguments,
            constraints,
            qualifiers,
        })
    }
}

fn format_candidates(matches: Vec<ResolvedConcept>) -> Vec<(ConceptCcid, String)> {
    let mut seen = BTreeSet::new();
    let mut out = Vec::new();
    for entry in matches {
        let ccid = ConceptCcid::from_bytes(entry.ccid);
        if seen.insert(ccid) {
            out.push((ccid, entry.canonical_name));
            if out.len() >= MAX_DISAMBIGUATION_CANDIDATES {
                break;
            }
        }
    }
    out.sort_by_key(|a| a.0);
    out
}

fn parse_ccid_hex_token(token: &str) -> Option<ConceptCcid> {
    let hex_str = token.strip_prefix("ccid:").unwrap_or(token);
    if hex_str.len() != 32 || !hex_str.chars().all(|c| c.is_ascii_hexdigit()) {
        return None;
    }
    let bytes = hex::decode(hex_str).ok()?;
    let arr: [u8; 16] = bytes.try_into().ok()?;
    Some(ConceptCcid::from_bytes(arr))
}

fn canonicalize_comparison_orientation(
    left: TermRef,
    operator: ComparisonOperator,
    right: TermRef,
) -> (TermRef, ComparisonOperator, TermRef) {
    // Place Variable/Concept on the left when compared against a Literal on the left
    // so `100 <= ?x` and `?x >= 100` normalize identically.
    if matches!(left, TermRef::Literal(_)) && !matches!(right, TermRef::Literal(_)) {
        let flipped = match operator {
            ComparisonOperator::Equal => ComparisonOperator::Equal,
            ComparisonOperator::NotEqual => ComparisonOperator::NotEqual,
            ComparisonOperator::LessThan => ComparisonOperator::GreaterThan,
            ComparisonOperator::LessThanOrEqual => ComparisonOperator::GreaterThanOrEqual,
            ComparisonOperator::GreaterThan => ComparisonOperator::LessThan,
            ComparisonOperator::GreaterThanOrEqual => ComparisonOperator::LessThanOrEqual,
        };
        (right, flipped, left)
    } else {
        (left, operator, right)
    }
}

/// Canonicalize a `SemanticFrameSet` so variable renaming (`?a` vs `?x`) and commutative
/// constraint permutations yield byte-identical canonical frames.
fn canonicalize_frame_set(frames: SemanticFrameSet) -> Result<SemanticFrameSet, NeedCompileError> {
    if frames.statements.is_empty() {
        return Ok(frames);
    }
    // Pass 1: sort each statement's constraints using variable-erased canonical keys so
    // constraint order never depends on input order or original variable identifiers.
    let mut prepared = frames;
    for stmt in &mut prepared.statements {
        sort_and_dedup_constraints(&mut stmt.constraints)?;
    }

    // If statements do not reference `StatementId` conditions/terms, sort conjunctive
    // statements by a variable-erased structural key so statement permutation also converges.
    let has_statement_cross_refs = prepared.statements.iter().any(|s| {
        s.qualifiers.condition.is_some()
            || s.arguments
                .iter()
                .any(|a| matches!(a, TermRef::Statement(_)))
    });
    if !has_statement_cross_refs && prepared.statements.len() > 1 {
        let mut keyed = prepared
            .statements
            .into_iter()
            .map(|stmt| {
                let key = erased_statement_sort_key(&stmt)?;
                Ok((key, stmt))
            })
            .collect::<Result<Vec<_>, NeedCompileError>>()?;
        keyed.sort_by(|a, b| a.0.cmp(&b.0));
        prepared.statements = keyed
            .into_iter()
            .enumerate()
            .map(|(idx, (_, mut stmt))| {
                stmt.statement_id = StatementId(idx as u32);
                stmt
            })
            .collect();
    }

    // Pass 2: alpha-normalize variable IDs across the whole frame set, then re-sort
    // constraints by exact canonical bytes and run alpha-normalization one final time.
    let mut normalized = prepared.alpha_normalized()?;
    for stmt in &mut normalized.statements {
        let mut encoded = stmt
            .constraints
            .iter()
            .cloned()
            .map(|c| {
                let bytes = encode_canonical(&c.canonical_value(), ResourceProfile::ObjectV1)
                    .map_err(SemanticError::Canonical)?;
                Ok((bytes, c))
            })
            .collect::<Result<Vec<_>, NeedCompileError>>()?;
        encoded.sort_by(|a, b| a.0.cmp(&b.0));
        encoded.dedup_by(|a, b| a.0 == b.0);
        stmt.constraints = encoded.into_iter().map(|(_, c)| c).collect();
    }
    Ok(normalized.alpha_normalized()?)
}

fn canonicalize_receptor_constraints(
    role: ConceptCcid,
    constraints: &[TypedConstraint],
) -> Result<Vec<TypedConstraint>, NeedCompileError> {
    let mut cloned = constraints.to_vec();
    sort_and_dedup_constraints(&mut cloned)?;
    let wrapper = SemanticFrameSet {
        statements: vec![StatementFrame {
            statement_id: StatementId(0),
            operator_or_predicate: role,
            arguments: Vec::new(),
            constraints: cloned,
            qualifiers: StatementQualifiers::default(),
        }],
    };
    let normalized = canonicalize_frame_set(wrapper)?;
    Ok(normalized
        .statements
        .into_iter()
        .next()
        .map(|s| s.constraints)
        .unwrap_or_default())
}

fn sort_and_dedup_constraints(
    constraints: &mut Vec<TypedConstraint>,
) -> Result<(), NeedCompileError> {
    let mut keyed = Vec::with_capacity(constraints.len());
    for constraint in constraints.drain(..) {
        let erased = erase_variables_in_constraint(&constraint);
        let key = encode_canonical(&erased.canonical_value(), ResourceProfile::ObjectV1)
            .map_err(SemanticError::Canonical)?;
        let exact = encode_canonical(&constraint.canonical_value(), ResourceProfile::ObjectV1)
            .map_err(SemanticError::Canonical)?;
        keyed.push((key, exact, constraint));
    }
    keyed.sort_by(|a, b| a.0.cmp(&b.0).then_with(|| a.1.cmp(&b.1)));
    keyed.dedup_by(|a, b| a.1 == b.1);
    *constraints = keyed.into_iter().map(|(_, _, c)| c).collect();
    Ok(())
}

fn erase_variables_in_term(term: &TermRef) -> TermRef {
    match term {
        TermRef::Variable {
            type_constraint, ..
        } => TermRef::Variable {
            id: VariableId(0),
            type_constraint: *type_constraint,
        },
        other => other.clone(),
    }
}

fn erase_variables_in_constraint(constraint: &TypedConstraint) -> TypedConstraint {
    let expression = match &constraint.expression {
        ConstraintExpression::Compare {
            left,
            operator,
            right,
        } => ConstraintExpression::Compare {
            left: erase_variables_in_term(left),
            operator: *operator,
            right: erase_variables_in_term(right),
        },
        ConstraintExpression::Dimension { term, expected } => ConstraintExpression::Dimension {
            term: erase_variables_in_term(term),
            expected: *expected,
        },
        ConstraintExpression::Range {
            term,
            lower,
            upper,
            include_lower,
            include_upper,
        } => ConstraintExpression::Range {
            term: erase_variables_in_term(term),
            lower: lower.clone(),
            upper: upper.clone(),
            include_lower: *include_lower,
            include_upper: *include_upper,
        },
    };
    TypedConstraint {
        expression,
        required: constraint.required,
    }
}

fn erased_statement_sort_key(stmt: &StatementFrame) -> Result<Vec<u8>, NeedCompileError> {
    let erased = StatementFrame {
        statement_id: StatementId(0),
        operator_or_predicate: stmt.operator_or_predicate,
        arguments: stmt.arguments.iter().map(erase_variables_in_term).collect(),
        constraints: stmt
            .constraints
            .iter()
            .map(erase_variables_in_constraint)
            .collect(),
        qualifiers: StatementQualifiers {
            negated: stmt.qualifiers.negated,
            modality: stmt.qualifiers.modality,
            condition: None,
            time: stmt.qualifiers.time.as_ref().map(erase_variables_in_term),
            location: stmt
                .qualifiers
                .location
                .as_ref()
                .map(erase_variables_in_term),
            perspective: stmt
                .qualifiers
                .perspective
                .as_ref()
                .map(erase_variables_in_term),
            tolerance: stmt.qualifiers.tolerance.clone(),
            source_spans: Vec::new(),
        },
    };
    SemanticFrameSet {
        statements: vec![erased],
    }
    .canonical_bytes()
    .map_err(NeedCompileError::Semantic)
}

// ═══════════════════════════════════════════════════════════════════════════
// Bounded Textual `FIND` Lowering (`ku_kql::parser::parse_query`)
// ═══════════════════════════════════════════════════════════════════════════

fn is_forbidden_non_semantic_field(name: &str) -> bool {
    let lower = name.trim().to_ascii_lowercase();
    FORBIDDEN_NON_SEMANTIC_FIELDS.contains(&lower.as_str())
}

fn lower_textual_find_to_structured(
    raw_kql: &str,
) -> Result<StructuredNeedCompileRequestV1, NeedCompileError> {
    let trimmed = raw_kql.trim();
    if trimmed.is_empty() {
        return Err(NeedCompileError::EmptyNeed);
    }

    let mut head_words = trimmed.split_whitespace();
    let mut first_kw = head_words.next().unwrap_or("").to_ascii_uppercase();
    if first_kw == "EXPLAIN" {
        first_kw = head_words.next().unwrap_or("").to_ascii_uppercase();
    }
    if matches!(
        first_kw.as_str(),
        "CREATE" | "UPDATE" | "DEPRECATE" | "WATCH"
    ) {
        return Err(NeedCompileError::UnsupportedQueryForm(first_kw));
    }

    let normalized_text = Zeroizing::new(preprocess_binder_pattern_syntax(trimmed)?);
    let parsed =
        parse_query(&normalized_text).map_err(|err| NeedCompileError::Parse(err.message))?;
    let find = match parsed {
        Query::Find(find) => find,
        Query::Explain(inner) => match *inner {
            Query::Find(find) => find,
            other => {
                return Err(NeedCompileError::UnsupportedQueryForm(format!("{other:?}")));
            }
        },
        other => {
            return Err(NeedCompileError::UnsupportedQueryForm(format!("{other:?}")));
        }
    };

    validate_find_ownership_boundaries(&find)?;
    extract_structured_from_find(&find)
}

/// Quote-aware preprocessor allowing P3 §2.1 `(?x)-[:pred]->(?y)` open-binder pattern
/// syntax to pass cleanly through the frozen `ku_kql::parser::parse_query` grammar.
fn preprocess_binder_pattern_syntax(input: &str) -> Result<String, NeedCompileError> {
    let chars: Vec<char> = input.chars().collect();
    let mut out = String::with_capacity(input.len() + 32);
    let mut i = 0usize;
    let mut in_quotes = false;

    while i < chars.len() {
        let ch = chars[i];
        if ch == '"' {
            in_quotes = !in_quotes;
            out.push(ch);
            i += 1;
            continue;
        }
        if in_quotes {
            out.push(ch);
            i += 1;
            continue;
        }

        // Convert `(?ident)` or `(?ident:KU)` / `(?ident:Concept)` outside quotes.
        if ch == '(' {
            let mut j = i + 1;
            while j < chars.len() && chars[j].is_whitespace() {
                j += 1;
            }
            if j < chars.len() && chars[j] == '?' {
                let start_ident = j + 1;
                let mut k = start_ident;
                while k < chars.len() && (chars[k].is_alphanumeric() || chars[k] == '_') {
                    k += 1;
                }
                if k > start_ident {
                    let ident: String = chars[start_ident..k].iter().collect();
                    let mut m = k;
                    while m < chars.len() && chars[m].is_whitespace() {
                        m += 1;
                    }
                    if m < chars.len() && chars[m] == ')' {
                        out.push_str("(__slot_");
                        out.push_str(&ident);
                        out.push_str(":KU)");
                        i = m + 1;
                        continue;
                    } else if m < chars.len() && chars[m] == ':' {
                        out.push_str("(__slot_");
                        out.push_str(&ident);
                        i = m;
                        continue;
                    }
                }
            }
        }

        // Convert `?ident` in WHERE field position (outside quotes) to `__slot_ident`.
        if ch == '?'
            && i + 1 < chars.len()
            && (chars[i + 1].is_alphanumeric() || chars[i + 1] == '_')
        {
            out.push_str("__slot_");
            i += 1;
            continue;
        }

        out.push(ch);
        i += 1;
    }

    Ok(out)
}

fn validate_find_ownership_boundaries(find: &FindQuery) -> Result<(), NeedCompileError> {
    match find.scope {
        Scope::Local | Scope::Auto | Scope::Semantic => {}
        Scope::Global | Scope::Neighbors | Scope::Cluster | Scope::Dht => {
            return Err(NeedCompileError::NonSemanticFieldForbidden(
                NON_SEMANTIC_FIELD_ERROR_CODE,
            ));
        }
    }

    if find.history || find.temporal.is_some() {
        return Err(NeedCompileError::NonSemanticFieldForbidden(
            NON_SEMANTIC_FIELD_ERROR_CODE,
        ));
    }

    for node in &find.pattern.nodes {
        for prop in &node.properties {
            check_property_ownership(prop)?;
        }
    }

    if let Some(order_list) = &find.order_by {
        for order in order_list {
            check_field_path_ownership(&order.field)?;
        }
    }

    if let Some(returns) = &find.return_clause {
        for ret in returns {
            match ret {
                ReturnExpr::Field(fp) | ReturnExpr::Aggregate { field: fp, .. } => {
                    check_field_path_ownership(fp)?;
                }
                ReturnExpr::Alias(_) => {}
            }
        }
    }

    if let Some(where_cond) = &find.where_clause {
        check_condition_ownership_and_floats(where_cond)?;
    }

    Ok(())
}

fn check_property_ownership(prop: &Property) -> Result<(), NeedCompileError> {
    if is_forbidden_non_semantic_field(&prop.key) {
        return Err(NeedCompileError::NonSemanticFieldForbidden(
            NON_SEMANTIC_FIELD_ERROR_CODE,
        ));
    }
    if matches!(prop.value, Value::Float(_)) {
        return Err(NeedCompileError::FloatLiteralForbidden);
    }
    Ok(())
}

fn leaf_field_name(fp: &FieldPath) -> &str {
    fp.segments.last().map(String::as_str).unwrap_or("")
}

fn check_field_path_ownership(fp: &FieldPath) -> Result<(), NeedCompileError> {
    for segment in &fp.segments {
        if is_forbidden_non_semantic_field(segment) {
            return Err(NeedCompileError::NonSemanticFieldForbidden(
                NON_SEMANTIC_FIELD_ERROR_CODE,
            ));
        }
    }
    Ok(())
}

fn check_condition_ownership_and_floats(cond: &Condition) -> Result<(), NeedCompileError> {
    match cond {
        Condition::And(left, right) | Condition::Or(left, right) => {
            check_condition_ownership_and_floats(left)?;
            check_condition_ownership_and_floats(right)
        }
        Condition::Not(inner) => check_condition_ownership_and_floats(inner),
        Condition::Exists(fp) => check_field_path_ownership(fp),
        Condition::Contains { field, value } | Condition::Comparison { field, value, .. } => {
            // Always reject non-semantic fields BEFORE checking literal types so
            // `WHERE pomv > 0.5` deterministically yields `NonSemanticFieldForbidden`.
            check_field_path_ownership(field)?;
            if matches!(value, Value::Float(_)) {
                return Err(NeedCompileError::FloatLiteralForbidden);
            }
            Ok(())
        }
    }
}

#[derive(Default)]
struct ExtractedWhereClauses {
    predicate: Option<String>,
    role: Option<String>,
    subject: Option<NeedTermDraftV1>,
    object: Option<NeedTermDraftV1>,
    node_bindings: BTreeMap<String, NeedTermDraftV1>,
    node_types: BTreeMap<String, String>,
    expected_types: Vec<String>,
    unit_override: Option<String>,
    raw_numeric_comparisons: Vec<(Option<String>, ComparisonOperator, String, bool)>,
    constraints: Vec<NeedConstraintDraftV1>,
    negated: bool,
    time: Option<NeedTermDraftV1>,
    location: Option<NeedTermDraftV1>,
    perspective: Option<NeedTermDraftV1>,
    condition_concept: Option<String>,
    tolerance: Option<NeedQuantityDraftV1>,
    unknown_policy: Option<UnknownConstraintPolicy>,
}

fn extract_structured_from_find(
    find: &FindQuery,
) -> Result<StructuredNeedCompileRequestV1, NeedCompileError> {
    let mut extracted = ExtractedWhereClauses::default();

    for node in &find.pattern.nodes {
        match node.label {
            NodeLabel::KU | NodeLabel::Concept => {}
        }
        if let Some(alias) = &node.alias {
            for prop in &node.properties {
                apply_node_property(alias, prop, &mut extracted)?;
            }
        }
    }

    if let Some(cond) = &find.where_clause {
        collect_conjunctive_clauses(cond, false, &mut extracted)?;
    }

    // Resolve any integer/decimal numeric comparisons paired with a separate `unit = "..."`
    // clause (e.g. `WHERE quantity >= 100 AND unit = "degC"`).
    let raw_numeric_comparisons = std::mem::take(&mut extracted.raw_numeric_comparisons);
    for (slot_alias, op, number_str, required) in raw_numeric_comparisons {
        let unit =
            extracted
                .unit_override
                .clone()
                .ok_or_else(|| NeedCompileError::InvalidUnit {
                    token: "<missing unit for numeric quantity constraint>".to_string(),
                })?;
        let slot_name = slot_alias.unwrap_or_else(|| "__qty_slot".to_string());
        extracted.constraints.push(NeedConstraintDraftV1::Compare {
            left: NeedTermDraftV1::OpenSlot {
                name: slot_name,
                type_constraint: None,
            },
            operator: op,
            right: NeedTermDraftV1::Quantity(NeedQuantityDraftV1::Exact {
                number: number_str,
                unit,
            }),
            required,
        });
    }

    let mut statements = Vec::new();
    let mut local_context = Vec::new();

    // Optional `WHERE condition = "<pred_or_concept>"` becomes a context/condition frame.
    let condition_idx = if let Some(cond_tok) = &extracted.condition_concept {
        local_context.push(NeedStatementDraftV1 {
            predicate: cond_tok.clone(),
            arguments: Vec::new(),
            constraints: Vec::new(),
            qualifiers: NeedQualifiersDraftV1::default(),
        });
        None
    } else {
        None
    };

    if !find.pattern.edges.is_empty() {
        if find.pattern.edges.len() > MAX_COMPILED_STATEMENTS {
            return Err(NeedCompileError::StatementLimitExceeded {
                count: find.pattern.edges.len(),
                max: MAX_COMPILED_STATEMENTS,
            });
        }
        for (edge_idx, edge) in find.pattern.edges.iter().enumerate() {
            if edge.path_depth.is_some() || edge.edge_types.len() != 1 {
                return Err(NeedCompileError::UnsupportedQueryForm(
                    "NeedIR pattern edges require exactly one semantic predicate without *min..max"
                        .to_string(),
                ));
            }
            let (subj_idx, obj_idx) = match edge.direction {
                EdgeDirection::Outgoing => (edge.from, edge.to),
                EdgeDirection::Incoming => (edge.to, edge.from),
                EdgeDirection::Undirected => {
                    return Err(NeedCompileError::UnsupportedQueryForm(
                        "undirected edge not supported in NeedIR".to_string(),
                    ));
                }
            };
            let subj_term = node_index_to_term(find, subj_idx, &extracted, "subj")?;
            let obj_term = node_index_to_term(find, obj_idx, &extracted, "obj")?;
            let qualifiers = if edge_idx == 0 {
                NeedQualifiersDraftV1 {
                    negated: extracted.negated,
                    modality: Modality::Asserted,
                    condition_statement_index: condition_idx,
                    time: extracted.time.clone(),
                    location: extracted.location.clone(),
                    perspective: extracted.perspective.clone(),
                    tolerance: extracted.tolerance.clone(),
                }
            } else {
                NeedQualifiersDraftV1::default()
            };
            let stmt_constraints = if edge_idx == 0 {
                std::mem::take(&mut extracted.constraints)
            } else {
                Vec::new()
            };
            statements.push(NeedStatementDraftV1 {
                predicate: edge.edge_types[0].clone(),
                arguments: vec![subj_term, obj_term],
                constraints: stmt_constraints,
                qualifiers,
            });
        }
    } else {
        let predicate = extracted
            .predicate
            .clone()
            .or_else(|| extracted.role.clone())
            .ok_or_else(|| {
                NeedCompileError::UnsupportedQueryForm(
                    "FIND query must specify semantic `predicate` / `role` or a typed edge pattern"
                        .to_string(),
                )
            })?;
        let mut arguments = Vec::new();
        if let Some(subj) = extracted.subject {
            arguments.push(subj);
        } else if let Some(first_alias) = find.pattern.nodes.first().and_then(|n| n.alias.as_ref())
        {
            if let Some(bound) = extracted.node_bindings.get(first_alias) {
                arguments.push(bound.clone());
            } else if first_alias.starts_with("__slot_") {
                arguments.push(NeedTermDraftV1::OpenSlot {
                    name: first_alias.clone(),
                    type_constraint: extracted.node_types.get(first_alias).cloned(),
                });
            }
        }
        if let Some(obj) = extracted.object {
            arguments.push(obj);
        }

        // If a quantity constraint references `__qty_slot` and no open slot argument was
        // declared yet, bind `__qty_slot` as an open argument slot so `ExactTypedMatcher`
        // binds candidate KU quantity arguments directly into the constraint!
        let uses_default_qty_slot = extracted.constraints.iter().any(|c| match c {
            NeedConstraintDraftV1::Compare {
                left: NeedTermDraftV1::OpenSlot { name, .. },
                ..
            }
            | NeedConstraintDraftV1::Range {
                term: NeedTermDraftV1::OpenSlot { name, .. },
                ..
            }
            | NeedConstraintDraftV1::Dimension {
                term: NeedTermDraftV1::OpenSlot { name, .. },
                ..
            } => name == "__qty_slot",
            _ => false,
        });
        if uses_default_qty_slot {
            let has_open_arg = arguments
                .iter()
                .any(|arg| matches!(arg, NeedTermDraftV1::OpenSlot { .. }));
            if !has_open_arg {
                arguments.push(NeedTermDraftV1::OpenSlot {
                    name: "__qty_slot".to_string(),
                    type_constraint: None,
                });
            } else if let Some(NeedTermDraftV1::OpenSlot { name, .. }) = arguments
                .iter()
                .rev()
                .find(|arg| matches!(arg, NeedTermDraftV1::OpenSlot { .. }))
            {
                let target_slot = name.clone();
                rewire_default_qty_slot(&mut extracted.constraints, &target_slot);
            }
        }

        statements.push(NeedStatementDraftV1 {
            predicate,
            arguments,
            constraints: std::mem::take(&mut extracted.constraints),
            qualifiers: NeedQualifiersDraftV1 {
                negated: extracted.negated,
                modality: Modality::Asserted,
                condition_statement_index: condition_idx,
                time: extracted.time,
                location: extracted.location,
                perspective: extracted.perspective,
                tolerance: extracted.tolerance,
            },
        });
    }

    // Rewire `__qty_slot` onto the last open slot of statement 0 in edge-pattern queries too.
    if let Some(first_stmt) = statements.first_mut() {
        if let Some(NeedTermDraftV1::OpenSlot { name, .. }) = first_stmt
            .arguments
            .iter()
            .rev()
            .find(|arg| matches!(arg, NeedTermDraftV1::OpenSlot { .. }))
        {
            let slot_name = name.clone();
            rewire_default_qty_slot(&mut first_stmt.constraints, &slot_name);
        }
    }

    Ok(StructuredNeedCompileRequestV1 {
        target_role: extracted.role,
        expected_types: extracted.expected_types,
        statements,
        local_context,
        hard_constraints: extracted.constraints,
        unknown_constraint_policy: extracted
            .unknown_policy
            .unwrap_or(UnknownConstraintPolicy::KeepUnresolved),
        cardinality: ReceptorCardinality {
            minimum: 1,
            maximum: find.limit,
        },
    })
}

fn rewire_default_qty_slot(constraints: &mut [NeedConstraintDraftV1], target_slot: &str) {
    for constraint in constraints {
        match constraint {
            NeedConstraintDraftV1::Compare {
                left: NeedTermDraftV1::OpenSlot { name, .. },
                ..
            }
            | NeedConstraintDraftV1::Range {
                term: NeedTermDraftV1::OpenSlot { name, .. },
                ..
            }
            | NeedConstraintDraftV1::Dimension {
                term: NeedTermDraftV1::OpenSlot { name, .. },
                ..
            } if name == "__qty_slot" => {
                *name = target_slot.to_string();
            }
            _ => {}
        }
    }
}

fn node_index_to_term(
    find: &FindQuery,
    idx: usize,
    extracted: &ExtractedWhereClauses,
    fallback_prefix: &str,
) -> Result<NeedTermDraftV1, NeedCompileError> {
    let node = find.pattern.nodes.get(idx).ok_or_else(|| {
        NeedCompileError::UnsupportedQueryForm("invalid node index in pattern".to_string())
    })?;
    if let Some(alias) = &node.alias {
        if let Some(bound) = extracted.node_bindings.get(alias) {
            return Ok(bound.clone());
        }
        return Ok(NeedTermDraftV1::OpenSlot {
            name: alias.clone(),
            type_constraint: extracted.node_types.get(alias).cloned(),
        });
    }
    Ok(NeedTermDraftV1::OpenSlot {
        name: format!("{fallback_prefix}_{idx}"),
        type_constraint: None,
    })
}

fn apply_node_property(
    alias: &str,
    prop: &Property,
    extracted: &mut ExtractedWhereClauses,
) -> Result<(), NeedCompileError> {
    let key = prop.key.to_ascii_lowercase();
    let text = value_as_string(&prop.value)?;
    match key.as_str() {
        "concept" | "ccid" | "label" => {
            extracted
                .node_bindings
                .insert(alias.to_string(), parse_term_string(&text));
        }
        "type" | "expected_type" => {
            extracted.node_types.insert(alias.to_string(), text.clone());
            extracted.expected_types.push(text);
        }
        "predicate" => extracted.predicate = Some(text),
        "role" => extracted.role = Some(text),
        other => {
            return Err(NeedCompileError::UnsupportedQueryForm(format!(
                "unsupported node property '{other}'"
            )));
        }
    }
    Ok(())
}

fn collect_conjunctive_clauses(
    cond: &Condition,
    negated_scope: bool,
    extracted: &mut ExtractedWhereClauses,
) -> Result<(), NeedCompileError> {
    match cond {
        Condition::And(left, right) => {
            if negated_scope {
                return Err(NeedCompileError::UnsupportedQueryForm(
                    "negated conjunction (De Morgan OR) is outside conjunctive NeedIR".to_string(),
                ));
            }
            collect_conjunctive_clauses(left, false, extracted)?;
            collect_conjunctive_clauses(right, false, extracted)
        }
        Condition::Or(_, _) => Err(NeedCompileError::UnsupportedQueryForm(
            "disjunctive OR is outside single-receptor conjunctive NeedIR".to_string(),
        )),
        Condition::Not(inner) => collect_conjunctive_clauses(inner, !negated_scope, extracted),
        Condition::Exists(fp) => Err(NeedCompileError::UnsupportedQueryForm(format!(
            "EXISTS {} is not a typed semantic constraint",
            leaf_field_name(fp)
        ))),
        Condition::Contains { field, value } => {
            let name = leaf_field_name(field).to_ascii_lowercase();
            if name == "concept_ccids" {
                let tok = value_as_string(value)?;
                if extracted.subject.is_none() {
                    extracted.subject = Some(parse_term_string(&tok));
                } else if extracted.object.is_none() {
                    extracted.object = Some(parse_term_string(&tok));
                } else {
                    extracted.expected_types.push(tok);
                }
                if negated_scope {
                    extracted.negated = true;
                }
                Ok(())
            } else if name == "concept_ids" {
                Err(NeedCompileError::NonSemanticFieldForbidden(
                    NON_SEMANTIC_FIELD_ERROR_CODE,
                ))
            } else {
                Err(NeedCompileError::UnsupportedQueryForm(format!(
                    "unsupported CONTAINS field '{name}'"
                )))
            }
        }
        Condition::Comparison { field, op, value } => {
            apply_comparison_clause(field, op, value, negated_scope, extracted)
        }
    }
}

fn apply_comparison_clause(
    field: &FieldPath,
    op: &CompOp,
    value: &Value,
    negated_scope: bool,
    extracted: &mut ExtractedWhereClauses,
) -> Result<(), NeedCompileError> {
    let (qualifier_alias, field_name) = match field.segments.as_slice() {
        [single] => (None, single.to_ascii_lowercase()),
        [alias, leaf] => (Some(alias.clone()), leaf.to_ascii_lowercase()),
        _ => {
            return Err(NeedCompileError::UnsupportedQueryForm(format!(
                "unsupported field path '{}'",
                field.segments.join(".")
            )));
        }
    };

    let effective_op = map_comp_op(op, negated_scope)?;

    match field_name.as_str() {
        "predicate" => {
            if effective_op != ComparisonOperator::Equal {
                return Err(NeedCompileError::UnsupportedQueryForm(
                    "predicate requires '='".to_string(),
                ));
            }
            extracted.predicate = Some(value_as_string(value)?);
        }
        "role" => {
            if effective_op != ComparisonOperator::Equal {
                return Err(NeedCompileError::UnsupportedQueryForm(
                    "role requires '='".to_string(),
                ));
            }
            extracted.role = Some(value_as_string(value)?);
        }
        "subject" => {
            let term = parse_term_string(&value_as_string(value)?);
            if effective_op == ComparisonOperator::Equal {
                extracted.subject = Some(term);
            } else {
                let slot = NeedTermDraftV1::OpenSlot {
                    name: "__subj_slot".to_string(),
                    type_constraint: None,
                };
                extracted.subject = Some(slot.clone());
                extracted.constraints.push(NeedConstraintDraftV1::Compare {
                    left: slot,
                    operator: effective_op,
                    right: term,
                    required: true,
                });
            }
        }
        "object" => {
            let term = parse_term_string(&value_as_string(value)?);
            if effective_op == ComparisonOperator::Equal {
                extracted.object = Some(term);
            } else {
                let slot = NeedTermDraftV1::OpenSlot {
                    name: "__obj_slot".to_string(),
                    type_constraint: None,
                };
                extracted.object = Some(slot.clone());
                extracted.constraints.push(NeedConstraintDraftV1::Compare {
                    left: slot,
                    operator: effective_op,
                    right: term,
                    required: true,
                });
            }
        }
        "concept" | "primary_concept" => {
            let term = parse_term_string(&value_as_string(value)?);
            if effective_op != ComparisonOperator::Equal {
                return Err(NeedCompileError::UnsupportedQueryForm(
                    "concept binding requires '=' (or NOT ... for negated claim)".to_string(),
                ));
            }
            if let Some(alias) = qualifier_alias {
                extracted.node_bindings.insert(alias, term);
            } else if extracted.subject.is_none() {
                extracted.subject = Some(term);
            } else {
                extracted.object = Some(term);
            }
        }
        "expected_type" | "type" => {
            let tok = value_as_string(value)?;
            if let Some(alias) = qualifier_alias {
                extracted.node_types.insert(alias, tok.clone());
            }
            extracted.expected_types.push(tok);
        }
        "unit" => {
            if effective_op != ComparisonOperator::Equal {
                return Err(NeedCompileError::UnsupportedQueryForm(
                    "unit requires '='".to_string(),
                ));
            }
            extracted.unit_override = Some(value_as_string(value)?);
        }
        "quantity" | "value" => {
            let slot_name = qualifier_alias
                .clone()
                .unwrap_or_else(|| "__qty_slot".to_string());
            match value {
                Value::Text(s) => {
                    if let Some((num_part, unit_part)) = split_quantity_and_unit(s) {
                        extracted.constraints.push(NeedConstraintDraftV1::Compare {
                            left: NeedTermDraftV1::OpenSlot {
                                name: slot_name,
                                type_constraint: None,
                            },
                            operator: effective_op,
                            right: NeedTermDraftV1::Quantity(NeedQuantityDraftV1::Exact {
                                number: num_part,
                                unit: unit_part,
                            }),
                            required: true,
                        });
                    } else {
                        extracted.raw_numeric_comparisons.push((
                            qualifier_alias,
                            effective_op,
                            s.clone(),
                            true,
                        ));
                    }
                }
                Value::Integer(int_val) => {
                    extracted.raw_numeric_comparisons.push((
                        qualifier_alias,
                        effective_op,
                        int_val.to_string(),
                        true,
                    ));
                }
                Value::Float(_) => return Err(NeedCompileError::FloatLiteralForbidden),
                _ => {
                    return Err(NeedCompileError::InvalidQuantity {
                        raw: format!("{value:?}"),
                    });
                }
            }
        }
        "time" => {
            let tok = value_as_string(value)?;
            let term = if let Some((num, unit)) = split_quantity_and_unit(&tok) {
                NeedTermDraftV1::Quantity(NeedQuantityDraftV1::Exact { number: num, unit })
            } else {
                parse_term_string(&tok)
            };
            extracted.time = Some(term);
        }
        "location" => {
            extracted.location = Some(parse_term_string(&value_as_string(value)?));
        }
        "perspective" => {
            extracted.perspective = Some(parse_term_string(&value_as_string(value)?));
        }
        "condition" => {
            extracted.condition_concept = Some(value_as_string(value)?);
            if negated_scope {
                extracted.negated = true;
            }
        }
        "tolerance" => {
            let tok = value_as_string(value)?;
            let (num, unit) = split_quantity_and_unit(&tok)
                .ok_or_else(|| NeedCompileError::InvalidQuantity { raw: tok.clone() })?;
            extracted.tolerance = Some(NeedQuantityDraftV1::Exact { number: num, unit });
        }
        "negated" => match value {
            Value::Bool(flag) => {
                extracted.negated = if negated_scope { !*flag } else { *flag };
            }
            _ => {
                return Err(NeedCompileError::UnsupportedQueryForm(
                    "negated must be boolean".to_string(),
                ));
            }
        },
        "unknown_policy" => {
            let tok = value_as_string(value)?.to_ascii_lowercase();
            extracted.unknown_policy = Some(match tok.as_str() {
                "reject" | "reject_unknown" | "reject_binding" => {
                    UnknownConstraintPolicy::RejectBinding
                }
                "keep" | "keep_as_unknown" | "keep_unresolved" => {
                    UnknownConstraintPolicy::KeepUnresolved
                }
                _ => {
                    return Err(NeedCompileError::UnsupportedQueryForm(format!(
                        "unsupported unknown_policy '{tok}'"
                    )));
                }
            });
        }
        other if qualifier_alias.is_none() && other.starts_with("__slot_") => {
            let slot_term = NeedTermDraftV1::OpenSlot {
                name: other.to_string(),
                type_constraint: None,
            };
            let right_term = match value {
                Value::Text(s) => {
                    if let Some((num, unit)) = split_quantity_and_unit(s) {
                        NeedTermDraftV1::Quantity(NeedQuantityDraftV1::Exact { number: num, unit })
                    } else {
                        parse_term_string(s)
                    }
                }
                Value::Bool(b) => NeedTermDraftV1::Boolean(*b),
                Value::Integer(i) => {
                    extracted.raw_numeric_comparisons.push((
                        Some(other.to_string()),
                        effective_op,
                        i.to_string(),
                        true,
                    ));
                    return Ok(());
                }
                Value::Float(_) => return Err(NeedCompileError::FloatLiteralForbidden),
                _ => {
                    return Err(NeedCompileError::UnsupportedQueryForm(format!(
                        "unsupported literal value for slot '{other}'"
                    )));
                }
            };
            extracted.constraints.push(NeedConstraintDraftV1::Compare {
                left: slot_term,
                operator: effective_op,
                right: right_term,
                required: true,
            });
        }
        other => {
            return Err(NeedCompileError::UnsupportedQueryForm(format!(
                "unknown or non-compilable field '{other}'"
            )));
        }
    }
    Ok(())
}

fn map_comp_op(op: &CompOp, negated: bool) -> Result<ComparisonOperator, NeedCompileError> {
    let mapped = match (op, negated) {
        (CompOp::Eq, false) | (CompOp::NotEq, true) => ComparisonOperator::Equal,
        (CompOp::NotEq, false) | (CompOp::Eq, true) => ComparisonOperator::NotEqual,
        (CompOp::Lt, false) | (CompOp::GtEq, true) => ComparisonOperator::LessThan,
        (CompOp::LtEq, false) | (CompOp::Gt, true) => ComparisonOperator::LessThanOrEqual,
        (CompOp::Gt, false) | (CompOp::LtEq, true) => ComparisonOperator::GreaterThan,
        (CompOp::GtEq, false) | (CompOp::Lt, true) => ComparisonOperator::GreaterThanOrEqual,
    };
    Ok(mapped)
}

fn value_as_string(value: &Value) -> Result<String, NeedCompileError> {
    match value {
        Value::Text(s) => Ok(s.clone()),
        Value::Float(_) => Err(NeedCompileError::FloatLiteralForbidden),
        other => Err(NeedCompileError::UnsupportedQueryForm(format!(
            "expected string token, found {other:?}"
        ))),
    }
}

fn parse_term_string(raw: &str) -> NeedTermDraftV1 {
    let trimmed = raw.trim();
    if let Some(slot) = trimmed.strip_prefix('?') {
        NeedTermDraftV1::OpenSlot {
            name: slot.to_string(),
            type_constraint: None,
        }
    } else if trimmed.starts_with("__slot_") {
        NeedTermDraftV1::OpenSlot {
            name: trimmed.to_string(),
            type_constraint: None,
        }
    } else {
        NeedTermDraftV1::ConceptToken(trimmed.to_string())
    }
}

fn split_quantity_and_unit(raw: &str) -> Option<(String, String)> {
    let trimmed = raw.trim();
    let (num, unit) = trimmed.split_once(char::is_whitespace)?;
    let num = num.trim();
    let unit = unit.trim();
    if num.is_empty() || unit.is_empty() {
        return None;
    }
    let first = num.chars().next()?;
    if !(first.is_ascii_digit() || first == '-' || first == '+') {
        return None;
    }
    Some((num.to_string(), unit.to_string()))
}

#[cfg(test)]
mod tests {
    use ku_core::concept_lifecycle::{
        ConceptLifecycleRecord, ConceptLifecycleTable, ConceptRedirect, ConceptRetire, ConceptSplit,
    };
    use ku_core::concept_registry::ConceptRegistry;
    use ku_core::foundation::public_knowledge_exchange_fixture_v1;

    use super::*;

    fn ccid(byte: u8) -> [u8; 16] {
        [byte; 16]
    }

    fn reference(byte: u8) -> ObjectReference {
        ObjectReference::new(0, [byte; 32])
    }

    fn test_registry() -> ConceptRegistry {
        let mut reg = ConceptRegistry::new();
        reg.add(
            ResolvedConcept {
                ccid: ccid(1),
                qid: 283,
                category: ConceptCategory::Substance,
                canonical_name: "water".to_string(),
            },
            &["water", "h2o"],
        );
        reg.add(
            ResolvedConcept {
                ccid: ccid(2),
                qid: 200,
                category: ConceptCategory::Property,
                canonical_name: "boiling_point".to_string(),
            },
            &["boiling_point"],
        );
        reg.add(
            ResolvedConcept {
                ccid: ccid(3),
                qid: 25267,
                category: ConceptCategory::Unit,
                canonical_name: "degC".to_string(),
            },
            &["degC", "celsius"],
        );
        reg.add(
            ResolvedConcept {
                ccid: ccid(4),
                qid: 11579,
                category: ConceptCategory::Unit,
                canonical_name: "K".to_string(),
            },
            &["K", "kelvin"],
        );
        reg.add(
            ResolvedConcept {
                ccid: ccid(5),
                qid: 828224,
                category: ConceptCategory::Unit,
                canonical_name: "km".to_string(),
            },
            &["km", "kilometer"],
        );
        reg.add(
            ResolvedConcept {
                ccid: ccid(6),
                qid: 11573,
                category: ConceptCategory::Unit,
                canonical_name: "m".to_string(),
            },
            &["m", "meter"],
        );
        // Ambiguous label "bank"
        reg.add(
            ResolvedConcept {
                ccid: ccid(10),
                qid: 22687,
                category: ConceptCategory::Entity,
                canonical_name: "bank (finance)".to_string(),
            },
            &["bank"],
        );
        reg.add(
            ResolvedConcept {
                ccid: ccid(11),
                qid: 202975,
                category: ConceptCategory::Place,
                canonical_name: "bank (river)".to_string(),
            },
            &["bank"],
        );
        // Lifecycle entries: redirect 20 -> 21, split 30 -> {31, 32}, retire 40
        reg.add(
            ResolvedConcept {
                ccid: ccid(20),
                qid: 2000,
                category: ConceptCategory::Entity,
                canonical_name: "deprecated_thermo".to_string(),
            },
            &["deprecated_thermo"],
        );
        reg.add(
            ResolvedConcept {
                ccid: ccid(21),
                qid: 2001,
                category: ConceptCategory::Entity,
                canonical_name: "canonical_thermo".to_string(),
            },
            &["canonical_thermo"],
        );
        reg.add(
            ResolvedConcept {
                ccid: ccid(30),
                qid: 3000,
                category: ConceptCategory::Entity,
                canonical_name: "cell_ambiguous_parent".to_string(),
            },
            &["cell_parent"],
        );
        reg.add(
            ResolvedConcept {
                ccid: ccid(31),
                qid: 3001,
                category: ConceptCategory::Entity,
                canonical_name: "biological_cell".to_string(),
            },
            &["biological_cell"],
        );
        reg.add(
            ResolvedConcept {
                ccid: ccid(32),
                qid: 3002,
                category: ConceptCategory::Entity,
                canonical_name: "battery_cell".to_string(),
            },
            &["battery_cell"],
        );
        reg.add(
            ResolvedConcept {
                ccid: ccid(40),
                qid: 4000,
                category: ConceptCategory::Entity,
                canonical_name: "phlogiston".to_string(),
            },
            &["phlogiston"],
        );

        let lifecycle = ConceptLifecycleTable::from_records(vec![
            ConceptLifecycleRecord::Redirect(ConceptRedirect {
                deprecated_ccid: ConceptCcid::from_bytes(ccid(20)),
                canonical_ccid: ConceptCcid::from_bytes(ccid(21)),
                since_release_seq: 2,
                provenance_ref: "wd:Q2000->wd:Q2001".to_string(),
            }),
            ConceptLifecycleRecord::Split(ConceptSplit {
                parent_ccid: ConceptCcid::from_bytes(ccid(30)),
                branch_ccids: vec![
                    ConceptCcid::from_bytes(ccid(31)),
                    ConceptCcid::from_bytes(ccid(32)),
                ],
                disambiguation_role_ccid: ConceptCcid::from_bytes(ccid(99)),
                since_release_seq: 3,
            }),
            ConceptLifecycleRecord::Retire(ConceptRetire {
                retired_ccid: ConceptCcid::from_bytes(ccid(40)),
                reason_code: ConceptRetireReason::RetractedUpstream,
                since_release_seq: 4,
            }),
        ])
        .unwrap();
        reg.set_lifecycle_table(lifecycle);
        reg
    }

    fn with_ctx<R>(
        reg: &ConceptRegistry,
        units: &SiUnitCatalog,
        f: impl FnOnce(&NeedCompileContext<'_>, SelectorCid) -> R,
    ) -> R {
        let selector = public_knowledge_exchange_fixture_v1().cid().unwrap();
        let allowed = [selector];
        let ctx = NeedCompileContext {
            registry: reg,
            registry_release_root: [0x77; 32],
            unit_catalog: units,
            allowed_public_selectors: &allowed,
            selector,
            query_policy: reference(50),
            exploration_policy: reference(51),
            watch_policy: reference(52),
            acceptance_policy: reference(53),
            observed_frontier: [0x88; 32],
            source_frontier: EventCid::from_bytes([0x99; 32]),
            generator: reference(54),
            metrics: MatcherMetricConcepts {
                structural_fit: ConceptCcid::from_bytes(ccid(60)),
                constraint_fit: ConceptCcid::from_bytes(ccid(61)),
            },
            unmapped_reason: ConceptCcid::from_bytes(ccid(62)),
            created_at_evaluation: 1,
            expires_after_evaluations: 16,
        };
        f(&ctx, selector)
    }

    #[test]
    fn rejects_all_non_semantic_fields_and_scope_global_fail_closed() {
        let reg = test_registry();
        let units = SiUnitCatalog::standard();
        with_ctx(&reg, &units, |ctx, _| {
            for bad_query in [
                "FIND (k:KU) WHERE pomv > 10",
                "FIND (k:KU) WHERE trust_score > 8000",
                "FIND (k:KU) WHERE metabolic_rate > 1",
                "FIND (k:KU) WHERE obt_balance >= 100",
                "FIND (k:KU) WHERE peer_count > 3",
                "FIND (k:KU) WHERE wire_size < 512",
                "FIND (k:KU) WHERE encoding_status = \"verified\"",
                "FIND (k:KU) WHERE gene_type = 0",
                "FIND (k:KU) WHERE predicate = \"boiling_point\" SCOPE GLOBAL",
            ] {
                let err = KqlNeedCompiler::compile_kql(bad_query, ctx).unwrap_err();
                assert_eq!(
                    err,
                    NeedCompileError::NonSemanticFieldForbidden(NON_SEMANTIC_FIELD_ERROR_CODE),
                    "query did not fail with non_semantic_field_forbidden_in_need_ir: {bad_query}"
                );
            }
        });
    }

    #[test]
    fn rejects_ieee754_floats_and_parses_exact_decimals_and_rationals() {
        let reg = test_registry();
        let units = SiUnitCatalog::standard();
        with_ctx(&reg, &units, |ctx, _| {
            assert_eq!(
                KqlNeedCompiler::compile_kql(
                    "FIND (k:KU) WHERE predicate = \"boiling_point\" AND quantity > 99.5",
                    ctx,
                )
                .unwrap_err(),
                NeedCompileError::FloatLiteralForbidden
            );
            assert_eq!(
                parse_exact_decimal_or_rational("1.25e2").unwrap_err(),
                NeedCompileError::FloatLiteralForbidden
            );
            assert_eq!(
                parse_exact_decimal_or_rational("12.50").unwrap(),
                ExactRatio::new(25, 2).unwrap()
            );
            assert_eq!(
                parse_exact_decimal_or_rational("25/2").unwrap(),
                ExactRatio::new(25, 2).unwrap()
            );
        });
    }

    #[test]
    fn canonical_identity_converges_across_whitespace_variable_names_and_constraint_order() {
        let reg = test_registry();
        let units = SiUnitCatalog::standard();
        with_ctx(&reg, &units, |ctx, allowed_selector| {
            let mut q1 = String::from(
                "FIND (?a)-[:boiling_point]->(?b) WHERE a.concept = \"water\" AND quantity >= \"99.5 degC\" AND quantity <= \"201/2 degC\"",
            );
            let mut q2 = String::from(
                "   FIND   (?x)-[:boiling_point]->(?y)   WHERE   quantity <= \"100.50 degC\"   AND   x.concept = \"water\"   AND   quantity >= \"199/2 degC\"  ",
            );

            let c1 = KqlNeedCompiler::compile_kql_in_place(&mut q1, ctx).unwrap();
            let c2 = KqlNeedCompiler::compile_kql_in_place(&mut q2, ctx).unwrap();

            // Raw KQL buffers are zeroized upon compilation.
            assert!(q1.is_empty());
            assert!(q2.is_empty());

            // Byte-identical ReceptorDefinition CID and QueryDefinition private CID!
            assert_eq!(c1.receptor_cid, c2.receptor_cid);
            assert_eq!(c1.query_cid, c2.query_cid);
            assert_eq!(
                c1.bundle
                    .query_definition
                    .private_canonical_bytes()
                    .unwrap(),
                c2.bundle
                    .query_definition
                    .private_canonical_bytes()
                    .unwrap()
            );
            assert_eq!(c1.bundle.target.need.selector, allowed_selector);
            assert_eq!(c1.provenance.compiler_profile, COMPILER_PROFILE_ID);
            assert!(c1.omits_hash_derived_limitation());
        });
    }

    #[test]
    fn enforces_lifecycle_redirects_splits_retirements_ambiguity_and_selector_firewall() {
        let reg = test_registry();
        let units = SiUnitCatalog::standard();
        with_ctx(&reg, &units, |ctx, _| {
            // Redirect 20 -> 21 applied and recorded in provenance.
            let redirected = KqlNeedCompiler::compile_kql(
                "FIND (k:KU) WHERE predicate = \"boiling_point\" AND subject = \"deprecated_thermo\"",
                ctx,
            )
            .unwrap();
            assert_eq!(
                redirected.provenance.lifecycle_redirects_applied,
                vec![LifecycleRedirectApplied {
                    deprecated_ccid: ConceptCcid::from_bytes(ccid(20)),
                    canonical_ccid: ConceptCcid::from_bytes(ccid(21)),
                }]
            );
            assert_eq!(
                redirected.bundle.query_definition.need.goal.statements[0].arguments[0],
                TermRef::Concept(ConceptCcid::from_bytes(ccid(21)))
            );

            // Ambiguous label fails closed with candidate list (never hashes string!).
            match KqlNeedCompiler::compile_kql(
                "FIND (k:KU) WHERE predicate = \"boiling_point\" AND subject = \"bank\"",
                ctx,
            )
            .unwrap_err()
            {
                NeedCompileError::AmbiguousTerm { candidates, .. } => {
                    assert_eq!(candidates.len(), 2);
                }
                other => panic!("expected AmbiguousTerm, got {other:?}"),
            }

            // Split concept fails closed with branch disambiguation candidates.
            match KqlNeedCompiler::compile_kql(
                "FIND (k:KU) WHERE predicate = \"boiling_point\" AND subject = \"cell_parent\"",
                ctx,
            )
            .unwrap_err()
            {
                NeedCompileError::SplitDisambiguationRequired { candidates, .. } => {
                    assert_eq!(candidates.len(), 2);
                }
                other => panic!("expected SplitDisambiguationRequired, got {other:?}"),
            }

            // Retired concept fails closed.
            assert!(matches!(
                KqlNeedCompiler::compile_kql(
                    "FIND (k:KU) WHERE predicate = \"boiling_point\" AND subject = \"phlogiston\"",
                    ctx,
                ),
                Err(NeedCompileError::RetiredConcept { .. })
            ));

            // Unknown concept fails closed (never invents CCID).
            assert!(matches!(
                KqlNeedCompiler::compile_kql(
                    "FIND (k:KU) WHERE predicate = \"boiling_point\" AND subject = \"nonexistent_xyz\"",
                    ctx,
                ),
                Err(NeedCompileError::UnresolvedTerm { .. })
            ));

            // Unapproved selector derived from private bytes fails closed (NEG-KQL-001/005).
            let bad_ctx = NeedCompileContext {
                selector: SelectorCid::from_bytes([0xfe; 32]),
                ..ctx.clone()
            };
            assert_eq!(
                KqlNeedCompiler::compile_kql(
                    "FIND (k:KU) WHERE predicate = \"boiling_point\" AND subject = \"water\"",
                    &bad_ctx,
                )
                .unwrap_err(),
                NeedCompileError::UnapprovedSelector(SelectorCid::from_bytes([0xfe; 32]))
            );
        });
    }
}

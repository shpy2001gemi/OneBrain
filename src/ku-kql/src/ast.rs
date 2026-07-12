//! # KQL Abstract Syntax Tree
//!
//! Types representing parsed KQL queries.
//! Covers FIND, CREATE, UPDATE, DEPRECATE, WATCH, and EXPLAIN.

use ku_core::{EpistemicStatus, EvidenceType, RoleId};

// ═══════════════════════════════════════════════════════════════════════════
// Top-Level Query
// ═══════════════════════════════════════════════════════════════════════════

/// A parsed KQL query.
#[derive(Debug, Clone, PartialEq)]
pub enum Query {
    Find(FindQuery),
    Create(CreateQuery),
    CreateFromText(CreateFromTextQuery),
    Update(UpdateQuery),
    Deprecate(DeprecateQuery),
    Watch(WatchQuery),
    Explain(Box<Query>),
}

// ═══════════════════════════════════════════════════════════════════════════
// FIND Query
// ═══════════════════════════════════════════════════════════════════════════

/// `FIND (k:KU) WHERE ... SCOPE ... RETURN ... LIMIT ...`
#[derive(Debug, Clone, PartialEq)]
pub struct FindQuery {
    /// Pattern to match.
    pub pattern: Pattern,
    /// Filter conditions.
    pub where_clause: Option<Condition>,
    /// Query scope.
    pub scope: Scope,
    /// What to return.
    pub return_clause: Option<Vec<ReturnExpr>>,
    /// Maximum results.
    pub limit: Option<u32>,
    /// Ordering.
    pub order_by: Option<Vec<OrderExpr>>,
    /// Temporal clause: AT TIME or DURING.
    pub temporal: Option<TemporalClause>,
    /// Whether to query historical versions.
    pub history: bool,
}

/// Temporal query clause.
#[derive(Debug, Clone, PartialEq)]
pub enum TemporalClause {
    /// Query state at a specific timestamp.
    AtTime(u64),
    /// Query state during a time range.
    During { from: u64, to: u64 },
}

// ═══════════════════════════════════════════════════════════════════════════
// CREATE Query
// ═══════════════════════════════════════════════════════════════════════════

/// `CREATE (k:KU { ... }) SIGNED BY ...`         — Legacy property-bag syntax
/// `CREATE (k:KU) FACT certainty=9000 { ... }`    — Tier 1 structured syntax
/// `CREATE FROM TEXT "..." WITH AI model="..."`    — Tier 2 AI-assisted syntax
#[derive(Debug, Clone, PartialEq)]
pub struct CreateQuery {
    /// The node pattern to create.
    pub pattern: Pattern,
    /// Properties to set (legacy property-bag syntax).
    pub properties: Vec<Property>,
    /// Gene type (Tier 1 structured syntax). If None, falls back to properties.
    pub gene_type: Option<KqlGeneType>,
    /// Certainty level (0-10000). If None, defaults to 5000.
    pub certainty: Option<u16>,
    /// Structured instruction clauses (Tier 1).
    pub instructions: Vec<CreateClause>,
    /// Signer identity.
    pub signed_by: String,
}

/// Tier 2: `CREATE FROM TEXT "..." WITH AI model="..." [gene_hint="..."]`
///
/// AI-assisted knowledge creation from natural language text.
/// The executor calls a local AI model to decompose text into CoreDna instructions.
#[derive(Debug, Clone, PartialEq)]
pub struct CreateFromTextQuery {
    /// Natural language text to parse into knowledge.
    pub text: String,
    /// AI model name (e.g., "gemma4", "qwen", "phi-3").
    pub model: String,
    /// Optional gene type hint for the AI.
    pub gene_hint: Option<KqlGeneType>,
    /// Signer identity (optional).
    pub signed_by: String,
}

/// Gene type keyword in Tier 1 CREATE syntax (v7: 13 types).
#[derive(Debug, Clone, PartialEq)]
pub enum KqlGeneType {
    Fact,
    Procedure,
    Experience,
    Creative,
    MediaExperience,
    Testimony,
    Formal,
    Hypothesis,
    Narrative,
    Sensory,
    Composite,
    Normative,
    Definition,
}

impl KqlGeneType {
    /// Convert to Core DNA gene_type number (v7 numbering).
    pub fn to_u8(&self) -> u8 {
        match self {
            Self::Fact => 0,
            Self::Procedure => 1,
            Self::Experience => 2,
            Self::Creative => 3,
            Self::MediaExperience => 4,
            Self::Testimony => 5,
            Self::Formal => 6,
            Self::Hypothesis => 7,
            Self::Narrative => 8,
            Self::Sensory => 9,
            Self::Composite => 10,
            Self::Normative => 11,
            Self::Definition => 12,
        }
    }
}

/// A single instruction clause in a Tier 1 structured CREATE block.
///
/// Clause args use `String` (concept names) — ConceptDict resolution
/// happens at execution time, not parse time.
#[derive(Debug, Clone, PartialEq)]
pub enum CreateClause {
    /// `TRIPLE(subject, predicate, object)` — 3 concept names
    Triple { s: String, p: String, o: String },
    /// `QUALITY(subject, quality)` — 2 concept names
    Quality { s: String, q: String },
    /// `QUANTITY(subject, value, unit)` — concept, number, concept
    Quantity { s: String, value: f64, unit: String },
    /// `PARTOF(part, whole)` — 2 concept names
    PartOf { part: String, whole: String },
    /// `LOCATED(subject, location)` — 2 concept names
    Located { s: String, location: String },
    /// `TEMPORAL(subject, time)` — 2 concept names
    Temporal { s: String, time: String },
    /// `CAUSAL(cause, effect)` — 2 concept names
    Causal { cause: String, effect: String },
    /// `STEP(order, action, target)` — ordinal + 2 concepts
    Step { ord: u8, action: String, target: String },
    /// `PRECOND(concept)` — 1 concept name
    Precond { concept: String },
    /// `EFFECT(concept)` — 1 concept name
    Effect { concept: String },
    /// `CERTAINTY(level)` — u16 value
    Certainty { level: u16 },
    /// `TOLERANCE(subject, value, delta)` — concept + 2 numbers
    Tolerance { s: String, value: f64, delta: f64 },
    /// `RANGE(subject, min, max)` — concept + 2 numbers
    Range { s: String, min: f64, max: f64 },
    /// `CONSTRAINT(source, op, target)` — concept, operator string, concept
    Constraint { source: String, op: String, target: String },
}

// ═══════════════════════════════════════════════════════════════════════════
// UPDATE / DEPRECATE Queries
// ═══════════════════════════════════════════════════════════════════════════

/// `UPDATE (k:KU) SET ... WHERE ... SIGNED BY ...`
#[derive(Debug, Clone, PartialEq)]
pub struct UpdateQuery {
    pub pattern: Pattern,
    pub set_clause: Vec<Assignment>,
    pub where_clause: Option<Condition>,
    pub signed_by: String,
}

/// `DEPRECATE (k:KU) WHERE ... REASON "..." SIGNED BY ...`
#[derive(Debug, Clone, PartialEq)]
pub struct DeprecateQuery {
    pub pattern: Pattern,
    pub where_clause: Option<Condition>,
    pub reason: String,
    pub signed_by: String,
}

// ═══════════════════════════════════════════════════════════════════════════
// WATCH Query
// ═══════════════════════════════════════════════════════════════════════════

/// `WATCH FIND ... ON event NOTIFY "endpoint"`
#[derive(Debug, Clone, PartialEq)]
pub struct WatchQuery {
    pub find: FindQuery,
    pub event: WatchEvent,
    pub notify: String,
}

/// Events that can trigger a WATCH notification.
#[derive(Debug, Clone, PartialEq)]
pub enum WatchEvent {
    Create,
    Update,
    Deprecate,
    Any,
}

// ═══════════════════════════════════════════════════════════════════════════
// Pattern (Graph matching)
// ═══════════════════════════════════════════════════════════════════════════

/// A graph pattern for matching KUs.
#[derive(Debug, Clone, PartialEq)]
pub struct Pattern {
    /// Node patterns.
    pub nodes: Vec<NodePattern>,
    /// Edge patterns connecting nodes.
    pub edges: Vec<EdgePattern>,
}

/// A single node in the pattern.
#[derive(Debug, Clone, PartialEq)]
pub struct NodePattern {
    /// Alias for referencing (e.g., `k` in `(k:KU)`).
    pub alias: Option<String>,
    /// Label (e.g., `KU`, `Concept`).
    pub label: NodeLabel,
    /// Property filters.
    pub properties: Vec<Property>,
}

/// Node labels.
#[derive(Debug, Clone, PartialEq)]
pub enum NodeLabel {
    KU,
    Concept,
}

/// Edge pattern between two nodes.
#[derive(Debug, Clone, PartialEq)]
pub struct EdgePattern {
    /// Alias for the edge.
    pub alias: Option<String>,
    /// Edge type(s) to match.
    pub edge_types: Vec<String>,
    /// Direction.
    pub direction: EdgeDirection,
    /// Source node index in Pattern::nodes.
    pub from: usize,
    /// Target node index.
    pub to: usize,
    /// Variable-length path depth (e.g., *1..3).
    pub path_depth: Option<PathDepth>,
}

/// Variable-length path depth: `*min..max`.
#[derive(Debug, Clone, PartialEq)]
pub struct PathDepth {
    /// Minimum hops (default: 1)
    pub min: usize,
    /// Maximum hops (default: 1)
    pub max: usize,
}

impl Default for PathDepth {
    fn default() -> Self {
        Self { min: 1, max: 1 }
    }
}

/// Edge direction in pattern matching.
#[derive(Debug, Clone, PartialEq)]
pub enum EdgeDirection {
    Outgoing,   // -[r:TYPE]->
    Incoming,   // <-[r:TYPE]-
    /// Programmatic only — no KQL text syntax currently parses this direction.
    Undirected, // -[r:TYPE]-
}

// ═══════════════════════════════════════════════════════════════════════════
// Conditions (WHERE clause)
// ═══════════════════════════════════════════════════════════════════════════

/// A filter condition.
#[derive(Debug, Clone, PartialEq)]
pub enum Condition {
    /// Field comparison: `k.trust_score > 8000`
    Comparison {
        field: FieldPath,
        op: CompOp,
        value: Value,
    },
    /// Logical AND.
    And(Box<Condition>, Box<Condition>),
    /// Logical OR.
    Or(Box<Condition>, Box<Condition>),
    /// Logical NOT.
    Not(Box<Condition>),
    /// Field existence check.
    Exists(FieldPath),
    /// Membership: `value IN field`
    Contains {
        field: FieldPath,
        value: Value,
    },
}

/// Comparison operators.
#[derive(Debug, Clone, PartialEq)]
pub enum CompOp {
    Eq,       // =
    NotEq,    // !=
    Gt,       // >
    GtEq,     // >=
    Lt,       // <
    LtEq,     // <=
}

/// A dotted field path: `k.trust.trust_score`
#[derive(Debug, Clone, PartialEq)]
pub struct FieldPath {
    pub segments: Vec<String>,
}

impl FieldPath {
    pub fn new(segments: Vec<String>) -> Self {
        Self { segments }
    }

    pub fn root(&self) -> &str {
        &self.segments[0]
    }

    pub fn field(&self) -> &str {
        if self.segments.len() > 1 {
            &self.segments[1]
        } else {
            &self.segments[0]
        }
    }
}

// ═══════════════════════════════════════════════════════════════════════════
// Values
// ═══════════════════════════════════════════════════════════════════════════

/// A literal value in KQL.
#[derive(Debug, Clone, PartialEq)]
pub enum Value {
    Integer(i64),
    Float(f64),
    Text(String),
    Bool(bool),
    /// Programmatic only — no KQL text syntax currently parses this variant.
    ConceptId(u64),
    /// Programmatic only — no KQL text syntax currently parses this variant.
    EpistemicStatus(EpistemicStatus),
    /// Programmatic only — no KQL text syntax currently parses this variant.
    EvidenceType(EvidenceType),
    /// Programmatic only — no KQL text syntax currently parses this variant.
    Role(RoleId),
    /// Programmatic only — no KQL text syntax currently parses this variant.
    /// ★ OBKG Phase 3: Unix timestamp value
    Timestamp(u64),
}

// ═══════════════════════════════════════════════════════════════════════════
// Scope
// ═══════════════════════════════════════════════════════════════════════════

/// Query execution scope.
#[derive(Debug, Clone, PartialEq)]
pub enum Scope {
    Local,
    Neighbors,
    Cluster,
    Dht,
    Semantic,
    Global,
    Auto,
}

impl Default for Scope {
    fn default() -> Self {
        Self::Auto
    }
}

// ═══════════════════════════════════════════════════════════════════════════
// Return / Order / Assignment
// ═══════════════════════════════════════════════════════════════════════════

/// What to return from a query.
#[derive(Debug, Clone, PartialEq)]
pub enum ReturnExpr {
    /// Return a field: `k.trust_score`
    Field(FieldPath),
    /// Aggregate: `COUNT(k)`
    Aggregate {
        func: AggFunc,
        field: FieldPath,
        alias: Option<String>,
    },
    /// Return all: `k`
    Alias(String),
}

/// Aggregation functions.
#[derive(Debug, Clone, PartialEq)]
pub enum AggFunc {
    Count,
    Sum,
    Avg,
    Min,
    Max,
}

/// Ordering expression.
#[derive(Debug, Clone, PartialEq)]
pub struct OrderExpr {
    pub field: FieldPath,
    pub descending: bool,
}

/// SET clause assignment.
#[derive(Debug, Clone, PartialEq)]
pub struct Assignment {
    pub field: FieldPath,
    pub value: Value,
}

/// Key-value property in patterns.
#[derive(Debug, Clone, PartialEq)]
pub struct Property {
    pub key: String,
    pub value: Value,
}

//! # Tier 0 Universal Concepts (0-127)
//!
//! Hardcoded concept IDs for the universal knowledge grammar.
//! These are the "genetic code" of KU — fixed, universal, never change.
//!
//! ## Capacity: 128 slots
//! - 74 defined (0-79)
//! - 47 reserved (80-126)
//! - 1 sentinel (127)

use crate::types::ConceptId;

// ═══════════════════════════════════════════════════════════════
// Structural Predicates (0-15)
// ═══════════════════════════════════════════════════════════════

/// Self-reference (identity)
pub const SELF_REF: ConceptId = 0;
/// Taxonomy: X is a Y
pub const IS_A: ConceptId = 1;
/// Meronymy: X has part Y
pub const HAS_PART: ConceptId = 2;
/// Generic relation (fallback)
pub const RELATED_TO: ConceptId = 3;
/// Instantiation: X is instance of class Y
pub const INSTANCE_OF: ConceptId = 4;
/// Class hierarchy: X is subclass of Y
pub const SUBCLASS_OF: ConceptId = 5;
/// Antonymy: X is opposite of Y
pub const OPPOSITE_OF: ConceptId = 6;
/// Analogy/synonymy: X is similar to Y
pub const SIMILAR_TO: ConceptId = 7;
/// Origin/etymology: X derives from Y
pub const DERIVES_FROM: ConceptId = 8;
/// Logical implication: X implies Y
pub const IMPLIES: ConceptId = 9;
/// Equivalence/identity: X is equivalent to Y
pub const EQUIVALENT: ConceptId = 10;
/// Distinctness: X is distinct from Y
pub const DISTINCT: ConceptId = 11;
/// X is property of Y
pub const PROPERTY_OF: ConceptId = 12;
/// X is value of property Y
pub const VALUE_OF: ConceptId = 13;
/// Material composition: X is made of Y
pub const MADE_OF: ConceptId = 14;
/// Purpose/function: X is used for Y
pub const USED_FOR: ConceptId = 15;

// ═══════════════════════════════════════════════════════════════
// Causal & Temporal (16-27)
// ═══════════════════════════════════════════════════════════════

/// Causation: X causes Y
pub const CAUSES: ConceptId = 16;
/// Prevention: X prevents Y
pub const PREVENTS: ConceptId = 17;
/// Enablement: X enables Y
pub const ENABLES: ConceptId = 18;
/// Temporal before: X precedes Y
pub const PRECEDES: ConceptId = 19;
/// Temporal after: X follows Y
pub const FOLLOWS: ConceptId = 20;
/// Temporal containment: X during Y
pub const DURING: ConceptId = 21;
/// Start point
pub const BEGINS: ConceptId = 22;
/// End point
pub const ENDS: ConceptId = 23;
/// Co-occurrence: X simultaneous with Y
pub const SIMULTANEOUS: ConceptId = 24;
/// Correlation (not causation)
pub const CORRELATES: ConceptId = 25;
/// Prerequisite: X requires Y
pub const REQUIRES: ConceptId = 26;
/// Production/output: X produces Y
pub const PRODUCES: ConceptId = 27;

// ═══════════════════════════════════════════════════════════════
// Spatial (28-35)
// ═══════════════════════════════════════════════════════════════

/// Location: X at Y
pub const AT: ConceptId = 28;
/// Spatial containment: X contains Y
pub const CONTAINS: ConceptId = 29;
/// Spatial above
pub const ABOVE: ConceptId = 30;
/// Spatial below
pub const BELOW: ConceptId = 31;
/// Spatial proximity
pub const NEAR: ConceptId = 32;
/// Spatial inside
pub const INSIDE: ConceptId = 33;
/// Spatial between
pub const BETWEEN: ConceptId = 34;
/// Spatial adjacency
pub const ADJACENT: ConceptId = 35;

// ═══════════════════════════════════════════════════════════════
// Logical & Modal (36-43)
// ═══════════════════════════════════════════════════════════════

/// Negation (as concept)
pub const NOT: ConceptId = 36;
/// Conjunction
pub const AND: ConceptId = 37;
/// Disjunction
pub const OR: ConceptId = 38;
/// Conditional: if X then Y
pub const IF_THEN: ConceptId = 39;
/// Possibility
pub const POSSIBLE: ConceptId = 40;
/// Necessity
pub const NECESSARY: ConceptId = 41;
/// Existential quantifier
pub const EXISTS: ConceptId = 42;
/// Universal quantifier
pub const FOR_ALL: ConceptId = 43;

// ═══════════════════════════════════════════════════════════════
// Units: SI Base (44-50)
// ═══════════════════════════════════════════════════════════════

/// Length (m)
pub const UNIT_METER: ConceptId = 44;
/// Mass (kg)
pub const UNIT_KILOGRAM: ConceptId = 45;
/// Time (s)
pub const UNIT_SECOND: ConceptId = 46;
/// Electric current (A)
pub const UNIT_AMPERE: ConceptId = 47;
/// Temperature (K)
pub const UNIT_KELVIN: ConceptId = 48;
/// Amount of substance (mol)
pub const UNIT_MOLE: ConceptId = 49;
/// Luminous intensity (cd)
pub const UNIT_CANDELA: ConceptId = 50;

// ═══════════════════════════════════════════════════════════════
// Units: Common Derived (51-63)
// ═══════════════════════════════════════════════════════════════

/// Frequency (Hz)
pub const UNIT_HERTZ: ConceptId = 51;
/// Force (N)
pub const UNIT_NEWTON: ConceptId = 52;
/// Pressure (Pa)
pub const UNIT_PASCAL: ConceptId = 53;
/// Energy (J)
pub const UNIT_JOULE: ConceptId = 54;
/// Power (W)
pub const UNIT_WATT: ConceptId = 55;
/// Voltage (V)
pub const UNIT_VOLT: ConceptId = 56;
/// Angle (°)
pub const UNIT_DEGREE: ConceptId = 57;
/// Angle (rad)
pub const UNIT_RADIAN: ConceptId = 58;
/// Percentage (%)
pub const UNIT_PERCENT: ConceptId = 59;
/// Digital storage (byte)
pub const UNIT_BYTE: ConceptId = 60;
/// Digital information (bit)
pub const UNIT_BIT: ConceptId = 61;
/// Volume (L)
pub const UNIT_LITER: ConceptId = 62;
/// Dimensionless quantity
pub const UNIT_DIMENSIONLESS: ConceptId = 63;

// ═══════════════════════════════════════════════════════════════
// Epistemological (64-69)
// ═══════════════════════════════════════════════════════════════

/// Truth value
pub const TRUE_VAL: ConceptId = 64;
/// Falsehood value
pub const FALSE_VAL: ConceptId = 65;
/// Unknown value
pub const UNKNOWN_VAL: ConceptId = 66;
/// Approximate value
pub const APPROXIMATE: ConceptId = 67;
/// Exact value
pub const EXACT: ConceptId = 68;
/// Measured value
pub const MEASURED: ConceptId = 69;

// ═══════════════════════════════════════════════════════════════
// Agentive / Thematic Roles (70-79)
// ═══════════════════════════════════════════════════════════════

/// Who does (actor)
pub const AGENT: ConceptId = 70;
/// Who receives (affected)
pub const PATIENT: ConceptId = 71;
/// With what (tool)
pub const INSTRUMENT: ConceptId = 72;
/// For whom
pub const BENEFICIARY: ConceptId = 73;
/// From where
pub const SOURCE: ConceptId = 74;
/// To where/what
pub const GOAL: ConceptId = 75;
/// Why
pub const PURPOSE: ConceptId = 76;
/// How
pub const METHOD: ConceptId = 77;
/// Outcome
pub const RESULT: ConceptId = 78;
/// Under what condition
pub const CONDITION: ConceptId = 79;

// ═══════════════════════════════════════════════════════════════
// Reserved (80-126) — 47 slots for future universal concepts
// ═══════════════════════════════════════════════════════════════

// IDs 80-126 are reserved. Do not assign without spec update.

// ═══════════════════════════════════════════════════════════════
// Sentinel
// ═══════════════════════════════════════════════════════════════

/// Unknown/fallback concept (sentinel value)
pub const UNKNOWN_CONCEPT: ConceptId = 127;

/// Total number of defined Tier 0 concepts
pub const TIER0_DEFINED_COUNT: usize = 80; // 0-79

/// Total Tier 0 slots (including reserved + sentinel)
pub const TIER0_TOTAL_SLOTS: usize = 128; // 0-127

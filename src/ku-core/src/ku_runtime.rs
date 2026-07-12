//! KuRuntime — The unified runtime composite for KU v6 3-layer architecture.
//!
//! `KuRuntime` combines all three layers of a Knowledge Unit into a single
//! queryable in-memory representation:
//!
//! - **Layer 1 (Core DNA)**: Compact binary instruction stream (stored)
//! - **Layer 2 (Epigenetics)**: Runtime trust/bond metadata (stored separately)
//! - **Layer 3 (Expression)**: Natural language rendering (generated on-demand)
//!
//! # Usage
//! ```text
//! // Create from Core DNA bytes
//! let runtime = KuRuntime::from_wire(wire_bytes)?;
//!
//! // Access layers
//! let gene_type = runtime.dna.header.gene_type;
//! let trust = runtime.epi.trust.trust_score;
//! let text = runtime.expression("vi", &dict);
//! ```

use crate::concept_dict::ConceptDict;
use crate::core_dna::{CoreDna, Instruction, decode_core_dna, encode_core_dna};
use crate::encoding_consensus::EncodingStatus;
use crate::epigenetics::{Epigenetics, Expression};
use crate::error::KuError;
use crate::pomv_runtime::TrustSectionUpdate;
use crate::types::ConceptId;

// ============================================================================
// KuRuntime — the full runtime composite
// ============================================================================

/// Complete Knowledge Unit runtime representation.
///
/// This is the primary struct used by KQL queries, PoK/PoMV scoring,
/// and OBP network transport.
///
/// # 3-Layer Architecture
///
/// | Layer | Field | Stored? | Format |
/// |-------|-------|---------|--------|
/// | 1. Core DNA | `dna` | ✅ Persistent | Custom binary (16-172B) |
/// | 2. Epigenetics | `epi` | ✅ Separate store | CBOR/SQLite |
/// | 3. Expression | `expr` | ❌ Generated | On-demand text |
#[derive(Debug, Clone, PartialEq)]
pub struct KuRuntime {
    /// Content identity — BLAKE3 hash of `wire_bytes`.
    /// This is the globally unique identifier for this KU's knowledge content.
    /// Immutable: changing Core DNA produces a new CID (= new KU).
    pub cid: [u8; 32],

    /// Layer 1: Core DNA — the compact binary instruction stream.
    /// Contains: gene type, instructions (32 opcodes), certainty, etc.
    /// This is the ONLY layer persisted in the primary wire format.
    pub dna: CoreDna,

    /// Layer 2: Epigenetics — runtime metadata overlay.
    /// Contains: trust scores, bonds, epistemic status, embeddings.
    /// Stored separately (e.g., in SQLite). Optional for newly received KUs.
    pub epi: Epigenetics,

    /// Layer 3: Expression — natural language rendering.
    /// Generated on-demand from Core DNA + ConceptDict.
    /// `None` until explicitly requested (lazy evaluation).
    pub expr: Option<Expression>,

    /// Raw Core DNA wire bytes — for storage and network transport.
    /// This is the canonical serialized form: `MAGIC | VER_META | INSTRUCTIONS | END | CRC-16`.
    pub wire_bytes: Vec<u8>,

    /// Encoding verification status — tracks how well-verified the encoding is.
    /// RAW → SELF → PART → FULL. See `encoding_consensus` module.
    /// Separate from `EpistemicStatus` (which tracks knowledge quality).
    pub encoding_status: EncodingStatus,
}

impl KuRuntime {
    /// Create a new KuRuntime from a decoded CoreDna and its wire bytes.
    pub fn new(dna: CoreDna, wire_bytes: Vec<u8>) -> Self {
        let cid = blake3::hash(&wire_bytes).into();
        Self {
            cid,
            dna,
            epi: Epigenetics::default(),
            expr: None,
            wire_bytes,
            encoding_status: EncodingStatus::Self_,
        }
    }

    /// Decode from raw Core DNA wire bytes.
    pub fn from_wire(wire_bytes: Vec<u8>) -> Result<Self, KuError> {
        let dna = decode_core_dna(&wire_bytes)?;
        let cid = blake3::hash(&wire_bytes).into();
        Ok(Self {
            cid,
            dna,
            epi: Epigenetics::default(),
            expr: None,
            wire_bytes,
            encoding_status: EncodingStatus::Self_,
        })
    }

    /// Build from a CoreDna struct (encodes to wire bytes automatically).
    pub fn from_dna(dna: CoreDna) -> Result<Self, KuError> {
        let wire_bytes = encode_core_dna(&dna)?;
        let cid = blake3::hash(&wire_bytes).into();
        Ok(Self {
            cid,
            dna,
            epi: Epigenetics::default(),
            expr: None,
            wire_bytes,
            encoding_status: EncodingStatus::Self_,
        })
    }

    /// Attach epigenetics data (e.g., loaded from SQLite).
    pub fn with_epigenetics(mut self, epi: Epigenetics) -> Self {
        self.epi = epi;
        self
    }

    /// Recompute wire_bytes and CID from current dna state.
    /// Call after mutating `dna.instructions` directly.
    pub fn recompute(&mut self) {
        if let Ok(bytes) = encode_core_dna(&self.dna) {
            self.cid = blake3::hash(&bytes).into();
            self.wire_bytes = bytes;
        }
    }

    // ========================================================================
    // PoMV Bridge — KuRuntime ↔ PomvRuntime
    // ========================================================================

    /// Apply a PoMV trust section update to this KU's epigenetics.
    ///
    /// This is the bridge between `PomvRuntime::tick()` output and a live
    /// `KuRuntime`. It copies all signal fields into `epi.trust` via
    /// `TrustSectionUpdate::apply_to` (which also sets epistemic_status).
    pub fn apply_pomv_update(&mut self, update: &TrustSectionUpdate) {
        update.apply_to(&mut self.epi.trust);
    }

    /// Return the raw CID bytes for PomvRuntime key lookup.
    ///
    /// Both `KuRuntime` and `PomvRuntime` use `[u8; 32]` as the KU identity key.
    /// This accessor makes the bridge explicit.
    #[inline]
    pub fn cid_bytes(&self) -> [u8; 32] {
        self.cid
    }

    // ========================================================================
    // Core DNA field accessors (instruction scan)
    // ========================================================================

    /// Gene type as u8 (from CoreDna header VER_META byte).
    pub fn gene_type(&self) -> u8 {
        self.dna.header.gene_type
    }

    /// Extract ALL ConceptIDs referenced in the instruction stream.
    pub fn concept_ids(&self) -> Vec<ConceptId> {
        let mut ids = Vec::new();
        for instr in &self.dna.instructions {
            match instr {
                Instruction::Triple { s, p, o } => {
                    ids.push(*s); ids.push(*p); ids.push(*o);
                }
                Instruction::Quality { s, q } => {
                    ids.push(*s); ids.push(*q);
                }
                Instruction::Quantity { s, unit, .. } => {
                    ids.push(*s); ids.push(*unit);
                }
                Instruction::PartOf { part, whole } => {
                    ids.push(*part); ids.push(*whole);
                }
                Instruction::Located { s, location } => {
                    ids.push(*s); ids.push(*location);
                }
                Instruction::Temporal { s, time } => {
                    ids.push(*s); ids.push(*time);
                }
                Instruction::Causal { cause, effect } => {
                    ids.push(*cause); ids.push(*effect);
                }
                Instruction::Simulates { s, model } => {
                    ids.push(*s); ids.push(*model);
                }
                Instruction::Condition { cond, result } => {
                    ids.push(*cond); ids.push(*result);
                }
                Instruction::Agent { actor, action } => {
                    ids.push(*actor); ids.push(*action);
                }
                Instruction::Tool { action, instrument } => {
                    ids.push(*action); ids.push(*instrument);
                }
                Instruction::Step { action, target, .. } => {
                    ids.push(*action); ids.push(*target);
                }
                Instruction::Precond { concept } | Instruction::Effect { concept } => {
                    ids.push(*concept);
                }
                Instruction::Constraint { source, target, .. } => {
                    ids.push(*source); ids.push(*target);
                }
                Instruction::Range { s, .. } | Instruction::Tolerance { s, .. } => {
                    ids.push(*s);
                }
                Instruction::Sequence { items } => {
                    ids.extend(items);
                }
                Instruction::EnumVal { s, values } => {
                    ids.push(*s);
                    ids.extend(values);
                }
                Instruction::Label { key, value } => {
                    ids.push(*key); ids.push(*value);
                }
                Instruction::Member { label, .. } => {
                    ids.push(*label);
                }
                _ => {} // Certainty, Difficulty, CidRef, Affect, TextRef, Formula, etc.
            }
        }
        ids.sort_unstable();
        ids.dedup();
        ids
    }

    /// Primary concept — the first subject ConceptID in the instruction stream.
    pub fn primary_concept(&self) -> Option<ConceptId> {
        for instr in &self.dna.instructions {
            match instr {
                Instruction::Triple { s, .. }
                | Instruction::Quality { s, .. }
                | Instruction::Quantity { s, .. }
                | Instruction::PartOf { part: s, .. }
                | Instruction::Located { s, .. }
                | Instruction::Temporal { s, .. }
                | Instruction::Range { s, .. }
                | Instruction::Tolerance { s, .. } => return Some(*s),
                Instruction::Causal { cause, .. } => return Some(*cause),
                Instruction::Agent { actor, .. } => return Some(*actor),
                Instruction::Step { action, .. } => return Some(*action),
                _ => continue,
            }
        }
        None
    }

    /// Certainty level (from CERTAINTY instruction, if present).
    pub fn certainty(&self) -> Option<u16> {
        self.dna.instructions.iter().find_map(|i| match i {
            Instruction::Certainty { level } => Some(*level),
            _ => None,
        })
    }

    /// Difficulty level (from DIFFICULTY instruction, if present).
    pub fn difficulty(&self) -> Option<u8> {
        self.dna.instructions.iter().find_map(|i| match i {
            Instruction::Difficulty { level } => Some(*level),
            _ => None,
        })
    }

    /// Number of instructions in Core DNA (excluding END).
    pub fn instruction_count(&self) -> usize {
        self.dna.instructions.iter()
            .filter(|i| !matches!(i, Instruction::End))
            .count()
    }

    /// Whether the instruction stream contains any Triple instructions.
    pub fn has_triple(&self) -> bool {
        self.dna.instructions.iter().any(|i| matches!(i, Instruction::Triple { .. }))
    }

    /// Whether the instruction stream contains any Step instructions.
    pub fn has_step(&self) -> bool {
        self.dna.instructions.iter().any(|i| matches!(i, Instruction::Step { .. }))
    }

    /// Wire size in bytes.
    pub fn wire_size(&self) -> usize {
        self.wire_bytes.len()
    }

    // ========================================================================
    // Expression rendering (Layer 3)
    // ========================================================================

    /// Get or lazily generate the Expression for a given language.
    ///
    /// The expression is cached and reused on subsequent calls with the
    /// same language. Requesting a different language regenerates it.
    ///
    /// # Arguments
    /// * `lang` — ISO 639-1 language code ("en", "vi", etc.)
    /// * `dict` — ConceptDict for resolving ConceptIDs to names
    pub fn expression(&mut self, lang: &str, dict: &ConceptDict) -> &Expression {
        if self.expr.is_none() || self.expr.as_ref().map(|e| e.lang.as_str()) != Some(lang) {
            self.expr = Some(Self::render_expression(&self.dna, lang, dict));
        }
        self.expr.as_ref().unwrap()
    }

    /// Render an Expression from Core DNA instructions.
    ///
    /// Walks the instruction stream, resolves each ConceptID via the
    /// ConceptDict, and produces a semicolon-separated text representation.
    fn render_expression(dna: &CoreDna, lang: &str, dict: &ConceptDict) -> Expression {
        let mut parts: Vec<String> = Vec::new();
        let mut concept_names: Vec<(ConceptId, String)> = Vec::new();

        // Helper closure: resolve a ConceptId to a name, tracking the mapping.
        let mut cname = |id: ConceptId| -> String {
            let name = dict
                .name_lang(id, lang)
                .unwrap_or_else(|| dict.name(id).unwrap_or("?"))
                .to_string();
            concept_names.push((id, name.clone()));
            name
        };

        for instr in &dna.instructions {
            let rendered = match instr {
                Instruction::Triple { s, p, o } => {
                    let sn = cname(*s);
                    let pn = cname(*p);
                    let on = cname(*o);
                    format!("{} {} {}", sn, pn, on)
                }
                Instruction::Quality { s, q } => {
                    let sn = cname(*s);
                    let qn = cname(*q);
                    format!("{}: {}", sn, qn)
                }
                Instruction::Quantity { s, value, unit } => {
                    let sn = cname(*s);
                    let un = cname(*unit);
                    format!("{} = {} {}", sn, value, un)
                }
                Instruction::Step { ord, action, target } => {
                    let an = cname(*action);
                    let tn = cname(*target);
                    format!("Step {}: {} {}", ord, an, tn)
                }
                Instruction::Precond { concept } => {
                    let cn = cname(*concept);
                    format!("Requires: {}", cn)
                }
                Instruction::Effect { concept } => {
                    let cn = cname(*concept);
                    format!("Effect: {}", cn)
                }
                Instruction::PartOf { part, whole } => {
                    let pn = cname(*part);
                    let wn = cname(*whole);
                    format!("{} ⊂ {}", pn, wn)
                }
                Instruction::Located { s, location } => {
                    let sn = cname(*s);
                    let ln = cname(*location);
                    format!("{} @ {}", sn, ln)
                }
                Instruction::Temporal { s, time } => {
                    let sn = cname(*s);
                    let tn = cname(*time);
                    format!("{} → {}", sn, tn)
                }
                Instruction::Causal { cause, effect } => {
                    let cn = cname(*cause);
                    let en = cname(*effect);
                    format!("{} → {}", cn, en)
                }
                Instruction::Certainty { level } => {
                    let pct = *level as f64 / 100.0;
                    format!("Certainty: {}%", pct)
                }
                Instruction::Tolerance { s, value, delta } => {
                    let sn = cname(*s);
                    format!("{} = {} ± {}", sn, value, delta)
                }
                Instruction::Range { s, min, max } => {
                    let sn = cname(*s);
                    format!("{} ∈ [{}, {}]", sn, min, max)
                }
                Instruction::Constraint { source, op, target } => {
                    let sn = cname(*source);
                    let tn = cname(*target);
                    format!("{} {} {}", sn, op, tn)
                }
                Instruction::Simulates { s, model } => {
                    let sn = cname(*s);
                    let mn = cname(*model);
                    format!("{} ~ {}", sn, mn)
                }
                Instruction::Condition { cond, result } => {
                    let cn = cname(*cond);
                    let rn = cname(*result);
                    format!("IF {} THEN {}", cn, rn)
                }
                Instruction::Agent { actor, action } => {
                    let an = cname(*actor);
                    let actn = cname(*action);
                    format!("{} → {}", an, actn)
                }
                Instruction::Tool { action, instrument } => {
                    let an = cname(*action);
                    let in_ = cname(*instrument);
                    format!("{} USING {}", an, in_)
                }
                Instruction::Sequence { items } => {
                    let names: Vec<String> = items.iter().map(|id| cname(*id)).collect();
                    format!("[{}]", names.join(", "))
                }
                Instruction::EnumVal { s, values } => {
                    let sn = cname(*s);
                    let vals: Vec<String> = values.iter().map(|id| cname(*id)).collect();
                    format!("{} ∈ {{{}}}", sn, vals.join(", "))
                }
                Instruction::Label { key, value } => {
                    let kn = cname(*key);
                    let vn = cname(*value);
                    format!("{}: {}", kn, vn)
                }
                Instruction::Difficulty { level } => {
                    format!("Difficulty: {}", level)
                }
                Instruction::Affect { v, a, d } => {
                    format!("Affect(V={}, A={}, D={})", v, a, d)
                }
                Instruction::Witness { count, proximity } => {
                    format!("Witness(count={}, proximity={})", count, proximity)
                }
                // Skip structural/binary instructions that have no text rendering
                Instruction::End
                | Instruction::CidRef { .. }
                | Instruction::TextRef { .. }
                | Instruction::Formula { .. }
                | Instruction::MediaRef { .. }
                | Instruction::CompositeHdr { .. }
                | Instruction::Member { .. } => continue,
            };
            parts.push(rendered);
        }

        // Deduplicate concept_names (same ID may appear multiple times)
        concept_names.sort_by_key(|(id, _)| *id);
        concept_names.dedup_by_key(|(id, _)| *id);

        Expression {
            text: parts.join("; "),
            lang: lang.to_string(),
            concept_names,
        }
    }

    /// Check if a specific ConceptID exists in the instruction stream.
    pub fn contains_concept(&self, concept_id: ConceptId) -> bool {
        self.concept_ids().contains(&concept_id)
    }

    // ========================================================================
    // Epigenetics accessors (convenience)
    // ========================================================================

    /// Trust score (from Epigenetics layer).
    pub fn trust_score(&self) -> u16 {
        self.epi.trust.trust_score
    }

    /// Confidence (from Epigenetics layer).
    pub fn confidence(&self) -> u16 {
        self.epi.trust.confidence
    }

    /// Bond count (from Epigenetics layer).
    pub fn bond_count(&self) -> usize {
        self.epi.bonds.len()
    }

    // ========================================================================
    // KQL field extraction (used by KQL executor)
    // ========================================================================

    /// Extract a named field value for KQL query conditions.
    /// Returns None if the field name is not recognized.
    pub fn extract_field(&self, field: &str) -> Option<ExtractedValue> {
        match field {
            // Core DNA fields (instruction scan)
            "gene_type" => {
                let name = match self.gene_type() {
                    0 => "Fact",
                    1 => "Procedure",
                    2 => "Experience",
                    3 => "Creative",
                    4 => "MediaExperience",
                    5 => "Testimony",
                    6 => "Formal",
                    7 => "Hypothesis",
                    8 => "Narrative",
                    9 => "Sensory",
                    10 => "Composite",
                    11 => "Normative",
                    12 => "Definition",
                    _ => "Unknown",
                };
                Some(ExtractedValue::Text(name.to_string()))
            },
            "primary_concept" => self.primary_concept().map(|c| ExtractedValue::Integer(c as i64)),
            "certainty" => self.certainty().map(|c| ExtractedValue::Integer(c as i64)),
            "difficulty" => self.difficulty().map(|d| ExtractedValue::Integer(d as i64)),
            "instruction_count" => Some(ExtractedValue::Integer(self.instruction_count() as i64)),
            "has_triple" => Some(ExtractedValue::Bool(self.has_triple())),
            "has_step" => Some(ExtractedValue::Bool(self.has_step())),
            "wire_size" => Some(ExtractedValue::Integer(self.wire_size() as i64)),
            "concept_table_size" => Some(ExtractedValue::Integer(self.dna.concept_table.len() as i64)),

            // Epigenetics fields (direct access)
            "trust_score" => Some(ExtractedValue::Integer(self.epi.trust.trust_score as i64)),
            "confidence" => Some(ExtractedValue::Integer(self.epi.trust.confidence as i64)),
            "verification_level" => Some(ExtractedValue::Integer(self.epi.trust.verification_level as i64)),
            "corroboration_count" => Some(ExtractedValue::Integer(self.epi.trust.corroboration_count as i64)),
            "challenge_count" => Some(ExtractedValue::Integer(self.epi.trust.challenge_count as i64)),
            "error_susceptibility" => Some(ExtractedValue::Integer(self.epi.trust.error_susceptibility as i64)),
            "bond_count" => Some(ExtractedValue::Integer(self.bond_count() as i64)),
            "epistemic_status" => {
                let name = match self.epi.trust.epistemic_status as u8 {
                    0x00 => "Rumor",
                    0x01 => "Hearsay",
                    0x02 => "Testimony",
                    0x03 => "Observation",
                    0x04 => "Hypothesis",
                    0x05 => "Evidence",
                    0x06 => "Corroborated",
                    0x07 => "PeerReviewed",
                    0x08 => "Consensus",
                    0x09 => "FormallyProven",
                    0x0A => "Axiomatic",
                    _ => "Unknown",
                };
                Some(ExtractedValue::Text(name.to_string()))
            },
            "evidence_type" => Some(ExtractedValue::Integer(self.epi.trust.evidence_type as u8 as i64)),

            // PoMV signals
            "metabolic_rate" => Some(ExtractedValue::Integer(self.epi.trust.metabolic_rate as i64)),
            "prediction_score" => Some(ExtractedValue::Integer(self.epi.trust.prediction_score as i64)),
            "entropy_at_creation" => Some(ExtractedValue::Integer(self.epi.trust.entropy_at_creation as i64)),
            "survival_score" => Some(ExtractedValue::Integer(self.epi.trust.survival_score as i64)),
            "synaptic_centrality" => Some(ExtractedValue::Integer(self.epi.trust.synaptic_centrality as i64)),
            "niche_fitness" => Some(ExtractedValue::Integer(self.epi.trust.niche_fitness as i64)),

            // Expression fields
            "text" => self.expr.as_ref().map(|e| ExtractedValue::Text(e.text.clone())),

            // Existence checks
            "epi" => Some(ExtractedValue::Bool(true)), // always present in v6
            "expression" => Some(ExtractedValue::Bool(self.expr.is_some())),

            // Encoding consensus status
            "encoding_status" => Some(ExtractedValue::Text(self.encoding_status.name().to_string())),

            "cid" => Some(ExtractedValue::Text(self.cid.iter().map(|b| format!("{:02x}", b)).collect::<String>())),

            // Future: "encoding_time_ms" — requires EncodingConsensus data attachment

            _ => None,
        }
    }
}

// ============================================================================
// Extracted value — for KQL field extraction
// ============================================================================

/// A value extracted from a KuRuntime field, used in KQL query evaluation.
#[derive(Debug, Clone, PartialEq)]
pub enum ExtractedValue {
    /// Numeric value (all KU numeric fields fit in i64).
    Integer(i64),
    /// Floating point (for computed values like PoMV composite).
    Float(f64),
    /// Text value (expression text, gene type name, etc.).
    Text(String),
    /// Boolean (existence checks, has_triple, etc.).
    Bool(bool),
    /// Null/missing value.
    Null,
}

// ============================================================================
// Tests
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core_dna::{CoreDna, CoreDnaHeader, Instruction, NumericValue};

    fn make_test_dna() -> CoreDna {
        CoreDna {
            header: CoreDnaHeader {
                version: 1,
                gene_type: 0, // Fact
                has_concept_table: false,
            },
            concept_table: Vec::new(),
            instructions: vec![
                Instruction::Triple { s: 301, p: 500, o: 1042 },
                Instruction::Located { s: 301, location: 600 },
                Instruction::Certainty { level: 9000 },
            ],
        }
    }

    #[test]
    fn test_from_dna() {
        let dna = make_test_dna();
        let runtime = KuRuntime::from_dna(dna).unwrap();
        assert_eq!(runtime.gene_type(), 0);
        assert!(!runtime.wire_bytes.is_empty());
        assert_eq!(runtime.cid.len(), 32);
    }

    #[test]
    fn test_roundtrip_wire() {
        let dna = make_test_dna();
        let runtime1 = KuRuntime::from_dna(dna).unwrap();
        let runtime2 = KuRuntime::from_wire(runtime1.wire_bytes.clone()).unwrap();
        assert_eq!(runtime1.cid, runtime2.cid);
        assert_eq!(runtime1.dna, runtime2.dna);
    }

    #[test]
    fn test_concept_ids() {
        let dna = make_test_dna();
        let runtime = KuRuntime::from_dna(dna).unwrap();
        let ids = runtime.concept_ids();
        assert!(ids.contains(&301));
        assert!(ids.contains(&500));
        assert!(ids.contains(&1042));
        assert!(ids.contains(&600));
    }

    #[test]
    fn test_primary_concept() {
        let dna = make_test_dna();
        let runtime = KuRuntime::from_dna(dna).unwrap();
        assert_eq!(runtime.primary_concept(), Some(301));
    }

    #[test]
    fn test_certainty() {
        let dna = make_test_dna();
        let runtime = KuRuntime::from_dna(dna).unwrap();
        assert_eq!(runtime.certainty(), Some(9000));
    }

    #[test]
    fn test_instruction_count() {
        let dna = make_test_dna();
        let runtime = KuRuntime::from_dna(dna).unwrap();
        assert_eq!(runtime.instruction_count(), 3);
    }

    #[test]
    fn test_has_triple() {
        let dna = make_test_dna();
        let runtime = KuRuntime::from_dna(dna).unwrap();
        assert!(runtime.has_triple());
        assert!(!runtime.has_step());
    }

    #[test]
    fn test_contains_concept() {
        let dna = make_test_dna();
        let runtime = KuRuntime::from_dna(dna).unwrap();
        assert!(runtime.contains_concept(301));
        assert!(!runtime.contains_concept(999));
    }

    #[test]
    fn test_extract_field_core_dna() {
        let dna = make_test_dna();
        let runtime = KuRuntime::from_dna(dna).unwrap();
        assert_eq!(runtime.extract_field("gene_type"), Some(ExtractedValue::Text("Fact".to_string())));
        assert_eq!(runtime.extract_field("certainty"), Some(ExtractedValue::Integer(9000)));
        assert_eq!(runtime.extract_field("instruction_count"), Some(ExtractedValue::Integer(3)));
        assert_eq!(runtime.extract_field("has_triple"), Some(ExtractedValue::Bool(true)));
        assert_eq!(runtime.extract_field("primary_concept"), Some(ExtractedValue::Integer(301)));
    }

    #[test]
    fn test_extract_field_epigenetics() {
        let dna = make_test_dna();
        let mut runtime = KuRuntime::from_dna(dna).unwrap();
        runtime.epi.trust.trust_score = 7500;
        runtime.epi.trust.confidence = 8000;
        assert_eq!(runtime.extract_field("trust_score"), Some(ExtractedValue::Integer(7500)));
        assert_eq!(runtime.extract_field("confidence"), Some(ExtractedValue::Integer(8000)));
    }

    #[test]
    fn test_extract_field_unknown() {
        let dna = make_test_dna();
        let runtime = KuRuntime::from_dna(dna).unwrap();
        assert_eq!(runtime.extract_field("nonexistent"), None);
    }

    #[test]
    fn test_with_epigenetics() {
        let dna = make_test_dna();
        let epi = Epigenetics::with_trust(9000, 9500);
        let runtime = KuRuntime::from_dna(dna).unwrap().with_epigenetics(epi);
        assert_eq!(runtime.trust_score(), 9000);
        assert_eq!(runtime.confidence(), 9500);
    }

    // ====================================================================
    // Expression rendering tests (Layer 3)
    // ====================================================================

    use crate::concept_dict::{ConceptDict, ConceptEntry};
    use crate::core_dna::ConstraintOp;

    /// Build a ConceptDict with known entries for expression tests.
    fn make_test_dict() -> ConceptDict {
        ConceptDict::with_entries(vec![
            ConceptEntry { id: 301, name: "water".into(), name_vi: Some("nước".into()), name_en: Some("water".into()), tier: 1, category: None },
            ConceptEntry { id: 500, name: "boils_at".into(), name_vi: Some("sôi_ở".into()), name_en: Some("boils_at".into()), tier: 1, category: None },
            ConceptEntry { id: 600, name: "laboratory".into(), name_vi: Some("phòng_thí_nghiệm".into()), name_en: Some("laboratory".into()), tier: 1, category: None },
            ConceptEntry { id: 1042, name: "100_celsius".into(), name_vi: Some("100_độ_C".into()), name_en: Some("100_celsius".into()), tier: 1, category: None },
            ConceptEntry { id: 200, name: "heat".into(), name_vi: Some("nhiệt".into()), name_en: Some("heat".into()), tier: 1, category: None },
            ConceptEntry { id: 201, name: "ignite".into(), name_vi: Some("đốt".into()), name_en: Some("ignite".into()), tier: 1, category: None },
            ConceptEntry { id: 202, name: "burner".into(), name_vi: Some("bếp".into()), name_en: Some("burner".into()), tier: 1, category: None },
            ConceptEntry { id: 203, name: "boiling".into(), name_vi: Some("sôi".into()), name_en: Some("boiling".into()), tier: 1, category: None },
            ConceptEntry { id: 204, name: "fuel".into(), name_vi: None, name_en: Some("fuel".into()), tier: 1, category: None },
            ConceptEntry { id: 205, name: "celsius".into(), name_vi: None, name_en: Some("celsius".into()), tier: 1, category: None },
            ConceptEntry { id: 206, name: "temperature".into(), name_vi: Some("nhiệt_độ".into()), name_en: Some("temperature".into()), tier: 1, category: None },
            ConceptEntry { id: 207, name: "pressure".into(), name_vi: Some("áp_suất".into()), name_en: Some("pressure".into()), tier: 1, category: None },
            ConceptEntry { id: 208, name: "max_temp".into(), name_vi: None, name_en: Some("max_temp".into()), tier: 1, category: None },
            ConceptEntry { id: 209, name: "molecule".into(), name_vi: None, name_en: None, tier: 1, category: None },
            ConceptEntry { id: 210, name: "atom".into(), name_vi: None, name_en: None, tier: 1, category: None },
            ConceptEntry { id: 211, name: "red".into(), name_vi: None, name_en: None, tier: 1, category: None },
            ConceptEntry { id: 212, name: "blue".into(), name_vi: None, name_en: None, tier: 1, category: None },
            ConceptEntry { id: 213, name: "green".into(), name_vi: None, name_en: None, tier: 1, category: None },
            ConceptEntry { id: 214, name: "color".into(), name_vi: None, name_en: None, tier: 1, category: None },
            ConceptEntry { id: 215, name: "source".into(), name_vi: None, name_en: None, tier: 1, category: None },
            ConceptEntry { id: 216, name: "domain".into(), name_vi: None, name_en: None, tier: 1, category: None },
            ConceptEntry { id: 217, name: "model_x".into(), name_vi: None, name_en: None, tier: 1, category: None },
            ConceptEntry { id: 218, name: "rain".into(), name_vi: None, name_en: None, tier: 1, category: None },
            ConceptEntry { id: 219, name: "flood".into(), name_vi: None, name_en: None, tier: 1, category: None },
            ConceptEntry { id: 220, name: "student".into(), name_vi: None, name_en: None, tier: 1, category: None },
            ConceptEntry { id: 221, name: "study".into(), name_vi: None, name_en: None, tier: 1, category: None },
            ConceptEntry { id: 222, name: "cut".into(), name_vi: None, name_en: None, tier: 1, category: None },
            ConceptEntry { id: 223, name: "knife".into(), name_vi: None, name_en: None, tier: 1, category: None },
            ConceptEntry { id: 300, name: "yesterday".into(), name_vi: Some("hôm_qua".into()), name_en: Some("yesterday".into()), tier: 1, category: None },
        ])
    }

    #[test]
    fn test_expression_basic_rendering() {
        // make_test_dna has: Triple(301,500,1042), Located(301,600), Certainty(9000)
        let dna = make_test_dna();
        let dict = make_test_dict();
        let mut runtime = KuRuntime::from_dna(dna).unwrap();

        let expr = runtime.expression("en", &dict);
        assert_eq!(expr.lang, "en");
        assert!(expr.text.contains("water boils_at 100_celsius"));
        assert!(expr.text.contains("water @ laboratory"));
        assert!(expr.text.contains("Certainty: 90%"));

        // Parts are joined with "; "
        let parts: Vec<&str> = expr.text.split("; ").collect();
        assert_eq!(parts.len(), 3);
    }

    #[test]
    fn test_expression_multilingual_vi() {
        let dna = make_test_dna();
        let dict = make_test_dict();
        let mut runtime = KuRuntime::from_dna(dna).unwrap();

        let expr = runtime.expression("vi", &dict);
        assert_eq!(expr.lang, "vi");
        assert!(expr.text.contains("nước sôi_ở 100_độ_C"));
        assert!(expr.text.contains("nước @ phòng_thí_nghiệm"));
    }

    #[test]
    fn test_expression_lazy_caching() {
        let dna = make_test_dna();
        let dict = make_test_dict();
        let mut runtime = KuRuntime::from_dna(dna).unwrap();

        // First call generates
        assert!(runtime.expr.is_none());
        let _expr = runtime.expression("en", &dict);
        assert!(runtime.expr.is_some());

        // Second call with same lang returns cached (verify content matches)
        let text1 = runtime.expression("en", &dict).text.clone();
        let text2 = runtime.expression("en", &dict).text.clone();
        assert_eq!(text1, text2);
    }

    #[test]
    fn test_expression_lang_switch_regenerates() {
        let dna = make_test_dna();
        let dict = make_test_dict();
        let mut runtime = KuRuntime::from_dna(dna).unwrap();

        let en_text = runtime.expression("en", &dict).text.clone();
        let vi_text = runtime.expression("vi", &dict).text.clone();
        // Should be different languages
        assert_ne!(en_text, vi_text);
        assert_eq!(runtime.expr.as_ref().unwrap().lang, "vi");
    }

    #[test]
    fn test_expression_concept_names_collected() {
        let dna = make_test_dna();
        let dict = make_test_dict();
        let mut runtime = KuRuntime::from_dna(dna).unwrap();

        let expr = runtime.expression("en", &dict);
        // Should contain all concept IDs from the instructions
        let ids: Vec<u64> = expr.concept_names.iter().map(|(id, _)| *id).collect();
        assert!(ids.contains(&301));  // water
        assert!(ids.contains(&500));  // boils_at
        assert!(ids.contains(&1042)); // 100_celsius
        assert!(ids.contains(&600));  // laboratory
    }

    #[test]
    fn test_expression_procedure_gene_type() {
        let dna = CoreDna {
            header: CoreDnaHeader { version: 2, gene_type: 3, has_concept_table: false },
            concept_table: Vec::new(),
            instructions: vec![
                Instruction::Precond { concept: 204 },       // fuel
                Instruction::Step { ord: 1, action: 201, target: 202 }, // ignite burner
                Instruction::Step { ord: 2, action: 200, target: 301 }, // heat water
                Instruction::Effect { concept: 203 },        // boiling
            ],
        };
        let dict = make_test_dict();
        let mut runtime = KuRuntime::from_dna(dna).unwrap();
        let expr = runtime.expression("en", &dict);

        assert!(expr.text.contains("Requires: fuel"));
        assert!(expr.text.contains("Step 1: ignite burner"));
        assert!(expr.text.contains("Step 2: heat water"));
        assert!(expr.text.contains("Effect: boiling"));
    }

    #[test]
    fn test_expression_quantity_rendering() {
        let dna = CoreDna {
            header: CoreDnaHeader { version: 2, gene_type: 0, has_concept_table: false },
            concept_table: Vec::new(),
            instructions: vec![
                Instruction::Quantity {
                    s: 206, // temperature
                    value: NumericValue::U16(100),
                    unit: 205, // celsius
                },
            ],
        };
        let dict = make_test_dict();
        let mut runtime = KuRuntime::from_dna(dna).unwrap();
        let expr = runtime.expression("en", &dict);
        assert_eq!(expr.text, "temperature = 100 celsius");
    }

    #[test]
    fn test_expression_tolerance_rendering() {
        let dna = CoreDna {
            header: CoreDnaHeader { version: 2, gene_type: 0, has_concept_table: false },
            concept_table: Vec::new(),
            instructions: vec![
                Instruction::Tolerance {
                    s: 206,
                    value: NumericValue::F32(99.5),
                    delta: NumericValue::F32(0.5),
                },
            ],
        };
        let dict = make_test_dict();
        let mut runtime = KuRuntime::from_dna(dna).unwrap();
        let expr = runtime.expression("en", &dict);
        assert!(expr.text.contains("temperature = 99.5 ± 0.5"));
    }

    #[test]
    fn test_expression_range_rendering() {
        let dna = CoreDna {
            header: CoreDnaHeader { version: 2, gene_type: 0, has_concept_table: false },
            concept_table: Vec::new(),
            instructions: vec![
                Instruction::Range {
                    s: 207, // pressure
                    min: NumericValue::U16(0),
                    max: NumericValue::U16(1000),
                },
            ],
        };
        let dict = make_test_dict();
        let mut runtime = KuRuntime::from_dna(dna).unwrap();
        let expr = runtime.expression("en", &dict);
        assert_eq!(expr.text, "pressure ∈ [0, 1000]");
    }

    #[test]
    fn test_expression_constraint_rendering() {
        let dna = CoreDna {
            header: CoreDnaHeader { version: 2, gene_type: 4, has_concept_table: false },
            concept_table: Vec::new(),
            instructions: vec![
                Instruction::Constraint {
                    source: 206, // temperature
                    op: ConstraintOp::Le,
                    target: 208, // max_temp
                },
            ],
        };
        let dict = make_test_dict();
        let mut runtime = KuRuntime::from_dna(dna).unwrap();
        let expr = runtime.expression("en", &dict);
        assert_eq!(expr.text, "temperature <= max_temp");
    }

    #[test]
    fn test_expression_partof_rendering() {
        let dna = CoreDna {
            header: CoreDnaHeader { version: 2, gene_type: 6, has_concept_table: false },
            concept_table: Vec::new(),
            instructions: vec![
                Instruction::PartOf { part: 210, whole: 209 }, // atom ⊂ molecule
            ],
        };
        let dict = make_test_dict();
        let mut runtime = KuRuntime::from_dna(dna).unwrap();
        let expr = runtime.expression("en", &dict);
        assert_eq!(expr.text, "atom ⊂ molecule");
    }

    #[test]
    fn test_expression_causal_rendering() {
        let dna = CoreDna {
            header: CoreDnaHeader { version: 2, gene_type: 0, has_concept_table: false },
            concept_table: Vec::new(),
            instructions: vec![
                Instruction::Causal { cause: 200, effect: 203 }, // heat → boiling
            ],
        };
        let dict = make_test_dict();
        let mut runtime = KuRuntime::from_dna(dna).unwrap();
        let expr = runtime.expression("en", &dict);
        assert_eq!(expr.text, "heat → boiling");
    }

    #[test]
    fn test_expression_temporal_rendering() {
        let dna = CoreDna {
            header: CoreDnaHeader { version: 2, gene_type: 2, has_concept_table: false },
            concept_table: Vec::new(),
            instructions: vec![
                Instruction::Temporal { s: 203, time: 300 }, // boiling → yesterday
            ],
        };
        let dict = make_test_dict();
        let mut runtime = KuRuntime::from_dna(dna).unwrap();
        let expr = runtime.expression("en", &dict);
        assert_eq!(expr.text, "boiling → yesterday");
    }

    #[test]
    fn test_expression_quality_rendering() {
        let dna = CoreDna {
            header: CoreDnaHeader { version: 2, gene_type: 5, has_concept_table: false },
            concept_table: Vec::new(),
            instructions: vec![
                Instruction::Quality { s: 301, q: 211 }, // water: red
            ],
        };
        let dict = make_test_dict();
        let mut runtime = KuRuntime::from_dna(dna).unwrap();
        let expr = runtime.expression("en", &dict);
        assert_eq!(expr.text, "water: red");
    }

    #[test]
    fn test_expression_remaining_instruction_types() {
        let dna = CoreDna {
            header: CoreDnaHeader { version: 2, gene_type: 0, has_concept_table: false },
            concept_table: Vec::new(),
            instructions: vec![
                Instruction::Simulates { s: 301, model: 217 },
                Instruction::Condition { cond: 218, result: 219 },
                Instruction::Agent { actor: 220, action: 221 },
                Instruction::Tool { action: 222, instrument: 223 },
                Instruction::Sequence { items: vec![211, 212, 213] },
                Instruction::EnumVal { s: 214, values: vec![211, 212, 213] },
                Instruction::Label { key: 215, value: 216 },
                Instruction::Difficulty { level: 3 },
                Instruction::Affect { v: 500, a: -200, d: 100 },
                Instruction::Witness { count: 5, proximity: 2 },
            ],
        };
        let dict = make_test_dict();
        let mut runtime = KuRuntime::from_dna(dna).unwrap();
        let expr = runtime.expression("en", &dict);
        let text = &expr.text;

        assert!(text.contains("water ~ model_x"), "Simulates: {}", text);
        assert!(text.contains("IF rain THEN flood"), "Condition: {}", text);
        assert!(text.contains("student → study"), "Agent: {}", text);
        assert!(text.contains("cut USING knife"), "Tool: {}", text);
        assert!(text.contains("[red, blue, green]"), "Sequence: {}", text);
        assert!(text.contains("color ∈ {red, blue, green}"), "EnumVal: {}", text);
        assert!(text.contains("source: domain"), "Label: {}", text);
        assert!(text.contains("Difficulty: 3"), "Difficulty: {}", text);
        assert!(text.contains("Affect(V=500, A=-200, D=100)"), "Affect: {}", text);
        assert!(text.contains("Witness(count=5, proximity=2)"), "Witness: {}", text);
    }

    #[test]
    fn test_expression_unknown_concepts_show_question_mark() {
        let dna = CoreDna {
            header: CoreDnaHeader { version: 2, gene_type: 0, has_concept_table: false },
            concept_table: Vec::new(),
            instructions: vec![
                Instruction::Triple { s: 99999, p: 99998, o: 99997 },
            ],
        };
        let dict = make_test_dict(); // no entries for 99999/99998/99997
        let mut runtime = KuRuntime::from_dna(dna).unwrap();
        let expr = runtime.expression("en", &dict);
        assert_eq!(expr.text, "? ? ?");
    }

    #[test]
    fn test_expression_empty_instructions() {
        let dna = CoreDna {
            header: CoreDnaHeader { version: 2, gene_type: 0, has_concept_table: false },
            concept_table: Vec::new(),
            instructions: vec![],
        };
        let dict = make_test_dict();
        let mut runtime = KuRuntime::from_dna(dna).unwrap();
        let expr = runtime.expression("en", &dict);
        assert_eq!(expr.text, "");
        assert!(expr.concept_names.is_empty());
    }

    // ====================================================================
    // PoMV Bridge tests
    // ====================================================================

    #[test]
    fn test_apply_pomv_update_copies_all_signals() {
        use crate::types::EpistemicStatus;

        let dna = make_test_dna();
        let mut rt = KuRuntime::from_dna(dna).unwrap();

        let update = TrustSectionUpdate {
            epistemic_status: EpistemicStatus::Testimony,
            metabolic_rate: 5000,
            prediction_score: 7000,
            entropy_at_creation: 3000,
            survival_score: 1000,
            synaptic_centrality: 4000,
            niche_fitness: 6000,
            pomv_total: 0.65,
        };

        rt.apply_pomv_update(&update);

        assert_eq!(rt.epi.trust.metabolic_rate, 5000);
        assert_eq!(rt.epi.trust.prediction_score, 7000);
        assert_eq!(rt.epi.trust.entropy_at_creation, 3000);
        assert_eq!(rt.epi.trust.survival_score, 1000);
        assert_eq!(rt.epi.trust.synaptic_centrality, 4000);
        assert_eq!(rt.epi.trust.niche_fitness, 6000);
    }

    #[test]
    fn test_apply_pomv_update_syncs_epistemic_status() {
        use crate::types::EpistemicStatus;

        let dna = make_test_dna();
        let mut rt = KuRuntime::from_dna(dna).unwrap();
        assert_eq!(rt.epi.trust.epistemic_status, EpistemicStatus::Rumor);

        let update = TrustSectionUpdate {
            epistemic_status: EpistemicStatus::Evidence,
            metabolic_rate: 0,
            prediction_score: 0,
            entropy_at_creation: 0,
            survival_score: 0,
            synaptic_centrality: 0,
            niche_fitness: 0,
            pomv_total: 0.0,
        };

        rt.apply_pomv_update(&update);

        // Single source of truth in trust
        assert_eq!(rt.epi.trust.epistemic_status, EpistemicStatus::Evidence);
    }

    #[test]
    fn test_cid_bytes_returns_identity_key() {
        let dna = make_test_dna();
        let rt = KuRuntime::from_dna(dna).unwrap();
        let bytes = rt.cid_bytes();
        assert_eq!(bytes, rt.cid);
        assert_eq!(bytes.len(), 32);
        // Non-zero (BLAKE3 of actual wire bytes)
        assert!(bytes.iter().any(|&b| b != 0));
    }

    #[test]
    fn test_cid_bytes_usable_as_pomv_key() {
        use crate::pomv_runtime::{PomvRuntime, PomvConfig};

        let dna = make_test_dna();
        let rt = KuRuntime::from_dna(dna).unwrap();
        let key = rt.cid_bytes();

        // Register in PomvRuntime using the same key
        let mut pomv = PomvRuntime::new(PomvConfig::default());
        pomv.register_ku(key, 1_000_000, vec![], 0.5, 0.5);

        assert!(pomv.ku_states.contains_key(&key));
    }
}

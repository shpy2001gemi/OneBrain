//! Core DNA v6 — Ultra-compact binary knowledge encoding.
//!
//! Replaces CBOR with a custom binary instruction stream that is
//! **smaller than natural language text** while remaining language-agnostic.
//!
//! # Wire Format
//! ```text
//! MAGIC(0x4B) | VER_META(1B) | INSTRUCTION_STREAM | END(0x1E) | CRC-16(2B)
//! ```
//!
//! # Biological Analogy
//! - Core DNA = nucleotide sequence (compact, immutable, stored)
//! - Epigenetics = histone modifications (runtime-only, not persisted)
//! - Expression = protein synthesis (generated on-demand from DNA)

use crate::error::KuError;
use crate::varint::{encode_varint, decode_varint};
use crate::types::ConceptId;
use std::fmt;

// ============================================================================
// Constants
// ============================================================================

/// Core DNA magic byte ('K' = 0x4B). Single byte, NOT the 2-byte v4/v5 0x4B44.
pub const CORE_DNA_MAGIC: u8 = 0x4B;

/// Core DNA format version (3 bits, stored in VER_META byte bits 7-5).
pub const CORE_DNA_VERSION: u8 = 1;

// ============================================================================
// Numeric literal prefixes (0xFA-0xFF, outside varint range)
// ============================================================================

const NUM_U8:  u8 = 0xFA;
const NUM_U16: u8 = 0xFB;
const NUM_I16: u8 = 0xFC;
const NUM_U32: u8 = 0xFD;
const NUM_I32: u8 = 0xFE;
const NUM_F32: u8 = 0xFF;

// ============================================================================
// Opcodes (5-bit, values 0x00-0x1F)
// ============================================================================

/// Instruction opcodes for the Core DNA format.
/// Each opcode occupies 5 bits in the OPCODE byte, with 3 modifier bits.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum Op {
    /// `TRIPLE(S, P, O)` — basic S-P-O fact.
    Triple      = 0x00,
    /// `QUALITY(S, Q)` — subject has quality.
    Quality     = 0x01,
    /// `QUANTITY(S, value, unit)` — numeric measurement.
    Quantity    = 0x02,
    /// `SEQUENCE(N, items...)` — ordered list of concepts.
    Sequence    = 0x03,
    /// `PART_OF(part, whole)` — hierarchical containment.
    PartOf      = 0x04,
    /// `LOCATED(S, location)` — spatial relation.
    Located     = 0x05,
    /// `TEMPORAL(S, time)` — time relation.
    Temporal    = 0x06,
    /// `CAUSAL(cause, effect)` — causation.
    Causal      = 0x07,
    /// `SIMULATES(S, model)` — analogy/simulation.
    Simulates   = 0x08,
    /// `CONDITION(if, then)` — conditional.
    Condition   = 0x09,
    /// `AGENT(actor, action)` — who performs.
    Agent       = 0x0A,
    /// `TOOL(action, instrument)` — using what.
    Tool        = 0x0B,
    /// `RANGE(S, min, max)` — value range.
    Range       = 0x0C,
    /// `TOLERANCE(S, value, ±delta)` — precision with error margin.
    Tolerance   = 0x0D,
    /// `CONSTRAINT(source, op_code, target)` — numeric constraint (≤, ≥, =, ≠).
    Constraint  = 0x0E,
    /// `ENUM_VAL(S, N, values...)` — one of a set.
    EnumVal     = 0x0F,
    /// `CERTAINTY(level_u16)` — confidence 0-10000.
    Certainty   = 0x10,
    /// `DIFFICULTY(level_u8)` — 0-4 difficulty.
    Difficulty  = 0x11,
    /// `CID_REF(32 bytes)` — BLAKE3 content reference.
    CidRef      = 0x12,
    /// `STEP(ord, action, target)` — procedure step.
    Step        = 0x13,
    /// `PRECOND(concept)` — step precondition.
    Precond     = 0x14,
    /// `EFFECT(concept)` — step effect/result.
    Effect      = 0x15,
    /// `AFFECT(V_i16, A_i16, D_i16)` — VAD emotion model.
    Affect      = 0x16,
    /// `LABEL(key, value)` — generic key-value metadata.
    Label       = 0x17,
    /// `TEXT_REF(lang, len, bytes)` — compressed canonical text.
    TextRef     = 0x18,
    /// `FORMULA(format, len, bytes)` — LaTeX/MathML notation.
    Formula     = 0x19,
    /// `WITNESS(count, proximity)` — testimony data.
    Witness     = 0x1A,
    /// `MEDIA_REF(system, len, id_bytes)` — external media reference.
    MediaRef    = 0x1B,
    /// `COMPOSITE_HDR(type, completeness, version)` — composite header.
    CompositeHdr = 0x1C,
    /// `MEMBER(order, role, required, label, cid)` — composite member entry.
    Member      = 0x1D,
    /// `END` — terminates instruction stream.
    End         = 0x1E,
    /// `EXTENDED(ext_byte, ...)` — future extension.
    Extended    = 0x1F,
}

impl Op {
    pub fn from_u8(v: u8) -> Option<Self> {
        match v {
            0x00 => Some(Self::Triple),
            0x01 => Some(Self::Quality),
            0x02 => Some(Self::Quantity),
            0x03 => Some(Self::Sequence),
            0x04 => Some(Self::PartOf),
            0x05 => Some(Self::Located),
            0x06 => Some(Self::Temporal),
            0x07 => Some(Self::Causal),
            0x08 => Some(Self::Simulates),
            0x09 => Some(Self::Condition),
            0x0A => Some(Self::Agent),
            0x0B => Some(Self::Tool),
            0x0C => Some(Self::Range),
            0x0D => Some(Self::Tolerance),
            0x0E => Some(Self::Constraint),
            0x0F => Some(Self::EnumVal),
            0x10 => Some(Self::Certainty),
            0x11 => Some(Self::Difficulty),
            0x12 => Some(Self::CidRef),
            0x13 => Some(Self::Step),
            0x14 => Some(Self::Precond),
            0x15 => Some(Self::Effect),
            0x16 => Some(Self::Affect),
            0x17 => Some(Self::Label),
            0x18 => Some(Self::TextRef),
            0x19 => Some(Self::Formula),
            0x1A => Some(Self::Witness),
            0x1B => Some(Self::MediaRef),
            0x1C => Some(Self::CompositeHdr),
            0x1D => Some(Self::Member),
            0x1E => Some(Self::End),
            0x1F => Some(Self::Extended),
            _ => None,
        }
    }
}

// ============================================================================
// Numeric Value — inline numeric literals
// ============================================================================

/// Numeric value that can be encoded inline in the instruction stream.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum NumericValue {
    U8(u8),
    U16(u16),
    I16(i16),
    U32(u32),
    I32(i32),
    F32(f32),
}

impl NumericValue {
    /// Encode to bytes with type prefix.
    pub fn encode(&self) -> Vec<u8> {
        match self {
            Self::U8(v)  => vec![NUM_U8, *v],
            Self::U16(v) => { let mut out = vec![NUM_U16]; out.extend_from_slice(&v.to_be_bytes()); out },
            Self::I16(v) => { let mut out = vec![NUM_I16]; out.extend_from_slice(&v.to_be_bytes()); out },
            Self::U32(v) => { let mut out = vec![NUM_U32]; out.extend_from_slice(&v.to_be_bytes()); out },
            Self::I32(v) => { let mut out = vec![NUM_I32]; out.extend_from_slice(&v.to_be_bytes()); out },
            Self::F32(v) => { let mut out = vec![NUM_F32]; out.extend_from_slice(&v.to_be_bytes()); out },
        }
    }

    /// Decode from bytes at cursor position. Returns (value, bytes_consumed).
    pub fn decode(data: &[u8], pos: usize) -> Result<(Self, usize), KuError> {
        if pos >= data.len() {
            return Err(KuError::InvalidData("Unexpected end reading numeric prefix".into()));
        }
        match data[pos] {
            NUM_U8 => {
                if pos + 1 >= data.len() { return Err(KuError::InvalidData("Truncated u8".into())); }
                Ok((Self::U8(data[pos + 1]), 2))
            }
            NUM_U16 => {
                if pos + 2 >= data.len() { return Err(KuError::InvalidData("Truncated u16".into())); }
                let v = u16::from_be_bytes([data[pos + 1], data[pos + 2]]);
                Ok((Self::U16(v), 3))
            }
            NUM_I16 => {
                if pos + 2 >= data.len() { return Err(KuError::InvalidData("Truncated i16".into())); }
                let v = i16::from_be_bytes([data[pos + 1], data[pos + 2]]);
                Ok((Self::I16(v), 3))
            }
            NUM_U32 => {
                if pos + 4 >= data.len() { return Err(KuError::InvalidData("Truncated u32".into())); }
                let v = u32::from_be_bytes([data[pos + 1], data[pos + 2], data[pos + 3], data[pos + 4]]);
                Ok((Self::U32(v), 5))
            }
            NUM_I32 => {
                if pos + 4 >= data.len() { return Err(KuError::InvalidData("Truncated i32".into())); }
                let v = i32::from_be_bytes([data[pos + 1], data[pos + 2], data[pos + 3], data[pos + 4]]);
                Ok((Self::I32(v), 5))
            }
            NUM_F32 => {
                if pos + 4 >= data.len() { return Err(KuError::InvalidData("Truncated f32".into())); }
                let v = f32::from_be_bytes([data[pos + 1], data[pos + 2], data[pos + 3], data[pos + 4]]);
                Ok((Self::F32(v), 5))
            }
            other => Err(KuError::InvalidData(format!("Invalid numeric prefix: 0x{:02X}", other))),
        }
    }

    /// Get the f64 representation for comparison/display.
    pub fn as_f64(&self) -> f64 {
        match self {
            Self::U8(v)  => *v as f64,
            Self::U16(v) => *v as f64,
            Self::I16(v) => *v as f64,
            Self::U32(v) => *v as f64,
            Self::I32(v) => *v as f64,
            Self::F32(v) => *v as f64,
        }
    }
}

impl fmt::Display for NumericValue {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::U8(v)  => write!(f, "{}", v),
            Self::U16(v) => write!(f, "{}", v),
            Self::I16(v) => write!(f, "{}", v),
            Self::U32(v) => write!(f, "{}", v),
            Self::I32(v) => write!(f, "{}", v),
            Self::F32(v) => {
                // Display integers without decimal point
                if v.fract() == 0.0 && v.is_finite() {
                    write!(f, "{}", *v as i64)
                } else {
                    write!(f, "{}", v)
                }
            }
        }
    }
}

// ============================================================================
// Constraint operators
// ============================================================================

/// Constraint comparison operator (stored as u8).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum ConstraintOp {
    Eq     = 0, // ==
    Ne     = 1, // !=
    Lt     = 2, // <
    Le     = 3, // <=
    Gt     = 4, // >
    Ge     = 5, // >=
}

impl ConstraintOp {
    pub fn from_u8(v: u8) -> Option<Self> {
        match v {
            0 => Some(Self::Eq),
            1 => Some(Self::Ne),
            2 => Some(Self::Lt),
            3 => Some(Self::Le),
            4 => Some(Self::Gt),
            5 => Some(Self::Ge),
            _ => None,
        }
    }
}

impl fmt::Display for ConstraintOp {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let sym = match self {
            Self::Eq => "==",
            Self::Ne => "!=",
            Self::Lt => "<",
            Self::Le => "<=",
            Self::Gt => ">",
            Self::Ge => ">=",
        };
        f.write_str(sym)
    }
}

// ============================================================================
// Instruction — typed instruction variants
// ============================================================================

/// A single instruction in the Core DNA stream.
#[derive(Debug, Clone, PartialEq)]
pub enum Instruction {
    /// `(S, P, O)` — subject-predicate-object triple.
    Triple { s: ConceptId, p: ConceptId, o: ConceptId },
    /// `(S, Q)` — subject has quality Q.
    Quality { s: ConceptId, q: ConceptId },
    /// `(S, value, unit)` — subject has numeric value with unit.
    Quantity { s: ConceptId, value: NumericValue, unit: ConceptId },
    /// Ordered sequence of concept IDs.
    Sequence { items: Vec<ConceptId> },
    /// `(part, whole)` — part belongs to whole.
    PartOf { part: ConceptId, whole: ConceptId },
    /// `(S, location)` — subject located at.
    Located { s: ConceptId, location: ConceptId },
    /// `(S, time)` — temporal relation.
    Temporal { s: ConceptId, time: ConceptId },
    /// `(cause, effect)` — causation.
    Causal { cause: ConceptId, effect: ConceptId },
    /// `(S, model)` — S simulates/mimics model.
    Simulates { s: ConceptId, model: ConceptId },
    /// `(condition, result)` — conditional.
    Condition { cond: ConceptId, result: ConceptId },
    /// `(actor, action)` — agent performs action.
    Agent { actor: ConceptId, action: ConceptId },
    /// `(action, instrument)` — action uses tool.
    Tool { action: ConceptId, instrument: ConceptId },
    /// `(S, min, max)` — value range.
    Range { s: ConceptId, min: NumericValue, max: NumericValue },
    /// `(S, value, delta)` — value ± tolerance.
    Tolerance { s: ConceptId, value: NumericValue, delta: NumericValue },
    /// `(source, op, target)` — numeric constraint.
    Constraint { source: ConceptId, op: ConstraintOp, target: ConceptId },
    /// `(S, values...)` — one of set.
    EnumVal { s: ConceptId, values: Vec<ConceptId> },
    /// Certainty level 0-10000.
    Certainty { level: u16 },
    /// Difficulty 0-4.
    Difficulty { level: u8 },
    /// 32-byte BLAKE3 content ID reference.
    CidRef { cid: [u8; 32] },
    /// Procedure step: `(order, action_concept, target_concept)`.
    Step { ord: u8, action: ConceptId, target: ConceptId },
    /// Step precondition concept.
    Precond { concept: ConceptId },
    /// Step effect/result concept.
    Effect { concept: ConceptId },
    /// VAD affect model (Valence, Arousal, Dominance).
    Affect { v: i16, a: i16, d: i16 },
    /// Generic label `(key, value)`.
    Label { key: ConceptId, value: ConceptId },
    /// Canonical text reference (compressed).
    TextRef { lang: u8, data: Vec<u8> },
    /// Formula notation (LaTeX/MathML).
    Formula { format: u8, data: Vec<u8> },
    /// Witness/testimony data.
    Witness { count: u16, proximity: u8 },
    /// External media reference.
    MediaRef { system: u8, id: Vec<u8> },
    /// Composite header metadata.
    CompositeHdr { composite_type: u8, completeness: u8, version: u32 },
    /// Composite member entry.
    Member { order: u16, role: u8, required: bool, label: ConceptId, cid: [u8; 32] },
    /// End of instruction stream.
    End,
}

// ============================================================================
// CoreDna — the complete Core DNA unit
// ============================================================================

/// Core DNA header metadata.
#[derive(Debug, Clone, PartialEq)]
pub struct CoreDnaHeader {
    /// Format version (0-7, current = 1).
    pub version: u8,
    /// Gene type (0-15, maps to GeneType).
    pub gene_type: u8,
    /// Whether any instructions contain qualifiers.
    pub has_qualifiers: bool,
}

/// A complete Core DNA unit — the smallest persistable knowledge unit.
#[derive(Debug, Clone, PartialEq)]
pub struct CoreDna {
    pub header: CoreDnaHeader,
    pub instructions: Vec<Instruction>,
}

// ============================================================================
// CRC-16/CCITT implementation
// ============================================================================

/// Compute CRC-16/CCITT (polynomial 0x1021, init 0xFFFF).
pub fn crc16_ccitt(data: &[u8]) -> u16 {
    let mut crc: u16 = 0xFFFF;
    for &byte in data {
        crc ^= (byte as u16) << 8;
        for _ in 0..8 {
            if crc & 0x8000 != 0 {
                crc = (crc << 1) ^ 0x1021;
            } else {
                crc <<= 1;
            }
        }
    }
    crc
}

// ============================================================================
// Encoder: CoreDna → Vec<u8>
// ============================================================================

/// Encode a CoreDna into compact binary wire format.
pub fn encode_core_dna(dna: &CoreDna) -> Result<Vec<u8>, KuError> {
    let mut buf = Vec::with_capacity(64);

    // Header: MAGIC (1B) + VER_META (1B)
    buf.push(CORE_DNA_MAGIC);

    let ver_meta: u8 =
        ((dna.header.version & 0x07) << 5) |
        ((dna.header.gene_type & 0x0F) << 1) |
        (dna.header.has_qualifiers as u8);
    buf.push(ver_meta);

    // Instruction stream
    for instr in &dna.instructions {
        encode_instruction(&mut buf, instr)?;
    }

    // END marker
    encode_opcode_byte(&mut buf, Op::End, 0);

    // CRC-16 over everything before CRC
    let crc = crc16_ccitt(&buf);
    buf.extend_from_slice(&crc.to_be_bytes());

    Ok(buf)
}

/// Encode an opcode byte: [op:5][modifier:3].
fn encode_opcode_byte(buf: &mut Vec<u8>, op: Op, modifier: u8) {
    buf.push(((op as u8) << 3) | (modifier & 0x07));
}

/// Encode a single instruction into the buffer.
fn encode_instruction(buf: &mut Vec<u8>, instr: &Instruction) -> Result<(), KuError> {
    match instr {
        Instruction::Triple { s, p, o } => {
            encode_opcode_byte(buf, Op::Triple, 0);
            buf.extend(encode_varint(*s)?);
            buf.extend(encode_varint(*p)?);
            buf.extend(encode_varint(*o)?);
        }
        Instruction::Quality { s, q } => {
            encode_opcode_byte(buf, Op::Quality, 0);
            buf.extend(encode_varint(*s)?);
            buf.extend(encode_varint(*q)?);
        }
        Instruction::Quantity { s, value, unit } => {
            encode_opcode_byte(buf, Op::Quantity, 0);
            buf.extend(encode_varint(*s)?);
            buf.extend(value.encode());
            buf.extend(encode_varint(*unit)?);
        }
        Instruction::Sequence { items } => {
            encode_opcode_byte(buf, Op::Sequence, 0);
            buf.push(items.len() as u8);
            for id in items {
                buf.extend(encode_varint(*id)?);
            }
        }
        Instruction::PartOf { part, whole } => {
            encode_opcode_byte(buf, Op::PartOf, 0);
            buf.extend(encode_varint(*part)?);
            buf.extend(encode_varint(*whole)?);
        }
        Instruction::Located { s, location } => {
            encode_opcode_byte(buf, Op::Located, 0);
            buf.extend(encode_varint(*s)?);
            buf.extend(encode_varint(*location)?);
        }
        Instruction::Temporal { s, time } => {
            encode_opcode_byte(buf, Op::Temporal, 0);
            buf.extend(encode_varint(*s)?);
            buf.extend(encode_varint(*time)?);
        }
        Instruction::Causal { cause, effect } => {
            encode_opcode_byte(buf, Op::Causal, 0);
            buf.extend(encode_varint(*cause)?);
            buf.extend(encode_varint(*effect)?);
        }
        Instruction::Simulates { s, model } => {
            encode_opcode_byte(buf, Op::Simulates, 0);
            buf.extend(encode_varint(*s)?);
            buf.extend(encode_varint(*model)?);
        }
        Instruction::Condition { cond, result } => {
            encode_opcode_byte(buf, Op::Condition, 0);
            buf.extend(encode_varint(*cond)?);
            buf.extend(encode_varint(*result)?);
        }
        Instruction::Agent { actor, action } => {
            encode_opcode_byte(buf, Op::Agent, 0);
            buf.extend(encode_varint(*actor)?);
            buf.extend(encode_varint(*action)?);
        }
        Instruction::Tool { action, instrument } => {
            encode_opcode_byte(buf, Op::Tool, 0);
            buf.extend(encode_varint(*action)?);
            buf.extend(encode_varint(*instrument)?);
        }
        Instruction::Range { s, min, max } => {
            encode_opcode_byte(buf, Op::Range, 0);
            buf.extend(encode_varint(*s)?);
            buf.extend(min.encode());
            buf.extend(max.encode());
        }
        Instruction::Tolerance { s, value, delta } => {
            encode_opcode_byte(buf, Op::Tolerance, 0);
            buf.extend(encode_varint(*s)?);
            buf.extend(value.encode());
            buf.extend(delta.encode());
        }
        Instruction::Constraint { source, op, target } => {
            encode_opcode_byte(buf, Op::Constraint, 0);
            buf.extend(encode_varint(*source)?);
            buf.push(*op as u8);
            buf.extend(encode_varint(*target)?);
        }
        Instruction::EnumVal { s, values } => {
            encode_opcode_byte(buf, Op::EnumVal, 0);
            buf.extend(encode_varint(*s)?);
            buf.push(values.len() as u8);
            for v in values {
                buf.extend(encode_varint(*v)?);
            }
        }
        Instruction::Certainty { level } => {
            encode_opcode_byte(buf, Op::Certainty, 0);
            buf.extend_from_slice(&level.to_be_bytes());
        }
        Instruction::Difficulty { level } => {
            encode_opcode_byte(buf, Op::Difficulty, 0);
            buf.push(*level);
        }
        Instruction::CidRef { cid } => {
            encode_opcode_byte(buf, Op::CidRef, 0);
            buf.extend_from_slice(cid);
        }
        Instruction::Step { ord, action, target } => {
            encode_opcode_byte(buf, Op::Step, 0);
            buf.push(*ord);
            buf.extend(encode_varint(*action)?);
            buf.extend(encode_varint(*target)?);
        }
        Instruction::Precond { concept } => {
            encode_opcode_byte(buf, Op::Precond, 0);
            buf.extend(encode_varint(*concept)?);
        }
        Instruction::Effect { concept } => {
            encode_opcode_byte(buf, Op::Effect, 0);
            buf.extend(encode_varint(*concept)?);
        }
        Instruction::Affect { v, a, d } => {
            encode_opcode_byte(buf, Op::Affect, 0);
            buf.extend_from_slice(&v.to_be_bytes());
            buf.extend_from_slice(&a.to_be_bytes());
            buf.extend_from_slice(&d.to_be_bytes());
        }
        Instruction::Label { key, value } => {
            encode_opcode_byte(buf, Op::Label, 0);
            buf.extend(encode_varint(*key)?);
            buf.extend(encode_varint(*value)?);
        }
        Instruction::TextRef { lang, data } => {
            encode_opcode_byte(buf, Op::TextRef, 0);
            buf.push(*lang);
            let len = data.len() as u16;
            buf.extend_from_slice(&len.to_be_bytes());
            buf.extend_from_slice(data);
        }
        Instruction::Formula { format, data } => {
            encode_opcode_byte(buf, Op::Formula, 0);
            buf.push(*format);
            let len = data.len() as u16;
            buf.extend_from_slice(&len.to_be_bytes());
            buf.extend_from_slice(data);
        }
        Instruction::Witness { count, proximity } => {
            encode_opcode_byte(buf, Op::Witness, 0);
            buf.extend_from_slice(&count.to_be_bytes());
            buf.push(*proximity);
        }
        Instruction::MediaRef { system, id } => {
            encode_opcode_byte(buf, Op::MediaRef, 0);
            buf.push(*system);
            buf.push(id.len() as u8);
            buf.extend_from_slice(id);
        }
        Instruction::CompositeHdr { composite_type, completeness, version } => {
            encode_opcode_byte(buf, Op::CompositeHdr, 0);
            buf.push(*composite_type);
            buf.push(*completeness);
            buf.extend_from_slice(&version.to_be_bytes());
        }
        Instruction::Member { order, role, required, label, cid } => {
            encode_opcode_byte(buf, Op::Member, 0);
            buf.extend_from_slice(&order.to_be_bytes());
            buf.push(*role);
            buf.push(*required as u8);
            buf.extend(encode_varint(*label)?);
            buf.extend_from_slice(cid);
        }
        Instruction::End => {
            // END is written by the top-level encoder, skip here
        }
    }
    Ok(())
}

// ============================================================================
// Decoder: &[u8] → CoreDna
// ============================================================================

/// Decode a Core DNA binary wire into a CoreDna struct.
pub fn decode_core_dna(data: &[u8]) -> Result<CoreDna, KuError> {
    if data.len() < 5 {
        return Err(KuError::PayloadTruncated {
            expected: 5,
            got: data.len(),
        });
    }

    // Verify magic
    if data[0] != CORE_DNA_MAGIC {
        return Err(KuError::InvalidMagic([data[0], 0x00]));
    }

    // Parse VER_META byte
    let ver_meta = data[1];
    let version = (ver_meta >> 5) & 0x07;
    let gene_type = (ver_meta >> 1) & 0x0F;
    let has_qualifiers = (ver_meta & 0x01) != 0;

    // Verify CRC-16 (last 2 bytes)
    let payload_end = data.len() - 2;
    let stored_crc = u16::from_be_bytes([data[payload_end], data[payload_end + 1]]);
    let computed_crc = crc16_ccitt(&data[..payload_end]);
    if stored_crc != computed_crc {
        return Err(KuError::CrcMismatch {
            stored: stored_crc as u32,
            computed: computed_crc as u32,
        });
    }

    // Decode instruction stream (bytes 2..payload_end)
    let mut pos = 2usize;
    let mut instructions = Vec::new();

    while pos < payload_end {
        let opcode_byte = data[pos];
        let op_val = opcode_byte >> 3;
        let _modifier = opcode_byte & 0x07;
        pos += 1;

        let op = Op::from_u8(op_val).ok_or_else(|| {
            KuError::InvalidData(format!("Unknown opcode: 0x{:02X} at pos {}", op_val, pos - 1))
        })?;

        if op == Op::End {
            break;
        }

        let (instr, new_pos) = decode_instruction(op, data, pos, payload_end)?;
        instructions.push(instr);
        pos = new_pos;
    }

    Ok(CoreDna {
        header: CoreDnaHeader { version, gene_type, has_qualifiers },
        instructions,
    })
}

/// Read a varint operand from data at position. Returns (value, new_pos).
fn read_varint(data: &[u8], pos: usize) -> Result<(ConceptId, usize), KuError> {
    let (value, consumed) = decode_varint(&data[pos..])?;
    Ok((value, pos + consumed))
}

/// Read a numeric value (prefixed) or a varint concept ID.
/// If the byte at `pos` is a numeric prefix (0xFA-0xFF), reads a numeric literal.
/// Otherwise reads a varint ConceptId.
fn read_numeric_or_varint(data: &[u8], pos: usize) -> Result<(NumericValue, usize), KuError> {
    if pos >= data.len() {
        return Err(KuError::InvalidData("Unexpected end reading operand".into()));
    }
    if data[pos] >= NUM_U8 {
        let (val, consumed) = NumericValue::decode(data, pos)?;
        Ok((val, pos + consumed))
    } else {
        // It's a varint — wrap as U32 or U16
        let (v, new_pos) = read_varint(data, pos)?;
        if v <= u16::MAX as u64 {
            Ok((NumericValue::U16(v as u16), new_pos))
        } else {
            Ok((NumericValue::U32(v as u32), new_pos))
        }
    }
}

/// Decode a single instruction from data at position. Returns (instruction, new_pos).
fn decode_instruction(op: Op, data: &[u8], pos: usize, end: usize) -> Result<(Instruction, usize), KuError> {
    let mut p = pos;

    macro_rules! read_v {
        () => {{ let (v, np) = read_varint(data, p)?; p = np; v }};
    }
    macro_rules! read_num {
        () => {{ let (v, np) = read_numeric_or_varint(data, p)?; p = np; v }};
    }
    macro_rules! read_u8 {
        () => {{
            if p >= end { return Err(KuError::InvalidData("Truncated u8".into())); }
            let v = data[p]; p += 1; v
        }};
    }
    macro_rules! read_u16 {
        () => {{
            if p + 1 >= end { return Err(KuError::InvalidData("Truncated u16".into())); }
            let v = u16::from_be_bytes([data[p], data[p+1]]); p += 2; v
        }};
    }
    macro_rules! read_i16 {
        () => {{
            if p + 1 >= end { return Err(KuError::InvalidData("Truncated i16".into())); }
            let v = i16::from_be_bytes([data[p], data[p+1]]); p += 2; v
        }};
    }
    macro_rules! read_u32 {
        () => {{
            if p + 3 >= end { return Err(KuError::InvalidData("Truncated u32".into())); }
            let v = u32::from_be_bytes([data[p], data[p+1], data[p+2], data[p+3]]); p += 4; v
        }};
    }
    macro_rules! read_cid {
        () => {{
            if p + 31 >= end { return Err(KuError::InvalidData("Truncated CID".into())); }
            let mut cid = [0u8; 32];
            cid.copy_from_slice(&data[p..p+32]);
            p += 32;
            cid
        }};
    }

    let instr = match op {
        Op::Triple => {
            let s = read_v!(); let pr = read_v!(); let o = read_v!();
            Instruction::Triple { s, p: pr, o }
        }
        Op::Quality => {
            let s = read_v!(); let q = read_v!();
            Instruction::Quality { s, q }
        }
        Op::Quantity => {
            let s = read_v!();
            let value = read_num!();
            let unit = read_v!();
            Instruction::Quantity { s, value, unit }
        }
        Op::Sequence => {
            let n = read_u8!() as usize;
            let mut items = Vec::with_capacity(n);
            for _ in 0..n { items.push(read_v!()); }
            Instruction::Sequence { items }
        }
        Op::PartOf     => { let part = read_v!(); let whole = read_v!(); Instruction::PartOf { part, whole } }
        Op::Located    => { let s = read_v!(); let loc = read_v!(); Instruction::Located { s, location: loc } }
        Op::Temporal   => { let s = read_v!(); let t = read_v!(); Instruction::Temporal { s, time: t } }
        Op::Causal     => { let c = read_v!(); let e = read_v!(); Instruction::Causal { cause: c, effect: e } }
        Op::Simulates  => { let s = read_v!(); let m = read_v!(); Instruction::Simulates { s, model: m } }
        Op::Condition  => { let c = read_v!(); let r = read_v!(); Instruction::Condition { cond: c, result: r } }
        Op::Agent      => { let a = read_v!(); let act = read_v!(); Instruction::Agent { actor: a, action: act } }
        Op::Tool       => { let a = read_v!(); let i = read_v!(); Instruction::Tool { action: a, instrument: i } }
        Op::Range => {
            let s = read_v!();
            let min = read_num!();
            let max = read_num!();
            Instruction::Range { s, min, max }
        }
        Op::Tolerance => {
            let s = read_v!();
            let value = read_num!();
            let delta = read_num!();
            Instruction::Tolerance { s, value, delta }
        }
        Op::Constraint => {
            let src = read_v!();
            let op_byte = read_u8!();
            let cop = ConstraintOp::from_u8(op_byte).ok_or_else(|| {
                KuError::InvalidData(format!("Unknown constraint op: {}", op_byte))
            })?;
            let tgt = read_v!();
            Instruction::Constraint { source: src, op: cop, target: tgt }
        }
        Op::EnumVal => {
            let s = read_v!();
            let n = read_u8!() as usize;
            let mut values = Vec::with_capacity(n);
            for _ in 0..n { values.push(read_v!()); }
            Instruction::EnumVal { s, values }
        }
        Op::Certainty  => { let lvl = read_u16!(); Instruction::Certainty { level: lvl } }
        Op::Difficulty => { let lvl = read_u8!(); Instruction::Difficulty { level: lvl } }
        Op::CidRef     => { let cid = read_cid!(); Instruction::CidRef { cid } }
        Op::Step => {
            let ord = read_u8!();
            let action = read_v!();
            let target = read_v!();
            Instruction::Step { ord, action, target }
        }
        Op::Precond => { let c = read_v!(); Instruction::Precond { concept: c } }
        Op::Effect  => { let c = read_v!(); Instruction::Effect { concept: c } }
        Op::Affect  => {
            let v = read_i16!(); let a = read_i16!(); let d = read_i16!();
            Instruction::Affect { v, a, d }
        }
        Op::Label      => { let k = read_v!(); let val = read_v!(); Instruction::Label { key: k, value: val } }
        Op::TextRef => {
            let lang = read_u8!();
            let len = read_u16!() as usize;
            if p + len > end { return Err(KuError::InvalidData("Truncated text".into())); }
            let d = data[p..p+len].to_vec(); p += len;
            Instruction::TextRef { lang, data: d }
        }
        Op::Formula => {
            let fmt = read_u8!();
            let len = read_u16!() as usize;
            if p + len > end { return Err(KuError::InvalidData("Truncated formula".into())); }
            let d = data[p..p+len].to_vec(); p += len;
            Instruction::Formula { format: fmt, data: d }
        }
        Op::Witness => {
            let count = read_u16!(); let prox = read_u8!();
            Instruction::Witness { count, proximity: prox }
        }
        Op::MediaRef => {
            let sys = read_u8!();
            let len = read_u8!() as usize;
            if p + len > end { return Err(KuError::InvalidData("Truncated media ref".into())); }
            let id = data[p..p+len].to_vec(); p += len;
            Instruction::MediaRef { system: sys, id }
        }
        Op::CompositeHdr => {
            let ct = read_u8!(); let comp = read_u8!(); let ver = read_u32!();
            Instruction::CompositeHdr { composite_type: ct, completeness: comp, version: ver }
        }
        Op::Member => {
            let order = read_u16!();
            let role = read_u8!();
            let required = read_u8!() != 0;
            let label = read_v!();
            let cid = read_cid!();
            Instruction::Member { order, role, required, label, cid }
        }
        Op::End => Instruction::End,
        Op::Extended => {
            return Err(KuError::InvalidData("Extended opcode not yet supported".into()));
        }
    };

    Ok((instr, p))
}

// ============================================================================
// Convenience: Build CoreDna from instruction list
// ============================================================================

impl CoreDna {
    /// Create a new Core DNA with given gene type and instructions.
    pub fn new(gene_type: u8, instructions: Vec<Instruction>) -> Self {
        Self {
            header: CoreDnaHeader {
                version: CORE_DNA_VERSION,
                gene_type,
                has_qualifiers: false,
            },
            instructions,
        }
    }

    /// Encode to compact binary wire format.
    pub fn encode(&self) -> Result<Vec<u8>, KuError> {
        encode_core_dna(self)
    }

    /// Decode from compact binary wire format.
    pub fn decode(data: &[u8]) -> Result<Self, KuError> {
        decode_core_dna(data)
    }

    /// Total number of instructions (excluding END).
    pub fn len(&self) -> usize {
        self.instructions.len()
    }

    /// Check if instruction stream is empty.
    pub fn is_empty(&self) -> bool {
        self.instructions.is_empty()
    }
}

// ============================================================================
// Bridge: KnowledgeUnit → CoreDna
// ============================================================================

use crate::types::{
    KnowledgeUnit, Gene, GeneType, Codon, RoleId, Triple, ProcedureStep,
    HeaderFlags, Bond, EpistemicStatus, EvidenceType,
    CompositeEntry, CompositeConstraint, CompositeType, Completeness, StructuralRole,
};

/// Convert a rich KnowledgeUnit into a compact CoreDna.
///
/// This is a LOSSY conversion: trust, epigenetic, bond metadata, and
/// qualifier details are dropped (they live in the epigenetic runtime layer).
pub fn ku_to_core_dna(ku: &KnowledgeUnit) -> Result<CoreDna, KuError> {
    let gene_type = ku.gene.gene_type() as u8;
    let mut instructions = Vec::new();

    // Find primary subject from codons (first Object-role codon)
    let primary = ku.codons.iter()
        .find(|c| c.role == RoleId::Object)
        .map(|c| c.concept_id)
        .unwrap_or(0);

    // Encode codons as semantic instructions (skip Object — it's the subject)
    for codon in &ku.codons {
        match codon.role {
            RoleId::Object => {} // primary subject, used implicitly
            RoleId::Quality => {
                instructions.push(Instruction::Quality { s: primary, q: codon.concept_id });
            }
            RoleId::Agent => {
                instructions.push(Instruction::Agent { actor: codon.concept_id, action: primary });
            }
            RoleId::Location => {
                instructions.push(Instruction::Located { s: primary, location: codon.concept_id });
            }
            RoleId::Manner | RoleId::Purpose | RoleId::Condition |
            RoleId::Cause | RoleId::Result | RoleId::Time |
            RoleId::Quantity | RoleId::Tool |
            RoleId::CompoundHead | RoleId::CompoundMod => {
                instructions.push(Instruction::Label { key: codon.role as u64, value: codon.concept_id });
            }
        }
    }

    // Encode gene-specific data
    match &ku.gene {
        Gene::Fact { triples, certainty, .. } => {
            for t in triples {
                instructions.push(Instruction::Triple { s: t.subject, p: t.predicate, o: t.object });
            }
            if *certainty > 0 {
                instructions.push(Instruction::Certainty { level: *certainty });
            }
        }
        Gene::Procedure { steps, difficulty, .. } => {
            for step in steps {
                instructions.push(Instruction::Step {
                    ord: step.ord as u8,
                    action: step.act,
                    target: step.tgt,
                });
                for pre in &step.pre {
                    instructions.push(Instruction::Precond { concept: pre.concept_id });
                }
                for eff in &step.eff {
                    instructions.push(Instruction::Effect { concept: eff.concept_id });
                }
            }
            instructions.push(Instruction::Difficulty { level: *difficulty });
        }
        Gene::Experience { scene, affect, .. } => {
            for codon in scene {
                instructions.push(Instruction::Label { key: codon.role as u64, value: codon.concept_id });
            }
            instructions.push(Instruction::Affect { v: affect.v, a: affect.a, d: affect.d });
        }
        Gene::Hypothesis { body_codons, confidence, maturity_level, .. } => {
            for codon in body_codons {
                instructions.push(Instruction::Label { key: codon.role as u64, value: codon.concept_id });
            }
            instructions.push(Instruction::Certainty { level: *confidence });
            instructions.push(Instruction::Difficulty { level: *maturity_level });
        }
        Gene::Formal { notation_source, notation_format, domain, .. } => {
            instructions.push(Instruction::Label {
                key: 0xF000, // DOMAIN marker
                value: *domain as u64,
            });
            instructions.push(Instruction::Formula {
                format: *notation_format,
                data: notation_source.clone(),
            });
        }
        Gene::Testimony { triples, witness_count, proximity, .. } => {
            for t in triples {
                instructions.push(Instruction::Triple { s: t.subject, p: t.predicate, o: t.object });
            }
            instructions.push(Instruction::Witness { count: *witness_count, proximity: *proximity });
        }
        Gene::Composite { members, constraints: _, cluster_version, composite_type, completeness, summary_codons, .. } => {
            instructions.push(Instruction::CompositeHdr {
                composite_type: *composite_type as u8,
                completeness: *completeness as u8,
                version: *cluster_version,
            });
            for member in members {
                let mut cid = [0u8; 32];
                let copy_len = member.cid.len().min(32);
                cid[..copy_len].copy_from_slice(&member.cid[..copy_len]);
                instructions.push(Instruction::Member {
                    order: member.order,
                    role: member.role as u8,
                    required: member.required,
                    label: member.label,
                    cid,
                });
            }
            for sc in summary_codons {
                instructions.push(Instruction::Label { key: 0xF001, value: sc.concept_id }); // SUMMARY marker
            }
        }
        // Creative, Narrative, MediaExperience, Sensory — encode as labels
        _ => {
            // Fallback: encode primary subject as a quality
            instructions.push(Instruction::Label { key: 0xFFFF, value: primary });
        }
    }

    Ok(CoreDna::new(gene_type, instructions))
}

// ============================================================================
// Bridge: CoreDna → KnowledgeUnit
// ============================================================================

/// Convert a compact CoreDna back into a runtime KnowledgeUnit.
///
/// Creates a minimal KU with core knowledge only.
/// Trust, epigenetic, and bond metadata are set to None/empty.
pub fn core_dna_to_ku(dna: &CoreDna) -> Result<KnowledgeUnit, KuError> {
    // In Core DNA, gene_type is stored directly as 0-10 (4 bits).
    // This differs from v4/v5 wire where types 7+ use extended encoding.
    let gene_type = match dna.header.gene_type {
        0  => GeneType::Fact,
        1  => GeneType::Procedure,
        2  => GeneType::Experience,
        3  => GeneType::Creative,
        4  => GeneType::MediaExperience,
        5  => GeneType::Testimony,
        6  => GeneType::Formal,
        7  => GeneType::Hypothesis,
        8  => GeneType::Narrative,
        9  => GeneType::Sensory,
        10 => GeneType::Composite,
        other => return Err(KuError::UnknownGeneType(other)),
    };

    let mut codons = Vec::new();
    let mut triples = Vec::new();
    let mut steps = Vec::new();
    let mut certainty: u16 = 0;
    let mut difficulty: u8 = 0;
    let mut affect_val: Option<(i16, i16, i16)> = None;
    let mut witness_count: u16 = 0;
    let mut proximity: u8 = 0;
    let mut composite_type: u8 = 0;
    let mut completeness: u8 = 0;
    let mut cluster_version: u32 = 0;
    let mut members = Vec::new();
    let mut formula_data: Vec<u8> = Vec::new();
    let mut formula_fmt: u8 = 0;
    let mut domain: u8 = 0;
    let mut summary_codons = Vec::new();
    let mut primary_subject: ConceptId = 0;

    // First pass: find primary subject from Triple or Quality instructions
    for instr in &dna.instructions {
        match instr {
            Instruction::Triple { s, .. } => { primary_subject = *s; break; }
            Instruction::Quality { s, .. } => { primary_subject = *s; break; }
            Instruction::Quantity { s, .. } => { primary_subject = *s; break; }
            _ => {}
        }
    }

    // Add primary subject as Object codon
    if primary_subject > 0 {
        codons.push(Codon { concept_id: primary_subject, role: RoleId::Object, qualifiers: vec![] });
    }

    // Second pass: collect all instructions
    for instr in &dna.instructions {
        match instr {
            Instruction::Triple { s, p, o } => {
                triples.push(Triple { subject: *s, predicate: *p, object: *o });
            }
            Instruction::Quality { q, .. } => {
                codons.push(Codon { concept_id: *q, role: RoleId::Quality, qualifiers: vec![] });
            }
            Instruction::Quantity { s, value, unit } => {
                triples.push(Triple { subject: *s, predicate: *unit, object: value.as_f64() as u64 });
            }
            Instruction::Located { location, .. } => {
                codons.push(Codon { concept_id: *location, role: RoleId::Location, qualifiers: vec![] });
            }
            Instruction::Agent { actor, .. } => {
                codons.push(Codon { concept_id: *actor, role: RoleId::Agent, qualifiers: vec![] });
            }
            Instruction::Simulates { s, model } => {
                triples.push(Triple { subject: *s, predicate: 0xF008, object: *model }); // SIMULATES predicate
            }
            Instruction::Step { ord, action, target } => {
                steps.push(ProcedureStep {
                    ord: *ord as u16,
                    act: *action,
                    tgt: *target,
                    pre: vec![],
                    tools: vec![],
                    eff: vec![],
                    warn: vec![],
                });
            }
            Instruction::Precond { concept } => {
                if let Some(last) = steps.last_mut() {
                    last.pre.push(Codon { concept_id: *concept, role: RoleId::Condition, qualifiers: vec![] });
                }
            }
            Instruction::Effect { concept } => {
                if let Some(last) = steps.last_mut() {
                    last.eff.push(Codon { concept_id: *concept, role: RoleId::Result, qualifiers: vec![] });
                }
            }
            Instruction::Certainty { level } => { certainty = *level; }
            Instruction::Difficulty { level } => { difficulty = *level; }
            Instruction::Affect { v, a, d } => { affect_val = Some((*v, *a, *d)); }
            Instruction::Witness { count, proximity: prox } => {
                witness_count = *count; proximity = *prox;
            }
            Instruction::CompositeHdr { composite_type: ct, completeness: comp, version } => {
                composite_type = *ct; completeness = *comp; cluster_version = *version;
            }
            Instruction::Member { order, role, required, label, cid } => {
                members.push(CompositeEntry {
                    cid: cid.to_vec(),
                    order: *order,
                    role: StructuralRole::from_u8(*role).unwrap_or_default(),
                    required: *required,
                    label: *label,
                    expected_gene_type: None,
                });
            }
            Instruction::Formula { format, data } => {
                formula_fmt = *format; formula_data = data.clone();
            }
            Instruction::Label { key, value } => {
                if *key == 0xF000 { domain = *value as u8; }
                else if *key == 0xF001 {
                    summary_codons.push(Codon { concept_id: *value, role: RoleId::Object, qualifiers: vec![] });
                }
            }
            _ => {} // Skip instructions not relevant to current gene type
        }
    }

    // Build Gene variant based on gene_type
    let gene = match gene_type {
        GeneType::Fact => Gene::Fact { triples, certainty, evidence: vec![] },
        GeneType::Procedure => Gene::Procedure {
            steps,
            total_time: None,
            difficulty,
            tools_req: vec![],
        },
        GeneType::Experience => Gene::Experience {
            scene: codons.iter().filter(|c| c.role != RoleId::Object).cloned().collect(),
            affect: crate::types::Affect {
                v: affect_val.map(|a| a.0).unwrap_or(0),
                a: affect_val.map(|a| a.1).unwrap_or(0),
                d: affect_val.map(|a| a.2).unwrap_or(0),
            },
            canonical: None,
            perspective: None,
        },
        GeneType::Hypothesis => Gene::Hypothesis {
            base_type: 0,
            body_codons: codons.iter().filter(|c| c.role != RoleId::Object).cloned().collect(),
            maturity_level: difficulty,
            confidence: certainty,
            completeness: 5000,
            falsifiable: true,
        },
        GeneType::Formal => Gene::Formal {
            domain,
            notation_format: formula_fmt,
            notation_source: formula_data,
            statement_type: 0,
            verification_status: 0,
        },
        GeneType::Testimony => Gene::Testimony {
            triples,
            claim_type: 0,
            extraordinary: 0,
            witness_count,
            proximity,
            verification_status: 0,
        },
        GeneType::Composite => Gene::Composite {
            members,
            constraints: vec![],
            cluster_version,
            max_depth: 255,
            composite_type: CompositeType::from_u8(composite_type).unwrap_or_default(),
            schema: None,
            completeness: Completeness::from_u8(completeness).unwrap_or_default(),
            summary_codons,
        },
        // Fallback for unsupported gene types
        _ => Gene::Fact { triples, certainty, evidence: vec![] },
    };

    Ok(KnowledgeUnit {
        codons,
        bonds: vec![], // Bonds live in epigenetic layer
        gene,
        flags: HeaderFlags::default(),
        epistemic_status: None,
        evidence_type: None,
        trust: None,      // Trust lives in epigenetic layer
        epigenetic: None,  // Epigenetic lives in runtime only
    })
}

// ============================================================================
// Auto-detect decoder: v4/v5 CBOR vs v6 Core DNA
// ============================================================================

/// Detected wire format.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WireFormat {
    /// v4/v5 CBOR format (MAGIC = 0x4B44 "KD")
    CborV4V5,
    /// v6 Core DNA format (MAGIC = 0x4B, second byte is VER_META)
    CoreDnaV6,
    /// Unknown format
    Unknown,
}

/// Detect the wire format from the first 2 bytes.
pub fn detect_wire_format(data: &[u8]) -> WireFormat {
    if data.len() < 2 {
        return WireFormat::Unknown;
    }
    if data[0] == 0x4B && data[1] == 0x44 {
        WireFormat::CborV4V5
    } else if data[0] == CORE_DNA_MAGIC {
        WireFormat::CoreDnaV6
    } else {
        WireFormat::Unknown
    }
}

/// Unified decoder: auto-detects format and returns a KnowledgeUnit.
///
/// - v4/v5 CBOR → uses existing decoder
/// - v6 Core DNA → decodes to CoreDna then bridges to KnowledgeUnit
pub fn decode_any(data: &[u8]) -> Result<KnowledgeUnit, KuError> {
    match detect_wire_format(data) {
        WireFormat::CborV4V5 => {
            let (_, ku) = crate::decoder::decode_full_knowledge_unit(data)?;
            Ok(ku)
        }
        WireFormat::CoreDnaV6 => {
            let dna = decode_core_dna(data)?;
            core_dna_to_ku(&dna)
        }
        WireFormat::Unknown => {
            Err(KuError::InvalidData("Unknown wire format: first bytes don't match any known magic".into()))
        }
    }
}

// ============================================================================
// Tests
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_crc16() {
        let crc = crc16_ccitt(b"123456789");
        assert_eq!(crc, 0x29B1, "CRC-16/CCITT check value");
        println!("  CRC-16 check value: 0x{:04X} ✓", crc);
    }

    #[test]
    fn test_numeric_roundtrip() {
        let values = vec![
            NumericValue::U8(42),
            NumericValue::U16(1000),
            NumericValue::I16(-500),
            NumericValue::U32(100_000),
            NumericValue::I32(-100_000),
            NumericValue::F32(35.2),
        ];
        for val in &values {
            let encoded = val.encode();
            let (decoded, consumed) = NumericValue::decode(&encoded, 0).unwrap();
            assert_eq!(*val, decoded, "Numeric roundtrip failed for {:?}", val);
            assert_eq!(consumed, encoded.len());
        }
        println!("  Numeric roundtrip: {} values ✓", values.len());
    }

    #[test]
    fn test_simple_fact_roundtrip() {
        println!("\n══════════════════════════════════════════════════");
        println!("  TEST: Core DNA — Simple Fact Roundtrip");
        println!("══════════════════════════════════════════════════");

        // "Water boils at 100°C"
        let dna = CoreDna::new(0, vec![ // gene_type=0 (Fact)
            Instruction::Quantity {
                s: 100,                              // WATER
                value: NumericValue::F32(100.0),     // 100
                unit: 101,                           // CELSIUS
            },
            Instruction::Certainty { level: 9999 },
        ]);

        let wire = dna.encode().unwrap();
        println!("  Wire size:    {} bytes", wire.len());
        println!("  vs text:      {} bytes (\"Water boils at 100°C\")", "Water boils at 100°C".len());
        assert!(wire.len() < "Water boils at 100°C".len(),
            "Core DNA should be smaller than text: {} vs {}",
            wire.len(), "Water boils at 100°C".len());

        // Verify header
        assert_eq!(wire[0], CORE_DNA_MAGIC);

        // Decode roundtrip
        let decoded = CoreDna::decode(&wire).unwrap();
        assert_eq!(decoded.header.version, CORE_DNA_VERSION);
        assert_eq!(decoded.header.gene_type, 0);
        assert_eq!(decoded.instructions.len(), 2);
        assert_eq!(decoded.instructions, dna.instructions);

        println!("  Roundtrip:    PASSED ✓");
        println!("  Compression:  {:.1}x smaller than text", "Water boils at 100°C".len() as f64 / wire.len() as f64);
    }

    #[test]
    fn test_boi_ech_core_dna() {
        println!("\n══════════════════════════════════════════════════════════════");
        println!("  TEST: 🧬 Core DNA — \"Bơi ếch\" (Breaststroke)");
        println!("══════════════════════════════════════════════════════════════");

        const C_BREASTSTROKE: ConceptId = 500;
        const C_SWIMMING_STYLE: ConceptId = 501;
        const C_BASIC: ConceptId = 502;
        const C_FROG: ConceptId = 503;
        const C_WATER: ConceptId = 505;
        const C_SWIMMER: ConceptId = 506;
        const C_PRONE: ConceptId = 507;
        const C_ARM_SWEEP: ConceptId = 508;
        const C_LEG_KICK: ConceptId = 509;
        const C_BREATHING: ConceptId = 510;
        const C_GLIDE: ConceptId = 511;
        const C_FORWARD: ConceptId = 512;
        const C_RHYTHMIC: ConceptId = 513;
        const C_ENERGY_EFF: ConceptId = 514;

        // KU#1: Fact — Definition
        let dna1 = CoreDna::new(0, vec![
            Instruction::Triple { s: C_BREASTSTROKE, p: C_SWIMMING_STYLE, o: C_BASIC },
            Instruction::Simulates { s: C_BREASTSTROKE, model: C_FROG },
            Instruction::Located { s: C_BREASTSTROKE, location: C_WATER },
            Instruction::Certainty { level: 9900 },
        ]);

        let wire1 = dna1.encode().unwrap();
        println!("\n  KU#1 [Fact: Definition]");
        println!("    Instructions: {} ops", dna1.len());
        println!("    Wire:         {} bytes", wire1.len());

        // KU#2: Procedure — Swimming cycle
        let dna2 = CoreDna::new(1, vec![
            Instruction::Agent { actor: C_SWIMMER, action: C_BREASTSTROKE },
            Instruction::Precond { concept: C_PRONE },
            Instruction::Step { ord: 0, action: C_ARM_SWEEP, target: C_WATER },
            Instruction::Step { ord: 1, action: C_LEG_KICK, target: C_WATER },
            Instruction::Effect { concept: C_FORWARD },
            Instruction::Step { ord: 2, action: C_BREATHING, target: C_SWIMMER },
            Instruction::Step { ord: 3, action: C_GLIDE, target: C_FORWARD },
            Instruction::Effect { concept: C_FORWARD },
            Instruction::Difficulty { level: 1 },
        ]);

        let wire2 = dna2.encode().unwrap();
        println!("\n  KU#2 [Procedure: Swimming Cycle]");
        println!("    Instructions: {} ops", dna2.len());
        println!("    Wire:         {} bytes", wire2.len());

        // KU#3: Fact — Properties
        let dna3 = CoreDna::new(0, vec![
            Instruction::Quality { s: C_BREASTSTROKE, q: C_RHYTHMIC },
            Instruction::Quality { s: C_BREASTSTROKE, q: C_ENERGY_EFF },
            Instruction::Certainty { level: 8500 },
        ]);

        let wire3 = dna3.encode().unwrap();
        println!("\n  KU#3 [Fact: Properties]");
        println!("    Instructions: {} ops", dna3.len());
        println!("    Wire:         {} bytes", wire3.len());

        // Decode roundtrip all 3
        let d1 = CoreDna::decode(&wire1).unwrap();
        let d2 = CoreDna::decode(&wire2).unwrap();
        let d3 = CoreDna::decode(&wire3).unwrap();
        assert_eq!(d1.instructions, dna1.instructions);
        assert_eq!(d2.instructions, dna2.instructions);
        assert_eq!(d3.instructions, dna3.instructions);

        let total = wire1.len() + wire2.len() + wire3.len();
        let text_bytes = "Bơi ếch là kiểu bơi cơ bản mô phỏng chuyển động của con ếch dưới nước. Người bơi nằm úp, thực hiện chu kỳ lặp lại liên tục bao gồm: quạt tay, thu và đạp chân, lấy hơi, và lướt nước để tiến về phía trước một cách nhịp nhàng, ít tốn sức".len();

        println!("\n  ═══════════════════════════════════════════════════");
        println!("  📊 Core DNA vs Text vs CBOR");
        println!("  ═══════════════════════════════════════════════════");
        println!("  Original text (UTF-8): {} bytes", text_bytes);
        println!("  Core DNA KU#1:         {} bytes", wire1.len());
        println!("  Core DNA KU#2:         {} bytes", wire2.len());
        println!("  Core DNA KU#3:         {} bytes", wire3.len());
        println!("  Core DNA total:        {} bytes ({:.1}x smaller than text)",
            total, text_bytes as f64 / total as f64);
        println!("  CBOR v5 total:         1053 bytes (3.3x LARGER than text)");
        println!("  ═══════════════════════════════════════════════════");

        assert!(total < text_bytes,
            "Core DNA ({}) must be smaller than text ({})", total, text_bytes);

        println!("\n  test_boi_ech_core_dna: PASSED ✓ 🧬");
    }

    #[test]
    fn test_airplane_wing_precision() {
        println!("\n══════════════════════════════════════════════════════════════");
        println!("  TEST: ✈️  Core DNA — Airplane Wing Design (Precision)");
        println!("══════════════════════════════════════════════════════════════");

        // Concept IDs for aerospace domain
        const C_WING: ConceptId = 2000;
        const C_SWEEP_ANGLE: ConceptId = 2001;
        const C_WING_AREA: ConceptId = 2002;
        const C_MAX_SPEED: ConceptId = 2003;
        const C_ASPECT_RATIO: ConceptId = 2004;
        const C_TAPER_RATIO: ConceptId = 2005;
        const C_DIHEDRAL: ConceptId = 2006;
        const C_THICKNESS_RATIO: ConceptId = 2007;
        const C_SPAN: ConceptId = 2008;
        const C_MAC: ConceptId = 2009; // Mean Aerodynamic Chord
        const C_DEGREES: ConceptId = 3000;
        const C_SQ_METER: ConceptId = 3001;
        const C_MACH: ConceptId = 3002;
        const C_RATIO: ConceptId = 3003;
        const C_METER: ConceptId = 3004;
        const C_AIRPLANE: ConceptId = 4000;

        let dna = CoreDna::new(0, vec![
            // Hierarchy
            Instruction::PartOf { part: C_WING, whole: C_AIRPLANE },
            // Precise measurements
            Instruction::Tolerance { s: C_SWEEP_ANGLE,
                value: NumericValue::F32(35.2), delta: NumericValue::F32(0.1) },
            Instruction::Quantity { s: C_WING_AREA,
                value: NumericValue::F32(122.4), unit: C_SQ_METER },
            Instruction::Quantity { s: C_MAX_SPEED,
                value: NumericValue::F32(0.82), unit: C_MACH },
            Instruction::Quantity { s: C_ASPECT_RATIO,
                value: NumericValue::F32(9.5), unit: C_RATIO },
            Instruction::Quantity { s: C_TAPER_RATIO,
                value: NumericValue::F32(0.3), unit: C_RATIO },
            Instruction::Tolerance { s: C_DIHEDRAL,
                value: NumericValue::F32(5.0), delta: NumericValue::F32(0.5) },
            Instruction::Range { s: C_THICKNESS_RATIO,
                min: NumericValue::F32(0.10), max: NumericValue::F32(0.14) },
            Instruction::Quantity { s: C_SPAN,
                value: NumericValue::F32(34.1), unit: C_METER },
            Instruction::Quantity { s: C_MAC,
                value: NumericValue::F32(4.19), unit: C_METER },
            // Constraints
            Instruction::Constraint { source: C_SWEEP_ANGLE,
                op: ConstraintOp::Le, target: C_DIHEDRAL },
            Instruction::Certainty { level: 9800 },
        ]);

        let wire = dna.encode().unwrap();
        let text = "Wing: sweep=35.2°±0.1°, area=122.4m², Mach=0.82, AR=9.5, taper=0.3, dihedral=5.0°±0.5°, t/c=0.10-0.14, span=34.1m, MAC=4.19m";
        let text_bytes = text.len();

        println!("\n  Measurements: 10 parameters (7 QUANTITY + 2 TOLERANCE + 1 RANGE)");
        println!("  Constraints:  1 (sweep ≤ dihedral)");
        println!("  Wire:         {} bytes", wire.len());
        println!("  Text:         {} bytes", text_bytes);
        println!("  Ratio:        {:.1}x smaller than text", text_bytes as f64 / wire.len() as f64);

        // Roundtrip — verify precision is preserved
        let decoded = CoreDna::decode(&wire).unwrap();
        assert_eq!(decoded.instructions.len(), dna.instructions.len());

        // Verify float precision survived
        if let Instruction::Tolerance { s, value, delta } = &decoded.instructions[1] {
            assert_eq!(*s, C_SWEEP_ANGLE);
            assert_eq!(value.as_f64(), 35.200000762939453); // f32 precision
            assert!(delta.as_f64() < 0.2); // delta ~0.1
        } else {
            panic!("Expected Tolerance instruction");
        }

        if let Instruction::Range { s, min, max } = &decoded.instructions[7] {
            assert_eq!(*s, C_THICKNESS_RATIO);
            assert!((min.as_f64() - 0.10).abs() < 0.001);
            assert!((max.as_f64() - 0.14).abs() < 0.001);
        } else {
            panic!("Expected Range instruction");
        }

        assert!(wire.len() < text_bytes,
            "Core DNA ({}) must be smaller than text ({})", wire.len(), text_bytes);

        println!("\n  test_airplane_wing_precision: PASSED ✓ ✈️");
    }

    #[test]
    fn test_all_instruction_types_roundtrip() {
        println!("\n══════════════════════════════════════════════════");
        println!("  TEST: Core DNA — All 30 Instruction Types Roundtrip");
        println!("══════════════════════════════════════════════════");

        let test_cid = [0xABu8; 32];

        let instructions = vec![
            Instruction::Triple { s: 100, p: 200, o: 300 },
            Instruction::Quality { s: 100, q: 400 },
            Instruction::Quantity { s: 100, value: NumericValue::F32(3.14), unit: 500 },
            Instruction::Sequence { items: vec![10, 20, 30, 40] },
            Instruction::PartOf { part: 100, whole: 200 },
            Instruction::Located { s: 100, location: 600 },
            Instruction::Temporal { s: 100, time: 700 },
            Instruction::Causal { cause: 100, effect: 200 },
            Instruction::Simulates { s: 100, model: 300 },
            Instruction::Condition { cond: 100, result: 200 },
            Instruction::Agent { actor: 100, action: 200 },
            Instruction::Tool { action: 100, instrument: 200 },
            Instruction::Range { s: 100, min: NumericValue::F32(1.0), max: NumericValue::F32(10.0) },
            Instruction::Tolerance { s: 100, value: NumericValue::F32(5.0), delta: NumericValue::F32(0.5) },
            Instruction::Constraint { source: 100, op: ConstraintOp::Le, target: 200 },
            Instruction::EnumVal { s: 100, values: vec![1, 2, 3] },
            Instruction::Certainty { level: 9500 },
            Instruction::Difficulty { level: 3 },
            Instruction::CidRef { cid: test_cid },
            Instruction::Step { ord: 0, action: 100, target: 200 },
            Instruction::Precond { concept: 100 },
            Instruction::Effect { concept: 200 },
            Instruction::Affect { v: -5000, a: 7000, d: 3000 },
            Instruction::Label { key: 100, value: 200 },
            Instruction::TextRef { lang: 1, data: b"hello".to_vec() },
            Instruction::Formula { format: 0, data: b"E=mc^2".to_vec() },
            Instruction::Witness { count: 5, proximity: 1 },
            Instruction::MediaRef { system: 1, id: b"tt1234567".to_vec() },
            Instruction::CompositeHdr { composite_type: 3, completeness: 1, version: 42 },
            Instruction::Member { order: 0, role: 2, required: true, label: 999, cid: test_cid },
        ];

        let dna = CoreDna::new(0, instructions.clone());
        let wire = dna.encode().unwrap();
        let decoded = CoreDna::decode(&wire).unwrap();

        assert_eq!(decoded.instructions.len(), instructions.len(),
            "Instruction count mismatch");

        for (i, (original, decoded_i)) in instructions.iter().zip(decoded.instructions.iter()).enumerate() {
            assert_eq!(original, decoded_i,
                "Instruction {} mismatch:\n  expected: {:?}\n  got:      {:?}", i, original, decoded_i);
        }

        println!("  Instructions:  {} types roundtripped", instructions.len());
        println!("  Wire size:     {} bytes", wire.len());
        println!("  All matched:   PASSED ✓");
    }

    #[test]
    fn test_crc_corruption_detected() {
        let dna = CoreDna::new(0, vec![
            Instruction::Triple { s: 1, p: 2, o: 3 },
        ]);
        let mut wire = dna.encode().unwrap();

        // Corrupt one byte in the instruction stream
        wire[3] ^= 0xFF;

        let result = CoreDna::decode(&wire);
        assert!(result.is_err(), "Should detect CRC corruption");
        println!("  CRC corruption detected: PASSED ✓");
    }

    #[test]
    fn test_empty_dna() {
        let dna = CoreDna::new(0, vec![]);
        let wire = dna.encode().unwrap();
        // Header(2) + END(1) + CRC(2) = 5 bytes minimum
        assert_eq!(wire.len(), 5, "Empty Core DNA should be 5 bytes");

        let decoded = CoreDna::decode(&wire).unwrap();
        assert!(decoded.is_empty());
        println!("  Empty Core DNA: {} bytes, PASSED ✓", wire.len());
    }

    // =======================================================================
    // Phase 2: Bridge & Auto-detect tests
    // =======================================================================

    #[test]
    fn test_ku_to_core_dna_fact() {
        use crate::types::*;
        println!("\n══════════════════════════════════════════════════");
        println!("  TEST: Bridge — KnowledgeUnit(Fact) → CoreDna → KU");
        println!("══════════════════════════════════════════════════");

        let ku = KnowledgeUnit {
            codons: vec![
                Codon { concept_id: 500, role: RoleId::Object, qualifiers: vec![] },
                Codon { concept_id: 513, role: RoleId::Quality, qualifiers: vec![] },
                Codon { concept_id: 505, role: RoleId::Location, qualifiers: vec![] },
            ],
            bonds: vec![],
            gene: Gene::Fact {
                triples: vec![
                    Triple { subject: 500, predicate: 501, object: 502 },
                ],
                certainty: 9900,
                evidence: vec![],
            },
            flags: HeaderFlags::default(),
            epistemic_status: Some(EpistemicStatus::Consensus),
            evidence_type: Some(EvidenceType::Observational),
            trust: None,
            epigenetic: None,
        };

        // KU → CoreDna
        let dna = ku_to_core_dna(&ku).unwrap();
        assert_eq!(dna.header.gene_type, 0); // Fact
        println!("  Instructions: {}", dna.len());

        // CoreDna → wire
        let wire = dna.encode().unwrap();
        println!("  Wire size: {} bytes", wire.len());

        // Compare with CBOR encoding
        let cbor_wire = crate::encoder::encode_knowledge_unit(&ku).unwrap();
        println!("  CBOR size: {} bytes", cbor_wire.len());
        println!("  Reduction: {:.1}x smaller", cbor_wire.len() as f64 / wire.len() as f64);

        assert!(wire.len() < cbor_wire.len(),
            "Core DNA ({}) should be smaller than CBOR ({})", wire.len(), cbor_wire.len());

        // Wire → CoreDna → KU roundtrip
        let decoded_dna = CoreDna::decode(&wire).unwrap();
        let decoded_ku = core_dna_to_ku(&decoded_dna).unwrap();

        // Verify gene type preserved
        assert_eq!(decoded_ku.gene.gene_type(), GeneType::Fact);

        // Verify triples preserved
        if let Gene::Fact { triples, certainty, .. } = &decoded_ku.gene {
            assert_eq!(triples.len(), 1);
            assert_eq!(triples[0].subject, 500);
            assert_eq!(triples[0].predicate, 501);
            assert_eq!(triples[0].object, 502);
            assert_eq!(*certainty, 9900);
        } else {
            panic!("Expected Fact gene");
        }

        // Verify codons restored
        assert!(decoded_ku.codons.iter().any(|c| c.concept_id == 500 && c.role == RoleId::Object));
        assert!(decoded_ku.codons.iter().any(|c| c.concept_id == 513 && c.role == RoleId::Quality));
        assert!(decoded_ku.codons.iter().any(|c| c.concept_id == 505 && c.role == RoleId::Location));

        // Verify epigenetic data dropped (by design)
        assert!(decoded_ku.trust.is_none());
        assert!(decoded_ku.epigenetic.is_none());
        assert!(decoded_ku.bonds.is_empty());

        println!("  Bridge roundtrip: PASSED ✓");
    }

    #[test]
    fn test_ku_to_core_dna_procedure() {
        use crate::types::*;
        println!("\n══════════════════════════════════════════════════");
        println!("  TEST: Bridge — KnowledgeUnit(Procedure) → CoreDna → KU");
        println!("══════════════════════════════════════════════════");

        let ku = KnowledgeUnit {
            codons: vec![
                Codon { concept_id: 500, role: RoleId::Object, qualifiers: vec![] },
            ],
            bonds: vec![],
            gene: Gene::Procedure {
                steps: vec![
                    ProcedureStep {
                        ord: 0, act: 508, tgt: 505,
                        pre: vec![Codon { concept_id: 507, role: RoleId::Condition, qualifiers: vec![] }],
                        tools: vec![], eff: vec![], warn: vec![],
                    },
                    ProcedureStep {
                        ord: 1, act: 509, tgt: 505,
                        pre: vec![], tools: vec![],
                        eff: vec![Codon { concept_id: 512, role: RoleId::Result, qualifiers: vec![] }],
                        warn: vec![],
                    },
                ],
                total_time: None,
                difficulty: 1,
                tools_req: vec![],
            },
            flags: HeaderFlags::default(),
            epistemic_status: None,
            evidence_type: None,
            trust: None,
            epigenetic: None,
        };

        let dna = ku_to_core_dna(&ku).unwrap();
        assert_eq!(dna.header.gene_type, 1); // Procedure

        let wire = dna.encode().unwrap();
        let cbor_wire = crate::encoder::encode_knowledge_unit(&ku).unwrap();

        println!("  Core DNA: {} bytes", wire.len());
        println!("  CBOR:     {} bytes", cbor_wire.len());
        println!("  Reduction: {:.1}x", cbor_wire.len() as f64 / wire.len() as f64);

        let decoded_dna = CoreDna::decode(&wire).unwrap();
        let decoded_ku = core_dna_to_ku(&decoded_dna).unwrap();

        if let Gene::Procedure { steps, difficulty, .. } = &decoded_ku.gene {
            assert_eq!(steps.len(), 2);
            assert_eq!(steps[0].act, 508);
            assert_eq!(steps[0].tgt, 505);
            assert_eq!(steps[0].pre.len(), 1); // precondition preserved
            assert_eq!(steps[1].eff.len(), 1); // effect preserved
            assert_eq!(*difficulty, 1);
        } else {
            panic!("Expected Procedure gene");
        }

        println!("  Procedure bridge: PASSED ✓");
    }

    #[test]
    fn test_auto_detect_format() {
        println!("\n══════════════════════════════════════════════════");
        println!("  TEST: Auto-detect wire format");
        println!("══════════════════════════════════════════════════");

        // Core DNA v6 wire
        let dna = CoreDna::new(0, vec![
            Instruction::Triple { s: 1, p: 2, o: 3 },
        ]);
        let core_wire = dna.encode().unwrap();
        assert_eq!(detect_wire_format(&core_wire), WireFormat::CoreDnaV6);
        println!("  Core DNA detected: ✓");

        // v4/v5 CBOR wire (starts with 0x4B 0x44)
        let cbor_wire = vec![0x4B, 0x44, 0x05, 0x00]; // KD magic
        assert_eq!(detect_wire_format(&cbor_wire), WireFormat::CborV4V5);
        println!("  CBOR v4/v5 detected: ✓");

        // Unknown format
        assert_eq!(detect_wire_format(&[0xFF, 0xFF]), WireFormat::Unknown);
        println!("  Unknown detected: ✓");

        // v6 wire → decode_any → KnowledgeUnit
        let ku = decode_any(&core_wire).unwrap();
        assert_eq!(ku.gene.gene_type(), GeneType::Fact);
        println!("  decode_any(Core DNA): ✓");

        println!("  auto_detect: PASSED ✓");
    }

    #[test]
    fn test_backward_compat_cbor_decode_any() {
        use crate::types::*;
        println!("\n══════════════════════════════════════════════════");
        println!("  TEST: Backward compat — v5 CBOR via decode_any");
        println!("══════════════════════════════════════════════════");

        // Create a KU using the old CBOR encoder
        let ku = KnowledgeUnit {
            codons: vec![
                Codon { concept_id: 100, role: RoleId::Object, qualifiers: vec![] },
            ],
            bonds: vec![],
            gene: Gene::Fact {
                triples: vec![Triple { subject: 100, predicate: 200, object: 300 }],
                certainty: 9000,
                evidence: vec![],
            },
            flags: HeaderFlags::default(),
            epistemic_status: None,
            evidence_type: None,
            trust: None,
            epigenetic: None,
        };

        let cbor_wire = crate::encoder::encode_knowledge_unit(&ku).unwrap();

        // decode_any should auto-detect CBOR and decode
        let decoded = decode_any(&cbor_wire).unwrap();
        assert_eq!(decoded.gene.gene_type(), GeneType::Fact);

        if let Gene::Fact { triples, .. } = &decoded.gene {
            assert_eq!(triples[0].subject, 100);
            assert_eq!(triples[0].predicate, 200);
            assert_eq!(triples[0].object, 300);
        } else {
            panic!("Expected Fact gene from CBOR");
        }

        println!("  CBOR v5 decode_any: PASSED ✓");
    }

    #[test]
    fn test_rocket_systems_encoding() {
        println!("\n══════════════════════════════════════════════════════════════");
        println!("  DEMO: 🚀 Rocket Systems → Core DNA Encoding");
        println!("══════════════════════════════════════════════════════════════");

        let text = "Các hệ thống chính của tên lửa. \
Thân và vỏ tên lửa: Thường được làm từ hợp kim nhôm-liti, titan hoặc vật liệu composite sợi carbon \
để tối ưu hóa trọng lượng nhưng vẫn đảm bảo độ bền cơ học trước áp lực lớn. \
Hệ thống động cơ: Nhiên liệu lỏng: Sử dụng hệ thống bơm phức tạp để đưa nhiên liệu \
(như hydro lỏng) và chất oxy hóa (như oxy lỏng) vào buồng đốt, sinh lực đẩy mạnh. \
Nhiên liệu rắn: Hỗn hợp nhiên liệu và chất oxy hóa được đúc sẵn thành khối rắn, \
cấu trúc đơn giản, tin cậy cao. \
Hệ thống dẫn đường & điều khiển: Bao gồm cảm biến quán tính (IMU), \
con quay hồi chuyển và máy tính bay, giúp điều chỉnh hướng phụt của động cơ \
(thông qua van điều hướng) để tên lửa bay đúng quỹ đạo. \
Khoang tải trọng: Nằm ở phần đầu tên lửa, chứa vệ tinh, thiết bị nghiên cứu \
hoặc đầu đạn tùy theo mục đích sử dụng";

        let text_bytes = text.len();
        println!("\n  📝 Input text: {} bytes (UTF-8)", text_bytes);

        // ─── Approach 1: Tier 1 Parser (automated, offline, no AI) ───
        println!("\n  ╔══════════════════════════════════════════════╗");
        println!("  ║ Approach 1: Tier 1 Parser (automated)       ║");
        println!("  ╚══════════════════════════════════════════════╝");

        let mut dict = crate::text_parser::default_dict();
        // Add rocket-specific concepts to dictionary
        dict.insert("tên lửa", 600);
        dict.insert("rocket", 600);
        dict.insert("thân", 601);
        dict.insert("vỏ", 602);
        dict.insert("hợp kim", 603);
        dict.insert("nhôm", 604);
        dict.insert("titan", 605);
        dict.insert("carbon", 606);
        dict.insert("composite", 607);
        dict.insert("động cơ", 610);
        dict.insert("nhiên liệu", 611);
        dict.insert("hydro", 612);
        dict.insert("oxy", 613);
        dict.insert("buồng đốt", 614);
        dict.insert("lực đẩy", 615);
        dict.insert("bơm", 616);
        dict.insert("dẫn đường", 620);
        dict.insert("điều khiển", 621);
        dict.insert("cảm biến", 622);
        dict.insert("imu", 623);
        dict.insert("con quay hồi chuyển", 624);
        dict.insert("máy tính bay", 625);
        dict.insert("quỹ đạo", 626);
        dict.insert("khoang tải trọng", 630);
        dict.insert("vệ tinh", 631);
        dict.insert("đầu đạn", 632);

        let auto_dna = crate::text_parser::parse_text_to_core_dna(text, &mut dict).unwrap();
        let auto_wire = auto_dna.encode().unwrap();

        println!("  Instructions: {}", auto_dna.len());
        println!("  Wire size:    {} bytes", auto_wire.len());
        for (i, instr) in auto_dna.instructions.iter().enumerate() {
            println!("    [{:2}] {:?}", i, instr);
        }

        // ─── Approach 2: Manual precision encoding (like a powerful AI would) ───
        println!("\n  ╔══════════════════════════════════════════════╗");
        println!("  ║ Approach 2: Manual Precision Encoding        ║");
        println!("  ╚══════════════════════════════════════════════╝");

        // Concept IDs for rocket domain
        const C_ROCKET: ConceptId       = 600;
        const C_BODY: ConceptId         = 601;
        const C_SHELL: ConceptId        = 602;
        const C_AL_LI_ALLOY: ConceptId  = 603;
        const C_TITANIUM: ConceptId     = 605;
        const C_CARBON_COMP: ConceptId  = 607;
        const C_ENGINE_SYS: ConceptId   = 610;
        const C_FUEL: ConceptId         = 611;
        const C_LIQ_HYDROGEN: ConceptId = 612;
        const C_LIQ_OXYGEN: ConceptId   = 613;
        const C_COMBUSTION: ConceptId   = 614;
        const C_THRUST: ConceptId       = 615;
        const C_PUMP: ConceptId         = 616;
        const C_SOLID_FUEL: ConceptId   = 617;
        const C_SIMPLE: ConceptId       = 618;
        const C_RELIABLE: ConceptId     = 619;
        const C_GUIDANCE: ConceptId     = 620;
        const C_CONTROL: ConceptId      = 621;
        const C_IMU: ConceptId          = 623;
        const C_GYROSCOPE: ConceptId    = 624;
        const C_FLIGHT_COMP: ConceptId  = 625;
        const C_TRAJECTORY: ConceptId   = 626;
        const C_THRUST_VEC: ConceptId   = 627;
        const C_PAYLOAD: ConceptId      = 630;
        const C_SATELLITE: ConceptId    = 631;
        const C_WARHEAD: ConceptId      = 632;
        const C_RESEARCH_EQ: ConceptId  = 633;
        const C_LIGHTWEIGHT: ConceptId  = 640;
        const C_STRONG: ConceptId       = 641;
        const C_HIGH_PRESS: ConceptId   = 642;
        const C_MATERIAL: ConceptId     = 643;
        const C_NOSE: ConceptId         = 644;

        // KU#1: Body & Shell (Fact)
        let dna1 = CoreDna::new(0, vec![
            // Thân và vỏ là part of tên lửa
            Instruction::PartOf { part: C_BODY, whole: C_ROCKET },
            Instruction::PartOf { part: C_SHELL, whole: C_ROCKET },
            // Vật liệu: hợp kim nhôm-liti HOẶC titan HOẶC composite carbon
            Instruction::Triple { s: C_BODY, p: C_MATERIAL, o: C_AL_LI_ALLOY },
            Instruction::EnumVal { s: C_MATERIAL, values: vec![C_AL_LI_ALLOY, C_TITANIUM, C_CARBON_COMP] },
            // Tính chất: tối ưu trọng lượng + độ bền cơ học
            Instruction::Quality { s: C_BODY, q: C_LIGHTWEIGHT },
            Instruction::Quality { s: C_BODY, q: C_STRONG },
            // Chịu áp lực lớn
            Instruction::Causal { cause: C_HIGH_PRESS, effect: C_STRONG },
            Instruction::Certainty { level: 9500 },
        ]);
        let wire1 = dna1.encode().unwrap();

        // KU#2: Liquid Fuel Engine (Procedure)
        let dna2 = CoreDna::new(1, vec![
            Instruction::PartOf { part: C_ENGINE_SYS, whole: C_ROCKET },
            Instruction::Step { ord: 0, action: C_PUMP, target: C_FUEL },
            Instruction::Step { ord: 1, action: C_PUMP, target: C_LIQ_HYDROGEN },
            Instruction::Step { ord: 2, action: C_PUMP, target: C_LIQ_OXYGEN },
            Instruction::Step { ord: 3, action: C_COMBUSTION, target: C_THRUST },
            Instruction::Effect { concept: C_THRUST },
            Instruction::Quality { s: C_ENGINE_SYS, q: C_THRUST },
            Instruction::Difficulty { level: 4 }, // phức tạp
        ]);
        let wire2 = dna2.encode().unwrap();

        // KU#3: Solid Fuel (Fact)
        let dna3 = CoreDna::new(0, vec![
            Instruction::Triple { s: C_SOLID_FUEL, p: C_MATERIAL, o: C_FUEL },
            Instruction::Quality { s: C_SOLID_FUEL, q: C_SIMPLE },
            Instruction::Quality { s: C_SOLID_FUEL, q: C_RELIABLE },
            Instruction::Certainty { level: 9000 },
        ]);
        let wire3 = dna3.encode().unwrap();

        // KU#4: Guidance & Control (Fact)
        let dna4 = CoreDna::new(0, vec![
            Instruction::PartOf { part: C_GUIDANCE, whole: C_ROCKET },
            Instruction::PartOf { part: C_CONTROL, whole: C_ROCKET },
            // Thành phần: IMU, con quay hồi chuyển, máy tính bay
            Instruction::EnumVal { s: C_GUIDANCE, values: vec![C_IMU, C_GYROSCOPE, C_FLIGHT_COMP] },
            // Chức năng: điều chỉnh hướng phụt → bay đúng quỹ đạo
            Instruction::Tool { action: C_CONTROL, instrument: C_THRUST_VEC },
            Instruction::Causal { cause: C_THRUST_VEC, effect: C_TRAJECTORY },
            Instruction::Certainty { level: 9500 },
        ]);
        let wire4 = dna4.encode().unwrap();

        // KU#5: Payload Bay (Fact)
        let dna5 = CoreDna::new(0, vec![
            Instruction::PartOf { part: C_PAYLOAD, whole: C_ROCKET },
            Instruction::Located { s: C_PAYLOAD, location: C_NOSE },
            Instruction::EnumVal { s: C_PAYLOAD, values: vec![C_SATELLITE, C_RESEARCH_EQ, C_WARHEAD] },
            Instruction::Certainty { level: 9000 },
        ]);
        let wire5 = dna5.encode().unwrap();

        // Print each KU
        let kus = [
            ("Body & Shell", &dna1, &wire1),
            ("Liquid Fuel Engine", &dna2, &wire2),
            ("Solid Fuel", &dna3, &wire3),
            ("Guidance & Control", &dna4, &wire4),
            ("Payload Bay", &dna5, &wire5),
        ];

        let mut total_manual = 0usize;
        for (name, dna, wire) in &kus {
            println!("\n  KU [{}]", name);
            println!("    Gene type:    {}", dna.header.gene_type);
            println!("    Instructions: {}", dna.len());
            println!("    Wire:         {} bytes", wire.len());
            println!("    Header hex:   {:02X} {:02X}", wire[0], wire[1]);
            total_manual += wire.len();

            // Verify roundtrip
            let decoded = CoreDna::decode(wire).unwrap();
            assert_eq!(decoded.instructions.len(), dna.instructions.len());
        }

        // ─── Summary ───
        println!("\n  ═══════════════════════════════════════════════════════════");
        println!("  📊 SUMMARY: Rocket Systems → Core DNA");
        println!("  ═══════════════════════════════════════════════════════════");
        println!("  Input text (UTF-8):        {} bytes", text_bytes);
        println!("  ─────────────────────────────────────────────────────────");
        println!("  Tier 1 Parser (auto):      {} bytes ({:.1}x vs text)",
            auto_wire.len(),
            if auto_wire.len() < text_bytes { text_bytes as f64 / auto_wire.len() as f64 }
            else { -(auto_wire.len() as f64 / text_bytes as f64) });
        println!("  ─────────────────────────────────────────────────────────");
        println!("  Manual Precision (5 KUs):");
        for (name, _, wire) in &kus {
            println!("    {:24} {} bytes", format!("{}:", name), wire.len());
        }
        println!("  ─────────────────────────────────────────────────────────");
        println!("  Total manual:              {} bytes ({:.1}x smaller than text)",
            total_manual, text_bytes as f64 / total_manual as f64);
        println!("  ═══════════════════════════════════════════════════════════");
        println!("  ✅ Language-agnostic:   ConceptIds only, no Vietnamese");
        println!("  ✅ Machine-queryable:   \"What contains IMU?\" → GUIDANCE");
        println!("  ✅ Precise:             Materials as EnumVal, steps ordered");
        println!("  ✅ Composable:          Bond each KU to 'Falcon 9' KU");
        println!("  ═══════════════════════════════════════════════════════════");

        // Verify manual encoding is smaller than text
        assert!(total_manual < text_bytes,
            "Manual Core DNA ({}) should be smaller than text ({})",
            total_manual, text_bytes);

        println!("\n  test_rocket_systems_encoding: PASSED ✓ 🚀");
    }
}

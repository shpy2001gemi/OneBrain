//! UKRL v4/v5 Decoder — Wire format back to structured components.
//!
//! Decoding pipeline:
//! 1. Validate header (magic, version, flags)
//! 2. Read PAYLOAD_LEN based on version (u16 for v4, u32 for v5)
//! 3. Verify CRC-32 integrity
//! 4. Extract gene type (including EXTENDED mechanism)
//! 5. Return validated payload for CBOR deserialization
//! 6. Optionally deserialize CBOR payload into full KnowledgeUnit
//!
//! ★ v5: Dual-version decoder — accepts both v4 (6-byte header, u16 payload_len)
//! and v5 (8-byte header, u32 payload_len) wire formats.

use crate::error::KuError;
use crate::types::*;

/// Result of decoding a wire-format Knowledge Unit.
#[derive(Debug)]
pub struct DecodedKu {
    /// Wire format version (0x04 or 0x05)
    pub version: u8,
    /// Resolved gene type (including EXTENDED types)
    pub gene_type: GeneType,
    /// Raw flags byte value
    pub flags: u8,
    /// Parsed header flags
    pub header_flags: HeaderFlags,
    /// Payload length from header (u32 for both v4/v5; v4 values fit in u16)
    pub payload_len: u32,
    /// Whether the CRC-32 was valid
    pub crc32_valid: bool,
    /// The raw CBOR payload bytes (for further deserialization).
    /// For EXTENDED gene types, the ext byte is stripped — this is pure CBOR.
    pub payload: Vec<u8>,
}

/// Decode a complete wire-format KU back into validated components.
///
/// ★ v5: Dual-version decoder supporting both v4 and v5 wire formats.
///
/// Wire format v4:
/// ```text
/// MAGIC(0x4B44) | VERSION(0x04) | FLAGS(u8) | PAYLOAD_LEN(u16 BE) | PAYLOAD(CBOR) | CRC32(u32 BE)
/// Header: 6 bytes
/// ```
///
/// Wire format v5:
/// ```text
/// MAGIC(0x4B44) | VERSION(0x05) | FLAGS(u8) | PAYLOAD_LEN(u32 BE) | PAYLOAD(CBOR) | CRC32(u32 BE)
/// Header: 8 bytes
/// ```
///
/// Returns a `DecodedKu` with the validated header fields and the raw CBOR
/// payload bytes ready for deserialization with `ciborium`.
pub fn decode_knowledge_unit(wire: &[u8]) -> Result<DecodedKu, KuError> {
    // 1. Validate minimum size (smallest header is v4 = 6 + 4 CRC = 10)
    if wire.len() < 10 {
        return Err(KuError::PayloadTruncated { expected: 10, got: wire.len() });
    }

    // 2. Check MAGIC bytes
    if wire[0] != MAGIC[0] || wire[1] != MAGIC[1] {
        return Err(KuError::InvalidMagic([wire[0], wire[1]]));
    }

    // 3. Determine version and decode accordingly
    let version = wire[2];
    let (header_size, payload_len, flags_byte): (usize, u32, u8) = match version {
        // ─── v4 path: 6-byte header, u16 PAYLOAD_LEN ───
        VERSION_V4 => {
            if wire.len() < 10 {
                return Err(KuError::PayloadTruncated { expected: 10, got: wire.len() });
            }
            let flags = wire[3];
            let plen = ((wire[4] as u16) << 8) | (wire[5] as u16);
            (6, plen as u32, flags)
        }
        // ─── v5 path: 8-byte header, u32 PAYLOAD_LEN ───
        VERSION => {
            if wire.len() < 12 {
                return Err(KuError::PayloadTruncated { expected: 12, got: wire.len() });
            }
            let flags = wire[3];
            let plen = u32::from_be_bytes([wire[4], wire[5], wire[6], wire[7]]);
            (8, plen, flags)
        }
        _ => return Err(KuError::UnsupportedVersion(version)),
    };

    // 4. Parse FLAGS (gene_type from bits 5-7)
    let (header_flags, gene_base) = HeaderFlags::from_byte(flags_byte);

    // 5. Verify total size >= header + payload + CRC
    let expected_total = header_size + payload_len as usize + 4;
    if wire.len() < expected_total {
        return Err(KuError::PayloadTruncated { expected: expected_total, got: wire.len() });
    }

    // 6. Compute CRC-32 over header + payload, compare with stored CRC
    let crc_offset = header_size + payload_len as usize;
    let stored_crc = u32::from_be_bytes([
        wire[crc_offset],
        wire[crc_offset + 1],
        wire[crc_offset + 2],
        wire[crc_offset + 3],
    ]);
    let computed_crc = crc32fast::hash(&wire[..crc_offset]);

    if stored_crc != computed_crc {
        return Err(KuError::CrcMismatch { stored: stored_crc, computed: computed_crc });
    }

    // 7. If gene_type == EXTENDED (7), read ext byte from payload start
    let raw_payload = &wire[header_size..crc_offset];
    let (gene_type, cbor_payload) = if gene_base == 7 {
        if raw_payload.is_empty() {
            return Err(KuError::InvalidData(
                "EXTENDED gene type but payload is empty".into(),
            ));
        }
        let ext_byte = raw_payload[0];
        let gt = GeneType::from_wire(7, Some(ext_byte))
            .ok_or(KuError::UnknownGeneType(ext_byte))?;
        // Strip the ext byte — remaining bytes are pure CBOR
        (gt, &raw_payload[1..])
    } else {
        let gt = GeneType::from_wire(gene_base, None)
            .ok_or(KuError::UnknownGeneType(gene_base))?;
        (gt, raw_payload)
    };

    // 8. Return DecodedKu
    Ok(DecodedKu {
        version,
        gene_type,
        flags: flags_byte,
        header_flags,
        payload_len,
        crc32_valid: true,
        payload: cbor_payload.to_vec(),
    })
}

/// Internal payload structure for CBOR deserialization (mirrors KuPayload in encoder).
#[derive(serde::Deserialize)]
struct DecodedPayload {
    #[serde(rename = "cd")]
    codons: Vec<Codon>,
    #[serde(rename = "bd")]
    bonds: Vec<Bond>,
    #[serde(rename = "g")]
    gene: Gene,
    #[serde(rename = "es", default)]
    epistemic_status: Option<EpistemicStatus>,
    #[serde(rename = "et", default)]
    evidence_type: Option<EvidenceType>,
    /// ★ v4: Trust & Epistemic section
    #[serde(rename = "tr", default)]
    trust: Option<TrustSection>,
    /// ★ v4: Layer 4 Epigenetic metadata
    #[serde(rename = "ep", default)]
    epigenetic: Option<EpigeneticSection>,
}

/// Decode wire format AND deserialize CBOR payload into a full KnowledgeUnit.
///
/// ★ v5: Supports both v4 and v5 wire formats via dual-version decoder.
///
/// This performs both wire-level validation (magic, version, CRC) and
/// CBOR deserialization of the payload into all KU fields including
/// trust and epigenetic sections.
///
/// Returns `(DecodedKu, KnowledgeUnit)` on success:
/// - `DecodedKu`: wire-level metadata (version, gene type, flags, etc.)
/// - `KnowledgeUnit`: fully deserialized KU with all layers
pub fn decode_full_knowledge_unit(wire: &[u8]) -> Result<(DecodedKu, KnowledgeUnit), KuError> {
    let decoded = decode_knowledge_unit(wire)?;

    // Deserialize the CBOR payload into DecodedPayload
    let payload: DecodedPayload = ciborium::from_reader(&decoded.payload[..])
        .map_err(|e| KuError::CborDecode(e.to_string()))?;

    let ku = KnowledgeUnit {
        codons: payload.codons,
        bonds: payload.bonds,
        gene: payload.gene,
        flags: decoded.header_flags.clone(),
        epistemic_status: payload.epistemic_status,
        evidence_type: payload.evidence_type,
        trust: payload.trust,
        epigenetic: payload.epigenetic,
    };

    Ok((decoded, ku))
}

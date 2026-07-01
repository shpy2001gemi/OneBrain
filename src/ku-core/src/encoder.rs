//! UKRL v4/v5 Encoder — Layers 1-5 to CBOR wire format.
//!
//! Encoding pipeline:
//! 1. Encode codons → CBOR
//! 2. Encode bonds → CBOR
//! 3. Encode gene → CBOR (includes Composite Gene in v5)
//! 4. Encode trust + epigenetic (Layer 4-5) → CBOR
//! 5. Assemble: HEADER(8B) + PAYLOAD(CBOR) + CRC32(4B)
//!
//! Wire format v5 changes:
//! - VERSION: 0x04 → 0x05
//! - PAYLOAD_LEN: u16 (2B) → u32 (4B)
//! - Header: 6B → 8B

use crate::error::KuError;
use crate::types::*;


/// Encode a single Codon to CBOR bytes.
pub fn encode_codon(codon: &Codon) -> Result<Vec<u8>, KuError> {
    let mut buf = Vec::new();
    ciborium::into_writer(codon, &mut buf)
        .map_err(|e| KuError::CborEncode(e.to_string()))?;
    Ok(buf)
}

/// Encode a slice of Codons to CBOR bytes.
pub fn encode_codons(codons: &[Codon]) -> Result<Vec<u8>, KuError> {
    let mut buf = Vec::new();
    ciborium::into_writer(&codons, &mut buf)
        .map_err(|e| KuError::CborEncode(e.to_string()))?;
    Ok(buf)
}

/// Encode a single Bond to CBOR bytes.
pub fn encode_bond(bond: &Bond) -> Result<Vec<u8>, KuError> {
    let mut buf = Vec::new();
    ciborium::into_writer(bond, &mut buf)
        .map_err(|e| KuError::CborEncode(e.to_string()))?;
    Ok(buf)
}

/// Encode a Gene to CBOR bytes.
///
/// Uses the tagged enum serialization — includes the gene type discriminator.
pub fn encode_gene(gene: &Gene) -> Result<Vec<u8>, KuError> {
    let mut buf = Vec::new();
    ciborium::into_writer(gene, &mut buf)
        .map_err(|e| KuError::CborEncode(e.to_string()))?;
    Ok(buf)
}

/// Encode a TrustSection to standalone CBOR bytes (for size measurement).
pub fn encode_trust(trust: &TrustSection) -> Result<Vec<u8>, KuError> {
    let mut buf = Vec::new();
    ciborium::into_writer(trust, &mut buf)
        .map_err(|e| KuError::CborEncode(e.to_string()))?;
    Ok(buf)
}

/// Encode an EpigeneticSection to standalone CBOR bytes (for size measurement).
pub fn encode_epigenetic(epigenetic: &EpigeneticSection) -> Result<Vec<u8>, KuError> {
    let mut buf = Vec::new();
    ciborium::into_writer(epigenetic, &mut buf)
        .map_err(|e| KuError::CborEncode(e.to_string()))?;
    Ok(buf)
}

/// Encode a complete KnowledgeUnit to v5 wire format.
///
/// Wire layout (v5):
/// ```text
/// [MAGIC: 2B] [VERSION: 1B] [FLAGS: 1B] [PAYLOAD_LEN: 4B] [PAYLOAD: CBOR] [CRC32: 4B]
/// ```
///
/// The CBOR payload includes all populated layers:
/// - Layer 1-2: codons + bonds
/// - Layer 3: gene content
/// - Layer 4: trust section + epigenetic metadata (when present)
/// - Layer 5: CRC32 integrity
///
/// Returns the full wire-encoded bytes.
pub fn encode_knowledge_unit(ku: &KnowledgeUnit) -> Result<Vec<u8>, KuError> {
    let gene_type = ku.gene.gene_type();
    let (_base, ext) = gene_type.wire_encoding();

    // Encode payload as CBOR
    let mut payload = Vec::new();

    // If EXTENDED gene type (base=7), prepend the ext byte
    if let Some(ext_byte) = ext {
        payload.push(ext_byte);
    }

    // Encode the KU content (codons + bonds + gene + trust + epigenetic) as CBOR
    let ku_payload = KuPayload {
        codons: &ku.codons,
        bonds: &ku.bonds,
        gene: &ku.gene,
        epistemic_status: ku.epistemic_status,
        evidence_type: ku.evidence_type,
        trust: ku.trust.as_ref(),
        epigenetic: ku.epigenetic.as_ref(),
    };
    let mut cbor_buf = Vec::new();
    ciborium::into_writer(&ku_payload, &mut cbor_buf)
        .map_err(|e| KuError::CborEncode(e.to_string()))?;
    payload.extend_from_slice(&cbor_buf);

    if payload.len() > u32::MAX as usize {
        return Err(KuError::PayloadTooLargeV5 { size: payload.len() });
    }
    let payload_len = payload.len() as u32;

    // Build header
    let flags_byte = ku.flags.to_byte(gene_type);
    let mut wire = Vec::with_capacity(8 + payload.len() + 4);

    // Header (8 bytes — v5)
    wire.extend_from_slice(&MAGIC);       // 2B: "KD"
    wire.push(VERSION);                    // 1B: 0x05
    wire.push(flags_byte);                 // 1B: flags + gene_type
    wire.extend_from_slice(&payload_len.to_be_bytes()); // 4B: u32 BE ★ v5

    // Payload
    wire.extend_from_slice(&payload);

    // CRC-32 over header + payload
    let crc = crc32fast::hash(&wire);
    wire.extend_from_slice(&crc.to_be_bytes());

    Ok(wire)
}

/// Create a KU with full Layer 1-5 data.
///
/// This is a convenience function that assembles a complete KnowledgeUnit
/// with all layers populated and encodes it to wire format.
///
/// # Arguments
/// * `gene_type` - The gene type (determines FLAGS bits 5-7)
/// * `codons` - Layer 1 concept codons
/// * `bonds` - Layer 2 relation bonds
/// * `gene` - Layer 3 content gene
/// * `trust` - Layer 4 trust & epistemic section
/// * `epigenetic` - Layer 4 epigenetic metadata
///
/// # Returns
/// Wire-encoded bytes `[HEADER(8B) | PAYLOAD(CBOR) | CRC32(4B)]`
pub fn create_full_ku(
    codons: Vec<Codon>,
    bonds: Vec<Bond>,
    gene: Gene,
    trust: TrustSection,
    epigenetic: EpigeneticSection,
) -> Result<Vec<u8>, KuError> {
    let ku = KnowledgeUnit {
        codons,
        bonds,
        gene,
        flags: HeaderFlags::default(),
        epistemic_status: Some(trust.epistemic_status),
        evidence_type: Some(trust.evidence_type),
        trust: Some(trust),
        epigenetic: Some(epigenetic),
    };
    encode_knowledge_unit(&ku)
}

/// Internal payload structure for CBOR serialization.
///
/// Includes all layers: codons, bonds, gene, and optional trust/epigenetic sections.
#[derive(serde::Serialize)]
struct KuPayload<'a> {
    #[serde(rename = "cd")]
    codons: &'a [Codon],
    #[serde(rename = "bd")]
    bonds: &'a [Bond],
    #[serde(rename = "g")]
    gene: &'a Gene,
    #[serde(rename = "es", default, skip_serializing_if = "Option::is_none")]
    epistemic_status: Option<EpistemicStatus>,
    #[serde(rename = "et", default, skip_serializing_if = "Option::is_none")]
    evidence_type: Option<EvidenceType>,
    /// ★ v4: Trust & Epistemic section (spec §8)
    #[serde(rename = "tr", skip_serializing_if = "Option::is_none")]
    trust: Option<&'a TrustSection>,
    /// ★ v4: Layer 4 Epigenetic metadata (spec §6)
    #[serde(rename = "ep", skip_serializing_if = "Option::is_none")]
    epigenetic: Option<&'a EpigeneticSection>,
}

/// Print size breakdown for a KU encoding (useful during development).
///
/// Only available in test builds. Uses `if let` to gracefully handle
/// encoding failures instead of panicking.
#[cfg(test)]
pub fn print_size_breakdown(ku: &KnowledgeUnit) {
    let codons_size = if let Ok(b) = encode_codons(&ku.codons) { b.len() } else { return };
    let bonds_sizes: Vec<usize> = ku.bonds.iter()
        .filter_map(|b| encode_bond(b).ok().map(|v| v.len()))
        .collect();
    let gene_size = if let Ok(b) = encode_gene(&ku.gene) { b.len() } else { return };
    let wire = if let Ok(w) = encode_knowledge_unit(ku) { w } else { return };

    println!("═══ KU Size Breakdown ═══");
    println!("  Gene type:       {:?}", ku.gene.gene_type());
    println!("  Layer 1 (codons): {} bytes ({} codons)", codons_size, ku.codons.len());
    for (i, size) in bonds_sizes.iter().enumerate() {
        println!("  Layer 2 (bond {}): {} bytes", i, size);
    }
    println!("  Layer 2 total:    {} bytes ({} bonds)", bonds_sizes.iter().sum::<usize>(), ku.bonds.len());
    println!("  Layer 3 (gene):   {} bytes", gene_size);
    println!("  ─────────────────────────");
    println!("  Wire total:       {} bytes", wire.len());
    println!("    Header:         8 bytes (v5)");
    println!("    Payload:        {} bytes", wire.len() - 12);
    println!("    CRC-32:         4 bytes");
    println!("  ═══════════════════════════");
}

/// Print full Layer 1-5 size breakdown for a KU encoding.
///
/// Shows individual sizes for each layer including trust and epigenetic sections.
/// Returns `Err` if any encoding step fails.
pub fn size_breakdown_full(ku: &KnowledgeUnit) -> Result<String, KuError> {
    let codons_size = encode_codons(&ku.codons)?.len();
    let bonds_sizes: Vec<usize> = ku.bonds.iter()
        .map(|b| encode_bond(b).map(|v| v.len()))
        .collect::<Result<Vec<_>, _>>()?;
    let bonds_total: usize = bonds_sizes.iter().sum();
    let gene_size = encode_gene(&ku.gene)?.len();
    let trust_size = match ku.trust.as_ref() {
        Some(t) => encode_trust(t)?.len(),
        None => 0,
    };
    let epigenetic_size = match ku.epigenetic.as_ref() {
        Some(e) => encode_epigenetic(e)?.len(),
        None => 0,
    };
    let wire = encode_knowledge_unit(ku)?;

    let mut report = String::new();
    report.push_str("=== KU Size Report ===\n");
    report.push_str(&format!("Layer 1 (Header):     8B (v5)\n"));
    report.push_str(&format!("Layer 1 (Codons):     {}B ({} codons)\n", codons_size, ku.codons.len()));
    report.push_str(&format!("Layer 2 (Bonds):      {}B ({} bonds)\n", bonds_total, ku.bonds.len()));
    report.push_str(&format!("Layer 3 (Gene):       {}B ({:?})\n", gene_size, ku.gene.gene_type()));
    report.push_str(&format!("Layer 4 (Trust):      {}B\n", trust_size));
    report.push_str(&format!("Layer 4 (Epigenetic): {}B\n", epigenetic_size));
    report.push_str(&format!("Layer 5 (CRC):        4B\n"));
    report.push_str("---\n");
    report.push_str(&format!("Total wire:           {}B\n", wire.len()));
    Ok(report)
}


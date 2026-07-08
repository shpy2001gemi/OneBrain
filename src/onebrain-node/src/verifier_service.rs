//! Verifier service — cross-node KU verification.
//!
//! When a peer asks us to verify a KU, we re-encode the source text
//! using our local AI and compare the CoreDna structures. This is
//! the basis for encoding consensus in the OBP protocol.

use ku_ai::OllamaBackend;
use ku_core::KuRuntime;
use ku_core::encoding_verifier::core_dna_agreement;
use ku_core::text_parser::{ConceptDict, default_dict};
use ku_encoder::{AiEncoder, EncoderConfig};

/// Result of a verification check.
#[derive(Debug, Clone)]
pub struct VerifyResult {
    /// Semantic agreement score (0.0 – 1.0).
    pub agreement_score: f64,
    /// Whether the KU passes verification (agreement ≥ 0.6).
    pub verified: bool,
    /// Whether the gene types matched.
    pub gene_type_match: bool,
    /// Human-readable details.
    pub details: String,
}

/// Verify a KU by re-encoding the source text and comparing CoreDna.
///
/// Pipeline:
/// 1. Create a fresh AiEncoder with local Ollama
/// 2. Encode source_text → get our own wire_bytes
/// 3. Decode both wire_bytes → KuRuntime → CoreDna
/// 4. Compare using `core_dna_agreement()` (gene_type 0.3 + opcode 0.3 + concept 0.4)
/// 5. Return agreement score and verdict
pub async fn verify_ku(
    source_text: &str,
    original_wire_bytes: &[u8],
    ollama_url: &str,
    model: &str,
) -> VerifyResult {
    // Decode the original wire bytes
    let original_ku = match KuRuntime::from_wire(original_wire_bytes.to_vec()) {
        Ok(ku) => ku,
        Err(e) => {
            return VerifyResult {
                agreement_score: 0.0,
                verified: false,
                gene_type_match: false,
                details: format!("Failed to decode original wire bytes: {}", e),
            };
        }
    };

    // Create a fresh encoder with local Ollama
    let encoder_backend = match OllamaBackend::new(ollama_url, model, "nomic-embed-text", 120) {
        Ok(b) => b,
        Err(e) => {
            return VerifyResult {
                agreement_score: 0.0,
                verified: false,
                gene_type_match: false,
                details: format!("Failed to create AI backend: {}", e),
            };
        }
    };

    let dict: ConceptDict = default_dict();
    let encoder = AiEncoder::new(
        Box::new(encoder_backend),
        dict,
        EncoderConfig::default(),
    );

    // Re-encode the source text
    let our_result = match encoder.encode(source_text).await {
        Ok(r) => r,
        Err(e) => {
            return VerifyResult {
                agreement_score: 0.0,
                verified: false,
                gene_type_match: false,
                details: format!("Failed to re-encode source text: {}", e),
            };
        }
    };

    if our_result.wire_bytes.is_empty() {
        return VerifyResult {
            agreement_score: 0.0,
            verified: false,
            gene_type_match: false,
            details: "Re-encoding produced no KUs".into(),
        };
    }

    // Decode our re-encoded wire bytes
    let our_ku = match KuRuntime::from_wire(our_result.wire_bytes[0].clone()) {
        Ok(ku) => ku,
        Err(e) => {
            return VerifyResult {
                agreement_score: 0.0,
                verified: false,
                gene_type_match: false,
                details: format!("Failed to decode our re-encoded wire bytes: {}", e),
            };
        }
    };

    // Compare CoreDna structures using ku-core's agreement function
    let score = core_dna_agreement(&original_ku.dna, &our_ku.dna);
    let gene_type_match = original_ku.dna.header.gene_type == our_ku.dna.header.gene_type;

    let details = format!(
        "Gene type: {} vs {} ({}), agreement: {:.1}%, our instructions: {}, original instructions: {}",
        original_ku.dna.header.gene_type,
        our_ku.dna.header.gene_type,
        if gene_type_match { "match" } else { "mismatch" },
        score * 100.0,
        our_ku.dna.instructions.len(),
        original_ku.dna.instructions.len(),
    );

    VerifyResult {
        agreement_score: score as f64,
        verified: score >= 0.6,
        gene_type_match,
        details,
    }
}

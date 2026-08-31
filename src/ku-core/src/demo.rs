//! # End-to-End Demo — Registry ↔ Encoder Integration
//!
//! Demonstrates the FULL pipeline:
//! 1. ConceptID Registry (simulated in-memory)
//! 2. Concept resolution from labels
//! 3. KU creation with codons, bonds, trust, epigenetic
//! 4. Wire encoding (MAGIC + VERSION + FLAGS + PAYLOAD + CRC32)
//! 5. Wire decoding + CBOR deserialization
//! 6. Roundtrip verification

use std::collections::HashMap;

use crate::decoder::decode_full_knowledge_unit;
use crate::encoder::{encode_knowledge_unit, size_breakdown_full};
use crate::types::*;
use crate::varint::encode_varint;

// ============================================================================
// Part 1: ConceptID Registry Simulation
// ============================================================================

/// Metadata for a registered concept.
#[derive(Debug, Clone)]
pub struct ConceptEntry {
    id: u64,
    canonical_label: String,
    domain: String,
    tier: u8,
}

/// Simulated Concept Registry (in production: Prolly Tree + CRDT).
///
/// Provides a bootstrap set of ~50 core concepts across multiple varint
/// tiers, enabling demonstration of the full encoding pipeline.
pub struct ConceptRegistry {
    concepts: HashMap<u64, ConceptEntry>,
    labels: HashMap<String, Vec<u64>>,
}

impl ConceptRegistry {
    /// Create a registry bootstrapped with ~50 core concepts.
    ///
    /// # Tier layout
    /// - **Tier 0** (1-byte varint, 0–127): Fundamental primitives
    /// - **Tier 1** (2-byte varint, 128–16,511): Common concepts
    /// - **Tier 2** (3-byte varint, 16,512–2,113,663): Standard concepts
    pub fn new_with_bootstrap() -> Self {
        let mut reg = ConceptRegistry {
            concepts: HashMap::new(),
            labels: HashMap::new(),
        };

        // ── Tier 0: Universal Primitives (1-byte varint) ──
        reg.insert(1, "entity", "meta", 0);
        reg.insert(2, "property", "meta", 0);
        reg.insert(3, "relation", "meta", 0);
        reg.insert(4, "quantity", "meta", 0);
        reg.insert(5, "time", "meta", 0);
        reg.insert(6, "space", "meta", 0);
        reg.insert(7, "cause", "meta", 0);
        reg.insert(8, "effect", "meta", 0);
        reg.insert(9, "truth", "meta", 0);
        reg.insert(10, "false", "meta", 0);
        reg.insert(11, "observation", "meta", 0);
        reg.insert(12, "action", "meta", 0);
        reg.insert(13, "state", "meta", 0);
        reg.insert(14, "process", "meta", 0);
        reg.insert(15, "event", "meta", 0);
        reg.insert(16, "light", "physics", 0);

        // ── Tier 1: Common Concepts (2-byte varint) ──
        // Natural phenomena
        reg.insert(128, "water", "chemistry", 1);
        reg.insert(129, "fire", "chemistry", 1);
        reg.insert(130, "earth", "geology", 1);
        reg.insert(131, "air", "chemistry", 1);

        // Physical properties
        reg.insert(132, "temperature", "physics", 1);
        reg.insert(133, "boiling_point", "physics", 1);
        reg.insert(134, "pressure", "physics", 1);

        // Living things
        reg.insert(135, "human", "biology", 1);
        reg.insert(136, "animal", "biology", 1);
        reg.insert(137, "plant", "biology", 1);

        // Domains
        reg.insert(140, "physics", "science", 1);
        reg.insert(141, "chemistry", "science", 1);
        reg.insert(142, "biology", "science", 1);
        reg.insert(143, "math", "science", 1);

        // Nature scenes
        reg.insert(150, "sunrise", "nature", 1);
        reg.insert(151, "sunset", "nature", 1);
        reg.insert(152, "ocean", "nature", 1);
        reg.insert(153, "mountain", "nature", 1);

        // Emotions
        reg.insert(160, "joy", "emotion", 1);
        reg.insert(161, "sadness", "emotion", 1);
        reg.insert(162, "fear", "emotion", 1);
        reg.insert(163, "wonder", "emotion", 1);
        reg.insert(164, "awe", "emotion", 1);

        // Media / arts
        reg.insert(170, "movie", "media", 1);
        reg.insert(171, "book", "media", 1);
        reg.insert(172, "music", "media", 1);
        reg.insert(173, "painting", "media", 1);

        // Paranormal
        reg.insert(180, "ufo", "paranormal", 1);
        reg.insert(181, "ghost", "paranormal", 1);
        reg.insert(182, "bigfoot", "paranormal", 1);

        // Physics constants / math
        reg.insert(190, "energy", "physics", 1);
        reg.insert(191, "mass", "physics", 1);
        reg.insert(192, "light_speed", "physics", 1);
        reg.insert(193, "equation", "math", 1);

        // Additional concepts
        reg.insert(200, "galaxy", "astronomy", 1);
        reg.insert(201, "rotation", "physics", 1);
        reg.insert(202, "dark_matter", "physics", 1);
        reg.insert(203, "night_sky", "nature", 1);
        reg.insert(204, "experiment", "science", 1);
        reg.insert(205, "confirmation", "science", 1);
        reg.insert(206, "celsius", "physics", 1);
        reg.insert(207, "color", "perception", 1);
        reg.insert(208, "horizon", "nature", 1);
        reg.insert(209, "warmth", "perception", 1);

        // ── Tier 2: Standard Concepts (3-byte varint) ──
        reg.insert(20_000, "photosynthesis", "biology", 2);
        reg.insert(20_001, "mitochondria", "biology", 2);
        reg.insert(20_002, "general_relativity", "physics", 2);

        reg
    }

    fn insert(&mut self, id: u64, label: &str, domain: &str, tier: u8) {
        let entry = ConceptEntry {
            id,
            canonical_label: label.to_string(),
            domain: domain.to_string(),
            tier,
        };
        self.concepts.insert(id, entry);
        self.labels.entry(label.to_string()).or_default().push(id);
    }

    /// Resolve a label to its ConceptID (returns the first match).
    pub fn resolve(&self, label: &str) -> Option<u64> {
        self.labels.get(label).and_then(|ids| ids.first().copied())
    }

    /// Get a concept entry by ID.
    pub fn get(&self, id: u64) -> Option<&ConceptEntry> {
        self.concepts.get(&id)
    }

    /// Get the canonical label for a concept ID.
    pub fn label(&self, id: u64) -> &str {
        self.concepts
            .get(&id)
            .map(|e| e.canonical_label.as_str())
            .unwrap_or("???")
    }

    /// Total number of registered concepts.
    pub fn len(&self) -> usize {
        self.concepts.len()
    }
}

// ============================================================================
// Part 2: KU Creation Pipeline
// ============================================================================

/// Returns the varint wire size for a ConceptID.
fn varint_size(id: u64) -> usize {
    encode_varint(id).unwrap().len()
}

/// Returns tier name string for a varint size.
fn tier_name(id: u64) -> &'static str {
    match varint_size(id) {
        1 => "T0(1B)",
        2 => "T1(2B)",
        3 => "T2(3B)",
        4 => "T3(4B)",
        5 => "T3+(5B)",
        _ => "T?(??)",
    }
}

/// Build a KU from structured input with concept resolution.
///
/// Takes concept labels + roles, resolves them through the registry,
/// and creates a fully assembled KnowledgeUnit ready for encoding.
fn build_ku(
    codons: Vec<Codon>,
    bonds: Vec<Bond>,
    gene: Gene,
    trust: Option<TrustSection>,
    epigenetic: Option<EpigeneticSection>,
) -> KnowledgeUnit {
    let epistemic_status = trust.as_ref().map(|t| t.epistemic_status);
    let evidence_type = trust.as_ref().map(|t| t.evidence_type);
    KnowledgeUnit {
        codons,
        bonds,
        gene,
        flags: HeaderFlags::default(),
        epistemic_status,
        evidence_type,
        trust,
        epigenetic,
    }
}

/// Encode → Decode → Verify roundtrip. Returns (wire_bytes, decoded_ku).
fn roundtrip(ku: &KnowledgeUnit) -> (Vec<u8>, KnowledgeUnit) {
    let wire = encode_knowledge_unit(ku).expect("encoding failed");
    let (decoded, ku_back) = decode_full_knowledge_unit(&wire).expect("decoding failed");
    assert!(decoded.crc32_valid, "CRC32 must be valid after roundtrip");
    (wire, ku_back)
}

// ============================================================================
// Part 3: Complete E2E Flow Tests
// ============================================================================

#[test]
fn demo_e2e_fact_creation() {
    println!("\n============================================================");
    println!("  SCENARIO: Scientist records \"Water boils at 100°C\"");
    println!("============================================================\n");

    // Step 1: Initialize concept registry
    let registry = ConceptRegistry::new_with_bootstrap();
    println!(
        "[Step 1] Registry initialized with {} concepts",
        registry.len()
    );

    // Step 2: Resolve concepts from text
    let water_id = registry.resolve("water").unwrap(); // 128
    let boiling_id = registry.resolve("boiling_point").unwrap(); // 133
    let temp_id = registry.resolve("temperature").unwrap(); // 132

    println!("[Step 2] Concept resolution:");
    println!(
        "  water       → CID {} {} varint={:02X?}",
        water_id,
        tier_name(water_id),
        encode_varint(water_id).unwrap()
    );
    println!(
        "  boiling_point → CID {} {} varint={:02X?}",
        boiling_id,
        tier_name(boiling_id),
        encode_varint(boiling_id).unwrap()
    );
    println!(
        "  temperature → CID {} {} varint={:02X?}",
        temp_id,
        tier_name(temp_id),
        encode_varint(temp_id).unwrap()
    );

    // Step 3: Create codons (Layer 1)
    let codons = vec![
        Codon {
            concept_id: water_id,
            role: RoleId::Agent,
            qualifiers: vec![],
        },
        Codon {
            concept_id: boiling_id,
            role: RoleId::Quality,
            qualifiers: vec![],
        },
        Codon {
            concept_id: temp_id,
            role: RoleId::Quantity,
            qualifiers: vec![
                Qualifier {
                    key: "unit".into(),
                    value: QualifierValue::Text("CELSIUS".into()),
                },
                Qualifier {
                    key: "val".into(),
                    value: QualifierValue::Integer(100),
                },
            ],
        },
    ];
    println!("[Step 3] Created {} codons", codons.len());

    // Step 4: Create Fact gene (Layer 3) with SPO triple
    let gene = Gene::Fact {
        triples: vec![Triple {
            subject: water_id,
            predicate: boiling_id,
            object: temp_id,
        }],
        certainty: 9800, // 0.98
        evidence: vec![],
    };
    println!("[Step 4] Gene: Fact with 1 triple, certainty=0.98");

    // Step 5: Create trust section (Layer 4 Trust)
    let trust = TrustSection {
        epistemic_status: EpistemicStatus::Evidence,
        evidence_type: EvidenceType::Experimental,
        verification_level: 3, // expert-verified
        corroboration_count: 42,
        challenge_count: 0,
        error_susceptibility: 0,      // no known biases
        trust_score: 9500,            // 0.95
        confidence: 8500,             // 0.85
        domain_codes: vec![140, 141], // physics, chemistry
        verifications: vec![],
        challenges: vec![],
        ..Default::default()
    };
    println!("[Step 5] Trust: Evidence/Experimental, score=0.95, corroborations=42");

    // Step 6: Assemble KU
    let ku = build_ku(codons, vec![], gene, Some(trust), None);
    println!("[Step 6] KU assembled (no bonds, no epigenetic for minimal demo)");

    // Step 7: Encode to wire format
    let wire = encode_knowledge_unit(&ku).unwrap();
    println!("[Step 7] Wire format: {} bytes total", wire.len());
    println!("  Header:  {:02X?}", &wire[..8]);
    println!(
        "  Magic:   0x{:02X}{:02X} = \"{}{}\"",
        wire[0], wire[1], wire[0] as char, wire[1] as char
    );
    println!("  Version: 0x{:02X} (v{})", wire[2], wire[2]);
    println!("  Flags:   0b{:08b}", wire[3]);
    let payload_len = u32::from_be_bytes([wire[4], wire[5], wire[6], wire[7]]);
    println!("  PayloadLen: {} bytes", payload_len);
    let crc_offset = 8 + payload_len as usize;
    println!("  CRC32:   {:02X?}", &wire[crc_offset..crc_offset + 4]);

    // Step 8: Verify wire format
    assert_eq!(&wire[0..2], &[0x4B, 0x44], "MAGIC must be 'KD'");
    assert_eq!(wire[2], 0x05, "VERSION must be 5");
    println!("[Step 8] ✓ Wire format header verified");

    // Step 9: Decode back
    let (decoded, ku_back) = decode_full_knowledge_unit(&wire).unwrap();
    assert!(decoded.crc32_valid);
    assert_eq!(decoded.gene_type, GeneType::Fact);
    println!(
        "[Step 9] ✓ Decoded: gene_type={:?}, CRC valid={}",
        decoded.gene_type, decoded.crc32_valid
    );

    // Step 10: Verify concepts survived roundtrip
    assert_eq!(ku_back.codons[0].concept_id, water_id);
    assert_eq!(ku_back.codons[1].concept_id, boiling_id);
    assert_eq!(ku_back.codons[2].concept_id, temp_id);
    println!("[Step 10] ✓ Concept IDs survived roundtrip");

    // Step 11: Verify trust survived roundtrip
    let trust_back = ku_back.trust.as_ref().unwrap();
    assert_eq!(trust_back.epistemic_status, EpistemicStatus::Evidence);
    assert_eq!(trust_back.trust_score, 9500);
    assert_eq!(trust_back.corroboration_count, 42);
    println!("[Step 11] ✓ Trust section survived roundtrip");

    // Step 12: Print human-readable summary
    println!("\n┌─── Decoded KU Summary ───────────────────────┐");
    println!("│ Gene Type:      {:?}", decoded.gene_type);
    println!(
        "│ Codons:         {} ({}, {}, {})",
        ku_back.codons.len(),
        registry.label(ku_back.codons[0].concept_id),
        registry.label(ku_back.codons[1].concept_id),
        registry.label(ku_back.codons[2].concept_id)
    );
    println!("│ Trust Status:   {:?}", trust_back.epistemic_status);
    println!(
        "│ Trust Score:    {}/10000 ({:.2}%)",
        trust_back.trust_score,
        trust_back.trust_score as f64 / 100.0
    );
    println!("│ Corroborations: {}", trust_back.corroboration_count);
    println!("│ CRC Valid:      {}", decoded.crc32_valid);
    println!("│ Wire Size:      {} bytes", wire.len());
    println!("└──────────────────────────────────────────────┘");

    // Print full size breakdown
    println!("\n{}", size_breakdown_full(&ku).unwrap());
}

#[test]
fn demo_e2e_testimony() {
    println!("\n============================================================");
    println!("  SCENARIO: Person reports seeing strange lights (UFO)");
    println!("============================================================\n");

    let registry = ConceptRegistry::new_with_bootstrap();

    let ufo_id = registry.resolve("ufo").unwrap(); // 180
    let light_id = registry.resolve("light").unwrap(); // 16
    let observation_id = registry.resolve("observation").unwrap(); // 11
    let night_sky_id = registry.resolve("night_sky").unwrap(); // 203

    println!(
        "Concepts: ufo={} {}, light={} {}, observation={} {}, night_sky={} {}",
        ufo_id,
        tier_name(ufo_id),
        light_id,
        tier_name(light_id),
        observation_id,
        tier_name(observation_id),
        night_sky_id,
        tier_name(night_sky_id)
    );

    let codons = vec![
        Codon {
            concept_id: ufo_id,
            role: RoleId::Agent,
            qualifiers: vec![],
        },
        Codon {
            concept_id: light_id,
            role: RoleId::Quality,
            qualifiers: vec![],
        },
        Codon {
            concept_id: observation_id,
            role: RoleId::Object,
            qualifiers: vec![],
        },
        Codon {
            concept_id: night_sky_id,
            role: RoleId::Location,
            qualifiers: vec![],
        },
    ];

    // TestimonyGene — witness report
    let gene = Gene::Testimony {
        triples: vec![Triple {
            subject: ufo_id,
            predicate: observation_id,
            object: night_sky_id,
        }],
        claim_type: 0,    // SIGHTING
        extraordinary: 2, // HIGH
        witness_count: 1,
        proximity: 0,           // FIRSTHAND
        verification_status: 0, // UNVERIFIED
    };

    // Low-trust section for unverified testimony
    let trust = TrustSection {
        epistemic_status: EpistemicStatus::Hearsay,
        evidence_type: EvidenceType::Anecdotal,
        verification_level: 0, // none
        corroboration_count: 0,
        challenge_count: 0,
        error_susceptibility: 0b0000_0000_0001_1001, // EYEWITNESS_MEMORY | EMOTIONAL_STATE | SINGLE_SOURCE
        trust_score: 2000,                           // 0.20
        confidence: 3000,                            // 0.30
        domain_codes: vec![],
        verifications: vec![],
        challenges: vec![],
        ..Default::default()
    };

    let ku = build_ku(codons, vec![], gene, Some(trust), None);
    let (wire, ku_back) = roundtrip(&ku);

    let trust_back = ku_back.trust.as_ref().unwrap();
    println!("\n┌─── Testimony KU ─────────────────────────────┐");
    println!("│ Gene Type:      Testimony");
    println!("│ Claim:          SIGHTING (extraordinary=HIGH)");
    println!("│ Trust Status:   {:?}", trust_back.epistemic_status);
    println!(
        "│ Trust Score:    {}/10000 ({:.2}%)",
        trust_back.trust_score,
        trust_back.trust_score as f64 / 100.0
    );
    println!(
        "│ Error Flags:    0b{:016b}",
        trust_back.error_susceptibility
    );
    println!("│ Wire Size:      {} bytes", wire.len());
    println!("│ CRC Valid:      ✓");
    println!("└──────────────────────────────────────────────┘");

    // Verify low trust differentiates from high-trust fact
    assert_eq!(trust_back.trust_score, 2000);
    assert_eq!(trust_back.epistemic_status, EpistemicStatus::Hearsay);
    println!("✓ Low-trust testimony correctly differentiated from verified facts");
}

#[test]
fn demo_e2e_experience() {
    println!("\n============================================================");
    println!("  SCENARIO: Person shares sunset experience");
    println!("============================================================\n");

    let registry = ConceptRegistry::new_with_bootstrap();

    let sunset_id = registry.resolve("sunset").unwrap(); // 151
    let ocean_id = registry.resolve("ocean").unwrap(); // 152
    let wonder_id = registry.resolve("wonder").unwrap(); // 163
    let color_id = registry.resolve("color").unwrap(); // 207
    let horizon_id = registry.resolve("horizon").unwrap(); // 208
    let warmth_id = registry.resolve("warmth").unwrap(); // 209

    println!(
        "Concepts: sunset={}, ocean={}, wonder={}, color={}, horizon={}, warmth={}",
        sunset_id, ocean_id, wonder_id, color_id, horizon_id, warmth_id
    );

    // Scene codons — sensory experience
    let scene = vec![
        Codon {
            concept_id: sunset_id,
            role: RoleId::Agent,
            qualifiers: vec![],
        },
        Codon {
            concept_id: ocean_id,
            role: RoleId::Location,
            qualifiers: vec![],
        },
        Codon {
            concept_id: color_id,
            role: RoleId::Quality,
            qualifiers: vec![Qualifier {
                key: "hue".into(),
                value: QualifierValue::Text("orange-pink".into()),
            }],
        },
        Codon {
            concept_id: horizon_id,
            role: RoleId::Location,
            qualifiers: vec![],
        },
        Codon {
            concept_id: warmth_id,
            role: RoleId::Quality,
            qualifiers: vec![],
        },
    ];

    // ExperienceGene — subjective experience with VAD affect
    let gene = Gene::Experience {
        scene,
        affect: Affect {
            v: 8500, // very positive valence
            a: 3000, // low arousal (calm)
            d: 7000, // moderate-high dominance
        },
        canonical: Some(CanonicalText {
            lang: 1, // English
            text: b"Watching the sun melt into the ocean, painting the sky in orange and pink"
                .to_vec(),
        }),
        perspective: Some(Perspective {
            expertise: 0,        // novice (personal experience, not expert)
            perspective_type: 1, // SUBJECTIVE
        }),
    };

    // Codons for the KU envelope (the main semantic codons)
    let codons = vec![
        Codon {
            concept_id: sunset_id,
            role: RoleId::Agent,
            qualifiers: vec![],
        },
        Codon {
            concept_id: ocean_id,
            role: RoleId::Location,
            qualifiers: vec![],
        },
        Codon {
            concept_id: wonder_id,
            role: RoleId::Quality,
            qualifiers: vec![],
        },
    ];

    // Trust for subjective experience
    let trust = TrustSection {
        epistemic_status: EpistemicStatus::Testimony,
        evidence_type: EvidenceType::Anecdotal,
        verification_level: 1, // self-reported
        corroboration_count: 0,
        challenge_count: 0,
        error_susceptibility: 0b0000_0000_0001_0000, // SELF_REPORTED
        trust_score: 5000,                           // 0.50 — subjective experience
        confidence: 7000,                            // 0.70
        domain_codes: vec![],
        verifications: vec![],
        challenges: vec![],
        ..Default::default()
    };

    let ku = build_ku(codons, vec![], gene, Some(trust), None);
    let (wire, ku_back) = roundtrip(&ku);

    // Verify affect data
    if let Gene::Experience {
        ref affect,
        ref canonical,
        ref perspective,
        ..
    } = ku_back.gene
    {
        println!("\n┌─── Experience KU ────────────────────────────┐");
        println!("│ Gene Type:      Experience");
        println!(
            "│ VAD Affect:     V={} A={} D={}",
            affect.v, affect.a, affect.d
        );
        println!(
            "│   Valence:      {:.2} (positive)",
            affect.v as f64 / 10000.0
        );
        println!("│   Arousal:      {:.2} (calm)", affect.a as f64 / 10000.0);
        println!(
            "│   Dominance:    {:.2} (in control)",
            affect.d as f64 / 10000.0
        );
        if let Some(ref c) = canonical {
            println!("│ Canonical:      \"{}\"", String::from_utf8_lossy(&c.text));
        }
        if let Some(ref p) = perspective {
            println!(
                "│ Perspective:    type={} expertise={}",
                match p.perspective_type {
                    0 => "OBJECTIVE",
                    1 => "SUBJECTIVE",
                    2 => "INTERSUBJECTIVE",
                    _ => "CONTESTED",
                },
                p.expertise
            );
        }
        println!("│ Wire Size:      {} bytes", wire.len());
        println!("└──────────────────────────────────────────────┘");

        assert_eq!(affect.v, 8500);
        assert_eq!(affect.a, 3000);
    } else {
        panic!("Expected Experience gene");
    }

    println!("✓ Subjective experience encoded differently from facts (VAD + perspective)");
}

#[test]
fn demo_e2e_formal() {
    println!("\n============================================================");
    println!("  SCENARIO: Encoding E=mc²");
    println!("============================================================\n");

    let registry = ConceptRegistry::new_with_bootstrap();

    let energy_id = registry.resolve("energy").unwrap(); // 190
    let mass_id = registry.resolve("mass").unwrap(); // 191
    let speed_id = registry.resolve("light_speed").unwrap(); // 192
    let equation_id = registry.resolve("equation").unwrap(); // 193

    println!(
        "Concepts: energy={}, mass={}, light_speed={}, equation={}",
        energy_id, mass_id, speed_id, equation_id
    );

    let codons = vec![
        Codon {
            concept_id: energy_id,
            role: RoleId::Agent,
            qualifiers: vec![],
        },
        Codon {
            concept_id: mass_id,
            role: RoleId::Object,
            qualifiers: vec![],
        },
        Codon {
            concept_id: speed_id,
            role: RoleId::Quality,
            qualifiers: vec![Qualifier {
                key: "power".into(),
                value: QualifierValue::Integer(2),
            }],
        },
        Codon {
            concept_id: equation_id,
            role: RoleId::CompoundHead,
            qualifiers: vec![],
        },
    ];

    // FormalGene — mathematical/physical equation
    let gene = Gene::Formal {
        domain: 1,          // PHYSICS
        notation_format: 0, // LATEX
        notation_source: b"E = mc^2".to_vec(),
        statement_type: 2,      // THEOREM
        verification_status: 3, // FORMALLY_PROVED
    };

    let trust = TrustSection {
        epistemic_status: EpistemicStatus::FormallyProven,
        evidence_type: EvidenceType::FormalProof,
        verification_level: 4,      // formal verification
        corroboration_count: 10000, // universally confirmed
        challenge_count: 0,
        error_susceptibility: 0,
        trust_score: 10000,      // 1.0 — maximum trust
        confidence: 10000,       // 1.0
        domain_codes: vec![140], // physics
        verifications: vec![],
        challenges: vec![],
        ..Default::default()
    };

    let ku = build_ku(codons, vec![], gene, Some(trust), None);
    let (wire, ku_back) = roundtrip(&ku);

    // Verify formal gene data
    if let Gene::Formal {
        ref notation_source,
        domain,
        verification_status,
        ..
    } = ku_back.gene
    {
        println!("\n┌─── Formal KU ────────────────────────────────┐");
        println!("│ Gene Type:      Formal (PHYSICS)");
        println!(
            "│ Notation:       \"{}\"",
            String::from_utf8_lossy(notation_source)
        );
        println!("│ Domain:         {} (PHYSICS)", domain);
        println!(
            "│ Verification:   {} (FORMALLY_PROVED)",
            verification_status
        );
        let trust_back = ku_back.trust.as_ref().unwrap();
        println!(
            "│ Trust Score:    {}/10000 (PERFECT)",
            trust_back.trust_score
        );
        println!("│ Epistemic:      {:?}", trust_back.epistemic_status);
        println!("│ Corroborations: {}", trust_back.corroboration_count);
        println!("│ Wire Size:      {} bytes", wire.len());
        println!("└──────────────────────────────────────────────┘");
    } else {
        panic!("Expected Formal gene");
    }

    assert_eq!(ku_back.trust.as_ref().unwrap().trust_score, 10000);
    println!("✓ Formally-proven knowledge encoded with maximum trust");
}

#[test]
fn demo_e2e_hypothesis() {
    println!("\n============================================================");
    println!("  SCENARIO: \"Dark matter explains galaxy rotation\"");
    println!("  (EXTENDED gene type — uses ext byte in wire format)");
    println!("============================================================\n");

    let registry = ConceptRegistry::new_with_bootstrap();

    let dark_matter_id = registry.resolve("dark_matter").unwrap(); // 202
    let galaxy_id = registry.resolve("galaxy").unwrap(); // 200
    let rotation_id = registry.resolve("rotation").unwrap(); // 201

    println!(
        "Concepts: dark_matter={}, galaxy={}, rotation={}",
        dark_matter_id, galaxy_id, rotation_id
    );

    let codons = vec![
        Codon {
            concept_id: dark_matter_id,
            role: RoleId::Agent,
            qualifiers: vec![],
        },
        Codon {
            concept_id: galaxy_id,
            role: RoleId::Object,
            qualifiers: vec![],
        },
        Codon {
            concept_id: rotation_id,
            role: RoleId::Result,
            qualifiers: vec![],
        },
    ];

    let body_codons = vec![
        Codon {
            concept_id: dark_matter_id,
            role: RoleId::Cause,
            qualifiers: vec![],
        },
        Codon {
            concept_id: rotation_id,
            role: RoleId::Result,
            qualifiers: vec![],
        },
    ];

    // HypothesisGene — EXTENDED type (base=7, ext=0x00)
    let gene = Gene::Hypothesis {
        base_type: 0, // will mature into Fact
        body_codons,
        maturity_level: 3,  // WELL_SUPPORTED
        confidence: 7000,   // 0.70
        completeness: 5000, // 0.50
        falsifiable: true,
    };

    // Verify EXTENDED wire encoding
    let (base, ext) = GeneType::Hypothesis.wire_encoding();
    println!(
        "Wire encoding: base={} (EXTENDED), ext=0x{:02X}",
        base,
        ext.unwrap()
    );
    assert_eq!(base, 7, "Hypothesis must use EXTENDED base type");
    assert_eq!(ext, Some(0x00), "Hypothesis ext byte must be 0x00");

    let trust = TrustSection {
        epistemic_status: EpistemicStatus::Hypothesis,
        evidence_type: EvidenceType::Observational,
        verification_level: 2, // peer review
        corroboration_count: 15,
        challenge_count: 3,
        error_susceptibility: 0,
        trust_score: 6000,       // 0.60
        confidence: 5000,        // 0.50
        domain_codes: vec![140], // physics
        verifications: vec![],
        challenges: vec![],
        ..Default::default()
    };

    let ku = build_ku(codons, vec![], gene, Some(trust), None);
    let wire = encode_knowledge_unit(&ku).unwrap();

    // Inspect the wire format — EXTENDED type means FLAGS bits 5-7 = 111 = 7
    let flags_byte = wire[3];
    let gene_base_from_wire = (flags_byte >> 5) & 0x07;
    assert_eq!(
        gene_base_from_wire, 7,
        "FLAGS gene type must be 7 (EXTENDED)"
    );

    // The first byte of payload should be the ext byte (0x00 for Hypothesis)
    let ext_byte = wire[8]; // first byte after v5 header (8 bytes)
    println!("Payload ext byte: 0x{:02X} (Hypothesis)", ext_byte);
    assert_eq!(ext_byte, 0x00, "Ext byte must be 0x00 for Hypothesis");

    // Decode and verify
    let (decoded, ku_back) = decode_full_knowledge_unit(&wire).unwrap();
    assert_eq!(decoded.gene_type, GeneType::Hypothesis);

    if let Gene::Hypothesis {
        maturity_level,
        confidence,
        falsifiable,
        ..
    } = &ku_back.gene
    {
        println!("\n┌─── Hypothesis KU ────────────────────────────┐");
        println!("│ Gene Type:      Hypothesis (EXTENDED 0x00)");
        println!("│ Maturity:       {} (WELL_SUPPORTED)", maturity_level);
        println!(
            "│ Confidence:     {}/10000 ({:.2}%)",
            confidence,
            *confidence as f64 / 100.0
        );
        println!("│ Falsifiable:    {}", falsifiable);
        let tb = ku_back.trust.as_ref().unwrap();
        println!("│ Trust Score:    {}/10000", tb.trust_score);
        println!("│ Challenges:     {}", tb.challenge_count);
        println!("│ Wire Size:      {} bytes", wire.len());
        println!(
            "│ FLAGS byte:     0b{:08b} (gene bits=111=EXTENDED)",
            flags_byte
        );
        println!("└──────────────────────────────────────────────┘");
    } else {
        panic!("Expected Hypothesis gene");
    }

    println!("✓ EXTENDED gene type wire mechanism verified");
}

#[test]
fn demo_e2e_multi_ku_bonds() {
    println!("\n============================================================");
    println!("  SCENARIO: KU2 corroborates KU1");
    println!("============================================================\n");

    let registry = ConceptRegistry::new_with_bootstrap();

    let water_id = registry.resolve("water").unwrap();
    let boiling_id = registry.resolve("boiling_point").unwrap();
    let temp_id = registry.resolve("temperature").unwrap();
    let experiment_id = registry.resolve("experiment").unwrap();
    let confirmation_id = registry.resolve("confirmation").unwrap();

    // ── KU1: Original fact ──
    let ku1_codons = vec![
        Codon {
            concept_id: water_id,
            role: RoleId::Agent,
            qualifiers: vec![],
        },
        Codon {
            concept_id: boiling_id,
            role: RoleId::Quality,
            qualifiers: vec![],
        },
        Codon {
            concept_id: temp_id,
            role: RoleId::Quantity,
            qualifiers: vec![Qualifier {
                key: "val".into(),
                value: QualifierValue::Integer(100),
            }],
        },
    ];
    let ku1_gene = Gene::Fact {
        triples: vec![Triple {
            subject: water_id,
            predicate: boiling_id,
            object: temp_id,
        }],
        certainty: 9000,
        evidence: vec![],
    };
    let ku1_trust = TrustSection {
        epistemic_status: EpistemicStatus::Evidence,
        evidence_type: EvidenceType::Experimental,
        verification_level: 2,
        corroboration_count: 10,
        challenge_count: 0,
        error_susceptibility: 0,
        trust_score: 8500,
        confidence: 8000,
        domain_codes: vec![141],
        verifications: vec![],
        challenges: vec![],
        ..Default::default()
    };
    let ku1 = build_ku(ku1_codons, vec![], ku1_gene, Some(ku1_trust), None);
    let (ku1_wire, _) = roundtrip(&ku1);
    println!("KU1 (original fact): {} bytes", ku1_wire.len());

    // Simulate a CID for KU1 (in production: SHA-256 hash)
    let ku1_fake_cid: Vec<u8> = vec![
        0xDE, 0xAD, 0xBE, 0xEF, 0x01, 0x02, 0x03, 0x04, 0x05, 0x06, 0x07, 0x08, 0x09, 0x0A, 0x0B,
        0x0C, 0x0D, 0x0E, 0x0F, 0x10, 0x11, 0x12, 0x13, 0x14, 0x15, 0x16, 0x17, 0x18, 0x19, 0x1A,
        0x1B, 0x1C, 0x1D, 0x1E, 0x1F, 0x20,
    ];

    // ── KU2: Corroborating experiment with bond to KU1 ──
    let ku2_codons = vec![
        Codon {
            concept_id: experiment_id,
            role: RoleId::Agent,
            qualifiers: vec![],
        },
        Codon {
            concept_id: confirmation_id,
            role: RoleId::Result,
            qualifiers: vec![],
        },
        Codon {
            concept_id: water_id,
            role: RoleId::Object,
            qualifiers: vec![],
        },
    ];

    // Bond: KU2 CORROBORATES KU1
    let bond = Bond {
        target_cid: ku1_fake_cid.clone(),
        relation: RelationType::Corroborates,
        weight: 9000, // strong corroboration
        creator: Creator::Human,
        created_at: 1719100000, // example unix timestamp
        evidence: vec![],
        state: EdgeState::Active,
        initial_weight: Some(9000),
        decay: Some(DecayRate::None), // corroboration never decays
        last_reinforced: None,
        reinforce_count: None,
        bidirectional: None,
        context: vec![141], // chemistry context
        order: None,
        required: None,
    };

    let ku2_gene = Gene::Fact {
        triples: vec![Triple {
            subject: experiment_id,
            predicate: confirmation_id,
            object: water_id,
        }],
        certainty: 9500,
        evidence: vec![],
    };

    let ku2_trust = TrustSection {
        epistemic_status: EpistemicStatus::Corroborated,
        evidence_type: EvidenceType::Experimental,
        verification_level: 3,
        corroboration_count: 1,
        challenge_count: 0,
        error_susceptibility: 0,
        trust_score: 9200,
        confidence: 9000,
        domain_codes: vec![141],
        verifications: vec![],
        challenges: vec![],
        ..Default::default()
    };

    let ku2 = build_ku(ku2_codons, vec![bond], ku2_gene, Some(ku2_trust), None);
    let (ku2_wire, ku2_back) = roundtrip(&ku2);
    println!("KU2 (corroboration): {} bytes", ku2_wire.len());

    // Verify bond survived roundtrip
    assert_eq!(ku2_back.bonds.len(), 1);
    let bond_back = &ku2_back.bonds[0];
    assert_eq!(bond_back.relation, RelationType::Corroborates);
    assert_eq!(bond_back.target_cid, ku1_fake_cid);
    assert_eq!(bond_back.weight, 9000);

    println!("\n┌─── Bond Verification ────────────────────────┐");
    println!("│ Bond:           KU2 → KU1");
    println!(
        "│ Relation:       {:?} (category {})",
        bond_back.relation,
        bond_back.relation.category()
    );
    println!(
        "│ Weight:         {}/10000 ({:.2})",
        bond_back.weight,
        bond_back.weight as f64 / 10000.0
    );
    println!("│ Creator:        {:?}", bond_back.creator);
    println!("│ State:          {:?}", bond_back.state);
    println!("│ Target CID:     {:02X?}...", &bond_back.target_cid[..4]);
    println!("│ KU1 Size:       {} bytes", ku1_wire.len());
    println!("│ KU2 Size:       {} bytes (with bond)", ku2_wire.len());
    println!(
        "│ Bond overhead:  ~{} bytes",
        ku2_wire.len() as i64 - ku1_wire.len() as i64
    );
    println!("└──────────────────────────────────────────────┘");

    println!("✓ Inter-KU bonds with CID references verified");
}

#[test]
fn demo_e2e_size_scaling() {
    println!("\n============================================================");
    println!("  SCENARIO: Compare KU sizes across trust levels");
    println!("============================================================\n");

    let registry = ConceptRegistry::new_with_bootstrap();
    let water_id = registry.resolve("water").unwrap();
    let boiling_id = registry.resolve("boiling_point").unwrap();
    let temp_id = registry.resolve("temperature").unwrap();

    // Same content, different trust levels
    let base_codons = || {
        vec![
            Codon {
                concept_id: water_id,
                role: RoleId::Agent,
                qualifiers: vec![],
            },
            Codon {
                concept_id: boiling_id,
                role: RoleId::Quality,
                qualifiers: vec![],
            },
            Codon {
                concept_id: temp_id,
                role: RoleId::Quantity,
                qualifiers: vec![],
            },
        ]
    };
    let base_gene = || Gene::Fact {
        triples: vec![Triple {
            subject: water_id,
            predicate: boiling_id,
            object: temp_id,
        }],
        certainty: 9000,
        evidence: vec![],
    };

    // ── Level 1: Minimal (no trust section) ──
    let ku1 = build_ku(base_codons(), vec![], base_gene(), None, None);
    let wire1 = encode_knowledge_unit(&ku1).unwrap();

    // ── Level 2: With trust section ──
    let trust = TrustSection {
        epistemic_status: EpistemicStatus::Evidence,
        evidence_type: EvidenceType::Experimental,
        verification_level: 3,
        corroboration_count: 42,
        challenge_count: 0,
        error_susceptibility: 0,
        trust_score: 9500,
        confidence: 8500,
        domain_codes: vec![140, 141],
        verifications: vec![],
        challenges: vec![],
        ..Default::default()
    };
    let ku2 = build_ku(base_codons(), vec![], base_gene(), Some(trust), None);
    let wire2 = encode_knowledge_unit(&ku2).unwrap();

    // ── Level 3: With trust + epigenetic (incl. embedding) ──
    let trust3 = TrustSection {
        epistemic_status: EpistemicStatus::Evidence,
        evidence_type: EvidenceType::Experimental,
        verification_level: 3,
        corroboration_count: 42,
        challenge_count: 0,
        error_susceptibility: 0,
        trust_score: 9500,
        confidence: 8500,
        domain_codes: vec![140, 141],
        verifications: vec![],
        challenges: vec![],
        ..Default::default()
    };
    let epi = EpigeneticSection {
        embedding: vec![0x42; 512],        // simulated int8[512]
        embedding_binary: vec![0xFF; 128], // simulated binary[1024/8]
        embed_version: Some(1),
        valid_from: Some(1700000000),
        valid_until: None,
        recorded_at: Some(1719100000),
        temporal_precision: Some(4), // DAY
        temporal_uncertainty: None,
        half_life: Some(31_536_000), // 1 year
        krl: Some(6),                // KRL 6
        language: Some(1),           // English
        template: Some(0),           // NARRATIVE
        difficulty: Some(2),         // INTERMEDIATE
        categories: vec![140, 141],
        tags: vec![128, 132],
        simhash: vec![0xAB; 16],
        lsh_buckets: vec![0xCD; 16],
        schema_ver: Some(4),
        version: Some(1),
        prev_cid: None,
        superseded_by: None,
    };
    let ku3 = build_ku(base_codons(), vec![], base_gene(), Some(trust3), Some(epi));
    let wire3 = encode_knowledge_unit(&ku3).unwrap();

    println!("┌─── Size Comparison ──────────────────────────┐");
    println!("│                                              │");
    println!(
        "│  Level 1 (Minimal, no trust):    {:>4} bytes │",
        wire1.len()
    );
    println!(
        "│  Level 2 (With trust section):   {:>4} bytes │",
        wire2.len()
    );
    println!(
        "│  Level 3 (Trust + epigenetic):   {:>4} bytes │",
        wire3.len()
    );
    println!("│                                              │");
    println!(
        "│  Trust overhead:       +{:>4} bytes ({:.1}x)   │",
        wire2.len() - wire1.len(),
        wire2.len() as f64 / wire1.len() as f64
    );
    println!(
        "│  Epigenetic overhead:  +{:>4} bytes ({:.1}x)   │",
        wire3.len() - wire2.len(),
        wire3.len() as f64 / wire2.len() as f64
    );
    println!(
        "│  Total overhead:       +{:>4} bytes ({:.1}x)   │",
        wire3.len() - wire1.len(),
        wire3.len() as f64 / wire1.len() as f64
    );
    println!("│                                              │");
    println!("└──────────────────────────────────────────────┘");

    // Sanity checks
    assert!(wire1.len() < wire2.len(), "Trust should add size");
    assert!(wire2.len() < wire3.len(), "Epigenetic should add size");
    assert!(
        wire3.len() > wire1.len() + 600,
        "Embedding should add ~640+ bytes"
    );

    // Print full breakdown for the most complete KU
    println!("\nFull breakdown (Level 3):");
    println!("{}", size_breakdown_full(&ku3).unwrap());

    println!("✓ Progressive size growth verified across trust levels");
}

#[test]
fn demo_print_summary() {
    println!("\n╔══════════════════════════════════════════════════════════╗");
    println!("║            UKRL v4 E2E DEMO — FINAL SUMMARY            ║");
    println!("╠══════════════════════════════════════════════════════════╣");

    // Registry stats
    let registry = ConceptRegistry::new_with_bootstrap();
    println!("║                                                        ║");
    println!(
        "║  📚 Registry: {} concepts bootstrapped                 ║",
        registry.len()
    );

    // Varint tier distribution
    let mut tier_counts = [0u32; 4];
    for id in [
        1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14, 15, 16, 128, 129, 130, 131, 132, 133, 134,
        135, 136, 137, 140, 141, 142, 143, 150, 151, 152, 153, 160, 161, 162, 163, 164, 170, 171,
        172, 173, 180, 181, 182, 190, 191, 192, 193, 200, 201, 202, 203, 204, 205, 206, 207, 208,
        209,
    ] {
        match varint_size(id) {
            1 => tier_counts[0] += 1,
            2 => tier_counts[1] += 1,
            3 => tier_counts[2] += 1,
            5 => tier_counts[3] += 1,
            _ => {}
        }
    }
    println!("║                                                        ║");
    println!("║  🧬 Varint Tier Distribution:                          ║");
    println!(
        "║     Tier 0 (1B, 0-127):       {:>2} concepts             ║",
        tier_counts[0]
    );
    println!(
        "║     Tier 1 (2B, 128-16K):     {:>2} concepts             ║",
        tier_counts[1]
    );
    println!(
        "║     Tier 2 (3B, 16K-2M):      {:>2} concepts             ║",
        tier_counts[2]
    );
    println!(
        "║     Tier 3 (5B, 2M+):         {:>2} concepts             ║",
        tier_counts[3]
    );

    // Build all demo KUs for size comparison
    let water_id = registry.resolve("water").unwrap();
    let boiling_id = registry.resolve("boiling_point").unwrap();
    let temp_id = registry.resolve("temperature").unwrap();

    let fact_codons = vec![
        Codon {
            concept_id: water_id,
            role: RoleId::Agent,
            qualifiers: vec![],
        },
        Codon {
            concept_id: boiling_id,
            role: RoleId::Quality,
            qualifiers: vec![],
        },
        Codon {
            concept_id: temp_id,
            role: RoleId::Quantity,
            qualifiers: vec![],
        },
    ];
    let fact_gene = Gene::Fact {
        triples: vec![Triple {
            subject: water_id,
            predicate: boiling_id,
            object: temp_id,
        }],
        certainty: 9800,
        evidence: vec![],
    };
    let fact_trust = TrustSection {
        epistemic_status: EpistemicStatus::Evidence,
        evidence_type: EvidenceType::Experimental,
        verification_level: 3,
        corroboration_count: 42,
        challenge_count: 0,
        error_susceptibility: 0,
        trust_score: 9500,
        confidence: 8500,
        domain_codes: vec![140, 141],
        verifications: vec![],
        challenges: vec![],
        ..Default::default()
    };
    let fact_ku = build_ku(fact_codons, vec![], fact_gene, Some(fact_trust), None);
    let fact_wire = encode_knowledge_unit(&fact_ku).unwrap();

    // Testimony
    let ufo_id = registry.resolve("ufo").unwrap();
    let tst_ku = build_ku(
        vec![Codon {
            concept_id: ufo_id,
            role: RoleId::Agent,
            qualifiers: vec![],
        }],
        vec![],
        Gene::Testimony {
            triples: vec![Triple {
                subject: ufo_id,
                predicate: 11,
                object: 203,
            }],
            claim_type: 0,
            extraordinary: 2,
            witness_count: 1,
            proximity: 0,
            verification_status: 0,
        },
        Some(TrustSection {
            epistemic_status: EpistemicStatus::Hearsay,
            evidence_type: EvidenceType::Anecdotal,
            verification_level: 0,
            corroboration_count: 0,
            challenge_count: 0,
            error_susceptibility: 0,
            trust_score: 2000,
            confidence: 3000,
            domain_codes: vec![],
            verifications: vec![],
            challenges: vec![],
            ..Default::default()
        }),
        None,
    );
    let tst_wire = encode_knowledge_unit(&tst_ku).unwrap();

    // Formal
    let formal_ku = build_ku(
        vec![Codon {
            concept_id: 190,
            role: RoleId::Agent,
            qualifiers: vec![],
        }],
        vec![],
        Gene::Formal {
            domain: 1,
            notation_format: 0,
            notation_source: b"E=mc^2".to_vec(),
            statement_type: 2,
            verification_status: 3,
        },
        Some(TrustSection {
            epistemic_status: EpistemicStatus::FormallyProven,
            evidence_type: EvidenceType::FormalProof,
            verification_level: 4,
            corroboration_count: 10000,
            challenge_count: 0,
            error_susceptibility: 0,
            trust_score: 10000,
            confidence: 10000,
            domain_codes: vec![140],
            verifications: vec![],
            challenges: vec![],
            ..Default::default()
        }),
        None,
    );
    let formal_wire = encode_knowledge_unit(&formal_ku).unwrap();

    // Hypothesis (EXTENDED)
    let hyp_ku = build_ku(
        vec![Codon {
            concept_id: 202,
            role: RoleId::Agent,
            qualifiers: vec![],
        }],
        vec![],
        Gene::Hypothesis {
            base_type: 0,
            body_codons: vec![Codon {
                concept_id: 202,
                role: RoleId::Cause,
                qualifiers: vec![],
            }],
            maturity_level: 3,
            confidence: 7000,
            completeness: 5000,
            falsifiable: true,
        },
        Some(TrustSection {
            epistemic_status: EpistemicStatus::Hypothesis,
            evidence_type: EvidenceType::Observational,
            verification_level: 2,
            corroboration_count: 15,
            challenge_count: 3,
            error_susceptibility: 0,
            trust_score: 6000,
            confidence: 5000,
            domain_codes: vec![140],
            verifications: vec![],
            challenges: vec![],
            ..Default::default()
        }),
        None,
    );
    let hyp_wire = encode_knowledge_unit(&hyp_ku).unwrap();

    println!("║                                                        ║");
    println!("║  📦 Wire Format Sizes:                                 ║");
    println!(
        "║     Fact (water boils):        {:>4} bytes              ║",
        fact_wire.len()
    );
    println!(
        "║     Testimony (UFO):           {:>4} bytes              ║",
        tst_wire.len()
    );
    println!(
        "║     Formal (E=mc²):            {:>4} bytes              ║",
        formal_wire.len()
    );
    println!(
        "║     Hypothesis (dark matter):  {:>4} bytes              ║",
        hyp_wire.len()
    );

    // Roundtrip verification for all
    let scenarios = vec![
        ("Fact", &fact_wire),
        ("Testimony", &tst_wire),
        ("Formal", &formal_wire),
        ("Hypothesis", &hyp_wire),
    ];
    let mut all_ok = true;
    for (name, wire) in &scenarios {
        match decode_full_knowledge_unit(wire) {
            Ok((decoded, _)) => {
                if !decoded.crc32_valid {
                    all_ok = false;
                }
            }
            Err(e) => {
                println!("║     ✗ {} roundtrip FAILED: {}             ║", name, e);
                all_ok = false;
            }
        }
    }

    println!("║                                                        ║");
    println!(
        "║  🔄 Codec Roundtrip: {} success rate           ║",
        if all_ok { "100%" } else { "FAIL" }
    );
    println!("║                                                        ║");
    println!("╚══════════════════════════════════════════════════════════╝");

    assert!(all_ok, "All roundtrips must succeed");
}

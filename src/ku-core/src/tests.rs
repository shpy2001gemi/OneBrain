//! Integration tests for ku-core — UKRL v4 Layers 1-3.

#[cfg(test)]
#[allow(unused)]
mod integration {
    use crate::decoder::*;
    use crate::encoder::*;
    use crate::error::KuError;
    use crate::types::*;
    use crate::varint::{decode_varint, encode_varint};

    // ========================================================================
    // Helper: Create example concept IDs
    // ========================================================================

    // Tier 0 primitives (0-127)
    const C_DO: ConceptId = 1;
    const C_HEAT: ConceptId = 10;
    const C_HOT: ConceptId = 11;

    // Tier 1 common concepts (128-16,511)
    const C_WATER: ConceptId = 312;
    const C_BOIL: ConceptId = 847;
    const C_TEMPERATURE: ConceptId = 500;
    const C_PRESSURE: ConceptId = 501;
    const C_SUNSET: ConceptId = 1200;
    const C_BEAUTY: ConceptId = 1201;
    const C_SKY: ConceptId = 1202;
    const C_ORANGE: ConceptId = 1203;
    const C_RED: ConceptId = 1204;
    const C_WARM: ConceptId = 1205;
    const C_CALM: ConceptId = 1206;

    // Tier 2 standard concepts (16,512-2,113,663)
    const C_CELSIUS: ConceptId = 20_000;
    const C_ATM: ConceptId = 20_001;
    const C_DEGREE: ConceptId = 20_002;
    const C_BOILING_POINT: ConceptId = 20_003;

    // ========================================================================
    // Test 1: FactGene — "Water boils at 100°C at 1 atm"
    // ========================================================================

    fn make_fact_ku() -> KnowledgeUnit {
        KnowledgeUnit {
            codons: vec![
                Codon {
                    concept_id: C_WATER,
                    role: RoleId::Agent,
                    qualifiers: vec![],
                },
                Codon {
                    concept_id: C_BOIL,
                    role: RoleId::Result,
                    qualifiers: vec![],
                },
                Codon {
                    concept_id: 100, // literal value "100"
                    role: RoleId::Quantity,
                    qualifiers: vec![Qualifier {
                        key: "unit".into(),
                        value: QualifierValue::Concept(C_CELSIUS),
                    }],
                },
                Codon {
                    concept_id: 1, // literal "1"
                    role: RoleId::Condition,
                    qualifiers: vec![Qualifier {
                        key: "unit".into(),
                        value: QualifierValue::Concept(C_ATM),
                    }],
                },
            ],
            bonds: vec![Bond {
                target_cid: vec![0u8; 36], // placeholder CID
                relation: RelationType::Qualifies,
                weight: 9500, // 0.95
                creator: Creator::Human,
                created_at: 1719072000, // 2024-06-22
                evidence: vec![],
                state: EdgeState::Active,
                initial_weight: Some(9500),
                decay: Some(DecayRate::None),
                last_reinforced: None,
                reinforce_count: None,
                bidirectional: None,
                context: vec![],
                order: None,
                required: None,
            }],
            gene: Gene::Fact {
                triples: vec![Triple {
                    subject: C_WATER,
                    predicate: C_BOILING_POINT,
                    object: C_CELSIUS,
                }],
                certainty: 9900, // 0.99 — established scientific fact
                evidence: vec![],
            },
            flags: HeaderFlags::default(),
            epistemic_status: Some(EpistemicStatus::Consensus),
            evidence_type: Some(EvidenceType::Experimental),
            trust: None,
            epigenetic: None,
        }
    }

    #[test]
    fn test_encode_fact_gene_water_boils() {
        let ku = make_fact_ku();

        // Encode individual layers
        let codons_bytes = encode_codons(&ku.codons).unwrap();
        let bond_bytes = encode_bond(&ku.bonds[0]).unwrap();
        let gene_bytes = encode_gene(&ku.gene).unwrap();

        println!("\n══════════════════════════════════════════════════");
        println!("  TEST: FactGene — \"Water boils at 100°C at 1 atm\"");
        println!("══════════════════════════════════════════════════");
        println!("  Layer 1 (4 codons):  {} bytes", codons_bytes.len());
        println!("  Layer 2 (1 bond):    {} bytes", bond_bytes.len());
        println!("  Layer 3 (FactGene):  {} bytes", gene_bytes.len());

        // Full wire encoding
        let wire = encode_knowledge_unit(&ku).unwrap();
        println!("  Wire total:          {} bytes", wire.len());
        println!("  v4 spec estimate:    200-400 bytes (gene only)");

        // Verify header
        assert_eq!(wire[0], 0x4B, "Magic byte 0");
        assert_eq!(wire[1], 0x44, "Magic byte 1 ('D')");
        assert_eq!(wire[2], 0x05, "Version v5");

        // Verify flags — gene type 0 (Fact) in bits 5-7
        let (flags, gene_base) = HeaderFlags::from_byte(wire[3]);
        assert_eq!(gene_base, 0, "Gene type base should be 0 (Fact)");
        assert!(!flags.has_ecc);
        assert!(!flags.is_encrypted);

        // Verify CRC
        let result = decode_knowledge_unit(&wire);
        assert!(result.is_ok(), "Wire decode failed: {:?}", result.err());
        let decoded = result.unwrap();
        assert_eq!(decoded.gene_type, GeneType::Fact);

        // Size assertion: Layer 1-3 should be <500 bytes
        assert!(
            wire.len() < 500,
            "FactGene wire size {} exceeds 500 byte target",
            wire.len()
        );

        print_size_breakdown(&ku);
    }

    // ========================================================================
    // Test 2: ExperienceGene — "The sunset was beautiful"
    // ========================================================================

    fn make_experience_ku() -> KnowledgeUnit {
        KnowledgeUnit {
            codons: vec![
                Codon {
                    concept_id: C_SUNSET,
                    role: RoleId::Agent,
                    qualifiers: vec![],
                },
                Codon {
                    concept_id: C_BEAUTY,
                    role: RoleId::Quality,
                    qualifiers: vec![],
                },
                Codon {
                    concept_id: C_SKY,
                    role: RoleId::Location,
                    qualifiers: vec![],
                },
                Codon {
                    concept_id: C_ORANGE,
                    role: RoleId::Quality,
                    qualifiers: vec![],
                },
                Codon {
                    concept_id: C_RED,
                    role: RoleId::Quality,
                    qualifiers: vec![],
                },
            ],
            bonds: vec![],
            gene: Gene::Experience {
                scene: vec![
                    Codon {
                        concept_id: C_SKY,
                        role: RoleId::Location,
                        qualifiers: vec![],
                    },
                    Codon {
                        concept_id: C_WARM,
                        role: RoleId::Quality,
                        qualifiers: vec![],
                    },
                ],
                affect: Affect {
                    v: 8500, // Valence: +0.85 (very positive)
                    a: 3000, // Arousal: 0.30 (calm)
                    d: 6000, // Dominance: 0.60 (moderate)
                },
                canonical: Some(CanonicalText {
                    lang: 1, // English
                    text: b"The sunset was beautiful".to_vec(),
                }),
                perspective: Some(Perspective {
                    expertise: 0,        // novice
                    perspective_type: 1, // SUBJECTIVE
                }),
            },
            flags: HeaderFlags::default(),
            epistemic_status: Some(EpistemicStatus::Observation),
            evidence_type: None,
            trust: None,
            epigenetic: None,
        }
    }

    #[test]
    fn test_encode_experience_gene_sunset() {
        let ku = make_experience_ku();

        let codons_bytes = encode_codons(&ku.codons).unwrap();
        let gene_bytes = encode_gene(&ku.gene).unwrap();

        println!("\n══════════════════════════════════════════════════");
        println!("  TEST: ExperienceGene — \"The sunset was beautiful\"");
        println!("══════════════════════════════════════════════════");
        println!("  Layer 1 (5 codons):       {} bytes", codons_bytes.len());
        println!("  Layer 2 (0 bonds):        0 bytes");
        println!("  Layer 3 (ExperienceGene): {} bytes", gene_bytes.len());

        let wire = encode_knowledge_unit(&ku).unwrap();
        println!("  Wire total:               {} bytes", wire.len());
        println!("  v4 spec estimate:         300-600 bytes (gene only)");

        // Verify header
        let result = decode_knowledge_unit(&wire);
        assert!(result.is_ok(), "Wire decode failed: {:?}", result.err());
        let decoded = result.unwrap();
        assert_eq!(decoded.gene_type, GeneType::Experience);

        // Verify flags — gene type 2 (Experience) in bits 5-7
        let (_, gene_base) = HeaderFlags::from_byte(wire[3]);
        assert_eq!(gene_base, 2, "Gene type base should be 2 (Experience)");

        // Size assertion
        assert!(
            wire.len() < 500,
            "ExperienceGene wire size {} exceeds 500 byte target",
            wire.len()
        );

        print_size_breakdown(&ku);
    }

    // ========================================================================
    // Test 3: Varint round-trip for all tiers
    // ========================================================================

    #[test]
    fn test_varint_roundtrip_all_tiers() {
        let test_cases: Vec<(ConceptId, usize, &str)> = vec![
            (0, 1, "Tier 0 min"),
            (1, 1, "Tier 0 (C_DO)"),
            (63, 1, "Tier 0 mid"),
            (127, 1, "Tier 0 max"),
            (128, 2, "Tier 1 min"),
            (312, 2, "Tier 1 (C_WATER)"),
            (847, 2, "Tier 1 (C_BOIL)"),
            (16_511, 2, "Tier 1 max"),
            (16_512, 3, "Tier 2 min"),
            (20_000, 3, "Tier 2 (C_CELSIUS)"),
            (100_000, 3, "Tier 2 mid"),
            (2_113_663, 3, "Tier 2 max"),
            (2_113_664, 4, "Tier 3 min (4B)"),
            (5_000_000, 4, "Tier 3 (community concept)"),
            (270_549_119, 4, "Tier 3 max (4B)"),
            (270_549_120, 5, "Tier 3+ min (5B)"),
            (0xF000_0000, 5, "Tier 3+ (provisional ID range)"),
        ];

        println!("\n══════════════════════════════════════════════════");
        println!("  TEST: Varint Round-Trip — All Tiers");
        println!("══════════════════════════════════════════════════");
        println!(
            "  {:>15}  {:>5}  {:>10}  {}",
            "Value", "Bytes", "Hex", "Label"
        );
        println!("  {:─>15}  {:─>5}  {:─>10}  {:─>30}", "", "", "", "");

        for (value, expected_len, label) in &test_cases {
            let encoded = encode_varint(*value).unwrap();
            let hex: String = encoded
                .iter()
                .map(|b| format!("{:02X}", b))
                .collect::<Vec<_>>()
                .join(" ");
            let (decoded, consumed) = decode_varint(&encoded).unwrap();

            println!(
                "  {:>15}  {:>5}  {:>10}  {}",
                value,
                encoded.len(),
                hex,
                label
            );

            assert_eq!(
                encoded.len(),
                *expected_len,
                "Wrong size for {} ({})",
                value,
                label
            );
            assert_eq!(
                decoded, *value,
                "Roundtrip failed for {} ({})",
                value, label
            );
            assert_eq!(
                consumed, *expected_len,
                "Consumed wrong byte count for {} ({})",
                value, label
            );
        }
    }

    // ========================================================================
    // Test 4: Individual codon encoding sizes
    // ========================================================================

    #[test]
    fn test_codon_encoding_sizes() {
        println!("\n══════════════════════════════════════════════════");
        println!("  TEST: Codon Encoding Sizes");
        println!("══════════════════════════════════════════════════");

        // Simple codon (Tier 0 concept, no qualifiers)
        let simple = Codon {
            concept_id: 10, // C_HEAT
            role: RoleId::Agent,
            qualifiers: vec![],
        };
        let simple_bytes = encode_codon(&simple).unwrap();
        println!(
            "  Simple codon (Tier 0, no quals): {} bytes",
            simple_bytes.len()
        );

        // Tier 1 codon with qualifier
        let with_qual = Codon {
            concept_id: C_WATER,
            role: RoleId::Object,
            qualifiers: vec![Qualifier {
                key: "unit".into(),
                value: QualifierValue::Concept(C_CELSIUS),
            }],
        };
        let qual_bytes = encode_codon(&with_qual).unwrap();
        println!(
            "  Tier 1 codon + qualifier:        {} bytes",
            qual_bytes.len()
        );

        // Tier 2 codon
        let tier2 = Codon {
            concept_id: 100_000,
            role: RoleId::Result,
            qualifiers: vec![],
        };
        let tier2_bytes = encode_codon(&tier2).unwrap();
        println!(
            "  Tier 2 codon (no quals):         {} bytes",
            tier2_bytes.len()
        );

        // All should be reasonable
        assert!(simple_bytes.len() < 20, "Simple codon too large");
        assert!(qual_bytes.len() < 50, "Qualified codon too large");
    }

    // ========================================================================
    // Test 5: Bond encoding
    // ========================================================================

    #[test]
    fn test_bond_encoding_size() {
        println!("\n══════════════════════════════════════════════════");
        println!("  TEST: Bond Encoding Sizes");
        println!("══════════════════════════════════════════════════");

        let minimal_bond = Bond {
            target_cid: vec![0u8; 36],
            relation: RelationType::Extends,
            weight: 8000,
            creator: Creator::Human,
            created_at: 1719072000,
            evidence: vec![],
            state: EdgeState::Active,
            initial_weight: None,
            decay: None,
            last_reinforced: None,
            reinforce_count: None,
            bidirectional: None,
            context: vec![],
            order: None,
            required: None,
        };
        let minimal_bytes = encode_bond(&minimal_bond).unwrap();
        println!(
            "  Minimal bond:                 {} bytes",
            minimal_bytes.len()
        );
        println!("  v4 spec estimate:             55-110 bytes");

        let full_bond = Bond {
            target_cid: vec![0xAB; 36],
            relation: RelationType::ReactionTo,
            weight: 7500,
            creator: Creator::Ai,
            created_at: 1719072000,
            evidence: vec![vec![0xCD; 36]],
            state: EdgeState::Active,
            initial_weight: Some(7500),
            decay: Some(DecayRate::Med),
            last_reinforced: Some(1719158400),
            reinforce_count: Some(3),
            bidirectional: Some(false),
            context: vec![C_WATER, C_BOIL],
            order: None,
            required: None,
        };
        let full_bytes = encode_bond(&full_bond).unwrap();
        println!("  Full bond (all fields):       {} bytes", full_bytes.len());

        assert!(
            minimal_bytes.len() < 150,
            "Minimal bond too large: {}",
            minimal_bytes.len()
        );
        assert!(
            full_bytes.len() < 250,
            "Full bond too large: {}",
            full_bytes.len()
        );
    }

    // ========================================================================
    // Test 6: All 10 gene types encode correctly
    // ========================================================================

    #[test]
    fn test_all_gene_types_encode() {
        println!("\n══════════════════════════════════════════════════");
        println!("  TEST: All 10 Gene Types Encode");
        println!("══════════════════════════════════════════════════");

        let genes: Vec<(&str, Gene)> = vec![
            (
                "FactGene",
                Gene::Fact {
                    triples: vec![Triple {
                        subject: 1,
                        predicate: 2,
                        object: 3,
                    }],
                    certainty: 9000,
                    evidence: vec![],
                },
            ),
            (
                "ProcedureGene",
                Gene::Procedure {
                    steps: vec![ProcedureStep {
                        ord: 1,
                        act: C_DO,
                        pre: vec![],
                        tgt: C_WATER,
                        tools: vec![],
                        eff: vec![],
                        warn: vec![],
                    }],
                    total_time: Some(300),
                    difficulty: 2,
                    tools_req: vec![],
                },
            ),
            (
                "ExperienceGene",
                Gene::Experience {
                    scene: vec![],
                    affect: Affect {
                        v: 5000,
                        a: 5000,
                        d: 5000,
                    },
                    canonical: None,
                    perspective: None,
                },
            ),
            (
                "CreativeGene",
                Gene::Creative {
                    steps: vec![],
                    cultural_context: vec![],
                    origin_story: None,
                },
            ),
            (
                "MediaExperienceGene",
                Gene::MediaExperience {
                    id_sys: 1, // IMDB
                    ext_id: b"tt1234567".to_vec(),
                    media_type: 0, // FILM
                    rating: 85,
                    affect: Affect {
                        v: 7000,
                        a: 6000,
                        d: 5000,
                    },
                    spoiler_level: 0,
                },
            ),
            (
                "TestimonyGene",
                Gene::Testimony {
                    triples: vec![Triple {
                        subject: 100,
                        predicate: 200,
                        object: 300,
                    }],
                    claim_type: 1,    // EVENT
                    extraordinary: 0, // MUNDANE
                    witness_count: 1,
                    proximity: 0,           // FIRSTHAND
                    verification_status: 0, // UNVERIFIED
                },
            ),
            (
                "FormalGene",
                Gene::Formal {
                    domain: 0,          // MATH
                    notation_format: 0, // LATEX
                    notation_source: b"E = mc^2".to_vec(),
                    statement_type: 5,      // EQUATION
                    verification_status: 2, // COMPUTATIONALLY
                },
            ),
            (
                "HypothesisGene",
                Gene::Hypothesis {
                    base_type: 0, // → FACT when mature
                    body_codons: vec![Codon {
                        concept_id: 42,
                        role: RoleId::Agent,
                        qualifiers: vec![],
                    }],
                    maturity_level: 2, // HYPOTHESIS
                    confidence: 5000,
                    completeness: 3000,
                    falsifiable: true,
                },
            ),
            (
                "NarrativeGene",
                Gene::Narrative {
                    narrative_type: 0, // FOLKTALE
                    origin_culture: vec![5000],
                    era: 6,      // TIMELESS
                    function: 1, // MORAL_TEACHING
                    sacred: false,
                    moral: vec![],
                    canonical: None,
                },
            ),
            (
                "SensoryGene",
                Gene::Sensory {
                    modality: 0, // VISUAL
                    property: 1000,
                    feature: 2000,
                    result_codons: vec![],
                    sensor_type: 0, // HUMAN_EYE
                    quality: 0,     // RAW
                },
            ),
        ];

        for (name, gene) in &genes {
            let bytes = encode_gene(gene).unwrap();
            let gt = gene.gene_type();
            let (base, ext) = gt.wire_encoding();
            println!(
                "  {:25} → {:>4} bytes  (wire: base={}, ext={:?})",
                name,
                bytes.len(),
                base,
                ext
            );
        }
    }

    // ========================================================================
    // Test 7: Wire format round-trip (header decode)
    // ========================================================================

    #[test]
    fn test_wire_format_header_roundtrip() {
        println!("\n══════════════════════════════════════════════════");
        println!("  TEST: Wire Format Header Round-Trip");
        println!("══════════════════════════════════════════════════");

        // Test with each gene type
        let test_kus = vec![
            ("Fact", make_fact_ku()),
            ("Experience", make_experience_ku()),
        ];

        for (name, ku) in &test_kus {
            let wire = encode_knowledge_unit(ku).unwrap();
            let result = decode_knowledge_unit(&wire);
            assert!(
                result.is_ok(),
                "Failed to decode {} KU: {:?}",
                name,
                result.err()
            );

            let decoded = result.unwrap();
            println!(
                "  {} KU: {} bytes, gene={:?}, payload={} bytes",
                name,
                wire.len(),
                decoded.gene_type,
                decoded.payload.len()
            );
            assert_eq!(decoded.gene_type, ku.gene.gene_type());
        }
    }

    // ========================================================================
    // Test 8: CRC integrity check
    // ========================================================================

    #[test]
    fn test_crc_integrity() {
        let ku = make_fact_ku();
        let mut wire = encode_knowledge_unit(&ku).unwrap();

        // Valid wire should decode fine
        assert!(decode_knowledge_unit(&wire).is_ok());

        // Corrupt a payload byte
        if wire.len() > 10 {
            wire[8] ^= 0xFF;
            let result = decode_knowledge_unit(&wire);
            assert!(result.is_err(), "Should fail CRC check after corruption");
            println!("\n  CRC integrity test: PASSED (detected corruption)");
        }
    }

    // ========================================================================
    // Test 9: EXTENDED gene types wire encoding
    // ========================================================================

    #[test]
    fn test_extended_gene_types() {
        println!("\n══════════════════════════════════════════════════");
        println!("  TEST: EXTENDED Gene Type Wire Encoding");
        println!("══════════════════════════════════════════════════");

        // HypothesisGene → base=7, ext=0x00
        let hypothesis_ku = KnowledgeUnit {
            codons: vec![Codon {
                concept_id: 42,
                role: RoleId::Agent,
                qualifiers: vec![],
            }],
            bonds: vec![],
            gene: Gene::Hypothesis {
                base_type: 0,
                body_codons: vec![],
                maturity_level: 2,
                confidence: 5000,
                completeness: 3000,
                falsifiable: true,
            },
            flags: HeaderFlags::default(),
            epistemic_status: None,
            evidence_type: None,
            trust: None,
            epigenetic: None,
        };

        let wire = encode_knowledge_unit(&hypothesis_ku).unwrap();
        let (_, gene_base) = HeaderFlags::from_byte(wire[3]);
        assert_eq!(gene_base, 7, "Hypothesis should use EXTENDED (base=7)");

        // Payload first byte should be 0x00 (Hypothesis ext)
        assert_eq!(
            wire[6], 0x00,
            "First payload byte should be ext=0x00 for Hypothesis"
        );

        let result = decode_knowledge_unit(&wire);
        assert!(result.is_ok());
        let decoded = result.unwrap();
        assert_eq!(decoded.gene_type, GeneType::Hypothesis);

        println!(
            "  HypothesisGene: {} bytes, FLAGS base=7, ext=0x00 ✓",
            wire.len()
        );

        // NarrativeGene → base=7, ext=0x01
        let narrative_ku = KnowledgeUnit {
            codons: vec![],
            bonds: vec![],
            gene: Gene::Narrative {
                narrative_type: 0,
                origin_culture: vec![],
                era: 6,
                function: 1,
                sacred: false,
                moral: vec![],
                canonical: None,
            },
            flags: HeaderFlags::default(),
            epistemic_status: None,
            evidence_type: None,
            trust: None,
            epigenetic: None,
        };

        let wire = encode_knowledge_unit(&narrative_ku).unwrap();
        assert_eq!(
            wire[8], 0x01,
            "First payload byte should be ext=0x01 for Narrative"
        );

        let result = decode_knowledge_unit(&wire);
        assert!(result.is_ok());
        let decoded = result.unwrap();
        assert_eq!(decoded.gene_type, GeneType::Narrative);

        println!(
            "  NarrativeGene:  {} bytes, FLAGS base=7, ext=0x01 ✓",
            wire.len()
        );
    }

    // ========================================================================
    // NEW Test: Varint max value (u32::MAX)
    // ========================================================================

    #[test]
    fn test_varint_max_value() {
        // Test Tier 3 max (4 bytes)
        let tier3_max = 270_549_119u64;
        let encoded = encode_varint(tier3_max).unwrap();
        assert_eq!(encoded.len(), 4, "Tier 3 max should be 4 bytes");
        let (decoded, consumed) = decode_varint(&encoded).unwrap();
        assert_eq!(decoded, tier3_max, "Roundtrip failed for Tier 3 max");
        assert_eq!(consumed, 4);

        // Test Tier 3+ with large value
        let raw_max = u32::MAX as u64;
        let encoded = encode_varint(raw_max).unwrap();
        assert_eq!(encoded.len(), 5);
        let (decoded, consumed) = decode_varint(&encoded).unwrap();
        assert_eq!(decoded, raw_max);
        assert_eq!(consumed, 5);

        println!("\n  test_varint_max_value: PASSED");
    }

    // ========================================================================
    // NEW Test: Empty KU (0 codons, 0 bonds)
    // ========================================================================

    #[test]
    fn test_empty_ku() {
        let ku = KnowledgeUnit {
            codons: vec![],
            bonds: vec![],
            gene: Gene::Fact {
                triples: vec![],
                certainty: 0,
                evidence: vec![],
            },
            flags: HeaderFlags::default(),
            epistemic_status: None,
            evidence_type: None,
            trust: None,
            epigenetic: None,
        };

        let wire = encode_knowledge_unit(&ku).unwrap();

        // Should still have valid header + CRC
        assert!(wire.len() >= 12, "Empty KU wire too short: {}", wire.len());
        assert_eq!(wire[0], 0x4B);
        assert_eq!(wire[1], 0x44);
        assert_eq!(wire[2], 0x05);

        // Should decode cleanly
        let result = decode_knowledge_unit(&wire);
        assert!(result.is_ok(), "Empty KU decode failed: {:?}", result.err());
        let decoded = result.unwrap();
        assert_eq!(decoded.gene_type, GeneType::Fact);

        // Also test full decoder
        let decoded = decode_knowledge_unit(&wire).unwrap();
        assert_eq!(decoded.gene_type, GeneType::Fact);
        assert_eq!(decoded.version, 0x05);
        assert!(decoded.crc32_valid);

        println!("\n  test_empty_ku: PASSED ({} bytes)", wire.len());
    }

    // ========================================================================
    // NEW Test: Decode round-trip — FactGene
    // ========================================================================

    #[test]
    fn test_decode_roundtrip_fact() {
        let ku = make_fact_ku();
        let wire = encode_knowledge_unit(&ku).unwrap();

        let decoded = decode_knowledge_unit(&wire).unwrap();

        assert_eq!(decoded.version, VERSION);
        assert_eq!(decoded.gene_type, GeneType::Fact);
        assert!(decoded.crc32_valid);
        assert!(!decoded.payload.is_empty(), "Payload should not be empty");

        // Verify flags
        assert!(!decoded.header_flags.has_ecc);
        assert!(!decoded.header_flags.is_encrypted);

        // Verify payload length matches header
        // For non-EXTENDED types, payload = raw CBOR (no ext byte stripped)
        assert_eq!(decoded.payload_len as usize, decoded.payload.len());

        println!(
            "\n  test_decode_roundtrip_fact: PASSED (gene={:?}, payload={} bytes)",
            decoded.gene_type,
            decoded.payload.len()
        );
    }

    // ========================================================================
    // NEW Test: Decode round-trip — ExperienceGene
    // ========================================================================

    #[test]
    fn test_decode_roundtrip_experience() {
        let ku = make_experience_ku();
        let wire = encode_knowledge_unit(&ku).unwrap();

        let decoded = decode_knowledge_unit(&wire).unwrap();

        assert_eq!(decoded.version, VERSION);
        assert_eq!(decoded.gene_type, GeneType::Experience);
        assert!(decoded.crc32_valid);
        assert!(!decoded.payload.is_empty());

        // Experience is base=2, not EXTENDED, so payload = raw CBOR
        assert_eq!(decoded.payload_len as usize, decoded.payload.len());

        println!(
            "\n  test_decode_roundtrip_experience: PASSED (gene={:?}, payload={} bytes)",
            decoded.gene_type,
            decoded.payload.len()
        );
    }

    // ========================================================================
    // NEW Test: Decode truncated data returns error
    // ========================================================================

    #[test]
    fn test_decode_truncated_data() {
        let ku = make_fact_ku();
        let wire = encode_knowledge_unit(&ku).unwrap();

        // Truncate to less than minimum header
        let result = decode_knowledge_unit(&wire[..5]);
        assert!(result.is_err());
        match result.unwrap_err() {
            KuError::PayloadTruncated { .. } => { /* expected */ }
            other => panic!("Expected PayloadTruncated, got: {:?}", other),
        }

        // Truncate to header-only (no payload/CRC)
        if wire.len() > 10 {
            let result = decode_knowledge_unit(&wire[..8]);
            assert!(result.is_err());
        }

        println!("\n  test_decode_truncated_data: PASSED");
    }

    // ========================================================================
    // NEW Test: Wrong magic returns InvalidMagic error
    // ========================================================================

    #[test]
    fn test_decode_wrong_magic() {
        let ku = make_fact_ku();
        let mut wire = encode_knowledge_unit(&ku).unwrap();

        // Corrupt magic bytes
        wire[0] = 0xFF;
        wire[1] = 0xFE;

        let result = decode_knowledge_unit(&wire);
        assert!(result.is_err());
        match result.unwrap_err() {
            KuError::InvalidMagic(m) => {
                assert_eq!(m, [0xFF, 0xFE]);
            }
            other => panic!("Expected InvalidMagic, got: {:?}", other),
        }

        println!("\n  test_decode_wrong_magic: PASSED");
    }

    // ========================================================================
    // NEW Test: CRC corruption returns CrcMismatch error
    // ========================================================================

    #[test]
    fn test_decode_crc_corruption() {
        let ku = make_fact_ku();
        let mut wire = encode_knowledge_unit(&ku).unwrap();

        // Corrupt a payload byte (not the CRC itself — the computed CRC will differ)
        let payload_idx = 8; // somewhere in the payload
        if wire.len() > payload_idx + 4 {
            wire[payload_idx] ^= 0xFF;

            let result = decode_knowledge_unit(&wire);
            assert!(result.is_err());
            match result.unwrap_err() {
                KuError::CrcMismatch { stored, computed } => {
                    assert_ne!(stored, computed, "CRC values should differ");
                }
                other => panic!("Expected CrcMismatch, got: {:?}", other),
            }
        }

        println!("\n  test_decode_crc_corruption: PASSED");
    }

    // ========================================================================
    // Test 3a: Encode with TrustSection
    // ========================================================================

    fn make_trust_section() -> TrustSection {
        TrustSection {
            epistemic_status: EpistemicStatus::Evidence,
            evidence_type: EvidenceType::Experimental,
            verification_level: 3,
            corroboration_count: 5,
            challenge_count: 1,
            error_susceptibility: 0,
            trust_score: 8500,
            confidence: 9000,
            domain_codes: vec![],
            verifications: vec![],
            challenges: vec![],
            ..Default::default()
        }
    }

    #[test]
    fn test_encode_with_trust_section() {
        println!("\n══════════════════════════════════════════════════");
        println!("  TEST 3a: Encode with TrustSection");
        println!("══════════════════════════════════════════════════");

        let trust = make_trust_section();

        let ku = KnowledgeUnit {
            codons: vec![
                Codon {
                    concept_id: C_WATER,
                    role: RoleId::Agent,
                    qualifiers: vec![],
                },
                Codon {
                    concept_id: C_BOIL,
                    role: RoleId::Result,
                    qualifiers: vec![],
                },
            ],
            bonds: vec![],
            gene: Gene::Fact {
                triples: vec![Triple {
                    subject: C_WATER,
                    predicate: C_BOILING_POINT,
                    object: C_CELSIUS,
                }],
                certainty: 9900,
                evidence: vec![],
            },
            flags: HeaderFlags::default(),
            epistemic_status: Some(EpistemicStatus::Evidence),
            evidence_type: Some(EvidenceType::Experimental),
            trust: Some(trust.clone()),
            epigenetic: None,
        };

        // Encode
        let wire = encode_knowledge_unit(&ku).unwrap();
        println!("  Wire total:          {} bytes", wire.len());

        // Verify wire format is valid
        assert_eq!(wire[0], 0x4B, "Magic byte 0");
        assert_eq!(wire[1], 0x44, "Magic byte 1");
        assert_eq!(wire[2], 0x05, "Version v5");

        // Verify CRC is valid by decoding
        let decoded = decode_knowledge_unit(&wire);
        assert!(decoded.is_ok(), "Wire decode failed: {:?}", decoded.err());
        let decoded = decoded.unwrap();
        assert!(decoded.crc32_valid);
        assert_eq!(decoded.gene_type, GeneType::Fact);

        // Full decode to verify trust section is present
        let (_, ku_decoded) = decode_full_knowledge_unit(&wire).unwrap();
        assert!(
            ku_decoded.trust.is_some(),
            "Trust section should be present after decode"
        );
        let trust_decoded = ku_decoded.trust.unwrap();
        assert_eq!(trust_decoded.epistemic_status, EpistemicStatus::Evidence);
        assert_eq!(trust_decoded.evidence_type, EvidenceType::Experimental);
        assert_eq!(trust_decoded.verification_level, 3);
        assert_eq!(trust_decoded.corroboration_count, 5);
        assert_eq!(trust_decoded.challenge_count, 1);
        assert_eq!(trust_decoded.error_susceptibility, 0);
        assert_eq!(trust_decoded.trust_score, 8500);
        assert_eq!(trust_decoded.confidence, 9000);

        // Measure trust section size
        let trust_size = encode_trust(&trust).unwrap().len();
        println!("  TrustSection CBOR:   {} bytes", trust_size);
        println!("  test_encode_with_trust_section: PASSED ✓");
    }

    // ========================================================================
    // Test 3b: Encode with EpigeneticSection
    // ========================================================================

    fn make_epigenetic_section(with_embedding: bool) -> EpigeneticSection {
        EpigeneticSection {
            embedding: if with_embedding {
                vec![0u8; 512]
            } else {
                vec![]
            },
            embedding_binary: vec![],
            embed_version: None,
            valid_from: Some(1719072000),
            valid_until: None,
            recorded_at: Some(1719072000),
            temporal_precision: None,
            temporal_uncertainty: None,
            half_life: None,
            krl: Some(5),
            language: Some(1), // English (numeric code)
            template: None,
            difficulty: None,
            categories: vec![],
            tags: vec![],
            simhash: vec![],
            lsh_buckets: vec![],
            schema_ver: None,
            version: None,
            prev_cid: None,
            superseded_by: None,
        }
    }

    #[test]
    fn test_encode_with_epigenetic() {
        println!("\n══════════════════════════════════════════════════");
        println!("  TEST 3b: Encode with EpigeneticSection");
        println!("══════════════════════════════════════════════════");

        let epigenetic = make_epigenetic_section(true); // with 512-byte embedding

        let ku = KnowledgeUnit {
            codons: vec![Codon {
                concept_id: C_WATER,
                role: RoleId::Agent,
                qualifiers: vec![],
            }],
            bonds: vec![],
            gene: Gene::Fact {
                triples: vec![Triple {
                    subject: C_WATER,
                    predicate: C_BOILING_POINT,
                    object: C_CELSIUS,
                }],
                certainty: 9900,
                evidence: vec![],
            },
            flags: HeaderFlags::default(),
            epistemic_status: None,
            evidence_type: None,
            trust: None,
            epigenetic: Some(epigenetic),
        };

        let wire = encode_knowledge_unit(&ku).unwrap();
        println!("  Wire total (with 512B embed): {} bytes", wire.len());

        // Verify wire format is valid
        let decoded = decode_knowledge_unit(&wire);
        assert!(decoded.is_ok(), "Wire decode failed: {:?}", decoded.err());

        // Full decode to verify epigenetic section
        let (_, ku_decoded) = decode_full_knowledge_unit(&wire).unwrap();
        assert!(
            ku_decoded.epigenetic.is_some(),
            "Epigenetic section should be present"
        );
        let epi = ku_decoded.epigenetic.unwrap();
        assert_eq!(epi.embedding.len(), 512, "Embedding should be 512 bytes");
        assert_eq!(epi.valid_from, Some(1719072000));
        assert_eq!(epi.recorded_at, Some(1719072000));
        assert_eq!(epi.krl, Some(5));
        assert_eq!(epi.language, Some(1));

        // Measure epigenetic section size
        let epi_size = encode_epigenetic(&make_epigenetic_section(true))
            .unwrap()
            .len();
        let epi_no_embed_size = encode_epigenetic(&make_epigenetic_section(false))
            .unwrap()
            .len();
        println!("  EpigeneticSection (with embed):    {} bytes", epi_size);
        println!(
            "  EpigeneticSection (without embed):  {} bytes",
            epi_no_embed_size
        );
        println!("  test_encode_with_epigenetic: PASSED ✓");
    }

    // ========================================================================
    // Test 3c: Full roundtrip with ALL layers (L1-5)
    // ========================================================================

    #[test]
    fn test_full_roundtrip_all_layers() {
        println!("\n══════════════════════════════════════════════════");
        println!("  TEST 3c: Full Roundtrip — All Layers (L1-5)");
        println!("══════════════════════════════════════════════════");

        let trust = make_trust_section();
        let epigenetic = make_epigenetic_section(true);

        let original_ku = KnowledgeUnit {
            // Layer 1: Codons
            codons: vec![
                Codon {
                    concept_id: C_WATER,
                    role: RoleId::Agent,
                    qualifiers: vec![],
                },
                Codon {
                    concept_id: C_BOIL,
                    role: RoleId::Result,
                    qualifiers: vec![],
                },
                Codon {
                    concept_id: 100,
                    role: RoleId::Quantity,
                    qualifiers: vec![Qualifier {
                        key: "unit".into(),
                        value: QualifierValue::Concept(C_CELSIUS),
                    }],
                },
            ],
            // Layer 2: Bonds
            bonds: vec![Bond {
                target_cid: vec![0xABu8; 36],
                relation: RelationType::Corroborates,
                weight: 9000,
                creator: Creator::Human,
                created_at: 1719072000,
                evidence: vec![],
                state: EdgeState::Active,
                initial_weight: Some(9000),
                decay: Some(DecayRate::None),
                last_reinforced: None,
                reinforce_count: None,
                bidirectional: None,
                context: vec![],
                order: None,
                required: None,
            }],
            // Layer 3: Gene
            gene: Gene::Fact {
                triples: vec![Triple {
                    subject: C_WATER,
                    predicate: C_BOILING_POINT,
                    object: C_CELSIUS,
                }],
                certainty: 9900,
                evidence: vec![],
            },
            flags: HeaderFlags::default(),
            epistemic_status: Some(EpistemicStatus::Evidence),
            evidence_type: Some(EvidenceType::Experimental),
            // Layer 4: Trust + Epigenetic
            trust: Some(trust),
            epigenetic: Some(epigenetic),
        };

        // Encode
        let wire = encode_knowledge_unit(&original_ku).unwrap();
        println!("  Encoded wire:        {} bytes", wire.len());

        // Decode
        let (decoded_meta, decoded_ku) = decode_full_knowledge_unit(&wire).unwrap();

        // Verify wire metadata
        assert_eq!(decoded_meta.version, VERSION);
        assert_eq!(decoded_meta.gene_type, GeneType::Fact);
        assert!(decoded_meta.crc32_valid);

        // Verify Layer 1: Codons
        assert_eq!(decoded_ku.codons.len(), 3, "Should have 3 codons");
        assert_eq!(decoded_ku.codons[0].concept_id, C_WATER);
        assert_eq!(decoded_ku.codons[0].role, RoleId::Agent);
        assert_eq!(decoded_ku.codons[1].concept_id, C_BOIL);
        assert_eq!(decoded_ku.codons[2].qualifiers.len(), 1);

        // Verify Layer 2: Bonds
        assert_eq!(decoded_ku.bonds.len(), 1, "Should have 1 bond");
        assert_eq!(decoded_ku.bonds[0].target_cid, vec![0xABu8; 36]);
        assert_eq!(decoded_ku.bonds[0].relation, RelationType::Corroborates);
        assert_eq!(decoded_ku.bonds[0].weight, 9000);

        // Verify Layer 3: Gene
        match &decoded_ku.gene {
            Gene::Fact {
                triples, certainty, ..
            } => {
                assert_eq!(triples.len(), 1);
                assert_eq!(triples[0].subject, C_WATER);
                assert_eq!(*certainty, 9900);
            }
            other => panic!("Expected Fact gene, got: {:?}", other),
        }

        // Verify Layer 4: Trust
        assert!(decoded_ku.trust.is_some(), "Trust should be present");
        let t = decoded_ku.trust.as_ref().unwrap();
        assert_eq!(t.epistemic_status, EpistemicStatus::Evidence);
        assert_eq!(t.evidence_type, EvidenceType::Experimental);
        assert_eq!(t.verification_level, 3);
        assert_eq!(t.corroboration_count, 5);
        assert_eq!(t.challenge_count, 1);
        assert_eq!(t.trust_score, 8500);
        assert_eq!(t.confidence, 9000);

        // Verify Layer 4: Epigenetic
        assert!(
            decoded_ku.epigenetic.is_some(),
            "Epigenetic should be present"
        );
        let e = decoded_ku.epigenetic.as_ref().unwrap();
        assert_eq!(e.embedding.len(), 512);
        assert_eq!(e.valid_from, Some(1719072000));
        assert_eq!(e.recorded_at, Some(1719072000));
        assert_eq!(e.krl, Some(5));
        assert_eq!(e.language, Some(1));

        // Verify epistemic_status and evidence_type top-level fields
        assert_eq!(decoded_ku.epistemic_status, Some(EpistemicStatus::Evidence));
        assert_eq!(decoded_ku.evidence_type, Some(EvidenceType::Experimental));

        println!(
            "  All layers verified: L1(codons) ✓ L2(bonds) ✓ L3(gene) ✓ L4(trust+epi) ✓ L5(CRC) ✓"
        );
        println!("  test_full_roundtrip_all_layers: PASSED ✓");
    }

    // ========================================================================
    // Test 3d: Size comparison (L1-3 vs L1-5)
    // ========================================================================

    #[test]
    fn test_size_comparison() {
        println!("\n══════════════════════════════════════════════════");
        println!("  TEST 3d: Size Comparison (L1-3 vs L1-5)");
        println!("══════════════════════════════════════════════════");

        // L1-3 only KU (same as make_fact_ku)
        let ku_l13 = make_fact_ku();
        let wire_l13 = encode_knowledge_unit(&ku_l13).unwrap();

        // L1-5 with trust only (no embedding)
        let ku_l15_trust = KnowledgeUnit {
            trust: Some(make_trust_section()),
            epigenetic: None,
            ..make_fact_ku()
        };
        let wire_l15_trust = encode_knowledge_unit(&ku_l15_trust).unwrap();

        // L1-5 with trust + epigenetic (no embedding)
        let ku_l15_no_embed = KnowledgeUnit {
            trust: Some(make_trust_section()),
            epigenetic: Some(make_epigenetic_section(false)),
            ..make_fact_ku()
        };
        let wire_l15_no_embed = encode_knowledge_unit(&ku_l15_no_embed).unwrap();

        // L1-5 full with 512B embedding
        let ku_l15_full = KnowledgeUnit {
            trust: Some(make_trust_section()),
            epigenetic: Some(make_epigenetic_section(true)),
            ..make_fact_ku()
        };
        let wire_l15_full = encode_knowledge_unit(&ku_l15_full).unwrap();

        let trust_overhead = wire_l15_trust.len() - wire_l13.len();
        let epi_overhead_no_embed = wire_l15_no_embed.len() - wire_l15_trust.len();
        let epi_overhead_full = wire_l15_full.len() - wire_l15_no_embed.len();

        println!("  L1-3 only:           {} bytes", wire_l13.len());
        println!(
            "  L1-5 (trust only):   {} bytes (+{} trust overhead)",
            wire_l15_trust.len(),
            trust_overhead
        );
        println!(
            "  L1-5 (no embedding): {} bytes (+{} epi overhead)",
            wire_l15_no_embed.len(),
            epi_overhead_no_embed
        );
        println!(
            "  L1-5 (512B embed):   {} bytes (+{} embedding overhead)",
            wire_l15_full.len(),
            epi_overhead_full
        );
        println!("  ────────────────────────────");
        println!("  Trust overhead:      ~{} bytes", trust_overhead);
        println!("  Epi (no embed):      ~{} bytes", epi_overhead_no_embed);
        println!("  Embedding (512B):    ~{} bytes", epi_overhead_full);

        // Size assertions
        assert!(
            wire_l13.len() < 500,
            "L1-3 should be <500B, got {}",
            wire_l13.len()
        );
        assert!(trust_overhead > 0, "Trust should add overhead");
        assert!(
            wire_l15_full.len() > wire_l13.len(),
            "L1-5 should be larger than L1-3"
        );

        println!("  test_size_comparison: PASSED ✓");
    }

    // ========================================================================
    // Test 3e: Empty optional fields (backward compat)
    // ========================================================================

    #[test]
    fn test_empty_optional_fields() {
        println!("\n══════════════════════════════════════════════════");
        println!("  TEST 3e: Empty Optional Fields (Backward Compat)");
        println!("══════════════════════════════════════════════════");

        // KU with trust=None and epigenetic=None (original format)
        let ku_none = KnowledgeUnit {
            codons: vec![Codon {
                concept_id: C_WATER,
                role: RoleId::Agent,
                qualifiers: vec![],
            }],
            bonds: vec![],
            gene: Gene::Fact {
                triples: vec![Triple {
                    subject: C_WATER,
                    predicate: C_BOILING_POINT,
                    object: C_CELSIUS,
                }],
                certainty: 9900,
                evidence: vec![],
            },
            flags: HeaderFlags::default(),
            epistemic_status: None,
            evidence_type: None,
            trust: None,
            epigenetic: None,
        };

        let wire = encode_knowledge_unit(&ku_none).unwrap();
        println!("  Wire size (no trust/epi): {} bytes", wire.len());

        // Should decode cleanly
        let decoded = decode_knowledge_unit(&wire);
        assert!(
            decoded.is_ok(),
            "Should decode without trust/epi: {:?}",
            decoded.err()
        );

        // Full decode should work with None fields
        let (_, ku_decoded) = decode_full_knowledge_unit(&wire).unwrap();
        assert!(ku_decoded.trust.is_none(), "Trust should be None");
        assert!(ku_decoded.epigenetic.is_none(), "Epigenetic should be None");
        assert_eq!(ku_decoded.epistemic_status, None);
        assert_eq!(ku_decoded.evidence_type, None);

        // Verify codons survived
        assert_eq!(ku_decoded.codons.len(), 1);
        assert_eq!(ku_decoded.codons[0].concept_id, C_WATER);

        // Verify gene survived
        match &ku_decoded.gene {
            Gene::Fact { certainty, .. } => assert_eq!(*certainty, 9900),
            other => panic!("Expected Fact, got: {:?}", other),
        }

        println!("  Backward compat verified: None fields roundtrip correctly ✓");
        println!("  test_empty_optional_fields: PASSED ✓");
    }

    // ========================================================================
    // Test: Full Size Report
    // ========================================================================

    #[test]
    fn test_size_report() {
        println!("\n══════════════════════════════════════════════════");
        println!("  KU SIZE REPORT — Layers 1-5");
        println!("══════════════════════════════════════════════════\n");

        // --- Component sizes ---
        let codons = vec![
            Codon {
                concept_id: C_WATER,
                role: RoleId::Agent,
                qualifiers: vec![],
            },
            Codon {
                concept_id: C_BOIL,
                role: RoleId::Result,
                qualifiers: vec![],
            },
            Codon {
                concept_id: 100,
                role: RoleId::Quantity,
                qualifiers: vec![Qualifier {
                    key: "unit".into(),
                    value: QualifierValue::Concept(C_CELSIUS),
                }],
            },
            Codon {
                concept_id: 1,
                role: RoleId::Condition,
                qualifiers: vec![Qualifier {
                    key: "unit".into(),
                    value: QualifierValue::Concept(C_ATM),
                }],
            },
        ];

        let bond = Bond {
            target_cid: vec![0u8; 36],
            relation: RelationType::Qualifies,
            weight: 9500,
            creator: Creator::Human,
            created_at: 1719072000,
            evidence: vec![],
            state: EdgeState::Active,
            initial_weight: Some(9500),
            decay: Some(DecayRate::None),
            last_reinforced: None,
            reinforce_count: None,
            bidirectional: None,
            context: vec![],
            order: None,
            required: None,
        };

        let gene = Gene::Fact {
            triples: vec![Triple {
                subject: C_WATER,
                predicate: C_BOILING_POINT,
                object: C_CELSIUS,
            }],
            certainty: 9900,
            evidence: vec![],
        };

        let trust = make_trust_section();
        let epi_no_embed = make_epigenetic_section(false);
        let epi_with_embed = make_epigenetic_section(true);

        // Measure individual sizes
        let codons_size = encode_codons(&codons).unwrap().len();
        let bond_size = encode_bond(&bond).unwrap().len();
        let gene_size = encode_gene(&gene).unwrap().len();
        let trust_size = encode_trust(&trust).unwrap().len();
        let epi_no_embed_size = encode_epigenetic(&epi_no_embed).unwrap().len();
        let epi_with_embed_size = encode_epigenetic(&epi_with_embed).unwrap().len();

        // Wire totals
        let ku_l13 = KnowledgeUnit {
            codons: codons.clone(),
            bonds: vec![bond.clone()],
            gene: gene.clone(),
            flags: HeaderFlags::default(),
            epistemic_status: Some(EpistemicStatus::Consensus),
            evidence_type: Some(EvidenceType::Experimental),
            trust: None,
            epigenetic: None,
        };
        let wire_l13 = encode_knowledge_unit(&ku_l13).unwrap();

        let ku_l15 = KnowledgeUnit {
            trust: Some(trust.clone()),
            epigenetic: Some(epi_no_embed.clone()),
            ..ku_l13.clone()
        };
        let wire_l15 = encode_knowledge_unit(&ku_l15).unwrap();

        let ku_l15_embed = KnowledgeUnit {
            trust: Some(trust.clone()),
            epigenetic: Some(epi_with_embed.clone()),
            ..ku_l13.clone()
        };
        let wire_l15_embed = encode_knowledge_unit(&ku_l15_embed).unwrap();

        println!("=== KU Size Report ===");
        println!("Layer 1 (Header):     6B");
        println!("Layer 1 (Codons):     {}B  (4 codons)", codons_size);
        println!("Layer 2 (Bonds):      {}B  (1 bond)", bond_size);
        println!("Layer 3 (Gene):       {}B  (FactGene)", gene_size);
        println!("Layer 4 (Trust):      {}B", trust_size);
        println!(
            "Layer 4 (Epigenetic): {}B (without embedding)",
            epi_no_embed_size
        );
        println!(
            "Layer 4 (Epigenetic): {}B (with 512B embedding)",
            epi_with_embed_size
        );
        println!("Layer 5 (CRC):        4B");
        println!("---");
        println!("Total (L1-3 only):    {}B", wire_l13.len());
        println!("Total (L1-5 full):    {}B", wire_l15.len());
        println!("Total (L1-5 + embed): {}B", wire_l15_embed.len());

        // Also print the full breakdown using the helper function
        println!("\n--- size_breakdown_full() for L1-5 + embed ---");
        let report = size_breakdown_full(&ku_l15_embed).unwrap();
        print!("{}", report);

        println!("\n  test_size_report: PASSED ✓");
    }

    // ========================================================================
    // ★ v5 Test: Composite Gene encode/decode roundtrip
    // ========================================================================

    #[test]
    fn test_composite_gene_roundtrip() {
        println!("\n══════════════════════════════════════════════════");
        println!("  TEST: ★ v5 Composite Gene — Encode/Decode Roundtrip");
        println!("══════════════════════════════════════════════════");

        use crate::types::{
            Completeness, CompositeConstraint, CompositeEntry, CompositeType, StructuralRole,
        };

        // Build a Composite Gene with 3 members (simulating a wing design doc)
        let member1_cid = vec![0x01u8; 32]; // sweep angle fact
        let member2_cid = vec![0x02u8; 32]; // wing area fact
        let member3_cid = vec![0x03u8; 32]; // drag polar formal

        let composite_gene = Gene::Composite {
            members: vec![
                CompositeEntry {
                    cid: member1_cid.clone(),
                    order: 0,
                    role: StructuralRole::Section,
                    required: true,
                    label: 1001,                 // WING_SWEEP concept
                    expected_gene_type: Some(0), // Fact
                },
                CompositeEntry {
                    cid: member2_cid.clone(),
                    order: 1,
                    role: StructuralRole::Section,
                    required: true,
                    label: 1002,                 // WING_AREA concept
                    expected_gene_type: Some(0), // Fact
                },
                CompositeEntry {
                    cid: member3_cid.clone(),
                    order: 2,
                    role: StructuralRole::Appendix,
                    required: false,
                    label: 1003,                 // DRAG_POLAR concept
                    expected_gene_type: Some(6), // Formal
                },
            ],
            constraints: vec![CompositeConstraint {
                name: "sweep_requires_supercritical".into(),
                source_cid: member1_cid.clone(),
                target_cid: member2_cid.clone(),
                condition: vec![0xA1, 0x61, 0x73, 0x01], // example KQL-Lite bytecode
                severity: 2,                             // ERROR
            }],
            cluster_version: 1,
            max_depth: 255,
            composite_type: CompositeType::Specification,
            schema: Some(9999), // AEROSPACE_DESIGN_DOC
            completeness: Completeness::Partial,
            summary_codons: vec![Codon {
                concept_id: 1001,
                role: RoleId::Object,
                qualifiers: vec![],
            }],
        };

        // Verify gene type
        assert_eq!(composite_gene.gene_type(), GeneType::Composite);
        let (base, ext) = GeneType::Composite.wire_encoding();
        assert_eq!(base, 7, "Composite is EXTENDED type");
        assert_eq!(ext, Some(0x03), "Composite ext byte is 0x03");

        // Build KU
        let ku = KnowledgeUnit {
            codons: vec![Codon {
                concept_id: 1001,
                role: RoleId::Object,
                qualifiers: vec![],
            }],
            bonds: vec![],
            gene: composite_gene,
            flags: HeaderFlags::default(),
            epistemic_status: None,
            evidence_type: None,
            trust: None,
            epigenetic: None,
        };

        // Encode
        let wire = encode_knowledge_unit(&ku).unwrap();
        println!("  Wire total:          {} bytes", wire.len());

        // Verify v5 header
        assert_eq!(wire[0], 0x4B, "Magic 'K'");
        assert_eq!(wire[1], 0x44, "Magic 'D'");
        assert_eq!(wire[2], 0x05, "Version v5");

        // Verify EXTENDED gene type
        let (_, gene_base) = HeaderFlags::from_byte(wire[3]);
        assert_eq!(gene_base, 7, "Gene base should be 7 (EXTENDED)");
        assert_eq!(wire[8], 0x03, "Ext byte should be 0x03 (Composite)");

        // Decode header
        let decoded = decode_knowledge_unit(&wire).unwrap();
        assert_eq!(decoded.gene_type, GeneType::Composite);
        assert_eq!(decoded.version, 0x05);
        assert!(decoded.crc32_valid);

        // Full decode + verify roundtrip
        let (_, ku_back) = decode_full_knowledge_unit(&wire).unwrap();

        // Verify Composite gene content survived roundtrip
        if let Gene::Composite {
            members,
            constraints,
            cluster_version,
            max_depth,
            composite_type,
            schema,
            completeness,
            summary_codons,
        } = &ku_back.gene
        {
            assert_eq!(members.len(), 3, "Should have 3 members");
            assert_eq!(members[0].cid, vec![0x01u8; 32]);
            assert_eq!(members[0].order, 0);
            assert_eq!(members[0].role, StructuralRole::Section);
            assert!(members[0].required);
            assert_eq!(members[0].label, 1001);
            assert_eq!(members[0].expected_gene_type, Some(0));

            assert_eq!(members[2].role, StructuralRole::Appendix);
            assert!(!members[2].required);

            assert_eq!(constraints.len(), 1);
            assert_eq!(constraints[0].name, "sweep_requires_supercritical");
            assert_eq!(constraints[0].severity, 2);

            assert_eq!(*cluster_version, 1);
            assert_eq!(*max_depth, 255);
            assert_eq!(*composite_type, CompositeType::Specification);
            assert_eq!(*schema, Some(9999));
            assert_eq!(*completeness, Completeness::Partial);
            assert_eq!(summary_codons.len(), 1);
            assert_eq!(summary_codons[0].concept_id, 1001);

            println!("  Members:             {} entries", members.len());
            println!("  Constraints:         {} rules", constraints.len());
            println!("  Composite type:      {:?}", composite_type);
            println!("  Completeness:        {:?}", completeness);
            println!("  Max depth:           {}", max_depth);
            println!("  Schema:              {:?}", schema);
        } else {
            panic!(
                "Expected Gene::Composite, got {:?}",
                ku_back.gene.gene_type()
            );
        }

        println!("\n  test_composite_gene_roundtrip: PASSED ✓");
    }

    // ========================================================================
    // ★ v5 Test: Bond with order and required fields
    // ========================================================================

    #[test]
    fn test_bond_v5_order_required_fields() {
        println!("\n══════════════════════════════════════════════════");
        println!("  TEST: ★ v5 Bond — order + required fields");
        println!("══════════════════════════════════════════════════");

        // Bond WITH order and required (v5 fields populated)
        let bond_with_order = Bond {
            target_cid: vec![0xAAu8; 36],
            relation: RelationType::PartOf,
            weight: 9000,
            creator: Creator::Human,
            created_at: 1719072000,
            evidence: vec![],
            state: EdgeState::Active,
            initial_weight: Some(9000),
            decay: Some(DecayRate::None),
            last_reinforced: None,
            reinforce_count: None,
            bidirectional: None,
            context: vec![],
            order: Some(3),       // ★ v5: position 3
            required: Some(true), // ★ v5: required bond
        };

        let encoded = encode_bond(&bond_with_order).unwrap();
        println!("  Bond with order+required: {} bytes", encoded.len());

        // Decode back via CBOR
        let decoded: Bond = ciborium::from_reader(&encoded[..]).unwrap();
        assert_eq!(decoded.order, Some(3));
        assert_eq!(decoded.required, Some(true));
        assert_eq!(decoded.relation, RelationType::PartOf);

        // Bond WITHOUT order and required (backward compatible)
        let bond_no_order = Bond {
            target_cid: vec![0xBBu8; 36],
            relation: RelationType::Extends,
            weight: 8000,
            creator: Creator::Human,
            created_at: 1719072000,
            evidence: vec![],
            state: EdgeState::Active,
            initial_weight: None,
            decay: None,
            last_reinforced: None,
            reinforce_count: None,
            bidirectional: None,
            context: vec![],
            order: None,
            required: None,
        };

        let encoded2 = encode_bond(&bond_no_order).unwrap();
        println!("  Bond without order:       {} bytes", encoded2.len());

        // v5 bond should be slightly larger due to extra fields
        assert!(
            encoded.len() > encoded2.len(),
            "Bond with order should be larger: {} vs {}",
            encoded.len(),
            encoded2.len()
        );

        let decoded2: Bond = ciborium::from_reader(&encoded2[..]).unwrap();
        assert_eq!(decoded2.order, None);
        assert_eq!(decoded2.required, None);

        println!("\n  test_bond_v5_order_required_fields: PASSED ✓");
    }

    // ========================================================================
    // ★ v5 Test: Composite Gene in full KU with all layers
    // ========================================================================

    #[test]
    fn test_composite_gene_full_layers() {
        println!("\n══════════════════════════════════════════════════");
        println!("  TEST: ★ v5 Composite Gene — Full L1-5 roundtrip");
        println!("══════════════════════════════════════════════════");

        use crate::types::{Completeness, CompositeEntry, CompositeType, StructuralRole};

        let trust = make_trust_section();
        let epi = make_epigenetic_section(false);

        let ku = KnowledgeUnit {
            codons: vec![
                Codon {
                    concept_id: 5001,
                    role: RoleId::Object,
                    qualifiers: vec![],
                },
                Codon {
                    concept_id: 5002,
                    role: RoleId::Quality,
                    qualifiers: vec![],
                },
            ],
            bonds: vec![Bond {
                target_cid: vec![0xFFu8; 36],
                relation: RelationType::PartOf,
                weight: 9500,
                creator: Creator::Human,
                created_at: 1719072000,
                evidence: vec![],
                state: EdgeState::Active,
                initial_weight: Some(9500),
                decay: Some(DecayRate::None),
                last_reinforced: None,
                reinforce_count: None,
                bidirectional: None,
                context: vec![],
                order: Some(0),
                required: Some(true),
            }],
            gene: Gene::Composite {
                members: vec![CompositeEntry {
                    cid: vec![0xDE; 32],
                    order: 0,
                    role: StructuralRole::Chapter,
                    required: true,
                    label: 6001,
                    expected_gene_type: None,
                }],
                constraints: vec![],
                cluster_version: 42,
                max_depth: 8,
                composite_type: CompositeType::Document,
                schema: None,
                completeness: Completeness::Draft,
                summary_codons: vec![],
            },
            flags: HeaderFlags::default(),
            epistemic_status: Some(EpistemicStatus::Evidence),
            evidence_type: Some(EvidenceType::Experimental),
            trust: Some(trust),
            epigenetic: Some(epi),
        };

        let wire = encode_knowledge_unit(&ku).unwrap();
        let (decoded, ku_back) = decode_full_knowledge_unit(&wire).unwrap();

        assert_eq!(decoded.gene_type, GeneType::Composite);
        assert_eq!(decoded.version, 0x05);
        assert!(decoded.crc32_valid);

        // Verify trust survived
        assert!(ku_back.trust.is_some());
        assert!(ku_back.epigenetic.is_some());

        // Verify bond v5 fields survived
        assert_eq!(ku_back.bonds[0].order, Some(0));
        assert_eq!(ku_back.bonds[0].required, Some(true));

        // Verify composite gene survived
        if let Gene::Composite {
            cluster_version,
            max_depth,
            ..
        } = &ku_back.gene
        {
            assert_eq!(*cluster_version, 42);
            assert_eq!(*max_depth, 8);
        } else {
            panic!("Expected Composite gene");
        }

        // Print size report
        let report = size_breakdown_full(&ku).unwrap();
        print!("{}", report);

        println!("\n  test_composite_gene_full_layers: PASSED ✓");
    }

    // ========================================================================
    // ★ DEMO: Vietnamese text → KU DNA ("Bơi ếch" / Breaststroke)
    // ========================================================================

    #[test]
    fn test_demo_boi_ech_to_ku_dna() {
        println!("\n══════════════════════════════════════════════════════════════");
        println!("  DEMO: 🧬 Vietnamese Text → KU DNA");
        println!("  \"Bơi ếch là kiểu bơi cơ bản mô phỏng chuyển động con ếch\"");
        println!("══════════════════════════════════════════════════════════════");

        use crate::types::{Completeness, CompositeEntry, CompositeType, StructuralRole};

        // ── Concept Registry (language-agnostic IDs) ──────────────────────
        const C_BREASTSTROKE: ConceptId = 500;
        const C_SWIMMING_STYLE: ConceptId = 501;
        const C_BASIC_LEVEL: ConceptId = 502;
        const C_FROG: ConceptId = 503;
        const C_MOVEMENT_SIM: ConceptId = 504;
        const C_WATER_ENV: ConceptId = 505;
        const C_SWIMMER: ConceptId = 506;
        const C_PRONE_POS: ConceptId = 507;
        const C_ARM_SWEEP: ConceptId = 508;
        const C_LEG_KICK: ConceptId = 509;
        const C_BREATHING: ConceptId = 510;
        const C_GLIDE: ConceptId = 511;
        const C_FORWARD: ConceptId = 512;
        const C_RHYTHMIC: ConceptId = 513;
        const C_ENERGY_EFF: ConceptId = 514;
        const C_CYCLIC: ConceptId = 515;

        // ── KU #1: Fact — Definition ──────────────────────────────────────
        // "Bơi ếch là kiểu bơi cơ bản mô phỏng chuyển động con ếch"
        let ku1 = KnowledgeUnit {
            codons: vec![
                Codon {
                    concept_id: C_BREASTSTROKE,
                    role: RoleId::Object,
                    qualifiers: vec![],
                },
                Codon {
                    concept_id: C_SWIMMING_STYLE,
                    role: RoleId::Quality,
                    qualifiers: vec![Qualifier {
                        key: "level".into(),
                        value: QualifierValue::Concept(C_BASIC_LEVEL),
                    }],
                },
                Codon {
                    concept_id: C_MOVEMENT_SIM,
                    role: RoleId::Manner,
                    qualifiers: vec![],
                },
                Codon {
                    concept_id: C_FROG,
                    role: RoleId::Agent,
                    qualifiers: vec![],
                },
                Codon {
                    concept_id: C_WATER_ENV,
                    role: RoleId::Location,
                    qualifiers: vec![],
                },
            ],
            bonds: vec![],
            gene: Gene::Fact {
                triples: vec![
                    Triple {
                        subject: C_BREASTSTROKE,
                        predicate: C_SWIMMING_STYLE,
                        object: C_BASIC_LEVEL,
                    },
                    Triple {
                        subject: C_BREASTSTROKE,
                        predicate: C_MOVEMENT_SIM,
                        object: C_FROG,
                    },
                ],
                certainty: 9900, // 99% — established sports science
                evidence: vec![],
            },
            flags: HeaderFlags::default(),
            epistemic_status: Some(EpistemicStatus::Consensus),
            evidence_type: Some(EvidenceType::Experimental),
            trust: None,
            epigenetic: None,
        };

        let wire1 = encode_knowledge_unit(&ku1).unwrap();
        println!("\n  KU#1 [Fact: Definition]");
        println!(
            "    Codons:    {} (BREASTSTROKE, STYLE, SIMULATION, FROG, WATER)",
            ku1.codons.len()
        );
        println!("    Triples:   2 (IS_A + SIMULATES)");
        println!("    Certainty: 99%");
        println!("    Wire:      {} bytes", wire1.len());
        println!("    Header:    {:02X?}", &wire1[..8]);

        // Verify
        let (d1, k1) = decode_full_knowledge_unit(&wire1).unwrap();
        assert_eq!(d1.gene_type, GeneType::Fact);
        assert_eq!(k1.codons.len(), 5);
        if let Gene::Fact {
            certainty, triples, ..
        } = &k1.gene
        {
            assert_eq!(*certainty, 9900);
            assert_eq!(triples.len(), 2);
        }

        // ── KU #2: Procedure — Swimming Cycle ────────────────────────────
        // quạt tay → đạp chân → lấy hơi → lướt (cyclic)
        let ku2 = KnowledgeUnit {
            codons: vec![
                Codon {
                    concept_id: C_SWIMMER,
                    role: RoleId::Agent,
                    qualifiers: vec![],
                },
                Codon {
                    concept_id: C_PRONE_POS,
                    role: RoleId::Condition,
                    qualifiers: vec![],
                },
                Codon {
                    concept_id: C_CYCLIC,
                    role: RoleId::Manner,
                    qualifiers: vec![],
                },
                Codon {
                    concept_id: C_FORWARD,
                    role: RoleId::Purpose,
                    qualifiers: vec![],
                },
            ],
            bonds: vec![],
            gene: Gene::Procedure {
                steps: vec![
                    ProcedureStep {
                        ord: 0,
                        act: C_ARM_SWEEP, // quạt tay
                        tgt: C_WATER_ENV,
                        pre: vec![Codon {
                            concept_id: C_PRONE_POS,
                            role: RoleId::Condition,
                            qualifiers: vec![],
                        }],
                        tools: vec![],
                        eff: vec![],
                        warn: vec![],
                    },
                    ProcedureStep {
                        ord: 1,
                        act: C_LEG_KICK, // thu và đạp chân
                        tgt: C_WATER_ENV,
                        pre: vec![],
                        tools: vec![],
                        eff: vec![Codon {
                            concept_id: C_FORWARD,
                            role: RoleId::Result,
                            qualifiers: vec![],
                        }],
                        warn: vec![],
                    },
                    ProcedureStep {
                        ord: 2,
                        act: C_BREATHING, // lấy hơi
                        tgt: C_SWIMMER,
                        pre: vec![],
                        tools: vec![],
                        eff: vec![],
                        warn: vec![],
                    },
                    ProcedureStep {
                        ord: 3,
                        act: C_GLIDE, // lướt nước
                        tgt: C_FORWARD,
                        pre: vec![],
                        tools: vec![],
                        eff: vec![Codon {
                            concept_id: C_FORWARD,
                            role: RoleId::Result,
                            qualifiers: vec![],
                        }],
                        warn: vec![],
                    },
                ],
                total_time: None,  // cyclic — no fixed duration
                difficulty: 1,     // beginner-friendly
                tools_req: vec![], // no equipment needed
            },
            flags: HeaderFlags::default(),
            epistemic_status: Some(EpistemicStatus::Consensus),
            evidence_type: Some(EvidenceType::Observational),
            trust: None,
            epigenetic: None,
        };

        let wire2 = encode_knowledge_unit(&ku2).unwrap();
        println!("\n  KU#2 [Procedure: Swimming Cycle]");
        println!("    Steps:     4 (ARM_SWEEP → LEG_KICK → BREATHING → GLIDE) 🔄");
        println!("    Difficulty: 1 (beginner)");
        println!("    Wire:      {} bytes", wire2.len());
        println!("    Header:    {:02X?}", &wire2[..8]);

        // Verify
        let (d2, k2) = decode_full_knowledge_unit(&wire2).unwrap();
        assert_eq!(d2.gene_type, GeneType::Procedure);
        if let Gene::Procedure {
            steps, difficulty, ..
        } = &k2.gene
        {
            assert_eq!(steps.len(), 4);
            assert_eq!(steps[0].act, C_ARM_SWEEP);
            assert_eq!(steps[1].act, C_LEG_KICK);
            assert_eq!(steps[2].act, C_BREATHING);
            assert_eq!(steps[3].act, C_GLIDE);
            assert_eq!(*difficulty, 1);
        }

        // ── KU #3: Fact — Properties ─────────────────────────────────────
        // "nhịp nhàng, ít tốn sức"
        let ku3 = KnowledgeUnit {
            codons: vec![
                Codon {
                    concept_id: C_BREASTSTROKE,
                    role: RoleId::Object,
                    qualifiers: vec![],
                },
                Codon {
                    concept_id: C_RHYTHMIC,
                    role: RoleId::Quality,
                    qualifiers: vec![],
                },
                Codon {
                    concept_id: C_ENERGY_EFF,
                    role: RoleId::Quality,
                    qualifiers: vec![],
                },
            ],
            bonds: vec![],
            gene: Gene::Fact {
                triples: vec![
                    Triple {
                        subject: C_BREASTSTROKE,
                        predicate: C_RHYTHMIC,
                        object: C_RHYTHMIC,
                    },
                    Triple {
                        subject: C_BREASTSTROKE,
                        predicate: C_ENERGY_EFF,
                        object: C_ENERGY_EFF,
                    },
                ],
                certainty: 8500, // 85% — subjective but widely agreed
                evidence: vec![],
            },
            flags: HeaderFlags::default(),
            epistemic_status: Some(EpistemicStatus::Evidence),
            evidence_type: Some(EvidenceType::Observational),
            trust: None,
            epigenetic: None,
        };

        let wire3 = encode_knowledge_unit(&ku3).unwrap();
        println!("\n  KU#3 [Fact: Properties]");
        println!("    Qualities: RHYTHMIC + ENERGY_EFFICIENT");
        println!("    Certainty: 85%");
        println!("    Wire:      {} bytes", wire3.len());

        let (d3, _) = decode_full_knowledge_unit(&wire3).unwrap();
        assert_eq!(d3.gene_type, GeneType::Fact);

        // ── KU #4: Composite — Full Document ─────────────────────────────
        // Groups KU#1-3 into a structured cluster
        let ku4 = KnowledgeUnit {
            codons: vec![
                Codon {
                    concept_id: C_BREASTSTROKE,
                    role: RoleId::Object,
                    qualifiers: vec![],
                },
                Codon {
                    concept_id: C_SWIMMING_STYLE,
                    role: RoleId::Quality,
                    qualifiers: vec![],
                },
            ],
            bonds: vec![],
            gene: Gene::Composite {
                members: vec![
                    CompositeEntry {
                        cid: blake3_placeholder(&wire1), // CID of KU#1
                        order: 0,
                        role: StructuralRole::Chapter,
                        required: true,
                        label: C_BREASTSTROKE,       // "Definition"
                        expected_gene_type: Some(0), // Fact
                    },
                    CompositeEntry {
                        cid: blake3_placeholder(&wire2), // CID of KU#2
                        order: 1,
                        role: StructuralRole::Chapter,
                        required: true,
                        label: C_CYCLIC,             // "Technique"
                        expected_gene_type: Some(1), // Procedure
                    },
                    CompositeEntry {
                        cid: blake3_placeholder(&wire3), // CID of KU#3
                        order: 2,
                        role: StructuralRole::Section,
                        required: false,
                        label: C_RHYTHMIC,           // "Properties"
                        expected_gene_type: Some(0), // Fact
                    },
                ],
                constraints: vec![],
                cluster_version: 1,
                max_depth: 255,
                composite_type: CompositeType::Document,
                schema: None,
                completeness: Completeness::Complete,
                summary_codons: vec![Codon {
                    concept_id: C_BREASTSTROKE,
                    role: RoleId::Object,
                    qualifiers: vec![],
                }],
            },
            flags: HeaderFlags::default(),
            epistemic_status: None,
            evidence_type: None,
            trust: None,
            epigenetic: None,
        };

        let wire4 = encode_knowledge_unit(&ku4).unwrap();
        println!("\n  KU#4 [Composite: Full Document]");
        println!("    Members:   3 (Definition + Technique + Properties)");
        println!("    Type:      Document");
        println!("    Complete:  true");
        println!("    Wire:      {} bytes", wire4.len());
        println!("    Header:    {:02X?}", &wire4[..9]); // +1 for ext byte

        let (d4, k4) = decode_full_knowledge_unit(&wire4).unwrap();
        assert_eq!(d4.gene_type, GeneType::Composite);
        if let Gene::Composite {
            members,
            composite_type,
            completeness,
            ..
        } = &k4.gene
        {
            assert_eq!(members.len(), 3);
            assert_eq!(*composite_type, CompositeType::Document);
            assert_eq!(*completeness, Completeness::Complete);
            assert_eq!(members[0].role, StructuralRole::Chapter);
            assert_eq!(members[1].expected_gene_type, Some(1)); // Procedure
        }

        // ── Summary ──────────────────────────────────────────────────────
        let total = wire1.len() + wire2.len() + wire3.len() + wire4.len();
        let original_text = "Bơi ếch là kiểu bơi cơ bản mô phỏng chuyển động của con ếch dưới nước. Người bơi nằm úp, thực hiện chu kỳ lặp lại liên tục bao gồm: quạt tay, thu và đạp chân, lấy hơi, và lướt nước để tiến về phía trước một cách nhịp nhàng, ít tốn sức";
        let text_bytes = original_text.len();

        println!("\n  ═══════════════════════════════════════════════════");
        println!("  📊 SUMMARY: Text → KU DNA Conversion");
        println!("  ═══════════════════════════════════════════════════");
        println!("  Original text (UTF-8):  {} bytes", text_bytes);
        println!("  ───────────────────────────────────────────────────");
        println!("  KU#1 Fact (definition): {} bytes", wire1.len());
        println!("  KU#2 Procedure (cycle): {} bytes", wire2.len());
        println!("  KU#3 Fact (properties): {} bytes", wire3.len());
        println!("  KU#4 Composite (doc):   {} bytes", wire4.len());
        println!("  ───────────────────────────────────────────────────");
        println!(
            "  Total KU DNA:           {} bytes ({:.1}x text)",
            total,
            total as f64 / text_bytes as f64
        );
        println!("  ═══════════════════════════════════════════════════");
        println!("  ✅ Language-agnostic:   ConceptIds, no Vietnamese stored");
        println!("  ✅ Machine-queryable:   \"Step 3?\" → BREATHING");
        println!("  ✅ Composable:          Bond to 'butterfly stroke' KU");
        println!("  ✅ Merkle-verifiable:   CRC-32 + BLAKE3 CIDs");
        println!("  ═══════════════════════════════════════════════════");

        println!("\n  test_demo_boi_ech_to_ku_dna: PASSED ✓ 🧬");
    }

    /// Helper: generate a BLAKE3-like placeholder CID from wire bytes.
    /// In production, this would be actual BLAKE3 hash.
    fn blake3_placeholder(wire: &[u8]) -> Vec<u8> {
        // Use CRC32 repeated to fill 32 bytes (demo only)
        let crc = crc32fast::hash(wire);
        let mut cid = Vec::with_capacity(32);
        for _ in 0..8 {
            cid.extend_from_slice(&crc.to_be_bytes());
        }
        cid
    }
}

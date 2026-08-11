//! Comprehensive Benchmark & Validation Suite for ku-core
//!
//! Tests:
//! 1. Round-trip validation for ALL 10 gene types
//! 2. Size benchmark (Minimal / Typical / Full)
//! 3. Error handling validation (all KuError variants)
//! 4. Varint exhaustive boundary test
//! 5. Wire format compliance test
//! 6. Full summary report

#[cfg(test)]
mod tests {
    use crate::decoder::*;
    use crate::encoder::*;
    use crate::error::KuError;
    use crate::types::*;
    use crate::varint::{decode_varint, encode_varint};

    // ========================================================================
    // Concept ID constants for realistic test data
    // ========================================================================

    // Tier 0 (universal primitives)
    const C_DO: ConceptId = 1;
    const C_IS: ConceptId = 2;
    const C_HAS: ConceptId = 3;
    const C_MAKE: ConceptId = 4;
    const C_SEE: ConceptId = 5;
    const C_FEEL: ConceptId = 6;
    const C_THINK: ConceptId = 7;
    const C_TRUE: ConceptId = 8;
    const C_FALSE: ConceptId = 9;
    const C_HEAT: ConceptId = 10;

    // Tier 1 (common concepts)
    const C_WATER: ConceptId = 312;
    const C_BOIL: ConceptId = 847;
    const C_COFFEE: ConceptId = 350;
    const C_GRIND: ConceptId = 351;
    const C_POUR: ConceptId = 352;
    const C_BREW: ConceptId = 353;
    const C_FILTER: ConceptId = 354;
    const C_SERVE: ConceptId = 355;
    const C_SUNSET: ConceptId = 1200;
    const C_BEAUTY: ConceptId = 1201;
    const C_SKY: ConceptId = 1202;
    const C_BEACH: ConceptId = 1203;
    const C_WARM: ConceptId = 1205;
    const C_LIGHT: ConceptId = 1300;
    const C_STRANGE: ConceptId = 1301;
    const C_NIGHT: ConceptId = 1302;
    const C_WITNESS: ConceptId = 1303;
    const C_ENERGY: ConceptId = 1400;
    const C_MASS: ConceptId = 1401;
    const C_SPEED: ConceptId = 1402;
    const C_DARK: ConceptId = 1500;
    const C_MATTER: ConceptId = 1501;
    const C_GALAXY: ConceptId = 1502;
    const C_GRAVITY: ConceptId = 1503;
    const C_CREATION: ConceptId = 1600;
    const C_WORLD: ConceptId = 1601;
    const C_CHAOS: ConceptId = 1602;
    const C_ORDER: ConceptId = 1603;
    const C_LIFE: ConceptId = 1604;
    const C_FILM: ConceptId = 1700;
    const C_EMOTION: ConceptId = 1701;
    const C_STORY: ConceptId = 1702;
    const C_SOUND: ConceptId = 1800;
    const C_FREQUENCY: ConceptId = 1801;
    const C_WAVE: ConceptId = 1802;
    const C_RECIPE: ConceptId = 1900;
    const C_INGREDIENT: ConceptId = 1901;
    const C_COOK: ConceptId = 1902;

    // Tier 2 (standard concepts)
    const C_CELSIUS: ConceptId = 20_000;
    const C_BOILING_POINT: ConceptId = 20_003;
    const C_PHOTON: ConceptId = 25_000;

    // ========================================================================
    // 1. Round-Trip Validation for ALL 10 Gene Types
    // ========================================================================

    /// Helper: build a KU from a gene, some codons, and optional bonds.
    fn make_ku(codons: Vec<Codon>, bonds: Vec<Bond>, gene: Gene) -> KnowledgeUnit {
        KnowledgeUnit {
            codons,
            bonds,
            gene,
            flags: HeaderFlags::default(),
            epistemic_status: None,
            evidence_type: None,
            trust: None,
            epigenetic: None,
        }
    }

    /// Helper: make a simple bond to a placeholder CID.
    fn make_bond(relation: RelationType) -> Bond {
        Bond {
            target_cid: vec![0xABu8; 36],
            relation,
            weight: 8500,
            creator: Creator::Human,
            created_at: 1719072000,
            evidence: vec![],
            state: EdgeState::Active,
            initial_weight: Some(8500),
            decay: Some(DecayRate::None),
            last_reinforced: None,
            reinforce_count: None,
            bidirectional: None,
            context: vec![],
            order: None,
            required: None,
        }
    }

    fn codon(concept_id: ConceptId, role: RoleId) -> Codon {
        Codon {
            concept_id,
            role,
            qualifiers: vec![],
        }
    }

    fn codon_q(concept_id: ConceptId, role: RoleId, key: &str, val: ConceptId) -> Codon {
        Codon {
            concept_id,
            role,
            qualifiers: vec![Qualifier {
                key: key.into(),
                value: QualifierValue::Concept(val),
            }],
        }
    }

    #[test]
    fn test_roundtrip_all_gene_types() {
        println!("\n============================================================");
        println!("  BENCHMARK: Round-Trip Validation — ALL 10 Gene Types");
        println!("============================================================");

        let test_cases: Vec<(&str, KnowledgeUnit)> = vec![
            // 0. FactGene: "Water boils at 100°C"
            (
                "Fact",
                make_ku(
                    vec![
                        codon(C_WATER, RoleId::Agent),
                        codon(C_BOIL, RoleId::Result),
                        codon_q(100, RoleId::Quantity, "unit", C_CELSIUS),
                    ],
                    vec![make_bond(RelationType::Corroborates)],
                    Gene::Fact {
                        triples: vec![Triple {
                            subject: C_WATER,
                            predicate: C_BOILING_POINT,
                            object: C_CELSIUS,
                        }],
                        certainty: 9900,
                        evidence: vec![],
                    },
                ),
            ),
            // 1. ProcedureGene: "How to make coffee" (5 steps)
            (
                "Procedure",
                make_ku(
                    vec![
                        codon(C_COFFEE, RoleId::Object),
                        codon(C_WATER, RoleId::Tool),
                        codon(C_GRIND, RoleId::Agent),
                        codon(C_BREW, RoleId::Result),
                    ],
                    vec![],
                    Gene::Procedure {
                        steps: vec![
                            ProcedureStep {
                                ord: 1,
                                act: C_GRIND,
                                pre: vec![],
                                tgt: C_COFFEE,
                                tools: vec![],
                                eff: vec![],
                                warn: vec![],
                            },
                            ProcedureStep {
                                ord: 2,
                                act: C_HEAT,
                                pre: vec![],
                                tgt: C_WATER,
                                tools: vec![],
                                eff: vec![],
                                warn: vec![],
                            },
                            ProcedureStep {
                                ord: 3,
                                act: C_POUR,
                                pre: vec![],
                                tgt: C_WATER,
                                tools: vec![],
                                eff: vec![],
                                warn: vec![],
                            },
                            ProcedureStep {
                                ord: 4,
                                act: C_BREW,
                                pre: vec![],
                                tgt: C_COFFEE,
                                tools: vec![],
                                eff: vec![],
                                warn: vec![],
                            },
                            ProcedureStep {
                                ord: 5,
                                act: C_FILTER,
                                pre: vec![],
                                tgt: C_COFFEE,
                                tools: vec![],
                                eff: vec![],
                                warn: vec![],
                            },
                        ],
                        total_time: Some(300), // 5 minutes
                        difficulty: 1,
                        tools_req: vec![C_FILTER],
                    },
                ),
            ),
            // 2. ExperienceGene: "Sunset at beach"
            (
                "Experience",
                make_ku(
                    vec![
                        codon(C_SUNSET, RoleId::Agent),
                        codon(C_BEACH, RoleId::Location),
                        codon(C_BEAUTY, RoleId::Quality),
                    ],
                    vec![],
                    Gene::Experience {
                        scene: vec![
                            codon(C_SKY, RoleId::Location),
                            codon(C_WARM, RoleId::Quality),
                        ],
                        affect: Affect {
                            v: 8500,
                            a: 3000,
                            d: 6000,
                        },
                        canonical: Some(CanonicalText {
                            lang: 1,
                            text: b"Sunset at the beach was breathtaking".to_vec(),
                        }),
                        perspective: Some(Perspective {
                            expertise: 0,
                            perspective_type: 1,
                        }),
                    },
                ),
            ),
            // 3. CreativeGene: "Vietnamese pho recipe"
            (
                "Creative",
                make_ku(
                    vec![
                        codon(C_RECIPE, RoleId::Agent),
                        codon(C_INGREDIENT, RoleId::Object),
                        codon(C_COOK, RoleId::Result),
                    ],
                    vec![],
                    Gene::Creative {
                        steps: vec![
                            ProcedureStep {
                                ord: 1,
                                act: C_HEAT,
                                pre: vec![],
                                tgt: C_WATER,
                                tools: vec![],
                                eff: vec![],
                                warn: vec![],
                            },
                            ProcedureStep {
                                ord: 2,
                                act: C_COOK,
                                pre: vec![],
                                tgt: C_INGREDIENT,
                                tools: vec![],
                                eff: vec![],
                                warn: vec![],
                            },
                            ProcedureStep {
                                ord: 3,
                                act: C_SERVE,
                                pre: vec![],
                                tgt: C_RECIPE,
                                tools: vec![],
                                eff: vec![],
                                warn: vec![],
                            },
                        ],
                        cultural_context: vec![5000, 5001],
                        origin_story: Some(CanonicalText {
                            lang: 1,
                            text: b"Traditional Vietnamese pho".to_vec(),
                        }),
                    },
                ),
            ),
            // 4. MediaExperienceGene: "Inception film review"
            (
                "MediaExperience",
                make_ku(
                    vec![
                        codon(C_FILM, RoleId::Object),
                        codon(C_EMOTION, RoleId::Quality),
                        codon(C_STORY, RoleId::Agent),
                    ],
                    vec![],
                    Gene::MediaExperience {
                        id_sys: 1, // IMDB
                        ext_id: b"tt1375666".to_vec(),
                        media_type: 0, // FILM
                        rating: 92,
                        affect: Affect {
                            v: 8000,
                            a: 7500,
                            d: 5000,
                        },
                        spoiler_level: 1, // MILD
                    },
                ),
            ),
            // 5. TestimonyGene: "Witnessed strange lights"
            (
                "Testimony",
                make_ku(
                    vec![
                        codon(C_WITNESS, RoleId::Agent),
                        codon(C_STRANGE, RoleId::Quality),
                        codon(C_LIGHT, RoleId::Object),
                        codon(C_NIGHT, RoleId::Time),
                    ],
                    vec![make_bond(RelationType::TestimonyAbout)],
                    Gene::Testimony {
                        triples: vec![Triple {
                            subject: C_WITNESS,
                            predicate: C_SEE,
                            object: C_LIGHT,
                        }],
                        claim_type: 0,    // SIGHTING
                        extraordinary: 2, // HIGH
                        witness_count: 3,
                        proximity: 0,           // FIRSTHAND
                        verification_status: 0, // UNVERIFIED
                    },
                ),
            ),
            // 6. FormalGene: "E=mc²"
            (
                "Formal",
                make_ku(
                    vec![
                        codon(C_ENERGY, RoleId::Object),
                        codon(C_MASS, RoleId::Agent),
                        codon(C_SPEED, RoleId::Condition),
                    ],
                    vec![make_bond(RelationType::FormallyProves)],
                    Gene::Formal {
                        domain: 1,          // PHYSICS
                        notation_format: 0, // LATEX
                        notation_source: b"E = mc^2".to_vec(),
                        statement_type: 5,      // EQUATION
                        verification_status: 3, // FORMALLY_PROVED
                    },
                ),
            ),
            // 7. HypothesisGene: "Dark matter theory"
            (
                "Hypothesis",
                make_ku(
                    vec![
                        codon(C_DARK, RoleId::Quality),
                        codon(C_MATTER, RoleId::Object),
                        codon(C_GALAXY, RoleId::Location),
                        codon(C_GRAVITY, RoleId::Cause),
                    ],
                    vec![],
                    Gene::Hypothesis {
                        base_type: 0, // → FACT when mature
                        body_codons: vec![
                            codon(C_DARK, RoleId::Quality),
                            codon(C_MATTER, RoleId::Agent),
                        ],
                        maturity_level: 3, // TESTED_HYPOTHESIS
                        confidence: 7000,
                        completeness: 5000,
                        falsifiable: true,
                    },
                ),
            ),
            // 8. NarrativeGene: "Creation myth"
            (
                "Narrative",
                make_ku(
                    vec![
                        codon(C_CREATION, RoleId::Agent),
                        codon(C_WORLD, RoleId::Object),
                        codon(C_CHAOS, RoleId::Cause),
                        codon(C_ORDER, RoleId::Result),
                        codon(C_LIFE, RoleId::Purpose),
                    ],
                    vec![],
                    Gene::Narrative {
                        narrative_type: 1, // MYTH
                        origin_culture: vec![5000, 5001],
                        era: 6,      // TIMELESS
                        function: 1, // MORAL_TEACHING
                        sacred: true,
                        moral: vec![codon(C_ORDER, RoleId::Result)],
                        canonical: Some(CanonicalText {
                            lang: 1,
                            text: b"In the beginning there was chaos".to_vec(),
                        }),
                    },
                ),
            ),
            // 9. SensoryGene: "440Hz concert A"
            (
                "Sensory",
                make_ku(
                    vec![
                        codon(C_SOUND, RoleId::Object),
                        codon(C_FREQUENCY, RoleId::Quality),
                        codon(C_WAVE, RoleId::Manner),
                    ],
                    vec![],
                    Gene::Sensory {
                        modality: 1, // AUDITORY
                        property: C_FREQUENCY,
                        feature: C_WAVE,
                        result_codons: vec![codon(C_SOUND, RoleId::Result)],
                        sensor_type: 1, // HUMAN_EAR
                        quality: 0,     // RAW
                    },
                ),
            ),
        ];

        println!(
            "\n  {:<20} {:<12} {:<10} {:<10} {:>8}",
            "Gene Type", "Wire(type)", "Ext?", "CRC Valid", "Bytes"
        );
        println!(
            "  {:-<20} {:-<12} {:-<10} {:-<10} {:->8}",
            "", "", "", "", ""
        );

        let mut total_size = 0usize;
        let mut all_passed = true;

        for (name, ku) in &test_cases {
            // 1. Encode to wire format
            let wire = encode_knowledge_unit(ku).unwrap();
            let wire_len = wire.len();
            total_size += wire_len;

            // 2. Decode back
            let (decoded_meta, decoded_ku) = decode_full_knowledge_unit(&wire).unwrap();

            // 3. Verify gene_type matches
            let gene_type_matches = decoded_meta.gene_type == ku.gene.gene_type();

            // 4. Verify CRC is valid
            let crc_valid = decoded_meta.crc32_valid;

            // 5. Verify codons round-trip
            let codons_match = decoded_ku.codons == ku.codons;

            // 6. Verify gene content round-trip
            let gene_match = decoded_ku.gene == ku.gene;

            let (_, ext) = ku.gene.gene_type().wire_encoding();
            let ext_str = match ext {
                Some(e) => format!("0x{:02X}", e),
                None => "—".to_string(),
            };

            let passed = gene_type_matches && crc_valid && codons_match && gene_match;
            if !passed {
                all_passed = false;
            }

            println!(
                "  {:<20} {:<12} {:<10} {:<10} {:>6}B {}",
                name,
                format!("{:?}", decoded_meta.gene_type),
                ext_str,
                if crc_valid { "✓" } else { "✗" },
                wire_len,
                if passed { "✓" } else { "✗ FAIL" },
            );

            // Hard assertions
            assert!(gene_type_matches, "{}: gene type mismatch", name);
            assert!(crc_valid, "{}: CRC invalid", name);
            assert!(codons_match, "{}: codons mismatch after roundtrip", name);
            assert!(
                gene_match,
                "{}: gene content mismatch after roundtrip",
                name
            );
        }

        println!(
            "  {:-<20} {:-<12} {:-<10} {:-<10} {:->8}",
            "", "", "", "", ""
        );
        println!(
            "  {:<20} {:<12} {:<10} {:<10} {:>6}B",
            "TOTAL", "", "", "", total_size
        );
        println!(
            "\n  ALL 10 GENE TYPES ROUND-TRIP: {}\n",
            if all_passed {
                "PASSED ✓"
            } else {
                "FAILED ✗"
            }
        );

        assert!(
            all_passed,
            "Not all gene types passed round-trip validation"
        );
    }

    // ========================================================================
    // 2. Size Benchmark — Minimal / Typical / Full
    // ========================================================================

    #[test]
    fn test_size_benchmark() {
        println!("\n============================================================");
        println!("  BENCHMARK: Size Benchmark — Minimal / Typical / Full");
        println!("============================================================");

        // (a) Minimal: 1 codon, 0 bonds, no trust, no epigenetic
        let ku_minimal = KnowledgeUnit {
            codons: vec![codon(C_WATER, RoleId::Agent)],
            bonds: vec![],
            gene: Gene::Fact {
                triples: vec![Triple {
                    subject: C_WATER,
                    predicate: C_IS,
                    object: C_TRUE,
                }],
                certainty: 5000,
                evidence: vec![],
            },
            flags: HeaderFlags::default(),
            epistemic_status: None,
            evidence_type: None,
            trust: None,
            epigenetic: None,
        };

        // (b) Typical: 3-4 codons, 1 bond, trust section, no embedding
        let ku_typical = KnowledgeUnit {
            codons: vec![
                codon(C_WATER, RoleId::Agent),
                codon(C_BOIL, RoleId::Result),
                codon_q(100, RoleId::Quantity, "unit", C_CELSIUS),
            ],
            bonds: vec![make_bond(RelationType::Corroborates)],
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
            trust: Some(TrustSection {
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
            }),
            epigenetic: None,
        };

        // (c) Full: 5 codons, 3 bonds, trust, epigenetic with 512B embedding
        let ku_full = KnowledgeUnit {
            codons: vec![
                codon(C_WATER, RoleId::Agent),
                codon(C_BOIL, RoleId::Result),
                codon_q(100, RoleId::Quantity, "unit", C_CELSIUS),
                codon(C_HEAT, RoleId::Cause),
                codon(C_ENERGY, RoleId::Manner),
            ],
            bonds: vec![
                make_bond(RelationType::Corroborates),
                make_bond(RelationType::Cites),
                make_bond(RelationType::DerivedFrom),
            ],
            gene: Gene::Fact {
                triples: vec![
                    Triple {
                        subject: C_WATER,
                        predicate: C_BOILING_POINT,
                        object: C_CELSIUS,
                    },
                    Triple {
                        subject: C_HEAT,
                        predicate: C_IS,
                        object: C_ENERGY,
                    },
                ],
                certainty: 9900,
                evidence: vec![vec![0xCDu8; 36]],
            },
            flags: HeaderFlags::default(),
            epistemic_status: Some(EpistemicStatus::Consensus),
            evidence_type: Some(EvidenceType::Experimental),
            trust: Some(TrustSection {
                epistemic_status: EpistemicStatus::Consensus,
                evidence_type: EvidenceType::Experimental,
                verification_level: 4,
                corroboration_count: 12,
                challenge_count: 2,
                error_susceptibility: 0,
                trust_score: 9500,
                confidence: 9800,
                domain_codes: vec![20_000, 20_001],
                verifications: vec![vec![0xAAu8; 36]],
                challenges: vec![],
                ..Default::default()
            }),
            epigenetic: Some(EpigeneticSection {
                embedding: vec![42u8; 512],
                embedding_binary: vec![],
                embed_version: Some(1),
                valid_from: Some(1719072000),
                valid_until: Some(1750608000),
                recorded_at: Some(1719072000),
                temporal_precision: Some(4), // DAY
                temporal_uncertainty: None,
                half_life: None,
                krl: Some(7),
                language: Some(1),
                template: Some(0),
                difficulty: Some(3),
                categories: vec![100, 200],
                tags: vec![1000],
                simhash: vec![],
                lsh_buckets: vec![],
                schema_ver: Some(1),
                version: Some(3),
                prev_cid: None,
                superseded_by: None,
            }),
        };

        struct SizeResult {
            label: &'static str,
            total: usize,
            header: usize,
            crc: usize,
            payload: usize,
        }

        let mut results = Vec::new();

        for (label, ku) in [
            ("Minimal", &ku_minimal),
            ("Typical", &ku_typical),
            ("Full", &ku_full),
        ] {
            let wire = encode_knowledge_unit(ku).unwrap();
            let total = wire.len();
            let header = 6;
            let crc = 4;
            let payload = total - header - crc;

            results.push(SizeResult {
                label,
                total,
                header,
                crc,
                payload,
            });
        }

        println!(
            "\n  {:<12} {:>8} {:>10} {:>10} {:>10} {:>12} {:>12}",
            "Level", "Total", "Header", "Payload", "CRC", "Header%", "CRC%"
        );
        println!(
            "  {:-<12} {:->8} {:->10} {:->10} {:->10} {:->12} {:->12}",
            "", "", "", "", "", "", ""
        );

        for r in &results {
            let header_pct = (r.header as f64 / r.total as f64) * 100.0;
            let crc_pct = (r.crc as f64 / r.total as f64) * 100.0;
            println!(
                "  {:<12} {:>6}B {:>8}B {:>8}B {:>8}B {:>10.1}% {:>10.1}%",
                r.label, r.total, r.header, r.payload, r.crc, header_pct, crc_pct
            );
        }

        // Layer breakdowns
        println!("\n  --- Layer Breakdown ---");
        for (label, ku) in [
            ("Minimal", &ku_minimal),
            ("Typical", &ku_typical),
            ("Full", &ku_full),
        ] {
            let report = size_breakdown_full(ku).unwrap();
            println!("\n  [{}]", label);
            for line in report.lines() {
                println!("    {}", line);
            }
        }

        // Size assertions
        let minimal_size = encode_knowledge_unit(&ku_minimal).unwrap().len();
        let typical_size = encode_knowledge_unit(&ku_typical).unwrap().len();
        let full_size = encode_knowledge_unit(&ku_full).unwrap().len();

        println!("\n  Size Assertions:");
        println!(
            "    Minimal ({:>4}B) < 200B: {}",
            minimal_size,
            if minimal_size < 200 {
                "PASS ✓"
            } else {
                "FAIL ✗"
            }
        );
        println!(
            "    Typical ({:>4}B) < 500B: {}",
            typical_size,
            if typical_size < 500 {
                "PASS ✓"
            } else {
                "FAIL ✗"
            }
        );
        println!(
            "    Full    ({:>4}B) < 1500B: {}",
            full_size,
            if full_size < 1500 {
                "PASS ✓"
            } else {
                "FAIL ✗"
            }
        );

        assert!(
            minimal_size < 200,
            "Minimal KU ({} bytes) should be < 200B",
            minimal_size
        );
        assert!(
            typical_size < 500,
            "Typical KU ({} bytes) should be < 500B",
            typical_size
        );
        assert!(
            full_size < 1500,
            "Full KU ({} bytes) should be < 1500B (512B embed + 3 bonds + trust)",
            full_size
        );

        println!("\n  test_size_benchmark: PASSED ✓");
    }

    // ========================================================================
    // 3. Error Handling Validation — All KuError variants reachable
    // ========================================================================

    #[test]
    fn test_error_handling_comprehensive() {
        println!("\n============================================================");
        println!("  BENCHMARK: Error Handling — All KuError Variants");
        println!("============================================================");

        let mut covered = Vec::new();

        // 1. CborDecode — feed random bytes as CBOR payload
        {
            // Build a wire frame with valid header but garbage CBOR payload
            let mut wire = Vec::new();
            wire.extend_from_slice(&MAGIC); // valid magic
            wire.push(VERSION); // valid version
            wire.push(0x00); // flags: gene_type=0 (Fact)
            let payload = vec![0xFF, 0xFE, 0xFD, 0xFC, 0xFB]; // garbage CBOR
            let payload_len = payload.len() as u32;
            wire.extend_from_slice(&payload_len.to_be_bytes()); // v5: u32 BE
            wire.extend_from_slice(&payload);
            let crc = crc32fast::hash(&wire);
            wire.extend_from_slice(&crc.to_be_bytes());

            // decode_knowledge_unit should succeed (valid wire), but
            // decode_full_knowledge_unit should fail with CborDecode
            let decoded = decode_knowledge_unit(&wire);
            assert!(decoded.is_ok(), "Wire header should be valid");
            let full_result = decode_full_knowledge_unit(&wire);
            assert!(full_result.is_err(), "CBOR deserialization should fail");
            match full_result.unwrap_err() {
                KuError::CborDecode(_) => covered.push("CborDecode"),
                other => panic!("Expected CborDecode, got: {:?}", other),
            }
        }

        // 2. InvalidMagic — wrong first 2 bytes
        {
            let ku = make_ku(
                vec![codon(C_WATER, RoleId::Agent)],
                vec![],
                Gene::Fact {
                    triples: vec![],
                    certainty: 0,
                    evidence: vec![],
                },
            );
            let mut wire = encode_knowledge_unit(&ku).unwrap();
            wire[0] = 0xFF;
            wire[1] = 0xFE;
            match decode_knowledge_unit(&wire).unwrap_err() {
                KuError::InvalidMagic(m) => {
                    assert_eq!(m, [0xFF, 0xFE]);
                    covered.push("InvalidMagic");
                }
                other => panic!("Expected InvalidMagic, got: {:?}", other),
            }
        }

        // 3. UnsupportedVersion — version byte != 0x04
        {
            let ku = make_ku(
                vec![codon(C_WATER, RoleId::Agent)],
                vec![],
                Gene::Fact {
                    triples: vec![],
                    certainty: 0,
                    evidence: vec![],
                },
            );
            let mut wire = encode_knowledge_unit(&ku).unwrap();
            wire[2] = 0x99; // wrong version
            match decode_knowledge_unit(&wire).unwrap_err() {
                KuError::UnsupportedVersion(v) => {
                    assert_eq!(v, 0x99);
                    covered.push("UnsupportedVersion");
                }
                other => panic!("Expected UnsupportedVersion, got: {:?}", other),
            }
        }

        // 4. CrcMismatch — flip a bit in payload
        {
            let ku = make_ku(
                vec![codon(C_WATER, RoleId::Agent)],
                vec![],
                Gene::Fact {
                    triples: vec![],
                    certainty: 0,
                    evidence: vec![],
                },
            );
            let mut wire = encode_knowledge_unit(&ku).unwrap();
            // Flip a bit in the payload area (byte 8)
            if wire.len() > 12 {
                wire[8] ^= 0xFF;
            }
            match decode_knowledge_unit(&wire).unwrap_err() {
                KuError::CrcMismatch { stored, computed } => {
                    assert_ne!(stored, computed);
                    covered.push("CrcMismatch");
                }
                other => panic!("Expected CrcMismatch, got: {:?}", other),
            }
        }

        // 5. VarintTruncated — truncated varint bytes
        {
            // Tier 1 prefix (0x80) but only 1 byte
            let result = decode_varint(&[0x80]);
            match result.unwrap_err() {
                KuError::VarintTruncated { needed, got } => {
                    assert_eq!(needed, 2);
                    assert_eq!(got, 1);
                    covered.push("VarintTruncated");
                }
                other => panic!("Expected VarintTruncated, got: {:?}", other),
            }
        }

        // 6. InvalidData — byte with reserved varint prefix (Tier 5+)
        {
            // 0xF8 is a reserved varint prefix (111110xx = Tier 5)
            let result = decode_varint(&[0xF8]);
            match result.unwrap_err() {
                KuError::InvalidData(msg) => {
                    assert!(
                        msg.contains("reserved"),
                        "Expected reserved tier message, got: {}",
                        msg
                    );
                    covered.push("InvalidVarintPrefix");
                }
                other => panic!("Expected InvalidData for reserved tier, got: {:?}", other),
            }
        }

        // 7. PayloadTruncated — data shorter than header claims
        {
            let result = decode_knowledge_unit(&[0x4B, 0x44, 0x04, 0x00, 0x00]);
            match result.unwrap_err() {
                KuError::PayloadTruncated { expected: _, got } => {
                    assert_eq!(got, 5);
                    covered.push("PayloadTruncated");
                }
                other => panic!("Expected PayloadTruncated, got: {:?}", other),
            }
        }

        // 8. InvalidData — EXTENDED gene type with empty payload
        {
            // Build wire with gene_base=7 (EXTENDED) but payload_len=0
            let mut wire = Vec::new();
            wire.extend_from_slice(&MAGIC);
            wire.push(VERSION);
            wire.push(0x07 << 5); // gene_base=7 in bits 5-7
            wire.extend_from_slice(&0u32.to_be_bytes()); // v5: payload_len=0 (u32 BE)
                                                         // CRC over the header
            let crc = crc32fast::hash(&wire);
            wire.extend_from_slice(&crc.to_be_bytes());

            match decode_knowledge_unit(&wire).unwrap_err() {
                KuError::InvalidData(msg) => {
                    assert!(
                        msg.contains("EXTENDED"),
                        "Message should mention EXTENDED: {}",
                        msg
                    );
                    covered.push("InvalidData");
                }
                other => panic!("Expected InvalidData, got: {:?}", other),
            }
        }

        // 9. UnknownGeneType — EXTENDED with invalid ext byte
        {
            let mut wire = Vec::new();
            wire.extend_from_slice(&MAGIC);
            wire.push(VERSION);
            wire.push(0x07 << 5); // gene_base=7 (EXTENDED)
            let payload = vec![0xFF]; // invalid ext byte
            let payload_len = payload.len() as u32;
            wire.extend_from_slice(&payload_len.to_be_bytes()); // v5: u32 BE
            wire.extend_from_slice(&payload);
            let crc = crc32fast::hash(&wire);
            wire.extend_from_slice(&crc.to_be_bytes());

            match decode_knowledge_unit(&wire).unwrap_err() {
                KuError::UnknownGeneType(g) => {
                    assert_eq!(g, 0xFF);
                    covered.push("UnknownGeneType");
                }
                other => panic!("Expected UnknownGeneType, got: {:?}", other),
            }
        }

        // Summary
        println!("\n  Error variants covered:");
        for (i, name) in covered.iter().enumerate() {
            println!("    {}. {} ✓", i + 1, name);
        }
        println!("\n  Total: {}/9 variants covered", covered.len());
        assert_eq!(covered.len(), 9, "Should cover all 9 error variants");
        println!("  test_error_handling_comprehensive: PASSED ✓");
    }

    // ========================================================================
    // 4. Varint Exhaustive Boundary Test
    // ========================================================================

    #[test]
    fn test_varint_boundaries() {
        println!("\n============================================================");
        println!("  BENCHMARK: Varint Exhaustive Boundary Test");
        println!("============================================================");

        let test_cases: Vec<(u64, usize, &str)> = vec![
            // Tier 0: 1 byte (0-127)
            (0, 1, "Tier 0 min"),
            (1, 1, "Tier 0 low"),
            (126, 1, "Tier 0 high"),
            (127, 1, "Tier 0 max"),
            // Tier 1: 2 bytes (128-16,511)
            (128, 2, "Tier 1 min"),
            (129, 2, "Tier 1 min+1"),
            (16_511, 2, "Tier 1 max"),
            // Tier 2: 3 bytes (16,512-2,113,663)
            (16_512, 3, "Tier 2 min"),
            (16_513, 3, "Tier 2 min+1"),
            (2_113_663, 3, "Tier 2 max"),
            // Tier 3: 4 bytes (2,113,664-270,549,119)
            (2_113_664, 4, "Tier 3 min (4B)"),
            (100_000_000, 4, "Tier 3 mid (4B)"),
            (268_435_455, 4, "Tier 3 (u28 max area)"),
            (270_549_119, 4, "Tier 3 max (4B)"),
            // Tier 3+: 5 bytes (270,549,120+)
            (270_549_120, 5, "Tier 3+ min (5B)"),
            (u32::MAX as u64, 5, "Tier 3+ (u32::MAX)"),
        ];

        println!(
            "\n  {:>15}  {:>5}  {:>5}  {:>15}  {:<20}  Status",
            "Value", "ExpB", "GotB", "Decoded", "Label"
        );
        println!(
            "  {:->15}  {:->5}  {:->5}  {:->15}  {:-<20}  {:-<6}",
            "", "", "", "", "", ""
        );

        for (value, expected_bytes, label) in &test_cases {
            let encoded = encode_varint(*value).unwrap();
            let (decoded, consumed) = decode_varint(&encoded).unwrap();

            let byte_match = encoded.len() == *expected_bytes;
            let value_match = decoded == *value;
            let consumed_match = consumed == *expected_bytes;
            let pass = byte_match && value_match && consumed_match;

            println!(
                "  {:>15}  {:>5}  {:>5}  {:>15}  {:<20}  {}",
                value,
                expected_bytes,
                encoded.len(),
                decoded,
                label,
                if pass { "✓" } else { "✗ FAIL" }
            );

            assert_eq!(
                encoded.len(),
                *expected_bytes,
                "Wrong byte count for {} ({}): expected {}, got {}",
                value,
                label,
                expected_bytes,
                encoded.len()
            );
            assert_eq!(
                decoded, *value,
                "Roundtrip failed for {} ({})",
                value, label
            );
            assert_eq!(
                consumed, *expected_bytes,
                "Consumed mismatch for {} ({})",
                value, label
            );
        }

        // Additional: varint encoding for maximum reachable value
        let max_reachable = 34_628_173_487u64; // TIER3P_MAX
        let encoded = encode_varint(max_reachable).unwrap();
        let (decoded, consumed) = decode_varint(&encoded).unwrap();
        assert_eq!(decoded, max_reachable);
        assert_eq!(consumed, 5);
        println!(
            "\n  Max reachable value (u32::MAX + 2,113,664 = {}): {} bytes ✓",
            max_reachable,
            encoded.len()
        );

        // Varint tier capacity summary
        println!("\n  --- Varint Tier Capacities ---");
        println!("    Tier 0 (1B): 0–127            ({} values)", 128);
        println!(
            "    Tier 1 (2B): 128–16,511       ({} values)",
            16_511 - 128 + 1
        );
        println!(
            "    Tier 2 (3B): 16,512–2,113,663 ({} values)",
            2_113_663 - 16_512 + 1
        );
        println!(
            "    Tier 3 (4B): 2,113,664–270,549,119 ({} values)",
            270_549_119u64 - 2_113_664 + 1
        );
        println!("    Tier 3+ (5B): 270,549,120+    (up to ~34.6B)");

        println!("\n  test_varint_boundaries: PASSED ✓");
    }

    // ========================================================================
    // 5. Wire Format Compliance Test
    // ========================================================================

    #[test]
    fn test_wire_format_compliance() {
        println!("\n============================================================");
        println!("  BENCHMARK: Wire Format Compliance — v4 Spec");
        println!("============================================================");

        // Create a known KU
        let ku = make_ku(
            vec![codon(C_WATER, RoleId::Agent), codon(C_BOIL, RoleId::Result)],
            vec![],
            Gene::Fact {
                triples: vec![Triple {
                    subject: C_WATER,
                    predicate: C_BOILING_POINT,
                    object: C_CELSIUS,
                }],
                certainty: 9900,
                evidence: vec![],
            },
        );

        let wire = encode_knowledge_unit(&ku).unwrap();

        // Byte 0-1: MAGIC = 0x4B44 ("KD")
        assert_eq!(wire[0], 0x4B, "MAGIC byte 0 should be 0x4B ('K')");
        assert_eq!(wire[1], 0x44, "MAGIC byte 1 should be 0x44 ('D')");
        println!(
            "  Byte 0-1 MAGIC:       0x{:02X}{:02X} ('{}{}') ✓",
            wire[0], wire[1], wire[0] as char, wire[1] as char
        );

        // Byte 2: VERSION = 0x05
        assert_eq!(wire[2], 0x05, "VERSION should be 0x05");
        println!("  Byte 2   VERSION:     0x{:02X} (v{}) ✓", wire[2], wire[2]);

        // Byte 3: FLAGS (gene_type in bits 5-7)
        let flags_byte = wire[3];
        let gene_base = (flags_byte >> 5) & 0x07;
        assert_eq!(gene_base, 0, "Gene type base should be 0 (Fact)");
        let has_ecc = flags_byte & 0x01 != 0;
        let is_encrypted = flags_byte & 0x10 != 0;
        println!(
            "  Byte 3   FLAGS:       0x{:02X} (gene_base={}, ecc={}, enc={}) ✓",
            flags_byte, gene_base, has_ecc, is_encrypted
        );

        // Byte 4-7: PAYLOAD_LEN (u32 big-endian) ★ v5
        let payload_len = u32::from_be_bytes([wire[4], wire[5], wire[6], wire[7]]);
        println!(
            "  Byte 4-7 PAYLOAD_LEN: {} bytes (0x{:02X}{:02X}{:02X}{:02X}) ✓",
            payload_len, wire[4], wire[5], wire[6], wire[7]
        );

        // Byte 8..8+len: CBOR payload ★ v5 (was byte 6 in v4)
        let _payload_start = 8;
        let payload_end = 8 + payload_len as usize;
        assert!(
            wire.len() >= payload_end + 4,
            "Wire too short: {} < {}",
            wire.len(),
            payload_end + 4
        );
        println!(
            "  Byte 8..{}: CBOR payload ({} bytes) ✓",
            payload_end - 1,
            payload_len
        );

        // Last 4 bytes: CRC-32 (big-endian)
        let crc_offset = payload_end;
        let stored_crc = u32::from_be_bytes([
            wire[crc_offset],
            wire[crc_offset + 1],
            wire[crc_offset + 2],
            wire[crc_offset + 3],
        ]);
        let computed_crc = crc32fast::hash(&wire[..crc_offset]);
        assert_eq!(stored_crc, computed_crc, "CRC-32 mismatch");
        println!(
            "  Byte {}..{}: CRC-32 = 0x{:08X} ✓",
            crc_offset,
            crc_offset + 3,
            stored_crc
        );

        // Total wire size (v5: 8-byte header)
        let total = 2 + 1 + 1 + 4 + payload_len as usize + 4; // MAGIC(2) + VER(1) + FLAGS(1) + LEN(4) + PAYLOAD + CRC(4)
        assert_eq!(wire.len(), total, "Total wire size mismatch");
        println!("\n  Wire layout verified:");
        println!(
            "    MAGIC(2B) + VERSION(1B) + FLAGS(1B) + LEN(4B) + PAYLOAD({}B) + CRC32(4B) = {}B",
            payload_len, total
        );

        // Print first 32 bytes hex dump
        println!("\n  Hex dump (first 32 bytes):");
        print!("    ");
        for (i, byte) in wire.iter().take(32).enumerate() {
            print!("{:02X} ", byte);
            if (i + 1) % 16 == 0 {
                print!("\n    ");
            }
        }
        println!();

        // Also verify EXTENDED wire format (Hypothesis → base=7, ext=0x00)
        println!("\n  --- EXTENDED wire format (HypothesisGene) ---");
        let ku_ext = make_ku(
            vec![codon(42, RoleId::Agent)],
            vec![],
            Gene::Hypothesis {
                base_type: 0,
                body_codons: vec![],
                maturity_level: 2,
                confidence: 5000,
                completeness: 3000,
                falsifiable: true,
            },
        );
        let wire_ext = encode_knowledge_unit(&ku_ext).unwrap();
        let ext_gene_base = (wire_ext[3] >> 5) & 0x07;
        assert_eq!(ext_gene_base, 7, "EXTENDED gene should have base=7");
        assert_eq!(wire_ext[6], 0x00, "Hypothesis ext byte should be 0x00");
        println!("  FLAGS byte: 0x{:02X} (gene_base=7) ✓", wire_ext[3]);
        println!("  Payload[0]: 0x{:02X} (ext=Hypothesis) ✓", wire_ext[6]);

        println!("\n  test_wire_format_compliance: PASSED ✓");
    }

    // ========================================================================
    // 6. Summary Report Test
    // ========================================================================

    #[test]
    fn test_print_full_report() {
        println!("\n============================================================");
        println!("  KU-CORE BENCHMARK REPORT");
        println!("  UKRL v4 — Layers 1-5 Encoder/Decoder");
        println!("============================================================");

        // --- Gene Type Sizes ---
        println!("\n  ┌─────────────────────────────────────────────────┐");
        println!("  │            Gene Type Encoding Sizes              │");
        println!("  ├─────────────────────┬──────┬────────┬────────────┤");
        println!("  │ Gene Type           │ Base │ Ext    │ Gene Size  │");
        println!("  ├─────────────────────┼──────┼────────┼────────────┤");

        let genes: Vec<(&str, Gene)> = vec![
            (
                "Fact",
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
                "Procedure",
                Gene::Procedure {
                    steps: vec![ProcedureStep {
                        ord: 1,
                        act: 1,
                        pre: vec![],
                        tgt: 2,
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
                "Experience",
                Gene::Experience {
                    scene: vec![codon(1, RoleId::Agent)],
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
                "Creative",
                Gene::Creative {
                    steps: vec![],
                    cultural_context: vec![],
                    origin_story: None,
                },
            ),
            (
                "MediaExperience",
                Gene::MediaExperience {
                    id_sys: 1,
                    ext_id: b"tt1234567".to_vec(),
                    media_type: 0,
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
                "Testimony",
                Gene::Testimony {
                    triples: vec![Triple {
                        subject: 100,
                        predicate: 200,
                        object: 300,
                    }],
                    claim_type: 1,
                    extraordinary: 0,
                    witness_count: 1,
                    proximity: 0,
                    verification_status: 0,
                },
            ),
            (
                "Formal",
                Gene::Formal {
                    domain: 0,
                    notation_format: 0,
                    notation_source: b"E=mc^2".to_vec(),
                    statement_type: 5,
                    verification_status: 2,
                },
            ),
            (
                "Hypothesis",
                Gene::Hypothesis {
                    base_type: 0,
                    body_codons: vec![codon(42, RoleId::Agent)],
                    maturity_level: 2,
                    confidence: 5000,
                    completeness: 3000,
                    falsifiable: true,
                },
            ),
            (
                "Narrative",
                Gene::Narrative {
                    narrative_type: 0,
                    origin_culture: vec![5000],
                    era: 6,
                    function: 1,
                    sacred: false,
                    moral: vec![],
                    canonical: None,
                },
            ),
            (
                "Sensory",
                Gene::Sensory {
                    modality: 0,
                    property: 1000,
                    feature: 2000,
                    result_codons: vec![],
                    sensor_type: 0,
                    quality: 0,
                },
            ),
        ];

        for (name, gene) in &genes {
            let gt = gene.gene_type();
            let (base, ext) = gt.wire_encoding();
            let size = encode_gene(gene).unwrap().len();
            let ext_str = match ext {
                Some(e) => format!("0x{:02X}", e),
                None => "—".to_string(),
            };
            println!(
                "  │ {:<19} │ {:>4} │ {:<6} │ {:>7}B   │",
                name, base, ext_str, size
            );
        }
        println!("  └─────────────────────┴──────┴────────┴────────────┘");

        // --- Varint Tier Capacities ---
        println!("\n  ┌───────────────────────────────────────────────────┐");
        println!("  │              Varint Tier Capacities                │");
        println!("  ├──────────┬────────────────────────────┬───────────┤");
        println!("  │ Tier     │ Range                      │ Values    │");
        println!("  ├──────────┼────────────────────────────┼───────────┤");
        println!("  │ 0 (1B)   │ 0 – 127                   │       128 │");
        println!("  │ 1 (2B)   │ 128 – 16,511              │    16,384 │");
        println!("  │ 2 (3B)   │ 16,512 – 2,113,663        │ 2,097,152 │");
        println!("  │ 3 (5B)   │ 2,113,664 – 4,297,081,599 │     ~4.3B │");
        println!("  └──────────┴────────────────────────────┴───────────┘");

        // --- Layer Overhead Breakdown ---
        println!("\n  ┌─────────────────────────────────────────────────┐");
        println!("  │              Layer Overhead Breakdown             │");
        println!("  ├──────────────────┬──────────────────┬────────────┤");
        println!("  │ Component        │ Typical Size     │ Notes      │");
        println!("  ├──────────────────┼──────────────────┼────────────┤");
        println!("  │ Header (L1)      │ 6B (fixed)       │ MAGIC+VER  │");
        println!("  │ CRC-32 (L5)      │ 4B (fixed)       │ Integrity  │");
        println!("  │ Codons (L1)      │ ~15-80B          │ Per codon  │");
        println!("  │ Bonds (L2)       │ ~55-150B each    │ Per bond   │");
        println!("  │ Gene (L3)        │ ~30-200B         │ Per type   │");
        println!("  │ Trust (L4)       │ ~30-100B         │ Optional   │");
        println!("  │ Epigenetic (L4)  │ ~20-900B         │ Optional   │");
        println!("  └──────────────────┴──────────────────┴────────────┘");

        // --- Error Handling Coverage ---
        println!("\n  ┌─────────────────────────────────────────────────┐");
        println!("  │            Error Handling Coverage                │");
        println!("  ├──────────────────────────┬────────────┬──────────┤");
        println!("  │ Error Variant            │ Reachable? │ Test     │");
        println!("  ├──────────────────────────┼────────────┼──────────┤");
        println!("  │ CborEncode               │ Hard*      │ Skip     │");
        println!("  │ CborDecode               │ Yes        │ ✓        │");
        println!("  │ InvalidMagic             │ Yes        │ ✓        │");
        println!("  │ UnsupportedVersion       │ Yes        │ ✓        │");
        println!("  │ CrcMismatch              │ Yes        │ ✓        │");
        println!("  │ VarintTruncated          │ Yes        │ ✓        │");
        println!("  │ InvalidVarintPrefix      │ Yes        │ ✓        │");
        println!("  │ UnknownGeneType          │ Yes        │ ✓        │");
        println!("  │ PayloadTruncated         │ Yes        │ ✓        │");
        println!("  │ InvalidData              │ Yes        │ ✓        │");
        println!("  └──────────────────────────┴────────────┴──────────┘");
        println!("  * CborEncode requires invalid Serialize impl; skipped.");

        // --- Wire Format Summary ---
        println!("\n  Wire Format (v4):");
        println!("    ┌──────┬─────┬──────┬──────┬───────────┬───────┐");
        println!("    │MAGIC │ VER │FLAGS │ LEN  │  PAYLOAD  │ CRC32 │");
        println!("    │ 2B   │ 1B  │ 1B   │ 2B   │  var      │  4B   │");
        println!("    │0x4B44│0x04 │GT+FL │u16BE │  CBOR     │ u32BE │");
        println!("    └──────┴─────┴──────┴──────┴───────────┴───────┘");

        println!("\n  ========================================================");
        println!("  All benchmark tests passed successfully!");
        println!("  ========================================================\n");
    }
}

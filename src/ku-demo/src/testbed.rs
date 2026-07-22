//! # Multi-Node Testbed
//!
//! In-process simulation of an OBP network with multiple nodes.
//! Tests end-to-end flows: creation, sync, query, trust propagation.

use crate::runtime::OBPNode;
use ku_core::*;

/// Run the multi-node testbed and return results.
pub fn run_testbed() -> TestbedReport {
    println!("═══════════════════════════════════════════════════════════");
    println!("  OneBrain Protocol — Multi-Node Testbed");
    println!("═══════════════════════════════════════════════════════════\n");

    // ─── Phase 1: Create 3 nodes ───────────────────────────────────────
    println!("▸ Phase 1: Creating 3 nodes...");

    let mut alice = OBPNode::new("Alice", [10, 0, 0, 1], 4242);
    let mut bob = OBPNode::new("Bob", [10, 0, 0, 2], 4242);
    let mut carol = OBPNode::new("Carol", [10, 0, 0, 3], 4242);

    // Register peers
    alice.add_peer(&bob);
    alice.add_peer(&carol);
    bob.add_peer(&alice);
    bob.add_peer(&carol);
    carol.add_peer(&alice);
    carol.add_peer(&bob);

    println!(
        "  {} — NodeId: {:02x}{:02x}...",
        alice.name, alice.node_id.0[0], alice.node_id.0[1]
    );
    println!(
        "  {} — NodeId: {:02x}{:02x}...",
        bob.name, bob.node_id.0[0], bob.node_id.0[1]
    );
    println!(
        "  {} — NodeId: {:02x}{:02x}...\n",
        carol.name, carol.node_id.0[0], carol.node_id.0[1]
    );

    // ─── Phase 2: Create KUs ───────────────────────────────────────────
    println!("▸ Phase 2: Creating Knowledge Units...");

    // Alice creates a Fact KU: "Water boils at 100°C"
    let ku_water = KnowledgeUnit {
        codons: vec![
            Codon {
                concept_id: 128,
                role: RoleId::Agent,
                qualifiers: vec![],
            },
            Codon {
                concept_id: 132,
                role: RoleId::Quality,
                qualifiers: vec![],
            },
            Codon {
                concept_id: 133,
                role: RoleId::Quantity,
                qualifiers: vec![],
            },
        ],
        bonds: vec![],
        gene: Gene::Fact {
            triples: vec![Triple {
                subject: 128,
                predicate: 132,
                object: 133,
            }],
            certainty: 9500,
            evidence: vec![],
        },
        flags: HeaderFlags::default(),
        epistemic_status: Some(EpistemicStatus::Evidence),
        evidence_type: Some(EvidenceType::Experimental),
        trust: Some(TrustSection {
            epistemic_status: EpistemicStatus::Evidence,
            evidence_type: EvidenceType::Experimental,
            verification_level: 3,
            corroboration_count: 10,
            challenge_count: 0,
            error_susceptibility: 0,
            trust_score: 9000,
            confidence: 8500,
            domain_codes: vec![100],
            verifications: vec![],
            challenges: vec![],
            ..Default::default()
        }),
        epigenetic: None,
    };

    // Bob creates a Fact KU: "Gravity = 9.8 m/s²"
    let ku_gravity = KnowledgeUnit {
        codons: vec![
            Codon {
                concept_id: 200,
                role: RoleId::Agent,
                qualifiers: vec![],
            },
            Codon {
                concept_id: 201,
                role: RoleId::Object,
                qualifiers: vec![],
            },
        ],
        bonds: vec![],
        gene: Gene::Fact {
            triples: vec![Triple {
                subject: 200,
                predicate: 201,
                object: 202,
            }],
            certainty: 9800,
            evidence: vec![],
        },
        flags: HeaderFlags::default(),
        epistemic_status: Some(EpistemicStatus::FormallyProven),
        evidence_type: Some(EvidenceType::Experimental),
        trust: Some(TrustSection {
            epistemic_status: EpistemicStatus::FormallyProven,
            evidence_type: EvidenceType::Experimental,
            verification_level: 5,
            corroboration_count: 100,
            challenge_count: 0,
            error_susceptibility: 0,
            trust_score: 9900,
            confidence: 9500,
            domain_codes: vec![200],
            verifications: vec![],
            challenges: vec![],
            ..Default::default()
        }),
        epigenetic: None,
    };

    // Carol creates a Procedure KU: "Recipe"
    let ku_recipe = KnowledgeUnit {
        codons: vec![Codon {
            concept_id: 300,
            role: RoleId::Agent,
            qualifiers: vec![],
        }],
        bonds: vec![],
        gene: Gene::Procedure {
            steps: vec![ProcedureStep {
                ord: 1,
                act: 301,
                pre: vec![],
                tgt: 302,
                tools: vec![],
                eff: vec![],
                warn: vec![],
            }],
            total_time: Some(1800),
            difficulty: 2,
            tools_req: vec![],
        },
        flags: HeaderFlags::default(),
        epistemic_status: Some(EpistemicStatus::Evidence),
        evidence_type: Some(EvidenceType::Observational),
        trust: Some(TrustSection {
            epistemic_status: EpistemicStatus::Evidence,
            evidence_type: EvidenceType::Observational,
            verification_level: 2,
            corroboration_count: 5,
            challenge_count: 1,
            error_susceptibility: 0,
            trust_score: 7000,
            confidence: 6000,
            domain_codes: vec![300],
            verifications: vec![],
            challenges: vec![],
            ..Default::default()
        }),
        epigenetic: None,
    };

    // Alice creates the water KU
    let cid_water = alice.create_ku(ku_water);
    println!(
        "  Alice created KU 'water' — CID: {:02x}{:02x}{:02x}{:02x}...",
        cid_water[0], cid_water[1], cid_water[2], cid_water[3]
    );

    // Bob creates the gravity KU
    let cid_gravity = bob.create_ku(ku_gravity);
    println!(
        "  Bob created KU 'gravity' — CID: {:02x}{:02x}{:02x}{:02x}...",
        cid_gravity[0], cid_gravity[1], cid_gravity[2], cid_gravity[3]
    );

    // Carol creates the recipe KU
    let cid_recipe = carol.create_ku(ku_recipe);
    println!(
        "  Carol created KU 'recipe' — CID: {:02x}{:02x}{:02x}{:02x}...\n",
        cid_recipe[0], cid_recipe[1], cid_recipe[2], cid_recipe[3]
    );

    // ─── Phase 3: CRDT Sync ───────────────────────────────────────────
    println!("▸ Phase 3: CRDT Sync (delta-state exchange)...");

    let sync_ab = alice.sync_with(&mut bob);
    println!(
        "  Alice ↔ Bob: {} deltas→us, {} deltas→peer",
        sync_ab.deltas_to_us, sync_ab.deltas_to_peer
    );

    let sync_ac = alice.sync_with(&mut carol);
    println!(
        "  Alice ↔ Carol: {} deltas→us, {} deltas→peer",
        sync_ac.deltas_to_us, sync_ac.deltas_to_peer
    );

    let sync_bc = bob.sync_with(&mut carol);
    println!(
        "  Bob ↔ Carol: {} deltas→us, {} deltas→peer\n",
        sync_bc.deltas_to_us, sync_bc.deltas_to_peer
    );

    // Verify convergence
    let a_count = alice.sync.local_count();
    let b_count = bob.sync.local_count();
    let c_count = carol.sync.local_count();
    println!(
        "  Convergence: Alice={}, Bob={}, Carol={} KUs",
        a_count, b_count, c_count
    );
    let converged = a_count == 3 && b_count == 3 && c_count == 3;
    println!("  All converged: {}\n", if converged { "✅" } else { "❌" });

    // ─── Phase 4: Trust Propagation ────────────────────────────────────
    println!("▸ Phase 4: Trust propagation (CRDT PN-Counter)...");

    // Multiple nodes corroborate the water KU
    alice.corroborate(&cid_water);
    bob.corroborate(&cid_water);
    carol.corroborate(&cid_water);

    // Carol challenges the recipe
    carol.challenge(&cid_recipe);

    println!("  Water trust (Alice): {}", alice.trust_score(&cid_water));
    println!(
        "  Recipe trust (Carol): {}\n",
        carol.trust_score(&cid_recipe)
    );

    // ─── Phase 5: KQL Queries ──────────────────────────────────────────
    println!("▸ Phase 5: KQL queries...");

    let q1 = "FIND (k:KU) WHERE k.trust_score > 8000";
    let r1 = alice.query(q1).unwrap_or_default();
    println!("  Query: \"{}\"", q1);
    println!("  Results: {} KUs (high-trust)\n", r1.len());

    let q2 = "FIND (k:KU) ORDER BY k.trust_score DESC LIMIT 1";
    let r2 = alice.query(q2).unwrap_or_default();
    println!("  Query: \"{}\"", q2);
    if let Some(top) = r2.first() {
        let score = top.trust_score();
        println!("  Top KU trust_score: {}\n", score);
    }

    // ─── Phase 6: Summary ──────────────────────────────────────────────
    println!("▸ Phase 6: Node summaries");
    println!("  {}", alice.summary());
    println!("  {}", bob.summary());
    println!("  {}", carol.summary());

    println!("\n═══════════════════════════════════════════════════════════");
    println!("  Testbed complete ✅");
    println!("═══════════════════════════════════════════════════════════\n");

    TestbedReport {
        nodes: 3,
        total_kus: a_count,
        converged,
        queries_run: 2,
        sync_rounds: 3,
    }
}

/// Summary report from testbed run.
#[derive(Debug)]
#[allow(dead_code)]
pub struct TestbedReport {
    pub nodes: usize,
    pub total_kus: usize,
    pub converged: bool,
    pub queries_run: usize,
    pub sync_rounds: usize,
}

// ═══════════════════════════════════════════════════════════════════════════
// Tests
// ═══════════════════════════════════════════════════════════════════════════

#[cfg(test)]
mod tests {
    use super::*;

    fn make_simple_ku(trust_score: u16) -> KnowledgeUnit {
        KnowledgeUnit {
            codons: vec![Codon {
                concept_id: 1,
                role: RoleId::Agent,
                qualifiers: vec![],
            }],
            bonds: vec![],
            gene: Gene::Fact {
                triples: vec![Triple {
                    subject: 1,
                    predicate: 2,
                    object: 3,
                }],
                certainty: 9500,
                evidence: vec![],
            },
            flags: HeaderFlags::default(),
            epistemic_status: Some(EpistemicStatus::Evidence),
            evidence_type: Some(EvidenceType::Experimental),
            trust: Some(TrustSection {
                epistemic_status: EpistemicStatus::Evidence,
                evidence_type: EvidenceType::Experimental,
                verification_level: 3,
                corroboration_count: 10,
                challenge_count: 0,
                error_susceptibility: 0,
                trust_score,
                confidence: 8000,
                domain_codes: vec![],
                verifications: vec![],
                challenges: vec![],
                ..Default::default()
            }),
            epigenetic: None,
        }
    }

    #[test]
    fn test_testbed_runs_and_converges() {
        let report = run_testbed();
        assert_eq!(report.nodes, 3);
        assert_eq!(report.total_kus, 3);
        assert!(report.converged, "All 3 nodes should converge to 3 KUs");
    }

    #[test]
    fn test_node_create_and_query() {
        let mut node = OBPNode::new("Test", [127, 0, 0, 1], 4242);
        node.create_ku(make_simple_ku(9000));
        let results = node
            .query("FIND (k:KU) WHERE k.trust_score > 5000")
            .unwrap();
        assert_eq!(results.len(), 1);
    }

    #[test]
    fn test_trust_crdt_corroborate() {
        let mut node = OBPNode::new("TrustTest", [127, 0, 0, 1], 4242);
        let ku = make_simple_ku(5000);
        let cid = node.create_ku(ku);
        assert_eq!(node.trust_score(&cid), 0);

        node.corroborate(&cid);
        node.corroborate(&cid);
        assert_eq!(node.trust_score(&cid), 2);

        node.challenge(&cid);
        assert_eq!(node.trust_score(&cid), 1);
    }

    #[test]
    fn test_two_node_sync() {
        let mut a = OBPNode::new("A", [10, 0, 0, 1], 4242);
        let mut b = OBPNode::new("B", [10, 0, 0, 2], 4242);

        a.create_ku(make_simple_ku(8000));
        assert_eq!(a.sync.local_count(), 1);
        assert_eq!(b.sync.local_count(), 0);

        let report = a.sync_with(&mut b);
        assert_eq!(report.deltas_to_peer, 1);
        assert_eq!(b.sync.local_count(), 1);
    }
}

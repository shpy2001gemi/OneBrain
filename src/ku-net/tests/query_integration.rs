//! # KQL Integration Tests — Phase G1+G2
//!
//! End-to-end tests for the full KQL pipeline and stress tests.

use ku_core::foundation::ConceptCcid;
use ku_core::*;
use ku_net::identity::*;
use ku_net::query::cache::QueryCache;
use ku_net::query::discovery::bridges::BridgeFinder;
use ku_net::query::discovery::gaps::GapDetector;
use ku_net::query::discovery::serendipity::SerendipityEngine;
use ku_net::query::index::ConceptIndex;
use ku_net::query::learning::{PheromoneLearner, QueryOutcome};
use ku_net::query::merger::ResultMerger;
use ku_net::query::messages::{QueryForwardMsg, QueryScope};
use ku_net::query::router::QueryRouter;
use ku_net::query::watch::{WatchCondition, WatchEngine, WatchEvent};
use std::time::Duration;

fn concept(value: u128) -> ConceptCcid {
    ConceptCcid::from_bytes(value.to_be_bytes())
}

// ════════════════════════════════════════════════════════════════════════════
// Helpers
// ════════════════════════════════════════════════════════════════════════════

fn make_node_id() -> NodeId {
    let kp = KeyPair::generate();
    let proof = generate_node_id(&kp.pubkey_bytes(), PUZZLE_C_SMALL);
    proof.node_id
}

fn make_ku_with_gene(concept_id: u64, trust_score: u16, gene_type: u8) -> KuRuntime {
    let dna = CoreDna {
        header: CoreDnaHeader {
            version: 2,
            gene_type,
            has_concept_table: true,
        },
        concept_table: vec![ConceptTableEntry {
            local_id: concept_id,
            ccid: *concept(concept_id as u128).as_bytes(),
        }],
        instructions: vec![
            Instruction::Triple {
                s: concept_id,
                p: 2,
                o: 3,
            },
            Instruction::Certainty { level: 8000 },
        ],
    };
    let mut ku = KuRuntime::from_dna(dna).expect("test KU should encode");
    ku.epi = Epigenetics::with_trust(trust_score, 8000);
    ku.epi.trust.epistemic_status = EpistemicStatus::Evidence;
    ku.epi.trust.evidence_type = EvidenceType::Experimental;
    ku.epi.trust.verification_level = 3;
    ku.epi.trust.corroboration_count = 5;
    ku
}

fn make_ku(concept_id: u64, trust_score: u16) -> KuRuntime {
    make_ku_with_gene(concept_id, trust_score, 0)
}

fn make_ku_with_ctx(concept_id: u64, trust_score: u16, ctx: &[u64]) -> KuRuntime {
    let mut ku = make_ku(concept_id, trust_score);
    for &local_id in ctx {
        if !ku
            .dna
            .concept_table
            .iter()
            .any(|entry| entry.local_id == local_id)
        {
            ku.dna.concept_table.push(ConceptTableEntry {
                local_id,
                ccid: *concept(local_id as u128).as_bytes(),
            });
        }
    }
    let mut rebuilt = KuRuntime::from_dna(ku.dna.clone()).expect("context KU should encode");
    rebuilt.epi = ku.epi;
    ku = rebuilt;
    ku.epi.add_bond(vec![0x42; 32], RelationType::Extends, 8000);
    ku.epi.bonds[0].context.extend_from_slice(ctx);
    ku
}

// ════════════════════════════════════════════════════════════════════════════
// G1: End-to-End Pipeline Tests
// ════════════════════════════════════════════════════════════════════════════

/// Full KQL pipeline: Index → QueryMsg → Merge → Results
#[test]
fn test_e2e_full_query_pipeline() {
    // 1. Index concepts
    let mut index = ConceptIndex::new(10_000);
    for concept_id in 1..=100u128 {
        index.register_concept(concept(concept_id));
    }

    // 2. Create query via wire format
    let origin = make_node_id();
    let query = QueryForwardMsg::new(
        "FIND (k:KU) WHERE k.trust_score > 5000".to_string(),
        origin,
        QueryScope::Dht,
        10, // max_results
    );

    // 3. Check concept in index
    assert!(
        index.has_concept(concept(42)),
        "Concept 42 should be indexed"
    );
    assert!(
        index.might_have_concept(concept(42)),
        "VacuumFilter should contain 42"
    );

    // 4. Merge results from multiple "remote" nodes
    let mut merger = ResultMerger::new(query.query_id, 10);
    merger.add_results(vec![make_ku(42, 9000), make_ku(43, 7000)], QueryScope::Dht);
    merger.add_results(vec![make_ku(44, 8000)], QueryScope::Neighbors);

    // 5. Finalize and verify
    let results = merger.finalize();
    assert_eq!(results.len(), 3, "Should have 3 unique results");
    assert!(
        results[0].score >= results[1].score,
        "Results should be ranked"
    );
}

/// Watch + Discovery pipeline
#[test]
fn test_e2e_watch_fires_on_new_ku() {
    let my_id = make_node_id();
    let mut engine = WatchEngine::new(my_id);

    // Register watch for high-trust concept 42
    let wid = engine
        .register(
            make_node_id(),
            WatchEvent::Create,
            WatchCondition::And(
                Box::new(WatchCondition::TrustAbove(7000)),
                Box::new(WatchCondition::HasConcept(concept(42))),
            ),
            "callback://discovery".to_string(),
            3,
        )
        .unwrap();

    // Simulate events
    assert_eq!(
        engine
            .on_ku_event(&make_ku(42, 9000), WatchEvent::Create)
            .len(),
        1
    );
    assert_eq!(
        engine
            .on_ku_event(&make_ku(42, 3000), WatchEvent::Create)
            .len(),
        0
    );
    assert_eq!(
        engine
            .on_ku_event(&make_ku(99, 9000), WatchEvent::Create)
            .len(),
        0
    );

    assert_eq!(engine.get(wid).unwrap().fire_count, 1);
}

/// Gap detection → Watch generation pipeline
#[test]
fn test_e2e_gap_to_watch_pipeline() {
    let kus = vec![
        make_ku_with_ctx(1, 9000, &[99]),
        make_ku_with_ctx(2, 9000, &[99]),
        make_ku(3, 1000),
        make_ku(3, 1000),
    ];

    let detector = GapDetector::new();
    let report = detector.analyze(&kus);
    assert!(!report.gaps.is_empty(), "Should detect gaps");

    // Convert gaps into watches
    let my_id = make_node_id();
    let mut watch_engine = WatchEngine::new(my_id);
    for gap in &report.gaps {
        for &concept_ccid in &gap.concept_ccids {
            watch_engine.register(
                make_node_id(),
                WatchEvent::Create,
                WatchCondition::HasConcept(concept_ccid),
                format!("gap://fill/{concept_ccid}"),
                2,
            );
        }
    }
    assert!(watch_engine.count() > 0);
}

/// Serendipity pipeline
#[test]
fn test_e2e_serendipity_pipeline() {
    let mut user_kus = Vec::new();
    for _ in 0..5 {
        user_kus.push(make_ku(100, 8000));
    }
    for _ in 0..3 {
        user_kus.push(make_ku(200, 7000));
    }
    user_kus.push(make_ku(300, 6000));

    let mut engine = SerendipityEngine::new();
    engine.build_profile(&user_kus);

    let queries = engine.generate_exploration_queries();
    assert!(!queries.is_empty(), "Should generate exploration queries");

    let candidates = vec![make_ku(150, 9000), make_ku(999, 9000)];
    let discoveries = engine.evaluate_candidates(&candidates);
    for d in &discoveries {
        assert!(d.serendipity_score >= 0.0);
        assert!(!d.suggested_query.is_empty());
    }
}

/// Cross-domain bridge discovery
#[test]
fn test_e2e_cross_domain_discovery() {
    let mut finder = BridgeFinder::new();
    finder.min_strength = 0.01;

    let kus = vec![
        make_ku_with_ctx(100, 9000, &[50, 51]),
        make_ku_with_ctx(100, 8500, &[50, 52]),
        make_ku_with_ctx(100, 8000, &[51]),
        make_ku_with_ctx(200, 7000, &[50, 53]),
        make_ku_with_ctx(200, 7500, &[50]),
    ];

    let report = finder.analyze(&kus);
    assert!(report.domains_analyzed >= 2);
    assert!(report.bridge_concepts_found > 0);
}

/// Router + Merger + Learner feedback loop
#[test]
fn test_e2e_route_merge_learn() {
    let my_id = make_node_id();
    let mut router = QueryRouter::new(my_id);
    let mut learner = PheromoneLearner::new();

    // Add neighbors
    for _ in 0..5 {
        router.add_neighbor(make_node_id());
    }
    assert_eq!(router.neighbor_count(), 5);

    // Merge results
    let query_id = [0x42; 16];
    let mut merger = ResultMerger::new(query_id, 10);
    merger.add_results(vec![make_ku(42, 9000)], QueryScope::Neighbors);
    let results = merger.finalize();
    assert_eq!(results.len(), 1);

    // Feed back to learner
    learner.record_outcome(&QueryOutcome {
        concept: concept(42),
        resolved_at: QueryScope::Neighbors,
        provider: None,
        quality: results[0].score,
        latency_ms: 50,
        result_count: results.len(),
    });

    let recs = learner.recommend_scopes(concept(42));
    assert_eq!(recs[0].0, QueryScope::Neighbors);
}

/// Cache integration
#[test]
fn test_e2e_cache_hit_miss() {
    let mut cache = QueryCache::new(100, Duration::from_secs(60));
    let kql = "FIND (k:KU) WHERE k.trust_score > 5000";

    assert!(cache.get(kql).is_none());

    cache.put(kql.to_string(), vec![0xBF, 0xFF], 5);

    let cached = cache.get(kql).unwrap();
    assert_eq!(cached.result_count, 5);
    assert_eq!(cache.hit_rate(), 50.0);

    // Normalized query also hits
    assert!(cache
        .get("find  (k:ku)  where  k.trust_score > 5000")
        .is_some());
}

// ════════════════════════════════════════════════════════════════════════════
// G2: Stress Tests
// ════════════════════════════════════════════════════════════════════════════

/// Stress: Index 10,000 concepts
#[test]
fn test_stress_concept_index_10k() {
    let mut index = ConceptIndex::new(20_000);

    for id in 0..10_000u128 {
        index.register_concept(concept(id));
    }

    let mut found = 0;
    for id in 0..10_000u128 {
        if index.has_concept(concept(id)) {
            found += 1;
        }
    }
    assert_eq!(found, 10_000);
}

/// Stress: Merge from 100 "nodes"
#[test]
fn test_stress_merger_100_responses() {
    let query_id = [0x01; 16];
    let mut merger = ResultMerger::new(query_id, 50);

    for node in 0..100u64 {
        let kus: Vec<_> = (0..5)
            .map(|i| make_ku(node * 100 + i, 5000 + (i as u16) * 1000))
            .collect();
        merger.add_results(kus, QueryScope::Dht);
    }

    assert_eq!(merger.responses_received(), 100);
    let results = merger.finalize();
    assert_eq!(results.len(), 50);

    for i in 1..results.len() {
        assert!(results[i - 1].score >= results[i].score);
    }
}

/// Stress: 1000 watches
#[test]
fn test_stress_1000_watches() {
    let my_id = make_node_id();
    let mut engine = WatchEngine::new(my_id);

    for i in 0..1000u64 {
        let threshold = (i % 10) as u16 * 1000;
        engine.register(
            my_id,
            WatchEvent::Any,
            WatchCondition::TrustAbove(threshold),
            format!("watch://{}", i),
            0,
        );
    }
    assert_eq!(engine.count(), 1000);

    let ku = make_ku(1, 9500);
    let notifs = engine.on_ku_event(&ku, WatchEvent::Create);
    assert_eq!(notifs.len(), 1000);
}

/// Stress: Cache with 1000 entries
#[test]
fn test_stress_cache_1000_entries() {
    let mut cache = QueryCache::new(500, Duration::from_secs(60));

    for i in 0..1000u64 {
        cache.put(format!("FIND concept_{}", i), vec![i as u8], 1);
    }
    assert_eq!(cache.len(), 500);

    assert!(cache.get("FIND concept_0").is_none());
    assert!(cache.get("FIND concept_999").is_some());
}

/// Stress: Learner with 1000 outcomes
#[test]
fn test_stress_learner_1000_outcomes() {
    let mut learner = PheromoneLearner::new();

    for concept_value in 0..100u64 {
        for _ in 0..10 {
            let scope = match concept_value % 6 {
                0 => QueryScope::Local,
                1 => QueryScope::Neighbors,
                2 => QueryScope::Cluster,
                3 => QueryScope::Dht,
                4 => QueryScope::Semantic,
                _ => QueryScope::Global,
            };
            let quality = if concept_value % 3 == 0 { 0.9 } else { 0.2 };
            learner.record_outcome(&QueryOutcome {
                concept: concept(concept_value as u128),
                resolved_at: scope,
                provider: None,
                quality,
                latency_ms: 50,
                result_count: if quality > 0.5 { 5 } else { 0 },
            });
        }
    }

    assert_eq!(learner.route_count(), 100);
    let rate = learner.global_success_rate();
    assert!(rate > 0.0 && rate < 1.0);
}

/// Stress: Gap detector with 500 KUs
#[test]
fn test_stress_gap_detector_500_kus() {
    let mut kus = Vec::new();

    for i in 0..200u64 {
        kus.push(make_ku_with_ctx(i % 50, 8000, &[1000 + i]));
    }
    for i in 0..100u64 {
        kus.push(make_ku(200 + i % 20, 500));
    }
    for i in 0..100u64 {
        let mut ku = make_ku_with_gene(300 + i % 30, 3000, 1);
        ku.epi.trust.corroboration_count = 0;
        ku.epi.trust.challenge_count = 0;
        kus.push(ku);
    }
    for i in 0..100u64 {
        kus.push(make_ku(400 + i, 9000));
    }

    let detector = GapDetector::new();
    let report = detector.analyze(&kus);
    assert_eq!(report.kus_analyzed, 500);
    assert!(!report.gaps.is_empty());
    for i in 1..report.gaps.len() {
        assert!(report.gaps[i - 1].severity >= report.gaps[i].severity);
    }
}

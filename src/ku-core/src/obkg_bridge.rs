//! # OBKG Bridge — Read-Only Adapter Layer
//!
//! Bridges existing pillar types to OBKG engine inputs.
//! Follows the OBT integration pattern: reads from KuRuntime, never modifies.
//!
//! ## Design Principle
//! > "Pillar sau build bridges, đừng break foundations."
//! OBKG (P7) adapts to P1-P5, not the other way around.

use crate::types::{Bond, RelationType};
use crate::ku_runtime::KuRuntime;
use crate::graph_types::{BondMeta, BondEvent};
use crate::graph_embeddings::EntityEmbedding;
use crate::graph_dream::AccessRecord;
use crate::graph_bio::{CoAccess, ConsolidationCandidate};
use std::collections::HashMap;

// ============================================================================
// 1. Bond Conversion Functions
// ============================================================================

/// Convert a Bond creation into a BondEvent.
/// Used by ObkgOrchestrator when it detects new bonds.
pub fn bond_to_created_event(
    source_cid: &[u8; 32],
    bond: &Bond,
    timestamp: u64,
) -> BondEvent {
    BondEvent::Created {
        source_cid: *source_cid,
        target_cid: cid_from_bond_target(&bond.target_cid),
        relation: bond.relation,
        weight: bond.weight,
        creator: bond.creator,
        evidence: bond.evidence.clone(),
        timestamp,
    }
}

/// Convert Bond target_cid (Vec<u8>) to [u8; 32].
///
/// Copies up to 32 bytes from the variable-length target CID,
/// zero-padding if shorter than 32 bytes.
pub fn cid_from_bond_target(target: &[u8]) -> [u8; 32] {
    let mut cid = [0u8; 32];
    let len = target.len().min(32);
    cid[..len].copy_from_slice(&target[..len]);
    cid
}

/// Collect all bonds from a KuRuntime as (source, target, relation, BondMeta) tuples.
pub fn collect_bond_metas(ku: &KuRuntime) -> Vec<([u8; 32], [u8; 32], RelationType, BondMeta)> {
    ku.epi.bonds.iter().map(|bond| {
        (ku.cid, cid_from_bond_target(&bond.target_cid), bond.relation, BondMeta::from_bond(bond))
    }).collect()
}

/// Collect bonds from multiple KUs into a HashMap suitable for DreamEngine/DecayRunner.
pub fn collect_all_bonds(
    kus: &HashMap<[u8; 32], KuRuntime>,
) -> HashMap<([u8; 32], [u8; 32], RelationType), BondMeta> {
    let mut map = HashMap::new();
    for ku in kus.values() {
        for bond in &ku.epi.bonds {
            let key = (ku.cid, cid_from_bond_target(&bond.target_cid), bond.relation);
            map.insert(key, BondMeta::from_bond(bond));
        }
    }
    map
}

/// Collect bonds as (source_target_pair, Bond) tuples for DecayRunner.
pub fn collect_bonds_for_decay(
    kus: &HashMap<[u8; 32], KuRuntime>,
) -> Vec<(([u8; 32], [u8; 32]), Bond)> {
    let mut result = Vec::new();
    for ku in kus.values() {
        for bond in &ku.epi.bonds {
            result.push(((ku.cid, cid_from_bond_target(&bond.target_cid)), bond.clone()));
        }
    }
    result
}

// ============================================================================
// 2. Embedding Bridge
// ============================================================================

/// Generate an EntityEmbedding from a KU's CID.
/// Uses CID as seed for deterministic embedding initialization.
pub fn ku_to_entity_embedding(ku: &KuRuntime) -> ([u8; 32], EntityEmbedding) {
    (ku.cid, EntityEmbedding::from_seed(&ku.cid))
}

/// Collect entity embeddings from all KUs.
pub fn collect_entity_embeddings(
    kus: &HashMap<[u8; 32], KuRuntime>,
) -> Vec<([u8; 32], EntityEmbedding)> {
    kus.values().map(|ku| ku_to_entity_embedding(ku)).collect()
}

// ============================================================================
// 3. Dream Engine Bridge
// ============================================================================

/// Build AccessRecords from KU bond data for DreamEngine.
/// Uses bond weights as access indicators (higher weight = more accessed).
pub fn build_access_log(
    kus: &HashMap<[u8; 32], KuRuntime>,
    min_weight: u16,
) -> Vec<AccessRecord> {
    let mut records = Vec::new();
    for ku in kus.values() {
        for bond in &ku.epi.bonds {
            if bond.weight >= min_weight {
                records.push(AccessRecord {
                    source_cid: ku.cid,
                    target_cid: cid_from_bond_target(&bond.target_cid),
                    relation: bond.relation,
                    access_count: (bond.weight / 100).max(1) as u32, // weight→access proxy
                    last_access: bond.created_at as u64,
                });
            }
        }
    }
    records
}

// ============================================================================
// 4. STDP Bridge
// ============================================================================

/// Build CoAccess records for STDP from recent bond pairs.
/// delta_t is estimated from bond creation timestamps.
pub fn build_co_accesses(
    kus: &HashMap<[u8; 32], KuRuntime>,
) -> Vec<CoAccess> {
    let mut accesses = Vec::new();
    for ku in kus.values() {
        // For each pair of bonds from the same KU, create a CoAccess
        let bonds = &ku.epi.bonds;
        for i in 0..bonds.len() {
            for j in (i + 1)..bonds.len() {
                let t_i = bonds[i].created_at as f64;
                let t_j = bonds[j].created_at as f64;
                let delta_t = t_j - t_i; // positive = j after i
                if delta_t.abs() < 86400.0 { // within 24h
                    accesses.push(CoAccess {
                        source_cid: ku.cid,
                        target_cid: cid_from_bond_target(&bonds[j].target_cid),
                        relation: bonds[j].relation,
                        current_weight: bonds[j].weight,
                        delta_t,
                    });
                }
            }
        }
    }
    accesses
}

// ============================================================================
// 5. Consolidation Bridge
// ============================================================================

/// Build ConsolidationCandidates from KU data.
///
/// Uses the earliest bond `created_at` as a proxy for KU creation time,
/// since `CoreDnaHeader` does not carry a timestamp field.
pub fn build_consolidation_candidates(
    kus: &HashMap<[u8; 32], KuRuntime>,
    now_secs: u64,
) -> Vec<ConsolidationCandidate> {
    kus.values().map(|ku| {
        // Use earliest bond created_at as proxy for KU age (0 if no bonds)
        let earliest = ku.epi.bonds.iter()
            .map(|b| b.created_at)
            .min()
            .unwrap_or(0);
        let age_hours = (now_secs.saturating_sub(earliest as u64)) as f64 / 3600.0;
        ConsolidationCandidate {
            cid: ku.cid,
            retrieval_count: 0, // would come from metabolism, reads only
            pomv_score: ku.epi.pomv_score(),
            bond_count: ku.epi.bonds.len(),
            age_hours,
        }
    }).collect()
}

// ============================================================================
// 6. Diff Detection
// ============================================================================

/// Detect new bonds added since last snapshot.
/// Returns bonds present in `current` but not in `previous`.
pub fn diff_bonds(
    previous: &HashMap<([u8; 32], [u8; 32], RelationType), BondMeta>,
    current: &HashMap<([u8; 32], [u8; 32], RelationType), BondMeta>,
) -> Vec<([u8; 32], [u8; 32], RelationType, BondMeta)> {
    current.iter()
        .filter(|(key, _)| !previous.contains_key(key))
        .map(|(key, meta)| (key.0, key.1, key.2, *meta))
        .collect()
}

/// Detect bonds removed since last snapshot.
pub fn removed_bonds(
    previous: &HashMap<([u8; 32], [u8; 32], RelationType), BondMeta>,
    current: &HashMap<([u8; 32], [u8; 32], RelationType), BondMeta>,
) -> Vec<([u8; 32], [u8; 32], RelationType)> {
    previous.keys()
        .filter(|key| !current.contains_key(key))
        .copied()
        .collect()
}

// ============================================================================
// Tests
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core_dna::{CoreDna, CoreDnaHeader, Instruction};
    use crate::types::{Creator, EdgeState, DecayRate};

    // ── Test helpers ─────────────────────────────────────────────────────

    fn make_test_ku() -> KuRuntime {
        let dna = CoreDna {
            header: CoreDnaHeader {
                version: 1,
                gene_type: 0,
                has_qualifiers: false,
            },
            instructions: vec![
                Instruction::Triple { s: 301, p: 500, o: 1042 },
            ],
        };
        KuRuntime::from_dna(dna).unwrap()
    }

    fn make_bond(target_byte: u8, relation: RelationType, weight: u16, created_at: u32) -> Bond {
        Bond {
            target_cid: vec![target_byte; 32],
            relation,
            weight,
            creator: Creator::Human,
            created_at,
            evidence: vec![],
            state: EdgeState::Active,
            initial_weight: Some(weight),
            decay: Some(DecayRate::None),
            last_reinforced: None,
            reinforce_count: None,
            bidirectional: None,
            context: vec![],
            order: None,
            required: None,
        }
    }

    fn make_ku_with_bonds(bonds: Vec<Bond>) -> KuRuntime {
        let mut ku = make_test_ku();
        ku.epi.bonds = bonds;
        ku
    }

    // ── 1. bond_to_created_event creates correct BondEvent ───────────────

    #[test]
    fn bond_to_created_event_correct() {
        let source = [0xAAu8; 32];
        let bond = make_bond(0xBB, RelationType::Extends, 7000, 1_600_000_000);
        let event = bond_to_created_event(&source, &bond, 1_700_000_000);

        match event {
            BondEvent::Created { source_cid, target_cid, relation, weight, creator, timestamp, .. } => {
                assert_eq!(source_cid, source);
                assert_eq!(target_cid, [0xBBu8; 32]);
                assert_eq!(relation, RelationType::Extends);
                assert_eq!(weight, 7000);
                assert_eq!(creator, Creator::Human);
                assert_eq!(timestamp, 1_700_000_000);
            }
            _ => panic!("expected Created event"),
        }
    }

    // ── 2. cid_from_bond_target handles short CIDs ──────────────────────

    #[test]
    fn cid_from_bond_target_short() {
        let short = vec![0xAA, 0xBB, 0xCC];
        let cid = cid_from_bond_target(&short);
        assert_eq!(cid[0], 0xAA);
        assert_eq!(cid[1], 0xBB);
        assert_eq!(cid[2], 0xCC);
        assert_eq!(cid[3], 0x00); // zero-padded
        assert_eq!(cid[31], 0x00);
    }

    // ── 3. cid_from_bond_target handles full CIDs ───────────────────────

    #[test]
    fn cid_from_bond_target_full() {
        let full = vec![0xFFu8; 32];
        let cid = cid_from_bond_target(&full);
        assert_eq!(cid, [0xFFu8; 32]);
    }

    // ── 4. collect_bond_metas from KU with N bonds ──────────────────────

    #[test]
    fn collect_bond_metas_correct() {
        let ku = make_ku_with_bonds(vec![
            make_bond(0x01, RelationType::PartOf, 5000, 1000),
            make_bond(0x02, RelationType::Extends, 8000, 2000),
        ]);
        let metas = collect_bond_metas(&ku);
        assert_eq!(metas.len(), 2);
        assert_eq!(metas[0].3.weight, 5000);
        assert_eq!(metas[1].3.weight, 8000);
    }

    // ── 5. collect_all_bonds from multiple KUs ──────────────────────────

    #[test]
    fn collect_all_bonds_multiple_kus() {
        let ku1 = make_ku_with_bonds(vec![
            make_bond(0x01, RelationType::PartOf, 5000, 1000),
        ]);
        let ku2 = {
            // Create a different KU with different CID
            let dna = CoreDna {
                header: CoreDnaHeader { version: 1, gene_type: 1, has_qualifiers: false },
                instructions: vec![Instruction::Triple { s: 100, p: 200, o: 300 }],
            };
            let mut ku = KuRuntime::from_dna(dna).unwrap();
            ku.epi.bonds = vec![make_bond(0x02, RelationType::Causes, 7000, 2000)];
            ku
        };
        let mut kus = HashMap::new();
        kus.insert(ku1.cid, ku1);
        kus.insert(ku2.cid, ku2);

        let bonds = collect_all_bonds(&kus);
        assert_eq!(bonds.len(), 2);
    }

    // ── 6. ku_to_entity_embedding deterministic ─────────────────────────

    #[test]
    fn ku_to_entity_embedding_deterministic() {
        let ku = make_test_ku();
        let (cid1, emb1) = ku_to_entity_embedding(&ku);
        let (cid2, emb2) = ku_to_entity_embedding(&ku);
        assert_eq!(cid1, cid2);
        assert_eq!(emb1, emb2);
    }

    // ── 7. build_access_log filters by min_weight ───────────────────────

    #[test]
    fn build_access_log_filters_by_weight() {
        let ku = make_ku_with_bonds(vec![
            make_bond(0x01, RelationType::PartOf, 500, 1000),  // below threshold
            make_bond(0x02, RelationType::Extends, 3000, 2000), // above threshold
        ]);
        let mut kus = HashMap::new();
        kus.insert(ku.cid, ku);

        let log = build_access_log(&kus, 1000);
        assert_eq!(log.len(), 1);
        assert_eq!(log[0].relation, RelationType::Extends);
    }

    // ── 8. build_access_log empty KU ────────────────────────────────────

    #[test]
    fn build_access_log_empty() {
        let ku = make_test_ku(); // no bonds
        let mut kus = HashMap::new();
        kus.insert(ku.cid, ku);

        let log = build_access_log(&kus, 0);
        assert!(log.is_empty());
    }

    // ── 9. diff_bonds detects new bonds ─────────────────────────────────

    #[test]
    fn diff_bonds_detects_new() {
        let previous: HashMap<([u8; 32], [u8; 32], RelationType), BondMeta> = HashMap::new();
        let mut current = HashMap::new();
        let meta = BondMeta {
            weight: 5000,
            creator: Creator::Human,
            state: EdgeState::Active,
            decay: DecayRate::None,
            timestamp: 1000,
        };
        current.insert(([1u8; 32], [2u8; 32], RelationType::Extends), meta);

        let diff = diff_bonds(&previous, &current);
        assert_eq!(diff.len(), 1);
        assert_eq!(diff[0].3.weight, 5000);
    }

    // ── 10. diff_bonds empty sets ───────────────────────────────────────

    #[test]
    fn diff_bonds_empty_sets() {
        let previous: HashMap<([u8; 32], [u8; 32], RelationType), BondMeta> = HashMap::new();
        let current: HashMap<([u8; 32], [u8; 32], RelationType), BondMeta> = HashMap::new();
        let diff = diff_bonds(&previous, &current);
        assert!(diff.is_empty());
    }

    // ── 11. removed_bonds detects deletions ─────────────────────────────

    #[test]
    fn removed_bonds_detects_deletions() {
        let meta = BondMeta {
            weight: 5000,
            creator: Creator::Human,
            state: EdgeState::Active,
            decay: DecayRate::None,
            timestamp: 1000,
        };
        let mut previous = HashMap::new();
        previous.insert(([1u8; 32], [2u8; 32], RelationType::Extends), meta);
        let current: HashMap<([u8; 32], [u8; 32], RelationType), BondMeta> = HashMap::new();

        let removed = removed_bonds(&previous, &current);
        assert_eq!(removed.len(), 1);
        assert_eq!(removed[0].2, RelationType::Extends);
    }

    // ── 12. collect_bonds_for_decay format ──────────────────────────────

    #[test]
    fn collect_bonds_for_decay_format() {
        let ku = make_ku_with_bonds(vec![
            make_bond(0x01, RelationType::PartOf, 5000, 1000),
        ]);
        let mut kus = HashMap::new();
        let cid = ku.cid;
        kus.insert(cid, ku);

        let bonds = collect_bonds_for_decay(&kus);
        assert_eq!(bonds.len(), 1);
        assert_eq!(bonds[0].0.0, cid); // source
        assert_eq!(bonds[0].0.1, [0x01u8; 32]); // target
        assert_eq!(bonds[0].1.weight, 5000);
    }

    // ── 13. build_consolidation_candidates pomv_score ────────────────────

    #[test]
    fn build_consolidation_candidates_has_pomv() {
        let ku = make_ku_with_bonds(vec![
            make_bond(0x01, RelationType::PartOf, 5000, 1000),
            make_bond(0x02, RelationType::Extends, 8000, 2000),
        ]);
        let mut kus = HashMap::new();
        kus.insert(ku.cid, ku);

        let candidates = build_consolidation_candidates(&kus, 100_000);
        assert_eq!(candidates.len(), 1);
        assert_eq!(candidates[0].bond_count, 2);
        // pomv_score should be a valid f64 (default trust all zeros → score = 0.0)
        assert!(candidates[0].pomv_score >= 0.0);
    }

    // ── 14. collect_entity_embeddings from multiple KUs ──────────────────

    #[test]
    fn collect_entity_embeddings_multiple() {
        let ku1 = make_test_ku();
        let ku2 = {
            let dna = CoreDna {
                header: CoreDnaHeader { version: 1, gene_type: 1, has_qualifiers: false },
                instructions: vec![Instruction::Triple { s: 100, p: 200, o: 300 }],
            };
            KuRuntime::from_dna(dna).unwrap()
        };
        let mut kus = HashMap::new();
        kus.insert(ku1.cid, ku1);
        kus.insert(ku2.cid, ku2);

        let embeddings = collect_entity_embeddings(&kus);
        assert_eq!(embeddings.len(), 2);
        // Each embedding should have a non-zero CID
        for (cid, _emb) in &embeddings {
            assert_ne!(*cid, [0u8; 32]);
        }
    }

    // ── 15. build_co_accesses within 24h window ─────────────────────────

    #[test]
    fn build_co_accesses_within_window() {
        let ku = make_ku_with_bonds(vec![
            make_bond(0x01, RelationType::PartOf, 5000, 1000),
            make_bond(0x02, RelationType::Extends, 8000, 1500), // 500s apart (within 24h)
            make_bond(0x03, RelationType::Causes, 3000, 100_000), // 99000s apart (>24h from first)
        ]);
        let mut kus = HashMap::new();
        kus.insert(ku.cid, ku);

        let co_accesses = build_co_accesses(&kus);
        // Pairs within 24h: (0,1)=500s, (1,2)=98500s ✓
        // Pair (0,2)=99000s > 86400 → excluded
        assert!(co_accesses.len() >= 1);
        // At least bond pair (0,1) should be present
        let has_close_pair = co_accesses.iter().any(|ca| ca.delta_t.abs() < 1000.0);
        assert!(has_close_pair, "should have at least one close pair");
    }
}

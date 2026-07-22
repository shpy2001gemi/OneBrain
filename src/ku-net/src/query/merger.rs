//! # Result Merger — Phase D3
//!
//! Deduplicates, ranks, and merges results from distributed queries.
//! Results arriving from multiple nodes are combined into a single
//! ordered result set.

use ku_core::KuRuntime;
use std::collections::HashMap;

use super::messages::{QueryId, QueryScope};

// ═══════════════════════════════════════════════════════════════════════════
// Result Merger
// ═══════════════════════════════════════════════════════════════════════════

/// A single result with provenance information.
#[derive(Debug, Clone)]
pub struct RankedResult {
    /// The KU data.
    pub ku: KuRuntime,
    /// Content hash for deduplication (BLAKE3 of serialized KU).
    pub content_hash: [u8; 32],
    /// Trust-based relevance score [0.0, 1.0].
    pub score: f64,
    /// Scope at which this result was found.
    pub found_at: QueryScope,
    /// Number of independent sources that returned this same result.
    pub source_count: u32,
}

/// Accumulates and merges results from distributed query responses.
pub struct ResultMerger {
    /// Query this merger is tracking.
    query_id: QueryId,
    /// Accumulated results keyed by content hash.
    results: HashMap<[u8; 32], RankedResult>,
    /// Maximum results to keep.
    max_results: usize,
    /// Number of responses received.
    responses_received: u32,
    /// Whether the merge is finalized.
    finalized: bool,
}

impl ResultMerger {
    /// Create a new merger for a query.
    pub fn new(query_id: QueryId, max_results: usize) -> Self {
        Self {
            query_id,
            results: HashMap::new(),
            max_results,
            responses_received: 0,
            finalized: false,
        }
    }

    /// Add results from a query response.
    ///
    /// Deduplicates by content hash and merges source counts.
    pub fn add_results(&mut self, kus: Vec<KuRuntime>, scope: QueryScope) {
        if self.finalized {
            return;
        }

        self.responses_received += 1;

        for ku in kus {
            let hash = compute_ku_hash(&ku);
            let score = compute_score(&ku, &scope);

            if let Some(existing) = self.results.get_mut(&hash) {
                // Same KU from another source → increase source_count and score
                existing.source_count += 1;
                // Corroboration boost: more sources = higher confidence
                existing.score = (existing.score + score * 0.5).min(1.0);
                // Keep the higher scope (more authoritative)
                if scope < existing.found_at {
                    existing.found_at = scope;
                }
            } else if self.results.len() < self.max_results * 2 {
                // New unique result
                self.results.insert(
                    hash,
                    RankedResult {
                        ku,
                        content_hash: hash,
                        score,
                        found_at: scope,
                        source_count: 1,
                    },
                );
            }
        }
    }

    /// Finalize and return merged, ranked results.
    ///
    /// Sorts by score descending and truncates to max_results.
    pub fn finalize(&mut self) -> Vec<RankedResult> {
        self.finalized = true;

        let mut ranked: Vec<RankedResult> = self.results.values().cloned().collect();

        // Sort by: score DESC, source_count DESC, scope ASC (closer = better)
        ranked.sort_by(|a, b| {
            b.score
                .partial_cmp(&a.score)
                .unwrap_or(std::cmp::Ordering::Equal)
                .then(b.source_count.cmp(&a.source_count))
                .then((a.found_at as u8).cmp(&(b.found_at as u8)))
        });

        ranked.truncate(self.max_results);
        ranked
    }

    /// Check if we have enough results to stop early.
    pub fn has_enough(&self) -> bool {
        self.results.len() >= self.max_results
    }

    /// Number of unique results accumulated so far.
    pub fn result_count(&self) -> usize {
        self.results.len()
    }

    /// Number of responses received.
    pub fn responses_received(&self) -> u32 {
        self.responses_received
    }

    /// The query ID this merger tracks.
    pub fn query_id(&self) -> &QueryId {
        &self.query_id
    }
}

/// Compute a deterministic content hash for a KU (for dedup).
fn compute_ku_hash(ku: &KuRuntime) -> [u8; 32] {
    // Hash the serialized form
    let mut data = Vec::new();
    // Use concept_ids + gene_type as identity (bonds and trust can change)
    for concept_id in ku.concept_ids() {
        data.extend_from_slice(&concept_id.to_be_bytes());
        data.push(0u8); // no role equivalent in v6
    }
    // Hash gene type
    data.push(ku.gene_type());
    *blake3::hash(&data).as_bytes()
}

/// Compute a relevance score for a KU based on trust and scope.
fn compute_score(ku: &KuRuntime, scope: &QueryScope) -> f64 {
    // Base score from trust
    let trust_score = ku.trust_score() as f64 / 10_000.0;

    // Scope penalty: local results are more trusted than remote
    let scope_penalty = match scope {
        QueryScope::Local => 1.0,
        QueryScope::Neighbors => 0.95,
        QueryScope::Cluster => 0.90,
        QueryScope::Dht => 0.85,
        QueryScope::Semantic => 0.80,
        QueryScope::Global => 0.70,
    };

    // Confidence factor
    let confidence = ku.confidence() as f64 / 10_000.0;

    (trust_score * 0.6 + confidence * 0.4) * scope_penalty
}

// ═══════════════════════════════════════════════════════════════════════════
// Tests
// ═══════════════════════════════════════════════════════════════════════════

#[cfg(test)]
mod tests {
    use super::*;
    use ku_core::core_dna::{CoreDna, CoreDnaHeader, Instruction};
    use ku_core::{Epigenetics, KuRuntime};

    fn make_ku(trust_score: u16, concept_id: u64) -> KuRuntime {
        let dna = CoreDna {
            header: CoreDnaHeader {
                version: 2,
                gene_type: 0,
                has_concept_table: false,
            },
            concept_table: Vec::new(),
            instructions: vec![
                Instruction::Triple {
                    s: concept_id,
                    p: 133,
                    o: 132,
                },
                Instruction::Certainty { level: 9500 },
            ],
        };
        let mut ku = KuRuntime::from_dna(dna).unwrap();
        ku.epi = Epigenetics::with_trust(trust_score, 8000);
        ku
    }

    #[test]
    fn test_merger_basic() {
        let query_id = [0x42; 16];
        let mut merger = ResultMerger::new(query_id, 10);

        merger.add_results(vec![make_ku(9000, 1), make_ku(5000, 2)], QueryScope::Local);

        assert_eq!(merger.result_count(), 2);
        assert_eq!(merger.responses_received(), 1);
    }

    #[test]
    fn test_merger_dedup() {
        let query_id = [0x42; 16];
        let mut merger = ResultMerger::new(query_id, 10);

        let ku = make_ku(9000, 42);
        // Add same KU from two different scopes
        merger.add_results(vec![ku.clone()], QueryScope::Local);
        merger.add_results(vec![ku.clone()], QueryScope::Neighbors);

        // Should be 1 unique result with source_count = 2
        assert_eq!(merger.result_count(), 1);
        assert_eq!(merger.responses_received(), 2);

        let results = merger.finalize();
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].source_count, 2);
    }

    #[test]
    fn test_merger_ranking() {
        let query_id = [0x42; 16];
        let mut merger = ResultMerger::new(query_id, 10);

        merger.add_results(
            vec![
                make_ku(3000, 1), // Low trust
                make_ku(9000, 2), // High trust
                make_ku(6000, 3), // Medium trust
            ],
            QueryScope::Local,
        );

        let results = merger.finalize();
        assert_eq!(results.len(), 3);
        // Highest trust should be first
        assert!(results[0].score >= results[1].score);
        assert!(results[1].score >= results[2].score);
    }

    #[test]
    fn test_merger_has_enough() {
        let query_id = [0x42; 16];
        let mut merger = ResultMerger::new(query_id, 2);

        merger.add_results(vec![make_ku(9000, 1), make_ku(5000, 2)], QueryScope::Local);

        assert!(merger.has_enough());
    }

    #[test]
    fn test_merger_max_results() {
        let query_id = [0x42; 16];
        let mut merger = ResultMerger::new(query_id, 2);

        merger.add_results(
            vec![
                make_ku(9000, 1),
                make_ku(8000, 2),
                make_ku(7000, 3),
                make_ku(6000, 4),
            ],
            QueryScope::Local,
        );

        let results = merger.finalize();
        assert_eq!(results.len(), 2); // Truncated to max
    }

    #[test]
    fn test_scope_penalty() {
        let ku = make_ku(9000, 1);
        let local_score = compute_score(&ku, &QueryScope::Local);
        let global_score = compute_score(&ku, &QueryScope::Global);
        assert!(
            local_score > global_score,
            "Local results should score higher"
        );
    }

    #[test]
    fn test_finalized_no_more_adds() {
        let query_id = [0x42; 16];
        let mut merger = ResultMerger::new(query_id, 10);

        merger.add_results(vec![make_ku(9000, 1)], QueryScope::Local);
        let _ = merger.finalize();

        // After finalize, new results should be ignored
        merger.add_results(vec![make_ku(5000, 2)], QueryScope::Neighbors);
        assert_eq!(merger.result_count(), 1); // Still 1, new result ignored
    }
}

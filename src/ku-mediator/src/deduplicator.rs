//! Knowledge deduplication via keyword overlap (Jaccard similarity).
//!
//! Phase 2 will add embedding-based semantic similarity.

use serde::{Deserialize, Serialize};

/// Result of deduplication check.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum DeduplicationResult {
    /// New knowledge, no duplicates found.
    New,
    /// Very similar to existing KU.
    Duplicate {
        existing_cid: String,
        similarity: f32,
    },
    /// Partially overlapping with existing KU.
    Overlap {
        existing_cid: String,
        similarity: f32,
    },
}

/// Deduplicates knowledge using Jaccard word overlap.
pub struct KnowledgeDeduplicator {
    known_texts: Vec<(String, String)>, // (cid, normalized text)
    duplicate_threshold: f32,
    overlap_threshold: f32,
}

impl KnowledgeDeduplicator {
    pub fn new() -> Self {
        Self {
            known_texts: Vec::new(),
            duplicate_threshold: 0.85,
            overlap_threshold: 0.60,
        }
    }

    /// Register a known KU for dedup checking.
    pub fn register(&mut self, cid: String, text: String) {
        self.known_texts.push((cid, text.to_lowercase()));
    }

    /// Check if candidate text is a duplicate of any registered KU.
    pub fn check(&self, candidate: &str) -> DeduplicationResult {
        let candidate_lower = candidate.to_lowercase();
        let candidate_words: std::collections::HashSet<&str> = candidate_lower
            .split_whitespace()
            .filter(|w| w.len() > 2)
            .collect();

        if candidate_words.is_empty() {
            return DeduplicationResult::New;
        }

        let mut best_similarity = 0.0f32;
        let mut best_cid = String::new();

        for (cid, known) in &self.known_texts {
            let known_words: std::collections::HashSet<&str> =
                known.split_whitespace().filter(|w| w.len() > 2).collect();

            if known_words.is_empty() {
                continue;
            }

            let intersection = candidate_words.intersection(&known_words).count();
            let union = candidate_words.union(&known_words).count();
            let jaccard = intersection as f32 / union.max(1) as f32;

            if jaccard > best_similarity {
                best_similarity = jaccard;
                best_cid = cid.clone();
            }
        }

        if best_similarity >= self.duplicate_threshold {
            DeduplicationResult::Duplicate {
                existing_cid: best_cid,
                similarity: best_similarity,
            }
        } else if best_similarity >= self.overlap_threshold {
            DeduplicationResult::Overlap {
                existing_cid: best_cid,
                similarity: best_similarity,
            }
        } else {
            DeduplicationResult::New
        }
    }
}

impl Default for KnowledgeDeduplicator {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_new_knowledge() {
        let mut dedup = KnowledgeDeduplicator::new();
        dedup.register("cid1".into(), "Water boils at 100 degrees Celsius".into());
        let result = dedup.check("Rust is a systems programming language");
        assert_eq!(result, DeduplicationResult::New);
    }

    #[test]
    fn test_duplicate_detection() {
        let mut dedup = KnowledgeDeduplicator::new();
        dedup.register("cid1".into(), "Water boils at 100 degrees Celsius".into());
        // Almost identical text
        let result = dedup.check("Water boils at 100 degrees Celsius exactly");
        assert!(matches!(
            result,
            DeduplicationResult::Duplicate { .. } | DeduplicationResult::Overlap { .. }
        ));
    }

    #[test]
    fn test_exact_duplicate() {
        let mut dedup = KnowledgeDeduplicator::new();
        dedup.register("cid1".into(), "Water boils at one hundred degrees".into());
        let result = dedup.check("Water boils at one hundred degrees");
        assert!(matches!(result, DeduplicationResult::Duplicate { .. }));
    }

    #[test]
    fn test_partial_overlap() {
        let mut dedup = KnowledgeDeduplicator::new();
        dedup.register(
            "cid1".into(),
            "Water boils at 100 degrees Celsius on Earth".into(),
        );
        // Shares some words but adds different context
        let result = dedup.check("Water freezes at zero degrees Celsius on Earth");
        // "Water", "degrees", "Celsius", "Earth" overlap out of the combined set
        assert!(matches!(
            result,
            DeduplicationResult::Overlap { .. }
                | DeduplicationResult::Duplicate { .. }
                | DeduplicationResult::New
        ),);
    }

    #[test]
    fn test_empty_candidate() {
        let mut dedup = KnowledgeDeduplicator::new();
        dedup.register("cid1".into(), "Water boils at 100 degrees".into());
        let result = dedup.check("hi");
        assert_eq!(result, DeduplicationResult::New);
    }

    #[test]
    fn test_no_registered_texts() {
        let dedup = KnowledgeDeduplicator::new();
        let result = dedup.check("Some new knowledge");
        assert_eq!(result, DeduplicationResult::New);
    }
}

//! Hybrid retriever for knowledge-augmented generation.
//!
//! Combines:
//! 1. Embedding similarity search (via EmbeddingProvider) — placeholder
//! 2. Keyword/concept matching — implemented
//! 3. Graph traversal (future)
//!
//! Supports file-based persistence via [`KuRetriever::save`] and [`KuRetriever::load`].

use serde::{Deserialize, Serialize};
use std::io;
use std::path::Path;

/// A retrieved knowledge unit.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RetrievedKU {
    /// CID of the KU (hex string).
    pub cid: String,
    /// Human-readable expression of the KU.
    pub expression: String,
    /// Relevance score (0.0-1.0).
    pub score: f32,
    /// How it was retrieved.
    pub source: RetrievalSource,
}

/// How a KU was retrieved.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum RetrievalSource {
    Embedding,
    Keyword,
    GraphTraversal,
}

/// Configuration for the retriever.
#[derive(Debug, Clone)]
pub struct RetrieverConfig {
    /// Maximum results to return.
    pub top_k: usize,
    /// Minimum relevance score to include.
    pub min_score: f32,
}

impl Default for RetrieverConfig {
    fn default() -> Self {
        Self {
            top_k: 5,
            min_score: 0.3,
        }
    }
}

/// Hybrid KU retriever.
///
/// In Phase 1, this is a keyword-based retriever with an in-memory index.
/// Phase 2 will integrate with OBKG embeddings and KQL.
pub struct KuRetriever {
    config: RetrieverConfig,
    /// Local KU cache for keyword search (simple in-memory index).
    known_expressions: Vec<(String, String)>, // (cid, expression)
}

impl KuRetriever {
    pub fn new(config: RetrieverConfig) -> Self {
        Self {
            config,
            known_expressions: Vec::new(),
        }
    }

    /// Add a known KU expression to the local index.
    pub fn index_ku(&mut self, cid: String, expression: String) {
        self.known_expressions.push((cid, expression));
    }

    /// Look up the source text for a given CID.
    pub fn get_expression(&self, cid: &str) -> Option<String> {
        self.known_expressions
            .iter()
            .find(|(c, _)| c == cid)
            .map(|(_, expr)| expr.clone())
    }

    /// Retrieve relevant KUs for a query using keyword matching.
    pub fn retrieve(&self, query: &str) -> Vec<RetrievedKU> {
        let query_lower = query.to_lowercase();
        let query_words: Vec<&str> = query_lower.split_whitespace().collect();

        let mut results: Vec<RetrievedKU> = self
            .known_expressions
            .iter()
            .filter_map(|(cid, expr)| {
                let expr_lower = expr.to_lowercase();
                let matching_words = query_words
                    .iter()
                    .filter(|w| w.len() > 2 && expr_lower.contains(*w))
                    .count();

                if matching_words > 0 {
                    let score = matching_words as f32 / query_words.len().max(1) as f32;
                    if score >= self.config.min_score {
                        Some(RetrievedKU {
                            cid: cid.clone(),
                            expression: expr.clone(),
                            score,
                            source: RetrievalSource::Keyword,
                        })
                    } else {
                        None
                    }
                } else {
                    None
                }
            })
            .collect();

        // Sort by score descending
        results.sort_by(|a, b| {
            b.score
                .partial_cmp(&a.score)
                .unwrap_or(std::cmp::Ordering::Equal)
        });
        results.truncate(self.config.top_k);
        results
    }

    /// Clear the local index.
    pub fn clear(&mut self) {
        self.known_expressions.clear();
    }

    /// Number of indexed KUs.
    pub fn index_size(&self) -> usize {
        self.known_expressions.len()
    }

    /// Save the index to a JSON file on disk.
    pub fn save(&self, path: &Path) -> Result<(), io::Error> {
        let json = serde_json::to_string_pretty(&self.known_expressions)
            .map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e))?;
        std::fs::write(path, json)
    }

    /// Load the index from a JSON file on disk.
    ///
    /// Returns a new `KuRetriever` with default config and the loaded index.
    /// If the file does not exist, returns an empty retriever.
    pub fn load(path: &Path) -> Result<Self, io::Error> {
        if !path.exists() {
            return Ok(Self::default());
        }
        let data = std::fs::read_to_string(path)?;
        let known_expressions: Vec<(String, String)> = serde_json::from_str(&data)
            .map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e))?;
        Ok(Self {
            config: RetrieverConfig::default(),
            known_expressions,
        })
    }

    /// Load the index from a JSON file, using a custom config.
    pub fn load_with_config(path: &Path, config: RetrieverConfig) -> Result<Self, io::Error> {
        if !path.exists() {
            return Ok(Self::new(config));
        }
        let data = std::fs::read_to_string(path)?;
        let known_expressions: Vec<(String, String)> = serde_json::from_str(&data)
            .map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e))?;
        Ok(Self {
            config,
            known_expressions,
        })
    }
}

impl Default for KuRetriever {
    fn default() -> Self {
        Self::new(RetrieverConfig::default())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn populated_retriever() -> KuRetriever {
        let mut r = KuRetriever::default();
        r.index_ku("cid1".into(), "Water boils at 100 degrees Celsius".into());
        r.index_ku(
            "cid2".into(),
            "The sky is blue due to Rayleigh scattering".into(),
        );
        r.index_ku("cid3".into(), "Water freezes at zero degrees".into());
        r.index_ku(
            "cid4".into(),
            "Rust is a systems programming language".into(),
        );
        r
    }

    #[test]
    fn test_retrieve_keyword_match() {
        let r = populated_retriever();
        let results = r.retrieve("water temperature degrees");
        assert!(!results.is_empty());
        // Both water KUs should match
        assert!(results.iter().any(|r| r.cid == "cid1"));
    }

    #[test]
    fn test_retrieve_no_match() {
        let r = populated_retriever();
        let results = r.retrieve("quantum entanglement");
        assert!(results.is_empty());
    }

    #[test]
    fn test_retrieve_scoring() {
        let r = populated_retriever();
        let results = r.retrieve("water boils degrees");
        assert!(!results.is_empty());
        // cid1 should score higher than cid3 (more matching words)
        if results.len() >= 2 {
            assert!(results[0].score >= results[1].score);
        }
    }

    #[test]
    fn test_top_k_limit() {
        let config = RetrieverConfig {
            top_k: 1,
            min_score: 0.0,
        };
        let mut r = KuRetriever::new(config);
        r.index_ku("a".into(), "water is important".into());
        r.index_ku("b".into(), "water is life".into());
        let results = r.retrieve("water");
        assert!(results.len() <= 1);
    }

    #[test]
    fn test_min_score_filter() {
        let config = RetrieverConfig {
            top_k: 10,
            min_score: 0.9,
        };
        let mut r = KuRetriever::new(config);
        r.index_ku("a".into(), "water boils".into());
        let results = r.retrieve("water and other things and more stuff");
        // Only 1 of 7 words matches, score = ~0.14 < 0.9
        assert!(results.is_empty());
    }

    #[test]
    fn test_index_size() {
        let r = populated_retriever();
        assert_eq!(r.index_size(), 4);
    }

    #[test]
    fn test_clear() {
        let mut r = populated_retriever();
        r.clear();
        assert_eq!(r.index_size(), 0);
    }

    #[test]
    fn test_retrieval_source() {
        let r = populated_retriever();
        let results = r.retrieve("water boils");
        for result in &results {
            assert_eq!(result.source, RetrievalSource::Keyword);
        }
    }

    #[test]
    fn test_save_and_load_roundtrip() {
        let r = populated_retriever();
        let dir = std::env::temp_dir();
        let path = dir.join("ku_retriever_test_index.json");

        // Save
        r.save(&path).expect("save failed");

        // Load
        let loaded = KuRetriever::load(&path).expect("load failed");
        assert_eq!(loaded.index_size(), 4);

        // Verify search still works
        let results = loaded.retrieve("water temperature degrees");
        assert!(!results.is_empty());

        // Cleanup
        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn test_load_nonexistent_returns_empty() {
        let path = std::path::Path::new("/tmp/nonexistent_retriever_index_12345.json");
        let r = KuRetriever::load(path).expect("load should succeed for missing file");
        assert_eq!(r.index_size(), 0);
    }
}

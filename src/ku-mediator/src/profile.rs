//! Local user profile — privacy-preserving personalization.
//!
//! Tracks user preferences, expertise areas, concept frequency,
//! and interaction statistics. All data stays local.

use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// User profile for personalization.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UserProfile {
    pub display_name: String,
    pub preferred_language: String,
    pub response_style: ResponseStyle,
    pub proactive_encoding: bool,
    pub total_kus_encoded: u64,
    pub total_queries: u64,
    pub expertise_areas: Vec<ExpertiseArea>,
    pub concept_frequency: HashMap<String, u32>,
    pub created_at: u64,
    pub last_active: u64,
}

/// How verbose/detailed the AI's responses should be.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum ResponseStyle {
    Concise,
    Balanced,
    Detailed,
    Academic,
}

/// A domain the user has expertise in.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExpertiseArea {
    pub domain: String,
    pub ku_count: u32,
    pub last_active: u64,
}

impl UserProfile {
    pub fn new(name: impl Into<String>) -> Self {
        let now = current_timestamp_ms();
        Self {
            display_name: name.into(),
            preferred_language: "en".to_string(),
            response_style: ResponseStyle::Balanced,
            proactive_encoding: false,
            total_kus_encoded: 0,
            total_queries: 0,
            expertise_areas: Vec::new(),
            concept_frequency: HashMap::new(),
            created_at: now,
            last_active: now,
        }
    }

    /// Record an encoding event and update concept frequencies.
    pub fn record_encoding(&mut self, concepts: &[String]) {
        self.total_kus_encoded += 1;
        self.last_active = current_timestamp_ms();
        for concept in concepts {
            *self.concept_frequency.entry(concept.clone()).or_insert(0) += 1;
        }
    }

    /// Record a query event.
    pub fn record_query(&mut self) {
        self.total_queries += 1;
        self.last_active = current_timestamp_ms();
    }

    /// Generate context block for system prompt injection.
    pub fn to_context_block(&self) -> String {
        format!(
            "User: {}\nLanguage: {}\nStyle: {:?}\nKUs encoded: {}\nQueries: {}",
            self.display_name,
            self.preferred_language,
            self.response_style,
            self.total_kus_encoded,
            self.total_queries
        )
    }

    /// Top expertise areas by KU count.
    pub fn top_expertise(&self, n: usize) -> Vec<&ExpertiseArea> {
        let mut areas: Vec<&ExpertiseArea> = self.expertise_areas.iter().collect();
        areas.sort_by_key(|area| std::cmp::Reverse(area.ku_count));
        areas.truncate(n);
        areas
    }

    /// Save profile to JSON file.
    pub fn save(&self, path: &std::path::Path) -> Result<(), std::io::Error> {
        let json = serde_json::to_string_pretty(self).map_err(std::io::Error::other)?;
        std::fs::write(path, json)
    }

    /// Load profile from JSON file.
    pub fn load(path: &std::path::Path) -> Result<Self, std::io::Error> {
        let json = std::fs::read_to_string(path)?;
        serde_json::from_str(&json).map_err(std::io::Error::other)
    }
}

impl Default for UserProfile {
    fn default() -> Self {
        Self::new("User")
    }
}

fn current_timestamp_ms() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as u64
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_new_profile() {
        let profile = UserProfile::new("Alice");
        assert_eq!(profile.display_name, "Alice");
        assert_eq!(profile.preferred_language, "en");
        assert_eq!(profile.response_style, ResponseStyle::Balanced);
        assert_eq!(profile.total_kus_encoded, 0);
    }

    #[test]
    fn test_record_encoding() {
        let mut profile = UserProfile::new("Bob");
        profile.record_encoding(&["physics".to_string(), "water".to_string()]);
        assert_eq!(profile.total_kus_encoded, 1);
        assert_eq!(profile.concept_frequency.get("physics"), Some(&1));

        profile.record_encoding(&["physics".to_string()]);
        assert_eq!(profile.total_kus_encoded, 2);
        assert_eq!(profile.concept_frequency.get("physics"), Some(&2));
    }

    #[test]
    fn test_record_query() {
        let mut profile = UserProfile::new("Carol");
        profile.record_query();
        profile.record_query();
        assert_eq!(profile.total_queries, 2);
    }

    #[test]
    fn test_context_block() {
        let mut profile = UserProfile::new("Dave");
        profile.total_kus_encoded = 42;
        profile.total_queries = 100;
        let block = profile.to_context_block();
        assert!(block.contains("Dave"));
        assert!(block.contains("42"));
        assert!(block.contains("100"));
        assert!(block.contains("Balanced"));
    }

    #[test]
    fn test_top_expertise() {
        let mut profile = UserProfile::new("Eve");
        profile.expertise_areas = vec![
            ExpertiseArea {
                domain: "physics".into(),
                ku_count: 50,
                last_active: 0,
            },
            ExpertiseArea {
                domain: "biology".into(),
                ku_count: 30,
                last_active: 0,
            },
            ExpertiseArea {
                domain: "chemistry".into(),
                ku_count: 80,
                last_active: 0,
            },
        ];
        let top = profile.top_expertise(2);
        assert_eq!(top.len(), 2);
        assert_eq!(top[0].domain, "chemistry");
        assert_eq!(top[1].domain, "physics");
    }

    #[test]
    fn test_json_roundtrip() {
        let mut profile = UserProfile::new("Frank");
        profile.record_encoding(&["rust".to_string()]);
        profile.record_query();

        let json = serde_json::to_string(&profile).unwrap();
        let restored: UserProfile = serde_json::from_str(&json).unwrap();
        assert_eq!(restored.display_name, "Frank");
        assert_eq!(restored.total_kus_encoded, 1);
        assert_eq!(restored.total_queries, 1);
    }

    #[test]
    fn test_save_and_load() {
        let dir = std::env::temp_dir();
        let path = dir.join("test_profile_ku_mediator.json");

        let mut profile = UserProfile::new("Grace");
        profile.record_encoding(&["test".to_string()]);
        profile.save(&path).unwrap();

        let loaded = UserProfile::load(&path).unwrap();
        assert_eq!(loaded.display_name, "Grace");
        assert_eq!(loaded.total_kus_encoded, 1);

        // Cleanup
        let _ = std::fs::remove_file(&path);
    }
}

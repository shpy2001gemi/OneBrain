//! User profile and node configuration views for [`OneBrainNode`].

use super::*;

impl OneBrainNode {
    // ═══════════════════════════════════════════════════════
    // Step 4: Profile
    // ═══════════════════════════════════════════════════════

    /// Get user profile.
    pub fn get_profile(&self) -> Result<UserProfileView, NodeError> {
        let profile = self.mediator.profile();
        Ok(UserProfileView {
            name: profile.display_name.clone(),
            language: profile.preferred_language.clone(),
            style: format!("{:?}", profile.response_style),
            expertise: profile
                .top_expertise(5)
                .iter()
                .map(|e| ExpertiseView {
                    domain: e.domain.clone(),
                    ku_count: e.ku_count as u64,
                    last_active: e.last_active,
                })
                .collect(),
            total_kus: self.ku_count().unwrap_or(0) as u64,
            total_queries: profile.total_queries,
            member_since: profile.created_at,
        })
    }

    /// Update a profile field.
    pub fn update_profile(&mut self, field: &str, value: &str) -> Result<(), NodeError> {
        use ku_mediator::profile::ResponseStyle;
        let profile = self.mediator.profile_mut();
        match field {
            "name" | "display_name" => profile.display_name = value.to_string(),
            "language" => profile.preferred_language = value.to_string(),
            "style" | "response_style" => {
                profile.response_style = match value {
                    "concise" => ResponseStyle::Concise,
                    "balanced" => ResponseStyle::Balanced,
                    "detailed" => ResponseStyle::Detailed,
                    "academic" => ResponseStyle::Academic,
                    _ => {
                        return Err(NodeError::InvalidArgument(format!(
                            "Invalid style: '{}'. Options: concise, balanced, detailed, academic",
                            value
                        )))
                    }
                };
            }
            _ => {
                return Err(NodeError::InvalidArgument(format!(
                    "Unknown profile field: '{}'. Fields: name, language, style",
                    field
                )))
            }
        }
        profile
            .save(&self.config.profile_path())
            .map_err(|e| NodeError::Storage(format!("Failed to save profile: {}", e)))?;
        Ok(())
    }

    // ═══════════════════════════════════════════════════════
    // Step 5: Config
    // ═══════════════════════════════════════════════════════

    /// Get configuration as a view.
    pub fn get_config_view(&self) -> ConfigView {
        ConfigView {
            name: self.config.name.clone(),
            port: self.config.port,
            data_dir: self.config.data_dir.display().to_string(),
            ollama_url: self.config.ollama_url.clone(),
            model: self.config.model.clone(),
            seeds: self.config.seeds.iter().map(|s| s.to_string()).collect(),
            identity_path: self.config.identity_path().display().to_string(),
            storage_path: self.config.storage_path().display().to_string(),
            profile_path: self.config.profile_path().display().to_string(),
            peers_path: self.config.peer_memory_path().display().to_string(),
        }
    }

    /// Update a config value.
    pub fn update_config(&mut self, key: &str, value: &str) -> Result<(), NodeError> {
        match key {
            "name" => self.config.name = value.to_string(),
            "port" => {
                self.config.port = value
                    .parse::<u16>()
                    .map_err(|_| NodeError::InvalidArgument(format!("Invalid port: {}", value)))?;
            }
            "ollama_url" => self.config.ollama_url = value.to_string(),
            "model" => self.config.model = value.to_string(),
            _ => {
                return Err(NodeError::InvalidArgument(format!(
                    "Unknown config key: '{}'. Keys: name, port, ollama_url, model",
                    key
                )))
            }
        }
        Ok(())
    }
}

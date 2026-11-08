//! Chat/mediator and AI model selection for [`OneBrainNode`].
//!
//! Backend construction stays in `crate::model_host`; this module only routes
//! node calls to the mediator and records the selected model.

use super::*;

impl OneBrainNode {
    /// Process user input through the mediator pipeline and return the full response.
    pub async fn process_input_response(
        &mut self,
        input: &str,
    ) -> Result<ku_mediator::MediatorResponse, NodeError> {
        let user_input = UserInput::Text(input.to_string());
        self.mediator
            .process(user_input)
            .await
            .map_err(NodeError::Mediator)
    }

    /// Process user input through the mediator pipeline.
    pub async fn process_input(&mut self, input: &str) -> Result<String, NodeError> {
        Ok(self.process_input_response(input).await?.text)
    }

    /// List available AI models.
    pub fn list_ai_models(&self) -> Result<Vec<ModelInfo>, NodeError> {
        // Read from ku-ai registry
        let current_model = self.config.model.clone();
        let models = vec![ModelInfo {
            name: current_model.clone(),
            params: "current".to_string(),
            is_current: true,
            is_installed: true,
        }];
        // TODO: Query Ollama API for installed models
        // GET http://localhost:11434/api/tags
        Ok(models)
    }

    /// Switch the active AI model.
    ///
    /// Creates new AI backends with the new model and replaces them in the Mediator,
    /// so chat and encoding will use the new model immediately.
    pub fn switch_model(&mut self, model_name: &str) -> Result<(), NodeError> {
        // Create new backends with the new model
        let new_chat_backend = model_host::model_backend(&self.config.ollama_url, model_name, 120)?;

        let new_encoder_backend =
            model_host::model_backend(&self.config.ollama_url, model_name, 120)?;

        // Replace backends in mediator
        self.mediator
            .replace_backends(new_chat_backend, new_encoder_backend);

        // Update config
        self.config.model = model_name.to_string();

        // Also update config in shared state
        if let Ok(mut state) = self.shared.try_lock() {
            state.config.model = model_name.to_string();
        }

        Ok(())
    }

    /// Test AI connection.
    pub async fn test_ai_connection(&self) -> Result<AiHealthInfo, NodeError> {
        let start = std::time::Instant::now();

        // Simple TCP check to Ollama
        let addr = self
            .config
            .ollama_url
            .replace("http://", "")
            .replace("https://", "");

        match tokio::net::TcpStream::connect(&addr).await {
            Ok(_) => {
                let latency = start.elapsed().as_millis() as u64;
                Ok(AiHealthInfo {
                    connected: true,
                    model: self.config.model.clone(),
                    ollama_url: self.config.ollama_url.clone(),
                    latency_ms: latency,
                    status_message: format!("Connected ({}ms)", latency),
                })
            }
            Err(e) => Ok(AiHealthInfo {
                connected: false,
                model: self.config.model.clone(),
                ollama_url: self.config.ollama_url.clone(),
                latency_ms: 0,
                status_message: format!("Connection failed: {}", e),
            }),
        }
    }
}

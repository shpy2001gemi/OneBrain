use super::*;
use async_trait::async_trait;
use ku_ai::backend::OllamaBackend;
use ku_ai::types::{ChatMessage, InferenceOptions};
use serde_json::Value;

/// Exact pinned adapter identity and resource declaration, validated by the host
/// against the reviewed ProviderManifest schema before dispatch.
pub struct ProviderRequest {
    pub input: Value,
    pub repair_errors: Vec<&'static str>,
    pub deadline: Duration,
    pub output_tokens: u32,
    pub max_response_bytes: usize,
}

#[async_trait]
pub trait ExtractionProvider: Send + Sync {
    fn manifest(&self) -> &Value;
    /// Includes schema, prompt, examples, repair errors and backend chat wrapper.
    /// The host must supply a tokenizer bound to the manifest's tokenizer hash.
    fn input_tokens(&self, request: &ProviderRequest) -> Result<u32>;
    async fn extract(&self, request: ProviderRequest) -> Result<Vec<u8>>;
}

/// Adapter-owned tokenizer integration. An unavailable tokenizer is a dependency
/// error, never chars/4 or a provider-controlled underestimate.
pub trait ExtractionTokenizer: Send + Sync {
    fn artifact_sha256(&self) -> [u8; 32];
    fn count_chat(&self, messages: &[ChatMessage]) -> Result<u32>;
}

pub struct OllamaExtractionProvider {
    backend: OllamaBackend,
    manifest: Value,
    tokenizer: Arc<dyn ExtractionTokenizer>,
}

impl OllamaExtractionProvider {
    pub fn new(
        backend: OllamaBackend,
        manifest: Value,
        tokenizer: Arc<dyn ExtractionTokenizer>,
    ) -> Result<Self> {
        let mut budget = WorkBudget::new(
            1_000_000,
            Duration::from_secs(1),
            Arc::new(AtomicBool::new(false)),
        )?;
        check(&manifest, "ProviderManifest", &mut budget)?;
        require(
            manifest["mode"] == "json_schema" && manifest["tools_enabled"] == false,
            "provider_mode",
        )?;
        require(
            manifest["model_artifact_sha256"].is_string()
                && manifest["tokenizer_sha256"].as_str()
                    == Some(hex(&tokenizer.artifact_sha256()).as_str()),
            "provider_identity",
        )?;
        Ok(Self {
            backend,
            manifest,
            tokenizer,
        })
    }
    fn messages(&self, request: &ProviderRequest) -> Result<Vec<ChatMessage>> {
        let schema =
            include_str!("../../../../docs/specs/vnext/ku-encoder-v1/candidate.schema.json");
        let prompt = include_str!("../../../../docs/specs/vnext/ku-encoder-v1/prompt.en.txt");
        let examples = include_str!("../../../../docs/specs/vnext/ku-encoder-v1/examples.json");
        let context =
            serde_json::to_string(&request.input).map_err(|_| ExtractionError("invalid_json"))?;
        let errors = serde_json::to_string(&request.repair_errors)
            .map_err(|_| ExtractionError("invalid_json"))?;
        require(
            prompt.len() + schema.len() + examples.len() + context.len() + errors.len()
                <= 1_048_576,
            "payload_bytes",
        )?;
        Ok(vec![
            ChatMessage::system(prompt),
            ChatMessage::user(format!(
                "SCHEMA\n{schema}\nEXAMPLE\n{examples}\nCONTEXT\n{context}\nERRORS\n{errors}"
            )),
        ])
    }
}

#[async_trait]
impl ExtractionProvider for OllamaExtractionProvider {
    fn manifest(&self) -> &Value {
        &self.manifest
    }
    fn input_tokens(&self, request: &ProviderRequest) -> Result<u32> {
        self.tokenizer.count_chat(&self.messages(request)?)
    }
    async fn extract(&self, request: ProviderRequest) -> Result<Vec<u8>> {
        let messages = self.messages(&request)?;
        let schema = serde_json::from_str(include_str!(
            "../../../../docs/specs/vnext/ku-encoder-v1/candidate.schema.json"
        ))
        .map_err(|_| ExtractionError("schema"))?;
        let options = InferenceOptions {
            temperature: self.manifest["temperature_milli"].as_u64().unwrap_or(0) as f32 / 1000.0,
            max_tokens: Some(request.output_tokens),
            seed: self.manifest["seed"].as_u64(),
            ..Default::default()
        };
        self.backend
            .chat_structured_bounded(
                &messages,
                &schema,
                &options,
                request.max_response_bytes,
                request.deadline,
            )
            .await
            .map_err(|_| ExtractionError("provider_failed"))
    }
}

//! Experimental local Qwen3 provider. Admission binds installed artifacts, exact
//! raw prompt tokens and a private, killable, memory-limited CPU worker.
use super::*;
use async_trait::async_trait;
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::{
    fs::File,
    io::{Read, Seek, SeekFrom},
    path::{Path, PathBuf},
};
use tokio::sync::Semaphore;

pub struct ManagedOllamaProvider {
    model: String,
    exe: PathBuf,
    models: PathBuf,
    manifest: Value,
    tokenizer: tokenizers::Tokenizer,
    memory: u64,
    gate: Arc<Semaphore>,
    _locks: Vec<File>,
}

// Preserve reviewed schema property order on the inference wire. Ordinary Value
// uses sorted maps, which can change grammar decoding on installed Ollama builds.
// Canonical hashing and host validation continue to use ordinary sorted Values.
#[derive(serde::Serialize)]
struct GenerateBody {
    #[serde(flatten)]
    fields: Value,
    format: Box<serde_json::value::RawValue>,
}
#[cfg(test)]
fn generate_body(fields: Value) -> Result<GenerateBody> {
    let format = serde_json::from_str(include_str!(
        "../../../../docs/specs/vnext/ku-encoder-v1/candidate.schema.json"
    ))
    .map_err(|_| ExtractionError("schema"))?;
    Ok(GenerateBody { fields, format })
}
fn open(path: &Path) -> Result<File> {
    let mut options = std::fs::OpenOptions::new();
    options.read(true);
    #[cfg(windows)]
    {
        use std::os::windows::fs::OpenOptionsExt;
        options.share_mode(1); // FILE_SHARE_READ: refuse writes/deletion while admitted.
    }
    options
        .open(path)
        .map_err(|_| ExtractionError("model_artifacts_unavailable"))
}
fn digest(file: &mut File) -> Result<String> {
    file.seek(SeekFrom::Start(0))
        .map_err(|_| ExtractionError("model_artifacts_unavailable"))?;
    let mut hash = Sha256::new();
    let mut bytes = vec![0; 1024 * 1024];
    loop {
        let n = file
            .read(&mut bytes)
            .map_err(|_| ExtractionError("model_artifacts_unavailable"))?;
        if n == 0 {
            break;
        }
        hash.update(&bytes[..n]);
    }
    Ok(hex(&hash.finalize()))
}
// A lossless structural glossary of the reviewed schema. Numeric/string bounds
// remain in the full JSON schema sent to Ollama and in the host validator.
// This avoids spending most of a CPU call re-reading repetitive schema syntax.
fn schema_shape(schema: &Value) -> Result<String> {
    if let Some(v) = schema.get("const") {
        return Ok(v.to_string());
    }
    if let Some(v) = schema["$ref"].as_str() {
        return Ok(v.trim_start_matches("#/$defs/").into());
    }
    if let Some(v) = schema["enum"].as_array() {
        return Ok(v.iter().map(Value::to_string).collect::<Vec<_>>().join("|"));
    }
    if let Some(v) = schema["oneOf"].as_array() {
        return Ok(v
            .iter()
            .map(schema_shape)
            .collect::<Result<Vec<_>>>()?
            .join("|"));
    }
    match schema["type"].as_str() {
        Some("object") => {
            let fields = schema["properties"]
                .as_object()
                .ok_or(ExtractionError("schema"))?;
            let required = schema["required"]
                .as_array()
                .ok_or(ExtractionError("schema"))?;
            Ok(format!(
                "{{{}}}",
                fields
                    .iter()
                    .map(|(k, v)| Ok(format!(
                        "{k}{}:{}",
                        if required.contains(&json!(k)) {
                            ""
                        } else {
                            "?"
                        },
                        schema_shape(v)?
                    )))
                    .collect::<Result<Vec<_>>>()?
                    .join(",")
            ))
        }
        Some("array") => Ok(format!("[{}]", schema_shape(&schema["items"])?)),
        Some("string" | "boolean" | "integer") => Ok(schema["type"].as_str().unwrap().into()),
        _ => Err(ExtractionError("schema")),
    }
}

// Position hints only: parsing a substring does not establish its semantic role.
fn numeric_spans(input: &Value) -> Result<Vec<Value>> {
    let numbers = regex::Regex::new(r"-?[0-9]+(?:[.,/][0-9]+)*(?:[eE][+-]?[0-9]+)?")
        .map_err(|_| ExtractionError("schema"))?;
    let mut spans = Vec::new();
    for window in input["windows"]
        .as_array()
        .ok_or(ExtractionError("context"))?
    {
        let text = window["text"].as_str().ok_or(ExtractionError("context"))?;
        let offset = window["start"].as_u64().ok_or(ExtractionError("context"))?;
        for number in numbers.find_iter(text) {
            let previous = text[..number.start()].chars().next_back();
            if previous
                .is_some_and(|c| c.is_ascii_digit() || ['.', ',', '/', '+', '-'].contains(&c))
                || super::compiler::exact_number(number.as_str()).is_err()
            {
                continue;
            }
            require(spans.len() < 1024, "call_tokens")?;
            spans.push(json!([
                offset + number.start() as u64,
                offset + number.end() as u64,
                number.as_str()
            ]));
        }
    }
    Ok(spans)
}
impl ManagedOllamaProvider {
    /// Host configuration, never a browser endpoint or filesystem path.
    pub fn open(
        exe: PathBuf,
        models: PathBuf,
        model: &str,
        memory: u64,
        gate: Arc<Semaphore>,
    ) -> Result<Self> {
        require(cfg!(windows), "worker_unavailable")?;
        let tag = model
            .strip_prefix("qwen3:")
            .ok_or(ExtractionError("unsupported_model"))?;
        require(
            !tag.is_empty()
                && tag.len() <= 64
                && tag
                    .bytes()
                    .all(|b| b.is_ascii_alphanumeric() || b == b'-' || b == b'.'),
            "unsupported_model",
        )?;
        require(
            (8 * 1024u64.pow(3)..=32 * 1024u64.pow(3)).contains(&memory),
            "memory_admission",
        )?;
        let exe = exe
            .canonicalize()
            .map_err(|_| ExtractionError("worker_unavailable"))?;
        let models = models
            .canonicalize()
            .map_err(|_| ExtractionError("model_artifacts_unavailable"))?;
        let mut binary = open(&exe)?;
        let binary_hash = digest(&mut binary)?;
        let mut manifest_file = open(
            &models
                .join("manifests/registry.ollama.ai/library/qwen3")
                .join(tag),
        )?;
        let mut manifest_bytes = Vec::new();
        (&mut manifest_file)
            .take(65537)
            .read_to_end(&mut manifest_bytes)
            .map_err(|_| ExtractionError("model_artifacts_unavailable"))?;
        require(manifest_bytes.len() <= 65536, "model_artifacts_unavailable")?;
        let manifest: Value = serde_json::from_slice(&manifest_bytes)
            .map_err(|_| ExtractionError("model_artifacts_unavailable"))?;
        let layers = manifest["layers"]
            .as_array()
            .ok_or(ExtractionError("model_artifacts_unavailable"))?;
        require(layers.len() <= 16, "model_artifacts_unavailable")?;
        let mut locks = vec![binary, manifest_file];
        // Ollama delegates CPU inference to the installed native runner. Pin its
        // actual DLLs as well as ollama.exe; version/tag labels are insufficient.
        let runtime_dir = exe
            .parent()
            .ok_or(ExtractionError("worker_unavailable"))?
            .join("lib/ollama");
        let mut runtime_files = std::fs::read_dir(&runtime_dir)
            .map_err(|_| ExtractionError("worker_unavailable"))?
            .map(|e| {
                e.map(|e| e.path())
                    .map_err(|_| ExtractionError("worker_unavailable"))
            })
            .collect::<Result<Vec<_>>>()?;
        require(runtime_files.len() <= 128, "worker_artifacts_limit")?;
        runtime_files.sort();
        let mut runtime_pins = serde_json::Map::new();
        let mut runtime_bytes = 0u64;
        for path in runtime_files {
            let name = path
                .file_name()
                .and_then(|s| s.to_str())
                .ok_or(ExtractionError("worker_unavailable"))?;
            if !name.ends_with(".dll") && name != "llama-server.exe" {
                continue;
            }
            let mut file = open(&path)?;
            runtime_bytes = runtime_bytes
                .checked_add(
                    file.metadata()
                        .map_err(|_| ExtractionError("worker_unavailable"))?
                        .len(),
                )
                .ok_or(ExtractionError("worker_artifacts_limit"))?;
            require(runtime_bytes <= 256 * 1024 * 1024, "worker_artifacts_limit")?;
            runtime_pins.insert(name.into(), json!(digest(&mut file)?));
            locks.push(file);
        }
        require(
            runtime_pins.contains_key("llama-server.exe")
                && runtime_pins.contains_key("ggml-base.dll"),
            "worker_unavailable",
        )?;
        let mut tokenizer = None;
        let mut weight_hash = None;
        for layer in layers.iter().chain(std::iter::once(&manifest["config"])) {
            let expected = layer["digest"]
                .as_str()
                .and_then(|s| s.strip_prefix("sha256:"))
                .ok_or(ExtractionError("model_artifacts_unavailable"))?;
            require(
                expected.len() == 64
                    && expected
                        .bytes()
                        .all(|b| b.is_ascii_hexdigit() && !b.is_ascii_uppercase()),
                "model_artifacts_unavailable",
            )?;
            let mut file = open(&models.join("blobs").join(format!("sha256-{expected}")))?;
            require(
                file.metadata()
                    .map_err(|_| ExtractionError("model_artifacts_unavailable"))?
                    .len()
                    == layer["size"].as_u64().unwrap_or(0),
                "model_artifacts_unavailable",
            )?;
            require(digest(&mut file)? == expected, "model_artifact_changed")?;
            if layer["mediaType"] == "application/vnd.ollama.image.model" {
                require(tokenizer.is_none(), "unsupported_model")?;
                tokenizer = Some(super::qwen_tokenizer::load(&mut file)?);
                weight_hash = Some(expected.to_owned());
            }
            locks.push(file);
        }
        let (tokenizer, tokenizer_hash) = tokenizer.ok_or(ExtractionError("unsupported_model"))?;
        let backend_hash = artifact_sha256(
            &json!({"binary":binary_hash,"cpu_runtime":runtime_pins,"installed_manifest":manifest,
            "provider":include_str!("managed_ollama.rs"),"semantic_guide":include_str!("ollama_semantic_guide.txt"),"worker":include_str!("ollama_worker.rs"),"tokenizer":include_str!("qwen_tokenizer.rs")}),
        )?;
        let manifest = json!({"profile":"ku-extraction-provider/1.0","provider_id":format!("experimental-ollama-{model}"),
            "backend_build_sha256":backend_hash,"mode":"json_schema","tools_enabled":false,"max_context_tokens":8192,
            "peak_bytes_reservation":memory,"schema_bundle_sha256":ExtractionWorkflow::bundle_hash(),
            "model_artifact_sha256":weight_hash,"tokenizer_sha256":tokenizer_hash,"supported_schema_keywords":[],"temperature_milli":0,"seed":1});
        Ok(Self {
            model: model.into(),
            exe,
            models,
            manifest,
            tokenizer,
            memory,
            gate,
            _locks: locks,
        })
    }
    fn prompt(&self, request: &ProviderRequest) -> Result<String> {
        let examples: Value = serde_json::from_str(include_str!(
            "../../../../docs/specs/vnext/ku-encoder-v1/examples.json"
        ))
        .map_err(|_| ExtractionError("schema"))?;
        let schema: Value = serde_json::from_str(include_str!(
            "../../../../docs/specs/vnext/ku-encoder-v1/candidate.schema.json"
        ))
        .map_err(|_| ExtractionError("schema"))?;
        let system = format!(
            "{}\n{}",
            include_str!("../../../../docs/specs/vnext/ku-encoder-v1/prompt.en.txt"),
            include_str!("ollama_semantic_guide.txt")
        );
        let glossary = schema["$defs"]
            .as_object()
            .ok_or(ExtractionError("schema"))?
            .iter()
            .map(|(k, v)| Ok(format!("{k}={}\n", schema_shape(v)?)))
            .collect::<Result<String>>()?;
        let mut spans = Vec::new();
        let words = regex::Regex::new(r"[\p{L}\p{N}]+(?:['’-][\p{L}\p{N}]+)*")
            .map_err(|_| ExtractionError("schema"))?;
        for window in request.input["windows"]
            .as_array()
            .ok_or(ExtractionError("context"))?
        {
            let text = window["text"].as_str().ok_or(ExtractionError("context"))?;
            let offset = window["start"].as_u64().ok_or(ExtractionError("context"))?;
            for word in words.find_iter(text) {
                require(spans.len() < 1024, "call_tokens")?;
                spans.push(json!([
                    offset + word.start() as u64,
                    offset + word.end() as u64,
                    word.as_str()
                ]));
            }
        }
        let user = format!(
            "SCHEMA\nReturn Candidate JSON. Structural glossary (? means optional; | means alternatives; no extra fields). Full bounds enforced by the supplied JSON schema.\n{glossary}\nEXAMPLE\n{}\nCONTEXT\n{}\nBYTE_SPANS\n{}\nThese are exact [start,end,quote] for words in CONTEXT. Copy these offsets for word evidence; whole-unit spans are already in required_units. Multiword evidence must include intervening bytes.\nNUMBER_SPANS\n{}\nExact numeric substrings only, not semantic decisions. Select number evidence separately from the unit, even when printed together. The host performs arithmetic; do not compute or normalize source numbers.\nERRORS\n{}",
            examples["examples"][0]["candidate"],
            request.input,
            json!(spans),
            json!(numeric_spans(&request.input)?),
            json!(request.repair_errors)
        );
        require(user.len() + system.len() <= 1_048_576, "payload_bytes")?;
        Ok(format!("<|im_start|>system\n{system}<|im_end|>\n<|im_start|>user\n{user}<|im_end|>\n<|im_start|>assistant\n<think>\n\n</think>\n\n"))
    }
}
#[async_trait]
impl ExtractionProvider for ManagedOllamaProvider {
    fn manifest(&self) -> &Value {
        &self.manifest
    }
    fn input_tokens(&self, request: &ProviderRequest) -> Result<u32> {
        let n = self
            .tokenizer
            .encode(self.prompt(request)?, false)
            .map_err(|_| ExtractionError("tokenizer"))?
            .len();
        u32::try_from(n).map_err(|_| ExtractionError("call_tokens"))
    }
    async fn extract(&self, request: ProviderRequest) -> Result<Vec<u8>> {
        self.generate(
            self.prompt(&request)?,
            include_str!("../../../../docs/specs/vnext/ku-encoder-v1/candidate.schema.json"),
            request.deadline,
            request.output_tokens,
            request.max_response_bytes,
        )
        .await
    }
    fn review_task_tokens(&self, request: &review_draft::TaskRequest) -> Result<u32> {
        u32::try_from(
            self.tokenizer
                .encode(request.prompt()?, false)
                .map_err(|_| ExtractionError("tokenizer"))?
                .len(),
        )
        .map_err(|_| ExtractionError("call_tokens"))
    }
    async fn review_task(&self, request: review_draft::TaskRequest) -> Result<Vec<u8>> {
        let schema = request.wire_schema()?;
        self.generate(
            request.prompt()?,
            &schema,
            request.deadline,
            2048,
            1_048_576,
        )
        .await
    }
}
impl ManagedOllamaProvider {
    async fn generate(
        &self,
        prompt: String,
        schema: &str,
        timeout: Duration,
        output_tokens: u32,
        max_response_bytes: usize,
    ) -> Result<Vec<u8>> {
        let _permit = self
            .gate
            .try_acquire()
            .map_err(|_| ExtractionError("memory_admission"))?;
        let expected_tokens = u32::try_from(
            self.tokenizer
                .encode(prompt.clone(), false)
                .map_err(|_| ExtractionError("tokenizer"))?
                .len(),
        )
        .map_err(|_| ExtractionError("call_tokens"))?;
        require(
            output_tokens <= 2048 && expected_tokens + output_tokens <= 8192,
            "call_tokens",
        )?;
        let listener = std::net::TcpListener::bind("127.0.0.1:0")
            .map_err(|_| ExtractionError("worker_unavailable"))?;
        let port = listener
            .local_addr()
            .map_err(|_| ExtractionError("worker_unavailable"))?
            .port();
        drop(listener);
        // The admission also reserves bounded host tokenizer/parser residency
        // for up to eight installed models; only the remaining RAM goes to the job.
        let _worker = super::ollama_worker::Worker::start(
            &self.exe,
            &self.models,
            port,
            self.memory - 4 * 1024u64.pow(3),
        )?;
        let client = reqwest::Client::builder()
            .no_proxy()
            .redirect(reqwest::redirect::Policy::none())
            .timeout(timeout)
            .build()
            .map_err(|_| ExtractionError("worker_unavailable"))?;
        let url = format!("http://127.0.0.1:{port}");
        let deadline = tokio::time::Instant::now() + timeout;
        tokio::time::timeout_at(deadline, async {
            let startup_deadline = tokio::time::Instant::now() + Duration::from_secs(10);
            loop {
                if client.get(format!("{url}/api/version")).timeout(Duration::from_millis(500)).send().await.is_ok_and(|r|r.status().is_success()) { break; }
                require(tokio::time::Instant::now() < startup_deadline,"worker_startup_failed")?;
                tokio::time::sleep(Duration::from_millis(100)).await;
            }
            let body = GenerateBody { fields: json!({"model":self.model,"prompt":prompt,"raw":true,"stream":false,"keep_alive":0,
                "options":{"num_ctx":8192,"num_predict":output_tokens,"temperature":0,"seed":1,"num_gpu":0}}), format: serde_json::from_str(schema).map_err(|_| ExtractionError("schema"))? };
            let mut response = client.post(format!("{url}/api/generate")).json(&body).send().await.map_err(|_|ExtractionError("provider_failed"))?;
            require(response.status().is_success(),"provider_failed")?;
            let mut bytes = Vec::new();
            while let Some(chunk) = response.chunk().await.map_err(|_|ExtractionError("provider_failed"))? {
                require(bytes.len() + chunk.len() <= max_response_bytes.min(1_048_576),"payload_bytes")?; bytes.extend_from_slice(&chunk);
            }
            let result: Value = serde_json::from_slice(&bytes).map_err(|_|ExtractionError("provider_failed"))?;
            require(result["model"] == self.model && result["done"] == true && result["done_reason"] == "stop" && result["prompt_eval_count"].as_u64() == Some(expected_tokens as u64)
                && result["eval_count"].as_u64().is_some_and(|n|n <= output_tokens as u64)
                && result.get("tool_calls").is_none(),"provider_token_binding")?;
            let output = result["response"].as_str().ok_or(ExtractionError("provider_failed"))?;
            Ok(output.as_bytes().to_vec())
        }).await.map_err(|_|ExtractionError("deadline"))?
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn numeric_hints_split_attached_units_without_rewriting_unsupported_notation() {
        let text = "Giá 12kg; 1,000; 3e2; .5; +6; -3.5m; 1/2s";
        let spans = numeric_spans(&json!({"windows":[{"start":7,"text":text}]})).unwrap();
        assert_eq!(
            spans
                .iter()
                .map(|s| s[2].as_str().unwrap())
                .collect::<Vec<_>>(),
            vec!["12", "-3.5", "1/2"]
        );
        for span in spans {
            assert_eq!(
                &text[span[0].as_u64().unwrap() as usize - 7
                    ..span[1].as_u64().unwrap() as usize - 7],
                span[2].as_str().unwrap()
            );
        }
    }
    #[test]
    fn generation_wire_retains_reviewed_schema_without_changing_its_meaning() {
        let source =
            include_str!("../../../../docs/specs/vnext/ku-encoder-v1/candidate.schema.json");
        let bytes =
            serde_json::to_string(&generate_body(json!({"model":"test","raw":true})).unwrap())
                .unwrap();
        assert!(bytes.contains(source.trim()));
        let decoded: Value = serde_json::from_str(&bytes).unwrap();
        assert_eq!(
            decoded["format"],
            serde_json::from_str::<Value>(source).unwrap()
        );
        assert_eq!(decoded["model"], "test");
        assert_eq!(decoded["raw"], true);
    }
    #[test]
    fn experimental_model_requires_valid_tag_and_existing_local_artifacts() {
        for name in [
            "qwen3:../../escape",
            "qwen3:",
            "qwen3:8b/remote",
            "gemma:8b",
            "qwen3:8b",
        ] {
            assert!(ManagedOllamaProvider::open(
                PathBuf::from("missing-ollama.exe"),
                PathBuf::from("missing-models"),
                name,
                12 * 1024u64.pow(3),
                Arc::new(Semaphore::new(1))
            )
            .is_err());
        }
    }
    #[test]
    fn glossary_retains_optional_fields_unions_and_qualifiers() {
        let schema: Value = serde_json::from_str(include_str!(
            "../../../../docs/specs/vnext/ku-encoder-v1/candidate.schema.json"
        ))
        .unwrap();
        for definition in schema["$defs"].as_object().unwrap().values() {
            schema_shape(definition).unwrap();
        }
        let statement = schema_shape(&schema["$defs"]["Statement"]).unwrap();
        assert!(statement.contains("condition?:"));
        assert!(statement.contains("negation:"));
        assert!(statement.contains("modality:"));
        let term = schema_shape(&schema["$defs"]["Term"]).unwrap();
        assert!(term.contains("Quantity"));
        assert!(term.contains("\"statement\""));
    }
}

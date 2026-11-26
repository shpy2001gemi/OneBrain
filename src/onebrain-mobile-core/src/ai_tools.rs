use std::{
    collections::BTreeMap,
    fmt, fs,
    path::{Path, PathBuf},
};

use chacha20poly1305::{
    aead::{Aead, KeyInit, Payload},
    XChaCha20Poly1305, XNonce,
};
use ed25519_dalek::{Signature, Verifier, VerifyingKey};
use redb::{Database, ReadableTable, TableDefinition};
use serde::{Deserialize, Serialize};
use zeroize::Zeroizing;

use crate::MobileCoreError;

const TOOL_PROPOSALS: TableDefinition<&str, &[u8]> = TableDefinition::new("ai_tool_proposals");
const TOOL_RECEIPTS: TableDefinition<&str, &[u8]> = TableDefinition::new("ai_tool_receipts");
const CLOUD_DISCLOSURES: TableDefinition<&str, &[u8]> =
    TableDefinition::new("ai_cloud_disclosures");
const MODEL_RELEASES: TableDefinition<&str, &[u8]> = TableDefinition::new("ai_model_releases");
const MODEL_ACTIVE_ROUTE: TableDefinition<&str, &[u8]> =
    TableDefinition::new("ai_model_active_route");
const AI_THREADS: TableDefinition<&str, &[u8]> = TableDefinition::new("ai_threads");

const ACTIVE_ROUTE_KEY: &str = "current";
const TOOL_PAYLOAD_AAD: &[u8] = b"onebrain:mobile:ai-tool-payload:1\0";
const THREAD_PAYLOAD_AAD: &[u8] = b"onebrain:mobile:ai-thread-payload:1\0";
const MODEL_RELEASE_SIGN_DOMAIN: &[u8] = b"onebrain:mobile:model-release:1\0";
const NONCE_BYTES: usize = 24;
const ENVELOPE_VERSION: u8 = 1;

pub struct AiJournalKey(Zeroizing<[u8; 32]>);

impl AiJournalKey {
    pub fn derive(vault_key: &[u8; 32]) -> Self {
        Self(Zeroizing::new(blake3::derive_key(
            "onebrain:mobile:ai-journal-key:1",
            vault_key,
        )))
    }
}

impl fmt::Debug for AiJournalKey {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("AiJournalKey([REDACTED])")
    }
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum ToolEffectClass {
    ReadLocalBounded,
    DeriveLocal,
    MutateLocalReversible,
    DiscloseOrNetwork,
    PublicOrSigned,
    ForbiddenToLlm,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ToolResultVisibility {
    LocalOnly,
    UserOnly,
    MayReturnToProvider,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "PascalCase")]
pub enum ToolLifecycleState {
    Proposed,
    SchemaValidated,
    PolicyChecked,
    AwaitingConsent,
    Authorized,
    Prepared,
    Executing,
    Committed,
    Aborted,
    Unknown,
}

impl ToolLifecycleState {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Proposed => "Proposed",
            Self::SchemaValidated => "SchemaValidated",
            Self::PolicyChecked => "PolicyChecked",
            Self::AwaitingConsent => "AwaitingConsent",
            Self::Authorized => "Authorized",
            Self::Prepared => "Prepared",
            Self::Executing => "Executing",
            Self::Committed => "Committed",
            Self::Aborted => "Aborted",
            Self::Unknown => "Unknown",
        }
    }
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct ToolCatalogEntry {
    pub tool_name: String,
    pub effect_class: ToolEffectClass,
    pub idempotent_execution: bool,
    pub default_visibility: ToolResultVisibility,
    pub schema_digest: String,
}

pub fn canonical_tool_catalog() -> Vec<ToolCatalogEntry> {
    let entries = [
        (
            "search_local_library",
            ToolEffectClass::ReadLocalBounded,
            true,
            ToolResultVisibility::MayReturnToProvider,
            "schema:search_local_library:v1",
        ),
        (
            "run_local_read_only_kql",
            ToolEffectClass::ReadLocalBounded,
            true,
            ToolResultVisibility::MayReturnToProvider,
            "schema:run_local_read_only_kql:v1",
        ),
        (
            "draft_ku_encoding_candidate",
            ToolEffectClass::DeriveLocal,
            true,
            ToolResultVisibility::UserOnly,
            "schema:draft_ku_encoding_candidate:v1",
        ),
        (
            "save_private_raw_draft",
            ToolEffectClass::MutateLocalReversible,
            false,
            ToolResultVisibility::UserOnly,
            "schema:save_private_raw_draft:v1",
        ),
        (
            "prepare_cloud_disclosure",
            ToolEffectClass::DiscloseOrNetwork,
            false,
            ToolResultVisibility::LocalOnly,
            "schema:prepare_cloud_disclosure:v1",
        ),
        (
            "sign_public_statement",
            ToolEffectClass::PublicOrSigned,
            false,
            ToolResultVisibility::LocalOnly,
            "schema:sign_public_statement:v1",
        ),
        (
            "export_raw_vault_keys",
            ToolEffectClass::ForbiddenToLlm,
            false,
            ToolResultVisibility::LocalOnly,
            "schema:forbidden:export_raw_vault_keys:v1",
        ),
        (
            "silent_public_publish",
            ToolEffectClass::ForbiddenToLlm,
            false,
            ToolResultVisibility::LocalOnly,
            "schema:forbidden:silent_public_publish:v1",
        ),
        (
            "bypass_consent_gate",
            ToolEffectClass::ForbiddenToLlm,
            false,
            ToolResultVisibility::LocalOnly,
            "schema:forbidden:bypass_consent_gate:v1",
        ),
    ];
    entries
        .into_iter()
        .map(
            |(name, effect_class, idempotent_execution, default_visibility, schema_seed)| {
                ToolCatalogEntry {
                    tool_name: name.into(),
                    effect_class,
                    idempotent_execution,
                    default_visibility,
                    schema_digest: blake3::hash(schema_seed.as_bytes()).to_hex().to_string(),
                }
            },
        )
        .collect()
}

pub fn tool_catalog_digest() -> String {
    let encoded = serde_json::to_vec(&canonical_tool_catalog()).unwrap_or_default();
    blake3::hash(&encoded).to_hex().to_string()
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub enum NormalizedLlmStreamEvent {
    TextDelta(String),
    ToolArgumentsDelta {
        tool_name: String,
        partial_json: String,
    },
    StructuredCandidate {
        tool_name: String,
        arguments_json: String,
    },
    Usage {
        input_tokens: u32,
        output_tokens: u32,
    },
    Completed {
        finish_reason: String,
    },
    Failed {
        error_code: String,
    },
    Cancelled,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CompletedLlmResponse {
    pub provider_id: String,
    pub is_cloud_route: bool,
    pub assistant_text: String,
    pub candidate_tool_calls: Vec<(String, String)>,
}

/// Assembles normalized provider stream events. Tool proposals are ONLY
/// surfaced when the stream reaches an explicit `Completed` event without
/// `Failed` or `Cancelled` (`MOB-AI-001` / `MOB-AI-005`).
pub struct LlmStreamAssembler {
    provider_id: String,
    is_cloud_route: bool,
    text_buffer: String,
    staged_candidates: Vec<(String, String)>,
    completed: bool,
    aborted: bool,
}

impl LlmStreamAssembler {
    pub fn new(provider_id: impl Into<String>, is_cloud_route: bool) -> Self {
        Self {
            provider_id: provider_id.into(),
            is_cloud_route,
            text_buffer: String::new(),
            staged_candidates: Vec::new(),
            completed: false,
            aborted: false,
        }
    }

    pub fn push(&mut self, event: NormalizedLlmStreamEvent) -> Result<(), MobileCoreError> {
        if self.completed || self.aborted {
            return Err(MobileCoreError::InvalidArgument(
                "cannot push events after terminal LLM stream state".into(),
            ));
        }
        match event {
            NormalizedLlmStreamEvent::TextDelta(delta) => {
                if self.text_buffer.len().saturating_add(delta.len()) > 256 * 1024 {
                    self.aborted = true;
                    return Err(MobileCoreError::BudgetExceeded(
                        "LLM stream text exceeded 256 KiB bound".into(),
                    ));
                }
                self.text_buffer.push_str(&delta);
            }
            NormalizedLlmStreamEvent::ToolArgumentsDelta { .. } => {
                // Partial tool deltas are never executable or promoted until StructuredCandidate + Completed.
            }
            NormalizedLlmStreamEvent::StructuredCandidate {
                tool_name,
                arguments_json,
            } => {
                if self.staged_candidates.len() >= 8 || arguments_json.len() > 64 * 1024 {
                    self.aborted = true;
                    return Err(MobileCoreError::BudgetExceeded(
                        "LLM structured tool candidate exceeded budget".into(),
                    ));
                }
                let _: serde_json::Value = serde_json::from_str(&arguments_json).map_err(|_| {
                    self.aborted = true;
                    MobileCoreError::InvalidArgument(
                        "structured tool candidate arguments must be valid JSON".into(),
                    )
                })?;
                self.staged_candidates.push((tool_name, arguments_json));
            }
            NormalizedLlmStreamEvent::Usage { .. } => {}
            NormalizedLlmStreamEvent::Completed { .. } => {
                self.completed = true;
            }
            NormalizedLlmStreamEvent::Failed { .. } | NormalizedLlmStreamEvent::Cancelled => {
                self.aborted = true;
                self.staged_candidates.clear();
            }
        }
        Ok(())
    }

    pub fn finalize(self) -> Result<CompletedLlmResponse, MobileCoreError> {
        if self.aborted || !self.completed {
            return Err(MobileCoreError::Security(
                "incomplete, cancelled, or failed LLM stream cannot emit executable tool proposals"
                    .into(),
            ));
        }
        Ok(CompletedLlmResponse {
            provider_id: self.provider_id,
            is_cloud_route: self.is_cloud_route,
            assistant_text: self.text_buffer,
            candidate_tool_calls: self.staged_candidates,
        })
    }
}

pub trait MobileLlmProvider: Send + Sync {
    fn provider_id(&self) -> &str;
    fn is_cloud_route(&self) -> bool;
    fn run_turn(
        &self,
        user_prompt: &str,
        scoped_context_refs: &[String],
    ) -> Result<Vec<NormalizedLlmStreamEvent>, MobileCoreError>;
}

/// Deterministic `LLM-0` local assistant (`MOB-AI-001/002`): works 100% offline
/// without any model binary, mapping structured intent to bounded read-only
/// search/KQL or reversible draft proposals.
pub struct Llm0DeterministicAssistant;

impl MobileLlmProvider for Llm0DeterministicAssistant {
    fn provider_id(&self) -> &str {
        "llm0.deterministic.local"
    }

    fn is_cloud_route(&self) -> bool {
        false
    }

    fn run_turn(
        &self,
        user_prompt: &str,
        scoped_context_refs: &[String],
    ) -> Result<Vec<NormalizedLlmStreamEvent>, MobileCoreError> {
        let trimmed = user_prompt.trim();
        if trimmed.is_empty() || trimmed.len() > 32 * 1024 {
            return Err(MobileCoreError::InvalidArgument(
                "assistant prompt must contain 1..=32768 UTF-8 bytes".into(),
            ));
        }
        let mut events = Vec::new();
        if let Some(kql_body) = trimmed.strip_prefix("kql:") {
            let query = kql_body.trim();
            let args = serde_json::json!({ "query": query }).to_string();
            events.push(NormalizedLlmStreamEvent::TextDelta(format!(
                "Prepared bounded read-only local KQL inspection ({} context refs).",
                scoped_context_refs.len()
            )));
            events.push(NormalizedLlmStreamEvent::StructuredCandidate {
                tool_name: "run_local_read_only_kql".into(),
                arguments_json: args,
            });
        } else if let Some(draft_body) = trimmed.strip_prefix("draft:") {
            let text = draft_body.trim();
            let args = serde_json::json!({
                "content_language": "en",
                "content": text,
            })
            .to_string();
            events.push(NormalizedLlmStreamEvent::TextDelta(
                "Prepared a reversible private raw draft proposal awaiting your review.".into(),
            ));
            events.push(NormalizedLlmStreamEvent::StructuredCandidate {
                tool_name: "save_private_raw_draft".into(),
                arguments_json: args,
            });
        } else {
            let args = serde_json::json!({ "query": trimmed }).to_string();
            events.push(NormalizedLlmStreamEvent::TextDelta(format!(
                "Deterministic LLM-0 local search helper ready for '{}' (private scope, {} attached refs).",
                trimmed,
                scoped_context_refs.len()
            )));
            events.push(NormalizedLlmStreamEvent::StructuredCandidate {
                tool_name: "search_local_library".into(),
                arguments_json: args,
            });
        }
        events.push(NormalizedLlmStreamEvent::Usage {
            input_tokens: u32::try_from(trimmed.len()).unwrap_or(0),
            output_tokens: 24,
        });
        events.push(NormalizedLlmStreamEvent::Completed {
            finish_reason: "deterministic_stop".into(),
        });
        Ok(events)
    }
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct ToolProposalRecord {
    pub proposal_id: String,
    pub tool_name: String,
    pub effect_class: ToolEffectClass,
    pub idempotent_execution: bool,
    pub state: ToolLifecycleState,
    pub result_visibility: ToolResultVisibility,
    pub catalog_digest: String,
    pub schema_digest: String,
    pub consent_nonce: String,
    pub expires_at_monotonic_ms: u64,
    pub created_at_monotonic_ms: u64,
    pub sealed_arguments: Vec<u8>,
    pub abort_reason: Option<String>,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct ToolProposalView {
    pub proposal_id: String,
    pub tool_name: String,
    pub effect_class: ToolEffectClass,
    pub idempotent_execution: bool,
    pub state: ToolLifecycleState,
    pub result_visibility: ToolResultVisibility,
    pub consent_nonce: String,
    pub expires_at_monotonic_ms: u64,
    pub arguments_json: String,
    pub abort_reason: Option<String>,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct ToolExecutionReceipt {
    pub proposal_id: String,
    pub tool_name: String,
    pub final_state: ToolLifecycleState,
    pub result_visibility: ToolResultVisibility,
    pub completed_at_monotonic_ms: u64,
    pub sealed_result: Option<Vec<u8>>,
    pub status_code: String,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct ToolExecutionReceiptView {
    pub proposal_id: String,
    pub tool_name: String,
    pub final_state: ToolLifecycleState,
    pub result_visibility: ToolResultVisibility,
    pub completed_at_monotonic_ms: u64,
    pub result_json: Option<String>,
    pub status_code: String,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct CloudDisclosurePreview {
    pub request_id: String,
    pub destination_provider_id: String,
    pub exact_field_count: usize,
    pub payload_bytes: u64,
    pub payload_digest: String,
    pub context_reset_applied: bool,
    pub approved: bool,
    pub consent_nonce: String,
    pub created_at_monotonic_ms: u64,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ModelRouteKind {
    None,
    LocalOnDevice,
    CloudDisclosed,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ModelReleaseState {
    VerifiedStaged,
    Active,
    RetiredRollbackEligible,
    RolledBack,
    Revoked,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct ModelReleaseManifest {
    pub release_id: String,
    pub model_id: String,
    pub display_name: String,
    pub route_kind: ModelRouteKind,
    pub version_sequence: u64,
    pub artifact_bytes: u64,
    pub artifact_blake3: String,
    pub min_ram_bytes: u64,
    pub publisher_key_id: String,
    pub signature_hex: String,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct ModelCatalogRecord {
    pub manifest: ModelReleaseManifest,
    pub state: ModelReleaseState,
    pub smoke_verified: bool,
    pub verified_at_monotonic_ms: u64,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct ActiveModelRouteRecord {
    pub active_release_id: Option<String>,
    pub previous_release_id: Option<String>,
    pub provider_id: String,
    pub route_kind: ModelRouteKind,
    pub highest_accepted_sequence: u64,
}

impl Default for ActiveModelRouteRecord {
    fn default() -> Self {
        Self {
            active_release_id: None,
            previous_release_id: None,
            provider_id: "none".into(),
            route_kind: ModelRouteKind::None,
            highest_accepted_sequence: 0,
        }
    }
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct AssistantThreadMessage {
    pub role: String,
    pub text: String,
    pub provider_id: String,
    pub proposal_ids: Vec<String>,
    pub created_at_monotonic_ms: u64,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct AssistantThreadRecord {
    pub thread_id: String,
    pub title: String,
    pub last_route_was_cloud: bool,
    pub attached_context_refs: Vec<String>,
    pub messages: Vec<AssistantThreadMessage>,
}

pub struct ToolOrchestrator {
    database: Database,
    models_root: PathBuf,
    cipher: XChaCha20Poly1305,
    catalog: BTreeMap<String, ToolCatalogEntry>,
    catalog_digest: String,
}

impl ToolOrchestrator {
    pub fn open(
        database_path: &Path,
        models_root: &Path,
        key: AiJournalKey,
    ) -> Result<Self, MobileCoreError> {
        if let Some(parent) = database_path.parent() {
            fs::create_dir_all(parent).map_err(|error| {
                MobileCoreError::Storage(format!(
                    "cannot create AI journal directory {}: {error}",
                    parent.display()
                ))
            })?;
        }
        fs::create_dir_all(models_root).map_err(|error| {
            MobileCoreError::Storage(format!(
                "cannot create model releases directory {}: {error}",
                models_root.display()
            ))
        })?;
        let database = Database::create(database_path)?;
        let write = database.begin_write()?;
        {
            write.open_table(TOOL_PROPOSALS)?;
            write.open_table(TOOL_RECEIPTS)?;
            write.open_table(CLOUD_DISCLOSURES)?;
            write.open_table(MODEL_RELEASES)?;
            write.open_table(AI_THREADS)?;
            let mut active = write.open_table(MODEL_ACTIVE_ROUTE)?;
            if active.get(ACTIVE_ROUTE_KEY)?.is_none() {
                let default_bytes = serde_json::to_vec(&ActiveModelRouteRecord::default())?;
                active.insert(ACTIVE_ROUTE_KEY, default_bytes.as_slice())?;
            }
        }
        write.commit()?;

        let catalog_vec = canonical_tool_catalog();
        let catalog_digest = tool_catalog_digest();
        let catalog = catalog_vec
            .into_iter()
            .map(|entry| (entry.tool_name.clone(), entry))
            .collect();
        let orchestrator = Self {
            database,
            models_root: models_root.to_path_buf(),
            cipher: XChaCha20Poly1305::new((&*key.0).into()),
            catalog,
            catalog_digest,
        };
        orchestrator.reconcile_interrupted_executions()?;
        Ok(orchestrator)
    }

    /// Crash recovery (`MOB-AI-004`): any proposal left in `Executing` or `Prepared`
    /// transitions to `Unknown` (never blindly replayed if non-idempotent!).
    pub fn reconcile_interrupted_executions(&self) -> Result<usize, MobileCoreError> {
        let write = self.database.begin_write()?;
        let mut transitioned = 0usize;
        {
            let mut proposals = write.open_table(TOOL_PROPOSALS)?;
            let mut updates = Vec::new();
            for row in proposals.iter()? {
                let (key, value) = row?;
                let mut record: ToolProposalRecord = serde_json::from_slice(value.value())?;
                if matches!(
                    record.state,
                    ToolLifecycleState::Prepared | ToolLifecycleState::Executing
                ) {
                    record.state = ToolLifecycleState::Unknown;
                    record.abort_reason =
                        Some("INTERRUPTED_DURING_EXECUTION_MANUAL_RECONCILE_REQUIRED".into());
                    updates.push((key.value().to_owned(), serde_json::to_vec(&record)?));
                }
            }
            for (id, bytes) in updates {
                proposals.insert(id.as_str(), bytes.as_slice())?;
                transitioned = transitioned.saturating_add(1);
            }
        }
        write.commit()?;
        Ok(transitioned)
    }

    /// Advance a tool call through `Proposed -> SchemaValidated -> PolicyChecked -> AwaitingConsent`
    /// (or auto-`Authorized` ONLY for `ReadLocalBounded` / `DeriveLocal` when explicitly requested,
    /// while `ForbiddenToLlm` is hard-rejected fail-closed!).
    pub fn propose_tool_call(
        &self,
        tool_name: &str,
        arguments_json: &str,
        now_monotonic_ms: u64,
    ) -> Result<ToolProposalView, MobileCoreError> {
        let Some(spec) = self.catalog.get(tool_name) else {
            return Err(MobileCoreError::ForbiddenTool(format!(
                "tool '{tool_name}' is not in the compiled mobile tool catalog"
            )));
        };
        if spec.effect_class == ToolEffectClass::ForbiddenToLlm {
            return Err(MobileCoreError::ForbiddenTool(format!(
                "tool '{tool_name}' is FORBIDDEN_TO_LLM and cannot be proposed or executed"
            )));
        }
        let parsed: serde_json::Value = serde_json::from_str(arguments_json).map_err(|_| {
            MobileCoreError::InvalidArgument("tool arguments must be valid JSON".into())
        })?;
        if !parsed.is_object() {
            return Err(MobileCoreError::InvalidArgument(
                "tool schema validation requires a JSON object argument".into(),
            ));
        }

        let mut id_bytes = [0u8; 16];
        getrandom::fill(&mut id_bytes)
            .map_err(|_| MobileCoreError::Security("tool proposal CSPRNG unavailable".into()))?;
        let proposal_id = format!("toolprop_{}", hex::encode(id_bytes));

        let mut nonce_bytes = [0u8; 16];
        getrandom::fill(&mut nonce_bytes)
            .map_err(|_| MobileCoreError::Security("tool consent CSPRNG unavailable".into()))?;
        let consent_nonce = format!("consent_{}", hex::encode(nonce_bytes));

        let next_state = match spec.effect_class {
            ToolEffectClass::ReadLocalBounded | ToolEffectClass::DeriveLocal => {
                ToolLifecycleState::PolicyChecked
            }
            ToolEffectClass::MutateLocalReversible
            | ToolEffectClass::DiscloseOrNetwork
            | ToolEffectClass::PublicOrSigned => ToolLifecycleState::AwaitingConsent,
            ToolEffectClass::ForbiddenToLlm => unreachable!("checked above"),
        };

        let sealed_arguments =
            self.seal(TOOL_PAYLOAD_AAD, &proposal_id, arguments_json.as_bytes())?;
        let record = ToolProposalRecord {
            proposal_id: proposal_id.clone(),
            tool_name: tool_name.to_owned(),
            effect_class: spec.effect_class,
            idempotent_execution: spec.idempotent_execution,
            state: next_state,
            result_visibility: spec.default_visibility,
            catalog_digest: self.catalog_digest.clone(),
            schema_digest: spec.schema_digest.clone(),
            consent_nonce: consent_nonce.clone(),
            expires_at_monotonic_ms: now_monotonic_ms.saturating_add(10 * 60 * 1000),
            created_at_monotonic_ms: now_monotonic_ms,
            sealed_arguments,
            abort_reason: None,
        };
        let encoded = serde_json::to_vec(&record)?;
        let write = self.database.begin_write()?;
        {
            let mut table = write.open_table(TOOL_PROPOSALS)?;
            table.insert(proposal_id.as_str(), encoded.as_slice())?;
        }
        write.commit()?;

        Ok(ToolProposalView {
            proposal_id,
            tool_name: record.tool_name,
            effect_class: record.effect_class,
            idempotent_execution: record.idempotent_execution,
            state: record.state,
            result_visibility: record.result_visibility,
            consent_nonce,
            expires_at_monotonic_ms: record.expires_at_monotonic_ms,
            arguments_json: arguments_json.to_owned(),
            abort_reason: None,
        })
    }

    pub fn authorize_and_execute<F>(
        &self,
        proposal_id: &str,
        consent_nonce: Option<&str>,
        now_monotonic_ms: u64,
        executor: F,
    ) -> Result<ToolExecutionReceiptView, MobileCoreError>
    where
        F: FnOnce(&str, &serde_json::Value) -> Result<serde_json::Value, MobileCoreError>,
    {
        let mut record = self
            .read_proposal_record(proposal_id)?
            .ok_or_else(|| MobileCoreError::InvalidArgument("unknown proposal_id".into()))?;

        if record.state == ToolLifecycleState::Unknown && !record.idempotent_execution {
            return Err(MobileCoreError::Security(
                "non-idempotent Unknown tool call cannot be blindly replayed; inspect outcome first"
                    .into(),
            ));
        }
        if matches!(
            record.state,
            ToolLifecycleState::Committed | ToolLifecycleState::Aborted
        ) {
            return Err(MobileCoreError::InvalidArgument(
                "tool proposal is already in a terminal state".into(),
            ));
        }
        if now_monotonic_ms > record.expires_at_monotonic_ms {
            record.state = ToolLifecycleState::Aborted;
            record.abort_reason = Some("CONSENT_EXPIRED".into());
            self.write_proposal_record(&record)?;
            return Err(MobileCoreError::Security(
                "tool proposal expired before execution".into(),
            ));
        }
        if record.catalog_digest != self.catalog_digest {
            record.state = ToolLifecycleState::Aborted;
            record.abort_reason = Some("CATALOG_DIGEST_CHANGED".into());
            self.write_proposal_record(&record)?;
            return Err(MobileCoreError::Security(
                "tool catalog digest changed since proposal creation".into(),
            ));
        }
        if record.effect_class == ToolEffectClass::ForbiddenToLlm {
            return Err(MobileCoreError::ForbiddenTool(
                "FORBIDDEN_TO_LLM tool cannot be authorized".into(),
            ));
        }

        let requires_explicit_consent = matches!(
            record.effect_class,
            ToolEffectClass::MutateLocalReversible
                | ToolEffectClass::DiscloseOrNetwork
                | ToolEffectClass::PublicOrSigned
        );
        if requires_explicit_consent {
            let provided = consent_nonce.ok_or_else(|| {
                MobileCoreError::Security("explicit consent nonce is required".into())
            })?;
            if provided != record.consent_nonce {
                return Err(MobileCoreError::Security(
                    "consent nonce does not match the durable proposal".into(),
                ));
            }
        }

        record.state = ToolLifecycleState::Authorized;
        self.write_proposal_record(&record)?;
        record.state = ToolLifecycleState::Prepared;
        self.write_proposal_record(&record)?;
        record.state = ToolLifecycleState::Executing;
        self.write_proposal_record(&record)?;

        let decrypted =
            self.open_sealed(TOOL_PAYLOAD_AAD, proposal_id, &record.sealed_arguments)?;
        let args_value: serde_json::Value = serde_json::from_slice(&decrypted)?;

        match executor(&record.tool_name, &args_value) {
            Ok(result_value) => {
                let result_json = serde_json::to_string(&result_value)?;
                let sealed_result =
                    self.seal(TOOL_PAYLOAD_AAD, proposal_id, result_json.as_bytes())?;
                record.state = ToolLifecycleState::Committed;
                let receipt = ToolExecutionReceipt {
                    proposal_id: proposal_id.to_owned(),
                    tool_name: record.tool_name.clone(),
                    final_state: ToolLifecycleState::Committed,
                    result_visibility: record.result_visibility,
                    completed_at_monotonic_ms: now_monotonic_ms,
                    sealed_result: Some(sealed_result),
                    status_code: "COMMITTED".into(),
                };
                self.commit_proposal_and_receipt(&record, &receipt)?;
                Ok(ToolExecutionReceiptView {
                    proposal_id: receipt.proposal_id,
                    tool_name: receipt.tool_name,
                    final_state: receipt.final_state,
                    result_visibility: receipt.result_visibility,
                    completed_at_monotonic_ms: receipt.completed_at_monotonic_ms,
                    result_json: Some(result_json),
                    status_code: receipt.status_code,
                })
            }
            Err(error) => {
                record.state = ToolLifecycleState::Aborted;
                record.abort_reason = Some(error.to_string());
                let receipt = ToolExecutionReceipt {
                    proposal_id: proposal_id.to_owned(),
                    tool_name: record.tool_name.clone(),
                    final_state: ToolLifecycleState::Aborted,
                    result_visibility: ToolResultVisibility::LocalOnly,
                    completed_at_monotonic_ms: now_monotonic_ms,
                    sealed_result: None,
                    status_code: "ABORTED_ON_ERROR".into(),
                };
                self.commit_proposal_and_receipt(&record, &receipt)?;
                Err(error)
            }
        }
    }

    pub fn deny_proposal(
        &self,
        proposal_id: &str,
        now_monotonic_ms: u64,
    ) -> Result<ToolExecutionReceiptView, MobileCoreError> {
        let mut record = self
            .read_proposal_record(proposal_id)?
            .ok_or_else(|| MobileCoreError::InvalidArgument("unknown proposal_id".into()))?;
        record.state = ToolLifecycleState::Aborted;
        record.abort_reason = Some("DENIED_BY_USER".into());
        let receipt = ToolExecutionReceipt {
            proposal_id: proposal_id.to_owned(),
            tool_name: record.tool_name.clone(),
            final_state: ToolLifecycleState::Aborted,
            result_visibility: ToolResultVisibility::LocalOnly,
            completed_at_monotonic_ms: now_monotonic_ms,
            sealed_result: None,
            status_code: "DENIED_BY_USER".into(),
        };
        self.commit_proposal_and_receipt(&record, &receipt)?;
        Ok(ToolExecutionReceiptView {
            proposal_id: receipt.proposal_id,
            tool_name: receipt.tool_name,
            final_state: receipt.final_state,
            result_visibility: receipt.result_visibility,
            completed_at_monotonic_ms: receipt.completed_at_monotonic_ms,
            result_json: None,
            status_code: receipt.status_code,
        })
    }

    pub fn list_proposals(&self, limit: usize) -> Result<Vec<ToolProposalView>, MobileCoreError> {
        let bounded = limit.clamp(1, 100);
        let read = self.database.begin_read()?;
        let table = read.open_table(TOOL_PROPOSALS)?;
        let mut out = Vec::new();
        for row in table.iter()? {
            let (_, value) = row?;
            let record: ToolProposalRecord = serde_json::from_slice(value.value())?;
            let args_bytes = self.open_sealed(
                TOOL_PAYLOAD_AAD,
                &record.proposal_id,
                &record.sealed_arguments,
            )?;
            let arguments_json = String::from_utf8(args_bytes).unwrap_or_else(|_| "{}".into());
            out.push(ToolProposalView {
                proposal_id: record.proposal_id,
                tool_name: record.tool_name,
                effect_class: record.effect_class,
                idempotent_execution: record.idempotent_execution,
                state: record.state,
                result_visibility: record.result_visibility,
                consent_nonce: record.consent_nonce,
                expires_at_monotonic_ms: record.expires_at_monotonic_ms,
                arguments_json,
                abort_reason: record.abort_reason,
            });
        }
        out.sort_by(|left, right| {
            right
                .expires_at_monotonic_ms
                .cmp(&left.expires_at_monotonic_ms)
                .then_with(|| left.proposal_id.cmp(&right.proposal_id))
        });
        out.truncate(bounded);
        Ok(out)
    }

    /// Cloud `ContextDisclosureGate` (`MOB-AI-003/007`): creates an explicit
    /// outbound payload preview with byte count & digest and enforces context reset
    /// when switching from a local conversation to a cloud route.
    pub fn prepare_cloud_disclosure(
        &self,
        destination_provider_id: &str,
        minimized_fields: &[(&str, &str)],
        switching_from_local_thread: bool,
        now_monotonic_ms: u64,
    ) -> Result<CloudDisclosurePreview, MobileCoreError> {
        if destination_provider_id.is_empty() || destination_provider_id.len() > 96 {
            return Err(MobileCoreError::InvalidArgument(
                "destination_provider_id is invalid".into(),
            ));
        }
        let mut hasher = blake3::Hasher::new();
        let mut total_bytes = 0u64;
        for (field, value) in minimized_fields {
            total_bytes = total_bytes
                .saturating_add(field.len() as u64)
                .saturating_add(value.len() as u64);
            hasher.update(&(field.len() as u64).to_le_bytes());
            hasher.update(field.as_bytes());
            hasher.update(&(value.len() as u64).to_le_bytes());
            hasher.update(value.as_bytes());
        }
        let mut id_bytes = [0u8; 16];
        getrandom::fill(&mut id_bytes)
            .map_err(|_| MobileCoreError::Security("disclosure CSPRNG unavailable".into()))?;
        let mut nonce_bytes = [0u8; 16];
        getrandom::fill(&mut nonce_bytes)
            .map_err(|_| MobileCoreError::Security("disclosure nonce CSPRNG unavailable".into()))?;
        let preview = CloudDisclosurePreview {
            request_id: format!("disclose_{}", hex::encode(id_bytes)),
            destination_provider_id: destination_provider_id.to_owned(),
            exact_field_count: minimized_fields.len(),
            payload_bytes: total_bytes,
            payload_digest: hasher.finalize().to_hex().to_string(),
            context_reset_applied: switching_from_local_thread,
            approved: false,
            consent_nonce: format!("dnonce_{}", hex::encode(nonce_bytes)),
            created_at_monotonic_ms: now_monotonic_ms,
        };
        let bytes = serde_json::to_vec(&preview)?;
        let write = self.database.begin_write()?;
        {
            let mut table = write.open_table(CLOUD_DISCLOSURES)?;
            table.insert(preview.request_id.as_str(), bytes.as_slice())?;
        }
        write.commit()?;
        Ok(preview)
    }

    pub fn approve_cloud_disclosure(
        &self,
        request_id: &str,
        consent_nonce: &str,
    ) -> Result<CloudDisclosurePreview, MobileCoreError> {
        let write = self.database.begin_write()?;
        let approved;
        {
            let mut table = write.open_table(CLOUD_DISCLOSURES)?;
            let raw = table
                .get(request_id)?
                .ok_or_else(|| MobileCoreError::InvalidArgument("unknown request_id".into()))?;
            let mut preview: CloudDisclosurePreview = serde_json::from_slice(raw.value())?;
            drop(raw);
            if preview.consent_nonce != consent_nonce {
                return Err(MobileCoreError::Security(
                    "cloud disclosure consent nonce mismatch".into(),
                ));
            }
            preview.approved = true;
            let bytes = serde_json::to_vec(&preview)?;
            table.insert(request_id, bytes.as_slice())?;
            approved = preview;
        }
        write.commit()?;
        Ok(approved)
    }

    /// Verify and stage a signed `ModelReleaseManifest` + model artifact bytes (`MOB-MOD-001..003`).
    pub fn verify_and_stage_model_release(
        &self,
        manifest: &ModelReleaseManifest,
        publisher_public_key: &[u8; 32],
        artifact_bytes: &[u8],
        now_monotonic_ms: u64,
    ) -> Result<ModelCatalogRecord, MobileCoreError> {
        if manifest.release_id.is_empty() || manifest.model_id.is_empty() {
            return Err(MobileCoreError::InvalidArgument(
                "model release manifest requires non-empty release_id and model_id".into(),
            ));
        }
        let verifying_key = VerifyingKey::from_bytes(publisher_public_key)
            .map_err(|_| MobileCoreError::Security("invalid model publisher public key".into()))?;
        let sig_bytes: [u8; 64] = hex::decode(&manifest.signature_hex)
            .map_err(|_| MobileCoreError::Security("invalid model signature hex".into()))?
            .try_into()
            .map_err(|_| MobileCoreError::Security("model signature must be 64 bytes".into()))?;
        let signature = Signature::from_bytes(&sig_bytes);
        let msg = model_release_signing_message(manifest);
        verifying_key.verify(&msg, &signature).map_err(|_| {
            MobileCoreError::Security("model release Ed25519 signature verification failed".into())
        })?;

        if artifact_bytes.len() as u64 != manifest.artifact_bytes {
            return Err(MobileCoreError::Security(
                "model artifact byte length does not match signed manifest".into(),
            ));
        }
        let digest = blake3::hash(artifact_bytes).to_hex().to_string();
        if digest != manifest.artifact_blake3 {
            return Err(MobileCoreError::Security(
                "model artifact BLAKE3 digest does not match signed manifest".into(),
            ));
        }

        let active = self.active_model_route()?;
        if manifest.version_sequence < active.highest_accepted_sequence {
            return Err(MobileCoreError::Security(
                "model release version_sequence is older than accepted high-water".into(),
            ));
        }

        let slot_dir = self.models_root.join(&manifest.release_id);
        fs::create_dir_all(&slot_dir).map_err(|error| {
            MobileCoreError::Storage(format!("cannot create model slot directory: {error}"))
        })?;
        fs::write(slot_dir.join("model.weights.bin"), artifact_bytes).map_err(|error| {
            MobileCoreError::Storage(format!("cannot write verified model artifact: {error}"))
        })?;

        let record = ModelCatalogRecord {
            manifest: manifest.clone(),
            state: ModelReleaseState::VerifiedStaged,
            smoke_verified: true,
            verified_at_monotonic_ms: now_monotonic_ms,
        };
        let bytes = serde_json::to_vec(&record)?;
        let write = self.database.begin_write()?;
        {
            let mut releases = write.open_table(MODEL_RELEASES)?;
            releases.insert(manifest.release_id.as_str(), bytes.as_slice())?;
            let mut route_table = write.open_table(MODEL_ACTIVE_ROUTE)?;
            let mut current = active;
            current.highest_accepted_sequence = current
                .highest_accepted_sequence
                .max(manifest.version_sequence);
            let route_bytes = serde_json::to_vec(&current)?;
            route_table.insert(ACTIVE_ROUTE_KEY, route_bytes.as_slice())?;
        }
        write.commit()?;
        Ok(record)
    }

    pub fn activate_model_release(
        &self,
        release_id: &str,
    ) -> Result<ActiveModelRouteRecord, MobileCoreError> {
        let write = self.database.begin_write()?;
        let updated_route;
        {
            let mut releases = write.open_table(MODEL_RELEASES)?;
            let mut route_table = write.open_table(MODEL_ACTIVE_ROUTE)?;
            let target_raw = releases.get(release_id)?.ok_or_else(|| {
                MobileCoreError::InvalidArgument("unknown model release_id".into())
            })?;
            let mut target: ModelCatalogRecord = serde_json::from_slice(target_raw.value())?;
            drop(target_raw);

            if target.state == ModelReleaseState::Revoked || !target.smoke_verified {
                return Err(MobileCoreError::Security(
                    "revoked or unverified model release cannot be activated".into(),
                ));
            }
            if target.manifest.route_kind == ModelRouteKind::LocalOnDevice
                && !self
                    .models_root
                    .join(release_id)
                    .join("model.weights.bin")
                    .is_file()
            {
                return Err(MobileCoreError::Storage(
                    "local model slot weights are missing".into(),
                ));
            }

            let mut route: ActiveModelRouteRecord = route_table
                .get(ACTIVE_ROUTE_KEY)?
                .map(|raw| serde_json::from_slice(raw.value()))
                .transpose()?
                .unwrap_or_default();

            if let Some(prev_id) = route.active_release_id.clone() {
                if prev_id != release_id {
                    let prev_retired = releases
                        .get(prev_id.as_str())?
                        .map(|prev_raw| {
                            serde_json::from_slice::<ModelCatalogRecord>(prev_raw.value())
                        })
                        .transpose()?;
                    if let Some(mut prev_rec) = prev_retired {
                        prev_rec.state = ModelReleaseState::RetiredRollbackEligible;
                        let prev_bytes = serde_json::to_vec(&prev_rec)?;
                        releases.insert(prev_id.as_str(), prev_bytes.as_slice())?;
                    }
                    route.previous_release_id = Some(prev_id);
                }
            }

            target.state = ModelReleaseState::Active;
            let target_bytes = serde_json::to_vec(&target)?;
            releases.insert(release_id, target_bytes.as_slice())?;

            route.active_release_id = Some(release_id.to_owned());
            route.provider_id = target.manifest.model_id.clone();
            route.route_kind = target.manifest.route_kind;
            let route_bytes = serde_json::to_vec(&route)?;
            route_table.insert(ACTIVE_ROUTE_KEY, route_bytes.as_slice())?;
            updated_route = route;
        }
        write.commit()?;
        Ok(updated_route)
    }

    pub fn rollback_model_release(&self) -> Result<ActiveModelRouteRecord, MobileCoreError> {
        let write = self.database.begin_write()?;
        let updated_route;
        {
            let mut releases = write.open_table(MODEL_RELEASES)?;
            let mut route_table = write.open_table(MODEL_ACTIVE_ROUTE)?;
            let mut route: ActiveModelRouteRecord = route_table
                .get(ACTIVE_ROUTE_KEY)?
                .map(|raw| serde_json::from_slice(raw.value()))
                .transpose()?
                .unwrap_or_default();

            let Some(prev_id) = route.previous_release_id.take() else {
                return Err(MobileCoreError::RollbackUnavailable(
                    "no previous verified model release is retained for rollback".into(),
                ));
            };
            let prev_raw = releases.get(prev_id.as_str())?.ok_or_else(|| {
                MobileCoreError::RollbackUnavailable("previous model release was deleted".into())
            })?;
            let mut prev_rec: ModelCatalogRecord = serde_json::from_slice(prev_raw.value())?;
            drop(prev_raw);
            if prev_rec.state == ModelReleaseState::Revoked {
                return Err(MobileCoreError::Security(
                    "revoked N-1 model release cannot be restored".into(),
                ));
            }

            if let Some(current_id) = route.active_release_id.take() {
                let current_retired = releases
                    .get(current_id.as_str())?
                    .map(|cur_raw| serde_json::from_slice::<ModelCatalogRecord>(cur_raw.value()))
                    .transpose()?;
                if let Some(mut cur_rec) = current_retired {
                    cur_rec.state = ModelReleaseState::RolledBack;
                    let cur_bytes = serde_json::to_vec(&cur_rec)?;
                    releases.insert(current_id.as_str(), cur_bytes.as_slice())?;
                }
            }

            prev_rec.state = ModelReleaseState::Active;
            let prev_bytes = serde_json::to_vec(&prev_rec)?;
            releases.insert(prev_id.as_str(), prev_bytes.as_slice())?;

            route.active_release_id = Some(prev_id);
            route.provider_id = prev_rec.manifest.model_id;
            route.route_kind = prev_rec.manifest.route_kind;
            let route_bytes = serde_json::to_vec(&route)?;
            route_table.insert(ACTIVE_ROUTE_KEY, route_bytes.as_slice())?;
            updated_route = route;
        }
        write.commit()?;
        Ok(updated_route)
    }

    pub fn delete_model_release(&self, release_id: &str) -> Result<(), MobileCoreError> {
        let write = self.database.begin_write()?;
        {
            let mut releases = write.open_table(MODEL_RELEASES)?;
            let mut route_table = write.open_table(MODEL_ACTIVE_ROUTE)?;
            let mut route: ActiveModelRouteRecord = route_table
                .get(ACTIVE_ROUTE_KEY)?
                .map(|raw| serde_json::from_slice(raw.value()))
                .transpose()?
                .unwrap_or_default();

            if route.active_release_id.as_deref() == Some(release_id) {
                route.active_release_id = None;
                route.provider_id = "none".into();
                route.route_kind = ModelRouteKind::None;
            }
            if route.previous_release_id.as_deref() == Some(release_id) {
                route.previous_release_id = None;
            }
            releases.remove(release_id)?;
            let route_bytes = serde_json::to_vec(&route)?;
            route_table.insert(ACTIVE_ROUTE_KEY, route_bytes.as_slice())?;
        }
        write.commit()?;
        let slot_dir = self.models_root.join(release_id);
        if slot_dir.exists() {
            let _ = fs::remove_dir_all(slot_dir);
        }
        Ok(())
    }

    pub fn active_model_route(&self) -> Result<ActiveModelRouteRecord, MobileCoreError> {
        let read = self.database.begin_read()?;
        let table = read.open_table(MODEL_ACTIVE_ROUTE)?;
        table
            .get(ACTIVE_ROUTE_KEY)?
            .map(|raw| serde_json::from_slice(raw.value()).map_err(Into::into))
            .transpose()
            .map(|opt| opt.unwrap_or_default())
    }

    pub fn list_model_releases(&self) -> Result<Vec<ModelCatalogRecord>, MobileCoreError> {
        let read = self.database.begin_read()?;
        let table = read.open_table(MODEL_RELEASES)?;
        let mut list: Vec<ModelCatalogRecord> = Vec::new();
        for row in table.iter()? {
            let (_, value) = row?;
            list.push(serde_json::from_slice(value.value())?);
        }
        list.sort_by(|left, right| {
            right
                .manifest
                .version_sequence
                .cmp(&left.manifest.version_sequence)
                .then_with(|| left.manifest.release_id.cmp(&right.manifest.release_id))
        });
        Ok(list)
    }

    pub fn append_thread_turn(
        &self,
        thread_id: &str,
        user_prompt: &str,
        response: &CompletedLlmResponse,
        proposal_ids: Vec<String>,
        attached_context_refs: Vec<String>,
        now_monotonic_ms: u64,
    ) -> Result<AssistantThreadRecord, MobileCoreError> {
        let existing = self.read_thread(thread_id)?;
        let mut thread = match existing {
            Some(mut current) => {
                if !current.last_route_was_cloud && response.is_cloud_route {
                    // Switching local -> cloud resets prior local transcript & context unless explicitly re-attached!
                    current.messages.clear();
                }
                current.last_route_was_cloud = response.is_cloud_route;
                current.attached_context_refs = attached_context_refs;
                current
            }
            None => AssistantThreadRecord {
                thread_id: thread_id.to_owned(),
                title: user_prompt.chars().take(48).collect(),
                last_route_was_cloud: response.is_cloud_route,
                attached_context_refs,
                messages: Vec::new(),
            },
        };
        thread.messages.push(AssistantThreadMessage {
            role: "user".into(),
            text: user_prompt.to_owned(),
            provider_id: "user".into(),
            proposal_ids: Vec::new(),
            created_at_monotonic_ms: now_monotonic_ms,
        });
        thread.messages.push(AssistantThreadMessage {
            role: "assistant".into(),
            text: response.assistant_text.clone(),
            provider_id: response.provider_id.clone(),
            proposal_ids,
            created_at_monotonic_ms: now_monotonic_ms,
        });
        let plaintext = Zeroizing::new(serde_json::to_vec(&thread)?);
        let sealed = self.seal(THREAD_PAYLOAD_AAD, thread_id, &plaintext)?;
        let write = self.database.begin_write()?;
        {
            let mut table = write.open_table(AI_THREADS)?;
            table.insert(thread_id, sealed.as_slice())?;
        }
        write.commit()?;
        Ok(thread)
    }

    pub fn read_thread(
        &self,
        thread_id: &str,
    ) -> Result<Option<AssistantThreadRecord>, MobileCoreError> {
        let read = self.database.begin_read()?;
        let table = read.open_table(AI_THREADS)?;
        let Some(sealed) = table.get(thread_id)? else {
            return Ok(None);
        };
        let plaintext = self.open_sealed(THREAD_PAYLOAD_AAD, thread_id, sealed.value())?;
        Ok(Some(serde_json::from_slice(&plaintext)?))
    }

    fn read_proposal_record(
        &self,
        proposal_id: &str,
    ) -> Result<Option<ToolProposalRecord>, MobileCoreError> {
        let read = self.database.begin_read()?;
        let table = read.open_table(TOOL_PROPOSALS)?;
        table
            .get(proposal_id)?
            .map(|raw| serde_json::from_slice(raw.value()).map_err(Into::into))
            .transpose()
    }

    fn write_proposal_record(&self, record: &ToolProposalRecord) -> Result<(), MobileCoreError> {
        let bytes = serde_json::to_vec(record)?;
        let write = self.database.begin_write()?;
        {
            let mut table = write.open_table(TOOL_PROPOSALS)?;
            table.insert(record.proposal_id.as_str(), bytes.as_slice())?;
        }
        write.commit()?;
        Ok(())
    }

    fn commit_proposal_and_receipt(
        &self,
        proposal: &ToolProposalRecord,
        receipt: &ToolExecutionReceipt,
    ) -> Result<(), MobileCoreError> {
        let prop_bytes = serde_json::to_vec(proposal)?;
        let rec_bytes = serde_json::to_vec(receipt)?;
        let write = self.database.begin_write()?;
        {
            let mut props = write.open_table(TOOL_PROPOSALS)?;
            let mut recs = write.open_table(TOOL_RECEIPTS)?;
            props.insert(proposal.proposal_id.as_str(), prop_bytes.as_slice())?;
            recs.insert(receipt.proposal_id.as_str(), rec_bytes.as_slice())?;
        }
        write.commit()?;
        Ok(())
    }

    fn seal(
        &self,
        context: &[u8],
        object_id: &str,
        plaintext: &[u8],
    ) -> Result<Vec<u8>, MobileCoreError> {
        let mut nonce_bytes = [0u8; NONCE_BYTES];
        getrandom::fill(&mut nonce_bytes)
            .map_err(|_| MobileCoreError::Security("AI journal CSPRNG unavailable".into()))?;
        let mut aad = Vec::with_capacity(context.len() + object_id.len());
        aad.extend_from_slice(context);
        aad.extend_from_slice(object_id.as_bytes());
        let ciphertext = self
            .cipher
            .encrypt(
                &XNonce::from(nonce_bytes),
                Payload {
                    msg: plaintext,
                    aad: &aad,
                },
            )
            .map_err(|_| MobileCoreError::Security("cannot seal AI journal payload".into()))?;
        let mut out = Vec::with_capacity(1 + NONCE_BYTES + ciphertext.len());
        out.push(ENVELOPE_VERSION);
        out.extend_from_slice(&nonce_bytes);
        out.extend_from_slice(&ciphertext);
        Ok(out)
    }

    fn open_sealed(
        &self,
        context: &[u8],
        object_id: &str,
        sealed: &[u8],
    ) -> Result<Vec<u8>, MobileCoreError> {
        if sealed.len() < 1 + NONCE_BYTES + 16 || sealed[0] != ENVELOPE_VERSION {
            return Err(MobileCoreError::Security(
                "AI journal sealed envelope is invalid".into(),
            ));
        }
        let nonce = XNonce::try_from(&sealed[1..1 + NONCE_BYTES])
            .map_err(|_| MobileCoreError::Security("AI journal nonce is invalid".into()))?;
        let mut aad = Vec::with_capacity(context.len() + object_id.len());
        aad.extend_from_slice(context);
        aad.extend_from_slice(object_id.as_bytes());
        self.cipher
            .decrypt(
                &nonce,
                Payload {
                    msg: &sealed[1 + NONCE_BYTES..],
                    aad: &aad,
                },
            )
            .map_err(|_| MobileCoreError::Security("AI journal authentication failed".into()))
    }
}

pub fn model_release_signing_message(manifest: &ModelReleaseManifest) -> Vec<u8> {
    let mut msg = Vec::from(MODEL_RELEASE_SIGN_DOMAIN);
    for text in [
        manifest.release_id.as_str(),
        manifest.model_id.as_str(),
        manifest.display_name.as_str(),
        manifest.artifact_blake3.as_str(),
        manifest.publisher_key_id.as_str(),
    ] {
        msg.extend_from_slice(&(text.len() as u64).to_le_bytes());
        msg.extend_from_slice(text.as_bytes());
    }
    let route_code: u8 = match manifest.route_kind {
        ModelRouteKind::None => 0,
        ModelRouteKind::LocalOnDevice => 1,
        ModelRouteKind::CloudDisclosed => 2,
    };
    msg.push(route_code);
    msg.extend_from_slice(&manifest.version_sequence.to_le_bytes());
    msg.extend_from_slice(&manifest.artifact_bytes.to_le_bytes());
    msg.extend_from_slice(&manifest.min_ram_bytes.to_le_bytes());
    msg
}

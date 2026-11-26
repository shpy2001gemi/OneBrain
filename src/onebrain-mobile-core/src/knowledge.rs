use std::{
    collections::{BTreeMap, BTreeSet},
    fmt,
    path::Path,
};

use chacha20poly1305::{
    aead::{Aead, KeyInit, Payload},
    XChaCha20Poly1305, XNonce,
};
use ku_core::{
    concept_registry::ConceptLookup,
    foundation::{
        DisclosureClass, EncodingAttempt, EncodingAttemptRole, EventCid, FidelityAssessmentStatus,
        FidelityPolicy, ObjectCid, ObjectReference, TermRef,
    },
};
use onebrain_core::{
    FidelityPortfolioTargetKey, PreparedKuArtifact, RedbCompositionStore, RedbFidelityPortfolio,
    SavedPrivateKuRecord, SyncKuWorkspace,
};
use redb::{Database, TableDefinition};
use serde::{Deserialize, Serialize};
use zeroize::Zeroizing;

use crate::{DerivedRegistryReadiness, MobileCoreError, PrivateDraftStore};

const ENCODING_DRAFTS: TableDefinition<&str, &[u8]> =
    TableDefinition::new("private_encoding_drafts");
const KU_LOCAL_ORG: TableDefinition<&str, &[u8]> = TableDefinition::new("private_ku_local_org");

const ENCODING_DRAFT_AAD: &[u8] = b"onebrain:mobile:encoding-draft:1\0";
const KU_ORG_AAD: &[u8] = b"onebrain:mobile:ku-local-org:1\0";
const NONCE_BYTES: usize = 24;
const ENVELOPE_VERSION: u8 = 1;
const DEFAULT_FIDELITY_POLICY_CID: [u8; 32] = [0xF1; 32];

pub struct KnowledgeMetaKey(Zeroizing<[u8; 32]>);

impl KnowledgeMetaKey {
    pub fn derive(vault_key: &[u8; 32]) -> Self {
        Self(Zeroizing::new(blake3::derive_key(
            "onebrain:mobile:knowledge-meta-key:1",
            vault_key,
        )))
    }
}

impl fmt::Debug for KnowledgeMetaKey {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("KnowledgeMetaKey([REDACTED])")
    }
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct EncodingDraftRecord {
    pub encoding_draft_id: String,
    pub source_raw_draft_ref: Option<String>,
    pub title: String,
    pub subject_label: String,
    pub predicate_label: String,
    pub object_label: String,
    pub source_text: String,
    pub predecessor_cid_hex: Option<String>,
    pub alternate_of_cid_hex: Option<String>,
    pub last_prepare_id: Option<String>,
    pub saved_object_cid_hex: Option<String>,
    pub created_at_monotonic_ms: u64,
}

#[derive(Clone, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
pub struct KuLocalOrganizationRecord {
    pub object_cid_hex: String,
    pub tags: Vec<String>,
    pub collections: Vec<String>,
    pub alternate_encoding_cids: Vec<String>,
    pub source_raw_draft_ref: Option<String>,
    pub fidelity_target_key_hex: Option<String>,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct PreparedKuPreviewView {
    pub prepare_id: String,
    pub title: String,
    pub candidate_object_cid_hex: String,
    pub semantic_content_cid_hex: String,
    pub canonical_preview_base64: String,
    pub redirect_rewrites: Vec<(String, String)>,
    pub predecessor_cid_hex: Option<String>,
    pub disclosure: String,
    pub author_status: String,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct MobileFidelityStatusView {
    pub target_key_hex: String,
    pub status_code: String,
    pub distinct_4d_corroboration_groups: usize,
    pub legacy_claim_count: u32,
    pub is_global_or_final: bool,
    pub establishes_proposition_truth: bool,
    pub blocks_preserve_or_query: bool,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct WorkflowStageView {
    pub stage_number: u8,
    pub stage_code: String,
    pub status_code: String,
    pub read_only_on_mobile_v1: bool,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct MobileKuSummary {
    pub object_cid_hex: String,
    pub semantic_content_cid_hex: String,
    pub title: String,
    pub saved_seq: u64,
    pub disclosure: String,
    pub author_status: String,
    pub fidelity_status: String,
    pub tags: Vec<String>,
    pub collections: Vec<String>,
    pub has_revisions: bool,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct PaginatedKuPage {
    pub items: Vec<MobileKuSummary>,
    pub next_cursor_seq: Option<u64>,
    pub total_count: usize,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct MobileRevisionEntry {
    pub object_cid_hex: String,
    pub predecessor_cid_hex: Option<String>,
    pub saved_seq: u64,
    pub title: String,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct MobileKuDetailView {
    pub summary: MobileKuSummary,
    pub source_text: Option<String>,
    pub statement_count: usize,
    pub concept_ccids_hex: Vec<String>,
    pub alternate_encoding_cids: Vec<String>,
    pub revision_lineage: Vec<MobileRevisionEntry>,
    pub fidelity: MobileFidelityStatusView,
    pub workflow_stages: Vec<WorkflowStageView>,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct NeighborhoodNodeView {
    pub node_id: String,
    pub label: String,
    pub kind: String,
    pub x_norm: i32,
    pub y_norm: i32,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct NeighborhoodEdgeView {
    pub from_id: String,
    pub to_id: String,
    pub predicate_label: String,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct LocalNeighborhoodProjection {
    pub center_cid_hex: String,
    pub nodes: Vec<NeighborhoodNodeView>,
    pub edges: Vec<NeighborhoodEdgeView>,
    pub bounded_local_only: bool,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct LocalReadOnlyKqlView {
    pub query: String,
    pub total_count: usize,
    pub columns: Vec<String>,
    pub formatted_rows: Vec<String>,
    pub read_only_enforced: bool,
}

pub struct MobileKnowledgeEngine {
    workspace: SyncKuWorkspace,
    fidelity: RedbFidelityPortfolio,
    composition: RedbCompositionStore,
    meta_db: Database,
    cipher: XChaCha20Poly1305,
}

impl MobileKnowledgeEngine {
    pub fn open(
        ku_vault_path: &Path,
        ku_journal_path: &Path,
        fidelity_path: &Path,
        composition_path: &Path,
        meta_path: &Path,
        vault_key: &[u8; 32],
    ) -> Result<Self, MobileCoreError> {
        let ku_key = blake3::derive_key("onebrain:mobile:ku-workspace-vault-key:1", vault_key);
        let workspace = SyncKuWorkspace::open(ku_vault_path, ku_journal_path, ku_key)
            .map_err(|error| MobileCoreError::Storage(format!("open SyncKuWorkspace: {error}")))?;
        let fidelity = RedbFidelityPortfolio::open(fidelity_path).map_err(|error| {
            MobileCoreError::Storage(format!("open RedbFidelityPortfolio: {error}"))
        })?;
        let composition = RedbCompositionStore::open(composition_path).map_err(|error| {
            MobileCoreError::Storage(format!("open RedbCompositionStore: {error}"))
        })?;
        if let Some(parent) = meta_path.parent() {
            std::fs::create_dir_all(parent).map_err(|error| {
                MobileCoreError::Storage(format!(
                    "create knowledge meta dir {}: {error}",
                    parent.display()
                ))
            })?;
        }
        let meta_db = Database::create(meta_path)?;
        let write = meta_db.begin_write()?;
        {
            write.open_table(ENCODING_DRAFTS)?;
            write.open_table(KU_LOCAL_ORG)?;
        }
        write.commit()?;
        let meta_key = KnowledgeMetaKey::derive(vault_key);
        Ok(Self {
            workspace,
            fidelity,
            composition,
            meta_db,
            cipher: XChaCha20Poly1305::new((&*meta_key.0).into()),
        })
    }

    pub fn composition_store(&self) -> &RedbCompositionStore {
        &self.composition
    }

    pub fn saved_ku_count(&self) -> usize {
        self.workspace.list().len()
    }

    /// Connect `draft.rs` (`MOB-CAP-001/002` Limited raw drafts) -> `MOB-ENC-001` encoding draft!
    #[allow(clippy::too_many_arguments)]
    pub fn create_encoding_draft_from_raw_draft(
        &self,
        readiness: &DerivedRegistryReadiness,
        draft_store: &PrivateDraftStore,
        raw_draft_ref: &str,
        title: &str,
        subject_label: &str,
        predicate_label: &str,
        object_label: &str,
        now_monotonic_ms: u64,
    ) -> Result<EncodingDraftRecord, MobileCoreError> {
        if !readiness.is_ready_for_canonical_encode() {
            return Err(MobileCoreError::RegistryNotReady(format!(
                "canonical KU encoding requires ReadyOffline Concept Registry (current: {})",
                readiness.as_snapshot_str()
            )));
        }
        let (_, decrypted_text) = draft_store
            .read_decrypted_text(raw_draft_ref)?
            .ok_or_else(|| MobileCoreError::InvalidArgument("raw draft_ref not found".into()))?;
        let resolved_title = if title.trim().is_empty() {
            decrypted_text
                .lines()
                .next()
                .unwrap_or("Private Knowledge Draft")
                .chars()
                .take(80)
                .collect()
        } else {
            title.trim().to_owned()
        };

        let mut id_bytes = [0u8; 16];
        getrandom::fill(&mut id_bytes)
            .map_err(|_| MobileCoreError::Security("encoding draft CSPRNG unavailable".into()))?;
        let record = EncodingDraftRecord {
            encoding_draft_id: format!("encdraft_{}", hex::encode(id_bytes)),
            source_raw_draft_ref: Some(raw_draft_ref.to_owned()),
            title: resolved_title,
            subject_label: subject_label.trim().to_owned(),
            predicate_label: predicate_label.trim().to_owned(),
            object_label: object_label.trim().to_owned(),
            source_text: decrypted_text.as_str().to_owned(),
            predecessor_cid_hex: None,
            alternate_of_cid_hex: None,
            last_prepare_id: None,
            saved_object_cid_hex: None,
            created_at_monotonic_ms: now_monotonic_ms,
        };
        self.write_encoding_draft(&record)?;
        Ok(record)
    }

    /// Stage and validate a canonical KU candidate against the active `IndexedConceptRegistry`
    /// (`MOB-ENC-001` / `MOB-ENC-002`). Strictly gated on `ReadyOffline`.
    #[allow(clippy::too_many_arguments)]
    pub fn prepare_canonical_triple(
        &self,
        readiness: &DerivedRegistryReadiness,
        registry: &dyn ConceptLookup,
        title: &str,
        subject_label: &str,
        predicate_label: &str,
        object_label: &str,
        source_text: Option<&str>,
        predecessor_cid_hex: Option<&str>,
    ) -> Result<PreparedKuPreviewView, MobileCoreError> {
        if !readiness.is_ready_for_canonical_encode() {
            return Err(MobileCoreError::RegistryNotReady(format!(
                "canonical KU prepare requires ReadyOffline (current: {})",
                readiness.as_snapshot_str()
            )));
        }
        let predecessor = predecessor_cid_hex.map(parse_object_cid_hex).transpose()?;
        let prepared = self
            .workspace
            .prepare_manual_triple(
                registry,
                title,
                subject_label,
                predicate_label,
                object_label,
                source_text,
                predecessor,
            )
            .map_err(|error| MobileCoreError::InvalidArgument(error.to_string()))?;
        Ok(to_preview_view(&prepared))
    }

    pub fn preview_prepared_ku(
        &self,
        prepare_id: &str,
    ) -> Result<PreparedKuPreviewView, MobileCoreError> {
        let prepared = self
            .workspace
            .preview_prepared(prepare_id)
            .map_err(|error| MobileCoreError::InvalidArgument(error.to_string()))?;
        Ok(to_preview_view(&prepared))
    }

    /// Commit a reviewed prepared KU as an immutable `LOCAL_ONLY` private KU (`MOB-ENC-002/003`),
    /// record its local publisher `EncodingAttempt` in `RedbFidelityPortfolio` (`MOB-FID-001`),
    /// and project its virtual affordance in `RedbCompositionStore` (`MOB-KNO-004`).
    pub fn save_prepared_ku(
        &mut self,
        prepare_id: &str,
        alternate_of_cid_hex: Option<&str>,
        source_raw_draft_ref: Option<&str>,
    ) -> Result<MobileKuDetailView, MobileCoreError> {
        let saved = self
            .workspace
            .save_prepared(prepare_id)
            .map_err(|error| MobileCoreError::Storage(format!("save_prepared failed: {error}")))?;
        let object_cid_hex = hex::encode(saved.object_cid);

        // Register fidelity target & publisher EncodingAttempt -> SELF_ATTESTED relative status!
        let source_bytes = saved
            .source_text_record_cid
            .unwrap_or(saved.semantic_content_cid);
        let policy_ref = ObjectReference::new(0, DEFAULT_FIDELITY_POLICY_CID);
        let source_ref = ObjectReference::new(0, source_bytes);
        let encoding_ref = ObjectReference::new(0, saved.object_cid);

        let target_key = self
            .fidelity
            .register_target(
                policy_ref,
                FidelityPolicy::default_v1(),
                source_ref.clone(),
                encoding_ref.clone(),
                EventCid::from_bytes(saved.semantic_content_cid),
            )
            .map_err(|error| {
                MobileCoreError::Storage(format!("register fidelity target: {error}"))
            })?;

        let attempt_id_hash = *blake3::hash(&saved.object_cid).as_bytes();
        self.fidelity
            .record_publisher_attempt(
                target_key,
                ObjectReference::new(0, attempt_id_hash),
                EncodingAttempt {
                    role: EncodingAttemptRole::Publisher,
                    source_artifact: source_ref,
                    candidate_encoding: Some(encoding_ref.clone()),
                    output_commitment: saved.semantic_content_cid,
                    pipeline_model_tool_commitments: vec![
                        *blake3::hash(b"mobile:sync-ku-v1").as_bytes()
                    ],
                    source_acquisition_or_derivation_commitment: source_bytes,
                    execution_record_ref: ObjectReference::new(0, attempt_id_hash),
                    blind_session_commitment: None,
                    challenge_nonce_commitment: None,
                },
            )
            .map_err(|error| {
                MobileCoreError::Storage(format!("record publisher fidelity attempt: {error}"))
            })?;

        // Project virtual SFS affordance into private composition store (`MOB-KNO-004`).
        if let Ok(detail) = self.workspace.get(ObjectCid::from_bytes(saved.object_cid)) {
            let _ = self
                .composition
                .declare_reference_disclosure(&encoding_ref, DisclosureClass::LocalOnly);
            let _ = self.composition.project_and_store_virtual_affordance(
                encoding_ref,
                &detail.semantic,
                DisclosureClass::LocalOnly,
            );
        }

        let mut org =
            self.read_local_org(&object_cid_hex)?
                .unwrap_or_else(|| KuLocalOrganizationRecord {
                    object_cid_hex: object_cid_hex.clone(),
                    ..Default::default()
                });
        org.fidelity_target_key_hex = Some(hex::encode(target_key.as_bytes()));
        if let Some(raw_ref) = source_raw_draft_ref {
            org.source_raw_draft_ref = Some(raw_ref.to_owned());
        }
        if let Some(alt_hex) = alternate_of_cid_hex {
            let alt_cid = parse_object_cid_hex(alt_hex)?;
            let normalized_alt = hex::encode(alt_cid.as_bytes());
            if !org.alternate_encoding_cids.contains(&normalized_alt) {
                org.alternate_encoding_cids.push(normalized_alt.clone());
            }
            if let Some(mut peer_org) = self.read_local_org(&normalized_alt)? {
                if !peer_org.alternate_encoding_cids.contains(&object_cid_hex) {
                    peer_org
                        .alternate_encoding_cids
                        .push(object_cid_hex.clone());
                    self.write_local_org(&peer_org)?;
                }
            }
        }
        self.write_local_org(&org)?;

        self.inspect_ku_detail(&object_cid_hex)
    }

    pub fn update_local_organization(
        &self,
        object_cid_hex: &str,
        tags: Vec<String>,
        collections: Vec<String>,
    ) -> Result<KuLocalOrganizationRecord, MobileCoreError> {
        let cid = parse_object_cid_hex(object_cid_hex)?;
        let normalized_hex = hex::encode(cid.as_bytes());
        let _ = self
            .workspace
            .get(cid)
            .map_err(|error| MobileCoreError::InvalidArgument(error.to_string()))?;

        let mut clean_tags: Vec<String> = tags
            .into_iter()
            .map(|t| t.trim().to_owned())
            .filter(|t| !t.is_empty() && t.len() <= 64)
            .collect();
        clean_tags.sort();
        clean_tags.dedup();

        let mut clean_collections: Vec<String> = collections
            .into_iter()
            .map(|c| c.trim().to_owned())
            .filter(|c| !c.is_empty() && c.len() <= 64)
            .collect();
        clean_collections.sort();
        clean_collections.dedup();

        let mut org =
            self.read_local_org(&normalized_hex)?
                .unwrap_or_else(|| KuLocalOrganizationRecord {
                    object_cid_hex: normalized_hex.clone(),
                    ..Default::default()
                });
        org.tags = clean_tags;
        org.collections = clean_collections;
        self.write_local_org(&org)?;
        Ok(org)
    }

    pub fn list_paginated(
        &self,
        cursor_saved_seq_exclusive: Option<u64>,
        limit: usize,
        collection_filter: Option<&str>,
    ) -> Result<PaginatedKuPage, MobileCoreError> {
        let bounded = limit.clamp(1, 50);
        let all = self.workspace.list();
        let total_count = all.len();

        let mut filtered = Vec::new();
        for item in all {
            if let Some(cursor) = cursor_saved_seq_exclusive {
                if item.saved_seq >= cursor {
                    continue;
                }
            }
            let summary = self.to_summary(&item)?;
            if let Some(col) = collection_filter {
                if !col.is_empty() && !summary.collections.iter().any(|c| c == col) {
                    continue;
                }
            }
            filtered.push(summary);
        }

        let has_more = filtered.len() > bounded;
        filtered.truncate(bounded);
        let next_cursor_seq = if has_more {
            filtered.last().map(|last| last.saved_seq)
        } else {
            None
        };
        Ok(PaginatedKuPage {
            items: filtered,
            next_cursor_seq,
            total_count,
        })
    }

    pub fn inspect_ku_detail(
        &self,
        object_cid_hex: &str,
    ) -> Result<MobileKuDetailView, MobileCoreError> {
        let cid = parse_object_cid_hex(object_cid_hex)?;
        let normalized_hex = hex::encode(cid.as_bytes());
        let detail = self
            .workspace
            .get(cid)
            .map_err(|error| MobileCoreError::InvalidArgument(error.to_string()))?;
        let summary = self.to_summary(&detail.record)?;
        let org = self.read_local_org(&normalized_hex)?.unwrap_or_default();
        let lineage = self
            .workspace
            .revision_lineage(cid)
            .map_err(|error| MobileCoreError::Storage(error.to_string()))?
            .into_iter()
            .map(|entry| MobileRevisionEntry {
                object_cid_hex: hex::encode(entry.object_cid.as_bytes()),
                predecessor_cid_hex: entry.predecessor_cid.map(|p| hex::encode(p.as_bytes())),
                saved_seq: entry.saved_seq,
                title: entry.title,
            })
            .collect::<Vec<_>>();

        let mut concept_set = BTreeSet::new();
        for stmt in &detail.semantic.statements {
            concept_set.insert(hex::encode(stmt.operator_or_predicate.as_bytes()));
            for arg in &stmt.arguments {
                if let TermRef::Concept(ccid) = arg {
                    concept_set.insert(hex::encode(ccid.as_bytes()));
                }
            }
        }

        let fidelity = self.fidelity_status_for_record(&detail.record)?;
        let workflow_stages = vec![
            WorkflowStageView {
                stage_number: 1,
                stage_code: "CAPTURE_SOURCE".into(),
                status_code: if detail.source_text.is_some() {
                    "RECORDED_LOCAL"
                } else {
                    "MANUAL_ENTRY"
                }
                .into(),
                read_only_on_mobile_v1: true,
            },
            WorkflowStageView {
                stage_number: 2,
                stage_code: "CANONICAL_ENCODE".into(),
                status_code: "COMMITTED_LOCAL_ONLY".into(),
                read_only_on_mobile_v1: true,
            },
            WorkflowStageView {
                stage_number: 3,
                stage_code: "FIDELITY_PORTFOLIO".into(),
                status_code: fidelity.status_code.clone(),
                read_only_on_mobile_v1: true,
            },
            WorkflowStageView {
                stage_number: 4,
                stage_code: "VIRTUAL_AFFORDANCE_PROJECTION".into(),
                status_code: "PROJECTED_PRIVATE".into(),
                read_only_on_mobile_v1: true,
            },
            WorkflowStageView {
                stage_number: 5,
                stage_code: "RECEPTOR_ASSEMBLY_COMPOSITION".into(),
                status_code: "READ_ONLY_INSPECTION_ON_MOBILE".into(),
                read_only_on_mobile_v1: true,
            },
            WorkflowStageView {
                stage_number: 6,
                stage_code: "PUBLICATION_AND_REUNION".into(),
                status_code: "PRIVATE_NOT_SHARED_GATED".into(),
                read_only_on_mobile_v1: true,
            },
        ];

        Ok(MobileKuDetailView {
            summary,
            source_text: detail.source_text,
            statement_count: detail.semantic.statements.len(),
            concept_ccids_hex: concept_set.into_iter().collect(),
            alternate_encoding_cids: org.alternate_encoding_cids,
            revision_lineage: lineage,
            fidelity,
            workflow_stages,
        })
    }

    pub fn search_semantic(
        &self,
        query: &str,
        registry: Option<&dyn ConceptLookup>,
    ) -> Result<Vec<MobileKuSummary>, MobileCoreError> {
        let records = self
            .workspace
            .search(query, registry)
            .map_err(|error| MobileCoreError::InvalidArgument(error.to_string()))?;
        records.iter().map(|rec| self.to_summary(rec)).collect()
    }

    pub fn execute_read_only_kql(
        &self,
        query_text: &str,
    ) -> Result<LocalReadOnlyKqlView, MobileCoreError> {
        let result = self
            .workspace
            .execute_read_only_kql(query_text)
            .map_err(|error| MobileCoreError::LocalKql(error.to_string()))?;
        let columns = vec![
            "gene_type".to_owned(),
            "text".to_owned(),
            "trust_score".to_owned(),
            "instruction_count".to_owned(),
        ];
        let formatted_rows = result
            .rows
            .iter()
            .take(64)
            .map(|row| {
                let mut map = serde_json::Map::new();
                for field in &columns {
                    if let Some(val) = row.extract_field(field) {
                        map.insert(field.clone(), serde_json::Value::String(format!("{val:?}")));
                    }
                }
                if let Some(expr) = &row.expr {
                    map.insert("title".into(), serde_json::Value::String(expr.text.clone()));
                }
                serde_json::Value::Object(map).to_string()
            })
            .collect();
        Ok(LocalReadOnlyKqlView {
            query: query_text.trim().to_owned(),
            total_count: result.total_count,
            columns,
            formatted_rows,
            read_only_enforced: true,
        })
    }

    /// Bounded 2D local neighborhood projection (`MOB-LIB-006`).
    pub fn project_local_neighborhood(
        &self,
        object_cid_hex: &str,
        registry: Option<&dyn ConceptLookup>,
    ) -> Result<LocalNeighborhoodProjection, MobileCoreError> {
        let cid = parse_object_cid_hex(object_cid_hex)?;
        let detail = self
            .workspace
            .get(cid)
            .map_err(|error| MobileCoreError::InvalidArgument(error.to_string()))?;

        let mut nodes = Vec::new();
        let mut edges = Vec::new();
        let center_id = hex::encode(cid.as_bytes());
        nodes.push(NeighborhoodNodeView {
            node_id: center_id.clone(),
            label: detail.record.title.clone(),
            kind: "KnowledgeUnit".into(),
            x_norm: 0,
            y_norm: 0,
        });

        let mut seen_nodes = BTreeMap::new();
        seen_nodes.insert(center_id.clone(), 0usize);

        let mut concept_index = 0i32;
        for stmt in &detail.semantic.statements {
            let pred_label = resolve_ccid_label(registry, stmt.operator_or_predicate.as_bytes());
            for arg in &stmt.arguments {
                if let TermRef::Concept(ccid) = arg {
                    let node_id = hex::encode(ccid.as_bytes());
                    if !seen_nodes.contains_key(&node_id) && nodes.len() < 24 {
                        concept_index += 1;
                        let angle_slot = concept_index % 8;
                        let (dx, dy) = match angle_slot {
                            0 => (100, 0),
                            1 => (70, 70),
                            2 => (0, 100),
                            3 => (-70, 70),
                            4 => (-100, 0),
                            5 => (-70, -70),
                            6 => (0, -100),
                            _ => (70, -70),
                        };
                        seen_nodes.insert(node_id.clone(), nodes.len());
                        nodes.push(NeighborhoodNodeView {
                            node_id: node_id.clone(),
                            label: resolve_ccid_label(registry, ccid.as_bytes()),
                            kind: "Concept".into(),
                            x_norm: dx,
                            y_norm: dy,
                        });
                    }
                    edges.push(NeighborhoodEdgeView {
                        from_id: center_id.clone(),
                        to_id: node_id,
                        predicate_label: pred_label.clone(),
                    });
                }
            }
        }

        Ok(LocalNeighborhoodProjection {
            center_cid_hex: center_id,
            nodes,
            edges,
            bounded_local_only: true,
        })
    }

    #[allow(clippy::type_complexity)]
    pub fn export_all_for_backup(
        &self,
    ) -> Result<
        Vec<(
            SavedPrivateKuRecord,
            Vec<u8>,
            Option<String>,
            KuLocalOrganizationRecord,
        )>,
        MobileCoreError,
    > {
        let mut out = Vec::new();
        for rec in self.workspace.list() {
            let cid = ObjectCid::from_bytes(rec.object_cid);
            let detail = self
                .workspace
                .get(cid)
                .map_err(|error| MobileCoreError::Storage(error.to_string()))?;
            let hex_id = hex::encode(rec.object_cid);
            let org = self
                .read_local_org(&hex_id)?
                .unwrap_or_else(|| KuLocalOrganizationRecord {
                    object_cid_hex: hex_id,
                    ..Default::default()
                });
            out.push((rec, detail.canonical_object_bytes, detail.source_text, org));
        }
        // Sort ascending by saved_seq so predecessors restore before successors!
        out.sort_by_key(|(rec, _, _, _)| rec.saved_seq);
        Ok(out)
    }

    pub fn restore_ku_from_backup(
        &mut self,
        registry: &dyn ConceptLookup,
        title: &str,
        canonical_object_bytes: &[u8],
        source_text: Option<&str>,
        predecessor_cid: Option<[u8; 32]>,
        org: &KuLocalOrganizationRecord,
    ) -> Result<String, MobileCoreError> {
        let decoded = ku_core::foundation::decode_knowledge_object(
            canonical_object_bytes,
            ku_core::foundation::ResourceProfile::ObjectV1,
            &[ku_core::foundation::KnownObjectKind::new(
                ku_core::foundation::SEMANTIC_KERNEL_OBJECT_KIND,
                1,
            )],
            &[],
        )
        .map_err(|error| {
            MobileCoreError::Archive(format!("invalid KU object in backup: {error}"))
        })?;
        let ku_core::foundation::ObjectSemantics::Known(envelope) = decoded.semantics() else {
            return Err(MobileCoreError::Archive(
                "backup KU object is not a Known SemanticKernel".into(),
            ));
        };
        let sfs = ku_core::foundation::SemanticFrameSet::from_canonical_value(&envelope.payload)
            .map_err(|error| {
                MobileCoreError::Archive(format!("invalid SFS payload in backup: {error}"))
            })?;
        let pred = predecessor_cid.map(ObjectCid::from_bytes);
        let prepared = self
            .workspace
            .prepare_semantic_draft(registry, title, &sfs, source_text, pred)
            .map_err(|error| {
                MobileCoreError::Archive(format!("prepare restored KU failed: {error}"))
            })?;
        let detail = self.save_prepared_ku(
            &prepared.prepare_id,
            None,
            org.source_raw_draft_ref.as_deref(),
        )?;
        let _ = self.update_local_organization(
            &detail.summary.object_cid_hex,
            org.tags.clone(),
            org.collections.clone(),
        )?;
        Ok(detail.summary.object_cid_hex)
    }

    fn fidelity_status_for_record(
        &self,
        record: &SavedPrivateKuRecord,
    ) -> Result<MobileFidelityStatusView, MobileCoreError> {
        let source_bytes = record
            .source_text_record_cid
            .unwrap_or(record.semantic_content_cid);
        let target_key = FidelityPortfolioTargetKey::derive(
            &ObjectReference::new(0, DEFAULT_FIDELITY_POLICY_CID),
            &ObjectReference::new(0, source_bytes),
            &ObjectReference::new(0, record.object_cid),
        );
        match self.fidelity.status_view(target_key) {
            Ok(view) => {
                let status_code = match view.relative_status() {
                    Some(FidelityAssessmentStatus::SelfAttested) => "SELF_ATTESTED".to_owned(),
                    Some(FidelityAssessmentStatus::PartiallyCorroborated) => {
                        "PARTIALLY_CORROBORATED".to_owned()
                    }
                    Some(FidelityAssessmentStatus::FidelityCorroboratedRelative) => {
                        "FIDELITY_CORROBORATED_RELATIVE".to_owned()
                    }
                    None => "UNASSESSED_LOCAL".to_owned(),
                };
                let legacy_claim_count = view
                    .assessment
                    .as_ref()
                    .map_or(0, |a| a.coverage.normalized_legacy_claims);
                Ok(MobileFidelityStatusView {
                    target_key_hex: hex::encode(target_key.as_bytes()),
                    status_code,
                    distinct_4d_corroboration_groups: view
                        .corroboration_4d
                        .distinct_non_overlapping_group_count,
                    legacy_claim_count: u32::try_from(legacy_claim_count).unwrap_or(u32::MAX),
                    is_global_or_final: view.is_global_or_final(),
                    establishes_proposition_truth: view.establishes_proposition_truth(),
                    blocks_preserve_or_query: view.blocks_preserve_publish_query_or_use(),
                })
            }
            Err(_) => Ok(MobileFidelityStatusView {
                target_key_hex: hex::encode(target_key.as_bytes()),
                status_code: "UNASSESSED_LOCAL".into(),
                distinct_4d_corroboration_groups: 0,
                legacy_claim_count: 0,
                is_global_or_final: false,
                establishes_proposition_truth: false,
                blocks_preserve_or_query: false,
            }),
        }
    }

    fn to_summary(
        &self,
        record: &SavedPrivateKuRecord,
    ) -> Result<MobileKuSummary, MobileCoreError> {
        let hex_id = hex::encode(record.object_cid);
        let org = self.read_local_org(&hex_id)?.unwrap_or_default();
        let fidelity = self.fidelity_status_for_record(record)?;
        let has_revisions = record.predecessor_cid.is_some()
            || self
                .workspace
                .revision_lineage(ObjectCid::from_bytes(record.object_cid))
                .map(|lin| lin.len() > 1)
                .unwrap_or(false);
        Ok(MobileKuSummary {
            object_cid_hex: hex_id,
            semantic_content_cid_hex: hex::encode(record.semantic_content_cid),
            title: record.title.clone(),
            saved_seq: record.saved_seq,
            disclosure: record.disclosure.clone(),
            // Explicit architecture invariant (`MOB-LIB-005`): never infer author from local save or generic Feed events!
            author_status: "Unresolved".into(),
            fidelity_status: fidelity.status_code,
            tags: org.tags,
            collections: org.collections,
            has_revisions,
        })
    }

    fn write_encoding_draft(&self, record: &EncodingDraftRecord) -> Result<(), MobileCoreError> {
        let plaintext = Zeroizing::new(serde_json::to_vec(record)?);
        let sealed = self.seal(ENCODING_DRAFT_AAD, &record.encoding_draft_id, &plaintext)?;
        let write = self.meta_db.begin_write()?;
        {
            let mut table = write.open_table(ENCODING_DRAFTS)?;
            table.insert(record.encoding_draft_id.as_str(), sealed.as_slice())?;
        }
        write.commit()?;
        Ok(())
    }

    fn read_local_org(
        &self,
        object_cid_hex: &str,
    ) -> Result<Option<KuLocalOrganizationRecord>, MobileCoreError> {
        let read = self.meta_db.begin_read()?;
        let table = read.open_table(KU_LOCAL_ORG)?;
        let Some(sealed) = table.get(object_cid_hex)? else {
            return Ok(None);
        };
        let plain = self.open_sealed(KU_ORG_AAD, object_cid_hex, sealed.value())?;
        Ok(Some(serde_json::from_slice(&plain)?))
    }

    fn write_local_org(&self, record: &KuLocalOrganizationRecord) -> Result<(), MobileCoreError> {
        let plain = Zeroizing::new(serde_json::to_vec(record)?);
        let sealed = self.seal(KU_ORG_AAD, &record.object_cid_hex, &plain)?;
        let write = self.meta_db.begin_write()?;
        {
            let mut table = write.open_table(KU_LOCAL_ORG)?;
            table.insert(record.object_cid_hex.as_str(), sealed.as_slice())?;
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
            .map_err(|_| MobileCoreError::Security("knowledge meta CSPRNG unavailable".into()))?;
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
            .map_err(|_| MobileCoreError::Security("cannot seal knowledge meta".into()))?;
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
                "knowledge meta envelope is invalid".into(),
            ));
        }
        let nonce = XNonce::try_from(&sealed[1..1 + NONCE_BYTES])
            .map_err(|_| MobileCoreError::Security("knowledge meta nonce is invalid".into()))?;
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
            .map_err(|_| MobileCoreError::Security("knowledge meta authentication failed".into()))
    }
}

fn parse_object_cid_hex(value: &str) -> Result<ObjectCid, MobileCoreError> {
    let bytes = hex::decode(value.trim())
        .map_err(|_| MobileCoreError::InvalidArgument("object CID must be canonical hex".into()))?;
    let array: [u8; 32] = bytes
        .try_into()
        .map_err(|_| MobileCoreError::InvalidArgument("object CID must be 32 bytes".into()))?;
    Ok(ObjectCid::from_bytes(array))
}

fn to_preview_view(prepared: &PreparedKuArtifact) -> PreparedKuPreviewView {
    PreparedKuPreviewView {
        prepare_id: prepared.prepare_id.clone(),
        title: prepared.title.clone(),
        candidate_object_cid_hex: hex::encode(prepared.candidate_object_cid),
        semantic_content_cid_hex: hex::encode(prepared.semantic_content_cid),
        canonical_preview_base64: prepared.canonical_preview_base64.clone(),
        redirect_rewrites: prepared
            .redirect_rewrites
            .iter()
            .map(|notice| {
                (
                    hex::encode(notice.deprecated_ccid),
                    hex::encode(notice.canonical_ccid),
                )
            })
            .collect(),
        predecessor_cid_hex: prepared.predecessor_cid.map(hex::encode),
        disclosure: "LOCAL_ONLY".into(),
        author_status: "Unresolved".into(),
    }
}

fn resolve_ccid_label(registry: Option<&dyn ConceptLookup>, ccid_bytes: &[u8]) -> String {
    if let (Some(reg), Ok(fixed)) = (registry, <&[u8; 16]>::try_from(ccid_bytes)) {
        if let Ok(Some(resolved)) = reg.describe_checked(fixed) {
            return resolved.canonical_name;
        }
    }
    hex::encode(ccid_bytes)
}

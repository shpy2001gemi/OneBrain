//! Synchronous Private KU Workspace (`ARCH-10`):
//! `PrivateVault<RedbVerifiedBackend>` + `ku_private_journal_v1` + `RebuildableSemanticIndex`
//! + `ConceptLookup` / `ConceptLifecycleTable` authoring gates + read-only local KQL (`ARCH-04`).

use std::collections::{BTreeMap, BTreeSet};
use std::path::Path;

use ku_core::concept_lifecycle::{AuthoringConceptResolution, ConceptLifecycleTable};
use ku_core::concept_registry::{ConceptLookup, ResolveResult};
use ku_core::core_dna::{CoreDna, CoreDnaHeader, Instruction};
use ku_core::epigenetics::Expression;
use ku_core::foundation::semantic_content::{normalize_semantic_content, SEMANTIC_CONTENT_PROFILE};
use ku_core::foundation::{
    decode_canonical, decode_knowledge_object, encode_canonical, ConceptCcid, ConstraintExpression,
    DimensionVector, DisclosureClass, KnowledgeObjectEnvelope, KnownObjectKind, LiteralValue,
    LocalSourceTextRecordV1, ObjectCid, ObjectReference, ObjectSemantics, PrivateVault,
    PutVerifiedOutcome, QuantityLiteral, RedbVerifiedBackend, ResourceProfile, SemanticContentCid,
    SemanticFrameSet, StatementFrame, StatementId, StatementQualifiers, TermRef, TypedConstraint,
    VaultKey, SEMANTIC_KERNEL_OBJECT_KIND,
};
use ku_core::{ConceptTableEntry, GeneType, KuRuntime};
use ku_kql::executor::{LocalExecutor, QueryResult};
use ku_kql::read_only::parse_local_read_only;
use ku_kql::vnext_semantic_index::{
    RebuildableSemanticIndex, SemanticIndexSnapshot, SemanticIndexSource,
};
use redb::{Database, ReadableTable, TableDefinition};
use serde::{Deserialize, Serialize};

use crate::base64_encode_standard;
use crate::error::CoreEngineError;

///Encrypted journal table storing `PrivateVault::seal_local_metadata`-protected workspace entries.
pub const KU_PRIVATE_JOURNAL_V1: TableDefinition<&[u8; 32], &[u8]> =
    TableDefinition::new("ku_private_journal_v1");

/// Prepared-draft staging table storing `PrivateVault::seal_local_metadata`-protected drafts.
pub const KU_PREPARED_DRAFTS_V1: TableDefinition<&str, &[u8]> =
    TableDefinition::new("ku_prepared_drafts_v1");

const JOURNAL_METADATA_DOMAIN: &[u8] = b"onebrain:core:ku-journal-entry:1\0";
const PREPARED_METADATA_DOMAIN: &[u8] = b"onebrain:core:ku-prepared-draft:1\0";

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct RedirectRewriteNotice {
    pub deprecated_ccid: [u8; 16],
    pub canonical_ccid: [u8; 16],
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct PreparedKuArtifact {
    pub prepare_id: String,
    pub title: String,
    pub source_text: Option<String>,
    pub semantic_bytes: Vec<u8>,
    pub semantic_content_cid: [u8; 32],
    pub candidate_object_bytes: Vec<u8>,
    pub candidate_object_cid: [u8; 32],
    pub canonical_preview_base64: String,
    pub redirect_rewrites: Vec<RedirectRewriteNotice>,
    pub predecessor_cid: Option<[u8; 32]>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct SavedPrivateKuRecord {
    pub object_cid: [u8; 32],
    pub semantic_content_cid: [u8; 32],
    pub title: String,
    pub source_text_record_cid: Option<[u8; 32]>,
    pub predecessor_cid: Option<[u8; 32]>,
    pub saved_seq: u64,
    pub disclosure: String,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PrivateKuDetail {
    pub record: SavedPrivateKuRecord,
    pub semantic: SemanticFrameSet,
    pub canonical_object_bytes: Vec<u8>,
    pub source_text: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RevisionLineageEntry {
    pub object_cid: ObjectCid,
    pub predecessor_cid: Option<ObjectCid>,
    pub saved_seq: u64,
    pub title: String,
}

/// Synchronous, socket-free, `tokio`-free private Knowledge Unit workspace (`ARCH-10`).
pub struct SyncKuWorkspace {
    vault: PrivateVault<RedbVerifiedBackend>,
    journal_db: Database,
    semantic_index: RebuildableSemanticIndex,
    entries: BTreeMap<[u8; 32], SavedPrivateKuRecord>,
    cached_semantics: BTreeMap<[u8; 32], SemanticFrameSet>,
    next_seq: u64,
}

impl SyncKuWorkspace {
    /// Opens (or creates) the encrypted `PrivateVault` database at `vault_db_path` and
    /// the local journal database at `journal_db_path`, then deterministically rebuilds
    /// `RebuildableSemanticIndex` from verified `LOCAL_ONLY` objects.
    pub fn open(
        vault_db_path: &Path,
        journal_db_path: &Path,
        vault_key_bytes: [u8; 32],
    ) -> Result<Self, CoreEngineError> {
        if vault_key_bytes == [0; 32] {
            return Err(CoreEngineError::InvalidInput("zero_vault_key"));
        }
        let backend = RedbVerifiedBackend::open(vault_db_path)
            .map_err(|e| CoreEngineError::Redb(format!("{e:?}")))?;
        let vault = PrivateVault::new(backend, VaultKey::from_bytes(vault_key_bytes));
        let journal_db = Database::create(journal_db_path)?;

        {
            let write_txn = journal_db.begin_write()?;
            let _ = write_txn.open_table(KU_PRIVATE_JOURNAL_V1)?;
            let _ = write_txn.open_table(KU_PREPARED_DRAFTS_V1)?;
            write_txn.commit()?;
        }

        let mut workspace = Self {
            vault,
            journal_db,
            semantic_index: RebuildableSemanticIndex::default(),
            entries: BTreeMap::new(),
            cached_semantics: BTreeMap::new(),
            next_seq: 1,
        };
        workspace.rebuild_from_storage()?;
        Ok(workspace)
    }

    pub fn index_snapshot(&self) -> SemanticIndexSnapshot {
        self.semantic_index.snapshot()
    }

    pub fn rebuild_from_storage(&mut self) -> Result<SemanticIndexSnapshot, CoreEngineError> {
        self.entries.clear();
        self.cached_semantics.clear();
        self.semantic_index.clear_derived();
        let mut max_seq = 0u64;

        let read_txn = self.journal_db.begin_read()?;
        let table = read_txn.open_table(KU_PRIVATE_JOURNAL_V1)?;
        for item in table.iter()? {
            let (cid_guard, sealed_guard) = item?;
            let cid_bytes = *cid_guard.value();
            let binding = metadata_binding(JOURNAL_METADATA_DOMAIN, &cid_bytes);
            let plaintext = self
                .vault
                .open_local_metadata(binding, sealed_guard.value())?;
            let record: SavedPrivateKuRecord = serde_json::from_slice(&plaintext)
                .map_err(|_| CoreEngineError::InvalidInput("corrupt_journal_record"))?;
            if record.object_cid != cid_bytes {
                return Err(CoreEngineError::InvalidInput("journal_cid_mismatch"));
            }
            let obj_cid = ObjectCid::from_bytes(cid_bytes);
            let Some(obj_bytes) = self.vault.get_object(obj_cid)? else {
                return Err(CoreEngineError::KuNotFound(obj_cid));
            };
            let semantic = decode_semantic_ku_object(&obj_bytes)?;
            self.semantic_index
                .insert(SemanticIndexSource::KnowledgeUnit {
                    cid: obj_cid,
                    semantic: &semantic,
                });
            max_seq = max_seq.max(record.saved_seq);
            self.cached_semantics.insert(cid_bytes, semantic);
            self.entries.insert(cid_bytes, record);
        }
        self.next_seq = max_seq.saturating_add(1);
        Ok(self.semantic_index.snapshot())
    }

    /// Resolves a manual `(subject_label, predicate_label, object_label)` triple through
    /// `ConceptLookup` + `ConceptLifecycleTable` and stages a prepared `LOCAL_ONLY` draft.
    #[allow(clippy::too_many_arguments)]
    pub fn prepare_manual_triple(
        &self,
        registry: &dyn ConceptLookup,
        title: &str,
        subject_label: &str,
        predicate_label: &str,
        object_label: &str,
        source_text: Option<&str>,
        predecessor: Option<ObjectCid>,
    ) -> Result<PreparedKuArtifact, CoreEngineError> {
        let subj = resolve_label_unique(registry, subject_label)?;
        let pred = resolve_label_unique(registry, predicate_label)?;
        let obj = resolve_label_unique(registry, object_label)?;

        let draft = SemanticFrameSet {
            statements: vec![StatementFrame {
                statement_id: StatementId(0),
                operator_or_predicate: pred,
                arguments: vec![TermRef::Concept(subj), TermRef::Concept(obj)],
                constraints: Vec::new(),
                qualifiers: StatementQualifiers::default(),
            }],
        };
        self.prepare_semantic_draft(registry, title, &draft, source_text, predecessor)
    }

    /// Validates a `SemanticFrameSet` against `ConceptLookup` lifecycle rules:
    /// - Canonical concepts pass unchanged;
    /// - Redirected concepts rewrite deterministically to their canonical target (`RedirectRewriteNotice`);
    /// - Split concepts fail closed with `CoreEngineError::SplitDisambiguationRequired`;
    /// - Retired concepts fail closed with `CoreEngineError::RetiredConcept`.
    ///
    /// Stages the encrypted prepared artifact in `KU_PREPARED_DRAFTS_V1` without saving to accepted storage.
    pub fn prepare_semantic_draft(
        &self,
        registry: &dyn ConceptLookup,
        title: &str,
        draft: &SemanticFrameSet,
        source_text: Option<&str>,
        predecessor: Option<ObjectCid>,
    ) -> Result<PreparedKuArtifact, CoreEngineError> {
        if title.trim().is_empty() || draft.statements.is_empty() {
            return Err(CoreEngineError::InvalidInput("empty_title_or_statements"));
        }
        if let Some(pred_cid) = predecessor {
            if !self.entries.contains_key(pred_cid.as_bytes()) {
                return Err(CoreEngineError::KuNotFound(pred_cid));
            }
        }

        let mut rewrites = Vec::new();
        let lifecycle_checked =
            enforce_lifecycle_on_frame_set(draft, registry.lifecycle_table(), &mut rewrites)?;
        let normalized = normalize_semantic_content(&lifecycle_checked, SEMANTIC_CONTENT_PROFILE)?;

        let envelope = build_private_semantic_envelope(&normalized.semantic, predecessor)?;
        let (candidate_object_bytes, candidate_object_cid) =
            envelope.encode(ResourceProfile::ObjectV1)?;

        let mut id_hasher = blake3::Hasher::new();
        id_hasher.update(b"onebrain:core:prepare-id:1\0");
        id_hasher.update(candidate_object_cid.as_bytes());
        id_hasher.update(title.trim().as_bytes());
        if let Some(src) = source_text {
            id_hasher.update(src.as_bytes());
        }
        let prepare_id = format!(
            "prep1.{}",
            hex::encode(&id_hasher.finalize().as_bytes()[..16])
        );

        let artifact = PreparedKuArtifact {
            prepare_id: prepare_id.clone(),
            title: title.trim().to_string(),
            source_text: source_text.map(str::to_string),
            semantic_bytes: normalized.canonical_bytes.clone(),
            semantic_content_cid: normalized.cid.into_bytes(),
            candidate_object_bytes: candidate_object_bytes.clone(),
            candidate_object_cid: candidate_object_cid.into_bytes(),
            canonical_preview_base64: base64_encode_standard(&candidate_object_bytes),
            redirect_rewrites: rewrites,
            predecessor_cid: predecessor.map(ObjectCid::into_bytes),
        };

        let serialized = serde_json::to_vec(&artifact)
            .map_err(|_| CoreEngineError::InvalidInput("prepare_serialize"))?;
        let binding = metadata_binding(PREPARED_METADATA_DOMAIN, prepare_id.as_bytes());
        let sealed = self.vault.seal_local_metadata(binding, &serialized)?;

        let write_txn = self.journal_db.begin_write()?;
        {
            let mut table = write_txn.open_table(KU_PREPARED_DRAFTS_V1)?;
            table.insert(prepare_id.as_str(), sealed.as_slice())?;
        }
        write_txn.commit()?;

        Ok(artifact)
    }

    /// Reads back a staged `PreparedKuArtifact` without mutating accepted KU storage.
    pub fn preview_prepared(
        &self,
        prepare_id: &str,
    ) -> Result<PreparedKuArtifact, CoreEngineError> {
        let read_txn = self.journal_db.begin_read()?;
        let table = read_txn.open_table(KU_PREPARED_DRAFTS_V1)?;
        let Some(sealed) = table.get(prepare_id)? else {
            return Err(CoreEngineError::PreparedDraftNotFound(
                prepare_id.to_string(),
            ));
        };
        let binding = metadata_binding(PREPARED_METADATA_DOMAIN, prepare_id.as_bytes());
        let plain = self.vault.open_local_metadata(binding, sealed.value())?;
        serde_json::from_slice(&plain)
            .map_err(|_| CoreEngineError::PreparedDraftMismatch("corrupt_prepared_draft"))
    }

    /// Commits a previously prepared draft as an immutable `DisclosureClass::LocalOnly`
    /// private Knowledge Unit inside `PrivateVault`, optionally storing encrypted
    /// `LocalSourceTextRecordV1` provenance, and updates `RebuildableSemanticIndex`.
    pub fn save_prepared(
        &mut self,
        prepare_id: &str,
    ) -> Result<SavedPrivateKuRecord, CoreEngineError> {
        let prepared = self.preview_prepared(prepare_id)?;
        let object_cid = ObjectCid::from_bytes(prepared.candidate_object_cid);

        if let Some(existing) = self.entries.get(&prepared.candidate_object_cid) {
            return Ok(existing.clone());
        }

        let outcome = self.vault.put_verified_object(
            object_cid,
            &prepared.candidate_object_bytes,
            ResourceProfile::ObjectV1,
            &[KnownObjectKind::new(SEMANTIC_KERNEL_OBJECT_KIND, 1)],
            &[],
        )?;
        if matches!(outcome, PutVerifiedOutcome::Quarantined { .. }) {
            return Err(CoreEngineError::PreparedDraftMismatch(
                "candidate_quarantined",
            ));
        }

        let source_text_record_cid = if let Some(src) = &prepared.source_text {
            let src_record = LocalSourceTextRecordV1::new(
                ObjectReference::new(0, object_cid.into_bytes()),
                src.clone(),
            )
            .map_err(|_| CoreEngineError::InvalidInput("source_text_bounds"))?;
            let (src_cid, _) = self.vault.put_source_text(&src_record)?;
            Some(src_cid.into_bytes())
        } else {
            None
        };

        let semantic = decode_semantic_ku_object(&prepared.candidate_object_bytes)?;
        let saved_seq = self.next_seq;
        self.next_seq = self.next_seq.saturating_add(1);

        let record = SavedPrivateKuRecord {
            object_cid: prepared.candidate_object_cid,
            semantic_content_cid: prepared.semantic_content_cid,
            title: prepared.title,
            source_text_record_cid,
            predecessor_cid: prepared.predecessor_cid,
            saved_seq,
            disclosure: "LOCAL_ONLY".to_string(),
        };

        let record_json = serde_json::to_vec(&record)
            .map_err(|_| CoreEngineError::InvalidInput("journal_serialize"))?;
        let binding = metadata_binding(JOURNAL_METADATA_DOMAIN, &record.object_cid);
        let sealed_record = self.vault.seal_local_metadata(binding, &record_json)?;

        let write_txn = self.journal_db.begin_write()?;
        {
            let mut j_table = write_txn.open_table(KU_PRIVATE_JOURNAL_V1)?;
            j_table.insert(&record.object_cid, sealed_record.as_slice())?;
            let mut p_table = write_txn.open_table(KU_PREPARED_DRAFTS_V1)?;
            let _ = p_table.remove(prepare_id)?;
        }
        write_txn.commit()?;

        self.semantic_index
            .insert(SemanticIndexSource::KnowledgeUnit {
                cid: object_cid,
                semantic: &semantic,
            });
        self.cached_semantics.insert(record.object_cid, semantic);
        self.entries.insert(record.object_cid, record.clone());
        Ok(record)
    }

    pub fn get(&self, cid: ObjectCid) -> Result<PrivateKuDetail, CoreEngineError> {
        let Some(record) = self.entries.get(cid.as_bytes()).cloned() else {
            return Err(CoreEngineError::KuNotFound(cid));
        };
        let Some(canonical_object_bytes) = self.vault.get_object(cid)? else {
            return Err(CoreEngineError::KuNotFound(cid));
        };
        let semantic = match self.cached_semantics.get(cid.as_bytes()) {
            Some(cached) => cached.clone(),
            None => decode_semantic_ku_object(&canonical_object_bytes)?,
        };
        let source_text = match record.source_text_record_cid {
            Some(src_cid) => self
                .vault
                .get_source_text(ObjectCid::from_bytes(src_cid))?
                .map(|r| r.source_text.as_str().to_string()),
            None => None,
        };
        Ok(PrivateKuDetail {
            record,
            semantic,
            canonical_object_bytes,
            source_text,
        })
    }

    /// Deterministic listing ordered by `(saved_seq DESC, object_cid ASC)`.
    pub fn list(&self) -> Vec<SavedPrivateKuRecord> {
        let mut out: Vec<SavedPrivateKuRecord> = self.entries.values().cloned().collect();
        out.sort_by(|a, b| {
            b.saved_seq
                .cmp(&a.saved_seq)
                .then_with(|| a.object_cid.cmp(&b.object_cid))
        });
        out
    }

    /// Walks local revision lineage (ancestors + direct/transitive successors) for `cid`,
    /// preserving all immutable revisions without overwriting predecessors.
    pub fn revision_lineage(
        &self,
        cid: ObjectCid,
    ) -> Result<Vec<RevisionLineageEntry>, CoreEngineError> {
        if !self.entries.contains_key(cid.as_bytes()) {
            return Err(CoreEngineError::KuNotFound(cid));
        }
        // Walk backwards to root ancestor
        let mut root = *cid.as_bytes();
        let mut visited = BTreeSet::new();
        while let Some(rec) = self.entries.get(&root) {
            if !visited.insert(root) {
                break;
            }
            if let Some(pred) = rec.predecessor_cid {
                if self.entries.contains_key(&pred) {
                    root = pred;
                    continue;
                }
            }
            break;
        }

        // Collect all reachable descendants from `root`
        let mut lineage_cids = BTreeSet::new();
        let mut queue = vec![root];
        while let Some(cur) = queue.pop() {
            if lineage_cids.insert(cur) {
                for rec in self.entries.values() {
                    if rec.predecessor_cid == Some(cur) {
                        queue.push(rec.object_cid);
                    }
                }
            }
        }

        let mut out: Vec<RevisionLineageEntry> = lineage_cids
            .into_iter()
            .filter_map(|k| self.entries.get(&k))
            .map(|r| RevisionLineageEntry {
                object_cid: ObjectCid::from_bytes(r.object_cid),
                predecessor_cid: r.predecessor_cid.map(ObjectCid::from_bytes),
                saved_seq: r.saved_seq,
                title: r.title.clone(),
            })
            .collect();
        out.sort_by(|a, b| {
            a.saved_seq
                .cmp(&b.saved_seq)
                .then_with(|| a.object_cid.as_bytes().cmp(b.object_cid.as_bytes()))
        });
        Ok(out)
    }

    /// Structured semantic search supporting:
    /// - `concept:<32-hex-ccid | label>` (expanded across `ConceptLifecycleTable::equivalence_classes()`)
    /// - `predicate:<32-hex-ccid | label>`
    /// - `unit:<32-hex-ccid | label>`
    /// - `dimension:L,M,T,I,Theta,N,J`
    /// - bare label terms resolved through `ConceptLookup` + title substring fallback.
    ///
    /// Multiple space-separated clauses are intersected (`AND`).
    pub fn search(
        &self,
        query: &str,
        registry: Option<&dyn ConceptLookup>,
    ) -> Result<Vec<SavedPrivateKuRecord>, CoreEngineError> {
        let trimmed = query.trim();
        if trimmed.is_empty() {
            return Ok(self.list());
        }

        let equiv_map = registry
            .and_then(ConceptLookup::lifecycle_table)
            .map(ConceptLifecycleTable::equivalence_classes)
            .unwrap_or_default();

        let mut running_filter: Option<BTreeSet<[u8; 32]>> = None;
        for token in trimmed.split_whitespace() {
            let matched = if let Some(spec) = token.strip_prefix("concept:") {
                let ccids = resolve_query_ccids(spec, registry, &equiv_map)?;
                let mut set = BTreeSet::new();
                for c in ccids {
                    for cid in self.semantic_index.objects_for_concept(c) {
                        set.insert(cid.into_bytes());
                    }
                }
                set
            } else if let Some(spec) = token.strip_prefix("predicate:") {
                let ccids = resolve_query_ccids(spec, registry, &equiv_map)?;
                let mut set = BTreeSet::new();
                for c in ccids {
                    for cid in self.semantic_index.objects_for_predicate(c) {
                        set.insert(cid.into_bytes());
                    }
                }
                set
            } else if let Some(spec) = token.strip_prefix("unit:") {
                let ccids = resolve_query_ccids(spec, registry, &equiv_map)?;
                let mut set = BTreeSet::new();
                for c in ccids {
                    for cid in self.semantic_index.objects_for_unit(c) {
                        set.insert(cid.into_bytes());
                    }
                }
                set
            } else if let Some(spec) = token.strip_prefix("dimension:") {
                let dim = parse_dimension_spec(spec)?;
                self.semantic_index
                    .objects_for_dimension(dim)
                    .into_iter()
                    .map(ObjectCid::into_bytes)
                    .collect()
            } else {
                let mut set = BTreeSet::new();
                if let Ok(ccids) = resolve_query_ccids(token, registry, &equiv_map) {
                    for c in ccids {
                        for cid in self.semantic_index.objects_for_concept(c) {
                            set.insert(cid.into_bytes());
                        }
                    }
                }
                let lower = token.to_lowercase();
                for (cid, rec) in &self.entries {
                    if rec.title.to_lowercase().contains(&lower) {
                        set.insert(*cid);
                    }
                }
                set
            };

            running_filter = Some(match running_filter {
                None => matched,
                Some(prev) => prev.intersection(&matched).copied().collect(),
            });
        }

        let allowed = running_filter.unwrap_or_default();
        let mut results: Vec<SavedPrivateKuRecord> = allowed
            .into_iter()
            .filter_map(|cid| self.entries.get(&cid).cloned())
            .collect();
        results.sort_by(|a, b| {
            b.saved_seq
                .cmp(&a.saved_seq)
                .then_with(|| a.object_cid.cmp(&b.object_cid))
        });
        Ok(results)
    }

    /// Executes a local read-only KQL query (`FIND` / `EXPLAIN FIND`) gated by
    /// `ku_kql::read_only::parse_local_read_only`. Rejects `CREATE`, `UPDATE`,
    /// `DEPRECATE`, and `WATCH` fail-closed.
    pub fn execute_read_only_kql(&self, query_text: &str) -> Result<QueryResult, CoreEngineError> {
        let parsed = parse_local_read_only(query_text)
            .map_err(|e| CoreEngineError::KqlRejected(e.message))?;
        let mut executor = LocalExecutor::new();
        for rec in self.list() {
            if let Some(sem) = self.cached_semantics.get(&rec.object_cid) {
                executor.insert(project_ku_runtime_for_read_only_kql(&rec, sem));
            }
        }
        executor
            .execute(&parsed)
            .map_err(|e| CoreEngineError::KqlExecution(e.to_string()))
    }
}

fn project_ku_runtime_for_read_only_kql(
    record: &SavedPrivateKuRecord,
    semantic: &SemanticFrameSet,
) -> KuRuntime {
    let mut ccids: Vec<[u8; 16]> = Vec::new();
    let mut push_ccid = |c: ConceptCcid| -> u64 {
        let b = *c.as_bytes();
        if let Some(pos) = ccids.iter().position(|x| *x == b) {
            pos as u64
        } else {
            let idx = ccids.len() as u64;
            ccids.push(b);
            idx
        }
    };

    let mut instructions = Vec::new();
    for stmt in &semantic.statements {
        let p_idx = push_ccid(stmt.operator_or_predicate);
        let subj_idx = stmt
            .arguments
            .first()
            .and_then(|t| match t {
                TermRef::Concept(c) => Some(push_ccid(*c)),
                _ => None,
            })
            .unwrap_or(p_idx);
        let obj_idx = stmt
            .arguments
            .get(1)
            .and_then(|t| match t {
                TermRef::Concept(c) => Some(push_ccid(*c)),
                _ => None,
            })
            .unwrap_or(subj_idx);
        instructions.push(Instruction::Triple {
            s: subj_idx,
            p: p_idx,
            o: obj_idx,
        });
    }

    let concept_table: Vec<ConceptTableEntry> = ccids
        .into_iter()
        .enumerate()
        .map(|(idx, ccid)| ConceptTableEntry {
            local_id: idx as u64,
            ccid,
        })
        .collect();
    let dna = CoreDna {
        header: CoreDnaHeader {
            version: ku_core::CORE_DNA_VERSION,
            gene_type: GeneType::Fact as u8,
            has_concept_table: !concept_table.is_empty(),
        },
        concept_table,
        instructions,
    };
    let mut ku = KuRuntime::new(dna, Vec::new());
    ku.expr = Some(Expression {
        text: record.title.clone(),
        lang: "und".to_string(),
        concept_names: Vec::new(),
    });
    ku
}

fn build_private_semantic_envelope(
    semantic: &SemanticFrameSet,
    predecessor: Option<ObjectCid>,
) -> Result<KnowledgeObjectEnvelope, CoreEngineError> {
    let mut envelope = semantic.to_knowledge_object(DisclosureClass::LocalOnly)?;
    if let Some(pred) = predecessor {
        envelope.references = vec![ObjectReference::new(0, pred.into_bytes())];
    }
    Ok(envelope)
}

fn decode_semantic_ku_object(bytes: &[u8]) -> Result<SemanticFrameSet, CoreEngineError> {
    let validated = decode_knowledge_object(
        bytes,
        ResourceProfile::ObjectV1,
        &[KnownObjectKind::new(SEMANTIC_KERNEL_OBJECT_KIND, 1)],
        &[],
    )?;
    let ObjectSemantics::Known(envelope) = validated.semantics() else {
        return Err(CoreEngineError::InvalidInput("unknown_ku_semantics"));
    };
    Ok(SemanticFrameSet::from_canonical_value(&envelope.payload)?)
}

fn resolve_label_unique(
    registry: &dyn ConceptLookup,
    label: &str,
) -> Result<ConceptCcid, CoreEngineError> {
    let trimmed = label.trim();
    match registry.resolve(trimmed) {
        ResolveResult::Found(c) => Ok(ConceptCcid::from_bytes(c.ccid)),
        ResolveResult::Fuzzy(_) => Err(CoreEngineError::UnresolvedConceptLabel {
            label: trimmed.to_string(),
            candidates: 1,
        }),
        ResolveResult::Ambiguous(list) => Err(CoreEngineError::UnresolvedConceptLabel {
            label: trimmed.to_string(),
            candidates: list.len(),
        }),
        ResolveResult::NotFound => Err(CoreEngineError::UnresolvedConceptLabel {
            label: trimmed.to_string(),
            candidates: 0,
        }),
    }
}

fn resolve_query_ccids(
    spec: &str,
    registry: Option<&dyn ConceptLookup>,
    equiv_map: &BTreeMap<ConceptCcid, BTreeSet<ConceptCcid>>,
) -> Result<Vec<ConceptCcid>, CoreEngineError> {
    let base_ccids: Vec<ConceptCcid> =
        if spec.len() == 32 && spec.bytes().all(|b| b.is_ascii_hexdigit()) {
            let raw = hex::decode(spec).map_err(|_| CoreEngineError::InvalidInput("ccid_hex"))?;
            let arr: [u8; 16] = raw
                .try_into()
                .map_err(|_| CoreEngineError::InvalidInput("ccid_hex"))?;
            vec![ConceptCcid::from_bytes(arr)]
        } else if let Some(reg) = registry {
            match reg.resolve(spec) {
                ResolveResult::Found(c) | ResolveResult::Fuzzy(c) => {
                    vec![ConceptCcid::from_bytes(c.ccid)]
                }
                ResolveResult::Ambiguous(list) => list
                    .into_iter()
                    .map(|c| ConceptCcid::from_bytes(c.ccid))
                    .collect(),
                ResolveResult::NotFound => Vec::new(),
            }
        } else {
            return Err(CoreEngineError::UnresolvedConceptLabel {
                label: spec.to_string(),
                candidates: 0,
            });
        };

    let mut expanded = BTreeSet::new();
    for ccid in base_ccids {
        expanded.insert(ccid);
        if let Some(reg) = registry {
            if let AuthoringConceptResolution::Redirected { to, .. } =
                reg.resolve_for_authoring(ccid)
            {
                expanded.insert(to);
                if let Some(cls) = equiv_map.get(&to) {
                    expanded.extend(cls.iter().copied());
                }
            }
        }
        if let Some(cls) = equiv_map.get(&ccid) {
            expanded.extend(cls.iter().copied());
        }
    }
    Ok(expanded.into_iter().collect())
}

fn parse_dimension_spec(spec: &str) -> Result<DimensionVector, CoreEngineError> {
    let parts: Vec<&str> = spec.split(',').collect();
    if parts.len() != 7 {
        return Err(CoreEngineError::InvalidInput(
            "dimension_requires_7_exponents",
        ));
    }
    let mut exps = [0i8; 7];
    for (idx, p) in parts.iter().enumerate() {
        exps[idx] = p
            .trim()
            .parse::<i8>()
            .map_err(|_| CoreEngineError::InvalidInput("invalid_dimension_exponent"))?;
    }
    Ok(DimensionVector::new(exps))
}

fn enforce_lifecycle_on_frame_set(
    draft: &SemanticFrameSet,
    lifecycle: Option<&ConceptLifecycleTable>,
    rewrites: &mut Vec<RedirectRewriteNotice>,
) -> Result<SemanticFrameSet, CoreEngineError> {
    let Some(table) = lifecycle else {
        return Ok(draft.clone());
    };
    let mut seen_rewrites = BTreeSet::new();
    let mut map_ccid = |ccid: ConceptCcid| -> Result<ConceptCcid, CoreEngineError> {
        match table.resolve_for_authoring(ccid) {
            AuthoringConceptResolution::Canonical(c) => Ok(c),
            AuthoringConceptResolution::Redirected { from, to } => {
                if seen_rewrites.insert((*from.as_bytes(), *to.as_bytes())) {
                    rewrites.push(RedirectRewriteNotice {
                        deprecated_ccid: *from.as_bytes(),
                        canonical_ccid: *to.as_bytes(),
                    });
                }
                Ok(to)
            }
            AuthoringConceptResolution::NeedsSplitDisambiguation {
                parent,
                branches,
                role,
            } => Err(CoreEngineError::SplitDisambiguationRequired {
                parent,
                branches,
                disambiguation_role: role,
            }),
            AuthoringConceptResolution::Retired { ccid, reason } => {
                Err(CoreEngineError::RetiredConcept { ccid, reason })
            }
        }
    };

    let mut out_statements = Vec::with_capacity(draft.statements.len());
    for stmt in &draft.statements {
        let op = map_ccid(stmt.operator_or_predicate)?;
        let mut args = Vec::with_capacity(stmt.arguments.len());
        for a in &stmt.arguments {
            args.push(rewrite_term(a, &mut map_ccid)?);
        }
        let mut constraints = Vec::with_capacity(stmt.constraints.len());
        for c in &stmt.constraints {
            constraints.push(rewrite_constraint(c, &mut map_ccid)?);
        }
        let qualifiers = StatementQualifiers {
            negated: stmt.qualifiers.negated,
            modality: stmt.qualifiers.modality,
            condition: stmt.qualifiers.condition,
            time: match &stmt.qualifiers.time {
                Some(t) => Some(rewrite_term(t, &mut map_ccid)?),
                None => None,
            },
            location: match &stmt.qualifiers.location {
                Some(t) => Some(rewrite_term(t, &mut map_ccid)?),
                None => None,
            },
            perspective: match &stmt.qualifiers.perspective {
                Some(t) => Some(rewrite_term(t, &mut map_ccid)?),
                None => None,
            },
            tolerance: match &stmt.qualifiers.tolerance {
                Some(q) => Some(rewrite_quantity(q, &mut map_ccid)?),
                None => None,
            },
            source_spans: stmt.qualifiers.source_spans.clone(),
        };
        out_statements.push(StatementFrame {
            statement_id: stmt.statement_id,
            operator_or_predicate: op,
            arguments: args,
            constraints,
            qualifiers,
        });
    }
    // Re-encode/decode canonical constraints so rewritten frames remain canonical!
    let mut rebuilt = SemanticFrameSet {
        statements: out_statements,
    };
    for s in &mut rebuilt.statements {
        let mut keyed = Vec::with_capacity(s.constraints.len());
        for c in s.constraints.drain(..) {
            let b = encode_canonical(&c.canonical_value(), ResourceProfile::ObjectV1)?;
            keyed.push((b, c));
        }
        keyed.sort_by(|a, b| a.0.cmp(&b.0));
        keyed.dedup_by(|a, b| a.0 == b.0);
        s.constraints = keyed.into_iter().map(|(_, c)| c).collect();
    }
    let _ = decode_canonical(&rebuilt.canonical_bytes()?, ResourceProfile::ObjectV1)?;
    let _ = SemanticContentCid::from_bytes([0; 32]);
    Ok(rebuilt)
}

fn rewrite_quantity(
    q: &QuantityLiteral,
    map_ccid: &mut impl FnMut(ConceptCcid) -> Result<ConceptCcid, CoreEngineError>,
) -> Result<QuantityLiteral, CoreEngineError> {
    let mut cloned = q.clone();
    cloned.source_unit.unit = map_ccid(q.source_unit.unit)?;
    Ok(cloned)
}

fn rewrite_term(
    term: &TermRef,
    map_ccid: &mut impl FnMut(ConceptCcid) -> Result<ConceptCcid, CoreEngineError>,
) -> Result<TermRef, CoreEngineError> {
    match term {
        TermRef::Concept(c) => Ok(TermRef::Concept(map_ccid(*c)?)),
        TermRef::Variable {
            id,
            type_constraint,
        } => Ok(TermRef::Variable {
            id: *id,
            type_constraint: match type_constraint {
                Some(c) => Some(map_ccid(*c)?),
                None => None,
            },
        }),
        TermRef::Receptor {
            slot,
            expected_type,
        } => Ok(TermRef::Receptor {
            slot: *slot,
            expected_type: match expected_type {
                Some(c) => Some(map_ccid(*c)?),
                None => None,
            },
        }),
        TermRef::Literal(LiteralValue::Quantity(q)) => Ok(TermRef::Literal(
            LiteralValue::Quantity(rewrite_quantity(q, map_ccid)?),
        )),
        other => Ok(other.clone()),
    }
}

fn rewrite_constraint(
    c: &TypedConstraint,
    map_ccid: &mut impl FnMut(ConceptCcid) -> Result<ConceptCcid, CoreEngineError>,
) -> Result<TypedConstraint, CoreEngineError> {
    let expression = match &c.expression {
        ConstraintExpression::Compare {
            left,
            operator,
            right,
        } => ConstraintExpression::Compare {
            left: rewrite_term(left, map_ccid)?,
            operator: *operator,
            right: rewrite_term(right, map_ccid)?,
        },
        ConstraintExpression::Range {
            term,
            lower,
            upper,
            include_lower,
            include_upper,
        } => ConstraintExpression::Range {
            term: rewrite_term(term, map_ccid)?,
            lower: rewrite_quantity(lower, map_ccid)?,
            upper: rewrite_quantity(upper, map_ccid)?,
            include_lower: *include_lower,
            include_upper: *include_upper,
        },
        ConstraintExpression::Dimension { term, expected } => ConstraintExpression::Dimension {
            term: rewrite_term(term, map_ccid)?,
            expected: *expected,
        },
    };
    Ok(TypedConstraint {
        expression,
        required: c.required,
    })
}

fn metadata_binding(domain: &[u8], key: &[u8]) -> [u8; 32] {
    let mut hasher = blake3::Hasher::new();
    hasher.update(domain);
    hasher.update(key);
    *hasher.finalize().as_bytes()
}

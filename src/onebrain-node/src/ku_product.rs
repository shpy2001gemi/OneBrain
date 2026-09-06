//! KU-RUN-001: node-owned local KU semantics behind authenticated Base sessions.
//!
//! PrivateVault remains the canonical acceptance owner. The encrypted journal
//! records the complete bundle before writes and publishes one commit marker
//! only after every record has survived acceptance. Reads require that marker.
use std::collections::{BTreeMap, BTreeSet, VecDeque};
use std::sync::{Arc, Mutex};

use base64::{engine::general_purpose::STANDARD, Engine};
use ku_core::foundation::semantic::SemanticFrameSet;
use ku_core::foundation::semantic_content::{normalize_semantic_content, SEMANTIC_CONTENT_PROFILE};
use ku_core::foundation::{
    decode_knowledge_object, DisclosureClass, KnownObjectKind, ObjectCid, ObjectKind,
    ObjectSemantics, PrivateVault, PutVerifiedOutcome, RedbVerifiedBackend, ResourceProfile,
    SourceArtifact, VaultKey,
};
use onebrain_base_contract::ku::*;
use onebrain_base_contract::ku_payload::{decode_hex, hex, KuPayload};
use onebrain_base_contract::{BaseErrorCodeV1, ResourceBudgetV1};
use redb::{Database, ReadableTable, TableDefinition};
use serde::{Deserialize, Serialize};

use crate::base_runtime::{BaseServiceError, BaseServices};
use crate::concept_registry_runtime::{
    ConceptRegistryGenerationManager, ConceptRegistryReaderLease,
};
use crate::dataset_path::{BaseStorageOwnerId, DatasetGenerationId, DatasetPathResolver};

const JOURNAL: TableDefinition<&[u8], &[u8]> = TableDefinition::new("ku_private_journal_v1");
const MAX_OPERATIONS: usize = 1024;
const MAX_JOURNAL_BYTES: usize = 64 * 1024 * 1024;
const MAX_SNAPSHOTS: usize = 32;
const MAX_CURSORS: usize = 1024;
pub(crate) const KU_PREPARED_MARKER: &[u8] = b"onebrain:ku:prepared:1\0";

/// Host-only source/encoder port. Ordinary product handles cannot install a port,
/// supply authority booleans, or bypass its current custody/consent assessment.
pub trait KuInputProvider: Send + Sync {
    fn implementation(&self, mode: InputMode) -> Option<[u8; 32]>;
    fn check_access(
        &self,
        principal: [u8; 32],
        sources: &[[u8; 32]],
    ) -> Result<(), BaseServiceError>;
    fn resolve(
        &self,
        principal: [u8; 32],
        request: &KuPrepareV1,
        registry: &ConceptRegistryReaderLease,
        budget: &ResourceBudgetV1,
    ) -> Result<KuResolvedInput, BaseServiceError>;
}

/// Resolved input is unaccepted producer output. The service still validates
/// every source object, CCID selection, semantic frame and output identity.
pub struct KuResolvedInput {
    pub drafts: Vec<SemanticFrameSet>,
    pub source_objects: Vec<Vec<u8>>,
    pub bindings: Vec<KuConceptBinding>,
}

#[derive(Clone, Serialize, Deserialize)]
pub struct KuConceptBinding {
    pub label: String,
    pub selected: Option<[u8; 16]>,
}

/// Read-only view of the already owned public acceptance store.
pub trait KuPublicReader: Send + Sync {
    fn get_public_object(&self, cid: [u8; 32]) -> Result<Option<Vec<u8>>, BaseServiceError>;
}

impl<B: ku_core::foundation::AtomicVerifiedBackend + Send> KuPublicReader
    for crate::vnext_validated_sink::SharedVNextValidatedSink<B>
{
    fn get_public_object(&self, cid: [u8; 32]) -> Result<Option<Vec<u8>>, BaseServiceError> {
        self.get_object(ObjectCid::from_bytes(cid))
            .map_err(|_| corrupt())
    }
}

pub struct KuRuntimeConfig {
    pub vault_key: VaultKey,
    pub registry: Option<Arc<ConceptRegistryGenerationManager>>,
    pub inputs: Arc<dyn KuInputProvider>,
    pub public: Option<Arc<dyn KuPublicReader>>,
}

#[derive(Clone)]
pub struct KuServices {
    pub(crate) base: BaseServices,
}

impl KuServices {
    pub async fn reserve(&self) -> Result<OperationId, BaseServiceError> {
        self.base.ku_reserve().await
    }
    pub async fn invoke(
        &self,
        request: KuRequestV1,
        budget: ResourceBudgetV1,
    ) -> Result<KuResponseV1, BaseServiceError> {
        request.validate().map_err(|_| invalid())?;
        self.base.ku_invoke(request, budget).await
    }
    pub async fn invoke_payload(
        &self,
        kind: u16,
        bytes: &[u8],
        budget: ResourceBudgetV1,
    ) -> Result<KuResponseV1, BaseServiceError> {
        if bytes.len() as u64 > budget.max_bytes {
            return Err(resource());
        }
        let request = KuRequestV1::decode(kind, bytes).map_err(|_| invalid())?;
        self.invoke(request, budget).await
    }
}

#[derive(Clone, Serialize, Deserialize)]
struct PreparedBundle {
    principal: [u8; 32],
    dataset: DatasetGenerationId,
    request: KuPrepareV1,
    predecessor: Option<ObjectCID>,
    expected_frontier: Option<RevisionFrontier>,
    preview: KuPreparedV1,
    sources: Vec<Vec<u8>>,
    /// Input bytes contain source spans; the accepted semantic payload does not.
    private_semantic_inputs: Vec<Vec<u8>>,
    original_statement_ids: Vec<Vec<u32>>,
    bindings: Vec<KuConceptBinding>,
    registry_builder: String,
    committed: bool,
    canceled: bool,
}

struct Snapshot {
    principal: [u8; 32],
    dataset: DatasetGenerationId,
    query: Option<String>,
    items: Vec<KuSummaryV1>,
    frontier: RevisionFrontier,
}

struct Cursor {
    snapshot: [u8; 32],
    last_cid: [u8; 32],
}

#[derive(Default)]
struct Catalog {
    keys: BTreeMap<[u8; 32], OperationId>,
    items: BTreeMap<[u8; 32], BTreeMap<[u8; 32], (KuSummaryV1, String)>>,
    revisions: BTreeMap<[u8; 32], BTreeSet<([u8; 32], [u8; 32])>>,
}

#[derive(Default)]
struct Paging {
    snapshots: BTreeMap<[u8; 32], Snapshot>,
    order: VecDeque<[u8; 32]>,
    cursors: BTreeMap<String, Cursor>,
}

pub(crate) struct KuStore {
    vault: PrivateVault<RedbVerifiedBackend>,
    journal: Database,
    dataset: DatasetGenerationId,
    inputs: Arc<dyn KuInputProvider>,
    registry: Option<Arc<ConceptRegistryGenerationManager>>,
    releases: Mutex<BTreeMap<[u8; 32], ConceptRegistryReaderLease>>,
    public: Option<Arc<dyn KuPublicReader>>,
    /// Serializes validation/revision checks with journal commit publication.
    mutation: Mutex<()>,
    paging: Mutex<Paging>,
    catalog: Mutex<Catalog>,
}

impl KuStore {
    pub(crate) fn open(
        paths: &dyn DatasetPathResolver,
        config: KuRuntimeConfig,
    ) -> Result<Self, BaseServiceError> {
        let root = paths
            .owner_path(BaseStorageOwnerId::VAULT)
            .map_err(|_| corrupt())?
            .join("ku-product-v1");
        std::fs::create_dir_all(&root).map_err(|_| unavailable())?;
        let backend =
            RedbVerifiedBackend::open(&root.join("objects.redb")).map_err(|_| corrupt())?;
        let vault = PrivateVault::new(backend, config.vault_key);
        let journal = Database::create(root.join("journal.redb")).map_err(|_| corrupt())?;
        let tx = journal.begin_write().map_err(|_| corrupt())?;
        {
            tx.open_table(JOURNAL).map_err(|_| corrupt())?;
        }
        tx.commit().map_err(|_| corrupt())?;
        let store = Self {
            vault,
            journal,
            dataset: paths.current_generation(),
            inputs: config.inputs,
            registry: config.registry,
            public: config.public,
            releases: Mutex::new(BTreeMap::new()),
            mutation: Mutex::new(()),
            paging: Mutex::new(Paging::default()),
            catalog: Mutex::new(Catalog::default()),
        };
        // Authentication failure, a damaged record or a partial committed bundle
        // is a typed open failure, never an empty store with reconstructed success.
        for bundle in store.bundles()? {
            store.validate_bundle(&bundle)?;
            if bundle.committed {
                store.verify_committed(&bundle)?;
            }
            store.index_bundle(&bundle)?;
        }
        Ok(store)
    }

    pub(crate) fn check_dataset(
        &self,
        current: DatasetGenerationId,
    ) -> Result<(), BaseServiceError> {
        if current != self.dataset {
            Err(conflict())
        } else {
            Ok(())
        }
    }

    fn bundle_key(&self, operation: OperationId) -> [u8; 32] {
        let mut h = blake3::Hasher::new_derive_key("onebrain:ku:private-journal-binding:1");
        h.update(&self.dataset.0);
        h.update(&operation.0);
        *h.finalize().as_bytes()
    }

    fn read_bundle(
        &self,
        operation: OperationId,
    ) -> Result<Option<PreparedBundle>, BaseServiceError> {
        let tx = self.journal.begin_read().map_err(|_| corrupt())?;
        let table = tx.open_table(JOURNAL).map_err(|_| corrupt())?;
        let value = table.get(operation.0.as_slice()).map_err(|_| corrupt())?;
        value
            .map(|v| self.decode_bundle(operation, v.value()))
            .transpose()
    }

    fn decode_bundle(
        &self,
        operation: OperationId,
        bytes: &[u8],
    ) -> Result<PreparedBundle, BaseServiceError> {
        let plaintext = zeroize::Zeroizing::new(
            self.vault
                .open_local_metadata(self.bundle_key(operation), bytes)
                .map_err(|_| corrupt())?,
        );
        let result: PreparedBundle = serde_json::from_slice(&plaintext).map_err(|_| corrupt())?;
        if result.request.operation_id != operation || result.dataset != self.dataset {
            return Err(corrupt());
        }
        Ok(result)
    }

    fn bundles(&self) -> Result<Vec<PreparedBundle>, BaseServiceError> {
        let tx = self.journal.begin_read().map_err(|_| corrupt())?;
        let table = tx.open_table(JOURNAL).map_err(|_| corrupt())?;
        let mut result = Vec::new();
        let mut total = 0usize;
        for row in table.iter().map_err(|_| corrupt())? {
            let (key, value) = row.map_err(|_| corrupt())?;
            total = total
                .checked_add(value.value().len())
                .ok_or_else(resource)?;
            if total > MAX_JOURNAL_BYTES || result.len() >= MAX_OPERATIONS {
                return Err(resource());
            }
            let operation = OperationId(key.value().try_into().map_err(|_| corrupt())?);
            result.push(self.decode_bundle(operation, value.value())?);
        }
        Ok(result)
    }

    fn write_bundle(&self, bundle: &PreparedBundle) -> Result<(), BaseServiceError> {
        let bytes = zeroize::Zeroizing::new(serde_json::to_vec(bundle).map_err(|_| corrupt())?);
        let sealed = self
            .vault
            .seal_local_metadata(self.bundle_key(bundle.request.operation_id), &bytes)
            .map_err(|_| resource())?;
        let tx = self.journal.begin_write().map_err(|_| unknown())?;
        {
            let mut table = tx.open_table(JOURNAL).map_err(|_| unknown())?;
            let mut count = 0;
            let mut total = sealed.len();
            for row in table.iter().map_err(|_| unknown())? {
                let (key, value) = row.map_err(|_| unknown())?;
                if key.value() != bundle.request.operation_id.0 {
                    count += 1;
                    total = total
                        .checked_add(value.value().len())
                        .ok_or_else(resource)?;
                }
            }
            if count >= MAX_OPERATIONS || total > MAX_JOURNAL_BYTES {
                return Err(resource());
            }
            table
                .insert(bundle.request.operation_id.0.as_slice(), sealed.as_slice())
                .map_err(|_| unknown())?;
        }
        tx.commit().map_err(|_| unknown())?;
        self.index_bundle(bundle)
    }

    fn index_bundle(&self, b: &PreparedBundle) -> Result<(), BaseServiceError> {
        let mut catalog = self.catalog.lock().map_err(|_| corrupt())?;
        if let Some(existing) = catalog
            .keys
            .insert(b.request.idempotency_key.0, b.request.operation_id)
        {
            if existing != b.request.operation_id {
                return Err(corrupt());
            }
        }
        if !b.committed {
            return Ok(());
        }
        for a in &b.preview.artifacts {
            let bytes = STANDARD
                .decode(&a.canonical_preview)
                .map_err(|_| corrupt())?;
            let text = semantic_text(&bytes)?;
            let summary = KuSummaryV1 {
                object_cid: a.object_cid,
                semantic_content_cid: Some(a.semantic_content_cid),
                disclosure_class: artifact_disclosure(disclosure(b.request.destination)),
                artifact_validity: ArtifactValidity::AcceptedKnown,
                coverage: Coverage::LocalOnly,
                limitations: vec!["fidelity_unassessed".into()],
                executable: false,
                fidelity_policy_cid: None,
                fidelity_frontier: None,
            };
            catalog
                .items
                .entry(b.principal)
                .or_default()
                .insert(a.object_cid.0, (summary, text));
            if let Some(predecessor) = b.predecessor {
                if predecessor != a.object_cid {
                    catalog
                        .revisions
                        .entry(b.principal)
                        .or_default()
                        .insert((predecessor.0, a.object_cid.0));
                }
            }
        }
        Ok(())
    }

    fn owned_bundle(
        &self,
        principal: [u8; 32],
        operation: OperationId,
    ) -> Result<PreparedBundle, BaseServiceError> {
        let b = self.read_bundle(operation)?.ok_or_else(not_found)?;
        if b.principal != principal {
            return Err(not_found());
        }
        self.validate_bundle(&b)?;
        Ok(b)
    }

    fn pinned_release(
        &self,
        root: ReleaseRoot,
    ) -> Result<ConceptRegistryReaderLease, BaseServiceError> {
        let mut releases = self.releases.lock().map_err(|_| corrupt())?;
        if let Some(release) = releases.get(&root.0) {
            return Ok(release.clone());
        }
        let release = self
            .registry
            .as_ref()
            .ok_or_else(unavailable)?
            .reader_lease();
        let actual = release
            .status()
            .release_aggregate_root
            .as_ref()
            .ok_or_else(unavailable)?;
        if decode_hex::<32>(actual).map_err(|_| corrupt())? != root.0 {
            return Err(unavailable());
        }
        if releases.len() >= 16 {
            return Err(resource());
        }
        releases.insert(root.0, release.clone());
        Ok(release)
    }

    pub(crate) fn preview(
        &self,
        principal: [u8; 32],
        operation: OperationId,
    ) -> Result<KuPreparedV1, BaseServiceError> {
        let bundle = self.owned_bundle(principal, operation)?;
        if bundle.canceled {
            return Err(conflict());
        }
        Ok(bundle.preview)
    }

    pub(crate) fn is_prepared(
        &self,
        principal: [u8; 32],
        operation: OperationId,
    ) -> Result<bool, BaseServiceError> {
        Ok(self
            .read_bundle(operation)?
            .is_some_and(|b| b.principal == principal && !b.canceled))
    }

    pub(crate) fn owns_operation(
        &self,
        principal: [u8; 32],
        operation: OperationId,
    ) -> Result<bool, BaseServiceError> {
        Ok(self
            .read_bundle(operation)?
            .is_some_and(|b| b.principal == principal))
    }

    pub(crate) fn prepare(
        &self,
        principal: [u8; 32],
        request: KuPrepareV1,
        revision: Option<(ObjectCID, RevisionFrontier)>,
        budget: &ResourceBudgetV1,
    ) -> Result<KuPreparedV1, BaseServiceError> {
        let _guard = self.mutation.lock().map_err(|_| corrupt())?;
        request.validate().map_err(|_| invalid())?;
        let (predecessor, expected_frontier) = revision
            .map(|(a, b)| (Some(a), Some(b)))
            .unwrap_or_default();
        if let Some(existing) = self.read_bundle(request.operation_id)? {
            if existing.principal != principal
                || existing.request != request
                || existing.predecessor != predecessor
                || existing.expected_frontier != expected_frontier
                || existing.canceled
            {
                return Err(conflict());
            }
            self.validate_bundle(&existing)?;
            return Ok(existing.preview);
        }
        if self
            .catalog
            .lock()
            .map_err(|_| corrupt())?
            .keys
            .contains_key(&request.idempotency_key.0)
        {
            return Err(conflict());
        }
        if request.semantic_profile != SEMANTIC_CONTENT_PROFILE {
            return Err(incompatible());
        }
        if request.source_refs.is_empty() || request.source_refs.len() > budget.max_items as usize {
            return Err(invalid());
        }
        if (request.input_mode == InputMode::ResolvedSemanticDraft) != request.draft_ref.is_some() {
            return Err(invalid());
        }
        if self.inputs.implementation(request.input_mode)
            != Some(request.implementation_commitment.0)
        {
            return Err(unavailable());
        }
        let sources: Vec<_> = request.source_refs.iter().map(|s| s.0).collect();
        self.inputs.check_access(principal, &sources)?;
        let release = self.pinned_release(request.registry_release_root)?;
        if let Some(cid) = predecessor {
            self.get(principal, cid)?;
            if self.frontier(principal)? != expected_frontier.unwrap() {
                return Err(conflict());
            }
        }
        let input = self.inputs.resolve(principal, &request, &release, budget)?;
        if input.drafts.is_empty()
            || input.drafts.len() > budget.max_items as usize
            || input.drafts.len() > 256
            || input.source_objects.len() != request.source_refs.len()
            || input.bindings.len() > 4096
        {
            return Err(resource());
        }
        let mut source_sizes = BTreeMap::new();
        let mut total = 0usize;
        for (bytes, cid) in input.source_objects.iter().zip(&request.source_refs) {
            total = total.checked_add(bytes.len()).ok_or_else(resource)?;
            if total > budget.max_bytes as usize {
                return Err(resource());
            }
            let object = decode_object(bytes)?;
            if object.cid().into_bytes() != cid.0 {
                return Err(invalid());
            }
            let source = SourceArtifact::from_validated(&object).map_err(|_| invalid())?;
            source_sizes.insert(cid.0, source.raw_bytes.len());
        }
        let selected = validate_bindings(&release, &input.bindings)?;
        let mut artifacts = Vec::new();
        let mut private_semantic_inputs = Vec::new();
        let mut original_statement_ids = Vec::new();
        let mut unresolved = false;
        let mut seen = BTreeSet::new();
        let mut work = 0u64;
        for draft in &input.drafts {
            work = work
                .checked_add(draft.statements.len() as u64)
                .ok_or_else(resource)?;
            if work > budget.max_work_units {
                return Err(resource());
            }
            for statement in &draft.statements {
                for span in &statement.qualifiers.source_spans {
                    if span.source.reference_kind != 1
                        || span.start > span.end
                        || source_sizes
                            .get(&span.source.cid)
                            .is_none_or(|len| span.end > *len as u64)
                    {
                        return Err(invalid());
                    }
                }
            }
            let normalized = normalize_semantic_content(draft, &request.semantic_profile)
                .map_err(|_| invalid())?;
            for ccid in semantic_ccids(&normalized.semantic)? {
                if !selected.contains(&ccid) {
                    unresolved = true;
                }
            }
            let (bytes, cid) = normalized
                .semantic
                .to_knowledge_object(disclosure(request.destination))
                .map_err(|_| invalid())?
                .encode(ResourceProfile::ObjectV1)
                .map_err(|_| invalid())?;
            total = total
                .checked_add(bytes.len())
                .and_then(|n| n.checked_add(normalized.private_input_bytes.len()))
                .ok_or_else(resource)?;
            if total > budget.max_bytes as usize {
                return Err(resource());
            }
            private_semantic_inputs.push(normalized.private_input_bytes);
            original_statement_ids.push(normalized.original_statement_ids);
            if !seen.insert(cid.into_bytes()) {
                continue;
            }
            artifacts.push(KuPreparedArtifactV1 {
                object_cid: ObjectCID(cid.into_bytes()),
                semantic_content_cid: SemanticContentCID(normalized.cid.into_bytes()),
                canonical_preview: STANDARD.encode(&bytes),
            });
        }
        if unresolved {
            artifacts.clear();
        }
        let preview = KuPreparedV1 {
            operation_id: request.operation_id,
            validity: if unresolved {
                Validity::NeedsResolution
            } else {
                Validity::Ready
            },
            object_cids: artifacts.iter().map(|a| a.object_cid).collect(),
            registry_release_root: request.registry_release_root,
            semantic_profile: request.semantic_profile.clone(),
            destination: request.destination,
            limitations: if unresolved {
                vec!["unresolved_ccid".into()]
            } else {
                vec!["fidelity_unassessed".into()]
            },
            executable: false,
            artifacts,
        };
        if preview.encode().map_err(|_| resource())?.len() > budget.max_bytes as usize {
            return Err(resource());
        }
        let bundle = PreparedBundle {
            principal,
            dataset: self.dataset,
            request,
            predecessor,
            expected_frontier,
            preview: preview.clone(),
            sources: input.source_objects,
            private_semantic_inputs,
            original_statement_ids,
            bindings: input.bindings,
            registry_builder: release.status().builder_version.clone().unwrap_or_default(),
            committed: false,
            canceled: false,
        };
        self.validate_bundle(&bundle)?;
        self.inputs.check_access(principal, &sources)?;
        self.write_bundle(&bundle)?;
        failpoint("after_prepared_journal")?;
        Ok(preview)
    }

    fn validate_bundle(&self, bundle: &PreparedBundle) -> Result<(), BaseServiceError> {
        bundle.request.validate().map_err(|_| corrupt())?;
        bundle.preview.validate().map_err(|_| corrupt())?;
        if bundle.dataset != self.dataset
            || bundle.request.operation_id != bundle.preview.operation_id
            || bundle.preview.object_cids.len() != bundle.preview.artifacts.len()
            || bundle.private_semantic_inputs.len() != bundle.original_statement_ids.len()
            || bundle.request.source_refs.len() != bundle.sources.len()
            || bundle.committed && bundle.canceled
        {
            return Err(corrupt());
        }
        for (i, artifact) in bundle.preview.artifacts.iter().enumerate() {
            let bytes = STANDARD
                .decode(&artifact.canonical_preview)
                .map_err(|_| corrupt())?;
            let object = decode_object(&bytes)?;
            if object.cid().into_bytes() != artifact.object_cid.0
                || bundle.preview.object_cids[i] != artifact.object_cid
                || object.disclosure() != disclosure(bundle.request.destination)
            {
                return Err(corrupt());
            }
            let ObjectSemantics::Known(envelope) = object.semantics() else {
                return Err(corrupt());
            };
            let semantic =
                SemanticFrameSet::from_canonical_value(&envelope.payload).map_err(|_| corrupt())?;
            let normalized =
                normalize_semantic_content(&semantic, &bundle.request.semantic_profile)
                    .map_err(|_| corrupt())?;
            if normalized.cid.into_bytes() != artifact.semantic_content_cid.0
                || normalized.semantic != semantic
            {
                return Err(corrupt());
            }
        }
        let mut input_cids = BTreeSet::new();
        for (bytes, ids) in bundle
            .private_semantic_inputs
            .iter()
            .zip(&bundle.original_statement_ids)
        {
            let original = ku_core::foundation::decode_canonical(bytes, ResourceProfile::ObjectV1)
                .map_err(|_| corrupt())?;
            let original =
                SemanticFrameSet::from_canonical_value(&original).map_err(|_| corrupt())?;
            if ids.len() != original.statements.len() {
                return Err(corrupt());
            }
            input_cids.insert(
                normalize_semantic_content(&original, &bundle.request.semantic_profile)
                    .map_err(|_| corrupt())?
                    .cid
                    .into_bytes(),
            );
        }
        if bundle.preview.validity == Validity::Ready
            && input_cids
                != bundle
                    .preview
                    .artifacts
                    .iter()
                    .map(|a| a.semantic_content_cid.0)
                    .collect()
        {
            return Err(corrupt());
        }
        for (cid, bytes) in bundle.request.source_refs.iter().zip(&bundle.sources) {
            let source = decode_object(bytes)?;
            if source.cid().into_bytes() != cid.0
                || SourceArtifact::from_validated(&source).is_err()
            {
                return Err(corrupt());
            }
        }
        Ok(())
    }

    pub(crate) fn preflight_save(
        &self,
        principal: [u8; 32],
        save: &KuSaveV1,
    ) -> Result<(), BaseServiceError> {
        let b = self.owned_bundle(principal, save.operation_id)?;
        if b.request.idempotency_key != save.idempotency_key
            || b.preview.object_cids != save.object_cids
            || b.canceled
            || b.preview.validity != Validity::Ready
        {
            return Err(conflict());
        }
        if b.committed {
            return Ok(());
        }
        self.pinned_release(b.request.registry_release_root)?;
        self.inputs.check_access(
            principal,
            &b.request
                .source_refs
                .iter()
                .map(|s| s.0)
                .collect::<Vec<_>>(),
        )?;
        if b.predecessor.is_some() && b.expected_frontier != Some(self.frontier(principal)?) {
            return Err(conflict());
        }
        Ok(())
    }

    pub(crate) fn save(
        &self,
        principal: [u8; 32],
        save: &KuSaveV1,
    ) -> Result<KuReceiptV1, BaseServiceError> {
        let _guard = self.mutation.lock().map_err(|_| corrupt())?;
        self.preflight_save(principal, save)?;
        let mut b = self.owned_bundle(principal, save.operation_id)?;
        if b.committed {
            self.verify_committed(&b)?;
            return Ok(receipt(&b, BaseState::Committed));
        }
        failpoint("before_objects")?;
        for (index, bytes) in b
            .sources
            .iter()
            .cloned()
            .chain(b.preview.artifacts.iter().map(|a| {
                STANDARD
                    .decode(&a.canonical_preview)
                    .expect("prevalidated base64")
            }))
            .enumerate()
        {
            // Recheck current source authority immediately before EACH durable write.
            self.inputs.check_access(
                principal,
                &b.request
                    .source_refs
                    .iter()
                    .map(|s| s.0)
                    .collect::<Vec<_>>(),
            )?;
            let object = decode_object(&bytes)?;
            let result = self
                .vault
                .put_verified_object(
                    object.cid(),
                    &bytes,
                    ResourceProfile::ObjectV1,
                    &known_kinds(),
                    &[],
                )
                .map_err(|_| unknown())?;
            if !matches!(
                result,
                PutVerifiedOutcome::Stored | PutVerifiedOutcome::AlreadyPresent
            ) {
                return Err(corrupt());
            }
            failpoint("after_object")?;
            failpoint(&format!("after_object_{index}"))?;
        }
        failpoint("before_commit_marker")?;
        self.inputs.check_access(
            principal,
            &b.request
                .source_refs
                .iter()
                .map(|s| s.0)
                .collect::<Vec<_>>(),
        )?;
        b.committed = true;
        self.write_bundle(&b)?;
        failpoint("after_commit_marker")?;
        Ok(receipt(&b, BaseState::Committed))
    }

    fn verify_committed(&self, b: &PreparedBundle) -> Result<(), BaseServiceError> {
        for cid in b
            .request
            .source_refs
            .iter()
            .map(|s| s.0)
            .chain(b.preview.object_cids.iter().map(|s| s.0))
        {
            self.vault
                .get_object(ObjectCid::from_bytes(cid))
                .map_err(|_| corrupt())?
                .ok_or_else(corrupt)?;
        }
        Ok(())
    }

    pub(crate) fn saved_receipt(
        &self,
        principal: [u8; 32],
        operation: OperationId,
    ) -> Result<Option<KuReceiptV1>, BaseServiceError> {
        let b = self.owned_bundle(principal, operation)?;
        if b.committed {
            self.verify_committed(&b)?;
            self.index_bundle(&b)?;
            Ok(Some(receipt(&b, BaseState::Committed)))
        } else {
            Ok(None)
        }
    }

    pub(crate) fn confirmation_receipt(
        &self,
        principal: [u8; 32],
        operation: OperationId,
    ) -> Result<KuReceiptV1, BaseServiceError> {
        Ok(receipt(
            &self.owned_bundle(principal, operation)?,
            BaseState::Committed,
        ))
    }

    pub(crate) fn recovery_save(
        &self,
        principal: [u8; 32],
        operation: OperationId,
        key: [u8; 32],
    ) -> Result<KuReceiptV1, BaseServiceError> {
        let bundle = self.owned_bundle(principal, operation)?;
        self.save(
            principal,
            &KuSaveV1 {
                operation_id: operation,
                idempotency_key: IdempotencyKey(key),
                object_cids: bundle.preview.object_cids,
            },
        )
    }

    pub(crate) fn cancel(
        &self,
        principal: [u8; 32],
        operation: OperationId,
    ) -> Result<(), BaseServiceError> {
        let _guard = self.mutation.lock().map_err(|_| corrupt())?;
        let Some(mut b) = self.read_bundle(operation)? else {
            return Ok(());
        };
        if b.principal != principal {
            return Err(not_found());
        }
        if b.committed {
            return Err(conflict());
        }
        b.canceled = true;
        // Retain encrypted exact intent for reconciliation; never undo accepted bytes.
        self.write_bundle(&b)
    }

    pub(crate) fn get(
        &self,
        principal: [u8; 32],
        cid: ObjectCID,
    ) -> Result<KuViewV1, BaseServiceError> {
        let summary = self
            .catalog
            .lock()
            .map_err(|_| corrupt())?
            .items
            .get(&principal)
            .and_then(|m| m.get(&cid.0))
            .map(|v| v.0.clone());
        if let Some(summary) = summary {
            let bytes = self
                .vault
                .get_object(ObjectCid::from_bytes(cid.0))
                .map_err(|_| corrupt())?
                .ok_or_else(corrupt)?;
            let object = decode_object(&bytes)?;
            return Ok(view(
                cid,
                bytes.clone(),
                summary.semantic_content_cid,
                object.disclosure(),
            ));
        }
        if let Some(public) = &self.public {
            if let Some(bytes) = public.get_public_object(cid.0)? {
                let object = decode_object(&bytes)?;
                if object.cid().into_bytes() != cid.0
                    || object.disclosure() != DisclosureClass::Public
                {
                    return Err(not_found());
                }
                let opaque = object.is_opaque();
                let mut result = view(cid, bytes, None, DisclosureClass::Public);
                if opaque {
                    result.artifact_validity = ArtifactValidity::AcceptedOpaque;
                }
                return Ok(result);
            }
        }
        Err(not_found())
    }

    fn frontier(&self, principal: [u8; 32]) -> Result<RevisionFrontier, BaseServiceError> {
        let catalog = self.catalog.lock().map_err(|_| corrupt())?;
        Ok(self.catalog_frontier(principal, &catalog))
    }

    fn catalog_frontier(&self, principal: [u8; 32], catalog: &Catalog) -> RevisionFrontier {
        let mut h = blake3::Hasher::new_derive_key("onebrain:ku:local-revision-frontier:1");
        h.update(&principal);
        h.update(&self.dataset.0);
        if let Some(items) = catalog.items.get(&principal) {
            for cid in items.keys() {
                h.update(&[1]);
                h.update(cid);
            }
        }
        if let Some(rows) = catalog.revisions.get(&principal) {
            for (a, b) in rows {
                h.update(&[2]);
                h.update(a);
                h.update(b);
            }
        }
        RevisionFrontier(*h.finalize().as_bytes())
    }

    pub(crate) fn page(
        &self,
        principal: [u8; 32],
        query: Option<String>,
        limit: u64,
        continuation: Option<String>,
        budget: &ResourceBudgetV1,
    ) -> Result<KuPageV1, BaseServiceError> {
        let mut paging = self.paging.lock().map_err(|_| corrupt())?;
        let (snapshot_id, last) = if let Some(token) = continuation {
            let cursor = paging.cursors.get(&token).ok_or_else(expired)?;
            (cursor.snapshot, Some(cursor.last_cid))
        } else {
            let catalog = self.catalog.lock().map_err(|_| corrupt())?;
            let mut items = Vec::new();
            let mut work = 0u64;
            if let Some(owned) = catalog.items.get(&principal) {
                for (summary, text) in owned.values() {
                    work = work
                        .checked_add(if query.is_some() {
                            text.len() as u64 + 1
                        } else {
                            1
                        })
                        .ok_or_else(resource)?;
                    if work > budget.max_work_units || items.len() >= 4096 {
                        return Err(resource());
                    }
                    if query.as_ref().is_none_or(|q| text.contains(q)) {
                        items.push(summary.clone());
                    }
                }
            }
            let id = random()?;
            if paging.snapshots.len() >= MAX_SNAPSHOTS {
                if let Some(old) = paging.order.pop_front() {
                    paging.snapshots.remove(&old);
                    paging.cursors.retain(|_, v| v.snapshot != old);
                }
            }
            let snapshot = Snapshot {
                principal,
                dataset: self.dataset,
                query: query.clone(),
                items,
                frontier: self.catalog_frontier(principal, &catalog),
            };
            paging.snapshots.insert(id, snapshot);
            paging.order.push_back(id);
            (id, None)
        };
        let snapshot = paging.snapshots.get(&snapshot_id).ok_or_else(expired)?;
        if snapshot.principal != principal
            || snapshot.dataset != self.dataset
            || snapshot.query != query
        {
            return Err(conflict());
        }
        let mut page = KuPageV1 {
            items: Vec::new(),
            coverage: Coverage::LocalOnly,
            snapshot_frontier: snapshot.frontier,
            limitations: vec![
                "private_owned_store_only".into(),
                "index_version_ku_text_1".into(),
                "frontier_is_local_revision_journal".into(),
            ],
            continuation: None,
        };
        let candidates: Vec<_> = snapshot
            .items
            .iter()
            .filter(|s| last.is_none_or(|v| s.object_cid.0 > v))
            .collect();
        let take = limit
            .min(budget.max_items as u64)
            .min(budget.max_work_units)
            .min(256) as usize;
        for item in candidates.iter().take(take) {
            page.items.push((*item).clone());
            if page.encode().map_err(|_| resource())?.len() + 128 > budget.max_bytes as usize {
                page.items.pop();
                break;
            }
        }
        if page.items.len() < candidates.len() {
            let last = page.items.last().ok_or_else(resource)?.object_cid.0;
            if paging.cursors.len() >= MAX_CURSORS {
                return Err(resource());
            }
            let token = format!("obc1.{}", hex(&random()?));
            paging.cursors.insert(
                token.clone(),
                Cursor {
                    snapshot: snapshot_id,
                    last_cid: last,
                },
            );
            page.coverage = Coverage::Partial;
            page.continuation = Some(token);
        }
        Ok(page)
    }

    pub(crate) fn status(&self) -> KuStatusV1 {
        let registry_ready = self
            .registry
            .as_ref()
            .is_some_and(|r| r.reader_lease().status().release_aggregate_root.is_some());
        KuStatusV1 {
            lifecycle: if registry_ready {
                Lifecycle::Active
            } else {
                Lifecycle::Degraded
            },
            coverage: Coverage::LocalOnly,
            limitations: vec!["local_only".into(), "fidelity_unassessed".into()],
            registry_ready,
            local_encoder_ready: self.inputs.implementation(InputMode::LocalRule).is_some()
                || self.inputs.implementation(InputMode::LocalAi).is_some(),
            remote_encoding_enabled: false,
            direct_issuance_enabled: false,
            receipt: None,
        }
    }

    pub(crate) fn public_export(
        &self,
        principal: [u8; 32],
        request: KuExportV1,
    ) -> Result<KuExportViewV1, BaseServiceError> {
        let mut entries = Vec::new();
        let mut seen = BTreeSet::new();
        let mut pending = request.object_cids.clone();
        let mut total = 0usize;
        while let Some(cid) = pending.pop() {
            if !seen.insert(cid.0) {
                continue;
            }
            if seen.len() > 256 {
                return Err(resource());
            }
            let object = self.get(principal, cid)?;
            if object.disclosure_class != ArtifactDisclosure::PUBLIC {
                return Err(conflict());
            }
            let bytes = STANDARD
                .decode(object.canonical_bytes)
                .map_err(|_| corrupt())?;
            total += bytes.len();
            if total > 262144 {
                return Err(resource());
            }
            let decoded = decode_object(&bytes)?;
            // Opaque records remain readable, but their dependency semantics
            // cannot establish a complete public export closure here.
            if decoded.is_opaque() {
                return Err(unavailable());
            }
            if let ObjectSemantics::Known(envelope) = decoded.semantics() {
                for reference in &envelope.references {
                    if reference.reference_kind != 1 {
                        return Err(unavailable());
                    }
                    pending.push(ObjectCID(reference.cid));
                }
            }
            entries.push(
                crate::canonical_exchange::BaseExchangeEntryV1::VNextPublic {
                    kind: ku_core::foundation::StoredRecordKind::Object,
                    cid: cid.0,
                    canonical_bytes: bytes,
                },
            );
        }
        let mut bytes = Vec::new();
        crate::canonical_exchange::write_canonical_exchange(&entries, &mut bytes)
            .map_err(|_| invalid())?;
        Ok(KuExportViewV1 {
            mode: ExportMode::CanonicalPublicExchange,
            object_cids: request.object_cids,
            limitations: vec![],
            requires_base_management: false,
            public_records: Some(STANDARD.encode(bytes)),
            archive_operation_id: None,
        })
    }
}

fn validate_bindings(
    release: &ConceptRegistryReaderLease,
    bindings: &[KuConceptBinding],
) -> Result<BTreeSet<[u8; 16]>, BaseServiceError> {
    use ku_core::concept_registry::ResolveResult;
    let mut selected = BTreeSet::new();
    for binding in bindings {
        if binding.label.len() > 4096 {
            return Err(resource());
        }
        let Some(ccid) = binding.selected else {
            continue;
        };
        let candidates = match release
            .resolve_checked(&binding.label)
            .map_err(|_| unavailable())?
        {
            ResolveResult::Found(value) | ResolveResult::Fuzzy(value) => vec![value],
            ResolveResult::Ambiguous(values) => values,
            ResolveResult::NotFound => Vec::new(),
        };
        if !candidates.iter().any(|c| c.ccid == ccid) {
            return Err(invalid());
        }
        selected.insert(ccid);
    }
    Ok(selected)
}

fn semantic_ccids(semantic: &SemanticFrameSet) -> Result<BTreeSet<[u8; 16]>, BaseServiceError> {
    // The typed walker covers predicates, every term/constraint and unit binding.
    Ok(semantic
        .concept_ccids()
        .map_err(|_| invalid())?
        .into_iter()
        .map(|c| *c.as_bytes())
        .collect())
}

fn semantic_text(bytes: &[u8]) -> Result<String, BaseServiceError> {
    fn walk(v: &ku_core::foundation::CanonicalValue, text: &mut String) {
        use ku_core::foundation::CanonicalValue;
        match v {
            CanonicalValue::Text(t) => {
                text.push_str(t);
                text.push('\0');
            }
            CanonicalValue::Array(a) => {
                for v in a {
                    walk(v, text);
                }
            }
            CanonicalValue::Map(m) => {
                for (_, v) in m {
                    walk(v, text);
                }
            }
            _ => {}
        }
    }
    let mut text = String::new();
    if let ObjectSemantics::Known(e) = decode_object(bytes)?.semantics() {
        walk(&e.payload, &mut text);
    }
    Ok(text)
}

fn known_kinds() -> Vec<KnownObjectKind> {
    ku_core::foundation::schema_registry::OBJECT_KINDS_V1
        .iter()
        .map(|k| KnownObjectKind::new(ObjectKind(k.id), 1))
        .collect()
}
fn decode_object(
    bytes: &[u8],
) -> Result<ku_core::foundation::ValidatedKnowledgeObject, BaseServiceError> {
    decode_knowledge_object(bytes, ResourceProfile::ObjectV1, &known_kinds(), &[])
        .map_err(|_| invalid())
}
fn disclosure(d: Disclosure) -> DisclosureClass {
    match d {
        Disclosure::LOCALONLY => DisclosureClass::LocalOnly,
        Disclosure::NEGOTIATEDENCRYPTED => DisclosureClass::NegotiatedEncrypted,
    }
}
fn artifact_disclosure(d: DisclosureClass) -> ArtifactDisclosure {
    match d {
        DisclosureClass::LocalOnly => ArtifactDisclosure::LOCALONLY,
        DisclosureClass::NegotiatedEncrypted => ArtifactDisclosure::NEGOTIATEDENCRYPTED,
        DisclosureClass::Public => ArtifactDisclosure::PUBLIC,
        DisclosureClass::RouteMinimal => ArtifactDisclosure::ROUTEMINIMAL,
    }
}
fn view(
    cid: ObjectCID,
    bytes: Vec<u8>,
    semantic: Option<SemanticContentCID>,
    d: DisclosureClass,
) -> KuViewV1 {
    KuViewV1 {
        object_cid: cid,
        semantic_content_cid: semantic,
        disclosure_class: artifact_disclosure(d),
        artifact_validity: ArtifactValidity::AcceptedKnown,
        coverage: Coverage::LocalOnly,
        limitations: vec!["fidelity_unassessed".into()],
        executable: false,
        canonical_bytes: STANDARD.encode(bytes),
        fidelity_policy_cid: None,
        fidelity_frontier: None,
    }
}
fn receipt(b: &PreparedBundle, state: BaseState) -> KuReceiptV1 {
    KuReceiptV1 {
        operation_id: b.request.operation_id,
        state,
        object_cids: b.preview.object_cids.clone(),
        limitations: vec!["fidelity_unassessed".into()],
        published: false,
        authorizes_reward: false,
    }
}
fn random() -> Result<[u8; 32], BaseServiceError> {
    let mut bytes = [0; 32];
    getrandom::fill(&mut bytes).map_err(|_| unavailable())?;
    Ok(bytes)
}
pub(crate) fn invalid() -> BaseServiceError {
    BaseServiceError::new(BaseErrorCodeV1::InvalidRequest, "ku_invalid_request")
}
pub(crate) fn conflict() -> BaseServiceError {
    BaseServiceError::new(BaseErrorCodeV1::Conflict, "ku_conflict")
}
pub(crate) fn corrupt() -> BaseServiceError {
    BaseServiceError::new(BaseErrorCodeV1::CorruptState, "ku_corrupt_state")
}
pub(crate) fn resource() -> BaseServiceError {
    BaseServiceError::new(BaseErrorCodeV1::ResourceExhausted, "ku_resource_bound")
}
pub(crate) fn unavailable() -> BaseServiceError {
    BaseServiceError::new(
        BaseErrorCodeV1::DependencyUnavailable,
        "ku_dependency_unavailable",
    )
}
pub(crate) fn unknown() -> BaseServiceError {
    BaseServiceError::new(BaseErrorCodeV1::UnknownOutcome, "ku_reconcile_required")
}
fn incompatible() -> BaseServiceError {
    BaseServiceError::new(
        BaseErrorCodeV1::IncompatibleProfile,
        "ku_profile_unsupported",
    )
}
fn not_found() -> BaseServiceError {
    BaseServiceError::new(BaseErrorCodeV1::NotFound, "ku_not_found")
}
fn expired() -> BaseServiceError {
    BaseServiceError::new(BaseErrorCodeV1::Expired, "ku_snapshot_expired")
}

fn failpoint(_phase: &str) -> Result<(), BaseServiceError> {
    #[cfg(test)]
    {
        if FAILPOINT.with(|p| p.borrow().as_deref() == Some(&format!("crash:{_phase}"))) {
            std::process::exit(86);
        }
        if FAILPOINT.with(|p| p.borrow().as_deref() == Some(_phase)) {
            return Err(unknown());
        }
    }
    Ok(())
}
#[cfg(test)]
thread_local! { static FAILPOINT: std::cell::RefCell<Option<String>> = const { std::cell::RefCell::new(None) }; }

#[cfg(test)]
#[path = "ku_product_tests.rs"]
mod tests;

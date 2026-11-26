//! Pillar 1 (`KU-002..008`, `P1-KU-LIFECYCLE-001`) node composition and supersession facade.
//!
//! Lazily opens `onebrain_core::RedbCompositionStore` (`vnext_ku_composition.redb`) ONLY
//! when explicitly invoked so default `OneBrainNode` / `VNextProductRuntime` startup
//! artifact sets remain strictly unchanged (`VNEXT_STARTUP_ARTIFACTS`).

use std::path::{Path, PathBuf};
use std::sync::{Arc, OnceLock};

use ku_core::foundation::scoped_concept::{
    build_concept_equivalence_mapping, ConceptDefinitionFrame,
};
use ku_core::foundation::{
    ActorId, BindingAcceptance, ConceptCcid, DisclosureClass, FrontierAssemblyManifest,
    KnowledgeAffordance, MappingKernel, MaterializationAuthority, MaterializeMappingCommand,
    MaterializedMapping, ObjectCid, ObjectReference, ReceptorDefinition, ResolutionApplyOutcome,
    ResolutionAuthority, ResolutionTarget, ResolutionView, SemanticContentCid, SemanticFrameSet,
    SupersessionApplyOutcome, SupersessionLineageView, ValidatedFeedInception,
};
use ku_kql::vnext_proposal::{BindingProposal, ProposalId};
use onebrain_core::{RedbCompositionStore, RedbMappingBackend};

use crate::error::NodeError;

pub const DEFAULT_KU_COMPOSITION_DB_FILENAME: &str = "vnext_ku_composition.redb";

/// Node-owned composition & supersession runtime facade over `onebrain_core::RedbCompositionStore`.
#[derive(Clone)]
pub struct KuCompositionRuntime {
    data_dir: PathBuf,
    store: Arc<OnceLock<Arc<RedbCompositionStore>>>,
}

impl KuCompositionRuntime {
    /// Construct a lazy handle; no `.redb` file is created on disk until first use.
    pub fn new_lazy(data_dir: impl AsRef<Path>) -> Self {
        Self {
            data_dir: data_dir.as_ref().to_path_buf(),
            store: Arc::new(OnceLock::new()),
        }
    }

    /// Open immediately at `data_dir/vnext_ku_composition.redb`.
    pub fn open(data_dir: impl AsRef<Path>) -> Result<Self, NodeError> {
        let runtime = Self::new_lazy(data_dir);
        let _ = runtime.ensure_store()?;
        Ok(runtime)
    }

    pub fn db_path(&self) -> PathBuf {
        self.data_dir.join(DEFAULT_KU_COMPOSITION_DB_FILENAME)
    }

    pub fn is_opened(&self) -> bool {
        self.store.get().is_some()
    }

    fn ensure_store(&self) -> Result<&Arc<RedbCompositionStore>, NodeError> {
        if let Some(existing) = self.store.get() {
            return Ok(existing);
        }
        std::fs::create_dir_all(&self.data_dir)
            .map_err(|err| NodeError::Storage(err.to_string()))?;
        let opened = Arc::new(
            RedbCompositionStore::open(&self.db_path())
                .map_err(|err| NodeError::Storage(err.to_string()))?,
        );
        let _ = self.store.set(opened);
        self.store
            .get()
            .ok_or_else(|| NodeError::Storage("failed to initialize KuCompositionRuntime".into()))
    }

    pub fn mapping_backend(&self) -> Result<RedbMappingBackend, NodeError> {
        Ok(self.ensure_store()?.mapping_backend())
    }

    pub fn declare_reference_disclosure(
        &self,
        reference: &ObjectReference,
        disclosure: DisclosureClass,
    ) -> Result<(), NodeError> {
        self.ensure_store()?
            .declare_reference_disclosure(reference, disclosure)
            .map_err(|err| NodeError::Storage(err.to_string()))
    }

    /// Author a `ReceptorDefinition` (`KU-002`, `ObjectKind(3)`).
    pub fn author_receptor_definition(
        &self,
        definition: &ReceptorDefinition,
        disclosure: DisclosureClass,
    ) -> Result<ObjectCid, NodeError> {
        self.ensure_store()?
            .author_receptor_definition(definition, disclosure)
            .map_err(|err| NodeError::Storage(err.to_string()))
    }

    pub fn get_receptor_definition(
        &self,
        cid: ObjectCid,
    ) -> Result<Option<ReceptorDefinition>, NodeError> {
        self.ensure_store()?
            .get_receptor_definition(cid)
            .map_err(|err| NodeError::Storage(err.to_string()))
    }

    /// Attach an explicit `KnowledgeAffordance` (`KU-003`, `ObjectKind(5)`).
    pub fn attach_affordance(
        &self,
        affordance: &KnowledgeAffordance,
        disclosure: DisclosureClass,
    ) -> Result<ObjectCid, NodeError> {
        self.ensure_store()?
            .author_affordance(affordance, disclosure)
            .map_err(|err| NodeError::Storage(err.to_string()))
    }

    /// Deterministically extract & store a virtual affordance from a `SemanticFrameSet` (`KU-003`).
    pub fn extract_affordance_from_semantic(
        &self,
        ku_ref: ObjectReference,
        semantic: &SemanticFrameSet,
        disclosure: DisclosureClass,
    ) -> Result<(ObjectCid, KnowledgeAffordance), NodeError> {
        self.ensure_store()?
            .project_and_store_virtual_affordance(ku_ref, semantic, disclosure)
            .map_err(|err| NodeError::Storage(err.to_string()))
    }

    pub fn get_affordance(&self, cid: ObjectCid) -> Result<Option<KnowledgeAffordance>, NodeError> {
        self.ensure_store()?
            .get_affordance(cid)
            .map_err(|err| NodeError::Storage(err.to_string()))
    }

    /// Persist a `FrontierAssemblyManifest` (`KU-004`, `ObjectKind(4)`).
    pub fn store_assembly_manifest(
        &self,
        manifest: &FrontierAssemblyManifest,
        disclosure: DisclosureClass,
    ) -> Result<ObjectCid, NodeError> {
        self.ensure_store()?
            .store_assembly_manifest(manifest, disclosure)
            .map_err(|err| NodeError::Storage(err.to_string()))
    }

    pub fn get_assembly_manifest_bytes(
        &self,
        cid: ObjectCid,
    ) -> Result<Option<Vec<u8>>, NodeError> {
        self.ensure_store()?
            .get_assembly_manifest_bytes(cid)
            .map_err(|err| NodeError::Storage(err.to_string()))
    }

    /// Stage a non-executable `BindingProposal` in local quarantine (`KU-005`).
    pub fn stage_binding_proposal(
        &self,
        proposal: BindingProposal,
    ) -> Result<ProposalId, NodeError> {
        self.ensure_store()?
            .stage_binding_proposal(proposal)
            .map_err(|err| NodeError::Storage(err.to_string()))
    }

    pub fn get_binding_proposal(
        &self,
        id: ProposalId,
    ) -> Result<Option<BindingProposal>, NodeError> {
        Ok(self.ensure_store()?.get_binding_proposal(id))
    }

    /// Materialize a `MappingKernel` + `MappingEnvelope` pair atomically (`KU-006`).
    pub fn materialize_mapping(
        &self,
        command: &MaterializeMappingCommand,
        authority: MaterializationAuthority,
    ) -> Result<MaterializedMapping, NodeError> {
        self.ensure_store()?
            .materialize_mapping(command, authority)
            .map_err(|err| NodeError::Storage(err.to_string()))
    }

    /// Apply and persist a `ReceptorResolutionEvent` (`KU-007..008`).
    pub fn apply_receptor_resolution_event(
        &self,
        feed: &ValidatedFeedInception,
        action_object_bytes: &[u8],
        event_bytes: &[u8],
        authority: ResolutionAuthority,
        acceptance: Option<BindingAcceptance>,
    ) -> Result<(ResolutionApplyOutcome, ResolutionView), NodeError> {
        self.ensure_store()?
            .record_resolution_event(
                feed,
                action_object_bytes,
                event_bytes,
                authority,
                acceptance,
            )
            .map_err(|err| NodeError::Storage(err.to_string()))
    }

    pub fn resolution_view(
        &self,
        target: ResolutionTarget,
    ) -> Result<Option<ResolutionView>, NodeError> {
        self.ensure_store()?
            .resolution_view(target)
            .map_err(|err| NodeError::Storage(err.to_string()))
    }

    /// Register root delegation for supersession author authority checks (`P1` §4.2).
    pub fn register_supersession_root_delegation(
        &self,
        feed: &ValidatedFeedInception,
        root_delegation_bytes: &[u8],
    ) -> Result<(), NodeError> {
        self.ensure_store()?
            .register_supersession_root_delegation(feed, root_delegation_bytes)
            .map_err(|err| NodeError::Storage(err.to_string()))
    }

    /// Bind an immutable KU `ObjectCid` to its original author `ActorId`.
    pub fn register_supersession_object_author(
        &self,
        object_cid: ObjectCid,
        actor_id: ActorId,
    ) -> Result<(), NodeError> {
        self.ensure_store()?
            .register_supersession_object_author(object_cid, actor_id)
            .map_err(|err| NodeError::Storage(err.to_string()))
    }

    /// Record and reduce a `KnowledgeSupersessionEvent` (`ObjectKind(24)`, `EventType(8)`).
    pub fn record_supersession_event(
        &self,
        feed: &ValidatedFeedInception,
        payload_object_bytes: &[u8],
        event_bytes: &[u8],
    ) -> Result<(SupersessionApplyOutcome, SupersessionLineageView), NodeError> {
        self.ensure_store()?
            .record_supersession_event(feed, payload_object_bytes, event_bytes)
            .map_err(|err| NodeError::Storage(err.to_string()))
    }

    /// Inspect deterministic supersession lineage for `predecessor` (`NEG-KU-004`).
    pub fn inspect_supersession_lineage(
        &self,
        predecessor: ObjectCid,
    ) -> Result<SupersessionLineageView, NodeError> {
        Ok(self.ensure_store()?.supersession_lineage(predecessor))
    }

    /// Author a novel `ConceptDefinitionFrame` (`P1` §2.3) and return `(encoded_bytes, ObjectCid, ScopedCcid, SemanticContentCid)`.
    pub fn author_scoped_concept_definition(
        &self,
        definition: &ConceptDefinitionFrame,
        disclosure: DisclosureClass,
    ) -> Result<(Vec<u8>, ObjectCid, ConceptCcid, SemanticContentCid), NodeError> {
        let (envelope, obj_cid, scoped_ccid, sem_cid) = definition
            .to_knowledge_object(disclosure)
            .map_err(|err| NodeError::Storage(err.to_string()))?;
        let (bytes, _) = envelope
            .encode(ku_core::foundation::ResourceProfile::ObjectV1)
            .map_err(|err| NodeError::Storage(err.to_string()))?;
        self.declare_reference_disclosure(
            &ObjectReference::new(0, obj_cid.into_bytes()),
            disclosure,
        )?;
        Ok((bytes, obj_cid, scoped_ccid, sem_cid))
    }

    /// Build a `ConceptEquivalence` `MappingKernel` absorbing a scoped concept into a standard CCID (`P1` §2.3.3).
    pub fn build_concept_equivalence(
        &self,
        scoped_def_obj: ObjectReference,
        scoped_ccid: ConceptCcid,
        standard_ku_or_def: ObjectReference,
        standard_ccid: ConceptCcid,
    ) -> Result<MappingKernel, NodeError> {
        build_concept_equivalence_mapping(
            scoped_def_obj,
            scoped_ccid,
            standard_ku_or_def,
            standard_ccid,
        )
        .map_err(|err| NodeError::Storage(format!("{err:?}")))
    }
}

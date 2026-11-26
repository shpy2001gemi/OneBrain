//! Synchronous `redb`-backed Composition Store (`ARCH-10` / `KU-002..008` + `P1` Supersession):
//! - Implements `AtomicMappingBackend` (`ku_core::foundation::materialization`)
//! - Implements `MaterializedMappingLookup` (`ku_core::foundation::resolution`)
//! - Authors & persists `ReceptorDefinition`, `KnowledgeAffordance` (explicit & `project_virtual_sfs_affordance`),
//!   and `FrontierAssemblyManifest`
//! - Stages ephemeral private `BindingProposal`s (`ku_kql::vnext_proposal`)
//! - Materializes `MappingKernel` + `MappingEnvelope` pairs atomically
//! - Persists & reduces `ValidatedResolutionEvent`s -> `ResolutionView` (`ResolutionReducer`)
//! - Persists & reduces `ValidatedKnowledgeSupersessionEvent`s -> `SupersessionLineageView`

use std::collections::BTreeMap;
use std::path::Path;
use std::sync::{Arc, Mutex};

use ku_core::foundation::materialization::BackendMappingOutcome;
use ku_core::foundation::{
    assess_resolution_event, decode_actor_root_delegation, decode_feed_inception,
    decode_knowledge_event, decode_knowledge_object, project_virtual_sfs_affordance,
    AtomicMappingBackend, BindingAcceptance, DisclosureClass, EventCid, FrontierAssemblyManifest,
    KeyStateReducer, KnowledgeAffordance, KnownObjectKind, MappingKernelCid, MappingMaterializer,
    MappingRecordKind, MappingWriteBatch, MaterializationAuthority, MaterializeMappingCommand,
    MaterializedMapping, MaterializedMappingLookup, ObjectCid, ObjectReference, ObjectSemantics,
    ReceptorDefinition, ReferenceDisclosureIndex, ResolutionApplyOutcome, ResolutionAuthority,
    ResolutionReducer, ResolutionTarget, ResolutionView, ResourceProfile, ScopedDelegation,
    SemanticFrameSet, SupersessionApplyOutcome, SupersessionLineageReducer,
    SupersessionLineageView, ValidatedFeedInception, ValidatedKnowledgeSupersessionEvent,
    ValidatedResolutionEvent, ASSEMBLY_MANIFEST_KIND, KNOWLEDGE_AFFORDANCE_KIND,
    KNOWLEDGE_SUPERSESSION_EVENT_TYPE, KNOWLEDGE_SUPERSESSION_KIND, RECEPTOR_DEFINITION_KIND,
    RECEPTOR_RESOLUTION_ACTION_KIND, RECEPTOR_RESOLUTION_EVENT_TYPE,
};
use ku_kql::vnext_proposal::{BindingProposal, ProposalId, ProposalQuarantine};
use redb::{Database, ReadableTable, TableDefinition};
use serde::{Deserialize, Serialize};

use crate::error::CoreEngineError;

const COMP_OBJECTS_V1: TableDefinition<&[u8; 33], &[u8]> = TableDefinition::new("comp_objects_v1");
const COMP_MAPPING_RECORDS_V1: TableDefinition<&[u8; 34], &[u8]> =
    TableDefinition::new("comp_mapping_records_v1");
const COMP_MAPPING_OPS_V1: TableDefinition<&[u8; 32], &[u8; 72]> =
    TableDefinition::new("comp_mapping_ops_v1");
const COMP_RESOLUTION_LOG_V1: TableDefinition<&[u8; 32], &[u8]> =
    TableDefinition::new("comp_resolution_log_v1");
const COMP_SUPERSESSION_META_V1: TableDefinition<&str, &[u8]> =
    TableDefinition::new("comp_supersession_meta_v1");
const COMP_SUPERSESSION_LOG_V1: TableDefinition<&[u8; 32], &[u8]> =
    TableDefinition::new("comp_supersession_log_v1");

#[derive(Serialize, Deserialize)]
struct PersistedResolutionRow {
    feed_bytes: Vec<u8>,
    action_object_bytes: Vec<u8>,
    event_bytes: Vec<u8>,
    authority_code: u8,
    acceptance_code: Option<u8>,
}

#[derive(Serialize, Deserialize)]
struct PersistedSupersessionRow {
    feed_bytes: Vec<u8>,
    payload_object_bytes: Vec<u8>,
    event_bytes: Vec<u8>,
}

#[derive(Serialize, Deserialize, Default)]
struct PersistedSupersessionMeta {
    root_delegations: Vec<(Vec<u8>, Vec<u8>)>, // (feed_bytes, root_delegation_bytes)
    object_authors: Vec<([u8; 32], [u8; 32])>, // (object_cid, actor_id)
}

#[derive(Clone)]
pub struct RedbMappingBackend {
    db: Arc<Database>,
}

impl AtomicMappingBackend for RedbMappingBackend {
    fn store_pair_atomically(
        &self,
        batch: MappingWriteBatch<'_>,
    ) -> Result<BackendMappingOutcome, String> {
        let dest = disclosure_byte(batch.destination);
        let write_txn = self.db.begin_write().map_err(|e| e.to_string())?;
        let outcome = {
            let mut ops = write_txn
                .open_table(COMP_MAPPING_OPS_V1)
                .map_err(|e| e.to_string())?;
            let mut recs = write_txn
                .open_table(COMP_MAPPING_RECORDS_V1)
                .map_err(|e| e.to_string())?;

            let op_val = encode_op_tuple(batch.kernel_cid, batch.envelope_cid, dest);
            let existing_match = ops
                .get(&batch.idempotency_key)
                .map_err(|e| e.to_string())?
                .map(|g| *g.value() == op_val);
            if let Some(matches) = existing_match {
                return Ok(if matches {
                    BackendMappingOutcome::IdempotentReplay
                } else {
                    BackendMappingOutcome::IdempotencyConflict
                });
            }

            let k_key =
                mapping_record_key(dest, MappingRecordKind::Kernel, batch.kernel_cid.as_bytes());
            let e_key = mapping_record_key(
                dest,
                MappingRecordKind::Envelope,
                batch.envelope_cid.as_bytes(),
            );

            let k_existing = recs
                .get(&k_key)
                .map_err(|e| e.to_string())?
                .map(|g| g.value().to_vec());
            let e_existing = recs
                .get(&e_key)
                .map_err(|e| e.to_string())?
                .map(|g| g.value().to_vec());

            if k_existing
                .as_deref()
                .is_some_and(|b| b != batch.kernel_bytes)
                || e_existing
                    .as_deref()
                    .is_some_and(|b| b != batch.envelope_bytes)
            {
                return Ok(BackendMappingOutcome::Collision);
            }

            let both_present = k_existing.is_some() && e_existing.is_some();
            if k_existing.is_none() {
                recs.insert(&k_key, batch.kernel_bytes)
                    .map_err(|e| e.to_string())?;
            }
            if e_existing.is_none() {
                recs.insert(&e_key, batch.envelope_bytes)
                    .map_err(|e| e.to_string())?;
            }
            ops.insert(&batch.idempotency_key, &op_val)
                .map_err(|e| e.to_string())?;

            if both_present {
                BackendMappingOutcome::AlreadyPresent
            } else {
                BackendMappingOutcome::Stored
            }
        };
        write_txn.commit().map_err(|e| e.to_string())?;
        Ok(outcome)
    }

    fn get(
        &self,
        destination: DisclosureClass,
        kind: MappingRecordKind,
        cid: &[u8; 32],
    ) -> Result<Option<Vec<u8>>, String> {
        let read_txn = self.db.begin_read().map_err(|e| e.to_string())?;
        let recs = read_txn
            .open_table(COMP_MAPPING_RECORDS_V1)
            .map_err(|e| e.to_string())?;
        let key = mapping_record_key(disclosure_byte(destination), kind, cid);
        Ok(recs
            .get(&key)
            .map_err(|e| e.to_string())?
            .map(|g| g.value().to_vec()))
    }
}

impl MaterializedMappingLookup for RedbMappingBackend {
    fn contains_materialized_mapping(&self, mapping: MappingKernelCid) -> Result<bool, String> {
        for class in [
            DisclosureClass::LocalOnly,
            DisclosureClass::NegotiatedEncrypted,
            DisclosureClass::Public,
            DisclosureClass::RouteMinimal,
        ] {
            if self
                .get(class, MappingRecordKind::Kernel, mapping.as_bytes())?
                .is_some()
            {
                return Ok(true);
            }
        }
        Ok(false)
    }
}

type ResolutionTargetKey = ([u8; 32], [u8; 32], [u8; 32]);

/// Synchronous `redb`-backed composition & supersession store (`ARCH-10`).
pub struct RedbCompositionStore {
    db: Arc<Database>,
    mapping_backend: RedbMappingBackend,
    proposals: Mutex<ProposalQuarantine>,
    disclosures: Mutex<ReferenceDisclosureIndex>,
    resolution_reducers: Mutex<BTreeMap<ResolutionTargetKey, ResolutionReducer>>,
    key_state: Mutex<KeyStateReducer>,
    supersession: Mutex<SupersessionLineageReducer>,
}

impl RedbCompositionStore {
    pub fn open(path: &Path) -> Result<Self, CoreEngineError> {
        let db = Arc::new(Database::create(path)?);
        {
            let write_txn = db.begin_write()?;
            let _ = write_txn.open_table(COMP_OBJECTS_V1)?;
            let _ = write_txn.open_table(COMP_MAPPING_RECORDS_V1)?;
            let _ = write_txn.open_table(COMP_MAPPING_OPS_V1)?;
            let _ = write_txn.open_table(COMP_RESOLUTION_LOG_V1)?;
            let _ = write_txn.open_table(COMP_SUPERSESSION_META_V1)?;
            let _ = write_txn.open_table(COMP_SUPERSESSION_LOG_V1)?;
            write_txn.commit()?;
        }

        let mapping_backend = RedbMappingBackend {
            db: Arc::clone(&db),
        };
        let store = Self {
            db,
            mapping_backend,
            proposals: Mutex::new(ProposalQuarantine::default()),
            disclosures: Mutex::new(ReferenceDisclosureIndex::default()),
            resolution_reducers: Mutex::new(BTreeMap::new()),
            key_state: Mutex::new(KeyStateReducer::new(EventCid::from_bytes([1; 32]))),
            supersession: Mutex::new(SupersessionLineageReducer::default()),
        };
        store.rebuild_reducers()?;
        Ok(store)
    }

    pub fn mapping_backend(&self) -> RedbMappingBackend {
        self.mapping_backend.clone()
    }

    pub fn declare_reference_disclosure(
        &self,
        reference: &ObjectReference,
        disclosure: DisclosureClass,
    ) -> Result<(), CoreEngineError> {
        let mut guard = self
            .disclosures
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        guard.declare(reference, disclosure)?;
        Ok(())
    }

    pub fn author_receptor_definition(
        &self,
        definition: &ReceptorDefinition,
        disclosure: DisclosureClass,
    ) -> Result<ObjectCid, CoreEngineError> {
        let envelope = definition.to_knowledge_object(disclosure)?;
        let (bytes, cid) = envelope.encode(ResourceProfile::ObjectV1)?;
        self.store_composition_object(0x01, cid, &bytes, disclosure)?;
        Ok(cid)
    }

    pub fn get_receptor_definition(
        &self,
        cid: ObjectCid,
    ) -> Result<Option<ReceptorDefinition>, CoreEngineError> {
        let Some(bytes) = self.load_composition_object(0x01, cid)? else {
            return Ok(None);
        };
        let validated = decode_knowledge_object(
            &bytes,
            ResourceProfile::ObjectV1,
            &[KnownObjectKind::new(RECEPTOR_DEFINITION_KIND, 1)],
            &[],
        )?;
        let ObjectSemantics::Known(env) = validated.semantics() else {
            return Err(CoreEngineError::InvalidInput("unknown_receptor_semantics"));
        };
        Ok(Some(ReceptorDefinition::from_canonical_payload(
            &env.payload,
        )?))
    }

    pub fn author_affordance(
        &self,
        affordance: &KnowledgeAffordance,
        disclosure: DisclosureClass,
    ) -> Result<ObjectCid, CoreEngineError> {
        let envelope = affordance.to_knowledge_object(disclosure)?;
        let (bytes, cid) = envelope.encode(ResourceProfile::ObjectV1)?;
        self.store_composition_object(0x02, cid, &bytes, disclosure)?;
        Ok(cid)
    }

    pub fn project_and_store_virtual_affordance(
        &self,
        ku_ref: ObjectReference,
        semantic: &SemanticFrameSet,
        disclosure: DisclosureClass,
    ) -> Result<(ObjectCid, KnowledgeAffordance), CoreEngineError> {
        let projected = project_virtual_sfs_affordance(ku_ref, semantic)?;
        let cid = self.author_affordance(&projected, disclosure)?;
        Ok((cid, projected))
    }

    pub fn get_affordance(
        &self,
        cid: ObjectCid,
    ) -> Result<Option<KnowledgeAffordance>, CoreEngineError> {
        let Some(bytes) = self.load_composition_object(0x02, cid)? else {
            return Ok(None);
        };
        let validated = decode_knowledge_object(
            &bytes,
            ResourceProfile::ObjectV1,
            &[KnownObjectKind::new(KNOWLEDGE_AFFORDANCE_KIND, 1)],
            &[],
        )?;
        Ok(Some(KnowledgeAffordance::from_validated_object(
            &validated,
        )?))
    }

    pub fn store_assembly_manifest(
        &self,
        manifest: &FrontierAssemblyManifest,
        disclosure: DisclosureClass,
    ) -> Result<ObjectCid, CoreEngineError> {
        let envelope = manifest.to_knowledge_object(disclosure)?;
        let (bytes, cid) = envelope.encode(ResourceProfile::ObjectV1)?;
        self.store_composition_object(0x03, cid, &bytes, disclosure)?;
        Ok(cid)
    }

    pub fn get_assembly_manifest_bytes(
        &self,
        cid: ObjectCid,
    ) -> Result<Option<Vec<u8>>, CoreEngineError> {
        let Some(bytes) = self.load_composition_object(0x03, cid)? else {
            return Ok(None);
        };
        let _ = decode_knowledge_object(
            &bytes,
            ResourceProfile::ObjectV1,
            &[KnownObjectKind::new(ASSEMBLY_MANIFEST_KIND, 1)],
            &[],
        )?;
        Ok(Some(bytes))
    }

    pub fn stage_binding_proposal(
        &self,
        proposal: BindingProposal,
    ) -> Result<ProposalId, CoreEngineError> {
        let mut guard = self
            .proposals
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        Ok(guard.insert(proposal)?)
    }

    pub fn get_binding_proposal(&self, id: ProposalId) -> Option<BindingProposal> {
        let guard = self
            .proposals
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        guard.get(id).cloned()
    }

    pub fn materialize_mapping(
        &self,
        command: &MaterializeMappingCommand,
        authority: MaterializationAuthority,
    ) -> Result<MaterializedMapping, CoreEngineError> {
        let disclosures = self
            .disclosures
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .clone();
        let materializer = MappingMaterializer::new(self.mapping_backend.clone());
        Ok(materializer.materialize(command, authority, &disclosures)?)
    }

    pub fn record_resolution_event(
        &self,
        feed: &ValidatedFeedInception,
        action_object_bytes: &[u8],
        event_bytes: &[u8],
        authority: ResolutionAuthority,
        acceptance: Option<BindingAcceptance>,
    ) -> Result<(ResolutionApplyOutcome, ResolutionView), CoreEngineError> {
        let validated_obj = decode_knowledge_object(
            action_object_bytes,
            ResourceProfile::ObjectV1,
            &[KnownObjectKind::new(RECEPTOR_RESOLUTION_ACTION_KIND, 1)],
            &[],
        )?;
        let validated_ev =
            decode_knowledge_event(event_bytes, feed, &[RECEPTOR_RESOLUTION_EVENT_TYPE])
                .map_err(|_| CoreEngineError::InvalidInput("invalid_resolution_event"))?;
        let bound = ValidatedResolutionEvent::bind(&validated_ev, &validated_obj)?;
        let target = bound.payload().target;
        let policy = bound.payload().resolution_policy.clone();
        let frontier = bound.payload().observed_frontier;
        let event_key = bound.event_cid().into_bytes();

        let assessed =
            assess_resolution_event(bound, authority, acceptance, &self.mapping_backend)?;

        let persisted = PersistedResolutionRow {
            feed_bytes: feed.original_bytes().to_vec(),
            action_object_bytes: action_object_bytes.to_vec(),
            event_bytes: event_bytes.to_vec(),
            authority_code: match authority {
                ResolutionAuthority::Authorized => 0,
                ResolutionAuthority::Unauthorized => 1,
                ResolutionAuthority::Unresolved => 2,
            },
            acceptance_code: acceptance.map(|a| match a {
                BindingAcceptance::Rejected => 0,
                BindingAcceptance::Partial => 1,
                BindingAcceptance::Satisfied => 2,
            }),
        };
        let row_bytes = serde_json::to_vec(&persisted)
            .map_err(|_| CoreEngineError::InvalidInput("resolution_serialize"))?;

        let write_txn = self.db.begin_write()?;
        {
            let mut table = write_txn.open_table(COMP_RESOLUTION_LOG_V1)?;
            table.insert(&event_key, row_bytes.as_slice())?;
        }
        write_txn.commit()?;

        let mut reducers = self
            .resolution_reducers
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        let t_key = target_key(target);
        let reducer = reducers
            .entry(t_key)
            .or_insert_with(|| ResolutionReducer::new(target, policy, frontier));
        reducer.set_assessed_frontier(frontier);
        let outcome = reducer.apply(assessed)?;
        let view = reducer.view()?;
        Ok((outcome, view))
    }

    pub fn resolution_view(
        &self,
        target: ResolutionTarget,
    ) -> Result<Option<ResolutionView>, CoreEngineError> {
        let reducers = self
            .resolution_reducers
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        match reducers.get(&target_key(target)) {
            Some(r) => Ok(Some(r.view()?)),
            None => Ok(None),
        }
    }

    pub fn register_supersession_root_delegation(
        &self,
        feed: &ValidatedFeedInception,
        root_delegation_bytes: &[u8],
    ) -> Result<(), CoreEngineError> {
        let validated_root = decode_actor_root_delegation(root_delegation_bytes)
            .map_err(|_| CoreEngineError::InvalidInput("invalid_root_delegation"))?;
        let scoped: ScopedDelegation = validated_root.scoped_delegation();
        {
            let mut ks = self
                .key_state
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner);
            ks.accept_root(scoped);
        }
        self.append_supersession_meta(|meta| {
            let entry = (
                feed.original_bytes().to_vec(),
                root_delegation_bytes.to_vec(),
            );
            if !meta.root_delegations.contains(&entry) {
                meta.root_delegations.push(entry);
            }
        })
    }

    pub fn register_supersession_object_author(
        &self,
        object_cid: ObjectCid,
        actor_id: ku_core::foundation::ActorId,
    ) -> Result<(), CoreEngineError> {
        {
            let mut sup = self
                .supersession
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner);
            sup.register_object_author(object_cid, actor_id)?;
        }
        self.append_supersession_meta(|meta| {
            let entry = (object_cid.into_bytes(), *actor_id.as_bytes());
            if !meta.object_authors.contains(&entry) {
                meta.object_authors.push(entry);
            }
        })
    }

    pub fn record_supersession_event(
        &self,
        feed: &ValidatedFeedInception,
        payload_object_bytes: &[u8],
        event_bytes: &[u8],
    ) -> Result<(SupersessionApplyOutcome, SupersessionLineageView), CoreEngineError> {
        let v_obj = decode_knowledge_object(
            payload_object_bytes,
            ResourceProfile::ObjectV1,
            &[KnownObjectKind::new(KNOWLEDGE_SUPERSESSION_KIND, 1)],
            &[],
        )?;
        let v_ev = decode_knowledge_event(event_bytes, feed, &[KNOWLEDGE_SUPERSESSION_EVENT_TYPE])
            .map_err(|_| CoreEngineError::InvalidInput("invalid_supersession_event"))?;
        let bound = ValidatedKnowledgeSupersessionEvent::bind(&v_ev, &v_obj)?;
        let pred_cid = bound.payload().predecessor_object_cid;
        let ev_key = bound.event_cid().into_bytes();

        let (outcome, view) = {
            let ks = self
                .key_state
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner);
            let mut sup = self
                .supersession
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner);
            let out = sup.apply_event(bound, feed, &ks)?;
            let v = sup.lineage_for(pred_cid);
            (out, v)
        };

        let row = PersistedSupersessionRow {
            feed_bytes: feed.original_bytes().to_vec(),
            payload_object_bytes: payload_object_bytes.to_vec(),
            event_bytes: event_bytes.to_vec(),
        };
        let row_bytes = serde_json::to_vec(&row)
            .map_err(|_| CoreEngineError::InvalidInput("supersession_serialize"))?;
        let write_txn = self.db.begin_write()?;
        {
            let mut table = write_txn.open_table(COMP_SUPERSESSION_LOG_V1)?;
            table.insert(&ev_key, row_bytes.as_slice())?;
        }
        write_txn.commit()?;

        Ok((outcome, view))
    }

    pub fn supersession_lineage(&self, predecessor: ObjectCid) -> SupersessionLineageView {
        let sup = self
            .supersession
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        sup.lineage_for(predecessor)
    }

    fn rebuild_reducers(&self) -> Result<(), CoreEngineError> {
        let read_txn = self.db.begin_read()?;

        // Rebuild reference disclosures from stored composition objects
        {
            let obj_table = read_txn.open_table(COMP_OBJECTS_V1)?;
            let mut disc = self
                .disclosures
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner);
            for item in obj_table.iter()? {
                let (key_g, bytes_g) = item?;
                let mut cid_bytes = [0u8; 32];
                cid_bytes.copy_from_slice(&key_g.value()[1..]);
                let v = decode_knowledge_object(
                    bytes_g.value(),
                    ResourceProfile::ObjectV1,
                    &[
                        KnownObjectKind::new(RECEPTOR_DEFINITION_KIND, 1),
                        KnownObjectKind::new(KNOWLEDGE_AFFORDANCE_KIND, 1),
                        KnownObjectKind::new(ASSEMBLY_MANIFEST_KIND, 1),
                    ],
                    &[],
                )?;
                let _ = disc.declare(&ObjectReference::new(0, cid_bytes), v.disclosure());
            }
        }

        // Rebuild ResolutionReducers
        {
            let res_table = read_txn.open_table(COMP_RESOLUTION_LOG_V1)?;
            let mut reducers = self
                .resolution_reducers
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner);
            for item in res_table.iter()? {
                let (_, row_g) = item?;
                let row: PersistedResolutionRow = serde_json::from_slice(row_g.value())
                    .map_err(|_| CoreEngineError::InvalidInput("corrupt_resolution_row"))?;
                let feed = decode_feed_inception(&row.feed_bytes)
                    .map_err(|_| CoreEngineError::InvalidInput("corrupt_resolution_feed"))?;
                let v_obj = decode_knowledge_object(
                    &row.action_object_bytes,
                    ResourceProfile::ObjectV1,
                    &[KnownObjectKind::new(RECEPTOR_RESOLUTION_ACTION_KIND, 1)],
                    &[],
                )?;
                let v_ev = decode_knowledge_event(
                    &row.event_bytes,
                    &feed,
                    &[RECEPTOR_RESOLUTION_EVENT_TYPE],
                )
                .map_err(|_| CoreEngineError::InvalidInput("corrupt_resolution_event"))?;
                let bound = ValidatedResolutionEvent::bind(&v_ev, &v_obj)?;
                let target = bound.payload().target;
                let policy = bound.payload().resolution_policy.clone();
                let frontier = bound.payload().observed_frontier;
                let authority = match row.authority_code {
                    0 => ResolutionAuthority::Authorized,
                    1 => ResolutionAuthority::Unauthorized,
                    _ => ResolutionAuthority::Unresolved,
                };
                let acceptance = row.acceptance_code.map(|c| match c {
                    0 => BindingAcceptance::Rejected,
                    1 => BindingAcceptance::Partial,
                    _ => BindingAcceptance::Satisfied,
                });
                let assessed =
                    assess_resolution_event(bound, authority, acceptance, &self.mapping_backend)?;
                let reducer = reducers
                    .entry(target_key(target))
                    .or_insert_with(|| ResolutionReducer::new(target, policy, frontier));
                reducer.set_assessed_frontier(frontier);
                reducer.apply(assessed)?;
            }
        }

        // Rebuild SupersessionLineageReducer
        {
            let meta_table = read_txn.open_table(COMP_SUPERSESSION_META_V1)?;
            let mut ks = self
                .key_state
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner);
            let mut sup = self
                .supersession
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner);
            if let Some(meta_g) = meta_table.get("meta")? {
                let meta: PersistedSupersessionMeta = serde_json::from_slice(meta_g.value())
                    .map_err(|_| CoreEngineError::InvalidInput("corrupt_supersession_meta"))?;
                for (_feed_b, root_b) in meta.root_delegations {
                    let root = decode_actor_root_delegation(&root_b)
                        .map_err(|_| CoreEngineError::InvalidInput("corrupt_root"))?;
                    ks.accept_root(root.scoped_delegation());
                }
                for (obj_b, act_b) in meta.object_authors {
                    sup.register_object_author(
                        ObjectCid::from_bytes(obj_b),
                        ku_core::foundation::ActorId::from_bytes(act_b),
                    )?;
                }
            }

            let log_table = read_txn.open_table(COMP_SUPERSESSION_LOG_V1)?;
            for item in log_table.iter()? {
                let (_, row_g) = item?;
                let row: PersistedSupersessionRow = serde_json::from_slice(row_g.value())
                    .map_err(|_| CoreEngineError::InvalidInput("corrupt_supersession_row"))?;
                let feed = decode_feed_inception(&row.feed_bytes)
                    .map_err(|_| CoreEngineError::InvalidInput("corrupt_feed"))?;
                let v_obj = decode_knowledge_object(
                    &row.payload_object_bytes,
                    ResourceProfile::ObjectV1,
                    &[KnownObjectKind::new(KNOWLEDGE_SUPERSESSION_KIND, 1)],
                    &[],
                )?;
                let v_ev = decode_knowledge_event(
                    &row.event_bytes,
                    &feed,
                    &[KNOWLEDGE_SUPERSESSION_EVENT_TYPE],
                )
                .map_err(|_| CoreEngineError::InvalidInput("corrupt_supersession_event"))?;
                let bound = ValidatedKnowledgeSupersessionEvent::bind(&v_ev, &v_obj)?;
                sup.apply_event(bound, &feed, &ks)?;
            }
        }
        Ok(())
    }

    fn store_composition_object(
        &self,
        tag: u8,
        cid: ObjectCid,
        bytes: &[u8],
        disclosure: DisclosureClass,
    ) -> Result<(), CoreEngineError> {
        let mut key = [0u8; 33];
        key[0] = tag;
        key[1..].copy_from_slice(cid.as_bytes());
        let write_txn = self.db.begin_write()?;
        {
            let mut table = write_txn.open_table(COMP_OBJECTS_V1)?;
            table.insert(&key, bytes)?;
        }
        write_txn.commit()?;
        self.declare_reference_disclosure(&ObjectReference::new(0, cid.into_bytes()), disclosure)?;
        Ok(())
    }

    fn load_composition_object(
        &self,
        tag: u8,
        cid: ObjectCid,
    ) -> Result<Option<Vec<u8>>, CoreEngineError> {
        let mut key = [0u8; 33];
        key[0] = tag;
        key[1..].copy_from_slice(cid.as_bytes());
        let read_txn = self.db.begin_read()?;
        let table = read_txn.open_table(COMP_OBJECTS_V1)?;
        Ok(table.get(&key)?.map(|g| g.value().to_vec()))
    }

    fn append_supersession_meta(
        &self,
        mutate: impl FnOnce(&mut PersistedSupersessionMeta),
    ) -> Result<(), CoreEngineError> {
        let write_txn = self.db.begin_write()?;
        {
            let mut table = write_txn.open_table(COMP_SUPERSESSION_META_V1)?;
            let mut current = match table.get("meta")? {
                Some(g) => serde_json::from_slice::<PersistedSupersessionMeta>(g.value())
                    .map_err(|_| CoreEngineError::InvalidInput("corrupt_supersession_meta"))?,
                None => PersistedSupersessionMeta::default(),
            };
            mutate(&mut current);
            let encoded = serde_json::to_vec(&current)
                .map_err(|_| CoreEngineError::InvalidInput("supersession_meta_serialize"))?;
            table.insert("meta", encoded.as_slice())?;
        }
        write_txn.commit()?;
        Ok(())
    }
}

fn disclosure_byte(d: DisclosureClass) -> u8 {
    match d {
        DisclosureClass::Public => 0,
        DisclosureClass::RouteMinimal => 1,
        DisclosureClass::NegotiatedEncrypted => 2,
        DisclosureClass::LocalOnly => 3,
    }
}

fn mapping_record_key(dest: u8, kind: MappingRecordKind, cid: &[u8; 32]) -> [u8; 34] {
    let mut out = [0u8; 34];
    out[0] = dest;
    out[1] = match kind {
        MappingRecordKind::Kernel => 0,
        MappingRecordKind::Envelope => 1,
    };
    out[2..].copy_from_slice(cid);
    out
}

fn encode_op_tuple(kernel: MappingKernelCid, envelope: ObjectCid, dest: u8) -> [u8; 72] {
    let mut out = [0u8; 72];
    out[..32].copy_from_slice(kernel.as_bytes());
    out[32..64].copy_from_slice(envelope.as_bytes());
    out[64] = dest;
    out
}

fn target_key(target: ResolutionTarget) -> ResolutionTargetKey {
    (
        *target.assembly_lineage.as_bytes(),
        *target.assembly_revision.as_bytes(),
        *target.placement.as_bytes(),
    )
}

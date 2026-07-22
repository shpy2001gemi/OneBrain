//! Explicit durable boundary between ephemeral proposals and Mapping records.

use std::collections::{BTreeMap, HashMap};
use std::sync::Mutex;

use super::canonical::ResourceProfile;
use super::content_id::{MappingKernelCid, ObjectCid, PermitCid};
use super::identity::ActorId;
use super::mapping::{MappingEnvelope, MappingError, MappingKernel, MappingTransform};
use super::object::{DisclosureClass, ObjectError, ObjectReference};
use super::resolution::MaterializedMappingLookup;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MaterializationIntent {
    PinPrivate,
    Archive,
    Publish,
    DurableUse,
    Derive,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MaterializationAuthority {
    Authorized,
    Unauthorized,
    Unresolved,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct MaterializeMappingCommand {
    pub mapping_kernel: MappingKernel,
    pub mapping_envelope: MappingEnvelope,
    pub intent: MaterializationIntent,
    pub authorization_ref: Option<PermitCid>,
    pub destination: DisclosureClass,
    pub idempotency_key: [u8; 32],
    pub requester: ActorId,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum MappingRecordKind {
    Kernel,
    Envelope,
}

#[derive(Clone, Copy, Debug)]
pub struct MappingWriteBatch<'a> {
    pub destination: DisclosureClass,
    pub kernel_cid: MappingKernelCid,
    pub kernel_bytes: &'a [u8],
    pub envelope_cid: ObjectCid,
    pub envelope_bytes: &'a [u8],
    pub idempotency_key: [u8; 32],
}

#[doc(hidden)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum BackendMappingOutcome {
    Stored,
    AlreadyPresent,
    IdempotentReplay,
    Collision,
    IdempotencyConflict,
}

pub trait AtomicMappingBackend: Send + Sync {
    /// Preflight and commit both records under one backend transaction/lock.
    /// A collision or idempotency conflict must write neither record.
    fn store_pair_atomically(
        &self,
        batch: MappingWriteBatch<'_>,
    ) -> Result<BackendMappingOutcome, String>;

    fn get(
        &self,
        destination: DisclosureClass,
        kind: MappingRecordKind,
        cid: &[u8; 32],
    ) -> Result<Option<Vec<u8>>, String>;
}

#[derive(Default)]
struct MemoryMappingState {
    records: HashMap<(u64, MappingRecordKind, [u8; 32]), Vec<u8>>,
    operations: HashMap<[u8; 32], ([u8; 32], [u8; 32], u64)>,
}

/// Deterministic conformance backend. Production private destinations must use
/// an encrypted Vault-backed implementation of [`AtomicMappingBackend`].
#[derive(Default)]
pub struct InMemoryMappingBackend {
    state: Mutex<MemoryMappingState>,
}

impl AtomicMappingBackend for InMemoryMappingBackend {
    fn store_pair_atomically(
        &self,
        batch: MappingWriteBatch<'_>,
    ) -> Result<BackendMappingOutcome, String> {
        let mut state = self
            .state
            .lock()
            .map_err(|_| "MAPPING_STORE_LOCK_POISONED".to_string())?;
        let destination = disclosure_code(batch.destination);
        let identity = (
            batch.kernel_cid.into_bytes(),
            batch.envelope_cid.into_bytes(),
            destination,
        );
        if let Some(existing) = state.operations.get(&batch.idempotency_key) {
            return Ok(if *existing == identity {
                BackendMappingOutcome::IdempotentReplay
            } else {
                BackendMappingOutcome::IdempotencyConflict
            });
        }

        let kernel_key = (
            destination,
            MappingRecordKind::Kernel,
            batch.kernel_cid.into_bytes(),
        );
        let envelope_key = (
            destination,
            MappingRecordKind::Envelope,
            batch.envelope_cid.into_bytes(),
        );
        let kernel_existing = state.records.get(&kernel_key);
        let envelope_existing = state.records.get(&envelope_key);
        if kernel_existing.is_some_and(|bytes| bytes != batch.kernel_bytes)
            || envelope_existing.is_some_and(|bytes| bytes != batch.envelope_bytes)
        {
            return Ok(BackendMappingOutcome::Collision);
        }
        let both_present = kernel_existing.is_some() && envelope_existing.is_some();
        state
            .records
            .entry(kernel_key)
            .or_insert_with(|| batch.kernel_bytes.to_vec());
        state
            .records
            .entry(envelope_key)
            .or_insert_with(|| batch.envelope_bytes.to_vec());
        state.operations.insert(batch.idempotency_key, identity);
        Ok(if both_present {
            BackendMappingOutcome::AlreadyPresent
        } else {
            BackendMappingOutcome::Stored
        })
    }

    fn get(
        &self,
        destination: DisclosureClass,
        kind: MappingRecordKind,
        cid: &[u8; 32],
    ) -> Result<Option<Vec<u8>>, String> {
        Ok(self
            .state
            .lock()
            .map_err(|_| "MAPPING_STORE_LOCK_POISONED".to_string())?
            .records
            .get(&(disclosure_code(destination), kind, *cid))
            .cloned())
    }
}

#[derive(Clone, Debug, Default)]
pub struct ReferenceDisclosureIndex {
    classes: BTreeMap<(u64, [u8; 32]), DisclosureClass>,
}

impl ReferenceDisclosureIndex {
    pub fn declare(
        &mut self,
        reference: &ObjectReference,
        disclosure: DisclosureClass,
    ) -> Result<(), MaterializationError> {
        let key = (reference.reference_kind, reference.cid);
        match self.classes.get(&key) {
            Some(existing) if *existing != disclosure => {
                Err(MaterializationError::ConflictingReferenceDisclosure)
            }
            Some(_) => Ok(()),
            None => {
                self.classes.insert(key, disclosure);
                Ok(())
            }
        }
    }

    fn get(&self, reference: &ObjectReference) -> Option<DisclosureClass> {
        self.classes
            .get(&(reference.reference_kind, reference.cid))
            .copied()
    }
}

pub struct MappingMaterializer<B> {
    backend: B,
}

impl<B: AtomicMappingBackend> MappingMaterializer<B> {
    pub const fn new(backend: B) -> Self {
        Self { backend }
    }

    pub fn materialize(
        &self,
        command: &MaterializeMappingCommand,
        authority: MaterializationAuthority,
        disclosures: &ReferenceDisclosureIndex,
    ) -> Result<MaterializedMapping, MaterializationError> {
        match authority {
            MaterializationAuthority::Unauthorized => {
                return Err(MaterializationError::Unauthorized)
            }
            MaterializationAuthority::Unresolved => {
                return Err(MaterializationError::AuthorityUnresolved)
            }
            MaterializationAuthority::Authorized => {}
        }
        validate_command(command)?;
        let kernel_cid = command.mapping_kernel.cid()?;
        if kernel_cid != command.mapping_envelope.kernel {
            return Err(MaterializationError::KernelEnvelopeMismatch);
        }
        for reference in mapping_references(command) {
            let source = disclosures
                .get(reference)
                .ok_or(MaterializationError::ReferenceDisclosureUnknown)?;
            if !disclosure_can_flow(source, command.destination) {
                return Err(MaterializationError::DisclosureTaint);
            }
        }

        let kernel_bytes = command.mapping_kernel.canonical_bytes()?;
        let (envelope_bytes, envelope_cid) = command
            .mapping_envelope
            .to_knowledge_object(command.destination)?
            .encode(ResourceProfile::ObjectV1)?;
        let outcome = match self
            .backend
            .store_pair_atomically(MappingWriteBatch {
                destination: command.destination,
                kernel_cid,
                kernel_bytes: &kernel_bytes,
                envelope_cid,
                envelope_bytes: &envelope_bytes,
                idempotency_key: command.idempotency_key,
            })
            .map_err(MaterializationError::Backend)?
        {
            BackendMappingOutcome::Stored => MaterializationOutcome::Stored,
            BackendMappingOutcome::AlreadyPresent => MaterializationOutcome::AlreadyPresent,
            BackendMappingOutcome::IdempotentReplay => MaterializationOutcome::IdempotentReplay,
            BackendMappingOutcome::Collision => return Err(MaterializationError::Collision),
            BackendMappingOutcome::IdempotencyConflict => {
                return Err(MaterializationError::IdempotencyConflict)
            }
        };
        Ok(MaterializedMapping {
            kernel_cid,
            envelope_cid,
            destination: command.destination,
            outcome,
        })
    }

    pub fn get_kernel(
        &self,
        destination: DisclosureClass,
        cid: MappingKernelCid,
    ) -> Result<Option<Vec<u8>>, MaterializationError> {
        self.backend
            .get(destination, MappingRecordKind::Kernel, cid.as_bytes())
            .map_err(MaterializationError::Backend)
    }

    pub fn get_envelope(
        &self,
        destination: DisclosureClass,
        cid: ObjectCid,
    ) -> Result<Option<Vec<u8>>, MaterializationError> {
        self.backend
            .get(destination, MappingRecordKind::Envelope, cid.as_bytes())
            .map_err(MaterializationError::Backend)
    }

    pub fn is_materialized(&self, cid: MappingKernelCid) -> Result<bool, MaterializationError> {
        for destination in [
            DisclosureClass::Public,
            DisclosureClass::NegotiatedEncrypted,
            DisclosureClass::LocalOnly,
        ] {
            if self.get_kernel(destination, cid)?.is_some() {
                return Ok(true);
            }
        }
        Ok(false)
    }
}

impl<B: AtomicMappingBackend> MaterializedMappingLookup for MappingMaterializer<B> {
    fn contains_materialized_mapping(&self, mapping: MappingKernelCid) -> Result<bool, String> {
        self.is_materialized(mapping)
            .map_err(|error| format!("{error:?}"))
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MaterializationOutcome {
    Stored,
    AlreadyPresent,
    IdempotentReplay,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct MaterializedMapping {
    pub kernel_cid: MappingKernelCid,
    pub envelope_cid: ObjectCid,
    pub destination: DisclosureClass,
    pub outcome: MaterializationOutcome,
}

fn validate_command(command: &MaterializeMappingCommand) -> Result<(), MaterializationError> {
    if command.idempotency_key == [0; 32] {
        return Err(MaterializationError::InvalidIdempotencyKey);
    }
    if command.destination == DisclosureClass::RouteMinimal {
        return Err(MaterializationError::InvalidDestination);
    }
    match command.intent {
        MaterializationIntent::PinPrivate
            if !matches!(
                command.destination,
                DisclosureClass::LocalOnly | DisclosureClass::NegotiatedEncrypted
            ) =>
        {
            Err(MaterializationError::IntentDestinationMismatch)
        }
        MaterializationIntent::Publish if command.destination != DisclosureClass::Public => {
            Err(MaterializationError::IntentDestinationMismatch)
        }
        _ => Ok(()),
    }
}

fn mapping_references(command: &MaterializeMappingCommand) -> Vec<&ObjectReference> {
    let mut references = Vec::new();
    references.extend(command.mapping_kernel.source_objects.iter());
    references.extend(command.mapping_kernel.target_objects.iter());
    for correspondence in &command.mapping_kernel.correspondences {
        references.push(&correspondence.source.object);
        references.push(&correspondence.target.object);
        if let MappingTransform::ExplicitRule { rule } = &correspondence.transform {
            references.push(rule);
        }
    }
    for region in &command.mapping_kernel.unmapped_regions {
        references.push(&region.locator.object);
    }
    references.push(&command.mapping_envelope.generator);
    references.extend(command.mapping_envelope.derivation_rule.iter());
    references.extend(command.mapping_envelope.evidence.iter());
    references
}

fn disclosure_can_flow(source: DisclosureClass, destination: DisclosureClass) -> bool {
    match destination {
        DisclosureClass::Public => source == DisclosureClass::Public,
        DisclosureClass::NegotiatedEncrypted => matches!(
            source,
            DisclosureClass::Public | DisclosureClass::NegotiatedEncrypted
        ),
        DisclosureClass::LocalOnly => source != DisclosureClass::RouteMinimal,
        DisclosureClass::RouteMinimal => false,
    }
}

const fn disclosure_code(disclosure: DisclosureClass) -> u64 {
    disclosure as u64
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum MaterializationError {
    Mapping(MappingError),
    Object(ObjectError),
    Backend(String),
    Unauthorized,
    AuthorityUnresolved,
    InvalidIdempotencyKey,
    InvalidDestination,
    IntentDestinationMismatch,
    KernelEnvelopeMismatch,
    ReferenceDisclosureUnknown,
    ConflictingReferenceDisclosure,
    DisclosureTaint,
    IdempotencyConflict,
    Collision,
}

impl From<MappingError> for MaterializationError {
    fn from(error: MappingError) -> Self {
        Self::Mapping(error)
    }
}

impl From<ObjectError> for MaterializationError {
    fn from(error: ObjectError) -> Self {
        Self::Object(error)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::foundation::{
        CorrespondenceKind, MappingSide, MappingTermLocator, ResolutionReducer, ResolutionState,
        ResolutionTarget, SemanticFrameSet, TermCorrespondence, UnmappedRegion,
    };

    fn reference(byte: u8) -> ObjectReference {
        ObjectReference::new(0, [byte; 32])
    }

    fn command(destination: DisclosureClass, idempotency: u8) -> MaterializeMappingCommand {
        let source = MappingTermLocator {
            object: reference(1),
            statement_index: 0,
            argument_index: Some(0),
        };
        let target = MappingTermLocator {
            object: reference(2),
            statement_index: 0,
            argument_index: Some(1),
        };
        let kernel = MappingKernel {
            source_objects: vec![reference(1)],
            target_objects: vec![reference(2)],
            correspondences: vec![TermCorrespondence {
                source: source.clone(),
                target,
                kind: CorrespondenceKind::Analogous,
                transform: MappingTransform::Identity,
            }],
            assumptions: SemanticFrameSet {
                statements: Vec::new(),
            },
            constraint_regions: Vec::new(),
            unmapped_regions: vec![UnmappedRegion {
                side: MappingSide::Source,
                locator: source,
                reason: crate::foundation::ConceptCcid::from_bytes([9; 16]),
            }],
        };
        let kernel_id = kernel.cid().unwrap();
        MaterializeMappingCommand {
            mapping_kernel: kernel,
            mapping_envelope: MappingEnvelope {
                kernel: kernel_id,
                generator: reference(3),
                derivation_rule: Some(reference(4)),
                evidence: vec![reference(5)],
                source_event: None,
            },
            intent: if destination == DisclosureClass::Public {
                MaterializationIntent::Publish
            } else {
                MaterializationIntent::PinPrivate
            },
            authorization_ref: None,
            destination,
            idempotency_key: [idempotency; 32],
            requester: ActorId::from_bytes([6; 32]),
        }
    }

    fn disclosures(class: DisclosureClass) -> ReferenceDisclosureIndex {
        let mut index = ReferenceDisclosureIndex::default();
        for byte in 1..=5 {
            index.declare(&reference(byte), class).unwrap();
        }
        index
    }

    #[test]
    fn qa006_materialization_pair_is_atomic_and_idempotent() {
        let materializer = MappingMaterializer::new(InMemoryMappingBackend::default());
        let command = command(DisclosureClass::Public, 7);
        let first = materializer
            .materialize(
                &command,
                MaterializationAuthority::Authorized,
                &disclosures(DisclosureClass::Public),
            )
            .unwrap();
        assert_eq!(first.outcome, MaterializationOutcome::Stored);
        assert!(materializer
            .get_kernel(DisclosureClass::Public, first.kernel_cid)
            .unwrap()
            .is_some());
        assert!(materializer
            .get_envelope(DisclosureClass::Public, first.envelope_cid)
            .unwrap()
            .is_some());
        assert!(materializer.is_materialized(first.kernel_cid).unwrap());
        assert_eq!(
            materializer
                .materialize(
                    &command,
                    MaterializationAuthority::Authorized,
                    &disclosures(DisclosureClass::Public),
                )
                .unwrap()
                .outcome,
            MaterializationOutcome::IdempotentReplay
        );
    }

    #[test]
    fn public_destination_rejects_private_or_unknown_reference_taint() {
        let materializer = MappingMaterializer::new(InMemoryMappingBackend::default());
        let command = command(DisclosureClass::Public, 7);
        assert_eq!(
            materializer
                .materialize(
                    &command,
                    MaterializationAuthority::Authorized,
                    &disclosures(DisclosureClass::LocalOnly),
                )
                .unwrap_err(),
            MaterializationError::DisclosureTaint
        );
        assert_eq!(
            materializer
                .materialize(
                    &command,
                    MaterializationAuthority::Authorized,
                    &ReferenceDisclosureIndex::default(),
                )
                .unwrap_err(),
            MaterializationError::ReferenceDisclosureUnknown
        );
    }

    #[test]
    fn unauthorized_command_and_idempotency_conflict_write_nothing_extra() {
        let materializer = MappingMaterializer::new(InMemoryMappingBackend::default());
        let first_command = command(DisclosureClass::Public, 7);
        assert_eq!(
            materializer
                .materialize(
                    &first_command,
                    MaterializationAuthority::Unauthorized,
                    &disclosures(DisclosureClass::Public),
                )
                .unwrap_err(),
            MaterializationError::Unauthorized
        );
        assert!(materializer
            .get_kernel(
                DisclosureClass::Public,
                first_command.mapping_kernel.cid().unwrap()
            )
            .unwrap()
            .is_none());

        materializer
            .materialize(
                &first_command,
                MaterializationAuthority::Authorized,
                &disclosures(DisclosureClass::Public),
            )
            .unwrap();
        let conflicting = command(DisclosureClass::Public, 7);
        // Same operation key but a different destination identity.
        let mut conflicting = conflicting;
        conflicting.destination = DisclosureClass::LocalOnly;
        conflicting.intent = MaterializationIntent::PinPrivate;
        assert_eq!(
            materializer
                .materialize(
                    &conflicting,
                    MaterializationAuthority::Authorized,
                    &disclosures(DisclosureClass::Public),
                )
                .unwrap_err(),
            MaterializationError::IdempotencyConflict
        );
    }

    #[test]
    fn backend_collision_preflight_prevents_partial_pair() {
        let backend = InMemoryMappingBackend::default();
        let kernel = MappingKernelCid::from_bytes([1; 32]);
        let envelope_a = ObjectCid::from_bytes([2; 32]);
        let envelope_b = ObjectCid::from_bytes([3; 32]);
        assert_eq!(
            backend
                .store_pair_atomically(MappingWriteBatch {
                    destination: DisclosureClass::Public,
                    kernel_cid: kernel,
                    kernel_bytes: b"kernel-a",
                    envelope_cid: envelope_a,
                    envelope_bytes: b"envelope-a",
                    idempotency_key: [4; 32],
                })
                .unwrap(),
            BackendMappingOutcome::Stored
        );
        assert_eq!(
            backend
                .store_pair_atomically(MappingWriteBatch {
                    destination: DisclosureClass::Public,
                    kernel_cid: kernel,
                    kernel_bytes: b"kernel-collision",
                    envelope_cid: envelope_b,
                    envelope_bytes: b"envelope-b",
                    idempotency_key: [5; 32],
                })
                .unwrap(),
            BackendMappingOutcome::Collision
        );
        assert!(backend
            .get(
                DisclosureClass::Public,
                MappingRecordKind::Envelope,
                envelope_b.as_bytes()
            )
            .unwrap()
            .is_none());
    }

    #[test]
    fn qa006_materialization_does_not_adopt_or_change_resolution() {
        let target = ResolutionTarget {
            assembly_lineage: crate::foundation::AssemblyLineageId::from_bytes([10; 32]),
            assembly_revision: ObjectCid::from_bytes([11; 32]),
            placement: crate::foundation::PlacementId::from_bytes([12; 32]),
        };
        let reducer = ResolutionReducer::new(target, reference(13), [14; 32]);
        let materializer = MappingMaterializer::new(InMemoryMappingBackend::default());
        materializer
            .materialize(
                &command(DisclosureClass::Public, 7),
                MaterializationAuthority::Authorized,
                &disclosures(DisclosureClass::Public),
            )
            .unwrap();
        assert_eq!(reducer.view().unwrap().state, ResolutionState::Open);
    }
}

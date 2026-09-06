//! OneBrain vNext foundation contracts.
//!
//! This module is deliberately side-by-side with the KU v7 Core DNA codec. It
//! does not reinterpret, rewrite, or replace existing Core DNA/CCID bytes.

pub mod actor_root;
pub mod affordance;
pub mod assembly;
pub mod authority;
pub mod authority_event;
pub mod base_profile;
pub mod blob_reference;
pub mod canonical;
pub mod capability;
pub mod capability_offer;
pub mod capability_permit;
pub mod checkpoint;
pub mod checkpoint_compaction;
pub mod conformance;
pub mod content_id;
pub mod dr_m5_failpoint;
pub mod envelope;
pub mod event;
pub mod feed;
pub mod feed_signer;
pub mod feed_store;
pub mod fidelity;
pub mod fidelity_assessment;
pub mod identity;
pub mod inventory;
pub mod key_state;
pub mod mapping;
pub mod materialization;
pub mod metabolic_view;
pub mod migration;
pub mod object;
pub mod observation;
pub mod operational_compaction;
pub mod outcome_evidence;
pub mod provider;
pub mod receptor;
pub mod resolution;
pub mod revocation;
pub mod schema_registry;
pub mod semantic;
pub mod semantic_content;
pub mod source_text;
pub mod storage;
pub mod use_evidence;
pub mod vault;

pub use actor_root::{
    actor_id_from_root_key, decode_actor_root_delegation, ActorRootDelegation,
    ActorRootDelegationError, SignedActorRootDelegation, ValidatedActorRootDelegation,
};
pub use affordance::{
    AcceptedInput, AffordanceError, AffordanceOrigin, AffordanceSemantics, KnowledgeAffordance,
    KNOWLEDGE_AFFORDANCE_KIND,
};
pub use assembly::{
    AssemblyError, AssemblyLineageId, FrontierAssemblyManifest, PlacementId, ReceptorPlacement,
    ASSEMBLY_MANIFEST_KIND,
};
pub use authority::{
    validate_successor_structure, AcceptedRevocation, DelegationGrant, FeedAuthorityDecision,
    FeedAuthorityView, FeedSuccessorDecision, SuccessorStructureError, UnresolvedAuthorityReason,
};
pub use authority_event::{
    authority_event_descriptor, decode_actor_delegation, decode_actor_revocation, ActorDelegation,
    ActorRevocation, AuthorityEventDescriptor, AuthorityEventError, SignedActorDelegation,
    SignedActorRevocation, ValidatedActorDelegation, ValidatedActorRevocation,
};
pub use base_profile::{
    base_v1_profile_digest, base_v1_profile_registry, BaseProfileRegistry,
    StorageOwnerRegistryEntry, BASE_PROFILE_MAJOR, STORAGE_OWNERS_V1,
};
pub use blob_reference::{
    BlobReferenceError, BlobRetentionState, OwnedBlobReferenceV1, OwnedBlobRole,
    OWNED_BLOB_REFERENCE_SCHEMA_MAJOR, OWNED_BLOB_REFERENCE_SCHEMA_MINOR,
};
pub use canonical::{
    canonicalize_set_by_key, decode_canonical, encode_canonical, CanonicalDocument, CanonicalError,
    CanonicalErrorKind, CanonicalValue, NormalizedText, ResourceLimits, ResourceProfile,
};
pub use capability::{
    CapabilityDefinition, CapabilityDeterminism, CapabilityError, CapabilityExecutionRecordBody,
    CapabilityExecutionState, CapabilityImplementationManifest, CapabilityImplementationSelector,
    CapabilityLayer, CapabilityOfferBody, CapabilityPrivacyMode, CapabilityProviderPrincipal,
    CapabilityResourceBuckets, DelegationPermitBody, OperationalCommitment,
    OperationalCommitmentKind, RetentionRule, CAPABILITY_DEFINITION_KIND,
    IMPLEMENTATION_MANIFEST_KIND,
};
pub use capability_offer::{
    decode_capability_offer, CapabilityOfferApplyOutcome, CapabilityOfferError,
    CapabilityOfferIdentity, CapabilityOfferReducer, SignedCapabilityOffer,
    ValidatedCapabilityOffer,
};
pub use capability_permit::{
    authenticate_delegation_permit, AuthenticatedDelegationPermit, PermitApplyOutcome,
    PermitAuthorityDecision, PermitExecutionScope, PermitValidationError, PermitValidator,
    SignedDelegationPermit, ValidatedDelegationPermit,
};
pub use checkpoint::{
    assess_checkpoint_extension_suppression, assess_checkpoint_suppression, decode_feed_checkpoint,
    CheckpointApplyOutcome, CheckpointConflictProof, CheckpointEffectVerifier, CheckpointError,
    CheckpointExtensionBinding, CheckpointHistoryWitness, CheckpointLeaf, CheckpointRegister,
    CheckpointSuppressionAssessment, FeedCheckpointBody, MerkleInclusionProof, MerkleSibling,
    SignedFeedCheckpoint, ValidatedCheckpointConsistency, ValidatedCheckpointSuppression,
    ValidatedFeedCheckpoint, CHECKPOINT_PROFILE_MAJOR, CHECKPOINT_PROFILE_MINOR,
    MAX_CHECKPOINT_LEAVES, MAX_CHECKPOINT_PROOF_DEPTH,
};
pub use checkpoint_compaction::{
    execute_local_eviction, validate_custody_receipt, ApprovedLocalEviction, ArchiveEntry,
    ArchiveManifest, CheckpointRestoreRebuilder, CompactionError, CustodyReceiptBody,
    ExactHighWaterAnchors, ExactHighWaterEntry, GcBlockReason, HighWaterLane,
    HighWaterObserveOutcome, LocalEvictionCoordinator, LocalGcGate, LocalPayloadBackend,
    LocalRetentionPolicy, PayloadClass, PayloadDescriptor, PayloadRecordId, ProofedPayload,
    RestoreDrill, RestoreDrillFailure, RestoreDrillReport, RetentionAction, ShadowBlockReason,
    ShadowCompactionPlan, ShadowCompactionPlanner, SignedCustodyReceipt, ValidatedCustodyReceipt,
    COMPACTION_PROFILE_MAJOR, COMPACTION_PROFILE_MINOR, MAX_COMPACTION_RECORDS,
    MAX_HIGH_WATER_ANCHORS,
};
pub use content_id::{
    signature_message, CheckpointCid, EventCid, FeedHeadCid, FeedIdMaterial, LeaseCid, ManifestCid,
    MappingKernelCid, ObjectCid, PermitCid, ReservedDomain, SelectorCid, SemanticContentCid,
    TypedDigest, VectorCid,
};
pub use event::{
    decode_knowledge_event, event_author_feed, EventReadiness, EventReplayGuard,
    EventReplayOutcome, EventType, KnowledgeEventEnvelope, SignedKnowledgeEvent,
    ValidatedKnowledgeEvent,
};
pub use feed::{
    decode_feed_inception, FeedInception, NamespaceCommitment, SignedFeedInception,
    ValidatedFeedInception,
};
pub use feed_signer::{FeedEventSigner, FeedSignerError, ProvenFeedEventSigner};
pub use feed_store::{
    FeedEquivocationProof, FeedInsertOutcome, FeedProjection, FeedSuccessorProof, SequenceRange,
    UnresolvedFeedPosition, ValidatedFeedStore,
};
pub use fidelity::{
    CorrelationDimension, CorrelationDimensionEvidence, CorrelationEvidence, EncodingAttempt,
    EncodingAttemptRole, EncodingFidelityAttestation, EvidenceStrength, FidelityCheck,
    FidelityCheckKind, FidelityCheckStatus, FidelityError, FidelityPolicy,
    ValidatedEncodingFidelityAttestationEvent, ENCODING_ATTEMPT_KIND,
    ENCODING_FIDELITY_ATTESTATION_EVENT_TYPE, ENCODING_FIDELITY_ATTESTATION_KIND,
    FIDELITY_POLICY_KIND, FIDELITY_PROFILE_MAJOR, FIDELITY_PROFILE_MINOR,
};
pub use fidelity_assessment::{
    FidelityAssessment, FidelityAssessmentError, FidelityAssessmentReducer,
    FidelityAssessmentStatus, FidelityEvidenceCoverage, FidelityRecordOutcome, LegacyEncodingClaim,
};
pub use identity::{ActorId, CrdtDot, DeviceId, FeedId, FullWidthClock, NodeId, TypedIdentity};
pub use inventory::{
    public_knowledge_exchange_fixture_v1, Budget, CarrierKind, CarrierProfile, CoverageBasis,
    CoverageLimitation, CoverageStatement, CoverageStatus, InventoryError, InventoryRecordKind,
    Selector, SelectorOffer, SelectorPurpose, SELECTOR_PROFILE_MAJOR, SELECTOR_PROFILE_MINOR,
};
pub use key_state::{
    KeyStateApplyOutcome, KeyStateCheckpointProof, KeyStateRecordStatus, KeyStateReducer,
    ScopedDelegation, ScopedRevocation,
};
pub use mapping::{
    CorrespondenceKind, MappingConstraintRegion, MappingEnvelope, MappingError, MappingKernel,
    MappingSide, MappingTermLocator, MappingTransform, TermCorrespondence, UnmappedRegion,
    MAPPING_ENVELOPE_KIND,
};
pub use materialization::{
    AtomicMappingBackend, InMemoryMappingBackend, MappingMaterializer, MappingRecordKind,
    MappingWriteBatch, MaterializationAuthority, MaterializationError, MaterializationIntent,
    MaterializationOutcome, MaterializeMappingCommand, MaterializedMapping,
    ReferenceDisclosureIndex,
};
pub use metabolic_view::{
    ExposureKind, ExposureObservation, ExposureRecordOutcome, ExposureTelemetry,
    MetabolicEvidenceFrontier, MetabolicEvidenceReducer, MetabolicEvidenceView,
    MetabolicRecordOutcome, MetabolicViewError, MetabolicViewLimitation, MetabolicViewPolicy,
    RecentExerciseActivity, RecentExerciseKind, MAX_ACCEPTED_EVIDENCE_POLICIES,
    MAX_EXPOSURE_OBSERVATIONS, MAX_METABOLIC_EVIDENCE_RECORDS, METABOLIC_VIEW_MAJOR,
    METABOLIC_VIEW_MINOR,
};
#[cfg(feature = "persist")]
pub use migration::RedbMigrationBackend;
pub use migration::{
    AtomicMigrationBackend, BackendMigrationOutcome, DualReadRecord, InMemoryMigrationBackend,
    LegacyDataClass, LegacyIdentityPrefix, LegacyRowKey, LegacyRowNormalizer, LegacySourceRow,
    MigrationBatchJournal, MigrationBatchOutcome, MigrationDisposition, MigrationError,
    MigrationJournalEntry, MigrationQuarantineRecord, MigrationRejection,
    MigrationStateSnapshotPort, MigrationStore, NormalizedLegacyRow, PortableMigrationSnapshot,
    ReadOnlyLegacyRow, StoredVNextMigration, ValidatedMigrationRestorePort,
    MAX_LEGACY_PRIMARY_KEY_BYTES, MAX_LEGACY_ROW_BYTES, MAX_MIGRATION_REASON_BYTES,
    MIGRATION_PROFILE_MAJOR,
};
pub use object::{
    decode_knowledge_object, DisclosureClass, KnowledgeObjectEnvelope, KnownObjectKind,
    ObjectError, ObjectKind, ObjectLimits, ObjectReference, ObjectSemantics, SchemaVersion,
    ValidatedKnowledgeObject, KNOWLEDGE_OBJECT_SCHEMA_ID,
};
pub use observation::{
    ObservationError, ObservationEventPayload, ObservationGovernance, SourceArtifact,
    SourceArtifactKind, ValidatedObservationEvent, MAX_OBSERVATION_LIMITATIONS,
    MAX_OBSERVATION_SPANS, MAX_RAW_OBSERVATION_BYTES, OBSERVATION_EVENT_PAYLOAD_KIND,
    OBSERVATION_EVENT_TYPE, OBSERVATION_PROFILE_MAJOR, OBSERVATION_PROFILE_MINOR,
    SOURCE_ARTIFACT_KIND,
};
pub use operational_compaction::{
    CompactionFenceError, OperationalCompactionPermit, OperationalCompactionSwitch,
};
pub use outcome_evidence::{
    AffectedPrincipal, AffectedScope, AssessedOutcomeObservation, AttributionStatus,
    BenefitEvidencePayload, EvidenceLimitation, OutcomeAuthority, OutcomeBranchRecord,
    OutcomeBranchView, OutcomeCaseId, OutcomeEvidenceError, OutcomeEvidenceReducer,
    OutcomeObservationPayload, OutcomeRecordResult, OutcomeValence, ValidatedBenefitEvidenceEvent,
    ValidatedOutcomeObservationEvent, BENEFIT_EVIDENCE_EVENT_TYPE, BENEFIT_EVIDENCE_KIND,
    MAX_OUTCOME_LIMITATIONS, MAX_OUTCOME_REFERENCES, OUTCOME_EVIDENCE_MAJOR,
    OUTCOME_EVIDENCE_MINOR, OUTCOME_OBSERVATION_EVENT_TYPE, OUTCOME_OBSERVATION_KIND,
};
pub use provider::{
    decode_provider_lease, decode_provider_retire, LeaseObservationOutcome, LeaseObservationStore,
    ProviderApplyOutcome, ProviderLeaseBody, ProviderLeaseMap, ProviderOfferKind,
    ProviderPrincipal, ProviderRecordError, ProviderRetireBody, ProviderSubject, ProviderTuple,
    SignedProviderLease, SignedProviderRetire, ValidatedProviderLease, ValidatedProviderRetire,
    MAX_PROVIDER_CAPABILITY_CLASSES, MAX_PROVIDER_ENDPOINTS, MAX_PROVIDER_LEASE_TICKS,
    PROVIDER_RECORD_MAJOR, PROVIDER_RECORD_MINOR,
};
pub use receptor::{
    ReceptorAcceptanceProfile, ReceptorCardinality, ReceptorClaimCommitment, ReceptorClaimEnvelope,
    ReceptorClaimValue, ReceptorDefinition, ReceptorError, ReceptorOrigin, StatementLocator,
    UnknownConstraintPolicy, RECEPTOR_CLAIM_KIND, RECEPTOR_DEFINITION_KIND,
};
pub use resolution::{
    assess_resolution_event, AssessedResolutionEvent, BindingAcceptance, MaterializedMappingLookup,
    ResolutionAction, ResolutionActionPayload, ResolutionApplyOutcome, ResolutionAuthority,
    ResolutionBranch, ResolutionError, ResolutionReducer, ResolutionState, ResolutionTarget,
    ResolutionView, ValidatedResolutionEvent, RECEPTOR_RESOLUTION_ACTION_KIND,
    RECEPTOR_RESOLUTION_EVENT_TYPE,
};
pub use revocation::{
    AuthorityFreshnessObservation, AuthorityScope, FreshnessWindows, ObservedAuthorityState,
    RevocationAction, RevocationCheck, RevocationFreshnessDecision, RevocationFreshnessError,
    RevocationFreshnessEvaluator, RevocationFreshnessProfile, RevocationRisk,
    TaskSpecificDtnProfile, TASK_SPECIFIC_DTN_PROFILE, TERRESTRIAL_INTERACTIVE_PROFILE,
};
pub use semantic::{
    ComparisonOperator, ConceptCcid, ConstraintEvaluation, ConstraintExpression, DimensionVector,
    ExactRatio, LiteralValue, Modality, QuantityLiteral, ReceptorSlotId, SemanticError,
    SemanticFrameSet, SourceSpan, StatementFrame, StatementId, StatementQualifiers, TermRef,
    TypedConstraint, UnitRef, VariableId, SEMANTIC_KERNEL_OBJECT_KIND,
};
pub use source_text::{
    source_text_digest, BoundedUtf8, LocalSourceTextRecordV1, SourceTextError,
    LOCAL_SOURCE_TEXT_KIND, LOCAL_SOURCE_TEXT_KNOWN_KIND, MAX_LOCAL_SOURCE_TEXT_BYTES,
};
#[cfg(feature = "persist")]
pub use storage::RedbVerifiedBackend;
pub use storage::{
    AcceptedRecordEntry, AtomicVerifiedBackend, InMemoryVerifiedBackend, PortableVerifiedSnapshot,
    PutVerifiedOutcome, QuarantineRecord, StoredRecordKind, ValidatedStore,
    ValidatedStoreRestorePort, VerifiedStoreError, VerifiedStoreSnapshotPort,
};
pub use use_evidence::{
    AssessedExerciseEvidence, DerivationEvidencePayload, DerivationInput, ExerciseAuthority,
    ExerciseEvidence, ExerciseEvidencePath, ExerciseRecordOutcome, UseEvidenceError,
    UseEvidencePayload, UseMode, ValidatedDerivationEvidenceEvent, ValidatedUseEvidenceEvent,
    DERIVATION_EVIDENCE_EVENT_TYPE, DERIVATION_EVIDENCE_KIND, USE_EVIDENCE_EVENT_TYPE,
    USE_EVIDENCE_KIND,
};
pub use vault::{
    PortableVaultRecord, PortableVaultSnapshot, PortableVaultSnapshotPort, PrivateVault,
    ValidatedVaultRestorePort, VaultKey, VaultQuarantineRecord, VaultSourceSnapshotPort,
    VaultSourceSnapshotRecord, VaultStagingId,
};

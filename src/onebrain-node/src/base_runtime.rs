//! Offline-first, product-neutral OneBrain Base runtime facade.
//!
//! Service handles contain only a `Weak` aggregate reference plus immutable
//! generation/principal fences. Host-only constructors and authority adapters
//! remain on `BaseRuntime`; product consumers never receive stores, paths,
//! archive readers/writers, keys, or subsystem runtimes.

use std::collections::{BTreeMap, BTreeSet, VecDeque};
use std::sync::{Arc, Mutex, RwLock, Weak};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use onebrain_archive::ArchiveCredentialKind;
use onebrain_base_contract::{
    ArchiveCapabilityHandleV1, ArchiveCredentialKindV1, ArchiveRestorePolicyV1,
    ArchiveSecretHandleV1, ArchiveSinkHandleV1, ArchiveSourceHandleV1, BaseCapabilityRequirements,
    BaseCapabilitySet, BaseCommandV1, BaseCompatibilityPolicy, BaseCompatibilityTuple,
    BaseErrorCodeV1, BaseIdempotencyKey, BaseManagementRequestV1, BaseNegotiationOutcome,
    BaseOperationId, BaseOperationKindV1, BaseOperationReservationId, BasePollEventsRequestV1,
    BasePrepareRequestV1, BaseQualificationState, BaseQueryRequestV1, BaseRequestV1,
    BaseSubscriptionId, BaseSubscriptionRequestV1, BaseVersionStatus, CompatibilityDigestV1,
    CompleteSignerReprovisionV1, MigrationVectorBindingV1, NegotiatedVersions, ProfileVersion,
    ResourceBudgetV1, SignerProvisionHandleV1, SignerPublicIdV1, StorageSchemaVersion,
    TargetTriple, TopicKindV1, TypedPayloadV1, BASE_V1_RELEASE_VERSION, COMPILED_BASE_COMMIT,
    COMPILED_TARGET_TRIPLE, COMPILED_TOOLCHAIN, MAX_BASE_ARCHIVE_DATASET_BYTES,
};
use tokio::sync::Notify;

use crate::activation_journal::ActivationOperationContext;
use crate::archive::{BaseArchiveService, DatasetRestoreReceipt};
use crate::archive_capabilities::{
    ArchiveCapabilityRegistry, ArchiveOperationReservationId, ArchiveProcessGeneration,
    ArchiveSecretHandle, ReadableArchiveSinkHandle, SealedArchiveSourceHandle,
    WritableArchiveSinkHandle, WritableArchiveSourceHandle, DEFAULT_ARCHIVE_SPOOL_BYTES,
};
use crate::base_operation_store::{
    BaseAuthorityKindV1, BaseAuthorityStateV1, BaseOperationReceiptV1, BaseOperationStore,
    BaseOperationStoreError, OsProcessGenerationIdSource, PreparedBaseIntentV1,
    ProcessGenerationId, ProcessGenerationIdSource, ProcessGenerationLease, ReconciliationResultV1,
};
use crate::dataset_generation::{
    DatasetBaseRuntimeClaim, DatasetGenerationStore, RestoreOperationBinding,
};
use crate::dataset_path::{DatasetGenerationId, DatasetPathResolver};
use crate::error::NodeError;
use crate::identity_recovery::SignerReprovisionRequirement;
use crate::signer_ports::{SignerPossessionProof, SignerProviderRegistry};

const MAX_IN_FLIGHT: u32 = 64;
const MAX_SUBSCRIPTIONS: usize = 64;
const MAX_EVENTS: usize = 1024;
const MAX_EVENT_ITEMS: u32 = 256;
const MAX_EVENT_PAYLOAD_BYTES: usize = 64 * 1024;
const MAX_SIGNER_PROVISIONS: usize = 16;
const MANAGEMENT_SCOPE_DOMAIN: &str = "onebrain:base:management-scopes:1";

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(u8)]
pub enum BaseRuntimeLifecycle {
    Open = 1,
    Draining = 2,
    Closed = 3,
}

pub type BaseServiceErrorCode = BaseErrorCodeV1;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct BaseServiceError {
    pub code: BaseErrorCodeV1,
    pub reason: &'static str,
    pub retryable: bool,
    pub reconcile_before_retry: bool,
}

impl BaseServiceError {
    /// Construct a typed projection error without discarding the frozen retry
    /// and reconciliation semantics owned by the Base contract.
    pub fn new(code: BaseErrorCodeV1, reason: &'static str) -> Self {
        Self {
            code,
            reason,
            retryable: code.retryable(),
            reconcile_before_retry: code.reconcile_before_retry(),
        }
    }

    fn stale() -> Self {
        Self::new(BaseErrorCodeV1::Conflict, "generation_mismatch")
    }
}

impl std::fmt::Display for BaseServiceError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str(self.reason)
    }
}

impl std::error::Error for BaseServiceError {}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct BaseNegotiationRequest {
    pub peer: onebrain_base_contract::BaseCompatibilityTuple,
    pub peer_capabilities: BaseCapabilityRequirements,
    pub verified_migration: Option<MigrationVectorBindingV1>,
}

pub type BaseNegotiationResponse = BaseNegotiationOutcome;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct BaseStatusV1 {
    pub lifecycle: BaseRuntimeLifecycle,
    pub process_generation: ProcessGenerationId,
    pub dataset_generation: DatasetGenerationId,
    pub version: BaseVersionStatus,
    pub in_flight: u32,
    pub open_subscriptions: u32,
    pub active_management_handles: u32,
    pub network_compiled: bool,
    pub network_enabled: bool,
    pub local_usable: bool,
    pub limitations: Vec<&'static str>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct BaseEventV1 {
    pub cursor: u64,
    pub topic: TopicKindV1,
    pub operation_id: Option<BaseOperationId>,
    pub payload: Vec<u8>,
}

pub struct BaseEventBatchV1 {
    pub subscription_id: BaseSubscriptionId,
    pub events: Vec<BaseEventV1>,
    pub next_cursor: u64,
    pub earliest_available_cursor: u64,
    pub resync_required: bool,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct BaseDrainReceiptV1 {
    pub process_generation: ProcessGenerationId,
    pub dataset_generation: DatasetGenerationId,
    pub lifecycle: BaseRuntimeLifecycle,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct BaseCloseReceiptV1 {
    pub process_generation: ProcessGenerationId,
    pub dataset_generation: DatasetGenerationId,
    pub lifecycle: BaseRuntimeLifecycle,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct BaseManagementCloseReceiptV1 {
    pub management_handle: [u8; 32],
    pub revoked_capabilities: u32,
}

pub enum BaseResponseV1 {
    Status(Box<BaseStatusV1>),
    Query {
        payload: TypedPayloadV1,
        continuation: Option<onebrain_base_contract::BaseOpaqueContinuation>,
    },
    Reserved(BaseOperationReservationId),
    Prepared(PreparedBaseIntentV1),
    Receipt(BaseOperationReceiptV1),
    Reconciled(ReconciliationResultV1),
    Subscription(BaseSubscriptionId),
    Events(BaseEventBatchV1),
    SubscriptionClosed,
    Drain(BaseDrainReceiptV1),
    Close(BaseCloseReceiptV1),
}

pub enum BaseManagementResponseV1 {
    ArchiveSource(ArchiveSourceHandleV1),
    ArchiveSink(ArchiveSinkHandleV1),
    ArchiveSecret(ArchiveSecretHandleV1),
    ArchiveCapability(ArchiveCapabilityHandleV1),
    ArchiveChunk {
        offset: u64,
        bytes: Vec<u8>,
        eof: bool,
    },
    CapabilityClosed,
    SignerReprovisioned,
    Close(BaseManagementCloseReceiptV1),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum BaseManagementScope {
    ArchiveSource,
    ArchiveSink,
    ArchiveSecret,
    SignerReprovision,
}

pub trait BaseHostAuthorizer: Send + Sync {
    fn authenticate(&self, principal: [u8; 32], proof: &[u8]) -> bool;
}

#[derive(Default)]
pub struct DenyAllBaseHostAuthorizer;

impl BaseHostAuthorizer for DenyAllBaseHostAuthorizer {
    fn authenticate(&self, _principal: [u8; 32], _proof: &[u8]) -> bool {
        false
    }
}

pub trait BaseLocalOperationAdapter: Send + Sync {
    fn query(
        &self,
        request: BaseQueryRequestV1,
    ) -> Result<
        (
            TypedPayloadV1,
            Option<onebrain_base_contract::BaseOpaqueContinuation>,
        ),
        BaseServiceError,
    >;

    fn confirm_local(
        &self,
        command: onebrain_base_contract::BaseLocalCommandV1,
    ) -> Result<Vec<u8>, BaseServiceError>;
}

#[derive(Default)]
pub struct UnavailableBaseLocalOperationAdapter;

impl BaseLocalOperationAdapter for UnavailableBaseLocalOperationAdapter {
    fn query(
        &self,
        _request: BaseQueryRequestV1,
    ) -> Result<
        (
            TypedPayloadV1,
            Option<onebrain_base_contract::BaseOpaqueContinuation>,
        ),
        BaseServiceError,
    > {
        Err(BaseServiceError::new(
            BaseErrorCodeV1::DependencyUnavailable,
            "local_query_adapter_unavailable",
        ))
    }

    fn confirm_local(
        &self,
        _command: onebrain_base_contract::BaseLocalCommandV1,
    ) -> Result<Vec<u8>, BaseServiceError> {
        Err(BaseServiceError::new(
            BaseErrorCodeV1::DependencyUnavailable,
            "local_command_adapter_unavailable",
        ))
    }
}

pub trait BaseArchiveServiceFactory: Send + Sync {
    fn build(
        &self,
        capabilities: ArchiveCapabilityRegistry,
        dataset_generations: Arc<DatasetGenerationStore>,
        operation_store: Arc<BaseOperationStore>,
    ) -> Result<BaseArchiveService, NodeError>;
}

impl<F> BaseArchiveServiceFactory for F
where
    F: Fn(
            ArchiveCapabilityRegistry,
            Arc<DatasetGenerationStore>,
            Arc<BaseOperationStore>,
        ) -> Result<BaseArchiveService, NodeError>
        + Send
        + Sync,
{
    fn build(
        &self,
        capabilities: ArchiveCapabilityRegistry,
        dataset_generations: Arc<DatasetGenerationStore>,
        operation_store: Arc<BaseOperationStore>,
    ) -> Result<BaseArchiveService, NodeError> {
        self(capabilities, dataset_generations, operation_store)
    }
}

pub struct BaseRuntimeConfig {
    pub ku: Option<crate::ku_product::KuRuntimeConfig>,
    pub compatibility_policy: BaseCompatibilityPolicy,
    pub version_status: BaseVersionStatus,
    pub capabilities: BaseCapabilityRequirements,
    pub host_authorizer: Arc<dyn BaseHostAuthorizer>,
    pub local_adapter: Arc<dyn BaseLocalOperationAdapter>,
    pub archive_factory: Option<Arc<dyn BaseArchiveServiceFactory>>,
    pub process_generation_source: Arc<dyn ProcessGenerationIdSource>,
    pub max_in_flight: u32,
    pub network_enabled: bool,
}

impl BaseRuntimeConfig {
    pub fn new(
        compatibility_policy: BaseCompatibilityPolicy,
        version_status: BaseVersionStatus,
        capabilities: BaseCapabilityRequirements,
    ) -> Self {
        Self {
            ku: None,
            compatibility_policy,
            version_status,
            capabilities,
            host_authorizer: Arc::new(DenyAllBaseHostAuthorizer),
            local_adapter: Arc::new(UnavailableBaseLocalOperationAdapter),
            archive_factory: None,
            process_generation_source: Arc::new(OsProcessGenerationIdSource),
            max_in_flight: MAX_IN_FLIGHT,
            network_enabled: false,
        }
    }
}

/// Compose the truthful compiled Base tuple without opening any store. Product
/// hosts may replace the deny-all authorizer and unavailable local adapter
/// before installing the returned config exactly once on a node.
pub fn compiled_base_runtime_config() -> BaseRuntimeConfig {
    let registry = ku_core::foundation::base_v1_profile_registry();
    let mut features = blake3::Hasher::new();
    features.update(b"onebrain:base-v1:compiled-features:1\0");
    features.update(b"base-v1");
    features.update(b";ku-product-v1");
    if cfg!(feature = "legacy-read-compat") {
        features.update(b";legacy-read-compat");
    }
    if cfg!(feature = "vnext-network-runtime") {
        features.update(b";vnext-network-runtime");
    }
    let tuple = BaseCompatibilityTuple {
        base_version: BASE_V1_RELEASE_VERSION,
        base_commit: COMPILED_BASE_COMMIT,
        canonical_schema_digest: CompatibilityDigestV1(registry.canonical_schema_digest),
        domain_registry_digest: CompatibilityDigestV1(registry.domain_registry_digest),
        resource_registry_digest: CompatibilityDigestV1(registry.resource_registry_digest),
        storage_schema: StorageSchemaVersion(1),
        archive_profile: ProfileVersion { major: 2, minor: 0 },
        migration_profile: ProfileVersion { major: 1, minor: 0 },
        registry_profile: ProfileVersion { major: 1, minor: 0 },
        registry_profile_digest: CompatibilityDigestV1([0x24; 32]),
        wire_session: ProfileVersion { major: 1, minor: 0 },
        product_api: ProfileVersion { major: 1, minor: 1 },
        c_abi: ProfileVersion { major: 1, minor: 0 },
        feature_set_digest: CompatibilityDigestV1(*features.finalize().as_bytes()),
        target_triple: TargetTriple::try_from_string(COMPILED_TARGET_TRIPLE.to_owned())
            .expect("build.rs emits a bounded target triple"),
        toolchain: COMPILED_TOOLCHAIN,
    };
    let archive_restore = ArchiveRestorePolicyV1 {
        canonical_schema_digest: tuple.canonical_schema_digest,
        domain_registry_digest: tuple.domain_registry_digest,
        resource_registry_digest: tuple.resource_registry_digest,
        storage_schema: tuple.storage_schema,
        archive_profile: tuple.archive_profile,
        migration_profile: tuple.migration_profile,
        max_dataset_bytes: MAX_BASE_ARCHIVE_DATASET_BYTES,
    };
    let empty =
        || BaseCapabilitySet::try_from_discriminators(Vec::new()).expect("empty bounded set");
    BaseRuntimeConfig::new(
        BaseCompatibilityPolicy {
            current: tuple.clone(),
            minimum_additive: NegotiatedVersions {
                base_minor: 0,
                wire_session_minor: 0,
                product_api_minor: 0,
                c_abi_minor: 0,
            },
            archive_restore,
        },
        tuple.unqualified_status(),
        BaseCapabilityRequirements {
            supported: empty(),
            required: empty(),
        },
    )
}

pub struct BaseManagementGrant {
    id: [u8; 32],
    principal: [u8; 32],
    process_generation: ProcessGenerationId,
    dataset_generation: DatasetGenerationId,
}

pub struct BaseRuntime {
    core: Option<Arc<BaseServiceCore>>,
}

impl BaseRuntime {
    pub fn open(
        dataset_generations: Arc<DatasetGenerationStore>,
        config: BaseRuntimeConfig,
    ) -> Result<Self, BaseServiceError> {
        validate_runtime_config(&config)?;
        let runtime_claim = dataset_generations
            .claim_base_runtime()
            .map_err(|_| conflict("base_runtime_already_owned"))?;
        let process_lease = ProcessGenerationLease::allocate(
            dataset_generations.control_path(),
            config.process_generation_source.as_ref(),
        )
        .map_err(store_error)?;
        let process_generation = process_lease.id();
        let operation_store = Arc::new(
            BaseOperationStore::open(dataset_generations.as_ref(), process_generation)
                .map_err(store_error)?,
        );
        let archive_capabilities = ArchiveCapabilityRegistry::with_process_generation(
            ArchiveProcessGeneration::from_bytes(process_generation.0),
            DEFAULT_ARCHIVE_SPOOL_BYTES,
        )
        .map_err(node_error)?;
        let archive_service = config
            .archive_factory
            .as_ref()
            .map(|factory| {
                factory
                    .build(
                        archive_capabilities.clone(),
                        dataset_generations.clone(),
                        operation_store.clone(),
                    )
                    .map(Arc::new)
                    .map_err(node_error)
            })
            .transpose()?;
        let ku = config
            .ku
            .map(|config| {
                crate::ku_product::KuStore::open(
                    dataset_generations.as_ref(),
                    config,
                    process_generation.0,
                )
            })
            .transpose()?;
        let core = Arc::new(BaseServiceCore {
            ku,
            lifecycle: Mutex::new(LifecycleState {
                state: BaseRuntimeLifecycle::Open,
                in_flight: 0,
            }),
            drained: Notify::new(),
            process_lease,
            _runtime_claim: runtime_claim,
            process_generation,
            dataset_generations,
            operation_store: RwLock::new(operation_store),
            compatibility_policy: config.compatibility_policy,
            version_status: config.version_status,
            capabilities: config.capabilities,
            host_authorizer: config.host_authorizer,
            local_adapter: config.local_adapter,
            archive_capabilities,
            archive_service,
            authority: Mutex::new(AuthorityState::default()),
            prepared: Mutex::new(BTreeMap::new()),
            subscriptions: Mutex::new(BTreeMap::new()),
            events: Mutex::new(EventState::default()),
            max_in_flight: config.max_in_flight,
            network_enabled: config.network_enabled,
        });
        core.recover_activation_receipt()?;
        Ok(Self { core: Some(core) })
    }

    pub fn services(&self) -> Result<BaseServices, BaseServiceError> {
        self.services_for_principal([0; 32])
    }

    /// Authenticate a local KU session at its node-owned Base host boundary.
    pub fn ku_services(
        &self,
        principal: [u8; 32],
        proof: &[u8],
    ) -> Result<crate::ku_product::KuServices, BaseServiceError> {
        let core = self.core.as_ref().ok_or_else(BaseServiceError::stale)?;
        if proof.is_empty() || !core.host_authorizer.authenticate(principal, proof) {
            return Err(crate::ku_product::invalid());
        }
        if core.ku.is_none() {
            return Err(crate::ku_product::unavailable());
        }
        Ok(crate::ku_product::KuServices {
            base: self.services_for_principal(principal)?,
        })
    }

    pub fn services_for_principal(
        &self,
        principal: [u8; 32],
    ) -> Result<BaseServices, BaseServiceError> {
        let core = self.core.as_ref().ok_or_else(BaseServiceError::stale)?;
        Ok(BaseServices {
            core: Arc::downgrade(core),
            process_generation: core.process_generation,
            dataset_generation: core.dataset_generations.current_generation(),
            principal,
            negotiated_migration: Arc::new(Mutex::new(None)),
        })
    }

    pub fn issue_management_grant(
        &self,
        principal: [u8; 32],
        authentication_proof: &[u8],
        scopes: impl IntoIterator<Item = BaseManagementScope>,
        ttl: Duration,
    ) -> Result<BaseManagementGrant, BaseServiceError> {
        let core = self.core.as_ref().ok_or_else(BaseServiceError::stale)?;
        core.ensure_open_generation(
            core.process_generation,
            core.dataset_generations.current_generation(),
        )?;
        if authentication_proof.is_empty()
            || !core
                .host_authorizer
                .authenticate(principal, authentication_proof)
        {
            return Err(BaseServiceError::new(
                BaseErrorCodeV1::InvalidRequest,
                "principal_authentication_failed",
            ));
        }
        let scopes = scopes.into_iter().collect::<BTreeSet<_>>();
        if scopes.is_empty() || ttl.is_zero() {
            return Err(BaseServiceError::new(
                BaseErrorCodeV1::InvalidRequest,
                "management_grant_is_empty",
            ));
        }
        let expires_at = now_seconds().checked_add(ttl.as_secs()).ok_or_else(|| {
            BaseServiceError::new(BaseErrorCodeV1::Expired, "grant_expiry_overflow")
        })?;
        let mut authority = core.lock_authority()?;
        let id = unique_id(&authority.grants)?;
        let record = GrantRecord {
            principal,
            scopes,
            process_generation: core.process_generation,
            dataset_generation: core.dataset_generations.current_generation(),
            expires_at,
            revocation_epoch: authority.revocation_epoch,
        };
        let store = core.current_store()?;
        store
            .register_authority(
                id,
                BaseAuthorityKindV1::ManagementGrant,
                principal,
                None,
                None,
            )
            .map_err(store_error)?;
        store
            .transition_authority(
                id,
                BaseAuthorityKindV1::ManagementGrant,
                principal,
                BaseAuthorityStateV1::Active,
            )
            .map_err(store_error)?;
        authority.grants.insert(id, record);
        Ok(BaseManagementGrant {
            id,
            principal,
            process_generation: core.process_generation,
            dataset_generation: core.dataset_generations.current_generation(),
        })
    }

    pub fn register_signer_provision(
        &self,
        principal: [u8; 32],
        authentication_proof: &[u8],
        requirement: SignerReprovisionRequirement,
        proof: SignerPossessionProof,
        registry: Arc<dyn SignerProviderRegistry>,
    ) -> Result<SignerProvisionHandleV1, BaseServiceError> {
        let core = self.core.as_ref().ok_or_else(BaseServiceError::stale)?;
        core.ensure_open_generation(
            core.process_generation,
            core.dataset_generations.current_generation(),
        )?;
        if authentication_proof.is_empty()
            || !core
                .host_authorizer
                .authenticate(principal, authentication_proof)
        {
            return Err(BaseServiceError::new(
                BaseErrorCodeV1::InvalidRequest,
                "principal_authentication_failed",
            ));
        }
        if requirement.expected.domain() != proof.challenge.domain
            || requirement.provider_id != proof.provider_id
            || proof.challenge.expected_identity_digest != requirement.expected.digest()
            || proof.challenge.dataset_generation != core.dataset_generations.current_generation()
        {
            return Err(conflict("signer_provision_binding_mismatch"));
        }
        let mut authority = core.lock_authority()?;
        if authority.signer_provisions.len() >= MAX_SIGNER_PROVISIONS {
            return Err(resource_exhausted("signer_provision_budget_exhausted"));
        }
        let id = unique_id(&authority.signer_provisions)?;
        let store = core.current_store()?;
        store
            .register_authority(
                id,
                BaseAuthorityKindV1::SignerProvision,
                principal,
                None,
                Some(requirement.expected.domain().code() as u16),
            )
            .map_err(store_error)?;
        store
            .transition_authority(
                id,
                BaseAuthorityKindV1::SignerProvision,
                principal,
                BaseAuthorityStateV1::Active,
            )
            .map_err(store_error)?;
        authority.signer_provisions.insert(
            id,
            SignerProvisionRecord {
                principal,
                process_generation: core.process_generation,
                dataset_generation: core.dataset_generations.current_generation(),
                requirement,
                proof,
                registry,
            },
        );
        Ok(SignerProvisionHandleV1::from_opaque_bytes(id))
    }

    pub async fn drain(&self) -> Result<BaseDrainReceiptV1, BaseServiceError> {
        self.core
            .as_ref()
            .ok_or_else(BaseServiceError::stale)?
            .drain()
            .await
    }

    pub async fn close(&mut self) -> Result<BaseCloseReceiptV1, BaseServiceError> {
        let core = self.core.as_ref().ok_or_else(BaseServiceError::stale)?;
        let receipt = core.close().await?;
        self.core.take();
        Ok(receipt)
    }
}

impl Drop for BaseRuntime {
    fn drop(&mut self) {
        if let Some(core) = self.core.take() {
            core.close_best_effort();
        }
    }
}

#[derive(Clone)]
pub struct BaseServices {
    core: Weak<BaseServiceCore>,
    process_generation: ProcessGenerationId,
    dataset_generation: DatasetGenerationId,
    principal: [u8; 32],
    negotiated_migration: Arc<Mutex<Option<MigrationVectorBindingV1>>>,
}

impl BaseServices {
    pub(crate) async fn ku_editor(
        &self,
        request: crate::ku_manual::ManualEditorRequest,
        budget: ResourceBudgetV1,
    ) -> Result<crate::ku_manual::ManualEditorResponse, BaseServiceError> {
        let lease = self.lease(Admission::NewWork)?;
        validate_budget(&budget)?;
        if budget.max_bytes < 16384 || budget.max_items == 0 || budget.max_work_units < 1024 {
            return Err(crate::ku_product::resource());
        }
        let ku = lease
            .core
            .ku
            .as_ref()
            .ok_or_else(crate::ku_product::unavailable)?;
        ku.check_dataset(self.dataset_generation)?;
        if let crate::ku_manual::ManualEditorRequest::Draft(input) = &request {
            let receipt = lease
                .core
                .current_store()?
                .reconcile(BaseOperationId(input.operation_id.0), self.principal)
                .map_err(store_error)?
                .receipt;
            if receipt.state != crate::base_operation_store::BaseOperationStateV1::Reserved {
                return Err(crate::ku_product::conflict());
            }
        }
        ku.inputs.editor(self.principal, request, &budget)
    }
    pub(crate) async fn ku_reserve(
        &self,
    ) -> Result<onebrain_base_contract::ku::OperationId, BaseServiceError> {
        let lease = self.lease(Admission::NewWork)?;
        let ku = lease
            .core
            .ku
            .as_ref()
            .ok_or_else(crate::ku_product::unavailable)?;
        ku.check_dataset(self.dataset_generation)?;
        let op = lease
            .core
            .current_store()?
            .reserve_operation(BaseOperationKindV1::ExistingLocalCommand, self.principal)
            .map_err(store_error)?;
        Ok(onebrain_base_contract::ku::OperationId(op.0))
    }

    pub(crate) async fn ku_invoke(
        &self,
        request: onebrain_base_contract::ku::KuRequestV1,
        budget: ResourceBudgetV1,
    ) -> Result<onebrain_base_contract::ku::KuResponseV1, BaseServiceError> {
        use crate::base_operation_store::BaseOperationStateV1 as State;
        use onebrain_base_contract::ku::*;
        use onebrain_base_contract::ku_payload::KuPayload;
        let admission = match request {
            KuRequestV1::Cancel(_)
            | KuRequestV1::Reconcile(_)
            | KuRequestV1::Status(_)
            | KuRequestV1::Preview(_) => Admission::Recovery,
            _ => Admission::NewWork,
        };
        let lease = self.lease(admission)?;
        validate_budget(&budget)?;
        if budget.max_bytes == 0 || budget.max_items == 0 || budget.max_work_units == 0 {
            return Err(crate::ku_product::resource());
        }
        if request
            .payload_bytes()
            .map_err(|_| crate::ku_product::resource())?
            .len() as u64
            > budget.max_bytes
        {
            return Err(crate::ku_product::resource());
        }
        let ku = lease
            .core
            .ku
            .as_ref()
            .ok_or_else(crate::ku_product::unavailable)?;
        ku.check_dataset(self.dataset_generation)?;
        let store = lease.core.current_store()?;
        let principal = self.principal;
        let admit_receipt = |receipt: &KuReceiptV1| -> Result<(), BaseServiceError> {
            if receipt
                .encode()
                .map_err(|_| crate::ku_product::resource())?
                .len() as u64
                > budget.max_bytes
            {
                return Err(crate::ku_product::resource());
            }
            Ok(())
        };
        let complete_ku =
            |id: OperationId, receipt: &KuReceiptV1| -> Result<(), BaseServiceError> {
                let result = receipt
                    .encode()
                    .map_err(|_| crate::ku_product::unknown())
                    .and_then(|bytes| {
                        store
                            .complete(BaseOperationId(id.0), bytes)
                            .map(|_| ())
                            .map_err(|_| crate::ku_product::unknown())
                    });
                if result.is_err() {
                    let _ = store.mark_unknown(BaseOperationId(id.0));
                }
                result
            };
        let map_receipt = |r: &BaseOperationReceiptV1| -> Result<KuReceiptV1, BaseServiceError> {
            if r.state == State::Committed && !r.result.is_empty() {
                return KuReceiptV1::decode(&r.result).map_err(|_| crate::ku_product::corrupt());
            }
            Ok(KuReceiptV1 {
                operation_id: OperationId(r.operation_id.0),
                state: match r.state {
                    State::Reserved => BaseState::Reserved,
                    State::Prepared => BaseState::Prepared,
                    State::Confirming => BaseState::Confirming,
                    State::Committed => BaseState::Committed,
                    State::Canceled => BaseState::Canceled,
                    State::Failed => BaseState::Failed,
                    State::UnknownOutcome => BaseState::UnknownOutcome,
                },
                object_cids: vec![],
                limitations: vec![],
                published: false,
                authorizes_reward: false,
            })
        };
        let preparation_store = &store;
        let preparation_budget = &budget;
        let prepare = |preparation: KuPrepareV1,
                       revision: Option<(ObjectCID, RevisionFrontier)>| async move {
            let store = preparation_store;
            let budget = preparation_budget;
            let id = preparation.operation_id;
            if store
                .operation_kind(BaseOperationId(id.0), principal)
                .map_err(store_error)?
                != BaseOperationKindV1::ExistingLocalCommand
            {
                return Err(crate::ku_product::conflict());
            }
            let state = store
                .reconcile(BaseOperationId(id.0), principal)
                .map_err(store_error)?
                .receipt
                .state;
            if !matches!(state, State::Reserved | State::Prepared) {
                return Err(crate::ku_product::conflict());
            }
            if state == State::Prepared && !ku.is_prepared(principal, id)? {
                return Err(crate::ku_product::corrupt());
            }
            let preview = ku.prepare(principal, preparation, revision, budget).await?;
            let mut exact = crate::ku_product::KU_PREPARED_MARKER.to_vec();
            exact.extend_from_slice(&id.0);
            store
                .prepare(
                    BaseOperationReservationId(id.0),
                    BaseOperationKindV1::ExistingLocalCommand,
                    exact,
                    None,
                    principal,
                )
                .map_err(store_error)?;
            Ok(preview)
        };
        let response = match request {
            KuRequestV1::Prepare(r) => KuResponseV1::Prepare(prepare(r, None).await?),
            KuRequestV1::Revise(r) => KuResponseV1::Revise(
                prepare(
                    r.preparation,
                    Some((r.predecessor_object_cid, r.expected_revision_frontier)),
                )
                .await?,
            ),
            KuRequestV1::Preview(r) => {
                let state = store
                    .reconcile(BaseOperationId(r.operation_id.0), principal)
                    .map_err(store_error)?
                    .receipt
                    .state;
                if state == State::Canceled {
                    return Err(crate::ku_product::conflict());
                }
                KuResponseV1::Preview(ku.preview(principal, r.operation_id)?)
            }
            KuRequestV1::Save(r) => {
                r.validate().map_err(|_| crate::ku_product::invalid())?;
                ku.preflight_save(principal, &r)?;
                admit_receipt(&ku.confirmation_receipt(principal, r.operation_id)?)?;
                if let Some(receipt) = store
                    .begin_confirm(
                        BaseOperationId(r.operation_id.0),
                        BaseIdempotencyKey(r.idempotency_key.0),
                        principal,
                    )
                    .map_err(store_error)?
                {
                    KuResponseV1::Save(map_receipt(&receipt)?)
                } else {
                    match ku.save(principal, &r) {
                        Ok(result) => {
                            complete_ku(r.operation_id, &result)?;
                            KuResponseV1::Save(result)
                        }
                        Err(_) => {
                            let _ = store.mark_unknown(BaseOperationId(r.operation_id.0));
                            return Err(crate::ku_product::unknown());
                        }
                    }
                }
            }
            KuRequestV1::Get(r) => KuResponseV1::Get(ku.get(principal, r.object_cid)?),
            KuRequestV1::List(r) => {
                KuResponseV1::List(ku.page(principal, None, r.limit, r.continuation, &budget)?)
            }
            KuRequestV1::Search(r) => KuResponseV1::Search(ku.page(
                principal,
                Some(r.query),
                r.limit,
                r.continuation,
                &budget,
            )?),
            KuRequestV1::Status(r) => {
                let mut result = ku.status();
                if lease
                    .core
                    .lifecycle
                    .lock()
                    .map_err(|_| internal_error())?
                    .state
                    != BaseRuntimeLifecycle::Open
                {
                    result.lifecycle = Lifecycle::Degraded;
                    result.limitations.push("base_runtime_draining".into());
                }
                if let Some(id) = r.operation_id {
                    let base = store
                        .reconcile(BaseOperationId(id.0), principal)
                        .map_err(store_error)?;
                    result.receipt = Some(map_receipt(&base.receipt)?);
                }
                KuResponseV1::Status(result)
            }
            KuRequestV1::Cancel(r) => {
                if store
                    .operation_kind(BaseOperationId(r.operation_id.0), principal)
                    .map_err(store_error)?
                    != BaseOperationKindV1::ExistingLocalCommand
                {
                    return Err(crate::ku_product::conflict());
                }
                let mut projected = map_receipt(
                    &store
                        .reconcile(BaseOperationId(r.operation_id.0), principal)
                        .map_err(store_error)?
                        .receipt,
                )?;
                projected.state = BaseState::Canceled;
                admit_receipt(&projected)?;
                // The eligibility check and state change share the Base journal lock.
                let receipt = store
                    .cancel_before_confirmation(BaseOperationId(r.operation_id.0), principal)
                    .map_err(store_error)?;
                ku.cancel(principal, r.operation_id)
                    .map_err(|_| crate::ku_product::unknown())?;
                KuResponseV1::Cancel(map_receipt(&receipt)?)
            }
            KuRequestV1::Reconcile(r) => {
                let base = store
                    .reconcile(BaseOperationId(r.operation_id.0), principal)
                    .map_err(store_error)?
                    .receipt;
                if base.state == State::UnknownOutcome {
                    if base.idempotency_key.is_some() {
                        admit_receipt(&ku.confirmation_receipt(principal, r.operation_id)?)?;
                    }
                    if let Some(result) = ku.saved_receipt(principal, r.operation_id)? {
                        admit_receipt(&result)?;
                        complete_ku(r.operation_id, &result)?;
                        KuResponseV1::Reconcile(result)
                    } else if let Some(key) = base.idempotency_key {
                        // Repair the finite saved command from encrypted staging; never invoke an encoder.
                        let result = ku
                            .recovery_save(principal, r.operation_id, key.0)
                            .map_err(|_| crate::ku_product::unknown())?;
                        complete_ku(r.operation_id, &result)?;
                        KuResponseV1::Reconcile(result)
                    } else {
                        // No confirmation key means save never began. Close the
                        // interrupted preparation without invoking its encoder.
                        let mut projected = map_receipt(&base)?;
                        projected.state = BaseState::Failed;
                        admit_receipt(&projected)?;
                        let failed = store
                            .fail(BaseOperationId(r.operation_id.0), BaseErrorCodeV1::Conflict)
                            .map_err(store_error)?;
                        KuResponseV1::Reconcile(map_receipt(&failed)?)
                    }
                } else {
                    KuResponseV1::Reconcile(map_receipt(&base)?)
                }
            }
            KuRequestV1::Export(r) => {
                if r.mode == ExportMode::CanonicalPublicExchange {
                    KuResponseV1::Export(ku.public_export(principal, r)?)
                } else {
                    for cid in &r.object_cids {
                        ku.get(principal, *cid)?;
                    }
                    if lease.core.archive_service.is_none() {
                        return Err(crate::ku_product::unavailable());
                    }
                    let mut projected = KuExportViewV1 {
                        mode: r.mode,
                        object_cids: r.object_cids,
                        limitations: vec!["base_archive_dataset_scope".into()],
                        requires_base_management: true,
                        public_records: None,
                        archive_operation_id: Some(OperationId([0; 32])),
                    };
                    if projected
                        .encode()
                        .map_err(|_| crate::ku_product::resource())?
                        .len() as u64
                        > budget.max_bytes
                    {
                        return Err(crate::ku_product::resource());
                    }
                    let BaseResponseV1::Reserved(operation) = self
                        .invoke(BaseRequestV1::ReserveOperation(
                            BaseOperationKindV1::CreateArchive,
                        ))
                        .await?
                    else {
                        return Err(internal_error());
                    };
                    projected.archive_operation_id = Some(OperationId(operation.0));
                    KuResponseV1::Export(projected)
                }
            }
        };
        if response
            .payload_bytes()
            .map_err(|_| crate::ku_product::resource())?
            .len() as u64
            > budget.max_bytes
        {
            return Err(crate::ku_product::resource());
        }
        Ok(response)
    }

    pub fn negotiate(
        &self,
        request: BaseNegotiationRequest,
    ) -> Result<BaseNegotiationResponse, BaseServiceError> {
        let lease = self.lease(Admission::Read)?;
        let outcome = lease.core.compatibility_policy.negotiate(
            &request.peer,
            &lease.core.capabilities,
            &request.peer_capabilities,
            request.verified_migration,
        );
        let mut session = self
            .negotiated_migration
            .lock()
            .map_err(|_| internal_error())?;
        *session = match &outcome {
            BaseNegotiationOutcome::MigrationRequired(required) => Some(required.vector.clone()),
            _ => None,
        };
        Ok(outcome)
    }

    pub fn snapshot(&self) -> Result<BaseStatusV1, BaseServiceError> {
        let lease = self.lease(Admission::Read)?;
        lease.core.status()
    }

    pub async fn invoke(&self, request: BaseRequestV1) -> Result<BaseResponseV1, BaseServiceError> {
        match request {
            BaseRequestV1::Status => Ok(BaseResponseV1::Status(Box::new(self.snapshot()?))),
            BaseRequestV1::Query(request) => {
                let lease = self.lease(Admission::NewWork)?;
                validate_query_budget(&request)?;
                let response_budget = request.budget.max_bytes;
                let (payload, continuation) = lease.core.local_adapter.query(request)?;
                if payload.as_bytes().len() as u64 > response_budget {
                    return Err(resource_exhausted("query_response_budget_exhausted"));
                }
                Ok(BaseResponseV1::Query {
                    payload,
                    continuation,
                })
            }
            BaseRequestV1::ReserveOperation(kind) => {
                let lease = self.lease(Admission::NewWork)?;
                let store = lease.core.current_store()?;
                let reservation = store
                    .reserve_operation(kind, self.principal)
                    .map_err(store_error)?;
                if matches!(
                    kind,
                    BaseOperationKindV1::CreateArchive | BaseOperationKindV1::RestoreArchive
                ) {
                    lease
                        .core
                        .archive_capabilities
                        .register_operation(ArchiveOperationReservationId::from_bytes(
                            reservation.0,
                        ))
                        .map_err(node_error)?;
                    lease
                        .core
                        .lock_authority()?
                        .archive_reservations
                        .insert(reservation.0, self.principal);
                }
                lease
                    .core
                    .emit_operation_event(BaseOperationId(reservation.0), b"reserved")?;
                Ok(BaseResponseV1::Reserved(reservation))
            }
            BaseRequestV1::Prepare(request) => {
                let lease = self.lease(Admission::NewWork)?;
                let response = self.prepare(&lease.core, request)?;
                Ok(BaseResponseV1::Prepared(response))
            }
            BaseRequestV1::Confirm(request) => {
                let lease = self.lease(Admission::NewWork)?;
                let receipt = lease
                    .core
                    .confirm(
                        request.operation_id,
                        request.idempotency_key,
                        self.principal,
                    )
                    .await?;
                Ok(BaseResponseV1::Receipt(receipt))
            }
            BaseRequestV1::Cancel(operation_id) => {
                let lease = self.lease(Admission::Recovery)?;
                Ok(BaseResponseV1::Receipt(
                    lease.core.cancel(operation_id, self.principal)?,
                ))
            }
            BaseRequestV1::Reconcile(operation_id) => {
                let lease = self.lease(Admission::Recovery)?;
                let response = lease
                    .core
                    .current_store()?
                    .reconcile(operation_id, self.principal)
                    .map_err(store_error)?;
                Ok(BaseResponseV1::Reconciled(response))
            }
            BaseRequestV1::Subscribe(request) => {
                let lease = self.lease(Admission::Read)?;
                Ok(BaseResponseV1::Subscription(
                    lease.core.subscribe(self.principal, request)?,
                ))
            }
            BaseRequestV1::PollEvents(request) => {
                Ok(BaseResponseV1::Events(self.poll_events(request).await?))
            }
            BaseRequestV1::CloseSubscription(id) => {
                self.close_subscription(id).await?;
                Ok(BaseResponseV1::SubscriptionClosed)
            }
            BaseRequestV1::Drain => Ok(BaseResponseV1::Drain(self.drain().await?)),
            BaseRequestV1::Close => Ok(BaseResponseV1::Close(self.close().await?)),
        }
    }

    pub async fn poll_events(
        &self,
        request: BasePollEventsRequestV1,
    ) -> Result<BaseEventBatchV1, BaseServiceError> {
        let lease = self.lease(Admission::Recovery)?;
        lease.core.poll_events(self.principal, request)
    }

    pub async fn close_subscription(&self, id: BaseSubscriptionId) -> Result<(), BaseServiceError> {
        let lease = self.lease(Admission::Recovery)?;
        lease.core.close_subscription(self.principal, id)
    }

    pub async fn drain(&self) -> Result<BaseDrainReceiptV1, BaseServiceError> {
        let core = self.upgrade()?;
        core.ensure_open_generation(self.process_generation, self.dataset_generation)?;
        core.drain().await
    }

    pub async fn close(&self) -> Result<BaseCloseReceiptV1, BaseServiceError> {
        let core = self.upgrade()?;
        core.ensure_generation(self.process_generation, self.dataset_generation)?;
        core.close().await
    }

    pub fn management(
        &self,
        grant: BaseManagementGrant,
    ) -> Result<BaseManagementServices, BaseServiceError> {
        let core = self.upgrade()?;
        core.ensure_open_generation(self.process_generation, self.dataset_generation)?;
        if grant.principal != self.principal
            || grant.process_generation != self.process_generation
            || grant.dataset_generation != self.dataset_generation
        {
            return Err(BaseServiceError::new(
                BaseErrorCodeV1::Conflict,
                "management_grant_binding_mismatch",
            ));
        }
        core.consume_management_grant(grant, self.principal)
    }

    fn prepare(
        &self,
        core: &Arc<BaseServiceCore>,
        request: BasePrepareRequestV1,
    ) -> Result<PreparedBaseIntentV1, BaseServiceError> {
        let kind = command_kind(&request.command);
        validate_command_budget(&request.command)?;
        core.validate_archive_command(&request.command, request.reservation_id, self.principal)?;
        let migration = if kind == BaseOperationKindV1::RestoreArchive {
            self.negotiated_migration
                .lock()
                .map_err(|_| internal_error())?
                .clone()
        } else {
            None
        };
        let exact = encode_command(&request.command)?;
        let prepared = core
            .current_store()?
            .prepare(
                request.reservation_id,
                kind,
                exact,
                migration.as_ref(),
                self.principal,
            )
            .map_err(store_error)?;
        core.prepared.lock().map_err(|_| internal_error())?.insert(
            prepared.operation_id.0,
            PreparedCommand {
                command: request.command,
            },
        );
        core.emit_operation_event(prepared.operation_id, b"prepared")?;
        Ok(prepared)
    }

    fn upgrade(&self) -> Result<Arc<BaseServiceCore>, BaseServiceError> {
        self.core.upgrade().ok_or_else(BaseServiceError::stale)
    }

    fn lease(&self, admission: Admission) -> Result<BaseServiceLease, BaseServiceError> {
        self.upgrade()?
            .acquire(self.process_generation, self.dataset_generation, admission)
    }
}

#[derive(Clone)]
pub struct BaseManagementServices {
    core: Weak<BaseServiceCore>,
    process_generation: ProcessGenerationId,
    dataset_generation: DatasetGenerationId,
    principal: [u8; 32],
    principal_scope_digest: [u8; 32],
    management_id: [u8; 32],
}

impl BaseManagementServices {
    pub async fn invoke(
        &self,
        request: BaseManagementRequestV1,
    ) -> Result<BaseManagementResponseV1, BaseServiceError> {
        let core = self.core.upgrade().ok_or_else(BaseServiceError::stale)?;
        let _lease = core.clone().acquire(
            self.process_generation,
            self.dataset_generation,
            management_admission(&request),
        )?;
        core.ensure_management(self)?;
        match request {
            BaseManagementRequestV1::ArchiveSourceBegin(request) => {
                self.require_scope(&core, BaseManagementScope::ArchiveSource)?;
                core.begin_archive_source(
                    self,
                    request.reservation_id,
                    request.declared_total_bytes,
                )
            }
            BaseManagementRequestV1::ArchiveSourcePush(request) => {
                self.require_scope(&core, BaseManagementScope::ArchiveSource)?;
                core.push_archive_source(
                    self,
                    *request.handle.as_bytes(),
                    request.offset,
                    request.chunk.as_bytes(),
                )?;
                Ok(BaseManagementResponseV1::ArchiveCapability(
                    ArchiveCapabilityHandleV1::from_opaque_bytes(*request.handle.as_bytes()),
                ))
            }
            BaseManagementRequestV1::ArchiveSourceSeal(handle) => {
                self.require_scope(&core, BaseManagementScope::ArchiveSource)?;
                core.seal_archive_source(self, *handle.as_bytes())
            }
            BaseManagementRequestV1::ArchiveSinkBegin(request) => {
                self.require_scope(&core, BaseManagementScope::ArchiveSink)?;
                core.begin_archive_sink(self, request.reservation_id, request.max_total_bytes)
            }
            BaseManagementRequestV1::ArchiveSinkRead(request) => {
                self.require_scope(&core, BaseManagementScope::ArchiveSink)?;
                core.read_archive_sink(
                    self,
                    *request.handle.as_bytes(),
                    request.offset,
                    request.max_bytes,
                )
            }
            BaseManagementRequestV1::ArchiveSinkCommit(handle) => {
                self.require_scope(&core, BaseManagementScope::ArchiveSink)?;
                core.commit_archive_sink(self, *handle.as_bytes())?;
                Ok(BaseManagementResponseV1::CapabilityClosed)
            }
            BaseManagementRequestV1::ArchiveSecretRegister(secret) => {
                self.require_scope(&core, BaseManagementScope::ArchiveSecret)?;
                core.register_archive_secret(self, secret)
            }
            BaseManagementRequestV1::ArchiveCapabilityAbort(handle) => {
                core.abort_archive_capability(self, *handle.as_bytes())?;
                Ok(BaseManagementResponseV1::CapabilityClosed)
            }
            BaseManagementRequestV1::ArchiveCapabilityDestroy(handle) => {
                core.destroy_archive_capability(self, *handle.as_bytes())?;
                Ok(BaseManagementResponseV1::CapabilityClosed)
            }
            BaseManagementRequestV1::CompleteSignerReprovision(request) => {
                self.require_scope(&core, BaseManagementScope::SignerReprovision)?;
                core.complete_signer_reprovision(self, request)?;
                Ok(BaseManagementResponseV1::SignerReprovisioned)
            }
            BaseManagementRequestV1::Close => {
                Ok(BaseManagementResponseV1::Close(self.clone().close().await?))
            }
        }
    }

    pub async fn close(self) -> Result<BaseManagementCloseReceiptV1, BaseServiceError> {
        let core = self.core.upgrade().ok_or_else(BaseServiceError::stale)?;
        let _lease = core.clone().acquire(
            self.process_generation,
            self.dataset_generation,
            Admission::Recovery,
        )?;
        core.ensure_management(&self)?;
        core.close_management(self.management_id, self.principal)
    }

    fn require_scope(
        &self,
        core: &BaseServiceCore,
        scope: BaseManagementScope,
    ) -> Result<(), BaseServiceError> {
        core.require_management_scope(self, scope)
    }
}

struct BaseServiceCore {
    ku: Option<crate::ku_product::KuStore>,
    lifecycle: Mutex<LifecycleState>,
    drained: Notify,
    process_lease: ProcessGenerationLease,
    _runtime_claim: DatasetBaseRuntimeClaim,
    process_generation: ProcessGenerationId,
    dataset_generations: Arc<DatasetGenerationStore>,
    operation_store: RwLock<Arc<BaseOperationStore>>,
    compatibility_policy: BaseCompatibilityPolicy,
    version_status: BaseVersionStatus,
    capabilities: BaseCapabilityRequirements,
    host_authorizer: Arc<dyn BaseHostAuthorizer>,
    local_adapter: Arc<dyn BaseLocalOperationAdapter>,
    archive_capabilities: ArchiveCapabilityRegistry,
    archive_service: Option<Arc<BaseArchiveService>>,
    authority: Mutex<AuthorityState>,
    prepared: Mutex<BTreeMap<[u8; 32], PreparedCommand>>,
    subscriptions: Mutex<BTreeMap<[u8; 32], SubscriptionRecord>>,
    events: Mutex<EventState>,
    max_in_flight: u32,
    network_enabled: bool,
}

struct LifecycleState {
    state: BaseRuntimeLifecycle,
    in_flight: u32,
}

#[derive(Default)]
struct AuthorityState {
    grants: BTreeMap<[u8; 32], GrantRecord>,
    management: BTreeMap<[u8; 32], ManagementRecord>,
    capabilities: BTreeMap<[u8; 32], ManagedCapabilityRecord>,
    signer_provisions: BTreeMap<[u8; 32], SignerProvisionRecord>,
    archive_reservations: BTreeMap<[u8; 32], [u8; 32]>,
    revocation_epoch: u64,
}

#[derive(Clone)]
struct GrantRecord {
    principal: [u8; 32],
    scopes: BTreeSet<BaseManagementScope>,
    process_generation: ProcessGenerationId,
    dataset_generation: DatasetGenerationId,
    expires_at: u64,
    revocation_epoch: u64,
}

struct ManagementRecord {
    principal: [u8; 32],
    scopes: BTreeSet<BaseManagementScope>,
    scope_digest: [u8; 32],
    process_generation: ProcessGenerationId,
    dataset_generation: DatasetGenerationId,
    revocation_epoch: u64,
    active_reservations: BTreeSet<[u8; 32]>,
}

struct ManagedCapabilityRecord {
    management_id: [u8; 32],
    principal: [u8; 32],
    operation_id: [u8; 32],
    process_generation: ProcessGenerationId,
    dataset_generation: DatasetGenerationId,
    value: ManagedCapability,
}

#[derive(Clone)]
struct SignerProvisionRecord {
    principal: [u8; 32],
    process_generation: ProcessGenerationId,
    dataset_generation: DatasetGenerationId,
    requirement: SignerReprovisionRequirement,
    proof: SignerPossessionProof,
    registry: Arc<dyn SignerProviderRegistry>,
}

enum ManagedCapability {
    SourceWriting(WritableArchiveSourceHandle),
    SourceSealed(SealedArchiveSourceHandle),
    SinkWriting(WritableArchiveSinkHandle),
    SinkReadable(ReadableArchiveSinkHandle),
    Secret(ArchiveSecretHandle),
}

struct PreparedCommand {
    command: BaseCommandV1,
}

struct SubscriptionRecord {
    principal: [u8; 32],
    process_generation: ProcessGenerationId,
    dataset_generation: DatasetGenerationId,
    topic: TopicKindV1,
    cursor: u64,
}

#[derive(Default)]
struct EventState {
    next_cursor: u64,
    entries: VecDeque<BaseEventV1>,
}

#[derive(Clone, Copy)]
enum Admission {
    Read,
    NewWork,
    Recovery,
}

fn management_admission(request: &BaseManagementRequestV1) -> Admission {
    match request {
        BaseManagementRequestV1::ArchiveSinkRead(_)
        | BaseManagementRequestV1::ArchiveSinkCommit(_)
        | BaseManagementRequestV1::ArchiveCapabilityAbort(_)
        | BaseManagementRequestV1::ArchiveCapabilityDestroy(_)
        | BaseManagementRequestV1::CompleteSignerReprovision(_)
        | BaseManagementRequestV1::Close => Admission::Recovery,
        _ => Admission::NewWork,
    }
}

struct BaseServiceLease {
    core: Arc<BaseServiceCore>,
}

impl Drop for BaseServiceLease {
    fn drop(&mut self) {
        let mut lifecycle = self
            .core
            .lifecycle
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        lifecycle.in_flight = lifecycle.in_flight.saturating_sub(1);
        let drained = lifecycle.in_flight == 0;
        drop(lifecycle);
        if drained {
            self.core.drained.notify_waiters();
        }
    }
}

impl BaseServiceCore {
    fn acquire(
        self: Arc<Self>,
        process: ProcessGenerationId,
        dataset: DatasetGenerationId,
        admission: Admission,
    ) -> Result<BaseServiceLease, BaseServiceError> {
        self.ensure_generation(process, dataset)?;
        let mut lifecycle = self.lifecycle.lock().map_err(|_| internal_error())?;
        let allowed = matches!(
            (lifecycle.state, admission),
            (BaseRuntimeLifecycle::Open, _)
                | (
                    BaseRuntimeLifecycle::Draining,
                    Admission::Read | Admission::Recovery
                )
        );
        if !allowed {
            return Err(BaseServiceError::new(
                BaseErrorCodeV1::Conflict,
                "runtime_not_admitting_work",
            ));
        }
        if lifecycle.in_flight >= self.max_in_flight {
            return Err(BaseServiceError::new(
                BaseErrorCodeV1::ResourceExhausted,
                "runtime_worker_budget_exhausted",
            ));
        }
        lifecycle.in_flight += 1;
        drop(lifecycle);
        Ok(BaseServiceLease { core: self })
    }

    fn ensure_generation(
        &self,
        process: ProcessGenerationId,
        dataset: DatasetGenerationId,
    ) -> Result<(), BaseServiceError> {
        if process != self.process_generation
            || dataset != self.dataset_generations.current_generation()
        {
            return Err(BaseServiceError::stale());
        }
        Ok(())
    }

    fn ensure_open_generation(
        &self,
        process: ProcessGenerationId,
        dataset: DatasetGenerationId,
    ) -> Result<(), BaseServiceError> {
        self.ensure_generation(process, dataset)?;
        if self.lifecycle.lock().map_err(|_| internal_error())?.state != BaseRuntimeLifecycle::Open
        {
            return Err(BaseServiceError::new(
                BaseErrorCodeV1::Conflict,
                "runtime_not_open",
            ));
        }
        Ok(())
    }

    fn status(&self) -> Result<BaseStatusV1, BaseServiceError> {
        let lifecycle = self.lifecycle.lock().map_err(|_| internal_error())?;
        let subscriptions = self
            .subscriptions
            .lock()
            .map_err(|_| internal_error())?
            .len();
        let management = self.lock_authority()?.management.len();
        let mut limitations = Vec::new();
        if !cfg!(feature = "vnext-network-runtime") {
            limitations.push("network_build_unavailable");
        } else if !self.network_enabled {
            limitations.push("network_lanes_disabled");
        }
        if self.archive_service.is_none() {
            limitations.push("archive_service_unavailable");
        }
        if matches!(
            self.version_status.qualification,
            BaseQualificationState::Unqualified
        ) {
            limitations.push("base_build_unqualified");
        }
        Ok(BaseStatusV1 {
            lifecycle: lifecycle.state,
            process_generation: self.process_generation,
            dataset_generation: self.dataset_generations.current_generation(),
            version: self.version_status.clone(),
            in_flight: lifecycle.in_flight,
            open_subscriptions: subscriptions as u32,
            active_management_handles: management as u32,
            network_compiled: cfg!(feature = "vnext-network-runtime"),
            network_enabled: cfg!(feature = "vnext-network-runtime") && self.network_enabled,
            local_usable: true,
            limitations,
        })
    }

    fn current_store(&self) -> Result<Arc<BaseOperationStore>, BaseServiceError> {
        let current_generation = self.dataset_generations.current_generation();
        {
            let store = self.operation_store.read().map_err(|_| internal_error())?;
            if store.dataset_generation() == current_generation {
                return Ok(store.clone());
            }
        }
        let replacement = Arc::new(
            BaseOperationStore::open(self.dataset_generations.as_ref(), self.process_generation)
                .map_err(store_error)?,
        );
        let mut store = self.operation_store.write().map_err(|_| internal_error())?;
        if store.dataset_generation() != current_generation {
            *store = replacement;
        }
        Ok(store.clone())
    }

    async fn drain(&self) -> Result<BaseDrainReceiptV1, BaseServiceError> {
        {
            let mut lifecycle = self.lifecycle.lock().map_err(|_| internal_error())?;
            if lifecycle.state == BaseRuntimeLifecycle::Closed {
                return Err(BaseServiceError::stale());
            }
            lifecycle.state = BaseRuntimeLifecycle::Draining;
        }
        loop {
            if self
                .lifecycle
                .lock()
                .map_err(|_| internal_error())?
                .in_flight
                == 0
            {
                break;
            }
            self.drained.notified().await;
        }
        Ok(BaseDrainReceiptV1 {
            process_generation: self.process_generation,
            dataset_generation: self.dataset_generations.current_generation(),
            lifecycle: BaseRuntimeLifecycle::Draining,
        })
    }

    async fn close(&self) -> Result<BaseCloseReceiptV1, BaseServiceError> {
        let _ = self.drain().await?;
        self.close_best_effort();
        Ok(BaseCloseReceiptV1 {
            process_generation: self.process_generation,
            dataset_generation: self.dataset_generations.current_generation(),
            lifecycle: BaseRuntimeLifecycle::Closed,
        })
    }

    fn close_best_effort(&self) {
        if let Ok(mut lifecycle) = self.lifecycle.lock() {
            lifecycle.state = BaseRuntimeLifecycle::Closed;
        }
        if let Ok(mut subscriptions) = self.subscriptions.lock() {
            subscriptions.clear();
        }
        if let Ok(mut prepared) = self.prepared.lock() {
            prepared.clear();
        }
        let archive_operations = self
            .authority
            .lock()
            .map(|authority| {
                authority
                    .archive_reservations
                    .iter()
                    .map(|(operation, principal)| (*operation, *principal))
                    .collect::<Vec<_>>()
            })
            .unwrap_or_default();
        for (operation, principal) in archive_operations {
            let _ =
                self.release_archive_operation(operation, principal, BaseAuthorityStateV1::Aborted);
        }
        if let Ok(mut authority) = self.authority.lock() {
            if let Ok(store) = self.current_store() {
                for (id, capability) in &authority.capabilities {
                    let _ = revoke_archive_capability(&self.archive_capabilities, capability);
                    let _ = store.transition_authority(
                        *id,
                        BaseAuthorityKindV1::ArchiveCapability,
                        capability.principal,
                        BaseAuthorityStateV1::Aborted,
                    );
                }
                for (id, management) in &authority.management {
                    let _ = store.transition_authority(
                        *id,
                        BaseAuthorityKindV1::ManagementHandle,
                        management.principal,
                        BaseAuthorityStateV1::Revoked,
                    );
                }
                for (id, grant) in &authority.grants {
                    let _ = store.transition_authority(
                        *id,
                        BaseAuthorityKindV1::ManagementGrant,
                        grant.principal,
                        BaseAuthorityStateV1::Revoked,
                    );
                }
                for (id, provision) in &authority.signer_provisions {
                    let _ = store.transition_authority(
                        *id,
                        BaseAuthorityKindV1::SignerProvision,
                        provision.principal,
                        BaseAuthorityStateV1::Revoked,
                    );
                }
            }
            authority.capabilities.clear();
            authority.management.clear();
            authority.grants.clear();
            authority.signer_provisions.clear();
            authority.archive_reservations.clear();
            authority.revocation_epoch = authority.revocation_epoch.saturating_add(1);
        }
        let _ = self.process_lease.close();
        self.drained.notify_waiters();
    }

    async fn confirm(
        self: &Arc<Self>,
        operation_id: BaseOperationId,
        idempotency_key: BaseIdempotencyKey,
        principal: [u8; 32],
    ) -> Result<BaseOperationReceiptV1, BaseServiceError> {
        let source_store = self.current_store()?;
        if let Some(ku) = &self.ku {
            if ku.owns_operation(
                principal,
                onebrain_base_contract::ku::OperationId(operation_id.0),
            )? {
                return Err(crate::ku_product::invalid());
            }
        }
        if let Some(receipt) = source_store
            .begin_confirm(operation_id, idempotency_key, principal)
            .map_err(store_error)?
        {
            return Ok(receipt);
        }
        let prepared = self
            .prepared
            .lock()
            .map_err(|_| internal_error())?
            .remove(&operation_id.0);
        let Some(prepared) = prepared else {
            return source_store.mark_unknown(operation_id).map_err(store_error);
        };
        let effect = match prepared.command {
            BaseCommandV1::ExistingLocalCommand(command) => {
                self.local_adapter.confirm_local(command)
            }
            BaseCommandV1::CreateArchive(command) => {
                self.confirm_create_archive(operation_id, command).await
            }
            BaseCommandV1::RestoreArchive(command) => {
                self.confirm_restore_archive(
                    source_store.clone(),
                    operation_id,
                    idempotency_key,
                    principal,
                    command,
                )
                .await
            }
        };
        match effect {
            Ok(bytes) => {
                let target_store = self.current_store()?;
                let receipt = source_store
                    .complete(operation_id, bytes)
                    .map_err(store_error)?;
                if !Arc::ptr_eq(&source_store, &target_store) {
                    source_store
                        .carry_record_to(operation_id, &target_store, false)
                        .map_err(store_error)?;
                }
                self.emit_operation_event(operation_id, b"committed")?;
                Ok(receipt)
            }
            Err(error) if error.code == BaseErrorCodeV1::UnknownOutcome => {
                source_store.mark_unknown(operation_id).map_err(store_error)
            }
            Err(error) => {
                let receipt = source_store
                    .fail(operation_id, error.code)
                    .map_err(store_error)?;
                self.emit_operation_event(operation_id, b"failed")?;
                Ok(receipt)
            }
        }
    }

    fn cancel(
        &self,
        operation_id: BaseOperationId,
        principal: [u8; 32],
    ) -> Result<BaseOperationReceiptV1, BaseServiceError> {
        let store = self.current_store()?;
        let kind = store
            .operation_kind(operation_id, principal)
            .map_err(store_error)?;
        if let Some(ku) = &self.ku {
            let id = onebrain_base_contract::ku::OperationId(operation_id.0);
            if ku.owns_operation(principal, id)? {
                let receipt = store
                    .cancel_before_confirmation(operation_id, principal)
                    .map_err(store_error)?;
                ku.cancel(principal, id)
                    .map_err(|_| crate::ku_product::unknown())?;
                return Ok(receipt);
            }
        }
        let receipt = store.cancel(operation_id, principal).map_err(store_error)?;
        if matches!(
            kind,
            BaseOperationKindV1::CreateArchive | BaseOperationKindV1::RestoreArchive
        ) {
            self.release_archive_operation(
                operation_id.0,
                principal,
                BaseAuthorityStateV1::Aborted,
            )?;
        }
        self.remove_prepared(operation_id)?;
        self.emit_operation_event(operation_id, b"canceled")?;
        Ok(receipt)
    }

    async fn confirm_create_archive(
        &self,
        operation_id: BaseOperationId,
        command: onebrain_base_contract::CreateArchiveCommandV1,
    ) -> Result<Vec<u8>, BaseServiceError> {
        let service = self.archive_service.as_ref().ok_or_else(|| {
            BaseServiceError::new(
                BaseErrorCodeV1::CapabilityDisabled,
                "archive_service_disabled",
            )
        })?;
        let (sink, secret, owner) = self.take_archive_create_capabilities(
            operation_id,
            *command.sink.as_bytes(),
            *command.secret.as_bytes(),
        )?;
        let producer = self
            .compatibility_policy
            .current
            .producer_artifact_identity();
        let receipt = match service.create_archive(sink, secret, producer).await {
            Ok(receipt) => receipt,
            Err(_error) => {
                let store = self.current_store()?;
                for id in [*command.sink.as_bytes(), *command.secret.as_bytes()] {
                    let _ = store.transition_authority(
                        id,
                        BaseAuthorityKindV1::ArchiveCapability,
                        owner.principal,
                        BaseAuthorityStateV1::UnknownOutcome,
                    );
                }
                let _ = self.release_archive_operation(
                    operation_id.0,
                    owner.principal,
                    BaseAuthorityStateV1::UnknownOutcome,
                );
                return Err(BaseServiceError::new(
                    BaseErrorCodeV1::UnknownOutcome,
                    "archive_create_unknown",
                ));
            }
        };
        let sink_id = *receipt.readable_sink.id().as_bytes();
        let manifest_root = receipt.manifest_root;
        let publish_capability = (|| {
            self.current_store()?
                .transition_authority(
                    *command.secret.as_bytes(),
                    BaseAuthorityKindV1::ArchiveCapability,
                    owner.principal,
                    BaseAuthorityStateV1::Committed,
                )
                .map_err(store_error)?;
            self.insert_managed_capability(
                sink_id,
                owner,
                ManagedCapability::SinkReadable(receipt.readable_sink),
            )
        })();
        if publish_capability.is_err() {
            let _ = self.release_archive_operation(
                operation_id.0,
                owner.principal,
                BaseAuthorityStateV1::UnknownOutcome,
            );
            return Err(BaseServiceError::new(
                BaseErrorCodeV1::UnknownOutcome,
                "archive_create_receipt_unknown",
            ));
        }
        let mut result = Vec::with_capacity(64);
        result.extend_from_slice(&sink_id);
        result.extend_from_slice(&manifest_root);
        Ok(result)
    }

    async fn confirm_restore_archive(
        &self,
        source_store: Arc<BaseOperationStore>,
        operation_id: BaseOperationId,
        idempotency_key: BaseIdempotencyKey,
        principal: [u8; 32],
        command: onebrain_base_contract::RestoreArchiveCommandV1,
    ) -> Result<Vec<u8>, BaseServiceError> {
        let service = self.archive_service.as_ref().ok_or_else(|| {
            BaseServiceError::new(
                BaseErrorCodeV1::CapabilityDisabled,
                "archive_service_disabled",
            )
        })?;
        let (source, secret) = self.take_archive_restore_capabilities(
            operation_id,
            *command.source.as_bytes(),
            *command.secret.as_bytes(),
        )?;
        let migration = source_store.migration(operation_id).map_err(store_error)?;
        let context = ActivationOperationContext {
            principal_digest: principal,
            process_generation: self.process_generation.0,
            migration_vector_id: migration
                .as_ref()
                .map(|value| value.vector_id.as_str().to_owned()),
            migration_vector_blake3: migration.as_ref().map(|value| value.vector_blake3.0),
            migration_trust_policy_digest: migration
                .as_ref()
                .map(|value| value.trust_policy_digest.0),
        };
        let policy = self
            .compatibility_policy
            .to_archive_restore_policy()
            .map_err(|_| {
                BaseServiceError::new(
                    BaseErrorCodeV1::IncompatibleProfile,
                    "archive_policy_invalid",
                )
            })?;
        let receipt = service
            .restore_archive_for_base(
                source,
                secret,
                &policy,
                RestoreOperationBinding {
                    operation_id: operation_id.0,
                    idempotency_key: idempotency_key.0,
                },
                context,
            )
            .await;
        let receipt = match receipt {
            Ok(receipt) => receipt,
            Err(_) => {
                for id in [*command.source.as_bytes(), *command.secret.as_bytes()] {
                    let _ = source_store.transition_authority(
                        id,
                        BaseAuthorityKindV1::ArchiveCapability,
                        principal,
                        BaseAuthorityStateV1::UnknownOutcome,
                    );
                }
                let _ = self.release_archive_operation(
                    operation_id.0,
                    principal,
                    BaseAuthorityStateV1::UnknownOutcome,
                );
                return Err(BaseServiceError::new(
                    BaseErrorCodeV1::UnknownOutcome,
                    "archive_restore_unknown",
                ));
            }
        };
        let finalize = (|| {
            for id in [*command.source.as_bytes(), *command.secret.as_bytes()] {
                source_store
                    .transition_authority(
                        id,
                        BaseAuthorityKindV1::ArchiveCapability,
                        principal,
                        BaseAuthorityStateV1::Committed,
                    )
                    .map_err(store_error)?;
            }
            self.release_archive_operation(
                operation_id.0,
                principal,
                BaseAuthorityStateV1::Committed,
            )
        })();
        if finalize.is_err() {
            return Err(BaseServiceError::new(
                BaseErrorCodeV1::UnknownOutcome,
                "archive_restore_receipt_unknown",
            ));
        }
        Ok(encode_restore_receipt(&receipt))
    }

    fn validate_archive_command(
        &self,
        command: &BaseCommandV1,
        reservation: BaseOperationReservationId,
        principal: [u8; 32],
    ) -> Result<(), BaseServiceError> {
        match command {
            BaseCommandV1::ExistingLocalCommand(_) => Ok(()),
            BaseCommandV1::CreateArchive(command) => self.validate_capability_pair(
                reservation,
                principal,
                *command.sink.as_bytes(),
                *command.secret.as_bytes(),
                false,
            ),
            BaseCommandV1::RestoreArchive(command) => self.validate_capability_pair(
                reservation,
                principal,
                *command.source.as_bytes(),
                *command.secret.as_bytes(),
                true,
            ),
        }
    }

    fn validate_capability_pair(
        &self,
        reservation: BaseOperationReservationId,
        principal: [u8; 32],
        primary: [u8; 32],
        secret: [u8; 32],
        restore: bool,
    ) -> Result<(), BaseServiceError> {
        let authority = self.lock_authority()?;
        let primary = authority.capabilities.get(&primary).ok_or_else(not_found)?;
        let secret = authority.capabilities.get(&secret).ok_or_else(not_found)?;
        if primary.operation_id != reservation.0
            || secret.operation_id != reservation.0
            || primary.principal != principal
            || secret.principal != principal
            || primary.management_id != secret.management_id
            || !matches!(secret.value, ManagedCapability::Secret(_))
            || (restore && !matches!(primary.value, ManagedCapability::SourceSealed(_)))
            || (!restore && !matches!(primary.value, ManagedCapability::SinkWriting(_)))
        {
            return Err(conflict("archive_capability_binding_mismatch"));
        }
        Ok(())
    }

    fn consume_management_grant(
        self: &Arc<Self>,
        grant: BaseManagementGrant,
        principal: [u8; 32],
    ) -> Result<BaseManagementServices, BaseServiceError> {
        let mut authority = self.lock_authority()?;
        let record = authority.grants.get(&grant.id).cloned().ok_or_else(|| {
            BaseServiceError::new(BaseErrorCodeV1::NotFound, "management_grant_not_found")
        })?;
        if record.principal != principal
            || record.process_generation != self.process_generation
            || record.dataset_generation != self.dataset_generations.current_generation()
            || record.revocation_epoch != authority.revocation_epoch
            || record.expires_at < now_seconds()
        {
            return Err(BaseServiceError::new(
                BaseErrorCodeV1::Expired,
                "management_grant_stale_or_expired",
            ));
        }
        let management_id = unique_id(&authority.management)?;
        let scope_digest = scope_digest(&record.scopes);
        let revocation_epoch = authority.revocation_epoch;
        let store = self.current_store()?;
        store
            .transition_authority(
                grant.id,
                BaseAuthorityKindV1::ManagementGrant,
                principal,
                BaseAuthorityStateV1::Revoked,
            )
            .map_err(store_error)?;
        store
            .register_authority(
                management_id,
                BaseAuthorityKindV1::ManagementHandle,
                principal,
                None,
                None,
            )
            .and_then(|()| {
                store.transition_authority(
                    management_id,
                    BaseAuthorityKindV1::ManagementHandle,
                    principal,
                    BaseAuthorityStateV1::Active,
                )
            })
            .map_err(store_error)?;
        authority.grants.remove(&grant.id);
        authority.management.insert(
            management_id,
            ManagementRecord {
                principal,
                scopes: record.scopes,
                scope_digest,
                process_generation: self.process_generation,
                dataset_generation: self.dataset_generations.current_generation(),
                revocation_epoch,
                active_reservations: BTreeSet::new(),
            },
        );
        Ok(BaseManagementServices {
            core: Arc::downgrade(self),
            process_generation: self.process_generation,
            dataset_generation: self.dataset_generations.current_generation(),
            principal,
            principal_scope_digest: scope_digest,
            management_id,
        })
    }

    fn ensure_management(&self, handle: &BaseManagementServices) -> Result<(), BaseServiceError> {
        self.ensure_generation(handle.process_generation, handle.dataset_generation)?;
        let authority = self.lock_authority()?;
        let record = authority
            .management
            .get(&handle.management_id)
            .ok_or_else(not_found)?;
        if record.principal != handle.principal
            || record.scope_digest != handle.principal_scope_digest
            || record.process_generation != handle.process_generation
            || record.dataset_generation != handle.dataset_generation
            || record.revocation_epoch != authority.revocation_epoch
        {
            return Err(conflict("management_handle_binding_mismatch"));
        }
        Ok(())
    }

    fn require_management_scope(
        &self,
        handle: &BaseManagementServices,
        scope: BaseManagementScope,
    ) -> Result<(), BaseServiceError> {
        self.ensure_management(handle)?;
        if !self
            .lock_authority()?
            .management
            .get(&handle.management_id)
            .ok_or_else(not_found)?
            .scopes
            .contains(&scope)
        {
            return Err(BaseServiceError::new(
                BaseErrorCodeV1::CapabilityDisabled,
                "management_scope_missing",
            ));
        }
        Ok(())
    }

    fn complete_signer_reprovision(
        &self,
        handle: &BaseManagementServices,
        request: CompleteSignerReprovisionV1,
    ) -> Result<(), BaseServiceError> {
        let provision_id = *request.provision_handle.as_bytes();
        let provision = self
            .lock_authority()?
            .signer_provisions
            .get(&provision_id)
            .cloned()
            .ok_or_else(not_found)?;
        if provision.principal != handle.principal
            || provision.process_generation != handle.process_generation
            || provision.dataset_generation != handle.dataset_generation
            || !signer_request_matches(&request, &provision.requirement)
        {
            return Err(conflict("signer_provision_binding_mismatch"));
        }
        self.dataset_generations
            .complete_reprovision(
                &provision.requirement,
                &provision.proof,
                provision.registry.as_ref(),
            )
            .map_err(|_| {
                BaseServiceError::new(
                    BaseErrorCodeV1::ReprovisionRequired,
                    "signer_reprovision_failed",
                )
            })?;
        self.current_store()?
            .transition_authority(
                provision_id,
                BaseAuthorityKindV1::SignerProvision,
                handle.principal,
                BaseAuthorityStateV1::Committed,
            )
            .map_err(store_error)?;
        self.lock_authority()?
            .signer_provisions
            .remove(&provision_id);
        Ok(())
    }

    fn begin_archive_source(
        &self,
        handle: &BaseManagementServices,
        reservation: BaseOperationReservationId,
        total: u64,
    ) -> Result<BaseManagementResponseV1, BaseServiceError> {
        self.current_store()?
            .validate_reservation(
                reservation,
                BaseOperationKindV1::RestoreArchive,
                handle.principal,
            )
            .map_err(store_error)?;
        let archive_reservation = ArchiveOperationReservationId::from_bytes(reservation.0);
        let source = self
            .archive_capabilities
            .begin_source(archive_reservation, total)
            .map_err(node_error)?;
        let id = *source.id().as_bytes();
        self.insert_management_capability(
            handle,
            reservation.0,
            id,
            ManagedCapability::SourceWriting(source),
        )?;
        Ok(BaseManagementResponseV1::ArchiveSource(
            ArchiveSourceHandleV1::from_opaque_bytes(id),
        ))
    }

    fn push_archive_source(
        &self,
        handle: &BaseManagementServices,
        id: [u8; 32],
        offset: u64,
        bytes: &[u8],
    ) -> Result<(), BaseServiceError> {
        let authority = self.lock_authority()?;
        let record = validate_managed_capability(&authority, handle, id)?;
        let ManagedCapability::SourceWriting(source) = &record.value else {
            return Err(conflict("archive_source_state_mismatch"));
        };
        self.archive_capabilities
            .push_source_chunk(source, offset, bytes)
            .map_err(node_error)
    }

    fn seal_archive_source(
        &self,
        handle: &BaseManagementServices,
        id: [u8; 32],
    ) -> Result<BaseManagementResponseV1, BaseServiceError> {
        let mut authority = self.lock_authority()?;
        let record = authority.capabilities.remove(&id).ok_or_else(not_found)?;
        validate_managed_record(&record, handle)?;
        let owner = ManagedCapabilityOwner::from_record(&record);
        let ManagedCapability::SourceWriting(source) = record.value else {
            return Err(conflict("archive_source_state_mismatch"));
        };
        let sealed = self
            .archive_capabilities
            .seal_source(source)
            .map_err(node_error)?;
        let sealed_id = *sealed.id().as_bytes();
        if sealed_id != id {
            let _ = self.archive_capabilities.abort(sealed.id());
            return Err(conflict("archive_capability_identity_changed"));
        }
        self.current_store()?
            .transition_authority(
                id,
                BaseAuthorityKindV1::ArchiveCapability,
                owner.principal,
                BaseAuthorityStateV1::Sealed,
            )
            .map_err(store_error)?;
        authority.capabilities.insert(
            sealed_id,
            ManagedCapabilityRecord {
                management_id: owner.management_id,
                principal: owner.principal,
                operation_id: owner.operation_id,
                process_generation: owner.process_generation,
                dataset_generation: owner.dataset_generation,
                value: ManagedCapability::SourceSealed(sealed),
            },
        );
        Ok(BaseManagementResponseV1::ArchiveSource(
            ArchiveSourceHandleV1::from_opaque_bytes(sealed_id),
        ))
    }

    fn begin_archive_sink(
        &self,
        handle: &BaseManagementServices,
        reservation: BaseOperationReservationId,
        total: u64,
    ) -> Result<BaseManagementResponseV1, BaseServiceError> {
        self.current_store()?
            .validate_reservation(
                reservation,
                BaseOperationKindV1::CreateArchive,
                handle.principal,
            )
            .map_err(store_error)?;
        let sink = self
            .archive_capabilities
            .begin_sink(
                ArchiveOperationReservationId::from_bytes(reservation.0),
                total,
            )
            .map_err(node_error)?;
        let id = *sink.id().as_bytes();
        self.insert_management_capability(
            handle,
            reservation.0,
            id,
            ManagedCapability::SinkWriting(sink),
        )?;
        Ok(BaseManagementResponseV1::ArchiveSink(
            ArchiveSinkHandleV1::from_opaque_bytes(id),
        ))
    }

    fn read_archive_sink(
        &self,
        handle: &BaseManagementServices,
        id: [u8; 32],
        offset: u64,
        max: u32,
    ) -> Result<BaseManagementResponseV1, BaseServiceError> {
        let authority = self.lock_authority()?;
        let record = validate_managed_capability(&authority, handle, id)?;
        let ManagedCapability::SinkReadable(sink) = &record.value else {
            return Err(conflict("archive_sink_state_mismatch"));
        };
        let chunk = self
            .archive_capabilities
            .read_sink_chunk(sink, offset, max)
            .map_err(node_error)?;
        Ok(BaseManagementResponseV1::ArchiveChunk {
            offset: chunk.offset,
            bytes: chunk.bytes,
            eof: chunk.eof,
        })
    }

    fn commit_archive_sink(
        &self,
        handle: &BaseManagementServices,
        id: [u8; 32],
    ) -> Result<(), BaseServiceError> {
        let mut authority = self.lock_authority()?;
        let record = authority.capabilities.remove(&id).ok_or_else(not_found)?;
        validate_managed_record(&record, handle)?;
        let operation_id = record.operation_id;
        let principal = record.principal;
        let ManagedCapability::SinkReadable(sink) = record.value else {
            return Err(conflict("archive_sink_state_mismatch"));
        };
        self.archive_capabilities
            .commit_sink(sink)
            .map_err(node_error)?;
        let journaled = self.current_store()?.transition_authority(
            id,
            BaseAuthorityKindV1::ArchiveCapability,
            handle.principal,
            BaseAuthorityStateV1::Committed,
        );
        drop(authority);
        let released = self.release_archive_operation(
            operation_id,
            principal,
            BaseAuthorityStateV1::Committed,
        );
        if journaled.is_err() || released.is_err() {
            return Err(BaseServiceError::new(
                BaseErrorCodeV1::UnknownOutcome,
                "archive_sink_commit_unknown",
            ));
        }
        Ok(())
    }

    fn register_archive_secret(
        &self,
        handle: &BaseManagementServices,
        secret: onebrain_base_contract::BoundedSecretIngressV1,
    ) -> Result<BaseManagementResponseV1, BaseServiceError> {
        let reservation = {
            let authority = self.lock_authority()?;
            let management = authority
                .management
                .get(&handle.management_id)
                .ok_or_else(not_found)?;
            if management.active_reservations.len() != 1 {
                return Err(conflict("archive_secret_requires_one_active_reservation"));
            }
            *management
                .active_reservations
                .iter()
                .next()
                .ok_or_else(not_found)?
        };
        let kind = match secret.kind() {
            ArchiveCredentialKindV1::Password => ArchiveCredentialKind::Password,
            ArchiveCredentialKindV1::RecoveryKey => ArchiveCredentialKind::RecoveryKey,
        };
        let registered = self
            .archive_capabilities
            .register_secret(
                ArchiveOperationReservationId::from_bytes(reservation),
                kind,
                secret.into_zeroizing_bytes(),
            )
            .map_err(node_error)?;
        let id = *registered.id().as_bytes();
        self.insert_management_capability(
            handle,
            reservation,
            id,
            ManagedCapability::Secret(registered),
        )?;
        Ok(BaseManagementResponseV1::ArchiveSecret(
            ArchiveSecretHandleV1::from_opaque_bytes(id),
        ))
    }

    fn abort_archive_capability(
        &self,
        handle: &BaseManagementServices,
        id: [u8; 32],
    ) -> Result<(), BaseServiceError> {
        let mut authority = self.lock_authority()?;
        let record = authority.capabilities.remove(&id).ok_or_else(not_found)?;
        validate_managed_record(&record, handle)?;
        self.archive_capabilities
            .abort(capability_id(&record))
            .map_err(node_error)?;
        self.current_store()?
            .transition_authority(
                id,
                BaseAuthorityKindV1::ArchiveCapability,
                handle.principal,
                BaseAuthorityStateV1::Aborted,
            )
            .map_err(store_error)?;
        drop(record);
        Ok(())
    }

    fn destroy_archive_capability(
        &self,
        handle: &BaseManagementServices,
        id: [u8; 32],
    ) -> Result<(), BaseServiceError> {
        let mut authority = self.lock_authority()?;
        let record = authority.capabilities.remove(&id).ok_or_else(not_found)?;
        validate_managed_record(&record, handle)?;
        self.archive_capabilities
            .destroy(capability_id(&record))
            .map_err(node_error)?;
        self.current_store()?
            .transition_authority(
                id,
                BaseAuthorityKindV1::ArchiveCapability,
                handle.principal,
                BaseAuthorityStateV1::Destroyed,
            )
            .map_err(store_error)?;
        drop(record);
        Ok(())
    }

    fn release_archive_operation(
        &self,
        operation_id: [u8; 32],
        principal: [u8; 32],
        state: BaseAuthorityStateV1,
    ) -> Result<(), BaseServiceError> {
        let mut authority = self.lock_authority()?;
        match authority.archive_reservations.get(&operation_id) {
            None => return Ok(()),
            Some(owner) if owner != &principal => {
                return Err(conflict("archive_reservation_principal_mismatch"));
            }
            Some(_) => {}
        }
        let ids = authority
            .capabilities
            .iter()
            .filter_map(|(id, capability)| {
                (capability.operation_id == operation_id && capability.principal == principal)
                    .then_some(*id)
            })
            .collect::<Vec<_>>();
        let store = self.current_store()?;
        for id in &ids {
            store
                .transition_authority(
                    *id,
                    BaseAuthorityKindV1::ArchiveCapability,
                    principal,
                    state,
                )
                .map_err(store_error)?;
        }
        self.archive_capabilities
            .release_reservation(ArchiveOperationReservationId::from_bytes(operation_id))
            .map_err(node_error)?;
        authority.archive_reservations.remove(&operation_id);
        for id in ids {
            authority.capabilities.remove(&id);
        }
        for management in authority.management.values_mut() {
            management.active_reservations.remove(&operation_id);
        }
        Ok(())
    }

    fn insert_management_capability(
        &self,
        handle: &BaseManagementServices,
        operation_id: [u8; 32],
        id: [u8; 32],
        value: ManagedCapability,
    ) -> Result<(), BaseServiceError> {
        let capability_kind = managed_capability_kind(&value);
        let mut authority = self.lock_authority()?;
        let management = authority
            .management
            .get_mut(&handle.management_id)
            .ok_or_else(not_found)?;
        let store = self.current_store()?;
        store
            .register_authority(
                id,
                BaseAuthorityKindV1::ArchiveCapability,
                handle.principal,
                Some(operation_id),
                Some(capability_kind),
            )
            .map_err(store_error)?;
        store
            .transition_authority(
                id,
                BaseAuthorityKindV1::ArchiveCapability,
                handle.principal,
                BaseAuthorityStateV1::Active,
            )
            .map_err(store_error)?;
        management.active_reservations.insert(operation_id);
        if authority
            .capabilities
            .insert(
                id,
                ManagedCapabilityRecord {
                    management_id: handle.management_id,
                    principal: handle.principal,
                    operation_id,
                    process_generation: handle.process_generation,
                    dataset_generation: handle.dataset_generation,
                    value,
                },
            )
            .is_some()
        {
            return Err(conflict("archive_capability_collision"));
        }
        Ok(())
    }

    fn insert_managed_capability(
        &self,
        id: [u8; 32],
        owner: ManagedCapabilityOwner,
        value: ManagedCapability,
    ) -> Result<(), BaseServiceError> {
        self.current_store()?
            .transition_authority(
                id,
                BaseAuthorityKindV1::ArchiveCapability,
                owner.principal,
                BaseAuthorityStateV1::Sealed,
            )
            .map_err(store_error)?;
        let mut authority = self.lock_authority()?;
        if authority
            .capabilities
            .insert(
                id,
                ManagedCapabilityRecord {
                    management_id: owner.management_id,
                    principal: owner.principal,
                    operation_id: owner.operation_id,
                    process_generation: owner.process_generation,
                    dataset_generation: owner.dataset_generation,
                    value,
                },
            )
            .is_some()
        {
            return Err(conflict("archive_capability_collision"));
        }
        Ok(())
    }

    fn take_archive_create_capabilities(
        &self,
        operation_id: BaseOperationId,
        sink_id: [u8; 32],
        secret_id: [u8; 32],
    ) -> Result<
        (
            WritableArchiveSinkHandle,
            ArchiveSecretHandle,
            ManagedCapabilityOwner,
        ),
        BaseServiceError,
    > {
        let mut authority = self.lock_authority()?;
        let sink = authority
            .capabilities
            .remove(&sink_id)
            .ok_or_else(not_found)?;
        let secret = authority
            .capabilities
            .remove(&secret_id)
            .ok_or_else(not_found)?;
        if sink.operation_id != operation_id.0 || secret.operation_id != operation_id.0 {
            return Err(conflict("archive_capability_operation_mismatch"));
        }
        let owner = ManagedCapabilityOwner::from_record(&sink);
        let ManagedCapability::SinkWriting(sink) = sink.value else {
            return Err(conflict("archive_sink_state_mismatch"));
        };
        let ManagedCapability::Secret(secret) = secret.value else {
            return Err(conflict("archive_secret_state_mismatch"));
        };
        Ok((sink, secret, owner))
    }

    fn take_archive_restore_capabilities(
        &self,
        operation_id: BaseOperationId,
        source_id: [u8; 32],
        secret_id: [u8; 32],
    ) -> Result<(SealedArchiveSourceHandle, ArchiveSecretHandle), BaseServiceError> {
        let mut authority = self.lock_authority()?;
        let source = authority
            .capabilities
            .remove(&source_id)
            .ok_or_else(not_found)?;
        let secret = authority
            .capabilities
            .remove(&secret_id)
            .ok_or_else(not_found)?;
        if source.operation_id != operation_id.0 || secret.operation_id != operation_id.0 {
            return Err(conflict("archive_capability_operation_mismatch"));
        }
        let ManagedCapability::SourceSealed(source) = source.value else {
            return Err(conflict("archive_source_state_mismatch"));
        };
        let ManagedCapability::Secret(secret) = secret.value else {
            return Err(conflict("archive_secret_state_mismatch"));
        };
        Ok((source, secret))
    }

    fn close_management(
        &self,
        management_id: [u8; 32],
        principal: [u8; 32],
    ) -> Result<BaseManagementCloseReceiptV1, BaseServiceError> {
        let (ids, operations) = {
            let authority = self.lock_authority()?;
            let record = authority
                .management
                .get(&management_id)
                .ok_or_else(not_found)?;
            if record.principal != principal {
                return Err(conflict("management_principal_mismatch"));
            }
            let ids = authority
                .capabilities
                .iter()
                .filter_map(|(id, capability)| {
                    (capability.management_id == management_id).then_some(*id)
                })
                .collect::<Vec<_>>();
            (ids, record.active_reservations.clone())
        };
        for operation in operations {
            self.release_archive_operation(operation, principal, BaseAuthorityStateV1::Aborted)?;
        }
        self.current_store()?
            .transition_authority(
                management_id,
                BaseAuthorityKindV1::ManagementHandle,
                principal,
                BaseAuthorityStateV1::Revoked,
            )
            .map_err(store_error)?;
        self.lock_authority()?.management.remove(&management_id);
        Ok(BaseManagementCloseReceiptV1 {
            management_handle: management_id,
            revoked_capabilities: ids.len() as u32,
        })
    }

    fn subscribe(
        &self,
        principal: [u8; 32],
        request: BaseSubscriptionRequestV1,
    ) -> Result<BaseSubscriptionId, BaseServiceError> {
        let mut subscriptions = self.subscriptions.lock().map_err(|_| internal_error())?;
        if subscriptions.len() >= MAX_SUBSCRIPTIONS {
            return Err(BaseServiceError::new(
                BaseErrorCodeV1::ResourceExhausted,
                "subscription_budget_exhausted",
            ));
        }
        let id = unique_id(&subscriptions)?;
        self.current_store()?
            .create_subscription(
                id,
                principal,
                request.topic.discriminator(),
                request.cursor.unwrap_or(0),
            )
            .map_err(store_error)?;
        subscriptions.insert(
            id,
            SubscriptionRecord {
                principal,
                process_generation: self.process_generation,
                dataset_generation: self.dataset_generations.current_generation(),
                topic: request.topic,
                cursor: request.cursor.unwrap_or(0),
            },
        );
        Ok(BaseSubscriptionId::from_opaque_bytes(id))
    }

    fn poll_events(
        &self,
        principal: [u8; 32],
        request: BasePollEventsRequestV1,
    ) -> Result<BaseEventBatchV1, BaseServiceError> {
        if request.max_items == 0 || request.max_items > MAX_EVENT_ITEMS {
            return Err(BaseServiceError::new(
                BaseErrorCodeV1::InvalidRequest,
                "event_batch_bound_invalid",
            ));
        }
        let id = *request.subscription_id.as_bytes();
        let mut subscriptions = self.subscriptions.lock().map_err(|_| internal_error())?;
        let subscription = subscriptions.get_mut(&id).ok_or_else(not_found)?;
        if subscription.principal != principal
            || subscription.process_generation != self.process_generation
            || subscription.dataset_generation != self.dataset_generations.current_generation()
            || request.after_cursor < subscription.cursor
        {
            return Err(conflict("subscription_binding_or_cursor_mismatch"));
        }
        let events = self.events.lock().map_err(|_| internal_error())?;
        let earliest = events
            .entries
            .front()
            .map(|event| event.cursor)
            .unwrap_or(events.next_cursor);
        if request.after_cursor.saturating_add(1) < earliest {
            return Ok(BaseEventBatchV1 {
                subscription_id: request.subscription_id,
                events: Vec::new(),
                next_cursor: request.after_cursor,
                earliest_available_cursor: earliest,
                resync_required: true,
            });
        }
        let selected = events
            .entries
            .iter()
            .filter(|event| {
                event.topic == subscription.topic && event.cursor > request.after_cursor
            })
            .take(request.max_items as usize)
            .cloned()
            .collect::<Vec<_>>();
        let next = selected
            .last()
            .map(|event| event.cursor)
            .unwrap_or(request.after_cursor);
        self.current_store()?
            .advance_subscription(id, principal, next)
            .map_err(store_error)?;
        subscription.cursor = next;
        Ok(BaseEventBatchV1 {
            subscription_id: request.subscription_id,
            events: selected,
            next_cursor: next,
            earliest_available_cursor: earliest,
            resync_required: false,
        })
    }

    fn close_subscription(
        &self,
        principal: [u8; 32],
        id: BaseSubscriptionId,
    ) -> Result<(), BaseServiceError> {
        let mut subscriptions = self.subscriptions.lock().map_err(|_| internal_error())?;
        match subscriptions.get(id.as_bytes()) {
            Some(record) if record.principal != principal => {
                Err(conflict("subscription_principal_mismatch"))
            }
            Some(_) => {
                self.current_store()?
                    .close_subscription(*id.as_bytes(), principal)
                    .map_err(store_error)?;
                subscriptions.remove(id.as_bytes());
                Ok(())
            }
            None => Ok(()),
        }
    }

    fn emit_operation_event(
        &self,
        operation_id: BaseOperationId,
        payload: &[u8],
    ) -> Result<(), BaseServiceError> {
        if payload.len() > MAX_EVENT_PAYLOAD_BYTES {
            return Err(BaseServiceError::new(
                BaseErrorCodeV1::ResourceExhausted,
                "event_payload_too_large",
            ));
        }
        let mut events = self.events.lock().map_err(|_| internal_error())?;
        events.next_cursor = events
            .next_cursor
            .checked_add(1)
            .ok_or_else(internal_error)?;
        let cursor = events.next_cursor;
        events.entries.push_back(BaseEventV1 {
            cursor,
            topic: TopicKindV1::OperationReceipts,
            operation_id: Some(operation_id),
            payload: payload.to_vec(),
        });
        let mut gap = None;
        while events.entries.len() > MAX_EVENTS {
            events.entries.pop_front();
            gap = events.entries.front().map(|event| event.cursor);
        }
        drop(events);
        if let Some(earliest) = gap {
            self.current_store()?
                .record_gap_marker(earliest)
                .map_err(store_error)?;
        }
        Ok(())
    }

    fn remove_prepared(&self, operation_id: BaseOperationId) -> Result<(), BaseServiceError> {
        self.prepared
            .lock()
            .map_err(|_| internal_error())?
            .remove(&operation_id.0);
        Ok(())
    }

    fn recover_activation_receipt(&self) -> Result<(), BaseServiceError> {
        let directory = self
            .dataset_generations
            .current_resolver()
            .map_err(|_| corrupt_error())?
            .owner_path(crate::dataset_path::BaseStorageOwnerId::BASE_OPERATIONS)
            .map_err(|_| corrupt_error())?
            .join("activation-receipts");
        if !directory.exists() {
            return Ok(());
        }
        for entry in std::fs::read_dir(directory).map_err(|_| corrupt_error())? {
            let entry = entry.map_err(|_| corrupt_error())?;
            let receipt: crate::DatasetGenerationReceipt =
                serde_json::from_slice(&std::fs::read(entry.path()).map_err(|_| corrupt_error())?)
                    .map_err(|_| corrupt_error())?;
            let Some(context) = receipt.operation_context.as_ref() else {
                continue;
            };
            let _prior_process = context.process_generation;
            self.current_store()?
                .import_activation_receipt(&receipt)
                .map_err(store_error)?;
        }
        Ok(())
    }

    fn lock_authority(
        &self,
    ) -> Result<std::sync::MutexGuard<'_, AuthorityState>, BaseServiceError> {
        self.authority.lock().map_err(|_| internal_error())
    }
}

#[derive(Clone, Copy)]
struct ManagedCapabilityOwner {
    management_id: [u8; 32],
    principal: [u8; 32],
    operation_id: [u8; 32],
    process_generation: ProcessGenerationId,
    dataset_generation: DatasetGenerationId,
}

impl ManagedCapabilityOwner {
    fn from_record(record: &ManagedCapabilityRecord) -> Self {
        Self {
            management_id: record.management_id,
            principal: record.principal,
            operation_id: record.operation_id,
            process_generation: record.process_generation,
            dataset_generation: record.dataset_generation,
        }
    }
}

fn validate_runtime_config(config: &BaseRuntimeConfig) -> Result<(), BaseServiceError> {
    if config.max_in_flight == 0 || config.max_in_flight > MAX_IN_FLIGHT {
        return Err(BaseServiceError::new(
            BaseErrorCodeV1::InvalidRequest,
            "runtime_worker_bound_invalid",
        ));
    }
    if config.version_status.compatibility != config.compatibility_policy.current
        || config.version_status.candidate_semantic_digest
            != config
                .compatibility_policy
                .current
                .candidate_semantic_digest()
        || config.version_status.artifact_tuple_digest
            != config.compatibility_policy.current.artifact_tuple_digest()
        || config
            .compatibility_policy
            .to_archive_restore_policy()
            .is_err()
    {
        return Err(BaseServiceError::new(
            BaseErrorCodeV1::IncompatibleProfile,
            "runtime_compatibility_status_mismatch",
        ));
    }
    Ok(())
}

fn command_kind(command: &BaseCommandV1) -> BaseOperationKindV1 {
    match command {
        BaseCommandV1::ExistingLocalCommand(_) => BaseOperationKindV1::ExistingLocalCommand,
        BaseCommandV1::CreateArchive(_) => BaseOperationKindV1::CreateArchive,
        BaseCommandV1::RestoreArchive(_) => BaseOperationKindV1::RestoreArchive,
    }
}

fn validate_query_budget(request: &BaseQueryRequestV1) -> Result<(), BaseServiceError> {
    validate_budget(&request.budget)?;
    if request.budget.max_items == 0
        || request.budget.max_work_units == 0
        || request.payload.as_bytes().len() as u64 > request.budget.max_bytes
    {
        return Err(resource_exhausted("query_request_budget_exhausted"));
    }
    Ok(())
}

fn validate_command_budget(command: &BaseCommandV1) -> Result<(), BaseServiceError> {
    let budget = match command {
        BaseCommandV1::ExistingLocalCommand(local) => {
            if onebrain_base_contract::ku::KuRequestV1::is_registered_kind(local.kind) {
                return Err(BaseServiceError::new(
                    BaseErrorCodeV1::InvalidRequest,
                    "ku_requires_authenticated_typed_dispatch",
                ));
            }
            return Ok(());
        }
        BaseCommandV1::CreateArchive(command) => &command.budget,
        BaseCommandV1::RestoreArchive(command) => &command.budget,
    };
    validate_budget(budget)?;
    if budget.max_items == 0 || budget.max_bytes == 0 || budget.max_work_units == 0 {
        return Err(resource_exhausted("archive_command_budget_exhausted"));
    }
    Ok(())
}

fn validate_budget(budget: &ResourceBudgetV1) -> Result<(), BaseServiceError> {
    budget.validate().map_err(|_| {
        BaseServiceError::new(BaseErrorCodeV1::InvalidRequest, "resource_budget_invalid")
    })
}

fn encode_command(command: &BaseCommandV1) -> Result<Vec<u8>, BaseServiceError> {
    let mut output = Vec::new();
    output.extend_from_slice(&command.discriminator().to_le_bytes());
    match command {
        BaseCommandV1::ExistingLocalCommand(command) => {
            output.extend_from_slice(&command.kind.to_le_bytes());
            push_bytes(&mut output, command.payload.as_bytes())?;
        }
        BaseCommandV1::CreateArchive(command) => {
            output.extend_from_slice(command.sink.as_bytes());
            output.extend_from_slice(command.secret.as_bytes());
            encode_budget(&mut output, &command.budget);
        }
        BaseCommandV1::RestoreArchive(command) => {
            output.extend_from_slice(command.source.as_bytes());
            output.extend_from_slice(command.secret.as_bytes());
            encode_budget(&mut output, &command.budget);
        }
    }
    Ok(output)
}

fn encode_budget(output: &mut Vec<u8>, budget: &onebrain_base_contract::ResourceBudgetV1) {
    output.extend_from_slice(&budget.max_items.to_le_bytes());
    output.extend_from_slice(&budget.max_bytes.to_le_bytes());
    output.extend_from_slice(&budget.max_work_units.to_le_bytes());
}

fn push_bytes(output: &mut Vec<u8>, bytes: &[u8]) -> Result<(), BaseServiceError> {
    let length = u32::try_from(bytes.len()).map_err(|_| {
        BaseServiceError::new(
            BaseErrorCodeV1::ResourceExhausted,
            "command_payload_too_large",
        )
    })?;
    output.extend_from_slice(&length.to_le_bytes());
    output.extend_from_slice(bytes);
    Ok(())
}

fn encode_restore_receipt(receipt: &DatasetRestoreReceipt) -> Vec<u8> {
    let mut bytes = Vec::with_capacity(105);
    bytes.extend_from_slice(&receipt.activation.operation_id);
    bytes.extend_from_slice(&receipt.activation.old_generation_root);
    bytes.extend_from_slice(&receipt.activation.new_generation_root);
    bytes.extend_from_slice(&receipt.activation.generation_sequence.to_le_bytes());
    bytes.push(receipt.activation.phase as u8);
    bytes
}

fn validate_managed_capability<'a>(
    authority: &'a AuthorityState,
    handle: &BaseManagementServices,
    id: [u8; 32],
) -> Result<&'a ManagedCapabilityRecord, BaseServiceError> {
    let record = authority.capabilities.get(&id).ok_or_else(not_found)?;
    validate_managed_record(record, handle)?;
    Ok(record)
}

fn validate_managed_record(
    record: &ManagedCapabilityRecord,
    handle: &BaseManagementServices,
) -> Result<(), BaseServiceError> {
    if record.management_id != handle.management_id
        || record.principal != handle.principal
        || record.process_generation != handle.process_generation
        || record.dataset_generation != handle.dataset_generation
    {
        return Err(conflict("archive_capability_binding_mismatch"));
    }
    Ok(())
}

fn capability_id(record: &ManagedCapabilityRecord) -> crate::ArchiveCapabilityId {
    match &record.value {
        ManagedCapability::SourceWriting(value) => value.id(),
        ManagedCapability::SourceSealed(value) => value.id(),
        ManagedCapability::SinkWriting(value) => value.id(),
        ManagedCapability::SinkReadable(value) => value.id(),
        ManagedCapability::Secret(value) => value.id(),
    }
}

fn revoke_archive_capability(
    registry: &ArchiveCapabilityRegistry,
    record: &ManagedCapabilityRecord,
) -> Result<(), NodeError> {
    match &record.value {
        ManagedCapability::SourceSealed(_) | ManagedCapability::SinkReadable(_) => {
            registry.destroy(capability_id(record))
        }
        ManagedCapability::SourceWriting(_)
        | ManagedCapability::SinkWriting(_)
        | ManagedCapability::Secret(_) => registry.abort(capability_id(record)),
    }
}

fn managed_capability_kind(value: &ManagedCapability) -> u16 {
    match value {
        ManagedCapability::SourceWriting(_) | ManagedCapability::SourceSealed(_) => 1,
        ManagedCapability::SinkWriting(_) | ManagedCapability::SinkReadable(_) => 2,
        ManagedCapability::Secret(_) => 3,
    }
}

fn signer_request_matches(
    request: &CompleteSignerReprovisionV1,
    requirement: &SignerReprovisionRequirement,
) -> bool {
    if request.domain.discriminator() != requirement.expected.domain().code() {
        return false;
    }
    let expected = requirement.expected.public_key();
    match &request.expected_public_id {
        SignerPublicIdV1::NodeTransport(value) => {
            request.domain.discriminator() == 1 && value.0 == expected
        }
        SignerPublicIdV1::ActorRoot(value) => {
            request.domain.discriminator() == 2 && value.0 == expected
        }
        SignerPublicIdV1::FeedAuthor(value) => {
            request.domain.discriminator() == 3 && value.0 == expected
        }
    }
}

fn scope_digest(scopes: &BTreeSet<BaseManagementScope>) -> [u8; 32] {
    let mut bytes = Vec::with_capacity(scopes.len());
    for scope in scopes {
        bytes.push(match scope {
            BaseManagementScope::ArchiveSource => 1,
            BaseManagementScope::ArchiveSink => 2,
            BaseManagementScope::ArchiveSecret => 3,
            BaseManagementScope::SignerReprovision => 4,
        });
    }
    blake3::derive_key(MANAGEMENT_SCOPE_DOMAIN, &bytes)
}

fn unique_id<T>(map: &BTreeMap<[u8; 32], T>) -> Result<[u8; 32], BaseServiceError> {
    for _ in 0..32 {
        let mut id = [0; 32];
        getrandom::fill(&mut id).map_err(|_| {
            BaseServiceError::new(BaseErrorCodeV1::InternalError, "os_entropy_unavailable")
        })?;
        if id != [0; 32] && !map.contains_key(&id) {
            return Ok(id);
        }
    }
    Err(BaseServiceError::new(
        BaseErrorCodeV1::InternalError,
        "random_identifier_collision",
    ))
}

fn now_seconds() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs()
}

fn store_error(error: BaseOperationStoreError) -> BaseServiceError {
    match error {
        BaseOperationStoreError::NotFound => not_found(),
        BaseOperationStoreError::Conflict | BaseOperationStoreError::StaleGeneration => {
            conflict("operation_binding_conflict")
        }
        BaseOperationStoreError::UnknownOutcome => {
            BaseServiceError::new(BaseErrorCodeV1::UnknownOutcome, "operation_outcome_unknown")
        }
        BaseOperationStoreError::Capacity => BaseServiceError::new(
            BaseErrorCodeV1::ResourceExhausted,
            "operation_store_capacity_exhausted",
        ),
        BaseOperationStoreError::EntropyUnavailable => {
            BaseServiceError::new(BaseErrorCodeV1::InternalError, "os_entropy_unavailable")
        }
        BaseOperationStoreError::CorruptState | BaseOperationStoreError::Io(_) => corrupt_error(),
    }
}

fn node_error(_error: NodeError) -> BaseServiceError {
    BaseServiceError::new(
        BaseErrorCodeV1::DependencyUnavailable,
        "base_dependency_failed",
    )
}

fn conflict(reason: &'static str) -> BaseServiceError {
    BaseServiceError::new(BaseErrorCodeV1::Conflict, reason)
}

fn resource_exhausted(reason: &'static str) -> BaseServiceError {
    BaseServiceError::new(BaseErrorCodeV1::ResourceExhausted, reason)
}

fn not_found() -> BaseServiceError {
    BaseServiceError::new(BaseErrorCodeV1::NotFound, "base_resource_not_found")
}

fn corrupt_error() -> BaseServiceError {
    BaseServiceError::new(BaseErrorCodeV1::CorruptState, "base_durable_state_corrupt")
}

fn internal_error() -> BaseServiceError {
    BaseServiceError::new(BaseErrorCodeV1::InternalError, "base_internal_error")
}

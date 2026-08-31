//! Stable C projection of the product-neutral Base v1 facade.
//!
//! C-visible pointers are opaque, monotonically allocated tokens. They are
//! never dereferenced, never expose a Rust owner, and are fenced by the exact
//! process/dataset generations captured when the host registration is opened.

#![deny(unsafe_op_in_unsafe_fn)]

use std::collections::HashMap;
use std::panic::{catch_unwind, AssertUnwindSafe};
use std::sync::atomic::{AtomicU64, AtomicUsize, Ordering};
use std::sync::{Mutex, OnceLock};

use onebrain_base_contract::{
    ArchiveCapabilityHandleV1, ArchiveChunkV1, ArchiveCredentialKindV1, ArchiveSinkBeginV1,
    ArchiveSinkHandleV1, ArchiveSinkReadV1, ArchiveSourceBeginV1, ArchiveSourceHandleV1,
    ArchiveSourcePushV1, BaseCapabilityRequirements, BaseCapabilitySet, BaseCommandV1,
    BaseConfirmRequestV1, BaseIdempotencyKey, BaseLocalCommandV1, BaseManagementRequestV1,
    BaseOperationId, BaseOperationKindV1, BasePollEventsRequestV1, BasePrepareRequestV1,
    BaseQueryRequestV1, BaseRequestV1, BaseSubscriptionId, BaseSubscriptionRequestV1,
    BoundedSecretIngressV1, CompleteSignerReprovisionV1, ResourceBudgetV1, SignerDomainV1,
    SignerProvisionHandleV1, SignerPublicIdV1, TopicKindV1, TypedPayloadV1,
};
use onebrain_node::{
    BaseManagementGrant, BaseManagementResponseV1, BaseManagementServices, BaseResponseV1,
    BaseServiceError, BaseServices,
};

pub const OB_BASE_ABI_MAJOR_V1: u16 = 1;
pub const OB_BASE_ABI_MINOR_V1: u16 = 0;
pub const OB_BASE_OK_V1: u16 = 0;
/// SHA-256 of the canonical field-width/bound/ownership/discriminator and
/// lifecycle descriptor derived from the frozen Base v1 machine IDL.
pub const OB_BASE_IDL_DESCRIPTOR_SHA256_V1: [u8; 32] = [
    0x0f, 0xb0, 0x33, 0x10, 0xa9, 0x6d, 0x92, 0x60, 0x65, 0x02, 0x67, 0x44, 0x5e, 0x20, 0xd3, 0x57,
    0xc6, 0xf7, 0xe7, 0x82, 0xc1, 0x37, 0x5a, 0x8d, 0x7b, 0xd8, 0xa8, 0x7d, 0x99, 0xa4, 0x28, 0x60,
];
const MAX_C_PAYLOAD: usize = 1_048_576;

/// Opaque ordinary Base handle. Values are tokens and are never dereferenced.
pub enum ObBaseRuntimeV1 {}

/// Opaque scoped-management handle. Values are tokens and are never dereferenced.
pub enum ObBaseManagementV1 {}

#[repr(C)]
#[derive(Clone, Copy)]
pub struct ObBaseOpenRequestV1 {
    pub struct_size: u32,
    pub abi_major: u16,
    pub abi_minor: u16,
    pub registration_token: [u8; 32],
    pub host_trust_digest: [u8; 32],
}

#[repr(C)]
#[derive(Clone, Copy)]
pub struct ObBaseCallV1 {
    pub struct_size: u32,
    pub abi_major: u16,
    pub abi_minor: u16,
    pub process_generation: [u8; 32],
    pub dataset_generation: [u8; 32],
    pub request_id: [u8; 32],
    pub operation_id: [u8; 32],
    pub auxiliary_id: [u8; 32],
    pub discriminator: u16,
    pub flags: u16,
    pub value0: u64,
    pub value1: u64,
    pub payload_ptr: *const u8,
    pub payload_len: usize,
}

#[repr(C)]
#[derive(Clone, Copy)]
pub struct ObBaseOutputV1 {
    pub struct_size: u32,
    pub abi_major: u16,
    pub abi_minor: u16,
    pub process_generation: [u8; 32],
    pub dataset_generation: [u8; 32],
    pub response_discriminator: u16,
    pub reserved: u16,
    pub operation_id: [u8; 32],
    pub buffer_ptr: *mut u8,
    pub buffer_capacity: usize,
    pub required_len: usize,
    pub written_len: usize,
}

#[repr(C)]
#[derive(Clone, Copy)]
pub struct ObBaseErrorV1 {
    pub struct_size: u32,
    pub abi_major: u16,
    pub abi_minor: u16,
    pub code: u16,
    pub retryable: u8,
    pub reconcile_before_retry: u8,
    pub reserved: u16,
    pub message_ptr: *const u8,
    pub message_len: usize,
    pub allocation_tag: u64,
}

#[repr(C)]
#[derive(Clone, Copy)]
pub struct ObBaseOwnedBufferV1 {
    pub struct_size: u32,
    pub abi_major: u16,
    pub abi_minor: u16,
    pub ptr: *const u8,
    pub len: usize,
    pub allocation_tag: u64,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct BaseAbiRegistration {
    pub token: [u8; 32],
    pub host_trust_digest: [u8; 32],
}

struct PendingRegistration {
    services: BaseServices,
    trust_digest: [u8; 32],
}

#[derive(Clone)]
struct RuntimeRecord {
    services: BaseServices,
    process_generation: [u8; 32],
    dataset_generation: [u8; 32],
    closed: bool,
}

#[derive(Clone)]
struct ManagementRecord {
    services: Option<BaseManagementServices>,
    runtime_key: usize,
    process_generation: [u8; 32],
    dataset_generation: [u8; 32],
    closed: bool,
}

struct PendingGrant {
    runtime_key: usize,
    grant: BaseManagementGrant,
}

struct OwnedAllocation {
    bytes: Box<[u8]>,
}

static REGISTRATIONS: OnceLock<Mutex<HashMap<[u8; 32], PendingRegistration>>> = OnceLock::new();
static RUNTIMES: OnceLock<Mutex<HashMap<usize, RuntimeRecord>>> = OnceLock::new();
static MANAGEMENT: OnceLock<Mutex<HashMap<usize, ManagementRecord>>> = OnceLock::new();
static GRANTS: OnceLock<Mutex<HashMap<[u8; 32], PendingGrant>>> = OnceLock::new();
static ALLOCATIONS: OnceLock<Mutex<HashMap<u64, OwnedAllocation>>> = OnceLock::new();
static NEXT_HANDLE: AtomicUsize = AtomicUsize::new(1);
static NEXT_ALLOCATION: AtomicU64 = AtomicU64::new(1);

fn registrations() -> &'static Mutex<HashMap<[u8; 32], PendingRegistration>> {
    REGISTRATIONS.get_or_init(|| Mutex::new(HashMap::new()))
}

fn runtimes() -> &'static Mutex<HashMap<usize, RuntimeRecord>> {
    RUNTIMES.get_or_init(|| Mutex::new(HashMap::new()))
}

fn management() -> &'static Mutex<HashMap<usize, ManagementRecord>> {
    MANAGEMENT.get_or_init(|| Mutex::new(HashMap::new()))
}

fn grants() -> &'static Mutex<HashMap<[u8; 32], PendingGrant>> {
    GRANTS.get_or_init(|| Mutex::new(HashMap::new()))
}

fn allocations() -> &'static Mutex<HashMap<u64, OwnedAllocation>> {
    ALLOCATIONS.get_or_init(|| Mutex::new(HashMap::new()))
}

/// Rust-host registration boundary. Native code receives only the returned
/// immutable token and trust digest, never a `BaseServices` pointer.
pub fn register_base_services_for_abi(
    services: BaseServices,
    immutable_host_trust: &[u8],
) -> Result<BaseAbiRegistration, &'static str> {
    if immutable_host_trust.is_empty() || immutable_host_trust.len() > 4096 {
        return Err("invalid_host_trust_configuration");
    }
    let trust_digest = blake3_digest(b"onebrain:base-abi:host-trust:1\0", immutable_host_trust);
    let token = unique_random_token(registrations())?;
    registrations()
        .lock()
        .map_err(|_| "internal_error")?
        .insert(
            token,
            PendingRegistration {
                services,
                trust_digest,
            },
        );
    Ok(BaseAbiRegistration {
        token,
        host_trust_digest: trust_digest,
    })
}

/// Registers one already authenticated, random, single-use host grant for a C
/// management-open call. Grant issuance deliberately remains outside the ABI.
pub fn register_management_grant_for_abi(
    runtime: *mut ObBaseRuntimeV1,
    grant: BaseManagementGrant,
) -> Result<[u8; 32], &'static str> {
    let runtime_key = opaque_key(runtime)?;
    let runtime_valid = runtimes()
        .lock()
        .map_err(|_| "internal_error")?
        .get(&runtime_key)
        .is_some_and(|record| !record.closed);
    if !runtime_valid {
        return Err("stale_runtime_handle");
    }
    let token = unique_random_token(grants())?;
    grants()
        .lock()
        .map_err(|_| "internal_error")?
        .insert(token, PendingGrant { runtime_key, grant });
    Ok(token)
}

fn unique_random_token<T>(
    registry: &Mutex<HashMap<[u8; 32], T>>,
) -> Result<[u8; 32], &'static str> {
    for _ in 0..8 {
        let mut token = [0; 32];
        getrandom::fill(&mut token).map_err(|_| "entropy_unavailable")?;
        if token != [0; 32]
            && !registry
                .lock()
                .map_err(|_| "internal_error")?
                .contains_key(&token)
        {
            return Ok(token);
        }
    }
    Err("entropy_collision_budget_exhausted")
}

#[derive(Debug)]
struct AbiFailure {
    code: u16,
    reason: &'static str,
}

impl AbiFailure {
    const fn invalid(reason: &'static str) -> Self {
        Self { code: 1, reason }
    }

    const fn conflict(reason: &'static str) -> Self {
        Self { code: 3, reason }
    }

    const fn exhausted(reason: &'static str) -> Self {
        Self { code: 9, reason }
    }

    const fn internal(reason: &'static str) -> Self {
        Self { code: 13, reason }
    }
}

impl From<BaseServiceError> for AbiFailure {
    fn from(error: BaseServiceError) -> Self {
        Self {
            code: error.code.discriminator(),
            reason: error.reason,
        }
    }
}

#[derive(Clone, Debug)]
struct OwnedCall {
    process_generation: [u8; 32],
    dataset_generation: [u8; 32],
    request_id: [u8; 32],
    operation_id: [u8; 32],
    auxiliary_id: [u8; 32],
    discriminator: u16,
    flags: u16,
    value0: u64,
    value1: u64,
    payload: Vec<u8>,
}

fn ffi_entry(
    error: *mut ObBaseErrorV1,
    family: &'static str,
    body: impl FnOnce() -> Result<(), AbiFailure>,
) -> u16 {
    let result = catch_unwind(AssertUnwindSafe(|| {
        if std::env::var("ONEBRAIN_BASE_ABI_FAILPOINT").ok().as_deref() == Some(family) {
            panic!("Base ABI failpoint: {family}");
        }
        body()
    }));
    match result {
        Ok(Ok(())) => {
            clear_error(error);
            OB_BASE_OK_V1
        }
        Ok(Err(failure)) => {
            write_error(error, failure.code, failure.reason);
            failure.code
        }
        Err(_) => {
            write_error(error, 13, "panic_caught_at_ffi_boundary");
            13
        }
    }
}

fn clear_error(error: *mut ObBaseErrorV1) {
    if validate_public_struct(error.cast_const(), std::mem::size_of::<ObBaseErrorV1>()).is_err() {
        return;
    }
    // SAFETY: the size guard above proves the caller supplied the full v1 prefix.
    let output = unsafe { &mut *error };
    output.code = 0;
    output.retryable = 0;
    output.reconcile_before_retry = 0;
    output.message_ptr = std::ptr::null();
    output.message_len = 0;
    output.allocation_tag = 0;
}

fn write_error(error: *mut ObBaseErrorV1, code: u16, reason: &'static str) {
    if validate_public_struct(error.cast_const(), std::mem::size_of::<ObBaseErrorV1>()).is_err() {
        return;
    }
    let (pointer, length, tag) =
        allocate_owned(reason.as_bytes()).unwrap_or((std::ptr::null(), 0, 0));
    // SAFETY: the size guard above proves the caller supplied the full v1 prefix.
    let output = unsafe { &mut *error };
    output.code = code;
    output.retryable = u8::from(matches!(code, 5 | 7 | 9 | 12));
    output.reconcile_before_retry = u8::from(matches!(code, 5 | 7 | 9 | 12 | 13));
    output.message_ptr = pointer;
    output.message_len = length;
    output.allocation_tag = tag;
}

fn allocate_owned(bytes: &[u8]) -> Result<(*const u8, usize, u64), AbiFailure> {
    let boxed = bytes.to_vec().into_boxed_slice();
    let pointer = boxed.as_ptr();
    let length = boxed.len();
    let tag = NEXT_ALLOCATION.fetch_add(1, Ordering::Relaxed);
    if tag == 0 {
        return Err(AbiFailure::internal("allocation_tag_exhausted"));
    }
    allocations()
        .lock()
        .map_err(|_| AbiFailure::internal("allocation_registry_poisoned"))?
        .insert(tag, OwnedAllocation { bytes: boxed });
    Ok((pointer, length, tag))
}

fn validate_public_struct<T>(pointer: *const T, required: usize) -> Result<(), AbiFailure> {
    if pointer.is_null() {
        return Err(AbiFailure::invalid("null_struct_pointer"));
    }
    // SAFETY: reading the first u32 is the minimum public-struct contract.
    let supplied = unsafe { std::ptr::read_unaligned(pointer.cast::<u32>()) } as usize;
    if supplied < required {
        return Err(AbiFailure::invalid("undersized_struct"));
    }
    // SAFETY: fields are immediately after struct_size in every public ABI struct.
    let major = unsafe { std::ptr::read_unaligned(pointer.cast::<u8>().add(4).cast::<u16>()) };
    if major != OB_BASE_ABI_MAJOR_V1 {
        return Err(AbiFailure::invalid("abi_major_mismatch"));
    }
    Ok(())
}

fn read_call(input: *const ObBaseCallV1) -> Result<OwnedCall, AbiFailure> {
    validate_public_struct(input, std::mem::size_of::<ObBaseCallV1>())?;
    // SAFETY: full v1 prefix was size-checked and C ABI requires natural alignment.
    let input = unsafe { &*input };
    if input.payload_len > MAX_C_PAYLOAD {
        return Err(AbiFailure::invalid("payload_bound_exceeded"));
    }
    if input.payload_ptr.is_null() != (input.payload_len == 0) {
        return Err(AbiFailure::invalid("null_length_mismatch"));
    }
    let payload = if input.payload_len == 0 {
        Vec::new()
    } else {
        // SAFETY: non-null plus explicit bounded length is the caller contract.
        unsafe { std::slice::from_raw_parts(input.payload_ptr, input.payload_len) }.to_vec()
    };
    Ok(OwnedCall {
        process_generation: input.process_generation,
        dataset_generation: input.dataset_generation,
        request_id: input.request_id,
        operation_id: input.operation_id,
        auxiliary_id: input.auxiliary_id,
        discriminator: input.discriminator,
        flags: input.flags,
        value0: input.value0,
        value1: input.value1,
        payload,
    })
}

fn write_output(
    output: *mut ObBaseOutputV1,
    runtime: &RuntimeRecord,
    discriminator: u16,
    operation_id: [u8; 32],
    payload: &[u8],
) -> Result<(), AbiFailure> {
    validate_public_struct(output.cast_const(), std::mem::size_of::<ObBaseOutputV1>())?;
    // SAFETY: the complete output prefix was size-checked.
    let output = unsafe { &mut *output };
    output.process_generation = runtime.process_generation;
    output.dataset_generation = runtime.dataset_generation;
    output.response_discriminator = discriminator;
    output.operation_id = operation_id;
    output.required_len = payload.len();
    output.written_len = 0;
    if output.buffer_ptr.is_null() != (output.buffer_capacity == 0) {
        return Err(AbiFailure::invalid("output_null_length_mismatch"));
    }
    if output.buffer_capacity < payload.len() {
        return Err(AbiFailure::exhausted("output_too_small"));
    }
    if !payload.is_empty() {
        // SAFETY: the caller advertised at least payload.len() writable bytes.
        unsafe {
            std::ptr::copy_nonoverlapping(payload.as_ptr(), output.buffer_ptr, payload.len())
        };
    }
    output.written_len = payload.len();
    Ok(())
}

fn runtime_for_call(
    handle: *mut ObBaseRuntimeV1,
    call: &OwnedCall,
) -> Result<(usize, RuntimeRecord), AbiFailure> {
    let key = opaque_key(handle).map_err(AbiFailure::invalid)?;
    let record = runtimes()
        .lock()
        .map_err(|_| AbiFailure::internal("runtime_registry_poisoned"))?
        .get(&key)
        .cloned()
        .ok_or_else(|| AbiFailure::conflict("unknown_runtime_handle"))?;
    if record.closed {
        return Err(AbiFailure::conflict("closed_runtime_handle"));
    }
    if call.process_generation != record.process_generation
        || call.dataset_generation != record.dataset_generation
    {
        return Err(AbiFailure::conflict("stale_generation"));
    }
    Ok((key, record))
}

fn management_for_call(
    handle: *mut ObBaseManagementV1,
    call: &OwnedCall,
) -> Result<(usize, ManagementRecord, RuntimeRecord), AbiFailure> {
    let key = opaque_key(handle).map_err(AbiFailure::invalid)?;
    let record = management()
        .lock()
        .map_err(|_| AbiFailure::internal("management_registry_poisoned"))?
        .get(&key)
        .cloned()
        .ok_or_else(|| AbiFailure::conflict("unknown_management_handle"))?;
    if record.closed || record.services.is_none() {
        return Err(AbiFailure::conflict("closed_management_handle"));
    }
    if call.process_generation != record.process_generation
        || call.dataset_generation != record.dataset_generation
    {
        return Err(AbiFailure::conflict("stale_generation"));
    }
    let runtime = runtimes()
        .lock()
        .map_err(|_| AbiFailure::internal("runtime_registry_poisoned"))?
        .get(&record.runtime_key)
        .cloned()
        .ok_or_else(|| AbiFailure::conflict("unknown_runtime_handle"))?;
    if runtime.closed {
        return Err(AbiFailure::conflict("closed_runtime_handle"));
    }
    Ok((key, record, runtime))
}

fn opaque_key<T>(handle: *mut T) -> Result<usize, &'static str> {
    let key = handle as usize;
    if key == 0 || key & 1 == 0 {
        return Err("invalid_opaque_handle");
    }
    Ok(key)
}

fn new_opaque<T>() -> Result<*mut T, AbiFailure> {
    let sequence = NEXT_HANDLE.fetch_add(1, Ordering::Relaxed);
    let key = sequence
        .checked_mul(2)
        .and_then(|value| value.checked_add(1))
        .ok_or_else(|| AbiFailure::internal("handle_space_exhausted"))?;
    Ok(key as *mut T)
}

fn block_on<T: Send + 'static>(
    future: impl std::future::Future<Output = T> + Send + 'static,
) -> Result<T, AbiFailure> {
    std::thread::Builder::new()
        .name("onebrain-base-abi".into())
        .spawn(move || {
            tokio::runtime::Builder::new_current_thread()
                .enable_time()
                .build()
                .map_err(|_| AbiFailure::internal("async_runtime_unavailable"))?
                .block_on(async { Ok::<T, AbiFailure>(future.await) })
        })
        .map_err(|_| AbiFailure::internal("async_worker_unavailable"))?
        .join()
        .map_err(|_| AbiFailure::internal("async_worker_panicked"))?
}

fn blake3_digest(domain: &[u8], bytes: &[u8]) -> [u8; 32] {
    let mut hasher = blake3::Hasher::new();
    hasher.update(domain);
    hasher.update(&(bytes.len() as u64).to_le_bytes());
    hasher.update(bytes);
    *hasher.finalize().as_bytes()
}

fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|byte| format!("{byte:02x}")).collect()
}

fn status_json(status: &onebrain_node::BaseStatusV1) -> Vec<u8> {
    use onebrain_base_contract::{SourceCommitId, SourceCommitIdentity, ToolchainIdentity};

    let tuple = &status.version.compatibility;
    let source_commit = match tuple.base_commit {
        SourceCommitIdentity::Known(SourceCommitId::Sha1(value)) => {
            serde_json::json!({ "kind": "sha1", "digest": hex(&value.0) })
        }
        SourceCommitIdentity::Known(SourceCommitId::Sha256(value)) => {
            serde_json::json!({ "kind": "sha256", "digest": hex(&value.0) })
        }
        SourceCommitIdentity::Unknown => serde_json::json!({ "kind": "unknown" }),
    };
    let toolchain = match tuple.toolchain {
        ToolchainIdentity::Known(value) => {
            serde_json::json!({ "kind": "known", "digest": hex(&value.0) })
        }
        ToolchainIdentity::Unknown => serde_json::json!({ "kind": "unknown" }),
    };
    serde_json::to_vec(&serde_json::json!({
        "profile_major": onebrain_base_contract::BASE_RUNTIME_PROFILE_MAJOR,
        "profile_minor": onebrain_base_contract::BASE_RUNTIME_PROFILE_MINOR,
        "process_generation": hex(status.process_generation.as_bytes()),
        "dataset_generation": hex(&status.dataset_generation.0),
        "lifecycle": status.lifecycle as u8,
        "candidate_semantic_digest": hex(&status.version.candidate_semantic_digest.0),
        "artifact_tuple_digest": hex(&status.version.artifact_tuple_digest.0),
        "qualification": status.version.qualification.discriminator(),
        "compatibility": {
            "base_version": {
                "major": tuple.base_version.major,
                "minor": tuple.base_version.minor,
                "patch": tuple.base_version.patch,
                "prerelease": tuple.base_version.prerelease.as_ref().map(|value| value.as_str()),
            },
            "base_commit": source_commit,
            "canonical_schema_digest": hex(&tuple.canonical_schema_digest.0),
            "domain_registry_digest": hex(&tuple.domain_registry_digest.0),
            "resource_registry_digest": hex(&tuple.resource_registry_digest.0),
            "storage_schema": tuple.storage_schema.0,
            "archive_profile": { "major": tuple.archive_profile.major, "minor": tuple.archive_profile.minor },
            "migration_profile": { "major": tuple.migration_profile.major, "minor": tuple.migration_profile.minor },
            "registry_profile": { "major": tuple.registry_profile.major, "minor": tuple.registry_profile.minor },
            "registry_profile_digest": hex(&tuple.registry_profile_digest.0),
            "wire_session": { "major": tuple.wire_session.major, "minor": tuple.wire_session.minor },
            "product_api": { "major": tuple.product_api.major, "minor": tuple.product_api.minor },
            "c_abi": { "major": tuple.c_abi.major, "minor": tuple.c_abi.minor },
            "feature_set_digest": hex(&tuple.feature_set_digest.0),
            "target_triple": tuple.target_triple.as_str(),
            "toolchain": toolchain,
        },
        "network_compiled": status.network_compiled,
        "network_requested": false,
        "network_active": status.network_enabled,
        "network_enabled": status.network_enabled,
        "local_usable": status.local_usable,
        "limitations": status.limitations,
    }))
    .expect("bounded status JSON")
}

fn empty_capabilities() -> Result<BaseCapabilityRequirements, AbiFailure> {
    let empty = BaseCapabilitySet::try_from_discriminators(Vec::new())
        .map_err(|_| AbiFailure::internal("empty_capability_set_failed"))?;
    Ok(BaseCapabilityRequirements {
        supported: empty.clone(),
        required: empty,
    })
}

#[derive(Clone, Copy)]
enum OrdinaryOperation {
    Capabilities,
    Negotiate,
    Status,
    Snapshot,
    Query,
    Reserve,
    Prepare,
    Confirm,
    Cancel,
    Reconcile,
    Subscribe,
    Poll,
    CloseSubscription,
    Drain,
    Close,
}

fn ordinary_entry(
    handle: *mut ObBaseRuntimeV1,
    input: *const ObBaseCallV1,
    output: *mut ObBaseOutputV1,
    error: *mut ObBaseErrorV1,
    family: &'static str,
    operation: OrdinaryOperation,
) -> u16 {
    ffi_entry(error, family, || {
        let call = read_call(input)?;
        let (runtime_key, runtime) = runtime_for_call(handle, &call)?;
        let services = runtime.services.clone();
        let (discriminator, operation_id, payload) = match operation {
            OrdinaryOperation::Capabilities => {
                let status = services.snapshot().map_err(AbiFailure::from)?;
                let payload = serde_json::to_vec(&serde_json::json!({
                    "base_v1": true,
                    "legacy_read_compat_compiled": cfg!(feature = "legacy-read-compat"),
                    "network_compiled": status.network_compiled,
                    "network_requested": false,
                    "network_active": status.network_enabled,
                }))
                .map_err(|_| AbiFailure::internal("response_encoding_failed"))?;
                (3, [0; 32], payload)
            }
            OrdinaryOperation::Negotiate => {
                let status = services.snapshot().map_err(AbiFailure::from)?;
                let request = onebrain_node::BaseNegotiationRequest {
                    peer: status.version.compatibility,
                    peer_capabilities: empty_capabilities()?,
                    verified_migration: None,
                };
                let outcome = services.negotiate(request).map_err(AbiFailure::from)?;
                let payload = serde_json::to_vec(&serde_json::json!({
                    "outcome": outcome.discriminator()
                }))
                .map_err(|_| AbiFailure::internal("response_encoding_failed"))?;
                (2, [0; 32], payload)
            }
            OrdinaryOperation::Status | OrdinaryOperation::Snapshot => {
                let status = services.snapshot().map_err(AbiFailure::from)?;
                let discriminator = if matches!(operation, OrdinaryOperation::Snapshot) {
                    4
                } else {
                    3
                };
                (discriminator, [0; 32], status_json(&status))
            }
            OrdinaryOperation::Query => {
                let payload = TypedPayloadV1::try_from_bytes(call.payload)
                    .map_err(|_| AbiFailure::invalid("invalid_query_payload"))?;
                let budget = ResourceBudgetV1::try_new(
                    u32::try_from(call.value0)
                        .map_err(|_| AbiFailure::invalid("invalid_item_budget"))?,
                    call.value1,
                    1_000_000,
                )
                .map_err(|_| AbiFailure::invalid("invalid_query_budget"))?;
                response_parts(block_on(async move {
                    services
                        .invoke(BaseRequestV1::Query(BaseQueryRequestV1 {
                            payload,
                            continuation: None,
                            budget,
                        }))
                        .await
                })??)?
            }
            OrdinaryOperation::Reserve => {
                let kind = operation_kind(call.discriminator)?;
                response_parts(block_on(async move {
                    services.invoke(BaseRequestV1::ReserveOperation(kind)).await
                })??)?
            }
            OrdinaryOperation::Prepare => {
                let command = match call.discriminator {
                    1 => BaseCommandV1::ExistingLocalCommand(BaseLocalCommandV1 {
                        kind: call.flags,
                        payload: TypedPayloadV1::try_from_bytes(call.payload)
                            .map_err(|_| AbiFailure::invalid("invalid_command_payload"))?,
                    }),
                    2 => BaseCommandV1::CreateArchive(
                        onebrain_base_contract::CreateArchiveCommandV1 {
                            sink: ArchiveSinkHandleV1::from_opaque_bytes(call.auxiliary_id),
                            secret:
                                onebrain_base_contract::ArchiveSecretHandleV1::from_opaque_bytes(
                                    call.operation_id,
                                ),
                            budget: ResourceBudgetV1::try_new(1, call.value0, call.value1)
                                .map_err(|_| AbiFailure::invalid("invalid_archive_budget"))?,
                        },
                    ),
                    3 => BaseCommandV1::RestoreArchive(
                        onebrain_base_contract::RestoreArchiveCommandV1 {
                            source: ArchiveSourceHandleV1::from_opaque_bytes(call.auxiliary_id),
                            secret:
                                onebrain_base_contract::ArchiveSecretHandleV1::from_opaque_bytes(
                                    call.operation_id,
                                ),
                            budget: ResourceBudgetV1::try_new(1, call.value0, call.value1)
                                .map_err(|_| AbiFailure::invalid("invalid_archive_budget"))?,
                        },
                    ),
                    _ => return Err(AbiFailure::invalid("unknown_command_discriminator")),
                };
                let request = BasePrepareRequestV1 {
                    reservation_id: onebrain_base_contract::BaseOperationReservationId(
                        call.request_id,
                    ),
                    command,
                };
                response_parts(block_on(async move {
                    services.invoke(BaseRequestV1::Prepare(request)).await
                })??)?
            }
            OrdinaryOperation::Confirm => {
                let request = BaseConfirmRequestV1 {
                    operation_id: BaseOperationId(call.operation_id),
                    idempotency_key: BaseIdempotencyKey(call.auxiliary_id),
                };
                response_parts(block_on(async move {
                    services.invoke(BaseRequestV1::Confirm(request)).await
                })??)?
            }
            OrdinaryOperation::Cancel | OrdinaryOperation::Reconcile => {
                let request = if matches!(operation, OrdinaryOperation::Cancel) {
                    BaseRequestV1::Cancel(BaseOperationId(call.operation_id))
                } else {
                    BaseRequestV1::Reconcile(BaseOperationId(call.operation_id))
                };
                response_parts(block_on(async move { services.invoke(request).await })??)?
            }
            OrdinaryOperation::Subscribe => {
                let topic = topic_kind(call.discriminator)?;
                let cursor = (call.value0 != u64::MAX).then_some(call.value0);
                response_parts(block_on(async move {
                    services
                        .invoke(BaseRequestV1::Subscribe(BaseSubscriptionRequestV1 {
                            topic,
                            cursor,
                        }))
                        .await
                })??)?
            }
            OrdinaryOperation::Poll => {
                let max_items = u32::try_from(call.value1)
                    .map_err(|_| AbiFailure::invalid("invalid_poll_limit"))?;
                let request = BasePollEventsRequestV1 {
                    subscription_id: BaseSubscriptionId::from_opaque_bytes(call.operation_id),
                    after_cursor: call.value0,
                    max_items,
                };
                response_parts(block_on(async move {
                    services.invoke(BaseRequestV1::PollEvents(request)).await
                })??)?
            }
            OrdinaryOperation::CloseSubscription => response_parts(block_on(async move {
                services
                    .invoke(BaseRequestV1::CloseSubscription(
                        BaseSubscriptionId::from_opaque_bytes(call.operation_id),
                    ))
                    .await
            })??)?,
            OrdinaryOperation::Drain => response_parts(block_on(async move {
                services.invoke(BaseRequestV1::Drain).await
            })??)?,
            OrdinaryOperation::Close => {
                let parts = response_parts(block_on(async move {
                    services.invoke(BaseRequestV1::Close).await
                })??)?;
                if let Some(record) = runtimes()
                    .lock()
                    .map_err(|_| AbiFailure::internal("runtime_registry_poisoned"))?
                    .get_mut(&runtime_key)
                {
                    record.closed = true;
                }
                parts
            }
        };
        write_output(output, &runtime, discriminator, operation_id, &payload)
    })
}

fn operation_kind(value: u16) -> Result<BaseOperationKindV1, AbiFailure> {
    match value {
        1 => Ok(BaseOperationKindV1::ExistingLocalCommand),
        2 => Ok(BaseOperationKindV1::CreateArchive),
        3 => Ok(BaseOperationKindV1::RestoreArchive),
        _ => Err(AbiFailure::invalid("unknown_operation_kind")),
    }
}

fn topic_kind(value: u16) -> Result<TopicKindV1, AbiFailure> {
    match value {
        1 => Ok(TopicKindV1::RuntimeStatus),
        2 => Ok(TopicKindV1::OperationReceipts),
        3 => Ok(TopicKindV1::QueryResults),
        4 => Ok(TopicKindV1::ArchiveProgress),
        5 => Ok(TopicKindV1::Compatibility),
        _ => Err(AbiFailure::invalid("unknown_topic_kind")),
    }
}

fn response_parts(response: BaseResponseV1) -> Result<(u16, [u8; 32], Vec<u8>), AbiFailure> {
    Ok(match response {
        BaseResponseV1::Status(status) => (3, [0; 32], status_json(&status)),
        BaseResponseV1::Query { payload, .. } => (5, [0; 32], payload.as_bytes().to_vec()),
        BaseResponseV1::Reserved(id) => (6, id.0, Vec::new()),
        BaseResponseV1::Prepared(intent) => (
            7,
            intent.operation_id.0,
            serde_json::to_vec(&serde_json::json!({
                "operation_id": hex(&intent.operation_id.0),
                "command_blake3": hex(&intent.command_blake3),
            }))
            .map_err(|_| AbiFailure::internal("response_encoding_failed"))?,
        ),
        BaseResponseV1::Receipt(receipt) => (
            8,
            receipt.operation_id.0,
            serde_json::to_vec(&serde_json::json!({
                "operation_id": hex(&receipt.operation_id.0),
                "state": receipt.state as u8,
                "attempts": receipt.attempts,
                "reconcile_required": receipt.reconcile_required,
                "result_blake3": receipt.result_blake3.map(|value| hex(&value)),
                "error": receipt.error.map(|value| value.discriminator()),
            }))
            .map_err(|_| AbiFailure::internal("response_encoding_failed"))?,
        ),
        BaseResponseV1::Reconciled(result) => (
            10,
            result.receipt.operation_id.0,
            serde_json::to_vec(&serde_json::json!({
                "operation_id": hex(&result.receipt.operation_id.0),
                "state": result.receipt.state as u8,
                "resumed_effect": result.resumed_effect,
                "reconcile_required": result.receipt.reconcile_required,
            }))
            .map_err(|_| AbiFailure::internal("response_encoding_failed"))?,
        ),
        BaseResponseV1::Subscription(id) => (11, *id.as_bytes(), Vec::new()),
        BaseResponseV1::Events(batch) => (
            12,
            *batch.subscription_id.as_bytes(),
            serde_json::to_vec(&serde_json::json!({
                "next_cursor": batch.next_cursor,
                "earliest_available_cursor": batch.earliest_available_cursor,
                "resync_required": batch.resync_required,
                "events": batch.events.into_iter().map(|event| serde_json::json!({
                    "cursor": event.cursor,
                    "topic": event.topic.discriminator(),
                    "operation_id": event.operation_id.map(|id| hex(&id.0)),
                    "payload": hex(&event.payload),
                })).collect::<Vec<_>>()
            }))
            .map_err(|_| AbiFailure::internal("response_encoding_failed"))?,
        ),
        BaseResponseV1::SubscriptionClosed => (13, [0; 32], Vec::new()),
        BaseResponseV1::Drain(receipt) => (
            14,
            [0; 32],
            serde_json::to_vec(&serde_json::json!({
                "lifecycle": receipt.lifecycle as u8
            }))
            .map_err(|_| AbiFailure::internal("response_encoding_failed"))?,
        ),
        BaseResponseV1::Close(receipt) => (
            15,
            [0; 32],
            serde_json::to_vec(&serde_json::json!({
                "lifecycle": receipt.lifecycle as u8
            }))
            .map_err(|_| AbiFailure::internal("response_encoding_failed"))?,
        ),
    })
}

#[no_mangle]
/// Open one host-registered Base service through an opaque C token.
///
/// # Safety
/// `request`, `out_handle`, `output`, and `error` must point to caller-owned
/// storage that is valid for the full call and satisfies each public struct's
/// advertised `struct_size`. Any non-null buffer pointer must cover its stated
/// length/capacity.
pub unsafe extern "C" fn ob_base_open_v1(
    request: *const ObBaseOpenRequestV1,
    out_handle: *mut *mut ObBaseRuntimeV1,
    output: *mut ObBaseOutputV1,
    error: *mut ObBaseErrorV1,
) -> u16 {
    ffi_entry(error, "open", || {
        validate_public_struct(request, std::mem::size_of::<ObBaseOpenRequestV1>())?;
        if out_handle.is_null() {
            return Err(AbiFailure::invalid("null_handle_output"));
        }
        // SAFETY: full request prefix was size-checked.
        let request = unsafe { &*request };
        let registration = registrations()
            .lock()
            .map_err(|_| AbiFailure::internal("registration_registry_poisoned"))?
            .remove(&request.registration_token)
            .ok_or_else(|| AbiFailure::conflict("unknown_or_consumed_registration"))?;
        if registration.trust_digest != request.host_trust_digest {
            return Err(AbiFailure::conflict("host_trust_digest_mismatch"));
        }
        let status = registration.services.snapshot().map_err(AbiFailure::from)?;
        let record = RuntimeRecord {
            services: registration.services,
            process_generation: *status.process_generation.as_bytes(),
            dataset_generation: status.dataset_generation.0,
            closed: false,
        };
        let handle = new_opaque::<ObBaseRuntimeV1>()?;
        let key = opaque_key(handle).map_err(AbiFailure::invalid)?;
        runtimes()
            .lock()
            .map_err(|_| AbiFailure::internal("runtime_registry_poisoned"))?
            .insert(key, record.clone());
        // SAFETY: out_handle was checked non-null and is caller-owned output.
        unsafe { *out_handle = handle };
        // Open returns the generation fence in the fixed output prefix. The
        // caller obtains the variable status body through snapshot/status,
        // which supports the ordinary two-call sizing protocol.
        write_output(output, &record, 1, [0; 32], &[])
    })
}

macro_rules! ordinary_symbol {
    ($name:ident, $family:literal, $operation:expr) => {
        #[no_mangle]
        /// Invoke one ordinary Base operation through a generation-fenced
        /// opaque handle.
        ///
        /// # Safety
        /// `handle` must be a token returned by `ob_base_open_v1`; `input`,
        /// `output`, and `error` must remain valid for this call and every
        /// pointer/length pair must describe accessible caller-owned storage.
        pub unsafe extern "C" fn $name(
            handle: *mut ObBaseRuntimeV1,
            input: *const ObBaseCallV1,
            output: *mut ObBaseOutputV1,
            error: *mut ObBaseErrorV1,
        ) -> u16 {
            ordinary_entry(handle, input, output, error, $family, $operation)
        }
    };
}

ordinary_symbol!(
    ob_base_capabilities_v1,
    "status",
    OrdinaryOperation::Capabilities
);
ordinary_symbol!(
    ob_base_negotiate_v1,
    "negotiate",
    OrdinaryOperation::Negotiate
);
ordinary_symbol!(ob_base_status_v1, "status", OrdinaryOperation::Status);
ordinary_symbol!(ob_base_snapshot_v1, "status", OrdinaryOperation::Snapshot);
ordinary_symbol!(ob_base_query_v1, "query", OrdinaryOperation::Query);
ordinary_symbol!(
    ob_base_reserve_operation_v1,
    "operation",
    OrdinaryOperation::Reserve
);
ordinary_symbol!(ob_base_prepare_v1, "operation", OrdinaryOperation::Prepare);
ordinary_symbol!(ob_base_confirm_v1, "operation", OrdinaryOperation::Confirm);
ordinary_symbol!(ob_base_cancel_v1, "operation", OrdinaryOperation::Cancel);
ordinary_symbol!(
    ob_base_reconcile_v1,
    "operation",
    OrdinaryOperation::Reconcile
);
ordinary_symbol!(
    ob_base_subscribe_v1,
    "subscription",
    OrdinaryOperation::Subscribe
);
ordinary_symbol!(
    ob_base_poll_events_v1,
    "subscription",
    OrdinaryOperation::Poll
);
ordinary_symbol!(
    ob_base_close_subscription_v1,
    "subscription",
    OrdinaryOperation::CloseSubscription
);
ordinary_symbol!(ob_base_drain_v1, "lifecycle", OrdinaryOperation::Drain);
ordinary_symbol!(ob_base_close_v1, "lifecycle", OrdinaryOperation::Close);

// cbindgen intentionally does not expand `macro_rules!`. This declaration-only
// view is enabled by cbindgen's parser and compiled out by rustc; the exported
// definitions above remain the single implementation.
#[allow(unexpected_cfgs)]
#[cfg(cbindgen)]
extern "C" {
    pub fn ob_base_capabilities_v1(
        handle: *mut ObBaseRuntimeV1,
        input: *const ObBaseCallV1,
        output: *mut ObBaseOutputV1,
        error: *mut ObBaseErrorV1,
    ) -> u16;
    pub fn ob_base_negotiate_v1(
        handle: *mut ObBaseRuntimeV1,
        input: *const ObBaseCallV1,
        output: *mut ObBaseOutputV1,
        error: *mut ObBaseErrorV1,
    ) -> u16;
    pub fn ob_base_status_v1(
        handle: *mut ObBaseRuntimeV1,
        input: *const ObBaseCallV1,
        output: *mut ObBaseOutputV1,
        error: *mut ObBaseErrorV1,
    ) -> u16;
    pub fn ob_base_snapshot_v1(
        handle: *mut ObBaseRuntimeV1,
        input: *const ObBaseCallV1,
        output: *mut ObBaseOutputV1,
        error: *mut ObBaseErrorV1,
    ) -> u16;
    pub fn ob_base_query_v1(
        handle: *mut ObBaseRuntimeV1,
        input: *const ObBaseCallV1,
        output: *mut ObBaseOutputV1,
        error: *mut ObBaseErrorV1,
    ) -> u16;
    pub fn ob_base_reserve_operation_v1(
        handle: *mut ObBaseRuntimeV1,
        input: *const ObBaseCallV1,
        output: *mut ObBaseOutputV1,
        error: *mut ObBaseErrorV1,
    ) -> u16;
    pub fn ob_base_prepare_v1(
        handle: *mut ObBaseRuntimeV1,
        input: *const ObBaseCallV1,
        output: *mut ObBaseOutputV1,
        error: *mut ObBaseErrorV1,
    ) -> u16;
    pub fn ob_base_confirm_v1(
        handle: *mut ObBaseRuntimeV1,
        input: *const ObBaseCallV1,
        output: *mut ObBaseOutputV1,
        error: *mut ObBaseErrorV1,
    ) -> u16;
    pub fn ob_base_cancel_v1(
        handle: *mut ObBaseRuntimeV1,
        input: *const ObBaseCallV1,
        output: *mut ObBaseOutputV1,
        error: *mut ObBaseErrorV1,
    ) -> u16;
    pub fn ob_base_reconcile_v1(
        handle: *mut ObBaseRuntimeV1,
        input: *const ObBaseCallV1,
        output: *mut ObBaseOutputV1,
        error: *mut ObBaseErrorV1,
    ) -> u16;
    pub fn ob_base_subscribe_v1(
        handle: *mut ObBaseRuntimeV1,
        input: *const ObBaseCallV1,
        output: *mut ObBaseOutputV1,
        error: *mut ObBaseErrorV1,
    ) -> u16;
    pub fn ob_base_poll_events_v1(
        handle: *mut ObBaseRuntimeV1,
        input: *const ObBaseCallV1,
        output: *mut ObBaseOutputV1,
        error: *mut ObBaseErrorV1,
    ) -> u16;
    pub fn ob_base_close_subscription_v1(
        handle: *mut ObBaseRuntimeV1,
        input: *const ObBaseCallV1,
        output: *mut ObBaseOutputV1,
        error: *mut ObBaseErrorV1,
    ) -> u16;
    pub fn ob_base_drain_v1(
        handle: *mut ObBaseRuntimeV1,
        input: *const ObBaseCallV1,
        output: *mut ObBaseOutputV1,
        error: *mut ObBaseErrorV1,
    ) -> u16;
    pub fn ob_base_close_v1(
        handle: *mut ObBaseRuntimeV1,
        input: *const ObBaseCallV1,
        output: *mut ObBaseOutputV1,
        error: *mut ObBaseErrorV1,
    ) -> u16;
}

#[no_mangle]
/// Consume one registered host grant into a scoped management handle.
///
/// # Safety
/// `runtime` must be a live token returned by `ob_base_open_v1`; all other
/// pointers must remain valid for the call, and the input payload must cover
/// the exact registered 32-byte grant envelope.
pub unsafe extern "C" fn ob_base_management_open_v1(
    runtime: *mut ObBaseRuntimeV1,
    input: *const ObBaseCallV1,
    out_handle: *mut *mut ObBaseManagementV1,
    output: *mut ObBaseOutputV1,
    error: *mut ObBaseErrorV1,
) -> u16 {
    ffi_entry(error, "management", || {
        let call = read_call(input)?;
        let (runtime_key, runtime_record) = runtime_for_call(runtime, &call)?;
        if out_handle.is_null() || call.payload.len() != 32 {
            return Err(AbiFailure::invalid("invalid_management_open_request"));
        }
        let mut envelope = [0; 32];
        envelope.copy_from_slice(&call.payload);
        let pending = grants()
            .lock()
            .map_err(|_| AbiFailure::internal("grant_registry_poisoned"))?
            .remove(&envelope)
            .ok_or_else(|| AbiFailure::conflict("unknown_or_consumed_management_grant"))?;
        if pending.runtime_key != runtime_key {
            return Err(AbiFailure::conflict("cross_runtime_management_grant"));
        }
        let management_services = runtime_record
            .services
            .management(pending.grant)
            .map_err(AbiFailure::from)?;
        let handle = new_opaque::<ObBaseManagementV1>()?;
        let key = opaque_key(handle).map_err(AbiFailure::invalid)?;
        management()
            .lock()
            .map_err(|_| AbiFailure::internal("management_registry_poisoned"))?
            .insert(
                key,
                ManagementRecord {
                    services: Some(management_services),
                    runtime_key,
                    process_generation: runtime_record.process_generation,
                    dataset_generation: runtime_record.dataset_generation,
                    closed: false,
                },
            );
        // SAFETY: out_handle was checked non-null.
        unsafe { *out_handle = handle };
        write_output(output, &runtime_record, 101, [0; 32], b"{}")
    })
}

#[derive(Clone, Copy)]
enum ManagementOperation {
    SourceBegin,
    SourcePush,
    SourceSeal,
    SinkBegin,
    SinkRead,
    SinkCommit,
    SecretRegister,
    CapabilityAbort,
    CapabilityDestroy,
    CompleteReprovision,
}

fn management_entry(
    handle: *mut ObBaseManagementV1,
    input: *const ObBaseCallV1,
    output: *mut ObBaseOutputV1,
    error: *mut ObBaseErrorV1,
    operation: ManagementOperation,
) -> u16 {
    ffi_entry(error, "management", || {
        let call = read_call(input)?;
        let (_, record, runtime) = management_for_call(handle, &call)?;
        let services = record
            .services
            .ok_or_else(|| AbiFailure::conflict("closed_management_handle"))?;
        let request = match operation {
            ManagementOperation::SourceBegin => {
                BaseManagementRequestV1::ArchiveSourceBegin(ArchiveSourceBeginV1 {
                    reservation_id: onebrain_base_contract::BaseOperationReservationId(
                        call.operation_id,
                    ),
                    declared_total_bytes: call.value0,
                })
            }
            ManagementOperation::SourcePush => {
                BaseManagementRequestV1::ArchiveSourcePush(ArchiveSourcePushV1 {
                    handle: ArchiveSourceHandleV1::from_opaque_bytes(call.operation_id),
                    offset: call.value0,
                    chunk: ArchiveChunkV1::try_from_bytes(call.payload)
                        .map_err(|_| AbiFailure::invalid("invalid_archive_chunk"))?,
                })
            }
            ManagementOperation::SourceSeal => BaseManagementRequestV1::ArchiveSourceSeal(
                ArchiveCapabilityHandleV1::from_opaque_bytes(call.operation_id),
            ),
            ManagementOperation::SinkBegin => {
                BaseManagementRequestV1::ArchiveSinkBegin(ArchiveSinkBeginV1 {
                    reservation_id: onebrain_base_contract::BaseOperationReservationId(
                        call.operation_id,
                    ),
                    max_total_bytes: call.value0,
                })
            }
            ManagementOperation::SinkRead => {
                BaseManagementRequestV1::ArchiveSinkRead(ArchiveSinkReadV1 {
                    handle: ArchiveSinkHandleV1::from_opaque_bytes(call.operation_id),
                    offset: call.value0,
                    max_bytes: u32::try_from(call.value1)
                        .map_err(|_| AbiFailure::invalid("invalid_archive_read_limit"))?,
                })
            }
            ManagementOperation::SinkCommit => BaseManagementRequestV1::ArchiveSinkCommit(
                ArchiveCapabilityHandleV1::from_opaque_bytes(call.operation_id),
            ),
            ManagementOperation::SecretRegister => BaseManagementRequestV1::ArchiveSecretRegister(
                BoundedSecretIngressV1::try_new(
                    match call.discriminator {
                        1 => ArchiveCredentialKindV1::Password,
                        2 => ArchiveCredentialKindV1::RecoveryKey,
                        _ => return Err(AbiFailure::invalid("unknown_credential_kind")),
                    },
                    call.payload,
                )
                .map_err(|_| AbiFailure::invalid("invalid_secret_ingress"))?,
            ),
            ManagementOperation::CapabilityAbort => {
                BaseManagementRequestV1::ArchiveCapabilityAbort(
                    ArchiveCapabilityHandleV1::from_opaque_bytes(call.operation_id),
                )
            }
            ManagementOperation::CapabilityDestroy => {
                BaseManagementRequestV1::ArchiveCapabilityDestroy(
                    ArchiveCapabilityHandleV1::from_opaque_bytes(call.operation_id),
                )
            }
            ManagementOperation::CompleteReprovision => {
                BaseManagementRequestV1::CompleteSignerReprovision(CompleteSignerReprovisionV1 {
                    domain: signer_domain(call.discriminator)?,
                    expected_public_id: signer_public_id(call.discriminator, call.operation_id)?,
                    provision_handle: SignerProvisionHandleV1::from_opaque_bytes(call.auxiliary_id),
                })
            }
        };
        let response = block_on(async move { services.invoke(request).await })??;
        let (discriminator, operation_id, payload) = management_response_parts(response)?;
        write_output(output, &runtime, discriminator, operation_id, &payload)
    })
}

fn signer_domain(value: u16) -> Result<SignerDomainV1, AbiFailure> {
    match value {
        1 => Ok(SignerDomainV1::NodeTransport),
        2 => Ok(SignerDomainV1::ActorRoot),
        3 => Ok(SignerDomainV1::FeedAuthor),
        _ => Err(AbiFailure::invalid("unknown_signer_domain")),
    }
}

fn signer_public_id(value: u16, bytes: [u8; 32]) -> Result<SignerPublicIdV1, AbiFailure> {
    Ok(match value {
        1 => {
            SignerPublicIdV1::NodeTransport(onebrain_base_contract::NodeTransportPublicIdV1(bytes))
        }
        2 => SignerPublicIdV1::ActorRoot(onebrain_base_contract::ActorRootPublicIdV1(bytes)),
        3 => SignerPublicIdV1::FeedAuthor(onebrain_base_contract::FeedAuthorPublicIdV1(bytes)),
        _ => return Err(AbiFailure::invalid("unknown_signer_domain")),
    })
}

fn management_response_parts(
    response: BaseManagementResponseV1,
) -> Result<(u16, [u8; 32], Vec<u8>), AbiFailure> {
    Ok(match response {
        BaseManagementResponseV1::ArchiveSource(handle) => (102, *handle.as_bytes(), Vec::new()),
        BaseManagementResponseV1::ArchiveSink(handle) => (105, *handle.as_bytes(), Vec::new()),
        BaseManagementResponseV1::ArchiveSecret(handle) => (108, *handle.as_bytes(), Vec::new()),
        BaseManagementResponseV1::ArchiveCapability(handle) => {
            (104, *handle.as_bytes(), Vec::new())
        }
        BaseManagementResponseV1::ArchiveChunk { offset, bytes, eof } => (
            106,
            [0; 32],
            serde_json::to_vec(&serde_json::json!({
                "offset": offset,
                "eof": eof,
                "bytes": hex(&bytes),
            }))
            .map_err(|_| AbiFailure::internal("response_encoding_failed"))?,
        ),
        BaseManagementResponseV1::CapabilityClosed => (110, [0; 32], Vec::new()),
        BaseManagementResponseV1::SignerReprovisioned => (111, [0; 32], Vec::new()),
        BaseManagementResponseV1::Close(receipt) => (
            112,
            receipt.management_handle,
            serde_json::to_vec(&serde_json::json!({
                "revoked_capabilities": receipt.revoked_capabilities,
            }))
            .map_err(|_| AbiFailure::internal("response_encoding_failed"))?,
        ),
    })
}

macro_rules! management_symbol {
    ($name:ident, $operation:expr) => {
        #[no_mangle]
        /// Invoke one scoped Base management operation.
        ///
        /// # Safety
        /// `handle` must be a live token returned by
        /// `ob_base_management_open_v1`; the public input/output/error
        /// structures and all pointer/length pairs must remain valid for the
        /// complete call.
        pub unsafe extern "C" fn $name(
            handle: *mut ObBaseManagementV1,
            input: *const ObBaseCallV1,
            output: *mut ObBaseOutputV1,
            error: *mut ObBaseErrorV1,
        ) -> u16 {
            management_entry(handle, input, output, error, $operation)
        }
    };
}

management_symbol!(
    ob_base_archive_source_begin_v1,
    ManagementOperation::SourceBegin
);
management_symbol!(
    ob_base_archive_source_push_chunk_v1,
    ManagementOperation::SourcePush
);
management_symbol!(
    ob_base_archive_source_push_v1,
    ManagementOperation::SourcePush
);
management_symbol!(
    ob_base_archive_source_seal_v1,
    ManagementOperation::SourceSeal
);
management_symbol!(
    ob_base_archive_sink_begin_v1,
    ManagementOperation::SinkBegin
);
management_symbol!(
    ob_base_archive_sink_read_chunk_v1,
    ManagementOperation::SinkRead
);
management_symbol!(ob_base_archive_sink_read_v1, ManagementOperation::SinkRead);
management_symbol!(
    ob_base_archive_sink_commit_v1,
    ManagementOperation::SinkCommit
);
management_symbol!(
    ob_base_archive_secret_register_v1,
    ManagementOperation::SecretRegister
);
management_symbol!(
    ob_base_archive_capability_abort_v1,
    ManagementOperation::CapabilityAbort
);
management_symbol!(
    ob_base_archive_capability_destroy_v1,
    ManagementOperation::CapabilityDestroy
);
management_symbol!(
    ob_base_complete_signer_reprovision_v1,
    ManagementOperation::CompleteReprovision
);
management_symbol!(
    ob_base_complete_reprovision_v1,
    ManagementOperation::CompleteReprovision
);

#[allow(unexpected_cfgs)]
#[cfg(cbindgen)]
extern "C" {
    pub fn ob_base_archive_source_begin_v1(
        handle: *mut ObBaseManagementV1,
        input: *const ObBaseCallV1,
        output: *mut ObBaseOutputV1,
        error: *mut ObBaseErrorV1,
    ) -> u16;
    pub fn ob_base_archive_source_push_chunk_v1(
        handle: *mut ObBaseManagementV1,
        input: *const ObBaseCallV1,
        output: *mut ObBaseOutputV1,
        error: *mut ObBaseErrorV1,
    ) -> u16;
    pub fn ob_base_archive_source_push_v1(
        handle: *mut ObBaseManagementV1,
        input: *const ObBaseCallV1,
        output: *mut ObBaseOutputV1,
        error: *mut ObBaseErrorV1,
    ) -> u16;
    pub fn ob_base_archive_source_seal_v1(
        handle: *mut ObBaseManagementV1,
        input: *const ObBaseCallV1,
        output: *mut ObBaseOutputV1,
        error: *mut ObBaseErrorV1,
    ) -> u16;
    pub fn ob_base_archive_sink_begin_v1(
        handle: *mut ObBaseManagementV1,
        input: *const ObBaseCallV1,
        output: *mut ObBaseOutputV1,
        error: *mut ObBaseErrorV1,
    ) -> u16;
    pub fn ob_base_archive_sink_read_chunk_v1(
        handle: *mut ObBaseManagementV1,
        input: *const ObBaseCallV1,
        output: *mut ObBaseOutputV1,
        error: *mut ObBaseErrorV1,
    ) -> u16;
    pub fn ob_base_archive_sink_read_v1(
        handle: *mut ObBaseManagementV1,
        input: *const ObBaseCallV1,
        output: *mut ObBaseOutputV1,
        error: *mut ObBaseErrorV1,
    ) -> u16;
    pub fn ob_base_archive_sink_commit_v1(
        handle: *mut ObBaseManagementV1,
        input: *const ObBaseCallV1,
        output: *mut ObBaseOutputV1,
        error: *mut ObBaseErrorV1,
    ) -> u16;
    pub fn ob_base_archive_secret_register_v1(
        handle: *mut ObBaseManagementV1,
        input: *const ObBaseCallV1,
        output: *mut ObBaseOutputV1,
        error: *mut ObBaseErrorV1,
    ) -> u16;
    pub fn ob_base_archive_capability_abort_v1(
        handle: *mut ObBaseManagementV1,
        input: *const ObBaseCallV1,
        output: *mut ObBaseOutputV1,
        error: *mut ObBaseErrorV1,
    ) -> u16;
    pub fn ob_base_archive_capability_destroy_v1(
        handle: *mut ObBaseManagementV1,
        input: *const ObBaseCallV1,
        output: *mut ObBaseOutputV1,
        error: *mut ObBaseErrorV1,
    ) -> u16;
    pub fn ob_base_complete_signer_reprovision_v1(
        handle: *mut ObBaseManagementV1,
        input: *const ObBaseCallV1,
        output: *mut ObBaseOutputV1,
        error: *mut ObBaseErrorV1,
    ) -> u16;
    pub fn ob_base_complete_reprovision_v1(
        handle: *mut ObBaseManagementV1,
        input: *const ObBaseCallV1,
        output: *mut ObBaseOutputV1,
        error: *mut ObBaseErrorV1,
    ) -> u16;
}

#[no_mangle]
/// Revoke a scoped management handle and every capability it still owns.
///
/// # Safety
/// `handle` must be a token returned by `ob_base_management_open_v1`; input,
/// output, and error pointers must reference valid caller-owned public
/// structures for the complete call.
pub unsafe extern "C" fn ob_base_management_close_v1(
    handle: *mut ObBaseManagementV1,
    input: *const ObBaseCallV1,
    output: *mut ObBaseOutputV1,
    error: *mut ObBaseErrorV1,
) -> u16 {
    ffi_entry(error, "management", || {
        let call = read_call(input)?;
        let (key, record, runtime) = management_for_call(handle, &call)?;
        let services = record
            .services
            .ok_or_else(|| AbiFailure::conflict("closed_management_handle"))?;
        let receipt = block_on(async move { services.close().await })??;
        if let Some(record) = management()
            .lock()
            .map_err(|_| AbiFailure::internal("management_registry_poisoned"))?
            .get_mut(&key)
        {
            record.services = None;
            record.closed = true;
        }
        let payload = serde_json::to_vec(&serde_json::json!({
            "revoked_capabilities": receipt.revoked_capabilities,
        }))
        .map_err(|_| AbiFailure::internal("response_encoding_failed"))?;
        write_output(output, &runtime, 112, receipt.management_handle, &payload)
    })
}

#[no_mangle]
/// Release one tagged library-owned event/error allocation exactly once.
///
/// # Safety
/// `buffer` and `error` must reference valid caller-owned public structures;
/// the buffer's pointer, length, and tag must be the unchanged binding
/// returned by this library.
pub unsafe extern "C" fn ob_base_buffer_free_v1(
    buffer: *mut ObBaseOwnedBufferV1,
    error: *mut ObBaseErrorV1,
) -> u16 {
    ffi_entry(error, "buffer", || {
        validate_public_struct(
            buffer.cast_const(),
            std::mem::size_of::<ObBaseOwnedBufferV1>(),
        )?;
        // SAFETY: complete buffer prefix was size-checked.
        let buffer = unsafe { &mut *buffer };
        let allocation = allocations()
            .lock()
            .map_err(|_| AbiFailure::internal("allocation_registry_poisoned"))?
            .remove(&buffer.allocation_tag)
            .ok_or_else(|| AbiFailure::conflict("unknown_or_freed_allocation"))?;
        if allocation.bytes.as_ptr() != buffer.ptr || allocation.bytes.len() != buffer.len {
            return Err(AbiFailure::conflict("allocation_binding_mismatch"));
        }
        buffer.ptr = std::ptr::null();
        buffer.len = 0;
        buffer.allocation_tag = 0;
        Ok(())
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    static ABI_TEST_LOCK: Mutex<()> = Mutex::new(());
    use std::sync::Arc;

    use onebrain_base_contract::{
        ArchiveRestorePolicyV1, BaseCompatibilityPolicy, BaseCompatibilityTuple,
        BaseReleaseVersion, CompatibilityDigestV1, NegotiatedVersions, ProfileVersion,
        StorageSchemaVersion, TargetTriple, COMPILED_BASE_COMMIT, COMPILED_TARGET_TRIPLE,
        COMPILED_TOOLCHAIN, MAX_BASE_ARCHIVE_DATASET_BYTES,
    };
    use onebrain_node::{
        BaseLocalOperationAdapter, BaseRuntime, BaseRuntimeConfig, DatasetGenerationStore,
    };

    struct EchoAdapter;

    struct TestAuthorizer;

    impl onebrain_node::BaseHostAuthorizer for TestAuthorizer {
        fn authenticate(&self, principal: [u8; 32], proof: &[u8]) -> bool {
            principal == [0; 32] && proof == b"authenticated-abi-host"
        }
    }

    impl BaseLocalOperationAdapter for EchoAdapter {
        fn query(
            &self,
            request: BaseQueryRequestV1,
        ) -> Result<
            (
                TypedPayloadV1,
                Option<onebrain_base_contract::BaseOpaqueContinuation>,
            ),
            BaseServiceError,
        > {
            Ok((request.payload, request.continuation))
        }

        fn confirm_local(&self, command: BaseLocalCommandV1) -> Result<Vec<u8>, BaseServiceError> {
            Ok(command.payload.as_bytes().to_vec())
        }
    }

    fn runtime_config() -> BaseRuntimeConfig {
        let registry = ku_core::foundation::base_v1_profile_registry();
        let tuple = BaseCompatibilityTuple {
            base_version: BaseReleaseVersion {
                major: 1,
                minor: 1,
                patch: 0,
                prerelease: None,
            },
            base_commit: COMPILED_BASE_COMMIT,
            canonical_schema_digest: CompatibilityDigestV1(registry.canonical_schema_digest),
            domain_registry_digest: CompatibilityDigestV1(registry.domain_registry_digest),
            resource_registry_digest: CompatibilityDigestV1(registry.resource_registry_digest),
            storage_schema: StorageSchemaVersion(1),
            archive_profile: ProfileVersion { major: 1, minor: 0 },
            migration_profile: ProfileVersion { major: 1, minor: 0 },
            registry_profile: ProfileVersion { major: 1, minor: 0 },
            registry_profile_digest: CompatibilityDigestV1([4; 32]),
            wire_session: ProfileVersion { major: 1, minor: 0 },
            product_api: ProfileVersion { major: 1, minor: 1 },
            c_abi: ProfileVersion { major: 1, minor: 0 },
            feature_set_digest: CompatibilityDigestV1([5; 32]),
            target_triple: TargetTriple::try_from_string(COMPILED_TARGET_TRIPLE.to_owned())
                .unwrap(),
            toolchain: COMPILED_TOOLCHAIN,
        };
        let empty = || BaseCapabilitySet::try_from_discriminators(Vec::new()).unwrap();
        let mut config = BaseRuntimeConfig::new(
            BaseCompatibilityPolicy {
                archive_restore: ArchiveRestorePolicyV1 {
                    canonical_schema_digest: tuple.canonical_schema_digest,
                    domain_registry_digest: tuple.domain_registry_digest,
                    resource_registry_digest: tuple.resource_registry_digest,
                    storage_schema: tuple.storage_schema,
                    archive_profile: tuple.archive_profile,
                    migration_profile: tuple.migration_profile,
                    max_dataset_bytes: MAX_BASE_ARCHIVE_DATASET_BYTES,
                },
                current: tuple.clone(),
                minimum_additive: NegotiatedVersions {
                    base_minor: 0,
                    wire_session_minor: 0,
                    product_api_minor: 0,
                    c_abi_minor: 0,
                },
            },
            tuple.unqualified_status(),
            BaseCapabilityRequirements {
                supported: empty(),
                required: empty(),
            },
        );
        config.local_adapter = Arc::new(EchoAdapter);
        config.host_authorizer = Arc::new(TestAuthorizer);
        config
    }

    fn output(buffer: &mut [u8]) -> ObBaseOutputV1 {
        ObBaseOutputV1 {
            struct_size: std::mem::size_of::<ObBaseOutputV1>() as u32,
            abi_major: 1,
            abi_minor: 0,
            process_generation: [0; 32],
            dataset_generation: [0; 32],
            response_discriminator: 0,
            reserved: 0,
            operation_id: [0; 32],
            buffer_ptr: if buffer.is_empty() {
                std::ptr::null_mut()
            } else {
                buffer.as_mut_ptr()
            },
            buffer_capacity: buffer.len(),
            required_len: 0,
            written_len: 0,
        }
    }

    fn call(generation: &ObBaseOutputV1) -> ObBaseCallV1 {
        ObBaseCallV1 {
            struct_size: std::mem::size_of::<ObBaseCallV1>() as u32,
            abi_major: 1,
            abi_minor: 0,
            process_generation: generation.process_generation,
            dataset_generation: generation.dataset_generation,
            request_id: [1; 32],
            operation_id: [0; 32],
            auxiliary_id: [0; 32],
            discriminator: 0,
            flags: 0,
            value0: 16,
            value1: 4096,
            payload_ptr: std::ptr::null(),
            payload_len: 0,
        }
    }

    fn error() -> ObBaseErrorV1 {
        ObBaseErrorV1 {
            struct_size: std::mem::size_of::<ObBaseErrorV1>() as u32,
            abi_major: 1,
            abi_minor: 0,
            code: 0,
            retryable: 0,
            reconcile_before_retry: 0,
            reserved: 0,
            message_ptr: std::ptr::null(),
            message_len: 0,
            allocation_tag: 0,
        }
    }

    fn release_error_message(error: &mut ObBaseErrorV1) {
        if error.allocation_tag == 0 {
            return;
        }
        let mut owned = ObBaseOwnedBufferV1 {
            struct_size: std::mem::size_of::<ObBaseOwnedBufferV1>() as u32,
            abi_major: 1,
            abi_minor: 0,
            ptr: error.message_ptr,
            len: error.message_len,
            allocation_tag: error.allocation_tag,
        };
        // SAFETY: the error allocation and its exact binding came from this
        // library, and the caller releases it once.
        assert_eq!(unsafe { ob_base_buffer_free_v1(&mut owned, error) }, 0);
    }

    #[test]
    fn public_struct_guards_reject_short_and_wrong_major() {
        let _test_guard = ABI_TEST_LOCK.lock().unwrap();
        let mut call = ObBaseCallV1 {
            struct_size: 4,
            abi_major: 1,
            abi_minor: 0,
            process_generation: [0; 32],
            dataset_generation: [0; 32],
            request_id: [0; 32],
            operation_id: [0; 32],
            auxiliary_id: [0; 32],
            discriminator: 0,
            flags: 0,
            value0: 0,
            value1: 0,
            payload_ptr: std::ptr::null(),
            payload_len: 0,
        };
        assert_eq!(read_call(&call).unwrap_err().reason, "undersized_struct");
        call.struct_size = std::mem::size_of::<ObBaseCallV1>() as u32 + 32;
        call.abi_major = 2;
        assert_eq!(read_call(&call).unwrap_err().reason, "abi_major_mismatch");
        call.abi_major = 1;
        assert!(
            read_call(&call).is_ok(),
            "oversized same-major tail is ignored"
        );
    }

    #[test]
    fn null_length_and_owned_buffer_double_free_are_fail_closed() {
        let _test_guard = ABI_TEST_LOCK.lock().unwrap();
        let call = ObBaseCallV1 {
            struct_size: std::mem::size_of::<ObBaseCallV1>() as u32,
            abi_major: 1,
            abi_minor: 0,
            process_generation: [0; 32],
            dataset_generation: [0; 32],
            request_id: [0; 32],
            operation_id: [0; 32],
            auxiliary_id: [0; 32],
            discriminator: 0,
            flags: 0,
            value0: 0,
            value1: 0,
            payload_ptr: std::ptr::null(),
            payload_len: 1,
        };
        assert_eq!(read_call(&call).unwrap_err().reason, "null_length_mismatch");
        let (ptr, len, tag) = allocate_owned(b"typed-error").unwrap();
        let mut owned = ObBaseOwnedBufferV1 {
            struct_size: std::mem::size_of::<ObBaseOwnedBufferV1>() as u32,
            abi_major: 1,
            abi_minor: 0,
            ptr,
            len,
            allocation_tag: tag,
        };
        let mut error = error();
        // SAFETY: test supplies valid ABI structs.
        assert_eq!(unsafe { ob_base_buffer_free_v1(&mut owned, &mut error) }, 0);
        assert_eq!(unsafe { ob_base_buffer_free_v1(&mut owned, &mut error) }, 3);
        release_error_message(&mut error);
    }

    #[test]
    fn every_extern_family_catches_panics_and_maps_internal_error() {
        let _test_guard = ABI_TEST_LOCK.lock().unwrap();
        let assert_family =
            |family: &'static str, call: &mut dyn FnMut(*mut ObBaseErrorV1) -> u16| {
                std::env::set_var("ONEBRAIN_BASE_ABI_FAILPOINT", family);
                let mut error = error();
                let code = call(&mut error);
                std::env::remove_var("ONEBRAIN_BASE_ABI_FAILPOINT");
                assert_eq!(code, 13, "family {family}");
                assert_eq!(error.code, 13, "family {family}");
                release_error_message(&mut error);
            };
        // SAFETY: every failpoint fires before any null test pointer is read.
        assert_family("open", &mut |error| unsafe {
            ob_base_open_v1(
                std::ptr::null(),
                std::ptr::null_mut(),
                std::ptr::null_mut(),
                error,
            )
        });
        assert_family("status", &mut |error| unsafe {
            ob_base_status_v1(
                std::ptr::null_mut(),
                std::ptr::null(),
                std::ptr::null_mut(),
                error,
            )
        });
        assert_family("negotiate", &mut |error| unsafe {
            ob_base_negotiate_v1(
                std::ptr::null_mut(),
                std::ptr::null(),
                std::ptr::null_mut(),
                error,
            )
        });
        assert_family("query", &mut |error| unsafe {
            ob_base_query_v1(
                std::ptr::null_mut(),
                std::ptr::null(),
                std::ptr::null_mut(),
                error,
            )
        });
        assert_family("operation", &mut |error| unsafe {
            ob_base_reserve_operation_v1(
                std::ptr::null_mut(),
                std::ptr::null(),
                std::ptr::null_mut(),
                error,
            )
        });
        assert_family("subscription", &mut |error| unsafe {
            ob_base_subscribe_v1(
                std::ptr::null_mut(),
                std::ptr::null(),
                std::ptr::null_mut(),
                error,
            )
        });
        assert_family("lifecycle", &mut |error| unsafe {
            ob_base_drain_v1(
                std::ptr::null_mut(),
                std::ptr::null(),
                std::ptr::null_mut(),
                error,
            )
        });
        assert_family("management", &mut |error| unsafe {
            ob_base_management_close_v1(
                std::ptr::null_mut(),
                std::ptr::null(),
                std::ptr::null_mut(),
                error,
            )
        });
        assert_family("buffer", &mut |error| unsafe {
            ob_base_buffer_free_v1(std::ptr::null_mut(), error)
        });
    }

    #[test]
    fn registered_facade_obeys_sizing_generation_and_close_fences() {
        let _test_guard = ABI_TEST_LOCK.lock().unwrap();
        let temp = tempfile::tempdir().unwrap();
        let generations = Arc::new(DatasetGenerationStore::open_exclusive(temp.path()).unwrap());
        let mut runtime = BaseRuntime::open(generations, runtime_config()).unwrap();
        let registration =
            register_base_services_for_abi(runtime.services().unwrap(), b"immutable-test-trust")
                .unwrap();
        let request = ObBaseOpenRequestV1 {
            struct_size: std::mem::size_of::<ObBaseOpenRequestV1>() as u32,
            abi_major: 1,
            abi_minor: 0,
            registration_token: registration.token,
            host_trust_digest: registration.host_trust_digest,
        };
        let mut handle = std::ptr::null_mut();
        let mut empty = [];
        let mut open_output = output(&mut empty);
        let mut ffi_error = error();
        // SAFETY: test supplies complete ABI structs and writable outputs.
        assert_eq!(
            unsafe { ob_base_open_v1(&request, &mut handle, &mut open_output, &mut ffi_error) },
            0
        );
        assert!(!handle.is_null());
        assert_ne!(open_output.process_generation, [0; 32]);

        let request = call(&open_output);
        let mut sizing = output(&mut empty);
        // SAFETY: handle and generation fence came from open.
        assert_eq!(
            unsafe { ob_base_status_v1(handle, &request, &mut sizing, &mut ffi_error) },
            9
        );
        assert!(sizing.required_len > 0);
        let mut bytes = vec![0; sizing.required_len];
        let mut status_output = output(&mut bytes);
        assert_eq!(
            unsafe { ob_base_status_v1(handle, &request, &mut status_output, &mut ffi_error) },
            0
        );
        let status: serde_json::Value =
            serde_json::from_slice(&bytes[..status_output.written_len]).unwrap();
        assert_eq!(status["network_enabled"], false);

        let mut stale = request;
        stale.process_generation[0] ^= 1;
        assert_eq!(
            unsafe { ob_base_status_v1(handle, &stale, &mut status_output, &mut ffi_error) },
            3
        );
        release_error_message(&mut ffi_error);

        let mut close_bytes = vec![0; 128];
        let mut close_output = output(&mut close_bytes);
        assert_eq!(
            unsafe { ob_base_close_v1(handle, &request, &mut close_output, &mut ffi_error) },
            0
        );
        assert_eq!(
            unsafe { ob_base_close_v1(handle, &request, &mut close_output, &mut ffi_error) },
            3
        );
        // The aggregate owner still performs its idempotent owner-side close.
        let _ = block_on(async move { runtime.close().await }).unwrap();
    }

    #[test]
    fn ordinary_operation_subscription_and_binary_payload_corpus_crosses_ffi() {
        let _test_guard = ABI_TEST_LOCK.lock().unwrap();
        let temp = tempfile::tempdir().unwrap();
        let generations = Arc::new(DatasetGenerationStore::open_exclusive(temp.path()).unwrap());
        let mut runtime = BaseRuntime::open(generations, runtime_config()).unwrap();
        let registration =
            register_base_services_for_abi(runtime.services().unwrap(), b"immutable-test-trust")
                .unwrap();
        let open_request = ObBaseOpenRequestV1 {
            struct_size: std::mem::size_of::<ObBaseOpenRequestV1>() as u32,
            abi_major: 1,
            abi_minor: 0,
            registration_token: registration.token,
            host_trust_digest: registration.host_trust_digest,
        };
        let mut handle = std::ptr::null_mut();
        let mut empty = [];
        let mut generation = output(&mut empty);
        let mut ffi_error = error();
        assert_eq!(
            unsafe { ob_base_open_v1(&open_request, &mut handle, &mut generation, &mut ffi_error) },
            0
        );

        let mut buffer = vec![0; 4096];
        let mut reserve = call(&generation);
        reserve.discriminator = 1;
        let mut response = output(&mut buffer);
        assert_eq!(
            unsafe {
                ob_base_reserve_operation_v1(handle, &reserve, &mut response, &mut ffi_error)
            },
            0
        );
        let reservation_id = response.operation_id;

        // TypedPayloadV1 is binary. Invalid UTF-8 must cross the ABI without
        // an accidental string conversion or unbounded scan.
        let binary = [0xff, 0x00, 0xfe];
        let mut query = call(&generation);
        query.value0 = 1;
        query.value1 = binary.len() as u64;
        query.payload_ptr = binary.as_ptr();
        query.payload_len = binary.len();
        let mut query_buffer = vec![0; binary.len()];
        let mut query_output = output(&mut query_buffer);
        assert_eq!(
            unsafe { ob_base_query_v1(handle, &query, &mut query_output, &mut ffi_error) },
            0
        );
        assert_eq!(&query_buffer[..query_output.written_len], &binary);

        let payload = b"canonical-local-effect";
        let mut prepare = call(&generation);
        prepare.request_id = reservation_id;
        prepare.discriminator = 1;
        prepare.flags = 7;
        prepare.payload_ptr = payload.as_ptr();
        prepare.payload_len = payload.len();
        let mut prepare_output = output(&mut buffer);
        assert_eq!(
            unsafe { ob_base_prepare_v1(handle, &prepare, &mut prepare_output, &mut ffi_error) },
            0
        );
        let operation_id = prepare_output.operation_id;

        let mut confirm = call(&generation);
        confirm.operation_id = operation_id;
        confirm.auxiliary_id = [8; 32];
        let mut confirm_output = output(&mut buffer);
        assert_eq!(
            unsafe { ob_base_confirm_v1(handle, &confirm, &mut confirm_output, &mut ffi_error) },
            0
        );
        assert_eq!(confirm_output.operation_id, operation_id);

        let mut reconcile = call(&generation);
        reconcile.operation_id = operation_id;
        let mut reconcile_output = output(&mut buffer);
        assert_eq!(
            unsafe {
                ob_base_reconcile_v1(handle, &reconcile, &mut reconcile_output, &mut ffi_error)
            },
            0
        );

        let mut subscribe = call(&generation);
        subscribe.discriminator = 2;
        subscribe.value0 = 0;
        let mut subscribe_output = output(&mut buffer);
        assert_eq!(
            unsafe {
                ob_base_subscribe_v1(handle, &subscribe, &mut subscribe_output, &mut ffi_error)
            },
            0
        );
        let subscription_id = subscribe_output.operation_id;

        let mut poll = call(&generation);
        poll.operation_id = subscription_id;
        poll.value0 = 0;
        poll.value1 = 256;
        let mut poll_output = output(&mut buffer);
        assert_eq!(
            unsafe { ob_base_poll_events_v1(handle, &poll, &mut poll_output, &mut ffi_error) },
            0
        );
        let events: serde_json::Value =
            serde_json::from_slice(&buffer[..poll_output.written_len]).unwrap();
        assert!(!events["events"].as_array().unwrap().is_empty());
        assert_eq!(events["resync_required"], false);

        let mut close_subscription = call(&generation);
        close_subscription.operation_id = subscription_id;
        let mut close_subscription_output = output(&mut empty);
        assert_eq!(
            unsafe {
                ob_base_close_subscription_v1(
                    handle,
                    &close_subscription,
                    &mut close_subscription_output,
                    &mut ffi_error,
                )
            },
            0
        );

        let handle_key = handle as usize;
        let process_generation = generation.process_generation;
        let dataset_generation = generation.dataset_generation;
        let concurrent = (0..8)
            .map(|index| {
                std::thread::spawn(move || {
                    let handle = handle_key as *mut ObBaseRuntimeV1;
                    let call = ObBaseCallV1 {
                        struct_size: std::mem::size_of::<ObBaseCallV1>() as u32,
                        abi_major: 1,
                        abi_minor: 0,
                        process_generation,
                        dataset_generation,
                        request_id: [index as u8; 32],
                        operation_id: [0; 32],
                        auxiliary_id: [0; 32],
                        discriminator: 0,
                        flags: 0,
                        value0: 0,
                        value1: 0,
                        payload_ptr: std::ptr::null(),
                        payload_len: 0,
                    };
                    let mut bytes = vec![0; 4096];
                    let mut output = output(&mut bytes);
                    let mut error = error();
                    // SAFETY: every thread owns its C structs/buffer; the
                    // opaque handle is cloneable service authority, not a
                    // dereferenced Rust pointer.
                    unsafe { ob_base_status_v1(handle, &call, &mut output, &mut error) }
                })
            })
            .collect::<Vec<_>>();
        for result in concurrent {
            assert_eq!(result.join().unwrap(), 0);
        }

        let mut drain_output = output(&mut buffer);
        let lifecycle = call(&generation);
        assert_eq!(
            unsafe { ob_base_drain_v1(handle, &lifecycle, &mut drain_output, &mut ffi_error) },
            0
        );
        let mut close_output = output(&mut buffer);
        assert_eq!(
            unsafe { ob_base_close_v1(handle, &lifecycle, &mut close_output, &mut ffi_error) },
            0
        );
        let _ = block_on(async move { runtime.close().await }).unwrap();
    }

    #[test]
    fn scoped_management_grant_and_archive_ingress_cross_ffi_once() {
        let _test_guard = ABI_TEST_LOCK.lock().unwrap();
        let temp = tempfile::tempdir().unwrap();
        let generations = Arc::new(DatasetGenerationStore::open_exclusive(temp.path()).unwrap());
        let mut runtime = BaseRuntime::open(generations, runtime_config()).unwrap();
        let registration =
            register_base_services_for_abi(runtime.services().unwrap(), b"immutable-test-trust")
                .unwrap();
        let open_request = ObBaseOpenRequestV1 {
            struct_size: std::mem::size_of::<ObBaseOpenRequestV1>() as u32,
            abi_major: 1,
            abi_minor: 0,
            registration_token: registration.token,
            host_trust_digest: registration.host_trust_digest,
        };
        let mut runtime_handle = std::ptr::null_mut();
        let mut empty = [];
        let mut generation = output(&mut empty);
        let mut ffi_error = error();
        assert_eq!(
            unsafe {
                ob_base_open_v1(
                    &open_request,
                    &mut runtime_handle,
                    &mut generation,
                    &mut ffi_error,
                )
            },
            0
        );

        let mut buffer = vec![0; 4096];
        let mut reserve = call(&generation);
        reserve.discriminator = 3;
        let mut reserve_output = output(&mut empty);
        assert_eq!(
            unsafe {
                ob_base_reserve_operation_v1(
                    runtime_handle,
                    &reserve,
                    &mut reserve_output,
                    &mut ffi_error,
                )
            },
            0
        );
        let reservation_id = reserve_output.operation_id;
        let grant = runtime
            .issue_management_grant(
                [0; 32],
                b"authenticated-abi-host",
                [
                    onebrain_node::BaseManagementScope::ArchiveSource,
                    onebrain_node::BaseManagementScope::ArchiveSecret,
                ],
                std::time::Duration::from_secs(60),
            )
            .unwrap();
        let envelope = register_management_grant_for_abi(runtime_handle, grant).unwrap();
        let mut management_open = call(&generation);
        management_open.payload_ptr = envelope.as_ptr();
        management_open.payload_len = envelope.len();
        let mut management_handle = std::ptr::null_mut();
        let mut management_output = output(&mut buffer);
        assert_eq!(
            unsafe {
                ob_base_management_open_v1(
                    runtime_handle,
                    &management_open,
                    &mut management_handle,
                    &mut management_output,
                    &mut ffi_error,
                )
            },
            0
        );
        assert!(!management_handle.is_null());

        let mut replay_handle = std::ptr::null_mut();
        assert_eq!(
            unsafe {
                ob_base_management_open_v1(
                    runtime_handle,
                    &management_open,
                    &mut replay_handle,
                    &mut management_output,
                    &mut ffi_error,
                )
            },
            3
        );
        assert!(replay_handle.is_null());
        release_error_message(&mut ffi_error);

        let mut source_begin = call(&generation);
        source_begin.operation_id = reservation_id;
        source_begin.value0 = 3;
        let mut source_output = output(&mut empty);
        assert_eq!(
            unsafe {
                ob_base_archive_source_begin_v1(
                    management_handle,
                    &source_begin,
                    &mut source_output,
                    &mut ffi_error,
                )
            },
            0
        );
        let source = source_output.operation_id;

        let bytes = [1, 2, 3];
        let mut source_push = call(&generation);
        source_push.operation_id = source;
        source_push.value0 = 0;
        source_push.payload_ptr = bytes.as_ptr();
        source_push.payload_len = bytes.len();
        let mut source_push_output = output(&mut empty);
        assert_eq!(
            unsafe {
                ob_base_archive_source_push_v1(
                    management_handle,
                    &source_push,
                    &mut source_push_output,
                    &mut ffi_error,
                )
            },
            0
        );

        let mut source_seal = call(&generation);
        source_seal.operation_id = source;
        let mut source_seal_output = output(&mut empty);
        assert_eq!(
            unsafe {
                ob_base_archive_source_seal_v1(
                    management_handle,
                    &source_seal,
                    &mut source_seal_output,
                    &mut ffi_error,
                )
            },
            0
        );

        let password = b"bounded-password";
        let mut secret = call(&generation);
        secret.discriminator = 1;
        secret.payload_ptr = password.as_ptr();
        secret.payload_len = password.len();
        let mut secret_output = output(&mut empty);
        assert_eq!(
            unsafe {
                ob_base_archive_secret_register_v1(
                    management_handle,
                    &secret,
                    &mut secret_output,
                    &mut ffi_error,
                )
            },
            0
        );

        let mut management_close_output = output(&mut buffer);
        let close = call(&generation);
        assert_eq!(
            unsafe {
                ob_base_management_close_v1(
                    management_handle,
                    &close,
                    &mut management_close_output,
                    &mut ffi_error,
                )
            },
            0
        );
        let close_receipt: serde_json::Value =
            serde_json::from_slice(&buffer[..management_close_output.written_len]).unwrap();
        assert!(close_receipt["revoked_capabilities"].as_u64().unwrap() >= 2);
        assert_eq!(
            unsafe {
                ob_base_archive_source_seal_v1(
                    management_handle,
                    &source_seal,
                    &mut source_seal_output,
                    &mut ffi_error,
                )
            },
            3
        );
        release_error_message(&mut ffi_error);

        let mut close_output = output(&mut buffer);
        assert_eq!(
            unsafe {
                ob_base_close_v1(
                    runtime_handle,
                    &call(&generation),
                    &mut close_output,
                    &mut ffi_error,
                )
            },
            0
        );
        let _ = block_on(async move { runtime.close().await }).unwrap();
    }

    #[test]
    fn forged_management_envelope_cannot_open_scoped_handle() {
        let _test_guard = ABI_TEST_LOCK.lock().unwrap();
        let temp = tempfile::tempdir().unwrap();
        let generations = Arc::new(DatasetGenerationStore::open_exclusive(temp.path()).unwrap());
        let runtime = BaseRuntime::open(generations, runtime_config()).unwrap();
        let registration =
            register_base_services_for_abi(runtime.services().unwrap(), b"immutable-test-trust")
                .unwrap();
        let request = ObBaseOpenRequestV1 {
            struct_size: std::mem::size_of::<ObBaseOpenRequestV1>() as u32,
            abi_major: 1,
            abi_minor: 0,
            registration_token: registration.token,
            host_trust_digest: registration.host_trust_digest,
        };
        let mut handle = std::ptr::null_mut();
        let mut empty = [];
        let mut open_output = output(&mut empty);
        let mut ffi_error = error();
        assert_eq!(
            unsafe { ob_base_open_v1(&request, &mut handle, &mut open_output, &mut ffi_error) },
            0
        );
        let forged = [7u8; 32];
        let mut call = call(&open_output);
        call.payload_ptr = forged.as_ptr();
        call.payload_len = forged.len();
        let mut management_handle = std::ptr::null_mut();
        let mut management_output = output(&mut empty);
        assert_eq!(
            unsafe {
                ob_base_management_open_v1(
                    handle,
                    &call,
                    &mut management_handle,
                    &mut management_output,
                    &mut ffi_error,
                )
            },
            3
        );
        assert!(management_handle.is_null());
        release_error_message(&mut ffi_error);
    }
}

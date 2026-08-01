//! Stable native bridge for the autonomous OneBrain mobile runtime profile.
//!
//! Native code owns platform paths and execution opportunities. Rust owns the
//! runtime singleton, bootstrap database, activation grants, local KQL smoke
//! and callback-generation fence. No path or database handle crosses to Dart.

use std::{
    ffi::c_char,
    path::PathBuf,
    slice, str,
    sync::{
        atomic::{AtomicBool, Ordering},
        Mutex, OnceLock,
    },
};

#[cfg(target_os = "android")]
use onebrain_mobile_core::OwnedMediaSummary;
use onebrain_mobile_core::{
    ActivationPhase, AppLockPolicy, MobileFeatureFlags, MobileRuntimeFacade, MobileRuntimeSnapshot,
    OnboardingCursor, RegistryInitPlan, RegistryNetworkPolicy, RegistryOperationState,
    ResourceBudgets, RuntimeServices, SecurityBootstrapMaterial, SecuritySessionState,
};

/// Stable ABI revision understood by the current Swift/Kotlin adapters.
pub const OB_MOBILE_BRIDGE_ABI_VERSION: u32 = 8;

pub const OB_MOBILE_RUNTIME_OK: u32 = 0;
pub const OB_MOBILE_RUNTIME_INVALID_PATH: u32 = 1;
pub const OB_MOBILE_RUNTIME_CORE_ERROR: u32 = 2;
pub const OB_MOBILE_RUNTIME_LOCK_POISONED: u32 = 3;
pub const OB_MOBILE_RUNTIME_NOT_OPEN: u32 = 4;
pub const OB_MOBILE_RUNTIME_INVALID_SECURITY_MATERIAL: u32 = 5;
pub const OB_MOBILE_RUNTIME_INVALID_DRAFT: u32 = 6;
pub const OB_MOBILE_RUNTIME_INVALID_ONBOARDING_CURSOR: u32 = 7;
pub const OB_MOBILE_RUNTIME_INVALID_SHARE_SPOOL: u32 = 8;
pub const OB_MOBILE_RUNTIME_SHARE_SPOOL_NOT_FOUND: u32 = 9;
pub const OB_MOBILE_RUNTIME_INVALID_MEDIA_STAGE: u32 = 10;
pub const OB_MOBILE_RUNTIME_INVALID_REGISTRY_INIT: u32 = 11;

const CORE_VERSION: &[u8] = concat!(env!("CARGO_PKG_VERSION"), "\0").as_bytes();

static RUNTIME: OnceLock<Mutex<Option<MobileRuntimeFacade>>> = OnceLock::new();
static REGISTRY_REQUEST_ISSUED: AtomicBool = AtomicBool::new(false);

#[repr(C)]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ObMobileRegistryPlan {
    pub status_code: u32,
    pub operation_id: [u8; 129],
    pub operation_id_len: u32,
    pub state_code: u32,
    pub channel_id: [u8; 65],
    pub channel_id_len: u32,
    pub release_id: [u8; 65],
    pub release_id_len: u32,
    pub manifest_digest: [u8; 65],
    pub manifest_digest_len: u32,
    pub trust_profile_digest: [u8; 65],
    pub trust_profile_digest_len: u32,
    pub head_generation: u64,
    pub release_sequence: u64,
    pub publisher_min_additional_free_bytes: u64,
    pub artifact_total_bytes: u64,
    pub target_total_alloc_bytes: u64,
    pub transfer_initial_bytes: u64,
    pub verification_workspace_bytes: u64,
    pub catalog_growth_bytes: u64,
    pub safety_reserve_bytes: u64,
    pub destination_total_usable_bytes: u64,
    pub measured_free_bytes: u64,
    pub initial_required_free_bytes: u64,
    pub admitted: u8,
}

impl ObMobileRegistryPlan {
    const fn error(status_code: u32) -> Self {
        Self {
            status_code,
            operation_id: [0; 129],
            operation_id_len: 0,
            state_code: 0,
            channel_id: [0; 65],
            channel_id_len: 0,
            release_id: [0; 65],
            release_id_len: 0,
            manifest_digest: [0; 65],
            manifest_digest_len: 0,
            trust_profile_digest: [0; 65],
            trust_profile_digest_len: 0,
            head_generation: 0,
            release_sequence: 0,
            publisher_min_additional_free_bytes: 0,
            artifact_total_bytes: 0,
            target_total_alloc_bytes: 0,
            transfer_initial_bytes: 0,
            verification_workspace_bytes: 0,
            catalog_growth_bytes: 0,
            safety_reserve_bytes: 0,
            destination_total_usable_bytes: 0,
            measured_free_bytes: 0,
            initial_required_free_bytes: 0,
            admitted: 0,
        }
    }

    fn from_core(plan: RegistryInitPlan) -> Self {
        let mut bridged = Self::error(OB_MOBILE_RUNTIME_OK);
        bridged.operation_id_len =
            copy_utf8(&mut bridged.operation_id, &plan.operation.operation_id);
        bridged.state_code = registry_state_code(plan.operation.state);
        bridged.channel_id_len = copy_utf8(
            &mut bridged.channel_id,
            plan.operation.channel_id.as_deref().unwrap_or(""),
        );
        bridged.release_id_len = copy_utf8(&mut bridged.release_id, &plan.release.release_id);
        bridged.manifest_digest_len =
            copy_utf8(&mut bridged.manifest_digest, &plan.release.manifest_digest);
        bridged.trust_profile_digest_len = copy_utf8(
            &mut bridged.trust_profile_digest,
            &plan.release.trust_profile_digest,
        );
        bridged.head_generation = plan.operation.head_generation.unwrap_or(0);
        bridged.release_sequence = plan.release.release_sequence;
        bridged.publisher_min_additional_free_bytes =
            plan.capacity.publisher_min_additional_free_bytes;
        bridged.artifact_total_bytes = plan.release.artifact_total_bytes;
        bridged.target_total_alloc_bytes = plan.capacity.target_total_alloc_bytes;
        bridged.transfer_initial_bytes = plan.capacity.transfer_initial_bytes;
        bridged.verification_workspace_bytes = plan.capacity.verification_workspace_bytes;
        bridged.catalog_growth_bytes = plan.capacity.catalog_growth_bytes;
        bridged.safety_reserve_bytes = plan.capacity.safety_reserve_bytes;
        bridged.destination_total_usable_bytes = plan.capacity.destination_total_usable_bytes;
        bridged.measured_free_bytes = plan.capacity.measured_free_bytes;
        bridged.initial_required_free_bytes = plan.capacity.initial_required_free_bytes;
        bridged.admitted = bool_byte(plan.capacity.admitted());
        bridged
    }
}

#[repr(C)]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ObMobileRuntimeSnapshot {
    pub status_code: u32,
    pub process_generation: u64,
    pub activation_phase: u32,
    pub active_grant_count: u32,
    pub recovered_unclean_start: u8,
    pub bootstrap_store_opened: u8,
    pub registry_bootstrap_only: u8,
    pub local_kql_fixture_verified: u8,
    pub private_planner_verified: u8,
    pub no_llm_provider: u8,
    pub stale_callback_rejected: u8,
    pub secure_profile_active: u8,
    pub installation_binding_verified: u8,
    pub installation_created: u8,
    pub security_session_unlocked: u8,
    pub private_vault_ready: u8,
    pub identity_domains_separated: u8,
    pub privacy_defaults_fail_safe: u8,
    pub redacted_history_ready: u8,
    pub encrypted_raw_draft_count: u64,
    pub pending_share_spool_count: u64,
    pub staged_verified_media_count: u64,
    pub onboarding_cursor: u32,
}

impl ObMobileRuntimeSnapshot {
    const fn error(status_code: u32) -> Self {
        Self {
            status_code,
            process_generation: 0,
            activation_phase: 0,
            active_grant_count: 0,
            recovered_unclean_start: 0,
            bootstrap_store_opened: 0,
            registry_bootstrap_only: 0,
            local_kql_fixture_verified: 0,
            private_planner_verified: 0,
            no_llm_provider: 0,
            stale_callback_rejected: 0,
            secure_profile_active: 0,
            installation_binding_verified: 0,
            installation_created: 0,
            security_session_unlocked: 0,
            private_vault_ready: 0,
            identity_domains_separated: 0,
            privacy_defaults_fail_safe: 0,
            redacted_history_ready: 0,
            encrypted_raw_draft_count: 0,
            pending_share_spool_count: 0,
            staged_verified_media_count: 0,
            onboarding_cursor: 0,
        }
    }

    fn from_core(snapshot: MobileRuntimeSnapshot) -> Self {
        Self {
            status_code: OB_MOBILE_RUNTIME_OK,
            process_generation: snapshot.process_generation,
            activation_phase: activation_phase_code(snapshot.activation_phase),
            active_grant_count: u32::try_from(snapshot.active_grant_count).unwrap_or(u32::MAX),
            recovered_unclean_start: bool_byte(snapshot.recovered_unclean_start),
            bootstrap_store_opened: bool_byte(snapshot.bootstrap_store_opened),
            registry_bootstrap_only: bool_byte(snapshot.registry_state == "BootstrapOnly"),
            local_kql_fixture_verified: bool_byte(snapshot.local_kql_fixture_verified),
            private_planner_verified: bool_byte(snapshot.private_planner_verified),
            no_llm_provider: bool_byte(
                snapshot.llm_provider_id == "none" && !snapshot.llm_available,
            ),
            stale_callback_rejected: bool_byte(snapshot.stale_callback_rejected),
            secure_profile_active: bool_byte(snapshot.secure_profile_active),
            installation_binding_verified: bool_byte(snapshot.installation_binding_verified),
            installation_created: bool_byte(snapshot.installation_created),
            security_session_unlocked: bool_byte(
                snapshot.security_session_state == SecuritySessionState::Unlocked,
            ),
            private_vault_ready: bool_byte(snapshot.private_vault_ready),
            identity_domains_separated: bool_byte(snapshot.identity_domains_separated),
            privacy_defaults_fail_safe: bool_byte(snapshot.privacy_defaults_fail_safe),
            redacted_history_ready: bool_byte(snapshot.redacted_history_ready),
            encrypted_raw_draft_count: snapshot.encrypted_raw_draft_count,
            pending_share_spool_count: snapshot.pending_share_spool_count,
            staged_verified_media_count: snapshot.staged_verified_media_count,
            onboarding_cursor: snapshot.onboarding_cursor.code(),
        }
    }
}

#[repr(C)]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ObMobileShareSpoolSummary {
    pub status_code: u32,
    pub spool_ref: [u8; 39],
    pub spool_ref_len: u32,
    pub mime_type: [u8; 64],
    pub mime_type_len: u32,
    pub content_bytes: u64,
    pub received_at_monotonic_ms: u64,
}

impl ObMobileShareSpoolSummary {
    const fn error(status_code: u32) -> Self {
        Self {
            status_code,
            spool_ref: [0; 39],
            spool_ref_len: 0,
            mime_type: [0; 64],
            mime_type_len: 0,
            content_bytes: 0,
            received_at_monotonic_ms: 0,
        }
    }

    fn from_core(summary: onebrain_mobile_core::ShareSpoolSummary) -> Self {
        let mut bridged = Self::error(OB_MOBILE_RUNTIME_OK);
        let spool_ref = summary.spool_ref.as_bytes();
        let mime_type = summary.mime_type.as_bytes();
        bridged.spool_ref[..spool_ref.len()].copy_from_slice(spool_ref);
        bridged.spool_ref_len = u32::try_from(spool_ref.len()).unwrap_or(0);
        bridged.mime_type[..mime_type.len()].copy_from_slice(mime_type);
        bridged.mime_type_len = u32::try_from(mime_type.len()).unwrap_or(0);
        bridged.content_bytes = summary.content_bytes;
        bridged.received_at_monotonic_ms = summary.received_at_monotonic_ms;
        bridged
    }
}

#[repr(C)]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ObMobileMediaStageReceipt {
    pub status_code: u32,
    pub source_ref: [u8; 40],
    pub source_ref_len: u32,
    pub media_class: [u8; 16],
    pub media_class_len: u32,
    pub mime_type: [u8; 128],
    pub mime_type_len: u32,
    pub content_bytes: u64,
    pub blake3_digest: [u8; 64],
    pub blake3_digest_len: u32,
}

impl ObMobileMediaStageReceipt {
    const fn error(status_code: u32) -> Self {
        Self {
            status_code,
            source_ref: [0; 40],
            source_ref_len: 0,
            media_class: [0; 16],
            media_class_len: 0,
            mime_type: [0; 128],
            mime_type_len: 0,
            content_bytes: 0,
            blake3_digest: [0; 64],
            blake3_digest_len: 0,
        }
    }

    fn started(source_ref: &str) -> Self {
        let mut bridged = Self::error(OB_MOBILE_RUNTIME_OK);
        let reference = source_ref.as_bytes();
        bridged.source_ref[..reference.len()].copy_from_slice(reference);
        bridged.source_ref_len = u32::try_from(reference.len()).unwrap_or(0);
        bridged
    }

    fn from_core(receipt: onebrain_mobile_core::MediaStageReceipt) -> Self {
        let mut bridged = Self::error(OB_MOBILE_RUNTIME_OK);
        let source_ref = receipt.source_ref.as_bytes();
        let media_class = receipt.media_class.as_bytes();
        let mime_type = receipt.mime_type.as_bytes();
        let digest = receipt.blake3_digest.as_bytes();
        bridged.source_ref[..source_ref.len()].copy_from_slice(source_ref);
        bridged.source_ref_len = u32::try_from(source_ref.len()).unwrap_or(0);
        bridged.media_class[..media_class.len()].copy_from_slice(media_class);
        bridged.media_class_len = u32::try_from(media_class.len()).unwrap_or(0);
        bridged.mime_type[..mime_type.len()].copy_from_slice(mime_type);
        bridged.mime_type_len = u32::try_from(mime_type.len()).unwrap_or(0);
        bridged.content_bytes = receipt.content_bytes;
        bridged.blake3_digest[..digest.len()].copy_from_slice(digest);
        bridged.blake3_digest_len = u32::try_from(digest.len()).unwrap_or(0);
        bridged
    }
}

#[repr(C)]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ObMobileRawDraftReceipt {
    pub status_code: u32,
    pub draft_ref: [u8; 39],
    pub draft_ref_len: u32,
    pub content_language: [u8; 36],
    pub content_language_len: u32,
    pub content_bytes: u64,
    pub saved_at_monotonic_ms: u64,
    pub total_drafts: u64,
}

impl ObMobileRawDraftReceipt {
    const fn error(status_code: u32) -> Self {
        Self {
            status_code,
            draft_ref: [0; 39],
            draft_ref_len: 0,
            content_language: [0; 36],
            content_language_len: 0,
            content_bytes: 0,
            saved_at_monotonic_ms: 0,
            total_drafts: 0,
        }
    }

    fn from_core(receipt: onebrain_mobile_core::RawDraftReceipt) -> Self {
        let mut bridged = Self::error(OB_MOBILE_RUNTIME_OK);
        let draft_ref = receipt.draft_ref.as_bytes();
        let language = receipt.content_language.as_bytes();
        bridged.draft_ref[..draft_ref.len()].copy_from_slice(draft_ref);
        bridged.draft_ref_len = u32::try_from(draft_ref.len()).unwrap_or(0);
        bridged.content_language[..language.len()].copy_from_slice(language);
        bridged.content_language_len = u32::try_from(language.len()).unwrap_or(0);
        bridged.content_bytes = receipt.content_bytes;
        bridged.saved_at_monotonic_ms = receipt.saved_at_monotonic_ms;
        bridged.total_drafts = receipt.total_drafts;
        bridged
    }
}

/// Return the stable native-to-Rust ABI revision.
#[unsafe(no_mangle)]
pub extern "C" fn ob_mobile_bridge_abi_version() -> u32 {
    OB_MOBILE_BRIDGE_ABI_VERSION
}

/// Return a process-lifetime, NUL-terminated Rust bridge version.
///
/// The caller must not free or mutate this pointer.
#[unsafe(no_mangle)]
pub extern "C" fn ob_mobile_bridge_core_version() -> *const c_char {
    CORE_VERSION.as_ptr().cast()
}

/// Report whether this bootstrap-only bridge has requested Registry bytes.
///
/// Registry transfer authority remains disabled until the explicit Init slice.
#[unsafe(no_mangle)]
pub extern "C" fn ob_mobile_bridge_registry_request_issued() -> u8 {
    bool_byte(REGISTRY_REQUEST_ISSUED.load(Ordering::Acquire))
}

/// Resolve and verify signed Registry metadata, then return an exact local
/// capacity plan. This does not submit or transfer any Registry artifact.
///
/// # Safety
///
/// Every pointer must reference its declared readable byte length.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn ob_mobile_runtime_prepare_registry_init_signed(
    channel_id: *const u8,
    channel_id_len: usize,
    trust_profile_cbor: *const u8,
    trust_profile_cbor_len: usize,
    channel_head_envelope_cbor: *const u8,
    channel_head_envelope_cbor_len: usize,
    release_envelope_cbor: *const u8,
    release_envelope_cbor_len: usize,
    allocation_unit_bytes: u64,
    destination_total_usable_bytes: u64,
    measured_free_bytes: u64,
) -> ObMobileRegistryPlan {
    let Some(channel_id) = (unsafe { parse_bounded_utf8(channel_id, channel_id_len, 64) }) else {
        return ObMobileRegistryPlan::error(OB_MOBILE_RUNTIME_INVALID_REGISTRY_INIT);
    };
    let Some(trust_profile) =
        (unsafe { parse_bounded_bytes(trust_profile_cbor, trust_profile_cbor_len, 64 * 1024) })
    else {
        return ObMobileRegistryPlan::error(OB_MOBILE_RUNTIME_INVALID_REGISTRY_INIT);
    };
    let Some(channel_head) = (unsafe {
        parse_bounded_bytes(
            channel_head_envelope_cbor,
            channel_head_envelope_cbor_len,
            64 * 1024,
        )
    }) else {
        return ObMobileRegistryPlan::error(OB_MOBILE_RUNTIME_INVALID_REGISTRY_INIT);
    };
    let Some(release) = (unsafe {
        parse_bounded_bytes(
            release_envelope_cbor,
            release_envelope_cbor_len,
            1024 * 1024 + 1024,
        )
    }) else {
        return ObMobileRegistryPlan::error(OB_MOBILE_RUNTIME_INVALID_REGISTRY_INIT);
    };
    prepare_registry_init(
        channel_id,
        trust_profile,
        channel_head,
        release,
        allocation_unit_bytes,
        destination_total_usable_bytes,
        measured_free_bytes,
    )
}

/// Persist a Limited-mode receipt bound to the exact reviewed manifest.
///
/// # Safety
///
/// Both pointers must reference their declared readable UTF-8 byte lengths.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn ob_mobile_runtime_defer_registry_init_utf8(
    operation_id: *const u8,
    operation_id_len: usize,
    manifest_digest: *const u8,
    manifest_digest_len: usize,
) -> u32 {
    let Some(operation_id) = (unsafe { parse_bounded_utf8(operation_id, operation_id_len, 128) })
    else {
        return OB_MOBILE_RUNTIME_INVALID_REGISTRY_INIT;
    };
    let Some(manifest_digest) =
        (unsafe { parse_bounded_utf8(manifest_digest, manifest_digest_len, 64) })
    else {
        return OB_MOBILE_RUNTIME_INVALID_REGISTRY_INIT;
    };
    defer_registry_init(operation_id, manifest_digest)
}

/// Recheck native storage facts and bind exact user confirmation in Rust.
/// This admits capacity only; transfer submission remains a later slice.
///
/// # Safety
///
/// Every pointer must reference its declared readable byte length.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn ob_mobile_runtime_confirm_registry_init_signed(
    operation_id: *const u8,
    operation_id_len: usize,
    manifest_digest: *const u8,
    manifest_digest_len: usize,
    trust_profile_cbor: *const u8,
    trust_profile_cbor_len: usize,
    network_policy_code: u32,
    one_time_network_override: u8,
    allocation_unit_bytes: u64,
    destination_total_usable_bytes: u64,
    measured_free_bytes: u64,
) -> ObMobileRegistryPlan {
    let Some(operation_id) = (unsafe { parse_bounded_utf8(operation_id, operation_id_len, 128) })
    else {
        return ObMobileRegistryPlan::error(OB_MOBILE_RUNTIME_INVALID_REGISTRY_INIT);
    };
    let Some(manifest_digest) =
        (unsafe { parse_bounded_utf8(manifest_digest, manifest_digest_len, 64) })
    else {
        return ObMobileRegistryPlan::error(OB_MOBILE_RUNTIME_INVALID_REGISTRY_INIT);
    };
    let Some(trust_profile) =
        (unsafe { parse_bounded_bytes(trust_profile_cbor, trust_profile_cbor_len, 64 * 1024) })
    else {
        return ObMobileRegistryPlan::error(OB_MOBILE_RUNTIME_INVALID_REGISTRY_INIT);
    };
    let Some(network_policy) = registry_network_policy(network_policy_code) else {
        return ObMobileRegistryPlan::error(OB_MOBILE_RUNTIME_INVALID_REGISTRY_INIT);
    };
    confirm_registry_init(
        operation_id,
        manifest_digest,
        trust_profile,
        network_policy,
        one_time_network_override != 0,
        allocation_unit_bytes,
        destination_total_usable_bytes,
        measured_free_bytes,
    )
}

/// Bounded deterministic call used to verify the complete generated call path.
#[unsafe(no_mangle)]
pub extern "C" fn ob_mobile_bridge_round_trip(nonce: u64) -> u64 {
    nonce
}

/// Open the process-wide mobile runtime from a native-owned UTF-8 data root.
///
/// Repeated calls in the same process return the existing generation.
///
/// # Safety
///
/// `path` must reference `path_len` readable bytes for the duration of this
/// call. The bytes must form a non-empty UTF-8 path.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn ob_mobile_runtime_open_utf8(
    path: *const u8,
    path_len: usize,
) -> ObMobileRuntimeSnapshot {
    if path.is_null() || path_len == 0 || path_len > 32_768 {
        return ObMobileRuntimeSnapshot::error(OB_MOBILE_RUNTIME_INVALID_PATH);
    }
    let path = match unsafe { parse_path(path, path_len) } {
        Ok(path) => path,
        Err(snapshot) => return snapshot,
    };
    open_runtime(path)
}

/// Open the process-wide runtime with native-protected installation material.
///
/// The native caller must zeroize its temporary plaintext buffer immediately
/// after this function returns. The material never crosses the Dart bridge.
///
/// # Safety
///
/// Both pointers must reference their declared readable byte lengths for the
/// duration of this call.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn ob_mobile_runtime_open_secure_utf8(
    path: *const u8,
    path_len: usize,
    security_material: *const u8,
    security_material_len: usize,
) -> ObMobileRuntimeSnapshot {
    let path = match unsafe { parse_path(path, path_len) } {
        Ok(path) => path,
        Err(snapshot) => return snapshot,
    };
    if security_material.is_null() {
        return ObMobileRuntimeSnapshot::error(OB_MOBILE_RUNTIME_INVALID_SECURITY_MATERIAL);
    }
    let material_bytes = unsafe { slice::from_raw_parts(security_material, security_material_len) };
    let material = match SecurityBootstrapMaterial::from_bytes(material_bytes) {
        Ok(material) => material,
        Err(_) => {
            return ObMobileRuntimeSnapshot::error(OB_MOBILE_RUNTIME_INVALID_SECURITY_MATERIAL);
        }
    };
    open_runtime_secured(path, material)
}

/// Inspect the process-wide runtime without reopening its database.
#[unsafe(no_mangle)]
pub extern "C" fn ob_mobile_runtime_snapshot() -> ObMobileRuntimeSnapshot {
    runtime_snapshot()
}

/// Quiesce the current process generation.
#[unsafe(no_mangle)]
pub extern "C" fn ob_mobile_runtime_graceful_stop() -> u32 {
    let runtime = RUNTIME.get_or_init(|| Mutex::new(None));
    let mut guard = match runtime.lock() {
        Ok(guard) => guard,
        Err(_) => return OB_MOBILE_RUNTIME_LOCK_POISONED,
    };
    let Some(facade) = guard.as_mut() else {
        return OB_MOBILE_RUNTIME_NOT_OPEN;
    };
    match facade.graceful_stop() {
        Ok(()) => OB_MOBILE_RUNTIME_OK,
        Err(_) => OB_MOBILE_RUNTIME_CORE_ERROR,
    }
}

/// Lock and zeroize the current private-node session without relying on a
/// lifecycle callback for correctness.
#[unsafe(no_mangle)]
pub extern "C" fn ob_mobile_runtime_lock_private_node() -> u32 {
    let runtime = RUNTIME.get_or_init(|| Mutex::new(None));
    let mut guard = match runtime.lock() {
        Ok(guard) => guard,
        Err(_) => return OB_MOBILE_RUNTIME_LOCK_POISONED,
    };
    let Some(facade) = guard.as_mut() else {
        return OB_MOBILE_RUNTIME_NOT_OPEN;
    };
    match facade.lock_private_node() {
        Ok(()) => OB_MOBILE_RUNTIME_OK,
        Err(_) => OB_MOBILE_RUNTIME_CORE_ERROR,
    }
}

/// Persist bounded UTF-8 text as an encrypted `PrivateLocal` raw draft.
///
/// The returned reference is opaque. No filesystem path, database handle,
/// encryption key or plaintext content is returned.
///
/// # Safety
///
/// Both pointers must reference their declared readable byte lengths for the
/// duration of this call.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn ob_mobile_runtime_save_raw_text_draft_utf8(
    content_language: *const u8,
    content_language_len: usize,
    content_utf8: *const u8,
    content_len: usize,
) -> ObMobileRawDraftReceipt {
    if content_language.is_null()
        || content_utf8.is_null()
        || content_language_len == 0
        || content_language_len > 35
        || content_len == 0
        || content_len > 512 * 1024
    {
        return ObMobileRawDraftReceipt::error(OB_MOBILE_RUNTIME_INVALID_DRAFT);
    }
    let language_bytes = unsafe { slice::from_raw_parts(content_language, content_language_len) };
    let language = match str::from_utf8(language_bytes) {
        Ok(language) => language,
        Err(_) => return ObMobileRawDraftReceipt::error(OB_MOBILE_RUNTIME_INVALID_DRAFT),
    };
    let content = unsafe { slice::from_raw_parts(content_utf8, content_len) };
    save_raw_text_draft(language, content)
}

/// Land a bounded native share callback into the encrypted private spool.
///
/// This native-only entry point is not exposed to Dart. The callback token is
/// used solely for idempotency and the returned spool reference is opaque.
///
/// # Safety
///
/// Every pointer must reference its declared readable byte length.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn ob_mobile_runtime_enqueue_shared_text_utf8(
    callback_token: *const u8,
    callback_token_len: usize,
    mime_type: *const u8,
    mime_type_len: usize,
    content_utf8: *const u8,
    content_len: usize,
) -> ObMobileShareSpoolSummary {
    if callback_token.is_null()
        || mime_type.is_null()
        || content_utf8.is_null()
        || callback_token_len == 0
        || callback_token_len > 96
        || mime_type_len == 0
        || mime_type_len > 63
        || content_len == 0
        || content_len > 512 * 1024
    {
        return ObMobileShareSpoolSummary::error(OB_MOBILE_RUNTIME_INVALID_SHARE_SPOOL);
    }
    let callback_token = match str::from_utf8(unsafe {
        slice::from_raw_parts(callback_token, callback_token_len)
    }) {
        Ok(value) => value,
        Err(_) => {
            return ObMobileShareSpoolSummary::error(OB_MOBILE_RUNTIME_INVALID_SHARE_SPOOL);
        }
    };
    let mime_type = match str::from_utf8(unsafe { slice::from_raw_parts(mime_type, mime_type_len) })
    {
        Ok(value) => value,
        Err(_) => {
            return ObMobileShareSpoolSummary::error(OB_MOBILE_RUNTIME_INVALID_SHARE_SPOOL);
        }
    };
    let content = unsafe { slice::from_raw_parts(content_utf8, content_len) };
    enqueue_shared_text(callback_token, mime_type, content)
}

/// Return one pending encrypted share spool by stable sorted index.
#[unsafe(no_mangle)]
pub extern "C" fn ob_mobile_runtime_pending_share_spool_at(
    index: usize,
) -> ObMobileShareSpoolSummary {
    pending_share_spool_at(index)
}

/// Import a pending `text/plain` spool into an encrypted raw draft.
///
/// # Safety
///
/// Both pointers must reference their declared readable byte lengths.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn ob_mobile_runtime_import_shared_text_utf8(
    spool_ref: *const u8,
    spool_ref_len: usize,
    content_language: *const u8,
    content_language_len: usize,
) -> ObMobileRawDraftReceipt {
    if spool_ref.is_null()
        || content_language.is_null()
        || spool_ref_len == 0
        || spool_ref_len > 38
        || content_language_len == 0
        || content_language_len > 35
    {
        return ObMobileRawDraftReceipt::error(OB_MOBILE_RUNTIME_INVALID_SHARE_SPOOL);
    }
    let spool_ref = match str::from_utf8(unsafe { slice::from_raw_parts(spool_ref, spool_ref_len) })
    {
        Ok(value) => value,
        Err(_) => {
            return ObMobileRawDraftReceipt::error(OB_MOBILE_RUNTIME_INVALID_SHARE_SPOOL);
        }
    };
    let language = match str::from_utf8(unsafe {
        slice::from_raw_parts(content_language, content_language_len)
    }) {
        Ok(value) => value,
        Err(_) => {
            return ObMobileRawDraftReceipt::error(OB_MOBILE_RUNTIME_INVALID_SHARE_SPOOL);
        }
    };
    import_shared_text(spool_ref, language)
}

/// Begin a native-owned system-picker stream into encrypted private staging.
///
/// Neither a provider URI nor filesystem path enters this ABI.
///
/// # Safety
///
/// Both pointers must reference their declared readable byte lengths.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn ob_mobile_runtime_start_media_stage_utf8(
    requested_class: *const u8,
    requested_class_len: usize,
    declared_mime_type: *const u8,
    declared_mime_type_len: usize,
) -> ObMobileMediaStageReceipt {
    let Some(requested_class) =
        (unsafe { parse_bounded_utf8(requested_class, requested_class_len, 15) })
    else {
        return ObMobileMediaStageReceipt::error(OB_MOBILE_RUNTIME_INVALID_MEDIA_STAGE);
    };
    let Some(declared_mime_type) =
        (unsafe { parse_bounded_utf8(declared_mime_type, declared_mime_type_len, 127) })
    else {
        return ObMobileMediaStageReceipt::error(OB_MOBILE_RUNTIME_INVALID_MEDIA_STAGE);
    };
    start_media_stage(requested_class, declared_mime_type)
}

/// Append one bounded plaintext chunk; Rust encrypts and commits it before return.
///
/// # Safety
///
/// Both pointers must reference their declared readable byte lengths.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn ob_mobile_runtime_append_media_stage(
    source_ref: *const u8,
    source_ref_len: usize,
    chunk: *const u8,
    chunk_len: usize,
) -> u32 {
    let Some(source_ref) = (unsafe { parse_bounded_utf8(source_ref, source_ref_len, 39) }) else {
        return OB_MOBILE_RUNTIME_INVALID_MEDIA_STAGE;
    };
    if chunk.is_null() || chunk_len == 0 || chunk_len > 256 * 1024 {
        return OB_MOBILE_RUNTIME_INVALID_MEDIA_STAGE;
    }
    append_media_stage(source_ref, unsafe {
        slice::from_raw_parts(chunk, chunk_len)
    })
}

/// Verify the complete encrypted stream and return its opaque source receipt.
///
/// # Safety
///
/// `source_ref` must reference its declared readable byte length.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn ob_mobile_runtime_finish_media_stage_utf8(
    source_ref: *const u8,
    source_ref_len: usize,
) -> ObMobileMediaStageReceipt {
    let Some(source_ref) = (unsafe { parse_bounded_utf8(source_ref, source_ref_len, 39) }) else {
        return ObMobileMediaStageReceipt::error(OB_MOBILE_RUNTIME_INVALID_MEDIA_STAGE);
    };
    finish_media_stage(source_ref)
}

/// Mark a non-verified stream interrupted and remove its encrypted partial file.
///
/// # Safety
///
/// `source_ref` must reference its declared readable byte length.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn ob_mobile_runtime_abort_media_stage_utf8(
    source_ref: *const u8,
    source_ref_len: usize,
) -> u32 {
    let Some(source_ref) = (unsafe { parse_bounded_utf8(source_ref, source_ref_len, 39) }) else {
        return OB_MOBILE_RUNTIME_INVALID_MEDIA_STAGE;
    };
    abort_media_stage(source_ref)
}

/// Persist a bounded onboarding resume cursor in the Rust bootstrap store.
#[unsafe(no_mangle)]
pub extern "C" fn ob_mobile_runtime_set_onboarding_cursor(cursor_code: u32) -> u32 {
    let Some(cursor) = OnboardingCursor::from_code(cursor_code) else {
        return OB_MOBILE_RUNTIME_INVALID_ONBOARDING_CURSOR;
    };
    set_onboarding_cursor(cursor)
}

fn open_runtime(data_root: PathBuf) -> ObMobileRuntimeSnapshot {
    let runtime = RUNTIME.get_or_init(|| Mutex::new(None));
    let mut guard = match runtime.lock() {
        Ok(guard) => guard,
        Err(_) => return ObMobileRuntimeSnapshot::error(OB_MOBILE_RUNTIME_LOCK_POISONED),
    };
    if let Some(facade) = guard.as_ref() {
        return ObMobileRuntimeSnapshot::from_core(facade.snapshot());
    }
    match MobileRuntimeFacade::open_bootstrap_only(
        RuntimeServices::bootstrap_only(data_root),
        MobileFeatureFlags::default(),
        ResourceBudgets::default(),
    ) {
        Ok(facade) => {
            let snapshot = ObMobileRuntimeSnapshot::from_core(facade.snapshot());
            *guard = Some(facade);
            snapshot
        }
        Err(_) => ObMobileRuntimeSnapshot::error(OB_MOBILE_RUNTIME_CORE_ERROR),
    }
}

fn open_runtime_secured(
    data_root: PathBuf,
    material: SecurityBootstrapMaterial,
) -> ObMobileRuntimeSnapshot {
    let runtime = RUNTIME.get_or_init(|| Mutex::new(None));
    let mut guard = match runtime.lock() {
        Ok(guard) => guard,
        Err(_) => return ObMobileRuntimeSnapshot::error(OB_MOBILE_RUNTIME_LOCK_POISONED),
    };
    if let Some(facade) = guard.as_mut() {
        return match facade.unlock_private_node(material, AppLockPolicy::default()) {
            Ok(()) => ObMobileRuntimeSnapshot::from_core(facade.snapshot()),
            Err(_) => ObMobileRuntimeSnapshot::error(OB_MOBILE_RUNTIME_CORE_ERROR),
        };
    }
    match MobileRuntimeFacade::open_secured(
        RuntimeServices::bootstrap_only(data_root),
        MobileFeatureFlags::default(),
        ResourceBudgets::default(),
        material,
        AppLockPolicy::default(),
    ) {
        Ok(facade) => {
            let snapshot = ObMobileRuntimeSnapshot::from_core(facade.snapshot());
            *guard = Some(facade);
            snapshot
        }
        Err(_) => ObMobileRuntimeSnapshot::error(OB_MOBILE_RUNTIME_CORE_ERROR),
    }
}

unsafe fn parse_path(path: *const u8, path_len: usize) -> Result<PathBuf, ObMobileRuntimeSnapshot> {
    if path.is_null() || path_len == 0 || path_len > 32_768 {
        return Err(ObMobileRuntimeSnapshot::error(
            OB_MOBILE_RUNTIME_INVALID_PATH,
        ));
    }
    let bytes = unsafe { slice::from_raw_parts(path, path_len) };
    match str::from_utf8(bytes) {
        Ok(path) if !path.is_empty() => Ok(PathBuf::from(path)),
        _ => Err(ObMobileRuntimeSnapshot::error(
            OB_MOBILE_RUNTIME_INVALID_PATH,
        )),
    }
}

unsafe fn parse_bounded_utf8<'a>(
    value: *const u8,
    value_len: usize,
    max_len: usize,
) -> Option<&'a str> {
    if value.is_null() || value_len == 0 || value_len > max_len {
        return None;
    }
    str::from_utf8(unsafe { slice::from_raw_parts(value, value_len) }).ok()
}

unsafe fn parse_bounded_bytes<'a>(
    value: *const u8,
    value_len: usize,
    max_len: usize,
) -> Option<&'a [u8]> {
    if value.is_null() || value_len == 0 || value_len > max_len {
        return None;
    }
    Some(unsafe { slice::from_raw_parts(value, value_len) })
}

#[allow(clippy::too_many_arguments)]
fn prepare_registry_init(
    channel_id: &str,
    trust_profile_cbor: &[u8],
    channel_head_envelope_cbor: &[u8],
    release_envelope_cbor: &[u8],
    allocation_unit_bytes: u64,
    destination_total_usable_bytes: u64,
    measured_free_bytes: u64,
) -> ObMobileRegistryPlan {
    let runtime = RUNTIME.get_or_init(|| Mutex::new(None));
    let guard = match runtime.lock() {
        Ok(guard) => guard,
        Err(_) => return ObMobileRegistryPlan::error(OB_MOBILE_RUNTIME_LOCK_POISONED),
    };
    let Some(facade) = guard.as_ref() else {
        return ObMobileRegistryPlan::error(OB_MOBILE_RUNTIME_NOT_OPEN);
    };
    match facade.prepare_signed_registry_init(
        channel_id,
        trust_profile_cbor,
        channel_head_envelope_cbor,
        release_envelope_cbor,
        allocation_unit_bytes,
        destination_total_usable_bytes,
        measured_free_bytes,
    ) {
        Ok(plan) => {
            REGISTRY_REQUEST_ISSUED.store(true, Ordering::Release);
            ObMobileRegistryPlan::from_core(plan)
        }
        Err(_) => ObMobileRegistryPlan::error(OB_MOBILE_RUNTIME_CORE_ERROR),
    }
}

fn defer_registry_init(operation_id: &str, manifest_digest: &str) -> u32 {
    let runtime = RUNTIME.get_or_init(|| Mutex::new(None));
    let guard = match runtime.lock() {
        Ok(guard) => guard,
        Err(_) => return OB_MOBILE_RUNTIME_LOCK_POISONED,
    };
    let Some(facade) = guard.as_ref() else {
        return OB_MOBILE_RUNTIME_NOT_OPEN;
    };
    match facade.defer_registry_init(operation_id, manifest_digest) {
        Ok(_) => OB_MOBILE_RUNTIME_OK,
        Err(_) => OB_MOBILE_RUNTIME_CORE_ERROR,
    }
}

#[allow(clippy::too_many_arguments)]
fn confirm_registry_init(
    operation_id: &str,
    manifest_digest: &str,
    trust_profile_cbor: &[u8],
    network_policy: RegistryNetworkPolicy,
    one_time_network_override: bool,
    allocation_unit_bytes: u64,
    destination_total_usable_bytes: u64,
    measured_free_bytes: u64,
) -> ObMobileRegistryPlan {
    let runtime = RUNTIME.get_or_init(|| Mutex::new(None));
    let guard = match runtime.lock() {
        Ok(guard) => guard,
        Err(_) => return ObMobileRegistryPlan::error(OB_MOBILE_RUNTIME_LOCK_POISONED),
    };
    let Some(facade) = guard.as_ref() else {
        return ObMobileRegistryPlan::error(OB_MOBILE_RUNTIME_NOT_OPEN);
    };
    match facade.confirm_signed_registry_init(
        operation_id,
        manifest_digest,
        trust_profile_cbor,
        network_policy,
        one_time_network_override,
        allocation_unit_bytes,
        destination_total_usable_bytes,
        measured_free_bytes,
    ) {
        Ok(plan) => ObMobileRegistryPlan::from_core(plan),
        Err(_) => ObMobileRegistryPlan::error(OB_MOBILE_RUNTIME_CORE_ERROR),
    }
}

fn runtime_snapshot() -> ObMobileRuntimeSnapshot {
    let runtime = RUNTIME.get_or_init(|| Mutex::new(None));
    let guard = match runtime.lock() {
        Ok(guard) => guard,
        Err(_) => return ObMobileRuntimeSnapshot::error(OB_MOBILE_RUNTIME_LOCK_POISONED),
    };
    guard.as_ref().map_or_else(
        || ObMobileRuntimeSnapshot::error(OB_MOBILE_RUNTIME_NOT_OPEN),
        |facade| ObMobileRuntimeSnapshot::from_core(facade.snapshot()),
    )
}

fn save_raw_text_draft(content_language: &str, content_utf8: &[u8]) -> ObMobileRawDraftReceipt {
    let runtime = RUNTIME.get_or_init(|| Mutex::new(None));
    let mut guard = match runtime.lock() {
        Ok(guard) => guard,
        Err(_) => {
            return ObMobileRawDraftReceipt::error(OB_MOBILE_RUNTIME_LOCK_POISONED);
        }
    };
    let Some(facade) = guard.as_mut() else {
        return ObMobileRawDraftReceipt::error(OB_MOBILE_RUNTIME_NOT_OPEN);
    };
    match facade.save_raw_text_draft(content_language, content_utf8) {
        Ok(receipt) => ObMobileRawDraftReceipt::from_core(receipt),
        Err(_) => ObMobileRawDraftReceipt::error(OB_MOBILE_RUNTIME_CORE_ERROR),
    }
}

fn enqueue_shared_text(
    callback_token: &str,
    mime_type: &str,
    content_utf8: &[u8],
) -> ObMobileShareSpoolSummary {
    let runtime = RUNTIME.get_or_init(|| Mutex::new(None));
    let mut guard = match runtime.lock() {
        Ok(guard) => guard,
        Err(_) => {
            return ObMobileShareSpoolSummary::error(OB_MOBILE_RUNTIME_LOCK_POISONED);
        }
    };
    let Some(facade) = guard.as_mut() else {
        return ObMobileShareSpoolSummary::error(OB_MOBILE_RUNTIME_NOT_OPEN);
    };
    match facade.enqueue_shared_text(callback_token, mime_type, content_utf8) {
        Ok(receipt) => ObMobileShareSpoolSummary::from_core(receipt),
        Err(_) => ObMobileShareSpoolSummary::error(OB_MOBILE_RUNTIME_CORE_ERROR),
    }
}

fn pending_share_spool_at(index: usize) -> ObMobileShareSpoolSummary {
    let runtime = RUNTIME.get_or_init(|| Mutex::new(None));
    let guard = match runtime.lock() {
        Ok(guard) => guard,
        Err(_) => {
            return ObMobileShareSpoolSummary::error(OB_MOBILE_RUNTIME_LOCK_POISONED);
        }
    };
    let Some(facade) = guard.as_ref() else {
        return ObMobileShareSpoolSummary::error(OB_MOBILE_RUNTIME_NOT_OPEN);
    };
    match facade.pending_share_spools(index.saturating_add(1)) {
        Ok(spools) => spools
            .into_iter()
            .nth(index)
            .map(ObMobileShareSpoolSummary::from_core)
            .unwrap_or_else(|| {
                ObMobileShareSpoolSummary::error(OB_MOBILE_RUNTIME_SHARE_SPOOL_NOT_FOUND)
            }),
        Err(_) => ObMobileShareSpoolSummary::error(OB_MOBILE_RUNTIME_CORE_ERROR),
    }
}

fn import_shared_text(spool_ref: &str, content_language: &str) -> ObMobileRawDraftReceipt {
    let runtime = RUNTIME.get_or_init(|| Mutex::new(None));
    let mut guard = match runtime.lock() {
        Ok(guard) => guard,
        Err(_) => {
            return ObMobileRawDraftReceipt::error(OB_MOBILE_RUNTIME_LOCK_POISONED);
        }
    };
    let Some(facade) = guard.as_mut() else {
        return ObMobileRawDraftReceipt::error(OB_MOBILE_RUNTIME_NOT_OPEN);
    };
    match facade.import_shared_text(spool_ref, content_language) {
        Ok(receipt) => ObMobileRawDraftReceipt::from_core(receipt),
        Err(_) => ObMobileRawDraftReceipt::error(OB_MOBILE_RUNTIME_CORE_ERROR),
    }
}

fn start_media_stage(requested_class: &str, declared_mime_type: &str) -> ObMobileMediaStageReceipt {
    let runtime = RUNTIME.get_or_init(|| Mutex::new(None));
    let mut guard = match runtime.lock() {
        Ok(guard) => guard,
        Err(_) => {
            return ObMobileMediaStageReceipt::error(OB_MOBILE_RUNTIME_LOCK_POISONED);
        }
    };
    let Some(facade) = guard.as_mut() else {
        return ObMobileMediaStageReceipt::error(OB_MOBILE_RUNTIME_NOT_OPEN);
    };
    match facade.start_media_stage(requested_class, declared_mime_type) {
        Ok(source_ref) => ObMobileMediaStageReceipt::started(&source_ref),
        Err(_) => ObMobileMediaStageReceipt::error(OB_MOBILE_RUNTIME_CORE_ERROR),
    }
}

fn append_media_stage(source_ref: &str, chunk: &[u8]) -> u32 {
    let runtime = RUNTIME.get_or_init(|| Mutex::new(None));
    let mut guard = match runtime.lock() {
        Ok(guard) => guard,
        Err(_) => return OB_MOBILE_RUNTIME_LOCK_POISONED,
    };
    let Some(facade) = guard.as_mut() else {
        return OB_MOBILE_RUNTIME_NOT_OPEN;
    };
    match facade.append_media_stage(source_ref, chunk) {
        Ok(()) => OB_MOBILE_RUNTIME_OK,
        Err(_) => OB_MOBILE_RUNTIME_CORE_ERROR,
    }
}

fn finish_media_stage(source_ref: &str) -> ObMobileMediaStageReceipt {
    let runtime = RUNTIME.get_or_init(|| Mutex::new(None));
    let mut guard = match runtime.lock() {
        Ok(guard) => guard,
        Err(_) => {
            return ObMobileMediaStageReceipt::error(OB_MOBILE_RUNTIME_LOCK_POISONED);
        }
    };
    let Some(facade) = guard.as_mut() else {
        return ObMobileMediaStageReceipt::error(OB_MOBILE_RUNTIME_NOT_OPEN);
    };
    match facade.finish_media_stage(source_ref) {
        Ok(receipt) => ObMobileMediaStageReceipt::from_core(receipt),
        Err(_) => ObMobileMediaStageReceipt::error(OB_MOBILE_RUNTIME_CORE_ERROR),
    }
}

#[cfg(target_os = "android")]
fn finish_owned_media_import(source_ref: &str) -> Option<OwnedMediaSummary> {
    let runtime = RUNTIME.get_or_init(|| Mutex::new(None));
    let mut guard = runtime.lock().ok()?;
    guard.as_mut()?.finish_owned_media_import(source_ref).ok()
}

#[cfg(target_os = "android")]
fn owned_media(index: usize) -> Option<OwnedMediaSummary> {
    let runtime = RUNTIME.get_or_init(|| Mutex::new(None));
    let guard = runtime.lock().ok()?;
    guard
        .as_ref()?
        .owned_media(100)
        .ok()?
        .into_iter()
        .nth(index)
}

#[cfg(target_os = "android")]
fn owned_media_count() -> u64 {
    let runtime = RUNTIME.get_or_init(|| Mutex::new(None));
    runtime
        .lock()
        .ok()
        .and_then(|guard| {
            guard
                .as_ref()
                .map(|facade| facade.snapshot().owned_original_media_count)
        })
        .unwrap_or(0)
}

#[cfg(target_os = "android")]
fn encode_owned_media(summary: &OwnedMediaSummary) -> String {
    format!(
        "{}|{}|{}|{}|{}|{}|{}|{}",
        summary.media_ref,
        summary.media_class,
        summary.mime_type,
        summary.content_bytes,
        summary.verified_bytes,
        summary.storage_class,
        u8::from(summary.owned_hold),
        summary.import_state,
    )
}

fn abort_media_stage(source_ref: &str) -> u32 {
    let runtime = RUNTIME.get_or_init(|| Mutex::new(None));
    let mut guard = match runtime.lock() {
        Ok(guard) => guard,
        Err(_) => return OB_MOBILE_RUNTIME_LOCK_POISONED,
    };
    let Some(facade) = guard.as_mut() else {
        return OB_MOBILE_RUNTIME_NOT_OPEN;
    };
    match facade.abort_media_stage(source_ref) {
        Ok(()) => OB_MOBILE_RUNTIME_OK,
        Err(_) => OB_MOBILE_RUNTIME_CORE_ERROR,
    }
}

fn set_onboarding_cursor(cursor: OnboardingCursor) -> u32 {
    let runtime = RUNTIME.get_or_init(|| Mutex::new(None));
    let guard = match runtime.lock() {
        Ok(guard) => guard,
        Err(_) => return OB_MOBILE_RUNTIME_LOCK_POISONED,
    };
    let Some(facade) = guard.as_ref() else {
        return OB_MOBILE_RUNTIME_NOT_OPEN;
    };
    match facade.set_onboarding_cursor(cursor) {
        Ok(()) => OB_MOBILE_RUNTIME_OK,
        Err(_) => OB_MOBILE_RUNTIME_CORE_ERROR,
    }
}

const fn bool_byte(value: bool) -> u8 {
    if value {
        1
    } else {
        0
    }
}

fn copy_utf8<const N: usize>(destination: &mut [u8; N], value: &str) -> u32 {
    let bytes = value.as_bytes();
    let length = bytes.len().min(N.saturating_sub(1));
    destination[..length].copy_from_slice(&bytes[..length]);
    u32::try_from(length).unwrap_or(0)
}

const fn registry_network_policy(code: u32) -> Option<RegistryNetworkPolicy> {
    match code {
        0 => Some(RegistryNetworkPolicy::WifiOnly),
        1 => Some(RegistryNetworkPolicy::Unmetered),
        2 => Some(RegistryNetworkPolicy::AnyNetwork),
        _ => None,
    }
}

const fn registry_state_code(state: RegistryOperationState) -> u32 {
    match state {
        RegistryOperationState::IntentRecorded => 0,
        RegistryOperationState::ResolvingHead => 1,
        RegistryOperationState::HeadVerified => 2,
        RegistryOperationState::ResolvingManifest => 3,
        RegistryOperationState::ManifestVerified => 4,
        RegistryOperationState::AwaitingExactConfirm => 5,
        RegistryOperationState::DeferredByUser => 6,
        RegistryOperationState::AdmissionPending => 7,
        RegistryOperationState::CapacityAdmitted => 8,
        RegistryOperationState::SchedulePrepared => 9,
        RegistryOperationState::TransferSubmitted => 10,
        RegistryOperationState::TransferAdopted => 11,
        RegistryOperationState::TransferQueued => 12,
        RegistryOperationState::Downloading => 13,
        RegistryOperationState::BytesComplete => 14,
        RegistryOperationState::WholeArtifactsVerified => 15,
        RegistryOperationState::QuerySmokePassed => 16,
        RegistryOperationState::DirectoryCommitted => 17,
        RegistryOperationState::PointerCommitted => 18,
        RegistryOperationState::HealthPending => 19,
        RegistryOperationState::Completed => 20,
        RegistryOperationState::Waiting => 21,
        RegistryOperationState::Failed => 22,
        RegistryOperationState::Cancelled => 23,
    }
}

const fn activation_phase_code(phase: ActivationPhase) -> u32 {
    match phase {
        ActivationPhase::Dormant => 0,
        ActivationPhase::Starting => 1,
        ActivationPhase::Active => 2,
        ActivationPhase::Draining => 3,
    }
}

#[cfg(target_os = "android")]
mod android {
    use jni::{
        errors::ThrowRuntimeExAndDefault,
        jni_mangle,
        objects::{JByteArray, JClass, JString},
        sys::{jboolean, jint, jlong, JNI_FALSE, JNI_TRUE},
        EnvUnowned,
    };

    use super::{
        abort_media_stage, append_media_stage, confirm_registry_init, defer_registry_init,
        encode_owned_media, enqueue_shared_text, finish_media_stage, finish_owned_media_import,
        import_shared_text, ob_mobile_bridge_abi_version, ob_mobile_bridge_registry_request_issued,
        ob_mobile_bridge_round_trip, ob_mobile_runtime_lock_private_node,
        ob_mobile_runtime_set_onboarding_cursor, open_runtime, open_runtime_secured, owned_media,
        owned_media_count, pending_share_spool_at, prepare_registry_init, registry_network_policy,
        runtime_snapshot, save_raw_text_draft, start_media_stage, ObMobileRegistryPlan,
        CORE_VERSION, OB_MOBILE_RUNTIME_INVALID_REGISTRY_INIT, OB_MOBILE_RUNTIME_OK,
    };
    use onebrain_mobile_core::SecurityBootstrapMaterial;
    use zeroize::Zeroize;

    #[jni_mangle("org.onebrain.onebrain_mobile.RustMobileBridge", "nativeAbiVersion")]
    pub fn native_abi_version(_env: EnvUnowned<'_>, _class: JClass<'_>) -> jint {
        ob_mobile_bridge_abi_version() as jint
    }

    #[jni_mangle("org.onebrain.onebrain_mobile.RustMobileBridge", "nativeCoreVersion")]
    pub fn native_core_version<'caller>(
        mut unowned_env: EnvUnowned<'caller>,
        _class: JClass<'caller>,
    ) -> JString<'caller> {
        let version = std::str::from_utf8(&CORE_VERSION[..CORE_VERSION.len() - 1])
            .expect("Cargo package versions are valid UTF-8");
        unowned_env
            .with_env(|env| JString::from_str(env, version))
            .resolve::<ThrowRuntimeExAndDefault>()
    }

    #[jni_mangle(
        "org.onebrain.onebrain_mobile.RustMobileBridge",
        "nativeRegistryRequestIssued"
    )]
    pub fn native_registry_request_issued(_env: EnvUnowned<'_>, _class: JClass<'_>) -> jboolean {
        if ob_mobile_bridge_registry_request_issued() == 0 {
            JNI_FALSE
        } else {
            JNI_TRUE
        }
    }

    #[jni_mangle("org.onebrain.onebrain_mobile.RustMobileBridge", "nativeRoundTrip")]
    pub fn native_round_trip(_env: EnvUnowned<'_>, _class: JClass<'_>, nonce: jlong) -> jlong {
        ob_mobile_bridge_round_trip(nonce as u64) as jlong
    }

    #[jni_mangle("org.onebrain.onebrain_mobile.RustMobileBridge", "nativeRuntimeOpen")]
    pub fn native_runtime_open<'caller>(
        mut unowned_env: EnvUnowned<'caller>,
        _class: JClass<'caller>,
        data_root: JString<'caller>,
    ) -> jint {
        let path = unowned_env
            .with_env(|env| data_root.try_to_string(env))
            .resolve::<ThrowRuntimeExAndDefault>();
        open_runtime(path.into()).status_code as jint
    }

    #[jni_mangle(
        "org.onebrain.onebrain_mobile.RustMobileBridge",
        "nativeRuntimeOpenSecure"
    )]
    pub fn native_runtime_open_secure<'caller>(
        mut unowned_env: EnvUnowned<'caller>,
        _class: JClass<'caller>,
        data_root: JString<'caller>,
        security_material: JByteArray<'caller>,
    ) -> jint {
        let path = unowned_env
            .with_env(|env| data_root.try_to_string(env))
            .resolve::<ThrowRuntimeExAndDefault>();
        let mut material_bytes = unowned_env
            .with_env(|env| env.convert_byte_array(&security_material))
            .resolve::<ThrowRuntimeExAndDefault>();
        let result = SecurityBootstrapMaterial::from_bytes(&material_bytes)
            .map(|material| open_runtime_secured(path.into(), material).status_code as jint)
            .unwrap_or(super::OB_MOBILE_RUNTIME_INVALID_SECURITY_MATERIAL as jint);
        material_bytes.zeroize();
        result
    }

    #[jni_mangle("org.onebrain.onebrain_mobile.RustMobileBridge", "nativeRuntimeLock")]
    pub fn native_runtime_lock(_env: EnvUnowned<'_>, _class: JClass<'_>) -> jint {
        ob_mobile_runtime_lock_private_node() as jint
    }

    #[jni_mangle(
        "org.onebrain.onebrain_mobile.RustMobileBridge",
        "nativeRuntimePrepareRegistryInit"
    )]
    #[allow(clippy::too_many_arguments)]
    pub fn native_runtime_prepare_registry_init<'caller>(
        mut unowned_env: EnvUnowned<'caller>,
        _class: JClass<'caller>,
        channel_id: JString<'caller>,
        trust_profile: JByteArray<'caller>,
        channel_head: JByteArray<'caller>,
        release: JByteArray<'caller>,
        allocation_unit_bytes: jlong,
        destination_total_usable_bytes: jlong,
        measured_free_bytes: jlong,
    ) -> JString<'caller> {
        let channel_id = unowned_env
            .with_env(|env| channel_id.try_to_string(env))
            .resolve::<ThrowRuntimeExAndDefault>();
        let trust_profile = unowned_env
            .with_env(|env| env.convert_byte_array(&trust_profile))
            .resolve::<ThrowRuntimeExAndDefault>();
        let channel_head = unowned_env
            .with_env(|env| env.convert_byte_array(&channel_head))
            .resolve::<ThrowRuntimeExAndDefault>();
        let release = unowned_env
            .with_env(|env| env.convert_byte_array(&release))
            .resolve::<ThrowRuntimeExAndDefault>();
        let plan = if allocation_unit_bytes <= 0
            || destination_total_usable_bytes <= 0
            || measured_free_bytes < 0
        {
            ObMobileRegistryPlan::error(OB_MOBILE_RUNTIME_INVALID_REGISTRY_INIT)
        } else {
            prepare_registry_init(
                &channel_id,
                &trust_profile,
                &channel_head,
                &release,
                allocation_unit_bytes as u64,
                destination_total_usable_bytes as u64,
                measured_free_bytes as u64,
            )
        };
        let encoded = encode_registry_plan(&plan);
        unowned_env
            .with_env(|env| JString::from_str(env, &encoded))
            .resolve::<ThrowRuntimeExAndDefault>()
    }

    #[jni_mangle(
        "org.onebrain.onebrain_mobile.RustMobileBridge",
        "nativeRuntimeDeferRegistryInit"
    )]
    pub fn native_runtime_defer_registry_init<'caller>(
        mut unowned_env: EnvUnowned<'caller>,
        _class: JClass<'caller>,
        operation_id: JString<'caller>,
        manifest_digest: JString<'caller>,
    ) -> jint {
        let operation_id = unowned_env
            .with_env(|env| operation_id.try_to_string(env))
            .resolve::<ThrowRuntimeExAndDefault>();
        let manifest_digest = unowned_env
            .with_env(|env| manifest_digest.try_to_string(env))
            .resolve::<ThrowRuntimeExAndDefault>();
        defer_registry_init(&operation_id, &manifest_digest) as jint
    }

    #[jni_mangle(
        "org.onebrain.onebrain_mobile.RustMobileBridge",
        "nativeRuntimeConfirmRegistryInit"
    )]
    #[allow(clippy::too_many_arguments)]
    pub fn native_runtime_confirm_registry_init<'caller>(
        mut unowned_env: EnvUnowned<'caller>,
        _class: JClass<'caller>,
        operation_id: JString<'caller>,
        manifest_digest: JString<'caller>,
        trust_profile: JByteArray<'caller>,
        network_policy_code: jint,
        one_time_network_override: jboolean,
        allocation_unit_bytes: jlong,
        destination_total_usable_bytes: jlong,
        measured_free_bytes: jlong,
    ) -> JString<'caller> {
        let operation_id = unowned_env
            .with_env(|env| operation_id.try_to_string(env))
            .resolve::<ThrowRuntimeExAndDefault>();
        let manifest_digest = unowned_env
            .with_env(|env| manifest_digest.try_to_string(env))
            .resolve::<ThrowRuntimeExAndDefault>();
        let trust_profile = unowned_env
            .with_env(|env| env.convert_byte_array(&trust_profile))
            .resolve::<ThrowRuntimeExAndDefault>();
        let plan = registry_network_policy(network_policy_code as u32)
            .filter(|_| {
                allocation_unit_bytes > 0
                    && destination_total_usable_bytes > 0
                    && measured_free_bytes >= 0
            })
            .map_or_else(
                || ObMobileRegistryPlan::error(OB_MOBILE_RUNTIME_INVALID_REGISTRY_INIT),
                |network_policy| {
                    confirm_registry_init(
                        &operation_id,
                        &manifest_digest,
                        &trust_profile,
                        network_policy,
                        one_time_network_override != JNI_FALSE,
                        allocation_unit_bytes as u64,
                        destination_total_usable_bytes as u64,
                        measured_free_bytes as u64,
                    )
                },
            );
        let encoded = encode_registry_plan(&plan);
        unowned_env
            .with_env(|env| JString::from_str(env, &encoded))
            .resolve::<ThrowRuntimeExAndDefault>()
    }

    #[jni_mangle(
        "org.onebrain.onebrain_mobile.RustMobileBridge",
        "nativeRuntimeSaveRawTextDraft"
    )]
    pub fn native_runtime_save_raw_text_draft<'caller>(
        mut unowned_env: EnvUnowned<'caller>,
        _class: JClass<'caller>,
        content_language: JString<'caller>,
        content: JString<'caller>,
    ) -> JString<'caller> {
        let language = unowned_env
            .with_env(|env| content_language.try_to_string(env))
            .resolve::<ThrowRuntimeExAndDefault>();
        let text = unowned_env
            .with_env(|env| content.try_to_string(env))
            .resolve::<ThrowRuntimeExAndDefault>();
        let receipt = save_raw_text_draft(&language, text.as_bytes());
        let draft_ref = if receipt.status_code == super::OB_MOBILE_RUNTIME_OK {
            std::str::from_utf8(&receipt.draft_ref[..receipt.draft_ref_len as usize]).unwrap_or("")
        } else {
            ""
        };
        unowned_env
            .with_env(|env| JString::from_str(env, draft_ref))
            .resolve::<ThrowRuntimeExAndDefault>()
    }

    #[jni_mangle(
        "org.onebrain.onebrain_mobile.RustMobileBridge",
        "nativeRuntimeEnqueueSharedText"
    )]
    pub fn native_runtime_enqueue_shared_text<'caller>(
        mut unowned_env: EnvUnowned<'caller>,
        _class: JClass<'caller>,
        callback_token: JString<'caller>,
        mime_type: JString<'caller>,
        content: JString<'caller>,
    ) -> JString<'caller> {
        let token = unowned_env
            .with_env(|env| callback_token.try_to_string(env))
            .resolve::<ThrowRuntimeExAndDefault>();
        let mime = unowned_env
            .with_env(|env| mime_type.try_to_string(env))
            .resolve::<ThrowRuntimeExAndDefault>();
        let text = unowned_env
            .with_env(|env| content.try_to_string(env))
            .resolve::<ThrowRuntimeExAndDefault>();
        let receipt = enqueue_shared_text(&token, &mime, text.as_bytes());
        let spool_ref = if receipt.status_code == super::OB_MOBILE_RUNTIME_OK {
            std::str::from_utf8(&receipt.spool_ref[..receipt.spool_ref_len as usize]).unwrap_or("")
        } else {
            ""
        };
        unowned_env
            .with_env(|env| JString::from_str(env, spool_ref))
            .resolve::<ThrowRuntimeExAndDefault>()
    }

    #[jni_mangle(
        "org.onebrain.onebrain_mobile.RustMobileBridge",
        "nativeRuntimePendingShareSpoolEntry"
    )]
    pub fn native_runtime_pending_share_spool_entry<'caller>(
        mut unowned_env: EnvUnowned<'caller>,
        _class: JClass<'caller>,
        index: jint,
    ) -> JString<'caller> {
        let encoded = if index < 0 {
            String::new()
        } else {
            let summary = pending_share_spool_at(index as usize);
            if summary.status_code == super::OB_MOBILE_RUNTIME_OK {
                let spool_ref =
                    std::str::from_utf8(&summary.spool_ref[..summary.spool_ref_len as usize])
                        .unwrap_or("");
                let mime_type =
                    std::str::from_utf8(&summary.mime_type[..summary.mime_type_len as usize])
                        .unwrap_or("");
                format!(
                    "{spool_ref}|{mime_type}|{}|{}",
                    summary.content_bytes, summary.received_at_monotonic_ms
                )
            } else {
                String::new()
            }
        };
        unowned_env
            .with_env(|env| JString::from_str(env, &encoded))
            .resolve::<ThrowRuntimeExAndDefault>()
    }

    #[jni_mangle(
        "org.onebrain.onebrain_mobile.RustMobileBridge",
        "nativeRuntimeImportSharedText"
    )]
    pub fn native_runtime_import_shared_text<'caller>(
        mut unowned_env: EnvUnowned<'caller>,
        _class: JClass<'caller>,
        spool_ref: JString<'caller>,
        content_language: JString<'caller>,
    ) -> JString<'caller> {
        let reference = unowned_env
            .with_env(|env| spool_ref.try_to_string(env))
            .resolve::<ThrowRuntimeExAndDefault>();
        let language = unowned_env
            .with_env(|env| content_language.try_to_string(env))
            .resolve::<ThrowRuntimeExAndDefault>();
        let receipt = import_shared_text(&reference, &language);
        let draft_ref = if receipt.status_code == super::OB_MOBILE_RUNTIME_OK {
            std::str::from_utf8(&receipt.draft_ref[..receipt.draft_ref_len as usize]).unwrap_or("")
        } else {
            ""
        };
        unowned_env
            .with_env(|env| JString::from_str(env, draft_ref))
            .resolve::<ThrowRuntimeExAndDefault>()
    }

    #[jni_mangle(
        "org.onebrain.onebrain_mobile.RustMobileBridge",
        "nativeRuntimeStartMediaStage"
    )]
    pub fn native_runtime_start_media_stage<'caller>(
        mut unowned_env: EnvUnowned<'caller>,
        _class: JClass<'caller>,
        requested_class: JString<'caller>,
        declared_mime_type: JString<'caller>,
    ) -> JString<'caller> {
        let requested_class = unowned_env
            .with_env(|env| requested_class.try_to_string(env))
            .resolve::<ThrowRuntimeExAndDefault>();
        let declared_mime_type = unowned_env
            .with_env(|env| declared_mime_type.try_to_string(env))
            .resolve::<ThrowRuntimeExAndDefault>();
        let receipt = start_media_stage(&requested_class, &declared_mime_type);
        let source_ref = if receipt.status_code == super::OB_MOBILE_RUNTIME_OK {
            std::str::from_utf8(&receipt.source_ref[..receipt.source_ref_len as usize])
                .unwrap_or("")
        } else {
            ""
        };
        unowned_env
            .with_env(|env| JString::from_str(env, source_ref))
            .resolve::<ThrowRuntimeExAndDefault>()
    }

    #[jni_mangle(
        "org.onebrain.onebrain_mobile.RustMobileBridge",
        "nativeRuntimeAppendMediaStage"
    )]
    pub fn native_runtime_append_media_stage<'caller>(
        mut unowned_env: EnvUnowned<'caller>,
        _class: JClass<'caller>,
        source_ref: JString<'caller>,
        chunk: JByteArray<'caller>,
    ) -> jint {
        let source_ref = unowned_env
            .with_env(|env| source_ref.try_to_string(env))
            .resolve::<ThrowRuntimeExAndDefault>();
        let mut bytes = unowned_env
            .with_env(|env| env.convert_byte_array(&chunk))
            .resolve::<ThrowRuntimeExAndDefault>();
        let status = append_media_stage(&source_ref, &bytes);
        bytes.zeroize();
        status as jint
    }

    #[jni_mangle(
        "org.onebrain.onebrain_mobile.RustMobileBridge",
        "nativeRuntimeFinishMediaStage"
    )]
    pub fn native_runtime_finish_media_stage<'caller>(
        mut unowned_env: EnvUnowned<'caller>,
        _class: JClass<'caller>,
        source_ref: JString<'caller>,
    ) -> JString<'caller> {
        let source_ref = unowned_env
            .with_env(|env| source_ref.try_to_string(env))
            .resolve::<ThrowRuntimeExAndDefault>();
        let receipt = finish_media_stage(&source_ref);
        let encoded = if receipt.status_code == super::OB_MOBILE_RUNTIME_OK {
            let source_ref =
                std::str::from_utf8(&receipt.source_ref[..receipt.source_ref_len as usize])
                    .unwrap_or("");
            let media_class =
                std::str::from_utf8(&receipt.media_class[..receipt.media_class_len as usize])
                    .unwrap_or("");
            let mime_type =
                std::str::from_utf8(&receipt.mime_type[..receipt.mime_type_len as usize])
                    .unwrap_or("");
            let digest =
                std::str::from_utf8(&receipt.blake3_digest[..receipt.blake3_digest_len as usize])
                    .unwrap_or("");
            format!(
                "{source_ref}|{media_class}|{mime_type}|{}|{digest}",
                receipt.content_bytes
            )
        } else {
            String::new()
        };
        unowned_env
            .with_env(|env| JString::from_str(env, &encoded))
            .resolve::<ThrowRuntimeExAndDefault>()
    }

    #[jni_mangle(
        "org.onebrain.onebrain_mobile.RustMobileBridge",
        "nativeRuntimeFinishOwnedMediaImport"
    )]
    pub fn native_runtime_finish_owned_media_import<'caller>(
        mut unowned_env: EnvUnowned<'caller>,
        _class: JClass<'caller>,
        source_ref: JString<'caller>,
    ) -> JString<'caller> {
        let source_ref = unowned_env
            .with_env(|env| source_ref.try_to_string(env))
            .resolve::<ThrowRuntimeExAndDefault>();
        let encoded = finish_owned_media_import(&source_ref)
            .as_ref()
            .map(encode_owned_media)
            .unwrap_or_default();
        unowned_env
            .with_env(|env| JString::from_str(env, &encoded))
            .resolve::<ThrowRuntimeExAndDefault>()
    }

    #[jni_mangle(
        "org.onebrain.onebrain_mobile.RustMobileBridge",
        "nativeRuntimeOwnedMediaCount"
    )]
    pub fn native_runtime_owned_media_count(_env: EnvUnowned<'_>, _class: JClass<'_>) -> jlong {
        owned_media_count() as jlong
    }

    #[jni_mangle(
        "org.onebrain.onebrain_mobile.RustMobileBridge",
        "nativeRuntimeOwnedMediaEntry"
    )]
    pub fn native_runtime_owned_media_entry<'caller>(
        mut unowned_env: EnvUnowned<'caller>,
        _class: JClass<'caller>,
        index: jint,
    ) -> JString<'caller> {
        let encoded = usize::try_from(index)
            .ok()
            .and_then(owned_media)
            .as_ref()
            .map(encode_owned_media)
            .unwrap_or_default();
        unowned_env
            .with_env(|env| JString::from_str(env, &encoded))
            .resolve::<ThrowRuntimeExAndDefault>()
    }

    #[jni_mangle(
        "org.onebrain.onebrain_mobile.RustMobileBridge",
        "nativeRuntimeAbortMediaStage"
    )]
    pub fn native_runtime_abort_media_stage<'caller>(
        mut unowned_env: EnvUnowned<'caller>,
        _class: JClass<'caller>,
        source_ref: JString<'caller>,
    ) -> jint {
        let source_ref = unowned_env
            .with_env(|env| source_ref.try_to_string(env))
            .resolve::<ThrowRuntimeExAndDefault>();
        abort_media_stage(&source_ref) as jint
    }

    #[jni_mangle(
        "org.onebrain.onebrain_mobile.RustMobileBridge",
        "nativeRuntimeSetOnboardingCursor"
    )]
    pub fn native_runtime_set_onboarding_cursor(
        _env: EnvUnowned<'_>,
        _class: JClass<'_>,
        cursor_code: jint,
    ) -> jint {
        if cursor_code < 0 {
            return super::OB_MOBILE_RUNTIME_INVALID_ONBOARDING_CURSOR as jint;
        }
        ob_mobile_runtime_set_onboarding_cursor(cursor_code as u32) as jint
    }

    #[jni_mangle(
        "org.onebrain.onebrain_mobile.RustMobileBridge",
        "nativeRuntimeProcessGeneration"
    )]
    pub fn native_runtime_process_generation(_env: EnvUnowned<'_>, _class: JClass<'_>) -> jlong {
        runtime_snapshot().process_generation as jlong
    }

    #[jni_mangle(
        "org.onebrain.onebrain_mobile.RustMobileBridge",
        "nativeRuntimeActivationPhase"
    )]
    pub fn native_runtime_activation_phase(_env: EnvUnowned<'_>, _class: JClass<'_>) -> jint {
        runtime_snapshot().activation_phase as jint
    }

    #[jni_mangle(
        "org.onebrain.onebrain_mobile.RustMobileBridge",
        "nativeRuntimeActiveGrantCount"
    )]
    pub fn native_runtime_active_grant_count(_env: EnvUnowned<'_>, _class: JClass<'_>) -> jint {
        runtime_snapshot().active_grant_count as jint
    }

    #[jni_mangle(
        "org.onebrain.onebrain_mobile.RustMobileBridge",
        "nativeRuntimeEncryptedRawDraftCount"
    )]
    pub fn native_runtime_encrypted_raw_draft_count(
        _env: EnvUnowned<'_>,
        _class: JClass<'_>,
    ) -> jlong {
        runtime_snapshot().encrypted_raw_draft_count as jlong
    }

    #[jni_mangle(
        "org.onebrain.onebrain_mobile.RustMobileBridge",
        "nativeRuntimePendingShareSpoolCount"
    )]
    pub fn native_runtime_pending_share_spool_count(
        _env: EnvUnowned<'_>,
        _class: JClass<'_>,
    ) -> jlong {
        runtime_snapshot().pending_share_spool_count as jlong
    }

    #[jni_mangle(
        "org.onebrain.onebrain_mobile.RustMobileBridge",
        "nativeRuntimeStagedVerifiedMediaCount"
    )]
    pub fn native_runtime_staged_verified_media_count(
        _env: EnvUnowned<'_>,
        _class: JClass<'_>,
    ) -> jlong {
        runtime_snapshot().staged_verified_media_count as jlong
    }

    #[jni_mangle(
        "org.onebrain.onebrain_mobile.RustMobileBridge",
        "nativeRuntimeOnboardingCursor"
    )]
    pub fn native_runtime_onboarding_cursor(_env: EnvUnowned<'_>, _class: JClass<'_>) -> jint {
        runtime_snapshot().onboarding_cursor as jint
    }

    macro_rules! boolean_runtime_getter {
        ($rust_name:ident, $java_name:literal, $field:ident) => {
            #[jni_mangle("org.onebrain.onebrain_mobile.RustMobileBridge", $java_name)]
            pub fn $rust_name(_env: EnvUnowned<'_>, _class: JClass<'_>) -> jboolean {
                if runtime_snapshot().$field == 0 {
                    JNI_FALSE
                } else {
                    JNI_TRUE
                }
            }
        };
    }

    boolean_runtime_getter!(
        native_runtime_recovered_unclean_start,
        "nativeRuntimeRecoveredUncleanStart",
        recovered_unclean_start
    );
    boolean_runtime_getter!(
        native_runtime_bootstrap_store_opened,
        "nativeRuntimeBootstrapStoreOpened",
        bootstrap_store_opened
    );
    boolean_runtime_getter!(
        native_runtime_registry_bootstrap_only,
        "nativeRuntimeRegistryBootstrapOnly",
        registry_bootstrap_only
    );
    boolean_runtime_getter!(
        native_runtime_local_kql_fixture_verified,
        "nativeRuntimeLocalKqlFixtureVerified",
        local_kql_fixture_verified
    );
    boolean_runtime_getter!(
        native_runtime_private_planner_verified,
        "nativeRuntimePrivatePlannerVerified",
        private_planner_verified
    );
    boolean_runtime_getter!(
        native_runtime_no_llm_provider,
        "nativeRuntimeNoLlmProvider",
        no_llm_provider
    );
    boolean_runtime_getter!(
        native_runtime_stale_callback_rejected,
        "nativeRuntimeStaleCallbackRejected",
        stale_callback_rejected
    );
    boolean_runtime_getter!(
        native_runtime_secure_profile_active,
        "nativeRuntimeSecureProfileActive",
        secure_profile_active
    );
    boolean_runtime_getter!(
        native_runtime_installation_binding_verified,
        "nativeRuntimeInstallationBindingVerified",
        installation_binding_verified
    );
    boolean_runtime_getter!(
        native_runtime_installation_created,
        "nativeRuntimeInstallationCreated",
        installation_created
    );
    boolean_runtime_getter!(
        native_runtime_security_session_unlocked,
        "nativeRuntimeSecuritySessionUnlocked",
        security_session_unlocked
    );
    boolean_runtime_getter!(
        native_runtime_private_vault_ready,
        "nativeRuntimePrivateVaultReady",
        private_vault_ready
    );
    boolean_runtime_getter!(
        native_runtime_identity_domains_separated,
        "nativeRuntimeIdentityDomainsSeparated",
        identity_domains_separated
    );
    boolean_runtime_getter!(
        native_runtime_privacy_defaults_fail_safe,
        "nativeRuntimePrivacyDefaultsFailSafe",
        privacy_defaults_fail_safe
    );
    boolean_runtime_getter!(
        native_runtime_redacted_history_ready,
        "nativeRuntimeRedactedHistoryReady",
        redacted_history_ready
    );

    fn encode_registry_plan(plan: &ObMobileRegistryPlan) -> String {
        if plan.status_code != OB_MOBILE_RUNTIME_OK {
            return format!("ERR:{}", plan.status_code);
        }
        let operation_id =
            std::str::from_utf8(&plan.operation_id[..plan.operation_id_len as usize]).unwrap_or("");
        let channel_id =
            std::str::from_utf8(&plan.channel_id[..plan.channel_id_len as usize]).unwrap_or("");
        let release_id =
            std::str::from_utf8(&plan.release_id[..plan.release_id_len as usize]).unwrap_or("");
        let manifest_digest =
            std::str::from_utf8(&plan.manifest_digest[..plan.manifest_digest_len as usize])
                .unwrap_or("");
        let trust_profile_digest = std::str::from_utf8(
            &plan.trust_profile_digest[..plan.trust_profile_digest_len as usize],
        )
        .unwrap_or("");
        format!(
            "{operation_id}|{}|{channel_id}|{release_id}|{manifest_digest}|{trust_profile_digest}|{}|{}|{}|{}|{}|{}|{}|{}|{}|{}|{}|{}|{}",
            plan.state_code,
            plan.head_generation,
            plan.release_sequence,
            plan.publisher_min_additional_free_bytes,
            plan.artifact_total_bytes,
            plan.target_total_alloc_bytes,
            plan.transfer_initial_bytes,
            plan.verification_workspace_bytes,
            plan.catalog_growth_bytes,
            plan.safety_reserve_bytes,
            plan.destination_total_usable_bytes,
            plan.measured_free_bytes,
            plan.initial_required_free_bytes,
            plan.admitted,
        )
    }
}

#[cfg(test)]
mod tests {
    use std::ffi::CStr;

    use super::*;

    #[test]
    fn abi_and_version_are_bounded_and_stable() {
        assert_eq!(ob_mobile_bridge_abi_version(), 8);
        let version = unsafe { CStr::from_ptr(ob_mobile_bridge_core_version()) };
        assert_eq!(version.to_str().expect("version UTF-8"), "0.1.0");
        assert!(version.to_bytes().len() <= 32);
    }

    #[test]
    fn bootstrap_bridge_never_requests_registry_data() {
        assert_eq!(ob_mobile_bridge_registry_request_issued(), 0);
    }

    #[test]
    fn round_trip_preserves_full_u64_domain() {
        for nonce in [0, 1, u32::MAX as u64, i64::MAX as u64, u64::MAX] {
            assert_eq!(ob_mobile_bridge_round_trip(nonce), nonce);
        }
    }

    #[test]
    fn invalid_runtime_paths_are_rejected_without_dereferencing() {
        let snapshot = unsafe { ob_mobile_runtime_open_utf8(std::ptr::null(), 1) };
        assert_eq!(snapshot.status_code, OB_MOBILE_RUNTIME_INVALID_PATH);
    }
}

//! Stable native bridge for the autonomous OneBrain mobile runtime profile.
//!
//! Native code owns platform paths and execution opportunities. Rust owns the
//! runtime singleton, bootstrap database, activation grants, local KQL smoke
//! and callback-generation fence. No path or database handle crosses to Dart.

use std::{
    ffi::c_char,
    path::PathBuf,
    slice, str,
    sync::{Mutex, OnceLock},
};

use onebrain_mobile_core::{
    ActivationPhase, AppLockPolicy, MobileFeatureFlags, MobileRuntimeFacade, MobileRuntimeSnapshot,
    OnboardingCursor, ResourceBudgets, RuntimeServices, SecurityBootstrapMaterial,
    SecuritySessionState,
};

/// Stable ABI revision understood by the current Swift/Kotlin adapters.
pub const OB_MOBILE_BRIDGE_ABI_VERSION: u32 = 6;

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

const CORE_VERSION: &[u8] = concat!(env!("CARGO_PKG_VERSION"), "\0").as_bytes();

static RUNTIME: OnceLock<Mutex<Option<MobileRuntimeFacade>>> = OnceLock::new();

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
    0
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
        enqueue_shared_text, import_shared_text, ob_mobile_bridge_abi_version,
        ob_mobile_bridge_registry_request_issued, ob_mobile_bridge_round_trip,
        ob_mobile_runtime_lock_private_node, ob_mobile_runtime_set_onboarding_cursor, open_runtime,
        open_runtime_secured, pending_share_spool_at, runtime_snapshot, save_raw_text_draft,
        CORE_VERSION,
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
}

#[cfg(test)]
mod tests {
    use std::ffi::CStr;

    use super::*;

    #[test]
    fn abi_and_version_are_bounded_and_stable() {
        assert_eq!(ob_mobile_bridge_abi_version(), 6);
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

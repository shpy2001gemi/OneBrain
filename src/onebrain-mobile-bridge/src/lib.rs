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
    ActivationPhase, MobileFeatureFlags, MobileRuntimeFacade, MobileRuntimeSnapshot,
    ResourceBudgets, RuntimeServices,
};

/// Stable ABI revision understood by the current Swift/Kotlin adapters.
pub const OB_MOBILE_BRIDGE_ABI_VERSION: u32 = 2;

pub const OB_MOBILE_RUNTIME_OK: u32 = 0;
pub const OB_MOBILE_RUNTIME_INVALID_PATH: u32 = 1;
pub const OB_MOBILE_RUNTIME_CORE_ERROR: u32 = 2;
pub const OB_MOBILE_RUNTIME_LOCK_POISONED: u32 = 3;
pub const OB_MOBILE_RUNTIME_NOT_OPEN: u32 = 4;

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
        }
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
    let bytes = unsafe { slice::from_raw_parts(path, path_len) };
    let path = match str::from_utf8(bytes) {
        Ok(path) if !path.is_empty() => PathBuf::from(path),
        _ => return ObMobileRuntimeSnapshot::error(OB_MOBILE_RUNTIME_INVALID_PATH),
    };
    open_runtime(path)
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
        objects::{JClass, JString},
        sys::{jboolean, jint, jlong, JNI_FALSE, JNI_TRUE},
        EnvUnowned,
    };

    use super::{
        ob_mobile_bridge_abi_version, ob_mobile_bridge_registry_request_issued,
        ob_mobile_bridge_round_trip, open_runtime, runtime_snapshot, CORE_VERSION,
    };

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
}

#[cfg(test)]
mod tests {
    use std::ffi::CStr;

    use super::*;

    #[test]
    fn abi_and_version_are_bounded_and_stable() {
        assert_eq!(ob_mobile_bridge_abi_version(), 2);
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

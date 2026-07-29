//! Narrow native bridge used to prove the OneBrain Mobile call topology.
//!
//! This crate deliberately owns no database, Registry transfer, identity, or
//! network behavior. MOB-02 will place the activation arbiter and runtime
//! facade behind this stable boundary.

use std::ffi::c_char;

/// Stable ABI revision understood by the current Swift/Kotlin adapters.
pub const OB_MOBILE_BRIDGE_ABI_VERSION: u32 = 1;

const CORE_VERSION: &[u8] = concat!(env!("CARGO_PKG_VERSION"), "\0").as_bytes();

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
/// This is always false in MOB-01. Registry transfer authority is introduced
/// only behind the later explicit Init contract.
#[unsafe(no_mangle)]
pub extern "C" fn ob_mobile_bridge_registry_request_issued() -> u8 {
    0
}

/// Bounded deterministic call used to verify the complete generated call path.
#[unsafe(no_mangle)]
pub extern "C" fn ob_mobile_bridge_round_trip(nonce: u64) -> u64 {
    nonce
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
        ob_mobile_bridge_round_trip, CORE_VERSION,
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
}

#[cfg(test)]
mod tests {
    use std::ffi::CStr;

    use super::*;

    #[test]
    fn abi_and_version_are_bounded_and_stable() {
        assert_eq!(ob_mobile_bridge_abi_version(), 1);
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
}

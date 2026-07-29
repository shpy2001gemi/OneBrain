# MOB-01 Rust bridge

This slice proves the bounded production topology without claiming that the
mobile runtime is ready:

```text
Flutter typed intent
  -> generated Pigeon API
  -> Swift/Kotlin NativeHost
  -> generated C header / jni-rs wrapper
  -> onebrain-mobile-bridge
```

The bridge owns no database, Registry transfer, identity, signing, tool,
network, or LLM behavior. Those authorities remain unavailable until their
implementation packages and gates exist.

## Package-first toolchain

| Boundary | Maintained package/tool | Pinned version | Purpose |
|---|---|---:|---|
| Flutter to native | Pigeon | `27.3.0` | Generate Dart, Kotlin and Swift APIs |
| Kotlin to Rust | `jni` crate | `0.22.4` | FFI-safe JNI environment and name mangling |
| Android Rust build | `cargo-ndk` | `4.1.2` | NDK discovery, target configuration and `jniLibs` layout |
| Swift to Rust | `cbindgen` | `0.29.4` | Generate the checked-in C header from Rust exports |

Application code does not recreate channel serialization, JNI environment
handling, NDK linker discovery, or C declaration generation.

## ABI and thread ownership

- ABI revision `1` exposes only bounded primitive facts and a deterministic
  nonce round trip.
- Returned version text points to immutable process-lifetime storage and is
  never freed by native code.
- Pigeon host callbacks currently execute on the platform host path. MOB-01
  Rust calls are constant-time and perform no I/O, locking, allocation across
  ownership boundaries, or callback.
- Long-running runtime work must later enter the Rust facade with request ID,
  deadline and cancellation; it must never block the Flutter/UI thread.
- Android uses the main app process and a package-provided FFI-safe JNI wrapper.
- iOS links a static library and calls the same stable C symbols from Swift.
- Missing Android libraries degrade to `rustCoreLinked=false`; the app does not
  invent node readiness or crash merely because a developer skipped the Rust
  build step.

## Build

From `src/onebrain-mobile`:

```text
cargo install cargo-ndk --version 4.1.2 --locked
cargo install cbindgen --version 0.29.4 --locked
python tool/generate_rust_bridge_header.py
python tool/build_rust_android.py
flutter build apk --debug
```

On macOS:

```text
bash tool/build_rust_ios.sh
flutter build ios --simulator --debug
```

The Android script builds `arm64-v8a` and `x86_64`. The iOS script builds an
arm64 device archive plus a universal arm64/x86_64 simulator archive. Generated
binary artifacts are ignored; CI rebuilds them from source.

## Fallback

If `cargo-ndk` fails, keep the same Rust crate and JNI exports and replace only
the build orchestration with explicit NDK linker configuration or a maintained
Cargokit integration. If `cbindgen` becomes unavailable, retain the last
generated, ABI-tested header while evaluating a maintained generator.

The fallback must not move durable authority into Dart or make direct Dart FFI
the only entry path: OS callbacks must still reach Rust through NativeHost when
no Flutter engine exists.

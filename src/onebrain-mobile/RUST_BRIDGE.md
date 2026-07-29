# MOB-02 mobile runtime bridge

This slice proves the bounded production topology without claiming that the
mobile runtime is ready:

```text
Flutter typed intent
  -> generated Pigeon API
  -> Swift/Kotlin NativeHost
  -> generated C header / jni-rs wrapper
  -> onebrain-mobile-bridge
  -> onebrain-mobile-core
```

The bridge owns no product policy. `onebrain-mobile-core` owns the bounded
`bootstrap.redb` operational ledger, process-generation lifecycle, execution
grants, callback commit fence, deterministic local KQL and LocalOnly private
planning smokes. Registry
network transfer, identity provisioning, signing, product tools, seeding and
all LLM providers remain unavailable until their implementation packages and
gates exist.

## Package-first toolchain

| Boundary | Maintained package/tool | Pinned version | Purpose |
|---|---|---:|---|
| Flutter to native | Pigeon | `27.3.0` | Generate Dart, Kotlin and Swift APIs |
| Kotlin to Rust | `jni` crate | `0.22.4` | FFI-safe JNI environment and name mangling |
| Android Rust build | `cargo-ndk` | `4.1.2` | NDK discovery, target configuration and `jniLibs` layout |
| Swift to Rust | `cbindgen` | `0.29.4` | Generate the checked-in C header from Rust exports |
| Bootstrap state | `redb` | `2.6.3` | Pure-Rust ACID process, operation, chunk and transfer ledger |
| Fixture signatures | `ed25519-dalek` | `2.2.0` | Verify the pinned local KQL smoke fixture |

Application code does not recreate channel serialization, JNI environment
handling, NDK linker discovery, or C declaration generation.

## ABI and thread ownership

- ABI revision `2` adds a fixed-layout runtime snapshot and native-owned path
  open call while retaining bounded primitive facts and the deterministic
  nonce round trip.
- Returned version text points to immutable process-lifetime storage and is
  never freed by native code.
- Kotlin and Swift open the runtime on a dedicated serial native queue and
  deliver Pigeon completion on the platform main thread, so redb recovery and
  local KQL do not block Flutter/UI work.
- Platform paths never cross into Dart. Repeated opens in one process return
  the existing runtime generation.
- Long-running work enters the Rust facade with a bounded execution grant,
  deadline and cancellation. The current foreground grant has no network
  scope.
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
python tool/verify_mobile_rust_dependency_graph.py
flutter build apk --debug
python tool/verify_android_runtime_recovery.py \
  build/app/outputs/flutter-apk/app-debug.apk
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

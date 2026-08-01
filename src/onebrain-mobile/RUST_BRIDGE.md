# MOB-05B mobile runtime and durable transfer bridge

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
grants, callback commit fence, deterministic local KQL, LocalOnly private
planning, exact installation binding, independent typed signer domains,
encrypted private-vault session, encrypted raw-draft/share-spool store, durable
onboarding cursor and portable encrypted-archive profile.
The Android system-picker lane additionally streams native-owned provider
content into Rust-owned encrypted media staging without exposing URI/path or
source bytes to Dart. ABI 9 adds the Rust-owned Registry
`SchedulePrepared -> TransferSubmitted -> TransferAdopted` barrier, stable
request/descriptor fingerprints, Android job ID, process-generation receipts
and conservative stop recovery. Actual Registry network transfer, product
tools, seeding and all LLM providers remain unavailable until their transport
authority and implementation gates exist.

## Package-first toolchain

| Boundary | Maintained package/tool | Pinned version | Purpose |
|---|---|---:|---|
| Flutter to native | Pigeon | `27.3.0` | Generate Dart, Kotlin and Swift APIs |
| Kotlin to Rust | `jni` crate | `0.22.4` | FFI-safe JNI environment and name mangling |
| Android Rust build | `cargo-ndk` | `4.1.2` | NDK discovery, target configuration and `jniLibs` layout |
| Swift to Rust | `cbindgen` | `0.29.4` | Generate the checked-in C header from Rust exports |
| Bootstrap state | `redb` | `2.6.3` | Pure-Rust ACID process, operation, chunk and transfer ledger |
| Fixture signatures | `ed25519-dalek` | `2.2.0` | Verify the pinned local KQL smoke fixture |
| Private vault and archive AEAD | existing `ku-core` vault + `chacha20poly1305` | workspace / `0.11.0` | Reuse validated private storage and authenticated chunk encryption |
| Secret cleanup | `zeroize` | `1.9.0` | Erase temporary native/Rust key buffers |
| OS entropy | `getrandom` | `0.3.4` | Recovery/archive nonce generation |
| Media magic-byte classification | `infer` | `0.22.0` | Verify selected bytes without trusting filename/extension/provider MIME |

Application code does not recreate channel serialization, JNI environment
handling, NDK linker discovery, or C declaration generation.

## ABI and thread ownership

- ABI revision `9` preserves ABI 8 signed Registry Init admission and adds
  bounded native-only prepare, submit, adopt and missing-task recovery calls.
  No URL, filesystem path, credential or OS handle crosses into Dart. ABI 7
  includes the opaque encrypted share-spool commands from ABI 6 and adds
  bounded native-to-Rust media-stage start/append/finish/abort plus a
  verified-stage count. The completed receipt contains only source ref, media
  class, verified MIME, byte count and BLAKE3 digest. ABI 6 added opaque
  encrypted share-spool summaries and idempotent
  text import to the protected-runtime snapshot, native-owned secure-open and
  private-draft calls, durable onboarding cursor and explicit private-session
  lock. No share plaintext or filesystem path crosses to Dart.
- Returned version text points to immutable process-lifetime storage and is
  never freed by native code.
- Kotlin and Swift open the runtime on a dedicated serial native queue and
  deliver Pigeon completion on the platform main thread, so redb recovery and
  local KQL do not block Flutter/UI work.
- Platform paths never cross into Dart. Repeated opens in one process return
  the existing runtime generation; protected material travels native-to-Rust
  only and is zeroized after open.
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
flutter test integration_test/native_host_bridge_test.dart -d emulator-5554
# Rebuild because integration_test replaces app-debug.apk with its test target.
flutter build apk --debug
python tool/verify_android_share_intent.py \
  build/app/outputs/flutter-apk/app-debug.apk
python tool/verify_android_runtime_recovery.py \
  build/app/outputs/flutter-apk/app-debug.apk
python tool/verify_android_install_binding_fail_closed.py \
  build/app/outputs/flutter-apk/app-debug.apk
python tool/verify_android_media_picker.py \
  build/app/outputs/flutter-apk/app-debug.apk
```

On macOS:

```text
bash tool/build_rust_ios.sh
flutter build ios --simulator --debug
```

The Android script builds `armeabi-v7a`, `arm64-v8a` and `x86_64` so every ABI
packaged by Flutter carries the same Rust authority bridge. The package scanner
fails closed on an ABI mismatch. The iOS script builds an arm64 device archive
plus a universal arm64/x86_64 simulator archive. Generated binary artifacts are
ignored; CI rebuilds them from source.

## Fallback

If `cargo-ndk` fails, keep the same Rust crate and JNI exports and replace only
the build orchestration with explicit NDK linker configuration or a maintained
Cargokit integration. If `cbindgen` becomes unavailable, retain the last
generated, ABI-tested header while evaluating a maintained generator.

The fallback must not move durable authority into Dart or make direct Dart FFI
the only entry path: OS callbacks must still reach Rust through NativeHost when
no Flutter engine exists.

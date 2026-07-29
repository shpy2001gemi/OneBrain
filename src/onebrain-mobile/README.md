# onebrain-mobile

> OneBrain Mobile — autonomous iOS and Android node.
>
> Status: **MOB-04 private Limited shell in progress; production capability and
> `ReadyOffline` are not claimed**

## Target

OneBrain Mobile is not a desktop companion or a thin client to
`onebrain-api`. The installation owns its NodeID/signing domains, private vault,
local knowledge, Concept Registry release, journals, media, and intermittent
network lifecycle.

Target stack:

```text
Flutter UI
  -> generated Pigeon API
  -> Swift/Kotlin NativeHost
  -> stable C ABI/JNI
  -> autonomous Rust mobile core
```

The product remains useful offline with every LLM and network lane disabled.
Optional device, app-managed, or remote LLM providers return candidate data;
Rust policy and deterministic handlers own every tool and durable side effect.

## Specifications

- [Implementation plan](../../docs/research/WIP_MOBILE_APP_IMPLEMENTATION_PLAN_V1.md)
- [Technical architecture](../../docs/research/WIP_MOBILE_APP_TECHNICAL_ARCHITECTURE_V1.md)
- [Feature tree](../../docs/features/mobile/MOBILE_APP_FEATURE_TREE_V1.md)
- [Feature details](../../docs/features/mobile/MOBILE_APP_FEATURE_DETAILS_V1.md)
- [App sitemap](../../docs/features/mobile/MOBILE_APP_SITEMAP_V1.md)
- [Mobile Design System](../../docs/design/mobile/MOBILE_DESIGN_SYSTEM_V1.md)
- [Component catalog](../../docs/design/mobile/MOBILE_COMPONENT_CATALOG_V1.md)
- [Screen patterns](../../docs/design/mobile/MOBILE_SCREEN_PATTERNS_V1.md)
- [Design tokens](../../docs/design/mobile/tokens/mobile_design_tokens_v1.json)
- [Mandatory build compliance harness](../../docs/design/mobile/MOBILE_BUILD_HARNESS_V1.md)

When these documents conflict with the distributed-runtime plan, the
distributed-runtime plan remains authoritative.

Before adding a Flutter scaffold or implementation code, follow
[`AGENTS.md`](./AGENTS.md) and run:

```text
python scripts/ci/validate_mobile_build_contracts.py
```

## Current foundation slice

The checked-in app now includes:

- Android/iOS Flutter scaffolds;
- generated semantic tokens and Material 3 theme extensions;
- shared catalog widgets instead of screen-local controls;
- generated English/Vietnamese localization with a package-backed durable UI
  preference;
- pinned offline Nunito Sans, Roboto Mono and Material Symbols Rounded assets
  with license/hash verification;
- `go_router` entry/onboarding and five-destination adaptive shell routes;
- Riverpod-owned async presentation state;
- one Pigeon schema that generates Dart, Kotlin and Swift host APIs with an
  async capability call, bounded feasibility operation, event stream and
  cancellation.

The native snapshot now crosses the generated Pigeon API, Swift/Kotlin host,
the Rust C ABI/JNI bridge and `onebrain-mobile-core`. ABI revision `7` includes a
platform-protected installation session, exact epoch/nonce authority binding,
three independent signer domains, an encrypted private vault, encrypted
Limited-mode raw drafts and a durable onboarding resume cursor to the bounded
`BootstrapOnly` runtime, plus opaque encrypted share-spool inspection and
idempotent import. It also adds Android system-picker streaming into encrypted
Rust media staging with a verified MIME/length/BLAKE3 opaque receipt. It still
reports that no Registry request has been issued.

See [`PACKAGE_POLICY.md`](./PACKAGE_POLICY.md) for the package-first and
shared-widget contract, and [`RUST_BRIDGE.md`](./RUST_BRIDGE.md) for ABI,
thread-ownership, build and fallback details.

The current automated evidence additionally covers:

- thirteen Windows goldens spanning light/dark, high contrast,
  English/Vietnamese, 200% text, reduced motion, compact/large/expanded
  layouts, the runtime status card, Limited Home and private text capture;
- Android 16 integration of async `started`, `cancelled` and `completed`
  events, idempotent cancellation, and the bounded `HOST_INVALID_DELAY` error;
- a release APK without Registry/model payloads and without Android network
  permission;
- static Dart/Kotlin/Swift/Rust bootstrap isolation from transport APIs.

These are MOB-01 feasibility gates. They do not authorize Registry Init,
background seeding or a product network lane.

MOB-02 virtual-device evidence additionally covers:

- `RuntimeServices` injection with no signer, network, scheduler or LLM
  availability in the default profile;
- process-generation recovery across Android emulator force-stop and relaunch;
- restart-stable Registry operation, chunk and OS-transfer landing identities;
- dependency isolation from `ku-ai`, `ku-net`, `onebrain-node`, Ollama,
  `reqwest` and Tokio;
- Android runtime execution off the UI thread and iOS simulator compilation.

MOB-03 virtual evidence additionally covers Android Keystore and iOS Keychain
adapters, no-backup install markers, fail-closed injected restore residue,
independent Node/feed/Actor signer domains, `ku-core` encrypted-vault reuse and
the chunked/versioned `OBARV001` encrypted archive foundation.

MOB-04 virtual evidence additionally covers:

- English/Vietnamese onboarding through required-data handoff and the honest
  `BootstrapOnly` Limited shell;
- a Rust/redb onboarding cursor that resumes after process death and refuses
  to regress after completion;
- bounded UTF-8 text capture into an XChaCha20-Poly1305 encrypted
  `PrivateLocal` draft store, with only an opaque receipt returned to Dart;
- shared adaptive Home/Library/Capture/Assistant/Settings navigation,
  Registry-gated disabled states, runtime/storage facts and no-LLM behavior;
- Android 16 ABI 7 integration, force-stop recovery, fail-closed missing-marker
  injection, 320 px/200% text reflow and deterministic UI goldens.
- Android `text/plain` share intents land on cold start, survive force-stop,
  expose only opaque metadata to Flutter and import idempotently into an
  encrypted raw draft.
- Android `ACTION_OPEN_DOCUMENT` selects image/video/audio/PDF sources without
  broad storage permission; native streams bounded chunks to Rust, which
  encrypts, magic-byte checks, hashes, journals and recovers interrupted work.
  The emulator picker test verifies a real PNG selection and force-stop reopen.

iOS share-extension landing, iOS system-picker media staging, full
`OwnedOriginal` pack activation/reference commit and user-selected encrypted
backup/export destinations remain explicit MOB-04 work. Android share-intent
and picker staging do not imply iOS extension or full media lifecycle readiness.

Physical Android/iOS validation is intentionally deferred by the owner while
emulator/simulator development continues. This does not close the physical
MOB-01/MOB-02/MOB-03/MOB-04 release gates.

## Generate and test

From `src/onebrain-mobile`:

```text
flutter pub get
cargo install cargo-ndk --version 4.1.2 --locked
cargo install cbindgen --version 0.29.4 --locked
python tool/generate_rust_bridge_header.py
python tool/build_rust_android.py
python tool/verify_mobile_rust_dependency_graph.py
dart run tool/generate_design_tokens.dart
dart run pigeon --input pigeons/mobile_host_api.dart
python tool/normalize_generated_sources.py
flutter gen-l10n
python tool/verify_font_assets.py
python tool/verify_bootstrap_source_isolation.py
dart format lib/app lib/design/onebrain_theme.dart
dart format lib/design/onebrain_theme_extensions.dart lib/main.dart
dart format lib/platform/mobile_host_gateway.dart lib/ui test tool pigeons
flutter analyze
flutter test --exclude-tags golden
flutter test test/golden_test.dart
flutter build apk --debug
flutter test integration_test/native_host_bridge_test.dart -d emulator-5554
# The integration target replaces app-debug.apk; rebuild the application APK
# before standalone ADB lifecycle scripts.
flutter build apk --debug
python tool/verify_android_share_intent.py \
  build/app/outputs/flutter-apk/app-debug.apk
python tool/verify_android_runtime_recovery.py \
  build/app/outputs/flutter-apk/app-debug.apk
python tool/verify_android_install_binding_fail_closed.py \
  build/app/outputs/flutter-apk/app-debug.apk
python tool/verify_android_media_picker.py \
  build/app/outputs/flutter-apk/app-debug.apk
python tool/build_rust_android.py --release
flutter build apk --release --target-platform android-arm,android-arm64,android-x64
python tool/inspect_android_permissions.py build/app/outputs/flutter-apk/app-release.apk
```

On macOS, run `bash tool/build_rust_ios.sh` before the Flutter iOS build. iOS
physical-device launch still requires device evidence before MOB-01 can exit.

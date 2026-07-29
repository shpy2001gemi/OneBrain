# onebrain-mobile

> OneBrain Mobile — autonomous iOS and Android node.
>
> Status: **MOB-01 foundation build in progress; production capability and
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
- generated English/Vietnamese localization;
- pinned offline Nunito Sans, Roboto Mono and Material Symbols Rounded assets
  with license/hash verification;
- `go_router` entry/onboarding routes;
- Riverpod-owned async presentation state;
- one Pigeon schema that generates Dart, Kotlin and Swift host APIs with an
  async capability call, bounded feasibility operation, event stream and
  cancellation.

The native snapshot now crosses the generated Pigeon API, Swift/Kotlin host and
the Rust C ABI/JNI bridge. It verifies ABI revision `1`, a deterministic typed
round trip and that no Registry request has been issued. This is bridge
evidence, not runtime or node readiness.

See [`PACKAGE_POLICY.md`](./PACKAGE_POLICY.md) for the package-first and
shared-widget contract, and [`RUST_BRIDGE.md`](./RUST_BRIDGE.md) for ABI,
thread-ownership, build and fallback details.

The current automated evidence additionally covers:

- eight Windows goldens spanning light/dark, high contrast, English/Vietnamese,
  200% text, reduced motion and compact/large/expanded layouts;
- Android 16 integration of async `started`, `cancelled` and `completed`
  events, idempotent cancellation, and the bounded `HOST_INVALID_DELAY` error;
- a release APK without Registry/model payloads and without Android network
  permission;
- static Dart/Kotlin/Swift/Rust bootstrap isolation from transport APIs.

These are MOB-01 feasibility gates. They do not authorize Registry Init,
background seeding or a product network lane.

## Generate and test

From `src/onebrain-mobile`:

```text
flutter pub get
cargo install cargo-ndk --version 4.1.2 --locked
cargo install cbindgen --version 0.29.4 --locked
python tool/generate_rust_bridge_header.py
python tool/build_rust_android.py
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
python tool/build_rust_android.py --release
flutter build apk --release --target-platform android-arm64,android-x64
python tool/inspect_android_permissions.py build/app/outputs/flutter-apk/app-release.apk
```

On macOS, run `bash tool/build_rust_ios.sh` before the Flutter iOS build. iOS
physical-device launch still requires device evidence before MOB-01 can exit.

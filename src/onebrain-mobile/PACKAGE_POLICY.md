# OneBrain Mobile package-first policy

This app composes maintained packages and platform SDKs before considering
custom infrastructure. Project code supplies OneBrain contracts, typed state,
policy boundaries and shared presentation; it does not clone package behavior.

## Foundation inventory

| Dependency | Version/source | Owned responsibility |
|---|---|---|
| Flutter / Material 3 | Flutter SDK 3.41.5 | cross-platform rendering, accessibility semantics and UI primitives |
| `go_router` | 17.3.0, `flutter.dev` | route parsing, resolver navigation and later restorable shell routing |
| `flutter_riverpod` | 3.3.2 | dependency injection and asynchronous presentation-state projection |
| `pigeon` | 27.3.0, `flutter.dev` | generated typed Dart/Kotlin/Swift host contract, events and bounded errors |
| `flutter_localizations` + `intl` | Flutter SDK / 0.20.2 | generated English/Vietnamese localization and platform formatting |
| `integration_test` | Flutter SDK 3.41.5 | official on-device Flutter test harness for the generated native bridge |
| Android SDK / AndroidX via Flutter | SDK 36 | native lifecycle and platform integration; no custom scheduler/network stack |
| Apple UIKit/Foundation via Flutter | platform SDK | native lifecycle and bounded host integration; compile requires macOS/Xcode |
| `jni` | 0.22.4 | FFI-safe Android JNI environment and native-method name generation |
| `cargo-ndk` | 4.1.2 | Android NDK discovery, target setup and standard `jniLibs` output |
| `cbindgen` | 0.29.4 | checked-in Swift-facing C header generated from Rust exports |
| `redb` | 2.6.3 | ACID bootstrap process/operation/chunk/transfer state without a C toolchain |
| `ed25519-dalek` | 2.2.0 | signed deterministic local KQL fixture verification |
| Android Keystore / Apple Keychain | platform SDK | non-exportable wrapping/protected-item custody; no custom key store |
| `ku-core::PrivateVault` + `redb` | workspace / 2.6.3 | validated XChaCha20-Poly1305 private vault over an atomic persistent backend |
| `chacha20poly1305` + `zeroize` + `getrandom` | 0.11.0 / 1.9.0 / 0.3.4 | chunked portable archive AEAD, key cleanup and OS entropy |
| existing `ku-core` + `ku-kql` crates | workspace | canonical KU types and local parser/executor reuse without `ku-ai`, Ollama or a transport stack |
| Google Fonts assets | pinned commits and SHA-256 in `assets/font_asset_manifest_v1.json` | offline Nunito Sans, Roboto Mono and Material Symbols Rounded assets under their upstream licenses |

The Riverpod version is pinned to the newest stable release compatible with
the repository's Dart 3.11.3 toolchain. Package upgrades are reviewed and
locked through `pubspec.lock`.

The native package versions, ABI/thread ownership and fallback are documented
in [`RUST_BRIDGE.md`](./RUST_BRIDGE.md).

## Shared-widget rule

Feature screens compose `lib/ui/shared/` widgets identified by catalog
contracts. These widgets wrap Material 3 controls:

- `ObmTopAppBar` — `OBM-CMP-NAV-002`;
- `ObmButton` — `OBM-CMP-ACT-001`;
- `ObmNodeFactCard` — `OBM-CMP-STA-002`;
- `ObmStatusBadge` — `OBM-CMP-STA-001`.
- `ObmIcon` — the only Flutter feature-facing gateway to the pinned rounded
  symbol family.

A wrapper exists to centralize semantic tokens, typed variants and
accessibility—not to reimplement Flutter controls. A new screen-local copy is
not accepted when a shared widget already represents the contract.

Font family, typography role and icon size are projected from the owner-approved
design token JSON. The asset verifier checks bytes, source commits, licenses and
hashes before tests or packaging; no font is downloaded at runtime.

## Custom-code threshold

Custom infrastructure requires a short ADR showing that no maintained package,
platform framework or existing OneBrain Rust crate satisfies the requirements.
Cryptography, databases, media codecs, background scheduling, file/media
pickers, routing and typed platform serialization may not be rebuilt ad hoc.

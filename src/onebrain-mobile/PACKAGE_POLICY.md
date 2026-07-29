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
| Android SDK / AndroidX via Flutter | SDK 36 | native lifecycle and platform integration; no custom scheduler/network stack |
| Apple UIKit/Foundation via Flutter | platform SDK | native lifecycle and bounded host integration; compile requires macOS/Xcode |

The Riverpod version is pinned to the newest stable release compatible with
the repository's Dart 3.11.3 toolchain. Package upgrades are reviewed and
locked through `pubspec.lock`.

## Shared-widget rule

Feature screens compose `lib/ui/shared/` widgets identified by catalog
contracts. These widgets wrap Material 3 controls:

- `ObmTopAppBar` — `OBM-CMP-NAV-002`;
- `ObmButton` — `OBM-CMP-ACT-001`;
- `ObmNodeFactCard` — `OBM-CMP-STA-002`;
- `ObmStatusBadge` — `OBM-CMP-STA-001`.

A wrapper exists to centralize semantic tokens, typed variants and
accessibility—not to reimplement Flutter controls. A new screen-local copy is
not accepted when a shared widget already represents the contract.

## Custom-code threshold

Custom infrastructure requires a short ADR showing that no maintained package,
platform framework or existing OneBrain Rust crate satisfies the requirements.
Cryptography, databases, media codecs, background scheduling, file/media
pickers, routing and typed platform serialization may not be rebuilt ad hoc.

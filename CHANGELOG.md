# 📋 Changelog

All notable changes to the OneBrain project are documented in this file.
Detailed Base v1 release records live in [`docs/operations/ONEBRAIN_BASE_V1_CHANGELOG.md`](docs/operations/ONEBRAIN_BASE_V1_CHANGELOG.md),
and live workstream/checkpoint history is tracked in [`docs/handoffs/2026-09-ku-obp-productization/PROGRESS.md`](docs/handoffs/2026-09-ku-obp-productization/PROGRESS.md).

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

---

## [Unreleased]

### Added
- **Runnable KU–OBP Concept MVP (`D-044` / `D-045` / `D-046`):** Shared `/ku` local workspace across Web, CLI (`onebrain ku`), local REST API (`/api/vnext/ku`), and Tauri Desktop (`22/24` productization tasks merged on `main`).
- **Two-node Public KU Sharing (`D-045`, `D-057`–`D-059`, `D-062`, `D-063`):** Explicit `share_prepare` → `share_preview` → `share_confirm` → durable outbound intent → receiver `share_list` / `share_inspect` over authenticated OBP-RP on Web and `onebrain ku share` CLI, display-only reverse CCID→Registry labels (`describe_ccids`), restart-durable prepared shares, seeding new private drafts from accepted Public KUs, and opt-in DEV-ONLY private-LAN binding (`--ku-network-allow-lan` / `--lan-ip`).
- **One-Command Fresh-Clone Bootstrap & Multi-OS CI (`D-056`):** `scripts/base/bootstrap_ku_dev_demo.py` (opt-in DEV-ONLY synthetic Concept Registry, secrets, sample private source, host configs) and `scripts/base/check_ku_public_share_demo.py`, verified continuously on Linux, macOS, and Windows in `ku-dev-demo-scripts.yml`.
- **AI Extraction & Bounded Semantic Repair (`KU-ENC-001/002`, `ARCH-07`, `KU-SEM-001`):** Shared extraction framework with experimental Ollama intake (`model_qualified=false`), deterministic lowering of reviewed simple claims (`review_lowering`), and 8 narrow host-bounded `SEM-F03` repair slices (`D-060`, `D-061`, `D-064`–`D-069`) in `semantic_selection_v2`.
- **Structured KU Search (`ARCH-08`):** Persistent per-principal `vnext_semantic_index` projection supporting `concept:`, `predicate:`, `unit:`, `dimension:`, and folded Registry label / literal queries.
- **Autonomous Mobile Foundation (`MOB-01..05B`, `D-053`):** Flutter + Kotlin/Swift NativeHost + `onebrain-mobile-core` (`BootstrapOnly/Limited`), encrypted private text/media capture, shared `onebrain-archive` (`OBARV002`) integration in `MOB-03`, signed Registry admission (`MOB-05A`), and Android Local Import (`MOB-05B`).

### Changed
- **Architecture Remediation (`D-051`, `ARCH-01..09`, `ARCH-11..13`, `ARCH-15`):**
  - Split `onebrain-node` into a host-agnostic shared runtime façade with CI layering enforcement (`check_node_layering.py`, `0` bypasses) and extracted leaf storage crates `onebrain-blob-store` and `onebrain-legacy-ku-store`.
  - Fenced the unauthenticated legacy TCP/JSON listener to disabled-by-default (`ARCH-02` / `D-052`), moved legacy Core DNA/OBT/DHT-SWIM behind the `legacy` feature (`ARCH-06`), froze textual KQL to bounded local read-only queries (`ARCH-04`), and gated unconsumed vNext KQL modules behind `experimental` (`ARCH-11`).
  - Added KU journal (`ARCH-03`) and Base operation store (`ARCH-15`) terminal compaction, relay outer-auth nonce/challenge lifecycle cleanup (`ARCH-01`), recovery-unit documentation (`OBS_SPEC §7.4`, `ARCH-12`), and profile-bound Rust conformance tests replacing source-snippet needle checks (`ARCH-13`).
- **CI & Dependency Hygiene (`ci`, `ci2`, `ci3`):** Patched lockfile advisories, resolved workspace `clippy -D warnings` on Rust `1.100`, and brought `vNext foundation contract`, `Base v1 three-OS`, and `KU dev demo scripts` workflows green on `main`.

---

## [base-v1.0.0-owner-waiver.1] - 2026-09-05

### Added
- Frozen vNext foundation contracts (99 tasks, 18 ADRs, canonical CBOR/BLAKE3/CID domains, validated storage, Private Vault, P0–P3 product integration, and DR-M5 hardening).
- Product-neutral Base v1 contract (`onebrain-base-contract`, Rust/TypeScript/Dart projections, stable C ABI `onebrain-base-abi`, encrypted `OBARV002` archive, three-OS candidate qualification, prebuilt Concept Registry & P5 reference roots, and 72-hour three-runner soak evidence) released under owner-approved disclosed exceptions (`BASE-GATE-V1` strict `qualified=true` not claimed).

---

## Versioning Conventions

- **MAJOR** (x.0.0): Breaking changes, major architectural shifts
- **MINOR** (0.x.0): New backward-compatible features
- **PATCH** (0.0.x): Bug fixes, minor improvements

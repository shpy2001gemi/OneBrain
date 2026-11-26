# OneBrain Agent & Claude Instructions

> **Primary workspace contract:** Always read and follow [`AGENTS.md`](AGENTS.md)
> first, then [`docs/handoffs/2026-09-ku-obp-productization/NEXT_CONVERSATION.md`](docs/handoffs/2026-09-ku-obp-productization/NEXT_CONVERSATION.md)
> and only the bounded `CURRENT_CHECKPOINT_START` / `CURRENT_CHECKPOINT_END` block
> in [`docs/handoffs/2026-09-ku-obp-productization/PROGRESS.md`](docs/handoffs/2026-09-ku-obp-productization/PROGRESS.md).

## Current Delivery Priority & Continuity

- **Active plan (`D-044` / `D-051`):** [`docs/handoffs/2026-09-ku-obp-productization/MASTER_PLAN.md`](docs/handoffs/2026-09-ku-obp-productization/MASTER_PLAN.md) owns scope, architecture remediation (`ARCH-01..15`), and the contributor backlog.
- **Single ledger:** [`docs/handoffs/2026-09-ku-obp-productization/PROGRESS.md`](docs/handoffs/2026-09-ku-obp-productization/PROGRESS.md) owns task states, checkpoint status, and retained branch/worktree/dirty-file inventory. Never delete or overwrite retained work (`KU-ENC-003` local prep, retained branches, or secondary worktree).
- **Standing Git authorization (`D-050`):** Scoped, verified work and its synchronized handoff updates may be merged and pushed to `main` without asking again, while preserving unrelated dirty/untracked/unmerged work and honoring all product, consent, trust, and mobile contracts.

## Workspace & Architecture Overview

- **Rust workspace (`src/Cargo.toml`, 21 crates):**
  - Core & Knowledge: `ku-core` (canonical vNext foundation + `legacy`-gated Core DNA/PoMV/OBT/OBKG), `ku-encoder` (shared extraction, semantic selection v1/v2, simple claim lowering), `ku-kql` (`vnext_semantic_index` KU search, local read-only text KQL, `experimental` vNext discovery), `ku-ai`, `ku-mediator`.
  - Protocol & Transport: `onebrain-protocol`, `ku-net` (feature-gated authenticated QUIC/OBP reconciliation + `legacy` DHT/SWIM), `onebrain-relay`, `onebrain-seed` (explicit legacy compat only).
  - Shared Runtime & Storage: `onebrain-node` (host-agnostic façade, 0 layering bypasses), `onebrain-base-contract`, `onebrain-base-abi`, `onebrain-archive` (`OBARV002`), `onebrain-blob-store` (`TX-BLOB-001`), `onebrain-legacy-ku-store`.
  - Surfaces: `onebrain-cli`, `onebrain-api`, `onebrain-desktop` (Tauri 2.x), `onebrain-mobile-core`, `onebrain-mobile-bridge`, `ku-demo`, plus `src/onebrain-web` (React/Vite TypeScript `/ku` workspace & dashboard) and `src/onebrain-mobile` (Flutter autonomous mobile node).
- **Contract reachability:** Consult [`docs/specs/vnext/README.md`](docs/specs/vnext/README.md) (`default-build`, `feature-gated`, `library-only`, `spec-only`) rather than historical snapshots such as `docs/PILLAR_REVIEW.md`.

## Mandatory Checks & Mobile Trigger

- **Core / vNext / Layering checks (from repo root):**
  - `python scripts/ci/validate_vnext_contracts.py`
  - `python scripts/ci/check_node_layering.py`
  - Focused `cargo test --manifest-path src/Cargo.toml -p <crate>` and `npm run test:ku --prefix src/onebrain-web` for touched areas.
- **Mobile build trigger:** Before planning or editing anything under `src/onebrain-mobile`, `src/onebrain-mobile-core`, `src/onebrain-mobile-bridge`, or mobile packaging/evidence, follow the 7-step **Mobile build trigger** in [`AGENTS.md`](AGENTS.md) and [`src/onebrain-mobile/AGENTS.md`](src/onebrain-mobile/AGENTS.md), starting and ending with `python scripts/ci/validate_mobile_build_contracts.py`.

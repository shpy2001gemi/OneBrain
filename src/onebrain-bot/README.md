# onebrain-bot

> OneBrain Bot — restricted `BotPrincipal` DMZ chat platform adapter (`P10-SURFACES-SYNC-001` §4.2).

| Field       | Value                                                          |
|-------------|----------------------------------------------------------------|
| **Tech**    | Rust (`#![forbid(unsafe_code)]`) DMZ HTTP Client + CLI         |
| **Pillar**  | `P10` (Auxiliary Surface Adapter)                              |
| **Status**  | Implemented (`BotPrincipal` PUBLIC-only + Quarantine intake)   |

## Enforced Security Invariants

1. **Strict Multi-Tenant Isolation (`NEG-P10-003`):** Authenticates exclusively via scoped `obc_surface1` (`surface_kind = "bot"`) tokens. Holds zero `PrivateVault`, `PrivateNeedVault`, `obm1.` management, or `FeedEventSigner` credentials.
2. **Public-Only Answers (`P10 §2.2` & `§4.2`):** Queries `POST /api/vnext/bot/query` (`ValidatedStore` `PUBLIC` only), fails closed if any non-public disclosure class is encountered, and renders compact provenance CIDs with canonical `[Public · Unassessed]` / `[Public · Fidelity Corroborated (k groups)]` and `[Local Store Only]` / `[Sampled Reachable Peers: n]` badges.
3. **Quarantined External Submissions (`FW-01` / `FW-02`):** Inbound messages from Discord, Telegram, Slack, or Matrix route exclusively to `POST /api/vnext/bot/submissions` with `is_executable = false` and `untrusted_external_provenance = true`, blocking step-2 `PUBLIC` confirmation and `P9` somatic access (`T-BCI-04`).

## Spec

See [P10 Multi-Surface Sync & Client Adapter Profile v1](../../docs/specs/vnext/P10_MULTI_SURFACE_SYNC_AND_CLIENT_ADAPTER_PROFILE_V1.md) (`§4.2`) and [Ten-Pillar Architecture & Interaction Matrix v1](../../docs/specs/vnext/TEN_PILLAR_ARCHITECTURE_AND_INTERACTION_MATRIX_V1.md).

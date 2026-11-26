# Pillar 10 Specification — Personal Mesh Multi-Surface Sync and Scoped Client Adapter Profile v1

> - **Profile ID:** `P10-SURFACES-SYNC-001`
> - **Pillar:** `P10` (User Interfaces, Personal Mesh & Client Adapters)
> - **Status:** Specified (additive vNext contract; runtime gate open)
> - **Reachability:** `spec-only (CI-checked)`
> - **Depends on:** [VNEXT_PRODUCT_INTEGRATION_PROFILE_V1](VNEXT_PRODUCT_INTEGRATION_PROFILE_V1.md), [VNEXT_REST_API_PROFILE_V1](VNEXT_REST_API_PROFILE_V1.md), [VNEXT_PRIVATE_WEBSOCKET_PROFILE_V1](VNEXT_PRIVATE_WEBSOCKET_PROFILE_V1.md), [VNEXT_CLI_PROFILE_V1](VNEXT_CLI_PROFILE_V1.md), [VNEXT_DESKTOP_WEB_UX_PROFILE_V1](VNEXT_DESKTOP_WEB_UX_PROFILE_V1.md), [ACTOR_ROOT_AUTHORITY_PROFILE_V1](ACTOR_ROOT_AUTHORITY_PROFILE_V1.md), [BASE_V1_RUNTIME_INTERFACE_PROFILE](BASE_V1_RUNTIME_INTERFACE_PROFILE.md), [SCOPED_RUNTIME_STATUS_PROFILE_V1](SCOPED_RUNTIME_STATUS_PROFILE_V1.md), [AI_LOCAL_OBSERVATION_INTAKE_V1](AI_LOCAL_OBSERVATION_INTAKE_V1.md), [P9_BCI_NEURAL_INTENT_AND_SAFETY_PROFILE_V1](P9_BCI_NEURAL_INTENT_AND_SAFETY_PROFILE_V1.md)
> - **Owner decisions addressed:** `D-029`, `D-045`, `D-051`, `D-062`

## 1. Purpose, Scope, and Authority Preservation

Existing P10 vNext profiles freeze the REST (`DR-P3.1`), Private WebSocket (`DR-P3.2`), CLI (`DR-P3.3`), Desktop/Web UX (`DR-P3.4`), and Base v1 C-ABI (`BASE-V1`) contracts, while the autonomous Mobile app (`onebrain-mobile`) is governed without modification by its hash-pinned contract set (`docs/design/mobile/mobile_build_contract_v1.json`). Three cross-surface specification gaps remained open in P10:

1. **Superseding Legacy `P10_TECHNICAL_GUIDE.md` Offline & Wallet Anti-Invariants:** Aligning every client surface on offline-first usability (`USABLE_OFFLINE`) and honest scope/qualification badges.
2. **Zero-Key-Reuse Multi-Device Personal Mesh (`obp/personal-mesh/1`):** Specifying how one human `ActorId` pairs Desktop, Mobile, CLI, and Glasses devices, synchronizes personal encrypted Vault items without sharing `FeedEventSigner` private keys, and rotates encryption epochs upon device loss (`ActorRevocation/1`).
3. **Least-Privilege Auxiliary Surface Adapters (`onebrain-extension`, `onebrain-bot`, `onebrain-glasses`):** Defining exact `obc_surface1` capability ceilings and hard prohibitions for the three scaffolded auxiliary surfaces.

---

## 2. Offline-First Usability & Honest Surface Law

1. **Supersession of Legacy Offline Disablement:** Legacy `P10_TECHNICAL_GUIDE.md` §15 ("Offline mode: Encode disabled, Chat disabled, OBT disabled") is **superseded**. Every primary OneBrain surface (`CLI`, `Web`, `Desktop`, `Mobile`) **MUST** remain `USABLE_OFFLINE` (`RUN-004`) with zero network peers or seed servers reachable, supporting local KU authoring/saving (`LOCAL_ONLY`), local structured KQL search, local OBKG inspection, `OBARV002` backup/restore, and local `P6` AI extraction/companion whenever a local model and Concept Registry are installed.
2. **Mandatory Epistemic & Reachability Badges:** Every surface **MUST** render canonical `RUN-004` / `P0-ADR-001` truth labels without `FULL`, `GLOBAL`, or `CLOSED` aliases:
   - KU/draft provenance: `[Private · Local Only]`, `[Personal Mesh · Encrypted]`, `[Public · Unassessed]` (`model_qualified: false`), or `[Public · Fidelity Corroborated (k groups)]`;
   - Query coverage: `[Local Store Only]` vs `[Sampled Reachable Peers: n]`;
   - Legacy wallet surfaces: `[Simulated · Non-Economic Compatibility View]` until `P5` `obt_ledger_v1` is explicitly enabled.

---

## 3. Zero-Key-Reuse Personal Mesh (`obp/personal-mesh/1`)

### 3.1 Four-Principal Device Separation (`NEG-P10-001`)

Across all devices owned by a single human user, `P10` **MUST** preserve strict separation of four cryptographic principals (`IDN-001`, `ACTOR_ROOT_AUTHORITY_PROFILE_V1.md`):

1. **`ActorId` (`32 B`):** User root authority anchor (cold recovery phrase / hardware passkey).
2. **`DeviceId` (`32 B`):** Per-installation hardware-bound device identity (OS Keychain / Secure Enclave / StrongBox).
3. **`FeedId` (`32 B`, `FeedEventSigner`):** Per-device **single-writer** append-only event log signer (`DR-P1.2`). Two devices **MUST NEVER** share the same `FeedId` private key, eliminating accidental multi-device feed equivocation under offline partition.
4. **`NodeId` (`32 B`, `SessionIdentitySigner`):** Per-endpoint `NET-001` QUIC transport identity (`NET-002`).

### 3.2 Out-of-Band Device Pairing & Delegation (`PersonalMeshPairV1`)

1. Enrolling a new device (`Device B`) into an actor's Personal Mesh **MUST** execute an out-of-band authenticated SPAKE2+/QR ceremony over a `NET-001` transcript, displaying a 6-digit / 4-word Short Authentication String verified on both screens.
2. Upon user confirmation on the authorizing device (`Device A`), `Device A` signs an `ActorRootDelegation/1` or attenuated `ActorDelegation/1` event binding `(actor_id, new_device_id, new_feed_id, new_device_hpke_pubkey, allowed_scopes,mesh_epoch)` — **zero private signing keys ever cross the wire** (`NEG-P10-001`).

### 3.3 Epoch-Encrypted Personal Vault Sync & Device Revocation (`FW-10`)

1. Items opted into Personal Mesh sync (private `KU` drafts, `LOCAL_ONLY` KUs promoted to `PersonalMeshScope`, `StandingNeed` bundles, and `PAM` Tier M1/M3 state; **never** `P9` `SomaticBiometricRaw` or local `obm1` tokens) are encrypted under `PersonalMeshEpochDek(epoch)` (`XChaCha20-Poly1305`, `disclosure_class = NEGOTIATED_ENCRYPTED` scoped strictly to active authorized `FeedId`s of the same `ActorId`).
2. `PersonalMeshEpochDek(epoch)` is wrapped individually via `X25519-HKDF-SHA256` to each currently non-revoked `new_device_hpke_pubkey` in the actor's active delegation frontier (`FEED-002`).
3. **Lost/Compromised Device Rotation (`ActorRevocation/1`):** When the user revokes `Device B` via `ActorRevocation/1`:
   - all remaining devices immediately reject `obp/personal-mesh/1` sessions from `Device B`,
   - the revoking device increments `mesh_epoch -> mesh_epoch + 1` and generates a fresh independent `PersonalMeshEpochDek(mesh_epoch + 1)` wrapped **only** to remaining active devices (guaranteeing post-revocation forward secrecy for all newly created/edited private items), and
   - historical public `FeedEvent`s validly signed by `Device B` prior to the revocation frontier remain verifiable per `FEED-002`.

---

## 4. Per-Surface Capability Ceiling (`obc_surface1`) & Auxiliary Adapters

Every out-of-process client surface authenticates to `onebrain-api` using a scoped capability token `obc_surface1` (`BLAKE3`-MACed and bound to `surface_kind`, loopback/origin/channel identity, and explicit permission bits):

| Surface | Transport to Core | `PrivateVault` Read | `obm1` Admin / Archive | Step-2 `PUBLIC` Confirm | Somatic `P9` Access |
|---|---|---|---|---|---|
| **`onebrain-desktop`** | In-process / loopback `REST+WS` | Full (owner session) | Scoped via `base_archive_file` | Allowed (typed receipt wizard) | Host foreground only |
| **`onebrain-web`** | Loopback `REST+WS` (`DR-P3.1/3.2`) | Scoped session | Disabled by default | Allowed (typed receipt wizard) | Denied |
| **`onebrain-cli`** | Loopback `REST` / Base facade (`DR-P3.3`) | Scoped CLI capability | Allowed via scoped Base facade | Allowed (interactive exact match, no `--yes`) | Denied |
| **`onebrain-mobile`** | In-process `base_v1` C-ABI/FFI | Local mobile Vault | Mobile archive contract | Allowed (mobile two-step consent) | Paired companion host |
| **`onebrain-extension`** | Native-messaging / origin-pinned loopback | Own captured drafts + scoped preview only | **Prohibited** | **Prohibited** (`NEG-P10-002`) | **Prohibited** |
| **`onebrain-bot`** | Isolated DMZ `REST` (`BotPrincipal`) | **Zero (`PUBLIC` store only)** | **Prohibited** | **Prohibited** (`NEG-P10-003`) | **Prohibited** |
| **`onebrain-glasses`** | Paired mTLS/Noise (`obp/companion-hud/1`) | Active HUD context cards only | **Prohibited** | Two-step physical + readback only | Paired `SomaticImmediate/1` |

### 4.1 `onebrain-extension` (Browser Extension Adapter)

1. **Sanitized Capture (`AI-003`):** Web captures (`SourceArtifact(WebCapture)`) **MUST** strip `Cookie`, `Authorization`, `Set-Cookie`, URL query tokens (`access_token`, `session`, `sig`, `code`), and autofill `<input type="password">` DOM nodes in the extension content script *before* transmitting bytes to `onebrain-api`.
2. **No Silent Escalation (`NEG-P10-002`):** Compromising a browser renderer or extension context **MUST NOT** grant bulk `PrivateVault` export, `obm1` archive access, or step-2 `PUBLIC` publication confirmation; publishing a captured draft requires opening the prepared receipt in the host `Desktop`/`Web` confirmation surface.

### 4.2 `onebrain-bot` (Chat Platform Adapter)

1. **Strict Multi-Tenant Isolation (`NEG-P10-003`):** `onebrain-bot` executes under a dedicated restricted `BotPrincipal` with **zero** read path to the node operator's `PrivateVault`, `PrivateNeedVault`, or `FeedEventSigner`.
2. **Public-Only Answers & Quarantined Submissions:** Channel responses query `ValidatedStore` (`PUBLIC`) only and **MUST** include compact provenance CIDs and `[Unassessed]` / `[Sampled Local Store]` badges. Inbound chat submissions from external users land exclusively in `QuarantineRecord` tagged with external platform identity and untrusted provenance.

### 4.3 `onebrain-glasses` (Smart Glasses / AR HUD Adapter)

1. **Zero At-Rest Master Secrets:** Glasses pair as an ephemeral/bounded companion (`obp/companion-hud/1`) to the user's `onebrain-mobile` or `onebrain-desktop` host. Powered-off or stolen glasses **MUST** retain zero `ActorId`, `FeedEventSigner`, or `PrivateVault` master keys.
2. **Doff-Lock & Bystander Privacy:** Physical removal of the glasses (`<= 40 ms` wear-sensor break under `SomaticImmediate/1`) **MUST** immediately blank the optical HUD, lock the companion session, and flush cached Tier M0 context cards. Outward camera/audio `AI-003` capture **MUST** assert hardware recording-indicator LED activation where supported by device firmware.
3. **Anti-Voice-Spoofing Confirmation:** Single-utterance voice commands on `onebrain-glasses` **MUST NOT** execute step-2 `PUBLIC` confirmation; confirmation requires an intentional physical temple/ring gesture paired with visual HUD / bone-conduction summary verification.

---

## 5. Cross-Pillar Boundaries & Negative Assertions

- **`P10 ↔ P1 / P3 / P4 / P6 / P8`:** All surfaces consume the shared Rust domain contracts (`onebrain-node` / `onebrain-base-abi`) rather than re-implementing canonical CBOR, CID hashing, or SQLite shortcuts in UI code.
- **`P10 ↔ P9`:** Only foreground `Desktop`/`Mobile` hosts and paired `Glasses` holding a live `SomaticImmediate/1` lease may bind `P9` somatic streams; `Extension` and `Bot` surfaces are hard-blocked (`T-BCI-04`).
- **Negative Assertions:** `NEG-P10-001` (no cross-device `FeedEventSigner` private key sharing), `NEG-P10-002` (no browser-extension `PUBLIC` confirm or `obm1` admin access), and `NEG-P10-003` (no bot access to owner `PrivateVault` or `FeedEventSigner`).

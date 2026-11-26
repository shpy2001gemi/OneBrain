# Pillar 9 Specification — BCI Neural Intent, Bounded Sensory Feedback, and Somatic Safety Profile v1

> - **Profile ID:** `P9-BCI-SAFETY-001`
> - **Pillar:** `P9` (BCI Protocol & Somatic Safety)
> - **Status:** Specified (additive vNext contract; runtime gate open)
> - **Reachability:** `spec-only (CI-checked)`
> - **Depends on:** [AI_LOCAL_OBSERVATION_INTAKE_V1](AI_LOCAL_OBSERVATION_INTAKE_V1.md), [LOCAL_RECEPTOR_ENCODER_PROFILE_V1](LOCAL_RECEPTOR_ENCODER_PROFILE_V1.md), [CAPABILITY_LAYER_PROFILE_V1](CAPABILITY_LAYER_PROFILE_V1.md), [DELEGATION_PERMIT_VALIDATION_PROFILE_V1](DELEGATION_PERMIT_VALIDATION_PROFILE_V1.md), [TYPED_COGNITIVE_EXECUTOR_PROFILE_V1](TYPED_COGNITIVE_EXECUTOR_PROFILE_V1.md), [REVOCATION_FRESHNESS_POLICY_V1](REVOCATION_FRESHNESS_POLICY_V1.md), [DISCLOSURE_POLICY_SANITIZER_V1](DISCLOSURE_POLICY_SANITIZER_V1.md), [VALIDATED_STORAGE_PROFILE_V1](VALIDATED_STORAGE_PROFILE_V1.md), [THREAT_MODEL_V1](THREAT_MODEL_V1.md)
> - **Normative baseline:** [Research Baseline v7.1](../../research/ONEBRAIN_RESEARCH_BASELINE_V7_1.md) (`§1.6`), [Foundation Threat Model v1](THREAT_MODEL_V1.md) (`§5`)

## 1. Purpose, Non-Dependency Law, and Sovereign Mediation

Pillar 9 defines the somatic and neural interface boundary connecting wearable/non-invasive neural sensors (surface EMG, EEG, EOG, fNIRS, speech-motor bands) and bounded sensory actuators to a user's OneBrain node. Previously, `P9` existed only as a high-level non-dependency thesis (`Baseline §1.6`) and a deferred higher-risk threat-model item (`THREAT_MODEL_V1` §5).

This specification establishes the complete normative v1 contract for `P9` under two foundational laws:

1. **Zero System Dependency (`Baseline §1.6`):** Pillars `P1`–`P8` and `P10` **MUST** compile, pass all conformance gates, and deliver 100% of knowledge, query, storage, AI, and economic capabilities with `bci-surface` disabled and zero BCI hardware present.
2. **Sovereign Local Mediation (`FW-08`):** No network peer (`P2`), query (`P3`), external application (`P10`), or economic actor (`P5`) **MAY** address a `P9` device directly. Every somatic signal **MUST** terminate locally inside the user's own `P8` `PrivateVault` and sovereign `P6` Personal AI Mediator (`PAM`).

---

## 2. Strict Three-Plane Somatic Capability Architecture

`P9` partitions somatic interaction into three disjoint capability effect classes enforced across CBOR schemas, `CAP-004` `PermitValidator`, `CAP-005` `TypedCognitiveExecutor`, and hardware abstraction gateware (`BciHalSafetyGate`):

| Plane | Effect Identifier | v1 Policy Status | Allowed Signal / Payload Boundary |
|---|---|---|---|
| **Plane A — Read-Intent** | `EFFECT_NEURAL_READ_INTENT` | Permitted (default-off, `SomaticImmediate/1` required) | Discrete motor/gesture/sub-vocal intent windows sealed `LOCAL_ONLY` via `AI-003` -> untrusted `ObservationEncodingProposal` or `Emergent` `ReceptorEncodingDraft` (`AI-001`). |
| **Plane B — Bounded Sensory Feedback** | `EFFECT_SENSORY_FEEDBACK` | Permitted (default-off, HAL safety envelope + `SomaticImmediate/1` required) | `SensoryFeedbackFrameV1` perceptual cues (vibrotactile, bone-conduction audio, peripheral micro-LED, or certified low-energy peripheral haptic cues) within hardware-burned safety limits. |
| **Plane C — Arbitrary Neural Write** | `EFFECT_ARBITRARY_NEURAL_WRITE` | **`PROHIBITED` (fail-closed in v1)** | Direct cortical/deep-brain semantic write, closed-loop behavioral conditioning, or subliminal/coercive stimulation **MUST** reject fail-closed (`CAPABILITY_PROHIBITED`, `NEG-BCI-001`). |

### 2.1 Plane A: Read-Intent & Anti-Impulsive Confirmation Guard

1. Raw sensor windows enter `P8` exclusively via `AI-003` (`SourceArtifact` `kind=22` + `ObservationEvent` `evt=7`, `kind=23`) encrypted in `PrivateVault` (`purpose = 4`, `LOCAL_ONLY`).
2. Neural intent decoders **MUST** execute strictly on-device inside `CAP-005` with `NetworkAccess::None`; offloading raw biosignals or neural embeddings over `RUN-003` (`P6`) is **prohibited** (`NEG-BCI-002`).
3. Decoded outputs **MUST** carry `unassessed_somatic_decode = true` and **MUST NOT** directly execute high-consequence state transitions (`PUBLIC` KU share, `PublicUseConsent` step-2 confirm, `ActorRevocation`, `P5` transfer, or vault deletion). High-consequence actions **MUST** require an explicit secondary confirmation gesture separated by a `>= 500 ms` refractory dwell window (or an orthogonal physical/visual confirmation channel on `P10`).

### 2.2 Plane B: Hardware-Bounded Sensory Feedback (`BciHalSafetyGate`)

1. Software layers communicate with Plane B exclusively via typed `SensoryFeedbackFrameV1`:
   - `cue_kind: SensoryCueKind::{HapticPulse, TactilePattern, BoneConductionTone, PeripheralVisualCue}`,
   - `intensity_permille: u16` (`<= 1000`),
   - `frequency_milli_hz: u32`,
   - `pulse_width_us: u32`,
   - `duration_ms: u16` (`<= 2_000` per frame),
   - `min_inter_frame_cooldown_ms: u16` (`>= 100`).
2. `BciHalSafetyGate` (enforced in device firmware/HAL independent of `onebrain-node` software bugs) **MUST** clamp and reject any frame violating hardware-certified peak current, charge density per phase, cumulative 60-second thermal/duty-cycle budget (`max_duty_cycle_permille <= 250`), or startle slew-rate ceilings (`HAL_SAFETY_ENVELOPE_EXCEEDED`).

---

## 3. Non-Sanitizable Biometric Taint & Crypto-Shredding (`SEC-001` / `OBS-002`)

1. Raw neural waveforms (EEG/EMG/fNIRS time series), spectral features, user-specific decoder calibration matrices (which act as permanent **biometric brainprints**), error-related potentials (`ErrP`), and inferred affective/fatigue/pain/arousal metrics **MUST** be tagged at ingestion with `TaintClass::SomaticBiometricRaw`.
2. **Non-Sanitizable Vault Lock (`FW-04`, `FW-08`):** Unlike text notes that can be user-confirmed for sharing, `TaintClass::SomaticBiometricRaw` is **permanently non-sanitizable**. `DisclosurePolicySanitizer` (`SEC-001`), `PersonalMeshSync` (`P10`), and `OBARV002` cloud-portable exports **MUST** reject any projection of `SomaticBiometricRaw` to `PUBLIC`, `ROUTE_MINIMAL`, or `NEGOTIATED_ENCRYPTED`.
3. **Epoch DEK Crypto-Shredding:** `SomaticBiometricRaw` artifacts in `PrivateVault` **MUST** be encrypted under a dedicated `SomaticVaultEpochDek` rotated at least every `24 h` (and on user demand), enabling $O(1)$ cryptographic erasure of historical raw waveforms while retaining only user-reviewed symbolic KUs derived from them.

---

## 4. `SomaticImmediate/1` Authority & Zero-Latency Revocation (`REV-001` Extension)

Standard `REV-001` tiers span `R0` (immutable read, no gate) through `R4` (`60 s` terrestrial freshness for safety-critical actions). A `60 s` revocation lag is unacceptable at the neural boundary. `P9` therefore mandates **`R4` as floor** extended by the named somatic freshness profile **`SomaticImmediate/1`**:

1. **Non-Delegable Root Permit:** Every `P9` session requires a `SignedDelegationPermit` signed directly by an active device authority of the user's `ActorId` with `onward_delegation = false`, `retention_policy = DeleteAfterTask`, `training_permission = false`, and `max_permit_ttl_ms <= 300_000` (`5 min` rolling explicit session lease).
2. **Continuous Wear / Contact Attestation (`<= 20 ms`):** The hardware adapter **MUST** verify physical wear/skin-contact/optical-proximity continuity every `<= 20 ms`. Missing `2` consecutive contact heartbeats (`<= 40 ms` physical removal/doff) **MUST** immediately revoke the active session lease.
3. **`0-ms` Synchronous Cut-Off & `ErrP` Veto:** Any of the following four triggers:
   - hardware physical kill-switch actuation,
   - contact/wear loss (`> 40 ms`),
   - explicit user somatic abort gesture (e.g., double jaw-clench / brisk blink-hold), or
   - validated error-related potential (`ErrP`) veto during Plane A/B execution
   **MUST** execute synchronously (`0 ms` software scheduling delay at the HAL gate): (a) zero Plane B actuator outputs and ground Plane A read channels, (b) trip the `CAP-005` `CancellationToken`, (c) wipe uncommitted Tier M0 decode buffers in RAM, and (d) append a durable `SomaticAbortTombstone` in `PrivateVault`.

---

## 5. BCI Higher-Risk Threat Model Controls (`THREAT_MODEL_V1` §5 Closure)

| Threat ID | Adversary Goal | Mandatory Normative Control |
|---|---|---|
| **`T-BCI-01` (Brainprint Re-ID)** | Fingerprint user identity across sessions via neural calibration weights or spectral signatures | `TaintClass::SomaticBiometricRaw` blocks all network/mesh/capsule export; only symbolic `ConceptCcid` drafts reviewed by user can ever become KUs. |
| **`T-BCI-02` (Involuntary Inner-Speech Leakage)** | Capture fleeting, unendorsed sub-vocal thoughts as durable or shared claims | Plane A requires explicit active push-to-intent / dwell window, lands strictly in `LOCAL_ONLY` draft preview, and enforces the `>= 500 ms` secondary confirmation guard. |
| **`T-BCI-03` (Sensory Conditioning / Coercive Loop)** | Use rapid Plane B feedback + Plane A readouts to steer user choices subliminally | Plane C prohibited; Plane B restricted to coarse notification schemas with `>= 100 ms` cooldown, `<= 25%` duty cycle, and zero hidden high-frequency modulation. |
| **`T-BCI-04` (Compromised UI / Extension Escalation)** | Browser extension or background app silently polls neural stream | `obc_surface1` (`P10`) forbids `EFFECT_NEURAL_READ_INTENT` for Extensions/Bots; only the foreground host session holding a live `SomaticImmediate/1` lease receives decoded intents. |
| **`T-BCI-05` (Sensor Replay Injection)** | Inject pre-recorded EMG/EEG frames over USB/BLE to forge user intent | Mutually authenticated per-frame monotonic counter + hardware nonce bound to `NET-001`/BLE-LESC pairing transcript; replayed frame counters immediately trip `SomaticImmediate/1` abort. |

---

## 6. Cross-Pillar Boundaries & Negative Assertions

- **`P9 → P6 / P8`:** Raw somatic signals never leave `PrivateVault` (`LOCAL_ONLY`); only user-reviewed symbolic drafts cross into `P1`.
- **`P9 ↛ P2 / P3 / P5`:** Zero direct wire messages exist between `P9` and `P2`, `P3`, or `P5`.
- **Negative Assertions:** `NEG-BCI-001` (`EFFECT_ARBITRARY_NEURAL_WRITE` prohibited) and `NEG-BCI-002` (raw somatic telemetry / calibration weights forbidden on wire and remote `RUN-003` capsules).

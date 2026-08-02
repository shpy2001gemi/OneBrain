# OneBrain Mobile App Feature Details V1

> Status: **Target product specification — not implementation evidence**
>
> Snapshot: **2026-08-02 (Asia/Saigon)**
>
> Feature IDs and delivery lanes:
> [`MOBILE_APP_FEATURE_TREE_V1.md`](./MOBILE_APP_FEATURE_TREE_V1.md)
>
> Screen hierarchy and routes:
> [`MOBILE_APP_SITEMAP_V1.md`](./MOBILE_APP_SITEMAP_V1.md)
>
> Visual system and component contracts:
> [`MOBILE_DESIGN_SYSTEM_V1.md`](../../design/mobile/MOBILE_DESIGN_SYSTEM_V1.md)
> and [`MOBILE_COMPONENT_CATALOG_V1.md`](../../design/mobile/MOBILE_COMPONENT_CATALOG_V1.md)

## 0. Reading this catalog

This catalog defines the user-visible and correctness contract for every
feature in the mobile tree. It does not replace protocol, architecture, privacy,
store-policy, or release gates.

For each feature:

- **contract** is the outcome the product may claim;
- **durable result** names state that must survive process death;
- **interruption rule** defines kill/offline/denied behavior;
- **acceptance** is the minimum evidence before the feature is labelled ready;
- **lane/gate** comes from the feature tree and cannot be bypassed by UI work.

An implementation issue should reference one feature ID, one or more screen
IDs, typed commands/queries, durable tables/artifacts, and test IDs.

## 1. Global feature contract

### 1.1 Common states

Every feature that performs work exposes only applicable states from this set:

```text
Unavailable(reason)
Eligible
Preparing
WaitingForUser
Running(progress_scope)
Paused(reason)
PendingExternal
Succeeded(receipt_scope)
Degraded(reason, retained_capabilities)
Failed(stable_error, retry_class)
Cancelled
```

`Succeeded` means the named finite operation succeeded. It never means
network-wide completion, truth, delivery, adoption, benefit, or custody.

### 1.2 Required alternate paths

Every feature detail must specify:

- offline behavior;
- LLM-disabled behavior;
- locked/protected-data-unavailable behavior;
- permission-denied behavior where a native capability is involved;
- app-killed and resume behavior;
- low-storage, memory, thermal, and constrained-network behavior where relevant;
- accessibility and English/Vietnamese presentation;
- stale deep-link/notification behavior.

### 1.3 Authority firewall

Presentation code may request typed commands and render typed queries. It does
not:

- open or write `redb`;
- sign arbitrary bytes;
- mutate canonical data;
- execute provider-native tools;
- convert a local save into Public UseEvidence;
- infer truth, benefit, reward, delivery, network completeness, or custody;
- treat a notification/deep link as trusted authority.

### 1.4 First-launch Init operation and readiness profile

The production executable contains code, locale/UI assets, schema support, the
immutable V1 trust profile/channel floors and bounded bootstrap metadata only.
Required large data is acquired after first launch through this durable profile:

```text
IntentRecorded
  -> ResolvingHead
  -> HeadVerified
  -> ResolvingManifest
  -> ManifestVerified
  -> AwaitingExactConfirm
  -> AdmissionPending
  -> CapacityAdmitted
  -> SchedulePrepared
  -> TransferSubmitted
  -> TransferAdopted
  -> TransferQueued
  -> Downloading
  -> BytesComplete
  -> WholeArtifactsVerified
  -> QuerySmokePassed
  -> DirectoryCommitted
  -> PointerCommitted
  -> HealthPending
  -> Completed

AwaitingExactConfirm -> DeferredByUser -> ResolvingHead
Any nonterminal work state -> Waiting(reason, resume_state) -> resume_state
Any pre-pointer state -> Failed(stable_class) or Cancelled
HealthPending -> RollbackRequired -> FailedAfterCompensation
```

This is the Registry operation machine, not process lifecycle or product
readiness. Here `healthy` means health-complete, compatible, non-revoked and
bound to valid bootstrap authority. First Init begins while derived readiness
is `BootstrapOnly` and a
nonterminal first Init, including its no-fallback `HealthPending` candidate,
projects `Provisioning(reason)`. An update begins and remains
`ReadyOffline`, including `ReadyOffline(UpdateHealthPending)`, only while an
eligible healthy, compatible, non-revoked previous release is the rollback
guarantee. `Completed` triggers an independent
readiness requery; it does not directly set readiness. Invalid/mixed authority
or a revoked current release is `RegistryDegraded`; a compensated failed first
activation returns to `BootstrapOnly` with a separate last-failure fact.

`registry.init_defer(op_id, manifest_digest)` is durable only from
`AwaitingExactConfirm`: it writes the Limited-mode receipt, schedules no large
bytes and is neither Cancel nor Pause. Returning calls
`registry.init_resume_deferred(op_id)`, re-resolves/revalidates the current
signed target and requires a new confirmation; a changed digest is never
inherited.

`WaitingNetwork`, `WaitingUnmetered`, `WaitingCharging`, `WaitingBattery`,
`WaitingThermal`, `WaitingStorage`, `PausedByUser`, `UserStoppedOSJob`,
`ResumeRequiredAfterUnobservedStop`, `PausedByOSBudget`, and
`ProtectedCallbackUnavailable` are durable pause reasons, not integrity
failures. `RetryableTransport`,
`SourceSuspect`, `IntegrityFailed`, `IncompatibleRelease`,
`InsufficientStorage`, and `ActivationFailed` are stable failure classes.
Only a signed manifest and exact artifact hashes authorize activation. A source
descriptor, peer identity, URL, mirror, ETag, store/CDN receipt, notification,
or completed byte count is delivery evidence and does not become authority.

## 2. Detailed feature catalog

### 2.0 Mobile node foundation — `MOB-FND`

| ID | Product contract and main flow | Durable result / interruption rule | Acceptance | Lane / gate |
|---|---|---|---|---|
| `MOB-FND-001` | Activate one Rust mobile core generation from foreground or bounded native background grants. The arbiter owns the grant set and one storage writer. | Process generation, active-grant set and unclean-start evidence. Last-grant expiry drains best-effort; abrupt kill requires no callback. | Flutter-absent background entry, stale callback fencing, grant replacement and repeated kill/relaunch on physical devices. | T0, `CORE` |
| `MOB-FND-002` | Verify the nonportable sealed installation epoch plus its excluded/no-backup install marker, resolve `bootstrap.redb` and `ACTIVE_DATASET`, recover operations, then open one verified dataset generation. Generic OS restore is never authority and corruption never triggers automatic reset. A clean install never reuses an orphaned platform key that survived removal of the app container. | Installation epoch/instance nonce/seal/marker, dataset switch journal, domain operation journals, idempotency receipts and safe-mode state. Marker-only `Creating` genesis may resume only with its exact valid seal; authority with a missing/mismatched marker fails closed. | Kill/ENOSPC/fsync fault at every DB/file/pointer/genesis boundary plus injected OS-restored pointer/chunk bytes and same-device iOS uninstall/reinstall with a surviving Keychain item; previous or one fully valid next state survives, a clean install rotates key/epoch, and mismatches fail closed. | T0, `CORE` |
| `MOB-FND-003` | Expose bounded typed commands, queries and sequence-numbered streams through Flutter → NativeHost → stable Rust ABI/JNI. | Command/query generation and durable receipt where applicable; streams are hints. | ABI drift, bounds, cancellation, sequence-gap refetch, Dart engine absent and no raw pointer/path/secret crossing FFI. | T0, `CORE` |
| `MOB-FND-004` | Admit each job, including first-launch Init, against foreground/deadline, storage, RAM, battery, thermal, network, roaming, user policy and platform execution facts. | Evaluated resource snapshot, decision and checkpointed durable job. | Policy/resource changes mid-batch, deadline expiry, memory warning, low disk, Data Saver/Low Power and no idle polling/keepalive. | T0, `CORE` |
| `MOB-FND-005` | Keep cloud AI, system/local model, push, network, discovery, Public UseEvidence and seeding lanes independently compiled/requested/active/default-off/kill-switchable. | Feature generation and rollback receipt; disabled lanes preserve local storage/KQL. | Stale-generation fence, immediate admission stop, in-flight bounded drain and rollback without deleting canonical/private state. | T0 |

### 2.1 Onboarding and capability qualification — `MOB-ONB`

| ID | Product contract and main flow | Durable result / interruption rule | Acceptance | Lane / gate |
|---|---|---|---|---|
| `MOB-ONB-001` | Show product boundary, choose effective UI locale, and explain that this installation is its own node. No blanket permission prompts. | Locale preference and onboarding cursor. Resume the same step after kill; changing locale never changes content/canonical bytes. | Fresh install completes in English and Vietnamese; long text, RTL-safe layout, screen reader and process-kill resume pass. | T1 |
| `MOB-ONB-002` | Inspect OS/runtime, architecture, protected-data state, available storage, estimated Init peak, and optional AI capabilities before the large signed manifest is resolved. Present supported, optional, and unavailable capabilities separately; final Init admission uses current manifest/network/storage facts. | Signed/typed capability snapshot with observation time; re-evaluate after OS/app update. No marketing capability becomes a guarantee. | Physical iOS/Android evidence; estimate-versus-final byte math; low-space and unsupported-device paths explain retained capabilities. | T1, `CORE` |
| `MOB-ONB-003` | Create a new transport NodeID and independent typed signer domains for this installation, or enter an explicit restore/import route. Never require desktop pairing. | Atomic identity/vault creation receipt and onboarding operation ID. Partial creation resumes or rolls back without a compatibility plaintext key. | Kill/fault at each key/file/DB boundary; Node/feed/Actor separation test; no key in Dart/logs. | T1, `CORE` |
| `MOB-ONB-004` | Hand first launch into the canonical Init feature, require explicit Begin Init before the small signed-manifest fetch, and disclose before Begin that accepting newer signed security metadata may advance the anti-downgrade high-water or fence an explicitly revoked local release, while it still cannot schedule large bytes. Then disclose the exact required release/bytes plus current network/energy policy and collect exact Confirm/Defer/override choices before observing `MOB-DAT-002`. The production app package and install-time asset packs contain none of the large Registry artifacts. | Onboarding cursor, selected Init policy, durable Limited-mode receipt projection, opaque `registry_operation_ref` and readiness projection only. The begin/defer/confirm state, chunk ledger, verification receipt and release pointer remain solely `MOB-DAT-002` authority; kill resumes by requerying that operation. | The current 2.056 GiB large-artifact transfer starts only after exact manifest/capacity/network consent, survives kill/offline/policy waits, and never exposes `ReadyOffline` before the operation is `Completed` and readiness independently re-derives a healthy active release. | T1, `REGISTRY` |
| `MOB-ONB-005` | Present independent readiness facts for node data, Init/registry, recovery, AI, notifications, and normal node networking. Optional AI/notification setup is skippable; before `NETWORKED-BETA`, reconciliation, publication exchange, discovery and seeding remain non-actionable disabled/unavailable facts. The narrowly scoped Registry provider lane may still obtain public signed artifacts from a direct peer/community seed, local import or optional mirror. | Onboarding completion or limited-mode receipt plus unresolved recommendation list; skipping optional features is not an error, while deferring required Init cannot create `ReadyOffline`. | A fresh install without an eligible Registry provider remains `InitWaitingForSource`; after verified activation, readiness completes in airplane mode with no model. No false “fully ready/online” badge, and Registry peer delivery never implies that normal node networking is enabled. | T1 |

### 2.2 Identity, lock, recovery, and privacy — `MOB-SEC`

| ID | Product contract and main flow | Durable result / interruption rule | Acceptance | Lane / gate |
|---|---|---|---|---|
| `MOB-SEC-001` | Lock/unlock the private node using app policy and platform credential/biometric gates. Explain `ProtectedDataUnavailable` separately from a wrong credential. | Lock policy and bounded key-session receipt; plaintext keys are never durable. Background/kill zeroizes sessions and requires fresh eligibility. Device-bound seeds, key envelopes and wrapping metadata are excluded from generic OS backup/transfer; an iOS `ThisDeviceOnly` item is still treated as potentially surviving uninstall and is never reused without its current install marker. | Wrong credential, biometric unavailable/cancelled, reboot-before-first-unlock, memory warning, process-kill, physical OS-backup/restore exclusion and uninstall/reinstall orphan-key tests. | T1 |
| `MOB-SEC-002` | Show transport NodeID, feed and Actor authority domains, public identifiers, readiness and typed failure without exposing private material. | Public identity metadata and signer health evidence only. | UI never labels Node authentication as feed/Actor authority; signer mismatch/unavailable fails closed. | T1 |
| `MOB-SEC-003` | Create an encrypted, versioned recovery package; require the user to verify that the selected recovery method can be reopened before calling it configured. | Authenticated recovery manifest and verification receipt; recovery secret is never logged, copied to notification, or included in generic OS backup. | Wrong secret, corrupt/truncated package, downgrade and no-network restore-inspection tests. | T1, `RECOVERY` |
| `MOB-SEC-004` | Offer explicit `ReplaceEmptyInstallation` or exceptional old-device retirement/key-rotation flow. Never silently clone Node/feed identity. Ordinary `ImportDataKeepCurrentIdentity` is owned by `MOB-DAT-007` and does not require this feature. | Typed identity-recovery operation, selected mode, retirement/rotation records and generation activation receipt. | Duplicate-identity simulation, non-empty replace rejection, partial restore, old-device retirement and rollback tests. | T1, `RECOVERY` |
| `MOB-SEC-005` | Let users inspect/change privacy defaults for `PrivateLocal`, `PrivateShared`, `PublicCandidate`, and `PublicAccepted` transitions. | Versioned policy and audit entry; existing canonical disclosure does not silently change. | Every outbound AI/network/media flow resolves an explicit privacy class and destination. | T1 |
| `MOB-SEC-006` | Show redacted history for unlock, recovery, signer use, Public UseEvidence, cloud disclosure, peer enrollment/revocation, and sensitive settings. | Bounded privacy-safe receipts; secrets/content/raw private IDs are excluded. | Retention limits, pagination, export review and locked-state redaction tests. | T1 |
| `MOB-SEC-007` | After release/security review, erase selected local domains or the entire installation through a typed destructive flow with exact scope, backup warning, re-authentication and final confirmation. It is never callable by an LLM/notification/deep link. | Root-journaled erase/crypto-erase receipt and tombstone/retirement work required by each identity/data domain. | Wrong scope, cancellation before commit, interruption after commit, retained OS backup, peer/identity retirement and no “global delete” wording. | T3 |

### 2.3 Home and node overview — `MOB-HOM`

| ID | Product contract and main flow | Durable result / interruption rule | Acceptance | Lane / gate |
|---|---|---|---|---|
| `MOB-HOM-001` | Show separate cards for node data, required Init/Concept Registry, runtime grant, LLM route, network presence, sync scope, seeding, and storage. A missing release links to Init without calling the node ready. | `NodeSnapshot` is queryable; UI event streams are refetch hints only. | Stale event gap refetch, BootstrapOnly/Init waiting/locked/degraded states, and no single ambiguous “Online” status. | T1 |
| `MOB-HOM-002` | Start text, clipboard, camera, picker, audio, import, search, or Assistant actions according to current capability. | Typed route intent only; no side effect before the destination validates inputs/permission. | Disabled actions show exact reason; capture works without network/LLM. | T1 |
| `MOB-HOM-003` | Continue recent private items, drafts, interrupted imports, and explicit user work. | Recency is a rebuildable private projection; authoritative draft/import remains elsewhere. | Locked privacy, empty state, bounded list and kill-resume tests. | T1 |
| `MOB-HOM-004` | Summarize durable approvals, notification intents, failed jobs, and operations needing user action. | Counts derive from durable inbox/jobs, never OS delivery state alone. | Notification denied/omitted still exposes all decisions; dedupe and stale-action tests. | T1 |
| `MOB-HOM-005` | Show actionable low-storage, registry/model update, backup and recovery notices with exact local impact. | Notice references a typed current operation/policy state. | No alarm from stale snapshot; dismissing presentation does not discard required work. | T1 |
| `MOB-HOM-006` | When Networked Beta is active, show scoped network, reconciliation, carrier and seed notices without implying global completeness. | Notice references the exact network feature, observation interval and durable work item. | Entire feature is absent before the composite gate; stale reachability and disabled optional carrier/custody lanes are stated exactly. | T2, `NETWORKED-BETA` |

### 2.4 Capture and ingestion — `MOB-CAP`

| ID | Product contract and main flow | Durable result / interruption rule | Acceptance | Lane / gate |
|---|---|---|---|---|
| `MOB-CAP-001` | Capture bounded text/clipboard input into a private draft with explicit content language. | Encrypted draft plus operation receipt; clipboard is not retained after bounded intake unless saved. | Airplane/no-LLM path, Unicode/large-input bounds, cancel and process-kill resume. | T1 |
| `MOB-CAP-002` | Receive an iOS share-extension or Android share-intent spool, show source/type/size, then import after unlock. | Encrypted landing spool and idempotent ingestion receipt. Extension never receives vault/signer keys. | Stale URI, duplicate callback, locked core, unsupported type, low disk and malicious metadata tests. | T1 |
| `MOB-CAP-003` | Select photo/document/audio/video through system pickers and stream it into bounded staging. | Picker handle is transient; resulting source uses encrypted local storage and full-length/hash evidence. | Revoked grant, mismatched MIME/extension, huge file, archive/path attack and kill tests. | T1 |
| `MOB-CAP-004` | Capture camera input and optionally run OCR after contextual permission. Preserve raw original; OCR is editable derived text with provenance. | `OwnedOriginal`, OCR candidate, model/tool/version and link in private vault. | Permission denial leaves text/picker capture; poor OCR never overwrites source; energy/thermal bounds. | T1 Optional |
| `MOB-CAP-005` | Record bounded voice/audio and optionally transcribe through a qualified local/system speech-recognition service. Remote raw-audio transcription is outside V1 and requires a separate ADR, gate and feature ID. | Owned audio source, derived transcript, speech provider/revision/language provenance. | Permission/interruption/call/route loss, no-service fallback, language/quality, privacy, energy and max-duration tests; assert no raw-audio network route. | T1 Optional, `SPEECH` |
| `MOB-CAP-006` | Treat every raw source as encrypted `PrivateLocal`; allow strip/transcode/redact only into a new representation. | Original and derived representation have distinct IDs, keys, salt and provenance. | No plaintext scan outside vault/approved spool; original cannot enter seed/share by existence alone. | T1, `MEDIA` |
| `MOB-CAP-007` | Run deterministic parsing and optional LLM candidate generation, show validation/unknowns/provenance, allow edit, then explicitly save only the source/draft or continue to KU encoding. It does not commit a KU. | Candidate remains quarantine/draft state; draft save has an idempotent durable receipt. Cancellation or model failure preserves the source/draft. | LLM disabled, malformed proposal, stale catalog, duplicate draft save, kill-after-draft-commit and no-canonical-KU tests. | T1 |

### 2.5 Self-encoding, private save, and KU publication — `MOB-ENC`

| ID | Product contract and main flow | Durable result / interruption rule | Acceptance | Lane / gate |
|---|---|---|---|---|
| `MOB-ENC-001` | Start from one exact encrypted `LOCAL_ONLY` source artifact and create a non-executable encoding draft with source commitment, selected profile and provenance. Deterministic/rule encoding is always available; a chosen LLM route is optional and separately gated. | Source artifact, operation journal, draft and exact provider/pipeline commitment survive process death. No candidate is canonical yet. | Airplane/no-LLM path, source-byte immutability, stale registry, route switch, cloud disclosure and kill-at-every-step tests. | T1, `REGISTRY/KU-ENCODE`; `AI` plus `CLOUD` only for chosen LLM route |
| `MOB-ENC-002` | Deterministically resolve CCIDs and validate the selected KU/Receptor profile: source spans, gene, roles/direction/order, instructions, values/units, negation, modality, conditions and limitations. Missing required structure returns `Incomplete`; it never fabricates canonical bytes/CID. | Versioned validation report and candidate canonical bytes only after every required profile check passes. | Unknown/ambiguous concept, reversed relation, lost condition/scope, invalid role/arity/unit, registry change and encode/decode/re-encode vectors. | T1, `KU-ENCODE` |
| `MOB-ENC-003` | Require explicit Save to place the exact validated immutable bytes in verified encrypted private storage and create source/derivation provenance. Save does not establish authorship, and a generic Feed event proves only event authorship—not authorship of referenced KU. KU author stays unresolved until a frozen `AuthorshipEvidence` predicate binds an authorized event/profile to the exact object. Save is neither publication, seeding, Public UseEvidence, Mapping materialization nor adoption. | Verified object CID/bytes, private envelope, source/derivation links and idempotent save receipt; author remains unresolved unless future qualifying `AuthorshipEvidence` exists, and retry returns the same result. | Kill before/after canonical commit, generic-payload-reference non-authorship, absent/mismatched qualifying evidence, CID collision/quarantine, duplicate operation, locked vault, low disk and no-network tests. | T1, `KU-ENCODE` |
| `MOB-ENC-004` | Create a local revision or preserve a validated alternate encoding as a new immutable object with lineage. Never overwrite the predecessor, erase disagreement, elect a global winner or infer authorship from local storage/generic Feed references. | New CID, revision/alternate relation, source binding and retained original bytes; optional author only from future qualifying `AuthorshipEvidence`. | Concurrent revision, same-source alternates, generic Feed reference, absent author evidence, hard-fidelity-mismatch preservation, rollback and arrival-order invariance. | T1, `KU-ENCODE` |
| `MOB-ENC-005` | After a generic KU-publication profile exists, derive and prepare one exact `PublicCandidate`: public KU/envelope, Feed, namespace, selector, a reviewed public source representation or explicit source-unavailable disclosure, media representations, rights/limitations and permanence. Any verifier-only raw-source access is a separate `MOB-FID-002` flow requiring `MOB-GATE-VERIFIER-EXCHANGE`. This is not the Public UseEvidence prepare contract. | Private prepared publication intent and exact canonical preview; no Feed transition, verifier grant or outbox work yet. | Private-taint/source/media leakage, verifier-grant substitution, unsupported object kind, wrong Feed/namespace, expired intent, missing rights and receipt-swap tests. | BLOCKED → T2, `NETWORKED-BETA/KU-PUBLISH` |
| `MOB-ENC-006` | Require foreground unlock, fresh re-authorization and exact confirm; sign and atomically commit the Public KU/feed transition, then enqueue bounded outbox work and show local/pending/deferred status. | Signed public object/event, consumed intent, Feed head, publication identity and idempotent outbox receipt. | Wrong signer, forged/rotated receipt, kill after commit before handoff, retry/reunion and no availability/adoption/truth claim. | BLOCKED → T2, `NETWORKED-BETA/KU-PUBLISH` |

### 2.6 Encoding-fidelity evidence and external blind verification — `MOB-FID`

| ID | Product contract and main flow | Durable result / interruption rule | Acceptance | Lane / gate |
|---|---|---|---|---|
| `MOB-FID-001` | Record the publisher encoding attempt for one exact source/encoding pair, including output, pipeline/model/tool, source-acquisition and execution commitments. It establishes provenance only. | Immutable `EncodingAttempt(Publisher)` and referenced execution record. | Source/candidate mismatch, incomplete execution, commitment mutation and explicit non-truth/non-independence assertions. | T1, `FIDELITY` |
| `MOB-FID-002` | After a remote verifier protocol exists, prepare a blind-verifier task and exact source-access grant binding source commitment, policy, challenge, purpose, permitted verifier/capability, TTL, bytes, retention and onward-delegation prohibition. Raw source never enters ordinary OBP inventory. | Private prepared task/grant plus revocable permit commitment; no remote access before acceptance. | Missing rights, oversized/unbounded source, wrong verifier/purpose, stale key state, expiry/revoke and private-taint tests. | BLOCKED → T3, `NETWORKED-BETA/VERIFIER-EXCHANGE` |
| `MOB-FID-003` | Show offered verifier work with exact source class, disclosure/license, byte/work/deadline/retention budgets and local model/resource eligibility; the user may accept or reject. Acceptance grants no publication/adoption/tool authority. | Durable offered/accepted/rejected job state and permit binding. Background hints cannot accept. | Locked/expired/replayed offer, insufficient storage/energy/model, cancellation and process-kill resume. | BLOCKED → T3, `NETWORKED-BETA/VERIFIER-EXCHANGE` |
| `MOB-FID-004` | Download only the authorized exact raw source into an isolated encrypted verifier vault, verify its source commitment before decode and retain it only under the task policy. Large input remains unavailable until the future encrypted chunk/manifest profile is frozen under `MOB-GATE-VERIFIER-EXCHANGE`. | Resumable ciphertext/source checkpoint, verified source receipt and temporary-vault hold. | Unfrozen-profile rejection, corrupt/reordered chunks, URI/manifest substitution, permit expiry/revoke, low disk, network switch and no plaintext spool. | BLOCKED → T3, `NETWORKED-BETA/VERIFIER-EXCHANGE` |
| `MOB-FID-005` | Run a bounded external-blind encode whose typed request and workflow transcript omit the target candidate. Commit exact completed output and canonical execution provenance before the workflow can reveal the target. This proves transcript order/binding, not absence of information leakage outside the workflow. A model-backed route uses only a qualified local/system/app-managed provider in the first exchange profile. | Blind output bytes/commitment, immutable external attempt and commit-before-reveal state. | Target-field/transcript absence, reveal-before-commit rejection, already-public-target limitation, partial/cancelled output, same-task mutation, OOM/thermal/deadline and kill boundary tests. | BLOCKED → T3, `NETWORKED-BETA/VERIFIER-EXCHANGE`; `AI` for model-backed route |
| `MOB-FID-006` | Reveal the target only after durable commitment, run the named source-span/gene/concept checks plus any approved extended role/instruction/value/condition checks, then review and sign an attestation as `CONSISTENT_WITH_SOURCE`, `HARD_ENCODING_MISMATCH`, `UNRESOLVED` or `NOT_APPLICABLE`. | Signed `EncodingFidelityAttestation` binds the exact policy, source, candidate, attempt/execution, correlation evidence, checks and limitations; the return receipt is pending/deferred. Assessed frontier belongs only to `MOB-FID-007`. | Hard mismatch remains non-truth/non-delete, frontier-not-in-attestation schema, unresolved completeness, signer/replay, return retry and no reward/OBT effect. | BLOCKED → T3, `NETWORKED-BETA/VERIFIER-EXCHANGE` |
| `MOB-FID-007` | Show a local assessment for one policy/source/encoding/frontier: `SELF_ATTESTED`, `PARTIALLY_CORROBORATED` or `FIDELITY_CORROBORATED_RELATIVE`, accepted evidence root, limitations and evidenced correlation groups. Never label it independent, true, false, final or globally verified. | Rebuildable deterministic `FidelityAssessment` projection over immutable evidence. | Opposite event order, 100 NodeIDs in one correlation group, frontier advance, hard mismatch count and legacy-claim non-upgrade tests. | T1, `FIDELITY` |
| `MOB-FID-008` | Preserve every validated alternate encoding by exact source/CID for history, regression and later assessment; provide no winner-selection or cleanup API. | Immutable alternate archive and source lineage; exact replay is idempotent. | Same-CID/different-bytes conflict, later revision, hard mismatch, backup/restore and no winner deletion. | T1, `FIDELITY` |
| `MOB-FID-009` | Revoke future source access, show grant expiry, stop new work and clean local verifier source bytes according to retention. Never promise deletion from a verifier that already lawfully received bytes. | Revocation/expiry record, task fence and local cleanup receipt while immutable attempts/attestations remain. | In-flight fence, offline revoke queue, expired permit, app kill, cleanup hold and honest remote-deletion wording. | BLOCKED → T3, `NETWORKED-BETA/VERIFIER-EXCHANGE` |

### 2.7 Library, search, and local KQL — `MOB-LIB`

| ID | Product contract and main flow | Durable result / interruption rule | Acceptance | Lane / gate |
|---|---|---|---|---|
| `MOB-LIB-001` | Browse local private/public items with cursor pagination and explicit scope. | Query generation and cursor; list is a projection and may be rebuilt. | Large dataset, process restart, deleted/retained branch and screen-reader list tests. | T1 |
| `MOB-LIB-002` | Search private local text, public labels, CCIDs and the active Concept Registry release. | Search profile/version and named source coverage in each result page. | Vietnamese diacritics/folding, exact CCID, locale fallback, bounded latency/RAM and stale-index rebuild. | T1 |
| `MOB-LIB-003` | Edit/run local KQL only, with syntax help, deadline/budget and typed results. Running it does not contact peers or create a StandingNeed. | Optional private query history, exact local scope/frontier and terminal reason. | Network blocked, cancellation, budget/deadline, malformed query and zero-result wording tests. | T1 |
| `MOB-LIB-004` | Filter/sort by local state, type, date, language, disclosure, tags, source and availability facts without changing canonical meaning. | Versioned query/view definition; OS locale collation is not canonical ordering. | Filter combination bounds, locale switch and unavailable index behavior. | T1 |
| `MOB-LIB-005` | Display source, validation profile, disclosure, branch, local scope, frontier and limitations next to results. | Provenance references authoritative records; no inferred truth score. | Conflict/unknown/unresolved states remain visible; no “global/no knowledge exists” wording. | T1 |
| `MOB-LIB-006` | Explore a bounded 2D neighborhood from local graph projections; selecting an edge opens its evidence/detail. | View parameters only; graph navigation never creates a relationship. | Node/edge cap, reduced motion, accessible list alternative, projection-degraded fallback. | T1 Optional |
| `MOB-LIB-007` | Optionally embed/rerank only the selected local scope with a qualified on-device model; keyword, label and local KQL remain the deterministic fallback. | Versioned local embedding projection with source frontier/model release and no canonical effect. | Model absent/update/OOM, Vietnamese/mixed-language quality, stale embedding rebuild, private-at-rest and result-equivalence fallback. | T1 Optional, `AI` |
| `MOB-LIB-008` | Show My/local-created KU from local capture or derivation as an origin shelf, not an authorship claim. Label “authored by me” only after a frozen `AuthorshipEvidence` predicate validates the required event type/profile, exact object binding and Feed/Actor authority; until then author is unresolved. Imported external/legacy origin is never coerced into local authorship. A raw draft is not a KU. | Rebuildable local-origin facet over source links and accepted objects plus optional future qualifying authorship reference; no inferred author and no mutable `owned` boolean. | Generic Feed payload reference does not author KU, absent/mismatched qualifying evidence, same CID local+remote observation, multiple local feeds, external import, legacy unknown and offline pagination. | T1 |
| `MOB-LIB-009` | Show Received KU after canonical bytes/CID/object-profile/disclosure validation. Establish an author only through the same future exact `AuthorshipEvidence` predicate; otherwise display author unresolved, separately from authenticated source peer, selector and acquisition path. Invalid/colliding bytes stay in quarantine. | Accepted canonical bytes, optional future qualifying authorship reference, append-only source observations and local Received index. | Generic signed event is insufficient, unresolved author, sender-not-author, mismatched evidence, duplicate through multiple peers, network rollback/offline browse, quarantine and no truth/trust inference. | T2, `NETWORKED-BETA` |
| `MOB-LIB-010` | Open a received KU with provenance, fidelity, media references and local actions: tag/collect, retain/unretain, annotate privately or create a new local derivative. Viewing, retaining or deriving never adopts or republishes the received object. | Local retention/organization state and, if chosen, a new derived object with lineage; received bytes remain immutable. | Access loss, source peer disappearance, local eviction/refetch, derivative authorship and no implicit UseEvidence/adoption. | T2, `NETWORKED-BETA` |

### 2.8 Knowledge detail and explicit authority transitions — `MOB-KNO`

| ID | Product contract and main flow | Durable result / interruption rule | Acceptance | Lane / gate |
|---|---|---|---|---|
| `MOB-KNO-001` | Render one item with content, language, source, provenance, disclosure, validation evidence, media and current local resolution. | Detail query includes generation; raw private content requires unlock. | Missing dependency, quarantined/corrupt object, conflict branch and offline media states. | T1 |
| `MOB-KNO-002` | Edit a local draft by creating a new revision; never rewrite immutable canonical bytes. Validate before save. | New encrypted draft/revision and parent lineage. | Concurrent edit, stale expected generation, cancel, crash and validation-error recovery. | T1 |
| `MOB-KNO-003` | Organize locally with tags/collections and create typed relationship proposals. A proposal is not an active canonical edge. | Private organization state or quarantined `BindingProposal` with provenance. | Removing local organization does not delete canonical objects; LLM proposals cannot auto-materialize. | T1 |
| `MOB-KNO-004` | Inspect Assembly, Receptor, Discover, Proposal, Mapping and Resolution as separate read-only stages with exact identity, scope and next valid action. A future stage mutation requires a new feature ID. | Stage view references exact revision, placement, policy and assessed frontier. | No materialize/adopt side effect from view; unknown/violated constraints and relative resolution wording. | T1 |
| `MOB-KNO-005` | Inspect local concurrent branches, revisions and missing dependencies without choosing a silent winner. | Branch-preserving local records; inspection never rewrites immutable revision history. | Stale branch and missing dependency shown as unknown/deferred; no network reconciliation path is reachable. | T1 |
| `MOB-KNO-006` | Prepare an exact Public `UseEvidence` intent showing canonical preview, target, recipient NodeID, selector, namespace, disclosure, idempotency identity, expiry and consequences. It cannot prepare generic KU publication. | Private prepared Public UseEvidence intent and receipt commitment; no publication yet. | Wrong target/disclosure, excessive/expired TTL, generic-KU substitution, re-prepare receipt rotation, locked/background rejection. | T2, `NETWORKED-BETA` |
| `MOB-KNO-007` | Require fresh re-authorization and exact confirmation; publish Public UseEvidence once, then show pending/deferred outbox status without claiming delivery/adoption/benefit. | Atomic UseEvidence publication/feed transition/consumed intent and idempotent retry receipt. | Forged/swapped/stale receipt, signer mismatch, generic-KU substitution, kill/reopen and duplicate-confirm tests. | T2, `NETWORKED-BETA` |
| `MOB-KNO-008` | Inspect reconciliation conflicts created by approved T2 selectors/frontiers and explicitly create a new resolution event when authorized. | Branch-preserving records and explicit resolution event, never arrival-order overwrite. | Partition/reunion vectors, disabled-gate absence, stale frontier, no silent winner and kill-safe idempotent resolution. | T2, `NETWORKED-BETA` |

### 2.9 Assistant, providers, and deterministic tools — `MOB-AI`

| ID | Product contract and main flow | Durable result / interruption rule | Acceptance | Lane / gate |
|---|---|---|---|---|
| `MOB-AI-001` | Offer deterministic search, query planning, capture assistance and status without any generative provider. | Local typed plan/result; no provider dependency. | Entire T1 journey passes with network and all models disabled. | T1 |
| `MOB-AI-002` | Provide bounded Assistant threads; user selects/removes local context and can cancel streaming. Responses are not canonical records. | Encrypted local thread, provider identity, prompt package and candidate provenance according to retention policy. | Context/token bounds, cancel, partial stream, kill and private-history lock tests. | T1 Optional |
| `MOB-AI-003` | Show selected mode (`Local only`, `Smart; ask before cloud`, explicit remote), route identity, availability reason and limitations. | Versioned preference; each turn records local release, system qualification or remote route release ID. | System/provider update quarantine, unavailable locale and no silent fallback. | T1 |
| `MOB-AI-004` | Preview the exact local context classes selected for inference and any local tool-result return to a remote provider. | Disclosure decision, destination, purpose, data classes, retention/cost policy and expiry. | Denial/minimization, conversation route switch and result-disclosure tests. | T1 |
| `MOB-AI-005` | Use Apple/Android system AI only when device, OS, task, input-language/script and output-locale qualification pass. | Observed system qualification ID and OS/provider fingerprint. | Top-foreground/platform limits, OS model change, unsupported locale and quota behavior on physical devices. | T1 Optional, `AI` |
| `MOB-AI-006` | Run a signed app-managed local model through the selected portable runtime within memory/thermal/deadline policy. | Exact model release/profile activation and per-turn audit. Model/KV state is ephemeral. | Vietnamese/mixed-language structured evaluation, OOM, unload, tamper, background and cancellation tests. | T1 Optional, `AI` |
| `MOB-AI-007` | Send minimized context to an explicitly selected gateway/custom provider only after provider-neutral task × input-language × output-locale qualification; show endpoint/region, data class, cost and retention posture. | Immutable `RemoteRouteRelease`, AI qualification ID and disclosure audit; vendor secret is not embedded in app. | Quality/structured-output threshold, network loss, alias change, revoked route, billing/error and no-training/retention capability versioning. | T1 Optional, `AI/CLOUD` |
| `MOB-AI-008` | Treat structured output/tool calls as untrusted proposals; Rust resolves schema/catalog, policy, authority, budget, consent and idempotency before deterministic execution. Show receipt and optional redacted result. | Trusted Rust proposal envelope, permit/nonce, execution journal and tool receipt. | Unknown/stale/malformed/replayed proposal, prompt injection, kill at effect boundary and provider built-in bypass tests. | T1 |

### 2.10 Media ownership, viewing, and transfer — `MOB-MED`

| ID | Product contract and main flow | Durable result / interruption rule | Acceptance | Lane / gate |
|---|---|---|---|---|
| `MOB-MED-001` | Import attachment bytes as `OwnedOriginal`, stream/hash/encrypt into packs, activate files before committing local reference. | StagedVerified → FilesActivated → ReferenceCommitted → Complete; owned hold blocks GC. | Multi-GB stream, duplicate root, ENOSPC/fsync/kill, unsupported type and unattached-original retention. | T1, `MEDIA` |
| `MOB-MED-002` | Decode/play only verified pieces; support image/document/audio/video and progressive verified ranges. | Playback position may be cosmetic; piece verification/catalog is durable. | Bad proof/index/final length, malicious media, range seek, app background and no whole-file RAM load. | T1, `MEDIA` |
| `MOB-MED-003` | Show manifest body ID, storage class, local verified bytes, pin/ownership and disclosure without exposing private inner metadata. | Query over logical catalog and physical hold ledger. | Locked redaction, catalog/pack reconcile and no availability-as-custody label. | T1, `MEDIA` |
| `MOB-MED-004` | Derive a reviewed share representation with metadata stripping/transcode/redaction, fresh key/salt and separate recipient access grants. | New encrypted representation, provenance, manifest and access/revocation records. Original is unchanged. | Nonce/key reuse, unauthorized recipient, creator-linkage policy and exact preview tests. | T2, `NETWORKED-BETA/MEDIA` |
| `MOB-MED-005` | Fetch missing pieces/ranges from bounded provider choices, verify before activation, checkpoint and resume across path/process changes. | Missing-piece bitmap, provider/session checkpoint, verified immutable packs. | Replay/duplicate/corrupt/reordered piece, network switch, carrier batch and process-kill tests. | T2, `NETWORKED-BETA/MEDIA` |
| `MOB-MED-006` | Display recent provider observations, local TTL and probe limits without presenting a sampled set as complete. Custody commitments are absent until the separate custody ADR/gate passes. | Local observation age and immutable provider/retirement records; signed custody records only on the gated custody view. | Lease replay, reboot/clock rollback expiry, probe-does-not-create-custody and complete absence of custody claims before its gate. | T2, `NETWORKED-BETA`; `CUSTODY` for custody views |
| `MOB-MED-007` | Let users pin/unpin owned media and reclaim eligible local derived/cache representations. Owned originals and active backup/rollback holds are ineligible. | Local policy/hold update and recoverable GC operation. | Concurrent reference/GC, rollback generation, backup epoch, trash recovery and exact freed-byte reporting. | T1, `MEDIA` |
| `MOB-MED-008` | Let users pin/unpin verified remote media and reclaim eligible `SeedCache`/remote cache while preserving active access/custody holds. | Network-aware policy/hold update and recoverable GC operation. | Disabled-gate absence, active transfer/custody race, remote reference, retry and exact freed-byte reporting. | T2, `NETWORKED-BETA/MEDIA` |
| `MOB-MED-009` | For media referenced by a received KU or admitted through the separately gated private share-representation/access-grant lane, show exact `MediaManifestBodyCid`, disclosure/access state, zero/partial/complete verified bytes, `ReferenceOnly` state, KU/direct-share provenance and sampled provider observations. Never label a local copy `OwnedOriginal`. | Received media reference projection plus optional KU link, validated manifest/access grant and local piece state. | Missing/revoked key, no/stale provider, reference-only offline view, direct share without KU, multiple KUs per manifest and provider-not-custody wording. | T2, `NETWORKED-BETA/MEDIA` |
| `MOB-MED-010` | On explicit user action, preflight space/access, fetch bounded manifest/pieces/ranges, verify before activation/decode, progressively stream verified ranges and optionally retain as `PinnedRemote`. | Download session, missing-piece bitmap, verified pack state, resume cursor and local retention class. | Malicious manifest/piece, partial stream, seek, network switch, process kill, access revoke and no download-as-UseEvidence/adoption/authorship. | T2, `NETWORKED-BETA/MEDIA` |

### 2.11 Network, reconciliation, availability, and seeding — `MOB-NET`

| ID | Product contract and main flow | Durable result / interruption rule | Acceptance | Lane / gate |
|---|---|---|---|---|
| `MOB-NET-001` | Show `Standalone`, `ComponentReachable`, or `PathLimited` as local derived reachability with interval, peer/carrier limitations and pending work. | `ReachabilityView` and current network policy; no canonical island identity. | Airplane/VPN/path change, stale observation and no network-wide completeness claim. | T2, `NETWORKED-BETA` |
| `MOB-NET-002` | Enroll an ordinary peer using a signed/expiring/replay-safe invitation and exact capability disclosure. Same user/LAN is not authority. | Peer identity, authorized scopes, key epoch and enrollment receipt. | Forged/expired/replayed QR, wrong NodeID, locked state and cancellation tests. | T2, `NETWORKED-BETA` |
| `MOB-NET-003` | Inspect peer scopes, recent sessions/routes and revoke future authorization. Do not present raw address as identity. | Revocation/key-state record and durable route cleanup work. | Existing session fence, offline revocation queue, stale route and re-enrollment tests. | T2, `NETWORKED-BETA` |
| `MOB-NET-004` | Run outbound-first bounded reconciliation for approved selectors/frontiers; preserve branches and resume tokens. | Durable intents/cursors/checkpoints and exact scoped terminal reason. | Duplicate/replay, partition/reunion, peer loss, process kill and no “fully synced” wording. | T2, `NETWORKED-BETA` |
| `MOB-NET-005` | Show per-selector/frontier pending, last assessed, limitations, conflicts and retry state. | Queryable sync journal; presentation events are hints. | Stale cursor, unknown peer coverage, partial/budget/deadline outcomes and branch conflicts. | T2, `NETWORKED-BETA` |
| `MOB-NET-006` | In a future M6 lane, prepare only a bounded sanitized network-fetch/discovery proposal; never send raw KQL/private NeedIR. | Local proposal/consent state; no send/materialize/adopt side effect. | Hidden/disabled before both gates; RouteMinimal privacy and zero-result scope tests after authorization. | BLOCKED, `NETWORKED-BETA/M6` |
| `MOB-NET-007` | Start a foreground authenticated direct-peer session with visible limits and stop control. | Session operation ID and checkpoints; socket is ephemeral. | No-callback kill, app-owned graceful stop, network change, deadline and Android/iOS policy matrix. | T2, `NETWORKED-BETA` |
| `MOB-NET-008` | Configure Off, Smart, Manual, and Android-only finite Aggressive seed sessions with network/charging/thermal/byte/time limits. | Policy plus per-session budget/checkpoint; no idle keepalive. | FGS quota/Task Manager Stop/UIDT pause, iOS foreground restriction, caps and restart tests. | T2, `NETWORKED-BETA/MEDIA` |
| `MOB-NET-009` | After the carrier ADR/gate passes, show carrier/SeedInbox as an optional ciphertext mailbox/transfer path, not authority or custody. Batch HTTPS containers/ranges and verify pieces locally. | Mailbox cursor, opaque hint dedupe and signed transport receipt. | Feature absent before its gate; carrier compromise, omitted push, duplicate batch, ciphertext privacy, retention and direct-path fallback. | T2 Optional, `NETWORKED-BETA/MEDIA/CARRIER` |
| `MOB-NET-010` | Discover peers on the local network only in an explicit permitted foreground window; Internet peer routes remain usable when LAN access is denied. | Ephemeral discovery observations and approved enrollment proposal only; discovery grants no trust. | iOS Bonjour/multicast declarations, Android local-network permission, denial, malicious advertisements and app-background stop. | T2 Optional, `NETWORKED-BETA` |

### 2.12 Passive OBP reunion matching — `MOB-MAT`

| ID | Product contract and main flow | Durable result / interruption rule | Acceptance | Lane / gate |
|---|---|---|---|---|
| `MOB-MAT-001` | Create, pause, resume and retire an exact private matching target from one local KU/Receptor/Assembly or an authored StandingNeed, with local selector, constraints, budget and retention. The full NeedIR, raw KQL and private object identity never leave the vault in this passive profile. | Encrypted private target, source binding, lifecycle event and dedupe identity. | KU-detail entry, restart/retire, duplicate target, private-store lock, zero-result wording and wire-capture absence tests. | T2, `NETWORKED-BETA/OBP-MATCH` |
| `MOB-MAT-002` | After authenticated OBP reconciliation admits a validated Public `KnowledgeAffordance` or permitted Public Receptor delta, run a bounded local reunion join against indexed private targets and emit only a private quarantined `BindingProposal`. OBP transports bytes; it does not perform semantic matching. | Accepted source observation, `ReunionFrontier`, match key and proposal quarantine record committed before notification. | Duplicate through multiple peers/bridges, process restart, inverse Receptor join, invalid public delta, no Need-derived outbound packet and no auto-materialize/adopt. | T2, `NETWORKED-BETA/OBP-MATCH` |
| `MOB-MAT-003` | Explain one match with author resolved only by qualifying future `AuthorshipEvidence`, otherwise unresolved, separately from authenticated responder/source-peer provenance, selector/frontiers, structural/constraint score vector, satisfied/violated/unknown checks, continuation, budget/deadline and `PARTIAL`/`PATH_LIMITED` coverage. | Rebuildable explanation tied to exact matcher/rule version, optional qualifying authorship reference and assessed frontiers. | Generic Feed reference and sender are not author evidence, absent qualifying evidence remains unresolved, unknown constraints remain unknown, hard candidate mismatch affects only this proposal, zero result is scope-relative and no global/provider-count claim. | T2, `NETWORKED-BETA/OBP-MATCH` |
| `MOB-MAT-004` | List/review quarantined proposals and allow only local retain, dismiss with reason, re-evaluate or return to target. `executable=false`; no Mapping materialization, adoption, publish or tool action is exposed under this ID. | Private proposal inbox state and idempotent local disposition receipt. | Forged/stale proposal ref, dedupe, offline browse, notification safety, kill/reopen and absence of forbidden actions. | T2, `NETWORKED-BETA/OBP-MATCH` |

### 2.13 Notifications and durable activity — `MOB-NTF`

| ID | Product contract and main flow | Durable result / interruption rule | Acceptance | Lane / gate |
|---|---|---|---|---|
| `MOB-NTF-001` | Provide the authoritative in-app inbox for approvals, jobs, reminders, security notices and failures. | Durable intent and observable-state receipt; OS notification is only presentation. | Permission denied, push omitted, dedupe, stale action, pagination and locked preview tests. | T1 |
| `MOB-NTF-002` | Schedule/submit local job, reminder, security and progress notifications using localized native-safe keys. | `scheduled`, `submitted`, `active_observed`, `interacted`, `cancelled`, `permission_denied`, `platform_error`, or `delivery_unknown`. | Never record `delivered` from API success; boot/timezone/exact-alarm and category registration tests. | T1 |
| `MOB-NTF-003` | Manage contextual permission, generic/detailed preview, categories/channels, quiet hours and digest preferences. | Rust policy plus native registration ledger; channel importance remains user-owned. | Android denial/Task Manager visibility, iOS hidden preview, locale change and channel-version migration. | T1 |
| `MOB-NTF-004` | Allow only reversible/idempotent actions such as pause/cancel queued transfer/retry. Validate current intent, nonce and expiry after unlock where needed. | Consumed action nonce and command receipt. | Forged/replayed/stale action and prohibition of publish/delete/sign/grant/restore from notification. | T1 |
| `MOB-NTF-005` | After the push-broker ADR/gate passes, optionally register an opaque APNs/FCM route for coarse mailbox-generation hints. Payload contains no product intent, content, message key or private identifier. | Revocable installation route, token TTL/last-seen and hint dedupe. | Feature absent before its gate; token rotate/uninstall/invalid response, normal-priority delay, opt-out and metadata-retention review. | T2 Optional, `NETWORKED-BETA/PUSH` |

### 2.14 Concept Registry, storage, and portability — `MOB-DAT`

| ID | Product contract and main flow | Durable result / interruption rule | Acceptance | Lane / gate |
|---|---|---|---|---|
| `MOB-DAT-001` | Show required target, staged, active and previous Concept Registry releases, with each artifact length/hash, signer, schema/runtime range and local verification/activation receipt. Show the selected `RegistrySourcePlan` separately from trust facts: direct peer, community seed, carrier peer, optional HTTPS mirror or local import is only a byte source. A clean install with no operation is normally `BootstrapOnly`; a nonterminal first Init is `Provisioning(reason)`. | Signed target manifest plus source-plan digest and device-local verification/activation receipts; provider observations, advisory timestamps, peer IDs, URLs, ETags and OS transfer receipts are not authenticity. | Absent, partial, mismatch, corruption, revocation, head/release replay, all configured mirrors unavailable and degraded private-node behavior. | T1, `REGISTRY` |
| `MOB-DAT-002` | Solely own first-launch Init and later update execution: resolve/verify one signed manifest; atomically commit its exact record, `ManifestVerified`, head/release high-water and every revocation mutation; construct and freeze a canonical multi-provider `RegistrySourcePlan`; enforce durable Begin/Defer/exact Confirm, initial/remaining capacity and network policy, pre-submit schedule/adopt barrier, source-kind/executor separation, range/chunk resume, deterministic failover, complete verification, format/mmap/query-smoke, final trust/compatibility fence, immutable activation, deterministic health and completion. Preferred network sources are direct OneBrain peers/community seeds; local import is valid; HTTPS mirror/carrier delivery is optional. Large artifacts never come from the app bundle or install-time packs. | Idempotent Init/update operation, authoritative revocation set, source-plan digest, provider attempt receipts, Limited-mode and OS-executor adoption receipts, resumable chunk ledger, immutable release directory, separate verification/activation receipts and generation-fenced `bootstrap.redb.registry_active_state`. A process/OS kill needs no callback and cannot expose half-accepted revocation state. | Kill before/after manifest acceptance and OS submit/adopt; peer disappearance, provider reorder/failover, mirror/redirect/range change, local-import removal, Defer/changed manifest, metered/roaming override scope, progressive ENOSPC, corrupt/reordered/mixed chunks, app-update/revocation fence, reboot, health compensation, clean-device provision while every OneBrain mirror is blocked, and current release physical-device provision. | T1, `REGISTRY` |
| `MOB-DAT-003` | Roll back/repair a corrupt or unhealthy registry without deleting a live mmap generation or private node data. On first Init there may be no rollback target: failed staging is quarantined/cleanable and the node remains `BootstrapOnly`. | Active/eligible previous generation and reader holds through deterministic health plus the separate post-completion rollback-retention window; revoked/failed staged state remains ineligible and separate from activation. | Old reader during swap, pointer fsync/rename failure, initial activation failure, revoked fallback rejection and corrupt active-release recovery. | T1, `REGISTRY` |
| `MOB-DAT-004` | Show protected existing bytes, signed publisher floor, initial requirement, exact credited operation-bound progress, remaining incremental requirement, authoritative maximum, active/rollback registry, models, owned/pinned/cache media, immutable staging, transfer/copy peak, verification workspace, filesystem allocation overhead, OS safety reserve and reclaimable bytes. Before `CUSTODY`, any unknown safety hold is included only in protected/non-reclaimable bytes and is not labelled as custody. | Catalog counters plus bounded physical audit receipt and exact manifest/device-derived initial/remaining capacity plan. | Filesystem drift, no partial-progress double count, total-volume reserve basis, allocation overhead, multi-GB first-Init/update preflight, opaque-hold preservation and no custody claim before its gate. | T1 |
| `MOB-DAT-005` | Offer only eligible cleanup with predicted impact and explicit confirmation for user-selected model/release removal. Partial failed/paused Init staging may be removed only after explaining that resume progress will be lost; active/required rollback generations remain protected. | Recoverable GC/delete operation and freed-byte receipt. | Owned original/custody/backup/active/rollback hold exclusion; partial-ledger invalidation and crash/trash reconciliation. | T1 |
| `MOB-DAT-006` | Create/inspect a new vault-encrypted, versioned backup at a logical multi-DB cut, including all transitive saga inputs and the exact Registry profile/head/release high-water bindings needed for safe restore. | Backup epoch, database/frontier/high-water manifest with whole tuples and profile digest/generation, chunk/root hashes and media GC holds until verified; the source installation seal/key is never portable. | Pending saga, wrong key, corrupt/truncated archive, interruption, no plaintext outside vault/envelope, no generic-OS authority backup and no legacy plaintext API. | T1, `ARCHIVE` |
| `MOB-DAT-007` | Restore/import encrypted archive data into a new local installation epoch and dataset generation. Per channel, select the complete archived or app-floor `(head_generation, head_digest)` tuple with the higher generation; separately select the complete publisher-global `(release_sequence, release_id, manifest_digest)` tuple with the higher sequence. Equal numbers require identical bindings, and an archived profile newer than the app yields `UpgradeRequiredForRegistryTrustProfile`; IDs/digests are never maximized independently. Then validate all domains/holds and atomically switch. `ImportDataKeepCurrentIdentity` is the archive path; `ReplaceEmptyInstallation` and any identity recovery are absent until `RECOVERY` also passes. | New sealed installation epoch, selected whole high-water bindings, current embedded profile binding, verified dataset, explicit mode, switch receipt and retained rollback generation. | N-1/bridge/pre-write rollback, restored-pointer/chunk rejection, equal-generation equivocation, newer-profile upgrade, high-water downgrade, post-switch mutation and media-hold tests; identity modes are inaccessible before their separate gate. | T1, `ARCHIVE`; `RECOVERY` for identity recovery |
| `MOB-DAT-008` | Export a user-reviewed scope through a versioned vault-encrypted portable package without implying desktop ownership or live replication. V1 exposes no plaintext private export. | Encrypted export manifest/provenance and operation receipt. | Scope preview, cancel/resume, wrong key, no secret/provider credential leak and no legacy/plaintext private path. | T1, `ARCHIVE` |
| `MOB-DAT-009` | In a later lane, migrate an autonomous node to another device through encrypted packages and an explicit identity retirement/rotation mode. | Migration manifest, target verification, source-retirement decision and operation receipt. | Target capacity, app-version compatibility, duplicate identity, interrupted handoff and source-retirement recovery. | T3 |

### 2.15 Model management — `MOB-MOD`

| ID | Product contract and main flow | Durable result / interruption rule | Acceptance | Lane / gate |
|---|---|---|---|---|
| `MOB-MOD-001` | Scan system-model eligibility and portable runtime/device resources without downloading a model. | Capability snapshot with OS/runtime/device class and qualification freshness. | OS update, no accelerator, low RAM/storage and provider-disabled behavior. | T1 |
| `MOB-MOD-002` | List signed app model profiles, observed system qualifications and immutable remote route releases with task/language limitations. | Catalog manifests only; untrusted community URL is never an install source. | License/region/age/policy and unsupported-locale labels. | T1 |
| `MOB-MOD-003` | Download a selected app model from an approved store pack or signed OneBrain host; verify all artifacts and smoke before activation. | Resumable download, release manifest, verification receipt and immutable generation. | Tamper, pack-path change, low disk, kill, incompatible runtime and license gate. | T1 Optional, `AI` |
| `MOB-MOD-004` | Activate a provider route only for qualified task/input-language/output-locale classes; show exact route each turn. Remote activation additionally requires the cloud disclosure/route gate. | Active profile pointer and route qualification/audit ID. | Model/system/cloud alias update, prompt incompatibility, remote route without both gates and no silent cloud route. | T1 Optional, `AI`; plus `CLOUD` for remote |
| `MOB-MOD-005` | Roll back/delete app-managed releases when not active/in use and after exact storage impact. System models are OS-owned. | Release state, reader/session hold and recoverable delete receipt. | Active inference, Play-pack rollback limitation, old reader and interrupted delete. | T1 Optional, `AI` |
| `MOB-MOD-006` | Show measured quality/resource evidence by device, task and language class: TTFT, speed, RSS, energy/thermal, structured/tool accuracy and limitations. | Signed evaluation digest and observed device result; not a truth/reward score. | Vietnamese/mixed/unknown language, system drift and remote route revision coverage. | T1 Optional, `AI` |

### 2.16 Settings, diagnostics, language, and accessibility — `MOB-SYS`

| ID | Product contract and main flow | Durable result / interruption rule | Acceptance | Lane / gate |
|---|---|---|---|---|
| `MOB-SYS-001` | Configure UI locale, content language, query fallback, Concept label locale, notification locale and requested LLM output independently. | Canonical BCP-47 preferences and versioned normalization profile for derived search only. | Runtime locale switch, Vietnamese/English completeness, mixed language and no canonical byte rewrite. | T1 |
| `MOB-SYS-002` | Support screen readers, text scaling, contrast, reduced motion, touch targets and non-visual graph/status alternatives. | Accessibility preferences where applicable; OS preference remains authoritative. | Automated semantics plus physical-device dynamic type, TalkBack and VoiceOver review. | T1 |
| `MOB-SYS-003` | Show camera, microphone, photo/document picker, notification, local-network and biometric capability/permission independently; request in context. | Native permission observation and last rationale; not a canonical grant. | Revocation while absent, denial fallback, iOS LAN declarations and Android target-SDK behavior. | T1 |
| `MOB-SYS-004` | Configure first-launch Init, local import, Registry/model update and maintenance-job constraints for Wi-Fi/metered/roaming, charging, battery, thermal, quiet hours and byte/time caps. Init's scoped Registry provider lane is available before `NETWORKED-BETA`: direct peer/community seed, carrier peer, optional HTTPS mirror and local import obey the same admission policy, while their executor is selected independently. This exception never enables reconciliation, KU/media exchange or seeding. | Versioned local-job policy plus scoped one-time overrides; each admitted job records the evaluated snapshot, source-plan digest and executor. | Policy change mid-job, metered/roaming override isolation, Low Power, provider failover, BGTask/worker expiry and cap enforcement with every normal node-network lane disabled. | T1 |
| `MOB-SYS-005` | Inspect privacy-safe health and explicitly export bounded diagnostics after reviewing included fields. | Redacted diagnostics archive with manifest and retention. | No content/prompt/tool args/key/token/private filename; locked-state and truncation tests. | T1 |
| `MOB-SYS-006` | Provide the T0 mechanism that exposes compiled/requested/active/kill-switch states and safe mode independently for operator-approved lanes. Normal users cannot bypass rollout authority. | Durable feature flag generation and rollback receipt. | Default-off future lanes, stale generation fence, rollback and no effect on local KQL/storage. | T0 |
| `MOB-SYS-007` | Show app/core/schema/registry/model/prompt/route release IDs, licenses, privacy/support links and device-local support bundle entry. | Read-only build/release metadata. | Offline availability, third-party/model license coverage and no secret identifiers. | T1 |
| `MOB-SYS-008` | Configure T2 transfer, reconciliation and seeding constraints for Wi-Fi/metered/roaming, charging, battery, thermal, quiet hours, byte/time caps and foreground-only modes. | Versioned network-job policy; each admitted job records the evaluated snapshot and governing gate generation. | Entire feature absent before Networked Beta; Data Saver/Low Power, FGS/UIDT/BGTask expiry, policy change and cap enforcement. | T2, `NETWORKED-BETA` |

## 3. Primary journey contracts

### `MOB-JRN-001` — First private/offline readiness

```mermaid
sequenceDiagram
    participant U as User
    participant UI as Mobile UI
    participant Core as Rust mobile core
    participant Host as Native host

    U->>UI: choose locale and start
    UI->>Host: inspect device/storage capabilities
    Host-->>UI: typed capability snapshot
    U->>UI: create node or explicit restore
    UI->>Core: provision identity and private stores
    Core-->>UI: durable provision receipt
    U->>UI: explicit Begin Init
    UI->>Core: registry.init_begin(channel)
    Core-->>UI: manifest, bytes, provider plan, network/energy facts
    alt Defer from the exact plan
        U->>UI: Defer
        UI->>Core: registry.init_defer(op_id, manifest_digest)
        Core-->>UI: durable Limited-mode receipt
        UI-->>U: Limited Home; return to Init later
    else Exact Confirm (start now or wait by policy)
        U->>UI: confirm manifest digest, source-plan digest, capacity and override
        UI->>Core: registry.init_confirm(...)
        Core->>Host: submit durable transfer using selected source + OS executor
        Host-->>Core: landed ranges/chunks by stable transfer and operation IDs
        Core->>Core: resume, verify all artifacts, query-smoke, atomic activate
        Core-->>UI: operation HealthPending, then Completed + receipts
        UI->>Core: independently query Registry readiness
        Core-->>UI: ReadyOffline from healthy active release
        UI-->>U: ONB-006 readiness; optional AI/network remain separate
    end
```

Exit: capture, library and local KQL work in airplane mode with all LLM and
node-network flags disabled. A clean install without artifact connectivity
remains durably `InitWaitingForSource` in the Limited shell; this is not a
failed node and is never labelled `ReadyOffline`.

### `MOB-JRN-002` — Private capture

```text
source
  -> encrypted PrivateLocal staging
  -> deterministic extraction
  -> optional qualified LLM candidate
  -> validation + editable preview
  -> explicit Save source/draft locally
     OR continue to MOB-JRN-009 Encode
  -> durable draft receipt
```

Cancel/model failure preserves or discards only according to the user's draft
choice. This journey does not commit a KU; it never publishes or seeds raw
input.

### `MOB-JRN-003` — Local recall

```text
local scope + query
  -> keyword/Concept Registry/local KQL
  -> bounded result page + coverage/frontier
  -> detail/provenance/neighborhood
```

An empty page reports the assessed local scope only.

### `MOB-JRN-004` — Assistant and deterministic tool

```text
request
  -> provider/disclosure gate
  -> inference candidate
  -> Rust proposal envelope
  -> schema/policy/authority/budget checks
  -> user approval when required
  -> deterministic handler
  -> durable tool receipt
  -> optional separately disclosed result to LLM
```

Interrupted inference can be discarded. An executed effect is reconciled by its
operation/idempotency receipt and is never blindly repeated.

### `MOB-JRN-005` — Media share and retrieval

```text
OwnedOriginal
  -> reviewed derived ShareRepresentation
  -> fresh encryption key/salt
  -> manifest body + access grants
  -> explicit network/access policy
  -> bounded verified piece transfer
```

The original remains protected. Provider observations do not become custody.
A custody-obligation branch is absent until `MOB-GATE-CUSTODY` passes; opaque
safety holds may still block GC without being exposed as a custody product
claim.

### `MOB-JRN-006` — Public UseEvidence

```text
local item
  -> exact intent preview
  -> prepare private receipt
  -> fresh re-authorization
  -> exact confirm
  -> one signed publication
  -> pending/deferred scoped outbox status
```

There is no background, notification, assistant, or deep-link shortcut around
prepare and confirm.

### `MOB-JRN-007` — Backup and restore

```text
single-writer logical cut
  -> encrypted archive + transitive inputs
  -> root verification
  -> new restore dataset generation
  -> identity-mode confirmation
  -> validation + media holds
  -> atomic ACTIVE_DATASET switch
```

Failure leaves the previous active generation and its holds intact.

### `MOB-JRN-008` — Network reconciliation and bounded seeding

```text
eligible execution grant + policy snapshot
  -> authenticated outbound session
  -> bounded selector/piece batch
  -> durable checkpoint
  -> local scoped receipt
  -> disconnect
```

Abrupt process death stops renewal; it does not erase pending durable work or
prove a peer/content is absent.

### `MOB-JRN-009` — Self-encode, save, and publish KU

```text
exact encrypted PrivateLocal source
  -> deterministic draft [optional qualified LLM proposal]
  -> resolved CCIDs + source spans + KU/Receptor profile
  -> deterministic canonical validation
  -> editable review of genes/roles/instructions/values/conditions/unknowns
  -> explicit Save private
  -> immutable private KU + durable receipt
  -> optional publisher EncodingAttempt
  -> [after KU-PUBLISH gate] exact PublicCandidate prepare
  -> fresh confirm/sign
  -> local Public publication + pending/deferred outbox
```

Save remains fully offline and never publishes. Publication cannot be
confirmed by a model, notification, verifier result or background job.
`delivered-observed` is not availability, adoption, fidelity, truth or value.
After publication, the UI may offer `MOB-JRN-010` as a separate next action
only when `MOB-GATE-VERIFIER-EXCHANGE` closes; publication itself never grants
raw-source access or dispatches verifier work.

### `MOB-JRN-010` — External blind encoding-fidelity job

```text
published/source-authorized KU
  -> exact source-access grant + FidelityPolicy
  -> authenticated verifier offer/permit
  -> user reviews and accepts bounded work
  -> encrypted exact source download + commitment verification
  -> external-blind encode with target omitted from the workflow transcript
  -> durable output commitment
  -> target reveal
  -> source-span/gene/concept checks
  -> signed attestation + pending/deferred return
  -> local frontier-relative FidelityAssessment
```

This journey is reserved until `MOB-GATE-VERIFIER-EXCHANGE` closes. Process
death before output commitment cannot reveal the target through this workflow;
death after commitment resumes at reveal/check. The transcript cannot prove
that an external environment did not learn an already published target
elsewhere. Default policy counts evidenced correlation groups, not NodeIDs. A
hard mismatch is encoding-fidelity evidence only and preserves the alternate.

### `MOB-JRN-011` — My/Received KU and received media

```text
authenticated OBP reconciliation
  -> canonical bytes/CID/object-profile/disclosure validation
  -> optional qualifying future AuthorshipEvidence resolves author;
     generic Feed references leave author unresolved
  -> invalid/collision branch to quarantine
  -> accepted record + author and source observations
  -> Received KU shelf
  -> shared immutable KU detail
  -> referenced media manifest/access state
  -> explicit View once or Keep offline
  -> bounded piece/range fetch
  -> verify before activation/decode
  -> progressive verified playback
```

My/Received, private/public, local/remote retention and semantic
adoption/materialization are separate facets. Downloading, viewing, pinning,
tagging or deriving does not change authorship, adopt the KU or create
UseEvidence. Already accepted KU/media remains browsable offline after a
network gate is disabled.

### `MOB-JRN-012` — Passive one-hop OBP reunion match

```text
private local KU/Receptor target or StandingNeed
  + validated Public Affordance/Receptor delta received through OBP-RP
  -> bounded local ReunionFrontier join
  -> ExactTypedMatcher checks
  -> private quarantined BindingProposal
  -> durable deduplicated Activity hint
  -> match list/detail
  -> retain, dismiss, re-evaluate, or return to target
```

The passive journey sends no Need-derived payload. Every match remains
`executable=false` and displays selector/frontiers, limitations,
`PARTIAL/PATH_LIMITED`, continuation and unknown constraints. Active network
fetch/discovery remains the separate M6-blocked lane.

## 4. Feature-to-data ownership

| Feature modules | Primary authoritative owner | Rebuildable/supporting state |
|---|---|---|
| `FND` | `bootstrap.redb`, active dataset domains and operation journals | process/runtime observations |
| `ONB` | onboarding cursor, selected Init policy, opaque operation reference and readiness projection | screen progress derived from the owning operation |
| `SEC` | typed signer custody and `private_vault.redb` security/recovery state | security/readiness projection |
| `HOM` | queried domain stores and durable jobs | recent/attention projections |
| `CAP` | private vault, operation journal, physical media ledger | OCR/transcript/candidate projections |
| `ENC` | exact source artifacts, verified private/canonical objects, Feed/publication journal | encoding drafts, validation reports and authored-origin projections |
| `FID` | immutable attempts/attestations, private source grants and verifier-job journal | fidelity assessments, portfolios and alternate index |
| `LIB`, `KNO` | canonical public store and private vault | public/private search and graph projections |
| `AI`, `MOD` | signed model/route releases, disclosure/tool journals | ephemeral sessions/KV, bounded thread projections |
| `MED` | private vault, media catalog, bootstrap physical holds and immutable packs | thumbnails/playback cache |
| `NET` | network work store, canonical provider/retire records | sampled reachability/provider view |
| `MAT` | private target/proposal vault plus validated public source observations | match explanation, scoped coverage and inbox projection |
| `NTF` | notification intents/receipts and durable inbox | native scheduling ledger |
| `DAT` | signed Init/update target manifests, Registry operation/chunk ledger, immutable staged/active/previous releases, verification receipts, dataset generations, backup/restore manifests and release pointers | size/audit/readiness projections |
| `SYS` | Rust policy and signed build/release metadata | native permission/resource observations |

## 5. Definition of ready

A feature may move from target specification to release-ready only when:

1. its entry gate is closed with linked evidence;
2. feature, screen, DTO, storage and tests share the same stable ID mapping;
3. main, alternate, interrupted, permission-denied, offline and degraded paths
   are implemented;
4. privacy/authority review finds no hidden disclosure or side-effect path;
5. English/Vietnamese, accessibility and native lifecycle tests pass;
6. physical-device resource evidence is within the approved budget;
7. default-off/kill-switch/rollback behavior is proven where applicable;
8. product wording states exact scope/frontier/limitation and avoids global
   truth, completion, delivery, availability, benefit or custody claims.

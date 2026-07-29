# OneBrain Mobile implementation instructions

This subtree implements an autonomous iOS/Android node. It is not a desktop
replica, desktop extension, companion client, or UI over a permanently running
remote node.

## Mandatory preflight

From the repository root:

```text
python scripts/ci/validate_mobile_build_contracts.py
```

Then read every document in the manifest `required_read_set` completely before
changing code. Record the current authority-set digest and work package in
`compliance/mobile_build_evidence_v1.json`.

## Non-negotiable boundaries

- Flutter owns presentation and typed intent only.
- Swift/Kotlin owns bounded platform integration and OS callbacks.
- Rust owns canonical validation, one storage writer, keys/signing, database
  transactions, lifecycle state machines, network policy, and deterministic
  tool execution.
- No database handle, filesystem path, signing primitive, provider-native tool,
  arbitrary network transport, or canonical mutation crosses into Dart.
- LLM output is candidate data. Local/device/remote providers never become tool
  executors.
- Save Private KU is not Publish. Publication and Public UseEvidence remain
  separate gated authority transitions.
- The app package contains no Concept Registry release, `.obr`, indexes,
  compressed equivalents, model bundle, or seed cache. Required large data is
  obtained only through explicit resumable post-launch Init.
- Durable work assumes Flutter, the process, network, or execution grant may
  disappear without a callback. A foreground service is a finite grant, not an
  always-live promise.
- Node data, Registry, runtime grant, AI route, network presence, sync, seeding,
  and storage are independent facts; never collapse them into “Online.”
- UI uses generated semantic design tokens and catalog components. Raw
  per-screen colors, spacing, radius, motion, or one-off screen architecture are
  prohibited.
- English and Vietnamese, accessibility, 200% text, reduced motion, compact
  phone, and adaptive tablet behavior are part of the feature—not polish added
  later.

## Change protocol

1. Identify the implementation-plan work package and affected stable IDs.
2. Add or update tests/evidence in the same change as implementation.
3. Keep `deviations` empty. A deviation requires an owner-approved ADR, an
   explicit manifest update, and new authority/evidence digests.
4. Run:

```text
python scripts/ci/validate_mobile_build_contracts.py
python -m unittest scripts.ci.test_validate_mobile_build_contracts
```

5. Also run the work-package-specific Flutter, native, Rust, lifecycle,
   packaging, accessibility, golden, and physical-device checks required by the
   implementation plan. Passing this structural harness alone is never release
   evidence.

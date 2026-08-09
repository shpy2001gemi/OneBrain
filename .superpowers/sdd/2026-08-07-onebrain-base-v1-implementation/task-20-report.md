# Task 20 Report — Registry immutable full-size qualification harnesses

## Status

DONE. The implementation and focused verification are complete on
`codex/base-v1-registry` from base `99be17eaf6daf49e9f6cea57aa2d86d755c9ae33`.
This task creates qualification machinery only; it does not claim that the
full-size measurements exist or that `BASE-GATE-V1` passes.

## Implementation summary

- Extended resource qualification with the frozen `cold-cache`, `low-ram`,
  `ssd`, and `hdd` budgets, inclusive 2,200,000,000–2,500,000,000-byte OBR
  bounds, Linux sysfs rotational evidence, Windows physical-disk media type,
  macOS storage protocol/solid-state evidence, host/filesystem/candidate
  evidence, and Linux-only production-reference enforcement.
- Added closed `QualificationRunContextV1` handling. Prequalification receipts
  carry `closure_digest`, set `base_candidate_bound=false`, omit all four
  Release-only fields, and cannot derive `registry_production_qualified`.
  Release receipts carry the exact request/session/commit/tree plus verified
  candidate semantic/artifact bindings.
- Added canonical sorted-JSON Ed25519 Registry receipt envelopes and a pure
  production aggregator. It verifies the frozen Task 19 profile/policy,
  allowlisted signer, signatures, true results/oracles, exact component set,
  no duplicates/carry-forward, and equality of every exact-candidate binding.
  Its output sets only `registry_production_qualified=true` and explicitly
  keeps `base_gate_v1=false`.
- Added a real nine-step signed release-cycle runner. It executes package,
  verify, activate, query, build-new-signed-generation, CCID diff, activate-new,
  rollback, and reactivate-new; it rejects `quarterly_update.py`.
- Made release publication and activation-state append interruption-safe at
  before/during/after failpoints. The signed failure harness now performs all
  six real child-process kill drills and records old-or-new-complete receipts.
- Added `ConceptRegistryGenerationManager` and Arc-backed reader leases. A
  refresh fully verifies/opens the next immutable generation before swapping;
  existing readers stay pinned through activation and rollback. Runtime status
  exposes the exact signed release aggregate root.
- Extended the Rust probe with exact artifact sizes/hashes and executable/probe
  hashes, and added a signed generation-swap/rollback qualification example.

## RED evidence

### Python resource and aggregate contract

Command:

```powershell
python -m unittest scripts.concept_registry.test_resource_qualification scripts.concept_registry.test_production_qualification -v
```

Relevant expected failure:

```text
ImportError: cannot import name 'MAX_PRODUCTION_OBR_BYTES' from 'resource_qualification'
ModuleNotFoundError: No module named 'production_qualification'
```

Reason: production size/storage fields and the pure signed aggregator did not
exist.

### Signed release-cycle runner

Command:

```powershell
python -m unittest scripts.concept_registry.test_release_cycle_qualification -v
```

Relevant expected failure:

```text
ModuleNotFoundError: No module named 'release_cycle_qualification'
```

Reason: the required independent nine-step signed cycle harness did not exist.

### Release publication and activation process kills

Command:

```powershell
cargo test --locked --manifest-path src/Cargo.toml -p ku-core concept_registry_release -- --test-threads=1
```

Relevant expected failure:

```text
process_kills_around_activation_append_reopen_old_or_new_exact_state ... FAILED
process_kills_around_release_publication_leave_only_complete_releases ... FAILED
kill worker exited before state-append-before marker
kill worker exited before release-publication-before marker
test result: FAILED. 8 passed; 2 failed
```

Reason: the durable code paths did not yet expose the six kill synchronization
points.

### Immutable reader generation manager

Command:

```powershell
cargo test --locked --manifest-path src/Cargo.toml -p onebrain-node --lib concept_registry_runtime -- --test-threads=1
```

Relevant expected failure:

```text
error[E0433]: cannot find type `ConceptRegistryGenerationManager` in this scope
error: could not compile `onebrain-node` (lib test) due to 3 previous errors
```

Reason: no Arc-backed reader-lease generation swap API existed.

### Signed failure receipt context

Command:

```powershell
cargo build --locked --manifest-path src/Cargo.toml -p ku-core --example concept_registry_failure_qualification --features concept-registry-failure-harness
$env:ONEBRAIN_REGISTRY_FAILURE_QUALIFICATION=(Resolve-Path 'src\target\debug\examples\concept_registry_failure_qualification.exe').Path
python -m unittest scripts.concept_registry.test_failure_qualification -v
```

Relevant expected failure:

```text
unexpected argument: ...\trust-policy.json
usage: concept_registry_failure_qualification ... PRIVATE_KEY_FILE OUTPUT_JSON
```

Reason: the producer did not yet accept a closed run context, verified binding,
or receipt trust policy.

### Self-review regressions caught by new behavior tests

Commands:

```powershell
python -m unittest scripts.concept_registry.test_resource_qualification.ResourceQualificationTests.test_portability_storage_collector_cannot_claim_production_reference scripts.concept_registry.test_failure_qualification -v
python -m unittest scripts.concept_registry.test_resource_qualification.ResourceQualificationTests.test_unknown_or_missing_production_volume_evidence_fails_closed -v
```

Relevant expected failures:

```text
KeyError: 'production_reference_host_is_linux'
KeyError: 'process_kills'
AssertionError: True is not false
```

Reasons: portability evidence was not yet prevented from claiming a production
reference; signed failure evidence did not yet contain process-kill receipts;
and contradictory Linux `rotational=1`/`storage_class=ssd` evidence was not yet
rejected.

## GREEN verification

### Exact Rust commands required by the brief

```powershell
cargo test --locked --manifest-path src/Cargo.toml -p ku-core concept_registry_release -- --test-threads=1
```

Result: PASS — 10 passed, 0 failed (including six child-process kill phases).

```powershell
cargo test --locked --manifest-path src/Cargo.toml -p onebrain-node --lib concept_registry_runtime -- --test-threads=1
```

Result: PASS — 12 passed, 0 failed (including pinned old/new readers, rollback
with an active reader, and exact aggregate root after reopen).

### Focused Python suites with real compiled executables

```powershell
cargo build --locked --manifest-path src/Cargo.toml -p ku-core --example registry_probe --example concept_registry_failure_qualification --features concept-registry-failure-harness
$env:ONEBRAIN_REGISTRY_PROBE=(Resolve-Path 'src\target\debug\examples\registry_probe.exe').Path
$env:ONEBRAIN_REGISTRY_FAILURE_QUALIFICATION=(Resolve-Path 'src\target\debug\examples\concept_registry_failure_qualification.exe').Path
python -m unittest scripts.concept_registry.test_resource_qualification scripts.concept_registry.test_failure_qualification scripts.concept_registry.test_release_cycle_qualification scripts.concept_registry.test_production_qualification -v
```

Result: PASS — 20 passed, 0 failed, 1 skipped. The skip is the documented
Linux-only RLIMIT/cache integration on this Windows host; the compiled probe
and compiled failure harness integrations both ran and passed.

### Example/build and syntax checks

```powershell
python -m py_compile scripts/concept_registry/resource_qualification.py scripts/concept_registry/release_cycle_qualification.py scripts/concept_registry/production_qualification.py
cargo check --locked --manifest-path src/Cargo.toml -p onebrain-node --example concept_registry_production_qualification
cargo check --locked --manifest-path src/Cargo.toml -p ku-core --example registry_probe
```

Result: PASS.

### Contract validation

```powershell
python scripts/ci/validate_vnext_contracts.py
```

Result: PASS — `vNext contracts OK ... 444 local links`.

## Files changed

- Created `src/onebrain-node/examples/concept_registry_production_qualification.rs`
- Created `scripts/concept_registry/release_cycle_qualification.py`
- Created `scripts/concept_registry/test_release_cycle_qualification.py`
- Created `scripts/concept_registry/production_qualification.py`
- Created `scripts/concept_registry/test_production_qualification.py`
- Modified `scripts/concept_registry/resource_qualification.py`
- Modified `scripts/concept_registry/test_resource_qualification.py`
- Modified `scripts/concept_registry/test_failure_qualification.py`
- Modified `src/ku-core/examples/registry_probe.rs`
- Modified `src/ku-core/examples/concept_registry_failure_qualification.rs`
- Modified `src/ku-core/src/concept_registry_release.rs`
- Modified `src/onebrain-node/src/concept_registry_runtime.rs`
- Modified `src/onebrain-node/Cargo.toml`

`src/Cargo.lock` did not require a delta because Task 20 introduced no Rust
dependency; all used crates were already locked. No file outside the approved
implementation list was changed. A transient line-ending-only `lib.rs` worktree
change from formatting was explicitly restored and is absent from the diff.

## Self-review

- Re-read the Task 19 profile and machine contract after implementation.
- Confirmed envelope fields are closed and unsigned signing transforms retain
  `signature=""` under canonical sorted-key JSON.
- Confirmed the production CLI pins the exact Task 19 profile digest, trust
  policy digest, and approved signer public key; test-only happy paths use
  ephemeral keys/policies and never access production key material.
- Confirmed key material is never emitted and key-read failures are sanitized
  so the production private-key path is not printed.
- Confirmed every component must be directly signed, true, present once, and
  equal across request/session/commit/tree, semantic/artifact tuple, release
  root/generation, profile/policy/signer, probe/executable, five payload hashes,
  and stamp hash.
- Confirmed Prequalification omits Release-only fields and cannot enter the
  aggregate; fixture/unknown context and carry-forward wrappers fail closed.
- Confirmed Windows/macOS collectors remain portability evidence and cannot
  satisfy the Linux production-reference oracle.
- Confirmed immutable generations are never overwritten or deleted while
  leased; refresh swaps only after complete verification/open.
- Confirmed `git diff --check` passes and `src/onebrain-node/src/lib.rs` is clean.

## Concerns

- No full-size production artifacts or measurements are included; that is
  intentionally deferred to Tasks 21 and 28. The authoritative state remains
  `production_qualified=false` until fresh exact-candidate receipts exist.
- The Windows host cannot execute the Linux-only RLIMIT/cache integration.
- Focused Rust commands report pre-existing dead-code warnings in unrelated
  `ku-core`/`ku-kql` code; Task 20 introduced no new warning in its examples.

---

# Fix loop round 1/5 — signed request and immutable-generation hardening

## Implementation summary

- Added the owner-approved OpenPGP Ed25519 qualification-approver policy,
  canonical vector, and reusable Base release-request verifier. It requires
  canonical request/policy bytes, a closed request schema, exact candidate Git
  object format and target/tooling maps, request/policy validity, GPG
  `--status-fd` `VALIDSIG`, algorithm 22, signature creation inside request
  validity, the full primary fingerprint, explicit allowlisting, and the exact
  exported public-key packet BLAKE3.
- The verifier returns frozen `VerifiedQualificationContextV1` context and
  Registry bindings derived from signed bytes. It does not accept context or
  binding overrides. Tests use an isolated temporary GPG home and ephemeral
  Ed25519 keys; no production private key or path is read.
- Added exact measurement checks for the five Registry payloads, release stamp,
  executable/probe, detached probe signature and signer identity, toolchain
  evidence, runner-image evidence, and target triple. Resource Release mode now
  invokes the verifier and rejects legacy raw Release context/binding JSON;
  Prequalification retains its closed non-claiming form without Release-only
  fields.
- Changed the Rust generation-swap producer to invoke the production request
  verifier, derive its context/bindings, independently compare the signed
  release stamp/five artifact hashes and reference-environment measurements,
  and verify the detached probe signature. Reactivation now drops the manager
  and opens a fresh manager before checking exact root/generation.
- Added the real request-bound signed CCID producer. It invokes the existing
  SQLite-backed `generate_report` implementation on exact old/candidate
  input/OBR/manifest bytes and rejects a one-byte input mutation before signing.
- Fixed the signed resource CLI `KeyError` by preserving the underlying gate
  result before replacing the report with a receipt envelope.
- Prevented stale concurrent refresh installation by comparing generation under
  the write lock. A real two-generation overlapping installer test proves a
  stale load cannot replace generation N+1.
- Split aggregation into a frozen production API and an explicitly
  non-production test helper. The public API requires the exact Task 19 profile,
  trust-policy digest, allowlisted public key, and corresponding signing key;
  the ephemeral helper always emits
  `registry_production_qualified=false`.

## RED evidence

### Missing signed-request verifier

```powershell
python -m unittest scripts.release.test_verify_base_release_request -v
```

Expected RED:

```text
ModuleNotFoundError: No module named 'verify_base_release_request'
FAILED (errors=1)
```

Reason: no production verifier existed to convert signed external request bytes
into closed context/bindings.

The first ephemeral GPG harness attempt then exposed a Windows test-environment
failure, not a product failure:

```text
gpg: error running '/usr/bin/gpg-agent': exit status 2
gpg: failed to start gpg-agent '/usr/bin/gpg-agent': General error
gpg: agent_genkey failed: No agent running
```

Git for Windows' `gpg.exe` was being launched outside its MSYS `/usr` mount.
Key generation/signing was moved under Git Bash while verification remains the
real direct `gpg.exe --status-fd 1 --verify` call.

### Signed resource CLI envelope regression

```powershell
python -m unittest scripts.concept_registry.test_resource_qualification.ResourceQualificationTests.test_signed_cli_returns_payload_gate_status_without_key_error -v
```

Expected RED:

```text
KeyError: 'qualified'
FAILED (errors=1)
```

Reason: `main()` replaced the raw report with a receipt envelope and then read
the removed top-level `qualified` field.

### Raw Release override rejection

```powershell
python -m unittest scripts.concept_registry.test_resource_qualification.ResourceQualificationTests.test_raw_release_context_and_binding_cannot_create_a_receipt -v
```

Expected RED:

```text
AssertionError: QualificationError not raised
FAILED (failures=1)
```

Reason: `create_resource_receipt` still accepted caller-created Release context
and binding objects.

### Frozen production aggregate API

```powershell
python -m unittest scripts.concept_registry.test_production_qualification.ProductionQualificationTests.test_public_production_aggregator_rejects_ephemeral_profile_and_signer -v
```

Expected RED:

```text
AssertionError: AggregationError not raised
FAILED (failures=1)
```

Reason: the public aggregation function accepted an arbitrary ephemeral
profile/signer and derived a production claim.

### Stale overlapping refresh

```powershell
cargo test --manifest-path src/Cargo.toml -p onebrain-node concept_registry_runtime::tests::overlapping_refresh_install_cannot_replace_newer_generation_with_stale_load -- --exact --nocapture
```

Expected RED:

```text
error[E0599]: no method named `install_loaded_generation` found
error: could not compile `onebrain-node` (lib test) due to 2 previous errors
```

Reason: refresh loaded outside the lock and had no generation-checked install
operation, allowing an older completed load to overwrite a newer generation.

### CCID producer process note

The real signed CCID integration test was added in the same edit sequence as
its new producer and its first recorded execution was GREEN. This does not meet
the brief's requested separately captured RED for that new producer. The test
does nevertheless execute the real diff and proves a candidate-input byte
mutation is rejected. This process deviation is carried as a concern rather
than misrepresented as RED evidence.

## GREEN verification

### Signed request, resource, CCID, aggregate, release-cycle, and failure suites

```powershell
python -m unittest scripts.release.test_verify_base_release_request scripts.concept_registry.test_resource_qualification scripts.concept_registry.test_ccid_stability_diff scripts.concept_registry.test_production_qualification scripts.concept_registry.test_release_cycle_qualification scripts.concept_registry.test_failure_qualification -v
```

Result: PASS — 35 tests, 0 failures/errors, 3 environment skips. The skips are
the compiled resource probe, Linux-only resource-limit integration, and
compiled failure drill because their environment variables were not set in
this final combined invocation. The unchanged real release-cycle happy test is
not accepted as closure of the open forgeability finding below.

Focused GREEN results also recorded:

```text
SignedReleaseRequestTests: 3/3 OK
ProductionQualificationTests: 5/5 OK
test_real_ccid_diff_signs_only_exact_request_bound_six_file_tuple: OK
test_signed_cli_returns_payload_gate_status_without_key_error: OK
test_raw_release_context_and_binding_cannot_create_a_receipt: OK
```

### Rust commands required by the Task 20 brief

```powershell
cargo test --locked --manifest-path src/Cargo.toml -p ku-core concept_registry_release -- --test-threads=1
```

Result: PASS — 10 passed, 0 failed.

```powershell
cargo test --locked --manifest-path src/Cargo.toml -p onebrain-node --lib concept_registry_runtime -- --test-threads=1
```

Result: PASS — 13 passed, 0 failed, including the new overlapping stale-load
regression.

### Producer/example compilation

```powershell
cargo check --locked --manifest-path src/Cargo.toml -p onebrain-node --example concept_registry_production_qualification
cargo check --locked --manifest-path src/Cargo.toml -p ku-core --features concept-registry-failure-harness --example concept_registry_failure_qualification
cargo check --locked --manifest-path src/Cargo.toml -p ku-core --example registry_probe
```

Result: PASS for all three examples. Only pre-existing unrelated dead-code
warnings were emitted.

### Contract, formatting, and diff verification

```powershell
python scripts/ci/validate_vnext_contracts.py
git diff --check
cargo fmt --all --manifest-path src/Cargo.toml -- --check
```

Result: PASS. Validator ended with `vNext contracts OK ... 444 local links`.

## Files changed in fix round 1

- Created `docs/security/BASE_V1_QUALIFICATION_APPROVER_POLICY.md` — minimal
  owner-approved signer/verification policy documentation pulled forward from
  Task 27.
- Created
  `src/test-vectors/vnext/base-v1-qualification-approver-policy-v1.json` — exact
  canonical policy preimage and derive-key digest vector.
- Created `scripts/release/verify_base_release_request.py` — shared closed
  request/policy/GPG verifier and candidate measurement helper.
- Created `scripts/release/test_verify_base_release_request.py` — isolated
  ephemeral-GPG positive/negative and literal digest recomputation tests.
- Created `scripts/concept_registry/ccid_stability_qualification.py` — real
  request-bound signed CCID receipt producer.
- Modified `scripts/concept_registry/resource_qualification.py` and its tests —
  verified Release path, byte measurement, raw override rejection, and CLI fix.
- Modified `scripts/concept_registry/production_qualification.py` and its tests
  — frozen production API and non-claiming ephemeral helper.
- Modified `scripts/concept_registry/test_ccid_stability_diff.py` — real signed
  CCID integration and mutation rejection.
- Modified
  `src/onebrain-node/examples/concept_registry_production_qualification.rs` —
  shared verifier invocation, actual measurements, signature identity, and
  fresh-manager reopen.
- Modified `src/onebrain-node/src/concept_registry_runtime.rs` — monotonic
  generation install and overlap regression.

The first five created files intentionally expand the original Task 20 list per
the owner's resolution to pull forward only the minimal reusable Task 27
qualification-approver policy/verifier contract. No Task 27 manifest,
release-envelope, tag, or publication workflow was implemented.

## Self-review

- Recomputed the exact owner-approved policy digest from the literal policy
  object with UTF-8, sorted keys, compact separators, `ensure_ascii=false`, and
  BLAKE3 derive-key context; it equals
  `2e7cc2dacafad658ab5fe4e1536a4b92590f788c9c9e5a450d123930d65cfbd6`.
- Confirmed `VALIDSIG` supplies the full primary fingerprint and that valid but
  unlisted signatures, tampered bytes, expired requests, noncanonical bytes,
  wrong algorithm, and extra schema fields fail closed.
- Confirmed no production private key, key path, or material was read, printed,
  or committed. Tests generate ephemeral keys only under temporary GPG homes.
- Confirmed resource and generation Release producers derive context/bindings
  from the verifier and compare the exact artifact/environment bytes they use.
- Confirmed Prequalification remains closed, omits Release-only fields, sets
  `base_candidate_bound=false`, and cannot claim the Registry production gate.
- Confirmed the public aggregate cannot claim production with an ephemeral
  identity; the explicit test helper always emits false.
- Confirmed generation rollback/reactivation reads exact root/generation from a
  newly opened manager and refresh installation is monotonic under overlap.
- Confirmed formatting, diff checks, focused suites, examples, and validator are
  green after the last verifier creation-time check.

## Concerns / explicitly open reviewer findings

This fix round is **not Task 20 completion**. Two critical/important clusters
remain for round 2:

1. `src/ku-core/examples/concept_registry_failure_qualification.rs` still
   accepts raw Release context/binding JSON. It must invoke the shared signed
   request verifier, measure its exact five payload/stamp/probe/environment
   inputs, forbid overrides, and emit sanitized full invocation provenance.
2. `scripts/concept_registry/release_cycle_qualification.py` still executes
   caller-selected processes and trusts their JSON step claims; its existing
   happy test remains a print-only fabrication. It must be replaced by a real
   small-fixture nine-step first-party cycle with independent stamp/root/state/
   generation/query/CCID inspection after every operation, then participate in
   an all-real-producer aggregate integration.

Consequently, command provenance is complete for the new resource and CCID
paths but still summary-only in the legacy failure/release-cycle paths, and no
all-eight-real-receipt aggregation test exists yet. The CCID test's missing
separately recorded RED is also a TDD process concern. These items prevent a
`DONE` status for round 1.

---

# Fix loop round 2/5 — closed qualification evidence chain

## Implementation summary

- Closed the production verifier boundary. Python production verification uses
  only `/usr/bin/gpg`; Rust production producers use `/usr/bin/python3`, the
  candidate-owned verifier, and `/usr/bin/git`. Executable injection exists
  only in explicitly named non-production test helpers whose verified context
  is required to carry `production=false` and whose aggregate cannot claim the
  Registry production subgate.
- Extended candidate measurement through Git commit/tree/object format,
  semantic evidence, target artifact tuple, canonical profile/vector, IDL
  history root, exact candidate-owned tooling, five installed payloads,
  release stamp/root/generation, probe/signature identity, executable,
  toolchain, runner image, and target triple.
- Replaced failure-harness raw Release context/binding arguments with signed
  request modes. The compiled behavioral integration proves the legacy raw
  shape is rejected before inputs are read and that an ephemeral signed
  request plus actual installed Registry/Git/evidence state is accepted only
  as non-production evidence.
- Replaced the caller-command release cycle with a narrow compiled first-party
  release-operation bridge. The cycle independently inspects package and
  verify stamps/roots, activation states/generations, actual OBR queries, exact
  candidate payload/stamp bytes, the real CCID diff, rollback state, and final
  reactivation at signed generation 4. No caller plan or step JSON is accepted.
- Added signed-request verification to the real CCID receipt producer and
  exact six-file byte comparison before invoking the SQLite-backed diff.
- Producers now derive canonical sanitized command provenance internally.
  Rust producers hash the actual argv/files while redacting GPG/private-key
  paths; Python producers construct full measured input/tool/profile option
  identities. Tests independently recompute every emitted `command_blake3`,
  prove an omitted option changes it, and assert the private-key path is absent.
- Added an all-real-producer integration: four measured resource producer
  receipts, compiled failure and generation receipts, the real CCID producer,
  and the compiled first-party nine-step cycle feed the test-only aggregate.
  No direct component receipt constructor or handcrafted signed payload is
  used; the aggregate verifies every signature/equality binding and emits
  `registry_production_qualified=false`.

## RED evidence

### Closed verifier and candidate measurement

```powershell
python -m unittest scripts.release.test_verify_base_release_request.SignedReleaseRequestTests.test_production_verifier_api_has_no_executable_or_policy_mode_injection -v
```

Expected RED: `AssertionError: 'gpg_executable' unexpectedly found` because the
public production verifier still accepted a caller-selected GPG executable.

```powershell
python -m unittest scripts.release.test_verify_base_release_request.SignedReleaseRequestTests.test_candidate_measurement_api_requires_git_state_release_state_and_semantic_evidence -v
```

Expected RED: `AssertionError: 'candidate_root' not found` (and the other six
state/evidence parameters), because measurement covered hashes but not actual
candidate Git/semantic/installed state.

```powershell
python -m unittest scripts.release.test_verify_base_release_request.SignedReleaseRequestTests.test_explicit_nonproduction_cli_can_never_return_production_context -v
```

Expected RED: argparse rejected `--test-nonproduction-gpg` as unrecognized.
The minimal GREEN path explicitly returns `production:false` and is separate
from the non-injectable production API.

### CCID signed boundary

```powershell
python -m unittest scripts.release.test_verify_base_release_request.SignedReleaseRequestTests.test_ccid_producer_verifies_signed_request_before_exact_input_diff -v
```

Expected RED: `ImportError: cannot import name
'qualify_ccid_stability_from_signed_request_for_test_nonproduction'`. The GREEN
test verifies canonical request/signature bytes with an isolated ephemeral GPG
home, invokes the real diff, and rejects a mutated candidate input.

### Compiled failure boundary

```powershell
cargo build --manifest-path src/Cargo.toml -p ku-core --features concept-registry-failure-harness --example concept_registry_failure_qualification
$env:ONEBRAIN_REGISTRY_FAILURE_QUALIFICATION=(Resolve-Path 'src\target\debug\examples\concept_registry_failure_qualification.exe')
python -m unittest scripts.concept_registry.test_failure_qualification.FailureQualificationIntegrationTests.test_raw_release_context_binding_cli_is_rejected_before_inputs_are_read -v
```

Expected initial RED: the old binary attempted to open a raw caller path rather
than rejecting it. After the mode boundary existed, the signed acceptance test
was RED with `explicit --prequalification or --release mode is required`
because the explicit non-production signed mode did not yet exist.

The behavioral command-provenance regression was separately RED with
`KeyError: 'command_blake3'` when inspecting the emitted failure receipt.

The final self-audit added a behavioral reference-environment mutation to the
all-real test. Its RED failed with `unexpected argument: ...rust-toolchain.txt`
instead of the required `reference environment` rejection, proving the
compiled failure producer did not yet accept/measure probe signature,
toolchain, or runner evidence. GREEN adds those exact inputs, independently
hashes them and the executing binary, verifies the probe signer
fingerprint/public key and detached Ed25519 signature, and rejects a mutated
runner before running the real failure drills.

### First-party release cycle

```powershell
python -m unittest scripts.concept_registry.test_release_cycle_qualification.ReleaseCycleQualificationTests.test_release_cycle_api_cannot_accept_caller_step_plan_or_commands -v
```

Expected RED: `plan` remained in the public signature and the implementation
executed caller-provided commands.

```powershell
$env:ONEBRAIN_REGISTRY_RELEASE_OPS=(Resolve-Path 'src\target\debug\examples\concept_registry_release_ops.exe')
python -m unittest scripts.release.test_verify_base_release_request.SignedReleaseRequestTests.test_first_party_nine_step_cycle_inspects_real_state_and_signed_inputs -v
```

The first real-cycle RED was `CycleError: built candidate manifest differs
from signed request`. Root cause: rebuilding a previously signed candidate
regenerated the manifest timestamp. GREEN treats the exact signed candidate
bytes as immutable and uses the real package operation to build the signed
generation, then independently compares all five payloads/stamp/root.

### Exact provenance and all-real aggregation

```powershell
python -m unittest scripts.release.test_verify_base_release_request.SignedReleaseRequestTests.test_all_release_producers_bind_full_sanitized_command_provenance -v
```

Expected RED: generation and failure sources omitted `command_blake3` and used
two-token summaries.

```powershell
python -m unittest scripts.release.test_verify_base_release_request.SignedReleaseRequestTests.test_python_producers_do_not_accept_caller_command_provenance -v
```

Expected RED: CCID exposed an `invocation` parameter, allowing caller-selected
provenance. Resource and CCID now derive provenance from measured inputs.

```powershell
python -m unittest scripts.release.test_verify_base_release_request.SignedReleaseRequestTests.test_resource_production_cli_has_no_caller_selected_gpg -v
```

Expected RED: `parser.add_argument("--gpg"` was present and the CLI forwarded
`gpg_executable=args.gpg`.

The first all-real aggregate execution reached the production validator and
failed with `AggregationError: component is not bound to the Base candidate`.
Root cause: the resource receipt incorrectly equated non-production signer
identity with lack of signed candidate binding. After fixing that distinction,
the next RED was `component release_stamp_blake3 mismatch`; the failure harness
validated the installed signed stamp but emitted its timestamped drill-copy
stamp. Release mode now emits the verified installed stamp digest.

## GREEN verification

### Producer builds and focused Python suite

```powershell
cargo build --locked --manifest-path src/Cargo.toml -p ku-core --example registry_probe --example concept_registry_release_ops --example concept_registry_failure_qualification --features concept-registry-failure-harness
cargo build --locked --manifest-path src/Cargo.toml -p onebrain-node --example concept_registry_production_qualification
```

Result: PASS. Only the pre-existing unrelated `ku-kql` dead-code warning was
reported.

```powershell
$env:ONEBRAIN_REGISTRY_PROBE=(Resolve-Path 'src\target\debug\examples\registry_probe.exe')
$env:ONEBRAIN_REGISTRY_FAILURE_QUALIFICATION=(Resolve-Path 'src\target\debug\examples\concept_registry_failure_qualification.exe')
$env:ONEBRAIN_REGISTRY_RELEASE_OPS=(Resolve-Path 'src\target\debug\examples\concept_registry_release_ops.exe')
$env:ONEBRAIN_REGISTRY_GENERATION_QUALIFICATION=(Resolve-Path 'src\target\debug\examples\concept_registry_production_qualification.exe')
python -m unittest scripts.release.test_verify_base_release_request scripts.concept_registry.test_resource_qualification scripts.concept_registry.test_ccid_stability_diff scripts.concept_registry.test_production_qualification scripts.concept_registry.test_release_cycle_qualification scripts.concept_registry.test_failure_qualification -v
```

Result: PASS — 45 tests, 0 failures/errors, 1 expected Linux-only skip. The
compiled probe, failure, generation, release-operation, signed-request, real
CCID, first-party cycle, and all-real aggregate paths ran on Windows.

### Required Rust filters

```powershell
cargo test --locked --manifest-path src/Cargo.toml -p ku-core concept_registry_release -- --test-threads=1
```

Result: PASS — 10 passed, 0 failed.

```powershell
cargo test --locked --manifest-path src/Cargo.toml -p onebrain-node --lib concept_registry_runtime -- --test-threads=1
```

Result: PASS — 13 passed, 0 failed.

### Syntax, contracts, format, and diff

```powershell
python -m py_compile scripts/concept_registry/resource_qualification.py scripts/concept_registry/ccid_stability_qualification.py scripts/concept_registry/release_cycle_qualification.py scripts/release/verify_base_release_request.py
python scripts/ci/validate_vnext_contracts.py
cargo fmt --manifest-path src/Cargo.toml --all -- --check
git diff --check
```

Result: PASS. Validator ended with `vNext contracts OK ... 444 local links`.

## Files changed in round 2

- Modified `scripts/release/verify_base_release_request.py` and
  `scripts/release/test_verify_base_release_request.py`: fixed production
  verifier boundary, full candidate measurement, signed producer/e2e tests.
- Modified `scripts/concept_registry/resource_qualification.py`: exact
  production measurement inputs, fixed GPG policy, internally derived
  provenance, explicit non-claiming test producer.
- Modified `scripts/concept_registry/ccid_stability_qualification.py` and
  `test_ccid_stability_diff.py`: closed signed-request producer and internally
  derived exact six-input provenance; removed fabricated context test.
- Replaced `scripts/concept_registry/release_cycle_qualification.py` behavior
  and its test: compiled first-party operations and independent real-state
  inspection, with no caller step plan.
- Modified `scripts/concept_registry/test_failure_qualification.py` and
  `src/ku-core/examples/concept_registry_failure_qualification.rs`: behavioral
  raw rejection, signed acceptance, actual measurements, and exact provenance.
- Modified
  `src/onebrain-node/examples/concept_registry_production_qualification.rs`:
  closed signed verification, actual candidate evidence, explicit non-claiming
  test mode, and full invocation provenance.
- Added `src/ku-core/src/qualification_request.rs`: smallest shared fixed-path
  Rust bridge to the candidate-owned verifier. Added its required module export
  in `src/ku-core/src/lib.rs`; without that one-line integration change the two
  producer examples cannot call the shared verifier.
- Added `src/ku-core/examples/concept_registry_release_ops.rs`: smallest narrow
  compiled first-party bridge for package/verify/activate/rollback. It emits no
  trusted step JSON and is independently inspected by the Python cycle.

These two added Rust files and the `lib.rs` export intentionally expand the
original Task 20 list under the round-2 owner direction. No Task 27 manifest,
tag, release publication, or Base signer workflow was implemented.

## Self-review

- Re-read the round-2 findings and audited all six clusters. Production
  verifier/Python/GPG/Git executable selection is fixed; injection is isolated
  behind explicitly named non-production helpers that reject production
  contexts and cannot make a production aggregate claim.
- Confirmed the approved policy canonical preimage recomputes the exact
  owner-approved derive-key digest and `VALIDSIG` full primary fingerprint is
  required; valid unlisted, tampered, expired, and extended-schema requests
  fail closed.
- Confirmed every Release producer derives context from signed bytes and
  compares the candidate state/artifacts/environment it actually uses. No raw
  Release context or binding override remains. Prequalification remains
  closed, non-candidate-bound, and omits Release-only fields.
- Confirmed the compiled failure producer specifically measures the five
  payloads, installed stamp/root/generation, executable/probe, detached probe
  signature and signer identity, Rust toolchain, runner image, target, Git
  commit/tree, semantic evidence, profile/vector, and IDL history.
- Confirmed all nine cycle steps are first-party and independently inspected;
  the test mutates a signed candidate input and observes rejection.
- Confirmed the all-real integration uses real producer calls/binaries for all
  required receipt kinds, then independently recomputes provenance digests and
  runs the signature/equality aggregator without a direct receipt constructor.
- Confirmed no production private key, path, or material was read or printed.
  Every signing test uses ephemeral keys and isolated temporary GPG homes.
- Confirmed the Registry result remains a Registry-only subgate and the
  non-production aggregate is false; nothing in this round implies
  `BASE-GATE-V1`.

## Concerns

- This Windows host cannot run the documented Linux-only RLIMIT/cache resource
  integration; it remains the single expected skip. The compiled probe and all
  other round-2 process integrations ran.
- No full-size production measurements or production private signing key are
  present or used. The test-only aggregate intentionally cannot claim
  production.

---

# Fix round 3/5 — harden qualification trust boundary

## Implementation summary

- The signed receipt payload contract now carries an explicit `evidence_tier`:
  `production-reference`, `nonproduction-test`, or `prequalification`.
  Production aggregation rejects a nonproduction set before frozen signer/key
  availability can be the only fence. `base_candidate_bound` remains separate.
- Python and Rust independently encode the approved Base compatibility tuple
  (`u16` field ID + `u32` little-endian length + exact field bytes), recompute
  semantic fields 1-14 and artifact fields 1-16, and bind the artifact tuple to
  the measured target and toolchain. Payload/stamp/probe/executable/runner
  hashes remain separate mandatory measurements.
- The nine-step cycle binds the signed Git commit/tree and exact candidate-owned
  bridge. After every step it independently hashes exact installed payload
  lengths/bytes, recomputes the aggregate root, recomputes authoritative state
  roots from Rust struct-order bytes, resolves the active release root, and
  queries actual OBR/CCID data. It signs exact sanitized step commands and the
  complete top-level invocation.
- Resource provenance now includes and cross-checks labels, requested cache
  strategy, budget profile, timeout, all payload/tool/profile/environment
  identities, and fixed redactions. The integration independently constructs
  literal expected resource and cycle invocations.
- Rust production verification now uses only fixed `/usr/bin/python3` and
  `/usr/bin/gpg`; locally verifies canonical request and frozen policy bytes,
  the approved derive-key policy digest, closed request fields, OpenPGP
  `VALIDSIG` primary fingerprint/Ed25519 algorithm/signature time, current and
  signer validity intervals, exported public packet hash, fixed tooling hashes,
  and exact equality of all eight closed Python result fields and every binding.
- The authoritative Task 19 profile/vector/validator were updated for the
  closed evidence tier and fixed Python/GPG identities. No Task 27 manifest,
  tag, release publication, or private production key operation was added.

## RED evidence

### Explicit evidence tier

```powershell
python -m unittest scripts.concept_registry.test_production_qualification.ProductionQualificationTests.test_production_evidence_preflight_rejects_nonproduction_receipts_before_signer
```

RED: `ImportError: cannot import name '_require_production_reference_evidence'`.
Expected reason: no signed production-reference identity or early production
component preflight existed.

### Approved tuple bytes

```powershell
python -m unittest scripts.release.test_verify_base_release_request.SignedReleaseRequestTests.test_approved_compatibility_tuple_bytes_recompute_semantic_and_artifact_digests
```

RED: `ImportError: cannot import name 'canonical_compatibility_tuple_bytes'`.
Expected reason: semantic evidence was still a digest line and artifact evidence
was two self-consistent request fields.

### Rust fake verifier output

```powershell
cargo test --manifest-path src/Cargo.toml -p ku-core fake_python_stdout_without_authenticated_bindings_is_rejected -- --exact
```

RED compile error: `cannot find function validate_python_context in this scope`.
Expected reason: Rust accepted Python stdout after checking only format and the
production boolean. The later exact focused command omitted `--exact` because
Rust test names include the module path.

### Exact resource provenance

```powershell
python -m unittest scripts.release.test_verify_base_release_request.SignedReleaseRequestTests.test_resource_receipt_api_requires_every_behavior_affecting_option
```

RED: four assertion failures: `labels_file`, `cache_strategy`,
`budget_profile`, and `timeout_seconds` were absent from the producer API.
Expected reason: the signed command omitted real behavior-affecting options.

### Real-cycle debugging REDs

```powershell
$env:ONEBRAIN_REGISTRY_RELEASE_OPS=(Resolve-Path 'src/target/release/examples/concept_registry_release_ops.exe')
$env:ONEBRAIN_REGISTRY_FAILURE_QUALIFICATION=(Resolve-Path 'src/target/release/examples/concept_registry_failure_qualification.exe')
$env:ONEBRAIN_REGISTRY_GENERATION_QUALIFICATION=(Resolve-Path 'src/target/release/examples/concept_registry_production_qualification.exe')
python -m unittest scripts.release.test_verify_base_release_request.SignedReleaseRequestTests.test_first_party_nine_step_cycle_inspects_real_state_and_signed_inputs
```

First RED: `CycleError: active state root does not match authoritative state
bytes`. Root cause: Rust hashes compact serde struct field order, while the
first Python recomputation sorted keys. After correcting that, the next RED was
`measured candidate semantic digest differs from signed request`; the two Rust
producers still read a digest line instead of deriving tuple bytes. Both were
replaced with the shared Rust tuple encoder. The debug suite later exposed
`release bridge is not the fixed candidate-owned operation`; the nonproduction
constraint now accepts only the exact candidate-root debug/release example,
while production remains release-only.

```powershell
python -m unittest scripts.concept_registry.test_release_cycle_qualification -v
```

RED: the legacy static assertion rejected any `subprocess.run`, including the
new trusted Git commit/tree measurement. It now checks the real security
boundary: no caller plan/commands/candidate root, fixed candidate verification,
and first-party `Popen` operations.

The combined compiled regression behaviorally mutates and rejects:

- semantic tuple `feature_set_digest` bytes: `measured candidate semantic digest differs`;
- artifact tuple target: `artifact tuple target/toolchain differs`;
- installed `concepts.obr` bytes: `installed payload bytes differ from stamp`;
- authoritative `active_release` with unchanged root: `active state root does not match authoritative state bytes`.

## GREEN verification

### Focused behavior

```powershell
python -m unittest scripts.concept_registry.test_production_qualification.ProductionQualificationTests.test_production_evidence_preflight_rejects_nonproduction_receipts_before_signer scripts.concept_registry.test_production_qualification.ProductionQualificationTests.test_nonproduction_helper_verifies_components_but_cannot_claim_subgate
```

PASS: 2 tests.

```powershell
python -m unittest scripts.release.test_verify_base_release_request.SignedReleaseRequestTests.test_approved_compatibility_tuple_bytes_recompute_semantic_and_artifact_digests
```

PASS: 1 test; exact frozen semantic and artifact golden digests matched.

```powershell
cargo test --locked --manifest-path src/Cargo.toml -p ku-core fake_python_stdout_without_authenticated_bindings_is_rejected -- --nocapture
```

PASS: 1 test, 0 failed.

```powershell
python -m unittest scripts.release.test_verify_base_release_request.SignedReleaseRequestTests.test_actual_python_verifier_and_gpg_byte_mutations_fail_measurement scripts.release.test_verify_base_release_request.SignedReleaseRequestTests.test_authenticated_runtime_tooling_identity_rejects_python_and_gpg_mutations scripts.release.test_verify_base_release_request.SignedReleaseRequestTests.test_authenticated_verifier_tooling_identity_rejects_altered_bytes -v
```

PASS: 3 tests. Copies of the actual Python image, verifier script, and GPG
binary were byte-mutated and independently rejected by digest measurement.

```powershell
$env:ONEBRAIN_REGISTRY_RELEASE_OPS=(Resolve-Path 'src/target/debug/examples/concept_registry_release_ops.exe')
$env:ONEBRAIN_REGISTRY_FAILURE_QUALIFICATION=(Resolve-Path 'src/target/debug/examples/concept_registry_failure_qualification.exe')
$env:ONEBRAIN_REGISTRY_GENERATION_QUALIFICATION=(Resolve-Path 'src/target/debug/examples/concept_registry_production_qualification.exe')
python -m unittest scripts.release.test_verify_base_release_request.SignedReleaseRequestTests.test_first_party_nine_step_cycle_inspects_real_state_and_signed_inputs -v
```

PASS: 1 test. It ran nine real operations, independently compared literal
resource/cycle commands, produced all five real receipt kinds, aggregated them
without handcrafted receipts, and executed all four byte/state mutations.

### Required builds and suites

```powershell
cargo build --locked --manifest-path src/Cargo.toml -p ku-core --example registry_probe --example concept_registry_release_ops --example concept_registry_failure_qualification --features concept-registry-failure-harness
cargo build --locked --manifest-path src/Cargo.toml -p onebrain-node --example concept_registry_production_qualification
```

PASS: locked debug builds completed; only the pre-existing unrelated `ku-kql`
dead-code warning appeared.

```powershell
$env:ONEBRAIN_REGISTRY_PROBE=(Resolve-Path 'src/target/debug/examples/registry_probe.exe')
$env:ONEBRAIN_REGISTRY_FAILURE_QUALIFICATION=(Resolve-Path 'src/target/debug/examples/concept_registry_failure_qualification.exe')
$env:ONEBRAIN_REGISTRY_RELEASE_OPS=(Resolve-Path 'src/target/debug/examples/concept_registry_release_ops.exe')
$env:ONEBRAIN_REGISTRY_GENERATION_QUALIFICATION=(Resolve-Path 'src/target/debug/examples/concept_registry_production_qualification.exe')
python -m unittest scripts.release.test_verify_base_release_request scripts.concept_registry.test_resource_qualification scripts.concept_registry.test_ccid_stability_diff scripts.concept_registry.test_production_qualification scripts.concept_registry.test_release_cycle_qualification scripts.concept_registry.test_failure_qualification
```

PASS: 51 tests, 0 failures/errors, 1 expected Linux-only skip in 47.495s.

```powershell
cargo test --locked --manifest-path src/Cargo.toml -p ku-core concept_registry_release -- --test-threads=1
cargo test --locked --manifest-path src/Cargo.toml -p onebrain-node --lib concept_registry_runtime -- --test-threads=1
```

PASS: 10/10 and 13/13 respectively.

```powershell
python -m py_compile scripts/concept_registry/resource_qualification.py scripts/concept_registry/ccid_stability_qualification.py scripts/concept_registry/release_cycle_qualification.py scripts/concept_registry/production_qualification.py scripts/release/verify_base_release_request.py scripts/release/test_verify_base_release_request.py
python scripts/ci/validate_vnext_contracts.py
cargo fmt --manifest-path src/Cargo.toml --all -- --check
git diff --check
```

PASS: syntax, format, and diff checks passed. Validator reported 726 normative
lines and 444 local links.

## Files changed in round 3

- Updated the Task 19 production qualification profile, machine vector, and
  validator for evidence tiers and fixed Python/GPG tooling identities.
- Updated Python verifier/tests, resource/cycle/CCID producers, pure aggregate,
  and relevant tests for tuple derivation, exact provenance, state/root
  inspection, tool identities, and production-reference separation.
- Updated the shared Rust request bridge and both compiled Release producers
  for independent OpenPGP/tool/context verification and Base tuple derivation.
- Updated the existing release-cycle static regression to test the current
  no-caller-plan boundary. No unrelated production file was added.

## Self-review

- Audited all four round-3 clusters against the open-findings file after the
  final 51-test run. Each has behavioral, not merely source-text, coverage.
- Confirmed cycle inspection hashes real installed bytes and authoritative
  state after every operation; bridge stdout is rejected and never trusted.
- Confirmed Base artifact tuple layout was not broadened: it is semantic fields
  plus measured target/toolchain only. Registry payload/environment bindings
  remain separate exact prerequisites.
- Confirmed every Release producer emits the explicit tier derived from its
  closed verified context. Nonproduction receipts remain candidate-bound facts
  but cannot enter production aggregation.
- Confirmed production executable selection has no caller injection; test
  helpers remain explicitly nonproduction and cannot claim the subgate.
- Confirmed private signing paths/material are fixed-redacted from provenance;
  tests use only isolated ephemeral GPG and Ed25519 keys. The production private
  key was never read or used.

## Concerns

- This Windows host cannot execute the Linux-only RLIMIT/cache production
  resource integration; it remains the one expected skip.
- Production APIs intentionally fail closed on Windows. No full-size production
  artifacts, production signature, or `BASE-GATE-V1` claim were produced.

---

# Fix round 4/5 — close remaining qualification gaps

## Status

DONE_WITH_CONCERNS. Both round-4 findings are behaviorally closed. This round
does not claim that production measurements exist or that `BASE-GATE-V1`
passes.

## Implementation summary

- The release cycle now requires explicit pre-existing
  `candidate_release_stamp` and `candidate_state` files as untrusted locators.
  Before creating the mutable cycle root or starting operation 1, it verifies
  canonical Rust stamp/state bytes, exact closed nested shapes and lowercase
  encodings, recomputed source root, manifest bindings, safe/distinct release
  identities, the release Ed25519 signature and key identity, signed sources,
  exact previous/active state and artifact roots/generation, the
  exact five staged and operation-input payloads, and the existing complete
  Git/semantic/artifact/profile/vector/IDL/tool/probe/signature/environment
  measurement set. The post-step inspections and post-cycle full remeasurement
  remain unchanged defense-in-depth.
- The two new pre-operation inputs are explicit required keyword parameters.
  Their sanitized provenance identities are frozen from the exact bytes
  authenticated before operation 1, rather than re-reading untrusted locators
  after the cycle. The production entry still fixes the candidate repository,
  Git executable,
  request verifier, and release-operation bridge; path identity itself grants
  no trust.
- The Rust Base tuple encoder now validates the exact approved 16-field
  top-level schema, every nested object field set and scalar type/range, known
  commit/toolchain forms, lowercase hexadecimal, and bounded ASCII strings
  before deriving tuple bytes. The approved field IDs, framing, semantic
  fields 1–14, and artifact fields 1–16 are unchanged. Existing canonical-byte
  comparison continues to reject duplicate and noncanonical JSON.
- Real compiled failure and generation producers are exercised with canonical
  tuple JSON containing an unknown top-level field while all approved fields
  remain unchanged. Failure rejects before even creating its work root or
  emitting a receipt. Generation is mutated on a fresh stable-only Registry,
  emits no receipt, and leaves every Registry file byte-identical before the
  separately executed happy generation swap.

## RED evidence

### Complete candidate measurement occurred after operation output

Command:

```powershell
$env:ONEBRAIN_REGISTRY_RELEASE_OPS=(Resolve-Path 'src\target\debug\examples\concept_registry_release_ops.exe').Path
$env:ONEBRAIN_REGISTRY_FAILURE_QUALIFICATION=(Resolve-Path 'src\target\debug\examples\concept_registry_failure_qualification.exe').Path
$env:ONEBRAIN_REGISTRY_GENERATION_QUALIFICATION=(Resolve-Path 'src\target\debug\examples\concept_registry_production_qualification.exe').Path
python -m unittest scripts.release.test_verify_base_release_request.SignedReleaseRequestTests.test_first_party_nine_step_cycle_inspects_real_state_and_signed_inputs -v
```

Expected RED against the round-3 implementation:

```text
AssertionError: True is not false : candidate mismatch must cause zero first-party operation output
Ran 1 test in 5.122s
FAILED (failures=1)
```

The real signed semantic mismatch was detected only after the cycle created its
Registry root and ran operations.

### Rust tuple schemas were not closed

Command:

```powershell
cargo test --locked --manifest-path src/Cargo.toml -p ku-core qualification_request::tests::base_tuple_schema_rejects_unknown_missing_and_wrong_type_fields -- --exact --nocapture
```

Expected RED:

```text
accepted invalid Base tuple schemas: ["extra top-level field", "extra nested field"]
test result: FAILED. 0 passed; 1 failed
```

The missing and wrong-type table rows already rejected; the failure isolated
the absent top-level/nested closure.

### Compiled producer mutation check

The Rust unit/table RED above was the pre-implementation RED that drove the
schema gate. The compiled producer assertions were added after that minimal
implementation, so their negative proof was recorded honestly as a mutation
check: binaries were rebuilt once with only the new schema-gate call disabled,
then the real integration was run.

```powershell
cargo build --locked --manifest-path src/Cargo.toml -p ku-core --example registry_probe --example concept_registry_release_ops --example concept_registry_failure_qualification --features concept-registry-failure-harness
cargo build --locked --manifest-path src/Cargo.toml -p onebrain-node --example concept_registry_production_qualification
python -m unittest scripts.release.test_verify_base_release_request.SignedReleaseRequestTests.test_first_party_nine_step_cycle_inspects_real_state_and_signed_inputs -v
```

Mutation-check result:

```text
AssertionError: 0 == 0
Ran 1 test in 18.025s
FAILED (failures=1)
```

The real failure binary accepted the unknown tuple field, completed, and
returned zero when the gate was disabled. The gate was immediately restored,
the binaries rebuilt, and the same behavioral test passed.

### Reviewer-edge REDs for complete pre-operation semantics and ordering

The same focused all-real command above was extended with a legitimately
re-signed candidate stamp whose `source_root` did not match its exact sources,
and with an assertion that compiled failure tuple rejection precedes creation
of the harness work root. Before the corresponding production fixes it failed
for both intended reasons:

```text
AssertionError: 4 != 0 : candidate mismatch must run zero first-party operations
AssertionError: True is not false : unknown tuple field must be rejected before failure work-root creation
Ran 1 test in 10.986s
FAILED (failures=2)
```

The re-signed bad stamp passed the old Python checks and reached four real
bridge operations. Separately, the real failure binary rejected the tuple only
after creating its work root. A state with the same valid old/candidate release
identity, a recomputed state root, and matching caller operation IDs isolated
the distinct-cycle invariant:

```text
AssertionError: 4 != 0 : candidate mismatch must run zero first-party operations
Ran 1 test in 17.849s
FAILED (failures=1)
```

An earlier focused state mutation with a valid recomputed root but arbitrary
`previous_release` also ran the cycle and returned no error, failing with
`unexpectedly None : candidate mismatch must fail closed` in 19.515s. These
REDs drove source-root/schema/manifest validation, previous-release binding,
and distinct safe release IDs in the pre-operation context.

The append-only state reader also received a focused type regression:

```powershell
python -m unittest scripts.concept_registry.test_release_cycle_qualification -v
```

Before the explicit integer/non-boolean check, JSON `true` was treated as
generation 1 and failed later with `installed stamp is invalid` instead of the
required `active state generation is not append-only`. The restored focused
suite passed 2/2 tests in 0.005s.

### Frozen-provenance mutation check

The reviewer-added regression changes the staged stamp/state locator bytes
after preflight and requires the receipt command to retain the identities of
the bytes actually authenticated. With the two command fields deliberately
changed back to late path re-reads, the focused integration failed as intended:

```text
AssertionError: '--candidate-release-stamp=release.stamp.json@blake3:<authenticated>' not found in [... '--candidate-release-stamp=release.stamp.json@blake3:<changed>' ...]
Ran 1 test in 7.154s
FAILED (failures=1)
```

The frozen measured-context fields were restored immediately.

## GREEN verification

### Focused cycle and compiled producer behavior

```powershell
$env:ONEBRAIN_REGISTRY_RELEASE_OPS=(Resolve-Path 'src\target\debug\examples\concept_registry_release_ops.exe').Path
$env:ONEBRAIN_REGISTRY_FAILURE_QUALIFICATION=(Resolve-Path 'src\target\debug\examples\concept_registry_failure_qualification.exe').Path
$env:ONEBRAIN_REGISTRY_GENERATION_QUALIFICATION=(Resolve-Path 'src\target\debug\examples\concept_registry_production_qualification.exe').Path
python -m unittest scripts.release.test_verify_base_release_request.SignedReleaseRequestTests.test_first_party_nine_step_cycle_inspects_real_state_and_signed_inputs -v
```

Result: PASS - 1 test in 20.200s. For each semantic, payload,
valid-root-but-wrong-previous-release state, and tooling mutation,
`_execute_bridge`, `_query_obr`, and the real CCID diff had zero calls; no
receipt was returned and the unique operation root remained absent. The same
zero-operation assertions cover a legitimately re-signed wrong source root and
same old/candidate release identity. A second real cycle proves provenance uses
the initially authenticated stamp/state bytes even when those untrusted
locators change after preflight. The test then completes the normal nine-step
happy cycle and both compiled unknown-field producer regressions: failure
rejects before work-root creation, and generation rejects on fresh pre-swap
state without changing any Registry byte.

### Rust tuple and required Task 20 filters

```powershell
cargo test --locked --manifest-path src/Cargo.toml -p ku-core qualification_request -- --test-threads=1
cargo test --locked --manifest-path src/Cargo.toml -p ku-core concept_registry_release -- --test-threads=1
cargo test --locked --manifest-path src/Cargo.toml -p onebrain-node --lib concept_registry_runtime -- --test-threads=1
```

Result: PASS - 3/3 tuple/request tests, 10/10 release tests, and 13/13 runtime
tests. The tuple filter includes exact-schema and duplicate/noncanonical JSON
coverage.

### Full compiled Python integration regression

```powershell
$env:ONEBRAIN_REGISTRY_PROBE=(Resolve-Path 'src\target\debug\examples\registry_probe.exe').Path
$env:ONEBRAIN_REGISTRY_FAILURE_QUALIFICATION=(Resolve-Path 'src\target\debug\examples\concept_registry_failure_qualification.exe').Path
$env:ONEBRAIN_REGISTRY_RELEASE_OPS=(Resolve-Path 'src\target\debug\examples\concept_registry_release_ops.exe').Path
$env:ONEBRAIN_REGISTRY_GENERATION_QUALIFICATION=(Resolve-Path 'src\target\debug\examples\concept_registry_production_qualification.exe').Path
python -m unittest scripts.release.test_verify_base_release_request scripts.concept_registry.test_resource_qualification scripts.concept_registry.test_ccid_stability_diff scripts.concept_registry.test_production_qualification scripts.concept_registry.test_release_cycle_qualification scripts.concept_registry.test_failure_qualification -v
```

Result: PASS - 52 tests, 0 failures/errors, 1 expected Linux-only skip in
56.369s. All compiled probe, failure, generation, release-operation, signed
request, real CCID, cycle, provenance, evidence-tier, and aggregate regressions
ran on Windows.

### Examples, syntax, contracts, format, and diff

```powershell
python -m py_compile scripts/concept_registry/resource_qualification.py scripts/concept_registry/ccid_stability_qualification.py scripts/concept_registry/release_cycle_qualification.py scripts/concept_registry/production_qualification.py scripts/concept_registry/test_release_cycle_qualification.py scripts/release/verify_base_release_request.py scripts/release/test_verify_base_release_request.py
cargo check --locked --manifest-path src/Cargo.toml -p ku-core --example registry_probe --example concept_registry_release_ops --example concept_registry_failure_qualification --features concept-registry-failure-harness
cargo check --locked --manifest-path src/Cargo.toml -p onebrain-node --example concept_registry_production_qualification
python scripts/ci/validate_vnext_contracts.py
cargo fmt --manifest-path src/Cargo.toml --all -- --check
git diff --check
```

Result: PASS. The validator reported 726 normative lines and 444 local links.
Git emitted only the Windows LF-to-CRLF working-copy advisory.

## Files changed in round 4

- Modified `scripts/concept_registry/release_cycle_qualification.py` — complete
  immutable candidate preflight before operation 1, Rust-equivalent
  stamp/state semantics, and frozen authenticated-byte provenance.
- Modified `scripts/concept_registry/test_release_cycle_qualification.py` —
  append-only state generation rejects JSON booleans before release lookup.
- Modified `scripts/release/test_verify_base_release_request.py` — real staged
  generation-4 context, semantic/payload/state/tool/source-root/release-ID
  zero-operation cases, frozen-provenance behavior, and compiled
  failure/generation unknown-field behavior on observable pre-operation state.
- Modified `src/ku-core/examples/concept_registry_failure_qualification.rs` —
  verifies Base tuple evidence before creating the failure-harness work root.
- Modified `src/ku-core/src/qualification_request.rs` — closed tuple schema and
  Rust table/canonical-byte tests.
- Appended this round to `task-20-report.md`.

## Self-review

- Confirmed `registry_root.mkdir` and every `_execute_bridge`/query/CCID call
  occur only after the full immutable candidate measurement succeeds.
- Confirmed staged paths are untrusted locators: acceptance derives from exact
  signed hashes, signature/key identity, recomputed artifact/source/state
  roots, exact previous/active release state, canonical bytes, safe/distinct
  release IDs, and equality with independently measured source payloads.
- Confirmed stamp/state command provenance is frozen from the initially
  authenticated bytes and cannot be relabeled by a later locator change.
- Confirmed all post-operation stamp/state/query/CCID checks and the final full
  installed-candidate remeasurement remain present.
- Confirmed no tuple field, identifier, framing byte, semantic/artifact field
  count, payload binding, evidence tier, provenance rule, CCID input, or
  production claim was broadened or weakened.
- Confirmed compiled failure rejection occurs before its work root, drills, or
  receipt; the generation mutation runs before the happy activation and leaves
  its fresh Registry snapshot byte-identical.
- Confirmed the diff is limited to five implementation/test files plus this
  required report, with no private key material or production evidence.

## Concerns

- The compiled producer regressions were added after the Rust schema gate, so
  they do not have separate chronological REDs. The failure binary received a
  deliberate post-implementation mutation check; generation is exercised on
  a fresh pre-swap Registry. The Rust unit/table test is the required
  pre-implementation RED, and both compiled tests assert real side effects.
- This Windows host cannot execute the Linux-only RLIMIT/cache resource test;
  it remains the one expected skip.
- Existing unrelated Rust dead-code warnings remain in `ku-core`/`ku-kql`.
- No full-size production artifacts, production private signing key, or
  `BASE-GATE-V1` claim was created.

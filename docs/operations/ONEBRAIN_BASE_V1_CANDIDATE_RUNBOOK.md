# OneBrain Base v1 three-OS candidate runbook

This runbook operates Task 26. It creates prequalification or release-bound CI
evidence; only Task 27/28 can derive a qualified Base release.

## Reviewed action mapping

The workflow uses only local scripts and these reviewed immutable revisions.
The tag is documentation; the full commit is the executable authority.

| Action | Reviewed tag | Full commit SHA |
| --- | --- | --- |
| `actions/checkout` | `v5` | `fbc6f3992d24b796d5a048ff273f7fcc4a7b6c09` |
| `actions/setup-python` | `v6` | `ece7cb06caefa5fff74198d8649806c4678c61a1` |
| `actions/setup-node` | `v5` | `a0853c24544627f65ddf259abe73b1d18a591444` |
| `dart-lang/setup-dart` | `v1` | `65eb853c7ba17dde3be364c3d2858773e7144260` |
| `actions/upload-artifact` | `v4` | `ea165f8d65b6e75b540449e92b4886f43607fa02` |
| `actions/download-artifact` | `v5` | `634f93cb2916e3fdff6788551b99b062d0335ce0` |

These values were resolved from the official Git repositories on 2026-08-11.
Changing a tag or SHA requires review of the upstream diff, an update to the
workflow allowlist and this table, and a fresh candidate run.

All three jobs use the pinned setup action to select Python 3.13. The only
third-party Python package installed by the workflow is `blake3==1.0.8`; pip
must use binary wheels, `--require-hashes`, and the closed six-wheel SHA-256
set embedded in the workflow. Adding a source distribution, version, platform,
or hash requires the same supply-chain review as changing an action revision.

## Modes

Before either mode, configure repository variable
`ONEBRAIN_BASE_V1_IDL_BASELINE_RECEIPT` with the immutable Task 14 receipt JSON.
It is public integrity metadata, not a secret. Each lane fetches the protected
`refs/heads/base-v1-idl-baseline` ref, writes the receipt only under
`RUNNER_TEMP`, and lets the generator verify ref commit/tree, `git show` bytes,
ancestor relation, IDL digest, and history-chain prefix. A missing or moved ref
fails all lanes.

Repository administration must publish the exact Task 14 commit as
`refs/heads/base-v1-idl-baseline` and protect it against force-push/deletion
before enabling this workflow. As of the local Task 26 implementation review,
the `origin` remote did not yet advertise that ref; this runbook does not grant
the implementation task authority to publish or protect it.

`prequalification` is used for pull requests, `main`, and manual diagnostic
runs. It derives a non-production request/session namespace from the checked
out commit and tree. Its receipt can never substitute for a signed Task 27
request.

`release` is manual only. Supply the run ID of a prior immutable artifact named
exactly `base-v1-signed-release-request`. The envelope must contain
`request.json`, `request.json.asc`, the exact request-bound
`approver-policy.json`, and `approver-public-key.gpg`. Importing that public key
grants no trust: the production verifier still requires the frozen full
fingerprint, exported-key digest, policy digest, and `VALIDSIG`. The workflow does not expose request
digest, qualification-session ID, candidate commit, candidate tree, semantic
digest, or target digests as inputs. The candidate-owned verifier validates the
signature/policy/tooling first and derives those values from the verified
bytes. The job creates a fresh isolated public-key verification home; no
signing private key or persistent GPG home is required by this workflow.

## Three-OS closure

Linux (`x86_64-unknown-linux-gnu`), Windows
(`x86_64-pc-windows-msvc`), and macOS (`aarch64-apple-darwin`) each run:

- format, full workspace check/clippy/tests, and the real network feature on
  Node/API/CLI;
- generated Base drift, vNext/mobile validators, locked TypeScript and Dart
  conformance;
- archive/recovery, Registry fixtures, P5 preflights, and the legacy-disabled
  packaging scan;
- raw Cargo/npm audits, exact default release CLI build, and deterministic
  target-bound SPDX generation.

All build, package-manager, and evidence outputs live under `RUNNER_TEMP`; the
checked-out candidate remains clean even when ignored files are requested from
Git status. Each lane uploads a create-new, non-overwriting artifact. The final
job downloads the raw files, recomputes hashes, requires one semantic digest,
requires three distinct target/toolchain artifact tuple digests, rechecks Git,
and emits `onebrain/base-v1-provenance-receipt/1`.

## Failure and rerun

Do not edit or replace a failed artifact. Preserve its run URL and raw logs,
fix the source/policy/triage on a new commit, create a new signed request for a
release attempt, and rerun. A missing OS lane, mixed request/session, dirty
tracked/untracked/ignored file, mutable action, copied artifact tuple, invalid
SBOM, changed executable, malformed audit, or untriaged P0/P1 is a hard stop.

After a successful run, record the request digest, session ID, candidate
commit/tree, three lane artifact IDs, provenance artifact ID, workflow SHA256,
and raw audit/SBOM/executable hashes. Task 27 must consume those exact retained
bytes rather than a summary copied into a new file.

## Task 26 implementation validation state

The Task 26 Python suites, real 674-package SPDX generation, mobile/vNext
validators, generated-contract drift check, YAML parse, and checksum-verified
`actionlint 1.7.12` pass locally. The exact required Clippy command remains a
stop-the-line candidate blocker on Rust/Clippy 1.96: it reports 97 pre-existing
`ku-core` diagnostics in source/test/benchmark/governance code outside the
Task 26 file set. This workflow intentionally retains `-D warnings`; do not
weaken, skip, or relabel that gate. No three-OS run may be treated as candidate
evidence until those diagnostics are fixed in an owner-approved source task
and the entire workflow is rerun.

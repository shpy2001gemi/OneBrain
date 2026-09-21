# KU-CLI-001 implementation and evidence

2026-09-20 — Review, local and uncommitted. Branch
`codex/ku-cli-001-workflow` starts at `467e2ad3ada305597b2d1808c03da6190d0643cb`
with the complete modified/untracked Web/semantic foundation retained under
D-024. HEAD alone is not this implementation's baseline.

`onebrain ku` now projects all eleven registered KU operations plus the existing
reservation transport through the authenticated local API. The adapter uses
generated DTO validation for bounded payload files and responses, preserves
typed IDs and opaque continuations, refreshes generation fences before POST,
and never automatically retries preparation/save or generates replacement
idempotency identities. The node remains the sole service/storage owner.

Save is an explicit private command. Cancellation requires exact typed operation
confirmation without a `--yes` bypass. Output retains the entire bounded JSON
envelope, including conflict/partial state, limitations and Base retry/reconcile
policy. Transport loss/malformed or oversized replies require reconciliation.
Loopback-only destinations, disabled proxy/redirects, bounded reads and generic
transport diagnostics protect local credentials and private payloads.

See [CLI reference](../../../../src/onebrain-cli/CLI_REFERENCE.md),
[adapter specification](../../../specs/vnext/KU_LOCAL_CLI_PROJECTION_V1.md), and
[implementation](../../../../src/onebrain-cli/src/cli/ku.rs).

## Verification

Cargo commands ran from `src` on Windows; Python from repository root.

| Check | Result |
|---|---|
| `cargo check --locked -q -p onebrain-cli` | Pass. |
| `cargo test --locked -q -p onebrain-cli` | 42 unit tests and 2 integration tests pass, including 5 new KU tests. |
| `cargo check --locked -q -p onebrain-cli --no-default-features` | Pass with the existing dependency feature configuration. |
| `cargo fmt -p onebrain-cli -- --check` | Pass. |
| `python scripts/ci/validate_vnext_contracts.py` | Pass; existing frozen P3.3 command inventory retains its meaning. KU adapter coverage is executable Rust tests. |
| `git diff --check` | Pass. |

The real API-backed test uses an isolated temporary node, encrypted Vault and
test-signed one-concept Registry. It runs parsed CLI commands through actual HTTP
for reserve, prepare, exact replay, preview, private save/replay, status,
reconcile, exact get, list pagination, search, revision, typed cancellation and
public export refusal. Two artifact outputs are retained; prepare replay calls
the input provider only once. A continuation reused with a changed query fails
as conflict. Save remains unpublished and grants no reward. Cancellation tests
inject stdin text at the prompt boundary, including rejected `yes`; no command
line bypass exists. Transport fixtures prove bounded response reads, no redirect
or retry, credential-reflection suppression and visible unknown-outcome policy.

## Limits

This is the contracted advanced CLI surface, not a new source editor/intake or
semantic draft-to-canonical conversion. Payload files require existing
host-admitted references, commitments and custody. Full JSON output intentionally
preserves all artifacts and scope rather than hiding details in a summary.
Encrypted export only projects the separate Base-management requirement; it
does not provision sink/secret capabilities or claim archive completion.

No model experiments, qualification, live-host changes, mobile work, default
network activation, commit, push or merge occurred. Existing dependency
dead-code warnings remain. Broader cross-surface and restart/crash qualification
stays with the shared runtime/API evidence and KU-QA-001.

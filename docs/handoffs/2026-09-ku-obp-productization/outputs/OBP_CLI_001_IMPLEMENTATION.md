# OBP-CLI-001 — Local CLI implementation evidence

2026-09-22. Original workspace `C:/Users/shpy2/Documents/OneBrain`, branch
`codex/obp-cli-001-networking`, clean starting main `b66780f`. API `de04a57`
remains unchanged in merge `7d37a30` under D-030. D-029 approval is satisfied.
These CLI changes are retained locally, uncommitted; no push or merge occurred.

## Implementation

[CLI projection](../../../specs/vnext/OBP_LOCAL_CLI_PROJECTION_V1.md) registered
the exact syntax before implementation. `onebrain obp` maps all 13 accepted
operations plus metadata reconciliation through authenticated local REST.
The additive `vnext-outbound-first` build feature reuses the existing node DTO
contract; defaults remain unchanged. The adapter constructs no node/runtime,
grant, source resolver, socket carrier, signer, retry scheduler or outbox.

The client accepts numeric loopback HTTP(S) origins, disables proxies and
redirects and transport retries, bounds connect/response time and JSON bytes, and rejects duplicate,
unknown, null-optional and malformed DTO fields through the shared validator.
Both request and returned state/identifier/authority projections are checked.
Command results must match the original key/operation and exact target. False
connected/acknowledged claims fail closed against existing contract rules.
Recorded failed_no_effect reasons match the accepted transport inventory.

Status output is saved as the explicit context file. Commands and queries reuse
its exact process/dataset fences; mutation payload files retain the caller's
original idempotency key and expected network generation. No auto-fetch replaces
context and no failure triggers replay, new keys, new generation or page restart.
Reconcile returns only recorded metadata and checks the requested key.

Every mutation previews its exact operation/payload on stderr and requires the
exact key on stdin, with no --yes bypass. Management credentials are supplied
through `ONEBRAIN_OBP_MANAGEMENT_TOKEN` and sent only on the five management
commands. Ordinary Bearer auth uses the existing flag/environment convention.
Reflected credentials, including nested JSON keys, are suppressed. Successful
envelopes and valid API error envelopes retain JSON stdout; guidance uses stderr.
Exit 0 describes a valid successful API envelope, not acknowledged delivery.

## Verification

Executed with local temporary stores and loopback listeners, offline Rust
dependencies. No OneBrainLocal host, production Registry, jobs, keys or Ollama
was accessed or changed.

```powershell
cargo test --offline --locked --manifest-path src/Cargo.toml -p onebrain-cli --features vnext-outbound-first -q
cargo test --offline --locked --manifest-path src/Cargo.toml -p onebrain-cli -q
cargo check --offline --locked --manifest-path src/Cargo.toml -p onebrain-cli --no-default-features -q
cargo fmt --manifest-path src/Cargo.toml -p onebrain-cli -- --check
python -m unittest scripts.ci.test_validate_obp_local_api scripts.ci.test_validate_obp_product_contract scripts.ci.test_validate_vnext_cli_profile
python scripts/ci/validate_vnext_contracts.py
git diff --check
```

- Default CLI: 42 unit + two integration tests passed.
- Feature-enabled CLI: 51 unit + two integration tests passed, including eight
  new OBP tests and existing KU/vNext regressions.
- No-default-features check and formatting passed. Existing ku-net dead-code
  warnings remain; no unrelated warning cleanup was performed.
- Python OBP product/API and frozen CLI contract tests: 25 passed.
- Aggregate vNext validator and whitespace checks passed.

The eight OBP tests cover all command/DTO mappings against the accepted inventory,
invalid syntax and unsupported operations, bounded strict input/context parsing,
local-origin and credential controls, existing route/intent and operation outcome
fixtures, all 13 error/reconciliation policies, malformed/redirect/oversized/lost
responses, and lost-command-response no-replay behavior.

The real HTTP integration runs the actual ApiServer router and OneBrainNode with
isolated deterministic custody fixtures. It verifies status and empty source/
reservation views, disabled/missing dependencies, configure(false,false), wrong
and revoked management grants, rejected typed confirmation with no generation
change, durable kill and exact replay, metadata-only reconcile, conflicting key
reuse, stale generation, wrong dataset context, and explicit re-enable preserving
requested=false. The fixture shuts down the server and node. Route/source/intent
operations without host bindings are checked as failures, not successful traffic.

Development corrections: the first compile found a missing closing brace; the
first test compile used an incorrect shutdown method name. Both were corrected.
The failed_no_effect decoder was aligned with the accepted six-field failure
fixture before tests passed. No API or node implementation changes were needed.

## Run and scope limits

See [CLI reference](../../../../src/onebrain-cli/CLI_REFERENCE.md#local-obp-workflow-obp-cli-001)
for build and command examples. An operator-provisioned host must install its
OBP service, principal/control binding, management capability and typed discovery
inputs. The ordinary CLI start path has not been expanded into a provisioning
tool, and the running local host has not been activated for this feature.

`source-admit` consumes an existing host-issued input_ref. It does not implement
file/QR/raw invitation import. Management grants expire within the existing host
limit and cannot be minted by CLI. Missing intake or capabilities fail explicitly;
there is no REST upload or address fallback. Advertising's public-fields/expiry
preview remains the trusted host's prerequisite before granting that authority.

The accepted API has no source-delete, intent-cancel, peer-directory or outbox-list
operation. Source disable preserves replay floors; exact route and intent queries
provide the registered observations. The task's generic removal/cancellation
wording does not allocate additional operations beyond the accepted API.

This evidence is CLI transport/contract/local integration coverage, not successful
multi-host discovery, NAT failover, public advertisement, live peer delivery,
cross-platform qualification or rollout acceptance. Those remain separate tasks.
Model tuning, mobile, Web implementation and default networking remain outside
scope. Existing API/node evidence is retained, not relabeled as a fresh full rerun.

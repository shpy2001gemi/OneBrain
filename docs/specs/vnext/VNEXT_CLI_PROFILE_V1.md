# OneBrain vNext - CLI Profile v1

> **Work package:** `DR-P3.3`
> **Status:** Executable product surface - complete 2026-07-26
> **Code:** `onebrain-cli::cli::vnext`
> **Machine profile:** `src/test-vectors/vnext/vnext-cli-profile-v1.json`

## 1. Boundary and command inventory

The additive CLI surface is:

| Group | Commands |
|---|---|
| Private Need | `need prepare`, `need activate`, `need list`, `need scan`, `need matches`, `need retire` |
| Public Use / PoMV | `pomv use prepare`, `pomv use confirm`, `pomv use status`, `pomv view` |
| Runtime | `vnext status` |

The vNext CLI MUST use the authenticated
`VNEXT_PRODUCT_INTEGRATION_PROFILE_V1` REST contract rather than reinterpret
legacy REPL `kql`, PoMV scalar, or `status` output.

The client MUST send a Bearer token supplied explicitly by `--api-token` or
the `ONEBRAIN_API_TOKEN` environment variable and MUST NOT print the token in
normal output or errors.

Typed CIDs and local identifiers MUST remain 32-byte lowercase hexadecimal,
and continuations MUST remain opaque `obc1` values.

## 2. Private Need behavior

`need prepare` MUST send raw KQL only to the authenticated local-private
prepare endpoint and MUST state that preparation does not activate a Need.

Every Need scan MUST remain one-hop and bounded by the product budget.

An empty Need or match page MUST be described as a local or bounded partial
result and MUST NOT be described as absence from the network.

Every displayed match MUST remain labelled `quarantined proposal` with
`executable=false`; CLI retrieval MUST NOT materialize, adopt, or authorize it.

Prepare, activate, scan, and retire retry MUST preserve the REST
idempotency identity rather than synthesize a new local identity.

## 3. Public Use confirmation

`pomv use prepare` MUST require the explicit `--public-permanent`
acknowledgement and MUST display the canonical payload preview, exact target,
exact recipient, selector, namespace, disclosure, intent identity, and expiry.

Preparation MUST NOT create UseEvidence and MUST NOT expose the in-process
single-use core capability.

`pomv use confirm` MUST warn that publication is Public and permanent and MUST
require the operator to type the exact prepared `intent_cid`.

The CLI MUST NOT expose a `--yes` or equivalent non-interactive confirmation
bypass.

The interaction receipt MUST be derived only after exact typed confirmation,
MUST remain absent from displayed output, and MUST be submitted only to the
authenticated confirmation endpoint.

Exact confirmation replay MUST return the same publication identity.

Publication status MUST remain `pending` or `deferred` unless a separate
durable authenticated delivery acknowledgement exists; CLI output MUST NOT
infer delivery.

## 4. Feed signer startup

The CLI MUST name the selected Feed signer provider at startup.

Public Use startup MUST fail closed when no Feed signer provider was selected.

The `development-file` Feed signer MUST require the separate
`--allow-development-file-signer` opt-in and MUST display a non-production,
exportable-key warning.

Failure of the selected Feed signer MUST NOT fall back to another provider.

The development file adapter MUST NOT be represented as HSM, remote, OS
keystore, or other production custody.

## 5. Evidence view and status

`pomv view` MUST reject a response unless `establishes_truth`,
`establishes_benefit`, `authorizes_reward`, and
`claims_global_completion` are all literal `false`.

A conflict or unresolved evidence branch MUST NOT be displayed as
`Authorized`.

`vnext status` MUST preserve compiled, requested, active, kill-switch, signer
readiness, lifecycle, coverage, and limitations as independent fields.

## 6. Executable evidence

The default and feature-enabled CLI test suites prove:

- all eleven P3.3 commands parse;
- `--yes` is rejected and Public Use preparation requires
  `--public-permanent`;
- exact typed-intent confirmation and the REST receipt derivation stay frozen;
- unsafe match or evidence-view projections fail closed;
- zero-result and quarantine wording remain scope-honest;
- safe-default startup enables no vNext lane;
- the development Feed signer requires explicit provider selection plus
  explicit opt-in; and
- a real feature-enabled API/runtime round trip preserves Need and Public Use
  identities across exact replay without exporting a receipt.

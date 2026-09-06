# KU-RUN-001 implementation evidence

Date: 2026-09-06. Branch: `codex/ku-run-001-shared-service`.
Authority: KU-PC-A/B/C under D-015; prerequisite registration extension D-016.
Starting main: `91cc715b547b71941f6d66fea2093fc2326eb481`.

## Delivered boundary

- Registered `semantic-content/1` as a distinct typed digest. Six independent
  Python CBOR/BLAKE3 goldens cover negation, argument/statement order and source
  unit separation. Rust also checks alpha renaming, source-span removal, exact
  private provenance, disclosure/ObjectCID separation and a fresh process.
- Base IDL 1.2 registers eleven payload kinds and eighteen DTO kinds, appends
  29 history entries, and generates Rust/TypeScript/Dart projections. Existing
  discriminators retain their values. The development candidate is 1.2.0;
  qualification remains separate. Generated Rust rejects unknown fields,
  explicit nulls, malformed identities, unsupported minor versions, excess
  bounds and inconsistent retry/reconcile flags.
- `OneBrainNode::ku_services` exposes the authenticated weak Base service.
  Preparation, exact preview/save/get, snapshot list/search, revision,
  export reservation/public exchange, status, cancel and reconcile share this
  owner. Product handles carry no store, key, filesystem or signer capability.
- A signed pinned Registry reader validates every selected full CCID. Source
  custody is checked before resolution, staging and each accepted write.
  Unresolved concepts return no apparently ready output identities. All
  distinct output artifacts and private input/binder provenance are retained.
- The dataset VAULT owner has one KU PrivateVault and an encrypted journal.
  Canonical validation precedes acceptance. Complete commit markers gate
  product visibility; the Base operation journal records durable state and an
  opaque preparation pointer. Exact repeated saves share their receipt.
- Restart restores prepared bytes without encoding again; missing pinned
  release blocks an uncommitted save. Confirmed partial writes reconcile from
  exact staging. Committed reads survive unavailable Registry/encoder. An
  interrupted, unconfirmed preparation resolves to no-effect failure. Wrong
  Vault keys fail open with `CorruptState`.
- Generic Base commands cannot bypass KU confirmation or cancellation fences.
  Unknown outcomes cannot be canceled or blindly retried. Response admission
  precedes save confirmation and private archive reservation. Lost completion
  acknowledgements retain reconcile-required errors.
- Private list/search use a bounded index of accepted semantic Text values,
  sorted by full ObjectCID. Opaque continuations bind principal, dataset,
  query and immutable snapshot/frontier. Revision preserves predecessors and
  branches, excludes self-edges and rejects a changed frontier at save.
- Public exact reads use the existing shared validated sink. Public exchange
  rejects private, missing, unsupported-reference and opaque dependencies;
  opaque objects remain exactly readable and non-executable. Private export
  delegates a reservation to existing Base CreateArchive management.

## Verification

Cargo commands below run from `src/` with `--locked`; no live network or model
is required. The final branch ledger records the pushed content checkpoint.

| Command | Result / exercised boundary |
|---|---|
| `cargo test --locked -q -p onebrain-node --lib` | 117 tests; 13 KU cases including a child-process helper, authenticated node integration, concurrent confirmations, revoked custody, budgets, unresolved input, revisions, pagination, public/private firewall and lifecycle. |
| KU process-kill case in that suite | Six real child exits at before objects, after each of three object writes, before commit marker and after commit marker. No partial list visibility; reconcile without encoder calls. Actual journal/object files are read and checked for the fixture's private plaintext. |
| `cargo test --locked -q -p ku-core --lib foundation` | 196 tests, including local metadata authentication/purpose separation, existing canonical and private accepted/quarantine conformance. |
| `cargo test --locked -q -p ku-core --test semantic_content_conformance` | Two tests plus a fresh child-process golden run. |
| `cargo test --locked -q -p onebrain-base-contract` | 21 tests across unit, compatibility, generated drift/projection and KU corpus suites. |
| `cargo test --locked -q -p onebrain-node --test base_runtime_facade --test canonical_exchange --test durable_data_recovery --test p0_capability_truth --test vnext_index_parity` | 9 + 5 + 4 + 1 + 3 tests. |
| `cargo check --workspace --locked -q` | Whole workspace compile gate. |
| `cargo fmt --all -- --check` | Whole workspace formatting gate. |
| `python scripts/base/generate_contract.py --check` | Generated projections match the registered IDL and immutable Task 14 baseline receipt. |
| `python scripts/ci/validate_vnext_contracts.py` | Global contract gate, including 22 foundation domains and unchanged 27 Base operations. |
| `python -m unittest scripts.ci.test_validate_ku_product_contract scripts.ci.test_validate_ku_registration scripts.ci.test_validate_base_v1_runtime_interface scripts.base.test_generate_contract` | 85 tests, including registration and golden-binding mutation rejection. |
| `npm test` in `src/onebrain-base-contract/conformance/typescript` | Typecheck, build and existing projection conformance. |
| `dart test` in `src/onebrain-base-contract/conformance/dart` | Two existing shared-contract tests outside Flutter. |

The TypeScript/Dart runs verify generated declarations compile and retain the
existing corpus. Rust runs the eleven KU DTO fixtures and all eleven operation
round trips. No claim of new KU client codecs or end-to-end UI conformance.

## Explicit limits and integration requirements

- KU installation remains host-configured and disabled when `BaseRuntimeConfig.ku`
  is absent. The host supplies a Vault key, authenticated source/draft intake,
  local input provider and optional signed Registry/public-reader ports.
  Controlled resolved-draft and local-rule fixture outputs prove convergence;
  no production natural-language parser, live Ollama, automatic Registry
  download or remote encoder is qualified by these tests.
- Source intake remains host-only. It must admit bounded opaque references and
  enforce current consent/custody. The shared service independently validates
  the returned source objects and semantic output; clients cannot install
  providers or supply authorization booleans.
- Index coverage is the principal's committed private KU store. Search uses
  `index_version_ku_text_1`; frontier is the local revision journal, not a
  replicated Assembly supersession claim. Snapshot capacity is 32, cursor
  capacity 1,024, and each snapshot is capped at 4,096 summaries.
- Journal capacity is 1,024 preparations and 64 MiB ciphertext, with 8 MiB per
  encrypted metadata record. Exhaustion returns a typed resource error.
  Cancellation retains encrypted evidence; automatic retention/GC is absent.
  Restoring a different dataset requires reopening the host KU runtime with
  its appropriate key and ports; old handles cannot cross generations.
- Private archive export returns a management reservation, never archive
  completion or a receipt proving selected KU rows were included. Host archive
  source coverage and portable KU provenance/revision restore are not
  qualified here. With no configured archive service it fails explicitly.
  Existing encrypted archive capabilities and management checks remain intact.
- Public export requires known object dependency semantics; unsupported
  closure returns `DependencyUnavailable`. No private plaintext substitution.
- Existing legacy read/export paths retain their byte families. No legacy
  mutation adapter, migration, REST/CLI/UI, mobile implementation, OBP rollout,
  new WS event, reward issuance, domain DNS or deployment change is included.
  `onebrain.live` remains infrastructure context for later scoped work.

Current task remains KU-RUN-001 pending owner review. No merge or branch deletion.

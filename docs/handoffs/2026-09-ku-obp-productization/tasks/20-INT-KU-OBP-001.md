# INT-KU-OBP-001 — Two-node concept and contributor entry

> State: Merged under D-046, 2026-10-02; actual Git state: [PROGRESS](../PROGRESS.md).
> Parent scope: [MASTER_PLAN](../MASTER_PLAN.md), owner decision D-044 (2026-09-28).
> Depends on: working KU-QA-001 local MVP slice and accepted OBP-QA-001 functional work.
> Local integration can use the available task-09 implementation before merge;
> record that dependency. Strict ENC-003/NAT/platform qualification is not required.

## Objective

Demonstrate one supported KU moving from local creation to explicit exchange and
receiver inspection through the existing shared framework. Make the same concept
approachable for open-source contributors. Two real processes on one host are an
acceptable initial topology; neither mocked delivery nor ordinary consumer-NAT
qualification is implied.

## Focused reading

- MASTER_PLAN, current PROGRESS checkpoint and D-044;
- the task-09 outcome and existing OBP functional acceptance;
- [additive workflow surface](../../../specs/vnext/ADDITIVE_KU_WORKFLOW_SURFACE_V1.md)
  plus publication/consent/reconciliation profiles for interfaces actually changed;
- existing host/demo/harness code needed to compose the journey.

## Substeps — keep all work under this parent

- [x] A. Compose the smallest repeatable two-node setup from existing runtime,
  API/CLI/Web and authenticated transport. Record topology and explicit opt-in;
  preserve unrelated live hosts and durable state. Use a supported manual/resolved
  KU, or output genuinely accepted by the existing experimental pipeline.
- [x] B. Run local preview/save → prepare/confirm sharing → durable outbound intent
  → peer reconciliation → receiver validation/read. Show matching canonical IDs,
  source/provenance scope and a useful status. Run one restart or idempotent retry;
  check private source data is not included in the public exchange.
- [x] C. Update the existing run instructions and root CONTRIBUTING/README entry
  links so a newcomer knows prerequisites, commands, sample input, expected outcome,
  known limits and one bounded contribution to pick. Keep secrets and private data
  out of examples. Reuse an existing launcher/example or add one small helper only
  if setup demonstrably needs it; do not build a new installer/platform matrix.
- [x] D. Record actual result and remaining gaps in PROGRESS/the existing parent
  output. Keep improvement tasks in MASTER_PLAN's contributor backlog. A short demo
  recording is useful when available but is not another acceptance gate.

## MVP acceptance

The local-to-peer happy path actually runs on the declared topology; the receiver
can inspect the accepted object and the same ID. Explicit save/share remain separate;
a model proposal or transport acknowledgement does not grant knowledge authority.
The setup/run entry is usable without undocumented maintainer-only paths; any
Registry/host provisioning prerequisite is stated honestly. Focused functional
checks and a short result note suffice.

Reuse shared-service tests for surfaces outside the primary demonstration. Do not
require every native UI, independent consumer networks, two model families, formal
reviewer records, signed bundles, exhaustive fault injection or a production release
to close this concept task. Do not mark the contributor setup substep complete
merely because historical build commands exist.

## Deferred / excluded

Broader cross-surface/NAT/platform tests and product packaging stay in the central
backlog. M6 multipath queries, Outcome/Benefit, OBT economics, mobile/browser,
mailbox wake and default rollout remain later lanes. `model_qualified=false` and
`consumer_nat_qualified=false` are not changed by a successful concept demo.

## A — integration seam and proposed disclosure boundary (2026-10-01)

The existing runtime can run two independent loopback QUIC processes with durable
NodeIDs and authenticated expected-peer checks. The local Web host has no network
configuration; the P5 agent is a qualification harness, not a local KU host.
Use one small opt-in example with separate new data directories; do not alter
`OneBrainLocal/host.json`, its source catalog, keys, Registry or dataset.

Actual interface gap: private save creates a `LOCAL_ONLY` semantic-kernel object.
`vnext_outbox::validate_intent` admits only Public/RouteMinimal, and KU public
export admits only already-public objects. `PUBLIC_USE_CONSENT_PROFILE_V1`
authorizes Public **UseEvidence**, not publication of a semantic kernel.
[KU workflow §2](../../../specs/vnext/KU_PRODUCT_WORKFLOW_PROFILE_V1.md#2-ku-identity-and-supported-semantic-normalization)
requires: “Publication later needs its own reviewed provenance/disclosure contract”.
D-044 does not change runtime/disclosure authority. Therefore no private object
will be relabelled or enqueued to bypass this boundary.

**Owner-approved bounded experiment, 2026-10-01 (D-045):**

1. Read one explicitly selected, authorized saved manual/resolved semantic kernel
   through the existing KU service. Admit only the known normalization profile,
   with no source spans, private references or unsupported extensions.
2. Prepare a **new Public object** from the exact normalized semantic root. Its
   ObjectCID differs from the private object's CID because disclosure is part of
   the envelope; sender and receiver must match this new Public CID and bytes.
   Preserve the old private object/receipt. The semantic root/comparison identity
   is intentionally disclosed only by confirmation; never log the private CID.
3. Display the exact Public preview, full recipient NodeID, selector/namespace,
   public/permanent consequence and provenance limitation. A separate explicit
   confirmation binds these exact fields, expiry (at most 15 minutes) and durable
   idempotency identity. Save, read, prepare or restart cannot confirm sharing.
4. Only confirmation may create the existing durable outbound intent. Use the
   existing expected-peer authentication, reconciliation and validate-then-accept
   storage; retry/restart keeps the same public bytes/CID and intent.
5. Receiver inspection reports canonical storage validity and authenticated
   sender/selector provenance only. Raw SourceArtifact, governance documents,
   private preparation journal, private provenance and private IDs stay local.
   The receiver cannot verify the original source or infer an author/fidelity,
   adoption, Use, truth or reward grant. No new feed/event/wire schema is allocated.

The owner explicitly approved this boundary on 2026-10-01. A successful
handshake alone does not complete A/B or the parent MVP acceptance.

### Repeatable topology check (KU sharing is unavailable)

Requires Rust/Cargo and a writable private local directory. This check does not
need a Registry, model or an existing KU dataset; the full local-to-peer journey
still needs the [manual host prerequisites](../outputs/KU_WEB_001_IMPLEMENTATION.md).
Build from the repository root:

```powershell
cargo build --locked --manifest-path src/Cargo.toml -p onebrain-node --example ku_peer_demo --features vnext-outbound-first
$demoRoot = Join-Path $env:LOCALAPPDATA 'OneBrainPeerDemo'
New-Item -ItemType Directory -Force -Path $demoRoot | Out-Null
@{ data_dir = (Join-Path $demoRoot 'left'); bind_addr = '127.0.0.1:0'; network_opt_in = $true } | ConvertTo-Json | Set-Content -Encoding utf8NoBOM (Join-Path $demoRoot 'left.json')
@{ data_dir = (Join-Path $demoRoot 'right'); bind_addr = '127.0.0.1:0'; network_opt_in = $true } | ConvertTo-Json | Set-Content -Encoding utf8NoBOM (Join-Path $demoRoot 'right.json')
```

Use PowerShell 7 (`utf8NoBOM`). In two terminals at the repository root, run one
command in each (these processes own only their respective demo directories):

```powershell
.\src\target\debug\examples\ku_peer_demo.exe "$env:LOCALAPPDATA\OneBrainPeerDemo\right.json"
.\src\target\debug\examples\ku_peer_demo.exe "$env:LOCALAPPDATA\OneBrainPeerDemo\left.json"
```

The first JSON line shows the full `node_id` and actual `listen_addr`. In the
left terminal, replace BOTH placeholders with the right process's values, then
type this single JSON line followed by Enter:

```json
{"command":"connect","expected_node_id":"<right node_id: 64 hex characters>","address":"<right listen_addr>"}
```

Expected: `event=connected`, `authenticated=true`, `payload_sent=false`. Type
`{"command":"status"}` to inspect the route; `accepted_records` remains zero,
`ku_share_host_configured=false`. This is configuration presence, not a host
readiness assertion. A wrong full NodeID returns
`peer_authentication_failed` before route authority or payload transfer.

Type `{"command":"shutdown"}` in each terminal to stop cleanly. Re-run using
the same config: each NodeID persists, while an ephemeral port may change. Copy
the new address before reconnecting. Do not use the existing `OneBrainLocal`
dataset as a demo directory. This checks loopback authentication only; no
reconciliation delivery, source provenance, public KU, NAT or production claim.

### Run the local-to-peer manual concept (D-045)

This is an operator demo with **three processes**: the existing local Web/KU API
custody host, plus the isolated left and right network runtimes above. The left
runtime uses an authenticated read of the custody host, not a second Vault writer.
The right runtime needs no source, Registry, model, API token or Vault key. Node
identity, canonical bytes, outbox and reconciliation remain owned by existing
node services. The normal Web/CLI/API do not yet expose KU share operations.

1. Follow the linked manual-host instructions: obtain a complete signed activated
   Registry and its trusted public key, provide your own binary Vault key and API
   token, and provision your permitted source with `ku_manual_source`. An unsigned
   OBR and qualification/test fixtures do not supply Registry trust. There is no
   bundled Registry or automated fresh-clone provisioning; operator input remains
   an explicit prerequisite. Do not reuse another maintainer's private files.
2. Start that host at its configured loopback port (4280 in the example), open
   `/ku`, resolve/select the intended full Registry CCID, and manually preview
   `Water is a liquid.` as one predicate + one text argument. Inspect the exact
   preview, then deliberately **Save exact preview privately**. Copy the saved
   ObjectCID locally. An unresolved proposal cannot be used here. Reusing a
   previously accepted supported manual KU is also allowed; no model rerun is
   needed. Remember that selecting `water` plus a text literal is the finite
   manual form, not automatic encoding of the sentence's full meaning.
3. Stop the left peer helper before changing its config. Add `local_ku_host` to
   its operator JSON, retaining its same `data_dir`, then restart it:

```json
{
  "data_dir": "<same absolute left demo data directory>",
  "bind_addr": "127.0.0.1:0",
  "network_opt_in": true,
  "local_ku_host": {
    "port": 4280,
    "api_token_file": "C:/OneBrainLocal/secrets/api-token.txt"
  }
}
```

The token file is read only by the left process; never paste the token into a
command, preview, peer config or committed file. The stdin command interface is
trusted local operator control, not an exposed unauthenticated API. The file
signer used by these loopback runtimes is the existing development path; this
demo does not supply production key custody.

4. Start the right helper and note its full NodeID/listen address. In the left
   helper, replace the three placeholders below and send this JSON line. The
   selector/namespace constants below define only this synthetic demo scope;
   they do not represent private namespace text or grant authority.

```json
{"command":"prepare_share","object_cid":"<saved private ObjectCID>","expected_node_id":"<right full node_id>","address":"<right listen_addr>","selector":"2222222222222222222222222222222222222222222222222222222222222222","namespace":"3333333333333333333333333333333333333333333333333333333333333333"}
```

Expected `event=share_prepared`: review the human-readable predicate/text,
exact `canonical_base64`, new `public_object_cid`, `semantic_content_cid`, full
recipient, scope, expiry and public/permanent consequence. The Public ObjectCID
differs from the saved private ObjectCID; the semantic comparison identity is
equal and becomes intentionally disclosed. Nothing is enqueued or sent by this
step. Only one predicate + one text literal with no dependencies/qualifiers/
extensions is supported. Advanced roots return `unsupported_public_ku` rather
than stripping potentially private or meaning-bearing content.

5. If you choose to share that exact preview, replace the placeholder with its
   `prepared_id` and type a **separate** confirmation line:

```json
{"command":"confirm_share","prepared_id":"<fresh prepared_id>","confirm_public":true}
```

This rechecks authorized access to the exact saved view, consumes the preparation
once, and creates the existing durable outbox intent. `confirm_public=false`
is rejected. Confirmation means pending durable intent; it is not delivery.
An expired preparation (15 minutes), inaccessible saved object or unavailable
host fails before enqueue. At most 16 preparations exist in process memory;
restart discards them and requires a fresh prepare and explicit confirmation.

6. In the left helper, inspect the returned `intent_id`:

```json
{"command":"intent","intent_id":"<intent_id>"}
```

Wait for `state=Acknowledged`; failed/pending means delivery has not completed.
On the right, inspect the prepared **Public** CID under the same selector:

```json
{"command":"inspect","object_cid":"<public_object_cid>","selector":"2222222222222222222222222222222222222222222222222222222222222222"}
```

Expected `event=public_object`, identical canonical Base64/Public CID/semantic
identity and readable predicate/text, plus the authenticated left NodeID in
`authenticated_source_peers`. That is transport/selector provenance. The raw
SourceArtifact, governance documents, private journal, private ObjectCID and
source-origin proof are not included. Source/fidelity/author/adoption/truth/Use/
reward are not established by a storage receipt. Keep this displayed limit.

7. Stop/restart both helpers using the same directories. Copy any new ephemeral
   address before new sends. Re-inspect the Public object on the right and the
   acknowledged intent on the left: IDs, bytes and provenance persist. A fresh
   prepare/confirm for the same object/recipient/selector/namespace reuses the
   same intent; `Existing`/`RouteUpdated` does not create another accepted object.
   Reopen the private KU in Web; its original bytes/receipt remain private.

Shutdown all demo helpers explicitly; stop only the local custody host you
started for this run. The same helper can run on other systems with the built
executable path adjusted, but only the Windows loopback topology has been tested.

### Actual local result — 2026-10-01

Windows at `689efd5` plus the retained task-09 work and uncommitted task-20 code:
the existing supported private manual KU from task 09 was read through the actual
authorized API, prepared and separately confirmed for the new Public object
`32f3b411f5944780d41cd482ad6262e1ee447181aa57ea2d78281ac74a9b9161`.
Its semantic comparison identity is
`03479711778482703b83dc009e2563fc72b82f723307c9854a5fd29996fe54ac`.
Two real isolated loopback network processes reconciled one public object; the
receiver matched exact bytes/CID and authenticated sender/selector provenance.

Opt-in rejection before store creation, wrong-peer rejection before route/payload,
no enqueue after prepare/false confirmation, exact authorized private-read
preservation, both-runtime restart, lost in-memory preparation and same-intent
retry passed. Original host config/keys/dataset were retained; the custody host
was started with a separate model-free config and all processes started for the
run were stopped. The existing Ollama listener was untouched. Public result and
isolated stores remain outside Git in the checkpoint's retained demo directory.

Known remaining gaps: Registry/secret provisioning is manual; share currently
uses this helper instead of a registered product API/Web action; richer semantic
roots, original-source/author/fidelity proof, normal product-host composition,
non-loopback/NAT/platform coverage and production custody are deferred. These
are contributor follow-ups in MASTER_PLAN, not qualification gained by this run.

Focused validation: `cargo build --locked --manifest-path src/Cargo.toml -p
onebrain-node --example ku_peer_demo --features vnext-outbound-first` PASS;
`cargo test --locked --manifest-path src/Cargo.toml -p onebrain-node --features
vnext-outbound-first --lib ku_public_share::tests` 3/3 PASS; the existing
`--test vnext_outbound_first_runtime expected_peer_identity_is_checked_before_route_authority_mutates`
1/1 PASS. KU/vNext validators, new-file rustfmt and whitespace PASS. Tests cover
new exact identity, source-reference/span/extension rejection, expiry/current
saved access and idempotent intent identity; the actual helper/API run covers
separate confirmation, durable delivery, receiver read and both-process restart.
Task 09's passing local Web/save/restart evidence was reused, not resampled.
Whole-workspace formatting/native/platform qualification was not rerun.

### Git review — 2026-10-02

MVP commit `8de723d` on `codex/ku-enc-003-handoff` contains the task-09 source
helper/Web label, shared Public KU module, peer helper/public read port and
MVP run/coordination documents. Review made no runtime change. The private saved
CID was removed from result documentation; local preparation reports are explicit
local pointers. All 265 local Markdown file links in the staged snapshot resolve
against the index, with no dependency on untracked qualification artifacts.
KU product/registration and vNext validators, focused new-file rustfmt and
whitespace pass. The prior scoped build/tests and actual delivery/restart result
above are reused. The accompanying ledger commit updates task/Git state only.

ENC-003 preparation/history, reports, scripts/tests, local overview and the older
qualification branch remain retained outside this slice. No push/merge or host
change was performed. Owner direction is needed for integration/publication under
D-010/D-044; local Review does not grant main acceptance or broader qualification.

### Owner acceptance and merge — 2026-10-02

D-046 accepts the scoped A–D operator concept. Reviewed commits `8de723d` and
`0b050a4` entered main through merge `8d064c4`, whose tree exactly matches the
reviewed tip; push was verified directly on origin/main. The post-merge vNext
validator and whitespace pass; prior scoped runtime results are reused. The
source branch, qualification branch and all local preparation are preserved.
The accompanying docs closure commit updates the existing ledger and overview.
No model/NAT/platform/production qualification or host change follows from this
acceptance. The earlier Git-review section records its historical local state.

## Contributor follow-up — first-run experience

Registry, config/token, Vault-key/source-input diagnostics and empty-catalog guidance: Merged and published under D-047, merge `cbb7d58` from `codex/ku-first-run-diagnostics` at `1c4c78a`, based on main `49e529f`;
this is the existing first-run backlog item in MASTER_PLAN, outside the completed
A–D operator demo. Its integration state is separate from D-046's accepted MVP.

- [x] Fix one concrete Registry startup failure: the shared custody helper returns
  `ku_registry_unavailable` with a read-only runtime, but this example's Required
  Registry node cannot start. Reject that result before dataset creation or node
  initialization, with a safe message naming the config fields, signed activation
  prerequisite, existing operator guide and data/key preservation on retry.
  Emit the retained-read warning only after the node's KU runtime is installed.
- [x] Extend the existing [manual-host instructions](../outputs/KU_WEB_001_IMPLEMENTATION.md#resolve-a-registry-startup-failure)
  with the correct Registry root layout, independent signer trust and retry steps.
- [x] Verify the focused example regression and actual executable's failure path.
- [x] Reproduce missing `api_token_file` returning only an OS error. Add bounded
  config/token loading diagnostics with field/path/format guidance, safe JSON
  line/column and no operator values. Retain config schema, token acceptance,
  file size limits and rejection before dataset/node initialization.
- [x] Extend the [operator guide](../outputs/KU_WEB_001_IMPLEMENTATION.md#resolve-config-or-api-token-setup-failures)
  and verify invalid config/token retries preserve existing custody inputs.
- [x] Reproduce Vault key read/length errors in `prepare_ku_runtime`. Add example-only
  guidance for unreadable, too-large and short key files, without rereading inputs or
  changing shared helper/Desktop codes, exact 32-byte acceptance or read-only fallback.
- [x] Extend the [Vault-key retry instructions](../outputs/KU_WEB_001_IMPLEMENTATION.md#resolve-vault-key-setup-failures)
  and check failure before dataset/Registry initialization, input suppression and
  unchanged saved/key/token bytes on retry.
- [x] Reproduce missing/invalid/oversized `sources[].canonical_file` with the
  existing synthetic signed activated Registry builder. Add example-only field,
  canonical format/label/duplicate-ID and retry guidance, including the 64-entry
  limit. Preserve the shared read-only fallback and reject partial admission.
- [x] Extend the [source-input operator guide](../outputs/KU_WEB_001_IMPLEMENTATION.md#resolve-source-input-setup-failures).
  Verify exact saved private bytes/receipt through the authenticated API after
  restart under all four fallback codes; deny wrong-token reads and editor access.
  Replace the blanket saved-read startup claim with conditional access guidance.
- [x] Reproduce omitted/empty `sources` with synthetic signed activated custody:
  both return a successful empty catalog without a fallback warning. Add Web
  guidance linking the existing explicit operator provisioning steps and disable
  only the empty manual form; retain saved inspection and separate AI consent.
  Extend the same regression to check exact private bytes/receipt after restart.
- [x] Review the cumulative five first-run slices against the parent scope and
  isolate code/guide/checklist/ledger changes for integration, preserving incoming
  ENC-003 preparation. Registry/secret provisioning and normal host composition
  remain broader contributor work; do not imply fresh-clone self-provisioning.
- [x] Integrate the reviewed `f2c0baf` + `1c4c78a` slice under explicit owner
  instruction D-047, push main and verify origin; preserve uncommitted ENC-003
  preparation, all untracked files and the qualification/source branches.
- [x] Follow-up local: reproduce the manual provisioning command's missing
  `text_file` OS-only error. Add source-validation/output-directory diagnostics
  with field/format/path-resolution guidance and safe custody retry instructions;
  preserve exact bytes, explicit consent, bounds and refusal to overwrite output.
- [x] Document [manual provisioning retries](../outputs/KU_WEB_001_IMPLEMENTATION.md#resolve-manual-provisioning-sourceoutput-failures)
  and verify source failure/no output, accepted 8192-byte private source, unchanged
  prior governance/canonical bytes, missing parent and explicit corrected retry.
  This additional slice is Review local, separate from D-047's merge;
  actual scoped commits are recorded in PROGRESS.
- [x] Follow-up local: reproduce serde diagnostics echoing a wrong-type consent
  value and an unknown request field. Add bounded request-file/JSON diagnostics
  with safe category guidance and numeric line/column; retain the closed schema,
  65536-byte bound and explicit consent gate before source access/output creation.
- [x] Extend the [request retry guide](../outputs/KU_WEB_001_IMPLEMENTATION.md#resolve-manual-provisioning-request-failures)
  and verify no value/path disclosure, no output on refusal, exact-size acceptance,
  deliberate successful retry and unchanged request/source/prior custody bytes.
  Both manual provisioning follow-ups are reviewed locally; actual scoped
  commits and integration state are recorded in PROGRESS.
- [x] Review both manual provisioning follow-ups against main `ac83f2a`, align
  code/guide/checklist/overview and isolate the local commits. Exclude task 23,
  all 15 untracked paths and its 108-line preparation history; retain their bytes.
  D-047 does not authorize merge/publication of these additional follow-ups.

Preserve Registry trust and private source/key custody. Product API/Web share
projection, richer roots and source/author/fidelity proof remain separate backlog
entries; do not reopen completed MVP acceptance or resume ENC-003 qualification.

Validation on Windows, 2026-10-02: `cargo test --locked --manifest-path src/Cargo.toml
-p onebrain-api --example ku_local_web` 1/1 PASS; `cargo build --locked --manifest-path
src/Cargo.toml -p onebrain-api --example ku_local_web_staging` PASS. The actual staging
executable exited 1 for missing and unsigned/unactivated Registry in an isolated
synthetic directory: no ready/read claim, no new dataset, exact saved/key/token/
Registry bytes retained on retry, no private values in diagnostics. KU/vNext
validators, scoped rustfmt and whitespace pass. The prior valid-host/save/share/
restart result is reused; no real host, private input, model, native or platform
campaign was run. This change is uncommitted/unpublished; broader first-run setup
remains open. Existing duplicate-example-target and ku-net dead-code warnings remain.

Config/token checkpoint, Windows 2026-10-02: the same example tests now pass 3/3,
including the retained Registry regression; staging build PASS. Thirteen actual
executable cases passed in a separate synthetic directory: missing/oversized/
malformed config, missing field, unknown field, wrong type, missing token with
new/existing dataset, short/non-UTF-8/quoted/oversized token, and a valid trimmed
token reaching the Registry check. All exited 1 without ready/read claims or
input-value disclosure; exact existing files were unchanged and no new dataset
was created on early failures. Tests also retain optional config defaults and
the 65536-byte config / 1024-byte token limits. KU product/registration and vNext
validators, scoped rustfmt and whitespace PASS. Reuse the prior valid-host/save/
share/restart evidence; no real custody files or live hosts were opened. Both
diagnostic slices remain local/uncommitted/unpublished; broader setup stays open.

Vault-key checkpoint, Windows 2026-10-02: missing/short/long keys reproduced the
shared helper's generic read/limit/invalid codes. The example now identifies
`vault_key_file`, exact binary length and recovery of the original key with the
same dataset. Example tests PASS 4/4, staging build PASS; fourteen actual executable
cases PASS for new/existing datasets, unreadable directory, empty/31/33-byte/text/
hex keys, and exact 32-byte non-UTF-8/whitespace acceptance reaching Registry checks.
All exited 1 without ready/read claims or private-value disclosure; file/directory
snapshots remained identical. No file is trimmed, padded, truncated or regenerated;
format acceptance does not establish that a key belongs to the saved dataset.
KU product/registration and vNext validators, scoped rustfmt and whitespace PASS.
Reuse prior valid-host/save/share/restart evidence; shared custody code, real inputs
and live hosts untouched. All three diagnostic slices remain local/uncommitted;
broader first-run provisioning/source diagnostics remain open.

Source-input checkpoint, Windows 2026-10-03: missing/invalid/oversized canonical
source files reproduced the helper's generic fallback codes. The opt-in example
now gives source-field/format/retry guidance for these and the 64-entry limit,
without rereading inputs or changing shared admission/Desktop behavior. Example
tests PASS 5/5; staging build PASS. The new regression uses the existing synthetic
signed activated Registry, actually saves one manual private KU, then restarts
the node/API under each of the four fallback codes. Authorized Get preserves
exact canonical bytes and LOCAL_ONLY disclosure; List retains one object and
Reconcile the original committed receipt. Wrong-token Get is denied, Catalog
fails even with a usable source before the failed entry, and source/key bytes
remain unchanged. This tests the actual router/service/storage, not a live browser
or external executable campaign. KU product/registration and vNext validators,
scoped rustfmt and whitespace PASS. Prior happy-path/share and executable
Registry/config/token/key checks are reused; private operator inputs/live hosts
were untouched. Four diagnostics slices remain local/uncommitted/unpublished;
empty-catalog onboarding and broader provisioning/composition remain open.

Empty-catalog checkpoint, Windows 2026-10-03: omitted `sources` and `sources: []`
with the synthetic signed activated Registry both return a successful empty
catalog, no runtime fallback. Web links the existing explicit operator setup,
disables only the empty manual form and restores it on an admitted-catalog refresh.
The same Rust regression PASS 5/5 now checks exact private bytes/LOCAL_ONLY and
original committed receipt after both restarts; wrong-token catalog access is
denied. Web component tests PASS 16/16 cover saved inspection, separate AI consent,
empty-to-admitted refresh and no automatic encoding/save/share. Web/staging builds,
KU product/registration, vNext, scoped rustfmt and whitespace PASS. Prior executable
and share/demo checks are reused; no real custody/host or browser/model campaign.
All five first-run slices remain local/uncommitted/unpublished, retained preparation
unchanged; integration review and broader provisioning/composition remain open.

### Cumulative first-run review — 2026-10-03

Reviewed the five slices against `49e529f` and the existing task-20 backlog.
Corrected Registry guidance to name the root containing `releases/` and activation
`state/`, and clarified that sources are required for manual creation but optional
for saved inspection. The example keeps the existing config/token/key format
acceptance, shared custody/admission, authenticated reads and explicit save/share.
The Web change gates only the empty manual form; AI retains its separate consent.

The review scope is six code/guide/task/overview paths plus a separate progress
ledger update. Incoming task-23 preparation, its retained history and all 15
untracked paths are excluded from the commits and retained in the working tree.
Actual commit IDs and integration state are in [PROGRESS](../PROGRESS.md).
Local commits are Review, not main acceptance; no push/merge is performed.
All 45 local Markdown links / 11 anchors in the three staged documents resolve
against the index; the Web guide URL also names an existing indexed file/anchor.

Focused recheck after the wording correction: example 5/5 and Web workflow 16/16
PASS; KU product/registration, vNext, scoped rustfmt and whitespace PASS.
Prior Web/staging builds, executable failure cases and real private-save/share/
restart results are reused. No private operator input, live host or model was
opened. Registry/secret provisioning and normal host composition remain open.

### Owner-directed first-run merge — 2026-10-03

D-047 authorizes integration/publication of `f2c0baf` + `1c4c78a`. Merge `cbb7d58`
has exactly the reviewed tip's tree, with parents `49e529f` and `1c4c78a`, and is
verified directly on origin/main. Post-merge vNext validator and whitespace PASS;
the earlier scoped tests/builds and real save/share/restart results are reused.

All 17 incoming dirty/untracked paths were restored byte-for-byte before the
closure edits. Task-23 preparation and 15 untracked files remain outside all
integration commits; its 108-line retained history also stays uncommitted.
Qualification `4a8f29d`, source branches and the clean detached worktree remain
unchanged. No private inputs, live hosts or models were opened. The five fixes
are Merged; broader provisioning/composition and product sharing remain open.

### Manual provisioning source/output diagnostics — 2026-10-03

On main `ac83f2a`, `cargo run --locked --manifest-path src/Cargo.toml -p
onebrain-api --example ku_manual_source -- <synthetic-request.json>` reproduced
an OS-only missing-file error without identifying `text_file`. The example now
names source read/UTF-8/size/empty errors and output-directory creation/existence
errors, including launch-working-directory resolution and custody-preserving
retry. Diagnostics omit operator values, private paths and source content.
Canonical encoding, consent, bounds, Registry trust and host admission are
unchanged. Output writes remain non-transactional; the guide states how to retain
an incomplete directory after a later write failure. Request-file/JSON diagnostics
remain a separate bounded first-run opportunity.

Windows focused checks: `cargo test --locked --manifest-path src/Cargo.toml -p
onebrain-api --example ku_manual_source` 3/3 PASS after correcting the byte-count
fixture. Covers consent, source rejection before output, exact 8192-byte UTF-8
LOCAL_ONLY output, source immutability, original governance/canonical retention
on repeat and missing-parent correction. KU product/registration and vNext
validators PASS. Executable checks and final format/whitespace results are
recorded in the current [PROGRESS](../PROGRESS.md) checkpoint. Prior host saved
read/restart/share results are reused; no host/model/private operator input was
opened. This follow-up is local, uncommitted/unpublished; the five D-047 fixes
remain Merged and broader provisioning/composition remains open.

### Manual provisioning request diagnostics — 2026-10-03

On main `ac83f2a` plus the retained source/output slice, the actual executable
echoed a synthetic wrong-type `consent_local_private` string and an unknown field
name through serde errors. Both failures exited before output creation. The new
`load_request` reports unreadable/oversized/invalid request codes, fixed field/type
or syntax guidance and numeric line/column without serde's operator-value detail.
The four-field closed schema, 65536-byte limit, path resolution, explicit consent,
source/output behavior and canonical encoding remain unchanged. The same operator
guide now explains request corrections; later writes remain non-transactional.

Windows focused checks: manual example tests 5/5 PASS (including the three retained
source/custody tests); build PASS. Fourteen actual executable calls PASS: ten
request refusals, two explicit private successes (ordinary and exactly 65536-byte
JSON, using launch-relative paths), and two existing-output refusals. Diagnostics
do not echo sentinels/paths; request/source bytes and both prior custody files stay
unchanged; failures create no output and success saves no KU. KU product/registration
and vNext validators, scoped rustfmt and whitespace PASS. Prior host saved-read/
restart/share checks are reused; no live host, private input, Registry or model was
opened. Both manual follow-ups remain local/uncommitted/unpublished, separate from
the five merged D-047 fixes. Broader provisioning/composition remains open.

### Cumulative manual provisioning review — 2026-10-03

Reviewed source/output and request diagnostics together against main `ac83f2a`.
The manual example retains the four-field closed request schema, 65536-byte
request / 8192-byte source bounds, consent before source access, exact UTF-8
source bytes and refusal to replace prior custody. Request parsing reports
fixed guidance and numeric positions; source/output creation errors identify
the relevant field and safe retry without operator values or private paths.
Canonical encoding, Registry trust, host admission and explicit save/share
remain unchanged. The guide agrees with these boundaries and explicitly states
that later writes still use their existing errors and are non-transactional.
The overview now includes the completed request slice; it previously described
request diagnostics as remaining work.

Local review isolates five code/guide/task/overview/prompt paths and a separate
PROGRESS ledger update. Incoming task-23 preparation, all 15 untracked paths and
108 lines of preparation history remain outside these commits and byte-identical
in the working tree. Actual commits and integration state are in [PROGRESS](../PROGRESS.md).
This is Review local; the five earlier first-run fixes remain Merged under D-047.
D-047 does not authorize merge/push of these new manual provisioning follow-ups.

Focused recheck: manual example Rust tests 5/5, KU product/registration and vNext
validators, scoped rustfmt and whitespace PASS. Prior build, nine source/output
and fourteen request executable checks, and host saved-read/restart/share checks
are reused because runtime code was unchanged during this review. Indexed local
Markdown links/anchors are checked before committing, without relying on excluded
untracked preparation artifacts. No private operator inputs, host, Registry or
model were opened. Later write failure handling, broader Registry/secret
provisioning, normal host composition and product share projection remain open.

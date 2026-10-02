# INT-KU-OBP-001 — Two-node concept and contributor entry

> State and actual working branch: [PROGRESS](../PROGRESS.md).
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

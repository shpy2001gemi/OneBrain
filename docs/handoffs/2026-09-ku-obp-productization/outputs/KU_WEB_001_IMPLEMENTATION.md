# KU-WEB-001 local manual workflow

This document records the original manual milestone. The owner-approved D-023
[Ollama addition and current run instructions](KU_WEB_001_OLLAMA_IMPLEMENTATION.md)
supersede the original no-inference/raw-intake limitations below.

The `/ku` page now projects the node-owned KU workflow: create a manual draft,
look up and explicitly select a Registry concept, preview/validate, explicitly
save privately, search/list, inspect exact accepted bytes and prepare a revision.
It has no dependency on OBP or a model. AI remains visibly unqualified.

## Delivered integration

- [Editor transport contract](../../../specs/vnext/KU_LOCAL_EDITOR_PROFILE_V1.md)
  was written before implementation. The additive authenticated POST
  `/api/vnext/ku/editor` exposes catalog/resolve/draft through the authenticated
  Base service and an optional host input port. Existing providers default to
  unavailable; no default host configuration or new network lane is enabled.
- `ManualKuInputs` validates operator-admitted canonical private Text sources,
  fences principal/access and reservation ownership, uses a signed Registry,
  checks explicit CCID choices and admits bounded volatile drafts. It constructs
  one predicate/text statement and a private whole-source provenance span.
  It does not extract or infer semantics from raw text. Missing selection
  yields `needs_resolution` with no saveable artifacts.
- The host returns the prepare template, including opaque draft/source refs and
  exact implementation/Registry commitments. The browser imports generated Base
  KU DTO types; it never hashes canonical objects, supplies governance, selects
  a fallback concept or grants storage authority. The normal KU service owns
  prepare/revise, validation, encrypted staging, exact save and recovery.
- The page is separate from the legacy `/drafts` and `/encode` meanings.
  Direct `/ku` navigation bypasses the unrelated onboarding wizard after the
  existing local authentication gate; the sidebar also links to the page.
- Private KU requests bypass the existing debug-logging client. IDs, search,
  draft text and canonical previews stay out of URLs, debug logs, browser
  persistence and WS. The API preserves no-store and typed Base error policy.
  Only the existing app's credential storage behavior is retained.
- Saved work stays visible if editor setup fails. Failed reads preserve the
  last displayed snapshot. Pagination retains opaque continuation/query context.
  Empty search is limited to the assessed local snapshot. Inspection retains
  disclosure, validity, fidelity limitations and exact canonical base64.
- Display and refresh never save. Lost mutation responses gate further changes
  on explicit reconciliation. The original operation ID remains visible.
  Changed host generations require reconciliation of pending work. The page
  never replays extraction automatically; recovery can retrieve a durable preview.

## Run the local MVP

This is an opt-in integration host, **not a self-provisioning installation**.
It requires operator-controlled custody and Registry inputs. There is no bundled
fake Registry, invented governance, source reference or production fixture.

Prepare these actual host inputs outside the repository:

1. A Registry root containing signed `releases/` and activation `state/`, and its
   independently trusted public key, using the existing Registry activation toolchain. The
   host uses `ConceptRegistryGenerationManager::open` to verify it. An unsigned
   `concepts.obr` or test fixture is not a substitute.
2. For manual creation, one to 64 canonical **LOCAL_ONLY Text SourceArtifacts** that the
   operator admits for this local principal. These are binary canonical object
   files from the trusted capture/custody producer, including actual governance
   references. They are not raw `.txt`, JSON views or Base64 text files. The
   host validates each object; each is bounded to 64 KiB and all to 4 MiB.
   Source acquisition and initial governance provisioning remain external host
   responsibilities. There is no browser raw-source upload or capture-policy UI.
   The explicit developer provisioning command below can supply a canonical
   source and real local consent records for the manual demo without a model.
   For saved inspection without manual creation, `sources` can be omitted or
   empty; see [manual source setup](#provision-a-developer-owned-manual-source).
3. A stable 32-byte **binary** Vault key file and an API token file with at least
   32 random ASCII letters/digits (hyphen/underscore also accepted). Supply these
   through the operator's local secret-management process; retain the same Vault
   key and dataset directory on restart. Keys are never generated from fixture
   constants or sent to the Web.

Create an operator-owned JSON config outside Git. Replace every placeholder
with your real host values. Paths should be absolute; relative paths resolve
against the launch working directory.

```json
{
  "data_dir": "C:/OneBrainLocal/dataset",
  "registry_root": "C:/OneBrainLocal/registry",
  "registry_public_key": "<trusted Registry public key: 64 lowercase hex characters>",
  "vault_key_file": "C:/OneBrainLocal/secrets/vault.key",
  "api_token_file": "C:/OneBrainLocal/secrets/api-token.txt",
  "sources": [
    { "label": "My admitted source", "canonical_file": "C:/OneBrainLocal/custody/source.canonical" }
  ],
  "web_dir": "C:/Users/shpy2/Documents/OneBrain/src/onebrain-web/dist",
  "port": 4280
}
```

From repository root, build and start (PowerShell):

```powershell
Push-Location src/onebrain-web
npm ci
npm run build
Pop-Location
Push-Location src
cargo run --locked -p onebrain-api --example ku_local_web -- C:/OneBrainLocal/host.json
Pop-Location
```

Open `http://127.0.0.1:4280/ku` and enter the host's API token. The example binds
loopback only and does not call the node's network startup. For a different
API port, rebuild with `VITE_API_BASE` pointing at that loopback port; the
existing Web client otherwise defaults to port 4280. This task does not deploy
a website or configure `onebrain.live`.

### Resolve config or API-token setup failures

The host reports these failures before creating a dataset or starting the API:

| Diagnostic | Correction |
|---|---|
| `ku_host_config_unreadable` | Check the config path passed on the command line and local read permissions. Relative config/input paths resolve against the launch working directory. |
| `ku_host_config_too_large` | Keep the JSON config at most 65536 bytes. Store token, key and canonical source bytes in the referenced private files. |
| `ku_host_config_invalid` | Use complete UTF-8 JSON. The message gives a line/column and either syntax guidance or the required field/type checklist. Use forward slashes or escaped backslashes in Windows paths; unknown fields are rejected. Compare the config with the example above, including nested `sources`/`ollama` fields if present. |
| `ku_api_token_unreadable` | Set `api_token_file` to an existing locally readable private token file. |
| `ku_api_token_too_large` | Keep that file at most 1024 bytes, including surrounding whitespace. Supply plain token text rather than JSON, a Vault key or a source file. |
| `ku_api_token_invalid` | Use a UTF-8 token with 32..1024 ASCII letters, digits, hyphen or underscore after trimming surrounding whitespace. Quotes, JSON and interior whitespace are invalid. |

Config/token diagnostics omit input values, private paths and token bytes. Inspect
the indicated config position locally; do not paste custody contents into reports.
Correct the setup and retry with the same `data_dir`, Vault key and source files.
The host does not generate or replace secrets on failure. Secret provisioning
remains a contributor follow-up.

### Resolve Vault-key setup failures

These failures also stop before Registry or dataset initialization:

| Diagnostic | Correction |
|---|---|
| `ku_vault_key_unreadable` | Set `vault_key_file` to an existing locally readable private key file. Relative paths resolve against the launch working directory. For an existing dataset, restore access to its original key. |
| `ku_vault_key_too_large` | The file exceeds 32 bytes. Supply the original 32-byte binary key; hex, Base64, JSON or token text are not decoded. Do not truncate the file to make it fit. |
| `ku_vault_key_invalid` | The file contains fewer than 32 bytes. Restore the original binary key through your local secret-management process; do not pad or generate a replacement key. |

Retain the same `data_dir` and original Vault key when retrying. No whitespace is
trimmed from key bytes, and exactly 32 bytes retains the existing acceptance
rule; this format check does not verify that a key belongs to an existing dataset.
Diagnostics omit private paths and key contents. This example adds guidance to
the shared helper's fatal errors; Desktop codes and read-only fallback stay intact.

### Resolve source-input setup failures

With a valid Registry and Vault-key file, these warnings mean the shared helper
installed its existing read-only fallback. The host can start after successful
node/runtime initialization, but source editing/encoding is unavailable for that
launch. One failed source disables the whole input catalog; the host does not
silently admit a partial list.

| Diagnostic | Correction |
|---|---|
| `ku_host_input_unavailable` | Check every `sources[].canonical_file` and local read permissions. Relative paths resolve against the launch working directory. |
| `ku_host_input_exceeds_limit` | Supply the intended complete canonical source within 65536 bytes. Do not truncate or alter canonical bytes to fit. |
| `ku_source_admission_failed` | Check `sources[].canonical_file` and `sources[].label`: each file must be a valid binary canonical **LOCAL_ONLY Text SourceArtifact**, each label 1..128 UTF-8 bytes, and source object IDs distinct. Raw text, JSON, Base64, KU objects and duplicate source IDs are not admitted. |
| `ku_host_source_limit` | Explicitly choose at most 64 operator-admitted source entries. |

Restore the intended source/custody files locally, or use the existing explicit
[manual provisioning command](#provision-a-developer-owned-manual-source) for text
you are permitted to capture. Keep the same `data_dir` and original Vault key,
then restart deliberately. Do not invent governance or treat file decoding as
source authorization. Diagnostics name config fields without displaying source
paths, labels or bytes; shared helper/Desktop codes remain unchanged.

Successful startup is not a saved-access check. Authenticate and use **Search / list**
and **Inspect** to verify your existing saved KU. With the original dataset/key,
the fallback retains saved reads; source-dependent preparation/save/recovery can
still fail. Do not replay encoding or save/share automatically when retrying.
The focused synthetic test verifies exact saved bytes and the committed receipt
after restart with missing, invalid and oversized sources or more than 64 entries,
plus wrong-token denial.
It does not verify arbitrary operator datasets or key ownership.

### Resolve a Registry startup failure

`ku_registry_unavailable: local KU host cannot start.` means the host could not
load and verify an activated signed Registry generation. Check these operator
config fields:

- `registry_root`: the Registry root containing `releases/` and activation
  `state/`, rather than an individual release directory or an unsigned OBR file.
- `registry_public_key`: the independently trusted signer public key, encoded
  as 64 lowercase hexadecimal characters. Keep that trust source independent
  of the downloaded package.

Use the existing [Registry operator commands](../../../specs/vnext/CONCEPT_REGISTRY_OPERATIONS_PROFILE_V1.md#4-operator-commands)
to verify the package and inspect or activate the intended trusted release.
This failure exits before creating the dataset or starting the API. Retry with
the same `data_dir`, Vault key and admitted source files. This example requires
a verified Registry even for saved reads; with a valid Registry, the existing
unavailable-model path can still serve saved private KU. Registry provisioning
remains an operator prerequisite.

### Provision a developer-owned manual source

Omitting `sources` or setting `"sources": []` is valid. With a verified Registry
and original Vault key, the host returns a successful empty manual catalog,
rather than a source-admission failure. The Web manual editor explains the
missing setup, links here and disables its form until a source is admitted.
Use **Search / list** and **Inspect** to check saved artifacts independently.
Experimental AI intake remains subject to its own model availability and
explicit source/retention consent; it does not require a manual catalog entry.

Follow the steps below deliberately, then restart the host and select **Refresh
host status** in Web. The catalog is frozen until host restart. Keep the same
dataset and original key. Provisioning/refresh does not replay encoding, save
or share; old source-dependent preparations may still require their original
admitted source. An empty catalog is not a promise that those operations work.

Use an existing private directory outside Git. Write a short UTF-8 text file
you own or are permitted to capture and an operator request, for example:

```json
{
  "operator": "Local developer",
  "text_file": "C:/OneBrainLocal/provisioning/manual-text.txt",
  "output_dir": "C:/OneBrainLocal/provisioning/manual-custody",
  "consent_local_private": true
}
```

`consent_local_private: true` explicitly permits manual local encoding and private
retention of this source, its governance and KU until operator removal. It does
not permit publication. The output directory must not exist; the existing private
parent supplies filesystem custody. The bounded command preserves exact source
bytes and refuses false/missing consent or existing output.

```powershell
Push-Location src
cargo run --locked -p onebrain-api --example ku_manual_source -- C:/OneBrainLocal/provisioning/manual-request.json
Pop-Location
```

Retain both `governance.json` and `source.canonical`. Add the latter explicitly
to `sources` in the trusted host config with your chosen display label, then
restart the host with the same dataset, key and signed Registry. This command
neither calls a model nor saves/adopts a KU. It is operator provisioning under
the [manual editor contract](../../../specs/vnext/KU_LOCAL_EDITOR_PROFILE_V1.md),
not automatic source admission. Custody files contain plaintext private source
material; keep the whole directory private and outside Git.

#### Resolve manual provisioning request failures

The operator request must be a UTF-8 JSON object at most 65536 bytes with all four
fields from the example above. Unknown and duplicate fields are rejected. Request
loading/parsing failures exit before reading source text or creating output:

| Diagnostic | Operator correction |
|---|---|
| `ku_manual_request_unreadable` | Check the request path passed on the command line and local read permissions. Relative paths resolve against the launch working directory. Keep the request outside Git. |
| `ku_manual_request_too_large` | Keep the request within 65536 bytes, including whitespace. Store source text in `text_file`; the request contains its path. |
| `ku_manual_request_invalid` | Use complete UTF-8 JSON with `operator`, `text_file` and `output_dir` as strings and `consent_local_private` as a boolean. Use forward slashes or escaped Windows backslashes. Inspect the reported line/column locally for syntax, missing/unknown/duplicate fields or wrong types. |

Diagnostics include fixed guidance and numeric JSON positions, without echoing
operator values, unknown field names or private paths. Correct the request locally
and deliberately retry; retain existing source/governance custody. Do not paste
private request contents into reports. Valid JSON still requires a nonempty operator
identity within 128 UTF-8 bytes and explicit `consent_local_private: true` before
source access/provisioning. A request does not admit a source or save/share a KU.

#### Resolve manual provisioning source/output failures

`ku_manual_source` reports bounded source/output setup diagnostics without printing
operator identity, private paths or text. `text_file` and `output_dir` relative
paths resolve against the **launch working directory**, not the request JSON's
directory. In the command above that directory is `src`; absolute paths avoid
this ambiguity. The request must still explicitly grant local/private consent.

| Diagnostic | Operator correction |
|---|---|
| `ku_manual_text_unreadable` | Check `text_file` exists and is locally readable plain text. Correct its path or read permissions; keep the private file outside Git. |
| `ku_manual_text_too_large` | Explicitly choose a shorter permitted source, at most 8192 UTF-8 bytes. The command does not truncate or rewrite input. |
| `ku_manual_text_invalid` | Supply plain UTF-8 text; UTF-16 and binary canonical objects are unsuitable. If needed, prepare a separate UTF-8 copy deliberately. |
| `ku_manual_text_empty` | Supply nonempty text; an empty or whitespace-only file is rejected. |
| `ku_manual_output_exists` | Retain the existing directory and both custody files. If this is the intended source, explicitly configure host admission; otherwise choose a new output directory name. Do not delete prior custody to retry. |
| `ku_manual_output_unavailable` | Choose a new directory under an existing private parent and check write permissions. The command does not create missing parents or replace custody. |

Source-validation failures occur before output creation. Directory-creation
failures do not write custody files. Successful provisioning preserves exact
input bytes, including whitespace; it still needs explicit host admission and
creates no saved/shared KU. If a later filesystem write fails after directory
creation, the directory may be incomplete: retain and inspect it locally before
selecting a new destination; this command does not roll back or overwrite it.
Request-file/JSON failures use the bounded diagnostics above. Later write errors
retain their existing diagnostics; writes are not transactional.

Try the journey:

1. Select an admitted source; enter a predicate label and look it up. Explicitly
   select the intended returned full CCID, then enter a manual text argument.
2. Click **Preview and validate**. Inspect validity, limitations, destination,
   exact IDs and canonical preview. No accepted KU exists yet. Leave the concept
   unresolved to inspect the non-saveable state. Cancel before correcting it.
3. Click **Save exact preview privately**. Only a committed receipt establishes
   save completion. Publication, Use, adoption and reward remain separate.
4. Click **Search / list** (empty query lists); inspect an exact object. Choose
   **Create revision**, edit the statement, preview and explicitly save again.
   The predecessor remains immutable; stale revision frontiers fail at the node.
5. If a response is lost, retain the displayed operation ID and use **Reconcile
   operation**. After page reload, use **Recover an operation**. This Web page
   uses its server-reserved operation ID as its idempotency key; the recovery
   form is for work created by this page, not arbitrary external operation keys.

## Limits and contribution boundaries

This is a finite manual editor, not arbitrary-text AI readiness. It supports one
predicate plus one text literal per draft, with a whole-source provenance span.
It does not infer negation, quantification, units, relationships or semantic truth.
Readiness labels describe host/service availability, never model qualification.
The owner holdouts and KU-ENC-003 branch remain untouched; no model is run.

Draft admission is memory-only until preparation; 256 admissions / 4 MiB of
encoded requests per process are retained, including canceled/completed inputs.
Restart to clear this bounded cache. Prepared/saved records use existing encrypted
node journals. Unprepared draft handles expire on process restart. Keep original
sources admitted for operations requiring source access. To revoke admission,
stop the host and change its source list; there is no live revocation editor.

The browser does not persist pending IDs or text across navigation/reload. Keep
the operation ID for explicit recovery; the page cannot enumerate lost pending
operations. Unused reservations after a lost reservation reply create no draft
or save. Canonical previews are exact Base64 bytes, not a semantic tree renderer.

CLI/Desktop lifecycle and packaging, raw source intake/capture governance UI,
private export management, publication/Use/adoption, richer manual semantics,
Registry distribution and real-model qualification remain separate work. Bounded
contributions can improve the canonical preview reader, host intake onboarding or
broaden UI verification without changing these authority boundaries.

## KU-QA-001 local MVP checkpoint — 2026-09-30

On Windows at main `689efd5` plus the uncommitted `ku_manual_source` helper and
Web label change, the real browser/API journey passed using the existing
`Documents/OneBrainLocal/host.json`, dataset, Vault key, token and signed Registry.
The missing executable was rebuilt. The initially empty source catalog was
provisioned with developer-owned `Water is a liquid.` and actual consent records;
the prior config was retained in `provisioning/host-before-manual-mvp.json` outside Git.

An unresolved manual preview showed `needs_resolution` and disabled Save. After
cancel, explicit Registry selection of `water` / local release Q283
(`784f7c285e57f5ec29348e2d7c2dd0c1`) and manual argument
`MVP demo 2026-09-30: Water is a liquid.` produced one ready private preview.
Its exact private ObjectCID, canonical bytes and receipt remain in the local
result outside Git. Their equality was checked across save/read/restart; use that
local result for operator inspection. This is the finite predicate/text manual
form, not automatic semantic encoding.

Reading preview left the saved list empty. The deliberate Web Save returned
`committed`, `published=false`, `authorizes_reward=false`. Search found the object;
Get returned the exact preview bytes. One real process restart using the same
dataset/key/Registry with `ollama` omitted preserved dataset generation, both IDs,
exact bytes and the same receipt. Web reload/Inspect also worked without a model;
reads left one saved object. Model configuration was then restored for task 09.C.
The status label now says **Automatic encoder** because its readiness bit does
not describe manual editing or saved reads.

Current focused checks: host build and Web build PASS; Web 89 tests PASS,
15 workflow tests PASS after the label edit; provisioning consent/preservation
test PASS; 2 manual API and 12 experimental API tests PASS; KU/vNext validators
and whitespace PASS. `rustfmt --check` passes for the added helper. Whole-workspace
`cargo fmt --all -- --check` reports existing formatting drift in unchanged
encoder/node/relay files; it was not repaired as part of the local demo.
CLI [42 unit + 2 integration results](KU_CLI_001_IMPLEMENTATION.md#verification)
and Desktop [8/10 lifecycle results](KU_DESK_001_IMPLEMENTATION.md#verification-commands)
are reused historical checks, not new native runs; the documented Windows `--lib`
test-host limitation remains. No new CLI/Desktop/native or all-OS claim is made.

Private local result/bytes and screenshot are in
`Documents/OneBrainLocal/development-reports/local-mvp-20260930.json` / `.jpg`.
They are outside Git. Keep the source/governance directory and dataset/keys.
The small experimental result is recorded in the
[existing Ollama instructions](KU_WEB_001_OLLAMA_IMPLEMENTATION.md#ku-qa-001-small-experimental-check--2026-09-30).

## Verification

Run on Windows; Cargo commands from `src`, npm commands from `src/onebrain-web`.

| Command | Result and evidence boundary |
|---|---|
| `cargo test --locked -q -p onebrain-api` | 24 library + 8 integration tests pass. Two new API tests use the actual manual provider with explicitly test-only signed Registry/source fixtures. |
| `cargo test --locked -q -p onebrain-api --features vnext-network-runtime --lib` | 26 tests pass, including existing private WS and the manual editor with the opt-in feature compiled. No runtime rollout is enabled. |
| `cargo test --locked -q -p onebrain-node --lib ku_` | 19 existing KU tests pass, including durable save/extraction recovery and custody fences. |
| `cargo check --locked -q -p onebrain-api --example ku_local_web` | Opt-in host example compiles. No operator production data was loaded or claimed qualified. |
| `cargo check --locked -q -p onebrain-api --no-default-features` | Feature-disabled library remains buildable. |
| `npm run test:ku` | 8 component/transport tests cover exact explicit save, revision binding, unresolved state, editor outage with retained reads, lost-save reconciliation, lost-reservation retry, changed host generations and accessibility. |
| `npm run test:vnext` | Both existing cross-language receipt tests pass. |
| `npm run build` | TypeScript + Vite production build passes. Generated KU imports are type-only; `erasableSyntaxOnly` is relaxed because the generated Base file also declares enums. |
| `npm run lint` | Pass under existing warning policy; pre-existing non-KU hook/Fast Refresh warnings remain. |
| `npm audit` | Zero reported vulnerabilities after compatible lockfile fixes for PostCSS, nanoid and React Router; no forced major upgrade. |
| `cargo fmt --all -- --check` | Pass. |
| `git diff --check` | Pass. |
| `python -m scripts.base.generate_contract --check` | Generated Base projections unchanged. |
| `python scripts/ci/validate_vnext_contracts.py` | Existing vNext contracts pass; additive editor contract is separate from the frozen original route inventory and is exercised by API tests. |

Accessibility evidence is labelled native controls, keyboard focus and automated
axe checks in jsdom. Color contrast and physical-browser/screen-reader operation
are not measured by jsdom. Component HTTP fixtures are tests, not a substitute
for the real host/provider path; no full browser-to-operator-dataset run or model
quality result is claimed. Existing dependency/compiler warnings remain.

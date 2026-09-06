# KU-WEB-001 local manual workflow

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

1. A complete activated signed Registry release directory and its independently
   trusted public key, using the existing Registry activation toolchain. The
   host uses `ConceptRegistryGenerationManager::open` to verify it. An unsigned
   `concepts.obr` or test fixture is not a substitute.
2. One to 64 existing canonical **LOCAL_ONLY Text SourceArtifacts** that the
   operator admits for this local principal. These are binary canonical object
   files from the trusted capture/custody producer, including actual governance
   references. They are not raw `.txt`, JSON views or Base64 text files. The
   host validates each object; each is bounded to 64 KiB and all to 4 MiB.
   Source acquisition and initial governance provisioning remain external host
   responsibilities. There is no browser raw-source upload or capture-policy UI.
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

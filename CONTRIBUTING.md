# Contributing to OneBrain

OneBrain is building a runnable concept MVP so people can test the idea and help
extend it. A small working contribution is useful now; contributors do not need
to complete the full research or production-qualification roadmap first.

## Start here

1. Read [AGENTS.md](AGENTS.md) for workspace instructions.
2. Read the single [MVP plan and contributor backlog](docs/handoffs/2026-09-ku-obp-productization/MASTER_PLAN.md).
3. Check [PROGRESS](docs/handoffs/2026-09-ku-obp-productization/PROGRESS.md) for
   existing implementations, unmerged branches and local work before duplicating it.
4. Choose one parent task or bounded backlog item. Explain the user-visible
   problem and the small improvement you want to make.

Good first contributions include correcting a setup step, making an unavailable
capability understandable, reproducing one semantic error, improving a Web view,
or fixing one platform-specific issue. Larger work belongs at the existing shared
service/provider boundary, not a separate implementation inside each frontend.

## Build and try the current local example

The primary MVP surface is local Web backed by the Rust node/API. You need the
Rust toolchain, Node/npm and the prerequisites declared by the existing host
instructions. Python is used by contract checks and the optional
[new-dataset secret helper](docs/handoffs/2026-09-ku-obp-productization/outputs/KU_WEB_001_IMPLEMENTATION.md#provision-secrets-for-a-new-dataset).
This is not yet a production installation: a real integration host needs a
signed Registry, local Vault key/API token and host configuration outside Git.
Do not invent production trust or use secrets from another contributor's machine.

**Fastest first run (fresh clone, DEV-ONLY, D-056):** one command checks the
prerequisites and creates every demo input outside the repository — a DEV-ONLY
synthetic signed Registry (a few concepts such as `water`/`liquid`; self-issued
signer discarded after activation; not operator trust; CCIDs incompatible with
real releases), a new Vault key/API token, a private sample source and `host.json`
— then prints the exact commands to start `/ku`:

```powershell
python scripts/base/bootstrap_ku_dev_demo.py --check-only
python scripts/base/bootstrap_ku_dev_demo.py --demo-root C:/OneBrainLocal/dev-demo --dev-demo-registry
```

On Linux/macOS use `python3` (fresh Ubuntu/Debian ship no `python`) and a demo root
such as `~/OneBrainLocal/dev-demo`; Rust needs a C toolchain (`build-essential`) and
the Web build needs Node.js 20.19+/22.12+. The printed next steps follow the
platform shell (PowerShell on Windows, POSIX `sh` elsewhere) and paste as-is.

It never prints secrets, overwrites files, saves/shares a KU or starts a host. It
also prints an equivalent `onebrain start --api` (`-p onebrain-cli`) host command
for the same data and `/ku`. Add `--with-receiver` for a second host config and
opt-in loopback network blocks, then run `check_ku_public_share_demo.py` with
`--receiver-kind web` (Web hosts) or `--sender-kind cli --receiver-kind cli
--require-cli-client` (CLI hosts plus `onebrain ku get/export`); the explicit
`--seed-sample-ku` flag saves the sample KU privately through the `/ku` button API
only when nothing is saved yet. The
[ku-dev-demo-scripts](.github/workflows/ku-dev-demo-scripts.yml) workflow runs the
script unit tests and `--check-only` on Ubuntu and Windows, plus a Linux job that
repeats this first run (Web build, DEV-ONLY bootstrap, the printed CLI/CLI share
check run verbatim). For a real Registry, pass
`--registry-root` and `--registry-public-key-file` instead of `--dev-demo-registry`.
Details: [SETUP_GUIDE](docs/SETUP_GUIDE.md) and the
[manual host guide](docs/handoffs/2026-09-ku-obp-productization/outputs/KU_WEB_001_IMPLEMENTATION.md#run-the-local-mvp).

After cloning your fork, these commands build the Web UI and the current host
example from the repository root (PowerShell; add `--features vnext-network-runtime`
when exercising opt-in loopback Public KU sharing directly from `ku_local_web`):

```powershell
npm ci --prefix src/onebrain-web
npm run build --prefix src/onebrain-web
cargo build --locked --manifest-path src/Cargo.toml -p onebrain-api --example ku_local_web --features vnext-network-runtime
```

Then follow the existing [manual local journey and host configuration](docs/handoffs/2026-09-ku-obp-productization/outputs/KU_WEB_001_IMPLEMENTATION.md)
or [experimental local model setup](docs/handoffs/2026-09-ku-obp-productization/outputs/KU_WEB_001_OLLAMA_IMPLEMENTATION.md).
Those documents describe implemented paths and historical checks; task 09 refreshes
and verifies the current primary journey. Experimental AI is unqualified and may
produce a draft requiring review. A draft is not automatically a saved KU.

Try supported preview → explicit private save → search/read → restart/read, then
the [manual local-to-peer concept](docs/handoffs/2026-09-ku-obp-productization/tasks/20-INT-KU-OBP-001.md#run-the-local-to-peer-manual-concept-d-045):
review a new Public object, explicitly confirm sharing, inspect the same Public
CID/bytes on the receiving node and retry/restart. You can run D-045 either
(1) directly through `/ku` Web and `ku_local_web` (with an opt-in loopback
`"network"` config block), `onebrain start --api` (with `--api-token-file`,
`--ku-vault-key-file`, `--ku-network-bind 127.0.0.1:0` and optional
`--ku-ollama-*`), or a `OneBrainNode` / `onebrain-desktop` host with opt-in
`VNextProductRuntime` enabled (`POST /api/vnext/ku/editor` `share_prepare`,
`share_confirm`, `share_intent`, `share_list`, `share_inspect`; see the
[normal-host share seam](docs/handoffs/2026-09-ku-obp-productization/outputs/KU_WEB_001_IMPLEMENTATION.md#normal-host-public-ku-reader-and-bounded-share-seam-d-045)),
or (2) through the isolated `ku_peer_demo` helper processes alongside the local
custody host. To check path (1) end to end on one machine, run
[check_ku_public_share_demo.py](scripts/base/check_ku_public_share_demo.py)
against a copy of your dataset (instructions in the linked task-20 section);
`--receiver-kind web` makes the receiver a second `/ku` Web host,
`--receiver-kind cli` makes the receiver a second `onebrain start --api
--ku-network-bind` host, and `--sender-kind cli` makes the sender
`onebrain start --api` (`--api-token-file`, `--ku-network-bind`, plus
`--ku-ollama-*` if `"ollama"` is configured). On Web and CLI receivers, when
`onebrain` is built (or `--require-cli-client` is passed), the helper also
verifies `onebrain ku get` and `onebrain ku export` (`canonical_public_exchange`)
with `--api-token-file` plus local-only offline receiver reads (`share_list`,
`workspace_summary`, `saved_semantic`, and fail-closed `share_inspect`).
Networking remains opt-in (default off), source material and
private IDs stay private, and D-045 currently supports one predicate and one text
literal. Real Registry and local-secret provisioning remain operator prerequisites,
described in the linked run instructions; the DEV-ONLY bootstrap above is only for
local demos. A bounded first contribution is
simplifying another first-run setup step, reproducing one semantic defect
from the [KU-SEM-001 backlog](docs/handoffs/2026-09-ku-obp-productization/outputs/KU_SEM_001_CONTRIBUTOR_BACKLOG.md),
or fixing one CLI/Desktop/platform parity gap so a newcomer has a clearer path.
Report the exact step
that fails, expected/actual behavior and your platform; a small public reproduction
is more useful than a large private trace. Never commit private inputs or keys.

## Where to make a change

| Area | Existing code |
|---|---|
| Canonical objects, IDs and storage | `src/ku-core`, `src/onebrain-base-contract` |
| Extraction and model adapters | `src/ku-encoder/src/extraction` |
| Shared KU workflow and networking orchestration | `src/onebrain-node` |
| Local API, CLI, Web and Desktop | `src/onebrain-api`, `src/onebrain-cli`, `src/onebrain-web`, `src/onebrain-desktop` |
| Peer transport and relay | `src/ku-net`, `src/onebrain-relay` |

Use the relevant contract when changing an interface. Source consent, explicit
save/share, stable IDs and saved-data preservation matter even in a demo. If a
feature is unsupported, expose that clearly rather than silently claiming success.
Mobile has its own [subtree instructions](src/onebrain-mobile/AGENTS.md) and build
contract; the desktop/local MVP does not bypass them.

## Work in small increments

- Inspect relevant refs, worktrees and uncommitted changes before starting. On
  your fork, use a short branch for the contribution; Codex branches use `codex/`.
  A local MVP substep can reuse a suitable existing checkout.
- Keep the substep in the existing parent task. Link it in the single ledger if
  coordinating shared work; do not create another roadmap/handoff/report per PR.
- Fix the smallest concrete obstacle and keep unrelated changes intact.
- Commit only intended files. Describe the outcome and link the parent task or
  issue; a public issue is useful but is not required for a small contribution.

## Check enough to trust the change

For an MVP change, run the affected happy path and appropriate focused tests.
A meaningful bug fix may need one regression. Documentation-only changes usually
need link/format checks, not new tests or a full workspace rebuild. Reuse existing
passing results when their scope still applies.

Useful focused commands from the repository root:

```powershell
npm run test:ku --prefix src/onebrain-web
cargo test --locked --manifest-path src/Cargo.toml -p ku-encoder extraction:: --lib
python scripts/ci/validate_vnext_contracts.py
```

Choose the command relevant to the changed area; this is not a mandatory bundle
for every PR. Existing applicable CI still runs. Do not disable a failing relevant
check to finish a demo; identify the cause and fix or document its actual scope.
Formal model qualification, independent reviewers, signed evidence, exhaustive
platform/fault matrices and long soak runs belong to their later tasks. They are
not prerequisites for contributing an experimental concept improvement.

## Pull requests

State the problem, what now works, how you checked it and known limitations.
Use a screenshot for a visual change when useful, not as an unconditional gate.
Do not report tests you did not run or qualify a model/platform from local smoke
results. Follow the [PR template](.github/PULL_REQUEST_TEMPLATE.md).

Report bugs with a concise reproduction, environment and expected/actual result.
English and Vietnamese are welcome in issues/PRs. Keep code identifiers and shared
contracts consistent with the existing codebase. Contributors are recognized in
[CONTRIBUTORS.md](CONTRIBUTORS.md).

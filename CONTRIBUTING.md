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
instructions. Python is used by contract checks. This is not yet a one-command
installation: the integration host currently needs a signed Registry, local Vault
key/API token and host configuration outside Git. Simplifying that setup is part
of the MVP contributor-entry work; do not invent production trust or use secrets
from another contributor's machine.

After cloning your fork, these commands build the Web UI and the current host
example from the repository root (PowerShell):

```powershell
npm ci --prefix src/onebrain-web
npm run build --prefix src/onebrain-web
cargo build --locked --manifest-path src/Cargo.toml -p onebrain-api --example ku_local_web
```

Then follow the existing [manual local journey and host configuration](docs/handoffs/2026-09-ku-obp-productization/outputs/KU_WEB_001_IMPLEMENTATION.md)
or [experimental local model setup](docs/handoffs/2026-09-ku-obp-productization/outputs/KU_WEB_001_OLLAMA_IMPLEMENTATION.md).
Those documents describe implemented paths and historical checks; task 09 refreshes
and verifies the current primary journey. Experimental AI is unqualified and may
produce a draft requiring review. A draft is not automatically a saved KU.

Try supported preview → explicit private save → search/read → restart/read, then
the [manual local-to-peer concept](docs/handoffs/2026-09-ku-obp-productization/tasks/20-INT-KU-OBP-001.md#run-the-local-to-peer-manual-concept-d-045):
review a new Public object, explicitly confirm sharing, inspect the same Public
CID/bytes on the receiving node and retry/restart. This uses two isolated peer
processes plus the existing local custody host; source material stays private.
The helper currently supports one predicate and one text literal. Registry and
local-secret provisioning remain operator prerequisites, described in the linked
run instructions. A bounded first contribution is improving a missing-Registry
or expired-preparation error so a newcomer knows the next required action.
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

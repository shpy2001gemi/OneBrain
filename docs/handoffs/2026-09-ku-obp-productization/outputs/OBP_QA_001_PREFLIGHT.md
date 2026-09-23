# OBP-QA-001 local preflight evidence

2026-09-23. Original workspace `C:/Users/shpy2/Documents/OneBrain`.
Branch `codex/obp-qa-001-nat-canary`, starting clean main
`fc65f08dede54d9e50731449776a5b2171d257ad`, independently checked against
`git ls-remote origin refs/heads/main`. Desktop merge `7e4fc14` and earlier
API/CLI/Web merges remain ancestors; all retained branches remain present.

## Deliverables and boundary

[Acceptance scenarios v1](OBP_QA_001_ACCEPTANCE_V1.md) maps ten scenarios to
existing harnesses and explicit missing real-network observations. The fixed
[local collector](../../../../scripts/runner/obp_product_preflight.py) records
commands, exit codes and SHA-256 log commitments, refuses output-directory
reuse and cannot emit positive qualification flags. Its integrity verifier
rejects changed logs, missing/duplicate suites, substituted commands/paths,
false success and qualification promotion. It is not a production verifier.

No runtime, API, CLI, Web or Desktop implementation changed. Every product
surface retains the same node-owned service. D-029 remains satisfied; no
transport review is reopened. No OneBrainLocal, Registry, jobs, production
keys, Ollama, live host configuration or networking opt-in was changed.

The original tree was clean before branching. QA files remain local and
uncommitted; no push/merge/branch deletion. No signed immutable candidate is
claimed from this working tree. D-024 model tuning remains Deferred.

## Local execution

Host/compiler: Windows, `x86_64-pc-windows-gnu`, rustc/cargo 1.96.0,
Python 3.13.14, Node v22.20.0. Cargo uses offline locked dependencies.
Fixtures use temporary stores and loopback listeners. Compilation and tests
start no production application or remote controller. Existing opt-in
real-Ollama tests remain ignored.

```powershell
python scripts/runner/obp_product_preflight.py --run target/obp-qa-001/2026-09-23-local-01
python scripts/runner/obp_product_preflight.py --verify target/obp-qa-001/2026-09-23-local-01/report.json
python -m unittest scripts.ci.test_validate_vnext_p5_canary_preflight scripts.ci.test_validate_vnext_p5_operations_preflight
git diff --check
```

All 13 collection groups returned exit code 0; report/log integrity verification
passed. Collection interval: `2026-09-23T02:35:36.595686+00:00` through
`2026-09-23T02:46:06.612802+00:00`.

| Group | Measured result | Scope |
|---|---|---|
| Node | 252 passed | Shared lifecycle/discovery/routing/service and durable journals |
| Routes | 32 passed | Five integration/matrix targets; includes ten modeled matrix tests |
| Relay | 20 unit + 11 integration passed | Local relay service and bounded carriers |
| ku-net | 315 passed | Core conformance and local fixtures |
| API | 44 unit + 8 integration passed; 1 ignored | Real local API/WS; opt-in real-Ollama test ignored |
| API feature-off | 16 passed | Disabled/unavailable boundary |
| CLI | 51 unit + 2 integration passed | CLI plus actual temporary node/API host |
| Desktop | 6 passed | Shared service, lifecycle fencing, restart/recovery |
| Desktop feature-off | 4 passed | Local-only shell and disabled feature |
| Web | 89 passed | Adapter/component fixtures, not native WebView E2E |
| Web receipts | 2 passed | Frozen receipt projections |
| Python contracts/collectors | 143 run, 142 passed, 1 skipped | Includes 7 new local collector integrity tests; existing Linux bundle-mode test skipped on Windows |
| vNext validator | PASS | Frozen contracts remain consistent; this does not verify a production run |

Supplemental P5 canary/operations contract mutation tests: 22 passed.
Existing unused/dead-code compiler warnings remain. No test failure was hidden
or turned into a scenario pass. The Linux skip and ignored model test retain
their own unexecuted scope.

Local raw collection: `target/obp-qa-001/2026-09-23-local-01/`, with the exact
argv/exit status/SHA-256 for every log in `report.json` and the retained
`partial.json`. These ignored local artifacts are retained on this workstation;
they are not uploaded public evidence or a portable signed receipt bundle.
Report SHA-256: `52ae3b33b6556ec7b1cb265184a746376701cefe979c27db1383201a2ced01e4`.
Collector SHA-256 at execution: `bda45e8039913cc0583ac292947abb0645ddf67d82a005a592c46fd1329ec4d3`.

`git diff --check` and local handoff link checks pass. `git diff --name-only --
src` is empty: application code, contracts and default configuration are unchanged.
No frontend asset build, native GUI launch, physical lifecycle, Internet/NAT or
production P5 execution is claimed by this collection.

## Remaining acceptance blockers

No verified two-consumer independent-network environment or two independently
configured relay hosts was supplied. No real bootstrap outage, independent
relay shutdown, bidirectional product exchange over consumer NAT, packet/artifact
privacy collection, native WebView parity or real OS sleep/interface-switch
scenario was executed. Existing local fixture evidence does not fill those gaps.

The P5 lane additionally lacks this candidate's exact signed request/inventory,
three physical Linux hosts, approved topology/provider bundle, signed child/admin
receipts, complete fault/oracle/resource/privacy observations and cleanup proof.
The repository's historical Linux reference and owner-attested provider status
are not fresh evidence for this Desktop candidate. Strict Base remains excluded.

Keep `product_acceptance_qualified=false`, `multi_host_qualified=false` and
all platform qualification flags false. Task 18 cannot be accepted as complete;
task 19 must not start. The next step is the authorized isolated environment and
evidence collection described in acceptance v1, without changing the live host.

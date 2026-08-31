# Base v1 dependency audit and triage

This document defines the Task 26 dependency gate. It is not a qualification
claim. A Base release must retain the raw reports produced from the exact
candidate, bind them into each OS lane receipt, and pass the independent
provenance verifier.

## Frozen commands and inputs

The three-OS workflow runs these commands without changing the source tree:

```text
cargo audit --file src/Cargo.lock --json
npm audit --package-lock-only --json --prefix src/onebrain-base-contract/conformance/typescript
cargo metadata --locked --manifest-path src/Cargo.toml --format-version 1
```

`cargo-audit` is installed as the locked published crate version `0.22.2`, the
reviewed version that parses the current RustSec CVSS 4.0 advisory corpus.
Cargo dependencies come only from `src/Cargo.lock`; TypeScript conformance
dependencies come only from its lockfile version 3. Raw stdout is preserved
before classification. A command failure above exit code 1, malformed JSON,
missing report, or report digest mismatch stops the lane.

## Severity and disposition

| Base class | Upstream signal | Required disposition |
| --- | --- | --- |
| P0 | known active exploitation, credential/key compromise, arbitrary code execution in a shipped/default Base path | resolve; no risk acceptance |
| P1 | high-impact vulnerability reachable from a shipped/default Base path, or supply-chain integrity loss | resolve, or explicit owner-approved time-bounded acceptance before a candidate rerun |
| P2 | lower-impact or build/test-only finding with demonstrated non-reachability | document evidence and remediation owner |
| P3 | informational, unmaintained, or duplicate advisory without current Base exposure | document and monitor |

Every advisory emitted by either raw report must have one unique triage row.
Unknown, missing, duplicate, or `untriaged` P0/P1 rows fail provenance. An
`accepted-risk` P1 must name the owner, expiry, and compensating control in the
candidate evidence; Task 27 may impose a stricter no-acceptance rule.

## Current reviewed snapshot

Review date: 2026-08-31. Candidate integration commit at the start of Task 26:
`6f5c44a1940919995c351efbc4a46ef310f21ee3`.

The locked TypeScript audit reports zero vulnerabilities (23 total packages,
including optional target packages). A local `cargo-audit 0.22.2` run reports
zero vulnerabilities and 18 warnings. `cargo tree --locked --target all`
confirms every warned package is reachable only from `onebrain-desktop` through
Tauri/build dependencies. Desktop product completion and packaging are outside
the Base v1 gate in the approved program design; none is reachable from the
default Base Node/API/CLI artifact. The first three-OS run still produces the
authoritative raw reports and fails on any advisory ID outside this reviewed
closed set.

| Advisory | Ecosystem | Base class | Disposition | Owner / expiry / evidence |
| --- | --- | --- | --- | --- |
| _none in reviewed npm lock_ | npm | N/A | resolved | raw local npm audit, 2026-08-11 |
| `RUSTSEC-2026-0258` | Cargo (`h2` via `hyper`/`axum` and `reqwest`) | P2 | resolved | default Base API/Node/CLI path; upgraded locked `h2` from `0.4.15` to patched `0.4.16` on 2026-08-31; `cargo-audit 0.22.2` then reports zero vulnerabilities |
| `RUSTSEC-2026-0221` | Cargo (`event-listener` via desktop notification/single-instance) | P2 | documented-non-base | desktop-only target-all tree; revisit before desktop release |
| `RUSTSEC-2024-0429` | Cargo (`glib` via Tauri GTK) | P2 | documented-non-base | desktop-only target-all tree; revisit before desktop release |
| `RUSTSEC-2024-0411`, `0412`, `0413`, `0414`, `0415`, `0416`, `0417`, `0418`, `0419`, `0420` | Cargo (GTK3 bindings) | P3 | documented-non-base | unmaintained desktop-only GTK stack |
| `RUSTSEC-2024-0370` | Cargo (`proc-macro-error` via GTK macros) | P3 | documented-non-base | desktop-only build dependency |
| `RUSTSEC-2025-0075`, `0080`, `0081`, `0098`, `0100` | Cargo (`unic-*` via Tauri `urlpattern`) | P3 | documented-non-base | desktop-only build/runtime dependency |

The table deliberately contains no sample advisory or blanket acceptance.

The shortened IDs in grouped rows inherit the `RUSTSEC-2024-` or
`RUSTSEC-2025-` prefix printed at the start of that row. The verifier stores
and compares every full ID separately; grouping here is presentation only.

## Review procedure

1. Confirm the report bytes and digest belong to one release-request digest,
   qualification session, candidate commit/tree, target, and toolchain.
2. Reproduce the advisory against the locked dependency graph and determine
   whether it is shipped, default-active, optional, test-only, or unreachable.
3. Assign the Base class above; do not copy an upstream severity without the
   reachability analysis.
4. Resolve or obtain the explicitly allowed disposition, then rerun all three
   OS lanes. Never edit a retained lane receipt.
5. Task 27 consumes only the newly verified provenance receipt and raw hashes.

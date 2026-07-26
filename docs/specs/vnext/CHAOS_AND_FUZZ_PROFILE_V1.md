# Chaos and Fuzz Profile v1

Status: normative DR-M5 M5-04 contract.

Machine-readable contract:
[`dr-m5-chaos-fuzz-v1.json`](../../../src/test-vectors/vnext/dr-m5-chaos-fuzz-v1.json).

## Feature firewall

- The chaos harness MUST remain behind the default-off `vnext-chaos-harness` feature.
- Production/default builds MUST NOT expose partial-stream test injection or fuzz-only parser entry points.

## Real-QUIC chaos

- Acceptance MUST use authenticated real QUIC connections rather than an in-memory carrier substitute.
- Acceptance MUST exercise drop, duplicate, delay, reorder, disconnect, partition/reunion, and slow reader/writer behavior.
- A reconnected peer MUST authenticate a new session before reconciliation resumes.
- A real-QUIC scenario MUST terminate inside the frozen 15-second bound.
- Fair redelivery MUST converge to the exact same CID set as an unfaulted delivery.
- Carrier fault behavior MUST NOT grant authority or claim network-wide completion.
- Wire frames MUST NOT disclose the frozen private-need canary.

## Adversarial resource pressure

- Pre-auth flood acceptance MUST attempt 20,000 rejected handshakes while retaining only the admitted bounded state.
- Authenticated-session flood acceptance MUST attempt 1,024 excess promotions while retaining only two admitted sessions.
- Authenticated context/manifest flood acceptance MUST retain at most eight live contexts.
- The invalid-CID parser campaign MUST process 4,096 unique bounded inputs without growing authenticated context state.
- Slowloris acceptance MUST enforce the frozen 75-millisecond read deadline.
- Flood rejection MUST fail explicitly without authority amplification.

## Long trace oracle

- The property suite MUST execute 64 deterministic seeds with 4,096 steps and 64 records per seed.
- Every long trace MUST cover every frozen chaos fault family.
- The final fair-redelivery oracle MUST equal the frozen BLAKE3 root in the machine profile.
- The oracle MUST remain selector/content scoped and MUST NOT assert global completeness.

## Parser fuzzing

- PR acceptance MUST smoke all six frozen fuzz targets against exactly three versioned seeds per target.
- Every fuzz input MUST be bounded to 4,096 bytes.
- An accepted canonical, session, reconciliation, carrier, or journal encoding MUST re-encode byte-for-byte.
- Domain-record fuzzing MUST cover Object, Event, Feed, Authority, UseEvidence, and DerivationEvidence decode paths.
- Legacy fuzzing MUST preserve the no-authority boundary and MUST NOT serialize `GLOBAL` as a vNext scope.
- Nightly CI MUST pin Rust nightly-2026-07-20, cargo-fuzz 0.13.2, and libfuzzer-sys 0.4.13.
- Nightly CI MUST give every frozen target 60 seconds total, a 10-second per-input timeout, and a 4,096-byte maximum.
- Nightly crashes MUST upload artifacts for 14 days.

## Exit

- The package MUST exit with zero panic, OOM, hang, privacy leak, or invariant violation in its bounded acceptance gates.
- Corpus, target inventory, budgets, oracle root, and corpus digest MUST remain machine validated.

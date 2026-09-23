# OBP-QA-001 functional acceptance v2

2026-09-23. Owner decision D-039 changes the completion criterion for task 18:
working code and tests of the accepted design are sufficient. The ten procedures
in [acceptance v1](OBP_QA_001_ACCEPTANCE_V1.md) remain useful for a future
consumer-network run, but their strict host and artifact collection is no longer
a prerequisite for functional QA review. This does not convert local tests or
the existing VPS into consumer-NAT or platform qualification.

2026-09-24 D-041 closure decision: the owner directed skipping consumer-NAT
qualification when the three existing VPS are insufficient, then completing
task 18 and advancing to the dependent task. The [two-VPS admission](OBP_QA_001_VPS_PRODUCT_ADMISSION_20260923.md)
already establishes that limit. Functional QA is accepted under D-040;
`consumer_nat_qualified=false` and the ten independent-network product
scenarios remain unrun. This is a scoped waiver of that gate, not a passing
qualification result.

## Functional checks

- The node-owned runtime remains opt-in. The Desktop supervisor, local API,
  CLI and Web use its shared service; none starts a second network owner.
- The two-relay integration test now sends durable domain intents A→B and B→A
  through authenticated, pre-reserved relay connections. It checks the
  receiver's durable feed state, sender acknowledgements and checkpoints,
  selected-relay loss, and continuation through the alternate relay.
- Existing node tests cover discovery, source loss, wrong peer, replay,
  reconciliation, restart, outbox, privacy and bounded failure behavior.
  Desktop, API, CLI and Web suites cover the shared status and command surfaces.
- The separate exact-candidate Linux P5 V2 three-host gate already qualified
  on `c453f3e`; see [its report](OBP_QA_001_P5_PRODUCTION_20260923.md).

## Verification in this continuation

`onebrain-node` feature-enabled library: 253 passed. Desktop lifecycle: 6
passed. API feature-enabled library: 44 passed, 1 existing opt-in model test
ignored. CLI: 51 unit and 2 integration passed. Web: 89 passed. The aggregate
vNext contract validator passed. The bidirectional two-relay test passed
individually and in the node suite. The edited Rust test file passes
`rustfmt --check`; workspace-wide formatting reports pre-existing differences
in other files. This document is a functional QA checkpoint, not a signed
evidence bundle.

## Claim boundary

No ordinary consumer NAT, two independent consumer networks, native WebView
end-to-end run, OS sleep/interface event, Windows/macOS platform qualification
or default rollout is claimed. The read-only two-VPS admission found no
supported product executable on those hosts; no product scenario ran there.
Keep the existing P5 production qualification separate. All prior remote
staging, runs and durable state are retained. Networking remains default-off,
no tuning occurred, and OBP-MIG-001 has not started.

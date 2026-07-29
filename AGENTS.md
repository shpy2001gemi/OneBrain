# OneBrain agent instructions

## Mobile build trigger

These rules apply to every task that creates or changes the autonomous mobile
app, its Flutter/native/Rust bridge, mobile packaging, or mobile implementation
evidence.

1. Run `python scripts/ci/validate_mobile_build_contracts.py` before planning or
   editing mobile implementation.
2. Open
   `docs/design/mobile/mobile_build_contract_v1.json` and read every file in
   `required_read_set` completely. A README, screenshot, prior chat summary, or
   generated design is not a substitute.
3. Respect the manifest's authority order. The pinned distributed-runtime plan
   wins for shared runtime semantics; the mobile architecture then constrains
   platform ownership; the implementation plan controls sequencing; feature,
   sitemap, component, pattern, and token documents control product/UI
   realization.
4. Before editing, name the active `MOB-00..09` work package and the affected
   `MOB-*`, `MOB-SCR-*`, `OBM-CMP-*`, and `OBM-PAT-*` IDs. If an applicable ID
   does not exist, update the owner-approved specification before inventing
   implementation behavior.
5. Do not silently resolve a conflict between canonical documents. Stop, record
   the exact conflict, and request owner direction.
6. Update
   `src/onebrain-mobile/compliance/mobile_build_evidence_v1.json` with the
   current phase/work package and evidence. A target document is not
   implementation evidence.
7. Run the mobile contract validator and relevant build/tests after changes.
   Do not claim completion while the validator is red.

The detailed subtree rules are in `src/onebrain-mobile/AGENTS.md`.

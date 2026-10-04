# OneBrain agent instructions

## Current delivery priority and task continuity

Owner direction on 2026-09-28: deliver a runnable concept MVP quickly so people
can evaluate the idea and join development. Build on the existing shared
architecture; do not try to finish the entire research/production roadmap alone.

- Use `docs/handoffs/2026-09-ku-obp-productization/MASTER_PLAN.md` for active
  scope/order and its `PROGRESS.md` for state, branch/worktree inventory and next
  action. D-044 in that folder's `DECISIONS.md` records the owner-approved scope.
- Before new work, inspect relevant local/remote refs, worktrees and uncommitted
  changes. Work missing from main may already exist elsewhere; do not recreate
  or discard it. Reuse suitable work and record its actual integration state.
- Keep substeps in the existing parent task. Update the same plan/ledger and
  existing task output instead of creating a new plan, report or handoff per
  branch/attempt. New chats read the short entry point and relevant task only.
- For the MVP, use a working happy path plus focused checks for changed behavior,
  saved-data preservation and explicit private/save/share boundaries. Reuse
  passing tests. Do not require exhaustive matrices, independent evaluators,
  formal evidence bundles or model qualification for the experimental demo.
- Keep deferred quality, portability and research gaps discoverable for
  contributors. A demo must label unsupported/unassessed behavior honestly;
  completing an MVP task does not grant production/model qualification.

This priority does not change the mobile-specific instructions below.

## Standing Git authorization

Owner direction on 2026-10-04: always permit merge/push for OneBrain work;
do not ask for merge/push approval again. D-050 in the existing DECISIONS.md
records this standing authorization and supersedes D-010's separate approval
requirement. Integrate scoped, checked work and its handoff updates; verify actual
remote state and preserve unrelated dirty/untracked/unmerged work. This changes
Git authorization only; existing product, trust, consent and mobile contracts
still apply.

## Handoff after every meaningful task/checkpoint

- Start with `docs/handoffs/2026-09-ku-obp-productization/NEXT_CONVERSATION.md`.
  Read only the bounded `CURRENT_CHECKPOINT_START` / `CURRENT_CHECKPOINT_END`
  block in `PROGRESS.md`, then the named task/files. Expand context only as needed;
  applicable mandatory contract read sets, including mobile, still apply.
- Before ending work, handing off or changing tasks after a meaningful checkpoint,
  replace that block using the template in NEXT_CONVERSATION: actual result,
  remaining work, concrete next action, focused file pointers, Git/dirty state,
  retained work, checks and applicable decisions. Do not append a growing log there.
- Update the parent checklist and ledger when state changes; synchronize the task
  overview when affected. Keep history to a short entry linking existing artifacts.
  Reuse the same handoff and fixed prompt; do not create a new file per checkpoint.
- If interrupted before a handoff update, recover from actual Git/artifacts rather
  than treating the stale checkpoint as proof of completion. Never record secrets.

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

# Next conversation — build the product framework

> Owner direction: 2026-09-20. Accept the current semantic implementation as
> sufficient for this development stage and move on to the other functions.
> Current checkpoint: KU-CLI-001 implemented/tested; OBP-PROD-001 accepted in D-025.
> OBP-PROD-002/003 implemented/tested locally under D-025/D-026. Latest node
> evidence: 234 unit + 22 integration tests pass. Semantic quality work stays in the contributor backlog.
> See [CLI evidence](outputs/KU_CLI_001_IMPLEMENTATION.md) and
> [OBP proposal/evidence](outputs/OBP_PROD_001_CONTRACT.md).

## Start in the correct workspace

Use `C:/Users/shpy2/Documents/OneBrain`, current branch
`codex/obp-prod-003-discovery`. All uncommitted CLI/Web/semantic changes
are still in this original tree; task branch refs alone do not contain them.
Observed HEAD: `467e2ad3ada305597b2d1808c03da6190d0643cb`.
The actual implementation includes many modified AND untracked files; HEAD alone
is not the baseline. The separate `C:/Users/shpy2/.codex/worktrees/3bbf/OneBrain`
checkout does not contain this complete work. Do not create a fresh checkout from
main and assume the follow-ups are present. Inspect status first, preserve all
changes, and work in the original tree. The CLI was implemented on `codex/ku-cli-001-workflow`, then all dirty changes
were carried through the OBP contract/lifecycle/discovery branches. Branch separation must retain
the complete dirty baseline. Do not reset, clean, stash away, overwrite, commit, push or merge the
existing work as part of this handoff. No Git publication was requested.

This continuation overrides older package instructions to wait for another
semantic approval, tune the rocket example before other work, or start from clean
main. It does not waive contracts or authorize production activation.

## Minimal read set

1. Repository `AGENTS.md` and applicable subtree instructions.
2. This file, current checkpoint in [PROGRESS.md](PROGRESS.md), and D-024/D-025/D-026 in
   [DECISIONS.md](DECISIONS.md).
3. [OBP-PROD-001](tasks/10-OBP-PROD-001.md), its
   [concrete proposal](../../specs/vnext/OBP_PRODUCT_ORCHESTRATION_PROFILE_V1.md),
   machine inventory and [evidence](outputs/OBP_PROD_001_CONTRACT.md).
4. [CLI implementation/evidence](outputs/KU_CLI_001_IMPLEMENTATION.md) when
   reviewing or changing CLI. Use the linked product/API/CLI contracts for edits.
5. [Task 003 evidence and limits](outputs/OBP_PROD_003_IMPLEMENTATION.md) and
   [task 004 scope/prerequisites](tasks/13-OBP-PROD-004.md) before routing work.

Do not reload all semantic history or run model experiments by default.

## Next work and sequence

KU-CLI-001 is locally implemented and tested: eleven operations plus reservation
through shared REST, exact typed cancellation, bounded requests/responses,
private save, scope-honest output and explicit reconciliation. All 42 CLI unit
and two integration tests pass, including an actual temporary API/node journey.
No merge or source-intake/draft-to-canonical capability is claimed.

OBP-PROD-001 defines 13 logical operations, 18 DTOs and 26 contract fixtures.
The owner accepted this concrete proposal in D-025 and explicitly authorized
OBP-PROD-002 local implementation before merge. Do not ask for that approval again.
See [lifecycle implementation evidence](outputs/OBP_PROD_002_IMPLEMENTATION.md)
and the current PROGRESS ledger for validation and remaining task boundaries.
The owner then explicitly authorized task 003 local before task 002 merge in
D-026: “Cho phép 003 local, chưa merge”. Task 003 is now locally implemented
and tested; see [discovery evidence](outputs/OBP_PROD_003_IMPLEMENTATION.md).
Do not ask again for permission to implement or review task 003.

Next lane is OBP-PROD-004, then OBP-API-001 in dependency order. Task 004 still
requires task 003 merged or a new explicit local exception; D-026 is limited to
task 003. Do not infer an authorization to merge or waive task 004's prerequisite.
Preserve frozen wire and authority semantics and opt-in networking. Do not commit,
push or merge without explicit authorization. D-025/D-026 authorize local work only.

KU-DESK-001 requires its Web merge prerequisite, which has not occurred here.
Cross-surface release/qualification gates remain separate. Mobile is outside
this continuation. Preserve the running local host, Registry, jobs and secrets.

## Current foundation and evidence

- Shared semantic selection v1 and private draft v2, Rust assembly/validation,
  bounded repair, encrypted durable background jobs and Web draft rendering exist.
- V2 represents OR, ellipsis, references and comparisons with provenance. Latest
  predicate repair preserves the qualifier and only offers exact local substrings.
- Encoder: 70 passed, one opt-in worker test ignored; API: 8 passed; Web: 22 passed.
  Web and staging-host builds and aggregate vNext validator passed.
- Qwen v2 final development run: 2/13; six-case transport retry: 1/6 with no transport
  errors. Gemma comparison: 2/3. Latest rocket replay fixes the predicate but still
  fails reference/future meaning checks. These are known development cases only.
- Latest v2 executor commitment:
  `7c1e2c767aa2c053a0ee2b3cdfb7d658d82a915712bb701cd625ba4d145fdc7a`.
  Changed code changes commitments; never relabel old jobs or reset their budget.
- Runtime: `C:/Users/shpy2/Documents/OneBrainLocal`; existing Web
  `http://127.0.0.1:4280/ku`, Qwen3 8b, unqualified. V2 is not activated there.
  Staging binary is built, not launched. Preserve jobs, Registry, keys and services.
  Do not expose secrets or stop the user's host/Ollama for unrelated work.

See [contributor backlog](outputs/KU_SEM_001_CONTRIBUTOR_BACKLOG.md). Owner acceptance
means enough foundation to continue, not semantic correctness, model qualification,
canonical lowering, public rollout, or a merge. No need to ask again whether to
move on; that direction is approved.

## Copy into the new conversation

```text
Hãy đọc AGENTS.md và docs/handoffs/2026-09-ku-obp-productization/NEXT_CONVERSATION.md
trên working tree gốc C:/Users/shpy2/Documents/OneBrain.
KU-CLI-001 đã triển khai và kiểm tra; OBP-PROD-001 đã được chấp nhận (D-025).
OBP-PROD-002/003 đã triển khai và kiểm tra local theo D-025/D-026, chưa merge.
Task 004 còn điều kiện task 003 merged hoặc ngoại lệ local mới từ owner.
Đọc evidence và PROGRESS hiện tại rồi tiếp tục đúng dependency/task scope. Giữ toàn bộ thay đổi chưa commit, không dùng
main hay worktree khác thay baseline. Không tuning model hoặc tự bật networking.
```

# OneBrain concept MVP — start here

Owner direction is [D-044](DECISIONS.md#d-044--runnable-concept-mvp-first-one-plan-and-one-progress-ledger):
build a working concept quickly, preserve the shared architecture and make it
easy for open-source contributors to help. Strict qualification is deferred
from MVP delivery; experimental behavior must still be described accurately.

## Read only what you need

1. Repository [AGENTS.md](../../../AGENTS.md).
2. [NEXT_CONVERSATION.md](NEXT_CONVERSATION.md): handoff procedure and stable prompt.
3. Only the marked Current checkpoint block in [PROGRESS.md](PROGRESS.md).
4. The named parent task/files; relevant contracts and [MASTER_PLAN](MASTER_PLAN.md)
   sections only as needed. Do not load the full progress history by default.

[NEXT_CONVERSATION.md](NEXT_CONVERSATION.md) also defines the update template
agents use after each meaningful task/checkpoint.
[Task index](tasks/README.md) retains the existing task IDs. Substeps stay in their
parent task; do not create a new plan/report/handoff for each branch or attempt.

## What we are demonstrating

A local KU can be inspected, explicitly saved, found after restart and explicitly
exchanged with another node. Existing experimental AI can show a few public or
developer-owned examples with clear draft/needs-review outcomes. A supported
manual/resolved path provides the reliable save demonstration. Finish with usable
setup/run instructions and bounded contribution opportunities.

Use local Web on the available host as the primary surface and CLI for inspection
and a second node. Reuse shared services and existing Desktop integration. Broad
model, NAT, OS and release qualification do not block this concept demo.

## Preserved work and history

The exact inventory and next action live only in PROGRESS. It covers main,
retained qualification commits, local preparation scripts/reports and both
worktrees. Never infer that work is missing merely because it is absent from main.
The qualification tools/data stay available for later; do not resume that lane
just because an older continuation says it is the next task.

Historical details remain in PROGRESS, [DECISIONS](DECISIONS.md) and existing
[outputs](outputs/README.md). The retained local qualification preparation
reports are inventoried in PROGRESS; they are outside the committed MVP slice.
Their former priority is superseded by D-044.

## Working rules

D-044 and MASTER_PLAN govern the current lightweight workflow. Older ledger
quotes of this section describe the rules in force at their recorded date.
This folder governs delivery coordination, not new wire or canonical semantics.
Relevant product contracts remain under [vNext specs](../../specs/vnext/README.md).
Mobile keeps its separate build contract and is outside this MVP implementation.

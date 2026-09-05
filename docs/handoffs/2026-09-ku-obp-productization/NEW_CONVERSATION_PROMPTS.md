# New conversation prompts

Replace `<TASK_FILE>`, `<TASK_ID>` and `<BRANCH>` with values from
`PROGRESS.md`. These prompts intentionally avoid asking a new conversation to
read all historical plans.

## Start one task

```text
Read the repository AGENTS.md, then read only:
1. docs/handoffs/2026-09-ku-obp-productization/README.md
2. docs/handoffs/2026-09-ku-obp-productization/PROGRESS.md
3. docs/handoffs/2026-09-ku-obp-productization/<TASK_FILE>

Execute only <TASK_ID> on branch <BRANCH>. Follow the task's exact required
read set, scope, exclusions and acceptance criteria. Preserve unrelated work.
Update PROGRESS.md and the README Current task pointer before handoff. Push the
task branch, but do not merge or delete it without my explicit instruction.
```

## Review a completed task

```text
Review <TASK_ID> against its task file and canonical references. Inspect the
branch diff and run the focused acceptance commands. Report findings first.
Do not modify, merge or delete the branch unless I explicitly ask you to fix or
merge it.
```

## Merge an accepted task and prepare the next one

```text
I accept <TASK_ID>. Verify its branch is clean, pushed and passes the task
gates; merge it into main without changing unrelated history, push main, update
the handoff PROGRESS.md/README pointer for the next dependency-ready task, and
clean only the merged local task branch/worktree. Do not delete remote branches
unless I explicitly request that.
```

## Resume after interruption

```text
Read the handoff README, PROGRESS.md and the current task file. Inspect the
current Git branch/worktree and continue from existing changes. Do not restart
the task, recreate completed work or load unrelated historical plans.
```

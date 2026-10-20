# Escalation: prd4a: group C's R2' run and R4' passes were deleted with its scratch; items 2-4 need R2' regenerated

- id: `esc-20261020T145216Z-f94a9d32`
- raised: 2026-10-20T14:52:16Z
- from: zach-opus-prd4a
- worktree: `/Users/alex/ab/richos-wt/zach-opus-prd4a` (branch `cc/zach-opus-prd4a`)
- head: `db9ce714d6dba47e0811da5c49a0313a51670040`
- state: **proceeding**
- for: lead

## The question

None needed unless you object: I am re-running the author's complete run R2' (about 70 min measured last time on a contended host) because items 3 and 4 and the land check all consume it and the brief's 'the 86 passes are kept for the same tree' no longer holds.

## What was already tried

Searched for run 20261020T073243Z-jgcyn7x7 under /Volumes/E1TB/state/richos/proof-runs and /Volumes/E1TB/tmp/claude: /Volumes/E1TB/tmp/claude/zach-opus-prd3c (which held proof-runs/ and autocheck/, per 03-C appendix: 'deleted after this pass') no longer exists; only logs and stats were copied to 03-C-evidence/. Commits 855ebed52, cd8d7e7c0 and the replay-only 9f87f3685 still exist as objects.

## Proceeding meanwhile

Item 1 (apply_fresh patch, pins, test, variant) first; then the alone proof-for run in the land seat (item 2 diagnosis) on a quiet host; then R2' rebuilt with the same helpers; then items 3, 4 and the land check.

## How this reaches the lead

This file is the RECORD, not the delivery. The escalation was written to the
engine escalation ledger at `/Users/alex/.claude/state/escalations.jsonl` as `esc-20261020T145216Z-f94a9d32`, which the
lead's session reads at session start and at every turn end WITHOUT this branch
being merged. If this file is never landed, the escalation still arrives.

Close it with:

    escalate.sh ack esc-20261020T145216Z-f94a9d32 --disposition "<what you decided or did>"

# Escalation: Verification rollout: the land's new readiness preflight still costs about 10 CPU s (14 to 22 s wall on today's saturated host), PRD line 139's 10 s budget not demonstrated

- id: `esc-20261020T181006Z-e8cb12ae`
- raised: 2026-10-20T18:10:07Z
- from: zach-opus-prd4b
- worktree: `/Users/alex/ab/richos-wt/zach-opus-prd4b` (branch `cc/zach-opus-prd4b`)
- head: `cbf94aea57b63a83bed2701f1b6e04784cec84b9`
- state: **proceeding**
- for: lead

## The question

Land the switch with the preflight at about 10 CPU s per land of a branch (cut from 48.5 s; every remaining second is the fresh reading PRD line 57 requires) or time it first on a quiet host before landing?

## What was already tried

Timed old and new runner bookkeeping on the same 94-check tree with no check executed, interleaved; profiled; fixed three regressions (richos 9f10f1601 saved plan as selection, 36b8c2e34 closure scans once per stat identity, 042986a63 one browser walk per pass). Runner reuse-only path now equals the old one. Preflight: 48.5 s derived / 69.7 CPU s before, 13.8 to 22.5 s derived / 10.4 to 10.8 CPU s after, host 52 to 76% busy at admission, load up to 36. Remaining cost: one identity pass over the plan, the nightly suite list, runtime verification, one browser walk. No quiet host was available today. Record: richos-hq 03-E-rollout.md on cc/zach-opus-prd4b.

## Proceeding meanwhile

Finishing the recovery demonstration on the fixed code, the owning suites and the handover; nothing is landed or switched on by me.

## How this reaches the lead

This file is the RECORD, not the delivery. The escalation was written to the
engine escalation ledger at `/Users/alex/.claude/state/escalations.jsonl` as `esc-20261020T181006Z-e8cb12ae`, which the
lead's session reads at session start and at every turn end WITHOUT this branch
being merged. If this file is never landed, the escalation still arrives.

Close it with:

    escalate.sh ack esc-20261020T181006Z-e8cb12ae --disposition "<what you decided or did>"

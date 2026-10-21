# Escalation: landgate2: reusing Echo's item 4 (login-shell PATH) puts Echo's items 0 and 4 on my branch; land order needed

- id: `esc-20261021T091526Z-6987bba6`
- raised: 2026-10-21T09:15:26Z
- from: zach-opus-landgate2
- worktree: `/Users/alex/ab/richos-wt/zach-opus-landgate2` (branch `cc/zach-opus-landgate2`)
- head: `25c6c24142e1d8480089768b9ff3a268e9676348`
- state: **proceeding**
- for: lead

## The question

My branch cc/zach-opus-landgate2 now merges Echo's d53d42777 (item 4) to fix the cargo-PATH finding, as you directed; item 4's parent is item 0 (6a246fab3), whose review rv-20261021T072931Z-6a246fab-ad3e#1 (user_settings.rs:74, starter instructions.md seeded empty) is still open and blocking. So landing my branch also lands items 0 and 4. Land Echo's branch L first (or together) or tell me to drop the merge and leave the PATH fix to branch L?

## What was already tried

Item 4 is the only reuse: EngineProfile::configure sets every lease's PATH to runtime bin + the user's login PATH + system folders and land_merge's merge checks inherit it. It cannot be taken without its parent commit; a cherry-pick would duplicate Echo's commits. I did not edit Echo's files.

## Proceeding meanwhile

Merging d53d42777, adding an end-to-end test (cargo stand-in in the fixture home's .cargo/bin, a pre-merge-commit that runs cargo check, land_merge run in a lease's configured environment: refused with no login environment, landed with it), then the rebuilt bundle, the VM walk and the proof run.

## How this reaches the lead

This file is the RECORD, not the delivery. The escalation was written to the
engine escalation ledger at `/Users/alex/.claude/state/escalations.jsonl` as `esc-20261021T091526Z-6987bba6`, which the
lead's session reads at session start and at every turn end WITHOUT this branch
being merged. If this file is never landed, the escalation still arrives.

Close it with:

    escalate.sh ack esc-20261021T091526Z-6987bba6 --disposition "<what you decided or did>"

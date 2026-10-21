# Escalation: folderfiles1: the brief's red premise is false, because on main Stu already reads a connected folder's CLAUDE.md or AGENTS.md itself and relays its rule

- id: `esc-20261021T191249Z-bbf10a3f`
- raised: 2026-10-21T19:12:49Z
- from: zach-opus-folderfiles1
- worktree: `/Users/alex/ab/richos-wt/zach-opus-folderfiles1` (branch `cc/zach-opus-folderfiles1`)
- head: `1bc7d2bf15ef33ea00f9bb0b61b9e2ab77d9620b`
- state: **proceeding**
- for: lead

## The question

None needed to finish. For the record: the brief and plan section 5.5 say the walk run on main is red because the rule is not followed. Measured on main's engine in the VM: Stu read the Tools folder's AGENTS.md with its own tools and wrote [agents-rule] into the worker's brief in its own words; the job settled, landed and the commit ended with [agents-rule]. Plan sections 1.2 and 2 (case 1, Ignored) describe only what is LOADED as instructions, not what Stu reads as a file. So the red I report is: on main neither the worker nor the reviewer received the folder's file (each agent's first transcript message lacks it) and the rule reaches them only as Stu chose to paraphrase it. The branch gives every worker and reviewer the file itself, in full and keeps it out of what the app hands Stu. Stu reading connected folders' files on its own is outside this change and unchanged.

## What was already tried

Red walk on main's engine (same bundle, engine payload with only this branch's runtime files reverted): Tools job settled with the rule followed through Stu; Acme job still running at 25 minutes because Stu first hired a worker and a reviewer; preservation PASS. Logs: /Volumes/E1TB/tmp/claude/zach-opus-folderfiles1/walk-red/ (copied into the handover report).

## Proceeding meanwhile

Merging main, rebuilding the bundle and engine at the merged SHA, then the green walk with 45 minutes per job, then the branch proof run.

## How this reaches the lead

This file is the RECORD, not the delivery. The escalation was written to the
engine escalation ledger at `/Users/alex/.claude/state/escalations.jsonl` as `esc-20261021T191249Z-bbf10a3f`, which the
lead's session reads at session start and at every turn end WITHOUT this branch
being merged. If this file is never landed, the escalation still arrives.

Close it with:

    escalate.sh ack esc-20261021T191249Z-bbf10a3f --disposition "<what you decided or did>"

# Escalation: Reap walk 10 reaches stop: the job's FOREGROUND command keeps running after Stop and the code is the same on main (Stop only sends stop_task to background tasks)

- id: `esc-20261021T224235Z-4a2d4e70`
- raised: 2026-10-21T22:42:35Z
- from: echo-opus-oneappb3
- worktree: `/Users/alex/ab/richos-wt/echo-opus-oneappb3` (branch `cc/echo-opus-oneappb3`)
- head: `e38f09404b8a9aa1452432601684e0839d087753`
- state: **proceeding**
- for: lead

## The question

Is a foreground command that runs on after the user presses Stop a product defect to fix and who fixes it? Or is the reap walk's stop step stale in expecting it to end? This branch does not change the behavior: stop_assignment (work_host.rs:1238) and stop_worker (native.rs:3571) are the same on main apart from the TaskBook path. native.rs:3571-3574: stop_worker looks up agent_tasks.task_for_tool(tool_use_id) and returns NotDelivered when the tool is not a background task, so a foreground Bash command is never stopped by Stop. The 2026-09-27 reap walk passed stop (richos-hq docs/verification/2026-09-27-reap-walk-commands/final-run/walk.log:52) under the older Stop that interrupted the turn.

## What was already tried

Walks 6-10 today after the earlier fixes, each a fix plus one walk. The walk fixes on this branch: f6a9bd9cf, 1d0f056eb, 15659f41a and 0338c32d9 (enter() waits for the home door, takes the conversation it opens and reads the crumb by its value), e38f09404 (stop opens the work summary by the chip's 'Open the work summary.' ending), 4c7870f98, 41d9b16c3 and dfff4d901 (a failed wait samples the app or pictures the screen, chosen by the last read). Walk 10 (walk-9ce771fa2ab1, bundle from 624f5ca24, planted operator.json, admitted at idle 30.5%): identity, review-watch, first-run, connect, seed, watch and unrelated PASS; stop FAIL: 'stop-fg (pid 3294) was still running after the trigger' (2000 ms bound, graded 3 s after the press). Guest cleanup: '[testvm] 22:40:36Z clean: app quit, VM stopped, clone deleted, state removed.' Walk 6 answered the earlier question: the relaunched app showed the right screen (home with its door, then the Acme / Onboarding conversation); the stale parts were the walk's.

## Proceeding meanwhile

Not walking again on this. Committing this record, then the proof run over the merge base..HEAD with --reuse of my earlier runs, then the handover. The leftover operator.json is inert as far as the walk reached: the app ran first-run, connected a folder, relaunched, took a job and ran its commands with the file in place, which the deleted reader would have refused.

## How this reaches the lead

This file is the RECORD, not the delivery. The escalation was written to the
engine escalation ledger at `/Users/alex/.claude/state/escalations.jsonl` as `esc-20261021T224235Z-4a2d4e70`, which the
lead's session reads at session start and at every turn end WITHOUT this branch
being merged. If this file is never landed, the escalation still arrives.

Close it with:

    escalate.sh ack esc-20261021T224235Z-4a2d4e70 --disposition "<what you decided or did>"

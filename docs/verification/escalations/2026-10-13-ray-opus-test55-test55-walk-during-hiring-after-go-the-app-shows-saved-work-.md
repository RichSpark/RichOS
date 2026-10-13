# Escalation: test55 walk: during hiring after go, the app shows 'Saved work is unavailable: A saved work record has an unsupported state.'

- id: `esc-20261013T203037Z-9b49483d`
- raised: 2026-10-13T20:30:37Z
- from: ray-opus-test55
- worktree: `/Users/alex/ab/richos-wt/ray-opus-test55` (branch `cc/ray-opus-test55`)
- head: `e441235196904d0525126d7aab7d02d42e6ab388`
- state: **proceeding**
- for: lead

## The question

Should the app's saved-work reader (richos-core work_status.rs, the status match around line 95) accept the engine's 'blocked' status, which mega-lander app.py:649 writes when a preparation never dispatched (commit 1b88dcb2e, already on main and in nightly 54)? Today one such record makes the whole Under-the-hood work list and the status line read 'Saved work unavailable' while the hiring is still running.

## What was already tried

Fresh guest, merged build e44123519 (voice55+onboard55+hire55+rclick55 on main 470845fbe), hire-walk.py. Identity, first-run, stock and interview PASSED. After 'Go.' Pierce refused the first Dean consult brief (receipt pierce_refused=true), the engine settled it status=blocked ('never dispatched'), a second Dean consult started and is running. From then the side panel says 'Saved work is unavailable: A saved work record has an unsupported state.' and the status line says 'Saved work unavailable - 1 assignment running'. Frame: /Volumes/E1TB/tmp/claude/ray-opus-test55/runs/hire-check-1.png; receipts copied to /Volumes/E1TB/tmp/claude/ray-opus-test55/runs/hire-receipts/. The reader gap is not from the four merged branches (it is on main).

## Proceeding meanwhile

Letting the hire walk finish (go, relaunch, roster, delegate, approvals), then the setup, right-click and voice walks. No code is changed by me.

## How this reaches the lead

This file is the RECORD, not the delivery. The escalation was written to the
engine escalation ledger at `/Users/alex/.claude/state/escalations.jsonl` as `esc-20261013T203037Z-9b49483d`, which the
lead's session reads at session start and at every turn end WITHOUT this branch
being merged. If this file is never landed, the escalation still arrives.

Close it with:

    escalate.sh ack esc-20261013T203037Z-9b49483d --disposition "<what you decided or did>"

# Escalation: test55: hire-walk cannot get past its relaunch step on this Mac today (4 of 4 runs), so 'still there after a relaunch' and 'Rich hands one of them a job' are NOT verified

- id: `esc-20261013T231904Z-2d7fa529`
- raised: 2026-10-13T23:19:04Z
- from: ray-opus-test55
- worktree: `/Users/alex/ab/richos-wt/ray-opus-test55` (branch `cc/ray-opus-test55`)
- head: `e441235196904d0525126d7aab7d02d42e6ab388`
- state: **proceeding**
- for: lead

## The question

Who changes hire-walk.py so its relaunch step outlasts the guest's post-relaunch load (wait until no whisper-cli runs and the home screen is passed before the 'Onboarding' click, and an ax() subprocess cap above TESTVM_AX_TIMEOUT), or should the hire relaunch/roster/delegate checks be done another way?

## What was already tried

Merged build e44123519, fresh guests, hire-walk.py. Runs 1, 3, 5: identity, first-run, stock, interview and go PASS (Rich proposes Clark, Dean, Frank, Pierce, Reed + Will and a bookkeeper; after 'Go.' both saved to team/, 0 Approve presses). All three then fail at the relaunch step on 'ax.sh click --title Onboarding' after the home screen door: exit 124 with the default 20 s deadline (runs 1, 3) and with TESTVM_AX_TIMEOUT=35 (run 5). Run 4 with TESTVM_AX_TIMEOUT=60 failed earlier in go because hire-walk's own ax() subprocess timeout is 40 s. Cause measured (esc-20261013T222951Z-d927933b): after every relaunch the guest runs 2-5 whisper-cli at 50-140% CPU plus the animated home screen, guest idle 0%; the same happens on nightly 54. Evidence: /Volumes/E1TB/tmp/claude/ray-opus-test55/runs/hire-run1, hire3, hire5.

## Proceeding meanwhile

Finishing the voice check (item 5) and the report; item 2 will be reported as partly verified.

## How this reaches the lead

This file is the RECORD, not the delivery. The escalation was written to the
engine escalation ledger at `/Users/alex/.claude/state/escalations.jsonl` as `esc-20261013T231904Z-2d7fa529`, which the
lead's session reads at session start and at every turn end WITHOUT this branch
being merged. If this file is never landed, the escalation still arrives.

Close it with:

    escalate.sh ack esc-20261013T231904Z-2d7fa529 --disposition "<what you decided or did>"

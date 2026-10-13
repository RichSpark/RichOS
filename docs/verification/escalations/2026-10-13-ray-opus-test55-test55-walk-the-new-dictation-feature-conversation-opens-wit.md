# Escalation: test55 walk: the new 'Dictation feature' conversation opens with the 'I don't know your business yet. Start the questions' card, while the interview is already running in Onboarding

- id: `esc-20261013T210016Z-20625b89`
- raised: 2026-10-13T21:00:16Z
- from: ray-opus-test55
- worktree: `/Users/alex/ab/richos-wt/ray-opus-test55` (branch `cc/ray-opus-test55`)
- head: `e441235196904d0525126d7aab7d02d42e6ab388`
- state: **proceeding**
- for: lead

## The question

Should the business-questions card be kept out of the automatically created 'Dictation feature' conversation (at least while the onboarding interview has started in 'Onboarding')? As a regular user I had just pressed 'Start the questions' and Rich had asked his first question; opening 'Dictation feature' shows the card again above the CEO's 'Good news! Dictation is set up and enabled.' text, inviting a second interview.

## What was already tried

Fresh guest, merged build e44123519, setup-walk.py identity,r19-grants,r19-sheet,r19-set-up: Start, company 'Brightwave LABS', Start the questions, Turn on dictation, F2 picked, sheet closed. Everything the CEO asked for passed (his offer copy, his 'Good news' copy with tap F2, F2 saved, text moved out, no line under the message box, 'Onboarding' title, name case kept in rail and in Rich's words). The extra card is visible in /Volumes/E1TB/tmp/claude/ray-opus-test55/runs/setup/set-up-feature.png.

## Proceeding meanwhile

Continuing the right-click, hire and voice walks. No code changed by me.

## How this reaches the lead

This file is the RECORD, not the delivery. The escalation was written to the
engine escalation ledger at `/Users/alex/.claude/state/escalations.jsonl` as `esc-20261013T210016Z-20625b89`, which the
lead's session reads at session start and at every turn end WITHOUT this branch
being merged. If this file is never landed, the escalation still arrives.

Close it with:

    escalate.sh ack esc-20261013T210016Z-20625b89 --disposition "<what you decided or did>"

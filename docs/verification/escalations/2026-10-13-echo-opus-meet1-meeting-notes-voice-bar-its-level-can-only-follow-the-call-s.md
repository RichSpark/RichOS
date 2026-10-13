# Escalation: Meeting notes voice bar: its level can only follow the call's voices by listening to a call the CEO declined

- id: `esc-20261013T090538Z-8defdf42`
- raised: 2026-10-13T09:05:38Z
- from: echo-opus-meet1
- worktree: `/Users/alex/ab/richos-wt/echo-opus-meet1` (branch `cc/echo-opus-meet1`)
- head: `44e76bb2beba04d50f1a423ffaa36a3e11c568db`
- state: **proceeding**
- for: ceo

## The question

Round 25.1's voice bar (the prompt put aside after Not this one) shows a gold level that 'follows the voices on the call'. In the real app the only way to get that level is to read the call's audio through the system-audio tap while the CEO has said Not this one, and PRD V4 says a dismissed prompt captures nothing (nothing would be stored, only a level computed). Should the bar's level (a) stay still at its resting height, or (b) move with the call's sound, read live and never stored, once he has granted system-audio access?

## What was already tried

Built the bar exactly as drawn except the level, which stands at the round's 6% resting height (richos app/ui/meeting-panel.js header says why). No other source of a call's level exists without a tap: Core Audio reports which process is using audio, not how loud.

## Proceeding meanwhile

Proceeding with the rest of the meeting-notes workflow on branch cc/echo-opus-meet1 (v1.3.0 work); (a) is what is built until he answers.

## How this reaches the lead

This file is the RECORD, not the delivery. The escalation was written to the
engine escalation ledger at `/Users/alex/.claude/state/escalations.jsonl` as `esc-20261013T090538Z-8defdf42`, which the
lead's session reads at session start and at every turn end WITHOUT this branch
being merged. If this file is never landed, the escalation still arrives.

Close it with:

    escalate.sh ack esc-20261013T090538Z-8defdf42 --disposition "<what you decided or did>"

# Escalation: Spoken-answer delay: most of it is Opus high-effort thinking (CEO 119), not RichOS waiting

- id: `esc-20261014T071924Z-46bcc4c7`
- raised: 2026-10-14T07:19:24Z
- from: echo-opus-replylag55
- worktree: `/Users/alex/ab/richos-wt/echo-opus-replylag55` (branch `cc/echo-opus-replylag55`)
- head: `6c4836750cf569118ba784382fc50e9671b4b150`
- state: **proceeding**
- for: ceo

## The question

On a spoken turn, Rich thinks for about 8.7 s before his first word because every lease runs at --effort high (CEO 119, native.rs LEASE_EFFORT). May spoken (voice) turns run at a lower effort so Rich answers sooner, or does high effort stay for voice too?

## What was already tried

VM voice walk on main 0a2ae649f (walk-a0fe8cc9cd75, 2026-10-14 07:18Z), split from the app's own ledger and machinery journal: his turn handed over 07:18:12.872Z -> his words reach the model 07:18:16.136Z (3.264 s: 1.073 s engine bind/brief + a 1.949 s hidden context-only model turn + 0.228 s) -> model thinking (seven system:thinking_tokens frames, 07:18:17.518Z to 07:18:24.958Z) -> first text 07:18:25.003Z (8.867 s after his words reached the model) -> first sentence to the voice 07:18:25.078Z -> first sample 07:18:25.536Z. Total 12.664 s. The 3.2 s that is RichOS's own waiting (the hidden context turn on the first turn after priming) is fixed on branch cc/echo-opus-replylag55 (commit a01f942af). The model, its effort and the voice were not changed, per the brief.

## Proceeding meanwhile

Measuring the fix with one after-walk; handing over the fix and the split. Effort is left at high until the CEO decides.

## How this reaches the lead

This file is the RECORD, not the delivery. The escalation was written to the
engine escalation ledger at `/Users/alex/.claude/state/escalations.jsonl` as `esc-20261014T071924Z-46bcc4c7`, which the
lead's session reads at session start and at every turn end WITHOUT this branch
being merged. If this file is never landed, the escalation still arrives.

Close it with:

    escalate.sh ack esc-20261014T071924Z-46bcc4c7 --disposition "<what you decided or did>"

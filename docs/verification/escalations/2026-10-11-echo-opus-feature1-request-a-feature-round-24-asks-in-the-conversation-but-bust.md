# Escalation: Request a feature: round 24 asks in the conversation, but Bust a bug is panel-only since the CEO's 2026-10-10 order; which does the idea follow?

- id: `esc-20261011T131928Z-e14f68b1`
- raised: 2026-10-11T13:19:28Z
- from: echo-opus-feature1
- worktree: `/Users/alex/ab/richos-wt/echo-opus-feature1` (branch `cc/echo-opus-feature1`)
- head: `ba6a6a47f2e7019c3520c73091e3f86b217f7aeb`
- state: **proceeding**
- for: lead

## The question

Review note (1) says to ask in the conversation 'exactly as Bust a bug does'. Bust a bug does not: commit 8e454cff1 (CEO 2026-10-10, 'pressing Bust a bug always shows the bug report on top, ready to answer') moved every Bust a bug exchange to the Rich panel and removed the in-conversation path, after reviews found it colliding with Dictation's key capture, conversation voice mode and focus. Round 24 (drawn from round 21, before that order) asks in the conversation. Should Request a feature (a) follow round 24 and bring back an in-conversation exchange for ideas only, or (b) stay in the Rich panel like Bust a bug, as built at 2ed789b6d? The brief also says copy Bust a bug.

## What was already tried

Built (b): the idea uses the same panel, card and states as Bust a bug, so every input-collision fix since 2026-10-10 applies to it unchanged. Bringing back (a) means re-adding the conversation composer intercept and voice hold that 8e454cff1 removed, for ideas only, so the two buttons would behave differently.

## Proceeding meanwhile

Fixing review note (2) (Try now with a changed reason says the new reason, for both kinds), and walking the built app in the test VM with the panel as it is. If the answer is (a), I build it next on this branch.

## How this reaches the lead

This file is the RECORD, not the delivery. The escalation was written to the
engine escalation ledger at `/Users/alex/.claude/state/escalations.jsonl` as `esc-20261011T131928Z-e14f68b1`, which the
lead's session reads at session start and at every turn end WITHOUT this branch
being merged. If this file is never landed, the escalation still arrives.

Close it with:

    escalate.sh ack esc-20261011T131928Z-e14f68b1 --disposition "<what you decided or did>"

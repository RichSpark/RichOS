# Escalation: Hunt 57 finding 19: the brief asks to reword 'Connected folders (repositories)', which is the CEO's own copy from 2026-10-06

- id: `esc-20261015T102219Z-e4c2dd8f`
- raised: 2026-10-15T10:22:19Z
- from: echo-opus-words58
- worktree: `/Users/alex/ab/richos-wt/echo-opus-words58` (branch `cc/echo-opus-words58`)
- head: `4443cfaad78097a8946c423927c0c2bf94b1ab3b`
- state: **proceeding**
- for: lead

## The question

Finding 19 lists 'Connected folders (repositories)' (Settings row) as machinery to reword. That label, and the sheet headline 'Connected project folders (repositories)', are word for word the CEO's own copy: richos-hq docs/ceo-input/2026-10-06_01/feedback.md lines 19 and 31 ('Replace "Connected repositories" with "Connected folders (repositories)" in the settings menu', and his copy table's Headline). Does his 2026-10-15 'fix all those findings' replace his own 10-06 wording here (then: 'Connected folders' / 'Connected project folders', one-line change each), or does his 10-06 copy stand?

## What was already tried

Read the brief (keep wording the approved round-23 design shows; the folders label is not in round 23, so the brief does not protect it), the CEO feedback file above, and the code: ui/settings-button.js:523 and ui/repositories.js:14. Related, decided by the brief and done: the Codex review switch is now hidden while it cannot be turned on, although the approved round 20.2 drew that disabled state; its wording is unchanged.

## Proceeding meanwhile

Keeping the CEO's own folders wording unchanged (no commit touches it). Everything else in findings 11, 12 and 19 is being fixed and committed, one commit per finding; the folders change is a one-line follow-up either way.

## How this reaches the lead

This file is the RECORD, not the delivery. The escalation was written to the
engine escalation ledger at `/Users/alex/.claude/state/escalations.jsonl` as `esc-20261015T102219Z-e4c2dd8f`, which the
lead's session reads at session start and at every turn end WITHOUT this branch
being merged. If this file is never landed, the escalation still arrives.

Close it with:

    escalate.sh ack esc-20261015T102219Z-e4c2dd8f --disposition "<what you decided or did>"

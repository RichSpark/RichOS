# Escalation: walk48fix14: one stacking rule fixes Dictation, but over four backdrops a new report never reaches the Rich panel

- id: `esc-20261010T145439Z-2c9af588`
- raised: 2026-10-10T14:54:39Z
- from: echo-opus-walk48fix14
- worktree: `/Users/alex/ab/richos-wt/echo-opus-walk48fix14` (branch `cc/echo-opus-walk48fix14`)
- head: `279943ec66450c60239bf1bcf654d7c4334c978f`
- state: **proceeding**
- for: lead

## The question

The brief's proof (a new report over EVERY overlay tests/lib/overlays.js finds is on top in the panel) needs one more rule than the brief allows: over #slideover-backdrop, #inspector-scrim, #outpanel-scrim and #rail-scrim (the narrow-window side panes, under 1180px) the report goes to the CONVERSATION behind the backdrop, not to the panel, so no stacking rule can lift it. Keep the second, separate commit that sends a new report to the panel whenever the conversation's answer box is not what is painted at its place, or drop that commit and land the stacking rule alone?

## What was already tried

Measured on 26a0213b5 with the new check in tests/bug-report.js: Dictation opened from Settings puts the panel UNDER the sheet (the review's finding, red). The same run shows the report going to the conversation over the four backdrops, COVERED by the backdrop over three of them. The stacking rule (#bugdock z-index 298, the raise class and covered() deleted) fixes Dictation and every dialog; it cannot touch where the report is placed.

## Proceeding meanwhile

Committing the stacking rule and its check first (green on its own, the conversation cases named in the result, not hidden). Then the placement rule as its own commit, with the check tightened to require the panel over every overlay. Either can be landed without the other: drop the second commit to keep the brief's exact scope.

## How this reaches the lead

This file is the RECORD, not the delivery. The escalation was written to the
engine escalation ledger at `/Users/alex/.claude/state/escalations.jsonl` as `esc-20261010T145439Z-2c9af588`, which the
lead's session reads at session start and at every turn end WITHOUT this branch
being merged. If this file is never landed, the escalation still arrives.

Close it with:

    escalate.sh ack esc-20261010T145439Z-2c9af588 --disposition "<what you decided or did>"

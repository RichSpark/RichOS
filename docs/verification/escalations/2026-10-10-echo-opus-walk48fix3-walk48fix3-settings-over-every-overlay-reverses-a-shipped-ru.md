# Escalation: walk48fix3: Settings over every overlay reverses a shipped rule that the first-run questions make Settings unreachable

- id: `esc-20261010T084749Z-19605156`
- raised: 2026-10-10T08:47:49Z
- from: echo-opus-walk48fix3
- worktree: `/Users/alex/ab/richos-wt/echo-opus-walk48fix3` (branch `cc/echo-opus-walk48fix3`)
- head: `1f1717179d71194d5c17d033d205c8ed8db6c906`
- state: **proceeding**
- for: lead

## The question

The brief says every aria-modal or window-covering overlay keeps Settings on top and reachable. Eight shipped sheets match main.js QUESTION_SELECTOR (.overlay[aria-modal=true][data-dismiss^=control:]) and inert everything outside them, Settings included: #setup-sheet, #memory-setup (the two first-run questions, Ray's candidate .16 audit row B1b, asserted by ui/tests/setup.js check 20: set-btn refused, menu does not open), and also the Settings-opened sheets #accounts-sheet, #quota-sheet, #phone-sheet, #repositories-sheet, #dictation-sheet and #quit-question. I am building to the brief and the CEO's section 15 (Settings on every screen, above every overlay; the only declared exception is the running splash, section 62): no overlay is aria-modal, the question walk spares Settings and the Bust a bug panel, and setup.js check 20 changes its Settings half from refused to reachable (composer and rail stay unreachable). If B1b should win for the two first-run questions only, say so and the check gets one declared, checked exemption for them instead.

## What was already tried

Searched richos-hq wiki/ceo-decisions.md for a CEO ruling behind B1b or 'a question owns the window': none found; the rule's only source is Ray's audit, quoted in main.js and setup.js. bug-report.js:134 also says 'while one is up, Settings cannot be pressed'.

## Proceeding meanwhile

Proceeding with Settings reachable over every overlay including the two first-run questions; the attribution fix is already committed (1f1717179).

## How this reaches the lead

This file is the RECORD, not the delivery. The escalation was written to the
engine escalation ledger at `/Users/alex/.claude/state/escalations.jsonl` as `esc-20261010T084749Z-19605156`, which the
lead's session reads at session start and at every turn end WITHOUT this branch
being merged. If this file is never landed, the escalation still arrives.

Close it with:

    escalate.sh ack esc-20261010T084749Z-19605156 --disposition "<what you decided or did>"

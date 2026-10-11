# Escalation: walk48fix20: contrast.js section 9 premise is false; nothing hides the sheets, the floors were set on an inert-shell artifact

- id: `esc-20261010T172009Z-3e3d762f`
- raised: 2026-10-10T17:20:10Z
- from: echo-opus-walk48fix20
- worktree: `/Users/alex/ab/richos-wt/echo-opus-walk48fix20` (branch `cc/echo-opus-walk48fix20`)
- head: `d63d9e3d3f66e5b3f2ac688fd15c848b3dc94357`
- state: **proceeding**
- for: lead

## The question

May I re-derive the contrast-debt.json floors of the 15 failing section-9 surfaces (accounts-sheet, repositories, quota-offer/approved/uncertain, phone-* x10, quit-question) from a run on this branch? The brief says fix the cause, never the floors, and the only cause-side fix is putting aria-modal back on those sheets, which makes Settings (and so Bust a bug) unreachable over them again (undoes 26a0213b5 / D24).

## What was already tried

Measured, not assumed. The sheets are still walked and measured: repositories measures 20 nodes on the branch, the same sheet nodes as before (considered 738 on both, from the autocheck logs: main-era runs show checked 118 considered 738). The ~98 missing nodes are the SHELL BEHIND the sheet. On main these sheets declared aria-modal with a control data-dismiss, so main.js QUESTION_SELECTOR set inert on #app, #home, .settings etc. Inert content is not hit-testable, so lib/contrast.js stackFor() found the element absent from elementsFromPoint and fell back to viaAncestors, measuring shell text as if no scrim were over it. Probe (scratch probe-inert.js, Playwright WebKit, repositories sheet open): branch -> aria-modal null, inert [], a shell label Search is index 1 under repositories-sheet in elementsFromPoint (filed obscured). Same tree with aria-modal put back on repositories.js -> inert [#app, #home, .settings, ...], the label is absent from elementsFromPoint (index -1) and is measured through ancestors. Corroboration: Corrections, Feedback and Search were never inerted (data-dismiss=escape) and pass with their floors (corrections 46, feedback 22), set with the shell obscured. Check 11 still holds: every obscured shell node is measured on the shell surface. Failing log: /Volumes/E1TB/state/richos/proof-runs/autocheck/60e53bf487d588c6bd6f0d49760bdfa9135182f96da7a4924ce2fcf80dd88507/attempt-2ftg7yru/01-contrast.js.log

## Proceeding meanwhile

Building the one-place Bust a bug design, bug-report.js test and the affordances fix (cherry-picked 7462be184). contrast.js floors are left untouched as instructed; section 9 will stay red on those 15 surfaces until you answer.

## How this reaches the lead

This file is the RECORD, not the delivery. The escalation was written to the
engine escalation ledger at `/Users/alex/.claude/state/escalations.jsonl` as `esc-20261010T172009Z-3e3d762f`, which the
lead's session reads at session start and at every turn end WITHOUT this branch
being merged. If this file is never landed, the escalation still arrives.

Close it with:

    escalate.sh ack esc-20261010T172009Z-3e3d762f --disposition "<what you decided or did>"

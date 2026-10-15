# Escalation: batch58b tip 7eb7fc7f4: 2 UI suites still red, both from convo58c alone (affordances.js, memory-strategy.js); not fixed, outside my widened brief

- id: `esc-20261015T144812Z-e2c31fe6`
- raised: 2026-10-15T14:48:12Z
- from: zach-opus-batch58b
- worktree: `/Users/alex/ab/richos-wt/zach-opus-batch58b` (branch `cc/zach-opus-batch58b`)
- head: `7eb7fc7f435e62b89808a18cf5d68413ad6fafb7`
- state: **work-complete**
- for: lead

## The question

Who fixes these two convo58c UI failures before the nightly? (1) affordances.js: 5 new user-visible strings convo58c added are not classified in ui/tests/lib/state-registry.js: "I couldn't start that thread just now. Press" / "to try again." (main.js:2284), "again when the setting up is done." (main.js:2394), "when you want me to try again." (main.js:2395), "What Rich did" (timeline.js:2775). Each needs a registry row (actionable ones need a control in the same view). (2) memory-strategy.js: shots-26 ms-01..ms-05 each differ by 224 px at (1150-1181, 794-809): the working band's "18s" timer to the right of "Rich is working" is GONE in the new render, while the timeline above still says "Working for 18s". That looks like convo58c's finding-21 timer change hiding the band timer on a turn that is not answered: an intended baseline change or a regression? I did not regenerate the baselines.

## What was already tried

UI selection (73 suites from proof-for main..HEAD, plus cargo test --bin richos-tauri) run on the final tip 7eb7fc7f4 via proof-run.py: 72 of 74 pass, tauri 618 passed. Run log: /Volumes/E1TB/state/richos/proof-runs/6b6895047bb4/20261015T142821Z-1eff0sdh. Same two failures in the earlier run on 8d0d543e5, before my timeline change, so they are not mine. waitline58 changes only assignment.rs, work_host.rs and src-tauri main.rs, no UI or mock file, so they are not caused by merging the two branches either. All 5 flagged strings appear only on convo58c (git grep: main 1 match, convo58c 5).

## Proceeding meanwhile

Handing over the tip 7eb7fc7f4 with these two failures stated, not fixed. cargo test -p richos-core on the tip: lib and all integration targets green (timeline_tests rerun alone after the amend: 15 passed).

## How this reaches the lead

This file is the RECORD, not the delivery. The escalation was written to the
engine escalation ledger at `/Users/alex/.claude/state/escalations.jsonl` as `esc-20261015T144812Z-e2c31fe6`, which the
lead's session reads at session start and at every turn end WITHOUT this branch
being merged. If this file is never landed, the escalation still arrives.

Close it with:

    escalate.sh ack esc-20261015T144812Z-e2c31fe6 --disposition "<what you decided or did>"

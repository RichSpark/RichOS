# Escalation: reuse7: the engine suites cannot be given read lists from what exists; a pass can stand where the selector says a change does not reach it (decision needed)

- id: `esc-20261022T054303Z-e2b6f41e`
- raised: 2026-10-22T05:43:03Z
- from: zach-opus-reuse7
- worktree: `/Users/alex/ab/richos-wt/zach-opus-reuse7` (branch `cc/zach-opus-reuse7`)
- head: `1cc3ce2960e6c2db7ceadfb8ad3bca2c6ee5cdb7`
- state: **proceeding**
- for: lead

## The question

The brief assumes each engine suite can be given its real read list cheaply (Hank sized it at 2-4 lines a suite). Measured, that premise is false. The dependency graph says of itself that its edges are executed/source relationships, not every file read (verification_inputs.py Dependencies docstring; resolve-roots.sh's row reads plugin.json and agent frontmatter as data that are not edges). In holds3's run 4 (20261022T040637Z-yh9qti0r), all 66 engine units the graph knows run code that reads paths it does not name (globs, git ls-files/diff/status, os.walk, cp -R: resolve-roots.sh in 54 of them, operator_fences.py in 54, workspaces.py in 50) and 14 more are not in the graph at all. So a derived read list is an inference from an absent grep match, which the engine's own selector refuses to make. Two sound ways remain. (1) A pass saved at commit A stands at commit B when everything else in its identity is unchanged and the selector, which already decides what a land runs, maps A..B cleanly and does not select that check: the same standard by which a land of A..B alone would skip it. Measured on holds3: proof-for.sh 6e48116e8..6259cc3f5 selects 9 engine units, make-engine-asset and 7 run-tests checks, so run 4 would have executed about 17 of its 94 checks instead of 94. It would also cover the gate-after-main-moved case in this same escalation's predecessor. (2) A reviewed read contract per suite through the existing qualification process: sound to the current standard, but about 80 reviews, not a few lines each. Which do you want: (1), (2) or (1) now with (2) for the costliest suites later?

## What was already tried

Derived read sets from the R1 graph plus every file a graph file names: the three holds3 files still reach 65 of 66 graph units (closures of 300-900 files) and the graph does not claim to list reads in any case. Scanned each unit's executed files for constructs that read unnamed paths: 0 of 80 units are free of them. Checked runtime measurement: no tracer works on /bin/bash children without root on this Mac; sandbox-exec would need nested sandboxes (review_watch.py already uses one) and git-metadata exceptions, feasibility unproven.

## Proceeding meanwhile

Done and committed: PID 9650 stopped (start, parent, group and command matched first) and the EndOwned loop now ends once its test process is gone (1cc3ce296, test red before, green after). Proceeding on (1) as the default on this branch, with tests that a selected check still runs and an unselected one is reused and the holds3 replay. If you choose (2), the branch is not landed as is.

## How this reaches the lead

This file is the RECORD, not the delivery. The escalation was written to the
engine escalation ledger at `/Users/alex/.claude/state/escalations.jsonl` as `esc-20261022T054303Z-e2b6f41e`, which the
lead's session reads at session start and at every turn end WITHOUT this branch
being merged. If this file is never landed, the escalation still arrives.

Close it with:

    escalate.sh ack esc-20261022T054303Z-e2b6f41e --disposition "<what you decided or did>"

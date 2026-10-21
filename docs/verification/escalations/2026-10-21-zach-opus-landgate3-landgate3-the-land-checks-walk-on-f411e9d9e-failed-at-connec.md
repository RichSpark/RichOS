# Escalation: landgate3: the land-checks walk on f411e9d9e failed at connect again: with the 15 s wait from d093d39a6 the chooser's Open never became enabled

- id: `esc-20261021T204817Z-14118fbb`
- raised: 2026-10-21T20:48:17Z
- from: zach-opus-landgate3
- worktree: `/Users/alex/ab/richos-wt/zach-opus-landgate3` (branch `cc/zach-opus-landgate3`)
- head: `f411e9d9ee1d88cc2b7cbb6ef6159bf00fccdedf`
- state: **proceeding**
- for: lead

## The question

Two walks have now failed at the shared connect_folder step (adopt-walk.py), so it is not timing: after Go to folder + the path + Return, the chooser never reached a choosable folder (Open enabled=False for the full 15 s). The step captures no screenshot or AX tree at that point, so no cause can be named from this run. Who takes the shared step: its owner (echo-sonnet-connectfix1 / walkfix1) adds a screenshot and AX tree at that failure and finds why the chooser does not take the path or do you want me to add that capture on my branch and walk once more?

## What was already tried

Walk walk-eb9f006fb025 on bundle 1.2.2-dev.f411e9d9e (includes d093d39a6 and 060cbab4b): identity PASS 1.1 s, team PASS, first-run PASS 40.9 s, fixture PASS 6.3 s, connect FAIL 51.0 s with: connect folder: the chooser's Open button was not enabled within 15 s (enabled=False). Not the 20 s deadline. Guest cleaned: 'clean: app quit, VM stopped, clone deleted, state removed.' Not rerun. Evidence: /Volumes/E1TB/state/richos/verification/zach-opus-landgate3-walk-eb9f006fb025 (and walk-9696d0b3a4f1 for the first). land-checks-walk.py itself makes no AX reads; it inherits connect from adopt_walk.Walk via ReviewWalk/CommandWalk.

## Proceeding meanwhile

Running the proof run over merge-base(main,HEAD)..HEAD with --reuse of both earlier run directories, then handing over with the walk recorded as FAILED at connect.

## How this reaches the lead

This file is the RECORD, not the delivery. The escalation was written to the
engine escalation ledger at `/Users/alex/.claude/state/escalations.jsonl` as `esc-20261021T204817Z-14118fbb`, which the
lead's session reads at session start and at every turn end WITHOUT this branch
being merged. If this file is never landed, the escalation still arrives.

Close it with:

    escalate.sh ack esc-20261021T204817Z-14118fbb --disposition "<what you decided or did>"

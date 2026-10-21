# Escalation: landgate3: the land-checks walk on 0bc8f16be failed at connect: the shared connect_folder step clicked the chooser's Open while it was disabled

- id: `esc-20261021T191731Z-ac12598b`
- raised: 2026-10-21T19:17:31Z
- from: zach-opus-landgate3
- worktree: `/Users/alex/ab/richos-wt/zach-opus-landgate3` (branch `cc/zach-opus-landgate3`)
- head: `0bc8f16be7d38a2b0ceab1b34f95b0accc26f1ad`
- state: **proceeding**
- for: lead

## The question

The failure is in the shared step every walk uses (adopt-walk.py connect_folder, lines 198-200: Return in Go to folder, a fixed 2.5 s sleep, then click Open with no check that Open is enabled). Shall I make that one step wait (bounded, about 15 s) for Open to be enabled before the click, on my branch and walk once more? Or does the owner of that step (455193c1a, echo-sonnet-walkfix1) fix it on main first?

## What was already tried

One walk, run-walk.py on bundle 1.2.2-dev.0bc8f16be (walk-9696d0b3a4f1): identity PASS, team PASS, first-run PASS, fixture PASS (fence on, positive control REFUSED), connect FAIL. Exact line: ax.sh walk-9696d0b3a4f1 click --title Open --role AXButton --in dialog --first --json -> {"error": "disabled", "detail": "matched element is disabled"}. Not the 20 s deadline (the read took 1.7 s). The guest was cleaned: 'clean: app quit, VM stopped, clone deleted, state removed.' Not rerun.

## Proceeding meanwhile

Running the proof run over merge-base(main,HEAD)..HEAD (4b1454781..0bc8f16be) with --reuse /Volumes/E1TB/state/richos/proof-runs/2dba54c05837/20261021T165438Z-3bbcracc, as the coordinator directed. No change to the shared step until you answer.

## How this reaches the lead

This file is the RECORD, not the delivery. The escalation was written to the
engine escalation ledger at `/Users/alex/.claude/state/escalations.jsonl` as `esc-20261021T191731Z-ac12598b`, which the
lead's session reads at session start and at every turn end WITHOUT this branch
being merged. If this file is never landed, the escalation still arrives.

Close it with:

    escalate.sh ack esc-20261021T191731Z-ac12598b --disposition "<what you decided or did>"

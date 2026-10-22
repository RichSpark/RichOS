# Escalation: Andreas issue 4 (machinery rows): the screenshots show Technical view was switched ON after the Settings shot, not off

- id: `esc-20261021T145551Z-e1ab3851`
- raised: 2026-10-21T14:55:51Z
- from: echo-opus-andreasupd1
- worktree: `/Users/alex/ab/richos-wt/echo-opus-andreasupd1` (branch `cc/echo-opus-andreasupd1`)
- head: `70667c2b977522c96bb87fd97eaa035b3567e4a7`
- state: **proceeding**
- for: lead

## The question

Can we get one file from Andreas's Mac to settle it: ~/Library/Application Support/com.richos.app/config.json (the keys techy_default, techy_entities, techy_threads)? If any is true, the rows were the Technical view he turned on, not a defect.

## What was already tried

Read the 1.2.1 code (tag v1.2.1 = 361da6d16, UI and timeline code identical to main for this path). The rows in his screenshots (control_request, system:init, 'outcome not recorded' with a chevron and the 'Between turns' section) are drawn only from get_machinery, which ui/main.js loadTimeline calls only when techyOn() is true (main.js ~2502-2509); renderBetweenTurns fills the section only from that payload (main.js ~9215). Rust removes them from the user view by construction (timeline.rs Timeline::view, machinery_visibility, redacted). Screenshot order: 12:46:53 the same conversation end has NO Between turns section (the app shows that section whenever the view is on); 12:47:45 Settings shows the switch off; 12:49:02 and 12:49:15 show the rows. So the view went on between 12:47:45 and 12:49:02. Every way to turn it on asks where and needs a confirm (Settings switch, rail switch, chip or Cmd-Shift-T then Enter). The brief's premise 'although Settings shows Technical view off' is drawn from a shot taken before the rows appeared.

## Proceeding meanwhile

Finishing issues 3 and 5 and the class docs; for issue 4 I am adding the real-app check (Technical view off shows no machinery rows, on shows them) and the class doc and not inventing a code change for a cause the code cannot produce.

## How this reaches the lead

This file is the RECORD, not the delivery. The escalation was written to the
engine escalation ledger at `/Users/alex/.claude/state/escalations.jsonl` as `esc-20261021T145551Z-e1ab3851`, which the
lead's session reads at session start and at every turn end WITHOUT this branch
being merged. If this file is never landed, the escalation still arrives.

Close it with:

    escalate.sh ack esc-20261021T145551Z-e1ab3851 --disposition "<what you decided or did>"

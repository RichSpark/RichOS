# Escalation: zach-opus-loops1: ending owned commands at SubagentStop contradicts the engine's own deliberate end-deferral (record_end, 2026-10-04 panel16 rule)

- id: `esc-20261011T032337Z-91311023`
- raised: 2026-10-11T03:23:37Z
- from: zach-opus-loops1
- worktree: `/Users/alex/ab/richos-wt/zach-opus-loops1` (branch `cc/zach-opus-loops1`)
- head: `d4e069db15d3daa6fca1aeb15a44ed9bf4766b06`
- state: **proceeding**
- for: lead

## The question

The brief's premise is false: the platform DID fire SubagentStop for echo-opus-walk49fix1 at 00:17:17Z, 00:38:11Z and 00:38:17Z (registry events.jsonl, event end-deferred, signal SubagentStop). The engine itself refused to record the end, on purpose: workspaces.py record_end() says a SubagentStop while the agent's own command still runs is NOT its end, because the platform wakes the agent when that command finishes (echo-opus-panel16, 2026-10-04: an agent that ended its turn to await its own background timer was sealed out and lost its evidence). Ending owned commands at every SubagentStop would kill exactly that awaited command. Which do you want? (A) SubagentStop ends the agent's owned background commands always, and the deferral is dropped: an agent must await its own commands with agent_hold.py wait inside its run (the rewrite context already tells it to before its final reply), so ending a turn to be woken by its own command stops working. (B) Keep the deferral; owned commands are ended only when the end is actually recorded (TaskStop / stoppedByUser / a SubagentStop with nothing running). Then for the walk49fix1 case Rich's way out is one TaskStop of the agent instead of finding PIDs by hand, but a forgotten loop still keeps the end deferred until someone stops it. (C) Something else you name.

## What was already tried

Read agent_hold.py (ownership record and owned_tree confirmed as the brief says), workspaces.py record_end/_running_calls/finished_state, the registry events for walk49fix1 and its transcript: the 00:38 wakes were task notifications from the loops being killed, each followed by a deferred SubagentStop until the last loop died at 00:41:03Z. Also: the loops watched app-run.log and x-07-done.png in the deleted scratch folder, not build.log (two of the deferred calls did watch build.log).

## Proceeding meanwhile

Building (B), which every option needs: agent_hold.end_owned() ends the owned tree with the pause's exclusions (shared daemons, test VM guest) and logs what it ended, called wherever the registry records an agent's end. The SubagentStop deferral is left exactly as it is until you answer; switching to (A) is then a few lines in record_end.

## How this reaches the lead

This file is the RECORD, not the delivery. The escalation was written to the
engine escalation ledger at `/Users/alex/.claude/state/escalations.jsonl` as `esc-20261011T032337Z-91311023`, which the
lead's session reads at session start and at every turn end WITHOUT this branch
being merged. If this file is never landed, the escalation still arrives.

Close it with:

    escalate.sh ack esc-20261011T032337Z-91311023 --disposition "<what you decided or did>"

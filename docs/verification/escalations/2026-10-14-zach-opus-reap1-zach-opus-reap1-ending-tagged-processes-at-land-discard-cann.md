# Escalation: zach-opus-reap1: ending tagged processes at land/discard cannot fire for the echo-opus-tap22b case; the gap is the deferred end, not land

- id: `esc-20261014T102810Z-f59b0c74`
- raised: 2026-10-14T10:28:10Z
- from: zach-opus-reap1
- worktree: `/Users/alex/ab/richos-wt/zach-opus-reap1` (branch `cc/zach-opus-reap1`)
- head: `01381affc5429f3a7fd5882359d27f84a2ca348d`
- state: **stopped**
- for: lead

## The question

Which one should I build? (A) When a teammate's turn ends (SubagentStop) and its only running commands are its own background calls that have run longer than N minutes, record its end and end those commands with end_owned(); or (B) the stall watcher runs agent_hold.py end by itself instead of only naming the command when an agent's end has been deferred and it has been silent for N minutes. Both would also end a legitimately long background command the agent is still waiting on, so N is the trade-off.

## What was already tried

Read the code in /Users/alex/ab/richos-wt/zach-opus-reap1 (richos engine). (1) workspaces.py land() line 5086 and discard() line 5840 refuse an agent that finished_state() (line 1407) does not call finished. (2) record_end() lines 2148-2163: a SubagentStop while the agent's own calls still run is DEFERRED, so rec.end is not set and the agent is not finished. (3) Registry record ~/.claude/state/workspaces/agents/01308d06-...--echo-opus-tap22b.json: history shows end-deferred at 10:14:43Z (3 calls: toolu_0139n24..., toolu_014U95..., toolu_01RVrA...) and 10:21:45Z (toolu_0139n24...), and end was recorded only at 10:21:52Z after the loops were ended by hand. So between those times land and discard would have refused it, and code added to them would never have run. (4) end_owned_work() is ALREADY called at every recorded end: record_end line 2175, observe_platform_end line 2401, stop() line 2735. discard requires a stop or a recorded end first, so it is already covered. land() also already runs stop_processes() on processes whose cwd is in the agent's workspaces, and the native worktree is one of tap22b's recorded workspaces. Adding the call to land/discard would only cover an agent finished by its session ending, which is not this incident. (5) The same class happened 2026-10-11 (echo-opus-walk49fix1); the response then was stall_watch.py lines 648-728, which only NAMES agent_hold.py end for the lead to run by hand. That is exactly the step the CEO wants gone.

## Proceeding meanwhile

Nothing committed. I did not build the land/discard version because it would not stop a repeat of this incident, and the brief says to report and stop when the code contradicts its premise.

## How this reaches the lead

This file is the RECORD, not the delivery. The escalation was written to the
engine escalation ledger at `/Users/alex/.claude/state/escalations.jsonl` as `esc-20261014T102810Z-f59b0c74`, which the
lead's session reads at session start and at every turn end WITHOUT this branch
being merged. If this file is never landed, the escalation still arrives.

Close it with:

    escalate.sh ack esc-20261014T102810Z-f59b0c74 --disposition "<what you decided or did>"

# Escalation: holds3: the RichOS app (regular path) shows no Stop-hook notice at all, so the turn-end hold line cannot reach him in the app without new UI

- id: `esc-20261021T235756Z-846318be`
- raised: 2026-10-21T23:57:56Z
- from: zach-opus-holds3
- worktree: `/Users/alex/ab/richos-wt/zach-opus-holds3` (branch `cc/zach-opus-holds3`)
- head: `b87ebca10283e6893274b0ac0fa76fbabea89eb6`
- state: **proceeding**
- for: lead

## The question

The brief says: if the app does not show the turn-end hold line, make it show. Making it show is new RichOS UI (CLAUDE.md: Iris mockup round first, engineer builds only the chosen design) in Echo's native.rs. Should I (a) stop at the engine half and hand the display to an Iris round plus Echo or (b) build a plain display of the Stop systemMessage in the conversation now without a mockup?

## What was already tried

Read the code on cc/zach-opus-holds3 (base b87ebca10). native.rs child_args (line 620) pass no --include-hook-events; the regular lease reader handles no system/hook_response or system/informational frame (grep: only operator_frames.rs reads them and the operator path is retired per CEO 2026-10-21 s137). engine/scripts/app-engine-hook.py Stop (line 211) runs only ass-kicker/guard-stated-actions.py, whose plain stdout reaches nobody. team_watch.rs starts the stall watcher on his team's install only (operator host). So in the app no hold line, no deferral notice and no stall-watch HELD wake reaches the user.

## Proceeding meanwhile

Engine side done and tested (holds in the registry, end kinds, deferral opens a hold, wait-message and correction records, CHANGES-REQUESTED review holds, pause end text, turn-end line in the terminal gate, stall-watch HELD/RELEASE DUE, CEO-only row quieting). I am adding the engine half for the app: app-engine-hook.py Stop emits the hold lines as a systemMessage, so a display reads them as soon as it exists. No Rust change, no VM walk until this is answered.

## How this reaches the lead

This file is the RECORD, not the delivery. The escalation was written to the
engine escalation ledger at `/Users/alex/.claude/state/escalations.jsonl` as `esc-20261021T235756Z-846318be`, which the
lead's session reads at session start and at every turn end WITHOUT this branch
being merged. If this file is never landed, the escalation still arrives.

Close it with:

    escalate.sh ack esc-20261021T235756Z-846318be --disposition "<what you decided or did>"

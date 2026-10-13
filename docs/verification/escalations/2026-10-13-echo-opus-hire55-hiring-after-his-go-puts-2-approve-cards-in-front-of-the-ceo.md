# Escalation: Hiring after his go puts 2 Approve cards in front of the CEO: Pierce's inspection fails in the app ('unknown option --print') and its shell dismissal needs approval; Dean also runs ls

- id: `esc-20261013T182614Z-fd02403b`
- raised: 2026-10-13T18:26:14Z
- from: echo-opus-hire55
- worktree: `/Users/alex/ab/richos-wt/echo-opus-hire55` (branch `cc/echo-opus-hire55`)
- head: `9a249c144cdc48642ecb5dc756b85f61ebb30118`
- state: **proceeding**
- for: lead

## The question

Should the Approve cards during hiring be fixed on this branch (echo-opus-hire55), or go to the engine owner as a separate item? They are not caused by the onboarding change and look like they hit every teammate dispatch in the app.

## What was already tried

Walk 6 of hire-walk.py on this branch's build f8afd9187 (fresh install, test VM): the interview ended in the CEO's exact words, 'Go.' registered 'Hire these AI teammates onto my team: Will (Marketing Director), Penny (Bookkeeper)', Clark researched, Dean wrote Penny and fitted Will, both saved in team/ and both named to him. But the hiring needed 2 Approve presses. From the back end's hook records (kept at /Volumes/E1TB/tmp/claude/echo-opus-hire55/walk6/callbacks.txt): (1) the back end ran `python3 engine/scripts/lib/pierce.py --dismiss <id> --reason 'Pierce produced no findings: its inspection could not run (Claude Code invocation failed with "unknown option --print")...'` through Bash, so Pierce's own inspection is broken in the app and every dispatch gets a shell dismissal the permission desk shows him; (2) Dean, on a no-repository consult, ran `ls` on its own consult workspace under coordination/.claude/worktrees/. My DESKTOP.md line already tells the back end to use no shell command for hiring; the Pierce dismissal comes from the dispatch flow itself.

## Proceeding meanwhile

Walk 7 is running with the approval count judged in a final step, so relaunch, roster and delegation to a hired teammate are proven on the same guest. The branch's onboarding change and the walk are committed. I am not changing pierce.py or the permission desk without your word.

## How this reaches the lead

This file is the RECORD, not the delivery. The escalation was written to the
engine escalation ledger at `/Users/alex/.claude/state/escalations.jsonl` as `esc-20261013T182614Z-fd02403b`, which the
lead's session reads at session start and at every turn end WITHOUT this branch
being merged. If this file is never landed, the escalation still arrives.

Close it with:

    escalate.sh ack esc-20261013T182614Z-fd02403b --disposition "<what you decided or did>"

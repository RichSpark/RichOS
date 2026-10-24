# Escalation: Reap walk rerun on 1e99dd8e0 (axretry1 merged): first-run now PASSES, but the walk ends at connect because ax.py labels a window read the app did not answer 'blocked', which axwait does not retry

- id: `esc-20261021T192253Z-03aba211`
- raised: 2026-10-21T19:22:54Z
- from: echo-opus-oneappb3
- worktree: `/Users/alex/ab/richos-wt/echo-opus-oneappb3` (branch `cc/echo-opus-oneappb3`)
- head: `1e99dd8e0ce5e9c073d50a8998f06607b0eace66`
- state: **proceeding**
- for: lead

## The question

Who makes ax.py report an unanswered AXWindows read as 'unresponsive' (so axwait retries it) and do I walk once more after that lands? The fix I would make, one place: ax.py lines 213-216 catch every Exception from ax.get(app, 'AXWindows') and return error('blocked', e); AX.get (ax.py 536-541) raises TimeoutError for kAXErrorCannotComplete. Adding 'except TimeoutError: raise' before the generic except sends it to respond() (ax.py 646-650), which answers 'unresponsive', already in axwait.NO_ANSWER.

## What was already tried

One rerun after the axretry1 fix, never a loop. run-walk.py --bundle (built from 1e99dd8e0ce5e9c073d50a8998f06607b0eace66, cdhash 65c2ed39a49cf95c6f5fd3c3e4f923f4b5e07139) --home (fixture with the planted operator.json) ... reap-walk.py --expect-sha 1e99dd8e0ce5e9c073d50a8998f06607b0eace66 --steps identity,review-watch,first-run,connect,seed,watch,unrelated,stop,quit. Result: identity PASS, review-watch PASS, first-run PASS, connect FAIL at the enter() read reap-walk.py:313 present('Talk to Rich'): ax.sh find --title Talk to Rich --role AXButton --contains --first --json returned {"error": "blocked", "detail": "AXUIElement(0x6000006a0900) did not answer for AXWindows (kAXErrorCannotComplete)"}. Guest cleanup: '[testvm] 19:21:26Z clean: app quit, VM stopped, clone deleted, state removed.' The app had been ready 59 s; the same kAXErrorCannotComplete class as esc-20261021T164604Z-4c30b43e, reached through the window list instead of an element attribute.

## Proceeding meanwhile

Running the proof run over 4b1454781..HEAD (1e99dd8e0) with --reuse of my earlier runs. Bundle and engine asset from 1e99dd8e0 stay in my scratch until I report; they rebuild in about 3 minutes.

## How this reaches the lead

This file is the RECORD, not the delivery. The escalation was written to the
engine escalation ledger at `/Users/alex/.claude/state/escalations.jsonl` as `esc-20261021T192253Z-03aba211`, which the
lead's session reads at session start and at every turn end WITHOUT this branch
being merged. If this file is never landed, the escalation still arrives.

Close it with:

    escalate.sh ack esc-20261021T192253Z-03aba211 --disposition "<what you decided or did>"

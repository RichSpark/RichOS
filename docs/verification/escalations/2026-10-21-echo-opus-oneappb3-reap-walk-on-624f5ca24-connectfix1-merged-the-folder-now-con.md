# Escalation: Reap walk on 624f5ca24 (connectfix1 merged): the folder now connects, but the walk ends right after the relaunch, because enter() presses the thread title without waiting for it

- id: `esc-20261021T205850Z-48c70d7d`
- raised: 2026-10-21T20:58:50Z
- from: echo-opus-oneappb3
- worktree: `/Users/alex/ab/richos-wt/echo-opus-oneappb3` (branch `cc/echo-opus-oneappb3`)
- head: `624f5ca24000afd6ea158ab53e875f86fb6394c0`
- state: **proceeding**
- for: lead

## The question

Should I make the one-line fix in reap-walk.py enter() on my branch (self.wait_for(title) before self.press(title), so the press waits through axwait like every other find) and walk once more or does someone else own it? The fix: reap-walk.py:313-316, enter() does present('Talk to Rich') / press, time.sleep(2), then press(self.facts['thread_title'] or 'Running') straight away. A press is never retried (axwait: a click may have acted). Here it failed at ax.py's AXWindows read, before any element was found, on an app relaunched seconds earlier.

## What was already tried

One walk after the connectfix1 fix, no rerun. run-walk.py --bundle (built from 624f5ca24000afd6ea158ab53e875f86fb6394c0, cdhash 1ee5cdd9d3be6a46fdf210644a374eb260e87e52) --home (fixture with the planted operator.json) ... reap-walk.py --expect-sha 624f5ca24000afd6ea158ab53e875f86fb6394c0 --steps identity,review-watch,first-run,connect,seed,watch,unrelated,stop,quit. identity PASS, review-watch PASS, first-run PASS, connect FAIL. connect_folder finished and the app was relaunched (facts.json: app pid 1938, thread_title Onboarding). Then reap-walk.py connect() -> enter() -> press('Onboarding'): ax.sh click --title Onboarding --role AXButton --first --contains --json returned {"error": "unresponsive", "detail": "AXUIElement(0x600002a04000) did not answer for AXWindows (kAXErrorCannotComplete); the read is incomplete, so nothing is reported as found or absent"}. Guest cleanup: '[testvm] 20:57:30Z clean: app quit, VM stopped, clone deleted, state removed.'

## Proceeding meanwhile

Committing this record, then running the proof run over cb0c7ae43..HEAD with --reuse of 20261021T182631Z-7mvo3lrc and 20261021T192653Z-x22tolsj. A fix in reap-walk.py touches only testvm checks, so a later --reuse rerun keeps the rest.

## How this reaches the lead

This file is the RECORD, not the delivery. The escalation was written to the
engine escalation ledger at `/Users/alex/.claude/state/escalations.jsonl` as `esc-20261021T205850Z-48c70d7d`, which the
lead's session reads at session start and at every turn end WITHOUT this branch
being merged. If this file is never landed, the escalation still arrives.

Close it with:

    escalate.sh ack esc-20261021T205850Z-48c70d7d --disposition "<what you decided or did>"

# Escalation: Reap walk on f3d472073 (main ced86ec1d, axhang2 landed) failed at first-run: the new ax.py 'unresponsive' read is treated as fatal by every walk's find-retry

- id: `esc-20261021T164604Z-4c30b43e`
- raised: 2026-10-21T16:46:04Z
- from: echo-opus-oneappb3
- worktree: `/Users/alex/ab/richos-wt/echo-opus-oneappb3` (branch `cc/echo-opus-oneappb3`)
- head: `f3d47207316136a37b4a297f7fb1171566c0b6c6`
- state: **proceeding**
- for: lead

## The question

Who fixes the walk harness so an ax.py 'unresponsive' read (an incomplete read, nothing found or absent) is retried by waiting finds the same way 'guest_deadline' is? Until then a walk on a busy host can end at its first transient AX read. Should I add that one-place fix on my branch or does echo-opus-axhang2 (or a fresh brief) own it?

## What was already tried

One walk, no rerun (brief: never rerun in a loop). run-walk.py ... reap-walk.py --expect-sha f3d47207316136a37b4a297f7fb1171566c0b6c6 --steps identity,review-watch,first-run,connect,seed,watch,unrelated,stop,quit. identity PASS, review-watch PASS, first-run FAIL at 16:44:57Z, 13 s after the app was ready, host CPU 100%: ax.sh find --title 'Add this company' returned {"error": "unresponsive", "detail": "AXUIElement(0x600000d0c030) did not answer for AXRole (kAXErrorCannotComplete); the read is incomplete..."} in 2.74 s guest time. Guest cleanup confirmed: 'clean: app quit, VM stopped, clone deleted, state removed.' Cause, from the code: adopt-walk.py present() (lines 97-110) returns False ('not yet') for 'notfound', 'nothing matched' and 'guest_deadline' and re-raises everything else; ax.py respond() (line 646-650) emits the new error 'unresponsive' on a TimeoutError, which present() re-raises, so wait_for/decline_first_run_sheets end the step instead of asking again. The same guest_deadline-only retry exists in about 15 other walks (git grep -n guest_deadline richos/app/scripts/testvm/*-walk.py: hire, setup, netcut, update-relaunch, folders, lease-setup, voice and others), so this hits nightly walks too, not only mine. Nothing points at the app: the read is of one element during first-run sheets on a pinned host.

## Proceeding meanwhile

Proceeding with the proof run over ced86ec1d..HEAD (f3d472073) reusing my two stopped runs. The bundle (cdhash d7d7d7555ca4562612336cc0b131e2a30b71f612, version 1.2.2-dev.f3d472073) and engine asset (repo HEAD f3d472073161) from that tip are kept in my scratch for a rerun of the walk once the harness treats 'unresponsive' as 'not yet'.

## How this reaches the lead

This file is the RECORD, not the delivery. The escalation was written to the
engine escalation ledger at `/Users/alex/.claude/state/escalations.jsonl` as `esc-20261021T164604Z-4c30b43e`, which the
lead's session reads at session start and at every turn end WITHOUT this branch
being merged. If this file is never landed, the escalation still arrives.

Close it with:

    escalate.sh ack esc-20261021T164604Z-4c30b43e --disposition "<what you decided or did>"

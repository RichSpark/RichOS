# Escalation: Test-VM guest AX reads hang to the 20 s deadline on random walk steps with the dev bundle of richos main 455193c1a

- id: `esc-20261021T122950Z-b8b7362a`
- raised: 2026-10-21T12:29:50Z
- from: echo-sonnet-walkfix1
- worktree: `/Users/alex/ab/richos-wt/echo-sonnet-walkfix1` (branch `cc/echo-sonnet-walkfix1`)
- head: `cecdbe18abddfdeae68f10cbd7e2c4a6c7bab11b`
- state: **proceeding**
- for: lead

## The question

Is the app itself unresponsive to accessibility reads at random moments (an app defect) or is the guest starved by the busy host? Four of six guest runs of my walks hit one exit-124 guest_deadline at a different step each time.

## What was already tried

Fixed the stale AXMenuItem press (shared connect_folder in adopt-walk.py). Live: Settings then Connected folders pressed as AXButton works and the Company popup appears (command-walk subset, run 5). Then the run died on a later AX read: folders-walk run 1-3 at find Connected folders AXButton and later find No name is set (the screenshot of run 3 shows the Settings dialog open with the row plainly visible); command-walk subset run 4 at click Not now in first-run, run 5 at find Project folder location, run 6 at type into the company field in first-run. Successful reads in the same runs take 0.01 to 0.5 s; the failures take 16 s, so a hang, not slowness. Host CPU was 95 to 100 percent busy from other agents.

## Proceeding meanwhile

Handing over the code fix and its offline test; the live proof is partial (past the stale press, not a full green walk). Not adding retries to the walk without evidence they help.

## How this reaches the lead

This file is the RECORD, not the delivery. The escalation was written to the
engine escalation ledger at `/Users/alex/.claude/state/escalations.jsonl` as `esc-20261021T122950Z-b8b7362a`, which the
lead's session reads at session start and at every turn end WITHOUT this branch
being merged. If this file is never landed, the escalation still arrives.

Close it with:

    escalate.sh ack esc-20261021T122950Z-b8b7362a --disposition "<what you decided or did>"

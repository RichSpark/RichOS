# Escalation: Walk 5 (calm host, photograph armed): the relaunched app ANSWERED every read; there is no app hang. The step fails because no AXButton 'Onboarding' is on screen after the relaunch

- id: `esc-20261021T213902Z-b513879e`
- raised: 2026-10-21T21:39:02Z
- from: echo-opus-oneappb3
- worktree: `/Users/alex/ab/richos-wt/echo-opus-oneappb3` (branch `cc/echo-opus-oneappb3`)
- head: `4c7870f98d47fd0379b09ee8dc155b514e7a23a3`
- state: **proceeding**
- for: lead

## The question

Answer to 'app defect or host load' for walk 4: host load. On a calm host the same step's reads all answered, so the photograph (armed only for a wait whose last read had no answer) never fired; no sample of a blocked main thread exists because there was none to take. What remains is a different failure: after connect_folder and the relaunch, enter() waits 30 s for AXButton 'Onboarding' (facts.json thread_title, set at first-run) and it never appears. The walk saves no screenshot, so I cannot tell whether the row is absent, renamed or a different role. Should I add a screenshot plus tree dump when a wait ends 'not found' and walk once more? Or should someone who knows the post-relaunch screen say what enter() should press?

## What was already tried

Walk 5, one run, on 4c7870f98 (wait_for photographs the recorded app pid on an unanswered wait), bundle unchanged from 624f5ca24 (cdhash 1ee5cdd9d3be6a46fdf210644a374eb260e87e52). Admission: 'slot held ... waited 0s ... user CPU 35.8%, system 32.5%, idle 31.8%'. identity PASS, review-watch PASS (pid 1136), first-run PASS (thread thr_02d3fa20959b40a0915fc5ce0f0362bc, title Onboarding), connect FAIL after 90.8 s: 'AXButton "Onboarding" never appeared within 30 s', with no 'last read had no answer' suffix and no photograph, so every read in that wait answered 'not found'. Relaunched app pid 1929. Guest cleanup: '[testvm] 21:36:09Z clean: app quit, VM stopped, clone deleted, state removed.' git log since 2026-10-10 shows no reap walk that ever passed connect followed by enter().

## Proceeding meanwhile

Running the proof run over the merge base..HEAD with --reuse of my earlier runs. Not walking again without an answer: that would be walk 6 today.

## How this reaches the lead

This file is the RECORD, not the delivery. The escalation was written to the
engine escalation ledger at `/Users/alex/.claude/state/escalations.jsonl` as `esc-20261021T213902Z-b513879e`, which the
lead's session reads at session start and at every turn end WITHOUT this branch
being merged. If this file is never landed, the escalation still arrives.

Close it with:

    escalate.sh ack esc-20261021T213902Z-b513879e --disposition "<what you decided or did>"

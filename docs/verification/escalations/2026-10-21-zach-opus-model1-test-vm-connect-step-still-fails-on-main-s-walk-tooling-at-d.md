# Escalation: Test-VM connect step still fails on main's walk tooling at d3b8e8835 (chooser Open never enabled), blocking the model walk's final run

- id: `esc-20261021T223625Z-7fad6200`
- raised: 2026-10-21T22:36:26Z
- from: zach-opus-model1
- worktree: `/Users/alex/ab/richos-wt/zach-opus-model1` (branch `cc/zach-opus-model1`)
- head: `d3b8e88352eaf468373970fc9ef076af3662bdc6`
- state: **proceeding**
- for: lead

## The question

Who owns the connect-folder chooser failure in adopt-walk.py / ax.py on main (cb0c7ae43 did not fix it in my run) and should the model walk's handover rest on the three passing walks of identical app and engine code until it is fixed?

## What was already tried

Walked lease-setup-walk.py --steps identity,first-run,connect,model at d3b8e8835 (bundle 1.2.2-dev.d3b8e8835, engine asset from the same tip, fresh fixture home): connect FAIL 'the chooser's Open button was not enabled within 15 s (enabled=False)'; model then FAIL because ax type could not write into the composer while the dialog was up (typefailed). The same two failures at af2e6d01a before cb0c7ae43. The guest was cleaned both times. The same steps PASSED at b92e31372, 0fe64c416 and 1530f7939 (app and engine code identical to d3b8e8835; only richos/app/scripts/testvm and docs changed since), each with a sonnet worker start record and a fable refusal. Evidence: /Volumes/E1TB/tmp/claude/zach-opus-model1/walk5/report.json and walk5b.log.

## Proceeding meanwhile

Running the final proof run over cb0c7ae43..HEAD with --reuse of my two earlier runs; then handing over with the earlier passing walk evidence and this failure reported, not looped.

## How this reaches the lead

This file is the RECORD, not the delivery. The escalation was written to the
engine escalation ledger at `/Users/alex/.claude/state/escalations.jsonl` as `esc-20261021T223625Z-7fad6200`, which the
lead's session reads at session start and at every turn end WITHOUT this branch
being merged. If this file is never landed, the escalation still arrives.

Close it with:

    escalate.sh ack esc-20261021T223625Z-7fad6200 --disposition "<what you decided or did>"

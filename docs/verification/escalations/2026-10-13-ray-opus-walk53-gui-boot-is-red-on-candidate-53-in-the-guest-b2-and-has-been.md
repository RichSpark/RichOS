# Escalation: gui-boot is red on candidate 53 in the guest (B2), and has been since 2026-10-09: two boot lines no rule accounts for

- id: `esc-20261013T003340Z-e4910ff2`
- raised: 2026-10-13T00:33:40Z
- from: ray-opus-walk53
- worktree: `/Users/alex/ab/richos-wt/ray-opus-walk53` (branch `cc/ray-opus-walk53`)
- head: `aad62a2142b7c41b037dbc59efac100aac0c7eaf`
- state: **proceeding**
- for: lead

## The question

Who fixes gui-boot's B2 accounting? The healthy boot prints two lines the suite has no rule for: '[richos] window: launch kind fresh, splash on' (added in ef9fbbe81, 2026-10-08) and '[richos] second review: the app's review-watch runs as pid N' (added in d74d92732, 2026-10-09). The suite was last changed 2026-10-08 (941988288). Every nightly since has recorded gui-boot NOT RUN (no host screen), so nobody saw it. Run in the guest via RICHOS_GUI_HOST, it failed twice, the same way (1 FAILED, 34 passed). It is not a user-visible defect and not caused by this nightly's changes.

## What was already tried

RICHOS_GUI_HOST=richos-test-1 run-tests.sh --only gui-boot.test.sh from the walk53 worktree at aad62a214, with the nightly's runtime and speech models; run-tests retried it once alone; both runs fail B2 only, on the same two lines.

## Proceeding meanwhile

Finishing the walk of candidate 53 (clean-guest first launch, the starter file, front-door in the guest, D30). The verdict will state gui-boot's result as it is.

## How this reaches the lead

This file is the RECORD, not the delivery. The escalation was written to the
engine escalation ledger at `/Users/alex/.claude/state/escalations.jsonl` as `esc-20261013T003340Z-e4910ff2`, which the
lead's session reads at session start and at every turn end WITHOUT this branch
being merged. If this file is never landed, the escalation still arrives.

Close it with:

    escalate.sh ack esc-20261013T003340Z-e4910ff2 --disposition "<what you decided or did>"

# Escalation: hostshim land: the merge gate will still refuse on engine scripts/lib/appinstances.test.sh, which fails on main's own code under ci-shard (not the shield)

- id: `esc-20261011T154641Z-08f6cd96`
- raised: 2026-10-11T15:46:41Z
- from: zach-opus-hostshim2
- worktree: `/Users/alex/ab/richos-wt/zach-opus-hostshim2` (branch `cc/zach-opus-hostshim2`)
- head: `d0066287aef592c7d14b44c66024192a9818ed50`
- state: **proceeding**
- for: lead

## The question

appinstances.test.sh A2 and A6 fail whenever ci-shard runs the unit (15 passed, 2 failed: A2 expected LEAVE got COLLECT; A6 expected INDETERMINATE got COLLECT). Cause: ci-shard gives every unit a private HOME (record-canary rc_sandbox, HOME=$LOG_DIR/home.N), and the test takes $HOME/Library/Application Support/com.richos.app as the operator's real app state, which under that HOME is scratch, so the module rightly says COLLECT. Proven not the shield: the same ci-shard with the shield switched off fails identically (2 failed), and the test run directly passes 17/17 with and without the shield. Neither file is touched by this land; the land only selects the unit because ci-shard.sh changed. Who fixes it (the test needs the operator's real home from the password database, or ci-shard needs to leave HOME for this unit), and should the hostshim land wait for it?

## What was already tried

Merge-gate replay from the worktree at d0066287a over main 9debfbcf6: round 1 28 passed, round 2 77 passed with only appinstances failed (and failed again alone); proof-evidence, run-tests, proof-run, test-results, autocheck, ci-shard and host_shield suites all passed. Logs: /Volumes/E1TB/state/richos/proof-runs/autocheck/b9585aaa888ea808b354bcebdc5a55b1844f4eebcc136e1dff2873fd1feb8593/attempt-izyaxpge and attempt-d7im62xm.

## Proceeding meanwhile

Running the 103 checks the gate left with no verdict (resumed rounds, passes kept) to find any other suite that fails with the shield on; fixing any of those on this branch.

## How this reaches the lead

This file is the RECORD, not the delivery. The escalation was written to the
engine escalation ledger at `/Users/alex/.claude/state/escalations.jsonl` as `esc-20261011T154641Z-08f6cd96`, which the
lead's session reads at session start and at every turn end WITHOUT this branch
being merged. If this file is never landed, the escalation still arrives.

Close it with:

    escalate.sh ack esc-20261011T154641Z-08f6cd96 --disposition "<what you decided or did>"

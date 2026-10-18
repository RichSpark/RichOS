# Escalation: Follow-up 13: the 2.11 and 2.12 modules were already deleted on richos main by 83e41a271; only their leftovers remain

- id: `esc-20261018T191608Z-6fa6c4f1`
- raised: 2026-10-18T19:16:08Z
- from: echo-opus-dead66
- worktree: `/Users/alex/ab/richos-wt/echo-opus-dead66` (branch `cc/echo-opus-dead66`)
- head: `6d075e8929701e997e852beb378f2cb960b4d3c8`
- state: **proceeding**
- for: lead

## The question

None needed. For the record: follow-ups-after-nightly65.md item 13 and nightly63-what-is-handled.md section G still describe 2.11 (reachability.rs) and 2.12 (first_reply.rs) as present, but 83e41a271 (2026-10-18 00:14, an ancestor of main 6d075e892) deleted src/reachability.rs, src/first_reply.rs, their examples and tests/reachability_tests.rs. What is left: FakeUpstream (used only by its own two self-tests, which say they exist for reachability.rs), CONTINUITY_TOOL_PREFIX plus its drift test (whose stated reason is the deleted first_reply_faults), and doc text naming the deleted files. I am removing those leftovers as the 2.11 and 2.12 commits, and doing 1.11 (the second company registry copy) in full.

## What was already tried

git log --diff-filter=D on both module paths; git merge-base --is-ancestor 83e41a271 6d075e892 (true); grep of every remaining reference.

## Proceeding meanwhile

Proceeding with all three commits.

## How this reaches the lead

This file is the RECORD, not the delivery. The escalation was written to the
engine escalation ledger at `/Users/alex/.claude/state/escalations.jsonl` as `esc-20261018T191608Z-6fa6c4f1`, which the
lead's session reads at session start and at every turn end WITHOUT this branch
being merged. If this file is never landed, the escalation still arrives.

Close it with:

    escalate.sh ack esc-20261018T191608Z-6fa6c4f1 --disposition "<what you decided or did>"

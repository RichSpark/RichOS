# Escalation: batch58b: convo58c alone fails 2 richos-core integration tests (not a combination break); nightly blocked until decided

- id: `esc-20261015T141749Z-f43e0917`
- raised: 2026-10-15T14:17:50Z
- from: zach-opus-batch58b
- worktree: `/Users/alex/ab/richos-wt/zach-opus-batch58b` (branch `cc/zach-opus-batch58b`)
- head: `8d0d543e57d17b07ec439e07b205616b7d4513cb`
- state: **proceeding**
- for: lead

## The question

convo58c's finding-13 rule (activity_item in timeline.rs: a ToolCall whose type is Other is Technical, never in his view) makes two OLDER tests fail on cc/echo-opus-convo58c by itself: live_event_tests::a_lifecycle_row_that_lands_after_the_tool_result_still_reaches_the_screen_in_the_same_turn (line 1306: a Task delegation before its worker row is no longer drawn as an activity row, because classify() returns None for Task/Agent) and timeline_tests::no_machinery_from_one_entity_renders_in_another_entitys_thread (line 307: the fixture's tool call named 'git status' is unclassified, so the CEO view has no Activity). Which should it be: (a) the tests move to the new rule (timeline test step 4 checks the technical view or uses a classified Bash tool; live_event test drops the degrade-row assertion and keeps the one-upgrade/same-id checks), or (b) Task/Agent delegations stay visible in his view before their worker row exists (a carve-out in activity_item), which also covers a synchronous delegation that never becomes a worker and is otherwise invisible to him while it runs? My brief allows fixing only what the combination broke, so I have not changed either.

## What was already tried

Merged convo58c and waitline58 clean (no conflicts); set README richos-core total to 2038 (2034 direct, 4 ignored), docs-claims passes. cargo test -p richos-core on the merged tip: lib 1295 passed, 3 ignored; integration 70 targets ok, 2 failed (above). Isolated retry on the tip: same failure, deterministic. Same test on a git-archive snapshot of main 44592c8ad: passes. On a snapshot of cc/echo-opus-convo58c 66cdd0ca3 alone: fails identically. So convo58c's own crate suite was red when it was reviewed.

## Proceeding meanwhile

Finishing the UI suites and the richos-tauri bin tests on the merged tip; will hand over the tip SHA with these two failures stated, unfixed.

## How this reaches the lead

This file is the RECORD, not the delivery. The escalation was written to the
engine escalation ledger at `/Users/alex/.claude/state/escalations.jsonl` as `esc-20261015T141749Z-f43e0917`, which the
lead's session reads at session start and at every turn end WITHOUT this branch
being merged. If this file is never landed, the escalation still arrives.

Close it with:

    escalate.sh ack esc-20261015T141749Z-f43e0917 --disposition "<what you decided or did>"

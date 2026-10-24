# Escalation: Changelog brief premise: the terminal's hooks do not run in the app; only the app's own wiring does

- id: `esc-20261024T220103Z-37f2d007`
- raised: 2026-10-24T22:01:03Z
- from: newt-opus-notesfix2
- worktree: `/Users/alex/ab/richos-wt/newt-opus-notesfix2` (branch `cc/newt-opus-notesfix2`)
- head: `fbbd13869c9f36c6811947a62e25f75ea8759e70`
- state: **work-complete**
- for: lead

## The question

None needed to finish. The lead should know before relying on the premise: the app starts Claude with --setting-sources '' and its own generated hook list (native.rs child_args, engine_profile.rs prepare), so engine/hooks/hooks.json and the terminal's hooks never load in the app. The engine changes that reach users are those the app wires: app-engine-hook.py and what it calls, four Bash checks, the user-work spawn guards with Pierce, Stu's stall watcher, the review watcher and the question-card checks. So the cleanup alert, the disk notices, the CPU guard and the turn-end refusal never reach a user. The corrected instructions and docs/release-notes-74-missed.md classify by that.

## What was already tried

Read engine_profile.rs prepare and configure, native.rs child_args and StallWatch, review_watch.rs, question_engine.rs and app-engine-hook.py at 95b10c215.

## Proceeding meanwhile

Work is committed on cc/newt-opus-notesfix2: 64c6742c2 (instructions), 59078a40f (draft lines), fbbd13869 (merge of main).

## How this reaches the lead

This file is the RECORD, not the delivery. The escalation was written to the
engine escalation ledger at `/Users/alex/.claude/state/escalations.jsonl` as `esc-20261024T220103Z-37f2d007`, which the
lead's session reads at session start and at every turn end WITHOUT this branch
being merged. If this file is never landed, the escalation still arrives.

Close it with:

    escalate.sh ack esc-20261024T220103Z-37f2d007 --disposition "<what you decided or did>"

# Escalation: landenv1: the end-to-end acceptance on Codex's real run must execute inside the codex/ workspace, which the worktree guard forbids agents (no override)

- id: `esc-20261022T141022Z-e2b49358`
- raised: 2026-10-22T14:10:23Z
- from: zach-opus-landenv1
- worktree: `/Users/alex/ab/richos-wt/zach-opus-landenv1` (branch `cc/zach-opus-landenv1`)
- head: `ee5d2b58bfeb189258314acbbb6d120c34fc8246`
- state: **proceeding**
- for: lead

## The question

The acceptance you asked for (Codex's real run 20261022T131701Z-vjaoaa1d, through the real land entry, from a changed shell, expecting 56 proven / 2 NOT RUN / 0 missing) can only be asked in that run's own checkout. Is it acceptable that it is run as the retry of Codex's land after this branch lands (main's autocheck then rebuilds the run's environment and hands it to Codex's own runner; the gate reuses under it), from your normal shell, under both /usr/bin/git and Homebrew git? A refused land runs nothing, so a failure costs one preflight.

## What was already tried

The preflight is answered by the run's frozen runner in its own checkout (plan root) and whole-checkout identities name that checkout, so it cannot be asked from a copy without editing the plan. My direct read inside that workspace was refused by guard-worktree-removal.sh (an agent never works inside a codex/ workspace; no override), so I did not reach it by indirection. Offline, from the run directory only: the environment rebuilt from its rerun files has PATH equal to the one 44 passes saved and reproduces the saved environment digests of 41 of 54 comparable passes in my mimic. The other 13 differ only where my mimic, running my checkout's runner, names my checkout (the Cargo shim directory; UI-suite gate detection by cwd). One mimic run listed suites (run-tests.sh --list, read-only) with its working directory in that checkout before I noticed; I stopped.

## Proceeding meanwhile

Committing the fix (new plans record their environment; older runs rebuild it from their rerun files; autocheck hands it to the author's runner; main's gate reuse reads under it), the fixture test from this case, unit tests, then the branch proof run.

## How this reaches the lead

This file is the RECORD, not the delivery. The escalation was written to the
engine escalation ledger at `/Users/alex/.claude/state/escalations.jsonl` as `esc-20261022T141022Z-e2b49358`, which the
lead's session reads at session start and at every turn end WITHOUT this branch
being merged. If this file is never landed, the escalation still arrives.

Close it with:

    escalate.sh ack esc-20261022T141022Z-e2b49358 --disposition "<what you decided or did>"

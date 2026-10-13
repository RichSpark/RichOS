# Escalation: Every richos commit under richos/app is now refused: Homebrew moved python3 to 3.15, the lint baselines record 3.14

- id: `esc-20261013T073742Z-e6e0526e`
- raised: 2026-10-13T07:37:42Z
- from: ray-opus-walk54
- worktree: `/Users/alex/ab/richos-wt/ray-opus-walk54` (branch `cc/ray-opus-walk54`)
- head: `17fe48f48ae059a4ff805e0f28ca09ef67607aa4`
- state: **proceeding**
- for: lead

## The question

Should python3 on this Mac go back to 3.14 (or lint.sh call python3.14), or should the lint baselines be migrated to 3.15 by an explicit integration migration?

## What was already tried

The pre-commit autocheck ran lint.sh --changed --strict and refused with: Lint refused: lint versions do not match the committed baseline. /opt/homebrew/bin/python3 now points at python@3.15 3.15.0 (link dated 13 Oct 08:35 local); driver.py records sys.version major.minor and every baseline under richos/app/scripts/lint/baselines says python 3.14. python3.14 (3.14.8) is still installed. The same lint, run as python3.14 richos/app/scripts/lint/driver.py --changed --strict on my staged change, prints Lint passed.

## Proceeding meanwhile

Committing the nightly 54 walk record with the hook's own lint run under python 3.14 (a PATH entry whose python3 is python3.14), so the check runs unchanged against the baseline's own interpreter; nothing is skipped and no --no-verify is used.

## How this reaches the lead

This file is the RECORD, not the delivery. The escalation was written to the
engine escalation ledger at `/Users/alex/.claude/state/escalations.jsonl` as `esc-20261013T073742Z-e6e0526e`, which the
lead's session reads at session start and at every turn end WITHOUT this branch
being merged. If this file is never landed, the escalation still arrives.

Close it with:

    escalate.sh ack esc-20261013T073742Z-e6e0526e --disposition "<what you decided or did>"

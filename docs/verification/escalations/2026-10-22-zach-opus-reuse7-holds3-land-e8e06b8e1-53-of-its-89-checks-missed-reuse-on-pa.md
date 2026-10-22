# Escalation: holds3 land e8e06b8e1: 53 of its 89 checks missed reuse on PATH order only (fixed on reuse7); the other 27 are recipes that cannot soundly be reused from a worktree into the main checkout

- id: `esc-20261022T074633Z-0288dc90`
- raised: 2026-10-22T07:46:33Z
- from: zach-opus-reuse7
- worktree: `/Users/alex/ab/richos-wt/zach-opus-reuse7` (branch `cc/zach-opus-reuse7`)
- head: `8fd1675a021eb79263532580fc518ccd728f6480`
- state: **proceeding**
- for: lead

## The question

For the 27 recipe checks that declare the whole richos/engine folder: should install.sh stop writing its .sha256 sidecars (170 of them) and should session state (richos/engine/.claude/state/agent-definitions-*.snapshot) stop living inside the main checkout's engine folder, so that the main checkout's engine folder holds what a worktree holds? Until then those 27 run again at every land, correctly. Default if unanswered: I change nothing about them and hand over the PATH fix.

## What was already tried

Tallied the land's first attempt (attempt-ke8udd6v, progress.json): 53 whole-checkout checks missed with the single reason 'made in another checkout, where it no longer stands in its own checkout (environment.PATH)'; the two PATHs held the same 21 entries, ~/.cargo/bin second in the author's agent shell and last in the lead's and the runner only appended a missing one. Fixed on cc/zach-opus-reuse7: the runner gives every check one canonical PATH order (test red on 8fd1675a0, green after). 3 were reused already; 4 had no passing author execution to reuse. The 27 recipe misses all include paths.richos/engine: the main checkout has 170 git-ignored install.sh .sha256 sidecars and live session snapshots under richos/engine that no worktree has and contract-integrity reads sidecars, so those passes really were made on other inputs. Recipe identities also carry the checkout's own absolute bin directory in PATH (unplaced), a second reason they never cross checkouts.

## Proceeding meanwhile

Committing the PATH fix, then replaying the exact land case (author handover run in my worktree, gate run from a second checkout of the same commit with the other PATH order and --reuse of the author run), alongside the 3-file replay; then the handover.

## How this reaches the lead

This file is the RECORD, not the delivery. The escalation was written to the
engine escalation ledger at `/Users/alex/.claude/state/escalations.jsonl` as `esc-20261022T074633Z-0288dc90`, which the
lead's session reads at session start and at every turn end WITHOUT this branch
being merged. If this file is never landed, the escalation still arrives.

Close it with:

    escalate.sh ack esc-20261022T074633Z-0288dc90 --disposition "<what you decided or did>"

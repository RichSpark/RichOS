# Escalation: zach-opus-reap3: the brief's point 1 (automatic end only for processes whose own environment carries RICHOS_AGENT_OWNER) cannot end the incident's loop on this Mac; building it as written makes the automatic end inert

- id: `esc-20261014T111028Z-11fec728`
- raised: 2026-10-14T11:10:28Z
- from: zach-opus-reap3
- worktree: `/Users/alex/ab/richos-wt/zach-opus-reap3` (branch `cc/zach-opus-reap3`)
- head: `6aad0306e7848a09cb798cede4bdf95601929265`
- state: **proceeding**
- for: lead

## The question

For the automatic idle end in stall_watch.py, which ownership rule do you want? (A) As the brief says: only processes whose own readable environment carries the agent's tag. Measured on this Mac it ends NOTHING in the incident class, so the hand step stays. (B) Recommended: the agent's recorded shells (pid, birth and parent captured at spawn by agent_hold mark, re-read from the kernel before each signal; this is CLAUDE.md working rule 1's own wording, 'a PID you captured at spawn') plus their descendants plus tagged processes, EXCEPT any process whose environment IS readable and does not carry this agent's owner and session tag (that one is provably not this agent's and is never ended automatically). Reply A or B; I build it as the next commit.

## What was already tried

Measured on this Mac (macOS Darwin 24.6.0) with agent_hold._environment (KERN_PROCARGS2) and cross-checked with ps -wwE: (1) /bin/sleep, /bin/bash, /bin/zsh and /usr/bin/env started WITH RICHOS_AGENT_OWNER in their exec environment show ZERO environment entries, to agent_hold and to ps -E alike: macOS hides the environment of Apple platform binaries. A Homebrew python3 shows 49 entries and the tag. (2) The recorded shell exports the tag after its own exec (agent_hold._head), so its own environment never carries it, and the forked subshells of _wrap inherit that exec-time environment (fork, no exec). The review's witness (own_environment_tag=False for the native shell) is this. (3) Probe: a real set -eo pipefail shell exporting the tag and running 'until grep -q DONE f; do sleep 30; done' (also inside a ( ... ) & subshell, also with sleep 30 || :), under both bash and zsh: tree of 2-3 processes, tagged=0 every time, so ending only tagged processes leaves the loop running in all 8 cases. The incident (echo-opus-tap22b) was exactly such a grep/sleep loop. Scripts: /Volumes/E1TB/tmp/claude/zach-opus-reap3/probe*.py (deleted at handover; the numbers are in this escalation).

## Proceeding meanwhile

Building point 2 (a missed monitor sample restarts the 10-minute idle count) with its red/green test in /Users/alex/ab/richos-wt/zach-opus-reap3 on cc/zach-opus-reap3, committing it, and handing over. Point 1 is not built until you answer; the code on the branch is unchanged for it (automatic end still covers recorded shells and their descendants).

## How this reaches the lead

This file is the RECORD, not the delivery. The escalation was written to the
engine escalation ledger at `/Users/alex/.claude/state/escalations.jsonl` as `esc-20261014T111028Z-11fec728`, which the
lead's session reads at session start and at every turn end WITHOUT this branch
being merged. If this file is never landed, the escalation still arrives.

Close it with:

    escalate.sh ack esc-20261014T111028Z-11fec728 --disposition "<what you decided or did>"

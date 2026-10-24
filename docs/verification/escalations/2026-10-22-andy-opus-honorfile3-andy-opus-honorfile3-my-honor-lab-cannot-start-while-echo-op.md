# Escalation: andy-opus-honorfile3: my Honor lab cannot start while echo-opus-filecard3's lab holds the fixed Connect port 18443

- id: `esc-20261022T132519Z-1c7cba4c`
- raised: 2026-10-22T13:25:19Z
- from: andy-opus-honorfile3
- worktree: `/Users/alex/ab/richos-wt/andy-opus-honorfile3` (branch `cc/andy-opus-honorfile3`)
- head: `884951b3d5f6b1d911e15ce0f11f6edeb8850875`
- state: **proceeding**
- for: lead

## The question

Can echo-opus-filecard3 stop its android-parity lab (pid 94436 on 127.0.0.1:18443, cloudflared pid 95272 on 18445, cwd /Users/alex/ab/richos-wt/echo-opus-filecard3) when it is done or should I keep waiting for it to free the port?

## What was already tried

The brief says two labs share this Mac. They cannot: richos/app/src-tauri/src/phone/connect/mod.rs:7 is PORT: u16 = 18443 (a constant, no env override) and cloudflared's metrics port 18445 is also fixed (supervisor.rs:121 METRICS_PORT). lsof shows echo's lab test binary listening on 18443 now. I have not touched echo's processes and will not.

## Proceeding meanwhile

Running step 5 (proof-run.py with the given --reuse) now, then polling, with bounded waits, for 18443 to be free; the Honor walks (steps 1-4) start the moment it is.

## How this reaches the lead

This file is the RECORD, not the delivery. The escalation was written to the
engine escalation ledger at `/Users/alex/.claude/state/escalations.jsonl` as `esc-20261022T132519Z-1c7cba4c`, which the
lead's session reads at session start and at every turn end WITHOUT this branch
being merged. If this file is never landed, the escalation still arrives.

Close it with:

    escalate.sh ack esc-20261022T132519Z-1c7cba4c --disposition "<what you decided or did>"

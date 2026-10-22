# Escalation: Test-VM AX hangs: not the app; a starved guest multiplied by an AX client that costs 7-190 ms per attribute (direct AX costs 0.3-0.4 ms)

- id: `esc-20261021T133408Z-5d70a3e4`
- raised: 2026-10-21T13:34:08Z
- from: echo-opus-axhang1
- worktree: `/Users/alex/ab/richos-wt/echo-opus-axhang1` (branch `cc/echo-opus-axhang1`)
- head: `40e59602e4a70f27716c7906980e55a2e814e264`
- state: **proceeding**
- for: lead

## The question

The brief offered two causes (app main thread blocked or guest starved). Evidence says the app is answering and the guest is starved, but the hang needs a third factor: ax.js reads every attribute as an Apple Event through System Events, which re-resolves the element from the process root each time. Default I am building now unless told otherwise: keep ax.sh and its output contract unchanged and move its element reads and presses from System Events Apple Events to the macOS accessibility API called directly in the guest (measured 50-300x cheaper), plus the guest RAM back to 5120 MB. Is that rewrite of the shared AX tool in scope or should I stop at the RAM fix and the diagnostics?

## What was already tried

Committed an overdue-read snapshot in ax-deadline.py (1bbddfd02, 9a19ef798, 40e59602e) and reproduced the hang in 2 of 2 first-run walks. walk-c9d2266f135e at the hang: app main thread 71 of 88 samples idle in its run loop and 9 answering AX; WebContent idle; System Events 85 of 90 samples inside Apple Event specifier evaluation; guest 4.65 percent idle, load 43.65, 28 runnable; host 100 percent busy with two guests at about 165 percent CPU each. walk-254ad130a344: guest 0.0 percent idle, load 44.80, VM process 392 percent. Guest timelines (walk-4bee667d4653, walk-56dbcca781c4): the first 2 minutes after boot are 81-85 percent busy, 40-50 percent system time; the 4096 MB guest thrashes memory (500k compressions, 353k decompressions, 52k pageouts in 129 s); at 5120 MB (walk-7eb7ae8b5a94) 181k, 64k, 3k in 82 s. Profile in the guest (walk with prof.js and a ctypes AX reader): one attribute through System Events 7-26 ms shallow, 15-120 ms at depth 7; the whole app tree through the AX API directly, 98 nodes and 392 attribute reads, 115-161 ms including during the storm. ax.sh tree of 20 nodes: 4.4 s calm, 16.0 s in the storm.

## Proceeding meanwhile

Building the direct-AX element layer behind the unchanged ax.sh interface with its tests, then proving it with the failing first-run walk; RAM change measured and committed separately.

## How this reaches the lead

This file is the RECORD, not the delivery. The escalation was written to the
engine escalation ledger at `/Users/alex/.claude/state/escalations.jsonl` as `esc-20261021T133408Z-5d70a3e4`, which the
lead's session reads at session start and at every turn end WITHOUT this branch
being merged. If this file is never landed, the escalation still arrives.

Close it with:

    escalate.sh ack esc-20261021T133408Z-5d70a3e4 --disposition "<what you decided or did>"

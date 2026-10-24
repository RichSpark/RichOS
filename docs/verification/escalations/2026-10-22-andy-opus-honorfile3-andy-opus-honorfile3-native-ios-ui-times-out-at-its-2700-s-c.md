# Escalation: andy-opus-honorfile3: native-ios-ui times out at its 2700 s cap twice on this range (125 UI cases serially on one simulator, host at 100% CPU)

- id: `esc-20261022T155522Z-db7758aa`
- raised: 2026-10-22T15:55:22Z
- from: andy-opus-honorfile3
- worktree: `/Users/alex/ab/richos-wt/andy-opus-honorfile3` (branch `cc/andy-opus-honorfile3`)
- head: `839cffff9f18ea63cbc50fe50731e2d7b0b7e921`
- state: **proceeding**
- for: lead

## The question

If native-ios-ui times out a third time when retried alone after this run, should the land take it from a quieter window or from a run you choose or do I keep retrying?

## What was already tried

Run e0b69996eb8c/20261022T132530Z-4dcs8ag_ (TIMEOUT 2705 s, exit 124) and final run e0b69996eb8c/20261022T145638Z-l3bwpidw (TIMEOUT 2703 s, exit 124). Both logs stop at 'device 1: 0 retained cases, 125 unresolved/control cases'; native-ios-ui.test.sh runs SHARDS=1, serially on one prepared simulator, budget 600 s, cap 2700 s. My branch's own changes are Android plus one test string; the iOS commits in range came from cc/andy-opus-honorfile1's base (echo-opus-filecard3's merge). Both Honor checks PASSED on the phone; native-android-ui, native-android-app and native-android-core PASSED; D27 fixed (f237122a8).

## Proceeding meanwhile

Letting the final run finish its other 100+ checks, then retrying native-ios-ui alone with --reuse of the final run and handing over with whatever that retry shows.

## How this reaches the lead

This file is the RECORD, not the delivery. The escalation was written to the
engine escalation ledger at `/Users/alex/.claude/state/escalations.jsonl` as `esc-20261022T155522Z-db7758aa`, which the
lead's session reads at session start and at every turn end WITHOUT this branch
being merged. If this file is never landed, the escalation still arrives.

Close it with:

    escalate.sh ack esc-20261022T155522Z-db7758aa --disposition "<what you decided or did>"

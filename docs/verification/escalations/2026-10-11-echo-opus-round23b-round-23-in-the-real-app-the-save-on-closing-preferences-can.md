# Escalation: Round 23: in the real app the save on closing Preferences can land 10 to 70+ seconds late, because the app's command threads are taken up by blocking work (a defect that was already there)

- id: `esc-20261011T062202Z-883703bd`
- raised: 2026-10-11T06:22:02Z
- from: echo-opus-round23b
- worktree: `/Users/alex/ab/richos-wt/echo-opus-round23b` (branch `cc/echo-opus-round23b`)
- head: `9e186dc3cb33133d68881f55517b69af3c169be8`
- state: **proceeding**
- for: lead

## The question

Should a fresh brief move the blocking command bodies off Tauri's async workers (richos_voice::stt::time_decode's Command::output, and commands parked on the spine mutex, e.g. decline_onboarding at src-tauri/src/main.rs:5379)? Or is the late save on close acceptable for round 23 as it stands?

## What was already tried

Test VM, dev bundle 1.2.0-dev.a817887fc, nightly 50 engine. Clean flow (no other command waiting): Fable, then Escape; toast 'Rich now talks with you on Fable.' on screen; config.json rich_model=fable at +15 s. With the interview card's 'Not now' pending (decline_onboarding parked on take_the_spine; app.log: 'a window command waited 15093 ms for the spine at src/main.rs:5379'): config.json still rich_model=opus at +2 s, +10 s and about +68 s in three runs, and fable at +70 s in another run. In the run where the save landed by +10 s, a `sample` of richos-tauri showed 3 of the 4 tokio-rt-worker threads inside richos_voice::stt::time_decode -> std::process::Command::output (blocking). Tauri 2.11.5 runs #[tauri::command(async)] sync bodies inside async_runtime::spawn (tauri-macros 2.6.3 command/wrapper.rs:378-392; tauri ipc/mod.rs:329), so a blocking body ties up a worker, and set_lease_models waits its turn. set_assertiveness and every other async command share the same path. While the save waits, reopening Preferences shows the new choice (draft = optimistic saved), and no toast appears until it lands.

## Proceeding meanwhile

Finishing the brief's list as written: contrast.js 9z/19 green, 7 shots regenerated and committed, merge of local main. No code change for this (out of the brief's scope); reporting it in the handoff.

## How this reaches the lead

This file is the RECORD, not the delivery. The escalation was written to the
engine escalation ledger at `/Users/alex/.claude/state/escalations.jsonl` as `esc-20261011T062202Z-883703bd`, which the
lead's session reads at session start and at every turn end WITHOUT this branch
being merged. If this file is never landed, the escalation still arrives.

Close it with:

    escalate.sh ack esc-20261011T062202Z-883703bd --disposition "<what you decided or did>"

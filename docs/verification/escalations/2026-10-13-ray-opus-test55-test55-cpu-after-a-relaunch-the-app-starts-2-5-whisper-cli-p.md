# Escalation: test55 CPU after a relaunch: the app starts 2-5 whisper-cli processes at 50-140% CPU each on every launch; same on nightly 54; the animated home screen adds ~40-60%

- id: `esc-20261013T222951Z-d927933b`
- raised: 2026-10-13T22:29:51Z
- from: ray-opus-test55
- worktree: `/Users/alex/ab/richos-wt/ray-opus-test55` (branch `cc/ray-opus-test55`)
- head: `e441235196904d0525126d7aab7d02d42e6ab388`
- state: **proceeding**
- for: lead

## The question

Is re-running the whisper speed measurement on every launch intended (hardware.rs says 'measure once per machine, not once per launch')? In the guest it pins the CPU for over a minute after each relaunch and is what makes every accessibility read take 16-20 s.

## What was already tried

Guest top (top -l 4 -s 10) 15 s after a relaunch, same steps on both bundles, fresh guests. MERGED e44123519 (runs/cpu55/steps.json): guest idle 0.0% in all 3 samples; whisper-cli pids 1550 (started BEFORE the relaunch, still running after the app it belonged to was stopped), 1700, 1723, 1756, 1757 at 8-100% each; MTLCompilerService 50-82%; WindowServer 76-86%; richos-tauri 9.5/25.2/39.7%; WebKit WebContent 19-30%, WebKit GPU 9-25% on the animated home screen. After the door was clicked (conversation screen): whisper-cli still 58-99% x2 + 26-39% x2, richos-tauri 4-13%, WebContent 6-11%. NIGHTLY 54 10bbdf9f7 (runs/cpu54/steps.json): on the home screen whisper-cli 82-135% + 23-68%, MTLCompilerService 64-92%, richos-tauri 15.7/16.6/27.0%, WebContent 25-35%, WebKit GPU 18-20%, guest idle 4.9-10.9%; after the door whisper-cli 126-141% + 2 more at 49-74%, richos-tauri 0.9-1.2%, WebContent 0.9-1.6%. Before the relaunch (conversation screen, first run, downloads running) richos-tauri was 17-23% on the merged build and 4-6% on nightly 54 (different moments of the first-run download; not a like-for-like pair). Measured in a VM guest (no host GPU), so absolute numbers are a VM's; the whisper-cli re-run on every launch and the orphaned whisper-cli surviving its app are behaviors, not VM artifacts. Evidence: /Volumes/E1TB/tmp/claude/ray-opus-test55/runs/cpu55 and cpu54.

## Proceeding meanwhile

Not a regression from the four merged branches (none touches hardware.rs, stt.rs or home.js). Rerunning the hire and voice walks with TESTVM_AX_TIMEOUT=60 so relaunch, roster and delegate are checked.

## How this reaches the lead

This file is the RECORD, not the delivery. The escalation was written to the
engine escalation ledger at `/Users/alex/.claude/state/escalations.jsonl` as `esc-20261013T222951Z-d927933b`, which the
lead's session reads at session start and at every turn end WITHOUT this branch
being merged. If this file is never landed, the escalation still arrives.

Close it with:

    escalate.sh ack esc-20261013T222951Z-d927933b --disposition "<what you decided or did>"

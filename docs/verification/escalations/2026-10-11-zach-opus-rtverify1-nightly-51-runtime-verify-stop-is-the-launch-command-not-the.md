# Escalation: Nightly 51 runtime-verify stop is the launch command, not the code: drop --runtime-dir and rebuild now

- id: `esc-20261011T213046Z-c95e4c5e`
- raised: 2026-10-11T21:30:47Z
- from: zach-opus-rtverify1
- worktree: `/Users/alex/ab/richos-wt/zach-opus-rtverify1` (branch `cc/zach-opus-rtverify1`)
- head: `4474cf8e402e619ecfad27872e4b9b0a69cf7478`
- state: **proceeding**
- for: lead

## The question

No code change is needed to unblock the nightly. /Volumes/E1TB/tmp/claude/rich/nightly51-build.sh passes --runtime-dir $HOME/.richos-nightly/runtime, the legacy fixed-path runtime from 2026-09-17 whose runtime-sources.json lists only git, iconv, jq, node, python; the recipe on main 4474cf8e4 also lists ffmpeg, ffprobe, whisper-cpp. Without --runtime-dir the runner uses ~/.richos-nightly/runtime-6a23e0727aff (sha256 of the recipe), whose runtime-sources.json is byte-identical to the recipe, and verify-runtime.py on it prints verified true (measured). Relaunch with: python3 scripts/nightly-local.py build --gates-at-once all

## What was already tried

Hashed the recipe in the nightly source checkout (6a23e0727aff...), hashed every runtime*/runtime-sources.json under ~/.richos-nightly, ran verify-runtime.py on runtime-6a23e0727aff (verified true), read the launch script.

## Proceeding meanwhile

Committing a refusal that names the cause and the recipe-keyed runtime path when --runtime-dir points at a runtime built from a different recipe, with a test red on main.

## How this reaches the lead

This file is the RECORD, not the delivery. The escalation was written to the
engine escalation ledger at `/Users/alex/.claude/state/escalations.jsonl` as `esc-20261011T213046Z-c95e4c5e`, which the
lead's session reads at session start and at every turn end WITHOUT this branch
being merged. If this file is never landed, the escalation still arrives.

Close it with:

    escalate.sh ack esc-20261011T213046Z-c95e4c5e --disposition "<what you decided or did>"

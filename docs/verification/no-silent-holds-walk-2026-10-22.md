# No silent holds: the app's Rich is told of held work and tells the user (2026-10-22)

The CEO: "WHEN WILL [THIS] STOP ONCE AND FOR ALL???" Plan: the private record's no-silent-holds plan of 2026-10-22 (Otto). Lead's answer to
esc-20261021T235756Z-846318be: no new UI; the hold lines reach the app's Rich as context in his
own turn and his instruction file has him tell the user.

**Command** (host, CPU-admitted by run-walk.py):

```
run-walk.py --bundle ~/.richos-nightly/releases/v1.2.2-nightly.20261021.70/RichOS-macos-aarch64.zip \
  --home <empty dir> --engine <make-engine-asset.sh output at 9c0ea5d09, the branch with main 198b1fb41 merged with nightly 70's runtime> \
  --report run-report.json --wait 3600 -- richos/app/scripts/testvm/holds-walk.py --out <dir> --expect-sha b0ca69c31
```

**Outcome:** identity, first-run, hold and ask all PASS; the guest was cleaned up (`cleanup_complete`).
Fixture data only: company "Acme", teammates `spelling-mate` (an unresolved wait) and `fix-mate`
(a correction sent after its last commit that its run never acted on), filed 40 minutes old in the
conversation's own registry partition.

## Rich's reply, as the app recorded it (conversation-ledger AssistantDelta of the turn)

> Good morning. It's quiet: no jobs are running in this conversation and nothing is waiting on you.
>
> Two pieces of work have been on hold for about 40 minutes, since 01:16 UTC. I'm the one holding them, so they won't move until you say go:
>
> 1. **Sign-in status fix.** A code review found that the sign-in status gets lost when reading usage fails. I sent the developer the fix, but their session ended before they made it, so the fix hasn't been made yet.
> 2. **Spelling fix.** The code is written. It still needs to be checked in the real app before I'd call it done.
>
> Should I go ahead on both now or keep holding them until a date or event you choose? If you say go, I'll keep them moving while you're out and tell you how they turn out.

Screenshot of the guest at the end of the walk: `no-silent-holds-walk-2026-10-22-logs/end.png`.

## The lines in Rich's own transcript (UserPromptSubmit hook_additional_context)

```json
[
  {
    "type": "hook_additional_context",
    "content": [
      "HELD BY RICH, NOT BY YOU: the correction sent to fix-mate since 01:16Z (40 min). Why: its run ended before it acted on it: Fix review finding #1: the sign-in status must survive a failed usage read. Your call: go, or hold until a date or event.\nHELD BY RICH, NOT BY YOU: the work of spelling-mate since 01:16Z (40 min). Why: code done, real-app checks left. Your call: go, or hold until a date or event."
    ],
    "hookName": "UserPromptSubmit",
    "toolUseID": "hook-121d7ebb-a402-4be6-a89a-8f6627224505",
    "hookEvent": "UserPromptSubmit"
  }
]
```

The transcript's api-request-blob row (Rich's system prompt), which also carries them, is left out.

## The hold step (what was filed and the engine's own lines for that partition)

```json
{
  "records": [
    "/Users/admin/testvm/walk-bf8fa302b2e4/home/Library/Application Support/com.richos.app/engine-state/workspaces/07a00a2007e427ad61f1768d1f295fadbe01ec5afdeb6aefc142e288a0b3df0b/agents/holds-walk--spelling-mate.json",
    "/Users/admin/testvm/walk-bf8fa302b2e4/home/Library/Application Support/com.richos.app/engine-state/workspaces/07a00a2007e427ad61f1768d1f295fadbe01ec5afdeb6aefc142e288a0b3df0b/agents/holds-walk--fix-mate.json"
  ],
  "tip": "68ab283a3d0fff8e10182e1ac213bcbfe2a37ad2",
  "lines": [
    "HELD BY RICH, NOT BY YOU: the correction sent to fix-mate since 01:16Z (40 min). Why: its run ended before it acted on it: Fix review finding #1: the sign-in status must survive a failed usage read. Your call: go, or hold until a date or event.",
    "HELD BY RICH, NOT BY YOU: the work of spelling-mate since 01:16Z (40 min). Why: code done, real-app checks left. Your call: go, or hold until a date or event."
  ],
  "partition": "/Users/admin/testvm/walk-bf8fa302b2e4/home/Library/Application Support/com.richos.app/engine-state/workspaces/07a00a2007e427ad61f1768d1f295fadbe01ec5afdeb6aefc142e288a0b3df0b"
}
```

## Steps

```json
{
  "vm": "walk-bf8fa302b2e4",
  "expect_sha": "b0ca69c31",
  "steps": [
    {
      "step": "identity",
      "evidence": {
        "built_from": "b0ca69c3133cb8ce7aac1b09915d125c6816d70e"
      },
      "outcome": "PASS",
      "seconds": 0.3
    },
    {
      "step": "first-run",
      "evidence": {
        "thread": "thr_e06d6270e9494736bc35df56ea6fbf75",
        "title": "Onboarding"
      },
      "outcome": "PASS",
      "seconds": 18.7
    },
    {
      "step": "hold",
      "evidence": {
        "partition": "/Users/admin/testvm/walk-bf8fa302b2e4/home/Library/Application Support/com.richos.app/engine-state/workspaces/07a00a2007e427ad61f1768d1f295fadbe01ec5afdeb6aefc142e288a0b3df0b",
        "lines": [
          "HELD BY RICH, NOT BY YOU: the correction sent to fix-mate since 01:16Z (40 min). Why: its run ended before it acted on it: Fix review finding #1: the sign-in status must survive a failed usage read. Your call: go, or hold until a date or event.",
          "HELD BY RICH, NOT BY YOU: the work of spelling-mate since 01:16Z (40 min). Why: code done, real-app checks left. Your call: go, or hold until a date or event."
        ]
      },
      "outcome": "PASS",
      "seconds": 1.1
    },
    {
      "step": "ask",
      "evidence": {
        "turn": "turn_99dabb34c41b4984ad2707b9f191dff6",
        "session": "88ac6eea-bcef-4a33-8734-3d4b52886bc1",
        "reply": "Good morning. It's quiet: no jobs are running in this conversation and nothing is waiting on you.\n\nTwo pieces of work have been on hold for about 40 minutes, since 01:16 UTC. I'm the one holding them, so they won't move until you say go:\n\n1. **Sign-in status fix.** A code review found that the sign-in status gets lost when reading usage fails. I sent the developer the fix, but their session ended before they made it, so the fix hasn't been made yet.\n2. **Spelling fix.** The code is written. It still needs to be checked in the real app before I'd call it done.\n\nShould I go ahead on both now, or keep holding them until a date or event you choose? If you say go, I'll keep them moving while you're out and tell you how they turn out."
      },
      "outcome": "PASS",
      "seconds": 19.6
    }
  ]
}
```

## Run (boot, scenario, cleanup)

```json
{
  "vm": "walk-bf8fa302b2e4",
  "load": 4.439453125,
  "reservation": "/Users/alex/.richos-testvm/guest.lock",
  "resources": {
    "TESTVM_CPU": "4",
    "TESTVM_RAM_MB": "5120",
    "TESTVM_DISPLAY": "1680x1050"
  },
  "boot_seconds": 35.36505649983883,
  "scenario_seconds": 41.24455912504345,
  "execution": "completed",
  "scenario_exit": 0,
  "cleanup_complete": true,
  "cleanup_seconds": 2.9950162500608712,
  "elapsed_seconds": 79.60468029184267
}
```

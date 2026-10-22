# No silent holds: the app's Rich is told of held work and tells the user (2026-10-22)

The CEO: "WHEN WILL [THIS] STOP ONCE AND FOR ALL???" Plan: the private record's no-silent-holds plan of 2026-10-22 (Otto). Lead's answer to
esc-20261021T235756Z-846318be: no new UI; the hold lines reach the app's Rich as context in his
own turn and his instruction file has him tell the user.

**Command** (host, CPU-admitted by run-walk.py):

```
run-walk.py --bundle ~/.richos-nightly/releases/v1.2.2-nightly.20261021.70/RichOS-macos-aarch64.zip \
  --home <empty dir> --engine <make-engine-asset.sh output at 968d52048, the branch with main 198b1fb41 merged with nightly 70's runtime> \
  --report run-report.json --wait 3600 -- richos/app/scripts/testvm/holds-walk.py --out <dir> --expect-sha b0ca69c31
```

**Outcome:** identity, first-run, hold and ask all PASS; the guest was cleaned up (`cleanup_complete`).
Fixture data only: company "Acme", teammates `spelling-mate` (an unresolved wait) and `fix-mate`
(a correction sent after its last commit that its run never acted on), filed 40 minutes old in the
conversation's own registry partition.

## Rich's reply, as the app recorded it (conversation-ledger AssistantDelta of the turn)

> Good morning. Two pieces of work have been on hold for about 40 minutes, since 02:24 UTC and both are waiting on your call:
>
> 1. **Sign-in status fix.** A code review found that a failed usage read wipes out the sign-in status. It should keep it. I sent the correction to the teammate fixing it, but their session ended before they acted on it, so the fix hasn't been made yet.
> 2. **Spelling work.** The code is written, but it hasn't been checked in the real app yet. I won't call it ready until those checks pass.
>
> For each one, you can tell me to go ahead now or to keep holding it until a certain date or event. If you say go on both before you leave, I'll get them moving and tell you how they turn out.

Screenshot of the guest at the end of the walk: `no-silent-holds-walk-2026-10-22-logs/end.png`.

## The lines in Rich's own transcript (UserPromptSubmit hook_additional_context)

```json
[
  {
    "type": "hook_additional_context",
    "content": [
      "HELD BY RICH, NOT BY YOU: the correction sent to fix-mate since 02:24Z (40 min). Why: its run ended before it acted on it: Fix review finding #1: the sign-in status must survive a failed usage read. Your call: go, or hold until a date or event.\nHELD BY RICH, NOT BY YOU: the work of spelling-mate since 02:24Z (40 min). Why: code done, real-app checks left. Your call: go, or hold until a date or event."
    ],
    "hookName": "UserPromptSubmit",
    "toolUseID": "hook-62f9f476-38b2-41d6-9e42-05daed2521fd",
    "hookEvent": "UserPromptSubmit"
  }
]
```

The transcript's api-request-blob row (Rich's system prompt), which also carries them, is left out.

## The hold step (what was filed and the engine's own lines for that partition)

```json
{
  "records": [
    "/Users/admin/testvm/walk-297671603eff/home/Library/Application Support/com.richos.app/engine-state/workspaces/40e362a4685c3b82bfcbd8f25df9bf21990250cb2b77088b48949a62191148b2/agents/holds-walk--spelling-mate.json",
    "/Users/admin/testvm/walk-297671603eff/home/Library/Application Support/com.richos.app/engine-state/workspaces/40e362a4685c3b82bfcbd8f25df9bf21990250cb2b77088b48949a62191148b2/agents/holds-walk--fix-mate.json"
  ],
  "tip": "c3fb324fa419f722e59984b7785189b58a5eb534",
  "lines": [
    "HELD BY RICH, NOT BY YOU: the correction sent to fix-mate since 02:24Z (40 min). Why: its run ended before it acted on it: Fix review finding #1: the sign-in status must survive a failed usage read. Your call: go, or hold until a date or event.",
    "HELD BY RICH, NOT BY YOU: the work of spelling-mate since 02:24Z (40 min). Why: code done, real-app checks left. Your call: go, or hold until a date or event."
  ],
  "partition": "/Users/admin/testvm/walk-297671603eff/home/Library/Application Support/com.richos.app/engine-state/workspaces/40e362a4685c3b82bfcbd8f25df9bf21990250cb2b77088b48949a62191148b2"
}
```

## Steps

```json
{
  "vm": "walk-297671603eff",
  "expect_sha": "b0ca69c31",
  "steps": [
    {
      "step": "identity",
      "evidence": {
        "built_from": "b0ca69c3133cb8ce7aac1b09915d125c6816d70e"
      },
      "outcome": "PASS",
      "seconds": 0.2
    },
    {
      "step": "first-run",
      "evidence": {
        "thread": "thr_8b1412704a7a407796b367275eabadf4",
        "title": "Onboarding"
      },
      "outcome": "PASS",
      "seconds": 23.1
    },
    {
      "step": "hold",
      "evidence": {
        "partition": "/Users/admin/testvm/walk-297671603eff/home/Library/Application Support/com.richos.app/engine-state/workspaces/40e362a4685c3b82bfcbd8f25df9bf21990250cb2b77088b48949a62191148b2",
        "lines": [
          "HELD BY RICH, NOT BY YOU: the correction sent to fix-mate since 02:24Z (40 min). Why: its run ended before it acted on it: Fix review finding #1: the sign-in status must survive a failed usage read. Your call: go, or hold until a date or event.",
          "HELD BY RICH, NOT BY YOU: the work of spelling-mate since 02:24Z (40 min). Why: code done, real-app checks left. Your call: go, or hold until a date or event."
        ]
      },
      "outcome": "PASS",
      "seconds": 1.2
    },
    {
      "step": "ask",
      "evidence": {
        "turn": "turn_bf9e9fd5ba9745308a11784a7190faa3",
        "session": "7507b4c2-e2f1-467f-99a4-1267e3595f66",
        "reply": "Good morning. Two pieces of work have been on hold for about 40 minutes, since 02:24 UTC, and both are waiting on your call:\n\n1. **Sign-in status fix.** A code review found that a failed usage read wipes out the sign-in status. It should keep it. I sent the correction to the teammate fixing it, but their session ended before they acted on it, so the fix hasn't been made yet.\n2. **Spelling work.** The code is written, but it hasn't been checked in the real app yet. I won't call it ready until those checks pass.\n\nFor each one, you can tell me to go ahead now, or to keep holding it until a certain date or event. If you say go on both before you leave, I'll get them moving and tell you how they turn out."
      },
      "outcome": "PASS",
      "seconds": 16.2
    }
  ]
}
```

## Run (boot, scenario, cleanup)

```json
{
  "vm": "walk-297671603eff",
  "load": 13.19677734375,
  "reservation": "/Users/alex/.richos-testvm/guest.lock",
  "resources": {
    "TESTVM_CPU": "4",
    "TESTVM_RAM_MB": "5120",
    "TESTVM_DISPLAY": "1680x1050"
  },
  "boot_seconds": 43.52591670793481,
  "scenario_seconds": 42.88872829079628,
  "execution": "completed",
  "scenario_exit": 0,
  "cleanup_complete": true,
  "cleanup_seconds": 3.1429679580032825,
  "elapsed_seconds": 89.55766354105435
}
```

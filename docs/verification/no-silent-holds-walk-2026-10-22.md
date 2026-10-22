# No silent holds: the app's Rich is told of held work and tells the user (2026-10-22)

The CEO: "WHEN WILL [THIS] STOP ONCE AND FOR ALL???" Plan: the private record's no-silent-holds plan of 2026-10-22 (Otto). Lead's answer to
esc-20261021T235756Z-846318be: no new UI; the hold lines reach the app's Rich as context in his
own turn and his instruction file has him tell the user.

**Command** (host, CPU-admitted by run-walk.py):

```
run-walk.py --bundle ~/.richos-nightly/releases/v1.2.2-nightly.20261021.70/RichOS-macos-aarch64.zip \
  --home <empty dir> --engine <make-engine-asset.sh output at c53eaf643 with nightly 70's runtime> \
  --report run-report.json --wait 3600 -- richos/app/scripts/testvm/holds-walk.py --out <dir> --expect-sha b0ca69c31
```

**Outcome:** identity, first-run, hold and ask all PASS; the guest was cleaned up (`cleanup_complete`).
Fixture data only: company "Acme", teammates `spelling-mate` (an unresolved wait) and `fix-mate`
(a correction sent after its last commit that its run never acted on), filed 40 minutes old in the
conversation's own registry partition.

## Rich's reply, as the app recorded it (conversation-ledger AssistantDelta of the turn)

> Good morning. Two pieces of work are on hold, both since 00:08 UTC (about 40 minutes ago). I'm the one holding them and nothing moves until you say:
>
> 1. **Sign-in status fix (fix-mate).** A review found that the sign-in status disappears when the app fails to read usage. I sent a correction, but the worker's run ended before it made the fix, so it hasn't been made.
> 2. **Spelling work (spelling-mate).** The code is written, but it hasn't been checked in the real app yet, so it isn't ready.
>
> For each one, should I go now or keep holding until a date or event you name? If you say go before you head out, I'll start both and you'll get updates as they finish or if anything fails.
>
> I don't have any records of priorities, deadlines or open decisions for Acme, so I can't tell you anything else about today.

Screenshot of the guest at the end of the walk: `no-silent-holds-walk-2026-10-22-logs/end.png`.

## The lines in Rich's own transcript (UserPromptSubmit hook_additional_context)

```json
[
  {
    "type": "hook_additional_context",
    "content": [
      "HELD BY RICH, NOT BY YOU: the correction sent to fix-mate since 00:08Z (40 min). Why: its run ended before it acted on it: Fix review finding #1: the sign-in status must survive a failed usage read. Your call: go, or hold until a date or event.\nHELD BY RICH, NOT BY YOU: the work of spelling-mate since 00:08Z (40 min). Why: code done, real-app checks left. Your call: go, or hold until a date or event."
    ],
    "hookName": "UserPromptSubmit",
    "toolUseID": "hook-9586a509-ba74-4a4f-9774-ae42af251409",
    "hookEvent": "UserPromptSubmit"
  }
]
```

The transcript's api-request-blob row (Rich's system prompt), which also carries them, is left out.

## The hold step (what was filed and the engine's own lines for that partition)

```json
{
  "records": [
    "/Users/admin/testvm/walk-8741837ec670/home/Library/Application Support/com.richos.app/engine-state/workspaces/6387977559188f7fbaaab1a785e394a8492a97153afa42b0eee560aaef1c438e/agents/holds-walk--spelling-mate.json",
    "/Users/admin/testvm/walk-8741837ec670/home/Library/Application Support/com.richos.app/engine-state/workspaces/6387977559188f7fbaaab1a785e394a8492a97153afa42b0eee560aaef1c438e/agents/holds-walk--fix-mate.json"
  ],
  "tip": "07550abecc08abe5dc2c94b08ebd53470ea86c72",
  "lines": [
    "HELD BY RICH, NOT BY YOU: the correction sent to fix-mate since 00:08Z (40 min). Why: its run ended before it acted on it: Fix review finding #1: the sign-in status must survive a failed usage read. Your call: go, or hold until a date or event.",
    "HELD BY RICH, NOT BY YOU: the work of spelling-mate since 00:08Z (40 min). Why: code done, real-app checks left. Your call: go, or hold until a date or event."
  ],
  "partition": "/Users/admin/testvm/walk-8741837ec670/home/Library/Application Support/com.richos.app/engine-state/workspaces/6387977559188f7fbaaab1a785e394a8492a97153afa42b0eee560aaef1c438e"
}
```

## Steps

```json
{
  "vm": "walk-8741837ec670",
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
        "thread": "thr_5750aaf25a1444059034468106e82007",
        "title": "Onboarding"
      },
      "outcome": "PASS",
      "seconds": 20.0
    },
    {
      "step": "hold",
      "evidence": {
        "partition": "/Users/admin/testvm/walk-8741837ec670/home/Library/Application Support/com.richos.app/engine-state/workspaces/6387977559188f7fbaaab1a785e394a8492a97153afa42b0eee560aaef1c438e",
        "lines": [
          "HELD BY RICH, NOT BY YOU: the correction sent to fix-mate since 00:08Z (40 min). Why: its run ended before it acted on it: Fix review finding #1: the sign-in status must survive a failed usage read. Your call: go, or hold until a date or event.",
          "HELD BY RICH, NOT BY YOU: the work of spelling-mate since 00:08Z (40 min). Why: code done, real-app checks left. Your call: go, or hold until a date or event."
        ]
      },
      "outcome": "PASS",
      "seconds": 1.6
    },
    {
      "step": "ask",
      "evidence": {
        "turn": "turn_8aeb95cee8ef4e60a9c3e2d7d608464d",
        "session": "34d3f583-1676-4483-b00c-ef13a84fcff7",
        "reply": "Good morning. Two pieces of work are on hold, both since 00:08 UTC (about 40 minutes ago). I'm the one holding them, and nothing moves until you say:\n\n1. **Sign-in status fix (fix-mate).** A review found that the sign-in status disappears when the app fails to read usage. I sent a correction, but the worker's run ended before it made the fix, so it hasn't been made.\n2. **Spelling work (spelling-mate).** The code is written, but it hasn't been checked in the real app yet, so it isn't ready.\n\nFor each one, should I go now, or keep holding until a date or event you name? If you say go before you head out, I'll start both and you'll get updates as they finish or if anything fails.\n\nI don't have any records of priorities, deadlines or open decisions for Acme, so I can't tell you anything else about today."
      },
      "outcome": "PASS",
      "seconds": 21.3
    }
  ]
}
```

## Run (boot, scenario, cleanup)

```json
{
  "vm": "walk-8741837ec670",
  "load": 9.802734375,
  "reservation": "/Users/alex/.richos-testvm/guest-2.lock",
  "resources": {
    "TESTVM_CPU": "4",
    "TESTVM_RAM_MB": "5120",
    "TESTVM_DISPLAY": "1680x1050"
  },
  "boot_seconds": 64.17089462513104,
  "scenario_seconds": 45.27289316710085,
  "execution": "completed",
  "scenario_exit": 0,
  "cleanup_complete": true,
  "cleanup_seconds": 3.7196270839776844,
  "elapsed_seconds": 113.16348604205996
}
```

# Spoken reply delay: app waits repaired, full delay still open

The app's avoidable waits are repaired. Fast replies are **not fully restored**:
the measured first sample still arrived 9.955 seconds after the recorded utterance
ended. Opus, high effort, Peter Yearsley's voice and the 2.0-second end-of-turn
wait were unchanged. No comparison without high effort was run, so these results
do not establish that high effort caused the original regression.

## Changes

- The inherited replylag55/55b changes supply the continuity brief while both
  ordinary and spare conversation desks are primed. A new conversation therefore
  avoids the hidden model round trip on its first send.
- `prepare-turn` runs the existing current, bind, receipt synchronization and
  brief handlers in one Python invocation. Revision checks, per-thread seats,
  receipt-error disclosure and the final brief fence remain. The app discovers
  this capability once per lease; older engines retain the four-call path.
- The timing report accounts for preparation when a cached brief omits the hidden
  turn. It reads the model's first-text log before app word buffering, with the
  ledger fallback for old builds.
- The voice walk retains this run's original installation receipt across a
  relaunch. Previously it could wait 600 seconds for a download-success line that
  an already-installed voice correctly never printed again.

## Measured result

| Milestone | Prior baseline | After |
| --- | ---: | ---: |
| Turn handed over to first played sample | 12.664 s | 7.153 s |
| Recorded utterance end to first played sample | 15.032 s | 9.955 s |
| Hidden context records during the spoken turn | 5 | 0 |

The after-run spent 359 ms from durable prompt receipt to sending his words,
5.231 s from sending to the first model text, 320 ms assembling the first sentence
and 1.239 s synthesizing its first played sample. Recognition and the unchanged
end-of-turn wait preceded handover by 2.802 s. Rich did not talk over the sample.

The same recording and pinned small.en decoder were used. The baseline was app
`0a2ae649f` in a 5 GiB guest; the after-run was app `261664585` in an explicitly
requested 4 GiB experiment, after a typed new-conversation control. These are
separate observations, not a controlled estimate of the effect on model latency.
The local synthetic engine benchmark separately measured the four calls at
657.44 ms and the combined call at 136.64 ms.

## Verification and provenance

- 48 ECS tests passed, including idempotence, concurrent seats, receipt failure
  and a binding superseded before the final brief.
- The real-engine native preparation regression passed for the advertised
  combined request and the legacy fallback. All 18 spare-desk tests passed.
- All 26 voice-harness tests and 130 verification-input tests passed.
- The normal commit checks passed. The development app was built, signed and
  verified by `package-app.sh`; this is not a published release proof.
- One complete after-run, `walk-ce528db499a9`, passed all 12 steps, exited 0 and
  confirmed app quit, guest stop and clone deletion. It used runner main
  `283c3de59`, which contains the landed VM-memory attribution fix, scenario and
  app `261664585`, and engine SHA-256
  `9042a4d394f7ffd1e05a8c20180147e762853f75f6bc4edfa07ac5b399d9549d`.
- Two earlier own attempts were cancelled during admission, before any guest
  boot or scenario. Their logs remain separate and are not passes. The 4 GiB
  experiment was requested by Rich at 09:12:58Z after measured 5 GiB refusals;
  CPU guards and the 4 GiB host reserve stayed intact.

Evidence is retained at
`/Volumes/E1TB/ab/richos-evidence/voice-reply-delay-20261014/`:
`result.json`, `live/report.json`, `live/run.json`, `live/reply-split.json`, raw
guest logs and journals, the prior baseline, admission logs, test logs and the
development artifacts. Verification is assembled from the individual checks
above; the final VM walk itself was one clean completed invocation. The normal
second review and landing gates remain Rich's work.

The remaining performance issue is the full delay, including the provider's
time to first text. This change must not be presented as restoring instant replies.

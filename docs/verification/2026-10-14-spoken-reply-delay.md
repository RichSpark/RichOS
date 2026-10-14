# Spoken reply delay: app waits repaired, full delay still open

The app's avoidable waits are repaired. Fast replies are **not fully restored**:
the measured first sample still arrived 9.955 seconds after the recorded utterance
ended. Opus, high effort, Peter Yearsley's voice and the 2.0-second end-of-turn
wait were unchanged. The first proof did not compare effort levels. The later
comparison below identifies an observed contribution from the flag, without
establishing the complete cause of the historical regression.

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

## Later effort comparison and decoder isolation

Rich requested a measurement-only comparison at 09:26:52Z. Both conditions used
the same app `261664585`, engine, recording, decoder and 5 GiB guest RAM. Each used
the same transparent launcher; one preserved `--effort high`, while the other
removed exactly that flag/value pair. Live audits verified that `--model opus`
and empty `--setting-sources` remained. No production setting changed.

| Phase | Explicit high | Effort flag omitted |
| --- | ---: | ---: |
| Prompt receipt to model send | 0.508 s | 0.604 s |
| Model send to first text | 5.974 s | 3.364 s |
| First text to first sentence | 0.274 s | 0.307 s |
| First sentence to first played sample | 1.504 s | 1.600 s |
| Handover to first played sample | 8.264 s | 5.876 s |
| Recorded utterance end to first played sample | 13.261 s | 35.959 s |

Both walks passed all 12 steps, returned 0 and confirmed cleanup. They ran
sequentially because the audio-capture test held the other VM slot. One sample per
condition points to the effort flag contributing 2.610 s of observed model wait;
it does not establish a stable expected improvement or reproduce pre-October-10
startup, which also omitted the model flag. Omitted effort means the CLI's
unspecified default, not a named low effort level.

The omitted condition also exposed a **separate recognition stall** before model
submission: its three fragments took 20.569, 11.759 and 6.636 s, and handover came
30.080 s after the final recorded utterance. It must not be described as a
5.876-second full voice response. Its model flag cannot explain work that
preceded model submission.

The smallest isolated follow-up reproduced the cold-invocation spike with no app
or provider turn: the first four seconds of the same recording decoded in
19.356 s, then 3.034 and 3.398 s, with identical text. A second isolated guest,
omitting only the no-print flag to retain native phase diagnostics, measured
10.587 s cold and 2.080 s warm. Metal library initialization on its virtual GPU
took 7.844 s cold and 0.008 s warm; ordinary model loading took 0.293 s, and the
first native decode total was 2.572 s. This identifies GPU initialization as a
cold VM cost, not evidence that the effort flag caused the recognizer stall.
No decoder algorithm, model or thread setting was changed.

The additional evidence is in `effort-comparison-result.json`, `ab-high/`,
`ab-omitted/`, `launcher-control.json`, `decoder-isolation/` and
`decoder-phases/` under the retained evidence directory. The test-only launcher
and diagnostic scripts are retained there for inspection.

The remaining performance issue is the full delay, including the provider's
time to first text and cold decoder initialization. This change must not be
presented as restoring instant replies.

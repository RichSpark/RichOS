# Spoken replies: preserve fast startup and diagnose the actual wait

The October 14, 2026 investigation found separate delays. Do not treat one as an explanation
for all of them, or repeat the investigation without reading this record and its evidence.
The user accepted a normal short response wait and specifically rejected spending time
optimizing a 1.38-second synthesis as the main problem. Restore responsive conversation.

## Confirmed observations

- October 10 changed leases from provider defaults to explicit model and high-effort flags
  (`fb7de54f0`). In the October 14 matched application experiment, model-send to first text
  took 5.974 seconds at high effort versus 3.364 with the effort flag omitted. Omission does
  not prove a particular named effort or the historical account default. Low-effort Opus
  subsequently produced typed first text in 0.978 seconds and spoken first text in 1.962.
- An unnecessary hidden continuity model turn and repeated interpreter preparation had
  added time before the visible request. The ordinary and spare conversation leases must
  receive their continuity brief during priming, and reuse it at the first real request.
- On the physical Mac, a fresh packaged speech decoder took 9.764 seconds on first use,
  then 0.932 and 0.879 seconds on the same padded recording and unchanged production flags.
  A separate VM phase profile identified 7.844 seconds loading the embedded Metal library
  on its first run versus 0.008 when warm. The physical first-use delay is measured;
  attributing its entire cost specifically to Metal is an inference from that VM profile.
- A warm binary version check, pinned weight hashes, cached hardware calibration or a
  successful download is not evidence that the decoder has initialized. The decoder must
  actually run before the first user utterance. Otherwise the first reply pays startup.

Evidence and exact identities: [the investigation record](../verification/2026-10-14-spoken-reply-delay.md)
and [the follow-up record](../verification/2026-10-14-instant-replies.md).
The [accepted real Mac microphone benchmark](../verification/2026-10-14-real-mac-voice-benchmark.md)
preserves the three live turns Alex judged to work well, their component timings,
hardware, models and thinking witnesses. Use its original observations for future
comparisons, keeping cold launch, changed briefs, VM runs and different speech models separate.
The retained raw evidence is outside disposable scratch under the external SSD's
`richos-evidence` directories. Preserve failed attempts and distinguish VM and physical-host
measurements. These observations do not promise a fixed network latency on every request.

## Required startup behavior

Readiness starts a bounded, silent initialization of the selected pinned decoder in the
background. Starting voice capture joins that initialization before opening the microphone.
Only a successful decode counts. A failure is not cached as success. A changed verified
binary or model requires new initialization. Temporary silence and decoder output are
removed on success and failure; none becomes a conversation turn. Use the production model,
thread count and decoder flags. Do not weaken recognition or turn-taking to hide a wait.

Conversation replies use low effort, as authorized on October 14. Background work keeps
high effort. Keep Opus thinking enabled. The user rejected disabling thinking as a latency
fix on October 14: fast replies still need to be capable. No app-level benefit from disabling
thinking was measured. Model choices remain the choices saved by the user. Preserve the
app's complete existing settings, including permission boundaries. A test launcher that
overwrites them is invalid evidence.

Keep the agreed two-second end-of-turn silence and all fragment assembly and barge-in rules.
An answer spoken over an unfinished thought is a failure, even if it is quick. Do not add
filler, fake acknowledgments or sounds to manufacture a fast first-sample number.

## Diagnose by milestones

Read the current voice log, conversation ledger and machinery before running more tests.
Measure the user's last admitted utterance end, the end-of-turn silence, final recognition,
handover, model send, first text, first complete sentence and first played sample separately.
`ANSWER AUDIBLE` marks sentence handover to synthesis. Actual audio begins at
`playout chunk START`. First PCM and complete sentence synthesis are also different facts.
Always report the complete user-visible wait alongside any smaller internal interval.

A slow first decode followed by fast decodes needs startup initialization evidence. A fast
recognizer followed by thought frames and a long model wait needs a provider investigation.
An absent spare brief needs continuity preparation investigation. Use the smallest affected
check; do not rerun passing suites just because a later stage failed. Follow
[verification retries](verification-retries.md).

## Verification that prevents this being forgotten

Run `decoder_warmup_is_silent_once_per_identity_and_a_failure_is_not_cached` for initialization,
identity changes, cleanup and failed initialization recovery. Run the voice-walk parser suite
for initialization ordering, complete timing and early-speech refusal. The full new-app
voice walk must use `--expect-warmed-decoder`: a real initialization completion line must
precede `voice session OPEN`. A decoder warmed after capture or on the first utterance fails.
Use a new guest and the packaged decoder for first-use coverage. Reused warm-host timings
cannot qualify a changed decoder's startup. Preserve its build, engine and model hashes,
provider version and flags, recording identity, resource settings and cleanup outcome.

Run the real app through the owned test-VM runner with guest output muted. Silent file-only
recognition can run on the physical host. Host speakers cannot stand in for a person at the
microphone. Do not claim VM timing predicts physical-host latency or count ignored live
checks as passes. Keep performance evidence and required release gates distinct.

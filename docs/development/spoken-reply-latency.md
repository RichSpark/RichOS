# Spoken replies: preserve startup and diagnose the actual wait

Decoder initialization, recognition, turn closure, continuity preparation, model
output and speech playback are separate stages. Diagnose the stage that actually
blocks the reply. Technical measurements belong here when they contain no personal
identifiers, quotations or machine details. Keep identifying run records, recordings
and local evidence paths in private verification records.

## Measured causes and boundaries

The October 14, 2026 investigation measured three distinct delays:

- Explicit high-effort leases introduced by `fb7de54f0` increased model wait.
  A matched application experiment measured model-send to first text at 5.974 s
  with high effort versus 3.364 s with the effort flag omitted. Omission does not
  establish a named effort level or the historical account default. Later
  low-effort Opus samples reached typed first text in 0.978 s and spoken first
  text in 1.962 s. Those later samples are separate observations, not the matched
  experiment. Thinking remains enabled; no app-level benefit from disabling it
  was measured.
- A changed continuity brief caused an additional foreground context-only model
  request. A recorded-input physical-host run measured 3.135 s in that request
  and 7.907 s from the final admitted fragment end to playback activity. Sending
  changed continuity inside the visible request removed that extra request;
  a later changed-context run measured 4.818 s at the same endpoint. Background
  work and load differed, so the total difference is not a controlled estimate
  of the fix's effect. The structural extra request is the demonstrated blocker.
- A fresh packaged decoder took 9.764 s on first use, then 0.932 s and 0.879 s
  on the same padded recording with unchanged production flags. A separate VM
  profile measured embedded Metal library loading at 7.844 s cold versus 0.008 s
  warm. The first-use delay is measured; attributing its entire cost to Metal
  is an inference from the VM profile. Real initialization must precede capture.

### Live-microphone baseline

Three baseline turns with unchanged continuity produced the following disjoint
wall-clock intervals. Values are milliseconds. These are warm physical-host
observations, not cold-start or changed-context coverage.

| Interval | Turn 1 | Turn 2 | Turn 3 |
| --- | ---: | ---: | ---: |
| Final decoder execution | 571 | 634 | 599 |
| Other recognition work | 13 | 23 | 33 |
| Remaining turn closure after recognition | 603 | 557 | 571 |
| Continuity brief preparation | 196 | 201 | 224 |
| Other foreground preparation | 35 | 47 | 38 |
| Model send to first text | 2057 | 1700 | 1528 |
| First text to first complete sentence | 25 | 395 | 415 |
| Sentence handover to playback activity | 137 | 117 | 164 |
| Final admitted fragment end to playback activity | **3637** | **3674** | **3572** |
| Handover to playback activity | **2450** | **2460** | **2369** |

The two-second silence rule overlaps recognition and fragment assembly. Add only
its remaining post-recognition portion when reconstructing the total above.
The fragment-end marker follows voice activity detection; it is not an
independently measured final acoustic word. Playback activity is the supervisor
queue marker, not an acoustic microphone measurement. Separate first-PCM
stopwatches measured 148/120/163 ms and must not be added to these totals.

### Changed-continuity and first-use coverage

A separate recorded-input physical-host test exercised a real continuity change
between requests. Its first turn registered work; its second turn recognized
that existing assignment instead of starting it again. Both had zero hidden
foreground model requests and visible thinking frames.

| Interval, milliseconds | First turn | Changed continuity |
| --- | ---: | ---: |
| Final decoder execution | 639 | 1187 |
| Other recognition work | 33 | 27 |
| Remaining turn closure | 534 | 26 |
| Continuity brief preparation | 196 | 684 |
| Other foreground preparation | 41 | 52 |
| Model send to first text | 4952 | 2124 |
| First text to first complete sentence | 1236 | 313 |
| Sentence handover to playback activity | 197 | 405 |
| Final admitted fragment end to playback activity | **7828** | **4818** |
| Handover to playback activity | **6622** | **3578** |

Cold decoder initialization completed in 9749 ms before capture in that test.
A fresh packaged-app VM walk separately passed all twelve required steps with
`--expect-warmed-decoder`: initialization completed in 11935 ms before capture,
recognition produced one exact transcript, speech used Pocket and the two visible
thought frames contained no hidden foreground request. Its fragment-end to
playback interval was 9005 ms and handover to playback was 4252 ms. VM numbers
are separate observations, not predictions of physical-host latency.

Preserve the original baseline rather than replacing it with slower later
samples. Compare like conditions: cold versus warm, live microphone versus
recorded input, unchanged versus changed continuity, speech model, provider flags
and foreground versus background work. These observations do not guarantee
identical network response times. A passing isolated app or VM walk does not
prove the fixed release has been published, installed or accepted end to end.

## Required startup behavior

Readiness starts bounded, silent initialization of the selected pinned decoder
in the background. Starting voice capture joins that initialization before
opening the microphone. Only a successful decode counts. A failure is not
cached as success. A changed verified binary or model requires new initialization.
Temporary silence and decoder output are removed on success and failure; none
becomes a conversation turn. Use the production model, thread count and decoder
flags. Do not weaken recognition or turn-taking to hide a wait.

A binary version check, pinned weight hashes, cached hardware calibration or a
successful download does not prove first-use initialization. The packaged decoder
must actually run before the first user utterance.

Conversation replies use low effort. Background work keeps high effort. Keep
Opus thinking enabled and preserve the model choices saved by the user. Preserve
complete existing settings and permission boundaries. A test launcher that
overwrites them is invalid evidence.

Keep the two-second end-of-turn silence, fragment assembly and barge-in rules.
An answer spoken over an unfinished thought is a failure. Do not add filler,
fake acknowledgments or sounds to manufacture a fast first-sample number.

## Preserve current context without another foreground turn

Prime ordinary and spare conversation leases before a visible request. Reuse
unchanged held continuity. When authoritative dialogue changes the brief, queue
that brief and attach it as a separate data block alongside the exact original
user message in the same visible request. Historical context grants no authority.
Ledger entries, instruction digests, bindings and tool grants still derive from
the original user request. Do not issue a separate foreground context-only model
request or discard the changed brief to make a reply appear faster.

Clear stale pending context when preparing a new turn. Mark a new brief identity
held only after a completed request. Cancellation, refusal and disconnection
must not qualify the brief as received. Ahead-of-time priming remains separate
from visible request handling.

## Startup and onboarding preparation

Begin preparing the selected conversation during launch and immediately after a
company is confirmed. Preparing only a spare for a future thread leaves the
conversation actually restored on screen cold. Each conversation owns its
connection during startup, continuity priming, rotation
handoff, work-scope preparation and provider retry waits. Those operations run
outside the shared state lock. Ledger and snapshot changes take short locked
commits. Priming claims are scoped to their conversation, so preparing one does
not block navigation, intake or another conversation's preparation and reply.
Do not guess a company or reuse a provider spawned with another company's
permissions to start earlier.

Deliver initial session history and continuity in one hidden priming request.
Keep all mutation grants closed and qualify the held brief only after completion.
A cancelled or failed prime must not populate the context cache. Adapters that
cannot combine these inputs retain their explicit preparation fallback.

Saved interview answers update company material. Carry that current snapshot
with the continuity data in the next visible request instead of invalidating the
session and issuing another foreground re-prime. Keep the original user message,
ledger text and permission instruction exact. Removing obsolete company material
must replace its previous snapshot rather than append contradictory facts.

Automatic recognizer selection is recorded in provenance rather than announced
as a conversation card. Readiness logs describe the observed state and must not
predict a future download offer when setup may already be downloading the model.
A top available model can exceed the time budget when no faster installed model
is available; log the actual comparison rather than declaring it within budget.

## Diagnose by milestones

Read the current voice log, conversation ledger and machinery before more tests.
Measure the last admitted utterance end, end-of-turn silence, final recognition,
handover, model send, first text, first complete sentence and first played sample
separately. Recognition and the silence rule overlap.

`ANSWER AUDIBLE` marks sentence handover to synthesis. `playout chunk START` is
the supervisor queue-activity marker, not an independent acoustic-onset
measurement. First PCM and complete sentence synthesis are different facts.
Report the complete user-visible wait alongside smaller internal intervals and
state the endpoint used. Do not sum overlapping intervals or cross-thread
stopwatches into a wall-clock total.

A slow first decode followed by fast decodes needs startup initialization
evidence. A fast recognizer followed by thought frames and a long model wait
needs a provider investigation. An absent spare brief or extra context request
needs continuity preparation investigation. Use the smallest affected check and
follow [verification retries](verification-retries.md).

## Required verification

Run `decoder_warmup_is_silent_once_per_identity_and_a_failure_is_not_cached` for
initialization, identity changes, cleanup and failed initialization recovery. Run
the voice-walk parser suite for initialization ordering, complete timing and
early-speech refusal. The full new-app voice walk must use
`--expect-warmed-decoder`: a real initialization completion line must precede
`voice session OPEN`. A decoder warmed after capture or on the first utterance
fails. Use a fresh guest and packaged decoder for first-use coverage. Warm-host
timings cannot qualify a changed decoder's startup. Preserve build, engine and
model hashes, provider version and flags, recording identity, resource settings
and cleanup outcome in private verification evidence.

Exercise a real changed engine record between consecutive requests. Verify that
the new record reaches the next visible provider message, the original user
instruction remains exact, no foreground context-only request is sent and the
unchanged next brief stays cached. Retain cancellation, permission and
mutation-grant coverage. A cached-brief sample alone does not cover changed
continuity.

Run the real app through the owned test-VM runner with guest output muted. Silent
file-only recognition can run on a physical host. Speakers cannot stand in for a
person at the microphone. Do not claim VM timing predicts physical-host latency
or count ignored live checks as passes. Keep performance evidence and required
release gates distinct.

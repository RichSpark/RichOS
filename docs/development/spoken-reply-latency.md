# Spoken replies: preserve startup and diagnose the actual wait

Decoder initialization, recognition, turn closure, continuity preparation, model
output and speech playback are separate stages. Diagnose the stage that actually
blocks the reply. Keep run records, measurements, recordings and machine details
in private verification records rather than this public repository.

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

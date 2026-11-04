# Built-in diagnostics

Relevant diagnostic logging is part of a feature's implementation. Record enough
bounded evidence to distinguish a failure, a slow stage and missing coverage
without guessing or asking a user to reproduce ordinary use with special flags.
Reuse existing local logs and the Bust a bug / RichDoctor collection paths.

For latency, record explicit monotonic durations and name the start and endpoint.
Include waiting, cancellation and cleanup when they occur in the user's wait.
A decoder-only stopwatch must never be labeled end-to-end latency. Correlate
stages within one operation using an ephemeral local ID. Include build version,
relevant feature state and finite outcomes. Record actual failure categories at
the point they are known, including unavailable or unmeasured states.

Keep normal diagnostic entries free of speech, document text, prompts, credentials,
company names and arbitrary app content. Bound log size and collector reads.
Diagnostics must not introduce a microphone, network request, model call, busy
poller or visible choice. A failed diagnostic write must not fail the operation.
Reuse the existing report scrubber, review and send controls before sharing any
collected material. Local collection does not authorize automatic public upload.

Every changed feature must identify where its useful evidence is recorded and
how support can obtain it. Verify that collection preserves the necessary fields,
respects time/size bounds and represents missing evidence honestly. Test the real
producer and collector with synthetic fixtures. Log presence alone does not prove
quality, a causal speed regression or a product acceptance threshold.

For RichScribe, correlate capture, first preview result, preview pause, Finish,
preview reaping, final recognition and delivery. Report the sampled-window age
as that measurement, not as the acoustic age of a recognized word or proof that
a pixel reached the display. On/off comparisons remain necessary to estimate
preview's added cost. Resource and hardware qualification remains separate.

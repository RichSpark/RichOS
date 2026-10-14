# Accepted real Mac spoken-reply benchmark

On October 14, 2026 Alex spoke into the real microphone and heard the real
speakers. His verdict was **“OK, that works great.”** He requested preserving
every component timing as the benchmark for when the real Mac works well.

The preserved evidence is
`/Volumes/E1TB/ab/richos-evidence/host-live-mic-20261014/`.
Its `README.md`, `benchmark.json`, `timings.csv`, `extract-benchmark.py`,
`app-resumed.log`, hardware metadata and `SHA256SUMS` retain the complete
measurement. The isolated ledger and machinery journals remain private there.
This document contains the timing summary, without the live conversation.

## Identity and conditions

App source: `edec90eccfb3d7934597dad836ed4544cfcd8768`, the same runtime
implementation described in [the implementation record](2026-10-14-instant-replies.md).
Mac mini M4, Mac16,10, 10 cores (4 performance/6 efficiency), 24 GB RAM.
Real Elgato Wave:3 input at 96 kHz mono and real Mac mini Speakers output
at 48 kHz stereo. Isolated synthetic test company and resumed conversation.
No recording injection or guest VM.

Claude Code 2.1.299 reports `claude-opus-5-5`. Conversation low effort,
Work high effort and thinking enabled. The three visible turns contain
1, 3 and 2 thinking frames. Speech recognition used pinned whisper.cpp
1.9.1 and **small.en explicitly selected for this test**. Actual synthesis
used Pocket TTS english_2026-04 INT8, Peter Yearsley and 2 threads.
The full report records artifact hashes and OS identity. A run with another
speech model or another configuration is a separate comparison condition.

## Component timings

Milliseconds, three actual accepted turns in chronological order:

| Component | Turn 1 | Turn 2 | Turn 3 |
| --- | ---: | ---: | ---: |
| Speech decoder | 571 | 634 | 599 |
| Other recognition-side elapsed time | 13 | 23 | 33 |
| Remaining turn-closure wait after recognition | 603 | 557 | 571 |
| Continuity brief read | 196 | 201 | 224 |
| Other handover-to-model-send preparation | 35 | 47 | 38 |
| Model send to first text | 2057 | 1700 | 1528 |
| First text to sentence handoff | 25 | 395 | 415 |
| Sentence handoff to playback START marker | 137 | 117 | 164 |
| **Fragment END to playback START marker** | **3637** | **3674** | **3572** |
| **Turn handover to playback START marker** | **2450** | **2460** | **2369** |

The eight component rows add exactly to each fragment-END total. Recognition
including surrounding elapsed time was 584, 657 and 632 ms. Handover to
model send was 231, 248 and 262 ms. Admitted audio lengths were 4256,
8384 and 8784 ms. No foreground hidden continuity model turn occurred in
these three samples: the session reused its existing brief.

Decoder initialization completed before capture in 1075 ms. Pocket loaded
and verified in 956 ms before the first synthesis. Front desk readiness
was 6817 ms and spare readiness 13074 ms, outside these reply intervals.
First-sentence PCM stopwatches were 148, 120 and 163 ms. They are separate
internal thread counters, not additional additive waits after the final row.

The complete evidence preserves millisecond UTC event timestamps, each
sentence's generation and speech duration, every playback marker and
turn completion. Accepted handover to model/engine turn completion took
5488, 8164 and 10661 ms; speech began before that completion.

## Boundaries and future use

The endpoint is the supervisor's `playout chunk START` queue marker,
not independently recorded sound arrival at the ear. `ANSWER AUDIBLE`
opens the half-duplex window at sentence handoff. Reported output buffering
was 10.7 ms, input buffering 5.3 ms, slicer 16.0 ms and post-speech hold
32 ms. No independent acoustic-onset measurement was made.

The two-second silence check overlaps recognition. Fragment END contains
VAD hangover and occurs after the user's final word. Do not add two more
seconds to the recognition row or label that marker an exact last-word
timestamp. Provider queue, network transit and internal model thinking
are included in the measured model interval, not separately instrumented.
The live microphone waveform was not recorded for replay; these are observed
turns rather than a bit-identical audio fixture for a future run.

Speaker-tainted audio was discarded with an explanatory status. A later
sentence also followed the first turn's initial short response. Preserve
those events; this accepted responsiveness benchmark is not proof of
flawless barge-in or a completed useful answer at the first playback marker.

An earlier real Mac **recorded-input** run demonstrated a separate hidden
continuity refresh when the brief changed, adding 3135 ms on its second
turn. Its evidence remains in
`/Volumes/E1TB/ab/richos-evidence/host-reply-breakdown-20261014/`.
That condition is still relevant and must not be hidden by the three
cached-brief samples here.

Keep this original accepted baseline. Compare like-for-like hardware,
speech models, thinking, effort, decoder warmth, context preparation and
endpoints. Record all turns and brief invalidations. Separate cold startup,
VM runs and injected recordings. Three observations establish neither
percentiles nor an automatic regression threshold. Landing and installation
remain separate from this development-app test.

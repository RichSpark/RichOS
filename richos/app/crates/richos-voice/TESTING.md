# Testing richos-voice on this Mac

**AUDIO TESTING ON THIS MAC — a standing limitation (CEO, 2026-09-18).** The Mac's own speakers are the only sound source an agent has. Anything played with `say`, `afplay` or the app's own playout goes through the SAME loudspeaker as Rich's voice, takes the same acoustic path into the microphone at the same level, and is therefore indistinguishable from Rich's echo to the canceller and to every gate downstream of it. It cannot stand in for a person at the microphone. So an interruption, barge-in, talk-over or "the CEO speaks while Rich speaks" test is NEVER done with `say` on this Mac. It is done either (a) on the committed echo-path fixture with a near-end voice recording superimposed on the microphone track at a stated near-field level, (b) by a human at the desk, or (c) by a separate sound source (a second device) near the microphone. A defect "found" with the Mac's own speakers standing in for the human is a harness artifact until reproduced by (a), (b) or (c) — that is exactly how the candidate-.6 "onset defect" of 2026-09-17 came about. Barge-in itself has existed since 2026-08-29 (`bargein.rs`). And at most ONE live playback per measurement, never a loop (CEO, 2026-09-17).

The committed echo-path fixture is `tests/fixtures/echo-path/ceo-rig-2026-09-17-{mic,reference}.wav` (captured at the walk's volume; see `tests/echo_path_replay.rs` and `tests/self_voice_replay.rs`). A near-end voice is superimposed on the mic track, never played through the speakers. Full ruling: richos-hq `wiki/ceo-decisions.md` §53.

## First-use speech recognition and reply latency

Read [the startup and diagnosis record](../../../../docs/development/spoken-reply-latency.md)
before latency work. A fresh packaged decoder measured 9.764 seconds on this Mac, then
0.932 and 0.879 when warm. Binary metadata and cached calibration are insufficient.
Rust setup initializes the real decoder silently before frontend state reads and joins it
before capture. The full application proof uses `voice-walk.py --expect-warmed-decoder`
and refuses initialization that finishes after capture opens. The dedicated decoder
warmup unit verifies successful reuse, identity changes, failure recovery and cleanup.
The existing physical audio limitations below still apply.

## The three echo-path fixtures, and which question each one answers

```text
  ceo-rig-2026-09-17-{mic,reference}.wav             9.536 s   echo only, the CEO's morning volume
  ceo-rig-2026-09-18-nearend-{mic,reference}.wav    25.579 s   echo only, OUTPUT VOLUME 60
  ceo-rig-2026-09-18-nearend2-{mic,reference}.wav   24.784 s   THE CEO'S OWN VOICE over Rich
```

### One of those six files is NOT in this repository

`ceo-rig-2026-09-18-nearend2-mic.wav` is the microphone track with the CEO speaking into it. This
is a public repository and nothing carrying his voice goes into one, so that ONE file lives in the
private `richos-hq` repository at `fixtures/echo-path/`:

```text
  793132 bytes, PCM16 16 kHz mono, 396544 samples = 24.784 s
  sha256 786adf8073d0eee99b5a1d639abc5f459e2f6c1284188ab33d75ba8c1f93cfe3
```

**Everything else of that pair is here**, and that this is safe was measured rather than assumed
(`docs/verification/2026-09-18-the-ceos-voice-moves-to-the-private-repository.md`):
`-nearend2-reference.wav` is the signal that went to the DAC, and it matches the previous run's
reference track at exactly the expected 12 800-sample offset with a -37.5 dBFS residual that is
flat to within -0.0 dB inside and outside the intervals where he speaks. The sidecars carry his
intervals and his levels and **not one word he said**.

**Pointing the tests at it** — option 1 is the one to use from a git worktree, because a worktree's
parent directory is not where option 2 looks:

```sh
RICHOS_PRIVATE_FIXTURES=/path/to/richos-hq/fixtures/echo-path \
    cargo test -p richos-voice --release
```

1. `$RICHOS_PRIVATE_FIXTURES` — a directory.
2. `<repo-root>/../richos-hq/fixtures/echo-path` — the side-by-side checkout layout, no setup.

**Without it nothing fails and nothing is faked.** `the_ceos_own_voice_over_rich_interrupts_him_\
and_his_words_are_kept` reports `ignored, PRIVATE FIXTURE: …` naming the file, and is counted in
libtest's own `N ignored` column — it never prints `ok`, for the reason `build.rs` exists (a test
that did not run must not report a pass). The other two tests in that file, **including the
false-positive test that proves Rich never cuts himself off**, need no private file and run
everywhere. `examples/bargein_score.rs` resolves the same way but prints the reason and **exits 2**
rather than skipping: an operator asked it for a table, and a harness that scored nothing must not
exit 0.

If a run reports `ignored` when you believe the file is present, `touch build.rs` and re-run — the
cfg is decided at compile time and a file appearing is not something Cargo can always notice.

Each pair carries a `-session.json` sidecar written by the harness at capture time: both UTC
instants, the playback offset and duration, both device labels with rates and channel counts, the
`say` voice and rate, and the operator-reported output volume. `-nearend2` also carries
`-nearend2-nearend.json`, which states that his voice is in the microphone track, where (five
intervals, 4.880 s in total), at what level (**-18.1 dBFS**, against **-46.7 dBFS** of echo on the
same microphone — **28.6 dB** above it), and by exactly what criterion those intervals were
derived. **No transcript of his words is kept anywhere.**

**WHICH FIXTURE ANSWERS WHICH QUESTION IS NOT INTERCHANGEABLE.**

* **False positives — "does Rich interrupt himself?"** — are measured ONLY on the two recordings
  that contain no near-end voice at all. Never on `-nearend2`: the interval marker there claims
  only frames above the echo-only baseline's maximum, so his quietest speech falls outside the
  intervals and the complement of them is contaminated with his voice. It is not an echo-only set
  and using it as one would read his own breath as a false positive.
* **Misses — "is the CEO heard?"** — are measured on `-nearend2`, because that is the only
  recording with a real person in it. A `say`-through-the-speakers "interruption" reaches the
  microphone at the echo's own level (-44.7 dBFS, measured) and a gate that fires on it fires on
  Rich; that is the §53 ruling restated as a measurement.

`tests/barge_in_on_the_ceos_rig.rs` holds both sides, each carrying the other's positive control.
`examples/bargein_score.rs` is the offline instrument that chose the rule from these recordings;
it opens no device and plays nothing.

## Reproducing or replacing a fixture — the ONE command that makes a sound

```text
  RICHOS_VOICE_LIVE_AUDIO=1 cargo run -p richos-voice --release --example aec_capture -- \
      --save tests/fixtures/echo-path/<name> \
      [--text-file <path>] [--lead <ms>] [--output-volume <0-100>]
```

`--text-file` speaks a longer passage so a cue relayed to a human at the desk has room to land
inside the playback; synthesis happens BEFORE the microphone opens, so `PLAYBACK START` is exactly
`--lead` ms after `RECORDING START` and both are printed in UTC. `--output-volume` is RECORDED in
the sidecar and is **not set** by the tool — read the machine's volume yourself, before and after,
and restore it if you changed it. ONE playback per measurement, never a loop.

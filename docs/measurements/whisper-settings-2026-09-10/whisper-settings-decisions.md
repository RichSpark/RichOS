# Whisper decode settings — the decision table

**Every setting the transcription pipeline hands to `whisper-cli` and every setting it leaves
unset, decided deliberately.** One row per flag: what the vendor does, what we ship and the
measurement or argument that makes ours the better one.

**The rule this answers,** CEO, 2026-09-10:

> "NEVER use the **default** settings of ANY third-party tool/software for ANYTHING. UNLESS the
> default settings have proven the be the best possible settings."

So a row whose reason is "this is the default" is acceptable **only** where the comparison that
proves it is on disk. Every such row below cites the run.

- The rig, the corpora and their limits: [`README.md`](README.md)
- The runs: [`measurements/`](measurements) — `shortcall-sweep.txt`, `longform.txt`,
  `name-hits.txt`, `identical-to-baseline.txt`, `dictation-probe.txt`, `rig-validation.txt`
- The real command lines: `measurements/raw/*/*.argv*.json`, recorded by the shim from the process
  that ran, never transcribed from source

**Vendor build every "vendor default" column refers to:** whisper.cpp **1.9.1**, ggml **0.17.0**,
Apple M4. Defaults are a property of a build, not of a product — §6 is about that.

---

## How to read the evidence column

| Token | Means |
|---|---|
| **MEASURED** | a run in `measurements/` here, today, against the alternative |
| **MEASURED (record)** | a run in the earlier briefs, cited by file; re-derived here where the conclusion leans on it |
| **ARGUED** | no measurement decides it — the reason is structural (the flag would break a contract or its input does not exist). Stated as an argument, never dressed as a result. |

**The baseline every short-call figure is compared to is the shipping configuration**, which scores
**WER 2.89%** (S40 D7 I8 over 1,905 reference tokens, 152 segments, 76 s) on the 6-call corpus.
Twelve configurations produce output **byte-identical** to it (sha256 `b8de7bb389f5…`); that is the
strong form of "changed nothing" and `identical-to-baseline.txt` is where it is checked.

---

## 1. What the pipeline passes today

Captured from a full `richos-service run`, not read out of `config.js`:

```
-m <model> -f <wav> -l en -t 4 -mc 0 -oj -np -ojf -of <base>
```

and the deletion detector's clip probe, same decode flags, no `-ojf`/`-of`, several files at once:

```
-m <model> -l en -t 4 -mc 0 -oj -np <clip0> <clip1>
```

plus one `--version` per session from `whisperVersion()` — the identity probe, §4.1.

And, since 2026-10-04, the restyle re-decode (stage 3.9, §9): one clip per run-on span, the main
pass's decode flags, then a prompt budget and the transcript's own preceding text:

```
-m <model> -f <clip> -l en -t 4 -mc 0 -oj -np -fa -mc 100 --prompt <preceding text> --carry-initial-prompt -of <base>
```

| Flag | Vendor default | We ship | Why ours, with evidence |
|---|---|---|---|
| `-m` | `models/ggml-base.en.bin` | resolved model path | **ARGUED.** The vendor default names a file that does not exist on this machine. `resolveModelChecked()` picks by id and refuses a candidate failing size/GGML-magic against its pin. |
| `-f` | none | the channel WAV | **ARGUED.** One decode per channel is what makes every segment speaker-attributed before the merge, with no diarization model. |
| `-l en` | `en` (same value) | `en`, explicitly | **MEASURED.** `-l auto` is byte-identical on this corpus and **20% slower** (91 s vs 76 s): the detect pass is pure cost on known-English audio. Pinned rather than inherited because `auto` is one upstream edit away from being the default and a mis-detected language changes every word. |
| `-t 4` | `4` (same value) | `4`, explicitly | **MEASURED.** `-t 8` is byte-identical **and the same wall clock** (76 s both) — the decode is Metal-bound, so threads past 4 buy nothing. Pinned so the value does not follow the vendor's idea of this machine. |
| `-mc 0` | `-1` (carry everything) | `0` | **MEASURED, re-derived today on the 92-minute real recording.** `-mc 0`: 0 loop findings, 0.0 s, 2 silence fabrications, 226 s. `-mc -1`: 9 findings, **429.8 s = 7.8% of the timeline**, 130 silence fabrications, 509 s — including one sentence repeated **203 times** across 290 s. The 2,008 extra words `-mc -1` emits are not recovered speech: the guard removes 1,493 of them. On short calls the two are within noise today (2.78% vs 2.89%), so this costs nothing at call length and prevents everything at conference length. §5 holds what it does cost. |
| `-oj` | off | on | **ARGUED.** The merge needs per-segment timestamps; the plain text output has none. |
| `-ojf` | off | on | **MEASURED (record).** Adds per-token offsets and nothing else — proven byte-identical on the 92-minute corpus (`transcribe.js:117-122`). The deletion detector localizes on word times; scored on segment extents instead it invents deletions (8 of 9 detected vs 6 of 9). |
| `-np` | off | on | **ARGUED.** Progress prints on stdout would corrupt the log; the transcript is read from the JSON file, never from stdout. |
| `-of` | none | the channel base path | **ARGUED.** Contract with the caller. Deliberately **not** passed on the multi-file clip probe, where one output base would make every clip overwrite the last. |
| `-fa` | `true` | **`true`, explicitly — NEW** | **MEASURED and this is the row that proves the CEO's rule pays.** `-nfa` scores **4.46%** against the baseline's 2.89% (+1.57 points, S47 D9 I29 vs S40 D7 I8) and is **18% slower**; proper-noun hits fall 46 → 41 of 66. Flash attention is worth 1.57 WER points, so it is far too valuable to hold by inheritance: passing it explicitly is byte-identical today and cannot silently flip when the vendor changes its mind. |

## 2. Decode parameters we leave at the vendor value — each with the comparison on disk

| Flag | Vendor default | We ship | Why, with evidence |
|---|---|---|---|
| `-bs` / `-bo` | `5` / `5` | `5` / `5` | **MEASURED both directions.** Greedy (`-bs 1 -bo 1`) is **4.09%** (+1.20 points) for 5% less wall clock; a wider beam (`-bs 8 -bo 8`) is **3.31%** (+0.42 points) for **33% more**. The vendor's 5 is a genuine optimum here, not an unexamined value. |
| `-p` | `1` | `1` | **MEASURED.** `-p 2` scores the same WER with a worse error profile (S37 D9 I9, cased 4.30% vs 3.94%) and is 5% slower. Splitting one file across processors also breaks it at fixed boundaries, which is the failure mode the vendor's own documentation warns about. |
| `-ac` | `0` (full) | `0` | **MEASURED.** `-ac 1500` is byte-identical and **8% slower**. Nothing to gain, so the full audio context stays. |
| `-wt` | `0.01` | `0.01` | **MEASURED.** `-wt 0.5` — a 50x move — is byte-identical and the token-offset count the deletion detector consumes is unchanged (1,829 both). Inert on this material in this build. |
| `-nth` | `0.60` | `0.60` | **MEASURED here and in the record.** `-nth 0.9` is byte-identical. The record's six-way sweep (`0.01 … 0.9`) on a real host channel holding 14 segments over measured silence produced six byte-identical JSON files (`config.js:190-200`). Inert across a 90x range. |
| `-et` | `2.40` | `2.40` | **MEASURED (record).** Passing it explicitly at its default value produced byte-identical JSON on the 92-minute corpus (`config.js:180-186`); it was removed from the `max` tier for implying a defense that was never running. |
| `-lpt` | `-1.00` | `-1.00` | **MEASURED (record).** `-lpt 0.0` removed one of 14 silence segments for **+90% wall clock** and perturbed the real decode (+1 segment, +8 words). A cost with no defensible benefit. |
| `-tp` / `-tpi` / `-nf` | `0.00` / `0.20` / off | unchanged | **MEASURED and the chain matters.** `-nf` (fallback off entirely) is **byte-identical** to the baseline, which proves temperature fallback never fires on this material — so temperature and its increment are provably inert here and no material exists on which to tune them. Leaving fallback ENABLED is the deliberate half: it is the vendor's recovery path for audio harder than anything we can measure and turning it off would remove a defense to buy nothing. |
| `-ml` / `-sow` | `0` / off | `0` / off | **MEASURED.** `-ml 60 -sow` produces byte-identical TEXT with 250 segments instead of 152 and costs 7% more time. Finer segments would change caption and merge granularity for no accuracy gain, so the segment boundaries stay the model's. |
| `-dtw` | off | off | **MEASURED.** `-dtw large.v3.turbo` is byte-identical **including the token offsets we consume** (1,829 word times either way) and costs **26% more wall clock**. It writes its timestamps somewhere `parseSegmentWordTimes` does not read; enabling it would be paying for an unused output. |
| `-sns` | off | off | **MEASURED.** `-sns` scores **3.41%** (+0.52 points, insertions 8 → 16). Suppressing non-speech tokens makes this pipeline worse, not safer. |
| `--prompt` / `--carry-initial-prompt` | unset / off | **main pass: unset and §5 is why. Restyle re-decode only: set, with `-mc 100` — CHANGED 2026-10-04, §9** | **MEASURED.** Under `-mc 0` both are **byte-identical to no prompt at all** — a zero text-context budget leaves no room for the prompt tokens — while costing 29–55% wall clock. On the main pass they stay unset. On the one-clip restyle re-decode they are the mechanism: §9 has the measurement and the reason it cannot fabricate. |
| `--suppress-regex` | unset | unset | **ARGUED.** A regex that deletes text before anything looks at the audio is the failure this project has already recorded: the captured fabricated span contains a real spoken "Zero.", so a blanket strip deletes speech (`ceo-decisions.md` §10). Suppression here is post-decode, adjudicated against the audio and stays there. |
| `--grammar` / `--grammar-rule` / `--grammar-penalty` | unset / unset / `100.0` | unset | **ARGUED.** A GBNF grammar constrains the decoder to a formal language. Conversation is not one. Nothing to write. |
| `-ot` / `-on` / `-d` | `0` / `0` / `0` | unchanged | **ARGUED.** Offsets and durations are for decoding part of a file. We decode the whole channel; the clip probe cuts its own audio with ffmpeg so that every stage measures the same clip the same way. |
| `-ng` / `-dev` | off / `0` | unchanged | **ARGUED.** Disabling the GPU on an Apple-Silicon host would multiply decode time for nothing. One GPU, device 0. |
| `-oved` | `CPU` | unset | **ARGUED.** OpenVINO is not in this build and not on this platform. |
| `-tr` | off | off | **ARGUED.** Translation would replace an English transcript with an English translation of it — a different artifact, silently. |
| `-di` / `-tdrz` | off / off | off | **ARGUED.** Speakers are separated by CAPTURE, one channel each, before any model sees them. `-di` needs stereo we do not feed it; `-tdrz` needs a tinydiarize model we do not ship. Both would add a second, weaker opinion about a fact we already know. |
| `-dl` | off | off | **ARGUED.** It exits after detecting the language and produces no transcript. |
| `-debug` / `-ls` | off / off | off | **ARGUED.** Diagnostics that put non-transcript material on the output paths. |
| `-ps` / `-pc` / `-pp` / `--print-confidence` / `-nt` | all off | all off | **ARGUED.** Console presentation. We read the JSON file. (`-nt` IS used by the dictation path, §7, which reads stdout on purpose.) |
| `-otxt` / `-ovtt` / `-osrt` / `-olrc` / `-ocsv` / `-owts` / `-fp` | all off | all off | **ARGUED.** Alternative output formats and the karaoke font. `-oj`/`-ojf` is the one the merge parses; the others would be unread files in the session directory. |
| `-h` / `--version` | — | **`--version` once per session — CHANGED 2026-09-10** | **MEASURED.** It probed `--help` and returned a constant. It now probes `--version`, whose stdout carries the build (`whisper.cpp version: 1.9.1`) and whose stderr carries the `load_backend:` lines naming every ggml backend loaded — one 0.04 s invocation for both, out of the decode path. The "worth revisiting" note this row used to carry is now done; §4.1 holds the measurement that made it harder than it looked, because **1.8.3 rejects `--version` and exits 0 with empty stdout.** |

## 3. Voice Activity Detection — the default nobody had justified, now justified

`--vad` is off. It was off because nobody had turned it on. It is off now because it was measured
three ways.

| Configuration | Short-call WER | Deletions | Wall (short) | Wall (92 min real) | Silence fabrications (92 min) |
|---|---|---|---|---|---|
| VAD off (ships) | **2.89%** | 7 | 76 s | **226 s** | 2 |
| `--vad`, vendor sub-defaults | 4.67% | **31** | 59 s | — | — |
| `--vad`, tuned (`-vt 0.3 -vp 200 -vsd 500 -vspd 100`) | **2.68%** | 8 | 65 s | **431 s** | **0** |

Three findings and the third is the decision:

1. **The vendor's VAD sub-defaults clip speech.** Deletions go 7 → 31. `Brightmoor Dental` goes from
   3 of 4 exact to **0 of 4** — the name is inside the audio the segmenter trimmed. Shipping `--vad`
   on its own defaults would have been the same mistake as `-mc -1`, in the other direction.
2. **Tuned, VAD is the best short-call score measured** (2.68%) and 14% faster.
3. **On the real 92 minutes the same tuned VAD is 91% SLOWER** (431 s vs 226 s) for ten words of
   difference. The synthetic speed-up is an artifact of the corpus: `build-corpus.mjs` fills the
   non-speaking channel with `anullsrc` — **exact digital silence** — which Silero skips instantly.
   Real captured audio is never digitally silent; VAD there fragments continuous speech into many
   small decode windows, each paying encoder cost.

**Decision: `--vad` stays off and its seven sub-flags (`-vm`, `-vt`, `-vspd`, `-vsd`, `-vmsd`,
`-vp`, `-vo`) stay unset** — they are inert while it is off. Enabling it would also mean bundling,
pinning and integrity-checking an eighth model file (`ggml-silero-v5.1.2.bin`, 885,098 B, sha256
`29940d98d42b91fbd05ce489f3ecf7c72f0a42f027e4875919a28fb4c04ea2cf`), which is not on this machine by
default and had to be fetched from `huggingface.co/ggml-org/whisper-vad` to run these rows at all.

**What it costs us, stated rather than buried:** tuned VAD is the only configuration measured that
removes the silence-fabrication class *at the decode stage* (2 → 0 on the real channel). We keep
handling that class post-decode, in `repetition-guard.js` class 4, against the physical audio.

**The condition under which this answer flips and why nobody should act on it yet:** a channel that
is mostly true silence is where VAD wins and a real two-party call capture — where `me` is silent
while the other person talks — is exactly that shape. The only long real audio in existence here is
a webinar where the host talks continuously and per the CEO's ruling of 2026-09-10 there will never
be another recording to settle it on. So this is named, priced and **deliberately not built**: a
per-channel silence-share switch would be tuning on material we do not have.

## 4. Not flags, but settings all the same — and these are the unguarded ones

| Setting | Today | Assessment |
|---|---|---|
| `whisper-cli` build | **PINNED per machine, 2026-09-10.** Homebrew's linked `whisper-cpp`, 1.9.1 today, 1.8.3 also on disk | **Was the largest remaining exposure in the table; now closed.** Every default in the "vendor default" column is a property of 1.9.1 and `-fa` alone is worth 1.57 WER points while being a *default*. The binary's sha256 is recorded on first use in a machine lock and re-hashed on **every** run (654,720 B, 0.01 s — free), so a changed build produces a loud warning naming both identities rather than a silent accuracy change. Not a source pin and §4.1 is why. |
| ggml backend | **PINNED per machine, 2026-09-10.** 0.17.0, separate formula | Same mechanism. The backends are read off the binary's own `load_backend:` startup lines — measured, not guessed from a Cellar path — and hashed with it, so a ggml formula bump is caught with `whisper-cpp` untouched. |
| `whisperVersion()` | **records the run, 2026-09-10.** | It returned the constant string `'whisper.cpp (whisper-cli)'` on every run ever made, so it could never disagree with itself. It now returns e.g. `whisper.cpp 1.9.1 bin:7dc20e3106d7 [BLAS/MTL/CPU] model:large-v3-turbo@1fc70f774d38` and `session.json` carries the full hashes **per run**, so a re-transcription cannot re-attribute an earlier one. |
| model files | pinned by size + sha256 in `model-pins.json`, verified on fetch, **and now hashed before every decode** (cached on identity) | The pattern the rows above were missing and it is now enforced at the decode too: `resolveModel` on the hot path never looked at the hash at all. |
| `RICHOS_WHISPER_*` env overrides | `RICHOS_WHISPER_MAX_CONTEXT`, `_LANG`, `_THREADS`, `_MODEL`, `_BIN` | Deliberate escape hatches. Each one can silently restore a vendor default — `RICHOS_WHISPER_MAX_CONTEXT=-1` is `-mc -1` again — which is why `verification.json` records the EFFECTIVE `whisperArgs`, not the intended one. `_MODEL` and `_BIN` are now recorded by IDENTITY as well as by path. |

### 4.1 Why the binary is locked per machine and the weights are pinned in source

**Two tiers, deliberately unequal and the asymmetry is the whole design.**

| | Where the expectation lives | On a mismatch | Why that one |
|---|---|---|---|
| **model weights** | `model-pins.json`, in source, six models | **REFUSE** | The table has authority: HuggingFace's `x-linked-etag` plus a shasum of the CEO's own copy say what the bytes are supposed to be. The fetch path already refuses on this exact hash, so a decode path that shrugged would be overruling it. |
| **`whisper-cli` + ggml backends** | `~/.config/richos/whisper-toolchain.lock.json`, written on first use | **WARN, naming both identities** | `whisper-cli` comes from Homebrew. No source sha256 could hold across arch, OS version and bottle revision and a pin every other machine fails is a pin nobody keeps. The lock records what WAS there, not what is right — so a routine `brew upgrade` must not leave an already-captured call untranscribable. The CEO cannot re-record a meeting. |

`RICHOS_WHISPER_STRICT_TOOLCHAIN=1` escalates every warning to a refusal — for reproducing the
measurements in this table, where "the binary changed" invalidates the run. **Off by default and
the default is a decision:** on by default would mean a Homebrew upgrade silently stops
transcription for someone who has no idea what a bottle is.

**The reference build lives in `model-pins.json` too**, in a `toolchain` block beside the six model
pins, because two registries of truth is how the second unpinned consumer came to exist. It is what
a run is compared and attributed to, never a requirement a second machine would fail.

**And the version string is a LABEL, not the identity — measured, not assumed:**

```
/opt/homebrew/Cellar/whisper-cpp/1.9.1/bin/whisper-cli --version
  exit 0, stdout: whisper.cpp version: 1.9.1
/opt/homebrew/Cellar/whisper-cpp/1.8.3/bin/whisper-cli --version
  exit 0, stdout: EMPTY (0 bytes), stderr: error: unknown argument: --version
```

**1.8.3 rejects the flag and still exits ZERO.** A probe that trusted the exit code would see
success and an empty version — indistinguishable from a build that simply has none. So the sha256
is the identity, the exit status is recorded and never consulted and a build that will not
identify itself gets a finding of its own rather than an empty field nobody reads. That also
supersedes the "worth revisiting" note in §2's `-h` / `--version` row: `--version` IS now the probe,
and this is the trap it had to be written around.

**What is hashed when**, measured on this M4 with `shasum -a 256`, three runs each:

| | Size | Time | When |
|---|---|---|---|
| `whisper-cli` | 654,720 B | 0.01 / 0.01 / 0.01 s | **every run** |
| ggml backends (×3) | ~1.5 MB | under 0.05 s | **every run** |
| `ggml-small.en.bin` | 487,614,201 B | 0.95 / 0.93 / 0.93 s | cache miss only |
| `ggml-large-v3-turbo.bin` | 1,624,555,275 B | 3.13 / 3.11 s | cache miss only |

The binary is free to hash and is precisely the thing a package manager swaps under you, so it is
hashed every time. A dictation utterance decodes in 0.47–0.74 s (§7), so a 0.93 s hash on each one
would nearly triple that path's latency; models are cached on (path, size, mtime, inode, device).
**The limit of that cache, stated rather than buried:** a rewrite in place restoring all five fields
would be believed until something re-hashes. `richos-service toolchain --recheck` and
`verify-model` are that something and a refused run never writes the cache.

**Both consumers, one lock.** `tools/richos-service` and `app/crates/richos-voice` share the same
lock file and the same pin table (compiled in with `include_str!`), so a changed binary is caught by
whichever surface meets it first and a model hashed by one is verified for the other.

**Inspect it:** `richos-service toolchain` (`--recheck` re-hashes, `--relock` accepts a change on
purpose) or `cargo run -p richos-voice --example toolchain_probe`.

## 5. What `-mc 0` costs, priced rather than asserted

The invariant is right and the ledger is not free. Both halves, measured today:

| | `-mc 0` (ships) | `-mc 64` | `-mc 224` | `-mc -1` |
|---|---|---|---|---|
| short-call WER | 2.89% | **2.73%** | 9.92% | 2.78% |
| proper nouns exact (of 66) | 46 (69.70%) | **55 (83.33%)** | 55 (83.33%) | 49 (74.24%) |
| 92-minute fabricated share | **0.0%** | 3.3% | — | 7.8% |
| 92-minute wall | **226 s** | 393 s | — | 509 s |

The `-mc 64` column is with a carried initial prompt naming the entities; without a prompt the
budget does nothing for names. Read across: **the setting that fixes name spelling is the same
setting that reopens fabrication.** `--prompt` is not a third path around the invariant, which is
what this table set out to find out.

That prices open item **3.3d**, whose CEO decision is *"leave the invariant and canonicalize names in
loro-correction or make decode context length-dependent"*: the second option now has a number
against it (3.3% of a 92-minute timeline at the cheapest budget that helps) and a third option that
looked available — bias with a prompt and keep `-mc 0` — is measured to do **nothing at all**.

**No default, tier or invariant is changed by this document**, except `-fa`, which is pinned at the
value it already had. The `-mc` decision remains the CEO's.

## 6. What the earlier record this supersedes and what it does not

| Earlier finding | Status after today |
|---|---|
| `-mc 0` over `-mc -1` (`ceo-decisions.md` §10) | **CONFIRMED**, re-derived on today's build against the same recording: 0.0% vs 7.8% of the timeline, 203 fabricated repeats reproduced. |
| Short-call WER 4.15% at `-mc 0`, 10.34% at `-mc -1` (row 3.3d) | **NOT REPRODUCIBLE as absolute numbers.** The same configuration scores 2.89% today; `say` has drifted (call-01 is 47.10 s against the committed 47.08 s). Re-scoring the committed August transcripts reproduces all four figures exactly, so the rig is sound and the AUDIO is what changed. The *direction* is also no longer visible at short call length today (2.78% vs 2.89%, inside noise), which is why the invariant's justification here rests on the long-form measurement rather than on that corpus. |
| "The fix costs name spelling" (row 3.3d) | **CONFIRMED and extended.** 46 of 66 at `-mc 0` against 49 at `-mc -1` and 55 with a prompt at `-mc 64`. Extension: the prompt lever is unavailable at `-mc 0`. |
| `-et` / `-lpt` / `-nth` inert (`config.js:180-200`) | **CONFIRMED** on this corpus for `-nth`; the record's own byte-identical runs stand for the others. |
| Deletion detection at `-mc 0` = 2.2 s (row 3.3f) | **Untouched.** Nothing here changes a decode setting that feeds it. |
| Model choice, turbo vs q5_0 (§10 / item 1.3) | **NOT DECIDED HERE and not mine.** Nothing in this table is model-specific by construction: every row was measured on `large-v3-turbo` and every conclusion is about the decoder's behavior, not the weights. The one row that could plausibly differ between a full and a quantized model is `-fa`, since flash attention interacts with numeric precision; if the CEO moves the default to q5_0, re-run the `nofa` row and nothing else. |

## 7. The second consumer nobody had audited

`app/crates/richos-voice/src/stt.rs` also shells out to `whisper-cli` — the dictation and
conversational path — and it was running `-l en -t 4 -np -nt` and **nothing else**, so its decode
context was at the vendor's `-mc -1`: the identical unexamined default that started all of this,
still live in a second place. It also offers `RICHOS_WHISPER_PROMPT` as the loro entity-biasing seam.

Measured on one 6-second utterance through `small.en`, three runs each
(`measurements/dictation-probe.txt`):

| Configuration | Result | Latency |
|---|---|---|
| `-mc -1` (what it shipped) | "Halda**n** Freight" | 587–661 ms |
| `-mc 0` | "Halda**n** Freight" | 612–641 ms |
| `-mc 0` + prompt | "Halda**n** Freight" — **the prompt does nothing** | 609–614 ms |
| `-mc 64` + prompt | "Halde**n** Freight" — correct | 607–654 ms |

So the path now passes `-fa` and an explicit `-mc`: **0 when no prompt is configured** (the
invariant, no measurable cost at utterance length) and **64 when one is** — because at 0 the
configured prompt is silently inert and a seam that reports on while doing nothing is the defect
class this codebase already refuses elsewhere. Latency is unchanged either way, inside run-to-run
noise.

**The bound, stated:** 64 carried tokens is a fabrication risk on audio long enough to accumulate
them (3.3% of a 92-minute timeline). It is taken only when a prompt is set and dictation utterances
are seconds long. Anyone wiring a long recording through the voice path should read §5 first.

## 8. What remains undecidable and the honest fallback in each case

There will be no more recordings. These do not resolve later; they are answered now, conservatively,
or they are named as not answerable.

| Question | Why it cannot be settled | What we do instead |
|---|---|---|
| Does `-mc 0` DELETE real speech on long audio? | After the guard removes fabrication, `-mc -1` still has 515 more words over 92 minutes and no reference exists to say whose they are. | Ship `-mc 0` — the configuration that provably does not destroy 7.8% of a timeline — and leave the deletion and substitution detectors running, which is where under-transcription is caught with evidence rather than guessed at. |
| Is `--vad` right for a real two-party CALL capture? | The only long real audio is a continuously-talking webinar; a call's silent channel is the shape where VAD wins and no such recording exists. | Keep VAD off (never deletes speech, bundles no extra model). The condition is recorded in §3 and deliberately not built. |
| Are `-tp` / `-tpi` / `-et` / `-lpt` tuned right for hard audio? | Fallback never fires on any material we have, so every value is inert on it. | Keep the vendor's recovery path enabled and unmodified. Tuning a defense on material that never triggers it would be fitting to noise. |
| Would a different `-wt` improve deletion-detector localization? | Two settings 50x apart give identical token offsets on this corpus; nothing here exercises it. | Leave at `0.01` and keep scoring coverage on word times, which is already the measured-better unit. |
| turbo vs q5_0 | The CEO's and out of scope for this table. | Nothing here depends on it; see §6. |

## 9. Run-on windows and the restyle re-decode (2026-10-04)

**The defect** (CEO, 2026-10-04): a 66-minute imported video, decoded with exactly §1's command
(whisper.cpp 1.9.4, `large-v3-turbo-q5_0`), came back with stretches of lowercase, unpunctuated
run-on text between stretches that read perfectly. The measurement run reproduced his transcript
byte for byte (text sha256 prefix `42919666dfdef913`, both runs).

**The cause.** At `-mc 0` every 30-second window decodes with no text before it, so punctuation and
case are chosen afresh per window. A window starts at the previous one's last timestamp, usually
mid-sentence. Of 143 windows, 105 began on a lowercase word and 37 of those (35%) came out run-on;
of the 38 that began capitalized, 1 did (2.6%). The bad windows are scattered from 02:50 to 65:55,
so this is not drift.

**Every decode-level lever, measured on the same file:**

| Lever | Result | Verdict |
|---|---|---|
| `-mc 16` (carry 16 decoded tokens) | run-on words 32.7% → **64.6%**, lowercase pronoun 102 → 293, longest segment 10.5 s → 30.0 s | Rejected. Bad style is carried forward as well as good; this is the drift `-mc 0` prevents. |
| `--prompt` + `--carry-initial-prompt` at `-mc 20` (main pass) | punctuation returns, but timestamps collapse to one 30 s segment per window and words at a window end are **dropped** | Rejected: deletes speech. |
| the same at `-mc 1` | only the prompt's last token survives (`max(1, mc-1)`, whisper.cpp `src/whisper.cpp:7067`); a repetition loop appears | Rejected. |
| `--grammar` forbidding a lowercase first letter | 9 of 17 restored; **deletes** the leading words it cannot capitalize (up to twelve at one window start); 2.3× slower | Rejected: deletes speech. |
| isolated re-decode of the span, no prompt | 15 of 33 restored | Rejected: a coin flip. |
| **one-clip re-decode of each run-on window, prompted with the transcript's own preceding text, `-mc 100`** | **36 of 38** came back punctuated (the other two punctuated too, by a stricter detector's miss); 81.6 s for all 38 | **Adopted, as stage 3.9.** |

**Why the adopted row cannot fabricate.** Each clip fits one decode window and is conditioned on a
fixed prompt, so no decoded text accumulates, which is the mechanism behind every loop in §5. And
`restyle.js` takes only punctuation and case from it: the main pass's words are aligned to the
re-decode's and keep their identity, so no word can be inserted, deleted or substituted and every
detector (3.5 to 3.8) has already judged the words that ship. The main pass is unchanged:
`-mc 0`, no prompt.

**Cost:** one model load and one short decode per run-on clip, so zero on a transcript without
run-on windows and proportional to the defect on one with them.

## 10. RichScribe provisional preview (2026-11-11)

| Flag | Scope | Decision |
|---|---|---|
| `-bs 3` | RichScribe provisional Faster-model results | A matched 16-second English passage had identical text with three- and five-wide search during concurrent RichBrief decoding. Three-wide search spent less time on batched token decoding. The final dictation and meeting settings are specified separately above. |

# RichScribe: how a finished dictation is recognized, measured 2026-11-12

**The complaint.** RichScribe pasted words nobody said ("Thank you." after the last sentence)
and dropped words that were said. Both come from how the finished recording was handed to the
decoder, not from the two speech models and both were measured on the shipped settings before
anything was changed.

The settings table is
[`../whisper-settings-2026-09-10/whisper-settings-decisions.md`](../whisper-settings-2026-09-10/whisper-settings-decisions.md);
this is the record behind its section 11. No recording is committed: a dictation is somebody's
voice. The numbers are in [`results.txt`](results.txt).

## What was shipped and what it did

The whole recording went to `whisper-cli` once, with `-l en -t 4 -fa -np -nt -mc 0`. Whisper
decodes 30 seconds at a time and with `-nt` whisper.cpp 1.9.1 steps through a longer
recording in fixed 30-second windows whatever it decoded (`seek_delta =
100*WHISPER_CHUNK_SIZE`, `src/whisper.cpp`).

| Defect | Cause | Measured on 97 dictations |
|---|---|---|
| Words lost | A word across a window's edge or a clause after the decoder ended a window early, is gone. | 38 dictations lost three or more words in a row on More Accurate and 23 on Faster: nearly every dictation longer than about 27 seconds. 598 and 709 words deleted of 10,987. |
| Words nobody said | A window with no speech in it is where Whisper invents text. The room sound after the last word of a 32-second dictation is a window of its own. | 11 dictations ended in unspoken words on More Accurate ("Thank you." in most). Room sound alone produced 8 words. |

## What changed

The standard answer, "cut and merge" as [WhisperX](https://github.com/m-bain/whisperX) and
faster-whisper's batched pipeline do it:

1. **A speech detector finds the speech.** Silero VAD 5.1.2, the detector those pipelines
   use, run in process on the ONNX Runtime the app already carries for Rich's voice
   (`speech.rs`). The model is compiled in (2.3 MB, MIT): nothing to download or install.
2. **The recording is cut into pieces that each fit one window.** Talking separated by less
   than two seconds stays together, pauses included. A stretch too long for a window is cut
   in the final third of the window, in the middle of the last pause there. Each piece keeps
   0.3 s of the recording's own sound at each end. The audio itself is never altered: a
   piece is a range of the recording.
3. **Every piece is decoded as one utterance**, with the flags one utterance has always been
   decoded with (`-l en -t 4 -fa -np -nt -mc 0`), all in one decoder run so the model is
   loaded once.

So no window ever holds only the room and no word ever sits across two windows. Nothing is
filtered out of the text afterwards: there is no phrase list and no level threshold. If the
detector cannot run or hears nothing in a recording the app's voice gate let through, the
recording is decoded whole as before.

## Results

The corpus: 63 dictations read by 40 different people (LibriSpeech test-clean, public
domain recordings, 8 to 185 seconds, with pauses of up to 20 seconds and up to 12 seconds of
trailing silence), 28 synthesized business dictations in four voices with and without a
clearly spoken closing "Thank you." and 6 recordings of room sound alone. Every one is
laid over room tone recorded by a desk microphone, at several speech and room levels including
a low-gain microphone (speech at -40 dBFS) and a loud room (a 16 dB signal-to-noise ratio).
79 minutes, 10,987 reference words. The "new" rows are the app's own recognizer
(`Recognizer::transcribe_dictation_until`, run by `tests/dictation_recordings.rs`).

| | Word errors | Words deleted | Dictations losing 3+ words in a row | Dictations ending in unspoken words | Words from room sound alone |
|---|---|---|---|---|---|
| More Accurate, shipped | 7.34% | 598 | 38 of 97 | 11 of 97 | 8 |
| More Accurate, new | **1.62%** | **15** | **0** | **0** | **0** |
| Faster, shipped | 8.46% | 709 | 23 of 97 | 0 of 97 | 0 |
| Faster, new | **2.03%** | **23** | **1 of 97** | **0** | **0** |

Word errors count substitutions, deletions and insertions against the reference text with case
and punctuation removed. Most of what remains is the reference writing a word differently
("two hundred" against "200", "Mr." against "mister").

The one dictation that still lost words is on Faster: four isolated words and phrases at the
start of a piece ("Angor. Pain. Painful to hear. Hay fever.") came back empty. More Accurate
wrote them and so did Faster when the same piece was cut 1 second shorter, so it is the
model's own instability on that passage, not a cut. The six room-only recordings are refused by
the app's voice gate before any decode, as they were before; the eight words they produced as
shipped came from decoding them anyway, which the measurement of the shipped flags did.

### Capitals and punctuation

Whisper chooses a style for each window it decodes and sometimes chooses none: a passage in
lower case with no punctuation. Counted over the dictations of twelve words or more:

| | No punctuation at all | Lowercase first letter |
|---|---|---|
| More Accurate, shipped | 3 of 91 | 8 of 91 |
| More Accurate, new | 11 of 91 | 27 of 91 |
| Faster, shipped | 0 of 91 | 2 of 91 |
| Faster, new | 0 of 91 | 2 of 91 |

All 28 synthesized business dictations came back with capitals and punctuation on both
models, before and after. The difference is in the audiobook readings on More Accurate and it
has one cause: that model writes a window that is cut off mid-speech with punctuation and a
clip that ends in a pause more often without. The shipped flags cut every long recording off
mid-word at 30 seconds, which is how the words were lost; pieces end in pauses, as every short
dictation always has. It is the model's behavior on short clips, it did not show on the
business dictations or on the real desk captures below and a text prompt that would force the
style costs words (see the table at the end). Faster is unchanged.

### On the real microphone

Two sets of real captures, both a desk microphone (Elgato Wave:3) hearing speech played once
through the Mac's speakers.

**Seven captures saved on 2026-11-12 before the change**, among them the ones on which the
shipped flags wrote the unspoken "Thank you.": through the new recognizer, 14 of 14 decodes (both models) have no
unspoken ending and every sentence of the body, including the first word of the two that begin
with 14 seconds of silence.

**Eight new captures through the signed app** (source `1d204b542`, Developer ID development
build), dictated into a TextEdit document with the real key, two meeting decodes running
beside each one, scored against what was played:

| Case | More Accurate | Faster |
|---|---|---|
| Speech ending at the 30-second mark, then 2.3 s of silence (86 words) | 0 word errors, nothing added | 0 word errors, nothing added |
| The same, then a clearly spoken "Thank you." (88 words) | 0 word errors, closing kept | 0 word errors, closing kept |
| 55 seconds of continuous speech (166 words) | 1 substitution, 0 lost | 1 substitution, 0 lost |
| Speech, a 12-second pause, more speech, preview on (146 words) | 0 word errors; fresh preview words 1.9 s after the pause | 0 word errors; fresh preview words 2.0 s after the pause |

In both pause cases the preview panel was visible in every sample (two a second) and never
shrank. The one substitution is "nine thirty" written as "9:30". A first attempt at one case
also holds twelve words of somebody else speaking in the room at its start; they were written
correctly, the attempt is kept and the case was captured again.

### Time

Wall-clock time on the same six dictations, the shipped single run against the new detector
plus one run over the pieces, three rounds alternating, medians, on a Mac that was busy with
other work throughout (`results.txt`, `time`):

| Dictation | More Accurate, shipped | More Accurate, new | Faster, shipped | Faster, new |
|---|---|---|---|---|
| 14 s, 1 piece | 1.96 s | 2.72 s | 1.06 s | 1.45 s |
| 32 s, 1 piece | 3.81 s | 3.72 s | 2.91 s | 2.06 s |
| 43 s, 2 pieces | 4.34 s | 4.89 s | 2.54 s | 3.17 s |
| 79 s, 3 pieces | 6.33 s | 7.31 s | 3.99 s | 3.65 s |
| 102 s, 4 pieces | 7.61 s | 10.25 s | 3.72 s | 4.73 s |
| 191 s, 7 pieces | 15.61 s | 17.63 s | 11.01 s | 9.19 s |

The speech detector's own share is 0.1 to 1.0 s. The rest of the difference, in both
directions, is inside the run-to-run spread of a loaded machine: the decoder does the same
work on the same speech and no longer decodes silence.

## Measured and not taken

Each of these was run on the same corpus or the same desk captures before the design above
was chosen.

| Alternative | Result | Why not |
|---|---|---|
| whisper.cpp's built-in detector (`--vad`) with the decoder's own seeking (no `-nt`), detector values from the settings table section 3 | Words as good as the design above on More Accurate (15 deleted, none lost three in a row, 2 unspoken endings left). No punctuation at all in 27 of 63 human dictations, against 3 of 63 as shipped | That detector splices the speech together with digital silence between the parts and the More Accurate model answers spliced audio in lower case without punctuation. |
| The same with the detector's vendor defaults | 2 of the 83 dictations that ran lost three or more words in a row on Faster (seven words after 14 seconds of leading silence) and the first sentence of two real desk captures was dropped | It clips speech, as section 3 measured. |
| The same with a 30 ms speech pad | The first word ("I") of a real desk capture was cut off on both models | The pad is the only lead-in spliced audio has. |
| The decoder's own seeking with no detector | Of the 36 dictations that ran on More Accurate, 18 started in lower case and 8 had no punctuation at all and silence still reaches the decoder | Timestamp mode is where the More Accurate model drops its style. |
| A style prompt (`--prompt`, carried into every window or given once) | Capitals and punctuation returned and one 62-second dictation lost 26 words in a row, one of 185 seconds lost 22 | A prompted window comes back as one segment with no timestamps inside it, the seek jumps 30 seconds and the end of the window is lost. Section 9 found the same on a 66-minute video. |
| Removing a closing "Thank you." by its text, its level or the room's level | Three versions were reviewed and held on 2026-11-12; on the real microphone the last one left the unspoken closing in | A phrase rule can delete a real phrase and a level rule fits one room. |
| Another speech engine (Parakeet) | Not needed for either defect | Adding an engine is the CEO's decision and nothing here depends on it. |

## What this does not claim

- Speech at the level of the room is not kept. The two closings an earlier desk run called
  "spoken" sit at -60 dBFS against a -65 dBFS room, 20 dB under the rest of the speech; they
  are not in the delivered text, before or after.
- Capitals and punctuation stay Whisper's own choice per piece. See the table above.
- One Mac (Apple M4), one microphone for the desk captures, English, the two shipped models.

## Run it again

```sh
RICHOS_DICTATION_PROBE_LIST=list.txt RICHOS_DICTATION_PROBE_RUNTIME_BIN=<runtime>/bin \
RICHOS_DICTATION_PROBE_OUT=heard.json \
    cargo test -p richos-voice --test dictation_recordings what_the_dictation -- --ignored
```

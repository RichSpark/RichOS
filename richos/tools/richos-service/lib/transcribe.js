/**
 * RichOS local service — pipeline stage 3: TRANSCRIBE (whisper.cpp, per channel).
 *
 * whisper.cpp (`whisper-cli`, Metal auto-on) is a native binary the service shells out to — the
 * precise reason the pipeline is not in the MV3 extension (the system architecture §4.1). Each channel
 * is transcribed independently, so every segment is ALREADY speaker-attributed by channel
 * (me / others) before the merge — no diarization model involved.
 *
 * The default model is whatever `config.js` DEFAULT_TIER resolves to — `large-v3-turbo-q5_0`
 * since 2026-09-10 (CEO decision page §10). It is NOT restated here, because a comment naming a
 * model is a second declaration of the default that no test can keep honest. Full turbo remains
 * selectable: `--tier turbo`, or `--model large-v3-turbo`.
 */

import fs from 'node:fs';
import path from 'node:path';
import { execFileSync } from 'node:child_process';
import { withPrivateOutputs } from './private-files.js';
import { whisperBin, resolveModel, whisperArgs, restyleArgs, DEFAULT_MODEL } from './config.js';
import { assertToolchain, resolveToolchain, probeWhisper, provenanceString } from './toolchain.js';

/**
 * Per-token start offsets of one whisper.cpp segment, in ms — the model's OWN claim about where
 * its words are, present only when the run passed `-ojf` (`--output-json-full`).
 *
 * WHY THIS MATTERS AND WHY IT IS NOT OPTIONAL POLISH: the deletion detector
 * (`deletion-guard.js`) asks "did the run emit any word over this second of audio?", and a
 * whisper.cpp SEGMENT extent cannot answer it. Segments routinely stretch across the pauses around
 * them — measured on the 2026-08-29 corpus, one 30-second segment carried 8 words over 2.2 seconds
 * of actual speech, and 23-second segments carrying 6 words are ordinary. Score coverage on the
 * extent and the words get smeared across silence the speaker never filled, which invents
 * deletions where there are none. Score it on the token offsets and the same eight spans land
 * inside their bursts, correctly, every time.
 *
 * ONE TIME PER WORD, NOT PER TOKEN, and the difference is not cosmetic. whisper's vocabulary is
 * sub-word: a hyphenated compound arrives as several pieces, and a continuation piece carries its own
 * offset which can land well inside the following pause. Counting pieces therefore marks a burst
 * "covered" because the tail of the PREVIOUS word drifted into it. A word starts where whisper puts
 * a leading space; continuation pieces extend it and contribute no time of their own. This is the
 * same unit the 2026-08-29 coverage measurement used, and adopting it moved this pipeline's
 * detector from 6 of the 9 documented deletions to 8.
 *
 * `[_BEG_]` / `[_TT_nn]` are whisper's own structural markers, not words, and are dropped.
 * @param {any} segment one `transcription[]` row
 * @returns {number[]}
 */
export function parseSegmentWordTimes(segment) {
  const toks = Array.isArray(segment?.tokens) ? segment.tokens : [];
  const out = [];
  let started = false;
  for (const t of toks) {
    const text = String(t?.text ?? '');
    if (/^\[_/.test(text)) continue;
    if (!text.trim()) continue;
    const isWordStart = /^\s/.test(text) || !started;
    started = true;
    if (!isWordStart) continue;
    const ms = Number(t?.offsets?.from);
    if (Number.isFinite(ms)) out.push(ms);
  }
  return out;
}

/**
 * Parse whisper.cpp `-oj` JSON into normalized segments.
 *
 * `wordTimesMs` rides ON the segment rather than in a parallel array, deliberately: every later
 * stage that drops, collapses or splits a segment (the repetition guard, diarization) then carries
 * its word times with it for free, and a segment the guard removed cannot leave orphaned token
 * times behind claiming coverage for text that is no longer in the transcript.
 *
 * @param {any} json parsed whisper JSON
 * @param {string} speaker channel label ("me" | "others")
 * @returns {{startMs: number, endMs: number, text: string, speaker: string, wordTimesMs?: number[]}[]}
 */
export function parseWhisperJson(json, speaker) {
  const rows = Array.isArray(json?.transcription) ? json.transcription : [];
  return rows
    .map((r) => {
      const seg = {
        startMs: Number(r?.offsets?.from ?? 0),
        endMs: Number(r?.offsets?.to ?? 0),
        text: String(r?.text ?? '').trim(),
        speaker,
      };
      const times = parseSegmentWordTimes(r);
      if (times.length) seg.wordTimesMs = times;
      return seg;
    })
    .filter((s) => s.text.length > 0);
}

/**
 * The identity of the binary, the backends and the weights that are about to decode this audio —
 * checked before a byte is read, and returned so the caller can record it.
 *
 * THIS IS THE GATE. `whisperArgs()` decides HOW the audio is decoded and the settings table
 * (`docs/measurements/whisper-settings-2026-09-10/`) justifies every value in it — but a decided
 * setting handed to an unknown binary is a decision about nothing. `-fa` is the worked example the
 * table gives: flash attention is whisper.cpp's own DEFAULT, and the table pins it explicitly so a
 * vendor formula bump cannot silently flip it. That defends one flag. This defends the premise
 * underneath all of them.
 *
 * Refuse-versus-warn, and which is which, is `toolchain.js`'s `SEVERITY` table with its reasons.
 * The short version: a mismatch against a SOURCE pin (the model weights) refuses here; a mismatch
 * against the machine's own trust-on-first-use lock (the binary, its backends) warns loudly and
 * the transcript is attributed to the new identity rather than thrown away.
 *
 * @param {string} modelPath resolved .bin
 * @param {string} modelId portable model id
 * @returns {object} the `resolveToolchain` result
 * @throws {Error} code `TOOLCHAIN_REFUSED` when the weights are not the pinned weights
 */
export function checkToolchain(modelPath, modelId) {
  return assertToolchain({ binPath: whisperBin(), modelPath, modelId });
}

/**
 * A provenance line naming the binary AND the weights that produced a transcript.
 *
 * WHAT THIS USED TO BE, and why that was the exposure rather than a cosmetic gap: it ran
 * `whisper-cli --help`, matched `/whisper\.cpp|usage/`, and returned the CONSTANT STRING
 * `'whisper.cpp (whisper-cli)'` — the same 24 characters for 1.9.1, for 1.8.3, for a build with
 * flash attention removed, and for anything else that prints the word "usage". Recorded against
 * every transcript, it could never disagree with itself, so no later reader could notice a change.
 * The settings decision table §4 named exactly that: "it records nothing that would let anyone
 * notice the change."
 *
 * It now returns what actually ran, e.g.
 *
 *   whisper.cpp 1.9.1 bin:7dc20e3106d7 [BLAS/MTL/CPU] model:large-v3-turbo-q5_0@394221709cd5
 *
 * so "which binary and which weights produced this transcript" is answerable from the string
 * alone, and the structured form (`record.pipeline.toolchain`) carries the full hashes.
 *
 * Kept returning a STRING at this name because `contract.js` and `pipeline.js` store it as one.
 * Called with no model it still answers about the binary — the honest partial answer, not a lie.
 *
 * @param {{modelPath?: string, modelId?: string}} [opts]
 * @returns {string}
 */
export function whisperVersion(opts = {}) {
  try {
    const bin = whisperBin();
    if (opts.modelPath) {
      return resolveToolchain({ binPath: bin, modelPath: opts.modelPath, modelId: opts.modelId || DEFAULT_MODEL })
        .provenance;
    }
    const probed = probeWhisper(bin);
    return provenanceString({ bin: probed.bin, backends: probed.backends });
  } catch (err) {
    // NAMES THE FAILURE. "whisper-cli" as a provenance value is indistinguishable from a
    // successful probe of a binary called whisper-cli, which is the defect this function had.
    return `whisper-cli (identity not recorded: ${String(err.message || err).split('\n')[0]})`;
  }
}

/**
 * Transcribe one mono WAV channel.
 * @param {string} wavPath
 * @param {string} speaker "me" | "others"
 * @param {{model?: string, outDir?: string, extraArgs?: string[], language?: string, timeoutMs?: number}} [opts]
 * @returns {{segments: object[], jsonPath: string, model: string}}
 */
export function transcribeChannel(wavPath, speaker, opts = {}) {
  const modelId = opts.model || DEFAULT_MODEL;
  const modelPath = resolveModel(modelId);
  // BEFORE the decode, never after. A refusal here has cost nothing and destroyed nothing: the
  // audio is untouched on disk and the caller can re-run once the model is re-fetched. The result
  // is memoized per process, so transcribing both channels of one call probes the binary once.
  const toolchain = checkToolchain(modelPath, modelId);
  const outDir = opts.outDir || path.dirname(wavPath);
  const outBase = path.join(outDir, `${speaker}`);
  const jsonPath = `${outBase}.json`;
  withPrivateOutputs([jsonPath], ([output]) => {
    const args = [
      '-m', modelPath,
      '-f', wavPath,
      ...whisperArgs({ extraArgs: opts.extraArgs, language: opts.language }),
      // OUTPUT verbosity, NOT a decode parameter — it sits out here with `-of` for exactly that
      // reason, so `whisperArgs()` stays the honest record of how the audio was decoded. `-ojf` adds
      // per-token offsets to the JSON and changes nothing else: proven on the 92-minute corpus, where
      // a run with `-ojf` produced a transcript byte-identical (sha256) to the committed run without
      // it. The deletion detector cannot localize anything without these times — see
      // `parseSegmentWordTimes` — and a detector scored on segment extents invents deletions.
      '-ojf',
      '-of', output.slice(0, -5),
    ];
    execFileSync(whisperBin(), args, { stdio: ['ignore', 'ignore', 'inherit'], timeout: opts.timeoutMs });
  });
  const json = JSON.parse(fs.readFileSync(jsonPath, 'utf8'));
  return { segments: parseWhisperJson(json, speaker), jsonPath, model: modelId, toolchain };
}

/**
 * Transcribe both channels of a normalized session.
 *
 * `toolchain` comes back from the channel that actually ran rather than from a fresh probe, so the
 * provenance recorded against the transcript is the identity that DECODED it, not the identity of
 * whatever is installed by the time the record gets written. On a long call those are minutes
 * apart and a `brew upgrade` fits comfortably in between.
 *
 * @param {{me: string, others: string}} channels
 * @param {{model?: string, outDir?: string, extraArgs?: string[], language?: string, timeoutMs?: number}} [opts]
 * @returns {{me: object[], others: object[], model: string, whisper: string, toolchain: object}}
 */
export function transcribeSession(channels, opts = {}) {
  const me = transcribeChannel(channels.me, 'me', opts);
  const others = transcribeChannel(channels.others, 'others', opts);
  return {
    me: me.segments,
    others: others.segments,
    model: me.model,
    whisper: me.toolchain ? me.toolchain.provenance : whisperVersion(),
    toolchain: me.toolchain || null,
  };
}

/**
 * Re-decode ONE clip conditioned on the transcript text that precedes it — the restyle stage's
 * decode (`restyle.js`, stage 3.9). Returns the clip's text.
 *
 * One invocation per clip, unlike `transcribeClips`: `--prompt` is per invocation, and each clip's
 * prompt is different (and, for consecutive clips, includes the previous clip's restyled text).
 * The decode parameters are `whisperArgs()` exactly as the main pass uses them, with
 * `restyleArgs()` after them; gated by the same toolchain check, because this text decides the
 * punctuation and case of a shipped transcript.
 *
 * @param {string} clipPath
 * @param {string} prompt
 * @param {{model?: string, extraArgs?: string[], language?: string}} [opts]
 * @returns {string}
 */
export function transcribeStyled(clipPath, prompt, opts = {}) {
  const modelId = opts.model || DEFAULT_MODEL;
  const modelPath = resolveModel(modelId);
  checkToolchain(modelPath, modelId);
  const jsonPath = `${clipPath}.json`;
  withPrivateOutputs([jsonPath], ([output]) => {
    execFileSync(
      whisperBin(),
      [
        '-m', modelPath,
        '-f', clipPath,
        ...whisperArgs({ extraArgs: opts.extraArgs, language: opts.language }),
        ...restyleArgs(prompt),
        '-of', output.slice(0, -5),
      ],
      { stdio: ['ignore', 'ignore', 'ignore'] },
    );
  });
  const json = JSON.parse(fs.readFileSync(jsonPath, 'utf8'));
  return (json.transcription || [])
    .map((r) => String(r?.text ?? ''))
    .join(' ')
    .replace(/\s+/g, ' ')
    .trim();
}

/**
 * Decode a batch of short CLIPS in ONE whisper-cli invocation — the isolated re-decode the
 * deletion detector adjudicates on (`deletion-guard.js`).
 *
 * ONE invocation, not N, and that is the whole reason this function exists rather than a loop over
 * `transcribeChannel`. `whisper-cli` takes `file0 file1 ...` and loads the model ONCE; a loop pays
 * the model load — which dominates a 3-second clip by an order of magnitude — for every span. This
 * is what keeps the detector's cost proportional to the number of SUSPECT SPANS rather than to the
 * length of the recording, and a detector that must re-run the whole file is not shippable.
 *
 * Decode parameters are `whisperArgs()`, exactly as the main pass uses them — a probe decoded on
 * different settings from the run it is judging would not be a control. `-of` is deliberately NOT
 * passed: with several inputs one output base would make every clip overwrite the last, so each
 * clip's JSON lands next to the clip as `<clip>.json`.
 *
 * @param {string[]} clipPaths
 * @param {{model?: string, extraArgs?: string[], language?: string}} [opts]
 * @returns {string[]} one decoded text per clip, in the order given ('' where the clip decoded to
 *   nothing at all, which for this caller is a meaningful answer rather than a failure)
 */
export function transcribeClips(clipPaths, opts = {}) {
  const paths = (clipPaths || []).filter(Boolean);
  if (!paths.length) return [];
  const modelId = opts.model || DEFAULT_MODEL;
  const modelPath = resolveModel(modelId);
  // The clip probe is a CONTROL: the deletion detector believes a span was deleted because this
  // re-decode disagreed with the main pass. A control decoded by a different binary or different
  // weights than the run it is judging is not a control, so it is gated identically — and by then
  // the memo means it costs nothing.
  checkToolchain(modelPath, modelId);
  withPrivateOutputs(paths.map((p) => `${p}.json`), (outputs) => {
    // Without -of, Whisper derives JSON filenames from its input filenames.
    const inputs = outputs.map((output, i) => {
      const input = output.slice(0, -5);
      fs.copyFileSync(paths[i], input);
      return input;
    });
    execFileSync(
      whisperBin(),
      ['-m', modelPath, ...whisperArgs({ extraArgs: opts.extraArgs, language: opts.language }), ...inputs],
      { stdio: ['ignore', 'ignore', 'ignore'] },
    );
  });
  return paths.map((p) => {
    try {
      const json = JSON.parse(fs.readFileSync(`${p}.json`, 'utf8'));
      return (json.transcription || [])
        .map((r) => String(r?.text ?? ''))
        .join(' ')
        .replace(/\s+/g, ' ')
        .trim();
    } catch {
      return '';
    }
  });
}

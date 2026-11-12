/**
 * RichOS local service — pipeline stage 2: NORMALIZE (ffmpeg).
 *
 * Concatenate the session's self-contained audio parts, then split the stereo contract stream into
 * two 16 kHz mono WAVs — LEFT = me (mic), RIGHT = others (system/tab). This is exactly the
 * production step the benchmark measured (webm/opus -> 16 kHz mono WAV), extended with the
 * channelsplit the 2-channel contract makes free (the system architecture §4, stage 2 + §3.1).
 *
 * The contract does not mandate a codec (§3.1) — ffmpeg decodes whatever the surface wrote
 * (Opus-in-WebM from the extension; Opus-in-Ogg or WAV from a companion). Parts are self-contained
 * so a corrupt part cannot poison the rest.
 */

import fs from 'node:fs';
import path from 'node:path';
import { execFileSync, spawnSync } from 'node:child_process';
import { ffmpegBin } from './config.js';
import { privateDirectory, writePrivateFile, withPrivateOutputs } from './private-files.js';

export const SAMPLE_RATE = 16000;
export const CHANNEL_FILES = { me: 'me.wav', others: 'others.wav' };

/** Audio part files, in stable numeric order, as written by any capture surface. */
export function listAudioParts(sessionDir) {
  return fs
    .readdirSync(sessionDir)
    .filter((f) => /^audio-part-\d+\.[a-z0-9]+$/i.test(f))
    .sort((a, b) => a.localeCompare(b, undefined, { numeric: true }));
}

/** ffmpeg -version's first line, for the pipeline provenance record. */
export function ffmpegVersion() {
  try {
    const out = execFileSync(ffmpegBin(), ['-version'], { encoding: 'utf8' });
    return out.split(/\r?\n/)[0].trim();
  } catch {
    return null;
  }
}

/**
 * Number of channels in an audio file, via ffprobe-free ffmpeg stderr parse (ffprobe may be absent).
 * @param {string} file
 * @returns {number}
 */
function channelCount(file) {
  try {
    const res = execFileSync(ffmpegBin(), ['-i', file, '-hide_banner'], {
      encoding: 'utf8',
      stdio: ['ignore', 'ignore', 'pipe'],
    });
    return parseChannels(res);
  } catch (err) {
    // ffmpeg exits non-zero on `-i` with no output; the info we want is on stderr regardless.
    return parseChannels(String(err.stderr || ''));
  }
}

/**
 * Peak/mean volume of a WAV via ffmpeg `volumedetect` — a deterministic energy probe that does not
 * depend on the ASR (whisper hallucinates "Thank you." on pure silence, so word count alone cannot
 * distinguish a silent capture from a real one). Returns dBFS; -inf/very-low = digital silence.
 * @param {string} wavPath
 * @returns {{meanDb: number, maxDb: number}}
 */
export function measureVolume(wavPath) {
  // volumedetect prints its stats to stderr and ffmpeg exits 0, so use spawnSync (execFileSync only
  // surfaces stderr when the process throws).
  const res = spawnSync(ffmpegBin(), ['-i', wavPath, '-af', 'volumedetect', '-f', 'null', '-'], {
    encoding: 'utf8',
  });
  return parseVolume(String(res.stderr || ''));
}

/** @param {string} ffmpegStderr */
export function parseVolume(ffmpegStderr) {
  const mean = ffmpegStderr.match(/mean_volume:\s*(-?[\d.]+|-inf)\s*dB/i);
  const max = ffmpegStderr.match(/max_volume:\s*(-?[\d.]+|-inf)\s*dB/i);
  const num = (m) => (m ? (/-inf/i.test(m[1]) ? -Infinity : Number(m[1])) : -Infinity);
  return { meanDb: num(mean), maxDb: num(max) };
}

/** dBFS at/below which a channel is treated as digitally silent (a dead/empty capture). */
export const SILENCE_MAX_DB = -60;

/**
 * How far below a channel's own PEAK the speech/silence boundary sits, in dB. The two capture
 * setups in the 2026-08-29 real-audio corpus are not level-matched (64 vs 122 kbps, -34.6 vs
 * -30.5 dBFS mean), so a shared ABSOLUTE threshold is invalid — it must be relative to the channel.
 * 34 dB down from peak reproduces the -35 dB threshold that brief verified by hand against four
 * windows on channels peaking at -0.9 / -1.2 dBFS.
 */
export const SPEECH_FLOOR_BELOW_PEAK_DB = 34;

/** Minimum gap that counts as a pause between two spoken deliveries, seconds. */
export const MIN_PAUSE_SEC = 0.4;

/**
 * Parse `silencedetect` + the input banner into SPEECH intervals (the complement of the silences).
 * PURE, so the burst structure is unit-testable without ffmpeg.
 * @param {string} ffmpegStderr
 * @param {number} [durationSec] falls back to the banner's `Duration:` when omitted
 * @returns {{speech: {startMs:number, endMs:number}[], silence: {startMs:number, endMs:number}[],
 *            durationSec: number}}
 */
export function parseSilenceLog(ffmpegStderr, durationSec) {
  const s = String(ffmpegStderr || '');
  let total = Number(durationSec);
  if (!Number.isFinite(total) || total <= 0) {
    const d = s.match(/Duration:\s*(\d+):(\d+):(\d+(?:\.\d+)?)/);
    total = d ? Number(d[1]) * 3600 + Number(d[2]) * 60 + Number(d[3]) : 0;
  }
  const starts = [];
  const ends = [];
  for (const line of s.split(/\r?\n/)) {
    const a = line.match(/silence_start:\s*(-?[\d.]+)/);
    if (a) starts.push(Math.max(0, Number(a[1])));
    const b = line.match(/silence_end:\s*(-?[\d.]+)/);
    if (b) ends.push(Number(b[1]));
  }
  const silence = [];
  for (let i = 0; i < starts.length; i += 1) {
    const from = starts[i];
    const to = i < ends.length ? ends[i] : total; // a trailing silence has no silence_end
    if (to > from) silence.push([from, to]);
  }
  silence.sort((x, y) => x[0] - y[0]);
  const speech = [];
  let cur = 0;
  for (const [from, to] of silence) {
    if (from > cur) speech.push([cur, from]);
    cur = Math.max(cur, to);
  }
  if (total > cur) speech.push([cur, total]);
  const ms = (iv) => iv
    .filter(([from, to]) => to - from > 0.05)
    .map(([from, to]) => ({ startMs: Math.round(from * 1000), endMs: Math.round(to * 1000) }));
  return { speech: ms(speech), silence: ms(silence), durationSec: total };
}

/**
 * The channel's PHYSICAL speech-burst structure — where there is energy and where there is not.
 *
 * This is the model-free evidence the 2026-08-29 real-audio brief used to decide, one finding at a
 * time, whether a repeated phrase was a human retake or a decoder loop: saying an N-word phrase K
 * times requires K SEPARATE bursts each long enough to hold it, and no decoder output can conjure
 * those. `repetition-guard.js` consumes this as injected data and stays pure.
 *
 * @param {string} wavPath
 * @param {{noiseDb?: number, minPauseSec?: number, peakDb?: number}} [opts]
 * @returns {{speech: {startMs:number,endMs:number}[], silence: {startMs:number,endMs:number}[],
 *            durationSec: number, noiseDb: number}}
 */
export function detectSpeechBursts(wavPath, opts = {}) {
  const peak = opts.peakDb != null ? Number(opts.peakDb) : measureVolume(wavPath).maxDb;
  const noiseDb = opts.noiseDb != null
    ? Number(opts.noiseDb)
    : Math.max(-55, Math.min(-25, (Number.isFinite(peak) ? peak : 0) - SPEECH_FLOOR_BELOW_PEAK_DB));
  const minPause = opts.minPauseSec != null ? Number(opts.minPauseSec) : MIN_PAUSE_SEC;
  const res = spawnSync(
    ffmpegBin(),
    ['-i', wavPath, '-af', `silencedetect=noise=${noiseDb}dB:d=${minPause}`, '-f', 'null', '-'],
    { encoding: 'utf8', maxBuffer: 64 * 1024 * 1024 },
  );
  return { ...parseSilenceLog(String(res.stderr || '')), noiseDb };
}

/**
 * Is a normalized session effectively silent on BOTH channels? That is the honest "captured but
 * nothing recorded" signal, independent of ASR hallucination.
 * @param {{me: string, others: string}} channels
 * @returns {{silent: boolean, me: {meanDb:number,maxDb:number}, others: {meanDb:number,maxDb:number}}}
 */
export function detectSilence(channels) {
  const me = measureVolume(channels.me);
  const others = measureVolume(channels.others);
  const silent = me.maxDb <= SILENCE_MAX_DB && others.maxDb <= SILENCE_MAX_DB;
  return { silent, me, others };
}

/** @param {string} ffmpegStderr */
export function parseChannels(ffmpegStderr) {
  const m = ffmpegStderr.match(/Audio:.*?,\s*(?:mono|stereo|(\d+)\s*channels)/i);
  if (!m) return 0;
  if (/mono/i.test(m[0])) return 1;
  if (/stereo/i.test(m[0])) return 2;
  return Number(m[1]) || 0;
}

/**
 * Normalize a session's audio into two mono 16 kHz WAV channels.
 *
 * @param {string} sessionDir absolute path to the session directory
 * @param {{workDir?: string}} [opts] where to put intermediate files (defaults to sessionDir)
 * @returns {{me: string, others: string, parts: string[], stereo: boolean, ffmpeg: string|null}}
 */
export function normalizeSession(sessionDir, opts = {}) {
  const ffmpeg = ffmpegBin();
  const flags = ['-y', '-v', 'error'];
  const parts = listAudioParts(sessionDir);
  if (parts.length === 0) throw new Error('normalize: no audio parts to normalize');
  const workDir = privateDirectory(opts.workDir || sessionDir);

  // 1) Concatenate parts. Single part -> use it directly; many -> ffmpeg concat demuxer.
  let concatInput;
  const cleanup = [];
  if (parts.length === 1) {
    concatInput = path.join(sessionDir, parts[0]);
  } else {
    const listFile = path.join(workDir, '_concat.txt');
    writePrivateFile(
      listFile,
      parts.map((p) => `file '${path.join(sessionDir, p).replace(/'/g, "'\\''")}'`).join('\n'),
    );
    concatInput = path.join(workDir, '_concat.webm');
    withPrivateOutputs([concatInput], ([output]) =>
      execFileSync(ffmpeg, [...flags, '-f', 'concat', '-safe', '0', '-i', listFile, '-c', 'copy', output]));
    cleanup.push(listFile, concatInput);
  }

  const channels = channelCount(concatInput);
  const stereo = channels >= 2;
  const mePath = path.join(workDir, CHANNEL_FILES.me);
  const othersPath = path.join(workDir, CHANNEL_FILES.others);

  withPrivateOutputs([mePath, othersPath], ([meOutput, othersOutput]) => {
    if (stereo) {
      // channelsplit gives free me-vs-them separation with no diarization model (§3.1).
      execFileSync(ffmpeg, [
        ...flags, '-i', concatInput,
        '-filter_complex', 'channelsplit=channel_layout=stereo[L][R]',
        '-map', '[L]', '-ac', '1', '-ar', String(SAMPLE_RATE), meOutput,
        '-map', '[R]', '-ac', '1', '-ar', String(SAMPLE_RATE), othersOutput,
      ]);
    } else {
      // In-person / mono fallback (§3.1): both voices on LEFT; the 2-channel split degrades to
      // single-channel gracefully. `others.wav` is a silent placeholder so downstream stays uniform.
      execFileSync(ffmpeg, [...flags, '-i', concatInput, '-ac', '1', '-ar', String(SAMPLE_RATE), meOutput]);
      execFileSync(ffmpeg, [
        ...flags, '-f', 'lavfi', '-t', '0.1', '-i', `anullsrc=r=${SAMPLE_RATE}:cl=mono`, othersOutput,
      ]);
    }
  });

  for (const f of cleanup) {
    try {
      fs.rmSync(f, { force: true });
    } catch {
      /* best-effort */
    }
  }

  return { me: mePath, others: othersPath, parts, stereo, ffmpeg: ffmpegVersion() };
}

/**
 * Cut one span out of a channel as its own WAV, padded, for an ISOLATED re-decode.
 *
 * The deletion detector's adjudicator (`deletion-guard.js`) needs the audio of a suspect span with
 * no prior context, no chunk seam and no neighboring repetition — which is a clip, cut here
 * because ffmpeg lives in this module. Fast-seek (`-ss` before `-i`) so the cost is independent of
 * how far into a 92-minute file the span sits.
 *
 * @param {string} wavPath the channel WAV
 * @param {{startMs:number, endMs:number}} span
 * @param {string} outPath where to write the clip
 * @param {{padSec?: number}} [opts]
 * @returns {string} outPath
 */
export function cutSpan(wavPath, span, outPath, opts = {}) {
  const pad = opts.padSec != null ? Number(opts.padSec) : 0.3;
  const from = Math.max(0, (Number(span.startMs) || 0) / 1000 - pad);
  const to = (Number(span.endMs) || 0) / 1000 + pad;
  withPrivateOutputs([outPath], ([output]) => execFileSync(ffmpegBin(), [
    '-y', '-v', 'error',
    '-ss', String(from),
    '-t', Math.max(0.05, to - from).toFixed(3),
    '-i', wavPath,
    '-ac', '1', '-ar', String(SAMPLE_RATE),
    output,
  ]));
  return outPath;
}

/**
 * The physical level of a span of a channel, WITHOUT the padding a decode clip carries.
 *
 * Deliberately measured on the span itself: the deletion detector's level condition asks whether
 * THAT span holds speech-level energy, and padding it with the loud speech either side would
 * answer a different question, and answer it wrongly every single time.
 *
 * @param {string} wavPath
 * @param {{startMs:number, endMs:number}} span
 * @returns {{meanDb:number, maxDb:number}}
 */
export function measureSpanVolume(wavPath, span) {
  const from = Math.max(0, (Number(span.startMs) || 0) / 1000);
  const dur = Math.max(0.01, ((Number(span.endMs) || 0) - (Number(span.startMs) || 0)) / 1000);
  const res = spawnSync(
    ffmpegBin(),
    ['-v', 'info', '-ss', String(from), '-t', dur.toFixed(3), '-i', wavPath, '-af', 'volumedetect', '-f', 'null', '-'],
    { encoding: 'utf8', maxBuffer: 32 * 1024 * 1024 },
  );
  return parseVolume(String(res.stderr || ''));
}

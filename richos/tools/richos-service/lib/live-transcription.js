import fs from 'node:fs';
import os from 'node:os';
import path from 'node:path';
import { spawn } from 'node:child_process';
import { createHash } from 'node:crypto';
import { Readable } from 'node:stream';
import { pipeline } from 'node:stream/promises';
import { setTimeout as delay } from 'node:timers/promises';
import { Worker } from 'node:worker_threads';
import { ffmpegBin, resolveModel, whisperArgs } from './config.js';
import { CHANNEL_FILES, cutSpan, listAudioParts, normalizeSession, stereoNormalizationArgs } from './normalize.js';
import { log } from './log.js';
import { checkToolchain, transcribeChannel, transcribeSession } from './transcribe.js';
import { privateDirectory, privateFile, withPrivateOutputs, writePrivateFile } from './private-files.js';

const SOURCE = 'desktop-companion-macos';
const AUDIO = 'audio-part-00.wav';
const HEADER_BYTES = 44;
const FRAME_BYTES = 4;
const CORE_SECONDS = 24;
const CUT_SEARCH_SECONDS = 3;
const MAX_WINDOW_SECONDS = 120;
const CHECKPOINTS = '.live-transcription';
const NORMALIZED_AUDIO = 'normalized-audio';
const NORMALIZATION_RECEIPT = 'source.json';
const NORMALIZE_BLOCK_FRAMES = 256 * 1024;

function readHeader(fd) {
  // The native WavWriter uses this fixed PCM layout. The data length is rewritten on
  // each flush, so bytes still being appended beyond that length are left for a later pass.
  const header = Buffer.alloc(HEADER_BYTES);
  if (fs.readSync(fd, header, 0, header.length, 0) !== header.length
    || header.toString('ascii', 0, 4) !== 'RIFF'
    || header.toString('ascii', 8, 16) !== 'WAVEfmt '
    || header.readUInt32LE(16) !== 16
    || header.readUInt16LE(20) !== 1
    || header.readUInt16LE(22) !== 2
    || header.readUInt16LE(32) !== FRAME_BYTES
    || header.readUInt16LE(34) !== 16
    || header.toString('ascii', 36, 40) !== 'data') {
    throw new Error('unsupported native WAV layout');
  }
  const rate = header.readUInt32LE(24);
  const bytes = header.readUInt32LE(40);
  if (rate < 8_000 || rate > 192_000 || header.readUInt32LE(28) !== rate * FRAME_BYTES
    || bytes % FRAME_BYTES !== 0 || bytes + HEADER_BYTES > fs.fstatSync(fd).size) {
    throw new Error('invalid native WAV length or sample rate');
  }
  return { header, rate, frames: bytes / FRAME_BYTES };
}

function readPcm(fd, from, to) {
  const pcm = Buffer.allocUnsafe((to - from) * FRAME_BYTES);
  let read = 0;
  while (read < pcm.length) {
    const n = fs.readSync(fd, pcm, read, pcm.length - read, HEADER_BYTES + from * FRAME_BYTES + read);
    if (n === 0) throw new Error('native audio changed while reading a chunk');
    read += n;
  }
  return pcm;
}

function readWindow(fd, header, from, to) {
  const pcm = readPcm(fd, from, to);
  const wav = Buffer.from(header);
  wav.writeUInt32LE(HEADER_BYTES - 8 + pcm.length, 4);
  wav.writeUInt32LE(pcm.length, 40);
  return Buffer.concat([wav, pcm]);
}

function cutNear(fd, nominal, rate) {
  const radius = CUT_SEARCH_SECONDS * rate;
  const start = nominal - radius;
  const pcm = readPcm(fd, start, nominal + radius);
  const block = Math.round(rate / 50);
  const margin = block * 4;
  let best = null;
  let bestDistance = Infinity;
  let quietStart = 0;
  for (let frame = 0; frame + block <= radius * 2; frame += block) {
    let left = 0;
    let right = 0;
    for (let i = frame * FRAME_BYTES; i < (frame + block) * FRAME_BYTES; i += FRAME_BYTES) {
      left += pcm.readInt16LE(i) ** 2;
      right += pcm.readInt16LE(i + 2) ** 2;
    }
    // A cut needs quiet on both sides and on both channels.
    const end = frame + block;
    if (Math.max(left, right) > block * 64 ** 2) {
      quietStart = end;
    } else if (end - quietStart >= margin * 2) {
      const candidate = Math.max(start + quietStart + margin, Math.min(nominal, start + end - margin));
      const distance = Math.abs(candidate - nominal);
      if (distance < bestDistance) {
        best = candidate;
        bestDistance = distance;
      }
    }
  }
  return best;
}

function scanForQuietCut(fd, nominal, available, rate) {
  const radius = CUT_SEARCH_SECONDS * rate;
  for (; nominal + radius <= available; nominal += radius) {
    const cut = cutNear(fd, nominal, rate);
    if (cut !== null) return { cut, next: nominal };
  }
  return { cut: null, next: nominal };
}

// Whisper can start its first segment at zero even when the channel was quiet
// for seconds. Find the first sustained audible block on that channel.
function firstAudibleMs(wav, channelOffset, rate) {
  const block = Math.round(rate / 50);
  const bytes = block * FRAME_BYTES;
  let previousAudible = false;
  for (let from = HEADER_BYTES; from + bytes <= wav.length; from += bytes) {
    let power = 0;
    for (let i = from + channelOffset; i < from + bytes; i += FRAME_BYTES) {
      power += wav.readInt16LE(i) ** 2;
    }
    const audible = power > block * 64 ** 2;
    if (audible && previousAudible) return (from - HEADER_BYTES - bytes) * 1_000 / (rate * FRAME_BYTES);
    previousAudible = audible;
  }
  return null;
}

function absoluteSegments(segments, from, to, rate, firstAudible) {
  const offset = from * 1_000 / rate;
  const limit = to * 1_000 / rate;
  return segments.map((segment) => {
    const startMs = Math.max(offset, Math.min(limit, segment.startMs + offset));
    const endMs = Math.max(startMs, Math.min(limit, segment.endMs + offset));
    const result = { ...segment, startMs, endMs };
    if (firstAudible !== null) {
      const audibleStartMs = firstAudible + offset;
      if (startMs + 1_000 <= audibleStartMs && audibleStartMs < endMs) {
        // Use the measured onset for display and keep Whisper's original times
        // for the existing transcription guards.
        result.displayStartMs = audibleStartMs;
      }
    }
    if (segment.wordTimesMs) {
      result.wordTimesMs = segment.wordTimesMs.map((ms) => ms + offset).filter((ms) => ms >= offset && ms <= limit);
    }
    return result;
  });
}

function decodeWindow(cache, wav, options, knownToolchain) {
  let me = false;
  let others = false;
  for (let i = HEADER_BYTES; i < wav.length && !(me && others); i += FRAME_BYTES) {
    me ||= wav.readInt16LE(i) !== 0;
    others ||= wav.readInt16LE(i + 2) !== 0;
  }
  const toolchain = knownToolchain ?? checkToolchain(resolveModel(options.model), options.model);
  const result = { me: [], others: [], toolchain, skippedSilentChannels: Number(!me) + Number(!others) };
  // Only digital zero is safe to skip. Keep even the quietest nonzero audio.
  if (!me && !others) return result;
  const work = fs.mkdtempSync(path.join(cache, 'decode-'));
  try {
    writePrivateFile(path.join(work, AUDIO), wav, work);
    const channels = normalizeSession(work);
    for (const speaker of ['me', 'others']) {
      if (speaker === 'me' ? !me : !others) continue;
      const decoded = transcribeChannel(channels[speaker], speaker, {
        ...options, outDir: work, timeoutMs: 90_000, quiet: true,
      });
      if (decoded.toolchain?.provenance !== toolchain.provenance) {
        throw new Error('native decoder changed while transcribing a chunk');
      }
      result[speaker] = decoded.segments;
    }
    return result;
  } finally {
    fs.rmSync(work, { recursive: true, force: true });
  }
}

// The normalizer has already created the two 16 kHz files at close. Recover a
// long uninterrupted stretch from those files without allocating its raw WAV.
export function decodeLongWindow(cache, channels, fromMs, toMs, options) {
  const work = fs.mkdtempSync(path.join(cache, 'decode-'));
  try {
    const span = { startMs: fromMs, endMs: toMs };
    const clips = Object.fromEntries(['me', 'others'].map((speaker) => [
      speaker, cutSpan(channels[speaker], span, path.join(work, `${speaker}-gap.wav`), { padSec: 0 }),
    ]));
    return transcribeSession(clips, { ...options, outDir: work, quiet: true });
  } finally {
    fs.rmSync(work, { recursive: true, force: true });
  }
}

function validCheckpointRows(rows, fromMs, toMs) {
  const inWindow = (time) => Number.isFinite(time) && time >= fromMs && time <= toMs;
  return Array.isArray(rows) && rows.every((row) => row && typeof row.text === 'string'
    && inWindow(row.startMs) && inWindow(row.endMs) && row.startMs <= row.endMs
    && (row.displayStartMs === undefined
      || (inWindow(row.displayStartMs) && row.startMs <= row.displayStartMs && row.displayStartMs <= row.endMs))
    && (row.wordTimesMs === undefined || (Array.isArray(row.wordTimesMs) && row.wordTimesMs.every(inWindow))));
}

function readCheckpoint(file, key, fromMs, toMs) {
  try {
    const value = JSON.parse(fs.readFileSync(privateFile(file), 'utf8'));
    if (value?.key === key && validCheckpointRows(value.me, fromMs, toMs)
      && validCheckpointRows(value.others, fromMs, toMs)) return value;
  } catch (error) {
    if (!(error instanceof SyntaxError) && error.code !== 'ENOENT') throw error;
  }
  return null;
}

function checkpointKey(wav, from, to, options, toolchain) {
  return createHash('sha256')
    .update(JSON.stringify([5, from, to, options.model, whisperArgs(options), toolchain.provenance]))
    .update(wav)
    .digest('hex');
}

/**
 * Decode contiguous windows ending at quiet points in the durable audio.
 * Keep listening across a nominal boundary when speech is still in progress.
 */
export function advanceLiveTranscription(dir, options, {
  closed = false, cursor = 0, scanFrom, decode = decodeWindow, recover,
  recorderIsRunning = () => true,
  toolchain = checkToolchain(resolveModel(options.model), options.model),
} = {}) {
  const cache = privateDirectory(path.join(dir, CHECKPOINTS), dir);
  const fd = fs.openSync(privateFile(path.join(dir, AUDIO), dir), 'r');
  try {
    const { header, frames, rate } = readHeader(fd);
    const output = { me: [], others: [], model: options.model, toolchain, whisper: toolchain.provenance };
    let reused = 0;
    let decoded = 0;
    let skippedSilentChannels = 0;
    let longWindows = 0;
    while (cursor < frames && recorderIsRunning()) {
      const boundary = scanForQuietCut(fd, scanFrom ?? cursor + CORE_SECONDS * rate, frames, rate);
      const to = boundary.cut ?? (closed ? frames : null);
      scanFrom = boundary.next;
      if (to === null) break;
      if (to - cursor > MAX_WINDOW_SECONDS * rate) {
        // Resume after a genuinely quiet point. The closed pass alone recovers
        // the uncheckpointed stretch, so live work never cuts through speech.
        if (closed) {
          if (!recover) throw new Error('no quiet meeting boundary within two minutes');
          const segments = recover(cache, cursor * 1_000 / rate, to * 1_000 / rate, options);
          if (segments.toolchain?.provenance !== toolchain.provenance) {
            throw new Error('native decoder changed while recovering a long chunk');
          }
          output.me.push(...absoluteSegments(segments.me, cursor, to, rate, null));
          output.others.push(...absoluteSegments(segments.others, cursor, to, rate, null));
        }
        longWindows += 1;
        cursor = to;
        scanFrom = undefined;
        continue;
      }
      const wav = readWindow(fd, header, cursor, to);
      const key = checkpointKey(wav, cursor, to, options, toolchain);
      const file = path.join(cache, `${cursor}.json`);
      let result = readCheckpoint(file, key, cursor * 1_000 / rate, to * 1_000 / rate);
      if (result) {
        reused += 1;
      } else {
        const segments = decode(cache, wav, options, toolchain);
        if (segments.toolchain?.provenance !== toolchain.provenance) {
          throw new Error('native decoder changed while transcribing a chunk');
        }
        result = {
          key,
          me: absoluteSegments(segments.me, cursor, to, rate, firstAudibleMs(wav, 0, rate)),
          others: absoluteSegments(segments.others, cursor, to, rate, firstAudibleMs(wav, 2, rate)),
        };
        withPrivateOutputs([file], ([temp]) => writePrivateFile(temp, JSON.stringify(result)));
        skippedSilentChannels += segments.skippedSilentChannels ?? 0;
        decoded += 1;
      }
      output.me.push(...result.me);
      output.others.push(...result.others);
      cursor = to;
      scanFrom = undefined;
    }
    return { asr: output, cursor, scanFrom, reused, decoded, skippedSilentChannels, longWindows };
  } finally {
    fs.closeSync(fd);
  }
}

export function supportsLiveTranscription(dir, record) {
  if (record?.capture?.source !== SOURCE) return false;
  const parts = listAudioParts(dir);
  return parts.length === 1 && parts[0] === AUDIO;
}

export function recorderIsRunning(pid) {
  try {
    process.kill(pid, 0);
    return true;
  } catch {
    return false;
  }
}

function fileIdentity(stat) {
  return [stat.dev, stat.ino, stat.size, stat.mtimeNs, stat.ctimeNs].join(':');
}

function identifyFile(file) {
  return fileIdentity(fs.statSync(file, { bigint: true }));
}

// Native capture appends PCM and only rewrites the RIFF header. Give the existing
// normalizer the flushed samples in order, with one resampler for the whole call.
export async function prepareLiveNormalization(dir, isRecording, signal) {
  const audio = privateFile(path.join(dir, AUDIO), dir);
  const fd = fs.openSync(audio, fs.constants.O_RDONLY | fs.constants.O_NOFOLLOW);
  let work;
  let child;
  let stopped;
  try {
    const { rate } = readHeader(fd);
    const ffmpeg = ffmpegBin();
    const converter = identifyFile(ffmpeg);
    const cache = privateDirectory(path.join(dir, CHECKPOINTS), dir);
    work = fs.mkdtempSync(path.join(cache, 'decode-'));
    const outputs = Object.values(CHANNEL_FILES).map((name) => path.join(work, name));
    child = spawn(ffmpeg, stereoNormalizationArgs(
      ['-f', 's16le', '-ar', String(rate), '-ac', '2', '-i', 'pipe:0'], ...outputs,
    ), { stdio: ['pipe', 'ignore', 'ignore'] });
    stopped = new Promise((resolve) => {
      child.once('error', () => resolve(false));
      child.once('close', (code) => resolve(code === 0));
    });
    let source;
    async function* flushedAudio() {
      let cursor = 0;
      for (;;) {
        signal.throwIfAborted();
        const running = isRecording();
        const header = readHeader(fd);
        if (header.rate !== rate || header.frames < cursor) throw new Error('native audio changed while normalizing');
        while (cursor < header.frames) {
          const to = Math.min(cursor + NORMALIZE_BLOCK_FRAMES, header.frames);
          yield readPcm(fd, cursor, to);
          cursor = to;
        }
        if (!running) {
          const stat = fs.fstatSync(fd, { bigint: true });
          if (stat.size !== BigInt(HEADER_BYTES + cursor * FRAME_BYTES)) {
            throw new Error('native audio was not fully flushed');
          }
          source = fileIdentity(stat);
          return;
        }
        await delay(500, undefined, { signal });
      }
    }
    await pipeline(Readable.from(flushedAudio()), child.stdin, { signal });
    if (!(await stopped)) throw new Error('the live audio converter failed');
    const record = JSON.parse(fs.readFileSync(privateFile(path.join(dir, 'session.json'), dir), 'utf8'));
    if (record.status !== 'closed' || !supportsLiveTranscription(dir, record)
      || source !== identifyFile(audio) || converter !== identifyFile(ffmpegBin())) return;
    const tracks = outputs.map((file) => identifyFile(privateFile(file, work)));
    const readyAt = Date.now();
    writePrivateFile(path.join(work, NORMALIZATION_RECEIPT), JSON.stringify({ source, converter, tracks, readyAt }), work);
    fs.renameSync(work, path.join(cache, NORMALIZED_AUDIO));
  } finally {
    fs.closeSync(fd);
    if (child && child.exitCode === null && child.signalCode === null) {
      child.kill();
      await stopped;
    }
    if (work) fs.rmSync(work, { recursive: true, force: true });
  }
}

/** Consume only a complete conversion of this closed native recording. */
export function takeLiveNormalization(dir) {
  const prepared = path.join(dir, CHECKPOINTS, NORMALIZED_AUDIO);
  const names = Object.values(CHANNEL_FILES);
  let cache;
  try {
    if (!fs.lstatSync(prepared, { throwIfNoEntry: false })?.isDirectory()) return null;
    cache = privateDirectory(prepared, dir);
    const { source, converter, tracks, readyAt } = JSON.parse(fs.readFileSync(
      privateFile(path.join(cache, NORMALIZATION_RECEIPT), dir), 'utf8',
    ));
    const files = names.map((name) => privateFile(path.join(cache, name), dir));
    if (source !== identifyFile(privateFile(path.join(dir, AUDIO), dir)) || converter !== identifyFile(ffmpegBin())
      || !files.every((file, index) => identifyFile(file) === tracks?.[index])) return null;
    const outputs = names.map((name) => privateFile(path.join(dir, name), dir));
    withPrivateOutputs(outputs, (staging) => {
      for (const [index, file] of files.entries()) fs.renameSync(file, staging[index]);
    });
    return { me: outputs[0], others: outputs[1], parts: [AUDIO], stereo: true, ffmpeg: null, readyAt };
  } catch {
    return null;
  } finally {
    if (cache) {
      try {
        for (const name of [...names, NORMALIZATION_RECEIPT]) fs.rmSync(path.join(cache, name), { force: true });
        fs.rmdirSync(cache);
      } catch {
        // The retained recording can still be processed.
      }
    }
  }
}

/** Remove staging left by a worker terminated before its cleanup could run. */
export function removeLiveTranscriptionScratch(dir, { keepPrepared = false } = {}) {
  const candidate = path.join(dir, CHECKPOINTS);
  if (!fs.lstatSync(candidate, { throwIfNoEntry: false })) return;
  const cache = privateDirectory(candidate, dir);
  for (const entry of fs.readdirSync(cache, { withFileTypes: true })) {
    if (entry.isDirectory() && (/^(?:decode-|\.richos-output-)[a-zA-Z0-9]{6}$/.test(entry.name)
      || (entry.name === NORMALIZED_AUDIO && !keepPrepared))) {
      fs.rmSync(path.join(cache, entry.name), { recursive: true, force: true });
    }
  }
}

function reuseExcerptCheckpoints(dir, options, fd, { header, frames, rate }, from, through, knownToolchain) {
  const cached = { through: from, me: [], others: [] };
  const candidate = path.join(dir, CHECKPOINTS);
  if (!fs.lstatSync(candidate, { throwIfNoEntry: false })) return cached;
  const cache = privateDirectory(candidate, dir);
  const starts = fs.readdirSync(cache)
    .filter((name) => /^\d+\.json$/.test(name))
    .map((name) => Number.parseInt(name, 10))
    .filter((start) => start < through)
    .sort((a, b) => a - b);
  const startIndex = starts.findLastIndex((start) => start <= from);
  if (startIndex < 0) return cached;
  const toolchain = knownToolchain ?? checkToolchain(resolveModel(options.model), options.model);
  let cursor = starts[startIndex];
  const fromMs = from * 1_000 / rate;
  const withinExcerpt = (row) => (row.displayStartMs ?? row.startMs) >= fromMs;
  const crossesStart = (row) => !withinExcerpt(row) && row.endMs > fromMs;
  for (const start of starts.slice(startIndex)) {
    if (start < cursor) continue;
    if (start > cursor) break;
    const bound = Math.min(frames, start + MAX_WINDOW_SECONDS * rate + CUT_SEARCH_SECONDS * rate);
    const { cut: to } = scanForQuietCut(fd, start + CORE_SECONDS * rate, bound, rate);
    if (to === null || to > through) break;
    const wav = readWindow(fd, header, start, to);
    const saved = readCheckpoint(path.join(cache, `${start}.json`), checkpointKey(wav, start, to, options, toolchain),
      start * 1_000 / rate, to * 1_000 / rate);
    if (!saved) break;
    const me = firstAudibleMs(wav, 0, rate) === null ? [] : saved.me;
    const others = firstAudibleMs(wav, 2, rate) === null ? [] : saved.others;
    // A whole checkpoint cannot give us only the words after a cut through speech.
    if (me.some(crossesStart) || others.some(crossesStart)) break;
    cached.me.push(...me.filter(withinExcerpt));
    cached.others.push(...others.filter(withinExcerpt));
    cached.through = Math.max(from, to);
    cursor = to;
  }
  return cached;
}

/** Reuse verified checkpoints in the flushed minute leading up to the request
 * and decode only audio the live worker has yet to reach. */
export function recentLiveExcerpt(dir, options, at, decode = decodeWindow, knownToolchain) {
  const record = JSON.parse(fs.readFileSync(privateFile(path.join(dir, 'session.json'), dir), 'utf8'));
  if (record.status !== 'open' || !supportsLiveTranscription(dir, record)
    || !Number.isSafeInteger(at) || !Number.isSafeInteger(record.startedAt) || at <= record.startedAt) return null;
  const fd = fs.openSync(privateFile(path.join(dir, AUDIO), dir), 'r');
  try {
    const audio = readHeader(fd);
    const { header, frames, rate } = audio;
    const through = Math.min(frames, Math.floor((at - record.startedAt) * rate / 1_000));
    if (!through) return null;
    const from = Math.max(0, through - 60 * rate);
    const cached = reuseExcerptCheckpoints(dir, options, fd, audio, from, through, knownToolchain);
    if (cached.through < through) {
      const wav = readWindow(fd, header, cached.through, through);
      const meAudible = firstAudibleMs(wav, 0, rate);
      const othersAudible = firstAudibleMs(wav, 2, rate);
      if (meAudible !== null || othersAudible !== null) {
        const work = fs.mkdtempSync(path.join(os.tmpdir(), 'richbrief-excerpt-'));
        try {
          const decoded = decode(work, wav, options, knownToolchain);
          if (meAudible !== null) cached.me.push(...absoluteSegments(decoded.me, cached.through, through, rate, meAudible));
          if (othersAudible !== null) cached.others.push(...absoluteSegments(decoded.others, cached.through, through, rate, othersAudible));
        } finally {
          fs.rmSync(work, { recursive: true, force: true });
        }
      }
    }
    const passage = (segment, speaker) => ({
      speaker,
      atMs: segment.displayStartMs ?? segment.startMs,
      text: segment.text,
    });
    const passages = [...cached.me.map((row) => passage(row, 'Me')), ...cached.others.map((row) => passage(row, 'Them'))]
      .sort((a, b) => a.atMs - b.atMs);
    if (!passages.length) return null;
    return { fromMs: from * 1_000 / rate, throughMs: through * 1_000 / rate, passages };
  } finally {
    fs.closeSync(fd);
  }
}

export async function watchLiveTranscription(dir, options, recorderPid, onChunk) {
  let cursor = 0;
  let scanFrom;
  const toolchain = checkToolchain(resolveModel(options.model), options.model);
  const isRecording = () => recorderIsRunning(recorderPid);
  let normalizer;
  let preparing;
  try {
    while (isRecording()) {
      const record = JSON.parse(fs.readFileSync(privateFile(path.join(dir, 'session.json'), dir), 'utf8'));
      if (record.status !== 'open' || !supportsLiveTranscription(dir, record)) {
        if (record.status !== 'closed') normalizer?.postMessage('abort');
        break;
      }
      if (!preparing) {
        try {
          // Whisper decodes synchronously. Keep the PCM feed running while it decodes.
          normalizer = new Worker(new URL('./live-normalization-worker.js', import.meta.url), {
            workerData: { dir, recorderPid },
          });
          preparing = new Promise((resolve) => {
            normalizer.once('error', () => log.warn('live audio normalization could not finish; using the retained WAV'));
            normalizer.once('exit', resolve);
          });
        } catch {
          preparing = Promise.resolve();
          log.warn('live audio normalization could not start; using the retained WAV');
        }
      }
      const start = performance.now();
      const result = advanceLiveTranscription(dir, options, { cursor, scanFrom, toolchain, recorderIsRunning: isRecording });
      cursor = result.cursor;
      scanFrom = result.scanFrom;
      if (result.decoded || result.reused || result.longWindows) {
        onChunk({
          decoded: result.decoded, reused: result.reused, skippedSilentChannels: result.skippedSilentChannels,
          longWindows: result.longWindows,
          elapsedMs: Math.round(performance.now() - start),
        });
      }
      await delay(1_000);
    }
  } catch (error) {
    normalizer?.postMessage('abort');
    throw error;
  } finally {
    await preparing;
  }
}

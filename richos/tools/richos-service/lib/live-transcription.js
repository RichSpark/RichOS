import fs from 'node:fs';
import path from 'node:path';
import { createHash } from 'node:crypto';
import { setTimeout as delay } from 'node:timers/promises';
import { resolveModel, whisperArgs } from './config.js';
import { listAudioParts, normalizeSession } from './normalize.js';
import { checkToolchain, transcribeSession } from './transcribe.js';
import { privateDirectory, privateFile, withPrivateOutputs, writePrivateFile } from './private-files.js';

const SOURCE = 'desktop-companion-macos';
const AUDIO = 'audio-part-00.wav';
const HEADER_BYTES = 44;
const FRAME_BYTES = 4;
const CORE_SECONDS = 24;
const PADDING_SECONDS = 2;
const CHECKPOINTS = '.live-transcription';

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

function readWindow(fd, header, from, to) {
  const pcm = Buffer.allocUnsafe((to - from) * FRAME_BYTES);
  let read = 0;
  while (read < pcm.length) {
    const n = fs.readSync(fd, pcm, read, pcm.length - read, HEADER_BYTES + from * FRAME_BYTES + read);
    if (n === 0) throw new Error('native audio changed while reading a chunk');
    read += n;
  }
  const wav = Buffer.from(header);
  wav.writeUInt32LE(HEADER_BYTES - 8 + pcm.length, 4);
  wav.writeUInt32LE(pcm.length, 40);
  return Buffer.concat([wav, pcm]);
}

function absoluteSegments(segments, left, right, from, to, rate) {
  const offset = left * 1_000 / rate;
  const limit = right * 1_000 / rate;
  const start = from * 1_000 / rate;
  const end = to * 1_000 / rate;
  return segments.flatMap((segment) => {
    const startMs = Math.max(offset, Math.min(limit, segment.startMs + offset));
    const endMs = Math.max(startMs, Math.min(limit, segment.endMs + offset));
    const center = (startMs + endMs) / 2;
    // Adjacent chunks hear overlapping context. Each segment belongs to the chunk
    // whose non-overlapping interval contains its center.
    if (center < start || center >= end) return [];
    const result = { ...segment, startMs, endMs };
    if (segment.wordTimesMs) {
      result.wordTimesMs = segment.wordTimesMs.map((ms) => ms + offset).filter((ms) => ms <= limit);
    }
    return [result];
  });
}

function decodeWindow(cache, wav, options) {
  const work = fs.mkdtempSync(path.join(cache, 'decode-'));
  try {
    writePrivateFile(path.join(work, AUDIO), wav, work);
    return transcribeSession(normalizeSession(work), { ...options, outDir: work, timeoutMs: 90_000, quiet: true });
  } finally {
    fs.rmSync(work, { recursive: true, force: true });
  }
}

function readCheckpoint(file, key) {
  try {
    const value = JSON.parse(fs.readFileSync(privateFile(file), 'utf8'));
    if (value?.key === key && Array.isArray(value.me) && Array.isArray(value.others)) return value;
  } catch (error) {
    if (!(error instanceof SyntaxError) && error.code !== 'ENOENT') throw error;
  }
  return null;
}

/**
 * One pass over the durable prefix. During recording, leave the last window
 * until it has right-hand context. At close, decode the remaining tail.
 */
export function advanceLiveTranscription(dir, options, {
  closed = false, cursor = 0, decode = decodeWindow,
  toolchain = checkToolchain(resolveModel(options.model), options.model),
} = {}) {
  const cache = privateDirectory(path.join(dir, CHECKPOINTS), dir);
  const fd = fs.openSync(privateFile(path.join(dir, AUDIO), dir), 'r');
  try {
    const { header, frames, rate } = readHeader(fd);
    const step = CORE_SECONDS * rate;
    const padding = PADDING_SECONDS * rate;
    const output = { me: [], others: [], model: options.model, toolchain, whisper: toolchain.provenance };
    let reused = 0;
    let decoded = 0;
    for (; cursor < frames; cursor += step) {
      if (!closed && cursor + step + padding > frames) break;
      const to = Math.min(cursor + step, frames);
      const left = Math.max(0, cursor - padding);
      const right = Math.min(frames, to + padding);
      const wav = readWindow(fd, header, left, right);
      const key = createHash('sha256')
        .update(JSON.stringify([1, left, cursor, to, right, options.model, whisperArgs(options), toolchain.provenance]))
        .update(wav)
        .digest('hex');
      const file = path.join(cache, `${cursor}.json`);
      let result = readCheckpoint(file, key);
      if (result) {
        reused += 1;
      } else {
        const segments = decode(cache, wav, options);
        if (segments.toolchain?.provenance !== toolchain.provenance) {
          throw new Error('native decoder changed while transcribing a chunk');
        }
        result = {
          key,
          me: absoluteSegments(segments.me, left, right, cursor, to, rate),
          others: absoluteSegments(segments.others, left, right, cursor, to, rate),
        };
        withPrivateOutputs([file], ([temp]) => writePrivateFile(temp, JSON.stringify(result)));
        decoded += 1;
      }
      output.me.push(...result.me);
      output.others.push(...result.others);
    }
    return { asr: output, cursor, reused, decoded };
  } finally {
    fs.closeSync(fd);
  }
}

export function supportsLiveTranscription(dir, record) {
  const parts = listAudioParts(dir);
  return record?.capture?.source === SOURCE && parts.length === 1 && parts[0] === AUDIO;
}

export async function watchLiveTranscription(dir, options, recorderIsRunning, onChunk) {
  let cursor = 0;
  const toolchain = checkToolchain(resolveModel(options.model), options.model);
  while (recorderIsRunning()) {
    const record = JSON.parse(fs.readFileSync(privateFile(path.join(dir, 'session.json'), dir), 'utf8'));
    if (record.status !== 'open' || !supportsLiveTranscription(dir, record)) return;
    const start = performance.now();
    const result = advanceLiveTranscription(dir, options, { cursor, toolchain });
    cursor = result.cursor;
    if (result.decoded || result.reused) {
      onChunk({ decoded: result.decoded, reused: result.reused, elapsedMs: Math.round(performance.now() - start) });
    }
    await delay(1_000);
  }
}

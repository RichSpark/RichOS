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
const CUT_SEARCH_SECONDS = 3;
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
  let best = nominal;
  let bestDistance = Infinity;
  let quietStart = 0;
  for (let frame = 0; frame + block <= radius * 2; frame += block) {
    let left = 0;
    let right = 0;
    for (let i = frame * FRAME_BYTES; i < (frame + block) * FRAME_BYTES; i += FRAME_BYTES) {
      left += pcm.readInt16LE(i) ** 2;
      right += pcm.readInt16LE(i + 2) ** 2;
    }
    // A cut needs quiet on both sides and on both channels. Keep overlap for calls
    // with continuous speech around the boundary.
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
  return { frame: best, quiet: bestDistance !== Infinity };
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

function absoluteSegments(segments, left, right, from, to, rate, firstAudible) {
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
    if (firstAudible !== null) {
      const audibleStartMs = firstAudible + offset;
      if (startMs + 1_000 <= audibleStartMs && audibleStartMs < endMs) {
        // Use the measured onset for display. Keep Whisper's original times for
        // segment ownership and the existing transcription guards.
        result.displayStartMs = audibleStartMs;
      }
    }
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
    const lookahead = padding + CUT_SEARCH_SECONDS * rate;
    const output = { me: [], others: [], model: options.model, toolchain, whisper: toolchain.provenance };
    let reused = 0;
    let decoded = 0;
    let previousCut;
    for (; cursor < frames; cursor += step) {
      if (!closed && cursor + step + lookahead > frames) break;
      const startCut = previousCut ?? (cursor === 0 ? { frame: 0, quiet: true } : cutNear(fd, cursor, rate));
      const endCut = cursor + step + lookahead > frames
        ? { frame: frames, quiet: true }
        : cutNear(fd, cursor + step, rate);
      const from = startCut.frame;
      const to = endCut.frame;
      const left = startCut.quiet ? from : Math.max(0, from - padding);
      const right = endCut.quiet ? to : Math.min(frames, to + padding);
      const wav = readWindow(fd, header, left, right);
      const key = createHash('sha256')
        .update(JSON.stringify([3, left, from, to, right, options.model, whisperArgs(options), toolchain.provenance]))
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
          me: absoluteSegments(segments.me, left, right, from, to, rate, firstAudibleMs(wav, 0, rate)),
          others: absoluteSegments(segments.others, left, right, from, to, rate, firstAudibleMs(wav, 2, rate)),
        };
        withPrivateOutputs([file], ([temp]) => writePrivateFile(temp, JSON.stringify(result)));
        decoded += 1;
      }
      output.me.push(...result.me);
      output.others.push(...result.others);
      previousCut = endCut;
      if (to === frames) {
        cursor = frames;
        break;
      }
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

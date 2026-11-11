import assert from 'node:assert/strict';
import fs from 'node:fs';
import os from 'node:os';
import path from 'node:path';
import { test } from 'node:test';
import { advanceLiveTranscription, supportsLiveTranscription } from '../lib/live-transcription.js';
import { mergeTranscript, renderMarkdown } from '../lib/merge.js';
import { assertPrivateTree } from '../lib/private-files.js';

test('live transcription checkpoints the flushed prefix and only decodes the tail after close', (t) => {
  const dir = fs.mkdtempSync(path.join(os.tmpdir(), 'richbrief-live-'));
  t.after(() => fs.rmSync(dir, { recursive: true, force: true }));
  const audio = path.join(dir, 'audio-part-00.wav');
  const rate = 8_000;
  const bytesPerSecond = rate * 4;
  const header = Buffer.alloc(44);
  header.write('RIFF', 0);
  header.write('WAVEfmt ', 8);
  header.writeUInt32LE(16, 16);
  header.writeUInt16LE(1, 20);
  header.writeUInt16LE(2, 22);
  header.writeUInt32LE(rate, 24);
  header.writeUInt32LE(bytesPerSecond, 28);
  header.writeUInt16LE(4, 32);
  header.writeUInt16LE(16, 34);
  header.write('data', 36);
  const pcm = Buffer.alloc(30 * bytesPerSecond);
  pcm.fill(Buffer.from([0, 8]), 21 * bytesPerSecond, 27 * bytesPerSecond);
  pcm.writeInt16LE(17, 24 * bytesPerSecond);
  pcm.writeInt16LE(-31, 24 * bytesPerSecond + 2);
  const flush = (seconds) => {
    header.writeUInt32LE(36 + seconds * bytesPerSecond, 4);
    header.writeUInt32LE(seconds * bytesPerSecond, 40);
    fs.writeFileSync(audio, Buffer.concat([header, pcm]));
  };

  const requests = [];
  const toolchain = { provenance: 'test-decoder' };
  const segment = (text, startMs, endMs) => ({ text, startMs, endMs, wordTimesMs: [startMs] });
  const decode = (_cache, wav) => {
    requests.push(wav.readUInt32LE(40) / bytesPerSecond);
    if (requests.at(-1) === 26) {
      return {
        me: [segment('First', 23_000, 24_000), segment('Next', 24_000, 25_000)],
        others: [segment('Other channel', 500, 1_000)],
        toolchain,
      };
    }
    assert.equal(wav.readInt16LE(44 + 2 * bytesPerSecond), 17);
    assert.equal(wav.readInt16LE(44 + 2 * bytesPerSecond + 2), -31);
    return { me: [segment('First', 1_000, 2_000), segment('Next', 2_000, 3_000)], others: [], toolchain };
  };
  const options = { model: 'large-v3-turbo-q5_0' };
  const advance = (state) => advanceLiveTranscription(dir, options, { decode, toolchain, ...state });

  flush(28);
  assert.equal(supportsLiveTranscription(dir, { capture: { source: 'desktop-companion-macos' } }), true);
  assert.equal(supportsLiveTranscription(dir, { capture: { source: 'chrome' } }), false);
  assert.equal(advance().decoded, 0, 'Unflushed bytes cannot fill the overlap');
  flush(29);
  const live = advance();
  assert.equal(live.decoded, 1);
  assert.equal(live.asr.me[0].text, 'First');
  assert.equal(live.asr.others[0].text, 'Other channel');
  assert.equal(advance({ cursor: live.cursor }).decoded, 0);

  flush(30);
  const finished = advance({ closed: true });
  assert.equal(finished.reused, 1);
  assert.equal(finished.decoded, 1);
  assert.deepEqual(finished.asr.me, [segment('First', 23_000, 24_000), segment('Next', 24_000, 25_000)]);
  assert.deepEqual(requests, [26, 8]);

  fs.writeFileSync(path.join(dir, '.live-transcription', '0.json'), 'truncated');
  const recovered = advance({ closed: true });
  assert.equal(recovered.reused, 1);
  assert.equal(recovered.decoded, 1);
  assert.deepEqual(recovered.asr.me, finished.asr.me);

  assert.throws(
    () => advance({ closed: true, toolchain: { provenance: 'different-decoder' } }),
    /decoder changed/,
  );
  assert.equal(advance({ closed: true }).reused, 2, 'A mismatched decoder cannot replace valid checkpoints');

  // A jointly quiet gap moves the cut out of a sentence and requires no overlap.
  pcm.fill(0);
  pcm.fill(Buffer.from([0, 8, 0, 0]), 23 * bytesPerSecond, 25 * bytesPerSecond);
  flush(30);
  const cutLengths = [];
  const quiet = advance({
    closed: true,
    decode: (_cache, wav) => {
      cutLengths.push(wav.readUInt32LE(40) / bytesPerSecond);
      return { me: [], others: [], toolchain };
    },
  });
  assert.equal(quiet.decoded, 2);
  assert.deepEqual(cutLengths, [22.92, 7.08]);

  pcm.fill(0);
  pcm.fill(Buffer.from([0, 8, 0, 0]), 4 * bytesPerSecond, 5 * bytesPerSecond);
  pcm.fill(Buffer.from([0, 0, 0, 8]), 16 * bytesPerSecond, 17 * bytesPerSecond);
  flush(20);
  const timed = advance({
    closed: true,
    decode: () => ({
      me: [segment('Local phrase.', 0, 5_000)],
      others: [segment('Remote phrase.', 0, 17_000)],
      toolchain,
    }),
  }).asr;
  assert.equal(timed.me[0].startMs, 0);
  assert.deepEqual(timed.me[0].wordTimesMs, [0]);
  const transcript = renderMarkdown(mergeTranscript(timed));
  assert.match(transcript, /\[00:04\] Me:[\s\S]*\[00:16\] Them:/);
});

test('private session validation tolerates a live worker removing its staging', (t) => {
  const dir = fs.realpathSync(fs.mkdtempSync(path.join(os.tmpdir(), 'richbrief-resume-')));
  t.after(() => fs.rmSync(dir, { recursive: true, force: true }));
  const staging = path.join(dir, '.live-transcription', 'decode-finished');
  fs.mkdirSync(staging, { recursive: true });
  const read = fs.readdirSync;
  t.mock.method(fs, 'readdirSync', (file, ...args) => {
    if (file === staging) fs.rmdirSync(staging);
    return read(file, ...args);
  });
  assert.equal(assertPrivateTree(dir), dir);
  const retained = path.join(dir, 'recording.wav');
  fs.writeFileSync(retained, '');
  fs.symlinkSync(retained, path.join(dir, 'alias.wav'));
  assert.throws(() => assertPrivateTree(dir), /regular unlinked file/);
});

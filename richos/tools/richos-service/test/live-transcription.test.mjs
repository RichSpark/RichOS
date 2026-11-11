import assert from 'node:assert/strict';
import fs from 'node:fs';
import os from 'node:os';
import path from 'node:path';
import { test } from 'node:test';
import { advanceLiveTranscription, supportsLiveTranscription } from '../lib/live-transcription.js';

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
  pcm.writeInt16LE(17, 24 * bytesPerSecond);
  pcm.writeInt16LE(-31, 24 * bytesPerSecond + 2);
  const flush = (seconds) => {
    header.writeUInt32LE(36 + seconds * bytesPerSecond, 4);
    header.writeUInt32LE(seconds * bytesPerSecond, 40);
    fs.writeFileSync(audio, Buffer.concat([header, pcm]));
  };

  const requests = [];
  const segment = (text, startMs, endMs) => ({ text, startMs, endMs, wordTimesMs: [startMs] });
  const decode = (_cache, wav) => {
    requests.push(wav.readUInt32LE(40) / bytesPerSecond);
    if (requests.at(-1) === 26) {
      return {
        me: [segment('First', 23_000, 24_000), segment('Next', 24_000, 25_000)],
        others: [segment('Other channel', 500, 1_000)],
      };
    }
    assert.equal(wav.readInt16LE(44 + 2 * bytesPerSecond), 17);
    assert.equal(wav.readInt16LE(44 + 2 * bytesPerSecond + 2), -31);
    return { me: [segment('First', 1_000, 2_000), segment('Next', 2_000, 3_000)], others: [] };
  };
  const toolchain = { provenance: 'test-decoder' };
  const options = { model: 'large-v3-turbo-q5_0' };
  const advance = (state) => advanceLiveTranscription(dir, options, { decode, toolchain, ...state });

  flush(25);
  assert.equal(supportsLiveTranscription(dir, { capture: { source: 'desktop-companion-macos' } }), true);
  assert.equal(supportsLiveTranscription(dir, { capture: { source: 'chrome' } }), false);
  assert.equal(advance().decoded, 0, 'Unflushed bytes cannot fill the overlap');
  flush(26);
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
});

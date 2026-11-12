import assert from 'node:assert/strict';
import fs from 'node:fs';
import os from 'node:os';
import path from 'node:path';
import { test } from 'node:test';
import { advanceLiveTranscription, prepareLiveNormalization, recentLiveExcerpt, removeLiveTranscriptionScratch, supportsLiveTranscription, takeLiveNormalization } from '../lib/live-transcription.js';
import { mergeTranscript, renderMarkdown } from '../lib/merge.js';
import { normalizeSession } from '../lib/normalize.js';
import { runPipeline } from '../lib/pipeline.js';
import { log } from '../lib/log.js';
import { assertPrivateTree } from '../lib/private-files.js';

function nativeHeader(rate, dataBytes = 0) {
  const header = Buffer.alloc(44);
  header.write('RIFF', 0);
  header.writeUInt32LE(36 + dataBytes, 4);
  header.write('WAVEfmt ', 8);
  header.writeUInt32LE(16, 16);
  header.writeUInt16LE(1, 20);
  header.writeUInt16LE(2, 22);
  header.writeUInt32LE(rate, 24);
  header.writeUInt32LE(rate * 4, 28);
  header.writeUInt16LE(4, 32);
  header.writeUInt16LE(16, 34);
  header.write('data', 36);
  header.writeUInt32LE(dataBytes, 40);
  return header;
}

test('live normalization uses flushed native audio and refuses a changed recording', async (t) => {
  for (const changed of [false, true]) {
    await t.test(changed ? 'changed retained audio' : 'complete conversion', async (subtest) => {
      const dir = fs.mkdtempSync(path.join(os.tmpdir(), 'richbrief-normalize-'));
      subtest.after(() => fs.rmSync(dir, { recursive: true, force: true }));
      const audio = path.join(dir, 'audio-part-00.wav');
      const rate = 48_000;
      const pcm = Buffer.alloc(rate * 4);
      for (let offset = 0; offset < pcm.length; offset += 4) {
        pcm.writeInt16LE(Math.round(2_000 * Math.sin(offset / 70)), offset);
        pcm.writeInt16LE(Math.round(7_000 * Math.cos(offset / 50)), offset + 2);
      }
      const record = { status: 'open', capture: { source: 'desktop-companion-macos' } };
      const recordPath = path.join(dir, 'session.json');
      fs.writeFileSync(recordPath, JSON.stringify(record));
      fs.writeFileSync(audio, Buffer.concat([nativeHeader(rate, pcm.length / 2), pcm]));

      let running = true;
      const { promise: firstRead, resolve: notifyRead } = Promise.withResolvers();
      const preparing = prepareLiveNormalization(dir, () => {
        notifyRead();
        return running;
      }, new AbortController().signal);
      await Promise.race([firstRead, preparing]);
      assert.equal(takeLiveNormalization(dir), null, 'Unfinished output is unavailable');
      const fd = fs.openSync(audio, 'r+');
      fs.writeSync(fd, nativeHeader(rate, pcm.length), 0, 44, 0);
      fs.closeSync(fd);
      record.status = 'closed';
      fs.writeFileSync(recordPath, JSON.stringify(record));
      running = false;
      await preparing;

      if (changed) {
        const changedAudio = fs.readFileSync(audio);
        changedAudio.writeInt16LE(24_000, 44);
        fs.writeFileSync(audio, changedAudio);
        assert.equal(takeLiveNormalization(dir), null, 'Old tracks cannot describe changed audio');
      } else {
        const expected = normalizeSession(dir, { workDir: path.join(dir, 'reference') });
        const actual = takeLiveNormalization(dir);
        assert.ok(actual, 'Both converted tracks are ready after recorder close');
        assert.deepEqual(fs.readFileSync(actual.me), fs.readFileSync(expected.me));
        assert.deepEqual(fs.readFileSync(actual.others), fs.readFileSync(expected.others));
      }
      assert.equal(takeLiveNormalization(dir), null, 'The one-use preparation was cleared');
    });
  }
});

test('live transcription checkpoints the flushed prefix and only decodes the tail after close', (t) => {
  const dir = fs.mkdtempSync(path.join(os.tmpdir(), 'richbrief-live-'));
  t.after(() => fs.rmSync(dir, { recursive: true, force: true }));
  const audio = path.join(dir, 'audio-part-00.wav');
  const rate = 8_000;
  const bytesPerSecond = rate * 4;
  const header = nativeHeader(rate);
  const pcm = Buffer.alloc(170 * bytesPerSecond);
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
    if (requests.at(-1) === 24) {
      return {
        me: [segment('First', 23_000, 23_900)],
        others: [segment('Other channel', 500, 1_000)],
        toolchain,
      };
    }
    assert.equal(wav.readInt16LE(44), 17);
    assert.equal(wav.readInt16LE(46), -31);
    return { me: [segment('Next', 100, 1_000)], others: [], toolchain };
  };
  const options = { model: 'large-v3-turbo-q5_0' };
  const advance = (state) => advanceLiveTranscription(dir, options, { decode, toolchain, ...state });

  flush(26);
  assert.equal(supportsLiveTranscription(dir, { capture: { source: 'desktop-companion-macos' } }), true);
  assert.equal(supportsLiveTranscription(dir, { capture: { source: 'chrome' } }), false);
  assert.equal(advance().decoded, 0, 'Unflushed bytes cannot establish a quiet boundary');
  flush(27);
  const live = advance();
  assert.equal(live.decoded, 1);
  assert.equal(live.asr.me[0].text, 'First');
  assert.equal(live.asr.others[0].text, 'Other channel');
  assert.equal(advance({ cursor: live.cursor }).decoded, 0);

  flush(30);
  const finished = advance({ closed: true });
  assert.equal(finished.reused, 1);
  assert.equal(finished.decoded, 1);
  assert.deepEqual(finished.asr.me, [segment('First', 23_000, 23_900), segment('Next', 24_100, 25_000)]);
  assert.deepEqual(requests, [24, 6]);

  const checkpoint = path.join(dir, '.live-transcription', '0.json');
  fs.writeFileSync(checkpoint, 'truncated');
  const recovered = advance({ closed: true });
  assert.equal(recovered.reused, 1);
  assert.equal(recovered.decoded, 1);
  assert.deepEqual(recovered.asr.me, finished.asr.me);

  const saved = JSON.parse(fs.readFileSync(checkpoint, 'utf8'));
  for (const me of [[null], [{ ...saved.me[0], startMs: 4_000_000 }], [{ ...saved.me[0], wordTimesMs: [null] }]]) {
    fs.writeFileSync(checkpoint, JSON.stringify({ ...saved, me }));
    const repaired = advance({ closed: true });
    assert.equal(repaired.reused, 1);
    assert.equal(repaired.decoded, 1);
    assert.deepEqual(repaired.asr.me, finished.asr.me);
  }

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

  // Both sides are active across the nominal cut. Wait for the next pause and
  // send the sentence to Whisper in one window instead of trimming overlapping decodes.
  pcm.fill(0);
  pcm.fill(Buffer.from([0, 8]), 21 * bytesPerSecond, 32 * bytesPerSecond);
  flush(30);
  assert.equal(advance({ decode: () => assert.fail('continuous speech was cut') }).decoded, 0);
  flush(40);
  const continuous = advance({
    decode: (_cache, wav) => {
      assert.equal(wav.readUInt32LE(40) / bytesPerSecond, 32.08);
      return { me: [segment('Read the whole sentence once.', 23_000, 28_000)], others: [], toolchain };
    },
  });
  assert.deepEqual(continuous.asr.me.map((s) => s.text), ['Read the whole sentence once.']);
  assert.equal(advance({
    cursor: continuous.cursor, closed: true,
    decode: (_cache, wav) => {
      assert.equal(wav.readUInt32LE(40) / bytesPerSecond, 7.92);
      return { me: [], others: [], toolchain };
    },
  }).decoded, 1);

  pcm.fill(Buffer.from([0, 8]));
  flush(121);
  assert.equal(advance({
    recorderIsRunning: () => false, decode: () => assert.fail('the recorder had already exited'),
  }).decoded, 0);
  assert.throws(() => advance({
    closed: true, decode: () => assert.fail('unbounded audio was decoded as a chunk'),
  }), /no quiet meeting boundary/);

  const waiting = advance({ decode: () => assert.fail('continuous speech was cut during capture') });
  assert.equal(waiting.cursor, 0);
  assert.equal(waiting.scanFrom, 120 * rate);
  pcm.fill(0, 122 * bytesPerSecond);
  pcm.fill(Buffer.from([0, 0, 0, 8]), 130 * bytesPerSecond, 131 * bytesPerSecond);
  flush(154);
  const resumed = advance({
    scanFrom: waiting.scanFrom,
    decode: (_cache, wav) => {
      assert.equal(wav.readUInt32LE(40) / bytesPerSecond, 24);
      return { me: [], others: [segment('Later fact.', 7_920, 8_920)], toolchain };
    },
  });
  assert.equal(resumed.longWindows, 1);
  assert.equal(resumed.decoded, 1);
  assert.equal(resumed.cursor, 146.08 * rate);
  const caughtUp = advance({
    closed: true,
    recover: (_cache, from, to) => {
      assert.deepEqual([from, to], [0, 122_080]);
      return { me: [segment('Unbroken statement.', 100, 122_000)], others: [], toolchain };
    },
    decode: (_cache, wav) => {
      assert.equal(wav.readUInt32LE(40) / bytesPerSecond, 7.92);
      return { me: [], others: [], toolchain };
    },
  });
  assert.equal(caughtUp.longWindows, 1);
  assert.equal(caughtUp.reused, 1);
  assert.deepEqual(caughtUp.asr.me, [segment('Unbroken statement.', 100, 122_000)]);
  assert.deepEqual(caughtUp.asr.others, [segment('Later fact.', 130_000, 131_000)]);

  // A handoff may arrive before the worker checkpoints the sentence just spoken.
  pcm.fill(0);
  pcm.fill(Buffer.from([0, 0, 0, 8]), 16 * bytesPerSecond, 17 * bytesPerSecond);
  pcm.fill(Buffer.from([0, 8, 0, 0]), 68 * bytesPerSecond, 69 * bytesPerSecond);
  pcm.fill(Buffer.from([0, 8, 0, 0]), 72 * bytesPerSecond, 73 * bytesPerSecond);
  flush(75);
  const record = path.join(dir, 'session.json');
  const capture = { source: 'desktop-companion-macos' };
  const startedAt = 1_000_000;
  const requestedAt = startedAt + 70_000;
  fs.writeFileSync(record, JSON.stringify({ status: 'open', capture, startedAt }));
  let excerptScratch;
  const fresh = recentLiveExcerpt(dir, options, requestedAt, (scratch, wav) => {
    excerptScratch = scratch;
    assert.equal(wav.readUInt32LE(40) / bytesPerSecond, 60);
    return {
      me: [{ ...segment('The new decision.', 58_000, 59_000), speaker: 'me' }],
      others: [{ ...segment('An earlier fact.', 6_000, 7_000), speaker: 'others' }],
    };
  }, toolchain);
  assert.deepEqual(fresh, {
    fromMs: 10_000, throughMs: 70_000,
    passages: [
      { speaker: 'Them', atMs: 16_000, text: 'An earlier fact.' },
      { speaker: 'Me', atMs: 68_000, text: 'The new decision.' },
    ],
  });
  assert(!fs.existsSync(excerptScratch), 'a completed excerpt keeps no audio copy');
  let windows = 0;
  advance({ decode: () => ({
    me: [],
    others: windows++ === 0 ? [segment('An earlier fact.', 16_000, 17_000)] : [],
    toolchain,
  }) });
  const reused = recentLiveExcerpt(dir, options, requestedAt, (_scratch, wav) => {
    assert.equal(wav.readUInt32LE(40) / bytesPerSecond, 22);
    return { me: [segment('The new decision.', 20_000, 21_000)], others: [] };
  }, toolchain);
  assert.deepEqual(reused, fresh, 'cached text and a newly decoded tail respect the same cutoff');
  const crossing = JSON.parse(fs.readFileSync(checkpoint, 'utf8'));
  crossing.others = [segment('Words that continue after the cutoff.', 8_000, 12_000)];
  fs.writeFileSync(checkpoint, JSON.stringify(crossing));
  const clipped = recentLiveExcerpt(dir, options, requestedAt, (_scratch, wav) => {
    assert.equal(wav.readUInt32LE(40) / bytesPerSecond, 60);
    return { me: [], others: [segment('After the cutoff.', 100, 300)] };
  }, toolchain);
  assert.equal(clipped.passages[0].text, 'After the cutoff.', 'a cut through a sentence decodes the leading continuation');
  assert.equal(recentLiveExcerpt(dir, options, startedAt - 1, () => assert.fail('future recording decoded')), null);
  fs.writeFileSync(record, JSON.stringify({ status: 'closed', capture, startedAt }));
  assert.equal(recentLiveExcerpt(dir, options, requestedAt, () => assert.fail('closed recording decoded')), null);

  pcm.fill(0);
  flush(24);
  const silence = advanceLiveTranscription(dir, options, { closed: true, toolchain });
  assert.equal(silence.decoded, 1);
  assert.equal(silence.skippedSilentChannels, 2);
  assert.deepEqual([silence.asr.me, silence.asr.others], [[], []]);
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

test('finalization clears abandoned live decoder copies and keeps checkpoints', (t) => {
  const dir = fs.mkdtempSync(path.join(os.tmpdir(), 'richbrief-drain-'));
  t.after(() => fs.rmSync(dir, { recursive: true, force: true }));
  const cache = path.join(dir, '.live-transcription');
  for (const name of ['decode-A1b2c3', '.richos-output-d4E5f6', 'excerpt-Abcd12', 'normalized-audio']) {
    const staging = path.join(cache, name);
    fs.mkdirSync(staging, { recursive: true });
    fs.writeFileSync(path.join(staging, 'audio-part-00.wav'), 'audio');
  }
  fs.writeFileSync(path.join(cache, '0.json'), '{}');
  removeLiveTranscriptionScratch(dir, { keepPrepared: true });
  assert.deepEqual(fs.readdirSync(cache).sort(), ['0.json', 'excerpt-Abcd12', 'normalized-audio']);
  removeLiveTranscriptionScratch(dir);
  assert.deepEqual(fs.readdirSync(cache).sort(), ['0.json', 'excerpt-Abcd12']);
});

test('the final pipeline clears prepared tracks when it skips conversion', async (t) => {
  for (const ready of [false, true]) {
    await t.test(ready ? 'already ready' : 'no usable audio', (subtest) => {
      const dir = fs.mkdtempSync(path.join(os.tmpdir(), 'richbrief-unused-normalize-'));
      subtest.after(() => fs.rmSync(dir, { recursive: true, force: true }));
      const prepared = path.join(dir, '.live-transcription', 'normalized-audio');
      fs.mkdirSync(prepared, { recursive: true });
      fs.writeFileSync(path.join(prepared, 'source.json'), '{}');
      const status = ready ? 'ready' : 'anomaly';
      fs.writeFileSync(path.join(dir, 'session.json'), JSON.stringify({
        status: 'closed', capture: { source: 'desktop-companion-macos' },
        audio: ready ? { parts: [{ bytes: 48 }], bytesTotal: 48 } : {}, pipeline: { status },
      }));
      if (ready) {
        fs.writeFileSync(path.join(dir, 'audio-part-00.wav'), Buffer.concat([nativeHeader(48_000, 4), Buffer.alloc(4)]));
        fs.writeFileSync(path.join(dir, 'transcript.md'), 'Existing transcript.');
      }
      assert.equal(runPipeline(dir).status, status);
      assert.equal(fs.existsSync(prepared), false);
    });
  }
});

test('live normalization reuse logs the interval from the app recording end', async (t) => {
  const dir = fs.mkdtempSync(path.join(os.tmpdir(), 'richbrief-normalize-timing-'));
  t.after(() => fs.rmSync(dir, { recursive: true, force: true }));
  const rate = 48_000;
  const pcm = Buffer.alloc(rate * 4);
  const wav = Buffer.concat([nativeHeader(rate, pcm.length), pcm]);
  fs.writeFileSync(path.join(dir, 'audio-part-00.wav'), wav);
  fs.writeFileSync(path.join(dir, 'session.json'), JSON.stringify({
    status: 'closed', capture: { source: 'desktop-companion-macos' },
    audio: { parts: [{ bytes: wav.length }], bytesTotal: wav.length },
  }));
  await prepareLiveNormalization(dir, () => false, new AbortController().signal);
  const receipt = path.join(dir, '.live-transcription', 'normalized-audio', 'source.json');
  const { readyAt } = JSON.parse(fs.readFileSync(receipt, 'utf8'));
  fs.writeFileSync(path.join(dir, 'meeting.json'), JSON.stringify({ endedAt: readyAt - 25 }));
  const info = t.mock.method(log, 'info', () => {});
  assert.equal(runPipeline(dir).status, 'anomaly', 'Silence never reaches the speech decoder');
  const reuse = info.mock.calls.find(({ arguments: args }) => args[0] === 'live audio normalization reused');
  assert.deepEqual(reuse?.arguments[1], { readyAt, afterRecordedEndMs: 25 });
});

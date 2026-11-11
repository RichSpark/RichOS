#!/usr/bin/env node
import { GGML, fakeModel, sparseModel, CAPTIVE_PORTAL } from '../../../engine/voice/tests/support/models.mjs';
import { ingestLedgerPath as auditLedgerPath } from '../lib/config.js';
/**
 * RichOS local service — pure-logic test harness (no ffmpeg, no whisper, no browser).
 *
 *   node test/run.js
 *
 * Everything that decides correctness or a reliability alarm lives in pure modules so it can be
 * tested here deterministically with a fake clock and temp dirs. Test names document the invariant.
 * The heavy end-to-end (real audio -> real transcript) lives in test/e2e.mjs.
 */

import assert from 'node:assert/strict';
import fs from 'node:fs';
import os from 'node:os';
import path from 'node:path';
import { execFileSync, spawnSync } from 'node:child_process';

import { upgradeRecord, CONTRACT_SCHEMA_VERSION, PIPELINE_STATUS, toAbsolute } from '../lib/contract.js';
import { reconcilePipeline, analyzeSession } from '../lib/reconcile.js';
import { mergeTranscript, captionSpeakerFor, renderMarkdown, verify, stamp, wordCount } from '../lib/merge.js';
import { correct, correctText, similarity, levenshtein, normalizeTerm } from '../lib/correct.js';
import { normalizeEntities, loadEntityMemory } from '../lib/entities.js';
import {
  learnTerm,
  learnFromEdits,
  extractTermCorrections,
  tokenReplaceHunks,
  looksLikeTerm,
  bumpVersion,
  serializeEntitiesDoc,
} from '../lib/capture.js';
import {
  CAPTURE_KIND,
  preferenceRank,
  scopesOverlap,
  decideClaim,
  findPromotable,
  buildPromotionOwnership,
  dedupeOverlapping,
  sessionDescriptor,
} from '../lib/coordination.js';
import { encodeMessage, FrameDecoder } from '../lib/stdio.js';
import { SessionSink } from '../lib/host-handlers.js';
import { appendLedger, alreadyLedgered } from '../lib/ledger.js';
import {
  phoneticKey,
  phoneticSimilarity,
  askCandidates,
  askKey,
  applyLedger,
  answerAsk,
  matchHeard,
  reviewSent,
  ASK_MIN_PHONETIC,
  MATCH_WINDOW_MS,
} from '../lib/dictation.js';
import { scanZone, sweepDictationRetention } from '../lib/watcher.js';
import { runPipeline } from '../lib/pipeline.js';
import { withPrivateOutputs, writePrivateFile } from '../lib/private-files.js';
import {
  parseJournalFile,
  loadJournal,
  loadLedger,
  saveLedger,
  markReconciled,
  withConsumed,
  planRetention,
  surveyJournal,
  sweepRetention,
  costPerHour,
} from '../lib/dictation-store.js';
import { parseChannels, parseVolume, parseSilenceLog, SILENCE_MAX_DB } from '../lib/normalize.js';
import { parseWhisperJson } from '../lib/transcribe.js';
import {
  resolveTier,
  resolveTierForHost,
  MODEL_TIERS,
  DEFAULT_TIER,
  DEFAULT_MODEL,
  whisperArgs,
  restyleArgs,
  MAX_CONTEXT_TOKENS,
  resolveModel,
  resolveModelChecked,
  modelSearchDirs,
} from '../lib/config.js';
import { loadCosts, availableMemoryBytes, cacheKey } from '../lib/hardware.js';
import { PIN_FILE, TOOLCHAIN_REFERENCE, pinFor, human } from '../../../engine/voice/provisioning/model-catalog.js';
import {
  TOOLCHAIN_FINDING,
  SEVERITY,
  strictMode,
  severityOf,
  parseVersion,
  parseBackends,
  compareToolchain,
  compareModel,
  verdictOf,
  describeFinding,
  sameIdentity,
  provenanceString,
  probeWhisper,
  fileIdentity,
  buildLock,
  resolveToolchain,
  assertToolchain,
  resetToolchainCache,
} from '../lib/toolchain.js';
import {
  guardChannel,
  guardChannelAll,
  guardInsertions,
  guardOverlapStutter,
  guardTranscription,
  guardWarnings,
  readEnumerationMarker,
  burstCapacity,
  burstOverlapMs,
  guardSilenceFabrication,
  numeralInText,
  adjudicateInsertionMarker,
  selectInsertionProbes,
  REPETITION_GUARD_DEFAULTS,
} from '../lib/repetition-guard.js';
import {
  TURBO_NUMERAL_INSERTION,
  Q5_SAME_AUDIO_CLEAN,
  GENUINE_SPOKEN_ENUMERATION,
  LARGE_V3_SLIDING_STUTTER,
  CLEAN_NO_BOUNDARY_OVERLAP,
  TURBO_NUMERAL_INSERTION_ISOLATED_DECODES,
} from './fixtures/captured-hallucinations.js';
import { diarizeOthers, readTurn, SPEAKER_TURN_MARKER } from '../lib/diarize.js';
import {
  findDeletionCandidates,
  adjudicateCandidate,
  guardDeletions,
  deletionWarnings,
  lexicalText,
  isSilenceFillerText,
  informativeWords,
  echoLength,
  echoRatio,
  nearbyTranscriptText,
} from '../lib/deletion-guard.js';
import {
  findSparseWindows,
  adjudicateSparseWindow,
  guardSubstitution,
  substitutionWarnings,
  tileWindows,
  DEFAULT_SPARSITY_OPTS,
} from '../lib/substitution-guard.js';

let passed = 0;
const failures = [];
function test(name, fn) {
  try {
    fn();
    passed += 1;
    console.log(`  ok  ${name}`);
  } catch (err) {
    failures.push({ name, err });
    console.log(`FAIL  ${name}\n      ${err.message}`);
  }
}
function group(t) {
  console.log(`\n${t}`);
}

/**
 * An asynchronous test. Queued rather than run where it is written, so the synchronous body of
 * this file still reads top to bottom and the whole queue drains before the summary is printed.
 *
 * It exists because the model-fetch failure paths are DRIVEN against a real HTTP server on a real
 * socket writing real files, and a stubbed transport would only ever prove the stub.
 */
const asyncTests = [];
function testAsync(name, fn) {
  asyncTests.push({ name, fn });
}
async function drainAsyncTests() {
  for (const { name, fn } of asyncTests) {
    try {
      // Serial on purpose: these bind sockets and write temp dirs, and a flaky suite is worse
      // than a slow one.
      // eslint-disable-next-line no-await-in-loop
      await fn();
      passed += 1;
      console.log(`  ok  ${name}`);
    } catch (err) {
      failures.push({ name, err });
      console.log(`FAIL  ${name}\n      ${err.message}`);
    }
  }
}
function tmp() {
  return fs.mkdtempSync(path.join(os.tmpdir(), 'richos-svc-'));
}

const T0 = 1_700_000_000_000;


group('native host installation');
test('reinstall refreshes moved host and Node paths, including shell metacharacters', () => {
  if (process.platform === 'win32') return; // The POSIX installer is macOS/Linux only.
  const root = tmp();
  try {
    const home = path.join(root, 'home');
    const browser = path.join(home, process.platform === 'darwin'
      ? 'Library/Application Support/Google/Chrome' : '.config/google-chrome');
    fs.mkdirSync(browser, { recursive: true });
    let host = path.join(root, 'old-host');
    fs.cpSync(path.resolve(import.meta.dirname, '../host'), host, { recursive: true });
    fs.writeFileSync(path.join(host, 'native-host.js'), 'console.log("host-ready");\n');
    const install = (dir, env) => execFileSync('sh', [path.join(dir, 'install-host.sh'), 'abcdefghijklmnopabcdefghijklmnop'],
      { env, encoding: 'utf8', timeout: 10000 });
    install(host, { ...process.env, HOME: home });
    const moved = path.join(root, "new host '& # $literal");
    fs.renameSync(host, moved);
    host = moved;
    const nodeDir = path.join(root, "new node '& # $literal");
    fs.mkdirSync(nodeDir);
    const quote = (value) => "'" + value.replaceAll("'", "'\\''") + "'";
    fs.writeFileSync(path.join(nodeDir, 'node'), `#!/bin/sh\nexport RICHOS_TEST_NEW_NODE=yes\nexec ${quote(process.execPath)} "$@"\n`, { mode: 0o755 });
    fs.writeFileSync(path.join(host, 'native-host.js'), 'console.log("host-ready:" + process.env.RICHOS_TEST_NEW_NODE);\n');
    const env = { ...process.env, HOME: home, PATH: nodeDir + path.delimiter + process.env.PATH };
    install(host, env);
    install(host, env); // Repeated installation remains valid.
    const manifest = JSON.parse(fs.readFileSync(path.join(browser, 'NativeMessagingHosts/com.richos.host.json')));
    assert.equal(manifest.path, path.join(host, 'richos-host-launcher.sh'));
    assert.equal(execFileSync(manifest.path, [], { env: { ...process.env, HOME: home }, encoding: 'utf8' }).trim(), 'host-ready:yes');
  } finally {
    fs.rmSync(root, { recursive: true, force: true });
  }
});

for (const tail of [Buffer.from('{"torn":'), Buffer.from([0xff, 0xe2, 0x82]), Buffer.from('{"complete":true}')]) {
  test(`appendLedger preserves the next row after a tail without a newline (${tail.toString('hex')})`, () => {
    const zone = tmp();
    try {
      const file = auditLedgerPath(zone);
      const before = Buffer.concat([Buffer.from('{"original":true}\n'), tail]);
      fs.writeFileSync(file, before);
      assert.equal(appendLedger({ sessionId: 'accepted', runIndex: 0 }, zone).appended, true);
      assert.equal(alreadyLedgered('accepted', 0, zone), true);
      assert.equal(appendLedger({ sessionId: 'accepted', runIndex: 0 }, zone).appended, false);
      assert.deepEqual(fs.readFileSync(file).subarray(0, before.length), before);
      assert.equal(appendLedger({ sessionId: 'later', runIndex: 0 }, zone).appended, true);
      assert.equal(alreadyLedgered('later', 0, zone), true);
      assert.equal(alreadyLedgered('accepted', 0, zone), true);
    } finally { fs.rmSync(zone, { recursive: true, force: true }); }
  });
}
test('appendLedger refuses a linked ledger without changing its target', () => {
  const zone = tmp();
  try {
    const target = path.join(zone, 'original');
    fs.writeFileSync(target, 'unchanged');
    fs.symlinkSync(target, auditLedgerPath(zone));
    assert.throws(() => appendLedger({ sessionId: 'accepted', runIndex: 0 }, zone), /storage boundary/);
    assert.equal(fs.readFileSync(target, 'utf8'), 'unchanged');
  } finally { fs.rmSync(zone, { recursive: true, force: true }); }
});

group('pipeline storage boundary');
for (const link of ['symlink', 'hardlink']) {
  for (const name of ['session.json', 'transcript.md', 'verification.json', 'me.wav', 'others.json', '_concat.txt', '_probe/clip.wav']) {
    test(`pipeline refuses ${link} output ${name} before modifying any artifact`, () => {
      const root = tmp();
      try {
        const session = path.join(root, 'session');
        fs.mkdirSync(session);
        const record = JSON.stringify({ sessionId: 'call', status: 'closed', audio: { parts: [], bytesTotal: 0 } });
        fs.writeFileSync(path.join(session, 'session.json'), record);
        const target = path.join(root, 'outside.json');
        fs.writeFileSync(target, record);
        const output = path.join(session, name);
        fs.mkdirSync(path.dirname(output), { recursive: true });
        fs.rmSync(output, { force: true });
        if (link === 'symlink') fs.symlinkSync(target, output);
        else fs.linkSync(target, output);
        assert.throws(() => runPipeline(session, { model: 'small.en', zone: root }), /storage boundary/);
        assert.equal(fs.readFileSync(target, 'utf8'), record);
        assert.equal(fs.readFileSync(path.join(session, 'session.json'), 'utf8'), record);
      } finally { fs.rmSync(root, { recursive: true, force: true }); }
    });
  }
}
test('native output staging refuses a leaf replaced during the native operation', () => {
  const root = tmp();
  try {
    const outside = path.join(root, 'outside');
    const output = path.join(root, 'result.wav');
    fs.writeFileSync(outside, 'original');
    assert.throws(() => withPrivateOutputs([output], ([stage]) => {
      assert.notEqual(stage, output);
      fs.writeFileSync(stage, 'native output');
      fs.symlinkSync(outside, output);
    }), /storage boundary/);
    assert.equal(fs.readFileSync(outside, 'utf8'), 'original');
    fs.unlinkSync(output);
    withPrivateOutputs([output], ([stage]) => fs.writeFileSync(stage, 'normal output'));
    assert.equal(fs.readFileSync(output, 'utf8'), 'normal output');
    assert.deepEqual(fs.readdirSync(root).sort(), ['outside', 'result.wav']);
  } finally { fs.rmSync(root, { recursive: true, force: true }); }
});
test('private writes refuse linked files and retain ordinary overwrite behavior', () => {
  const root = tmp();
  try {
    const file = path.join(root, 'output');
    const linked = path.join(root, 'linked');
    writePrivateFile(file, 'old');
    fs.linkSync(file, linked);
    assert.throws(() => writePrivateFile(file, 'new'), /storage boundary/);
    assert.equal(fs.readFileSync(linked, 'utf8'), 'old');
    fs.unlinkSync(linked);
    writePrivateFile(file, 'new');
    assert.equal(fs.readFileSync(file, 'utf8'), 'new');
  } finally { fs.rmSync(root, { recursive: true, force: true }); }
});
for (const command of ['run', 'retranscribe']) {
  test(`${command} rejects explicit product sessions and aliases before writing`, () => {
    const root = tmp();
    try {
      const product = path.join(root, 'product');
      const tools = path.join(product, 'richos/tools');
      const source = path.resolve(import.meta.dirname, '../..');
      for (const [component, entries] of [
        ['richos-service', ['lib', 'bin', 'package.json']],
        ['richos-extension', ['sync', 'core', 'package.json']],
      ]) {
        for (const entry of entries) {
          fs.cpSync(path.join(source, component, entry), path.join(tools, component, entry), { recursive: true });
        }
      }
      fs.cpSync(path.resolve(source, '../engine/voice'), path.join(product, 'richos/engine/voice'), { recursive: true });
      const session = path.join(product, 'docs/session');
      const external = path.join(root, 'external');
      const record = JSON.stringify({ schemaVersion: 2, sessionId: 'boundary-test', status: 'closed',
        startedAt: 1, endedAt: 2, audio: { bytes: 0, parts: 0 } });
      for (const dir of [session, external]) {
        fs.mkdirSync(dir, { recursive: true });
        fs.writeFileSync(path.join(dir, 'session.json'), record);
      }
      const alias = path.join(root, 'alias');
      fs.symlinkSync(session, alias, 'junction');
      const cli = path.join(tools, 'richos-service/bin/richos-service.js');
      const env = { ...process.env, HOME: path.join(root, 'home'), LORO_CORPUS: path.join(root, 'corpus') };
      const alternateCase = path.join(root, 'PRODUCT/docs/session');
      const targets = [session, alias];
      if (fs.existsSync(alternateCase)) targets.push(alternateCase);
      for (const target of targets) {
        const result = spawnSync(process.execPath, [cli, command, target, '--zone', external, '--model', 'small.en'],
          { env, encoding: 'utf8', timeout: 10000 });
        assert.match(result.stderr, /privacy invariant/);
        assert.notEqual(result.status, 0);
        assert.equal(fs.readFileSync(path.join(session, 'session.json'), 'utf8'), record);
        assert.deepEqual(fs.readdirSync(session), ['session.json']);
      }
      const allowed = spawnSync(process.execPath, [cli, command, external, '--zone', external, '--model', 'small.en'],
        { env, encoding: 'utf8', timeout: 10000 });
      assert.doesNotMatch(allowed.stderr, /privacy invariant/);
      assert.equal(JSON.parse(fs.readFileSync(path.join(external, 'session.json'))).pipeline.status, 'anomaly');
    } finally {
      fs.rmSync(root, { recursive: true, force: true });
    }
  });
}


// ---------------------------------------------------------------------------------------
group('capture->pipeline contract (schemaVersion 2) — surface-independence, no field lost');

test('upgradeRecord lifts a v1 extension record to v2 without discarding any v1 field', () => {
  const v1 = {
    schemaVersion: 1,
    sessionId: 's1',
    status: 'closed',
    platform: { id: 'meet' },
    capture: { chunkMs: 3000, micEnabled: true },
    audio: { parts: [{ part: 0, bytes: 10 }], bytesTotal: 10 },
    captions: { count: 5 },
  };
  const v2 = upgradeRecord(v1);
  assert.equal(v2.schemaVersion, CONTRACT_SCHEMA_VERSION);
  assert.equal(v2.capture.chunkMs, 3000, 'existing capture fields survive');
  assert.equal(v2.capture.source, 'chrome-extension');
  assert.equal(v2.capture.channels.left, 'microphone (me)');
  assert.equal(v2.ownership.ownerSurface, 'chrome-extension');
  assert.equal(v2.pipeline.status, 'pending');
  assert.deepEqual(v2.pipeline.modelRuns, []);
});

test('upgradeRecord is idempotent and never overwrites an existing pipeline block', () => {
  const rec = upgradeRecord({ sessionId: 's', pipeline: { status: 'ready', modelRuns: [{ model: 'x' }] } });
  const again = upgradeRecord(rec);
  assert.equal(again.pipeline.status, 'ready');
  assert.equal(again.pipeline.modelRuns.length, 1);
});

test('toAbsolute anchors a relative offset onto session start (no timestamp, no merge)', () => {
  assert.equal(toAbsolute(T0, 5000), T0 + 5000);
  assert.equal(toAbsolute(0, 0), 0);
});

// ---------------------------------------------------------------------------------------
group('reconcile — the never-silent guard extends the extension logic verbatim');

function recordWith({ audioBytes = 0, parts = 0, captionCount = 0, status = 'closed', endedAt = T0 }) {
  return {
    status,
    endedAt,
    audio: { parts: Array.from({ length: parts }, (_, i) => ({ part: i, bytes: audioBytes })), bytesTotal: audioBytes },
    captions: { count: captionCount },
    health: { redSeconds: 0 },
  };
}

test('reconcilePipeline reuses analyzeSession — a captions-only session is still a flagged anomaly', () => {
  const r = reconcilePipeline({ record: recordWith({ captionCount: 20 }), audioBytesOnDisk: 0, hasTranscript: false, now: T0 });
  assert.equal(r.captionsOnly, true);
  assert.equal(r.complete, false);
  assert.ok(r.problems.some((p) => /NO audio/.test(p)));
  // parity: analyzeSession alone agrees
  assert.equal(analyzeSession({ record: recordWith({ captionCount: 20 }), audioBytesOnDisk: 0 }).captionsOnly, true);
});

test('a CLOSED session with audio but no transcript, past the SLA, is a LOUD transcript-overdue anomaly', () => {
  const rec = recordWith({ audioBytes: 2_000_000, parts: 1, endedAt: T0 });
  const r = reconcilePipeline({
    record: rec,
    audioBytesOnDisk: 2_000_000,
    hasTranscript: false,
    now: T0 + 20 * 60 * 1000, // 20 min later, SLA is 10
    slaMs: 10 * 60 * 1000,
  });
  assert.equal(r.transcriptOverdue, true);
  assert.equal(r.complete, false);
  assert.ok(r.problems.some((p) => /NO transcript\.md/.test(p)));
});

test('the same session WITH a transcript is complete and not overdue', () => {
  const rec = recordWith({ audioBytes: 2_000_000, parts: 1, endedAt: T0 });
  const r = reconcilePipeline({ record: rec, audioBytesOnDisk: 2_000_000, hasTranscript: true, now: T0 + 3600_000 });
  assert.equal(r.transcriptOverdue, false);
  assert.equal(r.complete, true, r.problems.join('; '));
});

test('within the SLA, a not-yet-transcribed closed session is NOT yet an overdue anomaly', () => {
  const rec = recordWith({ audioBytes: 2_000_000, parts: 1, endedAt: T0 });
  const r = reconcilePipeline({ record: rec, audioBytesOnDisk: 2_000_000, hasTranscript: false, now: T0 + 60_000, slaMs: 600_000 });
  assert.equal(r.transcriptOverdue, false);
});

// ---------------------------------------------------------------------------------------
group('merge — interleave by timestamp + fold in caption speaker labels');

const meSegs = [
  { startMs: 0, endMs: 2000, text: 'hi there', speaker: 'me' },
  { startMs: 6000, endMs: 8000, text: 'sounds good', speaker: 'me' },
];
const othersSegs = [{ startMs: 2500, endMs: 5500, text: 'hello, thanks for joining', speaker: 'others' }];
const captions = [{ speaker: 'Marcus Whitfield', text: 'hello, thanks for joining', firstT: T0 + 2400, t: T0 + 5600, revision: 3 }];

test('segments from both channels interleave into one time-ordered transcript', () => {
  const merged = mergeTranscript({ me: meSegs, others: othersSegs, captions: [], startedAt: T0 });
  assert.deepEqual(merged.segments.map((s) => s.startMs), [0, 2500, 6000]);
  assert.deepEqual(merged.segments.map((s) => s.speaker), ['me', 'others', 'me']);
});

test('a far-side segment is attributed to the caption speaker it overlaps in time', () => {
  const name = captionSpeakerFor(othersSegs[0], T0, captions);
  assert.equal(name, 'Marcus Whitfield');
  const merged = mergeTranscript({ me: meSegs, others: othersSegs, captions, startedAt: T0 });
  const other = merged.segments.find((s) => s.speaker === 'others');
  assert.equal(other.label, 'Marcus Whitfield', 'audio gives you-vs-them; captions add WHICH of them');
  assert.deepEqual(merged.segments.filter((s) => s.speaker === 'me').map((s) => s.label), ['Me', 'Me']);
});

test('with no overlapping caption, a far-side segment falls back to the generic "Them" label', () => {
  const merged = mergeTranscript({ me: meSegs, others: othersSegs, captions: [], startedAt: T0 });
  assert.equal(merged.segments.find((s) => s.speaker === 'others').label, 'Them');
});

test('an "unknown" caption speaker never wins attribution (fails soft to Them)', () => {
  const anon = [{ speaker: 'unknown', firstT: T0 + 2400, t: T0 + 5600 }];
  assert.equal(captionSpeakerFor(othersSegs[0], T0, anon), null);
});

test('renderMarkdown produces speaker-attributed, timestamped lines with a provenance header', () => {
  const rec = upgradeRecord({
    sessionId: 's', startedAt: T0, endedAt: T0 + 600000,
    platform: { label: 'Google Meet' }, captions: { count: 1 },
  });
  rec.pipeline.model = 'large-v3-turbo';
  const merged = mergeTranscript({ me: meSegs, others: othersSegs, captions, startedAt: T0 });
  const md = renderMarkdown(merged, rec);
  assert.match(md, /# Transcript — Google Meet/);
  assert.match(md, /\*\*\[00:00\] Me:\*\* hi there/);
  assert.match(md, /\*\*\[00:02\] Marcus Whitfield:\*\* hello, thanks for joining/);
  assert.match(md, /Model:.*large-v3-turbo/);
});

test('stamp formats mm:ss and h:mm:ss; wordCount handles blanks', () => {
  assert.equal(stamp(0), '00:00');
  assert.equal(stamp(65000), '01:05');
  assert.equal(stamp(3_661_000), '1:01:01');
  assert.equal(wordCount('  '), 0);
  assert.equal(wordCount('one two three'), 3);
});

// ---------------------------------------------------------------------------------------
group('verification — coverage, caption<->ASR agreement, dead intervals as measured numbers');

test('verify computes coverage, per-channel words, and caption agreement', () => {
  const rec = { startedAt: T0, endedAt: T0 + 8000, pipeline: { model: 'large-v3-turbo' } };
  const merged = mergeTranscript({ me: meSegs, others: othersSegs, captions, startedAt: T0 });
  const v = verify(merged, { me: meSegs, others: othersSegs }, captions, rec);
  assert.equal(v.channels.meSegments, 2);
  assert.equal(v.channels.othersSegments, 1);
  assert.ok(v.channels.totalWords > 0);
  assert.equal(v.captions.count, 1);
  assert.equal(v.captions.matchedToSegments, 1, 'the caption overlaps the far-side segment');
  assert.equal(v.captions.agreementRatio, 1);
  assert.ok(v.coverage.ratio > 0 && v.coverage.ratio <= 1);
});

test('verify flags a long silent tail as a dead interval', () => {
  const rec = { startedAt: T0, endedAt: T0 + 300000 }; // 5 min session, speech only in first 8s
  const merged = mergeTranscript({ me: meSegs, others: othersSegs, captions: [], startedAt: T0 });
  const v = verify(merged, { me: meSegs, others: othersSegs }, [], rec);
  assert.ok(v.deadIntervalCount >= 1, 'a 5-minute session with 8s of speech has dead air');
});

// ---------------------------------------------------------------------------------------
group('loro-correction seam — P1 identity pass, P4-shaped contract');

test('correct() is a pass-through in P1: same text, zero corrections, applied=false', () => {
  const segs = meSegs.map((s) => ({ ...s, label: 'Me' }));
  const out = correct(segs, {});
  assert.equal(out.applied, false);
  assert.equal(out.corrections.length, 0);
  assert.deepEqual(out.segments.map((s) => s.text), segs.map((s) => s.text));
});

test('correct() returns the exact shape P4 will fill: {segments, corrections[], applied, entitiesVersion}', () => {
  const out = correct([], { entitiesVersion: 'v0' });
  assert.ok(Array.isArray(out.segments) && Array.isArray(out.corrections));
  assert.equal(out.entitiesVersion, 'v0');
  assert.equal(typeof out.applied, 'boolean');
});

test('correct() does not mutate the caller\'s segments (defensive copy)', () => {
  const segs = [{ startMs: 0, endMs: 1, text: 'x', speaker: 'me', label: 'Me' }];
  const out = correct(segs, {});
  out.segments[0].text = 'mutated';
  assert.equal(segs[0].text, 'x');
});

// ---------------------------------------------------------------------------------------
group('loro entity memory loader — the correction stage input source (P4)');

test('normalizeEntities keeps valid rows, applies defaults, drops rows with no canonical', () => {
  const { entities, entitiesVersion } = normalizeEntities({
    version: 'v1',
    entities: [
      { canonical: 'Deepgram', type: 'product', mangled: ['deep graham'] },
      { canonical: '   ', type: 'product' }, // no real canonical -> dropped
      { type: 'person' }, // no canonical -> dropped
      { canonical: 'loro', caseSensitive: true, fuzzy: false },
    ],
  });
  assert.equal(entitiesVersion, 'v1');
  assert.equal(entities.length, 2);
  assert.equal(entities[0].fuzzy, true, 'fuzzy defaults true');
  assert.equal(entities[0].caseSensitive, false, 'caseSensitive defaults false');
  assert.equal(entities[1].caseSensitive, true);
  assert.equal(entities[1].fuzzy, false);
});

test('loadEntityMemory returns an EMPTY memory (never throws) when the file is missing', () => {
  const mem = loadEntityMemory('/nonexistent/loro/entities.json');
  assert.deepEqual(mem.entities, []);
  assert.equal(mem.entitiesVersion, null);
  assert.equal(mem.source, null);
});

test('the in-repo loro/entities.json, when present, loads and is well-formed', () => {
  const mem = loadEntityMemory(); if (mem.source === null) return; // repo default; absent in a clean clone (the missing-file contract is asserted above)
  assert.ok(typeof mem.entitiesVersion === 'string' && mem.entitiesVersion.length > 0);
  assert.ok(mem.entities.length >= 5, `expected several curated entities, got ${mem.entities.length}`);
  assert.ok(mem.entities.every((e) => typeof e.canonical === 'string' && e.canonical.length > 0));
});

// ---------------------------------------------------------------------------------------
group('loro-correction (P4 real corrector) — MEASURED precision/recall, no overcorrection');

const CORR_ENTITIES = normalizeEntities({
  version: 'corrtest-1',
  entities: [
    { canonical: 'Deepgram', type: 'product', mangled: ['deep graham'] },
    { canonical: 'whisper.cpp', type: 'product', mangled: ['whisper c p p'] },
    { canonical: 'RichOS', type: 'product', mangled: ['rich o s'] },
    { canonical: 'Rich Hanna', type: 'person', aliases: ['Rich'], mangled: ['rich hand'] },
    { canonical: 'loro', type: 'product', caseSensitive: true, aliases: ['Loro'], minScore: 0.8 }, // fuzzy target
    { canonical: 'Karpathy', type: 'person' }, // fuzzy target (no curated mangling)
  ],
}).entities;

// Planted manglings + the canonical each MUST become; `method` = the path that should catch it.
const PLANTED = [
  { text: 'I tested Deep Graham last week.', expect: 'Deepgram', method: 'curated' },
  { text: 'We built it on Whisper C P P.', expect: 'whisper.cpp', method: 'curated' },
  { text: 'The Rich O S dashboard shipped.', expect: 'RichOS', method: 'curated' },
  { text: 'Rich Hand owns the roadmap.', expect: 'Rich Hanna', method: 'curated' },
  { text: 'Our memory layer is called Lorow.', expect: 'loro', method: 'fuzzy' }, // NOT curated
  { text: 'The Karpathi wiki idea seeded it.', expect: 'Karpathy', method: 'fuzzy' }, // NOT curated
];
// Controls: ordinary text with words that superficially resemble entities — must NOT change.
const CONTROLS = [
  'Take a deep breath before the rich history lesson.',
  'The ground rules are simple.',
  'I had granola for breakfast.',
  'Rich, our CEO, will decide today.',
  'She went to the room to watch the team.',
];

test('recall = 1.0 — every planted mangling is corrected to its canonical', () => {
  let fixed = 0;
  for (const p of PLANTED) {
    const r = correctText(p.text, CORR_ENTITIES);
    const hit = r.corrections.find((c) => c.entity === p.expect);
    if (hit && r.text.includes(p.expect) && hit.method === p.method) fixed += 1;
    else console.log(`      recall miss: ${JSON.stringify(p.text)} -> ${JSON.stringify(r.text)}`);
  }
  const recall = fixed / PLANTED.length;
  console.log(`      recall = ${fixed}/${PLANTED.length} = ${recall.toFixed(3)}`);
  assert.equal(recall, 1, 'all planted manglings corrected by the expected path');
});

test('precision = 1.0 — zero false positives across planted + control text', () => {
  let tp = 0;
  let fp = 0;
  for (const p of PLANTED) {
    const r = correctText(p.text, CORR_ENTITIES);
    for (const c of r.corrections) {
      if (c.entity === p.expect) tp += 1;
      else fp += 1;
    }
  }
  for (const text of CONTROLS) {
    const r = correctText(text, CORR_ENTITIES);
    fp += r.corrections.length; // ANY correction on control text is a false positive
    assert.equal(r.text, text, `control text was overcorrected: ${JSON.stringify(text)} -> ${JSON.stringify(r.text)}`);
  }
  const precision = tp / (tp + fp || 1);
  console.log(`      precision = ${tp}/${tp + fp} = ${precision.toFixed(3)} (fp=${fp})`);
  assert.equal(fp, 0, 'no ordinary word was corrected into an entity');
  assert.equal(precision, 1);
});

test('correctText preserves surrounding punctuation on a fuzzy fix (Lorow. -> loro.)', () => {
  const r = correctText('Our layer is called Lorow.', CORR_ENTITIES);
  assert.equal(r.text, 'Our layer is called loro.');
});

test('a caseSensitive entity is not title-cased and an already-correct alias is left alone', () => {
  const r = correctText('The Loro layer and loro memory are the same.', CORR_ENTITIES);
  assert.equal(r.text, 'The Loro layer and loro memory are the same.', 'Loro alias + lowercase loro untouched');
  assert.equal(r.corrections.length, 0);
});

test('correct() over segments: applied=true, corrections carry segmentIndex, version recorded', () => {
  const segs = [
    { startMs: 0, endMs: 2000, text: 'Hi, this is Deep Graham speaking.', speaker: 'others', label: 'Them' },
    { startMs: 2000, endMs: 4000, text: 'We run on Whisper C P P.', speaker: 'me', label: 'Me' },
  ];
  const out = correct(segs, { entities: CORR_ENTITIES, entitiesVersion: 'corrtest-1' });
  assert.equal(out.applied, true);
  assert.equal(out.entitiesVersion, 'corrtest-1');
  assert.equal(out.segments[0].text, 'Hi, this is Deepgram speaking.');
  assert.equal(out.segments[1].text, 'We run on whisper.cpp.');
  assert.deepEqual(out.corrections.map((c) => c.segmentIndex).sort(), [0, 1]);
});

test('applied=true even with zero corrections (the corrector RAN; count says nothing changed)', () => {
  const segs = [{ startMs: 0, endMs: 1000, text: 'Nothing to fix here at all.', speaker: 'me', label: 'Me' }];
  const out = correct(segs, { entities: CORR_ENTITIES, entitiesVersion: 'corrtest-1' });
  assert.equal(out.applied, true);
  assert.equal(out.corrections.length, 0);
  assert.equal(out.segments[0].text, 'Nothing to fix here at all.');
});

test('similarity + levenshtein are sane (fuzzy metric guardrails)', () => {
  assert.equal(levenshtein('loro', 'loro'), 0);
  assert.equal(levenshtein('lorow', 'loro'), 1);
  assert.ok(similarity('deepgramm', 'deepgram') > 0.85);
  assert.ok(similarity('deep breath', 'deepgram') < 0.7, 'an ordinary phrase is far from the canonical');
  assert.equal(normalizeTerm('Rich O.S.!'), 'rich o s');
});

// ---------------------------------------------------------------------------------------
group('correction flywheel — EXPLICIT intake (learnTerm): dedup, no-clobber, versioned');

function baseDoc() {
  return {
    schemaVersion: 1,
    version: '2026-08-24',
    entities: [
      { canonical: 'Deepgram', type: 'product', aliases: [], mangled: ['deep graham'] },
      { canonical: 'loro', type: 'product', caseSensitive: true, aliases: ['Loro'], mangled: ['lorro'], fuzzy: true, minScore: 0.8 },
    ],
  };
}

test('learnTerm adds a NEW entity and bumps the version', () => {
  const before = baseDoc();
  const res = learnTerm(before, { canonical: 'Segment Anything', mangled: 'segment any thing', type: 'product' }, { today: '2026-08-25' });
  assert.equal(res.changed, true);
  assert.equal(res.created, true);
  assert.equal(res.doc.entities.length, 3);
  const e = res.doc.entities.find((x) => x.canonical === 'Segment Anything');
  assert.deepEqual(e.mangled, ['segment any thing']);
  assert.equal(e.type, 'product');
  assert.equal(res.doc.version, '2026-08-25', 'cross-day bump = the new date');
  assert.equal(before.entities.length, 2, 'input doc not mutated');
});

test('learnTerm MERGES a new mangling into an existing entity without clobbering curated data', () => {
  const before = baseDoc();
  const res = learnTerm(before, { canonical: 'Deepgram', mangled: 'deep gramme', type: 'company' });
  assert.equal(res.created, false);
  const e = res.doc.entities.find((x) => x.canonical === 'Deepgram');
  assert.deepEqual(e.mangled, ['deep graham', 'deep gramme'], 'existing mangling preserved, new one appended');
  assert.equal(e.type, 'product', 'curated type NOT clobbered by the supplied one');
  assert.equal(before.entities[0].mangled.length, 1, 'input doc not mutated');
});

test('learnTerm dedups a mangling it already has (no change, no version bump)', () => {
  const before = baseDoc();
  const res = learnTerm(before, { canonical: 'Deepgram', mangled: 'Deep Graham' }); // same after normalize
  assert.equal(res.changed, false);
  assert.equal(res.added.mangled.length, 0);
  assert.equal(res.doc.version, '2026-08-24', 'no bump when nothing changed');
});

test('learnTerm drops a mangling that normalizes to the canonical (correct.js would skip it anyway)', () => {
  const res = learnTerm(baseDoc(), { canonical: 'Deepgram', mangled: 'DEEPGRAM' }); // casing-only
  const e = res.doc.entities.find((x) => x.canonical === 'Deepgram');
  assert.deepEqual(e.mangled, ['deep graham'], 'a casing-only mangling is not stored (curated one kept)');
  assert.equal(res.changed, false, 'nothing learnable -> no change');
});

test('learnTerm refuses to steal a mangling already owned by a different canonical (conflict, precision)', () => {
  const before = baseDoc();
  const res = learnTerm(before, { canonical: 'Deepgraham Corp', mangled: 'deep graham' });
  assert.equal(res.conflicts.length, 1);
  const e = res.doc.entities.find((x) => x.canonical === 'Deepgraham Corp');
  assert.deepEqual(e.mangled, [], 'the conflicting mangling was skipped');
  // The original owner keeps it.
  assert.ok(res.doc.entities.find((x) => x.canonical === 'Deepgram').mangled.includes('deep graham'));
});

test('learnTerm matches an entity by an existing alias, adds a new alias, dedups aliases', () => {
  const res = learnTerm(baseDoc(), { canonical: 'Loro', aliases: ['Lauro', 'Loro'] }); // 'Loro' is an existing alias of 'loro'
  const e = res.doc.entities.find((x) => x.canonical === 'loro');
  assert.equal(res.created, false, 'matched the existing entity via its alias');
  assert.deepEqual(e.aliases, ['Loro', 'Lauro'], 'new alias added once; existing (dedup) kept');
});

test('bumpVersion increments an intra-day revision counter monotonically', () => {
  assert.equal(bumpVersion('2026-08-24', '2026-08-24'), '2026-08-24.1');
  assert.equal(bumpVersion('2026-08-24.1', '2026-08-24'), '2026-08-24.2');
  assert.equal(bumpVersion('2026-08-24', '2026-08-25'), '2026-08-25');
  assert.equal(bumpVersion('', '2026-08-25'), '2026-08-25');
});

test('serializeEntitiesDoc round-trips to valid JSON in the curated inline-array style', () => {
  const doc = baseDoc();
  const out = serializeEntitiesDoc(doc);
  assert.deepEqual(JSON.parse(out), doc, 'parses back to an identical object');
  assert.ok(out.includes('"mangled": ["deep graham"]'), 'string arrays stay inline (minimal git diff)');
  assert.ok(out.endsWith('}\n'), 'trailing newline');
});

// ---------------------------------------------------------------------------------------
group('correction flywheel — TRANSCRIPT-EDIT diff intake: PRECISION over recall');

test('tokenReplaceHunks expands a name fix to the full proper-noun span and ignores pure insertions', () => {
  // "Hand"->"Hanna" absorbs the adjacent unchanged term token "Rich" so the mangling is the WHOLE
  // name (safe), never the dangerous lone word "Hand".
  // `coreFrom`/`coreTo` keep the UNEXPANDED delta beside it, because a similarity gate scored on the
  // expanded span is scored partly on context that is identical by construction.
  assert.deepEqual(
    tokenReplaceHunks('we run on Rich Hand today'.split(' '), 'we run on Rich Hanna today'.split(' ')),
    [{ from: 'Rich Hand', to: 'Rich Hanna', coreFrom: 'Hand', coreTo: 'Hanna' }],
  );
  // a pure insertion (no removed counterpart) yields no replace hunk
  assert.deepEqual(tokenReplaceHunks('we shipped it'.split(' '), 'we finally shipped it'.split(' ')), []);
});

test('looksLikeTerm accepts proper nouns / dotted terms and rejects ordinary words', () => {
  assert.equal(looksLikeTerm('Deepgram'), true);
  assert.equal(looksLikeTerm('Rich Hanna'), true);
  assert.equal(looksLikeTerm('whisper.cpp'), true);
  assert.equal(looksLikeTerm('RichOS'), true);
  assert.equal(looksLikeTerm('great'), false);
  assert.equal(looksLikeTerm('shall'), false);
  assert.equal(looksLikeTerm('the'), false);
});

// A realistic edited transcript: the CEO fixed ONE name and ONE ordinary word.
const BASELINE_TRANSCRIPT = [
  '# Transcript — call',
  '',
  '- **Session:** `s-1`',
  '',
  '---',
  '',
  '**[00:02] Them:** So Rich Hand walked us through the deep graham setup.',
  '',
  '**[00:15] Me:** Right, the plan is solid and we ship Monday.',
  '',
].join('\n');

const EDITED_TRANSCRIPT = [
  '# Transcript — call',
  '',
  '- **Session:** `s-1`',
  '',
  '---',
  '',
  '**[00:02] Them:** So Rich Hanna walked us through the Deepgram setup.',
  '',
  '**[00:15] Me:** Right, the plan is great and we ship Monday.', // ordinary word edit: solid -> great
  '',
].join('\n');

test('extractTermCorrections proposes the NAME + TERM fixes and NOT the ordinary word edit (precision)', () => {
  const { proposals, rejected } = extractTermCorrections(BASELINE_TRANSCRIPT, EDITED_TRANSCRIPT);
  const pairs = proposals.map((p) => `${p.from}=>${p.to}`).sort();
  assert.deepEqual(pairs, ['Rich Hand=>Rich Hanna', 'deep graham=>Deepgram']);
  // the ordinary-word edit must be rejected, not proposed (false-positive guard)
  assert.ok(!pairs.some((p) => p.includes('great')), 'solid->great was NOT learned');
  assert.ok(rejected.some((r) => r.to === 'great'), 'solid->great is explicitly recorded as rejected');
});

test('extractTermCorrections rejects a wholesale rewrite even when the new side is a proper noun', () => {
  const base = '**[00:01] Them:** um yeah exactly.';
  const edited = '**[00:01] Them:** Marcus Whitfield yeah exactly.'; // "um" -> a full name = a rewrite, not a mangling
  const { proposals } = extractTermCorrections(base, edited);
  assert.equal(proposals.length, 0, 'low edit-similarity rejects the rewrite');
});

test('learnFromEdits is PROPOSE-ONLY by default (doc untouched) and applies with apply=true', () => {
  const before = baseDoc();
  const dry = learnFromEdits(before, BASELINE_TRANSCRIPT, EDITED_TRANSCRIPT);
  assert.equal(dry.applied, false);
  assert.equal(dry.doc, before, 'default run does not touch the doc');
  assert.equal(dry.proposals.length, 2);

  const wet = learnFromEdits(before, BASELINE_TRANSCRIPT, EDITED_TRANSCRIPT, { apply: true, today: '2026-08-25' });
  assert.equal(wet.applied, true);
  const dg = wet.doc.entities.find((x) => x.canonical === 'Deepgram');
  assert.ok(dg.mangled.includes('deep graham'), 'existing curated mangling kept');
  const rh = wet.doc.entities.find((x) => x.canonical === 'Rich Hanna');
  assert.ok(rh && rh.mangled.includes('rich hand'), 'new person entity created from the edit');
  assert.equal(wet.doc.version, '2026-08-25');
});

// ---------------------------------------------------------------------------------------
group('correction flywheel — CLOSED LOOP: capture -> entities.json -> a later transcript is corrected');

test('a captured term fixes a subsequent transcript that mangles it (end-to-end through correct())', () => {
  // 1. A brand-new term the corrector does NOT yet know.
  const doc0 = baseDoc();
  const pre = correct(
    [{ startMs: 0, endMs: 1000, text: 'We evaluated Segment Any Thing for masks.', speaker: 'me', label: 'Me' }],
    normalizeEntities(doc0),
  );
  assert.equal(pre.segments[0].text, 'We evaluated Segment Any Thing for masks.', 'unknown term is NOT corrected pre-capture');

  // 2. Capture the correction (explicit intake).
  const { doc: doc1 } = learnTerm(doc0, { canonical: 'Segment Anything', mangled: 'segment any thing', type: 'product' });

  // 3. The SAME mangling now gets fixed — the loop is closed, and entitiesVersion advanced.
  const post = correct(
    [{ startMs: 0, endMs: 1000, text: 'We evaluated Segment Any Thing for masks.', speaker: 'me', label: 'Me' }],
    normalizeEntities(doc1),
  );
  assert.equal(post.segments[0].text, 'We evaluated Segment Anything for masks.', 'captured term now corrected');
  assert.notEqual(post.entitiesVersion, pre.entitiesVersion, 'entitiesVersion bumped by the capture');
});

// ---------------------------------------------------------------------------------------
group('coordination (P4) — one session per call, no double-capture, browser-crash failover');

const T = 2_000_000_000_000;
function liveExt({ id = 'ext-1', hb = T, hint = 'Google Chrome' } = {}) {
  return { sessionId: id, surface: 'chrome-extension', captureKind: CAPTURE_KIND.browserTab, processHint: hint, status: 'open', lastHeartbeat: hb };
}

test('preferenceRank: browser-tab (richest) > process > system (coverage net)', () => {
  assert.ok(preferenceRank(CAPTURE_KIND.browserTab) > preferenceRank(CAPTURE_KIND.process));
  assert.ok(preferenceRank(CAPTURE_KIND.process) > preferenceRank(CAPTURE_KIND.system));
});

test('scopesOverlap: all-system captures everything; a per-app scope only overlaps the same app', () => {
  assert.equal(scopesOverlap({ kind: CAPTURE_KIND.system }, { kind: CAPTURE_KIND.browserTab }), true);
  assert.equal(scopesOverlap({ kind: CAPTURE_KIND.system }, { kind: CAPTURE_KIND.process, processHint: 'zoom.us' }), true);
  assert.equal(scopesOverlap({ kind: CAPTURE_KIND.browserTab, processHint: 'Chrome' }, { kind: CAPTURE_KIND.process, processHint: 'zoom.us' }), false);
  assert.equal(scopesOverlap({ kind: CAPTURE_KIND.process, processHint: 'zoom.us' }, { kind: CAPTURE_KIND.process, processHint: 'zoom.us' }), true);
});

test('THE HANDSHAKE: a companion stands down while the extension owns the browser call (no double)', () => {
  const decision = decideClaim(
    { surface: 'desktop-companion-macos', sessionId: 'mac-1', captureKind: CAPTURE_KIND.system },
    [liveExt()],
    { now: T + 1000 },
  );
  assert.equal(decision.decision, 'stand-down');
  assert.equal(decision.conflictSessionId, 'ext-1');
  assert.equal(decision.excludeProcessHint, 'Google Chrome', 'hands back the browser process to exclude');
});

test('a companion OWNS a desktop-app call the extension is not capturing (no browser session live)', () => {
  const decision = decideClaim(
    { surface: 'desktop-companion-macos', sessionId: 'mac-2', captureKind: CAPTURE_KIND.process, processHint: 'zoom.us' },
    [liveExt()], // extension owns a browser tab — different app, no overlap
    { now: T + 1000 },
  );
  assert.equal(decision.decision, 'own');
});

test('a STALE extension session is not a live conflict — the companion may own', () => {
  const decision = decideClaim(
    { surface: 'desktop-companion-macos', sessionId: 'mac-3', captureKind: CAPTURE_KIND.system },
    [liveExt({ hb: T })],
    { now: T + 60000 }, // 60 s later: the extension session is stale
  );
  assert.equal(decision.decision, 'own');
});

test('the richer surface wins if it arrives late: extension supersedes a live all-system companion', () => {
  const liveMac = { sessionId: 'mac-4', surface: 'desktop-companion-macos', captureKind: CAPTURE_KIND.system, status: 'open', lastHeartbeat: T };
  const decision = decideClaim(
    { surface: 'chrome-extension', sessionId: 'ext-late', captureKind: CAPTURE_KIND.browserTab, processHint: 'Chrome' },
    [liveMac],
    { now: T + 1000 },
  );
  assert.equal(decision.decision, 'own');
  assert.deepEqual(decision.supersede, ['mac-4']);
});

test('FAILOVER: an interrupted browser call (crash) is promotable; a fresh one is not', () => {
  const now = T + 5000;
  const sessions = [
    { sessionId: 'ext-dead', surface: 'chrome-extension', captureKind: CAPTURE_KIND.browserTab, processHint: 'Chrome', status: 'interrupted', lastHeartbeat: T, hasTranscript: false },
    { sessionId: 'ext-live', surface: 'chrome-extension', captureKind: CAPTURE_KIND.browserTab, status: 'open', lastHeartbeat: now, hasTranscript: false },
  ];
  const promotable = findPromotable(sessions, { now });
  assert.equal(promotable.length, 1);
  assert.equal(promotable[0].sessionId, 'ext-dead');
  const ownership = buildPromotionOwnership({ deadSessionId: 'ext-dead', surface: 'desktop-companion-macos', processHint: 'Chrome' });
  assert.equal(ownership.supersedes, 'ext-dead');
  assert.equal(ownership.ownerSurface, 'desktop-companion-macos');
});

test('FAILOVER: a stale-open owner past the promote threshold is promotable; already-superseded is not', () => {
  const now = T + 20000;
  const sessions = [
    { sessionId: 'ext-hung', surface: 'chrome-extension', captureKind: CAPTURE_KIND.browserTab, status: 'open', lastHeartbeat: T, hasTranscript: false },
    { sessionId: 'ext-taken', surface: 'chrome-extension', captureKind: CAPTURE_KIND.browserTab, status: 'interrupted', lastHeartbeat: T, hasTranscript: false, supersededBy: 'mac-x' },
  ];
  const promotable = findPromotable(sessions, { now, promoteAfterMs: 12000 });
  assert.deepEqual(promotable.map((p) => p.sessionId), ['ext-hung']);
});

test('pipeline dedup backstop: two overlapping sessions keep the richer (captions) one', () => {
  const { keep, shadow } = dedupeOverlapping(
    [
      { sessionId: 'ext-rich', captureKind: CAPTURE_KIND.browserTab, captionCount: 12, startedAt: T, endedAt: T + 600000 },
      { sessionId: 'mac-plain', captureKind: CAPTURE_KIND.system, captionCount: 0, startedAt: T + 1000, endedAt: T + 600000 },
    ],
    { minOverlapMs: 30000 },
  );
  assert.deepEqual(keep, ['ext-rich']);
  assert.equal(shadow.length, 1);
  assert.equal(shadow[0].sessionId, 'mac-plain');
  assert.equal(shadow[0].preferredSessionId, 'ext-rich');
});

test('sessionDescriptor maps a companion record to a system-scope descriptor with its heartbeat', () => {
  const rec = {
    sessionId: 'mac-9', status: 'open', startedAt: T,
    capture: { source: 'desktop-companion-macos', captureTarget: 'system' },
    ownership: { ownerSurface: 'desktop-companion-macos', processHint: null },
  };
  const d = sessionDescriptor(rec, { lastHeartbeat: T + 3000, hasTranscript: false });
  assert.equal(d.captureKind, CAPTURE_KIND.system);
  assert.equal(d.surface, 'desktop-companion-macos');
  assert.equal(d.lastHeartbeat, T + 3000);
});

// ---------------------------------------------------------------------------------------
group('native-messaging stdio framing — round-trips length-prefixed JSON');

test('encodeMessage + FrameDecoder round-trip a message', () => {
  const dec = new FrameDecoder();
  const out = dec.push(encodeMessage({ type: 'hello', v: 1 }));
  assert.equal(out.length, 1);
  assert.deepEqual(out[0], { type: 'hello', v: 1 });
});

test('the decoder reassembles a message split across chunk boundaries', () => {
  const dec = new FrameDecoder();
  const frame = encodeMessage({ type: 'audio-chunk', part: 0, dataB64: 'AAAA' });
  assert.deepEqual(dec.push(frame.subarray(0, 3)), []); // header incomplete
  assert.deepEqual(dec.push(frame.subarray(3, 10)), []); // body incomplete
  const done = dec.push(frame.subarray(10));
  assert.equal(done.length, 1);
  assert.equal(done[0].type, 'audio-chunk');
});

test('two concatenated frames decode as two messages in one push', () => {
  const dec = new FrameDecoder();
  const buf = Buffer.concat([encodeMessage({ a: 1 }), encodeMessage({ b: 2 })]);
  const out = dec.push(buf);
  assert.equal(out.length, 2);
  assert.deepEqual(out.map((m) => Object.keys(m)[0]), ['a', 'b']);
});

// ---------------------------------------------------------------------------------------
group('native host handlers (SessionSink) — writes the SAME contract dir as the sync helper');

test('capture rejects path-shaped session IDs before creating any files', () => {
  const root = tmp();
  const zone = path.join(root, 'zone');
  try {
    const sink = new SessionSink(zone);
    for (const sessionId of ['../escaped', '.', '..', '/absolute', 'a/b', 'a\\b', 'a\0b', 42]) {
      assert.throws(() => sink.handle({ type: 'session-start', record: { sessionId } }), /capture boundary/);
      assert.throws(() => sink.handle({ type: 'session-close', sessionId, record: {} }), /capture boundary/);
    }
    assert.deepEqual(fs.readdirSync(zone), []);
  } finally { fs.rmSync(root, { recursive: true, force: true }); }
});

test('capture rejects linked session directories and accepts an external zone alias', () => {
  const root = tmp();
  try {
    const zone = path.join(root, 'zone');
    const outside = path.join(root, 'outside');
    fs.mkdirSync(zone); fs.mkdirSync(outside);
    fs.symlinkSync(zone, path.join(root, 'alias'), 'dir');
    const sink = new SessionSink(path.join(root, 'alias'));
    fs.symlinkSync(outside, path.join(zone, 'linked'), 'dir');
    assert.throws(() => sink.handle({ type: 'session-start', record: { sessionId: 'linked' } }), /capture boundary/);
    assert.equal(sink.handle({ type: 'session-start', record: { sessionId: 'normal' } }).type, 'started');
    assert.deepEqual(fs.readdirSync(outside), []);
  } finally { fs.rmSync(root, { recursive: true, force: true }); }
});

test('capture accepts safe room names produced from encoded URLs', () => {
  const zone = tmp();
  try {
    const sink = new SessionSink(zone);
    for (const sessionId of ['2026--whereby--caf%C3%A9', '2026--whereby--room!@+name']) {
      assert.equal(sink.handle({ type: 'session-start', record: { sessionId } }).type, 'started');
      assert.equal(sink.handle({ type: 'audio-chunk', sessionId, dataB64: 'YQ==' }).type, 'chunk-ack');
      assert.equal(fs.readFileSync(path.join(zone, sessionId, 'audio-part-00.webm'), 'utf8'), 'a');
    }
  } finally { fs.rmSync(zone, { recursive: true, force: true }); }
});

test('capture validates audio extensions before accepting a start or chunk', () => {
  const zone = tmp();
  try {
    const sink = new SessionSink(zone);
    for (const audioExt of ['../../escape', 'a/b', 'a\\b', 'webm\0', {}]) {
      assert.throws(() => sink.handle({ type: 'session-start', record: { sessionId: 'bad' }, audioExt }), /capture boundary/);
    }
    assert.deepEqual(fs.readdirSync(zone), []);
    sink.handle({ type: 'session-start', record: { sessionId: 'safe' } });
    assert.throws(() => sink.handle({ type: 'audio-chunk', sessionId: 'safe', ext: '../escape' }), /capture boundary/);
  } finally { fs.rmSync(zone, { recursive: true, force: true }); }
});

test('every capture writer refuses linked output files without modifying their targets', () => {
  const root = tmp();
  try {
    const zone = path.join(root, 'zone');
    const target = path.join(root, 'private-file');
    const original = '{"status":"open"}\n';
    fs.writeFileSync(target, original);
    const sink = new SessionSink(zone);
    const cases = [
      ['session.json', { type: 'session-start', record: { sessionId: 'safe' } }],
      ['session.json', { type: 'session-close', sessionId: 'safe' }],
      ['audio-part-00.webm', { type: 'audio-chunk', sessionId: 'safe', dataB64: 'YQ==' }],
      ['health.ndjson', { type: 'health', sessionId: 'safe', line: { t: T0 } }],
      ['health.ndjson', { type: 'heartbeat', sessionId: 'safe' }],
      ['captions.ndjson', { type: 'caption', sessionId: 'safe', line: { text: 'synthetic' } }],
    ];
    for (const [file, msg] of cases) {
      sink.handle({ type: 'session-start', record: { sessionId: 'safe' } }, T0);
      const output = path.join(zone, 'safe', file);
      fs.rmSync(output, { force: true });
      fs.symlinkSync(target, output);
      assert.throws(() => sink.handle(msg), /capture boundary/);
      assert.equal(fs.readFileSync(target, 'utf8'), original);
      fs.unlinkSync(output);
    }
    sink.handle({ type: 'session-start', record: { sessionId: 'safe' } }, T0);
    const output = path.join(zone, 'safe', 'session.json');
    fs.unlinkSync(output); fs.linkSync(target, output);
    assert.throws(() => sink.handle({ type: 'session-close', sessionId: 'safe' }), /capture boundary/);
    assert.equal(fs.readFileSync(target, 'utf8'), original);
  } finally { fs.rmSync(root, { recursive: true, force: true }); }
});

test('capture rechecks directories after start including watchdog and EOF cleanup', () => {
  const root = tmp();
  try {
    const zone = path.join(root, 'zone');
    const outside = path.join(root, 'outside');
    fs.mkdirSync(outside);
    const original = '{"status":"open"}\n';
    fs.writeFileSync(path.join(outside, 'session.json'), original);
    const sink = new SessionSink(zone);
    sink.handle({ type: 'session-start', record: { sessionId: 'safe' } }, T0);
    fs.renameSync(path.join(zone, 'safe'), path.join(zone, 'original'));
    fs.symlinkSync(outside, path.join(zone, 'safe'), 'dir');
    for (const type of ['audio-chunk', 'health', 'caption', 'heartbeat', 'session-close']) {
      assert.throws(() => sink.handle({ type, sessionId: 'safe', line: {}, dataB64: 'YQ==' }), /capture boundary/);
    }
    sink.checkWatchdog(T0 + 30000);
    assert.deepEqual(sink.finalizeOnEof(T0 + 30000), []);
    assert.equal(fs.readFileSync(path.join(outside, 'session.json'), 'utf8'), original);
    assert.deepEqual(fs.readdirSync(outside), ['session.json']);
  } finally { fs.rmSync(root, { recursive: true, force: true }); }
});

test('session-start creates the contract dir with an OPEN, v2 session.json', () => {
  const zone = tmp();
  const sink = new SessionSink(zone);
  const resp = sink.handle({ type: 'session-start', record: { sessionId: 'sX', platform: { id: 'meet' } } }, T0);
  assert.equal(resp.type, 'started');
  const rec = JSON.parse(fs.readFileSync(path.join(zone, 'sX', 'session.json'), 'utf8'));
  assert.equal(rec.status, 'open');
  assert.equal(rec.schemaVersion, CONTRACT_SCHEMA_VERSION);
  assert.equal(rec.capture.source, 'chrome-extension');
  fs.rmSync(zone, { recursive: true, force: true });
});

test('audio-chunk / health / caption append the right files; session-close finalizes + triggers the pipeline', () => {
  const zone = tmp();
  const sink = new SessionSink(zone);
  sink.handle({ type: 'session-start', record: { sessionId: 'sY' } }, T0);
  sink.handle({ type: 'audio-chunk', sessionId: 'sY', part: 0, dataB64: Buffer.from('opusbytes').toString('base64') }, T0 + 1000);
  sink.handle({ type: 'health', sessionId: 'sY', line: { t: T0 + 1000, level: 'green' } }, T0 + 1000);
  sink.handle({ type: 'caption', sessionId: 'sY', line: { speaker: 'Ada', text: 'hi', t: T0 + 1200 } }, T0 + 1200);
  const close = sink.handle({ type: 'session-close', sessionId: 'sY', record: { audio: { parts: [{ part: 0 }], bytesTotal: 9 } } }, T0 + 5000);
  assert.equal(close.type, 'closed');
  assert.equal(close._trigger, 'sY', 'close hands the session to the pipeline');
  const dir = path.join(zone, 'sY');
  assert.equal(fs.readFileSync(path.join(dir, 'audio-part-00.webm'), 'utf8'), 'opusbytes');
  assert.equal(fs.readFileSync(path.join(dir, 'health.ndjson'), 'utf8').trim().length > 0, true);
  assert.equal(fs.readFileSync(path.join(dir, 'captions.ndjson'), 'utf8').trim().length > 0, true);
  const rec = JSON.parse(fs.readFileSync(path.join(dir, 'session.json'), 'utf8'));
  assert.equal(rec.status, 'closed');
  assert.equal(rec.endedAt, T0 + 5000);
  fs.rmSync(zone, { recursive: true, force: true });
});

test('the watchdog alarms on a stale heartbeat (browser stopped talking mid-call)', () => {
  const zone = tmp();
  const sink = new SessionSink(zone);
  sink.handle({ type: 'session-start', record: { sessionId: 'sZ' } }, T0);
  assert.deepEqual(sink.checkWatchdog(T0 + 5000), [], 'fresh heartbeat: no alarm');
  const alarms = sink.checkWatchdog(T0 + 20000);
  assert.equal(alarms.length, 1);
  assert.equal(alarms[0].sessionId, 'sZ');
  fs.rmSync(zone, { recursive: true, force: true });
});

test('on pipe EOF an open session is finalized INTERRUPTED — a lost call is present on disk', () => {
  const zone = tmp();
  const sink = new SessionSink(zone);
  sink.handle({ type: 'session-start', record: { sessionId: 'sEOF' } }, T0);
  const finalized = sink.finalizeOnEof(T0 + 9000);
  assert.deepEqual(finalized, ['sEOF']);
  const rec = JSON.parse(fs.readFileSync(path.join(zone, 'sEOF', 'session.json'), 'utf8'));
  assert.equal(rec.status, 'interrupted');
  assert.ok(rec.notes.some((n) => /interrupted by the native host/.test(n)));
  fs.rmSync(zone, { recursive: true, force: true });
});

test('reconciliation reports abandoned open sessions in reporting and processing modes', () => {
  const zone = tmp();
  try {
    const sink = new SessionSink(zone);
    sink.handle({ type: 'session-start', record: { sessionId: 'abandoned', startedAt: T0 } }, T0);
    sink.handle({ type: 'audio-chunk', sessionId: 'abandoned', dataB64: 'YQ==' }, T0);
    for (const process of [false, true]) {
      const result = scanZone({ zone, now: T0 + 16000, process });
      assert.equal(result.anomalies.length, 1);
      assert.equal(result.anomalies[0].sessionId, 'abandoned');
      assert.match(result.anomalies[0].problems.join(' '), /heartbeat|activity/);
      assert.deepEqual(result.transcribed, []);
      assert.deepEqual(result.skipped, []);
      assert.equal(JSON.parse(fs.readFileSync(path.join(zone, 'abandoned', 'session.json'))).status, 'open');
    }
  } finally { fs.rmSync(zone, { recursive: true, force: true }); }
});

test('watcher isolates a broken recording on every scan and still inspects later sessions', () => {
  const zone = tmp();
  const broken = path.join(zone, 'a-broken');
  const ready = path.join(zone, 'b-ready');
  function record(dir, id, status) {
    fs.mkdirSync(dir);
    fs.writeFileSync(path.join(dir, 'session.json'), JSON.stringify({ sessionId: id, status: 'closed',
      startedAt: T0, endedAt: T0 + 1000, audio: { parts: ['audio-part-0.wav'], bytesTotal: 9 },
      pipeline: { status } }));
  }
  try {
    record(broken, 'broken', 'pending'); record(ready, 'ready', 'ready');
    fs.writeFileSync(path.join(ready, 'audio-part-0.wav'), 'synthetic');
    fs.writeFileSync(path.join(ready, 'transcript.md'), 'Synthetic completed transcript.');
    fs.symlinkSync(path.join(zone, 'missing.wav'), path.join(broken, 'audio-part-0.wav'));
    for (const process of [false, true]) {
      for (let repeat = 0; repeat < 2; repeat++) {
        const result = scanZone({ zone, now: T0 + 2000, process });
        assert.deepEqual(result.skipped, ['ready']);
        assert.equal(result.anomalies.length, 1);
        assert.equal(result.anomalies[0].sessionId, 'broken');
        assert.match(result.anomalies[0].problems.join(' '), /ENOENT/);
      }
    }
  } finally { fs.rmSync(zone, { recursive: true, force: true }); }
});

test('watcher preserves storage refusal without blocking the next pending pipeline', () => {
  const zone = tmp();
  const outside = tmp();
  const refused = path.join(zone, 'a-refused');
  const pending = path.join(zone, 'b-pending');
  try {
    const sentinel = path.join(outside, 'transcript.md'); fs.writeFileSync(sentinel, 'Do not overwrite.');
    for (const dir of [refused, pending]) {
      fs.mkdirSync(dir);
      fs.writeFileSync(path.join(dir, 'session.json'), JSON.stringify({ sessionId: path.basename(dir),
        status: 'closed', startedAt: T0, endedAt: T0 + 1000,
        audio: { parts: ['audio-part-0.wav'], bytesTotal: 9 }, pipeline: { status: 'pending' } }));
      // An ordinary but invalid media file proves the later pipeline is reached
      // without requiring speech models: normalization records its failure.
      fs.writeFileSync(path.join(dir, 'audio-part-0.wav'), 'synthetic');
    }
    fs.symlinkSync(sentinel, path.join(refused, 'transcript.md'));
    const original = fs.readFileSync(path.join(refused, 'session.json'), 'utf8');
    const result = scanZone({ zone, now: T0 + 2000, tier: 'low-resource' });
    assert.equal(result.anomalies.length, 2);
    assert.match(result.anomalies.find((r) => r.sessionId === 'a-refused').problems.join(' '), /storage boundary/);
    assert.equal(fs.readFileSync(sentinel, 'utf8'), 'Do not overwrite.');
    assert.equal(fs.readFileSync(path.join(refused, 'session.json'), 'utf8'), original);
    assert.equal(JSON.parse(fs.readFileSync(path.join(pending, 'session.json'))).pipeline.status, 'failed');
  } finally {
    fs.rmSync(zone, { recursive: true, force: true });
    fs.rmSync(outside, { recursive: true, force: true });
  }
});

test('fresh starts and durable native heartbeats keep active recordings out of recovery', () => {
  const zone = tmp();
  try {
    const sink = new SessionSink(zone);
    sink.handle({ type: 'session-start', record: { sessionId: 'live', startedAt: T0 } }, T0);
    assert.deepEqual(scanZone({ zone, now: T0 + 14000 }).skipped, ['live']);
    let heartbeat = T0;
    for (const type of ['heartbeat', 'audio-chunk', 'health']) {
      heartbeat += 30000;
      sink.handle({ type, sessionId: 'live', line: { level: 'green', t: 1 }, dataB64: 'YQ==' }, heartbeat);
      const result = scanZone({ zone, now: heartbeat + 10000 });
      assert.deepEqual(result.anomalies, [], type);
      assert.deepEqual(result.skipped, ['live'], type);
      assert.deepEqual(result.transcribed, []);
    }
    assert.equal(scanZone({ zone, now: heartbeat + 16000 }).anomalies.length, 1);
  } finally { fs.rmSync(zone, { recursive: true, force: true }); }
});

test('companion health ticks remain usable with a torn final health line', () => {
  const zone = tmp();
  try {
    const dir = path.join(zone, 'companion'); fs.mkdirSync(dir);
    fs.writeFileSync(path.join(dir, 'session.json'), JSON.stringify({ sessionId: 'companion', status: 'open', startedAt: 1 }));
    fs.writeFileSync(path.join(dir, 'health.ndjson'), JSON.stringify({ t: T0 }) + '\n{"t":');
    assert.deepEqual(scanZone({ zone, now: T0 + 10000 }).skipped, ['companion']);
    assert.equal(scanZone({ zone, now: T0 + 16000 }).anomalies.length, 1);
  } finally { fs.rmSync(zone, { recursive: true, force: true }); }
});

test('an open session without any usable liveness timestamp is an anomaly', () => {
  const zone = tmp();
  try {
    const dir = path.join(zone, 'unknown'); fs.mkdirSync(dir);
    fs.writeFileSync(path.join(dir, 'session.json'), JSON.stringify({ sessionId: 'unknown', status: 'open' }));
    const result = scanZone({ zone, now: T0, process: false });
    assert.equal(result.anomalies.length, 1);
    assert.deepEqual(result.skipped, []);
  } finally { fs.rmSync(zone, { recursive: true, force: true }); }
});

// ---------------------------------------------------------------------------------------
group('ingest ledger — collector-path parity, idempotent by (sessionId, runIndex)');

test('appendLedger writes once and dedups an identical (sessionId, runIndex)', () => {
  const zone = tmp();
  const first = appendLedger({ sessionId: 's', runIndex: 0, model: 'large-v3-turbo', words: 100 }, zone);
  assert.equal(first.appended, true);
  assert.equal(alreadyLedgered('s', 0, zone), true);
  const dup = appendLedger({ sessionId: 's', runIndex: 0, model: 'large-v3-turbo', words: 100 }, zone);
  assert.equal(dup.appended, false, 're-running the pipeline never double-ingests');
  const second = appendLedger({ sessionId: 's', runIndex: 1, model: 'large-v3', words: 105 }, zone);
  assert.equal(second.appended, true, 'a re-transcription appends a NEW run row for the same session');
  fs.rmSync(zone, { recursive: true, force: true });
});

// ---------------------------------------------------------------------------------------
group('parsers — ffmpeg channel probe + whisper JSON');

test('parseChannels reads mono/stereo/N-channel from ffmpeg stderr', () => {
  assert.equal(parseChannels('Stream #0:0: Audio: opus, 48000 Hz, stereo, fltp'), 2);
  assert.equal(parseChannels('Stream #0:0: Audio: pcm_s16le, 16000 Hz, mono, s16'), 1);
  assert.equal(parseChannels('Stream #0:0: Audio: aac, 44100 Hz, 6 channels, fltp'), 6);
  assert.equal(parseChannels('no audio here'), 0);
});

test('parseVolume reads mean/max dBFS and treats -inf as digital silence', () => {
  const real = parseVolume('[Parsed_volumedetect_0] mean_volume: -23.4 dB\n[Parsed_volumedetect_0] max_volume: -3.1 dB');
  assert.equal(real.meanDb, -23.4);
  assert.equal(real.maxDb, -3.1);
  const silent = parseVolume('[Parsed_volumedetect_0] mean_volume: -inf dB\n[Parsed_volumedetect_0] max_volume: -inf dB');
  assert.equal(silent.maxDb, -Infinity);
  assert.ok(silent.maxDb <= SILENCE_MAX_DB, 'a -inf channel is below the silence floor');
  assert.ok(real.maxDb > SILENCE_MAX_DB, 'a -3 dB channel is well above the silence floor');
});

test('parseWhisperJson normalizes segments and drops empties', () => {
  const json = {
    transcription: [
      { offsets: { from: 0, to: 2000 }, text: ' Hello there.' },
      { offsets: { from: 2000, to: 2500 }, text: '   ' },
      { offsets: { from: 2500, to: 4000 }, text: 'General Kenobi' },
    ],
  };
  const segs = parseWhisperJson(json, 'others');
  assert.equal(segs.length, 2);
  assert.deepEqual(segs[0], { startMs: 0, endMs: 2000, text: 'Hello there.', speaker: 'others' });
  assert.equal(segs[1].text, 'General Kenobi');
});

// ---------------------------------------------------------------------------------------
group('P5 model tiering — quantized default, full turbo + large-v3 opt-in, small.en fallback');

test('the default tier is quantized turbo when nothing is specified (CEO decision page §10)', () => {
  // MOVED 2026-09-10 from full `large-v3-turbo`. Memory decided it: 884,981,760 B peak RSS at call
  // length against 2,014,101,504 B — a factor of 2.28, the one axis in
  // docs/measurements/whisper-model-choice-2026-09-10/ that is decisive rather than close.
  const t = resolveTier(null);
  assert.equal(t.name, DEFAULT_TIER);
  assert.equal(DEFAULT_TIER, 'quantized');
  assert.equal(t.model, 'large-v3-turbo-q5_0');
  assert.deepEqual(t.decodeArgs, []); // no tier-private decode params on any tier
});

test('DEFAULT_MODEL is DERIVED from the default tier — one declaration, not two that agree', () => {
  // The failure this forecloses: `DEFAULT_MODEL` was its own string literal 90 lines from a tier
  // table naming the same model, so `transcribeChannel()` and `runPipeline()` could disagree about
  // the default after one edit. A second consumer disagreeing with the first is how this pipeline
  // shipped `-mc -1` on the two tiers that actually ran while `config.js` documented `-mc 0`.
  assert.equal(DEFAULT_MODEL, MODEL_TIERS[DEFAULT_TIER].model);
  assert.equal(DEFAULT_MODEL, 'large-v3-turbo-q5_0');
  assert.equal(resolveTier(null).model, DEFAULT_MODEL, 'the tier path and the raw-model path agree');
});

test('full turbo is NOT deleted — it stays selectable by tier, by raw id, and by pin', () => {
  // "Stops being the default and stays fetchable" is half of the ruling, so it gets a test.
  assert.equal(MODEL_TIERS.turbo.model, 'large-v3-turbo', 'the turbo tier still names full turbo');
  assert.equal(resolveTier('turbo').model, 'large-v3-turbo');
  assert.equal(resolveTier('large-v3-turbo').model, 'large-v3-turbo', 'raw --model still works');
  assert.ok(pinFor('large-v3-turbo'), 'full turbo keeps its pin, so `fetch-model` can still get it');
  assert.equal(pinFor('large-v3-turbo').bytes, 1624555275);
});

test('a tier id NEVER changes which weights it means — only which tier is default moved', () => {
  // A session.json recording `tier: "turbo"` before 2026-09-10 must still mean the same weights,
  // or every past record silently re-attributes itself. The default moved by re-pointing
  // DEFAULT_TIER, never by redefining a name.
  assert.equal(MODEL_TIERS.turbo.model, 'large-v3-turbo');
  assert.equal(MODEL_TIERS.max.model, 'large-v3');
  assert.equal(MODEL_TIERS['low-resource'].model, 'small.en');
  assert.equal(MODEL_TIERS.quantized.model, 'large-v3-turbo-q5_0');
});

// ---------------------------------------------------------------------------------------
group('Hardware resolution — the batch surface stops handing every machine the same decode');

// A machine roomy enough that only SPEED is in play, and one that cannot carry the default.
const HOST_ROOMY = { totalBytes: 25769803776, availableBytes: 6418923520, cores: 10 };
const HOST_SQUEEZED = { totalBytes: 8589934592, availableBytes: 1200000000, cores: 8 };

test('resolveTier stays a PURE lookup — the hardware question is a different function', () => {
  // The split is the design. Making resolveTier(null) machine-dependent would have turned a name
  // lookup into something whose answer depends on the afternoon, and would have made the test
  // above ("the default tier is quantized turbo") either flaky or false. It is neither: quantized
  // IS the default and §10 says so. What changed is that a machine which cannot carry the default
  // is no longer handed it anyway.
  assert.equal(resolveTier(null).name, DEFAULT_TIER);
  assert.equal(resolveTier(null).hardware, undefined, 'the pure lookup answers no hardware question');
});

test('a capable machine gets the §10 model by ASKING, not by a constant — and is told nothing', () => {
  // The completion criterion for this surface: no environment variable, resolved from the host.
  const t = resolveTierForHost(null, { machine: HOST_ROOMY, speedFactor: 1.0, installed: () => true });
  assert.equal(t.model, 'large-v3-turbo-q5_0');
  assert.equal(t.name, DEFAULT_TIER);
  assert.equal(t.hardware.basis, 'top-rung');
  assert.equal(t.hardware.notice, null, 'the product working as designed is not a notification');
  // And it is nowhere near the gate: 0.0473 s of compute per second of audio is 21.1x real time.
  assert.ok(t.hardware.projected < 0.05, `projected ${t.hardware.projected}`);
});

test('a machine that cannot keep up with the recording falls to the tier nothing ever selected', () => {
  // `low-resource` has been in MODEL_TIERS since it was written and NOTHING in this product has
  // ever chosen it — a human had to pass `--tier low-resource`. That is the gap this closes.
  const t = resolveTierForHost(null, { machine: HOST_ROOMY, speedFactor: 25, installed: () => true });
  assert.equal(t.name, 'low-resource');
  assert.equal(t.model, 'small.en');
  assert.equal(t.hardware.basis, 'too-slow');
  assert.equal(t.hardware.rejected.id, 'large-v3-turbo-q5_0');
  assert.ok(t.hardware.notice && t.hardware.notice.length > 0, 'and he is told');
  assert.ok(!/small\.en|q5_0|bytes/i.test(t.hardware.notice), `no filenames or byte counts: ${t.hardware.notice}`);
});

test('a machine without the memory demotes on MEMORY, and says memory rather than speed', () => {
  const t = resolveTierForHost(null, { machine: HOST_SQUEEZED, speedFactor: 1.0, installed: () => true });
  assert.equal(t.model, 'small.en');
  assert.equal(t.hardware.basis, 'too-large');
  assert.equal(t.hardware.rejected.reason, 'memory');
  assert.equal(t.hardware.rejected.bytes, 1859256320, 'the measured 92-minute peak RSS of q5_0');
  assert.ok(/memory/i.test(t.hardware.notice), t.hardware.notice);
});

test('an UNMEASURED machine is not a slow machine — absence of evidence never demotes', () => {
  const t = resolveTierForHost(null, { machine: HOST_ROOMY, speedFactor: null, installed: () => true });
  assert.equal(t.model, 'large-v3-turbo-q5_0', 'the ceiling is where an unmeasured machine starts');
  assert.equal(t.hardware.basis, 'top-rung');
});

test('NO machine is ever promoted above the §10 ruling, however much of it there is', () => {
  // §10 decided on post-guard WER on both corpus renders, on fabrication, and on the
  // 1,050,514,080 B a new user downloads. None of those is a hardware question, so no amount of
  // hardware may reopen it. Full turbo is deliberately in NEITHER ladder.
  const huge = { totalBytes: 137438953472, availableBytes: 100000000000, cores: 24 };
  const t = resolveTierForHost(null, { machine: huge, speedFactor: 0.25, installed: () => true });
  assert.equal(t.model, MODEL_TIERS[DEFAULT_TIER].model);
  assert.notEqual(t.model, 'large-v3-turbo');
  const costs = loadCosts();
  assert.ok(!costs.batchLadder.includes('large-v3-turbo'), 'full turbo is in no ladder');
  assert.ok(!costs.batchLadder.includes('large-v3'), 'nor is full large-v3');
});

test('an explicit --tier or --model is never second-guessed by the machine', () => {
  for (const asked of ['turbo', 'max', 'low-resource', 'large-v3-turbo']) {
    const t = resolveTierForHost(asked, { machine: HOST_SQUEEZED, speedFactor: 99 });
    assert.equal(t.model, resolveTier(asked).model, `${asked} must pass straight through`);
    assert.equal(t.hardware, null, `${asked}: no hardware question was asked`);
  }
});

test('os.freemem() is NOT what this machine has available, and the gap is 16x', () => {
  // The measured reason lib/hardware.js reads vm_stat instead. On the CEO's own Mac, at one
  // moment: os.freemem() 390,676,480 B against 6,457,278,464 B genuinely available. A memory
  // guard built on the Node default would refuse every model on a healthy 24 GB machine — and it
  // is the obvious call to make. The CEO's standing rule names exactly this: never use a
  // third-party default for anything unless it has proven to be the best possible setting.
  const readable = availableMemoryBytes();
  if (readable == null) return; // not macOS, or vm_stat unavailable — skipped, never faked
  assert.ok(readable > 0);
  assert.ok(
    readable > os.freemem(),
    `available (${readable}) must exceed free (${os.freemem()}) — reclaimable memory is real`,
  );
});

test('the Node and Rust surfaces key the shared speed cache IDENTICALLY', () => {
  // They must, or each silently keeps its own cache and pays for its own calibration — which
  // would look exactly like working. hardware.rs::cache_key is `sha12|totalBytes|cores`.
  assert.equal(cacheKey('7dc20e3106d70746d61c419646d9bf87', HOST_ROOMY), '7dc20e3106d7|25769803776|10');
});

test('the ladders and gates the two surfaces walk come from ONE file', () => {
  // The same argument model-pins.json makes in its own header: two registries of truth is how the
  // second unpinned consumer came to exist. This asserts the Node reader agrees with the values
  // hardware.rs's own test asserts against the identical bytes.
  const costs = loadCosts();
  assert.deepEqual(costs.batchLadder, ['large-v3-turbo-q5_0', 'small.en']);
  assert.deepEqual(costs.liveLadder, ['large-v3-turbo-q5_0', 'small.en', 'base.en', 'tiny.en']);
  assert.equal(costs.batchRealTimeMultiple, 1);
  assert.equal(costs.liveCeilingSeconds, 1);
  assert.equal(costs.safeRung, 'small.en');
  // THE FINDING THAT REDIRECTED THE WHOLE DESIGN, pinned on this side too: the quantized turbo
  // costs LESS resident memory than the model the tier table calls the low-RAM fallback.
  const q5 = costs.models.get('large-v3-turbo-q5_0').utterancePeakRssBytes;
  const small = costs.models.get('small.en').utterancePeakRssBytes;
  assert.ok(q5 < small, `q5_0 ${q5} B must be below small.en ${small} B`);
  assert.equal(small - q5, 8945664);
});

// ---------------------------------------------------------------------------------------
group('P5 model tiering — the opt-in tiers and the pipeline-wide decode invariant');

test('the "max" opt-in tier is full large-v3 and carries NO private decode params', () => {
  const t = resolveTier('max');
  assert.equal(t.model, 'large-v3');
  assert.ok(t.repetitionGuard === true);
  // -mc 0 used to live here and ONLY here, which is how the two shipping tiers ended up decoding
  // with full context carry-over and destroying up to 44.1% of a 92-minute channel (2026-08-29).
  // It is now a pipeline-wide invariant in whisperArgs, so this tier needs nothing of its own.
  assert.deepEqual(t.decodeArgs, []);
});

test('EVERY tier decodes with no previous-text conditioning — the loop fix cannot be tier-local', () => {
  // The invariant the old per-tier decodeArg failed to hold. It is asserted for every tier that
  // exists, not just the one that happened to carry the flag.
  for (const name of [...Object.keys(MODEL_TIERS), 'large-v3', 'large-v3-turbo', 'some-future-model']) {
    const t = resolveTier(name);
    const args = whisperArgs({ extraArgs: t.decodeArgs });
    const i = args.lastIndexOf('-mc');
    assert.ok(i >= 0, `${name}: -mc must be emitted`);
    assert.equal(args[i + 1], '0', `${name}: decode context must be 0, got ${args[i + 1]}`);
  }
});

test('a raw --model large-v3 is gated by the pipeline-wide cap, not by a per-model special case', () => {
  const t = resolveTier('large-v3');
  assert.equal(t.model, 'large-v3');
  const args = whisperArgs({ extraArgs: t.decodeArgs });
  assert.equal(args[args.lastIndexOf('-mc') + 1], '0', 'bare large-v3 must not run with context carry-over');
});

test('MAX_CONTEXT_TOKENS is overridable per call and by env, so an operator is never trapped', () => {
  assert.equal(whisperArgs({ maxContext: -1 })[whisperArgs({ maxContext: -1 }).lastIndexOf('-mc') + 1], '-1');
  const prev = process.env.RICHOS_WHISPER_MAX_CONTEXT;
  process.env.RICHOS_WHISPER_MAX_CONTEXT = '16';
  try {
    const a = whisperArgs();
    assert.equal(a[a.lastIndexOf('-mc') + 1], '16');
  } finally {
    if (prev === undefined) delete process.env.RICHOS_WHISPER_MAX_CONTEXT;
    else process.env.RICHOS_WHISPER_MAX_CONTEXT = prev;
  }
});

test('flash attention is PASSED, not inherited — the vendor default is not the guarantee', () => {
  // WHY, and deliberately NOT the accuracy number this comment used to carry. The settings table
  // recorded `-nfa` costing 1.57 WER points; re-run on 2026-09-10 on a fresh corpus render it
  // nearly vanishes on q5_0 and REVERSES SIGN on full turbo, so the CEO decision page §10 rules
  // that figure unquotable. What reproduces on both models and both repetitions is that `-nfa` is
  // 12.5-20.0% SLOWER. And the reason to pass the flag never depended on either number: whisper.cpp
  // 1.9.1 happens to default it ON, which is exactly why it must not be LEFT to the default — every
  // transcript this product produces would then depend on a Homebrew formula bump nobody would
  // attribute it to.
  const args = whisperArgs();
  assert.ok(args.includes('-fa'), `-fa must be emitted, got ${args.join(' ')}`);
  // It sits BEFORE extraArgs like every other default, so a caller can still say -nfa and be obeyed.
  const withOverride = whisperArgs({ extraArgs: ['-nfa'] });
  assert.ok(withOverride.lastIndexOf('-nfa') > withOverride.lastIndexOf('-fa'));
});

test('every flag the pipeline passes is one the settings table decided — no silent additions', () => {
  // The decision table is per FLAG, so a flag appearing here that is not in it is an undecided
  // setting shipping in production. This test is the seam that makes adding one a deliberate act:
  // add the flag to `docs/measurements/whisper-settings-2026-09-10/whisper-settings-decisions.md`
  // with its evidence, then add it here.
  // Flags that consume the next token, so the walk below never mistakes a VALUE for a flag —
  // `-mc -1` is the case that would otherwise read as an undecided flag called "-1".
  const takesValue = new Set(['-l', '-t', '-mc']);
  const decided = new Set([...takesValue, '-oj', '-np', '-fa']);
  const args = whisperArgs();
  for (let i = 0; i < args.length; i += 1) {
    assert.ok(decided.has(args[i]), `${args[i]} is passed to whisper-cli but is not in the settings decision table`);
    if (takesValue.has(args[i])) i += 1;
  }
  // The restyle re-decode (stage 3.9) adds its own three, decided in the table's restyle row.
  const restyleTakesValue = new Set(['-mc', '--prompt']);
  const restyleDecided = new Set([...restyleTakesValue, '--carry-initial-prompt']);
  const extra = restyleArgs('Some preceding text.');
  for (let i = 0; i < extra.length; i += 1) {
    assert.ok(restyleDecided.has(extra[i]), `${extra[i]} is passed by the restyle re-decode but is not in the settings decision table`);
    if (restyleTakesValue.has(extra[i])) i += 1;
  }
  // And the MAIN pass never carries a prompt: the restyle budget is that decode's alone.
  assert.ok(!args.includes('--prompt') && args[args.lastIndexOf('-mc') + 1] === String(MAX_CONTEXT_TOKENS));
});

test('a tier decodeArg still wins over the default (emitted after, whisper-cli takes the last)', () => {
  const args = whisperArgs({ extraArgs: ['-mc', '224'] });
  assert.equal(args.lastIndexOf('-mc'), args.length - 2);
  assert.equal(args[args.length - 1], '224');
});

test('a raw --model large-v3-turbo stays clean (no guard decode params forced on the safe model)', () => {
  const t = resolveTier('large-v3-turbo');
  assert.deepEqual(t.decodeArgs, []);
});

test('low-resource selects small.en; the quantized tier names a quantized .bin', () => {
  assert.equal(MODEL_TIERS['low-resource'].model, 'small.en');
  assert.ok(/q\d/.test(MODEL_TIERS.quantized.model), 'quantized tier names a quantized .bin');
  assert.equal(resolveTier('quantized').model, 'large-v3-turbo-q5_0');
});

// ---------------------------------------------------------------------------------------
group('P5 repetition guard — the model-agnostic post-decode hallucination net');

// A fixture built from the REAL captured large-v3 hallucination (benchmark §4.2): the line looped 4x.
const LOOP_LINE = "I'll flag your account for the large V3 turbo batch tier, and it'll apply to you as well.";
const largeV3Hallucination = [
  { startMs: 0, endMs: 3000, text: 'Then turbo should serve you well.', speaker: 'others' },
  { startMs: 3000, endMs: 6000, text: LOOP_LINE, speaker: 'others' },
  { startMs: 6000, endMs: 9000, text: LOOP_LINE, speaker: 'others' },
  { startMs: 9000, endMs: 12000, text: LOOP_LINE, speaker: 'others' },
  { startMs: 12000, endMs: 15000, text: LOOP_LINE, speaker: 'others' },
  { startMs: 15000, endMs: 18000, text: 'apply starting with tonight\'s run.', speaker: 'others' },
];

test('the guard collapses the real large-v3 4x repetition loop to a single line', () => {
  const r = guardChannel(largeV3Hallucination);
  const loopCount = r.segments.filter((s) => s.text === LOOP_LINE).length;
  assert.equal(loopCount, 1, 'the 4 looped copies collapse to exactly one');
  assert.equal(r.removed, 3, 'three duplicate segments removed');
  assert.equal(r.loops.length, 1);
  assert.equal(r.loops[0].count, 4);
});

test('the collapsed loop keeps timing honest (first kept, end extended over the whole run)', () => {
  const r = guardChannel(largeV3Hallucination);
  const kept = r.segments.find((s) => s.text === LOOP_LINE);
  assert.equal(kept.startMs, 3000);
  assert.equal(kept.endMs, 15000, 'the kept segment spans the full looped interval (4 copies, last ends 15000)');
});

test('the guard preserves the surrounding legitimate speech byte-for-byte', () => {
  const r = guardChannel(largeV3Hallucination);
  assert.equal(r.segments[0].text, 'Then turbo should serve you well.');
  assert.equal(r.segments[r.segments.length - 1].text, "apply starting with tonight's run.");
});

test('the guard does NOT collapse short legitimate backchannels ("Yeah." "Yeah.")', () => {
  const back = [
    { startMs: 0, endMs: 500, text: 'Yeah.', speaker: 'me' },
    { startMs: 800, endMs: 1300, text: 'Yeah.', speaker: 'me' },
    { startMs: 2000, endMs: 2500, text: 'Yeah.', speaker: 'me' },
  ];
  const r = guardChannel(back);
  assert.equal(r.removed, 0, 'three short backchannels are legitimate, not a loop');
});

// ---------------------------------------------------------------------------------------
// THE PHYSICAL VETO. The TIMINGS are the real 2026-08-29 92-minute artifact — the burst offsets are
// what ffmpeg silencedetect actually measured, and the three-take structure at 5502-5536 s on the
// `others` channel is what large-v3-turbo actually emitted. The SENTENCE is invented, deliberately:
// this test asserts SHAPE (three near-identical deliveries must survive the veto), never content, so
// a synthetic line proves exactly what a real one would. The real sentence was private speech and
// had no business in a repository that gets published — see docs/briefs/README-transcription-work.md.
const RETAKE_LINE = 'Because before you commit to a rollout, you need to agree the metrics.';
const REAL_RETAKE_X3 = [
  { startMs: 5502400, endMs: 5509500, text: RETAKE_LINE, speaker: 'others' },
  { startMs: 5512300, endMs: 5517100, text: 'because before you commit to a rollout you need to agree the metrics', speaker: 'others' },
  { startMs: 5523300, endMs: 5536300, text: 'because before you commit to a rollout you need to agree the metrics', speaker: 'others' },
];
// Three separate bursts, each verified in the brief to independently decode to the whole sentence.
const REAL_RETAKE_BURSTS = [
  { startMs: 5502498, endMs: 5507510 },
  { startMs: 5512384, endMs: 5517062 },
  { startMs: 5523479, endMs: 5528128 },
];

test('the guard DELETES two genuine deliveries of a real 3x human retake when it is text-only', () => {
  // This is the defect, pinned. minRun:3 vs a human who delivers the same line three times running.
  const r = guardChannel(REAL_RETAKE_X3);
  assert.equal(r.removed, 2, 'text alone cannot tell this from a decoder loop — and it does not');
  assert.equal(r.loops.length, 1);
});

test('the physical veto PRESERVES that retake: three bursts, three deliveries, nothing removed', () => {
  const r = guardChannel(REAL_RETAKE_X3, { speechBursts: REAL_RETAKE_BURSTS });
  assert.equal(r.removed, 0, 'the audio holds a qualifying burst per repetition, so it is speech');
  assert.equal(r.loops.length, 0);
  assert.equal(r.segments.length, 3, 'all three genuine deliveries survive');
  assert.equal(r.preserved.length, 1);
  assert.equal(r.preserved[0].burstCapacity, 3);
});

test('the veto still collapses a fabrication over digital silence (no bursts, no capacity)', () => {
  // turbo emitted "Thank you." x7 at 4885-4915 s over a window measuring -51.3 dBFS. There is no
  // burst there at all, so capacity is 0 and the collapse stands.
  const line = 'Thank you very much indeed.';
  const overSilence = [0, 1, 2, 3, 4, 5, 6].map((i) => ({
    startMs: 4885200 + i * 4000, endMs: 4889200 + i * 4000, text: line, speaker: 'me',
  }));
  const r = guardChannel(overSilence, { speechBursts: [] });
  assert.equal(r.segments.filter((s) => s.text === line).length, 1, 'a loop over silence still collapses');
  assert.equal(r.removed, 6);
});

test('the veto collapses PARTIALLY when the audio holds some deliveries but not all', () => {
  // 5 emitted copies over 2 qualifying bursts -> keep 2, drop 3. Neither "delete it all" nor
  // "keep it all" is right when the audio says the truth is in between.
  // INVENTED, like every other fixture line here. The sentence that used to sit on this line was a
  // verbatim 14-word run of the private corpus this guard was measured on, and it was caught by
  // running `engine/scripts/lib/publication-boundary.py` by hand over the branch — the guard's
  // hooks snapshot at session start, so "the hook did not block it" is evidence of nothing.
  // A phrase of the same length and the same "substantial repeated line" shape proves exactly the
  // same thing about burst capacity: 15 words -> needSec 4.55 s -> a 2.73 s floor per delivery, and
  // the two bursts below clear it, so capacity is 2.
  const line = 'the runbook pointed at the wrong dashboard and cost us twenty extra minutes of downtime';
  const segs = [0, 1, 2, 3, 4].map((i) => ({
    startMs: 2142400 + i * 9600, endMs: 2152000 + i * 9600, text: line, speaker: 'others',
  }));
  const bursts = [{ startMs: 2142400, endMs: 2160000 }, { startMs: 2170000, endMs: 2190400 }];
  const r = guardChannel(segs, { speechBursts: bursts });
  assert.equal(r.segments.filter((s) => s.text === line).length, 2);
  assert.equal(r.removed, 3);
  assert.equal(r.loops[0].kept, 2);
  assert.equal(r.loops[0].burstCapacity, 2);
});

test('a SHORT repeated phrase is clamped to what the audio holds — the 2026-08-30 residual, closed', () => {
  // The SHAPE of `turbo#2` in the 72 hand-verified findings: whisper emitted a one-word phrase six
  // times at 829.9-833.9 s and the man said it TWICE. Under the old 3-word veto floor this collapsed
  // to one and destroyed a real delivery — the last surviving false positive of the eight.
  // The word below is INVENTED, like every other fixture line in this file: what the test turns on
  // is the phrase's LENGTH and the burst structure under it, never which word it was. The corpus is
  // private material and it does not get quoted here.
  const segs = [0, 1, 2, 3, 4, 5].map((i) => ({
    startMs: 829900 + i * 600, endMs: 829900 + i * 600 + 500, text: 'Anyway...', speaker: 'me',
  }));
  const bursts = [{ startMs: 829900, endMs: 831200 }, { startMs: 832100, endMs: 833900 }];
  const r = guardChannel(segs, { speechBursts: bursts });
  assert.equal(r.loops[0].burstCapacity, 2, 'the audio starts twice in this span, so it holds at most two');
  assert.equal(r.loops[0].kept, 2, 'both real deliveries survive');
  assert.equal(r.removed, 4, 'and the four the model invented do not');
});

test('a short phrase over SILENCE still collapses to one — the clamp is a ceiling, not an amnesty', () => {
  const segs = [0, 1, 2, 3, 4, 5].map((i) => ({ startMs: i * 4000, endMs: i * 4000 + 3000, text: 'Okay.', speaker: 'me' }));
  assert.equal(guardChannel(segs, { speechBursts: [] }).removed, 5, 'no bursts, no protection');
  // One burst that covers the whole span is ONE speech event, so one delivery — not six.
  const one = guardChannel(segs, { speechBursts: [{ startMs: 0, endMs: 23000 }] });
  assert.equal(one.removed, 5);
  assert.equal(one.loops[0].kept, 1);
});

test('the veto can only REFUSE a collapse, never cause one (strictly more conservative)', () => {
  // Burst capacity is a ceiling, not proof of delivery, so adding the probe must never remove more.
  // SHORT phrases are in this loop since 2026-08-30, when the veto's 3-word floor was removed: the
  // safety of that change is not a measurement, it is `keep = max(1, min(runLen, capacity)) >= 1`,
  // and this is the assertion that holds the property at every phrase length the file has.
  const shortRun = [0, 1, 2, 3, 4, 5].map((i) => ({
    startMs: i * 4000, endMs: i * 4000 + 3000, text: 'Okay.', speaker: 'me',
  }));
  const oneWordRetake = [0, 1].map((i) => ({
    startMs: 829900 + i * 600, endMs: 829900 + i * 600 + 500, text: 'Anyway...', speaker: 'me',
  }));
  for (const fixture of [largeV3Hallucination, REAL_RETAKE_X3, shortRun, oneWordRetake]) {
    for (const bursts of [[], REAL_RETAKE_BURSTS, [{ startMs: 0, endMs: 60000 }],
      shortRun.map((s) => ({ startMs: s.startMs, endMs: s.endMs }))]) {
      const withProbe = guardChannel(fixture, { speechBursts: bursts });
      const without = guardChannel(fixture);
      assert.ok(withProbe.removed <= without.removed,
        `probe removed ${withProbe.removed} > text-only ${without.removed}`);
    }
  }
});

test('guardTranscription routes each channel its OWN bursts (never the other channel\'s)', () => {
  const r = guardTranscription(
    { me: largeV3Hallucination.map((s) => ({ ...s, speaker: 'me' })), others: REAL_RETAKE_X3 },
    { speechBursts: { me: [], others: REAL_RETAKE_BURSTS } },
  );
  assert.equal(r.others.length, 3, 'the retake survives on `others` (its own bursts back it)');
  assert.equal(r.me.filter((s) => s.text === LOOP_LINE).length, 1, 'the loop still collapses on `me`');
  assert.equal(r.report.classes.preservedByAudio, 1);
});

test('parseSilenceLog turns silencedetect output into speech bursts (the complement)', () => {
  const log = [
    '  Duration: 00:00:30.00, start: 0.000000, bitrate: 256 kb/s',
    '[silencedetect @ 0x1] silence_start: 5',
    '[silencedetect @ 0x1] silence_end: 10 | silence_duration: 5',
    '[silencedetect @ 0x1] silence_start: 20',
  ].join('\n');
  const r = parseSilenceLog(log);
  assert.equal(r.durationSec, 30);
  assert.deepEqual(r.speech, [{ startMs: 0, endMs: 5000 }, { startMs: 10000, endMs: 20000 }]);
  // a trailing silence_start with no silence_end runs to the end of the file, not to infinity
  assert.deepEqual(r.silence[r.silence.length - 1], { startMs: 20000, endMs: 30000 });
});

test('burstCapacity counts only bursts long enough to hold the phrase, clipped to the span', () => {
  const bursts = [{ startMs: 0, endMs: 5000 }, { startMs: 6000, endMs: 6500 }, { startMs: 8000, endMs: 13000 }];
  assert.equal(burstCapacity(bursts, 0, 20000, 3, 1), 2, '0.5 s burst cannot hold a 3 s phrase');
  assert.equal(burstCapacity(bursts, 0, 20000, 10, 1), 0, 'nothing holds a 10 s phrase');
  assert.equal(burstCapacity(bursts, 0, 5000, 3, 1), 1, 'bursts outside the span do not count');
  assert.equal(burstCapacity(null, 0, 20000, 1, 1), 0, 'no probe means no capacity, never a veto');
});

test('the guard does not touch a clean transcript (zero false positives on turbo output)', () => {
  const clean = [
    { startMs: 0, endMs: 2000, text: 'Hi Marcus, thanks for joining.', speaker: 'me' },
    { startMs: 2000, endMs: 5000, text: 'Happy to be on. Let us get started.', speaker: 'others' },
  ];
  const r = guardTranscription({ me: [clean[0]], others: [clean[1]] });
  assert.equal(r.report.removed, 0);
  assert.equal(r.report.detected, false);
});

test('guardTranscription reports per-channel loop findings for verification.json', () => {
  const r = guardTranscription({ me: [], others: largeV3Hallucination });
  assert.equal(r.report.detected, true);
  assert.equal(r.report.removed, 3);
  assert.equal(r.report.byChannel.others.loops, 1);
  assert.equal(r.report.loops[0].channel, 'others');
});

// ---------------------------------------------------------------------------------------
group('P5 hallucination guard, class 2 — PERSISTENT INSERTION (the real large-v3-turbo artifact)');

// Every fixture in this group is verbatim captured whisper.cpp output; see
// test/fixtures/captured-hallucinations.js for the sha256 of each source JSON.
const flat = (segs) => segs.map((s) => s.text).join('');

test('readEnumerationMarker reads a bare segment-initial ordinal and leaves real numbers alone', () => {
  assert.equal(readEnumerationMarker(' 12. Two small things.').value, 12);
  assert.equal(readEnumerationMarker(' 3) The runbook now links the dashboard.').value, 3);
  assert.equal(readEnumerationMarker(' 3.0.2 shipped last week.'), null, 'a version number is not a marker');
  assert.equal(readEnumerationMarker(' 9 cents. That tier starts at 25,000.'), null, 'no separator, no marker');
  assert.equal(readEnumerationMarker(' 21%. Yes.'), null, 'a percentage is not a marker');
  assert.equal(readEnumerationMarker(' 2.7.4'), null);
});

test('the guard DETECTS the real 59-marker turbo fabrication the 2026-08-26 benchmark found blind', () => {
  const r = guardInsertions(TURBO_NUMERAL_INSERTION);
  assert.equal(r.insertions.length, 1, 'the fabricated span is reported as one finding');
  assert.equal(r.stats.markerSegments, 59, '59 of 88 segments carry a leading ordinal (brief §6.3)');
  assert.equal(r.stats.channelSegments, 88);
  assert.ok(r.stats.spanDensity > 0.9, `markers dominate their own span (${r.stats.spanDensity})`);
  assert.equal(r.stats.maxRepeat, 13, 'the fabricated ordinal stalls on "12." for 13 consecutive segments');
  assert.equal(r.insertions[0].startMs, 171500);
  assert.equal(r.insertions[0].endMs, 615140);
});

test('the fabricated span is bounded by the FIRST well-formedness break, so the real "1." "3." list survives', () => {
  const r = guardInsertions(TURBO_NUMERAL_INSERTION);
  const f = r.insertions[0];
  assert.deepEqual(f.genuinePrefix, ['1.', '3.'], 'the speaker really did enumerate two items here');
  assert.equal(f.count, 57, '57 of the 59 markers are judged fabricated, not all 59');
  assert.equal(f.firstIndex, 20, 'the fabrication is dated from the first stall, not from the first marker');
});

test('detection NEVER rewrites the transcript — not one character of the 88 segments moves', () => {
  const r = guardInsertions(TURBO_NUMERAL_INSERTION);
  assert.equal(flat(r.segments), flat(TURBO_NUMERAL_INSERTION));
  assert.equal(r.stripped, 0);
});

// This test exists to DOCUMENT why the remedy is detect-only, not to endorse the opt-in.
test('opting into stripping would delete a real word: the speaker\'s "Zero." at 205.3 s', () => {
  const before = TURBO_NUMERAL_INSERTION.find((s) => s.startMs === 205260);
  assert.ok(before.text.startsWith(' 0. We ran a full checksum comparison'), 'the captured segment');
  const r = guardInsertions(TURBO_NUMERAL_INSERTION, { stripInsertions: true });
  assert.equal(r.stripped, 57);
  const after = r.segments.find((s) => s.startMs === 205260);
  assert.ok(!/\b0\b/.test(after.text.slice(0, 6)), 'the marker is gone');
  // The reference says: "Any data loss?" / "Zero. We ran a full checksum comparison..."
  assert.ok(after.text.trim().startsWith('We ran a full checksum'), 'and so is the answer the speaker gave');
});

test('POSITIVE CONTROL for the negatives below: the same detector fires on the artifact', () => {
  assert.equal(guardInsertions(TURBO_NUMERAL_INSERTION).insertions.length, 1);
});

// ---- class 2, the REPAIR (2026-08-30) ---------------------------------------------------------
// The probe is DATA in these tests, never a call: the fixture holds the real isolated re-decodes.

/** The recorded isolated decodes, in the order `guardInsertions` probes its suspect markers. */
const isolatedProbe = () => (spans) =>
  spans.map((s) => {
    const seg = TURBO_NUMERAL_INSERTION.findIndex((x) => x.startMs === s.startMs && x.endMs === s.endMs);
    const row = TURBO_NUMERAL_INSERTION_ISOLATED_DECODES.find((x) => x.index === seg);
    assert.ok(row, `no recorded isolated decode for segment ${seg}`);
    return { tight: row.tight, wide: row.wide };
  });

test('numeralInText reads a numeral in EITHER form, anywhere, and says "empty" rather than "absent"', () => {
  assert.equal(numeralInText('Three, we added a synthetic canary job', 3), true, 'the word form');
  assert.equal(numeralInText(' 3. We added a synthetic canary job', 3), true, 'the digit form');
  assert.equal(numeralInText('my worry the whole time zero we ran a full checksum', 0), true, 'seven words in');
  assert.equal(numeralInText('It pages after two consecutive failures', 3), false);
  assert.equal(numeralInText('   ', 7), null, 'an empty decode is absence of evidence');
  // Deliberately generous, and it can only ever PROTECT: "3.1.0" counts as a 3, so a marker whose
  // segment happens to quote a version number is kept rather than stripped. Zero of 57 on the real
  // artifact — a false negative here leaves fabrication in and says so; the other error deletes a word.
  assert.equal(numeralInText('build 3.1.0 ships next month', 3), true);
});

test('a marker is only ever stripped on POSITIVE, agreeing evidence from both paddings', () => {
  const m = { value: 7 };
  assert.equal(adjudicateInsertionMarker(m, { tight: 'no numeral here', wide: 'nor here' }).verdict, 'fabricated');
  assert.equal(adjudicateInsertionMarker(m, { tight: 'seven of them', wide: 'nor here' }).verdict, 'spoken');
  assert.equal(adjudicateInsertionMarker(m, { tight: 'no numeral here', wide: '7 of them' }).verdict, 'spoken');
  assert.equal(adjudicateInsertionMarker(m, { tight: '', wide: 'nothing' }).verdict, 'unprobed');
  assert.equal(adjudicateInsertionMarker(m, null).verdict, 'unprobed');
});

test('THE REPAIR, on the real artifact and its real isolated decodes: 56 of 57 stripped, the spoken "Zero." kept', () => {
  const r = guardInsertions(TURBO_NUMERAL_INSERTION, { probe: isolatedProbe() });
  const f = r.insertions[0];
  assert.equal(f.count, 57, 'the same 57 suspect markers as before');
  assert.equal(r.stripped, 56, '56 markers the audio never carried');
  assert.equal(r.kept, 1, 'and exactly one it did');
  assert.equal(r.unprobed, 0, 'every marker was adjudicated');
  // The whole reason this class was detect-only for two days.
  const zero = r.segments.find((s) => s.startMs === 205260);
  assert.ok(zero.text.trim().startsWith('0. We ran a full checksum'), 'the speaker\'s "Zero." is still there');
  const spoken = f.adjudicated.find((a) => a.verdict === 'spoken');
  assert.equal(spoken.index, 24);
});

test('the repair takes the marker and NOTHING else — every other word of all 88 segments is untouched', () => {
  const r = guardInsertions(TURBO_NUMERAL_INSERTION, { probe: isolatedProbe() });
  const strip = (t) => String(t).replace(/^\s*\d{1,3}\s*[.)]\s+/, ' ').replace(/\s+/g, ' ').trim();
  assert.equal(r.segments.length, TURBO_NUMERAL_INSERTION.length);
  TURBO_NUMERAL_INSERTION.forEach((before, i) => {
    assert.equal(strip(r.segments[i].text), strip(before.text), `segment ${i} lost or gained words`);
    assert.equal(r.segments[i].startMs, before.startMs);
    assert.equal(r.segments[i].endMs, before.endMs);
  });
});

test('the repair is one-directional: a probe can refuse a strip, never cause one', () => {
  // Whatever the probe says, the set of markers it can touch is the same suspect set text alone
  // already chose, and any numeral it recovers only ever SUBTRACTS from what gets stripped.
  const all = guardInsertions(TURBO_NUMERAL_INSERTION, { stripInsertions: true }).stripped;
  for (const probe of [
    () => (spans) => spans.map(() => ({ tight: 'nothing at all', wide: 'nothing at all' })),
    () => (spans) => spans.map(() => ({ tight: 'one two three four five six seven eight nine ten eleven twelve thirteen fourteen', wide: 'same' })),
    isolatedProbe,
  ]) {
    const r = guardInsertions(TURBO_NUMERAL_INSERTION, { probe: probe() });
    assert.ok(r.stripped <= all, 'the probe never strips more than the unadjudicated strip would');
  }
});

test('NO PROBE, NO REPAIR: without an isolated re-decode the class is byte-for-byte detect-only', () => {
  const r = guardInsertions(TURBO_NUMERAL_INSERTION);
  assert.equal(flat(r.segments), flat(TURBO_NUMERAL_INSERTION));
  assert.equal(r.stripped, 0);
  assert.equal(r.insertions[0].repaired, false, 'and the report says which of "clean" and "never looked" this is');
});

test('a probe that returns nothing, or throws, leaves the text alone and reports it unadjudicated', () => {
  const empty = guardInsertions(TURBO_NUMERAL_INSERTION, { probe: (spans) => spans.map(() => ({ tight: '', wide: '' })) });
  assert.equal(empty.stripped, 0, 'an empty decode is not evidence of fabrication');
  assert.equal(empty.unprobed, 57);
  assert.equal(flat(empty.segments), flat(TURBO_NUMERAL_INSERTION));
  const threw = guardInsertions(TURBO_NUMERAL_INSERTION, { probe: () => { throw new Error('whisper died'); } });
  assert.equal(threw.stripped, 0, 'a failed probe must never look like a clean result');
  assert.equal(threw.unprobed, 57);
  assert.equal(flat(threw.segments), flat(TURBO_NUMERAL_INSERTION));
});

test('the probe BUDGET caps the decodes and the markers past it stay in the text, named', () => {
  const { probe, unprobed } = selectInsertionProbes(new Array(120).fill(0).map((_, i) => ({ index: i, value: 1 })));
  assert.equal(probe.length, REPETITION_GUARD_DEFAULTS.insertionProbeBudget);
  assert.equal(unprobed.length, 120 - REPETITION_GUARD_DEFAULTS.insertionProbeBudget);
  const r = guardInsertions(TURBO_NUMERAL_INSERTION, { probe: isolatedProbe(), insertionProbeBudget: 10 });
  assert.equal(r.probed, 10, 'ten markers got a verdict');
  assert.equal(r.unprobed, 47, 'and the rest are named as unadjudicated, not as clean');
  assert.equal(r.stripped, 9, 'only what was actually looked at');
  assert.equal(r.kept, 1, 'and the real "Zero." is the fifth of them, so the budget still spares it');
});

test('the genuine spoken enumeration is never probed at all — it is not in the suspect set', () => {
  let calls = 0;
  const r = guardInsertions(TURBO_NUMERAL_INSERTION, {
    probe: (spans) => { calls += spans.length; return spans.map(() => ({ tight: 'x', wide: 'x' })); },
  });
  assert.equal(calls, 57, 'the two real "1." "3." list markers are not among them');
  assert.deepEqual(r.insertions[0].genuinePrefix, ['1.', '3.']);
});

test('SILENT on real speech carrying the same surface feature — a genuinely spoken "1. 2. 3." list', () => {
  // large-v3-turbo on the CLEAN sample B: " 3. We added a synthetic canary job…" is a legitimate
  // segment-initial ordinal, byte-identical in surface form to the fabricated ones.
  assert.ok(
    GENUINE_SPOKEN_ENUMERATION.some((s) => /^\s*3\.\s/.test(s.text)),
    'the control really does contain the surface feature',
  );
  const r = guardInsertions(GENUINE_SPOKEN_ENUMERATION);
  assert.equal(r.insertions.length, 0);
  assert.equal(flat(r.segments), flat(GENUINE_SPOKEN_ENUMERATION));
});

test('SILENT on the q5_0 transcript of the identical audio that produced the artifact', () => {
  const r = guardInsertions(Q5_SAME_AUDIO_CLEAN);
  assert.equal(r.insertions.length, 0);
  assert.equal(r.stats.markerSegments, 0);
});

test('SILENT on a long, well-formed spoken enumeration — counting up, each number used once', () => {
  // CONSTRUCTED control (no captured instance exists): a person reading a 12-item numbered agenda,
  // 100% marker density. Well-formed, so the guard must not touch it however dense it gets.
  const agenda = Array.from({ length: 12 }, (_, i) => ({
    startMs: i * 5000,
    endMs: i * 5000 + 4000,
    text: ` ${i + 1}. Agenda item number ${i + 1}, which we should cover today.`,
    speaker: 'me',
  }));
  const r = guardInsertions(agenda);
  assert.equal(r.stats.markerSegments, 12);
  assert.equal(r.stats.spanDensity, 1, 'maximum possible density');
  assert.equal(r.insertions.length, 0, 'density alone never fires — well-formedness is the discriminator');
});

test('SILENT on a spoken enumeration with ONE hiccup (a speaker repeating or going back a number)', () => {
  // CONSTRUCTED control: real speakers do lose their place once. One violation is not a fabrication.
  const list = [1, 2, 3, 3, 4, 5, 6, 7, 8, 9].map((v, i) => ({
    startMs: i * 5000,
    endMs: i * 5000 + 4000,
    text: ` ${v}. The next thing on the list that I wanted to raise with you.`,
    speaker: 'me',
  }));
  const r = guardInsertions(list);
  assert.equal(r.stats.violations, 1);
  assert.equal(r.insertions.length, 0, 'one stall is a person, thirteen is a decoder');
});

test('a segment that is ONLY a numeral is a person counting, never a prefix insertion', () => {
  const counting = Array.from({ length: 10 }, (_, i) => ({
    startMs: i * 2000,
    endMs: i * 2000 + 1500,
    text: ` ${(i % 3) + 1}.`,
    speaker: 'me',
  }));
  const r = guardInsertions(counting);
  assert.equal(r.stats.markerSegments, 0, 'nothing was prefixed ONTO anything');
  assert.equal(r.insertions.length, 0);
});

// ---------------------------------------------------------------------------------------
group('P5 hallucination guard, class 3 — SLIDING-OVERLAP STUTTER (the real large-v3 artifact)');

test('the guard DETECTS the real large-v3 sliding stutter the loop detector could only see 6 of', () => {
  const r = guardOverlapStutter(LARGE_V3_SLIDING_STUTTER);
  assert.ok(r.stutters.length >= 1, 'the stutter is reported');
  const chain = r.stutters[0];
  assert.ok(chain.links >= 30, `a long chain of overlapping boundaries, not one restart (${chain.links})`);
  assert.ok(chain.maxOverlapWords >= 7, 'whole phrases re-emitted, not a shared article');
  assert.equal(chain.kind, 'sliding-overlap');
});

test('the de-overlap is CONTENT-PRESERVING — every distinct word survives, just once', () => {
  const r = guardOverlapStutter(LARGE_V3_SLIDING_STUTTER);
  const words = (segs) =>
    segs
      .map((s) => s.text.toLowerCase())
      .join(' ')
      .match(/[a-z0-9']+/g) || [];
  const before = new Set(words(LARGE_V3_SLIDING_STUTTER));
  const after = new Set(words(r.segments));
  const lost = [...before].filter((w) => !after.has(w));
  assert.deepEqual(lost, [], 'no word present in the artifact is missing from the repair');
  assert.ok(words(r.segments).length < words(LARGE_V3_SLIDING_STUTTER).length * 0.65, 'the doubling is gone');
});

test('the repaired stream reads once, not two or three times (the captured phrase, verbatim)', () => {
  const r = guardOverlapStutter(LARGE_V3_SLIDING_STUTTER);
  const raw = flat(LARGE_V3_SLIDING_STUTTER);
  const fixed = flat(r.segments);
  const phrase = 'the cloud vendors sell as a premium';
  const count = (h, n) => h.split(n).length - 1;
  assert.equal(count(raw, phrase), 3, 'the artifact emits it three times');
  assert.equal(count(fixed, phrase), 1, 'the repair emits it once');
});

test('dropping a re-emitted segment folds its time into the kept one (timing/coverage stay honest)', () => {
  const r = guardOverlapStutter(LARGE_V3_SLIDING_STUTTER);
  const inSpan = (segs) => Math.max(...segs.map((s) => s.endMs)) - Math.min(...segs.map((s) => s.startMs));
  assert.equal(inSpan(r.segments), inSpan(LARGE_V3_SLIDING_STUTTER), 'the channel still spans the same audio');
});

test('SILENT on real clean speech — 18 captured clean transcripts share ZERO boundary words', () => {
  const r = guardOverlapStutter(CLEAN_NO_BOUNDARY_OVERLAP);
  assert.equal(r.stutters.length, 0);
  assert.equal(r.removed, 0);
  assert.equal(flat(r.segments), flat(CLEAN_NO_BOUNDARY_OVERLAP));
});

test('SILENT on a speaker genuinely repeating a phrase across ONE segment boundary', () => {
  // CONSTRUCTED control (the captured corpus contains no such case): a real restart is one link.
  const restart = [
    { startMs: 0, endMs: 3000, text: ' so what I would suggest is we push the migration', speaker: 'me' },
    { startMs: 3000, endMs: 6000, text: ' we push the migration to the following Tuesday', speaker: 'me' },
    { startMs: 6000, endMs: 9000, text: ' and tell procurement on the Monday.', speaker: 'me' },
  ];
  const r = guardOverlapStutter(restart);
  assert.equal(r.stutters.length, 0, 'one overlapping boundary is a person, not a decoder');
  assert.equal(flat(r.segments), flat(restart));
});

test('SILENT at two consecutive overlapping boundaries — the chain floor is three', () => {
  // CONSTRUCTED control: the deliberate precision/recall line. A 2-link stutter goes undetected;
  // that is a stated blind spot, chosen over risking a real speaker's repeated phrase.
  const two = [
    { startMs: 0, endMs: 3000, text: ' I said we would send the deck on Thursday', speaker: 'me' },
    { startMs: 3000, endMs: 6000, text: ' send the deck on Thursday and the tier table', speaker: 'me' },
    { startMs: 6000, endMs: 9000, text: ' and the tier table with it', speaker: 'me' },
  ];
  const r = guardOverlapStutter(two);
  assert.equal(r.stutters.length, 0);
  assert.equal(flat(r.segments), flat(two));
});

test('collapseStutter:false keeps detection while leaving the text untouched', () => {
  const r = guardOverlapStutter(LARGE_V3_SLIDING_STUTTER, { collapseStutter: false });
  assert.ok(r.stutters.length >= 1);
  assert.equal(r.removed, 0);
  assert.equal(flat(r.segments), flat(LARGE_V3_SLIDING_STUTTER));
});

// ---------------------------------------------------------------------------------------
group('P5 hallucination guard, class 4 — SILENCE FABRICATION');

// THE SHAPE, from the measurement. On a 126-minute per-speaker track — the shape of the `me`
// channel of any real call, silent ~90% of the time — large-v3-turbo at `-mc 0` emitted 159 of its
// 353 segments over measured silence, covering 60.4% of the timeline, 143 of them "Thank you.".
// Almost every one is a FULL 30-SECOND WHISPER WINDOW carrying 1-3 words, which is the shape below.
//
// THE TEXT IS NOT REDACTED AND DOES NOT NEED TO BE. "Thank you." over measured silence is whisper's
// canonical filler; isolated re-decode proved nobody said it. It is the only text in this file that
// is quoted from the corpus rather than invented, and it is quotable precisely because it is not
// speech. Every line here that stands for REAL speech is invented, like the rest of the file.
const SILSEG = (startMs, endMs, text, wordTimesMs) => ({ startMs, endMs, text, speaker: 'me', ...(wordTimesMs ? { wordTimesMs } : {}) });
const SILB = (startMs, endMs) => ({ startMs, endMs });

test('burstOverlapMs measures speech energy inside a span, and stops at the span', () => {
  const bursts = [SILB(0, 1000), SILB(5000, 6000), SILB(20000, 30000)];
  assert.equal(burstOverlapMs(bursts, 2000, 7000), 1000);
  assert.equal(burstOverlapMs(bursts, 2000, 4000), 0);
  assert.equal(burstOverlapMs(bursts, 25000, 40000), 5000, 'a partially overlapping burst counts its overlap only');
  assert.equal(burstOverlapMs(null, 0, 10000), 0, 'no grid is no energy, and the caller must not act on it');
});

test('isSilenceFillerText reads the vocabulary PER SENTENCE — the bug that inverted an adjudication', () => {
  assert.equal(isSilenceFillerText('Thank you.'), true);
  assert.equal(isSilenceFillerText('Thank you. Thank you.'), true, 'the count of repeats must not matter');
  assert.equal(isSilenceFillerText('Hmm. Hmm.'), true);
  assert.equal(isSilenceFillerText('[BLANK_AUDIO]'), true);
  assert.equal(isSilenceFillerText('-'), true);
  // and it stays conservative: ONE real unit and the whole thing is real
  assert.equal(isSilenceFillerText('Thank you. And then we agreed the metrics.'), false);
  assert.equal(isSilenceFillerText("That's a tough one."), false);
  assert.equal(isSilenceFillerText('the migration is finished'), false);
});

test('NO BURST GRID -> class 4 never ran. "Never looked" is not "nothing found"', () => {
  const segs = [SILSEG(219680, 249660, 'Thank you.', [219700, 219900])];
  const r = guardSilenceFabrication(segs, {});
  assert.equal(r.probed, false, 'without the physical probe this class must be inert');
  assert.equal(r.removed, 0);
  assert.equal(r.fabrications.length, 0);
  assert.deepEqual(r.segments, segs, 'and the channel is returned untouched');
});

test('THE DEFECT: a 30 s window of silence carrying "Thank you." is removed', () => {
  // 219.68-249.66 s on the 001 host channel: 29.98 s, 2 words, zero speech energy in the extent.
  const segs = [
    SILSEG(200000, 205000, 'and how did that land with the board', [200200, 200800, 201400, 202000, 202600, 203200, 203800]),
    SILSEG(219680, 249660, 'Thank you.', [219700, 219900]),
  ];
  const bursts = [SILB(200100, 204200)];
  const r = guardSilenceFabrication(segs, { speechBursts: bursts });
  assert.equal(r.probed, true);
  assert.equal(r.removed, 1);
  assert.equal(r.segments.length, 1);
  assert.equal(r.segments[0].startMs, 200000, 'the real clause beside it survives untouched');
  assert.equal(r.fabrications[0].action, 'removed');
  assert.equal(r.fabrications[0].durationSec, 29.98);
  assert.equal(r.fabrications[0].burstOverlapSec, 0);
  assert.equal(r.fabrications[0].unit, 'word-times');
});

test('condition 5, THE STRETCHED EXTENT: a real backchannel inside a long extent is NOT removed', () => {
  // Measured on the 001 host channel: 41 segments whose extent reaches tens of seconds back across
  // silence while the word was physically spoken inside one short burst. Judging that on the extent
  // alone deletes a word the person really said. The word time is the veto.
  const segs = [SILSEG(279680, 289040, 'Hmm.', [285000])];
  const r = guardSilenceFabrication(segs, { speechBursts: [SILB(284800, 285300)] });
  assert.equal(r.removed, 0);
  assert.equal(r.fabrications.length, 0, 'a word inside a burst is not a candidate at all');
  assert.deepEqual(r.segments, segs);
});

test('condition 6, THE VOCABULARY TIER: unfamiliar text over silence is REPORTED, never removed', () => {
  // A guest channel really produced this over a span the burst grid calls silent. It might be a
  // quiet real answer the channel-relative floor missed, and this guard does not guess.
  const segs = [SILSEG(4403720, 4405700, "That's a tough one.", [4403800, 4404100, 4404400, 4404700])];
  const r = guardSilenceFabrication(segs, { speechBursts: [SILB(0, 1000)] });
  assert.equal(r.removed, 0);
  assert.equal(r.reported, 1);
  assert.equal(r.fabrications[0].action, 'reported');
  assert.equal(r.segments.length, 1, 'and it is STILL IN the transcript');
});

test('condition 3, THE BREVITY CAP: this class can never remove a sentence, whatever else fails', () => {
  const long = 'okay okay okay okay okay okay okay okay';
  const segs = [SILSEG(0, 30000, long, [100, 200, 300, 400, 500, 600, 700, 800])];
  const r = guardSilenceFabrication(segs, { speechBursts: [] });
  assert.equal(r.removed, 0, '8 words is over the cap even though every one of them is filler');
  assert.equal(r.fabrications.length, 0);
});

test('condition 2, THE DURATION FLOOR: the sub-second band is out of scope, and stays in', () => {
  const segs = [SILSEG(6213600, 6214100, 'Hmm.', [6213600])];
  const r = guardSilenceFabrication(segs, { speechBursts: [] });
  assert.equal(r.removed, 0);
  assert.equal(r.segments.length, 1);
});

test('condition 4, BOTH HALVES: an absolute cap and a fraction, because either alone fails', () => {
  // (a) ONLY THE FRACTION SEES THIS. 0.08 s of energy is under the 0.10 s absolute cap, but inside
  //     a 1.2 s backchannel it is 6.7% of the span — the absolute cap alone would have removed it.
  const shortSeg = [SILSEG(0, 1200, 'Yeah.', [50])];
  assert.equal(guardSilenceFabrication(shortSeg, { speechBursts: [SILB(600, 680)] }).removed, 0);
  // (b) ONLY THE ABSOLUTE CAP SEES THIS. 0.5 s of energy inside a 30 s extent is 1.67% — under the
  //     2% fraction, which alone would have removed a span holding half a second of real speech.
  const longSeg = [SILSEG(0, 30000, 'Thank you.', [100, 300])];
  assert.equal(guardSilenceFabrication(longSeg, { speechBursts: [SILB(20000, 20500)] }).removed, 0);
  // and with the energy gone, the same segment IS removed
  assert.equal(guardSilenceFabrication(longSeg, { speechBursts: [SILB(20000, 20050)] }).removed, 1);
});

test('THE MEASURED LOSS, pinned: 0.2 s of energy in the extent means KEEP, not remove', () => {
  // The one place this class was measured destroying a real word: a genuine one-word backchannel
  // hum at -35.0 / -34.7 dBFS — the host's own speech MEDIAN — whose energy fell just under the
  // burst floor, leaving ~0.2 s inside a 20-25 s extent. Both decoders recovered "Mm-hmm" from
  // those spans in isolation. `maxSilenceOverlapSec` was swept over adjudicated spans and set to
  // 0.10 s for the margin; this test is the frontier, so nobody loosens it back by accident.
  const hum = [SILSEG(1193100, 1218660, 'Hmm.', [1193120])];
  assert.equal(guardSilenceFabrication(hum, { speechBursts: [SILB(1200000, 1200200)] }).removed, 0);
  assert.equal(REPETITION_GUARD_DEFAULTS.maxSilenceOverlapSec, 0.1, 'swept, not chosen — see the module header');
});

test('ONE-DIRECTIONAL: a segment the audio backs is left completely alone', () => {
  const segs = [SILSEG(0, 3000, 'Thank you.', [500, 800])];
  const r = guardSilenceFabrication(segs, { speechBursts: [SILB(400, 2900)] });
  assert.equal(r.removed, 0);
  assert.equal(r.fabrications.length, 0);
  assert.deepEqual(r.segments, segs, 'the grid can condemn silence, never speech');
});

test('ORDER: 143 copies of the filler over silence are class 4, not a repetition finding', () => {
  // Class 4 runs FIRST on purpose. Collapsing them as a loop would keep one and EXTEND its end
  // across the silence — destroying the very evidence this class reads — and would leave a
  // fabricated line in the transcript besides.
  const segs = [];
  for (let i = 0; i < 6; i += 1) segs.push(SILSEG(i * 30000, i * 30000 + 29980, 'Thank you.', [i * 30000 + 20]));
  const r = guardChannelAll(segs, { speechBursts: [] });
  assert.equal(r.silenceRemoved, 6);
  assert.equal(r.loops.length, 0, 'nothing is left for the loop class to find');
  assert.equal(r.segments.length, 0);
  assert.equal(r.removed, 6, 'and the total removed count carries class 4');
});

test('the guest-channel probe: genuine speech with genuine repetition is byte-identical after class 4', () => {
  // The powered false-positive test in miniature. On the real corpus this is 28,275 guest words
  // containing 217 immediate repeats and 23 same-word triples, of which class 4 touched 3 segments,
  // every one independently adjudicated as fabrication.
  const segs = [
    SILSEG(0, 2000, 'no no no that is not what the contract says', [100, 400, 700, 1000, 1200, 1400, 1600, 1800, 1900]),
    SILSEG(2000, 4000, "it's it's the renewal clause", [2100, 2400, 2700, 3000, 3400]),
    SILSEG(4000, 6000, 'Yeah.', [4100]),
    SILSEG(6000, 9000, 'right right right', [6100, 6600, 7100]),
  ];
  const bursts = [SILB(0, 4000), SILB(4050, 4400), SILB(6000, 8500)];
  const r = guardSilenceFabrication(segs, { speechBursts: bursts });
  assert.equal(r.removed, 0);
  assert.equal(r.reported, 0);
  assert.deepEqual(r.segments, segs);
});

test('THE TWO GUARDS DO NOT FIGHT: class 4 cannot add a deletion candidate under word times', () => {
  // Condition 5 means every removed word was already outside every burst, so no burst loses
  // coverage when the segment goes. Asserted rather than argued.
  const segs = [
    SILSEG(0, 4000, 'the rollout starts on monday morning', [100, 700, 1300, 1900, 2500, 3100]),
    SILSEG(10000, 39980, 'Thank you.', [10020, 10200]),
  ];
  const bursts = [SILB(0, 3800), SILB(45000, 47500)];
  const before = findDeletionCandidates(segs, bursts, { channel: 'me' });
  const guarded = guardSilenceFabrication(segs, { speechBursts: bursts });
  const after = findDeletionCandidates(guarded.segments, bursts, { channel: 'me' });
  assert.equal(guarded.removed, 1);
  assert.equal(before.coverageUnit, 'word-times');
  assert.equal(after.candidates.length, before.candidates.length);
  assert.deepEqual(after.candidates.map((c) => c.startMs), before.candidates.map((c) => c.startMs));
});

test('the known-silence span stays silent: class 4 removes the filler, the deletion guard still says NOT-SPEECH', () => {
  // 4885.2 s of the 92-minute artifact: max -51.3 dBFS against a -0.9 dBFS peak, whisper emitting
  // "Thank you." seven times over it. Neither guard may turn that window into a finding about speech.
  const segs = [SILSEG(4885200, 4915180, 'Thank you.', [4885220, 4885400])];
  const r = guardSilenceFabrication(segs, { speechBursts: [SILB(0, 2000)] });
  assert.equal(r.removed, 1, 'the fabricated words go');
  const v = adjudicateCandidate(
    { channel: 'me', index: 0, startMs: 4885200, endMs: 4915180, durationSec: 29.98, nearbyText: '' },
    { tight: 'Thank you.', wide: 'Thank you. Thank you.', maxDb: -51.3, meanDb: -62 },
    { peakDb: -0.9 },
  );
  assert.equal(v.verdict, 'not-speech', 'and the span is still not claimed as deleted speech');
});

// ---------------------------------------------------------------------------------------
group('P5 hallucination guard — all four classes through the one pipeline seam');

// The 2026-08-30 end-to-end pass found class 3 undoing class 1's veto: K identical consecutive
// segments are, to a word-exact boundary rule, a stutter chain, so the copies the burst grid had
// just protected were collapsed one stage later. It bit at K >= 4 (3 copies make only 2 links, and
// minChainLinks is 3), which is why the shipping world's one genuine 3x retake never showed it.
const FOUR_TAKES = [0, 1, 2, 3].map((i) => ({
  // INVENTED, like every fixture line here: what this turns on is four identical consecutive
  // segments over four qualifying bursts, never which sentence it was.
  startMs: 100000 + i * 10000,
  endMs: 100000 + i * 10000 + 8000,
  text: 'the runbook pointed at the wrong dashboard and cost us twenty extra minutes of downtime',
  speaker: 'me',
}));
const FOUR_BURSTS = FOUR_TAKES.map((s) => ({ startMs: s.startMs, endMs: s.endMs }));

test('class 3 does NOT collapse the deliveries class 1 preserved on the audio', () => {
  const kept = guardChannelAll(FOUR_TAKES, { speechBursts: FOUR_BURSTS });
  assert.equal(kept.preserved.length, 1, 'the burst grid holds all four, so class 1 preserves them');
  assert.equal(kept.segments.length, 4, 'and all four are still in the transcript afterwards');
  assert.equal(kept.stutterRemoved, 0);
  assert.equal(kept.removed, 0);
});

test('a CLAMPED run is handed over too, not only a fully preserved one', () => {
  // The shape that cost four deliveries: 7 emitted copies over 5 qualifying bursts. Class 1 clamps
  // to 5 — a partial collapse, so the run is reported under `loops` and not under `preserved`, and
  // a hand-off that only covered `preserved` would leave class 3 free to take the other four.
  const line = FOUR_TAKES[0].text;
  const segs = [0, 1, 2, 3, 4, 5, 6].map((i) => ({
    startMs: 200000 + i * 10000, endMs: 200000 + i * 10000 + 8000, text: line, speaker: 'others',
  }));
  const bursts = [0, 1, 2, 3, 4].map((i) => ({ startMs: 200000 + i * 10000, endMs: 200000 + i * 10000 + 8000 }));
  const r = guardChannelAll(segs, { speechBursts: bursts });
  assert.equal(r.loops.length, 1);
  assert.equal(r.loops[0].kept, 5, 'class 1 clamps to what the audio holds');
  assert.equal(r.preserved.length, 0, 'and reports it as a collapse, not as a preservation');
  assert.equal(r.stutterRemoved, 0, 'class 3 must not then take the other four');
  assert.equal(r.segments.filter((x) => x.text === line).length, 5);
});

test('the hand-off is scoped to the protected span — a real stutter outside one is still caught', () => {
  const before = guardOverlapStutter(LARGE_V3_SLIDING_STUTTER);
  const after = guardOverlapStutter(LARGE_V3_SLIDING_STUTTER, {
    protectedSpans: [{ startMs: 0, endMs: 1 }],
  });
  assert.ok(before.removed > 0, 'the captured stutter is caught');
  assert.equal(after.removed, before.removed, 'and an unrelated protected span changes nothing');
  assert.equal(after.stutters.length, before.stutters.length);
});

test('without the burst grid class 1 keeps one copy, so class 3 has nothing left to eat', () => {
  const textOnly = guardChannelAll(FOUR_TAKES);
  assert.equal(textOnly.segments.length, 1, 'text-only behaviour is unchanged by the hand-off');
  assert.equal(textOnly.loops[0].removed, 3);
});


test('guardTranscription reports the insertion class from the real turbo artifact', () => {
  const r = guardTranscription({ me: [], others: TURBO_NUMERAL_INSERTION });
  assert.equal(r.report.detected, true);
  assert.equal(r.report.classes.insertion, 1);
  assert.equal(r.report.classes.repetition, 0);
  assert.equal(r.report.classes.overlapStutter, 0);
  assert.equal(r.report.byChannel.others.insertions, 1);
  assert.equal(r.report.insertions[0].channel, 'others');
  assert.equal(r.report.removed, 0, 'the insertion class removes nothing');
});

test('guardTranscription reports the stutter class from the real large-v3 artifact', () => {
  const r = guardTranscription({ me: LARGE_V3_SLIDING_STUTTER, others: [] });
  assert.equal(r.report.detected, true);
  assert.ok(r.report.classes.overlapStutter >= 1);
  assert.ok(r.report.removed > 0);
  assert.equal(r.report.stutters[0].channel, 'me');
});

test('the loop class still reports exactly as before (no regression in class 1)', () => {
  const r = guardTranscription({ me: [], others: largeV3Hallucination });
  assert.equal(r.report.classes.repetition, 1);
  assert.equal(r.report.removed, 3);
  assert.equal(r.report.loops[0].count, 4);
});

test('all three real captured CLEAN channels stay untouched and undetected', () => {
  for (const [name, fixture] of [
    ['q5 on the noisy sample', Q5_SAME_AUDIO_CLEAN],
    ['turbo on the clean sample, genuine enumeration', GENUINE_SPOKEN_ENUMERATION],
    ['turbo, mid-call', CLEAN_NO_BOUNDARY_OVERLAP],
  ]) {
    const r = guardTranscription({ me: [], others: fixture });
    assert.equal(r.report.detected, false, `${name} must not trip any class`);
    assert.equal(r.report.removed, 0, name);
    assert.equal(flat(r.others), flat(fixture), `${name} is byte-identical after the guard`);
  }
});

test('a DETECTED-BUT-UNREPAIRED class becomes a plain-English verification warning (never silent)', () => {
  const r = guardTranscription({ me: [], others: TURBO_NUMERAL_INSERTION });
  const w = guardWarnings(r.report);
  assert.equal(w.length, 1);
  assert.ok(/STILL IN the transcript/.test(w[0]), 'it says the fabricated text was NOT removed');
  assert.ok(/re-transcribe/.test(w[0]), 'and names the remedy');
  assert.ok(/57 fabricated/.test(w[0]) && /"others" channel/.test(w[0]));
});

test('the report says whether a probe REACHED the guard — "detect-only" and "clean" never merge', () => {
  assert.equal(guardTranscription({ me: [], others: TURBO_NUMERAL_INSERTION }).report.insertionProbe, false);
  assert.equal(
    guardTranscription({ me: [], others: TURBO_NUMERAL_INSERTION }, { probe: (s) => s.map(() => ({ tight: 'x', wide: 'x' })) }).report.insertionProbe,
    true,
  );
});

test('the insertion probe is routed PER CHANNEL — it is told which wav to cut', () => {
  const seen = [];
  const r = guardTranscription(
    { me: [], others: TURBO_NUMERAL_INSERTION },
    { probe: (spans, channel) => { seen.push(channel); return spans.map(() => ({ tight: 'nothing', wide: 'nothing' })); } },
  );
  assert.deepEqual(seen, ['others'], 'only the channel that carries a finding costs a decode');
  assert.equal(r.report.insertionsRepaired, 57);
  assert.equal(r.report.insertionsKeptSpoken, 0);
});

test('a repaired insertion says what it removed AND what it left, and the two never merge', () => {
  const r = guardTranscription(
    { me: [], others: TURBO_NUMERAL_INSERTION },
    {
      probe: (spans) => spans.map((s) => (s.startMs === 205260
        ? { tight: 'zero we ran a full checksum comparison', wide: 'Zero. We ran a full checksum comparison' }
        : { tight: 'no numeral here at all', wide: 'nor here' })),
    },
  );
  assert.equal(r.report.insertionsRepaired, 56);
  assert.equal(r.report.insertionsKeptSpoken, 1);
  assert.equal(r.report.insertionsUnprobed, 0);
  const w = guardWarnings(r.report);
  assert.equal(w.length, 1);
  assert.ok(/were KEPT/.test(w[0]), 'the kept numeral is named as a refusal, not as a defect');
  assert.ok(/no action needed/.test(w[0]));
});

test('an insertion NOTHING adjudicated warns that it was not adjudicated — never that it was clean', () => {
  const r = guardTranscription(
    { me: [], others: TURBO_NUMERAL_INSERTION },
    { probe: (spans) => spans.map(() => ({ tight: '', wide: '' })) },
  );
  const w = guardWarnings(r.report);
  assert.ok(w.some((x) => /57 of 57/.test(x) && /NOT adjudicated/.test(x)));
  assert.equal(flat(r.others), flat(TURBO_NUMERAL_INSERTION), 'and not one character moved');
});

test('a REPAIRED class produces no warning — the transcript no longer contains it', () => {
  const loop = guardTranscription({ me: [], others: largeV3Hallucination });
  assert.deepEqual(guardWarnings(loop.report), []);
  const stutter = guardTranscription({ me: LARGE_V3_SLIDING_STUTTER, others: [] });
  assert.deepEqual(guardWarnings(stutter.report), []);
});

test('guardChannelAll runs loops before insertions before overlap so classes cannot mask each other', () => {
  const r = guardChannelAll(TURBO_NUMERAL_INSERTION);
  assert.equal(r.insertions.length, 1);
  assert.equal(r.loops.length, 0);
  assert.equal(r.stutters.length, 0, 'a leading marker must not be mistaken for a boundary overlap');
});

test('guardTranscription reports class 4 PER CHANNEL, and never mixes the two grids', () => {
  const me = [
    { startMs: 0, endMs: 29980, text: 'Thank you.', speaker: 'me', wordTimesMs: [20, 200] },
    { startMs: 40000, endMs: 43000, text: 'so what changed on the pricing page', speaker: 'me', wordTimesMs: [40100, 40500, 40900, 41300, 41700, 42100] },
  ];
  const others = [
    { startMs: 0, endMs: 29980, text: 'Thank you.', speaker: 'others', wordTimesMs: [20, 200] },
  ];
  // The `others` channel HAS a burst under that span; the `me` channel does not. Feeding one
  // channel's grid to the other would invent evidence, so the report must show 1 removal, not 2.
  const r = guardTranscription({ me, others }, {
    speechBursts: { me: [{ startMs: 40000, endMs: 42900 }], others: [{ startMs: 0, endMs: 25000 }] },
  });
  assert.equal(r.report.classes.silenceFabrication, 1);
  assert.equal(r.report.silenceRemoved, 1);
  assert.equal(r.report.byChannel.me.silenceRemoved, 1);
  assert.equal(r.report.byChannel.others.silenceRemoved, 0);
  assert.equal(r.me.length, 1);
  assert.equal(r.others.length, 1, 'the guest line is backed by audio and survives');
  assert.deepEqual(r.report.silenceProbed, { me: true, others: true });
  assert.deepEqual(r.report.silenceUnit, { me: 'word-times', others: 'word-times' });
  assert.equal(r.report.removed, 1, 'removed carries class 4');
});

test('class 4 REPORT-ONLY becomes a plain-English warning; class 4 REMOVED does not', () => {
  const removedOnly = guardTranscription(
    { me: [{ startMs: 0, endMs: 29980, text: 'Thank you.', speaker: 'me', wordTimesMs: [20] }], others: [] },
    { speechBursts: { me: [], others: [] } },
  );
  assert.equal(removedOnly.report.silenceRemoved, 1);
  assert.deepEqual(guardWarnings(removedOnly.report), [], 'a repaired class produces no warning');

  const reported = guardTranscription(
    { me: [{ startMs: 0, endMs: 29980, text: "That's a tough one.", speaker: 'me', wordTimesMs: [20] }], others: [] },
    { speechBursts: { me: [], others: [] } },
  );
  assert.equal(reported.report.silenceUnrepaired, 1);
  const w = guardWarnings(reported.report);
  assert.equal(w.length, 1);
  assert.ok(/measures as SILENT/.test(w[0]), 'it says what the physical probe found');
  assert.ok(/left them IN the transcript/.test(w[0]), 'and that it did NOT remove them');
  assert.ok(/"me" channel/.test(w[0]));
});

// ---------------------------------------------------------------------------------------
group('P5 diarization seam — honest scope (default off; opt-in tinydiarize turn segmentation)');

test('default method "none" is identity — one "Them", no wrong speaker counts', () => {
  const segs = [
    { startMs: 0, endMs: 2000, text: 'first remote line', speaker: 'others' },
    { startMs: 2000, endMs: 4000, text: 'second remote line', speaker: 'others' },
  ];
  const d = diarizeOthers(segs);
  assert.equal(d.method, 'none');
  assert.equal(d.speakerCount, 1);
  assert.ok(d.segments.every((s) => s.diarizedLabel === undefined), 'no per-turn labels when off');
});

test('readTurn strips the whisper.cpp tinydiarize marker and flags the turn', () => {
  const r = readTurn({ text: `okay let me hand over ${SPEAKER_TURN_MARKER}`, speaker: 'others' });
  assert.equal(r.turned, true);
  assert.ok(!r.text.includes('[SPEAKER'), 'the marker is stripped from the rendered text');
  assert.equal(r.text, 'okay let me hand over');
});

test('tinydiarize-turns splits the RIGHT channel at native turn markers into sequential remotes', () => {
  const segs = [
    { startMs: 0, endMs: 2000, text: `Alice speaking here ${SPEAKER_TURN_MARKER}`, speaker: 'others' },
    { startMs: 2000, endMs: 4000, text: 'now Bob replies', speaker: 'others', speakerTurn: true },
    { startMs: 4000, endMs: 6000, text: 'and a third voice', speaker: 'others' },
  ];
  const d = diarizeOthers(segs, { method: 'tinydiarize-turns' });
  assert.equal(d.method, 'tinydiarize-turns');
  assert.equal(d.identityStable, false, 'turn segmentation is honestly not stable identity');
  assert.equal(d.segments[0].diarizedLabel, 'Remote 1');
  assert.equal(d.segments[1].diarizedLabel, 'Remote 2');
  assert.equal(d.segments[2].diarizedLabel, 'Remote 3');
  assert.equal(d.turns, 3);
});

test('a caption NAME still wins over a diarized turn label in the merge', () => {
  const others = diarizeOthers(
    [{ startMs: 2500, endMs: 5500, text: 'hello, thanks for joining', speaker: 'others' }],
    { method: 'tinydiarize-turns' },
  ).segments;
  const merged = mergeTranscript({ me: [], others, captions, startedAt: T0 });
  assert.ok(merged.speakers.includes('Marcus Whitfield'), 'the real caption name beats "Remote 1"');
  assert.ok(!merged.speakers.includes('Remote 1'));
});

test('with no caption, the diarized turn label fills the gap (better than generic "Them")', () => {
  const others = diarizeOthers(
    [{ startMs: 0, endMs: 2000, text: 'unnamed remote speaker', speaker: 'others' }],
    { method: 'tinydiarize-turns' },
  ).segments;
  const merged = mergeTranscript({ me: [], others, captions: [], startedAt: T0 });
  assert.ok(merged.speakers.includes('Remote 1'));
});

// ---------------------------------------------------------------------------------------
group('P5 deletion detector, stage A — COVERAGE: which speech the transcript never claims');

// INVENTED FIXTURES, and that is deliberate. The real corpus this detector was measured on is
// private material; reproducing a line of it here would put private speech in the repository
// that goes public. Invented lines of the same SHAPE prove the same properties, and the shapes are
// taken from the measurement: a segment whose extent stretches tens of seconds across silence, a
// burst holding one short clause, laughter at speech level, near-silence with a fabricated
// "Thank you." over it.
const SEG = (startMs, endMs, text, wordTimesMs) => ({ startMs, endMs, text, speaker: 'me', ...(wordTimesMs ? { wordTimesMs } : {}) });
const BURST = (startMs, endMs) => ({ startMs, endMs });

test('a burst the transcript places words inside is NOT a deletion candidate', () => {
  const segs = [SEG(0, 4000, 'the migration is finished on two of the four workspaces', [200, 900, 1600, 2300, 3000])];
  const { candidates } = findDeletionCandidates(segs, [BURST(0, 4000)], { channel: 'me' });
  assert.equal(candidates.length, 0);
});

test('a burst with ZERO emitted words IS a candidate — the whole class in one assertion', () => {
  const segs = [SEG(0, 2000, 'the migration is finished', [200, 900, 1600])];
  const { candidates } = findDeletionCandidates(segs, [BURST(0, 2000), BURST(4000, 6500)], { channel: 'me' });
  assert.equal(candidates.length, 1);
  assert.equal(candidates[0].startMs, 4000);
  assert.equal(candidates[0].durationSec, 2.5);
});

test('a burst shorter than minGapSec is never a candidate — the sub-second band is out of scope', () => {
  const segs = [SEG(0, 2000, 'the migration is finished', [200, 900, 1600])];
  const { candidates } = findDeletionCandidates(segs, [BURST(4000, 4800)], { channel: 'me' });
  assert.equal(candidates.length, 0);
});

test('word times WIN over segment extents, and coverageUnit says which unit was used', () => {
  const segs = [SEG(0, 30000, 'we can add nodes', [26000, 26400, 26800, 27200])];
  const found = findDeletionCandidates(segs, [BURST(4000, 6000), BURST(26000, 27500)], { channel: 'me' });
  assert.equal(found.coverageUnit, 'word-times');
  // the 4-6 s burst is uncovered even though the segment's EXTENT spans it
  assert.equal(found.candidates.length, 1);
  assert.equal(found.candidates[0].startMs, 4000);
});

test('without word times the extent fallback spreads words across the extent — and SAYS so', () => {
  const segs = [SEG(0, 30000, 'we can add nodes')];
  const found = findDeletionCandidates(segs, [BURST(4000, 6000), BURST(26000, 27500)], { channel: 'me' });
  assert.equal(found.coverageUnit, 'segment-extent');
  // the same 4-6 s burst now looks covered: this is the measured weakness, asserted so it cannot
  // be mistaken for equivalent evidence
  assert.equal(found.candidates.length, 0);
});

test('coverage tolerance: a word 0.24 s outside the burst covers it, 0.4 s outside does not', () => {
  const near = findDeletionCandidates([SEG(0, 9000, 'a b c', [3760])], [BURST(4000, 5200)], {});
  assert.equal(near.candidates.length, 0);
  const far = findDeletionCandidates([SEG(0, 9000, 'a b c', [3600])], [BURST(4000, 5200)], {});
  assert.equal(far.candidates.length, 1);
});

test('nearbyTranscriptText picks up the CONTAINING segment however long its extent', () => {
  const segs = [SEG(0, 30000, 'we can add nodes', [200]), SEG(40000, 42000, 'anything for friday', [40100])];
  const t = nearbyTranscriptText(segs, 26000, 27500, 2);
  assert.ok(t.includes('we can add nodes'));
  assert.ok(!t.includes('anything for friday'));
});

// ---------------------------------------------------------------------------------------
group('P5 deletion detector, stage B — THE PRECISION RULE, one test per condition');

const CAND = (over = {}) => ({ channel: 'me', index: 0, startMs: 10000, endMs: 12500, durationSec: 2.5, nearbyText: '', ...over });
const PROBE = (over = {}) => ({ tight: '', wide: '', maxDb: -12, meanDb: -30, ...over });

test('lexicalText strips the silence-hallucination vocabulary whisper emits over dead air', () => {
  for (const s of ['Thank you.', ' you ', 'Shh', '*sniff*', '[BLANK_AUDIO]', '-', 'Ha ha ha ha!', '(laughs)']) {
    assert.equal(lexicalText(s), '', `expected "${s}" to carry no lexical content`);
  }
  assert.equal(lexicalText('  the runbook  pointed  at the wrong dashboard '), 'the runbook pointed at the wrong dashboard');
});

test('informativeWords counts a laugh once and drops it — the loudest false positive in the corpus', () => {
  assert.deepEqual(informativeWords('But first, ha, ha, ha, ha, ha, ha.').sort(), ['but', 'first']);
  assert.equal(informativeWords('yeah yeah okay okay').length, 0);
  assert.equal(informativeWords('the runbook pointed at the wrong dashboard').length, 6);
});

test('all five conditions satisfied -> DELETED, with the recovered words carried for the alarm', () => {
  const v = adjudicateCandidate(
    CAND(),
    PROBE({ tight: 'the runbook pointed at the wrong dashboard', wide: 'the runbook pointed at the wrong dashboard entirely', maxDb: -12 }),
    { peakDb: -1 },
  );
  assert.equal(v.verdict, 'deleted');
  assert.equal(v.text, 'the runbook pointed at the wrong dashboard');
});

test('condition 2 — a decode of only fillers is NOT-SPEECH, whatever its raw word count', () => {
  const v = adjudicateCandidate(CAND(), PROBE({ tight: 'But first, ha, ha, ha, ha, ha, ha.', wide: 'But first, ha, ha, ha.' }), { peakDb: -1 });
  assert.equal(v.verdict, 'not-speech');
  assert.match(v.reason, /distinct informative word/);
});

test('condition 3 — near-silence with a fabricated "Thank you." over it is NOT a deletion', () => {
  // the shape measured at 4885.2 s: max -51.3 dBFS against a -0.9 dBFS peak, whisper emitting text
  const v = adjudicateCandidate(
    CAND(),
    PROBE({ tight: 'the runbook pointed at the wrong dashboard', wide: 'the runbook pointed at the wrong dashboard', maxDb: -51.3 }),
    { peakDb: -0.9 },
  );
  assert.equal(v.verdict, 'not-speech');
  assert.match(v.reason, /speech floor/);
  const levelOnly = adjudicateCandidate(CAND(), PROBE({ tight: '', wide: '', maxDb: -51.3 }), { peakDb: -0.9 });
  assert.equal(levelOnly.verdict, v.verdict);
  assert.equal(levelOnly.reason, v.reason);
});

test('condition 3 is CHANNEL-RELATIVE — the same dBFS passes on a quieter channel', () => {
  const probe = PROBE({ tight: 'the runbook pointed at the wrong dashboard', wide: 'the runbook pointed at the wrong dashboard', maxDb: -30 });
  assert.equal(adjudicateCandidate(CAND(), probe, { peakDb: -0.9 }).verdict, 'not-speech');
  assert.equal(adjudicateCandidate(CAND(), probe, { peakDb: -12 }).verdict, 'deleted');
});

test('condition 4 — a decode that changes with the padding is a property of the window, not the audio', () => {
  const v = adjudicateCandidate(
    CAND(),
    PROBE({ tight: 'the runbook pointed at the wrong dashboard', wide: 'we can add two nodes there' }),
    { peakDb: -1 },
  );
  assert.equal(v.verdict, 'not-speech');
  assert.match(v.reason, /share only/);
});

test('condition 5a — words already in the surrounding transcript are MISTIMED, not missing', () => {
  const v = adjudicateCandidate(
    CAND({ nearbyText: 'sorry about that the runbook pointed at the wrong dashboard entirely' }),
    PROBE({ tight: 'the runbook pointed at the wrong dashboard', wide: 'the runbook pointed at the wrong dashboard' }),
    { peakDb: -1 },
  );
  assert.equal(v.verdict, 'mistimed');
  assert.match(v.reason, /wrong timestamps/);
});

test('condition 5b — one inserted word must not defeat the echo test (the measured near-miss)', () => {
  // the shape measured on real audio: the isolated re-decode returns the same clause plus one extra
  // connective word, which cuts the longest EXACT run in half while the sentence is plainly present
  const v = adjudicateCandidate(
    CAND({ nearbyText: 'we can add nodes here' }),
    PROBE({ tight: 'We can also add nodes.', wide: 'We can also add nodes.' }),
    { peakDb: -1 },
  );
  assert.equal(v.verdict, 'mistimed');
  assert.match(v.reason, /in order/);
});

test('echoLength wants a CONTIGUOUS run; echoRatio tolerates insertions', () => {
  assert.equal(echoLength('we can also add nodes', 'we can add nodes here'), 2);
  assert.ok(echoRatio('we can also add nodes', 'we can add nodes here') >= 0.8);
  assert.equal(echoLength('the frankfurt cluster crossed eighty percent', 'anything else for friday'), 0);
  assert.equal(echoRatio('the frankfurt cluster crossed eighty percent', 'anything else for friday'), 0);
});

test('no probe -> UNPROBED, and an unprobed span is never a deletion', () => {
  const v = adjudicateCandidate(CAND(), null, { peakDb: -1 });
  assert.equal(v.verdict, 'unprobed');
});

// ---------------------------------------------------------------------------------------
group('P5 deletion detector — the report, and the two answers it must never conflate');

const LOUD = 'the runbook pointed at the wrong dashboard';

test('guardDeletions reports a deletion with the span, the level and the recovered words', () => {
  const segs = [SEG(0, 2000, 'right lets start with the incident review', [200, 900, 1600])];
  const { report } = guardDeletions(
    { me: segs, others: [] },
    {
      speechBursts: { me: [BURST(0, 2000), BURST(6000, 9000)], others: [] },
      peaks: { me: -1, others: -1 },
      probe: () => [{ tight: LOUD, wide: `${LOUD} entirely`, maxDb: -11, meanDb: -25 }],
    },
  );
  assert.equal(report.detected, true);
  assert.equal(report.deletedSpans, 1);
  assert.equal(report.deletedSeconds, 3);
  assert.equal(report.deletions[0].startMs, 6000);
  assert.equal(report.deletions[0].recovered, LOUD);
  assert.equal(report.byChannel.me.candidates, 1);
});

test('NO BURST GRID is "never looked", not "nothing found" — probeAvailable says so per channel', () => {
  const { report } = guardDeletions(
    { me: [SEG(0, 2000, 'a b c', [100])], others: [] },
    { speechBursts: null, peaks: { me: -1, others: -1 }, probe: () => [] },
  );
  assert.equal(report.detected, false);
  assert.equal(report.candidates, 0);
  assert.equal(report.byChannel.me.probeAvailable, false);
});

test('NO PROBE -> every candidate is unprobed and NOTHING is called a deletion', () => {
  const { report } = guardDeletions(
    { me: [SEG(0, 2000, 'a b c', [100, 500, 900])], others: [] },
    { speechBursts: { me: [BURST(0, 2000), BURST(6000, 9000)], others: [] }, peaks: { me: -1, others: -1 } },
  );
  assert.equal(report.probeAvailable, false);
  assert.equal(report.deletedSpans, 0);
  assert.equal(report.unprobedSpans, 1);
  assert.equal(report.detected, false);
});

test('the probe budget is a HARD CAP: longest spans first, the rest reported UNPROBED', () => {
  const bursts = [BURST(0, 2000)];
  for (let i = 0; i < 5; i += 1) bursts.push(BURST(10000 + i * 10000, 10000 + i * 10000 + 1500 + i * 500));
  let asked = 0;
  const { report } = guardDeletions(
    { me: [SEG(0, 2000, 'a b c', [100, 500, 900])], others: [] },
    {
      speechBursts: { me: bursts, others: [] },
      peaks: { me: -1, others: -1 },
      maxProbes: 2,
      probe: (spans) => {
        asked = spans.length;
        return spans.map(() => ({ tight: LOUD, wide: `${LOUD} entirely`, maxDb: -11, meanDb: -25 }));
      },
    },
  );
  assert.equal(asked, 2, 'exactly the budget was probed');
  assert.equal(report.deletedSpans, 2);
  assert.equal(report.unprobedSpans, 3);
  // the two probed spans are the two LONGEST
  assert.deepEqual(report.deletions.map((d) => d.durationSec).sort(), [3, 3.5]);
});

test('deletionWarnings names the span in clock time, quotes what is missing, and says detect-only', () => {
  const w = deletionWarnings({
    deletions: [{ channel: 'me', startMs: 4983000, endMs: 4986000, durationSec: 3, recovered: LOUD }],
    unprobed: [],
  });
  assert.equal(w.length, 1);
  assert.match(w[0], /01:23:03–01:23:06/);
  assert.match(w[0], /MISSING/);
  assert.match(w[0], /detected, not repaired/);
  assert.match(w[0], /re-transcribe/);
});

test('deletionWarnings never lets an UNADJUDICATED span read as a clean transcript', () => {
  const w = deletionWarnings({ deletions: [], unprobed: [{ channel: 'me', startMs: 0, endMs: 2000, durationSec: 2 }] });
  assert.equal(w.length, 1);
  assert.match(w[0], /not claimed as deletions and they are not cleared either/);
});

test('SILENCE vs DELETION side by side: silent on the silence, loud on the clause beside it', () => {
  // one channel, two wordless bursts. The first is genuine silence measured 50 dB under the peak
  // (nothing was said); the second holds a clause at speech level that the transcript never claims.
  // If the detector fired on both, or on neither, this test would be worthless — so it asserts both.
  const segs = [SEG(0, 3000, 'legal wants the addendum signed before the fifteenth', [200, 900, 1600, 2300])];
  const probes = [
    { tight: LOUD, wide: `${LOUD} entirely`, maxDb: -11, meanDb: -26 }, // the loud clause (longest, probed first)
    { tight: 'Thank you.', wide: 'Thank you. Thank you.', maxDb: -51.3, meanDb: -62 }, // the silence
  ];
  const { report } = guardDeletions(
    { me: segs, others: [] },
    {
      speechBursts: { me: [BURST(0, 3000), BURST(20000, 22000), BURST(30000, 33000)], others: [] },
      peaks: { me: -0.9, others: -0.9 },
      probe: (spans) => spans.map((s) => (s.startMs === 30000 ? probes[0] : probes[1])),
    },
  );
  assert.equal(report.deletedSpans, 1, 'exactly one of the two wordless bursts is a deletion');
  assert.equal(report.deletions[0].startMs, 30000, 'the clause, not the silence');
  const silence = report.rejected.find((r) => r.startMs === 20000);
  assert.ok(silence, 'the silent burst was examined, not skipped');
  assert.equal(silence.verdict, 'not-speech');
});


// ---------------------------------------------------------------------------------------
group('P5 word-density instrument, stage A — THE BUDGET: how much speech, how many words');

// INVENTED FIXTURES, like the deletion detector's beside them and for the same reason: the corpus
// this instrument was measured on is private material. The SHAPES are taken from the
// measurement — an 8-second window of ordinary conversational speech, the same window reduced to
// four words, a burst of laughter at speech level, a channel whose median density is itself below
// the conversational floor. The rates are the measured ones: real conversational English on this
// project's corpora runs 1.87-3.68 words per second of detected speech, median 2.88.
const DSEG = (startMs, endMs, text, wordTimesMs) => ({ startMs, endMs, text, speaker: 'me', ...(wordTimesMs ? { wordTimesMs } : {}) });

/** A window of `n` bursts of `burstMs` each, `gapMs` apart, starting at `from`. */
const BURSTS = (from, n, burstMs, gapMs) =>
  Array.from({ length: n }, (_, i) => ({ startMs: from + i * (burstMs + gapMs), endMs: from + i * (burstMs + gapMs) + burstMs }));

/** `count` word times spread evenly across a burst — a transcript that claims that burst's speech. */
const WORDS_OVER = (burst, count) =>
  Array.from({ length: count }, (_, i) => Math.round(burst.startMs + ((i + 0.5) / count) * (burst.endMs - burst.startMs)));

/** A channel of `n` healthy bursts at `wps` words per second, as segments + its burst grid. */
function healthyChannel(n, wps, burstMs = 3000, gapMs = 700, from = 0) {
  const bursts = BURSTS(from, n, burstMs, gapMs);
  const segments = bursts.map((b, i) => {
    const count = Math.round((burstMs / 1000) * wps);
    const times = WORDS_OVER(b, count);
    return DSEG(b.startMs, b.endMs, new Array(count).fill('word').join(' '), times);
  });
  return { bursts, segments };
}

test('tileWindows groups consecutive bursts until the window holds windowSpeechSec of speech', () => {
  const bursts = BURSTS(0, 9, 3000, 700);
  const wins = tileWindows(bursts, 0, { ...DEFAULT_SPARSITY_OPTS, windowSpeechSec: 8 });
  assert.equal(wins.length, 3, 'nine 3 s bursts make three 9 s windows at an 8 s target');
  assert.equal(wins[0].bursts, 3);
  assert.equal(wins[0].speechMs, 9000);
});

test('the shipped window is 4 s of speech — swept, and the value is what the sweep chose', () => {
  // Pinned so the value cannot drift without someone re-reading why it is 4: 2/12 recall at 8 s
  // against 10/12 at 4 s on the reference corpus, and the candidate noise starting at 3 s.
  assert.equal(DEFAULT_SPARSITY_OPTS.windowSpeechSec, 4);
  const bursts = BURSTS(0, 9, 3000, 700);
  const wins = tileWindows(bursts, 0, DEFAULT_SPARSITY_OPTS);
  assert.equal(wins[0].bursts, 2, 'two 3 s bursts clear a 4 s target');
});

test('a window whose bursts are too far apart is ABANDONED, never judged on a denominator of silence', () => {
  // 3 s of speech every 40 s: reaching the speech target would break the 30 s wall cap.
  const bursts = BURSTS(0, 6, 3000, 40000);
  assert.equal(tileWindows(bursts, 0, DEFAULT_SPARSITY_OPTS).length, 0);
});

test('a window at ordinary conversational density is NOT a candidate', () => {
  const { bursts, segments } = healthyChannel(12, 2.9);
  const found = findSparseWindows(segments, bursts, { channel: 'me' });
  assert.equal(found.candidates.length, 0);
  assert.ok(found.medianDensity >= 2.5 && found.medianDensity <= 3.3, `median ${found.medianDensity}`);
});

test('THE WHOLE CLASS IN ONE ASSERTION: a window whose speech became four words IS a candidate', () => {
  const { bursts, segments } = healthyChannel(12, 2.9);
  // Replace the 4th, 5th and 6th bursts' text with four words total — 9 s of speech, 4 words.
  const kept = segments.filter((_, i) => i < 3 || i > 5);
  kept.push(DSEG(bursts[3].startMs, bursts[5].endMs, 'the orange folder arrived', [
    bursts[3].startMs + 100, bursts[3].startMs + 400, bursts[3].startMs + 700, bursts[3].startMs + 1000,
  ]));
  kept.sort((a, b) => a.startMs - b.startMs);
  const found = findSparseWindows(kept, bursts, { channel: 'me' });
  assert.ok(found.candidates.length >= 1, 'nine seconds of speech carrying four words is a candidate');
  const c = found.candidates[0];
  assert.ok(c.emittedWords <= 4, `emitted ${c.emittedWords}`);
  assert.ok(c.density < 1.2, `density ${c.density} is below the conversational floor`);
  assert.ok(c.deficitWords > 0, `deficit ${c.deficitWords} words against the budget`);
});

test('a WORDLESS window is never a candidate here — that is the deletion detector\'s class', () => {
  const { bursts, segments } = healthyChannel(12, 2.9);
  const kept = segments.filter((_, i) => i < 3 || i > 5); // nothing at all over bursts 3-5
  const found = findSparseWindows(kept, bursts, { channel: 'me' });
  assert.equal(found.candidates.length, 0, 'one failure must never be reported twice under two names');
});

test('on a channel at conversational pace the relative half is INERT — the absolute floor binds', () => {
  // Measured on both channels of the 92-minute corpus (medians 2.88 and 2.51 w/s): 0.45x a normal
  // median is above 1.2, so `min()` picks the absolute floor and the baseline changes nothing. The
  // relative half exists for the OTHER case, below.
  const { bursts, segments } = healthyChannel(24, 2.9);
  const found = findSparseWindows(segments, bursts, { channel: 'me' });
  assert.equal(found.thresholdWordsPerSec, 1.2);
});

test('THE COST OF THAT PROTECTION, asserted rather than hidden: a slow channel must collapse further', () => {
  // A channel whose whole delivery runs at 1.33 w/s. The relative half drops the floor to 0.6 w/s,
  // so a window at 0.44 w/s — a finding on any ordinary channel — is not even a candidate here.
  // That is the stated recall cost of not flagging a slow speaker wholesale, and it is one-directional.
  const { bursts, segments } = healthyChannel(24, 1.33);
  const kept = segments.filter((_, i) => i < 3 || i > 5);
  kept.push(DSEG(bursts[3].startMs, bursts[5].endMs, 'the orange folder arrived', [
    bursts[3].startMs + 100, bursts[3].startMs + 400, bursts[3].startMs + 700, bursts[3].startMs + 1000,
  ]));
  kept.sort((a, b) => a.startMs - b.startMs);
  const found = findSparseWindows(kept, bursts, { channel: 'me' });
  assert.ok(found.thresholdWordsPerSec < 1.2, `the relative half tightened the floor to ${found.thresholdWordsPerSec}`);
  assert.equal(found.candidates.length, 0);
  // The same four words over the same nine seconds ARE a finding on a channel at conversational pace.
  const fast = healthyChannel(24, 2.9);
  const fastKept = fast.segments.filter((_, i) => i < 3 || i > 5);
  fastKept.push(DSEG(fast.bursts[3].startMs, fast.bursts[5].endMs, 'the orange folder arrived', [
    fast.bursts[3].startMs + 100, fast.bursts[3].startMs + 400, fast.bursts[3].startMs + 700, fast.bursts[3].startMs + 1000,
  ]));
  fastKept.sort((a, b) => a.startMs - b.startMs);
  assert.equal(findSparseWindows(fastKept, fast.bursts, { channel: 'me' }).candidates.length, 1);
});

test('a confirmed DELETION is excluded from the budget — the exclusion can only remove findings', () => {
  const { bursts, segments } = healthyChannel(12, 2.9);
  const kept = segments.filter((_, i) => i < 3 || i > 5);
  kept.push(DSEG(bursts[3].startMs, bursts[3].endMs, 'the orange folder arrived', [
    bursts[3].startMs + 100, bursts[3].startMs + 400, bursts[3].startMs + 700, bursts[3].startMs + 1000,
  ]));
  kept.sort((a, b) => a.startMs - b.startMs);
  assert.equal(findSparseWindows(kept, bursts, { channel: 'me' }).candidates.length, 1);
  // Stage 3.7 already owns bursts 4 and 5: their seconds leave this stage's denominator.
  const excluded = findSparseWindows(kept, bursts, {
    channel: 'me',
    excludeSpans: [{ startMs: bursts[4].startMs, endMs: bursts[5].endMs }],
  });
  assert.equal(excluded.candidates.length, 0);
});

test('the whole channel being thin is a finding of its own, which no per-window comparison can see', () => {
  const { bursts, segments } = healthyChannel(24, 0.6);
  const found = findSparseWindows(segments, bursts, { channel: 'me' });
  assert.equal(found.baselineBelowFloor, true);
  assert.ok(found.medianDensity < 1.2, `median ${found.medianDensity}`);
});

test('the two counts a findings list cannot give: thin windows, and the ones that are 3.7\'s', () => {
  // The q5_0 catastrophe's shape: a quarter of the channel carries no word at all after the
  // repetition guard collapses the fabricated loop. Condition 2 correctly keeps those out of THIS
  // stage's findings, so without these counts the report would read as a broadly healthy channel.
  const { bursts, segments } = healthyChannel(24, 2.9);
  const kept = segments.filter((_, i) => i > 11); // the first half emits nothing at all
  const found = findSparseWindows(kept, bursts, { channel: 'me' });
  assert.equal(found.candidates.length, 0, 'a wordless window is never a finding here');
  assert.ok(found.wordlessWindows >= 6, `${found.wordlessWindows} wordless windows`);
  assert.equal(found.windowsBelowFloor, found.wordlessWindows, 'every thin window here is a wordless one');
});

test('analyzedSpeechSec is reported against burstSeconds, so "0 findings" can never read as "all clear"', () => {
  const dense = BURSTS(0, 6, 3000, 700);
  const scattered = BURSTS(200000, 4, 3000, 30000); // too spread out to make a window
  const bursts = [...dense, ...scattered];
  const segments = dense.map((b) => DSEG(b.startMs, b.endMs, 'a b c d e f g h', WORDS_OVER(b, 8)));
  const found = findSparseWindows(segments, bursts, { channel: 'me' });
  assert.ok(found.analyzedSpeechSec < found.burstSeconds, `${found.analyzedSpeechSec} of ${found.burstSeconds}s`);
  assert.equal(found.burstSeconds, 30);
});

// ---------------------------------------------------------------------------------------
group('P5 word-density instrument, stage B — THE PRECISION RULE, one test per condition');

const REAL = 'the runbook pointed at the wrong dashboard and nobody noticed until the second incident review';
const SPARSE_CAND = (over = {}) => ({
  channel: 'me', index: 0, startMs: 10000, endMs: 22000, wallSec: 12, speechSec: 9,
  bursts: 3, emittedWords: 4, density: 0.444, expectedWords: 10.8, deficitWords: 6.8,
  nearbyText: '', ...over,
});
const SPARSE_PROBE = (over = {}) => ({ tight: REAL, wide: `${REAL} entirely`, maxDb: -12, meanDb: -30, ...over });

test('all six conditions satisfied -> UNDER-TRANSCRIBED, carrying what the audio returned', () => {
  const v = adjudicateSparseWindow(SPARSE_CAND(), SPARSE_PROBE(), { peakDb: -0.9 });
  assert.equal(v.verdict, 'under-transcribed');
  assert.equal(v.recovered, REAL);
  assert.ok(v.probeWords >= 12, `probe recovered ${v.probeWords} informative words`);
});

test('condition 4 — a thin window at near-silence level is NOT speech, so its budget was never real', () => {
  const v = adjudicateSparseWindow(SPARSE_CAND(), SPARSE_PROBE({ maxDb: -44 }), { peakDb: -0.9 });
  assert.equal(v.verdict, 'not-speech');
  assert.match(v.reason, /speech floor/);
});

test('condition 5 — A SLOW, EMPHATIC DELIVERY IS NOT A DEFECT: the audio agrees with the transcript', () => {
  // Four words in the transcript, and decoding the window alone returns those same four words.
  const v = adjudicateSparseWindow(SPARSE_CAND(), SPARSE_PROBE({ tight: 'we are not doing that', wide: 'we are not doing that' }), { peakDb: -0.9 });
  assert.equal(v.verdict, 'matches-audio');
  assert.match(v.reason, /A thin transcript over thin speech is not a defect/);
});

test('condition 5 — laughter cannot manufacture a recovery: eight "words", two of them informative', () => {
  const laugh = 'But first, ha, ha, ha, ha, ha, ha.';
  const v = adjudicateSparseWindow(SPARSE_CAND(), SPARSE_PROBE({ tight: laugh, wide: laugh }), { peakDb: -0.9 });
  assert.equal(v.verdict, 'matches-audio');
  assert.equal(v.probeWords, 2);
});

test('condition 5 — a recovery that does not survive repadding is a property of the window', () => {
  const v = adjudicateSparseWindow(SPARSE_CAND(), SPARSE_PROBE({ wide: 'so' }), { peakDb: -0.9 });
  assert.equal(v.verdict, 'matches-audio');
  assert.match(v.reason, /does not survive repadding/);
});

test('condition 6 — words already in the transcript beside the window are PRESENT, not missing', () => {
  const v = adjudicateSparseWindow(SPARSE_CAND({ nearbyText: `and then ${REAL}` }), SPARSE_PROBE(), { peakDb: -0.9 });
  assert.equal(v.verdict, 'echoed');
  assert.match(v.reason, /timing or de-duplication question/);
});

test('condition 6 — a three-word coincidence is NOT an echo, and that is why this floor is four', () => {
  // The exact false rejection measured on the 92-minute corpus at the deletion detector's floor of 2.
  const v = adjudicateSparseWindow(SPARSE_CAND({ nearbyText: 'at the wrong end of it' }), SPARSE_PROBE(), { peakDb: -0.9 });
  assert.equal(v.verdict, 'under-transcribed');
});

test('no probe -> UNPROBED, and an unprobed window is never a finding', () => {
  const v = adjudicateSparseWindow(SPARSE_CAND(), null, { peakDb: -0.9 });
  assert.equal(v.verdict, 'unprobed');
});

// ---------------------------------------------------------------------------------------
group('P5 word-density instrument — the report, and what it refuses to claim');

test('guardSubstitution reports the span, the rate, the deficit and what the audio returned', () => {
  const { bursts, segments } = healthyChannel(12, 2.9);
  const kept = segments.filter((_, i) => i < 3 || i > 5);
  kept.push(DSEG(bursts[3].startMs, bursts[5].endMs, 'the orange folder arrived', [
    bursts[3].startMs + 100, bursts[3].startMs + 400, bursts[3].startMs + 700, bursts[3].startMs + 1000,
  ]));
  kept.sort((a, b) => a.startMs - b.startMs);
  const { report } = guardSubstitution(
    { me: kept, others: [] },
    {
      speechBursts: { me: bursts, others: [] },
      peaks: { me: -0.9, others: -0.9 },
      probe: (spans) => spans.map(() => SPARSE_PROBE()),
    },
  );
  assert.equal(report.sparseSpans, 1);
  const f = report.findings[0];
  assert.equal(f.emittedWords, 4);
  assert.equal(f.recovered, REAL);
  assert.ok(f.probeWords > f.emittedWords);
  assert.equal(report.byChannel.me.windows > 0, true);
});

test('NO PROBE -> every candidate is unprobed and NOTHING is called under-transcribed', () => {
  const { bursts, segments } = healthyChannel(12, 2.9);
  const kept = segments.filter((_, i) => i < 3 || i > 5);
  kept.push(DSEG(bursts[3].startMs, bursts[5].endMs, 'the orange folder arrived', [
    bursts[3].startMs + 100, bursts[3].startMs + 400, bursts[3].startMs + 700, bursts[3].startMs + 1000,
  ]));
  kept.sort((a, b) => a.startMs - b.startMs);
  const { report } = guardSubstitution({ me: kept, others: [] }, { speechBursts: { me: bursts, others: [] } });
  assert.equal(report.sparseSpans, 0);
  assert.equal(report.unprobedSpans, 1);
  assert.equal(report.probeAvailable, false);
});

test('no burst grid -> the instrument says it never looked, which is not the same as clean', () => {
  const { segments } = healthyChannel(12, 2.9);
  const { report } = guardSubstitution({ me: segments, others: [] }, { probe: () => [] });
  assert.equal(report.byChannel.me.probeAvailable, false);
  assert.equal(report.windows, 0);
  assert.equal(report.detected, false);
});

test('the warning names the span in clock time AND refuses to claim the words present are wrong', () => {
  const w = substitutionWarnings({
    findings: [{
      channel: 'me', startMs: 4983000, endMs: 4995000, speechSec: 9, emittedWords: 4,
      density: 0.44, probeWords: 16, recovered: REAL,
    }],
  });
  assert.match(w[0], /01:23:03–01:23:15/);
  assert.match(w[0], /Words that were spoken are NOT in the transcript here/);
  assert.match(w[0], /whether the words that ARE there are wrong cannot be decided without a reference/);
  assert.match(w[0], /detected, not repaired/);
});

test('a channel below the floor is warned about ONCE, as a channel, not as a window', () => {
  const w = substitutionWarnings({ findings: [], channelsBelowFloor: [{ channel: 'others', medianDensity: 0.6, floorWordsPerSec: 1.2, windows: 40 }] });
  assert.equal(w.length, 1);
  assert.match(w[0], /The WHOLE "others" channel is thin/);
  assert.match(w[0], /the sparse windows ARE the typical ones/);
});

test('an unadjudicated window never lets the transcript read as fully checked', () => {
  const w = substitutionWarnings({ findings: [], unprobed: [{ channel: 'me', startMs: 0, endMs: 12000 }] });
  assert.match(w[0], /not claimed as under-transcribed and they are not cleared either/);
});

// ---------------------------------------------------------------------------------------
group('P5 — the two detectors divide the timeline instead of overlapping it');

test('THE SAME WORDLESS BURSTS: a deletion to stage 3.7, invisible to stage 3.8, reported once', () => {
  // A whole WINDOW with no emitted word at all — the case where both stages are looking at the
  // same seconds and only one of them may speak.
  const { bursts, segments } = healthyChannel(12, 2.9);
  const kept = segments.filter((_, i) => i !== 4 && i !== 5);
  const del = findDeletionCandidates(kept, bursts, { channel: 'me' });
  assert.equal(del.candidates.length, 2, 'the wordless bursts are stage 3.7\'s');
  assert.equal(del.candidates[0].startMs, bursts[4].startMs);
  const sub = findSparseWindows(kept, bursts, { channel: 'me' });
  assert.equal(sub.candidates.length, 0, 'and stage 3.8 does not report the same seconds a second time');
});

test('THE SAME COVERED BURST: invisible to stage 3.7, a finding for stage 3.8 — the named blind spot', () => {
  const { bursts, segments } = healthyChannel(12, 2.9);
  const kept = segments.filter((_, i) => i < 3 || i > 5);
  kept.push(DSEG(bursts[3].startMs, bursts[5].endMs, 'the orange folder arrived', [
    bursts[3].startMs + 100, bursts[4].startMs + 400, bursts[5].startMs + 200, bursts[5].startMs + 900,
  ]));
  kept.sort((a, b) => a.startMs - b.startMs);
  // Every burst carries an emitted word, so coverage is satisfied and stage 3.7 asks nothing.
  assert.equal(findDeletionCandidates(kept, bursts, { channel: 'me' }).candidates.length, 0);
  assert.equal(findSparseWindows(kept, bursts, { channel: 'me' }).candidates.length, 1);
});

// ---------------------------------------------------------------------------------------
// Pass 3 of the corrector: canonical casing
// ---------------------------------------------------------------------------------------

test('one name spelled two ways by case is TWO spellings, and the corrector settles it', () => {
  const ents = normalizeEntities({ entities: [{ canonical: 'Halden Freight' }, { canonical: 'Everlock' }] }).entities;
  const r = correctText('the Halden freight manifest, and the everlock agent', ents);
  assert.equal(r.text, 'the Halden Freight manifest, and the Everlock agent');
  assert.deepEqual(r.corrections.map((c) => c.method), ['casing', 'casing']);
});

test("the casing pass never touches an ordinary word that happens to be someone's name", () => {
  const ents = normalizeEntities({ entities: [{ canonical: 'Rich' }, { canonical: 'Deep' }] }).entities;
  const text = 'a rich history and a deep breath';
  assert.equal(correctText(text, ents).text, text, 'stopwords are refused outright');
});

test("a short single-token canonical is below the casing pass's floor, a multi-word one is not", () => {
  const short = normalizeEntities({ entities: [{ canonical: 'Ada' }] }).entities;
  assert.equal(correctText('ada wrote it', short).text, 'ada wrote it', 'three letters is too little to be sure');
  const long = normalizeEntities({ entities: [{ canonical: 'Ada Systems' }] }).entities;
  assert.equal(correctText('ada systems wrote it', long).text, 'Ada Systems wrote it', 'a phrase is unambiguous');
});

test('caseSensitive: true opts an entity OUT — the flag declares that the casing IS the difference', () => {
  const ents = normalizeEntities({ entities: [{ canonical: 'NeXT', caseSensitive: true }] }).entities;
  assert.equal(correctText('we used next year', ents).text, 'we used next year');
});

// ---------------------------------------------------------------------------------------
// The correction flywheel, DICTATION half — "ASK, NEVER INFER" (ceo-decisions.md §7)
//
// Two properties are load-bearing and both are asserted below rather than described:
//   1. NOTHING here learns without a human answer. `reviewSent` produces questions and no route
//      from it reaches a vocabulary write.
//   2. The ask stays SILENT on a change of mind, and — the part the shipped orthographic gate got
//      wrong — it SPEAKS UP on a mis-hearing that sounds close and is spelled far apart.
// ---------------------------------------------------------------------------------------

test('phoneticKey hears b and p as one sound, which is what an orthographic gate cannot do', () => {
  assert.equal(phoneticKey('Briella'), phoneticKey('Priella'), 'b/p differ only in spelling');
  assert.equal(phoneticKey('Everlock'), phoneticKey('EverLock'), 'casing is not sound');
  assert.equal(phoneticKey('Brightmoor'), phoneticKey('Brightmore'), 'a trailing vowel is not sound');
  assert.equal(phoneticKey('aeiou'), '', 'a term with no classifiable consonant has no key');
});

test('the phonetic leg catches the mis-hearing the orthographic gate documents as its own blind spot', () => {
  // ceo-decisions.md §7: "an orthographic gate stays silent on exactly the worst ASR failures,
  // where the mis-hearing sounds close but is spelled far apart (Deke Graham -> Deepgram)".
  const orth = similarity(normalizeTerm('Deke Graham'), normalizeTerm('Deepgram'));
  const phon = phoneticSimilarity('Deke Graham', 'Deepgram');
  assert.ok(phon > orth, `sound (${phon.toFixed(2)}) beats spelling (${orth.toFixed(2)}) on this pair`);
  assert.ok(phon >= ASK_MIN_PHONETIC, 'and it clears the phonetic floor, so the ask happens');
});

test('"ship Thursday" -> "ship Friday" is a CHANGE OF MIND and produces no ask at all', () => {
  const { asks, rejected } = askCandidates('We should ship Thursday.', 'We should ship Friday.');
  assert.equal(asks.length, 0, 'a change of mind is never a vocabulary question');
  assert.ok(rejected.length >= 1, 'and the silence is explained, not merely empty');
  assert.match(rejected[0].reason, /change of mind/);
});

test('a term-shaped mis-hearing DOES ask, and names which leg let it through', () => {
  const { asks } = askCandidates(
    'The deep graham integration lands Tuesday.',
    'The Deepgram integration lands Tuesday.',
  );
  assert.equal(asks.length, 1);
  assert.equal(asks[0].to, 'Deepgram');
  assert.equal(asks[0].from, 'deep graham');
  assert.ok(['spelling', 'sound', 'both'].includes(asks[0].leg));
});

test('a lone-token mis-hearing that the LEARN gate rejects is still ASKED — the human is the judge', () => {
  // capture.js rejects "briella" -> "Priya" for inference (lone token, similarity 0.43 < 0.6). Under
  // §7 that same pair must reach the CEO as a question: similarity decides whether to ASK, never the
  // answer, and a missed ask loses the correction outright.
  const inferred = extractTermCorrections('Call Briella at four.', 'Call Priya at four.');
  assert.equal(inferred.proposals.length, 0, 'the INFERENCE path still refuses to learn it silently');
  const { asks } = askCandidates('Call Briella at four.', 'Call Priya at four.');
  assert.equal(asks.length, 1, 'but the ASK path raises it');
  assert.equal(asks[0].to, 'Priya');
});

test('"great" -> "Grant" ASKS and is never learned — a false ask is cheap, a wrong lesson is not', () => {
  // The single most dangerous pair in the whole flywheel: learn it and every future call transcript
  // starts rewriting the ordinary word "great" as a person's name. §7's answer is not a cleverer
  // threshold, it is that a human decides. So the ask happens (0.60 spelling, at the bar), nothing is
  // learned, and ONE "never" retires the question permanently.
  const { asks } = askCandidates('That was a great result.', 'That was a Grant result.');
  assert.equal(asks.length, 1, 'asked, because only he knows whether Grant is a person');
  const res = answerAsk({}, { ...asks[0] }, 'never');
  assert.equal(res.learn, null, 'and declining to learn it is the whole point');
  assert.equal(applyLedger(asks, res.ledger).prompts.length, 0, 'asked once, then never again');
});

test('the gate judges the CORE of a change, not the context wrapped around it', () => {
  // Found by a negative control over the short-call corpus: "Northgate, Tuesday" ->
  // "Northgate, Wednesday" is 0.9 similar as a phrase, because most of the phrase is identical by
  // construction. Its core, "Tuesday" -> "Wednesday", is what the question is actually about.
  const hunks = tokenReplaceHunks(
    'the review for Northgate, Tuesday night'.split(' '),
    'the review for Northgate, Wednesday night'.split(' '),
  );
  assert.equal(hunks[0].coreFrom, 'Tuesday', 'the delta, unexpanded');
  const { asks, rejected } = askCandidates(
    'the review for Northgate, Tuesday night',
    'the review for Northgate, Wednesday night',
  );
  assert.equal(asks.length, 0, 'a change of mind hiding inside a proper-noun phrase is still a change of mind');
  assert.ok(rejected.length >= 1);
});

test('a day or a month is capitalized by grammar and is never learned as a name', () => {
  // "Tuesday" -> "Wednesday" scores 0.75 by SOUND, so the phonetic leg would have asked. The list is
  // narrow and it was found by measurement, not reasoned into existence.
  for (const [a, b] of [['Tuesday', 'Wednesday'], ['March', 'April'], ['Thursday', 'Friday']]) {
    const { asks, rejected } = askCandidates(`we meet ${a} at noon`, `we meet ${b} at noon`);
    assert.equal(asks.length, 0, `${a} -> ${b} must not be asked`);
    assert.ok(rejected.some((r) => /change of mind/.test(r.reason)), `${a} -> ${b} says why`);
  }
  // and the escape hatch stays open for a customer genuinely called August
  const doc = learnTerm({ schemaVersion: 1, version: '', entities: [] }, { canonical: 'August', mangled: 'orgust' });
  assert.equal(doc.changed, true, 'learn-term is an explicit instruction and overrides the list');
});

test('an ordinary prose edit is not a vocabulary question', () => {
  const { asks, rejected } = askCandidates('I think we should wait.', 'I think we must wait.');
  assert.equal(asks.length, 0);
  assert.match(rejected.map((r) => r.reason).join(' '), /ordinary prose/);
});

test('a casing-only fix asks nothing — a vocabulary cannot hold it', () => {
  const { asks, rejected } = askCandidates('the everlock agent', 'the Everlock agent');
  assert.equal(asks.length, 0);
  assert.match(rejected.map((r) => r.reason).join(' '), /casing/);
});

test('hunk expansion stops at a full stop — a sentence-initial capital is grammar, not a name', () => {
  // Before this guard, the 2026-08-29 short-call corpus produced three asks of this shape out of
  // six captured corrections: Add "Cannery Street. That" to your vocabulary? — which is the kind of
  // question that gets a feature switched off.
  const hunks = tokenReplaceHunks(
    'the Brightmoor Dental on Canary Street. That may be the problem.'.split(' '),
    'the Brightmoor Dental on Cannery Street. That may be the problem.'.split(' '),
  );
  assert.equal(hunks.length, 1);
  assert.equal(hunks[0].to, 'Cannery Street.', 'the name, and not the next sentence with it');
  assert.equal(hunks[0].from, 'Canary Street.');
  const { asks } = askCandidates(
    'the Brightmoor Dental on Canary Street. That may be the problem.',
    'the Brightmoor Dental on Cannery Street. That may be the problem.',
  );
  // And the ask itself drops the sentence's own full stop: a vocabulary entry reading
  // "Cannery Street." is junk in the user's vocabulary file and reads as a broken feature.
  assert.equal(asks[0].to, 'Cannery Street');
  assert.equal(asks[0].from, 'Canary Street');
});

test('a trailing full stop is dropped from a learned term; an INTERNAL dot is part of it', () => {
  const { asks } = askCandidates('we moved off nodejs last year.', 'we moved off node.js last year.');
  assert.equal(asks.length, 1);
  assert.equal(asks[0].to, 'node.js', 'node.js keeps its dot — it is not sentence punctuation');
  const end = askCandidates('we moved off nodejs.', 'we moved off node.js.');
  assert.equal(end.asks[0].to, 'node.js', 'and the sentence\'s own full stop still goes');
});

test('a name fix asks about the WHOLE name, never the lone word inside it', () => {
  // "Hand" -> "Hanna" as a curated mangling would corrupt the ordinary word "hand" forever.
  const { asks } = askCandidates('I spoke to Rich Hand today.', 'I spoke to Rich Hanna today.');
  assert.equal(asks.length, 1);
  assert.equal(asks[0].from, 'Rich Hand');
  assert.equal(asks[0].to, 'Rich Hanna');
});

test('a wholesale rewrite asks nothing (neither close in spelling nor in sound)', () => {
  const { asks } = askCandidates('um', 'Marcus Whitfield');
  assert.equal(asks.length, 0);
});

test('matchHeard claims a recent, similar dictation', () => {
  const now = 1_700_000_000_000;
  const journal = [{ id: 'a', at: now - 5000, text: 'Send the deep graham numbers to Marla.' }];
  const m = matchHeard(journal, 'Send the Deepgram numbers to Marla.', { now });
  assert.ok(m, 'the sent text is recognizably that dictation, corrected');
  assert.equal(m.entry.id, 'a');
});

test('matchHeard REFUSES a typed message — this is how a typed sentence stays silent', () => {
  const now = 1_700_000_000_000;
  const journal = [{ id: 'a', at: now - 5000, text: 'Send the deep graham numbers to Marla.' }];
  assert.equal(matchHeard(journal, 'Remind me to book the flight on Tuesday.', { now }), null);
});

test('matchHeard REFUSES a dictation older than the window, and one already reconciled', () => {
  const now = 1_700_000_000_000;
  const text = 'Send the deep graham numbers to Marla.';
  assert.equal(matchHeard([{ id: 'a', at: now - MATCH_WINDOW_MS - 1, text }], text, { now }), null, 'stale');
  assert.equal(matchHeard([{ id: 'a', at: now - 1000, text, consumed: true }], text, { now }), null, 'already answered');
});

test('matchHeard breaks a tie toward the more recent dictation — he is looking at the last one', () => {
  const now = 1_700_000_000_000;
  const text = 'Send the deep graham numbers to Marla.';
  const m = matchHeard([{ id: 'old', at: now - 60000, text }, { id: 'new', at: now - 2000, text }], text, { now });
  assert.equal(m.entry.id, 'new');
});

test('reviewSent on a TYPED message returns no prompts and says why', () => {
  const now = 1_700_000_000_000;
  const journal = [{ id: 'a', at: now - 5000, text: 'Send the deep graham numbers to Marla.' }];
  const r = reviewSent(journal, 'Book the flight for Tuesday morning.', {}, { now });
  assert.equal(r.matched, false);
  assert.equal(r.prompts.length, 0);
  assert.match(r.reason, /treated as typed/);
});

test('reviewSent on an UNCHANGED dictation returns no prompts — nothing was corrected', () => {
  const now = 1_700_000_000_000;
  const text = 'Send the Deepgram numbers to Marla.';
  const r = reviewSent([{ id: 'a', at: now - 5000, text }], text, {}, { now });
  assert.equal(r.matched, true);
  assert.equal(r.prompts.length, 0);
  assert.match(r.reason, /sent unchanged/);
});

test('reviewSent asks the exact sentence §7 specifies', () => {
  const now = 1_700_000_000_000;
  const journal = [{ id: 'a', at: now - 5000, text: 'The deep graham contract is signed.' }];
  const r = reviewSent(journal, 'The Deepgram contract is signed.', {}, { now });
  assert.equal(r.prompts.length, 1);
  assert.equal(r.prompts[0].prompt, 'Add "Deepgram" to your vocabulary?');
  assert.equal(r.prompts[0].askedBefore, false);
});

test('the diff is taken against what was PASTED, not the recognizer\'s raw output', () => {
  // The shared vocabulary corrected `deep graham` on the way to the field, so `emitted` is
  // already right and he changed NOTHING. Diffing `text` would ask him to confirm a pair the
  // vocabulary already holds, at the one moment he did nothing wrong. Measured, not argued:
  // docs/measurements/heard-vs-sent-trigger-2026-08-30/README.md §6.
  const now = 1_700_000_000_000;
  const entry = {
    id: 'a',
    at: now - 5000,
    text: 'The deep graham contract is signed.',
    emitted: 'The Deepgram contract is signed.',
  };
  const unchanged = reviewSent([entry], 'The Deepgram contract is signed.', {}, { now });
  assert.equal(unchanged.matched, true, 'the pasted text must still pair with its own dictation');
  assert.deepEqual(unchanged.prompts, [], 'a pair the vocabulary already holds was asked again');
  assert.equal(unchanged.reason, 'sent unchanged — nothing was corrected');

  // And when he DOES edit it, the ask is his edit — not the hop the vocabulary already made.
  const edited = reviewSent([entry], 'The Deepgram contract is signed by Marla Kestrel.', {}, { now });
  assert.equal(edited.matched, true);
  assert.ok(
    !edited.prompts.some((p) => normalizeTerm(p.from) === 'deep graham'),
    'the already-learned pair leaked into the ask'
  );

  // POSITIVE PROBE: with no `emitted` the behavior is exactly what shipped before — the
  // fallback is a fallback, not a second silence.
  const older = { id: 'b', at: now - 5000, text: 'The deep graham contract is signed.' };
  const r = reviewSent([older], 'The Deepgram contract is signed.', {}, { now });
  assert.equal(r.prompts.length, 1, 'an older record with no `emitted` stopped producing its ask');
  assert.equal(r.prompts[0].to, 'Deepgram');
});

test('NO route out of reviewSent carries a learn — the review cannot change what the system believes', () => {
  const now = 1_700_000_000_000;
  const journal = [{ id: 'a', at: now - 5000, text: 'The deep graham contract is signed.' }];
  const r = reviewSent(journal, 'The Deepgram contract is signed.', {}, { now });
  assert.equal(r.learn, undefined, 'the review returns questions, never an instruction');
  assert.ok(r.prompts.every((p) => p.learn === undefined));
});

test('a DECLINE is not permanent: the very next repeat asks again, and says it has asked before', () => {
  const ask = { from: 'deep graham', to: 'Deepgram', key: askKey('deep graham', 'Deepgram') };
  const first = answerAsk({}, ask, 'decline');
  assert.equal(first.learn, null, 'a decline learns nothing');
  const { prompts } = applyLedger([ask], first.ledger);
  assert.equal(prompts.length, 1, 'asked again on the next repeat — no threshold, no cool-off');
  assert.equal(prompts[0].askedBefore, true);
  assert.match(prompts[0].prompt, /you corrected this before/);
});

test('"don\'t ask for this term again" is permanent, and lands on an INSPECTABLE list', () => {
  const ask = { from: 'great', to: 'Grant', key: askKey('great', 'Grant') };
  const res = answerAsk({}, ask, 'never');
  assert.equal(res.learn, null);
  assert.deepEqual(res.ledger.suppressed, [askKey('great', 'Grant')], 'readable, not a hash');
  const after = applyLedger([ask], res.ledger);
  assert.equal(after.prompts.length, 0, 'never asked again');
  assert.equal(after.suppressed.length, 1, 'and the suppression is reported, not silent');
});

test('CONFIRM is the only answer that yields a vocabulary pair', () => {
  const ask = { from: 'deep graham', to: 'Deepgram', key: askKey('deep graham', 'Deepgram') };
  assert.deepEqual(answerAsk({}, ask, 'confirm').learn, { canonical: 'Deepgram', mangled: 'deep graham' });
  assert.equal(answerAsk({}, ask, 'decline').learn, null);
  assert.equal(answerAsk({}, ask, 'never').learn, null);
  assert.throws(() => answerAsk({}, ask, 'probably'), /unknown answer/);
});

test('a confirmed pair goes in by the SAME explicit path learn-term uses, version bump included', () => {
  const ask = { from: 'deep graham', to: 'Deepgram', key: askKey('deep graham', 'Deepgram') };
  const { learn } = answerAsk({}, ask, 'confirm');
  const doc = { schemaVersion: 1, version: '2026-08-01', entities: [] };
  const res = learnTerm(doc, { canonical: learn.canonical, mangled: learn.mangled }, { today: '2026-08-29' });
  assert.equal(res.changed, true);
  assert.equal(res.doc.version, '2026-08-29');
  assert.deepEqual(res.doc.entities[0].mangled, ['deep graham']);
});

// ---------------------------------------------------------------------------------------
// The dictation journal on disk, and the retention posture it enforces
// ---------------------------------------------------------------------------------------

test('a corrupt journal line costs one dictation, never the whole journal', () => {
  const rows = parseJournalFile([
    '{"id":"a","at":1,"text":"one"}',
    'not json at all',
    '{"id":"b","at":2}',            // no text
    '{"id":"c","at":2,"text":"two"}',
    '',
  ].join('\n'));
  assert.deepEqual(rows.map((r) => r.id), ['a', 'c']);
});

test('a missing journal is an EMPTY journal, never a crash — the loop degrades, it does not break', () => {
  assert.deepEqual(loadJournal({ root: path.join(os.tmpdir(), 'no-such-journal-dir-xyz') }), []);
  const l = loadLedger(path.join(os.tmpdir(), 'no-such-journal-dir-xyz'));
  assert.deepEqual(l, { suppressed: [], declined: {}, reconciled: [] });
});

test('the ask ledger round-trips on disk and stores the suppression list sorted, for a human to read', () => {
  const root = fs.mkdtempSync(path.join(os.tmpdir(), 'dictjournal-'));
  saveLedger({ suppressed: ['z=>Z', 'a=>A'], declined: { 'q=>Q': 2 }, reconciled: ['e1'] }, root);
  const back = loadLedger(root);
  assert.deepEqual(back.suppressed, ['a=>A', 'z=>Z']);
  assert.equal(back.declined['q=>Q'], 2);
  assert.deepEqual(back.reconciled, ['e1']);
  fs.rmSync(root, { recursive: true, force: true });
});

test('reconciling an entry marks it consumed WITHOUT rewriting what open-wispr appended', () => {
  const ledger = markReconciled({ reconciled: [] }, 'e1');
  const rows = withConsumed([{ id: 'e1', at: 1, text: 'x' }, { id: 'e2', at: 2, text: 'y' }], ledger);
  assert.equal(rows[0].consumed, true);
  assert.equal(rows[1].consumed, undefined);
});

test('Tier A evicts whole day files past the window, oldest first, by unlink and nothing else', () => {
  const now = Date.parse('2026-08-29T12:00:00Z');
  const days = [
    { day: '2026-08-01', records: 10, bytes: 1000 },
    { day: '2026-08-20', records: 10, bytes: 1000 },
    { day: '2026-08-29', records: 10, bytes: 1000 },
  ];
  const plan = planRetention(days, { audio: [] }, { now, textDays: 14 });
  assert.deepEqual(plan.evictDays.map((d) => d.day), ['2026-08-01']);
  assert.equal(plan.keptRecords, 20);
  assert.match(plan.evictDays[0].why, /older than 14 days/);
});

test('Tier A also binds on the record ceiling, and says which limit bound', () => {
  const now = Date.parse('2026-08-29T12:00:00Z');
  const days = [
    { day: '2026-08-27', records: 400, bytes: 1 },
    { day: '2026-08-28', records: 400, bytes: 1 },
    { day: '2026-08-29', records: 400, bytes: 1 },
  ];
  const plan = planRetention(days, { audio: [] }, { now, textDays: 14, textRecords: 900 });
  assert.deepEqual(plan.evictDays.map((d) => d.day), ['2026-08-27']);
  assert.match(plan.evictDays[0].why, /record ceiling/);
  assert.equal(plan.keptRecords, 800);
});

test('Tier B audio binds on days OR bytes, whichever comes first — the techy-mode numbers, unchanged', () => {
  const now = Date.parse('2026-08-29T12:00:00Z');
  const audio = [
    { id: 'old', at: now - 20 * 24 * 3600 * 1000, bytes: 10 },
    { id: 'big1', at: now - 3 * 24 * 3600 * 1000, bytes: 800 },
    { id: 'big2', at: now - 1 * 24 * 3600 * 1000, bytes: 800 },
  ];
  const plan = planRetention([], { audio }, { now, audioDays: 14, audioBytes: 1000 });
  assert.deepEqual(plan.evictAudio.map((a) => a.id), ['old', 'big1']);
  assert.equal(plan.keptAudioBytes, 800);
  assert.match(plan.evictAudio[0].why, /older than 14 days/);
  assert.match(plan.evictAudio[1].why, /byte ceiling/);
});

test('Tier B is OFF by default, so the flywheel costs zero bytes of retained audio', () => {
  const root = fs.mkdtempSync(path.join(os.tmpdir(), 'dictjournal-'));
  fs.writeFileSync(path.join(root, '2026-08-29.jsonl'), `${JSON.stringify({ id: 'a', at: Date.now(), ms: 3000, text: 'hello' })}\n`);
  const survey = surveyJournal(root);
  assert.equal(survey.audio.length, 0, 'no audio directory exists unless retention was switched on');
  assert.equal(survey.days.length, 1);
  fs.rmSync(root, { recursive: true, force: true });
});

test('the retention sweep is a DRY RUN by default — a policy that erases his speech is readable first', () => {
  const root = fs.mkdtempSync(path.join(os.tmpdir(), 'dictjournal-'));
  fs.writeFileSync(path.join(root, '2020-01-01.jsonl'), `${JSON.stringify({ id: 'a', at: 1577836800000, text: 'old' })}\n`);
  const dry = sweepRetention({ root });
  assert.equal(dry.dryRun, true);
  assert.equal(dry.evictDays.length, 1);
  assert.ok(fs.existsSync(path.join(root, '2020-01-01.jsonl')), 'still there — nothing was deleted');
  const wet = sweepRetention({ root, dryRun: false });
  assert.equal(wet.dryRun, false);
  assert.ok(!fs.existsSync(path.join(root, '2020-01-01.jsonl')), 'and --apply really does unlink it');
  fs.rmSync(root, { recursive: true, force: true });
});

test('a record whose audio aged out still READS — an honest degrade, never a silent blank', () => {
  const rec = { id: 'a', at: 1, ms: 3000, text: 'the Deepgram numbers', audio: null };
  const rows = withConsumed([rec], { reconciled: [] });
  assert.equal(rows[0].text, 'the Deepgram numbers');
  assert.equal(rows[0].audio, null, 'the pointer is null, and null is the answer, not an error');
});

test('the SERVICE sweeps the journal, and it is the only thing that deletes from it', () => {
  // open-wispr appends and never rewrites; this long-running service evicts and never edits. The
  // split is what keeps a retention pass an unlink of a whole day file rather than a rewrite of the
  // CEO\'s speech.
  const root = fs.mkdtempSync(path.join(os.tmpdir(), 'dictjournal-'));
  fs.writeFileSync(path.join(root, '2019-05-05.jsonl'), `${JSON.stringify({ id: 'a', at: 1557014400000, ms: 1000, text: 'ancient' })}\n`);
  const today = new Date().toISOString().slice(0, 10);
  fs.writeFileSync(path.join(root, `${today}.jsonl`), `${JSON.stringify({ id: 'b', at: Date.now(), ms: 1000, text: 'fresh' })}\n`);
  const r = sweepDictationRetention({ root });
  assert.equal(r.dryRun, false, 'the scheduled sweep really evicts');
  assert.deepEqual(r.evictDays.map((d) => d.day), ['2019-05-05']);
  assert.ok(!fs.existsSync(path.join(root, '2019-05-05.jsonl')));
  assert.ok(fs.existsSync(path.join(root, `${today}.jsonl`)), "today's dictation is untouched");
  fs.rmSync(root, { recursive: true, force: true });
});

test('a journal that cannot be swept is a disk problem, never a reason to stop transcribing', () => {
  assert.doesNotThrow(() => sweepDictationRetention({ root: '/dev/null/not-a-directory' }));
});

test('costPerHour is measured from real records, not estimated', () => {
  const cost = costPerHour([
    { id: 'a', at: 1, ms: 1_800_000, text: 'x'.repeat(100) },
    { id: 'b', at: 2, ms: 1_800_000, text: 'y'.repeat(100) },
  ]);
  assert.equal(cost.records, 2);
  assert.equal(cost.spokenMs, 3_600_000, 'exactly one hour of dictation');
  assert.equal(cost.textBytesPerHour, cost.textBytes, 'so bytes/hour IS the measured byte total');
  assert.equal(cost.audioBytesPerHour, 0, 'Tier B off');
});


// ---------------------------------------------------------------------------------------
// THE SHARED GATE FIXTURE — the other half of the anti-drift pair
// ---------------------------------------------------------------------------------------
//
// ceo-decisions.md §7's gate now has TWO implementations: this one, and the Rust port in
// `app/crates/richos-core/src/spoken.rs` that decides whether a SPOKEN utterance is worth
// asking about. Both write into the same `loro/entities.json`, so a divergence does not
// produce two answers — it produces one vocabulary poisoned by whichever half was wrong.
//
// Neither side owns the answer. `test/gate-fixture.mjs` writes the answers down once,
// generated from THIS module, and both sides assert against the same bytes. These cases
// are what goes red if the JS moves; `cargo test -p richos-core --test
// spoken_gate_agreement` is what goes red if the Rust does.

const GATE_FIXTURE = JSON.parse(
  fs.readFileSync(path.join(import.meta.dirname, 'fixtures', 'correction-gate.json'), 'utf8'),
);

test('the shipped gate still gives the answers both implementations are held to', () => {
  assert.ok(GATE_FIXTURE.pairs.length >= 16, 'an empty fixture would pass every assertion below');
  for (const p of GATE_FIXTURE.pairs) {
    assert.equal(normalizeTerm(p.from), p.normalizedFrom, `normalizeTerm("${p.from}")`);
    assert.equal(normalizeTerm(p.to), p.normalizedTo, `normalizeTerm("${p.to}")`);
    assert.equal(phoneticKey(p.from), p.phoneticKeyFrom, `phoneticKey("${p.from}")`);
    assert.equal(phoneticKey(p.to), p.phoneticKeyTo, `phoneticKey("${p.to}")`);
    assert.equal(
      similarity(normalizeTerm(p.from), normalizeTerm(p.to)),
      p.orthographic,
      `similarity("${p.from}","${p.to}")`,
    );
    assert.equal(phoneticSimilarity(p.from, p.to), p.phonetic, `phoneticSimilarity("${p.from}","${p.to}")`);
    assert.equal(askKey(p.from, p.to), p.key, `askKey("${p.from}","${p.to}")`);
  }
  for (const s of GATE_FIXTURE.spans) {
    assert.equal(looksLikeTerm(s.text), s.looksLikeTerm, `looksLikeTerm("${s.text}")`);
  }
  assert.equal(GATE_FIXTURE.floors.askMinPhonetic, ASK_MIN_PHONETIC, 'the phonetic floor moved');
});

test('the gate fixture is regenerable, and regenerating it reproduces it byte for byte', () => {
  // A fixture nobody can rebuild is a set of magic numbers. This runs the generator and
  // compares the bytes, so "regenerate it" stays a real instruction rather than a comment,
  // and a generator that has rotted is caught here rather than the next time somebody
  // needs it.
  //
  // IT WRITES TO A TEMP PATH, NEVER TO THE COMMITTED FIXTURE. The first version of this
  // test invoked the generator with no argument, so a MOVED gate rewrote the committed
  // file, failed this comparison once, and passed for ever afterwards against its own new
  // answers. A test that repairs the evidence it is checking launders a regression into a
  // pass.
  const committed = path.join(import.meta.dirname, 'fixtures', 'correction-gate.json');
  const scratch = fs.mkdtempSync(path.join(os.tmpdir(), 'richos-gate-'));
  const rebuilt = path.join(scratch, 'correction-gate.json');
  try {
    execFileSync(process.execPath, [path.join(import.meta.dirname, 'gate-fixture.mjs'), rebuilt], {
      stdio: 'ignore',
    });
    assert.equal(
      fs.readFileSync(rebuilt, 'utf8'),
      fs.readFileSync(committed, 'utf8'),
      'regenerating the fixture changed it — the gate has moved',
    );
  } finally {
    fs.rmSync(scratch, { recursive: true, force: true });
  }
});

// ---------------------------------------------------------------------------------------
group('model resolution — resolve to a model, not to a directory listing');

/**
 * Two real model directories on the search path: an explicit RICHOS_MODEL_DIR first, then
 * `<home>/Models/Whisper`. Env is restored whatever happens.
 */
function withModelDirs(fn) {
  const root = tmp();
  const first = path.join(root, 'first');
  const home = path.join(root, 'home');
  const second = path.join(home, 'Models', 'Whisper');
  fs.mkdirSync(first, { recursive: true });
  fs.mkdirSync(second, { recursive: true });
  const saved = {
    RICHOS_MODEL_DIR: process.env.RICHOS_MODEL_DIR,
    RICHOS_WHISPER_MODEL: process.env.RICHOS_WHISPER_MODEL,
    HOME: process.env.HOME,
  };
  process.env.RICHOS_MODEL_DIR = first;
  process.env.HOME = home;
  delete process.env.RICHOS_WHISPER_MODEL;
  try {
    return fn({ root, first, second, home });
  } finally {
    for (const [k, v] of Object.entries(saved)) {
      if (v === undefined) delete process.env[k];
      else process.env[k] = v;
    }
    fs.rmSync(root, { recursive: true, force: true });
  }
}

test('a captive-portal page in the first search directory no longer beats a real model in the second', () => {
  // `resolveModel` answers "the first file with this name that exists", which is a directory
  // listing wearing the word resolve. This is the whole difference between the two functions.
  withModelDirs(({ first, second }) => {
    const pin = pinFor('small.en');
    fs.writeFileSync(path.join(first, pin.file), CAPTIVE_PORTAL);
    sparseModel(path.join(second, pin.file), pin.bytes); // 487 MB of nothing, instantly
    assert.equal(resolveModel('small.en'), path.join(first, pin.file), 'the old resolver takes the login page');
    const checked = resolveModelChecked('small.en');
    assert.equal(checked.path, path.join(second, pin.file), 'a rejected candidate must not end the search');
    assert.equal(checked.rejected.length, 1);
    assert.match(checked.rejected[0].message, /came back as a web page/);
  });
});

test('resolveModelChecked names every file it rejected, so the error explains itself', () => {
  withModelDirs(({ first }) => {
    fs.writeFileSync(path.join(first, 'ggml-large-v3-turbo.bin'), Buffer.concat([GGML, Buffer.alloc(500, 1)]));
    assert.throws(
      () => resolveModelChecked('large-v3-turbo'),
      (err) =>
        /download that never finished/.test(err.message) && /504 of the 1,624,555,275 bytes/.test(err.message),
    );
  });
});

test('an explicit RICHOS_WHISPER_MODEL override is honoured, never silently replaced', () => {
  // Overriding is the whole point of an override: the operator saying "use THIS file" outranks our
  // opinion of it. It is still reportable — `richos-service models --deep` will hash it — but it is
  // not quietly swapped for something we like better.
  withModelDirs(({ first, second }) => {
    const mine = path.join(first, 'my-own.bin');
    fs.writeFileSync(mine, fakeModel('override'));
    sparseModel(path.join(second, 'ggml-small.en.bin'), pinFor('small.en').bytes);
    process.env.RICHOS_WHISPER_MODEL = mine;
    const got = resolveModelChecked('small.en');
    assert.equal(got.path, mine);
    assert.equal(got.pin.id, 'small.en', 'the pin is still reported, so a caller can check it if it wants to');
  });
});

test('both resolvers walk one search path, in one order, from one definition', () => {
  withModelDirs(({ first, home }) => {
    const dirs = modelSearchDirs();
    assert.equal(dirs[0], first, 'an explicit RICHOS_MODEL_DIR is searched first');
    assert.equal(dirs[1], path.join(home, 'Models', 'Whisper'));
    assert.ok(dirs.some((d) => d.includes('open-wispr')), "the dictation app's own model dir stays on the path");
  });
});

test('`richos-service verify-model` exits non-zero on a file that does not match its pin', () => {
  // The command a human runs when they suspect a model. An exit code is what makes it usable from
  // a script, and a wrong exit code is worse than none.
  withModelDirs(({ first }) => {
    const pin = pinFor('small.en');
    sparseModel(path.join(first, pin.file), pin.bytes); // right size, right magic, wrong bytes
    const cli = path.join(import.meta.dirname, '..', 'bin', 'richos-service.js');
    let code = 0;
    let out = '';
    try {
      out = execFileSync(process.execPath, [cli, 'verify-model', 'small.en', '--dir', first], { encoding: 'utf8' });
    } catch (err) {
      code = err.status;
      out = String(err.stdout || '');
    }
    assert.equal(code, 1, 'a model that fails its hash must not exit 0');
    assert.match(out, /FAIL/);
    assert.match(out, /not the bytes RichOS pinned/);
  });
});


// ---------------------------------------------------------------------------------------
group('whisper toolchain — which binary, which backends, which weights (2026-09-10)');

// A fake `whisper-cli`: a shell script that prints exactly what a real one prints. Real enough,
// because everything under test consumes the two OUTPUT STREAMS and the file's bytes, and a
// script gives both plus a hash that changes when we want it to. The real binaries are exercised
// by the demonstration in the commit record; these are the cases a suite must be able to run on
// any machine, including one with no whisper installed at all.
function fakeWhisper(dir, { version = '1.9.1', backends = ['BLAS', 'MTL', 'CPU'], reject = false, salt = '' }) {
  const p = path.join(dir, 'whisper-cli');
  // A rejecting build emits NO backend lines, because it dies in argument parsing before it ever
  // loads one. Measured on the real thing, 2026-09-10: `whisper-cpp 1.8.3 --version` produced
  // zero `load_backend:` lines and exited 0. A fake that printed them anyway would be a fake of a
  // binary that does not exist, and the test built on it would prove nothing.
  const emitted = reject ? [] : backends;
  const backendLines = emitted
    .map((b) => `echo "load_backend: loaded ${b} backend from ${dir}/libggml-${b.toLowerCase()}.so" >&2`)
    .join('\n');
  const versionLine = reject
    ? 'echo "error: unknown argument: --version" >&2'
    : `echo "whisper.cpp version: ${version}"`;
  fs.writeFileSync(p, `#!/bin/sh\n# ${salt}\n${backendLines}\n${versionLine}\nexit 0\n`);
  fs.chmodSync(p, 0o755);
  for (const b of backends) fs.writeFileSync(path.join(dir, `libggml-${b.toLowerCase()}.so`), `${b}-backend-${salt}`);
  return p;
}

test('parseVersion reads the version off STDOUT and refuses to invent one from anything else', () => {
  assert.equal(parseVersion('whisper.cpp version: 1.9.1\n'), '1.9.1');
  assert.equal(parseVersion('whisper.cpp version:1.8.3'), '1.8.3');
  assert.equal(parseVersion(''), null);
  // The measured trap: whisper-cpp 1.8.3 exits 0, prints its usage banner to STDERR, and leaves
  // stdout empty. Anything that fell back to stderr would read a version out of the banner.
  assert.equal(parseVersion('error: unknown argument: --version\nusage: whisper-cli ...'), null);
});

test('a build that exits 0 while refusing --version is VERSION_UNKNOWN, never a silent empty string', () => {
  const dir = tmp();
  const bin = fakeWhisper(dir, { reject: true });
  const observed = probeWhisper(bin);
  assert.equal(observed.bin.version, null);
  assert.equal(observed.bin.probeExitStatus, 0, 'the trap is precisely that it exits ZERO');
  assert.equal(observed.bin.probeStderrFirstLine, 'error: unknown argument: --version');
  const r = compareToolchain({ observed, locked: null, reference: null });
  const kinds = r.findings.map((f) => f.kind);
  assert.ok(kinds.includes(TOOLCHAIN_FINDING.VERSION_UNKNOWN));
  // ...and the sha256 is what the run is attributed to instead.
  assert.match(String(observed.bin.sha256), /^[0-9a-f]{64}$/);
});

test('parseBackends reads the backends the binary itself says it loaded, in order', () => {
  const out = parseBackends(
    [
      'ggml_metal_device_init: has tensor            = false',
      'load_backend: loaded BLAS backend from /opt/homebrew/Cellar/ggml/0.17.0/libexec/libggml-blas.so',
      'load_backend: loaded MTL backend from /opt/homebrew/Cellar/ggml/0.17.0/libexec/libggml-metal.so',
      'load_backend: loaded CPU backend from /opt/homebrew/Cellar/ggml/0.17.0/libexec/libggml-cpu-apple_m4.so',
    ].join('\n'),
  );
  assert.deepEqual(out.map((b) => b.name), ['BLAS', 'MTL', 'CPU']);
  assert.equal(out[2].path, '/opt/homebrew/Cellar/ggml/0.17.0/libexec/libggml-cpu-apple_m4.so');
  assert.deepEqual(parseBackends(''), []);
});

test('a FIRST install has nothing to compare against, so it records and proceeds — never refuses', () => {
  const dir = tmp();
  const observed = probeWhisper(fakeWhisper(dir, {}));
  const r = compareToolchain({ observed, locked: null, reference: null });
  assert.equal(r.verdict, 'ok', 'a fresh machine must not be refused for having no history');
  assert.deepEqual(r.findings.map((f) => f.kind), [TOOLCHAIN_FINDING.FIRST_LOCK]);
  assert.equal(severityOf(TOOLCHAIN_FINDING.FIRST_LOCK), 'note');
  assert.match(describeFinding(r.findings[0]), /first install has no earlier identity to be wrong about/);
});

test('a CHANGED BINARY warns, and the message names BOTH identities — old and new', () => {
  const a = tmp();
  const b = tmp();
  const before = probeWhisper(fakeWhisper(a, { version: '1.9.1', salt: 'a' }));
  const after = probeWhisper(fakeWhisper(b, { version: '1.8.3', salt: 'b' }));
  assert.notEqual(before.bin.sha256, after.bin.sha256);
  const locked = buildLock({ observed: before });
  const r = compareToolchain({ observed: after, locked, reference: null });
  assert.equal(r.verdict, 'warn', 'a Homebrew upgrade must not make an already-captured call untranscribable');
  const changed = r.findings.find((f) => f.kind === TOOLCHAIN_FINDING.BIN_CHANGED);
  assert.ok(changed, 'the binary change must be its own finding');
  const msg = describeFinding(changed);
  assert.match(msg, /1\.9\.1/, 'names the version it WAS');
  assert.match(msg, /1\.8\.3/, 'names the version it IS');
  assert.match(msg, new RegExp(before.bin.sha256.slice(0, 12)), 'names the hash it WAS');
  assert.match(msg, new RegExp(after.bin.sha256.slice(0, 12)), 'names the hash it IS');
});

test('a changed ggml BACKEND is caught even when the binary itself is byte-identical', () => {
  // The exposure §4 names separately: the compute backends load at run time from their own
  // formula, so ggml can move with whisper-cpp untouched.
  const dir = tmp();
  const bin = fakeWhisper(dir, { salt: 'same' });
  const before = probeWhisper(bin);
  fs.writeFileSync(path.join(dir, 'libggml-mtl.so'), 'MTL-backend-REBUILT');
  const after = probeWhisper(bin);
  assert.equal(before.bin.sha256, after.bin.sha256, 'the binary must be unchanged for this test to mean anything');
  const r = compareToolchain({ observed: after, locked: buildLock({ observed: before }), reference: null });
  const f = r.findings.find((x) => x.kind === TOOLCHAIN_FINDING.BACKEND_CHANGED);
  assert.ok(f, 'a backend swap under an unchanged binary must still be caught');
  assert.equal(f.backend, 'MTL');
  assert.equal(r.verdict, 'warn');
});

test('a build that reports NO backends is not reported as having lost them', () => {
  // "Nothing observed" is not "changed". The binary hash already carries that case, and a second
  // finding built on an absence would be noise on exactly the run that needs to be read.
  const a = tmp();
  const b = tmp();
  const withBackends = probeWhisper(fakeWhisper(a, { salt: 'a' }));
  const without = probeWhisper(fakeWhisper(b, { backends: [], reject: true, salt: 'b' }));
  const r = compareToolchain({ observed: without, locked: buildLock({ observed: withBackends }), reference: null });
  assert.equal(r.findings.filter((f) => f.kind === TOOLCHAIN_FINDING.BACKEND_CHANGED).length, 0);
});

test('a version off the REFERENCE build warns and names the build the settings were measured on', () => {
  const dir = tmp();
  const observed = probeWhisper(fakeWhisper(dir, { version: '2.0.0' }));
  const r = compareToolchain({
    observed,
    locked: buildLock({ observed }),
    reference: { whisperCppVersion: '1.9.1', measuredOn: '2026-09-10' },
  });
  const f = r.findings.find((x) => x.kind === TOOLCHAIN_FINDING.VERSION_OFF_REFERENCE);
  assert.ok(f);
  assert.match(describeFinding(f), /measured against 1\.9\.1/);
  assert.match(describeFinding(f), /2\.0\.0/);
});

test('the reference block lives in model-pins.json — ONE registry, not a second one', () => {
  // The constraint the brief set: extend the existing pin file rather than invent a parallel
  // mechanism. This asserts the reference is read from the same source the six model pins are.
  assert.ok(TOOLCHAIN_REFERENCE, 'model-pins.json must carry a toolchain block');
  assert.equal(TOOLCHAIN_REFERENCE.whisperCppVersion, '1.9.1');
  assert.match(TOOLCHAIN_REFERENCE.referenceBinary.sha256, /^[0-9a-f]{64}$/);
  assert.ok(TOOLCHAIN_REFERENCE.referenceBackends.length >= 1);
  const raw = JSON.parse(fs.readFileSync(PIN_FILE, 'utf8'));
  assert.equal(raw.toolchain.whisperCppVersion, TOOLCHAIN_REFERENCE.whisperCppVersion);
  assert.equal(raw.models.length, 6, 'the six model pins are still in the same file');
});

test('WRONG WEIGHTS of the RIGHT SIZE and the right magic are REFUSED — the case size+magic passes', () => {
  const pin = pinFor('small.en');
  const f = compareModel({
    modelId: 'small.en',
    filePath: '/tmp/ggml-small.en.bin',
    sha256: '46f780af19a3e6d84eaa5ab4798e42e99c712a47d9625b9db64348f76ff4fbbf',
    pin,
  });
  assert.ok(f);
  assert.equal(f.kind, TOOLCHAIN_FINDING.MODEL_HASH_MISMATCH);
  assert.equal(f.severity, 'refuse', 'a source pin has authority over what the bytes should be');
  const msg = describeFinding(f);
  assert.match(msg, /REFUSING TO TRANSCRIBE/);
  assert.match(msg, /c6138d6d58ec/, 'names the hash it expected');
  assert.match(msg, /46f780af19a3/, 'names the hash it found');
  assert.match(msg, /audio is untouched and retained/);
});

test('the RIGHT weights produce no finding at all', () => {
  const pin = pinFor('small.en');
  assert.equal(compareModel({ modelId: 'small.en', filePath: '/x', sha256: pin.sha256, pin }), null);
  assert.equal(compareModel({ modelId: 'small.en', filePath: '/x', sha256: pin.sha256.toUpperCase(), pin }), null);
});

test('an UNPINNED model is recorded and warned about, not refused — an override is meant to override', () => {
  const f = compareModel({ modelId: 'my-own-model', filePath: '/x/ggml-my-own-model.bin', sha256: 'ab'.repeat(32), pin: null });
  assert.equal(f.kind, TOOLCHAIN_FINDING.MODEL_UNPINNED);
  assert.equal(f.severity, 'warn');
  assert.match(describeFinding(f), /recorded rather than refused/);
});

test('strict mode escalates every warning to a refusal, and leaves refusals refusals', () => {
  assert.equal(severityOf(TOOLCHAIN_FINDING.BIN_CHANGED, false), 'warn');
  assert.equal(severityOf(TOOLCHAIN_FINDING.BIN_CHANGED, true), 'refuse');
  assert.equal(severityOf(TOOLCHAIN_FINDING.MODEL_HASH_MISMATCH, true), 'refuse');
  // ...and a note stays a note: a first install is not a failure in any mode.
  assert.equal(severityOf(TOOLCHAIN_FINDING.FIRST_LOCK, true), 'note');
  assert.equal(strictMode({}), false, 'OFF by default, and the default is a decision');
  assert.equal(strictMode({ RICHOS_WHISPER_STRICT_TOOLCHAIN: '1' }), true);
});

test('the strict refusal does not tell the reader their transcript was produced — there is none', () => {
  const f = { kind: TOOLCHAIN_FINDING.BIN_CHANGED, severity: 'refuse', was: 'a'.repeat(64), now: 'b'.repeat(64) };
  const msg = describeFinding(f);
  assert.match(msg, /nothing was transcribed/);
  assert.ok(!/This transcript was produced/.test(msg));
});

test('every finding kind has a severity AND a sentence — a kind nobody described is a silent kind', () => {
  for (const kind of Object.values(TOOLCHAIN_FINDING)) {
    assert.ok(SEVERITY[kind], `${kind} has no severity`);
    const msg = describeFinding({ kind, severity: SEVERITY[kind] });
    assert.ok(msg && msg.length > 20 && !msg.startsWith('toolchain: '), `${kind} has no sentence`);
  }
});

test('the model-hash cache key is WHOLE milliseconds — the Rust consumer cannot express more', () => {
  // A CROSS-CONSUMER CONTRACT, and it was a real defect before it was a test. `statSync` reports
  // sub-millisecond mtime here (1787746219119.914 on the CEO's own ggml-small.en.bin); Rust's
  // `as_millis()` reports 1787746219119. Both correct, never equal — so a lock written by the
  // voice path silently missed on the call path and vice versa, costing a 3.1 s re-hash of a
  // 1.6 GB model on every run with nothing anywhere saying why. Whole ms is the coarsest unit
  // both produce exactly. `app/crates/richos-voice/src/toolchain.rs#file_identity` truncates to
  // the same one.
  const dir = tmp();
  const f = path.join(dir, 'x.bin');
  fs.writeFileSync(f, 'bytes');
  const id = fileIdentity(f);
  assert.equal(Number.isInteger(id.mtimeMs), true, 'a fractional mtime cannot survive the round trip to Rust');
  assert.equal(id.mtimeMs, Math.trunc(fs.statSync(f).mtimeMs));
  // dev is carried, not zeroed: the other side compares it, so a zero here would be a permanent miss.
  assert.equal(Number.isInteger(id.dev), true);
  assert.ok(id.dev !== 0 || process.platform === 'win32');
});

test('sameIdentity is the model-hash cache key, and every field of it is load-bearing', () => {
  const base = { bytes: 100, mtimeMs: 5, ino: 7, dev: 9 };
  assert.equal(sameIdentity(base, { ...base }), true);
  for (const k of ['bytes', 'mtimeMs', 'ino', 'dev']) {
    assert.equal(sameIdentity(base, { ...base, [k]: 999 }), false, `${k} must invalidate the cache`);
  }
  assert.equal(sameIdentity(base, null), false);
  assert.equal(sameIdentity(null, base), false);
});

test('resolveToolchain: first run writes a lock; the second run serves the model hash from it', () => {
  const dir = tmp();
  const bin = fakeWhisper(dir, {});
  const modelPath = path.join(dir, 'ggml-small.en.bin');
  fs.writeFileSync(modelPath, 'not really a model');
  const lockFile = path.join(dir, 'lock.json');

  resetToolchainCache();
  const first = resolveToolchain({ binPath: bin, modelPath, modelId: 'unpinned-test-model', lockFile, memo: false });
  assert.equal(first.firstRun, true);
  assert.equal(first.toolchain.model.hashed, 'fresh');
  assert.ok(fs.existsSync(lockFile));

  resetToolchainCache();
  const second = resolveToolchain({ binPath: bin, modelPath, modelId: 'unpinned-test-model', lockFile, memo: false });
  assert.equal(second.firstRun, false);
  assert.equal(second.toolchain.model.hashed, 'cached', 'an unchanged model must not be re-hashed');
  assert.equal(second.toolchain.model.sha256, first.toolchain.model.sha256);

  // ...and touching the file invalidates it. This is what makes the cache an optimization over a
  // check that happened rather than a substitute for one that did not.
  fs.writeFileSync(modelPath, 'different bytes entirely');
  resetToolchainCache();
  const third = resolveToolchain({ binPath: bin, modelPath, modelId: 'unpinned-test-model', lockFile, memo: false });
  assert.equal(third.toolchain.model.hashed, 'fresh');
  assert.notEqual(third.toolchain.model.sha256, first.toolchain.model.sha256);
});

test('the per-process memo notices a binary swapped UNDER a long-running watcher', () => {
  // `richos-service watch` is a daemon and can be up for days. A memo keyed on paths alone would
  // check the toolchain at the first call of the week and never see a `brew upgrade` on the
  // Tuesday — and every later transcript would carry a CONFIDENT provenance line naming a binary
  // no longer on the machine, which is worse than the gap this module closes. The memo is keyed on
  // the files' stat identity, so it hits only while the bytes are the ones it was built from.
  const dir = tmp();
  const bin = path.join(dir, 'whisper-cli');
  fs.writeFileSync(bin, '#!/bin/sh\necho "whisper.cpp version: 1.9.1"\nexit 0\n');
  fs.chmodSync(bin, 0o755);
  const modelPath = path.join(dir, 'ggml-x.bin');
  fs.writeFileSync(modelPath, 'weights');
  const lockFile = path.join(dir, 'lock.json');
  const args = { binPath: bin, modelPath, modelId: 'unpinned-daemon-test', lockFile };

  resetToolchainCache();
  const first = resolveToolchain({ ...args }); // memo ON, as the decode path uses it
  assert.equal(first.toolchain.bin.version, '1.9.1');
  // A second call in the same process, nothing moved: the memo hits and the object is IDENTICAL.
  assert.equal(resolveToolchain({ ...args }), first, 'both channels of one call must probe once');

  // Now swap the binary underneath, exactly as a package manager would, WITHOUT clearing the memo.
  fs.writeFileSync(bin, '#!/bin/sh\necho "whisper.cpp version: 2.0.0"\nexit 0\n');
  fs.chmodSync(bin, 0o755);
  const after = resolveToolchain({ ...args });
  assert.notEqual(after, first, 'a swapped binary must not be served from the memo');
  assert.equal(after.toolchain.bin.version, '2.0.0');
  assert.ok(
    after.findings.some((f) => f.kind === TOOLCHAIN_FINDING.BIN_CHANGED),
    'and the swap must be REPORTED, not merely re-probed',
  );
});

test('a REFUSED run never updates the lock — a guard that caches what it rejected disarms itself', () => {
  const dir = tmp();
  const bin = fakeWhisper(dir, {});
  const pin = pinFor('small.en');
  const modelPath = path.join(dir, 'ggml-small.en.bin');
  fs.writeFileSync(modelPath, 'these are not the pinned weights');
  const lockFile = path.join(dir, 'lock.json');

  resetToolchainCache();
  const r = resolveToolchain({ binPath: bin, modelPath, modelId: 'small.en', lockFile, memo: false });
  assert.equal(r.verdict, 'refuse');
  assert.equal(fs.existsSync(lockFile), false, 'nothing may be written on a refusal');

  // And a SECOND attempt refuses identically rather than sailing past on a cache hit.
  resetToolchainCache();
  const again = resolveToolchain({ binPath: bin, modelPath, modelId: 'small.en', lockFile, memo: false });
  assert.equal(again.verdict, 'refuse');
  assert.equal(again.toolchain.model.hashed, 'fresh');
  assert.notEqual(again.toolchain.model.sha256, pin.sha256);
});

test('assertToolchain THROWS on a refusal, with a code and every finding in the message', () => {
  const dir = tmp();
  const bin = fakeWhisper(dir, {});
  const modelPath = path.join(dir, 'ggml-small.en.bin');
  fs.writeFileSync(modelPath, 'wrong');
  resetToolchainCache();
  assert.throws(
    () => assertToolchain({ binPath: bin, modelPath, modelId: 'small.en', lockFile: path.join(dir, 'l.json'), memo: false }),
    (err) => {
      assert.equal(err.code, 'TOOLCHAIN_REFUSED');
      assert.match(err.message, /not the model RichOS pinned/);
      assert.ok(err.toolchain, 'the thrown error carries the full result for a caller that wants to record it');
      return true;
    },
  );
});

test('the provenance string answers "which binary and which weights" from the string alone', () => {
  const s = provenanceString({
    bin: { version: '1.9.1', sha256: '7dc20e3106d70746d61c419646d9bf87f726a5df7da562e26e8529067119f7b8' },
    backends: [{ name: 'BLAS' }, { name: 'MTL' }, { name: 'CPU' }],
    model: { id: 'large-v3-turbo', sha256: '1fc70f774d38eb169993ac391eea357ef47c88757ef72ee5943879b7e8e2bc69' },
  });
  assert.equal(s, 'whisper.cpp 1.9.1 bin:7dc20e3106d7 [BLAS/MTL/CPU] model:large-v3-turbo@1fc70f774d38');
  // The regression this replaced: a CONSTANT that could never disagree with itself.
  assert.notEqual(s, 'whisper.cpp (whisper-cli)');
  const unversioned = provenanceString({ bin: { version: null, sha256: 'ab'.repeat(32) }, backends: [] });
  assert.match(unversioned, /version not reported/, 'an unversioned build says so rather than looking normal');
});

test('the lock preserves the model cache across a re-lock of the BINARY', () => {
  const observedA = { bin: { sha256: 'a'.repeat(64), version: '1.9.1' }, backends: [] };
  const observedB = { bin: { sha256: 'b'.repeat(64), version: '2.0.0' }, backends: [] };
  const first = buildLock({ observed: observedA, at: new Date(T0) });
  first.models['/m/ggml-x.bin'] = { id: 'x', bytes: 1, mtimeMs: 2, ino: 3, dev: 4, sha256: 'c'.repeat(64) };
  const second = buildLock({ observed: observedB, previous: first, at: new Date(T0 + 1000) });
  assert.equal(second.bin.sha256, 'b'.repeat(64));
  assert.deepEqual(second.models['/m/ggml-x.bin'].sha256, 'c'.repeat(64), 'a binary upgrade must not force a 1.6 GB re-hash');
  assert.equal(second.lockedOn, first.lockedOn, 'lockedOn is when this machine FIRST locked');
  assert.notEqual(second.updatedOn, first.updatedOn);
});

test('the record contract is born with a null toolchain, so a never-run pipeline is visibly missing it', () => {
  const r = upgradeRecord({ sessionId: 's', startedAt: T0 });
  assert.equal(r.pipeline.toolchain, null);
  assert.ok('toolchain' in r.pipeline, 'absent and null are different answers');
});

test('the transcript header carries the provenance, so the artifact a person opens names its origin', () => {
  const md = renderMarkdown(
    { segments: [{ startMs: 0, label: 'Me', text: 'hello' }], speakers: ['Me'] },
    {
      sessionId: 's',
      startedAt: T0,
      // The string a real default run produces, copied from an executed run on 2026-09-10 rather
      // than composed here: full turbo and its q5_0 quantization are different weights under
      // neighboring ids, so a header that named only "turbo" would be ambiguous between them.
      pipeline: { model: 'large-v3-turbo-q5_0', toolchain: { provenance: 'whisper.cpp 1.9.1 bin:7dc20e3106d7 [BLAS/MTL/CPU] model:large-v3-turbo-q5_0@394221709cd5' } },
    },
  );
  assert.match(md, /- \*\*Transcribed by:\*\* whisper\.cpp 1\.9\.1 bin:7dc20e3106d7/);
  assert.match(md, /model:large-v3-turbo-q5_0@394221709cd5/);
  assert.match(md, /- \*\*Model:\*\* large-v3-turbo-q5_0/);
});

// ---------------------------------------------------------------------------------------
await drainAsyncTests();

// ---------------------------------------------------------------------------------------
console.log(`\n${passed} passed, ${failures.length} failed`);
if (failures.length) {
  console.error('\nFAILURES:');
  for (const f of failures) console.error(`- ${f.name}: ${f.err.stack}`);
  process.exit(1);
}

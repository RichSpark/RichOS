/**
 * RichOS local service — the transcription pipeline (surface-independent core, the system architecture §4).
 *
 * Triggered when the drop-zone watcher sees a session whose session.json.status became `closed`
 * (or on `retranscribe`). Every stage is idempotent by sessionId; re-runs never double-ingest.
 *
 *   1. RECONCILE GUARD  -> anomaly? LOUD alarm, pipeline.status="anomaly", STOP. Never silent.
 *   2. NORMALIZE (ffmpeg)   stereo contract -> me.wav / others.wav @ 16 kHz mono
 *   3. TRANSCRIBE (whisper) the DEFAULT_TIER model per channel, with timestamps
 *   4. MERGE by timestamp   + fold in caption speaker labels -> verification.json
 *   3.5 HALLUCINATION GUARD four decode-failure classes; loop / stutter / silence-fabrication are
 *                           repaired, and the ordinal-insertion class is repaired PER MARKER where
 *                           an isolated re-decode shows the audio never carried the numeral
 *   3.7 DELETION DETECTOR   physical speech bursts with NO emitted word, adjudicated by isolated
 *                           re-decode -> DETECT-ONLY alarm (a deletion cannot be repaired here)
 *   3.8 WORD-DENSITY        the class coverage scores as COVERED: a window of real speech carrying
 *                           far fewer words than that much audio can hold -> DETECT-ONLY alarm
 *   5. loro-CORRECTION      P1 seam (identity pass); P4 wires the real corrector
 *   6. EMIT                 transcript.md + verification.json; pipeline.status="ready";
 *                           append the ingest ledger; RETAIN audio for re-transcription
 */

import fs from 'node:fs';
import path from 'node:path';
import { reconcilePipeline } from './reconcile.js';
import { upgradeRecord, PIPELINE_STATUS, hasUsableAudio } from './contract.js';
import {
  normalizeSession,
  ffmpegVersion,
  detectSilence,
  detectSpeechBursts,
  cutSpan,
  measureSpanVolume,
} from './normalize.js';
import { transcribeSession, transcribeClips, transcribeStyled, whisperVersion } from './transcribe.js';
import { advanceLiveTranscription, decodeLongWindow, removeLiveTranscriptionScratch, supportsLiveTranscription, takeLiveNormalization } from './live-transcription.js';
import { restyleChannel, restyleWarnings } from './restyle.js';
import { mergeTranscript, renderMarkdown, verify, wordCount } from './merge.js';
import { correct } from './correct.js';
import { loadEntityMemory } from './entities.js';
import { appendLedger } from './ledger.js';
import { MIN_TRANSCRIPT_WORDS, resolveTier, resolveTierForHost, whisperArgs } from './config.js';
import { guardTranscription, guardWarnings } from './repetition-guard.js';
import { guardDeletions, deletionWarnings, informativeWords, lexicalText, DELETION_GUARD_DEFAULTS } from './deletion-guard.js';
import { guardSubstitution, substitutionWarnings, SUBSTITUTION_GUARD_DEFAULTS } from './substitution-guard.js';
import { diarizeOthers } from './diarize.js';
import { log } from './log.js';
import { assertPrivateTree, privateDirectory, writePrivateFile } from './private-files.js';

export const ARTIFACTS = { transcript: 'transcript.md', verification: 'verification.json' };

/** Read + JSON-parse a session directory's session.json (or null if unreadable). */
export function readRecord(sessionDir) {
  try {
    return JSON.parse(fs.readFileSync(path.join(sessionDir, 'session.json'), 'utf8'));
  } catch {
    return null;
  }
}

/** Read captions.ndjson (absolute-timestamped rows) if present. */
export function readCaptions(sessionDir) {
  const file = path.join(sessionDir, 'captions.ndjson');
  if (!fs.existsSync(file)) return [];
  return fs
    .readFileSync(file, 'utf8')
    .split(/\r?\n/)
    .filter(Boolean)
    .map((l) => {
      try {
        return JSON.parse(l);
      } catch {
        return null;
      }
    })
    .filter(Boolean);
}

/** Sum of on-disk audio-part byte sizes. */
export function audioBytesOnDisk(sessionDir) {
  return fs
    .readdirSync(sessionDir)
    .filter((f) => /^audio-part-\d+\./i.test(f))
    .reduce((sum, f) => sum + fs.statSync(path.join(sessionDir, f)).size, 0);
}

function writeRecord(sessionDir, record) {
  writePrivateFile(path.join(sessionDir, 'session.json'), `${JSON.stringify(record, null, 2)}\n`);
}

/**
 * Run the pipeline over one session directory.
 *
 * @param {string} sessionDir absolute path
 * @param {{model?: string, tier?: string, retranscribe?: boolean, extraArgs?: string[], now?: number,
 *          zone?: string, entityMemory?: object, liveCheckpoints?: boolean}} [opts]
 * @returns {{status: string, transcript?: string, verification?: object, problems?: string[], sessionId: string}}
 */
export function runPipeline(sessionDir, opts = {}) {
  // Explicit CLI sessions and direct callers must obey the same boundary as the drop zone.
  sessionDir = assertPrivateTree(sessionDir);
  const now = opts.now || Date.now();
  // P5 tiering: `tier` (quantized*|turbo|max|low-resource, * = default) or a raw `model` id both resolve here to a
  // concrete { model, decodeArgs, repetitionGuard }. `model` stays supported for backward compat.
  //
  // ASKED OF THE MACHINE SINCE 2026-09-10, not read off a constant. With no `--tier` and no
  // `--model`, `resolveTierForHost` walks the batch ladder against what this host can actually
  // do; with either of them, or with a `RICHOS_WHISPER_MODEL` path override, the caller has
  // already decided and it passes straight through. It NEVER promotes above `DEFAULT_TIER` — that
  // is the CEO decision page §10 ruling and it was decided on WER, fabrication and download size,
  // none of them hardware questions. What it adds is the demotion that never existed: the
  // `low-resource` tier has been in the table since it was written and nothing in this product has
  // ever selected it.
  const tier = resolveTierForHost(opts.tier || opts.model, { env: opts.env });
  const model = tier.model;
  let record = readRecord(sessionDir);
  const sessionId = record?.sessionId || record?.dir || path.basename(sessionDir);

  const hasTranscript = fs.existsSync(path.join(sessionDir, ARTIFACTS.transcript));

  // ---- Stage 1: RECONCILE GUARD (never silent) --------------------------------------------------
  const recon = reconcilePipeline({
    record,
    audioBytesOnDisk: audioBytesOnDisk(sessionDir),
    hasTranscript,
    now,
  });
  const usableAudio = hasUsableAudio(record);
  const alreadyReady = hasTranscript && record?.pipeline?.status === PIPELINE_STATUS.ready && !opts.retranscribe;
  const nativeLive = Boolean(record && supportsLiveTranscription(sessionDir, record));
  const keepPrepared = usableAudio && !alreadyReady && !opts.retranscribe && nativeLive && record.status === 'closed';
  if (record?.status === 'closed') {
    try {
      removeLiveTranscriptionScratch(sessionDir, { keepPrepared });
    } catch {
      log.warn('live transcription staging could not be cleared');
    }
  }
  // A genuine capture anomaly (no audio / captions-only / unreadable) can never yield a transcript.
  if (!usableAudio) {
    const problems = recon.problems.length ? recon.problems : ['no usable audio to transcribe'];
    log.alarm(`${sessionId} — cannot transcribe`, { problems });
    if (record) {
      record = upgradeRecord(record);
      record.pipeline.status = PIPELINE_STATUS.anomaly;
      record.pipeline.problems = problems;
      writeRecord(sessionDir, record);
    }
    return { status: PIPELINE_STATUS.anomaly, problems, sessionId };
  }

  record = upgradeRecord(record);

  // Idempotency: a ready transcript is only re-made on explicit retranscribe.
  if (alreadyReady) {
    log.info(`${sessionId} — transcript already exists; skipping (use retranscribe to re-run)`);
    return { status: PIPELINE_STATUS.ready, sessionId, transcript: path.join(sessionDir, ARTIFACTS.transcript) };
  }

  try {
    // ---- Stage 2: NORMALIZE ---------------------------------------------------------------------
    const prepared = keepPrepared ? takeLiveNormalization(sessionDir) : null;
    const norm = prepared ?? normalizeSession(sessionDir, { workDir: sessionDir });
    if (prepared) log.info('live audio normalization reused', { readyAt: prepared.readyAt });
    log.info(`${sessionId} — normalized ${norm.parts.length} part(s), stereo=${norm.stereo}`);

    // Deterministic silence guard: audio that decoded but carries no energy on either channel is a
    // captured-but-nothing-recorded anomaly — caught here, independent of ASR silence-hallucination.
    const channelPaths = { me: norm.me, others: norm.others };
    const silence = detectSilence(channelPaths);
    if (silence.silent) {
      const problems = [
        `captured audio is digitally silent on BOTH channels (me ${silence.me.maxDb} dB / others ` +
          `${silence.others.maxDb} dB max) — nothing was actually recorded. The session closed but ` +
          'held no sound. Investigate the capture; never treat as a complete transcript.',
      ];
      log.alarm(`${sessionId} — silent capture`, { me: silence.me, others: silence.others });
      record.pipeline.status = PIPELINE_STATUS.anomaly;
      record.pipeline.problems = problems;
      writeRecord(sessionDir, record);
      return { status: PIPELINE_STATUS.anomaly, problems, sessionId };
    }

    // ---- Stage 3: TRANSCRIBE --------------------------------------------------------------------
    // The decode HALF of the hallucination guard is no previous-text conditioning
    // (`config.js#MAX_CONTEXT_TOKENS`, emitted by whisperArgs on EVERY tier since 2026-08-29 —
    // it used to be one tier's private decodeArg and the shipping tiers went without it). A tier's
    // own decodeArgs and the caller's extraArgs still ride in on top and still win.
    const decodeArgs = [...(tier.decodeArgs || []), ...(opts.extraArgs || [])];
    log.info(`${sessionId} — tier=${tier.name} model=${model}${decodeArgs.length ? ` decode=[${decodeArgs.join(' ')}]` : ''}`);
    // A DEMOTION IS SAID OUT LOUD, at warn, once per session. The transcript is about to be worse
    // than it would be on a better machine, and a transcript that is quietly worse with nothing
    // saying so is the defect this work exists to remove. Silent on the top rung, because a
    // machine getting the model it was meant to get is the product working — and a line printed
    // on every healthy run is how a real one stops being read.
    if (tier.hardware && tier.hardware.notice) {
      log.warn(`${sessionId} — ${tier.hardware.notice}`);
      log.warn(
        `${sessionId} — basis=${tier.hardware.basis} rejected=${tier.hardware.rejected ? tier.hardware.rejected.id : 'none'}` +
          ` mem=${tier.hardware.machine.availableBytes}B/${tier.hardware.machine.totalBytes}B` +
          ` (docs/measurements/hardware-model-resolution-2026-09-10/)`,
      );
    }
    const decodeOptions = { model, outDir: sessionDir, extraArgs: decodeArgs };
    let asr;
    if (opts.liveCheckpoints && !opts.retranscribe && nativeLive) {
      try {
        const start = performance.now();
        const result = advanceLiveTranscription(sessionDir, decodeOptions, {
          closed: true,
          recover: (cache, from, to, options) => decodeLongWindow(cache, channelPaths, from, to, options),
        });
        log.info('live transcription finalized', {
          reused: result.reused, decoded: result.decoded, skippedSilentChannels: result.skippedSilentChannels,
          longWindows: result.longWindows,
          elapsedMs: Math.round(performance.now() - start),
        });
        asr = result.asr;
      } catch {
        log.warn('live transcription could not finish; decoding retained audio');
      }
    }
    asr ??= transcribeSession(channelPaths, decodeOptions);
    log.info(`${sessionId} — transcribed: me=${asr.me.length} seg, others=${asr.others.length} seg`);

    // The identity of what just decoded this audio. `transcribeChannel` REFUSED before decoding if
    // the weights were not the pinned weights, so reaching this line means the model is right; what
    // can still be here is a binary or a ggml backend that is not the one this machine locked. That
    // is a WARNING by design (toolchain.js `SEVERITY` says why: a Homebrew upgrade must not leave a
    // captured call untranscribable), and a warning is only worth anything if it is LOUD — so it
    // goes to the log as an alarm AND into verification.warnings, the same vocabulary a reader
    // already meets for fabricated and deleted speech, never a line only a log file ever sees.
    // `findings` and `messages` are parallel by construction (`resolveToolchain` builds the second
    // by mapping the first), so they are zipped by index rather than matched by value — two
    // findings of the same kind on different backends are not interchangeable.
    const toolchainWarnings = (asr.toolchain?.findings || [])
      .map((f, i) => (f.severity === 'warn' ? asr.toolchain.messages[i] || String(f.kind) : null))
      .filter(Boolean);
    for (const w of toolchainWarnings) log.alarm(`${sessionId} — whisper toolchain changed`, { detail: w });
    if (asr.toolchain?.firstRun) {
      log.info(`${sessionId} — whisper toolchain locked for the first time on this machine: ${asr.toolchain.provenance}`);
    }

    // THE PROVENANCE BLOCK, recorded HERE and not at Stage 6, for two reasons. It has to precede
    // `renderMarkdown`, which puts the provenance line in the transcript header. And a run that
    // dies in the guard or the merge should still leave behind what decoded it — the identity of
    // the binary is most worth having on the run that went wrong.
    //
    // Until 2026-09-10 `whisperVersion` held the constant string 'whisper.cpp (whisper-cli)' on
    // every run ever made, so "which binary produced this transcript" was not answerable from the
    // record at all. It is now a sentence for a human; this block is the record a person queries.
    record.pipeline.whisperVersion = asr.whisper || whisperVersion();
    record.pipeline.toolchain = asr.toolchain
      ? {
          provenance: asr.toolchain.provenance,
          verdict: asr.toolchain.verdict,
          checkedAt: asr.toolchain.toolchain.checkedAt,
          strict: asr.toolchain.toolchain.strict,
          whisperCli: asr.toolchain.toolchain.bin,
          backends: asr.toolchain.toolchain.backends,
          // THE WEIGHTS, with their full sha256 — not only the 12 hex characters the provenance
          // SENTENCE carries. `verification.toolchain` below has always recorded this; session.json
          // did not, so the same block written by the same file 550 lines apart disagreed about
          // whether the model's identity is part of the record. It is: a transcript attributed to
          // "whisper.cpp 1.9.1" says nothing about which weights ran, and on 2026-09-10 the answer
          // to that changed for every new call (CEO decision page §10).
          model: asr.toolchain.toolchain.model,
          findings: asr.toolchain.findings,
          messages: asr.toolchain.messages,
        }
      : null;

    // ---- Stage 3.5: HALLUCINATION GUARD (P5) ----------------------------------------------------
    // The post-decode HALF of the hallucination defense, model-agnostic, across FOUR measured
    // decode-failure classes (see lib/repetition-guard.js): a repetition LOOP, a sliding-overlap
    // STUTTER and a SILENCE FABRICATION are removed before they reach the merge/transcript; a
    // persistent ordinal INSERTION is repaired PER MARKER, and only where an isolated re-decode of
    // that segment's own audio fails to return the numeral — because one marker in the captured
    // artifact is a real spoken "Zero." and a blanket strip deletes it. Anything the probe does not
    // clear stays in the text and stays LOUD.
    // Runs on every tier (cheap, precision-guarded); the safety net that lets large-v3 be opt-in.
    let asrGuarded = { me: asr.me, others: asr.others };
    /** @type {{me: object[], others: object[]}|null} the physical evidence the loop class needs */
    let speechBursts = null;
    // Was the insertion class given a way to REPAIR, or could it only ever detect? Recorded on every
    // run, not only the runs where the class fires — otherwise the one transcript that needed the
    // probe is the only place anyone would find out the wiring had come loose.
    let insertionProbeAvailable = false;
    let repetitionReport = {
      removed: 0,
      detected: false,
      classes: { repetition: 0, insertion: 0, overlapStutter: 0, silenceFabrication: 0, preservedByAudio: 0 },
      loops: [],
      preserved: [],
      insertions: [],
      stutters: [],
      silenceFabrications: [],
      silenceRemoved: 0,
      silenceUnrepaired: 0,
      silenceProbed: { me: false, others: false },
      silenceUnit: { me: null, others: null },
      byChannel: { me: {}, others: {} },
    };
    // The PHYSICAL evidence, one ffmpeg pass per channel — 0.68 s for a 92-minute channel, measured
    // 2026-08-29, so it is free at any call length. THREE consumers and it is computed ONCE: the
    // repetition guard's burst veto (class 1: is a repeated phrase a human retake?), the same
    // guard's silence-fabrication class (class 4: was ANY of this span spoken?), and the deletion
    // detector below (is a wordless span a deleted clause?). It used to live
    // inside the `tier.repetitionGuard` branch; it is hoisted because the deletion detector needs it
    // whether or not the repetition classes are enabled, and because "which stage owns the audio
    // probe" is exactly the kind of hidden coupling that goes wrong later.
    // Best-effort: if ffmpeg cannot produce it, the repetition guard falls back to its old text-only
    // behavior and the deletion detector reports honestly that it could not look.
    try {
      const b = {
        me: detectSpeechBursts(channelPaths.me, { peakDb: silence.me.maxDb }).speech,
        others: detectSpeechBursts(channelPaths.others, { peakDb: silence.others.maxDb }).speech,
      };
      speechBursts = b;
      log.info(`${sessionId} — speech bursts: me=${b.me.length} others=${b.others.length}`);
    } catch (err) {
      log.alarm(`${sessionId} — speech-burst probe failed; the repetition guard runs TEXT-ONLY (it may delete genuine repeated speech, and its silence-fabrication class cannot run at all, so invented text over silence WILL reach transcript.md), and the deletion detector cannot run at all`, {
        error: String(err && err.message ? err.message : err),
      });
    }
    const probeDir = path.join(sessionDir, '_probe');
    /**
     * Cut + level + isolated re-decode for one channel's suspect spans. The impure half, and it is
     * ONE function for stages 3.5, 3.7 and 3.8 on purpose: three detectors that cut, measured or
     * decoded their evidence differently would eventually disagree about the same span, and the
     * pipeline would have no way to say which of them was looking at the audio wrong.
     *
     * Deletion and density checks already require a minimum peak. Measure the unpadded span
     * first and skip re-decoding where that existing condition cannot pass.
     */
    const makeProbe = (channel, pads, tag, minPeak = -Infinity) => (spans) => {
      const started = performance.now();
      const wavPath = channelPaths[channel];
      privateDirectory(probeDir, sessionDir);
      const finite = (x) => (Number.isFinite(x) ? x : null);
      const rows = spans.map((s) => {
        const level = measureSpanVolume(wavPath, s);
        return { tight: '', wide: '', maxDb: finite(level.maxDb), meanDb: finite(level.meanDb) };
      });
      const selected = rows.flatMap((row, i) =>
        minPeak === -Infinity || (row.maxDb != null && row.maxDb >= minPeak) ? [i] : []);
      const clips = (indices, suffix, padSec) => indices.map((i) =>
        cutSpan(wavPath, spans[i], path.join(probeDir, `${tag}-${channel}-${i}-${suffix}.wav`), { padSec }));
      const decode = (paths) => transcribeClips(paths, { model, extraArgs: decodeArgs });
      const tightPaths = clips(selected, 't', pads.probePadSec);
      let wide = selected;
      if (tag === 'del') {
        const tight = decode(tightPaths);
        for (const [offset, i] of selected.entries()) rows[i].tight = tight[offset] || '';
        // A deletion needs enough words in the tight decode before a wider
        // decode can establish that those words survive a different cut.
        wide = selected.filter((i) => informativeWords(lexicalText(rows[i].tight)).length >= pads.minProbeWords);
        const texts = decode(clips(wide, 'w', pads.probeWidePadSec));
        for (const [offset, i] of wide.entries()) rows[i].wide = texts[offset] || '';
      } else {
        const texts = decode([...tightPaths, ...clips(wide, 'w', pads.probeWidePadSec)]);
        for (const [offset, i] of selected.entries()) {
          rows[i].tight = texts[offset] || '';
          rows[i].wide = texts[selected.length + offset] || '';
        }
      }
      log.info('acoustic probe finished', {
        stage: tag, channel, candidates: spans.length, tight: selected.length, wide: wide.length,
        elapsedMs: Math.round(performance.now() - started),
      });
      return rows;
    };
    const probeFloor = (channel, opts) => Number.isFinite(silence[channel].maxDb)
      ? silence[channel].maxDb - opts.loudBelowPeakDb
      : -Infinity;
    if (tier.repetitionGuard !== false) {
      // The insertion class's probe. Costs NOTHING unless that class's four detection conditions
      // fire, which they have done on exactly one artifact ever — and when they do, this is what
      // lets the class repair instead of only shouting: each suspect marker is adjudicated against
      // an isolated re-decode of its own audio, and stripped only where the numeral does not come
      // back. Same cut, same paddings, same one-invocation decode as stages 3.7 and 3.8.
      let insertionProbeFailed = null;
      const guarded = guardTranscription(
        { me: asr.me, others: asr.others },
        {
          ...(speechBursts ? { speechBursts } : {}),
          probe: (spans, channel) => {
            try {
              return makeProbe(channel, DELETION_GUARD_DEFAULTS, 'ins')(spans);
            } catch (err) {
              insertionProbeFailed = String(err && err.message ? err.message : err);
              throw err;
            }
          },
        },
      );
      try {
        fs.rmSync(probeDir, { recursive: true, force: true });
      } catch {
        /* the clips are inside the session dir, which already holds the full audio */
      }
      if (insertionProbeFailed) {
        log.alarm(`${sessionId} — the insertion class could not re-decode; fabricated markers are NOT repaired and stay in the transcript`, {
          error: insertionProbeFailed,
        });
      }
      asrGuarded = { me: guarded.me, others: guarded.others };
      repetitionReport = guarded.report;
      // What the GUARD received, not what this file meant to hand it. A flag set here beside the
      // call would stay true through any wiring mistake on the line above.
      insertionProbeAvailable = repetitionReport.insertionProbe === true;
      if (repetitionReport.preserved && repetitionReport.preserved.length) {
        log.info(`${sessionId} — ${repetitionReport.preserved.length} repeated run(s) PRESERVED: the audio contains a speech burst for every repetition`, {
          preserved: repetitionReport.preserved.map((p) => ({ channel: p.channel, count: p.count, text: p.text.slice(0, 60) })),
        });
      }
      if (repetitionReport.silenceRemoved) {
        // The 2026-08-29 corpus put 60.4% of one 126-minute channel's timeline inside this class,
        // so on a real call this line is the difference between a transcript and a fiction.
        log.alarm(`${sessionId} — ${repetitionReport.silenceRemoved} segment(s) of text over MEASURED SILENCE removed by the guard`, {
          byChannel: {
            me: repetitionReport.byChannel.me.silenceRemoved,
            others: repetitionReport.byChannel.others.silenceRemoved,
          },
          seconds: +repetitionReport.silenceFabrications
            .filter((f) => f.action === 'removed')
            .reduce((n, f) => n + f.durationSec, 0)
            .toFixed(1),
          unit: repetitionReport.silenceUnit,
        });
      }
      for (const f of repetitionReport.silenceFabrications) {
        // NOT removed, on purpose: over silence, but not in the silence vocabulary. Still IN the
        // transcript, so it has to be loud — same treatment as the insertion class below.
        if (f.action !== 'reported') continue;
        log.alarm(`${sessionId} — TEXT OVER SILENCE LEFT IN THE TRANSCRIPT (not in the silence vocabulary)`, {
          channel: f.channel,
          fromMs: f.startMs,
          seconds: f.durationSec,
          words: f.words,
          burstOverlapSec: f.burstOverlapSec,
          remedy: 'check the span or re-transcribe; the guard will not guess against a quiet real utterance',
        });
      }
      if (repetitionReport.loops.length) {
        log.alarm(`${sessionId} — repetition loop(s) caught + collapsed by the guard`, {
          removedSegments: repetitionReport.removed,
          loops: repetitionReport.loops.map((l) => ({ channel: l.channel, count: l.count, text: l.text.slice(0, 60) })),
        });
      }
      if (repetitionReport.stutters.length) {
        log.alarm(`${sessionId} — sliding-overlap stutter caught + de-overlapped by the guard`, {
          chains: repetitionReport.stutters.map((s) => ({
            channel: s.channel,
            segments: s.segments,
            removed: s.removed,
            trimmed: s.trimmed,
          })),
        });
      }
      for (const ins of repetitionReport.insertions) {
        const left = Number(ins.keptSpoken || 0) + Number(ins.unprobed || 0);
        if (!ins.repaired) {
          // No probe reached this class, so it could only detect. The transcript still has them all.
          log.alarm(`${sessionId} — FABRICATED TEXT LEFT IN THE TRANSCRIPT (this class could not re-decode)`, {
            channel: ins.channel,
            kind: ins.kind,
            count: ins.count,
            fromMs: ins.startMs,
            toMs: ins.endMs,
            sample: ins.sample,
            remedy: 're-transcribe with another model (audio is retained); with no isolated re-decode the guard will not rewrite text',
          });
          continue;
        }
        // Repaired — and the alarm still fires, because a fabrication this size reaching the decode
        // at all is the operational fact, and because what was NOT repaired belongs beside it.
        log.alarm(`${sessionId} — fabricated ${ins.kind} insertion(s) found and REPAIRED on physical evidence`, {
          channel: ins.channel,
          count: ins.count,
          removed: ins.stripped,
          keptBecauseTheAudioSaysTheSpeakerSaidIt: ins.keptSpoken,
          notAdjudicated: ins.unprobed,
          fromMs: ins.startMs,
          toMs: ins.endMs,
          sample: ins.sample,
          remedy: left
            ? 'the unadjudicated/kept markers are still in the transcript — check the span, or re-transcribe'
            : 'none; every marker was adjudicated against its own audio',
        });
      }
    }

    // ---- Stage 3.6: DIARIZATION SEAM (P5, opt-in; default 'none' = today's behavior) ------------
    // Per-remote-speaker turn attribution for non-caption multi-speaker calls. Default off (no wrong
    // speaker counts); when opted in, splits the RIGHT channel at whisper.cpp tinydiarize turn markers.
    // Caption names still win in the merge — diarized turn labels only fill the gap where none exists.
    const diarizeMethod = opts.diarize || process.env.RICHOS_DIARIZE || 'none';
    const dia = diarizeOthers(asrGuarded.others, { method: diarizeMethod });
    if (dia.method !== 'none') {
      log.info(`${sessionId} — diarization(${dia.method}): ${dia.turns} remote turn(s), identityStable=${dia.identityStable}`);
    }

    // ---- Stage 3.7: DELETION DETECTOR (P5) ------------------------------------------------------
    // The class the three hallucination-guard classes structurally cannot see: the model saying
    // NOTHING over real speech. See lib/deletion-guard.js for the discriminator, the four-condition
    // precision rule, and the enumerated blind spots.
    //
    // Two stages, and only the second one costs anything. Stage A is arithmetic over the burst grid
    // already computed above and the segments already in memory. Stage B re-decodes ONLY the spans
    // stage A found suspect, as clips, through ONE whisper-cli invocation that loads the model once.
    // Nothing here re-transcribes the recording.
    //
    // DETECT-ONLY, like the insertion class: a deletion cannot be repaired from here, so the remedy
    // is a named span in the alarm and in verification.json, and re-transcription against retained
    // audio. `RICHOS_DELETION_GUARD=off` disables it; it changes no decode parameter either way.
    const deletionOn = String(opts.deletionGuard ?? process.env.RICHOS_DELETION_GUARD ?? 'on') !== 'off';
    let deletionReport = {
      detected: false,
      probeAvailable: false,
      coverageUnit: null,
      candidates: 0,
      probed: 0,
      deletedSpans: 0,
      deletedSeconds: 0,
      unprobedSpans: 0,
      deletions: [],
      rejected: [],
      unprobed: [],
      byChannel: {},
      enabled: deletionOn,
    };
    if (deletionOn && speechBursts) {
      try {
        const t0 = Date.now();
        const del = guardDeletions(
          // Post-diarization segments: turn splitting changes labels and boundaries, never extents,
          // so coverage is identical — but the detector must judge the timeline that actually ships.
          { me: asrGuarded.me, others: dia.segments },
          {
            speechBursts,
            peaks: { me: silence.me.maxDb, others: silence.others.maxDb },
            probe: (spans, channel) => makeProbe(channel, DELETION_GUARD_DEFAULTS, 'del',
              probeFloor(channel, DELETION_GUARD_DEFAULTS))(spans),
          },
        );
        deletionReport = { ...del.report, enabled: true, elapsedMs: Date.now() - t0 };
      } catch (err) {
        // A failed probe must never fail the pipeline, and must never look like a clean result.
        log.alarm(`${sessionId} — deletion detector could not run; this transcript is NOT checked for deleted speech`, {
          error: String(err && err.message ? err.message : err),
        });
        deletionReport = { ...deletionReport, probeAvailable: false, error: String(err && err.message ? err.message : err) };
      } finally {
        try {
          fs.rmSync(probeDir, { recursive: true, force: true });
        } catch {
          /* the clips are inside the session dir, which already holds the full audio */
        }
      }
      for (const d of deletionReport.deletions) {
        // NOT repaired, on purpose — see the module header. The transcript below is still missing it.
        log.alarm(`${sessionId} — SPEECH MISSING FROM THE TRANSCRIPT (detect-only class)`, {
          channel: d.channel,
          fromMs: d.startMs,
          toMs: d.endMs,
          seconds: d.durationSec,
          maxDb: d.maxDb,
          recovered: d.recovered,
          remedy: 're-transcribe (audio is retained); a deletion cannot be filled in from here',
        });
      }
      if (deletionReport.unprobedSpans) {
        log.alarm(`${sessionId} — ${deletionReport.unprobedSpans} wordless speech span(s) went UNADJUDICATED — not cleared, not claimed`, {
          spans: deletionReport.unprobed.slice(0, 10).map((u) => ({ channel: u.channel, fromMs: u.startMs, seconds: u.durationSec })),
        });
      }
      log.info(
        `${sessionId} — deletion detector: ${deletionReport.candidates} candidate(s), ${deletionReport.probed} probed, ` +
          `${deletionReport.deletedSpans} deletion(s) / ${deletionReport.deletedSeconds}s (unit=${deletionReport.coverageUnit})`,
      );
    } else if (deletionOn) {
      log.alarm(`${sessionId} — deletion detector SKIPPED: no speech-burst grid, so nothing physical to compare the transcript against`);
    }

    // ---- Stage 3.8: WORD-DENSITY INSTRUMENT (P5) -------------------------------------------------
    // The class stage 3.7 structurally cannot see, named in its own header: SUBSTITUTION scores as
    // perfect COVERAGE, because something is there. This stage asks the other question — is there
    // ENOUGH? — by measuring emitted words against the physical speech budget of the audio under
    // them. See lib/substitution-guard.js for the discriminator, the six-condition precision rule,
    // and what it deliberately does not claim (it can say words are MISSING; it can never say the
    // words that are present are WRONG).
    //
    // It runs AFTER 3.7 and consumes it: every span 3.7 confirmed as a deletion is excluded from
    // this stage's budget, so one failure is never reported twice under two names, and a hole
    // another instrument already owns cannot inflate this one's deficit.
    //
    // Same two stages and the same economics as 3.7: stage A is arithmetic over the burst grid
    // already in memory, stage B re-decodes only the windows stage A found thin, through the same
    // one-invocation clip decoder. DETECT-ONLY. `RICHOS_SUBSTITUTION_GUARD=off` disables it; it
    // changes no decode parameter either way.
    const densityOn = String(opts.substitutionGuard ?? process.env.RICHOS_SUBSTITUTION_GUARD ?? 'on') !== 'off';
    let substitutionReport = {
      detected: false,
      probeAvailable: false,
      coverageUnit: null,
      windows: 0,
      candidates: 0,
      probed: 0,
      sparseSpans: 0,
      sparseSeconds: 0,
      unprobedSpans: 0,
      channelsBelowFloor: [],
      findings: [],
      rejected: [],
      unprobed: [],
      byChannel: {},
      enabled: densityOn,
    };
    if (densityOn && speechBursts) {
      try {
        const t0 = Date.now();
        const excludeSpans = { me: [], others: [] };
        for (const d of deletionReport.deletions || []) {
          if (excludeSpans[d.channel]) excludeSpans[d.channel].push({ startMs: d.startMs, endMs: d.endMs });
        }
        const sub = guardSubstitution(
          // The same post-diarization timeline stage 3.7 judged, for the same reason: the detector
          // must measure the transcript that actually ships.
          { me: asrGuarded.me, others: dia.segments },
          {
            speechBursts,
            peaks: { me: silence.me.maxDb, others: silence.others.maxDb },
            excludeSpans,
            probe: (spans, channel) => makeProbe(channel, SUBSTITUTION_GUARD_DEFAULTS, 'density',
              probeFloor(channel, SUBSTITUTION_GUARD_DEFAULTS))(spans),
          },
        );
        substitutionReport = { ...sub.report, enabled: true, elapsedMs: Date.now() - t0 };
      } catch (err) {
        // A failed probe must never fail the pipeline, and must never look like a clean result.
        log.alarm(`${sessionId} — word-density instrument could not run; this transcript is NOT checked for lost words`, {
          error: String(err && err.message ? err.message : err),
        });
        substitutionReport = {
          ...substitutionReport,
          probeAvailable: false,
          error: String(err && err.message ? err.message : err),
        };
      } finally {
        try {
          fs.rmSync(probeDir, { recursive: true, force: true });
        } catch {
          /* the clips are inside the session dir, which already holds the full audio */
        }
      }
      for (const f of substitutionReport.findings) {
        // NOT repaired, on purpose — see the module header. The transcript below is still thin here.
        log.alarm(`${sessionId} — SPEECH UNDER-TRANSCRIBED (detect-only class): the audio holds more words than the transcript does`, {
          channel: f.channel,
          fromMs: f.startMs,
          toMs: f.endMs,
          speechSeconds: f.speechSec,
          emittedWords: f.emittedWords,
          wordsPerSecond: f.density,
          isolatedDecodeWords: f.probeWords,
          recovered: f.recovered,
          remedy: 're-transcribe (audio is retained); whether the words PRESENT are wrong cannot be decided without a reference',
        });
      }
      for (const c of substitutionReport.channelsBelowFloor) {
        log.alarm(`${sessionId} — the WHOLE "${c.channel}" channel is below the conversational word-density floor`, {
          medianWordsPerSecond: c.medianDensity,
          floorWordsPerSecond: c.floorWordsPerSec,
          windows: c.windows,
          remedy: 're-transcribe this channel, optionally with a different model; a per-window comparison cannot see this',
        });
      }
      log.info(
        `${sessionId} — word-density instrument: ${substitutionReport.windows} window(s), ${substitutionReport.candidates} candidate(s), ` +
          `${substitutionReport.probed} probed, ${substitutionReport.sparseSpans} under-transcribed span(s) / ${substitutionReport.sparseSeconds}s`,
      );
    } else if (densityOn) {
      log.alarm(`${sessionId} — word-density instrument SKIPPED: no speech-burst grid, so there is no physical speech budget to measure the transcript against`);
    }

    // ---- Stage 3.9: RESTYLE run-on windows (punctuation and case only) ---------------------------
    // At MAX_CONTEXT_TOKENS = 0 every 30-second window is decoded with no text before it, and a
    // window that starts mid-sentence comes out lowercase and unpunctuated about a third of the time
    // (lib/restyle.js has the measurement). Each such span is re-decoded ONCE, as one clip,
    // conditioned on the transcript's own preceding text, and only its punctuation and case are
    // taken — never a word. AFTER every detector on purpose: 3.5–3.8 judged exactly the words that
    // still ship. `RICHOS_RESTYLE=off` disables it; the main-pass decode is identical either way.
    const restyleOn = String(opts.restyle ?? process.env.RICHOS_RESTYLE ?? 'on') !== 'off';
    let restyleReport = { enabled: restyleOn, spans: 0, chunks: 0, restyled: 0, words: 0, unrestored: [], elapsedMs: null };
    let shipped = { me: asrGuarded.me, others: dia.segments };
    if (restyleOn) {
      const t0 = Date.now();
      let clipIndex = 0;
      const decodeFor = (channel) => (span, prompt) => {
        privateDirectory(probeDir, sessionDir);
        const clip = cutSpan(channelPaths[channel], span, path.join(probeDir, `sty-${channel}-${clipIndex++}.wav`), { padSec: 0 });
        return transcribeStyled(clip, prompt, { model, extraArgs: decodeArgs });
      };
      try {
        const me = restyleChannel(shipped.me, { decode: decodeFor('me'), channel: 'me' });
        const others = restyleChannel(shipped.others, { decode: decodeFor('others'), channel: 'others' });
        shipped = { me: me.segments, others: others.segments };
        restyleReport = {
          enabled: true,
          spans: me.report.spans + others.report.spans,
          chunks: me.report.chunks + others.report.chunks,
          restyled: me.report.restyled + others.report.restyled,
          words: me.report.words + others.report.words,
          unrestored: [...me.report.unrestored, ...others.report.unrestored],
          elapsedMs: Date.now() - t0,
        };
      } catch (err) {
        // Never fails the pipeline: the main-pass text ships as decoded, and says so.
        log.alarm(`${sessionId} — restyle could not run; run-on spans ship without punctuation and case`, {
          error: String(err && err.message ? err.message : err),
        });
        restyleReport = { ...restyleReport, error: String(err && err.message ? err.message : err) };
      } finally {
        try {
          fs.rmSync(probeDir, { recursive: true, force: true });
        } catch {
          /* the clips are inside the session dir, which already holds the full audio */
        }
      }
      log.info(
        `${sessionId} — restyle: ${restyleReport.spans} run-on span(s), ${restyleReport.restyled}/${restyleReport.chunks} clip(s) ` +
          `given punctuation and case, ${restyleReport.unrestored.length} left as decoded (${restyleReport.elapsedMs} ms)`,
      );
    }

    // ---- Stage 4: MERGE + caption fold-in -------------------------------------------------------
    const captions = readCaptions(sessionDir);
    const merged = mergeTranscript({
      me: shipped.me,
      others: shipped.others,
      captions,
      startedAt: Number(record.startedAt || 0),
    });

    // ---- Stage 5: loro-CORRECTION (P4 real corrector) -------------------------------------------
    // Load loro entity memory by default (single wiring point) so every trigger path — CLI, watcher,
    // host-spawned — gets name/jargon correction; a missing entities file yields an empty memory
    // (identity), never a failure. Tests can inject `opts.entityMemory` for determinism.
    const entityMemory = opts.entityMemory ?? loadEntityMemory();
    const corrected = correct(merged.segments, entityMemory);
    const finalMerged = { ...merged, segments: corrected.segments };
    record.pipeline.loroCorrection = {
      applied: corrected.applied,
      entitiesVersion: corrected.entitiesVersion,
      corrections: corrected.corrections.length,
    };

    // ---- Verification + trivial-transcript anomaly ----------------------------------------------
    record.pipeline.model = model;
    record.pipeline.tier = tier.name;
    // WHY THIS TIER AND NOT A BETTER ONE. `tier` alone cannot answer it: `low-resource` in a
    // record could mean a human passed `--tier low-resource` or that this machine could not carry
    // the default, and those are different facts about the same transcript. `null` when the
    // caller chose the tier outright — an absent block means "not resolved", never "resolved to
    // the top", which is the distinction a `basis: 'top-rung'` row exists to keep.
    record.pipeline.hardware = tier.hardware
      ? {
          basis: tier.hardware.basis,
          rejected: tier.hardware.rejected,
          projectedSecondsPerAudioSecond: tier.hardware.projected,
          totalMemoryBytes: tier.hardware.machine.totalBytes,
          availableMemoryBytes: tier.hardware.machine.availableBytes,
          cores: tier.hardware.machine.cores,
          record: 'docs/measurements/hardware-model-resolution-2026-09-10/',
        }
      : null;
    if (decodeArgs.length) record.pipeline.decodeArgs = decodeArgs;
    // The EFFECTIVE decode invocation, not just the tier's extras. Since `-mc` moved out of tier
    // data (2026-08-29) an empty `decodeArgs` no longer means "bare defaults", and a record that
    // implied it would be lying about the single most load-bearing decode setting we have.
    record.pipeline.whisperArgs = whisperArgs({ extraArgs: decodeArgs });
    record.pipeline.maxContextTokens = Number(
      record.pipeline.whisperArgs[record.pipeline.whisperArgs.lastIndexOf('-mc') + 1],
    );
    record.pipeline.repetitionGuard = {
      enabled: tier.repetitionGuard !== false,
      detected: repetitionReport.detected,
      removedSegments: repetitionReport.removed,
      classes: repetitionReport.classes,
      loops: repetitionReport.loops,
      // Runs the guard chose NOT to touch because the audio backs every repetition. A decision, so
      // it is recorded like one — otherwise "removedSegments: 0" cannot be told from "never looked".
      preserved: repetitionReport.preserved || [],
      speechBurstProbe: speechBursts
        ? { me: speechBursts.me.length, others: speechBursts.others.length }
        : null,
      stutters: repetitionReport.stutters,
      // Class 4. Every span the guard judged to sit over measured silence, both tiers, each row
      // carrying its own `action` — so the removals can never be read without what was left behind.
      silenceFabrications: repetitionReport.silenceFabrications,
      silenceRemoved: repetitionReport.silenceRemoved,
      // Still IN the transcript: over silence, but not in the silence vocabulary. Same meaning as
      // `unrepaired` below — never let "enabled: true" imply "clean".
      silenceUnrepaired: repetitionReport.silenceUnrepaired,
      // "Nothing found" and "never looked" are different answers: class 4 needs the burst grid.
      silenceProbed: repetitionReport.silenceProbed,
      silenceUnit: repetitionReport.silenceUnit,
      // Class 2. REPAIRED per marker where an isolated re-decode proved the audio never carried the
      // numeral, and only there — so `unrepaired` counts what is STILL in the transcript (a numeral
      // the audio backs, or a marker nothing looked at), never the whole finding. Never let
      // "enabled: true" imply "clean", and never let "repaired" imply "all of it".
      insertions: repetitionReport.insertions,
      // "This class could have repaired" vs "this class can only detect" — a property of the WIRING,
      // true even on a clean transcript where the class never fires.
      insertionProbeAvailable,
      insertionsRepaired: repetitionReport.insertionsRepaired || 0,
      insertionsKeptSpoken: repetitionReport.insertionsKeptSpoken || 0,
      unrepaired: repetitionReport.insertions.reduce(
        (n, i) => n + (i.repaired ? Number(i.keptSpoken || 0) + Number(i.unprobed || 0) : i.count),
        0,
      ),
    };
    record.pipeline.deletionGuard = {
      enabled: deletionReport.enabled,
      // "0 deletions" and "never looked" are DIFFERENT ANSWERS and this record must never conflate
      // them: probeAvailable says whether an isolated re-decode was possible at all, coverageUnit
      // says which evidence coverage was scored on, and unrepaired counts what is still missing.
      probeAvailable: deletionReport.probeAvailable,
      coverageUnit: deletionReport.coverageUnit,
      // What this stage COST, in the record, because a detector's price is an operational fact and
      // an unmeasured one gets guessed at. Burst grid + candidate arithmetic + the clip re-decodes.
      elapsedMs: deletionReport.elapsedMs ?? null,
      detected: deletionReport.detected,
      candidates: deletionReport.candidates,
      probed: deletionReport.probed,
      deletedSpans: deletionReport.deletedSpans,
      deletedSeconds: deletionReport.deletedSeconds,
      // Detect-only: every one of these is STILL missing from transcript.md. Never let
      // "enabled: true" imply "complete" — the same vocabulary the insertion class uses.
      unrepaired: deletionReport.deletedSpans,
      deletions: deletionReport.deletions,
      // Wordless speech spans that were adjudicated NOT to be deletions, kept because a rejection
      // is a decision and an empty `deletions` list otherwise cannot be told from an idle detector.
      rejected: deletionReport.rejected,
      unprobed: deletionReport.unprobed,
      byChannel: deletionReport.byChannel,
    };
    record.pipeline.substitutionGuard = {
      enabled: substitutionReport.enabled,
      // Same discipline as the deletion record: "0 findings", "never looked" and "looked at part of
      // it" are three different answers. `windows` and `byChannel[*].analyzedSpeechSec` say how much
      // of the channel had a measurable budget at all; a stretch too sparse in wall time to form a
      // window is never examined, and this record has to admit that rather than imply a clean sweep.
      probeAvailable: substitutionReport.probeAvailable,
      coverageUnit: substitutionReport.coverageUnit,
      elapsedMs: substitutionReport.elapsedMs ?? null,
      detected: substitutionReport.detected,
      windows: substitutionReport.windows,
      candidates: substitutionReport.candidates,
      probed: substitutionReport.probed,
      sparseSpans: substitutionReport.sparseSpans,
      sparseSeconds: substitutionReport.sparseSeconds,
      // Detect-only: every one of these is STILL thin in transcript.md. Same vocabulary as the
      // other two detect-only classes.
      unrepaired: substitutionReport.sparseSpans,
      findings: substitutionReport.findings,
      // The channel-level answer a per-window comparison cannot give: when most of a channel is
      // destroyed, its own median IS the failure and every window looks normal beside its neighbours.
      channelsBelowFloor: substitutionReport.channelsBelowFloor,
      rejected: substitutionReport.rejected,
      unprobed: substitutionReport.unprobed,
      byChannel: substitutionReport.byChannel,
    };
    record.pipeline.restyle = restyleReport;
    record.pipeline.diarization = {
      method: dia.method,
      remoteTurns: dia.turns,
      speakerCount: dia.speakerCount,
      identityStable: dia.identityStable,
    };
    const verification = verify(finalMerged, { me: asrGuarded.me, others: asrGuarded.others }, captions, record);
    verification.repetitionGuard = record.pipeline.repetitionGuard;
    verification.deletionGuard = record.pipeline.deletionGuard;
    verification.substitutionGuard = record.pipeline.substitutionGuard;
    verification.restyle = record.pipeline.restyle;
    // A detect-only class leaves fabricated text in transcript.md, and the deletion class leaves a
    // HOLE in it. verification.json must say both in plain English, or "enabled: true" reads as
    // "hallucination: handled" and a missing clause reads as a pause. ONE warnings vocabulary for
    // both classes, deliberately: a reader should meet one way of being told this is not clean.
    verification.warnings = [
      ...toolchainWarnings,
      ...guardWarnings(repetitionReport),
      ...deletionWarnings(deletionReport),
      ...substitutionWarnings(substitutionReport),
      ...restyleWarnings(restyleReport),
    ];
    // FIRST in the list, deliberately. The other three warn about what the transcript SAYS; this
    // one warns that the thing which produced all of it is not what produced the last one, which
    // is the frame a reader needs before they weigh any of the rest.
    verification.toolchain = asr.toolchain
      ? {
          provenance: asr.toolchain.provenance,
          verdict: asr.toolchain.verdict,
          whisperCli: asr.toolchain.toolchain.bin,
          backends: asr.toolchain.toolchain.backends,
          model: asr.toolchain.toolchain.model,
          findings: asr.toolchain.findings.map((f) => f.kind),
        }
      : null;
    const totalWords = verification.channels.totalWords;

    if (totalWords < MIN_TRANSCRIPT_WORDS) {
      // Captured audio that yields (almost) no transcript is a LOUD anomaly, never a silent empty file.
      const problems = [
        `transcript is trivial (${totalWords} words from ${audioBytesOnDisk(sessionDir)} bytes of audio) — ` +
          'probable ASR failure. Audio is retained; re-transcribe (optionally with a different model).',
      ];
      log.alarm(`${sessionId} — trivial transcript`, { totalWords });
      record.pipeline.status = PIPELINE_STATUS.anomaly;
      record.pipeline.problems = problems;
      writePrivateFile(path.join(sessionDir, ARTIFACTS.verification), `${JSON.stringify(verification, null, 2)}\n`);
      writeRecord(sessionDir, record);
      return { status: PIPELINE_STATUS.anomaly, problems, sessionId, verification };
    }

    // ---- Stage 6: EMIT --------------------------------------------------------------------------
    const md = renderMarkdown(finalMerged, record);
    writePrivateFile(path.join(sessionDir, ARTIFACTS.transcript), md);
    writePrivateFile(path.join(sessionDir, ARTIFACTS.verification), `${JSON.stringify(verification, null, 2)}\n`);

    record.pipeline.status = PIPELINE_STATUS.ready;
    record.pipeline.ffmpegVersion = norm.ffmpeg || ffmpegVersion();
    record.pipeline.modelRuns = record.pipeline.modelRuns || [];
    const runIndex = record.pipeline.modelRuns.length;
    record.pipeline.modelRuns.push({
      model,
      tier: tier.name,
      at: now,
      words: totalWords,
      coverageRatio: verification.coverage.ratio,
      captionAgreement: verification.captions.agreementRatio,
      repetitionLoopsCollapsed: repetitionReport.removed,
      // PER RUN, not only on the record's current toolchain block — a re-transcription on a new
      // binary overwrites that block, and then the earlier run in this very array would be
      // attributed to a build it never touched. Each row now carries its own two hashes.
      whisperSha256: asr.toolchain?.toolchain?.bin?.sha256 || null,
      whisperCppVersion: asr.toolchain?.toolchain?.bin?.version || null,
      modelSha256: asr.toolchain?.toolchain?.model?.sha256 || null,
      modelPinned: asr.toolchain?.toolchain?.model?.pinned ?? null,
    });
    delete record.pipeline.problems;
    writeRecord(sessionDir, record);

    const ledger = appendLedger(
      {
        sessionId,
        runIndex,
        at: new Date(now).toISOString(),
        model,
        source: record.capture?.source || null,
        words: totalWords,
        speakers: finalMerged.speakers,
        captionCount: captions.length,
        captionAgreement: verification.captions.agreementRatio,
        coverageRatio: verification.coverage.ratio,
        transcript: ARTIFACTS.transcript,
        corrections: corrected.corrections.length,
      },
      opts.zone,
    );

    log.info(`${sessionId} — READY: ${totalWords} words, ${finalMerged.speakers.length} speaker(s), ledger ${
      ledger.appended ? 'appended' : 'already present'
    }`);
    return {
      status: PIPELINE_STATUS.ready,
      sessionId,
      transcript: path.join(sessionDir, ARTIFACTS.transcript),
      verification,
      words: totalWords,
    };
  } catch (err) {
    const problems = [`pipeline error: ${String(err.message || err)}`];
    log.alarm(`${sessionId} — pipeline FAILED`, { error: String(err.message || err) });
    record.pipeline.status = PIPELINE_STATUS.failed;
    record.pipeline.problems = problems;
    try {
      writeRecord(sessionDir, record);
    } catch {
      /* the alarm is already loud */
    }
    return { status: PIPELINE_STATUS.failed, problems, sessionId };
  }
}

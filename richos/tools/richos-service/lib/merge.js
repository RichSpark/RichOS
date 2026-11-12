/**
 * RichOS local service — pipeline stage 4: MERGE by timestamp (+ caption fold-in) & verification.
 *
 * PURE (no fs) so the whole merge is node-testable with fixture segments. Interleaves the two
 * channel transcripts into one time-ordered, speaker-attributed transcript, then folds in the
 * platform caption speaker LABELS where present: the 2-channel audio gives "you vs them"; the
 * captions add WHICH of them, by timestamp overlap (the system architecture §4 stage 4 + §3.3).
 *
 * It also computes verification.json — coverage, caption<->ASR agreement, dead intervals — turning
 * "did we capture the call?" into measured numbers rather than a vibe.
 */

import { toAbsolute } from './contract.js';

const DEAD_INTERVAL_MS = 30000;

/** @param {string} text */
export function wordCount(text) {
  const t = String(text || '').trim();
  return t ? t.split(/\s+/).length : 0;
}

/** mm:ss (or h:mm:ss) from a ms offset. */
export function stamp(ms) {
  const total = Math.max(0, Math.floor(Number(ms || 0) / 1000));
  const h = Math.floor(total / 3600);
  const m = Math.floor((total % 3600) / 60);
  const s = total % 60;
  const mm = String(m).padStart(2, '0');
  const ss = String(s).padStart(2, '0');
  return h > 0 ? `${h}:${mm}:${ss}` : `${mm}:${ss}`;
}

/** Overlap in ms between two [start,end] windows (0 if disjoint). */
function overlap(a0, a1, b0, b1) {
  return Math.max(0, Math.min(a1, b1) - Math.max(a0, b0));
}

/**
 * Attribute a far-side ("others") segment to a specific remote speaker name using captions.
 * @param {{startMs:number,endMs:number}} seg
 * @param {number} startedAt
 * @param {{speaker:string, firstT:number, t:number}[]} captions
 * @returns {string|null} best-overlapping caption speaker name, or null
 */
export function captionSpeakerFor(seg, startedAt, captions) {
  const segStart = toAbsolute(startedAt, seg.startMs);
  const segEnd = toAbsolute(startedAt, seg.endMs);
  let best = null;
  let bestOverlap = 0;
  for (const c of captions) {
    const cStart = Number(c.firstT ?? c.t);
    const cEnd = Number(c.t ?? c.firstT);
    const ov = overlap(segStart, segEnd, Math.min(cStart, cEnd), Math.max(cStart, cEnd));
    if (ov > bestOverlap) {
      bestOverlap = ov;
      best = c.speaker;
    }
  }
  return best && best !== 'unknown' ? best : null;
}

/**
 * Merge the two channel transcripts + captions into one attributed, time-ordered transcript.
 *
 * @param {{
 *   me: object[], others: object[],
 *   captions?: object[],
 *   startedAt: number,
 *   meLabel?: string, othersLabel?: string,
 * }} input
 * @returns {{segments: object[], speakers: string[]}}
 */
export function mergeTranscript({ me, others, captions = [], startedAt = 0, meLabel = 'Me', othersLabel = 'Them' }) {
  const speakers = new Set();
  const meSegs = (me || []).map((s) => ({ ...s, label: meLabel }));
  const othersSegs = (others || []).map((s) => {
    // Attribution priority for a far-side segment: a real caption NAME (best) > a diarized turn label
    // (P5 opt-in, when captions are absent) > the generic "Them". Diarization only fills the gap.
    const name = captionSpeakerFor(s, startedAt, captions);
    return { ...s, label: name || s.diarizedLabel || othersLabel };
  });
  const all = [...meSegs, ...othersSegs].sort((a, b) => a.startMs - b.startMs || (a.speaker === 'me' ? -1 : 1));
  for (const s of all) speakers.add(s.label);
  return { segments: all, speakers: [...speakers] };
}

const SENTENCE_END = /[.?!…]["'’)\]]*$/;

function displayedStart(segment) {
  return segment.displayStartMs ?? segment.startMs;
}

/** Past this many words a paragraph closes at its next sentence end, even inside a segment. */
export const PARAGRAPH_SOFT_WORDS = 80;

/**
 * Group segments at sentence ends, speaker changes and long gaps in speech.
 *
 * A whisper segment is a stretch of audio between two timestamps the model chose, not a unit of
 * language: on the CEO's 66-minute import (2026-10-04) 664 of 1,052 segments (63.1%) ended without
 * a sentence end, and rendering one paragraph per segment broke sentences in two across lines.
 *
 * Join adjacent unfinished sentences. Close a long paragraph at its next sentence end even
 * mid-segment. A gap as long as a dead interval starts a fresh passage so a late remark can be
 * cited at the time it was spoken. A paragraph keeps the time of its first word.
 *
 * @param {{startMs:number, endMs?:number, label:string, text:string, wordTimesMs?:number[]}[]} segments
 * @returns {{startMs:number, endMs?:number, label:string, text:string}[]}
 */
export function paragraphs(segments) {
  const out = [];
  for (const seg of segments || []) {
    const words = String(seg.text || '').trim().split(/\s+/).filter(Boolean);
    if (!words.length) continue;
    // The main pass's own per-word times, used only when they still line up one-to-one.
    const times = Array.isArray(seg.wordTimesMs) && seg.wordTimesMs.length === words.length ? seg.wordTimesMs : null;
    let last = out[out.length - 1];
    const start = displayedStart(seg);
    const afterGap = Number.isFinite(last?.endMs) && start - last.endMs >= DEAD_INTERVAL_MS;
    if (!last || last.label !== seg.label || SENTENCE_END.test(last.text) || afterGap) {
      last = { startMs: start, endMs: seg.endMs, label: seg.label, text: '', words: 0 };
      out.push(last);
    }
    words.forEach((w, k) => {
      if (last.words >= PARAGRAPH_SOFT_WORDS && SENTENCE_END.test(last.text)) {
        const wordStart = times ? times[k] : seg.startMs;
        const startMs = seg.displayStartMs === undefined ? wordStart : Math.max(wordStart, seg.displayStartMs);
        last = { startMs, endMs: seg.endMs, label: seg.label, text: '', words: 0 };
        out.push(last);
      }
      last.text = last.text ? `${last.text} ${w}` : w;
      last.words += 1;
      last.endMs = seg.endMs;
    });
  }
  return out.map(({ words, ...p }) => p);
}

/**
 * Render the merged transcript to markdown.
 * @param {{segments: object[]}} merged
 * @param {Record<string, any>} record the session.json record (for the header)
 * @returns {string}
 */
export function renderMarkdown(merged, record) {
  const startedAt = Number(record?.startedAt || 0);
  const endedAt = Number(record?.endedAt || 0);
  const durationMin = startedAt && endedAt ? Math.round(((endedAt - startedAt) / 60000) * 10) / 10 : null;
  const platform = record?.platform?.label || record?.platform?.id || 'call';
  const source = record?.capture?.source || 'unknown-surface';
  const model = record?.pipeline?.model || 'unknown-model';

  const lines = [];
  lines.push(`# Transcript — ${platform}`);
  lines.push('');
  lines.push(`- **Session:** \`${record?.sessionId || record?.dir || 'unknown'}\``);
  if (startedAt) lines.push(`- **Started:** ${new Date(startedAt).toISOString()}`);
  if (durationMin != null) lines.push(`- **Duration:** ${durationMin} min`);
  lines.push(`- **Captured by:** ${source}`);
  lines.push(`- **Model:** ${model}`);
  // WHICH BINARY AND WHICH WEIGHTS, on the artifact a person actually opens. `- **Model:** turbo`
  // above names a family, not a file: two different sets of weights ship under that id and a
  // whisper.cpp version bump can change what its own defaults mean. `session.json` carries the
  // full hashes; this is the line that stops a reader having to go looking for them.
  const provenance = record?.pipeline?.toolchain?.provenance || record?.pipeline?.whisperVersion;
  if (provenance) lines.push(`- **Transcribed by:** ${provenance}`);
  lines.push(`- **Speaker attribution:** LEFT channel = me (mic); RIGHT channel = others (system/tab)${
    record?.captions?.count ? '; remote names folded in from platform captions' : ''
  }`);
  lines.push('');
  lines.push('---');
  lines.push('');
  if (merged.segments.length === 0) {
    lines.push('_(no speech transcribed)_');
  } else {
    const ordered = merged.segments.some((segment) => segment.displayStartMs !== undefined)
      ? [...merged.segments].sort((a, b) => displayedStart(a) - displayedStart(b))
      : merged.segments;
    for (const seg of paragraphs(ordered)) {
      lines.push(`**[${stamp(seg.startMs)}] ${seg.label}:** ${seg.text}`);
      lines.push('');
    }
  }
  return lines.join('\n');
}

/**
 * Compute the verification record (coverage, caption<->ASR agreement, dead intervals).
 * @param {{segments: object[]}} merged
 * @param {{me: object[], others: object[]}} channels
 * @param {object[]} captions
 * @param {Record<string, any>} record
 * @returns {Record<string, any>}
 */
export function verify(merged, channels, captions, record) {
  const startedAt = Number(record?.startedAt || 0);
  const endedAt = Number(record?.endedAt || 0);
  const sessionDurationMs = Math.max(0, endedAt - startedAt);

  // Coverage: union of transcribed intervals over the session length.
  const intervals = merged.segments
    .map((s) => [s.startMs, s.endMs])
    .filter(([a, b]) => b > a)
    .sort((x, y) => x[0] - y[0]);
  let transcribedMs = 0;
  let curStart = null;
  let curEnd = null;
  for (const [a, b] of intervals) {
    if (curEnd == null || a > curEnd) {
      if (curEnd != null) transcribedMs += curEnd - curStart;
      curStart = a;
      curEnd = b;
    } else {
      curEnd = Math.max(curEnd, b);
    }
  }
  if (curEnd != null) transcribedMs += curEnd - curStart;

  // Dead intervals: gaps > threshold between consecutive transcribed spans.
  const dead = [];
  let prevEnd = 0;
  for (const [a, b] of intervals) {
    if (a - prevEnd > DEAD_INTERVAL_MS) dead.push({ fromMs: prevEnd, toMs: a });
    prevEnd = Math.max(prevEnd, b);
  }
  if (sessionDurationMs && sessionDurationMs - prevEnd > DEAD_INTERVAL_MS) {
    dead.push({ fromMs: prevEnd, toMs: sessionDurationMs });
  }

  // Caption<->ASR agreement: fraction of caption lines that overlap at least one far-side segment.
  const captionList = captions || [];
  let matched = 0;
  for (const c of captionList) {
    const cStart = Math.min(Number(c.firstT ?? c.t), Number(c.t ?? c.firstT));
    const cEnd = Math.max(Number(c.firstT ?? c.t), Number(c.t ?? c.firstT));
    const hit = channels.others.some((s) => {
      const s0 = toAbsolute(startedAt, s.startMs);
      const s1 = toAbsolute(startedAt, s.endMs);
      return overlap(cStart, cEnd, s0, s1) > 0;
    });
    if (hit) matched += 1;
  }

  const meWords = channels.me.reduce((n, s) => n + wordCount(s.text), 0);
  const othersWords = channels.others.reduce((n, s) => n + wordCount(s.text), 0);

  return {
    generatedAt: new Date().toISOString(),
    model: record?.pipeline?.model || null,
    coverage: {
      sessionDurationMs,
      transcribedMs,
      ratio: sessionDurationMs ? Math.round((transcribedMs / sessionDurationMs) * 1000) / 1000 : null,
    },
    channels: {
      meSegments: channels.me.length,
      othersSegments: channels.others.length,
      meWords,
      othersWords,
      totalWords: meWords + othersWords,
    },
    captions: {
      count: captionList.length,
      matchedToSegments: matched,
      agreementRatio: captionList.length ? Math.round((matched / captionList.length) * 1000) / 1000 : null,
    },
    deadIntervals: dead,
    deadIntervalCount: dead.length,
  };
}

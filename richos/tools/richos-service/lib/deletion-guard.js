/**
 * RichOS local service — P5 pipeline stage 3.7: the post-decode DELETION DETECTOR.
 *
 * THE CLASS `repetition-guard.js` CANNOT SEE. That module covers three measured failure classes —
 * repetition loop, sliding-overlap stutter, persistent ordinal insertion — and all three are the
 * model SAYING TOO MUCH. Every one of them leaves evidence in the text, which is why a text-only
 * detector can find them. **Deletion leaves no text at all.** The model emits nothing over a span
 * of real speech, with no repetition, no marker, no stutter, and — measured — no confidence signal
 * either: on the 2026-08-29 coverage measurement the tokens either side of a confirmed deletion
 * carried confidences of 0.94–1.00 and the file-level confidence was 0.94–0.96. **Any alarm built
 * on model confidence would have stayed silent.** So would every guard we had.
 *
 * WHY IT MATTERS, in the one number that commissioned this module: on 92 minutes of real
 * two-channel audio, `large-v3-turbo` at the decode configuration shipping before 2026-08-29
 * silently dropped 75.8 seconds of speech, including NINE complete clauses of >= 2 s on one
 * channel, each of which both an independent model and the same model recovered perfectly when the
 * burst was decoded on its own. `large-v3-turbo-q5_0` dropped >= 104.2 s. Nothing in the product
 * noticed, because nothing in the product was looking.
 *
 * ---------------------------------------------------------------------------------------------
 * THE DISCRIMINATOR, unchanged from the measurement that established it
 * ---------------------------------------------------------------------------------------------
 * A CONFIRMED DELETION is:
 *
 *     a physically detected speech burst of >= minGapSec over which the run emitted NO word at
 *     all, where decoding THAT BURST IN ISOLATION returns real words.
 *
 * Both halves are load-bearing and neither is sufficient:
 *
 *   - The burst grid is MODEL-FREE (ffmpeg `silencedetect`, `normalize.js#detectSpeechBursts` —
 *     the same probe the repetition guard's physical veto already consumes, already computed once
 *     per channel by the pipeline, measured at 0.68 s for a 92-minute channel). It says where
 *     there is acoustic energy. It cannot say whether that energy is speech: the single loudest
 *     uncovered burst in the whole corpus — 5.81 s at max -0.9 dBFS — is LAUGHTER, and every
 *     model correctly emits nothing for it. **A detector that alarmed on raw burst gaps would have
 *     cried deletion over a man laughing.** That is the reason for the second half.
 *
 *   - The isolated re-decode is the ADJUDICATOR. A span decoded on its own has no prior context,
 *     no chunk seam and no adjacent repetition, so what comes back is what the audio contains.
 *     This is the same discriminator FluidAudio issue #850's reporter used and the same one the
 *     2026-08-29 briefs used on all 72 repetition findings.
 *
 * The measurement had a second model available and used "both decodes agree" as one arm of its
 * rule. THE RUNTIME HAS ONE MODEL, so that arm is replaced, not dropped — see THE PRECISION RULE.
 *
 * ---------------------------------------------------------------------------------------------
 * DETECT-ONLY, ON PURPOSE, AND THE PRECEDENT IS IN THE OTHER FILE
 * ---------------------------------------------------------------------------------------------
 * A deletion cannot be repaired from here. Splicing the isolated decode back into the transcript
 * would give a timeline stitched from two different decodes of the same file, and the class of
 * defect that produces is worse than the one it fixes. So this follows class 2 (persistent
 * insertion) exactly: make the failure LOUD — a report, a pipeline alarm, and a plain-English
 * `verification.json` warning naming the span — and never rewrite the transcript. The audio is
 * retained; re-transcription is the remedy, and an alarm that names `01:23:19–01:23:22` is
 * actionable in a way that a silent gap never is.
 *
 * ---------------------------------------------------------------------------------------------
 * THE PRECISION RULE — five conditions, and why each one is there
 * ---------------------------------------------------------------------------------------------
 * PRECISION IS THE CONTRACT (same doctrine as the rest of this pipeline). A false deletion alarm
 * on every natural pause makes the alarm worthless and it gets switched off, and then it protects
 * nothing. **Genuine silence must not read as deletion** — and genuine silence and deletion look
 * identical in the transcript, since both are "no words here". Everything separating them is
 * physical. A candidate is called DELETED only when ALL FIVE hold:
 *
 *   1. DURATION      the burst is >= minGapSec (1.0 s). Below a second the burst grid's own edge
 *                    clipping dominates and the losses are single backchannel words; the corpus
 *                    put 82.8% speech in that band on one channel and 23.3% on the other, which is
 *                    not a rate anything can alarm on. Deliberately out of scope, stated loudly.
 *   2. LEXICAL       the isolated decode returns >= minProbeWords (3) DISTINCT INFORMATIVE words —
 *                    content words, deduplicated, with interjections and backchannel removed (see
 *                    FILLER_WORDS) — after the silence-hallucination vocabulary is stripped. Both
 *                    filters were forced by the corpus: whisper.cpp reliably emits
 *                    "Thank you." / "you" / "Shh" / "*sniff*" over near-silence — proven in this
 *                    corpus at 4885.2 s, where it emitted "Thank you." SEVEN times over a window
 *                    measuring max -51.3 dBFS. If a fabrication over silence were allowed to count
 *                    as evidence that speech was there, this detector would manufacture its own
 *                    false positives out of the exact failure it sits next to.
 *   3. LEVEL         the span's own max level is within loudBelowPeakDb (24 dB) of the CHANNEL's
 *                    peak. Relative, never absolute: the two capture setups in the corpus are not
 *                    level-matched (-34.6 vs -30.5 dBFS mean), so a shared absolute threshold is
 *                    invalid — the same reasoning, and the same shape, as
 *                    `normalize.js#SPEECH_FLOOR_BELOW_PEAK_DB`. On a channel peaking at -0.9 dBFS
 *                    this is -24.9 dBFS, reproducing the measurement's hand-verified -25 dBFS
 *                    clause. The nine confirmed deletions measured -6.6 to -17.6 dBFS; the
 *                    rejected near-silence gaps measured -27 to -44 dBFS.
 *   4. STABILITY     the span is decoded TWICE at two paddings and the two decodes must share at
 *                    least minStableWords (2) content words. This is the runtime replacement for
 *                    the measurement's "both models agree" arm. A real clause is in the audio and
 *                    survives having a little more or a little less room around it; a hallucination
 *                    over near-silence is a property of the exact window and is padding-fragile.
 *                    It costs one extra clip decode per candidate and nothing else — see COST.
 *   5. ECHO          the recovered words must appear NOWHERE in the transcript around the span:
 *                    more than maxEchoWords (2) CONSECUTIVE content words already present nearby
 *                    means the speech is in the transcript with the wrong timestamps, and a timing
 *                    defect is not a deletion. This condition was not designed, it was FORCED by
 *                    measurement: the first version of this detector, scored on segment extents,
 *                    reported eight deletions on one 92-minute channel and ALL EIGHT were mistimed
 *                    text that was present in the transcript all along — segments whose extents
 *                    stretched 20-30 s across silence. Word times fix that case; this condition
 *                    catches the same failure whatever produced it, including the extent fallback
 *                    and the repetition guard's own collapse (which extends a surviving segment's
 *                    end over a run it removed). Such a candidate is reported as `mistimed`, with
 *                    the reason, never as a deletion and never silently dropped.
 *
 * The five are independent: two are physical, one is lexical, one is a repeat experiment, one is a
 * text comparison against the artifact itself. A false alarm has to beat all five.
 *
 * AND THE WHOLE THING IS ONE-DIRECTIONAL, like the repetition guard's burst veto. The probe can
 * only ever CONFIRM a deletion the coverage stage already suspected; it can never mark a covered
 * span as deleted. So the detector is strictly more conservative than the coverage number, and the
 * failure mode it prefers is silence about a real deletion rather than noise about a real pause.
 *
 * ---------------------------------------------------------------------------------------------
 * COST — measured, not estimated, because a detector that must re-run the file is not shippable
 * ---------------------------------------------------------------------------------------------
 * Nothing here re-transcribes anything. The burst grid is already computed by the pipeline for the
 * repetition guard's veto (0.68 s per 92-minute channel). Candidate generation is pure arithmetic
 * over data already in memory. Only the confirmed candidates are re-decoded, at most `maxProbes`
 * of them, longest first, as clips of a few seconds each — and all of them go through ONE
 * `whisper-cli` invocation, which takes `file0 file1 ...` and loads the model once. The measured
 * end-to-end addition on the 92-minute corpus is in the brief; the design bound is what matters
 * here: the probe budget is a hard cap, it is stated in the report, and a candidate that does not
 * fit inside it is reported as UNPROBED and never as a deletion.
 *
 * ---------------------------------------------------------------------------------------------
 * WHAT THIS CANNOT SEE — enumerated here rather than discovered later
 * ---------------------------------------------------------------------------------------------
 *   * PARTIAL deletion inside a covered burst. If the model emits some words over a burst and
 *     drops a clause within it, the burst is covered and this detector is blind. Coverage is a
 *     presence test, not a completeness test.
 *   * SUBSTITUTION. Words replaced by other words score as perfect coverage — `q5_0`'s 1,099-second
 *     collapse emitted 5,671 word tokens over the span it destroyed, so every burst there is
 *     "covered". Coverage and the repetition guard are complementary instruments and NEITHER is
 *     sufficient alone. Nothing in this pipeline yet measures word density against a physical
 *     speech budget, which is the instrument that would see it.
 *   * SPEECH BELOW THE BURST FLOOR. The grid is drawn at the channel's peak minus
 *     SPEECH_FLOOR_BELOW_PEAK_DB. Speech quieter than that produces no burst, so a deletion inside
 *     it produces no candidate. The floor is a channel-relative choice with a measured basis, not
 *     a guarantee.
 *   * DELETION HIDDEN BY A STRETCHED SEGMENT, when there are no word times. Coverage is scored on
 *     the emitted WORD wherever the segments carry `wordTimesMs` (whisper.cpp `-ojf` token offsets,
 *     which `transcribe.js` now always requests) and on the SEGMENT EXTENT otherwise. The extent
 *     fallback is bad in both directions and measured being bad — see `emittedWordTimes`. The report
 *     states which unit was used, every time, in `coverageUnit`; treat an `segment-extent` report as
 *     a weaker instrument, not an equal one.
 *   * A TIMING DEFECT. The transcript containing the right words at the wrong second is a real
 *     defect and this detector deliberately does NOT report it as a deletion (condition 5). It
 *     surfaces as a `mistimed` rejection in the report and nowhere else. Nothing in this pipeline
 *     yet audits timestamp accuracy.
 *   * SUB-SECOND LOSS. Out of scope by condition 1, above.
 *   * A SPAN THE MODEL ALSO REFUSES IN ISOLATION. If the isolated decode is empty too, the rule
 *     says NOT-SPEECH. A deletion that the model reproduces on the second attempt is beyond this
 *     method entirely, and no single-model method can be built that sees it.
 *   * ANYTHING WITHOUT A PROBE. No probe, no deletion — by construction. A pipeline that cannot cut
 *     or decode clips gets candidates and an honest `probed: 0`, never an alarm.
 *
 * ---------------------------------------------------------------------------------------------
 * PURE (no fs, no child_process), like `repetition-guard.js`, so the whole rule is node-testable on
 * fixtures. The two impure halves are injected by the pipeline and live where their tool already
 * lives: clip cutting + level measurement in `normalize.js` (ffmpeg), clip decoding in
 * `transcribe.js` (whisper). This module never touches either.
 */

/** @typedef {{startMs:number, endMs:number, text:string, speaker?:string}} Segment */
/** @typedef {{startMs:number, endMs:number}} Burst */
/**
 * @typedef {{channel:string, startMs:number, endMs:number, durationSec:number,
 *            index:number}} DeletionCandidate
 */
/**
 * @typedef {{tight:string, wide:string, maxDb:number|null, meanDb:number|null}} SpanProbe
 *   `tight` / `wide` are the isolated decodes at the two paddings; levels are of the span itself.
 */

export const DEFAULT_DELETION_OPTS = {
  // ---- stage A: candidate generation (pure, free) ---------------------------------------------
  minGapSec: 1.0, // condition 1 — below this the losses are single backchannel words
  coverToleranceSec: 0.25, // slack at each burst edge; silencedetect clips edges and so does whisper
  maxCandidateSec: 600, // a "burst" longer than this is a broken grid, not a deleted clause

  // ---- stage B: probe budget -------------------------------------------------------------------
  maxProbes: 40, // hard cap on isolated re-decodes per channel, longest candidate first
  probePadSec: 0.3, // padding for the primary (tight) clip — the measurement's value
  probeWidePadSec: 0.75, // the second clip, for the stability test

  // ---- stage B: the precision rule ---------------------------------------------------------------
  minProbeWords: 3, // condition 2 — a >= 1 s deletion is a clause, not a word
  loudBelowPeakDb: 24, // condition 3 — channel-relative, reproduces the measured -25 dBFS clause
  minStableWords: 2, // condition 4 — content words shared by the tight and wide decodes
  maxEchoWords: 2, // condition 5a — >2 consecutive probe words already nearby = mistimed, not missing
  // condition 5b — or >=70% of the probe's words already nearby, in order. Swept against both
  // 92-minute artifacts: below 0.6 a genuine deletion is lost, at/above 0.9 a measured false
  // positive returns; 0.7 is the middle of that plateau.
  maxEchoRatio: 0.7,
  localWindowSec: 2, // how far either side of the span "nearby" reaches, on top of segment overlap
};

/**
 * The silence-hallucination vocabulary, stripped before a decode counts as evidence of speech.
 *
 * Every entry is something whisper.cpp was OBSERVED emitting over measured near-silence in the
 * 2026-08-29 corpus, plus the non-lexical markers it brackets. This list is deliberately small and
 * anchored to observations: an over-broad list would start eating real short answers.
 */
const NON_LEXICAL = new RegExp(
  '^\\s*(' +
    '\\*[^*]*\\*' + // *sniff*  *cough*  *shriek*
    '|\\[[^\\]]*\\]' + // [BLANK_AUDIO]  [MUSIC]
    '|\\([^)]*\\)' + // (laughs)
    '|[-–—.,!?…]+' + // bare punctuation, the "-" whisper emits over dead air
    '|shh+[.!]?' +
    '|hm+[.!]?' +
    '|uh+[.!]?' +
    '|mm+[.!]?' +
    '|ah+[.!]?' +
    '|oh+[.!]?' +
    '|ha([ ,.!]*ha)*[ ,.!]*' +
    '|you' +
    '|thank you[.!]?' +
    '|thanks[.!]?' +
    '|bye[.!]?' +
    '|okay[.!]?' +
    '|yeah[.!]?' +
    '|so\\.{2,3}' +
    '|whew[.!]?' +
    '|ahem[.!]?' +
    ')\\s*$',
  'i',
);

/**
 * The lexical content of an isolated decode: '' when the decode is only silence-hallucination or
 * non-lexical markers. Whitespace-normalized, otherwise verbatim.
 * @param {string} text
 * @returns {string}
 */
export function lexicalText(text) {
  const t = String(text == null ? '' : text)
    .replace(/\s+/g, ' ')
    .trim();
  if (!t) return '';
  if (NON_LEXICAL.test(t)) return '';
  return t;
}

/**
 * Is this text ENTIRELY silence-hallucination filler, sentence by sentence?
 *
 * The same vocabulary as `lexicalText`, read in a stricter unit and exported for the OTHER
 * direction: `repetition-guard.js`'s silence-fabrication class (class 4) removes a segment only
 * when the audio under it is measured silence AND its text is nothing but this. ONE vocabulary
 * serves both directions deliberately — a second copy of "what whisper says over silence" that
 * drifts from this one would make the detector and the remover disagree about the same failure.
 *
 * PER SENTENCE, AND THE UNIT IS NOT A DETAIL. `lexicalText` tests the whole string against the
 * pattern, so `"Thank you. Thank you."` — two copies of whisper's canonical silence filler — does
 * not match the pattern for ONE `"Thank you."` and reads as lexical. That exact bug invalidated a
 * 24-span adjudication run in the six-track private measurement, in the direction that turns the
 * finding into its opposite (every confirmed fabrication came back "REAL-QUIET"). Splitting into
 * sentence units first makes the number of repeats irrelevant.
 *
 * AND IT STAYS CONSERVATIVE, because EVERY unit must be filler. `"Thank you. And then we agreed
 * the budget."` splits into one filler unit and one real one, so the whole thing is NOT filler and
 * class 4 will not touch it. The failure this can produce is a fabrication left in the transcript,
 * never a real clause removed from it.
 *
 * @param {string} text
 * @returns {boolean}
 */
export function isSilenceFillerText(text) {
  const t = String(text == null ? '' : text).replace(/\s+/g, ' ').trim();
  if (!t) return true;
  const units = t
    .split(/(?<=[.!?])\s+|\s*\.\s*/)
    .map((u) => u.replace(/^[-.,\s]+|[.!?,\s]+$/g, '').trim())
    .filter(Boolean);
  if (!units.length) return true; // bare punctuation — whisper's "-" over dead air
  return units.every((u) => NON_LEXICAL.test(u));
}

/**
 * Comparable content words of a decode — lowercased alphanumerics, the same tokenizer shape
 * `repetition-guard.js#wordsOf` uses, so "here's" is one token on both sides of a comparison.
 * @param {string} text
 * @returns {string[]}
 */
export function contentWords(text) {
  const out = String(text == null ? '' : text).toLowerCase().match(/[a-z0-9]+(?:['’][a-z0-9]+)*/g);
  return (out || []).map((w) => w.replace(/['’]/g, ''));
}

/**
 * Interjections and backchannel tokens that carry no information about whether a CLAUSE was lost.
 *
 * Small and observation-anchored, like NON_LEXICAL. It exists because of a specific measured false
 * positive: the loudest uncovered burst on one channel was the man LAUGHING, and while
 * `lexicalText` correctly rejects a bare "Ha ha ha ha", the isolated decode of that burst came back
 * as "But first, ha, ha, ha, ha, ha, ha." — eight content words, two of them real, six of them one
 * laugh repeated. Counting raw words there says "a clause was deleted"; counting DISTINCT
 * informative words says "two words and a laugh", which is the truth.
 */
const FILLER_WORDS = new Set([
  'ha', 'haha', 'hahaha', 'hah', 'heh', 'hehe', 'hm', 'hmm', 'hmmm', 'mm', 'mmm', 'mhm',
  'uh', 'uhh', 'um', 'umm', 'er', 'erm', 'ah', 'aah', 'oh', 'ooh', 'eh', 'huh',
  'yeah', 'yep', 'yes', 'no', 'okay', 'ok', 'shh', 'whew', 'ahem', 'hey', 'hi', 'hello', 'bye',
]);

/**
 * DISTINCT informative words of a decode — content words, deduplicated, with the filler set
 * removed. This is what condition 2 counts, and the reason it counts this rather than raw words is
 * in FILLER_WORDS.
 * @param {string} text
 * @returns {string[]}
 */
export function informativeWords(text) {
  const seen = new Set();
  for (const w of contentWords(text)) if (!FILLER_WORDS.has(w)) seen.add(w);
  return [...seen];
}

/**
 * How many content words do two decodes of the same span share? Multiset intersection, so a word
 * repeated in both counts as many times as both contain it.
 * @param {string} a
 * @param {string} b
 * @returns {number}
 */
export function sharedWordCount(a, b) {
  const counts = new Map();
  for (const w of contentWords(a)) counts.set(w, (counts.get(w) || 0) + 1);
  let n = 0;
  for (const w of contentWords(b)) {
    const c = counts.get(w) || 0;
    if (c > 0) {
      counts.set(w, c - 1);
      n += 1;
    }
  }
  return n;
}

/**
 * Where the transcript claims its words are, in milliseconds, sorted.
 *
 * TWO UNITS, AND THE REPORT ALWAYS SAYS WHICH ONE WAS USED, because they are not equally good and
 * pretending otherwise is how a detector ends up lying:
 *
 *   - WORD-TIMES — every segment carries its own `wordTimesMs` (whisper.cpp `-ojf` token offsets,
 *     attached per segment by `transcribe.js#parseWhisperJson`). The model's OWN claim about where
 *     its words are; nothing is inferred. This is the unit the 2026-08-29 measurement used and the
 *     unit the pipeline supplies.
 *
 *   - SEGMENT-EXTENT — the fallback when no word times exist: a segment's words spread uniformly
 *     across its extent. IT IS NOT MERELY LESS PRECISE, IT IS WRONG IN BOTH DIRECTIONS, and it was
 *     measured being wrong: on the 92-minute corpus it produced EIGHT false deletions on one
 *     channel, every one of them a segment whose extent stretched 20-30 s back across silence while
 *     its 4-8 words were physically spoken in the last 1.5 s. Spreading them uniformly filled the
 *     silence with phantom words and starved the one burst that actually held speech. The same
 *     eight spans are correct under word times. It also hides real deletions inside a stretched
 *     segment. It is kept only so a caller without token times still gets candidates rather than
 *     nothing — and the fifth adjudication condition (ECHO) exists in large part to catch what this
 *     unit gets wrong.
 *
 * @param {Segment[]} segments
 * @param {number[]|null} [wordTimesMs] an optional whole-channel override, for callers that hold
 *   the times separately from the segments
 * @returns {{times:number[], unit:'word-times'|'segment-extent'}}
 */
export function emittedWordTimes(segments, wordTimesMs) {
  if (Array.isArray(wordTimesMs) && wordTimesMs.length) {
    return { times: wordTimesMs.map(Number).filter(Number.isFinite).sort((a, b) => a - b), unit: 'word-times' };
  }
  const segs = segments || [];
  const perSegment = segs.some((s) => Array.isArray(s.wordTimesMs) && s.wordTimesMs.length);
  const times = [];
  for (const s of segs) {
    if (perSegment) {
      for (const t of s.wordTimesMs || []) if (Number.isFinite(Number(t))) times.push(Number(t));
      continue;
    }
    const a = Number(s.startMs) || 0;
    const b = Number(s.endMs) || 0;
    const words = String(s.text || '').trim().split(/\s+/).filter(Boolean);
    if (!words.length) continue;
    const span = Math.max(0, b - a);
    words.forEach((w, i) => times.push(a + ((i + 0.5) / words.length) * span));
  }
  times.sort((x, y) => x - y);
  return { times, unit: perSegment ? 'word-times' : 'segment-extent' };
}

/**
 * The text the transcript already claims for the stretch of time around a span.
 *
 * Feeds the ECHO condition: the union of every segment overlapping the span widened by
 * `localWindowSec`. A window rather than a radius, because the failure it guards against is a
 * segment whose extent is far larger than the speech in it — the offending text is inside the
 * containing segment, which the overlap test picks up whatever its length.
 *
 * @param {Segment[]} segments
 * @param {number} startMs
 * @param {number} endMs
 * @param {number} localWindowSec
 * @returns {string}
 */
export function nearbyTranscriptText(segments, startMs, endMs, localWindowSec) {
  const pad = Math.max(0, Number(localWindowSec) || 0) * 1000;
  const a = startMs - pad;
  const b = endMs + pad;
  const parts = [];
  for (const s of segments || []) {
    const x = Number(s.startMs) || 0;
    const y = Number(s.endMs) || 0;
    if (y < a || x > b) continue;
    parts.push(String(s.text || ''));
  }
  return parts.join(' ');
}

/**
 * Longest run of CONSECUTIVE content words from `probeText` that appears contiguously in
 * `nearbyText`. The ECHO measure.
 *
 * A bag-of-words overlap would be useless here — "we", "that", "one", "the" collide with any
 * English at all. A contiguous n-gram does not: three consecutive words reproduced exactly is the
 * transcript already containing the phrase, not a coincidence.
 * @param {string} probeText
 * @param {string} nearbyText
 * @returns {number}
 */
export function echoLength(probeText, nearbyText) {
  const p = contentWords(probeText);
  const n = contentWords(nearbyText);
  if (!p.length || !n.length) return 0;
  // longest common SUBSTRING over word arrays; the corpus is a handful of words either side
  let best = 0;
  const prev = new Array(n.length + 1).fill(0);
  const cur = new Array(n.length + 1).fill(0);
  for (let i = 1; i <= p.length; i += 1) {
    for (let j = 1; j <= n.length; j += 1) {
      cur[j] = p[i - 1] === n[j - 1] ? prev[j - 1] + 1 : 0;
      if (cur[j] > best) best = cur[j];
    }
    prev.fill(0);
    for (let j = 0; j <= n.length; j += 1) prev[j] = cur[j];
    cur.fill(0);
  }
  return best;
}

/**
 * What FRACTION of the probe's words the nearby transcript already contains, IN ORDER — longest
 * common subsequence over content words, divided by the probe's length.
 *
 * The contiguous measure above is sharp but brittle, and the corpus broke it: an isolated re-decode
 * came back carrying one extra connective word that the transcript beside it did not have.
 * One inserted word cuts the longest exact run to two, and a plainly-present sentence would have
 * been reported as missing. A subsequence tolerates the insertions and small word differences that
 * two decodes of the same audio always produce, while still requiring the transcript to contain the
 * probe's words in the probe's order.
 *
 * The two measures fire on different shapes and both are cheap, so both are used.
 * @param {string} probeText
 * @param {string} nearbyText
 * @returns {number} 0..1
 */
export function echoRatio(probeText, nearbyText) {
  const p = contentWords(probeText);
  const n = contentWords(nearbyText);
  if (!p.length || !n.length) return 0;
  const prev = new Array(n.length + 1).fill(0);
  const cur = new Array(n.length + 1).fill(0);
  for (let i = 1; i <= p.length; i += 1) {
    for (let j = 1; j <= n.length; j += 1) {
      cur[j] = p[i - 1] === n[j - 1] ? prev[j - 1] + 1 : Math.max(prev[j], cur[j - 1]);
    }
    for (let j = 0; j <= n.length; j += 1) prev[j] = cur[j];
    cur.fill(0);
  }
  return prev[n.length] / p.length;
}

/** Number of emitted words whose time falls inside [a, b] widened by `tolMs`. */
function wordsInSpan(times, a, b, tolMs) {
  let lo = 0;
  let hi = times.length;
  while (lo < hi) {
    const mid = (lo + hi) >> 1;
    if (times[mid] < a - tolMs) lo = mid + 1;
    else hi = mid;
  }
  let n = 0;
  for (let i = lo; i < times.length && times[i] <= b + tolMs; i += 1) n += 1;
  return n;
}

/**
 * STAGE A — every speech burst over which the transcript emitted no word at all.
 *
 * PURE and free: arithmetic over the burst grid the pipeline already computed and the segments it
 * already has. A candidate is a SUSPICION, never a finding — nothing here is reported to anyone
 * until stage B has re-decoded it.
 *
 * @param {Segment[]} segments this channel's post-guard segments
 * @param {Burst[]} speechBursts this channel's physical burst grid, time-ordered
 * @param {object} [opts] see DEFAULT_DELETION_OPTS; plus `channel` and `wordTimesMs`
 * @returns {{candidates: DeletionCandidate[], coverageUnit: string, bursts: number,
 *            burstSeconds: number, emittedWords: number}}
 */
export function findDeletionCandidates(segments, speechBursts, opts = {}) {
  const o = { ...DEFAULT_DELETION_OPTS, ...opts };
  const channel = opts.channel || '';
  const bursts = Array.isArray(speechBursts) ? speechBursts : [];
  const { times, unit } = emittedWordTimes(segments || [], opts.wordTimesMs);
  const tolMs = o.coverToleranceSec * 1000;
  const minMs = o.minGapSec * 1000;
  const maxMs = o.maxCandidateSec * 1000;

  const candidates = [];
  let burstMs = 0;
  for (const b of bursts) {
    const a = Number(b.startMs) || 0;
    const z = Number(b.endMs) || 0;
    const dur = z - a;
    if (dur <= 0) continue;
    burstMs += dur;
    if (dur < minMs || dur > maxMs) continue;
    if (wordsInSpan(times, a, z, tolMs) > 0) continue;
    candidates.push({
      channel,
      index: candidates.length,
      startMs: Math.round(a),
      endMs: Math.round(z),
      durationSec: +(dur / 1000).toFixed(2),
      // Carried on the candidate so `adjudicateCandidate` stays a pure function of (candidate,
      // probe): the ECHO condition needs the transcript, and stage A is the last place that has it.
      nearbyText: nearbyTranscriptText(segments || [], a, z, o.localWindowSec),
    });
  }
  return {
    candidates,
    coverageUnit: unit,
    bursts: bursts.length,
    burstSeconds: +(burstMs / 1000).toFixed(1),
    emittedWords: times.length,
  };
}

/**
 * Which candidates are worth spending a decode on, longest first, capped by the probe budget.
 * Separated out so the pipeline can size its ffmpeg/whisper work before doing any of it, and so
 * the cap is visible in the report rather than buried in a loop.
 * @param {DeletionCandidate[]} candidates
 * @param {object} [opts]
 * @returns {{probe: DeletionCandidate[], unprobed: DeletionCandidate[]}}
 */
export function selectProbeSpans(candidates, opts = {}) {
  const o = { ...DEFAULT_DELETION_OPTS, ...opts };
  const ordered = [...(candidates || [])].sort((a, b) => b.durationSec - a.durationSec);
  return { probe: ordered.slice(0, o.maxProbes), unprobed: ordered.slice(o.maxProbes) };
}

/**
 * STAGE B — the verdict for ONE candidate, given its isolated re-decodes and its physical level.
 *
 * All four conditions of THE PRECISION RULE, evaluated in the order that makes the `reason` most
 * useful when it says no. PURE: the probe is data, not a call.
 *
 * @param {DeletionCandidate} candidate
 * @param {SpanProbe|null} probe
 * @param {{peakDb?: number}} [opts]
 * @returns {{verdict:'deleted'|'not-speech'|'unprobed', reason:string, words:number,
 *            text:string, stableWords:number, maxDb:number|null}}
 */
export function adjudicateCandidate(candidate, probe, opts = {}) {
  const o = { ...DEFAULT_DELETION_OPTS, ...opts };
  const base = { words: 0, text: '', stableWords: 0, maxDb: probe ? probe.maxDb : null };
  if (!probe) {
    return { ...base, verdict: 'unprobed', reason: 'no isolated re-decode was performed for this span' };
  }
  const peak = Number(o.peakDb);
  const floor = Number.isFinite(peak) ? peak - o.loudBelowPeakDb : null;
  if (floor != null && (probe.maxDb == null || !(Number(probe.maxDb) >= floor))) {
    return {
      ...base,
      verdict: 'not-speech',
      reason:
        `the span measures ${probe.maxDb == null ? 'no level' : `${probe.maxDb} dBFS`} against a channel peak of ` +
        `${peak} dBFS and falls below the ${o.loudBelowPeakDb} dB speech floor`,
    };
  }
  const tight = lexicalText(probe.tight);
  const wide = lexicalText(probe.wide);
  const words = informativeWords(tight).length;
  const stable = sharedWordCount(tight, wide);
  const out = { ...base, words, text: tight, stableWords: stable };

  if (!tight) {
    return { ...out, verdict: 'not-speech', reason: 'the isolated re-decode returned no lexical words' };
  }
  if (words < o.minProbeWords) {
    return {
      ...out,
      verdict: 'not-speech',
      reason:
        `the isolated re-decode returned ${words} distinct informative word(s), below the ${o.minProbeWords}-word ` +
        `floor for a >= ${o.minGapSec}s span (laughter, a breath and a repeated backchannel all land here)`,
    };
  }
  if (stable < o.minStableWords) {
    return {
      ...out,
      verdict: 'not-speech',
      reason:
        `the two isolated re-decodes of this span share only ${stable} content word(s) — an unstable decode is a ` +
        'property of the window, not of the audio',
    };
  }
  const echo = echoLength(tight, candidate.nearbyText || '');
  const ratio = echoRatio(tight, candidate.nearbyText || '');
  out.echoWords = echo;
  out.echoRatio = +ratio.toFixed(2);
  if (echo > o.maxEchoWords || ratio >= o.maxEchoRatio) {
    return {
      ...out,
      verdict: 'mistimed',
      reason:
        (echo > o.maxEchoWords
          ? `${echo} consecutive words of the isolated re-decode already appear`
          : `${Math.round(ratio * 100)}% of the isolated re-decode's words already appear, in order,`) +
        ' in the transcript around this span — the speech is PRESENT and merely carries the wrong timestamps. ' +
        'A timing defect is not a deletion and must not be reported as one',
    };
  }
  return { ...out, verdict: 'deleted', reason: 'a physically detected speech burst with no emitted word, whose isolated re-decode returns stable lexical speech that appears nowhere in the surrounding transcript' };
}

/**
 * STAGE B for a whole channel: adjudicate every candidate against its probe.
 *
 * @param {DeletionCandidate[]} candidates
 * @param {Map<number, SpanProbe>|object} probes keyed by candidate index
 * @param {{peakDb?: number, channel?: string}} [opts]
 * @returns {{deletions: object[], rejected: object[], unprobed: object[], deletedSeconds: number}}
 */
export function adjudicateDeletions(candidates, probes, opts = {}) {
  const get = (i) => (probes instanceof Map ? probes.get(i) : probes ? probes[i] : null) || null;
  const deletions = [];
  const rejected = [];
  const unprobed = [];
  for (const c of candidates || []) {
    const v = adjudicateCandidate(c, get(c.index), opts);
    const row = {
      channel: c.channel || opts.channel || '',
      startMs: c.startMs,
      endMs: c.endMs,
      durationSec: c.durationSec,
      maxDb: v.maxDb,
      verdict: v.verdict,
      reason: v.reason,
    };
    if (v.verdict === 'deleted') {
      deletions.push({
        ...row,
        words: v.words,
        stableWords: v.stableWords,
        echoWords: v.echoWords || 0,
        echoRatio: v.echoRatio || 0,
        recovered: v.text,
      });
    } else if (v.verdict === 'unprobed') unprobed.push(row);
    else rejected.push(row);
  }
  deletions.sort((a, b) => a.startMs - b.startMs);
  rejected.sort((a, b) => a.startMs - b.startMs);
  unprobed.sort((a, b) => a.startMs - b.startMs);
  return {
    deletions,
    rejected,
    unprobed,
    deletedSeconds: +deletions.reduce((n, d) => n + d.durationSec, 0).toFixed(1),
  };
}

/**
 * The whole two-stage detector over both channels, with the impure half INJECTED.
 *
 * `opts.probe(spans, channel)` receives the selected candidate spans for one channel and must
 * return one `SpanProbe` per span, in order — cut the clips, decode them, measure the level. The
 * pipeline builds it from `normalize.js#probeSpanLevels` + `transcribe.js#transcribeClips`; a test
 * builds it from a fixture table. Absent, every candidate is reported UNPROBED and nothing is ever
 * called a deletion.
 *
 * @param {{me: Segment[], others: Segment[]}} channels post-guard segments
 * @param {{speechBursts?: {me: Burst[], others: Burst[]}|null,
 *          peaks?: {me: number, others: number},
 *          wordTimesMs?: {me: number[], others: number[]},
 *          probe?: (spans: DeletionCandidate[], channel: string) => (SpanProbe|null)[]}} [opts]
 * @returns {{report: object}}
 */
export function guardDeletions(channels, opts = {}) {
  const o = { ...DEFAULT_DELETION_OPTS, ...opts };
  const report = {
    detected: false,
    probeAvailable: typeof opts.probe === 'function',
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
  };

  for (const channel of ['me', 'others']) {
    const segments = (channels && channels[channel]) || [];
    const bursts = (opts.speechBursts && opts.speechBursts[channel]) || null;
    const chOut = {
      bursts: bursts ? bursts.length : 0,
      burstSeconds: 0,
      emittedWords: 0,
      candidates: 0,
      probed: 0,
      deletedSpans: 0,
      deletedSeconds: 0,
      unprobedSpans: 0,
      // Without a burst grid there is no physical evidence and therefore no question to ask. Said
      // out loud, because "0 deletions" and "never looked" must never be the same report.
      probeAvailable: report.probeAvailable && Boolean(bursts && bursts.length),
    };
    if (!bursts || !bursts.length) {
      report.byChannel[channel] = chOut;
      continue;
    }

    const found = findDeletionCandidates(segments, bursts, {
      ...o,
      channel,
      wordTimesMs: opts.wordTimesMs ? opts.wordTimesMs[channel] : null,
    });
    report.coverageUnit = report.coverageUnit || found.coverageUnit;
    chOut.burstSeconds = found.burstSeconds;
    chOut.emittedWords = found.emittedWords;
    chOut.candidates = found.candidates.length;
    report.candidates += found.candidates.length;

    const { probe: toProbe, unprobed: overBudget } = selectProbeSpans(found.candidates, o);
    let probes = null;
    if (typeof opts.probe === 'function' && toProbe.length) {
      const res = opts.probe(toProbe, channel) || [];
      probes = {};
      toProbe.forEach((c, i) => {
        probes[c.index] = res[i] || null;
      });
      chOut.probed = res.filter(Boolean).length;
      report.probed += chOut.probed;
    }

    const adj = adjudicateDeletions([...toProbe, ...overBudget], probes, {
      ...o,
      channel,
      peakDb: opts.peaks ? opts.peaks[channel] : undefined,
    });
    chOut.deletedSpans = adj.deletions.length;
    chOut.deletedSeconds = adj.deletedSeconds;
    chOut.unprobedSpans = adj.unprobed.length;
    report.deletions.push(...adj.deletions);
    report.rejected.push(...adj.rejected);
    report.unprobed.push(...adj.unprobed);
    report.deletedSpans += adj.deletions.length;
    report.deletedSeconds = +(report.deletedSeconds + adj.deletedSeconds).toFixed(1);
    report.unprobedSpans += adj.unprobed.length;
    report.byChannel[channel] = chOut;
  }

  report.detected = report.deletedSpans > 0;
  return { report };
}

/** hh:mm:ss for a warning a human has to act on — a millisecond offset is not actionable. */
function clock(ms) {
  const t = Math.max(0, Math.round(Number(ms) || 0) / 1000);
  const h = Math.floor(t / 3600);
  const m = Math.floor((t % 3600) / 60);
  const s = Math.floor(t % 60);
  return `${String(h).padStart(2, '0')}:${String(m).padStart(2, '0')}:${String(s).padStart(2, '0')}`;
}

/**
 * The plain-English `verification.json` warnings for this class — the same vocabulary
 * `repetition-guard.js#guardWarnings` uses for the other detect-only class, deliberately, so a
 * reader meets ONE way of being told the transcript is not clean.
 *
 * One line per deleted span, because a count is not actionable and a span is: the audio is
 * retained, so a named span can be re-transcribed.
 * @param {object} report
 * @returns {string[]}
 */
export function deletionWarnings(report) {
  const dels = (report && report.deletions) || [];
  const out = dels.map(
    (d) =>
      `${d.durationSec}s of speech on the "${d.channel}" channel at ${clock(d.startMs)}–${clock(d.endMs)} is MISSING ` +
      'from the transcript — the audio there carries speech (it re-decodes on its own as ' +
      `"${d.recovered}") and the run emitted no word over it. This class is detected, not repaired: ` +
      'a deletion cannot be filled in without re-transcribing. Audio is retained: re-transcribe, ' +
      'optionally with a different model.',
  );
  const un = (report && report.unprobed) || [];
  if (un.length) {
    out.push(
      `${un.length} span(s) of detected speech carry no emitted word and could NOT be adjudicated ` +
        '(no isolated re-decode was available or the probe budget was exhausted). They are not ' +
        'claimed as deletions and they are not cleared either — this transcript has not been fully checked.',
    );
  }
  return out;
}

export { DEFAULT_DELETION_OPTS as DELETION_GUARD_DEFAULTS };

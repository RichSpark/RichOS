#!/usr/bin/env node
/**
 * Transcript readability — one test per defect the CEO reported on 2026-10-04 (an imported video
 * whose transcript broke sentences across paragraphs, lost punctuation and capitalization partway
 * through, and lost the case of product names). Pure: no whisper, no ffmpeg.
 *
 *   node test/transcript-style.test.mjs
 *
 * The fixture is invented text in the exact SHAPE of the recorded defect (the recording's own words
 * stay in the private record): segments cut mid-sentence, then a window in the lowercase,
 * unpunctuated style with a lowercase "i" and lowercased names. `styledDecode` stands in for the
 * conditioned re-decode of that window's audio, i.e. what the model returns when it is given the
 * transcript's preceding text.
 */
import assert from 'node:assert/strict';
import { renderMarkdown } from '../lib/merge.js';

// Dynamic, so that on a tree WITHOUT the restyle stage each test still runs and fails on its own
// assertion rather than the whole file dying at import.
const restyle = await import('../lib/restyle.js').catch(() => null);
const need = () => assert.ok(restyle, 'there is no restyle stage (lib/restyle.js): run-on windows ship as decoded');
const wordsOf = (t) => String(t || '').split(/\s+/).filter(Boolean);
const normWord = (w) => String(w || '').toLowerCase().replace(/[^\p{L}\p{N}']/gu, '');
const restyleChannel = (...a) => {
  need();
  return restyle.restyleChannel(...a);
};

let failed = 0;
function test(name, fn) {
  try {
    fn();
    console.log(`  ok   ${name}`);
  } catch (err) {
    failed += 1;
    console.log(`  FAIL ${name}\n       ${String(err && err.message ? err.message : err).split('\n').join('\n       ')}`);
  }
}

const seg = (startMs, text) => ({ startMs, endMs: startMs + 5000, text, speaker: 'me', label: 'Me' });

// A punctuated window that ends mid-sentence, then a run-on window, then a punctuated one.
const SEGMENTS = [
  seg(0, 'We tested three tools last week. The first one was fine, but it is hard to compare it with'),
  seg(5000, 'grok because the grok models change every week so back to the list i made'),
  seg(10000, 'you want it on one machine you want it simple and you want it to route through'),
  seg(15000, 'cli proxy which is a good place to start the ui is plain but it had'),
  seg(20000, 'a dashboard all along and i never opened'),
  seg(25000, 'it until today. So that was the first lesson.'),
];

const STYLED =
  'Grok, because the Grok models change every week. So back to the list I made. You want it on ' +
  'one machine. You want it simple, and you want it to route through CLIProxy, which is a good ' +
  'place to start. The UI is plain, but it had a dashboard all along, and I never opened.';

function restyled(styled = STYLED) {
  const prompts = [];
  const res = restyleChannel(SEGMENTS, {
    channel: 'me',
    decode: (span, prompt) => {
      prompts.push({ span, prompt });
      return styled;
    },
  });
  return { ...res, prompts };
}
const runOnText = (segs) => segs.slice(1, 5).map((s) => s.text).join(' ');

console.log('transcript style (CEO report 2026-10-04)');

test('1. contiguous paragraphs end only at sentence ends — a segment cut mid-sentence never starts a new paragraph', () => {
  const md = renderMarkdown({ segments: SEGMENTS, speakers: ['Me'] }, { sessionId: 's' });
  const body = md.split('\n').filter((l) => /^\*\*\[/.test(l));
  for (const line of body.slice(0, -1)) {
    assert.match(line, /[.?!]$/, `a paragraph ended mid-sentence:\n${line}`);
  }
  assert.ok(md.includes('hard to compare it with grok because'), 'the sentence cut between two segments must read through');
  assert.equal(body.length, 1, `one speaker, one unbroken run of sentences: expected 1 paragraph, got ${body.length}`);
  assert.match(body[0], /^\*\*\[00:00\] Me:\*\* We tested/, 'a paragraph keeps the timestamp of its first segment');
  // Joining must not make walls of text: a long paragraph closes at its next sentence end, even
  // inside a segment — and still only at a sentence end.
  const sentence = 'This sentence has exactly ten words in it for counting.';
  const long = [0, 1, 2, 3].map((k) => seg(k * 5000, `${sentence} ${sentence} and then it continues without`));
  const longMd = renderMarkdown({ segments: long, speakers: ['Me'] }, { sessionId: 's' });
  const longBody = longMd.split('\n').filter((l) => /^\*\*\[/.test(l));
  assert.ok(longBody.length >= 2, `a ${4 * 25}-word run of sentences must not be one paragraph`);
  for (const line of longBody.slice(0, -1)) assert.match(line, /[.?!]$/, `a paragraph ended mid-sentence:\n${line}`);
});

test('a late passage after a long gap keeps its own source timestamp', () => {
  const segments = [
    seg(2_833_000, 'the earlier count has no punctuation'),
    { ...seg(3_543_160, 'Priya owns the security sign-off'), endMs: 3_567_880, displayStartMs: 3_564_840 },
    seg(3_568_160, 'by December 3 at noon.'),
  ];
  const md = renderMarkdown({ segments, speakers: ['Me'] }, { sessionId: 's' });
  const body = md.split('\n').filter((line) => line.startsWith('**['));
  assert.deepEqual(body, [
    '**[47:13] Me:** the earlier count has no punctuation',
    '**[59:24] Me:** Priya owns the security sign-off by December 3 at noon.',
  ]);
});

test('2. punctuation holds: a run-on window gets the model\'s own punctuation back, conditioned on the text before it, words unchanged', () => {
  const { segments, report, prompts } = restyled();
  assert.equal(report.spans, 1);
  assert.equal(report.restyled, 1);
  assert.equal(prompts.length, 1);
  assert.ok(
    prompts[0].prompt.endsWith('it is hard to compare it with'),
    `the re-decode must be conditioned on the transcript right up to the span, mid-sentence included; got: ${prompts[0].prompt}`,
  );
  assert.deepEqual(prompts[0].span, { startMs: 5000, endMs: 25000 });
  const after = runOnText(segments);
  assert.ok((after.match(/[.]/g) || []).length >= 4, `expected sentences restored, got: ${after}`);
  assert.ok(after.includes('simple, and'), 'commas come back too');
  // Words are the main pass's words: same comparison forms, the one model-made join aside.
  const before = wordsOf(runOnText(SEGMENTS)).map(normWord).join(' ').replace('cli proxy', 'cliproxy');
  assert.equal(wordsOf(after).map(normWord).join(' '), before);
  // The span ends where the next segment continues the sentence in lowercase: no invented full stop.
  assert.match(segments[4].text, /never opened$/);
  // A version number is not punctuation: a run-on window that names "4.7" is still a run-on window.
  const versions = [
    seg(0, 'We tried it on the old release first.'),
    seg(5000, 'then version 4.7 came out and i moved everything over because the new one is faster and'),
    seg(10000, 'the old one kept failing so now i only run 4.7 on every machine i have at home'),
  ];
  let asked = 0;
  restyle.restyleChannel(versions, { decode: () => { asked += 1; return 'Then version 4.7 came out.'; } });
  assert.equal(asked, 1, 'a decimal point must not hide a run-on window from the restyle stage');
});

test('2b. the re-decode can never add, drop or change a word — only punctuation and case transfer', () => {
  const wrong = STYLED.replace('change every week', 'really change every week').replace('the list I made', 'the list');
  const { segments } = restyled(wrong);
  const after = runOnText(segments);
  assert.ok(!/really/i.test(after), 'a word only the re-decode heard must not enter the transcript');
  const norm = wordsOf(after).map(normWord).join(' ');
  assert.ok(norm.includes('the list i made you want'), `a word only the main pass heard must stay: ${after}`);
  assert.equal(norm, wordsOf(runOnText(SEGMENTS)).map(normWord).join(' ').replace('cli proxy', 'cliproxy'));
  // Where the re-decode heard a DIFFERENT word in the same place, the main pass's word stays and
  // takes the re-decode's punctuation — no stray full stop moved onto the word before it.
  const misheard = runOnText(restyled(STYLED.replace('the list I made.', 'the list I played.')).segments);
  assert.ok(misheard.includes('the list I made. You want'), misheard);
  assert.ok(!/played/.test(misheard) && !/list\. /.test(misheard), misheard);
  // A re-decode that is itself run-on is not used at all.
  const still = restyled(runOnText(SEGMENTS));
  assert.equal(still.report.restyled, 0);
  assert.equal(still.report.unrestored.length, 1);
  assert.equal(runOnText(still.segments), runOnText(SEGMENTS));
  // A long sentence the main pass DID punctuate (commas, no full stop for 25+ words) is never
  // touched: re-punctuating good text can only make it worse.
  const longSentence = [
    seg(0, 'I have one machine at home that every account goes through, and every other machine'),
    seg(5000, 'around the world connects to it over the network, which means nobody can tell'),
    seg(10000, 'that the traffic comes from servers.'),
  ];
  let called = 0;
  const kept = restyle.restyleChannel(longSentence, { decode: () => { called += 1; return 'X.'; } });
  assert.equal(called, 0, 'a punctuated span must not even be re-decoded');
  assert.deepEqual(kept.segments.map((s) => s.text), longSentence.map((s) => s.text));
});

test('3. capitalization holds: sentence starts and the pronoun I are capitalized in a restyled window', () => {
  const { segments } = restyled();
  const after = runOnText(segments);
  assert.ok(!/(^|\s)i(\s|'|$)/.test(after), `lowercase pronoun i survived: ${after}`);
  for (const m of after.matchAll(/[.?!]\s+(\S)/g)) {
    assert.match(m[1], /[A-Z]/, `a sentence starts lowercase in: ${after}`);
  }
  assert.match(segments[2].text, /^You want it on one machine\./);
});

test('4. names keep the case the MODEL gives them — joined names included, and no word list behind it', () => {
  const { segments } = restyled();
  const after = runOnText(segments);
  assert.ok(after.includes('Grok, because the Grok models'), after);
  assert.ok(after.includes('CLIProxy'), `the model's own joined spelling must replace the two-word form: ${after}`);
  assert.ok(/\bUI\b/.test(after), after);
  // Proof there is no hand-made list: where the model itself writes a name in lowercase, it stays so.
  const lower = restyled(STYLED.replace('the Grok models', 'the grok models'));
  assert.ok(runOnText(lower.segments).includes('the grok models'));
});

if (failed) {
  console.log(`${failed} failed`);
  process.exit(1);
}
console.log('all passed');

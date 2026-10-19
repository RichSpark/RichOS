// THE FIFTY-SECOND SILENCE — what the CEO sees while the child compacts its own context.
//
// The first outside user of RichOS, 2026-09-06: a long wait *"looks like a crashed
// application"*. One cause is measured and committed: the child pauses mid-turn to summarize
// its own conversation, for **38.1 to 62.0 seconds** across 16 boundaries
// (`docs/verification/inner-doctrine-opens-2026-09-06/` Q1.3 and
// `docs/verification/compaction-notice-2026-09-06/`), and until this slice the calm surface
// was told NOTHING for the whole of it — every `system` frame fell to
// `Visibility::Technical`.
//
// THE TIMELINE THIS SUITE DRIVES IS NOT INVENTED. It is read off the committed arrival
// stamps of a real `claude` 2.1.263 stream
// (`docs/verification/compaction-notice-2026-09-06/raw/cellT1.jsonl` and its
// `.timed.jsonl` sidecar, joined by position), so the instants below — the announcement 13ms
// into the turn, the heartbeat 30.000s later, the ending at 43.6s — are the wire's own. A
// suite that made its own offsets up would be proving its author's idea of a compaction.
//
// THE SENTENCES ARE NOT THIS FILE'S EITHER. All three are read out of
// `app/crates/richos-core/src/timeline.rs` at run time, so the day somebody rewords one in
// Rust this suite either follows or fails; it cannot quietly test a string the product
// stopped saying.
//
// WHAT IT IS FOR, in one line each:
//
//   1. The pause is NAMED while it is happening, from the frame that announces it and never
//      from the length of a silence.
//   2. The wire's own 30-second heartbeat does not read as silence (this is the regression
//      guard on `QUIET_AFTER_MS`, raised 25000 -> 35000 the same day).
//   3. A child that DIES mid-compaction stops being described — a dead turn must never
//      inherit a busy label from an event that arrived a minute ago.
//   4. Nothing invents progress: no percentage, no estimate, no countdown. The spread is 38
//      to 62 seconds and the app cannot know which.
//   5. WCAG AA, computed in both themes, on every row this copy lands in.
//
// NO AUDIO PATH IS TOUCHED: headless WebKit, no voice mode, no output device.
//
// RUN:  node compaction-notice.js
//       RICHOS_PLAYWRIGHT=/path/to/node_modules/playwright node compaction-notice.js
//       RICHOS_COMPACTION_FRAMES=<dir> node compaction-notice.js     # also writes PNGs

"use strict";

const path = require("path");
const fs = require("fs");
const { loadPlaywright, createRun, assert, assertEqual, UI_DIR } = require("./lib/harness");
const C = require("./lib/contrast");

const APP = "file://" + path.join(UI_DIR, "index.html");
const REPO = path.resolve(UI_DIR, "..", "..", "..");
const RECORD = path.join(REPO, "docs", "verification", "compaction-notice-2026-09-06");
const RUST = path.join(REPO, "richos", "app", "crates", "richos-core", "src", "timeline.rs");
const FRAME_DIR = process.env.RICHOS_COMPACTION_FRAMES || null;

// ---------------------------------------------------------------------------------------
// THE MEASURED TIMELINE, off disk
// ---------------------------------------------------------------------------------------

/// The arrival stamps of one real compaction, in milliseconds from the prompt that triggered
/// it. `cellT1.jsonl` holds the frames and `cellT1.jsonl.timed.jsonl` holds one metadata
/// record per frame IN THE SAME ORDER (the driver writes both from one reader loop), so the
/// two join by position — which is what lets this read the arrival time of a frame whose
/// `status` value only the raw file carries.
function measuredSpan() {
  const raw = fs
    .readFileSync(path.join(RECORD, "raw", "cellT1.jsonl"), "utf8")
    .split("\n")
    .filter((l) => l.trim())
    .map((l) => JSON.parse(l));
  const timed = fs
    .readFileSync(path.join(RECORD, "raw", "cellT1.jsonl.timed.jsonl"), "utf8")
    .split("\n")
    .filter((l) => l.trim())
    .map((l) => JSON.parse(l));
  const frames = timed.filter((e) => e.kind === "frame");
  assertEqual(frames.length, raw.length, "the raw stream and its timing sidecar must line up 1:1 or the join below is fiction");

  // The LAST prompt-send before the first `compacting` frame that actually took time. Turns 1
  // and 2 of that cell each carried a compaction attempt that was abandoned in 1ms
  // (`too_few_groups`), so "the first compacting frame" is not the span this suite wants.
  const spans = [];
  let open = null;
  for (let i = 0; i < raw.length; i++) {
    const f = raw[i];
    if (f.type !== "system" || f.subtype !== "status") continue;
    if (f.status === "compacting") {
      if (!open) open = { startAt: frames[i].atMs, beats: [] };
      else open.beats.push(frames[i].atMs);
    } else if (f.compact_result && open) {
      open.endAt = frames[i].atMs;
      open.result = f.compact_result;
      spans.push(open);
      open = null;
    }
  }
  const real = spans.filter((s) => s.endAt - s.startAt > 1000);
  assert(real.length >= 1, "the committed cell holds no compaction that took longer than a millisecond");
  const s = real[0];
  const sends = timed.filter((e) => e.kind === "send" && /^turn \d+ prompt/.test(e.note || ""));
  const prompt = sends.filter((e) => e.atMs <= s.startAt).pop();
  assert(prompt, "no prompt precedes the measured compaction");
  return {
    promptAt: prompt.atMs,
    announceMs: s.startAt - prompt.atMs,
    beatsMs: s.beats.map((b) => b - prompt.atMs),
    endMs: s.endAt - prompt.atMs,
    result: s.result,
  };
}

/// The three sentences, read out of the crate rather than typed here.
function rustSentences() {
  const src = fs.readFileSync(RUST, "utf8");
  const found = {};
  for (const [key, re] of [
    ["running", /"(Making room[^"]*)"\.to_string\(\)/],
    ["done", /"(Made room[^"]*)"\.to_string\(\)/],
    ["kept", /"(Kept going[^"]*)"\.to_string\(\)/],
  ]) {
    const m = src.match(re);
    assert(m, "timeline.rs no longer contains the " + key + " compaction sentence — this suite is testing a string the product stopped saying");
    found[key] = m[1];
  }
  return found;
}

// ---------------------------------------------------------------------------------------
// The page
// ---------------------------------------------------------------------------------------

/// Captures every listener `main.js` registers, hangs `send_message`, and installs a
/// controllable clock. The same shape `waiting-state.js` uses, and for the same reasons:
/// `send_message` resolving only at the end of the turn IS the shipping contract, and a
/// frozen clock makes an elapsed label a function of the skew alone.
const INIT = `
window.__TAP = { listeners: {}, hang: false };
(function () {
  const BASE = Date.now();
  window.__CLOCK = { skew: 0 };
  Date.now = function () { return BASE + window.__CLOCK.skew; };
})();
let _rb;
Object.defineProperty(window, "RichBridge", {
  configurable: true,
  get() { return _rb; },
  set(v) {
    const ol = v.listen.bind(v), oi = v.invoke.bind(v);
    v.listen = (name, cb) => { (window.__TAP.listeners[name] = window.__TAP.listeners[name] || []).push(cb); return ol(name, cb); };
    v.invoke = (cmd, args) => (window.__TAP.hang && cmd === "send_message") ? new Promise(() => {}) : oi(cmd, args);
    _rb = v;
  }
});
window.__emit = (name, payload) => (window.__TAP.listeners[name] || []).forEach((cb) => cb({ payload }));
`;

async function openApp(browser, theme) {
  const page = await browser.newPage({ viewport: { width: 1280, height: 860 } });
  const errors = [];
  page.on("pageerror", (e) => errors.push(String(e)));
  await page.addInitScript((t) => {
    try {
      window.localStorage.setItem("richos-theme", t);
      window.localStorage.setItem("richos-font-scale", "100");
      window.localStorage.setItem("richos-mock-config", JSON.stringify({ theme: t, font_scale: 100, user_name: null }));
    } catch (e) {
      /* storage unavailable; theme-boot falls back to the shipped default */
    }
  }, theme || "dark");
  await page.addInitScript(INIT);
  await page.goto(APP);
  await page
    .evaluate(() => {
      const s = window.RichSplash;
      if (s && s.state && s.state.shown && !s.state.reason) s.yieldNow("compaction-notice-suite");
    })
    .catch(() => {});
  await page.waitForFunction(() => !document.getElementById("splash"), null, { timeout: 30000 }).catch(() => {});
  await page.waitForFunction("typeof window.RichHome === 'object'", null, { timeout: 30000 }).catch(() => {});
  await page.evaluate(() => {
    if (window.RichHome && window.RichHome.isOpen()) window.RichHome.hide("compaction-notice-suite");
  });
  await page.waitForFunction(() => { const h = document.getElementById("home"); return !h || h.hidden; }, null, { timeout: 30000 }).catch(() => {});
  await page.waitForSelector(".nav-thread", { state: "attached" });
  page.__errors = errors;
  return page;
}

async function startTurn(page, turnId) {
  await page.evaluate(() => { window.__TAP.hang = true; });
  const fence = await page.evaluate(() => {
    const m = window.__RICHOS_TIMELINE__();
    return { entityId: m.entityId, threadId: m.threadId, bindingRevision: m.bindingRevision };
  });
  await page.fill("#input", "What is in the Q4 folder?");
  await page.press("#input", "Enter");
  await page.evaluate(
    (o) => {
      const startedAt = Date.now();
      window.__emit("rich://turn-status", Object.assign({}, o.fence, {
        turnId: o.turnId, status: "queued", startedAt: null, activeDurationMs: null, visibility: "ceo", at: startedAt,
      }));
      window.__emit("rich://turn-status", Object.assign({}, o.fence, {
        turnId: o.turnId, status: "working", startedAt, activeDurationMs: null, visibility: "ceo", at: startedAt,
      }));
    },
    { fence, turnId }
  );
  return fence;
}

/// ONE `rich://activity-upserted`, in the exact shape the crate emits.
///
/// Not a shape guessed at here: `timeline.rs`'s
/// `the_compaction_payload_the_webview_receives_is_printed_here_in_full` serializes the real
/// item and prints it, and these are its keys. The `id` is constant across a span because
/// `merge_into` keeps the OPENING record's `machinery_id` — which is what makes the start,
/// the heartbeats and the ending ONE row rather than four.
async function emitCompaction(page, fence, turnId, id, state, summary) {
  await page.evaluate(
    (o) => {
      const at = Date.now();
      const p = Object.assign({}, o.fence, {
        kind: "activity",
        id: o.id,
        turnId: o.turnId,
        activityType: "other",
        state: o.state,
        summary: o.summary,
        detailRef: o.id,
        sequence: 0,
        slot: "stream",
        createdAt: at,
        startedAt: at,
        visibility: "ceo",
        at,
      });
      if (o.state === "completed") p.completedAt = at;
      window.__emit("rich://activity-upserted", p);
    },
    { fence, turnId, id, state, summary }
  );
}

async function endTurn(page, fence, turnId, status) {
  await page.evaluate(
    (o) => {
      window.__emit("rich://turn-status", Object.assign({}, o.fence, {
        turnId: o.turnId, status: o.status, startedAt: Date.now(), activeDurationMs: 1000, visibility: "ceo", at: Date.now(),
      }));
    },
    { fence, turnId, status }
  );
}

const advance = (page, ms) => page.evaluate((n) => { window.__CLOCK.skew += n; }, ms);

/// Repaint on the band's OWN one-second interval rather than by calling a renderer.
const tick = (page) => page.waitForTimeout(1100);

async function band(page) {
  return page.evaluate(() => {
    const b = document.getElementById("turn-wait");
    if (!b) return null;
    const r = b.getBoundingClientRect();
    return {
      tone: b.dataset.tone,
      head: b.querySelector(".wait-head").textContent,
      time: b.querySelector(".wait-time").textContent,
      detail: b.querySelector(".wait-detail").textContent,
      onScreen: r.width > 0 && r.height > 0 && r.top < innerHeight && r.bottom > 0,
    };
  });
}

const line = (b) => (b ? `${b.head} · ${b.time} — ${b.detail}   [${b.tone}]` : "(no band on screen)");

async function frame(page, name) {
  if (!FRAME_DIR) return;
  fs.mkdirSync(FRAME_DIR, { recursive: true });
  await page.screenshot({ path: path.join(FRAME_DIR, name + ".png") });
}

/// The claim vocabulary a progress display is not allowed to invent — `waiting-state.js`'s
/// list, applied to the sentences this slice adds.
const FABRICATION = [
  /\d+\s*%/,
  /\bstep\s+\d+\s+of\s+\d+/i,
  /\b\d+\s*\/\s*\d+\b/,
  /\balmost (there|done|finished)\b/i,
  /\bnearly (there|done|finished)\b/i,
  /\bshould (be|take)\b/i,
  /\bestimated?\b/i,
  /\bremaining\b/i,
  /\babout (a|an|\d)/i,
  // "Rich is thinking" is the band's words for the moment before anything has come back from Rich
  // (CEO order 2026-10-17); any other use of "thinking" would still be a made-up claim.
  /(?<!\bRich is )\bthinking\b/i,
  /\bprogress\b/i,
];

function assertNoFabrication(text, where) {
  for (const re of FABRICATION) {
    assert(!re.test(text), where + ' says "' + text + '", which matches ' + re + " — a claim about how much of a turn is done, and nothing on the wire carries one");
  }
}

async function main() {
  const { webkit } = loadPlaywright();
  const browser = await webkit.launch();
  const run = createRun("the compaction notice — WebKit");
  const S = rustSentences();
  const M = measuredSpan();

  // =====================================================================================
  // 1. THE SENTENCES ARE THE CRATE'S
  // =====================================================================================

  await run.check("the words the band says about a compaction are read from the crate, not from this file", async () => {
    assertEqual(S.running, "Making room to keep going", "the running sentence in timeline.rs");
    assertEqual(S.done, "Made room to keep going", "the success sentence in timeline.rs");
    assertEqual(S.kept, "Kept going without making room", "the abandoned-attempt sentence in timeline.rs");
    for (const [k, v] of Object.entries(S)) assertNoFabrication(v, "the " + k + " sentence");
    return [S.running, S.done, S.kept].join("  |  ");
  });

  // =====================================================================================
  // 2. THE MEASURED SPAN — before and after, at 10s, 30s and 60s
  // =====================================================================================

  await run.check("a real compacting turn at 10s, 30s and 60s — before this change, and after", async () => {
    const readings = [];

    // ---- BEFORE: exactly what the app did, which is nothing. The compaction frames were
    // `Visibility::Technical`, so the calm surface received no event at all.
    const before = await openApp(browser, "dark");
    const f0 = await startTurn(before, "t_before");
    const beforeAt = {};
    for (const t of [10000, 30000, 60000]) {
      await advance(before, t - (Object.keys(beforeAt).length ? [10000, 30000, 60000][Object.keys(beforeAt).length - 1] : 0));
      await tick(before);
      const b = await band(before);
      beforeAt[t] = b;
      readings.push(`BEFORE  ${String(t / 1000).padStart(2)}s   ${line(b)}`);
      await frame(before, "before-dark-" + t / 1000 + "s");
    }
    // The BEFORE arm's point is that the app said nothing about the pause, and since
    // 2026-09-20 it says nothing at all here: the absence report that used to occupy this
    // line was measured holding for 10.5-14.5 s on five healthy turns and was dropped
    // (`main.js`, "NOTHING YET IS SAID BY SAYING NOTHING"). The claim being made is
    // unchanged and is now made more directly — at 10s into a compaction the old app gave him
    // an empty line where the reason should have been.
    assertEqual(beforeAt[10000].detail, "", "before: 10s — not a word about the pause");
    assertEqual(beforeAt[60000].detail, "Nothing new for 1m 0s", "before: at a minute the app can only report the silence");
    assert(!/room/i.test(beforeAt[60000].detail), "before: nothing on screen names the reason");
    await before.close();

    // ---- AFTER: the same turn, with the events the crate now produces at the instants the
    // wire actually produced them.
    const page = await openApp(browser, "dark");
    const fence = await startTurn(page, "t_after");
    let at = 0;
    const step = async (ms) => { await advance(page, ms - at); at = ms; await tick(page); };

    await step(M.announceMs);
    await emitCompaction(page, fence, "t_after", "mach_compact_1", "running", S.running);
    let b = await band(page);
    assertEqual(b.detail, S.running, "the pause is named the moment the wire announces it (" + M.announceMs + "ms in)");

    await step(10000);
    b = await band(page);
    readings.push(`AFTER   10s   ${line(b)}`);
    await frame(page, "after-dark-10s");
    assert(b.detail.startsWith(S.running), "10s: still named");
    assertEqual(b.tone, "working", "10s: no alarm on work that is going fine");

    await step(30000);
    b = await band(page);
    readings.push(`AFTER   30s   ${line(b)}`);
    await frame(page, "after-dark-30s");
    assert(b.detail.startsWith(S.running), "30s: STILL named — this is the instant the old 25s threshold turned it into an alarm");
    assertEqual(b.tone, "working", "30s: the wire's own heartbeat cadence is 30.000s, so 30s is not silence");

    // The heartbeat, at the measured offset, and then the ending.
    await step(M.beatsMs[0]);
    await emitCompaction(page, fence, "t_after", "mach_compact_1", "running", S.running);
    await step(M.endMs);
    await emitCompaction(page, fence, "t_after", "mach_compact_1", "completed", S.done);
    b = await band(page);
    assertEqual(b.detail, S.done, "the ending changes the tense, at " + M.endMs + "ms, which is when it really arrived");

    await step(60000);
    b = await band(page);
    readings.push(`AFTER   60s   ${line(b)}`);
    await frame(page, "after-dark-60s");
    assert(b.detail.startsWith(S.done), "60s: the last thing that actually happened, with its age");

    // The transcript carries the row too, so the gap is explained on a reload and not only live.
    const rows = await page.evaluate(() => Array.from(document.querySelectorAll(".tl-activity")).map((e) => e.textContent));
    assert(rows.some((t) => t.includes("Made room to keep going")), "the working transcript carries the row: " + JSON.stringify(rows));
    for (const r of readings) assertNoFabrication(r, "a reading");
    await page.close();
    return readings.join("\n          ");
  });

  // =====================================================================================
  // 3. THE LONGEST MEASURED COMPACTION NEVER LOOKS QUIET
  // =====================================================================================

  await run.check("the longest compaction ever measured (62.0s) is described for all of it, never called quiet", async () => {
    const page = await openApp(browser, "dark");
    const fence = await startTurn(page, "t_long");
    await emitCompaction(page, fence, "t_long", "mach_long", "running", S.running);

    // q14's C6 cell, boundary 3: `duration_ms` 62029, the longest of the 16. Heartbeats land
    // at 30.000s and 60.000s, so the largest silence inside it is 30.000s and the last is
    // 2.029s.
    const seen = [];
    for (const beat of [30000, 60000]) {
      await advance(page, beat === 30000 ? 30000 : 30000);
      await tick(page);
      const b = await band(page);
      seen.push(`${beat / 1000}s (just before the heartbeat)  ${line(b)}`);
      assertEqual(b.tone, "working", "a heartbeat every 30.000s must never read as silence");
      assert(b.detail.startsWith(S.running), "and the reason stays on screen");
      await emitCompaction(page, fence, "t_long", "mach_long", "running", S.running);
    }
    await advance(page, 2029);
    await tick(page);
    await emitCompaction(page, fence, "t_long", "mach_long", "completed", S.done);
    const b = await band(page);
    seen.push(`62.029s (the ending)              ${line(b)}`);
    assertEqual(b.detail, S.done);
    await page.close();
    return seen.join("\n          ");
  });

  // =====================================================================================
  // 4. THE HONEST LIMIT — a dead child stops being described
  // =====================================================================================

  await run.check("a child that dies mid-compaction stops being described, and never inherits a busy label", async () => {
    const page = await openApp(browser, "dark");
    const fence = await startTurn(page, "t_dead");
    await emitCompaction(page, fence, "t_dead", "mach_dead", "running", S.running);

    const seen = [];
    // 20s: within the heartbeat cadence, so the label is still the honest answer.
    await advance(page, 20000);
    await tick(page);
    let b = await band(page);
    seen.push(`20s  ${line(b)}`);
    assert(b.detail.startsWith(S.running), "20s: no heartbeat is due yet");

    // 40s: the heartbeat never came. Past QUIET_AFTER_MS the band drops the description and
    // names the silence — because at that point the app genuinely does not know whether the
    // compaction is still running.
    await advance(page, 20000);
    await tick(page);
    b = await band(page);
    seen.push(`40s  ${line(b)}`);
    assertEqual(b.tone, "quiet", "40s: two missed heartbeats is not 'still making room'");
    assert(/^Nothing new for/.test(b.detail), "40s: the count, not the last thing that happened — got " + JSON.stringify(b.detail));
    assert(!/room/i.test(b.detail), "40s: a stale reason must not keep reassuring");

    await advance(page, 120000);
    await tick(page);
    b = await band(page);
    seen.push(`160s ${line(b)}`);
    assertEqual(b.detail, "Nothing new for 2m 40s", "the number keeps growing and keeps being true");

    // And a POSITIVE termination signal takes the whole band away, compaction row or not.
    await endTurn(page, fence, "t_dead", "failed");
    assertEqual(await band(page), null, "a failed turn leaves no band claiming Rich is making room");
    await page.close();
    return seen.join("\n          ");
  });

  // =====================================================================================
  // 5. CONTRAST — computed from the real DOM, both themes, both surfaces this copy lands on
  // =====================================================================================

  for (const theme of ["dark", "light"]) {
    await run.check("WCAG AA on the compaction copy, computed — " + theme, async () => {
      const page = await openApp(browser, theme);
      const painted = await page.evaluate(() => document.documentElement.getAttribute("data-theme"));
      assertEqual(painted, theme, "asked for " + theme + " and the document painted " + painted);

      const fence = await startTurn(page, "t_contrast_" + theme);
      await emitCompaction(page, fence, "t_contrast_" + theme, "mach_c", "running", S.running);
      await tick(page);
      const b = await band(page);
      assert(b.detail.startsWith(S.running), "the copy under measurement must be on screen");

      await page.evaluate(C.pageScript());
      const measured = await page.evaluate(() => {
        const M = window.__contrastMath;
        function bgOf(node) {
          let el = node;
          let acc = null;
          while (el) {
            const c = M.parseCssColor(getComputedStyle(el).backgroundColor);
            if (c && c.a > 0) {
              acc = acc ? M.compositeOver(acc, c) : c;
              if (c.a >= 0.999) return acc;
            }
            el = el.parentElement;
          }
          return null;
        }
        const out = [];
        // Every element that renders the compaction sentence, on the two surfaces it reaches:
        // the waiting band above the composer, and the working transcript's activity row. The
        // mark is a GLYPH here (a character, not a painted dot), so its ink is `color` like
        // the text beside it; it is held to the 3:1 non-text floor because what it carries is
        // a state, not a word.
        const targets = [
          [".wait-detail", false],
          [".tl-activity .tl-activity-text", false],
          [".tl-activity .tl-activity-state", false],
          [".tl-activity .tl-activity-mark", true],
        ];
        for (const [sel, indicator] of targets) {
          const el = document.querySelector(sel);
          if (!el) {
            out.push({ sel, ratio: null, fg: null, bg: null, px: null, floor: indicator ? 3 : 4.5 });
            continue;
          }
          const cs = getComputedStyle(el);
          const fg = M.parseCssColor(cs.color);
          const bg = bgOf(el);
          const px = parseFloat(cs.fontSize);
          out.push({
            sel,
            fg: fg && bg ? M.hex(M.compositeOver(fg, bg)) : null,
            bg: bg ? M.hex(bg) : null,
            px,
            // 3:1 for the non-text indicator; 3:1 for large text (18.66px bold / 24px+);
            // 4.5:1 for everything else. `isLargeText` is the shipping checker's own.
            floor: indicator ? 3 : M.isLargeText(px, cs.fontWeight) ? 3 : 4.5,
            ratio: fg && bg ? M.round2(M.contrastRatio(M.compositeOver(fg, bg), bg)) : null,
          });
        }
        return out;
      });

      const results = [];
      for (const r of measured) {
        assert(r.ratio !== null, r.sel + " is not on screen or could not be resolved against its background — a failure to prove, never a pass");
        assert(
          r.ratio >= r.floor,
          theme + " " + r.sel + " " + r.fg + " on " + r.bg + " at " + r.px + "px = " + r.ratio + ":1, floor " + r.floor + ":1"
        );
        results.push(theme + " " + r.sel.padEnd(34) + " " + String(r.ratio).padStart(6) + ":1  (floor " + r.floor + ")");
      }
      await page.close();
      return results.join("\n          ");
    });
  }

  await browser.close();
  process.exitCode = run.report() ? 1 : 0;
}

main().catch((e) => {
  console.error(e);
  process.exit(1);
});

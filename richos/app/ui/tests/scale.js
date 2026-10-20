// THE TWO PROMISED NUMBERS — RICH-TODOs "virtualization" row.
//
// §25 "Accessibility and performance" makes two claims with a number in them:
//
//     "A 10,000-item thread history remains smooth through virtualization."
//     "A 10,000-entity seeded index remains searchable and bounded."
//
// Until 2026-08-30 neither had a test, and the first had no implementation either — three
// documents said the timeline "uses virtualized infinite scroll" and
// `grep -rn virtualiz app/` returned nothing. A capability a document claims and the tree
// does not deliver is the defect this project exists to catch, and it was sitting in the
// design record for the surface the CEO looks at most.
//
// ---------------------------------------------------------------------------------------
// WHAT THIS SUITE PINS, AND WHY IT IS NOT ONLY TIMINGS
// ---------------------------------------------------------------------------------------
//
// A budget in milliseconds is a real check and it is also a flaky one: it moves with the
// machine, the load and the WebKit build. So every timing here is paired with a STRUCTURAL
// check of the property that produces it — reuse is asserted by NODE IDENTITY, correctness
// of that reuse by a byte-for-byte comparison against a from-scratch render, and the
// projection's fast path against the brute-force scan it replaced. If the budgets are ever
// loosened, those keep working.
//
// The budgets are set roughly an order of magnitude above the measured figure and roughly
// an order of magnitude below the number this work started from, so they catch a return of
// the defect and not a busy laptop. Measured and baseline figures are named at each one.
// Full sweeps: `docs/verification/timeline-scale-2026-08-30/`, tool `lib/scale-bench.js`.
//
// AND THAT PARAGRAPH WAS NOT ENOUGH, WHICH IS WORTH SAYING HERE RATHER THAN ONLY AT THE TWO
// SITES. It was written for "a busy laptop" and the runner is not one: seventeen runs of this
// suite on `macos-latest`, same code, read 443-932ms for first paint against a 900ms ceiling
// and 38-101ms for a structural change against a 90ms one. An order of magnitude of headroom
// was claimed; measured against the worst run, the ceilings stood at 0.97x and 0.89x of it —
// that is, BELOW the worst observation, INSIDE their own distributions. Both went through on a
// commit whose whole diff was two files under `docs/verification/`. So the rule is sharper now:
//
//   A WALL-CLOCK MILLISECOND COUNT IS NOT A GATE IN THIS FILE. It is measured and printed on
//   every run, and it gates only when an env var supplies the number — `RICHOS_FRAME_BUDGET_MS`,
//   `RICHOS_FIRST_PAINT_BUDGET_MS`, `RICHOS_STRUCT_BUDGET_MS` — which is how the CEO's own Mac
//   or any machine with GPU compositing still gets the absolute answer.
//
//   WHAT GATES IN CI IS WHAT DIVIDES THE MACHINE OUT: node identity, byte-identical markup,
//   zero DOM writes, a count of mounted sections, a fast path against the scan it replaced,
//   and RATIOS between two arms measured in the same page family in the same minute.
//
// The two exceptions are named where they sit and both are JS-only costs with real headroom
// on the same seventeen runs: the JS half of a structural change (4-9ms against 20) and the
// model projection (3-17ms against 20, and now the median of five samples rather than one).
//
// THE SECOND NUMBER IS NOT IN THIS FILE, AND THAT IS SAID OUT LOUD RATHER THAN LEFT TO A
// READER. The 10,000-entity index is `run_search` in `app/src-tauri/src/main.rs`, a
// different component in a different language; its test is the Rust one named in check 10,
// and check 10 exists so that half of the row cannot quietly lose its test while this half
// keeps passing.
//
// NO PAGINATION (standing rule, CEO). Check 9 is structural: at 10,000 items every turn is
// mounted and the DOM carries no page control of any kind. This work made the timeline
// faster by reusing nodes, NEVER by rendering fewer turns.
//
// Every check here was run RED once, by breaking the thing it guards in the shipped source.
// Those runs are transcribed in
// `docs/verification/timeline-scale-2026-08-30/mutation-runs.txt`.

"use strict";

const fs = require("fs");
const path = require("path");
const { loadPlaywright, openFixture, createRun, assert, assertEqual, UI_DIR } = require("./lib/harness.js");
const { makeSnapshot, oneMoreActivity, ENTITY, THREAD, T0 } = require("./lib/scale-fixtures.js");

// THE PROMISED NUMBER, at a realistic turn shape: 1,000 turns of ten items.
const TURNS = 1000;
const PER_TURN = 10;
const ITEMS = TURNS * PER_TURN;

// Budgets. Left column is what this branch measures, right is what the tree measured before
// it (docs/verification/timeline-scale-2026-08-30/baseline.txt).
// A structural change is TWO costs and they move independently. The JS half — projection,
// signatures, DOM construction — is what turn reuse removed, and it is the one that
// regresses if reuse ever breaks. The rest is WebKit re-measuring a 1,000-child flex column
// when one child changes height, which is a floor imposed by mounting every turn (and
// mounting every turn is the NO PAGINATION rule, so it stays). Two numbers, because one would
// hide which half moved — and, since 2026-09-15, only one of the two is a GATE.
const BUDGET_STRUCT_JS_MS = 20; // measured  2ms | baseline ~60ms of the 257
// AND THE TOTAL IS OFF THE GATE, 2026-09-15, for the reason the paragraph above already gives:
// the rest of it is WebKit re-measuring a 1,000-child flex column, which is a fact about the
// machine. `BUDGET_STRUCT_MS = 90` asserted it anyway. Seventeen runs of this suite on
// `macos-latest`, same code, sorted — `gh api repos/WebDevBooster/richos/actions/runs/<id>/logs`:
//
//   total  38 40 45 45 49 52 53 58 60 60 67 67 68 70 71 99 101      <- budget was 90
//   JS      4  5  5  5  5  6  6  6  6  6  7  8  9  9  9             <- budget is 20
//
// The ceiling sat inside the distribution and went through it twice: run 34458859801 read 101
// on 2026-09-10 and run 34971100374 read 99 on 2026-09-15, the second of them a
// `workflow_dispatch` of a commit whose whole diff was two files under `docs/verification/`.
// The JS column — the half turn reuse actually removed, and the half that regresses if reuse
// ever breaks — never got past 9 against its 20, on the same runs, on the same machines.
//
// So the JS half stays the gate, check 3 keeps asserting the same property by node identity,
// and the total is printed on every run and asserted only on request:
//
//     RICHOS_STRUCT_BUDGET_MS=90 node scale.js
const STRUCT_BUDGET_MS = Number(process.env.RICHOS_STRUCT_BUDGET_MS || 0);
// THE FRAME-RATE NUMBER IS OFF THE GATE, AND THIS IS WHERE IT WENT. It is not deleted, it
// is not loosened, and it is not asserted on a machine whose speed nobody controls:
//
//     RICHOS_FRAME_BUDGET_MS=30 node scale.js
//
// turns it back into a gate, and it is measured and PRINTED on every run either way. What it
// answers is "is this machine quick enough to hold 60fps over a 10,000-item thread", which is
// a question about the machine — 17ms here (vsync), 23ms p50 / 68ms p95 on a GitHub
// `macos-latest` runner (run 33933067025), which has three vCPUs and no GPU. Run it on the
// hardware the CEO actually uses, or on a machine with GPU compositing, and it means what it
// says; run it on a shared virtualized runner and it measures the runner.
//
// WHAT REPLACED IT AS THE GATE is two things that do not move with the machine: the drag over
// the whole history writes NOTHING to the DOM (check 6), and the SAME motion costs no more per
// frame at 10,000 items than at a short thread (check 7). Zero is zero on any machine, and a
// ratio divides the machine out of both halves.
const FRAME_BUDGET_MS = Number(process.env.RICHOS_FRAME_BUDGET_MS || 0);
// THE SHAPE, WITHOUT A MACHINE IN IT: 10,000 items must not cost more PER FRAME than a short
// thread does, in the SAME motion, at the same moment, in the same browser. A per-frame cost
// proportional to history length — which is the defect, and is what 257ms of structural work
// on the frame looked like — would read 8.3x here, because that is the ratio of the two arms'
// histories (1,000 turns against CONTROL_TURNS). Measured on this machine, median frame at
// 1,000 turns against the control: 1.00x at 1280x900 quiet, 1.00x under a load average of 34
// on ten cores, and 2.35x at 2560x1600 under that same load — a viewport 4.5x the area, which
// is the harshest per-frame rasterization cost this machine can be made to produce and is
// harsher than the runner's own (the runner's median frame was 23ms; that arm's was 40ms).
// 5 is a shade over twice the worst figure measured and still 1.7x under the defect it guards.
const BUDGET_FRAME_RATIO = 5;
// The control arm's history, and the number is derived rather than picked. The motion below
// steps ONE VIEWPORT per frame for 60 frames — 900px x 60 = 54,000px of travel — so the
// control must be at least that tall or it wraps and re-reads screenfuls it has already
// rasterized. At the fixture's measured 518.19px per turn (518,189px / 1,000 turns), 54,000px
// is 104.2 turns; 120 measures 62,223px, which clears the travel plus a whole viewport with
// room over. THE 40 THAT WAS HERE COULD NOT: 40 turns is 20,773px, 23 viewports, so at the big
// arm's own 8,636px step it wrapped twenty times in 60 frames and spent two frames in three on
// tiles it had already rasterized. That is what it was really measuring when it read 20ms
// against the big arm's 68ms on the runner, and it is why the ratio arm did not save the check.
const CONTROL_TURNS = 120;
// FIRST PAINT USED TO BE AN ABSOLUTE NUMBER HERE — `BUDGET_FIRST_PAINT_MS = 900` — AND IT HAD
// BEEN A COIN FLIP FOR FIVE DAYS. It read 265ms on the machine it was written on. This is what
// the SAME code read on `macos-latest`, one figure per run, oldest first, pulled with
// `gh api repos/WebDevBooster/richos/actions/runs/<id>/logs` over seventeen runs of this suite:
//
//   764 625 712 607 720 565 735 744 891 457 597 443 568 834 932 781 874
//
// The ceiling sat INSIDE that distribution. Run 34458859801 read 891 on 2026-09-10 — 99.0% of
// it — and nobody saw the near miss because that run was already red on the structural total
// below (101ms against 90), which is its own entry in this story. Run 34966364532
// read 932 and turned the whole workflow red on a commit whose entire diff was two new files
// under `docs/verification/`. Run 34970135120 was a `workflow_dispatch` of THAT SAME SHA
// thirty-nine minutes later: 781ms, green. A second dispatch of the same SHA, run 34971100374,
// read 874. One commit, three runs, 932 / 781 / 874 — the check was measuring the runner and
// reporting it as the timeline.
//
// SO IT GETS THE REPAIR CHECK 7 ALREADY HAS, the same shape for the same reason: a control arm
// in the same page family, interleaved, compared as a RATIO, with the absolute figure printed
// on every run and asserted only when somebody asks for it on hardware where the answer means
// something —
//
//     RICHOS_FIRST_PAINT_BUDGET_MS=900 node scale.js
//
// THE CONTROL IS HALF THE THREAD, not the 120 turns check 7 uses, and that is measured rather
// than tidy. The measurement ends with two `requestAnimationFrame`s, a fixed toll of whole
// frames that is about a fifth of a 500-turn paint and about half of a 120-turn one; a control
// small enough for the toll to dominate makes the RATIO move with the machine, which is the
// defect being removed. Measured here idle, then with the whole process tree pinned to the
// efficiency cores by `taskpolicy -b` — a 2.5x slower control arm:
//
//   control 120 turns   idle 6.29 6.17 6.04 5.98   throttled 2.78
//   control 250 turns   idle 3.30                  throttled 2.44
//   control 500 turns   idle 1.99 1.98 1.93 1.88   throttled 1.63 1.60 1.56
//
// Half the thread is the only one of the three whose ratio survives the machine changing under
// it. A cost proportional to history reads 2.0 there, and a quadratic one reads 4.0.
const CONTROL_PAINT_TURNS = TURNS / 2;
const BUDGET_FIRST_PAINT_RATIO = 3;
// The absolute figure, kept and named. It is a measurement of the machine, so it is printed on
// every run and gates only on request; 900 is the old ceiling and the value to pass on real
// hardware.
const FIRST_PAINT_BUDGET_MS = Number(process.env.RICHOS_FIRST_PAINT_BUDGET_MS || 0);
// Set BELOW the baseline on purpose. 34ms is what the tree measured before this work
// (apply 15 + turnsOf 19); a budget of 60 would have passed on the very code it exists to
// catch, which is a check that cannot fail. 20ms is 10x the measured figure and still well
// under the number it guards against.
const BUDGET_PROJECTION_MS = 20; // measured 2ms | baseline 34ms (apply 15 + turnsOf 19)

/// Install the shared page state every check below works from: one 10,000-item model,
/// rendered once, with the shipping options object.
async function seed(page, snapshot) {
  return page.evaluate(async (snap) => {
    const messages = document.getElementById("messages");
    const model = window.RichTimeline.createModel();
    window.RichTimeline.bind(model, snap.entityId, snap.threadId, snap.bindingRevision);
    const t0 = performance.now();
    window.RichTimeline.applySnapshot(model, snap);
    const applyMs = performance.now() - t0;
    const p0 = performance.now();
    window.RichTimeline.turnsOf(model);
    const turnsOfMs = performance.now() - p0;

    window.__model = model;
    window.__opts = {
      now: 1787950000000,
      expandedMessages: new Set(),
      avatarAlreadyShown: true,
      isExpanded: (id) => window.RichTimeline.isTurnExpanded(model, id),
      toggle: () => {},
      rerender: () => {},
      copy: () => {},
      retry: () => {},
      openWorker: () => {},
    };
    messages.innerHTML = "";
    window.RichTimeline.render(model, messages, window.__opts);
    // LET IT ACTUALLY PAINT HERE. Otherwise the first render in the next check pays for the
    // initial layout of all 32,001 nodes and reports it as the cost of adding one activity
    // row — measured 181ms of somebody else's bill before this line existed, and 24ms with
    // only a forced `scrollHeight` and no frame.
    void document.getElementById("conversation").scrollHeight;
    await new Promise((res) => requestAnimationFrame(() => requestAnimationFrame(res)));
    return { applyMs, turnsOfMs, turns: model.turnOrder.length, items: model.items.size };
  }, snapshot);
}

/// ONE MEASUREMENT, USED BY BOTH ARMS — so "the same measurement" is a fact about the code
/// rather than a claim in a comment, the rule `SCROLL_SRC` below already follows. Runs inside
/// the page, against whatever snapshot it is handed, on a page `seed()` has already warmed:
/// fonts resolved, stylesheet applied, one full render paid for. An arm measured cold against
/// an arm measured warm is not a ratio.
///
/// `sections` comes back with the timings because the title's first clause is "opening the
/// thread PAINTS", and a render that mounted nothing is fast.
const FIRST_PAINT_SRC = async (snap) => {
  const messages = document.getElementById("messages");
  const conv = document.getElementById("conversation");
  const model = window.RichTimeline.createModel();
  window.RichTimeline.bind(model, snap.entityId, snap.threadId, snap.bindingRevision);
  const a0 = performance.now();
  window.RichTimeline.applySnapshot(model, snap);
  const applyMs = performance.now() - a0;
  const p0 = performance.now();
  window.RichTimeline.turnsOf(model);
  const turnsOfMs = performance.now() - p0;
  const opts = Object.assign({}, window.__opts, {
    isExpanded: (id) => window.RichTimeline.isTurnExpanded(model, id),
  });
  messages.innerHTML = "";
  const f0 = performance.now();
  window.RichTimeline.render(model, messages, opts);
  void conv.scrollHeight;
  await new Promise((res) => requestAnimationFrame(() => requestAnimationFrame(res)));
  return {
    applyMs: applyMs,
    turnsOfMs: turnsOfMs,
    firstPaintMs: performance.now() - f0,
    sections: messages.querySelectorAll(":scope > .tl-turn").length,
  };
};

/// ONE MOTION, USED BY BOTH ARMS AND BY THE STRUCTURAL DRAG — so "the same motion" is a fact
/// about the code rather than a claim in a comment. Runs inside the page.
///
/// `px` is the step; omitted, it is whatever covers the WHOLE history in `frames` steps.
/// `wraps` counts the times the position ran off the top and restarted, because an arm that
/// wraps is re-reading screenfuls it has already rasterized and is no longer comparable to one
/// that does not. `mutations` counts every DOM record the timeline produces during the drag —
/// scrolling a fully-mounted timeline must produce none.
const SCROLL_SRC = async (o) => {
  const conv = document.getElementById("conversation");
  const messages = document.getElementById("messages");
  conv.scrollTop = conv.scrollHeight;
  await new Promise((res) => requestAnimationFrame(res));
  let mutations = 0;
  const mo = new MutationObserver((recs) => {
    mutations += recs.length;
  });
  mo.observe(messages, { childList: true, subtree: true, attributes: true, characterData: true });
  // THE OTHER HALF OF "WORK PER FRAME", and the DOM counter above cannot see it: a renderer
  // that re-projects and re-renders on every scroll frame writes NOTHING when turn reuse is
  // working, so a zero mutation count is compatible with the whole thread being rebuilt in JS
  // sixty times. Measured: adding `container.parentNode.addEventListener("scroll", () =>
  // render(...))` to timeline.js leaves mutations at 0 and the frame interval at vsync (17ms,
  // because 4ms of JS fits inside a 16.7ms frame), and this counter goes from 0 to 180,180.
  //
  // Nothing can render this timeline without READING THE MODEL, so the model is what is
  // counted. Three accessors over the shipped object, restored the moment the drag ends.
  const model = window.__model;
  const KEYS = ["turnOrder", "items", "turns"];
  const backing = {};
  let modelReads = 0;
  for (const k of KEYS) {
    backing[k] = model[k];
    Object.defineProperty(model, k, {
      configurable: true,
      get() { modelReads++; return backing[k]; },
      set(v) { backing[k] = v; },
    });
  }
  const frames = [];
  const n = o.frames;
  const step = o.px || Math.max(1, Math.floor(conv.scrollHeight / n));
  let pos = conv.scrollHeight;
  let wraps = 0;
  let prev = performance.now();
  for (let i = 0; i < n; i++) {
    pos -= step;
    if (pos < 0) {
      pos = conv.scrollHeight;
      wraps++;
    }
    conv.scrollTop = pos;
    await new Promise((res) => requestAnimationFrame(res));
    const now = performance.now();
    frames.push(now - prev);
    prev = now;
  }
  mo.disconnect();
  for (const k of KEYS) {
    delete model[k];
    model[k] = backing[k];
  }
  const moved = conv.scrollHeight - conv.scrollTop;
  frames.sort((a, b) => a - b);
  return {
    p50: frames[Math.floor(n / 2)],
    p95: frames[Math.floor(n * 0.95)],
    max: frames[n - 1],
    px: conv.scrollHeight,
    step,
    wraps,
    mutations,
    modelReads,
    traveled: moved,
    overflowY: getComputedStyle(conv).overflowY,
    viewport: conv.clientHeight,
  };
};

async function main() {
  const pw = loadPlaywright();
  const browser = await pw.webkit.launch();
  const run = createRun("§25 the timeline at 10,000 items — WebKit");
  const page = await openFixture(browser, { width: 1280, height: 900 });

  const snapshot = makeSnapshot(TURNS, PER_TURN);
  const snapshotItems = snapshot.items.length;
  const lastTurnId = "turn_" + (TURNS - 1);
  const seeded = await seed(page, snapshot);

  await run.check("the fixture really is a 10,000-item thread, not a small one named 10,000", async () => {
    // THE EMPTY-CORPUS GUARD. Every budget below is meaningless if the model is small, and
    // "10,000" in a constant is not evidence that 10,000 items reached the renderer.
    // `applySnapshot` folds every `work_duration` row into its turn RECORD instead of
    // storing it as an item (one per turn), so a 10,000-item snapshot is 9,000 model items.
    // Both numbers are asserted: the wire payload is the promised size and the model is the
    // exact projection of it.
    assertEqual(snapshotItems, ITEMS, "items on the wire");
    assertEqual(seeded.items, ITEMS - TURNS, "items in the model (durations live on the turn record)");
    assertEqual(seeded.turns, TURNS, "turns in the model");
    const drawn = await page.evaluate(() => document.querySelectorAll("#messages > .tl-turn").length);
    assertEqual(drawn, TURNS, "turn sections in the DOM");
    return `${snapshotItems} items on the wire -> ${seeded.items} model items + ${seeded.turns} turn records, ${drawn} mounted sections`;
  });

  await run.check("ONE new activity row does not rebuild the thread", async () => {
    // THE DEFECT THIS ROW WAS ABOUT. `render` used to empty the container and rebuild every
    // turn on every structural change — and a structural change is every activity row,
    // every worker upsert, every turn-status transition, dozens of them in one 52-second
    // multi-tool turn. Measured 257ms each at this size before 2cf03b0.
    const r = await page.evaluate(
      ({ payloads }) => {
        const messages = document.getElementById("messages");
        const conv = document.getElementById("conversation");
        const times = [];
        const js = [];
        for (const p of payloads) {
          window.RichTimeline.onActivityUpserted(window.__model, p);
          const t0 = performance.now();
          window.RichTimeline.render(window.__model, messages, window.__opts);
          js.push(performance.now() - t0);
          void conv.scrollHeight; // the layout the CEO pays for, not just the JS
          times.push(performance.now() - t0);
        }
        times.sort((a, b) => a - b);
        js.sort((a, b) => a - b);
        return { median: times[2], worst: times[4], js: js[2] };
      },
      { payloads: [0, 1, 2, 3, 4].map((i) => oneMoreActivity(lastTurnId, i)) }
    );
    assert(
      r.js <= BUDGET_STRUCT_JS_MS,
      `the JS half of a structural change at ${ITEMS} items took ${r.js.toFixed(0)}ms ` +
        `(budget ${BUDGET_STRUCT_JS_MS}ms) — the thread is being REBUILT rather than reused`
    );
    if (STRUCT_BUDGET_MS > 0) {
      assert(
        r.median <= STRUCT_BUDGET_MS,
        `a structural change at ${ITEMS} items took ${r.median.toFixed(0)}ms ` +
          `(gated at ${STRUCT_BUDGET_MS}ms by RICHOS_STRUCT_BUDGET_MS, baseline before this work 257ms)`
      );
    }
    return (
      `median of 5: ${r.js.toFixed(0)}ms of JS (budget ${BUDGET_STRUCT_JS_MS}ms; baseline ~60ms of the 257), ` +
      `${r.median.toFixed(0)}ms total on THIS machine` +
      (STRUCT_BUDGET_MS > 0
        ? ` (gated at ${STRUCT_BUDGET_MS}ms by RICHOS_STRUCT_BUDGET_MS)`
        : " — not gated here; `RICHOS_STRUCT_BUDGET_MS=90 node scale.js` on real hardware gates it") +
      `. The remainder is WebKit re-measuring ${TURNS} mounted flex children.`
    );
  });

  await run.check("STRUCTURAL: the other 999 turns keep the DOM nodes they already had", async () => {
    // The timing above is the symptom; THIS is the property. Node identity, not innerHTML —
    // an equal-looking rebuild would pass a text comparison and fail here.
    const r = await page.evaluate(({ payload }) => {
      const messages = document.getElementById("messages");
      const before = Array.from(messages.children);
      window.RichTimeline.onActivityUpserted(window.__model, payload);
      window.RichTimeline.render(window.__model, messages, window.__opts);
      const after = Array.from(messages.children);
      let same = 0;
      let replaced = [];
      for (let i = 0; i < Math.min(before.length, after.length); i++) {
        if (before[i] === after[i]) same++;
        else replaced.push(after[i].dataset.turnId);
      }
      return { beforeLen: before.length, afterLen: after.length, same, replaced };
    }, { payload: oneMoreActivity(lastTurnId, 99) });
    assertEqual(r.afterLen, TURNS, "the turn count is unchanged");
    assertEqual(r.replaced, [lastTurnId], "exactly the turn that changed is rebuilt, and only it");
    assertEqual(r.same, TURNS - 1, "every other turn is the SAME node object");
    return `${r.same}/${r.afterLen} sections identical by reference; rebuilt: ${r.replaced.join(", ")}`;
  });

  await run.check("CORRECTNESS: the reused DOM is byte-identical to a from-scratch render", async () => {
    // The check that makes the one above safe. Reuse is only ever an optimization if what
    // is on screen is what a full rebuild would have produced — so the same model is
    // rendered into a SECOND container, which has no cache and therefore builds everything,
    // and the two are compared as markup.
    const r = await page.evaluate(() => {
      const messages = document.getElementById("messages");
      const fresh = document.createElement("div");
      document.body.appendChild(fresh);
      window.RichTimeline.render(window.__model, fresh, window.__opts);
      const a = messages.innerHTML;
      const b = fresh.innerHTML;
      fresh.remove();
      let at = -1;
      for (let i = 0; i < Math.max(a.length, b.length); i++) {
        if (a[i] !== b[i]) {
          at = i;
          break;
        }
      }
      return { equal: a === b, length: a.length, at, a: a.slice(Math.max(0, at - 60), at + 60), b: b.slice(Math.max(0, at - 60), at + 60) };
    });
    assert(r.equal, `the incrementally-updated DOM diverges at character ${r.at}:\n          reused: ${r.a}\n          fresh:  ${r.b}`);
    return `${r.length} characters of markup, identical`;
  });

  await run.check("a streamed delta reaches a turn whose nodes were reused", async () => {
    // THE REGRESSION REUSE WOULD HAVE INTRODUCED. `onMessageDelta` used to do
    // `existing.text += p.textDelta` — mutating the item behind the reference the signature
    // is built from. Harmless while every render was a full rebuild; with reuse it freezes
    // the prose at whatever the last structural render caught. The delta path now replaces
    // the object, and this drives it through the RENDER path (not `updateProse`) to prove
    // the item, not the node write, is what carries the text.
    const r = await page.evaluate(() => {
      const messages = document.getElementById("messages");
      const model = window.__model;
      const turnId = "turn_500";
      const messageId = turnId + ":text:0";
      const before = model.items.get(messageId).text;
      window.RichTimeline.onMessageDelta(model, {
        entityId: model.entityId,
        threadId: model.threadId,
        bindingRevision: 1,
        turnId: turnId,
        messageId: messageId,
        textDelta: " AND THE TAIL THE DELTA ADDED.",
        at: 1787949000000,
      });
      window.RichTimeline.render(model, messages, window.__opts);
      const node = messages.querySelector('[id="prose:' + messageId + '"]');
      return { before: before, model: model.items.get(messageId).text, onScreen: node ? node.textContent : null };
    });
    assert(r.onScreen !== null, "the prose node must still be on screen");
    assert(
      r.onScreen.endsWith(" AND THE TAIL THE DELTA ADDED."),
      `a structural render after a delta showed stale prose:\n          on screen: ${JSON.stringify(r.onScreen.slice(-60))}`
    );
    assertEqual(r.onScreen, r.model, "the screen and the model agree");
    return `${r.before.length} chars -> ${r.onScreen.length} on screen, tail present after a full render`;
  });

  await run.check(`dragging the whole ${ITEMS}-item history does NO work — nothing read, nothing written`, async () => {
    // THE WORK PER FRAME, COUNTED RATHER THAN TIMED — and the gate this check now is.
    //
    // Deliberately harsher than a real flick: the viewport is dragged through the ENTIRE
    // history in 60 steps, so content is revealed the whole way. What must be true of that
    // drag is that the timeline does no work during it: no re-projection, no re-render, no
    // node swapped, no attribute rewritten, no text node touched. NO PAGINATION means every
    // turn is already mounted (check 10), so a scroll is WebKit moving a layer and nothing
    // else — and a future "optimization" that virtualizes by rebuilding on scroll, or a
    // scroll handler that re-renders, would show up here as a non-zero count on ANY machine.
    //
    // TWO INSTRUMENTS, BECAUSE ONE OF THEM IS BLIND ON ITS OWN. A MutationObserver over the
    // container counts what the drag WRITES; a set of accessors over the shipped model counts
    // what it READS. The second exists because turn reuse makes the first miss the regression
    // it is aimed at: a renderer wired to re-render on every scroll frame rebuilds nothing,
    // writes nothing, and is invisible to a DOM counter — measured, not supposed, and the
    // mutation run is transcribed at the foot of this file.
    //
    // ZERO IS ZERO EVERYWHERE. That is the whole reason this and not a millisecond: the frame
    // budget that used to live here answered "is this runner fast?", which is not the question
    // §25 asks. The timing half is check 7, as a RATIO, and the absolute number is printed
    // below and gated only when a machine worth measuring is asked for.
    const r = await page.evaluate(SCROLL_SRC, { frames: 60 });

    // THE POSITIVE PROBE FOR BOTH COUNTERS. A count of zero from an observer that never
    // attached is the same number as a count of zero from a timeline that did no work, and
    // this repository has shipped a scanner that reported CLEAN over an empty corpus. So the
    // SAME two instruments, over the same container and the same model, are made to count:
    // one activity row upserted and rendered has to move both off zero.
    const probe = await page.evaluate(({ payload }) => {
      const messages = document.getElementById("messages");
      const model = window.__model;
      let seen = 0;
      let reads = 0;
      const mo = new MutationObserver((recs) => { seen += recs.length; });
      mo.observe(messages, { childList: true, subtree: true, attributes: true, characterData: true });
      const KEYS = ["turnOrder", "items", "turns"];
      const backing = {};
      for (const k of KEYS) {
        backing[k] = model[k];
        Object.defineProperty(model, k, {
          configurable: true,
          get() { reads++; return backing[k]; },
          set(v) { backing[k] = v; },
        });
      }
      window.RichTimeline.onActivityUpserted(model, payload);
      window.RichTimeline.render(model, messages, window.__opts);
      for (const k of KEYS) { delete model[k]; model[k] = backing[k]; }
      return new Promise((res) => setTimeout(() => { mo.disconnect(); res({ seen, reads }); }, 0));
    }, { payload: oneMoreActivity(lastTurnId, 777) });

    // THE OTHER POSITIVE PROBE, and a mutation run earned it: `scrollTop` can be SET on a
    // container with `overflow-y: hidden`, so a check that never looks passes perfectly on a
    // timeline the CEO cannot scroll at all. §15 is a section about a SCROLL CONTAINER;
    // assert that it is one before asserting anything about scrolling it.
    assert(
      r.overflowY === "auto" || r.overflowY === "scroll",
      `#conversation is not a scroll container — overflow-y is ${r.overflowY}`
    );
    assert(r.px > r.viewport * 10, `the history must overflow the viewport — ${r.px}px in ${r.viewport}px`);
    assert(r.traveled > 100000, `the scroll must actually travel — it moved ${r.traveled}px`);
    assert(r.wraps === 0, `the drag must cover fresh history every frame — it wrapped ${r.wraps} times`);
    assert(probe.seen > 0, "the MutationObserver counted nothing when the DOM was rewritten under it — a zero above would prove nothing");
    assert(probe.reads > 0, "the model-read counter counted nothing when the model was rendered under it — a zero above would prove nothing");
    assertEqual(r.mutations, 0, `the timeline rewrote its own DOM while the CEO was only scrolling`);
    assertEqual(r.modelReads, 0, `the renderer read the model while the CEO was only scrolling — the thread is being re-projected per frame`);
    return (
      `${r.px}px of history dragged in 60 steps of ${r.step}px, ${r.traveled}px traveled, ` +
      `0 DOM mutations and 0 model reads (the same two instruments counted ${probe.seen} and ${probe.reads} ` +
      `when one row was rendered under them) · frames on THIS machine, reported not asserted: ` +
      `p50 ${r.p50.toFixed(0)}ms, p95 ${r.p95.toFixed(0)}ms, max ${r.max.toFixed(0)}ms`
    );
  });

  await run.check(`${ITEMS} items cost no more PER FRAME than a short thread, in the same motion`, async () => {
    // THE PROMISE §25 ACTUALLY MAKES, with the machine divided out of both halves.
    //
    // AN ABSOLUTE FRAME BUDGET LIVED HERE AND WAS RED ON EVERY PUBLIC RUN. It read 17-20ms on
    // the machine it was written on and 68ms (p95) on a GitHub `macos-latest` runner — three
    // vCPUs, no GPU, software rasterization — which is a fact about the runner. It had a ratio
    // arm as a second chance and the ratio went red too, at 3.40x, and THAT is the part worth
    // writing down, because the ratio was not measuring what it said either:
    //
    //   THE OLD CONTROL WAS 40 TURNS AT THE BIG ARM'S OWN STEP. The big arm covers 518,173px
    //   in 60 steps, so its step is 8,636px — 9.6 viewports. A 40-turn history is 20,773px,
    //   two and a bit steps, so the control WRAPPED twenty times and spent most of its frames
    //   re-rasterizing three screenfuls it had already drawn. Warm tiles against cold tiles is
    //   not "the same motion over a shorter thread"; on a machine fast enough for both arms to
    //   sit on vsync the difference is invisible (both read 17ms here), and on a runner where
    //   rasterization is the bottleneck it IS the ratio.
    //
    // SO THE ARMS ARE MATCHED ON THE THING THAT COSTS: one viewport of FRESH content per
    // frame, 60 frames, no wrap, in both. Same pixels rasterized per frame, same content
    // complexity, same browser, same viewport; the only difference left is how much timeline
    // is mounted behind it — 1,000 turns against ${CONTROL_TURNS}. Interleaved pass by pass so
    // a machine that gets busy halfway gets busy for both arms, and compared on the MEDIAN
    // frame rather than the p95: over 60 samples the p95 is the fourth-worst frame, which on a
    // shared virtual machine is a scheduling statistic and not a property of the timeline.
    //
    // IT CAN STILL FAIL, and by a mile. A per-frame cost proportional to history length reads
    // 8.3x here — that is the ratio of the two histories — against a budget of 5.
    const step = await page.evaluate(() => document.getElementById("conversation").clientHeight);
    const small = await openFixture(browser, { width: 1280, height: 900 });
    await seed(small, makeSnapshot(CONTROL_TURNS, PER_TURN));

    // THREE PASSES A SIDE, INTERLEAVED, MEDIAN OF THE MEDIANS. One 60-frame sample is one
    // sample; a ratio built from one of each is a ratio built from two coin flips.
    const PASSES = 3;
    const big = [];
    const ctrl = [];
    for (let i = 0; i < PASSES; i++) {
      big.push(await page.evaluate(SCROLL_SRC, { frames: 60, px: step }));
      ctrl.push(await small.evaluate(SCROLL_SRC, { frames: 60, px: step }));
    }
    await small.close();
    const mid = (rs) => rs.slice().sort((a, b) => a.p50 - b.p50)[Math.floor(rs.length / 2)];
    const r = mid(big);
    const c = mid(ctrl);

    // The control has to BE a control: the same motion, neither arm wrapping, and a genuinely
    // shorter thread behind it.
    assert(c.step === r.step, `the two arms did not take the same step (${r.step}px vs ${c.step}px)`);
    assert(r.wraps === 0 && c.wraps === 0, `an arm wrapped and re-read warm tiles — big ${r.wraps}, control ${c.wraps}`);
    assert(c.px * 4 < r.px, `the control is not a short history — ${c.px}px against ${r.px}px`);
    assert(
      r.mutations === 0 && c.mutations === 0 && r.modelReads === 0 && c.modelReads === 0,
      "the timeline did work mid-scroll — check 6 owns that failure and names which instrument saw it"
    );

    const ratio = r.p50 / Math.max(c.p50, 0.001);
    const proportional = TURNS / CONTROL_TURNS;
    const said =
      `median frame ${r.p50.toFixed(0)}ms over ${r.px}px of history against ${c.p50.toFixed(0)}ms over ` +
      `${CONTROL_TURNS} turns (${c.px}px), same ${r.step}px step, ${PASSES} interleaved passes a side — ` +
      `${ITEMS} items cost ${ratio.toFixed(2)}x a short thread per frame ` +
      `(budget ${BUDGET_FRAME_RATIO}x; a cost proportional to history would read ${proportional.toFixed(1)}x)`;
    assert(ratio <= BUDGET_FRAME_RATIO, `per-frame cost is scaling with history length: ${said}`);

    // THE ABSOLUTE NUMBER, KEPT AND NAMED. It is a measurement of the machine, so it is
    // printed on every run and gates only when someone asks it to, on hardware where the
    // answer means something — the CEO's own Mac, or any machine with GPU compositing.
    const abs =
      `p50 ${r.p50.toFixed(0)}ms, p95 ${r.p95.toFixed(0)}ms, max ${r.max.toFixed(0)}ms on THIS machine` +
      (FRAME_BUDGET_MS > 0
        ? ` (gated at ${FRAME_BUDGET_MS}ms by RICHOS_FRAME_BUDGET_MS)`
        : " — not gated here; `RICHOS_FRAME_BUDGET_MS=30 node scale.js` on real hardware gates it");
    if (FRAME_BUDGET_MS > 0) {
      assert(
        r.p95 <= FRAME_BUDGET_MS,
        `scroll frames at ${ITEMS} items: ${abs}`
      );
    }
    return said + `\n          ${abs}`;
  });

  await run.check("opening the thread paints, and the projection is not quadratic", async () => {
    const half = await openFixture(browser, { width: 1280, height: 900 });
    const halfSnapshot = makeSnapshot(CONTROL_PAINT_TURNS, PER_TURN);
    await seed(half, halfSnapshot);

    // FIVE PASSES A SIDE, INTERLEAVED, MEDIAN EACH SIDE — check 7's rule for check 7's reason
    // (a ratio built from one sample a side is a ratio built from two coin flips, and
    // interleaving means a machine that gets busy halfway gets busy for both arms), and FIVE
    // rather than check 7's three because the spread was measured at both, on this machine,
    // with nothing else changed:
    //
    //   three passes, eleven runs   1.70 1.92 1.95 1.99 2.01 2.06 2.07 2.12 2.13 2.20 2.26
    //   five passes, six runs       1.94 1.95 1.98 1.98 1.99 1.99
    //
    // Five costs about a second in a suite that takes 35s on the runner and it takes the
    // spread from 1.33x to 1.03x, which is what puts real daylight under the 3x budget.
    const PASSES = 5;
    const big = [];
    const ctrl = [];
    for (let i = 0; i < PASSES; i++) {
      big.push(await page.evaluate(FIRST_PAINT_SRC, snapshot));
      ctrl.push(await half.evaluate(FIRST_PAINT_SRC, halfSnapshot));
    }
    await half.close();
    const mid = (rs) => rs.slice().sort((a, b) => a.firstPaintMs - b.firstPaintMs)[Math.floor(rs.length / 2)];
    const r = mid(big);
    const c = mid(ctrl);

    // THE WORD "PAINTS" IN THE TITLE, ASSERTED RATHER THAN TIMED. Two arms that both rendered
    // nothing have a ratio of 1.0 and would sail through everything below this line.
    assertEqual(r.sections, TURNS, "the big arm did not mount the whole thread");
    assertEqual(c.sections, CONTROL_PAINT_TURNS, "the control arm did not mount its thread");

    const ratio = r.firstPaintMs / Math.max(c.firstPaintMs, 0.001);
    const proportional = TURNS / CONTROL_PAINT_TURNS;
    const said =
      `first paint ${r.firstPaintMs.toFixed(0)}ms over ${TURNS} turns against ` +
      `${c.firstPaintMs.toFixed(0)}ms over ${CONTROL_PAINT_TURNS}, ${PASSES} interleaved passes a side — ` +
      `${proportional.toFixed(0)}x the thread costs ${ratio.toFixed(2)}x the paint ` +
      `(budget ${BUDGET_FIRST_PAINT_RATIO}x; proportional reads ${proportional.toFixed(1)}x, ` +
      `quadratic ${(proportional * proportional).toFixed(1)}x)`;
    assert(ratio <= BUDGET_FIRST_PAINT_RATIO, `opening the thread costs more than its length: ${said}`);

    // THE PROJECTION STAYS AN ABSOLUTE NUMBER, and that is a decision rather than an oversight.
    // A control arm was tried here too and it does not survive contact with the quantity: at
    // 1,000 turns the projection reads 2-4ms and at 500 it reads 1-2ms, so the ratio comes out
    // 1.50, 3.00, 3.00 over three benches of five passes — `performance.now()` resolution and
    // JIT noise, not a measurement. Measured, and the reason this one is not converted.
    //
    // WHAT IT IS INSTEAD is the MEDIAN of the passes rather than one sample. Seventeen runs on
    // `macos-latest` read 3 6 6 7 7 7 8 8 9 9 9 9 11 12 12 17 against a budget of 20, and the
    // 17 is a single-sample outlier of exactly the kind five passes and a median remove. The
    // budget is deliberately BELOW the 34ms baseline (a budget of 60 would pass on the very
    // code this exists to catch), so there is no room to widen it, and the honest response to
    // a noisy sample is to take a better sample.
    const projection = big
      .map((x) => x.applyMs + x.turnsOfMs)
      .sort((a, b) => a - b)[Math.floor(PASSES / 2)];
    assert(
      projection <= BUDGET_PROJECTION_MS,
      `model + projection took ${projection.toFixed(0)}ms (budget ${BUDGET_PROJECTION_MS}ms, ` +
        `baseline 34ms) — a per-item scan over the turn list is back`
    );

    // THE ABSOLUTE NUMBER, KEPT AND NAMED — check 7's closing paragraph, applied here.
    const abs =
      `${r.firstPaintMs.toFixed(0)}ms absolute on THIS machine` +
      (FIRST_PAINT_BUDGET_MS > 0
        ? ` (gated at ${FIRST_PAINT_BUDGET_MS}ms by RICHOS_FIRST_PAINT_BUDGET_MS)`
        : " — not gated here; `RICHOS_FIRST_PAINT_BUDGET_MS=900 node scale.js` on real hardware gates it") +
      `; model + projection ${projection.toFixed(0)}ms, median of ${PASSES} ` +
      `(budget ${BUDGET_PROJECTION_MS}, was 34ms)`;
    if (FIRST_PAINT_BUDGET_MS > 0) {
      assert(r.firstPaintMs <= FIRST_PAINT_BUDGET_MS, `first paint at ${ITEMS} items: ${abs}`);
    }
    return said + `\n          ${abs}`;
  });

  await run.check("§9.2's steering cue: the fast predicate agrees with the scan it replaced", async () => {
    // `addedWhileWorking` went from `spans.some(...)` per turn to a binary search against a
    // running TOP-TWO maximum, and the top-two is the whole subtlety: a turn's own span
    // always reaches its own prompt, so a plain maximum answers "yes" for everything. This
    // runs both over a mix built to break a careless version — turns that overlap, a turn
    // nested wholly inside another, a live turn with no end, and ordinary turns that must
    // NOT be flagged — and requires them to agree message for message.
    const r = await page.evaluate(() => {
      const model = window.RichTimeline.createModel();
      window.RichTimeline.bind(model, "northwind", "thr_steer", 1);
      const T = 1787000000000;
      //
      // THE LIVE TURN IS LAST ON PURPOSE. Its span is open-ended (`to: Infinity`), so every
      // prompt after it is inside SOME other turn's span and legitimately flagged — put it
      // in the middle and the "not steering" half of this check has nothing left to hold,
      // which is exactly how the first version of this mix read.
      //
      // [turnId, promptAt, startedAt, activeMs|null, live]
      const spec = [
        ["a", T + 0, T + 10, 5000, false], // long turn, prompt before its own start -> quiet
        ["b", T + 2000, T + 2100, 100, false], // prompt landed INSIDE a's span -> steering
        ["c", T + 3000, T + 3100, 20000, false], // starts inside a, runs past it -> steering
        ["d", T + 4000, T + 4100, 50, false], // nested inside BOTH a and c -> steering
        ["g", T + 500000, T + 500010, 30, false], // alone and quiet -> NOT steering
        ["h", T + 500020, T + 500100, 40, false], // prompt inside g's SHORT span -> steering
        ["i", T + 700000, T + 700010, 40, false], // alone and quiet -> NOT steering
        // THE ONLY SHAPE IN WHICH THE EXCLUSION IS DECIDABLE AT ALL. Everywhere else the
        // prompt lands BEFORE its own turn started (`PromptReceived` is written before
        // `TurnStarted`), so a turn's own span is not even a candidate and any predicate
        // agrees. Here the prompt is AT the start, the own span IS the running maximum, and
        // only keeping the RUNNER-UP gets the answer right. A mutation run earned this row:
        // without it, replacing the top-two with a plain maximum passed.
        ["j", T + 800000, T + 800000, 5000, false], // prompt ON its own start -> quiet
        ["e", T + 900000, T + 900010, null, true], // live, no end -> quiet itself
        ["f", T + 950000, T + 950010, 10, false], // inside e's OPEN span -> steering
      ];
      const items = [];
      for (const [id, promptAt, startedAt, activeMs, live] of spec) {
        items.push({
          kind: "user_message", id: "turn_" + id + ":user", entityId: "northwind", threadId: "thr_steer",
          turnId: "turn_" + id, bindingRevision: 1, createdAt: promptAt, slot: "opening", sequence: null,
          visibility: "ceo", text: "prompt " + id, source: "text",
        });
        items.push({
          kind: "work_duration", id: "turn_" + id + ":duration", entityId: "northwind", threadId: "thr_steer",
          turnId: "turn_" + id, bindingRevision: 1, createdAt: promptAt, slot: "terminal", sequence: null,
          visibility: "ceo", state: live ? "working" : "completed", startedAt: startedAt,
          endedAt: activeMs === null ? null : startedAt + activeMs, activeMs: activeMs,
        });
      }
      window.RichTimeline.applySnapshot(model, {
        entityId: "northwind", threadId: "thr_steer", mode: "ceo", bindingRevision: 1, items: items,
      });
      for (const [id, , , , live] of spec) if (live) model.turns.get("turn_" + id).live = true;

      // THE BRUTE FORCE, written here rather than imported — this is the definition the
      // shipped predicate has to match, and importing it would compare the fast path to
      // itself.
      const spans = [];
      for (const [id, t] of model.turns) {
        if (typeof t.startedAt !== "number") continue;
        const end = typeof t.activeMs === "number" ? t.startedAt + t.activeMs : t.live ? Infinity : null;
        if (end === null) continue;
        spans.push({ id: id, from: t.startedAt, to: end });
      }
      const brute = (turnId, at) =>
        spans.some((s) => s.id !== turnId && at >= s.from && at <= s.to);

      const shipped = {};
      const expected = {};
      for (const turn of window.RichTimeline.turnsOf(model)) {
        if (!turn.user) continue;
        shipped[turn.turnId] = !!turn.user.steering;
        expected[turn.turnId] = brute(turn.turnId, turn.user.createdAt);
      }
      return { shipped, expected, spans: spans.length };
    });
    assertEqual(r.spans, 10, "the mix must actually produce spans");
    assertEqual(r.shipped, r.expected, "the fast predicate disagrees with the brute-force scan");
    const flagged = Object.keys(r.expected).filter((k) => r.expected[k]);
    const quiet = Object.keys(r.expected).filter((k) => !r.expected[k]);
    // POSITIVE AND NEGATIVE BOTH PRESENT — an all-false agreement would agree with anything.
    assert(flagged.length >= 3, "the mix must flag some messages as steering");
    assert(quiet.length >= 3, "the mix must leave some messages unflagged");
    return `10 turns, ${flagged.length} steering (${flagged.join(",")}), ${quiet.length} not (${quiet.join(",")}) — both agree`;
  });

  await run.check("a turn with a record is always in the turn order", async () => {
    // The implication `turnRecord`'s guard now relies on. It used to re-scan `turnOrder` for
    // every ITEM; it now only scans when it CREATES the record, which is only sound while
    // "has a record" implies "is in the order". Driven through every mutation site that
    // touches either: snapshot, live upsert, an optimistic bubble adopted onto a real turn,
    // a local notice (which is in the order with NO record — the reverse, and legal), and a
    // supersede, which drops one turn and re-inserts the replacement in its place.
    const r = await page.evaluate(() => {
      const model = window.RichTimeline.createModel();
      window.RichTimeline.bind(model, "northwind", "thr_inv", 1);
      const mk = (turnId, id, extra) =>
        Object.assign({ id, entityId: "northwind", threadId: "thr_inv", turnId, bindingRevision: 1,
          createdAt: 1787000000000, slot: "stream", sequence: 0, visibility: "ceo" }, extra);
      const violations = [];
      const everSeen = new Set();
      let checkpoints = 0;
      const checkpoint = (label) => {
        checkpoints++;
        for (const id of model.turns.keys()) everSeen.add(id);
        for (const id of model.turns.keys()) {
          if (model.turnOrder.indexOf(id) < 0) violations.push(label + ": " + id);
        }
        const seen = new Set();
        for (const id of model.turnOrder) {
          if (seen.has(id)) violations.push(label + ": duplicate in turnOrder: " + id);
          seen.add(id);
        }
      };

      window.RichTimeline.applySnapshot(model, { entityId: "northwind", threadId: "thr_inv", mode: "ceo",
        bindingRevision: 1, items: [mk("t1", "t1:a", { kind: "activity", activityType: "read", state: "completed", summary: "x" })] });
      checkpoint("after snapshot");

      // The payload IS the item (STREAMING.md), so it is passed flat.
      window.RichTimeline.onActivityUpserted(model,
        mk("t2", "t2:a", { kind: "activity", activityType: "read", state: "completed", summary: "y", at: 1787000000000 }));
      checkpoint("after a live activity on a new turn");

      window.RichTimeline.addPendingUserMessage(model, "hello", 1787000000001);
      window.RichTimeline.onTurnStatus(model, { entityId: "northwind", threadId: "thr_inv", bindingRevision: 1,
        turnId: "t3", status: "working", at: 1787000000002 });
      checkpoint("after an optimistic bubble was adopted");

      window.RichTimeline.addLocalNotice(model, "a locally authored line", 1787000000003);
      checkpoint("after a local notice (in the order with no record — the legal reverse)");

      window.RichTimeline.onTurnStatus(model, { entityId: "northwind", threadId: "thr_inv", bindingRevision: 1,
        turnId: "t4", status: "queued", supersedesTurnId: "t3", at: 1787000000004 });
      checkpoint("after a supersede dropped one turn and inserted its replacement");

      return { violations, everSeen: Array.from(everSeen).sort(), checkpoints, turns: model.turns.size, order: model.turnOrder.length };
    });
    assertEqual(r.violations, [], "a turn had a record but no place in the order");
    // THE EMPTY-CORPUS GUARD. Four distinct turn records are created across the sweep and
    // three survive it — `onTurnStatus` drops the superseded turn before creating its
    // replacement, so the two are never both present and the peak is three, not four.
    // Naming every id ever seen is what proves the sequence really ran: a check on the end
    // state alone would pass on a model where `onTurnStatus` had quietly stopped creating
    // records at all.
    assertEqual(r.everSeen, ["t1", "t2", "t3", "t4"], "four distinct turn records were created across the sweep");
    assertEqual(r.turns, 3, "three survive the supersede");
    assertEqual(r.checkpoints, 5, "every mutation site was visited");
    return `${r.everSeen.join(",")} created -> ${r.turns} after the supersede, ${r.order} ordered ids, ${r.checkpoints} checkpoints, 0 violations`;
  });

  await run.check("NO PAGINATION: every turn is mounted and no page control exists", async () => {
    // THE STANDING RULE (CEO), pinned structurally rather than trusted. This work made the
    // timeline faster by REUSING nodes; if a future change makes it faster by rendering
    // fewer turns behind a page control, it fails here. Infinite scroll, virtualization and
    // behind-the-scenes chunked loading are the sanctioned answers and none of them adds a
    // control with a page number on it.
    const r = await page.evaluate(() => {
      const messages = document.getElementById("messages");
      const sections = messages.querySelectorAll(":scope > .tl-turn");
      const ids = new Set(Array.from(sections).map((s) => s.dataset.turnId));
      const text = messages.textContent;
      const banned = [];
      for (const re of [/\bpage\s*\d+\b/i, /\bnext\s+page\b/i, /\bprevious\s+page\b/i, /\bpage\s+\d+\s+of\s+\d+\b/i, /\bshow\s+more\s+(turns|history|messages)\b/i, /\bload\s+more\b/i]) {
        const m = text.match(re);
        if (m) banned.push(m[0]);
      }
      const controls = messages.querySelectorAll("[class*=pagin], [class*=pager], [data-page], nav[aria-label*=agination]");
      return { sections: sections.length, distinct: ids.size, banned, controls: controls.length, chars: text.length };
    });
    assertEqual(r.sections, TURNS, "every turn of the 10,000-item history is mounted");
    assertEqual(r.distinct, TURNS, "and each one exactly once");
    assertEqual(r.banned, [], "page-control wording reached the CEO's timeline");
    assertEqual(r.controls, 0, "a pagination control reached the CEO's timeline");
    assert(r.chars > 100000, `the scan must have real text to scan — it saw ${r.chars} characters`);
    return `${r.sections} turns mounted, ${r.chars} characters scanned, 0 page controls, 0 page wording`;
  });

  await run.check("the OTHER promised number has a test, and it is not in this file", async () => {
    // §25 makes two claims with 10,000 in them. This suite pins the thread history; the
    // entity index is `run_search` in the Tauri shell, matched in Rust so thread bodies
    // never cross the IPC boundary. Joining them here is what stops half the row silently
    // losing its coverage while this half stays green — the same drift `docs-claims.js`
    // catches between this directory and its README.
    const rs = path.join(UI_DIR, "..", "src-tauri", "src", "main.rs");
    const src = fs.readFileSync(rs, "utf8");
    const name = "a_ten_thousand_entity_index_stays_searchable_and_the_result_stays_bounded";
    assert(src.includes("fn " + name + "()"), `${name} is gone from ${rs} — the 10,000-entity half of the row has no test`);
    assert(src.includes("hits.truncate(limit)"), "the bound that test asserts is gone from run_search");
    assert(src.includes("set_entity_registry(seeded)"), "the test no longer seeds a registry — it would be testing the four dogfood entities");
    for (const n of ["10_000", "assert_eq!(hits.len(), 40"]) {
      assert(src.includes(n), `the 10,000-entity test no longer contains ${n}`);
    }
    return `app/src-tauri/src/main.rs::${name} — run it with \`cargo test -p richos-tauri\` (12ms, 40 hits, debug build)`;
  });

  await run.check("no page errors anywhere in this suite", async () => {
    assertEqual(page.__errors, [], "uncaught errors in the fixture");
    return "0 uncaught errors, 0 console errors";
  });

  await page.close();
  const failed = run.report();
  await browser.close();
  return failed;
}

main().then((f) => { process.exitCode = f ? 1 : 0; });

// ---------------------------------------------------------------------------------------
// THE MUTATION RUNS BEHIND CHECKS 6 AND 7, 2026-09-05
// ---------------------------------------------------------------------------------------
//
// Both checks replace an absolute frame budget that was RED on every public `ui-suite-ci`
// run. A check that goes green by asserting less is worse than the red one it replaced, so
// each of these was run RED by breaking the thing it guards, on this machine, at 1280x900.
//
//  1. RE-RENDER ON SCROLL, REUSE INTACT — `timeline.js`'s `render()` given
//     `container.parentNode.addEventListener("scroll", () => render(model, container, opts))`.
//     This is the regression the row is about and it is INVISIBLE to every instrument this
//     check had before today: DOM mutations 0 (turn reuse rewrites nothing), median frame
//     17ms (4ms of JS fits inside a 16.7ms frame), ratio 1.00x. The model-read counter went
//     from 0 to 180,180 and check 6 went red. That run is why the counter exists.
//
//  2. FULL REBUILD ON SCROLL — the same listener plus `renderCache.delete(container)`, so
//     every frame rebuilds all 1,000 turns. Check 6 red on the DOM counter (60,060
//     mutations). Check 7 red on the ratio, measured with check 6's guard lifted so the
//     number could be read: median frame 363ms against the control's 40ms = 9.07x, against
//     a 5x budget and an 8.3x proportional-cost expectation. Checks 2 and 3 went red too,
//     which is correct — they own reuse.
//
//  3. THE CONTROL PUT BACK TO 40 TURNS — check 7 red on `an arm wrapped and re-read warm
//     tiles — big 0, control 2`. That guard is the whole reason this pair is not the old
//     check with a bigger number in it.
//
// WHAT IS NOT COVERED, SAID PLAINLY. Check 7's ratio is the weaker half and it is the only
// thing left holding the browser's OWN per-frame cost — a CSS or DOM-shape change that makes
// WebKit's work proportional to document height rather than to the viewport (a lost
// containment, a filter, a sticky child, a shadow over the whole column). Nothing in JS
// reads or writes on that path, so check 6 cannot see it, and a 5x budget will only catch it
// once it is gross. The absolute 60fps claim is no longer gated anywhere in CI; it is
// measured and printed on every run, and `RICHOS_FRAME_BUDGET_MS=30 node scale.js` on
// hardware with GPU compositing is what turns it back into a gate.

// ---------------------------------------------------------------------------------------
// THE MUTATION RUNS BEHIND CHECKS 2 AND 8, 2026-09-15
// ---------------------------------------------------------------------------------------
//
// Same rule as the section above: a check that goes green by asserting less is worse than the
// red one it replaced, so each of these was run by breaking the thing it guards in the SHIPPED
// source (`app/ui/timeline.js`), then restoring it. All on this machine, WebKit, 1280x900.
// The clean baseline it is read against is `2x the thread costs 1.94-1.99x the paint` over six
// runs, and `3ms of JS` / `2-3ms model + projection`.
//
//  1. A PER-TURN POSITIONAL SCAN over the turn list in `render()` — `for (const other of
//     turns) { if (other.turnId === turn.turnId) break; pos++; }`. Textbook O(n^2), and
//     NOTHING WENT RED: 1.99x and 1.94x, first paint 385ms and 375ms against a clean 374ms.
//     This is the most useful of the six and it is recorded first for that reason — 10^6
//     trivial comparisons is about a millisecond, and this paint costs 0.32ms PER TURN in DOM
//     construction and layout. A quadratic term small enough to hide inside that is also small
//     enough not to matter, and no arrangement of budgets in this file will ever see it.
//
//  2. THE SAME NESTED SCAN, ONE `turnSignature` PER PAIR — a dedupe written as a nested loop.
//     First paint 380 -> 958-1072ms, and it sits ON the 3x bite point: at three passes a side
//     it read 2.77x (GREEN) and 3.14x (red), and at the five this file now uses, 3.02x (red).
//     Recorded as a boundary rather than as a pass — a regression this size is caught here
//     most of the time and not every time. Check 2 takes it every time regardless: the JS half
//     of a structural change went to 607ms against its 20ms budget.
//
//  3. THE SAME SCAN, THREE SIGNATURES PER PAIR. First paint 380 -> 1757-2406ms and the ratio
//     3.51x and 3.86x at three passes, 3.55x at five — RED every time. That is where the bite
//     point actually is: a super-linear cost that roughly triples the time to open the CEO's
//     thread. Run again with the whole process tree on the efficiency cores (`taskpolicy -b`,
//     a 2.5x slower machine): 2406ms against the control's 694ms, 3.47x, still RED. That run
//     is the point of the whole change — the gate survives the machine changing under it,
//     which the 900ms ceiling it replaces did not.
//
//  4. `render()` MOUNTING ONLY THE NEWEST 200 TURNS — `turnsOf(model).slice(-200)`, the "make
//     it fast by rendering fewer turns" regression. Check 8 red on its own structural arm,
//     `the big arm did not mount the whole thread`, which is the arm added with this change:
//     before it, two arms that both rendered nothing would have had a ratio of 1.0 and sailed
//     through. Checks 1, 3, 5, 7 and 9 red too, which is correct — they own that property.
//
//  5. `turnIndex` BUILT WITH `ids.indexOf(ids[i])` — the projection's own O(n^2). GREEN
//     everywhere, for mutation 1's reason: 10^6 cheap comparisons against a 20ms budget.
//
//  6. `turnIndex` BUILT BY SCANNING `model.items` PER TURN — a genuine per-item scan over the
//     turn list, the defect the projection budget names. Model + projection 2-3ms -> 33-41ms,
//     RED on its own message. So the 20ms budget still bites after being changed from one
//     sample to the median of five.
//
// AND THE TWO OPT-IN GATES WERE RUN RED TOO, because a knob that does nothing is worse than no
// knob. `RICHOS_FIRST_PAINT_BUDGET_MS=1 node scale.js` -> `first paint at 10000 items: 397ms
// absolute on THIS machine (gated at 1ms ...)`. `RICHOS_STRUCT_BUDGET_MS=1 node scale.js` ->
// `a structural change at 10000 items took 30ms (gated at 1ms ...)`.
//
// WHAT IS NOT COVERED, SAID PLAINLY. The ratio only sees costs that grow FASTER than the
// thread. A regression that makes every turn 30% more expensive to paint moves both arms
// equally and the ratio does not move at all — that one is the absolute figure's job, it is
// printed on every run, and `RICHOS_FIRST_PAINT_BUDGET_MS=900 node scale.js` on real hardware
// is what turns it back into a gate. The same sentence the frame budget above already carries,
// for the same reason, and it is the price of a number that does not lie on a shared runner.

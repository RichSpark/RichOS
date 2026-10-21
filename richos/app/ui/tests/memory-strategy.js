// §26 — THE DETERMINISTIC `memory-strategy` FIXTURE, DRIVEN END TO END THROUGH THE REAL SHELL.
//
// This is the acceptance instrument for Phase 3's exit gate ("the representative
// memory-strategy fixture renders correctly from start to finish"), so it runs against the
// thing that ships: `index.html` + `main.js` + `mock.js` + `timeline.js` + `style.css`, all
// loaded from disk under WebKit, with nothing stubbed but the Tauri bridge that `mock.js`
// already replaces. The turn is STARTED BY TYPING THE PROMPT AND PRESSING ENTER — not by a
// side door into the renderer — so the CEO bubble is the shell's own optimistic bubble,
// re-keyed by `adoptPendingUserMessage` when `rich://turn-status: queued` names the turn.
//
// ---------------------------------------------------------------------------------------
// THE NEGATIVE HALF, WHICH IS THE POINT
// ---------------------------------------------------------------------------------------
//
// §26 prescribes sixteen steps. Two of them — "Sage failure" and "Plan update from step 2
// of 5 to step 3 of 5" — have no signal anywhere in this runtime, and eight more are only
// partly producible. Emitting them anyway would light up renderer branches production can
// never reach, and a green run over those branches would certify states that cannot occur.
//
// So `mock.js` emits none of them, and this suite ASSERTS THEIR ABSENCE. `MEMORY_STRATEGY_STEPS`
// carries a row per step with `status`, what is produced, what is not, and the file:line
// that makes it so; the checks below walk that table and require the DOM to agree with it in
// both directions. "We did not fake it" is a test here, not a promise — and if a signal
// lands later, these checks fail loudly and point at the row to update.
//
// ---------------------------------------------------------------------------------------
// THE CLOCK
// ---------------------------------------------------------------------------------------
//
// Injected as an anchor: the wall-clock instant the scenario's virtual t=0 maps onto. Every
// timestamp is `anchor + offset`, and the renderer derives the display from those timestamps
// exactly as §6.2 requires ("Persist timestamps and derive the display locally"). Nothing is
// monkey-patched, nothing sleeps, and the full two-hour turn is walked in under a
// millisecond. `Working for 18s` is produced by anchoring 18.6s in the past and reading the
// real row.
//
//   2h 17m 50s = (2 x 3600) + (17 x 60) + 50 = 7200 + 1020 + 50 = 8270 s = 8_270_000 ms
//
// which is asserted below as a MEASURED span (`endedAt - startedAt`), never as a literal.

"use strict";

const path = require("path");
const fs = require("fs");
const { leaveHome, loadPlaywright, shot, publishShotFile, createRun, assert, assertEqual, awaitWorkerChipSettled, pinClock, unpinClock, SEED_THEME, FRAME_STILL, UI_DIR } = require("./lib/harness");

const APP = "file://" + path.join(UI_DIR, "index.html");

// §26 names nine screenshots as required deliverables, so unlike `.shots/` — the per-run
// scratch every other suite writes and gitignores — these NINE are committed. They are the
// first visual record this UI has ever had; the display on this machine has been locked for
// three slices and `screencapture` returns a valid single-color (0,0,0) PNG, so every one
// of them comes out of WebKit's own compositor and is pixel-counted before it counts.
const SHOTS_26 = path.join(__dirname, "shots-26");

// The lease handoff (t=0 accepted -> the clock starts) plus the 18s §26 asks to see.
const ANCHOR_18S = 18600;
const ANCHOR_SUB_SECOND = 700; // 100ms of elapsed time: §6.1's under-one-second row

const shots = [];
async function evidence(page, name, note) {
  // THE CHROME OF THIS SURFACE IS STILL MOVING WHEN THE WALK HAS FINISHED DRIVING IT. The shell
  // polls `get_worker_status` every 3,000ms (`main.js:3098`) and rebuilds the drill chip from
  // the answer, so a shot taken between a step this suite APPLIED and the next tick shows the
  // step before it. Measured over five same-source runs on 2026-09-19: `ms-05` alternated
  // between `· 1 I can't see` and `· 1 done · 1 I can't see`, 962 pixels at delta 131, with
  // `ms-03` and `ms-04` moving in the same box. The wait is on the product's own re-render
  // agreeing with itself twice, not on a duration — see `lib/harness.js`.
  const settled = await awaitWorkerChipSettled(page);
  if (!settled.settled) {
    throw new Error(
      `${name}: the work chip was still changing after the budget (${settled.renders} re-renders, ` +
        `last text ${JSON.stringify(settled.text)}). A shot of a surface that is still moving is ` +
        `evidence of a moment, not of a state.`
    );
  }
  // AND THE LIVE COUNTER IN THE TURN ROW COUNTS THIS SUITE, NOT THE SCENARIO. §26's clock is
  // injected so the row reads `Working for 18s` (check 2 above proves it, measured), and then
  // `main.js:1437` keeps ticking it for as long as the row is live — so by shot 4 it read 34s
  // in one run and 35s in the next, and five of these nine shots moved between two same-source
  // runs for no reason other than how long the walk took. The pin puts every shot at the
  // fixture's own instant, `startedAt + 18,600 ms`, which is the state check 2 asserts; the
  // recompute goes through the product's own visibility handler, so nothing here re-implements
  // the formatter. Restored in the `finally`, so the walk that follows has the clock it drove.
  const pinAt = await page.evaluate(() => {
    const m = window.__RICHOS_MOCK__;
    const s = m && typeof m.activeMemoryStrategy === "function" ? m.activeMemoryStrategy() : null;
    return s && typeof s.startedAt === "number" ? s.startedAt + 18600 : null;
  });
  // AND THE BAND'S "Searched · Ns ago" IS A SECOND CLOCK THE PIN DOES NOT REACH. `whatAt` is
  // stamped with the REAL `Date.now()` when the signal arrives (`main.js:3363`, `noteTurnSignal`),
  // and the snapshot path only replaces it with the fixture's instant when that is newer than
  // `lastAt` (`main.js:3781`), which it is not. The pin sits at `startedAt + 18,600 ms`, about 6s
  // after the send, so the age shown is `6s - (real time the walk took to reach the signal)`:
  // "6s ago" on a quick run, nothing at all (age under 1s -> no " ago") on a slow or sharded one.
  // Nightly 70's build (run 20261021T003950Z-87713b77) was refused on exactly that: 337 pixels
  // in the 58x16 box at x655,y821 of `ms-03`, in the sharded run and again in its retry. The
  // band's own signal instant is put where each committed reference shows it: `ms-03` is the
  // one picture whose reference reads "Searched · 6s ago" (the snapshot path had just stamped
  // it, `main.js:3784`), so it gets 6,000 ms before the pin; the others read no age, so 0. The
  // recompute below goes through the product's own visibility handler like every other timer.
  if (pinAt !== null) {
    const ageMs = name === "ms-03-three-workers-active" ? 6000 : 0;
    await page.evaluate(
      ([at, age]) => {
        if (typeof waitTurn !== "undefined" && waitTurn && waitTurn.lastWhat) waitTurn.whatAt = at - age;
      },
      [pinAt, ageMs]
    );
    await pinClock(page, pinAt);
  }
  let s;
  try {
    s = await shot(page, name); // throws unless the pixels are a real render
  } finally {
    if (pinAt !== null) await unpinClock(page);
  }
  fs.mkdirSync(SHOTS_26, { recursive: true });
  publishShotFile(s.file, path.join(SHOTS_26, name + ".png"));
  shots.push({ name, note, distinct: s.distinct, bytes: s.bytes });
  return `${name}.png — ${s.width}x${s.height}, ${s.distinct} distinct colors, ${s.bytes} bytes`;
}

async function openApp(browser, viewport) {
  const page = await browser.newPage({ viewport: viewport || { width: 1440, height: 960 } });
  const errors = [];
  page.on("pageerror", (e) => errors.push(String(e)));
  page.on("console", (m) => {
    if (m.type() === "error") errors.push("console: " + m.text());
  });
  // THIS SUITE STATES ITS LIGHTING (CEO §63, 2026-09-19). The shipped default preference
  // used to be `dark`, so every walk in here rendered dark without anyone saying so, and the
  // committed reference PNGs below were taken that way. §63 makes the default `system`,
  // which hands the palette to the host's `colorScheme` — light, in this harness — and
  // flipped those references in a single run with every check still green, because nothing
  // this suite asserts is about lighting. The shots are pictures a person LOOKS at, so the
  // palette is named here rather than inherited from a default that is free to move again.
  // `SEED_THEME` writes both the mirror and the store and says why; the store is the one
  // that decides.
  await page.addInitScript(SEED_THEME, "dark");
  // The app's frame never scrolls: a scroll of the document, `#app` or a pane is a page error,
  // which each pass's "no page errors" check refuses (harness.js FRAME_STILL).
  await page.addInitScript(FRAME_STILL);
  await page.goto(APP);
  // The home screen is the landing surface now; this suite is about the app UI behind it.
  await leaveHome(page);
  await page.waitForFunction("typeof window.RichTimeline === 'object'");
  await page.waitForSelector(".nav-thread", { state: "attached" });
  page.__errors = errors;
  return page;
}

/// Open §10.1's thread, set the injected clock, and send the CEO's prompt through the
/// composer — the real path, including the composer clearing only after acceptance.
async function sendTheBrief(page, anchorBackMs) {
  await page.click('.nav-thread[data-thread-id="memory"]');
  await page.waitForSelector("#composer, #input, textarea", { state: "attached" });
  const prompt = await page.evaluate((back) => {
    window.__RICHOS_MOCK__.setMemoryStrategyAnchor(Date.now() - back);
    return window.__RICHOS_MOCK__.MEMORY_STRATEGY_PROMPT;
  }, anchorBackMs);
  const box = await page.$("#input");
  await box.click();
  await box.fill(prompt);
  await page.keyboard.press("Enter");
  await page.waitForSelector(".tl-user-bubble");
  await settle(page);
  return prompt;
}

/// The shell coalesces renders into an animation frame (§15: "at most once per animation
/// frame"), so a test that reads the DOM in the same tick it emitted an event reads the
/// PREVIOUS frame. Two frames is one full scheduled render plus its flush.
async function settle(page) {
  await page.evaluate(
    () => new Promise((r) => requestAnimationFrame(() => requestAnimationFrame(r)))
  );
}

/// Walk the scenario to step `n`. Instant: the driver never waits on a clock.
async function runTo(page, n) {
  const applied = await page.evaluate((step) => {
    const s = window.__RICHOS_MOCK__.activeMemoryStrategy();
    if (!s) throw new Error("no scenario running — the send did not start one");
    return s.runTo(step).map((r) => `${r.n}. ${r.spec} [${r.status}] :: ${r.did}`);
  }, n);
  await settle(page);
  return applied;
}

/// §10.8 — the CEO leaves the thread and comes back. This is also the ONLY route worker
/// rows have to the screen today: `rich://worker-upserted` is deferred in the emitter
/// (live.rs:26) and unwired in `main.js`, so workers arrive on the `get_timeline` snapshot.
async function leaveAndReturn(page) {
  await page.click('.nav-thread[data-thread-id="acme"]');
  await page.waitForFunction("document.querySelectorAll('.tl-turn').length > 0");
  await page.click('.nav-thread[data-thread-id="memory"]');
  await page.waitForSelector('.tl-turn[data-turn-id="turn_memory_01"]');
  await settle(page);
}

async function setTranscript(page, open) {
  await page.evaluate((wantOpen) => {
    const btn = document.querySelector('.tl-turn[data-turn-id="turn_memory_01"] .tl-duration-btn');
    if (!btn || !btn.classList) return;
    const isOpen = btn.getAttribute("aria-expanded") === "true";
    if (isOpen !== wantOpen) btn.click();
  }, open);
  await settle(page);
}

async function expandLiveTranscript(page) {
  await setTranscript(page, true);
}

async function expandTranscript(page) {
  await setTranscript(page, true);
  await page.waitForSelector(".tl-chip", { state: "attached" });
}

async function readTurn(page) {
  return page.evaluate(() => {
    const sec = document.querySelector('.tl-turn[data-turn-id="turn_memory_01"]');
    if (!sec) return null;
    const q = (s) => Array.from(sec.querySelectorAll(s));
    return {
      duration: sec.querySelector(".tl-duration-label")
        ? sec.querySelector(".tl-duration-label").textContent
        : (sec.querySelector(".tl-duration-btn") || {}).textContent,
      durationExpanded: (sec.querySelector(".tl-duration-btn") || {}).getAttribute
        ? sec.querySelector(".tl-duration-btn").getAttribute("aria-expanded")
        : null,
      activityRows: q(".tl-activity-text").map((n) => n.textContent),
      chips: q(".tl-chip").map((c) => ({
        id: c.id,
        name: c.querySelector(".tl-chip-name").textContent,
        role: c.querySelector(".tl-chip-role") ? c.querySelector(".tl-chip-role").textContent : null,
        state: c.querySelector(".tl-chip-state").textContent,
        qualifier: c.querySelector(".tl-chip-qualifier") ? c.querySelector(".tl-chip-qualifier").textContent : null,
      })),
      workerGroups: q(".tl-workers").length,
      prose: q(".tl-rich").map((n) => n.innerText),
      userText: (sec.querySelector(".tl-user-text") || {}).textContent || "",
      text: sec.innerText,
      // EVERYTHING A READER SEES IN THIS SECTION, including the numbers a list DRAWS rather
      // than stores. Rich's five questions arrive as Markdown (`1. …` on five lines) and
      // §5.4's renderer now builds them as a real `<ol>`, so the numerals are CSS markers —
      // on screen, announced by a screen reader as list positions, and invisible to
      // `innerText`, which can only see text nodes. The marker is reconstructed from the
      // item's position in its own list, so a check can still name a question by its number.
      // That is a stronger statement than the substring it replaced: it asserts the question
      // is the Nth item of an ordered list, not merely that a digit precedes it.
      visibleText: [
        sec.innerText,
        ...q("ol.tl-md-list > li.tl-md-item").map(
          (n) => Array.prototype.indexOf.call(n.parentNode.children, n) + 1 + ". " + n.textContent
        ),
      ].join("\n"),
    };
  });
}

/// The projected snapshot, straight off the mock bridge — the same bytes `get_timeline`
/// hands `applySnapshot`.
async function snapshot(page) {
  return page.evaluate(() => window.RichBridge.invoke("get_timeline", { threadId: "memory" }));
}

async function main() {
  const { webkit } = loadPlaywright();
  const browser = await webkit.launch();
  const run = createRun("§26 memory-strategy — real shell, real renderer, WebKit");

  // =====================================================================================
  // PASS A — the first second. §10.2's scaffold, before any duration exists.
  // =====================================================================================
  const a = await openApp(browser);

  await run.check("§26.1-2 the CEO's prompt lands and the turn is accepted", async () => {
    const prompt = await sendTheBrief(a, ANCHOR_SUB_SECOND);
    const t = await readTurn(a);
    assert(t, "the turn section rendered");
    assert(t.userText.includes("design a proper replacement for Claude auto memory"), "the prompt is the CEO's");
    assert(t.userText.includes("https://docs.anthropic.com/en/docs/claude-code/memory"), "URL 1 present");
    assert(t.userText.includes("https://code.claude.com/docs/en/memory"), "URL 2 present");
    assert(t.userText.includes("/Users/you/Projects/harbor/docs/plans/"), "a file path present");
    const composer = await a.evaluate(() => document.getElementById("input").value);
    assertEqual(composer, "", "§10.2: the composer clears after the message is accepted");
    // The bubble is the SHELL's optimistic one, re-keyed onto the real turn id.
    const id = await a.evaluate(() =>
      (document.querySelector(".tl-user-bubble").closest("[data-turn-id]") || {}).dataset.turnId
    );
    assertEqual(id, "turn_memory_01", "the optimistic bubble was adopted onto the real turn");
    return `${prompt.split("\n").length}-line prompt, composer cleared, adopted onto turn_memory_01`;
  });

  await run.check("§6.1 under one second the row reads `Working` — no number invented", async () => {
    const t = await readTurn(a);
    assertEqual(t.duration.trim(), "Working", "§6.1's first row: accepted, under one second");
    return `"${t.duration.trim()}" at ~${ANCHOR_SUB_SECOND - 600}ms of measured elapsed time`;
  });

  await run.check("§5.1 the long prompt clamps with an accessible Show more", async () => {
    const before = await a.evaluate(() => ({
      clamped: document.querySelector(".tl-user-bubble").classList.contains("is-clamped"),
      label: document.querySelector(".tl-more").textContent,
      expanded: document.querySelector(".tl-more").getAttribute("aria-expanded"),
    }));
    await a.click(".tl-more");
    await settle(a);
    const after = await a.evaluate(() => ({
      clamped: document.querySelector(".tl-user-bubble").classList.contains("is-clamped"),
      label: document.querySelector(".tl-more").textContent,
      expanded: document.querySelector(".tl-more").getAttribute("aria-expanded"),
    }));
    await a.click(".tl-more");
    await settle(a);
    const back = await a.evaluate(() => document.querySelector(".tl-user-bubble").classList.contains("is-clamped"));
    assert(before.clamped, "a 22-line prompt is over §5.1's 18-line clamp and starts clamped");
    assertEqual(before.label, "Show more");
    assertEqual(before.expanded, "false");
    assert(!after.clamped, "Show more expands it");
    assertEqual(after.label, "Show less");
    assertEqual(after.expanded, "true");
    assert(back, "Show less re-clamps it");
    return "clamped -> Show more -> Show less -> clamped, aria-expanded tracked both ways";
  });

  await run.check("SCREENSHOT 1/9: just after send", async () => {
    // A young turn has no absence caption. Keep the reference aligned with
    // this product state, independently of screenshot publication behavior.
    assertEqual(await a.locator(".wait-detail").textContent(), "", "young turn detail");
    return evidence(a, "ms-01-just-after-send", "§26 shot 1");
  });

  await a.close();

  // =====================================================================================
  // PASS B — the whole turn. Anchored 18.6s in the past so the live row reads 18s.
  // =====================================================================================
  const b = await openApp(browser);
  await sendTheBrief(b, ANCHOR_18S);

  await run.check(
    "§6.4 a LIVE turn's work transcript is expanded BY DEFAULT — and the CEO still wins",
    async () => {
      await runTo(b, 5);
      const live = await b.evaluate(() => {
        const sec = document.querySelector('.tl-turn[data-turn-id="turn_memory_01"]');
        return {
          expanded: sec.querySelector(".tl-duration-btn").getAttribute("aria-expanded"),
          activityRows: sec.querySelectorAll(".tl-activity-text").length,
        };
      });
      // §6.4 opens: "While active, the work activity beneath the duration row is expanded by
      // default." Until 2026-08-29 nothing put a live turn into `model.expanded` — that set
      // was written by the CEO's own toggle alone — so a running turn started closed and the
      // CEO watched a chevron instead of the work. `RichTimeline.isTurnExpanded` now carries
      // §6.4's two defaults, and the CEO's explicit choice overrules both.
      assertEqual(live.expanded, "true", "§6.4: while active, expanded by default");
      assert(live.activityRows > 0, "and the work is on screen without a click: " + live.activityRows);

      // THE CEO'S OWN COLLAPSE, MID-TURN, IS NOT OVERRULED. This is the half a naive fix
      // breaks: re-deriving "expanded" from liveness on every render re-opens what he just
      // closed, on the very next event.
      await setTranscript(b, false);
      const closed = await b.evaluate(() => {
        const sec = document.querySelector('.tl-turn[data-turn-id="turn_memory_01"]');
        return sec.querySelector(".tl-duration-btn").getAttribute("aria-expanded");
      });
      assertEqual(closed, "false", "the CEO closed it while the turn is still running");
      await runTo(b, 5);
      const stillClosed = await b.evaluate(() => {
        const sec = document.querySelector('.tl-turn[data-turn-id="turn_memory_01"]');
        return sec.querySelector(".tl-duration-btn").getAttribute("aria-expanded");
      });
      assertEqual(stillClosed, "false", "and further live events do not re-open it over him");

      // Back to the default state for the rest of this pass.
      await setTranscript(b, true);
      return (
        "live turn: aria-expanded=true with " + live.activityRows + " work rows, no click.\n" +
        "          CEO collapse mid-turn holds across further live events."
      );
    }
  );

  await run.check("§26.3-5 commentary, seven reads rolled into one summary, one search", async () => {
    const applied = await runTo(b, 5);
    await expandLiveTranscript(b);
    const t = await readTurn(b);
    assert(t.prose.length >= 1, "Rich's commentary is on screen");
    assert(t.prose[0].includes("reading the full source set"), "the commentary is his, verbatim");
    assert(t.activityRows.includes("Read 7 files"), "§26.4 seven reads, ONE summary: " + t.activityRows.join(" | "));
    assert(t.activityRows.includes("Searched"), "§26.5 one web search");
    assert(
      !t.activityRows.some((r) => /Read a file/.test(r)),
      "the seven individual rows are rolled up, not listed: " + t.activityRows.join(" | ")
    );
    return applied.join("\n          ");
  });

  await run.check("§26 the clock reads exactly `Working for 18s` — injected, not waited for", async () => {
    // READ AT THE INSTANT THE FIXTURE DEFINES, NOT AT WHATEVER INSTANT THE HARNESS ARRIVES
    // (2026-09-29, audit R2). The brief was sent with the anchor `Date.now() - 18,600` and the
    // mock starts the turn 600ms after the anchor, so at the send the turn is exactly 18,000ms
    // old. This used to read the LIVE row two checks and a few dozen round trips later and
    // require the real elapsed time to still be inside [18,000, 19,000): measured 301 and
    // 324ms of that second spent on a quiet Mac, and on a busy nightly host the row reads
    // `Working for 19s` for a product that is right. So the page's wall clock is pinned to the
    // send instant (anchor + 18,600ms) and the row is recomputed through the product's own
    // visibility handler (`pinClock`, `lib/harness.js`), exactly as every §26 shot is taken.
    // What this still proves: the fixture's anchor reached the product's `startedAt`, and the
    // product's own formatter turns 18,000ms of elapsed time into `Working for 18s`. What it no
    // longer does is race the harness.
    const m = await b.evaluate(() => {
      const s = window.__RICHOS_MOCK__.activeMemoryStrategy();
      return { startedAt: s.startedAt, anchor: s.anchor };
    });
    assertEqual(m.startedAt - m.anchor, 600, "the mock starts the turn 600ms after the anchor it was given");
    const sentAt = m.anchor + ANCHOR_18S;
    const pinned = await pinClock(b, sentAt);
    assertEqual(pinned, sentAt, "the page's clock could not be pinned to the send instant");
    let t;
    try {
      t = await readTurn(b);
    } finally {
      await unpinClock(b);
    }
    assertEqual(t.duration.trim(), "Working for 18s", "the anchored elapsed time, read at the send instant");
    return (
      `startedAt = anchor + 600; at the send instant (anchor + ${ANCHOR_18S}) Date.now() - startedAt = ` +
      `${sentAt - m.startedAt}ms -> "Working for 18s". No sleep; the clock is pinned to the fixture's instant, not to the harness's.`
    );
  });

  await run.check("SCREENSHOT 2/9: active at `Working for 18s`", async () =>
    evidence(b, "ms-02-working-for-18s", "§26 shot 2")
  );

  await run.check("§26.6 three workers start — LIVE, and the reload says the same thing", async () => {
    await runTo(b, 6);
    // MEASURED THE WAY THE GAP WAS MEASURED: read the chips with no snapshot in between.
    // This is the assertion that was `0` before `rich://worker-upserted` stopped being
    // deferred — a delegation reached the screen only after a `get_timeline` read, and
    // showed as a nameless "Worked" row for the rest of the turn.
    const live = await readTurn(b);
    const liveChips = live.chips.map((c) => `${c.name}/${c.role}/${c.state}`);
    assertEqual(
      liveChips,
      ["Sage/architecture/Working", "Frank/red team/Working", "Clark/research/Working"],
      "three chips, DURING the turn, with no snapshot read"
    );
    assertEqual(live.workerGroups, 1, "§7.1: one group, not three identical rows");
    assert(
      live.text.includes("Sage, Frank and Clark started working"),
      "§7.1's grouped summary, verbatim, live: " + live.text.slice(0, 400)
    );

    // AND THE TWO PATHS AGREE. A row that changed when the snapshot arrived would be a new
    // defect, not a fix — so the same three chips are re-read after leaving the thread and
    // coming back, which forces a real `get_timeline`.
    await leaveAndReturn(b);
    await expandTranscript(b);
    const t = await readTurn(b);
    assertEqual(
      t.chips.map((c) => `${c.name}/${c.role}/${c.state}`),
      liveChips,
      "the reloaded rows must be the SAME rows: same worker, same role, same state"
    );
    assertEqual(t.chips.map((c) => c.id), live.chips.map((c) => c.id), "and the same ids — an upsert, not a duplicate");
    return "live: " + liveChips.join(", ") + "\n          after a get_timeline read: identical, same ids";
  });

  await run.check("SCREENSHOT 3/9: three workers active", async () =>
    evidence(b, "ms-03-three-workers-active", "§26 shot 3")
  );

  await run.check("§26.7-9 states update INDEPENDENTLY and in place — one run, one row", async () => {
    await runTo(b, 9);
    await leaveAndReturn(b);
    await expandTranscript(b);
    const t = await readTurn(b);
    assertEqual(
      t.chips.map((c) => `${c.name}/${c.state}`),
      ["Sage/Working", "Frank/Ended", "Clark/Working"],
      "Clark updated, Frank's run ended, Sage updated — three different states, one group"
    );
    assertEqual(t.chips.length, 3, "still three rows: a run is ONE row however many events it produced");
    assertEqual(t.workerGroups, 1);
    const frank = t.chips.find((c) => c.name === "Frank");
    assertEqual(frank.qualifier, "outcome not recorded", "§26.8: `Ended`, never `finished`");
    return t.chips.map((c) => `${c.name}=${c.state}${c.qualifier ? " (" + c.qualifier + ")" : ""}`).join(", ");
  });

  await run.check("§26.10-12 the run ends, Rich says only what was witnessed, a second run opens", async () => {
    await runTo(b, 12);
    await leaveAndReturn(b);
    await expandTranscript(b);
    const t = await readTurn(b);
    const sage1 = t.chips.find((c) => c.id === "chip:agt_ms_sage_1");
    assertEqual(sage1.state, "Ended");
    assertEqual(sage1.qualifier, "outcome not recorded");
    assert(t.chips.some((c) => c.id === "chip:agt_ms_sage_2" && c.state === "Working"), "a second Sage run is open");
    assertEqual(t.workerGroups, 2, "§10.6: the replacement opens its OWN group, below the commentary");
    const recovery = t.prose.find((p) => p.includes("Sage's run ended"));
    assert(recovery, "Rich's recovery commentary is in the transcript");
    // THE SUBTLE HALF. Prose is the one channel that accepts any string, so the failure
    // must not be faked through Rich's mouth either.
    for (const word of ["failed", "failure", "crashed", "died", "error"]) {
      assert(
        !new RegExp("\\b" + word + "\\b", "i").test(recovery),
        `Rich claimed "${word}" about a run whose outcome nothing recorded: ${recovery}`
      );
    }
    assert(recovery.includes("nothing recorded how"), "he says what was actually witnessed");
    return `2 groups; sage_1=Ended(outcome not recorded), sage_2=Working; recovery prose carries no failure claim`;
  });

  await run.check("SCREENSHOT 4/9: the ended run with Rich's recovery commentary", async () =>
    evidence(
      b,
      "ms-04-run-ended-with-recovery-commentary",
      "§26 shot 4 asks for `Sage FAILED with Rich recovery commentary`. The failure half does " +
        "not exist in this runtime, so this is the honest analogue: the run ended, the outcome " +
        "was not recorded, and Rich's commentary says exactly that."
    )
  );

  await run.check("§7.2 worker detail opens BESIDE the thread, both panes readable", async () => {
    await b.click('[id="chip:agt_ms_sage_1"]');
    await b.waitForSelector("#inspector:not([hidden])");
    const r = await b.evaluate(() => {
      const insp = document.getElementById("inspector");
      const conv = document.getElementById("conversation");
      const ir = insp.getBoundingClientRect();
      const cr = conv.getBoundingClientRect();
      return {
        title: document.getElementById("inspector-title").textContent,
        modal: insp.getAttribute("aria-modal"),
        overlaps: ir.left < cr.right - 1 && cr.left < ir.right - 1,
        conversationWidth: Math.round(cr.width),
        inspectorWidth: Math.round(ir.width),
        body: document.getElementById("inspector-body").innerText,
      };
    });
    assertEqual(r.title, "Sage");
    assertEqual(r.modal, null, "§7.2: a sibling pane, not a modal");
    assert(!r.overlaps, "beside, not over");
    assert(r.body.includes("Ended"), "the state it actually has");
    // §26's fifth shot wants "its file-change card visible". §7.2 item 6 has no source
    // (timeline.js:1177-1201) — Phase 5 owns artifacts — so the card is ABSENT, and that
    // absence is asserted rather than papered over with an invented diff.
    for (const fake of ["+234", "-28", "Undo", "Review", "Edited 4 files"]) {
      assert(!r.body.includes(fake), "a file-change card was invented: " + fake);
    }
    return `conversation ${r.conversationWidth}px | inspector ${r.inspectorWidth}px, side by side, no invented diff`;
  });

  await run.check("SCREENSHOT 5/9: worker detail beside the main thread", async () =>
    evidence(
      b,
      "ms-05-worker-detail-beside-thread",
      "§26 shot 5 asks for the file-change card too. §7.2 item 6 has no source — Phase 5 owns " +
        "artifacts and changed files — so the pane shows the three things that are sourced and " +
        "the card is absent, asserted absent above."
    )
  );

  await run.check("§26.13 NO PLAN IS DRAWN, at either step count", async () => {
    const snap = await snapshot(b);
    const kinds = Array.from(new Set(snap.items.map((i) => i.kind)));
    assert(!kinds.includes("plan"), "no plan item on the wire: " + kinds.join(", "));
    const t = await readTurn(b);
    for (const s of ["Step 2 / 5", "Step 3 / 5", "Step 2 of 5", "Step 3 of 5", "tasks completed"]) {
      assert(!t.text.includes(s), "a plan was rendered anyway: " + s);
    }
    const planNodes = await b.evaluate(() => document.querySelectorAll(".tl-plan, [data-kind='plan']").length);
    assertEqual(planNodes, 0);
    return "kinds on the wire: " + kinds.join(", ") + " — no `plan`, and no plan copy in the DOM";
  });

  await run.check("§26.14-16 the turn completes at a MEASURED 2h 17m 50s", async () => {
    const applied = await runTo(b, 16);
    // `rich://turn-completed` triggers the shell's own reconciliation reload (main.js).
    await b.waitForFunction(
      "!!document.querySelector('.tl-turn[data-turn-id=\"turn_memory_01\"] .tl-duration-btn')"
    );
    const m = await b.evaluate(() => {
      const snap = null;
      const s = window.__RICHOS_MOCK__.activeMemoryStrategy();
      return { startedAt: s.startedAt, activeMs: s.activeMs, snap };
    });
    const snap = await snapshot(b);
    const dur = snap.items.find((i) => i.kind === "work_duration");
    assertEqual(dur.state, "completed");
    assertEqual(dur.endedAt - dur.startedAt, 8270000, "MEASURED span, endedAt - startedAt");
    assertEqual(dur.activeMs, 8270000, "and that is what the projection reports");
    const t = await readTurn(b);
    assert(t.duration.includes("Worked for 2h 17m 50s"), "the frozen row: " + t.duration);
    assertEqual(m.activeMs, 8270000);
    return (
      applied.join("\n          ") +
      `\n          (2 x 3600) + (17 x 60) + 50 = 7200 + 1020 + 50 = 8270 s = 8270000 ms; ` +
      `endedAt - startedAt = ${dur.endedAt - dur.startedAt} ms`
    );
  });

  await run.check("§6.4 the transcript collapses, the five questions stay visible", async () => {
    // This page's CEO opened the transcript himself several steps ago, and §6.4's settle
    // deliberately does not fight him (main.js:1176-1179: "Only if the CEO has not opened it
    // himself in the meantime"). The AUTOMATIC collapse is proved on its own page below,
    // with the disclosure never touched. Here the collapse is the CEO's.
    await setTranscript(b, false);
    const t = await readTurn(b);
    assertEqual(t.chips.length, 0, "worker rows collapsed under the duration row");
    assertEqual(t.activityRows.filter((r) => r === "Read 7 files").length, 0, "activity collapsed");
    assert(t.visibleText.includes("1. What is the first deliverable"), "the final response stays expanded");
    assert(t.visibleText.includes("5. On cutover day"), "all five questions, below the divider");
    const order = await b.evaluate(() => {
      const sec = document.querySelector('.tl-turn[data-turn-id="turn_memory_01"]');
      const row = sec.querySelector(".tl-duration");
      const last = Array.from(sec.querySelectorAll(".tl-rich")).pop();
      return row.compareDocumentPosition(last) & Node.DOCUMENT_POSITION_FOLLOWING ? "below" : "above";
    });
    assertEqual(order, "below", "§25: the final response appears below the completed-duration divider");
    return "collapsed to the duration row; five questions still on screen, below the divider";
  });

  await run.check("SCREENSHOT 6/9: final response with collapsed `Worked for 2h 17m 50s`", async () =>
    evidence(b, "ms-06-final-collapsed", "§26 shot 6")
  );

  await run.check("§6.4 expanding restores the original chronology in place", async () => {
    await expandTranscript(b);
    const t = await readTurn(b);
    assertEqual(t.chips.length, 4, "all four worker runs are back");
    assert(t.activityRows.includes("Read 7 files"), "and the activity");
    assert(t.prose[0].includes("reading the full source set"), "commentary in its ORIGINAL order, first");
    assert(t.visibleText.includes("5. On cutover day"), "the final response never left");
    return `4 chips, ${t.activityRows.length} activity rows, ${t.prose.length} prose runs, in order`;
  });

  await run.check("SCREENSHOT 7/9: expanded completed activity", async () =>
    evidence(b, "ms-07-expanded-completed-activity", "§26 shot 7")
  );

  await run.check("§25 both panes resize by keyboard and the widths persist", async () => {
    const before = await b.evaluate(() => ({
      rail: document.getElementById("rail").getBoundingClientRect().width,
      insp: document.getElementById("inspector").getBoundingClientRect().width,
    }));
    await b.focus("#rail-resizer");
    for (let i = 0; i < 12; i++) await b.keyboard.press("ArrowRight");
    await b.focus("#inspector-resizer");
    for (let i = 0; i < 12; i++) await b.keyboard.press("ArrowLeft");
    // `commitRailWidth` debounces its write by 150ms (main.js:2067-2073) while
    // `persistInspectorWidth` writes straight through — so the store is read after the
    // debounce has had time to land rather than racing it.
    await b.waitForTimeout(500);
    const after = await b.evaluate(async () => ({
      rail: document.getElementById("rail").getBoundingClientRect().width,
      insp: document.getElementById("inspector").getBoundingClientRect().width,
      stored: await window.RichBridge.invoke("nav_state"),
    }));
    assert(after.rail > before.rail, `rail ${before.rail} -> ${after.rail}`);
    assert(after.insp > before.insp, `worker pane ${before.insp} -> ${after.insp}`);
    assertEqual(
      [after.stored.sidebar_width, after.stored.inspector_width],
      [Math.round(after.rail), Math.round(after.insp)],
      `the widths the store accepted must be the widths on screen ` +
        `(rail ${Math.round(after.rail)}px, worker pane ${Math.round(after.insp)}px)`
    );
    return `rail ${Math.round(before.rail)} -> ${Math.round(after.rail)}px, worker pane ${Math.round(
      before.insp
    )} -> ${Math.round(after.insp)}px, both persisted (nav_state agrees)`;
  });

  await run.check("SCREENSHOT 8/9: resized left navigation and resized worker pane", async () => {
    // WHERE THE CONVERSATION SITS AFTER THE RESIZE WAS DECIDED BY TIMING, NOT BY THE APP'S STATE,
    // and this shot took whichever position the run happened to land on. Measured 2026-10-05 on
    // 3cc31f313 with a per-key probe: the thread is at its bottom after shot 7 (scrollTop 941,
    // scrollHeight 1730, clientHeight 789), the 24 key presses narrow the column so the text
    // rewraps (scrollHeight 1730 -> 2058), and WebKit moves scrollTop by one 29px line at some
    // of the intermediate widths it lays out (941 -> 970 -> 999) with no app code writing it.
    // Which widths get a layout depends on how the presses fall into frames, and the shell's
    // 3s `pollWorkerStatus` render (main.js) re-pins to the bottom only while `followBottom`
    // still holds. Three runs of this one commit gave three positions 29px apart: the committed
    // reference, the per-key probe (+29px) and nightly run 20261005T094008Z (+58px). The
    // pointer, left where shot 5 clicked Sage's chip, then sits over whichever message moved
    // under it, and `.tl-rich:hover` reveals that message's `Copy` (style.css `.tl-rich-actions`),
    // which is the "Copy" under "…nothing resets." in the refused picture.
    //
    // So the shot is taken from a stated state: the conversation scrolled to its end (the
    // scroll the CEO does himself; the shell's own scroll listener sets `followBottom` from it,
    // so the 3s render keeps it there), and the pointer resting on the empty lower half of the
    // worker pane, with every message's hover row faded out before the picture.
    await b.evaluate(() => {
      const c = document.getElementById("conversation");
      c.scrollTop = c.scrollHeight;
    });
    await b.mouse.move(1220, 700);
    await b.waitForFunction(() => {
      const c = document.getElementById("conversation");
      const atEnd = c.scrollHeight - c.scrollTop - c.clientHeight <= 1;
      const rows = Array.from(document.querySelectorAll(".tl-rich-actions, .tl-user-actions"));
      return atEnd && rows.every((r) => getComputedStyle(r).opacity === "0");
    });
    await settle(b);
    return evidence(b, "ms-08-resized-panes", "§26 shot 8");
  });

  // The snapshot as it stands, for the reload comparison below.
  const snapBefore = await snapshot(b);

  await run.check("§14/§26 the same thread, restored after an app reload", async () => {
    // A real reload of the shell. `mock.js` has no ledger on disk, so the durable record is
    // re-derived from the SAME anchor — which is exactly the claim a deterministic fixture
    // makes, and it is checked below by comparing the two snapshots byte for byte. The
    // renderer then rebuilds from `get_timeline` alone: ZERO live events reach this page.
    const anchor = await b.evaluate(() => window.__RICHOS_MOCK__.activeMemoryStrategy().anchor);
    await b.goto(APP);
    await leaveHome(b);
    await b.waitForFunction("typeof window.RichTimeline === 'object'");
    await b.waitForSelector(".nav-thread", { state: "attached" });
    await b.evaluate((a) => window.__RICHOS_MOCK__.memoryStrategy({ anchor: a }).runTo(16), anchor);
    await b.click('.nav-thread[data-thread-id="memory"]');
    await b.waitForSelector('.tl-turn[data-turn-id="turn_memory_01"]');
    await settle(b);
    const t = await readTurn(b);
    assert(t.duration.includes("Worked for 2h 17m 50s"), "the frozen duration survived: " + t.duration);
    assert(t.visibleText.includes("1. What is the first deliverable"), "the final response is expanded on arrival");
    assert(t.userText.includes("design a proper replacement"), "the CEO's prompt is back");
    await expandTranscript(b);
    const e = await readTurn(b);
    assertEqual(e.chips.length, 4, "and the full work transcript reopens");
    return `restored from get_timeline alone: "${t.duration.trim()}", 4 worker runs under it`;
  });

  await run.check("SCREENSHOT 9/9: same thread restored after app reload", async () =>
    evidence(b, "ms-09-restored-after-reload", "§26 shot 9")
  );

  await run.check("DETERMINISM: the same anchor twice produces identical bytes", async () => {
    const again = await snapshot(b);
    assertEqual(again, snapBefore, "the re-derived snapshot differs from the first run");
    // And a THIRD construction in a fresh page, compared as JSON text.
    const third = await b.evaluate((a) => {
      window.__RICHOS_MOCK__.memoryStrategy({ anchor: a }).runTo(16);
      return window.RichBridge.invoke("get_timeline", { threadId: "memory" });
    }, snapBefore.items[0].createdAt);
    assertEqual(JSON.stringify(third), JSON.stringify(snapBefore), "a third run differs");
    return `${snapBefore.items.length} items, identical across three constructions from one anchor`;
  });

  // =====================================================================================
  // PASS C — §6.4 point 3, with the disclosure NEVER touched.
  // =====================================================================================
  const c = await openApp(browser);
  await sendTheBrief(c, ANCHOR_18S);

  await run.check("§6.4 completion collapses the work transcript on its own", async () => {
    // THE TWO DEFAULTS, ON ONE PAGE, WITH THE DISCLOSURE NEVER TOUCHED. §6.4 asks for
    // expanded while active and collapsed after a settling transition, and the second must
    // not be defeated by the first: an implementation that simply derives "expanded" from
    // liveness would snap the transcript shut the instant the terminal status arrived,
    // leaving the 180ms settle with nothing to do and no transition to see.
    await runTo(c, 9);
    const midTurn = await c.evaluate(() => {
      const sec = document.querySelector('.tl-turn[data-turn-id="turn_memory_01"]');
      return {
        expanded: sec.querySelector(".tl-duration-btn").getAttribute("aria-expanded"),
        chips: sec.querySelectorAll(".tl-chip").length,
      };
    });
    assertEqual(midTurn.expanded, "true", "expanded by default while the turn is running");
    assertEqual(midTurn.chips, 3, "with the delegations visible, unclicked");

    await runTo(c, 16);
    await c.waitForFunction(
      () => {
        const btn = document.querySelector('.tl-turn[data-turn-id="turn_memory_01"] .tl-duration-btn');
        return btn && btn.getAttribute("aria-expanded") === "false" && /Worked for/.test(btn.textContent);
      }, null,
      { timeout: 5000 }
    );
    // Nothing in this page ever clicked the disclosure, so the 180ms settle owns the state.
    const t = await readTurn(c);
    assert(t.duration.includes("Worked for 2h 17m 50s"), "frozen: " + t.duration);
    assertEqual(t.chips.length, 0, "worker rows are inside the collapsed transcript");
    assert(t.visibleText.includes("1. What is the first deliverable"), "the final response stays out of it");
    assert(t.visibleText.includes("5. On cutover day"), "all five questions");
    return (
      "mid-turn: aria-expanded=true, 3 chips, never clicked -> after completion: collapsed " +
      "by the 180ms settle; five questions still on screen"
    );
  });

  await run.check("no page errors in the untouched pass", async () => {
    assertEqual(c.__errors, []);
    return "0 uncaught errors, 0 console errors";
  });

  await c.close();

  // =====================================================================================
  // PASS D — a reader at the bottom of the thread stays there through a resize. Its own page,
  // so nothing it resizes reaches pass B's shots 8 and 9.
  // =====================================================================================
  // Found while fixing shot 8 (a5f25ec59): narrowing a pane rewraps the thread (scrollHeight
  // 1730 -> 2058 over shot 8's 24 key presses) while scrollTop stays put or moves one 29px line,
  // so the reader who was at the bottom was left lines above it, with the jump-down button up.
  const d = await openApp(browser);

  await run.check("a reader at the bottom of the thread stays there through every pane and window resize", async () => {
    await sendTheBrief(d, ANCHOR_18S);
    await runTo(d, 16);
    // THE TURN HAS JUST ENDED, SO ITS TRANSCRIPT IS OPEN ONLY UNTIL §6.4's SETTLE CLOSES IT, 180ms
    // later (main.js, the `setTimeout` after a terminal status). `expandTranscript` reads "open"
    // during that window and clicks nothing, and the chip clicked next was then taken out from
    // under the pointer whenever the click landed after the settle: on the nightly of a1a26a615,
    // at 87-97% CPU, the inspector never opened and the whole suite ended with no evidence.
    // Measured on an idle host: a 120ms pause between the two was already enough to lose the
    // chip every time. So: wait for the settle's own collapse (pass C proves it happens with the
    // disclosure untouched), then open the transcript as the CEO does, which the settle never
    // overrules, and only then press the chip.
    await d.waitForFunction(
      () => {
        const btn = document.querySelector('.tl-turn[data-turn-id="turn_memory_01"] .tl-duration-btn');
        return btn && btn.getAttribute("aria-expanded") === "false" && /Worked for/.test(btn.textContent);
      },
      null,
      { timeout: 30000 }
    );
    await expandTranscript(d);
    await d.click('[id="chip:agt_ms_sage_1"]');
    await d.waitForSelector("#inspector:not([hidden])");
    const read = () =>
      d.evaluate(() => {
        const c = document.getElementById("conversation");
        return {
          off: Math.round(c.scrollHeight - c.scrollTop - c.clientHeight),
          height: c.scrollHeight,
          jump: !document.getElementById("jump-latest").hidden,
        };
      });
    // The reader scrolls to the end, as he would; the shell's own scroll listener takes it.
    await d.evaluate(() => {
      const c = document.getElementById("conversation");
      c.scrollTop = c.scrollHeight;
    });
    await settle(d);
    const start = await read();
    assert(start.off <= 1 && !start.jump, `the reader starts at the bottom: ${JSON.stringify(start)}`);
    const misses = [];
    let steps = 0, rewraps = 0, last = start.height;
    const step = async (what, act) => {
      await act();
      await settle(d);
      const s = await read();
      steps++;
      if (s.height !== last) rewraps++;
      last = s.height;
      if (s.off > 1 || s.jump) misses.push(`${what}: ${s.off}px above the bottom${s.jump ? ", jump-down button showing" : ""}`);
    };
    const width = (id) => d.evaluate((i) => Number(document.getElementById(i).getAttribute("aria-valuenow")), id);
    // Keyboard, both dividers, out and back by the same steps, inside their limits (rail 224-420
    // by 16, main.js applyRailWidth; worker pane 280-520 by 8, applyInspectorWidth).
    const rail0 = await width("rail-resizer"), insp0 = await width("inspector-resizer");
    const railKeys = Math.min(12, Math.floor((420 - rail0) / 16)), inspKeys = Math.min(12, Math.floor((520 - insp0) / 8));
    assert(railKeys >= 4 && inspKeys >= 4, `room to widen both panes: rail ${rail0}px, worker pane ${insp0}px`);
    await d.focus("#rail-resizer");
    for (let i = 0; i < railKeys; i++) await step(`rail key ${i + 1} wider`, () => d.keyboard.press("ArrowRight"));
    for (let i = 0; i < railKeys; i++) await step(`rail key ${i + 1} back`, () => d.keyboard.press("ArrowLeft"));
    await d.focus("#inspector-resizer");
    for (let i = 0; i < inspKeys; i++) await step(`worker pane key ${i + 1} wider`, () => d.keyboard.press("ArrowLeft"));
    for (let i = 0; i < inspKeys; i++) await step(`worker pane key ${i + 1} back`, () => d.keyboard.press("ArrowRight"));
    // The pointer: the rail's divider dragged out and back.
    const handle = await d.locator("#rail-resizer").boundingBox();
    const x0 = handle.x + handle.width / 2, y0 = handle.y + handle.height / 2;
    await d.mouse.move(x0, y0);
    await d.mouse.down();
    for (let i = 1; i <= 8; i++) await step(`rail dragged +${i * 10}px`, () => d.mouse.move(x0 + i * 10, y0));
    for (let i = 7; i >= 0; i--) await step(`rail dragged back to +${i * 10}px`, () => d.mouse.move(x0 + i * 10, y0));
    await d.mouse.up();
    // The window: narrower, shorter, and back.
    for (const [w, h] of [[1300, 960], [1300, 800], [1440, 960]]) await step(`window ${w}x${h}`, () => d.setViewportSize({ width: w, height: h }));
    assertEqual([await width("rail-resizer"), await width("inspector-resizer")], [rail0, insp0], "both panes are back at their widths");
    assert(rewraps >= 4, `the resizes rewrapped the thread (${rewraps} of ${steps} steps changed its height)`);
    assertEqual(misses.length, 0, misses.slice(0, 10).join("\n") + (misses.length > 10 ? `\n… ${misses.length} of ${steps} steps in all` : ""));
    return `${steps} resize steps (keyboard on both dividers, a pointer drag, the window), ${rewraps} rewrapped the thread; at the bottom after every one, jump-down button hidden`;
  });

  await run.check("no page errors in the resize pass", async () => {
    assertEqual(d.__errors, []);
    return "0 uncaught errors, 0 console errors";
  });

  await d.close();

  // =====================================================================================
  // THE MANIFEST — the DOM must agree with the table, in BOTH directions
  // =====================================================================================

  await run.check("every one of §26's sixteen steps is represented or declared", async () => {
    const steps = await b.evaluate(() => window.__RICHOS_MOCK__.MEMORY_STRATEGY_STEPS);
    assertEqual(steps.length, 16, "§26 lists sixteen steps");
    assertEqual(
      steps.map((s) => s.n),
      [1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14, 15, 16]
    );
    for (const s of steps) {
      assert(
        ["represented", "partial", "unrepresentable"].includes(s.status),
        `step ${s.n} has no status`
      );
      assert(s.source && /\.(rs|js):\d/.test(s.source), `step ${s.n} cites no file:line — "${s.source}"`);
      if (s.status !== "represented") assert(s.gap, `step ${s.n} is ${s.status} with no stated reason`);
    }
    const counts = steps.reduce((a, s) => ((a[s.status] = (a[s.status] || 0) + 1), a), {});
    return (
      `${counts.represented} represented, ${counts.partial} partial, ${counts.unrepresentable} unrepresentable\n          ` +
      steps
        .filter((s) => s.status !== "represented")
        .map((s) => `${s.n}. ${s.spec} [${s.status}] — ${s.source}`)
        .join("\n          ")
    );
  });

  await run.check("NEGATIVE CONTROL: no unwitnessable worker state reaches the wire or the DOM", async () => {
    const snap = await snapshot(b);
    const workers = snap.items.filter((i) => i.kind === "worker_activity");
    assertEqual(workers.length, 4, "four runs across the turn");
    const observed = Array.from(new Set(workers.map((w) => w.worker.observedState))).sort();
    for (const o of observed) {
      assert(
        ["created", "started", "updated", "run_ended"].includes(o),
        "an observed state the emitters cannot produce: " + o
      );
    }
    const states = Array.from(new Set(workers.map((w) => w.worker.state))).sort();
    for (const forbidden of ["failed", "completed", "interrupted", "waiting", "not_found"]) {
      assert(!states.includes(forbidden), "`" + forbidden + "` has no witness and is on the wire");
    }
    await expandTranscript(b);
    const t = await readTurn(b);
    for (const word of ["Failed", "Done", "Finished", "Completed", "Stopped", "Unavailable"]) {
      assert(
        !t.chips.some((c) => c.state === word),
        `a chip claims "${word}" — see MEMORY_STRATEGY_STEPS[9] and worker_events.rs:137`
      );
    }
    return `observed: ${observed.join(", ")} -> states: ${states.join(", ")}; chips: ${t.chips
      .map((c) => c.state)
      .join(", ")}`;
  });

  await run.check("SCREENSHOT INVENTORY", async () => {
    assertEqual(shots.length, 9, "§26 asks for nine");
    return shots.map((s) => `${s.name}.png (${s.distinct} colors) — ${s.note}`).join("\n          ");
  });

  await run.check("no page errors", async () => {
    assertEqual(b.__errors, []);
    return "0 uncaught errors, 0 console errors";
  });

  await b.close();
  await browser.close();
  return run.report();
}

main().then(
  (failed) => { process.exitCode = failed ? 1 : 0; },
  (e) => {
    console.error(e);
    process.exit(1);
  }
);

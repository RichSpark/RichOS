// **THE FAILURE CARD, WHEN THE MODEL API IS WHAT FAILED** — `open-items.md` row 3.30,
// rendered by the REAL renderer under WebKit, the engine Tauri ships on macOS.
//
// The row's second and fifth answers reach the CEO here or they reach him nowhere. Before
// this suite existed the card said two fixed sentences for every failure:
//
//     I hit a snag mid-thought and had to stop — say the word and I'll pick it back up.
//     Everything I'd already written above is saved.
//
// The second is true about the TEXT and false about the whole — the session's working
// context is gone, and on 2026-09-03 five agents died having written nothing at all. And
// neither sentence can tell a `529` from a `429`, which are the two ends of "waiting is a
// plan" and "waiting is a plan with a known end".
//
// **THE BYTES ARE THE BACKEND'S, NOT THIS FILE'S.** Every sentence asserted below is
// scraped out of `crates/richos-core/src/upstream.rs` at run time by
// `lib/state-strings.js`'s Rust bridge — the same scrape `affordances.js` builds its state
// inventory from. Nothing here types a sentence, so a reworded `ceo_message()` moves this
// suite with it instead of leaving it green over copy the product no longer says.
//
// Run: node outage.js   (or `npm test` for every suite in this directory)

"use strict";

const { loadPlaywright, openFixture, createRun, assert, assertEqual } = require("./lib/harness");
const { rustStrings, normalize } = require("./lib/state-strings");
const C = require("./lib/contrast");

const THREAD = "thr_outage";
const TURN = "turn_outage";

// ---------------------------------------------------------------------------------------
// THE SENTENCES, READ OUT OF THE RUST SOURCE
// ---------------------------------------------------------------------------------------

/// Find the one scraped `ceo_message` literal containing `needle`. Throws if there is not
/// exactly one — an ambiguous match means the source moved and this suite would otherwise
/// keep asserting against whichever it happened to pick.
function rustSentence(needle) {
  const hits = rustStrings()
    .map((s) => normalize(s.text))
    .filter((t) => t.includes(needle));
  const unique = Array.from(new Set(hits));
  assertEqual(
    unique.length,
    1,
    `expected exactly one Rust CEO sentence containing ${JSON.stringify(needle)}, found ${unique.length}`
  );
  return unique[0];
}

const OVERLOAD = rustSentence("Anthropic's servers are at capacity");
const QUOTA = rustSentence("Your Claude usage limit is used up");
const NOT_ON_DISK = rustSentence("Not on disk: everything the session had worked out");

// The two generic sentences, which must NOT appear when an outage is explained.
const GENERIC_BODY = "I hit a snag mid-thought and had to stop";
// The one label for the one action, after the 2026-09-17 nightly's D6.
const RETRY_LABEL = "Restore my message";
const GENERIC_NOTE = "Everything I'd already written above is saved.";

// ---------------------------------------------------------------------------------------
// FIXTURES
// ---------------------------------------------------------------------------------------

function snapshot(outage, opts) {
  const b = (id, extra) =>
    Object.assign(
      {
        id,
        entityId: "northwind",
        threadId: THREAD,
        turnId: TURN,
        bindingRevision: 1,
        createdAt: 1787949000000,
        sequence: null,
        slot: "stream",
        visibility: "ceo",
      },
      extra || {}
    );
  const items = [
    Object.assign(b(TURN + ":user", { slot: "opening", createdAt: 1787948000000 }), {
      kind: "user_message",
      text: "Check the Acme numbers again.",
      source: "text",
    }),
    Object.assign(b(TURN + ":text:0", { sequence: 0 }), {
      kind: "rich_message",
      phase: "unknown",
      text: "Pulling the comparables now.",
    }),
    Object.assign(b(TURN + ":duration", { slot: "terminal", createdAt: 1787948000000 }), {
      kind: "work_duration",
      state: "interrupted",
      startedAt: 1787948100000,
      endedAt: 1787948100000 + 461000,
      activeMs: 461000,
    }),
  ];
  // A turn a button or an agent started carries no words of his: core projects no user item.
  if (opts && opts.withoutHisWords) items.shift();
  if (outage) {
    items.push(Object.assign(b(TURN + ":upstream", { slot: "terminal", createdAt: 1787948561000 }), outage));
  }
  return { entityId: "northwind", threadId: THREAD, mode: "ceo", bindingRevision: 1, items };
}

/// A `529`, exactly as `richos-core` projects it.
function overloadItem(retryMessage) {
  return {
    kind: "upstream_outage",
    fault: "overloaded",
    clearsOnAKnownSchedule: false,
    ceoMessage: OVERLOAD,
    lossMessage: "On disk: what you asked for is saved. " + NOT_ON_DISK,
    retryMessage: retryMessage || undefined,
  };
}

/// A `429`.
function quotaItem() {
  return {
    kind: "upstream_outage",
    fault: "rate_limit",
    clearsOnAKnownSchedule: true,
    ceoMessage: QUOTA,
    lossMessage: "Nothing had been written to disk yet for this turn. " + NOT_ON_DISK,
  };
}

// The renderer fixture: the real `timeline.js`, the real `style.css`, and
// `window.__render(snapshot)` applying a `get_timeline` payload through the SAME
// `applySnapshot` path `main.js` uses on a reload. Nothing about the render is stubbed.


const cardText = (page) =>
  page.evaluate(() => {
    const card = document.querySelector(".tl-intervention");
    if (!card) return null;
    return {
      body: Array.from(card.querySelectorAll(".tl-intervention-body")).map((n) => n.textContent),
      notes: Array.from(card.querySelectorAll(".tl-intervention-note")).map((n) => n.textContent),
      action: card.querySelector(".tl-intervention-action")
        ? card.querySelector(".tl-intervention-action").textContent
        : null,
      actionUsable: (() => {
        const b = card.querySelector(".tl-intervention-action");
        if (!b) return false;
        const cs = getComputedStyle(b);
        return !b.disabled && cs.display !== "none" && cs.visibility !== "hidden";
      })(),
    };
  });

// ---------------------------------------------------------------------------------------
// CONTRAST — through the SHARED library, because this file used to have its own
// ---------------------------------------------------------------------------------------
//
// WHAT WAS HERE UNTIL 2026-09-05, and why a second copy of the arithmetic is worse than no
// check at all. This file carried its own `CONTRAST_PROBE`: forty lines of hand-rolled WCAG
// math and, under it, an `opaqueBehind(el)` that walked the node's ANCESTORS looking for the
// first opaque `background-color`. Three properties came with that, none of them stated:
//
//   1. AN ANCESTOR WALK IS NOT THE PAINT STACK. It cannot see a scrim, an overlay or any
//      sibling painted over the node, so it reports the comfortable ratio of a background
//      that is not the one the eye receives. `lib/contrast.js` resolves the background from
//      `document.elementsFromPoint` inside the text's own line box — the browser's real
//      stack — for exactly this reason, and its header says so.
//   2. A GRADIENT IS INVISIBLE TO IT. An element with `background-image` has
//      `background-color: rgba(0,0,0,0)`, so the walk stepped straight past it and measured
//      against something further up. The shared library REFUSES a gradient by name: a color
//      it cannot resolve is a failure to prove, never a pass.
//   3. IT FELL BACK TO WHITE. `return [255, 255, 255]` when nothing opaque was found — a
//      guess, reported as a measurement, in a file whose own header says the numbers are
//      "computed from the pixels the renderer actually produced, never eyeballed".
//
// It also parsed with `s.match(/[\d.]+/g)`, which turns `color(display-p3 1 0 0)` into
// numbers rather than refusing it, and used WCAG 2.0's published `0.03928` linearization
// threshold where the shared library uses the exact `0.04045`. Neither would have changed a
// verdict here today. That is the point: a second implementation that agrees today is a
// second implementation that will disagree silently later, and the disagreement will be in
// whichever copy nobody is testing.
//
// So the card is measured by the same probe that walks the whole shell, and this file makes
// no arithmetic claims of its own. What it still does itself is the TYPE SCALE — §15's 14px
// floor — because a font size is read, not computed, and there is nothing to get wrong.

const CARD_PARTS = ["tl-intervention-body", "tl-intervention-note", "tl-intervention-action"];

/// The whole fixture page, measured by `lib/contrast.js`'s probe.
///
/// NOT FILTERED TO A SELECTOR, and that is a correction rather than a shortcut. The first
/// version of this function kept only the paths containing one of `CARD_PARTS`, and it
/// silently dropped the card's own BUTTON: the probe's `shortPath` prefers an id over a
/// class chain, so the retry control comes back as `button#retry:turn_outage` with the word
/// "intervention" nowhere in it. Ten nodes measured, three kept, and the one the CEO presses
/// among the seven thrown away — a filter that reported a clean card over two thirds of it,
/// written into the same commit that removed a hand-rolled probe for being unsound.
///
/// The fixture renders one turn and one card and nothing else, so every node the probe finds
/// here is something this suite put on screen. Gating on all of it is both simpler and
/// stricter, and `cardNodes` below is what keeps it honest — it asserts the card's own parts
/// are among what was measured, so "the page is clean" can never mean "the card was not on it".
async function contrastOfPage(page) {
  await page.evaluate(C.pageScript());
  const out = await page.evaluate((o) => window.__contrastProbe(o), {
    surface: "outage-card",
    theme: "driven",
  });
  return {
    measured: out.measuredPaths || [],
    failures: Object.values(out.failures),
    unresolvable: Object.values(out.unresolvable),
    obscured: Object.values(out.obscured).map((o) => o.path),
  };
}

/// Which of the card's own parts the probe measured, matched against the DOM rather than
/// against a guess at the probe's path format. Each element's real path prefix is derived in
/// the page — `button#id` when it has an id, `tag.firstClass` otherwise, which is the rule
/// `shortPath` follows — so a change to either side shows up as a missing part rather than
/// as a quietly smaller number.
async function cardNodes(page, measured) {
  const wanted = await page.evaluate((parts) => {
    const sel = parts.map((c) => "." + c).join(", ");
    return Array.from(document.querySelectorAll(sel)).map((el) => {
      const tag = el.tagName.toLowerCase();
      return el.id ? tag + "#" + el.id : tag + "." + String(el.className).split(/\s+/)[0];
    });
  }, CARD_PARTS);
  const missing = wanted.filter((w) => !measured.some((m) => m.indexOf(w) >= 0));
  return { wanted, missing };
}

/// The type scale, read straight off the computed style. Not a contrast claim, so it needs
/// no probe and makes no arithmetic of its own.
async function typeOfCard(page) {
  return page.evaluate((parts) => {
    const sel = parts.map((c) => "." + c).join(", ");
    return Array.from(document.querySelectorAll(sel)).map((el) => {
      const cs = getComputedStyle(el);
      return {
        cls: el.className,
        px: parseFloat(cs.fontSize),
        text: (el.textContent || "").slice(0, 40),
      };
    });
  }, CARD_PARTS);
}

// ---------------------------------------------------------------------------------------

async function main() {
  const { webkit } = loadPlaywright();
  const browser = await webkit.launch();
  const run = createRun("row 3.30 — the model API failed, and the card says which kind");

  const page = await openFixture(browser);

  // =====================================================================================
  // The `529`
  // =====================================================================================

  await run.check("a 529 replaces the generic card with the backend's own three sentences", async () => {
    await page.evaluate((s) => window.__render(s), snapshot(overloadItem()));
    await page.waitForSelector(".tl-intervention");
    const c = await cardText(page);
    assert(c, "no failure card rendered at all");
    assertEqual(c.body, [OVERLOAD], "the body is the authored overload sentence, verbatim");
    assert(
      c.notes.some((n) => n.includes("Not on disk:")),
      "the loss note must say what is NOT on disk: " + JSON.stringify(c.notes)
    );
    assert(
      !c.body.concat(c.notes).some((t) => t.includes(GENERIC_BODY)),
      "the generic body must be replaced, not appended to"
    );
    assert(
      !c.notes.some((t) => t.includes(GENERIC_NOTE)),
      'the "Everything I\'d already written above is saved" claim must be gone — it is the ' +
        "half of the old card that is false about the whole"
    );
    return `body=1 note(s)=${c.notes.length}`;
  });

  await run.check("the control is still there, and it is still the one verb", async () => {
    const c = await cardText(page);
    assertEqual(c.action, RETRY_LABEL, "same label as every other failure");
    assert(c.actionUsable, "the button is present, enabled and visible");
    return RETRY_LABEL + " — present, enabled, visible";
  });

  await run.check("the attempts spent render when there are any, and not before", async () => {
    // First failure: `RetryBudget::ceo_message` is None on the item, so there is no line.
    let c = await cardText(page);
    assert(
      !c.notes.some((n) => n.includes("tried")),
      "no attempt line before anything was spent: " + JSON.stringify(c.notes)
    );
    const before = c.notes.length;

    await page.evaluate(
      (s) => window.__render(s),
      snapshot(overloadItem("RichOS tried 2 times and stopped there. Each attempt costs against your Claude usage."))
    );
    c = await cardText(page);
    assert(
      c.notes.some((n) => n.includes("tried 2 times")),
      "the count reaches the screen: " + JSON.stringify(c.notes)
    );
    return `${before} note(s) before, ${c.notes.length} after`;
  });

  // =====================================================================================
  // The `429` — the row's fifth answer, on screen
  // =====================================================================================

  await run.check("a 429 says something DIFFERENT from a 529 on the same card", async () => {
    await page.evaluate((s) => window.__render(s), snapshot(quotaItem()));
    const c = await cardText(page);
    assertEqual(c.body, [QUOTA], "the quota sentence");
    assert(!c.body[0].includes("at capacity"), "and it must not read as an overload");
    assert(c.body[0].includes("schedule"), "it names the schedule, which IS the difference");
    assert(OVERLOAD !== QUOTA, "the two sentences differ at the source");
    return "529 and 429 render two different sentences";
  });

  await run.check("a 529 or a 429 on a turn he did not type keeps its explanation and offers no Restore", async () => {
    // Review rv-20261019T115204Z-2b619840-5405#1: a capacity or usage-limit failure on a
    // button-started or agent-sourced turn drew Restore my message, which had nothing to restore.
    const seen = [];
    for (const item of [overloadItem(), quotaItem()]) {
      await page.evaluate((s) => window.__render(s), snapshot(item, { withoutHisWords: true }));
      await page.waitForSelector(".tl-intervention");
      const c = await cardText(page);
      assertEqual(c.body, [item.ceoMessage], "the failure's own sentence stays");
      assertEqual(c.notes, [item.lossMessage], "what is on disk stays");
      assertEqual(c.action, null, "no Restore my message without a message of his");
      seen.push(item.fault);
    }
    return seen.join(" and ") + ": explanation kept, no control";
  });

  // =====================================================================================
  // NEGATIVE CONTROL — the ordinary failure card is untouched
  // =====================================================================================

  await run.check("NEGATIVE CONTROL: a failure with NO outage still gets the generic card", async () => {
    await page.evaluate((s) => window.__render(s), snapshot(null));
    const c = await cardText(page);
    assert(c, "the generic failure card must still render");
    assert(/\bRich\b/.test(c.body[0]), "the fallback names whose answer was cut off: " + JSON.stringify(c.body));
    assert(!/\bI\b/.test(c.body.concat(c.notes).join(" ")), "the app cannot speak as Rich");
    assert(!c.notes.some((n) => n.includes(GENERIC_NOTE)), "no unchecked saved-answer claim");
    assertEqual(c.action, RETRY_LABEL);
    return "an unclassified outcome keeps a neutral RichOS card and its restoration control";
  });

  await run.check("NEGATIVE CONTROL: a COMPLETED turn draws no card at all", async () => {
    const ok = snapshot(null);
    ok.items = ok.items.map((i) =>
      i.kind === "work_duration" ? Object.assign({}, i, { state: "completed" }) : i
    );
    await page.evaluate((s) => window.__render(s), ok);
    const n = await page.evaluate(() => document.querySelectorAll(".tl-intervention").length);
    assertEqual(n, 0, "a healthy turn must not grow an intervention card");
    return "0 cards on a completed turn";
  });

  // =====================================================================================
  // CONTRAST — WCAG AA, BOTH THEMES, COMPUTED
  // =====================================================================================

  for (const theme of ["dark", "light"]) {
    await run.check(`CONTRAST ${theme}: every line of the outage card clears WCAG AA`, async () => {
      await page.evaluate((t) => {
        document.documentElement.dataset.theme = t;
      }, theme);
      await page.evaluate((s) => window.__render(s), snapshot(overloadItem("RichOS tried 2 times and stopped there.")));
      await page.waitForSelector(".tl-intervention");
      const seen = await contrastOfPage(page);

      // WHAT WAS MEASURED, BY NAME, BEFORE ANYTHING IS SAID ABOUT WHAT FAILED. A probe that
      // reached nothing reports zero failures, and zero failures over zero nodes is the exact
      // shape of "green over something that never ran". So the card's own parts — both
      // sentences, the loss note, and the button the CEO presses — are asserted PRESENT in
      // the measurement, not merely absent from the failures.
      const card = await cardNodes(page, seen.measured);
      assert(card.wanted.length >= 4, `the card rendered ${card.wanted.length} measurable part(s); this state has at least 4`);
      assertEqual(
        card.missing,
        [],
        `these part(s) of the card are on screen and the contrast probe measured NONE of them ` +
          `in ${theme} mode. They are not passing; nothing looked at them.`
      );

      // The 4.5 / 3.0 floors and the large-text rule are the library's, applied by the same
      // code that walks the whole shell. This file no longer decides either.
      assertEqual(
        seen.failures.map((f) => `${f.selector} ${f.ratio}:1 < ${f.threshold}:1 — "${f.text || ""}"`),
        [],
        `WCAG AA failures on the outage fixture in ${theme} mode`
      );
      assertEqual(
        seen.unresolvable.map((u) => `${u.path}: ${u.why}`),
        [],
        `colors on the outage fixture that the gate cannot PROVE in ${theme} mode — a failure ` +
          `to prove is never a pass`
      );
      assertEqual(
        seen.obscured,
        [],
        `nothing on this fixture is behind anything, so a node filed 'obscured' means the ` +
          `probe measured it nowhere: ${seen.obscured.join(", ")}`
      );

      // §15's type scale, read rather than computed: nothing readable below 14px.
      const type = await typeOfCard(page);
      assertEqual(
        type.filter((t) => t.px < 14).map((t) => `${t.cls} is ${t.px}px, below the 14px floor`),
        [],
        `type-scale failures in ${theme} mode`
      );

      return (
        `${seen.measured.length} node(s) measured by lib/contrast.js's probe, including all ` +
        `${card.wanted.length} of the card's own (${card.wanted.join(", ")}); 0 failures, ` +
        `0 unprovable, 0 obscured; type ${[...new Set(type.map((t) => t.px))].sort((a, b) => a - b).join("/")}px`
      );
    });
  }

  await run.check("no page errors anywhere in this suite", async () => {
    assertEqual(page.__errors, [], "the shell logged errors while rendering an outage");
    return "0 uncaught errors, 0 console errors";
  });

  await page.close();
  await browser.close();
  process.exitCode = run.report() > 0 ? 1 : 0;
}

main().catch((e) => {
  console.error(e);
  process.exit(1);
});

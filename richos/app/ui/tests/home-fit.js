// **THE HOME SCREEN AT THE SMALLEST WINDOW THE APP ADMITS TO** — candidate-.2 defect #4.
//
// `docs/verification/2026-09-17-nightly-1.2.0-20260917.2-onscreen-audit-2.md` §4:
//
//   *"#4 — home screen does not fit at 1024x700. Ran it on screen. Resize the window to the
//    app's own documented minimum (it restored this size itself on the next launch) and open
//    the home screen. 'INFRASTRUCTURE' loses its final E; the floating chip 'Initiative ·
//    Product' is cut mid-word by the 'This is what your home screen could look like…' panel
//    that sits over it; 'Organization · Product' lands on top of the PRODUCT label."*
//
// 1024x700 is not a size somebody has to go looking for. It is `min_inner_size` — the app's
// own floor, `src-tauri/tauri.conf.json` `minWidth: 1024 / minHeight: 700` and
// `window_geometry.rs` `PREFERRED_MIN_WIDTH/HEIGHT` — and the walk's window restored itself
// to exactly it on the next launch.
//
// ## WHAT THIS SUITE MEASURES, AND WHOSE NUMBERS THEY ARE
//
// `home/field-engine.js` exports the picture's own geometry and this file reads it rather
// than re-deriving it:
//
//   `__loro.domLabelRects`   the domain names it drew, box by box
//   `__loro.nodeLabelRects`  the chips it drew under the cursor, box by box
//   `__loro.eraseRects`      the rectangles `clearQuiet()` rubs out, straight off `quietRects`
//   `__loro.labelBlockers`   the whole no-caption list, the first-run banner included
//
// A test carrying its own copy of the quiet-rectangle arithmetic would be a test of the copy,
// which is rule 1 of this directory's harness ("THE REAL RENDERER, NEVER A COPY"). The price
// of reading the product's own list is that an EMPTY list would pass everything, so check 1
// is a negative control on the list itself: it counts the rects and proves each one covers the
// chrome element it is supposed to cover, measured from the DOM independently.
//
// ## THE SWEEP, AND WHY IT IS A SWEEP
//
// Three of the four symptoms only exist while the cursor is summoning chips, and where the
// cursor is decides which chips. One sample proves nothing about the next one: the first
// measurement taken here, at one hand-picked cursor position, showed a 1.36px overlap and
// would have looked like a rounding argument. The same 12x11 grid, before the fix, showed
// 15 chips under the banner and 106 chips across a domain name.
//
// ## ONE CORRECTION TO THE AUDIT, AND IT IS THE CONTROL SIZE
//
// The audit records 1400x880 as clean, and for the three symptoms it names it is. The
// underlying mechanism was firing there too: the same sweep at 1400x880, before the fix,
// found 269 samples whose box lay inside an erase rectangle — every one of them by less than
// the label's own 8px margin, so no glyph was ever cut and nothing was visible to a person
// looking at the screen. "Clean at 1400x880" is therefore true of what the walk could see and
// not true of what the renderer was doing, which is why this suite counts the two separately.
//
// Run: node home-fit.js   (or `npm test` for every suite in this directory)

"use strict";

const path = require("path");
const { loadPlaywright, createRun, assert, assertEqual, UI_DIR } = require("./lib/harness");

const APP = "file://" + path.join(UI_DIR, "index.html");

/// The app's own documented floor, and the size the walk's window restored itself to.
const MIN = { width: 1024, height: 700 };
/// The walk's control size — every one of the four symptoms was absent here.
const ROOMY = { width: 1400, height: 880 };

/// The cursor grid. Coarse enough to run in about twelve seconds a size, dense enough that
/// the pre-fix defect showed up 121 times rather than once. `STEP` is under the 130px radius
/// `drawOverlay` summons chips inside, so no band of the picture goes unvisited.
const STEP = 40;
const SWEEP = { x0: 380, x1: 820, y0: 90, y1: 520 };
/// How long a cursor move is given before the geometry is read. The label ease is
/// `LABEL_EASE = 0.18` (180ms) in `field-engine.js`; 90ms is half of it, so the sweep also
/// samples labels part-way through appearing — which is a state a person sees and a state a
/// settled-only sample would never look at.
const SETTLE_MS = 90;

const overlaps = (a, b) => a.x0 < b.x1 && a.x1 > b.x0 && a.y0 < b.y1 && a.y1 > b.y0;
const fmt = (r) => `[${r.x0.toFixed(1)},${r.y0.toFixed(1)} → ${r.x1.toFixed(1)},${r.y1.toFixed(1)}]`;

/// THE GLYPHS, not the box the glyphs sit in — and the two are a different fact.
///
/// Every exported label rect carries the `pad` the engine put between its ink and its edge:
/// 8px for a domain name, 3px for a chip. An overlap smaller than that eats margin and nobody
/// can see it; an overlap larger than it eats letters, which is what "INFRASTRUCTUR" is. The
/// suite asserts on the WHOLE box, because the engine keeps the whole box clear on purpose
/// (the 2.5px halo under the ink is what buys the label its contrast ratio) — and it REPORTS
/// the ink count separately, so a failure says which of the two happened instead of leaving a
/// reader to assume the worse one.
const inkBox = (r) => ({ x0: r.x0 + (r.pad || 0), y0: r.y0, x1: r.x1 - (r.pad || 0), y1: r.y1 });

async function openHome(browser, viewport) {
  const page = await browser.newPage({ viewport });
  const errors = [];
  page.on("pageerror", (e) => errors.push(String(e)));
  page.on("console", (m) => {
    if (m.type() === "error") errors.push("console: " + m.text());
  });
  page.__errors = errors;
  await page.goto(APP);
  await page.waitForFunction("typeof window.RichHome === 'object'", null, { timeout: 30000 });
  await page.evaluate(() => window.RichSplash && window.RichSplash.yieldNow("home-fit"));
  await page.evaluate(() => window.RichHome.startField());
  await page.waitForFunction("window.RichHome.state.field === 'live'", null, { timeout: 60000 });
  await page.waitForFunction("window.__loro && !window.__loro.blooming", null, { timeout: 90000 });
  await page.waitForTimeout(800);
  return page;
}

/// Walk the grid and collect EVERY offense, not the first one. A single failing position is
/// an anecdote; the count is what says whether a fix held.
async function sweep(page, viewport) {
  const found = { clipped: [], inkClipped: [], underBanner: [], onDomainName: [], nameOnName: [], offScreen: [] };
  let positions = 0;
  let domainsSeen = 0;
  let chipsSeen = 0;
  for (let x = SWEEP.x0; x <= SWEEP.x1; x += STEP) {
    for (let y = SWEEP.y0; y <= SWEEP.y1; y += STEP) {
      await page.mouse.move(x, y);
      await page.waitForTimeout(SETTLE_MS);
      positions++;
      const got = await page.evaluate(() => ({
        dom: window.__loro.domLabelRects.map((o) => Object.assign({}, o)),
        node: window.__loro.nodeLabelRects.map((o) => Object.assign({}, o)),
        erase: window.__loro.eraseRects,
        banner: (() => {
          const el = document.querySelector("#home-note");
          if (!el || el.hidden) return null;
          const b = el.getBoundingClientRect();
          return { x0: b.left, y0: b.top, x1: b.right, y1: b.bottom };
        })(),
      }));
      domainsSeen += got.dom.length;
      chipsSeen += got.node.length;
      const at = `cursor ${x},${y}`;
      for (const label of got.dom.concat(got.node)) {
        for (const e of got.erase) {
          if (!overlaps(label, e)) continue;
          const ink = overlaps(inkBox(label), e);
          const bite = Math.min(label.x1, e.x1) - Math.max(label.x0, e.x0);
          found.clipped.push(`${at}: label ${fmt(label)} inside erase ${fmt(e)} by ${bite.toFixed(1)}px${ink ? " — GLYPHS" : " — margin only"}`);
          if (ink) found.inkClipped.push(`${at}: ${fmt(label)} loses ${(bite - (label.pad || 0)).toFixed(1)}px of ink to ${fmt(e)}`);
        }
        if (got.banner && overlaps(label, got.banner)) {
          found.underBanner.push(`${at}: label ${fmt(label)} under banner ${fmt(got.banner)}`);
        }
        if (label.x0 < 0 || label.y0 < 0 || label.x1 > viewport.width || label.y1 > viewport.height) {
          found.offScreen.push(`${at}: label ${fmt(label)} outside ${viewport.width}x${viewport.height}`);
        }
      }
      for (const chip of got.node) {
        for (const name of got.dom) {
          if (overlaps(chip, name)) found.onDomainName.push(`${at}: chip ${fmt(chip)} over domain ${fmt(name)}`);
        }
      }
      // ONE DOMAIN NAME OVER ANOTHER — audit-7 row 13, `CAPITAL 558` sitting under
      // `LEGAL & RISK` on the CEO's home screen, carried from audit-6. This sweep read the
      // names against the chrome, against the banner, against the erase rects and against the
      // node chips, and never against each other: `clearingShift` tested a label's box against
      // `blockRects` (which is chrome, re-read from the DOM) and against nothing else, so two
      // names were free to land on top of one another. Each pair is reported ONCE (`i < j`),
      // because a pair counted twice would make a single defect read as two.
      for (let i = 0; i < got.dom.length; i++) {
        for (let j = i + 1; j < got.dom.length; j++) {
          if (!overlaps(got.dom[i], got.dom[j])) continue;
          const bite = Math.min(got.dom[i].x1, got.dom[j].x1) - Math.max(got.dom[i].x0, got.dom[j].x0);
          const ink = overlaps(inkBox(got.dom[i]), inkBox(got.dom[j]));
          found.nameOnName.push(
            `${at}: domain ${fmt(got.dom[i])} over domain ${fmt(got.dom[j])} by ${bite.toFixed(1)}px` +
              (ink ? " — GLYPHS" : " — margin only")
          );
        }
      }
    }
  }
  return { found, positions, domainsSeen, chipsSeen };
}

async function main() {
  const run = createRun("home fit at the app's documented minimum window");
  const { webkit } = loadPlaywright();
  const browser = await webkit.launch();

  const small = await openHome(browser, MIN);

  // =====================================================================================
  // THE NEGATIVE CONTROL, FIRST — everything below reads the picture's own geometry, so an
  // empty list would report a clean screen. This is what makes the rest mean something.
  // =====================================================================================

  await run.check("NEGATIVE CONTROL: the erase list is real, and it covers the chrome it claims to", async () => {
    const r = await small.evaluate(() => {
      const box = (sel) => {
        const el = document.querySelector(sel);
        if (!el) return null;
        const b = el.getBoundingClientRect();
        return { sel, x0: b.left, y0: b.top, x1: b.right, y1: b.bottom };
      };
      return {
        erase: window.__loro.eraseRects,
        blockers: window.__loro.labelBlockers,
        chrome: ["#home-brand", "#home-signals .sig", "#home-working", "#home-live .cap"].map(box).filter(Boolean),
        banner: box("#home-note"),
        bannerHidden: document.querySelector("#home-note").hidden,
      };
    });
    assert(r.erase.length >= 7, `the picture reports ${r.erase.length} erase rect(s); the home screen has seven groups`);
    const uncovered = r.chrome.filter((c) => !r.erase.some((e) => e.x0 <= c.x0 && e.y0 <= c.y0 && e.x1 >= c.x1 && e.y1 >= c.y1));
    assertEqual(uncovered.map((u) => u.sel), [], "chrome the erase list does not cover");
    assert(!r.bannerHidden, "the first-run banner must be up for this suite to mean anything");
    assert(
      r.blockers.some((b) => r.banner && b.x0 <= r.banner.x0 && b.x1 >= r.banner.x1 && b.y0 <= r.banner.y0 && b.y1 >= r.banner.y1),
      "the first-run banner is not in the no-caption list: " + JSON.stringify(r.blockers)
    );
    return `${r.erase.length} erase rect(s) covering ${r.chrome.length} chrome box(es); banner in the blocker list`;
  });

  // =====================================================================================
  // THE DEFECT, AT THE SIZE IT WAS FOUND
  // =====================================================================================

  const smallSweep = await sweep(small, MIN);

  await run.check("NEGATIVE CONTROL: the sweep saw a picture with names and chips on it", async () => {
    assert(smallSweep.positions >= 100, `only ${smallSweep.positions} cursor position(s) visited`);
    assert(smallSweep.domainsSeen > 0, "no domain name was drawn anywhere in the sweep");
    assert(smallSweep.chipsSeen > 0, "no chip was drawn anywhere in the sweep — the cursor summoned nothing");
    return `${smallSweep.positions} positions, ${smallSweep.domainsSeen} domain-name samples, ${smallSweep.chipsSeen} chip samples`;
  });

  await run.check("1024x700: no label is drawn where the picture then rubs it out", async () => {
    assertEqual(
      smallSweep.found.clipped.length,
      0,
      `clipped labels — this is the mechanism behind INFRASTRUCTUR; ` +
        `${smallSweep.found.inkClipped.length} of them lose actual GLYPHS:\n          ` +
        smallSweep.found.clipped.slice(0, 5).join("\n          ")
    );
    return "0 of " + (smallSweep.domainsSeen + smallSweep.chipsSeen) + " label samples inside an erase rect";
  });

  await run.check("1024x700: no label is drawn under the first-run banner", async () => {
    assertEqual(
      smallSweep.found.underBanner.length,
      0,
      "labels under the opaque banner:\n          " + smallSweep.found.underBanner.slice(0, 5).join("\n          ")
    );
    return "0 label samples under the banner";
  });

  await run.check("1024x700: no chip lands on a domain name", async () => {
    assertEqual(
      smallSweep.found.onDomainName.length,
      0,
      "chips over domain names — this is Organization · Product over PRODUCT:\n          " +
        smallSweep.found.onDomainName.slice(0, 5).join("\n          ")
    );
    return "0 of " + smallSweep.chipsSeen + " chip samples over a domain name";
  });

  await run.check("1024x700: no domain name lands on another domain name", async () => {
    // AUDIT-7 ROW 13, carried from audit-6: `CAPITAL 558` overlapped by `LEGAL & RISK` on the
    // CEO's home screen. This sweep already read the names against the chrome, the banner, the
    // erase rects and the node chips — and never against each other, which is exactly the gap
    // `clearingShift` had: it tested a label's box against `blockRects`, which is chrome
    // re-read from the DOM, and against nothing else.
    assertEqual(
      smallSweep.found.nameOnName.length,
      0,
      "one domain name over another — this is CAPITAL 558 under LEGAL & RISK:\n          " +
        smallSweep.found.nameOnName.slice(0, 5).join("\n          ")
    );
    return "0 overlapping pairs across " + smallSweep.domainsSeen + " domain-name samples";
  });

  await run.check("1024x700: no label leaves the window", async () => {
    assertEqual(
      smallSweep.found.offScreen.length,
      0,
      "labels outside the viewport:\n          " + smallSweep.found.offScreen.slice(0, 5).join("\n          ")
    );
    return "0 label samples outside 1024x700";
  });

  await run.check("1024x700: the fit is bought with labels MOVED, not with labels DELETED", async () => {
    // The cheap way to satisfy every check above is to stop drawing. The walk's own frame 36
    // carried seven domain names at this size; fewer than five would mean the picture had
    // paid for its fit by going quiet, which is a different defect and not an improvement.
    const perPosition = smallSweep.domainsSeen / smallSweep.positions;
    assert(perPosition >= 5, `only ${perPosition.toFixed(2)} domain names on screen on average — the fit was bought by hiding them`);
    return `${perPosition.toFixed(2)} domain names on screen per cursor position`;
  });

  await run.check("no page errors at the minimum size", async () => {
    assertEqual(small.__errors, [], "the page reported errors");
    return "0 errors";
  });

  await small.close();

  // =====================================================================================
  // THE POSITIVE CONTROL — the size the walk found clean stays clean
  // =====================================================================================

  const roomy = await openHome(browser, ROOMY);
  const roomySweep = await sweep(roomy, ROOMY);

  await run.check("CONTROL at 1400x880: the size the walk found clean is still clean", async () => {
    const all = roomySweep.found.clipped
      .concat(roomySweep.found.underBanner)
      .concat(roomySweep.found.onDomainName)
      .concat(roomySweep.found.nameOnName)
      .concat(roomySweep.found.offScreen);
    assertEqual(all.length, 0, "the control size regressed:\n          " + all.slice(0, 5).join("\n          "));
    assert(roomySweep.domainsSeen > 0 && roomySweep.chipsSeen > 0, "the control sweep saw an empty picture");
    return `${roomySweep.positions} positions, ${roomySweep.domainsSeen} domain-name samples, ${roomySweep.chipsSeen} chip samples, nothing found`;
  });

  await run.check("CONTROL at 1400x880: the roomier window still carries MORE names, not fewer", async () => {
    // The two sizes are measured against each other, so a fix that quietly thinned the
    // picture everywhere could not hide behind either one's own number.
    const smallPer = smallSweep.domainsSeen / smallSweep.positions;
    const roomyPer = roomySweep.domainsSeen / roomySweep.positions;
    assert(roomyPer >= smallPer, `1400x880 shows ${roomyPer.toFixed(2)} names and 1024x700 shows ${smallPer.toFixed(2)}`);
    return `1024x700: ${smallPer.toFixed(2)} names/position · 1400x880: ${roomyPer.toFixed(2)} names/position`;
  });

  await run.check("no page errors at the control size", async () => {
    assertEqual(roomy.__errors, [], "the page reported errors");
    return "0 errors";
  });

  await roomy.close();
  await browser.close();
  // `report()` returns the FAILED COUNT, not a verdict. Same form as every other suite here.
  process.exitCode = run.report() > 0 ? 1 : 0;
}

main().catch((e) => {
  console.error(e);
  process.exit(1);
});

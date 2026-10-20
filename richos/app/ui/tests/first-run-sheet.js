// **THE FIRST-RUN COMPANY SHEET, AND THE WAY OUT OF IT** — candidate-.2 defect #5.
//
// `docs/verification/2026-09-17-nightly-1.2.0-20260917.2-onscreen-audit-2.md` §4:
//
//   *"#5 — the explanatory paragraph captions the wrong button. Ran it on screen. On the
//    company sheet, 'Not now is fine. I'll leave the message box switched off until you pick
//    one, and the button to do it stays right there.' sits flush against the bottom edge of
//    Add this company, with zero gap, while the Not now button it describes is below it with
//    normal spacing. Every other paragraph on these sheets has clear spacing. Expected: the
//    sentence sits with the control it explains."*
//
// ## PROXIMITY IS A MEASUREMENT, NOT A JUDGMENT
//
// "Sits with the control it explains" is checkable without anybody's taste in it: the gap
// above the sentence and the gap below it are two numbers, and the sentence belongs to
// whichever is smaller. Before the fix those numbers were **-4px above and 12px below** — the
// sentence overlapped the bottom edge of the button it does NOT describe and stood clear of
// the one it does. That is not a close call in either direction.
//
// ## AND THE SECOND FINDING, WHICH THE WALK DID NOT SEE
//
// Measuring the first one turned up a worse one in the same sheet. With six companies in the
// registry the panel is 843px tall, `.overlay`'s `padding-top: 12vh` starts it at 108px in a
// 900px window, and the "Not now" button landed at y=901.8 with `overflow: hidden` above it
// and nothing to scroll. The control D5 added to end a wall was itself unreachable — Playwright
// refused to click it sixty times over thirty seconds ("element is outside of the viewport"),
// which is a fair model of a person who cannot reach it either. The walk could not have seen
// it: the CEO's own first launch has an EMPTY registry, so his sheet is shorter. Anybody who
// has added companies has the taller one.
//
// The sheet now clamps to `calc(88vh - 24px)` and scrolls inside that. What is NOT done here,
// and is the better shape if this sheet grows again: bounding the COMPANY LIST, which is the
// only part of the panel whose height is a function of how much a person has, so that the
// sheet's frame — its question and its way out — never moves. The content is 865px at both
// sizes measured (title 20, note 70, list 238, add-a-company form 340, deferral 114); with the
// list at zero it would still be 627px and still not fit at 1024x700, so bounding the list is
// an improvement and not the fix, and it is left undone rather than landed untested.
//
// Run: node first-run-sheet.js   (or `npm test` for every suite in this directory)

"use strict";

const path = require("path");
const { loadPlaywright, leaveHome, createRun, assert, assertEqual, UI_DIR } = require("./lib/harness");

const APP = "file://" + path.join(UI_DIR, "index.html");

/// Both sizes the app can be in. 1024x700 is `min_inner_size`; 1400x900 is the size the
/// unreachable-button measurement was taken at and is what the other suites here open.
const SIZES = [
  { width: 1400, height: 900 },
  { width: 1024, height: 700 },
];

/// The launch that asks the question: no company has ever been chosen. Handed to `mock.js`
/// before the page's own scripts run, because `init()` has already branched on it by the time
/// any setter could be called.
const FIRST_RUN = { chosenEntity: null };

async function openSheet(browser, viewport) {
  const page = await browser.newPage({ viewport });
  const errors = [];
  page.on("pageerror", (e) => errors.push(String(e)));
  page.on("console", (m) => {
    if (m.type() === "error") errors.push("console: " + m.text());
  });
  page.__errors = errors;
  await page.addInitScript((v) => {
    window.__RICHOS_MOCK_PRESET__ = v;
  }, FIRST_RUN);
  await page.goto(APP);
  await leaveHome(page);
  await page.waitForFunction("typeof window.RichTimeline === 'object'");
  await page.waitForSelector("#entity-picker:not([hidden])");
  await page.waitForSelector("#entity-picker-later", { state: "visible" });
  return page;
}

async function main() {
  const run = createRun("the first-run company sheet: whose caption, and can it be pressed");
  const { webkit } = loadPlaywright();
  const browser = await webkit.launch();

  for (const viewport of SIZES) {
    const size = `${viewport.width}x${viewport.height}`;
    const page = await openSheet(browser, viewport);

    await run.check(`${size}: the deferral sentence sits with the button it explains`, async () => {
      const m = await page.evaluate(() => {
        const box = (sel) => {
          const el = document.querySelector(sel);
          const b = el.getBoundingClientRect();
          return { top: b.top, bottom: b.bottom, height: b.height };
        };
        const add = box("#entity-add .memory-setup-actions");
        const note = box("#entity-picker-defer-note");
        const button = box("#entity-picker-later");
        return {
          addBottom: add.bottom,
          noteTop: note.top,
          noteBottom: note.bottom,
          buttonTop: button.top,
          above: note.top - add.bottom,
          below: button.top - note.bottom,
          addText: document.querySelector("#entity-add-go").textContent.trim(),
          buttonText: document.querySelector("#entity-picker-later").textContent.trim(),
          // The sentence and the button it names, so a reworded pair cannot pass this by
          // accident: the note must actually be about "Not now".
          noteText: document.querySelector("#entity-picker-defer-note").textContent.replace(/\s+/g, " ").trim(),
          nested: document.querySelector("#entity-picker-defer").contains(document.querySelector("#entity-picker-defer-note")),
        };
      });
      assert(m.noteText.startsWith(m.buttonText), `the sentence must name the button it sits with — note ${JSON.stringify(m.noteText.slice(0, 20))}, button ${JSON.stringify(m.buttonText)}`);
      assert(m.above > 0, `the sentence overlaps "${m.addText}" above it by ${(-m.above).toFixed(1)}px`);
      assert(
        m.below < m.above,
        `the sentence is ${m.below.toFixed(1)}px from the button it explains and ${m.above.toFixed(1)}px from "${m.addText}" — it captions the wrong control`
      );
      assert(m.nested, "the sentence is not inside the deferral group, so nothing keeps the two together");
      return `above "${m.addText}" ${m.above.toFixed(1)}px · below, to "${m.buttonText}", ${m.below.toFixed(1)}px`;
    });

    await run.check(`${size}: the way out of the sheet can actually be reached and pressed`, async () => {
      const before = await page.evaluate(() => {
        const b = document.querySelector("#entity-picker-later").getBoundingClientRect();
        const panel = document.querySelector("#entity-picker .overlay-panel");
        const pb = panel.getBoundingClientRect();
        return {
          bottom: b.bottom,
          top: b.top,
          viewportH: innerHeight,
          panelTop: pb.top,
          panelBottom: pb.bottom,
          panelH: pb.height,
          panelScrollH: panel.scrollHeight,
          scrollable: getComputedStyle(panel).overflowY,
        };
      });
      // THE PANEL MUST BE INSIDE THE WINDOW. That is the half that was broken: the panel ran
      // 843px from y=108 in a 900px window with `overflow: hidden` and nothing to scroll, so
      // its last 52px — the "Not now" and nothing else — were not merely below the fold, they
      // were unreachable by any means a person has.
      assert(
        before.panelBottom <= before.viewportH + 0.5 && before.panelTop >= -0.5,
        `the sheet runs ${before.panelTop.toFixed(1)}-${before.panelBottom.toFixed(1)} in a ${before.viewportH}px window`
      );
      // AND WHEN IT DOES NOT ALL FIT, IT SCROLLS. A clamp that clipped instead would pass the
      // assertion above and lose the button just as completely.
      if (before.panelScrollH > before.panelH + 1) {
        assertEqual(before.scrollable, "auto", "the sheet is taller than its clamp and cannot be scrolled");
      }
      // THE POSITIVE CONTROL: reachable is not the same fact as pressable. Playwright's click
      // does the scroll-into-view, visibility, stability and hit-test work a person's hand
      // does, and it is what refused for 30 seconds at 1400x900 before this was fixed.
      await page.click("#entity-picker-later", { timeout: 5000 });
      await page.waitForSelector("#entity-picker", { state: "hidden", timeout: 5000 });
      const after = await page.evaluate(() => ({
        blocked: !document.getElementById("composer-blocked").hidden,
        control: !document.getElementById("composer-choose-company").hidden,
      }));
      // D5's own invariant, restated here because this suite is the one that presses the
      // button: deferring must never leave an armed composer over a company nobody chose.
      assert(after.blocked, "deferring left the composer unblocked — that is the §21 leak the sheet guards");
      assert(after.control, '"Choose the company" must survive the deferral');
      return `sheet at y=${before.panelTop.toFixed(0)}-${before.panelBottom.toFixed(0)} of ${before.viewportH}, holding ${before.panelScrollH}px of content in ${before.panelH.toFixed(0)}px (overflow-y ${before.scrollable}); "Not now" pressed, block and control both stand`;
    });

    // FINDING 16 of the 2026-10-15 product hunt: the form promised "I'll pick this company on my
    // own whenever you start RichOS from inside it". A Mac app opened from the Dock or Finder
    // starts from /, so that never happens; the form must not promise it.
    await run.check(`${size}: the company form promises nothing about starting RichOS from a folder`, async () => {
      const text = await page.evaluate(() => document.querySelector("#entity-picker").textContent.replace(/\s+/g, " "));
      assert(!/from inside it|start RichOS from|open RichOS from/i.test(text), `the sheet still promises a folder launch: ${JSON.stringify(text.slice(0, 400))}`);
      return "no folder-launch promise in the sheet";
    });

    await run.check(`${size}: no page errors`, async () => {
      assertEqual(page.__errors, [], "the page reported errors");
      return "0 errors";
    });

    await page.close();
  }

  await browser.close();
  // `report()` returns the FAILED COUNT, not a verdict. Same form as every other suite here.
  process.exitCode = run.report() > 0 ? 1 : 0;
}

main().catch((e) => {
  console.error(e);
  process.exit(1);
});

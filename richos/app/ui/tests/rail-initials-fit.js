// **THE RAIL'S INITIALS FIT INSIDE THEIR GOLD CIRCLE** — walk 40, D12.
//
// The circle was 26px while `.rail-initials` went from 11px to 14px, so a wide pair like "MW"
// was as wide as the disc and the top of the W stuck out past its edge, where in dark it sits on
// the sidebar background at 1.04:1. This renders the widest likely pairs and measures the INK
// of every glyph (canvas `actualBoundingBox*`, placed by the per-character Range rect so the
// letter spacing is included), then fails if any corner of any glyph's ink box lies outside the
// circle with a 1px margin. The ink box's corners are the conservative test: the W and M fill
// theirs. Both themes, because the disc is gold in both and the page behind it is not.

"use strict";

const path = require("path");
const { loadPlaywright, createRun, assert, UI_DIR } = require("./lib/harness");

const APP = "file://" + path.join(UI_DIR, "index.html");
const PAIRS = ["MW", "WM", "MM", "WW"];
const MARGIN = 1;

const MEASURE = `(text) => {
  const el = document.getElementById("rail-initials");
  el.textContent = text;
  const cs = getComputedStyle(el);
  const box = el.getBoundingClientRect();
  const cx = box.left + box.width / 2, cy = box.top + box.height / 2, r = box.width / 2;
  const ctx = document.createElement("canvas").getContext("2d");
  ctx.font = cs.fontWeight + " " + cs.fontSize + " " + cs.fontFamily;
  const node = el.firstChild;
  let worst = Infinity, worstGlyph = null;
  for (let i = 0; i < text.length; i++) {
    const range = document.createRange();
    range.setStart(node, i); range.setEnd(node, i + 1);
    const rect = range.getBoundingClientRect();
    const m = ctx.measureText(text[i]);
    const baseline = rect.top + m.fontBoundingBoxAscent;
    const left = rect.left - m.actualBoundingBoxLeft, right = rect.left + m.actualBoundingBoxRight;
    const top = baseline - m.actualBoundingBoxAscent, bottom = baseline + m.actualBoundingBoxDescent;
    for (const [x, y] of [[left, top], [right, top], [left, bottom], [right, bottom]]) {
      const slack = r - Math.hypot(x - cx, y - cy);
      if (slack < worst) { worst = slack; worstGlyph = text[i]; }
    }
  }
  return { size: box.width, fontSize: cs.fontSize, slack: worst, glyph: worstGlyph };
}`;

async function main() {
  const run = createRun("Rail initials fit inside the gold circle (walk 40, D12)");
  const { webkit } = loadPlaywright();
  const browser = await webkit.launch();
  for (const theme of ["dark", "light"]) {
    await run.check(`${theme}: the widest capital pairs sit inside the circle at 14px`, async () => {
      const page = await browser.newPage({ viewport: { width: 1024, height: 700 } });
      await page.addInitScript((t) => {
        try {
          window.localStorage.setItem("richos-theme", t);
          window.localStorage.setItem("richos-font-scale", "100");
          window.localStorage.setItem("richos-mock-config",
            JSON.stringify({ theme: t, font_scale: 100, user_name: "Mary Wu" }));
        } catch (e) { /* default theme */ }
      }, theme);
      await page.goto(APP);
      await page.waitForSelector("#rail-initials", { state: "attached" });
      await page.evaluate(() => window.RichSplash && window.RichSplash.yieldNow("rail-initials-fit"));
      await page.waitForFunction(() => document.getElementById("rail-initials").textContent.length > 0);
      const out = [];
      for (const pair of PAIRS) {
        const m = await page.evaluate(`(${MEASURE})(${JSON.stringify(pair)})`);
        assert(parseFloat(m.fontSize) >= 14, `font is below 14px: ${m.fontSize}`);
        out.push(`${pair} ${m.size}px circle, ${m.slack.toFixed(2)}px clear`);
        assert(m.slack >= MARGIN,
          `${pair}: glyph "${m.glyph}" leaves the circle (${m.slack.toFixed(2)}px clear, need ${MARGIN}) at ${m.fontSize} in a ${m.size}px circle`);
      }
      await page.close();
      return out.join("; ");
    });
  }
  await browser.close();
  process.exitCode = run.report() > 0 ? 1 : 0;
}

main().catch((e) => { console.error(e && e.stack ? e.stack : e); process.exit(1); });

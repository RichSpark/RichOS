#!/usr/bin/env node
// openings-posters.js: the gallery's pictures, drawn by the app's own splash.
//
//   node scripts/openings-posters.js            write ui/assets/openings/posters/<id>.jpg, all 28
//   node scripts/openings-posters.js --check    exit 1 if a catalog row has no picture, or a
//                                               picture has no row
//   (--ui DIR reads and writes another copy of the UI tree; openings-posters.test.sh uses it)
//
// "Your openings" (round 74) shows each opening that is the user's as a card with a picture of
// its finished frame. The pictures ship with the app (640 x 400 JPEG, about 30 KB each) and are
// made here, never by hand: the page the shell draws from at quit (`ui/splash-opening.html`) is
// opened headless, asked for one opening as a saved image is (`card`: the finished frame, held
// at its last moment), and photographed in a 1024 x 640 window at twice the density, then
// scaled with `sips` (the round's own framing: the plinth fills the card). So a picture
// is what Play shows at the end of that opening, drawn by `splash.js` and `openings.js` as they
// are in this checkout. Run it again when a splash's look or the catalog changes, look at the
// pictures, and commit them.
//
// WHAT IS NOT ON A PICTURE: anything that is the user's own. No company line, no date, no held
// line; and the update edition, whose number is the version that is running, is drawn without a
// number (its card's text says the version). Nothing here plays a sound, and nothing is shown
// on this Mac's screen (the browser is headless).

"use strict";

const fs = require("fs");
const os = require("os");
const path = require("path");
const { execFileSync } = require("child_process");

const APP = path.resolve(__dirname, "..");
// `--ui DIR`: another copy of the UI tree (its catalog and its pictures), for the tool's own test.
const UI = process.argv.includes("--ui") ? path.resolve(process.argv[process.argv.indexOf("--ui") + 1]) : path.join(APP, "ui");
const OUT = path.join(UI, "assets", "openings", "posters");

function catalog() {
  const src = fs.readFileSync(path.join(UI, "openings-catalog.js"), "utf8");
  const at = src.indexOf("window.RichOpeningsCatalog");
  return JSON.parse(src.slice(src.indexOf("=", at) + 1).trim().replace(/;\s*$/, "")).openings;
}

function check() {
  const ids = catalog().map((o) => o.id);
  const have = fs.existsSync(OUT) ? fs.readdirSync(OUT).filter((f) => f.endsWith(".jpg")).map((f) => f.slice(0, -4)) : [];
  const missing = ids.filter((id) => !have.includes(id));
  const stray = have.filter((id) => !ids.includes(id));
  for (const id of missing) console.error("no picture for the opening " + id);
  for (const id of stray) console.error("a picture with no opening: " + id + ".jpg");
  console.log(ids.length + " openings, " + have.length + " pictures, " + missing.length + " missing, " + stray.length + " stray");
  return missing.length || stray.length ? 1 : 0;
}

async function draw() {
  const { loadPlaywright } = require(path.join(UI, "tests", "lib", "harness"));
  const openings = catalog();
  const earned = {};
  for (const o of openings) earned[o.id] = "2026-01-01";
  const browser = await loadPlaywright().webkit.launch();
  const tmp = fs.mkdtempSync(path.join(os.tmpdir(), "openings-posters-"));
  fs.mkdirSync(OUT, { recursive: true });
  try {
    for (const o of openings) {
      const ctx = await browser.newContext({ viewport: { width: 1024, height: 640 }, deviceScaleFactor: 2 });
      const page = await ctx.newPage();
      const errors = [];
      page.on("pageerror", (e) => errors.push(String(e)));
      await page.addInitScript(() => {
        // Nothing here makes a sound: a drawn page asks for none, and none can be made.
        window.Audio = function () { return { load() {}, play() { return Promise.resolve(); } }; };
      });
      await page.addInitScript(
        ([id, mine]) => {
          window.__RICHOS_OPENINGS__ = { firstStart: "2026-01-01", version: "", earned: mine, pick: null, sound: false, company: null, day: 0 };
          window.__RICHOS_LAUNCH__ = Object.freeze({ kind: "fresh", ordinal: null, draw: true, card: true, splash: Object.freeze({ id: "opening/" + id }) });
        },
        [o.id, earned]
      );
      await page.goto("file://" + path.join(UI, "splash-opening.html"));
      await page.waitForFunction(() => window.RichSplash && window.RichSplash.state.card === true, null, { timeout: 60000 });
      await page.evaluate(() => document.fonts.ready);
      const drawn = await page.evaluate(() => {
        const n = document.getElementById("splash");
        n.classList.remove("splash--card"); // the held line carries a date: not on a picture
        window.RichSplash.holdAt(window.RichSplash.state.heldAt);
        return { id: window.RichSplash.state.variationId, opening: n.dataset.opening };
      });
      if (drawn.id !== "opening/" + o.id || errors.length) {
        throw new Error(o.id + ": drew " + JSON.stringify(drawn) + (errors.length ? "; page errors: " + errors.join("; ") : ""));
      }
      const big = path.join(tmp, o.id + ".jpg");
      await page.screenshot({ path: big, type: "jpeg", quality: 92 });
      await ctx.close();
      const out = path.join(OUT, o.id + ".jpg");
      execFileSync("sips", ["-s", "format", "jpeg", "-s", "formatOptions", "82", "-z", "400", "640", big, "--out", out], { stdio: "ignore" });
      console.log(o.id + ".jpg  " + fs.statSync(out).size + " bytes");
    }
  } finally {
    await browser.close();
    fs.rmSync(tmp, { recursive: true, force: true });
  }
  return check();
}

if (process.argv.includes("--check")) process.exit(check());
draw().then(
  (code) => process.exit(code),
  (e) => {
    console.error(e);
    process.exit(1);
  }
);

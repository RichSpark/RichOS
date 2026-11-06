// THE NAVIGATION RAIL (design round 58, version A "Hairline"; the CEO's pick, 2026-11-06),
// driven through the real shell, both themes.
//
// His words, the acceptance criterion: "No nav rail on the home screen. So, the home screen
// needs to be the same as before. Nav rail everywhere else but not on the home screen." "And
// clicking the RichOS wordmark should still retain the same effect as clicking the home button
// on the nav rail i.e. getting the user to the home screen." "the 2 items at the very bottom of
// the left sidebar should move into the nav rail: The user's name (reduced to a circle with
// initials like in the Codex app) and above it the CPU thing (activity monitor)."
//
//    1  the rail is the window's left edge, 52 px, left of the sidebar; on the home screen it
//       is covered and inert, and the home screen's own frame is unchanged
//    2  Home and the wordmark both go to the home screen
//    3  Chats is selected on the app, with the 3 px gold bar beside it; RichFinder opens the
//       palette ⌘K opens and is selected while it is open; over it the rail stays reachable
//    4  RichCleaner opens RichCleaner's own window (`cleaner_open_window`)
//    5  RichScribe is shown only where RichScribe is; it opens the RichScribe sheet, is selected
//       while it is open, and carries its gold dot while a Mac permission is missing
//    6  Bust a bug opens the panel over this screen, with the RichDoctor line, is selected and
//       inert while a report is open; an idea carries no RichDoctor line
//    7  the initials circle: "AB" for Alex Booster and Settings opens; no name, a person
//       outline and Settings opens on the name field
//    8  the sidebar has no footer: the square and the circle are on the rail and nowhere else,
//       and hiding the sidebar (⌘⇧S) leaves the rail
//    9  every glyph, at rest, on hover and selected, the bar and the dot clear 3:1 against the
//       rail's painted pixels; the tooltip names each item to its right at 16 px and 4.5:1
//   10  Up and Down move along the rail
//   11  the rail photographed in both themes (shots-nav-rail/)
//   12  the rail stays on Token spend (review rv-20261106T173510Z-aa7dd4d6-bca3, finding 1), and with
//       RichScribe open its hover label and the activity square's Ink/Readout menu sit above the
//       sheet and a choice can be clicked (finding 2)
//
// Run: node nav-rail.js            (RICHOS_SHOTS_REGENERATE=shots-nav-rail to rewrite shots)

"use strict";

const fs = require("fs");
const path = require("path");
const {
  loadPlaywright, createRun, assert, assertEqual, UI_DIR, leaveHome, shellSettled, flushFrames,
  captureSettled, publishShot,
} = require("./lib/harness");
const C = require("./lib/contrast");
const png = require("./lib/png");

const APP = "file://" + path.join(UI_DIR, "index.html");
const SHOTS = path.join(__dirname, "shots-nav-rail");
const GLYPH_FLOOR = 3;
const TEXT_FLOOR = 4.5;
const ITEMS = ["home", "chats", "finder", "cleaner", "scribe", "bug"];

async function newContext(browser, theme, opts) {
  opts = opts || {};
  const context = await browser.newContext({
    viewport: { width: 1440, height: 900 }, colorScheme: theme, reducedMotion: "reduce", deviceScaleFactor: 2,
  });
  await context.addInitScript(([t, name, preset]) => {
    try {
      window.localStorage.setItem("richos-theme", t);
      window.localStorage.setItem("richos-font-scale", "100");
      window.localStorage.setItem("richos-mock-config", JSON.stringify({ theme: t, font_scale: 100, user_name: name }));
      if (preset) window.__RICHOS_MOCK_PRESET__ = preset;
    } catch (_e) { /* the default theme */ }
  }, [theme, opts.name === undefined ? "Alex Booster" : opts.name, opts.preset || null]);
  return context;
}

async function openApp(context, opts) {
  const page = await context.newPage();
  page.__errors = [];
  page.on("pageerror", (e) => page.__errors.push(String(e)));
  await page.goto(APP);
  if (opts && opts.stayHome) return page;
  await leaveHome(page);
  await shellSettled(page);
  await page.waitForSelector("#nav-rail", { state: "visible" });
  await flushFrames(page);
  return page;
}

const railFacts = (page) => page.evaluate(() => {
  const nav = document.getElementById("nav-rail").getBoundingClientRect();
  const ind = document.querySelector("#nav-rail .nr-ind");
  const sel = Array.from(document.querySelectorAll("#nav-rail .nr-item[aria-current='page']")).map((b) => b.dataset.item);
  const indBox = ind.getBoundingClientRect();
  const selBox = sel.length ? document.querySelector(`#nav-rail .nr-item[data-item="${sel[0]}"]`).getBoundingClientRect() : null;
  return {
    nav: { left: nav.left, width: nav.width, right: nav.right },
    sidebarLeft: document.getElementById("rail").getBoundingClientRect().left,
    selected: sel,
    barOn: ind.classList.contains("is-on") && getComputedStyle(ind).opacity === "1",
    barBesideSelected: !!selBox && Math.abs((indBox.top + indBox.bottom) / 2 - (selBox.top + selBox.bottom) / 2) <= 1 && indBox.left <= nav.left + 0.5,
    bugDisabled: document.getElementById("nr-bug").getAttribute("aria-disabled") === "true",
  };
});

/// The most common color in a strip of rendered pixels: the ground that is actually painted.
async function groundPixel(page, rect) {
  const buf = await page.screenshot({ clip: rect });
  const img = png.decode(buf);
  const counts = new Map();
  for (let i = 0; i < img.width * img.height; i++) {
    const o = i * 4;
    const k = C.hex({ r: img.data[o], g: img.data[o + 1], b: img.data[o + 2], a: 1 });
    counts.set(k, (counts.get(k) || 0) + 1);
  }
  const top = Array.from(counts.entries()).sort((a, b) => b[1] - a[1])[0][0];
  return hexColor(top);
}
function hexColor(h) {
  const m = /^#([0-9a-f]{2})([0-9a-f]{2})([0-9a-f]{2})$/i.exec(h);
  return { r: parseInt(m[1], 16), g: parseInt(m[2], 16), b: parseInt(m[3], 16), a: 1 };
}
/// How many pixels in a crop are within `tol` of a color: proof the glyph is drawn in it.
async function pixelsOf(page, rect, color, tol) {
  const img = png.decode(await page.screenshot({ clip: rect }));
  let n = 0;
  for (let i = 0; i < img.width * img.height; i++) {
    const o = i * 4;
    if (Math.abs(img.data[o] - color.r) <= tol && Math.abs(img.data[o + 1] - color.g) <= tol && Math.abs(img.data[o + 2] - color.b) <= tol) n++;
  }
  return n;
}
const boxOf = (page, sel) => page.evaluate((s) => {
  const r = document.querySelector(s).getBoundingClientRect();
  return { x: r.left, y: r.top, width: r.width, height: r.height };
}, sel);
const colorOf = (page, sel, prop) => page.evaluate(([s, p]) => getComputedStyle(document.querySelector(s))[p], [sel, prop || "color"]);

async function main() {
  const run = createRun("The navigation rail (round 58, version A): items, selection, the moved square and initials, contrast");
  const { webkit } = loadPlaywright();
  const browser = await webkit.launch();
  const report = [];

  for (const theme of ["dark", "light"]) {
    const context = await newContext(browser, theme, { preset: { dictation: true } });
    let page = await openApp(context);

    await run.check(`1 ${theme}  the rail is the window's left edge, 52 px; on the home screen it is covered and inert`, async () => {
      const f = await railFacts(page);
      assertEqual([f.nav.left, f.nav.width], [0, 52], "the rail's place and width");
      assertEqual(f.sidebarLeft, 52, "the sidebar starts at the rail's right edge");
      const home = await context.newPage();
      await home.goto(APP);
      const leave = home.evaluate(() => window.RichSplash && window.RichSplash.yieldNow("nav-rail"));
      await leave;
      await home.waitForFunction(() => window.RichHome && window.RichHome.isOpen() && !document.getElementById("splash"), null, { timeout: 30000 });
      const onHome = await home.evaluate(() => {
        const r = document.getElementById("nr-home").getBoundingClientRect();
        const hit = document.elementFromPoint(r.left + r.width / 2, r.top + r.height / 2);
        return { inert: document.getElementById("app").hasAttribute("inert"), covered: !!hit && !document.getElementById("nav-rail").contains(hit), by: hit && (hit.id || hit.tagName) };
      });
      await home.close();
      assert(onHome.inert && onHome.covered, "on the home screen the rail is not showing: " + JSON.stringify(onHome));
      return `x=0, 52px wide, sidebar from 52px; on the home screen the point is ${onHome.by} and #app is inert`;
    });

    await run.check(`2 ${theme}  Home and the RichOS wordmark both go to the home screen`, async () => {
      await page.click("#nr-home");
      await page.waitForFunction(() => window.RichHome.isOpen());
      await page.evaluate(() => window.RichHome.hide("nav-rail-test"));
      await page.waitForFunction(() => !window.RichHome.isOpen());
      await flushFrames(page);
      await page.click("#rail-wordmark");
      await page.waitForFunction(() => window.RichHome.isOpen());
      const reasons = await page.evaluate(() => window.RichHome.state ? window.RichHome.state.lastEnterReason : null);
      await page.evaluate(() => window.RichHome.hide("nav-rail-test"));
      await page.waitForFunction(() => !window.RichHome.isOpen());
      await flushFrames(page);
      return "Home and the wordmark each opened the home screen" + (reasons ? ` (last: ${reasons})` : "");
    });

    await run.check(`3 ${theme}  Chats is selected with the bar beside it; RichFinder opens the palette and is selected while it is open`, async () => {
      let f = await railFacts(page);
      assertEqual(f.selected, ["chats"], "on the app, Chats is the place");
      assert(f.barOn && f.barBesideSelected, "the 3 px gold bar is beside Chats");
      await page.click("#nr-finder");
      await page.waitForSelector("#search-overlay", { state: "visible" });
      await flushFrames(page);
      assertEqual(await page.evaluate(() => document.activeElement.id), "search-input", "the palette's field has the keyboard");
      f = await railFacts(page);
      assertEqual(f.selected, ["finder"], "RichFinder is selected while its palette is open");
      assert(f.barBesideSelected, "the bar slid to RichFinder");
      // The rail stays reachable over the palette (round 58): its scrim starts at the rail's
      // edge, and a press on Home closes the palette and goes home.
      const reach = await page.evaluate(() => {
        const o = document.getElementById("search-overlay").getBoundingClientRect();
        const h = document.getElementById("nr-home").getBoundingClientRect();
        const hit = document.elementFromPoint(h.left + h.width / 2, h.top + h.height / 2);
        return { overlayLeft: o.left, railRight: document.getElementById("nav-rail").getBoundingClientRect().right, onHome: !!hit && !!hit.closest("#nr-home") };
      });
      assert(reach.onHome && reach.overlayLeft >= reach.railRight - 1, "the palette covers the rail: " + JSON.stringify(reach));
      await page.click("#nr-home");
      await page.waitForFunction(() => window.RichHome.isOpen() && document.getElementById("search-overlay").hidden);
      await page.evaluate(() => window.RichHome.hide("nav-rail-test"));
      await page.waitForFunction(() => !window.RichHome.isOpen());
      await flushFrames(page);
      // Corrections open (an Escape-declared overlay), RichFinder from the rail: Corrections
      // closes by its own close and the palette opens in front with the keyboard, never under it.
      await page.click("#nav-corrections");
      await page.waitForSelector("#corrections-overlay", { state: "visible" });
      await page.click("#nr-finder");
      await page.waitForSelector("#search-overlay", { state: "visible" });
      await page.waitForSelector("#corrections-overlay", { state: "hidden" });
      await flushFrames(page);
      assertEqual(await page.evaluate(() => document.activeElement.id), "search-input", "the palette has the keyboard after Corrections closed");
      await page.keyboard.press("Escape");
      await page.waitForSelector("#search-overlay", { state: "hidden" });
      await flushFrames(page);
      assertEqual((await railFacts(page)).selected, ["chats"], "closed, Chats again");
      assert(await page.evaluate(() => {
        const img = document.querySelector("#search-overlay .srch-check img.feature-icon");
        return !!img && /richfinder-128\.png$/.test(img.getAttribute("src")) && img.nextElementSibling && img.nextElementSibling.tagName === "SPAN";
      }), "the RichFinder icon is in front of its name in the palette");
      return "Chats, then RichFinder while the palette is open, then Chats";
    });

    await run.check(`4 ${theme}  RichCleaner opens RichCleaner's own window`, async () => {
      await page.evaluate(() => {
        window.__navCalls = [];
        const invoke = window.RichBridge.invoke.bind(window.RichBridge);
        window.RichBridge.invoke = (cmd, args) => { window.__navCalls.push(cmd); return invoke(cmd, args); };
      });
      await page.click("#nr-cleaner");
      await page.waitForFunction(() => window.__navCalls.includes("cleaner_open_window"));
      await flushFrames(page);
      assertEqual((await railFacts(page)).selected, ["chats"], "another window is never a place on this rail");
      return "cleaner_open_window asked for";
    });

    await run.check(`5 ${theme}  RichScribe opens its sheet, is selected while open, and carries the dot while a permission is missing`, async () => {
      await page.waitForSelector("#nr-scribe", { state: "visible" });
      await page.evaluate(() => window.__RICHOS_MOCK__.dictationSet({ on: true, mic: "allowed", ax: "denied", owner: "self" }));
      await page.click("#nr-scribe");
      await page.waitForSelector("#dictation-sheet", { state: "visible" });
      await page.waitForFunction(() => !document.querySelector("#nr-scribe .nr-dot").hidden);
      await flushFrames(page);
      const f = await railFacts(page);
      assertEqual(f.selected, ["scribe"], "RichScribe is selected while its sheet is open");
      assertEqual(await page.getAttribute("#nr-scribe", "aria-label"), "RichScribe, needs a Mac permission", "the dot is said, not only shown");
      assert(await page.evaluate(() => {
        const h = document.getElementById("dict-title");
        const img = h && h.querySelector("img.feature-icon");
        return !!img && /richscribe-128\.png$/.test(img.getAttribute("src")) && h.textContent.trim() === "RichScribe";
      }), "the RichScribe icon is in front of the sheet's title");
      // Over the sheet the rail is still reachable: RichFinder closes it by its own Close and the
      // palette keeps the keyboard after the sheet's close has handed focus back.
      await page.click("#nr-finder");
      await page.waitForSelector("#dictation-sheet", { state: "hidden" });
      await page.waitForSelector("#search-overlay", { state: "visible" });
      await flushFrames(page);
      assertEqual(await page.evaluate(() => document.activeElement.id), "search-input", "the palette keeps the keyboard after RichScribe closed");
      await page.keyboard.press("Escape");
      await page.waitForSelector("#search-overlay", { state: "hidden" });
      await flushFrames(page);
      assertEqual((await railFacts(page)).selected, ["chats"], "Chats again, both closed");
      // Stacked: the palette, then the RichScribe sheet over it (Settings opens it there), then
      // Home: both close, top down, and the home screen stays up.
      await page.evaluate(() => { window.RichSearch.open(); window.RichDictation.open(); });
      await page.waitForSelector("#dictation-sheet", { state: "visible" });
      await page.click("#nr-home");
      await page.waitForFunction(() => window.RichHome.isOpen() && document.getElementById("search-overlay").hidden && document.getElementById("dictation-sheet").hidden);
      await flushFrames(page);
      assert(await page.evaluate(() => window.RichHome.isOpen()), "the home screen stays up once the stacked sheets are closed");
      await page.evaluate(() => window.RichHome.hide("nav-rail-test"));
      await page.waitForFunction(() => !window.RichHome.isOpen());
      await flushFrames(page);
      await page.evaluate(() => window.__RICHOS_MOCK__.dictationSet({ ax: "allowed", keyTap: true }));
      await page.evaluate(() => document.getElementById("nr-scribe").click());
      await page.waitForFunction(() => document.querySelector("#nr-scribe .nr-dot").hidden);
      await page.click("#dict-close");
      await page.waitForSelector("#dictation-sheet", { state: "hidden" });
      return "sheet, selected, dot while Accessibility is missing, no dot once it is allowed";
    });

    await run.check(`6 ${theme}  Bust a bug opens the panel with the RichDoctor line, selected and inert while open; an idea has no line`, async () => {
      await page.click("#nr-bug");
      await page.waitForSelector("#bugdock", { state: "visible" });
      await page.waitForFunction(() => document.getElementById("nr-bug").getAttribute("aria-disabled") === "true");
      await flushFrames(page);
      const f = await railFacts(page);
      assertEqual(f.selected, ["bug"], "Bust a bug is selected while its panel is open");
      const line = await page.evaluate(() => {
        const p = document.getElementById("bugdock-doctor");
        return { shown: !p.hidden && p.getClientRects().length > 0, text: p.textContent, size: parseFloat(getComputedStyle(p).fontSize) };
      });
      assertEqual(line.text, "If RichOS ever won't open, RichDoctor in your menu bar files the same report.", "the RichDoctor line");
      assert(line.shown && line.size >= 16, "the RichDoctor line is on screen at 16px: " + JSON.stringify(line));
      // `aria-disabled` keeps it focusable and named; a press reaches it and does nothing.
      // (Playwright's own click waits for an enabled control, so the press is the DOM's.)
      const asked = await page.evaluate(() => document.querySelectorAll("#bugdock-msgs > *").length);
      await page.evaluate(() => document.getElementById("nr-bug").click());
      await flushFrames(page);
      assertEqual(await page.evaluate(() => document.querySelectorAll("#bugdock-msgs > *").length), asked, "a second press while a report is open does nothing");
      await page.click("#bugdock-x");
      await page.waitForSelector("#bugdock", { state: "hidden" });
      await page.waitForFunction(() => document.getElementById("nr-bug").getAttribute("aria-disabled") !== "true");
      await page.evaluate(() => window.RichBug && window.RichSettings.openMenu());
      await page.click("#idea-btn");
      await page.waitForSelector("#bugdock", { state: "visible" });
      assertEqual(await page.evaluate(() => document.getElementById("bugdock-doctor").hidden), true, "no RichDoctor line under an idea");
      await page.click("#bugdock-x");
      await page.waitForSelector("#bugdock", { state: "hidden" });
      await flushFrames(page);
      return `panel open with the line at ${line.size}px; inert while open; none under Request a feature`;
    });

    await run.check(`7 ${theme}  the initials circle: AB opens Settings; with no name a person outline opens Settings on the name field`, async () => {
      const named = await page.evaluate(() => ({
        text: document.getElementById("nr-initials").textContent,
        unset: document.getElementById("nr-me").classList.contains("is-unset"),
        label: document.getElementById("nr-me").getAttribute("aria-label"),
        tip: document.getElementById("nr-me").dataset.tip,
      }));
      assertEqual(named, { text: "AB", unset: false, label: "Alex Booster, open Settings", tip: "Alex Booster" }, "named");
      await page.click("#nr-me");
      await page.waitForSelector("#set-menu", { state: "visible" });
      await page.keyboard.press("Escape");
      await page.waitForSelector("#set-menu", { state: "hidden" });
      const anon = await newContext(browser, theme, { name: "" });
      try {
        const p = await openApp(anon);
        const unset = await p.evaluate(() => ({
          text: document.getElementById("nr-initials").textContent,
          unset: document.getElementById("nr-me").classList.contains("is-unset"),
          outline: getComputedStyle(document.querySelector("#nr-me .nr-anon")).display !== "none",
          label: document.getElementById("nr-me").getAttribute("aria-label"),
        }));
        assertEqual(unset, { text: "", unset: true, outline: true, label: "Set your name, open Settings" }, "no name");
        await p.click("#nr-me");
        await p.waitForSelector("#set-menu", { state: "visible" });
        await p.waitForFunction(() => document.activeElement && document.activeElement.id === "user-name-input");
        assertEqual(await p.textContent("#user-name-hint"), "Shown at the bottom of the navigation rail, with your initials.", "the name field's hint");
        await p.fill("#user-name-input", "Mary Wu");
        await p.press("#user-name-input", "Enter");
        await p.waitForFunction(() => document.getElementById("nr-initials").textContent === "MW");
        assertEqual(p.__errors, [], "no page errors");
      } finally { await anon.close(); }
      return "AB opens Settings; no name: outline, Settings on the name field, a name typed there becomes MW";
    });

    await run.check(`8 ${theme}  the sidebar has no footer; hiding the sidebar leaves the rail`, async () => {
      const where = await page.evaluate(() => ({
        footer: !!document.getElementById("rail-footer"),
        squareInSidebar: document.getElementById("rail").contains(document.getElementById("res-square")),
        squareOnRail: document.getElementById("nav-rail").contains(document.getElementById("res-square")),
        circleOnRail: document.getElementById("nav-rail").contains(document.getElementById("nr-me")),
        oldIdentity: !!document.getElementById("rail-identity"),
      }));
      assertEqual(where, { footer: false, squareInSidebar: false, squareOnRail: true, circleOnRail: true, oldIdentity: false }, "where the two moved items are");
      await page.keyboard.press("Meta+Shift+S");
      await page.waitForFunction(() => document.body.classList.contains("rail-closed"));
      await flushFrames(page);
      assert(await page.isVisible("#nav-rail") && await page.isVisible("#res-square") && await page.isVisible("#nr-me"), "the rail, the square and the circle stay with the sidebar hidden");
      await page.keyboard.press("Meta+Shift+S");
      await page.waitForFunction(() => !document.body.classList.contains("rail-closed"));
      await flushFrames(page);
      return "no footer; square and circle on the rail only; ⌘⇧S hides the sidebar and leaves the rail";
    });

    await run.check(`9 ${theme}  glyphs at rest, on hover and selected, the bar and the dot clear 3:1; the tooltip reads at 16 px and 4.5:1`, async () => {
      const lines = [];
      // The rail's own painted pixels, in the gap between the divider and RichFinder.
      const div = await boxOf(page, "#nav-rail .nr-div");
      const ground = await groundPixel(page, { x: 4, y: div.y + 3, width: 44, height: 3 });
      const measure = async (what, sel, prop, bg) => {
        const c = C.parseCssColor(await colorOf(page, sel, prop));
        const under = bg || ground;
        const ratio = C.round2(C.contrastRatio(C.compositeOver(c, under), under));
        lines.push(`${theme} ${what} ${C.hex(c)} on ${C.hex(under)} ${ratio}:1`);
        assert(ratio >= GLYPH_FLOOR, `${what} ${C.hex(c)} on ${C.hex(under)} is ${ratio}:1, under ${GLYPH_FLOOR}`);
        return c;
      };
      await page.mouse.move(700, 450);
      for (const k of ITEMS) {
        if (k === "chats") continue;
        const c = await measure(`${k} at rest`, `#nr-${k}`);
        const drawn = await pixelsOf(page, await boxOf(page, `#nr-${k} .nr-glyph`), c, 3);
        assert(drawn >= 20, `${k}: the glyph is not drawn in its ink (${drawn} px)`);
      }
      await page.hover("#nr-cleaner");
      await flushFrames(page);
      await measure("glyph on hover", "#nr-cleaner");
      await page.mouse.move(700, 450);
      // Selected: the glyph against the soft gold tile it sits on, and the bar against the rail.
      const tile = C.compositeOver(C.parseCssColor(await page.evaluate(() => getComputedStyle(document.getElementById("nr-chats"), "::before").backgroundColor)), ground);
      const sel = await measure("selected glyph (on its tile)", "#nr-chats", "color", tile);
      const drawnSel = await pixelsOf(page, await boxOf(page, "#nr-chats .nr-glyph"), sel, 6);
      assert(drawnSel >= 20, `the selected glyph is not drawn in gold (${drawnSel} px)`);
      await measure("selected bar", "#nav-rail .nr-ind", "backgroundColor");
      await measure("RichScribe's dot", "#nr-scribe .nr-dot", "backgroundColor");
      // The tooltip: shown to the right on hover, its name at 16px and 4.5:1.
      await page.hover("#nr-finder");
      await page.waitForSelector("#nr-tip", { state: "visible" });
      await flushFrames(page);
      const tip = await page.evaluate(() => {
        const t = document.getElementById("nr-tip");
        const b = document.getElementById("nr-finder").getBoundingClientRect();
        const r = t.getBoundingClientRect();
        const cs = getComputedStyle(t);
        return { text: t.textContent, size: parseFloat(cs.fontSize), fg: cs.color, bg: cs.backgroundColor, right: r.left > b.right, mid: Math.abs((r.top + r.bottom) / 2 - (b.top + b.bottom) / 2) };
      });
      assertEqual(tip.text, "RichFinder⌘K", "the tooltip names RichFinder with its shortcut");
      assert(tip.right && tip.mid <= 1, "the tooltip is to the right of its item, level with it");
      const ratio = C.round2(C.contrastRatio(C.parseCssColor(tip.fg), C.parseCssColor(tip.bg)));
      assert(tip.size >= 16 && ratio >= TEXT_FLOOR, `the tooltip is ${tip.size}px at ${ratio}:1`);
      lines.push(`${theme} tooltip ${tip.fg} on ${tip.bg} ${ratio}:1 at ${tip.size}px`);
      await page.mouse.move(700, 450);
      await page.waitForSelector("#nr-tip", { state: "hidden" });
      report.push(...lines);
      return `${lines.length} measured, worst ${Math.min(...lines.map((l) => parseFloat(/([\d.]+):1/.exec(l)[1])))}:1`;
    });

    await run.check(`10 ${theme}  Up and Down move along the rail`, async () => {
      await page.focus("#nr-home");
      const seen = [];
      for (let i = 0; i < 3; i++) {
        await page.keyboard.press("ArrowDown");
        seen.push(await page.evaluate(() => document.activeElement.id));
      }
      await page.keyboard.press("ArrowUp");
      seen.push(await page.evaluate(() => document.activeElement.id));
      assertEqual(seen, ["nr-chats", "nr-finder", "nr-cleaner", "nr-finder"], "the order focus takes");
      await page.evaluate(() => document.activeElement.blur());
      return seen.join(" → ");
    });

    await run.check(`11 ${theme}  the rail photographed (shots-nav-rail/)`, async () => {
      await page.mouse.move(700, 450);
      const buf = await captureSettled(page, { screenshot: { clip: { x: 0, y: 0, width: 420, height: 900 } } });
      fs.mkdirSync(SHOTS, { recursive: true });
      publishShot(buf, path.join(SHOTS, `rail-${theme}.png`));
      assertEqual(page.__errors, [], "no page errors");
      return `shots-nav-rail/rail-${theme}.png`;
    });

    await run.check(`12 ${theme}  the rail stays on Token spend; over RichScribe its flyouts sit above the sheet and take a click`, async () => {
      const topId = (sel) => page.evaluate((s2) => {
        const r = document.querySelector(s2).getBoundingClientRect();
        const el = document.elementFromPoint(r.left + r.width / 2, r.top + r.height / 2);
        const own = el && el.closest(s2);
        return own ? "self" : (el ? (el.id || el.className || el.tagName) : "nothing");
      }, sel);
      await page.click("#set-btn");
      await page.waitForSelector("#set-menu #set-spend-open");
      await page.click("#set-spend-open");
      await page.waitForSelector("#spend-fig .ch");
      await flushFrames(page);
      const onSpend = await page.evaluate(() => {
        const n = document.getElementById("nav-rail");
        const r = n.getBoundingClientRect();
        return { shown: getComputedStyle(n).display !== "none" && r.width === 52 && r.height > 0, spendLeft: document.getElementById("spend").getBoundingClientRect().left };
      });
      assert(onSpend.shown, "the rail is drawn on Token spend");
      assertEqual(onSpend.spendLeft, 52, "Token spend starts at the rail's right edge");
      assertEqual(await topId("#nr-chats"), "self", "a rail item is the topmost thing at its own center on Token spend");
      await page.click("#spend-back");
      await page.waitForSelector("#spend", { state: "hidden" });
      await page.click("#nr-scribe");
      await page.waitForSelector("#dictation-sheet", { state: "visible" });
      await flushFrames(page);
      await page.hover("#nr-finder");
      await page.waitForSelector("#nr-tip", { state: "visible" });
      // The label takes no pointer (`pointer-events: none`), so it cannot be hit-tested; its stacking can.
      const z = await page.evaluate(() => ["nr-tip", "dictation-sheet"].map((id) => parseInt(getComputedStyle(document.getElementById(id)).zIndex, 10)));
      assert(z[0] > z[1], `the hover label (z ${z[0]}) is above the RichScribe sheet (z ${z[1]})`);
      await page.click("#res-square", { button: "right" });
      await page.waitForSelector("#res-menu", { state: "visible" });
      await flushFrames(page);
      assertEqual(await topId('#res-menu [data-view="readout"]'), "self", "the Readout choice is above the RichScribe sheet");
      await page.click('#res-menu [data-view="readout"]');
      await flushFrames(page);
      assertEqual(await page.getAttribute('#res-menu [data-view="readout"]', "aria-checked"), "true", "the click on the choice took");
      await page.click("#res-square", { button: "right" });
      await page.waitForSelector("#res-menu", { state: "visible" });
      await page.click('#res-menu [data-view="ink"]');
      await page.keyboard.press("Escape");
      assertEqual(page.__errors, [], "no page errors");
      return "Token spend shows the rail; over RichScribe the label and the menu are on top and a choice was clicked";
    });

    await page.close();
    await context.close();
  }

  console.log("\n  measured:\n    " + report.join("\n    "));
  await browser.close();
  return run.report();
}

main().then((f) => { process.exitCode = f ? 1 : 0; }, (e) => { console.error(e && e.stack ? e.stack : e); process.exit(1); });

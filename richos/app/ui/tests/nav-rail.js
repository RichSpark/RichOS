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
//       RichPass opens its page under the
//       header, selected, the themed RichPass mark before its title, round 65.1 copy; Chats and a chat
//       chosen in the sidebar leave it; the page photographed in both themes
//   12  the rail stays on Token spend (review rv-20261106T173510Z-aa7dd4d6-bca3, finding 1), and with
//       RichScribe open its hover label and the activity square's Ink/Readout menu sit above the
//       sheet and a choice can be clicked (finding 2)
//
// Run: node nav-rail.js            (RICHOS_SHOTS_REGENERATE=shots-nav-rail to rewrite shots)

"use strict";

const fs = require("fs");
const path = require("path");
const vm = require("node:vm");
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
const ITEMS = ["home", "chats", "finder", "cleaner", "scribe", "pass", "bug"];

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

async function verifyRichFilesColdDock(sourcePath) {
  const trace = [];
  const frames = [];
  const page = { hidden: true };
  const nodes = {
    "nr-richfiles": { hidden: true },
    "richfiles-page": page,
    "richfiles-pane": { getBoundingClientRect: () => ({ left: 52, top: 40, right: 1000, bottom: 700, width: 948, height: 660 }) },
    "richfiles-pane-error": { hidden: true },
    "richfiles-loading": { hidden: true },
  };
  let hooks;
  let installListener;
  let receiveEvent;
  let pending = false;
  const window = {
    addEventListener() {},
    RichNavRail: {
      registerPage(_name, registeredHooks) { hooks = registeredHooks; },
      showPage(name) { trace.push("show " + name); hooks.show(); },
    },
    RichBridge: {
      async invoke(command, args) {
        switch (command) {
          case "richfiles_pane_available": return true;
          case "richfiles_request_pending":
            trace.push("poll pending=" + pending);
            return pending;
          case "richfiles_place_pane":
            trace.push("place visible=" + args.bounds.visible);
            return;
          case "richfiles_accept_request":
            trace.push("accept pending=" + pending);
            pending = false;
            return;
          default: throw new Error("Unexpected command " + command);
        }
      },
      listen(name, handler) {
        trace.push("subscribe " + name);
        return new Promise((resolve) => {
          installListener = () => {
            receiveEvent = handler;
            trace.push("listener installed");
            resolve(() => {});
          };
        });
      },
    },
  };
  class Observer {
    observe() {}
  }
  vm.runInNewContext(fs.readFileSync(sourcePath, "utf8"), {
    window, innerWidth: 1000, innerHeight: 700,
    document: { getElementById: (id) => nodes[id], body: { classList: { add() {} } } },
    ResizeObserver: Observer,
    MutationObserver: Observer,
    requestAnimationFrame(callback) { frames.push(callback); return frames.length; },
  }, { filename: sourcePath });

  // Drain queued promise callbacks with the subscription still unresolved.
  await new Promise(setImmediate);
  assertEqual(typeof installListener, "function", "the native listener is being installed");
  pending = true;
  trace.push("dock request arrived");
  receiveEvent?.({ payload: null });
  installListener();

  await new Promise(setImmediate);
  // The pane was not ready: the page draws its loading line (two frames) before it asks the shell.
  for (let round = 0; round < 4; round++) {
    for (const frame of frames.splice(0)) frame();
    await new Promise(setImmediate);
  }
  assertEqual(trace, [
    "subscribe rich://richfiles-show",
    "dock request arrived",
    "listener installed",
    "poll pending=true",
    "show richfiles",
    "place visible=true",
    "accept pending=true",
  ], "a dock request queued during listener installation opens the pane once");
  assertEqual(page.hidden, false, "the RichFiles page is visible after the cold dock");
  assertEqual(pending, false, "the queued dock request was consumed");
  return "listener installed before polling, queued workspace accepted in the visible pane";
}

/// `richfiles-page.js` alone, against a shell that records what it is asked and a clock the check
/// moves: `later(ms)` passes that much time and runs every waiting timer.
async function loadRichFilesPage(sourcePath, { available, readyFirst }) {
  const trace = [];
  const timers = [];
  const frames = [];
  const listeners = {};
  const screen = { opening: true, home: true, now: 5000 };
  let hooks;
  const nodes = {
    "nr-richfiles": { hidden: true },
    "richfiles-page": { hidden: true },
    "richfiles-pane": { getBoundingClientRect: () => ({ left: 52, top: 40, right: 1000, bottom: 700, width: 948, height: 660 }) },
    "richfiles-pane-error": { hidden: true },
    "richfiles-loading": { hidden: true },
  };
  const window = {
    addEventListener(type, handler) { listeners[type] = handler; },
    RichSplash: { isUp: () => screen.opening },
    RichHome: { isOpen: () => screen.home },
    RichNavRail: { registerPage(_name, registered) { hooks = registered; }, showPage(name) { trace.push("show " + name); } },
    RichBridge: {
      async invoke(command, args) {
        if (command === "richfiles_pane_available") return available;
        if (command === "richfiles_request_pending") return false;
        if (command === "richfiles_prepare_pane") { trace.push(args.pane === true ? "pane" : "library"); return; }
        if (command === "richfiles_place_pane") { trace.push("place visible=" + args.bounds.visible); return; }
        throw new Error("Unexpected command " + command);
      },
      async listen() { return () => {}; },
    },
  };
  if (readyFirst) window.__richosRichFilesReady = true;
  class Observer { observe() {} }
  vm.runInNewContext(fs.readFileSync(sourcePath, "utf8"), {
    window, innerWidth: 1000, innerHeight: 700,
    document: { getElementById: (id) => nodes[id], body: { classList: { add() {} } } },
    ResizeObserver: Observer, MutationObserver: Observer,
    performance: { now: () => screen.now },
    requestAnimationFrame(callback) { frames.push(callback); return frames.length; },
    setTimeout(callback) { timers.push(callback); return timers.length; },
  }, { filename: sourcePath });
  await new Promise(setImmediate);
  // Time passes, then every waiting timer runs.
  const later = async (ms) => {
    screen.now += ms;
    for (const timer of timers.splice(0)) timer();
    await new Promise(setImmediate);
  };
  const prepared = () => trace.filter((t) => t === "library" || t === "pane");
  // One frame is drawn: every waiting animation-frame callback runs.
  const frame = async () => {
    for (const callback of frames.splice(0)) callback();
    await new Promise(setImmediate);
  };
  const placed = () => trace.filter((t) => t.startsWith("place"));
  return { prepared, placed, frame, loading: nodes["richfiles-loading"], screen, later, usable: () => window.RichFilesReady.start(), input: (type) => listeners[type]?.(), show: () => hooks.show() };
}

/// RichFiles is made ready before the first click on its icon (the CEO, 2026-11-15: "whatever
/// indexing or loading needs to be done to make RichFiles appear instantly after nav rail button
/// click can and should be done in those few seconds that we are almost always guaranteed to
/// have before the user click on the RichFiles icon for the first time"). `richfiles-page.js`
/// alone, against a shell that records what it is asked and a clock the check moves:
///   the library is asked for once the app is usable and the opening screen has left, whatever
///   else is on screen;
///   the pane is asked for a moment after the home screen has left too;
///   neither is asked for after RichFiles has been shown, or in a build without RichFiles.
async function verifyRichFilesMadeReady(sourcePath) {
  const load = (options) => loadRichFilesPage(sourcePath, options);
  const first = await load({ available: true, readyFirst: false });
  assertEqual(first.prepared(), [], "nothing is prepared before the app is usable");
  first.usable();
  await first.later(3000);
  assertEqual(first.prepared(), [], "nothing is prepared while the opening screen is up");
  first.screen.opening = false;
  await first.later(100);
  assertEqual(first.prepared(), ["library"], "the library is asked for once the opening screen has left, on the home screen");
  await first.later(5000);
  assertEqual(first.prepared(), ["library"], "the pane is not built while the home screen is up: its picture moves");
  first.screen.home = false;
  await first.later(100);
  await first.later(100);
  assertEqual(first.prepared(), ["library"], "the pane is not built while the home screen is still fading out");
  await first.later(300);
  assertEqual(first.prepared(), ["library", "pane"], "the pane is built a moment after the home screen has left, with no click on the RichFiles icon");
  first.usable();
  await first.later(5000);
  assertEqual(first.prepared(), ["library", "pane"], "each is asked for once");

  // Back on the home screen before the moment has passed: it starts again when it is left again.
  const back = await load({ available: true, readyFirst: true });
  back.screen.opening = false;
  back.screen.home = false;
  await back.later(100);
  back.screen.home = true;
  await back.later(6000);
  assertEqual(back.prepared(), ["library"], "the pane is not built on a home screen that came back");
  back.screen.home = false;
  await back.later(100);
  await back.later(400);
  assertEqual(back.prepared(), ["library", "pane"], "and is built a moment after it is left again");

  // A click on the icon that comes first shows RichFiles as before; nothing is left to prepare.
  const shown = await load({ available: true, readyFirst: true });
  shown.screen.opening = false;
  shown.screen.home = false;
  shown.show();
  await shown.later(5000);
  assertEqual(shown.prepared(), [], "RichFiles that was shown first is not prepared afterwards");

  // A build without RichFiles has nothing to prepare.
  const none = await load({ available: false, readyFirst: true });
  none.screen.opening = false;
  none.screen.home = false;
  await none.later(5000);
  assertEqual(none.prepared(), [], "a build without RichFiles prepares nothing");
  return "the library once the opening screen has left; the pane a moment after the home screen has left, typing or not";
}

/// TYPING DOES NOT PUT IT OFF (review rv-20261115T224451Z-bcd92e58-536e): five seconds on the home
/// screen, then six seconds of typing in the chat that never pauses, then the click on the
/// RichFiles icon. The pane was built under the typing, so the click finds RichFiles prepared.
async function verifyRichFilesMadeReadyUnderTyping(sourcePath) {
  const load = (options) => loadRichFilesPage(sourcePath, options);
  const typing = await load({ available: true, readyFirst: true });
  typing.screen.opening = false;
  await typing.later(5000);
  typing.screen.home = false;
  for (let key = 0; key < 60; key++) {
    typing.input("keydown");
    await typing.later(100);
    if (key === 9) assertEqual(typing.prepared(), ["library", "pane"], "one second into typing that never pauses, the pane is built");
  }
  typing.show();
  assertEqual(typing.prepared(), ["library", "pane"], "after six seconds of typing the click finds RichFiles prepared, once");

  return "prepared one second into six seconds of typing that never pauses; the click found it ready";
}

/// A CLICK NEVER LOOKS DEAD (the CEO, 2026-11-15: "when technically not possible to eliminate the
/// wait, then at least show a basic FUCKING LOADING SCREEN!!!"). A click on the icon that comes
/// before RichFiles is ready puts the loading line on screen at once, and the shell, whose loading
/// and building holds the window still, is asked only after the line has been drawn. A click on
/// RichFiles that is ready shows no loading line and asks the shell at once.
async function verifyRichFilesEarlyClick(sourcePath) {
  const early = await loadRichFilesPage(sourcePath, { available: true, readyFirst: false });
  assertEqual([early.prepared(), early.loading.hidden], [[], true], "nothing is prepared yet and no loading line is up");
  early.show();
  assertEqual(early.loading.hidden, false, "the loading line is on screen with the click, before anything else happens");
  await early.frame();
  await early.frame();
  assertEqual(early.placed(), [], "the shell is not asked to load RichFiles before the loading line has been drawn");
  await early.frame();
  assertEqual(early.placed(), ["place visible=true"], "then the shell is asked");
  assertEqual(early.loading.hidden, true, "the loading line leaves when RichFiles is on screen");

  const ready = await loadRichFilesPage(sourcePath, { available: true, readyFirst: true });
  ready.screen.opening = false;
  ready.screen.home = false;
  await ready.later(100);
  await ready.later(400);
  assertEqual(ready.prepared(), ["library", "pane"], "RichFiles was made ready ahead of the click");
  ready.show();
  assertEqual(ready.loading.hidden, true, "RichFiles that is ready shows no loading line");
  await ready.frame();
  assertEqual(ready.placed(), ["place visible=true"], "and the shell is asked in the click's own frame");
  return "an early click: the loading line at once, the shell asked two frames later; a click on ready RichFiles: no loading line";
}

/// The same in the real shell: `main.js` tells the RichFiles page when the app is usable; on the
/// home screen the page asks the shell for the library only, and for the pane once the home screen
/// has been left, with no click on the RichFiles icon.
async function verifyRichFilesMadeReadyInShell(browser) {
  const context = await newContext(browser, "dark", {});
  const page = await context.newPage();
  try {
    await page.addInitScript(() => {
      window.__richFilesAsked = [];
      Object.defineProperty(window, "RichBridge", {
        configurable: true,
        set(bridge) {
          const invoke = bridge.invoke.bind(bridge);
          bridge.invoke = async (command, args, options) => {
            if (command === "richfiles_pane_available") return true;
            if (command === "richfiles_request_pending") return false;
            if (command === "richfiles_prepare_pane" || command === "richfiles_place_pane") {
              window.__richFilesAsked.push({
                command, pane: args && typeof args.pane === "boolean" ? args.pane : null,
                visible: args && args.bounds ? args.bounds.visible : null,
                openingUp: !!(window.RichSplash && window.RichSplash.isUp()),
                homeUp: !!(window.RichHome && window.RichHome.isOpen()),
                pageHidden: document.getElementById("richfiles-page").hidden,
              });
              return;
            }
            return invoke(command, args, options);
          };
          Object.defineProperty(window, "RichBridge", { value: bridge, writable: true, configurable: true });
        },
      });
    });
    await page.goto(APP);
    await shellSettled(page);
    await page.waitForFunction(() => window.RichHome.isOpen() && window.__richFilesAsked.some((a) => a.command === "richfiles_prepare_pane"),
      null, { timeout: 30000 });
    // Many times longer than the pane waits once the home screen has left.
    // load-bound: an absence. Under any load the pane is not asked for on the home screen, so a slower host cannot fail this; the rule itself is proved on a clock the check above controls.
    await page.waitForTimeout(4000);
    const onHome = await page.evaluate(() => window.__richFilesAsked);
    assertEqual(onHome, [{ command: "richfiles_prepare_pane", pane: false, visible: null, openingUp: false, homeUp: true, pageHidden: true }],
      "on the home screen the library is asked for, once, and the pane is not");
    await leaveHome(page);
    await page.waitForFunction(() => window.__richFilesAsked.some((a) => a.pane === true), null, { timeout: 30000 });
    const asked = await page.evaluate(() => window.__richFilesAsked);
    assertEqual(asked.filter((a) => a.pane === true),
      [{ command: "richfiles_prepare_pane", pane: true, visible: null, openingUp: false, homeUp: false, pageHidden: true }],
      "the pane is asked for once, with the opening and home screens gone and the RichFiles page still hidden");
    assertEqual(asked.filter((a) => a.visible === true), [], "nothing showed RichFiles: its icon was not clicked");
    return "the library asked for on the home screen, the pane once after it was left, the RichFiles page hidden";
  } finally {
    await context.close();
  }
}

async function main() {
  const run = createRun("The navigation rail (round 58, version A): items, selection, the moved square and initials, contrast");
  await run.check("RichFiles accepts a cold dock request queued while its listener starts", () =>
    verifyRichFilesColdDock(path.join(UI_DIR, "richfiles-page.js")));
  await run.check("RichFiles is made ready before the first click: a moment after the home screen has left", () =>
    verifyRichFilesMadeReady(path.join(UI_DIR, "richfiles-page.js")));
  await run.check("RichFiles: a click before it is ready shows the loading line at once, and the shell is asked after it is drawn", () =>
    verifyRichFilesEarlyClick(path.join(UI_DIR, "richfiles-page.js")));
  await run.check("RichFiles is made ready under typing that never pauses: six seconds of it, then the click", () =>
    verifyRichFilesMadeReadyUnderTyping(path.join(UI_DIR, "richfiles-page.js")));
  const { webkit } = loadPlaywright();
  const browser = await webkit.launch();
  const report = [];
  await run.check("RichFiles is made ready before the first click, in the real shell", () => verifyRichFilesMadeReadyInShell(browser));

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

    await run.check(`RichFiles ${theme}  the heading and native placement error pass contrast`, async () => {
      const richFiles = await context.newPage();
      try {
        const placementError = "RichFiles could not appear in this window.";
        await richFiles.addInitScript((message) => {
          Object.defineProperty(window, "RichBridge", {
            configurable: true,
            set(bridge) {
              const invoke = bridge.invoke.bind(bridge);
              bridge.invoke = async (command, args, options) => {
                if (command === "richfiles_pane_available") return true;
                if (command === "richfiles_request_pending") return false;
                if (command === "richfiles_place_pane") {
                  if (args.bounds.visible) throw message;
                  return;
                }
                return invoke(command, args, options);
              };
              Object.defineProperty(window, "RichBridge", { value: bridge, writable: true, configurable: true });
            },
          });
        }, placementError);
        await richFiles.goto(APP);
        await leaveHome(richFiles);
        await shellSettled(richFiles);
        await richFiles.locator("#nr-richfiles").click();
        await richFiles.locator("#richfiles-title").waitFor({ state: "visible" });
        await richFiles.locator("#richfiles-pane-error").waitFor({ state: "visible" });
        await richFiles.mouse.move(700, 80);
        await flushFrames(richFiles);
        assertEqual((await railFacts(richFiles)).selected, ["richfiles"], "the rail selected the RichFiles page");
        assertEqual(await richFiles.locator("#richfiles-title").innerText(), "RichFiles", "the actual heading is visible");
        assertEqual(await richFiles.locator("#richfiles-pane-error").innerText(), placementError,
          "the normal native placement error is visible");
        assertEqual(await richFiles.evaluate(() => document.documentElement.dataset.theme), theme,
          "the document is using the requested theme");
        await richFiles.evaluate(C.pageScript());
        const walk = await richFiles.evaluate((t) => window.__contrastProbe({ surface: "richfiles-page", theme: t }), theme);
        for (const target of ["h1#richfiles-title", "p#richfiles-pane-error"]) {
          assert(walk.measuredPaths.includes(target), "the contrast probe did not measure " + target);
        }
        const mine = (pairs) => Object.values(pairs).filter((pair) =>
          /#richfiles-(page|title|pane-error)/.test(pair.selector || pair.path || ""));
        assertEqual(mine(walk.failures), [], "the contrast walk found a failing pair on the RichFiles page");
        assertEqual(mine(walk.unresolvable), [], "the contrast walk could not prove a pair on the RichFiles page");
        assertEqual(mine(walk.exempt), [], "the RichFiles page needs no contrast exemptions");
        return "the shipped RichFiles heading and placement error were both measured";
      } finally {
        await richFiles.close();
      }
    });

    await run.check(`RichFiles ${theme}  a click before it is ready: the loading line is on the page, and it passes contrast`, async () => {
      const richFiles = await context.newPage();
      try {
        await richFiles.addInitScript(() => {
          window.__richFilesPlaced = [];
          Object.defineProperty(window, "RichBridge", {
            configurable: true,
            set(bridge) {
              const invoke = bridge.invoke.bind(bridge);
              bridge.invoke = async (command, args, options) => {
                if (command === "richfiles_pane_available") return true;
                if (command === "richfiles_request_pending") return false;
                // RichFiles is never ready here, and never arrives: the loading line stays to be looked at.
                if (command === "richfiles_prepare_pane") return new Promise(() => {});
                if (command === "richfiles_place_pane") {
                  if (!args.bounds.visible) return;
                  window.__richFilesPlaced.push({ loadingUp: !document.getElementById("richfiles-loading").hidden });
                  return new Promise(() => {});
                }
                return invoke(command, args, options);
              };
              Object.defineProperty(window, "RichBridge", { value: bridge, writable: true, configurable: true });
            },
          });
        });
        await richFiles.goto(APP);
        await leaveHome(richFiles);
        await shellSettled(richFiles);
        await richFiles.locator("#nr-richfiles").click();
        await richFiles.locator("#richfiles-loading").waitFor({ state: "visible" });
        await richFiles.waitForFunction(() => window.__richFilesPlaced.length > 0, null, { timeout: 30000 });
        await richFiles.mouse.move(700, 80);
        await flushFrames(richFiles);
        assertEqual(await richFiles.evaluate(() => window.__richFilesPlaced), [{ loadingUp: true }],
          "the shell was asked for RichFiles once, with the loading line already up");
        assertEqual((await railFacts(richFiles)).selected, ["richfiles"], "the rail selected the RichFiles page");
        assertEqual(await richFiles.locator("#richfiles-loading").innerText(), "Opening RichFiles…", "the loading line says what is happening");
        assertEqual(await richFiles.evaluate(() => document.documentElement.dataset.theme), theme,
          "the document is using the requested theme");
        await richFiles.evaluate(C.pageScript());
        const walk = await richFiles.evaluate((t) => window.__contrastProbe({ surface: "richfiles-page", theme: t }), theme);
        assert(walk.measuredPaths.includes("p#richfiles-loading"), "the contrast probe did not measure p#richfiles-loading");
        const mine = (pairs) => Object.values(pairs).filter((pair) => /#richfiles-loading/.test(pair.selector || pair.path || ""));
        assertEqual(mine(walk.failures), [], "the contrast walk found a failing pair on the loading line");
        assertEqual(mine(walk.unresolvable), [], "the contrast walk could not prove a pair on the loading line");
        assertEqual(mine(walk.exempt), [], "the loading line needs no contrast exemption");
        return "the loading line was up before the shell was asked, and its text was measured";
      } finally {
        await richFiles.close();
      }
    });

    await run.check(`RichPass ${theme}  opens its page under the header, selected, its icon before its title; Chats and a chat in the sidebar leave it`, async () => {
      await page.click("#nr-pass");
      await page.waitForSelector("#richpass-page", { state: "visible" });
      await flushFrames(page);
      const f = await railFacts(page);
      assertEqual(f.selected, ["pass"], "RichPass is selected while its page is shown");
      assert(f.barBesideSelected, "the bar slid to RichPass");
      const p = await page.evaluate(() => {
        const ttl = document.querySelector("#richpass-page .rp-ttl");
        const mark = ttl.querySelector(".rp-mark");
        const img = [...mark.querySelectorAll("img")].find((node) => node.getClientRects().length);
        const body = document.querySelector("#richpass-page .rp-body");
        return {
          icon: img?.getAttribute("src") || "",
          iconBeforeTitle: mark.nextElementSibling.id === "rp-title",
          bodyFormatting: [body.querySelector("i").textContent, body.querySelector("b").textContent, body.querySelectorAll("br").length],
          facts: [...document.querySelectorAll("#richpass-page .rp-fact b")].map((node) => node.textContent),
          caption: document.getElementById("rp-download-cap").textContent,
          protection: document.getElementById("rp-prot-t").textContent,
          warning: !!document.querySelector("#richpass-page .rp-warn"),
          title: document.getElementById("rp-title").textContent,
          lede: document.querySelector("#richpass-page .rp-lede").textContent,
          crumb: document.getElementById("page-crumb").textContent,
          conversation: getComputedStyle(document.getElementById("conversation")).display,
          composer: getComputedStyle(document.getElementById("composer-zone")).display,
          smallest: Math.min(...Array.from(document.querySelectorAll("#richpass-page p, #richpass-page span, #richpass-page b, #richpass-page button")).filter((n) => n.getClientRects().length && n.textContent.trim()).map((n) => parseFloat(getComputedStyle(n).fontSize))),
        };
      });
      assert(p.iconBeforeTitle && p.icon.endsWith(`richpass-mark-${theme}.svg`), "the matching themed mark is in front of the title");
      assertEqual(p.bodyFormatting, ["without saving any", "never saves generated passwords!", 4], "the approved emphasis and line breaks");
      assertEqual(p.facts, ["Works offline, in your browser", "Ready when you return", "Re-generate instead of saving"], "the three approved facts");
      assertEqual([p.caption, p.protection, p.warning], ["RichPass.html · free · open-source · one file", "Enable protection", false], "approved caption and protection block");
      await page.locator("#rp-more").press("Enter");
      const sourceFolder = "github.com/RichSpark/RichOS/tree/main/richos/tools/richpass";
      await page.waitForFunction((target) => window.__RICHOS_OPENED__?.at(-1) === target, sourceFolder);
      assertEqual(await page.locator("#rp-more").getAttribute("title"), sourceFolder, "the disclosed and opened destination match");
      await page.click("#rp-download");
      await page.waitForFunction(() => document.getElementById("rp-error").textContent.includes("File selection and saving require the installed RichOS app."));
      await page.locator("#rp-more").press("Enter");
      await page.waitForFunction(() => document.getElementById("rp-error").hidden);
      await page.locator("#rp-title").focus();
      await page.mouse.move(700, 80);
      assertEqual([p.title, p.crumb], ["RichPass", "RichPass"], "the title and the header name the page");
      assertEqual(p.lede, "Safer Than Any Password Manager", "the approved lede");
      assertEqual([p.conversation, p.composer], ["none", "none"], "the page takes the conversation's place");
      assert(p.smallest >= 16, "every text on the page is 16px or more: " + p.smallest);
      // The repository's own walk: nothing on the page fails or is unprovable.
      await page.evaluate(C.pageScript());
      const walk = await page.evaluate((t) => window.__contrastProbe({ surface: "richpass-page", theme: t }), theme);
      const mine = (o) => Object.values(o || {}).filter((x) => /rp-|richpass-page/.test(x.selector || x.path || ""));
      assertEqual(mine(walk.failures), [], "the contrast walk found a failing pair on the RichPass page");
      assertEqual(mine(walk.unresolvable), [], "the contrast walk could not prove a pair on the RichPass page");
      const buf = await captureSettled(page, { parkPointer: true });
      fs.mkdirSync(SHOTS, { recursive: true });
      publishShot(buf, path.join(SHOTS, `richpass-page-${theme}.png`));
      await page.click("#nr-chats");
      await page.waitForSelector("#richpass-page", { state: "hidden" });
      assertEqual((await railFacts(page)).selected, ["chats"], "Chats leaves the page");
      await page.click("#nr-pass");
      await page.waitForSelector("#richpass-page", { state: "visible" });
      await page.click("#rail-nav .nav-thread");
      await page.waitForSelector("#richpass-page", { state: "hidden" });
      await flushFrames(page);
      assertEqual((await railFacts(page)).selected, ["chats"], "a chat chosen in the sidebar leaves the page");
      return `page with themed mark and round 65.1 copy, smallest text ${p.smallest}px; Chats and a sidebar chat leave it; shots-nav-rail/richpass-page-${theme}.png`;
    });

    // Photograph the established chat before case 13 creates its temporary new chat.
    await run.check(`11 ${theme}  the rail photographed (shots-nav-rail/)`, async () => {
      await page.mouse.move(700, 450);
      const buf = await captureSettled(page, { screenshot: { clip: { x: 0, y: 0, width: 420, height: 900 } } });
      fs.mkdirSync(SHOTS, { recursive: true });
      publishShot(buf, path.join(SHOTS, `rail-${theme}.png`));
      assertEqual(page.__errors, [], "no page errors");
      return `shots-nav-rail/rail-${theme}.png`;
    });

    await run.check(`RichPass ${theme}  from the page the keyboard leaves it too: Enter on a RichFinder chat result, and Command+N`, async () => {
      const onPage = async () => {
        await page.click("#nr-pass");
        await page.waitForSelector("#richpass-page", { state: "visible" });
      };
      await onPage();
      await page.keyboard.press("Meta+k");
      await page.waitForSelector("#search-overlay", { state: "visible" });
      await page.fill("#search-input", "a");
      await page.waitForSelector("#search-results .srch-hit", { state: "visible" });
      await page.keyboard.press("Enter");
      await page.waitForSelector("#richpass-page", { state: "hidden" });
      await flushFrames(page);
      assertEqual((await railFacts(page)).selected, ["chats"], "Enter on a chat result leaves the page");
      await onPage();
      await page.evaluate(() => document.activeElement && document.activeElement.blur());
      await page.keyboard.press("Meta+n");
      // With several companies Command+N opens the menu with its first company focused, and
      // Enter on that is the chat; with one company it is the chat at once.
      if (await page.evaluate(() => !document.getElementById("new-chat-menu").hidden)) await page.keyboard.press("Enter");
      await page.waitForSelector("#richpass-page", { state: "hidden" });
      await flushFrames(page);
      assertEqual((await railFacts(page)).selected, ["chats"], "Command+N leaves the page");
      await page.keyboard.press("Escape");
      return "Enter on a RichFinder chat result and Command+N both leave the RichPass page";
    });

    await run.check(`RichPass ${theme}  a past repair does not hide a current protection failure`, async () => {
      await page.click("#nr-pass");
      await page.waitForSelector("#richpass-page", { state: "visible" });
      await flushFrames(page);
      const repaired = {
        phase: "verified",
        path: "/fictional/RichPass.html",
        notice: { kind: "contents", at: 100 },
      };
      const emitStatus = (view) => page.evaluate((status) => window.__RICHOS_MOCK__.richpassChanged(status), view);
      await emitStatus(repaired);
      assertEqual(await page.locator("#rp-state-title").innerText(), "Restored", "a verified repair is shown");
      const message = "Allow folder access, then Check now.";
      await emitStatus({ ...repaired, phase: "blocked", message });
      assertEqual(await page.locator("#rp-state-line").innerText(), message, "the current failure is shown");
      const headline = await page.locator("#rp-state-title").innerText();
      await page.click("#nr-chats");
      await page.waitForSelector("#richpass-page", { state: "hidden" });
      assertEqual(headline, "Protection needs attention", "the current phase controls the headline");
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
      // The other way in: from Home (Settings is over Home too). Home steps aside, the rail is there and
      // takes a press; Chats leaves Token spend and does not bring Home back.
      await page.evaluate(() => window.RichHome.show("nav-rail-test"));
      await page.waitForFunction(() => window.RichHome.isOpen());
      await page.click("#set-btn");
      await page.waitForSelector("#set-menu #set-spend-open");
      await page.click("#set-spend-open");
      await page.waitForSelector("#spend-fig .ch");
      await flushFrames(page);
      assert(!(await page.evaluate(() => window.RichHome.isOpen())), "Home steps aside while Token spend is up");
      await page.waitForFunction(() => { const r = document.getElementById("nr-chats").getBoundingClientRect(); const e = document.elementFromPoint(r.left + r.width / 2, r.top + r.height / 2); return !!e && !!e.closest("#nr-chats"); });
      assertEqual(await topId("#nr-chats"), "self", "from Home too, a rail item is the topmost thing at its own center on Token spend");
      await page.click("#nr-chats");
      await page.waitForSelector("#spend", { state: "hidden" });
      assert(!(await page.evaluate(() => window.RichHome.isOpen())), "a rail press leaves Token spend without bringing Home back");
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
      await page.waitForSelector("#dictation-sheet", { state: "hidden" });
      // A card pinned from the keyboard does not stay over Home (Home is under the card's z-index now).
      await page.focus("#res-square");
      await page.keyboard.press("Space");
      await page.waitForSelector("#res-card", { state: "visible" });
      await page.evaluate(() => document.getElementById("nr-home").click());
      await page.waitForFunction(() => window.RichHome.isOpen());
      assertEqual(await page.evaluate(() => getComputedStyle(document.getElementById("res-card")).display), "none", "the pinned activity card is hidden once Home is up");
      assertEqual(page.__errors, [], "no page errors");
      return "Token spend shows the rail; over RichScribe the label and the menu are on top and a choice was clicked";
    });

    await page.close();
    await context.close();
  }

  // 13: the first click after RichCleaner's window has been opened and closed. macOS gives the
  // click that makes an inactive window key to the window and not to the page unless the window
  // accepts the first mouse; the main window did not, so a rail item or Settings needed two
  // clicks. Every window the main window is built from is
  // `tauri.conf.json`'s declaration (`from_config`, both the first window and the Dock reopen).
  await run.check("13  the main window takes the first click while the app is not in front (RichCleaner opened and closed)", async () => {
    const conf = JSON.parse(fs.readFileSync(path.resolve(UI_DIR, "..", "src-tauri", "tauri.conf.json"), "utf8"));
    const wins = conf.app.windows;
    assert(wins.length > 0, "tauri.conf.json declares the main window");
    for (const w of wins) assertEqual(w.acceptFirstMouse, true, `window "${w.title}" declares acceptFirstMouse: true`);
    return "the main window accepts the first mouse";
  });

  console.log("\n  measured:\n    " + report.join("\n    "));
  await browser.close();
  return run.report();
}

main().then((f) => { process.exitCode = f ? 1 : 0; }, (e) => { console.error(e && e.stack ? e.stack : e); process.exit(1); });

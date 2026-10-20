// THE DICTATION BAR, THE WORDS' FLIGHT AND THE MENU BAR MENU (dictation plan rev 2, slice 3).
//
// The dictation tool's three windows load `dictation-bar.html` (the bar; `?role=flight` the
// flight) and `dictation-menu.html`. This suite loads both pages in WebKit with a stand-in for
// the Tauri bridge that records every call the page makes and lets the suite say what the tool
// says, so the wiring is the real wiring:
//
//   1. Every state the bar draws, word for word against round 19 (`dictation.html`
//      `renderPill`/`PROBLEMS`) and Iris's two lines (`dictation-more-lines.html`, 5a and 5b),
//      Fix it on exactly the microphone and Accessibility bars, the key named as chosen.
//   2. The page tells the tool the window it needs, the orb's center and Fix it's box, and a
//      press on Fix it reaches the tool.
//   3. The meter moves with the tool's level and rests at its floor.
//   4. The flight: comets from the orb, then the light at the words' rectangle; with reduced
//      motion, no comets, the light alone.
//   5. The menu, word for word (round 19 `renderSb`, Iris's line 4 named and unnamed), every
//      row reaching the tool, Escape closing it, the arrows moving between rows.
//   6. CONTRAST, computed in both themes: every line on the bar and in the menu (the bar floats
//      over any app, so its translucent ground is composited over white AND over black and the
//      lower ratio is the one judged), every indicator, and the light drawn OVER another app's
//      words (plan section 5, point 6) over TextEdit, Notes and RichOS's own message box, in
//      both RichOS themes and both macOS appearances.
//   7. Every line meant to be read is 16px or larger (CEO §15).
//
// Run: node dictation-overlay.js

"use strict";

const path = require("path");
const { loadPlaywright, createRun, assert, assertEqual, UI_DIR } = require("./lib/harness");
const C = require("./lib/contrast");

const BAR = "file://" + path.join(UI_DIR, "dictation-bar.html");
const FLIGHT = BAR + "?role=flight";
const MENU = "file://" + path.join(UI_DIR, "dictation-menu.html");
const INDEX = "file://" + path.join(UI_DIR, "index.html");

/// The Tauri bridge, as far as these pages use it: `core.invoke` recorded, `event.listen`
/// kept so the suite can speak for the tool.
const BRIDGE = `
  window.__calls = [];
  window.__listeners = {};
  window.__TAURI__ = {
    core: { invoke: (cmd, args) => { window.__calls.push([cmd, args || {}]); return Promise.resolve(); } },
    event: { listen: (name, fn) => { window.__listeners[name] = fn; return Promise.resolve(() => {}); } },
  };
  window.__say = (name, payload) => window.__listeners[name]({ payload });
`;

/// Round 19's lines and Iris's, word for word, as the bar's text reads them with F1.
const LINES = {
  "did-not-catch": "I didn't catch anything. Tap F1 and talk.",
  "no-sound": "I can't hear anything. Check that your microphone is on.",
  "no-text-box": "No text box was selected, so I copied your words. Press ⌘V to paste them.",
  "no-microphone": "I need the microphone to hear you.",
  "no-accessibility": "I can't type into other apps yet, so I copied your words.",
  "model-missing": "I'm still downloading the voice AI. Try again in a few minutes.",
  "could-not-write": "Sorry, I couldn't write that down. Tap F1 and say it again.",
  "voice-still-listening": "Voice mode is still listening, so I didn't start. Tap F1 again in a moment.",
};

async function open(browser, url, theme, reduced) {
  const page = await browser.newPage({ viewport: { width: 900, height: 400 } });
  page.__errors = [];
  page.on("pageerror", (e) => page.__errors.push(String(e)));
  page.on("console", (m) => { if (m.type() === "error") page.__errors.push("console: " + m.text()); });
  if (reduced) await page.emulateMedia({ reducedMotion: "reduce" });
  await page.addInitScript(BRIDGE);
  await page.addInitScript((t) => { try { localStorage.setItem("richos-theme", t); } catch (e) { /* none */ } }, theme || "dark");
  await page.goto(url);
  await page.waitForFunction("window.__calls && window.__calls.some(c => c[0] === 'dictation_page_ready')");
  return page;
}

function bar(view, extra) {
  return Object.assign({ seq: 7, view, problem: null, key: "F1", startedMs: null, theme: "dark", fontScale: 100 }, extra || {});
}

async function say(page, name, payload) {
  await page.evaluate(([n, p]) => window.__say(n, p), [name, payload]);
  await page.evaluate(() => new Promise((r) => requestAnimationFrame(() => requestAnimationFrame(r))));
}

async function text(page, sel) {
  return (await page.locator(sel).innerText()).replace(/\s+/g, " ").trim();
}

/// THE CONTRAST WALK for a page whose own ground is transparent: every element with its own
/// text, its color composited over the first ancestor that paints, that chain composited over
/// `ground`. Returns [{text, px, weight, ratio}] for every text run, and the indicator pairs
/// the caller names.
const WALK = `
  window.__walk = (ground, indicators) => {
    const M = window.__contrastMath;
    const parse = (s) => M.parseCssColor(s);
    const behind = (el) => {
      const layers = [];
      for (let e = el; e && e.nodeType === 1; e = e.parentElement) {
        const c = parse(getComputedStyle(e).backgroundColor);
        if (c && c.a > 0) layers.push(c);
        if (c && c.a === 1) break;
      }
      let painted = ground;
      for (let i = layers.length - 1; i >= 0; i--) painted = M.compositeOver(layers[i], painted);
      return painted;
    };
    const out = { text: [], indicators: [] };
    for (const el of document.querySelectorAll("body *")) {
      const own = Array.from(el.childNodes).some((n) => n.nodeType === 3 && n.textContent.trim());
      const r = el.getBoundingClientRect();
      if (!own || r.width === 0 || r.height === 0) continue;
      const s = getComputedStyle(el);
      const bg = behind(el);
      const fg = M.compositeOver(parse(s.color), bg);
      out.text.push({ text: el.textContent.trim().slice(0, 60), px: parseFloat(s.fontSize), weight: s.fontWeight,
        ratio: M.round2(M.contrastRatio(fg, bg)) });
    }
    for (const [name, sel, prop, against] of indicators) {
      const el = document.querySelector(sel);
      if (!el) { out.indicators.push({ name, missing: true }); continue; }
      const s = getComputedStyle(el);
      const bg = behind(document.querySelector(against));
      const fg = M.compositeOver(parse(s[prop]), bg);
      out.indicators.push({ name, ratio: M.round2(M.contrastRatio(fg, bg)) });
    }
    return out;
  };
`;

async function walk(page, ground, indicators) {
  await page.addScriptTag({ content: C.pageScript() });
  await page.addScriptTag({ content: WALK });
  return page.evaluate(([g, i]) => window.__walk(g, i), [ground, indicators || []]);
}

const WHITE = { r: 255, g: 255, b: 255, a: 1 };
const BLACK = { r: 0, g: 0, b: 0, a: 1 };

async function main() {
  const run = createRun("the dictation bar, the words' flight and the menu bar menu");
  const browser = await loadPlaywright().webkit.launch();

  await run.check("every state the bar draws, word for word, with Fix it only where round 19 puts it", async () => {
    const page = await open(browser, BAR);
    await say(page, "dictation-bar", bar("listening", { startedMs: Date.now() - 3200 }));
    assertEqual(await page.locator(".dh.is-listen .dh-meter i").count(), 9, "the meter's nine bars");
    const listening = await text(page, "#dhud");
    assert(/^Listening 0:0[3-4] Tap F1 to finish$/.test(listening), "listening reads: " + listening);
    await say(page, "dictation-bar", bar("writing"));
    assertEqual(await text(page, "#dhud"), "Writing it down…", "writing");
    await say(page, "dictation-bar", bar("added"));
    assertEqual(await text(page, "#dhud"), "Added", "added");
    const seen = [];
    for (const [tag, line] of Object.entries(LINES)) {
      await say(page, "dictation-bar", bar("problem", { problem: tag }));
      const fix = await page.locator("#fixIt").count();
      const want = tag === "no-microphone" || tag === "no-accessibility";
      assertEqual(fix, want ? 1 : 0, tag + ": Fix it");
      assertEqual(await text(page, "#dhud .dh-label"), line, tag);
      if (want) assertEqual(await text(page, "#fixIt"), "Fix it", tag + ": the button's words");
      seen.push(tag);
    }
    await say(page, "dictation-bar", bar("problem", { problem: "model-missing" }));
    assertEqual(await page.locator('.dh-orb path[d="M12 4v11"]').count(), 1, "line 5a carries the download arrow (Iris)");
    await say(page, "dictation-bar", bar("problem", { problem: "did-not-catch", key: "F5" }));
    assertEqual(await text(page, "#dhud .dh-label"), "I didn't catch anything. Tap F5 and talk.", "the chosen key is named");
    await say(page, "dictation-bar", bar("off"));
    assertEqual(await text(page, "#dhud"), "RichScribe is off.", "the menu's off notice");
    await say(page, "dictation-bar", bar("hidden"));
    assertEqual(await page.locator("#dhud > *").count(), 0, "hidden draws nothing");
    assertEqual(page.__errors.length, 0, page.__errors.join(" | "));
    await page.close();
    return "listening, writing, added, off and " + seen.length + " problem lines, each word for word";
  });

  await run.check("the page reports the window it needs, the orb and Fix it, and Fix it reaches the tool", async () => {
    const page = await open(browser, BAR);
    await say(page, "dictation-bar", bar("problem", { problem: "no-accessibility", seq: 11 }));
    const calls = await page.evaluate(() => window.__calls.filter((c) => c[0] === "dictation_bar_laid_out"));
    assert(calls.length >= 1, "no layout was reported");
    const l = calls[calls.length - 1][1];
    assertEqual(l.seq, 11, "the layout names the view it measured");
    // Measured by the suite once the rise has ended: the page reported its size AT the first
    // frame, mid-rise, so the two agree only if the page measured layout and not the screen.
    await page.waitForFunction(() => document.querySelector(".dh").getAnimations().every((a) => a.playState === "finished"));
    const pill = await page.locator(".dh").boundingBox();
    // Within a pixel: layout widths are whole numbers, the drawn box is fractional.
    assert(Math.abs(l.w - (pill.width + 80)) <= 1, `width: the pill and 40px each side for its shadow: ${l.w} for a ${pill.width}px pill`);
    assert(Math.abs(l.h - (pill.height + 24 + 56)) <= 1, `height: the pill, 24px above, 56px below: ${l.h} for a ${pill.height}px pill`);
    assert(l.orbX > 40 && l.orbX < 40 + 60 && Math.abs(l.orbY - (24 + pill.height / 2)) < 1, "the orb's center: " + JSON.stringify(l));
    const fix = await page.locator("#fixIt").boundingBox();
    const near = (a, b) => Math.abs(a - b) <= 1;
    assert(near(l.fix.x, fix.x) && near(l.fix.y, fix.y) && near(l.fix.w, fix.width) && near(l.fix.h, fix.height),
      "Fix it's box: reported " + JSON.stringify(l.fix) + ", drawn " + JSON.stringify(fix));
    await page.locator("#fixIt").click();
    const pressed = await page.evaluate(() => window.__calls.filter((c) => c[0] === "dictation_fix_it").length);
    assertEqual(pressed, 1, "Fix it reached the tool");
    await say(page, "dictation-bar", bar("added", { seq: 12 }));
    const last = await page.evaluate(() => window.__calls.filter((c) => c[0] === "dictation_bar_laid_out").pop()[1]);
    assertEqual(last.fix, null, "no Fix it, no box");
    await page.close();
    return `window ${l.w}x${l.h}, orb at ${l.orbX.toFixed(1)},${l.orbY.toFixed(1)}, Fix it at ${fix.x.toFixed(0)},${fix.y.toFixed(0)}`;
  });

  await run.check("a hidden window still reports its layout: no animation frame is needed (walk-d25aed81eaa1)", async () => {
    // The tool shows the bar's and the menu's windows only once their page has reported the
    // size it needs, and WebKit runs no animation frame in a window that is not on screen. A
    // report made from requestAnimationFrame therefore never came, and the bar never appeared
    // (walk-d25aed81eaa1, check-window: "never said bar shown"). Frames are frozen here, as in a
    // hidden window, and the report must still arrive.
    const freeze = () => { window.requestAnimationFrame = () => 0; };
    const page = await open(browser, BAR);
    await page.evaluate(freeze);
    await page.evaluate(([n, p]) => window.__say(n, p), ["dictation-bar", bar("problem", { problem: "no-microphone", seq: 21 })]);
    const bars = await page.evaluate(() => window.__calls.filter((c) => c[0] === "dictation_bar_laid_out").map((c) => c[1].seq));
    assert(bars.includes(21), "the bar reported nothing without an animation frame: " + JSON.stringify(bars));
    await page.close();
    const menu = await open(browser, MENU);
    await menu.evaluate(freeze);
    await menu.evaluate(([n, p]) => window.__say(n, p), ["dictation-menu", { seq: 22, on: true, key: "F1", choice: "accurate", paused: false, secureApp: null, theme: "dark", fontScale: 100 }]);
    const menus = await menu.evaluate(() => window.__calls.filter((c) => c[0] === "dictation_menu_laid_out").map((c) => c[1].seq));
    assert(menus.includes(22), "the menu reported nothing without an animation frame: " + JSON.stringify(menus));
    await menu.close();
    return "with animation frames frozen, the bar reported view 21 and the menu view 22 at once";
  });

  await run.check("the meter follows the tool's level and rests at its floor", async () => {
    const page = await open(browser, BAR);
    await say(page, "dictation-bar", bar("listening", { startedMs: Date.now() }));
    const scale = () => page.evaluate(() => Array.from(document.querySelectorAll(".dh-meter i")).map((i) => new DOMMatrix(getComputedStyle(i).transform).d));
    const middle = () => new DOMMatrix(getComputedStyle(document.querySelectorAll(".dh-meter i")[4]).transform).d;
    await say(page, "dictation-level", { level: 1 });
    await page.waitForFunction(`(${middle})() > 0.8`);
    const loud = await scale();
    assert(loud[4] > 0.8, "the middle bar at full level: " + loud[4]);
    await say(page, "dictation-level", { level: 0 });
    await page.waitForFunction(() => Array.from(document.querySelectorAll(".dh-meter i")).every((i) => new DOMMatrix(getComputedStyle(i).transform).d < 0.2));
    const quiet = await scale();
    assert(quiet.every((v) => v < 0.2), "the bars at rest: " + quiet.map((v) => v.toFixed(2)).join(","));
    await page.close();
    return `middle bar ${loud[4].toFixed(2)} at full level, ${Math.max(...quiet).toFixed(2)} at rest`;
  });

  await run.check("the words fly from the orb and are lit where they landed; with reduced motion, lit only", async () => {
    const page = await open(browser, FLIGHT);
    const to = { x: 300, y: 120, w: 180, h: 20 };
    // The dots are made in the same call that starts the flight; the light waits for them to land.
    await page.evaluate((t) => { window.__fly = window.__say("dictation-flight", { from: [450, 360], to: t, theme: "dark", fontScale: 100 }); }, to);
    const comets = await page.locator(".dov-comet").count();
    assertEqual(comets, 16, "the comet's sixteen dots");
    assertEqual(await page.locator(".dov-light").count(), 0, "the light waits for the words to land");
    await page.waitForFunction(() => document.querySelectorAll(".dov-light").length === 1 && document.querySelectorAll(".dov-comet").length === 0);
    const light = await page.locator(".dov-light").boundingBox();
    assert(light && Math.abs(light.x - (to.x - 2)) < 0.5 && Math.abs(light.y - to.y) < 0.5 && Math.abs(light.width - (to.w + 4)) < 0.5, "the light sits on the words: " + JSON.stringify(light));
    assertEqual(await page.locator(".dov-comet").count(), 0, "the dots are gone once landed");
    await page.close();
    const still = await open(browser, FLIGHT, "dark", true);
    await still.evaluate((t) => window.__say("dictation-flight", { from: [450, 360], to: t, theme: "dark", fontScale: 100 }), to);
    assertEqual(await still.locator(".dov-comet").count(), 0, "reduced motion: no flight");
    assertEqual(await still.locator(".dov-light").count(), 1, "reduced motion: the light at once");
    await still.close();
    return "16 dots, then the light on the words' rectangle; reduced motion: the light alone";
  });

  await run.check("the menu, word for word, every row reaching the tool, Escape and the arrows", async () => {
    const page = await open(browser, MENU);
    const menu = (extra) => Object.assign({ seq: 3, on: true, key: "F1", choice: "accurate", paused: false, secureApp: null, theme: "dark", fontScale: 100 }, extra || {});
    await say(page, "dictation-menu", menu());
    assertEqual(await text(page, ".sb-head"), "RichScribe On", "the heading");
    assertEqual(await text(page, ".sb-say"), "Tap F1 to start and again to stop.", "the line under it");
    const rows = await page.locator(".sb-item").allInnerTexts();
    assertEqual(JSON.stringify(rows.map((r) => r.trim())), JSON.stringify(["More accurate", "Faster", "RichScribe settings…", "Turn RichScribe off", "Open RichOS"]), "the rows");
    assertEqual(await page.locator('.sb-item[aria-checked="true"]').innerText(), "More accurate", "the tick");
    await say(page, "dictation-menu", menu({ choice: "fast" }));
    assertEqual(await page.locator('.sb-item[aria-checked="true"]').innerText(), "Faster", "the tick follows the model");
    await say(page, "dictation-menu", menu({ paused: true, secureApp: "1Password" }));
    assertEqual(await text(page, ".sb-on"), "Paused", "line 4's heading");
    assertEqual(await text(page, ".sb-warn"), "1Password is hiding your keys from other apps, so F1 can't reach me. Quitting 1Password fixes it.", "line 4, named");
    await say(page, "dictation-menu", menu({ paused: true, secureApp: null }));
    assertEqual(await text(page, ".sb-warn"), "Another app is hiding your keys, so F1 can't reach me. Quitting the app where you last typed a password usually fixes it.", "line 4, unnamed");
    await say(page, "dictation-menu", menu({ on: false }));
    assertEqual(await text(page, ".sb-head"), "RichScribe Off", "off");
    assertEqual(await text(page, ".sb-say"), "Turn it on to type with your voice.", "off's line");
    await say(page, "dictation-menu", menu());
    const size = await page.evaluate(() => window.__calls.filter((c) => c[0] === "dictation_menu_laid_out").pop()[1]);
    const box = await page.locator("#sbMenu").boundingBox();
    assertEqual(size.w, Math.ceil(box.width + 16), "the menu's window width");
    for (const label of ["Faster", "RichScribe settings…", "Turn RichScribe off", "Open RichOS"]) {
      await page.locator(".sb-item", { hasText: label }).click();
    }
    const acts = await page.evaluate(() => window.__calls.filter((c) => c[0] === "dictation_menu_act").map((c) => c[1].act + (c[1].value ? ":" + c[1].value : "")));
    assertEqual(JSON.stringify(acts), JSON.stringify(["mode:fast", "settings", "onoff", "open"]), "each row reaches the tool");
    await page.locator(".sb-item").first().focus();
    await page.keyboard.press("ArrowDown");
    assertEqual(await page.evaluate(() => document.activeElement.textContent.trim()), "Faster", "ArrowDown moves to the next row");
    await page.keyboard.press("ArrowUp");
    await page.keyboard.press("ArrowUp");
    assertEqual(await page.evaluate(() => document.activeElement.textContent.trim()), "Open RichOS", "ArrowUp wraps to the last row");
    await page.keyboard.press("Escape");
    const closed = await page.evaluate(() => window.__calls.filter((c) => c[0] === "dictation_menu_act" && c[1].act === "close").length);
    assertEqual(closed, 1, "Escape closes the menu");
    assertEqual(page.__errors.length, 0, page.__errors.join(" | "));
    await page.close();
    return "heading, line, five rows, the tick, line 4 named and unnamed, off; four rows and Escape reach the tool";
  });

  const BAR_INDICATORS = [
    ["the orb (gold)", ".dh-orb", "backgroundColor", ".dh"],
    ["the meter's bars", ".dh-meter i", "backgroundColor", ".dh"],
  ];
  for (const theme of ["dark", "light"]) {
    await run.check(theme + ": every line and indicator on the bar and in the menu clears its floor, at 16px or more", async () => {
      const failures = [];
      const notes = [];
      let worst = { ratio: 99 };
      const judge = (where, result) => {
        for (const t of result.text) {
          const large = C.isLargeText(t.px, t.weight);
          if (t.ratio < (large ? 3 : 4.5)) failures.push(`${where}: "${t.text}" ${t.ratio}:1`);
          if (t.px < 16) failures.push(`${where}: "${t.text}" is ${t.px}px`);
          if (t.ratio < worst.ratio) worst = Object.assign({ where }, t);
        }
        for (const i of result.indicators) {
          if (i.missing) failures.push(`${where}: ${i.name} not drawn`);
          else if (i.ratio < 3) failures.push(`${where}: ${i.name} ${i.ratio}:1`);
          else notes.push(`${i.name} ${i.ratio}`);
        }
      };
      const views = [bar("listening", { startedMs: Date.now() }), bar("writing"), bar("added"), bar("off")]
        .concat(Object.keys(LINES).map((p) => bar("problem", { problem: p })));
      for (const v of views) {
        for (const [gname, ground] of [["over white", WHITE], ["over black", BLACK]]) {
          const page = await open(browser, BAR, theme);
          await say(page, "dictation-bar", Object.assign(v, { theme }));
          const ind = v.view === "listening" ? BAR_INDICATORS
            : v.view === "problem" ? [["the problem ring", ".dh-orb", "borderTopColor", ".dh"]].concat(
              v.problem === "no-microphone" || v.problem === "no-accessibility" ? [["Fix it's fill", "#fixIt", "backgroundColor", ".dh"]] : [])
            : v.view === "writing" ? [["the spinner's arc", ".dh-orb", "borderTopColor", ".dh"]] : [];
          judge(`${v.view}${v.problem ? " " + v.problem : ""} ${gname}`, await walk(page, ground, ind));
          await page.close();
        }
      }
      for (const m of [{ on: true }, { on: true, paused: true, secureApp: "1Password" }, { on: true, paused: true }, { on: false }]) {
        for (const [gname, ground] of [["over white", WHITE], ["over black", BLACK]]) {
          const page = await open(browser, MENU, theme);
          await say(page, "dictation-menu", Object.assign({ seq: 1, key: "F1", choice: "accurate", paused: false, secureApp: null, theme, fontScale: 100 }, m));
          judge(`menu ${JSON.stringify(m)} ${gname}`, await walk(page, ground, [["the tick", ".sb-check path", "stroke", ".sbmenu"]]));
          await page.close();
        }
      }
      // The menu bar item while listening (menubar.rs): the gold square on the menu bar as
      // round 19 draws it (`--mb-bg` over its desktop), the measure round 19 made.
      const mb = theme === "dark" ? C.compositeOver({ r: 10, g: 15, b: 27, a: 0.8 }, { r: 10, g: 17, b: 32, a: 1 })
        : C.compositeOver({ r: 246, g: 244, b: 239, a: 0.82 }, { r: 238, g: 234, b: 225, a: 1 });
      const gold = theme === "dark" ? { r: 0xc2, g: 0xa3, b: 0x5c, a: 1 } : { r: 0x9c, g: 0x7c, b: 0x34, a: 1 };
      const item = C.round2(C.contrastRatio(gold, mb));
      if (item < 3) failures.push(`the listening menu bar item ${item}:1`);
      notes.push(`menu bar item ${item}`);
      assert(failures.length === 0, failures.join("\n  "));
      return `worst text ${worst.ratio}:1 ("${worst.text}", ${worst.where}); indicators: ${Array.from(new Set(notes)).slice(0, 8).join(", ")}`;
    });
  }

  await run.check("the light over another app's words keeps them at 4.5:1: TextEdit, Notes and RichOS's message box, every theme", async () => {
    // What the light is drawn over. TextEdit and Notes paint AppKit's textColor on
    // textBackgroundColor: black on white in the light appearance, white at 85% on #1E1E1E in
    // the dark. RichOS's own message box is read off the shipped page, in each RichOS theme.
    const apps = [
      ["TextEdit, light", { r: 0, g: 0, b: 0, a: 1 }, WHITE],
      ["TextEdit, dark", { r: 255, g: 255, b: 255, a: 0.85 }, { r: 30, g: 30, b: 30, a: 1 }],
      ["Notes, light", { r: 0, g: 0, b: 0, a: 0.85 }, WHITE],
      ["Notes, dark", { r: 255, g: 255, b: 255, a: 0.85 }, { r: 30, g: 30, b: 30, a: 1 }],
    ];
    for (const appTheme of ["dark", "light"]) {
      const page = await browser.newPage();
      await page.addInitScript((t) => { try { localStorage.setItem("richos-theme", t); } catch (e) { /* none */ } }, appTheme);
      await page.goto(INDEX);
      await page.addScriptTag({ content: C.pageScript() });
      const box = await page.evaluate(() => {
        const M = window.__contrastMath;
        const el = document.getElementById("input");
        let bg = null;
        for (let e = el; e; e = e.parentElement) {
          const c = M.parseCssColor(getComputedStyle(e).backgroundColor);
          if (c && c.a === 1) { bg = c; break; }
        }
        return { ink: M.parseCssColor(getComputedStyle(el).color), bg };
      });
      apps.push([`RichOS's message box, ${appTheme}`, box.ink, box.bg]);
      await page.close();
    }
    const lines = [];
    let worst = 99;
    for (const theme of ["dark", "light"]) {
      const page = await open(browser, FLIGHT, theme);
      const hi = C.parseCssColor(await page.evaluate(() => {
        const probe = document.createElement("div");
        probe.style.color = "var(--dict-hi)";
        document.body.appendChild(probe);
        return getComputedStyle(probe).color;
      }));
      await page.close();
      for (const [name, ink, ground] of apps) {
        const under = C.compositeOver(ink, ground);
        const r = C.round2(C.contrastRatio(C.compositeOver(hi, under), C.compositeOver(hi, ground)));
        worst = Math.min(worst, r);
        lines.push(`${theme} light over ${name}: ${r}:1`);
        assert(r >= 4.5, `the light (${theme}, alpha ${hi.a}) over ${name} leaves the words at ${r}:1`);
      }
    }
    return `worst ${worst}:1 of ${lines.length}: ` + lines.join("; ");
  });

  await browser.close();
  process.exitCode = run.report() ? 1 : 0;
}

main().catch((e) => {
  console.error(e);
  process.exit(1);
});

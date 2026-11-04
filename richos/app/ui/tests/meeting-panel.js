// MEETING NOTES' FLOATING PANEL (Granola PRD V4; the CEO's chosen design, richos-hq
// design/mockups/rounds/round-25.1, `#fl` and its script).
//
// The app opens `meeting-panel.html` in a window of its own over the meeting app
// (`src-tauri/src/meetings.rs`). This suite loads the page in WebKit with a stand-in for the Tauri
// bridge that records every call the page makes and lets the suite speak for Rust:
//
//   1. Every view, word for word against round 25.1: the scheduled and unscheduled prompts (Take
//      Notes, Not this one, the hairline that never expires), "Starting…", "Recording meeting"
//      with its running time and Stop, a problem beside the call with its actions.
//   2. The answers reach Rust: Take Notes, Stop, Try again; Not this one and Escape put the prompt
//      ASIDE (the voice bar; nothing reaches Rust but the bar's window), and the bar's × is the
//      decline (`not-this-one`). Pointing at the bar for 200 ms brings the card back.
//   3. The window the page asks for is the panel plus its shadow room, and says when it is the bar.
//   4. The bar's level: at rest (6%) with no level from Rust, and moving with `meeting-level`
//      (the CEO, 2026-10-13: "move with the call's sound as designed").
//   5. The recording indicator folds after five seconds and opens while pointed at.
//   6. CONTRAST, computed in both themes over white and black grounds (the panel floats over any
//      app): every text run and every indicator the round declared.
//   7. Every line meant to be read is 16px or larger, except the declared 14px RichOS caps label.
//
// Run: node meeting-panel.js
"use strict";
const path = require("path");
const { loadPlaywright, createRun, assert, assertEqual, UI_DIR } = require("./lib/harness");
const C = require("./lib/contrast");

const PANEL = "file://" + path.join(UI_DIR, "meeting-panel.html");

const BRIDGE = `
  window.__calls = [];
  window.__listeners = {};
  window.__TAURI__ = {
    core: { invoke: (cmd, args) => { window.__calls.push([cmd, args || {}]); return Promise.resolve(null); } },
    event: { listen: (name, fn) => { window.__listeners[name] = fn; return Promise.resolve(() => {}); } },
  };
  window.__say = (name, payload) => window.__listeners[name]({ payload });
`;

async function open(browser, theme) {
  const page = await browser.newPage({ viewport: { width: 520, height: 260 } });
  page.__errors = [];
  page.on("pageerror", (e) => page.__errors.push(String(e)));
  page.on("console", (m) => { if (m.type() === "error") page.__errors.push("console: " + m.text()); });
  await page.addInitScript(BRIDGE);
  await page.addInitScript((t) => { try { localStorage.setItem("richos-theme", t); } catch (e) { /* none */ } }, theme || "dark");
  await page.goto(PANEL);
  await page.waitForFunction("window.__calls && window.__calls.some(c => c[0] === 'meeting_panel_state')");
  return page;
}

async function say(page, name, payload) {
  await page.evaluate(([n, p]) => window.__say(n, p), [name, payload]);
  await page.evaluate(() => new Promise((r) => requestAnimationFrame(() => requestAnimationFrame(r))));
}
async function settle(page, ms) { await page.waitForTimeout(ms); }
async function text(page, sel) { return (await page.locator(sel).innerText()).replace(/\s+/g, " ").trim(); }
async function calls(page, cmd) { return page.evaluate((c) => window.__calls.filter((x) => x[0] === c).map((x) => x[1]), cmd); }

const SCHEDULED = { view: "prompt", title: "Acme pricing review", sub: "Starts in 1 minute." };
const UNSCHEDULED = { view: "prompt", title: "Call detected in Zoom.", sub: null };

/// The contrast walk (as dictation-overlay.js does it for a transparent page): each text run over
/// the first ancestor that paints, composited over `ground`.
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
    for (const el of document.querySelectorAll("#fl *")) {
      const own = Array.from(el.childNodes).some((n) => n.nodeType === 3 && n.textContent.trim());
      const r = el.getBoundingClientRect();
      if (!own || r.width === 0 || r.height === 0) continue;
      const s = getComputedStyle(el);
      if (parseFloat(s.opacity) === 0) continue;
      const bg = behind(el);
      const fg = M.compositeOver(parse(s.color), bg);
      out.text.push({ text: el.textContent.trim().slice(0, 50), cls: el.className, px: parseFloat(s.fontSize), weight: s.fontWeight,
        ratio: M.round2(M.contrastRatio(fg, bg)) });
    }
    for (const [name, sel, prop] of indicators) {
      const el = document.querySelector(sel);
      if (!el) { out.indicators.push({ name, missing: true }); continue; }
      const s = getComputedStyle(el);
      const bg = behind(el.parentElement);
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
const LARGE = (t) => t.px >= 24 || (t.px >= 18.66 && Number(t.weight) >= 700);

async function main() {
  const run = createRun("meeting notes' floating panel");
  const browser = await loadPlaywright().webkit.launch();

  await run.check("every view, word for word against round 25.1", async () => {
    const page = await open(browser);
    await say(page, "meeting-panel", SCHEDULED);
    assertEqual(await text(page, "#fl .fl-app"), "RICHOS", "the caps micro-label");
    assertEqual(await text(page, "#fl .fl-title"), "Acme pricing review", "the calendar title");
    assertEqual(await text(page, "#fl .fl-sub"), "Starts in 1 minute.", "the start line");
    assertEqual(await text(page, "#fl .fl-actions"), "Take Notes Not this one", "the two answers");
    assertEqual(await page.locator("#fl .fl-life i").count(), 1, "the hairline");
    const anim = await page.evaluate(() => getComputedStyle(document.querySelector(".fl-life i")).animation);
    assert(/15s/.test(anim) && /infinite/.test(anim) && /alternate/.test(anim), "the hairline sweeps back and forth for ever: " + anim);
    await say(page, "meeting-panel", UNSCHEDULED);
    assertEqual(await text(page, "#fl .fl-title"), "Call detected in Zoom.", "the unscheduled prompt");
    assertEqual(await page.locator("#fl .fl-sub").count(), 0, "no start line for an unscheduled call");
    await say(page, "meeting-panel", { view: "starting" });
    assertEqual(await text(page, "#fl .fl-rec"), "Starting… Stop", "Starting…");
    await say(page, "meeting-panel", { view: "recording", startedAt: Date.now() - 42_000, note: null });
    const rec = await text(page, "#fl .fl-rec");
    assert(/^Recording meeting 0:4[2-3] Stop$/.test(rec), "Recording meeting with its time: " + rec);
    await say(page, "meeting-panel", { view: "problem", title: "Nothing was recorded.", sub: "The recorder did not start.",
      actions: [{ label: "Try again", act: "retry", primary: true }, { label: "Not this one", act: "aside", primary: false }] });
    assertEqual(await text(page, "#fl .fl-title"), "Nothing was recorded.", "a problem beside the call");
    assertEqual(await text(page, "#fl .fl-actions"), "Try again Not this one", "its actions");
    assertEqual(page.__errors.length, 0, page.__errors.join(" | "));
    await page.close();
    return "scheduled and unscheduled prompts, Starting…, Recording meeting 0:42, a problem";
  });

  await run.check("the answers reach Rust, and Not this one or Escape puts the prompt aside, never away", async () => {
    const page = await open(browser);
    await say(page, "meeting-panel", SCHEDULED);
    await page.locator("button", { hasText: "Take Notes" }).click();
    assertEqual(JSON.stringify(await calls(page, "meeting_answer")), JSON.stringify([{ answer: "take" }]), "Take Notes");
    await say(page, "meeting-panel", { view: "hidden" });
    await say(page, "meeting-panel", UNSCHEDULED);
    await page.locator("button", { hasText: "Not this one" }).click();
    await settle(page, 700);
    assert(await page.locator("#fl.is-bar").count() === 1, "Not this one made the voice bar");
    assertEqual((await calls(page, "meeting_answer")).length, 1, "putting it aside tells Rust nothing but the window");
    const aside = (await calls(page, "meeting_panel_laid_out")).pop();
    assertEqual(aside.aside, true, "the bar's window says it is the bar");
    // Pointing at the bar's main part for 200 ms brings the card back.
    await page.locator(".fl-vb-main").hover();
    await settle(page, 900);
    assertEqual(await page.locator("#fl.is-bar").count(), 0, "the card came back");
    assertEqual(await text(page, "#fl .fl-title"), "Call detected in Zoom.", "the same prompt");
    // Escape with the card focused does what Not this one does.
    await page.locator("button", { hasText: "Take Notes" }).focus();
    await page.keyboard.press("Escape");
    await settle(page, 700);
    assertEqual(await page.locator("#fl.is-bar").count(), 1, "Escape put it aside");
    await page.locator(".fl-vb-x").click();
    assertEqual(JSON.stringify((await calls(page, "meeting_answer")).pop()), JSON.stringify({ answer: "not-this-one" }), "the × is the decline");
    await say(page, "meeting-panel", { view: "recording", startedAt: Date.now(), note: null });
    await page.locator(".fl-stop").click();
    assertEqual(JSON.stringify((await calls(page, "meeting_answer")).pop()), JSON.stringify({ answer: "stop" }), "Stop");
    assertEqual(page.__errors.length, 0, page.__errors.join(" | "));
    await page.close();
    return "take, the bar (hover back, Escape), the × as the decline, stop";
  });

  await run.check("a window never yet on screen still asks to be shown: the size goes out in the same turn, no animation frame", async () => {
    // The installed app (walk run 7, 2026-10-13): the panel window is built hidden and only this
    // report shows it, and WebKit runs no animation frame for a window that has never been on
    // screen. The report waited for one, so the prompt never appeared. Here no frame ever runs.
    const page = await open(browser);
    const sent = await page.evaluate((view) => {
      window.requestAnimationFrame = () => 0;
      window.__calls.length = 0;
      window.__say("meeting-panel", view);
      return window.__calls.filter((c) => c[0] === "meeting_panel_laid_out").map((c) => c[1]);
    }, SCHEDULED);
    assertEqual(sent.length, 1, "one layout report, sent before any frame");
    assert(sent[0].w > 300 && sent[0].h > 100, `a real size: ${JSON.stringify(sent[0])}`);
    await page.close();
    return `${sent[0].w} x ${sent[0].h} with no animation frame`;
  });

  await run.check("the window asked for is the panel plus its shadow room", async () => {
    const page = await open(browser);
    await say(page, "meeting-panel", SCHEDULED);
    await settle(page, 500);
    const l = (await calls(page, "meeting_panel_laid_out")).pop();
    const box = await page.locator("#fl").boundingBox();
    assert(Math.abs(l.w - (Math.ceil(box.width) + 34)) <= 1, `width: the 380px card and 24 + 10 for its shadow: ${l.w} for ${box.width}`);
    assert(Math.abs(l.h - (Math.ceil(box.height) + 46)) <= 1, `height: the card and 6 + 40: ${l.h} for ${box.height}`);
    assertEqual(Math.round(box.width), 380, "the card is round 25.1's 380px");
    await say(page, "meeting-panel", { view: "hidden" });
    await settle(page, 500);
    const gone = (await calls(page, "meeting_panel_laid_out")).pop();
    assertEqual(gone.w, 0, "hidden asks for no window");
    await page.close();
    return `${l.w} x ${l.h} for a ${Math.round(box.width)} x ${Math.round(box.height)} card`;
  });

  await run.check("the bar rests with no level and moves with the call's sound", async () => {
    const page = await open(browser);
    await say(page, "meeting-panel", UNSCHEDULED);
    await page.locator("button", { hasText: "Not this one" }).click();
    await settle(page, 700);
    const rest = await page.evaluate(() => document.querySelector(".fl-vb-lvl").style.height || getComputedStyle(document.querySelector(".fl-vb-lvl")).height);
    assert(rest === "" || /^6(\.0)?%$/.test(rest) || parseFloat(rest) <= 6, "at rest with no level: " + rest);
    for (let i = 0; i < 20; i++) await say(page, "meeting-level", { rms: 0.1, peak: 0.3 });
    await settle(page, 300);
    const moving = await page.evaluate(() => parseFloat(document.querySelector(".fl-vb-lvl").style.height));
    assert(moving > 50, "-20 dBFS of call sound lifts the level well up: " + moving + "%");
    const levelOf = await page.evaluate(() => [0, 0.00316, 0.1, 0.316, 1].map((r) => window.RichMeetingPanel.levelOf(r)));
    assertEqual(JSON.stringify(levelOf.map((v) => Math.round(v * 100) / 100)), JSON.stringify([0, 0, 0.75, 1, 1]), "the dB mapping (-50 rests, -10 fills)");
    await page.close();
    return `rests at ${rest || "6%"}, ${moving.toFixed(1)}% at -20 dBFS`;
  });

  await run.check("the recording indicator folds after five seconds and opens while pointed at", async () => {
    const page = await open(browser);
    await page.mouse.move(5, 250);
    await say(page, "meeting-panel", { view: "recording", startedAt: Date.now(), note: null });
    await settle(page, 5600);
    assertEqual(await page.locator("#fl.is-compact").count(), 1, "folded after five seconds");
    await page.locator("#fl").hover();
    await settle(page, 200);
    assertEqual(await page.locator("#fl.is-compact").count(), 0, "pointing at it opens it");
    await page.close();
    return "folded at 5 s, open while pointed at";
  });

  for (const theme of ["dark", "light"]) {
    await run.check(`contrast in the ${theme} theme, over white and black: text 4.5:1 (3:1 large), indicators 3:1`, async () => {
      const page = await open(browser, theme);
      const fails = [];
      let n = 0;
      for (const view of [SCHEDULED, { view: "recording", startedAt: Date.now(), note: null }]) {
        await say(page, "meeting-panel", view);
        await settle(page, 600);
        for (const ground of [WHITE, BLACK]) {
          const r = await walk(page, ground, [
            ["hairline", ".fl-life i", "backgroundColor"], ["recording dot", ".fl-rec-dot", "backgroundColor"],
          ]);
          for (const t of r.text) {
            n++;
            const floor = LARGE(t) ? 3 : 4.5;
            if (t.ratio < floor) fails.push(`${t.text} ${t.ratio}:1 < ${floor}`);
          }
          for (const i of r.indicators) if (!i.missing && i.ratio < 3) fails.push(`${i.name} ${i.ratio}:1 < 3`);
        }
      }
      await say(page, "meeting-panel", UNSCHEDULED);
      await page.locator("button", { hasText: "Not this one" }).click();
      await settle(page, 700);
      for (const ground of [WHITE, BLACK]) {
        const r = await walk(page, ground, [["the bar's level", ".fl-vb-lvl", "backgroundColor"]]);
        for (const i of r.indicators) if (i.missing || i.ratio < 3) fails.push(`${i.name} ${i.ratio}:1 < 3`);
      }
      assertEqual(fails, [], `${theme}: below the floor`);
      await page.close();
      return `${n} text runs and the indicators, ${theme}`;
    });
  }

  await run.check("every line meant to be read is 16px or larger (the 14px RichOS caps label is declared)", async () => {
    const page = await open(browser);
    const small = [];
    for (const view of [SCHEDULED, { view: "recording", startedAt: Date.now(), note: "Recording continues while setup finishes" }]) {
      await say(page, "meeting-panel", view);
      await settle(page, 400);
      const r = await walk(page, WHITE, []);
      for (const t of r.text) if (t.px < 16 && !/fl-app/.test(t.cls)) small.push(`${t.text} ${t.px}px`);
    }
    assertEqual(small, [], "under 16px");
    await page.close();
    return "all 16px or larger";
  });

  await browser.close();
  process.exit(run.report() ? 1 : 0);
}

main().catch((e) => { console.error(e); process.exit(1); });

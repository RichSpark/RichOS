"use strict";
// "LET CODEX REVIEW YOUR TEAM'S WORK": round 20.2 (richos-hq design/mockups/rounds/round-20.2/,
// index.html and NOTES.md), measured against the CEO's words, ruling §114 (2026-10-09): "in our
// app, we should give the user a toggle/switch to manually enable that. Because those reviews
// consume a bit of their Codex tokens, but mostly to make them aware that this would be happening
// in the first place." and "Near that toggle for Codex usage in RichOS app, we should also mention
// that this review process won't be visible in their regular ChatGPT/Codex app."
//
// One check per part of the round, each red on a tree without the row, then every state in both
// themes for contrast (text 4.5:1, the switch, the ⓘ and the rules 3:1) and the 16px type floor,
// computed in WebKit at 1440 by 900 with the tooltip open.
const path = require("path");
const contrast = require("./lib/contrast");
const { loadPlaywright, createRun, assert, assertEqual, UI_DIR } = require("./lib/harness");

// Round 20.2's words, per state, as its index.html carries them (apostrophes compared plain).
const WORDS = {
  off: [
    "Before your team's finished work is accepted, a second AI reviews it. Claude does that now. Turn this on and Codex does it instead, so a model from a different company checks the work.",
    "Codex will read your team's work, and each review uses a little of your Codex allowance.",
    "These reviews won't show up in your ChatGPT or Codex app: no conversations or history appear there.",
  ],
  on: [
    "Codex reviews your team's finished work before it's accepted, through the Codex app on this Mac, signed in to ChatGPT.",
    "Codex reads your team's work, and each review uses a little of your Codex allowance. Turn this off and Claude reviews it again.",
    "These reviews won't show up in your ChatGPT or Codex app: no conversations or history appear there.",
  ],
  missing: [
    "Before your team's finished work is accepted, a second AI reviews it. Claude does that now. Codex could do it instead, so a model from a different company checks the work.",
    "The Codex app isn't on this Mac. Install it and sign in to ChatGPT, and this switch can be turned on.",
  ],
  signedout: [
    "Before your team's finished work is accepted, a second AI reviews it. Claude does that now. Codex could do it instead, so a model from a different company checks the work.",
    "Codex isn't signed in. Open the Codex app and sign in to ChatGPT, and this switch can be turned on.",
  ],
  "lapsed-signedout": [
    "You turned this on, so Codex reviews your team's work whenever it can.",
    "Codex isn't signed in right now, so Claude is reviewing in the meantime. Sign in to ChatGPT in the Codex app and Codex takes over again.",
    "These reviews won't show up in your ChatGPT or Codex app: no conversations or history appear there.",
  ],
  "lapsed-missing": [
    "You turned this on, so Codex reviews your team's work whenever it can.",
    "The Codex app isn't on this Mac right now, so Claude is reviewing in the meantime. Install it, sign in to ChatGPT, and Codex takes over again.",
    "These reviews won't show up in your ChatGPT or Codex app: no conversations or history appear there.",
  ],
};
// [name, preset, who, checked, unavailable, words]
const STATES = [
  ["off (first run)", { on: false, codex: "ready" }, "Claude", false, false, "off"],
  ["on", { on: true, codex: "ready" }, "Codex", true, false, "on"],
  ["on, then signed out", { on: true, codex: "signedout" }, "Claude", true, false, "lapsed-signedout"],
  ["on, then removed", { on: true, codex: "missing" }, "Claude", true, false, "lapsed-missing"],
];
// **A switch that cannot be used is not shown** (hunt 57, finding 19, 2026-10-15): off, with
// Codex not installed or not signed in, the row is not in the menu at all. Round 20.2 drew these
// two as a dashed switch; their words stay in the row's markup for when Codex is ready again.
const NOT_SHOWN = [
  ["not installed", { on: false, codex: "missing" }],
  ["not signed in", { on: false, codex: "signedout" }],
];
const plain = s => s.replace(/[’‘]/g, "'").replace(/\s+/g, " ").trim();

async function main() {
  const run = createRun("Let Codex review your team's work (round 20.2)");
  const browser = await loadPlaywright().webkit.launch();
  const errors = [];
  async function open(theme, codexReviews, opts = {}) {
    const page = await browser.newPage({ viewport: { width: 1440, height: 900 }, locale: "en-US", reducedMotion: opts.reducedMotion || "no-preference" });
    page.setDefaultTimeout(30000); // load-bound: a hang guard only; every wait below waits for a fact
    page.on("pageerror", e => errors.push(String(e)));
    await page.addInitScript(({ theme, codexReviews }) => {
      localStorage.setItem("richos-theme", theme);
      localStorage.setItem("richos-mock-config", JSON.stringify({ theme, font_scale: 100 }));
      window.__RICHOS_MOCK_PRESET__ = { codexReviews };
    }, { theme, codexReviews });
    await page.goto("file://" + path.join(UI_DIR, "index.html"));
    await page.waitForSelector("#set-btn");
    await page.evaluate(() => window.RichSplash.yieldNow("codex-reviews-test"));
    await page.evaluate(t => document.documentElement.setAttribute("data-theme", t), theme);
    return page;
  }
  async function menu(page) {
    if (await page.locator("#set-menu").isHidden()) await page.click("#set-btn");
    await page.waitForSelector("#set-menu #cx-switch", { state: "attached" });
    // The row is painted from the status read at open.
    await page.waitForFunction(() => window.RichCodexReviews && window.RichCodexReviews.state());
  }
  const text = (page, sel) => page.locator(sel).innerText().then(plain);
  async function shownWords(page) {
    return page.evaluate(() => [...document.querySelectorAll("#cx-tip > div:not([hidden]) p")].map(p => p.innerText));
  }
  const calls = page => page.evaluate(() => window.__RICHOS_MOCK__.codexReviewCalls());
  // "Still open after the pointer left" is decided by ORDER, not by the host's clock: the row puts
  // the tooltip away on a 220 ms page timer started when the pointer leaves (codex-reviews.js
  // tipHide). This page timer is started after that event was dispatched, with a later deadline,
  // and a page runs its timers in deadline order, so when it fires any put-away the leave
  // scheduled has already run, however slow the host is.
  // load-bound: an in-page timer ordered after the row's own 220 ms timer; a stalled host delays both alike.
  const pastGrace = page => page.evaluate(() => new Promise(r => setTimeout(r, 400)));

  // ---- 1. the row -------------------------------------------------------------------------
  await run.check("the row: below Technical view, between two rules; off on first run, Reviewing now: Claude, an ⓘ after Claude; nothing under it", async () => {
    const page = await open("dark", undefined);
    await menu(page);
    const order = await page.evaluate(() => [...document.querySelectorAll("#set-menu > *")].map(e => e.id || e.className));
    const techy = order.indexOf("set-techy-row");
    assert(techy >= 0, "Technical view is in the menu: " + order);
    // Round 22.1 put the Token spend row directly under Technical view, above this row's rule.
    assertEqual(order[techy + 1], "set-spend", "Token spend directly after Technical view (round 22.1): " + order);
    assertEqual(order[techy + 2], "set-codex", "the row comes directly after Token spend: " + order);
    assertEqual(await page.locator("#set-codex > .set-rule").count(), 2, "a rule above and below");
    assertEqual(await text(page, "#cx-title"), "Let Codex review your team's work");
    assertEqual(await text(page, "#cx-who"), "Reviewing now: Claude");
    assertEqual(await page.getAttribute("#cx-switch", "aria-checked"), "false", "off on first run");
    assertEqual(await page.getAttribute("#cx-switch", "role"), "switch");
    assertEqual(await page.getAttribute("#cx-info", "aria-label"), "About Codex reviews");
    // The ⓘ sits right after the reviewer's name, on the same line.
    const geo = await page.evaluate(() => {
      const b = document.querySelector("#cx-who b").getBoundingClientRect(), i = document.querySelector("#cx-info").getBoundingClientRect();
      return { gap: i.left - b.right, sameLine: Math.abs((i.top + i.bottom) / 2 - (b.top + b.bottom) / 2) < 6 };
    });
    assert(geo.sameLine && geo.gap >= 0 && geo.gap < 12, "the ⓘ right after Claude: " + JSON.stringify(geo));
    assert(await page.locator("#cx-tip").isHidden(), "the words are in the tooltip, not under the row");
    assertEqual(await page.evaluate(() => document.getElementById("set-menu").getBoundingClientRect().width), 372, "the menu widens to 372px, as drawn");
    await page.close();
  });

  // ---- 2. every state's words, in the ⓘ ---------------------------------------------------
  await run.check("every state: who reviews, the switch, and that state's words word for word in the ⓘ, describing both the ⓘ and the switch; the row is the same height in every state", async () => {
    const heights = new Set();
    const seen = [];
    for (const [name, preset, who, checked, unavailable, key] of STATES) {
      const page = await open("dark", preset);
      await menu(page);
      assertEqual(await text(page, "#cx-who"), "Reviewing now: " + who, name);
      assertEqual(await page.getAttribute("#cx-switch", "aria-checked"), String(checked), name + ": the switch");
      assertEqual(await page.getAttribute("#cx-switch", "aria-disabled"), String(unavailable), name + ": can it be pressed");
      assert(await page.locator("#cx-info").isVisible(), name + ": the ⓘ is there in every state");
      await page.hover("#cx-info");
      await page.waitForSelector("#cx-tip.is-shown");
      assertEqual((await shownWords(page)).map(plain), WORDS[key], name + ": the tooltip's words");
      const described = await page.evaluate(() => [document.getElementById("cx-info").getAttribute("aria-describedby"),
        document.getElementById("cx-switch").getAttribute("aria-describedby")]);
      assertEqual(described[0], described[1], name + ": one description for the ⓘ and the switch");
      assert(await page.locator("#" + described[0]).isVisible(), name + ": the description is the shown words");
      heights.add(await page.evaluate(() => Math.round(document.getElementById("cx-row").getBoundingClientRect().height)));
      // Unavailable: a dashed track and a hollow knob.
      const look = await page.evaluate(() => {
        const sw = document.getElementById("cx-switch");
        return { border: getComputedStyle(sw).borderTopStyle, knob: getComputedStyle(sw, "::after").backgroundColor };
      });
      if (unavailable) assert(look.border === "dashed" && /rgba\(0, 0, 0, 0\)|transparent/.test(look.knob), name + ": dashed, hollow: " + JSON.stringify(look));
      else assertEqual(look.border, "solid", name + ": a solid track");
      // Only the switch is unavailable. The ⓘ, the name and the words always work, so nothing
      // around them claims aria-disabled, which a screen reader reads into every descendant (the
      // 2026-10-09 VM walk's accessibility tree said the ⓘ was disabled in the not-installed state).
      assertEqual(await page.evaluate(() => {
        const host = document.getElementById("cx-info").parentElement.closest("[aria-disabled='true']");
        return host ? host.id || host.className : null;
      }), null, name + ": the ⓘ sits inside nothing marked aria-disabled");
      seen.push(name);
      await page.close();
    }
    assertEqual(heights.size, 1, "nothing sits under the row in any state: heights " + [...heights]);
    return seen.join(", ");
  });

  // ---- 3. turning it on and off ----------------------------------------------------------
  await run.check("flipping: on saves the choice and says Codex, the ⓘ's ring pulses; off again says Claude; the name flips it too", async () => {
    const page = await open("dark", { on: false, codex: "ready" });
    await menu(page);
    await page.click("#cx-switch");
    await page.waitForFunction(() => document.getElementById("cx-switch").getAttribute("aria-checked") === "true");
    assertEqual(await text(page, "#cx-who"), "Reviewing now: Codex");
    assert(await page.evaluate(() => document.getElementById("cx-info-wrap").classList.contains("is-new")), "the gold ring pulses on a flip");
    assertEqual((await calls(page)).filter(c => c.cmd === "codex_reviews_set"), [{ cmd: "codex_reviews_set", on: true }]);
    // Closed and opened again: the choice was saved, not only drawn.
    await page.click("#set-btn");
    await page.waitForSelector("#set-menu", { state: "hidden" });
    await menu(page);
    assertEqual(await page.getAttribute("#cx-switch", "aria-checked"), "true", "still on after the menu reopens");
    await page.click("#cx-title");
    await page.waitForFunction(() => document.getElementById("cx-switch").getAttribute("aria-checked") === "false");
    assertEqual(await text(page, "#cx-who"), "Reviewing now: Claude");
    // From the keyboard: Space on the focused switch.
    await page.focus("#cx-switch");
    await page.keyboard.press("Space");
    await page.waitForFunction(() => document.getElementById("cx-switch").getAttribute("aria-checked") === "true");
    await page.close();
  });

  await run.check("reduced motion: the flip changes the words with no ring", async () => {
    const page = await open("dark", { on: false, codex: "ready" }, { reducedMotion: "reduce" });
    await menu(page);
    await page.click("#cx-switch");
    await page.waitForFunction(() => document.getElementById("cx-switch").getAttribute("aria-checked") === "true");
    assert(!(await page.evaluate(() => document.getElementById("cx-info-wrap").classList.contains("is-new"))), "no ring");
    await page.close();
  });

  // ---- 4. a switch that cannot be used is not shown ---------------------------------------
  // Hunt 57, finding 19 (2026-10-15): nightly 57 showed a user without Codex "Let Codex review
  // your team's work · Reviewing now: Claude" beside a switch that could not be turned on.
  await run.check("off with Codex not installed or not signed in: the row, its switch and its rules are not shown", async () => {
    const seen = [];
    for (const [name, preset] of NOT_SHOWN) {
      const page = await open("dark", preset);
      await menu(page);
      assert(await page.locator("#set-codex").isHidden(), name + ": the row is shown");
      assert(await page.locator("#cx-switch").isHidden(), name + ": the switch that cannot be turned on is shown");
      assertEqual(await page.evaluate(() => [...document.querySelectorAll("#set-codex .set-rule")].filter(r => r.getClientRects().length).length), 0,
        name + ": a rule of the row is still drawn");
      assert(!(await page.evaluate(() => document.getElementById("set-menu").innerText)).includes("Codex"), name + ": the menu still names Codex");
      seen.push(name + ": not shown");
      await page.close();
    }
    return seen.join("; ");
  });

  // The second review of ecb68ec68 (rv-20261009T143828Z-ecb68ec6-82af, finding 2, fixture
  // unavailable-recovery.js): Settings stays open while the user installs Codex or signs in. The
  // row was not shown; coming back to the app reads Codex again, the row appears, and it can be
  // turned on, with no closing and reopening of Settings in between.
  await run.check("Codex ready while Settings stays open: returning to the app reads Codex again, the row appears and turns on", async () => {
    for (const codex of ["missing", "signedout"]) {
      const page = await open("dark", { on: false, codex });
      await menu(page);
      assert(await page.locator("#set-codex").isHidden(), codex + ": not shown as Settings opens");
      await page.evaluate(() => window.__RICHOS_MOCK__.codexSet("ready"));
      await page.evaluate(() => window.dispatchEvent(new Event("focus")));
      await page.waitForSelector("#cx-switch", { state: "visible" });
      await page.click("#cx-switch");
      await page.waitForFunction(() => document.getElementById("cx-switch").getAttribute("aria-checked") === "true");
      assertEqual(await text(page, "#cx-who"), "Reviewing now: Codex", codex + ": Codex reviews now");
      assertEqual(await page.getAttribute("#cx-switch", "aria-disabled"), "false", codex + ": the switch is usable");
      assert(!(await page.evaluate(() => document.getElementById("cx-row").classList.contains("is-nudged"))), codex + ": no nudge");
      assertEqual((await calls(page)).filter(c => c.cmd === "codex_reviews_set"), [{ cmd: "codex_reviews_set", on: true }], codex + ": turned on once");
      await page.close();
    }
  });

  // ---- 5. the tooltip ---------------------------------------------------------------------
  await run.check("the tooltip: hover, keyboard focus, the pointer can move into it, a click pins it, Esc puts away only the tooltip, and neither the ⓘ nor its words flip the switch", async () => {
    const page = await open("dark", { on: false, codex: "ready" });
    await menu(page);
    await page.hover("#cx-info");
    await page.waitForSelector("#cx-tip.is-shown");
    await page.hover("#cx-tip p");
    await pastGrace(page);
    assert(await page.locator("#cx-tip").isVisible(), "the pointer moved into it and it stayed");
    await page.click("#cx-tip p");
    assertEqual(await page.getAttribute("#cx-switch", "aria-checked"), "false", "a press on the words flips nothing");
    await page.mouse.move(5, 895);
    await page.waitForSelector("#cx-tip", { state: "hidden" });
    // A click pins it.
    await page.click("#cx-info");
    assertEqual(await page.getAttribute("#cx-switch", "aria-checked"), "false", "the ⓘ flips nothing");
    await page.mouse.move(5, 895);
    await pastGrace(page);
    assert(await page.locator("#cx-tip").isVisible(), "pinned by a click");
    // Esc: the tooltip goes, the menu stays.
    await page.keyboard.press("Escape");
    await page.waitForSelector("#cx-tip", { state: "hidden" });
    assert(await page.locator("#set-menu").isVisible(), "Esc put away only the tooltip");
    // Keyboard focus shows it.
    await page.focus("#cx-info");
    await page.waitForSelector("#cx-tip.is-shown");
    assertEqual(await page.getAttribute("#cx-info", "aria-expanded"), "true");
    await page.keyboard.press("Escape");
    await page.waitForSelector("#cx-tip", { state: "hidden" });
    assert(await page.locator("#set-menu").isVisible(), "the menu is still open");
    await page.keyboard.press("Escape");
    await page.waitForSelector("#set-menu", { state: "hidden" });
    await page.close();
  });

  // ---- 6. on, then Codex went away --------------------------------------------------------
  await run.check("on, then signed out: read when Settings opens; the choice stands, Claude covers and says so; it can still be turned off, and then, unusable, is not shown", async () => {
    const page = await open("dark", { on: false, codex: "ready" });
    await menu(page);
    await page.click("#cx-switch");
    await page.waitForFunction(() => document.getElementById("cx-switch").getAttribute("aria-checked") === "true");
    await page.click("#set-btn");
    await page.waitForSelector("#set-menu", { state: "hidden" });
    await page.evaluate(() => window.__RICHOS_MOCK__.codexSet("signedout"));
    await page.click("#set-btn");
    await page.waitForFunction(() => document.getElementById("cx-who").innerText.includes("Claude"));
    assertEqual(await page.getAttribute("#cx-switch", "aria-checked"), "true", "the choice stands");
    assertEqual(await page.getAttribute("#cx-switch", "aria-disabled"), "false", "it can be turned off");
    await page.hover("#cx-info");
    await page.waitForSelector("#cx-tip.is-shown");
    assertEqual((await shownWords(page)).map(plain), WORDS["lapsed-signedout"]);
    await page.click("#cx-switch");
    await page.waitForFunction(() => document.getElementById("cx-switch").getAttribute("aria-checked") === "false");
    // Off again, and it cannot be turned on while signed out, so it is no longer shown.
    await page.waitForSelector("#set-codex", { state: "hidden" });
    await page.close();
  });

  // The second review of acfdd9e70 (rv-20261009T151041Z-acfdd9e7-95ff, the one finding, fixture
  // status-transitions.js): with the switch on and Settings left OPEN, Codex is signed out (or
  // signed back in) in the Codex app, and the user comes back. Whenever the row can be seen again
  // it reads Codex again, so it names the reviewer the next review will actually use: when the
  // app window regains focus, when it becomes visible again, and when the tooltip opens.
  await run.check("Settings left open while Codex is signed out or back in: returning to the app, or opening the tooltip, reads Codex again and the row names the reviewer the next review uses", async () => {
    const seen = [];
    const triggers = {
      "window focus": page => page.evaluate(() => window.dispatchEvent(new Event("focus"))),
      "window visible": page => page.evaluate(() => document.dispatchEvent(new Event("visibilitychange"))),
      "tooltip opens": page => page.hover("#cx-info"),
    };
    // [case, before, after, who after, the tooltip's words after]
    for (const [name, before, after, who, words] of [
      ["signed out while open", "ready", "signedout", "Claude", "lapsed-signedout"],
      ["signed back in while open", "signedout", "ready", "Codex", "on"],
    ]) {
      for (const [how, act] of Object.entries(triggers)) {
        const page = await open("dark", { on: true, codex: before });
        await menu(page);
        assertEqual(await text(page, "#cx-who"), "Reviewing now: " + (who === "Codex" ? "Claude" : "Codex"), `${name}: as Settings opens`);
        await page.evaluate(c => window.__RICHOS_MOCK__.codexSet(c), after);
        await act(page);
        await page.waitForFunction(w => document.getElementById("cx-who").innerText.includes(w), who)
          .catch(async e => { throw new Error(`${name}, ${how}: the row still says "${await text(page, "#cx-who")}", the next review uses ${who} (${e.message.split("\n")[0]})`); });
        await page.hover("#cx-info");
        await page.waitForSelector("#cx-tip.is-shown");
        assertEqual((await shownWords(page)).map(plain), WORDS[words], `${name}, ${how}: the tooltip's words`);
        assertEqual(await page.getAttribute("#cx-switch", "aria-checked"), "true", `${name}, ${how}: the choice stands`);
        assertEqual((await calls(page)).filter(c => c.cmd === "codex_reviews_set"), [], `${name}, ${how}: nothing was turned on or off`);
        seen.push(`${name}/${how}: Reviewing now: ${who}`);
        await page.close();
      }
    }
    // With Settings shut, coming back asks nothing: the row is read when Settings opens.
    const page = await open("dark", { on: true, codex: "ready" });
    await menu(page);
    await page.click("#set-btn");
    await page.waitForSelector("#set-menu", { state: "hidden" });
    const before = (await calls(page)).length;
    await triggers["window focus"](page);
    await triggers["window visible"](page);
    assertEqual((await calls(page)).length, before, "Settings shut: no read on return");
    await page.close();
    return seen.join("; ");
  });

  // ---- 7. contrast and type, both themes ---------------------------------------------------
  for (const theme of ["dark", "light"]) {
    await run.check(theme + ": every state, tooltip open, meets AA contrast (text 4.5:1; the switch, the ⓘ and the rules 3:1) and the 16px floor", async () => {
      const worst = [];
      for (const [name, preset] of STATES) {
        // The nudge belonged to the two states that are no longer shown (NOT_SHOWN above).
        for (const how of ["hover"]) {
          const page = await open(theme, preset);
          await menu(page);
          await page.hover("#cx-info");
          await page.waitForSelector("#cx-tip.is-shown");
          await page.evaluate(() => Promise.all(document.getAnimations().filter(a => a.effect && a.effect.getTiming().iterations !== Infinity).map(a => a.finished.catch(() => null))));
          await page.addScriptTag({ content: contrast.pageScript() });
          const result = await page.evaluate(() => {
            const C = window.__contrastMath, failures = [];
            // WebKit gives a color-mix() as color(srgb r g b / a); the shared parser takes rgb() only.
            const parse = s => C.parseCssColor(s) || (m => m && { r: +m[1] * 255, g: +m[2] * 255, b: +m[3] * 255, a: m[4] === undefined ? 1 : +m[4] })(
              /^color\(srgb ([\d.e-]+) ([\d.e-]+) ([\d.e-]+)(?: \/ ([\d.e-]+))?\)$/.exec(String(s).trim()));
            const ground = el => { let bg = { r: 0, g: 0, b: 0, a: 0 }; const chain = []; for (let p = el; p; p = p.parentElement) chain.unshift(p);
              for (const p of chain) { const c = parse(getComputedStyle(p).backgroundColor); if (!c) failures.push({ unparsed: getComputedStyle(p).backgroundColor }); else bg = C.compositeOver(c, bg); } return bg; };
            const ratio = (paint, on) => { const bg = ground(on), c = parse(paint); if (!c) { failures.push({ unparsed: paint }); return 0; } return C.round2(C.contrastRatio(C.compositeOver(c, bg), bg)); };
            let worstText = 99, nodes = 0;
            const root = document.getElementById("set-codex");
            for (const e of [root, ...root.querySelectorAll("*")]) {
              if (!e.getClientRects().length || e.closest("[hidden]")) continue;
              if (![...e.childNodes].some(n => n.nodeType === Node.TEXT_NODE && n.textContent.trim())) continue;
              const st = getComputedStyle(e), r = ratio(st.color, e), size = parseFloat(st.fontSize);
              nodes++; worstText = Math.min(worstText, r);
              if (r < 4.5) failures.push({ text: e.textContent.trim().slice(0, 50), ratio: r });
              if (size < 16) failures.push({ text: e.textContent.trim().slice(0, 50), size });
            }
            const sw = document.getElementById("cx-switch"), sst = getComputedStyle(sw), knob = getComputedStyle(sw, "::after");
            const ind = {};
            if (sw.getAttribute("aria-checked") === "true") {
              ind.track = ratio(sst.backgroundColor, sw.parentElement);
              // The knob on the gold track.
              const gold = parse(sst.backgroundColor), k = parse(knob.backgroundColor);
              ind.knob = C.round2(C.contrastRatio(C.compositeOver(k, gold), gold));
            } else {
              ind.track = ratio(sst.borderTopColor, sw.parentElement);
              const hollow = parse(knob.backgroundColor).a === 0;
              ind.knob = hollow ? ratio(knob.boxShadow.match(/rgba?\([^)]+\)|color\([^)]+\)/)[0], sw.parentElement) : ratio(knob.backgroundColor, sw.parentElement);
            }
            const info = document.getElementById("cx-info");
            ind.info = ratio(getComputedStyle(info).color, info);
            for (const r of document.querySelectorAll("#cx-tip > div:not([hidden]) .cx-aware, #cx-tip > div:not([hidden]) .cx-why")) {
              ind[r.className] = ratio(getComputedStyle(r).borderLeftColor, r.parentElement);
            }
            for (const [k, v] of Object.entries(ind)) if (v < 3) failures.push({ indicator: k, ratio: v });
            return { failures, worstText, nodes, ind };
          });
          assertEqual(result.failures, [], `${theme} ${name} (${how})`);
          assert(result.nodes >= 4, `${theme} ${name}: EMPTY INVENTORY (${result.nodes} text nodes)`);
          worst.push(`${name}/${how} text ${result.worstText}, ${Object.entries(result.ind).map(([k, v]) => k + " " + v).join(", ")}`);
          await page.close();
        }
      }
      return worst.join("; ");
    });
  }

  await run.check("no renderer errors", async () => assertEqual(errors, []));
  await browser.close();
  process.exitCode = run.report() ? 1 : 0;
}
main().catch(e => { console.error(e); process.exitCode = 1; });

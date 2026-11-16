// KEEP CHAT ALIVE (CEO, 2026-11-16; rounds 81 to 83): "this checkbox would be in every chat but
// off by default and in long-running threads where the user knows that he'll be back the next
// morning, he could just check that checkbox to keep the cache for that particular thread." And:
// "There also needs to be one of those info icons (like we have in settings) that on hover shows
// what that checkbox actually does and what it is for."
//
// Runs the real renderer in headless WebKit over the mock bridge, whose `set_keep_chat_alive`
// keeps each chat's choice the way nav.rs does. What a checked box makes the app do (one
// keep-alive message after 55 idle minutes) is richos-core's, tested there.
"use strict";
const path = require("path");
const { loadPlaywright, createRun, assert, assertEqual, UI_DIR, leaveHome, bootSettled, awaitSettled, flushFrames } = require("./lib/harness");
const contrast = require("./lib/contrast");
const APP = "file://" + path.join(UI_DIR, "index.html");

// Round 83's words, word for word (richos-hq design/mockups/rounds/round-83/README.md).
const WORDS = [
  "Keep chat alive",
  "Keeps this chat ready while you’re away. Come back hours later (or the next morning) and Rich answers right away.",
  "It saves some of your Claude Code quota. Picking up a ready chat costs less than reading the whole chat again. " +
    "Meant for long chats you’ll come back to within about a day. Keeping a chat ready costs a little every hour. " +
    "If you stay away longer than a day and a half when RichOS is on Opus (or 3 days when it’s on Fable), it costs more than it saves.",
  "Only while your Mac is on. A Mac that stays on keeps the chat ready all night. A closed laptop is asleep, so nothing runs.",
];

async function openApp(context, opts = {}) {
  const page = await context.newPage();
  page.errors = [];
  page.on("pageerror", (error) => page.errors.push(String(error)));
  await page.addInitScript(({ failSave, meter }) => {
    window.calls = [];
    if (meter) window.localStorage.setItem("richos-mock-config", JSON.stringify({ show_context_meter: true }));
    Object.defineProperty(window, "RichBridge", {
      configurable: true,
      set(bridge) {
        const invoke = bridge.invoke.bind(bridge);
        bridge.invoke = async (name, args) => {
          if (name === "set_keep_chat_alive") {
            window.calls.push([name, args]);
            if (failSave) throw Error("fixture disk refused write");
          }
          return invoke(name, args);
        };
        Object.defineProperty(window, "RichBridge", { value: bridge, writable: true, configurable: true });
      },
    });
  }, { failSave: !!opts.failSave, meter: !!opts.meter });
  await page.goto(APP);
  await leaveHome(page);
  await bootSettled(page);
  await openChat(page, "general");
  return page;
}

const keep = (page) => page.evaluate(() => window.RichKeepAlive.state());
async function openChat(page, id) {
  await page.evaluate((thread) => openThread(thread), id);
  await awaitSettled(page);
  await page.waitForFunction((thread) => window.RichKeepAlive.state().thread === thread && window.RichKeepAlive.state().visible, id);
  await flushFrames(page);
}

/// The contrast of `selector`'s computed `property` against what is painted behind it: every
/// ancestor's background composited in order. A fill or a border is measured against the ground
/// its element stands on.
async function contrastOf(page, selector, property) {
  await page.addScriptTag({ content: contrast.pageScript() });
  return page.evaluate(({ selector, property }) => {
    const C = window.__contrastMath;
    // WebKit gives a color-mix() as color(srgb r g b / a); the shared parser takes rgb() only.
    const parse = (s) => C.parseCssColor(s) || ((m) => m && { r: +m[1] * 255, g: +m[2] * 255, b: +m[3] * 255, a: m[4] === undefined ? 1 : +m[4] })(
      /^color\(srgb ([\d.e-]+) ([\d.e-]+) ([\d.e-]+)(?: \/ ([\d.e-]+))?\)$/.exec(String(s).trim()));
    const el = document.querySelector(selector);
    let ground = { r: 0, g: 0, b: 0, a: 0 };
    const chain = [];
    for (let p = property === "color" ? el : el.parentElement; p; p = p.parentElement) chain.unshift(p);
    for (const p of chain) {
      const c = parse(getComputedStyle(p).backgroundColor);
      if (!c) throw new Error("unparsed ground: " + getComputedStyle(p).backgroundColor);
      ground = C.compositeOver(c, ground);
    }
    const paint = parse(getComputedStyle(el)[property]);
    if (!paint) throw new Error("unparsed paint: " + getComputedStyle(el)[property]);
    return C.contrastRatio(C.compositeOver(paint, ground), ground);
  }, { selector, property });
}

async function main() {
  const run = createRun("Keep chat alive");
  const { webkit } = loadPlaywright();
  const browser = await webkit.launch();
  const context = await browser.newContext({ viewport: { width: 1440, height: 900 }, colorScheme: "dark", reducedMotion: "reduce" });
  const page = await openApp(context);

  await run.check("every chat has the checkbox in its header, right of its name, and it is off until checked", async () => {
    for (const chat of ["general", "acme", "hiring"]) {
      await openChat(page, chat);
      const state = await keep(page);
      assertEqual([state.visible, state.checked], [true, false], chat + ": the box");
    }
    await openChat(page, "general");
    const place = await page.evaluate(() => {
      const box = document.getElementById("kca").getBoundingClientRect();
      const crumb = document.getElementById("scope-crumb").getBoundingClientRect();
      const header = document.getElementById("stage-header").getBoundingClientRect();
      return { label: document.querySelector("#kca .kca-said").textContent, rightOfName: box.left >= crumb.right,
        inHeader: box.top >= header.top && box.bottom <= header.bottom + 1,
        px: parseFloat(getComputedStyle(document.querySelector("#kca .kca-word")).fontSize),
        held: document.querySelector("#kca .kca-hold").textContent,
        holdShows: getComputedStyle(document.querySelector("#kca .kca-hold")).visibility,
        holdSaid: document.querySelector("#kca .kca-hold").getAttribute("aria-hidden"),
        info: document.getElementById("kca-info").getAttribute("aria-label") };
    });
    assertEqual(place.label, "Keep chat alive", "the label");
    // the hidden bold copy that holds the label's width says the label's own words, and is
    // neither seen nor read out
    assertEqual(place.held, place.label, "the words of the copy that holds the label's width");
    assertEqual([place.holdShows, place.holdSaid], ["hidden", "true"], "the copy that holds the label's width is seen or read out");
    assert(place.rightOfName && place.inHeader, "the checkbox is not in the header right of the chat's name: " + JSON.stringify(place));
    assert(place.px >= 16, "the label is " + place.px + "px, under 16px");
    assertEqual(place.info, "About Keep chat alive", "the info icon's name");
    assertEqual((await keep(page)).kept, [], "a chat is kept before anybody checked its box");
  });

  await run.check("the info icon shows what it does and what it is for, word for word, and never flips the box", async () => {
    await page.hover("#kca-info");
    await page.waitForSelector("#kca-tip.is-shown");
    const words = await page.evaluate(() => Array.from(document.querySelectorAll("#kca-tip p"), (p) => p.textContent.replace(/\s+/g, " ").trim()));
    assertEqual(words, WORDS, "the words");
    assertEqual((await keep(page)).checked, false, "hovering the info icon checked the box");
    await page.click("#kca-info");
    assertEqual((await keep(page)).checked, false, "pressing the info icon checked the box");
    await page.click("#kca-tip");
    assertEqual((await keep(page)).checked, false, "pressing the words checked the box");
    await page.keyboard.press("Escape");
    assertEqual((await keep(page)).tipOpen, false, "Escape left the words open");
    // the keyboard reaches it too
    await page.focus("#kca-info");
    await page.waitForSelector("#kca-tip.is-shown");
    await page.keyboard.press("Escape");
  });

  await run.check("checking it keeps that chat and no other, and the choice is saved for that chat", async () => {
    await page.evaluate(() => { window.calls.length = 0; });
    await page.click("#kca .kca-check");
    assertEqual((await keep(page)).checked, true, "the box did not check");
    assertEqual(await page.evaluate(() => window.calls), [["set_keep_chat_alive", { threadId: "general", kept: true }]], "what was saved");
    await openChat(page, "acme");
    assertEqual((await keep(page)).checked, false, "another chat's box was checked with it");
    await openChat(page, "general");
    assertEqual((await keep(page)).checked, true, "the chat's own choice did not come back with it");
    assertEqual(await page.evaluate(() => window.RichBridge.invoke("get_keep_chat_alive", { threadId: "general" })), true, "the saved choice");
    assertEqual(await page.evaluate(() => window.RichBridge.invoke("get_keep_chat_alive", { threadId: "acme" })), false, "the other chat's saved choice");
  });

  await run.check("unchecking it lets the chat go again", async () => {
    await page.click("#kca .kca-check");
    assertEqual((await keep(page)).checked, false, "the box did not uncheck");
    assertEqual(await page.evaluate(() => window.RichBridge.invoke("get_keep_chat_alive", { threadId: "general" })), false, "the saved choice");
    assertEqual(await page.evaluate(() => window.calls[window.calls.length - 1]), ["set_keep_chat_alive", { threadId: "general", kept: false }], "what was saved");
  });

  await run.check("outside a chat there is no checkbox", async () => {
    // a company's own screen, where a new chat is started: no chat is open
    await page.evaluate(() => showEntityView("northwind"));
    await page.waitForFunction(() => !window.RichKeepAlive.state().visible);
    assertEqual(await page.locator("#kca").isVisible(), false, "the checkbox shows where no chat is open");
    await openChat(page, "general");
    // a page that takes the stage over the chat (RichPass): its header carries the page's name only
    await page.click("#nr-pass");
    await page.waitForSelector("#page-crumb:not([hidden])");
    assertEqual(await page.locator("#kca").isVisible(), false, "the checkbox shows on the RichPass page");
    await page.click("#nr-chats");
    await page.waitForSelector("#page-crumb", { state: "hidden" });
    assertEqual(await page.locator("#kca").isVisible(), true, "the checkbox did not come back with the chat");
  });

  await run.check("readable in both themes: the label, the box, the info icon and every word clear WCAG AA", async () => {
    const ratios = {};
    const measure = async (theme, rows) => {
      for (const [name, selector, property, floor] of rows) {
        const ratio = await contrastOf(page, selector, property);
        ratios[theme + " " + name] = Math.round(ratio * 100) / 100;
        assert(ratio >= floor, `${theme} ${name}: ${ratio.toFixed(2)}:1 is under ${floor}:1`);
      }
    };
    for (const theme of ["dark", "light"]) {
      await page.evaluate((t) => { document.documentElement.setAttribute("data-theme", t); }, theme);
      // at rest: the pointer away from the label, whose hover is its checked ink
      await page.mouse.move(720, 520);
      await flushFrames(page);
      await measure(theme, [["label", "#kca .kca-word", "color", 4.5], ["box border", "#kca-box", "borderTopColor", 3],
        ["info icon", "#kca-info", "color", 3]]);
      await page.focus("#kca-info");
      await page.waitForSelector("#kca-tip.is-shown");
      await measure(theme, [["words: heading", "#kca-tip .kca-tip-head", "color", 4.5], ["words: what it does", "#kca-tip .cx-p", "color", 4.5],
        ["words: quota", "#kca-tip .cx-aware p", "color", 4.5], ["words: only while your Mac is on", "#kca-tip .cx-unseen", "color", 4.5]]);
      await page.keyboard.press("Escape");
      await page.click("#kca .kca-check");
      await flushFrames(page);
      await measure(theme, [["label, checked", "#kca .kca-word", "color", 4.5], ["box, checked", "#kca-box", "backgroundColor", 3]]);
      await page.click("#kca .kca-check");
    }
    await page.evaluate(() => { document.documentElement.setAttribute("data-theme", "dark"); });
    return JSON.stringify(ratios);
  });

  await run.check("with the context meter on at 1440 x 900 the checkbox stands beside the whole meter, and Output stays one press away", async () => {
    const metered = await openApp(context, { meter: true });
    try {
      await metered.waitForFunction(() => window.RichContextMeter.state().visible);
      await flushFrames(metered);
      const row = await metered.evaluate(() => {
        const box = document.getElementById("kca").getBoundingClientRect();
        const meter = document.getElementById("ctx-meter").getBoundingClientRect();
        const crumb = document.getElementById("scope-crumb");
        return { sameRow: Math.abs((box.top + box.bottom) / 2 - (meter.top + meter.bottom) / 2) < 2, clear: meter.left >= box.right,
          gauges: Array.from(document.querySelectorAll("#ctx-meter .cm-bar"), (g) => g.getBoundingClientRect().width > 0),
          nameWhole: crumb.scrollWidth <= crumb.clientWidth + 1,
          headerOutput: document.getElementById("out-top").getBoundingClientRect().width > 0,
          composerOutput: document.getElementById("out-bottom").getBoundingClientRect().width > 0,
          state: window.RichKeepAlive.state() };
      });
      assert(row.sameRow && row.clear, "the checkbox is not beside the meter on its row: " + JSON.stringify(row));
      assertEqual(row.gauges, [true, true], "the meter lost its gauges to make room");
      assert(row.nameWhole, "the chat's name was cut to make room");
      assertEqual([row.state.under, row.state.noOut], [false, true], "the header's give-way step");
      assertEqual([row.headerOutput, row.composerOutput], [false, true], "Output: gone from the header, still beside the message box");
      // a narrower window: the checkbox stands under the chat's name and the meter keeps its form
      await metered.setViewportSize({ width: 1280, height: 800 });
      await metered.waitForFunction(() => window.RichKeepAlive.state().under);
      const narrow = await metered.evaluate(() => {
        const box = document.getElementById("kca").getBoundingClientRect();
        const crumb = document.getElementById("scope-crumb").getBoundingClientRect();
        const header = document.getElementById("stage-header").getBoundingClientRect();
        return { under: box.top >= crumb.bottom - 1, inHeader: box.bottom <= header.bottom + 1 };
      });
      assert(narrow.under && narrow.inHeader, "at 1280 x 800 the checkbox is not under the chat's name inside the header: " + JSON.stringify(narrow));
      assertEqual(metered.errors, [], "page errors");
    } finally { await metered.close(); }
  });

  await run.check("with the meter off the header keeps its Output button", async () => {
    const state = await keep(page);
    assertEqual([state.under, state.noOut], [false, false], "the header's give-way step");
    assert(await page.locator("#out-top").isVisible(), "the header's Output is gone with the meter off");
  });

  await run.check("a choice that could not be saved puts the box back and says so", async () => {
    const failing = await openApp(context, { failSave: true });
    try {
      await failing.click("#kca .kca-check");
      await failing.waitForFunction(() => !document.getElementById("kca-box").checked);
      assertEqual(await failing.locator("#bug-toast").textContent(), "That could not be saved, so Keep chat alive is as it was.", "what was said");
      assertEqual(failing.errors, [], "page errors");
    } finally { await failing.close(); }
  });

  await run.check("no page error anywhere in the walk", async () => { assertEqual(page.errors, [], "page errors"); });
  await context.close();
  await browser.close();
  return run.report();
}
main().then((failed) => { process.exitCode = failed ? 1 : 0; });

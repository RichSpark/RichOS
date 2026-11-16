// THE CONTEXT METER (CEO, 2026-11-16; round 80, carried by round 83): "a "context meter" switch
// (somewhere at the bottom of the settings, off by default). Switching that on, would make the
// context window of Rich and Stu visible in each thread like so: "50% 500k/1000k"".
//
// Runs the real renderer and settings shell in headless WebKit. The figures are a fixture's: the
// bridge answers `chat_context` per chat and delivers `rich://context-meter` as a lease would.
"use strict";
const path = require("path");
const { loadPlaywright, createRun, assert, assertEqual, UI_DIR, leaveHome, bootSettled, awaitSettled, flushFrames } = require("./lib/harness");
const contrast = require("./lib/contrast");
const APP = "file://" + path.join(UI_DIR, "index.html");

/// The contrast of `selector`'s computed `property` against what is painted behind it: every
/// ancestor's background composited in order (the header's own ground included).
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
    for (let p = property === "backgroundColor" ? el.parentElement.parentElement : el; p; p = p.parentElement) chain.unshift(p);
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

const FIGURES = {
  general: { rich: { used: 500000, size: 1000000 }, team: { used: 212000, size: 1000000 } },
  acme: { rich: { used: 87200, size: 200000 }, team: { used: null, size: null } },
};

async function openApp(context, opts = {}) {
  const page = await context.newPage();
  page.errors = [];
  page.on("pageerror", (error) => page.errors.push(String(error)));
  await page.addInitScript(({ figures, failSave }) => {
    window.calls = [];
    window.fixture = { figures, failSave };
    Object.defineProperty(window, "RichBridge", {
      configurable: true,
      set(bridge) {
        const invoke = bridge.invoke.bind(bridge), listen = bridge.listen.bind(bridge);
        bridge.listen = (name, callback) => {
          // A session's stream changes the store first and tells the window second, as the app does.
          if (name === "rich://context-meter") window.deliverContext = (value) => {
            window.fixture.figures[value.threadId] = { rich: value.rich, team: value.team };
            callback({ payload: value });
          };
          return listen(name, callback);
        };
        bridge.invoke = async (name, args) => {
          if (name === "set_context_meter" || name === "chat_context") window.calls.push([name, args]);
          if (name === "chat_context") {
            const id = args.threadId;
            const f = window.fixture.figures[id] || { rich: { used: null, size: null }, team: { used: null, size: null } };
            return { threadId: id, rich: f.rich, team: f.team };
          }
          if (name === "set_context_meter" && window.fixture.failSave) throw Error("fixture disk refused write");
          return invoke(name, args);
        };
        Object.defineProperty(window, "RichBridge", { value: bridge, writable: true, configurable: true });
      },
    });
  }, { figures: FIGURES, failSave: !!opts.failSave });
  await page.goto(APP);
  await leaveHome(page);
  await bootSettled(page);
  await page.evaluate(() => openThread("general"));
  await awaitSettled(page);
  await flushFrames(page);
  return page;
}

const meter = (page) => page.evaluate(() => window.RichContextMeter.state());
async function openThread1440(page) {
  await page.setViewportSize({ width: 1440, height: 900 });
  await page.waitForFunction(() => !document.getElementById("stage-header").classList.contains("cm-compact"));
  await flushFrames(page);
}
async function openSettings(page) {
  if ((await page.getAttribute("#set-btn", "aria-expanded")) !== "true") await page.click("#set-btn");
  await page.waitForSelector("#set-ctxmeter");
}
async function closeSettings(page) {
  if ((await page.getAttribute("#set-btn", "aria-expanded")) === "true") await page.keyboard.press("Escape");
  await flushFrames(page);
}
async function switchOn(page) {
  await openSettings(page);
  if (!(await page.locator("#set-ctxmeter").isChecked())) await page.click("#set-ctxmeter");
  await closeSettings(page);
  await page.waitForFunction(() => window.RichContextMeter.state().visible);
  await flushFrames(page);
}

async function main() {
  const run = createRun("The context meter");
  const { webkit } = loadPlaywright();
  const browser = await webkit.launch();
  // Reduced motion: a figure is drawn at once instead of easing, so a check reads it when it asks.
  const context = await browser.newContext({ viewport: { width: 1440, height: 900 }, colorScheme: "dark", reducedMotion: "reduce" });
  let page = await openApp(context);

  await run.check("off by default: no meter in the chat, and the switch is the last setting of the first column, off", async () => {
    const state = await meter(page);
    assertEqual([state.on, state.visible], [false, false], "the meter shows before it is switched on");
    assertEqual(await page.locator("#ctx-meter").isVisible(), false, "the meter is on screen");
    await openSettings(page);
    assertEqual(await page.locator("#set-ctxmeter").isChecked(), false, "the switch is on by default");
    const place = await page.evaluate(() => {
      const col = document.querySelector('#set-menu .sm-col[aria-labelledby="set-group-look"]');
      const block = document.getElementById("set-ctxmeter-block");
      return { last: col.lastElementChild === block, label: block.querySelector(".set-name").textContent,
        hint: block.querySelector(".popover-hint").textContent };
    });
    assert(place.last, "Context meter is not the last setting of How it looks");
    assertEqual(place.label, "Context meter", "the switch's name");
    assertEqual(place.hint, "Shows how full Rich’s context window is at the top of each chat, for when he talks with you and when he directs your team.", "the switch's one line");
    await closeSettings(page);
  });

  await run.check("switched on: the chat's header reads its two real figures in his format, and the choice is saved", async () => {
    await page.evaluate(() => { window.calls.length = 0; });
    await switchOn(page);
    const state = await meter(page);
    assertEqual(state.text, ["Rich 50% 500k/1000k", "Team 21% 212k/1000k"], "the two readouts");
    const saved = await page.evaluate(() => window.calls.filter((c) => c[0] === "set_context_meter"));
    assertEqual(saved, [["set_context_meter", { show: true }]], "the choice was saved once");
    const place = await page.evaluate(() => {
      const m = document.getElementById("ctx-meter").getBoundingClientRect();
      const crumb = document.getElementById("scope-crumb").getBoundingClientRect();
      const header = document.getElementById("stage-header").getBoundingClientRect();
      return { rightOfName: m.left >= crumb.right, inHeader: m.top >= header.top && m.bottom <= header.bottom + 1 };
    });
    assert(place.rightOfName && place.inHeader, "the meter is not in the header right of the chat's name: " + JSON.stringify(place));
    return state.text.join(" | ");
  });

  await run.check("a session's context grows: the figure of the chat on screen follows its own stream", async () => {
    await page.evaluate(() => deliverContext({ threadId: "general", rich: { used: 734000, size: 1000000 }, team: { used: 212000, size: 1000000 } }));
    await flushFrames(page);
    assertEqual((await meter(page)).text, ["Rich 73% 734k/1000k", "Team 21% 212k/1000k"], "Rich's figure after a turn");
    // another chat's turn is not this chat's
    await page.evaluate(() => deliverContext({ threadId: "acme", rich: { used: 99000, size: 200000 }, team: { used: 5000, size: 1000000 } }));
    await flushFrames(page);
    assertEqual((await meter(page)).text, ["Rich 73% 734k/1000k", "Team 21% 212k/1000k"], "another chat's figures were drawn here");
  });

  await run.check("the card gives the exact figures and says what a drop means", async () => {
    await page.focus("#ctx-meter");
    await page.waitForSelector("#ctx-card:not([hidden])");
    const card = await page.evaluate(() => ({
      title: document.querySelector("#ctx-card .cc-title").textContent,
      rows: Array.from(document.querySelectorAll("#ctx-card .cc-row"), (r) => r.textContent.replace(/\s+/g, " ").trim()),
      note: document.querySelector("#ctx-card .cc-note").textContent,
    }));
    assertEqual(card.title, "Context window in this chat", "the card's title");
    assertEqual(card.rows, ["Rich, talking with you734,000 of 1,000,000 tokens", "Rich, directing your team212,000 of 1,000,000 tokens"], "the exact figures");
    assertEqual(card.note, "Each number only grows as the chat goes on. If one ever drops by itself, the chat was compacted.", "the note");
    await page.keyboard.press("Escape");
    assertEqual(await page.locator("#ctx-card").isVisible(), false, "Escape left the card open");
  });

  await run.check("each chat has its own figures: its own window, and \"not read yet\", never a made-up 0, for a session with no reading", async () => {
    await page.evaluate(() => openThread("acme"));
    await awaitSettled(page);
    await page.waitForFunction(() => window.RichContextMeter.state().thread === "acme");
    await flushFrames(page);
    assertEqual((await meter(page)).text, ["Rich 50% 99k/200k", "Team 1% 5k/1000k"], "the other chat, as its own stream last said");
    await page.evaluate(() => openThread("hiring"));
    await awaitSettled(page);
    await page.waitForFunction(() => window.RichContextMeter.state().thread === "hiring");
    await flushFrames(page);
    // A long chat from before the meter existed has no reading until its next reply. It is not
    // empty, so it is not shown as 0% (second review of 7224adeda, 2026-11-16).
    assertEqual((await meter(page)).text, ["Rich not read yet", "Team not read yet"], "a chat whose sessions have not been read yet");
    assertEqual(
      await page.locator("#ctx-meter").getAttribute("aria-label"),
      "Context window. Rich, talking with you: not read yet. Rich, directing your team: not read yet.",
      "what the meter says of a chat not read yet"
    );
    assertEqual(await page.evaluate(() => Array.from(document.querySelectorAll("#ctx-meter .cm-fill"), (f) => f.style.width)), ["0%", "0%"], "the gauges of a chat not read yet");
    await page.locator("#ctx-meter").focus();
    assertEqual(
      await page.evaluate(() => Array.from(document.querySelectorAll("#ctx-card .cc-fig"), (f) => f.textContent)),
      ["Not read yet", "Not read yet"],
      "the card of a chat not read yet"
    );
    await page.keyboard.press("Escape");
    // a page that takes the stage over the chat (RichPass) is not a chat: no meter
    await page.click("#nr-pass");
    await page.waitForSelector("#page-crumb:not([hidden])");
    assertEqual(await page.locator("#ctx-meter").isVisible(), false, "the meter shows on the RichPass page");
    await page.click("#nr-chats");
    await page.waitForSelector("#page-crumb", { state: "hidden" });
    assertEqual(await page.locator("#ctx-meter").isVisible(), true, "the meter did not come back with the chat");
    await page.evaluate(() => openThread("general"));
    await awaitSettled(page);
    await page.waitForFunction(() => window.RichContextMeter.state().thread === "general");
  });

  await run.check("in a narrower chat the meter takes round 80's forms: first without gauges, then two rows, and the header is as tall as ever", async () => {
    const form = () => page.evaluate(() => {
      const m = document.getElementById("ctx-meter").getBoundingClientRect();
      return { gauges: Array.from(document.querySelectorAll("#ctx-meter .cm-bar"), (g) => g.getBoundingClientRect().width > 0),
        height: Math.round(m.height), header: Math.round(document.getElementById("stage-header").getBoundingClientRect().height),
        headerOutput: document.getElementById("out-top").getBoundingClientRect().width > 0,
        composerOutput: document.getElementById("out-bottom").getBoundingClientRect().width > 0 };
    });
    await openThread1440(page);
    assertEqual(await form(), { gauges: [true, true], height: 36, header: 52, headerOutput: false, composerOutput: true }, "1440 x 900: one row with gauges, beside Keep chat alive");
    await page.setViewportSize({ width: 1280, height: 800 });
    await page.waitForFunction(() => document.getElementById("stage-header").classList.contains("cm-compact"));
    await flushFrames(page);
    const narrower = await form();
    assertEqual([narrower.gauges, narrower.height, narrower.header], [[false, false], 36, 52], "1280 x 800: one row, the gauges gone");
    await page.setViewportSize({ width: 1024, height: 700 });
    await page.waitForFunction(() => document.getElementById("stage-header").classList.contains("cm-stacked"));
    await flushFrames(page);
    const smallest = await form();
    assertEqual([smallest.gauges, smallest.height, smallest.header, smallest.headerOutput, smallest.composerOutput],
      [[false, false], 44, 52, false, true], "1024 x 700: the two readouts one above the other, Output beside the message box");
    assertEqual((await meter(page)).text, ["Rich 73% 734k/1000k", "Team 21% 212k/1000k"], "the figures, in his format, at every width");
    await openThread1440(page);
    return JSON.stringify({ narrower, smallest });
  });

  await run.check("readable in both themes: every readout and the gauge clear WCAG AA on the header", async () => {
    const ratios = {};
    for (const theme of ["dark", "light"]) {
      await page.evaluate((t) => { document.documentElement.setAttribute("data-theme", t); }, theme);
      await flushFrames(page);
      await page.focus("#ctx-meter");
      await page.waitForSelector("#ctx-card:not([hidden])");
      for (const [name, selector, floor] of [["who", "#ctx-meter .cm-who", 4.5], ["percent", "#ctx-meter .cm-pct", 4.5],
        ["tokens", "#ctx-meter .cm-tok", 4.5], ["gauge", "#ctx-meter .cm-fill", 3], ["card title", "#ctx-card .cc-title", 4.5],
        ["card who", "#ctx-card .cc-who", 4.5], ["card figure", "#ctx-card .cc-fig", 4.5], ["card note", "#ctx-card .cc-note", 4.5]]) {
        const ratio = await contrastOf(page, selector, name === "gauge" ? "backgroundColor" : "color");
        ratios[theme + " " + name] = Math.round(ratio * 100) / 100;
        assert(ratio >= floor, `${theme} ${name}: ${ratio.toFixed(2)}:1 is under ${floor}:1`);
      }
    }
    await page.evaluate(() => { document.documentElement.setAttribute("data-theme", "dark"); });
    return JSON.stringify(ratios);
  });

  await run.check("kept across a relaunch, and switching it off takes the meter out of every chat", async () => {
    await page.close();
    page = await openApp(context);
    await page.waitForFunction(() => window.RichContextMeter.state().visible);
    assertEqual((await meter(page)).on, true, "the choice did not survive a relaunch");
    await openSettings(page);
    assertEqual(await page.locator("#set-ctxmeter").isChecked(), true, "the switch does not show the saved choice");
    await page.click("#set-ctxmeter");
    await closeSettings(page);
    assertEqual((await meter(page)).visible, false, "the meter stayed after it was switched off");
    assertEqual(await page.locator("#ctx-meter").isVisible(), false, "the meter is still on screen");
  });

  await run.check("a choice that could not be saved is put back and said", async () => {
    const failing = await openApp(context, { failSave: true });
    try {
      await openSettings(failing);
      await failing.click("#set-ctxmeter");
      await failing.waitForFunction(() => !document.getElementById("set-ctxmeter").checked);
      assertEqual((await meter(failing)).on, false, "the meter stayed on over a save that failed");
      assertEqual(await failing.locator("#bug-toast").textContent(), "The context meter setting could not be saved.", "what was said");
      assertEqual(failing.errors, [], "page errors");
    } finally { await failing.close(); }
  });

  await run.check("no page error anywhere in the walk", async () => { assertEqual(page.errors, [], "page errors"); });
  await context.close();
  await browser.close();
  return run.report();
}
main().then((failed) => { process.exitCode = failed ? 1 : 0; });

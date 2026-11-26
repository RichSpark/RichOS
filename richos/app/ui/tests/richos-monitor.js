// MY RICHOS (CEO, 2026-11-26; design round 109): "I need a way to track or see at a glance 1)
// whether or not tokens are being consumed now 2) the velocity with which they've been being
// consumed in the last say 1 minute and last 5 minutes and last 15 minutes 3) see more detail on
// those stats if desired 4) also see how much memory/RAM is being used now by each app of the
// RichOS suite individuall and all combined and what was the average RAM usage in the last 1-2
// minutes or so". "This whole feature would be off by default". On the build: "I really hope
// that those graphs can be drawn without using much CPU/RAM resources."
//
// Runs the real renderer and settings shell in headless WebKit. The figures are a fixture's in
// `my_richos.rs`'s shape (`richos_monitor`); what is counted and when it is Idle is proven by
// that module's own tests. This suite holds what the window does with them: off runs nothing,
// on asks once a second, the rail, the card, the details, light and dark contrast.
"use strict";
const path = require("path");
const { loadPlaywright, createRun, assert, assertEqual, UI_DIR, leaveHome, bootSettled, flushFrames } = require("./lib/harness");
const contrast = require("./lib/contrast");
const APP = "file://" + path.join(UI_DIR, "index.html");

/// The fixture: a burst (tokens being used now, about 1.26 million in the last minute), idle
/// (nothing for 22 minutes) or fresh (idle, My RichOS turned on 30 seconds ago, every graph's
/// bars on the clock edges `my_richos.rs` gives them), and memory for every app `suite_memory`
/// measures, in its order.
function installFixture({ scene, failSave }) {
  window.calls = [];
  window.fixture = { scene, failSave };
  const MB = 1048576;
  const line = (mb, sway) => {
    const hist = Array.from({ length: 60 }, (_, i) => Math.round((mb + sway * Math.sin(i / 7)) * MB));
    return { now: hist[59], avg: Math.round(hist.reduce((a, b) => a + b, 0) / 60), peak: Math.max.apply(null, hist), hist };
  };
  // In round 109's order, as `suite_memory::apps()` gives them; RichFinder and RichPass are pages
  // in RichOS's window (`within`), RichCleaner is not open.
  const APPS = [
    ["richos", "RichOS", 2, 1320, 30], ["richscribe", "RichScribe", 1, 604, 10], ["richbrief", "RichBrief", 1, 142, 3],
    ["richfiles", "RichFiles", 1, 118, 4], ["richglance", "RichGlance", 1, 93, 2], ["richfinder", "RichFinder", 0, 0, 0, "richos"],
    ["richpass", "RichPass", 0, 0, 0, "browser"], ["richcleaner", "RichCleaner", 0, 0, 0], ["richglance-capture", "RichGlance Capture", 0, 0, 0],
    ["window-gestures", "Window Gestures", 1, 21, 0], ["richdoctor", "RichDoctor", 1, 46, 1], ["claude-code", "Claude Code", 6, 1480, 60],
    ["codex", "Codex", 0, 0, 0],
  ];
  window.__RICHOS_MOCK_MONITOR__ = (args) => {
    window.calls.push(["richos_monitor", args]);
    const now = Date.now();
    const burst = window.fixture.scene === "burst";
    const bars = (n, v) => Array.from({ length: n }, (_, i) => (burst ? Math.round(v * (0.6 + 0.4 * Math.abs(Math.sin(i)))) : 0));
    const fresh = window.fixture.scene === "fresh";
    const edge = (ms) => (Math.floor(now / ms) + 1) * ms;
    const detail = args.detail
      ? {
          span_s: args.detail.token_span_s,
          series: { bars: bars(args.detail.token_bars, 300000), bar_ms: (args.detail.token_span_s * 1000) / args.detail.token_bars, ends_at_ms: fresh ? edge((args.detail.token_span_s * 1000) / args.detail.token_bars) : now + 1000 },
          sources: burst
            ? [
                { entity: "harbor", thread: "ecs", team: true, working: 6, model: "Fable", tokens: 2144000, per_min: 517000 },
                { entity: "northwind", thread: "memory", team: true, working: 4, model: "Fable", tokens: 1569000, per_min: 378000 },
                { entity: "harbor", thread: "general", team: false, working: 1, model: "Opus", tokens: 418000, per_min: 0 },
              ]
            : [],
          models: burst ? [{ name: "Fable", tokens: 3713000 }, { name: "Opus", tokens: 418000 }] : [],
          kinds: [
            { name: "Read from cache", tokens: burst ? 3553000 : 0 }, { name: "Saved to cache", tokens: burst ? 248000 : 0 },
            { name: "New input", tokens: burst ? 248000 : 0 }, { name: "Written output", tokens: burst ? 82000 : 0 },
          ],
        }
      : null;
    // RichGlance Capture closed a moment ago: its last two minutes are still its own.
    const closed = (id) => id === "richglance-capture"
      ? { now: 0, avg: 100 * MB, peak: 200 * MB, hist: [200 * MB, 100 * MB, 0] }
      : { now: 0, avg: 0, peak: 0, hist: [] };
    const apps = APPS.map(([id, name, processes, mb, sway, within]) => Object.assign({ id, name, processes, within: within || null }, processes ? line(mb, sway) : within ? { now: null, avg: null, peak: null, hist: [] } : closed(id)));
    const all = line(APPS.reduce((a, x) => a + x[3], 0), 80);
    return {
      on: true,
      now_ms: now,
      tokens: {
        since_ms: fresh ? now - 30000 : now - 2 * 3600 * 1000,
        last_ms: burst ? now - 2000 : now - 22 * 60 * 1000,
        active: burst,
        m1: burst ? 1262000 : 0, m5: burst ? 4131000 : 0, m15: burst ? 5230000 : 0,
        tile: bars(10, 140000), scale: burst ? 150000 : 0,
        card: args.card ? { bars: bars(60, 320000), bar_ms: 15000, ends_at_ms: fresh ? edge(15000) : now + 1000 } : null,
        detail,
      },
      memory: args.card || args.detail
        ? { status: "available", detail: "", at_ms: now, mac_total_bytes: 36 * 1024 * MB, apps, all,
            series: args.detail ? { span_s: args.detail.memory_span_s, list: Array.from({ length: args.detail.memory_points }, (_, i) => (fresh && i < args.detail.memory_points - 1 ? null : all.hist[i % 60])) } : null }
        : null,
    };
  };
  Object.defineProperty(window, "RichBridge", {
    configurable: true,
    set(bridge) {
      const invoke = bridge.invoke.bind(bridge);
      bridge.invoke = async (name, args) => {
        if (name === "set_richos_monitor") {
          window.calls.push([name, args]);
          if (window.fixture.failSave) throw Error("fixture disk refused write");
        }
        return invoke(name, args);
      };
      Object.defineProperty(window, "RichBridge", { value: bridge, writable: true, configurable: true });
    },
  });
}

async function openApp(context, opts = {}) {
  const page = await context.newPage();
  page.errors = [];
  page.on("pageerror", (error) => page.errors.push(String(error)));
  await page.addInitScript(installFixture, { scene: opts.scene || "burst", failSave: !!opts.failSave });
  await page.goto(APP);
  await leaveHome(page);
  await bootSettled(page);
  await flushFrames(page);
  return page;
}

const state = (page) => page.evaluate(() => window.RichosMonitor.state());
const reads = (page) => page.evaluate(() => window.__richosMonitorReads || 0);
async function openSettings(page) {
  if ((await page.getAttribute("#set-btn", "aria-expanded")) !== "true") await page.click("#set-btn");
  await page.waitForSelector("#set-richosmon");
}
async function closeSettings(page) {
  if ((await page.getAttribute("#set-btn", "aria-expanded")) === "true") await page.keyboard.press("Escape");
  await flushFrames(page);
}
async function switchTo(page, on) {
  await openSettings(page);
  if ((await page.locator("#set-richosmon").isChecked()) !== on) await page.click("#set-richosmon");
  await closeSettings(page);
  await page.waitForFunction((on) => window.RichosMonitor.state().on === on, on);
  await flushFrames(page);
}

/// The contrast of `selector`'s text against what is painted behind it: every ancestor's ground
/// composited in order.
async function contrastOf(page, selector) {
  await page.addScriptTag({ content: contrast.pageScript() });
  return page.evaluate((selector) => {
    const C = window.__contrastMath;
    const parse = (s) => C.parseCssColor(s) || ((m) => m && { r: +m[1] * 255, g: +m[2] * 255, b: +m[3] * 255, a: m[4] === undefined ? 1 : +m[4] })(
      /^color\(srgb ([\d.e-]+) ([\d.e-]+) ([\d.e-]+)(?: \/ ([\d.e-]+))?\)$/.exec(String(s).trim()));
    const el = document.querySelector(selector);
    if (!el) throw new Error("no " + selector);
    let ground = parse(getComputedStyle(document.documentElement).backgroundColor) || { r: 0, g: 0, b: 0, a: 0 };
    if (ground.a === 0) ground = parse(getComputedStyle(document.body).backgroundColor);
    const chain = [];
    for (let p = el; p && p !== document.documentElement; p = p.parentElement) chain.unshift(p);
    for (const p of chain) {
      const c = parse(getComputedStyle(p).backgroundColor);
      if (c) ground = C.compositeOver(c, ground);
    }
    let paint = parse(getComputedStyle(el).color);
    let opacity = 1;
    for (let p = el; p; p = p.parentElement) opacity *= +getComputedStyle(p).opacity;
    paint = Object.assign({}, paint, { a: paint.a * opacity });
    return C.contrastRatio(C.compositeOver(paint, ground), ground);
  }, selector);
}
/// A CSS custom property's color against another's, both read from the theme in force.
async function tokenContrast(page, fg, bg) {
  await page.addScriptTag({ content: contrast.pageScript() });
  return page.evaluate(({ fg, bg }) => {
    const C = window.__contrastMath;
    const probe = document.createElement("i");
    document.body.appendChild(probe);
    probe.style.color = "var(" + fg + ")";
    probe.style.backgroundColor = "var(" + bg + ")";
    const f = C.parseCssColor(getComputedStyle(probe).color), b = C.parseCssColor(getComputedStyle(probe).backgroundColor);
    probe.remove();
    return C.contrastRatio(C.compositeOver(f, b), b);
  }, { fg, bg });
}

/// What of `scope`'s text and drawings reaches past the left or right edge of the box it is
/// meant to stay inside (`edge`: its content box, inside the padding). A name an ancestor
/// shortens with an ellipsis is held to that ancestor's box, not to its full text.
async function cutOff(page, scope, edge) {
  return page.evaluate(({ scope, edge }) => {
    const box = document.querySelector(edge);
    const b = box.getBoundingClientRect();
    const cs = getComputedStyle(box);
    const left = b.left + box.clientLeft + parseFloat(cs.paddingLeft) - 1;
    const right = b.left + box.clientLeft + box.clientWidth - parseFloat(cs.paddingRight) + 1;
    const root = document.querySelector(scope);
    const cut = [];
    const walker = document.createTreeWalker(root, NodeFilter.SHOW_TEXT);
    for (let t; (t = walker.nextNode()); ) {
      if (!t.textContent.trim()) continue;
      let r = null;
      for (let e = t.parentElement; e && e !== root; e = e.parentElement) {
        if (getComputedStyle(e).textOverflow === "ellipsis") { r = e.getBoundingClientRect(); break; }
      }
      if (!r) { const range = document.createRange(); range.selectNodeContents(t); r = range.getBoundingClientRect(); }
      if (!r.width) continue;
      if (r.left < left || r.right > right) cut.push(t.textContent.trim() + " (" + Math.round(r.left) + ".." + Math.round(r.right) + ")");
    }
    for (const c of root.querySelectorAll("canvas, .rom-bar")) {
      const r = c.getBoundingClientRect();
      if (r.width && (r.left < left || r.right > right)) cut.push(c.tagName.toLowerCase() + " (" + Math.round(r.left) + ".." + Math.round(r.right) + ")");
    }
    return { left: Math.round(left + 1), right: Math.round(right - 1), cut, sideways: box.scrollWidth - box.clientWidth };
  }, { scope, edge });
}

async function main() {
  const run = createRun("My RichOS");
  const { webkit } = loadPlaywright();
  const browser = await webkit.launch();
  const context = await browser.newContext({ viewport: { width: 1440, height: 900 }, colorScheme: "dark", reducedMotion: "reduce" });
  let page = await openApp(context);

  await run.check("off by default: nothing on the rail, nothing in the document, no timer and no read", async () => {
    const before = await reads(page);
    // load-bound: a stretch with nothing happening; a slow host can only make it read fewer, never a false "read"
    await page.waitForTimeout(2500);
    const s = await state(page);
    assertEqual([s.on, s.built, s.timer], [false, false, false], "the monitor runs before it is switched on");
    assertEqual(await reads(page), before, "the app was asked for My RichOS while it was off");
    assertEqual(before, 0, "My RichOS was read at all while off");
    assertEqual(await page.evaluate(() => [!!document.getElementById("rom-square"), !!document.getElementById("rom-card"), !!document.getElementById("rom-sheet"), document.getElementById("nr-richos").hidden]),
      [false, false, false, true], "what is in the document while off");
    await openSettings(page);
    assertEqual(await page.locator("#set-richosmon").isChecked(), false, "the switch is on by default");
    const place = await page.evaluate(() => {
      const col = document.querySelector('#set-menu .sm-col[aria-labelledby="set-group-look"]');
      const block = document.getElementById("set-richosmon-block");
      return { last: col.lastElementChild === block, label: block.querySelector(".set-name").textContent, hint: block.querySelector(".popover-hint").textContent };
    });
    assert(place.last, "Show My RichOS is not the last setting of How it looks");
    assertEqual(place.label, "Show My RichOS", "the switch's name");
    assertEqual(place.hint, "Puts a live monitor on the navigation rail, under My Mac: whether tokens are being used right now and how fast and how much memory the RichOS apps are using.", "the switch's one line");
    await closeSettings(page);
  });

  await run.check("switched on: the square sits under My Mac and above the name circle, and the choice is saved once", async () => {
    await page.evaluate(() => { window.calls.length = 0; });
    await switchTo(page, true);
    const saved = await page.evaluate(() => window.calls.filter((c) => c[0] === "set_richos_monitor"));
    assertEqual(saved, [["set_richos_monitor", { show: true }]], "the choice was saved once");
    const place = await page.evaluate(() => {
      const mac = document.getElementById("res-square").getBoundingClientRect();
      const rom = document.getElementById("rom-square").getBoundingClientRect();
      const me = document.getElementById("nr-me").getBoundingClientRect();
      return { below: rom.top >= mac.bottom - 1, above: rom.bottom <= me.top + 1, shown: rom.width > 0 };
    });
    assert(place.below && place.above && place.shown, "the square is not between My Mac and the name circle: " + JSON.stringify(place));
  });

  await run.check("on the rail without hovering: tokens being used now and the speed over the last minute, no unit", async () => {
    await page.waitForFunction(() => window.RichosMonitor.state().rail.state === "using");
    const s = await state(page);
    assertEqual(s.rail.speed, "1.3M", "the speed on the rail");
    assertEqual(s.rail.label, "My RichOS: using tokens, 1.3 million in the last minute", "what the square says");
    assertEqual(s.timer, true, "no timer while on");
  });

  await run.check("on, its timer keeps asking the app, for the rail only while the card is closed", async () => {
    await page.evaluate(() => { window.calls.length = 0; });
    const before = await reads(page);
    await page.waitForFunction((before) => (window.__richosMonitorReads || 0) >= before + 2, before);
    const asks = await page.evaluate(() => window.calls.filter((c) => c[0] === "richos_monitor").map((c) => c[1]));
    assert(asks.length >= 2 && asks.every((a) => a.card === false && a.detail === null), "the rail asked for more than it shows: " + JSON.stringify(asks));
  });

  await run.check("the card on hover: speeds over 1, 5 and 15 minutes, memory for every app and its 2-minute average", async () => {
    await page.hover("#rom-square");
    await page.waitForSelector("#rom-card.is-open");
    await page.waitForFunction(() => document.querySelector('#rom-card [data-k="mem-now"]').textContent !== "");
    const card = await page.evaluate(() => ({
      title: document.querySelector("#rom-card .res-card-title").textContent,
      chip: document.querySelector('#rom-card [data-k="chip"]').textContent,
      hero: document.querySelector('#rom-card [data-k="hero"]').textContent + " " + document.querySelector('#rom-card [data-k="hero-unit"]').textContent,
      wins: Array.from(document.querySelectorAll('#rom-card [data-k="wins"] > *'), (n) => n.textContent),
      memNow: document.querySelector('#rom-card [data-k="mem-now"]').textContent,
      memAvg: document.querySelector('#rom-card [data-k="mem-avg"]').textContent,
      apps: Array.from(document.querySelectorAll('#rom-card [data-k="apps"] > *'), (n) => n.textContent).filter(Boolean),
    }));
    assertEqual(card.title, "My RichOS", "the card's title");
    assertEqual(card.chip, "Using tokens", "the chip");
    assertEqual(card.hero, "1.26M tokens a minute", "the speed");
    assertEqual(card.wins, ["", "Speed", "Used", "Last minute", "1.26M/min", "1.26M", "Last 5 minutes", "826k/min", "4.13M", "Last 15 minutes", "349k/min", "5.23M"], "the three windows");
    assertEqual(card.memNow, "3.8 GB", "all RichOS apps now");
    assert(/GB$/.test(card.memAvg), "the 2-minute average: " + card.memAvg);
    assertEqual(card.apps.slice(0, 4), ["RichOS", "1.3 GB", "RichScribe", "612 MB"], "the first apps");
    assert(card.apps.includes("RichGlance Capture") && card.apps.includes("Not open"), "an app that is not open: " + card.apps.join(" | "));
    assert(card.apps.includes("RichFinder") && card.apps.includes("In RichOS"), "a page in RichOS's window: " + card.apps.join(" | "));
    assert(card.apps.includes("RichPass") && card.apps.includes("In your browser"), "RichPass, a document in his browser: " + card.apps.join(" | "));
    const names = await page.evaluate(() => Array.from(document.querySelectorAll("#rom-card .rom-sec"), (b) => b.getAttribute("aria-label")));
    assertEqual(names[0], "Tokens: 1.26M tokens a minute. Last minute 1.26M used, last 5 minutes 4.13M, last 15 minutes 5.23M. Open the details", "what VoiceOver reads of the token half");
    assert(names[1].startsWith("Memory, all RichOS apps: 3.8 GB now, 2-minute average") && names[1].includes("RichOS 1.3 GB") && names[1].endsWith("Open the details"), "what VoiceOver reads of the memory half: " + names[1]);
    const asked = await page.evaluate(() => window.calls.filter((c) => c[0] === "richos_monitor").pop()[1]);
    assertEqual(asked.card, true, "the open card did not ask for its graph and the memory");
  });

  await run.check("the details: Tokens names the chats as the sidebar does, the model and kinds; Memory lists each app", async () => {
    await page.click('#rom-card [data-go="tokens"]');
    await page.waitForSelector("#rom-sheet:not([hidden])");
    await page.waitForFunction(() => document.querySelector('#rom-sheet [data-s="t-where"] .rom-where-row'));
    const t = await page.evaluate(() => ({
      tab: window.RichosMonitor.state().sheet,
      where: Array.from(document.querySelectorAll("#rom-sheet .rom-where-row"), (r) => r.textContent.replace(/\s+/g, " ").trim()),
      models: document.querySelector('#rom-sheet [data-s="t-models"]').textContent,
      kinds: document.querySelector('#rom-sheet [data-s="t-kinds"]').textContent,
      title: document.querySelector('#rom-sheet [data-s="t-where-title"]').textContent,
    }));
    assertEqual(t.tab, "tokens", "the tab the token half opened");
    assertEqual(t.title, "Where they went, the last 15 minutes", "the list's title");
    assertEqual(t.where[0], "ECS architectureHarbor Analytics · Team, 6 workingFable517k/min2.14M", "the busiest chat");
    assertEqual(t.where[2], "RunningHarbor Analytics · RichOpusIdle418k", "a chat that is idle now");
    assert(t.models.startsWith("Fable") && t.kinds.startsWith("Read from cache"), "the splits: " + t.models + " / " + t.kinds);
    const asked = await page.evaluate(() => window.calls.filter((c) => c[0] === "richos_monitor").pop()[1]);
    assertEqual(asked.detail, { token_span_s: 900, token_bars: 60, memory_span_s: 120, memory_points: 60 }, "what the details asked for");
    // 24 hours
    await page.click('#rom-sheet [data-seg="tok"] [data-v="d1"]');
    await page.waitForFunction(() => window.calls.filter((c) => c[0] === "richos_monitor").pop()[1].detail.token_span_s === 86400);
    // Memory
    await page.click('#rom-sheet [data-tab="memory"]');
    await page.waitForFunction(() => document.querySelectorAll("#rom-sheet .rom-app-row").length > 0);
    const m = await page.evaluate(() => ({
      big: document.querySelector('#rom-sheet [data-s="m-big"]').textContent,
      line: document.querySelector('#rom-sheet [data-s="m-line"]').textContent,
      rows: Array.from(document.querySelectorAll("#rom-sheet .rom-app-name"), (n) => n.textContent),
      share: document.querySelector('#rom-sheet [data-s="m-ledger"]').textContent,
    }));
    assertEqual(m.big, "3.8 GB", "all apps now");
    assertEqual(m.line, "All RichOS apps together. 9 of 13 are open.", "the line under it");
    assertEqual(m.rows, ["RichOS", "RichScribe", "RichBrief", "RichFiles", "RichGlance", "RichFinder", "RichPass", "RichCleaner", "RichGlance Capture", "Window Gestures", "RichDoctor", "Claude Code", "Codex", "All together"], "each app");
    assert(/Share of this Mac11% of 36 GB/.test(m.share), "the share of the Mac: " + m.share);
    const capture = await page.evaluate(() => Array.from(document.querySelectorAll('#rom-sheet [data-app="richglance-capture"] [data-c]'), (n) => n.textContent));
    assertEqual(capture, ["Not open", "100 MB", "200 MB"], "an app closed within the two minutes keeps its figures");
    assertEqual(await page.evaluate(() => document.getElementById("rom-sheet").getAttribute("aria-modal")), null, "the details claim aria-modal, which leaves Settings out of the accessibility tree");
    // Settings over the details: Escape closes Settings, and the details stay
    await page.click("#set-btn");
    await page.waitForSelector("#set-richosmon");
    await page.keyboard.press("Escape");
    await page.waitForFunction(() => document.getElementById("set-btn").getAttribute("aria-expanded") !== "true");
    assertEqual(await page.evaluate(() => window.RichosMonitor.state().sheet), "memory", "Escape on Settings closed the details under it");
    assertEqual(await page.evaluate(() => document.querySelectorAll("#rom-sheet .rom-ico-r use").length + "/" + document.querySelectorAll("#rom-sheet .rom-ico-r path").length), "0/2", "the RichOS mark is drawn from <use>, which a recording does not draw");
    await page.keyboard.press("Escape");
    await page.waitForFunction(() => window.RichosMonitor.state().sheet === null);
    assertEqual(await page.evaluate(() => document.activeElement && document.activeElement.id), "rom-square", "closing the details from Memory left focus nowhere");
    assertEqual(await page.evaluate(() => window.RichosMonitor.state().card), false, "the card reopened on the returned focus");
    // a press on a rail item goes where it goes: the details step aside for RichFinder
    await page.evaluate(() => window.RichosMonitor.openSheet("tokens"));
    await page.waitForFunction(() => window.RichosMonitor.state().sheet === "tokens");
    await page.click("#nr-finder");
    await page.waitForFunction(() => window.RichosMonitor.state().sheet === null);
    await page.keyboard.press("Escape");
    // ... for My Mac, and for Command-K
    await page.evaluate(() => window.RichosMonitor.openSheet("memory"));
    await page.waitForFunction(() => window.RichosMonitor.state().sheet === "memory");
    await page.click("#res-square");
    await page.waitForFunction(() => window.RichosMonitor.state().sheet === null);
    await page.keyboard.press("Escape");
    await page.evaluate(() => window.RichosMonitor.openSheet("tokens"));
    await page.waitForFunction(() => window.RichosMonitor.state().sheet === "tokens");
    await page.keyboard.press("Meta+K");
    await page.waitForFunction(() => window.RichosMonitor.state().sheet === null);
    await page.keyboard.press("Escape");
    // Bust a bug reports the screen as it is: the details are still there when the report starts
    await page.evaluate(() => window.RichosMonitor.openSheet("tokens"));
    await page.waitForFunction(() => window.RichosMonitor.state().sheet === "tokens");
    const seen = await page.evaluate(() => {
      const real = window.RichSettings.bustABug;
      let at = "not started";
      window.RichSettings.bustABug = () => { at = String(window.RichosMonitor.state().sheet); };
      document.getElementById("nr-bug").click();
      window.RichSettings.bustABug = real;
      return at;
    });
    assertEqual(seen, "tokens", "Bust a bug started after the details were closed, so its picture loses them");
    await page.keyboard.press("Escape");
    await page.waitForFunction(() => window.RichosMonitor.state().sheet === null);
  });

  // The handover review at cbb69f6ab: with My RichOS on for 30 seconds and 24 hours picked,
  // pointing 12 hours back read "12:00 AM to 12:15 AM 0 used, 0 a minute". Time before it was
  // turned on (also after a relaunch) was never counted: it is drawn as nothing, not as the floor
  // mark of a measured zero, and pointing at it says it was not recorded.
  await run.check("time before My RichOS was turned on is drawn and named as not recorded, never as zero use", async () => {
    const fresh = await openApp(context, { scene: "fresh" });
    try {
      await switchTo(fresh, true);
      await fresh.waitForFunction(() => window.RichosMonitor.state().rail);
      // a canvas's floor row at bar `i`: the floor mark of a measured zero, or nothing
      const floorAt = (sel, h, n, i) => fresh.evaluate(({ sel, h, n, i }) => {
        const c = document.querySelector(sel);
        const dpr = c.width / c.clientWidth;
        const step = (c.clientWidth - 8) / n;
        const x = Math.round(4 + i * step) + 1;
        const px = c.getContext("2d").getImageData(Math.round(x * dpr), Math.round((h - 5) * dpr), 1, 1).data;
        const g = c.getContext("2d").getImageData(Math.round(x * dpr), Math.round((h - 8) * dpr), 1, 1).data;
        return { floor: Array.from(px.slice(0, 3)).join(), ground: Array.from(g.slice(0, 3)).join() };
      }, { sel, h, n, i });
      // the details over 24 hours
      await fresh.evaluate(() => window.RichosMonitor.openSheet("tokens"));
      await fresh.waitForSelector("#rom-sheet:not([hidden])");
      await fresh.click('#rom-sheet [data-seg="tok"] [data-v="d1"]');
      await fresh.waitForFunction(() => /Counted since/.test(document.querySelector('#rom-sheet [data-s="t-read"]').textContent));
      const n = 96; // TOK_RANGES' 24 hours
      const summary = await fresh.textContent('#rom-sheet [data-s="t-read"]');
      const old = await floorAt('#rom-sheet [data-s="t-graph"]', 128, n, 48);
      const recent = await floorAt('#rom-sheet [data-s="t-graph"]', 128, n, n - 1);
      assertEqual(old.floor, old.ground, "12 hours back, before the switch, is drawn as a measured zero");
      assert(recent.floor !== recent.ground, "the counted, idle last stretch lost its floor mark: " + JSON.stringify(recent));
      // pointing 12 hours back
      const box = await fresh.locator('#rom-sheet [data-s="t-graph"]').boundingBox();
      await fresh.mouse.move(box.x + 4 + ((48.5 / n) * (box.width - 8)), box.y + box.height / 2);
      await fresh.waitForFunction(() => !/Counted since/.test(document.querySelector('#rom-sheet [data-s="t-read"]').textContent));
      const pointed = await fresh.textContent('#rom-sheet [data-s="t-read"]');
      assert(/Not recorded: My RichOS was off then\.$/.test(pointed), "pointing 12 hours back: " + pointed);
      assert(!/0 used/.test(pointed), "pointing 12 hours back reads as zero use: " + pointed);
      // and the newest stretch, which the switch went on inside or before, reads as counted
      await fresh.mouse.move(box.x + box.width - 5, box.y + box.height / 2);
      await fresh.waitForFunction(() => / used, /.test(document.querySelector('#rom-sheet [data-s="t-read"]').textContent));
      const newest = await fresh.textContent('#rom-sheet [data-s="t-read"]');
      assert(!/Not recorded/.test(newest), "the counted stretch says not recorded: " + newest);
      // the memory graph over an hour: a point from before the switch says so too
      await fresh.click('#rom-sheet [data-tab="memory"]');
      await fresh.click('#rom-sheet [data-seg="mem"] [data-v="h1"]');
      await fresh.waitForFunction(() => /Counted since/.test(document.querySelector('#rom-sheet [data-s="m-read"]').textContent));
      const mbox = await fresh.locator('#rom-sheet [data-s="m-graph"]').boundingBox();
      await fresh.mouse.move(mbox.x + mbox.width / 2, mbox.y + mbox.height / 2);
      await fresh.waitForFunction(() => !/Counted since/.test(document.querySelector('#rom-sheet [data-s="m-read"]').textContent));
      const memPointed = await fresh.textContent('#rom-sheet [data-s="m-read"]');
      assert(/Not recorded: My RichOS was off then\.$/.test(memPointed), "pointing half an hour back in memory: " + memPointed);
      await fresh.mouse.move(mbox.x + mbox.width / 2, mbox.y - 80);
      await fresh.keyboard.press("Escape");
      await fresh.waitForFunction(() => window.RichosMonitor.state().sheet === null);
      // the card's 15 minutes: the bars from before the switch carry no floor mark, the newest does
      await fresh.hover("#rom-square");
      await fresh.waitForSelector("#rom-card.is-open");
      await fresh.waitForFunction(() => document.querySelector('#rom-card [data-k="mem-now"]').textContent !== "");
      const cardOld = await floorAt('#rom-card [data-k="tok-graph"]', 56, 60, 10);
      const cardNew = await floorAt('#rom-card [data-k="tok-graph"]', 56, 60, 59);
      assertEqual(cardOld.floor, cardOld.ground, "the card draws 12 minutes before the switch as a measured zero");
      assert(cardNew.floor !== cardNew.ground, "the card's newest stretch, counted and idle, lost its floor mark: " + JSON.stringify(cardNew));
      assertEqual(fresh.errors, [], "page errors");
      return "summary: " + summary.trim() + " / pointed: " + pointed.trim() + " / newest: " + newest.trim() + " / memory: " + memPointed.trim();
    } finally { await fresh.close(); }
  });

  // The handover review at cbb69f6ab: Bust a bug started over Memory details opened its panel
  // over the readings ("1400px window: bug dock x=930..1382, right-aligned Highest readings end
  // at x=981"). The details leave the panel its room, as every other overlay does, and at the
  // smallest window (1024px) every reading still fits beside it, in both tabs.
  await run.check("Bust a bug over the details: its panel has its room and every reading stays in view beside it", async () => {
    const out = [];
    for (const [width, height] of [[1400, 900], [1024, 700]]) {
      const p = await openApp(context);
      try {
        await p.setViewportSize({ width, height });
        await switchTo(p, true);
        await p.waitForFunction(() => window.RichosMonitor.state().rail);
        await p.evaluate(() => window.RichosMonitor.openSheet("memory"));
        await p.waitForFunction(() => document.querySelector('#rom-sheet [data-app="richos"] [data-c="now"]') && document.querySelector('#rom-sheet [data-app="richos"] [data-c="now"]').textContent);
        await p.click("#nr-bug");
        await p.waitForSelector("#bugdock");
        await p.waitForFunction(() => document.body.classList.contains("bug-dock-open"));
        assertEqual(await p.evaluate(() => window.RichosMonitor.state().sheet), "memory", "Bust a bug closed the details it reports");
        await p.evaluate(() => Promise.all(document.getAnimations().filter((a) => a.effect && a.effect.getTiming().iterations !== Infinity).map((a) => a.finished.catch(() => null))));
        // one more reading, so every graph is drawn at the width the details have now
        await p.evaluate(() => window.RichosMonitor.repaint());
        for (const tab of ["memory", "tokens"]) {
          if (tab === "tokens") {
            await p.click('#rom-sheet [data-tab="tokens"]');
            await p.waitForFunction(() => document.querySelector('#rom-sheet [data-s="t-where"] .rom-where-row'));
          }
          const place = await p.evaluate(() => ({ dock: document.getElementById("bugdock").getBoundingClientRect().left, sheet: document.getElementById("rom-sheet").getBoundingClientRect().right }));
          assert(place.sheet <= place.dock, width + "px, " + tab + ": the details reach under Bust a bug's panel: details end at x=" + place.sheet + ", the panel starts at x=" + place.dock);
          // the header: its title and chip, its tabs and its close button stay on the details
          const head = await p.evaluate(() => {
            const r = (sel) => { const b = document.querySelector(sel).getBoundingClientRect(); return [Math.round(b.left), Math.round(b.right)]; };
            return { sheet: r("#rom-sheet"), tabs: r("#rom-sheet .rom-tabs"), close: r("#rom-close") };
          });
          assert(head.tabs[1] <= head.sheet[1] && head.close[0] >= head.sheet[0] && head.close[1] <= head.sheet[1], width + "px, " + tab + ": the tabs or the close button leave the details: " + JSON.stringify(head));
          assertEqual((await cutOff(p, "#rom-sheet .rom-sh-head", "#rom-sheet")).cut, [], width + "px, " + tab + ": the details' title or chip cut off");
          const fit = await cutOff(p, '#rom-sheet [data-pane="' + tab + '"]', "#rom-sheet .rom-sh-body");
          assertEqual(fit.cut, [], width + "px, " + tab + ": readings cut off at the details' edge (" + fit.left + ".." + fit.right + ")");
          assert(fit.sideways <= 1, width + "px, " + tab + ": the details need a sideways scroll of " + fit.sideways + "px");
          out.push(width + "px " + tab + ": details end x=" + Math.round(place.sheet) + ", panel starts x=" + Math.round(place.dock));
        }
        assertEqual(p.errors, [], "page errors");
      } finally { await p.close(); }
    }
    return out.join("; ");
  });

  await run.check("contrast, dark: the rail's figure, the card and the details clear WCAG AA", async () => {
    const out = {};
    out.speed = await contrastOf(page, '#rom-square [data-k="speed"]');
    out.bars = await tokenContrast(page, "--rom-flow", "--res-ground");
    await page.click("#rom-square");
    await page.waitForSelector("#rom-card.is-open");
    out.cardRow = await contrastOf(page, '#rom-card [data-k="wins"] span');
    out.cardApp = await contrastOf(page, '#rom-card [data-k="apps"] span');
    out.cardFoot = await contrastOf(page, '#rom-card .res-foot span');
    await page.click('#rom-card [data-go="memory"]');
    await page.waitForSelector("#rom-sheet:not([hidden])");
    out.sheetLine = await contrastOf(page, '#rom-sheet [data-s="m-line"]');
    out.sheetAvg = await contrastOf(page, '#rom-sheet .rom-app-row [data-c="avg"]');
    await page.keyboard.press("Escape");
    for (const [k, v] of Object.entries(out)) assert(v >= (k === "bars" ? 3 : 4.5), k + " is " + v.toFixed(2));
    return Object.entries(out).map(([k, v]) => k + " " + v.toFixed(2)).join(", ");
  });

  await run.check("in a 700-pixel window the whole card can be reached: it stays inside the window and scrolls", async () => {
    await page.setViewportSize({ width: 1000, height: 700 });
    await page.mouse.move(600, 300);
    await page.waitForFunction(() => !window.RichosMonitor.state().card);
    await page.hover("#rom-square");
    await page.waitForSelector("#rom-card.is-open");
    await page.waitForFunction(() => document.querySelector('#rom-card [data-k="apps"]').children.length > 0);
    const box = await page.evaluate(() => {
      const c = document.getElementById("rom-card");
      const r = c.getBoundingClientRect();
      c.scrollTop = c.scrollHeight;
      const foot = c.querySelector(".res-foot").getBoundingClientRect();
      return { top: r.top, bottom: r.bottom, scrolls: c.scrollHeight > c.clientHeight, footShown: foot.bottom <= r.bottom + 1 && foot.top >= r.top - 1 };
    });
    assert(box.top >= 0 && box.bottom <= 700, "the card leaves the window: " + JSON.stringify(box));
    assert(box.scrolls && box.footShown, "the card's foot cannot be scrolled to: " + JSON.stringify(box));
    await page.mouse.move(600, 300);
    await page.setViewportSize({ width: 1440, height: 900 });
    return JSON.stringify(box);
  });

  await run.check("idle: the rail says Idle, the card says when tokens were last used", async () => {
    const idle = await openApp(context, { scene: "idle" });
    try {
      await idle.waitForFunction(() => window.RichosMonitor.state().rail && window.RichosMonitor.state().rail.state === "idle");
      const s = await state(idle);
      assertEqual([s.rail.speed, s.rail.label], ["Idle", "My RichOS: idle, no tokens being used"], "the idle rail");
      await idle.hover("#rom-square");
      await idle.waitForSelector("#rom-card.is-open");
      await idle.waitForFunction(() => document.querySelector('#rom-card [data-k="age"]').textContent.startsWith("Last used"));
      assertEqual(await idle.evaluate(() => [document.querySelector('#rom-card [data-k="chip"]').textContent, document.querySelector('#rom-card [data-k="age"]').textContent]),
        ["Idle", "Last used 22 minutes ago"], "the idle card");
      assertEqual(idle.errors, [], "page errors");
    } finally { await idle.close(); }
  });

  await run.check("light: the same text and indicators clear WCAG AA", async () => {
    const light = await browser.newContext({ viewport: { width: 1440, height: 900 }, colorScheme: "light", reducedMotion: "reduce" });
    const p = await openApp(light, { scene: "idle" });
    try {
      await switchTo(p, true);
      await p.waitForFunction(() => window.RichosMonitor.state().rail);
      const out = {};
      out.idle = await contrastOf(p, '#rom-square [data-k="speed"]');
      out.bars = await tokenContrast(p, "--rom-flow", "--res-ground");
      out.ring = await tokenContrast(p, "--nr-ink", "--res-ground");
      await p.click("#rom-square");
      await p.waitForSelector("#rom-card.is-open");
      await p.waitForFunction(() => document.querySelector('#rom-card [data-k="apps"] span'));
      out.cardRow = await contrastOf(p, '#rom-card [data-k="wins"] span');
      out.cardApp = await contrastOf(p, '#rom-card [data-k="apps"] span');
      out.cardOff = await contrastOf(p, '#rom-card [data-k="apps"] .is-off');
      out.cardFoot = await contrastOf(p, '#rom-card .res-foot span');
      for (const [k, v] of Object.entries(out)) assert(v >= (k === "bars" || k === "ring" ? 3 : 4.5), k + " is " + v.toFixed(2));
      assertEqual(await p.evaluate(() => document.documentElement.dataset.theme), "light", "the theme");
      return Object.entries(out).map(([k, v]) => k + " " + v.toFixed(2)).join(", ");
    } finally { await light.close(); }
  });

  await run.check("switched off: the square leaves the rail and the reads stop", async () => {
    await switchTo(page, false);
    const before = await reads(page);
    // load-bound: two timer periods with nothing expected; a slow host can only make it read fewer, never a false "read"
    await page.waitForTimeout(2200);
    const s = await state(page);
    assertEqual([s.on, s.timer, s.rail, s.card, s.sheet], [false, false, null, false, null], "after switching off");
    assertEqual(await reads(page), before, "the app was still asked after the switch went off");
    assertEqual(await page.locator("#rom-square").isVisible(), false, "the square is still on screen");
  });

  await run.check("a choice that could not be saved is put back and said", async () => {
    const failing = await openApp(context, { failSave: true });
    try {
      await openSettings(failing);
      await failing.click("#set-richosmon");
      await failing.waitForFunction(() => !document.getElementById("set-richosmon").checked);
      assertEqual((await state(failing)).on, false, "My RichOS stayed on over a save that failed");
      assertEqual(await failing.locator("#bug-toast").textContent(), "The My RichOS setting could not be saved.", "what was said");
      assertEqual(failing.errors, [], "page errors");
    } finally { await failing.close(); }
  });

  await run.check("no page error anywhere in the walk", async () => { assertEqual(page.errors, [], "page errors"); });
  await context.close();
  await browser.close();
  return run.report();
}
main().then((failed) => { process.exitCode = failed ? 1 : 0; });

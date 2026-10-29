// The live Mac resources square, driven through the real shell: Ink by default,
// with a persisted Readout view available through the context menu and keyboard.
// The data path is the app's own: `resources.js` asks the `system_resources` command, which in
// the app reads the native one-second sampler's cache. This harness has no sampler, so each
// check hands the mock a FIXED snapshot in `system_resources.rs`'s shape (live, busy, measuring,
// stale, unavailable, after sleep) — a fixture, not a sampler: nothing here moves on its own.
//
//    1  Ink is the default on a first run: the square is opposite the name at 34 px, no numbers
//    2  Ink and Readout, both themes: footer and card photographed (shots-resources/)
//    3  every text pair in the card, the readout and the menu clears 4.5:1, both themes,
//       computed with the repository's contrast arithmetic; the repository's own walk finds no
//       failure under the square, the card or the menu
//    4  every paint, from the canvas's own pixels, clears 3:1 against its ground; the square's
//       edge, the focus ring and the readout's keys clear 3:1 against the rail's painted pixels
//    5  right-click opens the menu (Ink checked), Readout switches the view, and the choice
//       survives a relaunch from the settings store alone
//    6  the keyboard: focus opens the card, Escape closes it with focus kept on the square;
//       Shift+F10 and the context-menu key open the same menu, arrows and Enter choose, Escape
//       leaves it, and focus comes back to the square; the selected chat never changes
//    7  Readout at the default 300 px rail does not cut the name, and the numbers never change
//       width between 8%, 88% and 100%
//    8  warming up, not current, unavailable and a failed read never show 0%
//    9  after sleep the gap is drawn as a gap labeled Asleep, never as zeros
//   10  hover and click open the card without moving focus or changing the selected chat
//
// Run: node resources.js            (RICHOS_SHOTS_REGENERATE=shots-resources to rewrite shots)

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
const SHOTS = path.join(__dirname, "shots-resources");
const NAME = "Alex Booster";
const TEXT_FLOOR = 4.5;
const PAINT_FLOOR = 3;

/// The snapshot the mock hands back, by kind. Runs in the page. Values are fixed functions of
/// the sample's index, so every read draws the same picture.
function installFixture(opts) {
  const GIB = 1073741824;
  const total = 36 * GIB;
  let calls = 0;
  let wokeAt = 0;
  const sample = (i, at, busy) => {
    const user = busy ? 64 + 16 * Math.sin(i / 5) : 18 + 11 * Math.sin(i / 4) + (i % 13 === 0 ? 18 : 0);
    const system = busy ? 14 + 3 * Math.cos(i / 3) : 6 + 2 * Math.cos(i / 3);
    return {
      sampled_at_ms: at, monotonic_ms: at, interval_ms: 1000,
      cpu_user_percent: user, cpu_system_percent: system, cpu_idle_percent: 100 - user - system,
      cpu_busy_percent: user + system,
      memory_used_bytes: Math.round((busy ? 32.1 : 22.4) * GIB), memory_total_bytes: total,
      memory_compressed_bytes: Math.round((busy ? 6.4 : 3.1) * GIB),
      memory_pressure: busy ? "warning" : "normal",
      swap_used_bytes: Math.round((busy ? 2.3 : 0.4) * GIB),
    };
  };
  const run = (n, endAt, busy) => {
    const h = [];
    for (let i = 0; i < n; i++) h.push(sample(i, endAt - (n - 1 - i) * 1000, busy));
    return h;
  };
  const snap = (status, extra) => Object.assign(
    { version: 1, instance_id: "fixture", status, age_ms: null, observed_monotonic_ms: Date.now(), current: null, history: [], detail: null },
    extra
  );
  window.__RICHOS_MOCK_RESOURCES__ = () => {
    calls++;
    const kind = window.__resourcesKind || opts.kind;
    const T = Date.now();
    if (kind === "live" || kind === "busy" || kind === "full") {
      const h = run(60, T - 200, kind === "busy");
      if (kind === "full") Object.assign(h[59], { cpu_user_percent: 88, cpu_system_percent: 12, cpu_busy_percent: 100, cpu_idle_percent: 0 });
      return snap("available", { age_ms: 200, current: h[59], history: h, observed_monotonic_ms: T });
    }
    if (kind === "measuring") return snap("warming_up", { detail: "Waiting for the first CPU interval." });
    if (kind === "stale") {
      return snap("stale", { age_ms: 7200, history: run(60, T - 7200, false), detail: "The sampler has not supplied a current reading." });
    }
    if (kind === "unavailable") return snap("unavailable", { history: run(48, T - 12400, false), detail: "macOS resource counters are unavailable." });
    if (kind === "fail") throw new Error("the resource sampler is not running");
    if (kind === "slept") {
      // The first read after waking is the sampler's own "fresh interval after sleep"; the next
      // carries 22 seconds before the sleep, a 14-second hole, and 23 seconds after it.
      if (calls === 1 || !wokeAt) {
        wokeAt = T;
        return snap("unavailable", { detail: "Starting a fresh CPU interval after sleep." });
      }
      const before = run(22, wokeAt - 14000, false);
      const after = run(23, wokeAt + 1000 + 22 * 1000, false);
      const h = before.concat(after);
      const last = h[h.length - 1];
      return snap("available", { age_ms: 200, current: last, history: h, observed_monotonic_ms: last.monotonic_ms + 200 });
    }
    return null;
  };
}

/// A browser context per theme, seeded once like an install: the theme, 100% type, the name.
/// A relaunch is a new page in the SAME context, which keeps the mock's durable config.
async function newContext(browser, theme) {
  const context = await browser.newContext({
    viewport: { width: 1440, height: 900 }, colorScheme: theme, reducedMotion: "reduce", deviceScaleFactor: 2,
  });
  await context.addInitScript((t) => {
    try {
      window.localStorage.setItem("richos-theme", t);
      window.localStorage.setItem("richos-font-scale", "100");
      if (!window.localStorage.getItem("richos-mock-config")) {
        window.localStorage.setItem("richos-mock-config", JSON.stringify({ theme: t, font_scale: 100, user_name: "Alex Booster" }));
      }
    } catch (_e) { /* the default theme */ }
  }, theme);
  return context;
}

async function openApp(context, kind) {
  const page = await context.newPage();
  page.__errors = [];
  page.on("pageerror", (e) => page.__errors.push(String(e)));
  await page.addInitScript(installFixture, { kind });
  await page.goto(APP);
  await leaveHome(page);
  await shellSettled(page);
  await page.waitForSelector("#res-square", { state: "visible" });
  await settleState(page, expectedState(kind));
  return page;
}

function expectedState(kind) {
  return { live: "live", busy: "live", full: "live", slept: "live", measuring: "measuring", stale: "stale", unavailable: "unavailable", fail: "unavailable" }[kind];
}

/// Two reads, then the drawn state: the square has absorbed the fixture and painted it.
async function settleState(page, want) {
  await page.evaluate(() => window.RichResources.pollNow());
  await page.evaluate(() => window.RichResources.pollNow());
  await page.waitForFunction((w) => document.getElementById("res-square").dataset.state === w, want, { timeout: 30000 });
  await flushFrames(page);
}

async function setKind(page, kind) {
  await page.evaluate((k) => { window.__resourcesKind = k; }, kind);
  await settleState(page, expectedState(kind));
}

const facts = (page) => page.evaluate(() => {
  const sq = document.getElementById("res-square");
  const tile = sq.querySelector(".res-tile").getBoundingClientRect();
  const name = document.getElementById("rail-user-name");
  const nr = name.getBoundingClientRect();
  const card = document.getElementById("res-card");
  const menu = document.getElementById("res-menu");
  return {
    view: sq.dataset.view,
    state: sq.dataset.state,
    tile: { w: tile.width, h: tile.height, left: tile.left, top: tile.top, right: tile.right, bottom: tile.bottom },
    name: { text: name.textContent, right: nr.right, top: nr.top, bottom: nr.bottom, truncated: name.scrollWidth > name.clientWidth },
    readoutShown: getComputedStyle(sq.querySelector(".res-readout")).display !== "none",
    readout: Array.from(sq.querySelectorAll(".res-num b")).map((b) => b.textContent),
    cardOpen: card.classList.contains("is-open"),
    cardText: card.innerText,
    menuOpen: !menu.hidden,
    checked: Array.from(menu.querySelectorAll('[aria-checked="true"]')).map((b) => b.dataset.view),
    focused: document.activeElement ? document.activeElement.id || document.activeElement.className : null,
    thread: typeof activeThreadId === "undefined" ? null : activeThreadId,
    stored: JSON.parse(localStorage.getItem("richos-mock-config") || "{}").resources_view || null,
    railWidth: document.getElementById("rail").getBoundingClientRect().width,
  };
});

/// The footer, and the card when it is open, as one picture.
async function footerShot(page, file) {
  const clip = await page.evaluate(() => {
    const rects = [document.getElementById("rail-footer").getBoundingClientRect()];
    const card = document.getElementById("res-card");
    if (card.classList.contains("is-open")) rects.push(card.getBoundingClientRect());
    const x = Math.min(...rects.map((r) => r.left)) - 8;
    const y = Math.min(...rects.map((r) => r.top)) - 8;
    const right = Math.max(...rects.map((r) => r.right)) + 8;
    const bottom = Math.max(...rects.map((r) => r.bottom)) + 8;
    return { x: Math.max(0, x), y: Math.max(0, y), width: right - Math.max(0, x), height: bottom - Math.max(0, y) };
  });
  const buf = await captureSettled(page, { screenshot: { clip } });
  fs.mkdirSync(SHOTS, { recursive: true });
  publishShot(buf, path.join(SHOTS, file));
  return buf;
}

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
  return toColor(top);
}

/// A color from a computed style ("rgb(...)") or a canvas ("#rrggbb"); lib/contrast.js parses
/// only the first.
function toColor(str) {
  const m = /^#([0-9a-f]{2})([0-9a-f]{2})([0-9a-f]{2})$/i.exec(String(str).trim());
  if (m) return { r: parseInt(m[1], 16), g: parseInt(m[2], 16), b: parseInt(m[3], 16), a: 1 };
  const c = C.parseCssColor(str);
  if (!c) throw new Error("not a color: " + str);
  return c;
}
const cssRgb = (c) => `rgb(${c.r}, ${c.g}, ${c.b})`;

/// Every text pair under a root, its color composited over the ground given. `bgFor(el)` names
/// the ground; the arithmetic is lib/contrast.js's, shipped into the page.
async function textPairs(page, selector, ground) {
  await page.evaluate(C.pageScript());
  return page.evaluate(({ selector, ground }) => {
    const M = window.__contrastMath;
    const out = [];
    for (const root of document.querySelectorAll(selector)) {
      const walker = document.createTreeWalker(root, NodeFilter.SHOW_TEXT);
      let n;
      while ((n = walker.nextNode())) {
        const text = n.textContent.trim();
        if (!text) continue;
        const el = n.parentElement;
        if (!el.getClientRects().length || getComputedStyle(el).visibility === "hidden") continue;
        const cs = getComputedStyle(el);
        let bg = ground ? M.parseCssColor(ground) : null;
        if (ground && !bg) throw new Error("the painted ground did not parse: " + ground);
        if (!bg) {
          for (let a = el; a; a = a.parentElement) {
            const c = M.parseCssColor(getComputedStyle(a).backgroundColor);
            if (c && c.a === 1) { bg = c; break; }
          }
        }
        const raw = M.parseCssColor(cs.color);
        const fg = M.compositeOver(raw, bg);
        out.push({ text: text.slice(0, 40), size: parseFloat(cs.fontSize), fg: M.hex(fg), bg: M.hex(bg), ratio: M.round2(M.contrastRatio(fg, bg)) });
      }
    }
    return out;
  }, { selector, ground: ground ? cssRgb(ground) : null });
}

/// Pixel colors painted in a canvas, counted, read back from the canvas itself.
async function canvasPaints(page, selector, names) {
  return page.evaluate(({ selector, names }) => {
    const c = document.querySelector(selector);
    const d = c.getContext("2d").getImageData(0, 0, c.width, c.height).data;
    const css = (n) => getComputedStyle(document.documentElement).getPropertyValue(n).trim();
    const toRgb = (v) => {
      const probe = document.createElement("canvas").getContext("2d");
      probe.fillStyle = v;
      const h = probe.fillStyle;
      return [parseInt(h.slice(1, 3), 16), parseInt(h.slice(3, 5), 16), parseInt(h.slice(5, 7), 16)];
    };
    const at = (x, y) => { const o = (y * c.width + x) * 4; return [d[o], d[o + 1], d[o + 2]]; };
    const ground = at(Math.round(c.width / 34) + 1, Math.round(c.height / 34) + 1);
    const out = { ground, paints: {} };
    for (const n of names) {
      const want = toRgb(css("--res-" + n));
      let count = 0;
      for (let i = 0; i < d.length; i += 4) {
        if (Math.abs(d[i] - want[0]) <= 2 && Math.abs(d[i + 1] - want[1]) <= 2 && Math.abs(d[i + 2] - want[2]) <= 2) count++;
      }
      out.paints[n] = { rgb: want, count };
    }
    return out;
  }, { selector, names });
}

const rgb = (a) => ({ r: a[0], g: a[1], b: a[2], a: 1 });

/// Pin the card with a click and take the pointer away, so the pin is what holds it open.
async function openCard(page) {
  await page.click("#res-square");
  await page.mouse.move(700, 450);
  await page.waitForFunction(() => document.getElementById("res-card").classList.contains("is-open"), null, { timeout: 30000 })
    .catch(async () => {
      const at = await page.evaluate(() => ({
        expanded: document.getElementById("res-square").getAttribute("aria-expanded"),
        menu: !document.getElementById("res-menu").hidden,
        focus: document.activeElement && (document.activeElement.id || document.activeElement.tagName),
      }));
      throw new Error("a click on the square did not pin the card open: " + JSON.stringify(at));
    });
  await flushFrames(page);
}

/// Unpin the card and take the pointer off the square, so hover is not what keeps it open.
async function closeCard(page) {
  await page.click("#res-square");
  await page.mouse.move(700, 450);
  await page.waitForFunction(() => !document.getElementById("res-card").classList.contains("is-open"), null, { timeout: 30000 });
}

async function main() {
  const run = createRun("The live Mac resources square (round 47): Ink, Readout, the card, the menu and honest states");
  const { webkit } = loadPlaywright();
  const browser = await webkit.launch();
  const report = { text: [], paints: [] };

  for (const theme of ["dark", "light"]) {
    const context = await newContext(browser, theme);
    let page = await openApp(context, "live");

    await run.check(`1 ${theme}  Ink is the default on a first run: 34 px, opposite the name, no numbers`, async () => {
      const f = await facts(page);
      assertEqual(f.view, "ink", "the view a first run opens in");
      assertEqual([f.tile.w, f.tile.h], [34, 34], "the square's size");
      assert(!f.readoutShown, "Ink shows no numbers");
      assert(f.tile.left > f.name.right, "the square is to the right of the name");
      assert(Math.abs((f.tile.top + f.tile.bottom) / 2 - (f.name.top + f.name.bottom) / 2) <= 2, "on the name's row");
      assertEqual(f.name.text, NAME, "the name shown");
      assert(!f.name.truncated, "the name is not cut");
      return `${f.tile.w}x${f.tile.h} at x=${f.tile.left}, name ends at ${Math.round(f.name.right)}`;
    });

    await run.check(`2 ${theme}  Ink and Readout photographed with the card pinned (shots-resources/)`, async () => {
      const done = [];
      await page.click("#res-square");
      await page.waitForFunction(() => document.getElementById("res-card").classList.contains("is-open"));
      await page.mouse.move(700, 450);
      await footerShot(page, `ink-${theme}.png`);
      done.push(`ink-${theme}.png`);
      await setKind(page, "busy");
      await footerShot(page, `ink-busy-${theme}.png`);
      done.push(`ink-busy-${theme}.png`);
      await setKind(page, "live");
      await page.evaluate(() => { document.getElementById("res-square").dataset.view = "readout"; });
      await flushFrames(page);
      await footerShot(page, `readout-${theme}.png`);
      done.push(`readout-${theme}.png`);
      await page.evaluate(() => { document.getElementById("res-square").dataset.view = "ink"; });
      for (const kind of ["measuring", "stale", "unavailable"]) {
        await setKind(page, kind);
        await footerShot(page, `${kind}-${theme}.png`);
        done.push(`${kind}-${theme}.png`);
      }
      await setKind(page, "live");
      await page.keyboard.press("Escape");
      await page.waitForFunction(() => !document.getElementById("res-card").classList.contains("is-open"));
      return done.join(", ");
    });

    await run.check(`3 ${theme}  every text pair in the card, the readout and the menu clears 4.5:1 (computed)`, async () => {
      const pairs = [];
      await page.evaluate(() => { document.getElementById("res-square").dataset.view = "readout"; });
      for (const kind of ["live", "busy", "measuring", "stale", "unavailable"]) {
        await setKind(page, kind);
        await openCard(page);
        for (const p of await textPairs(page, "#res-card")) pairs.push(Object.assign({ where: "card", kind }, p));
        const r = await page.evaluate(() => {
          const b = document.querySelector("#res-square .res-readout").getBoundingClientRect();
          return { x: b.left - 12, y: b.top - 2, width: 8, height: b.height + 4 };
        });
        const ground = await groundPixel(page, r);
        for (const p of await textPairs(page, "#res-square .res-readout", ground)) pairs.push(Object.assign({ where: "readout", kind }, p));
        // The repository's own walk: no failure and nothing unprovable under the square or card.
        await page.evaluate(C.pageScript());
        const walk = await page.evaluate((t) => window.__contrastProbe({ surface: "resources", theme: t }), theme);
        const mine = (o) => Object.values(o || {}).filter((x) => /res-/.test(x.selector || x.path || ""));
        assertEqual(mine(walk.failures), [], `${kind}: the repository's walk found a failing pair under the square or card`);
        assertEqual(mine(walk.unresolvable), [], `${kind}: the repository's walk could not prove a pair under the square or card`);
        await closeCard(page);
      }
      await page.evaluate(() => { document.getElementById("res-square").dataset.view = "ink"; });
      await setKind(page, "live");
      await page.click("#res-square", { button: "right" });
      await page.waitForSelector("#res-menu", { state: "visible" });
      for (const p of await textPairs(page, "#res-menu")) pairs.push(Object.assign({ where: "menu", kind: "live" }, p));
      await page.keyboard.press("Escape");
      for (const p of pairs) {
        assert(p.size >= 14, `${p.where} "${p.text}" is ${p.size}px, under the 14px floor`);
        assert(p.ratio >= TEXT_FLOOR, `${p.where} (${p.kind}) "${p.text}" ${p.fg} on ${p.bg} is ${p.ratio}:1, under ${TEXT_FLOOR}`);
      }
      const distinct = new Map();
      for (const p of pairs) {
        const k = `${p.where} ${p.fg} on ${p.bg} ${p.size}px`;
        if (!distinct.has(k) || distinct.get(k).ratio > p.ratio) distinct.set(k, p);
      }
      for (const [k, p] of distinct) report.text.push(`${theme} ${k} ${p.ratio}:1 ("${p.text}")`);
      return `${pairs.length} text runs, ${distinct.size} distinct pairs, worst ${Math.min(...pairs.map((p) => p.ratio))}:1`;
    });

    await run.check(`4 ${theme}  paints from the canvas pixels clear 3:1; edge, focus ring and keys clear 3:1 on the rail`, async () => {
      const lines = [];
      await page.evaluate(() => { document.getElementById("res-square").dataset.view = "readout"; });
      for (const [kind, names] of [["live", ["user", "system", "ram"]], ["busy", ["user", "system", "warn"]], ["stale", ["glyph"]], ["unavailable", ["glyph"]]]) {
        await setKind(page, kind);
        await openCard(page);
        for (const [where, sel] of [["square", "#res-square canvas"], ["card graph", "#res-card canvas"]]) {
          if (where === "card graph" && kind !== "live" && kind !== "busy") continue;
          // RAM is the square's own column; the card shows it as the bar, not in the graph.
          const want = where === "card graph" ? names.filter((n) => n === "user" || n === "system") : names;
          const got = await canvasPaints(page, sel, want);
          for (const n of want) {
            const p = got.paints[n];
            assert(p.count >= 4, `${kind} ${where}: no ${n} paint in the canvas (${p.count} px)`);
            const ratio = C.round2(C.contrastRatio(rgb(p.rgb), rgb(got.ground)));
            assert(ratio >= PAINT_FLOOR, `${kind} ${where}: ${n} ${C.hex(rgb(p.rgb))} on ${C.hex(rgb(got.ground))} is ${ratio}:1`);
            lines.push(`${theme} ${where} ${n} ${C.hex(rgb(p.rgb))} on ${C.hex(rgb(got.ground))} ${ratio}:1 (${kind})`);
          }
        }
        await closeCard(page);
      }
      await setKind(page, "busy");
      const geo = await page.evaluate(() => {
        const t = document.querySelector("#res-square .res-tile").getBoundingClientRect();
        const cs = getComputedStyle(document.querySelector("#res-square .res-tile"));
        const keys = Array.from(document.querySelectorAll("#res-square .res-key i")).map((i) => getComputedStyle(i).backgroundColor);
        const ring = getComputedStyle(document.documentElement).getPropertyValue("--accent").trim();
        return { t: { x: t.left, y: t.top, w: t.width, h: t.height }, edge: cs.borderTopColor, keys, ring };
      });
      // The rail's own painted pixels in the footer's padding just above the square.
      const rail = await groundPixel(page, { x: geo.t.x + 4, y: geo.t.y - 7, width: geo.t.w - 8, height: 4 });
      const probe = await page.evaluate((v) => { const p = document.createElement("canvas").getContext("2d"); p.fillStyle = v; return p.fillStyle; }, geo.ring);
      for (const [what, color] of [["square edge", geo.edge], ["focus ring", probe], ...geo.keys.map((k, i) => [["cpu key user", "cpu key system", "ram key (warning)"][i], k])]) {
        const c = toColor(color);
        const ratio = C.round2(C.contrastRatio(c, rail));
        assert(ratio >= PAINT_FLOOR, `${what} ${C.hex(c)} on the rail ${C.hex(rail)} is ${ratio}:1`);
        lines.push(`${theme} ${what} ${C.hex(c)} on rail ${C.hex(rail)} ${ratio}:1`);
      }
      await page.evaluate(() => { document.getElementById("res-square").dataset.view = "ink"; });
      await setKind(page, "live");
      report.paints.push(...lines);
      return `${lines.length} paints measured, worst ${Math.min(...lines.map((l) => parseFloat(/([\d.]+):1/.exec(l)[1])))}:1`;
    });

    await run.check(`5 ${theme}  right-click opens Ink/Readout (Ink checked); Readout is saved and survives a relaunch`, async () => {
      const thread = (await facts(page)).thread;
      await page.click("#res-square", { button: "right" });
      await page.waitForSelector("#res-menu", { state: "visible" });
      let f = await facts(page);
      const items = await page.$$eval("#res-menu [role=menuitemradio]", (n) => n.map((b) => b.textContent.replace("✓", "").trim()));
      assertEqual(items, ["Ink", "Readout"], "the menu's choices");
      assertEqual(f.checked, ["ink"], "the current view is checked");
      assert(!f.cardOpen, "the card does not open over the menu");
      await page.click('#res-menu [data-view="readout"]');
      await flushFrames(page);
      f = await facts(page);
      assertEqual([f.view, f.menuOpen, f.readoutShown, f.stored], ["readout", false, true, "readout"], "after choosing Readout");
      assertEqual(f.thread, thread, "the selected chat");
      // A relaunch, with the page's own mirror wiped: only the settings store can say Readout.
      await page.evaluate(() => localStorage.removeItem("richos.resources-view"));
      await page.close();
      page = await openApp(context, "live");
      await page.waitForFunction(() => document.getElementById("res-square").dataset.view === "readout", null, { timeout: 30000 });
      f = await facts(page);
      assertEqual([f.view, f.readoutShown], ["readout", true], "after the relaunch");
      // and back, so the next checks start where a first run does
      await page.click("#res-square", { button: "right" });
      await page.click('#res-menu [data-view="ink"]');
      f = await facts(page);
      assertEqual([f.view, f.stored], ["ink", "ink"], "switched back to Ink");
      return "Readout saved, restored from the store after a relaunch, and back to Ink";
    });

    await run.check(`6 ${theme}  keyboard: focus opens the card, Escape closes it with focus kept; Shift+F10 and the menu key switch the view`, async () => {
      const thread = (await facts(page)).thread;
      // Reached the way a keyboard user reaches it: Tab from the name beside it.
      // WebKit on macOS leaves buttons out of the Tab order unless the Mac's keyboard
      // navigation setting is on, as it does for every button in the app; so the focus is
      // given right after a key, which is what makes it keyboard focus (:focus-visible).
      await page.keyboard.press("Tab");
      await page.focus("#res-square");
      await page.waitForFunction(() => document.getElementById("res-card").classList.contains("is-open"), null, { timeout: 30000 })
        .catch(async () => {
          const at = await page.evaluate(() => ({ id: document.activeElement && (document.activeElement.id || document.activeElement.tagName), fv: document.getElementById("res-square").matches(":focus-visible") }));
          throw new Error("keyboard focus did not open the card: focus on " + at.id + ", square :focus-visible " + at.fv);
        });
      let f = await facts(page);
      assertEqual(f.focused, "res-square", "focus is on the square while the card is open");
      await page.keyboard.press("Escape");
      await flushFrames(page);
      f = await facts(page);
      assertEqual([f.cardOpen, f.focused], [false, "res-square"], "Escape closed the card and focus stayed on the square");
      await page.keyboard.press("Shift+F10");
      await page.waitForSelector("#res-menu", { state: "visible" });
      assertEqual(await page.evaluate(() => document.activeElement.dataset.view), "ink", "focus lands on the checked choice");
      await page.keyboard.press("ArrowDown");
      assertEqual(await page.evaluate(() => document.activeElement.dataset.view), "readout", "ArrowDown moves to Readout");
      await page.keyboard.press("Enter");
      await flushFrames(page);
      f = await facts(page);
      assertEqual([f.view, f.menuOpen, f.focused, f.stored], ["readout", false, "res-square", "readout"], "Enter chose Readout and gave focus back");
      await page.keyboard.press("ContextMenu");
      await page.waitForSelector("#res-menu", { state: "visible" });
      assertEqual(await page.evaluate(() => document.activeElement.dataset.view), "readout", "the menu key opens it on the checked choice");
      await page.keyboard.press("ArrowUp");
      await page.keyboard.press("Escape");
      await flushFrames(page);
      f = await facts(page);
      assertEqual([f.view, f.menuOpen, f.focused], ["readout", false, "res-square"], "Escape left the menu without choosing");
      await page.keyboard.press("Shift+F10");
      await page.keyboard.press("ArrowUp");
      await page.keyboard.press(" ");
      f = await facts(page);
      assertEqual([f.view, f.stored, f.thread], ["ink", "ink", thread], "Space chose Ink; the selected chat never changed");
      await page.evaluate(() => document.activeElement.blur());
      return "card on focus, Escape keeps focus, Shift+F10 / ContextMenu / arrows / Enter / Space / Escape";
    });

    await run.check(`7 ${theme}  Readout at the default 300 px rail keeps the whole name, and the numbers never change width`, async () => {
      await page.click("#res-square", { button: "right" });
      await page.click('#res-menu [data-view="readout"]');
      await setKind(page, "full");
      const f = await facts(page);
      assertEqual(f.railWidth, 300, "the rail's default width");
      assertEqual(f.readout[0], "100%", "the widest CPU figure");
      assert(!f.name.truncated, `"${f.name.text}" is cut at 300 px with Readout on`);
      const widths = await page.evaluate(() => {
        const r = document.querySelector("#res-square .res-readout");
        const b = document.getElementById("res-cpu");
        const keep = b.textContent;
        const out = [];
        for (const v of ["8%", "88%", "100%", "…", "-"]) { b.textContent = v; out.push(r.getBoundingClientRect().width); }
        b.textContent = keep;
        return out;
      });
      assert(widths.every((w) => w === widths[0]), "the readout's width changed with its figures: " + widths.join(", "));
      await page.click("#res-square", { button: "right" });
      await page.click('#res-menu [data-view="ink"]');
      await setKind(page, "live");
      return `rail ${f.railWidth}px, name whole, readout ${widths[0]}px for every figure`;
    });

    await run.check(`8 ${theme}  measuring, not current, unavailable and a failed read never show 0%`, async () => {
      await page.click("#res-square", { button: "right" });
      await page.click('#res-menu [data-view="readout"]');
      const seen = [];
      for (const [kind, chip, mark] of [["measuring", "Measuring", "…"], ["stale", "Not current", "-"], ["unavailable", "Unavailable", "-"], ["fail", "Unavailable", "-"]]) {
        await setKind(page, kind);
        await page.click("#res-square");
        await page.waitForFunction(() => document.getElementById("res-card").classList.contains("is-open"));
        const f = await facts(page);
        assertEqual(f.readout, [mark, mark], `${kind}: the readout`);
        assert(!/(^|\D)0%/.test(f.cardText), `${kind}: the card shows 0%: ${f.cardText}`);
        assert(f.cardText.includes(chip), `${kind}: the card's status says ${chip}`);
        const paints = await canvasPaints(page, "#res-square canvas", ["user", "system"]);
        if (kind === "measuring") assertEqual([paints.paints.user.count, paints.paints.system.count], [0, 0], "measuring draws no value");
        seen.push(`${kind}: ${f.readout.join("/")}, "${chip}"`);
        await closeCard(page);
      }
      await page.click("#res-square", { button: "right" });
      await page.click('#res-menu [data-view="ink"]');
      await setKind(page, "live");
      return seen.join("; ");
    });

    await run.check(`9 ${theme}  after sleep the gap is a gap labeled Asleep, never zeros`, async () => {
      await page.close();
      page = await openApp(context, "slept");
      await page.click("#res-square");
      await page.waitForFunction(() => document.getElementById("res-card").classList.contains("is-open"));
      await page.mouse.move(700, 450);
      await flushFrames(page);
      const f = await facts(page);
      assert(f.cardText.includes("The Mac slept for a while. The gap is that time; nothing was measured."), "the card says it slept: " + f.cardText);
      // In the card's graph (60 steps over 258 px), the hole holds no paint at all.
      const hole = await page.evaluate(() => {
        const c = document.querySelector("#res-card canvas");
        const dpr = c.width / 266;
        const step = 258 / 60;
        const css = getComputedStyle(document.documentElement);
        const probe = document.createElement("canvas").getContext("2d");
        const rgbOf = (v) => { probe.fillStyle = v; const h = probe.fillStyle; return [1, 3, 5].map((i) => parseInt(h.slice(i, i + 2), 16)); };
        const user = rgbOf(css.getPropertyValue("--res-user").trim());
        // 45 samples: 22, a 14-slot hole, 23. From the right: 23 samples, then the hole.
        const right = 4 + 258 - 23 * step;
        const x0 = Math.round((right - 12 * step) * dpr);
        const x1 = Math.round((right - 2 * step) * dpr);
        const d = c.getContext("2d").getImageData(x0, 0, x1 - x0, c.height).data;
        let painted = 0;
        for (let i = 0; i < d.length; i += 4) if (Math.abs(d[i] - user[0]) <= 2 && Math.abs(d[i + 1] - user[1]) <= 2 && Math.abs(d[i + 2] - user[2]) <= 2) painted++;
        return painted;
      });
      assertEqual(hole, 0, "user paint inside the sleep gap");
      await footerShot(page, `slept-${theme}.png`);
      await page.click("#res-square");
      return "Asleep sentence shown, gap unpainted, shots-resources/slept-" + theme + ".png";
    });

    await run.check(`10 ${theme}  hover and click open the card without moving focus or changing the chat`, async () => {
      await page.close();
      page = await openApp(context, "live");
      const before = await facts(page);
      const box = await page.locator("#res-square .res-tile").boundingBox();
      await page.mouse.move(box.x + 17, box.y + 17);
      await page.waitForFunction(() => document.getElementById("res-card").classList.contains("is-open"));
      let f = await facts(page);
      assertEqual([f.focused, f.thread], [before.focused, before.thread], "hover moved nothing");
      await page.mouse.move(700, 450);
      await page.waitForFunction(() => !document.getElementById("res-card").classList.contains("is-open"));
      await page.click("#res-square");
      f = await facts(page);
      assert(f.cardOpen, "a click pins the card");
      assertEqual(f.thread, before.thread, "the selected chat");
      await page.mouse.move(700, 450);
      await flushFrames(page);
      assert((await facts(page)).cardOpen, "pinned, it stays open when the pointer leaves");
      await page.mouse.click(900, 300);
      await page.waitForFunction(() => !document.getElementById("res-card").classList.contains("is-open"), null, { timeout: 30000 })
        .catch(() => { throw new Error("a click elsewhere did not close it"); });
      assertEqual(page.__errors, [], "no page error");
      return `focus stayed on ${before.focused}, chat ${before.thread}`;
    });

    await page.close();
    await context.close();
  }

  console.log("\n  text pairs (worst of each distinct pair):\n    " + report.text.join("\n    "));
  console.log("\n  paints from pixels:\n    " + report.paints.join("\n    "));
  await browser.close();
  return run.report();
}

main().then((f) => { process.exitCode = f ? 1 : 0; }, (e) => { console.error(e && e.stack ? e.stack : e); process.exit(1); });

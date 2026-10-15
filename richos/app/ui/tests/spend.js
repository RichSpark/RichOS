"use strict";
// API-EQUIVALENT TOKEN SPEND: round 22.1 (richos-hq design/mockups/rounds/round-22.1/, index.html
// and NOTES.md; the CEO, 2026-10-10: "design/mockups/rounds/round-22.1: go"), measured against his
// words, ruling §118: "I want to add this feature that will allow the user to see and brag about
// (via screenshots and screen recording videos) about their "API-equivalent token spend"" and
// "That's needs to be its own screen/page.", and his round 22 feedback
// (docs/ceo-input/2026-10-10_02/feedback.md): no two characters of the figure touching, Refresh
// instead of a live counter, "x agents working" kept and priced in dollars, a row in quick
// settings, the caption on two lines, not three.
//
// One check per part of the round, each red on a tree without the screen; the figure's gaps
// measured on the rendered pixels as round 22.1's own walk measures them (audit/figure.js); then
// contrast (text 4.5:1, large text and indicators 3:1) and the 16px floor computed in WebKit at
// 1440 by 900 in both themes. The figures themselves are the mock's (`spend_view`): what is proven
// here is the screen; the pricing is proven in richos-core (`spend::tests`, `spend::ledger::tests`).
const fs = require("fs");
const path = require("path");
const contrast = require("./lib/contrast");
const { loadPlaywright, createRun, assert, assertEqual, UI_DIR, shot } = require("./lib/harness");

const MIN_GAP = 4; // round 22.1's floor: comfortably apart at 196px, never merely not touching
const plain = s => s.replace(/[’‘]/g, "'").replace(/\s+/g, " ").trim();
const money = v => "$" + v.toLocaleString("en-US", { minimumFractionDigits: 2, maximumFractionDigits: 2 });

// The period starts, in local time, computed here independently of spend.js.
function starts(nowMs) {
  const d = new Date(nowMs);
  const midnight = new Date(d.getFullYear(), d.getMonth(), d.getDate());
  const monday = new Date(midnight); monday.setDate(monday.getDate() - ((monday.getDay() + 6) % 7));
  return { today: midnight.getTime(), week: monday.getTime(), month: new Date(d.getFullYear(), d.getMonth(), 1).getTime(), all: 0 };
}
const sumFrom = (rows, from) => rows.filter(r => r.at >= from).reduce((a, r) => a + r.cost, 0);

// In the page: the smallest 8-neighbor distance between the ink of the even characters and the ink
// of the odd ones (round 22.1's audit/figure.js MEASURE, unchanged).
const MEASURE = async (b64s) => {
  const load = (b64) => new Promise((res) => { const i = new Image(); i.onload = () => res(i); i.src = "data:image/png;base64," + b64; });
  const imgs = await Promise.all(b64s.map(load));
  const w = imgs[0].width, h = imgs[0].height;
  const data = imgs.map((img) => { const c = document.createElement("canvas"); c.width = w; c.height = h; const x = c.getContext("2d"); x.drawImage(img, 0, 0); return x.getImageData(0, 0, w, h).data; });
  const [even, odd, none] = data;
  const ink = (d) => { const m = new Uint8Array(w * h); for (let i = 0; i < w * h; i++) { const o = i * 4; if (Math.abs(d[o] - none[o]) > 2 || Math.abs(d[o + 1] - none[o + 1]) > 2 || Math.abs(d[o + 2] - none[o + 2]) > 2) m[i] = 1; } return m; };
  const A = ink(even), B = ink(odd);
  let inkA = 0, inkB = 0; for (let i = 0; i < w * h; i++) { inkA += A[i]; inkB += B[i]; }
  let cur = A;
  for (let step = 0; step <= 40; step++) {
    for (let i = 0; i < w * h; i++) if (cur[i] && B[i]) return { gap: step - 1, inkA, inkB };
    const next = new Uint8Array(w * h);
    for (let y = 0; y < h; y++) for (let x = 0; x < w; x++) {
      if (!cur[y * w + x]) continue;
      for (let dy = -1; dy <= 1; dy++) for (let dx = -1; dx <= 1; dx++) { const yy = y + dy, xx = x + dx; if (yy >= 0 && yy < h && xx >= 0 && xx < w) next[yy * w + xx] = 1; }
    }
    cur = next;
  }
  return { gap: 40, inkA, inkB };
};
// Every digit pair side by side, every digit beside the comma, the point and the dollar sign
// (round 22.1's stress set).
const STRESS = [];
for (let x = 0; x <= 9; x++) for (let y = 0; y <= 9; y++) STRESS.push("$" + (x === 0 ? "9" : String(x)) + x + y + "," + y + x + x + "." + x + y);
for (let x = 0; x <= 9; x++) STRESS.push("$" + x + "." + x + "9");

async function main() {
  const run = createRun("API-equivalent token spend (round 22.1)");
  const browser = await loadPlaywright().webkit.launch();
  const errors = [];
  async function open(theme, spend, opts = {}) {
    const page = await browser.newPage({ viewport: { width: 1440, height: 900 }, deviceScaleFactor: 1, locale: "en-US", reducedMotion: opts.reducedMotion || "no-preference" });
    page.setDefaultTimeout(30000); // load-bound: a hang guard only; every wait below waits for a fact
    page.on("pageerror", e => errors.push(String(e)));
    await page.addInitScript(({ theme, spend, extra }) => {
      localStorage.setItem("richos-theme", theme);
      localStorage.setItem("richos-mock-config", JSON.stringify({ theme, font_scale: 100 }));
      window.__RICHOS_MOCK_PRESET__ = Object.assign({ spend }, extra);
    }, { theme, spend, extra: opts.preset || {} });
    await page.goto("file://" + path.join(UI_DIR, "index.html"));
    await page.waitForSelector("#set-btn");
    await page.evaluate(() => window.RichSplash.yieldNow("spend-test"));
    await page.evaluate(t => document.documentElement.setAttribute("data-theme", t), theme);
    return page;
  }
  async function menu(page) {
    if (await page.locator("#set-menu").isHidden()) await page.click("#set-btn");
    await page.waitForSelector("#set-menu #set-spend-open");
  }
  // Opened the way a person opens it: Settings, then the row.
  async function screen(page) {
    await menu(page);
    await page.click("#set-spend-open");
    await page.waitForSelector("#spend-fig .ch");
  }
  const text = (page, sel) => page.locator(sel).innerText().then(plain);
  const rows = page => page.evaluate(() => window.__RICHOS_MOCK__.spendRows());
  const calls = page => page.evaluate(() => window.__RICHOS_MOCK__.spendCalls());
  const settle = page => page.evaluate(() => Promise.all(document.getAnimations().filter(a => a.effect && a.effect.getTiming().iterations !== Infinity).map(a => a.finished.catch(() => null))));

  // ---- 0. the wire ---------------------------------------------------------------------------
  await run.check("the wire: spend_view and spend_live are registered in main.rs, answered by mock.js and called by spend.js", async () => {
    // Read off the files, as corrections.js check 13 reads the handler: bounded at the macro's
    // own closing bracket, the shipped UI scanned with comments stripped.
    const rs = fs.readFileSync(path.join(UI_DIR, "..", "src-tauri", "src", "main.rs"), "utf8");
    const from = rs.indexOf("generate_handler![");
    const to = rs.indexOf("]", from + "generate_handler![".length);
    assert(from >= 0 && to > from, "no generate_handler! in main.rs");
    const handler = rs.slice(from, to);
    const strip = s => s.replace(/\/\*[\s\S]*?\*\//g, "").replace(/(^|[^:"'])\/\/.*$/gm, "$1");
    const ui = strip(fs.readFileSync(path.join(UI_DIR, "spend.js"), "utf8"));
    const mock = fs.readFileSync(path.join(UI_DIR, "mock.js"), "utf8");
    const out = [];
    for (const cmd of ["spend_view", "spend_live"]) {
      assert(new RegExp("\\b" + cmd + ",").test(handler), cmd + " is not registered in main.rs's generate_handler!");
      assert(new RegExp("fn " + cmd + "\\(").test(rs), "main.rs has no fn " + cmd);
      assert(mock.includes('case "' + cmd + '"'), "mock.js does not answer " + cmd + ", so no browser suite can drive it");
      assert(ui.includes('"' + cmd + '"'), "spend.js never calls " + cmd);
      out.push(cmd);
    }
    // The argument the UI sends is the one the command takes: `from`, Unix ms.
    assert(/fn spend_view\(state: State<AppState>, from: u64\)/.test(rs), "spend_view takes from: u64");
    assert(/call\("spend_view", \{ from: /.test(ui), "spend.js sends { from }");
    return out.join(", ") + ": registered, mocked, called";
  });

  // ---- 1. the way in -------------------------------------------------------------------------
  await run.check("the row: Token spend in Settings, directly under Home screen, with this month's figure and a chevron", async () => {
    const page = await open("dark", undefined);
    await menu(page);
    // Round 27: the third column, "Your company and team", row 3, under Company and Home screen.
    const order = await page.evaluate(() => [...document.getElementById("set-spend").parentElement.children].map(e => e.id || e.className));
    const home = order.indexOf("set-home-row");
    assert(home >= 0, "Home screen is in the column: " + order);
    assertEqual(order[home + 1], "set-spend", "Token spend comes directly after Home screen: " + order);
    assertEqual(await text(page, "#set-spend-open .set-name"), "Token spend");
    await page.waitForFunction(() => document.getElementById("set-spend-amt").textContent.length > 0);
    const s = starts(Date.now());
    const month = sumFrom(await rows(page), s.month);
    // The screen's own format, cents and all, so the row and the screen's This month figure never disagree.
    assertEqual(await text(page, "#set-spend-amt"), money(month) + " this month");
    assertEqual(await page.locator("#set-spend-open svg").count(), 1, "a chevron");
    assertEqual(await page.getAttribute("#set-spend-open", "aria-current"), null, "not the current page yet");
    const row = (await text(page, "#set-spend-amt")).replace(/ this month$/, "");
    await screen(page);
    assertEqual(await page.getAttribute("#spend-fig", "aria-label"), row, "the row's figure is the screen's This month figure");
    await page.close();
    return "row " + row + " this month; the screen's This month figure " + row;
  });

  // ---- 1b. the row's cents: a $0.64 month is not "$1 this month" (found by echo-opus-spend2 in the test VM) ----
  await run.check("the row: a $0.64 month reads \"$0.64 this month\", as the screen's This month figure reads it", async () => {
    const at = Date.now();
    const page = await open("dark", { rows: [
      { at, family: "opus", name: "Opus 5.5", effort: "high", requests: 3, cost: 0.4, tokens: 120000 },
      { at, family: "haiku", name: "Haiku 5.5", effort: "low", requests: 2, cost: 0.24, tokens: 40000 },
    ] });
    await menu(page);
    await page.waitForFunction(() => document.getElementById("set-spend-amt").textContent.length > 0);
    assertEqual(await text(page, "#set-spend-amt"), "$0.64 this month");
    await screen(page);
    assertEqual(await page.getAttribute("#spend-fig", "aria-label"), "$0.64", "the screen's This month figure");
    await page.close();
    return "row $0.64 this month; the screen's This month figure $0.64";
  });

  // ---- 2. its own screen ---------------------------------------------------------------------
  await run.check("its own screen: the app view and the home screen are covered, Settings stays on top, Back has the focus", async () => {
    const page = await open("dark", undefined);
    await screen(page);
    const got = await page.evaluate(() => {
      const sp = document.getElementById("spend").getBoundingClientRect();
      const hit = (x, y) => { const e = document.elementFromPoint(x, y); return e && (e.closest("#spend") ? "spend" : e.closest(".settings") ? "settings" : e.id || e.className); };
      return { full: sp.left === 0 && sp.top === 0 && sp.width === innerWidth && sp.height === innerHeight,
        app: getComputedStyle(document.getElementById("app")).display, middle: hit(innerWidth / 2, innerHeight / 2), corner: hit(innerWidth - 38, 38),
        focus: document.activeElement && document.activeElement.id, menu: document.getElementById("set-menu").hidden };
    });
    assertEqual(got, { full: true, app: "none", middle: "spend", corner: "settings", focus: "spend-back", menu: true });
    await page.close();
  });

  // ---- 3. the Meter, word for word -----------------------------------------------------------
  await run.check("the Meter: header, eyebrow, the figure for this month, the two sentences, the legend, the tokens line, the footer", async () => {
    const page = await open("dark", undefined);
    await screen(page);
    const now = new Date();
    const s = starts(now.getTime());
    const all = await rows(page);
    const month = sumFrom(all, s.month);
    const MONTHS = ["January", "February", "March", "April", "May", "June", "July", "August", "September", "October", "November", "December"];
    assertEqual(await text(page, "#spend-title"), "API-equivalent token spend · this month");
    assertEqual(await text(page, "#spend-back"), "Back");
    assertEqual(await text(page, "#spend-refresh"), "Refresh");
    assert(/^Updated \d{1,2}:\d{2}:\d{2} (AM|PM)$/.test(await text(page, "#spend-updated")), "Updated with seconds: " + await text(page, "#spend-updated"));
    assertEqual(await page.evaluate(() => [...document.querySelectorAll("#spend-period button")].map(b => b.textContent + (b.getAttribute("aria-pressed") === "true" ? "*" : ""))),
      ["Today", "This week", "This month*", "All time"]);
    assertEqual((await text(page, ".sp-eyebrow")).toLowerCase(), ("API-equivalent token spend · " + MONTHS[now.getMonth()] + " " + now.getFullYear()).toLowerCase());
    assertEqual(await page.getAttribute("#spend-fig", "aria-label"), money(month));
    assertEqual(await page.evaluate(() => document.getElementById("spend-fig").textContent), money(month));
    assertEqual(await page.evaluate(() => [...document.querySelectorAll(".sp-sub > span")].map(s => s.textContent.replace(/[’]/g, "'"))),
      ["What the same work would have cost at Claude's API prices.", "Billed by the API: nothing. It ran on your Claude subscription."]);
    const legend = await page.evaluate(() => [...document.querySelectorAll(".sp-key:not([hidden])")].map(k => ({
      m: k.dataset.m, name: k.querySelector("b").textContent, amt: k.querySelector(".amt").textContent,
      pct: k.querySelector(".pct").textContent, eff: k.querySelector(".eff [data-k]").textContent })));
    assertEqual(legend.map(l => l.m), ["fable", "opus", "sonnet", "haiku"], "a mark per model with spend, in round 22.1's order");
    const opus = legend.find(l => l.m === "opus");
    assertEqual([opus.name, opus.eff], ["Opus 5, 5.5 and 4.8", "high and medium effort"], "Opus names its versions, most spend first, and its efforts");
    for (const l of legend) {
      assert(/^\$[\d,]+\.\d\d$/.test(l.amt), "a dollar amount: " + JSON.stringify(l));
      assert(/^(<0\.1|\d+(\.\d)?)%$/.test(l.pct), "a share: " + JSON.stringify(l));
    }
    assert(/^[\d.,]+ (billion|million)? ?tokens across [\d,]+ requests\.$/.test(await text(page, ".sp-tokens > span:first-child")), await text(page, ".sp-tokens > span:first-child"));
    assert(/of them were read back from cache, which is what lets Rich keep the whole company in mind\.$/.test(await text(page, ".sp-tokens > span:last-child")));
    assertEqual((await text(page, "#spend-foot")).replace(/[’]/g, "'"),
      "What the same work would have cost at Claude's API prices as published on October 11, 2026. Counted on this Mac from every request. Not a bill.");
    await page.close();
  });

  // ---- 4. no live counter: Refresh -----------------------------------------------------------
  await run.check("no live counter: a request landing leaves the figure as it is and puts a dot on Refresh; Refresh brings it in", async () => {
    const page = await open("dark", undefined);
    await screen(page);
    const before = await page.evaluate(() => document.getElementById("spend-fig").textContent);
    const updated = await text(page, "#spend-updated");
    await page.evaluate(() => window.__RICHOS_MOCK__.spendLand({ family: "haiku", name: "Haiku 5.5", effort: "low", cost: 0.001, tokens: 58828 }));
    await page.waitForSelector("#spend-refresh.has-new");
    assertEqual(await page.evaluate(() => document.getElementById("spend-fig").textContent), before, "the figure holds still");
    assert((await page.getAttribute("#spend-refresh", "aria-label")).startsWith("Refresh the numbers: new requests since "), "the dot is named for a screen reader");
    await page.evaluate(() => window.__RICHOS_MOCK__.spendLand({ family: "opus", name: "Opus 5.5", effort: "high", cost: 1234.56, tokens: 1 }));
    // load-bound: one second, so the Updated time (with seconds) can move.
    await page.waitForTimeout(1100);
    await page.click("#spend-refresh");
    const s = starts(Date.now());
    const want = money(sumFrom(await rows(page), s.month));
    await page.waitForFunction(w => document.getElementById("spend-fig").getAttribute("aria-label") === w, want);
    assert(!(await page.locator("#spend-refresh.has-new").count()), "the dot goes with the refresh");
    assert((await text(page, "#spend-updated")) !== updated, "Updated moved");
    await page.close();
  });

  // ---- 5. the live line ----------------------------------------------------------------------
  await run.check("x agents working stays live, the last request priced in dollars ($0.001, not 0.1¢); none working says so", async () => {
    const page = await open("dark", { agents: 4 });
    await screen(page);
    await page.evaluate(() => window.__RICHOS_MOCK__.spendLand({ family: "haiku", name: "Haiku 5.5", effort: "low", cost: 0.00104, tokens: 58828 }));
    await page.waitForFunction(() => /58,828/.test(document.getElementById("spend-live").textContent));
    assertEqual(await text(page, "#spend-live"), "4 agents working · last request Haiku 5.5, 58,828 tokens, $0.001");
    await page.evaluate(() => window.__RICHOS_MOCK__.spendLand({ family: "opus", name: "Opus 5.5", effort: "high", cost: 0.1092, tokens: 237114 }));
    await page.waitForFunction(() => /237,114/.test(document.getElementById("spend-live").textContent));
    assertEqual(await text(page, "#spend-live"), "4 agents working · last request Opus 5.5, 237,114 tokens, $0.109");
    await page.evaluate(() => window.__RICHOS_MOCK__.spendAgents(0));
    await page.waitForFunction(() => document.getElementById("spend-live").classList.contains("is-idle"));
    assertEqual(await text(page, "#spend-live"), "No agents working right now");
    await page.close();
  });

  // ---- 6. the periods ------------------------------------------------------------------------
  await run.check("Today, This week, This month and All time each sum from their own local start", async () => {
    const page = await open("dark", undefined);
    await screen(page);
    const seen = [];
    for (const [p, label] of [["today", "Today"], ["week", "This week"], ["all", "All time"], ["month", "This month"]]) {
      await page.click(`#spend-period button[data-p="${p}"]`);
      const want = money(sumFrom(await rows(page), starts(Date.now())[p]));
      await page.waitForFunction(w => document.getElementById("spend-fig") && document.getElementById("spend-fig").getAttribute("aria-label") === w, want);
      assertEqual(await page.getAttribute(`#spend-period button[data-p="${p}"]`, "aria-pressed"), "true", label);
      assertEqual(await text(page, "#spend-title"), "API-equivalent token spend · " + label.toLowerCase());
      seen.push(label + " " + want);
    }
    assert(/since [A-Z][a-z]+ \d+/.test(await page.evaluate(async () => { document.querySelector('#spend-period button[data-p="all"]').click(); await new Promise(r => setTimeout(r, 300)); return document.getElementById("spend-when").textContent; })), "All time says since when");
    await page.close();
    return seen.join("; ");
  });

  // ---- 7. the way out, and the row on its own screen -----------------------------------------
  await run.check("Back and Esc close it; on its own screen the row is the current page and refreshes it", async () => {
    const page = await open("dark", undefined);
    await screen(page);
    await menu(page);
    assertEqual(await page.getAttribute("#set-spend-open", "aria-current"), "page");
    const views = (await calls(page)).filter(c => c.cmd === "spend_view").length;
    await page.click("#set-spend-open");
    await page.waitForFunction(n => window.__RICHOS_MOCK__.spendCalls().filter(c => c.cmd === "spend_view").length > n, views);
    assert(await page.locator("#spend").isVisible(), "pressing the row on its own screen keeps it open");
    assert(await page.locator("#set-menu").isHidden(), "and closes the menu");
    await page.keyboard.press("Escape");
    await page.waitForSelector("#spend", { state: "hidden" });
    assertEqual(await page.evaluate(() => getComputedStyle(document.getElementById("app")).display !== "none"), true, "the app view is back");
    await screen(page);
    await page.click("#spend-back");
    await page.waitForSelector("#spend", { state: "hidden" });
    await page.close();
  });

  // ---- 7b. Enter on the screen opened from Home ----------------------------------------------
  // The review of b80728780 (rv-20261011T065607Z-b8072878-65bc): opened from Home, the screen
  // covers Home, which stays open beneath it with its capturing key listener; that listener had no
  // exception for this screen, so Enter on Back, Refresh or a period was canceled and hid Home
  // instead ("Enter on spend-back: defaultPrevented=true, home hide=enter-key"). Each Enter here
  // must do what its control says, and Home must still be there when the screen closes.
  await run.check("opened from Home, Enter on Refresh, on a period and on Back does what the control says, and Home is still open beneath", async () => {
    const page = await open("dark", undefined);
    await page.evaluate(() => window.RichHome.show("spend-suite"));
    await page.waitForFunction(() => window.RichHome.isOpen());
    await screen(page);
    const homeOpen = () => page.evaluate(() => window.RichHome.isOpen());
    const views = () => page.evaluate(() => window.__RICHOS_MOCK__.spendCalls().filter(c => c.cmd === "spend_view").length);
    const wrong = [];
    // Refresh: a new spend_view, or Home gone, whichever happens first
    const n = await views();
    await page.focus("#spend-refresh");
    await page.keyboard.press("Enter");
    await page.waitForFunction(k => window.__RICHOS_MOCK__.spendCalls().filter(c => c.cmd === "spend_view").length > k || !window.RichHome.isOpen(), n);
    if ((await views()) <= n) wrong.push("Enter on Refresh did not refresh");
    if (!(await homeOpen())) wrong.push("Enter on Refresh hid Home");
    // Today
    await page.evaluate(() => window.RichHome.isOpen() || window.RichHome.show("spend-suite"));
    await page.focus('#spend-period button[data-p="today"]');
    await page.keyboard.press("Enter");
    await page.waitForFunction(() => document.querySelector('#spend-period button[data-p="today"]').getAttribute("aria-pressed") === "true" || !window.RichHome.isOpen());
    if ((await page.getAttribute('#spend-period button[data-p="today"]', "aria-pressed")) !== "true") wrong.push("Enter on Today did not choose Today");
    if (!(await homeOpen())) wrong.push("Enter on Today hid Home");
    // Back
    await page.evaluate(() => window.RichHome.isOpen() || window.RichHome.show("spend-suite"));
    await page.focus("#spend-back");
    await page.keyboard.press("Enter");
    await page.waitForFunction(() => document.getElementById("spend").hidden || !window.RichHome.isOpen());
    if (!(await page.evaluate(() => document.getElementById("spend").hidden))) wrong.push("Enter on Back did not close the screen");
    if (!(await homeOpen())) wrong.push("Enter on Back hid Home");
    await page.close();
    assertEqual(wrong, []);
    return "Refresh refreshed, Today chosen, Back closed the screen; Home open beneath after each";
  });

  // ---- 7c. a Settings sheet opened over the screen -------------------------------------------
  // The review of e7c316d11 (rv-20261011T075637Z-e7c316d1-466a): with the screen open, Settings
  // stays on top and its rows still work, but the sheets they open live on the desk (z-index 60 at
  // the body, 160 for Dictation, or inside #app, which this screen hides), so they opened UNDER
  // this opaque screen ("#dictation-sheet: hidden=false, spend_open=true, covered=true") and
  // Settings looked dead. Every row of the menu that opens a sheet is pressed from this screen:
  // the screen must give way, and the sheet must be the thing at its own middle, holding focus.
  await run.check("a Settings row that opens a sheet, pressed over the screen, closes the screen and puts that sheet on top with the focus", async () => {
    const out = [], wrong = [];
    const rows = ["set-accounts-open", "set-dictation-open", "set-repositories-open", "set-memory-open", "set-phone-open", "set-quota-open"];
    const DICTATION = { preset: { dictation: true } }; // a build with dictation: its row is in the menu
    const page = await open("dark", undefined, DICTATION);
    // the rows a person sees in the menu (Claude Code quota is Technical view's, hidden by default)
    const present = await page.evaluate(ids => { const b = document.getElementById("set-btn"); b.click(); const have = ids.filter(id => { const e = document.getElementById(id); return e && !e.hidden && e.offsetParent !== null; }); b.click(); return have; }, rows);
    for (const need of ["set-accounts-open", "set-dictation-open", "set-repositories-open"]) assert(present.includes(need), need + " is in the menu");
    await page.close();
    for (const id of present) {
      const p = await open("dark", undefined, DICTATION);
      await screen(p);
      await menu(p);
      await p.click("#" + id);
      await p.waitForFunction(() => [...document.querySelectorAll(".overlay")].some(o => !o.hidden && !o.closest("#spend")));
      await settle(p);
      // Home, open beneath in this suite, gives way to a desk sheet by its own transition: wait for
      // the fact that settles it either way: the sheet is the thing at its middle, or this screen
      // is still open (then nothing more will change, and the measurement below says so).
      await p.waitForFunction(() => {
        if (document.body.classList.contains("spend-open")) return true;
        const s = [...document.querySelectorAll(".overlay")].find(o => !o.hidden && !o.closest("#spend"));
        const r = s.getBoundingClientRect(), h = document.elementFromPoint(r.left + r.width / 2, r.top + r.height / 2);
        return h && s.contains(h);
      });
      const m = await p.evaluate(() => {
        const sheet = [...document.querySelectorAll(".overlay")].find(o => !o.hidden && !o.closest("#spend"));
        const r = sheet.getBoundingClientRect();
        const box = r.width > 0 && r.height > 0 ? r : { left: 0, top: 0, width: innerWidth, height: innerHeight };
        const hit = document.elementFromPoint(box.left + box.width / 2, box.top + box.height / 2);
        return { sheet: sheet.id, spendOpen: document.body.classList.contains("spend-open"), onTop: !!(hit && sheet.contains(hit)),
          hit: hit ? (hit.id || hit.className || hit.tagName) : null, focus: !!(document.activeElement && sheet.contains(document.activeElement)) };
      });
      if (m.spendOpen) wrong.push(`${id} -> #${m.sheet}: the Token spend screen stayed open`);
      if (!m.onTop) wrong.push(`${id} -> #${m.sheet}: covered (the middle of the sheet is ${m.hit})`);
      if (!m.focus) wrong.push(`${id} -> #${m.sheet}: the focus is not in the sheet`);
      out.push(`${id} -> #${m.sheet}`);
      await p.close();
    }
    assertEqual(wrong, [], out.join(", "));
    return out.join(", ") + ": each on top with the focus, the screen closed";
  });

  // ---- 8. the first count --------------------------------------------------------------------
  await run.check("before the first count is done the screen says it is counting and shows no partial figure, then draws the Meter", async () => {
    const page = await open("dark", { ready: false });
    await menu(page);
    await page.click("#set-spend-open");
    await page.waitForSelector(".sp-counting");
    assertEqual(await page.locator("#spend-fig").count(), 0, "no figure while counting");
    assertEqual(await text(page, ".sp-counting"), "Counting every request on this Mac. The first count reads the whole history once.");
    await page.evaluate(() => window.__RICHOS_MOCK__.spendReady());
    await page.waitForSelector("#spend-fig .ch");
    await page.close();
  });

  // ---- 9. the two-line sentences -------------------------------------------------------------
  await run.check("the sentences under the figure and the tokens line are two lines each, the Rich panel docked or not (CEO: 'it should be 2')", async () => {
    const lines = el => { const r = el.getClientRects(); return r.length; };
    const results = [];
    for (const theme of ["dark", "light"]) {
      const page = await open(theme, undefined);
      await screen(page);
      for (const docked of [false, true, "small"]) {
        // "small": the app's smallest window, 1024 by 700, undocked.
        if (docked === true) await page.evaluate(() => document.body.classList.add("bug-dock-open"));
        if (docked === "small") { await page.evaluate(() => document.body.classList.remove("bug-dock-open")); await page.setViewportSize({ width: 1024, height: 700 }); }
        await settle(page);
        const n = await page.evaluate(() => {
          const count = sel => [...document.querySelectorAll(sel + " > span")].reduce((a, s) => {
            const range = document.createRange(); range.selectNodeContents(s);
            const tops = new Set([...range.getClientRects()].map(r => Math.round(r.top)));
            return a + tops.size;
          }, 0);
          // Each legend mark's name and amount on one line, the widest real case included
          // ("Opus 5, 5.5 and 4.8" beside its five-figure amount).
          const keys = [...document.querySelectorAll(".sp-key:not([hidden]) > span:nth-child(2)")].map(s => {
            const range = document.createRange(); range.selectNodeContents(s);
            return new Set([...range.getClientRects()].map(r => Math.round(r.top))).size;
          });
          // And no mark runs into the next: on one row, at least the 24px gap between them
          // (the first build put "$30,054.28" against Sonnet's swatch).
          const boxes = [...document.querySelectorAll(".sp-key:not([hidden])")].map(k => {
            const ink = [...k.children].map(c => c.getBoundingClientRect());
            return { top: Math.round(k.getBoundingClientRect().top), left: Math.min(...ink.map(r => r.left)), right: Math.max(...ink.map(r => r.right)) };
          });
          let gap = Infinity;
          for (let i = 1; i < boxes.length; i++) if (boxes[i].top === boxes[i - 1].top) gap = Math.min(gap, boxes[i].left - boxes[i - 1].right);
          return { sub: count(".sp-sub"), tokens: count(".sp-tokens"), keys: keys.every(k => k === 1), apart: gap >= 23.5, gap, widest: document.querySelector('.sp-key[data-m="opus"] b').textContent };
        });
        assertEqual(n.widest, "Opus 5, 5.5 and 4.8", "the mock carries the CEO's three Opus versions");
        const where = `${theme}${docked === "small" ? ", 1024 by 700" : docked ? ", panel docked" : ""}`;
        assertEqual({ sub: n.sub, tokens: n.tokens, keys: n.keys, apart: n.apart }, { sub: 2, tokens: 2, keys: true, apart: true }, `${where} (closest marks ${n.gap}px)`);
        results.push(`${where}: 2+2 lines, every mark one line, marks ${n.gap}px apart`);
      }
      await page.close();
    }
    return results.join(", ");
  });

  // ---- 9b. the header and the Meter fit every window and Text size the app allows ------------
  // The second review of 6c21269e6 (rv-20261011T044256Z-6c21269e-56ec, its one blocking finding):
  // round 22.1 drew the header at 1440 wide, one row; the app's smallest window is 1024 by 700
  // (src-tauri/tauri.conf.json minWidth and minHeight), which with the Rich panel docked leaves the
  // screen a 536px column (#spend's right edge sits 488px in). The review of 41f028c17
  // (rv-20261011T055538Z-41f028c1-c16b): in that column the live line did not wrap, so at Text size
  // 120% and 150%, and even at 100% with 12 agents and a Fable request, its dollar price ran off
  // the column. Measured on the rendered boxes, at every window and at Text size 100%, 120%, 135%
  // and 150% (the app's steps above 100%), with the widest live line (12 agents, a Fable 5.1
  // request of 448,503 tokens): every control of the header is whole inside the screen's column,
  // none overlaps another or the Settings button, no label breaks onto a second line, and nothing
  // in the header scrolls sideways; and every word of the Meter (the live line's price included)
  // is inside the Meter's own column.
  await run.check("the header and the Meter fit the app's smallest window, 1024 by 700, and its 536px docked column, at Text size 100% to 150%: every control whole, every word of the Meter (the live line's price too) inside its column", async () => {
    const out = [], wrong = [];
    const page = await open("dark", { agents: 12 });
    await screen(page);
    await page.evaluate(() => window.__RICHOS_MOCK__.spendLand({ family: "fable", name: "Fable 5.1", effort: "high", cost: 0.8214, tokens: 448503 }));
    await page.waitForFunction(() => /448,503/.test(document.getElementById("spend-live").textContent));
    assertEqual(await text(page, "#spend-live"), "12 agents working · last request Fable 5.1, 448,503 tokens, $0.821", "the widest live line");
    // and the widest effort label (the review of b80728780): app leases record no effort, so on a
    // real Mac a mark reads "high and medium effort, some not recorded"; Refresh brings it in
    await page.evaluate(() => window.__RICHOS_MOCK__.spendLand({ family: "opus", name: "Opus 5.5", effort: null, cost: 0.2, tokens: 400000 }));
    // and the longest name a mark can carry (the review of 2dd1375d7): every Opus version a record can
    // accumulate as models are upgraded, seven in one mark, beside a five-figure amount
    await page.evaluate(() => { for (const v of ["4.7", "4.6", "4.5", "4.1"]) window.__RICHOS_MOCK__.spendLand({ family: "opus", name: "Opus " + v, effort: "high", cost: 0.01, tokens: 1000 }); });
    await page.click("#spend-refresh");
    await page.waitForFunction(() => /4\.1/.test(document.querySelector('.sp-key[data-m="opus"]').textContent));
    assertEqual(await text(page, '.sp-key[data-m="opus"] .eff'), "· high and medium effort, some not recorded", "the widest effort label");
    const opusName = await text(page, '.sp-key[data-m="opus"] b');
    for (const v of ["5.5", "5,", "4.8", "4.7", "4.6", "4.5", "4.1"]) assert(opusName.includes(v), `the Opus mark names ${v}: ${opusName}`);
    assert(/^\$\d{2},\d{3}\.\d{2}$/.test(await text(page, '.sp-key[data-m="opus"] .amt')), "a five-figure amount beside it");
    // the live line names the LAST request: the widest one goes last, so every window below measures it
    await page.evaluate(() => window.__RICHOS_MOCK__.spendLand({ family: "fable", name: "Fable 5.1", effort: "high", cost: 0.8214, tokens: 448503 }));
    await page.waitForFunction(() => /Fable 5\.1, 448,503/.test(document.getElementById("spend-live").textContent));
    assertEqual(await text(page, "#spend-live"), "12 agents working · last request Fable 5.1, 448,503 tokens, $0.821", "the widest live line, measured below");
    // EVERY WORD ON THIS SCREEN CAN WRAP (the same review): no readable text is held to one line by
    // `white-space: nowrap` or a `max-content` minimum, except the figure, which has its own fit rule
    const held = await page.evaluate(() => [...document.querySelectorAll("#spend *")].filter(el => {
      if (el.closest("#spend-fig")) return false;
      if (![...el.childNodes].some(n => n.nodeType === 3 && n.textContent.trim()) && !el.querySelector("b, span")) return false;
      const cs = getComputedStyle(el);
      return cs.whiteSpace === "nowrap" || cs.whiteSpace === "pre" || cs.minWidth === "max-content" || cs.width === "max-content";
    }).map(el => (el.id ? "#" + el.id : el.className || el.tagName) + " (" + getComputedStyle(el).whiteSpace + ", min-width " + getComputedStyle(el).minWidth + ")"));
    for (const h of held) wrong.push("text held to one line: " + h);   // judged with the measurements below
    for (const scale of [100, 120, 135, 150]) {
      await page.evaluate(sc => document.documentElement.style.setProperty("--app-font-scale", String(sc / 100)), scale);
      for (const [w, h, docked] of [[1440, 900, false], [1440, 900, true], [1024, 700, false], [1024, 700, true]]) {
        await page.setViewportSize({ width: w, height: h });
        await page.evaluate(d => document.body.classList.toggle("bug-dock-open", d), docked);
        await settle(page);
        const m = await page.evaluate(() => {
          const box = el => { const r = el.getBoundingClientRect(); return { l: r.left, r: r.right, t: r.top, b: r.bottom, w: r.width, h: r.height }; };
          const shown = el => { const cs = getComputedStyle(el); return cs.display !== "none" && cs.visibility !== "hidden" && el.getBoundingClientRect().width > 0; };
          const textRects = el => {
            const found = [], walk = document.createTreeWalker(el, NodeFilter.SHOW_TEXT);
            for (let n = walk.nextNode(); n; n = walk.nextNode()) {
              if (!n.textContent.trim()) continue;
              const r = document.createRange(); r.selectNodeContents(n);
              for (const x of r.getClientRects()) if (x.width > 0) found.push({ x, n });
            }
            return found;
          };
          // the lines an element's words take: its text's boxes (not its icon's), tops more than 8px apart
          const lines = el => {
            const tops = textRects(el).map(t => t.x.top).sort((a, b) => a - b);
            return tops.reduce((k, t, i) => k + (i === 0 || t - tops[i - 1] > 8 ? 1 : 0), 0);
          };
          const head = document.querySelector(".sp-head"), col = box(document.getElementById("spend"));
          const controls = [...head.querySelectorAll(".sp-back, .sp-title, .sp-updated, .sp-refresh, .sp-period button")].filter(shown)
            .map(el => ({ name: (el.id || el.className.split(" ")[0]) + (el.dataset.p ? ":" + el.dataset.p : ""), ...box(el),
              lines: lines(el) }));
          const gear = document.getElementById("set-btn"); const g = gear && shown(gear) ? box(gear) : null;
          const hit = (a, b) => a.l < b.r - 0.5 && b.l < a.r - 0.5 && a.t < b.b - 0.5 && b.t < a.b - 0.5;
          const problems = [];
          for (const c of controls) {
            if (c.l < col.l - 0.5 || c.r > col.r + 0.5) problems.push(`${c.name} spans ${c.l.toFixed(1)} to ${c.r.toFixed(1)}, the column ${col.l} to ${col.r}`);
            if (c.lines !== 1) problems.push(`${c.name} is on ${c.lines} lines`);
            if (g && hit(c, g)) problems.push(`${c.name} is under the Settings button`);
          }
          // the period buttons sit inside their own pill, so overlap is measured between the groups
          const groups = [...head.children].filter(shown).map(el => ({ name: el.className.split(" ")[0], ...box(el) }));
          for (let i = 0; i < groups.length; i++) for (let j = i + 1; j < groups.length; j++)
            if (hit(groups[i], groups[j])) problems.push(`${groups[i].name} overlaps ${groups[j].name}`);
          if (head.scrollWidth > head.clientWidth + 0.5) problems.push(`the header scrolls sideways (${head.scrollWidth} in ${head.clientWidth})`);
          // every word of the Meter inside the Meter's content box (its side padding excluded)
          const meter = document.querySelector(".sp-meter"), mb = box(meter), ms = getComputedStyle(meter);
          const inner = { l: mb.l + parseFloat(ms.paddingLeft), r: mb.r - parseFloat(ms.paddingRight) };
          const off = new Map();
          for (const { x, n } of textRects(meter)) {
            // the figure is not judged by its text boxes: its +0.02em tracking after the last digit and
            // round 22.1's -0.03em optical margin put its advance a few px outside, never its ink; check
            // 10 measures its ink inside the column on the pixels
            if (n.parentElement.closest("#spend-fig")) continue;
            if (x.left < inner.l - 0.5 || x.right > inner.r + 0.5) {
              const owner = n.parentElement.closest("[id], .sp-live, .sp-key, .sp-tokens, .sp-sub, .sp-eyebrow") || n.parentElement;
              const name = owner.id || owner.className.split(" ")[0];
              off.set(name, Math.max(off.get(name) || 0, x.right - inner.r, inner.l - x.left));
            }
          }
          for (const [name, by] of off) problems.push(`${name} runs ${by.toFixed(1)}px past the Meter's column (${inner.l.toFixed(0)} to ${inner.r.toFixed(0)})`);
          const liveLines = lines(document.getElementById("spend-live"));
          const rows = new Set(groups.map(x => Math.round(x.t + x.h / 2))).size;
          return { problems, col: Math.round(col.w), rows, height: Math.round(box(head).h), liveLines, title: controls.some(c => c.name === "spend-title"), periods: controls.filter(c => c.name.includes(":")).length };
        });
        const where = `${scale}%, ${w} by ${h}${docked ? `, panel docked (${m.col}px column)` : ""}`;
        // a picture of each window to look at (.shots/, gitignored scratch)
        await shot(page, `spend-fit-${scale}-${w}x${h}${docked ? "-docked" : ""}`, { fullPage: false });
        if (docked) {   // and the Meter scrolled to its legend, then to its end, where the live line is
          await page.evaluate(() => { document.querySelector(".sp-legend").scrollIntoView({ block: "center" }); });
          await shot(page, `spend-fit-${scale}-${w}x${h}-docked-legend`, { fullPage: false });
          await page.evaluate(() => { const b = document.querySelector(".sp-body"); b.scrollTop = b.scrollHeight; });
          await shot(page, `spend-fit-${scale}-${w}x${h}-docked-end`, { fullPage: false });
          await page.evaluate(() => { document.querySelector(".sp-body").scrollTop = 0; });
        }
        if (m.periods !== 4) m.problems.push(`${m.periods} of the four periods shown`);
        for (const p of m.problems) wrong.push(`${where}: ${p}`);
        out.push(`${where}: header ${m.rows} row${m.rows > 1 ? "s" : ""}, ${m.height}px, title ${m.title ? "shown" : "hidden"}, live line ${m.liveLines} line${m.liveLines > 1 ? "s" : ""}`);
      }
    }
    await page.close();
    // every size measured before judging, so a failure names each window it fails in
    assertEqual(wrong, [], out.join("; "));
    return out.join("; ");
  });

  // ---- 9c. nothing runs under the footer ----------------------------------------------------
  // The pictures of 9b showed it: in the 536px docked column the Meter is taller than the space
  // between the header and the footer, and it painted over the footer (the tokens line through the
  // price note). Measured: the body ends where the footer starts, and when its content is taller it
  // scrolls inside the body instead of spilling out of it.
  await run.check("nothing runs under the header or the footer at 1440 and 1024, docked or not: undocked the whole Meter is on screen; docked it scrolls inside its own space when it is taller", async () => {
    const out = [], wrong = [];
    const page = await open("dark", undefined);
    await screen(page);
    for (const [w, h, docked] of [[1440, 900, false], [1440, 900, true], [1024, 700, false], [1024, 700, true]]) {
      await page.setViewportSize({ width: w, height: h });
      await page.evaluate(d => document.body.classList.toggle("bug-dock-open", d), docked);
      await settle(page);
      const m = await page.evaluate(docked => {
        const body = document.querySelector(".sp-body"), r = body.getBoundingClientRect();
        const head = document.querySelector(".sp-head").getBoundingClientRect(), foot = document.querySelector(".sp-foot").getBoundingClientRect();
        const scrolls = ["auto", "scroll"].includes(getComputedStyle(body).overflowY);
        const taller = body.scrollHeight > body.clientHeight + 0.5;
        // the content's own extent, whether or not it is clipped
        const ink = [...body.querySelectorAll(".sp-meter > *")].map(e => e.getBoundingClientRect()).filter(x => x.height > 0);
        const top = Math.min(...ink.map(x => x.top)), bottom = Math.max(...ink.map(x => x.bottom));
        const problems = [];
        if (r.top < head.bottom - 0.5) problems.push(`the body starts at ${r.top.toFixed(1)}, under the header (${head.bottom.toFixed(1)})`);
        if (r.bottom > foot.top + 0.5) problems.push(`the body ends at ${r.bottom.toFixed(1)}, under the footer (${foot.top.toFixed(1)})`);
        if (taller && !scrolls) problems.push(`the Meter is ${body.scrollHeight}px in ${body.clientHeight}px and spills out (${top.toFixed(0)} to ${bottom.toFixed(0)}; the footer starts at ${foot.top.toFixed(0)})`);
        if (!taller && (top < r.top - 0.5 || bottom > r.bottom + 0.5)) problems.push(`the Meter (${top.toFixed(0)} to ${bottom.toFixed(0)}) is outside its space (${r.top.toFixed(0)} to ${r.bottom.toFixed(0)})`);
        // undocked, the whole Meter is on screen at once, so a screenshot of the window has all of it
        if (!docked && taller) problems.push(`the Meter is ${body.scrollHeight}px in ${body.clientHeight}px: not all of it on screen`);
        return { problems, room: Math.round(r.height), content: Math.round(body.scrollHeight), taller };
      }, docked);
      const where = `${w} by ${h}${docked ? ", panel docked" : ""}`;
      for (const p of m.problems) wrong.push(`${where}: ${p}`);
      out.push(`${where}: Meter ${m.content}px in ${m.room}px${m.taller ? ", taller, so it scrolls" : ""}`);
    }
    await page.close();
    assertEqual(wrong, [], out.join("; "));
    return out.join("; ");
  });

  // ---- 10. no two characters of the figure touch ---------------------------------------------
  await run.check("no two characters of the figure touch: every period and the panel docked in both themes, and the stress set, measured on the pixels (gap >= 4px)", async () => {
    const out = [];
    const shoot = async (page) => {
      const box = await page.evaluate(() => { const f = document.getElementById("spend-fig"), r = f.getBoundingClientRect(), col = f.parentElement, cs = getComputedStyle(col);
        return { x: r.left, y: r.top, w: r.width, h: r.height, colRight: col.getBoundingClientRect().right - parseFloat(cs.paddingRight), size: parseFloat(getComputedStyle(f).fontSize), text: f.textContent }; });
      const clip = { x: Math.max(0, Math.floor(box.x - 24)), y: Math.max(0, Math.floor(box.y - 24)), width: Math.ceil(box.w + 48), height: Math.ceil(box.h + 48) };
      const pics = [];
      for (const mode of ["even", "odd", "none"]) {
        await page.evaluate((mode) => {
          document.querySelectorAll("#spend-fig .ch").forEach((c, i) => { c.style.animation = "none"; c.style.color = mode === "none" || (mode === "even") !== (i % 2 === 0) ? "transparent" : ""; });
        }, mode);
        pics.push((await page.screenshot({ clip, animations: "disabled" })).toString("base64"));
      }
      await page.evaluate(() => document.querySelectorAll("#spend-fig .ch").forEach((c) => { c.style.color = ""; }));
      return Object.assign(box, await page.evaluate(MEASURE, pics));
    };
    const bad = [];
    let smallest = Infinity;
    const note = (what, r) => {
      smallest = Math.min(smallest, r.gap);
      if (r.gap < MIN_GAP || !(r.x + r.w <= r.colRight + 0.5) || !r.inkA || !r.inkB) bad.push(`${what} ${r.text} ${r.size}px gap ${r.gap}`);
    };
    for (const theme of ["dark", "light"]) {
      const page = await open(theme, undefined);
      await screen(page);
      for (const p of ["today", "week", "month", "all"]) {
        await page.click(`#spend-period button[data-p="${p}"]`);
        await page.waitForTimeout(100); // load-bound: the period's own snapshot; the shot waits on the figure below
        await page.waitForSelector("#spend-fig .ch");
        await settle(page);
        const r = await shoot(page);
        note(theme + " " + p, r);
        out.push(`${theme} ${p} ${r.text} ${r.size}px gap ${r.gap}`);
      }
      await page.evaluate(() => document.body.classList.add("bug-dock-open"));
      await page.waitForFunction(() => parseFloat(getComputedStyle(document.getElementById("spend-fig")).fontSize) > 0);
      await settle(page);
      { const r = await shoot(page); note(theme + " docked", r); out.push(`${theme} docked ${r.text} ${r.size}px gap ${r.gap}`); }
      if (theme === "dark") {
        await page.evaluate(() => document.body.classList.remove("bug-dock-open"));
        for (const s of STRESS) {
          await page.evaluate(s => window.RichSpend.paintFigure(s), s);
          note("stress", await shoot(page));
        }
        // The control: his screenshot's figure set so tight that it must touch, so the walk is seen to
        // see touching. Round 22's own setting (-0.035em, tabular figures) touched in Helvetica Neue,
        // the face the mockup was measured in; in Inter, this app's face, it does not (11px apart,
        // measured 2026-10-11), so the control tightens to -0.15em.
        await page.evaluate(() => { window.RichSpend.paintFigure("$11,686.24"); const f = document.getElementById("spend-fig"); f.style.letterSpacing = "-.15em"; f.style.fontVariantNumeric = "tabular-nums"; f.style.fontFeatureSettings = "normal"; });
        const c = await shoot(page);
        assert(c.gap < 1, "the control (-0.15em) measures as touching, or the walk is blind: gap " + c.gap);
        out.push(`control $11,686.24 at -0.15em: gap ${c.gap}`);
      }
      await page.close();
    }
    assertEqual(bad, [], "figures whose characters come closer than 4px or leave the column");
    return `smallest gap ${smallest}px over ${8 + 2 + STRESS.length} renderings; ` + out.join("; ");
  });

  // ---- 11. contrast and type, both themes ----------------------------------------------------
  for (const theme of ["dark", "light"]) {
    await run.check(theme + ": the screen and the row meet AA (text 4.5:1, large text and indicators 3:1) and the 16px floor", async () => {
      const page = await open(theme, { agents: 4 });
      await screen(page);
      await page.evaluate(() => window.__RICHOS_MOCK__.spendLand({ family: "haiku", name: "Haiku 5.5", effort: "low", cost: 0.00104, tokens: 58828 }));
      await page.waitForSelector("#spend-refresh.has-new");
      await page.waitForFunction(() => /58,828/.test(document.getElementById("spend-live").textContent));
      await menu(page);
      await page.waitForFunction(() => document.getElementById("set-spend-amt").textContent.length > 0);
      await settle(page);
      await page.addScriptTag({ content: contrast.pageScript() });
      const result = await page.evaluate(() => {
        const C = window.__contrastMath, failures = [];
        const parse = s => C.parseCssColor(s) || (m => m && { r: +m[1] * 255, g: +m[2] * 255, b: +m[3] * 255, a: m[4] === undefined ? 1 : +m[4] })(
          /^color\(srgb ([\d.e-]+) ([\d.e-]+) ([\d.e-]+)(?: \/ ([\d.e-]+))?\)$/.exec(String(s).trim()));
        const ground = el => { let bg = { r: 0, g: 0, b: 0, a: 0 }; const chain = []; for (let p = el; p; p = p.parentElement) chain.unshift(p);
          for (const p of chain) { const c = parse(getComputedStyle(p).backgroundColor); if (!c) failures.push({ unparsed: getComputedStyle(p).backgroundColor }); else bg = C.compositeOver(c, bg); } return bg; };
        const ratio = (paint, on) => { const bg = ground(on), c = parse(paint); if (!c) { failures.push({ unparsed: paint }); return 0; } return C.round2(C.contrastRatio(C.compositeOver(c, bg), bg)); };
        let worstText = 99, nodes = 0;
        const roots = [document.getElementById("spend"), document.getElementById("set-spend")];
        for (const root of roots) {
          for (const e of [root, ...root.querySelectorAll("*")]) {
            if (!e.getClientRects().length || e.closest("[hidden]")) continue;
            if (![...e.childNodes].some(n => n.nodeType === Node.TEXT_NODE && n.textContent.trim())) continue;
            const st = getComputedStyle(e), r = ratio(st.color, e), size = parseFloat(st.fontSize), weight = parseInt(st.fontWeight, 10);
            const large = size >= 24 || (size >= 18.66 && weight >= 700);
            nodes++; worstText = Math.min(worstText, r);
            if (r < (large ? 3 : 4.5)) failures.push({ text: e.textContent.trim().slice(0, 40), ratio: r, size });
            if (size < 16) failures.push({ text: e.textContent.trim().slice(0, 40), size });
          }
        }
        const ind = {};
        const sp = document.getElementById("spend");
        for (const k of sp.querySelectorAll(".sp-key:not([hidden]) .sw-m")) {
          const st = getComputedStyle(k), m = k.closest(".sp-key").dataset.m;
          ind["mark " + m] = m === "haiku" ? ratio(st.boxShadow.match(/rgba?\([^)]+\)|color\([^)]+\)/)[0], k.parentElement) : ratio(st.backgroundColor, k.parentElement);
        }
        const btn = document.getElementById("spend-refresh");
        ind["refresh arrow"] = ratio(getComputedStyle(btn.querySelector("svg")).color, btn);
        ind["new-requests dot"] = ratio(getComputedStyle(btn.querySelector(".sp-new")).backgroundColor, btn);
        const pressed = sp.querySelector('#spend-period button[aria-pressed="true"]');
        ind["chosen period"] = ratio(getComputedStyle(pressed).backgroundColor, pressed.parentElement);
        ind["working dot"] = ratio(getComputedStyle(sp.querySelector(".working-dot")).backgroundColor, sp.querySelector("#spend-live"));
        ind["back glyph"] = ratio(getComputedStyle(sp.querySelector(".sp-back svg")).color, sp.querySelector(".sp-back"));
        const signal = sp.querySelector(".sp-wordmark path[fill^='var']");
        ind["wordmark gold"] = signal ? ratio(getComputedStyle(signal).fill, sp.querySelector(".sp-foot")) : 0;
        const row = document.getElementById("set-spend-open");
        ind["row chevron"] = ratio(getComputedStyle(row.querySelector("svg")).color, row);
        for (const [k, v] of Object.entries(ind)) if (v < 3) failures.push({ indicator: k, ratio: v });
        return { failures, worstText, nodes, ind };
      });
      assertEqual(result.failures, [], theme);
      assert(result.nodes >= 20, `${theme}: EMPTY INVENTORY (${result.nodes} text nodes)`);
      await page.close();
      return `worst text ${result.worstText} over ${result.nodes} nodes; ` + Object.entries(result.ind).map(([k, v]) => k + " " + v).join(", ");
    });
  }

  await run.check("no renderer errors", async () => assertEqual(errors, []));
  await browser.close();
  process.exitCode = run.report() ? 1 : 0;
}
main().catch(e => { console.error(e); process.exitCode = 1; });

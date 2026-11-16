// Round 84's acceptance check in shipping WebKit. Sample the field on the
// exact frame it draws, before the fade, at reveal and after it enters idle.
"use strict";
const path = require("node:path");
const { loadPlaywright, createRun, assert, assertEqual, UI_DIR } = require("./lib/harness");
const { SILENT_AUDIO } = require("./lib/silent-audio");

const PROBE = () => {
  const P = (window.__entranceProbe = { rows: [], attached: false, error: null, cost: 0 });
  const raf = window.requestAnimationFrame.bind(window);
  const TW = 288, TH = 180;
  const cv = document.createElement("canvas");
  cv.width = TW; cv.height = TH;
  const cx = cv.getContext("2d", { willReadFrequently: true });
  let seen = null, geo = null, lastFrames = -1;
  function prepare(lo) {
    const D = lo.D, N = lo.N, home = D.home, cx0 = D.CORE.x, cy0 = D.CORE.y;
    const span = new Float32Array(N), ring = new Uint8Array(N), all = [];
    for (let i = 0; i < N; i++) { span[i] = Math.hypot(home[i * 2] - cx0, home[i * 2 + 1] - cy0); if (span[i] >= 1) all.push(span[i]); }
    all.sort((a, b) => a - b);
    const t1 = all[Math.floor(all.length / 3)], t2 = all[Math.floor((all.length * 2) / 3)];
    for (let i = 0; i < N; i++) ring[i] = span[i] < t1 ? 0 : span[i] < t2 ? 1 : 2;
    geo = { N, home, span, ring, cx0, cy0 };
  }
  function sample() {
    const lo = window.__loro;
    if (!lo || !lo.D || !lo.pos) return;
    if (lo !== seen) { seen = lo; prepare(lo); lastFrames = -1; }
    const drawn = lo.frames !== lastFrames;
    lastFrames = lo.frames;
    const g = geo, pos = lo.pos, gate = lo.gate, home = g.home;
    let n = 0, travel = 0, landed = 0;
    const rn = [0, 0, 0], rl = [0, 0, 0];
    for (let i = 0; i < g.N; i++) {
      if (g.span[i] < 1 || (gate && gate[i] <= 0.004)) continue;
      const d = Math.hypot(pos[i * 2] - home[i * 2], pos[i * 2 + 1] - home[i * 2 + 1]);
      n++; travel += Math.max(0, 1 - d / g.span[i]);
      rn[g.ring[i]]++;
      if (d < 0.75) { landed++; rl[g.ring[i]]++; }
    }
    const splash = document.getElementById("splash");
    const ent = lo.entrance || null;
    const row = {
      t: performance.now(),
      cov: !!splash,
      lifting: !!(splash && splash.classList.contains("splash--yielding")),
      open: !!(window.RichHome && window.RichHome.isOpen && window.RichHome.isOpen()),
      fr: lo.frames, drawn: drawn, bl: !!lo.blooming,
      st: ent ? ent.state : null, clk: ent ? ent.clock : null,
      travel: n ? travel / n : 0, landed: n ? landed / n : 0,
      ring: [0, 1, 2].map((k) => (rn[k] ? rl[k] / rn[k] : 0)),
      light: null, core: null,
    };
    if (drawn) {
      const t0 = performance.now();
      const gl = document.getElementById("home-gl");
      if (gl && gl.width) {
        cx.clearRect(0, 0, TW, TH);
        cx.drawImage(gl, 0, 0, TW, TH);
        const d = cx.getImageData(0, 0, TW, TH).data;
        let sum = 0, cs = 0, cn = 0, ccx = TW / 2, ccy = TH / 2;
        try { const c = lo.cam, W = innerWidth, H = innerHeight; ccx = ((g.cx0 - c.x) * c.s + W / 2) * (TW / W); ccy = ((g.cy0 - c.y) * c.s + H / 2) * (TH / H); } catch (e) { /* the middle will do */ }
        for (let y = 0, q = 0; y < TH; y++) for (let x = 0; x < TW; x++, q += 4) {
          const l = (0.2126 * d[q] + 0.7152 * d[q + 1] + 0.0722 * d[q + 2]) * (d[q + 3] / 255) / 255;
          sum += l;
          if (Math.abs(x - ccx) <= 9 && Math.abs(y - ccy) <= 9) { cs += l; cn++; }
        }
        row.light = sum / (TW * TH);
        row.core = cn ? cs / cn : 0;

      }
      P.cost = Math.max(P.cost, performance.now() - t0);
    }
    P.rows.push(row);
  }
  window.requestAnimationFrame = function (cb) {
    if (typeof cb === "function" && cb.name === "frame") {
      P.attached = true;
      return raf(function (ts) {
        cb(ts);
        try { sample(); } catch (e) { P.error = String((e && e.message) || e); }
      });
    }
    return raf(cb);
  };
};

const pct = (x) => (x * 100).toFixed(x > 0 && x < 0.0995 ? 1 : 0) + "%";
const median = (a) => { const s = a.slice().sort((x, y) => x - y); return s.length ? s[Math.floor(s.length / 2)] : 0; };
function judge(rows) {
  const out = [];
  const say = (ok, rule, detail) => out.push({ ok, rule, detail });
  // An engine keeps every light at its place in memory until its bloom first steps, and one build of
  // it (the app on richos main, 2026-11-16) runs a first frame before that step: one reading with
  // every light "landed" and no light on the canvas, then every light at the core. That reading is the
  // engine's starting memory, not a picture, and is left out.
  while (rows.length > 1 && rows[0].bl && rows[0].landed > 0.99 && rows[1].landed < 0.5 && !(rows[0].light > 0.001)) rows.shift();
  if (!rows.length) return { out, fatal: "the picture's engine never ran a frame" };
  const iRev = rows.findIndex((r) => !r.cov && r.open);
  if (iRev < 0) return { out, fatal: "the picture was never uncovered" };
  const covered = rows.slice(0, iRev).filter((r) => r.cov);
  const after = rows.slice(iRev);
  const tRev = rows[iRev].t;
  const iEnd = after.findIndex((r) => !r.bl);
  const tEnd = iEnd >= 0 ? after[iEnd].t : NaN;
  const lit = after.filter((r) => r.light !== null);
  const settledRows = lit.filter((r) => r.t > (isNaN(tEnd) ? after[after.length - 1].t - 1500 : tEnd + 1200));
  const Lset = median(settledRows.map((r) => r.light)) || 1e-9;
  const Cset = median(settledRows.map((r) => r.core)) || 1e-9;
  const firstLit = lit[0] || after[0];
  const exposed = rows.some((r) => r.clk !== null);

  // 1. the clock does not run while the picture is covered
  {
    const maxTravel = covered.reduce((m, r) => Math.max(m, r.travel), 0);
    const maxLanded = covered.reduce((m, r) => Math.max(m, r.landed), 0);
    const maxClock = covered.reduce((m, r) => Math.max(m, r.clk || 0), 0);
    const endedUnder = covered.some((r) => !r.bl);
    const lifting = covered.filter((r) => r.lifting);
    const atLift = lifting.length ? lifting[0] : null;
    const ok = maxTravel <= 0.002 && maxLanded <= 0.002 && maxClock === 0 && !endedUnder;
    let d;
    if (!covered.length) d = "no curtain was drawn in this launch, so the picture was never covered";
    else if (ok) d = "under the curtain for " + ((covered[covered.length - 1].t - covered[0].t) / 1000).toFixed(1) + " s: no light left the core" + (exposed ? ", the entrance clock stayed at 0.00 s" : "") + ", the bloom had not begun";
    else d = "under the curtain the lights came " + pct(maxTravel) + " of the way from the core and " + pct(maxLanded) + " of them landed" + (atLift ? " (" + pct(atLift.travel) + " of the way when the curtain began to lift)" : "") + (exposed ? ", the entrance clock reached " + maxClock.toFixed(2) + " s" : "") + (endedUnder ? "; the bloom ended while it was covered" : "");
    say(ok, "The bloom clock does not run while the picture is covered", d);
  }
  // 2. the first visible picture is not already assembled
  {
    const r = rows[iRev];
    const light = firstLit.light === null ? 0 : firstLit.light / Lset, core = firstLit.core === null ? 0 : firstLit.core / Cset;
    const ok = r.travel <= 0.002 && r.landed <= 0.002 && light <= 0.15 && core <= 0.2 && r.bl;
    say(ok, "The first visible picture is nearly dark, not assembled",
      "first frame with nothing over it: lights " + pct(r.travel) + " of the way from the core, " + pct(r.landed) + " landed, the picture at " + pct(light) + " of its settled light, the core at " + pct(core) + " of its own" + (r.bl ? "" : "; the bloom was already over"));
  }
  // 3. the core comes up first
  {
    const tCore = (lit.find((r) => r.core >= 0.5 * Cset) || {}).t;
    const tTenth = (after.find((r) => r.landed >= 0.1) || {}).t;
    const ok = tCore !== undefined && tTenth !== undefined && tCore > tRev && tCore < tTenth;
    say(ok, "The core comes up first",
      tCore === undefined || tTenth === undefined ? "the core or the lights never came up while the picture was watched"
        : tCore <= tRev ? "the core was already lit on the first visible frame"
        : "the core is at half its light " + ((tCore - tRev) / 1000).toFixed(2) + " s after the reveal; a tenth of the lights have landed at " + ((tTenth - tRev) / 1000).toFixed(2) + " s");
  }
  // 4. the lights travel outward
  {
    const t50 = [0, 1, 2].map((k) => (after.find((r) => r.ring[k] >= 0.5) || {}).t);
    const at0 = rows[iRev].ring;
    // no third may have begun to fill before the picture is seen (5% is the line, so a launch that
    // reveals mid-bloom fails here every time and not only when a third happens to be half full)
    const ok = t50.every((t) => t !== undefined) && at0.every((v) => v < 0.05) && t50[0] < t50[1] && t50[1] < t50[2];
    say(ok, "The lights travel outward from the core",
      t50.some((t) => t === undefined) ? "a third of the picture never filled while it was watched"
        : "half landed, seconds after the reveal: inner third " + ((t50[0] - tRev) / 1000).toFixed(2) + ", middle " + ((t50[1] - tRev) / 1000).toFixed(2) + ", outer " + ((t50[2] - tRev) / 1000).toFixed(2) + (at0.some((v) => v >= 0.05) ? "; but on the first visible frame " + at0.map(pct).join(", ") + " of the three had already landed" : ""));
  }
  // 5. into idle: no restart, no freeze, no pop
  {
    let top = 0, fell = 0, back = false, clockBack = false, lastClock = -1, ended = false;
    for (const r of after) {
      if (r.landed > top) top = r.landed; else fell = Math.max(fell, top - r.landed);
      if (!r.bl) ended = true; else if (ended) back = true;
      if (r.clk !== null) { if (r.clk < lastClock - 1e-6) clockBack = true; lastClock = r.clk; }
    }
    const drawnRows = after.filter((r) => r.drawn);
    let gap = 0, gapAt = 0;
    for (let i = 1; i < drawnRows.length; i++) { const g = drawnRows[i].t - drawnRows[i - 1].t; if (g > gap) { gap = g; gapAt = drawnRows[i - 1].t; } }
    let pop = 0, popAt = 0;
    for (let i = 1; i < lit.length; i++) {
      const frames = Math.max(1, (lit[i].t - lit[i - 1].t) / 16.7);
      const rise = Math.abs(lit[i].light - lit[i - 1].light) / Lset / frames;
      if (rise > pop) { pop = rise; popAt = lit[i].t; }
    }
    const ok = fell <= 0.02 && !back && !clockBack && gap <= 250 && pop <= 0.06 && iEnd > 0;
    const d = [];
    d.push(iEnd < 0 ? "the bloom never ended" : iEnd === 0 ? "the bloom was over before the picture was seen" : "the bloom ended " + ((tEnd - tRev) / 1000).toFixed(2) + " s after the reveal");
    d.push(back || clockBack || fell > 0.02 ? "it started again (" + pct(fell) + " of the landed lights left their places)" : "nothing started again");
    d.push("longest wait between two frames " + Math.round(gap) + " ms" + (gap > 250 ? " (" + ((gapAt - tRev) / 1000).toFixed(2) + " s in)" : ""));
    d.push("largest change of the picture's light in one frame " + pct(pop) + " of its settled light" + (pop > 0.06 ? " (" + ((popAt - tRev) / 1000).toFixed(2) + " s in)" : ""));
    say(ok, "The bloom runs into idle without a pop, a freeze or a restart", d.join("; "));
  }
  return { out, iRev, tRev, tEnd, Lset, Cset, exposed };
}


async function main() {
  const run = createRun("Loro starts from its opening state after the splash fade");
  const browser = await loadPlaywright().webkit.launch();
  try {
    for (const [id, kind] of [
      ["round-11/v1", "fresh"],
      ["opening/power-100", "fresh"],
      [null, "crash-restart"],
    ]) {
      await run.check(id || "a launch with no splash", async () => {
        const ctx = await browser.newContext({ viewport: { width: 1440, height: 900 } });
        try {
          await ctx.addInitScript(PROBE);
          await ctx.addInitScript(SILENT_AUDIO);
          await ctx.addInitScript(({ id, kind }) => {
            window.__RICHOS_LAUNCH__ = { kind, splash: id ? { id } : null };
          }, { id, kind });
          const page = await ctx.newPage();
          await page.goto("file://" + path.join(UI_DIR, "index.html"));
          await page.waitForFunction(() => {
            const rows = window.__entranceProbe.rows;
            const settled = rows.find((row) => !row.bl && row.drawn && !row.cov);
            return settled && performance.now() - settled.t > 1500;
          }, null, { timeout: 60000 });
          const { attached, rows, error } = await page.evaluate(() => window.__entranceProbe);
          assert(attached && !error && rows.length, error || "The field never drew");
          assertEqual(rows.some((row) => row.cov && row.drawn), !!id, "cover behavior");
          const verdict = judge(rows);
          assert(!verdict.fatal, verdict.fatal);
          for (const rule of verdict.out) assert(rule.ok, rule.rule + ": " + rule.detail);
          const first = rows.find((row) => !row.cov && row.open && row.drawn);
          assertEqual(first.clk, 0, "the first uncovered draw must be frame zero");
          return verdict.out.map((rule) => rule.detail).join(" · ");
        } finally {
          await ctx.close();
        }
      });
    }
  } finally {
    await browser.close();
  }
  process.exitCode = run.report() ? 1 : 0;
}

main().catch((error) => { console.error(error); process.exitCode = 1; });

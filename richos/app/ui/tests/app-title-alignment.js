#!/usr/bin/env node
// THE APP TITLE IS CENTERED ON THE NAME'S LETTERS (design round 105, CEO 2026-11-23).
//
// His words: "I'm seeing this weird misalignment on all the screens when clicking the
// corresponding items on the nav rail ... do it in a way that makes sure that engineers can't
// mess it up." and "I would expect the name and the icon to be center-aligned."
//
// The rule this enforces: on every header that shows an app icon beside the app's name, the
// icon's vertical center and the center of the name's capital letters (baseline to the top of
// an "H"; never the line box with its empty space above the capitals) differ by at most 1px.
//
// WHAT IT MEASURES IS THE RENDERED PICTURE, NOT THE CSS. For each header it paints the page
// on a flat green ground, photographs the icon alone (name hidden) and finds the rows its
// pixels cover, then replaces the name with "H" in the same font, photographs it alone (icon
// hidden) and finds the rows the capital covers. Both centers come from those pixels, at 2x.
// So a font swap, a size change, a stray margin or a rule from another file that moves either
// one is seen exactly as Alex would see it.
//
// WHAT ELSE FAILS IT
//   - a header with an app icon beside a name that does not use the component
//     (`.app-title`, round-105/app-title.css): found by looking, on every surface it opens,
//     for an app icon whose parent is a heading, sits beside a heading or is inside a <header>;
//   - a stylesheet rule outside the component that sets font, size, line height, margins,
//     padding, top, transform or alignment on `.app-title`, `.app-title-icon` or
//     `.app-title-name` (a screen keeping its own copy of the geometry);
//   - a surface in the list below that no longer shows an app title at all.
//
// HOW TO RUN
//   The round's own specimen sheet (both themes, every text size the app offers):
//     RICHOS_PLAYWRIGHT=<path>/node_modules/playwright node check/app-title-alignment.js
//   The RichOS app itself (its ui/ folder: index.html and cleaner.html):
//     RICHOS_PLAYWRIGHT=<path>/node_modules/playwright node check/app-title-alignment.js --ui <richos>/richos/app/ui
//   Copied into richos/app/ui/tests/ it runs as an ordinary suite of that harness
//   (`node app-title-alignment.js`, or `npm test`), on the app beside it, with the harness's
//   own Playwright and report.
//
// Exit 0: every header passes. Exit 1: at least one fails; each failure says which header,
// which theme and size, and by how many pixels.
"use strict";

const path = require("path");
const fs = require("fs");
const zlib = require("zlib");
const { fileURLToPath } = require("url");

const TOLERANCE_PX = 1;
const SCALE = 2; // device pixels per CSS pixel for the photographs
const GREEN = [0, 255, 0];
const GEOMETRY = ["font", "font-size", "font-family", "line-height", "height", "min-height", "max-height",
  "margin", "margin-top", "margin-bottom", "margin-block", "margin-block-start", "margin-block-end",
  "padding", "padding-top", "padding-bottom", "padding-block", "top", "bottom", "inset", "transform",
  "translate", "vertical-align", "align-items", "align-self", "position", "display",
  "--app-title-font", "--app-title-icon", "--app-title-mark", "--app-title-gap", "--app-title-lift",
  "--app-title-tracking"];
const COMPONENT_SELECTORS = [".app-title", ".app-title > .app-title-icon", ".app-title > .app-title-mark",
  ".app-title > .app-title-mark > img", ".app-title > .app-title-name", ".app-title--page",
  ".app-title--sheet", ".app-title--bar"];

// ---- where it runs ----------------------------------------------------------------------
const args = process.argv.slice(2);
const uiArg = args.includes("--ui") ? path.resolve(args[args.indexOf("--ui") + 1]) : null;
const insideAppTests = path.basename(__dirname) === "tests" && fs.existsSync(path.join(__dirname, "..", "index.html"));
const UI = uiArg || (insideAppTests ? path.resolve(__dirname, "..") : null);
const ROUND = path.resolve(__dirname, "..");

let harness = null;
if (insideAppTests) { try { harness = require("./lib/harness"); } catch (e) { harness = null; } }

function loadPlaywright() {
  if (harness) return harness.loadPlaywright();
  const tries = [process.env.RICHOS_PLAYWRIGHT, "playwright", UI && path.join(UI, "node_modules", "playwright")].filter(Boolean);
  for (const t of tries) { try { return require(t); } catch (e) { /* next */ } }
  console.error("Playwright not found. Set RICHOS_PLAYWRIGHT to a node_modules/playwright folder.");
  process.exit(2);
}

function createRun(label) {
  if (harness) return harness.createRun(label);
  const results = [];
  return {
    async check(name, fn) {
      try { results.push({ name, ok: true, detail: (await fn()) || "" }); }
      catch (e) { results.push({ name, ok: false, detail: (e && e.message) || String(e) }); }
    },
    report() {
      console.log("\n" + label + "\n");
      for (const r of results) console.log((r.ok ? "PASS  " : "FAIL  ") + r.name + (r.detail ? "\n        " + r.detail.split("\n").join("\n        ") : ""));
      const failed = results.filter((r) => !r.ok).length;
      console.log("\n" + (results.length - failed) + " passed, " + failed + " failed");
      return failed;
    },
  };
}
function assert(cond, msg) { if (!cond) throw new Error(msg); }

// ---- a PNG decoder (8-bit RGB/RGBA, the only kind a browser screenshot is) ----------------
function decodePng(buf) {
  let pos = 8, width = 0, height = 0, type = 0;
  const idat = [];
  while (pos < buf.length) {
    const len = buf.readUInt32BE(pos), kind = buf.toString("ascii", pos + 4, pos + 8), data = buf.subarray(pos + 8, pos + 8 + len);
    if (kind === "IHDR") { width = data.readUInt32BE(0); height = data.readUInt32BE(4); type = data[9]; assert(data[8] === 8, "16-bit PNG"); }
    if (kind === "IDAT") idat.push(data);
    pos += 12 + len;
  }
  const bpp = type === 6 ? 4 : 3, stride = width * bpp, raw = zlib.inflateSync(Buffer.concat(idat));
  const out = Buffer.alloc(height * stride);
  for (let y = 0; y < height; y++) {
    const f = raw[y * (stride + 1)], src = y * (stride + 1) + 1;
    for (let x = 0; x < stride; x++) {
      const a = x >= bpp ? out[y * stride + x - bpp] : 0, b = y ? out[(y - 1) * stride + x] : 0, c = x >= bpp && y ? out[(y - 1) * stride + x - bpp] : 0;
      let v = raw[src + x];
      if (f === 1) v += a; else if (f === 2) v += b; else if (f === 3) v += (a + b) >> 1;
      else if (f === 4) { const p = a + b - c, pa = Math.abs(p - a), pb = Math.abs(p - b), pc = Math.abs(p - c); v += pa <= pb && pa <= pc ? a : pb <= pc ? b : c; }
      out[y * stride + x] = v & 255;
    }
  }
  return { width, height, bpp, px: out };
}
// The rows a photograph's non-green pixels cover, in that photograph's own device pixels.
function inkRows(png) {
  let top = -1, bottom = -1;
  for (let y = 0; y < png.height; y++) {
    for (let x = 0; x < png.width; x++) {
      const i = (y * png.width + x) * png.bpp;
      const d = Math.abs(png.px[i] - GREEN[0]) + Math.abs(png.px[i + 1] - GREEN[1]) + Math.abs(png.px[i + 2] - GREEN[2]);
      if (d > 48) { if (top < 0) top = y; bottom = y; break; }
    }
  }
  return top < 0 ? null : { top, bottom: bottom + 1 };
}

// ---- in the page ------------------------------------------------------------------------
// Finds every header: the component's, and (as "before" or "outside") an app icon beside a
// name that is not the component's.
const PAGE = `(() => {
  window.__r105 = {
    titles() {
      const found = [];
      document.querySelectorAll(".app-title").forEach((t) => found.push({ kind: "component", t,
        icon: t.querySelector(":scope > .app-title-icon"), name: t.querySelector(":scope > .app-title-name") }));
      const isIcon = (n) => n.matches('img[src*="app-icons/"]') || n.matches(".rp-mark") || n.matches("[data-icon]");
      document.querySelectorAll('img[src*="app-icons/"], .rp-mark, [data-icon]').forEach((icon) => {
        if (icon.closest(".app-title")) return;
        if (icon.matches(".rp-mark img") || (icon.parentElement && icon.parentElement.matches(".rp-mark"))) return;
        const box = icon.parentElement;
        const heading = box.matches("h1,h2,h3,h4,h5,h6") ? box : box.querySelector(":scope > h1, :scope > h2, :scope > h3, :scope > h4");
        const inHeader = !!icon.closest("header");
        if (!heading && !inHeader) return;
        const name = icon.closest("[data-ttl]")?.querySelector("[data-name]") || heading || box;
        if (!name.textContent.trim()) return;
        found.push({ kind: icon.closest("[data-state=before]") ? "before" : "outside", t: box, icon, name });
      });
      return found.filter((f) => f.icon && f.name && f.icon.getClientRects().length);
    },
    label(f) {
      const spec = f.t.closest("[data-spec]");
      return (spec ? spec.dataset.spec : (f.name.textContent.trim() || "?")) + (f.kind === "component" ? "" : " (" + f.kind + ")");
    },
    rect(el) { const r = el.getBoundingClientRect(); return { x: r.left, y: r.top, w: r.width, h: r.height }; },
    ground(on) {
      let s = document.getElementById("r105-ground");
      if (on && !s) { s = document.createElement("style"); s.id = "r105-ground";
        // A flat green ground with nothing painted on it but the one header being measured:
        // every other text, picture, border and shadow on the screen goes transparent (an app
        // screen has a whole conversation behind a sheet). Every animation and transition stops
        // too: a sheet still sliding in would put the icon photographed at one moment against
        // letters photographed at another.
        s.textContent = [
          "*,*::before,*::after,*::placeholder{animation:none!important;transition:none!important;background:transparent!important;box-shadow:none!important;border-color:transparent!important;outline-color:transparent!important;text-shadow:none!important;color:transparent!important;-webkit-text-fill-color:transparent!important;caret-color:transparent!important;text-decoration-color:transparent!important}",
          "img,svg,video,canvas,iframe,input,textarea,select{opacity:0!important}",
          "html{background:#00ff00!important}",
          ".r105-on,.r105-on *{color:#000!important;-webkit-text-fill-color:#000!important}",
          ".r105-on img,.r105-on svg{opacity:1!important}",
        ].join(" ");
        document.head.appendChild(s); }
      if (!on && s) s.remove();
    },
    only(f, part) {
      // part "icon": the name hidden; part "caps": the icon hidden and the name's words an "H".
      // Hiding changes paint only, never layout: either the letters go transparent or the icon does.
      this.restore(f);
      f.t.classList.add("r105-on");
      if (part === "icon") {
        f.__color = f.name.style.cssText;
        f.name.style.setProperty("color", "transparent", "important");
        f.name.style.setProperty("-webkit-text-fill-color", "transparent", "important");
      } else {
        f.__iconVis = f.icon.style.cssText; f.icon.style.setProperty("opacity", "0", "important");
        f.__saved = []; let first = true;
        const walk = (n) => n.childNodes.forEach((c) => { if (c.nodeType === 3 && c.textContent.trim()) { f.__saved.push([c, c.textContent]); c.textContent = first ? "H" : ""; first = false; } else if (c.nodeType === 1 && c !== f.icon) walk(c); });
        walk(f.name);
      }
    },
    restore(f) {
      (f.__saved || []).forEach(([n, t]) => { n.textContent = t; }); f.__saved = [];
      if (f.__color !== undefined) { f.name.style.cssText = f.__color; delete f.__color; }
      if (f.__iconVis !== undefined) { f.icon.style.cssText = f.__iconVis; delete f.__iconVis; }
      f.t.classList.remove("r105-on");
    },
    // Rules outside the component that set geometry on the component's elements.
    strays({ props, styles, componentSelectors, componentFile }) {
      const out = [];
      const els = [...document.querySelectorAll(".app-title, .app-title > .app-title-icon, .app-title > .app-title-name")];
      const visit = (rules, href, allowed = new Set()) => { for (const r of rules) {
        if (r.cssRules && !r.selectorText) { visit(r.cssRules, href); continue; }
        if (!r.selectorText || allowed.delete(r.selectorText)) continue;
        if (!/[.#\\[]/.test(r.selectorText)) continue; // element-only resets (h1, h2, html, *)
        const set = props.filter((p) => r.style.getPropertyValue(p));
        if (!set.length) continue;
        for (const el of els) { let hit = false; try { hit = el.matches(r.selectorText); } catch (e) {}
          if (hit) { out.push((href.split("/").pop() || "inline") + ": " + r.selectorText + " { " + set.join(", ") + " }"); break; } }
      } };
      for (const sheet of document.styleSheets) {
        if (!sheet.href) visit(sheet.cssRules, "inline <style>");
      }
      for (const { href, css } of styles) {
        const sheet = new CSSStyleSheet();
        sheet.replaceSync(css);
        visit(sheet.cssRules, href, href === componentFile ? new Set(componentSelectors) : new Set());
      }
      return [...new Set(out)];
    },
  };
})();`;

async function measureAll(page) {
  await page.evaluate(PAGE);
  const count = await page.evaluate(async () => {
    await document.fonts.ready;
    const probe = window.__r105;
    probe.found = probe.titles();
    const images = probe.found.flatMap(({ icon }) =>
      icon.matches("img") ? [icon] : [...icon.querySelectorAll("img")]);
    // A broken image still has to fail the pixel measurement for its own header.
    await Promise.allSettled(images.map((img) => img.decode()));
    return probe.found.length;
  });
  const out = [];
  await page.evaluate(() => window.__r105.ground(true));
  await page.evaluate(() => new Promise((done) => requestAnimationFrame(() => requestAnimationFrame(done))));
  for (let i = 0; i < count; i++) {
    const label = await page.evaluate((i) => window.__r105.label(window.__r105.found[i]), i);
    const kind = await page.evaluate((i) => window.__r105.found[i].kind, i);
    const shot = async (part) => {
      const r = await page.evaluate(([i, part]) => { const f = window.__r105.found[i]; window.__r105.only(f, part);
        return window.__r105.rect(part === "icon" ? f.icon : f.name); }, [i, part]);
      // The photograph is the element's own box and nothing around it, so a neighbor (the
      // "Settings" line above a sheet title, a hint under it) never counts as its ink.
      const y0 = Math.floor(r.y), x0 = Math.floor(r.x), clip = { x: x0, y: y0, width: Math.ceil(r.x + r.w) - x0, height: Math.ceil(r.y + r.h) - y0 };
      const rows = inkRows(decodePng(await page.screenshot({ clip })));
      assert(rows, label + ": nothing painted for the " + part);
      return clip.y + (rows.top + rows.bottom) / 2 / SCALE;
    };
    try {
      const icon = await shot("icon");
      const caps = await shot("caps");
      out.push({ label, kind, icon: +icon.toFixed(2), caps: +caps.toFixed(2), offset: +(icon - caps).toFixed(2) });
    } catch (e) {
      out.push({ label, kind, error: e.message, offset: Infinity }); // a header that cannot be measured fails
    }
    await page.evaluate((i) => window.__r105.restore(window.__r105.found[i]), i);
  }
  await page.evaluate(() => window.__r105.ground(false));
  // WebKit cannot expose the cssRules of a linked file:// stylesheet. Parse the same files
  // for inspection in the renderer so per-screen overrides are checked even on disk.
  const hrefs = await page.evaluate(() => [...document.styleSheets].map((sheet) => sheet.href).filter(Boolean));
  const styles = hrefs.map((href) => ({ href, css: fs.readFileSync(fileURLToPath(href), "utf8") }));
  const componentFile = new URL(UI ? "style.css" : "app-title.css", page.url()).href;
  const strays = await page.evaluate((inputs) => window.__r105.strays(inputs), {
    props: GEOMETRY, styles, componentSelectors: COMPONENT_SELECTORS, componentFile,
  });
  return { titles: out, strays };
}

function describe(m) {
  if (m.error) return m.error;
  const d = Math.abs(m.offset).toFixed(2);
  return m.label + ": icon center " + m.icon + ", capitals' center " + m.caps + " -> " + (Math.abs(m.offset) <= TOLERANCE_PX ? "centered (" + d + "px)" : "icon " + d + "px " + (m.offset > 0 ? "LOW" : "HIGH"));
}

// ---- the surfaces of the app ------------------------------------------------------------
// Each entry opens one place a header with an app icon lives, the way the app opens it.
const CLEANER_BRIDGE = `window.__TAURI__={core:{invoke:async(name)=>{if(name==='cleaner_status')return {enabled:true,key:7,theme:window.__r105theme,fontScale:window.__r105scale,operation:{stage:'idle'}};if(name==='cleaner_list')return {entries:[],total:0,next:null};return null;}},event:{listen:async()=>()=>{}}};`;
// RichGlance's settings sheet exists once the app says RichGlance's shortcut is available.
const GLANCE_BRIDGE = `(()=>{let b;Object.defineProperty(window,"RichBridge",{configurable:true,get:()=>b,set(v){const inv=v.invoke.bind(v);b={...v,invoke:async(c,a)=>{if(c==="glance_capture_status")return {available:true,enabled:true,registered:true,shortcut:{code:"KeyC",control:true,shift:true,option:false,command:false}};if(c==="glance_workflow_status")return {opensEditor:false,keepsHistory:false};return inv(c,a);}};}});})();`;
const APP_SURFACES = [
  { name: "RichFiles page", file: "index.html", show: "#richfiles-page" },
  { name: "RichBrief page", file: "index.html", show: "#brief-page" },
  { name: "RichGlance page", file: "index.html", show: "#glance-page" },
  { name: "RichPass page", file: "index.html", show: "#richpass-page" },
  { name: "RichScribe sheet", file: "index.html", panel: "#dictation-sheet", open: "window.RichDictation && window.RichDictation.open()" },
  { name: "RichCleaner settings sheet", file: "index.html", panel: "#cleaner-settings", open: "window.RichCleaner && window.RichCleaner.open()" },
  { name: "RichCleaner window", file: "cleaner.html", panel: ".rf-win", bridge: CLEANER_BRIDGE },
  { name: "RichGlance settings sheet", file: "index.html", panel: "#richglance-capture-settings", bridge: GLANCE_BRIDGE,
    open: async (page) => { await page.click("#set-btn"); await page.click("#set-richglance-capture-open"); } },
];

async function openAppSurface(browser, s, theme, scale) {
  const page = await browser.newPage({ viewport: { width: 1440, height: 900 }, deviceScaleFactor: SCALE, reducedMotion: "reduce" });
  await page.addInitScript(([theme, scale, bridge]) => {
    window.__r105theme = theme; window.__r105scale = scale;
    localStorage.setItem("richos-theme", theme);
    localStorage.setItem("richos-font-scale", String(scale));
    localStorage.setItem("richos-mock-config", JSON.stringify({ theme, font_scale: scale, user_name: "Alex Booster" }));
    window.__RICHOS_MOCK_PRESET__ = { richFiles: true, dictation: true, dictationView: { on: true, keyTap: true, owner: "self", mic: "allowed", ax: "allowed" } };
    if (bridge) (0, eval)(bridge);
  }, [theme, scale, s.bridge || ""]);
  await page.goto("file://" + path.join(UI, s.file));
  if (s.file === "index.html") {
    if (harness) {
      await harness.leaveHome(page);
      await harness.shellSettled(page);
    } else {
      await page.evaluate(() => {
        const home = document.getElementById("home");
        if (home) home.hidden = true;
        document.body.classList.remove("home-open");
      });
    }
  }
  await page.evaluate(([theme, scale]) => { document.documentElement.dataset.theme = theme; document.documentElement.style.setProperty("--app-font-scale", String(scale / 100)); }, [theme, scale]);
  if (s.show) await page.evaluate((sel) => { document.querySelectorAll(".rp-page, #brief-page, #glance-page").forEach((p) => { p.hidden = true; });
    const e = document.querySelector(sel); e.hidden = false; e.style.display = "block"; }, s.show);
  if (s.open) await (typeof s.open === "function" ? s.open(page) : page.evaluate(s.open));
  if (s.panel) await page.locator(s.panel).waitFor({ state: "visible" });
  return page;
}

// ---- main ---------------------------------------------------------------------------------
async function main() {
  const run = createRun("App titles: the icon is centered on the name's capitals, within " + TOLERANCE_PX + "px (round 105, CEO 2026-11-23)");
  const { webkit } = loadPlaywright();
  const browser = await webkit.launch({ headless: true });
  // Both themes, at every text size the app offers (theme-boot.js STEPS).
  const themes = ["light", "dark"], scales = [80, 90, 100, 110, 120, 135, 150];
  try {
    if (!UI) {
      // THE ROUND'S SPECIMEN SHEET: every header, before (main) and after (the component).
      for (const theme of themes) for (const scale of scales) {
        const page = await browser.newPage({ viewport: { width: 1400, height: 900 }, deviceScaleFactor: SCALE, reducedMotion: "reduce" });
        await page.goto("file://" + path.join(ROUND, "headers.html") + "?guides=off&theme=" + theme + "&scale=" + scale);
        // One tall window, so nothing scrolls and every coordinate is the page's own.
        await page.evaluate(() => document.fonts.ready);
        const tall = await page.evaluate(() => document.documentElement.scrollHeight);
        await page.setViewportSize({ width: 1400, height: Math.min(tall + 200, 8000) });
        const m = await measureAll(page);
        const after = m.titles.filter((t) => t.kind === "component"), before = m.titles.filter((t) => t.kind === "before");
        await run.check("NEGATIVE CONTROL, " + theme + " " + scale + "%: every header as it is on main measures off by more than " + TOLERANCE_PX + "px", async () => {
          assert(before.length === 8, "expected 8 before headers, found " + before.length);
          // Two of them happen to line up today, both set in Inter, whose capitals sit in the
          // middle of its line box: the RichCleaner window's bar and the RichGlance settings sheet
          // (its title has no rule of its own and falls back to the browser's bold h2).
          const fine = before.filter((t) => Math.abs(t.offset) <= TOLERANCE_PX && !/window|richglance-settings/.test(t.label));
          assert(!fine.length, "the check sees no misalignment in: " + fine.map(describe).join("; "));
          return before.map(describe).join("\n");
        });
        await run.check("every app title is centered, " + theme + " " + scale + "%", async () => {
          assert(after.length === 8, "expected 8 component headers, found " + after.length);
          const bad = after.filter((t) => Math.abs(t.offset) > TOLERANCE_PX);
          assert(!bad.length, bad.map(describe).join("\n"));
          return after.map(describe).join("\n");
        });
        await run.check("no rule outside the component sets the app title's geometry, " + theme + " " + scale + "%", async () => {
          assert(!m.strays.length, "these rules keep their own copy:\n" + m.strays.join("\n"));
        });
        if (theme === "light" && scale === 100) {
          // An engineer "tidying" the component from a screen's own stylesheet: one override of
          // the one number that does the centering.
          await page.addStyleTag({ content: ".spec .app-title > .app-title-name { top: 0 }" });
          const broken = await measureAll(page);
          await run.check("NEGATIVE CONTROL: a screen's own rule that undoes the centering fails the check twice over", async () => {
            const bad = broken.titles.filter((t) => t.kind === "component" && Math.abs(t.offset) > TOLERANCE_PX);
            assert(bad.length >= 6, "only " + bad.length + " headers measured off with the centering undone");
            assert(broken.strays.some((r) => /top/.test(r)), "the overriding rule was not reported");
            return bad.length + " headers off by " + Math.min(...bad.map((t) => t.offset)).toFixed(2) + " to " +
              Math.max(...bad.map((t) => t.offset)).toFixed(2) + "px; reported: " + broken.strays.join("; ");
          });
        }
        await page.close();
      }
    } else {
      // THE APP.
      // The app's surfaces open slowly, so here the smallest, the default and the largest size.
      for (const theme of themes) for (const scale of [80, 100, 150]) for (const s of APP_SURFACES) {
        const page = await openAppSurface(browser, s, theme, scale);
        const m = await measureAll(page);
        await run.check(s.name + ", " + theme + " " + scale + "%: its header uses the component and is centered", async () => {
          const outside = m.titles.filter((t) => t.kind !== "component");
          const mine = m.titles.filter((t) => t.kind === "component");
          assert(m.titles.length, "no header with an app icon was found on this surface");
          const msgs = [];
          if (outside.length) msgs.push("not the .app-title component: " + outside.map(describe).join("; "));
          const bad = mine.filter((t) => Math.abs(t.offset) > TOLERANCE_PX);
          if (bad.length) msgs.push(bad.map(describe).join("; "));
          if (m.strays.length) msgs.push("rules outside the component set its geometry: " + m.strays.join("; "));
          assert(!msgs.length, msgs.join("\n"));
          return mine.map(describe).join("; ");
        });
        await page.close();
      }
    }
  } finally {
    await browser.close();
  }
  process.exitCode = run.report() > 0 ? 1 : 0;
}

main().catch((e) => { console.error(e); process.exitCode = 1; });

// THE SPECKLED GROUND IN THE DESKTOP APP — the CEO's words (2026-09-29) are the acceptance
// criterion, verbatim:
//
//   "add this speckled background design for dark theme [round-15/v6] and this for light theme
//    [round-15/v6-light-2] to our DESIGN SYSTEM so that those background designs can always be
//    easily and reliably integrated in our desktop and mobile apps. And add the following rule:
//    The splash screens on desktop and mobile apps as well as the desktop app home screen should
//    never have that speckled background design."
//   "After the design system addition lands, get it integrated in desktop and mobile apps."
//
// So this suite asks four things of the shipped shell, in the engine Tauri ships on macOS:
//
//   1. IS IT ON SCREEN, in both themes? Not "is there a canvas": a screenshot of the stage and of
//      the rail is taken with the field and again after `RichSpeckle.destroy()`, and the pixels
//      have to differ. That proves the canvas painted AND that every plane between it and the eye
//      lets it through — the half a DOM assertion cannot see.
//   2. IS IT THE DESIGN SYSTEM'S, unedited? The engine and its CSS are compared by sha256 with the
//      files landed in richos-hq design/system/ at 8b85de45, and the mount is the `desktop` preset.
//   3. IS THE RULE HELD? Never on the opening screen, never on the home screen — while each is up
//      the canvas is not composited at all, and it comes back when the shell does (the way in, the
//      way out, and back). And the engine refuses the two surfaces by name.
//   4. DOES EVERY LINE OF TEXT ON IT STILL READ? Every text node the shell draws on a ground plane
//      is composited over the STRONGEST point the mounted engine can paint, per tint, and has to
//      keep 4.5:1 (3:1 for large text). The caps are read from the live engine, not typed here.
//
// Run: node speckled-ground.js    (RICHOS_PLAYWRIGHT=/path/to/node_modules/playwright node speckled-ground.js)

"use strict";

const crypto = require("crypto");
const fs = require("fs");
const path = require("path");
const {
  loadPlaywright,
  leaveHome,
  leaveSplash,
  bootSettled,
  flushFrames,
  openThread,
  SEED_THEME,
  HOLD_CURTAIN,
  assertCurtainHeld,
  createRun,
  assert,
  assertEqual,
  UI_DIR,
} = require("./lib/harness");
const PNG = require("./lib/png");

const APP = "file://" + path.join(UI_DIR, "index.html");
// A Retina Mac window at the size the design system's page was verified at (NOTES.md).
const VIEWPORT = { width: 1440, height: 900 };
const DPR = 2;

// THE DESIGN SYSTEM'S FILES, BY HASH. richos-hq `design/system/speckle.js` and `speckle.css` at
// 8b85de45 (Iris, "the round-15 speckled backgrounds in the design system"). A later design-system
// revision is taken by copying both files and moving these two lines, and nothing else.
const DESIGN_SYSTEM = {
  "speckle.js": "744e51067c12208c6aeca9f69f8609727113ea8cc9d2c495c6e22b0ddcb53f8b",
  "speckle.css": "42383e4ca72d5b600fbfadfc63e1583b6fe69659db176646c3dbdca20027a053",
};

async function openPage(browser, theme, opts) {
  opts = opts || {};
  const page = await browser.newPage({ viewport: VIEWPORT, deviceScaleFactor: DPR });
  const errors = [];
  page.on("pageerror", (e) => errors.push(String(e)));
  page.on("console", (m) => {
    if (m.type() === "error") errors.push("console: " + m.text());
  });
  page.__errors = errors;
  await page.addInitScript(SEED_THEME, theme);
  if (opts.holdCurtain) await page.addInitScript(HOLD_CURTAIN);
  await page.goto(APP);
  return page;
}

/// The field is built on an idle moment after `load`. Wait for THAT, by the app's own state,
/// and for the engine to have solved for the theme the page is in.
async function speckleMounted(page, theme) {
  await page.waitForFunction(
    (t) => {
      const s = window.RichSpeckle;
      if (!s || !s.state || !(s.state.handle || s.state.error)) return false;
      if (s.state.error) return true;
      const r = s.report();
      return !!r && r.theme === t;
    },
    theme,
    { timeout: 60000 }
  );
  const err = await page.evaluate(() => (window.RichSpeckle && window.RichSpeckle.state.error) || null);
  assertEqual(err, null, "the speckle mount threw");
  // `report()` is written inside the engine's build(), which draws synchronously before it
  // returns; two animation frames after that, the drawn canvas has been composited.
  await flushFrames(page);
}

async function shotOf(page, clip) {
  return PNG.decode(await page.screenshot({ clip }));
}

/// The fraction of device pixels that differ between two decodes of the same clip.
function changedFraction(a, b) {
  assertEqual([a.width, a.height, a.channels], [b.width, b.height, b.channels], "the two shots differ in shape");
  const n = a.width * a.height;
  let changed = 0;
  for (let i = 0; i < n; i++) {
    const o = i * a.channels;
    if (a.data[o] !== b.data[o] || a.data[o + 1] !== b.data[o + 1] || a.data[o + 2] !== b.data[o + 2]) changed++;
  }
  return { changed, total: n, fraction: changed / n };
}

/// A rectangle of the stage and one of the rail that are GROUND: every sampled point's topmost
/// element is one of the ground planes, so the pixels there are the ground (and, with the field,
/// the sheen) and never text or a surface. Found on the rendered page rather than typed.
async function groundClips(page) {
  return page.evaluate(() => {
    const GROUND = new Set(["stage", "conversation", "messages", "app", "rail", "rail-nav", "stage-header", "composer-zone"]);
    const isGround = (x, y) => {
      const top = document.elementFromPoint(x, y);
      if (!top) return false;
      if (GROUND.has(top.id)) return true;
      // Round 32 fills the rail with company headings and chats. Those rows and buttons paint no
      // fill of their own, so the field shows through them wherever no glyph or icon is drawn:
      // a bare-ground point is a rail point whose topmost element is a plain, unfilled, borderless
      // box with no text of its own. Spans, icons, <kbd> and anything with a fill do not count.
      if (!top.closest("#rail") || !/^(BUTTON|DIV|SECTION|NAV)$/.test(top.tagName)) return false;
      if (Array.prototype.some.call(top.childNodes, (n) => n.nodeType === 3 && n.textContent.trim() !== "")) return false;
      const cs = getComputedStyle(top);
      return cs.backgroundColor === "rgba(0, 0, 0, 0)" && cs.backgroundImage === "none" &&
        cs.boxShadow === "none" && ["Top", "Right", "Bottom", "Left"].every((d) => parseFloat(cs["border" + d + "Width"]) === 0);
    };
    function find(x0, x1, y0, y1, w, h) {
      for (let y = y0; y + h <= y1; y += 8) {
        for (let x = x0; x + w <= x1; x += 8) {
          let ok = true;
          for (let yy = y; yy <= y + h && ok; yy += 6) for (let xx = x; xx <= x + w && ok; xx += 6) ok = isGround(xx, yy);
          if (ok) return { x, y, width: w, height: h };
        }
      }
      return null;
    }
    const stage = document.getElementById("stage").getBoundingClientRect();
    const rail = document.getElementById("rail").getBoundingClientRect();
    return {
      stage: find(Math.round(stage.left) + 4, Math.round(stage.right) - 4, Math.round(stage.top) + 60, Math.round(stage.top) + 420, 120, 60),
      rail: find(Math.round(rail.left) + 2, Math.round(rail.right) - 2, Math.round(rail.top) + 40, Math.round(rail.bottom) - 40, 40, 40),
    };
  });
}

/// Every text node the shell draws on a GROUND plane, with the layers between it and the ruled
/// ground, composited over the strongest point of every tint the MOUNTED engine can paint.
async function textOverTheField(page) {
  return page.evaluate(() => {
    const S = window.RichOSSpeckle;
    const r = window.RichSpeckle.report();
    const theme = r.theme;
    const cfg = S.preset("desktop", theme);
    const ground = cfg.ground || cfg.grounds[0];
    const parse = (s) => {
      const m = String(s).match(/rgba?\(([^)]+)\)/);
      if (!m) return null;
      const p = m[1].split(/[\s,/]+/).filter(Boolean).map(Number);
      return { rgb: [p[0], p[1], p[2]], a: p.length > 3 ? p[3] : 1 };
    };
    const opacityOf = (el) => {
      let o = 1;
      for (let n = el; n && n.nodeType === 1; n = n.parentElement) o *= Number(getComputedStyle(n).opacity);
      return o;
    };
    const out = { theme, caps: r.caps, tints: r.tints, walked: 0, onGround: 0, onSurface: 0, worst: null, failures: [], unresolvable: [], colors: {} };
    const els = document.querySelectorAll("#app *");
    for (const el of els) {
      const hasText = Array.prototype.some.call(el.childNodes, (n) => n.nodeType === 3 && n.textContent.trim() !== "");
      if (!hasText) continue;
      const rect = el.getBoundingClientRect();
      if (rect.width < 1 || rect.height < 1 || rect.bottom < 0 || rect.top > innerHeight || rect.right < 0 || rect.left > innerWidth) continue;
      const cs = getComputedStyle(el);
      if (cs.visibility !== "visible" || el.closest("[hidden]")) continue;
      const op = opacityOf(el);
      if (op < 0.01) continue;
      out.walked++;
      // Up the chain to the first opaque plane. `html` is where the ruled ground is.
      const layers = [];
      let surface = null;
      let odd = null;
      for (let n = el; n && n.nodeType === 1 && n !== document.documentElement; n = n.parentElement) {
        const s = getComputedStyle(n);
        if (s.backgroundImage && s.backgroundImage !== "none") { odd = "background-image on #" + (n.id || n.className); break; }
        const c = parse(s.backgroundColor);
        if (!c) { odd = "unparseable background on " + (n.id || n.className); break; }
        const a = c.a * opacityOf(n);
        if (a >= 0.999) { surface = n; break; }
        if (a > 0.001) layers.unshift({ rgb: c.rgb, a });
      }
      if (odd) { out.unresolvable.push(odd); continue; }
      if (surface) { out.onSurface++; continue; }
      out.onGround++;
      const tc = parse(cs.color);
      const textA = tc.a * op;
      const size = parseFloat(cs.fontSize), bold = Number(cs.fontWeight) >= 700;
      const large = size >= 24 || (bold && size >= 18.66);
      const floor = large ? 3 : 4.5;
      const key = tc.rgb.join(",") + "@" + textA.toFixed(3);
      out.colors[key] = (out.colors[key] || 0) + 1;
      // No point (the unlit ground) and each tint at its cap.
      const bases = [{ name: "unlit ground", bg: ground }].concat(r.tints.map((name, i) => ({ name, bg: S.over(cfg.tints[i].rgb, r.caps[i], ground) })));
      for (const b of bases) {
        let bg = b.bg;
        for (const l of layers) bg = S.over(l.rgb, l.a, bg);
        const fg = S.over(tc.rgb, textA, bg);
        const ratio = S.ratio(fg, bg);
        const tag = (el.id ? "#" + el.id : el.tagName.toLowerCase() + (el.className && typeof el.className === "string" ? "." + el.className.split(/\s+/)[0] : "")) +
          " '" + el.textContent.trim().slice(0, 32) + "' rgb(" + tc.rgb.join(",") + ")@" + textA.toFixed(2) + " on " + b.name;
        if (!out.worst || ratio < out.worst.ratio) out.worst = { ratio, tag, floor };
        if (ratio < floor) out.failures.push(ratio.toFixed(2) + ":1 < " + floor + " — " + tag);
      }
    }
    return out;
  });
}

async function main() {
  const run = createRun("the speckled ground (design system, CEO 2026-09-29) in the desktop app");
  const { webkit } = loadPlaywright();
  const browser = await webkit.launch();
  const pages = [];

  // ---- 1. the files are the design system's ----------------------------------------------------

  await run.check("1  speckle.js and speckle.css are the design system's files, byte for byte", async () => {
    const got = {};
    for (const f of Object.keys(DESIGN_SYSTEM)) {
      const p = path.join(UI_DIR, f);
      assert(fs.existsSync(p), f + " is not in app/ui — the design system's engine is not integrated");
      got[f] = crypto.createHash("sha256").update(fs.readFileSync(p)).digest("hex");
    }
    assertEqual(got, DESIGN_SYSTEM, "the app's copy differs from richos-hq design/system at 8b85de45");
    const html = fs.readFileSync(path.join(UI_DIR, "index.html"), "utf8").replace(/<!--[\s\S]*?-->/g, "");
    for (const tag of ['href="speckle.css"', 'src="speckle.js"', 'src="speckle-app.js"']) {
      assert(html.includes(tag), "index.html does not load " + tag);
    }
    return "both files match 8b85de45 by sha256; index.html loads speckle.css, speckle.js, speckle-app.js";
  });

  // ---- 2-5. per theme: mounted, on screen, and every text node still reads -----------------------

  for (const theme of ["dark", "light"]) {
    const page = await openPage(browser, theme);
    pages.push(page);
    await leaveHome(page);
    await bootSettled(page);
    await page.mouse.move(2, VIEWPORT.height - 2);

    await run.check(`2  ${theme}: the desktop preset is mounted behind the shell, and the shell lets it through`, async () => {
      await speckleMounted(page, theme);
      const s = await page.evaluate(() => {
        const cs = (sel) => getComputedStyle(document.querySelector(sel));
        const c = window.RichSpeckle.canvas();
        return {
          surface: c && window.RichSpeckle.state.handle.surface,
          count: document.querySelectorAll("canvas.speckle").length,
          parentIsBody: !!c && c.parentElement === document.body,
          ariaHidden: c && c.getAttribute("aria-hidden"),
          pointer: c && getComputedStyle(c).pointerEvents,
          z: c && getComputedStyle(c).zIndex,
          visibility: c && getComputedStyle(c).visibility,
          size: c && [c.width, c.height],
          report: window.RichSpeckle.report(),
          body: cs("body").backgroundColor,
          stage: cs("#stage").backgroundColor,
          header: cs("#stage-header").backgroundColor,
          composer: cs("#composer-zone").backgroundColor,
          rail: cs("#rail").backgroundColor,
          html: cs("html").backgroundColor,
        };
      });
      assertEqual(s.surface, "desktop", "the mounted surface");
      assertEqual(s.count, 1, "canvas.speckle elements");
      assert(s.parentIsBody, "the canvas is not a child of <body>");
      assertEqual([s.ariaHidden, s.pointer, s.z, s.visibility], ["true", "none", "-1", "visible"], "aria-hidden, pointer-events, z-index, visibility");
      assertEqual(s.size, [VIEWPORT.width * DPR, VIEWPORT.height * DPR], "canvas backing size (device pixels)");
      assertEqual(s.report.theme, theme, "the engine solved for");
      assert(s.report.caps.every((c) => c > 0), "a tint's cap solved to zero, so that tint paints nothing: " + JSON.stringify(s.report.caps));
      const clear = (v) => v === "rgba(0, 0, 0, 0)" || v === "transparent";
      assert(clear(s.body) && clear(s.stage) && clear(s.header) && clear(s.composer), "a ground plane is still opaque: " + JSON.stringify(s));
      if (theme === "dark") assertEqual(s.rail, "rgba(0, 0, 0, 0.32)", "the dark rail is the design system's translucent rail");
      else assert(clear(s.rail), "the light rail is the ruled paper, so it is transparent over the field: " + s.rail);
      return `surface desktop, caps ${s.report.tints.map((n, i) => n + " " + s.report.caps[i]).join(" · ")}, ` +
        `worst listed pair ${s.report.worstRatio.toFixed(2)}:1 (${s.report.worstPair.name}); body/stage/header/composer transparent, rail ${s.rail}`;
    });

    await run.check(`3  ${theme}: the sheen is ON SCREEN on the stage and the rail — pixels change when it is taken away`, async () => {
      const clips = await groundClips(page);
      assert(clips.stage, "no 120x60 rectangle of bare stage ground was found to photograph");
      assert(clips.rail, "no 40x40 rectangle of bare rail ground was found to photograph");
      const withStage = await shotOf(page, clips.stage);
      const withRail = await shotOf(page, clips.rail);
      await page.evaluate(() => window.RichSpeckle.destroy());
      await page.waitForFunction(() => !document.querySelector("canvas.speckle"), null, { timeout: 60000 });
      await flushFrames(page);
      const gone = await page.evaluate(() => document.querySelectorAll("canvas.speckle").length);
      const plainStage = await shotOf(page, clips.stage);
      const plainRail = await shotOf(page, clips.rail);
      // Back, for the checks after this one — the way out has a way back in.
      await page.evaluate(() => window.RichSpeckle.mount());
      await speckleMounted(page, theme);
      const st = changedFraction(withStage, plainStage);
      const rl = changedFraction(withRail, plainRail);
      assertEqual(gone, 0, "destroy() left a canvas behind");
      assert(st.fraction >= 0.01, `only ${(st.fraction * 100).toFixed(2)}% of the stage clip's pixels change with the field — it is not visible there`);
      assert(rl.fraction >= 0.002, `only ${(rl.fraction * 100).toFixed(3)}% of the rail clip's pixels change with the field — it is not visible there`);
      return `stage clip ${JSON.stringify(clips.stage)}: ${st.changed}/${st.total} device px (${(st.fraction * 100).toFixed(1)}%) differ; ` +
        `rail clip ${JSON.stringify(clips.rail)}: ${rl.changed}/${rl.total} (${(rl.fraction * 100).toFixed(1)}%); destroy() removed the canvas and mount() restored it`;
    });

    await run.check(`4  ${theme}: every text node on the ground keeps 4.5:1 (3:1 large) over the strongest point the engine can paint`, async () => {
      // Two screens of the shell: the thread it boots on, and a working thread with turns in it
      // (the timeline's rows, statuses and muted inks are what the boot greeting does not draw).
      const boot = await textOverTheField(page);
      await openThread(page, "acme");
      const acme = await textOverTheField(page);
      const all = [boot, acme];
      const onGround = boot.onGround + acme.onGround;
      const colors = new Set(Object.keys(boot.colors).concat(Object.keys(acme.colors)));
      const worst = boot.worst.ratio <= acme.worst.ratio ? boot.worst : acme.worst;
      assert(acme.onGround > boot.onGround, `the working thread put no more text on the ground (${acme.onGround}) than the boot greeting (${boot.onGround}) — the walk is not on it`);
      assertEqual(all.map((t) => t.unresolvable).flat(), [], "text whose background chain this walk cannot composite");
      assertEqual(all.map((t) => t.failures).flat(), [], "text under the floor over the field");
      return `boot thread ${boot.onGround} + working thread ${acme.onGround} text nodes on the ground ` +
        `(${boot.onSurface + acme.onSurface} on surfaces, not the field's business); ${colors.size} distinct text colors on the ground; ` +
        `worst ${worst.ratio.toFixed(2)}:1 (floor ${worst.floor}) — ${worst.tag}`;
    });

    await run.check(`5  ${theme}: the field is built after load, off the launch path, and its build time is measured`, async () => {
      const m = await page.evaluate(() => {
        const nav = performance.getEntriesByType("navigation")[0];
        return { loadEnd: nav ? nav.loadEventEnd : null, mountedAt: window.RichSpeckle.state.mountedAt, buildMs: window.RichSpeckle.state.buildMs };
      });
      assert(m.loadEnd > 0, "no navigation timing");
      assert(m.mountedAt >= m.loadEnd, `the field was built at ${m.mountedAt.toFixed(0)}ms, before load ended at ${m.loadEnd.toFixed(0)}ms`);
      return `load ended ${m.loadEnd.toFixed(0)}ms; last build at ${m.mountedAt.toFixed(0)}ms took ${m.buildMs.toFixed(1)}ms (measured, not gated)`;
    });
  }

  // ---- 6. the theme is followed live, both ways ---------------------------------------------------

  await run.check("6  switching the theme re-solves the field for the new theme, and back", async () => {
    const page = pages[0];
    const seen = [];
    for (const t of ["light", "dark"]) {
      await page.evaluate((x) => document.documentElement.setAttribute("data-theme", x), t);
      await speckleMounted(page, t);
      seen.push(await page.evaluate(() => { const r = window.RichSpeckle.report(); return r.theme + " " + r.tints.join("/"); }));
    }
    assertEqual(seen, ["light gold/rose/aqua/lit side", "dark cool/gold"], "the engine's theme after each switch");
    return seen.join(" → ");
  });

  // ---- 7-8. THE RULE: never on the home screen, never on the splash ------------------------------

  await run.check("7  the HOME screen never has it: not composited while home is up, back when he leaves, gone again when he returns", async () => {
    const page = await openPage(browser, "dark");
    pages.push(page);
    await leaveSplash(page);
    await page.waitForFunction(() => window.RichHome && window.RichHome.isOpen(), null, { timeout: 60000 });
    await page.evaluate(() => window.RichSpeckle.mount()); // do not wait for idle: the rule must hold with the field BUILT
    const state = () => page.evaluate(() => {
      const c = window.RichSpeckle.canvas();
      const home = document.getElementById("home");
      return {
        homeOpen: !!(window.RichHome && window.RichHome.isOpen()),
        visibility: c ? getComputedStyle(c).visibility : "(no canvas)",
        homeBg: home && !home.hidden ? getComputedStyle(home).backgroundColor : null,
      };
    });
    const onHome = await state();
    assert(onHome.homeOpen, "the home screen is not up");
    assertEqual(onHome.visibility, "hidden", "the speckle canvas while the home screen is up");
    assertEqual(onHome.homeBg, "rgb(12, 19, 34)", "the home screen's own opaque ground");
    await leaveHome(page);
    const inShell = await state();
    assertEqual(inShell.visibility, "visible", "the speckle canvas once he has left the home screen");
    await page.evaluate(() => window.RichHome.show("acceptance-suite"));
    await page.waitForFunction(() => window.RichHome.isOpen(), null, { timeout: 60000 });
    const back = await state();
    assertEqual(back.visibility, "hidden", "the speckle canvas after he went back to the home screen");
    // And the engine itself refuses the surface.
    const refused = await page.evaluate(() => {
      const before = document.querySelectorAll("canvas").length;
      const out = {};
      for (const s of ["home", "splash"]) {
        try { window.RichOSSpeckle.background({ surface: s }); out[s] = "MOUNTED"; } catch (e) { out[s] = String(e.message); }
      }
      out.added = document.querySelectorAll("canvas").length - before;
      return out;
    });
    assert(/desktop app home screen/.test(refused.home) && refused.home.includes("should never have that speckled background design"), "background({surface:'home'}) did not refuse with the rule: " + refused.home);
    assert(/splash screen/.test(refused.splash), "background({surface:'splash'}) did not refuse: " + refused.splash);
    assertEqual(refused.added, 0, "canvases added by the refused calls");
    return "home up: hidden · shell: visible · home again: hidden; background() refuses 'home' and 'splash' with his sentence";
  });

  await run.check("8  the SPLASH screen never has it: not composited while the curtain is up, even with the field built", async () => {
    const page = await openPage(browser, "dark", { holdCurtain: true });
    pages.push(page);
    await page.waitForFunction(() => window.RichSpeckle && window.RichSplash && window.RichSplash.state && window.RichSplash.state.shown, null, { timeout: 60000 });
    await page.evaluate(() => window.RichSpeckle.mount());
    const held = await assertCurtainHeld(page);
    const s = await page.evaluate(() => {
      const c = window.RichSpeckle.canvas();
      return { splash: !!document.getElementById("splash"), visibility: c ? getComputedStyle(c).visibility : "(no canvas)" };
    });
    assert(s.splash, "#splash is not in the document");
    assertEqual(s.visibility, "hidden", "the speckle canvas while the opening curtain is up");
    return "curtain up (" + held + "); canvas hidden";
  });

  await run.check("9  no page errors on any page this suite opened", async () => {
    const errs = pages.map((p) => p.__errors).flat();
    assertEqual(errs, [], "page errors");
    return pages.length + " pages, 0 errors";
  });

  for (const p of pages) await p.close();
  await browser.close();
  // `report()` returns the FAILED COUNT, not a verdict. Same form as every other suite here.
  process.exitCode = run.report() > 0 ? 1 : 0;
}

main().catch((e) => {
  console.error(e);
  process.exit(1);
});

// THE HOME SCREEN OF A USER WHO HAS A TEAM SHOWS HIS TEAM AND NOTHING MADE UP.
//
// Product hunt 2026-10-15 (richos-hq `docs/audits/2026-10-15-product-hunt/ray-opus-hunt57.md`),
// finding 7: after a relaunch the home screen showed "18 AI specialists, 6 working now",
// "1,596 tasks handled without you", "2,365 h of your attention saved", "7,508 memories" and
// six named specialists "working now" to a user who had just hired his first two teammates,
// and showed neither of them. The acceptance sentence: it shows only this user's real team and
// real numbers, and nothing made up.
//
// The demonstration stays for a user with no team of his own (CEO 2026-09-06: "The demo is
// definitely needed, initially, for the user"), so the last checks hold that side too.
//
// The home screen is clamped dark in both themes (§15), so the contrast check runs with the
// operating system seeded dark and seeded light and measures the pixels behind the text.
//
// Run: node home-team.js   (or `npm test` for every suite in this directory)

"use strict";

const path = require("path");
const { loadPlaywright, createRun, assert, assertEqual, SEED_THEME, UI_DIR } = require("./lib/harness");
const contrastLib = require("./lib/contrast");

const APP = "file://" + path.join(UI_DIR, "index.html");

/// The team `home_team` answers with: what Dean's template writes as `# {Name} — {Role Title}`.
const TEAM = [
  { name: "Beth", role: "Bookkeeper" },
  { name: "Will", role: "Marketing Director" },
];

/// Every number and name the demonstration draws, read from what the hunt saw on screen
/// (`hire/06-home-after-relaunch.png`). None of them may be on the screen of a user with a team.
const MADE_UP = ["1,596", "2,365", "7,508", "4,802", "430", "185 lessons", "14 months", "working now",
  "Cadence", "Fathom", "Gantry", "Kiln", "Nocturne", "Quarry"];

async function openApp(browser, theme, preset) {
  const page = await browser.newPage({ viewport: { width: 1440, height: 900 } });
  const errors = [];
  page.on("pageerror", (e) => errors.push(String(e)));
  page.on("console", (m) => {
    if (m.type() === "error") errors.push("console: " + m.text());
  });
  page.__errors = errors;
  await page.addInitScript(SEED_THEME, theme);
  await page.addInitScript((v) => {
    window.__RICHOS_MOCK_PRESET__ = v;
  }, preset);
  await page.goto(APP);
  await page.waitForFunction("typeof window.RichHome === 'object' && typeof window.RichTimeline === 'object'", null, { timeout: 60000 });
  await page.evaluate(() => window.RichSplash && window.RichSplash.yieldNow("acceptance-suite"));
  await page.evaluate(() => window.RichHome.startField());
  return page;
}

/// What the home screen says, as a person reads it, lowercased: the labels are set in capitals
/// by CSS, and a made-up number or name is made up in either case.
const screenText = (page) => page.evaluate(() => document.getElementById("home").innerText.replace(/\s+/g, " ").toLowerCase());

/// The left column's lines as "number label", read from the DOM's own text (not the capitals
/// the CSS draws).
const signalsOf = (page) =>
  page.evaluate(() =>
    Array.from(document.querySelectorAll("#home-signals .sig"))
      .map((s) => s.querySelector(".n").textContent.trim() + " " + s.querySelector(".l").textContent.trim())
      .join(" / ")
  );

/// The worst contrast between each element's text and the pixels actually behind its glyphs:
/// every glyph is made transparent (layout unchanged), the frame is captured, and each text
/// line box is sampled for the pixel that contrasts least with the element's own ink.
async function worstContrast(page, selectors) {
  await page.addScriptTag({ content: contrastLib.pageScript() });
  const targets = await page.evaluate((sels) => {
    const out = [];
    for (const sel of sels) {
      for (const el of document.querySelectorAll(sel)) {
        const rects = [];
        const walker = document.createTreeWalker(el, NodeFilter.SHOW_TEXT);
        for (let n = walker.nextNode(); n; n = walker.nextNode()) {
          if (!n.textContent.trim() || n.parentElement.closest("[hidden]")) continue;
          const r = document.createRange();
          r.selectNodeContents(n);
          const cs = getComputedStyle(n.parentElement);
          for (const b of r.getClientRects()) {
            if (b.width > 0.5 && b.height > 0.5)
              rects.push({ x: b.left, y: b.top, w: b.width, h: b.height, ink: cs.color, px: parseFloat(cs.fontSize), weight: cs.fontWeight, text: n.textContent.trim() });
          }
        }
        out.push({ sel, rects });
      }
    }
    const style = document.createElement("style");
    style.id = "__blank";
    style.textContent = "#home *{transition:none!important;animation:none!important;color:transparent!important}";
    document.head.appendChild(style);
    return out;
  }, selectors);
  await page.evaluate(() => new Promise((r) => requestAnimationFrame(() => requestAnimationFrame(r))));
  const buf = await page.screenshot({ fullPage: false });
  await page.evaluate(() => document.getElementById("__blank").remove());
  return page.evaluate(
    async ({ b64, targets }) => {
      const M = window.__contrastMath;
      const img = new Image();
      await new Promise((res, rej) => {
        img.onload = res;
        img.onerror = rej;
        img.src = "data:image/png;base64," + b64;
      });
      const c = document.createElement("canvas");
      c.width = img.width;
      c.height = img.height;
      const g = c.getContext("2d");
      g.drawImage(img, 0, 0);
      const scale = img.width / innerWidth;
      const rows = [];
      for (const t of targets) {
        for (const r of t.rects) {
          const ink = M.parseCssColor(r.ink);
          const data = g.getImageData(Math.floor(r.x * scale), Math.floor(r.y * scale), Math.max(1, Math.ceil(r.w * scale)), Math.max(1, Math.ceil(r.h * scale))).data;
          let worst = Infinity;
          for (let i = 0; i < data.length; i += 4) {
            const bg = { r: data[i], g: data[i + 1], b: data[i + 2], a: 1 };
            const fg = ink.a < 1 ? M.compositeOver(ink, bg) : ink;
            worst = Math.min(worst, M.contrastRatio(fg, bg));
          }
          rows.push({ sel: t.sel, text: r.text, ratio: M.round2(worst), needs: M.isLargeText(r.px, r.weight) ? 3 : 4.5 });
        }
      }
      return rows;
    },
    { b64: buf.toString("base64"), targets }
  );
}

async function main() {
  const run = createRun("the home screen of a user with a team: his team, his numbers, nothing made up");
  const { webkit } = loadPlaywright();
  const browser = await webkit.launch();

  for (const theme of ["dark", "light"]) {
    const page = await openApp(browser, theme, { homeTeam: TEAM });
    await page.waitForFunction("window.RichHome.state.field === 'none'", null, { timeout: 30000 }).catch(() => {});

    await run.check(`${theme}: his team is on the home screen, by name and role`, async () => {
      const seen = await page.evaluate(() => ({
        field: window.RichHome.state.field,
        cap: document.querySelector("#home-live .cap").textContent.trim(),
        rows: Array.from(document.querySelectorAll("#home-working li")).map((li) => li.textContent.replace(/\s+/g, " ").trim()),
      }));
      seen.signals = await signalsOf(page);
      assertEqual(seen.field, "none", "a user with a team and no picture of his own must get no picture");
      assertEqual(seen.rows, TEAM.map((t) => t.name + " " + t.role), "the list is not his team");
      assertEqual(seen.cap, "Your team", "the list's heading claims something about them");
      assertEqual(seen.signals, "2 AI specialists", "the numbers are not his");
      return `"${seen.cap}": ${seen.rows.join(", ")}; "${seen.signals}"`;
    });

    await run.check(`${theme}: nothing made up is on the screen`, async () => {
      const text = await screenText(page);
      const found = MADE_UP.filter((w) => text.includes(w.toLowerCase()));
      assertEqual(found, [], "the demonstration's numbers or names are still on his screen");
      const loaded = await page.evaluate(() => ({
        dataset: !!window.MATURE_LORO,
        scripts: Array.from(document.querySelectorAll("script[src]")).map((s) => s.getAttribute("src")).filter((s) => s.indexOf("home/field-") === 0),
        note: !document.getElementById("home-note") || document.getElementById("home-note").hidden,
      }));
      assertEqual(loaded.dataset, false, "a dataset was loaded for a screen that draws none");
      assertEqual(loaded.scripts, [], "the field's scripts were loaded");
      assert(loaded.note, "the banner calling the screen a demonstration is up over his own team");
      // The loading layer fades over 0.8 s (`home.css` `#home-loading.gone`) before it is hidden.
      const gone = await page
        .waitForFunction(() => getComputedStyle(document.getElementById("home-loading")).visibility === "hidden", null, { timeout: 60000 })
        .then(() => true, () => false);
      assert(gone, "\"waking loro…\" is left on a screen that draws no picture");
      return "no made-up number or name, no dataset, no field script, no banner, no loading line";
    });

    await run.check(`${theme}: his team and numbers clear WCAG AA against what is behind them`, async () => {
      const rows = await worstContrast(page, ["#home-signals", "#home-live .cap", "#home-working li", "#home-owner"]);
      assert(rows.length >= 5, "measured only " + rows.length + " line(s)");
      const bad = rows.filter((r) => !(r.ratio >= r.needs));
      assertEqual(bad, [], "text below its contrast floor");
      return rows.map((r) => `${JSON.stringify(r.text)} ${r.ratio}:1 (needs ${r.needs})`).join("; ");
    });

    await run.check(`${theme}: no page errors`, async () => {
      assertEqual(page.__errors, [], "the page reported errors");
      return "0 errors";
    });
    await page.close();
  }

  await run.check("his own loro, when it is drawn, does not put made-up specialists over his team", async () => {
    const page = await openApp(browser, "dark", { homeTeam: TEAM, homeField: { objects: 4000, records: 9000 } });
    await page.waitForFunction("window.RichHome.state.field === 'live' && !!window.__loro", null, { timeout: 90000 });
    const seen = await page.evaluate(() => ({
      synthetic: window.MATURE_LORO.meta.synthetic,
      rows: Array.from(document.querySelectorAll("#home-working li")).map((li) => li.textContent.replace(/\s+/g, " ").trim()),
    }));
    seen.signals = await signalsOf(page);
    assertEqual(seen.synthetic, false, "the picture is not his loro");
    assertEqual(seen.rows, TEAM.map((t) => t.name + " " + t.role), "the picture's engine replaced his team");
    assertEqual(seen.signals, "2 AI specialists", "the picture's engine replaced his numbers");
    assertEqual(page.__errors, [], "the page reported errors");
    await page.close();
    return `his loro drawn; the list and the numbers stay his: ${seen.rows.join(", ")}; "${seen.signals}"`;
  });

  // THE HIRE MADE IN THIS SESSION (handover review rv-20261015T114726Z-23f8c364-1623): the
  // first-run demonstration is up, he leaves for the app, hires, and comes back home; then he
  // hires again and comes back again. Each time the screen shows the team as it is now, with no
  // reload. Before the fix `home_team` was read once per page, so he came back to the
  // demonstration after his first hire and to the old team after the next.
  await run.check("a hire made since the home screen was last shown is on it when he comes back, no reload", async () => {
    const page = await openApp(browser, "dark", {});
    await page.waitForFunction("window.RichHome.state.field === 'live' && !!window.__loro", null, { timeout: 90000 });
    const homeAfter = async (team) => {
      await page.evaluate((t) => {
        window.RichHome.hide("enter-app");
        window.__RICHOS_MOCK__.homeTeamSet(t);
        window.RichHome.show("wordmark");
      }, team);
      const want = team.map((t) => t.name + " " + t.role);
      await page
        .waitForFunction(
          (w) => JSON.stringify(Array.from(document.querySelectorAll("#home-working li")).map((li) => li.textContent.replace(/\s+/g, " ").trim())) === JSON.stringify(w),
          want,
          { timeout: 90000 }
        )
        .catch(() => {});
      const seen = await page.evaluate(() => ({
        rows: Array.from(document.querySelectorAll("#home-working li")).map((li) => li.textContent.replace(/\s+/g, " ").trim()),
        cap: document.querySelector("#home-live .cap").textContent.trim(),
        demo: !!(window.MATURE_LORO && window.MATURE_LORO.meta && window.MATURE_LORO.meta.synthetic),
        drawing: !!(window.__loro && window.__loro.running),
        stageShown: !document.getElementById("home-stage").hidden,
        note: !document.getElementById("home-note").hidden,
        field: window.RichHome.state.field,
      }));
      seen.signals = await signalsOf(page);
      const text = await screenText(page);
      seen.madeUp = MADE_UP.filter((w) => text.includes(w.toLowerCase()));
      assertEqual(seen.rows, want, "the home screen does not show the team as it is now");
      assertEqual(seen.cap, "Your team", "the list's heading claims something about them");
      assertEqual(seen.signals, team.length + (team.length === 1 ? " AI specialist" : " AI specialists"), "the numbers are not his");
      assertEqual(seen.demo, false, "the demonstration's dataset is still on the page");
      assertEqual(seen.drawing, false, "the demonstration is still being drawn");
      assertEqual(seen.stageShown, false, "the demonstration's picture is still on screen");
      assertEqual(seen.note, false, "the banner calling the screen a demonstration is up over his own team");
      assertEqual(seen.field, "none", "a user with a team and no picture of his own must get no picture");
      assertEqual(seen.madeUp, [], "the demonstration's numbers or names are still on his screen");
      return `${seen.rows.join(", ")}; "${seen.signals}"`;
    };
    const first = await homeAfter([{ name: "Will", role: "Marketing Director" }]);
    const next = await homeAfter([{ name: "Will", role: "Marketing Director" }, { name: "Beth", role: "Bookkeeper" }]);
    assertEqual(page.__errors, [], "the page reported errors");
    await page.close();
    return `after the first hire: ${first}; after the next: ${next}`;
  });

  await run.check("NEGATIVE CONTROL: a user with no team still gets the demonstration (CEO 2026-09-06)", async () => {
    const page = await openApp(browser, "dark", {});
    await page.waitForFunction("window.RichHome.state.field === 'live'", null, { timeout: 90000 });
    const seen = await page.evaluate(() => ({
      synthetic: window.MATURE_LORO && window.MATURE_LORO.meta.synthetic,
      note: !document.getElementById("home-note").hidden,
      rows: document.querySelectorAll("#home-working li").length,
    }));
    assertEqual(seen.synthetic, true, "the first-run demonstration is gone for a user with no team");
    assert(seen.note, "the demonstration is up without the banner that says it is one");
    assert(seen.rows > 0, "the demonstration's list is empty");
    await page.close();
    return `demonstration drawn, banner up, ${seen.rows} illustrative row(s)`;
  });

  await browser.close();
  process.exitCode = run.report() > 0 ? 1 : 0;
}

main().catch((e) => {
  console.error(e);
  process.exit(1);
});

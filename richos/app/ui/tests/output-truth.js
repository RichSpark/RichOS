// THE OUTPUT PANEL SAYS WHAT IS TRUE NOW — the four Output panel defects Ray's walk of nightly
// candidate 38 found (`docs/verification/2026-10-05-nightly-38-candidate-walk.md`, D6, D7, D9,
// D10), each driven through the REAL SHELL (`index.html`, `main.js`, `output-panel.js`,
// `style.css`, `mock.js`) under WebKit, with nothing stubbed but the Tauri bridge.
//
//   D6  a file deleted while the panel was closed still looked present when it was opened again:
//       its row lit, its actions enabled with no reason, and its own view saying "Written … ·
//       30 bytes" right above "This file is no longer where it was written." Opening the panel,
//       and showing a file, now reflect what is on disk at that moment (PRD §4.6, §6.7).
//   D7  "Under the hood" opened OVER an open Output panel instead of replacing it. One right-hand
//       pane at a time, both ways (PRD §6.1), with both Output buttons' pressed state truthful.
//   D9  the CSV view's facts line ended a line with a lone "·" when the next part wrapped.
//   D10 in dark, the panel's scroll bar was drawn light: the panel declared no `color-scheme`.
//
// Each check fails on `ed13a8c76` (the candidate's source) and passes with the fix.
//
// Run: node output-truth.js   (or `npm test` for every suite in this directory)

"use strict";

const path = require("path");
const { loadPlaywright, leaveHome, createRun, assert, assertEqual, FRAME_STILL, UI_DIR } = require("./lib/harness");

const APP = "file://" + path.join(UI_DIR, "index.html");
const FIXED = new Date("2026-10-05T09:45:00-04:00");
const MISSING =
  "This file is no longer where it was written. If it was moved, open it from its new place; if Rich writes it again, it will be listed here.";
const MISSING_LINE = "No longer where it was written";

async function openApp(browser, opts) {
  opts = opts || {};
  const context = await browser.newContext({
    viewport: opts.viewport || { width: 1440, height: 900 },
    timezoneId: "America/New_York",
    locale: "en-US",
  });
  const page = await context.newPage();
  await page.clock.setFixedTime(FIXED);
  const errors = [];
  page.on("pageerror", (e) => errors.push(String(e)));
  page.on("console", (m) => {
    if (m.type() === "error") errors.push("console: " + m.text());
  });
  const theme = opts.theme || "dark";
  await page.addInitScript((t) => {
    try {
      window.localStorage.setItem("richos-theme", t);
      window.localStorage.setItem("richos-font-scale", "100");
      window.localStorage.setItem("richos-mock-config", JSON.stringify({ theme: t, font_scale: 100, user_name: null }));
    } catch (_e) {
      /* storage unavailable: the shipped default */
    }
  }, theme);
  await page.addInitScript(FRAME_STILL);
  await page.addInitScript((v) => {
    window.__RICHOS_MOCK_PRESET__ = v;
  }, {
    output: "round-17",
    // A saved work record on `acme`, so the conversation carries the chip that opens Under the hood.
    workSummaries: {
      acme: {
        items: [{ title: "Comparables pass", role: "worker", repository: "/fictional/acme", state: "integrated", detail: "Reviewed commit integrated locally." }],
        omitted: 0,
      },
    },
  });
  await page.goto(APP);
  await leaveHome(page);
  await page.waitForFunction(() => typeof window.RichOutput === "object" && typeof window.RichTimeline === "object");
  await page.waitForSelector(".nav-thread", { state: "attached" });
  await page.click('.nav-thread[data-thread-id="acme"]');
  await page.waitForFunction(() => {
    const s = window.RichOutput.snapshot();
    return s.thread === "acme" && s.count === 9;
  });
  await page.waitForFunction((t) => document.documentElement.dataset.theme === t, theme);
  page.__errors = errors;
  // The record's ids by file name: a row is found by the id the panel stamps on it.
  page.__ids = await page.evaluate(() =>
    Object.fromEntries(window.__RICHOS_MOCK__.outputList("acme").files.map((f) => [f.name, f.id]))
  );
  return page;
}

const rowOf = (p, name) => ".orow[data-output=\"" + p.__ids[name] + "\"]";

/// Wait for `fn` to hold in the page. The harness's own deadline only catches a hang: on the
/// defective build the stale state never changes, so it fails there with what the page showed.
async function within(page, fn, arg, what, read) {
  try {
    await page.waitForFunction(fn, arg);
  } catch (_e) {
    const seen = read ? await page.evaluate(read, arg) : null;
    assert(false, what + (seen != null ? " — the page shows " + JSON.stringify(seen) : ""));
  }
}

/// What a row tells about one file: dimmed or not, its line, and its Open control.
function rowTruth(name) {
  const r = [...document.querySelectorAll(".orow")].find((n) => n.querySelector(".oname").textContent === name);
  if (!r) return null;
  const open = r.parentElement.querySelector('[data-act="open"]');
  return {
    missing: r.classList.contains("is-missing"),
    line: r.querySelector(".opath").textContent,
    open: open ? { disabled: open.getAttribute("aria-disabled"), title: open.title } : null,
    head: document.getElementById("op-title").textContent,
  };
}

/// What the file view tells about the file it shows.
function fileTruth() {
  const main = document.querySelector('.of-open [data-act="open"]');
  const down = document.querySelector('[data-act="open-menu"]');
  return {
    title: document.getElementById("op-title").textContent,
    sub: document.getElementById("op-sub").textContent,
    open: main ? { disabled: main.getAttribute("aria-disabled"), title: main.title } : null,
    down: down ? { disabled: down.getAttribute("aria-disabled"), title: down.title } : null,
    missingSentence: !!document.querySelector(".of-missing"),
  };
}

async function main() {
  const run = createRun("Output panel — what it shows is true now (walk 38: D6, D7, D9, D10; real shell, WebKit)");
  const { webkit } = loadPlaywright();
  const browser = await webkit.launch();
  const page = await openApp(browser);

  await run.check("D6 — a file deleted while the panel was closed is marked missing the moment the panel opens again", async () => {
    await page.click("#out-top");
    await page.waitForSelector(rowOf(page, "q3-revenue.csv"));
    // The row's Open learns its app on hover, as his pointer does — the cache the reopen must drop.
    await page.hover(rowOf(page, "q3-revenue.csv"));
    await within(page, () => {
      const b = [...document.querySelectorAll(".orow")].find((n) => n.querySelector(".oname").textContent === "q3-revenue.csv");
      return b && /^Open in /.test(b.parentElement.querySelector('[data-act="open"]').title);
    }, null, "the row never learned its app");
    const before = await page.evaluate(rowTruth, "q3-revenue.csv");
    assertEqual(before.missing, false, "the file was present before it was deleted");
    await page.click("#op-close");
    await page.waitForFunction(() => document.getElementById("outpanel").hidden);
    // `rm -f` on disk, as Ray's walk did in the guest: nothing is announced to the app.
    await page.evaluate(() => window.__RICHOS_MOCK__.outputMissing("acme", "q3-revenue.csv"));
    await page.click("#out-top");
    await within(
      page,
      (n) => {
        const r = [...document.querySelectorAll(".orow")].find((x) => x.querySelector(".oname").textContent === n);
        return r && r.classList.contains("is-missing");
      },
      "q3-revenue.csv",
      "reopened, the deleted file's row still looks present",
      rowTruth
    );
    const after = await page.evaluate(rowTruth, "q3-revenue.csv");
    assertEqual(after.line, MISSING_LINE);
    assertEqual(after.open, { disabled: "true", title: MISSING }, "the row's Open is lit, or has no reason");
    assertEqual(after.head, "9 files from this thread · 1 no longer where it was written");
    // Its menu: every action that needs the file off, each with the §6.7 sentence.
    await page.hover(rowOf(page, "q3-revenue.csv"));
    await page.click(rowOf(page, "q3-revenue.csv") + ' + .oacts [data-act="menu"]');
    await page.waitForSelector("#op-menu");
    const menu = await page.evaluate(() =>
      [...document.querySelectorAll("#op-menu .op-menu-item")].map((b) => ({
        item: b.textContent.trim(),
        off: b.getAttribute("aria-disabled") === "true",
        title: b.title,
      }))
    );
    for (const m of menu) {
      if (m.item === "Preview" || m.item === "Copy path") continue;
      assert(m.off && m.title === MISSING, "the menu's " + m.item + " is lit, or has no reason: " + JSON.stringify(m));
    }
    await page.keyboard.press("Escape");
    await page.click(rowOf(page, "q3-revenue.csv"));
    await page.waitForSelector(".of-missing");
    const view = await page.evaluate(fileTruth);
    assert(view.sub.startsWith(MISSING_LINE) && !/Written|bytes|KB/.test(view.sub), "the file view still says it was written, with a size: " + JSON.stringify(view.sub));
    assertEqual(view.open, { disabled: "true", title: MISSING });
    return "row dimmed, its Open and menu off with the §6.7 sentence; its view says '" + view.sub + "'";
  });

  await run.check("D6 — a link in the conversation opens a file that went while the panel was closed as missing, never as written", async () => {
    await page.keyboard.press("Escape"); // any menu the check above left
    if (!(await page.evaluate(() => document.getElementById("outpanel").hidden))) await page.click("#op-close");
    await page.waitForFunction(() => document.getElementById("outpanel").hidden);
    const found = await page.evaluate(() => {
      const names = new Set(window.__RICHOS_MOCK__.outputList("acme").files.filter((f) => f.exists).map((f) => f.name));
      const links = [...document.querySelectorAll(".tl-file-link")];
      const at = links.findIndex((n) => names.has(n.textContent.trim()));
      return at < 0 ? null : { at, name: links[at].textContent.trim() };
    });
    assert(found, "no conversation link names a present file");
    const link = found.name;
    await page.evaluate((n) => window.__RICHOS_MOCK__.outputMissing("acme", n), link);
    const a = page.locator(".tl-file-link").nth(found.at);
    await a.scrollIntoViewIfNeeded();
    await a.click();
    await within(
      page,
      (n) => document.getElementById("op-title").textContent === n && document.getElementById("op-sub").textContent.startsWith("No longer where it was written"),
      link,
      "opened from its link, the gone file's view still says it was written",
      fileTruth
    );
    const view = await page.evaluate(fileTruth);
    assertEqual(view.open, { disabled: "true", title: MISSING });
    assertEqual(view.missingSentence, true);
    return link + ": '" + view.sub + "', Open off with its reason";
  });

  await run.check("D6 — showing a file in an open panel reads it now: one gone since the list was read is said gone, and its row dims", async () => {
    // The panel open on its list, whatever the check above left.
    if (await page.evaluate(() => document.getElementById("outpanel").hidden)) await page.click("#out-top");
    if (await page.evaluate(() => document.getElementById("op-body").dataset.view === "file")) await page.click("#of-back");
    await page.waitForFunction(() => !document.getElementById("outpanel").hidden && document.getElementById("op-body").dataset.view === "list");
    const name = await page.evaluate(() => {
      const f = window.__RICHOS_MOCK__.outputList("acme").files.find((x) => x.exists && x.kind === "md");
      return f ? f.name : null;
    });
    assert(name, "no present Markdown file left in the fixture");
    await page.evaluate((n) => window.__RICHOS_MOCK__.outputMissing("acme", n), name);
    await page.click(rowOf(page, name));
    await within(
      page,
      (n) => document.getElementById("op-title").textContent === n && document.getElementById("op-sub").textContent.startsWith("No longer where it was written"),
      name,
      "the file view of a file gone since the list was read says it was written",
      fileTruth
    );
    const view = await page.evaluate(fileTruth);
    assertEqual(view.open, { disabled: "true", title: MISSING });
    await page.keyboard.press("Escape");
    await page.waitForFunction(() => document.getElementById("op-body").dataset.view === "list");
    const row = await page.evaluate(rowTruth, name);
    assertEqual(row.missing, true, "back at the list, its row is not dimmed");
    return name + ": its view says '" + view.sub + "'; its row dims; the head reads '" + row.head + "'";
  });

  await run.check("D7 — Under the hood and the Output panel are one right-hand pane at a time, both ways, and both buttons say so", async () => {
    // The panel open on the list, as Ray had it.
    if (await page.evaluate(() => document.getElementById("outpanel").hidden)) await page.click("#out-top");
    await page.waitForFunction(() => !document.getElementById("outpanel").hidden);
    await page.waitForFunction(() => document.getElementById("drill-chip-zone").textContent.includes("team job"));
    await page.click(".drill-chip");
    await page.waitForSelector("#slideover:not([hidden])");
    const panes = () =>
      page.evaluate(() => ({
        slideover: !document.getElementById("slideover").hidden,
        output: !document.getElementById("outpanel").hidden,
        believes: window.RichOutput.isOpen(),
        pressed: ["out-top", "out-bottom"].map((id) => document.getElementById(id).getAttribute("aria-pressed")),
        outputOpenClass: document.body.classList.contains("output-open"),
      }));
    let s = await panes();
    assertEqual(s, { slideover: true, output: false, believes: false, pressed: ["false", "false"], outputOpenClass: false }, "Under the hood opened over the Output panel");
    // The other way. While Under the hood is open its backdrop takes every click outside it (a
    // click there closes it), so the Output panel is reached from the keyboard: ⌘⇧O.
    await page.focus("#input");
    await page.keyboard.press("Meta+Shift+O");
    await page.waitForFunction(() => !document.getElementById("outpanel").hidden);
    s = await panes();
    assertEqual(s, { slideover: false, output: true, believes: true, pressed: ["true", "true"], outputOpenClass: true }, "the Output panel opened under Under the hood");
    // And the toggle stays a toggle: after Under the hood came and went, ONE press of an Output
    // button opens the panel — it never "closes" a panel nobody can see.
    await page.click(".drill-chip");
    await page.waitForSelector("#slideover:not([hidden])");
    s = await panes();
    assertEqual(s.output, false, "the second time, Under the hood again opened over the panel");
    await page.click("#slideover-close");
    await page.waitForFunction(() => document.getElementById("slideover").hidden);
    await page.click("#out-top");
    await page.waitForFunction(() => !document.getElementById("outpanel").hidden);
    s = await panes();
    assertEqual(s, { slideover: false, output: true, believes: true, pressed: ["true", "true"], outputOpenClass: true }, "one press on Output after Under the hood did not open the panel");
    return "chip → the panel closed, both buttons unpressed; ⌘⇧O → Under the hood closed, the panel open and pressed; one press reopens";
  });

  await run.check("D9 — the CSV facts line never leaves a '·' alone at the end or the start of a line, however it wraps", async () => {
    // A page of its own: the checks above took the fixture's CSV away. Narrowed until the facts
    // wrap, at more than one width, as a resize does.
    const page2 = await openApp(browser);
    let wrapped = 0;
    const seen = [];
    for (const width of [1440, 1280, 1180, 1000]) {
      await page2.setViewportSize({ width, height: 900 });
      if (await page2.evaluate(() => document.getElementById("outpanel").hidden)) await page2.click("#out-top");
      await page2.waitForSelector(rowOf(page2, "q3-revenue.csv"));
      await page2.click(rowOf(page2, "q3-revenue.csv"));
      await page2.waitForSelector("#op-viewer .of-meta");
      const lines = await page2.evaluate(() => {
        const meta = document.querySelector("#op-viewer .of-meta");
        const clip = meta.getBoundingClientRect();
        const glyphs = [];
        const walker = document.createTreeWalker(meta, NodeFilter.SHOW_TEXT);
        for (let t = walker.nextNode(); t; t = walker.nextNode()) {
          for (let i = 0; i < t.data.length; i += 1) {
            if (/\s/.test(t.data[i])) continue;
            const r = document.createRange();
            r.setStart(t, i);
            r.setEnd(t, i + 1);
            const b = r.getBoundingClientRect();
            if (!b.width) continue;
            // A glyph clipped away by the facts line's own box is not on screen.
            const shown = b.left >= clip.left - 0.5 && b.right <= clip.right + 0.5 && b.top >= clip.top - 0.5 && b.bottom <= clip.bottom + 0.5;
            if (shown) glyphs.push({ ch: t.data[i], left: b.left, right: b.right, mid: (b.top + b.bottom) / 2 });
          }
        }
        const rows = [];
        for (const g of glyphs) {
          let row = rows.find((r) => Math.abs(r.mid - g.mid) < 4);
          if (!row) rows.push((row = { mid: g.mid, glyphs: [] }));
          row.glyphs.push(g);
        }
        return rows.sort((a, b) => a.mid - b.mid).map((r) => r.glyphs.sort((a, b) => a.left - b.left).map((g) => g.ch).join(""));
      });
      seen.push(width + "px " + JSON.stringify(lines));
      if (lines.length > 1) wrapped += 1;
      assert(lines.join("").includes("rows") && lines.join("").includes("Comma-separated"), "the facts line lost its words at " + width + "px: " + JSON.stringify(lines));
      for (const line of lines) {
        assert(!/^·|·$/.test(line), "at " + width + "px a line of the facts starts or ends with '·': " + JSON.stringify(lines));
      }
      await page2.keyboard.press("Escape");
    }
    assert(wrapped > 0, "the facts line never wrapped, so nothing was proven: " + seen.join("; "));
    assertEqual(page2.__errors, [], "the page reported errors");
    await page2.context().close();
    return seen.join("; ");
  });

  for (const theme of ["dark", "light"]) {
    await run.check("D10 — " + theme + ": the panel's scroll areas are drawn in the " + theme + " scheme", async () => {
      // A page of its own per theme, so nothing the checks above left on screen stands in the way.
      const p = await openApp(browser, { theme });
      await p.click("#out-top");
      await p.waitForSelector("#outpanel .orow");
      const schemes = await p.evaluate(() => {
        const of = (sel) => {
          const n = document.querySelector(sel);
          return n ? getComputedStyle(n).colorScheme : null;
        };
        return { panel: of("#outpanel"), list: of("#op-body") };
      });
      assertEqual(schemes, { panel: theme, list: theme }, "the scroll bar is drawn in the other scheme");
      assertEqual(p.__errors, [], "the page reported errors");
      await p.context().close();
      return JSON.stringify(schemes);
    });
  }

  await run.check("no page errors", async () => {
    assertEqual(page.__errors, [], "the page reported errors");
    return "0 errors";
  });

  await page.context().close();
  await browser.close();
  process.exitCode = run.report() > 0 ? 1 : 0;
}

main().catch((e) => {
  console.error(e);
  process.exit(1);
});

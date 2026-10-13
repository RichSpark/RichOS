"use strict";

// THE SEARCH PALETTE'S DOCUMENTS SECTION, SUPER FINDER AND REQUIRE QUOTED TEXT (search PRD v3.1,
// richos-hq `docs/prds/2026-10-13-qdrant-edge-semantic-search.md`; the CEO's chosen design is
// richos-hq `design/mockups/rounds/round-26/`), driven through the shipping shell against the
// mock's synthetic folder (`mock.js`, `MOCK_DOCS`). What a real folder and a real model do is
// `crates/richos-search`'s to prove; this suite proves what the palette does with the answers:
// the initial checkboxes, a toggle that keeps the query, the four combinations' labels, load on
// scroll with no page controls, request identity, Open original by source ID, the truthful
// fallback and no-folder lines, and their contrast in both themes.

const path = require("path");
const { spawnSync } = require("child_process");
const { loadPlaywright, leaveHome, createRun, assert, assertEqual, UI_DIR } = require("./lib/harness");
const APP = "file://" + path.join(UI_DIR, "index.html");

async function openApp(browser, preset) {
  const page = await browser.newPage({ viewport: { width: 1440, height: 900 } });
  page.__errors = [];
  page.on("pageerror", (error) => page.__errors.push(String(error)));
  if (preset) await page.addInitScript((v) => { window.__RICHOS_MOCK_PRESET__ = v; }, preset);
  await page.goto(APP);
  await leaveHome(page);
  await page.waitForSelector(".nav-thread", { state: "attached" });
  await page.keyboard.press("Meta+k");
  await page.waitForSelector("#search-overlay:not([hidden])");
  return page;
}

async function docTitles(page) {
  return page.evaluate(() => [...document.querySelectorAll("#search-results .srch-doc .srch-title")].map((t) => t.textContent));
}

function ratio(colors) {
  const contrast = spawnSync("python3", [path.resolve(UI_DIR, "../scripts/qa/contrast.py"), ...colors], { encoding: "utf8" });
  assertEqual(contrast.status, 0, contrast.stdout + contrast.stderr);
  return contrast.stdout.trim();
}

async function main() {
  const run = createRun("Search palette: Documents, Super Finder and Require quoted text");
  const browser = await loadPlaywright().webkit.launch();
  try {
    const page = await openApp(browser);

    await run.check("the palette opens with Super Finder checked and Require quoted text unchecked", async () => {
      const state = await page.evaluate(() => ({
        superFinder: document.getElementById("srch-super").checked,
        strict: document.getElementById("srch-strict").checked,
        hint: !document.getElementById("search-hint").hidden,
        status: document.getElementById("srch-status-text").textContent,
      }));
      assertEqual(state.superFinder, true, "Super Finder");
      assertEqual(state.strict, false, "Require quoted text");
      assert(state.hint, "the empty palette explains the two boxes");
      assert(state.status.startsWith("Super Finder ready"), state.status);
    });

    await run.check("a document found only by meaning is marked so, with its passage and Open original", async () => {
      await page.fill("#search-input", "the app felt slow on first open");
      await page.waitForSelector("#search-results .srch-doc");
      const row = await page.evaluate(() => {
        const r = document.querySelector("#search-results .srch-doc");
        return { why: r.querySelector(".srch-why").textContent, passage: r.querySelector(".srch-passage").textContent, mode: document.querySelector(".srch-group-mode").textContent };
      });
      assertEqual(row.why, "meaning", "why it matched");
      assert(row.passage.length > 40, "a useful passage under the title");
      assertEqual(row.mode, "meaning and words", "the Documents heading says the mode");
      await page.keyboard.press("ArrowDown");
      await page.waitForSelector("#search-results .srch-doc.is-active .srch-open-btn", { state: "visible" });
    });

    await run.check("unchecking Super Finder keeps the query and reruns as text search; checking it brings the match back", async () => {
      await page.click("label.srch-check:has(#srch-super)");
      await page.waitForFunction(() => document.getElementById("srch-status-text").textContent.startsWith("Text search"));
      await page.waitForFunction(() => !document.querySelector("#search-results .srch-doc"));
      assertEqual(await page.inputValue("#search-input"), "the app felt slow on first open", "the query");
      assertEqual(await page.evaluate(() => document.querySelector(".srch-group-mode").textContent), "words only", "the mode");
      const note = await page.evaluate(() => document.querySelector("#search-results .srch-note").textContent);
      assert(note.includes("Text search needs one of your words"), note);
      await page.click("label.srch-check:has(#srch-super)");
      await page.waitForSelector("#search-results .srch-doc");
    });

    await run.check("Require quoted text keeps only the exact span, and unchecking it lets related wording in below it", async () => {
      await page.fill("#search-input", 'cancel "30 days"');
      await page.waitForSelector("#search-results .srch-doc");
      await page.click("label.srch-check:has(#srch-strict)");
      await page.waitForFunction(() => document.querySelector(".srch-group-mode").textContent.endsWith("quoted text required"));
      await page.waitForFunction(() => document.querySelectorAll("#search-results .srch-doc").length === 1);
      const why = await page.evaluate(() => document.querySelector("#search-results .srch-doc .srch-why").textContent);
      assertEqual(why, "exact “30 days”", "the exact tag");
      await page.click("label.srch-check:has(#srch-strict)");
      await page.waitForFunction(() => document.querySelectorAll("#search-results .srch-doc").length > 1);
      const titles = await docTitles(page);
      assertEqual(titles[0], "Refund window for annual plans", "the exact one stays first");
      assert(titles.includes("Early draft of the refund rule"), "the 'thirty days' draft arrives below it");
    });

    await run.check("an unpaired quote under Require quoted text is said, never relaxed", async () => {
      await page.click("label.srch-check:has(#srch-strict)");
      await page.fill("#search-input", 'cancel "30 days');
      await page.waitForFunction(() => /quote marks don’t match/.test((document.querySelector("#search-results .srch-note") || {}).textContent || ""));
      assertEqual((await docTitles(page)).length, 0, "no documents on a guess");
      await page.click("label.srch-check:has(#srch-strict)");
    });

    await run.check("documents load as the list is scrolled, with no page controls", async () => {
      await page.fill("#search-input", "subscription pricing");
      await page.waitForFunction(() => document.querySelectorAll("#search-results .srch-doc").length === 12);
      for (let i = 0; i < 8; i++) {
        if (await page.evaluate(() => (document.getElementById("srch-tail") || {}).className === "srch-end")) break;
        await page.evaluate(() => { const r = document.getElementById("search-results"); r.scrollTop = r.scrollHeight; });
        await page.waitForFunction((n) => document.querySelectorAll("#search-results .srch-doc").length > n
          || (document.getElementById("srch-tail") || {}).className === "srch-end", await page.evaluate(() => document.querySelectorAll("#search-results .srch-doc").length));
      }
      const end = await page.evaluate(() => ({ n: document.querySelectorAll("#search-results .srch-doc").length, text: document.getElementById("srch-tail").textContent }));
      assertEqual(end.n, 31, "every document arrived");
      assertEqual(end.text, "That’s all 31 documents.", "the end of the list says so");
      const words = await page.evaluate(() => document.getElementById("search-overlay").innerText);
      assert(!/\bpage \d|next page|previous page/i.test(words), "no pagination words");
      return end.n + " documents in three scroll loads";
    });

    await run.check("a late reply for an older query never replaces the newer one", async () => {
      // The older query's reply is held until the newer list is on screen, then let go, and the
      // check waits for that reply to have come back: facts, never a clock.
      await page.evaluate(() => {
        const invoke = window.RichBridge.invoke.bind(window.RichBridge);
        window.__asked = [];
        window.__lateReturned = false;
        window.__releaseLate = new Promise((release) => { window.__letGo = release; });
        window.RichBridge.invoke = async (cmd, args) => {
          if (cmd !== "search_documents") return invoke(cmd, args);
          window.__asked.push(args.request.query);
          if (args.request.query !== "refund") return invoke(cmd, args);
          await window.__releaseLate;
          const reply = await invoke(cmd, args);
          window.__lateReturned = true;
          return reply;
        };
      });
      await page.fill("#search-input", "refund");
      await page.waitForFunction(() => window.__asked.includes("refund"));
      await page.fill("#search-input", "subscription pricing");
      await page.waitForFunction(() => [...document.querySelectorAll("#search-results .srch-doc .srch-title")]
        .some((t) => t.textContent.startsWith("Subscription pricing")));
      await page.evaluate(() => window.__letGo());
      await page.waitForFunction(() => window.__lateReturned);
      // Two frames, so the palette's own handler for that late reply has run and drawn.
      await page.evaluate(() => new Promise((done) => requestAnimationFrame(() => requestAnimationFrame(done))));
      const titles = await docTitles(page);
      // The list is still the newer query's: its own first result, and no refund document that
      // only the older query's reply holds.
      assert(titles.length > 0 && titles[0].startsWith("Subscription pricing"), titles.join(" | "));
      assert(!titles.some((t) => /refund/i.test(t)), "the older reply was drawn: " + titles.join(" | "));
    });

    await run.check("Enter on a document opens it by its source ID and closes the palette", async () => {
      await page.evaluate(() => {
        const invoke = window.RichBridge.invoke.bind(window.RichBridge);
        window.__opened = [];
        window.RichBridge.invoke = async (cmd, args) => {
          if (cmd === "open_search_document") window.__opened.push(args);
          return invoke(cmd, args);
        };
      });
      await page.keyboard.press("ArrowDown");
      await page.keyboard.press("Enter");
      await page.waitForFunction(() => document.getElementById("search-overlay").hidden);
      const opened = await page.evaluate(() => window.__opened);
      assertEqual(opened.length, 1, "one open");
      assertEqual(Object.keys(opened[0]).join(","), "sourceId", "only a source ID crosses to the host");
      assert(!opened[0].sourceId.includes("/"), "no path");
    });

    await run.check("no browser errors", () => assertEqual(page.__errors.length, 0, page.__errors.join("\n")));
    await page.close();

    for (const [preset, line, mode] of [
      ["unavailable", "Super Finder unavailable; showing text matches", "words only, Super Finder unavailable"],
      ["none", "No search folder configured", ""],
    ]) {
      await run.check("status '" + line + "' is said, readable in both themes", async () => {
        const p = await openApp(browser, { documentSearch: preset });
        await p.fill("#search-input", "refund");
        await p.waitForFunction((l) => document.getElementById("srch-status-text").textContent === l, line);
        await p.waitForFunction(() => !!document.querySelector("#search-results .srch-doc, #search-results .srch-note"));
        assertEqual(await p.evaluate(() => document.querySelector(".srch-group-mode").textContent), mode, "the Documents heading");
        if (preset === "none") {
          const note = await p.evaluate(() => document.querySelector("#search-results .srch-note").textContent);
          assert(note.includes("RICHOS_SEARCH_DOCUMENTS_ROOT"), note);
        }
        const out = [];
        for (const theme of ["light", "dark"]) {
          const colors = await p.evaluate(async (theme) => {
            document.documentElement.setAttribute("data-theme", theme);
            await new Promise((r) => requestAnimationFrame(() => requestAnimationFrame(r)));
            // The status line's ink is translucent (--ink-soft), as are some grounds: composite
            // each over what is under it, down to the first opaque ground, the way it is seen.
            const parse = (c) => {
              const n = c.match(/[\d.]+/g).map(Number);
              return { r: n[0], g: n[1], b: n[2], a: n.length > 3 ? n[3] : 1 };
            };
            const over = (top, under) => ({
              r: top.r * top.a + under.r * (1 - top.a),
              g: top.g * top.a + under.g * (1 - top.a),
              b: top.b * top.a + under.b * (1 - top.a),
              a: 1,
            });
            const text = document.getElementById("srch-status-text");
            const layers = [];
            for (let el = text; el; el = el.parentElement) {
              const c = parse(getComputedStyle(el).backgroundColor);
              if (c.a > 0) layers.push(c);
              if (c.a === 1) break;
            }
            let ground = { r: 255, g: 255, b: 255, a: 1 };
            for (const layer of layers.reverse()) ground = over(layer, ground);
            const ink = over(parse(getComputedStyle(text).color), ground);
            const rgb = (c) => [c.r, c.g, c.b].map((v) => Math.round(v)).join(",");
            return [rgb(ink), rgb(ground)];
          }, theme);
          out.push(theme + ": " + ratio(colors));
        }
        assertEqual(p.__errors.length, 0, p.__errors.join("\n"));
        await p.close();
        return out.join("; ");
      });
    }
  } finally {
    await browser.close();
  }
  process.exitCode = run.report() ? 1 : 0;
}

main().catch((error) => { console.error(error); process.exitCode = 1; });

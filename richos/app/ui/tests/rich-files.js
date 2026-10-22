// A FILE FROM RICH — round 36 screen 3 (§138, build plan item 12), driven through the REAL
// SHELL: `index.html`, `main.js`, `timeline.js`, `rich-files.js`, `mock.js` and `style.css`,
// loaded from disk, with nothing stubbed but the Tauri bridge (which `mock.js` replaces).
//
// The CEO, 2026-10-21: "yes, the regular app needs to have EVERYTHING I USE AND HAVE." Rich (or
// Stu) puts a file in front of the user: it shows as a card under Rich's words; a click opens it
// with the Mac's default app through `rich_file_open` (which `opener.rs` resolves); "Show in
// Finder" sits beside it; a file that has gone shows so and offers "Ask Rich again".
//
// THE FIXTURE is `mock.js`'s `{ richFiles: "round-36" }` preset: the round's own turn ("send me
// the final counter as a PDF, and the comps") with its two files, `Acme-counteroffer.pdf`
// (Preview) and `Acme-comparables.xlsx` (Numbers). `round-36-missing` is the same turn with the
// PDF gone from the Mac.
//
// TWO COMMITTED PICTURE PAIRS, `shots-36/files-{ready,missing}-{dark,light}.png`, published through
// `publishShotFile`: written only under `RICHOS_SHOTS_REGENERATE`, otherwise compared.
//
// Run: node rich-files.js

"use strict";

const path = require("path");
const {
  loadPlaywright,
  leaveHome,
  createRun,
  assert,
  assertEqual,
  shot,
  publishShotFile,
  openThread,
  FRAME_STILL,
  UI_DIR,
} = require("./lib/harness");
const { parseCssColor, compositeOver, contrastRatio, hex } = require("./lib/contrast");

const APP = "file://" + path.join(UI_DIR, "index.html");
const SHOTS = path.join(__dirname, "shots-36");
const FIXED = new Date("2026-10-21T09:05:00-04:00");

async function openApp(browser, opts) {
  opts = opts || {};
  const context = await browser.newContext({
    viewport: { width: 1440, height: 900 },
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
    } catch (e) {
      /* storage unavailable: the shipped default */
    }
  }, theme);
  await page.addInitScript(FRAME_STILL);
  await page.addInitScript(() => {
    let state = 0x6a09e667;
    Math.random = () => {
      state ^= state << 13;
      state ^= state >>> 17;
      state ^= state << 5;
      return (state >>> 0) / 4294967296;
    };
  });
  await page.addInitScript((v) => {
    window.__RICHOS_MOCK_PRESET__ = v;
  }, { richFiles: opts.missing ? "round-36-missing" : "round-36" });
  await page.goto(APP);
  await leaveHome(page);
  await page.waitForSelector(".nav-thread", { state: "attached" });
  await openThread(page, "acme");
  await page.waitForSelector(".tl-rfile");
  page.__errors = errors;
  return page;
}

/// The cards as the user sees them, read from the DOM.
async function cards(page) {
  return page.evaluate(() =>
    [...document.querySelectorAll(".tl-rfile")].map((c) => {
      const shown = (sel) => {
        const n = c.querySelector(sel);
        return !!n && getComputedStyle(n).display !== "none";
      };
      return {
        id: c.dataset.fileId,
        state: c.dataset.s,
        name: c.querySelector(".tl-rfile-name").textContent,
        meta: c.querySelector(".tl-rfile-meta").textContent,
        ext: c.querySelector(".tl-rfile-ext").textContent,
        open: c.querySelector(".tl-rfile-open").textContent,
        label: c.querySelector(".tl-rfile-main").getAttribute("aria-label"),
        openShown: shown(".tl-rfile-open"),
        finderShown: shown(".tl-rfile-finder"),
        askShown: shown(".tl-rfile-ask"),
      };
    })
  );
}

async function main() {
  const run = createRun("A file from Rich — round 36 screen 3 (real shell, WebKit)");
  const { webkit } = loadPlaywright();
  const browser = await webkit.launch();

  const page = await openApp(browser);

  await run.check("the files sit under Rich's words in his reply, one card each, built like the attachment chip", async () => {
    const got = await cards(page);
    assertEqual(got.length, 2, "cards in the conversation");
    assertEqual(got[0].name, "Acme-counteroffer.pdf");
    assertEqual(got[0].meta, "PDF · 184 KB · 2 pages", "a PDF's card says its pages, as the round writes it");
    assertEqual(got[0].ext, "PDF");
    assertEqual(got[0].open, "Open in Preview");
    // The name says what pressing does, then the card's line: the name stands in for the button's
    // children, so without the line a screen reader never said the type, size or pages.
    assertEqual(got[0].label, "Open Acme-counteroffer.pdf in Preview, PDF · 184 KB · 2 pages");
    assertEqual(got[1].label, "Open Acme-comparables.xlsx in Numbers, Spreadsheet · 46 KB");
    assertEqual(got[1].name, "Acme-comparables.xlsx");
    assertEqual(got[1].meta, "Spreadsheet · 46 KB");
    assertEqual(got[1].open, "Open in Numbers");
    for (const c of got) {
      assert(c.openShown && c.finderShown && !c.askShown, c.name + ": Open and Show in Finder, no Ask Rich again");
    }
    const place = await page.evaluate(() => {
      const list = document.querySelector(".tl-rfiles");
      const lane = list.closest(".tl-lane");
      const prose = [...lane.querySelectorAll(".tl-rich")];
      const last = prose[prose.length - 1];
      return {
        role: list.getAttribute("role"),
        label: list.getAttribute("aria-label"),
        underProse: !!last && (last.compareDocumentPosition(list) & Node.DOCUMENT_POSITION_FOLLOWING) !== 0,
        proseText: last ? last.textContent : "",
        ask: list.closest(".tl-turn").querySelector(".tl-user-text").textContent,
      };
    });
    assertEqual(place.role, "list");
    assertEqual(place.label, "2 files from Rich");
    assert(place.underProse, "the cards are not under Rich's words");
    assert(place.proseText.includes("Here they are."), "the cards are not in the reply that sent them");
    assertEqual(place.ask, "send me the final counter as a PDF, and the comps");
    return got.map((c) => c.name + " · " + c.open).join(" | ");
  });

  await run.check("a click anywhere on the card opens it through the opener; Show in Finder shows it", async () => {
    await page.click('.tl-rfile[data-file-id="rf_0a1b2c3d4e5f6071"] .tl-rfile-name');
    await page.click('.tl-rfile[data-file-id="rf_8192a3b4c5d6e7f8"] .tl-rfile-open');
    await page.click('.tl-rfile[data-file-id="rf_0a1b2c3d4e5f6071"] .tl-rfile-finder');
    await page.waitForFunction(() => window.__RICHOS_MOCK__.richFileActions().length >= 3);
    const acts = await page.evaluate(() => window.__RICHOS_MOCK__.richFileActions());
    assertEqual(
      acts.map((a) => a.cmd + " " + a.threadId + " " + a.id),
      [
        "rich_file_open acme rf_0a1b2c3d4e5f6071",
        "rich_file_open acme rf_8192a3b4c5d6e7f8",
        "rich_file_reveal acme rf_0a1b2c3d4e5f6071",
      ],
      "what the cards asked the shell to do (ids, never paths)"
    );
    // The keyboard reaches the same open.
    await page.focus('.tl-rfile[data-file-id="rf_8192a3b4c5d6e7f8"] .tl-rfile-main');
    await page.waitForFunction(
      () => document.querySelector('.tl-rfile[data-file-id="rf_8192a3b4c5d6e7f8"]').dataset.s !== "opening"
    );
    await page.keyboard.press("Enter");
    await page.waitForFunction(() => window.__RICHOS_MOCK__.richFileActions().length >= 4);
    assertEqual(page.__errors, [], "the page reported errors");
  });

  await run.check("a file attached between turns shows at once, without a reload", async () => {
    const turn = await page.evaluate(() => document.querySelectorAll(".tl-turn")[document.querySelectorAll(".tl-turn").length - 1].dataset.turnId);
    const id = await page.evaluate((t) => window.__RICHOS_MOCK__.richFileArrive("acme", t, "Acme-signature-page.pdf"), turn);
    await page.waitForSelector('.tl-rfile[data-file-id="' + id + '"]');
    const got = await cards(page);
    assertEqual(got.length, 3, "cards after the live arrival");
    assertEqual(got[2].name, "Acme-signature-page.pdf");
    assertEqual(got[2].meta, "PDF · 12 KB · 1 page");
    // Round 36's arrival: the card that just came rises into place once (`r36-file-in`, 0.7 s,
    // starting 0.25 s after it arrives); the cards a load drew stay still. The page's clock is
    // fixed, so the delay still to run reads exactly 250 ms.
    const motion = () =>
      page.evaluate(() =>
        [...document.querySelectorAll(".tl-rfile")].map((c) => {
          const s = getComputedStyle(c);
          return {
            arriving: c.classList.contains("is-arriving"),
            name: s.animationName,
            duration: s.animationDuration,
            delay: c.style.getPropertyValue("--arrival-delay"),
          };
        })
      );
    const moving = await motion();
    assertEqual(moving.map((m) => m.arriving), [false, false, true], "only the card that arrived rises in");
    assertEqual([moving[2].name, moving[2].duration, moving[2].delay], ["tl-rfile-in", "0.7s", "250ms"], "its rise");
    // A rebuild of the turn while it runs (the reply streaming, a reload) carries its start, never restarts it.
    await page.evaluate((t) => window.__RICHOS_MOCK__.richFileArrive("acme", t, "Acme-annex.pdf"), turn);
    await page.waitForFunction(() => document.querySelectorAll(".tl-rfile").length === 4);
    const again = await motion();
    assertEqual(again.map((m) => m.arriving), [false, false, true, true], "both arrivals still rising after the rebuild");
    assertEqual([again[2].delay, again[3].delay], ["250ms", "370ms"], "the second starts 0.12 s after the first");
    // Reduced motion: no rise at all.
    await page.emulateMedia({ reducedMotion: "reduce" });
    assertEqual((await motion())[3].name, "none", "with reduced motion the card is simply there");
    assertEqual(page.__errors, [], "the page reported errors");
    await page.context().close();
  });

  await run.check("a file that is gone says so and offers Ask Rich again, which asks Rich in his words", async () => {
    const p = await openApp(browser, { missing: true });
    const got = await cards(p);
    assertEqual(got[0].state, "missing");
    assertEqual(got[0].meta, "No longer on this Mac. It was moved or deleted.");
    assert(!got[0].openShown && !got[0].finderShown && got[0].askShown, "gone: only Ask Rich again");
    assertEqual(got[1].state, "ready", "the other file is untouched");
    // A click on the gone card's body opens nothing.
    await p.click('.tl-rfile[data-file-id="rf_0a1b2c3d4e5f6071"] .tl-rfile-name');
    await p.click('.tl-rfile[data-file-id="rf_0a1b2c3d4e5f6071"] .tl-rfile-ask');
    await p.waitForFunction(() =>
      [...document.querySelectorAll(".tl-user-text")].some((n) => n.textContent === "Please send Acme-counteroffer.pdf again.")
    );
    const acts = await p.evaluate(() => window.__RICHOS_MOCK__.richFileActions());
    assertEqual(acts, [], "nothing was opened for a gone file");
    assertEqual(p.__errors, [], "the page reported errors");
    await p.context().close();
  });

  await run.check("with Techy Mode on the cards stay through a turn's end and through reopening the conversation", async () => {
    // Review rv-20261021T083350Z-8ca9bacf-e592: Techy Mode reloads from `get_machinery`, which
    // carried no cards, so the reload at every turn's end took them away.
    const p = await openApp(browser);
    await p.evaluate(() => window.RichBridge.invoke("set_techy_mode", { threadId: "acme", enabled: true }));
    await openThread(p, "hiring");
    await openThread(p, "acme");
    await p.waitForSelector(".tl-rfile");
    assertEqual((await cards(p)).length, 2, "cards after reopening in Techy Mode");
    // Count Techy Mode's snapshot reads, so the check waits for the re-read itself.
    await p.evaluate(() => {
      window.__machineryReads = 0;
      const invoke = window.RichBridge.invoke.bind(window.RichBridge);
      window.RichBridge.invoke = (cmd, args, options) => {
        const answer = invoke(cmd, args, options);
        if (cmd !== "get_machinery") return answer;
        return answer.then((payload) => {
          window.__machineryReads++;
          window.__machineryCards = payload.timeline.items.filter((i) => i.kind === "rich_file").length;
          return payload;
        });
      };
    });
    const turnsBefore = await p.evaluate(() => document.querySelectorAll(".tl-turn").length);
    await p.fill("#input", "and the signed copy when it comes back");
    await p.keyboard.press("Enter");
    await p.waitForFunction((n) => document.querySelectorAll(".tl-turn").length > n, turnsBefore);
    // The reply streams, the turn ends and the conversation is re-read from Techy Mode's snapshot.
    await p.waitForFunction(() => window.__machineryReads > 0, null, { timeout: 30000 });
    assertEqual(await p.evaluate(() => window.__machineryCards), 2, "Techy Mode's snapshot carries the cards");
    await p.waitForFunction(() =>
      [...window.__RICHOS_TIMELINE__().items.values()].filter((i) => i.kind === "rich_file").length === 2 &&
      document.querySelectorAll(".tl-rfile").length === 2);
    const got = await cards(p);
    assertEqual(got.map((c) => c.name), ["Acme-counteroffer.pdf", "Acme-comparables.xlsx"], "the cards after the turn ended");
    assertEqual(p.__errors, [], "the page reported errors");
    await p.context().close();
  });

  for (const theme of ["dark", "light"]) {
    await run.check(theme + " — the card's text and edges clear WCAG AA", async () => {
      const p = await openApp(browser, { theme });
      const read = await p.evaluate(() => {
        const card = document.querySelector(".tl-rfile");
        const ground = (n) => {
          for (let e = n; e; e = e.parentElement) {
            const bg = getComputedStyle(e).backgroundColor;
            if (bg && bg !== "rgba(0, 0, 0, 0)" && bg !== "transparent") return bg;
          }
          return getComputedStyle(document.body).backgroundColor;
        };
        const fg = (sel) => getComputedStyle(card.querySelector(sel)).color;
        return {
          card: getComputedStyle(card).backgroundColor,
          chat: ground(card.parentElement),
          name: fg(".tl-rfile-name"),
          meta: fg(".tl-rfile-meta"),
          open: fg(".tl-rfile-open"),
          finder: fg(".tl-rfile-finder"),
          edge: getComputedStyle(card).borderTopColor,
          ext: fg(".tl-rfile-ext"),
          glyph: getComputedStyle(card.querySelector(".tl-rfile-glyph .pg")).fill,
        };
      });
      const solid = (c, under) => {
        const v = parseCssColor(c);
        return v.a < 1 ? compositeOver(v, under) : v;
      };
      const chat = solid(read.chat, parseCssColor(theme === "dark" ? "rgb(12, 19, 34)" : "rgb(234, 230, 221)"));
      const card = solid(read.card, chat);
      const lines = [];
      for (const [what, color, ground, floor] of [
        ["file name", read.name, card, 4.5],
        ["type and size", read.meta, card, 4.5],
        ["Open in …", read.open, card, 4.5],
        ["Show in Finder", read.finder, card, 4.5],
        ["file type in the glyph (declared 14px, skippable; floor kept anyway)", read.ext, solid(read.glyph, card), 4.5],
        ["card edge against the chat", read.edge, chat, 3],
      ]) {
        const fg = solid(color, ground);
        const ratio = Math.round(contrastRatio(fg, ground) * 100) / 100;
        lines.push(what + " " + hex(fg) + " on " + hex(ground) + " " + ratio + ":1");
        assert(ratio >= floor, theme + ": " + what + " is " + ratio + ":1 against " + floor + ":1");
      }
      await p.context().close();
      return lines.join(" | ");
    });

    await run.check(theme + " — the committed pictures: the cards, and a file that is gone", async () => {
      const p = await openApp(browser, { theme });
      await p.locator(".tl-rfiles").scrollIntoViewIfNeeded();
      const ready = await shot(p, "files-ready-" + theme, { fullPage: false, parkPointer: true });
      publishShotFile(ready.file, path.join(SHOTS, "files-ready-" + theme + ".png"));
      assertEqual(p.__errors, [], "the page reported errors");
      await p.context().close();
      const q = await openApp(browser, { theme, missing: true });
      await q.locator(".tl-rfiles").scrollIntoViewIfNeeded();
      const missing = await shot(q, "files-missing-" + theme, { fullPage: false, parkPointer: true });
      publishShotFile(missing.file, path.join(SHOTS, "files-missing-" + theme + ".png"));
      assertEqual(q.__errors, [], "the page reported errors");
      await q.context().close();
      return "shots-36/files-ready-" + theme + ".png, shots-36/files-missing-" + theme + ".png";
    });
  }

  await browser.close();
  process.exitCode = run.report() > 0 ? 1 : 0;
}

main().catch((e) => {
  console.error(e);
  process.exit(1);
});

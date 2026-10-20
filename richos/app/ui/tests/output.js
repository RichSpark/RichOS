// THE OUTPUT PANEL — slice S4 of the Output side panel PRD, driven through the REAL SHELL:
// `index.html`, `main.js`, `output-panel.js`, `timeline.js`, `mock.js` and `style.css`, loaded
// from disk, with nothing stubbed but the Tauri bridge (which `mock.js` already replaces).
//
// The CEO, 2026-10-05: "a button here … ideally, at the top and at the bottom, so that the user
// can easily just click a button. And the entire, this entire side panel would open … with a list
// of all the files … that were output by this entire thread. This is what we need."
//
// richos-hq `docs/prds/2026-10-05-output-side-panel.md` §12.4 names what this suite proves, and
// its "Done when": round 17's states `closed`, `open`, `empty`, `from-link`, `from-digest` and
// `arrives` are reproducible in the shell from `mock.js` fixtures. Each of the six is a check
// below, named by the state it reproduces.
//
// THE FIXTURE is `mock.js`'s `{ output: "round-17" }` preset: the mockup's nine files across four
// turns of the `acme` thread, Rich's replies naming files in code spans, one file by a worker; the
// `hiring` thread ("Q4 hiring", the mockup's own empty thread) has none. Without the preset every
// thread's record is empty, which is what every OTHER suite photographs.
//
// THE CLOCK IS FIXED (`page.clock.setFixedTime`, New York, 2026-10-05 09:45) so the group times
// read the same on every run and the committed pictures do not move with the hour of the run.
// Timers still run, so the mock's turns still stream.
//
// SLICE S5, THE PREVIEWS (§7, §12.5), on a page of its own: `file-md`, `file-source`,
// `file-image`, `file-table` (CSV), `file-pdf`, `file-video` (and audio), a QuickLook rendition,
// and the §6.7 states `too large`, `no preview`, `read failed` and a link, each the shell's own
// sentence; every viewer `src` is the URL `output_preview` returned, never one the page built.
//
// FOUR COMMITTED PICTURE PAIRS, `shots-output/output-{open,empty,file-md,file-table}-{dark,light}.png`,
// published through `publishShot`: written only under `RICHOS_SHOTS_REGENERATE`, otherwise compared.
// Since slice S6 the `open` pair shows the focused first row's Open and ⋯, as round 17 draws a row
// that has focus.
//
// SLICE S6 (§12.6) has its own block below, "S6: the actions": the `menu` and `open-menu` states,
// arrow keys and Escape in menus, every action's notice sentence, and the disabled actions on a
// missing file and a link, with their contrast computed in both themes.
//
// Run: node output.js   (or `npm test` for every suite in this directory)

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
  awaitWorkerChipSettled,
  FRAME_STILL,
  UI_DIR,
} = require("./lib/harness");
const { parseCssColor, compositeOver, contrastRatio, hex } = require("./lib/contrast");

const APP = "file://" + path.join(UI_DIR, "index.html");
const SHOTS = path.join(__dirname, "shots-output");
const FIXED = new Date("2026-10-05T09:45:00-04:00");

async function openApp(browser, opts) {
  opts = opts || {};
  const context = await browser.newContext({
    viewport: opts.viewport || { width: 1440, height: 900 },
    timezoneId: "America/New_York",
    locale: "en-US",
    reducedMotion: opts.reducedMotion || "no-preference",
  });
  const page = await context.newPage();
  await page.clock.setFixedTime(FIXED);
  const errors = [];
  page.on("pageerror", (e) => errors.push(String(e)));
  page.on("console", (m) => {
    if (m.type() === "error") errors.push("console: " + m.text());
  });
  const theme = opts.theme || "dark";
  // The app's own stored preference AND its mirror, as `contrast.js` seeds them: the store wins
  // at init, so a mirror-only seed would be overwritten a moment after boot.
  await page.addInitScript((t) => {
    try {
      window.localStorage.setItem("richos-theme", t);
      window.localStorage.setItem("richos-font-scale", "100");
      window.localStorage.setItem("richos-mock-config", JSON.stringify({ theme: t, font_scale: 100, user_name: null }));
    } catch (e) {
      /* storage unavailable: the shipped default */
    }
  }, theme);
  // The app's frame never scrolls: a scroll of the document, `#app` or a pane is a page error,
  // which every "no page errors" check below refuses (harness.js FRAME_STILL).
  await page.addInitScript(FRAME_STILL);
  // A repeatable stream for the mock's ids and canned replies.
  await page.addInitScript(() => {
    let state = 0x6a09e667;
    Math.random = () => {
      state ^= state << 13;
      state ^= state >>> 17;
      state ^= state << 5;
      return (state >>> 0) / 4294967296;
    };
  });
  if (opts.preset !== null) {
    await page.addInitScript((v) => {
      window.__RICHOS_MOCK_PRESET__ = v;
    }, opts.preset || { output: "round-17" });
  }
  await page.goto(APP);
  await leaveHome(page);
  await page.waitForFunction(() => typeof window.RichOutput === "object" && typeof window.RichTimeline === "object");
  await page.waitForSelector(".nav-thread", { state: "attached" });
  page.__errors = errors;
  return page;
}

/// Open a thread from the rail and wait for its record to have been read: the buttons carry the
/// count the mock's record holds (or none), and the conversation's links are drawn.
async function openThread(page, threadId, count) {
  await page.click('.nav-thread[data-thread-id="' + threadId + '"]');
  await page.waitForFunction(
    ({ id, n }) => {
      const s = window.RichOutput.snapshot();
      return s.thread === id && s.count === n;
    },
    { id: threadId, n: count }
  );
  if (count > 0) await page.waitForSelector(".tl-file-link");
}

async function panelState(page) {
  return page.evaluate(() => {
    const panel = document.getElementById("outpanel");
    const btn = (id) => {
      const b = document.getElementById(id);
      return {
        hidden: b.hidden,
        pressed: b.getAttribute("aria-pressed"),
        label: b.getAttribute("aria-label"),
        count: b.querySelector(".out-count").textContent,
        countShown: getComputedStyle(b.querySelector(".out-count")).display !== "none",
      };
    };
    return {
      open: !panel.hidden,
      view: document.getElementById("op-body").dataset.view || null,
      title: document.getElementById("op-title").textContent,
      sub: document.getElementById("op-sub").textContent,
      top: btn("out-top"),
      bottom: btn("out-bottom"),
      focus: document.activeElement ? (document.activeElement.id || document.activeElement.className) : null,
      groups: [...document.querySelectorAll("#op-body .og")].map((g) => ({
        words: g.querySelector(".og-words") ? g.querySelector(".og-words").textContent : "",
        time: g.querySelector("time") ? g.querySelector("time").textContent : "",
        files: [...g.querySelectorAll(".oname")].map((n) => n.textContent),
      })),
    };
  });
}

async function main() {
  const run = createRun("Output panel S4 — the buttons, the list, empty, the links (real shell, WebKit)");
  const { webkit } = loadPlaywright();
  const browser = await webkit.launch();
  const page = await openApp(browser);
  const record = await page.evaluate(() => window.__RICHOS_MOCK__.outputList("acme"));
  const byName = (name) => record.files.find((f) => f.name === name);

  await run.check("closed — both buttons carry the count of files this thread produced, and nothing opens", async () => {
    await openThread(page, "acme", 9);
    const s = await panelState(page);
    assert(!s.open, "the panel opened by itself (§6.6: nothing opens on its own)");
    for (const b of [s.top, s.bottom]) {
      assert(!b.hidden, "an Output button is hidden on an open thread");
      assertEqual(b.count, "9");
      assert(b.countShown, "the count is not shown");
      assertEqual(b.pressed, "false");
      assertEqual(b.label, "Output: 9 files from this thread");
    }
    const place = await page.evaluate(() => {
      const top = document.getElementById("out-top");
      const bottom = document.getElementById("out-bottom");
      return {
        topInHeader: !!top.closest("#stage-header"),
        bottomInComposer: !!bottom.closest("#composer-row"),
        topRight: Math.round(top.getBoundingClientRect().right),
        headerRight: Math.round(document.getElementById("stage-header").getBoundingClientRect().right),
        bottomH: Math.round(bottom.getBoundingClientRect().height),
        talkH: Math.round(document.getElementById("talk-toggle").getBoundingClientRect().height),
      };
    });
    assert(place.topInHeader && place.bottomInComposer, "the buttons are not where the CEO pointed: " + JSON.stringify(place));
    assertEqual(place.bottomH, place.talkH, "the bottom button is not as tall as the composer's own controls");
    return "9 on both; top at the header's right (" + place.topRight + " of " + place.headerRight + "px), bottom beside the composer at " + place.bottomH + "px";
  });

  await run.check("open — one click opens a docked panel listing every file, grouped by turn, newest first", async () => {
    await page.click("#out-top");
    await page.waitForSelector("#outpanel .orow");
    // The panel slides open (`op-open`, .38s): measured once the slide has landed, never mid-way.
    await page.evaluate(() => Promise.all(document.getElementById("outpanel").getAnimations().map((a) => a.finished)));
    const s = await panelState(page);
    assert(s.open, "the panel did not open");
    assertEqual(s.title, "9 files from this thread");
    assertEqual(s.sub, "Everything written here, newest first.");
    assertEqual(s.top.pressed, "true");
    assertEqual(s.bottom.pressed, "true", "the SAME control in two places: both say it is pressed");
    assertEqual(
      s.groups.map((g) => g.words),
      [
        "and a 30-second walkthrough of the comps for Priya",
        "pull the signed term sheet and the Q3 numbers before I send it",
        "draft it, keep it firm",
        "what's the status on Acme?",
      ],
      "the groups are his words, newest first"
    );
    assertEqual(s.groups.map((g) => g.time), ["Today 9:40 AM", "Today 9:31 AM", "Today 9:15 AM", "Yesterday 1:45 PM"]);
    assertEqual(
      s.groups.map((g) => g.files),
      [
        ["comps-walkthrough.mp4"],
        ["term-sheet-march.pdf", "q3-revenue.csv", "q3-revenue-chart.png"],
        ["counter-draft-v1.docx", "counter-draft-v1.md", "brief.md"],
        ["comps-2026-10-04.xlsx", "comps-summary.md"],
      ]
    );
    const layout = await page.evaluate(() => {
      const p = document.getElementById("outpanel").getBoundingClientRect();
      const c = document.getElementById("conversation").getBoundingClientRect();
      const row = [...document.querySelectorAll(".orow")].find((r) => r.textContent.includes("q3-revenue.csv"));
      return {
        width: Math.round(p.width),
        beside: p.left >= c.right - 1,
        modal: document.getElementById("outpanel").getAttribute("aria-modal"),
        scrim: document.getElementById("outpanel-scrim").hidden,
        by: row.querySelector(".oby") ? row.querySelector(".oby").textContent : null,
        folder: row.querySelector(".opath").textContent,
        tiles: [...document.querySelectorAll(".okind")].map((t) => t.textContent),
      };
    });
    assertEqual(layout.width, 400, "the panel's default width");
    assert(layout.beside, "the panel covers the conversation instead of sitting beside it");
    assertEqual(layout.modal, null, "a docked pane, not a modal");
    assert(layout.scrim, "nothing dims at 1440px");
    assertEqual(layout.by, "by Clark", "a worker's file says who wrote it (§11 row 7)");
    assertEqual(layout.folder, "~/FemcBoost/acme/reference/");
    assertEqual(layout.tiles, ["mp4", "pdf", "csv", "png", "doc", "md", "md", "xls", "md"]);
    return "400px beside the conversation; 4 groups, 9 rows; 'by Clark' on the worker's file";
  });

  await run.check("focus moves into the list on open and back to the button on close", async () => {
    const opened = await page.evaluate(() => ({
      cls: document.activeElement.className,
      name: document.activeElement.querySelector(".oname") ? document.activeElement.querySelector(".oname").textContent : null,
    }));
    assertEqual(opened.cls, "orow", "focus did not move to the first row");
    assertEqual(opened.name, "comps-walkthrough.mp4");
    await page.keyboard.press("Escape");
    await page.waitForFunction(() => document.getElementById("outpanel").hidden);
    const s = await panelState(page);
    assertEqual(s.focus, "out-top", "closing did not return focus to the button that opened it");
    assertEqual(s.top.pressed, "false");
    return "first row focused on open; Escape from the list closed it and focus is back on #out-top";
  });

  await run.check("a row opens the file in the panel; back keeps the row selected; ‹ › walk the files", async () => {
    await page.click("#out-bottom");
    await page.waitForSelector("#outpanel .orow");
    await page.click('.orow[data-output="' + byName("q3-revenue.csv").id + '"]');
    await page.waitForSelector("#of-back");
    await page.waitForSelector("#op-viewer[data-preview]");
    let s = await panelState(page);
    assertEqual(s.view, "file");
    assertEqual(s.title, "q3-revenue.csv");
    // A LANDED file (`source: "land"`, S2b): rendered like any other, by its worker, and its
    // earlier write is said — the panel reads `source` nowhere, so there is no unknown-source path.
    assertEqual(byName("q3-revenue.csv").source, "land", "the fixture's landed file is not a land entry");
    assertEqual(s.sub, "Written today 9:33 AM · 1 KB · also written earlier in this thread");
    const file = await page.evaluate(() => ({
      back: document.getElementById("of-back").textContent,
      k: document.querySelector(".of-k").textContent,
      path: document.querySelector(".of-path code").textContent,
      facts: document.querySelector(".of-meta") ? document.querySelector(".of-meta").textContent : null,
      focus: document.activeElement.id,
    }));
    assertEqual(file.back, "All output · 9");
    assertEqual(file.k, "3 of 9");
    assertEqual(file.path, "~/FemcBoost/acme/reference/q3-revenue.csv");
    assertEqual(file.facts, "4 rows·Comma-separated·The whole sheet opens in Numbers", "the file view states what the preview shows (S5, §7)");
    assertEqual(file.focus, "of-back");
    await page.click('[aria-label="Next file"]');
    assertEqual((await panelState(page)).title, "q3-revenue-chart.png", "‹ › walk the thread's files in list order");
    await page.click("#of-back");
    s = await panelState(page);
    assertEqual(s.view, "list");
    const sel = await page.evaluate(() => {
      const open = document.querySelector(".orow.is-open");
      return {
        open: open ? open.querySelector(".oname").textContent : null,
        focus: document.activeElement.classList.contains("orow") ? document.activeElement.querySelector(".oname").textContent : null,
      };
    });
    assertEqual(sel.open, "q3-revenue-chart.png", "back did not keep the row selected");
    assertEqual(sel.focus, "q3-revenue-chart.png", "back did not put focus on that row");
    return "row → file (3 of 9) → next (4 of 9) → back: the row is selected and focused";
  });

  await run.check("Escape steps back one level at a time: the file, then the list, then closed", async () => {
    await page.keyboard.press("Enter"); // the focused row opens its file
    await page.waitForSelector("#of-back");
    await page.keyboard.press("Escape");
    let s = await panelState(page);
    assert(s.open && s.view === "list", "Escape from a file must go back to the list, not close the panel: " + JSON.stringify(s));
    await page.keyboard.press("Escape");
    s = await panelState(page);
    assert(!s.open, "Escape from the list must close the panel");
    // And from OUTSIDE the panel, the way every popup answers it (main.js `dismissTopmostPopup`).
    await page.click("#out-top");
    await page.waitForSelector("#outpanel .orow");
    await page.focus("#input");
    await page.keyboard.press("Escape");
    s = await panelState(page);
    assert(!s.open, "Escape from the composer did not close the panel");
    return "file → list → closed; and closed from the composer too";
  });

  await run.check("from-link — a file named in Rich's message opens the panel on that file, and the link pulses", async () => {
    const id = byName("q3-revenue-chart.png").id;
    const link = '.tl-prose .tl-file-link[data-output-id="' + id + '"]';
    const before = await page.evaluate((sel) => {
      const l = document.querySelector(sel);
      return l ? { text: l.textContent, inProse: !!l.closest(".tl-prose") } : null;
    }, link);
    assert(before, "the code span `q3-revenue-chart.png` in Rich's reply is not a link");
    await page.click(link);
    await page.waitForSelector("#of-back");
    const s = await panelState(page);
    assertEqual(s.title, "q3-revenue-chart.png");
    const pulsed = await page.evaluate((sel) => document.querySelector(sel).classList.contains("flash"), link);
    assert(pulsed, "the link did not pulse");
    // ONLY RECORDED FILES BECOME LINKS. A code span naming a file nothing witnessed stays code.
    const plain = await page.evaluate(() => {
      const box = document.createElement("div");
      window.RichTimeline.renderMarkdownInto(box, "see `not-recorded.md` and `brief.md`", window.RichOutput.links);
      return { code: [...box.querySelectorAll("code")].map((c) => c.textContent), links: [...box.querySelectorAll(".tl-file-link")].map((c) => c.textContent) };
    });
    assertEqual(plain.code, ["not-recorded.md"]);
    assertEqual(plain.links, ["brief.md"]);
    // The list is one click away, and closing returns focus to the conversation.
    await page.keyboard.press("Escape");
    await page.keyboard.press("Escape");
    const closed = await panelState(page);
    assert(!closed.open, "two Escapes from a file opened by a link did not close the panel");
    assertEqual(closed.focus, "conversation", "a panel opened from a link returns focus to the conversation");
    return "`" + before.text + "` → the panel on that file; an unrecorded name stays a code span";
  });

  await run.check("from-digest — 'Wrote 3 files' opens the panel at the list and that turn's group pulses", async () => {
    const turn = byName("brief.md").turnId;
    const sel = '[id="wrote:' + turn + '"]';
    const label = await page.evaluate((s) => (document.querySelector(s) ? document.querySelector(s).textContent : null), sel);
    assertEqual(label, "Wrote 3 files");
    const strip = await page.evaluate((t) => {
      const sec = document.querySelector('.tl-turn[data-turn-id="' + t + '"] .tl-produced');
      return sec ? sec.textContent : null;
    }, turn);
    assertEqual(strip, "Produced 3 filescounter-draft-v1.docxcounter-draft-v1.mdbrief.md", "the produced strip under the turn");
    await page.click(sel);
    await page.waitForSelector("#outpanel .orow");
    const g = await page.evaluate((t) => {
      const sec = document.querySelector('.og[data-turn="' + t + '"]');
      return {
        flash: sec.classList.contains("flash"),
        focus: document.activeElement.closest(".og") === sec,
        view: document.getElementById("op-body").dataset.view,
      };
    }, turn);
    assertEqual(g.view, "list");
    assert(g.flash, "the turn's group did not pulse");
    assert(g.focus, "focus is not in that turn's group");
    await page.keyboard.press("Escape");
    return "'Wrote 3 files' → the list, \"draft it, keep it firm\" pulsed and focused";
  });

  // THE APP'S FRAME NEVER SCROLLS (nightly a1a26a615). `#app` was `overflow: hidden`, which is
  // still a scroll container, and the thread's screen-reader labels hang below the window once
  // the panel narrows it, so it had 417px of hidden range. "Wrote N files" (`openGroup`'s
  // `scrollIntoView({ block: "start" })`) scrolled the whole shell up by up to that much, and so
  // did the test driver's click when it retried with a forced alignment: the conversation, the
  // rail and the panel all started partway down, with a blank band at the bottom. Driven here
  // through the app's own click (not the driver's, which scrolls on its own account), every
  // turn's digest, then every row shown at every alignment, then the frame asked to scroll
  // outright: the document, `#app` and the panel stay at 0 throughout.
  await run.check("the app's frame never scrolls — 'Wrote N files' on every turn, any row shown at any alignment", async () => {
    const frame = () =>
      page.evaluate(() => ({
        doc: document.scrollingElement.scrollTop,
        app: document.getElementById("app").scrollTop,
        panel: document.getElementById("outpanel").scrollTop,
      }));
    const still = { doc: 0, app: 0, panel: 0 };
    try {
      return await frameWalk();
    } finally {
      // Whatever it found, the next check starts where it expects: the frame at 0, the panel closed.
      await page.evaluate(() => {
        document.scrollingElement.scrollTop = 0;
        document.getElementById("app").scrollTop = 0;
        document.getElementById("outpanel").scrollTop = 0;
      });
      if (await page.evaluate(() => window.RichOutput.isOpen())) {
        await page.keyboard.press("Escape");
        if (await page.evaluate(() => window.RichOutput.isOpen())) await page.keyboard.press("Escape");
        await page.waitForFunction(() => document.getElementById("outpanel").hidden);
      }
    }
    async function frameWalk() {
      const digests = await page.evaluate(() => [...document.querySelectorAll('[id^="wrote:"]')].map((n) => n.id));
      assert(digests.length >= 3, "the fixture's turns have no 'Wrote N files' to press: " + JSON.stringify(digests));
      for (const id of digests) {
        await page.evaluate((i) => document.getElementById(i).click(), id);
        await page.waitForSelector("#outpanel .orow");
        await page.evaluate(() => Promise.all(document.getElementById("outpanel").getAnimations().map((a) => a.finished)));
        assertEqual(await frame(), still, "pressing " + id + " scrolled the app's frame");
      }
      const aligned = await page.evaluate(() => {
        const moved = [];
        const rows = [...document.querySelectorAll("#outpanel .orow")];
        for (const row of rows) {
          for (const block of ["start", "center", "end", "nearest"]) {
            row.scrollIntoView({ block });
            const app = document.getElementById("app").scrollTop;
            const doc = document.scrollingElement.scrollTop;
            const panel = document.getElementById("outpanel").scrollTop;
            if (app || doc || panel) moved.push(row.querySelector(".oname").textContent + " " + block + ": app " + app + ", document " + doc + ", panel " + panel);
          }
        }
        document.getElementById("op-body").scrollTop = 0;
        return { rows: rows.length, moved };
      });
      assertEqual(aligned.moved, [], "showing a row scrolled the app's frame");
      const forced = await page.evaluate(() => {
        const boxes = { doc: document.scrollingElement, app: document.getElementById("app"), panel: document.getElementById("outpanel") };
        const out = {};
        for (const [k, b] of Object.entries(boxes)) {
          b.scrollTop = 100000;
          out[k] = b.scrollTop;
          b.scrollTop = 0;
        }
        return out;
      });
      assertEqual(forced, still, "the app's frame can be scrolled at all");
      return digests.length + " digests pressed; " + aligned.rows + " rows x 4 alignments; asked to scroll outright: document, #app and the panel all held at 0";
    }
  });

  await run.check("⌘⇧O toggles the panel, as either button does", async () => {
    await page.focus("#input");
    await page.keyboard.press("Meta+Shift+O");
    await page.waitForSelector("#outpanel .orow");
    assert((await panelState(page)).open, "⌘⇧O did not open the panel");
    await page.keyboard.press("Meta+Shift+O");
    const s = await panelState(page);
    assert(!s.open, "⌘⇧O did not close it");
    assertEqual(s.focus, "input", "⌘⇧O from the composer returns focus to the composer");
    return "open and closed by the shortcut; focus back in the composer";
  });

  await run.check("arrives — files a turn writes slide into an open list at the top, the scroll position holds, both counts tick", async () => {
    const small = await openApp(browser, { viewport: { width: 1400, height: 640 } });
    await openThread(small, "acme", 9);
    await small.click("#out-top");
    await small.waitForSelector("#outpanel .orow");
    const scrolled = await small.evaluate(() => {
      const body = document.getElementById("op-body");
      body.scrollTop = 120;
      return { top: body.scrollTop, max: body.scrollHeight - body.clientHeight };
    });
    assert(scrolled.top === 120, "the list is not tall enough to scroll at 640px, so this would prove nothing: " + JSON.stringify(scrolled));
    await small.evaluate(() => {
      // Every sentence the one polite region carries, in order: the shared region is written by
      // more than this panel, so what is asserted is that the count was among them, once.
      window.__said = [];
      const live = document.getElementById("live-region");
      new MutationObserver(() => {
        if (live.textContent) window.__said.push(live.textContent);
      }).observe(live, { childList: true, characterData: true, subtree: true });
      window.__ticks = 0;
      for (const c of document.querySelectorAll(".out-count")) {
        new MutationObserver(() => {
          if (c.classList.contains("tick")) window.__ticks += 1;
        }).observe(c, { attributes: true, attributeFilter: ["class"] });
      }
      window.__RICHOS_MOCK__.outputArrive("acme");
    });
    await small.waitForFunction(() => window.RichOutput.snapshot().count === 11, null, { timeout: 30000 });
    await small.waitForFunction(() => document.querySelectorAll("#op-body .og").length === 5);
    await small.waitForFunction(() => window.__said.indexOf("11 files from this thread") >= 0);
    const after = await small.evaluate(() => {
      const body = document.getElementById("op-body");
      const first = body.querySelector(".og");
      return {
        top: body.scrollTop,
        firstWords: first.querySelector(".og-words").textContent,
        firstFiles: [...first.querySelectorAll(".oname")].map((n) => n.textContent),
        arrived: first.classList.contains("arrive-group"),
        title: document.getElementById("op-title").textContent,
        counts: [...document.querySelectorAll(".out-count")].map((c) => c.textContent),
        ticks: window.__ticks,
        open: !document.getElementById("outpanel").hidden,
        said: window.__said.filter((t) => /files? from this thread$/.test(t)),
      };
    });
    assertEqual(after.top, 120, "the arrival moved what he was reading");
    assertEqual(after.firstWords, "make me a one-pager for the board");
    assertEqual(after.firstFiles, ["board-one-pager.pdf", "board-one-pager.md"]);
    assert(after.arrived, "the new group did not arrive with the rise");
    assertEqual(after.title, "11 files from this thread");
    assertEqual(after.counts, ["11", "11"]);
    assertEqual(after.ticks, 2, "the count ticks once on each button");
    assert(after.open, "the panel closed");
    assertEqual(after.said, ["11 files from this thread"], "one announcement of the new count, not one per button");
    assertEqual(small.__errors, [], "the page reported errors");
    await small.context().close();
    return "scrollTop 120 before and after; the one-pager group on top; 9 → 11 on both buttons, one tick each";
  });

  await run.check("arrives, panel closed — the count ticks and nothing opens on its own", async () => {
    const p = await openApp(browser);
    await openThread(p, "acme", 9);
    await p.evaluate(() => window.__RICHOS_MOCK__.outputArrive("acme"));
    await p.waitForFunction(() => window.RichOutput.snapshot().count === 11, null, { timeout: 30000 });
    const s = await panelState(p);
    assert(!s.open, "a turn's files opened the panel (§6.6, Iris's decision 8)");
    assertEqual([s.top.count, s.bottom.count], ["11", "11"]);
    // The conversation redraws on the record's revision, one frame after the count.
    await p.waitForFunction(() => document.querySelectorAll(".tl-produced").length === 5);
    const links = await p.evaluate(() => [...document.querySelectorAll(".tl-produced")].pop().textContent);
    assertEqual(links, "Produced 2 filesboard-one-pager.pdfboard-one-pager.md", "the new turn's produced strip");
    await p.context().close();
    return "closed panel stays closed; 11 on both; the new turn names its two files";
  });

  await run.check("empty — a thread that produced nothing: the button without a count, and the one sentence", async () => {
    await openThread(page, "hiring", 0);
    let s = await panelState(page);
    for (const b of [s.top, s.bottom]) {
      assert(!b.countShown, "an empty thread's button shows a count");
      assertEqual(b.label, "Output: nothing produced yet in this thread");
    }
    await page.click("#out-top");
    await page.waitForSelector(".op-empty");
    s = await panelState(page);
    assertEqual(s.title, "Nothing produced yet");
    assertEqual(s.sub, "Q4 hiring");
    const line = await page.evaluate(() => ({
      text: document.querySelector(".op-empty-line").textContent,
      bold: document.querySelector(".op-empty-line b").textContent,
      focus: document.activeElement.className,
    }));
    assertEqual(
      line.text,
      "Nothing in Q4 hiring has produced a file yet. The moment Rich or the team writes one, it is listed here and the count on the Output button says so."
    );
    assertEqual(line.bold, "Q4 hiring");
    assertEqual(line.focus, "op-empty", "focus moves to the sentence's container");
    return "no count; \"" + line.text.slice(0, 48) + "…\"";
  });

  await run.check("an open panel follows a thread switch to the new thread's list", async () => {
    await page.click('.nav-thread[data-thread-id="acme"]');
    await page.waitForFunction(() => document.querySelectorAll("#op-body .orow").length === 9);
    const s = await panelState(page);
    assert(s.open, "the panel closed on a thread switch");
    assertEqual(s.title, "9 files from this thread");
    return "Q4 hiring's empty panel → Acme deal's nine files, still open";
  });

  await run.check("the worker inspector and the panel are one right-hand pane at a time", async () => {
    await page.click('.nav-thread[data-thread-id="hiring"]');
    await page.waitForFunction(() => window.RichOutput.snapshot().thread === "hiring");
    await page.waitForSelector(".tl-duration-btn:not(.tl-duration-btn--static)");
    const disclosure = page.locator(".tl-duration-btn:not(.tl-duration-btn--static)").first();
    if ((await disclosure.getAttribute("aria-expanded")) !== "true") await disclosure.click();
    await page.waitForSelector(".tl-chip");
    await page.click(".tl-chip");
    await page.waitForSelector("#inspector:not([hidden])");
    assert(!(await panelState(page)).open, "opening the inspector left the Output panel open");
    await page.click("#out-top");
    await page.waitForSelector(".op-empty");
    const both = await page.evaluate(() => ({
      inspector: !document.getElementById("inspector").hidden,
      output: !document.getElementById("outpanel").hidden,
    }));
    assertEqual(both, { inspector: false, output: true }, "opening the panel did not close the inspector");
    await page.click(".tl-chip");
    await page.waitForSelector("#inspector:not([hidden])");
    assert(!(await panelState(page)).open, "opening the inspector did not close the panel");
    await page.keyboard.press("Escape");
    await page.waitForFunction(() => document.getElementById("inspector").hidden);
    return "panel → inspector → panel → inspector: never both";
  });

  await run.check("a missing file stays listed, dimmed, said so — and nothing on its row is an action", async () => {
    await page.evaluate(() => window.__RICHOS_MOCK__.outputMissing("acme", "brief.md"));
    await openThread(page, "acme", 9);
    await page.click("#out-top");
    await page.waitForSelector("#outpanel .orow.is-missing");
    const row = await page.evaluate(() => {
      const r = document.querySelector(".orow.is-missing");
      return {
        name: r.querySelector(".oname").textContent,
        line: r.querySelector(".opath").textContent,
        // S6: the row's Open is disabled (aria-disabled, its reason as the tooltip) and its menu
        // stays lit, because Copy path is (§6.7) — the menu's own contents are checked below.
        controls: [...r.parentElement.querySelectorAll("button, a[href], [data-act]")]
          .filter((c) => !c.disabled && c.getAttribute("aria-disabled") !== "true")
          .map((c) => c.dataset.act),
        nameColor: getComputedStyle(r.querySelector(".oname")).color,
        okColor: getComputedStyle(document.querySelector(".orow:not(.is-missing) .oname")).color,
        title: document.getElementById("op-title").textContent,
      };
    });
    assertEqual(row.name, "brief.md");
    assertEqual(row.line, "No longer where it was written");
    assertEqual(row.controls, ["menu"], "an action other than the menu is lit on a missing file's row");
    assert(row.nameColor !== row.okColor, "the missing row is not dimmed");
    assertEqual(row.title, "9 files from this thread · 1 no longer where it was written", "the count includes it and the header says so (§4.6)");
    await page.click(".orow.is-missing");
    await page.waitForSelector(".of-missing");
    const sentence = await page.evaluate(() => document.querySelector(".of-missing").textContent);
    assertEqual(
      sentence,
      "This file is no longer where it was written. If it was moved, open it from its new place; if Rich writes it again, it will be listed here."
    );
    await page.keyboard.press("Escape");
    await page.keyboard.press("Escape");
    return "dimmed, 'No longer where it was written', only its menu lit; its view says the §6.7 sentence";
  });

  await run.check("a record that cannot be read: no count, the shell's sentence and Try again", async () => {
    const p = await openApp(browser);
    await p.evaluate(() => window.__RICHOS_MOCK__.outputUnreadable(true));
    await p.click('.nav-thread[data-thread-id="acme"]');
    await p.waitForFunction(() => window.RichOutput.snapshot().error !== null);
    const s = await panelState(p);
    assert(!s.top.countShown && !s.bottom.countShown, "an unreadable record still shows a count");
    assertEqual(s.top.label, "Output", "no number nobody has read");
    await p.click("#out-top");
    await p.waitForSelector(".op-again");
    const text = await p.evaluate(() => document.querySelector(".op-state-line").textContent);
    assertEqual(text, "I can't read this thread's output record right now.");
    await p.evaluate(() => window.__RICHOS_MOCK__.outputUnreadable(false));
    await p.click(".op-again");
    await p.waitForFunction(() => document.querySelectorAll("#op-body .orow").length === 9);
    assertEqual((await panelState(p)).top.count, "9", "Try again did not bring the count back");
    await p.context().close();
    return "\"" + text + "\" with Try again; recovered to 9";
  });

  await run.check("no thread, no button: the company overview hides both and closes the panel", async () => {
    await page.click("#out-top");
    await page.waitForSelector("#outpanel .orow");
    await page.click(".nav-group-label.is-selectable");
    await page.waitForFunction(() => document.getElementById("out-top").hidden);
    const s = await panelState(page);
    assert(s.top.hidden && s.bottom.hidden, "an Output button is shown with no thread open");
    assert(!s.open, "the panel stayed open over a screen with no thread");
    return "both buttons hidden on the company overview; the panel closed with the conversation";
  });

  await run.check("below 1180px the panel overlays with a scrim, and the scrim closes it", async () => {
    const p = await openApp(browser, { viewport: { width: 1000, height: 800 } });
    await openThread(p, "acme", 9);
    await p.click("#out-top");
    await p.waitForSelector("#outpanel .orow");
    const r = await p.evaluate(() => ({
      position: getComputedStyle(document.getElementById("outpanel")).position,
      scrim: !document.getElementById("outpanel-scrim").hidden,
    }));
    assertEqual(r, { position: "fixed", scrim: true });
    await p.mouse.click(40, 400);
    await p.waitForFunction(() => document.getElementById("outpanel").hidden);
    await p.context().close();
    return "fixed over the conversation with a scrim at 1000px; a click on the scrim closed it";
  });

  // ---- S5: every kind in §7 shows inside the panel, or says why not (§12.5) ----------------
  //
  // On a page of its own, so nothing the S4 checks did (a missing brief.md) leaks in. Each check
  // opens one file from the list and reads what `#op-viewer` drew against what the bridge
  // answered for that id: every `src` in a viewer must be the URL the shell returned (the page
  // never builds one), and every facts line is the §6.4/§7 sentence for its kind.
  const s5 = await openApp(browser);
  await openThread(s5, "acme", 9);

  /// Open `name` from the list and wait for its viewer to be drawn; what the shell answered.
  async function openPreview(p, name) {
    if (!(await p.evaluate(() => window.RichOutput.isOpen()))) await p.click("#out-top");
    if ((await p.evaluate(() => document.getElementById("op-body").dataset.view)) === "file") await p.click("#of-back");
    await p.waitForSelector("#outpanel .orow");
    const id = byName(name).id;
    await p.click('.orow[data-output="' + id + '"]');
    await p.waitForFunction((n) => document.getElementById("op-title").textContent === n && !!document.querySelector("#op-viewer[data-preview]"), name);
    return p.evaluate((i) => window.RichBridge.invoke("output_preview", { outputId: i }), id);
  }

  const viewer = (p) =>
    p.evaluate(() => {
      const v = document.getElementById("op-viewer");
      const facts = v.querySelector(".of-meta");
      return {
        view: v.dataset.preview,
        why: v.dataset.why || null,
        facts: facts ? facts.textContent : null,
        factsBold: facts && facts.querySelector("b") ? facts.querySelector("b").textContent : null,
        none: v.querySelector(".of-none") ? v.querySelector(".of-none").textContent : null,
        note: v.querySelector(".of-note") ? v.querySelector(".of-note").textContent : null,
        srcs: [...v.querySelectorAll("[src]")].map((n) => n.tagName.toLowerCase() + " " + n.getAttribute("src")),
        anchors: v.querySelectorAll("a[href]").length,
      };
    });

  await run.check("file-md — a Markdown file is rendered in the panel: its heading, table, list and quotation; the facts count its words", async () => {
    await openPreview(s5, "comps-summary.md");
    const r = await s5.evaluate(() => {
      const md = document.querySelector("#op-viewer .of-md");
      return {
        h1: md.querySelector('[role="heading"]').textContent,
        h1Size: parseFloat(getComputedStyle(md.querySelector('[role="heading"]')).fontSize),
        rows: [...md.querySelectorAll("table tr")].map((tr) => [...tr.children].map((c) => c.textContent)),
        items: [...md.querySelectorAll("li")].length,
        quote: md.querySelector("blockquote") ? md.querySelector("blockquote").textContent : null,
        strong: [...md.querySelectorAll("strong")].map((n) => n.textContent),
        text: md.textContent,
        bodySize: parseFloat(getComputedStyle(md).fontSize),
        cellSize: parseFloat(getComputedStyle(md.querySelector("td")).fontSize),
        pressed: [...document.querySelectorAll(".of-seg button")].map((b) => b.textContent + "=" + b.getAttribute("aria-pressed")),
      };
    });
    const v = await viewer(s5);
    assertEqual(v.view, "text");
    assertEqual(r.h1, "Acme — comparables, 2026-10-04");
    assertEqual(r.rows.length, 4, "the comparables table: a header and three deals");
    assertEqual(r.rows[1], ["Northwind renewal", "Aug 14", "$1.9M", "−3%"]);
    assertEqual(r.items, 3, "the three bullets under 'What it means'");
    assertEqual(r.quote, "Source: the deal ledger, femcboost/deals/2026.xlsx, rows 14–41.");
    assertEqual(r.strong, ["above Acme's counter"]);
    assert(!/[#|]|\*\*/.test(r.text), "Markdown syntax is still on screen: " + r.text.slice(0, 120));
    assert(r.bodySize >= 16 && r.cellSize >= 16, "§6.9: everything readable is 16px or larger: body " + r.bodySize + ", cell " + r.cellSize);
    assertEqual(r.pressed, ["Preview=true", "Source=false"]);
    assert(/^\d+ words·Markdown$/.test(v.facts), "the facts line: " + v.facts);
    assertEqual(v.anchors, 0, "nothing in a preview navigates out of the panel (§7)");
    return r.h1 + " at " + r.h1Size + "px; table 4 rows; 3 items; quotation; facts \"" + v.facts + "\"";
  });

  await run.check("file-source — Preview | Source is one switch: Source is the file as written, and it stays chosen across files", async () => {
    // From the keyboard, so focus is the switch's own (WebKit does not focus a button on click).
    await s5.focus('[data-seg="source"]');
    await s5.keyboard.press("Enter");
    await s5.waitForSelector("#op-viewer pre.of-src");
    const source = await s5.evaluate(() => ({
      pre: document.querySelector("#op-viewer pre.of-src") ? document.querySelector("#op-viewer pre.of-src").textContent : null,
      md: !!document.querySelector("#op-viewer .of-md"),
      pressed: [...document.querySelectorAll(".of-seg button")].map((b) => b.getAttribute("aria-pressed")),
      focus: document.activeElement.dataset.seg || document.activeElement.className,
      font: getComputedStyle(document.querySelector("#op-viewer pre.of-src")).fontFamily,
      facts: document.querySelector("#op-viewer .of-meta").textContent,
    }));
    assert(source.pre && source.pre.startsWith("# Acme — comparables, 2026-10-04\n\nTwo deals"), "Source is not the file as written: " + JSON.stringify(source.pre && source.pre.slice(0, 60)));
    assert(source.pre.indexOf("|---|---|---:|---:|") !== -1, "the table's own syntax is in the source");
    assert(!source.md, "the rendered view is still on screen beside the source");
    assertEqual(source.pressed, ["false", "true"]);
    assertEqual(source.focus, "source", "the switch kept focus where it was pressed");
    assert(/mono/i.test(source.font), "Source is drawn in the app's monospace: " + source.font);
    // The words the facts count are the words of the file as written.
    const words = source.pre.match(/\S+/g).length;
    assertEqual(source.facts, words + " words·Markdown", "the facts count the file's words");
    // Chosen once, it holds for the next Markdown file, as round 17 holds it.
    await openPreview(s5, "brief.md");
    const next = await s5.evaluate(() => ({
      src: !!document.querySelector("#op-viewer pre.of-src"),
      pressed: [...document.querySelectorAll(".of-seg button")].map((b) => b.getAttribute("aria-pressed")),
    }));
    assertEqual(next, { src: true, pressed: ["false", "true"] }, "brief.md did not open in Source");
    await s5.click('[data-seg="preview"]');
    assert(await s5.evaluate(() => !!document.querySelector("#op-viewer .of-md")), "Preview did not come back");
    // Not offered where it means nothing: a CSV has no Source switch.
    await openPreview(s5, "q3-revenue.csv");
    assertEqual(await s5.evaluate(() => document.querySelectorAll(".of-seg").length), 0, "a CSV offers Preview | Source");
    return "Source = the file verbatim in monospace (" + words + " words), focus kept, held for brief.md, and back to Preview";
  });

  await run.check("file-table — a CSV is a real table: its first rows, number columns to the right, and where the whole sheet opens", async () => {
    await openPreview(s5, "q3-revenue.csv");
    const t = await s5.evaluate(() => {
      const tb = document.querySelector("#op-viewer table.of-tbl");
      return {
        head: [...tb.querySelectorAll("th")].map((c) => c.textContent),
        rows: [...tb.querySelectorAll("tbody tr")].map((tr) => [...tr.children].map((c) => c.textContent)),
        right: [...tb.querySelectorAll("th")].map((c) => getComputedStyle(c).textAlign),
        region: document.querySelector("#op-viewer .of-tbl-wrap").getAttribute("aria-label"),
      };
    });
    const v = await viewer(s5);
    assertEqual(t.head, ["Month", "Plan", "Actual", "Variance", "Note"]);
    assertEqual(t.rows.length, 4);
    assertEqual(t.rows[3], ["Q3 total", "$1,950,000", "$2,067,000", "+6.0%", ""]);
    assertEqual(t.right, ["left", "right", "right", "right", "left"], "money and percentages line up on the right");
    assertEqual(v.facts, "4 rows·Comma-separated·The whole sheet opens in Numbers");
    assertEqual(v.note, null, "every row is shown, so nothing says otherwise");
    // 200 rows shown of more, from the shell's own cap: said under the table.
    await s5.evaluate(() => {
      const rows = [["n", "value"]];
      for (let i = 0; i < 200; i += 1) rows.push(["r" + i, String(i)]);
      window.__RICHOS_MOCK__.outputPreviewAs("q3-revenue.csv", { view: "table", rows, totalRows: 251, countedAll: false, bytes: 21000000, app: "Numbers" });
    });
    await openPreview(s5, "q3-revenue-chart.png");
    await openPreview(s5, "q3-revenue.csv");
    const capped = await viewer(s5);
    await s5.evaluate(() => window.__RICHOS_MOCK__.outputPreviewAs("q3-revenue.csv", null));
    assertEqual(capped.note, "Showing the first 200 rows");
    assertEqual(capped.facts, "More than 250 rows·Comma-separated·The whole sheet opens in Numbers");
    return t.head.join(" | ") + "; " + v.facts + "; over the cap: \"" + capped.note + "\"";
  });

  await run.check("file-image — a picture at the panel's width with its size and format; the src is the shell's URL", async () => {
    const answer = await openPreview(s5, "q3-revenue-chart.png");
    await s5.waitForFunction(() => {
      const img = document.querySelector("#op-viewer .of-img img");
      return img && img.complete && img.naturalWidth > 0;
    });
    const r = await s5.evaluate(() => {
      const img = document.querySelector("#op-viewer .of-img img");
      return { alt: img.alt, width: Math.round(img.getBoundingClientRect().width), box: Math.round(document.getElementById("op-viewer").clientWidth) };
    });
    const v = await viewer(s5);
    assertEqual(v.srcs, ["img " + answer.url], "the page drew a URL the shell did not give it");
    assertEqual(r.alt, "q3-revenue-chart.png");
    assertEqual(v.facts, "1280 × 800·PNG·96 KB");
    assert(r.width > 300 && r.width <= 980, "the picture is not at the panel's width: " + JSON.stringify(r));
    return "the chart at " + r.width + "px; facts \"" + v.facts + "\"";
  });

  await run.check("file-pdf — a PDF in WebKit's own viewer, from the shell's URL", async () => {
    const answer = await openPreview(s5, "term-sheet-march.pdf");
    const v = await viewer(s5);
    const r = await s5.evaluate(() => {
      const f = document.querySelector("#op-viewer iframe.of-pdf");
      return { title: f.title, h: Math.round(f.getBoundingClientRect().height) };
    });
    assertEqual(v.view, "pdf");
    assertEqual(v.srcs, ["iframe " + answer.url]);
    assertEqual(r.title, "term-sheet-march.pdf", "the frame is named for the file");
    assert(r.h >= 420, "the PDF frame is too short to read a page: " + r.h);
    assertEqual(v.facts, "PDF·212 KB");
    return "iframe " + r.h + "px tall; facts \"" + v.facts + "\"";
  });

  await run.check("file-video — the native player on the real file, its length and frame size from the file's header", async () => {
    const answer = await openPreview(s5, "comps-walkthrough.mp4");
    const v = await viewer(s5);
    const r = await s5.evaluate(() => {
      const el = document.querySelector("#op-viewer video");
      return { controls: el.controls, label: el.getAttribute("aria-label"), preload: el.preload };
    });
    assertEqual(v.srcs, ["video " + answer.url]);
    assertEqual(r, { controls: true, label: "comps-walkthrough.mp4", preload: "metadata" });
    assertEqual(v.facts, "0:31·1920 × 1080·14.2 MB");
    // A recording: the native audio player, its length when the header gave one.
    await s5.evaluate((url) => window.__RICHOS_MOCK__.outputPreviewAs("comps-walkthrough.mp4", { view: "audio", url, bytes: 14200000, durationMs: 130000, app: "Music" }), answer.url);
    await openPreview(s5, "term-sheet-march.pdf");
    await openPreview(s5, "comps-walkthrough.mp4");
    const audio = await viewer(s5);
    await s5.evaluate(() => window.__RICHOS_MOCK__.outputPreviewAs("comps-walkthrough.mp4", null));
    assertEqual(audio.srcs, ["audio " + answer.url]);
    assertEqual(audio.facts, "2:10·Audio·14.2 MB");
    return "video controls, \"" + v.facts + "\"; audio \"" + audio.facts + "\"";
  });

  await run.check("a Word document is QuickLook's first page on its own paper, and says where the whole document opens", async () => {
    const answer = await openPreview(s5, "counter-draft-v1.docx");
    const v = await viewer(s5);
    const paper = await s5.evaluate(() => getComputedStyle(document.querySelector("#op-viewer .paper.of-rendition")).backgroundColor);
    assertEqual(v.view, "rendition");
    assertEqual(v.srcs, ["img " + answer.url]);
    assertEqual(v.facts, "Word document·18 KB·The whole document opens in Pages");
    assertEqual(paper, "rgb(253, 252, 248)", "the page is the file's own white (§6.9)");
    return "\"" + v.facts + "\" on #FDFCF8";
  });

  // The §6.7 states that are the shell's own sentences, verbatim from `output_files.rs`.
  const STATES = [
    ["too large", "q3-revenue-chart.png", { view: "none", why: "tooLarge", reason: "Too large to preview here (1.4 GB). Open in Preview has the whole thing." }],
    ["no preview", "comps-2026-10-04.xlsx", { view: "none", why: "noViewer", reason: "I don't have a preview for this kind of file. Open in Numbers has it." }],
    ["read failed", "brief.md", { view: "none", why: "readFailed", reason: "I couldn't read this file: permission denied." }],
    ["a link", "counter-draft-v1.md", { view: "none", why: "refused", reason: "This file is a link to somewhere else, so I won't open it from here. Show in Finder still works." }],
  ];
  for (const [state, name, answer] of STATES) {
    await run.check(state + " — the viewer says the shell's own sentence, and draws nothing else", async () => {
      await s5.evaluate(({ n, a }) => window.__RICHOS_MOCK__.outputPreviewAs(n, a), { n: name, a: answer });
      await openPreview(s5, name);
      const v = await viewer(s5);
      await s5.evaluate((n) => window.__RICHOS_MOCK__.outputPreviewAs(n, null), name);
      assertEqual(v.view, "none");
      assertEqual(v.why, answer.why);
      assertEqual(v.none, answer.reason);
      assertEqual(v.srcs, [], "a viewer was drawn under the sentence");
      const seg = await s5.evaluate(() => document.querySelectorAll(".of-seg").length);
      return name + ": \"" + v.none + "\"" + (seg ? " (Preview | Source still offered for the kind)" : "");
    });
  }

  await run.check("a picture the webview cannot draw says so, and names where it opens", async () => {
    await s5.evaluate(() =>
      window.__RICHOS_MOCK__.outputPreviewAs("q3-revenue-chart.png", { view: "image", url: "data:image/png;base64,AAAA", width: null, height: null, bytes: 96000, app: "Preview" })
    );
    await openPreview(s5, "q3-revenue-chart.png");
    await s5.waitForSelector("#op-viewer .of-none");
    const v = await viewer(s5);
    await s5.evaluate(() => window.__RICHOS_MOCK__.outputPreviewAs("q3-revenue-chart.png", null));
    assertEqual(v.none, "I couldn't show this file here. Open in Preview has it.");
    assertEqual(v.facts, "PNG·96 KB", "the facts stay, with no size the header did not give");
    return "\"" + v.none + "\"";
  });

  await run.check("loading — nothing for 150 ms, then 'Reading…', never a spinner; a late answer for a file no longer shown is dropped", async () => {
    await s5.evaluate(() => {
      const real = window.RichBridge.invoke.bind(window.RichBridge);
      window.__releasePreview = [];
      window.RichBridge.invoke = (cmd, args) =>
        cmd === "output_preview" ? new Promise((done) => window.__releasePreview.push(() => done(real(cmd, args)))) : real(cmd, args);
      window.__restoreInvoke = () => {
        window.RichBridge.invoke = real;
      };
    });
    if ((await s5.evaluate(() => document.getElementById("op-body").dataset.view)) === "file") await s5.click("#of-back");
    await s5.click('.orow[data-output="' + byName("brief.md").id + '"]');
    const early = await s5.evaluate(() => document.getElementById("op-viewer").textContent);
    await s5.waitForSelector("#op-viewer .of-reading");
    const r = await s5.evaluate(() => ({
      text: document.querySelector("#op-viewer .of-reading").textContent,
      bar: !!document.getElementById("of-back") && !!document.querySelector(".of-path"),
      spinners: document.querySelectorAll('#op-viewer [role="progressbar"], #op-viewer .spinner').length,
    }));
    // Step to the next file while the first answer is still out; then let both answers come.
    await s5.click('[aria-label="Next file"]');
    await s5.evaluate(() => window.__releasePreview.splice(0).forEach((go) => go()));
    await s5.waitForSelector("#op-viewer[data-preview]");
    const after = await s5.evaluate(() => ({ title: document.getElementById("op-title").textContent, md: document.querySelectorAll("#op-viewer .of-md, #op-viewer pre").length }));
    await s5.evaluate(() => window.__restoreInvoke());
    assertEqual(early, "", "something was said before 150 ms");
    assertEqual(r.text, "Reading…");
    assert(r.bar, "the bar and the path wait for the preview instead of painting at once (§6.7)");
    assertEqual(r.spinners, 0);
    assertEqual(after.title, "comps-2026-10-04.xlsx", "the step did not land on the next file");
    assertEqual(after.md, 0, "brief.md's late answer was drawn into the next file's view");
    return "'' at once, '" + r.text + "' after 150 ms, the bar and path at once; brief.md's late answer dropped";
  });

  await run.check("file content never becomes markup: a hostile Markdown file and its link are text", async () => {
    await s5.evaluate(() =>
      window.__RICHOS_MOCK__.outputPreviewAs("brief.md", {
        view: "text",
        truncated: true,
        bytes: 3000000,
        text: "# <img src=x onerror=alert(1)>\n\n| <script>alert(2)</script> |\n|---|\n| [go](javascript:alert(3)) |\n",
      })
    );
    await openPreview(s5, "brief.md");
    const r = await s5.evaluate(() => {
      const v = document.getElementById("op-viewer");
      return { img: v.querySelectorAll("img").length, script: v.querySelectorAll("script").length, a: v.querySelectorAll("a").length, text: v.textContent };
    });
    const v = await viewer(s5);
    await s5.evaluate(() => window.__RICHOS_MOCK__.outputPreviewAs("brief.md", null));
    assertEqual([r.img, r.script, r.a], [0, 0, 0]);
    assert(r.text.indexOf("<img src=x onerror=alert(1)>") !== -1 && r.text.indexOf("<script>alert(2)</script>") !== -1, "the characters must be on screen");
    assertEqual(v.note, "Showing the first 2 MB", "a capped text says so, under it (§6.9: a cap, not a page)");
    assertEqual(s5.__errors, [], "something executed");
    return "0 img, 0 script, 0 a; the link is its word; 'Showing the first 2 MB'";
  });

  await run.check("no page errors in the preview walk", async () => {
    assertEqual(s5.__errors, [], "the page reported errors");
    await s5.context().close();
    return "0 errors";
  });

  // ---- S6: the actions (PRD §12.6) ------------------------------------------------------------
  //
  // round 17's `menu` and `open-menu` states, the arrow keys and Escape in menus, every action's
  // notice sentence, and the disabled actions on a missing file and a link. A page of its own,
  // so nothing above changes what these see. Nothing is opened and nothing written: the mock's
  // `output_*` commands answer with the shell's own sentences and keep every call, which is how
  // these checks also prove the page never sends a path (§5.1).

  const S6_HOME = "/Users/you/FemcBoost/";
  const ap = await openApp(browser);
  await ap.evaluate(() => {
    // The clipboard, observed: Copy path is the one action with no shell command (§5.4).
    window.__copied = [];
    Object.defineProperty(navigator, "clipboard", {
      configurable: true,
      value: { writeText: (t) => (window.__copied.push(t), Promise.resolve()) },
    });
  });
  await openThread(ap, "acme", 9);
  await ap.click("#out-top");
  await ap.waitForSelector("#outpanel .orow");
  const rowOf = (name) => '.orow[data-output="' + byName(name).id + '"]';
  const menuState = () =>
    ap.evaluate(() => {
      const m = document.getElementById("op-menu");
      if (!m) return null;
      const a = document.activeElement;
      return {
        kind: m.dataset.kind,
        role: m.getAttribute("role"),
        head: m.querySelector(".op-menu-head") ? m.querySelector(".op-menu-head").textContent : null,
        items: [...m.querySelectorAll(".op-menu-item")].map((b) => b.textContent + (b.getAttribute("aria-disabled") === "true" ? " (off)" : "")),
        roles: [...m.querySelectorAll(".op-menu-item")].every((b) => b.getAttribute("role") === "menuitem"),
        seps: m.querySelectorAll('[role="separator"]').length,
        titles: Object.fromEntries([...m.querySelectorAll(".op-menu-item")].map((b) => [b.textContent, b.title])),
        focus: a && m.contains(a) ? a.textContent : null,
      };
    });
  const pick = (label) =>
    ap.evaluate((l) => {
      const b = [...document.querySelectorAll("#op-menu .op-menu-item")].find((x) => x.textContent === l);
      if (!b) throw new Error("no menu item " + JSON.stringify(l));
      b.click();
    }, label);
  const noticeSays = async (text) => {
    await ap.waitForFunction((t) => {
      const n = document.getElementById("op-notice");
      return n && !n.hidden && n.textContent === t;
    }, text);
    return ap.evaluate(() => {
      const n = document.getElementById("op-notice");
      return { role: n.getAttribute("role"), live: n.getAttribute("aria-live") };
    });
  };
  const calls = () => ap.evaluate(() => window.__RICHOS_MOCK__.outputCalls().filter((c) => c.cmd !== "output_file"));
  const openRowMenu = async (name) => {
    await ap.hover(rowOf(name));
    await ap.click(rowOf(name) + ' + .oacts [data-act="menu"]');
    await ap.waitForSelector("#op-menu");
  };

  await run.check("menu — a row's ⋯ opens round 17's set, named for the file, focus on its first item", async () => {
    await openRowMenu("brief.md");
    const m = await menuState();
    assertEqual(m.kind, "row");
    assertEqual(m.role, "menu");
    assert(m.roles, "every item is a menuitem");
    assertEqual(m.head, "brief.md");
    assertEqual(m.items, ["Preview", "Open in Obsidian", "Open with…", "Show in Finder", "Save a copy…", "Copy path", "Add to chat"]);
    // Round 17's two rules: before Show in Finder, and before Add to chat (S7, last).
    assertEqual(m.seps, 2);
    assertEqual(m.focus, "Preview", "focus did not move into the menu");
    const opener = await ap.evaluate((sel) => {
      const b = document.querySelector(sel + ' + .oacts [data-act="menu"]');
      return { expanded: b.getAttribute("aria-expanded"), haspopup: b.getAttribute("aria-haspopup"), label: b.getAttribute("aria-label"), rowOpen: b.closest(".orow-wrap").querySelector(".orow").classList.contains("menu-open") };
    }, rowOf("brief.md"));
    assertEqual(opener, { expanded: "true", haspopup: "menu", label: "More actions for brief.md", rowOpen: true });
    // A role=button row's children are presentational: an action nested inside one reaches no
    // assistive technology (the VM walk's accessibility tree showed the row as one button).
    const nested = await ap.evaluate(() => document.querySelectorAll('.orow[role="button"] [data-act]').length);
    assertEqual(nested, 0, "an action is nested inside a role=button row, where VoiceOver cannot reach it");
    return m.items.join(" · ");
  });

  await run.check("arrow keys walk the menu; → opens Open with…, ← and Escape step back one level, then to the ⋯", async () => {
    const focus = async () => (await menuState()).focus;
    await ap.keyboard.press("ArrowDown");
    assertEqual(await focus(), "Open in Obsidian");
    await ap.keyboard.press("End");
    assertEqual(await focus(), "Add to chat");
    await ap.keyboard.press("ArrowDown");
    assertEqual(await focus(), "Preview", "ArrowDown does not wrap");
    await ap.keyboard.press("ArrowUp");
    assertEqual(await focus(), "Add to chat", "ArrowUp does not wrap");
    await ap.keyboard.press("Home");
    await ap.keyboard.press("ArrowDown");
    await ap.keyboard.press("ArrowDown");
    assertEqual(await focus(), "Open with…");
    await ap.keyboard.press("ArrowRight");
    let m = await menuState();
    assertEqual([m.kind, m.head, m.items, m.focus], ["open-with", "Open brief.md with", ["TextEdit", "Visual Studio Code"], "TextEdit"]);
    await ap.keyboard.press("ArrowLeft");
    m = await menuState();
    assertEqual([m.kind, m.focus], ["row", "Open with…"], "← did not go back to the menu at Open with…");
    await ap.keyboard.press("Enter");
    assertEqual((await menuState()).kind, "open-with", "Enter on Open with… did not open its list");
    await ap.keyboard.press("Escape");
    m = await menuState();
    assertEqual([m.kind, m.focus], ["row", "Open with…"], "Escape in Open with… must step back to the menu");
    await ap.keyboard.press("Escape");
    const after = await ap.evaluate((sel) => ({
      menu: !!document.getElementById("op-menu"),
      focus: document.activeElement.getAttribute("aria-label"),
      expanded: document.querySelector(sel + ' + .oacts [data-act="menu"]').getAttribute("aria-expanded"),
      panel: !document.getElementById("outpanel").hidden,
      view: document.getElementById("op-body").dataset.view,
    }), rowOf("brief.md"));
    assertEqual(after, { menu: false, focus: "More actions for brief.md", expanded: "false", panel: true, view: "list" }, "Escape closed more than the menu, or lost focus");
    // Escape reaching the shell's own rule (focus outside the menu) steps the menu first too.
    await openRowMenu("brief.md");
    await ap.evaluate(() => document.getElementById("op-body").focus());
    await ap.keyboard.press("Escape");
    assert(!(await menuState()) && (await panelState(ap)).open, "the shell's Escape closed the panel before the menu");
    // A press anywhere else closes it, and so does the ⋯ that opened it.
    await openRowMenu("brief.md");
    await ap.click("#op-title");
    assert(!(await menuState()), "a press outside did not close the menu");
    await openRowMenu("brief.md");
    await ap.click(rowOf("brief.md") + ' + .oacts [data-act="menu"]');
    assert(!(await menuState()), "the ⋯ did not close its own menu");
    return "↓ End ↓(wraps) ↑(wraps) → ← Enter Esc Esc; the shell's Escape, a press outside and the ⋯ each close it";
  });

  await run.check("every action says what it did — Open, Open with…, Show in Finder, Save a copy… (and Cancel), Copy path", async () => {
    const lines = [];
    const say = async (text) => {
      const n = await noticeSays(text);
      assertEqual(n, { role: "status", live: "polite" }, "the notice is not a polite status");
      lines.push(text);
    };
    await openRowMenu("brief.md");
    await pick("Open in Obsidian");
    await say("Opening brief.md in Obsidian.");
    await openRowMenu("brief.md");
    await pick("Open with…");
    await pick("Visual Studio Code");
    await say("Opening brief.md in Visual Studio Code.");
    await openRowMenu("brief.md");
    await pick("Show in Finder");
    await say("Finder opens acme/counter/ with brief.md selected.");
    await openRowMenu("brief.md");
    await pick("Save a copy…");
    await say("Saved a copy of brief.md to you/Desktop/.");
    await ap.evaluate(() => window.__RICHOS_MOCK__.outputSaveSheet("/Users/you/Documents/Board/brief for the board.md"));
    await openRowMenu("brief.md");
    await pick("Save a copy…");
    await say("Saved a copy of brief.md to Documents/Board/ as brief for the board.md.");
    await ap.evaluate(() => window.__RICHOS_MOCK__.outputSaveSheet(null));
    await openRowMenu("brief.md");
    await pick("Save a copy…");
    await say("Nothing was saved.");
    await openRowMenu("brief.md");
    await pick("Copy path");
    await say("Copied the path.");
    assertEqual(await ap.evaluate(() => window.__copied), [S6_HOME + "acme/counter/brief.md"], "the clipboard does not hold the recorded path");
    // The row's own Open, named for the app once the shell has said which.
    await ap.hover(rowOf("brief.md"));
    await ap.waitForFunction((sel) => document.querySelector(sel + ' + .oacts [data-act="open"]').title === "Open in Obsidian", rowOf("brief.md"));
    assertEqual(await ap.getAttribute(rowOf("brief.md") + ' + .oacts [data-act="open"]', "aria-label"), "Open brief.md in Obsidian");
    await ap.click(rowOf("brief.md") + ' + .oacts [data-act="open"]');
    await say("Opening brief.md in Obsidian.");
    // A right-click opens the same menu where the pointer is.
    const box = await ap.locator(rowOf("comps-summary.md")).boundingBox();
    await ap.mouse.click(box.x + 60, box.y + 10, { button: "right" });
    await ap.waitForSelector("#op-menu");
    const m = await menuState();
    const at = await ap.evaluate(() => document.getElementById("op-menu").getBoundingClientRect().left);
    assertEqual([m.kind, m.head], ["row", "comps-summary.md"]);
    assert(Math.abs(at - (box.x + 60)) <= 1, "the context menu is not at the pointer: " + at);
    await pick("Preview");
    assertEqual((await panelState(ap)).view, "file", "Preview did not show the file");
    assertEqual((await panelState(ap)).title, "comps-summary.md");
    // §5.1: no command was ever given a path, only the id and (for Open with) the app's place.
    const sent = await calls();
    const keys = [...new Set(sent.flatMap((c) => Object.keys(c.args)))].sort();
    assertEqual(keys, ["appIndex", "outputId"], "a command was sent something other than an id and an index");
    assertEqual(sent[1], { cmd: "output_open", args: { outputId: byName("brief.md").id, appIndex: 1 } }, "Open with… did not send the app's place");
    await ap.keyboard.press("Escape");
    return lines.join(" | ");
  });

  await run.check("open-menu — the file view's gold Open in <app>, its ▾ (the others, Finder, Save), its ⋯ and Copy the full path", async () => {
    await ap.click(rowOf("counter-draft-v1.docx"));
    await ap.waitForFunction(() => document.querySelector('.of-open [data-act="open"]') && document.querySelector('.of-open [data-act="open"]').textContent === "Open in Pages");
    const tools = await ap.evaluate(() => ({
      down: document.querySelector('[data-act="open-menu"]').getAttribute("aria-label"),
      more: document.querySelector('.of-tools [data-act="menu"]').getAttribute("aria-label"),
      copy: document.querySelector('.of-path [data-act="copy"]').getAttribute("aria-label"),
    }));
    assertEqual(tools, { down: "Other ways to open", more: "More actions", copy: "Copy the full path" });
    await ap.click('[data-act="open-menu"]');
    await ap.waitForSelector("#op-menu");
    let m = await menuState();
    assertEqual([m.kind, m.head, m.focus], ["open-menu", null, "Open in Pages"]);
    assertEqual(m.items, ["Open in Pages", "Open in Google Docs", "Open in Microsoft Word", "Show in Finder", "Save a copy…"]);
    await pick("Open in Microsoft Word");
    await noticeSays("Opening counter-draft-v1.docx in Microsoft Word.");
    await ap.click('.of-open [data-act="open"]');
    await noticeSays("Opening counter-draft-v1.docx in Pages.");
    await ap.click('.of-tools [data-act="menu"]');
    await ap.waitForSelector("#op-menu");
    m = await menuState();
    assertEqual([m.kind, m.items], ["file", ["Open in Pages", "Open with…", "Show in Finder", "Save a copy…", "Copy path", "Add to chat"]], "the file view's ⋯ is the row's set without Preview");
    await ap.keyboard.press("Escape");
    assertEqual(await ap.evaluate(() => document.activeElement.getAttribute("aria-label")), "More actions", "Escape did not return focus to the file view's ⋯");
    await ap.click('.of-path [data-act="copy"]');
    await noticeSays("Copied the path.");
    assertEqual((await ap.evaluate(() => window.__copied)).pop(), S6_HOME + "acme/counter/counter-draft-v1.docx");
    // The Mac's list changed under the page: the shell refuses and says to choose again; the
    // list is shown again, fresh, at the same control (ACTIONABLE: the control is right there).
    await ap.evaluate(() => window.__RICHOS_MOCK__.outputAppsChange());
    await ap.click('[data-act="open-menu"]');
    await ap.waitForSelector("#op-menu");
    await pick("Open in Google Docs");
    await noticeSays("The apps that open this file changed since the list was shown. Choose one again.");
    await ap.waitForFunction(() => document.getElementById("op-menu") && document.getElementById("op-menu").dataset.kind === "open-with");
    m = await menuState();
    assertEqual([m.items, m.focus], [["Microsoft Word", "Google Docs"], "Microsoft Word"], "the list was not shown again as the Mac gives it now");
    await pick("Google Docs");
    await noticeSays("Opening counter-draft-v1.docx in Google Docs.");
    return "▾: " + "Open in Pages · Open in Google Docs · Open in Microsoft Word · Show in Finder · Save a copy…; a changed list re-shown and chosen from";
  });

  await run.check("disabled actions on a missing file and a link: only what still works is lit, each refusal its reason", async () => {
    const MISSING = "This file is no longer where it was written. If it was moved, open it from its new place; if Rich writes it again, it will be listed here.";
    const LINKED = "This file is a link to somewhere else, so I won't open it from here. Show in Finder still works.";
    await ap.keyboard.press("Escape"); // the file view back to the list
    await ap.evaluate(() => window.__RICHOS_MOCK__.outputMissing("acme", "brief.md"));
    await ap.evaluate(() => window.RichOutput.reload());
    await ap.waitForSelector(".orow.is-missing");
    const before = (await calls()).length;
    const rowOpen = await ap.evaluate((sel) => {
      const b = document.querySelector(sel + ' + .oacts [data-act="open"]');
      return { disabled: b.getAttribute("aria-disabled"), title: b.title };
    }, rowOf("brief.md"));
    assertEqual(rowOpen, { disabled: "true", title: MISSING });
    await ap.click(rowOf("brief.md") + ' + .oacts [data-act="open"]', { force: true });
    await openRowMenu("brief.md");
    let m = await menuState();
    assertEqual(m.items, ["Preview", "Open (off)", "Show in Finder (off)", "Save a copy… (off)", "Copy path", "Add to chat (off)"]);
    assertEqual([m.titles["Open"], m.titles["Show in Finder"], m.titles["Save a copy…"], m.titles["Copy path"], m.titles["Add to chat"]], [MISSING, MISSING, MISSING, "", MISSING], "each disabled action's tooltip is its reason");
    assertEqual(m.focus, "Preview", "focus landed on a disabled item");
    await ap.keyboard.press("ArrowDown");
    assertEqual((await menuState()).focus, "Open", "the arrows skipped a disabled item; it stays reachable, its reason in the tooltip");
    await ap.keyboard.press("Enter");
    assert(await menuState(), "activating a disabled item closed the menu");
    assertEqual((await calls()).length, before, "a disabled action reached the shell");
    await pick("Copy path");
    await noticeSays("Copied the path.");
    // Its own view: the pill leaves gold, Open and ▾ are off with the reason, Copy is lit.
    await ap.click(rowOf("brief.md"));
    await ap.waitForSelector(".of-missing");
    const view = await ap.evaluate(() => ({
      pill: document.querySelector(".of-open").classList.contains("is-disabled"),
      open: document.querySelector('.of-open [data-act="open"]').getAttribute("aria-disabled"),
      down: document.querySelector('[data-act="open-menu"]').getAttribute("aria-disabled"),
      downTitle: document.querySelector('[data-act="open-menu"]').title,
      copy: document.querySelector('.of-path [data-act="copy"]').getAttribute("aria-disabled"),
    }));
    assertEqual(view, { pill: true, open: "true", down: "true", downTitle: MISSING, copy: null });
    await ap.keyboard.press("Escape");
    // A link: Show in Finder lit, the rest off with the link sentence (§6.7).
    await ap.evaluate(() => window.__RICHOS_MOCK__.outputLinked("term-sheet-march.pdf"));
    await openRowMenu("term-sheet-march.pdf");
    m = await menuState();
    assertEqual(m.items, ["Preview", "Open (off)", "Show in Finder", "Save a copy… (off)", "Copy path (off)", "Add to chat (off)"]);
    assertEqual([m.titles["Open"], m.titles["Copy path"], m.titles["Show in Finder"], m.titles["Add to chat"]], [LINKED, LINKED, "", LINKED]);
    await pick("Show in Finder");
    await noticeSays("Finder opens acme/reference/ with term-sheet-march.pdf selected.");
    // A file that went after its menu was drawn: the shell's sentence, and its row dims.
    await openRowMenu("comps-summary.md");
    await ap.evaluate(() => window.__RICHOS_MOCK__.outputMissing("acme", "comps-summary.md"));
    await pick("Open in Obsidian");
    await noticeSays(MISSING);
    await ap.waitForSelector(rowOf("comps-summary.md") + ".is-missing");
    return "missing: Preview and Copy path lit; link: Preview and Show in Finder lit; a vanished file's refusal dims its row";
  });

  for (const theme of ["dark", "light"]) {
    await run.check(theme + " — the actions' text and indicators clear WCAG AA, computed", async () => {
      const p = await openApp(browser, { theme });
      await openThread(p, "acme", 9);
      await p.evaluate(() => window.__RICHOS_MOCK__.outputMissing("acme", "brief.md"));
      await p.click("#out-top");
      await p.waitForSelector("#outpanel .orow");
      const row = '.orow[data-output="' + byName("q3-revenue.csv").id + '"]';
      await p.hover(row);
      await p.click(row + ' + .oacts [data-act="menu"]');
      await p.waitForSelector("#op-menu");
      await p.keyboard.press("ArrowDown"); // a focused item, ringed
      // Each pair is read as the browser resolved it: the foreground, its font size, and every
      // background from the node up to the first opaque one (composited in order below), so a
      // translucent hover or fill is measured on what is really under it.
      const PAIRS = () => {
        window.__s6pair = (what, kind, n, fg, from) => {
          const stack = [];
          for (let x = from || n; x; x = x.parentElement) {
            const bg = getComputedStyle(x).backgroundColor;
            if (bg && bg !== "rgba(0, 0, 0, 0)" && bg !== "transparent") {
              stack.push(bg);
              if (!/rgba\(.*,\s*0?\.\d+\)$/.test(bg)) break;
            }
          }
          return { what, kind, fg, stack, size: parseFloat(getComputedStyle(n).fontSize) };
        };
      };
      await p.evaluate(PAIRS);
      const menu = await p.evaluate(() => {
        const pair = window.__s6pair;
        const cs = (n) => getComputedStyle(n);
        const m = document.getElementById("op-menu");
        const item = m.querySelector(".op-menu-item:not(:focus)");
        const focused = m.querySelector(".op-menu-item:focus");
        const ring = cs(focused).boxShadow.match(/rgba?\([^)]*\)/);
        const rowOpen = document.querySelector('.orow-wrap:hover [data-act="open"]');
        return [
          pair("menu item", "text", item, cs(item).color),
          pair("menu head (the file's name)", "text", m.querySelector(".op-menu-head"), cs(m.querySelector(".op-menu-head")).color),
          pair("menu item icon", "indicator", item, cs(item.querySelector("svg")).color),
          pair("focused item's ring", "indicator", focused, ring ? ring[0] : ""),
          pair("row Open icon, hovered row", "indicator", rowOpen, cs(rowOpen).color, rowOpen.closest(".orow-wrap").querySelector(".orow")),
        ];
      });
      await p.keyboard.press("Escape");
      // A disabled item, on the missing file's menu.
      const miss = '.orow[data-output="' + byName("brief.md").id + '"]';
      await p.hover(miss);
      await p.click(miss + ' + .oacts [data-act="menu"]');
      await p.waitForSelector('#op-menu [aria-disabled="true"]');
      const off = await p.evaluate(() => {
        const b = document.querySelector('#op-menu [aria-disabled="true"]');
        return window.__s6pair("disabled menu item (still read)", "text", b, getComputedStyle(b).color, document.getElementById("op-menu"));
      });
      await p.keyboard.press("Escape");
      // The file view's tools, a notice, and the refused pill.
      await p.click(row);
      await p.waitForFunction(() => document.querySelector('.of-open [data-act="open"]').textContent.startsWith("Open in"));
      await p.click('.of-path [data-act="copy"]');
      await p.waitForSelector("#op-notice:not([hidden])");
      await p.evaluate(() => Promise.all(document.getElementById("op-notice").getAnimations().map((a) => a.finished)));
      const tools = await p.evaluate(() => {
        const pair = window.__s6pair;
        const cs = (n) => getComputedStyle(n);
        const panel = document.getElementById("outpanel");
        const pill = document.querySelector(".of-open");
        const word = pill.querySelector("button");
        const tool = document.querySelector(".of-tool");
        const copy = document.querySelector(".of-copy");
        const n = document.getElementById("op-notice");
        return [
          pair("gold Open's words", "text", word, cs(pill).color),
          pair("gold Open's edge on the panel", "indicator", pill, cs(pill).borderTopColor, panel),
          pair("⋯ tool's edge on the panel", "indicator", tool, cs(tool).borderTopColor, panel),
          pair("⋯ tool's glyph", "indicator", tool, cs(tool.querySelector("svg")).color),
          pair("copy button glyph", "indicator", copy, cs(copy).color),
          pair("notice words", "text", n, cs(n).color),
          pair("notice rule", "indicator", n, cs(n).borderLeftColor),
        ];
      });
      await p.keyboard.press("Escape");
      await p.click(miss);
      await p.waitForSelector(".of-open.is-disabled");
      const refused = await p.evaluate(() => {
        const pill = document.querySelector(".of-open");
        return [
          window.__s6pair("refused Open's words", "text", pill.querySelector("button"), getComputedStyle(pill).color),
          window.__s6pair("refused Open's edge on the panel", "indicator", pill, getComputedStyle(pill).borderTopColor, document.getElementById("outpanel")),
        ];
      });
      const lines = [];
      for (const r of [...menu, off, ...tools, ...refused]) {
        assert(r.stack.length, "no background under " + r.what);
        const layers = r.stack.map(parseCssColor).reverse();
        assert(layers.every(Boolean), "could not resolve the background of " + r.what + ": " + r.stack.join(" over "));
        let base = layers[0].a < 1 ? compositeOver(layers[0], parseCssColor(theme === "dark" ? "rgb(12, 19, 34)" : "rgb(234, 230, 221)")) : layers[0];
        for (const layer of layers.slice(1)) base = compositeOver(layer, base);
        const fgRaw = parseCssColor(r.fg);
        assert(fgRaw, "could not resolve the color of " + r.what + ": " + r.fg);
        const fg = fgRaw.a < 1 ? compositeOver(fgRaw, base) : fgRaw;
        const ratio = Math.round(contrastRatio(fg, base) * 100) / 100;
        const floor = r.kind === "indicator" ? 3 : 4.5;
        lines.push(r.what + " " + hex(fg) + " on " + hex(base) + " " + ratio + ":1");
        assert(ratio >= floor, theme + ": " + r.what + " is " + ratio + ":1 against a floor of " + floor + ":1 (" + hex(fg) + " on " + hex(base) + ")");
        if (r.kind === "text") assert(r.size >= 16, theme + ": " + r.what + " is " + r.size + "px; text meant to be read is 16px or larger");
      }
      assertEqual(p.__errors, [], "the page reported errors");
      await p.context().close();
      return lines.join(" | ");
    });
  }

  await run.check("S6: no page errors", async () => {
    assertEqual(ap.__errors, [], "the page reported errors");
    await ap.context().close();
    return "0 errors";
  });

  // ---- both themes: the computed pairs, and the committed pictures --------------------------

  const ratios = [];
  for (const theme of ["dark", "light"]) {
    await run.check(theme + " — every new text pair and indicator clears WCAG AA, computed", async () => {
      const p = await openApp(browser, { theme });
      await openThread(p, "acme", 9);
      // At rest first: the boundary that says "this is a button" before anything is pressed.
      const rest = await p.evaluate(() => ({
        what: "button border at rest, on the composer's ground",
        kind: "indicator",
        fg: getComputedStyle(document.getElementById("out-bottom")).borderTopColor,
      }));
      await p.click("#out-top");
      await p.waitForSelector("#outpanel .orow");
      await p.hover('.orow[data-output="' + byName("q3-revenue.csv").id + '"]');
      const raw = await p.evaluate(() => {
        // The first opaque-ish background up the tree; where the stage is transparent over the
        // speckled ground, the paper token itself, resolved by the browser through a probe.
        const probe = document.createElement("div");
        probe.style.background = "var(--paper)";
        document.body.appendChild(probe);
        const paper = getComputedStyle(probe).backgroundColor;
        probe.remove();
        function ground(node) {
          for (let n = node; n; n = n.parentElement) {
            const bg = getComputedStyle(n).backgroundColor;
            if (bg && bg !== "rgba(0, 0, 0, 0)" && bg !== "transparent") return bg;
          }
          return paper;
        }
        const pick = (sel) => document.querySelector(sel);
        const text = (what, sel) => {
          const n = pick(sel);
          return { what, kind: "text", fg: getComputedStyle(n).color, bg: ground(n), size: parseFloat(getComputedStyle(n).fontSize) };
        };
        const edge = (what, sel, prop, against) => {
          const n = pick(sel);
          return { what, kind: "indicator", fg: getComputedStyle(n)[prop], bg: against ? ground(pick(against)) : ground(n.parentElement) };
        };
        return [
          text("button word", "#out-top .out-word"),
          text("count numeral", "#out-top .out-count"),
          text("panel title", "#op-title"),
          text("panel subline", "#op-sub"),
          text("OUTPUT eyebrow (14px)", "#op-eyebrow"),
          text("group words", ".og-words"),
          text("group time", ".og-head time"),
          text("file name", ".oname"),
          text("folder", ".opath"),
          text("worker line, hovered row", ".orow:hover .oby"),
          text("folder, hovered row", ".orow:hover .opath"),
          text("kind tile (14px)", ".okind"),
          text("file link in the prose", ".tl-prose .tl-file-link span"),
          text("Wrote N files", ".tl-wrote"),
          text("Produced label", ".tl-produced-label"),
          { what: "", kind: "rest-ground", bg: ground(pick("#composer-zone")) },
          edge("pressed button border", "#out-top", "borderTopColor", "#stage-header"),
          edge("count border on the count's own fill", "#out-top .out-count", "borderTopColor", "#out-top"),
        ];
      });
      // The at-rest border, against the ground it sits on (read in the same page).
      const restGround = raw.find((r) => r.kind === "rest-ground");
      raw.splice(raw.indexOf(restGround), 1, Object.assign(rest, { bg: restGround.bg }));
      const lines = [];
      for (const r of raw) {
        const bg = parseCssColor(r.bg);
        const fgRaw = parseCssColor(r.fg);
        assert(bg && fgRaw, "could not resolve the colors of " + r.what + ": " + r.fg + " on " + r.bg);
        const base = bg.a < 1 ? compositeOver(bg, parseCssColor(theme === "dark" ? "rgb(12, 19, 34)" : "rgb(234, 230, 221)")) : bg;
        const fg = fgRaw.a < 1 ? compositeOver(fgRaw, base) : fgRaw;
        const ratio = Math.round(contrastRatio(fg, base) * 100) / 100;
        const floor = r.kind === "indicator" ? 3 : 4.5;
        lines.push(r.what + " " + hex(fg) + " on " + hex(base) + " " + ratio + ":1");
        ratios.push({ theme, what: r.what, ratio });
        if (r.what.startsWith("count border")) continue; // decorative: the fill is the indicator
        assert(ratio >= floor, theme + ": " + r.what + " is " + ratio + ":1 against a floor of " + floor + ":1 (" + hex(fg) + " on " + hex(base) + ")");
      }
      assertEqual(p.__errors, [], "the page reported errors");
      await p.context().close();
      return lines.join(" | ");
    });

    await run.check(theme + " — the committed pictures: open and empty", async () => {
      const p = await openApp(browser, { theme });
      await openThread(p, "acme", 9);
      await p.click("#out-top");
      await p.waitForSelector("#outpanel .orow");
      await awaitWorkerChipSettled(p);
      const open = await shot(p, "output-open-" + theme, { fullPage: false, parkPointer: true });
      publishShotFile(open.file, path.join(SHOTS, "output-open-" + theme + ".png"));
      await openThread(p, "hiring", 0);
      await p.waitForSelector(".op-empty");
      await awaitWorkerChipSettled(p);
      const empty = await shot(p, "output-empty-" + theme, { fullPage: false, parkPointer: true });
      publishShotFile(empty.file, path.join(SHOTS, "output-empty-" + theme + ".png"));
      assertEqual(p.__errors, [], "the page reported errors");
      await p.context().close();
      return "shots-output/output-open-" + theme + ".png, shots-output/output-empty-" + theme + ".png";
    });

    // S5: the previews' own ink, computed in this theme, and two committed pictures of them.
    await run.check(theme + " — S5's previews: every new text pair and indicator clears WCAG AA, computed", async () => {
      const p = await openApp(browser, { theme });
      await openThread(p, "acme", 9);
      const measure = (pairs) =>
        p.evaluate((list) => {
          const probe = document.createElement("div");
          probe.style.background = "var(--paper-rail)";
          document.body.appendChild(probe);
          const rail = getComputedStyle(probe).backgroundColor;
          probe.remove();
          function ground(node) {
            for (let n = node; n; n = n.parentElement) {
              const bg = getComputedStyle(n).backgroundColor;
              if (bg && bg !== "rgba(0, 0, 0, 0)" && bg !== "transparent") return bg;
            }
            return rail;
          }
          return list.map(([what, sel, kind, prop]) => {
            const n = document.querySelector(sel);
            if (!n) return { what, missing: sel };
            const cs = getComputedStyle(n);
            return {
              what,
              kind,
              fg: prop ? cs[prop] : cs.color,
              bg: kind === "indicator" ? ground(n.parentElement) : ground(n),
              size: parseFloat(cs.fontSize),
            };
          });
        }, pairs);
      const raw = [];
      await openPreview(p, "comps-summary.md");
      raw.push(
        ...(await measure([
          ["document body", "#op-viewer .of-md .tl-md-p", "text"],
          ["document title (serif)", '#op-viewer .of-md [role="heading"]', "text"],
          ["document table header", "#op-viewer .of-md th", "text"],
          ["document table cell", "#op-viewer .of-md td", "text"],
          ["quotation", "#op-viewer .of-md blockquote", "text"],
          ["quotation bar", "#op-viewer .of-md blockquote", "indicator", "borderLeftColor"],
          ["facts, first part", "#op-viewer .of-meta b", "text"],
          ["facts, the rest", "#op-viewer .of-meta .of-fact:last-child", "text"],
          ["Preview, pressed", '.of-seg [aria-pressed="true"]', "text"],
          ["Source, not pressed", '.of-seg [aria-pressed="false"]', "text"],
          ["Preview | Source boundary", ".of-seg", "indicator", "borderTopColor"],
        ]))
      );
      await p.click('[data-seg="source"]');
      raw.push(...(await measure([["source", "#op-viewer pre.of-src", "text"]])));
      await p.click('[data-seg="preview"]');
      await openPreview(p, "q3-revenue.csv");
      raw.push(
        ...(await measure([
          ["sheet header", "#op-viewer .of-tbl th", "text"],
          ["sheet cell", "#op-viewer .of-tbl td", "text"],
          ["sheet facts", "#op-viewer .of-meta .of-fact:last-child", "text"],
        ]))
      );
      await p.evaluate(() => window.__RICHOS_MOCK__.outputPreviewAs("q3-revenue-chart.png", { view: "none", why: "tooLarge", reason: "Too large to preview here (1.4 GB). Open in Preview has the whole thing." }));
      await openPreview(p, "q3-revenue-chart.png");
      raw.push(...(await measure([["the §6.7 sentence", "#op-viewer .of-none", "text"]])));
      await p.evaluate(() => window.__RICHOS_MOCK__.outputPreviewAs("q3-revenue-chart.png", { view: "text", text: "x", truncated: true, bytes: 3000000 }));
      await openPreview(p, "q3-revenue-chart.png");
      raw.push(...(await measure([["Showing the first 2 MB", "#op-viewer .of-note", "text"]])));
      await p.evaluate(() => window.__RICHOS_MOCK__.outputPreviewAs("q3-revenue-chart.png", null));
      const lines = [];
      for (const r of raw) {
        assert(!r.missing, "nothing on screen to measure for " + r.what + " (" + r.missing + ")");
        const bg = parseCssColor(r.bg);
        const fgRaw = parseCssColor(r.fg);
        assert(bg && fgRaw, "could not resolve the colors of " + r.what + ": " + r.fg + " on " + r.bg);
        const base = bg.a < 1 ? compositeOver(bg, parseCssColor(theme === "dark" ? "rgb(12, 19, 34)" : "rgb(234, 230, 221)")) : bg;
        const fg = fgRaw.a < 1 ? compositeOver(fgRaw, base) : fgRaw;
        const ratio = Math.round(contrastRatio(fg, base) * 100) / 100;
        const floor = r.kind === "indicator" ? 3 : 4.5;
        lines.push(r.what + " " + hex(fg) + " on " + hex(base) + " " + ratio + ":1" + (r.kind === "text" ? " @" + r.size + "px" : ""));
        assert(ratio >= floor, theme + ": " + r.what + " is " + ratio + ":1 against a floor of " + floor + ":1 (" + hex(fg) + " on " + hex(base) + ")");
        if (r.kind === "text") assert(r.size >= 16, theme + ": " + r.what + " is " + r.size + "px; text meant to be read is 16px or larger");
      }
      assertEqual(p.__errors, [], "the page reported errors");
      await p.context().close();
      return lines.join(" | ");
    });

    await run.check(theme + " — the committed pictures of the previews: file-md and file-table", async () => {
      const p = await openApp(browser, { theme });
      await openThread(p, "acme", 9);
      const settle = () =>
        p.evaluate(() => Promise.all(document.getElementById("outpanel").getAnimations({ subtree: true }).map((a) => a.finished)));
      await openPreview(p, "comps-summary.md");
      await settle();
      await awaitWorkerChipSettled(p);
      const md = await shot(p, "output-file-md-" + theme, { fullPage: false, parkPointer: true });
      publishShotFile(md.file, path.join(SHOTS, "output-file-md-" + theme + ".png"));
      await openPreview(p, "q3-revenue.csv");
      await settle();
      await awaitWorkerChipSettled(p);
      const table = await shot(p, "output-file-table-" + theme, { fullPage: false, parkPointer: true });
      publishShotFile(table.file, path.join(SHOTS, "output-file-table-" + theme + ".png"));
      assertEqual(p.__errors, [], "the page reported errors");
      await p.context().close();
      return "shots-output/output-file-md-" + theme + ".png, shots-output/output-file-table-" + theme + ".png";
    });
  }

  await run.check("⌘+ scales the panel with everything else (rem, §6.9)", async () => {
    const p = await openApp(browser);
    await openThread(p, "acme", 9);
    await p.click("#out-top");
    await p.waitForSelector("#outpanel .orow");
    const size = () =>
      p.evaluate(() => ({
        name: parseFloat(getComputedStyle(document.querySelector(".oname")).fontSize),
        title: parseFloat(getComputedStyle(document.getElementById("op-title")).fontSize),
        button: parseFloat(getComputedStyle(document.getElementById("out-top")).fontSize),
      }));
    const before = await size();
    await p.focus("#input");
    await p.keyboard.press("Meta+=");
    await p.waitForFunction((b) => parseFloat(getComputedStyle(document.querySelector(".oname")).fontSize) > b, before.name);
    const after = await size();
    assert(after.title > before.title && after.button > before.button, "the panel did not scale: " + JSON.stringify({ before, after }));
    await p.context().close();
    return "file name " + before.name + " → " + after.name + "px, title " + before.title + " → " + after.title + "px, button " + before.button + " → " + after.button + "px";
  });

  await run.check("reduced motion: no tick, no rise, no pulse — the change still happens", async () => {
    const p = await openApp(browser, { reducedMotion: "reduce" });
    await openThread(p, "acme", 9);
    await p.click("#out-top");
    await p.waitForSelector("#outpanel .orow");
    const anim = await p.evaluate(() => ({
      panel: getComputedStyle(document.getElementById("outpanel")).animationName,
      view: getComputedStyle(document.querySelector(".op-view")).animationName,
    }));
    assertEqual(anim, { panel: "none", view: "none" });
    await p.context().close();
    return "the panel and its view open without animation under prefers-reduced-motion";
  });

  // ---- S9: the wide pull — the stop, the snap, open completely, the floating composer ---------
  //
  // The CEO: "when I'm dragging up to here, then there's a stop, I'm feeling a stop. But if I keep
  // dragging … then eventually it snaps open completely the output sidebar." PRD §12.9 names the
  // pointer path (100px before the stop, the stop, 44px past, 144px past, 230px back), the keys
  // and the six round-17.1 states this block reproduces: `stop`, `snap`, `full`, `full-list`,
  // `everything` and `wide`. A REAL POINTER throughout: `page.mouse` down on the divider, moved,
  // up — the events WebKit delivers to a hand, not a call into the panel's code.
  await pullChecks(run, browser, byName);

  await run.check("no page errors in the main walk", async () => {
    assertEqual(page.__errors, [], "the page reported errors");
    return "0 errors";
  });

  await page.context().close();
  await browser.close();
  process.exitCode = run.report() > 0 ? 1 : 0;
}

// =============================================================================================
// S9 — THE WIDE PULL (PRD §9, §12.9; round 17.1)
// =============================================================================================

const STAGE_MIN = 360; // §9.1, re-stated here so the suite derives the stop independently
const SNAP_PAST = 120; // §9.3

function near(actual, expected, tolerance, what) {
  assert(
    Math.abs(actual - expected) <= tolerance,
    (what || "value") + ": " + actual + " is not within " + tolerance + " of " + expected
  );
}

/// The width of the inset spine in a computed `box-shadow` ("rgb(…) 2.1px 0px 0px 0px inset").
function spineWidth(shadow) {
  const m = /(-?[\d.]+)px\s+(-?[\d.]+)px\s+(-?[\d.]+)px\s+(-?[\d.]+)px\s+inset/.exec(shadow || "");
  return m ? Number(m[1]) : 0;
}

/// Everything the pull paints, read in one frame, beside what the panel believes.
async function pullRead(page) {
  return page.evaluate(() => {
    const $ = (id) => document.getElementById(id);
    const rz = $("op-resizer");
    const stage = $("stage");
    const p = window.RichOutput.pull();
    const app = $("app").getBoundingClientRect();
    const away = document.body.classList.contains("rail-closed");
    const pill = $("op-conv");
    return {
      full: p.full,
      split: p.split,
      max: p.max,
      settling: p.settling,
      appRight: app.right,
      appWidth: app.width,
      railWidth: away ? 0 : $("rail").getBoundingClientRect().width,
      panel: $("outpanel").getBoundingClientRect().width,
      open: !$("outpanel").hidden,
      dividerLeft: rz.getBoundingClientRect().left,
      stage: stage.getBoundingClientRect().width,
      stageOpacity: Number(getComputedStyle(stage).opacity),
      stageVisibility: getComputedStyle(stage).visibility,
      spine: getComputedStyle($("outpanel")).boxShadow,
      narrow: stage.classList.contains("narrow"),
      valuenow: rz.getAttribute("aria-valuenow"),
      valuemax: rz.getAttribute("aria-valuemax"),
      valuetext: rz.getAttribute("aria-valuetext"),
      pill: pill.hidden ? null : pill.getAttribute("aria-label"),
      pillText: pill.hidden ? null : pill.textContent,
      composerIn: $("composer-zone").parentElement.id,
      arming: document.body.classList.contains("snap-arming"),
      focus: document.activeElement ? document.activeElement.id : null,
    };
  });
}

/// The snap's 420 ms and every width/opacity transition on the panel and the conversation have
/// landed — a state the page asserts, never a sleep (PRD §10, T3's harness discipline).
async function pullSettled(page) {
  await page.waitForFunction(() => !window.RichOutput.pull().settling);
  await page.evaluate(() =>
    Promise.all(
      ["outpanel", "stage"].flatMap((id) => document.getElementById(id).getAnimations()).map((a) => a.finished)
    )
  );
}

async function openPanelPage(browser, opts) {
  const page = await openApp(browser, opts);
  await openThread(page, "acme", 9);
  await page.click("#out-top");
  await page.waitForSelector("#outpanel .orow");
  await page.evaluate(() => Promise.all(document.getElementById("outpanel").getAnimations().map((a) => a.finished)));
  return page;
}

/// What the (mocked) nav.rs holds for the split width, once it holds `want` — read through the
/// bridge, as `inspector.js` reads its divider's, polled because the write is debounced 150 ms.
async function persistedWidth(page, want) {
  let got = null;
  for (let i = 0; i < 50; i++) {
    got = await page.evaluate(() => window.RichBridge.invoke("nav_state").then((n) => n.output_width));
    if (Math.round(got) === want) return got;
    await page.waitForTimeout(100);
  }
  throw new Error("nav.rs's output_width is " + got + ", never " + want);
}

/// The divider, focused, and one key on it.
async function pullKey(page, key) {
  await page.focus("#op-resizer");
  await page.keyboard.press(key);
  await pullSettled(page);
  return pullRead(page);
}

async function pullChecks(run, browser, byName) {
  await run.check("stop and snap — free to the stop and held there; 44px past it arms; 144px past it opens completely; 230px back returns", async () => {
    const p = await openPanelPage(browser);
    let g = await pullRead(p);
    // The stop, derived from the DOM rather than read from the code under test.
    const stopWidth = Math.floor(g.appWidth - g.railWidth - STAGE_MIN);
    assertEqual(stopWidth, 780, "the stop at 1440px with the 300px rail: 1440 − 300 − 360");
    assertEqual(g.max, stopWidth, "the panel's stop");
    assertEqual(g.valuemax, String(stopWidth), "aria-valuemax is the stop");
    const stopX = g.appRight - stopWidth;
    const y = 460;
    await p.mouse.move(g.dividerLeft + 3, y);
    await p.mouse.down();
    // 100px before the stop: free, and the divider under the pointer.
    await p.mouse.move(stopX + 100, y, { steps: 12 });
    g = await pullRead(p);
    near(g.panel, stopWidth - 100, 1, "100px before the stop");
    near(g.dividerLeft, stopX + 100, 1, "the divider under the pointer");
    assert(!g.arming, "the pull armed before the stop");
    // At the stop: the conversation at its narrowest, and the divider says so.
    await p.mouse.move(stopX, y, { steps: 6 });
    g = await pullRead(p);
    near(g.panel, stopWidth, 1, "at the stop");
    near(g.stage, STAGE_MIN, 1, "the conversation at its narrowest");
    assertEqual(g.valuetext, stopWidth + " pixels, at the stop; pull on to open it completely");
    // 44px past: the divider HOLDS — the stop he feels — and the pull arms.
    await p.mouse.move(stopX - 44, y, { steps: 4 });
    g = await pullRead(p);
    near(g.panel, stopWidth, 1, "past the stop the panel holds");
    near(g.dividerLeft, stopX, 1, "past the stop the divider stays at the stop");
    assert(g.arming, "44px past the stop did not arm");
    const armed = 44 / SNAP_PAST;
    near(g.stageOpacity, 1 - armed * 0.62, 0.01, "the conversation dims with the pull");
    near(spineWidth(g.spine), 1 + armed * 3, 0.06, "the gold spine thickens with the pull");
    const arming = { opacity: g.stageOpacity, spine: spineWidth(g.spine) };
    // 144px past: SNAP_PAST is crossed and the panel opens completely.
    await p.mouse.move(stopX - 144, y, { steps: 10 });
    await pullSettled(p);
    g = await pullRead(p);
    assert(g.full, "144px past the stop did not open it completely");
    near(g.panel, g.appWidth - g.railWidth, 1, "open completely is the whole stage");
    near(g.stage, 0, 0.5, "the conversation at zero width");
    assertEqual(g.stageVisibility, "hidden", "the conversation is hidden, not removed");
    assertEqual(g.pill, "Show the conversation: Acme deal");
    assertEqual(g.pillText, "Acme deal");
    assertEqual(g.composerIn, "op-float", "the composer is not in the panel");
    assertEqual(g.valuetext, "Open completely");
    assert(!g.arming, "still arming after the snap");
    const full = g.panel;
    // 230px back, read in two halves. SNAP_PAST back from where it snapped, the conversation
    // returns at its narrowest and the divider is at the stop, under the pointer...
    await p.mouse.move(stopX, y, { steps: 12 });
    await pullSettled(p);
    g = await pullRead(p);
    assert(!g.full, "pulled back 120px from the snap, the conversation did not return");
    near(g.panel, stopWidth, 1, "the return is at the stop");
    near(g.dividerLeft, stopX, 4, "the divider is under the pointer at the stop");
    assertEqual(g.composerIn, "stage", "the composer did not go home");
    near(g.stageOpacity, 1, 0.001, "the conversation is not fully back");
    // ...and on to 230px: the divider follows the pointer and nothing arms.
    await p.mouse.move(stopX + 86, y, { steps: 9 });
    g = await pullRead(p);
    near(g.dividerLeft, stopX + 86, 1, "after the return the divider follows the pointer");
    assert(!g.arming && !g.full);
    await p.mouse.up();
    // Written when the hand lets go, as nav.rs is written (the mock clamps with its rule).
    await persistedWidth(p, stopWidth - 86);
    assertEqual(p.__errors, [], "the page reported errors");
    await p.context().close();
    return (
      "stop " + stopWidth + "px (stage 360); 44px past: opacity " + arming.opacity.toFixed(3) + ", spine " +
      arming.spine.toFixed(2) + "px; 144px past: open completely at " + Math.round(full) + "px; back at the stop, then " +
      (stopWidth - 86) + "px under the pointer, persisted"
    );
  });

  await run.check("keys — End opens completely, → returns to the stop, ← at the stop snaps, Home and a double-click go to 400px", async () => {
    const p = await openPanelPage(browser);
    const stop = (await pullRead(p)).max;
    let g = await pullKey(p, "End");
    assert(g.full, "End did not open it completely");
    g = await pullKey(p, "ArrowRight");
    assert(!g.full, "→ from open completely did not return");
    near(g.panel, stop, 1, "→ returns to the stop");
    g = await pullKey(p, "ArrowLeft");
    assert(g.full, "← at the stop did not snap");
    g = await pullKey(p, "Home");
    assert(!g.full, "Home from open completely did not return");
    near(g.panel, 400, 1, "Home is the default 400px");
    g = await pullKey(p, "ArrowLeft");
    near(g.panel, 424, 1, "← widens by 24px");
    g = await pullKey(p, "ArrowRight");
    near(g.panel, 400, 1, "→ narrows by 24px");
    g = await pullKey(p, "ArrowRight");
    g = await pullKey(p, "ArrowRight");
    g = await pullKey(p, "ArrowRight");
    g = await pullKey(p, "ArrowRight");
    near(g.panel, 320, 1, "never under 320px");
    const box = await p.locator("#op-resizer").boundingBox();
    await p.mouse.dblclick(box.x + 3, box.y + 300);
    await pullSettled(p);
    g = await pullRead(p);
    near(g.panel, 400, 1, "a double-click goes back to 400px");
    assertEqual(g.focus, "op-resizer");
    await p.context().close();
    return "End → full, → → stop (" + stop + "px), ← → full, Home → 400, ← 424, → floor 320, double-click 400";
  });

  await run.check("close brings the whole conversation back and clears open completely; reopening is at the split width", async () => {
    const p = await openPanelPage(browser);
    await pullKey(p, "Home");
    for (let i = 0; i < 6; i++) await pullKey(p, "ArrowLeft"); // 400 + 6 × 24 = 544
    let g = await pullKey(p, "End");
    assert(g.full);
    // ×
    await p.click("#op-close");
    g = await pullRead(p);
    assert(!g.open, "× did not close the panel");
    assert(!g.full, "closing left the panel open completely");
    assertEqual(g.composerIn, "stage");
    // Home again, the empty field is one line tall. Found on the real app (the S9 walk's close
    // picture): moved into a conversation still at zero width, it measured itself 111px tall.
    const home = await p.evaluate(() => ({
      field: Math.round(document.getElementById("input").getBoundingClientRect().height),
      send: Math.round(document.getElementById("send").getBoundingClientRect().height),
    }));
    assert(home.field <= home.send, "the field came home " + home.field + "px tall beside a " + home.send + "px Send");
    near(g.stage, g.appWidth - g.railWidth, 1, "the whole conversation is back");
    assertEqual(g.stageVisibility, "visible");
    near(g.stageOpacity, 1, 0.001);
    await p.click("#out-top");
    await p.waitForSelector("#outpanel .orow");
    await pullSettled(p);
    g = await pullRead(p);
    assert(!g.full, "reopened open completely");
    near(g.panel, 544, 1, "reopened at the split width it had");
    // Escape from the list, and ⌘⇧O, do the same from open completely.
    await pullKey(p, "End");
    await p.focus("#op-body .orow");
    await p.keyboard.press("Escape");
    g = await pullRead(p);
    assert(!g.open && !g.full, "Escape from the list did not close it: " + JSON.stringify({ open: g.open, full: g.full }));
    await p.click("#out-top");
    await p.waitForSelector("#outpanel .orow");
    await pullKey(p, "End");
    await p.keyboard.press("Meta+Shift+O");
    g = await pullRead(p);
    assert(!g.open && !g.full, "⌘⇧O did not close it");
    near(g.stage, g.appWidth - g.railWidth, 1);
    await p.context().close();
    return "× / Escape / ⌘⇧O from open completely: the conversation whole (" + Math.round(g.stage) + "px); reopened at 544px";
  });

  await run.check("the pill — ‹ Acme deal brings the conversation back at the stop and puts focus in its composer", async () => {
    const p = await openPanelPage(browser);
    await pullKey(p, "End");
    await p.click("#op-conv");
    await pullSettled(p);
    await p.waitForFunction(() => document.activeElement && document.activeElement.id === "input");
    const g = await pullRead(p);
    assert(!g.full);
    near(g.panel, g.max, 1, "the pill returns to the stop");
    near(g.stage, STAGE_MIN, 1);
    assertEqual(g.pill, null, "the pill is shown with the conversation back");
    // And the field, home after the conversation's slide back from zero, is one line tall.
    await p.waitForFunction(
      () => document.getElementById("input").getBoundingClientRect().height <= document.getElementById("send").getBoundingClientRect().height
    );
    await p.context().close();
    return "‹ Acme deal → the stop (" + Math.round(g.panel) + "px), the conversation at 360px, focus in the composer";
  });

  await run.check("sidebar away + open completely = the whole window; the sidebar back gives the room back", async () => {
    const p = await openPanelPage(browser);
    await pullKey(p, "End");
    await p.keyboard.press("Meta+Shift+S");
    await p.waitForFunction(() => document.body.classList.contains("rail-closed"));
    await pullSettled(p);
    let g = await pullRead(p);
    near(g.panel, g.appWidth, 1, "sidebar away and open completely is the window");
    assert(g.full);
    await p.keyboard.press("Meta+Shift+S");
    await p.waitForFunction(() => !document.body.classList.contains("rail-closed"));
    await pullSettled(p);
    g = await pullRead(p);
    near(g.panel, g.appWidth - g.railWidth, 1, "the sidebar back, open completely is the stage");
    // At the split: with the sidebar away the stop moves out to 1440 − 360.
    await pullKey(p, "ArrowRight");
    await p.keyboard.press("Meta+Shift+S");
    await p.waitForFunction(() => document.body.classList.contains("rail-closed"));
    await pullSettled(p);
    const before = (await pullRead(p)).max;
    assertEqual(before, Math.floor(g.appWidth - STAGE_MIN), "the stop with the sidebar away");
    g = await pullKey(p, "End");
    g = await pullKey(p, "ArrowRight");
    near(g.panel, before, 1, "→ returns to the wider stop");
    await p.keyboard.press("Meta+Shift+S");
    await p.waitForFunction(() => !document.body.classList.contains("rail-closed"));
    await pullSettled(p);
    g = await pullRead(p);
    near(g.panel, g.max, 1, "the sidebar back pulls the panel in to the new stop");
    near(g.stage, STAGE_MIN, 1, "and the conversation keeps its 360px");
    await p.context().close();
    return "1440px with the sidebar away; " + Math.round(g.appWidth - g.railWidth) + "px with it; stop " + before + " → " + g.max + "px";
  });

  await run.check("the floating composer is THE composer: a message sent from it lands in the thread; Escape clears its words first", async () => {
    const p = await openPanelPage(browser);
    await p.fill("#input", "half a thought");
    await pullKey(p, "End");
    let g = await pullRead(p);
    assertEqual(g.composerIn, "op-float");
    const draft = await p.evaluate(() => document.getElementById("input").value);
    assertEqual(draft, "half a thought", "the draft did not come along: one draft, two homes");
    const placed = await p.evaluate(() => {
      const f = document.getElementById("op-float").getBoundingClientRect();
      const panel = document.getElementById("outpanel").getBoundingClientRect();
      return { right: Math.round(panel.right - f.right), bottom: Math.round(panel.bottom - f.bottom), width: Math.round(f.width) };
    });
    assertEqual([placed.right, placed.bottom], [18, 18], "the composer floats at the panel's bottom right");
    assertEqual(placed.width, 460);
    // Escape with words in the floating field clears them, and only them.
    await p.focus("#input");
    await p.keyboard.press("Escape");
    g = await pullRead(p);
    assert(g.open && g.full, "Escape with words in the floating composer stepped the panel back");
    assertEqual(await p.evaluate(() => document.getElementById("input").value), "");
    // A send from there lands in the conversation behind.
    const before = await p.evaluate(() => document.querySelectorAll(".tl-user-bubble").length);
    await p.fill("#input", "add Tolliver to the comps and re-run the sheet");
    await p.keyboard.press("Enter");
    await p.waitForFunction((n) => document.querySelectorAll(".tl-user-bubble").length === n + 1, before);
    const sent = await p.evaluate(() => ({
      last: [...document.querySelectorAll(".tl-user-bubble")].pop().textContent,
      field: document.getElementById("input").value,
      home: document.getElementById("composer-zone").parentElement.id,
    }));
    assert(sent.last.includes("add Tolliver to the comps"), "the message is not in the thread: " + sent.last);
    assertEqual(sent.field, "");
    assertEqual(sent.home, "op-float", "sending moved the composer");
    // With the field empty, Escape steps the panel back as everywhere else (the list → closed).
    await p.focus("#input");
    await p.keyboard.press("Escape");
    g = await pullRead(p);
    assert(!g.open && !g.full, "Escape with an empty floating composer did not close the panel");
    assertEqual(g.composerIn, "stage");
    assertEqual(p.__errors, [], "the page reported errors");
    await p.context().close();
    return "draft carried in; Escape cleared the words; the message is in the thread; Escape again closed the panel";
  });

  await run.check("the conversation at its narrowest reflows, keeps everything, and still sends", async () => {
    const p = await openPanelPage(browser);
    await p.evaluate(() => window.RichOutput.setSplitWidth(10000)); // as nav.rs would restore a wide window's
    await pullSettled(p);
    await p.waitForFunction(() => document.getElementById("stage").classList.contains("narrow"));
    const g = await pullRead(p);
    near(g.stage, STAGE_MIN, 1);
    assert(g.narrow, "no `narrow` reflow at 360px of conversation");
    const r = await p.evaluate(() => {
      const stage = document.getElementById("stage").getBoundingClientRect();
      // A control the conversation shows must be inside it; the talk control may be withheld
      // when voice is unavailable on this fixture, and a control not shown cannot spill.
      const optional = { "talk-toggle": true };
      const inside = (id) => {
        const b = document.getElementById(id).getBoundingClientRect();
        if (!b.width) return !!optional[id];
        return b.left >= stage.left - 0.5 && b.right <= stage.right + 0.5;
      };
      const word = document.querySelector("#out-bottom .out-word");
      return {
        wordShown: getComputedStyle(word).display !== "none",
        topWordShown: getComputedStyle(document.querySelector("#out-top .out-word")).display !== "none",
        count: document.querySelector("#out-bottom .out-count").textContent,
        inside: ["rail-toggle", "out-top", "out-bottom", "send", "talk-toggle", "input"].filter((id) => !inside(id)),
        name: document.getElementById("out-bottom").getAttribute("aria-label"),
      };
    });
    assert(!r.wordShown, "the bottom button kept its word at 360px");
    assert(r.topWordShown, "the top button lost its word (only the bottom one drops it)");
    assertEqual(r.count, "9");
    assertEqual(r.name, "Output: 9 files from this thread", "its name is unchanged");
    assertEqual(r.inside, [], "controls spill out of the 360px conversation");
    const before = await p.evaluate(() => document.querySelectorAll(".tl-user-bubble").length);
    await p.fill("#input", "and the narrow one sends");
    await p.keyboard.press("Enter");
    await p.waitForFunction((n) => document.querySelectorAll(".tl-user-bubble").length === n + 1, before);
    await p.context().close();
    return "360px: the bottom button keeps icon and count, every control inside the column, a message sent";
  });

  await run.check("open completely is not persisted across a reload; the split width is", async () => {
    const p = await openPanelPage(browser);
    await pullKey(p, "Home");
    for (let i = 0; i < 5; i++) await pullKey(p, "ArrowLeft"); // 520
    await persistedWidth(p, 520);
    await pullKey(p, "End");
    await p.reload();
    await leaveHome(p);
    await p.waitForFunction(() => typeof window.RichOutput === "object" && typeof window.RichOutput.pull === "function");
    await p.waitForSelector(".nav-thread", { state: "attached" });
    await openThread(p, "acme", 9);
    await p.click("#out-top");
    await p.waitForSelector("#outpanel .orow");
    await pullSettled(p);
    const g = await pullRead(p);
    assert(!g.full, "open completely survived a reload");
    near(g.panel, 520, 1, "the split width did not survive a reload");
    await p.context().close();
    return "520px split, open completely, reload → 520px, not open completely";
  });

  await run.check("below 1180px there is no stop, no snap and no divider — the panel overlays at its split width", async () => {
    const p = await openPanelPage(browser, { viewport: { width: 1000, height: 800 } });
    const r = await p.evaluate(() => ({
      divider: getComputedStyle(document.getElementById("op-resizer")).display,
      width: Math.round(document.getElementById("outpanel").getBoundingClientRect().width),
    }));
    assertEqual(r, { divider: "none", width: 400 });
    // A window narrowing under 1180px while open completely brings the conversation back.
    await p.setViewportSize({ width: 1440, height: 800 });
    await pullKey(p, "End");
    await p.setViewportSize({ width: 1000, height: 800 });
    await p.waitForFunction(() => !window.RichOutput.pull().full);
    const g = await pullRead(p);
    assertEqual(g.composerIn, "stage");
    await p.context().close();
    return "1000px: no divider, 400px overlay; narrowing from open completely returned the conversation";
  });

  await run.check("reduced motion: the snap and the return happen, without the dimming or the slide", async () => {
    const p = await openPanelPage(browser, { reducedMotion: "reduce" });
    const g0 = await pullRead(p);
    const stopX = g0.appRight - g0.max;
    await p.mouse.move(g0.dividerLeft + 3, 460);
    await p.mouse.down();
    await p.mouse.move(stopX - 60, 460, { steps: 10 });
    const armed = await pullRead(p);
    await p.mouse.move(stopX - 140, 460, { steps: 4 });
    await pullSettled(p);
    const full = await pullRead(p);
    await p.mouse.up();
    const t = await p.evaluate(() => getComputedStyle(document.getElementById("outpanel")).transitionDuration);
    assertEqual(armed.stageOpacity, 1, "the conversation dimmed under reduced motion");
    assert(full.full, "the snap did not happen under reduced motion");
    assertEqual(t, "0s", "the panel slides under reduced motion");
    await p.context().close();
    return "no dimming while armed, no slide; the snap still happens";
  });

  // ---- contrast, computed, both themes; and the six round-17.1 states, photographed ----------
  for (const theme of ["dark", "light"]) {
    await run.check(theme + " — the wide pull's pill, its borders, the floating composer and the divider's focus mark clear WCAG AA, computed", async () => {
      const p = await openPanelPage(browser, { theme });
      await pullKey(p, "End");
      await p.focus("#op-resizer");
      const read = () =>
        p.evaluate(() => {
          const $ = (id) => document.getElementById(id);
          const cs = (n) => getComputedStyle(n);
          const plane = cs($("outpanel")).backgroundColor;
          const pill = $("op-conv");
          return {
            plane,
            pillBg: cs(pill).backgroundColor,
            pillText: cs($("op-conv-t")).color,
            pillSize: parseFloat(cs($("op-conv-t")).fontSize),
            pillBorder: cs(pill).borderTopColor,
            chevron: cs(pill.querySelector("svg")).color,
            card: cs($("composer-zone")).borderTopColor,
            cardBg: cs($("composer-zone")).backgroundColor,
            focusMark: cs($("op-resizer")).boxShadow,
          };
        });
      const rest = await read();
      await p.hover("#op-conv");
      await p.waitForFunction(() => getComputedStyle(document.getElementById("op-conv")).borderTopColor !== "");
      const hover = await read();
      const base = parseCssColor(theme === "dark" ? "rgb(12, 19, 34)" : "rgb(234, 230, 221)");
      const solid = (c, under) => {
        const x = parseCssColor(c);
        return x.a < 1 ? compositeOver(x, under) : x;
      };
      const plane = solid(rest.plane, base);
      const pillRest = solid(rest.pillBg, plane);
      const pillHover = solid(hover.pillBg, plane);
      const focusColor = /rgba?\([^)]*\)/.exec(rest.focusMark);
      const pairs = [
        ["pill text at rest (16px)", "text", solid(rest.pillText, pillRest), pillRest],
        ["pill text hovered", "text", solid(hover.pillText, pillHover), pillHover],
        ["pill border at rest, on the panel plane", "indicator", solid(rest.pillBorder, plane), plane],
        ["pill border hovered (--gold-text), on the panel plane", "indicator", solid(hover.pillBorder, plane), plane],
        ["pill chevron at rest", "indicator", solid(rest.chevron, pillRest), pillRest],
        ["pill chevron hovered", "indicator", solid(hover.chevron, pillHover), pillHover],
        ["floating composer's boundary, on the panel plane", "indicator", solid(rest.card, plane), plane],
        ["divider focus mark (--gold-text), on the panel plane", "indicator", solid(focusColor ? focusColor[0] : "rgba(0,0,0,0)", plane), plane],
      ];
      assertEqual(rest.pillSize, 16, "the pill's words are 16px");
      const lines = [];
      for (const [what, kind, fg, bg] of pairs) {
        const ratio = Math.round(contrastRatio(fg, bg) * 100) / 100;
        const floor = kind === "indicator" ? 3 : 4.5;
        lines.push(what + " " + hex(fg) + " on " + hex(bg) + " " + ratio + ":1");
        assert(ratio >= floor, theme + ": " + what + " is " + ratio + ":1 against " + floor + ":1 (" + hex(fg) + " on " + hex(bg) + ")");
      }
      assertEqual(hex(solid(rest.cardBg, plane)), hex(solid(theme === "dark" ? "rgb(12, 19, 34)" : "rgb(234, 230, 221)", plane)), "the floating composer is not on --paper, the plane its lines were computed on");
      await p.context().close();
      return lines.join(" | ");
    });

    await run.check(theme + " — the six round-17.1 states, reproduced and photographed: stop, snap, full, full-list, everything, wide", async () => {
      const made = [];
      const picture = async (p, state, opts) => {
        await awaitWorkerChipSettled(p);
        const s = await shot(p, "output-" + state + "-" + theme, Object.assign({ fullPage: false, parkPointer: true }, opts || {}));
        publishShotFile(s.file, path.join(SHOTS, "output-" + state + "-" + theme + ".png"));
        made.push(state);
      };
      const fileRow = (p, name) => p.click('.orow[data-output="' + byName(name).id + '"]');

      // wide — round 17's 560px, kept in 17.1: the panel restored at 560 on the sheet.
      let p = await openPanelPage(browser, { theme });
      await p.evaluate(() => window.RichOutput.setSplitWidth(560));
      await fileRow(p, "comps-2026-10-04.xlsx");
      await pullSettled(p);
      near((await pullRead(p)).panel, 560, 1, "wide");
      await picture(p, "wide");
      await p.context().close();

      // stop — pulled to the stop, the conversation at its narrowest, with the sheet open.
      p = await openPanelPage(browser, { theme });
      await fileRow(p, "comps-2026-10-04.xlsx");
      await p.evaluate(() => window.RichOutput.setSplitWidth(10000));
      await pullSettled(p);
      let g = await pullRead(p);
      near(g.stage, STAGE_MIN, 1, "stop");
      await picture(p, "stop");

      // snap — the same, held 60px past the stop with the chart open: half armed.
      await p.click("#of-back");
      await fileRow(p, "q3-revenue-chart.png");
      g = await pullRead(p);
      const stopX = g.appRight - g.max;
      await p.mouse.move(g.dividerLeft + 3, 460);
      await p.mouse.down();
      await p.mouse.move(stopX - 60, 460, { steps: 8 });
      g = await pullRead(p);
      assert(g.arming && !g.full, "snap: not arming at 60px past the stop");
      await picture(p, "snap", { parkPointer: false });
      await p.mouse.move(stopX, 460, { steps: 4 });
      await p.mouse.up();
      await p.context().close();

      // full — open completely at the sheet, a message typed in the floating composer.
      p = await openPanelPage(browser, { theme });
      await fileRow(p, "comps-2026-10-04.xlsx");
      await pullKey(p, "End");
      await p.fill("#input", "add Tolliver to the comps and re-run the sheet");
      await p.focus("#op-body");
      g = await pullRead(p);
      assert(g.full && g.composerIn === "op-float", "full");
      await picture(p, "full");
      await p.context().close();

      // full-list — open completely at the list, the sidebar still open beside it.
      p = await openPanelPage(browser, { theme });
      await pullKey(p, "End");
      await p.focus("#op-body");
      g = await pullRead(p);
      assert(g.full && g.railWidth > 0, "full-list");
      await picture(p, "full-list");
      await p.context().close();

      // everything — the sidebar away and open completely: the walkthrough at the window's width.
      p = await openPanelPage(browser, { theme });
      await fileRow(p, "comps-walkthrough.mp4");
      await p.keyboard.press("Meta+Shift+S");
      await p.waitForFunction(() => document.body.classList.contains("rail-closed"));
      await pullKey(p, "End");
      await p.focus("#op-body");
      g = await pullRead(p);
      near(g.panel, g.appWidth, 1, "everything");
      // The empty floating field is one whole line tall, measured in its new home (it was once
      // measured while that home was still `display: none`, and came out 20px with its words cut).
      const field = await p.evaluate(() => ({
        h: Math.round(document.getElementById("input").getBoundingClientRect().height),
        talk: Math.round(document.getElementById("talk-toggle").getBoundingClientRect().height),
      }));
      assert(field.h >= field.talk - 2, "the floating field is " + field.h + "px against its " + field.talk + "px controls");
      await picture(p, "everything");
      assertEqual(p.__errors, [], "the page reported errors");
      await p.context().close();
      return made.map((s) => "shots-output/output-" + s + "-" + theme + ".png").join(", ");
    });
  }
}

main().catch((e) => {
  console.error(e);
  process.exit(1);
});

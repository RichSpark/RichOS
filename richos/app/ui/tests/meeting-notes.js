"use strict";
// MEETING NOTES IN THE MAIN WINDOW: round 25 (richos-hq design/mockups/rounds/round-25/, with
// round 25.1, approved 2026-10-13): the two Settings rows under the Codex row ("Start meeting
// notes", which reads "Stop meeting notes" with "Recording · 0:42" while a recording runs, and
// "Pause meeting prompts"), the paused chip in the stage header ("Meeting prompts paused ·
// Resume") and the quiet status at the foot of the conversation ("Preparing your meeting
// notes…"). Each check is red on a tree without `meeting-notes.js`; then every state in both
// themes for contrast (text 4.5:1; the switch, the chip's dot and the working dot 3:1) and the
// 16px floor, computed in WebKit at 1440 by 900.
const path = require("path");
const contrast = require("./lib/contrast");
const { loadPlaywright, createRun, assert, assertEqual, UI_DIR, leaveHome, bootSettled, openThread } = require("./lib/harness");

const plain = s => s.replace(/[’‘]/g, "'").replace(/\s+/g, " ").trim();

async function main() {
  const run = createRun("Meeting notes in the main window (round 25)");
  const browser = await loadPlaywright().webkit.launch();
  const errors = [];
  async function open(theme, meetingNotes) {
    const page = await browser.newPage({ viewport: { width: 1440, height: 900 }, locale: "en-US" });
    page.setDefaultTimeout(30000); // a hang guard only; every wait below waits for a fact
    page.on("pageerror", e => errors.push(String(e)));
    await page.addInitScript(({ theme, meetingNotes }) => {
      localStorage.setItem("richos-theme", theme);
      localStorage.setItem("richos-mock-config", JSON.stringify({ theme, font_scale: 100 }));
      window.__RICHOS_MOCK_PRESET__ = { meetingNotes };
    }, { theme, meetingNotes });
    await page.goto("file://" + path.join(UI_DIR, "index.html"));
    // In a conversation, where the round draws the chip and the status (the stage header and the
    // conversation's foot).
    await leaveHome(page);
    await bootSettled(page);
    await openThread(page, "acme");
    await page.evaluate(t => document.documentElement.setAttribute("data-theme", t), theme);
    return page;
  }
  async function menu(page) {
    if (await page.locator("#set-menu").isHidden()) await page.click("#set-btn");
    await page.waitForSelector("#set-menu #mn-start");
  }
  const text = (page, sel) => page.locator(sel).innerText().then(plain);
  const calls = page => page.evaluate(() => window.__RICHOS_MOCK__.meetingCalls());

  await run.check("the two rows sit under the Codex row, in the round's words, then a rule", async () => {
    const page = await open("dark", {});
    await menu(page);
    const order = await page.evaluate(() => [...document.querySelectorAll("#set-menu > *")].map(e => e.id || e.className));
    const cx = order.indexOf("set-codex"), mn = order.indexOf("set-meeting");
    assert(cx >= 0 && mn === cx + 1, "the meeting rows follow the Codex row: " + order.join(", "));
    assertEqual(await text(page, "#mn-start-name"), "Start meeting notes");
    assertEqual(await text(page, "#mn-pause-label"), "Pause meeting prompts");
    assertEqual(await page.getAttribute("#mn-pause", "aria-checked"), "false");
    assert(await page.locator("#mn-start-state").isHidden(), "no state line before a recording");
    assert(await page.locator("#set-meeting .set-rule").count() === 1, "a rule closes the pair");
    await page.close();
  });

  await run.check("Start meeting notes starts a recording, and the row then reads Stop with its time", async () => {
    const page = await open("dark", {});
    await menu(page);
    await page.click("#mn-start");
    assertEqual((await calls(page)).filter(c => c.cmd === "meeting_start_or_stop").length, 1);
    await menu(page);
    await page.waitForFunction(() => document.getElementById("mn-start-name").textContent === "Stop meeting notes");
    assert(/^Recording · 0:0\d$/.test(await text(page, "#mn-start-state")), "the running time: " + await text(page, "#mn-start-state"));
    assert(await page.locator("#mn-start.is-rec").count() === 1, "the row is marked recording");
    await page.click("#mn-start");
    await menu(page);
    await page.waitForFunction(() => document.getElementById("mn-start-name").textContent === "Start meeting notes");
    await page.close();
  });

  await run.check("pausing shows the chip wherever the user is, and its Resume (or the switch) ends it", async () => {
    const page = await open("dark", {});
    assert(await page.locator("#mn-paused-chip").isHidden(), "no chip while prompts run");
    await menu(page);
    await page.click("#mn-pause");
    await page.waitForSelector("#mn-paused-chip:not([hidden])");
    assertEqual(await text(page, "#mn-paused-chip"), "Meeting prompts paused · Resume");
    assertEqual(await page.getAttribute("#mn-pause", "aria-checked"), "true");
    await page.keyboard.press("Escape");
    await page.click("#mn-paused-chip");
    await page.waitForSelector("#mn-paused-chip[hidden]", { state: "attached" });
    assertEqual((await calls(page)).filter(c => c.cmd === "meeting_set_paused").map(c => c.paused), [true, false]);
    await page.close();
  });

  await run.check("while notes are prepared, the quiet status sits at the foot of the conversation, and goes", async () => {
    const page = await open("dark", {});
    assert(await page.locator("#mn-status").isHidden(), "no status before a recording ends");
    await page.evaluate(() => window.__RICHOS_MOCK__.meetingSet({ processing: "preparing" }));
    await page.waitForSelector("#mn-status:not([hidden])");
    assertEqual(await text(page, "#mn-status"), "Preparing your meeting notes…");
    const after = await page.evaluate(() => document.getElementById("mn-status").previousElementSibling.id);
    assertEqual(after, "messages", "it follows the conversation's messages");
    await page.evaluate(() => window.__RICHOS_MOCK__.meetingSet({ processing: null }));
    await page.waitForSelector("#mn-status[hidden]", { state: "attached" });
    await page.close();
  });

  await run.check("a build without meeting notes shows no rows and no chip", async () => {
    const page = await open("dark", { available: false, paused: true });
    await page.click("#set-btn");
    await page.waitForSelector("#set-menu:not([hidden])");
    await page.evaluate(() => new Promise(r => setTimeout(r, 300)));
    assertEqual(await page.locator("#mn-start").count(), 0);
    assert(await page.locator("#mn-paused-chip").isHidden(), "no chip");
    await page.close();
  });

  await run.check("the recording time reads as the round writes it (0:42, 12:05, 1:02:09)", async () => {
    const page = await open("dark", {});
    assertEqual(await page.evaluate(() => [42, 725, 3729].map(window.RichMeetingNotes.clock)), ["0:42", "12:05", "1:02:09"]);
    await page.close();
  });

  for (const theme of ["dark", "light"]) {
    await run.check(theme + ": every state meets AA contrast (text 4.5:1; switch, chip dot, working dot 3:1) and the 16px floor", async () => {
      const worst = [];
      const states = [
        ["at rest", {}],
        ["recording", { recording: true, startedAt: Date.now() - 42000 }],
        ["paused", { paused: true }],
        ["preparing", { processing: "preparing" }],
      ];
      for (const [name, preset] of states) {
        const page = await open(theme, preset);
        await menu(page);
        await page.evaluate(() => Promise.all(document.getAnimations().filter(a => a.effect && a.effect.getTiming().iterations !== Infinity).map(a => a.finished.catch(() => null))));
        await page.addScriptTag({ content: contrast.pageScript() });
        const result = await page.evaluate(() => {
          const C = window.__contrastMath, failures = [];
          const parse = s => C.parseCssColor(s) || (m => m && { r: +m[1] * 255, g: +m[2] * 255, b: +m[3] * 255, a: m[4] === undefined ? 1 : +m[4] })(
            /^color\(srgb ([\d.e-]+) ([\d.e-]+) ([\d.e-]+)(?: \/ ([\d.e-]+))?\)$/.exec(String(s).trim()));
          const ground = el => { let bg = { r: 0, g: 0, b: 0, a: 0 }; const chain = []; for (let p = el; p; p = p.parentElement) chain.unshift(p);
            for (const p of chain) { const c = parse(getComputedStyle(p).backgroundColor); if (!c) failures.push({ unparsed: getComputedStyle(p).backgroundColor }); else bg = C.compositeOver(c, bg); } return bg; };
          const ratio = (paint, on) => { const bg = ground(on), c = parse(paint); if (!c) { failures.push({ unparsed: paint }); return 0; } return C.round2(C.contrastRatio(C.compositeOver(c, bg), bg)); };
          let worstText = 99, nodes = 0;
          const roots = ["set-meeting", "mn-paused-chip", "mn-status"].map(id => document.getElementById(id)).filter(e => e && !e.hidden);
          for (const root of roots) {
            for (const e of [root, ...root.querySelectorAll("*")]) {
              if (!e.getClientRects().length || e.closest("[hidden]")) continue;
              if (![...e.childNodes].some(n => n.nodeType === Node.TEXT_NODE && n.textContent.trim())) continue;
              const st = getComputedStyle(e), r = ratio(st.color, e), size = parseFloat(st.fontSize);
              nodes++; worstText = Math.min(worstText, r);
              if (r < 4.5) failures.push({ text: e.textContent.trim().slice(0, 50), ratio: r });
              if (size < 16) failures.push({ text: e.textContent.trim().slice(0, 50), size });
            }
          }
          const ind = {};
          const sw = document.getElementById("mn-pause"), sst = getComputedStyle(sw);
          ind.switch = sw.getAttribute("aria-checked") === "true" ? ratio(sst.backgroundColor, sw.parentElement) : ratio(sst.borderTopColor, sw.parentElement);
          const chip = document.getElementById("mn-paused-chip");
          if (!chip.hidden) ind.chipDot = ratio(getComputedStyle(chip.querySelector(".dot")).backgroundColor, chip);
          const status = document.getElementById("mn-status");
          if (!status.hidden) ind.workingDot = ratio(getComputedStyle(status.querySelector(".working-dot")).backgroundColor, status);
          const glyph = document.querySelector("#mn-start svg");
          ind.glyph = ratio(getComputedStyle(glyph).color, glyph.parentElement);
          for (const [k, v] of Object.entries(ind)) if (v < 3) failures.push({ indicator: k, ratio: v });
          return { failures, worstText, nodes, ind };
        });
        assertEqual(result.failures, [], `${theme} ${name}`);
        assert(result.nodes >= 2, `${theme} ${name}: EMPTY INVENTORY (${result.nodes} text nodes)`);
        worst.push(`${name} text ${result.worstText}, ${Object.entries(result.ind).map(([k, v]) => k + " " + v).join(", ")}`);
        await page.close();
      }
      return worst.join("; ");
    });
  }

  await run.check("no renderer errors", async () => assertEqual(errors, []));
  await browser.close();
  process.exitCode = run.report() ? 1 : 0;
}
main().catch(e => { console.error(e); process.exitCode = 1; });

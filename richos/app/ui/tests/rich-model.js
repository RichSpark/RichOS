// RICH'S MODEL, IN PREFERENCES (CEO §119; design: richos-hq design/mockups/rounds/round-23).
//
// His words, the acceptance criterion: "Rich and Stu stay on Opus high effort in RichOS by
// default. But the user should have the option to switch one or both to Fable (in general
// settings, not in quick settings)." And the addendum: "No, there's no need for any confirm
// step. The switch simply shouldn't happen while the user is still on that preferences
// screen. Closing the preferences panel IS the confirmation that the settings chosen there
// now need to be saved and implemented."
//
// CEO 2026-10-14 prioritizes instant replies over effort; the conversation now uses low effort.
//
// WHAT THIS SUITE PROVES is the browser half, through the real renderer and mock.js's stand-in
// for config.rs:
//   1. the rows show the stored choice, first under "Working with Rich" in the one Settings
//      panel (round 27 retired the rail gear's Preferences), never in its first column, and
//      Stu is never named;
//   2. changing a row while Settings is open changes nothing in use: no command, no write
//      to the store, no toast;
//   3. closing Settings saves it, by every way it closes (the button, Escape, a click
//      outside), and says what is now true in one toast; a choice flipped and flipped back
//      saves nothing and says nothing;
//   4. reopening shows it, and so does the next launch;
//   5. reopening while the close's save is still on its way never paints the old choice back
//      (the second review's reproduction, held by a gate rather than a clock);
//   6. at every Text size and every window width down to the 1024 by 700 minimum, in both
//      themes, the chosen label sits inside its gold thumb and every setting in the panel stays
//      inside its column and its own box (second review of d623a3f2: 1440 at 135% put a 176px
//      "Opus · high effort" on a 158.68px thumb, its dark ink past the gold).
// The APPLYING half — the lease in use moves to the stored model at its next boundary — is
// Rust: `the_model_he_chose_in_preferences_answers_his_next_reply`
// (crates/richos-core/tests/rotation_tests.rs, his next reply) and
// `the_model_chosen_for_directing_the_team_is_the_model_of_the_next_job`
// (crates/richos-core/src/work_host.rs, the team's next job).
//
// Run: node rich-model.js   (or `npm test` for every suite in this directory)

"use strict";

const path = require("path");
const { leaveHome, loadPlaywright, createRun, assert, assertEqual, SEED_THEME, UI_DIR } = require("./lib/harness");

const APP = "file://" + path.join(UI_DIR, "index.html");
const STORE = "richos-mock-config";

/// Seed the stand-in for config.json's §119 pair before the page loads (merged, never replaced,
/// as `SEED_THEME` merges).
const SEED_MODELS = (models) => {
  let prev = {};
  try {
    prev = JSON.parse(window.localStorage.getItem("richos-mock-config")) || {};
  } catch (e) {
    prev = {};
  }
  if (models.rich) prev.rich_model = models.rich;
  if (models.stu) prev.stu_model = models.stu;
  window.localStorage.setItem("richos-mock-config", JSON.stringify(prev));
};

async function openApp(browser, opts) {
  const o = opts || {};
  const page = await browser.newPage({ viewport: o.viewport || { width: 1440, height: 900 }, reducedMotion: o.reducedMotion || "no-preference" });
  const errors = [];
  page.on("pageerror", (e) => errors.push(String(e)));
  page.on("console", (m) => {
    if (m.type() === "error") errors.push("console: " + m.text());
  });
  await page.addInitScript(SEED_THEME, o.theme || "dark");
  if (o.models) await page.addInitScript(SEED_MODELS, o.models);
  await page.goto(APP);
  await leaveHome(page);
  await page.waitForSelector(".nav-thread", { state: "attached" });
  await recordCalls(page);
  page.__errors = errors;
  return page;
}

/// Every bridge command from here on, in order. `main.js` reads `Bridge.invoke` at each call,
/// so wrapping the method sees every one.
async function recordCalls(page) {
  await page.evaluate(() => {
    window.__calls = [];
    const original = window.RichBridge.invoke.bind(window.RichBridge);
    window.RichBridge.invoke = (cmd, args) => {
      window.__calls.push({ cmd, args: args || null });
      return original(cmd, args);
    };
  });
}
const calls = (page, cmd) => page.evaluate((c) => window.__calls.filter((x) => x.cmd === c), cmd);
const stored = (page) =>
  page.evaluate((key) => {
    const c = JSON.parse(window.localStorage.getItem(key) || "{}");
    return { rich: c.rich_model || "opus", stu: c.stu_model || "opus" };
  }, STORE);

/// Round 27: Rich's model lives in the one Settings panel, opened by the button at top right.
async function openPrefs(page) {
  await page.click("#set-btn");
  await page.waitForSelector("#set-menu", { state: "visible" });
}
const isOpen = (page) => page.isVisible("#set-menu");

/// What the two rows show: the half each row has checked, and the "default" word beside it.
async function rows(page) {
  return page.evaluate(() => {
    const read = (seg, state) => {
      const on = document.querySelectorAll(`#${seg} .seg-opt[aria-checked="true"]`);
      return { chosen: on.length === 1 ? on[0].dataset.v : `${on.length} checked`, state: document.getElementById(state).textContent };
    };
    return { rich: read("md-rich", "md-rich-state"), stu: read("md-stu", "md-stu-state"), note: document.getElementById("md-note").textContent };
  });
}
/// Wait until the rows show `want` (the read on open is asynchronous). Waited for, not slept
/// for; Playwright's own timeout is only the hang guard, and on expiry the assertion after it
/// names what the rows showed instead.
async function rowsShow(page, want) {
  await page
    .waitForFunction(
      (w) => {
        const v = (seg) => {
          const on = document.querySelector(`#${seg} .seg-opt[aria-checked="true"]`);
          return on ? on.dataset.v : null;
        };
        return v("md-rich") === w.rich && v("md-stu") === w.stu;
      },
      want
    )
    .catch(() => {});
}
async function toastText(page) {
  return page.evaluate(() => {
    const t = document.getElementById("bug-toast");
    return t && !t.hidden ? t.textContent : null;
  });
}
async function toastSays(page, text) {
  await page
    .waitForFunction(
      (want) => {
        const t = document.getElementById("bug-toast");
        return !!t && !t.hidden && t.textContent === want;
      },
      text
    )
    .catch(() => {});
}

async function main() {
  const { webkit } = loadPlaywright();
  const browser = await webkit.launch();
  const run = createRun("Rich's model in Settings (§119, round 27): held while open, saved and applied on close");

  // ---- 1. the rows show the stored choice ------------------------------------------------
  await run.check("1. the rows show the stored choice, first under Working with Rich, away from the first column", async () => {
    // A fresh install: both on Opus, both marked default.
    const fresh = await openApp(browser);
    await openPrefs(fresh);
    await rowsShow(fresh, { rich: "opus", stu: "opus" });
    const r0 = await rows(fresh);
    assertEqual([r0.rich, r0.stu], [{ chosen: "opus", state: "default" }, { chosen: "opus", state: "default" }], "fresh install");
    assert(r0.note.startsWith("Opus is the default for both."), "the line under the rows: " + r0.note);
    // Round 27: first in the middle column, "Working with Rich", and never in the first column
    // of quick controls (§119: "in general settings, not in quick settings").
    const where = await fresh.evaluate(() => {
      const col = document.getElementById("md").closest(".sm-col");
      return {
        group: col.querySelector(".sm-group").textContent,
        firstTitle: col.querySelector(".popover-title").textContent,
        column: [...col.parentElement.children].indexOf(col),
      };
    });
    assertEqual(where, { group: "Working with Rich", firstTitle: "Rich's model", column: 1 }, "where Rich's model sits");
    // The app only ever names Rich (§110): no "Stu" anywhere a person can read.
    const visible = await fresh.$eval("#set-menu", (n) => n.innerText);
    assert(!/\bStu\b/.test(visible), "Stu is named in Settings");
    await fresh.close();

    // A stored choice of Fable for "talks with you" is what the row shows.
    const page = await openApp(browser, { models: { rich: "fable" } });
    await openPrefs(page);
    await rowsShow(page, { rich: "fable", stu: "opus" });
    const r = await rows(page);
    assertEqual([r.rich, r.stu], [{ chosen: "fable", state: "" }, { chosen: "opus", state: "default" }], "stored rich=fable");
    assert(r.note.startsWith("Rich talks with you on Fable."), "the line names the change first: " + r.note);
    assertEqual(page.__errors, [], "no page errors");
    await page.close();
    return "fresh: opus/opus default; stored fable: shown";
  });

  // ---- 2. changing a row while open changes nothing in use ------------------------------
  await run.check("2. a row changed while Settings is open changes nothing in use: no command, no write, no toast", async () => {
    const page = await openApp(browser);
    await openPrefs(page);
    await rowsShow(page, { rich: "opus", stu: "opus" });
    await page.click('#md-rich .seg-opt[data-v="fable"]');
    // The keyboard is a second way in: the arrows move the team row's choice.
    await page.focus('#md-stu .seg-opt[data-v="opus"]');
    await page.keyboard.press("ArrowRight");
    // A pick is synchronous (`pickModel`): once the rows show both, any command or write a
    // pick could make has already been made, so the negatives below wait on a fact, not a clock.
    await rowsShow(page, { rich: "fable", stu: "fable" });
    const r = await rows(page);
    assertEqual([r.rich.chosen, r.stu.chosen], ["fable", "fable"], "the popover shows both picks");
    assert(r.note.startsWith("Rich is on Fable for both."), "and says what they would mean: " + r.note);
    assert(await isOpen(page), "Settings is still open");
    assertEqual(await calls(page, "set_lease_models"), [], "a command went out while Settings was open");
    assertEqual(await stored(page), { rich: "opus", stu: "opus" }, "the store changed while Settings was open");
    assertEqual(await toastText(page), null, "a toast fired before Settings closed");
    await page.close();
    return "shown, not sent";
  });

  // ---- 3. closing Settings saves and applies it ------------------------------------------
  await run.check("3. closing Settings saves it by every way it closes, and says what is now true", async () => {
    // (a) the Settings button.
    const page = await openApp(browser);
    await openPrefs(page);
    await rowsShow(page, { rich: "opus", stu: "opus" });
    await page.click('#md-rich .seg-opt[data-v="fable"]');
    await page.click("#set-btn");
    await toastSays(page, "Rich now talks with you on Fable.");
    assertEqual(await isOpen(page), false, "the Settings button closed Settings");
    const sent = await calls(page, "set_lease_models");
    assertEqual(sent.map((c) => c.args), [{ rich: "fable", stu: "opus" }], "one save, both rows, on close");
    assertEqual(await stored(page), { rich: "fable", stu: "opus" }, "the store after close");
    assertEqual(await toastText(page), "Rich now talks with you on Fable.", "the toast says what is now true");

    // (b) Escape, both rows back to Opus at once: one save, one toast with both sentences.
    await openPrefs(page);
    await rowsShow(page, { rich: "fable", stu: "opus" });
    await page.click('#md-rich .seg-opt[data-v="opus"]');
    await page.click('#md-stu .seg-opt[data-v="fable"]');
    await page.keyboard.press("Escape");
    const both = "Rich talks with you on Opus again, the default. Rich now directs your team on Fable.";
    await toastSays(page, both);
    assertEqual(await isOpen(page), false, "Escape closed Settings");
    assertEqual(await stored(page), { rich: "opus", stu: "fable" }, "the store after Escape");
    assertEqual(await toastText(page), both, "one toast, both changes");

    // (c) a click outside; and a choice flipped and flipped back saves nothing, says nothing.
    await page.evaluate(() => {
      window.__calls = [];
      const t = document.getElementById("bug-toast");
      if (t) t.hidden = true;
    });
    await openPrefs(page);
    await rowsShow(page, { rich: "opus", stu: "fable" });
    await page.click('#md-rich .seg-opt[data-v="fable"]');
    await page.click('#md-rich .seg-opt[data-v="opus"]');
    // Below the panel, which now spans the stage. Measured, not typed: with round 36's Your
    // instructions and round 41's Terminal setup both in the middle column the panel ends at
    // y=887 in this 900px window, so the old fixed y=880 landed inside it.
    const below = await page.evaluate(() => {
      const r = document.getElementById("set-menu").getBoundingClientRect();
      return { y: Math.round((r.bottom + window.innerHeight) / 2), room: window.innerHeight - r.bottom };
    });
    assert(below.room >= 4, "no room below the Settings panel to click outside it: " + below.room + "px");
    await page.mouse.click(1100, below.y);
    // The save is sent synchronously inside the close (`commitModelChoice`), and a toast only
    // follows a sent save, so once the popover is hidden the negatives below are settled.
    await page.waitForSelector("#set-menu", { state: "hidden" }).catch(() => {});
    assertEqual(await isOpen(page), false, "a click outside closed Settings");
    assertEqual(await calls(page, "set_lease_models"), [], "an unchanged choice was saved");
    assertEqual(await toastText(page), null, "an unchanged choice was announced");
    assertEqual(page.__errors, [], "no page errors");
    await page.close();
    return "the button, Escape, outside click";
  });

  // ---- 4. reopening shows it ---------------------------------------------------------------
  await run.check("4. reopening Settings shows the saved choice, and so does the next launch", async () => {
    const page = await openApp(browser, { theme: "light" });
    await openPrefs(page);
    await rowsShow(page, { rich: "opus", stu: "opus" });
    await page.click('#md-stu .seg-opt[data-v="fable"]');
    await page.click("#set-btn");
    await toastSays(page, "Rich now directs your team on Fable.");
    await openPrefs(page);
    await rowsShow(page, { rich: "opus", stu: "fable" });
    const r = await rows(page);
    assertEqual([r.rich, r.stu], [{ chosen: "opus", state: "default" }, { chosen: "fable", state: "" }], "reopened");
    assert(r.note.includes("Your teammates keep their own models."), "the team row's line: " + r.note);

    // The next launch reads the store, not the page's memory.
    await page.reload();
    await leaveHome(page);
    await page.waitForSelector(".nav-thread", { state: "attached" });
    await openPrefs(page);
    await rowsShow(page, { rich: "opus", stu: "fable" });
    const after = await rows(page);
    assertEqual([after.rich.chosen, after.stu.chosen], ["opus", "fable"], "after a relaunch");
    assertEqual(page.__errors, [], "no page errors");
    await page.close();
    return "reopen and relaunch";
  });

  // ---- 5. a reopen while the save is on its way never paints the old choice back ----------
  await run.check("5. reopening Settings while the close's save is still on its way shows the new choice", async () => {
    // The second review's reproduction (rv-20261011T040418Z): the save is held open, so the
    // read that reopening makes would answer from the store as it was before the save. Held by
    // a gate this check releases, not by a clock, so the order is the same on any host.
    const page = await openApp(browser);
    await page.evaluate(() => {
      const inner = window.RichBridge.invoke;
      window.__saveGate = new Promise((release) => (window.__releaseSave = release));
      window.__readsDone = 0;
      window.RichBridge.invoke = async (cmd, args) => {
        if (cmd === "set_lease_models") await window.__saveGate;
        try {
          return await inner(cmd, args);
        } finally {
          if (cmd === "get_lease_models") window.__readsDone++;
        }
      };
    });
    await openPrefs(page);
    await rowsShow(page, { rich: "opus", stu: "opus" });
    await page.click('#md-rich .seg-opt[data-v="fable"]');
    await page.click("#set-btn"); // close: the save starts and is held
    const readsBefore = await page.evaluate(() => window.__readsDone);
    await openPrefs(page); // reopen: its read starts while the save is held
    await page.evaluate(() => window.__releaseSave());
    await toastSays(page, "Rich now talks with you on Fable.");
    await page.waitForFunction((n) => window.__readsDone > n, readsBefore).catch(() => {});
    assertEqual(await stored(page), { rich: "fable", stu: "opus" }, "the save landed");
    const r = await rows(page);
    assertEqual([r.rich.chosen, r.stu.chosen], ["fable", "opus"], "the reopened rows after the save and the read settled");
    assertEqual(page.__errors, [], "no page errors");
    await page.close();
    return "read waited for the save";
  });

  // ---- 6. every Text size, every window width: the chosen label inside its thumb --------
  await run.check("6. at every Text size and window width, in both themes, the chosen model sits inside its gold thumb and every setting stays readable", async () => {
    // 1359/1360 straddle the three-column line at 100%; 1366 is the earlier review's window;
    // 1440 is round 27's own and this review's; 1024 by 700 is the app's minimum.
    const widths = [1024, 1280, 1359, 1360, 1366, 1440];
    const problems = [];
    let cases = 0;
    for (const theme of ["dark", "light"]) {
      for (const width of widths) {
        const page = await openApp(browser, { theme, viewport: { width, height: 700 }, reducedMotion: "reduce" });
        await openPrefs(page);
        await rowsShow(page, { rich: "opus", stu: "opus" });
        const steps = await page.evaluate(() => window.RichTheme.STEPS);
        for (const scale of steps) {
          for (const stu of ["opus", "fable"]) {
            const seen = await page.evaluate(
              async ([s, choice]) => {
                window.RichTheme.setScale(s);
                document.querySelector(`#md-stu .seg-opt[data-v="${choice}"]`).click();
                await new Promise((r) => requestAnimationFrame(() => requestAnimationFrame(r)));
                const bad = [];
                const menu = document.getElementById("set-menu");
                // The chosen label of each row, against the gold thumb under it.
                for (const id of ["md-rich", "md-stu"]) {
                  const seg = document.getElementById(id);
                  const thumb = seg.querySelector(".seg-thumb").getBoundingClientRect();
                  const opt = seg.querySelector('.seg-opt[aria-checked="true"]');
                  const range = document.createRange();
                  range.selectNodeContents(opt);
                  const t = range.getBoundingClientRect();
                  const gap = Math.min(t.left - thumb.left, thumb.right - t.right, t.top - thumb.top, thumb.bottom - t.bottom);
                  if (gap < 0) bad.push(`${id} "${opt.textContent}" ${t.width.toFixed(1)}px runs ${(-gap).toFixed(1)}px past a ${thumb.width.toFixed(1)}px thumb`);
                }
                // Every line of text inside its column (or foot cell, or heading), and every
                // control wide enough for what it says.
                const walker = document.createTreeWalker(menu, NodeFilter.SHOW_TEXT);
                for (let n = walker.nextNode(); n; n = walker.nextNode()) {
                  const el = n.parentElement;
                  if (!n.textContent.trim() || !el.getClientRects().length || el.closest("[hidden], .cx-tip")) continue;
                  if (getComputedStyle(el).visibility === "hidden") continue;
                  const box = (el.closest(".sm-col, .sm-foot-cell, .sm-head") || menu).getBoundingClientRect();
                  const range = document.createRange();
                  range.selectNodeContents(n);
                  for (const t of range.getClientRects()) {
                    if (t.width >= 1 && (t.left < box.left - 0.5 || t.right > box.right + 0.5)) bad.push(`"${n.textContent.trim()}" leaves its column`);
                  }
                }
                for (const c of menu.querySelectorAll("button, select, input")) {
                  if (!c.getClientRects().length || c.closest("[hidden], .cx-tip")) continue;
                  if (c.scrollWidth > c.clientWidth + 1) bad.push(`#${c.id || c.className} holds ${c.scrollWidth}px in ${c.clientWidth}px`);
                }
                if (menu.scrollWidth > menu.clientWidth) bad.push(`the panel scrolls ${menu.scrollWidth - menu.clientWidth}px sideways`);
                return { scale: document.documentElement.getAttribute("data-font-scale"), bad };
              },
              [scale, stu]
            );
            cases++;
            assertEqual(seen.scale, String(scale), `Text size applied at ${width}px`);
            for (const b of seen.bad) problems.push(`${theme} ${width}px ${scale}% ${stu}: ${b}`);
          }
        }
        assertEqual(page.__errors, [], "no page errors");
        await page.close();
      }
    }
    assertEqual(problems, [], "nothing in Settings leaves its place");
    return `${cases} cases: 2 themes x ${widths.length} widths x 7 Text sizes x both choices for the second row`;
  });

  await browser.close();
  const failed = run.report();
  process.exitCode = failed ? 1 : 0;
}

main().catch((e) => {
  console.error(e);
  process.exit(1);
});

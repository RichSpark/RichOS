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
// CEO 2026-11-17: "Fable at low effort might actually be a good new default for Rich in the
// RichOS app. And for Stu, Fable at medium effort might be the best new default." So the row
// marked "default" is Fable in both, and Opus is the option.
//
// CEO 2026-11-17, round 86.1 ("round-86.1 is good to go. Implement now."): an effort control of
// five levels under each row, each model keeping its own level; the defaults are the recommended
// levels (Fable at Low and Medium, Opus at Low and High); choosing Max opens a warning.
//
// CEO 2026-11-26: "for Rich let's change our recommendation (and default setting) to "High" when the
// user selects Opus instead of Fable for Rich." So Opus is at High in both rows. And the jitter he
// showed over "Recommended" (check 9): the pointer over a step never moves the steps.
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
//   7. the effort control: the recommended levels and their words, a preview under the pointer,
//      a press, each model keeping its own level, the keyboard, and one save of everything on
//      close;
//   8. the Max warning: his words, the safe button focused, Escape and a press outside going
//      back to the recommended level with Settings still open, "Use Max anyway" keeping Max,
//      no second warning on a Max already chosen or saved, and the line on close.
//   9. the pointer over a step never moves the steps, at every Text size and window width, in
//      both rows on both models: the level's name it previews is held in a box as wide as the
//      widest name that row can show, so it never moves to or from its own line under the
//      label (CEO 2026-11-26, his recording: over "Recommended" the steps jumped up a line
//      under the pointer, out from under it and back, again and again).
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
    return { rich: c.rich_model || "fable", stu: c.stu_model || "fable" };
  }, STORE);
/// The four levels the recommended ones are, as `set_lease_models` carries them.
const RECOMMENDED = { rich: { opus: "high", fable: "low" }, stu: { opus: "high", fable: "medium" } };
const withEffort = (effort) => ({ rich: { ...RECOMMENDED.rich, ...(effort.rich || {}) }, stu: { ...RECOMMENDED.stu, ...(effort.stu || {}) } });
/// What one row's effort control shows.
async function effort(page, who) {
  return page.evaluate((w) => {
    const ef = document.getElementById("ef-" + w);
    const on = ef.querySelector('.ef-step[aria-checked="true"]');
    const label = ef.querySelector(".ef-mark-label").getBoundingClientRect();
    const def = ef.querySelector(".ef-step.is-default").getBoundingClientRect();
    const mid = def.left + def.width / 2;
    return {
      level: on ? on.dataset.l : null,
      // What the head shows: its label and the level's name (the head also holds every other
      // name that row can show, unseen, as the width of the name's box; check 9).
      head: ef.querySelector(".ef-label").textContent + ef.querySelector(".ef-name").textContent,
      say: ef.querySelector(".ef-say").textContent,
      steps: ef.querySelectorAll(".ef-step").length,
      peek: ef.querySelectorAll(".ef-step.is-peek").length,
      recommendedUnder: label.left <= mid && label.right >= mid ? ef.querySelector(".ef-step.is-default").dataset.l : "off",
    };
  }, who);
}
/// The Max warning while it is up, or null.
async function warning(page) {
  return page.evaluate(() => {
    const w = document.querySelector(".efw");
    if (!w || w.hidden || !w.classList.contains("is-open")) return null;
    const text = (sel) => w.querySelector(sel).textContent;
    return {
      title: text(".efw-title"),
      where: text(".efw-where"),
      lead: text(".efw-lead"),
      why: text(".efw-why"),
      rec: text(".efw-rec-text"),
      keep: text(".efw-keep"),
      anyway: text(".efw-anyway"),
      focus: document.activeElement ? document.activeElement.className : null,
    };
  });
}
const warningIs = (page, up) =>
  page
    .waitForFunction(
      (want) => {
        const w = document.querySelector(".efw");
        return (!!w && !w.hidden && w.classList.contains("is-open")) === want;
      },
      up
    )
    .catch(() => {});
const levelIs = (page, who, level) =>
  page
    .waitForFunction(
      ([w, l]) => {
        const on = document.querySelector(`#ef-${w} .ef-step[aria-checked="true"]`);
        return !!on && on.dataset.l === l;
      },
      [who, level]
    )
    .catch(() => {});

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
    // A fresh install: both on Fable, both marked default (CEO 2026-11-17).
    const fresh = await openApp(browser);
    await openPrefs(fresh);
    await rowsShow(fresh, { rich: "fable", stu: "fable" });
    const r0 = await rows(fresh);
    assertEqual([r0.rich, r0.stu], [{ chosen: "fable", state: "default" }, { chosen: "fable", state: "default" }], "fresh install");
    assertEqual(r0.note, "Fable is the default for both. Fable and higher effort both use up your Claude limits faster. Each model keeps its own effort.", "the line under the rows");
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
    assertEqual(where, { group: "Working with Rich", firstTitle: "Rich's model and effort", column: 1 }, "where Rich's model sits");
    // The app only ever names Rich (§110): no "Stu" anywhere a person can read.
    const visible = await fresh.$eval("#set-menu", (n) => n.innerText);
    assert(!/\bStu\b/.test(visible), "Stu is named in Settings");
    await fresh.close();

    // A stored choice of Opus for "talks with you" is what the row shows.
    const page = await openApp(browser, { models: { rich: "opus" } });
    await openPrefs(page);
    await rowsShow(page, { rich: "opus", stu: "fable" });
    const r = await rows(page);
    assertEqual([r.rich, r.stu], [{ chosen: "opus", state: "" }, { chosen: "fable", state: "default" }], "stored rich=opus");
    assert(r.note.startsWith("Rich talks with you on Opus at high effort."), "the line names the change first: " + r.note);
    // CEO 2026-11-26: "for Rich let's change our recommendation (and default setting) to "High" when
    // the user selects Opus instead of Fable for Rich." Nobody chose a level here, so it is High.
    const ro = await effort(page, "rich");
    assertEqual([ro.level, ro.head, ro.recommendedUnder], ["high", "Effort on OpusHigh · recommended for Opus", "high"], "Rich on Opus: High, chosen and recommended");
    assertEqual(page.__errors, [], "no page errors");
    await page.close();
    return "fresh: fable/fable default; stored opus: shown";
  });

  // ---- 2. changing a row while open changes nothing in use ------------------------------
  await run.check("2. a row changed while Settings is open changes nothing in use: no command, no write, no toast", async () => {
    const page = await openApp(browser);
    await openPrefs(page);
    await rowsShow(page, { rich: "fable", stu: "fable" });
    await page.click('#md-rich .seg-opt[data-v="opus"]');
    // The keyboard is a second way in: the arrows move the team row's choice.
    await page.focus('#md-stu .seg-opt[data-v="fable"]');
    await page.keyboard.press("ArrowLeft");
    // A pick is synchronous (`pickModel`): once the rows show both, any command or write a
    // pick could make has already been made, so the negatives below wait on a fact, not a clock.
    await rowsShow(page, { rich: "opus", stu: "opus" });
    const r = await rows(page);
    assertEqual([r.rich.chosen, r.stu.chosen], ["opus", "opus"], "the popover shows both picks");
    assert(r.note.startsWith("Rich talks with you on Opus at high effort. Rich directs your team on Opus at high effort."), "and says what they would mean: " + r.note);
    assert(await isOpen(page), "Settings is still open");
    assertEqual(await calls(page, "set_lease_models"), [], "a command went out while Settings was open");
    assertEqual(await stored(page), { rich: "fable", stu: "fable" }, "the store changed while Settings was open");
    assertEqual(await toastText(page), null, "a toast fired before Settings closed");
    await page.close();
    return "shown, not sent";
  });

  // ---- 3. closing Settings saves and applies it ------------------------------------------
  await run.check("3. closing Settings saves it by every way it closes, and says what is now true", async () => {
    // (a) the Settings button.
    const page = await openApp(browser);
    await openPrefs(page);
    await rowsShow(page, { rich: "fable", stu: "fable" });
    await page.click('#md-rich .seg-opt[data-v="opus"]');
    await page.click("#set-btn");
    await toastSays(page, "Rich now talks with you on Opus at high effort.");
    assertEqual(await isOpen(page), false, "the Settings button closed Settings");
    const sent = await calls(page, "set_lease_models");
    assertEqual(sent.map((c) => c.args), [{ rich: "opus", stu: "fable", effort: RECOMMENDED, chosen: ["rich_model"] }], "one save, both rows and their levels, on close");
    assertEqual(await stored(page), { rich: "opus", stu: "fable" }, "the store after close");
    assertEqual(await toastText(page), "Rich now talks with you on Opus at high effort.", "the toast says what is now true");

    // (b) Escape, both rows changed at once: one save, one toast with both sentences.
    await openPrefs(page);
    await rowsShow(page, { rich: "opus", stu: "fable" });
    await page.click('#md-rich .seg-opt[data-v="fable"]');
    await page.click('#md-stu .seg-opt[data-v="opus"]');
    await page.keyboard.press("Escape");
    const both = "Rich talks with you on Fable at low effort again, the default. Rich now directs your team on Opus at high effort.";
    await toastSays(page, both);
    assertEqual(await isOpen(page), false, "Escape closed Settings");
    assertEqual(await stored(page), { rich: "fable", stu: "opus" }, "the store after Escape");
    assertEqual(await toastText(page), both, "one toast, both changes");

    // (c) a click outside; and a choice flipped and flipped back saves nothing, says nothing.
    await page.evaluate(() => {
      window.__calls = [];
      const t = document.getElementById("bug-toast");
      if (t) t.hidden = true;
    });
    await openPrefs(page);
    await rowsShow(page, { rich: "fable", stu: "opus" });
    await page.click('#md-rich .seg-opt[data-v="opus"]');
    await page.click('#md-rich .seg-opt[data-v="fable"]');
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
    // Changed and set back before closing: nothing in use changed, so nothing is said, and
    // the row he pressed is saved as his choice (CEO 2026-11-17: the settings can always tell
    // what is what).
    assertEqual((await calls(page, "set_lease_models")).map((c) => c.args),
      [{ rich: "fable", stu: "opus", effort: RECOMMENDED, chosen: ["rich_model"] }], "the row pressed and set back was not saved as chosen");
    assertEqual(await toastText(page), null, "an unchanged choice was announced");
    assertEqual(await page.evaluate((key) => JSON.parse(window.localStorage.getItem(key)).lease_chosen, STORE), ["rich_model", "stu_model"], "what the store holds as chosen");
    // Opened and closed with nothing pressed: no save at all. And the default pressed where it
    // already stands is a press: saved as chosen, said nowhere.
    await page.evaluate(() => (window.__calls = []));
    await openPrefs(page);
    await rowsShow(page, { rich: "fable", stu: "opus" });
    await page.keyboard.press("Escape");
    await page.waitForSelector("#set-menu", { state: "hidden" }).catch(() => {});
    assertEqual(await calls(page, "set_lease_models"), [], "Settings opened and closed untouched was saved");
    await openPrefs(page);
    await rowsShow(page, { rich: "fable", stu: "opus" });
    await page.click("#ef-rich-low");
    await page.keyboard.press("Escape");
    await page.waitForSelector("#set-menu", { state: "hidden" }).catch(() => {});
    assertEqual((await calls(page, "set_lease_models")).map((c) => c.args),
      [{ rich: "fable", stu: "opus", effort: RECOMMENDED, chosen: ["rich_effort_fable"] }], "the default pressed where it stands was not saved as chosen");
    assertEqual(await toastText(page), null, "a press that changed nothing was announced");
    assertEqual(page.__errors, [], "no page errors");
    await page.close();
    return "the button, Escape, outside click";
  });

  // ---- 4. reopening shows it ---------------------------------------------------------------
  await run.check("4. reopening Settings shows the saved choice, and so does the next launch", async () => {
    const page = await openApp(browser, { theme: "light" });
    await openPrefs(page);
    await rowsShow(page, { rich: "fable", stu: "fable" });
    await page.click('#md-stu .seg-opt[data-v="opus"]');
    await page.click("#set-btn");
    await toastSays(page, "Rich now directs your team on Opus at high effort.");
    await openPrefs(page);
    await rowsShow(page, { rich: "fable", stu: "opus" });
    const r = await rows(page);
    assertEqual([r.rich, r.stu], [{ chosen: "fable", state: "default" }, { chosen: "opus", state: "" }], "reopened");
    assert(r.note.startsWith("Rich directs your team on Opus at high effort."), "the team row's line: " + r.note);

    // The next launch reads the store, not the page's memory.
    await page.reload();
    await leaveHome(page);
    await page.waitForSelector(".nav-thread", { state: "attached" });
    await openPrefs(page);
    await rowsShow(page, { rich: "fable", stu: "opus" });
    const after = await rows(page);
    assertEqual([after.rich.chosen, after.stu.chosen], ["fable", "opus"], "after a relaunch");
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
    await rowsShow(page, { rich: "fable", stu: "fable" });
    await page.click('#md-rich .seg-opt[data-v="opus"]');
    await page.click("#set-btn"); // close: the save starts and is held
    const readsBefore = await page.evaluate(() => window.__readsDone);
    await openPrefs(page); // reopen: its read starts while the save is held
    await page.evaluate(() => window.__releaseSave());
    await toastSays(page, "Rich now talks with you on Opus at high effort.");
    await page.waitForFunction((n) => window.__readsDone > n, readsBefore).catch(() => {});
    assertEqual(await stored(page), { rich: "opus", stu: "fable" }, "the save landed");
    const r = await rows(page);
    assertEqual([r.rich.chosen, r.stu.chosen], ["opus", "fable"], "the reopened rows after the save and the read settled");
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
        await rowsShow(page, { rich: "fable", stu: "fable" });
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

  // ---- 7. the effort control ------------------------------------------------------------------
  await run.check("7. the effort control: recommended levels, a preview, a press, each model's own level, the keyboard, one save on close", async () => {
    const page = await openApp(browser);
    await openPrefs(page);
    await rowsShow(page, { rich: "fable", stu: "fable" });
    const [r0, s0] = [await effort(page, "rich"), await effort(page, "stu")];
    assertEqual([r0.steps, s0.steps], [5, 5], "five levels under each row");
    assertEqual([r0.level, s0.level], ["low", "medium"], "the defaults: Fable at Low when he talks, Fable at Medium for the team");
    assertEqual([r0.head, s0.head], ["Effort on FableLow · recommended", "Effort on FableMedium · recommended"], "each default is named recommended");
    assertEqual([r0.recommendedUnder, s0.recommendedUnder], ["low", "medium"], '"Recommended" sits under the default step');
    assertEqual(
      await page.$eval("#ef-rec", (n) => n.textContent),
      "The defaults are the recommended levels for nearly all kinds of work. Higher effort is not always better and raising it is generally not recommended.",
      "the line under the title"
    );

    // The pointer over a level previews it, with the caution first, and changes nothing.
    await page.hover('#ef-rich .ef-step[data-l="xhigh"]');
    await page.waitForFunction(() => document.querySelectorAll("#ef-rich .ef-step.is-peek").length === 3).catch(() => {});
    const peek = await effort(page, "rich");
    assertEqual([peek.level, peek.peek, peek.head], ["low", 3, "Effort on FableExtra high"], "a preview, Low still chosen");
    assert(peek.say.startsWith("Generally not recommended. "), "a level above the recommended one leads with the caution: " + peek.say);
    await page.mouse.move(20, 450);
    await page.waitForFunction(() => document.querySelectorAll("#ef-rich .ef-step.is-peek").length === 0).catch(() => {});
    assertEqual((await effort(page, "rich")).head, "Effort on FableLow · recommended", "the pointer away: the preview is gone");

    // A press chooses; the row is no longer the default.
    await page.click('#ef-rich .ef-step[data-l="xhigh"]');
    await levelIs(page, "rich", "xhigh");
    assertEqual([(await effort(page, "rich")).level, (await rows(page)).rich.state], ["xhigh", ""], "Extra high chosen");
    // Each model keeps its own level.
    await page.click('#md-rich .seg-opt[data-v="opus"]');
    await levelIs(page, "rich", "high");
    assertEqual((await effort(page, "rich")).head, "Effort on OpusHigh · recommended for Opus", "Opus has its own level");
    await page.click('#ef-rich .ef-step[data-l="medium"]');
    await page.click('#md-rich .seg-opt[data-v="fable"]');
    await levelIs(page, "rich", "xhigh");
    assertEqual((await effort(page, "rich")).level, "xhigh", "Fable's Extra high came back");
    // The keyboard on the team row: Home, then the arrow up one.
    await page.focus('#ef-stu .ef-step[data-l="medium"]');
    await page.keyboard.press("Home");
    await levelIs(page, "stu", "low");
    await page.keyboard.press("ArrowRight");
    await page.keyboard.press("ArrowRight");
    await levelIs(page, "stu", "high");
    assertEqual(await page.evaluate(() => document.activeElement.dataset.l), "high", "the focus follows the chosen level");
    const say = (await effort(page, "stu")).say;
    assertEqual(say, "Generally not recommended. Careful work, checked as he goes.", "what High means for the team on Fable");

    // A press that arrives as a click alone, with no pointer (VoiceOver's press): it chooses too.
    await page.$eval("#ef-stu-low", (n) => n.click());
    await levelIs(page, "stu", "low");
    assertEqual((await effort(page, "stu")).level, "low", "a click with no pointer chose nothing");
    await page.$eval("#ef-stu-high", (n) => n.click());
    await levelIs(page, "stu", "high");

    // Nothing has left the page; closing saves all of it once and says it in one line.
    assertEqual(await calls(page, "set_lease_models"), [], "a command went out while Settings was open");
    await page.click("#set-btn");
    const line = "Rich now talks with you on Fable at extra high effort. Rich now directs your team on Fable at high effort.";
    await toastSays(page, line);
    assertEqual(await toastText(page), line, "the line on close");
    const sent = await calls(page, "set_lease_models");
    assertEqual(
      sent.map((c) => c.args),
      [{ rich: "fable", stu: "fable", effort: withEffort({ rich: { opus: "medium", fable: "xhigh" }, stu: { fable: "high" } }), chosen: ["rich_effort_fable", "rich_effort_opus", "rich_model", "stu_effort_fable"] }],
      "one save: both models and each row's level on each model"
    );
    // Reopened, and after a relaunch.
    await page.reload();
    await leaveHome(page);
    await page.waitForSelector(".nav-thread", { state: "attached" });
    await openPrefs(page);
    await levelIs(page, "stu", "high");
    assertEqual([(await effort(page, "rich")).level, (await effort(page, "stu")).level], ["xhigh", "high"], "after a relaunch");
    assertEqual(page.__errors, [], "no page errors");
    await page.close();
    return "preview, press, per-model levels, keyboard, one save";
  });

  // ---- 8. the Max warning ----------------------------------------------------------------------
  await run.check("8. choosing Max opens the warning in his words; the safe ways go back to the recommended level; Max can be kept", async () => {
    for (const theme of ["dark", "light"]) {
      const page = await openApp(browser, { theme });
      await openPrefs(page);
      await rowsShow(page, { rich: "fable", stu: "fable" });
      await page.click('#ef-rich .ef-step[data-l="max"]');
      await warningIs(page, true);
      assertEqual(
        await warning(page),
        {
          title: "WARNING!",
          where: "Effort on Fable, when he talks with you",
          lead: "The Max effort level is NOT recommended for anything.",
          why: "It has a strong tendency to yield less than great results and it does that at an extremely high cost.",
          rec: "Low is the recommended level here, right for nearly all kinds of work.",
          keep: "Use Low, the recommended level",
          anyway: "Use Max anyway",
          focus: "efw-keep",
        },
        theme + ": the warning"
      );
      assert(await isOpen(page), "Settings stays open under the warning");
      const behind = await effort(page, "rich");
      assertEqual([behind.level, behind.head], ["max", "Effort on FableMax · not recommended"], "behind it Max is named not recommended");
      assert(behind.say.startsWith("Not recommended for anything. "), behind.say);
      // Tab stays on the two buttons.
      await page.keyboard.press("Tab");
      const second = await page.evaluate(() => document.activeElement.className);
      await page.keyboard.press("Tab");
      assertEqual([second, await page.evaluate(() => document.activeElement.className)], ["efw-anyway", "efw-keep"], "Tab stays on the two buttons");
      // Escape is the safe button, and it does not close Settings.
      await page.keyboard.press("Escape");
      await warningIs(page, false);
      await levelIs(page, "rich", "low");
      assertEqual([await warning(page), await isOpen(page), (await effort(page, "rich")).level], [null, true, "low"], "Escape: back to Low, Settings open");
      // A press outside the card is the safe button too.
      await page.click('#ef-rich .ef-step[data-l="max"]');
      await warningIs(page, true);
      await page.mouse.click(30, 30);
      await warningIs(page, false);
      await levelIs(page, "rich", "low");
      assertEqual([await isOpen(page), (await effort(page, "rich")).level], [true, "low"], "a press outside: back to Low, Settings open");
      // Max chosen again right behind the press that closed the warning (the second press of a
      // double press, 2026-11-17's nightly): the new warning stays up. Both presses are made in one
      // turn of the page and it is read after the first warning's leaving has run out, so no
      // machine's speed decides which comes first.
      await page.click('#ef-rich .ef-step[data-l="max"]');
      await warningIs(page, true);
      const again = await page.evaluate(
        () =>
          new Promise((done) => {
            document.querySelector(".efw-scrim").click();
            document.querySelector("#ef-rich-max").click();
            setTimeout(() => {
              const w = document.querySelector(".efw");
              done({ hidden: w.hidden, classes: w.className, focus: document.activeElement ? document.activeElement.className : null });
            }, 600);
          })
      );
      assertEqual(again, { hidden: false, classes: "efw is-open", focus: "efw-keep" }, "Max chosen again while the first warning was leaving");
      await page.keyboard.press("Escape");
      await warningIs(page, false);
      await levelIs(page, "rich", "low");
      assertEqual([await warning(page), (await effort(page, "rich")).level], [null, "low"], "Escape after it: back to Low");
      // A click with no pointer on Max (VoiceOver's press) opens the warning too; Enter on the
      // focused safe button goes back.
      await page.$eval("#ef-rich-max", (n) => n.click());
      await warningIs(page, true);
      assertEqual((await warning(page)).focus, "efw-keep", "the warning after a click with no pointer");
      await page.keyboard.press("Enter");
      await warningIs(page, false);
      await levelIs(page, "rich", "low");
      assertEqual(await page.evaluate(() => document.activeElement.id), "ef-rich-low", "the focus goes back to the recommended step");
      // The warning card never claims aria-modal (appearance.js 7c: WebKit would leave Settings
      // out of the accessibility tree).
      assertEqual(await page.$eval(".efw", (n) => [n.getAttribute("role"), n.getAttribute("aria-modal")]), ["alertdialog", null], "the warning's role");
      // The team row on Opus names High, from the keyboard's End; "Use Max anyway" keeps Max.
      await page.click('#md-stu .seg-opt[data-v="opus"]');
      await levelIs(page, "stu", "high");
      await page.focus('#ef-stu .ef-step[data-l="high"]');
      await page.keyboard.press("End");
      await warningIs(page, true);
      const team = await warning(page);
      assertEqual([team.where, team.keep], ["Effort on Opus, when he directs your team", "Use High, the recommended level"], "the team row on Opus");
      await page.click(".efw-anyway");
      await warningIs(page, false);
      assertEqual([(await effort(page, "stu")).level, await isOpen(page)], ["max", true], "Max kept, Settings open");
      // Max pressed while already chosen asks nothing again.
      await page.click('#ef-stu .ef-step[data-l="max"]');
      assertEqual(await warning(page), null, "a second warning on a Max already chosen");
      assert((await rows(page)).note.startsWith("Rich directs your team on Opus at max effort, which is not recommended."), "the note names it");
      // Nothing was saved until now; the close says it.
      assertEqual(await calls(page, "set_lease_models"), [], "a command went out while Settings was open");
      await page.keyboard.press("Escape");
      const line = "Rich now directs your team on Opus at max effort, which is not recommended.";
      await toastSays(page, line);
      assertEqual(await toastText(page), line, "the line on close");
      assertEqual((await calls(page, "set_lease_models")).map((c) => c.args), [{ rich: "fable", stu: "opus", effort: withEffort({ stu: { opus: "max" } }), chosen: ["rich_effort_fable", "stu_effort_opus", "stu_model"] }], "the save");
      // A saved Max opens no warning when Settings opens.
      await openPrefs(page);
      await levelIs(page, "stu", "max");
      assertEqual([(await effort(page, "stu")).level, await warning(page)], ["max", null], "a saved Max opened a warning");
      assertEqual(page.__errors, [], "no page errors");
      await page.close();
    }
    return "both themes: his words, Escape, outside, End, Use Max anyway, the line on close";
  });

  // ---- 9. no jitter: the pointer over a step never moves the steps ------------------------------
  await run.check('9. the pointer over a step never moves the steps: no jitter over "Recommended" at any Text size or width', async () => {
    // His recording (CEO 2026-11-26, 1380px wide): Rich on Opus, the head "Effort on Opus" over
    // "Low · recommended for Opus" on a line of its own. The pointer just under the steps, above
    // "Recommended", previewed a level with a short name ("High"), which fits beside the label:
    // the head lost a line, the steps jumped up out from under the pointer, the preview ended,
    // the head took its line back, the steps came back under the pointer, and so on. Every level
    // of every row on both models is hovered here, at the bottom of its step where his pointer
    // was, and the steps must not move. The theme changes no geometry, so one theme is walked.
    const widths = [1024, 1280, 1380, 1440];
    const problems = [];
    let cases = 0;
    for (const width of widths) {
      const page = await openApp(browser, { viewport: { width, height: 900 }, reducedMotion: "reduce" });
      await openPrefs(page);
      await rowsShow(page, { rich: "fable", stu: "fable" });
      const scales = await page.evaluate(() => window.RichTheme.STEPS);
      for (const scale of scales) {
        for (const who of ["rich", "stu"]) {
          for (const model of ["opus", "fable"]) {
            await page.mouse.move(2, 2);
            const where = (w) => {
              const ef = document.getElementById("ef-" + w);
              const head = ef.querySelector(".ef-head").getBoundingClientRect();
              const label = ef.querySelector(".ef-label").getBoundingClientRect();
              const range = document.createRange();
              range.selectNodeContents(ef.querySelector(".ef-name"));
              const name = range.getBoundingClientRect();
              const steps = ef.querySelector(".ef-steps").getBoundingClientRect();
              return {
                stepsTop: steps.top,
                name: ef.querySelector(".ef-name").textContent,
                // On its own line under the label it starts where the label starts; beside the
                // label it ends where the head ends (round 86.1, and the app's narrow case).
                aligned: name.top >= label.bottom - 1 ? Math.abs(name.left - head.left) <= 1 : Math.abs(name.right - head.right) <= 1,
                at: [...ef.querySelectorAll(".ef-step")].map((s) => {
                  const r = s.getBoundingClientRect();
                  return { x: r.left + r.width / 2, y: steps.bottom - 2 };
                }),
              };
            };
            const base = await page.evaluate(
              async ([s, w, m, read]) => {
                window.RichTheme.setScale(s);
                document.querySelector(`#md-${w} .seg-opt[data-v="${m}"]`).click();
                document.getElementById("ef-" + w).scrollIntoView({ block: "center" });
                await new Promise((r) => requestAnimationFrame(() => requestAnimationFrame(r)));
                return new Function("return (" + read + ")")()(w);
              },
              [scale, who, model, where.toString()]
            );
            const tag = `${width}px ${scale}% ${who} on ${model}`;
            if (!base.aligned) problems.push(`${tag}: "${base.name}" is out of place in its head`);
            for (const [i, level] of ["Low", "Medium", "High", "Extra high", "Max"].entries()) {
              await page.mouse.move(base.at[i].x, base.at[i].y);
              const now = await page.evaluate(([w, read]) => new Function("return (" + read + ")")()(w), [who, where.toString()]);
              cases++;
              if (!now.name.startsWith(level)) problems.push(`${tag}: the pointer over ${level} previewed "${now.name}"`);
              if (Math.abs(now.stepsTop - base.stepsTop) > 0.5)
                problems.push(`${tag}: the pointer over ${level} ("${now.name}") moved the steps ${(now.stepsTop - base.stepsTop).toFixed(1)}px from under it ("${base.name}")`);
              if (!now.aligned) problems.push(`${tag}: "${now.name}" is out of place in its head`);
            }
          }
        }
      }
      await page.mouse.move(2, 2);
      assertEqual(page.__errors, [], "no page errors");
      await page.close();
    }
    assertEqual(problems, [], "the steps stay where they are under the pointer");
    return `${cases} hovers: ${widths.length} widths x 7 Text sizes x 2 rows x 2 models x 5 levels`;
  });

  await browser.close();
  const failed = run.report();
  process.exitCode = failed ? 1 : 0;
}

main().catch((e) => {
  console.error(e);
  process.exit(1);
});

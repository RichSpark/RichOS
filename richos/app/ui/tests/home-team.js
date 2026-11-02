// THE HOME SCREEN OF A USER WHO HAS A TEAM SHOWS HIS TEAM AND NOTHING MADE UP.
//
// Product hunt 2026-10-15 (richos-hq `docs/audits/2026-10-15-product-hunt/ray-opus-hunt57.md`),
// finding 7: after a relaunch the home screen showed "18 AI specialists, 6 working now",
// "1,596 tasks handled without you", "2,365 h of your attention saved", "7,508 memories" and
// six named specialists "working now" to a user who had just hired his first two teammates,
// and showed neither of them. The acceptance sentence: it shows only this user's real team and
// real numbers, and nothing made up.
//
// 2026-11-02 (CEO): the right-hand column is round 11.1's "Working now" again, listing ONLY the
// teammates working at this moment, cut off at the approved six lines with no "more" line, and
// not on the screen at all when nobody works. His whole team is never listed on the home screen.
//
// 2026-11-02 (CEO, round 11.4 "V2"): the picture behind those columns is no longer absent for a team
// with few records: it is "the ghost" (`home-ghost.js` holds that), so the dataset on the page is
// his, the example's shape is drawn dimmed behind him, and the left column lists what has arrived.
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
const { worstContrast } = require("./lib/worst-contrast");

const APP = "file://" + path.join(UI_DIR, "index.html");

/// The team `home_team` answers with: what Dean's template writes as `# {Name} — {Role Title}`.
const TEAM = [
  { name: "Beth", role: "Bookkeeper" },
  { name: "Will", role: "Marketing Director" },
];

/// Every number and name the demonstration draws, read from what the hunt saw on screen
/// (`hire/06-home-after-relaunch.png`). None of them may be on the screen of a user with a team.
const MADE_UP = ["1,596", "2,365", "7,508", "4,802", "430", "185 lessons", "14 months",
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
      .map((s) => Array.from(s.querySelector(".n").childNodes).map((c) => c.textContent.trim()).join(" ") + " " + s.querySelector(".l").textContent.trim())
      .join(" / ")
  );

async function main() {
  const run = createRun("the home screen of a user with a team: his team, his numbers, nothing made up");
  const { webkit } = loadPlaywright();
  const browser = await webkit.launch();

  for (const theme of ["dark", "light"]) {
    const page = await openApp(browser, theme, { homeTeam: TEAM, homeWorking: TEAM });
    await page.waitForFunction("window.RichHome.state.field === 'live' && !!window.RichHome.state.ghost", null, { timeout: 90000 }).catch(() => {});
    await page.waitForFunction("window.RichHome.state.workingNow === 2", null, { timeout: 30000 }).catch(() => {});

    await run.check(`${theme}: the teammates working now are on the home screen, by name and role`, async () => {
      const seen = await page.evaluate(() => ({
        field: window.RichHome.state.field,
        cap: document.querySelector("#home-live .cap").textContent.trim(),
        rows: Array.from(document.querySelectorAll("#home-working li")).map((li) => li.textContent.replace(/\s+/g, " ").trim()),
      }));
      seen.signals = await signalsOf(page);
      assertEqual(seen.field, "live", "a user with a team and none of his own records must get the ghost's picture");
      assertEqual(seen.rows, TEAM.map((t) => t.name + " " + t.role), "the list is not who is working");
      assertEqual(seen.cap, "Working now", "the column is not round 11.1's");
      assertEqual(seen.signals, "2 \u00b7 2 working now AI specialists", "the numbers are not his");
      return `"${seen.cap}": ${seen.rows.join(", ")}; "${seen.signals}"`;
    });

    await run.check(`${theme}: nothing made up is on the screen`, async () => {
      const text = await screenText(page);
      const found = MADE_UP.filter((w) => text.includes(w.toLowerCase()));
      assertEqual(found, [], "the demonstration's numbers or names are still on his screen");
      const loaded = await page.evaluate(() => ({
        synthetic: window.MATURE_LORO && window.MATURE_LORO.meta.synthetic,
        objects: window.MATURE_LORO && window.MATURE_LORO.meta.counts.objects,
        note: !document.getElementById("home-note") || document.getElementById("home-note").hidden,
      }));
      assertEqual(loaded.synthetic, false, "the dataset on the page does not say it is his");
      assertEqual(loaded.objects, 0, "the dataset counts objects he does not have");
      assert(loaded.note, "the banner calling the screen a demonstration is up over his own team");
      // The loading layer fades over 0.8 s (`home.css` `#home-loading.gone`) before it is hidden.
      const gone = await page
        .waitForFunction(() => getComputedStyle(document.getElementById("home-loading")).visibility === "hidden", null, { timeout: 60000 })
        .then(() => true, () => false);
      assert(gone, "\"waking loro…\" is left on a screen that has drawn its picture");
      return "no made-up number or name in the text, the dataset says it is his, no banner, no loading line";
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
    const page = await openApp(browser, "dark", { homeTeam: TEAM, homeWorking: TEAM, homeField: { objects: 4000, records: 9000 } });
    await page.waitForFunction("window.RichHome.state.field === 'live' && !!window.__loro", null, { timeout: 90000 });
    const seen = await page.evaluate(() => ({
      synthetic: window.MATURE_LORO.meta.synthetic,
      rows: Array.from(document.querySelectorAll("#home-working li")).map((li) => li.textContent.replace(/\s+/g, " ").trim()),
    }));
    seen.signals = await signalsOf(page);
    assertEqual(seen.synthetic, false, "the picture is not his loro");
    assertEqual(seen.rows, TEAM.map((t) => t.name + " " + t.role), "the picture's engine replaced his team");
    // his own objects: 4,000 of them in 500 groups, one decision each (the mock's own shape)
    assertEqual(seen.signals, "2 \u00b7 2 working now AI specialists / 500 sources understood / 500 decisions remembered", "the picture's engine replaced his numbers");
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
        window.__RICHOS_MOCK__.homeWorkingSet(t);
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
      assertEqual(seen.rows, want, "the home screen does not show who is working as it is now");
      assertEqual(seen.cap, "Working now", "the column is not round 11.1's");
      assertEqual(seen.signals, team.length + " \u00b7 " + team.length + " working now " + (team.length === 1 ? "AI specialist" : "AI specialists"), "the numbers are not his");
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

  /// The names a screen shows in its working column, as a person reads them.
  const columnOf = (page) =>
    page.evaluate(() => {
      const live = document.getElementById("home-live");
      return {
        shown: !live.hidden && getComputedStyle(live).display !== "none",
        cap: live.hidden ? "" : live.querySelector(".cap").textContent.trim(),
        rows: Array.from(document.querySelectorAll("#home-working li")).map((li) => li.textContent.replace(/\s+/g, " ").trim()),
        dots: document.querySelectorAll("#home-working li .dot").length,
      };
    });
  const bigTeam = Array.from({ length: 35 }, (_, i) => ({ name: "Mate" + String.fromCharCode(65 + (i % 26)) + i, role: "Role " + i }));

  for (const theme of ["dark", "light"]) {
    await run.check(`${theme}: nobody working: the column is not on the screen, and his 35 teammates are not listed`, async () => {
      const page = await openApp(browser, theme, { homeTeam: bigTeam });
      await page.waitForFunction("window.RichHome.state.team", null, { timeout: 30000 });
      const col = await columnOf(page);
      assertEqual(col.shown, false, "the working column is on the screen with nobody working");
      assertEqual(col.rows, [], "lines are in the column");
      const text = await screenText(page);
      assert(!text.includes("working now") && !text.includes("your team"), "a heading is on the screen with nothing under it");
      assert(!bigTeam.some((t) => text.includes(t.name.toLowerCase())), "his whole team is listed on the home screen");
      assertEqual(await signalsOf(page), "35 AI specialists", "the numbers are not his");
      assertEqual(page.__errors, [], "the page reported errors");
      await page.close();
      return "no column, no heading, none of the 35 names";
    });
  }

  await run.check("two working: exactly those two, each with role and gold dot", async () => {
    const two = [bigTeam[3], bigTeam[20]];
    const page = await openApp(browser, "dark", { homeTeam: bigTeam, homeWorking: two });
    await page.waitForFunction("document.querySelectorAll('#home-working li').length > 0", null, { timeout: 30000 });
    const col = await columnOf(page);
    assertEqual(col.shown, true, "the column is not on the screen");
    assertEqual(col.cap, "Working now", "the heading");
    assertEqual(col.rows, two.map((t) => t.name + " " + t.role), "the names");
    assertEqual(col.dots, 2, "a gold dot beside each");
    // One company has no entity row and no banner above the column: its heading must still clear
    // the settings button (seen in the test VM 2026-11-02: "WORKING NO" cut off beneath it).
    const clear = await page.evaluate(() => {
      // One company is no entity row: the inset the row adds is 0.
      document.getElementById("home").style.setProperty("--home-top-inset", "0px");
      const cap = document.querySelector("#home-live .cap").getBoundingClientRect();
      const btn = document.getElementById("set-btn").getBoundingClientRect();
      return { capTop: Math.round(cap.top), buttonBottom: Math.round(btn.bottom), overlap: !(cap.bottom <= btn.top || cap.top >= btn.bottom || cap.right <= btn.left || cap.left >= btn.right) };
    });
    assertEqual(clear.overlap, false, "the heading is under the settings button: " + JSON.stringify(clear));
    assertEqual(page.__errors, [], "the page reported errors");
    await page.close();
    return col.rows.join(", ");
  });

  await run.check("more working than the column holds: cut off at six lines, no \"more\" line", async () => {
    const nine = bigTeam.slice(0, 9);
    const page = await openApp(browser, "dark", { homeTeam: bigTeam, homeWorking: nine });
    await page.waitForFunction("document.querySelectorAll('#home-working li').length > 0", null, { timeout: 30000 });
    const col = await columnOf(page);
    assertEqual(col.rows, nine.slice(0, 6).map((t) => t.name + " " + t.role), "the first six, in order");
    const text = await screenText(page);
    assert(!text.includes(nine[6].name.toLowerCase()), "a seventh teammate is on the screen");
    assert(!/\d+ more|\bmore\.\.\.|…/.test(await page.evaluate(() => document.getElementById("home-live").innerText.toLowerCase())), "the column carries a \"more\" line");
    await page.close();
    return "six lines, no more line";
  });

  await run.check("it updates while the screen is open, as teammates start and finish", async () => {
    const page = await openApp(browser, "dark", { homeTeam: bigTeam });
    await page.waitForFunction("window.RichHome.state.team", null, { timeout: 30000 });
    const settle = async (want) => {
      await page
        .waitForFunction((w) => JSON.stringify(Array.from(document.querySelectorAll("#home-working li")).map((li) => li.textContent.replace(/\s+/g, " ").trim())) === JSON.stringify(w), want, { timeout: 90000 })
        .catch(() => {});
      return columnOf(page);
    };
    assertEqual((await columnOf(page)).shown, false, "the column starts on the screen");
    const a = [bigTeam[1], bigTeam[2]].map((t) => t.name + " " + t.role);
    await page.evaluate((w) => window.__RICHOS_MOCK__.homeWorkingSet(w), [bigTeam[1], bigTeam[2]]);
    const two = await settle(a);
    assertEqual([two.shown, two.rows], [true, a], "two started and are not on the screen");
    const b = [bigTeam[2]].map((t) => t.name + " " + t.role);
    await page.evaluate((w) => window.__RICHOS_MOCK__.homeWorkingSet(w), [bigTeam[2]]);
    const one = await settle(b);
    assertEqual([one.shown, one.rows], [true, b], "one finished and is still on the screen");
    await page.evaluate(() => window.__RICHOS_MOCK__.homeWorkingSet([]));
    const none = await settle([]);
    assertEqual([none.shown, none.rows], [false, []], "the last finished and the column is still on the screen");
    assertEqual(page.__errors, [], "the page reported errors");
    await page.close();
    return "0 -> 2 -> 1 -> 0 without a reload";
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

// THE HOME SCREEN FROM THE FIRST LAUNCH TO A PICTURE WORTH POSTING (round 75).
//
// CEO, 2026-11-14, on `richos-hq/design/mockups/rounds/round-75`: "Yes, build it with the picked
// defaults." The picked defaults: his lights in their finished color from stage 3, stage names
// Set A with "Self-Made" for the last stage, the distance line under the brand line without
// repeating the count, the stage name on the last frame of "Watch it grow".
//
// One check per moment the round shows (`home-growth.js` in the app; the numbers are the design's
// sections): the invitation (2.1), the handover at the first hire (2.2), "See it finished" (2.3),
// the stages and what each switches on (3.1, 3.2), the distance line and its card (3.3), new lights
// arriving when he comes home (3.4), "Watch it grow" (3.5), a stage crossed and the look ahead
// (3.6, 4.4), and the weekly line (4.3). Every new line of text is measured against what is behind
// it (WCAG AA; the home screen is always dark, so one theme).
//
// The clock is fixed at Wednesday 2026-11-18, 10:00 local, so "this morning" and "last week" have
// one answer. The dataset is the browser stand-in's (`mock.js`, `homeField`): N objects over 67
// days in three companies, the first of which is the largest.
//
// Run: node home-growth.js   (or `npm test` for every suite in this directory)

"use strict";

const path = require("path");
const { loadPlaywright, createRun, assert, assertEqual, SEED_THEME, UI_DIR } = require("./lib/harness");
const { worstContrast } = require("./lib/worst-contrast");

const APP = "file://" + path.join(UI_DIR, "index.html");
const TEAM = [
  { name: "Beth", role: "Bookkeeper" },
  { name: "Will", role: "Marketing Director" },
];
const NOW = new Date(2026, 10, 18, 10, 0, 0); // a Wednesday
const KEY = "richos-home-growth";
/// This week's Monday as the app names a week (`home-growth.js`, `weekOf`).
const WEEK = Math.round(new Date(2026, 10, 16).getTime() / 86400000);
const field = (objects, more) => Object.assign({ objects, days: 67, lanes: 3 }, more || {});
/// A returning user who has seen everything up to `count` memories: no moment is owed.
const seenAt = (count, stage, more) => Object.assign({ week: WEEK, stage, look: { count, at: NOW.getTime() - 3 * 3600 * 1000 } }, more || {});

async function openApp(browser, preset, store) {
  const page = await browser.newPage({ viewport: { width: 1440, height: 900 } });
  const errors = [];
  page.on("pageerror", (e) => errors.push(String(e)));
  page.on("console", (m) => {
    if (m.type() === "error") errors.push("console: " + m.text());
  });
  page.__errors = errors;
  await page.clock.setFixedTime(NOW);
  await page.addInitScript(SEED_THEME, "dark");
  await page.addInitScript(
    ([v, key, st]) => {
      window.__RICHOS_MOCK_PRESET__ = v;
      // once per page: a reload keeps what the app wrote
      if (st && !window.sessionStorage.getItem("seeded")) {
        window.localStorage.setItem(key, st);
        window.sessionStorage.setItem("seeded", "1");
      }
    },
    [preset, KEY, store ? JSON.stringify(store) : ""]
  );
  await page.goto(APP);
  await up(page);
  return page;
}

/// The picture is up and the app has decided what to play.
async function up(page) {
  await page.waitForFunction("typeof window.RichHome === 'object' && typeof window.RichTimeline === 'object'", null, { timeout: 60000 });
  await page.evaluate(() => window.RichSplash && window.RichSplash.yieldNow("acceptance-suite"));
  await page.evaluate(() => window.RichHome.startField());
  await page.waitForFunction("window.RichHome.state.field === 'live' && !!window.__loro", null, { timeout: 60000 });
}
const arrived = (page) => page.waitForFunction("window.RichHomeGrowth.state().arrived === true", null, { timeout: 60000 });
const growth = (page) => page.evaluate(() => window.RichHomeGrowth.state());
const stored = (page) => page.evaluate((k) => JSON.parse(window.localStorage.getItem(k) || "{}"), KEY);
const countIs = (page, n) => page.waitForFunction((want) => window.__loro && window.__loro.COUNT === want, n, { timeout: 60000 });
const text = (page, sel) => page.evaluate((s) => (document.querySelector(s) ? document.querySelector(s).textContent.replace(/\s+/g, " ").trim() : null), sel);
/// Is the element painted: laid out, visible and not transparent?
const shown = (page, sel) =>
  page.evaluate((s) => {
    const e = document.querySelector(s);
    if (!e || e.hidden) return false;
    const cs = getComputedStyle(e);
    const r = e.getBoundingClientRect();
    return cs.display !== "none" && cs.visibility === "visible" && Number(cs.opacity) > 0.5 && r.width > 0 && r.height > 0;
  }, sel);
/// The same, given the time a fade takes (the sentence fades in over 0.9 s, the caption over 0.6 s).
const PAINTED = (s) => {
  const e = document.querySelector(s);
  if (!e || e.hidden) return false;
  const cs = getComputedStyle(e);
  const r = e.getBoundingClientRect();
  return cs.display !== "none" && cs.visibility === "visible" && Number(cs.opacity) > 0.5 && r.width > 0 && r.height > 0;
};
const appears = (page, sel) => page.waitForFunction(PAINTED, sel, { timeout: 60000 }).then(() => true, () => false);
/// Gone, given the time a fade out takes.
const gone = (page, sel) =>
  page
    .waitForFunction((s) => {
      const e = document.querySelector(s);
      if (!e || e.hidden) return true;
      const cs = getComputedStyle(e);
      return cs.display === "none" || cs.visibility !== "visible" || Number(cs.opacity) <= 0.5;
    }, sel, { timeout: 60000 })
    .then(() => true, () => false);
const levels = (page) =>
  page.evaluate(() => {
    const g = window.__loro.growth;
    const x = g.X;
    const round = (v) => Math.round(v * 100) / 100;
    return { threads: round(x.threads), river: round(x.river), core: round(x.core), color: round(x.color), done: round(x.done), lanes: Array.from(g.laneOn).filter((v) => v > 0.99).length, ex: round(g.ex) };
  });
/// The engine's levels have eased to `want` (a cold open switches the pieces on as the lights land).
async function leveled(page, want) {
  await page
    .waitForFunction(
      (w) => {
        const g = window.__loro.growth;
        const x = g.X;
        return ["threads", "river", "core", "color", "done"].every((k) => Math.abs(x[k] - w[k]) < 0.005) && Array.from(g.laneOn).filter((v) => v > 0.99).length === w.lanes;
      },
      want,
      { timeout: 60000 }
    )
    .catch(() => {});
  const got = await levels(page);
  delete got.ex;
  return got;
}
/// Every measured line clears its floor; returns the measurements as text for the report.
async function clears(page, selectors) {
  const rows = await worstContrast(page, selectors);
  assert(rows.length > 0, "nothing was measured for " + selectors.join(", "));
  assertEqual(rows.filter((r) => !(r.ratio >= r.needs)), [], "text below its contrast floor");
  const worst = rows.reduce((a, b) => (b.ratio - b.needs < a.ratio - a.needs ? b : a));
  return `${rows.length} line(s), the nearest to its floor "${worst.text.slice(0, 28)}" ${worst.ratio}:1 (needs ${worst.needs})`;
}

async function main() {
  const run = createRun("the home screen's stages: round 75, from the first launch to a picture worth posting");
  const { webkit } = loadPlaywright();
  const browser = await webkit.launch();

  // ---- 2.1 -------------------------------------------------------------------------------------
  await run.check("first launch: the banner invites him to move through the example, and nothing else of the stages is on the screen", async () => {
    const page = await openApp(browser, {}, null);
    assertEqual(await text(page, "#home-note-line"), await page.evaluate(() => window.RichHome.NOTE), "the banner's own sentence changed");
    assertEqual(await text(page, "#home-note .grow-invite"), "Move through it. Click a light.", "the invitation is not under the banner");
    assert(await appears(page, "#home-note .grow-invite"), "the invitation is not painted");
    const under = await page.evaluate(() => {
      const bottom = (s) => document.querySelector(s).getBoundingClientRect().bottom;
      const top = (s) => document.querySelector(s).getBoundingClientRect().top;
      return [top("#home-note .grow-invite") >= bottom("#home-note-line") - 1, top("#home-live") >= bottom("#home-note .grow-invite") + 20];
    });
    assertEqual(under, [true, true], "the invitation is not under the sentence and clear of the column below it");
    for (const sel of ["#home-distance", "#home-growth-card", "#home-grow", "#home-finished", "#home-moment", "#home-caption", "#home-replay", "#home-grow-solo", ".grow-example-word"]) {
      assertEqual(await shown(page, sel), false, sel + " is on a first-run screen");
    }
    assertEqual((await stored(page)).example, true, "the app did not keep that the example was on this screen");
    const contrast = await clears(page, ["#home-note .grow-invite"]);
    assertEqual(page.__errors, [], "the page reported errors");
    await page.close();
    return "\"Move through it. Click a light.\" under the banner; " + contrast;
  });

  // ---- 3.1, 3.2, 3.3 ---------------------------------------------------------------------------
  const STAGES = [
    { n: 1, line: "29 to the threads", want: { threads: 0, river: 0, core: 0, color: 0, done: 0, lanes: 0 } },
    { n: 30, line: "70 to first color", want: { threads: 1, river: 0, core: 0, color: 0, done: 0, lanes: 0 } },
    { n: 100, line: "200 to the river", want: { threads: 1, river: 0, core: 0, color: 1, done: 0, lanes: 1 } },
    { n: 300, line: "700 to full color", want: { threads: 1, river: 1, core: 0, color: 1, done: 0, lanes: 1 } },
    { n: 1000, line: "1,500 to Self-Made", want: { threads: 1, river: 1, core: 1, color: 1, done: 0, lanes: 3 } },
    { n: 2500, line: null, want: { threads: 1, river: 1, core: 1, color: 1, done: 1, lanes: 3 } },
  ];
  for (const st of STAGES) {
    await run.check(`${st.n.toLocaleString("en-US")} memories: the stage's pieces are on, and the line says how far the next stage is`, async () => {
      const page = await openApp(browser, { homeTeam: TEAM, homeField: field(st.n) }, seenAt(st.n, STAGES.indexOf(st) + 1));
      await arrived(page);
      await countIs(page, st.n);
      const got = await leveled(page, st.want);
      assertEqual(got, st.want, "the pieces switched on are not this stage's");
      assertEqual(await text(page, "#home-brand-line"), "loro · " + st.n.toLocaleString("en-US") + (st.n === 1 ? " memory" : " memories"), "the brand line is not his count");
      if (st.line) {
        assertEqual(await text(page, "#home-distance"), st.line + "›", "the distance line is wrong");
        assert(!(await text(page, "#home-distance")).includes("memor"), "the distance line repeats the count the brand line already says");
        assert(await appears(page, "#home-distance"), "the distance line is not painted");
        assertEqual(await text(page, "#home-legend .row .tx b"), "Yours, in gold and in color", "the legend does not say what color means");
        assert((await appears(page, "#home-grow")) && (await appears(page, "#home-finished")), "the two presses are not on the legend");
        assertEqual(await shown(page, "#home-grow-solo"), false, "a second Watch it grow is up beside the legend's");
      } else {
        // the last stage: the count stands alone, the example and its legend are gone, the replay stays
        assertEqual(await shown(page, "#home-distance"), false, "a distance line is up after the last stage");
        assertEqual(await page.evaluate(() => document.getElementById("home-legend").classList.contains("on")), false, "the example's legend is up at 2,500");
        assert(await appears(page, "#home-grow-solo"), "Watch it grow is gone with the legend");
      }
      const contrast = await clears(page, st.line ? ["#home-distance", "#home-grow", "#home-finished"] : ["#home-grow-solo"]);
      assertEqual(page.__errors, [], "the page reported errors");
      await page.close();
      return `${JSON.stringify(got)}; "${st.line || "(the count alone)"}"; ${contrast}`;
    });
  }

  await run.check("the territory that earned the first color keeps it, whatever arrives later", async () => {
    // by today's numbers the largest of the first hundred is Northwind; Alder is the one written
    // down when the first color came
    const page = await openApp(browser, { homeTeam: TEAM, homeField: field(186) }, seenAt(186, 3, { firstLane: "alder" }));
    await arrived(page);
    await countIs(page, 186);
    await leveled(page, { threads: 1, river: 0, core: 0, color: 1, done: 0, lanes: 1 });
    const lit = await page.evaluate(() => window.__loro.domains.filter((d, i) => window.__loro.growth.laneOn[i] > 0.99).map((d) => d.id));
    assertEqual(lit, ["own-alder"], "the color moved to another territory");
    await page.close();
    // and a user reaching stage 3 has his written down
    const fresh = await openApp(browser, { homeTeam: TEAM, homeField: field(120) }, seenAt(120, 3));
    await arrived(fresh);
    assertEqual((await stored(fresh)).firstLane, "northwind", "the first territory was not kept");
    await fresh.close();
    return "kept: Alder stays in color; a new stage 3 writes down Northwind";
  });

  await run.check("the card behind the distance line: six stages, the next one says what it switches on, and it opens and closes by pointer and by key", async () => {
    const page = await openApp(browser, { homeTeam: TEAM, homeField: field(186) }, seenAt(186, 3));
    await arrived(page);
    await countIs(page, 186);
    assertEqual(await shown(page, "#home-growth-card"), false, "the card is open before anything was pressed");
    await page.click("#home-distance");
    assert(await appears(page, "#home-growth-card"), "a press on the distance line did not open the card");
    assertEqual(await page.getAttribute("#home-distance", "aria-expanded"), "true", "the line does not say its card is open");
    const rows = await page.evaluate(() => Array.from(document.querySelectorAll("#home-growth-card li")).map((li) => li.className + ": " + li.textContent.replace(/\s+/g, " ").trim()));
    assertEqual(rows, [
      "reached: One Light · 1 memory · the first light",
      "reached: The Threads · 30 memories · the lights are joined",
      "reached: First Color · 100 memories · the first territory in its own color",
      "next: The River · at 300 memories · the river runs from the sources",
      "ahead: Full Color · at 1,000 memories",
      "ahead: Self-Made · at 2,500 memories",
    ], "the card's rows are not the six stages at 186 memories");
    assertEqual(await text(page, "#home-growth-card h2"), "How this picture grows", "the card's heading");
    const grows = await text(page, "#home-growth-card .grows");
    assert(/^It grows from real work with Rich: every decision made, every source handed to Rich,? every job a specialist finishes\. Nothing in it is made up\. Nothing in it is ever taken away\.$/.test(grows), "the three sentences: " + grows);
    const contrast = await clears(page, ["#home-growth-card"]);
    // Escape closes the card and nothing else: the home screen stays
    await page.keyboard.press("Escape");
    assertEqual(await shown(page, "#home-growth-card"), false, "Escape did not close the card");
    assert(await page.evaluate(() => window.RichHome.isOpen()), "Escape on the card left the home screen");
    // the keyboard opens it as the pointer does, and does not go through the door
    await page.focus("#home-distance");
    await page.keyboard.press("Enter");
    assert(await appears(page, "#home-growth-card"), "Enter on the distance line did not open the card");
    assert(await page.evaluate(() => window.RichHome.isOpen()), "Enter on the distance line opened Talk to Rich");
    // a press anywhere else closes it
    await page.mouse.click(720, 300);
    assertEqual(await shown(page, "#home-growth-card"), false, "a press elsewhere did not close the card");
    assertEqual(page.__errors, [], "the page reported errors");
    await page.close();
    return "six rows, three reached, The River next with what it switches on; pointer, Enter, Escape, a press elsewhere; " + contrast;
  });

  // ---- 2.3 -------------------------------------------------------------------------------------
  await run.check("See it finished: the example in full for ten seconds, named Example throughout, his lights on top, then back to the outline", async () => {
    const page = await openApp(browser, { homeTeam: TEAM, homeField: field(186) }, seenAt(186, 3));
    await arrived(page);
    await countIs(page, 186);
    // the example's dim floor at 186 of his: 0.155 x (1 - 186 / 2500) = 0.1435
    const atFloor = await page.waitForFunction(() => Math.abs(window.__loro.growth.ex - 0.1435) < 0.003, null, { timeout: 60000 }).then(() => true, () => false);
    const floor = (await levels(page)).ex;
    assert(atFloor, "the example is not at its dim floor before the press: " + floor);
    assertEqual(await shown(page, "#home-note"), false, "the first-run banner is up over his own screen");
    const t0 = Date.now();
    await page.click("#home-finished");
    await page.waitForFunction(() => window.__loro.growth.ex > 0.95, null, { timeout: 60000 });
    assert(await appears(page, "#home-note"), "the banner sentence is not on screen while the example is lit");
    assertEqual(await text(page, "#home-note-line"), await page.evaluate(() => window.RichHome.NOTE), "the banner does not carry the app's sentence");
    assert(await appears(page, ".grow-example-word"), "the word Example is not on screen while the example is lit");
    assertEqual(await text(page, ".grow-example-word"), "Example", "the word is not Example");
    assertEqual(await text(page, "#home-finished"), "End the example", "the press does not offer the way out");
    assertEqual(await page.evaluate(() => { const lo = window.__loro; let on = 0; for (let i = 0; i < lo.N; i++) if (!lo.nodes[i].ghost && lo.gate[i] > 0.99) on++; return on; }), 186, "his own lights did not stay on top");
    const contrast = await clears(page, [".grow-example-word", "#home-note-line", "#home-finished"]);
    // it ends by itself, ten seconds after the press
    await page.waitForFunction(() => window.RichHomeGrowth.state().finished === false, null, { timeout: 60000 });
    const held = (Date.now() - t0) / 1000;
    assertEqual(await shown(page, "#home-note"), false, "the banner stayed after the example drained");
    await page.waitForFunction(() => Math.abs(window.__loro.growth.ex - 0.1435) < 0.01, null, { timeout: 60000 });
    assertEqual(await text(page, "#home-finished"), "See it finished", "the press did not go back to its own words");
    // ended early by its own button, and by Escape (the home screen stays)
    await page.click("#home-finished");
    await page.waitForFunction(() => window.__loro.growth.ex > 0.5, null, { timeout: 60000 });
    await page.click("#home-finished");
    assertEqual((await growth(page)).finished, false, "the same button did not end the example");
    await page.click("#home-finished");
    await page.keyboard.press("Escape");
    assertEqual((await growth(page)).finished, false, "Escape did not end the example");
    assert(await page.evaluate(() => window.RichHome.isOpen()), "Escape left the home screen");
    assertEqual(page.__errors, [], "the page reported errors");
    await page.close();
    return `held ${held.toFixed(1)} s, back to ${floor}; ended by itself, by its button and by Escape; ${contrast}`;
  });

  // ---- 3.5 -------------------------------------------------------------------------------------
  await run.check("Watch it grow: from the first light to today, each stage switching on, ending on the day, the count and the stage's name", async () => {
    const page = await openApp(browser, { homeTeam: TEAM, homeField: field(40) }, seenAt(40, 2));
    await arrived(page);
    await countIs(page, 40);
    const t0 = Date.now();
    await page.click("#home-grow");
    assertEqual((await growth(page)).replaying, true, "the press did not start the replay");
    assertEqual(await text(page, "#home-grow"), "Stop", "the press does not offer the way out");
    // (the press itself empties the picture and switches every piece off: read at once, as set)
    assertEqual(await page.evaluate(() => window.__loro.growth.XT.threads), 0, "the replay does not start before the threads");
    // partway: one light at a time, the counter under the picture, no stage yet
    await page.waitForFunction(() => window.__loro.COUNT >= 3 && window.__loro.COUNT < 30, null, { timeout: 60000 });
    const mid = await text(page, "#home-replay .line");
    assert(/^Day \d+ · \d+ memories$/.test(mid), "the counter partway: " + mid);
    assert(await appears(page, "#home-replay"), "the counter is not painted");
    assertEqual(await shown(page, "#home-arrived"), false, "the Newest caption is up during the replay");
    // the threads switch on when the thirtieth light lands
    await page.waitForFunction(() => window.__loro.COUNT >= 30, null, { timeout: 60000 });
    assertEqual(await text(page, "#home-replay .stage"), "The Threads", "the stage's name is not over the counter once it is reached");
    await page.waitForFunction(() => window.__loro.growth.X.threads > 0.9, null, { timeout: 60000 });
    // the last frame
    await page.waitForFunction(() => document.getElementById("home-replay").classList.contains("last"), null, { timeout: 60000 });
    const ran = (Date.now() - t0) / 1000;
    assertEqual(await text(page, "#home-replay .line"), "Day 67 · 40 memories · The Threads", "the last frame");
    const contrast = await clears(page, ["#home-replay .line"]);
    // and it ends by itself on today's picture
    await page.waitForFunction(() => window.RichHomeGrowth.state().replaying === false, null, { timeout: 60000 });
    assertEqual(await page.evaluate(() => window.__loro.COUNT), 40, "the replay did not end on today");
    assertEqual(await text(page, "#home-grow"), "Watch it grow", "the press did not go back to its own words");
    assert(await gone(page, "#home-replay"), "the counter stayed after the replay");
    // stopped early by its own button, by Escape, and by leaving the screen: today's picture each time
    for (const stop of ["button", "Escape", "leaving"]) {
      await page.click("#home-grow");
      await page.waitForFunction(() => window.__loro.COUNT < 40, null, { timeout: 60000 });
      if (stop === "button") await page.click("#home-grow");
      else if (stop === "Escape") await page.keyboard.press("Escape");
      else await page.evaluate(() => window.RichHome.hide("enter-app"));
      const g = await growth(page);
      assertEqual([g.replaying, g.count], [false, 40], "the replay was not stopped on today by " + stop);
      if (stop === "Escape") assert(await page.evaluate(() => window.RichHome.isOpen()), "Escape left the home screen");
    }
    assertEqual(page.__errors, [], "the page reported errors");
    await page.close();
    return `${ran.toFixed(1)} s to "Day 67 · 40 memories · The Threads"; stopped by its button, Escape and leaving; ${contrast}`;
  });

  await run.check("Watch it grow leaves the day out when the app cannot vouch for it", async () => {
    // records that existed before the app kept dates (`home_field.rs`, `meta.dates.undated`)
    const page = await openApp(browser, { homeTeam: TEAM, homeField: field(12, { undated: 12 }) }, seenAt(12, 1));
    await arrived(page);
    await countIs(page, 12);
    await page.click("#home-grow");
    await page.waitForFunction(() => document.getElementById("home-replay").classList.contains("last"), null, { timeout: 60000 });
    assertEqual(await text(page, "#home-replay .line"), "12 memories · One Light", "the last frame claims a day count");
    await page.close();
    return "\"12 memories · One Light\"";
  });

  // ---- 2.2 -------------------------------------------------------------------------------------
  await run.check("the first hire: the example drains to its outline, his first light comes on, one sentence, once", async () => {
    const page = await openApp(browser, { homeTeam: TEAM, homeField: field(48) }, { example: true });
    await arrived(page);
    const g0 = await growth(page);
    assertEqual([g0.scene, g0.handover, g0.count], ["handover", true, 0], "the handover is not what plays for a first hire");
    const full = await page.waitForFunction(() => window.__loro.growth.ex > 0.95, null, { timeout: 60000 }).then(() => true, () => false);
    assert(full, "the example is not in full when the handover starts");
    assertEqual(await shown(page, "#home-ghost-note"), false, "the line under the core did not wait for the handover");
    const t0 = Date.now();
    await page.waitForFunction(() => window.RichHomeGrowth.state().handover === false, null, { timeout: 60000 });
    const took = (Date.now() - t0) / 1000;
    const ex = (await levels(page)).ex;
    assert(ex < 0.35, "the example has not drained by the time the first light is on: " + ex);
    const line = await text(page, "#home-moment .line");
    assert(/^That was the example\. This light is the first of the real ones,? and the shape behind it is where they are going\.$/.test(line), "the sentence: " + line);
    assert(await appears(page, "#home-moment"), "the sentence is not painted");
    assertEqual(await shown(page, "#home-moment .offer"), false, "the handover offers the replay");
    assertEqual(await page.evaluate(() => window.__loro.COUNT), 1, "the first light is not alone when the sentence comes");
    const contrast = await clears(page, ["#home-moment .line"]);
    // the rest of what he already has arrives behind it
    await countIs(page, 48);
    assertEqual((await stored(page)).handover, true, "the app did not keep that the handover has played");
    assertEqual(await page.evaluate(() => window.__RICHOS_MOCK__.homeStageNotices), [{ stage: 2, name: "The Threads", count: 48 }], "Rich was not told of the stage the first hire begins at");
    // once per install
    await page.reload();
    await up(page);
    await arrived(page);
    assertEqual((await growth(page)).scene, "", "the handover played a second time");
    assertEqual(page.__errors, [], "the page reported errors");
    await page.close();
    return `${took.toFixed(1)} s, the example at ${ex} when the first light is on; ${contrast}; not played again after a reload`;
  });

  await run.check("the handover never holds up a key, and a user who never saw the example is not shown it", async () => {
    const page = await openApp(browser, { homeTeam: TEAM, homeField: field(48) }, { example: true });
    await arrived(page);
    assertEqual((await growth(page)).handover, true, "the handover did not start");
    await page.keyboard.press("a");
    const g = await growth(page);
    assertEqual(g.handover, false, "a key did not end the handover");
    assert((await text(page, "#home-moment .line")).startsWith("That was the example."), "the skipped handover lost its sentence");
    assertEqual(await page.evaluate(() => window.__loro.COUNT), 1, "the skipped handover lost the first light");
    await page.close();
    // a user who had his team before this was built: no example was kept, so nothing is handed over
    const old = await openApp(browser, { homeTeam: TEAM, homeField: field(48) }, null);
    await arrived(old);
    assertEqual((await growth(old)).scene, "", "a handover played for a user who never saw the example");
    await countIs(old, 48);
    await old.close();
    return "any key jumps to the first light and the sentence; no handover without the example";
  });

  await run.check("the first hire made this session: he comes back to the home screen and the handover plays there", async () => {
    const page = await openApp(browser, { homeField: field(48) }, null);
    assertEqual((await stored(page)).example, true, "the first-run example was not kept");
    await page.evaluate((t) => {
      window.RichHome.hide("enter-app");
      window.__RICHOS_MOCK__.homeTeamSet(t);
    }, TEAM);
    await page.waitForFunction(() => document.getElementById("home").hidden === true, null, { timeout: 60000 });
    await page.evaluate(() => window.RichHome.show("wordmark"));
    await page.waitForFunction(() => window.RichHomeGrowth.state().scene === "handover" && window.RichHomeGrowth.state().arrived, null, { timeout: 60000 });
    await page.waitForFunction(() => window.RichHomeGrowth.state().handover === false, null, { timeout: 60000 });
    assert((await text(page, "#home-moment .line")).startsWith("That was the example."), "no sentence after the handover");
    await countIs(page, 48);
    assertEqual(page.__errors, [], "the page reported errors");
    await page.close();
    return "hired from the app, came home: the example drained and 48 lights arrived, no relaunch";
  });

  // ---- 3.6, 4.4 --------------------------------------------------------------------------------
  await run.check("a stage crossed: the last lights arrive, the river switches on, its name and sentence, the offer, then a look ahead", async () => {
    const page = await openApp(browser, { homeTeam: TEAM, homeField: field(300) }, seenAt(295, 3));
    await arrived(page);
    const g0 = await growth(page);
    assertEqual([g0.scene, g0.count], ["crossed", 295], "the crossing is not what plays");
    assertEqual((await levels(page)).river, 0, "the river is on before he has watched it switch on");
    await page.waitForFunction(() => document.getElementById("home-moment").classList.contains("on"), null, { timeout: 60000 });
    assertEqual(await page.evaluate(() => window.__loro.COUNT), 300, "the last lights had not arrived when the stage's name came");
    assertEqual(await text(page, "#home-moment .title"), "The River", "the stage's name");
    assertEqual(await text(page, "#home-moment .line"), "The river is running from the sources.", "the stage's sentence");
    assert(await appears(page, "#home-moment .offer"), "the crossing does not offer Watch it grow");
    assertEqual(await text(page, "#home-moment .offer"), "Watch it grow", "the offer's words");
    await page.waitForFunction(() => window.__loro.growth.X.river > 0.9, null, { timeout: 60000 });
    assertEqual((await stored(page)).stage, 4, "the app did not keep that he has seen this crossing");
    // Rich is told, once: which stage and the count, and no words of his
    assertEqual(await page.evaluate(() => window.__RICHOS_MOCK__.homeStageNotices), [{ stage: 4, name: "The River", count: 300 }], "Rich was not told of the stage reached");
    // 4.4: the next stage's piece, faint, with its name
    await page.waitForFunction(() => document.querySelector("#home-moment .next").textContent !== "", null, { timeout: 60000 });
    assertEqual(await text(page, "#home-moment .next"), "Next: Full Color, at 1,000 memories", "the look ahead's words");
    const peek = await levels(page);
    const contrast = await clears(page, ["#home-moment .title", "#home-moment .line", "#home-moment .next", "#home-moment .offer"]);
    await page.waitForFunction(() => document.querySelector("#home-moment .next").textContent === "", null, { timeout: 60000 });
    const after = await leveled(page, { threads: 1, river: 1, core: 0, color: 1, done: 0, lanes: 1 });
    assertEqual([after.river, after.core, after.lanes], [1, 0, 1], "the picture did not come back to its own stage after the look ahead");
    // the sentence is for that visit only
    await page.evaluate(() => window.RichHome.hide("enter-app"));
    await page.waitForFunction(() => document.getElementById("home").hidden === true, null, { timeout: 60000 });
    await page.evaluate(() => window.RichHome.show("wordmark"));
    assert(await gone(page, "#home-moment"), "the crossing's sentence is still up on the next visit");
    await page.reload();
    await up(page);
    await arrived(page);
    assertEqual((await growth(page)).scene, "", "the crossing played a second time");
    assertEqual(page.__errors, [], "the page reported errors");
    await page.close();
    return `"The River": the river from 0 to 1, then "Next: Full Color, at 1,000 memories" with the core at ${peek.core}; gone on the next visit; ${contrast}`;
  });

  await run.check("a crossing passed by more than a few memories still switches its piece on while he watches", async () => {
    // 295 when he last looked, 320 now: the picture starts from the 295 he saw, all 25 new lights
    // arrive, and the river is off until its moment
    const page = await openApp(browser, { homeTeam: TEAM, homeField: field(320) }, seenAt(295, 3));
    await page.waitForFunction("!!window.RichHomeGrowth.state().active && !!window.__loro && !!window.__loro.growth", null, { timeout: 60000 });
    const before = await page.evaluate(() => ({ count: window.VARIANT.count, river: window.__loro.growth.XT.river, x: window.__loro.growth.X.river }));
    assertEqual(before, { count: 295, river: 0, x: 0 }, "the picture does not start from what he last saw with the river off");
    await arrived(page);
    await page.waitForFunction(() => document.getElementById("home-moment").classList.contains("on"), null, { timeout: 60000 });
    assertEqual(await text(page, "#home-moment .title"), "The River", "the stage's name");
    await page.waitForFunction(() => window.__loro.growth.X.river > 0.9, null, { timeout: 60000 });
    await page.close();
    return "starts at the 295 he last saw with the river off; The River switches it on";
  });

  await run.check("the offer under a crossing starts the replay", async () => {
    const page = await openApp(browser, { homeTeam: TEAM, homeField: field(30) }, seenAt(26, 1));
    await arrived(page);
    await page.waitForFunction(() => document.getElementById("home-moment").classList.contains("on"), null, { timeout: 60000 });
    assertEqual(await text(page, "#home-moment .title"), "The Threads", "the stage's name");
    await page.click("#home-moment .offer");
    assertEqual((await growth(page)).replaying, true, "the offer did not start the replay");
    assert(await gone(page, "#home-moment"), "the sentence stayed over the replay");
    await page.keyboard.press("Escape");
    assertEqual(await page.evaluate(() => window.__loro.COUNT), 30, "the replay did not stop on today");
    const back = await page.waitForFunction(() => window.__loro.growth.X.threads > 0.9, null, { timeout: 60000 }).then(() => true, () => false);
    assert(back, "the threads did not come back after the replay");
    await page.close();
    return "The Threads, Watch it grow, Escape: today's picture";
  });

  // ---- 3.4 -------------------------------------------------------------------------------------
  await run.check("coming home: the lights that are new since he last looked arrive while he watches, and the caption says how many", async () => {
    // (`rising`: the newest objects are the most significant ones, and the last ten share a day;
    // the ones he had seen must still be the ones on the screen when the picture comes up)
    const page = await openApp(browser, { homeTeam: TEAM, homeField: field(186, { rising: true }) }, seenAt(179, 3));
    await arrived(page);
    const g0 = await growth(page);
    assertEqual([g0.scene, g0.count], ["home", 179], "the picture did not start from what he last saw");
    const firstSeen = await page.evaluate(() => {
      const lo = window.__loro;
      let newest = 0;
      for (let r = 0; r < lo.COUNT; r++) newest = Math.max(newest, lo.nodes[lo.arrival[r]].arrivalOrder);
      return newest;
    });
    assertEqual(firstSeen, 179, "a light he had not seen yet is on the screen before the new ones arrive");
    await page.evaluate(() => {
      window.__counts = new Set();
      window.__countsTimer = setInterval(() => window.__counts.add(window.__loro.COUNT), 30);
    });
    assertEqual(await text(page, "#home-caption"), "7 new since this morning", "the caption");
    assertEqual(await page.evaluate(() => window.__RICHOS_MOCK__.homeStageNotices), [], "Rich was told of a stage that was not crossed");
    assert(await appears(page, "#home-caption"), "the caption is not painted");
    assertEqual(await shown(page, "#home-arrived"), false, "the Newest caption is under the new one");
    const contrast = await clears(page, ["#home-caption"]);
    // one by one: the count passes through the numbers between
    await countIs(page, 186);
    const seen = await page.evaluate(() => {
      clearInterval(window.__countsTimer);
      return Array.from(window.__counts);
    });
    assert(seen.length >= 4, "the seven lights did not arrive one by one: the count was seen at " + seen.join(", "));
    assertEqual((await stored(page)).look.count, 186, "the app did not keep what he has now seen");
    // while he is away Rich learns more; he comes back, and those arrive too, with no relaunch
    await page.evaluate((f) => {
      window.RichHome.hide("enter-app");
      window.__RICHOS_MOCK__.homeFieldSet(f);
    }, field(196));
    await page.waitForFunction(() => document.getElementById("home").hidden === true, null, { timeout: 60000 });
    await page.evaluate(() => window.RichHome.show("wordmark"));
    await page.waitForFunction(() => window.RichHomeGrowth.state().n === 196 && window.RichHomeGrowth.state().arrived, null, { timeout: 60000 });
    assertEqual(await text(page, "#home-caption"), "10 new since this morning", "the caption after coming back");
    await countIs(page, 196);
    assertEqual(await text(page, "#home-brand-line"), "loro · 196 memories", "the brand line after coming back");
    // the read behind the answer: the backend answers with the kept picture first and the new one
    // a moment later (`home_repos.rs`); the same visit picks it up
    await page.evaluate((f) => {
      window.RichHome.hide("enter-app");
      const mock = window.__RICHOS_MOCK__;
      setTimeout(() => mock.homeFieldSet(f), 4500);
    }, field(204));
    await page.waitForFunction(() => document.getElementById("home").hidden === true, null, { timeout: 60000 });
    await page.evaluate(() => window.RichHome.show("wordmark"));
    // ...and he presses Watch it grow meanwhile: the film runs to its last frame before the
    // picture is drawn again with the new records
    await page.click("#home-grow");
    await page.waitForFunction(() => document.getElementById("home-replay").classList.contains("last"), null, { timeout: 60000 });
    assertEqual(await page.evaluate(() => window.RichHomeGrowth.state().n), 196, "the picture was drawn again under the replay");
    assertEqual(await text(page, "#home-replay .line"), "Day 67 · 196 memories · First Color", "the replay did not reach its last frame");
    await page.waitForFunction(() => window.RichHomeGrowth.state().n === 204 && window.RichHomeGrowth.state().arrived, null, { timeout: 60000 });
    await countIs(page, 204);
    assertEqual(await text(page, "#home-caption"), "8 new since this morning", "the caption once the read behind the answer finished");
    // the words for when he last looked
    const words = await page.evaluate(() => {
      const f = window.RichHomeGrowth.sinceWords;
      const now = new Date(2026, 10, 18, 20, 0).getTime();
      return [new Date(2026, 10, 18, 7, 0), new Date(2026, 10, 18, 14, 0), new Date(2026, 10, 17, 23, 0), new Date(2026, 10, 16, 9, 0), new Date(2026, 10, 3, 9, 0)].map((d) => f(d.getTime(), now));
    });
    assertEqual(words, ["this morning", "this afternoon", "yesterday", "Monday", "November 3"], "the words for when he last looked");
    assertEqual(page.__errors, [], "the page reported errors");
    await page.close();
    return `"7 new since this morning", the count seen at ${seen.join(", ")}; then "10 new since this morning" after coming back; ${contrast}`;
  });

  await run.check("the first launch also waits for the read behind the answer: work since the last launch arrives on this one", async () => {
    // the backend answers with the kept picture (179) and the new one (186) a moment later
    const page = await openApp(browser, { homeTeam: TEAM, homeField: field(179, { rising: true }) }, seenAt(179, 3));
    await page.evaluate((f) => {
      const mock = window.__RICHOS_MOCK__;
      setTimeout(() => mock.homeFieldSet(f), 1500);
    }, field(186, { rising: true }));
    await page.waitForFunction(() => window.RichHomeGrowth.state().n === 186 && window.RichHomeGrowth.state().arrived, null, { timeout: 60000 });
    assertEqual(await text(page, "#home-caption"), "7 new since this morning", "the caption");
    await countIs(page, 186);
    await page.close();
    return "179 drawn, then 186 on the same launch: \"7 new since this morning\"";
  });

  await run.check("a cold open under 400 draws his lights in the order he earned them, and from 400 they are simply there", async () => {
    const page = await openApp(browser, { homeTeam: TEAM, homeField: field(186) }, seenAt(186, 3));
    await page.waitForFunction("!!window.RichHomeGrowth.state().active", null, { timeout: 60000 });
    assertEqual(await page.evaluate(() => window.VARIANT.count), 0, "the picture does not start empty under 400");
    await arrived(page);
    await countIs(page, 186);
    const order = await page.evaluate(() => { const lo = window.__loro; let last = -1; for (let r = 0; r < 186; r++) { const d = lo.D.createdDay[lo.arrival[r]]; if (d < last) return false; last = d; } return true; });
    assert(order, "the lights do not arrive in the order of their days");
    await page.close();
    const big = await openApp(browser, { homeTeam: TEAM, homeField: field(480) }, seenAt(480, 4));
    assertEqual(await big.evaluate(() => window.VARIANT.count), 480, "480 lights are made to arrive at every open");
    await big.close();
    return "186: from none, in day order; 480: there from the first frame";
  });

  // ---- 4.3 -------------------------------------------------------------------------------------
  await run.check("the weekly line: last week's memories and decisions and the stage reached, on the first visit of a week only", async () => {
    // 320 objects in 40 groups of eight over 67 days, today a Wednesday on day 66: last week is days
    // 57 to 63, which holds groups 35 to 38 (32 memories, one decision each), and 280 came before.
    const page = await openApp(browser, { homeTeam: TEAM, homeField: field(320) }, { stage: 4, look: { count: 320, at: NOW.getTime() - 5 * 86400000 }, week: WEEK - 7 });
    await arrived(page);
    await countIs(page, 320);
    const counted = await page.evaluate(() => {
      const mine = window.MATURE_LORO.nodes.filter((n) => !n.ghost);
      const week = mine.filter((n) => n.createdDay >= 57 && n.createdDay <= 63);
      return [mine.filter((n) => n.createdDay < 57).length, week.length, week.filter((n) => n.type === "decision").length];
    });
    assertEqual(counted, [280, 32, 4], "the dataset is not the one this check was written for");
    assertEqual(await text(page, "#home-caption"), "Last week: 32 memories, 4 decisions. Reached The River.", "the weekly line");
    assert(await appears(page, "#home-caption"), "the weekly line is not painted");
    assertEqual((await stored(page)).week, WEEK, "the app did not keep that this week's line has been seen");
    const contrast = await clears(page, ["#home-caption"]);
    // gone on the next visit
    await page.reload();
    await up(page);
    await arrived(page);
    assertEqual(await text(page, "#home-caption"), "", "the weekly line is up on a second visit in the same week");
    await page.close();
    // an app left running into the next week: coming back to the home screen is a visit too
    const open = await openApp(browser, { homeTeam: TEAM, homeField: field(320) }, { stage: 4, look: { count: 320, at: NOW.getTime() }, week: WEEK });
    await arrived(open);
    await countIs(open, 320);
    assertEqual(await text(open, "#home-caption"), "", "a line in a week whose line he has seen");
    await open.evaluate(() => window.RichHome.hide("enter-app"));
    await open.waitForFunction(() => document.getElementById("home").hidden === true, null, { timeout: 60000 });
    await open.clock.setFixedTime(new Date(2026, 10, 25, 9, 0, 0)); // the Wednesday after
    await open.evaluate((f) => window.__RICHOS_MOCK__.homeFieldSet(f), field(328)); // and Rich has learned more
    await open.evaluate(() => window.RichHome.show("wordmark"));
    // last week is now days 64 to 70 of the dataset: its last group of eight, one decision
    const lined = await open.waitForFunction(() => document.getElementById("home-caption").textContent === "Last week: 8 memories, 1 decision.", null, { timeout: 60000 }).then(() => true, () => false);
    assert(lined, "no weekly line on a visit in a new week, the app never relaunched: " + (await text(open, "#home-caption")));
    assertEqual((await stored(open)).week, WEEK + 7, "the new week was not kept");
    // the picture is drawn again with the new records during this same visit: the line stays
    await open.waitForFunction(() => window.RichHomeGrowth.state().n === 328 && window.RichHomeGrowth.state().arrived, null, { timeout: 60000 });
    await countIs(open, 328);
    assertEqual(await text(open, "#home-caption"), "Last week: 8 memories, 1 decision.", "the weekly line was lost when the picture was drawn again on the same visit");
    await open.close();
    // a week with nothing is never mentioned, and neither is a week the app cannot date
    const quiet = await openApp(browser, { homeTeam: TEAM, homeField: field(320, { daysAgo: 30 }) }, { stage: 4, look: { count: 320, at: NOW.getTime() }, week: WEEK - 7 });
    await arrived(quiet);
    assertEqual(await text(quiet, "#home-caption"), "", "an empty week was mentioned");
    await quiet.close();
    const undated = await openApp(browser, { homeTeam: TEAM, homeField: field(320, { days: 6, undated: 300 }) }, { stage: 4, look: { count: 320, at: NOW.getTime() }, week: WEEK - 7 });
    await arrived(undated);
    assertEqual(await text(undated, "#home-caption"), "", "a week from before the app kept dates was given a number");
    await undated.close();
    return `"Last week: 32 memories, 4 decisions. Reached The River."; once; none for an empty or an undated week; ${contrast}`;
  });

  // ---- what did not change ---------------------------------------------------------------------
  await run.check("a store that cannot be read plays no moment, and the picture and the distance line stay", async () => {
    const page = await browser.newPage({ viewport: { width: 1440, height: 900 } });
    page.__errors = [];
    page.on("pageerror", (e) => page.__errors.push(String(e)));
    await page.addInitScript(SEED_THEME, "dark");
    await page.addInitScript(([v, key]) => {
      window.__RICHOS_MOCK_PRESET__ = v;
      const get = Storage.prototype.getItem;
      Storage.prototype.getItem = function (k) {
        if (k === key) throw new Error("the store is closed");
        return get.call(this, k);
      };
    }, [{ homeTeam: TEAM, homeField: field(300) }, KEY]);
    await page.goto(APP);
    await up(page);
    await arrived(page);
    await countIs(page, 300);
    assertEqual((await growth(page)).scene, "", "a moment played with nothing remembered");
    assertEqual(await text(page, "#home-distance"), "700 to full color›", "the distance line");
    assertEqual(await text(page, "#home-caption"), "", "a caption with nothing remembered");
    assertEqual(page.__errors, [], "the page reported errors");
    await page.close();
    return "300 memories drawn, \"700 to full color\", no handover, crossing, caption or weekly line";
  });

  await browser.close();
  process.exitCode = run.report() > 0 ? 1 : 0;
}

main().catch((e) => {
  console.error(e);
  process.exit(1);
});

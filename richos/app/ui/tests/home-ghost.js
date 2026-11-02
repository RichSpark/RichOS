// THE HOME SCREEN OF A USER WITH A TEAM IS ROUND 11.4's v2, "THE GHOST", AT EVERY RECORD COUNT.
//
// CEO, 2026-11-02, `richos-hq/design/mockups/rounds/round-11.4`: "V2". The whole shape of a finished
// loro stands behind his own records at a dim floor (0.155 of full light at none of his, 0.09 at
// 1,000, nothing from 2,500), named on screen as an example for exactly as long as it is there; his
// own records burn in front of it in warm light; the left column lists only what is true of what has
// arrived; and from 2,500 of his objects the picture is his alone.
//
// THE COUNT. In the mockup one record is one object. The number used here is the one the app's
// `home_field_data` reports as objects (`meta.counts.objects`, the nodes it drew), set on the engine as
// `V.count` and read back as `__loro.COUNT`.
//
// A user with NO team keeps the first-run demonstration exactly as it was (CEO 2026-09-06), and the
// last check holds that side.
//
// Run: node home-ghost.js   (or `npm test` for every suite in this directory)

"use strict";

const path = require("path");
const { loadPlaywright, createRun, assert, assertEqual, SEED_THEME, UI_DIR } = require("./lib/harness");
const { worstContrast } = require("./lib/worst-contrast");
const png = require("./lib/png");

const APP = "file://" + path.join(UI_DIR, "index.html");

const TEAM = [
  { name: "Beth", role: "Bookkeeper" },
  { name: "Will", role: "Marketing Director" },
];

/// Round 11.4 `v2/index.html`: `ghost: 0.155, ghostSpan: 2500`, falling in a straight line.
const ghostAt = (objects) => 0.155 * Math.max(0, Math.min(1, 1 - objects / 2500));

/// The round's own legend line, verbatim (`v2/v2.js`).
const LEGEND = "Example, not yours · a finished loro, dimmed, for scale";

async function openApp(browser, preset) {
  const page = await browser.newPage({ viewport: { width: 1440, height: 900 } });
  const errors = [];
  page.on("pageerror", (e) => errors.push(String(e)));
  page.on("console", (m) => {
    if (m.type() === "error") errors.push("console: " + m.text());
  });
  page.__errors = errors;
  await page.addInitScript(SEED_THEME, "dark");
  await page.addInitScript((v) => {
    window.__RICHOS_MOCK_PRESET__ = v;
  }, preset);
  await page.goto(APP);
  await page.waitForFunction("typeof window.RichHome === 'object' && typeof window.RichTimeline === 'object'", null, { timeout: 60000 });
  await page.evaluate(() => window.RichSplash && window.RichSplash.yieldNow("acceptance-suite"));
  await page.evaluate(() => window.RichHome.startField());
  // A screen with no picture (the state before the ghost existed) is a failed check below, not a crash.
  const up = await page
    .waitForFunction("window.RichHome.state.field === 'live' && !!window.__loro && !!window.RichHome.state.ghost", null, { timeout: 45000 })
    .then(() => true, () => false);
  if (up) {
    // The bloom is the picture drawing itself in; the frame is judged after it.
    await page.waitForFunction("window.__loro.blooming === false", null, { timeout: 60000 });
    await page.waitForTimeout(700);
  }
  return page;
}

/// Everything the screen says about the ghost, read off the page.
const factsOf = (page) =>
  page.evaluate(() => {
    const lo = window.__loro;
    if (!lo || !window.RichHome.state.ghost) return { field: window.RichHome.state.field, count: null, missing: true };
    const legend = document.getElementById("home-legend");
    const note = document.getElementById("home-ghost-note");
    const text = (sel) => Array.from(document.querySelectorAll(sel)).map((e) => e.textContent.replace(/\s+/g, " ").trim());
    return {
      field: window.RichHome.state.field,
      count: lo.COUNT,
      nodes: lo.N,
      level: lo.ghostLevel,
      ghostNodes: lo.nodes.filter((n) => n.ghost === true).length,
      synthetic: window.MATURE_LORO.meta.synthetic,
      legendOn: legend.classList.contains("on"),
      legendVisible: getComputedStyle(legend).visibility === "visible" && Number(getComputedStyle(legend).opacity) > 0,
      legendText: legend.innerText.replace(/\s+/g, " ").trim(),
      noteShown: !note.classList.contains("hidden"),
      noteLead: note.querySelector(".lead").textContent,
      brand: document.getElementById("home-brand-line").textContent.replace(/\s+/g, " ").trim(),
      rows: Array.from(document.querySelectorAll("#home-signals .sig")).map((s) => s.querySelector(".l").textContent.trim()),
      signals: text("#home-signals .sig"),
      ghostScripts: Array.from(document.querySelectorAll("script[src]")).map((s) => s.getAttribute("src")).filter((s) => s.indexOf("home/field-ghost") === 0 || s.indexOf("home/field-data") === 0),
    };
  });

/// The brightest pixel in a small box on the screenshot around one of his records, and around
/// one of the example's.
async function pixelsAt(page) {
  const spots = await page.evaluate(() => {
    const lo = window.__loro;
    const inView = (x, y) => x > 330 && x < 1100 && y > 90 && y < 760; // the picture's own area, clear of the corner blocks
    const pick = (own) => {
      const out = [];
      for (let i = 0; i < lo.N; i++) {
        const isOwn = lo.nodes[i].ghost !== true;
        if (isOwn !== own) continue;
        if (own && lo.gate[i] < 0.99) continue;
        const [x, y] = lo.nodeScreen(i);
        if (!inView(x, y)) continue;
        // clear of the core's glow and of every other chosen spot
        if (Math.hypot(x - 748, y - 425) < 150) continue;
        out.push([Math.round(x), Math.round(y), i]);
        if (out.length >= 40) break;
      }
      return out;
    };
    return { own: pick(true), ghost: pick(false) };
  });
  const buf = await page.screenshot({ fullPage: false });
  const img = png.decode(buf);
  const scale = img.width / 1440;
  const brightest = (x, y) => {
    let best = null;
    for (let dy = -5; dy <= 5; dy++) {
      for (let dx = -5; dx <= 5; dx++) {
        const px = Math.round((x + dx) * scale), py = Math.round((y + dy) * scale);
        const o = (py * img.width + px) * img.channels;
        const r = img.data[o], g = img.data[o + 1], b = img.data[o + 2];
        const l = 0.2126 * r + 0.7152 * g + 0.0722 * b;
        if (!best || l > best.l) best = { r, g, b, l };
      }
    }
    return best;
  };
  const sample = (list) => list.map(([x, y]) => brightest(x, y));
  return { own: sample(spots.own), ghost: sample(spots.ghost) };
}

const median = (a) => a.slice().sort((x, y) => x - y)[Math.floor(a.length / 2)];

async function main() {
  const run = createRun("the home screen of a user with a team: round 11.4's v2, the ghost, at every count");
  const { webkit } = loadPlaywright();
  const browser = await webkit.launch();

  // --------------------------------------------------------------------------------------
  // THE FOUR COUNTS the brief names. `objects` is what `home_field_data` reports; the mock builds a
  // lane of that many objects in groups of eight, one decision per group.
  // --------------------------------------------------------------------------------------
  for (const objects of [0, 100, 1000, 2600]) {
    const page = await openApp(browser, { homeTeam: TEAM, homeWorking: [TEAM[0]], homeField: objects ? { objects, records: objects * 3 } : null });
    const f = await factsOf(page);
    const ghost = objects < 2500;

    await run.check(`${objects} objects: the example's light is ${ghostAt(objects).toFixed(4)} and the picture counts ${objects}`, async () => {
      assertEqual(f.field, "live", "no picture: a user with a team has no ghost");
      assertEqual(f.count, objects, "the arrival gate does not count his objects");
      assert(Math.abs(f.level - ghostAt(objects)) < 1e-4, `ghost level ${f.level}, round 11.4 says ${ghostAt(objects)}`);
      if (ghost) {
        assertEqual(f.nodes, 7500 + objects, "the picture is not the example's 7,500 plus his");
        assertEqual(f.ghostNodes, 7500, "the example is not flagged as the example");
      } else {
        assertEqual(f.nodes, objects, "from 2,500 of his objects the picture is his alone");
        assertEqual(f.ghostNodes, 0, "an example object is still in a picture that is his alone");
        assertEqual(f.ghostScripts, [], "the demonstration's dataset was loaded for a picture that is his alone");
      }
      assertEqual(f.synthetic, false, "the dataset on the page does not say it is his");
      return `ghost ${f.level.toFixed(4)}, ${f.count} of ${f.nodes} objects are his`;
    });

    await run.check(`${objects} objects: the legend is on the screen exactly while the example is`, async () => {
      assertEqual(f.legendOn, ghost, "the legend and the example disagree");
      assertEqual(f.legendVisible, ghost, "the legend's painted state and the example disagree");
      assertEqual(page.__errors, [], "the page reported errors");
      if (ghost) {
        assert(f.legendText.indexOf(LEGEND) >= 0, `the legend does not say "${LEGEND}": ${f.legendText}`);
        assert(f.legendText.indexOf("Yours") >= 0, "the legend does not name what is his");
      }
      return ghost ? `legend up: "${LEGEND}"` : "legend off the screen";
    });

    await run.check(`${objects} objects: the left column lists only what has arrived, and no row of zero`, async () => {
      // never: his loro holds no tasks and no minutes, so those two rows are never printed
      assert(!f.rows.some((r) => /tasks handled|attention saved/i.test(r)), "a task or hours row is printed with a value of zero: " + f.rows.join(" / "));
      assert(/AI specialist/.test(f.rows[0]), "the first row is not his team: " + f.rows.join(" / "));
      if (objects === 0) {
        assertEqual(f.rows.length, 1, "rows with nothing behind them are printed");
        assert(f.brand === "loro · empty", `the sub line is not "loro · empty": ${f.brand}`);
        assert(f.noteShown && f.noteLead === "None of this light is yours yet.", "the line under the core is not round 11.4's for none");
      } else {
        assert(f.rows.some((r) => /sources understood/.test(r)), "sources are not listed: " + f.rows.join(" / "));
        assert(f.rows.some((r) => /decisions remembered/.test(r)), "decisions are not listed: " + f.rows.join(" / "));
        assert(!f.signals.some((s) => /lesson/.test(s)), "a lessons count of zero is printed");
        assert(f.brand.indexOf(objects.toLocaleString("en-US") + " memories") >= 0, `the sub line does not count his memories: ${f.brand}`);
        assertEqual(f.noteShown, false, "the line under the core is up with five or more records");
      }
      return f.rows.join(" / ") + "; " + f.brand;
    });

    if (objects === 100 || objects === 1000) {
      await run.check(`${objects} objects: his records stand out from the example`, async () => {
        const px = await pixelsAt(page);
        assert(px.own.length >= 5 && px.ghost.length >= 5, `found ${px.own.length} of his and ${px.ghost.length} example objects on screen`);
        const ownL = median(px.own.map((p) => p.l)), ghostL = median(px.ghost.map((p) => p.l));
        assert(ownL > ghostL * 1.4, `his records (${ownL.toFixed(0)}) are not brighter than the example (${ghostL.toFixed(0)})`);
        const warm = px.own.filter((p) => p.r > p.b + 8).length / px.own.length;
        const coldGhost = px.ghost.filter((p) => p.b > p.r + 8).length / px.ghost.length;
        // below ~900 of his the field is in "starlight" (the round's `sparseWhite`): warm gold. Past
        // it his records return to their strand colors, and the check is only that they are not cold.
        if (objects < 900) assert(warm > 0.8, `only ${(warm * 100).toFixed(0)}% of his records are warm`);
        assert(coldGhost > 0.5, `only ${(coldGhost * 100).toFixed(0)}% of the example is cold blue`);
        return `his median luma ${ownL.toFixed(0)} vs the example's ${ghostL.toFixed(0)}; ${(warm * 100).toFixed(0)}% of his warm, ${(coldGhost * 100).toFixed(0)}% of the example cold`;
      });
    }

    if (ghost && objects > 0) {
      await run.check(`${objects} objects: his sources' threads ride the example's bands of the river, not one strand`, async () => {
        const seen = await page.evaluate(() => {
          const lo = window.__loro;
          const mine = lo.D.sources.map((s, si) => ({ s, si })).filter((x) => x.s.primaryNodeId.indexOf("own-") === 0);
          return {
            sources: mine.length,
            domains: Array.from(new Set(mine.map((x) => lo.domains[lo.D.srcDomain[x.si]].id))),
            strands: new Set(mine.map((x) => lo.D.srcStrand[x.si])).size,
          };
        });
        assert(seen.sources > 0, "none of his sources is in the river");
        assert(seen.domains.every((d) => d.indexOf("own-") !== 0), "a source of his has the band of a territory with no arc: " + seen.domains.join(", "));
        assert(seen.sources < 6 || seen.strands > 1, `${seen.sources} of his sources ride one strand`);
        return `${seen.sources} sources over ${seen.strands} strand(s) in ${seen.domains.length} example band(s)`;
      });
    }

    if (ghost) {
      await run.check(`${objects} objects: the legend, the line under the core and "Newest" clear WCAG AA behind their glyphs`, async () => {
        const rows = await worstContrast(page, ["#home-legend .tx", "#home-ghost-note .lead", "#home-ghost-note .sub", "#home-arrived", "#home-signals", "#home-owner", "#home-brand-line"]);
        assert(rows.length >= 4, "measured only " + rows.length + " line(s)");
        const bad = rows.filter((r) => !(r.ratio >= r.needs));
        assertEqual(bad, [], "text below its contrast floor");
        return rows.map((r) => `${JSON.stringify(r.text.slice(0, 28))} ${r.ratio}:1 (needs ${r.needs})`).join("; ");
      });
    }
    await page.close();
  }

  // --------------------------------------------------------------------------------------
  // NEGATIVE CONTROL: no team, no ghost. The first-run demonstration is untouched.
  // --------------------------------------------------------------------------------------
  await run.check("NEGATIVE CONTROL: a user with no team still gets the demonstration, with no ghost and no legend (CEO 2026-09-06)", async () => {
    const page = await browser.newPage({ viewport: { width: 1440, height: 900 } });
    await page.addInitScript(SEED_THEME, "dark");
    await page.goto(APP);
    await page.waitForFunction("typeof window.RichHome === 'object'", null, { timeout: 60000 });
    await page.evaluate(() => window.RichSplash && window.RichSplash.yieldNow("acceptance-suite"));
    await page.evaluate(() => window.RichHome.startField());
    await page.waitForFunction("window.RichHome.state.field === 'live' && !!window.__loro", null, { timeout: 90000 });
    const seen = await page.evaluate(() => ({
      synthetic: window.MATURE_LORO.meta.synthetic,
      nodes: window.__loro.N,
      ghost: window.RichHome.state.ghost,
      level: window.__loro.V.ghost || 0,
      legend: document.getElementById("home-legend").classList.contains("on"),
      note: !document.getElementById("home-note").hidden,
      ghostScript: !!document.querySelector('script[src^="home/field-ghost"]'),
    }));
    assertEqual(seen.synthetic, true, "the first-run demonstration is gone for a user with no team");
    assertEqual(seen.nodes, 7500, "the demonstration is not its own 7,500 objects");
    assertEqual(seen.ghost, null, "a user with no team has a ghost");
    assertEqual(seen.level, 0, "the demonstration is drawn with an example's floor");
    assertEqual(seen.legend, false, "the legend is up over the demonstration");
    assertEqual(seen.ghostScript, false, "the ghost's script was loaded for a user with no team");
    assert(seen.note, "the demonstration is up without the banner that says it is one");
    await page.close();
    return "demonstration drawn (7,500 objects, banner up), no ghost, no legend";
  });

  await browser.close();
  process.exitCode = run.report() > 0 ? 1 : 0;
}

main().catch((e) => {
  console.error(e);
  process.exit(1);
});

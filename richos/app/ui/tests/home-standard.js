// THE HOME SCREEN STANDARD OF ROUND 73 (CEO, 2026-11-13).
//
// "The home screen design from here: `richos-hq/design/mockups/rounds/round-73/` is the new home
// screen standard (new loro design + SwearMeter stats if enabled by the user). So, build that now.
// And also build the SwearMeter addition to the nav rail from the same round. And one more thing:
// On the home screen, hitting the enter key does the same as clicking the "Talk to Rich" button.
// Hitting the space key should also do the same as hitting the enter key there."
//
// One check per piece of that order:
//   1. Enter or Space (either key alone) opens Talk to Rich, from anywhere on the home screen.
//   2. The SwearMeter signal: the 30-day count as the column's last signal, above Talk to Rich,
//      only while SwearMeter is on.
//   3. The SwearMeter button on the rail (above Bust a bug, only while it is on) and its card.
//   4. The loro picture is round 72's: its lines have no sharp angles.
//   5. A user with a team: from 2,500 of their objects every company has a territory of its own
//      (the ghost hosted their objects inside the example's clusters, so no company had one).
//
// All data is synthetic. Headless WebKit, nothing on the screen.
//
// Run: node home-standard.js   (or `npm test` for every suite in this directory)

"use strict";

const path = require("path");
const { loadPlaywright, createRun, assert, assertEqual, SEED_THEME, UI_DIR } = require("./lib/harness");

const APP = "file://" + path.join(UI_DIR, "index.html");

/// The shape `get_swearmeter` returns on main (`count`, `periods.last_30_days`, `periods.months`).
function swearResult(last30) {
  const months = [[202602, 147], [202603, 438], [202604, 1237], [202605, 164], [202606, 42], [202607, 440], [202608, 871], [202609, 2155], [202610, 1564], [202611, 409]];
  return {
    detector_version: 1, count: 7467, history_loading: false, available: true, revision: 1,
    periods: { last_30_days: last30, undated: 0, months: months.map(([month, count]) => ({ month, count })) },
    provider_history_incomplete: false, ambiguous_matches: 0, unknown_card_inputs: 0, unknown_message_inputs: 0, incomplete_history: false,
  };
}

/// Opens the app with `fixture` answering some bridge commands before the page's own scripts run:
///   swear: { show, last30 } -> get_swearmeter
///   lanes: [[id, label, objects | [group sizes]], ...] -> home_field_data, one company per lane
///   source: "repositories" -> the dataset says it was read from the connected repositories
///   seeds: true -> every object carries its placement seed and key, as `home_repos.rs` writes them
///   delayMs -> home_field_data answers that late (the screen waits 8 s)
///   team: [{ name, role }] -> the preset's homeTeam
async function openApp(browser, fixture) {
  const page = await browser.newPage({ viewport: { width: 1440, height: 900 } });
  const errors = [];
  page.on("pageerror", (e) => errors.push(String(e)));
  page.on("console", (m) => {
    if (m.type() === "error") errors.push("console: " + m.text());
  });
  page.__errors = errors;
  await page.addInitScript(SEED_THEME, "dark");
  await page.addInitScript((f) => {
    if (f.team) window.__RICHOS_MOCK_PRESET__ = { homeTeam: f.team };
    function field(lanes) {
      const nodes = [], links = [], sources = [], domains = [];
      for (const [lane, label, size] of lanes) {
        // `size` is a number of objects (in groups of 8) or the size of each group
        const sizes = Array.isArray(size) ? size : null;
        const objects = sizes ? sizes.reduce((x, y) => x + y, 0) : size;
        const clusters = [];
        const groups = sizes ? sizes.length : Math.max(1, Math.round(objects / 8));
        let made = 0;
        for (let g = 0; g < groups; g++) {
          const gid = lane + "-grp-" + g, first = nodes.length;
          const n = sizes ? sizes[g] : g === groups - 1 ? objects - made : Math.min(8, objects - made);
          // what `home_repos.rs` gives every object: `layoutSeed`, a hash of its group, and
          // `layoutKey`, a hash of what the object is
          const hash = (text) => { let h = 2166136261; for (const ch of text) h = Math.imul(h ^ ch.charCodeAt(0), 16777619) >>> 0; return h; };
          const seed = hash(gid);
          for (let i = 0; i < n; i++) {
            nodes.push({ id: sizes ? gid + "-" + i : lane + "-obj-" + nodes.length, type: i === 0 ? "decision" : "memory", domain: lane, cluster: gid,
              ...(f.seeds ? { layoutSeed: seed, layoutKey: hash(gid + "-" + i) } : {}),
              degree: i === 0 ? n - 1 : 1, significance: 0.8, activity: 0, recency: 0, createdDay: 0 });
            if (i > 0) links.push({ s: nodes[first].id, t: nodes[nodes.length - 1].id, class: "membership", cross: false });
          }
          made += n;
          if (n > 0) {
            clusters.push({ id: gid, label: "Group " + (g + 1), nodeCount: n });
            sources.push({ id: lane + "-src-" + g, kind: "document", domain: lane, ingestedDay: 40000, primaryNodeId: nodes[first].id, derivedCount: n });
          }
        }
        domains.push({ id: lane, label, nodeCount: objects, clusters });
      }
      return { available: true, field: { meta: { name: "fixture", synthetic: false, ...(f.source ? { source: f.source } : {}), horizonDays: 40000,
        counts: { records: nodes.length, objects: nodes.length, links: links.length, sources: sources.length, domains: domains.length } },
        bragSignals: {}, specialists: [], domains, nodes, links, sources } };
    }
    let real = null;
    Object.defineProperty(window, "RichBridge", {
      configurable: true,
      get() { return real; },
      set(v) {
        real = v;
        const invoke = v.invoke.bind(v);
        v.invoke = async function (cmd, args) {
          if (cmd === "get_swearmeter" && f.swear) {
            // a macrotask after the answer: `swearmeter.js` has painted with it by then
            setTimeout(() => { window.__swearAnswered = true; }, 0);
            return { show: f.swear.show, result: f.swearResult };
          }
          if (cmd === "set_swearmeter_visible" && f.swear) { f.swear.show = args.show === true; return null; }
          if (cmd === "home_field_data" && f.lanes) {
            if (f.delayMs) await new Promise((done) => setTimeout(done, f.delayMs));
            return field(f.lanes);
          }
          return invoke(cmd, args);
        };
      },
    });
  }, fixture.swear ? { ...fixture, swearResult: swearResult(fixture.swear.last30) } : fixture);
  await page.goto(APP);
  await page.waitForFunction("typeof window.RichHome === 'object' && typeof window.RichTimeline === 'object'", null, { timeout: 60000 });
  await page.evaluate(() => window.RichSplash && window.RichSplash.yieldNow("acceptance-suite"));
  return page;
}

/// The card closes 160 ms after the pointer leaves (`swearmeter.js`). A timer set in the page after
/// the pointer has left, and longer than that one, runs after it on the page's own clock, whatever
/// the host is doing: what the card is then is what leaving made it.
async function afterHoverClose(page) {
  await page.evaluate(() => new Promise((done) => setTimeout(done, 200)));
}

async function withField(page) {
  await page.evaluate(() => window.RichHome.startField());
  await page.waitForFunction("window.RichHome.state.field === 'live' && !!window.__loro", null, { timeout: 90000 });
}

async function main() {
  const run = createRun("Home screen standard (round 73)");
  const { webkit } = loadPlaywright();
  const browser = await webkit.launch();

  // ---------------------------------------------------------------------------------------
  await run.check("Enter or Space, either key alone, opens Talk to Rich from anywhere on the home screen", async () => {
    const page = await openApp(browser, {});
    const press = async (key, focus) => {
      await page.evaluate(() => window.RichHome.show("suite"));
      await page.waitForFunction(() => window.RichHome.isOpen() && !document.getElementById("home").hidden);
      await page.evaluate((focus) => {
        if (focus === "door") return document.getElementById("home-enter").focus();
        if (document.activeElement && document.activeElement.blur) document.activeElement.blur();
      }, focus);
      await page.evaluate(() => { window.RichHome.state.lastLeaveReason = null; });
      // `onHomeKey` answers the key in the keydown itself (`hide` sets the state synchronously)
      await page.keyboard.press(key);
      return page.evaluate(() => ({ open: window.RichHome.isOpen(), why: window.RichHome.state.lastLeaveReason }));
    };
    const enter = await press("Enter", "none");
    assertEqual([enter.open, enter.why], [false, "enter-key"], "Enter did not open Talk to Rich");
    const space = await press("Space", "none");
    assertEqual([space.open, space.why], [false, "space-key"], "Space did not open Talk to Rich");
    const onDoor = await press("Space", "door");
    assertEqual(onDoor.open, false, "Space on the focused Talk to Rich button did not open it");
    const shifted = await press("Shift+Space", "none");
    assertEqual(shifted.open, true, "Shift+Space opened Talk to Rich; Enter refuses Shift and Space must do exactly what Enter does");
    // where a key already means something (the company-buttons panel), Space stays there, as Enter does
    await page.evaluate(() => window.RichHome.openSettings());
    await page.waitForFunction(() => document.querySelectorAll(".home-prefs-row").length > 0);
    await page.focus('.home-prefs-label[data-entity="northwind"]');
    await page.evaluate(() => { window.RichHome.state.lastLeaveReason = null; });
    await page.keyboard.press("Space");
    const panel = await page.evaluate(() => ({ open: window.RichHome.isOpen(), why: window.RichHome.state.lastLeaveReason }));
    await page.evaluate(() => window.RichHome.closeSettings());
    assertEqual([panel.open, panel.why], [true, null], "Space inside the company-buttons panel left the home screen");
    // Space is typed, Enter is not: a space in a text field anywhere is that field's, and a held
    // Space (a repeat) opens nothing.
    const typed = await page.evaluate(() => {
      const field = document.createElement("textarea");
      field.id = "suite-field";
      document.body.appendChild(field);
      window.RichHome.state.lastLeaveReason = null;
      const fire = (target, init) => target.dispatchEvent(new KeyboardEvent("keydown", { key: " ", bubbles: true, cancelable: true, ...init }));
      fire(field, {});
      const inField = window.RichHome.isOpen();
      fire(document.body, { repeat: true });
      const held = window.RichHome.isOpen();
      field.remove();
      return { inField, held };
    });
    assertEqual([typed.inField, typed.held], [true, true], "a typed space in a text field, or a held Space, opened Talk to Rich");
    const keys = await page.evaluate(() => document.getElementById("home-enter").getAttribute("aria-keyshortcuts"));
    assertEqual(keys, "Enter Space", "the button does not announce both keys");
    assertEqual(page.__errors, [], "renderer errors");
    await page.close();
    return `Enter -> ${enter.why}; Space -> ${space.why}; Space on the door opens it; Shift+Space and Space in the panel do not; aria-keyshortcuts="${keys}"`;
  });

  // ---------------------------------------------------------------------------------------
  await run.check("SwearMeter on: its 30-day count is the column's last signal, above Talk to Rich; off: no signal", async () => {
    // The SwearMeter answer comes before the picture; the picture then writes the column's
    // signals, and the SwearMeter signal must still be the last of them.
    const on = await openApp(browser, { swear: { show: true, last30: 1299 } });
    await withField(on);
    await on.waitForSelector("#home-swearmeter");
    const r = await on.evaluate(() => {
      const node = document.getElementById("home-swearmeter");
      const signals = document.getElementById("home-signals");
      return {
        n: node.querySelector(".n").textContent, l: node.querySelector(".l").textContent,
        last: signals.lastElementChild === node, others: signals.children.length - 1,
        bottom: node.getBoundingClientRect().bottom, door: document.getElementById("home-switch").getBoundingClientRect().top,
      };
    });
    assertEqual([r.n, r.l], ["1,299swear words", "in the last 30 days"], "the signal is not the number with 'swear words', then 'in the last 30 days'");
    assert(r.last && r.others > 0, "the signal is not the last of the column's signals");
    assert(r.bottom <= r.door, `the signal (bottom ${r.bottom}) is not above Talk to Rich (top ${r.door})`);
    assertEqual(on.__errors, [], "renderer errors");
    await on.close();
    const one = await openApp(browser, { swear: { show: true, last30: 1 } });
    await withField(one);
    await one.waitForSelector("#home-swearmeter");
    const singular = await one.evaluate(() => document.querySelector("#home-swearmeter small").textContent);
    assertEqual(singular, "swear word", "1 is not singular");
    await one.close();
    // SwearMeter on is enough: the signal is there in a column with no other signal, which round
    // 73's mockup left out and the CEO's brief does not.
    const bare = await openApp(browser, { swear: { show: true, last30: 12 } });
    await bare.waitForSelector("#home-swearmeter");
    const alone = await bare.evaluate(() => {
      const signals = document.getElementById("home-signals");
      for (const c of [...signals.children]) if (c.id !== "home-swearmeter") c.remove();
      window.RichSwearMeter.paintHome();
      return { there: !!document.getElementById("home-swearmeter"), others: signals.children.length - 1 };
    });
    await bare.close();
    assertEqual([alone.there, alone.others], [true, 0], "SwearMeter is on and the column shows no SwearMeter signal when it has no other signal");
    const off = await openApp(browser, { swear: { show: false, last30: 1299 } });
    await withField(off);
    await off.waitForFunction(() => document.querySelectorAll("#home-signals .sig").length > 0);
    const none = await off.evaluate(() => document.querySelectorAll("#home-swearmeter").length);
    assertEqual(none, 0, "SwearMeter off still put a signal on the home screen");
    await off.close();
    return `on: "${r.n}" / "${r.l}", last of ${r.others + 1} signals, ends at y=${r.bottom} above the door at y=${r.door}; 1 reads "${singular}"; off: no signal`;
  });

  // ---------------------------------------------------------------------------------------
  await run.check("the SwearMeter button sits above Bust a bug while SwearMeter is on, and its card opens on hover and click", async () => {
    const page = await openApp(browser, { swear: { show: true, last30: 1299 } });
    await page.evaluate(() => window.RichHome.hide && window.RichHome.hide("suite"));
    await page.waitForFunction(() => !document.getElementById("nr-swear").hidden);
    const order = await page.evaluate(() => {
      const b = document.getElementById("nr-swear"), next = b.nextElementSibling;
      return { next: next && next.id, above: b.getBoundingClientRect().bottom <= next.getBoundingClientRect().top + 1 };
    });
    assertEqual(order.next, "nr-bug", "the SwearMeter button is not directly above Bust a bug");
    assert(order.above, "the SwearMeter button is not above Bust a bug on screen");
    await page.hover("#nr-swear");
    await page.waitForSelector("#swear-card.is-open");
    const card = await page.evaluate(() => {
      const c = document.getElementById("swear-card"), q = (k) => c.querySelector('[data-k="' + k + '"]');
      return { avg: q("avg").textContent, all: q("all").textContent, months: c.querySelectorAll(".swear-months li").length, more: q("more-label").textContent };
    });
    assertEqual([card.avg, card.all, card.months], ["43", "7,467", 6], "the card's daily average, all-time total or first six months are wrong");
    assertEqual(card.more, "Show 4 earlier months", "the reveal button does not offer the remaining months");
    await page.click('#swear-card [data-k="more"]');
    const all = await page.evaluate(() => document.querySelectorAll("#swear-card .swear-months li").length);
    assertEqual(all, 10, "the reveal did not show every month in place");
    // leaving closes a card that was only hovered open
    await page.mouse.move(900, 450);
    await page.waitForFunction(() => !document.getElementById("swear-card").classList.contains("is-open"));
    // a click pins it: leaving does not close it, Escape does
    await page.click("#nr-swear");
    await page.mouse.move(900, 450);
    await afterHoverClose(page);
    const pinned = await page.evaluate(() => document.getElementById("swear-card").classList.contains("is-open"));
    assertEqual(pinned, true, "a click on the button did not pin the card");
    await page.keyboard.press("Escape");
    await page.waitForFunction(() => !document.getElementById("swear-card").classList.contains("is-open"));
    // pinned again, a click elsewhere closes it
    await page.click("#nr-swear");
    await page.mouse.move(900, 450);
    await afterHoverClose(page);
    await page.mouse.click(900, 450);
    await page.waitForFunction(() => !document.getElementById("swear-card").classList.contains("is-open"));
    // pinned from the keyboard, then Home: no pointer press follows to unpin it, and the home screen
    // has no rail, so the card is put away and is not painted over Home
    // (review rv-20261114T004049Z-3a3331d3-4a8e)
    await page.focus("#nr-swear");
    await page.keyboard.press("Enter");
    await page.waitForFunction(() => document.getElementById("swear-card").classList.contains("is-open") && document.getElementById("nr-swear").getAttribute("aria-expanded") === "true");
    await page.evaluate(() => window.RichHome.show("suite"));
    await page.waitForFunction(() => !document.getElementById("swear-card").classList.contains("is-open"));
    const overHome = await page.evaluate(() => getComputedStyle(document.getElementById("swear-card")).display);
    assertEqual(overHome, "none", "the SwearMeter card can be painted over the home screen");
    assertEqual(page.__errors, [], "renderer errors");
    await page.close();
    const off = await openApp(browser, { swear: { show: false, last30: 1299 } });
    await off.waitForFunction(() => window.__swearAnswered === true);
    const hidden = await off.evaluate(() => document.getElementById("nr-swear").hidden);
    await off.close();
    assertEqual(hidden, true, "the SwearMeter button shows while SwearMeter is off");
    return `above ${order.next}; card: ${card.avg} a day, ${card.all} all time, 6 months then 10 after "${card.more}"; leaving closes a hovered card, a click pins it, Escape and a click elsewhere close it; hidden while off`;
  });

  // ---------------------------------------------------------------------------------------
  await run.check("the loro picture is round 72's: no sharp angles in its lines", async () => {
    const page = await openApp(browser, {});
    await withField(page);
    const r = await page.evaluate(() => {
      let over30 = 0, over15 = 0;
      for (const s of window.__loro.D.strands) {
        for (let k = 1; k < s.n - 1; k++) {
          const ux = s.P[k * 2] - s.P[k * 2 - 2], uy = s.P[k * 2 + 1] - s.P[k * 2 - 1], vx = s.P[k * 2 + 2] - s.P[k * 2], vy = s.P[k * 2 + 3] - s.P[k * 2 + 1];
          const lu = Math.hypot(ux, uy), lv = Math.hypot(vx, vy);
          if (lu < 1e-6 || lv < 1e-6) continue;
          const t = (Math.acos(Math.max(-1, Math.min(1, (ux * vx + uy * vy) / (lu * lv)))) * 180) / Math.PI;
          if (t > 30) over30++;
          if (t > 15) over15++;
        }
      }
      return { over30, over15, strands: window.__loro.D.strands.length };
    });
    await page.close();
    // Round 72's README, measured the same way: 1 turn over 30 degrees and 6 over 15, all on two
    // strands that double back, each between pieces under a fifth of a pixel. Round 71 had 668 and 2,958.
    assert(r.over30 <= 1 && r.over15 <= 6, `the lines still turn sharply: ${r.over30} turns over 30 degrees, ${r.over15} over 15`);
    return `${r.strands} strands: ${r.over30} turn(s) over 30 degrees, ${r.over15} over 15`;
  });

  // ---------------------------------------------------------------------------------------
  await run.check("a user with a team: from 2,500 objects every company has a territory of its own", async () => {
    const team = [{ name: "Beth", role: "Bookkeeper" }];
    const page = await openApp(browser, { team, source: "repositories", seeds: true, lanes: [["northwind", "Northwind Traders", 1800], ["kestrel", "Kestrel Labs", 800]] });
    await withField(page);
    const r = await page.evaluate(() => {
      const lo = window.__loro, D = lo.D;
      const strandsOf = {};
      let ghosts = 0, hosted = 0;
      lo.nodes.forEach((n, i) => {
        if (n.ghost) { ghosts++; return; }
        if (n.hosted) hosted++;
        const d = n.domain.replace(/^own-/, "");
        (strandsOf[d] = strandsOf[d] || new Set()).add(D.nStrand[i]);
      });
      const a = strandsOf.northwind || new Set(), b = strandsOf.kestrel || new Set();
      let shared = 0;
      a.forEach((s) => { if (b.has(s)) shared++; });
      const labels = lo.domains.filter((d) => d.nodeCount > 0).map((d) => d.label);
      return { ghosts, hosted, a: a.size, b: b.size, shared, labels, team: !!window.RichHome.state.team };
    });
    assertEqual(page.__errors, [], "renderer errors");
    await page.close();
    assert(r.team, "the fixture's team did not reach the home screen");
    assertEqual([r.ghosts, r.hosted], [0, 0], `the example is still under 2,600 of his objects (${r.ghosts} example objects, ${r.hosted} of his hosted in its clusters)`);
    assert(r.a > 0 && r.b > 0 && r.shared === 0, `the two companies do not stand on territories of their own (${r.a} and ${r.b} strands, ${r.shared} shared)`);
    assertEqual(r.labels, ["Northwind Traders", "Kestrel Labs"], "the drawn territories are not his two companies");
    return `2,600 objects in two companies: no example objects, none hosted; ${r.a} and ${r.b} strands, none shared`;
  });

  // ---------------------------------------------------------------------------------------
  // Sage 2026-11-13, fact 2: the answer is waited on for 8 seconds, and a first read of several
  // repositories comes later than that, so the late path is the normal one.
  await run.check("his objects arrive after the 8-second wait: from 2,500 they are still drawn alone, each company a territory", async () => {
    const team = [{ name: "Beth", role: "Bookkeeper" }];
    const page = await openApp(browser, { team, source: "repositories", seeds: true, delayMs: 9500, lanes: [["northwind", "Northwind Traders", 1800], ["kestrel", "Kestrel Labs", 800]] });
    await withField(page);
    const early = await page.evaluate(() => window.RichHome.state.fieldOffer.state);
    assertEqual(early, "timed-out", "the screen waited for the answer");
    const came = await page
      .waitForFunction(() => window.__loro && window.__loro.nodes.length === 2600 && window.__loro.blooming === false, null, { timeout: 60000 })
      .then(() => true, () => false);
    const r = await page.evaluate(() => {
      const lo = window.__loro;
      return { nodes: lo.nodes.length, ghosts: lo.nodes.filter((n) => n.ghost).length, hosted: lo.nodes.filter((n) => n.hosted).length,
        labels: lo.domains.filter((d) => d.nodeCount > 0).map((d) => d.label) };
    });
    assertEqual(page.__errors, [], "renderer errors");
    await page.close();
    assert(came, `the late answer was drawn back inside the example: ${r.nodes} objects, ${r.ghosts} of the example's, ${r.hosted} of his hosted in its clusters`);
    assertEqual([r.ghosts, r.hosted, r.labels], [0, 0, ["Northwind Traders", "Kestrel Labs"]], "the late picture is not his two companies alone");
    return `timed out at 8 s; the late 2,600 objects are drawn alone: ${r.labels.join(", ")}`;
  });

  // ---------------------------------------------------------------------------------------
  // Sage 2026-11-13, fact 1: every object was placed from one shared random stream, so one added
  // object moved every object drawn after it, at every launch after a commit.
  await run.check("new work in one folder does not rearrange the picture", async () => {
    const team = [{ name: "Beth", role: "Bookkeeper" }];
    const placed = async (first) => {
      const page = await openApp(browser, { team, seeds: true, source: "repositories",
        lanes: [["northwind", "Northwind Traders", [first, 400, 400, 400]], ["kestrel", "Kestrel Labs", [350, 350, 300]]] });
      await withField(page);
      const at = await page.evaluate(() => {
        const lo = window.__loro, D = lo.D, out = {};
        // where the object stands in the picture's own 1,000-unit square, before any light is put on it
        lo.nodes.forEach((n, i) => { const t = [0, 0, 0, 0]; D.strandAt(D.strands[D.nStrand[i]], D.nU[i], t); out[n.id] = [t[0], t[1]]; });
        return out;
      });
      assertEqual(page.__errors, [], "renderer errors");
      await page.close();
      return at;
    };
    // a day's work: 6 objects more in the first folder of the first company
    const before = await placed(600), after = await placed(606);
    // The picture is 1,000 units across. A folder's strands are its share of the company's arc, so
    // when one grows the others slide by a strand: a few units. Dealt again, an object lands
    // anywhere in its folder's territory, hundreds of units away.
    const NUDGE = 25;
    let stayed = 0, moved = 0, compared = 0;
    for (const id in before) {
      if (!after[id]) continue;
      compared++;
      const d = Math.hypot(before[id][0] - after[id][0], before[id][1] - after[id][1]);
      if (d <= NUDGE) stayed++;
      else moved++;
    }
    assertEqual(compared, 2800, "not every object drawn before is drawn after");
    assert(moved <= 0.02 * compared, `${moved} of ${compared} objects moved more than ${NUDGE} units (the picture is 1,000 across) when 6 were added to one folder`);
    return `6 objects added to one folder: ${stayed} of ${compared} objects stay within ${NUDGE} units of where they stood, ${moved} moved farther`;
  });

  // ---------------------------------------------------------------------------------------
  // Sage 2026-11-13, fact 5 (round 72 hides it): the engine names a teammate at random for each
  // source, which over his own documents is a made-up event.
  await run.check("over a picture read from the repositories there is no 'learned from' line", async () => {
    const team = [{ name: "Beth", role: "Bookkeeper" }];
    const lanes = [["northwind", "Northwind Traders", 1800], ["kestrel", "Kestrel Labs", 800]];
    const shown = async (fixture) => {
      const page = await openApp(browser, fixture);
      await withField(page);
      const display = await page.evaluate(() => getComputedStyle(document.getElementById("home-ticker")).display);
      await page.close();
      return display;
    };
    const repos = await shown({ team, lanes, source: "repositories" });
    const example = await shown({});
    assertEqual(repos, "none", "the 'learned from' line can show over his repositories");
    assert(example !== "none", "the first-run example lost its 'learned from' line");
    return `repositories: display ${repos}; the first-run example: display ${example}`;
  });

  await browser.close();
  return run.report();
}

main().then((failed) => {
  process.exitCode = failed ? 1 : 0;
});

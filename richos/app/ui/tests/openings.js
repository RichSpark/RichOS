// THE OPENINGS (round 74, the CEO's order of 2026-11-14), on the shipping page.
//
// What he ordered, and what this suite holds the page to:
//
//   "1: yes"   the "Your openings" gallery with Play and "Make it my opening"
//   "2: approve"   the materials ladder
//   "3: on by default with the option to turn off in settings; use the 'Two notes' sound"
//   "4: yes"   "One of mine, a different one each day" as a second way to start
//   "5: adjust later"   the thresholds as the round has them (one data file)
//
// and, the same day: "We now also need to remove the settings toggle for disabling the splash
// screen. Because the splash screen is now needed for the illusion of a super-fast app launch",
// "that same switch can be replaced with the switch for switching off the sound in the splash
// screen".
//
// NOTHING HERE MAKES A SOUND. Every page this suite opens replaces `Audio` before any product
// script runs with a recorder that plays nothing, so what is asserted is which file would have
// played and at which moment of the splash, on a Mac that stays silent.
//
// The rules that decide a real start (what is earned, which opening is picked) are Rust's and
// are tested there (`richos-core` `openings.rs`, `launch.rs`; `src-tauri` `openings.rs`,
// `splash_pick.rs`). This suite drives the page.

"use strict";

const fs = require("fs");
const path = require("path");
const { loadPlaywright, createRun, assert, assertEqual, leaveSplash, UI_DIR } = require("./lib/harness");
const { contrastRatio, parseCssColor, compositeOver } = require("./lib/contrast");
const { SILENT_AUDIO } = require("./lib/silent-audio");

const APP = "file://" + path.join(UI_DIR, "index.html");
const read = (name) => fs.readFileSync(path.join(UI_DIR, name), "utf8");

/// Round 74's made-up user, 14 months in: 13 openings his, the rest not.
const USER = {
  firstStart: "2025-09-08",
  version: "1.4.0",
  earned: {
    "ruled-standard": "2025-09-08",
    "midnight-suede": "2025-09-08",
    "power-100": "2025-09-09",
    "power-1k": "2025-09-19",
    "power-10k": "2025-11-27",
    "power-50k": "2026-04-06",
    "power-100k": "2026-09-02",
    "edition-update": "2026-11-10",
    "edition-year": "2026-09-08",
    "edition-1000": "2026-10-21",
    "material-leather": "2025-12-08",
    "material-slate": "2026-03-08",
    "material-steel": "2026-09-08",
  },
  pick: null,
  sound: false,
  company: null,
  day: 20000,
};
const EARNED = Object.keys(USER.earned);

async function open(browser, opts) {
  opts = opts || {};
  const ctx = await browser.newContext({ viewport: opts.viewport || { width: 1440, height: 900 }, colorScheme: opts.scheme || "dark" });
  const page = await ctx.newPage();
  const errors = [];
  page.on("pageerror", (e) => errors.push(String(e)));
  page.__errors = errors;
  page.__ctx = ctx;
  if (opts.virtualTime) {
    await page.clock.install({ time: new Date(0) });
    await page.clock.pauseAt(new Date(0));
  }
  await page.addInitScript(SILENT_AUDIO);
  if (opts.record !== null) await page.addInitScript((r) => { window.__RICHOS_OPENINGS__ = r; }, Object.assign({}, USER, opts.record || {}));
  if (opts.launch) await page.addInitScript((l) => { window.__RICHOS_LAUNCH__ = l; }, opts.launch);
  if (opts.init) await page.addInitScript(opts.init);
  await page.goto(opts.url || APP);
  await page.waitForFunction(() => !!window.RichSplash, null, { timeout: 60000 });
  return page;
}

/// The gallery, opened the way he opens it: Settings, then "Your openings".
async function openGallery(browser, opts) {
  const page = await open(browser, opts);
  await leaveSplash(page);
  await page.waitForFunction(() => window.RichSettings && window.RichOpeningsPage, null, { timeout: 60000 });
  await page.evaluate(() => window.RichSettings.openMenu());
  await page.click("#openings-open");
  await page.waitForFunction(() => window.RichOpeningsPage.isOpen() && document.querySelectorAll("#openings-page .opg-card").length > 0, null, { timeout: 30000 });
  return page;
}

const calls = (page, cmd) => page.evaluate((c) => window.__RICHOS_MOCK__.openingsCalls().filter((x) => x.cmd === c), cmd);
const audio = (page) => page.evaluate(() => window.__audio.map((a) => ({ src: a.src, loaded: a.loaded, playedAt: a.playedAt })));

async function main() {
  const run = createRun("the openings (round 74): the gallery, his pick, a different one each day, the sound, and no off switch");
  const browser = await loadPlaywright().webkit.launch();

  await run.check("1  the catalog is data: 28 openings in round 74's order, and each is a library entry the page can draw", async () => {
    const src = read("openings-catalog.js");
    const json = src.slice(src.indexOf("=", src.indexOf("window.RichOpeningsCatalog")) + 1).trim().replace(/;\s*$/, "");
    const data = JSON.parse(json); // plain JSON after the assignment, which is what lets the shell read it
    assertEqual(data.openings.length, 28, "openings in the catalog");
    const by = (f) => data.openings.filter((o) => o.family === f).length;
    assertEqual([by("house"), by("power"), by("jobs"), by("hours"), by("edition"), by("material")].join(","), "2,8,6,4,3,5", "the six ladders");
    for (const o of data.openings) assertEqual(Object.keys(o.earn).length, 1, o.id + " has exactly one way to be earned");
    const page = await open(browser);
    const pool = await page.evaluate(() => window.RichSplash.pool().map((v) => ({ id: v.id, seconds: v.tokens.seconds || null })));
    assertEqual(pool.length, 30, "the page's pool: the library's two, then the 28");
    assertEqual(pool.slice(0, 2).map((p) => p.id).join(" "), "round-11/v1 round-11/v2", "the library's own entries come first, so the rule of 2026-09-01 still reads them");
    assertEqual(pool.slice(2).map((p) => p.id).join(" "), data.openings.map((o) => "opening/" + o.id).join(" "), "the openings, in the catalog's order (the shell builds the same list)");
    for (const p of pool.slice(2)) {
      const five = /opening\/(power|jobs|hours|edition)-/.test(p.id);
      assertEqual(p.seconds, five ? "5" : null, p.id + ": a milestone or an edition is up for 5 seconds, the rest for their own 3");
    }
    assertEqual(page.__errors.length, 0, "page errors: " + page.__errors.join("; "));
    await page.__ctx.close();
    return "28 rows, 30 entries in the pool, every one valid";
  });

  await run.check("2  the splash screen has no off switch: a start that was switched off plays it, and Settings holds the sound's switch in its place", async () => {
    // What a user who had turned it off carries: the old local mirror, and (from an older shell)
    // the old flag in the launch object.
    const page = await open(browser, {
      launch: { kind: "fresh", ordinal: 3, splashEnabled: false },
      init: () => window.localStorage.setItem("richos.splash.enabled", "false"),
    });
    const s = await page.evaluate(() => ({ shown: window.RichSplash.state.shown, declined: window.RichSplash.state.declined, up: !!document.getElementById("splash") }));
    assert(s.shown && s.up, "the splash did not play for a user who had switched it off: " + JSON.stringify(s));
    assertEqual(s.declined, null, "the splash gave a reason for not drawing");
    const row = await page.evaluate(() => ({
      old: document.querySelectorAll("#splash-enabled").length,
      sound: document.querySelectorAll("#splash-sound").length,
      label: (document.querySelector("#splash-sound") || { closest: () => null }).closest("label")?.textContent.trim(),
      title: document.querySelector('[data-settings-part="splash"] .popover-title')?.textContent.trim(),
      key: "KEY_ENABLED" in window.RichSplash,
    }));
    assertEqual(row.old, 0, "the off switch is still in the document");
    assertEqual(row.sound, 1, "the sound switch that replaced it");
    assertEqual(row.title + " / " + row.label, "Splash screen / Play its sound", "the row's title and label");
    assert(!row.key, "splash.js still exports the off switch's key");
    for (const file of ["splash.js", "main.js", "index.html"]) {
      assert(!/splash_enabled|splashEnabled|set_splash_enabled|richos\.splash\.enabled/.test(read(file)), file + " still reads or writes the off switch");
    }
    const shell = fs.readFileSync(path.join(UI_DIR, "..", "src-tauri", "src", "main.rs"), "utf8");
    assert(!/fn splash_enabled|fn set_splash_enabled|splashEnabled/.test(shell), "main.rs still has the off switch's commands or its flag");
    const config = fs.readFileSync(path.join(UI_DIR, "..", "crates", "richos-core", "src", "config.rs"), "utf8");
    assert(!/^\s*splash_enabled: bool|^\s*splash_disabled_at:/m.test(config), "config.rs still stores the off switch or its stamp");
    await page.__ctx.close();
    return "splashEnabled:false and the old local mirror are ignored; #splash-enabled is gone; #splash-sound is there";
  });

  await run.check("3  the sound is Two notes: its second note meets the bar's landing, the grand cut plays on a milestone, and off is silent", async () => {
    const out = [];
    // Every day: `two-note.wav`, started 130 ms before the bar lands, which is 800 ms before the
    // screen's 3 seconds are up: 3000 - 800 - 130 = 2070.
    let page = await open(browser, { record: { sound: true }, virtualTime: true });
    await page.clock.runFor(2000);
    let a = await audio(page);
    assertEqual(a.length, 1, "one file made ready for the landing");
    assertEqual(a[0].src, "assets/sounds/two-note.wav", "the everyday sound");
    assert(a[0].loaded && a[0].playedAt === null, "at 2.0 s it is loaded and has not played: " + JSON.stringify(a[0]));
    await page.clock.runFor(140);
    a = await audio(page);
    assert(a[0].playedAt !== null && a[0].playedAt >= 2070 && a[0].playedAt <= 2070 + 40, "it starts at 2,070 ms (within two frames); started at " + a[0].playedAt);
    out.push("everyday: " + a[0].src + " at " + a[0].playedAt + " ms");
    await page.clock.runFor(3000);
    assertEqual((await audio(page)).length, 1, "it plays once");
    await page.__ctx.close();

    // A milestone, as his pick: the grand cut, 130 ms before a landing at 5000 - 800.
    page = await open(browser, { record: { sound: true, pick: "power-100k" }, virtualTime: true });
    assertEqual(await page.evaluate(() => document.getElementById("splash").dataset.opening), "power-100k", "the picked milestone is up");
    await page.clock.runFor(4060);
    a = await audio(page);
    assertEqual(a[0].src, "assets/sounds/two-note-grand.wav", "the milestone sound");
    assertEqual(a[0].playedAt, null, "at 4.06 s it has not played");
    await page.clock.runFor(60);
    a = await audio(page);
    assert(a[0].playedAt >= 4070 && a[0].playedAt <= 4110, "it starts at 4,070 ms; started at " + a[0].playedAt);
    out.push("milestone: " + a[0].src + " at " + a[0].playedAt + " ms");
    await page.__ctx.close();

    // Switched off, a page with no shell behind it, and the page nobody sees: nothing is even loaded.
    for (const [what, opts] of [
      ["switched off", { record: { sound: false } }],
      ["no record (a plain browser)", { record: null }],
      ["the page drawn at quit", { record: { sound: true }, launch: { kind: "fresh", ordinal: null, draw: true, splash: { id: "round-11/v1" } }, url: "file://" + path.join(UI_DIR, "splash-opening.html") }],
    ]) {
      page = await open(browser, Object.assign({ virtualTime: true }, opts));
      await page.clock.runFor(3500);
      assertEqual((await audio(page)).length, 0, what + ": a sound file was made ready");
      await page.__ctx.close();
      out.push(what + ": silent");
    }
    // The files are the round's own two.
    for (const f of ["two-note.wav", "two-note-grand.wav"]) {
      const bytes = fs.readFileSync(path.join(UI_DIR, "assets", "sounds", f));
      assertEqual(bytes.slice(0, 4).toString() + bytes.slice(8, 12).toString(), "RIFFWAVE", f + " is a WAV file");
    }
    assertEqual(fs.readdirSync(path.join(UI_DIR, "assets", "sounds")).sort().join(" "), "two-note-grand.wav two-note.wav", "only the sound he chose ships");
    return out.join(" · ");
  });

  await run.check("4  the Settings switch turns the sound off and on, and the gallery's own switch says the same", async () => {
    const page = await openGallery(browser, { record: { sound: true } });
    const state = () => page.evaluate(() => ({ row: document.getElementById("splash-sound").checked, gallery: document.getElementById("opg-sound").checked, on: window.RichOpenings.sound.on() }));
    assertEqual(JSON.stringify(await state()), JSON.stringify({ row: true, gallery: true, on: true }), "on by default, in both places");
    // Off, from the gallery's switch.
    await page.click("#opg-sound");
    await page.waitForFunction(() => window.__RICHOS_MOCK__.openingsRecord().sound === false);
    assertEqual(JSON.stringify(await state()), JSON.stringify({ row: false, gallery: false, on: false }), "off, in both places");
    assertEqual((await audio(page)).filter((a) => a.playedAt !== null).length, 0, "turning it off played something");
    // On again, from the Settings row (the panel closed when the gallery opened; the row is the same node).
    await page.evaluate(() => window.RichOpeningsPage.close());
    await page.evaluate(() => window.RichSettings.openMenu());
    await page.click("#splash-sound");
    await page.waitForFunction(() => window.__RICHOS_MOCK__.openingsRecord().sound === true);
    assertEqual(JSON.stringify(await state()), JSON.stringify({ row: true, gallery: true, on: true }), "on again, in both places");
    const sent = (await calls(page, "set_opening_sound")).map((c) => c.on).join(",");
    assertEqual(sent, "false,true", "what the shell was told");
    // Turning it on answers with the sound itself, once.
    const played = (await audio(page)).filter((a) => a.playedAt !== null);
    assertEqual(played.map((a) => a.src).join(" "), "assets/sounds/two-note.wav", "the confirmation when it is switched on");
    assertEqual(page.__errors.length, 0, "page errors: " + page.__errors.join("; "));
    await page.__ctx.close();
    return "set_opening_sound " + sent + "; both switches agree at every step";
  });

  await run.check("5  the gallery shows what is his and nothing about what is not", async () => {
    const page = await openGallery(browser);
    const seen = await page.evaluate(() => ({
      cards: Array.from(document.querySelectorAll("#openings-page .opg-card[data-opening]")).map((c) => c.dataset.opening),
      sections: Array.from(document.querySelectorAll("#openings-page .opg-section")).map((s) => ({
        title: s.querySelector(".opg-section-title").textContent,
        mine: s.querySelectorAll(".opg-card[data-opening]").length,
        sealed: s.querySelectorAll(".opg-card--sealed").length,
      })),
      lede: document.querySelector("#openings-page .opg-lede").textContent,
      text: document.getElementById("openings-page").textContent,
      struck: (() => {
        const c = document.querySelector('.opg-card[data-opening="power-100k"]');
        return [c.querySelector(".opg-no").textContent, c.querySelector(".opg-name").textContent, c.querySelector(".opg-what").textContent, c.querySelector(".opg-when").textContent].join(" | ");
      })(),
      update: document.querySelector('.opg-card[data-opening="edition-update"] .opg-what').textContent,
      posters: Array.from(document.querySelectorAll("#openings-page .opg-poster img")).map((i) => i.getAttribute("src")),
    }));
    assertEqual(seen.cards.join(" "), EARNED.join(" "), "one card for each opening that is his, in the catalog's order");
    assert(/13 so far/.test(seen.lede), "the count: " + seen.lede);
    assertEqual(
      seen.sections.map((s) => s.title + " " + s.mine + "+" + s.sealed).join(" · "),
      "The two openings 2+0 · AI power harnessed 5+1 · Jobs completed 0+1 · Hours reclaimed 0+1 · Rare editions 3+0 · Materials 3+1",
      "each ladder: what is his, and ONE sealed tile where there is more"
    );
    assertEqual(seen.struck, "No. 7 | Struck | $100,000 in AI power harnessed | Earned September 2, 2026", "a card says its number, its name, what it shows and when it became his");
    assertEqual(seen.update, "1.4.0 RichOS, updated and ready", "the update edition shows the version that is running");
    // Never an opening he has not earned, by name or by number.
    for (const hidden of ["Bullion", "The Vault", "Sovereign", "$250,000", "The First Job", "Half a Day", "Guilloché", "Gold Leaf"]) {
      assert(!seen.text.includes(hidden), "the gallery names an opening that is not his: " + hidden);
    }
    for (const src of seen.posters) assert(fs.existsSync(path.join(UI_DIR, src)), "a card's picture is not shipped: " + src);
    for (const o of JSON.parse(read("openings-catalog.js").replace(/^[\s\S]*?=\s*/, "").replace(/;\s*$/, "")).openings) {
      assert(fs.existsSync(path.join(UI_DIR, "assets", "openings", "posters", o.id + ".jpg")), "no picture for " + o.id);
    }
    assertEqual(page.__errors.length, 0, "page errors: " + page.__errors.join("; "));
    await page.__ctx.close();
    return seen.cards.length + " cards, 4 sealed tiles, none naming what is behind it";
  });

  await run.check("6  Play runs the whole opening and returns focus to the gallery", async () => {
    const page = await openGallery(browser, { record: { company: "Acme Robotics" } });
    const before = await page.evaluate(() => JSON.stringify([window.RichSplash.state.reason, window.RichSplash.state.variationId]));
    await page.click('.opg-card[data-opening="power-100k"] .opg-btn--play');
    await page.waitForFunction(() => window.RichSplash.isUp());
    const up = await page.evaluate(() => {
      const n = document.getElementById("splash");
      return {
        opening: n.dataset.opening,
        variation: n.dataset.variation,
        figure: !!n.querySelector(".ro-figure"),
        caption: n.querySelector(".ro-caption").textContent,
        company: n.querySelector(".ro-company").textContent,
        collector: n.querySelector(".ro-collector").textContent,
        above: Number(getComputedStyle(n).zIndex) > Number(getComputedStyle(document.getElementById("openings-page")).zIndex),
      };
    });
    assertEqual(up.opening, "power-100k", "the opening that was asked for");
    assert(up.figure, "its number is on it");
    assertEqual(up.caption, "in AI power harnessed", "its line");
    assertEqual(up.company, "Acme Robotics", "his company's line, under ours");
    assertEqual(up.collector, "No. 7·Struck·Earned September 2, 2026", "the held line");
    assert(up.above, "the opening plays above the gallery");
    // The number counts up: whole by 3.0 s. Held at moments, so this reads the page, not a clock.
    const counted = await page.evaluate(() => {
      const at = (ms) => { window.RichSplash.holdAt(ms); return document.querySelector("#splash .ro-figure-num").textContent; };
      const r = [at(0), at(600), at(1500), at(3000), at(4200)];
      window.RichSplash.release();
      return r;
    });
    assertEqual(counted[0] + " " + counted[1], "0 0", "it starts at nothing");
    assert(Number(counted[2].replace(/,/g, "")) > 0 && Number(counted[2].replace(/,/g, "")) < 100000, "it is counting at 1.5 s: " + counted[2]);
    assertEqual(counted[3] + " " + counted[4], "100,000 100,000", "it is whole by 3.0 s and stays");
    await page.keyboard.press("Space");
    await page.mouse.click(700, 80);
    assert(await page.evaluate(() => window.RichSplash.isUp()), "input ended the preview");
    await page.waitForFunction(() => !window.RichSplash.isUp());
    const after = await page.evaluate(() => ({
      open: window.RichOpeningsPage.isOpen(),
      focus: document.activeElement.className + "@" + (document.activeElement.closest(".opg-card") || { dataset: {} }).dataset.opening,
      launch: JSON.stringify([window.RichSplash.state.reason, window.RichSplash.state.variationId]),
      replayed: window.RichSplash.state.replayed,
      how: window.RichSplash.state.replayReason,
      leftovers: document.querySelectorAll("#splash, .ro-handover").length,
    }));
    assert(after.open, "the gallery closed when the opening left");
    assertEqual(after.focus, "opg-btn opg-btn--play@power-100k", "focus is back on the Play he pressed");
    assertEqual(after.launch, before, "a replay rewrote the start's own record");
    assertEqual(after.replayed + " / " + after.how, "opening/power-100k / ceiling", "what was replayed, and how it left");
    assertEqual(after.leftovers, 0, "something of the opening was left on the page");
    // Left alone it leaves by itself when its own time is up (3 s for a material).
    await page.click('.opg-card[data-opening="material-leather"] .opg-poster');
    await page.waitForFunction(() => window.RichSplash.isUp());
    const started = Date.now();
    await page.waitForFunction(() => !window.RichSplash.isUp());
    // How it left is the fact: "ceiling" is its own time running out, with nobody's hand on it.
    // How long that took on this machine is said in the detail line and judges nothing.
    const took = Date.now() - started;
    assertEqual(await page.evaluate(() => window.RichSplash.state.replayReason), "ceiling", "it left when its time was up");
    assertEqual((await audio(page)).length, 0, "with the sound off a replay made a sound file ready");
    assertEqual(page.__errors.length, 0, "page errors: " + page.__errors.join("; "));
    await page.__ctx.close();
    return "power-100k: " + counted.join(" → ") + "; played through keyboard and mouse input; material-leather left by itself after " + took + " ms";
  });

  await run.check("7  Make it my opening: RichOS starts with that one from then on, and only one that is his can be picked", async () => {
    const page = await openGallery(browser);
    const tags = () => page.evaluate(() => Array.from(document.querySelectorAll("#openings-page .opg-card[data-opening]")).filter((c) => c.querySelector(".opg-tag").textContent).map((c) => c.dataset.opening + ":" + c.querySelector(".opg-tag").textContent + ":" + c.querySelector(".opg-btn--mine").textContent + (c.querySelector(".opg-btn--mine").disabled ? "(off)" : "")));
    assertEqual((await tags()).join(" "), "ruled-standard:Opens RichOS:Your opening(off)", "before he picks, the app's own rule is marked");
    await page.click('.opg-card[data-opening="power-100k"] .opg-btn--mine');
    await page.waitForFunction(() => window.__RICHOS_MOCK__.openingsRecord().pick === "power-100k");
    await page.waitForFunction(() => document.querySelector('.opg-card[data-opening="power-100k"] .opg-tag').textContent !== "");
    assertEqual((await tags()).join(" "), "power-100k:Opens RichOS:Your opening(off)", "his pick is marked, and only it");
    assertEqual(await page.evaluate(() => document.querySelector("#openings-page .opg-live").textContent), "Struck is your opening now.", "what the page says");
    // Pressed from the keyboard, the press leaves the keyboard on the page: the button he pressed
    // is now off ("Your opening"), so focus goes to Back, never to the screen under the gallery.
    await page.focus('.opg-card[data-opening="material-steel"] .opg-btn--mine');
    await page.keyboard.press("Enter");
    await page.waitForFunction(() => window.__RICHOS_MOCK__.openingsRecord().pick === "material-steel");
    await page.waitForFunction(() => document.querySelector('.opg-card[data-opening="material-steel"] .opg-tag').textContent !== "");
    assertEqual(await page.evaluate(() => document.activeElement.className), "opg-back", "where the keyboard is after a pick made from it");
    assert(await page.evaluate(() => window.RichOpeningsPage.isOpen()), "Enter on a card's button reached the screen under the gallery");
    await page.click('.opg-card[data-opening="power-100k"] .opg-btn--mine');
    await page.waitForFunction(() => window.__RICHOS_MOCK__.openingsRecord().pick === "power-100k");
    await page.waitForFunction(() => document.querySelector('.opg-card[data-opening="power-100k"] .opg-tag').textContent !== "");
    assertEqual((await calls(page, "set_opening_pick")).map((c) => c.pick).join(","), "power-100k,material-steel,power-100k", "what the shell was told");
    // The page's own pick, where the shell said nothing, is the same one (a 5-second milestone).
    const fallback = await page.evaluate(() => {
      const pool = window.RichSplash.pool();
      return [1, 2, 3, null].map((start) => window.RichSplash.choose(pool, start).id).join(" ");
    });
    assertEqual(fallback, "opening/power-100k opening/power-100k opening/power-100k opening/power-100k", "his pick, on every start");
    // An opening that is not his cannot be picked: it has no card, and the shell refuses it.
    const refused = await page.evaluate(() => window.RichBridge.invoke("set_opening_pick", { pick: "power-1m" }).then(() => "accepted", (e) => String(e)));
    assertEqual(refused, "That opening is not one of yours yet.", "picking an opening that is not his");
    assertEqual(await page.evaluate(() => window.__RICHOS_MOCK__.openingsRecord().pick), "power-100k", "the refused pick changed nothing");
    await page.__ctx.close();

    // The next start, with that pick and no word from the shell: his opening, for its own 5 seconds.
    const next = await open(browser, { record: { pick: "power-100k" } });
    const s = await next.evaluate(() => ({ id: window.RichSplash.state.variationId, seconds: window.RichSplash.state.seconds, by: window.RichSplash.state.pickedBy }));
    assertEqual(s.id + " " + s.seconds + " " + s.by, "opening/power-100k 5 page", "the start after he picked");
    // And the shell's pick, when it names an opening, is drawn as it is for the library's own.
    await next.__ctx.close();
    const shell = await open(browser, { launch: { kind: "fresh", ordinal: 9, splash: { id: "opening/material-steel", opening: false } } });
    const d = await shell.evaluate(() => ({ id: window.RichSplash.state.variationId, by: window.RichSplash.state.pickedBy, opening: document.getElementById("splash").dataset.opening, seconds: window.RichSplash.state.seconds }));
    assertEqual([d.id, d.by, d.opening, d.seconds].join(" "), "opening/material-steel shell material-steel 3", "the shell's pick of an opening");
    await shell.__ctx.close();
    // A pick that is not his (an older record, a dropped opening) is no pick: the rule stands.
    const stale = await open(browser, { record: { pick: "power-1m" }, launch: { kind: "fresh", ordinal: 2 } });
    assertEqual(await stale.evaluate(() => window.RichSplash.state.variationId), "round-11/v2", "start 2 with a pick that is not his");
    await stale.__ctx.close();
    return "picked power-100k; marked; drawn on the next start for 5 s; power-1m refused";
  });

  await run.check("8  One of mine, a different one each day: the calendar picks among what he has, the same day the same one", async () => {
    const page = await openGallery(browser, { record: { day: 20000 } });
    await page.click("#opg-mode-daily");
    await page.waitForFunction(() => window.__RICHOS_MOCK__.openingsRecord().pick === "daily");
    await page.waitForFunction(() => /today/.test(document.getElementById("openings-page").textContent));
    const today = EARNED[20000 % EARNED.length];
    const marked = await page.evaluate(() => Array.from(document.querySelectorAll("#openings-page .opg-card[data-opening]")).filter((c) => c.querySelector(".opg-tag").textContent).map((c) => c.dataset.opening + ":" + c.querySelector(".opg-tag").textContent + ":" + (c.querySelector(".opg-btn--mine").disabled ? "off" : "on")));
    assertEqual(marked.join(" "), today + ":Opens RichOS today:on", "today's is marked as today's, and any can still be made his opening");
    assertEqual(await page.evaluate(() => [document.getElementById("opg-mode-daily").checked, document.getElementById("opg-mode-pick").checked].join()), "true,false", "the two ways to start");
    // Back to "The one I pick": today's becomes his pick.
    await page.click("#opg-mode-pick");
    await page.waitForFunction((t) => window.__RICHOS_MOCK__.openingsRecord().pick === t, today);
    assertEqual((await calls(page, "set_opening_pick")).map((c) => c.pick).join(","), "daily," + today, "what the shell was told");
    await page.__ctx.close();

    // Thirteen days in a row are his thirteen openings, each once, in their own order; the
    // fourteenth is the first again; the same day asked twice is the same opening.
    const seen = [];
    for (const day of [20000, 20001, 20002, 20000 + EARNED.length, 20001]) {
      const p = await open(browser, { record: { pick: "daily", day } });
      seen.push(await p.evaluate(() => document.getElementById("splash").dataset.opening));
      await p.__ctx.close();
    }
    const at = (d) => EARNED[d % EARNED.length];
    assertEqual(seen.join(" "), [at(20000), at(20001), at(20002), at(20000), at(20001)].join(" "), "which opening each day starts with");
    // THE SHELL'S WORD WINS: his openings take turns, a later day on the one after the last
    // (so a Monday follows Friday's), and the shell says which one that is. One that is not his
    // is not taken at its word.
    const turn = at(20003);
    const told = await open(browser, { record: { pick: "daily", day: 20000, today: turn } });
    assertEqual(await told.evaluate(() => document.getElementById("splash").dataset.opening), turn, "the opening the shell said today opens on");
    await told.__ctx.close();
    const wrong = await open(browser, { record: { pick: "daily", day: 20000, today: "power-1m" } });
    assertEqual(await wrong.evaluate(() => document.getElementById("splash").dataset.opening), at(20000), "told one that is not his");
    await wrong.__ctx.close();
    assert(new Set(seen.slice(0, 3)).size === 3, "three days in a row were not three openings");
    // Every day of a long run is one he has.
    const all = await (async () => {
      const p = await open(browser, { record: { pick: "daily" } });
      const r = await p.evaluate(() => {
        const out = new Set();
        const rec = window.RichOpenings.record();
        for (let d = 19000; d < 19400; d++) {
          rec.day = d;
          out.add(window.RichOpenings.current().id);
        }
        return Array.from(out);
      });
      await p.__ctx.close();
      return r;
    })();
    assertEqual(all.slice().sort().join(" "), EARNED.slice().sort().join(" "), "400 days visit every opening he has and no other");
    return "day 20000 → " + seen[0] + ", 20001 → " + seen[1] + ", 20002 → " + seen[2] + "; 400 days stay inside his " + EARNED.length;
  });

  await run.check("9  the ending happens on screen: the bar lands 0.8 s before the time is up, the flare is 620 ms, then the finished frame", async () => {
    const src = read("splash.js");
    assert(/var FLARE_MS = 620;/.test(src), "the flare is not 620 ms");
    const page = await open(browser, { launch: { kind: "fresh", ordinal: null, draw: true, splash: { id: "round-11/v1" } }, url: "file://" + path.join(UI_DIR, "splash-opening.html") });
    const m = await page.evaluate(() => {
      const fill = () => {
        const layers = document.querySelectorAll('#splash .splash-bar-layer[data-role="progress"]');
        const track = document.querySelector("#splash .splash-bar").getBoundingClientRect().width;
        return Math.round((layers[layers.length - 1].getBoundingClientRect().width / track) * 1000) / 10;
      };
      const flare = () => Array.from(document.querySelectorAll('#splash .splash-bar-layer[data-role="flare"]')).some((f) => Number(getComputedStyle(f).opacity) > 0.01);
      const at = (ms) => { window.RichSplash.holdAt(ms); return { fill: fill(), flare: flare() }; };
      return { early: at(2100), landed: at(2200), flaring: at(2400), after: at(2200 + 630), end: at(3000), land: window.RichOpenings.landEarly() };
    });
    assertEqual(m.land, 800, "how early the bar lands");
    assert(m.early.fill < 100, "at 2.1 s the bar is already full: " + m.early.fill + "%");
    assertEqual(m.landed.fill, 100, "at 2.2 s (3.0 - 0.8) the bar has landed");
    assert(m.flaring.flare, "at 2.4 s the flare is not crossing the bar");
    assert(!m.after.flare, "630 ms after the landing the flare is still on");
    assert(!m.end.flare && m.end.fill === 100, "the last 170 ms are not the finished frame: " + JSON.stringify(m.end));
    await page.__ctx.close();
    return "fill " + m.early.fill + "% at 2.1 s, 100% at 2.2 s; flare on at 2.4 s, off by 2.83 s; still at 3.0 s";
  });

  await run.check("10  the lamp stays in its authored position across the catalog", async () => {
    const ids = JSON.parse(read("openings-catalog.js").replace(/^[\s\S]*?=\s*/, "").replace(/;\s*$/, "")).openings;
    for (const { id } of ids) {
      const page = await open(browser, { launch: { kind: "fresh", draw: true, splash: { id: "opening/" + id } }, url: "file://" + path.join(UI_DIR, "splash-opening.html") });
      await page.mouse.move(1150, 260);
      const light = await page.evaluate(() => {
        const root = document.getElementById("splash");
        return [0, 1500, 2900].map((ms) => {
          window.RichSplash.holdAt(ms);
          return [getComputedStyle(root.querySelector(".splash-lamp")).backgroundImage, root.querySelectorAll(".ro-pool, .ro-glint, .ro-bar-glint").length];
        });
      });
      assert(light[0][0].includes("40% 40%"), id + ": the lamp moved from its authored origin");
      for (const sample of light) assertEqual(sample, [light[0][0], 0], id + ": pointer light or glint");
      await page.__ctx.close();
    }
    return ids.length + " openings use the authored lamp throughout";
  });

  await run.check("11  what a milestone paints stands at a moment: the same held moment is the same picture, lights and all", async () => {
    const page = await open(browser, { launch: { kind: "fresh", ordinal: null, draw: true, splash: { id: "opening/jobs-500" } }, url: "file://" + path.join(UI_DIR, "splash-opening.html"), record: { earned: Object.assign({}, USER.earned, { "jobs-500": "2026-02-20" }) } });
    const r = await page.evaluate(() => {
      const canvas = document.querySelector("#splash .ro-fx");
      const sum = () => {
        const d = canvas.getContext("2d").getImageData(0, 0, canvas.width, canvas.height).data;
        let lit = 0, total = 0;
        for (let i = 3; i < d.length; i += 4) { if (d[i]) lit++; total += d[i]; }
        return lit + ":" + total;
      };
      const at = (ms) => { window.RichSplash.holdAt(ms); return { lights: sum(), n: document.querySelector("#splash .ro-figure-num").textContent }; };
      const a = at(900), b = at(300), c = at(900), end = at(4200);
      return { a, b, c, end, region: window.RichSplash.openingRegion(1000), w: innerWidth, h: innerHeight };
    });
    assertEqual(JSON.stringify(r.a), JSON.stringify(r.c), "0.9 s, held twice with another moment between");
    assertEqual(r.b.lights, "0:0", "before 0.6 s no light is on");
    assert(r.a.lights !== "0:0", "at 0.9 s no light has come on");
    assertEqual(r.end.n, "500", "the number is whole at the landing");
    assert(Number(r.end.lights.split(":")[0]) > Number(r.a.lights.split(":")[0]), "more lights are on at the landing than at 0.9 s");
    assertEqual(JSON.stringify(r.region), JSON.stringify([0, 0, r.w, r.h]), "the part the drawn-ahead opening covers is the whole page, since the lights are everywhere");
    await page.__ctx.close();
    return "0.9 s: " + r.a.lights + " (lit pixels:alpha) twice; landing: " + r.end.lights + "; region " + JSON.stringify(r.region);
  });

  await run.check("12  every way in has its way out, and the home screen's keys stay off while the gallery covers it", async () => {
    const page = await openGallery(browser);
    assertEqual(await page.evaluate(() => document.activeElement.className), "opg-back", "it opens with the keyboard on Back");
    assert(await page.evaluate(() => document.body.classList.contains("openings-open") && (!document.querySelector(".settings-panel") || true)), "the page is marked open");
    // Enter and Space, with the home screen under it, do not open Talk to Rich.
    const home = () => page.evaluate(() => !!(window.RichHome && window.RichHome.isVisible && window.RichHome.isVisible()));
    const wasHome = await home();
    await page.evaluate(() => document.activeElement.blur());
    await page.keyboard.press("Enter");
    await page.keyboard.press(" ");
    assertEqual(await home(), wasHome, "a key pressed over the gallery reached the home screen's door");
    assert(await page.evaluate(() => window.RichOpeningsPage.isOpen()), "a key closed the gallery");
    // Escape closes it; so does Back.
    await page.keyboard.press("Escape");
    assert(!(await page.evaluate(() => window.RichOpeningsPage.isOpen())), "Escape did not close the gallery");
    assert(!(await page.evaluate(() => document.body.classList.contains("openings-open"))), "the open mark was left on the page");
    await page.evaluate(() => window.RichSettings.openMenu());
    await page.click("#openings-open");
    await page.click("#openings-page .opg-back");
    assert(!(await page.evaluate(() => window.RichOpeningsPage.isOpen())), "Back did not close the gallery");
    // An opening that became his while the page was shut is on it the next time it opens.
    await page.evaluate(() => window.__RICHOS_MOCK__.openingsEarn("power-250k", "2026-11-14"));
    await page.evaluate(() => window.RichSettings.openMenu());
    await page.click("#openings-open");
    await page.waitForFunction(() => !!document.querySelector('.opg-card[data-opening="power-250k"]'));
    assert(/14 so far/.test(await page.evaluate(() => document.querySelector(".opg-lede").textContent)), "the count did not follow");
    // WHILE HIS SPEND IS STILL BEING COUNTED for the first time, the page asks again by itself,
    // and an opening the count gives him arrives on the wall with nobody pressing anything.
    await page.click("#openings-page .opg-back");
    await page.evaluate(() => window.__RICHOS_MOCK__.openingsCounting(true));
    await page.evaluate(() => window.RichSettings.openMenu());
    await page.click("#openings-open");
    await page.waitForFunction(() => window.RichOpenings.record().counting === true);
    const asked = (await calls(page, "openings_state")).length;
    await page.evaluate(() => { window.__RICHOS_MOCK__.openingsEarn("power-500k", "2026-11-15"); window.__RICHOS_MOCK__.openingsCounting(false); });
    await page.waitForFunction(() => !!document.querySelector('.opg-card[data-opening="power-500k"]'));
    assert((await calls(page, "openings_state")).length > asked, "the page did not ask the shell again while the count was running");
    assertEqual(await page.evaluate(() => window.RichOpenings.record().counting), false, "the page still thinks the count is running");
    const settled = (await calls(page, "openings_state")).length;
    await page.evaluate(() => new Promise((r) => setTimeout(r, 2500)));
    assertEqual((await calls(page, "openings_state")).length, settled, "the page keeps asking after the count is done");
    assertEqual(page.__errors.length, 0, "page errors: " + page.__errors.join("; "));
    await page.__ctx.close();

    // TOKEN SPEND IS A SCREEN ABOVE THIS PAGE, and Settings is reachable over it: opened from
    // there, the gallery sat under it (the review of 408e628f9). It steps aside now.
    const spend = await open(browser);
    await leaveSplash(spend);
    await spend.waitForFunction(() => window.RichSettings && window.RichOpeningsPage && window.RichSpend);
    await spend.evaluate(() => window.RichSpend.open());
    await spend.waitForFunction(() => window.RichSpend.isOpen());
    await spend.evaluate(() => window.RichSettings.openMenu());
    await spend.click("#openings-open");
    await spend.waitForFunction(() => window.RichOpeningsPage.isOpen());
    const under = await spend.evaluate(() => {
      const b = document.querySelector("#openings-page .opg-back").getBoundingClientRect();
      const top = document.elementFromPoint(b.x + b.width / 2, b.y + b.height / 2);
      return [window.RichSpend.isOpen(), document.body.classList.contains("spend-open"), top ? top.className : null, document.activeElement.className].join(" / ");
    });
    assertEqual(under, "false / false / opg-back / opg-back", "Token spend still open, its body class, what is on top where Back is, and where the focus is");
    assertEqual(spend.__errors.length, 0, "page errors: " + spend.__errors.join("; "));
    await spend.__ctx.close();

    // AND THE OTHER WAY ROUND (the review of aae1474f9): with the gallery up, Settings is still
    // reachable over it. A sheet one of its rows opens lives under this page, so the page gives
    // way to it, whichever sheet it is; and Escape pressed in Settings closes Settings, not the
    // page under it.
    const way = await openGallery(browser);
    await way.evaluate(() => window.RichSettings.openMenu());
    await way.keyboard.press("Escape");
    await way.waitForFunction(() => document.getElementById("set-menu").hidden);
    assertEqual(await way.evaluate(() => window.RichOpeningsPage.isOpen()), true, "the gallery after Escape was pressed in Settings");
    await way.evaluate(() => window.RichSettings.openMenu());
    await way.click("#set-repositories-open");
    await way.waitForFunction(() => !window.RichOpeningsPage.isOpen());
    // Home was under the gallery and gives way to the same sheet by its own rule; it leaves
    // over a moment, so the sheet is on top once Home has gone.
    const onTop = () => {
      const up = Array.from(document.querySelectorAll(".overlay")).filter((o) => !o.hidden);
      if (up.length !== 1) return false;
      const panel = up[0].querySelector(".overlay-panel") || up[0];
      const b = panel.getBoundingClientRect();
      const top = document.elementFromPoint(b.x + b.width / 2, b.y + 12);
      return !!top && up[0].contains(top);
    };
    await way.waitForFunction(onTop);
    const sheet = await way.evaluate(() => {
      const up = Array.from(document.querySelectorAll(".overlay")).filter((o) => !o.hidden);
      return [up.length, up.length && up[0].contains(document.activeElement), document.body.classList.contains("openings-open")].join(" / ");
    });
    assertEqual(sheet, "1 / true / false", "sheets up, the focus inside it, the gallery's body class");
    assertEqual(way.__errors.length, 0, "page errors: " + way.__errors.join("; "));
    await way.__ctx.close();
    return "Back and Escape close it; Enter and Space reach nothing under it; a newly earned opening appears on the next open, and by itself while the spend is still being counted; Token spend gives way when the gallery opens; the gallery gives way to a sheet opened from Settings and leaves Settings its Escape";
  });

  await run.check("13  Save image asks the shell for that opening's picture and says where it went; his company's line is saved", async () => {
    const page = await openGallery(browser);
    await page.click('.opg-card[data-opening="power-100k"] .opg-btn--save');
    await page.waitForFunction(() => /Saved to your Downloads folder/.test(document.querySelector(".opg-live").textContent));
    assertEqual((await calls(page, "save_opening_image")).map((c) => c.id).join(","), "power-100k", "what the shell was asked to draw");
    await page.fill("#opg-company", "Acme Robotics");
    await page.waitForFunction(() => window.__RICHOS_MOCK__.openingsRecord().company === "Acme Robotics");
    assertEqual(await page.evaluate(() => document.getElementById("opg-company").maxLength), 32, "the line's limit");
    // The line is on the next opening he plays.
    await page.click('.opg-card[data-opening="ruled-standard"] .opg-btn--play');
    await page.waitForFunction(() => window.RichSplash.isUp());
    assertEqual(await page.evaluate(() => document.querySelector("#splash .ro-company").textContent), "Acme Robotics", "his company's line on the opening");
    const sizes = await page.evaluate(() => [parseFloat(getComputedStyle(document.querySelector("#splash .ro-company")).fontSize), parseFloat(getComputedStyle(document.querySelector("#splash .splash-line")).fontSize)]);
    assert(sizes[0] <= sizes[1], "his company's line is larger than ours: " + sizes.join(" vs "));
    await page.keyboard.press("Escape");
    await page.waitForFunction(() => !window.RichSplash.isUp());
    assert(await page.evaluate(() => window.RichOpeningsPage.isOpen()), "Escape over a playing opening closed the gallery too");
    await page.__ctx.close();
    return "save_opening_image power-100k; company line saved and drawn";
  });

  await run.check("14  the handover: a landed bar leaves the splash for Talk to Rich; one cut short does not", async () => {
    // Landed: the screen's own time ran out.
    const page = await open(browser, { virtualTime: true });
    await page.clock.runFor(2950);
    assert(await page.evaluate(() => document.getElementById("splash").classList.contains("splash--landed")), "at 2.95 s the bar has not landed");
    await page.evaluate(() => window.RichSplash.yieldNow("acceptance-suite"));
    const handed = await page.evaluate(() => document.querySelectorAll(".ro-handover").length);
    const door = await page.evaluate(() => !!document.getElementById("home-enter") && document.getElementById("home-enter").getBoundingClientRect().width > 0);
    if (door) assertEqual(handed, 1, "a landed bar was not handed to the home screen");
    await page.clock.runFor(1500);
    assertEqual(await page.evaluate(() => document.querySelectorAll(".ro-handover, #splash").length), 0, "the handover or the splash was left on the page");
    await page.__ctx.close();
    // Cut short by his hand at 1 s: a plain fade, nothing crosses.
    const cut = await open(browser, { virtualTime: true });
    await cut.clock.runFor(1000);
    await cut.evaluate(() => window.RichSplash.yieldNow("acceptance-suite"));
    assertEqual(await cut.evaluate(() => document.querySelectorAll(".ro-handover").length), 0, "a bar that had not landed was handed over");
    await cut.__ctx.close();
    return door ? "landed: one line crosses to Talk to Rich and is gone; cut short: none" : "no Talk to Rich door on this page; cut short: none";
  });

  for (const scheme of ["dark", "light"]) {
    await run.check("15  contrast, computed (" + scheme + "): the gallery's text and the Settings row clear WCAG AA", async () => {
      const page = await openGallery(browser, { record: { pick: "power-100k", company: "Acme" }, init: scheme === "light" ? () => window.localStorage.setItem("richos.theme", "light") : undefined });
      await page.evaluate((s) => { if (window.RichTheme && window.RichTheme.set) window.RichTheme.set(s); else document.documentElement.setAttribute("data-theme", s); }, scheme);
      await page.click('.opg-card[data-opening="power-100k"] .opg-btn--save');
      await page.waitForFunction(() => /Saved/.test(document.querySelector(".opg-live").textContent));
      const read = (selector, root) =>
        page.evaluate(
          ([sel]) => {
            const out = [];
            for (const node of document.querySelectorAll(sel)) {
              const r = node.getBoundingClientRect();
              if (!r.width || !r.height) continue;
              const cs = getComputedStyle(node);
              let bg = [];
              for (let el = node; el; el = el.parentElement) bg.push(getComputedStyle(el).backgroundColor);
              out.push({ color: cs.color, border: cs.borderTopColor, bg, px: parseFloat(cs.fontSize), weight: Number(cs.fontWeight), text: node.textContent.trim().slice(0, 40) });
            }
            return out;
          },
          [selector, root]
        );
      const ground = (stack) => {
        // The first opaque ground behind the node, with every translucent layer over it composited.
        let base = null;
        const over = [];
        for (const b of stack) {
          const c = parseCssColor(b);
          if (!c || c.a === 0) continue;
          if (c.a >= 1) { base = c; break; }
          over.push(c);
        }
        if (!base) base = parseCssColor(scheme === "light" ? "#ffffff" : "#0c1322");
        for (const c of over.reverse()) base = compositeOver(c, base);
        return base;
      };
      const measured = [];
      const hold = (what, nodes, floor) => {
        assert(nodes.length > 0, scheme + ": nothing on the page for " + what);
        let worst = Infinity, at = "";
        for (const n of nodes) {
          const g = ground(n.bg);
          const ratio = contrastRatio(compositeOver(parseCssColor(n.color), g), g);
          const large = n.px >= 24 || (n.px >= 18.66 && n.weight >= 700);
          const need = floor || (large ? 3 : 4.5);
          if (ratio < worst) { worst = ratio; at = n.text; }
          assert(ratio >= need, scheme + ": " + what + " \"" + n.text + "\" is " + ratio.toFixed(2) + ":1 at " + n.px + "px, under " + need + ":1");
          assert(n.px >= 14, scheme + ": " + what + " is " + n.px + "px, under the 14px floor");
        }
        measured.push(what + " " + worst.toFixed(2));
      };
      // The gallery is always dark, like the screens it shows (declared in openings.css): one
      // set of numbers, measured here under both of the app's lightings to prove it does not move.
      hold("title", await read("#openings-page .opg-title"));
      hold("lede", await read("#openings-page .opg-lede"));
      hold("labels (14px tier)", await read("#openings-page .opg-set-title, #openings-page .opg-section-title, #openings-page .opg-hint"));
      hold("options", await read("#openings-page .opg-option span"));
      hold("back", await read("#openings-page .opg-back"));
      hold("card number and name", await read("#openings-page .opg-no, #openings-page .opg-name"));
      hold("card lines", await read("#openings-page .opg-what, #openings-page .opg-when"));
      hold("card actions", await read("#openings-page .opg-btn"));
      hold("the tag on a picture", await read("#openings-page .opg-tag"));
      hold("what the page says", await read("#openings-page .opg-live"));
      hold("the company field", await read("#openings-page .opg-input"));
      // The one thing that follows the app's theme: the row in Settings.
      await page.evaluate(() => window.RichOpeningsPage.close());
      await page.evaluate(() => window.RichSettings.openMenu());
      await page.waitForSelector("#openings-open", { state: "visible" });
      hold("Settings: Your openings", await read("#openings-open"));
      hold("Settings: Play its sound", await read('[data-settings-part="splash"] .popover-option'));
      const outline = (await read("#openings-open"))[0];
      const og = ground(outline.bg);
      const edge = contrastRatio(compositeOver(parseCssColor(outline.border), og), og);
      assert(edge >= 3, scheme + ": the Your openings button's outline is " + edge.toFixed(2) + ":1, under 3:1");
      measured.push("button outline " + edge.toFixed(2));
      await page.__ctx.close();
      return measured.join(" · ");
    });
  }

  await run.check("16  contrast ON THE GLASS: every line an opening carries, read from a photograph of its finished frame", async () => {
    // The opening screen is always dark (CEO §63), so there is one set of numbers. A lamp, a
    // vignette, a grain and, on the materials, a texture all sit between a line's declared
    // color and the eye, so nothing here is computed from styles: each line is photographed
    // (the finished frame the shell saves as an image, at twice the density) and its ink is
    // measured against the LIGHTEST ground it sits on.
    //
    //   ink     the median of the 5% of the line's pixels furthest in luminance from its ground
    //           (the core of the glyphs, never an antialiased edge)
    //   ground  the 95th-percentile luminance of a bare strip of the same surface beside the line
    //
    // Floors: 4.5:1 for the line under the mark, his company's line, a milestone's line and the
    // held line; 3:1 for the number, which is large text at every window size (54 px at least).
    const { decode } = require("./lib/png");
    const { relativeLuminance } = require("./lib/contrast");
    const measure = (img, box, bare) => {
      const dpr = 2;
      const pixels = (r) => {
        const out = [];
        const x0 = Math.max(0, Math.round(r.x * dpr)), y0 = Math.max(0, Math.round(r.y * dpr));
        const x1 = Math.min(img.width, Math.round((r.x + r.w) * dpr)), y1 = Math.min(img.height, Math.round((r.y + r.h) * dpr));
        for (let y = y0; y < y1; y++) for (let x = x0; x < x1; x++) {
          const o = (y * img.width + x) * 4;
          const c = { r: img.data[o], g: img.data[o + 1], b: img.data[o + 2], a: 1 };
          out.push({ c, l: relativeLuminance(c) });
        }
        return out;
      };
      const strip = pixels(bare).sort((a, b) => a.l - b.l);
      const ground = strip[Math.floor(strip.length * 0.95)];
      const glyphs = pixels(box).map((p) => ({ c: p.c, l: p.l, d: Math.abs(p.l - ground.l) })).sort((a, b) => b.d - a.d);
      const core = glyphs.slice(0, Math.max(3, Math.floor(glyphs.length * 0.05)));
      const ink = core[Math.floor(core.length / 2)];
      return { ratio: Math.round(contrastRatio(ink.c, ground.c) * 100) / 100, n: glyphs.length };
    };
    const out = [];
    const under = [];
    let lowest = { ratio: Infinity };
    const earned = {};
    for (const o of JSON.parse(read("openings-catalog.js").replace(/^[\s\S]*?=\s*/, "").replace(/;\s*$/, "")).openings) earned[o.id] = "2026-01-01";
    for (const id of ["material-leather", "material-slate", "material-steel", "material-enamel", "material-leaf", "power-100", "power-1m", "jobs-500", "hours-20", "edition-year"]) {
      const ctx = await browser.newContext({ viewport: { width: 1440, height: 900 }, deviceScaleFactor: 2 });
      const shot = await ctx.newPage();
      await shot.addInitScript(SILENT_AUDIO);
      await shot.addInitScript((r) => { window.__RICHOS_OPENINGS__ = r; }, Object.assign({}, USER, { earned, company: "Acme Robotics" }));
      await shot.addInitScript((l) => { window.__RICHOS_LAUNCH__ = l; }, { kind: "fresh", ordinal: null, draw: true, card: true, splash: { id: "opening/" + id } });
      await shot.goto("file://" + path.join(UI_DIR, "splash-opening.html"));
      await shot.waitForFunction(() => window.RichSplash && window.RichSplash.state.card === true, null, { timeout: 60000 });
      await shot.evaluate(() => document.fonts.ready);
      await shot.evaluate(() => window.RichSplash.holdAt(window.RichSplash.state.heldAt));
      const geo = await shot.evaluate(() => {
        const q = (s) => {
          const n = document.querySelector("#splash " + s);
          if (!n) return null;
          const r = n.getBoundingClientRect();
          return r.width ? { x: r.x, y: r.y, w: r.width, h: r.height } : null;
        };
        const plinth = q(".splash-plinth");
        const above = (r, gap, h) => (r ? { x: r.x, y: r.y - gap - h, w: r.w, h } : null);
        const line = q(".splash-line"), company = q(".ro-company"), caption = q(".ro-caption"), figure = q(".ro-figure"), held = q(".ro-collector");
        return {
          line, company, caption, figure, held,
          // a bare strip of the mat beside each line: to the left of it, inside the plinth
          lineBare: line && { x: plinth.x + 18, y: line.y, w: Math.max(8, line.x - plinth.x - 30), h: line.h },
          companyBare: company && { x: plinth.x + 18, y: company.y, w: Math.max(8, company.x - plinth.x - 30), h: company.h },
          captionBare: caption && { x: plinth.x + 18, y: caption.y, w: Math.max(8, caption.x - plinth.x - 30), h: caption.h },
          figureBare: figure && { x: plinth.x + 18, y: figure.y, w: Math.max(8, figure.x - plinth.x - 30), h: figure.h },
          heldBare: held && above(held, 6, 10),
          sizes: {
            line: line && parseFloat(getComputedStyle(document.querySelector("#splash .splash-line")).fontSize),
            company: company && parseFloat(getComputedStyle(document.querySelector("#splash .ro-company")).fontSize),
            caption: caption && parseFloat(getComputedStyle(document.querySelector("#splash .ro-caption")).fontSize),
            figure: figure && parseFloat(getComputedStyle(document.querySelector("#splash .ro-figure")).fontSize),
            held: held && parseFloat(getComputedStyle(document.querySelector("#splash .ro-collector")).fontSize),
          },
        };
      });
      const img = decode(await shot.screenshot());
      await ctx.close();
      const said = [];
      for (const [what, box, bare, floor, minPx] of [
        ["the line under the mark", geo.line, geo.lineBare, 4.5, 16],
        ["his company's line", geo.company, geo.companyBare, 4.5, 16],
        ["the milestone's line", geo.caption, geo.captionBare, 4.5, 16],
        ["the number", geo.figure, geo.figureBare, 3, 24],
        ["the held line", geo.held, geo.heldBare, 4.5, 16],
      ]) {
        if (!box) continue;
        const key = { "the line under the mark": "line", "his company's line": "company", "the milestone's line": "caption", "the number": "figure", "the held line": "held" }[what];
        assert(geo.sizes[key] >= minPx, id + ": " + what + " is " + geo.sizes[key] + "px, under " + minPx + "px");
        const m = measure(img, box, bare);
        assert(m.n > 50, id + ": nothing was photographed for " + what);
        if (m.ratio < floor) under.push(id + ": " + what + " is " + m.ratio.toFixed(2) + ":1 on the glass, under " + floor + ":1");
        if (m.ratio < lowest.ratio) lowest = { ratio: m.ratio, at: id + " " + what };
        said.push(key + " " + m.ratio.toFixed(2));
      }
      out.push(id + ": " + said.join(", "));
    }
    // Every line is measured before any is judged, so one run names every line under its floor.
    assertEqual(under, [], "lines under their floor on the glass (all measured: " + out.join(" · ") + ")");
    return out.join(" · ") + " · lowest " + lowest.ratio.toFixed(2) + ":1 (" + lowest.at + ")";
  });

  await browser.close();
  process.exitCode = run.report() ? 1 : 0;
}

main().catch((e) => {
  console.error(e);
  process.exit(1);
});

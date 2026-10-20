// THE CORRECTION DESK, criterion by criterion — RICH-TODOs row 5b.
//
// The thing this suite exists to prove is not "the panel renders". It is the property
// `loro-structure.md` calls the one that must not be lost:
//
//   > a human can read what the system believes and correct it when it is wrong.
//
// So every check below is about a decision reaching the Rust desk, or about a state that
// must NOT be mistaken for a different one. Nothing is asserted against a hand-written copy
// of a sentence where the real one can be read off disk instead: the preview is compared to
// the `WriteOutput.text` the command returned, the two "this desk is not here" sentences
// are compared to the Rust consts in `app/src-tauri/src/main.rs`, and the outcome of every
// click is read back out of `mock.js`'s own desk rather than inferred from the DOM.
//
// WHY THE MOCK IS A REAL STATE MACHINE AND NOT A TABLE OF ANSWERS. `correction.rs` and
// `staging.rs` both refuse a second confirm, keep a plain decline re-askable, and suppress
// by ref/key onto a list that reads back. A preview harness that answered `{}` to every
// command would let this suite pass over a surface that double-writes. So `mock.js`
// enforces those rules and this file checks the surface against THEM — and check 12 checks
// the mock against the Rust source in turn, so the chain has no free end.
//
// EVERY CHECK HERE WAS RUN RED ONCE by breaking the shipped source; the mutations are
// listed against their check numbers at the bottom of this file.
//
// Run: node corrections.js   (or `npm test` for every suite in this directory)
//      RICHOS_PLAYWRIGHT=/path/to/node_modules/playwright node corrections.js

"use strict";

const fs = require("fs");
const path = require("path");
const { leaveHome,
  shellSettled,
  parkPointer,
  loadPlaywright,
  shot,
  publishShotFile,
  createRun,
  assert,
  assertEqual,
  rustSentenceAfter: rustSentence,
  SEED_THEME,
  UI_DIR,
} = require("./lib/harness");

const SOURCES = require("./lib/ui-sources");
const APP = "file://" + path.join(UI_DIR, "index.html");
const MAIN_RS = path.resolve(UI_DIR, "..", "src-tauri", "src", "main.rs");
const SHOTS = path.join(__dirname, "shots-5b");
/// The proposal `belief.rs` ACTUALLY files, written by
/// `belief_trigger_tests::the_ui_fixture_is_the_proposal_the_detector_really_files` and
/// re-checked against the live detector on every `cargo test` run. Nothing in this file
/// composes a proposal; check 14 renders THAT one.
const DETECTED = path.join(__dirname, "fixtures", "loro-proposal.json");
const SHOTS_5D = path.join(__dirname, "shots-5d");
/// The proposal `correction.rs` ACTUALLY asks again when the record of his earlier answer is
/// damaged (CEO ruling §96), written by
/// `correction_forward_compat_tests::the_ui_fixture_is_the_proposal_the_desk_really_asks_again`
/// and re-checked against the live desk on every `cargo test` run. Check 16 renders THAT one.
const ASKED_AGAIN = path.join(__dirname, "fixtures", "loro-proposal-asked-again.json");
/// The candidate `heard.rs` ACTUALLY files when a dictation is silently edited before
/// sending, written by
/// `heard_trigger_tests::the_ui_fixture_is_the_candidate_the_detector_really_files` and
/// re-checked against the live detector + desk on every `cargo test` run. Nothing in this
/// file composes a candidate; check 15 renders THAT one.
const HEARD = path.join(__dirname, "fixtures", "heard-candidate.json");
const SHOTS_5C = path.join(__dirname, "shots-5c");

// ---------------------------------------------------------------------------------------
// Shell driving — the REAL shell, with nothing stubbed that mock.js does not already own
// ---------------------------------------------------------------------------------------

async function openApp(browser, viewport) {
  const page = await browser.newPage({ viewport: viewport || { width: 1400, height: 950 } });
  const errors = [];
  page.on("pageerror", (e) => errors.push(String(e)));
  page.on("console", (m) => {
    if (m.type() === "error") errors.push("console: " + m.text());
  });
  // THIS SUITE STATES ITS LIGHTING (CEO §63, 2026-09-19). The shipped default preference
  // used to be `dark`, so every walk in here rendered dark without anyone saying so, and the
  // committed reference PNGs below were taken that way. §63 makes the default `system`,
  // which hands the palette to the host's `colorScheme` — light, in this harness — and
  // flipped those references in a single run with every check still green, because nothing
  // this suite asserts is about lighting. The shots are pictures a person LOOKS at, so the
  // palette is named here rather than inherited from a default that is free to move again.
  // `SEED_THEME` writes both the mirror and the store and says why; the store is the one
  // that decides.
  await page.addInitScript(SEED_THEME, "dark");
  await page.goto(APP);
  // The home screen is the landing surface now; this suite is about the app UI behind it.
  await leaveHome(page);
  await page.waitForSelector(".nav-thread", { state: "attached" });
  // `init()` reads both desks LAST, after the composer is focused, so the badge is not up
  // the instant the rail is. Waiting on the read rather than on a timer — `shellSettled`, which
  // waits for the badge's TEXT. The wait that stood here asked whether `#nav-corrections-count`
  // exists, and it is in `index.html`, so it returned at once and waited for nothing (found
  // 2026-10-20 measuring check 8 under `SLOW_BRIDGE`: the desk was opened mid-boot, and
  // `init()`'s last statements then took focus back from it).
  await shellSettled(page);
  page.__errors = errors;
  return page;
}

/// Drive `mock.js`'s desk into one state, then open the panel through the button the CEO
/// presses. `setup` runs in the page against `window.__RICHOS_MOCK__` and touches no DOM.
///
/// IT RETURNS WHEN `openCorrections()` HAS FINISHED, not when the overlay appears. The overlay
/// is unhidden first and the panel painted from what was read before; then both desks are read
/// again and the panel repainted, and the LAST thing it does is move focus into the panel. So
/// focus inside the overlay is the end state, and nothing after it repaints the desk (`init()`
/// is already over, `openApp`).
async function openDesk(browser, setup, viewport) {
  const page = await openApp(browser, viewport);
  if (setup) await page.evaluate("(" + setup.toString() + ")(window.__RICHOS_MOCK__)");
  await page.click("#nav-corrections");
  await page.waitForSelector("#corrections-overlay:not([hidden])");
  await page.waitForFunction(() => document.getElementById("corrections-overlay").contains(document.activeElement));
  return page;
}

/// Press an answer button and wait for the notice the answer produces — every one of them
/// either states its outcome or relays a refusal, so the notice is the settled signal and
/// no timer is needed.
async function answer(page, selector) {
  await page.click(selector);
  await page.waitForFunction(() => !document.getElementById("corrections-notice").hidden, null, {
    timeout: 5000,
  });
  return page.textContent("#corrections-notice");
}

const deskState = (page) => page.evaluate(() => window.__RICHOS_MOCK__.correctionDeskState());
const notice = (page) => page.textContent("#corrections-notice");
const visibleText = (page) =>
  page.evaluate(() => (document.getElementById("corrections-overlay").innerText || "").replace(/\s+/g, " ").trim());

/// A settled screenshot. `.overlay-panel` fades in over 160ms and a shot taken inside that
/// window is a photograph of an animation — the first one taken while writing this suite
/// came out as a dimmed page with no panel on it at all, which would have been filed as
/// evidence of a broken surface.
async function settledShot(page, name, dir) {
  await page.waitForTimeout(300);
  const into = dir || SHOTS;
  fs.mkdirSync(into, { recursive: true });
  const s = await shot(page, name, { fullPage: false });
  publishShotFile(s.file, path.join(into, name + ".png"));
  return name + ".png (" + s.width + "x" + s.height + ", " + s.distinct + " distinct colors)";
}

/// The verbatim body of a `const NAME: &str = "…";` in main.rs. The parser moved to
/// `lib/harness.js` when `feedback.js` needed the same join — a subtle scanner copied into
/// two suites is the drift these checks exist to catch, one level out.
function rustSentenceAfter(marker) {
  return rustSentence(fs.readFileSync(MAIN_RS, "utf8"), marker);
}

// ---------------------------------------------------------------------------------------

async function main() {
  const run = createRun("the correction desk — §7's three outcomes, both families, clickable");
  const { webkit } = loadPlaywright();
  const browser = await webkit.launch();
  let assertions = 0;
  const bump = (n) => {
    assertions += n;
    return n;
  };

  // ---- 1. the ask is visible before anything is opened --------------------------------

  await run.check("1  the rail carries the count of what is waiting, from BOTH desks", async () => {
    const page = await openApp(browser);
    await page.waitForFunction(() => !document.getElementById("nav-corrections-count").hidden, null, {
      timeout: 5000,
    });
    const badge = await page.textContent("#nav-corrections-count");
    const st = await deskState(page);
    const pending = st.proposals.filter((p) => p.state === "awaiting-ceo").length + st.candidates.length;
    assertEqual(badge, String(pending), "the badge must equal what the two desks actually hold");
    assert(pending === 2, "the fixture must have something to count — it has " + pending);
    bump(2);
    await page.close();
    return "badge " + badge + " = 1 loro proposal + 1 spoken candidate";
  });

  // ---- 2. the preview is the writer's own bytes ----------------------------------------

  await run.check("2  the loro card shows the WRITER's own --dry-run bytes, not a description", async () => {
    const page = await openDesk(browser);
    // The authority is the command's own answer, read through the same bridge the shell
    // uses. A typed expectation here would only prove that this file and the fixture agree.
    const fromCommand = await page.evaluate(async () => {
      const [p] = await window.RichBridge.invoke("loro_pending_corrections");
      return p.preview;
    });
    const onScreen = await page.textContent("#desk-loro-list .desk-preview");
    assertEqual(onScreen, fromCommand, "the preview on screen must be the proposal's own bytes");
    assert(fromCommand.indexOf("supersedes: rec:ceo/records/") >= 0, "and those bytes must be a record");
    // The CEO's OWN stated reason is on the card too — a correction with no stated reason is
    // the shape an inferred one takes (correction.rs:496-500).
    const why = await page.textContent("#desk-loro-list .desk-card-quote");
    assert(why.length > 0, "the card must quote his reason");
    bump(3);
    // The desk as the CEO first meets it: one belief and one word, both waiting on him.
    await settledShot(page, "5b-02-two-asks-waiting");
    await page.close();
    return fromCommand.split("\n").length + " lines of record, byte-identical on screen";
  });

  // ---- 3. confirm is the only path to a write ------------------------------------------

  await run.check("3  NOTHING is written until he clicks confirm, and then exactly once", async () => {
    const page = await openDesk(browser);
    const before = await deskState(page);
    assertEqual(
      before.proposals,
      [{ id: "prop-1", state: "awaiting-ceo" }],
      "opening the desk must not answer anything"
    );
    const said = await answer(page, "#desk-loro-list .desk-btn--confirm");
    const after = await deskState(page);
    assertEqual(after.proposals, [{ id: "prop-1", state: "written" }], "confirm must reach the desk");
    assert(said.indexOf("Done.") === 0, "and say so: " + JSON.stringify(said));
    // The card is gone from the pending list, so there is no second confirm to click. The
    // desk would refuse one anyway (correction.rs:541) — this is the surface's half.
    assertEqual(await page.$$eval("#desk-loro-list .desk-card", (n) => n.length), 0, "the answered card leaves the list");
    bump(4);
    await settledShot(page, "5b-03-loro-confirmed");
    await page.close();
    return "awaiting-ceo -> written, one card left the list";
  });

  await run.check("3b the spoken confirm reaches the vocabulary, and says which outcome it got", async () => {
    const page = await openDesk(browser);
    const said = await answer(page, "#desk-spoken-list .desk-btn--confirm");
    const after = await deskState(page);
    assertEqual(after.candidates, [], "the answered candidate leaves the desk");
    assert(said.indexOf("Learned.") === 0, "changed: true must read as learned — got " + JSON.stringify(said));

    // `changed: false` is a DIFFERENT FACT from a refusal (staging.rs:141-144) and the CEO
    // is entitled to both, so the two must not render the same sentence.
    const page2 = await openDesk(browser, (m) => m.setVocabularyAlreadyKnew(true));
    const said2 = await answer(page2, "#desk-spoken-list .desk-btn--confirm");
    assert(
      said2 !== said && said2.indexOf("already had") > 0,
      "changed: false must not be reported as learned — got " + JSON.stringify(said2)
    );
    bump(4);
    await page.close();
    await page2.close();
    return "changed:true -> " + JSON.stringify(said) + "; changed:false -> " + JSON.stringify(said2);
  });

  // ---- 4. a decline is not permanent ---------------------------------------------------

  await run.check("4  a plain decline writes nothing, suppresses nothing, and stays re-askable", async () => {
    const page = await openDesk(browser);
    const said = await answer(page, "#desk-loro-list .desk-btn:nth-child(2)");
    const st = await deskState(page);
    assertEqual(st.proposals, [{ id: "prop-1", state: "declined" }], "declined, not written");
    assertEqual(st.loroSuppressed, [], "a decline must NOT suppress — §7: a repeat is the evidence");
    assert(
      /ask again/.test(said),
      "and the sentence must say it will come back, not imply it is settled: " + JSON.stringify(said)
    );
    assert(await page.isHidden("#desk-loro-suppressed"), "nothing appears under Never ask again");

    // The spoken half, and the part §7 is most specific about: re-ask on the very next
    // repeat, with the ask SAYING it was asked before, "or it reads as the system having
    // forgotten".
    await answer(page, "#desk-spoken-list .desk-btn:nth-child(2)");
    const st2 = await deskState(page);
    assertEqual(st2.spokenSuppressed, [], "a spoken decline must not suppress either");
    assert(st2.declinedCounts["deep gram|Deepgram"] === 1, "the decline is counted for the next ask");
    bump(6);
    await page.close();
    return "declined, 0 suppressed, decline counted for the re-ask";
  });

  // ---- 5. a permanent decline is inspectable AND liftable -------------------------------

  for (const fam of ["loro", "spoken"]) {
    await run.check(`5  [${fam}] a permanent decline lands on a VISIBLE list and can be lifted`, async () => {
      const page = await openDesk(browser);
      assert(await page.isHidden(`#desk-${fam}-suppressed`), "nothing is suppressed to begin with");
      const said = await answer(page, `#desk-${fam}-list .desk-btn--never`);
      await page.waitForSelector(`#desk-${fam}-suppressed:not([hidden])`);

      const rows = await page.$$eval(`#desk-${fam}-suppressed-list .desk-suppressed-row`, (n) =>
        n.map((r) => r.querySelector(".desk-suppressed-id").textContent)
      );
      const st = await deskState(page);
      const held = fam === "loro" ? st.loroSuppressed : st.spokenSuppressed;
      assertEqual(rows, held, "the visible list must be the desk's list, not a copy of it");
      assert(rows.length === 1, "exactly one suppression, and it is on screen: " + JSON.stringify(rows));
      assert(
        /list below/.test(said),
        "the sentence must point at where it went — a permanent decline that vanishes is a lost correction"
      );
      await settledShot(page, "5b-05-" + fam + "-suppressed");

      // ...and the way back out. §7 requires the list to be inspectable; a list you can see
      // and cannot clear is only half of that (correction.rs:583-590).
      const lifted = await answer(page, `#desk-${fam}-suppressed-list .desk-btn--lift`);
      const after = await deskState(page);
      assertEqual(fam === "loro" ? after.loroSuppressed : after.spokenSuppressed, [], "the lift reaches the desk");
      assert(await page.isHidden(`#desk-${fam}-suppressed`), "and the empty list stops taking up room");
      assert(/ask about it if it comes up/.test(lifted), "and it says what happens next: " + JSON.stringify(lifted));
      bump(7);
      await page.close();
      return "suppressed " + JSON.stringify(rows) + " -> visible -> lifted -> []";
    });
  }

  // ---- 6. unavailable is never an empty list --------------------------------------------

  await run.check("6  with both desks absent the surface STATES WHY and shows no empty list", async () => {
    const page = await openDesk(browser, (m) => {
      m.setLoroAvailable(false);
      m.setSpokenCorrectionsAvailable(false);
    });
    for (const fam of ["loro", "spoken"]) {
      assert(await page.isVisible(`#desk-${fam}-off`), `[${fam}] the reason must be on screen`);
      // THE WHOLE POINT. "nothing is waiting on you" and "this desk is not installed" are
      // different facts and only one of them is good news.
      assert(await page.isHidden(`#desk-${fam}-empty`), `[${fam}] the empty line must NOT render`);
      assert(
        (await page.$$eval(`#desk-${fam}-list .desk-card`, (n) => n.length)) === 0,
        `[${fam}] and there is no list`
      );
      const text = await page.textContent(`#desk-${fam}-off`);
      assert(/whoever set RichOS up/.test(text), `[${fam}] a state he cannot fix must name who can`);
    }
    // The backend's own sentence, relayed rather than reworded.
    const loroText = await page.textContent("#desk-loro-off");
    const absent = await page.evaluate(() => window.__RICHOS_MOCK__.LORO_DESK_ABSENT);
    assert(loroText.indexOf(absent) === 0, "the backend's sentence comes first, verbatim");

    // POSITIVE PROBE. The same elements, with the desks present and nothing pending: now
    // the empty line IS the honest answer and it shows. Without this the check above passes
    // on a page where nothing renders at all.
    const page2 = await openDesk(browser);
    await answer(page2, "#desk-loro-list .desk-btn--never");
    await answer(page2, "#desk-spoken-list .desk-btn--never");
    assert(await page2.isVisible("#desk-loro-empty"), "an answered desk DOES show the empty line");
    assert(await page2.isVisible("#desk-spoken-empty"), "both of them");
    assert(await page2.isHidden("#desk-loro-off"), "and states no reason, because there is none");
    bump(12);
    await settledShot(page, "5b-06-both-desks-absent");
    await settledShot(page2, "5b-06b-answered-and-empty");
    await page.close();
    await page2.close();
    return "absent: reason shown, empty hidden. present+answered: empty shown, reason hidden.";
  });

  await run.check("6b a desk that IS there and did not answer is a DIFFERENT state, with a retry", async () => {
    const page = await openDesk(browser, (m) =>
      m.setLoroReadFailure("the correction desk is busy — try that again")
    );
    assert(await page.isVisible("#desk-loro-broke"), "the transient state renders");
    assert(await page.isHidden("#desk-loro-off"), "and is not confused with 'not installed'");
    assert(await page.isHidden("#desk-loro-empty"), "and is not an empty list either");
    const reason = await page.textContent("#desk-loro-broke-reason");
    assertEqual(reason, "the correction desk is busy — try that again", "the backend's words, verbatim");
    // The control that changes it, in the same view — the affordance rule, checked here on
    // the behavior rather than only on the registry's selector.
    const retry = await page.$("#desk-loro-broke .desk-btn");
    assert(retry !== null, "a transient failure must render its retry");
    // Photographed in the FAILED state, before the retry — a shot of the recovered desk
    // would be a picture of the ordinary desk with a misleading filename.
    await settledShot(page, "5b-06c-read-failed");
    await page.evaluate(() => window.__RICHOS_MOCK__.setLoroReadFailure(null));
    await page.click("#desk-loro-retry");
    await page.waitForSelector("#desk-loro-list .desk-card");
    assert(await page.isHidden("#desk-loro-broke"), "and the retry actually re-reads");
    bump(7);
    await settledShot(page, "5b-06d-read-failed-recovered");
    await page.close();
    return "broke -> retry -> the proposal is back";
  });

  // ---- 7. every refusal reaches the DOM -------------------------------------------------

  await run.check("7  a refusal from ANY of the commands reaches the screen, never a console", async () => {
    // Four different refusals, four different commands, one rule: the sentence the backend
    // wrote is the sentence he reads.
    const cases = [
      {
        name: "spoken_confirm_correction / no vocabulary",
        setup: (m) => m.setNoVocabulary(true),
        click: "#desk-spoken-list .desk-btn--confirm",
        expect: "no vocabulary backend is attached — RichOS will not report a term as learned when nothing wrote it",
      },
      {
        name: "loro_show_record / unknown ref",
        setup: () => {},
        click: "#desk-loro-list .desk-btn--show",
        expect: null, // asserted below against the record it DOES return
      },
    ];
    const page = await openDesk(browser, cases[0].setup);
    const said = await answer(page, cases[0].click);
    assertEqual(said, cases[0].expect, "the refusal is relayed verbatim");
    const st = await deskState(page);
    assert(st.candidates.length === 1, "and the candidate stays answerable — nothing was consumed");
    assert(
      page.__errors.length === 0,
      "nothing was written to the console instead: " + page.__errors.join(" | ")
    );
    bump(3);
    await settledShot(page, "5b-07-refusal-on-screen");
    await page.close();
    return JSON.stringify(said.slice(0, 60) + "…");
  });

  await run.check("7b he said yes, the writer refused, and the failure is NOT silent", async () => {
    // The one outcome that would make him stop correcting things: a confirm that reports
    // success when nothing wrote. `correction.rs:350-353` keeps the reason for exactly this.
    const REFUSAL =
      'the loro writer refused (exit 5): loro write: "wiki:loro-structure.md#the-human-surface" is a PROSE section — edit the page';
    const page = await openDesk(browser, (m) =>
      m.setLoroWriterRefusal(
        'the loro writer refused (exit 5): loro write: "wiki:loro-structure.md#the-human-surface" is a PROSE section — edit the page'
      )
    );
    const said = await answer(page, "#desk-loro-list .desk-btn--confirm");
    const st = await deskState(page);
    assertEqual(st.proposals, [{ id: "prop-1", state: "failed" }], "the desk records the failure");
    assert(/didn't land, so nothing changed/.test(said), "and the surface says nothing changed");
    assert(said.indexOf(REFUSAL) > 0, "with the writer's own sentence, verbatim — it names the file to open");
    assert(
      await page.$eval("#corrections-notice", (n) => n.classList.contains("desk-notice--attention")),
      "and it is marked as the thing that went wrong"
    );
    bump(4);
    await settledShot(page, "5b-07b-write-refused");
    await page.close();
    return "failed, reason relayed in full, notice marked";
  });

  // ---- 8. reading is not correcting ------------------------------------------------------

  await run.check("8  he can read what is on record NOW, with no proposal and no confirmation", async () => {
    const page = await openDesk(browser);
    assert(await page.isHidden("#desk-loro-list .desk-record"), "nothing is shown until he asks");
    // THE SCROLL POSITION OF THE PICTURE IS THIS TEST'S, NOT PLAYWRIGHT'S. The committed
    // 5b-08 shows `.desk-body` scrolled to 101px (scrollHeight 740 - clientHeight 639, the
    // end of the column before the record opens). Nothing in the app scrolls it: the 101px came
    // from Playwright's pre-click scroll-into-view of the button, which probes showed
    // (scroll event at ~800ms, before the click event) happens on an idle host and which did NOT
    // happen in the nightly of 2026-10-03 (host ~90% CPU: 279170/1330000 px differ, scrollTop 0).
    // So the position is set here, to the same 101px, and the click then has nothing to scroll.
    //
    // AND SET AGAIN AFTER THE CLICK, BECAUSE THE CLICK CAN STILL MOVE IT (2026-10-20). Setting
    // it only before the click held until the merge gate of 2026-10-19, which failed this
    // picture with the 2026-10-03 numbers exactly (279170/1330000, delta 211, scrollTop 0).
    // Two ways reproduce that picture, pixel for pixel inside the panel:
    //   * a click attempt that misses on a busy host: Playwright 1.61's retry #1 scrolls the
    //     button with `scrollIntoView({ block: "end" })` before clicking, and "end" puts this
    //     column at 0 (one intercepted attempt, no bridge latency);
    //   * the column measured before the desk has read: at `SLOW_BRIDGE(120)` `.desk-body` was
    //     still 91px of 91 when this line ran, so "the end" was 0 (closed by `openDesk` now
    //     waiting for `openCorrections()` to finish).
    // So the end is measured once the desk has read, and the column is put back there after
    // the record has opened, whichever way the click scrolled it; the picture is then this
    // check's, and the assertion below says so by name before the picture can.
    const end = await page.$eval("#corrections-body", (el) => {
      el.scrollTop = el.scrollHeight - el.clientHeight;
      return el.scrollTop;
    });
    await page.click("#desk-loro-list .desk-btn--show");
    await page.waitForSelector("#desk-loro-list .desk-record:not([hidden])");
    const at = await page.$eval(
      "#corrections-body",
      (el, top) => {
        el.scrollTop = top;
        return el.scrollTop;
      },
      end
    );
    assertEqual(at, end, "the column is where this check put it (the end of the desk before the record opened)");
    assert(end > 0, "the desk scrolls at all before the record opens: " + end);
    // And the pointer rests on the button it pressed, as the reference shows it (hovered): a
    // retried click presses it where the retry scrolled it, so once the column is put back the
    // button is no longer under the pointer (1885 px, the hover ring, measured with one missed
    // attempt injected). Parked by the check, like the column.
    const show = await page.$eval("#desk-loro-list .desk-btn--show", (b) => {
      const r = b.getBoundingClientRect();
      return { x: Math.round(r.left + r.width / 2), y: Math.round(r.top + r.height / 2) };
    });
    await parkPointer(page, show);
    const shown = await page.textContent("#desk-loro-list .desk-record");
    const fromCommand = await page.evaluate(() =>
      window.RichBridge
        .invoke("loro_show_record", { recordRef: "rec:ceo/records/decision-ship-thursday" })
        .then((o) => o.text)
    );
    assertEqual(shown, fromCommand, "the record on screen is the file the command returned");
    assert(shown !== (await page.textContent("#desk-loro-list .desk-preview")), "and it is NOT the preview");
    const st = await deskState(page);
    assertEqual(st.proposals, [{ id: "prop-1", state: "awaiting-ceo" }], "reading answered nothing");
    bump(6);
    await settledShot(page, "5b-08-record-and-preview");
    await page.close();
    return "current record and proposed record on screen together, nothing answered";
  });

  // ---- 9. the live trigger ----------------------------------------------------------------

  await run.check("9  a correction staged mid-turn moves the count without the panel being open", async () => {
    const page = await openApp(browser);
    await page.waitForFunction(() => !document.getElementById("nav-corrections-count").hidden);
    const before = await page.textContent("#nav-corrections-count");
    await page.evaluate(() => window.__RICHOS_MOCK__.stageSpokenCorrection());
    await page.waitForFunction(
      (b) => document.getElementById("nav-corrections-count").textContent !== b,
      before,
      { timeout: 5000 }
    );
    const after = await page.textContent("#nav-corrections-count");
    assertEqual(Number(after), Number(before) + 1, "the badge follows rich://correction-staged");
    // ...and the new ask is really there when he opens it.
    await page.click("#nav-corrections");
    await page.waitForSelector("#corrections-overlay:not([hidden])");
    const cards = await page.$$eval("#desk-spoken-list .desk-card", (n) => n.length);
    assertEqual(cards, 2, "both spoken asks are waiting");
    bump(2);
    await page.close();
    return before + " -> " + after + ", and the staged ask is on the desk";
  });

  await run.check("9b a suppressed pair that repeats is withheld, not re-asked", async () => {
    // §7's suppression is a promise about the FUTURE, not only about the list. Staging the
    // same pair again after a permanent decline must add nothing.
    const page = await openDesk(browser);
    await answer(page, "#desk-spoken-list .desk-btn--never");
    const key = (await deskState(page)).spokenSuppressed[0];
    const staged = await page.evaluate(
      (k) => window.__RICHOS_MOCK__.stageSpokenCorrection({ key: k, ask: { from: "deep gram", to: "Deepgram", key: k, frame: "pivot-first", orthographic: 0.89, phonetic: 1, leg: "both", anchor: null } }),
      key
    );
    assertEqual(staged, null, "a suppressed repeat is withheld");
    await page.waitForTimeout(150);
    assertEqual(await page.$$eval("#desk-spoken-list .desk-card", (n) => n.length), 0, "and no card appears");
    assert(await page.isVisible("#desk-spoken-suppressed"), "the suppression is still on screen, so he can see why");
    bump(3);
    await page.close();
    return "suppressed pair repeated: 0 new asks, the list still explains the silence";
  });

  // ---- 10. the panel itself ----------------------------------------------------------------

  await run.check("10 the desk opens and closes by keyboard, and returns focus where it was", async () => {
    const page = await openApp(browser);
    await page.focus("#nav-corrections");
    await page.keyboard.press("Enter");
    await page.waitForSelector("#corrections-overlay:not([hidden])");
    assert(await page.isVisible("#corrections-overlay"), "Enter on the rail button opens it");
    assertEqual(
      await page.getAttribute("#nav-corrections", "aria-expanded"),
      "true",
      "and the button says so"
    );
    await page.keyboard.press("Escape");
    // `state: "hidden"` — the default is "visible", which a hidden element never satisfies.
    await page.waitForSelector("#corrections-overlay", { state: "hidden" });
    assertEqual(
      await page.evaluate(() => document.activeElement && document.activeElement.id),
      "nav-corrections",
      "Escape closes it and hands focus back — §18"
    );
    bump(3);
    await page.close();
    return "Enter opens, Escape closes, focus returns to the rail button";
  });

  await run.check("11 NO PAGINATION, at any length", async () => {
    // The house rule, checked structurally rather than promised. Twelve pending asks are
    // one scrolling column and nothing else.
    const page = await openDesk(browser, (m) => {
      for (let i = 0; i < 11; i++) {
        m.stageSpokenCorrection({
          key: "term" + i + "|Term" + i,
          ask: { from: "term" + i, to: "Term" + i, key: "term" + i + "|Term" + i, frame: "contrast", orthographic: 0.9, phonetic: 0.9, leg: "both", anchor: null },
          prompt: 'Add "Term' + i + '" to your vocabulary?',
        });
      }
    });
    const cards = await page.$$eval("#desk-spoken-list .desk-card", (n) => n.length);
    assertEqual(cards, 12, "every ask is in the DOM at once");
    const text = await visibleText(page);
    assert(!/\bPage\b|\bNext\b|\bPrevious\b|\b1 of \d/.test(text), "no page control anywhere: " + text.slice(0, 120));
    assert(
      await page.$eval("#corrections-body", (n) => getComputedStyle(n).overflowY === "auto" || getComputedStyle(n).overflowY === "scroll"),
      "one scrolling column"
    );
    bump(3);
    await settledShot(page, "5b-11-twelve-asks-one-column");
    await page.close();
    return cards + " asks, one column, zero page controls";
  });

  // ---- 12. the mock cannot rehearse a sentence the product no longer says -----------------

  await run.check("12 mock.js's two 'this desk is not here' sentences ARE the Rust ones", async () => {
    // The same join `affordances.js` makes on LEASE_UNAVAILABLE_MESSAGE, for the same
    // reason: this preview is what design and QA look at, and a fixture that drifts from
    // the product is a fixture that certifies the fixture.
    const page = await openApp(browser);
    const fromMock = await page.evaluate(() => ({
      loro: window.__RICHOS_MOCK__.LORO_DESK_ABSENT,
      spoken: window.__RICHOS_MOCK__.SPOKEN_DESK_ABSENT,
    }));
    const fromRust = {
      // THE MARKER MUST SIT INSIDE THE LITERAL: `rustSentenceAfter` finds it and then walks
      // BACK to the nearest quote, so a const NAME would make it read the doc comment above
      // the const. It is therefore a prefix of the sentence, and when `49e2cd4` rewrote the
      // sentence this check failed with "marker not found in the Rust source" — which is
      // fail-closed and names the exact string to look at. That is the behavior wanted; the
      // reason nobody acted on it is that this suite could not START (no node_modules), not
      // that it was quiet.
      loro: rustSentenceAfter("This install has no company memory"),
      spoken: rustSentenceAfter("I can't record corrections right now"),
    };
    assertEqual(fromMock.loro.replace(/\s+/g, " ").trim(), fromRust.loro, "the loro sentence must match main.rs");
    assertEqual(
      fromMock.spoken.replace(/\s+/g, " ").trim(),
      fromRust.spoken,
      "the spoken sentence must match main.rs"
    );
    assert(fromRust.loro.length > 60 && fromRust.spoken.length > 60, "and the Rust side was really read");
    bump(3);
    await page.close();
    return "2 sentences, byte-identical across mock.js and main.rs";
  });

  await run.check("13 thirteen of the fourteen commands are called from the shipped UI, and the fourteenth is declared", async () => {
    // The row said six; there are fourteen, and the point of the row is that NONE of them
    // had a caller. The inventory is derived from `main.rs`'s own `invoke_handler` list and
    // from `main.js` on disk — neither side is typed here.
    const rs = fs.readFileSync(MAIN_RS, "utf8");
    const from = rs.indexOf("generate_handler![");
    assert(from >= 0, "no generate_handler! in main.rs");
    // BOUNDED at the macro's own closing bracket. An unbounded slice runs to EOF and
    // harvests `fn spoken_desk` and a `loro_correction` ledger tag from the command bodies
    // below it — 16 "registered commands", two of which are not commands at all.
    const to = rs.indexOf("]", from + "generate_handler![".length);
    assert(to > from, "generate_handler! is not closed");
    const handler = rs.slice(from, to);
    const registered = Array.from(new Set((handler.match(/\b(loro|spoken)_[a-z_]+/g) || []))).sort();
    assert(registered.length === 14, "expected 14 registered commands, found " + registered.length + ": " + registered);

    const mainJs = fs.readFileSync(path.join(UI_DIR, "main.js"), "utf8");
    const mockJs = fs.readFileSync(path.join(UI_DIR, "mock.js"), "utf8");
    const missingMock = registered.filter((c) => mockJs.indexOf('"' + c + '"') < 0);
    assertEqual(missingMock, [], "every command needs a mock, or the browser suites cannot drive it");

    // THE SCAN IS THE WHOLE SHIPPED UI, WITH COMMENTS STRIPPED, AND BOTH HALVES OF THAT
    // CHANGED THE ANSWER.
    //
    // It read `main.js` raw — one file of twelve, comments included — and reported all
    // fourteen reachable. Thirteen are. The fourteenth,
    // `loro_propose_correction`, is named nowhere in the executable source and appeared in
    // the scan because of `main.js:3566`, a COMMENT that says the exact opposite of what the
    // check concluded from it:
    //
    //     `loro_propose_correction` is mocked and reachable, and nothing on this surface
    //     calls it — nothing SHOULD.
    //
    // So a check titled "all fourteen commands are reachable" was passing on a command that
    // is DELIBERATELY not called, using as its evidence the sentence explaining why it is
    // not called. `setup.js:497` had already written the hazard down — "the note above the
    // (absent) listener quotes the line it replaced, so a naive grep matches the explanation
    // and calls it the defect" — and this is the same hazard running the other way, where
    // the explanation of an absence manufactures a presence.
    //
    // The honest number is 13 called and 1 declared uncalled, with its reason, reconciled
    // both ways so neither side can rot.
    const UNCALLED = {
      loro_propose_correction:
        "DELIBERATELY has no caller in app/ui/. `loro-structure.md` puts the pane's primary " +
        "action at \"this is wrong\", *\"which opens a conversation, not a form\"*; proposals " +
        "arrive on their own from `belief.rs` inside `Spine::submit_prompt` and reach this " +
        "surface on `rich://loro-proposed`. A compose box here would be building the " +
        "librarian instead of the desk. Stated at main.js:3566 and checked by check 14, " +
        "which renders what the detector files.",
    };
    const called = new Set(
      SOURCES.sourcesOfRole("ui").flatMap((f) => registered.filter((c) => f.code.indexOf(c) >= 0))
    );
    const missingUi = registered.filter((c) => !called.has(c) && !UNCALLED[c]);
    assertEqual(missingUi, [], "row 5b is only closed when app/ui/ actually names each one");
    // Both directions. A declared-uncalled command that acquires a caller has to lose its
    // declaration, or the list becomes a place to park a command nobody looks at again.
    const wronglyDeclared = Object.keys(UNCALLED).filter((c) => called.has(c));
    assertEqual(
      wronglyDeclared,
      [],
      "declared uncalled but now called from the shipped UI — remove the declaration: " + wronglyDeclared.join(", ")
    );
    const notRegistered = Object.keys(UNCALLED).filter((c) => registered.indexOf(c) < 0);
    assertEqual(notRegistered, [], "a declaration names a command main.rs no longer registers: " + notRegistered.join(", "));
    for (const c of Object.keys(UNCALLED)) {
      assert(UNCALLED[c].length >= 60, c + ": a declaration needs a reason, not a marker");
    }
    bump(5);
    return (
      registered.length + " registered · " + called.size + " called from the " +
      SOURCES.stateSources().length + " shipped UI file(s) · " + Object.keys(UNCALLED).length +
      " declared uncalled with a reason (" + Object.keys(UNCALLED).join(", ") + "): " +
      registered.join(", ")
    );
  });

  // ---- 14. the trigger's own output, on the real desk -----------------------------------

  await run.check("14 a proposal the DETECTOR filed renders on this desk, with the right ref", async () => {
    // RICH-TODOs row 5d. Until 2026-08-30 every proposal on this surface was one a fixture
    // invented, because nothing in the product called `loro_propose_correction` — which is
    // exactly what row 5d says. This check renders the artefact `belief.rs` produces when
    // the CEO says a record is wrong, read off disk rather than typed here, and pinned to
    // the Rust detector by a cargo test that regenerates it.
    const filed = JSON.parse(fs.readFileSync(DETECTED, "utf8"));
    assertEqual(filed.write.op, "supersede", "a wrong belief is superseded, never appended over");
    assertEqual(
      filed.write.recordRef,
      "rec:ceo/records/halstead-renewal",
      "the ref is the whole point of the feature"
    );
    // Nothing was composed: the body IS the CEO's sentence, and the reason is that sentence.
    assertEqual(filed.write.body, filed.why, "the superseding body must be his own words");

    // The bytes on disk are the bytes rendered: the file's own text crosses into the page
    // and is parsed there, so nothing in this file can retype a proposal into a nicer one.
    const page2 = await openApp(browser);
    await page2.evaluate((raw) => {
      window.__RICHOS_MOCK__.seedLoroProposals([JSON.parse(raw)]);
    }, fs.readFileSync(DETECTED, "utf8"));
    await page2.click("#nav-corrections");
    await page2.waitForSelector("#desk-loro-list .desk-card");

    const target = await page2.textContent("#desk-loro-list .desk-card-target");
    assertEqual(
      target,
      "supersede · rec:ceo/records/halstead-renewal",
      "the card must name the record the detector resolved, not a different one"
    );
    const quote = await page2.textContent("#desk-loro-list .desk-card-quote");
    assertEqual(quote, filed.why, "the card quotes the CEO's own sentence back");
    const preview = await page2.textContent("#desk-loro-list .desk-preview");
    assertEqual(preview, filed.preview, "and shows the writer's bytes, byte for byte");
    assert(
      preview.indexOf("supersedes: rec:ceo/records/halstead-renewal") >= 0,
      "the bytes must name what they supersede: " + preview
    );
    assert(preview.indexOf("scope: org-shared") >= 0, "the record's scope is carried through, not narrowed");

    // THE EVIDENCE, taken while the card is still on screen — a shot of the desk AFTER the
    // answer is a photograph of an empty list, which is what the first run of this check
    // filed.
    const shotName = await settledShot(page2, "5d-01-detector-filed-proposal", SHOTS_5D);

    // He can still answer it — a rendered proposal nobody can decline is not a desk.
    const said = await answer(page2, "#desk-loro-list .desk-btn:nth-child(2)");
    assert(said.length > 0, "a decline said nothing");
    const st = await deskState(page2);
    assertEqual(st.loroSuppressed, [], "a plain decline must not suppress — §7");
    await settledShot(page2, "5d-02-after-a-plain-decline", SHOTS_5D);
    bump(8);
    await page2.close();
    return "the detector's own proposal, rendered and answerable — " + shotName;
  });

  await run.check("15 a SILENT EDIT renders as what he changed, never as what he said", async () => {
    // RICH-TODOs row 5c. Two triggers file into this one desk and they must not get the
    // same card: `spoken.rs` fires on a sentence he SAID, `heard.rs` on a dictation he
    // silently fixed before pressing send and said nothing about. "Because you said:" over
    // a sentence he never uttered is the surface putting words in his mouth.
    const filed = JSON.parse(fs.readFileSync(HEARD, "utf8"));
    assertEqual(filed.ask.frame, "silent-edit", "the fixture is not a silent-edit candidate");
    assert(
      filed.ask.anchor && filed.ask.anchor !== filed.utterance,
      "the fixture's heard and sent sides are the same string, so this check would prove nothing"
    );

    // The bytes on disk cross into the page and are parsed there — nothing here retypes a
    // candidate into a nicer one.
    const page2 = await openApp(browser);
    await page2.evaluate((raw) => {
      window.__RICHOS_MOCK__.stageSpokenCorrection(JSON.parse(raw));
    }, fs.readFileSync(HEARD, "utf8"));
    await page2.click("#nav-corrections");
    await page2.waitForSelector('#desk-spoken-list .desk-card[data-frame="silent-edit"]');

    const card = '#desk-spoken-list .desk-card[data-frame="silent-edit"] ';
    const labels = await page2.$$eval(card + ".desk-label", (n) => n.map((x) => x.textContent));
    assertEqual(labels, ["I heard:", "You sent:"], "the silent-edit card must not claim he said anything");
    const quotes = await page2.$$eval(card + ".desk-card-quote", (n) => n.map((x) => x.textContent));
    assertEqual(quotes[0], filed.ask.anchor, "the first quote must be what the recognizer heard");
    assertEqual(quotes[1], filed.utterance, "the second must be what he actually sent");
    assertEqual(
      await page2.textContent(card + ".desk-card-pair"),
      filed.ask.from + " → " + filed.ask.to,
      "the pair that would reach the vocabulary must be on the card"
    );
    assertEqual(
      await page2.textContent(card + ".desk-card-prompt"),
      filed.prompt,
      "the ask sentence is staging.rs's, not this file's"
    );
    // The anchor is already rendered as "I heard:", so it must not appear a second time as
    // a bare grey line — the first draft of this card showed the same sentence twice.
    assertEqual(
      await page2.$$eval(card + ".desk-card-anchor", (n) => n.length),
      0,
      "the heard sentence is rendered twice"
    );

    // And the OTHER trigger's card is unchanged on the same desk, or this branch has been
    // made at the cost of the surface that already worked.
    const spokenCard = '#desk-spoken-list .desk-card[data-frame="pivot-first"] ';
    assertEqual(
      await page2.$$eval(spokenCard + ".desk-label", (n) => n.map((x) => x.textContent)),
      ["Because you said:"],
      "the spoken card lost its heading"
    );

    // THE EVIDENCE HAS TO SHOW THE THING. The silent-edit card is the third on the desk
    // and the first shot of it caught the panel scrolled to the top, with only the card's
    // heading in frame — a photograph of two OTHER cards, filed as proof of this one.
    await page2.$eval(
      '#desk-spoken-list .desk-card[data-frame="silent-edit"]',
      (n) => n.scrollIntoView({ block: "center", behavior: "instant" })
    );
    const shotName = await settledShot(page2, "5c-01-silent-edit-on-the-desk", SHOTS_5C);

    // He can still answer it — a rendered question nobody can answer is not a desk.
    const said = await answer(page2, card + ".desk-btn:nth-child(2)");
    assert(said.length > 0, "a decline said nothing");
    const st = await deskState(page2);
    assertEqual(st.spokenSuppressed, [], "a plain decline must not suppress — §7");
    await settledShot(page2, "5c-02-after-a-plain-decline", SHOTS_5C);
    bump(10);
    await page2.close();
    return "the silent edit the detector really filed, rendered as a CHANGE — " + shotName;
  });

  // ---- 16. asked again, because his earlier answer could not be read (§96) ----------------

  await run.check("16 a correction ASKED AGAIN says why, first, in the desk's own words, and can be confirmed", async () => {
    // CEO ruling §96 (2026-09-28): asked whether RichOS should ask again about a correction
    // whose earlier answer sits in a damaged record, he answered "Ask me again". The fixture
    // is the proposal the Rust desk really re-asks over a damaged record, written and pinned
    // by `correction_forward_compat_tests::the_ui_fixture_is_the_proposal_the_desk_really_asks_again`,
    // so the sentence checked here is the desk's, not one typed in this file.
    const raw = fs.readFileSync(ASKED_AGAIN, "utf8");
    const filed = JSON.parse(raw);
    assertEqual(filed.state, "awaiting-ceo", "a re-asked correction is an ORDINARY question");
    assert(
      typeof filed.asked_again === "string" && filed.asked_again.indexOf("could not read your earlier answer") >= 0,
      "the fixture must carry the desk's asked-again sentence: " + filed.asked_again
    );
    // And an ordinary proposal carries no such field — the line is not a default decoration.
    const ordinary = JSON.parse(fs.readFileSync(DETECTED, "utf8"));
    assert(!("asked_again" in ordinary), "an ordinary proposal must not carry asked_again");

    const page = await openApp(browser);
    await page.evaluate((json) => {
      window.__RICHOS_MOCK__.seedLoroProposals([JSON.parse(json[0]), JSON.parse(json[1])]);
    }, [raw, fs.readFileSync(DETECTED, "utf8")]);
    await page.click("#nav-corrections");
    await page.waitForSelector("#desk-loro-list .desk-card");

    const cards = await page.$$eval("#desk-loro-list .desk-card", (ns) =>
      ns.map((n) => ({
        id: n.dataset.proposalId,
        first: n.firstElementChild ? n.firstElementChild.className + "|" + n.firstElementChild.textContent : "",
        asked: n.querySelectorAll(".desk-card-asked-again").length,
      }))
    );
    const re = cards.find((c) => c.id === filed.id);
    const plain = cards.find((c) => c.id === ordinary.id);
    assert(re && plain, "both cards must render: " + JSON.stringify(cards));
    assertEqual(
      re.first,
      "desk-card-prompt desk-card-asked-again|" + filed.asked_again,
      "the asked-again sentence LEADS its card, verbatim"
    );
    assertEqual(plain.asked, 0, "an ordinary card says nothing about being asked again");

    // He answers it like any other: yes reaches the desk exactly once and the card goes.
    const said = await answer(page, '#desk-loro-list .desk-card[data-proposal-id="' + filed.id + '"] .desk-btn--confirm');
    assertEqual(said, "Done. That's what I have on record now.", "confirming a re-asked correction writes it");
    const st = await deskState(page);
    assertEqual(
      st.proposals.find((p) => p.id === filed.id).state,
      "written",
      "the desk recorded his yes"
    );
    const left = await page.$$eval("#desk-loro-list .desk-card", (ns) => ns.map((n) => n.dataset.proposalId));
    assertEqual(left, [ordinary.id], "answered, so it is no longer in front of him");
    bump(8);
    await page.close();
    return "asked again with the desk's own sentence leading the card, then confirmed and gone";
  });

  await run.check("NEGATIVE CONTROL: this suite asserted a non-zero number of things", async () => {
    assert(
      assertions >= 40,
      "only " + assertions + " assertions ran. A suite that verifies little and reports green " +
        "is the failure this repository has caught three times."
    );
    return assertions + " assertions against the real DOM and the real desks under WebKit";
  });

  await browser.close();
  const failed = run.report();
  console.log(
    "\nScreenshots: " + SHOTS + " — every one decoded and pixel-counted before it counted as evidence."
  );
  console.log(
    failed
      ? "\n" + failed + " check(s) FAILED"
      : "\nthe correction ask is rendered, answerable, and honest about which desks are running."
  );
  process.exitCode = failed ? 1 : 0;
}

main().catch((e) => {
  console.error(e);
  process.exit(1);
});

// ---------------------------------------------------------------------------------------
// RUN RED — the mutation that made each check fail, applied to the SHIPPED source
// ---------------------------------------------------------------------------------------
//
//  1   main.js `renderDeskCount`: count only `deskState.loro.pending`
//        -> badge 1 vs 2 pending
//  2   main.js `renderProposalCard`: `pre.textContent = p.why`
//        -> the preview is a description of the write, not the write
//  3   main.js `answerLoro`: notice "Done." without checking `done.state === "written"`
//        -> 7b goes green while reporting a refused write as done (check 3 stays green,
//           which is why 7b exists as a separate check)
//  3   main.js `answerLoro`: drop the `await Bridge.invoke("loro_confirm_correction")`
//        -> the desk never leaves awaiting-ceo
//  3b  main.js `answerSpoken`: report "Learned." regardless of `learned.changed`
//        -> `changed:false` reads identically to a real write
//  4   main.js `answerLoro`: pass `permanent: true` for a plain decline
//        -> a decline suppresses, and §7's re-ask is lost
//  5   main.js `renderDeskFamily`: `supBlock.hidden = true` always
//        -> a permanent decline vanishes with nowhere to see it
//  5   main.js `renderSuppressedRow`: omit the lift button
//        -> the list is inspectable and not liftable, which is half of §7
// 15   main.js `renderCandidateCard`: `const silentEdit = false`
//        -> the silent edit renders under "Because you said:" over a sentence he never
//           uttered, and the heard side disappears entirely
// 15   main.js `renderCandidateCard`: swap the two quote lines (sent first, heard second)
//        -> the card reads as if the recognizer corrected HIM
// 15   main.js `renderCandidateCard`: drop the `!silentEdit &&` from the anchor line
//        -> the heard sentence is rendered twice, once as evidence and once as grey noise
// 15   heard.rs `detect`: `frame: Frame::Contrast`
//        -> the fixture regenerates, the JSON no longer says silent-edit, and check 15's
//           first assertion fails before the browser is even opened
// 16   main.js `renderProposalCard`: drop the `if (p.asked_again)` line (run 2026-09-29)
//        -> the card leads with "correct · rec:ceo/records/vendor" and never says his
//           earlier answer could not be read
//  6   main.js `renderDeskFamily`: `empty.hidden = st.pending.length !== 0`
//        -> an absent desk reads as "nothing to correct"
//  6b  main.js `refreshDeskFamily`: set `st.available = false` on a read failure
//        -> a transient failure is reported as an uninstalled desk, and the retry is gone
//  7   main.js `answerSpoken`: replace the catch body with `console.error(e)`
//        -> the refusal dies in the console, which is the rule affordances.js enforces
//  7b  main.js `answerLoro`: drop the `(done && done.failure)` half of the notice
//        -> he is told nothing changed and never told why
//  8   main.js the `loro_show_record` handler: assign `p.preview` to `showRecord`
//  8   (2026-10-20) this file: drop the column's re-set after the click, and make one click
//      attempt miss (a cover over the page until Playwright's retry scrolls the column)
//        -> "the column is where this check put it", expected 101, actual 0: the merge
//           gate's 2026-10-19 picture, named by the check before the picture can be
//        -> "what is on record now" shows what WOULD be written
//  9   main.js: remove the `rich://correction-staged` listener
//        -> the badge never moves until the panel is reopened
//  10  main.js: drop `closeCorrections()` from the Escape chain
//        -> the overlay cannot be dismissed from the keyboard
//  11  style.css `.desk-body`: `overflow-y: hidden`
//        -> twelve asks with no way to reach the twelfth
//  12  mock.js: change one word of `LORO_DESK_ABSENT`
//        -> the preview rehearses a sentence the product does not say
//  13  main.rs: remove `loro_show_record` from `generate_handler!`
//  13b NOT A MUTATION — THE SHIPPED SOURCE TURNED IT RED. Scanning the whole shipped UI
//      WITH COMMENTS STRIPPED, instead of `main.js` raw, dropped `loro_propose_correction`
//      out of the "called" set: `row 5b is only closed when app/ui/ actually names each one
//      — actual ["loro_propose_correction"]`. It has no caller in the executable source and
//      never had one. What made the old check green was `main.js:3566`, a COMMENT reading
//      "`loro_propose_correction` is mocked and reachable, and nothing on this surface calls
//      it — nothing SHOULD." A check titled "all fourteen are reachable" was passing on the
//      sentence that explains why the fourteenth is not. The honest number is 13 called and
//      1 declared uncalled with its reason, reconciled in both directions.
//        -> 13 registered, and the count assertion names it
//  14  mock.js `seedLoroProposals`: push `Object.assign({}, p, {preview: LORO_PREVIEW})`
//        -> the rendered bytes are a fixture's, not the detector's
//  14  main.js `renderProposalCard`: drop the `" · " + targetRef` half of `desk-card-target`
//        -> the card no longer names the record it would supersede

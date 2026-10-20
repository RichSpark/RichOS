// THE FEEDBACK CHANNEL, criterion by criterion — RICH-TODOs row 5.
//
// The row read as if nothing existed. At `aa364ed` the module was COMPLETE — the CEO's
// wording in constants, the four keys, `Rating::invites_report`, the versioned taxonomy, the
// one-file store, the disclosure, and four tests asserting there is no way off this machine
// — and `grep -rn feedback app/ui/main.js` returned nothing. The defect was the caller, not
// the core, which is exactly the shape rows 5b, 5c and 5d closed the same day.
//
// So this suite is not about "the panel renders". It is about the CEO's two constraints:
//
//   1. NOTHING SENDS ANYTHING. Held by `tests/feedback_no_outbound_tests.rs`, which now
//      reads this surface's code as well as the module's. This file's job is the other one.
//   2. HE SEES EXACTLY WHAT HIS RICHOS WOULD SAY, BEFORE ANY OF IT COULD EVER TRAVEL.
//      Checks 5, 6 and 7 are that constraint: the block on screen is compared byte for byte
//      against what the RUST renderer produces (read off a fixture a cargo test regenerates
//      from the live types), the approval carries that same block back, and the command
//      REFUSES an approval whose text is not what this build would say.
//
// NOTHING HERE IS ASSERTED AGAINST A HAND-WRITTEN COPY WHERE THE REAL THING CAN BE READ OFF
// DISK. The wording and the vocabulary come from `fixtures/feedback-vocabulary.json`, the
// rendered blocks from `fixtures/feedback-previews.json` and the stored shapes from
// `fixtures/feedback-entries.json` — all three written by
// `crates/richos-core/tests/feedback_surface_tests.rs` from the live constants and the live
// renderer. The store-unavailable sentence is compared to the `const` in `main.rs`. The
// outcome of every click is read back out of `mock.js`'s own store rather than inferred from
// the DOM, and check 2b checks the mock against the Rust constants in turn, so the chain has no
// free end.
//
// EVERY CHECK HERE WAS RUN RED ONCE by breaking the shipped source; the mutations are listed
// against their check numbers at the bottom of this file.
//
// Run: node feedback.js   (or `npm test` for every suite in this directory)
//      RICHOS_PLAYWRIGHT=/path/to/node_modules/playwright node feedback.js

"use strict";

const fs = require("fs");
const path = require("path");
const { leaveHome,
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
const SHOTS = path.join(__dirname, "shots-5");
const FIXTURES = path.join(__dirname, "fixtures");

/// The three fixtures, read once. Each is regenerated from the live Rust types by
/// `feedback_surface_tests.rs` and compared on every `cargo test` run, so a change to a
/// term, to the wrap, or to the stored shape reaches this file through a failing cargo test
/// rather than through a stale expectation nobody looked at.
const readFixture = (name) => JSON.parse(fs.readFileSync(path.join(FIXTURES, name), "utf8"));

/// Object keys, recursively sorted. `assertEqual` compares JSON text, and the two sides of
/// the vocabulary join are serialized by different libraries: `serde_json`'s map is ordered
/// and the browser's is insertion-ordered. Comparing raw text would fail on key ORDER, which
/// is not a fact about the product and would have to be worked around rather than fixed.
function canonical(v) {
  if (Array.isArray(v)) return v.map(canonical);
  if (v && typeof v === "object") {
    const out = {};
    for (const k of Object.keys(v).sort()) out[k] = canonical(v[k]);
    return out;
  }
  return v;
}
const VOCABULARY = readFixture("feedback-vocabulary.json");
const PREVIEWS = readFixture("feedback-previews.json");
const ENTRIES = readFixture("feedback-entries.json");
const REFERENCE = PREVIEWS.find((p) => p.name === "the reference case");

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
  await page.waitForSelector("#nav-feedback");
  page.__errors = errors;
  return page;
}

/// Drive `mock.js`'s store into one state, then open the panel through the button the CEO
/// presses. `setup` runs in the page against `window.__RICHOS_MOCK__` and touches no DOM.
async function open(browser, setup, viewport) {
  const page = await openApp(browser, viewport);
  if (setup) await page.evaluate("(" + setup.toString() + ")(window.__RICHOS_MOCK__)");
  await page.click("#nav-feedback");
  await page.waitForSelector("#feedback-overlay:not([hidden])");
  return page;
}

/// Press a key or an answer button and wait for the notice it produces — every recorded
/// answer either states its outcome or relays a refusal, so the notice is the settled signal
/// and no timer is needed.
async function answer(page, selector) {
  await page.click(selector);
  await page.waitForFunction(() => !document.getElementById("feedback-notice").hidden, null, {
    timeout: 5000,
  });
  return page.textContent("#feedback-notice");
}

/// Walk the surface the way the CEO does, up to the preview: a rating, the offer, the terms
/// of one fixture selection, and the button that renders it.
async function walkToPreview(page, fixtureCase) {
  const c = fixtureCase || REFERENCE;
  await page.click('#feedback-keys .desk-btn[data-key="' + c.key + '"]');
  await page.waitForSelector("#feedback-offer:not([hidden])");
  await page.click("#feedback-offer-yes");
  await page.waitForSelector("#feedback-choose:not([hidden])");
  await page.check('input[name="failure_class"][value="' + c.selection.failure_class + '"]');
  await page.check(
    'input[name="occurrences_this_session"][value="' + c.selection.occurrences_this_session + '"]'
  );
  for (const w of c.selection.generic_diagnosis) {
    await page.check('input[name="generic_diagnosis"][value="' + w + '"]');
  }
  for (const w of c.selection.contributing_condition) {
    await page.check('input[name="contributing_condition"][value="' + w + '"]');
  }
  await page.click("#feedback-show-preview");
  await page.waitForSelector("#feedback-preview-block:not([hidden])");
  return page;
}

const storeState = (page) => page.evaluate(() => window.__RICHOS_MOCK__.feedbackStoreState());
const visibleText = (page) =>
  page.evaluate(() =>
    (document.getElementById("feedback-overlay").innerText || "").replace(/\s+/g, " ").trim()
  );

/// A settled screenshot. `.overlay-panel` fades in over 160ms and a shot taken inside that
/// window is a photograph of an animation, not of a surface.
async function settledShot(page, name) {
  await page.waitForTimeout(300);
  fs.mkdirSync(SHOTS, { recursive: true });
  const s = await shot(page, name, { fullPage: false });
  publishShotFile(s.file, path.join(SHOTS, name + ".png"));
  return name + ".png (" + s.width + "x" + s.height + ", " + s.distinct + " distinct colors)";
}

// ---------------------------------------------------------------------------------------

async function main() {
  const run = createRun("the feedback channel — the local half, reachable, and honest about it");
  const { webkit } = loadPlaywright();
  const browser = await webkit.launch();
  let assertions = 0;
  const bump = (n) => {
    assertions += n;
    return n;
  };

  // ---- 1. the judgement, made visible -------------------------------------------------

  await run.check("1  a permanent control that NEVER counts up at him", async () => {
    // The design decision, checked structurally rather than written in a comment. Nothing
    // on this surface is ever waiting on the CEO — `feedback.rs` measured that a prompt
    // fired at a chosen moment would have caught none of the five moments in its reference
    // case — so there is no badge, no timer and no trigger. The Corrections button one line
    // up DOES carry a count, which is what makes the absence of one here a statement.
    const page = await openApp(browser);
    assert(await page.isVisible("#nav-feedback"), "the control must be in the rail at all times");
    assertEqual(
      await page.$$eval("#nav-feedback .rail-count", (n) => n.length),
      0,
      "a count on this control would be promising him something is waiting, and nothing ever is"
    );
    assert(
      await page.isVisible("#nav-corrections-count"),
      "the corrections badge must still be up, or this check is passing because nothing counts anywhere"
    );
    // Opening it is the ONLY thing that puts the question on screen: nothing fires it.
    assert(
      await page.isHidden("#feedback-overlay"),
      "the panel is on screen without the CEO having asked for it"
    );
    // ACROSS THE WHOLE SHIPPED UI, not `main.js` alone. This read one file of twelve until
    // 2026-09-05, so a `setTimeout(openFeedback, …)` written into `updates.js` or
    // `settings-button.js` would have left it green — and "nothing fires it" is the entire
    // claim of this check.
    const timers = SOURCES.uiMatches(/setTimeout\([^)]*openFeedback|setInterval\([^)]*openFeedback/);
    assertEqual(
      timers.map((t) => t.site),
      [],
      "something in the shipped UI opens the feedback surface on a timer: " +
        timers.map((t) => t.site).join(", ")
    );
    bump(5);
    await page.close();
    return (
      "no badge, no timer, no trigger across " + SOURCES.stateSources().length +
      " shipped UI file(s) — it appears when he opens it"
    );
  });

  // ---- 2. the wording is the backend's, not this layer's -------------------------------

  await run.check("2  the question and the four keys are the RUST constants, byte for byte", async () => {
    const page = await open(browser);
    assertEqual(
      await page.textContent("#feedback-question"),
      VOCABULARY.question,
      "the question on screen must be PROMPT_QUESTION"
    );
    const keys = await page.$$eval("#feedback-keys .desk-btn", (n) =>
      n.map((b) => ({ key: b.dataset.key, text: b.textContent }))
    );
    const expected = VOCABULARY.ratings
      .map((r) => ({ key: r.key, text: r.key + ": " + r.label }))
      .concat([{ key: "0", text: "0: Dismiss" }]);
    assertEqual(keys, expected, "the four keys must be the module's own, in its own order");
    // ...and the options line the module wrote agrees with what is on screen, so neither
    // side can drift without the other noticing.
    for (const k of keys.slice(0, 3)) {
      assert(VOCABULARY.options.indexOf(k.text) >= 0, "PROMPT_OPTIONS is missing " + k.text);
    }
    assert(VOCABULARY.options.indexOf("0: Dismiss") >= 0, "PROMPT_OPTIONS is missing the dismiss key");
    bump(4);
    await settledShot(page, "5-01-the-question-and-four-keys");
    await page.close();
    return "4 keys, verbatim: " + keys.map((k) => k.text).join(" | ");
  });

  await run.check("2b mock.js rehearses the wording and the vocabulary this build SHIPS", async () => {
    // The same join `corrections.js` check 12 makes, over a much larger surface: design and
    // QA look at this harness, and a fixture that drifts from the product certifies the
    // fixture. Both sides are read off disk — the fixture from a cargo test, the copy from
    // the running page.
    const page = await openApp(browser);
    const fromMock = await page.evaluate(() => window.__RICHOS_MOCK__.feedbackVocabulary());
    for (const field of ["question", "options", "reportOffer", "disclosureHeading"]) {
      assertEqual(fromMock[field], VOCABULARY[field], "mock.js drifted from " + field);
    }
    for (const list of ["ratings", "failureClass", "occurrences", "diagnosis", "conditions"]) {
      assertEqual(
        canonical(fromMock[list]),
        canonical(VOCABULARY[list]),
        "mock.js drifted from the " + list + " vocabulary"
      );
    }
    assert(VOCABULARY.diagnosis.length === 7 && VOCABULARY.failureClass.length === 5, "the fixture is not the vocabulary");
    assert(
      String(VOCABULARY.vocabularyFingerprint).length > 10,
      "the fingerprint is missing — a re-worded term could then change quietly"
    );
    bump(11);
    await page.close();
    return "4 sentences + 21 terms, identical across mock.js and the Rust constants";
  });

  // ---- 3. the offer is made on 1 and 2, and on nothing else ----------------------------

  await run.check("3  only 1 and 2 raise the offer to report", async () => {
    let page = null;
    for (const r of VOCABULARY.ratings.filter((x) => x.invitesReport)) {
      const fresh = await open(browser);
      await fresh.click('#feedback-keys .desk-btn[data-key="' + r.key + '"]');
      await fresh.waitForSelector("#feedback-offer:not([hidden])");
      assertEqual(
        await fresh.textContent("#feedback-offer-text"),
        VOCABULARY.reportOffer,
        "the offer must be REPORT_OFFER, verbatim"
      );
      // Nothing has been asked of the store yet — the offer is a question, not a record.
      assertEqual((await storeState(fresh)).entries.length, 0, "the offer recorded something");
      // The evidence is taken on the LAST of them, while the offer is on screen — a shot of
      // a page that has been closed and reopened is a photograph of something else.
      if (page) await page.close();
      page = fresh;
      bump(2);
    }
    await settledShot(page, "5-01b-the-offer-only-1-and-2-ever-see");
    await page.close();
    return VOCABULARY.ratings.filter((x) => x.invitesReport).length + " ratings invite the offer";
  });

  await run.check("3b a GOOD rating is recorded straight away and never offered a report", async () => {
    const page = await open(browser);
    const said = await answer(page, '#feedback-keys .desk-btn[data-key="3"]');
    assert(await page.isHidden("#feedback-offer"), "a 3 must not raise the offer — with_report would refuse it");
    const st = await storeState(page);
    assertEqual(st.entries.length, 1, "the rating must reach the store");
    assertEqual(st.entries[0].outcome, { kind: "rated", value: "good" }, "and be the rating he pressed");
    assertEqual(st.entries[0].report, { decision: "not_offered" }, "an offer that was never made is not a decline");
    assert(said.indexOf("Taken down") === 0, "and the surface says so: " + JSON.stringify(said));
    bump(5);
    await settledShot(page, "5-02-a-good-rating-is-never-asked-for-more");
    await page.close();
    return "3 -> rated good, report not_offered, no offer shown";
  });

  await run.check("3c a DISMISSAL is an answer; closing the panel is not", async () => {
    // `PromptOutcome::Dismissed` is what `0` records. Closing the panel records NOTHING —
    // he opened it himself, and writing a dismissal because he shut a window he chose to
    // open would put an answer in the file that nobody gave.
    const page = await open(browser);
    await answer(page, '#feedback-keys .desk-btn[data-key="0"]');
    let st = await storeState(page);
    assertEqual(st.entries.length, 1, "0 must record a dismissal");
    assertEqual(st.entries[0].outcome, { kind: "dismissed" }, "and it is a dismissal, not a rating");

    const other = await open(browser);
    await other.keyboard.press("Escape");
    await other.waitForSelector("#feedback-overlay", { state: "hidden" });
    assertEqual((await storeState(other)).entries.length, 0, "closing the panel wrote an answer nobody gave");
    bump(3);
    await other.close();
    await page.close();
    return "0 records a dismissal; Escape records nothing";
  });

  // ---- 4. the vocabulary, and the absence of anywhere else to put anything -------------

  await run.check("4  the whole vocabulary is on screen, and there is nowhere to type", async () => {
    const page = await open(browser);
    await page.click('#feedback-keys .desk-btn[data-key="1"]');
    await page.waitForSelector("#feedback-offer:not([hidden])");
    await page.click("#feedback-offer-yes");
    await page.waitForSelector("#feedback-choose:not([hidden])");

    for (const [name, list] of [
      ["failure_class", VOCABULARY.failureClass],
      ["occurrences_this_session", VOCABULARY.occurrences],
      ["generic_diagnosis", VOCABULARY.diagnosis],
      ["contributing_condition", VOCABULARY.conditions],
    ]) {
      const onScreen = await page.$$eval('#feedback-choose input[name="' + name + '"]', (n) =>
        n.map((i) => i.value)
      );
      assertEqual(onScreen, list.map((t) => t.wire), name + " on screen is not the vocabulary");
      bump(1);
    }
    // The sentences beside them are the terms' own, not this layer's paraphrase.
    const sentences = await page.$$eval(
      '#feedback-choose [data-group="generic_diagnosis"] .feedback-option span',
      (n) => n.map((s) => s.textContent)
    );
    assertEqual(sentences, VOCABULARY.diagnosis.map((t) => t.sentence), "the diagnosis sentences are not the module's");

    // THE FEATURE, STRUCTURALLY. `FeedbackPayload` has no `String` field at any depth, and
    // this is that fact on screen: a report is assembled from terms, and there is no box
    // for the CEO's specifics to occupy — not a filtered one, not a disabled one, none.
    const typeable = await page.$$eval("#feedback-overlay", (n) =>
      Array.from(n[0].querySelectorAll("input, textarea, [contenteditable]")).filter(
        (e) => !["radio", "checkbox"].includes(e.type)
      ).length
    );
    assertEqual(typeable, 0, "a free-text field appeared on a surface whose payload has nowhere to put one");
    bump(2);
    await settledShot(page, "5-03-the-whole-vocabulary-and-nowhere-to-type");
    await page.close();
    return "21 terms, 0 free-text fields";
  });

  await run.check("4b the preview cannot be asked for until the report could be assembled", async () => {
    // `assemble` refuses a report with no diagnosis term by name. Offering the button and
    // then relaying that refusal would teach him the surface is unreliable, so the control
    // is disabled instead — and it must genuinely arm when the choice is complete, or this
    // check would pass over a button that never works.
    const page = await open(browser);
    await page.click('#feedback-keys .desk-btn[data-key="2"]');
    await page.waitForSelector("#feedback-offer:not([hidden])");
    await page.click("#feedback-offer-yes");
    await page.waitForSelector("#feedback-show-preview");
    assert(await page.isDisabled("#feedback-show-preview"), "nothing chosen, and the button is live");
    await page.check('input[name="failure_class"][value="decision-handed-to-user"]');
    assert(await page.isDisabled("#feedback-show-preview"), "a class alone cannot be assembled into a report");
    await page.check('input[name="occurrences_this_session"][value="2"]');
    assert(await page.isDisabled("#feedback-show-preview"), "a report with no diagnosis term says nothing");
    await page.check('input[name="generic_diagnosis"][value="no-method-given"]');
    await page.waitForFunction(() => !document.getElementById("feedback-show-preview").disabled);
    assert(!(await page.isDisabled("#feedback-show-preview")), "the control never armed");
    bump(4);
    await page.close();
    return "disabled at 0, 1 and 2 of the three required choices; armed at 3";
  });

  // ---- 5. THE PREVIEW ---------------------------------------------------------------

  await run.check("5  what is on screen is what the RUST renderer produces, byte for byte", async () => {
    const page = await open(browser);
    await walkToPreview(page);
    const heading = await page.textContent("#feedback-disclosure-heading");
    const body = await page.textContent("#feedback-preview");
    assertEqual(heading, VOCABULARY.disclosureHeading, "the heading must be DISCLOSURE_HEADING");
    // The authority is `render_disclosure`'s own output, written to a fixture by a cargo
    // test that re-renders it on every run. A typed expectation here would prove only that
    // this file and itself agree.
    assertEqual(body, REFERENCE.text, "the report on screen is not what render_disclosure produces");
    // ...and the COMPOSITION on screen is the block the approval carries, so the two halves
    // being laid out separately cannot drift from the bytes that get checked.
    assertEqual(heading + "\n\n" + body, REFERENCE.full, "the on-screen composition is not the approved block");
    // It is the report, not a description of one: every line is a field or a folded block.
    assert(body.startsWith("taxonomy_version: v1\nrating: 1\n"), "the preview is not the payload: " + body.slice(0, 60));
    assert(body.indexOf("The asymmetry is the defect") > 0, "the vocabulary's own sentences are missing");
    // ALL OF IT IS ON SCREEN. The generic `.desk-preview` caps at 240px, which put the last
    // two lines of this block below the fold of a nested scroller — and "he sees exactly what
    // his RichOS would say" is not satisfied by a box he has to notice is scrollable. The
    // panel's one column scrolls; the report itself does not.
    const clipped = await page.$eval("#feedback-preview", (n) => ({
      scroll: n.scrollHeight,
      client: n.clientHeight,
    }));
    assertEqual(clipped.scroll, clipped.client, "the report is clipped inside its own scroller");
    // ...and no key is dressed as the recommended answer while he is reading a different one.
    assertEqual(
      await page.$$eval("#feedback-keys .desk-btn--confirm", (n) => n.length),
      0,
      "a rating is styled as selected on a screen showing a different rating's report"
    );
    bump(7);
    await settledShot(page, "5-04-exactly-what-would-be-said");
    await page.close();
    return body.split("\n").length + " lines, byte-identical to the Rust render";
  });

  await run.check("5b the same terms in any order render the same bytes", async () => {
    // "You are seeing exactly what would be reported" needs selection -> payload to be
    // deterministic, or two people who chose the same things would be shown two different
    // texts.
    //
    // TWO HALVES, AND THE FIRST ONE ALONE PROVED LESS THAN ITS NAME. Ticking the boxes in
    // reverse cannot detect an unsorted `assemble`: `chosen()` reads `querySelectorAll(…
    // :checked)`, which returns DOM order, so the surface hands over vocabulary order no
    // matter what order he clicked in. That was found by mutation — removing the sort from
    // `feedbackAssemble` left this check GREEN — and it is recorded rather than quietly
    // fixed. What it does prove is worth keeping: the surface cannot reorder a selection.
    // The second half is the one that reaches the assembler, with the term list posted
    // REVERSED through the bridge, which is the only way an unsorted assemble shows up.
    const reversed = PREVIEWS.find((p) => p.name === "the same terms in reverse, which must render identically");
    assert(reversed, "the reverse fixture is missing");
    const page = await open(browser);
    await walkToPreview(page, reversed);
    assertEqual(
      await page.textContent("#feedback-preview"),
      REFERENCE.text,
      "clicking the terms in a different order changed the block"
    );

    const posted = await page.evaluate(
      (c) => window.RichBridge.invoke("feedback_preview", { key: c.key, selection: c.selection }),
      { key: reversed.key, selection: reversed.selection }
    );
    assertEqual(posted.text, REFERENCE.text, "the assembler did not sort the selection it was handed");
    // And the fixture really is reversed, or this is two copies of one list.
    assertEqual(
      reversed.selection.generic_diagnosis,
      REFERENCE.selection.generic_diagnosis.slice().reverse(),
      "the reverse case is not reversed"
    );
    bump(4);
    await page.close();
    return "7 terms ticked in reverse AND posted in reverse — one block either way";
  });

  await run.check("5c a report with no conditions omits the key rather than showing it empty", async () => {
    const smallest = PREVIEWS.find((p) => p.name === "the smallest report there is");
    const page = await open(browser);
    await walkToPreview(page, smallest);
    const body = await page.textContent("#feedback-preview");
    assertEqual(body, smallest.text, "the smallest report is not what the renderer produces");
    assert(body.indexOf("contributing_condition") < 0, "showed a key with nothing under it");
    bump(2);
    await page.close();
    return "no conditions chosen, no empty key rendered";
  });

  // ---- 6. approving, and refusing ------------------------------------------------------

  await run.check("6  approving records exactly what he read, and it reads back", async () => {
    const page = await open(browser);
    await walkToPreview(page);
    assertEqual((await storeState(page)).entries.length, 0, "the preview recorded something");
    const said = await answer(page, "#feedback-approve");

    const st = await storeState(page);
    assertEqual(st.entries.length, 1, "approval must record exactly one entry");
    const entry = st.entries[0];
    assertEqual(entry.outcome, { kind: "rated", value: "bad" }, "the rating he gave");
    assertEqual(entry.report.decision, "approved", "the decision he made");
    assertEqual(
      entry.report.report.generic_diagnosis,
      REFERENCE.selection.generic_diagnosis,
      "the terms recorded are not the terms he chose"
    );
    // NO SECOND COPY OF THE TEXT. The entry carries the terms; the sentences come back only
    // by re-rendering, which is why the durable record has no unvalidated String in it.
    assert(
      JSON.stringify(entry).indexOf("The asymmetry is the defect") < 0,
      "the stored entry keeps a free-text copy of what he was shown"
    );
    assert(said.indexOf("Taken down") === 0, "the surface must say what happened: " + JSON.stringify(said));

    // And it is on screen, under the history, re-rendered from that payload.
    await page.waitForSelector(".feedback-entry");
    assertEqual(await page.$$eval(".feedback-entry", (n) => n.length), 1, "the answer is not in the history");
    assertEqual(
      await page.textContent(".feedback-entry .desk-preview"),
      REFERENCE.text,
      "the stored report renders differently from the one he approved"
    );
    bump(8);
    await settledShot(page, "5-05-approved-and-on-this-machine");
    await page.close();
    return "one entry, terms not prose, and the same block back on screen";
  });

  await run.check("6b refusing keeps the rating and nothing about the report", async () => {
    const page = await open(browser);
    await walkToPreview(page);
    const said = await answer(page, "#feedback-refuse");
    const st = await storeState(page);
    assertEqual(st.entries.length, 1, "the rating must still be kept");
    assertEqual(st.entries[0].report, { decision: "declined" }, "a declined report is not a report");
    assert(
      JSON.stringify(st.entries[0]).indexOf("unprepared-task") < 0,
      "a declined report left the payload behind"
    );
    assert(said.length > 0, "a refusal said nothing");
    await page.waitForSelector('.feedback-entry[data-decision="declined"]');
    assertEqual(
      await page.$$eval('.feedback-entry[data-decision="declined"] .desk-preview', (n) => n.length),
      0,
      "a declined row is showing a report that was never approved"
    );
    bump(5);
    await settledShot(page, "5-06-refused-and-nothing-kept-about-it");
    await page.close();
    return "rating kept, payload dropped, no report on the row";
  });

  // ---- 7. the two guards, invoked directly ---------------------------------------------

  await run.check("7  an approval for text he was NOT shown is refused, not recorded", async () => {
    // In one process this is structural — `ApprovedReport` has no public constructor and the
    // only route to one is `Disclosure::approve`, which cannot exist without having rendered
    // its text. Across the IPC boundary it has to be re-rendered and compared, and this is
    // that comparison being real rather than decorative. `main.js` posts back the block it
    // received, verbatim, so this is driven through the bridge directly — the shipped
    // surface has no path to it, which is exactly why the guard is worth proving.
    const page = await open(browser);
    const refusal = await page.evaluate(
      async (c) => {
        try {
          await window.RichBridge.invoke("feedback_record", {
            key: c.key,
            report: { decision: "approved", selection: c.selection, shown: c.full + " and one more thing" },
          });
          return null;
        } catch (e) {
          return String(e);
        }
      },
      { key: REFERENCE.key, selection: REFERENCE.selection, full: REFERENCE.full }
    );
    assert(refusal, "an approval for altered text was accepted");
    assertEqual(
      refusal,
      rustSentence(fs.readFileSync(MAIN_RS, "utf8"), "What you were shown isn't what I would say now"),
      "the refusal must be the backend's own sentence"
    );
    assertEqual((await storeState(page)).entries.length, 0, "the refused approval was recorded anyway");
    // The positive control: the SAME call with the unaltered block goes through, so this is
    // not passing because the command refuses everything.
    const ok = await page.evaluate(
      (c) =>
        window.RichBridge.invoke("feedback_record", {
          key: c.key,
          report: { decision: "approved", selection: c.selection, shown: c.full },
        }),
      { key: REFERENCE.key, selection: REFERENCE.selection, full: REFERENCE.full }
    );
    assertEqual(ok.report.decision, "approved", "the unaltered approval was refused too");
    assertEqual((await storeState(page)).entries.length, 1, "the good approval did not land");
    bump(5);
    await page.close();
    return "one byte of drift refuses; the exact block is accepted";
  });

  await run.check("7b a key that is not one of the four is refused, never taken as a dismissal", async () => {
    // `PromptOutcome::from_key` returns `None` for anything else, and silently recording it
    // as a dismissal would put invented data in the store. Unreachable from the four
    // buttons, which is why it is invoked directly.
    const page = await open(browser);
    for (const key of ["4", "x", "", "10"]) {
      const refusal = await page.evaluate(
        (k) =>
          window.RichBridge.invoke("feedback_record", { key: k, report: { decision: "not_offered" } }).then(
            () => null,
            (e) => String(e)
          ),
        key
      );
      assert(refusal, "key " + JSON.stringify(key) + " invented an answer");
      bump(1);
    }
    assertEqual((await storeState(page)).entries.length, 0, "an unrecognized key reached the store");
    bump(1);
    await page.close();
    return "4 unrecognized keys, 4 refusals, 0 entries";
  });

  // ---- 8. the honest states ------------------------------------------------------------

  await run.check("8  a file that would not open says so, and the keys are NOT offered", async () => {
    const page = await open(browser, (m) => m.setFeedbackAvailable(false));
    assert(await page.isVisible("#feedback-unavailable"), "the store-shut state must render");
    // The backend's own sentence, relayed verbatim — it names who owns the fix, and
    // paraphrasing it would lose that.
    const onScreen = (await page.textContent("#feedback-unavailable")).replace(/\s+/g, " ").trim();
    assertEqual(
      onScreen,
      rustSentence(fs.readFileSync(MAIN_RS, "utf8"), "the file I record them in wouldn't open"),
      "the sentence on screen must be main.rs's const"
    );
    // AND NO KEYS. Asking him what he thinks and dropping the answer is worse than not
    // asking, so the four buttons are not on the screen at all.
    assert(await page.isHidden("#feedback-keys"), "the four keys are offered over a store that cannot keep an answer");
    assert(await page.isHidden("#feedback-history-empty"), '"nothing recorded" is the one thing this state does not mean');
    bump(4);
    await settledShot(page, "5-07-the-file-would-not-open");
    await page.close();
    return "the backend's sentence, no keys, and no empty list pretending all is well";
  });

  await run.check("8b a store that IS there and refused a read is a different state, with a retry", async () => {
    const page = await open(browser, (m) =>
      m.setFeedbackReadFailure("the feedback log could not be read: Input/output error (os error 5)")
    );
    assert(await page.isVisible("#feedback-history-broke"), "a read failure must render as itself");
    assert((await page.textContent("#feedback-history-broke-reason")).indexOf("os error 5") >= 0, "the reason is kept");
    assert(await page.isVisible("#feedback-history-retry"), "transient, so the control that changes it is right here");
    assert(await page.isHidden("#feedback-history-empty"), "a failed read must not read as an empty store");
    assert(await page.isHidden("#feedback-unavailable"), "a failed read is not an uninstalled feature");
    // The keys ARE offered: the store is open, and an answer can still be kept.
    assert(await page.isVisible("#feedback-keys"), "the question is still answerable");
    // ...and the retry recovers, or the control is decoration.
    await page.evaluate(() => window.__RICHOS_MOCK__.setFeedbackReadFailure(null));
    await page.click("#feedback-history-retry");
    await page.waitForSelector("#feedback-history-empty:not([hidden])");
    bump(7);
    await page.close();
    return "read failure, its reason, a retry that works — and neither of the other two states";
  });

  await run.check("8c an empty store says WHY it is empty", async () => {
    const page = await open(browser);
    assert(await page.isVisible("#feedback-history-empty"), "an opened, empty store must say so");
    const text = await page.textContent("#feedback-history-empty");
    assert(
      /never puts the question to you on its own/.test(text),
      "an empty history would otherwise read as 'you had nothing to say'; the true reason is " +
        "that nothing has ever asked him: " + text
    );
    assertEqual(await page.$$eval(".feedback-entry", (n) => n.length), 0, "the empty state is rendering rows");
    bump(3);
    await page.close();
    return "empty, and honest about which of the three empties it is";
  });

  await run.check("9  stored answers the BACKEND really writes render as history rows", async () => {
    // The shapes come from `feedback_surface_tests.rs`, which takes them through a real
    // `FeedbackStore` round trip. Nothing in this file composes an entry; the file's own
    // bytes cross into the page and are parsed there.
    const page = await openApp(browser);
    await page.evaluate((raw) => {
      window.__RICHOS_MOCK__.seedFeedbackEntries(JSON.parse(raw).map((r) => r.entry));
    }, fs.readFileSync(path.join(FIXTURES, "feedback-entries.json"), "utf8"));
    await page.click("#nav-feedback");
    await page.waitForSelector(".feedback-entry");

    const rows = await page.$$eval(".feedback-entry", (n) =>
      n.map((c) => ({ rating: c.dataset.rating, decision: c.dataset.decision }))
    );
    assertEqual(
      rows,
      [
        { rating: "0", decision: "not_offered" },
        { rating: "2", decision: "declined" },
        { rating: "1", decision: "approved" },
      ],
      "the three stored shapes must render as three different rows"
    );
    // A dismissal is not a rating and must not be labelled as one.
    assertEqual(
      await page.$$eval(".feedback-entry .feedback-entry-label", (n) => n.map((x) => x.textContent)),
      ["Dismissed", "OK, but could be better", "Bad"],
      "a stored outcome is being labelled as something it is not"
    );
    // The approval's text is re-rendered from the stored payload — no second copy is kept.
    const approved = ENTRIES.find((r) => r.shown);
    assertEqual(
      await page.textContent('.feedback-entry[data-decision="approved"] .desk-preview'),
      approved.shown,
      "the stored report renders differently from what he approved"
    );
    assertEqual(
      await page.$$eval('.feedback-entry[data-decision="not_offered"] .desk-preview', (n) => n.length),
      0,
      "a row with no report is showing one"
    );
    bump(5);
    await settledShot(page, "5-08-three-answers-on-this-machine");
    await page.close();
    return "3 stored shapes, 3 distinct rows, the approval's text re-rendered";
  });

  // ---- 10. the surface's own rules ------------------------------------------------------

  await run.check("10 a refusal reaches the screen and never dies in a console", async () => {
    const page = await open(browser);
    await page.click('#feedback-keys .desk-btn[data-key="1"]');
    await page.waitForSelector("#feedback-offer:not([hidden])");
    // The store goes away between the question and the answer — the one ordinary way a
    // recorded answer refuses from this screen. He had a live, answerable panel a moment
    // ago, so the refusal has to reach him rather than a console.
    await page.evaluate(() => window.__RICHOS_MOCK__.setFeedbackAvailable(false));
    const said = await answer(page, "#feedback-offer-no");
    assert(said.length > 20, "the refusal did not reach the notice: " + JSON.stringify(said));
    assert(
      said.indexOf("wouldn't open") >= 0,
      "the notice must carry the backend's own words: " + JSON.stringify(said)
    );
    assert(
      await page.$eval("#feedback-notice", (n) => n.classList.contains("desk-notice--attention")),
      "a refusal must not be styled as good news"
    );
    assertEqual((await storeState(page)).entries.length, 0, "the refused answer was recorded anyway");
    bump(4);
    await page.close();
    return "the backend's refusal, on screen, marked as a refusal";
  });

  await run.check("11 NO PAGINATION, at any length", async () => {
    // The house rule, checked structurally. Forty answers are one scrolling column.
    const page = await openApp(browser);
    await page.evaluate((raw) => {
      const one = JSON.parse(raw)[0].entry;
      const many = [];
      for (let i = 0; i < 40; i++) many.push(JSON.parse(JSON.stringify(one)));
      window.__RICHOS_MOCK__.seedFeedbackEntries(many);
    }, fs.readFileSync(path.join(FIXTURES, "feedback-entries.json"), "utf8"));
    await page.click("#nav-feedback");
    await page.waitForSelector(".feedback-entry");
    assertEqual(await page.$$eval(".feedback-entry", (n) => n.length), 40, "every answer is in the DOM at once");
    const text = await visibleText(page);
    assert(!/\bPage\b|\bNext\b|\bPrevious\b|\b1 of \d/.test(text), "a page control appeared: " + text.slice(0, 120));
    assert(
      await page.$eval("#feedback-body", (n) => ["auto", "scroll"].includes(getComputedStyle(n).overflowY)),
      "one scrolling column"
    );
    bump(3);
    await page.close();
    return "40 answers, one column, zero page controls";
  });

  await run.check("12 the panel opens and closes by keyboard, and returns focus where it was", async () => {
    const page = await openApp(browser);
    await page.focus("#nav-feedback");
    await page.keyboard.press("Enter");
    await page.waitForSelector("#feedback-overlay:not([hidden])");
    assertEqual(await page.getAttribute("#nav-feedback", "aria-expanded"), "true", "the button must say it is open");
    await page.keyboard.press("Escape");
    await page.waitForSelector("#feedback-overlay", { state: "hidden" });
    assertEqual(
      await page.evaluate(() => document.activeElement && document.activeElement.id),
      "nav-feedback",
      "Escape closes it and hands focus back — §18"
    );
    bump(3);
    await page.close();
    return "Enter opens, Escape closes, focus returns to the rail button";
  });

  await run.check("13 all six commands are reachable from the shipped UI layer", async () => {
    // Row 5's whole point is that NONE of them had a caller. The inventory is derived from
    // `main.rs`'s own `invoke_handler` list and from `main.js`/`mock.js` on disk — neither
    // side is typed here.
    const rs = fs.readFileSync(MAIN_RS, "utf8");
    const from = rs.indexOf("generate_handler![");
    assert(from >= 0, "no generate_handler! in main.rs");
    const to = rs.indexOf("]", from + "generate_handler![".length);
    assert(to > from, "generate_handler! is not closed");
    const handler = rs.slice(from, to);
    const registered = Array.from(new Set(handler.match(/\bfeedback_[a-z_]+/g) || [])).sort();
    assertEqual(registered.length, 6, "expected 6 registered commands, found: " + registered.join(", "));

    const mainJs = fs.readFileSync(path.join(UI_DIR, "main.js"), "utf8");
    const mockJs = fs.readFileSync(path.join(UI_DIR, "mock.js"), "utf8");
    assertEqual(
      registered.filter((c) => mockJs.indexOf('"' + c + '"') < 0),
      [],
      "every command needs a mock, or the browser suites cannot drive it"
    );
    // ACROSS THE SHIPPED UI, for the reason `corrections.js` check 13 states: a narrow scan
    // fails in the safe direction here, but the title claims "the shipped UI layer" and a
    // check that can go red over a correct product is one somebody widens by deleting.
    const called = new Set(
      SOURCES.sourcesOfRole("ui").flatMap((f) => registered.filter((c) => f.code.indexOf('"' + c + '"') >= 0))
    );
    assertEqual(
      registered.filter((c) => !called.has(c)),
      [],
      "row 5 is only closed when app/ui/ actually calls each one"
    );
    bump(4);
    return (
      registered.length + " commands, every one called from the " + SOURCES.stateSources().length +
      " shipped UI file(s): " + registered.join(", ")
    );
  });

  await run.check("NEGATIVE CONTROL: this suite asserted a non-zero number of things", async () => {
    assert(
      assertions >= 60,
      "only " + assertions + " assertions ran. A suite that verifies little and reports green " +
        "is the failure this repository has caught three times."
    );
    return assertions + " assertions against the real DOM and the real store under WebKit";
  });

  await browser.close();
  const failed = run.report();
  console.log("\nScreenshots: " + SHOTS + " — every one decoded and pixel-counted before it counted as evidence.");
  console.log(
    failed
      ? "\n" + failed + " check(s) FAILED"
      : "\nhe can ask, rate, read exactly what would be said, refuse it, and find his own answers on his own machine."
  );
  process.exitCode = failed ? 1 : 0;
}

main().catch((e) => {
  console.error(e);
  process.exit(1);
});

// ---------------------------------------------------------------------------------------
// RUN RED — the mutation that made each check fail, applied to the SHIPPED source
//
// Full transcript, including the two bad mutations and what they taught:
// docs/verification/feedback-channel-2026-08-30/mutation-runs.txt — 25 runs, one edit each
// ---------------------------------------------------------------------------------------
//
//  1   index.html: add a `<span class="rail-count">` inside #nav-feedback
//  1b  updates.js: `setTimeout(openFeedback, 60000)`
//        -> `something in the shipped UI opens the feedback surface on a timer:
//           updates.js:833`
//        The timer clause read `main.js` alone until 2026-09-05 while the claim in the
//        check is "nothing fires it" — a claim about the product, tested against one file
//        of twelve.
//        -> the control promises something is waiting, and nothing on it ever is
//  2   main.js `renderFeedback`: question.textContent = "How are we doing?"
//        -> the surface paraphrases PROMPT_QUESTION, which is the paraphrase the module
//           holds its wording in constants to prevent
//  2b  mock.js: Occurrences "once" -> "one time"
//        -> the preview harness rehearses a term label the product does not use
//  3   main.js `answerFeedback`: offer text = "Can we tell the developers?"
//        -> REPORT_OFFER paraphrased, on the one screen where wording is the product
//  3b  main.js `renderFeedbackKeys`: answerFeedback(r.key, true) for every rating
//        -> a 3 is offered a report `with_report` would refuse
//  3c  main.js: the dismiss button sets phase = "answered" and records nothing
//        -> 0 becomes a way of closing the panel, and the dismissal is lost
//  4   main.js `renderFeedbackChoose`: drop the contributing-condition group
//        -> a whole term type is missing from the choice. Turns FIVE checks red, because
//           the reference selection can no longer be made at all — which is the right
//           answer and the reason it is listed here rather than treated as noise
//  4b  main.js `syncFeedbackChoice`: btn.disabled = false
//        -> the preview is offered over a selection the assembler will refuse
//  5   main.js `showFeedbackPreview`: preview.textContent = rendered.full
//        -> the heading is rendered twice and the block on screen stops being the block
//           the approval carries
//  5   style.css: restore max-height: 240px on #feedback-preview
//        -> the last two lines of the report sit below the fold of a nested scroller, and
//           "he sees exactly what would be said" quietly becomes "most of it"
//  5b  mock.js `feedbackAssemble`: drop the sort from generic_diagnosis
//        -> BAD MUTATION ON THE FIRST ATTEMPT, and the check was strengthened rather than
//           the mutation abandoned: 5b stayed GREEN, because `chosen()` reads
//           `querySelectorAll(:checked)` and gets DOM order whatever order he clicked in.
//           The check now also POSTS the reversed selection through the bridge, which is
//           the only path that reaches the assembler, and the mutation turns it red
//  5c  mock.js `feedbackRender`: emit contributing_condition unconditionally
//        -> an empty list renders as a key with nothing under it
//  6   main.js: approve posts { decision: "declined" }
//        -> he says yes and nothing is recorded
//  6b  main.js: refuse posts the approval
//        -> he says no and the report is recorded anyway
//  7   mock.js `feedback_record`: `if (false)` on the shown-text comparison
//        -> consent recorded for text he was never shown, which is the one failure this
//           whole feature is arranged to make impossible
//  7b  mock.js `feedbackOutcome`: return { kind: "dismissed" } for any key
//        -> an unrecognized key is silently recorded as an answer nobody gave
//  8   main.js `renderFeedback`: feedback-keys.hidden = false
//        -> the four keys are offered over a store that cannot keep an answer
//  8   mock.js: one word of FEEDBACK_STORE_ABSENT changed
//        -> the harness rehearses a sentence the product does not say
//  8b  main.js `refreshFeedback`: `if (false) feedback.historyFailed = String(e);`
//        -> a transient read failure is reported as an uninstalled feature, and the retry
//           disappears with it. Written as `if (false)` deliberately: replacing the line
//           outright would orphan the `else` beneath it and turn every check red, which
//           proves nothing about this one
//  8c  index.html: shorten the empty line to "Nothing is recorded here yet."
//        -> an empty history reads as "you had nothing to say" rather than "nothing has
//           ever asked you"
//  9   main.js `renderFeedbackEntry`: `else if (true)` on the approved branch
//        -> a row with no report is shown carrying one
//  10  main.js `recordFeedback`: replace the catch body with console.error(e)
//        -> the refusal dies in the console, which is the rule affordances.js enforces
//  11  style.css `.desk-body`: overflow-y: hidden
//        -> forty answers with no way to reach the fortieth
//  12  main.js: drop closeFeedback() from the Escape chain
//        -> the panel cannot be dismissed from the keyboard. Also turns 3c red, which uses
//           Escape to prove that closing the panel records nothing
//  13  main.rs: remove feedback_history from generate_handler!
//        -> a registered command disappears and nothing notices

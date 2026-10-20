// TECHY MODE, criterion by criterion — open-items row 3.1, Phase 2.
//
// Phase 1 (richos `48561e4`) routed every non-text agent frame, unified the per-turn `seq`,
// wrote the day-sharded journal and shipped `rich://machinery`, with 22 passing tests — and
// `grep -rn machinery app/ui/` returned three comments. A correct, tested core with no
// caller. This suite is about the caller, and about the two things the brief said must stay
// true while it was built.
//
// THE COMPLETION CRITERION IS OBSERVABLE, NOT INTERNAL: open a thread with techy mode on and
// see THAT TURN'S REAL MACHINERY — the tool calls and the status each actually returned —
// and toggle it off and on per thread. Checks 4-8 are that sentence, clause by clause.
//
// THE TWO THINGS THAT WERE ALREADY TRUE AND HAD TO STAY TRUE:
//
//   1. A thread from before the routing commit shows the HONEST empty state. "No machinery
//      was recorded for this conversation" and "this conversation had no machinery" are
//      different sentences and only the first is true. Check 12 is that; check 13 is its
//      harder half — a store the OS refuses to open is NOT an empty one, and a renderer
//      that serves the empty sentence over it is lying about the system.
//   3. §1.5's BETWEEN-TURN LANE, added 2026-08-30 (checks 18-21). Everything the session
//      says with NO turn in flight used to hit no sink at all. It now attaches to the
//      THREAD — `turnId: None` is a first-class state, not a bug — and renders in its own
//      section, because a record with no turn has no position in the conversation and
//      drawing one inside the stream would claim a position nothing witnessed. Check 19 is
//      the honest empty state for it, check 21 the standing order on the rendered surface.
//
//   2. No row is drawn for an event that provably never arrives. Readable thinking text
//      fires ZERO times on EITHER wire (0 on claude-agent-acp 0.70.0; 7 signature-only
//      blocks with empty text on the native binary) and client-directed `fs/*` never fires
//      at all — the native CLI was OBSERVED doing its own file IO — so §5's
//      own day-one mockup — which opens with `● thinking ⌄` — cannot be delivered. Check 16
//      asserts the absence, because an always-empty affordance tells the CEO the model is
//      not thinking when the truth is that the adapter does not say.
//
// NOTHING IS ASSERTED AGAINST A HAND-WRITTEN COPY WHERE THE REAL THING CAN BE READ OFF DISK.
// The four state sentences come from `src-tauri/src/machinery_view.rs` and `main.rs` through
// `rustSentenceAfter`; the payload SHAPE comes from `fixtures/machinery-payload.json`, which
// `examples/machinery_payload.rs` writes from the live Rust types against a real ledger and
// a real journal. So the chain renderer -> mock -> live types has no free end.
//
// EVERY CHECK HERE WAS RUN RED ONCE by breaking the shipped source; the mutations are listed
// against their check numbers at the bottom of this file.
//
// Run: node techy.js   (or `npm test` for every suite in this directory)
//      RICHOS_PLAYWRIGHT=/path/to/node_modules/playwright node techy.js

"use strict";

const fs = require("fs");
const path = require("path");
const { leaveHome,
  loadPlaywright,
  shot,
  awaitWorkerChipSettled,
  assertOnThread,
  createRun,
  assert,
  assertEqual,
  rustSentenceAfter: rustSentence,
  SEED_THEME,
  UI_DIR,
} = require("./lib/harness");
const { COUNT_BRIDGE, bridgeQuiet } = require("./lib/bridge-quiet");

/// Bridge latency for a whole run, in milliseconds; zero unless set. Every wait in this file is
/// on the app (`bridgeQuiet`), so the suite is meant to be as green at 300 as at 0; check 32
/// runs its walk at 300 every time (hunt part 2 recheck R41).
const LAG_MS = Number(process.env.RICHOS_TECHY_LAG_MS || 0);

const APP = "file://" + path.join(UI_DIR, "index.html");
const SRC = path.resolve(UI_DIR, "..", "src-tauri", "src");
const MACHINERY_RS = fs.readFileSync(path.join(SRC, "machinery_view.rs"), "utf8");
const MAIN_RS = fs.readFileSync(path.join(SRC, "main.rs"), "utf8");
const SHOTS = path.join(__dirname, "shots-3-1");
const FIXTURE = JSON.parse(fs.readFileSync(path.join(__dirname, "fixtures", "machinery-payload.json"), "utf8"));

/// The five CEO-facing sentences, read out of the Rust that ships them. A reworded const
/// reaches this file as a failing check rather than as a stale expectation.
///
/// EVERY MARKER IS A PHRASE THAT APPEARS ONLY IN THE CONST BODY, never in the doc comment
/// above it. `rustSentenceAfter` scans BACKWARD from the marker to the nearest opening
/// quote, and the doc comments here quote their own sentences to explain them — so a marker
/// taken from the opening clause finds the DOC COMMENT's quote and returns a fragment. That
/// is not hypothetical; it is what the first version of this file did, and the fragment it
/// returned was a prefix of the real sentence, so `assertEqual` was the only thing between
/// a green run and four checks silently asserting a substring.
const NOTHING_RECORDED = rustSentence(MACHINERY_RS, "Retention started on 2026-08-28");
const NOT_RETAINED = rustSentence(MACHINERY_RS, "fills up as Rich works");
const UNREADABLE = rustSentence(MACHINERY_RS, "is refusing to open it");
const RAW_NOT_RETAINED = rustSentence(MAIN_RS, "above is the whole record that was");
const RAW_TRUNCATED = rustSentence(MAIN_RS, "you're seeing the start of it");
const BETWEEN_TURNS_QUIET = rustSentence(MACHINERY_RS, "not proof the");

// ---------------------------------------------------------------------------------------
// Driving the REAL shell — nothing stubbed that mock.js does not already own
// ---------------------------------------------------------------------------------------

/// `preset` is the mock's own PRE-BOOT switch (`window.__RICHOS_MOCK_PRESET__`), applied
/// with `addInitScript` because it decides state `main.js` has already branched on by the
/// time a setter could run — see mock.js's note over it.
///
/// THE ONLY PRESET THIS SUITE USES IS `{ chosenEntity: null }`, and it is the only way to
/// reach a booted app with NO conversation active: `activeThreadId` in `main.js` is set
/// when a thread opens and is never set back to null, so once anything has been opened the
/// session has a conversation for the rest of its life. A check that wanted "nothing open"
/// and merely navigated away would be testing a state the product does not have.
async function openApp(browser, viewport, preset, lagMs) {
  const page = await browser.newPage({ viewport: viewport || { width: 1400, height: 950 } });
  // Every bridge call counted, so a press can be waited out by what it started rather than by
  // a number (`lib/bridge-quiet.js`).
  await page.addInitScript(COUNT_BRIDGE, typeof lagMs === "number" ? lagMs : LAG_MS);
  const errors = [];
  page.on("pageerror", (e) => errors.push(String(e)));
  page.on("console", (m) => {
    if (m.type() === "error") errors.push("console: " + m.text());
  });
  // THIS SUITE STATES ITS LIGHTING (CEO §63, 2026-09-19). The shipped default preference
  // used to be `dark`, so every walk in here rendered dark without anyone saying so, and the
  // committed reference PNGs were taken that way. §63 makes the default `system`, which
  // hands the palette to the host's `colorScheme` — light, in this harness — and flipped
  // every one of them in a single run with every check still green, because nothing this
  // suite asserts is about lighting. A reference shot is a picture a person LOOKS at, so the
  // palette is named here rather than inherited from a default that is free to move again.
  // `SEED_THEME` writes both the mirror and the store and says why; the store decides.
  await page.addInitScript(SEED_THEME, "dark");
  if (preset) await page.addInitScript((v) => { window.__RICHOS_MOCK_PRESET__ = v; }, preset);
  await page.goto(APP);
  // The home screen is the landing surface now; this suite is about the app UI behind it.
  await leaveHome(page);
  // With no company chosen there is no thread list to wait for — that is the whole point of
  // that preset — so the wait is on the shell instead.
  if (preset && preset.chosenEntity === null) await page.waitForSelector("#set-btn");
  else await page.waitForSelector(".nav-thread", { state: "attached" });
  page.__errors = errors;
  return page;
}

async function openThread(page, threadId) {
  await page.click('.nav-thread[data-thread-id="' + threadId + '"]');
  // `main.js`'s `openThread` is a chain of awaited bridge calls (switch, context, techy mode,
  // the timeline or machinery read, the work status) and then a render frame, so the thread
  // is open when the bridge is quiet and the frame is painted. This replaced a predicate that
  // was always true and a fixed 250 ms (hunt part 2 recheck R41).
  await bridgeQuiet(page, "opening " + threadId);
}

/// The CEO's own path to a per-conversation flip: the keyboard shortcut, then his third
/// option. TWO ACTS SINCE 2026-09-18, not one — ⌘⇧T asks WHERE it applies, like every
/// other entrance, so a helper that only pressed the key would leave a modal open and every
/// check below would then be measuring a sheet rather than a conversation.
///
/// It still stands for exactly what it stood for before: this conversation on (or off), the
/// other conversations and the global default untouched. `apply_techy_scope`'s
/// `TechyScope::Thread` arm is the same `techy_threads.insert(thread_id, enabled)` that the
/// old `set_techy_mode` path performed, so the twenty checks that use this helper as a way
/// to REACH the technical view are measuring the state they always were.
///
/// The raw keystroke is deliberately NOT wrapped here — checks 29 and 31 press it
/// themselves, because what the key alone does is the thing they are about.
///
/// AND THE SHEET IS ANSWERED FROM THE KEYBOARD, WHICH IS MEASURED RATHER THAN PREFERRED.
/// The first version clicked the radio and the confirm button; that moves the pointer to
/// the middle of the window, out of the rail where `openThread`'s click left it, which
/// un-hovers the conversation row and takes its `.nav-thread-more` (⋯) off the screen. Six
/// of this suite's committed shots changed by 60-100 bytes each for that reason and no
/// other, and the same move cost `contrast.js` two measured nodes on two surfaces. Arrow
/// keys and Enter leave the pointer where the CEO's own hand left it.
async function pressToggle(page) {
  await page.keyboard.press("Meta+Shift+T");
  await page.waitForSelector("#techy-scope:not([hidden])");
  // Derived, not typed: a conversation in no company is offered two options, not three.
  for (let i = 0; i < 3 && (await scopeChecked(page)) !== "thread"; i++) {
    await page.keyboard.press("ArrowDown");
  }
  assertEqual(await scopeChecked(page), "thread", "the per-conversation tier is the one being taken");
  await page.keyboard.press("Enter");
  await page.waitForSelector("#techy-scope", { state: "hidden" });
  // The sheet hides after `set_techy_scope` answers; the thread is then re-read in place.
  await bridgeQuiet(page, "the per-conversation toggle");
}

/// Flip the technical view's ONE switch (round 27: the Settings panel's `#set-techy`) — a
/// SCOPE-AMBIGUOUS control, so inside a conversation this is what opens the three-way choice.
/// The panel closes as the question opens (main.js `openTechyScope`), so nothing of it covers
/// the question's buttons.
async function openTechySwitch(page, on) {
  await page.click("#set-btn");
  await page.waitForSelector("#set-menu #set-techy");
  if (on) await page.check("#set-techy");
  else await page.uncheck("#set-techy");
  await page.waitForSelector("#techy-scope:not([hidden])");
}

/// Put Settings away, the way the CEO does (its own button), so the thread list underneath is
/// clickable again. A no-op when it is not open, which is the usual case: the panel closes
/// itself as the scope question opens.
async function dismissPopover(page) {
  if (await page.locator("#set-menu").isHidden()) return;
  await page.click("#set-btn");
  await page.waitForSelector("#set-menu", { state: "hidden" });
  await bridgeQuiet(page, "putting Settings away");
}

/// The sheet's option labels, in document order and only the ones on screen — a hidden
/// option is not a choice he has.
const scopeOptions = (page) =>
  page.$$eval("#techy-scope-options .techy-scope-option", (rows) =>
    rows.filter((r) => !r.hidden && r.getBoundingClientRect().height > 0)
      .map((r) => r.textContent.trim())
  );

/// Which option is selected right now.
const scopeChecked = (page) =>
  page.evaluate(() => {
    const on = document.querySelector('#techy-scope input[name="techy-scope"]:checked');
    return on ? on.value : null;
  });

/// Pick a scope and confirm it — the whole act, as he performs it, ending with the rail
/// back the way he found it.
async function pickScope(page, value) {
  await page.check('#techy-scope input[value="' + value + '"]');
  await page.click("#techy-scope-confirm");
  await page.waitForSelector("#techy-scope", { state: "hidden" });
  // The sheet hides after `set_techy_scope` answers; the thread is then re-read in place.
  await bridgeQuiet(page, "confirming the " + value + " scope");
  await dismissPopover(page);
}

/// The technical view's one switch, read without opening the panel (it is painted while the
/// panel is closed too), and read again by opening Settings the way a person does.
const railSwitch = (page) => page.isChecked("#set-techy");

async function settingsSwitch(page) {
  await page.click("#set-btn");
  await page.waitForSelector("#set-techy");
  const on = await page.isChecked("#set-techy");
  await page.keyboard.press("Escape");
  await bridgeQuiet(page, "closing the settings menu");
  return on;
}

/// The state line's SENTENCE, without the operator-facing reason appended after it.
const sentenceOf = (page) =>
  page.evaluate(() => {
    const el = document.getElementById("techy-state");
    if (!el || el.hidden) return null;
    return el.childNodes[0] ? el.childNodes[0].textContent : "";
  });

const techRows = (page) =>
  page.$$eval(".tl-tech", (rows) =>
    rows.map((r) => ({
      title: r.querySelector(".tl-tech-title").textContent,
      state: r.dataset.state,
      word: r.querySelector(".tl-activity-state") ? r.querySelector(".tl-activity-state").textContent : null,
      vendor: r.dataset.vendor || null,
      paths: Array.from(r.querySelectorAll(".tl-tech-path")).map((p) => p.textContent),
    }))
  );

/// ONE SHUTTER FOR EVERY COMMITTED SHOT IN THIS SUITE, and it waits for the chrome this
/// suite never drove.
///
/// `main.js` polls `get_worker_status` every 3,000 ms and rebuilds the drill chip from the
/// answer. It USED TO HAVE NO LEADING CALL, so from the moment a thread became the open
/// conversation the chip zone was empty for up to three seconds and then a whole line of text
/// arrived in it. Every shot below is of the conversation surface, and every one of them is
/// taken within a second or two of opening a thread, which was exactly that window.
///
/// THE LEADING CALL NOW EXISTS — `openThread` awaits `pollWorkerStatus()` before it returns —
/// and this wait is kept rather than deleted, because it was never about the FIRST paint: the
/// 3,000 ms interval still re-renders the chip for as long as the page is open, so a shutter
/// can still land in the middle of a re-render. What changed is the cost: the settle now
/// agrees with a chip that is already on screen instead of waiting for one to arrive.
///
/// MEASURED, 2026-09-20, on this tree, BEFORE the leading call existed. A probe that reached
/// `3-1-07`'s state and then read `document.body.innerText` three times 1.6 s apart: the line
/// `⋯ 1 working · 1 done · 1 I can't see` was ABSENT at the shutter point and PRESENT 1.6 s
/// later. That is the defect the leading call fixes. And with the five
/// owning suites of the seven undeclared unstable shots run concurrently — the contention
/// the gate's shards have, which a serial run of one suite does not — `3-1-03` came back
/// different in two consecutive rounds, 79,928 of 1,330,000 pixels at a worst channel delta
/// of 214, alternating between two pictures. That is the chip arriving, or not, before the
/// shutter; nothing in this suite is about the work chip.
///
/// `awaitWorkerChipSettled` is the harness's own wait on the PRODUCT'S re-render agreeing
/// with itself, not on a duration — the same call `memory-strategy.js` puts in front of all
/// nine of its committed shots, and for the same reason. It THROWS rather than photographing
/// a surface that is still moving: a picture of a moment is not evidence of a state.
async function evidence(page, name) {
  const settled = await awaitWorkerChipSettled(page);
  if (!settled.settled) {
    throw new Error(
      name + ": the work chip was still changing after the budget (" + settled.renders +
        " re-renders, last text " + JSON.stringify(settled.text) + "). A shot of a surface " +
        "that is still moving is evidence of a moment, not of a state."
    );
  }
  return shot(page, name);
}

async function main() {
  const pw = loadPlaywright();
  const run = createRun("techy mode — the renderer, the per-thread toggle and techy_default (row 3.1)");
  const browser = await pw.webkit.launch();
  fs.mkdirSync(SHOTS, { recursive: true });

  // ---- 1. the calm default, untouched -------------------------------------------------
  //
  // §3.3's constraint is the one that makes Urban's v1 direction need no amendment: with
  // techy mode off, the conversation surface is byte-identical to today. NOT "tidy" — no
  // affordance AT ALL, because a visible affordance IS a change to the default experience.
  let calmHTML = null;
  await run.check("1. off: no technical row, no chip, no state line, no hint", async () => {
    const page = await openApp(browser);
    await openThread(page, "acme");
    calmHTML = await page.innerHTML("#messages");
    assertEqual(await page.locator(".tl-tech").count(), 0, "a technical row cannot exist in the calm view");
    assert(await page.locator("#techy-chip").isHidden(), "no chip");
    assert(await page.locator("#techy-state").isHidden(), "no state line");
    // `innerText`, not `textContent`: the chip is IN the document and `hidden`, and the
    // claim being made is about what the CEO can SEE. `textContent` reads hidden nodes and
    // would fail this check over the very element that is correctly invisible.
    const text = await page.evaluate(() => document.getElementById("stage").innerText);
    for (const hint of ["technical detail", "Technical view", "Show technical", "machinery"]) {
      assert(!text.includes(hint), "the conversation must not hint at techy mode: found " + hint);
    }
    await evidence(page, "../shots-3-1/3-1-01-off-nothing-changed");
    assertEqual(page.__errors, [], "no page errors");
    await page.close();
    return "the calm surface carries no affordance at all";
  });

  // ---- 2. the sentences on screen are the ones Rust ships ------------------------------
  await run.check("2. the state sentences are the Rust constants, verbatim", async () => {
    // Read off disk on both sides. A copy in mock.js that drifted from `machinery_view.rs`
    // would let every other check below pass over a sentence the product no longer says.
    const page = await openApp(browser);
    const mockCopies = await page.evaluate(() => ({
      nothing: window.__RICHOS_MOCK__.TECHY_NOTHING_RECORDED,
      notRetained: window.__RICHOS_MOCK__.TECHY_NOT_RETAINED,
      unreadable: window.__RICHOS_MOCK__.TECHY_UNREADABLE,
      rawNotRetained: window.__RICHOS_MOCK__.TECHY_RAW_NOT_RETAINED,
      rawTruncated: window.__RICHOS_MOCK__.TECHY_RAW_TRUNCATED,
      betweenQuiet: window.__RICHOS_MOCK__.TECHY_BETWEEN_TURNS_QUIET,
    }));
    assertEqual(mockCopies.nothing, NOTHING_RECORDED, "the empty-state sentence");
    assertEqual(mockCopies.notRetained, NOT_RETAINED, "the never-retained sentence");
    assertEqual(mockCopies.unreadable, UNREADABLE, "the unreadable sentence");
    assertEqual(mockCopies.rawNotRetained, RAW_NOT_RETAINED, "the evicted-raw sentence");
    assertEqual(mockCopies.rawTruncated, RAW_TRUNCATED, "the truncated-raw sentence");
    assertEqual(mockCopies.betweenQuiet, BETWEEN_TURNS_QUIET, "the quiet-between-turns sentence");
    // And the three that answer "why is there nothing" are three DIFFERENT sentences.
    const distinct = new Set([NOTHING_RECORDED, NOT_RETAINED, UNREADABLE]);
    assertEqual(distinct.size, 3, "three states, three sentences — never one sentence for three facts");
    await page.close();
    return "6 sentences joined to their Rust consts";
  });

  // ---- 3. §7.2 is not answered in copy -------------------------------------------------
  //
  // "How long do raw payloads survive?" is the CEO's, and open. A sentence naming a
  // duration would answer it in copy, and then the answer would live in two places.
  await run.check("3. no sentence names a retention duration (§7.2 stays his)", async () => {
    for (const [name, sentence] of [
      ["NOTHING_RECORDED", NOTHING_RECORDED],
      ["NOT_RETAINED", NOT_RETAINED],
      ["UNREADABLE", UNREADABLE],
      ["RAW_NOT_RETAINED", RAW_NOT_RETAINED],
      ["RAW_TRUNCATED", RAW_TRUNCATED],
      ["BETWEEN_TURNS_QUIET", BETWEEN_TURNS_QUIET],
    ]) {
      assert(!/\b14 days?\b|\b2 ?GB\b|\bfortnight\b|\btwo weeks\b/i.test(sentence), name + " names a window");
    }
    // The retroactivity DATE is different and is deliberately allowed: it is a fact about
    // what was recorded, not a policy about what will be kept.
    assert(NOTHING_RECORDED.includes("2026-08-28"), "the routing date IS stated — that is a fact, not a policy");
    assert(BETWEEN_TURNS_QUIET.includes("2026-08-30"), "and so is the lane's own start date, for the same reason");
    return "no window in any sentence; the routing date is named";
  });

  // ---- 4. THE CRITERION: the turn's real machinery ------------------------------------
  await run.check("4. on: the tool calls that actually ran, one row each", async () => {
    const page = await openApp(browser);
    await openThread(page, "acme");
    await pressToggle(page);
    const rows = await techRows(page);
    assert(rows.length >= 6, "expected the turn's tool calls, got " + rows.length);
    // The MERGED title — the real command — never the opening event's placeholder. The wire
    // sends `title: "Terminal"` on the open and the command on a later update (probe run1
    // n=11 vs n=12), so a renderer reading only the open event shows a column of "Terminal".
    assert(!rows.some((r) => r.title === "Terminal" || r.title === "Preparing file…"),
      "a placeholder title reached the screen: " + JSON.stringify(rows.map((r) => r.title)));
    assert(rows.some((r) => r.title.startsWith("python3 scripts/counter-model.py --list")),
      "the full command, not an ellipsis: " + JSON.stringify(rows.map((r) => r.title)));
    assert(rows.some((r) => r.paths.length), "a touched path is on screen");
    await evidence(page, "../shots-3-1/3-1-02-the-turns-real-machinery");
    assertEqual(page.__errors, [], "no page errors");
    await page.close();
    return rows.length + " rows: " + rows.map((r) => r.state).join(", ");
  });

  // ---- 5. ...and the status each actually returned -------------------------------------
  await run.check("5. every outcome is drawn as its own word, and `unknown` is never `done`", async () => {
    const page = await openApp(browser);
    await openThread(page, "acme");
    await pressToggle(page);
    const rows = await techRows(page);
    const byState = {};
    for (const r of rows) byState[r.state] = r.word;
    // §18: status never by color alone. In technical mode EVERY state is spelled out —
    // `done` included, because it is what he is looking at.
    assertEqual(byState.completed, "done", "a completed call says done");
    assertEqual(byState.failed, "failed", "a failed call says failed");
    assertEqual(byState.unknown, "outcome not recorded",
      "34 of 58 measured tool events carried no status — that is not a completion claim");
    assert(byState.unknown !== "done", "an unrecorded outcome must NEVER read as done");
    await page.close();
    return JSON.stringify(byState);
  });

  // ---- 6. no rollup in technical mode --------------------------------------------------
  await run.check("6. three reads stay three commands, not `Read 3 files`", async () => {
    const page = await openApp(browser);
    await openThread(page, "acme");
    // The CALM view rolls them up, correctly: "Read a file" three times says one thing. The
    // turn is settled, so its work transcript is collapsed (§6.4) — open it the way the CEO
    // does, through the summary line, rather than reaching into the model.
    await page.click(".tl-collapsed-summary");
    // The rolled-up row is the fact waited for (hang guard); it used to be a 200ms sleep
    // (2026-09-29, audit R10). The assertion below still names a view that never rolled up.
    await page
      .waitForFunction(() => Array.from(document.querySelectorAll(".tl-activity-text")).some((n) => n.textContent === "Read 3 files"), null, { timeout: 10000 })
      .catch(() => {});
    const calmLabels = await page.$$eval(".tl-activity-text", (n) => n.map((x) => x.textContent));
    assert(calmLabels.includes("Read 3 files"), "the calm view DOES roll up: " + JSON.stringify(calmLabels));
    await pressToggle(page);
    const rows = await techRows(page);
    const reads = rows.filter((r) => r.title.startsWith("Read "));
    assertEqual(reads.length, 3, "three distinct reads, three rows");
    assertEqual(new Set(reads.map((r) => r.title)).size, 3, "and three distinct commands");
    assert(!rows.some((r) => /^Read \d+ files$/.test(r.title)), "no rollup label in technical mode");
    await page.close();
    return "calm labels " + JSON.stringify(calmLabels) + " -> " + reads.length + " technical rows";
  });

  // ---- 7. the rows that exist ONLY here ------------------------------------------------
  await run.check("7. the untyped vendor kind and the auto-approved permission are here, and only here", async () => {
    const page = await openApp(browser);
    await openThread(page, "acme");
    const calm = await page.textContent("#messages");
    assert(!calm.includes("message_delta"), "an accounting frame is not something Rich DID");
    assert(!calm.includes("auto-approved"), "and nobody was asked to approve anything");
    await pressToggle(page);
    const rows = await techRows(page);
    const vendor = rows.find((r) => r.vendor === "stream_event:message_delta");
    assert(vendor, "§1.4 G5: an untyped kind is retained and rendered as one dim line, with its kind name");
    assert((await page.textContent("#messages")).includes("auto-approved"),
      "a permission request is recorded as a FACT and shown here — never as a decision awaiting him");
    await page.close();
    return "vendorKind row present; both absent from the calm view";
  });

  // ---- 8. per thread, and reversible ---------------------------------------------------
  await run.check("8. the shortcut pins ONE conversation and leaves the others alone", async () => {
    const page = await openApp(browser);
    await openThread(page, "acme");
    await pressToggle(page);
    assert(await page.locator("#techy-chip").isVisible(), "acme is on");
    assertEqual(await page.textContent("#techy-chip-label"), "Technical view · this conversation",
      "and the chip says WHICH switch is holding it, so turning it off is predictable");
    await openThread(page, "hiring");
    assert(await page.locator("#techy-chip").isHidden(), "hiring is untouched");
    assertEqual(await page.locator(".tl-tech").count(), 0, "and shows no technical row");
    await openThread(page, "acme");
    assert(await page.locator("#techy-chip").isVisible(), "acme kept its pin across the switch away and back");
    const state = await page.evaluate(() => window.__RICHOS_MOCK__.techyState());
    assertEqual(state, { default: false, companies: {}, threads: { acme: true } },
      "one thread pinned, no company touched, the global default untouched");
    await page.close();
    return "per-thread override, and the global default never moved";
  });

  // ---- 9. the global switch (§3.1: "all" must be ONE switch) ----------------------------
  //
  // CHANGED 2026-09-18 WITH THE CEO'S §7.1 ANSWER, and the change is worth stating because
  // half of this check's old claim is now deliberately false. It used to flip the rail
  // switch from inside `acme` — which `acme` had pinned OFF — and assert that acme's pin
  // survived. That assertion described a switch that did nothing visible on the screen it
  // was flipped on, and it survived review because nobody had asked what the CEO would see.
  //
  // He answered §7.1 by making the reach an explicit choice, so "one switch for all of
  // them" is now his first option, and applying it CLEARS the pins below it on the path he
  // is standing on (and on no other path — `hiring` below is the proof). The subject of
  // this check is unchanged: the global tier reaches every conversation that is not pinned
  // away from it. What changed is that the conversation he used to do it from is no longer
  // the exception to his own instruction.
  await run.check("9. Settings switches every conversation, and pins elsewhere survive it", async () => {
    const page = await openApp(browser);
    // A conversation pinned OFF in the same company, so the global switch has something to
    // fail to reach — and it is NOT the one he does it from, because his own instruction is
    // that the conversation he is looking at follows the choice he just made.
    await openThread(page, "hiring");
    await pressToggle(page); // pin hiring ON
    await pressToggle(page); // ...then pin it OFF, deliberately against the default below
    await openThread(page, "acme");
    await openTechySwitch(page, true);
    await pickScope(page, "all-companies");
    await evidence(page, "../shots-3-1/3-1-03-one-switch-for-all-of-them");
    const state = await page.evaluate(() => window.__RICHOS_MOCK__.techyState());
    assertEqual(state, { default: true, companies: {}, threads: { hiring: false } },
      "global on; the pin he made on ANOTHER conversation keeps its own answer");
    await openThread(page, "hiring");
    assert(await page.locator("#techy-chip").isHidden(), "hiring is pinned off and stays off");
    await openThread(page, "acme");
    // Its subject is that the global switch reaches an unpinned conversation. Since round 27
    // the switch is in the Settings panel, which closes as the scope question opens, so
    // nothing covers the rail rows this check clicks next.
    assert(await page.locator("#techy-chip").isVisible(), "the conversation he switched it on from is ON");
    await openThread(page, "partner");
    assert(await page.locator("#techy-chip").isVisible(), "and so is an unpinned thread in ANOTHER company");
    assertEqual(await page.textContent("#techy-chip-label"), "Technical view · everywhere",
      "and says so, rather than implying he chose it there");
    await page.close();
    return "global default reaches the unpinned everywhere; a pin elsewhere holds against it";
  });

  // ---- 10. §7.1 stays reversible -------------------------------------------------------
  //
  // Without a way back to the default a pin is one-way, "all of their conversations" stops
  // reaching any thread he ever touched, and the product has answered the CEO's open
  // question for him. The clearing arm is `set_techy_mode(enabled: null)`.
  await run.check("10. a pinned conversation can be handed back to the default (§7.1 stays open)", async () => {
    const page = await openApp(browser);
    await openThread(page, "acme");
    await pressToggle(page);
    let mode = await page.evaluate(() => window.RichBridge.invoke("techy_mode", { threadId: "acme" }));
    assertEqual(mode, { enabled: true, source: "thread", default: false }, "pinned");
    mode = await page.evaluate(() => window.RichBridge.invoke("set_techy_mode", { threadId: "acme", enabled: null }));
    assertEqual(mode, { enabled: false, source: "default", default: false }, "handed back, not stuck at its old value");
    await page.evaluate(() => window.RichBridge.invoke("set_techy_default", { enabled: true }));
    mode = await page.evaluate(() => window.RichBridge.invoke("techy_mode", { threadId: "acme" }));
    assertEqual(mode, { enabled: true, source: "default", default: true }, "and now follows the global switch");
    await page.close();
    return "pin -> clear -> follows the default, in both directions";
  });

  // ---- 11. the §3.3 invariant, measured ------------------------------------------------
  await run.check("11. on and back off leaves the conversation BYTE-IDENTICAL", async () => {
    const page = await openApp(browser);
    await openThread(page, "acme");
    const before = await page.innerHTML("#messages");
    await pressToggle(page);
    await page.click("#mach\\:mach_a1"); // open a raw pane, so the round trip is not trivial
    // Open means its bytes are on screen, waited for rather than slept for (audit R10).
    await page.waitForFunction(() => /q3-acme\.csv/.test((document.getElementById("raw:mach_a1") || {}).textContent || ""), null, { timeout: 10000 });
    await pressToggle(page);
    const after = await page.innerHTML("#messages");
    assertEqual(after.length, before.length, "same length");
    assert(after === before, "the calm view must be byte-identical after a round trip");
    assert(await page.locator("#techy-state").isHidden(), "and the state line is gone with it");
    assert(await page.locator("#techy-chip").isHidden(), "and so is the chip");
    await evidence(page, "../shots-3-1/3-1-04-off-again-and-identical");
    await page.close();
    return before.length + " bytes of DOM, unchanged";
  });

  // ---- 12. THE HONEST EMPTY STATE ------------------------------------------------------
  await run.check("12. a conversation from before the routing commit says what was RECORDED", async () => {
    const page = await openApp(browser);
    await openThread(page, "partner");
    await pressToggle(page);
    assertEqual(await sentenceOf(page), NOTHING_RECORDED, "verbatim from machinery_view.rs");
    assertEqual(await page.getAttribute("#techy-state", "data-state"), "nothing_recorded", "and it is that state");
    assertEqual(await page.locator(".tl-tech").count(), 0, "there is genuinely nothing to draw");
    // The conversation itself is still there. An empty machinery column is not an empty
    // thread, and the CEO did not lose his conversation by turning a view on.
    assert((await page.textContent("#messages")).includes("carry split"), "his conversation still renders");
    await evidence(page, "../shots-3-1/3-1-05-nothing-was-recorded-for-this-one");
    assertEqual(page.__errors, [], "no page errors");
    await page.close();
    return "nothing_recorded, with the conversation intact";
  });

  // ---- 13. ...and the state that is NOT that one ---------------------------------------
  await run.check("13. a store that cannot be read is NOT reported as an empty one", async () => {
    const page = await openApp(browser);
    await page.evaluate(() => window.__RICHOS_MOCK__.breakMachinery("acme"));
    await openThread(page, "acme");
    await pressToggle(page);
    const said = await sentenceOf(page);
    assertEqual(said, UNREADABLE, "verbatim from machinery_view.rs");
    assert(said !== NOTHING_RECORDED, "and it is NOT the empty-state sentence");
    assertEqual(await page.getAttribute("#techy-state", "data-state"), "unreadable", "a different state, drawn differently");
    // The operator-facing reason is on screen too — the CEO is told plainly it is not his
    // to fix, and whoever set RichOS up gets the path.
    const reason = await page.textContent("#techy-state .techy-reason");
    assert(reason.includes("acme"), "the reason names the path: " + reason);
    assert(said.includes("haven't lost it"), "and it does not imply the record is gone");
    // And NO rows: `Spine::timeline` reads the journal through `read_thread`, which returns
    // nothing for a directory the OS refused, so the technical view of an unreadable thread
    // is prose and a duration row. The sentence is the only thing standing between the CEO
    // and "he did nothing" — which is why it must not be the empty-state one.
    assertEqual(await page.locator(".tl-tech").count(), 0, "the journal gave nothing back");
    assert((await page.textContent("#messages")).includes("comparables"), "his conversation still renders");
    await evidence(page, "../shots-3-1/3-1-06-i-cannot-read-it-which-is-not-empty");
    await page.close();
    return "unreadable, with the operator's reason: " + reason;
  });

  // ---- 14. the raw pane's three answers -------------------------------------------------
  await run.check("14. expanding a row shows the output, or says honestly why it cannot", async () => {
    const page = await openApp(browser);
    await openThread(page, "acme");
    await pressToggle(page);

    await page.click("#mach\\:mach_a1");
    await page.waitForSelector("#raw\\:mach_a1");
    // Each pane's answer is the fact waited for (hang guard); these were 200ms sleeps before the
    // read (2026-09-29, audit R10). Every assertion below still names a wrong answer.
    const paneSays = (id, test) =>
      page
        .waitForFunction(([i, t]) => {
          const el = document.getElementById("raw:" + i);
          return !!el && new RegExp(t).test(el.textContent || "");
        }, [id, test], { timeout: 10000 })
        .catch(() => {});
    await paneSays("mach_a1", "q3-acme\\.csv");
    const retained = await page.textContent("#raw\\:mach_a1");
    assert(retained.includes("q3-acme.csv"), "the raw payload is on screen: " + retained.slice(0, 80));
    assertEqual(await page.getAttribute("#raw\\:mach_a1", "data-note"), null, "no note over real bytes");

    // §2.4's honest degrade: the Tier-B window has passed over this one. The NORMALIZED
    // record above it is untouched — structure, title, status, paths, summary — and the
    // pane says why the bytes are gone rather than showing a blank.
    await page.click("#mach\\:mach_a5");
    await page.waitForFunction(() => { const el = document.getElementById("raw:mach_a5"); return !!el && el.getAttribute("data-note") !== null; }, null, { timeout: 10000 }).catch(() => {});
    assertEqual(await page.textContent("#raw\\:mach_a5"), RAW_NOT_RETAINED, "verbatim from main.rs");
    assertEqual(await page.getAttribute("#raw\\:mach_a5", "data-note"), "not_retained", "and it is that state");
    const stillThere = (await techRows(page)).find((r) => r.title.startsWith("grep -rn"));
    assert(stillThere && stillThere.word === "done", "the record still renders with its outcome");

    // §2.4's 32 KB cap fired: what is on screen is a PREFIX, and it is labelled. A prefix
    // that looks whole is worse than one that says it is not.
    await page.click("#mach\\:mach_a6");
    await page.waitForFunction(() => !!document.querySelector('[id="raw:mach_a6"] .tl-tech-note'), null, { timeout: 10000 }).catch(() => {});
    assertEqual(await page.textContent("#raw\\:mach_a6 .tl-tech-note"), RAW_TRUNCATED, "verbatim from main.rs");
    await evidence(page, "../shots-3-1/3-1-07-the-output-and-the-two-honest-degrades");
    assertEqual(page.__errors, [], "no page errors");
    await page.close();
    return "retained / not retained / truncated — three answers, three sentences";
  });

  // ---- 15. a window, not a cockpit ------------------------------------------------------
  await run.check("15. techy mode adds no control of any kind", async () => {
    const page = await openApp(browser);
    await openThread(page, "acme");
    await pressToggle(page);
    // The ONLY button inside a technical row is its own disclosure head. R2 business-action
    // governance is deferred to V2 by CEO decision for v1 and all 1.x, and an approve, a
    // re-run or an interrupt here is how that gets un-deferred by accident (§5, §9).
    const buttons = await page.$$eval(".tl-tech button", (b) => b.map((x) => x.className));
    assert(buttons.every((c) => c === "tl-tech-head"), "unexpected control in a technical row: " + JSON.stringify(buttons));
    const text = await page.textContent("#messages");
    for (const verb of ["Approve", "Deny", "Re-run", "Rerun", "Interrupt", "Kill"]) {
      assert(!text.includes(verb), "a control verb reached the technical view: " + verb);
    }
    await page.close();
    return buttons.length + " buttons, all of them the row's own disclosure";
  });

  // ---- 16. the two rows that are NOT drawn, and why -------------------------------------
  await run.check("16. no row exists for an event that provably never arrives", async () => {
    const page = await openApp(browser);
    await openThread(page, "acme");
    await pressToggle(page);
    const text = await page.textContent("#messages");
    // §5's own day-one mockup opens with `● thinking ⌄`. Readable thinking text fires ZERO
    // times on either wire — 0 `agent_thought_chunk` on claude-agent-acp 0.70.0, and 7
    // `thinking` blocks with EMPTY text on the native binary — including in probe runs built
    // for nothing else.
    assert(!/thinking/i.test(text), "a thinking row would tell him the model is not thinking, which is not what is true");
    // And `fs/read_text_file` / `fs/write_text_file` never fire with both capabilities
    // declared and both tools exercised — `ClientFsCall` is real and inert on 0.70.0.
    assert(!text.includes("fs/read_text_file") && !text.includes("fs/write_text_file"),
      "an inert route must not get a permanently empty affordance");
    const rows = await techRows(page);
    assert(!rows.some((r) => r.vendor === "agent_thought_chunk"), "and no thought row by vendor kind either");
    await page.close();
    return "no thinking row, no fs row — both routes stay built in richos-core";
  });

  // ---- 17. the mock is joined to the live Rust payload ----------------------------------
  await run.check("17. the mock's shape is the shape the real command emits", async () => {
    // `fixtures/machinery-payload.json` is written by `examples/machinery_payload.rs` from
    // the live types, a real ledger and a real journal. Without this join every check above
    // proves the renderer against a mock and nothing about the backend.
    const page = await openApp(browser);
    const got = await page.evaluate(() => window.RichBridge.invoke("get_machinery", { threadId: "acme" }));
    const real = FIXTURE.recorded;
    assertEqual(Object.keys(got).sort(), Object.keys(real).sort(), "the envelope's keys");
    assertEqual(got.state, real.state, "both are `recorded`");
    assertEqual(got.timeline.mode, real.timeline.mode, "both are the technical view");
    const realAct = real.timeline.items.find((i) => i.kind === "activity" && i.detail);
    const mockAct = got.timeline.items.find((i) => i.kind === "activity" && i.detail);
    assert(realAct && mockAct, "both carry an activity with a technical half");
    for (const key of ["kind", "state", "activityType", "detailRef", "summary", "visibility"]) {
      assert(key in mockAct, "the mock activity is missing " + key);
      assert(key in realAct, "the real activity is missing " + key);
    }
    for (const key of ["title", "locations"]) {
      assert(key in mockAct.detail && key in realAct.detail, "detail." + key + " on both sides");
    }
    // And the empty state's envelope agrees too — that is the one the renderer branches on.
    assertEqual(FIXTURE.nothingRecorded.state, "nothing_recorded", "the fixture carries the empty state");
    assertEqual(FIXTURE.nothingRecorded.sentence, NOTHING_RECORDED, "with the same sentence the UI shows");
    await page.close();
    return "envelope and item shapes agree with the live payload";
  });

  // ---- 18. §1.5's between-turn lane: the update that used to have nowhere to go ---------
  //
  // THE COMPLETION CRITERION, on the rendered surface. The client delivered a frame only
  // while `current_prompt` was `Some`; anything the adapter said at session start or after
  // a turn's answer had already been returned hit no sink at all. It now attaches to the
  // thread and shows here.
  await run.check("18. what the session said between turns is on screen, in its own section", async () => {
    const page = await openApp(browser);
    await openThread(page, "acme");

    // OFF: not a trace of it. §3.3's rule is about the whole conversation surface, and a
    // heading the CEO can read is an affordance whatever its rows say.
    assert(await page.locator("#between-turns").isHidden(), "the section does not exist for a calm view");
    const calm = await page.evaluate(() => document.getElementById("stage").innerText);
    assert(!calm.includes("Between turns"), "and its heading is not readable either");

    await pressToggle(page);
    assert(await page.locator("#between-turns").isVisible(), "the section is here in technical mode");
    const rows = await page.$$eval("#between-turns-rows .bt-row", (r) =>
      r.map((x) => ({ kind: x.dataset.vendor, title: x.querySelector(".tl-tech-title").textContent }))
    );
    assertEqual(rows.map((r) => r.kind), ["system:init", "system:status"],
      "the two frames measured between turns on 2026-08-31, in journal order");
    // The vendor frame name IS the row (§1.4 G5's "one dim line"), because there is no
    // CEO-safe semantic line for "the agent restated its tool list" and inventing one would
    // be worse than the frame name.
    assertEqual(rows.map((r) => r.title), ["system:init", "system:status"], "frame name as label");
    assert(await page.locator("#between-turns-quiet").isHidden(), "no empty-state sentence over a full lane");
    await evidence(page, "../shots-3-1/3-1-08-between-turns-has-somewhere-to-go");
    assertEqual(page.__errors, [], "no page errors");
    await page.close();
    return rows.length + " between-turn rows, present only in technical mode";
  });

  // ---- 19. the honest empty state for the lane -----------------------------------------
  await run.check("19. a quiet lane SAYS it is quiet, and says why", async () => {
    // An empty box under a heading reads as a broken feature. The sentence distinguishes
    // the two things an empty lane can mean — nothing arrived, or nothing was ever written
    // because the conversation predates the lane — exactly as `NOTHING_RECORDED` does for
    // the thread as a whole.
    const page = await openApp(browser);
    await page.evaluate(() => window.__RICHOS_MOCK__.clearBetweenTurns("acme"));
    await openThread(page, "acme");
    await pressToggle(page);
    assert(await page.locator("#between-turns").isVisible(), "the section still appears — silence is a state, not an absence");
    assertEqual(await page.$$eval("#between-turns-rows .bt-row", (r) => r.length), 0, "no rows");
    assertEqual(await page.textContent("#between-turns-quiet"), BETWEEN_TURNS_QUIET, "verbatim from machinery_view.rs");
    // And the conversation above it is untouched: an empty lane is not an empty thread.
    assert((await page.locator(".tl-tech").count()) > 0, "the turn's own machinery still renders");
    await evidence(page, "../shots-3-1/3-1-09-a-quiet-lane-says-so");
    assertEqual(page.__errors, [], "no page errors");
    await page.close();
    return "the quiet sentence, with the conversation intact above it";
  });

  // ---- 20. an unreadable store does not get a SECOND claim ------------------------------
  await run.check("20. over an unreadable store the lane says nothing at all", async () => {
    // "Nothing was recorded between turns" is a claim a store that refused to open never
    // supported. `machinery_view.rs` returns `null` in that state and the state line above
    // already speaks for the whole view — this is check 13's rule, one level down.
    const page = await openApp(browser);
    await page.evaluate(() => window.__RICHOS_MOCK__.breakMachinery("acme"));
    await openThread(page, "acme");
    await pressToggle(page);
    assertEqual(await sentenceOf(page), UNREADABLE, "the state line owns this screen");
    assert(await page.locator("#between-turns").isHidden(), "and the lane makes no second claim");
    assertEqual(await page.textContent("#between-turns-quiet"), "", "not even hidden text to leak later");
    await page.close();
    return "one honest sentence, not two contradictory ones";
  });

  // ---- 21. the standing order, on the rendered surface -----------------------------------
  await run.check("21. no between-turn row names a turn, a session, or a rotation", async () => {
    // Re-prime and rotation machinery is `internal: true` and refused at the timeline's
    // guard, which `richos-core` proves. What THIS checks is the surface: the lane's rows
    // arrive at session boundaries, so it is the one place a session identifier would
    // become a rotation tell — and `BetweenTurnItem` carries none.
    const page = await openApp(browser);
    await openThread(page, "acme");
    await pressToggle(page);
    const payload = await page.evaluate(() => window.RichBridge.invoke("get_machinery", { threadId: "acme" }));
    const lane = payload.timeline.betweenTurns;
    assert(lane.length > 0, "there is a lane to inspect");
    for (const row of lane) {
      assert(!("turnId" in row), "a lane row must carry no turn: " + JSON.stringify(row));
      assert(!("sessionId" in row), "and no session id: " + JSON.stringify(row));
      assertEqual(row.visibility, "technical", "every row is technical, so a CEO view is empty by construction");
    }
    const section = await page.textContent("#between-turns");
    for (const word of ["rotat", "re-prime", "reprime", "successor", "lease", "session id"]) {
      assert(!section.toLowerCase().includes(word), "the lane referenced " + word);
    }
    // The live fixture agrees — this is not a property of the mock.
    for (const row of FIXTURE.recorded.timeline.betweenTurns) {
      assert(!("turnId" in row) && !("sessionId" in row), "the LIVE payload leaks one: " + JSON.stringify(row));
    }
    await page.close();
    return lane.length + " rows, none of them naming a turn or a session";
  });

  // ---- 22. N3 (dev-walk audit, 2026-09-17) -----------------------------------------------
  //
  // "The technical-view badge is cut off mid-word and runs underneath the settings gear:
  // 'Technical view · this conversation'. No ellipsis; the gear simply sits on top of it."
  // The gear (`#set-btn`) is `position: fixed` at the window's own top-right corner
  // (`--chrome-lane`, style.css) and `#stage-header` never reserved that corner, so the
  // badge's own flexbox happily grew right through it. Checked at the app's documented
  // floor (`window_geometry.rs::PREFERRED_MIN_WIDTH` / `MIN_HEIGHT`, 1024x700) — the exact
  // geometry the audit's own screenshot was taken at, and the one with the least room to
  // give.
  await run.check("22. the technical-view badge and the settings gear never overlap, at the app's minimum window", async () => {
    const page = await openApp(browser, { width: 1024, height: 700 });
    await openThread(page, "acme");
    await pressToggle(page);
    assertEqual(
      await page.textContent("#techy-chip-label"),
      "Technical view · this conversation",
      "the longer of the chip's two sentences — the one the audit found running under the gear"
    );
    const boxes = await page.evaluate(() => {
      const rect = (el) => {
        const r = el.getBoundingClientRect();
        return { left: r.left, right: r.right, top: r.top, bottom: r.bottom };
      };
      return { chip: rect(document.getElementById("techy-chip")), gear: rect(document.getElementById("set-btn")) };
    });
    const overlapsX = boxes.chip.left < boxes.gear.right && boxes.chip.right > boxes.gear.left;
    const overlapsY = boxes.chip.top < boxes.gear.bottom && boxes.chip.bottom > boxes.gear.top;
    assert(
      !(overlapsX && overlapsY),
      "badge " + JSON.stringify(boxes.chip) + " intersects gear " + JSON.stringify(boxes.gear)
    );
    // And where the label genuinely does not fit, it ellipsizes rather than running under
    // anything — `scrollWidth` over `clientWidth` is the same clipped-label test 17b already
    // uses for the settings menu's own rows.
    const label = await page.evaluate(() => {
      const el = document.getElementById("techy-chip-label");
      return { scrollWidth: el.scrollWidth, clientWidth: el.clientWidth };
    });
    assert(
      label.scrollWidth <= label.clientWidth + 1,
      "the label overflows its own box instead of eliding: scrollWidth " +
        label.scrollWidth + " > clientWidth " + label.clientWidth
    );
    await page.close();
    return (
      "chip " + JSON.stringify(boxes.chip) + " vs gear " + JSON.stringify(boxes.gear) +
      " — no intersection, label " + label.scrollWidth + "/" + label.clientWidth + " (fits or elides)"
    );
  });


  // =======================================================================================
  // §7.1'S THREE-WAY SCOPE CHOICE — the CEO's answer, 2026-09-18
  //
  // His sentence is the acceptance criterion:
  //
  //   "when the user toggles the techy mode on while being inside a conversation thread,
  //    then there should be something like a radio button choice between 3 choices. And
  //    those choices could be something like 1) 'For all threads in all companies' 2) 'For
  //    all threads in this company' 3) 'For this thread only'. And the first choice should
  //    be preselected after the user switches the techy mode on. That also means that later
  //    the user should be able to switch off the techy mode for a given company or a given
  //    thread."
  //
  // The fixture is what makes the middle tier provable: `acme` and `hiring` are both in
  // `northwind`, `partner` is in `lumen`, and `legacy` is in no company at all. So "this
  // company" has a sibling to reach and a bystander to leave alone, which is the only way
  // a company tier differs observably from the two that already existed.
  // =======================================================================================

  await run.check("23. switching it on inside a conversation asks WHERE — his three choices, his order, first preselected", async () => {
    const page = await openApp(browser);
    await openThread(page, "acme");
    assert(await page.locator("#techy-scope").isHidden(), "nothing is asked before he touches a switch");
    await openTechySwitch(page, true);
    assert(await page.locator("#techy-scope").isVisible(), "the choice is on screen");
    assertEqual(
      await page.textContent("#techy-scope-title"),
      "Turn on the technical view",
      "and the title says which direction the confirm goes, so it is never inferred from which switch he touched"
    );
    assertEqual(await page.textContent("#techy-scope-confirm"), "Turn it on");
    // HIS THREE, IN HIS ORDER. The one word changed is "threads" -> "conversations", which
    // is the word every other surface in this product says to him.
    assertEqual(await scopeOptions(page), [
      "For all conversations in all companies",
      "For all conversations in this company",
      "For this conversation only",
    ]);
    assertEqual(await scopeChecked(page), "all-companies", "the FIRST choice is preselected");
    // AND NOTHING HAS BEEN WRITTEN YET. The question is a question until he confirms it.
    assertEqual(
      await page.evaluate(() => window.__RICHOS_MOCK__.techyState()),
      { default: false, companies: {}, threads: {} },
      "asking is not applying"
    );
    await evidence(page, "../shots-3-1/3-1-05-where-should-this-apply");
    await page.click("#techy-scope-cancel");
    await page.waitForSelector("#techy-scope", { state: "hidden" });
    await page.close();
    return "three options, his order, first preselected, nothing written until he confirms";
  });

  await run.check("24. 'for all conversations in this company' reaches the sibling and leaves the other company alone", async () => {
    const page = await openApp(browser);
    await openThread(page, "acme");
    await openTechySwitch(page, true);
    await pickScope(page, "company");
    assertEqual(
      await page.evaluate(() => window.__RICHOS_MOCK__.techyState()),
      { default: false, companies: { northwind: true }, threads: {} },
      "one company pinned, the global default untouched"
    );
    assert(await page.locator("#techy-chip").isVisible(), "the conversation he did it from is ON");
    assertEqual(await page.textContent("#techy-chip-label"), "Technical view · this company",
      "and the chip names the TIER holding it, so turning it off is a predictable act");

    await openThread(page, "hiring");
    assert(await page.locator("#techy-chip").isVisible(), "the sibling conversation in the same company is on too");
    assertEqual(await page.textContent("#techy-chip-label"), "Technical view · this company");

    await openThread(page, "partner");
    assert(await page.locator("#techy-chip").isHidden(), "and a conversation in ANOTHER company is untouched");
    assertEqual(await page.locator(".tl-tech").count(), 0, "it shows no technical row either");
    await page.close();
    return "northwind on, lumen untouched, the global default never moved";
  });

  await run.check("25. and later he can switch it off for a given company, or a given conversation", async () => {
    // His last sentence, which is the half a switch-on-only build would lose.
    const page = await openApp(browser);
    await page.evaluate(() => window.RichBridge.invoke("set_techy_default", { enabled: true }));
    await openThread(page, "acme");
    assert(await page.locator("#techy-chip").isVisible(), "everything is on to start with");

    // OFF for a given company, from the chip — the switch-off path inside a conversation. Round
    // 27: the chip opens Settings on the technical view's one switch, and turning that off asks
    // the same three.
    await page.click("#techy-chip");
    await page.waitForSelector("#set-menu", { state: "visible" });
    assertEqual(await page.evaluate(() => document.activeElement && document.activeElement.id), "set-techy",
      "the chip opens Settings on the switch");
    await page.uncheck("#set-techy");
    await page.waitForSelector("#techy-scope:not([hidden])");
    assert(await page.locator("#techy-scope").isVisible(), "the switch asks the same three");
    assertEqual(await page.textContent("#techy-scope-title"), "Turn off the technical view");
    assertEqual(await page.textContent("#techy-scope-confirm"), "Turn it off");
    assertEqual(await scopeChecked(page), "all-companies", "the same first choice, in both directions");
    await pickScope(page, "company");
    assertEqual(
      await page.evaluate(() => window.__RICHOS_MOCK__.techyState()),
      { default: true, companies: { northwind: false }, threads: {} },
      "this company is off UNDER an on global default"
    );
    assert(await page.locator("#techy-chip").isHidden(), "and the conversation he did it from went calm");
    await openThread(page, "partner");
    assert(await page.locator("#techy-chip").isVisible(), "the other company is still on");

    // OFF for a given conversation, one tier down, inside the company that is still on.
    await openTechySwitch(page, false);
    await pickScope(page, "thread");
    assertEqual(
      await page.evaluate(() => window.__RICHOS_MOCK__.techyState()),
      { default: true, companies: { northwind: false }, threads: { partner: false } },
      "one conversation off, its company and the global default where they were"
    );
    assert(await page.locator("#techy-chip").isHidden());
    await page.close();
    return "off for a company under an on default, then off for one conversation under both";
  });

  await run.check("26. the tier he picks clears the pins BELOW it on this conversation, and on no other", async () => {
    // THE DECISION IN `apply_techy_scope`, from the outside. He makes the choice while
    // looking at one conversation, so the choice has to be true on THAT conversation — and
    // every pin he made elsewhere has to survive it.
    const page = await openApp(browser);
    // A company pinned off last week, a conversation in it pinned off, and a bystander in
    // another company pinned on.
    await page.evaluate(() => {
      window.__RICHOS_MOCK__.setTechyCompany("northwind", false);
      return window.RichBridge.invoke("set_techy_mode", { threadId: "partner", enabled: true });
    });
    await openThread(page, "acme");
    await page.evaluate(() => window.RichBridge.invoke("set_techy_mode", { threadId: "acme", enabled: false }));
    await openThread(page, "acme");
    assert(await page.locator("#techy-chip").isHidden(), "pinned off at both tiers below the global one");

    await openTechySwitch(page, true);
    await pickScope(page, "all-companies");
    assertEqual(
      await page.evaluate(() => window.__RICHOS_MOCK__.techyState()),
      { default: true, companies: {}, threads: { partner: true } },
      "this path's two pins are cleared; the bystander in another company is untouched"
    );
    // THE POINT OF CLEARING THEM, and the reason this is not merely tidiness: without it the
    // switch he just moved would have changed nothing on the screen he moved it on.
    assert(await page.locator("#techy-chip").isVisible(), "the conversation he switched it on from is ON");
    assertEqual(await page.textContent("#techy-chip-label"), "Technical view · everywhere");
    await page.close();
    return "the path he stood on is cleared to his choice; every other pin survives";
  });

  await run.check("27. Cancel and Escape apply nothing, and leave every switch showing what the store took", async () => {
    // A checkbox flips ITSELF on the click that opens the sheet, so backing out has to
    // repaint from the model or a switch is left claiming a state nothing accepted.
    const page = await openApp(browser);
    await openThread(page, "acme");
    const before = await page.evaluate(() => window.__RICHOS_MOCK__.techyState());

    await openTechySwitch(page, true);
    await page.click("#techy-scope-cancel");
    await page.waitForSelector("#techy-scope", { state: "hidden" });
    await dismissPopover(page);
    assertEqual(await page.evaluate(() => window.__RICHOS_MOCK__.techyState()), before, "Cancel wrote nothing");
    assertEqual(await railSwitch(page), false, "and the switch is back where the store is");
    assert(await page.locator("#techy-chip").isHidden(), "and no chip appeared");

    // Escape is the CEO's rule (2026-09-17) and it CANCELS — it is never a silent yes to
    // the preselected option.
    await openTechySwitch(page, true);
    await page.keyboard.press("Escape");
    await page.waitForSelector("#techy-scope", { state: "hidden" });
    await dismissPopover(page);
    assertEqual(await page.evaluate(() => window.__RICHOS_MOCK__.techyState()), before, "Escape wrote nothing either");
    assertEqual(await railSwitch(page), false);
    await page.close();
    return "two ways out, neither of them an answer";
  });

  await run.check("28. with no conversation open there is no choice to make, and with no company there are two", async () => {
    // "Do not show a choice with one live option." On the opening screen "this company" and
    // "this conversation" have no referent, so the switch writes the only tier that means
    // anything. On a conversation with no company binding, two of the three are live and
    // the middle one is not offered — `apply_techy_scope` would refuse it.
    // `{ chosenEntity: null }` is the only boot with no conversation active — see `openApp`.
    // The door used here is the universal settings menu, which is the one piece of chrome on
    // EVERY screen and is exactly the entrance the CEO's "from Settings" names.
    const page = await openApp(browser, undefined, { chosenEntity: null });
    await page.click("#set-btn");
    await page.waitForSelector("#set-techy");
    await page.check("#set-techy");
    await bridgeQuiet(page, "the settings row with nothing open");
    assert(await page.locator("#techy-scope").isHidden(), "no sheet with nothing open");
    assertEqual(
      (await page.evaluate(() => window.__RICHOS_MOCK__.techyState())).default,
      true,
      "it went straight to the only tier that has a referent"
    );
    assertEqual(await page.isChecked("#set-techy"), true, "and the switch shows what was taken");
    await page.close();

    const page2 = await openApp(browser);
    await openThread(page2, "legacy"); // entity_id: null — written before entity scoping
    await openTechySwitch(page2, true);
    assert(await page2.locator("#techy-scope").isVisible());
    assertEqual(await scopeOptions(page2), [
      "For all conversations in all companies",
      "For this conversation only",
    ], "the company option is not offered for a conversation that is in no company");
    assertEqual(await scopeChecked(page2), "all-companies", "and his first choice is still the preselected one");
    await page2.click("#techy-scope-cancel");
    await page2.waitForSelector("#techy-scope", { state: "hidden" });
    await page2.close();
    return "opening screen: no sheet; a conversation with no company: two options, not a dead third";
  });

  await run.check("29. the keyboard shortcut ASKS WHERE, and Enter takes his first choice", async () => {
    // CORRECTED 2026-09-18, AND THE CORRECTION IS THE POINT OF THIS CHECK. It used to assert
    // the opposite — "the keyboard shortcut names its own scope, so it does not ask" — on
    // the argument that `#techy-hint` calls ⌘⇧T the control that "shows it for one
    // conversation only", so pressing it already chose option 3. That hint is a sentence
    // `main.js` writes. The CEO's sentence is the acceptance criterion and it has no
    // exception in it: "when the user toggles the techy mode on while being inside a
    // conversation thread, then there should be something like a radio button choice between
    // 3 choices ... And the first choice should be preselected". Pressing a shortcut is a
    // way of toggling it on, so the shortcut asks.
    //
    // THE COST IS ONE KEYSTROKE, which is what keeps it a shortcut: ⌘⇧T, Enter.
    const page = await openApp(browser);
    await openThread(page, "acme");
    const before = await page.evaluate(() => window.__RICHOS_MOCK__.techyState());

    await page.keyboard.press("Meta+Shift+T");
    await page.waitForSelector("#techy-scope:not([hidden])");
    assertEqual(await scopeOptions(page), [
      "For all conversations in all companies",
      "For all conversations in this company",
      "For this conversation only",
    ], "his three choices, in his order — the same sheet every other entrance opens");
    assertEqual(await scopeChecked(page), "all-companies", "with his first choice preselected");
    assertEqual(await page.textContent("#techy-scope-title"), "Turn on the technical view");
    assertEqual(
      await page.evaluate(() => window.__RICHOS_MOCK__.techyState()),
      before,
      "and NOTHING is written while the question is on screen — it asks instead of pinning"
    );
    assert(await page.locator("#techy-chip").isHidden(), "no chip yet either");

    // Enter, with focus where the sheet put it: on the preselected option.
    await page.keyboard.press("Enter");
    await page.waitForSelector("#techy-scope", { state: "hidden" });
    await bridgeQuiet(page, "Enter on the sheet");
    assertEqual(
      await page.evaluate(() => window.__RICHOS_MOCK__.techyState()),
      { default: true, companies: {}, threads: {} },
      "Enter took the preselected answer — his first choice, applied at the global tier"
    );
    assertEqual(await page.textContent("#techy-chip-label"), "Technical view · everywhere");

    // AND THE HINT NO LONGER PROMISES ONE CONVERSATION, because that promise was the whole
    // argument for the exception this check used to encode.
    assertEqual(
      await page.textContent("#techy-hint"),
      "On for all conversations in all companies. ⌘⇧T asks where to change it.",
      "the hint describes what the key actually does now"
    );
    await page.close();
    return "⌘⇧T opens the sheet, writes nothing until Enter, then takes option 1";
  });

  await run.check("31. ⌘⇧T on the opening screen still does nothing at all", async () => {
    // THE ONE THING THE CORRECTION HAD TO LEAVE ALONE. With no conversation open, two of the
    // three options have no referent, so `requestTechyToggle` — the entrance the rail's
    // switch and the settings row use — writes the GLOBAL DEFAULT directly. Wiring the
    // shortcut to that entrance would have been the obvious one-line change, and it would
    // have given ⌘⇧T a power it has never had: turning the technical view on for every
    // conversation, from a screen with no conversation on it, with no sheet and no undo.
    // This key has only ever meant something inside a conversation.
    const page = await openApp(browser, undefined, { chosenEntity: null });
    const before = await page.evaluate(() => window.__RICHOS_MOCK__.techyState());
    await page.keyboard.press("Meta+Shift+T");
    // Whatever the key set off has finished: a sheet opens in the key's own handler, and a
    // write would be a bridge call, which this waits out before the two assertions look.
    await bridgeQuiet(page, "the shortcut with nothing open");
    assert(await page.locator("#techy-scope").isHidden(), "no sheet, because there is no choice to make");
    assertEqual(
      await page.evaluate(() => window.__RICHOS_MOCK__.techyState()),
      before,
      "and nothing was written — in particular not the global default"
    );
    assertEqual(before.default, false, "(the boot state this is measured against)");
    await page.close();
    return "unchanged: the key is inert with no conversation open";
  });

  await run.check("30. the one switch always shows the conversation on screen", async () => {
    // Round 27: Technical view has ONE switch, in the Settings panel (the Preferences checkbox
    // that was its second door is gone, and so is the gear that opened it). It reads the
    // RESOLVED state rather than the global default, which is what makes it agree with the
    // WINDOW: a switch showing "on" over a conversation pinned off is one control disagreeing
    // with the screen it is drawn on.
    const page = await openApp(browser);
    assertEqual(await page.locator("#techy-default, #rail-settings, #assertiveness-popover").count(), 0,
      "no second door: the Preferences checkbox, the gear and its popover are gone");
    assertEqual(await page.locator("#set-techy").count(), 1, "one switch");
    await openThread(page, "acme");
    await pressToggle(page); // pin acme ON, global default still off
    assertEqual(await railSwitch(page), true, "the switch follows the conversation on screen");
    assertEqual(await settingsSwitch(page), true, "and reads the same with Settings open");

    await openThread(page, "hiring"); // unpinned, follows the off default
    assertEqual(await railSwitch(page), false);
    assertEqual(await settingsSwitch(page), false);

    // A COMPANY pin, the tier neither door could see before today.
    await openTechySwitch(page, true);
    await pickScope(page, "company");
    assertEqual(await railSwitch(page), true);
    assertEqual(await settingsSwitch(page), true);
    assertEqual(
      await page.textContent("#techy-hint"),
      "On for all conversations in this company. ⌘⇧T asks where to change it.",
      "and the hint names which of the three tiers is holding it, since the label no longer can"
    );
    await page.close();
    return "one switch, three tiers, one answer";
  });

  await run.check("32. every wait here is on the app, not the clock: the same presses at 300 ms of bridge latency", async () => {
    // Hunt part 2 recheck R41. The helpers above used to sleep a fixed 250 ms or 120 ms after
    // a press, which is the whole of a fast run's cost and too little on a slow one: opening a
    // thread is six awaited bridge calls and confirming the sheet is two. 300 ms on EVERY call
    // puts each round trip past those numbers, so a helper that still waited on the clock
    // returns early and the assertion after it reads the screen before the app has drawn it.
    // A helper that waits on the app (`bridgeQuiet`) is as right here as at zero.
    const LAG = 300;
    const page = await openApp(browser, undefined, undefined, LAG);
    await openThread(page, "acme");
    await assertOnThread(page, "acme", "acme, opened at 300 ms of bridge latency");
    await pressToggle(page);
    assertEqual(await page.textContent("#techy-chip-label"), "Technical view · this conversation",
      "the toggle's answer is on the chip");
    assert((await techRows(page)).length >= 6, "and the thread was re-read in place with its tool calls");
    assertEqual(await settingsSwitch(page), true, "the settings row shows what the store took");
    await openTechySwitch(page, false);
    await pickScope(page, "thread");
    assert(await page.locator("#techy-chip").isHidden(), "off again for this conversation");
    assertEqual(await page.locator(".tl-tech").count(), 0, "and the re-read drew the calm view");
    assertEqual(page.__errors, [], "no page errors");
    await page.close();

    // Check 28's opening-screen press, where the write is the only thing to wait for.
    const blank = await openApp(browser, undefined, { chosenEntity: null }, LAG);
    await blank.click("#set-btn");
    await blank.waitForSelector("#set-techy");
    await blank.check("#set-techy");
    await bridgeQuiet(blank, "the settings row with nothing open, at 300 ms");
    assertEqual((await blank.evaluate(() => window.__RICHOS_MOCK__.techyState())).default, true,
      "the only tier with a referent was written before the check looked");
    await blank.close();
    return "open, toggle on, settings, toggle off and the opening-screen write, all at 300 ms per call";
  });

  await browser.close();
  const failed = run.report();
  process.exitCode = failed ? 1 : 0;
}

main().catch((e) => {
  console.error(e);
  process.exit(1);
});

// =========================================================================================
// THE MUTATIONS — every check above was run RED once, by breaking the SHIPPED source
// =========================================================================================
//
//  1  main.js `renderTechyChip`: drop the `techyChipEl.hidden = !on` line  -> the chip is
//     visible in the calm view; check 1 fails on "no chip".
//  2  mock.js: change one word of `TECHY_UNREADABLE` -> check 2 fails against the Rust const.
//  3  machinery_view.rs: append " (kept for 14 days)" to NOTHING_RECORDED -> check 3 fails.
//  4  timeline.js `technicalLabel`: return `item.summary` first -> every row reads "Ran a
//     command"; check 4 fails on the missing full command.
//  5  timeline.js `ACTIVITY_STATE_LABEL.unknown = "done"` -> check 5 fails on the fold.
//  6  timeline.js `rollupActivity`: drop the `!item.detail` clause -> the three reads roll
//     up into one row; check 6 fails on `reads.length === 1`.
//  7  timeline.js `visible()`: return `item.visibility === "ceo"` -> the vendor and
//     permission rows vanish; check 7 fails. (This was a REAL defect, found by running it.)
//  8  main.js `toggleTechyThread`: pass `enabled: null` instead of `enabled: next` -> the
//     shortcut clears an override instead of setting one, so nothing ever turns on;
//     11 checks go red including 8. THAT FUNCTION NO LONGER EXISTS (2026-09-18: every
//     entrance goes through the sheet), and the equivalent mutation on the path that
//     replaced it is `confirmTechyScope`: send `scope` as `TECHY_SCOPE_PRESELECTED`
//     regardless of what is checked -> `pressToggle` writes the global default instead of a
//     pin and the same family of checks goes red, 8 first.
//  9  mock.js `techyModeOf`: ignore `techyThreads` -> the pin does not survive the global
//     switch; check 9 fails.
// 10  mock.js `set_techy_mode`: `techyThreads.set(id, false)` on the null arm -> the clear
//     becomes a pin-to-false; check 10 fails on `source: "thread"`.
// 11  timeline.js `applySnapshot`: `model.technical = snapshot.mode === "technical" ||
//     model.technical` -> the flag survives the toggle back off, so the calm view keeps its
//     technical rows and its expanded transcript; check 11 fails on the byte comparison,
//     and ONLY check 11 — which is what makes it the right mutation for this claim.
//     (The first attempt at this one — dropping `renderTechyState(null)` — did NOT go red,
//     because the thread under test HAS machinery and its state line is already hidden. It
//     is recorded here rather than quietly replaced.)
// 12  machinery_view.rs: return NOT_RETAINED for `NothingRecorded` -> check 12 fails on the
//     sentence, and the two states stop being distinguishable.
// 13  mock.js `machineryStateOf`: return the `nothing_recorded` shape for an unreadable
//     thread -> check 13 fails on both the sentence and the state. THIS IS THE ONE THE
//     BRIEF NAMED, and it was ALSO run against the Rust half: `journal.rs`'s
//     `read_thread_checked` mapping every read_dir error to `NothingRecorded` turns
//     `a_directory_the_os_refuses_is_unreadable_and_is_never_reported_as_empty` red and
//     takes two assertions of `examples/machinery_payload.rs` with it.
// 14  mock.js: give `mach_a5` a raw payload -> the evicted state never renders; check 14
//     fails on the sentence.
// 15  timeline.js `renderTechnicalRow`: append a `<button class="tl-tech-rerun">Re-run` ->
//     check 15 fails on both the class and the verb.
// 16  mock.js: add an activity with `detail.vendorKind = "agent_thought_chunk"` and the
//     title "thinking" -> check 16 fails twice.
// 17  mock.js `get_machinery`: drop the `rowCount` key -> check 17 fails on the envelope.
// 18  main.js `renderBetweenTurns`: end with `betweenTurnsEl.hidden = true` -> the section
//     never appears; checks 18 AND 19 fail.
// 19  main.js `renderBetweenTurns`: delete the `if (!rows.length && sentence)` branch -> an
//     empty lane draws an empty box under a heading; check 19 fails and ONLY check 19.
// 20  machinery_view.rs + mock.js: drop the `Unreadable => None` arm, so an unreadable store
//     also gets "nothing was recorded between turns" -> check 20 fails.
//     THE FIRST ATTEMPT AT THIS ONE DID NOT GO RED, and the reason was a real mock defect,
//     not a weak check: the mock computed the sentence from the raw lane fixture instead of
//     from the PROJECTION, so an unreadable thread still counted rows the renderer would
//     never receive. `machinery_view.rs` asks `view.between_turns()` — the gated lane — and
//     the mock now asks the same projection it returns. Recorded rather than quietly fixed.
// 21  timeline.rs `BetweenTurnItem`: add a `session_id` field and fill it from the record ->
//     `between_turn_thread_tests::a_lane_row_carries_no_session_id...` fails, and so does
//     `examples/machinery_payload.rs` (which exits 1). The mock-side half — adding
//     `sessionId` in `betweenTurnsFor` -> check 21 fails here too.
// 22  style.css `#stage-header`: `padding-right: calc(20px + var(--chrome-lane))` ->
//     `padding-right: 20px` (the shipped defect, dev-walk audit N3) -> check 22 fails:
//     "badge {...,\"right\":1004} intersects gear {\"left\":966,...}" at 1024x700, which is
//     the exact overlap the audit's screenshot showed.
//
// 29  main.js, the ⌘⇧T handler: `if (activeThreadId) openTechyScope(!techyOn())` reverted
//     to the 2026-09-18 morning behavior, `toggleTechyThread()` — which is literally the
//     previous commit (`6e5eb6e0`), so this one was run red against the shipped tree rather
//     than a hand edit. Check 29 fails at its first wait, "#techy-scope:not([hidden])"
//     timing out at 30s, because the key pins the conversation instead of asking. Removing
//     ONLY the keydown listener (Enter) leaves the same check red later instead, at the
//     `state: "hidden"` wait — the sheet stays open because nothing answers Enter, which is
//     the half of this check that no other check covers.
// 31  main.js, the same handler: `openTechyScope` -> `requestTechyToggle`, the obvious
//     one-line wiring -> on the opening screen the no-thread branch writes the global
//     default, and check 31 fails on "nothing was written — in particular not the global
//     default" with `default: true`. Checks 1-30 stay green under that mutation, which is
//     why 31 exists.
// 32  THE HARNESS, not the shipped source, because this check is about the helpers: the
//     waits in `openThread`, `pressToggle`, `pickScope`, `dismissPopover` and
//     `settingsSwitch` put back to their fixed 250 ms and 120 ms (hunt part 2 recheck R41)
//     -> check 32 fails at its first assertion and checks 1-31 stay green (measured
//     2026-09-30): "acme, opened at 300 ms of bridge latency is not on the thread it is named
//     for: ... the rail marks "acme", the model is bound to "acme", ... 0 turn(s) painted".
//     The fixed 250 ms returned after the rail and the binding and before the timeline was
//     drawn. With the waits on `bridgeQuiet`, all 32 pass.
//
// The whole sweep, with the run output and the two Rust-side mutations, is recorded at
// `docs/verification/techy-mode-2026-08-30/mutation-runs.txt`.

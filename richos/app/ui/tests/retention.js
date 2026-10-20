// THE RAW-RETENTION WINDOW AS A SETTING — techy-mode design §7.2, open-items 1.4.
//
// §7.2 IS THE CEO'S QUESTION AND NOTHING IN THIS SUITE ANSWERS IT. *"How long do raw
// payloads survive?"* stays open. What this branch changed is the COST of each of his
// answers: "14 days" was `RAW_RETENTION_DAYS` in `journal.rs` and "forever" was a
// developer's edit, and a question whose status quo is the only free answer is not really
// open. So the thing under test here is not a window — it is whether a person can change one.
//
// THE COMPLETION CRITERION IS OBSERVABLE, AND HALF OF IT IS NOT IN THIS FILE. "Set the window
// to three different values including forever, run eviction against a journal with entries of
// known ages, and show what survived" is a claim about eviction, and eviction is Rust. It is
// proven at `crates/richos-core/src/journal.rs`, by
// `the_survival_counts_at_four_windows` (6/6, 6/6, 3/6, 1/6 raw shards kept at forever,
// 90 days, 14 days and 0 days — and 6/6 RECORDS still rendering at every one of them) and by
// `the_shipping_default_is_exactly_the_two_constants_it_replaced`, which runs the old
// two-`u64` call and the new one over two identical journals and compares the survivors.
// Nothing here re-states those numbers: a browser suite asserting a count it got from a mock
// would prove the mock.
//
// WHAT IS HERE IS THE HALF A BROWSER CAN PROVE: that the control exists where a person would
// look, that the three choices on screen are exactly the three the Rust accepts, that the
// labels do not lie about the numbers behind them, that what is selected came from the store
// rather than from a local cache, and — the one that matters most —
//
//   THAT TIGHTENING THE WINDOW SAYS WHAT IT REMOVED.
//
// `evict_raw` is an `unlink`. Nothing else in this product would ever mention it. A CEO who
// tightens the window and is told nothing finds an empty row weeks later and cannot connect
// it to anything he did, and that is the failure this control is most likely to ship with,
// because it looks exactly like success. Check 4 is that sentence.
//
// EVERY CHECK WAS RUN RED ONCE by breaking the shipped source; the mutations are listed at
// the bottom of this file.
//
// Run: node retention.js   (or `npm test` for every suite in this directory)
//      RICHOS_PLAYWRIGHT=/path/to/node_modules/playwright node retention.js

"use strict";

const fs = require("fs");
const path = require("path");
const { leaveHome, loadPlaywright, shot, createRun, assert, assertEqual, SEED_THEME, UI_DIR } = require("./lib/harness");

const APP = "file://" + path.join(UI_DIR, "index.html");
const CORE = path.resolve(UI_DIR, "..", "crates", "richos-core", "src");
const CONFIG_RS = fs.readFileSync(path.join(CORE, "config.rs"), "utf8");
const JOURNAL_RS = fs.readFileSync(path.join(CORE, "journal.rs"), "utf8");
const MOCK_JS = fs.readFileSync(path.join(UI_DIR, "mock.js"), "utf8");
const SHOTS = "../shots-7-2";

/// The choices `RetentionChoice::parse` really accepts, read out of the Rust rather than
/// typed here. A choice added to the menu and not to the parser (or the reverse) reaches this
/// file as a failing check instead of as a radio that silently does nothing.
function rustChoices() {
  // Anchored on the IMPL, not on the signature: `Assertiveness::parse` in the same file has
  // a byte-identical signature and comes first, and the first version of this function read
  // that one and returned nothing — an EMPTY inventory, which the check below refuses.
  const impl = CONFIG_RS.indexOf("impl RetentionChoice {");
  assert(impl >= 0, "no `impl RetentionChoice` in config.rs");
  const body = CONFIG_RS.slice(impl);
  return [...body.matchAll(/"([a-z-]+)" => Some\(RetentionChoice::/g)].map((m) => m[1]).sort();
}

/// A `pub const NAME: u64 = <n>;` read off disk. The labels on screen say "two weeks" and
/// "three months"; these are the numbers those words are standing in for.
function rustConst(src, name) {
  const m = src.match(new RegExp("pub const " + name + ": u64 = ([0-9_]+)"));
  assert(m, "no such const: " + name);
  return Number(m[1].replace(/_/g, ""));
}

async function openApp(browser) {
  const page = await browser.newPage({ viewport: { width: 1400, height: 950 } });
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
  await page.goto(APP);
  // The home screen is the landing surface now; this suite is about the app UI behind it.
  await leaveHome(page);
  await page.waitForSelector(".nav-thread", { state: "attached" });
  page.__errors = errors;
  return page;
}

/// Open Settings (round 27: the one panel, from the button at top right). It re-reads the window on open, so this is also the trigger that
/// makes a config.json edited behind the app's back show up.
async function openSettings(page) {
  await page.click("#set-btn");
  await page.waitForSelector("#set-menu", { state: "visible" });
  await page.waitForTimeout(120);
}

async function closeSettings(page) {
  await page.click("#set-btn");
  await page.waitForTimeout(60);
}

/// Which retention radio is checked, or null when none is — which is a real state, not a bug.
async function selected(page) {
  return page.$eval("#set-menu", (pop) => {
    const on = pop.querySelector('input[name="raw-retention"]:checked');
    return on ? on.value : null;
  });
}

const hint = (page) => page.textContent("#retention-hint");

/// Wait until the popover's own state says `fact` (a function run in the page), with a hang
/// guard. WAITED FOR, NOT SLEPT FOR (2026-09-29, audit R10): every check below used to click a
/// radio, sleep 150-250ms and read the sentence, which is a bet that the store's round trip and
/// the re-render land inside that sleep. On expiry this returns and the assertion after it
/// names what the surface said instead.
async function surfaceSays(page, fact, arg) {
  await page.waitForFunction(fact, arg === undefined ? null : arg, { timeout: 10000 }).catch(() => {});
}
const hintHas = (page, text) =>
  surfaceSays(page, (t) => ((document.getElementById("retention-hint") || {}).textContent || "").includes(t), text);
const checkedIs = (page, value) =>
  surfaceSays(
    page,
    (v) => {
      const on = document.querySelector('#set-menu input[name="raw-retention"]:checked');
      return (on ? on.value : null) === v;
    },
    value
  );

async function main() {
  const { webkit } = loadPlaywright();
  const browser = await webkit.launch();
  const run = createRun("the raw-retention window as a setting (§7.2) — through the real shell");

  // ---- 1. the control is where a person would look -------------------------------------
  await run.check("1. the window is a control in Settings, not a constant in a source file", async () => {
    const page = await openApp(browser);
    // Nothing on the conversation surface: §7.3's rule for techy mode applies to its
    // retention too — the calm view gains no affordance, and this is one line in Settings.
    assertEqual(await page.isVisible("#retention-hint"), false, "nothing before Settings is opened");
    await openSettings(page);
    const labels = await page.$$eval('#set-menu input[name="raw-retention"]', (els) =>
      els.map((e) => ({ value: e.value, label: e.parentElement.textContent.trim(), enabled: !e.disabled }))
    );
    assertEqual(labels.length, 3, "three choices");
    assert(labels.every((l) => l.enabled), "and every one of them settable");
    assertEqual(
      labels.map((l) => l.label),
      ["For two weeks", "For three months", "Forever"],
      "the two ends of §7.2 and one point between them"
    );
    assert(await page.isVisible("#retention-hint"), "with a sentence saying what the choice means");
    await shot(page, SHOTS + "/7-2-01-the-window-is-a-setting");
    assertEqual(page.__errors, [], "no page errors");
    await page.close();
    return labels.map((l) => l.value).join(" / ");
  });

  // ---- 2. the three on screen are the three the Rust accepts ----------------------------
  await run.check("2. every choice on screen is a choice the backend takes, and vice versa", async () => {
    // A radio whose value `RetentionChoice::parse` does not accept is a control that looks
    // like it works and silently does nothing — the command refuses it, `invokeQuiet`
    // swallows the refusal, and the surface re-reads the unchanged store. Nothing on screen
    // would say so. Both sides are read off disk.
    const page = await openApp(browser);
    await openSettings(page);
    const onScreen = (await page.$$eval('#set-menu input[name="raw-retention"]', (els) =>
      els.map((e) => e.value)
    )).sort();
    const inRust = rustChoices();
    assert(inRust.length > 0, "EMPTY INVENTORY: no choices parsed out of config.rs");
    assertEqual(onScreen, inRust, "the radio values and RetentionChoice::parse");
    // And the harness's own model, so the checks below are not testing a third vocabulary.
    const inMock = [...MOCK_JS.slice(MOCK_JS.indexOf("const RETENTION_WINDOWS")).matchAll(
      /^\s{4}"?([a-z-]+)"?: \{ ageDays/gm
    )].map((m) => m[1]).sort();
    assertEqual(inMock, inRust, "mock.js's window table");
    // `custom` is a DESCRIPTION of a hand-edited file and never an instruction, so it must
    // not be on the menu at either end.
    assert(!inRust.includes("custom"), "custom is not parseable");
    assert(!onScreen.includes("custom"), "and not offered");
    await page.close();
    return inRust.join(" / ");
  });

  // ---- 3. the labels do not lie about the numbers behind them ---------------------------
  await run.check("3. `two weeks` is 14 days and `three months` is 90, read off the Rust", async () => {
    // The labels hide the arithmetic on purpose — asking a CEO to predict "90 days OR 2 GB,
    // whichever binds first" is asking him to hold the implementation in his head. Hiding it
    // is only honest if the words are true, and the words are in a different file from the
    // numbers.
    const days = rustConst(JOURNAL_RS, "RAW_RETENTION_DAYS");
    const months = rustConst(CONFIG_RS, "THREE_MONTHS_DAYS");
    assertEqual(days, 14, "'two weeks'");
    assertEqual(months, 90, "'three months'");
    const page = await openApp(browser);
    await openSettings(page);
    await page.click('input[name="raw-retention"][value="two-weeks"]');
    await hintHas(page, "Kept for " + days + " days");
    assert((await hint(page)).includes("Kept for " + days + " days"), "the sentence carries the real number");
    await page.click('input[name="raw-retention"][value="three-months"]');
    await hintHas(page, "Kept for " + months + " days");
    assert((await hint(page)).includes("Kept for " + months + " days"), "and so does the other one");
    await page.close();
    return days + " days / " + months + " days, from journal.rs and config.rs";
  });

  // ---- 4. THE ONE THAT MATTERS: a delete that says what it deleted ----------------------
  await run.check("4. tightening the window says what it removed, in the same breath", async () => {
    // `evict_raw` is an `unlink` and nothing else in this product would ever mention it. A
    // CEO who tightens the window and is told nothing finds an empty row weeks later and
    // cannot connect it to anything he did. The harness's store holds five raw day-shards at
    // 120/60/20/9/0 days old under a `forever` window; two weeks takes the first three.
    const page = await openApp(browser);
    await openSettings(page);
    assertEqual(await selected(page), "forever", "the store's answer, before he touches anything");

    await page.click('input[name="raw-retention"][value="two-weeks"]');
    await hintHas(page, "Removed the stored output");
    const said = await hint(page);
    assert(said.includes("Removed the stored output from 3 earlier days"), "it counted, out loud: " + said);
    assert(
      said.includes("The records are still there; their output is not."),
      "and it said which half went — Tier A is never evicted at any setting: " + said
    );
    await shot(page, SHOTS + "/7-2-02-a-delete-that-says-what-it-deleted");

    // A second, LOOSER change must not claim a removal it did not make — and must not
    // pretend the bytes came back either.
    await page.click('input[name="raw-retention"][value="forever"]');
    await hintHas(page, "Nothing is ever removed.");
    const after = await hint(page);
    assert(!after.includes("Removed"), "nothing removed, nothing claimed: " + after);
    assertEqual(after.includes("Nothing is ever removed."), true, "and the window reads as forever");
    assertEqual(page.__errors, [], "no page errors");
    await page.close();
    return "3 shards, counted on screen; the reverse change claims nothing";
  });

  // ---- 5. the cost of "forever" is on screen before he picks it -------------------------
  await run.check("5. the sentence states both limits and what the store costs today", async () => {
    // "Keep everything" is only a real choice if the person making it can see the bill.
    // `retainedBytes` comes off directory metadata — no payload is parsed to answer it.
    const page = await openApp(browser);
    await openSettings(page);
    const forever = await hint(page);
    assert(forever.startsWith("Nothing is ever removed."), "forever states itself plainly: " + forever);
    assert(/Using [\d.]+ MB now\./.test(forever), "and what it is costing: " + forever);

    await page.click('input[name="raw-retention"][value="three-months"]');
    await hintHas(page, "Kept for 90 days");
    const bounded = await hint(page);
    // BOTH axes. A day window and a byte ceiling are different limits and either can bind
    // first; a sentence naming only the one he picked would be a promise the other can break.
    assert(bounded.includes("Kept for 90 days"), "the axis he chose: " + bounded);
    assert(bounded.includes("GB of output"), "the axis he did not: " + bounded);
    assert(bounded.includes("whichever comes first"), "and which one wins: " + bounded);
    await page.close();
    return bounded;
  });

  // ---- 6. what is selected came from the store, never from a local guess ----------------
  await run.check("6. the store is the only answer — a click never becomes a second one", async () => {
    // The dial and the splash switch keep a `localStorage` mirror so something can paint
    // before the round trip resolves. This control governs a DELETE, and a local cache of a
    // delete setting is a second answer that can disagree with the store — after which the
    // popover is showing him a window that is not the one his machine will evict against.
    //
    // So: nothing is written locally, and when the store's answer and the last click
    // disagree, the STORE wins on the next open. (The disagreement is manufactured by
    // substituting the reply, the same way check 7 does; the renderer is the shipped one.)
    const page = await openApp(browser);
    await openSettings(page);
    await page.click('input[name="raw-retention"][value="three-months"]');
    await hintHas(page, "Kept for 90 days"); // the store's answer to the click has come back
    assertEqual(await selected(page), "three-months", "the click landed");
    const keys = await page.evaluate(() => Object.keys(window.localStorage).filter((k) => /retention/i.test(k)));
    assertEqual(keys, [], "and wrote nothing about retention to localStorage");

    await page.evaluate(() => {
      const real = window.RichBridge.invoke.bind(window.RichBridge);
      window.RichBridge.invoke = (cmd, args) =>
        cmd === "raw_retention"
          ? Promise.resolve({ choice: "two-weeks", ageDays: 14, totalBytes: 2147483648, retainedBytes: 4200000, evicted: 0 })
          : real(cmd, args);
    });
    await closeSettings(page);
    await openSettings(page);
    await checkedIs(page, "two-weeks");
    assertEqual(await selected(page), "two-weeks", "the store wins over the click, every time the popover opens");
    assert((await hint(page)).includes("Kept for 14 days"), "and the sentence came with it");
    assertEqual(page.__errors, [], "no page errors");
    await page.close();
    return "no local mirror; the popover re-reads on open and the store wins";
  });

  // ---- 7. a window nobody put on the menu describes itself ------------------------------
  await run.check("7. a hand-edited window is reported as itself, with no radio rounded on", async () => {
    // `config.json` is a file, and someone will edit it. Rounding a 45-day window onto the
    // nearest button would misreport his setting on the one screen whose job is to state it,
    // and the next click on ANY other control would write the rounded value back over his.
    // The backend answer is substituted here (the renderer under test is still the shipped
    // `main.js`; only the store's reply changes, which is all `mock.js` ever was).
    const page = await openApp(browser);
    await page.evaluate(() => {
      const real = window.RichBridge.invoke.bind(window.RichBridge);
      window.RichBridge.invoke = (cmd, args) =>
        cmd === "raw_retention"
          ? Promise.resolve({
              choice: "custom",
              ageDays: 45,
              totalBytes: "forever",
              retainedBytes: 65_800_000,
              evicted: 0,
            })
          : real(cmd, args);
    });
    await openSettings(page);
    assertEqual(await selected(page), null, "no radio is checked over a window none of them means");
    const said = await hint(page);
    assert(said.includes("Kept for 45 days"), "the window it actually is: " + said);
    assert(said.includes("No size limit."), "including the axis with no limit: " + said);
    assert(said.includes("Set by hand in config.json"), "and why nothing is selected: " + said);
    await shot(page, SHOTS + "/7-2-03-a-window-that-is-on-no-menu");
    await page.close();
    return said;
  });

  // ---- 8. a refused change leaves the surface on the store's answer ---------------------
  await run.check("8. a choice the backend refuses never leaves the radio claiming it", async () => {
    // The refusal arm is real — `set_raw_retention` rejects anything `RetentionChoice::parse`
    // does not accept, and writes and evicts NOTHING, because the failure mode of a bad
    // argument to a command that deletes has to be "did nothing". No path in the shipped UI
    // reaches it (check 2 is why), so it is driven here by putting a value on the radio that
    // the product never puts there.
    const page = await openApp(browser);
    await openSettings(page);
    const before = await selected(page);
    const hintBefore = await hint(page);
    await page.evaluate(() => {
      const input = document.querySelector('input[name="raw-retention"][value="three-months"]');
      input.value = "custom";
      input.checked = true;
      input.dispatchEvent(new Event("change", { bubbles: true }));
    });
    // Right after the dispatch the radio claims `custom`; going back is the fact waited for.
    await checkedIs(page, before);
    assertEqual(await selected(page), before, "the surface went back to what the store holds");
    assertEqual(await hint(page), hintBefore, "and claims no window it did not get");
    assertEqual(page.__errors, [], "a refusal is not a page error");
    await page.close();
    return "refused, re-read, nothing claimed";
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
// Recorded as run, including the two places a mutation did NOT redden the check I expected.
//
//  1  index.html: delete the three `input[name="raw-retention"]` labels -> checks 1, 2, 3,
//     4, 5, 6 and 8 fail. The setting stops being a setting, which is the whole item.
//     CHECK 7 SURVIVES IT, and that is worth stating rather than hiding: with no radios at
//     all `selected()` is legitimately null, and 7's other assertions are about the
//     sentence, which still renders. It is reddened by mutations 6 and 7 instead.
//  2  config.rs `RetentionChoice::parse`: drop the `"three-months"` arm -> check 2 alone.
//     The radio is still on screen and still LOOKS like it works: the command refuses it,
//     `invokeQuiet` swallows the refusal, and the surface re-reads the unchanged store. A
//     browser looking only at the DOM sees a control that worked, which is exactly the
//     shape this check exists for.
//  3  config.rs: `THREE_MONTHS_DAYS = 30` -> check 3 alone. The label still says three
//     months. The words and the number live in different files, which is why the join is
//     not optional.
//  4  main.js `renderRetention`: drop the `if (view.evicted > 0)` block -> check 4 alone.
//     The window still tightens, three days of stored output still go, and NOTHING says so.
//     One check, one claim — which is what makes it the right mutation for this one.
//  4b mock.js `set_raw_retention`: evict, then `return retentionView(0)` -> check 4 alone,
//     from the other side of the wire. The same silence, produced by the backend rather
//     than by the renderer.
//  5  main.js `retentionWindowSentence`: return only the age clause for the two-axis case
//     -> check 5 alone. A sentence naming only the limit he picked is a promise the other
//     limit can break.
//  6  main.js: drop `if (!open) syncRetentionFromBackend();` from the gear's click handler
//     -> checks 6 and 7. The popover keeps showing the last click and never notices the
//     store disagreeing, which is what a hand-edited config.json looks like.
//  7  main.js `renderRetention`: `input.checked = input.value === (view.choice === "custom"
//     ? "two-weeks" : view.choice)` — round a custom window onto the nearest button ->
//     check 7 alone.
//  8  mock.js `set_raw_retention`: coerce an unknown choice to `two-weeks` instead of
//     refusing it -> check 8 alone. A command that deletes must not guess at its argument.
//
// THE RUST HALF — the survival counts at four windows, the proof that the default
// reproduces yesterday's behaviour, and the keep-never-delete rule for an unreadable
// setting — is in `crates/richos-core/src/{journal,config}.rs`. Its eleven mutations are
// recorded in this branch's commit messages; the two that matter most are `from_json`'s
// unreadable arm returning `Of(0)` instead of `Forever` (an unreadable setting deletes
// everything) and `saturating_mul` becoming `*` (the `u64::MAX`-as-forever sentinel panics
// in debug and wraps to "delete everything" in release).

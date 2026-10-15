// FIRST-RUN PROVISIONING, AT THE SURFACE — the click the CEO makes, and everything it must
// and must not do.
//
// WHY THIS SUITE EXISTS. RichOS is installed, signed, and reaches the CEO's memory only
// because an engineer typed a symlink by hand. Measured on 2026-09-01, with the pointer
// removed and put straight back (`docs/verification/first-run-provisioning-2026-09-01/`):
// the app boots saying "no corpus configured", names three candidates, and nothing in the
// product creates any of them. The Rust half of the fix is proved by
// `cargo run --example first_run_demo` on a clean HOME and by 20 unit tests. This file is
// the other half: the question on screen, and the one press that answers it.
//
// WHAT IT HOLDS, and each is a clause of the contract rather than a nicety:
//
//   1. A fresh install ASKS, and asks THIS question first — one dialog at a time, with the
//      company question held back rather than stacked or dropped.
//   2. NO PATH IS SHOWN, AND THE BACKEND'S OFFER IS WHAT GETS SENT (product hunt 2026-10-15,
//      finding 15: the question showed a raw path ending in "RichOS/corpus"). The string the
//      command receives is compared byte for byte with `memory_status.offered_location`. It
//      is never a path he types, and the surface never composes a second opinion about where
//      his record goes.
//   3. A YES THAT WORKS CLOSES THE SHEET, with no second "That's set up" box to dismiss
//      (the same finding).
//   3. NO SILENT DEFAULT REACHES THE COMMAND. There is exactly one `provision_memory` call
//      site in the shipped source and it passes the offered location; a build that lost the
//      argument would send nothing, and the backend refuses that by name.
//   4. A REFUSAL REACHES THE SCREEN. `provision`'s messages are written for a human and
//      each names the thing to do, so they are rendered as they stand, never swallowed.
//   5. A CORPUS IT CANNOT READ IS ITS OWN STATE, with the party named and no button drawn
//      for a thing no button could do.
//   6. AN INSTALL THAT IS ALREADY SET UP IS NEVER ASKED.
//
// Run: node memory.js   (or `npm test` for every suite in this directory)

"use strict";

const fs = require("fs");
const path = require("path");
const { leaveHome, loadPlaywright, createRun, assert, assertEqual, UI_DIR } = require("./lib/harness");

const SOURCES = require("./lib/ui-sources");
const APP = "file://" + path.join(UI_DIR, "index.html");
const MAIN_JS = fs.readFileSync(path.join(UI_DIR, "main.js"), "utf8");

/// Open the shell with a preset, recording every `invoke` the page makes so the ARGUMENTS a
/// click produces can be read rather than inferred from what changed on screen.
async function openApp(browser, preset, resolveOverrides) {
  const page = await browser.newPage({ viewport: { width: 1400, height: 950 } });
  const errors = [];
  page.on("pageerror", (e) => errors.push(String(e)));
  page.on("console", (m) => {
    if (m.type() === "error") errors.push("console: " + m.text());
  });
  await page.addInitScript(() => {
    let real = null;
    window.__calls = [];
    window.__reject = {};
    window.__resolve = window.__resolve || {};
    Object.defineProperty(window, "RichBridge", {
      configurable: true,
      get() {
        return real;
      },
      set(v) {
        real = v;
        const origInvoke = v.invoke.bind(v);
        v.invoke = function (cmd, args) {
          window.__calls.push({ cmd, args });
          if (Object.prototype.hasOwnProperty.call(window.__reject, cmd))
            return Promise.reject(window.__reject[cmd]);
          if (Object.prototype.hasOwnProperty.call(window.__resolve, cmd))
            return Promise.resolve(window.__resolve[cmd]);
          return origInvoke(cmd, args);
        };
      },
    });
  });
  if (resolveOverrides) {
    await page.addInitScript((v) => {
      window.__resolve = v;
    }, resolveOverrides);
  }
  if (preset) {
    await page.addInitScript((v) => {
      window.__RICHOS_MOCK_PRESET__ = v;
    }, preset);
  }
  await page.goto(APP);
  // The home screen is the landing surface now; this suite is about the app UI behind it.
  await leaveHome(page);
  await page.waitForSelector(".nav-thread", { state: "attached" });
  page.__errors = errors;
  return page;
}

const dialogText = (page) =>
  page.evaluate(() =>
    (document.getElementById("memory-setup").innerText || "").replace(/\s+/g, " ").trim()
  );

/// Wait until the boot has got PAST the point that would ask the memory question: the shell has
/// read `memory_status`, and `take_work_notices` (called from `openThread`'s tail, after
/// `maybeAskAboutMemory` has had its turn) has run; then two of the page's own frames for the
/// sheet to be drawn if it was going to be. A NEGATIVE NEEDS SOMETHING TO WAIT FOR, and these
/// checks used to wait for 400ms of this process's time instead (2026-09-29, audit R10). The
/// 10s bound is a hang guard; the assertions after it name what went wrong.
async function pastTheQuestion(page) {
  await page
    .waitForFunction(
      () => {
        const calls = window.__calls || [];
        return calls.some((c) => c.cmd === "memory_status") && calls.some((c) => c.cmd === "take_work_notices");
      },
      null,
      { timeout: 10000 }
    )
    .catch(() => {});
  await page.evaluate(() => new Promise((r) => requestAnimationFrame(() => requestAnimationFrame(r))));
}

async function main() {
  const run = createRun("first-run provisioning — the question a fresh install asks, and the one click that answers it");
  const { webkit } = loadPlaywright();
  const browser = await webkit.launch();
  let assertions = 0;
  const bump = (n) => {
    assertions += n;
    return n;
  };

  await run.check("1  a fresh install asks, and asks this before the company question", async () => {
    const page = await openApp(browser, { memory: "none", chosenEntity: null });
    await page.waitForSelector("#memory-setup:not([hidden])");
    assert(await page.isVisible("#memory-setup"), "a machine with no memory must be asked");
    assert(
      await page.isHidden("#entity-picker"),
      "two modal dialogs at once — the company picker must be held back, not stacked"
    );
    // AND HELD BACK IS NOT DROPPED: dismissing this one asks that one.
    await page.click("#memory-setup-later");
    await page.waitForSelector("#entity-picker:not([hidden])");
    assert(await page.isHidden("#memory-setup"), "the memory dialog stays closed once dismissed");
    bump(4);
    assert(page.__errors.length === 0, "the shell logged errors: " + page.__errors.join(" | "));
    await page.close();
    return "asked first, one at a time, and the second question survives the first being answered";
  });

  await run.check("2  no path is shown, and the backend's offer is exactly what the command is given", async () => {
    // A LOCATION THE SURFACE COULD NOT HAVE GUESSED. The mock's own offer is
    // `/Users/you/RichOS/corpus`, which is also what a hard-coded path would say — so a
    // build that ignored the backend and wrote the path itself would pass against it. This
    // one comes from nowhere but the command's answer.
    const offered = "/Volumes/Archive/somewhere else/corpus";
    const page = await openApp(
      browser,
      { memory: "none" },
      { memory_status: { state: "none", root: null, source: null, compiler: null, tried: [], detail: null, offered_location: offered, provisioned_now: false } }
    );
    await page.waitForSelector("#memory-setup:not([hidden])");
    // FINDING 15 (product hunt 2026-10-15): the question showed a raw path ending in
    // "RichOS/corpus". A business owner is asked whether to keep his notes, not shown a path.
    const asked = await dialogText(page);
    assert(await page.isHidden("#memory-setup-location"), "the question shows a folder path: " + asked);
    assert(!/\/|corpus/i.test(asked), "the question carries a path or the word corpus: " + asked);
    // No text input anywhere in this dialog. His part is a choice, never a path he types.
    assertEqual(
      await page.$$eval("#memory-setup input, #memory-setup textarea", (n) => n.length),
      0,
      "the CEO must never be asked to type a path"
    );
    await page.click("#memory-setup-go");
    await page.waitForFunction(() =>
      window.__calls.some((c) => c.cmd === "provision_memory")
    );
    const call = await page.evaluate(() => window.__calls.find((c) => c.cmd === "provision_memory"));
    assertEqual(call.args.location, offered, "the string sent must be the backend's offer, byte for byte");
    bump(5);
    assert(page.__errors.length === 0, "the shell logged errors: " + page.__errors.join(" | "));
    await page.close();
    return "no path on the question, " + JSON.stringify(offered) + " sent byte-identical, and no field to type into";
  });

  await run.check("3  one press, and the sheet closes: no second box to close", async () => {
    // FINDING 15 (product hunt 2026-10-15): answering opened a second box, "That's set up",
    // that had to be closed. A yes that worked is done; the next first-run question, if one
    // was held back, is the next thing he sees.
    const page = await openApp(browser, { memory: "none", chosenEntity: null });
    await page.waitForSelector("#memory-setup:not([hidden])");
    await page.click("#memory-setup-go");
    await page.waitForFunction(() => document.getElementById("memory-setup").hidden, null, { timeout: 60000 });
    assert(await page.isHidden("#memory-setup-close"), "a Close button is still drawn");
    // AND THE QUESTION HELD BEHIND IT IS ASKED, not dropped.
    await page.waitForSelector("#entity-picker:not([hidden])", { timeout: 60000 });
    // ONE PRESS. Not two, not a wizard.
    const provisionCalls = await page.evaluate(
      () => window.__calls.filter((c) => c.cmd === "provision_memory").length
    );
    assertEqual(provisionCalls, 1, "his whole part is one click");
    bump(4);
    assert(page.__errors.length === 0, "the shell logged errors: " + page.__errors.join(" | "));
    await page.close();
    return "1 click -> 1 command -> the sheet is gone and the company question is up";
  });

  await run.check("4  a refusal reaches the screen, as it stands", async () => {
    // `provision`'s refusals are written for a human and each names the thing to do — "that
    // folder already exists and has things in it that are not a corpus". Paraphrasing one
    // would throw the instruction away; swallowing one is what `affordances.js` exists to
    // forbid.
    const refusal =
      "/Users/you/RichOS/corpus already exists and has things in it that are not a corpus. " +
      "Refusing to write a record into somebody else's folder.";
    const page = await openApp(browser, { memory: "none" });
    await page.waitForSelector("#memory-setup:not([hidden])");
    await page.evaluate((r) => {
      window.__reject.provision_memory = r;
    }, refusal);
    await page.click("#memory-setup-go");
    await page.waitForFunction(
      (r) => document.getElementById("memory-setup-note").textContent === r,
      refusal
    );
    assert(await page.isVisible("#memory-setup-go"), "a refusal must leave the control usable");
    assertEqual(await page.isDisabled("#memory-setup-go"), false, "and enabled, not stuck");
    bump(3);
    await page.close();
    return "the backend's own sentence, on screen, verbatim, with the button still live";
  });

  await run.check("5  a corpus it cannot read never sends him to fetch somebody else", async () => {
    // THE DEAD END THIS CLOSES (ray-opus-a1, 2026-09-04). The sentence used to end "It needs
    // whoever set RichOS up to add it". For Andreas, who installs RichOS in his lunch break,
    // that person is HIMSELF — so the product's headline promise ended on an instruction he
    // could not follow, with no button and no next step. The missing piece is the loro
    // compiler, which has no route onto any machine (no `loro/` file is tracked in this
    // repository, the bundle's Resources hold `icon.icns`, and the engine asset carries
    // `engine/`), so there is nobody to fetch and nothing to press. The sentence therefore
    // owes him three things and no imperative.
    const page = await openApp(browser, { memory: "none", memoryCompiler: false });
    await page.waitForSelector("#memory-setup:not([hidden])");
    await page.click("#memory-setup-go");
    await page.waitForSelector("#memory-setup-close:not([hidden])");
    const text = await dialogText(page);
    assert(
      !/whoever set RichOS up|whoever installed RichOS|an operator/i.test(text),
      "it must never send him to a third party who, on his own Mac, is him: " + text
    );
    // 1. WHAT DOES NOT WORK, said plainly and without implying the app is broken.
    assert(/can't read or write it yet/.test(text), "it must say what does not work: " + text);
    // 2. WHAT STILL DOES.
    assert(
      /Nothing else is affected/.test(text) && /conversations stay on this Mac/.test(text),
      "it must say what still works, or it reads as a broken app: " + text
    );
    // 3. WHAT HE CAN DO — which is nothing, said as such rather than left to be guessed at.
    assert(
      /nothing for you to install and nothing for you to fix/.test(text),
      "it must say what is being asked of him, even when the answer is nothing: " + text
    );
    assert(
      await page.isHidden("#memory-setup-go"),
      "there is nothing to set up — a button here would promise an action that does nothing"
    );
    assert(await page.isVisible("#memory-setup-close"), "and a way out");
    assert(
      (await page.textContent("#memory-setup-location")).trim().length > 0,
      "it names WHERE the memory is, which is the operator's first question"
    );
    bump(7);
    await page.close();
    return "no third party, and the three things he is owed: what is off, what is on, what is asked of him";
  });

  await run.check("5b a corpus it cannot read is said once, and never nagged about again", async () => {
    // THE OTHER HALF OF THE DEAD END (ray-opus-a1, finding 4, 2026-09-04): the sheet opened
    // on EVERY launch, not only the first. On a machine whose compiler ships from nowhere
    // that is every launch forever — a permanent interruption carrying a sentence that ends
    // "there's nothing for you to install and nothing for you to fix".
    const page = await openApp(browser, { memory: "no-compiler" });
    await pastTheQuestion(page);
    assert(
      await page.isHidden("#memory-setup"),
      "a launch that can do nothing about the state must not open a dialog about it"
    );
    // AND IT IS NOT SILENCE ABOUT A QUESTION HE HAS NOT ANSWERED: the state was still READ,
    // and a machine with NO corpus at all is still asked, because that one he can clear.
    const asked = await page.evaluate(() =>
      window.__calls.some((c) => c.cmd === "memory_status")
    );
    assert(asked, "the shell must still ask the backend rather than assuming the state");
    bump(2);
    await page.close();
    return "read at boot, silent on screen — the sentence belongs to the press that caused it";
  });

  await run.check('5c "Not now" is remembered across a relaunch, and the offer is still reachable', async () => {
    // **AUDIT-7 ROW 13.** Ray met the corpus sheet twice in one walk, over the opening screen
    // both times, and audit-6 met it before that. `maybeAskAboutMemory` had no record of a
    // decline, so on a machine with no memory folder it re-asked at every launch forever — the
    // same defect the `no-compiler` branch directly below it was fixed for on 2026-09-04 ("on a
    // provisioned machine with no compiler that is EVERY launch forever"), left in the branch
    // above it.
    //
    // THE RELAUNCH IS A RELOAD, and it is the right instrument rather than a convenience. What
    // is being tested is whether a FRESH `main.js` boot reads what the last one wrote, and a
    // reload gives exactly that: the whole shell re-parses and re-runs `init()` against the same
    // `localStorage`. (Two pages would not: Playwright gives each `browser.newPage()` its own
    // context, so they share no storage at all and the check would pass over a variable.)
    // `addInitScript` survives a reload, so the preset and the call recorder are still in front
    // of the second boot.
    const again = await openApp(browser, { memory: "none", chosenEntity: "northwind" });
    await again.waitForSelector("#memory-setup:not([hidden])");
    await again.click("#memory-setup-later");
    await again.waitForFunction(() => document.getElementById("memory-setup").hidden);
    const errors = again.__errors;

    await again.reload();
    await leaveHome(again);
    await again.waitForSelector(".nav-thread", { state: "attached" });
    // A NEGATIVE NEEDS SOMETHING TO WAIT FOR, so it waits for the boot to have got PAST the
    // point that would have asked: `take_work_notices` is called from `openThread`'s tail,
    // after `maybeAskAboutMemory` has had its turn.
    await again.waitForFunction(() => (window.__calls || []).some((c) => c.cmd === "memory_status"), null, { timeout: 30000 });
    await pastTheQuestion(again);
    assert(await again.isHidden("#memory-setup"), "the corpus question came back on the next launch after he said Not now");

    // AND THE OFFER IS NOT LOST. Going quiet without leaving a way in would trade a nag for a
    // dead end, which the affordance rule is right to refuse — so the settings menu carries the
    // row, and it opens this same sheet with the provisioning control still on it.
    await again.click("#set-btn");
    await again.waitForFunction(() => !document.getElementById("set-menu").hidden);
    const row = await again.evaluate(() => {
      const n = document.getElementById("set-memory-open");
      return n ? { text: (n.textContent || "").trim(), role: n.getAttribute("role") } : null;
    });
    assert(row, "the settings menu carries no way back to the memory folder");
    assertEqual(row.text, "Memory folder", "the row does not name the thing it opens");
    assertEqual(row.role, "menuitem");
    await again.click("#set-memory-open");
    await again.waitForSelector("#memory-setup:not([hidden])");
    assert(
      await again.isVisible("#memory-setup-go"),
      "the row opened the sheet with no way to set the folder up — the offer is reachable but not answerable"
    );
    const said = await dialogText(again);
    assert(said.includes("If this looks right, I'll set it up now."), "the sheet is not the offer: " + said);
    // Pressing the row is not another decline: `Close` and the scrim must not record one.
    await again.click("#memory-setup-later");
    bump(7);
    assertEqual(errors, [], "the relaunched shell logged errors");
    await again.close();
    return (
      '"Not now" survives the relaunch and the question is not re-asked; the settings menu\'s ' +
      '"Memory folder" row reopens the same offer with its own control on it'
    );
  });

  await run.check("6  an install that is already set up is never asked", async () => {
    const page = await openApp(browser, { memory: "ready" });
    await pastTheQuestion(page);
    assert(await page.isHidden("#memory-setup"), "a machine with memory must not be interrupted");
    const provisionCalls = await page.evaluate(
      () => window.__calls.filter((c) => c.cmd === "provision_memory").length
    );
    assertEqual(provisionCalls, 0, "nothing may provision without him answering a question");
    bump(2);
    await page.close();
    return "no dialog, and no command — his existing arrangement is left alone";
  });

  await run.check("7  there is exactly one call site, and it passes the location", async () => {
    // Structural, because the property is about the SHIPPED SOURCE rather than about one
    // run: a second call site is a second place a default could creep back in, which is the
    // whole thing `wiki/loro-structure.md` §"No silent default" removed from the compiler.
    //
    // AND "THE SHIPPED SOURCE" MEANT `main.js` UNTIL 2026-09-05. It was a `readFileSync` of
    // one file while the shell loads twelve, so the second call site this check exists to
    // refuse could have been written into `updates.js` or `home.js` and this would still
    // have printed "1 call site". `SOURCES.uiMatches` asks every file the manifest reaches
    // and takes no file argument, which is the only version of this that cannot narrow.
    const searched = SOURCES.stateSources();
    assert(searched.length >= 12, "the shipped-UI list is " + searched.length + " file(s) — that is not this tree");
    const sites = SOURCES.uiMatches(/invoke\(\s*"provision_memory"/);
    assertEqual(
      sites.length,
      1,
      "one door, or a default has somewhere to hide — found " + sites.length + " across " +
        searched.length + " shipped UI file(s): " + sites.map((s) => s.site).join(", ")
    );
    assertEqual(sites[0].file, "main.js", "the one call site moved to " + sites[0].site);
    assert(
      /const consented = memoryState\.offered_location;/.test(MAIN_JS),
      "the location the window showed must be held in one variable, not re-read"
    );
    assert(
      /invoke\("provision_memory",\s*\{\s*location:\s*consented\s*\}\)/.test(MAIN_JS),
      "the one call site must pass the backend's offer"
    );
    // AND THE SAME VARIABLE IS WHAT A FOLDER IT CANNOT READ RENDERS (the one result that still
    // draws a box). Two reads of `offered_location` would also happen to agree today and would
    // stop agreeing the moment the backend answered under another name — which is precisely
    // what it does.
    assert(
      /openMemorySetup\(MEMORY_NO_READER, consented,/.test(MAIN_JS),
      "the result screen must render the location he consented to, from the same variable"
    );
    // And nothing in the surface composes a path of its own — ACROSS THE WHOLE SHIPPED UI,
    // not just `main.js`. An absence claim scoped to one file of twelve is the weakest kind
    // of green there is: it passes by not looking.
    const paths = SOURCES.uiMatches(/["'`][^"'`]*\/RichOS\/corpus/);
    assertEqual(
      paths.map((p) => p.site),
      [],
      "a corpus path literal is in the shipped UI — the location comes from the backend or " +
        "not at all: " + paths.map((p) => p.site).join(", ")
    );
    bump(5);
    return (
      "1 call site (" + sites[0].site + "), passing `memoryState.offered_location`, and no path " +
      "literal anywhere in the " + searched.length + " shipped UI file(s)"
    );
  });

  await run.check("NEGATIVE CONTROL: this suite asserted a non-zero number of things", async () => {
    assert(
      assertions >= 20,
      "only " + assertions + " assertions ran. A suite that verifies little and reports green " +
        "is the failure this repository has caught three times."
    );
    return assertions + " assertions against the real DOM under WebKit";
  });

  await browser.close();
  const failed = run.report();
  console.log(
    failed
      ? "\n" + failed + " check(s) FAILED"
      : "\na machine with no memory asks once, takes one click, and never picks a location on its own."
  );
  process.exit(failed ? 1 : 0);
}

main().catch((e) => {
  console.error(e);
  process.exit(1);
});

// ---------------------------------------------------------------------------------------
// RUN RED — the mutation that makes each check fail, applied to the SHIPPED source
// ---------------------------------------------------------------------------------------
//
//  1   main.js `init`: call requireCompanyChoice() unconditionally
//        -> two modal dialogs at once on a fresh install
//  1b  main.js `closeMemorySetup`: drop the companyQuestionDeferred branch
//        -> the company question is asked never, and the composer stays blocked
//  2   main.js `provisionMemory`: invoke with { location: "~/RichOS/corpus" }
//        -> the string sent stops being the backend's offer
//  2c  main.js `maybeAskAboutMemory`: pass memoryState.offered_location to openMemorySetup
//        -> the question shows a raw path again (product hunt 2026-10-15, finding 15)
//  2b  index.html: add an <input> to the dialog
//        -> the CEO is asked to type a path
//  3   main.js `provisionMemory`: open the MEMORY_DONE box on success instead of closing
//        -> a second box he has to close (product hunt 2026-10-15, finding 15)
//  4   main.js `provisionMemory`: replace the catch body with console.error(e)
//        -> the refusal dies in a console the CEO does not have
//  5   main.js MEMORY_NO_READER: restore "It needs whoever set RichOS up to add it"
//        -> first run dead-ends on an instruction to fetch a person who, on a customer's Mac,
//           is the customer. Also turns affordances.js red: the registry row is INFORMATIONAL
//           and a sentence naming a party is not
//  5b  main.js `maybeAskAboutMemory`: restore the `no-compiler` branch that opens the dialog
//        -> a machine with a corpus and no compiler is interrupted on every launch forever
//  6   main.js `maybeAskAboutMemory`: return true for every state
//        -> an install that is already set up is interrupted on every launch
//  7   main.js: add a second provision_memory call site
//  7b  updates.js: `invoke("provision_memory", { location: "/Users/x/RichOS/corpus" })`
//        -> `one door, or a default has somewhere to hide — found 2 across 12 shipped UI
//           file(s): updates.js:832, main.js:3258`
//  7c  updates.js: `const __fallbackCorpus = "/Users/x/RichOS/corpus";` alone
//        -> `a corpus path literal is in the shipped UI ... ["updates.js:830"]`
//        Both are the widening, not the rule: until 2026-09-05 this check read `main.js`
//        and nothing else, so a second door or a path literal in any of the other eleven
//        shipped files was invisible to a check whose whole subject is that there is only
//        one of each.
//        -> a second door for a default to come back through

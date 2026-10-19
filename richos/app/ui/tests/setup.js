// FIRST-RUN SETUP, AT THE SURFACE — the one consent step, and everything it must and must
// not do.
//
// WHY THIS SUITE EXISTS. `ceo-decisions.md` §19: "today RichOS runs on his Mac and would not
// run on anyone else's". A customer needs Claude Code AND the engine directory, and the engine
// "ships in no payload and has no route onto another machine at all". The Rust half of the fix
// is proved by 33 tests in `crates/richos-core/tests/setup.rs` and by a fresh-install run on a
// clean HOME. This file is the other half: what he is asked, what he is told, and what happens
// when it goes wrong.
//
// WHAT IT HOLDS, and each is a clause of the contract rather than a nicety:
//
//   1. A CUSTOMER'S MAC ASKS, and asks THIS first — before the memory question and before the
//      company question, because without a binary and an engine there is nothing for a corpus
//      to be read by. One dialog at a time, and held back is not dropped.
//   2. NO TERMINAL, NO PATH, NO VERSION NUMBER anywhere on the sheet. Computed from the
//      rendered text, not from intent.
//   3. THE BYO-ANTHROPIC SENTENCE IS ON THE SHEET, ABOVE THE BUTTON. Row 3.14's second
//      condition: D removes one setup step of two and must not be sold as zero-touch.
//   4. A BUILD THAT CANNOT INSTALL EXPLAINS INSTEAD OF OFFERING A BUTTON THAT WILL FAIL.
//   5. A FAILURE REACHES THE SCREEN, verbatim, and the sheet stays usable — every SetupError's
//      sentence is written for him and names what to do.
//   6. PROGRESS IS LIVE. The events the backend emits are what moves the line; a sheet that
//      only rendered the return value would look hung for the minutes Anthropic's installer
//      takes.
//   7. A MACHINE THAT HAS EVERYTHING IS NEVER ASKED.
//   8. THE BUTTON CANNOT BE PRESSED TWICE. A second press mid-run would start a second
//      installer.
//   9. A QUESTION OWNS THE WINDOW UNTIL IT IS ANSWERED. `aria-modal="true"` is a promise that
//      everything outside the sheet is unavailable; case 20 measures that the window keeps it,
//      including for the one control that floats above every screen.
//
// Run: node setup.js   (or `npm test` for every suite in this directory)

"use strict";

const fs = require("fs");
const path = require("path");
const { leaveHome, loadPlaywright, createRun, assert, assertEqual, UI_DIR } = require("./lib/harness");

const SOURCES = require("./lib/ui-sources");
const { COUNT_BRIDGE, bridgeQuiet } = require("./lib/bridge-quiet");
const APP = "file://" + path.join(UI_DIR, "index.html");
const MAIN_JS = fs.readFileSync(path.join(UI_DIR, "main.js"), "utf8");

/// Open the shell with a preset, recording every `invoke` so the arguments a click produces
/// can be READ rather than inferred from what changed on screen.
async function openApp(browser, preset) {
  const page = await browser.newPage({ viewport: { width: 1400, height: 950 } });
  const errors = [];
  page.on("pageerror", (e) => errors.push(String(e)));
  page.on("console", (m) => {
    if (m.type() === "error") errors.push("console: " + m.text());
  });
  await page.addInitScript(() => {
    let real = null;
    window.__calls = [];
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
          return origInvoke(cmd, args);
        };
      },
    });
  });
  if (preset) {
    await page.addInitScript((v) => {
      window.__RICHOS_MOCK_PRESET__ = v;
    }, preset);
  }
  await page.goto(APP);
  await leaveHome(page);
  await page.waitForSelector(".nav-thread", { state: "attached" });
  page.__errors = errors;
  return page;
}

const sheetText = (page) =>
  page.evaluate(() =>
    (document.getElementById("setup-sheet").innerText || "").replace(/\s+/g, " ").trim()
  );

// ---------------------------------------------------------------------------------------
// HIS TWO SENTENCES AND THE COUNTER: THE FLOOR'S ONE DECLARED EXEMPTION (dictation plan
// revision 2, slice 4). Once dictation is there, the sheet carries the CEO's own setup line for
// the voice and video tools ("private/local", "$140+/year") and his download line
// ("$140+/year"), and the voice row counts "490 MB of 1.06 GB". Each would trip case 2's floor
// (a slash, a dollar sign, a version-shaped number), and each is meant to be read. So exactly
// those three are removed before the floor is checked:
//   - his two sentences, compared against the RUST CONSTANTS they are rendered from
//     (`setup.rs` MEDIA_TOOLS_WHY, `setup_view.rs` SETUP_DOWNLOAD_LINE), so a changed word is
//     not exempt; and each constant is asserted to be his text, word for word;
//   - the counter element's own text (`[data-counter]`), and only when it is a counter.
// Every other word on the sheet keeps the full floor; case 2b proves a path on another line
// still fails.
// ---------------------------------------------------------------------------------------

function rustConst(file, name) {
  const src = fs.readFileSync(path.join(UI_DIR, "..", file), "utf8");
  const m = src.match(new RegExp("pub const " + name + ': &str =\\s*"((?:[^"\\\\]|\\\\.)*)";'));
  if (!m) throw new Error(name + " is not a one-line string constant in " + file);
  return m[1]
    .replace(/\\u\{([0-9A-Fa-f]+)\}/g, (_, h) => String.fromCodePoint(parseInt(h, 16)))
    .replace(/\\(["\\])/g, "$1");
}
const HIS_TOOLS_LINE = rustConst("crates/richos-core/src/setup.rs", "MEDIA_TOOLS_WHY");
const HIS_DOWNLOAD_LINE = rustConst("src-tauri/src/setup_view.rs", "SETUP_DOWNLOAD_LINE");
const HIS_ACCOUNT_NOTE = rustConst("src-tauri/src/setup_view.rs", "SETUP_ACCOUNT_NOTE");
/// His account sentence (2026-10-08), word for word. It carries "Max 20x" and "Max/Max 20x".
const ROUND_ACCOUNT_NOTE =
  "You need your own Anthropic account and a Max subscription there. You can sign in through your browser after setup; I never see your password. If you don't already have that subscription, sign up there first and pick the Max/Max 20x tier.";
/// His setup sentence (2026-10-08) and round 19's download line, as he approved them.
const HIS_SETUP_SENTENCE =
  "My voice and video tools: the tools I use to watch and download videos for you. Plus, it gives you a free & private/local replacement for Wispr Flow. So, it saves you $140+/year👍 and allows you to talk instead of typing anywhere on this computer.";
const ROUND_19_DOWNLOAD_LINE =
  "Sit tight, we need to download about 1 GB of local voice AI so that you can just talk to Rich instead of typing. This will save you $140+/year👍 because you won't need Wispr Flow with this setup.";
const COUNTER_WORDS = /^(\d+ MB of (\d+\.\d\d GB|\d+ MB)|Checking…)$/;

/// Case 2's floor, as a list of what it found, so a fixture can prove it fails.
function floorViolations(text) {
  const out = [];
  if (/\//.test(text)) out.push("a path");
  if (/~/.test(text)) out.push("a home-relative path");
  if (/\$/.test(text)) out.push("a shell variable");
  if (/[Tt]erminal/.test(text)) out.push("the Terminal");
  if (/\d+\.\d+/.test(text)) out.push("a version number");
  return out;
}

/// The sheet's text with exactly the declared exemptions removed, and which were.
async function sheetTextExempt(page) {
  const { text, counters } = await page.evaluate(() => ({
    text: (document.getElementById("setup-sheet").innerText || "").replace(/\s+/g, " ").trim(),
    counters: [...document.querySelectorAll("#setup-sheet [data-counter]")]
      .filter((e) => e.getClientRects().length > 0)
      .map((e) => e.textContent.trim()),
  }));
  let rest = text;
  const removed = [];
  for (const s of [HIS_ACCOUNT_NOTE, HIS_TOOLS_LINE, HIS_DOWNLOAD_LINE]) {
    if (rest.includes(s)) {
      rest = rest.split(s).join(" ");
      removed.push(s);
    }
  }
  for (const c of counters) {
    assert(COUNTER_WORDS.test(c), "the exemption covers the counter's own words, not this: " + c);
    rest = rest.replace(c, " ");
    removed.push(c);
  }
  return { text, rest, removed };
}

/// Round 19's sheet: dictation there, Claude Code and the engine missing, the voice and video
/// tools downloading in the background from launch (a customer's first launch).
const R19 = { setup: "missing-both", dictation: true, videoTools: "downloading", memory: "ready" };

/// Two animation frames: whatever the last answer rendered is on screen.
const settledFrames = (page) =>
  page.evaluate(() => new Promise((r) => requestAnimationFrame(() => requestAnimationFrame(r))));

/// One background-download line as `setup_view::model_progress` emits it.
const toolsLine = (received, of, both, counter) => ({
  state: "started",
  component: "media-tools",
  what: "Getting my voice and video tools. " + Math.floor((100 * received) / of) + "%",
  index: 1,
  total: 1,
  received,
  of,
  counter,
  both_models: both,
});

async function main() {
  const run = createRun(
    "first-run setup — the consent step a customer's Mac gets, and what happens when it fails"
  );
  const { webkit } = loadPlaywright();
  const browser = await webkit.launch();
  let assertions = 0;
  const bump = (n) => {
    assertions += n;
    return n;
  };

  await run.check("0  the source-order check compares order, not just presence", async () => {
    const { setupAskedBeforeMemory } = require("./lib/source-order");
    assert(
      setupAskedBeforeMemory("const a = maybeAskAboutSetup();\nconst m = !a && maybeAskAboutMemory();"),
      "a setup-then-memory source was rejected"
    );
    assert(
      !setupAskedBeforeMemory("const m = maybeAskAboutMemory();\nconst a = maybeAskAboutSetup();"),
      "a memory-then-setup source was accepted"
    );
    return "a reversed order is refused";
  });

  await run.check("1  a customer's Mac asks, first, and one dialog at a time", async () => {
    const page = await openApp(browser, {
      setup: "missing-both",
      memory: "none",
      chosenEntity: null,
    });
    await page.waitForSelector("#setup-sheet:not([hidden])");
    assert(await page.isVisible("#setup-sheet"), "a machine missing both must be asked");
    assert(
      await page.isHidden("#memory-setup"),
      "the memory question must be HELD, not stacked on top of this one"
    );
    assert(
      await page.isHidden("#entity-picker"),
      "the company picker must be held back too — three modals at once is not a calm instrument"
    );
    // HELD BACK IS NOT DROPPED. Dismissing this one asks the next.
    await page.click("#setup-later");
    await page.waitForSelector("#memory-setup:not([hidden])");
    assert(await page.isHidden("#setup-sheet"), "the setup sheet stays closed once dismissed");
    // ...and that one hands off to the third.
    await page.click("#memory-setup-later");
    await page.waitForSelector("#entity-picker:not([hidden])");
    bump(5);
    assert(page.__errors.length === 0, "the shell logged errors: " + page.__errors.join(" | "));
    await page.close();
    return "asked first; the memory and company questions both survive being deferred";
  });

  // =======================================================================================
  // D5 — THE THIRD SHEET CAN BE DEFERRED TOO
  //
  // `docs/verification/2026-09-17-nightly-1.2.0-20260917.1-onscreen-audit.md` §D5: *"Sheets
  // 2 and 3 offer 'Not now'; sheet 4 does not, which is at least inconsistent and at most a
  // wall."* The one-at-a-time chain above was already right; this is the step with nothing
  // to press at the end of it.
  // =======================================================================================

  await run.check("1a the company question offers Not now, and says what it costs", async () => {
    const page = await openApp(browser, {
      setup: "missing-both",
      memory: "none",
      chosenEntity: null,
    });
    await page.waitForSelector("#setup-sheet:not([hidden])");
    await page.click("#setup-later");
    await page.waitForSelector("#memory-setup:not([hidden])");
    await page.click("#memory-setup-later");
    await page.waitForSelector("#entity-picker:not([hidden])");

    // THE CONTROL EXISTS AND IS PRESSABLE — the whole of D5.
    assert(await page.isVisible("#entity-picker-later"), "the third sheet still has no way out");
    assertEqual(await page.textContent("#entity-picker-later"), "Not now", "same verb as sheets 2 and 3");

    // AND IT SAYS WHAT DEFERRING COSTS, which is what the audit singled out the interview
    // offer for doing well. This cost is the opposite of that one's: the question comes
    // back, because he cannot type until it is answered.
    assert(await page.isVisible("#entity-picker-defer-note"), "the cost is not stated");
    const note = await page.textContent("#entity-picker-defer-note");
    assert(/message box/.test(note) && /until you pick one/.test(note), "the cost is vague: " + note);

    await page.click("#entity-picker-later");
    await page.waitForSelector("#entity-picker", { state: "hidden" });
    assert(await page.isHidden("#entity-picker"), "Not now did not close the sheet");

    // IT LEAVES NOTHING ARMED. Deferring must not become the §21 leak the modal guarded
    // against — the composer stays switched off, and the control that fixes it is on screen.
    assert(await page.isVisible("#composer-blocked"), "the composer was armed by a deferral");
    assert(
      await page.isVisible("#composer-choose-company"),
      "deferred with no way back — the control that answers the question must be on screen"
    );
    bump(6);
    assert(page.__errors.length === 0, "the shell logged errors: " + page.__errors.join(" | "));
    await page.close();
    return "Not now present, cost stated, composer still blocked with its control";
  });

  await run.check("1b NEGATIVE CONTROL: the per-thread picker grows no Not now", async () => {
    // The same picker, asked the OTHER question — which company one THREAD is for. That is
    // a choice among companies he has, the backdrop already dismisses it, and a deferral
    // control there would be a second way to do nothing. If this ever shows the button, the
    // check above has stopped proving anything about the first-run question specifically.
    const page = await openApp(browser, { setup: "ready", memory: "ready" });
    await page.waitForSelector("#rail-new-thread");
    await page.click("#rail-new-thread");
    await page.waitForSelector("#entity-picker:not([hidden])");
    assert(await page.isHidden("#entity-picker-later"), "the per-thread picker grew a deferral");
    assert(await page.isHidden("#entity-picker-defer-note"), "and a cost line for a cost it does not have");
    bump(2);
    await page.close();
    return "per-thread picker: no Not now, no cost line";
  });

  await run.check("2  no terminal, no path, no version number reaches his screen", async () => {
    const page = await openApp(browser, { setup: "missing-both" });
    await page.waitForSelector("#setup-sheet:not([hidden])");
    // His account sentence ("Max 20x", "Max/Max 20x") is the one declared exemption here, removed
    // by comparison with the Rust constant it is rendered from; everything else holds the floor.
    const { text, rest, removed } = await sheetTextExempt(page);
    assertEqual(removed.join(" | "), HIS_ACCOUNT_NOTE, "only his account sentence is exempt on this sheet");
    assert(text.length > 60, "the sheet said almost nothing: " + text);
    // COMPUTED FROM WHAT IS RENDERED. A path, a tilde, a shell prompt, a version number, or
    // the word Terminal each mean the non-technical constraint was lost somewhere between
    // the Rust and the DOM.
    assert(!/\//.test(rest), "a path reached his screen: " + rest);
    assert(!/~/.test(rest), "a home-relative path reached his screen: " + rest);
    assert(!/\$/.test(rest), "a shell variable reached his screen: " + rest);
    assert(!/[Tt]erminal/.test(rest), "the Terminal was mentioned: " + rest);
    assert(!/\d+\.\d+/.test(rest), "a version number reached his screen: " + rest);
    // AND THERE IS NO TEXT INPUT. His part is one press, not a path he types.
    const inputs = await page.evaluate(
      () => document.querySelectorAll("#setup-sheet input, #setup-sheet textarea").length
    );
    assertEqual(inputs, 0, "the setup sheet must never ask him to type anything");
    bump(7);
    await page.close();
    return "no path, no tilde, no shell, no version, no Terminal, no input field";
  });

  await run.check("3  the sheet says he still needs his own Anthropic account", async () => {
    const page = await openApp(browser, { setup: "missing-both" });
    await page.waitForSelector("#setup-sheet:not([hidden])");
    const note = (await page.textContent("#setup-account")).trim();
    assert(/Anthropic account/.test(note), "row 3.14's second condition is missing: " + note);
    assert(/sign in/i.test(note), "the sign-in he still has to do is not mentioned: " + note);
    assert(
      /never see your password/i.test(note),
      "the sheet must say RichOS never sees his password: " + note
    );
    assert(await page.isVisible("#setup-account"), "the caveat must be visible, not hidden");
    // ABOVE THE BUTTON, not a footnote after it. Compared by document position, because
    // "it is in the DOM" and "he reads it before deciding" are different claims.
    const above = await page.evaluate(() => {
      const a = document.getElementById("setup-account").getBoundingClientRect();
      const b = document.getElementById("setup-go").getBoundingClientRect();
      return a.bottom <= b.top;
    });
    assert(above, "the account caveat must sit ABOVE the button, not after it");
    bump(5);
    await page.close();
    return "BYO-Anthropic stated, in his words, above the button";
  });

  await run.check("3a the account sentence is his, and its last clause is an underlined link to the pricing page", async () => {
    const page = await openApp(browser, { setup: "missing-both" });
    await page.waitForSelector("#setup-sheet:not([hidden])");
    const LINK = "sign up there first and pick the Max/Max 20x tier";
    assertEqual((await page.textContent("#setup-account")).trim(), ROUND_ACCOUNT_NOTE, "the sheet's sentence is not his");
    const links = await page.evaluate(() => [...document.querySelectorAll("#setup-account a")].map((a) => ({
      text: a.textContent, href: a.getAttribute("href"), title: a.title, underline: getComputedStyle(a).textDecorationLine,
    })));
    assertEqual(JSON.stringify(links), JSON.stringify([{ text: LINK, href: "#", title: "claude.com/pricing", underline: "underline" }]), "exactly one underlined link, to the pricing page");
    await page.click("#setup-account a");
    const opened = await page.evaluate(() => window.__RICHOS_OPENED__ || []);
    assertEqual(JSON.stringify(opened), JSON.stringify(["claude.com/pricing"]), "the link asks the opener for the pricing page, once");
    assert(page.__errors.length === 0, "the shell logged errors: " + page.__errors.join(" | "));
    bump(4);
    await page.close();
    return "his sentence word for word; one underlined link; a click asks for claude.com/pricing";
  });

  await run.check("3b the setup sheet opens with focus on Set it up, not on the pricing link (D19)", async () => {
    const page = await openApp(browser, { setup: "missing-both" });
    await page.waitForSelector("#setup-sheet:not([hidden])");
    const active = await page.evaluate(() => (document.activeElement ? document.activeElement.id + "|" + document.activeElement.tagName : "none"));
    assertEqual(active, "setup-go|BUTTON", "focus must open on Set it up");
    const reachable = await page.evaluate(() => {
      const a = document.querySelector("#setup-account a");
      return !!a && a.tabIndex >= 0;
    });
    assert(reachable, "the pricing link must stay reachable by Tab");
    await page.close();
    return "first focus is Set it up; the link is still in the Tab order";
  });

  await run.check("4  each missing piece is named and explained, in his language", async () => {
    const page = await openApp(browser, { setup: "missing-both" });
    await page.waitForSelector("#setup-sheet:not([hidden])");
    const rows = await page.evaluate(() =>
      [...document.querySelectorAll("#setup-items li")].map((li) => ({
        name: li.querySelector(".setup-item-name").textContent.trim(),
        why: li.querySelector(".setup-item-why").textContent.trim(),
      }))
    );
    assertEqual(rows.length, 2, "a machine missing both must show two rows");
    assertEqual(rows[0].name, "Claude Code", "Claude Code must be named in plain text");
    assert(rows[0].why.length > 20, "a name with no explanation is a package list: " + rows[0].why);
    assert(/Anthropic/.test(rows[0].why), "who it comes from must be said: " + rows[0].why);
    assert(rows[1].why.length > 20, "the engine row explains nothing: " + rows[1].why);
    // ONE ROW ONLY when one thing is missing, and the title agrees with the count. The
    // two-piece page stays open so the singular and plural wordings are compared against each
    // other rather than each against a memory of the other.
    const page2 = page;
    const one = await openApp(browser, { setup: "missing-engine" });
    await one.waitForSelector("#setup-sheet:not([hidden])");
    const count = await one.evaluate(() => document.querySelectorAll("#setup-items li").length);
    assertEqual(count, 1, "only the engine is missing, so only one row");
    const title = (await one.textContent("#setup-title")).trim();
    assert(/one thing/.test(title), "the title must agree with the count: " + title);
    // AND SO MUST THE SENTENCE UNDER IT. Until 2026-09-04 the note was plural under both
    // titles, so this exact screen — the first a customer with Claude Code already installed
    // ever sees — read "There's one thing I need on this Mac. I can get them myself"
    // (ray-opus-a1, finding 7).
    const oneNote = (await one.textContent("#setup-note")).trim();
    assert(
      /I can get it myself/.test(oneNote) && !/get them myself/.test(oneNote),
      "one missing piece is an IT, not a THEM: " + JSON.stringify(title + " " + oneNote)
    );
    const bothNote = (await page2.textContent("#setup-note")).trim();
    assert(
      /I can get them myself/.test(bothNote),
      "and two pieces are still a THEM: " + JSON.stringify(bothNote)
    );
    await page2.close();
    bump(10);
    await one.close();
    return "two rows and a plural title and note, one row and a singular title and note";
  });

  await run.check("5  a build that cannot install EXPLAINS instead of offering a button", async () => {
    const page = await openApp(browser, { setup: "unpinned" });
    await page.waitForSelector("#setup-sheet:not([hidden])");
    assert(
      await page.isHidden("#setup-go"),
      "a button that will certainly fail must not be drawn"
    );
    assert(await page.isVisible("#setup-close"), "he must still be able to close the sheet");
    const why = (await page.textContent("#setup-error")).trim();
    assert(why.length > 40, "the reason must be a sentence, not a code: " + why);
    assert(
      /whoever set RichOS up/.test(why),
      "a state he cannot fix must name the party who can: " + why
    );
    bump(4);
    await page.close();
    return "explained, with the party named, and no button he could press in vain";
  });

  await run.check("6  a failure reaches the screen verbatim, and the sheet stays usable", async () => {
    const failure =
      "I couldn't reach the internet, so there's nothing to download yet. Connect and try " +
      "again — nothing has been changed on your Mac.";
    const page = await openApp(browser, { setup: "missing-both", setupFails: failure });
    await page.waitForSelector("#setup-sheet:not([hidden])");
    await page.click("#setup-go");
    await page.waitForSelector("#setup-error:not([hidden])");
    const shown = (await page.textContent("#setup-error")).trim();
    assertEqual(shown, failure, "the backend's sentence must be rendered as it stands");
    // A FAILURE IS NOT A DEAD END. He can try again, and the button says so.
    assert(await page.isVisible("#setup-go"), "he must be able to try again");
    assert(!(await page.evaluate(() => document.getElementById("setup-go").disabled)),
      "the retry button must be enabled again");
    assertEqual(
      (await page.textContent("#setup-go")).trim(),
      "Try again",
      "the button must say what pressing it does now"
    );
    assert(await page.isHidden("#setup-progress"), "a failed run must stop claiming progress");
    bump(5);
    assert(page.__errors.length === 0, "the shell logged errors: " + page.__errors.join(" | "));
    await page.close();
    return "the failure is his to read, and the sheet offers the next move";
  });

  await run.check("7  progress is driven by the backend's events, not by the return value", async () => {
    // THE EVENTS ARE THE SOURCE. A sheet that only rendered `run_setup`'s answer would sit
    // silent for the minutes Anthropic's installer takes — 197,220,928 B on a Mac with no
    // zstd (§19 finding 3).
    const page = await openApp(browser, { setup: "missing-both" });
    await page.waitForSelector("#setup-sheet:not([hidden])");
    // Record what the progress line says over the course of the run.
    await page.evaluate(() => {
      window.__progress = [];
      const el = document.getElementById("setup-progress");
      new MutationObserver(() => window.__progress.push(el.textContent)).observe(el, {
        characterData: true,
        childList: true,
        subtree: true,
      });
    });
    await page.click("#setup-go");
    await page.waitForSelector("#setup-close:not([hidden])");
    const seen = await page.evaluate(() => window.__progress);
    assert(seen.length >= 2, "the progress line never moved: " + JSON.stringify(seen));
    assert(
      seen.some((s) => /Claude Code/.test(s)),
      "the Claude Code step was never announced: " + JSON.stringify(seen)
    );
    assert(
      seen.some((s) => /instructions/.test(s)),
      "the engine step was never announced: " + JSON.stringify(seen)
    );
    // AND THE END IS THE BACKEND'S ANSWER, re-read from disk, not "no step threw".
    const done = (await page.textContent("#setup-note")).trim();
    assert(/software is installed/i.test(done), "the finished sheet must report installation: " + done);
    // AND THE HEADING AGREES WITH IT. It went on counting what was missing after the run, so
    // a successful install showed "There's one thing I need on this Mac." directly above
    // "That's everything. I'm ready." — two sentences contradicting each other on screen at
    // the same time (ray-opus-a1, finding 7, 2026-09-04).
    const heading = (await page.textContent("#setup-title")).trim();
    assert(
      !/I need on this Mac/.test(heading),
      "the heading is still asking for what the body just said it has: " +
        JSON.stringify(heading + " / " + done)
    );
    assert(
      /done/.test(heading),
      "the heading must say the state it is in: " + JSON.stringify(heading)
    );
    assert(await page.isHidden("#setup-go"), "a finished sheet must not offer to do it again");
    bump(8);
    await page.close();
    return "each step announced as it started; the ending came from the backend";
  });

  await run.check("17  the first button he ever presses does not move under his hand", async () => {
    // AUDIT-9 ROW 4. Ray, on candidate .9: the sheet "re-rendered under my cursor. It went from
    // 'setting up done / the software is installed' to 'setting up done / Your Anthropic account
    // is connected', and the `Close` button moved up ~46 px between the two. My click aimed at
    // Close landed on nothing and I had to click again. Minor, and it only happens once — but it
    // is the very first button he presses."
    //
    // THE MECHANISM, and it is why this check watches a POSITION rather than a string.
    // `runSetup` painted the done state and THEN awaited `provider_auth_status`. The mock's
    // default is `connected`, the same answer his Mac gives, and `renderProviderAuth` hides
    // `#provider-connect` on that answer — a control ABOVE
    // `#setup-close`. So the panel reached the screen at one height and shrank by their height a
    // round trip later.
    const page = await openApp(browser, { setup: "missing-both" });
    await page.waitForSelector("#setup-sheet:not([hidden])");
    // A ZERO-LATENCY BRIDGE CANNOT REPRODUCE A DEFECT THAT IS ABOUT THE ORDER OF TWO PAINTS,
    // and this check was written once without this and passed against the broken source — the
    // mock answers in a microtask, so no frame is painted between the done state and the account
    // answer, and the button appears never to move. A real Tauri IPC round trip is not free and
    // Ray measured a real one moving 46px. 150ms is about nine frames: enough for the browser to
    // paint the intermediate state if there is one, and nothing at all if there is not. Scoped
    // to the ONE command whose ordering is under test, and layered on the live object rather
    // than through `SLOW_BRIDGE`, because `openApp` above already owns `window.RichBridge`'s
    // property descriptor and a second `defineProperty` would take the call log with it.
    await page.evaluate(() => {
      const inv = window.RichBridge.invoke.bind(window.RichBridge);
      window.RichBridge.invoke = (cmd, args) =>
        cmd === "provider_auth_status"
          ? new Promise((r) => setTimeout(() => r(inv(cmd, args)), 150))
          : inv(cmd, args);
    });
    // EVERY FRAME FROM THE MOMENT IT IS VISIBLE, in the page. Sampling from the driver would
    // measure whenever the harness got its turn, which is the shape of check that goes green on
    // a fast machine and red on a runner (`README.md`, "Making this machine behave like a
    // runner").
    await page.evaluate(() => {
      window.__closeTops = [];
      const close = document.getElementById("setup-close");
      (function sample() {
        if (!close.hidden) window.__closeTops.push(Math.round(close.getBoundingClientRect().top));
        requestAnimationFrame(sample);
      })();
    });
    await page.click("#setup-go");
    await page.waitForSelector("#setup-close:not([hidden])");
    // The account answer, whenever it lands, has landed by now: its own effect is observable.
    await page.waitForFunction(
      () => /connected|sign in|Sign in|account/i.test(document.getElementById("setup-account").textContent || ""), null,
      { timeout: 10000 }
    );
    // A WINDOW OF THE PAGE'S OWN FRAMES after the answer landed, not 400ms of this process's
    // time (2026-09-29, audit R10): 24 more frames is the 400ms this used to sleep, at 60fps,
    // and on a busy host it is still 24 frames of the button being watched. Hang guard 20s.
    const seenAtAnswer = await page.evaluate(() => window.__closeTops.length);
    await page
      .waitForFunction((n) => window.__closeTops.length >= n + 24, seenAtAnswer, { timeout: 20000 })
      .catch(() => {});
    const tops = await page.evaluate(() => window.__closeTops);
    const distinct = [...new Set(tops)];
    assert(tops.length > 5, "the button was never sampled on screen: " + JSON.stringify(tops));
    assertEqual(
      distinct.length,
      1,
      "`Close` moved after it was on screen — it was at " + JSON.stringify(distinct) +
        " across " + tops.length + " frames, a travel of " +
        (Math.max(...distinct) - Math.min(...distinct)) + "px. A button a person is already " +
        "aiming at may not move."
    );
    // ...AND THE ACCOUNT SENTENCE WAS THERE FROM THAT FIRST FRAME, so nothing arrived late; the
    // sheet is not merely frozen, it is finished.
    assert(
      (await page.textContent("#setup-account")).trim().length > 0,
      "the account line is empty, so the sheet was painted before it had its answer after all"
    );
    assert(await page.isVisible("#setup-close"), "the way out is not on screen");
    bump(4);
    await page.close();
    return "`Close` held one position for " + tops.length + " frames at y=" + distinct[0] +
      ", with the account sentence already on it";
  });

  await run.check("8  a machine that has everything is never asked", async () => {
    const page = await openApp(browser, { setup: "ready" });
    // The status having been READ is the fact waited for, then two of the page's own frames for
    // a sheet to be drawn if it was going to be; this was a 300ms sleep (audit R10).
    await page.waitForFunction(() => (window.__calls || []).some((c) => c.cmd === "setup_status"), null, { timeout: 10000 }).catch(() => {});
    await page.evaluate(() => new Promise((r) => requestAnimationFrame(() => requestAnimationFrame(r))));
    assert(
      await page.isHidden("#setup-sheet"),
      "an install that is already set up must not be interrupted on every launch"
    );
    // And the status was still READ — the question is answered from disk, not skipped.
    const asked = await page.evaluate(() =>
      window.__calls.some((c) => c.cmd === "setup_status")
    );
    assert(asked, "the shell must ask the backend rather than assuming a set-up machine");
    bump(2);
    await page.close();
    return "read, and silent";
  });

  await run.check("9  the button cannot be pressed twice", async () => {
    const page = await openApp(browser, { setup: "missing-both" });
    await page.waitForSelector("#setup-sheet:not([hidden])");
    // Press, then immediately check the button is disabled — a second press mid-run would
    // start a second copy of Anthropic's installer.
    await page.evaluate(() => document.getElementById("setup-go").click());
    const disabled = await page.evaluate(() => document.getElementById("setup-go").disabled);
    assert(disabled, "the button must be disabled for the duration of the run");
    await page.waitForSelector("#setup-close:not([hidden])");
    const calls = await page.evaluate(
      () => window.__calls.filter((c) => c.cmd === "run_setup").length
    );
    assertEqual(calls, 1, "run_setup must be invoked exactly once per press");
    bump(2);
    await page.close();
    return "one press, one run";
  });

  await run.check("10  there is exactly one run_setup call site in the shipped source", async () => {
    // A SECOND DOOR IS A SECOND PLACE FOR THE GUARD TO BE MISSING. `memory.js` holds the
    // same rule over `provision_memory` for the same reason.
    //
    // AND UNTIL 2026-09-05 THIS CHECK WATCHED ONE DOOR OF TWELVE. It read `MAIN_JS` — a
    // single `readFileSync` of `main.js` — under a title saying "the shipped source". The
    // shell loads twelve `role: "ui"` files; a second `invoke("run_setup")` written into
    // `updates.js`, `home.js` or `settings-button.js` left this printing "one call site" in
    // green. `SOURCES.uiMatches` puts the question to every file the manifest reaches, with
    // comments stripped, and there is no file argument to narrow it back to one.
    const searched = SOURCES.stateSources();
    assert(searched.length >= 12, "the shipped-UI list is " + searched.length + " file(s) — that is not this tree");
    const found = SOURCES.uiMatches(/invoke\(\s*"run_setup"/);
    assertEqual(
      found.length,
      1,
      "run_setup is invoked from " + found.length + " place(s) across the " + searched.length +
        " shipped UI file(s): " + found.map((f) => f.site).join(", ")
    );
    assertEqual(found[0].file, "main.js", "the one call site moved to " + found[0].site);
    // ...and the setup question is asked before the memory one, in the source as well as on
    // screen, so the order is not an accident of two async calls racing.
    assert(
      require("./lib/source-order").setupAskedBeforeMemory(MAIN_JS),
      "main.js must ask the setup question first and gate the memory question on its answer"
    );
    bump(2);
    return (
      "one call site (" + found[0].site + ") across " + searched.length +
      " shipped UI file(s); the ordering is explicit in the source"
    );
  });

  // =======================================================================================
  // VOICE IS NOT OFFERED ON A MACHINE THAT CANNOT TRANSCRIBE
  // =======================================================================================
  //
  // Measured on published v1.0.0 by ray-opus-a1, 2026-09-04: the talk button asked for the
  // microphone, showed "listening…" with a level meter and lit the orange menu-bar
  // recording indicator for 25+ seconds. It never transcribed and never said it could not —
  // and the first-run greeting invited it: "You can type, or tap ◉ to talk to me."
  //
  // The shipping bundle carries no whisper binary and no model, so on a customer's Mac the
  // answer is always "no". This is that Mac.
  await run.check("12  a customer's Mac is not offered voice, and is not invited to it", async () => {
    const page = await openApp(browser, { setup: "missing-both", voice: "unavailable" });
    await page.waitForSelector("#setup-sheet:not([hidden])");
    // The window ASKED. A surface that decides this without asking is guessing.
    const asked = await page.evaluate(() =>
      window.__calls.some((c) => c.cmd === "voice_readiness")
    );
    assert(asked, "the window must ask voice_readiness before it decides");
    // NOT OFFERED. `[hidden]{display:none!important}` is what makes this real — the button
    // declares its own `display:flex`, which at equal specificity beats the UA rule.
    assert(
      await page.isHidden("#talk-toggle"),
      "the talk button must not be offered on a machine with no speech model"
    );
    // AND NOT INVITED. The greeting must not name a control that cannot work.
    await page.click("#setup-later");
    const greeting = await page.evaluate(() => {
      const n = document.querySelector("#messages .tl-prose");
      return n ? n.textContent : "";
    });
    assert(!/tap ◉/.test(greeting), "the greeting still invites voice: " + greeting);
    assert(!/talk to me/.test(greeting), "the greeting still invites voice: " + greeting);
    bump(4);
    assert(page.__errors.length === 0, "the shell logged errors: " + page.__errors.join(" | "));
    await page.close();
    return "voice_readiness asked; ◉ absent; the greeting names only the composer";
  });

  // The OTHER half, and it is the half that proves the first is a decision rather than a
  // deletion: a machine that can transcribe is still offered voice, and still invited.
  await run.check("13  a machine that can transcribe keeps the invitation", async () => {
    const page = await openApp(browser, { setup: "missing-both" });
    await page.waitForSelector("#setup-sheet:not([hidden])");
    assert(await page.isVisible("#talk-toggle"), "◉ must stay where voice can actually work");
    await page.click("#setup-later");
    const greeting = await page.evaluate(() => {
      const n = document.querySelector("#messages .tl-prose");
      return n ? n.textContent : "";
    });
    assert(/tap\s+to talk to me/.test(greeting) && !greeting.includes("◉"), "the invitation is missing or still shows the old glyph: " + greeting);
    // D17: the glyph's place is a small copy of the Talk-to-Rich button's icon, in its theme tokens.
    const inline = await page.evaluate(() => {
      const i = document.querySelector("#messages .tl-prose svg.talk-icon-inline");
      if (!i) return null;
      return {
        h: i.getBoundingClientRect().height,
        font: parseFloat(getComputedStyle(i.parentNode).fontSize),
        disc: getComputedStyle(i.querySelector(".talk-icon-disc")).fill,
        btnDisc: getComputedStyle(document.querySelector("#talk-toggle .talk-icon-disc")).fill,
      };
    });
    assert(inline, "the greeting shows no inline copy of the talk icon");
    assert(inline.h > inline.font * 0.9 && inline.h < inline.font * 1.5, "the inline icon is not sized to the text: " + JSON.stringify(inline));
    assertEqual(inline.disc, inline.btnDisc, "the inline icon must take the button's theme color");
    // ... and the voice row's footnote says the same thing the same way.
    const foot = await page.evaluate(() => {
      const f = document.querySelector(".voice-footnote");
      return { text: f.textContent, svg: !!f.querySelector("svg.talk-icon-inline") };
    });
    assert(foot.svg && !foot.text.includes("◉") && /tap\s+to end voice/.test(foot.text), "the voice footnote still shows the old glyph: " + JSON.stringify(foot));
    bump(2);
    assert(page.__errors.length === 0, "the shell logged errors: " + page.__errors.join(" | "));
    await page.close();
    return "◉ present and the greeting invites it";
  });

  await run.check("13b while voice is on, the icon in the sentences follows the button (D20) and has the button's name (D21)", async () => {
    const page = await openApp(browser, { setup: "missing-both" });
    await page.waitForSelector("#setup-sheet:not([hidden])");
    await page.click("#setup-later");
    const read = () =>
      page.evaluate(() => {
        const i = document.querySelector("#messages .tl-prose svg.talk-icon-inline");
        const f = document.querySelector(".voice-footnote svg.talk-icon-inline");
        const fill = (n, c) => getComputedStyle(n.querySelector(c)).fill;
        return {
          label: i.getAttribute("aria-label"), role: i.getAttribute("role"), hidden: i.getAttribute("aria-hidden"),
          disc: fill(i, ".talk-icon-disc"), bar: fill(i, ".talk-icon-bar"),
          fdisc: fill(f, ".talk-icon-disc"), flabel: f.getAttribute("aria-label"),
          bdisc: fill(document.querySelector("#talk-toggle"), ".talk-icon-disc"),
          bbar: fill(document.querySelector("#talk-toggle"), ".talk-icon-bar"),
          blabel: document.getElementById("talk-toggle").getAttribute("aria-label"),
        };
      });
    let r = await read();
    assertEqual(r.role, "img", "the inline icon needs a role");
    assert(r.hidden === null, "the inline icon must not be aria-hidden");
    assertEqual(r.label, r.blabel, "resting: the inline icon says the button's name");
    assertEqual(r.disc, r.bdisc, "resting: disc follows the button");
    // The renderer's own entry into voice mode; the preview cannot open a microphone.
    await page.evaluate(() => setTalkPressed(true));
    r = await read();
    assertEqual(r.disc, r.bdisc, "listening: the greeting icon's disc must follow the button: " + JSON.stringify(r));
    assertEqual(r.fdisc, r.bdisc, "listening: the voice row icon's disc must follow the button: " + JSON.stringify(r));
    assertEqual(r.bar, r.bbar, "listening: the bars must follow the button: " + JSON.stringify(r));
    assertEqual(r.label, "Stop talking", "listening: the name follows the button");
    assertEqual(r.flabel, "Stop talking", "listening: the voice row icon's name follows the button");
    await page.close();
    return "disc, bars and spoken name follow #talk-toggle at rest and listening";
  });

  await run.check("13c Tab on the setup sheet never leaves it (D22: every stop shows a ring)", async () => {
    const page = await openApp(browser, { setup: "missing-both" });
    await page.waitForSelector("#setup-sheet:not([hidden])");
    const seen = [];
    for (let k = 0; k < 7; k++) {
      await page.keyboard.press("Tab");
      seen.push(await page.evaluate(() => {
        const a = document.activeElement;
        return a && a.closest("#setup-sheet") ? a.id || a.tagName : "OUT:" + (a ? a.tagName : "none");
      }));
    }
    assert(!seen.some((x) => x.startsWith("OUT")), "Tab left the sheet: " + seen.join(" > "));
    await page.keyboard.press("Shift+Tab");
    const back = await page.evaluate(() => (document.activeElement.closest("#setup-sheet") ? "in" : "out"));
    assertEqual(back, "in", "Shift+Tab left the sheet");
    await page.close();
    return "seven Tabs and a Shift+Tab stayed on the sheet: " + seen.join(" > ");
  });

  // =======================================================================================
  // THE ENGINE STEP CANNOT BE GOT PAST BY ACCIDENT
  // =======================================================================================
  //
  // MEASURED ON PUBLISHED v1.0.1 (ray-opus-a2, 2026-09-04). A walk from a fresh state ended
  // with `~/Library/Application Support/RichOS/engine` holding zero files, the first-run
  // sheet ADVANCED to the corpus question, and four sends in a row refused with
  // LEASE_UNAVAILABLE_MESSAGE. The click that started it was aimed at "Set it up" and landed
  // in empty space below the panel.
  //
  // It landed on the backdrop. `#setup-sheet` is the full-screen overlay and the sheet the
  // customer sees is `.overlay-panel` inside it, and main.js closed the whole thing on
  // `e.target === setupSheetEl`. Re-measured here before the fix, on both presets: sheet
  // hidden=true, run_setup called=false, memory question showing=true.
  //
  // The bar is not "warn him". It is that a mis-aimed click, and a stranger's habit of
  // clicking beside a dialog to be rid of it, cannot produce an app that looks set up and is
  // not. So: the backdrop does nothing, the panel body does nothing, and the only ways out are
  // the two buttons that say what they do.
  //
  // ESCAPE CHANGED SIDES ON 2026-09-17, BY THE CEO'S OWN RULING, and this paragraph is where
  // the change is recorded rather than quietly made. Until that day this case asserted that
  // Escape was INERT here, on the reasoning that "nobody added it to the list" is not a
  // guarantee. His item 1 that morning: *"The user must always be able to close any popup of
  // any kind by simply tapping the escape key on the keyboard i.e. without having to click
  // anything"*. A sheet is a popup, so Escape closes this one too.
  //
  // WHAT THAT DOES NOT COST, and it is the reason the change is safe rather than a reversal of
  // 704b4596. This case was never about Escape; it is about an ACCIDENT — a click aimed at a
  // button that lands sixty pixels low. Escape is not a mis-aim, and it now does EXACTLY what
  // the named way out does, because `data-dismiss="control:#setup-later,#setup-close"` on the
  // element makes it press that button rather than hide the sheet behind its back. So:
  //
  //   * with "Not now" or "Close" on screen, Escape is that button — the same close, the same
  //     handoff to the corpus question, and `run_setup` still never called;
  //   * DURING THE INSTALL, when both buttons are hidden because dismissing mid-download is
  //     precisely what must not happen, Escape does nothing at all. That is asserted below and
  //     it is the half of the old invariant that actually mattered.
  await run.check("14  a mis-aimed click cannot skip the engine, and Escape is the named button", async () => {
    for (const preset of ["missing-engine", "missing-both"]) {
      const page = await openApp(browser, { setup: preset, memory: "none", chosenEntity: null });
      await page.waitForSelector("#setup-sheet:not([hidden])");
      const panel = await page.evaluate(() => {
        const r = document.querySelector("#setup-sheet .overlay-panel").getBoundingClientRect();
        return { top: r.top, bottom: r.bottom, left: r.left, right: r.right };
      });
      const midX = (panel.left + panel.right) / 2;
      const midY = (panel.top + panel.bottom) / 2;
      // The exact miss ray-opus-a2 made: aimed at the button, landed below the panel.
      const misses = [
        [midX, Math.min(panel.bottom + 60, 940)], // below
        [Math.max(panel.left - 80, 10), midY], // beside
        [midX, Math.max(panel.top - 60, 10)], // above
      ];
      for (const [x, y] of misses) {
        await page.mouse.click(x, y);
        await page.waitForTimeout(120);
        assert(
          await page.isVisible("#setup-sheet"),
          preset + ": a click at " + x + "," + y + " dismissed the engine step"
        );
        assert(
          await page.isHidden("#memory-setup"),
          preset + ": a backdrop click advanced to the corpus question with no engine installed"
        );
      }
      // A click inside the panel that hits no control does nothing either.
      await page.click("#setup-title");
      await page.waitForTimeout(120);
      assert(
        await page.isVisible("#setup-sheet"),
        preset + ": a click on the panel body dismissed the engine step"
      );
      // NOTHING WAS INSTALLED AND NOTHING WAS CLAIMED — five dismissal attempts, zero runs.
      const ran = await page.evaluate(() =>
        window.__calls.filter((c) => c.cmd === "run_setup").length
      );
      assertEqual(ran, 0, preset + ": run_setup should not have been reached by any of that");
      // THE NAMED WAY OUT STILL WORKS, and still hands off. Refusing the backdrop must not
      // turn the first screen a customer ever sees into a trap.
      await page.click("#setup-later");
      await page.waitForSelector("#memory-setup:not([hidden])");
      assert(await page.isHidden("#setup-sheet"), preset + ": \"Not now\" must still close it");
      bump(11);
      assert(page.__errors.length === 0, "the shell logged errors: " + page.__errors.join(" | "));
      await page.close();

      // ---- AND ESCAPE IS THAT SAME BUTTON, on a fresh copy of the same machine ----------
      //
      // CEO, 2026-09-17, item 1. Pressed from the COMPOSER, not from inside the panel: an
      // Escape that only works while focus is already in the popup is the defect this ruling
      // was given about, not a fix for it.
      const byKey = await openApp(browser, { setup: preset, memory: "none", chosenEntity: null });
      await byKey.waitForSelector("#setup-sheet:not([hidden])");
      await byKey.evaluate(() => {
        const input = document.getElementById("input");
        if (input) input.focus();
      });
      await byKey.keyboard.press("Escape");
      await byKey.waitForSelector("#memory-setup:not([hidden])");
      assert(
        await byKey.isHidden("#setup-sheet"),
        preset + ": Escape must close this sheet the way its own named control does"
      );
      // THE SAME CLOSE, NOT A SECOND ONE. Nothing was installed, and the question that was
      // held back is asked — which is `closeSetupSheet`'s handoff, reached through the button.
      assertEqual(
        await byKey.evaluate(() => window.__calls.filter((c) => c.cmd === "run_setup").length),
        0,
        preset + ": Escape must not have started an install"
      );
      bump(2);
      assert(byKey.__errors.length === 0, "the shell logged errors: " + byKey.__errors.join(" | "));
      await byKey.close();
    }

    // ---- MID-INSTALL, ESCAPE DOES NOTHING AT ALL ----------------------------------------
    //
    // This is the half of 704b4596 that matters, and it survives the CEO's ruling because of
    // the FORM of the declaration rather than an exception to it: `data-dismiss` names "Not
    // now" and "Close", `runSetup` hides both of them for the length of the run, and Escape
    // can only ever press a control that is on screen. So the sheet is undismissable exactly
    // while dismissing it would abandon a download, and nothing had to know that but the
    // buttons.
    //
    // HELD THERE BY A `run_setup` THAT NEVER ANSWERS, not by a sleep. The mock's run finishes
    // in about 60 ms, and a check that raced it would be measuring this machine.
    {
      const mid = await openApp(browser, { setup: "missing-engine", memory: "none", chosenEntity: null });
      await mid.waitForSelector("#setup-sheet:not([hidden])");
      await mid.evaluate(() => {
        const bridge = window.RichBridge;
        const real = bridge.invoke.bind(bridge);
        bridge.invoke = (cmd, args) =>
          cmd === "run_setup" ? new Promise(() => {}) : real(cmd, args);
      });
      await mid.click("#setup-go");
      await mid.waitForSelector("#setup-progress:not([hidden])");
      assert(await mid.isHidden("#setup-later"), "\"Not now\" must be off screen during the run");
      assert(await mid.isHidden("#setup-close"), "\"Close\" must be off screen during the run");
      await mid.evaluate(() => {
        const input = document.getElementById("input");
        if (input) input.focus();
      });
      await mid.keyboard.press("Escape");
      await mid.waitForTimeout(120);
      assert(
        await mid.isVisible("#setup-sheet"),
        "Escape dismissed the sheet mid-install, which is the download abandoned"
      );
      assert(
        await mid.isHidden("#memory-setup"),
        "Escape mid-install advanced to the corpus question with the engine half-installed"
      );
      bump(5);
      assert(mid.__errors.length === 0, "the shell logged errors: " + mid.__errors.join(" | "));
      await mid.close();
    }
    // AND IN THE SOURCE, so a later slice that re-adds the one-line convenience fails here
    // rather than on a customer's Mac.
    //
    // COMMENTS STRIPPED FIRST. The note above the (absent) listener quotes the line it
    // replaced, so a naive grep matches the explanation and calls it the defect. That
    // stripping used to be three lines here — `split("\n").filter(l => !l.startsWith("//"))`
    // — which is right about `//` and blind to a `/* */` block, and which read `main.js`
    // ALONE while calling itself a claim about the source. Both are now
    // `lib/ui-sources.js`'s job: one scanner that `run.js` self-tests on every run, over
    // every shipped UI file the manifest reaches.
    const reAdded = SOURCES.uiMatches(/setupSheetEl\)\s*closeSetupSheet/);
    assertEqual(
      reAdded.map((m) => m.site),
      [],
      "the setup sheet is closed on a backdrop click again, at: " + reAdded.map((m) => m.site).join(", ")
    );
    bump(1);
    return (
      "the backdrop and the panel body are inert and Escape is inert mid-install; the rest of " +
      "the time Escape is the named button and nothing else — and the backdrop re-add is " +
      "refused across all " + SOURCES.stateSources().length + " shipped UI file(s)"
    );
  });

  // =======================================================================================
  // A SEND WITH NO ENGINE SAYS SO, AND OFFERS THE INSTALL
  // =======================================================================================
  //
  // ray-opus-a2 sent four messages into a machine with no engine and got the same sentence
  // four times: "I'm not connected to my thinking right now... Quit RichOS and open it again
  // — that clears it most of the time." He had not restarted between them, and restarting
  // would not have helped: the next boot looks for the same absent engine, fails the same
  // attach, and says the same thing. The only instruction the product gave him was one that
  // could not work.
  //
  // That sentence is not wrong — it is written for the OTHER no-lease cause, the signed-out
  // one, which a restart does clear. The two causes were sharing it. `send_message` now asks
  // the disk which cause this is (main.rs) and the window puts the setting up back on screen,
  // so the sentence's promise is kept rather than claimed.
  //
  // NOTE THIS STATE IS STILL REACHABLE ON PURPOSE. "Not now" is a real answer. Case 14 stops
  // an ACCIDENT from skipping the engine; this case is what happens when he skips it
  // deliberately and then tries to send anyway.
  await run.check("15  a send with no engine names the engine and offers the install", async () => {
    const page = await openApp(browser, { setup: "missing-engine", memory: "ready" });
    await page.waitForSelector("#setup-sheet:not([hidden])");
    await page.click("#setup-later"); // he defers, which he is entitled to do
    await page.waitForSelector("#setup-sheet", { state: "hidden" });
    await page.fill("#input", "book the Acme call for Thursday");
    await page.click("#send");
    await page.waitForSelector("#setup-sheet:not([hidden])");

    // WHAT HE IS TOLD. Read off the screen, not off the source — and waited for, because
    // `scheduleRender` batches: the sheet is shown synchronously and the notice lands on the
    // next frame, so a read taken the instant the sheet appears catches the greeting.
    // BOTH LANES, and the second one is where this sentence lives now: a refusal is the app
    // talking about itself, and since 2026-09-17 a local notice renders as a status line
    // rather than as something Rich said (`timeline.js`'s `renderLocalNotice`, Ray's
    // candidate-.4 finding #8). The words are unchanged, so everything asserted below is too.
    await page.waitForFunction(() =>
      [...document.querySelectorAll("#messages .tl-prose, #messages .tl-notice-body")].some((n) =>
        /take that on yet/.test(n.textContent)
      )
    );
    const notice = await page.evaluate(() => {
      const rows = [...document.querySelectorAll("#messages .tl-prose, #messages .tl-notice-body")];
      const hit = rows.filter((n) => /take that on yet/.test(n.textContent));
      return hit.length ? hit[hit.length - 1].textContent : rows.map((n) => n.textContent).join(" | ");
    });
    assert(/RichOS engine/.test(notice), "the missing piece is not named: " + notice);
    assert(!/Quit RichOS/.test(notice), "it still tells him to restart: " + notice);
    assert(!/open it again/.test(notice), "it still tells him to restart: " + notice);
    assert(
      /nothing to quit and nothing to reopen/.test(notice),
      "it must close the door on the advice that cannot work: " + notice
    );
    assert(/Set it up/.test(notice), "it names no way forward: " + notice);

    // AND THE CONTROL IT NAMES IS ON SCREEN. A sentence that says "I've put the setting up
    // back on your screen" and does not is the same defect as "quit and reopen", one layer up.
    assert(await page.isVisible("#setup-go"), "the sheet must be back, with the button on it");
    assertEqual(
      (await page.textContent("#setup-go")).trim(),
      "Set it up",
      "the notice names this control by its label, so the label must be that"
    );

    // HIS WORDS ARE NOT LOST, and the tail does not contradict the notice — telling him to
    // press Send while a modal covers the composer would be an instruction he cannot follow.
    assertEqual(
      await page.inputValue("#input"),
      "book the Acme call for Thursday",
      "his words were swallowed"
    );
    assert(
      /when the setting up is done/.test(notice),
      "the tail must not send him to a control the sheet is covering: " + notice
    );

    // AND PRESSING IT WORKS FROM HERE. The offer is an offer, not a notice shaped like one.
    await page.click("#setup-go");
    await page.waitForSelector("#setup-close:not([hidden])");
    const done = (await page.textContent("#setup-note")).trim();
    assert(/software is installed/i.test(done), "the run started from the notice must finish: " + done);
    bump(10);
    assert(page.__errors.length === 0, "the shell logged errors: " + page.__errors.join(" | "));
    await page.close();
    return "named, offered, his words kept, and the install runs from the offer";
  });

  await run.check("16  the preview rehearses the product's own refusal, byte for byte", async () => {
    // The same join `affordances.js` makes on LEASE_UNAVAILABLE_MESSAGE, for the same reason:
    // a preview that teaches the CEO one sentence while the app says another is two products.
    const rust = fs.readFileSync(
      path.join(UI_DIR, "..", "src-tauri", "src", "setup_view.rs"),
      "utf8"
    );
    const mock = fs.readFileSync(path.join(UI_DIR, "mock.js"), "utf8");
    // Rust's `\` + newline + indent joins to nothing; JS's `" +` + newline + `"` likewise.
    const joinedMock = mock.replace(/"\s*\+\s*\n\s*"/g, "");
    let checked = 0;
    for (const name of ["SETUP_INCOMPLETE_ENGINE", "SETUP_INCOMPLETE_CLAUDE", "SETUP_INCOMPLETE_BOTH"]) {
      const m = rust.match(new RegExp("pub const " + name + ": &str =\\s*\"([\\s\\S]*?)\";"));
      assert(m, "the Rust const " + name + " is gone");
      const sentence = m[1].replace(/\\\n\s*/g, "");
      // A LITERAL THAT LOST ITS CONTINUATIONS SHIPS DOUBLE SPACES TO HIS SCREEN. This exact
      // mistake was made and caught by the Rust tests while writing this pass.
      assert(!/ {2}/.test(sentence), name + " carries a run of spaces: " + sentence);
      assert(
        joinedMock.indexOf(sentence) >= 0,
        "app/ui/mock.js rehearses a refusal the product no longer says: " + sentence
      );
      checked++;
    }
    assertEqual(checked, 3, "all three arms must be compared");
    bump(7);
    return "three sentences, one wording, no stray spaces";
  });

  // =======================================================================================
  // 18 — THE OFFER HAS TO REACH THE SCREEN, AND EVERY OTHER CHECK IN THIS FILE STEPPED PAST
  //      THE ONE SURFACE THAT WAS HIDING IT
  //
  // MEASURED ON THE REAL WINDOW, candidate .15 (v1.2.0-nightly.20260919.4, source dcebed09,
  // build 9375f30d), escalation `esc-20260919T152225Z-2e44d112`. Ray's scratch HOME carried an
  // engine installed from `3313945b26b7` against a build pinning `adece4c069e4`. The BACK END
  // was right about all of it — `app.log`, verbatim: *"first-run setup: the RichOS engine is
  // NOT installed — 3 place(s) looked"* and *"first-run setup: this build installs engine
  // 1.2.0."* — and across four live captures between 16:08 and 16:4x **no setup sheet ever
  // appeared**, at entry or afterwards. He typed a job into a Mac that could not answer it and
  // got "I lost my connection to the part of me that thinks" instead of the offer.
  //
  // `openApp` above — and `first-run-sheet.js`, and every other suite here — calls
  // `leaveHome()`, which is `RichHome.hide()` through the API. A person does not have that
  // function. He arrives on the HOME SCREEN, and measured under WebKit that screen is
  // `z-index: 150` over `.overlay`'s `60`, sets `inert` on `#app` (which is where
  // `#setup-sheet` lives), and pulls focus back to its own door. So the sheet `init()` opened
  // was in the DOM, `hidden === false`, un-clickable, un-focusable and behind the picture —
  // and `main.js`'s Escape machinery counted it as on screen and pressed its "Not now" for
  // him. `RichDismiss.open()` returned `["setup-sheet"]` with the home screen on top of it.
  //
  // So this check does what he does: it opens the app and presses the door. Nothing else in
  // this file would have caught it, because nothing else in this file ever saw the home screen.
  //
  // ---------------------------------------------------------------------------------------
  // AND THEN IT WENT GREEN OVER A DEFECT THAT WAS STILL THERE — candidate .16, Ray again
  // (`esc-20260919T171553Z-e42d1166`): the offer on screen and `AXFocusedUIElement` reading
  // `text area Message to Rich`. This check asserted focus and passed. TWO REASONS, and in
  // both of them the harness boots in an order the app never boots in:
  //
  //  1. THE MOCK BRIDGE COSTS NOTHING. `mock.js`'s `invoke` is an `async` function over data
  //     already in memory, so `init()`'s whole await chain drains on already-resolved promises
  //     and FINISHES BEFORE THE PARSER DOES. `home.js` installs its give-way in `afterShell`,
  //     at `DOMContentLoaded`. Measured under WebKit with a focus-tracing shim, mock timings:
  //
  //       t=151ms  main.js:7655  openSetupSheet -> focus #setup-go   (swallowed)
  //       t=168ms  main.js:7721  init tail      -> focus #input      (swallowed)
  //       t=177ms  home.js:1727  afterShell     -> DOMContentLoaded
  //       t=179ms  home.js:915   give-way       -> focus #setup-go   LANDED, and LAST
  //
  //     With the bridge costing one task per command — which is what a Tauri IPC round trip
  //     is, and the floor under every real one — the same four lines arrive in the opposite
  //     order and the last one is the one that wins:
  //
  //       t=145ms  DOMContentLoaded
  //       t=244ms  main.js:7655  openSetupSheet -> focus #setup-go   (swallowed, #app inert)
  //       t=252ms  home.js:915   give-way       -> focus #setup-go   LANDED
  //       t=304ms  main.js:7721  init tail      -> focus #input      LANDED, and LAST
  //
  //     So the shim below is not a slower harness; it is the ONLY one of the two orders the
  //     product can actually boot in. A suite that boots in the other one measures a build
  //     nobody runs.
  //
  //  2. IT MEASURED BEFORE THE BOOT HAD FINISHED. Waiting for `#home` to go hidden is waiting
  //     for the give-way, which happens 52 ms BEFORE `init()`'s last focus call. The wait is
  //     now `RichSplash.state.reason === "app-ready"` — `main.js` yields the curtain two lines
  //     below that focus call, so the marker is the product's own word that `init()` is past
  //     it. The curtain is left to the app for the same reason: a person does not call
  //     `yieldNow`, and hand-yielding it threw away the app's own end-of-boot signal.
  //
  // AND THE ESCAPE HALF OF THIS CHECK COULD NOT FAIL AT ALL. It asserted
  // `!homeOpen || !sheetHidden` three lines after waiting for `#home` to be hidden, so
  // `homeOpen` was false by construction and the disjunction was true whatever Escape did —
  // a check shaped like a check. What it should have held is below.
  // ---------------------------------------------------------------------------------------
  // =======================================================================================

  await run.check("18  the offer reaches the SCREEN, not just the DOM, on the way a person arrives", async () => {
    const page = await browser.newPage({ viewport: { width: 1400, height: 950 } });
    const errors = [];
    page.on("pageerror", (e) => errors.push(String(e)));
    page.on("console", (m) => {
      if (m.type() === "error") errors.push("console: " + m.text());
    });
    await page.addInitScript((v) => {
      window.__RICHOS_MOCK_PRESET__ = v;
    }, { setup: "missing-engine" });
    // EVERY COMMAND COSTS A TASK, because every command is an IPC round trip. See the long
    // note above: without this the parser finishes AFTER `init()` does, which is an ordering
    // no shipped build has. Installed by intercepting the assignment `mock.js` makes, so the
    // mock itself is untouched and the renderer under test is the shipped one.
    await page.addInitScript(() => {
      let real = null;
      Object.defineProperty(window, "RichBridge", {
        configurable: true,
        get() {
          return real;
        },
        set(v) {
          real = v;
          if (v && typeof v.invoke === "function" && !v.__ipcCost) {
            const inner = v.invoke.bind(v);
            v.invoke = (...a) =>
              new Promise((resolve, reject) =>
                setTimeout(() => {
                  try {
                    inner(...a).then(resolve, reject);
                  } catch (e) {
                    reject(e);
                  }
                }, 4)
              );
            v.__ipcCost = true;
          }
        },
      });
    });
    await page.goto(APP);
    await page.waitForSelector("#setup-sheet:not([hidden])", { timeout: 15000 });
    // The screen gets out of the way over its own 200 ms fade, so the measurement waits for
    // the end state rather than for a clock. A screen that never leaves fails HERE, with the
    // reason in the timeout, rather than being read as a paint that lost a race.
    await page.waitForFunction(
      () => { const h = document.getElementById("home"); return !h || h.hidden; }, null,
      { timeout: 5000 }
    );
    // ...AND THE BOOT IS FINISHED, said by the product rather than by a clock in this file.
    // `main.js` yields the curtain with `app-ready` two lines below its last focus call, so
    // the curtain LEAVING is that call having been made. The give-way's own signal is not
    // enough: it fires 52 ms too early, which is the whole of reason 2 above.
    //
    // THE REASON IS ASSERTED, because `app-ready` inside §62's three-second minimum is
    // deferred and comes back as `held` — and `ceiling` is the OTHER automatic caller, the
    // failsafe for a boot that hung. A curtain that left on `ceiling` means `init()` never
    // reached its end, and this check would then be measuring a boot that did not happen.
    await page.waitForSelector(".splash", { state: "detached", timeout: 15000 });
    const yielded = await page.evaluate(() => window.RichSplash.state.reason);
    assert(
      yielded === "held" || yielded === "app-ready",
      `the curtain left on "${yielded}" — the boot did not reach its own end, so nothing below is a measurement of it`
    );

    // THE LAYERS ARE STATED, so a future restyle that merely renumbers them is visible here
    // rather than quietly making this check true for a different reason.
    const painted = await page.evaluate(() => {
      const sheet = document.getElementById("setup-sheet");
      const panel = sheet.querySelector(".overlay-panel");
      const r = panel.getBoundingClientRect();
      const top = document.elementFromPoint(r.x + r.width / 2, r.y + r.height / 2);
      const home = document.getElementById("home");
      return {
        homeOpen: !!(window.RichHome && window.RichHome.isOpen()),
        homeZ: home ? getComputedStyle(home).zIndex : null,
        sheetZ: getComputedStyle(sheet).zIndex,
        appInert: document.getElementById("app").hasAttribute("inert"),
        onTop: top ? top.id || top.className || top.tagName : null,
        insideSheet: !!(top && top.closest && top.closest("#setup-sheet")),
        focusInSheet: !!(document.activeElement && document.activeElement.closest && document.activeElement.closest("#setup-sheet")),
        focusId: (document.activeElement && document.activeElement.id) || null,
        title: (document.getElementById("setup-title").textContent || "").trim(),
      };
    });
    assert(
      painted.insideSheet,
      `the offer is not painted at the middle of its own panel — ${painted.onTop} is (home z-index ${painted.homeZ}, sheet z-index ${painted.sheetZ}, #app inert ${painted.appInert})`
    );
    assert(
      !painted.appInert,
      "#app is inert, so the one button that puts an engine on this Mac cannot be pressed"
    );
    assert(
      painted.focusInSheet,
      `the offer is on screen but focus is on ${JSON.stringify(painted.focusId)} — it cannot be answered from the keyboard`
    );
    assertEqual(
      painted.title,
      "There's one thing I need on this Mac.",
      "the sheet on screen is not the engine offer"
    );

    // AND ESCAPE IS THE BUTTON HE CAN SEE, PRESSED BY A HAND THAT IS ON IT.
    //
    // The assertion this replaces was `!homeOpen || !sheetHidden`, written three lines under a
    // wait for `#home` to be hidden — `homeOpen` was false by construction, so it held whatever
    // Escape did. It pinned nothing, and it is why the defect above shipped with this check
    // green beside it.
    //
    // WHAT ACTUALLY HAS TO BE TRUE is one clause, and it is the clause the assertions above
    // have just established: the CEO's keyboard is ON the offer, so Escape means the sheet's
    // own named way out — `#setup-later`, "Not now", the button under his eyes — and not a key
    // that answered a question standing behind his hand. That is `setup.js` case 14's
    // equivalence, and what case 14 could not see is WHERE the hand was. So this measures the
    // click rather than the disappearance: a sheet that went away for some other reason is not
    // the same event as the named button being pressed, and only one of the two is consent.
    const escape = await page.evaluate(() => {
      window.__escapeClicked = null;
      for (const id of ["setup-later", "setup-close", "setup-go"]) {
        const control = document.getElementById(id);
        if (control) control.addEventListener("click", () => { window.__escapeClicked = id; });
      }
      return {
        focusInSheet: !!(document.activeElement && document.activeElement.closest && document.activeElement.closest("#setup-sheet")),
      };
    });
    assert(
      escape.focusInSheet,
      "the hand is not on the offer, so Escape is about to answer a question it is not pointed at"
    );
    await page.keyboard.press("Escape");
    await page.waitForFunction(() => document.getElementById("setup-sheet").hidden, null, { timeout: 10000 }).catch(() => {});
    const afterEscape = await page.evaluate(() => ({
      clicked: window.__escapeClicked,
      sheetHidden: document.getElementById("setup-sheet").hidden,
    }));
    assertEqual(
      afterEscape.clicked,
      "setup-later",
      "Escape did not press the named way out the CEO can see — a second, quieter way out of the one step that puts an engine on this Mac"
    );
    assert(
      afterEscape.sheetHidden,
      "the named button was pressed and the sheet is still up"
    );
    bump(7);
    assert(errors.length === 0, "the shell logged errors: " + errors.join(" | "));
    await page.close();
    return `home z-index ${painted.homeZ}, sheet z-index ${painted.sheetZ}; the offer is painted on top and still holds focus (${painted.focusId}) when the boot has finished; Escape presses the named Not now`;
  });

  // =======================================================================================
  // 19 — THE SAME DEFECT, ONE SURFACE HIGHER: THE OPENING CURTAIN
  //
  // `init()` opens the offer while the curtain is still up, and the curtain is `z-index: 200`
  // over `.overlay`'s `60`. `main.js`'s Escape machinery asks `isOnScreen()`, which tests
  // `hidden`, `display` and `visibility` and knows nothing about what is painted over what —
  // so at dcebed09 ONE Escape, pressed before anything else had happened, took the curtain
  // down AND pressed "Not now" on a question that had never been on screen. Measured:
  // `#setup-sheet.hidden` true, the offer gone for the session, and an app that looks
  // perfectly normal and cannot answer anything.
  //
  // THAT KEY IS THE ONE A PERSON PRESSES TO GET A CURTAIN OUT OF THE WAY, and candidate .15
  // is the first build with a curtain he can get STUCK behind (§62's space-bar hold), so it
  // is also the first build where pressing it is the obvious thing to do.
  //
  // The space bar is here as the negative control in the other direction: §62 says it HOLDS
  // this screen, so it must not reach the sheet either — and it must not dismiss the curtain.
  // =======================================================================================

  await run.check("19  a key pressed at the opening curtain cannot answer the offer behind it", async () => {
    const open = async () => {
      const p = await browser.newPage({ viewport: { width: 1400, height: 950 } });
      await p.addInitScript(COUNT_BRIDGE, 0);
      await p.addInitScript((v) => {
        window.__RICHOS_MOCK_PRESET__ = v;
      }, { setup: "missing-engine" });
      await p.goto(APP);
      await p.waitForFunction("typeof window.RichSplash === 'object'", null, { timeout: 30000 });
      // The curtain is UP and the offer is already behind it — the window this is about. If
      // either half is not true the check is measuring nothing, so both are asserted.
      await p.waitForSelector("#setup-sheet:not([hidden])", { timeout: 10000 });
      assert(
        await p.evaluate(() => !!document.querySelector(".splash")),
        "the curtain was already down, so this check is not in the window it is about"
      );
      return p;
    };

    const escaped = await open();
    await escaped.keyboard.press("Escape");
    // Two facts, not 2,500 ms (hunt part 2 recheck R41). The curtain leaves on its own clock:
    // `yieldNow` fades it for FADE_MS and removes it 40 ms later (splash.js). That removal is
    // waited for under a hang guard only; a curtain that never leaves is reported by the
    // assertion below, with its own sentence, rather than by the wait. Then whatever else the
    // key set off has finished: a press on "Not now" behind the curtain would be a bridge
    // call, and the sheet is read only after the bridge is quiet.
    await escaped
      .waitForFunction(() => !document.querySelector(".splash"), undefined, { timeout: 30000 })
      .catch(() => {});
    await bridgeQuiet(escaped, "Escape at the opening curtain");
    const afterEscape = await escaped.evaluate(() => ({
      curtain: !!document.querySelector(".splash"),
      sheetHidden: document.getElementById("setup-sheet").hidden,
      top: (function () {
        const e = document.elementFromPoint(700, 475);
        // The middle of the window falls on the panel now that the account sentence is longer,
        // and on the backdrop before: either way it is the offer, so name the sheet it is in.
        if (e && e.closest("#setup-sheet")) return "setup-sheet";
        return e ? e.id || e.className : null;
      })(),
    }));
    await escaped.close();
    assert(
      !afterEscape.sheetHidden,
      "Escape at the opening curtain answered the engine offer behind it — he declined a question he was never shown, and the app looks normal and can answer nothing"
    );
    assert(!afterEscape.curtain, "Escape must still take the curtain down — §5.5, his hand is never delayed here");
    assertEqual(afterEscape.top, "setup-sheet", "the offer is not what is painted after the curtain leaves");

    // §62's hold, from the other side: the space bar belongs to the curtain and reaches
    // nothing behind it.
    const held = await open();
    await held.keyboard.press("Space");
    await held.waitForTimeout(600);
    const afterSpace = await held.evaluate(() => ({
      curtain: !!document.querySelector(".splash"),
      sheetHidden: document.getElementById("setup-sheet").hidden,
      composer: document.getElementById("input").value,
    }));
    await held.close();
    assert(afterSpace.curtain, "the space bar must HOLD the opening screen, not dismiss it (§62)");
    assert(!afterSpace.sheetHidden, "the space bar reached the sheet behind the curtain");
    assertEqual(afterSpace.composer, "", "the space bar typed into the composer behind the curtain");
    bump(6);
    return "Escape takes the curtain and nothing else; space holds it and reaches nothing behind it";
  });

  // =======================================================================================
  // 20 — A QUESTION OWNS THE WINDOW UNTIL IT IS ANSWERED
  //
  // Ray, candidate .16 audit, row B1b: *"With the sheet up, the whole app subtree is still in
  // the AX tree, the composer holds focus and accepts typed characters, and the top-right
  // Settings control is still clickable and takes focus away from the sheet."*
  //
  // THE SHEET SAYS `aria-modal="true"` AND THE WINDOW DOES NOT MAKE IT TRUE. That attribute is
  // a promise to assistive technology that everything outside this dialog is unavailable;
  // `inert` is the only thing that keeps it. Measured at 5f3a1a1e under WebKit at 1400x950 on
  // `setup: "missing-engine"`, with the offer up and the curtain gone: `#input`, the rail's
  // `#rail-settings` gear and the top-right `#set-btn` each take focus on `.focus()`, and a
  // click on `#set-btn` opens `#set-menu` over the sheet — `hidden=false`, `aria-expanded=true`.
  //
  // WHICH CORRECTS HALF OF RAY'S SENTENCE AND MAKES THE OTHER HALF WORSE. "It takes focus and
  // then does nothing" is not what the window does here: it takes focus AND opens the menu, on
  // top of a question the CEO has not answered.
  //
  // WHY NEITHER OF THE TWO OBVIOUS FIXES IS THE FIX, both measured rather than reasoned about:
  //
  //   * MOVING THE SHEET TO BODY LEVEL closes nothing. `.settings` is mounted against
  //     `document.body` by `settings-button.js:748` and painted at z-index 300 by §15's "on
  //     every screen" rule; the sheet is an `.overlay` at 60 wherever it lives. Measured stack
  //     at the button's own center, offer up: `set-btn@auto < settings@300 < setup-sheet@60`
  //     — the control is above the sheet, and a body-level sheet is still at 60.
  //   * MARKING `#app` INERT cannot be done from inside `#app`: `#setup-sheet` is a child of it
  //     (`index.html:936` against `:100`), so that one attribute would inert the question too.
  //
  // So what is marked is the COMPLEMENT OF THE SHEET'S OWN ANCESTOR CHAIN, which needs no
  // opinion about where a sheet lives and reaches `.settings` because `<body>` is on that
  // chain. This check asserts the behavior (what takes focus) rather than the attribute, so a
  // different implementation of the same promise passes it.
  // =======================================================================================

  await run.check("20  nothing outside the offer can be reached while the offer is up", async () => {
    const page = await openApp(browser, { setup: "missing-engine" });
    await page.waitForSelector("#setup-sheet:not([hidden])", { timeout: 10000 });
    await page.waitForFunction(() => !document.querySelector(".splash"), null, { timeout: 30000 });

    const tryFocus = (id) =>
      page.evaluate((i) => {
        const n = document.getElementById(i);
        if (!n) return "absent";
        n.focus();
        return document.activeElement === n ? "took focus" : "refused";
      }, id);

    // THE WINDOW BEHIND THE QUESTION. Ray's three, by their own ids.
    assertEqual(await tryFocus("input"), "refused", "the composer takes focus behind the offer");
    // The rail's gear was the third of Ray's ids; round 27 removed it, so the rail's own
    // control at its foot, the nameplate, stands in for it.
    assertEqual(
      await tryFocus("rail-identity"),
      "refused",
      "the rail's nameplate takes focus behind the offer"
    );
    assertEqual(
      await tryFocus("set-btn"),
      "refused",
      "the top-right Settings control takes focus behind the offer — the one control that " +
        "floats above every screen also floats above a question"
    );

    // AND A REAL MOUSE CANNOT REACH IT EITHER. `page.mouse.click` at the button's own measured
    // center, deliberately, rather than `dispatchEvent`: a synthesized `MouseEvent` skips hit
    // testing entirely and runs the listener on any element, inert or not — measured, it opens
    // the menu either way — so it models nothing a person can do.
    const at = await page.evaluate(() => {
      const b = document.getElementById("set-btn").getBoundingClientRect();
      return { x: b.left + b.width / 2, y: b.top + b.height / 2 };
    });
    await page.mouse.click(at.x, at.y);
    // An ABSENCE of effect: two of the shell's own frames for a menu to open, not 300ms (R10).
    await page.evaluate(() => new Promise((r) => requestAnimationFrame(() => requestAnimationFrame(r))));
    const clicked = await page.evaluate(() => {
      const m = document.getElementById("set-menu");
      return {
        menuOpen: !!m && !m.hidden,
        expanded: document.getElementById("set-btn").getAttribute("aria-expanded"),
      };
    });
    assert(
      !clicked.menuOpen,
      "the settings menu opened on top of an unanswered question (aria-expanded=" +
        clicked.expanded + ")"
    );

    // THE WALL HAS THE QUESTION INSIDE IT, which is the half that keeps this from being a
    // dead app: both of the sheet's own answers still take the keyboard.
    assertEqual(await tryFocus("setup-go"), "took focus", "the offer's own Set it up is inert");
    assertEqual(await tryFocus("setup-later"), "took focus", "the offer's own Not now is inert");

    // ANSWERED, AND THE WINDOW COMES BACK. "Not now" is a real answer and he is entitled to
    // give it; a wall that outlives the question is a worse defect than the one it closed.
    await page.click("#setup-later");
    // The answer registering (the sheet gone) is the fact waited for, then two frames (R10).
    await page.waitForFunction(() => document.getElementById("setup-sheet").hidden, null, { timeout: 10000 }).catch(() => {});
    await page.evaluate(() => new Promise((r) => requestAnimationFrame(() => requestAnimationFrame(r))));
    assertEqual(await tryFocus("input"), "took focus", "the composer stayed unreachable after the offer was answered");
    assertEqual(
      await tryFocus("set-btn"),
      "took focus",
      "the Settings control stayed unreachable after the offer was answered"
    );

    bump(8);
    assert(page.__errors.length === 0, "the shell logged errors: " + page.__errors.join(" | "));
    await page.close();
    return "with the offer up, the composer, the rail gear and the top-right Settings control all refuse focus and the menu will not open; both of the offer's own buttons still take it; answered, the window comes back";
  });

  // =======================================================================================
  // 21 — A SPOKEN SENTENCE ON A STALE ENGINE GETS THE OFFER, NOT A LOST CONNECTION
  //
  // THE TYPED PATH WAS FIXED ON 2026-09-19 (`05735cac`) AND THE SPOKEN PATH WAS NOT. Identical
  // dead conjunction in `src-tauri/src/main.rs` — `!spine.has_lease() && !spine.has_lease_
  // factory()`, with a factory that is configured unconditionally at boot — so a spoken turn
  // on a Mac whose engine is stale started a turn that could not finish and came back as
  // `turn interrupted [transient]`: *"I lost my connection to the part of me that thinks…
  // asking again is worth a try."* Asking again cannot work.
  //
  // THE BACKEND HALF IS ASSERTED IN RUST (`cargo test --bin richos-tauri`,
  // `the_first_run_arm_is_asked_before_a_turn_starts_and_not_behind_the_factory`, which now
  // requires the arm at BOTH sites and refuses one). THIS IS THE WINDOW HALF, and without it
  // the backend's own sentence is a lie: `SETUP_INCOMPLETE_*` says "I've put the setting up
  // back on your screen: press Set it up", and nothing in the window was reopening it on
  // `rich://voice-error`.
  //
  // THE WINDOW ASKS THE DISK RATHER THAN READING THE SENTENCE, which is why this check emits a
  // message the handler has no way to recognize. Matching on a refusal's text would be a
  // second copy of a decision the backend already made; `setup_status` is one question with
  // one answer. The negative control is the other half: the same event on a machine that has
  // everything must open nothing at all.
  // =======================================================================================

  await run.check("21  a spoken turn refused for the setting up puts the offer back on screen", async () => {
    // The listener tap: every `Bridge.listen` callback `main.js` registers, so a backend
    // event can be delivered through the SAME path a real one takes.
    const TAP = `
      window.__TAP = { listeners: {} };
      let _rb;
      Object.defineProperty(window, "RichBridge", {
        configurable: true,
        get() { return _rb; },
        set(v) {
          const ol = v.listen.bind(v);
          v.listen = (name, cb) => { (window.__TAP.listeners[name] = window.__TAP.listeners[name] || []).push(cb); return ol(name, cb); };
          _rb = v;
        }
      });
      window.__emit = (name, payload) => (window.__TAP.listeners[name] || []).forEach((cb) => cb({ payload }));
    `;
    const open = async (preset) => {
      const p = await browser.newPage({ viewport: { width: 1400, height: 950 } });
      const errors = [];
      p.on("pageerror", (e) => errors.push(String(e)));
      p.on("console", (m) => {
        if (m.type() === "error") errors.push("console: " + m.text());
      });
      await p.addInitScript(TAP);
      await p.addInitScript((v) => {
        window.__RICHOS_MOCK_PRESET__ = v;
      }, preset);
      await p.goto(APP);
      await leaveHome(p);
      await p.waitForSelector(".nav-thread", { state: "attached" });
      p.__errors = errors;
      return p;
    };

    // A MACHINE WITH A STALE ENGINE, with the question already declined once — which is the
    // state a spoken sentence is actually said in. "Not now" is a real answer and writes
    // nothing, so the disk still says the engine is missing.
    const page = await open({ setup: "missing-engine" });
    await page.waitForSelector("#setup-sheet:not([hidden])", { timeout: 10000 });
    await page.click("#setup-later");
    await page.waitForFunction(() => document.getElementById("setup-sheet").hidden, null, { timeout: 30000 });

    // The refusal, on voice's own channel, exactly as `start_voice_capture`'s submit closure
    // emits it. The text is the shipped sentence and the window never looks at it.
    await page.evaluate(() => {
      window.__emit("rich://voice-error", {
        message:
          "I can't take that on yet: the RichOS engine isn't on this Mac, and that's the " +
          "part of me that knows how I work. I've put the setting up back on your screen: " +
          "press Set it up and I'll fetch it. There's nothing to quit and nothing to reopen.",
        at: Date.now(),
      });
    });
    await page.waitForFunction(() => !document.getElementById("setup-sheet").hidden, null, { timeout: 30000 })
      .catch(() => {});
    assert(
      await page.evaluate(() => !document.getElementById("setup-sheet").hidden),
      "a spoken turn was refused for the setting up and the offer did not come back — the " +
        "backend's own sentence promises it is on screen"
    );
    assert(
      await page.evaluate(() => !!document.getElementById("setup-go") && !document.getElementById("setup-go").hidden),
      "the sheet came back without the control its sentence names"
    );
    bump(2);
    assert(page.__errors.length === 0, "the shell logged errors: " + page.__errors.join(" | "));
    await page.close();

    // THE NEGATIVE CONTROL. Voice can stop for reasons that have nothing to do with the
    // setting up — a device lost, a recognizer that failed — and on a machine that has
    // everything, this event must open nothing.
    const healthy = await open({});
    await healthy.evaluate(() => {
      window.__emit("rich://voice-error", {
        message: "My ears stopped working just now.",
        at: Date.now(),
      });
    });
    await healthy.waitForTimeout(600);
    assert(
      await healthy.evaluate(() => document.getElementById("setup-sheet").hidden),
      "a voice error on a machine that has everything opened the first-run offer"
    );
    bump(1);
    assert(healthy.__errors.length === 0, "the shell logged errors: " + healthy.__errors.join(" | "));
    await healthy.close();

    return "the offer is back with Set it up on it after a spoken refusal, and a voice error on a healthy machine opens nothing";
  });

  await run.check("22  \"Setup is done.\" waits for the video tools still downloading", async () => {
    // THE CEO, 2026-10-07: the video tools download "right away and independently of the other
    // stuff". `run_setup` answers `complete` with them missing while that download runs
    // (`setup_view::sheet_needs`), and a press that found them alone on the sheet gets its answer
    // BEFORE the download's first line reaches the window. On main the heading then read
    // "Setup is done." above a line still counting a percentage.
    const page = await openApp(browser, { setup: "missing-engine" });
    await page.waitForSelector("#setup-sheet:not([hidden])");
    // `setup_view::run` with only the video tools wanted: it starts their download and returns at
    // once, emitting nothing, with `complete` true and `media_tools.present` false (the mock's
    // status has no `media_tools`; the shell's always does).
    await page.evaluate(() => {
      const inv = window.RichBridge.invoke.bind(window.RichBridge);
      window.RichBridge.invoke = async (cmd, args) => {
        if (cmd !== "run_setup") return inv(cmd, args);
        const out = await inv("setup_status");
        out.status.media_tools = { component: "media-tools", present: false, at: null, detail: null, looked_in: [] };
        out.status.installed_now = true;
        out.ask.items = [];
        out.ask.can_install = false;
        out.complete = true;
        return out;
      };
    });
    await page.click("#setup-go");
    await page.waitForSelector("#setup-close:not([hidden])");
    const title = async () => (await page.textContent("#setup-title")).trim();
    const closeTop = () => page.evaluate(() => Math.round(document.getElementById("setup-close").getBoundingClientRect().top));
    const atAnswer = await title();
    assert(atAnswer !== "Setup is done.", "the sheet says it is done while the video tools are still downloading");
    const top0 = await closeTop();
    const line = "Getting my video tools. 43%";
    await page.evaluate((what) => window.__RICHOS_MOCK__.setupEmit({ state: "started", component: "media-tools", what, index: 1, total: 1 }), line);
    const during = await title();
    assertEqual(during, line, "the sheet must show the download's progress line, not \"Setup is done.\"");
    assert(!(await sheetText(page)).includes("Setup is done."), "\"Setup is done.\" is on the sheet mid-download");
    const top1 = await closeTop();
    await page.evaluate(() => window.__RICHOS_MOCK__.setupEmit({ state: "done", component: "media-tools", what: "My video tools are installed.", index: 1, total: 1 }));
    const after = await title();
    assertEqual(after, "Setup is done.", "once the video tools are installed the sheet says so");
    const top2 = await closeTop();
    assertEqual([top1, top2].join(","), [top0, top0].join(","), "`Close` moved while the heading followed the download");
    assert(page.__errors.length === 0, "the shell logged errors: " + page.__errors.join(" | "));
    bump(5);
    await page.close();
    return "at the answer " + JSON.stringify(atAnswer) + ", mid-download " + JSON.stringify(during) +
      ", installed " + JSON.stringify(after) + "; Close held y=" + top0;
  });

  // =======================================================================================
  // ROUND 19, STATES 1 TO 4 (dictation plan revision 2, slice 4), behind `ask.dictation`.
  // =======================================================================================

  await run.check("2a his two sentences and the counter are the floor's only exemption", async () => {
    // EACH EXEMPT STRING IS HIS, WORD FOR WORD: the constants the sheet renders from.
    assertEqual(HIS_TOOLS_LINE, HIS_SETUP_SENTENCE.slice("My voice and video tools: ".length), "MEDIA_TOOLS_WHY is not his line");
    assertEqual(HIS_DOWNLOAD_LINE, ROUND_19_DOWNLOAD_LINE, "SETUP_DOWNLOAD_LINE is not round 19's line");
    assertEqual(HIS_ACCOUNT_NOTE, ROUND_ACCOUNT_NOTE, "SETUP_ACCOUNT_NOTE is not his account sentence");
    const page = await openApp(browser, R19);
    await page.waitForSelector("#setup-sheet:not([hidden])");
    // State 1: his item line is on the sheet, and is the only thing exempt.
    const ask = await sheetTextExempt(page);
    assertEqual(ask.removed.join(" | "), HIS_ACCOUNT_NOTE + " | " + HIS_TOOLS_LINE, "state 1 exempts his account sentence and his item line, nothing else");
    assertEqual(floorViolations(ask.rest).join(", "), "", "outside his line the floor holds: " + ask.rest);
    // State 2: his download line and the counter, and nothing else.
    await page.click("#setup-go");
    await page.waitForSelector("#setup-steps:not([hidden])");
    await page.evaluate((line) => window.__RICHOS_MOCK__.setupEmit(line), toolsLine(489_999_396, 1_061_655_396, true, "490 MB of 1.06 GB"));
    await page.waitForSelector("#setup-rich-line:not([hidden])");
    const run2 = await sheetTextExempt(page);
    // His account sentence is exempt wherever it is still on the sheet; once the sheet is running
    // the provider's own status may have replaced it, which is not an exemption to expect.
    assertEqual(run2.removed.filter((s) => s !== HIS_ACCOUNT_NOTE).join(" | "), HIS_DOWNLOAD_LINE + " | 490 MB of 1.06 GB", "state 2 exempts his download line and the counter, and his account sentence only if it is still there");
    assertEqual(floorViolations(run2.rest).join(", "), "", "outside them the floor holds: " + run2.rest);
    assert(page.__errors.length === 0, "the shell logged errors: " + page.__errors.join(" | "));
    bump(6);
    await page.close();
    return "exempt: his item line in state 1; his download line and \"490 MB of 1.06 GB\" in state 2; the rest holds the floor";
  });

  await run.check("2b NEGATIVE CONTROL: a path on another line, or his line with one word changed, still fails", async () => {
    const page = await openApp(browser, R19);
    await page.waitForSelector("#setup-sheet:not([hidden])");
    // A path and a version on the account line: not his sentence any more, so not exempt.
    await page.evaluate(() => {
      const el = document.getElementById("setup-account");
      el.textContent = el.textContent.replace("Anthropic", "Anthropik") + " See ~/Library/RichOS 1.2.3 first.";
    });
    const pathed = await sheetTextExempt(page);
    assertEqual(pathed.removed.includes(HIS_ACCOUNT_NOTE), false, "an altered account sentence was exempted");
    assert(floorViolations(pathed.rest).includes("a path"), "a path on the account line passed the floor");
    assert(floorViolations(pathed.rest).includes("a home-relative path"), "a home path on the account line passed the floor");
    // His line with one word changed is not his line: the dollar sign and the slash fail.
    await page.evaluate(() => {
      const why = document.querySelector('#setup-items li[data-component="media-tools"] .setup-item-why');
      why.textContent = why.textContent.replace("free", "cheap");
    });
    const changed = await sheetTextExempt(page);
    assertEqual(changed.removed.length, 0, "a changed line was exempted");
    assert(floorViolations(changed.rest).includes("a shell variable"), "a changed line's dollar sign passed the floor");
    // A counter element carrying anything but a counter is refused, not exempted.
    await page.click("#setup-go");
    await page.waitForSelector("#setup-steps:not([hidden])");
    await page.evaluate((line) => window.__RICHOS_MOCK__.setupEmit(line), toolsLine(1, 1_061_655_396, true, "see /tmp/x"));
    let refused = false;
    try { await sheetTextExempt(page); } catch (_) { refused = true; }
    assert(refused, "a counter element carrying a path was exempted");
    bump(4);
    await page.close();
    return "a path on the account line, a changed word in his line and a path in the counter element all fail";
  });

  await run.check("23  round 19, state 1: the setup sheet as drawn, his sentence word for word", async () => {
    const page = await openApp(browser, R19);
    await page.waitForSelector("#setup-sheet:not([hidden])");
    assertEqual((await page.textContent("#setup-title")).trim(), "There's a bit of setting up to do first.", "the drawn title");
    const items = await page.evaluate(() =>
      [...document.querySelectorAll("#setup-items li")].map((li) => [li.dataset.component, li.innerText.replace(/\s+/g, " ").trim()])
    );
    assertEqual(items.map((i) => i[0]).join(","), "claude-code,engine,media-tools", "three items, the video tools listed while they download");
    assertEqual(items[2][1], HIS_SETUP_SENTENCE, "the voice and video tools' item is his sentence, word for word");
    assertEqual(items[0][1], "Claude Code: the program I think with. It comes from Anthropic and installs itself; I only ask it to.", "Claude Code's item");
    assertEqual(items[1][1], "The RichOS engine: the part of me that knows how I work: my instructions and my team.", "the engine's item");
    assert(await page.isHidden("#setup-note"), "round 19 draws no note under the title");
    assert(await page.isVisible("#setup-go") && await page.isVisible("#setup-later"), "Set it up and Not now");
    assert(await page.isHidden("#setup-progress"), "no progress line in state 1");
    // GATE OFF: the same Mac on a build where dictation is not there sees the sheet as it was.
    const off = await openApp(browser, { ...R19, dictation: false });
    await off.waitForSelector("#setup-sheet:not([hidden])");
    assertEqual((await off.textContent("#setup-title")).trim(), "There are a couple of things I need on this Mac.", "gate off keeps the old title");
    const offItems = await off.evaluate(() => [...document.querySelectorAll("#setup-items li")].map((li) => li.dataset.component));
    assertEqual(offItems.join(","), "claude-code,engine", "gate off does not list the downloading video tools");
    assert(!(await sheetText(off)).includes("Wispr"), "gate off says nothing about dictation");
    assert(page.__errors.length === 0 && off.__errors.length === 0, "the shell logged errors");
    bump(10);
    await page.close();
    await off.close();
    return "title, three items with his sentence verbatim; gate off is the old sheet";
  });

  await run.check("24  round 19, state 2: the rows, the summed counter, and his line only for both models", async () => {
    const page = await openApp(browser, R19);
    await page.waitForSelector("#setup-sheet:not([hidden])");
    // RICH'S AVATAR IS NOT FETCHED BEFORE HIS LINE IS DRAWN. Loaded at boot, the hidden 40px copy
    // changed how WebKit drew the same picture at 18px beside every "Rich" in the conversation:
    // 67 to 253 pixels of it, in about fifty reference pictures across eight suites.
    assertEqual(await page.getAttribute("#setup-rich-avatar", "src"), null, "the avatar is fetched before his line is drawn");
    // THE MOCK IS HELD at Claude Code's step, so the engine row is read while the engine has
    // provably not started, however loaded the machine is. Its step events run on their own
    // 30 ms timers; unheld, a slow run had the engine finished before this was read.
    await page.evaluate(() => window.__RICHOS_MOCK__.setupHold(true));
    await page.click("#setup-go");
    await page.waitForSelector("#setup-steps:not([hidden])");
    const rows = () => page.evaluate(() =>
      [...document.querySelectorAll("#setup-steps .setup-step")].map((li) => ({
        c: li.dataset.component,
        cls: li.className,
        text: li.innerText.replace(/\s+/g, " ").trim(),
        bar: (li.querySelector(".setup-step-bar b") || {}).style?.width || null,
      }))
    );
    assertEqual((await page.textContent("#setup-title")).trim(), "Setting things up", "the drawn title");
    let r = await rows();
    assertEqual(r.map((x) => x.c).join(","), "claude-code,engine,media-tools", "a row per item");
    assert(/Claude Code/.test(r[0].text) && /The RichOS engine/.test(r[1].text) && /My voice and video tools/.test(r[2].text), "the drawn names: " + JSON.stringify(r));
    // The mock's first step event: Claude Code is installing, the engine waits.
    await page.waitForFunction(() => /Installing/.test(document.querySelector('#setup-steps [data-component="claude-code"]').innerText));
    r = await rows();
    assert(/Waiting/.test(r[1].text) && !/Installing|Installed/.test(r[1].text), "the engine row while Claude Code installs: " + r[1].text);
    await page.evaluate(() => window.__RICHOS_MOCK__.setupHold(false));
    // Two counter readings that increase, on a moving bar.
    await page.evaluate((line) => window.__RICHOS_MOCK__.setupEmit(line), toolsLine(489_999_396, 1_061_655_396, true, "490 MB of 1.06 GB"));
    r = await rows();
    assert(r[2].text.endsWith("490 MB of 1.06 GB"), "the first reading: " + r[2].text);
    const first = parseFloat(r[2].bar);
    assertEqual((await page.textContent("#setup-rich-text")).trim(), HIS_DOWNLOAD_LINE, "his line while both models download");
    // ...and once it is drawn, the avatar beside it is there.
    await page.waitForFunction(() => {
      const a = document.getElementById("setup-rich-avatar");
      return a.getAttribute("src") === "assets/rich-hand.png" && a.complete && a.naturalWidth > 0;
    });
    await page.evaluate((line) => window.__RICHOS_MOCK__.setupEmit(line), toolsLine(700_000_000, 1_061_655_396, true, "700 MB of 1.06 GB"));
    r = await rows();
    assert(r[2].text.endsWith("700 MB of 1.06 GB") && parseFloat(r[2].bar) > first, "the second reading moves on: " + JSON.stringify(r[2]));
    // MINOR 10: one model missing, so "about 1 GB" is untrue; the row counts without his line.
    await page.evaluate((line) => window.__RICHOS_MOCK__.setupEmit(line), toolsLine(100_000_000, 574_041_195, false, "100 MB of 574 MB"));
    assert(await page.isHidden("#setup-rich-line"), "his line shows while only one model is fetched");
    r = await rows();
    assert(r[2].text.endsWith("100 MB of 574 MB"), "the one-model counter: " + r[2].text);
    // Checking… while the file is hashed.
    await page.evaluate((line) => window.__RICHOS_MOCK__.setupEmit(line), toolsLine(574_041_195, 574_041_195, false, "Checking…"));
    r = await rows();
    assert(r[2].text.endsWith("Checking…"), "the hash: " + r[2].text);
    assert(page.__errors.length === 0, "the shell logged errors: " + page.__errors.join(" | "));
    bump(12);
    await page.close();
    return "rows Claude Code / The RichOS engine / My voice and video tools; 490 then 700 MB of 1.06 GB on a moving bar; his line only for both models; Checking…";
  });

  await run.check("25  round 19, state 3: Start while the voice row counts, then \"You're all set.\"", async () => {
    const page = await openApp(browser, R19);
    await page.waitForSelector("#setup-sheet:not([hidden])");
    await page.evaluate((line) => window.__RICHOS_MOCK__.setupEmit(line), toolsLine(300_000_000, 1_061_655_396, true, "300 MB of 1.06 GB"));
    await page.click("#setup-go");
    // CLAUDE CODE AND THE ENGINE ARE IN: Start is on the sheet while the voice row still counts
    // (his 2026-10-07 words: nothing in setup waits for the video tools).
    await page.waitForSelector("#setup-start:not([hidden])");
    assertEqual((await page.textContent("#setup-title")).trim(), "Setting things up", "not all set while the voice row counts");
    const voiceRow = () => page.evaluate(() => document.querySelector('#setup-steps [data-component="media-tools"]').innerText.replace(/\s+/g, " ").trim());
    assert((await voiceRow()).endsWith("300 MB of 1.06 GB"), "the voice row still counts: " + (await voiceRow()));
    const states = await page.evaluate(() => [...document.querySelectorAll("#setup-steps .setup-step")].map((li) => li.className));
    assert(/is-done/.test(states[0]) && /is-done/.test(states[1]), "Claude Code and the engine are Installed: " + states);
    assert(await page.isHidden("#setup-close"), "Start, not Close");
    const startTop = () => page.evaluate(() => Math.round(document.getElementById("setup-start").getBoundingClientRect().top));
    const top0 = await startTop();
    await page.evaluate((line) => window.__RICHOS_MOCK__.setupEmit(line), toolsLine(900_000_000, 1_061_655_396, true, "900 MB of 1.06 GB"));
    await page.evaluate(() => window.__RICHOS_MOCK__.setupEmit({ state: "done", component: "media-tools", what: "My voice and video tools are installed.", index: 1, total: 1 }));
    assertEqual((await page.textContent("#setup-title")).trim(), "You're all set.", "all three in");
    assertEqual((await page.textContent("#setup-rich-text")).trim(), "Voice is ready. You can just talk to me now instead of typing.", "Rich's done line");
    assert((await voiceRow()).endsWith("Installed"), "the voice row: " + (await voiceRow()));
    const top1 = await startTop();
    // START DOES NOT MOVE UNDER HIS HAND (case 17's rule). Before `lockSetupHeight` it moved up
    // 81 px here, measured, as the three-line download line became "Voice is ready.".
    assert(Math.abs(top1 - top0) <= 1, "Start jumped " + (top1 - top0) + "px as the download finished");
    // Escape is Start, the named way out.
    await page.keyboard.press("Escape");
    await page.waitForSelector("#setup-sheet", { state: "hidden" });
    assert(page.__errors.length === 0, "the shell logged errors: " + page.__errors.join(" | "));
    bump(8);
    await page.close();
    return "Start at y=" + top0 + " while counting, \"You're all set.\" with \"Voice is ready.\" when the download finished, Start at y=" + top1 + "; Escape presses it";
  });

  await run.check("26  round 19, state 4: Rich offers dictation once, after Start, and each answer says the drawn line", async () => {
    // Everything installed during the press (the models were already there), voice ready.
    const finish = async (preset, answer, before) => {
      const page = await openApp(browser, { ...R19, videoTools: "present", ...preset });
      await page.waitForSelector("#setup-sheet:not([hidden])");
      if (before) await page.evaluate(before);
      await page.click("#setup-go");
      await page.waitForSelector("#setup-start:not([hidden])");
      assertEqual((await page.textContent("#setup-title")).trim(), "You're all set.", "all set when the models were already there");
      await page.click("#setup-start");
      await page.waitForSelector("#setup-sheet", { state: "hidden" });
      return page;
    };
    const page = await finish({});
    await page.waitForSelector("#dictation-offer");
    const said = await page.evaluate(() => [...document.querySelectorAll("#dictation-offer .tl-prose")].map((p) => p.innerText.replace(/\s+/g, " ").trim()));
    assertEqual(said[0], "Voice is ready. Press the round button beside the message box and just talk to me.", "the drawn first paragraph");
    // The CEO's copy, 2026-10-13 (turn-on-dictation-paragraph.png), word for word; [F1] is the key.
    assertEqual(said[1], "If you turn on RichScribe, I will automatically give you a free & private/local replacement for Wispr Flow. (saves you $140+/year)", "his first new paragraph");
    assertEqual(said[2], "So, instead of typing you'll be able to do this: Just click into any other app (Mail, Slack, Chrome/Safari, anything), press F1 (or another key you'll pick as the trigger) and say what you want written. Your words will appear where your cursor is.😊", "his second new paragraph, the key filled in");
    assertEqual(said.length, 3, "his two paragraphs replace the old one");
    // NO BLANK LINE UNDER "Rich" (seen on the guest, 2026-10-08): the first sentence sits where
    // the greeting's does, right under the name.
    assertEqual(
      await page.evaluate(() => getComputedStyle(document.querySelector("#dictation-offer .tl-prose")).marginTop),
      "0px",
      "the offer's first sentence starts a line below Rich's name"
    );
    assertEqual((await page.textContent("#dictation-offer-yes")).trim(), "Turn on RichScribe", "the first button");
    assertEqual((await page.textContent("#dictation-offer-no")).trim(), "Not now", "the second button");
    assertEqual((await page.evaluate(() => window.__RICHOS_MOCK__.dictationOfferCalls())).join(","), "dictation_offer,dictation_offer_shown", "asked once and recorded as shown");
    await page.click("#dictation-offer-no");
    await page.waitForSelector("#dictation-offer-after");
    assertEqual((await page.textContent("#dictation-offer-after")).trim(), "Okay. It's in Settings, under RichScribe, whenever you want it.", "Not now's line");
    assert(await page.isHidden("#dictation-offer-yes"), "answered, the buttons go");
    await page.close();

    // Turn on dictation, handed to the Dictation sheet's own turn-on (slice 2, `dictation.js`):
    // on when both permissions are allowed, not on when macOS refused the microphone.
    const on = await finish({ dictationView: { mic: "allowed", ax: "allowed" } });
    await on.waitForSelector("#dictation-offer-yes");
    await on.click("#dictation-offer-yes");
    // ONCE IT IS SET UP, ITS TEXT MOVES TO ITS OWN CONVERSATION (the CEO, 2026-10-13,
    // onboarding5.png): this conversation keeps only "Voice is ready.", and he picks his key in
    // the sheet before closing it (onboarding3.png: he picked F2, the app said F1).
    await on.waitForSelector("#dictation-sheet:not([hidden])");
    await on.waitForFunction(() => !document.getElementById("dictation-offer-yes"));
    assertEqual(
      await on.evaluate(() => [...document.querySelectorAll("#dictation-offer .tl-prose")].map((p) => p.innerText.trim())),
      ["Voice is ready. Press the round button beside the message box and just talk to me."],
      "the dictation text left the onboarding conversation"
    );
    await on.click('.dict-key[data-k="2"]');
    await on.waitForFunction(() => window.__RICHOS_MOCK__.dictationCalls().some((c) => c.cmd === "dictation_set_key" && c.key === 2));
    await on.click("#dict-close");
    const feature = on.locator(".nav-thread", { hasText: "RichScribe feature" });
    await feature.waitFor();
    assertEqual(await on.locator(".nav-thread", { hasText: "RichScribe feature" }).count(), 1, "one conversation, under his company");
    await feature.click();
    await on.waitForFunction(() => /Good news!/.test(document.getElementById("messages").innerText));
    assertEqual(
      // Rich's prose is one Markdown body; his four paragraphs are its four paragraphs.
      await on.evaluate(() => [...document.querySelectorAll("#messages .tl-prose .tl-md-p")].map((p) => p.innerText.replace(/\s+/g, " ").trim())),
      [
        "Good news! RichScribe is set up and enabled.",
        "So, in any other app on this Mac (Mail, Slack Chrome/Safari etc.) you can now do this:",
        "Click into where you want your words to appear, tap F2, say what you want written, then tap it again. See how your words appear where your cursor is.",
        "If you have any questions about this, we can discuss it here.",
      ],
      "his text, word for word, with the key he picked, and nothing else in that conversation"
    );
    // HE PICKS ANOTHER KEY LATER (second review rv-20261013T183104Z, finding 2; the CEO: "make
    // sure the correct key is actually within that message"): the message names the new key,
    // and it is still the one message there.
    await on.evaluate(() => window.RichDictation.open());
    await on.waitForSelector("#dictation-sheet:not([hidden])");
    await on.click('.dict-key[data-k="3"]');
    await on.click("#dict-close");
    await on.waitForFunction(() => /tap F3,/.test(document.getElementById("messages").innerText));
    assertEqual(
      await on.evaluate(() => [...document.querySelectorAll("#messages .tl-prose .tl-md-p")].map((p) => p.innerText.replace(/\s+/g, " ").trim())),
      [
        "Good news! RichScribe is set up and enabled.",
        "So, in any other app on this Mac (Mail, Slack Chrome/Safari etc.) you can now do this:",
        "Click into where you want your words to appear, tap F3, say what you want written, then tap it again. See how your words appear where your cursor is.",
        "If you have any questions about this, we can discuss it here.",
      ],
      "the key changed to F3: the one message names F3"
    );
    await on.close();
    const later = await finish({ dictationKey: 5, dictationView: { mic: "denied" } });
    await later.waitForSelector("#dictation-offer-yes");
    assert((await later.textContent("#dictation-offer")).includes("press F5 (or another key"), "the chosen key is named");
    await later.click("#dictation-offer-yes");
    await later.waitForSelector("#dictation-offer-after");
    assertEqual((await later.textContent("#dictation-offer-after")).trim(), "It's in Settings, under RichScribe.", "not on");
    await later.close();

    // SET UP LATER, FROM SETTINGS (second review rv-20261013T173014Z, finding 2): "Not now" first,
    // then dictation turned on in its sheet with F2. The dictation text still leaves the
    // onboarding conversation, and "Dictation feature" names F2.
    const later2 = await finish({ dictationView: { mic: "allowed", ax: "allowed" } });
    await later2.waitForSelector("#dictation-offer-no");
    await later2.click("#dictation-offer-no");
    await later2.waitForSelector("#dictation-offer-after");
    await later2.evaluate(() => window.RichDictation.open());
    await later2.waitForSelector("#dictation-sheet:not([hidden])");
    await later2.click("#dict-switch");
    await later2.waitForFunction(() => /^On\. Tap/.test((document.getElementById("dict-state") || {}).textContent || ""));
    await later2.click('.dict-key[data-k="2"]');
    await later2.waitForFunction(() => window.__RICHOS_MOCK__.dictationCalls().some((c) => c.cmd === "dictation_set_key" && c.key === 2));
    await later2.click("#dict-close");
    await later2.locator(".nav-thread", { hasText: "RichScribe feature" }).waitFor();
    await later2.waitForFunction(() => document.querySelectorAll("#dictation-offer .tl-prose").length === 1);
    assertEqual(
      await later2.evaluate(() => [...document.querySelectorAll("#dictation-offer .tl-prose")].map((p) => p.innerText.trim())),
      ["Voice is ready. Press the round button beside the message box and just talk to me."],
      "set up later: the dictation text left the onboarding conversation too"
    );
    await later2.close();

    // A FRESH INSTALL: Start comes before his first company, so no conversation is open when
    // the offer is made; it is drawn in the first one, once he has added the company (walked on
    // the guest 2026-10-13: tying the note to the open conversation hid it when none was open).
    const fresh = await finish({ chosenEntity: null, memory: "none" });
    await fresh.waitForSelector("#memory-setup:not([hidden])");
    await fresh.click("#memory-setup-later");
    await fresh.waitForSelector("#entity-picker:not([hidden])");
    await fresh.evaluate(() => {
      const b = [...document.querySelectorAll("#entity-picker button")].find((x) => /add/i.test(x.textContent));
      b.click();
    });
    await fresh.fill("#entity-add-name", "Brightwave LABS");
    await fresh.click("#entity-add-go");
    await fresh.waitForSelector("#dictation-offer-yes");
    await fresh.close();

    // THE ROUND BUTTON LEARNED (the CEO, 2026-10-19): once he has pressed it, "Voice is ready.
    // Press the round button..." is gone from the conversation; the dictation paragraphs stay
    // while the offer is open, and when dictation is on the whole note goes.
    const used = await finish({});
    await used.waitForSelector("#dictation-offer");
    await used.click("#talk-toggle");
    await used.waitForFunction(() => ![...document.querySelectorAll("#dictation-offer .tl-prose")].some((p) => /round button/.test(p.innerText)));
    assertEqual(await used.evaluate(() => document.body.innerText.includes("Press the round button")), false, "the voice line is gone after the first press");
    assertEqual(await used.locator("#dictation-offer .tl-prose").count(), 2, "the dictation paragraphs stay");
    await used.close();
    const usedOn = await finish({ dictationView: { mic: "allowed", ax: "allowed" } });
    await usedOn.waitForSelector("#dictation-offer-yes");
    await usedOn.click("#talk-toggle");
    await usedOn.click("#dictation-offer-yes");
    await usedOn.waitForSelector("#dictation-sheet:not([hidden])");
    await usedOn.waitForFunction(() => !document.getElementById("dictation-offer"));
    await usedOn.close();

    // NEVER AGAIN: the offer was made in an earlier run (a relaunch reads `offered`).
    const again = await finish({ dictationOffered: true });
    await again.waitForFunction(() => window.__RICHOS_MOCK__.dictationOfferCalls().length > 0);
    // The answer was in hand when the call returned; two frames are its render, if it had one.
    await settledFrames(again);
    assertEqual(await again.locator("#dictation-offer").count(), 0, "the offer came back after it was made");
    assertEqual((await again.evaluate(() => window.__RICHOS_MOCK__.dictationOfferCalls())).join(","), "dictation_offer", "asked, and not shown");
    await again.close();

    // ONLY ONCE VOICE IS READY: with the model still missing there is no offer, and it comes
    // when the model arrives.
    const waits = await finish({ voice: "model-missing" });
    // The fact the offer waits on: voice was asked again after the press, and said not ready.
    await waits.waitForFunction(() => {
      const cmds = window.__calls.map((c) => c.cmd);
      return cmds.lastIndexOf("voice_readiness") > cmds.indexOf("run_setup");
    });
    await settledFrames(waits);
    assertEqual(await waits.locator("#dictation-offer").count(), 0, "offered before voice is ready");
    assertEqual((await waits.evaluate(() => window.__RICHOS_MOCK__.dictationOfferCalls())).length, 0, "asked before voice is ready");
    await waits.evaluate(() => window.__RICHOS_MOCK__.voiceModelEmit({ phase: "installed", modelId: "small.en", received: 1, total: 1 }));
    await waits.waitForSelector("#dictation-offer");
    await waits.close();

    // GATE OFF: no Start, no offer, and the window never asks.
    const off = await openApp(browser, { setup: "missing-both", memory: "ready" });
    await off.waitForSelector("#setup-sheet:not([hidden])");
    await off.click("#setup-go");
    await off.waitForSelector("#setup-close:not([hidden])");
    assert(await off.isHidden("#setup-start"), "gate off draws Start");
    await off.click("#setup-close");
    await off.waitForSelector("#setup-sheet", { state: "hidden" });
    await settledFrames(off);
    assertEqual(await off.locator("#dictation-offer").count(), 0, "gate off offers dictation");
    assertEqual((await off.evaluate(() => window.__RICHOS_MOCK__.dictationOfferCalls())).length, 0, "gate off asked about the offer");
    assert(off.__errors.length === 0, "the shell logged errors: " + off.__errors.join(" | "));
    await off.close();
    bump(16);
    return "both paragraphs and buttons as drawn; Not now, on and not-on lines; once only; after voice is ready; nothing with the gate off";
  });

  await run.check("27  round 19: a step that failed stops saying it is installing", async () => {
    // FOUND ON THE GUEST, 2026-10-08: the engine's download was refused (a 404), the failure's
    // sentence came up, and the engine's row kept "Installing…" with its spinner turning.
    const sentence = "The download didn't arrive. Nothing has been changed on your Mac.";
    const page = await openApp(browser, { ...R19, setupFails: sentence });
    await page.waitForSelector("#setup-sheet:not([hidden])");
    await page.click("#setup-go");
    await page.waitForSelector("#setup-error:not([hidden])");
    const rows = await page.evaluate(() =>
      [...document.querySelectorAll("#setup-steps .setup-step")].map((li) => ({
        c: li.dataset.component,
        cls: li.className,
        state: li.querySelector(".setup-step-state").textContent,
      }))
    );
    const failed = rows.find((r) => r.c === "claude-code");
    assert(/is-failed/.test(failed.cls), "the failed step's row: " + JSON.stringify(failed));
    assertEqual(failed.state, "", "a failed row says nothing of its own; the sentence beneath it does");
    assert(!rows.some((r) => r.c !== "media-tools" && /is-now/.test(r.cls)), "a row still spins after the failure: " + JSON.stringify(rows));
    assertEqual((await page.textContent("#setup-error")).trim(), sentence, "the failure's own sentence");
    assertEqual((await page.textContent("#setup-go")).trim(), "Try again", "the way on");
    assert(await page.isHidden("#setup-start"), "no Start after a failed press");
    assert(page.__errors.length === 0, "the shell logged errors: " + page.__errors.join(" | "));
    bump(6);
    await page.close();
    return "the failed row is still and silent, the sentence is verbatim, Try again is up, no Start";
  });

  await run.check("28  round 19, state 2 draws no button while the press runs", async () => {
    // SEEN ON THE GUEST, 2026-10-08: a disabled "Set it up" sat under the counting rows, where
    // round 19 draws no button at all. The press is held open here so the running state is the
    // one measured, not the finished one.
    const page = await openApp(browser, R19);
    await page.waitForSelector("#setup-sheet:not([hidden])");
    await page.evaluate(() => {
      const inv = window.RichBridge.invoke.bind(window.RichBridge);
      window.RichBridge.invoke = (cmd, args) => (cmd === "run_setup" ? new Promise(() => {}) : inv(cmd, args));
    });
    await page.click("#setup-go");
    await page.waitForSelector("#setup-steps:not([hidden])");
    const visible = await page.evaluate(() =>
      ["setup-go", "setup-later", "setup-close", "setup-start"].filter((id) => !document.getElementById(id).hidden)
    );
    assertEqual(visible.join(","), "", "a button is drawn while the press runs");
    assertEqual((await page.textContent("#setup-title")).trim(), "Setting things up", "state 2's heading");
    bump(2);
    await page.close();
    return "no button while the press runs; Try again (case 27) or Start (case 25) when it ends";
  });

  await run.check("29  round 19, state 2: the spinners keep turning through every counter reading and through Start appearing", async () => {
    // HIS RECORDING OF NIGHTLY 54 (docs/ceo-input/2026-10-13_1/animations.mov, 0 to 13.9 s): at
    // 60 frames a second both spinners swept from 12 o'clock to about 3 o'clock and snapped back
    // every 12 frames (200 ms), the cadence of the download's counter. Each reading rebuilt the
    // rows, so each spinner was a new element whose rotation started again at 0 degrees:
    // 400 degrees a second for 0.2 s is 80 degrees, and it never went round. Round 19 updates the
    // counter and the bar in place (its `runSetup`), so its ring turns without a break.
    //
    // Measured as he sees it, by the ANGLE: whatever element is turning in a row, its rotation
    // after a reading is where it was before plus the time between, never back at the start.
    const page = await openApp(browser, R19);
    await page.waitForSelector("#setup-sheet:not([hidden])");
    await page.evaluate((line) => window.__RICHOS_MOCK__.setupEmit(line), toolsLine(300_000_000, 1_061_655_396, true, "300 MB of 1.06 GB"));
    // Held at Claude Code's step: two rows turn at once, as in his recording.
    await page.evaluate(() => window.__RICHOS_MOCK__.setupHold(true));
    await page.click("#setup-go");
    await page.waitForFunction(() => document.querySelectorAll("#setup-steps .setup-step.is-now").length === 2);
    // Each turning row's spinner: its node, its turn so far (0..1) and the clock, read together.
    const turns = () => page.evaluate(() => {
      window.__spinNodes = window.__spinNodes || new Map();
      const now = document.timeline.currentTime;
      return [...document.querySelectorAll("#setup-steps .setup-step.is-now")].map((li) => {
        const a = li.getAnimations({ subtree: true }).find((x) => x.animationName === "setup-step-spin");
        const t = a && a.effect.getComputedTiming();
        const known = window.__spinNodes.get(li.dataset.component);
        if (!known && a) window.__spinNodes.set(li.dataset.component, a.effect.target);
        return {
          c: li.dataset.component,
          now,
          turn: t ? t.progress : null,
          ms: t ? t.duration : null,
          same: known ? known === (a && a.effect.target) && known.isConnected : true,
        };
      });
    });
    // The angle that should follow `before` after `dt` ms, compared on the circle.
    const offBy = (before, after) => {
      const want = (before.turn + (after.now - before.now) / before.ms) % 1;
      const d = Math.abs(after.turn - want);
      return Math.min(d, 1 - d);
    };
    // 50 ms of a turn: three frames of slack for a loaded machine, a fifth of what a restart loses.
    const SLACK = (b) => 50 / b.ms;
    await settledFrames(page);
    let before = await turns();
    assert(before.length === 2 && before.every((b) => typeof b.turn === "number"), "two turning spinners: " + JSON.stringify(before));
    for (const [n, label] of [[400_000_000, "400 MB of 1.06 GB"], [500_000_000, "500 MB of 1.06 GB"], [600_000_000, "600 MB of 1.06 GB"]]) {
      // A reading lands partway through a turn, as the real counter's do.
      await page.waitForTimeout(170);
      await page.evaluate((line) => window.__RICHOS_MOCK__.setupEmit(line), toolsLine(n, 1_061_655_396, true, label));
      await settledFrames(page);
      const after = await turns();
      for (let i = 0; i < 2; i++) {
        assert(offBy(before[i], after[i]) <= SLACK(before[i]),
          "the " + after[i].c + " spinner jumped " + Math.round(360 * offBy(before[i], after[i])) + " degrees at the " + label + " reading");
        assert(after[i].same, "the " + after[i].c + " spinner was rebuilt by the " + label + " reading");
      }
      before = after;
    }
    // START APPEARS while the voice row still counts (`lockSetupHeight` measures the finished
    // sheet in the same task). The voice spinner carries on from where it was.
    const voiceBefore = before.find((b) => b.c === "media-tools");
    await page.evaluate(() => window.__RICHOS_MOCK__.setupHold(false));
    await page.waitForSelector("#setup-start:not([hidden])");
    await settledFrames(page);
    const voiceAfter = (await turns()).find((b) => b.c === "media-tools");
    assert(voiceAfter && typeof voiceAfter.turn === "number", "the voice row still turns once Start is up");
    assert(offBy(voiceBefore, voiceAfter) <= SLACK(voiceBefore),
      "the voice spinner jumped " + Math.round(360 * offBy(voiceBefore, voiceAfter)) + " degrees as Start appeared");
    assert(page.__errors.length === 0, "the shell logged errors: " + page.__errors.join(" | "));
    bump(15);
    await page.close();
    return "two spinners kept their angle through three counter readings and the voice spinner through Start appearing (within 50 ms of a turn)";
  });

  await run.check("30  round 19, state 2: the installing row's spinner is round 19's ring, its arc on a track", async () => {
    // ROUND 19 (design/mockups/rounds/round-19/dictation.html, `.ring`): an 18px ring, a 2.5px
    // track in --line all the way round, the arc on its top, one turn a second; NOTES.md judges
    // "a spinner by its moving gold arc, not its faint track". Nightly 54 drew a 22px arc in a
    // 2px border with no track, turning every 0.9 s (his recording, animations.mov). The arc keeps
    // --live-mark rather than the round's --gold, the contrast substitution style.css's table
    // already records (--gold is 2.55:1 on the light track).
    const page = await openApp(browser, R19);
    await page.waitForSelector("#setup-sheet:not([hidden])");
    await page.evaluate((line) => window.__RICHOS_MOCK__.setupEmit(line), toolsLine(300_000_000, 1_061_655_396, true, "300 MB of 1.06 GB"));
    await page.evaluate(() => window.__RICHOS_MOCK__.setupHold(true));
    await page.click("#setup-go");
    await page.waitForSelector('#setup-steps .setup-step.is-now[data-component="media-tools"]');
    const read = () => page.evaluate(() => {
      const mark = document.querySelector('#setup-steps .setup-step.is-now[data-component="media-tools"] .setup-step-mark');
      const cs = getComputedStyle(mark);
      const probe = document.createElement("span");
      probe.style.color = "var(--line)";
      document.body.append(probe);
      const line = getComputedStyle(probe).color;
      probe.remove();
      // Round 19's 2.5px stroke as THIS page computes it: WebKit snaps a border to device pixels
      // (2px at a scale factor of 1, 2.5px on a Retina screen), and the round's own CSS snaps alike.
      const stroke = document.createElement("span");
      stroke.style.cssText = "display:block;border:2.5px solid";
      document.body.append(stroke);
      const roundStroke = getComputedStyle(stroke).borderRightWidth;
      stroke.remove();
      const live = document.createElement("span");
      live.style.color = "var(--live-mark)";
      document.body.append(live);
      const arc = getComputedStyle(live).color;
      live.remove();
      // Layout size, not getBoundingClientRect: a turning ring's box is its rotated bounds.
      return {
        w: mark.offsetWidth, h: mark.offsetHeight,
        stroke: cs.borderRightWidth, roundStroke, track: [cs.borderRightColor, cs.borderBottomColor, cs.borderLeftColor],
        top: cs.borderTopColor, line, arc, dur: cs.animationDuration, name: cs.animationName,
      };
    });
    for (const theme of ["dark", "light"]) {
      await page.evaluate((t) => document.documentElement.setAttribute("data-theme", t), theme);
      const m = await read();
      assertEqual(m.w + "x" + m.h, "18x18", theme + ": the ring's size");
      assertEqual(m.stroke, m.roundStroke, theme + ": the ring's stroke is round 19's 2.5px");
      assert(m.track.every((c) => c === m.line), theme + ": the track is --line all the way round: " + JSON.stringify(m));
      assertEqual(m.top, m.arc, theme + ": the arc is --live-mark");
      assertEqual(m.name + " " + m.dur, "setup-step-spin 1s", theme + ": one turn a second");
    }
    await page.evaluate(() => window.__RICHOS_MOCK__.setupHold(false));
    assert(page.__errors.length === 0, "the shell logged errors: " + page.__errors.join(" | "));
    bump(10);
    await page.close();
    return "18px ring, 2.5px --line track, --live-mark arc, 1 s a turn, in both themes";
  });

  await run.check("11  this suite actually checked something", async () => {
    assert(
      assertions >= 40,
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
      : "\na customer's Mac is asked once, in his language, and told the truth when it fails."
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
//  1   main.js `init`: call maybeAskAboutMemory() unconditionally
//        -> two modal dialogs at once on a customer's Mac
//  1b  main.js `closeSetupSheet`: drop the memoryQuestionDeferred branch
//        -> the memory and company questions are asked never
//  2   setup.rs `Component::why`: return "~/.local/bin/claude"
//        -> a path reaches his screen
//  2b  index.html: add an <input> to #setup-sheet
//        -> the CEO is asked to type something
//  3   setup_view.rs: blank SETUP_ACCOUNT_NOTE
//        -> D is sold as zero-touch, which row 3.14 forbids
//  3b  index.html: move #setup-account below the actions
//        -> the caveat becomes a footnote after the decision
//  4   main.js `openSetupSheet`: stop rendering item.why
//        -> the sheet becomes a package list
//  5   setup_view.rs `ask_for`: set can_install true when blocked
//        -> a button is drawn that will certainly fail
//  6   main.js `runSetup`: replace the catch body with console.error(e)
//        -> the failure dies in a console the CEO does not have
//  6b  main.js `runSetup`: leave setupGoEl disabled after a failure
//        -> a failure becomes a dead end
//  7   main.js: delete the Bridge.listen("richos://setup") block
//        -> the sheet sits silent for the whole run
//  7b  main.js `runSetup`: hard-code the finished sentence to "ready"
//        -> a run that left something missing claims it did not
// 14   main.js: restore setupSheetEl.addEventListener("click", e => { if (e.target ===
//        setupSheetEl) closeSetupSheet(); })
//        -> a click beside the panel skips the engine install and advances to the corpus
//           question, which is published v1.0.1's behavior and ray-opus-a2's dead app
// 14b  main.js: add setupSheetEl to the global Escape handler'''s list
//        -> Escape skips the engine install the same way
//  8   main.js `maybeAskAboutSetup`: return true for every status
//        -> a set-up machine is interrupted on every launch
//  9   main.js `runSetup`: drop `setupGoEl.disabled = true`
//        -> a double press starts two copies of Anthropic's installer
// 10   main.js: add a second run_setup call site
// 10b  updates.js: `invoke("run_setup", {})` in a function nothing calls
//        -> `run_setup is invoked from 2 place(s) across the 12 shipped UI file(s):
//           updates.js:831, main.js:3006`
//        THIS IS THE ONE THAT MATTERS. Before 2026-09-05 this check read `main.js` alone,
//        so a second door in any of the other eleven shipped files left it printing "one
//        call site" in green — under a comment saying "A SECOND DOOR IS A SECOND PLACE FOR
//        THE GUARD TO BE MISSING". The door it was watching was one of twelve.
// 12   main.js `refreshVoiceReadiness`: default voiceAvailable to true on an unknown answer
//        -> ◉ is offered on a machine with no speech model, and the mic goes hot
// 12b  main.js `renderFirstRun`: append GREETING_VOICE_INVITE unconditionally
//        -> the first sentence a customer reads names a control that cannot work
// 13   main.js `refreshVoiceReadiness`: hide ◉ unconditionally
//        -> voice is deleted rather than withheld, on every machine
// 20   main.js: delete the `syncQuestionInert` observer
//        -> the composer, the rail gear and the top-right Settings control all take focus
//           behind an unanswered engine offer, and the settings menu opens over it — which is
//           the shipped behavior at 5f3a1a1e and Ray's candidate .16 row B1b
// 20b  main.js `syncQuestionInert`: drop the `isPainted(question)` line
//        -> a full-screen surface mounted over the question is marked inert, which removes it
//           from hit testing, which blinds `isPainted()` — and `escape.js` B8 goes red
// 20c  main.js `syncQuestionInert`: mark the ancestors instead of their other children
//        -> the question inerts itself and the app has no way out at all
// 21   main.js: restore `Bridge.listen("rich://voice-error", ({payload}) => { if (!voiceMode)
//        return; richVoiceSays(payload.message); })`
//        -> RUN, not reasoned about: the old handler was put back, `node setup.js` reported
//           "a spoken turn was refused for the setting up and the offer did not come back",
//           and it was taken out again. The backend's own sentence promises the sheet is on
//           screen, and nothing was putting it there.
// 21b  main.js: drop the `items.length` condition and call `maybeAskAboutSetup()` on every
//        voice error
//        -> the negative control goes red: a lost microphone opens the first-run offer on a
//           machine that has everything

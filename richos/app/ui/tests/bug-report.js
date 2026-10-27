// BUST A BUG — round 21 in the shipping renderer (CEO §115, 2026-10-09).
//
// His words are the acceptance criterion: "When the user has Claude set up in the app (which is
// the expected default), we should just let the user say what's wrong and where and let their
// Rich check and articulate everything properly and then submit a GitHub issue on their behalf."
//
// What this suite holds the window to, against `mock.js`'s stand-in for the shell's six
// `bug_report_*` commands (the shell's own half, and what is private, are tested in Rust:
// crates/richos-core/tests/bug_report_tests.rs):
//
//   0. ONE PLACE (CEO, 2026-10-10: "pressing Bust a bug always shows the bug report on top, ready
//      to answer"). The report always lives in the Rich panel, never in the conversation: over
//      every overlay `lib/overlays.js` finds, plus Dictation and the bare conversation, with
//      plain typing, Talk to Rich on, and Dictation waiting for a key, it is on top with the keys in its answer box, takes typing, Enter and its talk
//      button, Change it takes typing, and nothing goes to the conversation.
//   1. Bust a bug starts the exchange with Rich in the Rich panel; no notice.
//   2. The draft is shown before anything is sent: nothing reaches `bug_report_send` until Send.
//   3. Private details are left out: the card shows stand-ins, each saying what it replaced only
//      to the user, and the sheet that is sent carries the stand-ins and never the names.
//   4. Send: the card says Sent with the issue number, from the RichOS reporting account.
//   5. A failed send is kept and resent: offline, the report waits on this Mac and goes out by
//      itself when the connection is back, and Rich and a notice say so.
//   6. Change it (in place, with the privacy heads-up, and by telling Rich), Cancel, and one
//      report at a time.
//   7. Over a window, the report is in the Rich panel beside it and the window stays open.
//   8. Both themes: every word on the card and in the panel meets WCAG AA and the 16px floor,
//      and every indicator 3:1, computed in WebKit (the "changing" and "canceling" states too).
//   9. The second review's cases (rv-20261009T102303Z-1c3dda1d-4c78, on 1c3dda1dc), each red
//      there: Rich's private words kept through changes, Send and Cancel held during a change,
//      Cancel confirmed (refused, or a retry already sending), Change it on a report that went
//      out, and the screen's words given to Rich with the window's picture asked for first.
//      `mock.js` holds a change or a cancel, fails a cancel, or files a waiting report mid-cancel.
//  10. The third review's cases (rv-20261009T135204Z-9f4d77d4-1892, on 9f4d77d44), each red
//      there: a name typed over a stand-in gets the heads-up before sending, a file address and
//      a drive path typed in get it too, and the digest of what Rich checked is 16px and no
//      longer exempt from the size check.
//  11. The fourth review's cases (rv-20261009T142223Z-3d74fe4c-3b6d, on 3d74fe4cb), each red
//      there: the heads-up is the core's answer (`bug_report_private_in`), so alice@büro.de is
//      named; and the voice hold follows the report's conversation, so speech reaches Rich after
//      leaving it. `mock.js` answers `bug_report_private_in` with `preset.bugPrivateWords`.
//  12. The review of 84d1bdced (rv-20261009T151254Z-84d1bdce-20df), red there: Send waits for
//      the privacy answer on the latest words, so the heads-up comes before the report goes out.
//      `mock.js` holds those answers with `preset.bugPrivateHold` until `releasePrivate()`.
//  13. The review of 3007e3200 (rv-20261009T174727Z-3007e320-578a), red there: words changed by
//      hand go out only after Rich has checked them (shown again with what he left out), and
//      wait on this Mac, unsent, when Claude can't check them. Whether a sheet may go is the
//      shell's decision (`richos_core::bug_report::decide`, tested in Rust); `mock.js` decides it
//      the same way and `filed()` is what actually went out.
//  14. The review of 665df1bb3 (rv-20261009T223503Z-665df1bb-17bf), red there: the heads-up
//      reads a line break typed in a change the way Send does, so a name on two lines is named.
//      `mock.js` matches a name across any whitespace, as the core does. (Its other case, Bust a
//      bug pressed again over Corrections, cannot happen since item 20.)
//  15. The review of b3b5c573d (rv-20261009T225657Z-b3b5c573-96c0), each red there: what Send reads
//      is what the card shows when a step holds a nested list, and a rich-text paste (a nested
//      list, a table, styled words) comes in as plain text. `mock.js` writes steps from
//      `preset.bugSteps`.
//  16. The review of ec2349eee (rv-20261010T121456Z-ec2349ee-5ac3), finding 2, red on 26a0213b5:
//      a new report started over Dictation, opened from Settings for the first time, put the Rich
//      panel under Dictation's sheet. The panel is the app's top layer now (one rule in
//      `style.css`), checked over every overlay `lib/overlays.js` derives from the shipped CSS.
//  17. The reviews of 4cc6e34db (rv-20261010T150403Z-4cc6e34d-61db, rv-20261010T150420Z-4cc6e34d-84f1):
//      Talk to Rich turned on before Bust a bug. Since the one-place design (item 0) the report
//      is in the panel then too, voice mode goes off, and the panel's talk button takes the answer.
//  18. The reviews of cac278721 (rv-20261010T153832Z-cac27872-e986, rv-20261010T153823Z-cac27872-1b7a),
//      each red there: a report in the Rich panel takes the input alone. It ends Dictation's
//      "Press a different key" through its own Cancel (the tool's key tap disarmed with it) and
//      turns conversation voice mode off through its own off path; the panel's talk button is
//      how the user speaks to it.
//  19. The reviews of f29db7082 (rv-20261010T180327Z-f29db708-24e0, rv-20261010T180313Z-f29db708-733e),
//      each red there: the panel's microphone is on only while the report waits for the user's
//      words, so while Rich checks it is stopped first and only then is the hold released (read
//      from the order of `stop_voice_capture` and `bug_report_voice`, as the mock does not run the
//      shell's submission). Stand-ins in a change are marked and counted in Rust
//      (`stand_ins_in_a_change_are_marked_and_counted_as_in_the_first_draft`).
//  20. The reviews of fe4cd6b14 (rv-20261010T183313Z-fe4cd6b1-1e66, rv-20261010T183337Z-fe4cd6b1-ce45)
//      and the CEO (2026-10-10), each red there: every stand-in in the card's final words is
//      marked and counted, whoever wrote it; a microphone start that waited for a stop runs only
//      if the panel's talk is still on; and while a report is open, from the press until it is
//      sent, canceled or its panel closed, Bust a bug in the Settings menu is disabled, so a press
//      is always a first press.
//  21. Nightly 49's walk (docs/verification/2026-10-10-nightly-49-candidate-walk.md), D28, red on
//      13aa9bc0f: while a report is open in the Rich panel, nothing moves focus to the
//      conversation's message box. `mock.js` holds "Not now" with `onboardingDeclineHold` until
//      `__RICHOS_MOCK__.releaseDecline()`, so it finishes while the panel is open, as on the guest.
//      D29, red on 13aa9bc0f: after Change it and Done with the button row floating over a long
//      report, the row is a new box (the shipped WebKit painted the refilled old one only where
//      Done had been), and all three buttons are drawn.
"use strict";

const path = require("path");
const fs = require("fs");
const { loadPlaywright, createRun, assert, assertEqual, UI_DIR, leaveHome, bootSettled, shellSettled, openThread, HOLD_CURTAIN } = require("./lib/harness");
const contrast = require("./lib/contrast");
const coverings = require("./lib/overlays");
const PNG = require("./lib/png");

const PRIVATE = ["Acme deal", "Northwind Traders", "/Users/you/Projects/northwind/notes.txt"];
const ANSWER =
  "In the Acme deal chat the names on the left get cut off when I make the text bigger. " +
  "Northwind Traders keeps its notes in /Users/you/Projects/northwind/notes.txt and those vanish too.";
const SHOTS = process.env.RICHOS_BUG_SHOTS || "";

async function main() {
  const run = createRun("Bust a bug: Rich writes it up, the user approves it, and it is filed");
  const browser = await loadPlaywright().webkit.launch();
  const errors = [];

  async function open(theme, preset, viewport) {
    const page = await browser.newPage({ viewport: viewport || { width: 1440, height: 900 }, locale: "en-US" });
    page.setDefaultTimeout(30000); // a hang guard only; every wait below waits for a fact
    page.on("pageerror", (e) => errors.push(String(e)));
    await page.addInitScript(({ theme, preset }) => {
      localStorage.setItem("richos-theme", theme);
      localStorage.setItem("richos-mock-config", JSON.stringify({ theme, font_scale: 100 }));
      window.__RICHOS_MOCK_PRESET__ = Object.assign({ bugWriteMs: 300 }, preset);
    }, { theme, preset: preset || {} });
    await page.goto("file://" + path.join(UI_DIR, "index.html"));
    await leaveHome(page);
    await bootSettled(page);
    await openThread(page, "acme");
    await page.evaluate((t) => document.documentElement.setAttribute("data-theme", t), theme);
    return page;
  }
  async function bustABug(page) {
    await page.click("#set-btn");
    await page.waitForSelector("#bug-btn");
    await page.click("#bug-btn");
  }
  async function answer(page, text) {
    await page.fill("#bugdock-input", text);
    await page.keyboard.press("Enter");
    await page.waitForSelector(".bugcard .bug-pill:has-text('Not sent yet')");
  }
  /// Rich's last words in the exchange: his line(s), without the speaker label.
  const lastSaid = (page, scope) =>
    page.locator((scope || "#bugdock-msgs") + " .bug-rich").last().locator(".tl-prose").allInnerTexts().then((t) => t.join("\n"));
  const calls = (page, cmd) => page.evaluate((c) => window.__RICHOS_MOCK_BUG__.calls.filter((x) => x.cmd === c), cmd);
  /// Every sheet that actually went out (not every one Send was pressed for: the shell decides).
  const filed = (page) => page.evaluate(() => window.__RICHOS_MOCK_BUG__.filed());
  const settle = (page) => page.evaluate(() => Promise.all(document.getAnimations().filter((a) => a.effect && a.effect.getTiming().iterations !== Infinity).map((a) => a.finished.catch(() => null))));
  async function shot(page, name) {
    if (!SHOTS) return;
    fs.mkdirSync(SHOTS, { recursive: true });
    await settle(page);
    await page.screenshot({ path: path.join(SHOTS, name + ".png") });
  }

  await run.check("Bust a bug starts the exchange with Rich in the Rich panel, not a notice, and nothing of it in the conversation", async () => {
    const page = await open("dark");
    const before = await page.locator("#messages .tl-user").count();
    await bustABug(page);
    await page.waitForSelector("#bugdock:not([hidden]) #bugdock-msgs .bug-rich");
    assertEqual(await page.locator("#conversation .bug-rich, #conversation .bugcard").count(), 0, "the report is in the conversation");
    await page.waitForFunction(() => document.activeElement === document.getElementById("bugdock-input"));
    assertEqual(await page.locator("#bugdock-where").innerText(), "· Acme deal", "panel header");
    assert(await page.isHidden("#set-menu"), "Settings stayed open");
    assertEqual(await page.locator("#bug-toast:visible").count(), 0, "the old notice is still what the button does");
    const asked = await page.locator("#bugdock-msgs .bug-rich .tl-prose").first().innerText();
    assert(asked.startsWith("What went wrong? Tell me in your own words, typed or out loud"), asked);
    assert(asked.includes("I've noted that you were on the Acme deal conversation."), asked);
    assertEqual(await page.getAttribute("#bugdock-input", "placeholder"), "Tell Rich what went wrong…", "the composer does not say what it is waiting for");
    assert(await page.isVisible("#bug-never-mind"), "no way out before answering");
    await shot(page, "ask-dark");
    // Never mind ends it with nothing written up, and the composer is Rich's again.
    await page.click("#bug-never-mind");
    await page.waitForSelector("#bugdock-msgs .bug-rich:has-text('No problem. Nothing was written up or sent.')");
    assertEqual(await page.getAttribute("#bugdock-input", "placeholder"), "Talk to Rich…", "the composer still belongs to the report");
    assertEqual(await page.locator("#messages .tl-user").count(), before, "a message went to Rich's conversation");
    await page.close();
    return asked;
  });

  await run.check("the whole report is shown before anything is sent, and private details are left out", async () => {
    const page = await open("dark");
    const before = await page.locator("#messages .tl-user").count();
    await bustABug(page);
    await page.fill("#bugdock-input", ANSWER);
    await page.keyboard.press("Enter");
    await page.waitForSelector(".bug-working:has-text('Rich is checking…')");
    await page.waitForSelector(".bugcard .bug-pill:has-text('Not sent yet')");
    // The words were the report's: in the exchange, and not a message to Rich's conversation.
    assertEqual(await page.locator("#messages .tl-user").count(), before, "the answer went to Rich's conversation");
    assert((await page.locator("#bugdock-msgs .bug-user").innerText()).includes("Acme deal chat"), "the answer is not shown as the user's");
    assertEqual((await calls(page, "bug_report_write")).length, 1, "Rich was not asked to write it up");
    assertEqual((await calls(page, "bug_report_send")).length, 0, "something was sent before Send");
    assert((await page.locator(".bug-turn .tl-duration-label").innerText()).startsWith("Worked for "), "no Worked for line");
    // What goes to GitHub is on the card, and the names are not.
    const sheetText = await page.locator(".bugcard .bug-doc").innerText();
    for (const p of PRIVATE) assert(!sheetText.includes(p), `"${p}" is on the report: ${sheetText}`);
    const subs = await page.locator(".bugcard .bug-sub").evaluateAll((n) => n.map((s) => [s.textContent, s.dataset.was]));
    for (const p of PRIVATE) assert(subs.some(([, was]) => was === p), `no stand-in for "${p}": ${JSON.stringify(subs)}`);
    assert(subs.every(([text]) => /^\[.+\]$/.test(text)), JSON.stringify(subs));
    // Each stand-in is MARKED: a dotted rule under it, drawn as a border because the test VM's
    // WKWebView painted no `text-decoration` underline there (walk-aba5c01c19ce).
    const marks = await page.locator(".bugcard .bug-sub").evaluateAll((n) => n.map((s) => { const c = getComputedStyle(s); return c.borderBottomStyle + " " + parseFloat(c.borderBottomWidth); }));
    assert(marks.every((m) => m === "dotted 2"), "a stand-in is not marked: " + JSON.stringify(marks));
    const from = await page.locator(".bugcard .r-from").innerText();
    assertEqual(from, "the RichOS reporting account, because RichOS isn't signed in to a GitHub account of yours", "From");
    assert((await page.locator(".bugcard .lo-text").innerText()).startsWith("Left out, because anyone can read GitHub issues: "), "no left-out line");
    // Pointing at a stand-in (and tabbing to one) says what it replaced, to this user only.
    await page.hover(".bugcard .bug-sub >> nth=0");
    await page.waitForSelector("#bug-subtip.is-shown");
    const tip = await page.locator("#bug-subtip").innerText();
    assert(/^Stands in for “.+”, an? .+\. Only you see this; it isn't in the report\.$/.test(tip), tip);
    await page.mouse.move(5, 5);
    await page.focus(".bugcard .bug-sub >> nth=1");
    await page.waitForSelector("#bug-subtip.is-shown");
    await page.locator(".bugcard .bug-sub >> nth=1").blur();
    for (const id of ["#bug-send", "#bug-change", "#bug-cancel"]) assert(await page.isVisible(id), id + " is not offered");
    await shot(page, "report-dark");
    await page.close();
    return `${subs.length} stand-ins; tooltip: ${tip}`;
  });

  // ---- nightly 48's walk, D23 (docs/verification/2026-10-10-nightly-48-candidate-walk.md) ----
  await run.check("stand-ins Rich wrote himself, with nothing listed beside them, are marked and counted, and none is called nothing private", async () => {
    // D23: on d0d06501c a report carrying [a person], [a company], [a conversation] and [a file
    // on this Mac] said "Nothing private was in it, so nothing was left out." and marked none of
    // them, because nothing told the card what they replaced. The shell now sends each one with
    // its kind and no replaced words (`Segment::unaccounted`); `mock.js` does the same with
    // `bugRichWrote`.
    const page = await open("light", {
      bugRichWrote: "In [a conversation], the user made the text bigger and the names got cut off. They say [a person] at [a company] sees the same thing, and their notes in [a file on this Mac] now look empty.",
    });
    await bustABug(page);
    await answer(page, "The names in the sidebar get cut off when I make the text bigger.");
    const subs = await page.locator(".bugcard .bug-doc .bug-sub").evaluateAll((n) => n.map((s) => [s.textContent, s.dataset.kind, s.tabIndex]));
    assertEqual(subs.map(([t]) => t), ["[a conversation]", "[a person]", "[a company]", "[a file on this Mac]"], "a stand-in on the card is not marked: " + JSON.stringify(subs));
    assert(subs.every(([, , tab]) => tab === 0), "a stand-in cannot be reached with Tab: " + JSON.stringify(subs));
    const line = await page.locator(".bugcard .lo-text").innerText();
    assertEqual(
      line,
      "Left out, because anyone can read GitHub issues: one conversation name, one person's name, one company name and one file path. The underlined words stand in for them; point at one to see what it replaced.",
      "the line under the card"
    );
    // Pointing at one says what kind of detail it leaves out, and claims no words it was not given.
    await page.hover(".bugcard .bug-sub >> nth=1");
    await page.waitForSelector("#bug-subtip.is-shown");
    const tip = await page.locator("#bug-subtip").innerText();
    // Its kind alone: the card does not know who wrote a stand-in it has no words for (review
    // rv-20261010T183313Z-fe4cd6b1-1e66: the user can type one too).
    assertEqual(tip, "Stands in for a person's name.", "the stand-in's tip");
    await page.mouse.move(5, 5);
    await page.focus(".bugcard .bug-sub >> nth=3");
    await page.waitForSelector("#bug-subtip.is-shown");
    assertEqual(await page.locator("#bug-subtip").innerText(), "Stands in for a file path.", "the tip on Tab");
    await page.close();
    return `${subs.length} marked; line: ${line}; tip: ${tip}`;
  });

  // ---- the review of fe4cd6b14 (rv-20261010T183313Z-fe4cd6b1-1e66), finding 1 ----
  await run.check("every stand-in in the card's final words is marked and counted, whoever wrote it: edited from another, typed by the user, in Rich's change and in a heading", async () => {
    // Fixture `edited-stand-in.js`: on fe4cd6b14, Change it, "[a person]" edited into "[a company]",
    // Done, and the card said "Nothing private was in it, so nothing was left out." under it. The
    // marking came from where a stand-in was written; now it is read from the card's words.
    const page = await open("dark", {
      bugRichPrivate: [{ text: "Jane Doe", kind: "person_name" }],
      bugRichSection: { heading: "Seen at [a company]", text: "It started after the last update." },
    });
    await bustABug(page);
    await answer(page, "The names on the left get cut off. Jane Doe saw it first.");
    const marks = () => page.locator(".bugcard .bug-doc .bug-sub").evaluateAll((n) => n.map((s) => [s.textContent, s.dataset.kind, s.dataset.was == null ? null : s.dataset.was]));
    const before = await marks();
    assertEqual(JSON.stringify(before), JSON.stringify([["[a person]", "person_name", "Jane Doe"], ["[a company]", "company_name", null]]), "the first draft's stand-ins (the heading's included)");
    const lineBefore = await page.locator(".bugcard .lo-text").innerText();
    assertEqual(lineBefore, "Left out, because anyone can read GitHub issues: one person's name and one company name. The underlined words stand in for them; point at one to see what it replaced.", "the line under the first draft");
    // Change it: the caret is at the end of the title; the user types a stand-in there, then
    // edits the words inside "[a person]" into "[a company]".
    await page.click("#bug-change");
    await page.waitForSelector(".bugcard.is-editing");
    await page.keyboard.type(" in [a folder]");
    await page.evaluate(() => {
      const sub = [...document.querySelectorAll(".bugcard .bug-doc .bug-sub")].find((s) => s.dataset.was === "Jane Doe");
      const words = sub.firstChild;
      sub.closest("[contenteditable]").focus();
      const r = document.createRange();
      r.setStart(words, 3);
      r.setEnd(words, words.length - 1);
      const sel = window.getSelection();
      sel.removeAllRanges();
      sel.addRange(r);
    });
    await page.keyboard.type("company");
    await page.click("#bug-done");
    await page.waitForSelector(".bugcard .bug-pill:has-text('Not sent yet · changed')");
    assert((await page.locator(".bugcard .bug-sec p").first().innerText()).includes("[a company] saw it first"), "the edit did not land inside the stand-in");
    const done = await marks();
    assertEqual(
      JSON.stringify(done),
      JSON.stringify([["[a folder]", "folder_name", null], ["[a company]", "company_name", null], ["[a company]", "company_name", null]]),
      "after Done: the typed one and the edited one marked by their kind, the edited one no longer claiming Jane Doe"
    );
    const lineDone = await page.locator(".bugcard .lo-text").innerText();
    assertEqual(lineDone, "Left out, because anyone can read GitHub issues: one folder name and two company names. The underlined words stand in for them; point at one to see what it replaced.", "the line after Done");
    // Pointing at the edited one says its kind alone.
    await page.hover(".bugcard .bug-sec p .bug-sub");
    await page.waitForSelector("#bug-subtip.is-shown");
    assertEqual(await page.locator("#bug-subtip").innerText(), "Stands in for a company name.", "the edited stand-in's tip");
    await page.mouse.move(5, 5);
    // Told to Rich: his change carries a stand-in.
    await page.fill("#bugdock-input", "Also say [a private word] is cut off as well");
    await page.keyboard.press("Enter");
    await page.waitForSelector(".bug-sec p.is-added");
    const added = await page.locator(".bug-sec p.is-added .bug-sub").evaluateAll((n) => n.map((s) => [s.textContent, s.dataset.kind]));
    assertEqual(JSON.stringify(added), JSON.stringify([["[a private word]", "private_word"]]), "the stand-in in Rich's change");
    const line = await page.locator(".bugcard .lo-text").innerText();
    assertEqual(line, "Left out, because anyone can read GitHub issues: one folder name, two company names and one private word. The underlined words stand in for them; point at one to see what it replaced.", "the line after Rich's change");
    assertEqual((await marks()).length, 4, "stand-ins marked on the card");
    await page.close();
    return line;
  });

  await run.check("Send files it, from the RichOS reporting account, with the words the card showed", async () => {
    const page = await open("dark");
    await bustABug(page);
    await answer(page, ANSWER);
    const shown = await page.locator(".bugcard .bug-doc").innerText();
    await page.click("#bug-send");
    await page.waitForSelector(".bugcard.is-sent .bug-pill:has-text('Sent · #412')");
    const said = await lastSaid(page);
    assert(said.includes("Sent. It's issue #412 on GitHub, filed from the RichOS reporting account, because RichOS isn't signed in to a GitHub account of yours."), said);
    assert(said.includes("github.com/WebDevBooster/richos/issues/412"), said);
    const sent = (await calls(page, "bug_report_send"))[0].sheet;
    const words = [sent.title, ...sent.sections.flatMap((s) => [s.heading, ...s.paragraphs, ...s.steps])].join("\n");
    for (const p of PRIVATE) assert(!words.includes(p), `"${p}" was sent: ${words}`);
    // Word for word what the card showed (the card's text is the title and sections in order).
    assertEqual(words.replace(/\s+/g, " ").trim(), shown.replace(/\s+/g, " ").trim(), "what was sent is not what was shown");
    // The link opens the page the relay answered with; the shell opens it only when it is an
    // issue of RichOS's repository (`bug::page_to_open`).
    await page.click(".bug-sentrow .bug-link");
    assertEqual((await calls(page, "bug_report_open_issue"))[0].url, "https://github.com/WebDevBooster/richos/issues/412", "Open issue");
    assertEqual(await page.getAttribute("#bugdock-input", "placeholder"), "Talk to Rich…", "the composer still belongs to the report");
    await shot(page, "sent-dark");
    await page.close();
    return said.split("\n")[0];
  });

  await run.check("a failed send is kept on this Mac and goes out by itself when the connection is back", async () => {
    const page = await open("dark", { bugNet: "offline" });
    await bustABug(page);
    await answer(page, ANSWER);
    await page.click("#bug-send");
    await page.waitForSelector(".bugcard.is-queued .bug-pill:has-text('Waiting to send · saved on this Mac')");
    const line = await lastSaid(page);
    assert(line.includes("This Mac is offline, so the report didn't go out. Nothing is lost: it's saved on this Mac exactly as you approved it."), line);
    for (const id of ["#bug-try-now", "#bug-change", "#bug-cancel"]) assert(await page.isVisible(id), id + " is not offered");
    assertEqual((await page.evaluate(() => window.__RICHOS_MOCK_BUG__.waiting())).length, 1, "the report is not kept");
    await shot(page, "queued-dark");
    // Try now while still offline: it stays waiting and says so.
    await page.click("#bug-try-now");
    await page.waitForSelector(".bugcard.is-queued");
    // Back online: it goes out by itself, and Rich and a notice say so.
    await page.evaluate(() => window.__RICHOS_MOCK_BUG__.setNet("online"));
    await page.waitForSelector(".bugcard.is-sent .bug-pill:has-text('Sent · #412')");
    const back = await lastSaid(page);
    assert(back.startsWith("You're back online, so I sent your bug report. It's issue #412 on GitHub,"), back);
    await page.waitForSelector("#bug-toast:has-text('Your bug report went out: issue #412 on GitHub.')");
    assertEqual((await page.evaluate(() => window.__RICHOS_MOCK_BUG__.waiting())).length, 0, "still waiting after it went out");
    await page.close();
    return back.split("\n")[0];
  });

  await run.check("GitHub down says so, and a report waiting to send that is changed waits for Send again", async () => {
    const page = await open("dark", { bugNet: "down" });
    await bustABug(page);
    await answer(page, "Not now fades a suggestion but leaves it.");
    await page.click("#bug-send");
    await page.waitForSelector(".bugcard.is-queued");
    const line = await lastSaid(page);
    assert(line.startsWith("GitHub isn't answering right now, so the report didn't go out."), line);
    await page.click("#bug-change");
    await page.waitForSelector(".bugcard.is-editing");
    assertEqual((await page.evaluate(() => window.__RICHOS_MOCK_BUG__.waiting())).length, 0, "the old copy still waits to go out");
    await page.click("#bug-done");
    await page.waitForSelector(".bugcard .bug-pill:has-text('Not sent yet · changed')");
    await page.close();
    return line.split(".")[0];
  });

  await run.check("Change it: in place with a privacy heads-up, by telling Rich, and Cancel", async () => {
    const page = await open("dark");
    await bustABug(page);
    await answer(page, "The names on the left get cut off when the text is bigger.");
    await page.click("#bug-change");
    await page.waitForSelector(".bugcard.is-editing .bug-pill:has-text('Changing it')");
    assert(await page.isVisible(".bug-edithint"), "no hint while changing it");
    assertEqual(await page.evaluate(() => document.activeElement.classList.contains("bug-title")), true, "the title is not where the cursor is");
    await page.keyboard.type(" for Acme deal");
    await page.waitForSelector(".bug-warn:not([hidden])");
    const warn = await page.locator(".bug-warn").innerText();
    assertEqual(warn, "“Acme deal” looks private. Anyone can read this report on GitHub.", "heads-up");
    await shot(page, "warn-dark");
    await page.click("#bug-done");
    await page.waitForSelector(".bugcard .bug-pill:has-text('Not sent yet · changed')");
    assert(await page.isVisible(".bug-warn"), "the heads-up went away while the private word is still there");
    // Told to Rich: he adds it to What happened and says so.
    await page.fill("#bugdock-input", "Also say it happens in the light theme too");
    await page.keyboard.press("Enter");
    await page.waitForSelector(".bug-sec p.is-added");
    assertEqual(await page.locator(".bug-sec p.is-added").innerText(), "It happens in the light theme too.", "the change");
    await page.waitForSelector("#bugdock-msgs .bug-rich:has-text('Added that to What happened, above. It still waits for you to send it.')");
    await shot(page, "changed-dark");
    await page.click("#bug-cancel");
    await page.waitForSelector(".bugcard.is-canceled .bug-pill:has-text('Canceled · nothing sent')");
    assert((await page.locator(".bug-ctitle").innerText()).includes("for Acme deal"), "the folded card does not show its title");
    await page.waitForSelector("#bugdock-msgs .bug-rich:has-text('Canceled. Nothing was sent.')");
    assertEqual((await calls(page, "bug_report_send")).length, 0, "a canceled report was sent");
    await shot(page, "canceled-dark");
    await page.close();
    return warn;
  });

  // ---- a line break typed in a change (CEO §115 brief, item 4) ----
  for (const key of ["Enter", "Shift+Enter"]) {
    await run.check(`a line break typed with ${key} in a change joins no two words, on the card or in what is sent`, async () => {
      // On a1d158b35 both keys made WebKit put "Send does nothing twice." in a <div> inside the
      // paragraph, `textContent` dropped the break, and the sheet sent read "ClickSend".
      const page = await open("dark");
      await bustABug(page);
      await answer(page, "The window froze while opening the plan.");
      await page.click("#bug-change");
      await page.waitForSelector(".bugcard.is-editing");
      // The cursor at the end of the first paragraph, as a click after its last word puts it.
      await page.evaluate(() => {
        const p = document.querySelector(".bugcard .bug-sec p");
        p.focus();
        const r = document.createRange();
        r.selectNodeContents(p);
        r.collapse(false);
        const sel = window.getSelection();
        sel.removeAllRanges();
        sel.addRange(r);
      });
      await page.keyboard.type(" Click");
      await page.keyboard.press(key);
      await page.keyboard.type("Send does nothing twice.");
      const html = await page.locator(".bugcard .bug-sec p").first().evaluate((p) => p.innerHTML);
      await page.click("#bug-done");
      await page.waitForSelector(".bugcard .bug-pill:has-text('Not sent yet · changed')");
      await page.click("#bug-send");
      await page.waitForSelector(".bugcard.is-sent");
      const sent = (await calls(page, "bug_report_send"))[0].sheet.sections[0].paragraphs.join("\n");
      const words = sent.split(/\s+/);
      for (const w of ["Click", "Send"]) assert(words.includes(w), `"${w}" is not a word of what was sent: ${JSON.stringify(words)} (the paragraph was ${html})`);
      assert(!sent.includes("ClickSend"), "two words were joined: " + JSON.stringify(sent));
      const out = JSON.stringify(await filed(page));
      assert(!out.includes("ClickSend") && out.includes("Click\\nSend"), "what went out: " + out);
      await page.close();
      return `${key}: ${html} sent as ${JSON.stringify(sent)}`;
    });
  }

  // ---- the second review's cases (rv-20261009T102303Z-1c3dda1d-4c78, on 1c3dda1dc) ----
  const prose = (page) => page.locator("#bugdock-msgs .bug-rich .tl-prose").allInnerTexts().then((t) => t.join("\n"));

  await run.check("a private name Rich found stays left out through every change, and the heads-up names it", async () => {
    // Finding 4: on the tip Rich's own private words applied to the first draft only.
    const page = await open("dark", { bugRichPrivate: [{ text: "Jane Doe", kind: "person_name" }] });
    await bustABug(page);
    await answer(page, "The names on the left get cut off. Jane Doe saw it first.");
    assert(!(await page.locator(".bugcard .bug-doc").innerText()).includes("Jane Doe"), "Rich's private name is on the first draft");
    await page.fill("#bugdock-input", "Also say Jane Doe saw it in light mode too");
    await page.keyboard.press("Enter");
    await page.waitForSelector(".bug-sec p.is-added");
    const asked = (await calls(page, "bug_report_change"))[0];
    assert((asked.private || []).some((t) => t.text === "Jane Doe"), "the change was asked without the report's private words: " + JSON.stringify(asked.private));
    const added = await page.locator(".bug-sec p.is-added").innerText();
    assert(!added.includes("Jane Doe") && added.includes("[a person]"), "the change put the name back: " + added);
    await page.click("#bug-change");
    await page.waitForSelector(".bugcard.is-editing");
    await page.keyboard.type(" Jane Doe");
    await page.waitForSelector(".bug-warn:not([hidden])");
    const warn = await page.locator(".bug-warn").innerText();
    assertEqual(warn, "“Jane Doe” looks private. Anyone can read this report on GitHub.", "heads-up");
    await page.close();
    return added + " / " + warn;
  });

  // ---- the third review's cases (rv-20261009T135204Z-9f4d77d4-1892, on 9f4d77d44) ----
  await run.check("a name typed over a stand-in is the user's own text: the heads-up names it before sending", async () => {
    // Finding 2, fixture `edited-stand-in.js`: on the tip the warning skipped every stand-in span,
    // even one whose text the user had replaced, and Send then filed the name with no heads-up.
    const page = await open("dark", { bugRichPrivate: [{ text: "Jane Doe", kind: "person_name" }] });
    await bustABug(page);
    await answer(page, "The names on the left get cut off. Jane Doe saw it first.");
    await page.click("#bug-change");
    await page.waitForSelector(".bugcard.is-editing");
    assert(await page.isHidden(".bug-warn"), "a heads-up before anything private was typed");
    // Select the words inside "[a person]" and type the name over them: the text stays in the span.
    await page.evaluate(() => {
      const sub = [...document.querySelectorAll(".bugcard .bug-doc .bug-sub")].find((s) => s.dataset.was === "Jane Doe");
      const words = sub.firstChild;
      sub.closest("[contenteditable]").focus();
      const r = document.createRange();
      r.setStart(words, 1);
      r.setEnd(words, words.length - 1);
      const sel = window.getSelection();
      sel.removeAllRanges();
      sel.addRange(r);
    });
    await page.keyboard.type("Jane Doe");
    await page.waitForSelector(".bug-warn:not([hidden])");
    const warn = await page.locator(".bug-warn").innerText();
    assertEqual(warn, "“Jane Doe” looks private. Anyone can read this report on GitHub.", "heads-up");
    // It is no longer marked as a stand-in: no dotted rule, no "Stands in for" tooltip, not counted.
    const marked = await page.locator(".bugcard .bug-doc .bug-sub").evaluateAll((n) => n.map((s) => s.textContent));
    assert(marked.every((t) => !t.includes("Jane Doe")), "the edited words are still marked as a stand-in: " + JSON.stringify(marked));
    await page.click("#bug-done");
    await page.waitForSelector(".bugcard .bug-pill:has-text('Not sent yet · changed')");
    assert(await page.isVisible(".bug-warn"), "the heads-up went away while the name is still there");
    // Send: words changed by hand are Rich's to check before anything goes, and he leaves the
    // name out (review rv-20261009T174727Z-3007e320-578a finding 1: on 3007e3200 it was filed as
    // typed). The card comes back as it would go, unsent.
    await page.click("#bug-send");
    await page.waitForSelector("#bugdock-msgs .bug-rich:has-text('I checked it again before sending')");
    assertEqual((await filed(page)).length, 0, "the name went out before Rich checked it");
    assert(!(await page.locator(".bugcard .bug-doc").innerText()).includes("Jane Doe"), "the card shown again still has the name");
    await page.click("#bug-send");
    await page.waitForSelector(".bugcard.is-sent");
    const out = JSON.stringify(await filed(page));
    assert(!out.includes("Jane Doe") && out.includes("[a person]"), "what went out: " + out);
    await page.close();
    return warn;
  });

  await run.check("a file address or a drive path typed into the report gets the heads-up too", async () => {
    // Finding 1's starts. That the core finds them is Rust's test (`a_file_url_is_left_out`,
    // `every_named_path_start_hides_its_clause_whatever_comes_before_it`); here, that the
    // heads-up names what the core answered, in the order written.
    const page = await open("dark", { bugPrivateWords: ["file:///Users/you/Secret.xlsx", "C:\\Users\\you\\notes.txt"] });
    await bustABug(page);
    await answer(page, "The names on the left get cut off when the text is bigger.");
    await page.click("#bug-change");
    await page.waitForSelector(".bugcard.is-editing");
    await page.keyboard.type(" in file:///Users/you/Secret.xlsx and C:\\Users\\you\\notes.txt");
    await page.waitForSelector(".bug-warn:not([hidden])");
    const warn = await page.locator(".bug-warn").innerText();
    assertEqual(warn, "“file:///Users/you/Secret.xlsx”, “C:\\Users\\you\\notes.txt” look private. Anyone can read this report on GitHub.", "heads-up");
    await page.close();
    return warn;
  });

  // ---- the fourth review's cases (rv-20261009T142223Z-3d74fe4c-3b6d, on 3d74fe4cb) ----
  await run.check("the heads-up is the core's answer about the edited words: an address like alice@büro.de is named", async () => {
    // Finding 2, fixture `email-warning.js`: on the tip the card kept its own ASCII-only copy of
    // the rules, which missed alice@büro.de while the core leaves it out. Now the card asks the
    // core (`bug_report_private_in`; Rust's `the_heads_up_on_the_users_own_words_is_the_scrubbers_answer`
    // proves the core names it) with the words on the card and the report's private words.
    const page = await open("dark", { bugRichPrivate: [{ text: "Jane Doe", kind: "person_name" }], bugPrivateWords: ["alice@büro.de"] });
    await bustABug(page);
    await answer(page, "The names on the left get cut off. Jane Doe saw it first.");
    await page.click("#bug-change");
    await page.waitForSelector(".bugcard.is-editing");
    await page.keyboard.type(" Write to alice@büro.de");
    // The fact the heads-up waits on: the core was asked about the words now on the card. On the
    // tip it never is (the card decided by itself), so this wait is where the tip fails.
    await page.waitForFunction(() => window.__RICHOS_MOCK_BUG__.calls.some((c) => c.cmd === "bug_report_private_in" && c.text.includes("Write to alice@büro.de")));
    await page.waitForSelector(".bug-warn:not([hidden])");
    const warn = await page.locator(".bug-warn").innerText();
    assertEqual(warn, "“alice@büro.de” looks private. Anyone can read this report on GitHub.", "heads-up");
    const asked = (await calls(page, "bug_report_private_in")).slice(-1)[0];
    assert(asked.text.includes("Write to alice@büro.de"), "the core was not asked about the words on the card: " + JSON.stringify(asked));
    assert(asked.private.some((t) => t.text === "Jane Doe"), "the core was asked without the report's private words: " + JSON.stringify(asked.private));
    // The stand-ins are not the user's words: what the core is asked about leaves them out.
    assert(!asked.text.includes("[a person]"), "a stand-in was asked about: " + asked.text);
    await page.close();
    return warn;
  });

  await run.check("the shell holds speech back only while the panel's talk button is on: Talk to Rich turned on again is Rich's conversation", async () => {
    // Finding 3, fixture `voice-routing.js`: on the tip the shell's hold (`bug_report_voice`)
    // stayed on while what was said was meant for Rich, so the shell dropped every spoken turn
    // while the window drew it as sent. The hold and the report's taking of a transcript follow
    // ONE condition (`takesSpeech`): a report waiting on words with the panel's talk button on.
    const page = await open("dark", { bugVoice: true });
    const lastHold = () => page.evaluate(() => {
      const v = window.__RICHOS_MOCK_BUG__.calls.filter((c) => c.cmd === "bug_report_voice");
      return v.length ? v[v.length - 1].on : null;
    });
    const holding = (on) => page.waitForFunction((want) => {
      const v = window.__RICHOS_MOCK_BUG__.calls.filter((c) => c.cmd === "bug_report_voice");
      return v.length > 0 && v[v.length - 1].on === want;
    }, on);
    const said = (text) => page.evaluate((t) => window.__RICHOS_MOCK_BUG__.say(t), text);
    await bustABug(page);
    await page.waitForSelector("#bugdock:not([hidden]) #bugdock-msgs .bug-rich");
    // Talk to Rich turned on again while the report is open: what is said is Rich's.
    await page.click("#talk-toggle");
    await page.waitForSelector("#voice-panel:not([hidden])");
    assertEqual(await lastHold(), false, "the shell holds speech back with only Talk to Rich on");
    await said("Is the hiring plan ready?");
    await page.waitForSelector("#messages .tl-user:has-text('Is the hiring plan ready?')");
    assertEqual(await page.locator("#bugdock .bug-user").count(), 0, "the report took words said to Rich");
    // The panel's talk button: voice mode goes off by its own path, and the report takes what is said.
    await page.click("#bugdock-talk");
    await holding(true);
    assertEqual(await page.getAttribute("#talk-toggle", "aria-pressed"), "false", "Talk to Rich still on beside the panel's talk button");
    const before = await page.locator("#messages .tl-user").count();
    await said("The names on the left get cut off when the text is bigger.");
    await page.waitForSelector(".bugcard .bug-pill:has-text('Not sent yet')");
    assertEqual(await page.locator("#messages .tl-user").count(), before, "the report's answer went to Rich's conversation");
    // The microphone went off while Rich checked the answer (review rv-20261010T180327Z-f29db708-24e0):
    // the talk button turns it on again for a spoken change.
    assertEqual(await page.getAttribute("#bugdock-talk", "aria-pressed"), "false", "the panel's talk button after Rich checked");
    await holding(false);
    await page.click("#bugdock-talk");
    await holding(true);
    await said("Also say it happens in the light theme too");
    await page.waitForSelector(".bug-sec p.is-added");
    assertEqual(await page.locator(".bug-sec p.is-added").innerText(), "It happens in the light theme too.", "the spoken change");
    // Off again: the hold goes with it.
    await page.click("#bugdock-talk");
    await holding(false);
    const holds = (await calls(page, "bug_report_voice")).map((c) => c.on);
    await page.close();
    return "bug_report_voice: " + holds.join(" → ");
  });

  // ---- the review of 84d1bdced (rv-20261009T151254Z-84d1bdce-20df) ----
  await run.check("Send waits for the privacy check on the latest words, so the heads-up comes before the report goes out", async () => {
    // Finding 2, fixture `warning-function-probe.js`: on the tip, with the core's answers held,
    // Send filed "Write to alice@büro.de" at once and the heads-up never showed. `mock.js` holds
    // every `bug_report_private_in` answer until `releasePrivate()`.
    const sends = (page) => calls(page, "bug_report_send").then((c) => c.length);
    const page = await open("dark", { bugPrivateHold: true, bugPrivateWords: ["alice@büro.de"] });
    await bustABug(page);
    await answer(page, "The names on the left get cut off when the text is bigger.");
    await page.click("#bug-change");
    await page.waitForSelector(".bugcard.is-editing");
    await page.keyboard.type(" Write to alice@büro.de");
    await page.click("#bug-done");
    await page.waitForSelector(".bugcard .bug-pill:has-text('Not sent yet · changed')");
    await page.click("#bug-send");
    assertEqual(await sends(page), 0, "the report went out before the privacy check answered");
    assert(await page.isHidden(".bug-warn"), "a heads-up before the core answered");
    await page.evaluate(() => window.__RICHOS_MOCK_BUG__.releasePrivate());
    await page.waitForSelector(".bug-warn:not([hidden])");
    const warn = await page.locator(".bug-warn").innerText();
    assertEqual(warn, "“alice@büro.de” looks private. Anyone can read this report on GitHub.", "heads-up");
    // The answer brought a heads-up the user had not seen: the report waits under it, unsent.
    await page.evaluate(() => new Promise((r) => setTimeout(r, 50)));
    assertEqual(await sends(page), 0, "the report went out with the heads-up the user had not seen");
    assertEqual(await page.locator(".bugcard .bug-pill").innerText(), "Not sent yet · changed", "the card left the draft");
    // Pressed again, having been told: the words as the user wrote them go to the shell, which
    // has Rich check them first (review rv-20261009T174727Z-3007e320-578a finding 1) and leaves
    // the address out; the card comes back as it would go, and goes on the next press.
    await page.click("#bug-send");
    assert(JSON.stringify((await calls(page, "bug_report_send"))[0].sheet).includes("alice@büro.de"), "Send was not asked with the words as the user wrote them");
    await page.waitForSelector("#bugdock-msgs .bug-rich:has-text('I checked it again before sending')");
    assertEqual((await filed(page)).length, 0, "the address went out before Rich checked the changed words");
    await page.click("#bug-send");
    await page.waitForSelector(".bugcard.is-sent");
    assert(!JSON.stringify(await filed(page)).includes("alice@büro.de"), "the address went out");
    await page.close();

    // A press while the check is out with nothing private to find: it goes once the answer comes.
    const quiet = await open("dark", { bugPrivateHold: true });
    await bustABug(quiet);
    await answer(quiet, "The names on the left get cut off when the text is bigger.");
    await quiet.click("#bug-send");
    assertEqual(await sends(quiet), 0, "the report went out before the privacy check answered");
    await quiet.evaluate(() => window.__RICHOS_MOCK_BUG__.releasePrivate());
    await quiet.waitForSelector(".bugcard.is-sent");
    assertEqual(await sends(quiet), 1, "the held Send did not go out, or went twice");
    await quiet.close();
    return "held until answered; " + warn;
  });

  // ---- the review of 692942153 (rv-20261009T162841Z-69294215-70e6) ----
  const kept = (page) => page.evaluate(() => window.__RICHOS_MOCK_BUG__.unchecked().length);
  const NOT_CHECKED = "I couldn't check this report for private details yet";
  await run.check("when Claude can't check the report, nothing is offered for sending: it waits on this Mac until Rich checks it", async () => {
    // Finding 2, fixture `privacy-probe.py`: on 692942153 a Claude error, timeout or malformed
    // answer put the user's own words on the card, "Jane Doe at SecretCo" included, with Send,
    // under "I left out names, company details and file paths". `mock.js` answers as the shell
    // does now: `{state: "unchecked"}` and the report kept.
    const SAID = "Jane Doe at SecretCo saw the window freeze while opening the plan.";
    const page = await open("dark", { bugClaude: "down", bugRichPrivate: [{ text: "Jane Doe", kind: "person_name" }, { text: "SecretCo", kind: "company_name" }] });
    await bustABug(page);
    await page.fill("#bugdock-input", SAID);
    await page.keyboard.press("Enter");
    await page.waitForSelector(`#bugdock-msgs .bug-rich:has-text("${NOT_CHECKED}")`);
    const said = await lastSaid(page);
    assertEqual(await page.locator(".bugcard").count(), 0, "a report Rich did not check was offered");
    for (const id of ["#bug-send", "#bug-change"]) assertEqual(await page.locator(id).count(), 0, id + " is offered");
    assert(!(await prose(page)).includes("I left out names"), "Rich says he left names out of a report he never checked");
    assertEqual(await kept(page), 1, "the report is not kept on this Mac");
    assertEqual((await calls(page, "bug_report_send")).length, 0, "something was sent");
    assert(await page.isVisible("#bug-unchecked-cancel"), "no way to cancel the report that waits");
    assertEqual(await page.getAttribute("#bugdock-input", "placeholder"), "Talk to Rich…", "the composer still waits for the report");
    await shot(page, "unchecked-dark");
    // Claude answers (twice over, as the shell's loop says it until the window takes it): Rich
    // checks it, and ONE card comes, unsent and without the names.
    await page.evaluate(() => { window.__RICHOS_MOCK_BUG__.claudeAnswers(); window.__RICHOS_MOCK_BUG__.claudeAnswers(); });
    await page.waitForSelector(".bugcard .bug-pill:has-text('Not sent yet')");
    assertEqual(await page.locator(".bugcard").count(), 1, "the check made more than one card");
    const card = await page.locator(".bugcard .bug-doc").innerText();
    assert(!card.includes("Jane Doe") && !card.includes("SecretCo"), "a name is on the checked card: " + card);
    const checkedLine = await lastSaid(page);
    assert(checkedLine.startsWith("I've checked it now. Here's the report as I'd file it. Nothing goes out until you press Send."), checkedLine);
    assert(await page.isHidden("#bug-unchecked-cancel"), "Cancel for the kept report is still offered beside the card");
    await page.waitForFunction(() => window.__RICHOS_MOCK_BUG__.unchecked().length === 0);
    assertEqual((await calls(page, "bug_report_send")).length, 0, "it was sent before the user pressed Send");
    // A change while Claude can't check it adds nothing.
    await page.evaluate(() => window.__RICHOS_MOCK_BUG__.setClaude("down"));
    await page.fill("#bugdock-input", "Also say Marta saw it too");
    await page.keyboard.press("Enter");
    await page.waitForSelector("#bugdock-msgs .bug-rich:has-text(\"I couldn't change the report just now\")");
    assertEqual(await page.locator(".bug-sec p.is-added").count(), 0, "a change Rich did not check was added");
    await page.close();
    return said + " / then: " + checkedLine.split(".")[0];
  });

  await run.check("a report Rich could not check can be canceled, and one kept from before is shown once he checks it", async () => {
    const page = await open("dark", { bugClaude: "down" });
    await bustABug(page);
    await page.fill("#bugdock-input", "The names on the left get cut off.");
    await page.keyboard.press("Enter");
    await page.waitForSelector("#bug-unchecked-cancel");
    await page.click("#bug-unchecked-cancel");
    await page.waitForSelector("#bugdock-msgs .bug-rich:has-text(\"Canceled. Nothing was sent and it's no longer saved on this Mac.\")");
    assertEqual(await kept(page), 0, "the canceled report is still kept");
    // Claude answering afterwards brings nothing back.
    await page.evaluate(() => window.__RICHOS_MOCK_BUG__.claudeAnswers());
    await page.evaluate(() => new Promise((r) => setTimeout(r, 50)));
    assertEqual(await page.locator(".bugcard").count(), 0, "a canceled report came back");
    await page.close();

    // Kept before the app was quit, with no card in this window: when Rich checks it, the Rich
    // panel opens with its card, unsent.
    const later = await open("dark");
    await later.evaluate(() => window.__RICHOS_MOCK_BUG__.keptBefore("The names on the left get cut off.", { here: "the Acme deal conversation" }));
    await later.evaluate(() => window.__RICHOS_MOCK_BUG__.claudeAnswers());
    await later.waitForSelector("#bugdock .bugcard .bug-pill:has-text('Not sent yet')");
    assertEqual(await later.locator("#bugdock-where").innerText(), "· the Acme deal conversation", "panel header");
    const line = await lastSaid(later, "#bugdock");
    assert(line.startsWith("I've checked the bug report you told me about earlier."), line);
    await later.waitForFunction(() => window.__RICHOS_MOCK_BUG__.unchecked().length === 0);
    await later.click("#bugdock #bug-send");
    await later.waitForSelector("#bugdock .bugcard.is-sent");
    await later.close();
    return line.split(".")[0];
  });

  await run.check("what Rich checked, under Worked for, is at least 16px in both themes", async () => {
    // Finding 3, fixture `readable-type.py`: on the tip it was 14px and exempt from the size check.
    const sizes = [];
    for (const theme of ["dark", "light"]) {
      const page = await open(theme);
      await bustABug(page);
      await answer(page, ANSWER);
      const px = await page.locator(".bug-digest").evaluate((e) => parseFloat(getComputedStyle(e).fontSize));
      assert(px >= 16, `${theme}: the digest is ${px}px`);
      sizes.push(`${theme} ${px}px`);
      await page.close();
    }
    return sizes.join(", ");
  });

  await run.check("while Rich changes the report, Send and Cancel wait for him, and what is sent is what he changed", async () => {
    // Finding 3: on the tip Send and Cancel stayed live, and a late change rewrote a sent card.
    const page = await open("dark", { bugChangeHold: true });
    await bustABug(page);
    await answer(page, "Not now fades a suggestion but leaves it.");
    await page.fill("#bugdock-input", "Also say it happens in the light theme too");
    await page.keyboard.press("Enter");
    await page.waitForSelector(".bug-working:has-text('Rich is changing the report…')");
    await page.waitForSelector(".bugcard .bug-pill:has-text('Changing it…')");
    for (const id of ["#bug-send", "#bug-cancel", "#bug-change"]) assert(await page.isHidden(id), id + " is offered while Rich is changing the report");
    await page.evaluate(() => window.__RICHOS_MOCK_BUG__.releaseChange());
    await page.waitForSelector(".bug-sec p.is-added");
    await page.waitForSelector(".bugcard .bug-pill:has-text('Not sent yet · changed')");
    const shown = await page.locator(".bugcard .bug-doc").innerText();
    await page.click("#bug-send");
    await page.waitForSelector(".bugcard.is-sent");
    const sent = (await calls(page, "bug_report_send"))[0].sheet;
    const words = [sent.title, ...sent.sections.flatMap((s) => [s.heading, ...s.paragraphs, ...s.steps])].join("\n");
    assertEqual(words.replace(/\s+/g, " ").trim(), shown.replace(/\s+/g, " ").trim(), "what was sent is not the changed report");
    assert(words.includes("It happens in the light theme too."), words);
    await page.close();
    return "held, then sent with the change";
  });

  await run.check("Cancel says nothing was sent only once the waiting copy is gone from this Mac", async () => {
    // Finding 2: on the tip Cancel said "Canceled. Nothing was sent." before, and whatever, the
    // shell answered; here the copy could not be removed and still goes out by itself.
    const page = await open("dark", { bugNet: "offline", bugCancel: "fail" });
    await bustABug(page);
    await answer(page, ANSWER);
    await page.click("#bug-send");
    await page.waitForSelector(".bugcard.is-queued");
    await page.click("#bug-cancel");
    await page.waitForSelector("#bugdock-msgs .bug-rich:has-text(\"I couldn't cancel the report\")");
    await page.waitForSelector(".bugcard.is-queued .bug-pill:has-text('Waiting to send · saved on this Mac')");
    assert(!(await prose(page)).includes("Nothing was sent"), "it said nothing was sent: " + (await prose(page)));
    assertEqual((await page.evaluate(() => window.__RICHOS_MOCK_BUG__.waiting())).length, 1, "the report is no longer waiting");
    for (const id of ["#bug-try-now", "#bug-change", "#bug-cancel"]) assert(await page.isVisible(id), id + " is not offered again");
    const said = await lastSaid(page);
    await page.close();
    return said;
  });

  await run.check("Cancel while a retry is already sending: the report went out, and Rich says so", async () => {
    // Finding 2: on the tip the card said "Canceled · nothing sent" beside a notice that it went out.
    const page = await open("dark", { bugNet: "offline", bugCancel: "hold" });
    await bustABug(page);
    await answer(page, ANSWER);
    await page.click("#bug-send");
    await page.waitForSelector(".bugcard.is-queued");
    await page.click("#bug-cancel");
    await page.waitForSelector(".bugcard .bug-pill:has-text('Canceling…')");
    await page.evaluate(() => window.__RICHOS_MOCK_BUG__.retryGoesOut());
    await page.evaluate(() => window.__RICHOS_MOCK_BUG__.releaseCancel());
    await page.waitForSelector(".bugcard.is-sent .bug-pill:has-text('Sent · #412')");
    await page.waitForSelector("#bugdock-msgs .bug-rich:has-text('It had already gone out')");
    const all = await prose(page);
    assert(!all.includes("Nothing was sent"), "it said nothing was sent: " + all);
    assertEqual(await page.locator(".bugcard.is-canceled").count(), 0, "the card says canceled");
    const said = await lastSaid(page);
    await page.close();
    return said.split("\n")[0];
  });

  await run.check("Change it on a waiting report that already went out says so instead of opening it", async () => {
    // Finding 2: Change it took the waiting copy out with the same unchecked cancellation.
    const page = await open("dark", { bugNet: "offline", bugCancel: "hold" });
    await bustABug(page);
    await answer(page, ANSWER);
    await page.click("#bug-send");
    await page.waitForSelector(".bugcard.is-queued");
    await page.click("#bug-change");
    await page.evaluate(() => window.__RICHOS_MOCK_BUG__.retryGoesOut());
    await page.evaluate(() => window.__RICHOS_MOCK_BUG__.releaseCancel());
    await page.waitForSelector(".bugcard.is-sent .bug-pill:has-text('Sent · #412')");
    await page.waitForSelector("#bugdock-msgs .bug-rich:has-text(\"so it can't be changed\")");
    assertEqual(await page.locator(".bugcard.is-editing").count(), 0, "a sent report was opened for changes");
    const said = await lastSaid(page);
    await page.close();
    return said.split("\n")[0];
  });

  await run.check("Rich is given what was on the screen the user was on, before the exchange covered it", async () => {
    // Finding 6: on the tip Rich had only the user's words, the screen's name, settings and version.
    const page = await open("dark");
    await bustABug(page);
    await answer(page, ANSWER);
    const all = await page.evaluate(() => window.__RICHOS_MOCK_BUG__.calls.map((c) => c.cmd));
    assert(all.indexOf("bug_report_look") !== -1 && all.indexOf("bug_report_look") < all.indexOf("bug_report_write"), "no picture was asked for before the write-up: " + all.join(", "));
    const screen = (await calls(page, "bug_report_write"))[0].screen;
    const content = String(screen.content || "");
    assert(content.includes("what's the status on Acme?"), "the conversation on screen is not in what Rich was given: " + content.slice(0, 300));
    assert(content.includes("Acme deal"), "the conversation list is not in what Rich was given");
    for (const not of ["What went wrong?", "Bust a bug!", "Never mind"]) assert(!content.includes(not), `"${not}" (the exchange or the menu, not the screen) was given as the screen`);
    await page.close();
    return content.length + " characters of the screen, e.g. " + JSON.stringify(content.slice(0, 80));
  });

  await run.check("over a window, the report is in the Rich panel beside it, and the window stays open", async () => {
    const page = await open("dark");
    await page.click("#nav-corrections");
    await page.waitForSelector("#corrections-overlay:not([hidden])");
    await bustABug(page);
    await page.waitForSelector("#bugdock:not([hidden])");
    assertEqual(await page.locator("#bugdock-where").innerText(), "· Corrections", "panel header");
    assert(await page.isVisible("#corrections-overlay"), "the window did not stay open");
    assertEqual(await page.locator("#conversation .bug-rich").count(), 0, "it also started in the conversation");
    const asked = await page.locator("#bugdock-msgs .bug-rich .tl-prose").first().innerText();
    assert(asked.includes("I've noted that you were in Corrections and it stays open beside us."), asked);
    // Side by side: the window moved left of the panel.
    const panel = await page.locator("#bugdock").boundingBox();
    const win = await page.locator("#corrections-overlay .overlay-panel").boundingBox();
    assert(win.x + win.width <= panel.x, `the window is under the panel: ${JSON.stringify({ win, panel })}`);
    await page.fill("#bugdock-input", "When I press Not now on a suggestion it fades but stays there.");
    await page.keyboard.press("Enter");
    await page.waitForSelector("#bugdock .bugcard .bug-pill:has-text('Not sent yet')");
    assert(await page.isHidden("#bugdock-x"), "the panel can be closed while the report waits on a decision");
    await shot(page, "dock-report-dark");
    await page.click("#bugdock #bug-send");
    await page.waitForSelector("#bugdock .bugcard.is-sent");
    assert(await page.isVisible("#bugdock-x"), "no way to close the panel after sending");
    await page.click("#bugdock-x");
    await page.waitForSelector("#bugdock", { state: "hidden" });
    assert(await page.isVisible("#corrections-overlay"), "closing the panel closed the window");
    await page.close();
    return asked;
  });

  // ---- wherever Bust a bug is pressed, the report is on top (echo-opus-bug20's finding) ----
  // On f619e9e5f the report opened UNDER the screen it was pressed on: the held opening screen
  // covers the window at z-index 200 and the home screen at 150, and the report was drawn in the
  // conversation or in `#bugdock` at 65 — and inside `#app`, which the home screen makes `inert`.
  // The user pressed Bust a bug and saw nothing. The question is asked of the compositor
  // (`elementFromPoint` at the middle of Rich's question and of the box the answer is typed in),
  // and the answer is TYPED, key by key, because the opening screen consumes the space key and
  // takes itself down on the first other one, and the home screen pulls focus back to its door.
  async function openCovered(where) {
    const page = await browser.newPage({ viewport: { width: 1440, height: 900 }, locale: "en-US" });
    page.setDefaultTimeout(30000);
    page.on("pageerror", (e) => errors.push(String(e)));
    await page.addInitScript(() => {
      localStorage.setItem("richos-theme", "dark");
      localStorage.setItem("richos-mock-config", JSON.stringify({ theme: "dark", font_scale: 100 }));
      window.__RICHOS_MOCK_PRESET__ = { bugWriteMs: 300 };
    });
    if (where === "held") await page.addInitScript(HOLD_CURTAIN);
    await page.goto("file://" + path.join(UI_DIR, "index.html"));
    if (where === "held") {
      await page.waitForSelector("#splash");
      await page.keyboard.press("Space");
      await page.waitForFunction(() => document.getElementById("splash").classList.contains("splash--paused"));
    } else {
      await page.waitForFunction(() => typeof window.RichHome === "object" && !!window.RichSplash);
      await page.evaluate(() => window.RichSplash.yieldNow("bug-report-suite"));
      await page.waitForFunction(() => !document.getElementById("splash"));
      await page.waitForFunction(() => window.RichHome.isOpen() && document.body.classList.contains("home-open"));
    }
    // `init()` focuses the composer at its very end: let it, so what is measured below is the
    // report's own focus and not a race with boot.
    await bootSettled(page);
    await shellSettled(page);
    return page;
  }
  /// Is the screen the report was opened on still the one behind it?
  const behind = (page, where) =>
    page.evaluate((w) => {
      if (w === "held") {
        const s = document.getElementById("splash");
        return !!s && s.classList.contains("splash--paused") && !(window.RichSplash.state || {}).reason;
      }
      return window.RichHome.isOpen();
    }, where);
  for (const [where, name, label] of [
    ["held", "the held opening screen", "· the opening screen"],
    ["home", "the home screen", "· the home screen"],
  ]) {
    await run.check(`Bust a bug on ${name}: the report is on top of it and can be answered at once`, async () => {
      const page = await openCovered(where);
      await bustABug(page);
      await page.waitForSelector("#bugdock-msgs .bug-rich", { state: "attached" });
      // The report's own entrance only: a HELD opening screen's animations are paused, and their
      // `finished` never comes, so the suite-wide `settle` would wait on it forever.
      await page.evaluate(() => Promise.all(document.getAnimations()
        .filter((a) => a.effect && a.effect.target && a.effect.target.closest && a.effect.target.closest("#bugdock"))
        .map((a) => a.finished.catch(() => null))));
      const top = await page.evaluate(() => {
        const q = document.querySelector("#bugdock-msgs .bug-rich .tl-prose");
        const input = document.getElementById("bugdock-input");
        // The curtain is `pointer-events: none` (splash.css), so hit-testing looks straight through
        // it: for the length of this read it is made hit-testable, so a curtain painted over the
        // report is what `elementFromPoint` answers rather than whatever is under both.
        const curtain = document.getElementById("splash");
        if (curtain) curtain.style.pointerEvents = "auto";
        const hitAt = (el) => {
          const r = el.getBoundingClientRect();
          const h = document.elementFromPoint(r.left + r.width / 2, r.top + r.height / 2);
          return h === el || el.contains(h) ? "ON-TOP" : "COVERED by " + (h ? h.tagName.toLowerCase() + (h.id ? "#" + h.id : h.className ? "." + String(h.className).split(" ")[0] : "") : "nothing");
        };
        const out = { asked: q.textContent.trim(), question: hitAt(q), input: hitAt(input), inputId: input.id, focused: document.activeElement === input };
        if (curtain) curtain.style.pointerEvents = "";
        return out;
      });
      assert(top.asked.startsWith("What went wrong?"), "the report did not start: " + JSON.stringify(top.asked));
      assertEqual(top.question, "ON-TOP", `on ${name}, Rich's question is not what is painted where it is`);
      assertEqual(top.input, "ON-TOP", `on ${name}, the box the answer goes in is not what is painted where it is`);
      assert(top.focused, `on ${name}, the box the answer goes in did not get the keyboard`);
      assert(await behind(page, where), `pressing Bust a bug took ${name} away: the bug is on that screen`);
      assertEqual(await page.locator("#bugdock-where").innerText(), label, "panel header");
      // Answer it the way a person does: a click into the box, then the keys, spaces included.
      const said = "The names on the left get cut off when I make the text bigger.";
      await page.click("#" + top.inputId);
      await page.keyboard.type(said);
      assertEqual(await page.inputValue("#" + top.inputId), said, `on ${name}, the keys did not all reach the answer`);
      assert(await behind(page, where), `typing the answer took ${name} away`);
      await page.keyboard.press("Enter");
      await page.waitForSelector("#bugdock .bug-user:has-text('" + said + "')");
      await page.waitForSelector("#bugdock .bugcard .bug-pill:has-text('Not sent yet')");
      const card = await page.evaluate(() => {
        const c = document.querySelector("#bugdock .bugcard");
        const r = c.getBoundingClientRect();
        const curtain = document.getElementById("splash");
        if (curtain) curtain.style.pointerEvents = "auto";
        const h = document.elementFromPoint(r.left + r.width / 2, r.top + Math.min(40, r.height / 2));
        if (curtain) curtain.style.pointerEvents = "";
        return h === c || c.contains(h);
      });
      assert(card, `on ${name}, the report card Rich wrote is under the screen`);
      assert(await behind(page, where), `answering took ${name} away`);
      await page.close();
      return `question ${top.question}, answer box ${top.input} and focused, typed and sent; card on top; ${name} still behind it`;
    });
  }

  // ---- ONE PLACE: every press, over everything, in every input mode (CEO, 2026-10-10) ----
  // His sentence is the spec: "pressing Bust a bug always shows the bug report on top, ready to
  // answer." The report always lives in the Rich panel, so there is one case, and this check walks
  // it over every place it can be pressed: every element a covering rule in the shipped CSS matches
  // (`lib/overlays.js`, one made from the selector where none is in the page yet, put on screen with
  // focus inside it, as `appearance.js` check 7c does), Dictation opened from Settings, and the bare
  // conversation. Over each, a press (always a first one: while a report is open Bust a bug is
  // disabled, checked below), with plain typing, with Talk to Rich on, and with Dictation waiting
  // for a different key. In every cell: the report is in the panel and nothing of it anywhere else;
  // what it shows and its answer box are what is painted at their places (the overlay made
  // hit-testable for the read, so a cover that lets the pointer through still counts); the keys are
  // in the answer box without a click; Talk to Rich and Dictation's wait are off; keys typed and
  // Enter reach the report; the panel's talk button takes what is said; Change it takes typing in
  // the card's title; and no message reaches the conversation. The setup clicks (Talk to Rich,
  // Press a different key) are made with the element's own click, as they are the starting state
  // and not what is under test; every press of Bust a bug and everything after it is a person's.
  // Left out: the two first-run questions main.js's own QUESTION_SELECTOR finds; while one is up
  // Settings cannot be pressed (`setup.js` check 20), so no report starts over it.
  const FIRST_RUN_QUESTIONS = ["memory-setup", "setup-sheet"];
  const MODES = ["typing", "Talk to Rich on", "Dictation waiting for a key"];
  /// Put target `t` on screen with focus inside it: `{name, sel}`, `{question}` or `{error}`.
  async function showTarget(page, t, qsel) {
    if (t.conversation) return { name: "the conversation", sel: null };
    if (t.dictation) {
      if (await page.locator("#dictation-sheet:not([hidden])").count() === 0) {
        await page.click("#set-btn");
        await page.click("#set-dictation-open");
        await page.waitForSelector("#dictation-sheet:not([hidden])");
      }
      return { name: "div#dictation-sheet (opened from Settings)", sel: "#dictation-sheet" };
    }
    return page.evaluate(({ sel, index, qsel, firstRun }) => {
      let el = index >= 0 ? document.querySelectorAll(sel)[index] : null;
      if (el && el.matches(qsel) && firstRun.includes(el.id)) return { question: el.id };
      let made = false;
      if (!el) {
        const m = sel.match(/^([a-z]+)?((?:[#.][\w-]+)+)$/i);
        if (!m) return { error: "no element matches it, and it is not a simple selector to make one from" };
        el = document.createElement(m[1] || "div");
        for (const part of m[2].match(/[#.][\w-]+/g)) {
          if (part[0] === "#") el.id = part.slice(1);
          else el.classList.add(part.slice(1));
        }
        document.body.appendChild(el);
        made = true;
      }
      for (let n = el; n && n.nodeType === 1; n = n.parentElement) if (n.hidden) n.hidden = false;
      if (!el.getClientRects().length || getComputedStyle(el).display === "none") el.style.display = "block";
      const first = el.querySelector("button:not([disabled]), input:not([disabled]), select, textarea, [tabindex]:not([tabindex='-1'])");
      if (first) first.focus();
      // Hit-testable from the start: a cover that lets the pointer through (the opening curtain
      // is one, `splash.css`) is still a cover.
      el.style.pointerEvents = "auto";
      el.setAttribute("data-under-test", "");
      const name = el.tagName.toLowerCase() + (el.id ? "#" + el.id : "." + String(el.className).split(" ")[0]);
      return { name: made ? name + " (made from " + sel + ")" : name, sel: "[data-under-test]" };
    }, { sel: t.sel, index: t.index, qsel, firstRun: FIRST_RUN_QUESTIONS });
  }
  /// One cell: target `t`, input `mode`. Returns a line; throws with what was wrong.
  async function oneCell(t, mode, qsel) {
    const page = await open("dark", { dictation: true, bugVoice: true });
    try {
      const before = await page.locator("#messages .tl-user").count();
      if (mode === "Talk to Rich on") {
        await page.evaluate(() => document.getElementById("talk-toggle").click());
        await page.waitForSelector("#voice-panel:not([hidden])");
      }
      if (mode === "Dictation waiting for a key") {
        await page.click("#set-btn");
        await page.click("#set-dictation-open");
        await page.waitForSelector("#dictation-sheet:not([hidden])");
        await page.evaluate(() => document.getElementById("dict-capture").click());
        await page.waitForSelector("#dict-capture-cancel");
      }
      const shown = await showTarget(page, t, qsel);
      if (shown.question) return { skipped: "#" + shown.question };
      if (shown.error) throw new Error(shown.error);
      await page.evaluate(() => new Promise((r) => requestAnimationFrame(() => requestAnimationFrame(r))));
      await bustABug(page);
      await page.waitForSelector("#bugdock:not([hidden]) #bugdock-msgs .bug-rich", { state: "attached" });
      await page.evaluate(() => Promise.all(document.getAnimations()
        .filter((a) => a.effect && a.effect.target && a.effect.target.closest && a.effect.target.closest("#bugdock"))
        .map((a) => a.finished.catch(() => null))));
      // `focusAnswer` puts the keys there in a zero-delay timer queued as the panel was shown; a
      // timer queued after it has run after it, so this reads the fact, not the clock.
      const ready = await page.evaluate(() => new Promise((r) => setTimeout(() => r(document.activeElement === document.getElementById("bugdock-input")), 0)));
      const st = await page.evaluate(({ sel }) => {
        const cover = sel ? document.querySelector(sel) : null;
        const was = cover ? cover.style.pointerEvents : "";
        if (cover) cover.style.pointerEvents = "auto";
        const box = document.getElementById("bugdock-msgs").getBoundingClientRect();
        const hitAt = (el) => {
          if (!el) return "MISSING";
          const r = el.getBoundingClientRect();
          const h = document.elementFromPoint(r.left + r.width / 2, r.top + r.height / 2);
          return h === el || el.contains(h) ? "ON-TOP" : "COVERED by " + (h ? h.tagName.toLowerCase() + (h.id ? "#" + h.id : "." + String(h.className).split(" ")[0]) : "nothing");
        };
        const shownEl = document.querySelector("#bugdock-msgs .bug-rich .tl-prose");
        const out = {
          shown: hitAt(shownEl),
          input: hitAt(document.getElementById("bugdock-input")),
          outside: [...document.querySelectorAll(".bug-rich, .bug-user, .bugcard")].filter((n) => !n.closest("#bugdock")).length,
          reports: [...document.querySelectorAll("#bugdock-msgs .bug-rich .tl-prose")].filter((p) => p.textContent.startsWith("What went wrong?")).length,
          capturing: !!document.getElementById("dict-capture-cancel"),
          voice: document.getElementById("talk-toggle").getAttribute("aria-pressed"),
        };
        if (cover) cover.style.pointerEvents = was;
        return out;
      }, { sel: shown.sel });
      const bad = [];
      if (st.shown !== "ON-TOP") bad.push("Rich's question " + st.shown);
      if (st.input !== "ON-TOP") bad.push("the answer box " + st.input);
      if (!ready) bad.push("the keys are not in the answer box");
      if (st.outside) bad.push(st.outside + " part(s) of the report outside the panel");
      if (st.reports !== 1) bad.push(st.reports + " reports asked for");
      if (st.capturing) bad.push("Dictation still waits for a key");
      if (st.voice !== "false") bad.push("Talk to Rich still on");
      if (bad.length) throw new Error(bad.join("; "));
      // Typing, then Enter, without a click first: ready to answer.
      const typed = "The names on the left get cut off when I make the text bigger.";
      await page.keyboard.type(typed);
      const value = await page.inputValue("#bugdock-input");
      if (value !== typed) throw new Error("the keys typed reached the answer box as " + JSON.stringify(value));
      await page.keyboard.press("Enter");
      const added = () => page.locator("#bugdock .bug-sec p.is-added").count();
      await page.waitForSelector("#bugdock .bugcard .bug-pill:has-text('Not sent yet')");
      // The panel's talk button takes what is said.
      await page.waitForSelector("#bugdock #bug-change", { state: "visible" });
      const had = await added();
      await page.click("#bugdock-talk");
      await page.waitForSelector("#bugdock-voice:not([hidden])");
      await page.evaluate((t) => window.__RICHOS_MOCK_BUG__.say(t), "Also say it shows in Search too");
      await page.waitForFunction((n) => document.querySelectorAll("#bugdock .bug-sec p.is-added").length === n, had + 1);
      await page.click("#bugdock-talk");
      await page.waitForSelector("#bugdock-form:not([hidden])");
      // Change it takes typing in the card.
      await page.waitForSelector("#bugdock #bug-change", { state: "visible" });
      await page.click("#bugdock #bug-change");
      await page.waitForSelector("#bugdock .bugcard.is-editing");
      await page.waitForFunction(() => document.activeElement && document.activeElement.classList.contains("bug-title"));
      await page.keyboard.type(" (seen twice)");
      const title = await page.locator("#bugdock .bugcard .bug-title").innerText();
      if (!title.endsWith("(seen twice)")) throw new Error("Change it did not take typing: the title is " + JSON.stringify(title));
      await page.click("#bugdock #bug-done");
      await page.waitForSelector("#bugdock .bugcard .bug-pill:has-text('Not sent yet · changed')");
      const after = await page.locator("#messages .tl-user").count();
      const outside = await page.evaluate(() => [...document.querySelectorAll(".bug-rich, .bug-user, .bugcard")].filter((n) => !n.closest("#bugdock")).length);
      if (after !== before || outside) throw new Error("the conversation got " + (after - before) + " message(s) and " + outside + " part(s) of the report");
      return { line: "on top, ready, typed, Enter, talk, Change it" };
    } finally {
      await page.close();
    }
  }
  await run.check("ONE PLACE: over every overlay, Dictation and the bare conversation, typing, Talk to Rich on and Dictation waiting for a key: the report is in the panel, on top, ready to answer, takes typing, Enter, its talk button and Change it, and nothing goes to the conversation", async () => {
    const qsel = (fs.readFileSync(path.join(UI_DIR, "main.js"), "utf8").match(/^const QUESTION_SELECTOR = '([^']+)';$/m) || [])[1];
    assert(qsel, "main.js has no `const QUESTION_SELECTOR = '...';` to read the first-run questions from");
    const rules = coverings.coveringRules();
    const scout = await open("dark", { dictation: true });
    const counts = await scout.evaluate((sels) => sels.map((s) => document.querySelectorAll(s).length), rules.map((r) => r.selector));
    await scout.close();
    const targets = [{ conversation: true }, { dictation: true }];
    rules.forEach((r, k) => {
      if (counts[k] === 0) targets.push({ sel: r.selector, index: -1 });
      for (let i = 0; i < counts[k]; i++) targets.push({ sel: r.selector, index: i });
    });
    const cells = [];
    for (const t of targets) for (const mode of MODES) cells.push({ t, mode });
    const wrong = [];
    const skipped = new Set();
    let passed = 0;
    // Four pages at a time: each cell is its own page, so none leans on another's state.
    let next = 0;
    async function worker() {
      while (next < cells.length) {
        const c = cells[next++];
        const label = (c.t.conversation ? "the conversation" : c.t.dictation ? "Dictation" : c.t.sel + (c.t.index >= 0 ? "[" + c.t.index + "]" : " (made)")) + ", " + c.mode;
        try {
          const r = await oneCell(c.t, c.mode, qsel);
          if (r.skipped) skipped.add(r.skipped);
          else passed++;
        } catch (e) {
          wrong.push(label + ": " + String(e && e.message ? e.message : e).split("\n")[0]);
        }
      }
    }
    await Promise.all([worker(), worker(), worker(), worker()]);
    assertEqual(wrong, [], "the report is not on top and ready to answer in every cell");
    assert(passed > 2 * MODES.length, "EMPTY INVENTORY: only " + passed + " cells walked");
    return passed + " cells passed (" + (targets.length - skipped.size) + " places x " + MODES.length + " input modes); left out, the first-run questions (Settings cannot be pressed over them): " + [...skipped].join(", ");
  });

  // ---- the instructions question (CEO 2026-10-22; review rv-20261022T132946Z-3284b71e-753a, finding 1) ----
  await run.check("with the instructions question up, Bust a bug opens on top of it and takes typing", async () => {
    // The real question (ui/instructions-update.js), not one made from its selector: at z-index
    // 299 its scrim lay over the report panel (298) and the panel could not be clicked.
    const page = await open("dark");
    try {
      await page.evaluate(() => window.RichInstructionsUpdate.show([{ name: "rich.md", path: "~/myrichos/rich.md" }]));
      await page.waitForSelector("#instructions-update:not([hidden])");
      await bustABug(page);
      await page.waitForSelector("#bugdock-msgs .bug-rich .tl-prose");
      await page.evaluate(() => Promise.all(document.getAnimations()
        .filter((a) => a.effect && a.effect.target && a.effect.target.closest && a.effect.target.closest("#bugdock"))
        .map((a) => a.finished.catch(() => null))));
      const top = await page.evaluate(() => {
        const input = document.getElementById("bugdock-input");
        const r = input.getBoundingClientRect();
        const h = document.elementFromPoint(r.left + r.width / 2, r.top + r.height / 2);
        return { input: h === input || input.contains(h) ? "ON-TOP" : "COVERED by " + (h ? h.tagName.toLowerCase() + (h.id ? "#" + h.id : "." + String(h.className).split(" ")[0]) : "nothing"),
          questionUp: !document.getElementById("instructions-update").hidden };
      });
      assertEqual(top.input, "ON-TOP", "the report's answer box is under the instructions question");
      await page.click("#bugdock-input");
      await page.keyboard.type("The question covers the screen.");
      assertEqual(await page.inputValue("#bugdock-input"), "The question covers the screen.", "the report did not take typing");
      return "the report is on top of the question" + (top.questionUp ? ", which stays up behind it" : "") + ", and takes typing";
    } finally {
      await page.close();
    }
  });

  // ---- the reviews of 4cc6e34db (rv-20261010T150420Z-4cc6e34d-84f1) and cac278721 (rv-20261010T153832Z-cac27872-e986, rv-20261010T153823Z-cac27872-1b7a) ----
  await run.check("with Dictation waiting for a different key, Bust a bug ends that wait: the panel's report takes typing and Enter, and a function key from the tool's tap leaves Dictation's key alone", async () => {
    // Settings, Dictation, Press a different key, Settings, Bust a bug. A report in the panel takes
    // the keyboard alone: it ends Dictation's wait through Dictation's own Cancel, which also
    // disarms the tool's key tap (rv-20261010T153832Z-cac27872-e986). On cac278721 the wait and
    // the tap stayed on under the report.
    const page = await open("dark", { dictation: true });
    await page.click("#set-btn");
    await page.click("#set-dictation-open");
    await page.waitForSelector("#dictation-sheet:not([hidden])");
    await page.click("#dict-capture");
    await page.waitForSelector("#dict-capture-cancel");
    await bustABug(page);
    await page.waitForSelector("#bugdock-msgs .bug-rich", { state: "attached" });
    assertEqual(await page.locator("#dict-capture-cancel").count(), 0, "Dictation still waits for a different key under the report");
    const tap = (await page.evaluate(() => window.__RICHOS_MOCK__.dictationCalls())).filter((c) => c.cmd === "dictation_capture_key").map((c) => c.on);
    assertEqual(JSON.stringify(tap), "[true,false]", "the tool's key tap, armed then disarmed");
    const said = "The names on the left get cut off when I make the text bigger.";
    await page.click("#bugdock-input");
    await page.keyboard.type(said);
    assertEqual(await page.inputValue("#bugdock-input"), said, "what was typed in the panel's answer box");
    await page.keyboard.press("Enter");
    await page.waitForSelector("#bugdock .bugcard .bug-pill:has-text('Not sent yet')");
    // F7 delivered by the tool's tap (`rich://dictation {key}`), the report in focus. The mock's
    // event and Dictation's handler run synchronously, so a key change is recorded by the time
    // the call returns.
    await page.focus("#bugdock .bugcard #bug-send");
    await page.evaluate(() => window.__RICHOS_MOCK__.dictationKey(7));
    const set = (await page.evaluate(() => window.__RICHOS_MOCK__.dictationCalls())).filter((c) => c.cmd === "dictation_set_key");
    assertEqual(JSON.stringify(set), "[]", "Dictation's key was changed by a function key from the tool's tap");
    assert(await page.evaluate(() => document.activeElement && document.activeElement.id === "bug-send"), "focus left the report");
    await page.close();
    return "the wait ended (tap " + JSON.stringify(tap) + "); typed and sent in the panel; F7 from the tap set no key";
  });

  // ---- the review of 060668495 (rv-20261010T161328Z-06066849-659f) ----
  await run.check("a report already in the panel, then Press a different key in Dictation, then back to the report: the wait ends, and the report takes typing and Enter", async () => {
    // Settings, Dictation, Bust a bug, Press a different key in the sheet that stays open, then
    // the report's answer box. On 060668495 the wait was ended only when the panel was shown, so
    // the second wait and its key tap kept every key: the typing and Enter never reached the report.
    const page = await open("dark", { dictation: true });
    await page.click("#set-btn");
    await page.click("#set-dictation-open");
    await page.waitForSelector("#dictation-sheet:not([hidden])");
    await bustABug(page);
    await page.waitForSelector("#bugdock-msgs .bug-rich", { state: "attached" });
    await page.click("#dict-capture");
    await page.waitForSelector("#dict-capture-cancel");
    await page.click("#bugdock-input");
    assertEqual(await page.locator("#dict-capture-cancel").count(), 0, "Dictation still waits for a different key with the report's answer box in focus");
    const tap = (await page.evaluate(() => window.__RICHOS_MOCK__.dictationCalls())).filter((c) => c.cmd === "dictation_capture_key").map((c) => c.on);
    assertEqual(JSON.stringify(tap), "[true,false]", "the tool's key tap, armed then disarmed");
    const said = "The names on the left get cut off when I make the text bigger.";
    await page.keyboard.type(said);
    assertEqual(await page.inputValue("#bugdock-input"), said, "what was typed in the panel's answer box");
    await page.keyboard.press("Enter");
    await page.waitForSelector("#bugdock .bugcard .bug-pill:has-text('Not sent yet')");
    // The talk button is the other way back to the report: it ends a wait started again too.
    await page.click("#dict-capture");
    await page.waitForSelector("#dict-capture-cancel");
    await page.click("#bugdock-talk");
    assertEqual(await page.locator("#dict-capture-cancel").count(), 0, "Dictation still waits for a different key after the report's talk button");
    const tap2 = (await page.evaluate(() => window.__RICHOS_MOCK__.dictationCalls())).filter((c) => c.cmd === "dictation_capture_key").map((c) => c.on);
    assertEqual(JSON.stringify(tap2), "[true,false,true,false]", "the tool's key tap, armed and disarmed twice");
    await page.close();
    return "the wait ended on the click into the answer box (tap " + JSON.stringify(tap) + "); typed and sent in the panel; ended again by the talk button (tap " + JSON.stringify(tap2) + ")";
  });

  await run.check("Talk to Rich on, Dictation open, Bust a bug: voice mode goes off, nothing said goes to the conversation, and the panel's talk button takes the answer", async () => {
    // rv-20261010T153823Z-cac27872-1b7a: on cac278721 the report opened in the panel with
    // conversation voice mode still on, so the spoken answer became a message to Rich.
    const page = await open("dark", { bugVoice: true, dictation: true });
    await page.click("#talk-toggle");
    await page.waitForSelector("#voice-panel:not([hidden])");
    await page.click("#set-btn");
    await page.click("#set-dictation-open");
    await page.waitForSelector("#dictation-sheet:not([hidden])");
    await bustABug(page);
    await page.waitForSelector("#bugdock-msgs .bug-rich", { state: "attached" });
    assertEqual(await page.getAttribute("#talk-toggle", "aria-pressed"), "false", "conversation voice mode under the report (Talk to Rich pressed)");
    const stops = (await calls(page, "stop_voice_capture")).length;
    assertEqual(stops, 1, "conversation voice mode's own off path (stop_voice_capture calls)");
    // Said now, before the panel's talk button: neither the conversation nor the report takes it.
    // The transcript listeners run synchronously; two frames let a drawn message land.
    const before = await page.locator("#messages .tl-user").count();
    await page.evaluate((t) => window.__RICHOS_MOCK_BUG__.say(t), "Rich, what's on my calendar today?");
    await page.evaluate(() => new Promise((r) => requestAnimationFrame(() => requestAnimationFrame(r))));
    assertEqual(await page.locator("#messages .tl-user").count(), before, "what was said went to Rich's conversation");
    assertEqual(await page.locator("#bugdock .bug-user").count(), 0, "what was said with the panel's talk button off went to the report");
    // The panel's talk button is how the user speaks to the report.
    await page.click("#bugdock-talk");
    await page.waitForSelector("#bugdock-voice:not([hidden])");
    await page.evaluate((t) => window.__RICHOS_MOCK_BUG__.say(t), "The names on the left get cut off when the text is bigger.");
    await page.waitForSelector("#bugdock .bugcard .bug-pill:has-text('Not sent yet')");
    assertEqual(await page.locator("#messages .tl-user").count(), before, "the spoken answer went to Rich's conversation");
    await page.close();
    return "voice mode off (Talk to Rich not pressed, 1 stop_voice_capture); nothing said went to the conversation; the panel's talk button took the answer";
  });

  // ---- the reviews of f29db7082 (rv-20261010T180327Z-f29db708-24e0, rv-20261010T180313Z-f29db708-733e) ----
  /// The order of the voice calls made after call number `from`: `stop`, `start`, `hold on`, `hold off`.
  const voiceOrder = (page, from) => page.evaluate((n) => window.__RICHOS_MOCK_BUG__.calls.slice(n)
    .filter((c) => c.cmd === "stop_voice_capture" || c.cmd === "start_voice_capture" || c.cmd === "bug_report_voice")
    .map((c) => (c.cmd === "bug_report_voice" ? "hold " + (c.on ? "on" : "off") : c.cmd === "stop_voice_capture" ? "stop" : "start")), from);
  const callCount = (page) => page.evaluate(() => window.__RICHOS_MOCK_BUG__.calls.length);
  const heldNow = (page) => page.evaluate(() => {
    const v = window.__RICHOS_MOCK_BUG__.calls.filter((c) => c.cmd === "bug_report_voice");
    return v.length ? v[v.length - 1].on : null;
  });

  await run.check("the panel's microphone is on only while the report waits for the user's words: while Rich checks, it is stopped first, then the hold is released, and nothing said goes anywhere", async () => {
    // Both reviews, fixtures `panel-voice-state.js` and `report-state.js`: on f29db7082 moving to
    // checking released the shell's hold with the microphone still on, so the next sentence said
    // became a turn in Rich's conversation (the shell submits it whenever nothing holds it back).
    // The mock does not run the shell's submission, so this reads the ORDER of the calls: the
    // microphone is stopped (`stop_voice_capture`) before the hold is released.
    const page = await open("dark", { bugVoice: true, bugWriteMs: 1500 });
    await bustABug(page);
    await page.waitForSelector("#bugdock:not([hidden]) #bugdock-msgs .bug-rich");
    await page.click("#bugdock-talk");
    await page.waitForSelector("#bugdock-voice:not([hidden])");
    assertEqual(await heldNow(page), true, "the hold while the report waits for the spoken answer");
    const from = await callCount(page);
    const before = await page.locator("#messages .tl-user").count();
    await page.evaluate((t) => window.__RICHOS_MOCK_BUG__.say(t), "The names on the left get cut off when the text is bigger.");
    await page.waitForSelector("#bugdock .bug-working");
    await page.waitForFunction(() => {
      const v = window.__RICHOS_MOCK_BUG__.calls.filter((c) => c.cmd === "bug_report_voice");
      return v[v.length - 1].on === false;
    });
    const checking = await page.evaluate(() => ({
      step: window.RichBug.state().slice(-1)[0].step,
      talk: document.getElementById("bugdock-talk").getAttribute("aria-pressed"),
      form: !document.getElementById("bugdock-form").hidden,
    }));
    assertEqual(checking.step, "checking", "the step while Rich checks");
    assertEqual(checking.talk, "false", "the panel's talk button still on while Rich checks");
    const order = await voiceOrder(page, from);
    assertEqual(order, ["stop", "hold off"], "the voice calls on the way to checking: the microphone stopped, then the hold released");
    // Said while Rich checks: the report does not take it, and nothing reaches the conversation.
    await page.evaluate((t) => window.__RICHOS_MOCK_BUG__.say(t), "And it happens in the light theme too.");
    // The talk button cannot turn the microphone on again while nothing waits for words.
    await page.click("#bugdock-talk");
    assertEqual(await page.getAttribute("#bugdock-talk", "aria-pressed"), "false", "the talk button turned the microphone on while Rich checks");
    await page.waitForSelector("#bugdock .bugcard .bug-pill:has-text('Not sent yet')");
    assertEqual(await page.locator("#bugdock .bug-user").count(), 1, "what was said while Rich checked went to the report");
    assertEqual(await page.locator("#messages .tl-user").count(), before, "what was said went to Rich's conversation");
    const all = await voiceOrder(page, from);
    assertEqual(all, ["stop", "hold off"], "voice calls from the spoken answer to the card (no start while Rich checked)");
    await page.close();
    return "talk off while checking; voice calls: " + all.join(" → ") + "; the second sentence went nowhere";
  });

  // ---- the review of fe4cd6b14 (rv-20261010T183313Z-fe4cd6b1-1e66), finding 2 ----
  await run.check("a microphone start that waited for a stop runs only if the panel's talk is still on: on, off, on, off while the stops run opens nothing", async () => {
    // Fixture `pending-mic-restart.js`: on fe4cd6b14 talk turned on while a stop was still running
    // queued an unconditional start; turned off again, the start still ran once the stops ended,
    // and the microphone was open with the hold released. `bugStopHold` keeps each stop running
    // until `releaseStop()`.
    const page = await open("dark", { bugVoice: true, bugStopHold: true });
    await bustABug(page);
    await page.waitForSelector("#bugdock:not([hidden]) #bugdock-msgs .bug-rich");
    await page.click("#bugdock-talk");
    await page.waitForSelector("#bugdock-voice:not([hidden])");
    const from = await callCount(page);
    await page.click("#bugdock-talk"); // off: its stop is still running
    await page.click("#bugdock-talk"); // on again: the start waits for that stop
    await page.click("#bugdock-talk"); // off again before the stop ends
    assertEqual(await page.getAttribute("#bugdock-talk", "aria-pressed"), "false", "the talk button after on, off, on, off");
    await page.evaluate(() => window.__RICHOS_MOCK_BUG__.releaseStop());
    await page.waitForFunction(() => {
      const v = window.__RICHOS_MOCK_BUG__.calls.filter((c) => c.cmd === "bug_report_voice");
      return v[v.length - 1].on === false;
    });
    // Anything queued behind the stops has run by the time two more frames have gone.
    await page.evaluate(() => new Promise((r) => requestAnimationFrame(() => requestAnimationFrame(r))));
    const order = await voiceOrder(page, from);
    assertEqual(order, ["stop", "stop", "hold off"], "the voice calls: both stops, no start, then the hold released");
    await page.close();
    return "voice calls: " + order.join(" → ");
  });

  // ---- the CEO, 2026-10-10, and rv-20261010T183337Z-fe4cd6b1-ce45 finding 1 ----
  // "WHY THE FUCK IS IT EVEN POSSIBLE TO CLICK THE FUCKING BUTTON AGAIN IF THE ACTION RELATED TO
  // THAT IS ALREADY GOING ON???" On fe4cd6b14 Bust a bug could be pressed again over a report in
  // progress (the review's case: while Rich checked, the press put the keys in an answer box whose
  // Enter did nothing). Now it is disabled from the press until the report is sent, canceled or its
  // panel closed.
  /// Settings open, then every way to press Bust a bug: the pointer, the element's own click, and
  /// focus for the keyboard. Returns what the menu showed and whether anything started.
  async function pressBugAgain(page) {
    await page.click("#set-btn");
    await page.waitForSelector("#set-menu:not([hidden]) #bug-btn");
    const count = () => page.evaluate(() => ({
      flows: window.RichBug.state().length,
      looks: window.__RICHOS_MOCK_BUG__.calls.filter((c) => c.cmd === "bug_report_look").length,
      asked: document.querySelectorAll("#bugdock-msgs .bug-rich .tl-prose").length,
    }));
    const before = await count();
    const shown = await page.evaluate(() => {
      const b = document.getElementById("bug-btn");
      const cs = getComputedStyle(b);
      return { disabled: b.disabled, shown: !!b.getClientRects().length && cs.visibility !== "hidden" && cs.display !== "none" };
    });
    await page.click("#bug-btn", { force: true });
    await page.evaluate(() => document.getElementById("bug-btn").click());
    const focused = await page.evaluate(() => { const b = document.getElementById("bug-btn"); b.focus(); return document.activeElement === b; });
    await page.evaluate(() => new Promise((r) => requestAnimationFrame(() => requestAnimationFrame(r))));
    const after = await count();
    if (await page.isVisible("#set-menu")) await page.click("#set-btn");
    await page.waitForSelector("#set-menu", { state: "hidden" });
    return { disabled: shown.disabled, shown: shown.shown, focusable: focused, started: JSON.stringify(after) !== JSON.stringify(before) };
  }
  await run.check("while a report is open, Bust a bug in Settings is shown disabled and cannot be pressed (asked, Rich checking, the card, Change it, Rich changing it, sending); after Send, Cancel or closing the panel it works again", async () => {
    const page = await open("dark", { bugWriteMs: 4000, bugChangeHold: true, bugSendCheckHold: true });
    const open_ = { disabled: true, shown: true, focusable: false, started: false };
    const seen = [];
    async function expectOpen(step) {
      const st = await pressBugAgain(page);
      assertEqual(st, open_, step + ": Bust a bug in the Settings menu");
      seen.push(step);
    }
    await bustABug(page);
    await page.waitForSelector("#bugdock:not([hidden]) #bugdock-msgs .bug-rich");
    await expectOpen("asked");
    await page.fill("#bugdock-input", "The names on the left get cut off when I make the text bigger.");
    await page.keyboard.press("Enter");
    await page.waitForSelector("#bugdock .bug-working");
    assertEqual(await page.evaluate(() => window.RichBug.state().slice(-1)[0].step), "checking", "the step");
    await expectOpen("Rich checking");
    await page.waitForSelector(".bugcard .bug-pill:has-text('Not sent yet')");
    await expectOpen("the card");
    await page.click("#bug-change");
    await page.waitForSelector(".bugcard.is-editing");
    await expectOpen("Change it");
    await page.click("#bug-done");
    await page.waitForSelector(".bugcard .bug-pill:has-text('Not sent yet')");
    await page.fill("#bugdock-input", "Also say it happens in the light theme too");
    await page.keyboard.press("Enter");
    await page.waitForSelector(".bugcard .bug-pill:has-text('Changing it…')");
    await expectOpen("Rich changing it");
    await page.evaluate(() => window.__RICHOS_MOCK_BUG__.releaseChange());
    await page.waitForSelector(".bug-sec p.is-added");
    await page.click("#bug-send");
    await page.waitForSelector(".bugcard.is-sending");
    await expectOpen("sending");
    await page.evaluate(() => window.__RICHOS_MOCK_BUG__.releaseSendCheck());
    await page.waitForSelector(".bugcard.is-sent");
    // Sent: it works again, and a press is a new report.
    const reopen = async (after) => {
      await page.click("#set-btn");
      await page.waitForSelector("#set-menu:not([hidden]) #bug-btn");
      assertEqual(await page.evaluate(() => document.getElementById("bug-btn").disabled), false, after + ": Bust a bug is still disabled");
      const flows = await page.evaluate(() => window.RichBug.state().length);
      await page.click("#bug-btn");
      await page.waitForFunction((n) => window.RichBug.state().length === n + 1 && window.RichBug.state()[n].step === "ask", flows);
      await page.waitForSelector("#bugdock:not([hidden]) #bugdock-msgs .bug-rich");
      seen.push("after " + after + ": pressed, a new report");
    };
    await reopen("Send");
    await expectOpen("a new report, asked");
    // Canceled: it works again.
    await page.fill("#bugdock-input", "The window froze while opening the plan.");
    await page.keyboard.press("Enter");
    await page.waitForSelector(".bugcard .bug-pill:has-text('Not sent yet')", { timeout: 30000 });
    await page.click("#bug-cancel");
    await page.waitForSelector(".bugcard.is-canceled");
    await reopen("Cancel");
    // The panel closed: it works again.
    await page.click("#bugdock-x");
    await page.waitForSelector("#bugdock", { state: "hidden" });
    await reopen("closing the panel");
    await page.close();
    return "disabled while " + seen.join("; ");
  });

  // ---- nightly 49's walk, D28 (docs/verification/2026-10-10-nightly-49-candidate-walk.md) ----
  // Run I on 13aa9bc0f: the panel asked "What went wrong?", "markerone" was typed into its box,
  // and while the panel was open the conversation behind it finished its first-run "Not now".
  // `declineFirstRunInterview` then called `inputEl.focus()`, and " markertwo" went into the
  // conversation's message box, where Return sends it to Rich. THE RULE: while a report is open in
  // the Rich panel, nothing moves focus to the conversation's message box. It is one method on the
  // box (`main.js`, `inputEl.focus`), so every caller goes through it: the first-run path and the
  // way the home screen and the question sheets reach it (`document.getElementById("input")
  // .focus()`). Once the panel closes the box takes focus again.
  await run.check("D28: while a report is open in the Rich panel, nothing moves focus to the conversation's message box: the first-run Not now finishing and a caller outside main.js, asked and at the card; typing and Enter stay the report's, and once the panel closes the box takes focus again", async () => {
    const page = await open("light", { onboarding: "not-yet", onboardingDeclineHold: true, bugWriteMs: 300 });
    const before = await page.locator("#messages .tl-user").count();
    const where = () => page.evaluate(() => { const a = document.activeElement; return a ? (a.id ? "#" + a.id : a.tagName.toLowerCase()) : "none"; });
    // The path the walk saw: "Not now" pressed in the conversation, its write still on the way.
    await page.waitForSelector("#first-run-later:visible");
    await page.click("#first-run-later");
    await bustABug(page);
    await page.waitForFunction(() => document.activeElement === document.getElementById("bugdock-input"));
    await page.keyboard.type("markerone");
    await page.evaluate(() => window.__RICHOS_MOCK__.releaseDecline());
    await page.waitForSelector("#first-run[data-state='declined']");
    const afterDecline = await where();
    await page.keyboard.type(" markertwo");
    // Any other caller, the way home.js and questions.js reach the box.
    await page.evaluate(() => document.getElementById("input").focus());
    const afterOther = await where();
    await page.keyboard.type(" markerthree");
    const seen = { afterDecline, afterOther, panel: await page.inputValue("#bugdock-input"), conversation: await page.inputValue("#input") };
    assertEqual(seen, { afterDecline: "#bugdock-input", afterOther: "#bugdock-input", panel: "markerone markertwo markerthree", conversation: "" }, "focus left the panel's box");
    // Return is the report's: the card comes, and nothing went to Rich's conversation.
    await page.keyboard.press("Enter");
    await page.waitForSelector(".bugcard .bug-pill:has-text('Not sent yet')");
    assertEqual(await page.locator("#messages .tl-user").count(), before, "a message went to Rich's conversation");
    // At the card (runs F, G and J): the panel's box has the keys, and a caller does not take them.
    await page.locator("#bugdock-input").focus();
    await page.evaluate(() => document.getElementById("input").focus());
    assertEqual(await where(), "#bugdock-input", "at the card, focus left the panel's box");
    // The way out: once the panel is closed, the app focuses the box again.
    await page.click("#bug-cancel");
    await page.waitForSelector(".bugcard.is-canceled");
    await page.click("#bugdock-x");
    await page.waitForSelector("#bugdock", { state: "hidden" });
    await page.evaluate(() => document.getElementById("input").focus());
    assertEqual(await where(), "#input", "after the panel closed, the box cannot take focus");
    await page.close();
    return `after Not now finished: ${afterDecline}; after a caller outside main.js: ${afterOther}; panel "${seen.panel}", conversation "${seen.conversation}"; at the card: focus stayed; panel closed: #input focuses`;
  });

  // ---- nightly 49's walk, D29 (docs/verification/2026-10-10-nightly-49-candidate-walk.md) ----
  // Runs A and E on 13aa9bc0f: Change it, then Done, and the sticky button row showed a gold "Send
  // rep" inside Done's old 74px, with Change it and Cancel not drawn, while the accessibility tree
  // had all three at full size. Reproduced on 13aa9bc0f's files in Safari 18.6 on a macOS 15.7.7
  // test VM guest, 3 runs of 3: the row floats over a long report, and Done shrinks the card (the
  // editing hint and the dashed outline go, 1064 -> 1006px here) in the same update that swaps the
  // row's buttons, so the row's place in the flow moves while it stays put on screen, and the new
  // buttons were painted only where Done had been. The same page with the card's height held at
  // Done drew all three, and so did the same page with the row replaced by a new box; the old box
  // given its own layer (will-change), or the panel without its entry animation, did not. The fix
  // is that: `paintCard` puts in a new row at every paint, and a new box is painted whole.
  //
  // WHAT THIS CAN AND CANNOT SEE. This harness's WebKit paints the whole page for a screenshot, so
  // its pixels show all three buttons on 13aa9bc0f too (measured before this check was written);
  // the stale paint is only in the shipped WebKit's on-screen layers, which is where the guest
  // check looks. So this holds the condition the walk was in (the row floating, the card shorter
  // after Done), checks the pixels of every button anyway, and asserts the property the fix rests
  // on: the row after Done is a new box, not the one that held Done. Red on 13aa9bc0f by that.
  await run.check("D29: after Change it and Done, with the button row floating over a long report, the row is a new box and all three buttons are drawn at full width", async () => {
    const steps = ["Open a conversation.", "Type a message.", "Press Send twice, quickly.", "Watch the conversation.", "Nothing appears.", "Press Send again.", "Close the window.", "Open it again from the Dock.", "Press Send twice again.", "See that nothing appears again.", "Wait a minute.", "Press Send once more."];
    const page = await open("light", { bugSteps: steps }, { width: 1400, height: 864 });
    await bustABug(page);
    await answer(page, "When I press the Send button twice quickly, the second press does nothing and no message appears.");
    await page.click("#bug-change");
    await page.waitForSelector(".bugcard.is-editing");
    // The walk's view: the report's top in sight, the row floating over the rest of it.
    await page.evaluate(() => {
      const sc = document.getElementById("bugdock-msgs");
      sc.scrollTop += document.querySelector(".bugcard .bug-doc").getBoundingClientRect().top - sc.getBoundingClientRect().top - 40;
    });
    await page.waitForSelector(".bugcard .bug-actions.is-stuck");
    const before = await page.evaluate(() => {
      window.__d29Row = document.querySelector(".bugcard .bug-actions");
      return { card: document.querySelector(".bugcard").getBoundingClientRect().height, row: window.__d29Row.getBoundingClientRect().top };
    });
    await page.click("#bug-done");
    await page.waitForSelector(".bugcard .bug-pill:has-text('Not sent yet')");
    await page.evaluate(() => new Promise((r) => requestAnimationFrame(() => requestAnimationFrame(r))));
    const after = await page.evaluate(() => {
      const row = document.querySelector(".bugcard .bug-actions");
      const rr = row.getBoundingClientRect();
      return {
        card: document.querySelector(".bugcard").getBoundingClientRect().height,
        row: rr.top,
        stuck: row.classList.contains("is-stuck"),
        same: row === window.__d29Row,
        buttons: [...row.querySelectorAll(".desk-btn")].map((b) => { const r = b.getBoundingClientRect(); return { text: b.textContent, x: r.left - rr.left, y: r.top - rr.top, w: r.width, h: r.height }; }),
      };
    });
    // The walk's condition: the row floats where it was while the card got shorter under it.
    assert(after.stuck, "the row is not floating over the report after Done, so this is not the walk's view");
    assert(after.card < before.card, `the card did not get shorter at Done (${before.card} -> ${after.card}px), so this is not the walk's view`);
    assertEqual(Math.round(after.row), Math.round(before.row), "the row moved on screen at Done");
    assertEqual(after.buttons.map((b) => b.text), ["Send report", "Change it", "Cancel"], "the row's buttons after Done");
    // The pixels: down each button's right edge, for at least half its height, something other
    // than the row's ground is drawn (the gold fill, or the outline of Change it and Cancel).
    const shot = PNG.decode(await page.locator(".bugcard .bug-actions").screenshot());
    const at = (x, y) => { const i = (Math.round(y) * shot.width + Math.round(x)) * shot.channels; return [shot.data[i], shot.data[i + 1], shot.data[i + 2]]; };
    const last = after.buttons[after.buttons.length - 1];
    const ground = at(Math.min(shot.width - 4, last.x + last.w + 12), last.y + last.h / 2);
    const drawn = after.buttons.map((b) => {
      let marked = 0;
      for (let y = b.y + 1; y < b.y + b.h - 1; y++) {
        const p = at(Math.floor(b.x + b.w - 1), y);
        if (Math.max(...p.map((v, k) => Math.abs(v - ground[k]))) > 24) marked++;
      }
      return { text: b.text, marked };
    });
    assert(drawn.every((d, k) => d.marked > after.buttons[k].h / 2), "a button is not drawn to its right edge: " + JSON.stringify(drawn));
    // What the fix rests on: a new box, painted whole.
    assertEqual(after.same, false, "after Done the row is the box that held Done, emptied and refilled, not a new box");
    await page.close();
    return `card ${Math.round(before.card)} -> ${Math.round(after.card)}px with the row floating at ${Math.round(after.row)}px; buttons ${after.buttons.map((b) => b.text + " " + Math.round(b.w)).join(", ")}px; drawn to the right edge: ${drawn.map((d) => d.text + " " + d.marked + "px").join(", ")}; a new box: ${!after.same}`;
  });

  // ---- nightly 48's walk, D25 (docs/verification/2026-10-10-nightly-48-candidate-walk.md) ----
  // On d0d06501c the Rich panel showed a horizontal scroll bar under its messages, its thumb
  // almost the panel's width (runs B2 and P2): the content was a few pixels wider than the panel.
  //
  // WHAT THIS CAN AND CANNOT SEE. The guest has no trackpad, so macOS draws classic scroll bars that
  // take width from the box; this WebKit draws overlay ones that take none, and on the mock's words
  // nothing overflowed here, with or without a 15px classic bar, and a long unbroken word already
  // wraps (all measured before this check was written). The guest's exact few pixels were not
  // reproduced. So the check puts the walk's observed state in directly, under two declared
  // stand-ins: a 15px classic scroll bar on the messages (the guest's always-on bars) and one
  // element a few pixels wider than the box (the overflow the walk saw). No horizontal scroll bar
  // may be drawn under it. Then, with that element gone, the panel's own content must be no wider
  // than the box and reach past nothing, so the clip that keeps the bar away cuts nothing off.
  const CLASSIC_BARS = "#bugdock-msgs::-webkit-scrollbar{width:15px;height:15px;background:#5a6070}#bugdock-msgs::-webkit-scrollbar-thumb{background:#9aa0ad}";
  const sideways = (page, wider) =>
    page.evaluate((wider) => {
      const box = document.getElementById("bugdock-msgs");
      let probe = null;
      if (wider) {
        probe = document.createElement("div");
        probe.style.cssText = "flex:none;height:1px;width:calc(100% + " + wider + "px)";
        box.appendChild(probe);
      }
      const cs = getComputedStyle(box);
      const r = box.getBoundingClientRect();
      // The padding box's right edge: where the content may reach, the vertical bar left aside.
      const edge = r.left + parseFloat(cs.borderLeftWidth) + box.clientWidth;
      const wide = [...box.querySelectorAll("*")]
        .filter((n) => n !== probe && n.getClientRects().length && getComputedStyle(n).position !== "absolute" && n.getBoundingClientRect().right > edge + 0.5)
        .slice(0, 3)
        .map((n) => n.tagName.toLowerCase() + (typeof n.className === "string" && n.className ? "." + n.className.split(" ")[0] : "") + " +" + (n.getBoundingClientRect().right - edge).toFixed(1) + "px");
      const bar = box.offsetHeight - parseFloat(cs.borderTopWidth) - parseFloat(cs.borderBottomWidth) - box.clientHeight;
      const over = box.scrollWidth - box.clientWidth;
      if (probe) probe.remove();
      return { over, bar, wide };
    }, wider || 0);
  await run.check("the Rich panel's messages never scroll sideways: over Corrections and on the home screen, asked and with the card", async () => {
    const seen = [];
    for (const where of ["corrections", "home"]) {
      let page;
      if (where === "corrections") {
        page = await open("dark");
        await page.click("#nav-corrections");
        await page.waitForSelector("#corrections-overlay:not([hidden])");
      } else {
        page = await openCovered("home");
      }
      await page.addStyleTag({ content: CLASSIC_BARS });
      await bustABug(page);
      await page.waitForSelector("#bugdock:not([hidden]) .bug-rich .tl-prose");
      await settle(page);
      const asked = await sideways(page, 6);
      assertEqual(asked.bar, 0, where + ", asked, with content 6px wider than the panel: a horizontal scroll bar is drawn under the messages (" + asked.bar + "px)");
      await page.fill("#bugdock-input", "The names in the sidebar get cut off when I make the text bigger.");
      await page.keyboard.press("Enter");
      await page.waitForSelector("#bugdock .bugcard .bug-pill:has-text('Not sent yet')");
      await settle(page);
      const wider = await sideways(page, 6);
      assertEqual(wider.bar, 0, where + ", with the card and content 6px wider than the panel: a horizontal scroll bar is drawn under the messages (" + wider.bar + "px)");
      const own = await sideways(page);
      assertEqual(own.over, 0, where + ", with the card: the panel's own content is " + own.over + "px wider than the box: " + JSON.stringify(own.wide));
      assertEqual(own.wide, [], where + ", with the card: something of the panel's own reaches past its edge and is cut off");
      seen.push(`${where}: no sideways bar with content 6px wider (asked ${asked.bar}px, card ${wider.bar}px); own content ${own.over}px wider`);
      await page.close();
    }
    return seen.join("; ");
  });

  // ---- nightly 48's walk, D24 (docs/verification/2026-10-10-nightly-48-candidate-walk.md) ----
  //
  // On d0d06501c, with Corrections open and focus inside it, the Settings button was missing from
  // the accessibility tree and a press at its place did nothing (runs B, B3, R, K; once it worked,
  // run Q, with focus left on the rail). Corrections claimed `aria-modal="true"`, and WebKit
  // leaves everything outside a modal dialog that holds focus out of the tree, so the press the
  // walk makes (System Events `click at`, which presses the accessibility element at a point)
  // never reached the button. A pointer is not the only way in; the tree is how VoiceOver gets
  // there too.
  //
  // IN THE TREE, COMPUTED: Playwright 1.61 ships no accessibility snapshot (control-names.js says
  // the same), so this applies WebKit's rules from the DOM: not hidden, not `inert`, not under
  // `aria-hidden`, and not outside an `aria-modal` dialog that holds focus. An approximation of
  // one platform's tree, declared as one; it is the rule that was broken.
  const settingsReach = (page) =>
    page.evaluate(() => {
      const btn = document.getElementById("set-btn");
      const r = btn.getBoundingClientRect();
      const hit = document.elementFromPoint(r.left + r.width / 2, r.top + r.height / 2);
      const named = (n) => (n ? n.tagName.toLowerCase() + (n.id ? "#" + n.id : "") : "nothing");
      let why = null;
      for (let n = btn; n && n.nodeType === 1 && !why; n = n.parentElement) {
        if (n.hidden || n.hasAttribute("inert") || n.getAttribute("aria-hidden") === "true") why = "hidden or inert at " + named(n);
      }
      const focus = document.activeElement;
      for (const modal of document.querySelectorAll('[aria-modal="true"]')) {
        if (why || modal.hidden || !modal.getClientRects().length) continue;
        if (modal.contains(focus) && !modal.contains(btn)) why = "outside " + named(modal) + ", an aria-modal dialog that holds focus (on " + named(focus) + ")";
      }
      return { top: hit === btn || btn.contains(hit) ? "ON-TOP" : "COVERED by " + named(hit), tree: why || "IN THE TREE", focus: named(focus) };
    });
  // Each overlay with the presses, in order, that open it the way a person does.
  const OVERLAYS = [
    ["Corrections", ["#nav-corrections"], "#corrections-overlay"],
    ["Feedback", ["#nav-feedback"], "#feedback-overlay"],
    ["Search", ["#nav-search"], "#search-overlay"],
    ["the company picker", [".nav-group--unbound .nav-thread", "#unbound-new-thread"], "#entity-picker"],
    // The second review of 9463220ef (rv-20261010T072702Z-9463220e-543f), finding 2, fixture
    // `settings-techy-scope.js`: opened from Settings in a conversation, the Technical view question
    // was aria-modal at z-index 310 over Settings (300), and focus went to its first choice.
    ["the Technical view question", ["#set-btn", "#set-techy"], "#techy-scope"],
    // The review of 66ba53861 (rv-20261010T081721Z-66ba5386-a4d1), finding 2, fixture
    // `settings-overlays.js`: Settings -> Home screen opened an aria-modal panel at z-index 340
    // over Settings (300). Every other overlay is found, not listed, by appearance.js check 7c.
    ["the Home screen buttons", ["#set-btn", "#set-home-open"], "#home-prefs"],
  ];
  await run.check("with Corrections open and focus inside it, the Settings button is on top, in the accessibility tree, and opens the menu; the same over every overlay it sits above", async () => {
    const seen = [];
    for (const [label, openers, overlay] of OVERLAYS) {
      const page = await open("dark");
      for (const opener of openers) await page.click(opener);
      await page.waitForSelector(overlay + ":not([hidden])");
      // The failing state is the normal one: focus inside the overlay. Where the overlay does not
      // move it there itself, it is put on its first control, as Tab would.
      await page.evaluate((sel) => {
        const o = document.querySelector(sel);
        if (!o.contains(document.activeElement)) o.querySelector("button, input, select, textarea, [tabindex]:not([tabindex='-1'])").focus();
      }, overlay);
      await page.waitForFunction((sel) => document.querySelector(sel).contains(document.activeElement), overlay);
      const reach = await settingsReach(page);
      assertEqual(reach.top, "ON-TOP", label + ": the Settings button is not what is painted at its place");
      assertEqual(reach.tree, "IN THE TREE", label + ": the Settings button is not in the accessibility tree");
      // Pressed the way a person does: the pointer at its place.
      const r = await page.locator("#set-btn").boundingBox();
      await page.mouse.click(r.x + r.width / 2, r.y + r.height / 2);
      await page.waitForSelector("#set-menu:not([hidden]) #bug-btn");
      assert(await page.isVisible(overlay), label + " closed when Settings was pressed");
      if (label === "Corrections") {
        // And Bust a bug starts from there: the Rich panel opens beside Corrections, which stays.
        await page.click("#bug-btn");
        await page.waitForSelector("#bugdock:not([hidden]) .bug-rich .tl-prose");
        assert(await page.isVisible(overlay), "Corrections closed when Bust a bug started");
      }
      seen.push(`${label}: ${reach.top}, ${reach.tree}, focus on ${reach.focus}, menu opened`);
      await page.close();
    }
    return seen.join("; ");
  });

  await run.check("the privacy heads-up reads a line break typed in a change as a break, so a name on two lines is still named", async () => {
    // Finding 2, fixture `line-break-warning.js`: on 665df1bb3 Send read the card with
    // `wordsOf()`, the heads-up with `textContent`, so "Jane" and "Doe" on two lines reached the
    // heads-up as "JaneDoe" and the name was never named.
    const page = await open("dark", { bugRichPrivate: [{ text: "Jane Doe", kind: "person_name" }] });
    await bustABug(page);
    await answer(page, "The window froze while opening the plan.");
    await page.click("#bug-change");
    await page.waitForSelector(".bugcard.is-editing");
    await page.evaluate(() => {
      const p = document.querySelector(".bugcard .bug-sec p");
      p.focus();
      const r = document.createRange();
      r.selectNodeContents(p);
      r.collapse(false);
      const sel = window.getSelection();
      sel.removeAllRanges();
      sel.addRange(r);
    });
    await page.keyboard.type(" Jane");
    await page.keyboard.press("Enter");
    await page.keyboard.type("Doe saw it.");
    const html = await page.locator(".bugcard .bug-sec p").first().evaluate((p) => p.innerHTML);
    await page.waitForFunction(() => window.__RICHOS_MOCK_BUG__.calls.some((c) => c.cmd === "bug_report_private_in" && String(c.text).includes("Doe saw it.")));
    const asked = (await calls(page, "bug_report_private_in")).filter((c) => String(c.text).includes("Doe saw it.")).pop().text;
    assert(!asked.includes("JaneDoe") && /Jane\s+Doe saw it\./.test(asked), `the heads-up was asked about joined words: ${JSON.stringify(asked)} (the paragraph was ${html})`);
    await page.waitForSelector(".bug-warn:not([hidden])");
    const warn = await page.locator(".bug-warn").innerText();
    assertEqual(warn, "“Jane Doe” looks private. Anyone can read this report on GitHub.", "heads-up");
    await page.close();
    return `asked ${JSON.stringify(asked)}; ${warn}`;
  });

  // ---- the review of b3b5c573d (rv-20261009T225657Z-b3b5c573-96c0) ----
  // On b3b5c573d the card's fields were rich text, so a nested list pasted into a step put a step
  // inside a step, and Send read every <li> while each one's words already held its children's:
  // "Choose Appearance" went out twice under a card that showed it once. A pasted table's cells
  // were read run together. Each check compares what Send read with what the card showed.
  const STEPS = ["Open Settings", "Press Text size"];
  const once = (s, w) => s.split(w).length - 1;
  /// Done, then Send: what the card showed when Send was pressed and what Send read
  /// (`bug_report_send`'s sheet), each with its runs of white space as one space.
  async function doneAndSend(page) {
    await page.click("#bug-done");
    await page.waitForSelector(".bugcard .bug-pill:has-text('Not sent yet · changed')");
    const shown = await page.locator(".bugcard .bug-doc").innerText();
    await page.click("#bug-send");
    await page.waitForFunction(() => window.__RICHOS_MOCK_BUG__.calls.some((c) => c.cmd === "bug_report_send"));
    const sheet = (await calls(page, "bug_report_send"))[0].sheet;
    const read = [sheet.title, ...sheet.sections.flatMap((s) => [s.heading, ...s.paragraphs, ...s.steps])].join("\n");
    return { shown: shown.replace(/\s+/g, " ").trim(), read: read.replace(/\s+/g, " ").trim() };
  }
  /// Change it, with the cursor at the end of the card's first `selector`.
  async function changeAtEnd(page, selector) {
    await page.click("#bug-change");
    await page.waitForSelector(".bugcard.is-editing");
    await page.evaluate((sel) => {
      const n = document.querySelector(".bugcard " + sel);
      n.focus();
      const r = document.createRange();
      r.selectNodeContents(n);
      r.collapse(false);
      const s = window.getSelection();
      s.removeAllRanges();
      s.addRange(r);
    }, selector);
  }
  /// A paste of a rich-text clipboard (`html`, and `plain` as its plain text) where the cursor is,
  /// the way WebKit pastes: the page's `paste` handler first, and if the page does not take it, a
  /// rich-text field gets the clipboard's HTML (`insertHTML` is WebKit's same replace-selection
  /// command). No pasteboard of this Mac is read or written.
  async function pasteRich(page, html, plain) {
    return page.evaluate(({ html, plain }) => {
      const at = window.getSelection().anchorNode;
      const el = at.nodeType === 1 ? at : at.parentElement;
      const dt = new DataTransfer();
      dt.setData("text/html", html);
      dt.setData("text/plain", plain);
      const taken = !el.dispatchEvent(new ClipboardEvent("paste", { clipboardData: dt, bubbles: true, cancelable: true }));
      if (!taken) document.execCommand("insertHTML", false, html);
      return taken;
    }, { html, plain });
  }
  /// Elements in a field other than WebKit's own line breaks (a <div> per line, a <br>) and the
  /// card's stand-ins: anything here came in with a paste.
  const markupIn = (page, selector) => page.evaluate((sel) =>
    [...document.querySelector(".bugcard " + sel).querySelectorAll("*")].filter((n) => !/^(DIV|BR)$/.test(n.nodeName) && !n.classList.contains("bug-sub")).map((n) => n.nodeName.toLowerCase()), selector);

  await run.check("a step that holds a nested list is read once: Send reads what the card shows", async () => {
    // The fixture `card-reader-probe.js`: the shape a rich-text paste left in a step on b3b5c573d.
    const page = await open("dark", { bugSteps: STEPS });
    await bustABug(page);
    await answer(page, "The window froze while opening the plan.");
    await changeAtEnd(page, ".bug-sec li");
    await page.evaluate(() => { document.querySelector(".bugcard .bug-sec li").innerHTML = "Open Settings<ul><li>Choose Appearance</li></ul>"; });
    const { shown, read } = await doneAndSend(page);
    assertEqual(once(shown, "Choose Appearance"), 1, "the card does not show it once");
    assertEqual(once(read, "Choose Appearance"), 1, "Send read it " + once(read, "Choose Appearance") + " times: " + read);
    assertEqual(read, shown, "what Send read is not what the card showed");
    await page.close();
    return "Choose Appearance read " + once(read, "Choose Appearance") + " time(s); Send reads the card";
  });

  await run.check("a nested list pasted into a step comes in as plain text, and Send reads what the card shows", async () => {
    const page = await open("dark", { bugSteps: STEPS });
    await bustABug(page);
    await answer(page, "The window froze while opening the plan.");
    await changeAtEnd(page, ".bug-sec li");
    await page.keyboard.press("Enter");
    await pasteRich(page, "<ul><li>Choose Appearance<ul><li><span style=\"color:#c00;font-weight:700\">Pick Dark</span></li></ul></li></ul>", "Choose Appearance\n\tPick Dark");
    const html = await page.locator(".bugcard .bug-sec li").first().evaluate((n) => n.innerHTML);
    const markup = await markupIn(page, ".bug-sec li");
    const { shown, read } = await doneAndSend(page);
    assertEqual(read, shown, "what Send read is not what the card showed (the step was " + html + ")");
    for (const w of ["Choose Appearance", "Pick Dark"]) assertEqual(once(read, w), 1, `Send read "${w}" ${once(read, w)} times: ${read}`);
    assertEqual(JSON.stringify(markup), "[]", "pasted markup reached the step: " + html);
    await page.close();
    return "step " + html;
  });

  await run.check("a table pasted into a paragraph comes in as plain text, and Send reads what the card shows", async () => {
    const page = await open("dark");
    await bustABug(page);
    await answer(page, "The window froze while opening the plan.");
    await changeAtEnd(page, ".bug-sec p");
    await pasteRich(page,
      "<table><tr><th>Setting</th><th>Value</th></tr><tr><td>Theme</td><td><b style=\"color:#c00\">Dark</b></td></tr></table>",
      "Setting\tValue\nTheme\tDark");
    const html = await page.locator(".bugcard .bug-sec p").first().evaluate((n) => n.innerHTML);
    const markup = await markupIn(page, ".bug-sec p");
    const { shown, read } = await doneAndSend(page);
    assertEqual(read, shown, "what Send read is not what the card showed (the paragraph was " + html + ")");
    for (const w of ["Setting Value", "Theme Dark"]) assert(read.includes(w), `"${w}" is not what Send read: ${read}`);
    assertEqual(JSON.stringify(markup), "[]", "pasted markup reached the paragraph: " + html);
    await page.close();
    return "paragraph " + html;
  });

  // ---- the review of 3007e3200 (rv-20261009T174727Z-3007e320-578a) ----
  const RICH_NAMES = [{ text: "Jane Doe", kind: "person_name" }, { text: "SecretCo", kind: "company_name" }];
  const SAID_BY_HAND = "Jane Doe at SecretCo saw the window freeze while opening the plan.";
  /// Change it, replace the first paragraph with `words` typed by hand, and Done.
  async function changeByHand(page, words) {
    await page.click("#bug-change");
    await page.waitForSelector(".bugcard.is-editing");
    await page.evaluate(() => {
      const p = document.querySelector(".bugcard .bug-sec p");
      p.focus();
      const r = document.createRange();
      r.selectNodeContents(p);
      const sel = window.getSelection();
      sel.removeAllRanges();
      sel.addRange(r);
    });
    await page.keyboard.type(words);
    await page.click("#bug-done");
    await page.waitForSelector(".bugcard .bug-pill:has-text('Not sent yet · changed')");
  }

  await run.check("words changed by hand go out only after Rich has checked them, and wait on this Mac when he can't", async () => {
    // Finding 1, fixture `manual_edit.js`: on 3007e3200 Change it → Done made the user's own words
    // sendable after only the heads-up, which cannot know an ordinary client's name, and Send
    // filed "Jane Doe at SecretCo saw the window freeze while opening the plan." with no heads-up.
    // `mock.js` decides Send as the shell does (`richos_core::bug_report::decide`, tested in Rust).
    const page = await open("dark", { bugSendCheckHold: true, bugRichCheck: RICH_NAMES });
    await bustABug(page);
    await answer(page, "The window froze while opening the plan.");
    await changeByHand(page, SAID_BY_HAND);
    await page.waitForFunction((said) => window.__RICHOS_MOCK_BUG__.calls.some((c) => c.cmd === "bug_report_private_in" && c.text.includes(said)), SAID_BY_HAND);
    assert(await page.isHidden(".bug-warn"), "a heads-up for names no rule can know");
    await page.click("#bug-send");
    await page.waitForSelector(".bug-working:has-text('Rich is checking your changes…')");
    await page.waitForSelector(".bugcard .bug-pill:has-text('Rich is checking your changes…')");
    assertEqual((await filed(page)).length, 0, "the changed words went out before Rich checked them");
    await page.evaluate(() => window.__RICHOS_MOCK_BUG__.releaseSendCheck());
    await page.waitForSelector("#bugdock-msgs .bug-rich:has-text('I checked it again before sending')");
    assertEqual(await page.locator(".bugcard").count(), 1, "the card shown again is a second card");
    assertEqual(await page.locator(".bug-working").count(), 0, "Rich is still shown checking");
    const again = await page.locator(".bugcard .bug-doc").innerText();
    assert(!again.includes("Jane Doe") && !again.includes("SecretCo") && again.includes("[a person] at [a company] saw the window freeze"), again);
    assertEqual((await filed(page)).length, 0, "it went out before the user saw what Rich left out");
    await page.click("#bug-send");
    await page.waitForSelector(".bugcard.is-sent");
    const out = JSON.stringify(await filed(page));
    assert(!out.includes("Jane Doe") && !out.includes("SecretCo") && out.includes("[a person] at [a company]"), out);
    await page.close();

    // Claude can't check them: nothing is sent, the words wait on this Mac with no Send offered,
    // and the card comes back once he has checked them.
    const down = await open("dark", { bugRichCheck: RICH_NAMES });
    await bustABug(down);
    await answer(down, "The window froze while opening the plan.");
    await down.evaluate(() => window.__RICHOS_MOCK_BUG__.setClaude("down"));
    await changeByHand(down, SAID_BY_HAND);
    await down.click("#bug-send");
    await down.waitForSelector("#bugdock-msgs .bug-rich:has-text(\"I couldn't check your changes for private details yet\")");
    await down.waitForSelector(".bugcard .bug-pill:has-text(\"Waiting for Rich's check · saved on this Mac\")");
    const said = await lastSaid(down);
    assertEqual(await down.locator("#bug-send").count(), 0, "Send is offered for words Rich hasn't checked");
    assert(await down.isVisible("#bug-unchecked-cancel"), "no way to cancel the words that wait");
    const kept = await down.evaluate(() => window.__RICHOS_MOCK_BUG__.unchecked());
    assert(kept.length === 1 && JSON.stringify(kept[0].edited).includes(SAID_BY_HAND), "the changed words are not kept: " + JSON.stringify(kept));
    assertEqual((await filed(down)).length, 0, "something went out");
    await down.evaluate(() => window.__RICHOS_MOCK_BUG__.claudeAnswers());
    await down.waitForSelector("#bugdock-msgs .bug-rich:has-text(\"I've checked your changes now.\")");
    assertEqual(await down.locator(".bugcard").count(), 1, "the checked card is a second card");
    const checked = await down.locator(".bugcard .bug-doc").innerText();
    assert(!checked.includes("Jane Doe") && !checked.includes("SecretCo"), checked);
    await down.waitForFunction(() => window.__RICHOS_MOCK_BUG__.unchecked().length === 0);
    assertEqual((await filed(down)).length, 0, "it went out before the user pressed Send");
    await down.click("#bug-send");
    await down.waitForSelector(".bugcard.is-sent");
    assert(!JSON.stringify(await filed(down)).includes("Jane Doe"), "the name went out");
    await down.close();
    return said;
  });

  // =====================================================================================
  // REQUEST A FEATURE (CEO 2026-10-11, richos-hq docs/ceo-input/2026-10-11_01/request.md, built to
  // round 24): "Under the "Bust a bug" button, I want to add a "Request a feature" button. But I
  // don't want feature requests to be submitted or branded as "issues" on GitHub." The same
  // exchange, carrying an idea, posted in RichOS's Discussions under Ideas. Where it goes (GraphQL
  // `createDiscussion`, never /issues) is the shell's, tested in Rust
  // (crates/richos-core/tests/feature_request_tests.rs); here every command is told `post: "idea"`
  // and `mock.js` posts it as a discussion.
  // =====================================================================================
  const IDEA = "It would be great if Rich sent me a short summary of each conversation every Friday afternoon, like what changed in the Acme deal this week.";
  async function requestAFeature(page) {
    await page.click("#set-btn");
    await page.waitForSelector("#idea-btn");
    await page.click("#idea-btn");
  }
  async function answerIdea(page, text) {
    await page.fill("#bugdock-input", text || IDEA);
    await page.keyboard.press("Enter");
    await page.waitForSelector(".bugcard.is-idea .bug-pill:has-text('Not posted yet')");
  }
  /// Every word a feature request puts on screen, in the panel and its notices: none may say
  /// "issue" or "bug".
  const NEVER_AN_ISSUE = /\b(issues?|bugs?)\b/i;
  const ideaWords = (page) => page.evaluate(() => [
    document.getElementById("bugdock").innerText,
    (document.getElementById("bug-toast") || {}).innerText || "",
    [...document.querySelectorAll("#bugdock [aria-label], #bugdock[aria-label]")].map((n) => n.getAttribute("aria-label")).join(" "),
    document.getElementById("bugdock-input").placeholder,
  ].join("\n"));

  await run.check("Request a feature sits directly under Bust a bug in Settings and starts Rich's question about an idea in the Rich panel", async () => {
    const page = await open("dark");
    await page.click("#set-btn");
    await page.waitForSelector("#idea-btn");
    const order = await page.evaluate(() => {
      const bug = document.getElementById("bug-btn"), idea = document.getElementById("idea-btn");
      return { next: bug.nextElementSibling === idea, label: idea.textContent.trim(), below: idea.getBoundingClientRect().top >= bug.getBoundingClientRect().bottom, icons: idea.querySelectorAll("svg").length };
    });
    assert(order.next && order.below && order.label === "Request a feature" && order.icons === 1, "Request a feature is not directly under Bust a bug: " + JSON.stringify(order));
    await page.click("#idea-btn");
    await page.waitForSelector("#bugdock:not([hidden]) #bugdock-msgs .bug-rich");
    assert(await page.isHidden("#set-menu"), "Settings stayed open");
    assertEqual(await page.locator("#bugdock .bugdock-title").innerText(), "Request a feature", "the panel's title");
    const asked = await page.locator("#bugdock-msgs .bug-rich .tl-prose").first().innerText();
    assertEqual(asked, "What would you like RichOS to do? Tell me in your own words, typed or out loud. I've noted that you were on the Acme deal conversation, in case it's about this screen.", "Rich's question");
    assertEqual(await page.getAttribute("#bugdock-input", "placeholder"), "Tell Rich your idea…", "the composer");
    await page.waitForFunction(() => document.activeElement === document.getElementById("bugdock-input"));
    assert(!NEVER_AN_ISSUE.test(await ideaWords(page)), "a feature request says issue or bug: " + (await ideaWords(page)));
    await page.click("#bug-never-mind");
    await page.waitForSelector("#bugdock-msgs .bug-rich:has-text('No problem. Nothing was written up or posted.')");
    assertEqual((await calls(page, "bug_report_write")).length, 0, "something was written up");
    await page.close();
    return asked;
  });

  await run.check("the idea's card shows From, To Discussions under Ideas and the whole post before anything goes, leaves private names out, and Post idea posts it in Ideas, never as an issue", async () => {
    const page = await open("dark", { ideaPictured: ["A message every Friday afternoon.", "One short part per conversation."] });
    await requestAFeature(page);
    await answerIdea(page);
    assertEqual((await calls(page, "bug_report_write"))[0].post, "idea", "Rich was not asked to write up an idea");
    assertEqual((await calls(page, "bug_report_send")).length, 0, "something was posted before Post idea");
    assertEqual(await page.locator(".bugcard .bug-kind").innerText(), "Feature idea", "the card's head");
    assertEqual(await page.getAttribute(".bugcard", "aria-label"), "Feature idea", "the card's name");
    assertEqual(await page.locator(".bugcard .r-from").innerText(), "the RichOS reporting account, because RichOS isn't signed in to a GitHub account of yours", "From");
    const to = await page.locator(".bugcard .bug-route dd").nth(1).innerText();
    assertEqual(to.replace(/\s+/g, " ").trim(), "RichOS on GitHub, as a post in its Discussions, under Ideas, where anyone can read and upvote it", "To");
    assertEqual(await page.locator(".bugcard .idea-cat").innerText(), "Ideas", "the Ideas label");
    const headings = await page.locator(".bugcard .bug-sec h4").allInnerTexts();
    assertEqual(headings, ["The idea", "What the user pictured", "Where it came up", "Version"], "the post's sections");
    assertEqual(await page.locator(".bugcard .bug-sec ul > li").count(), 2, "What the user pictured is not a bulleted list");
    const sheet = await page.locator(".bugcard .bug-doc").innerText();
    assert(!sheet.includes("Acme deal") && sheet.includes("[a conversation]"), "a private name is in the post: " + sheet);
    assert((await page.locator(".bugcard .lo-text").innerText()).startsWith("Left out, because anyone can read GitHub Discussions: one conversation name."), "the left-out line");
    assertEqual(await page.locator("#bug-send").innerText(), "Post idea", "the post button");
    assertEqual(await page.locator("#bug-cancel").innerText(), "Cancel", "Cancel");
    assertEqual(await page.getAttribute("#bugdock-input", "placeholder"), "Tell Rich what to change or post it…", "the composer");
    assert((await lastSaid(page)).startsWith("Here's your idea as I'd post it. Nothing goes out until you press Post idea. Anyone can read GitHub Discussions"), await lastSaid(page));
    const before = await ideaWords(page);
    assert(!NEVER_AN_ISSUE.test(before), "a feature request says issue or bug: " + before);
    await shot(page, "idea-report-dark");

    await page.click("#bug-send");
    await page.waitForSelector(".bugcard.is-sent .bug-pill:has-text('Posted · in Ideas')");
    const sent = await calls(page, "bug_report_send");
    assertEqual(sent.length, 1, "Post idea did not post once");
    assertEqual(sent[0].post, "idea", "the shell was not told it is an idea");
    const posted = await page.evaluate(() => window.__RICHOS_MOCK_BUG__.ideasPosted());
    assertEqual(posted.length, 1, "nothing went to Discussions");
    assertEqual((await filed(page)).filter((x) => x.post !== "idea").length, 0, "an idea went out as an issue");
    assert(!JSON.stringify(posted).includes("Acme deal"), "a private name was posted: " + JSON.stringify(posted));
    assertEqual(posted[0].sections[1].steps, ["A message every Friday afternoon.", "One short part per conversation."], "the pictured points as posted");
    const said = await lastSaid(page);
    assertEqual(said, "Posted. It's in RichOS's Discussions on GitHub, under Ideas, posted from the RichOS reporting account, because RichOS isn't signed in to a GitHub account of yours. Anyone can read it and upvote it there.\ngithub.com/WebDevBooster/richos/discussions/413", "Rich's line");
    assert(await page.locator(".bugcard .bulb-wrap.lit").count() === 1, "the bulb did not light");
    const row = await page.locator(".bugcard .bug-sentrow").innerText();
    assert(row.includes("Open it on GitHub") && row.includes("posted from the RichOS reporting account") && !/#\d/.test(row), "the posted card's row: " + row);
    const after = await ideaWords(page);
    assert(!NEVER_AN_ISSUE.test(after), "a posted idea says issue or bug: " + after);
    await page.click(".bugcard .bug-link");
    const opened = await calls(page, "bug_report_open_issue");
    assert(opened.length === 1 && opened[0].post === "idea" && opened[0].url === "https://github.com/WebDevBooster/richos/discussions/413", "the link does not open the discussion: " + JSON.stringify(opened));
    await page.close();
    return said;
  });

  // THE GITHUB MOVE (CEO, 2026-10-27): once the relay files on `RichCore/RichOS`, Rich names that
  // address and the card opens it; nothing in the window spells the repository itself.
  await run.check("after the GitHub move, a report and an idea name and open RichCore/RichOS", async () => {
    const page = await open("light", { bugRepository: "RichCore/RichOS" });
    await bustABug(page);
    await answer(page, ANSWER);
    await page.click("#bug-send");
    await page.waitForSelector(".bugcard.is-sent .bug-pill:has-text('Sent · #412')");
    assert((await lastSaid(page)).includes("github.com/RichCore/RichOS/issues/412"), await lastSaid(page));
    await page.click(".bug-sentrow .bug-link");
    assertEqual((await calls(page, "bug_report_open_issue"))[0].url, "https://github.com/RichCore/RichOS/issues/412", "Open issue");
    await page.close();
    const idea = await open("light", { bugRepository: "RichCore/RichOS" });
    await requestAFeature(idea);
    await answerIdea(idea);
    await idea.click("#bug-send");
    await idea.waitForSelector(".bugcard.is-sent .bug-pill:has-text('Posted · in Ideas')");
    assert((await lastSaid(idea)).endsWith("\ngithub.com/RichCore/RichOS/discussions/413"), await lastSaid(idea));
    await idea.click(".bugcard .bug-link");
    assertEqual((await calls(idea, "bug_report_open_issue"))[0].url, "https://github.com/RichCore/RichOS/discussions/413", "Open it");
    await idea.close();
    return "github.com/RichCore/RichOS/issues/412 and /discussions/413";
  });

  await run.check("an idea goes out from the user's own GitHub account when RichOS has one, and says so", async () => {
    const page = await open("light", { bugAccount: { kind: "user", login: "WebDevBooster" } });
    await requestAFeature(page);
    await answerIdea(page);
    assertEqual(await page.locator(".bugcard .r-from").innerText(), "your GitHub account, @WebDevBooster", "From");
    await page.click("#bug-send");
    await page.waitForSelector(".bugcard.is-sent");
    const said = await lastSaid(page);
    assert(said.includes("posted from your GitHub account, @WebDevBooster. Anyone can read it and upvote it there."), said);
    await page.close();
    return said.split("\n")[0];
  });

  await run.check("offline, the idea waits on this Mac and posts by itself when back online; an account that may not post there is said honestly", async () => {
    const page = await open("dark", { bugNet: "offline" });
    await requestAFeature(page);
    await answerIdea(page);
    await page.click("#bug-send");
    await page.waitForSelector(".bugcard.is-queued .bug-pill:has-text('Waiting to post · saved on this Mac')");
    const waiting = await lastSaid(page);
    assertEqual(waiting, "This Mac is offline, so your idea wasn't posted. Nothing is lost: it's saved on this Mac exactly as you approved it. I'll post it by myself as soon as you're back online and tell you here when it's up.", "Rich's line");
    assertEqual(await page.locator("#bug-try-now").innerText(), "Try now", "Try now");
    assertEqual(await page.locator("#bug-cancel").innerText(), "Cancel post", "Cancel post");
    assert(!NEVER_AN_ISSUE.test(await ideaWords(page)), "a waiting idea says issue or bug: " + (await ideaWords(page)));
    await page.evaluate(() => window.__RICHOS_MOCK_BUG__.setNet("online"));
    await page.waitForSelector(".bugcard.is-sent .bug-pill:has-text('Posted · in Ideas')");
    const back = await lastSaid(page);
    assert(back.startsWith("You're back online, so I posted your idea. It's in RichOS's Discussions on GitHub, under Ideas,"), back);
    await page.waitForSelector("#bug-toast:has-text('Your idea is posted in Ideas, on GitHub.')");
    assertEqual((await page.evaluate(() => window.__RICHOS_MOCK_BUG__.ideasPosted())).length, 1, "it was not posted once");
    await page.close();

    const refused = await open("dark", { bugNet: "refused" });
    await requestAFeature(refused);
    await answerIdea(refused);
    await refused.click("#bug-send");
    await refused.waitForSelector(".bugcard.is-queued");
    const honest = await lastSaid(refused);
    assertEqual(honest, "GitHub didn't let the RichOS reporting account post in RichOS's Discussions, so your idea wasn't posted. Nothing is lost: it's saved on this Mac exactly as you approved it. I'll try again in a few minutes and tell you here when it's up.", "the refused line");
    await refused.close();
    return waiting + " / " + honest;
  });

  // D30 (nightly 51 walk): a queued card's long pill ("Waiting to post · saved on this Mac") took the
  // head's width, broke the title onto two lines and ran past the card's edge. The pill drops under
  // the title when both do not fit; the title stays on one line and the pill stays inside the card.
  await run.check("a queued idea or bug card keeps its title on one line and its pill inside the card, in light and dark", async () => {
    const seen = [];
    // 1 = default text size; 1.5 = the 150% setting (review rv-20261013T021147Z-978eb49c-a02f: the nowrap
    // pill outgrew the fixed 452px panel there and ran past the card).
    for (const scale of [1, 1.5]) {
    for (const theme of ["light", "dark"]) {
      for (const kind of ["idea", "bug"]) {
        const page = await open(theme, { bugNet: "offline" });
        await page.evaluate((sc) => document.documentElement.style.setProperty("--app-font-scale", String(sc)), scale);
        if (kind === "idea") { await requestAFeature(page); await answerIdea(page); }
        else { await bustABug(page); await answer(page, ANSWER); }
        await page.click("#bug-send");
        await page.waitForSelector(".bugcard.is-queued .bug-pill");
        const m = await page.evaluate(() => {
          const card = document.querySelector(".bugcard.is-queued");
          const c = card.getBoundingClientRect();
          const k = card.querySelector(".bug-kind");
          const pillEl = card.querySelector(".bug-pill");
          const pill = pillEl.getBoundingClientRect();
          const pillLh = parseFloat(getComputedStyle(pillEl).lineHeight) || parseFloat(getComputedStyle(pillEl).fontSize) * 1.4;
          const lh = parseFloat(getComputedStyle(k).lineHeight) || parseFloat(getComputedStyle(k).fontSize) * 1.4;
          return { cardRight: c.right, pillRight: pill.right, pillLeft: pill.left, cardLeft: c.left, kindH: k.getBoundingClientRect().height, lh, cardW: c.width, pillW: pill.width, pillH: pill.height, pillLh };
        });
        assert(m.kindH < m.lh * 1.5, `${scale * 100}% ${theme} ${kind}: the title breaks onto two lines (height ${m.kindH}px, line ${m.lh}px)`);
        assert(m.pillRight <= m.cardRight - 1 && m.pillLeft >= m.cardLeft, `${scale * 100}% ${theme} ${kind}: the pill runs outside the card (pill ${m.pillLeft}..${m.pillRight}, card ${m.cardLeft}..${m.cardRight})`);
        // The pill's own text: one line at 100%, wrapped (taller than one and a half lines) at 150%.
        const pillLines = m.pillH > m.pillLh * 1.5;
        assert(pillLines === (scale > 1), `${scale * 100}% ${theme} ${kind}: the pill text ${pillLines ? "wraps" : "stays on one line"} (height ${m.pillH}px, line ${m.pillLh}px), expected ${scale > 1 ? "wrapped" : "one line"}`);
        seen.push(`${scale * 100}% ${theme} ${kind} card ${Math.round(m.cardW)}px pill ${Math.round(m.pillW)}px title ${Math.round(m.kindH)}px pill ${Math.round(m.pillH)}px high`);
        await page.close();
      }
    }
    }
    return seen.join("; ");
  });

  // Review rv-20261011T131221Z-ba6a6a47-c22a, P2: Try now that fails again for ANOTHER reason said
  // "It still didn't go out…" and hid the new reason (offline, then an account GitHub refuses).
  await run.check("Try now that fails for a different reason says the new reason, for an idea and a bug report", async () => {
    const page = await open("dark", { bugNet: "offline" });
    await requestAFeature(page);
    await answerIdea(page);
    await page.click("#bug-send");
    await page.waitForSelector(".bugcard.is-queued");
    await page.evaluate(() => window.__RICHOS_MOCK_BUG__.setNet("refused"));
    await page.click("#bug-try-now");
    await page.waitForSelector("#bugdock-msgs .bug-rich:has-text(\"GitHub didn't let the RichOS reporting account post\")");
    const idea = await lastSaid(page);
    // The same reason again is the short line.
    await page.click("#bug-try-now");
    await page.waitForFunction(() => /It still didn't go out/.test([...document.querySelectorAll("#bugdock-msgs .bug-rich")].pop().innerText));
    await page.close();

    const bug = await open("dark", { bugNet: "offline" });
    await bustABug(bug);
    await answer(bug, ANSWER);
    await bug.click("#bug-send");
    await bug.waitForSelector(".bugcard.is-queued");
    await bug.evaluate(() => window.__RICHOS_MOCK_BUG__.setNet("not-set-up"));
    await bug.click("#bug-try-now");
    await bug.waitForSelector("#bugdock-msgs .bug-rich:has-text(\"The RichOS reporting account isn't set up on this Mac yet\")");
    const report = await lastSaid(bug);
    await bug.close();
    return idea.slice(0, 80) + "… / " + report.slice(0, 80) + "…";
  });

  await run.check("Change it and Cancel on an idea speak of the post, and while an idea is open both buttons in Settings are disabled", async () => {
    const page = await open("dark");
    await requestAFeature(page);
    await answerIdea(page);
    await page.fill("#bugdock-input", "Also say I'd like to pick the day and time myself");
    await page.keyboard.press("Enter");
    await page.waitForSelector("#bugdock-msgs .bug-rich:has-text('Added that to The idea, above. It still waits for you to post it.')");
    assertEqual(await page.locator(".bugcard .bug-pill").innerText(), "Not posted yet · changed", "the changed badge");
    assertEqual((await calls(page, "bug_report_change"))[0].post, "idea", "Rich was not told it is an idea");
    await page.click("#set-btn");
    await page.waitForSelector("#idea-btn");
    assert(await page.isDisabled("#idea-btn") && await page.isDisabled("#bug-btn"), "a second press is possible while an idea is open");
    await page.keyboard.press("Escape");
    await page.click("#bug-cancel");
    await page.waitForSelector(".bugcard.is-canceled .bug-pill:has-text('Canceled · nothing posted')");
    assertEqual(await lastSaid(page), "Canceled. Nothing was posted.", "Rich's line");
    assert(!NEVER_AN_ISSUE.test(await ideaWords(page)), "a canceled idea says issue or bug: " + (await ideaWords(page)));
    await page.close();
    return "changed, both buttons disabled, canceled";
  });

  // ---- both themes: contrast, the type floor, indicators ----
  const ROOTS = "#bugdock:not([hidden]), #bug-subtip.is-shown";
  // Declared skippable, each the conversation's own 14px tier (style.css's Bust a bug note).
  const SKIPPABLE = ".tl-rich-meta, .bug-worked, .bugdock-send";
  const INDICATORS = [
    [".bugcard.is-sent", "borderTopColor"], [".bugcard.is-queued", "borderTopColor"], [".bug-pill", "borderTopColor"],
    [".bug-sub", "borderBottomColor"], [".bug-warn", "borderLeftColor"], [".bugcard.is-editing .bug-doc", "borderTopColor"],
    [".desk-btn--confirm", "backgroundColor"], [".desk-btn:not(.desk-btn--confirm)", "borderTopColor"], [".bugdock-form", "borderTopColor"],
  ];
  const STATES = [
    ["report", async (p) => { await bustABug(p); await answer(p, ANSWER); await p.hover(".bugcard .bug-sub >> nth=0"); await p.waitForSelector("#bug-subtip.is-shown"); }],
    ["editing", async (p) => { await bustABug(p); await answer(p, ANSWER); await p.click("#bug-change"); await p.keyboard.type(" Acme deal"); await p.waitForSelector(".bug-warn:not([hidden])"); }],
    ["sent", async (p) => { await bustABug(p); await answer(p, ANSWER); await p.click("#bug-send"); await p.waitForSelector(".bugcard.is-sent"); }],
    ["queued", async (p) => { await bustABug(p); await answer(p, ANSWER); await p.click("#bug-send"); await p.waitForSelector(".bugcard.is-queued"); }, { bugNet: "offline" }],
    ["changing", async (p) => { await bustABug(p); await answer(p, ANSWER); await p.fill("#bugdock-input", "Also say it happens in the light theme too"); await p.keyboard.press("Enter"); await p.waitForSelector(".bugcard .bug-pill:has-text('Changing it…')"); }, { bugChangeHold: true }],
    ["canceling", async (p) => { await bustABug(p); await answer(p, ANSWER); await p.click("#bug-send"); await p.waitForSelector(".bugcard.is-queued"); await p.click("#bug-cancel"); await p.waitForSelector(".bugcard .bug-pill:has-text('Canceling…')"); }, { bugNet: "offline", bugCancel: "hold" }],
    ["canceled", async (p) => { await bustABug(p); await answer(p, ANSWER); await p.click("#bug-cancel"); await p.waitForSelector(".bugcard.is-canceled"); }],
    ["unchecked", async (p) => { await bustABug(p); await p.fill("#bugdock-input", ANSWER); await p.keyboard.press("Enter"); await p.waitForSelector("#bug-unchecked-cancel"); }, { bugClaude: "down" }],
    ["checking-changes", async (p) => { await bustABug(p); await answer(p, ANSWER); await changeByHand(p, SAID_BY_HAND); await p.click("#bug-send"); await p.waitForSelector(".bugcard .bug-pill:has-text('Rich is checking your changes…')"); }, { bugSendCheckHold: true }],
    ["changes-unchecked", async (p) => { await bustABug(p); await answer(p, ANSWER); await p.evaluate(() => window.__RICHOS_MOCK_BUG__.setClaude("down")); await changeByHand(p, SAID_BY_HAND); await p.click("#bug-send"); await p.waitForSelector("#bug-unchecked-cancel"); }],
    ["panel", async (p) => { await p.click("#nav-corrections"); await bustABug(p); await p.fill("#bugdock-input", ANSWER); await p.keyboard.press("Enter"); await p.waitForSelector("#bugdock .bugcard .bug-pill:has-text('Not sent yet')"); }],
    // Request a feature (round 24): every state of an idea, and none of its words says issue or bug.
    ["idea-ask", async (p) => { await requestAFeature(p); await p.waitForSelector("#bug-never-mind"); }],
    ["idea-report", async (p) => { await requestAFeature(p); await answerIdea(p); await p.hover(".bugcard .bug-sub >> nth=0"); await p.waitForSelector("#bug-subtip.is-shown"); }, { ideaPictured: ["A message every Friday afternoon."] }],
    ["idea-editing", async (p) => { await requestAFeature(p); await answerIdea(p); await p.click("#bug-change"); await p.keyboard.type(" Acme deal"); await p.waitForSelector(".bug-warn:not([hidden])"); }],
    ["idea-sent", async (p) => { await requestAFeature(p); await answerIdea(p); await p.click("#bug-send"); await p.waitForSelector(".bugcard.is-sent"); }],
    ["idea-queued", async (p) => { await requestAFeature(p); await answerIdea(p); await p.click("#bug-send"); await p.waitForSelector(".bugcard.is-queued"); }, { bugNet: "offline" }],
    ["idea-canceled", async (p) => { await requestAFeature(p); await answerIdea(p); await p.click("#bug-cancel"); await p.waitForSelector(".bugcard.is-canceled"); }],
    ["idea-unchecked", async (p) => { await requestAFeature(p); await p.fill("#bugdock-input", IDEA); await p.keyboard.press("Enter"); await p.waitForSelector("#bug-unchecked-cancel"); }, { bugClaude: "down" }],
    ["idea-panel", async (p) => { await p.click("#nav-corrections"); await requestAFeature(p); await answerIdea(p, "I'd like to undo a correction after I've accepted it."); }],
  ];
  INDICATORS.push([".idea-cat", "borderTopColor"], [".bugcard.is-idea .bug-glyph", "color"]);
  for (const theme of ["dark", "light"]) {
    await run.check(theme + ": every Bust a bug state meets AA contrast, the 16px floor and 3:1 for indicators", async () => {
      const measured = [];
      for (const [name, drive, preset] of STATES) {
        const page = await open(theme, preset);
        await drive(page);
        await settle(page);
        await shot(page, name + "-" + theme);
        await page.addScriptTag({ content: contrast.pageScript() });
        const result = await page.evaluate(({ ROOTS, SKIPPABLE, INDICATORS }) => {
          const C = window.__contrastMath, failures = [];
          let worst = 99, worstIndicator = 99, nodes = 0;
          const ground = (el) => {
            let bg = { r: 0, g: 0, b: 0, a: 0 };
            const chain = [];
            for (let p = el; p; p = p.parentElement) chain.unshift(p);
            for (const p of chain) bg = C.compositeOver(C.parseCssColor(getComputedStyle(p).backgroundColor), bg);
            return bg;
          };
          const ratioOn = (paint, bg) => { const c = C.parseCssColor(paint); return c ? C.round2(C.contrastRatio(C.compositeOver(c, bg), bg)) : 0; };
          for (const root of document.querySelectorAll(ROOTS)) {
            if (!root.getClientRects().length) continue;
            for (const e of [root, ...root.querySelectorAll("*")]) {
              if (!e.getClientRects().length || e.closest("[hidden]") || e.closest(".sr-only")) continue;
              const own = [...e.childNodes].some((n) => n.nodeType === Node.TEXT_NODE && n.textContent.trim());
              if (!own) continue;
              const style = getComputedStyle(e);
              if (style.visibility === "hidden" || style.display === "none") continue;
              const bg = ground(e), fg = C.compositeOver(C.parseCssColor(style.color), bg);
              const size = parseFloat(style.fontSize), large = size >= 24 || (size >= 18.66 && parseInt(style.fontWeight) >= 700);
              const ratio = C.round2(C.contrastRatio(fg, bg));
              nodes++;
              worst = Math.min(worst, ratio);
              if (ratio < (large ? 3 : 4.5)) failures.push({ text: e.textContent.trim().slice(0, 50), ratio });
              if (size < 16 && !e.closest(SKIPPABLE)) failures.push({ text: e.textContent.trim().slice(0, 50), size });
            }
            for (const [sel, prop] of INDICATORS) {
              for (const e of root.querySelectorAll(sel)) {
                if (!e.getClientRects().length || e.closest("[hidden]")) continue;
                const ratio = ratioOn(getComputedStyle(e)[prop], ground(e.parentElement));
                worstIndicator = Math.min(worstIndicator, ratio);
                if (ratio < 3) failures.push({ indicator: sel, ratio });
              }
            }
          }
          return { failures, worst, worstIndicator, nodes };
        }, { ROOTS, SKIPPABLE, INDICATORS });
        assertEqual(result.failures, [], `${theme} ${name}: contrast and type`);
        if (name.startsWith("idea-")) {
          const words = (await ideaWords(page)) + "\n" + (await page.evaluate(() => (document.getElementById("bug-subtip") || {}).innerText || ""));
          assert(!NEVER_AN_ISSUE.test(words), `${theme} ${name}: a feature request says issue or bug: ${words}`);
        }
        // The unchecked state has no card: the panel's title, Rich's question, the user's words, his
        // line and Cancel report (7 text nodes measured), so its floor is that, not a card's.
        // An idea Rich could not check has no card either, nor does his question about an idea
        // (the title, his question, Never mind).
        const floor = ["unchecked", "idea-unchecked", "idea-ask"].includes(name) ? 6 : 10;
        assert(result.nodes > floor, `${theme} ${name}: EMPTY INVENTORY (${result.nodes} nodes)`);
        measured.push(`${name} ${result.worst}:1 text / ${result.worstIndicator}:1 indicator over ${result.nodes} nodes`);
        await page.close();
      }
      return measured.join("; ");
    });
  }

  await run.check("Escape closes the Bust a bug panel and the Request a feature panel like the x does, with the field focused or not (CEO 2026-10-19)", async () => {
    const seen = [];
    for (const [name, opener] of [["bug", bustABug], ["idea", requestAFeature]]) {
      for (const focused of [true, false]) {
        const page = await open("dark");
        await opener(page);
        await page.waitForSelector("#bugdock:not([hidden]) #bugdock-msgs .bug-rich");
        if (focused) {
          await page.waitForFunction(() => document.activeElement === document.getElementById("bugdock-input"));
        } else {
          await page.evaluate(() => document.activeElement && document.activeElement.blur());
          assert(await page.evaluate(() => document.activeElement === document.body), "focus did not leave the panel");
        }
        await page.keyboard.press("Escape");
        await page.waitForSelector("#bugdock", { state: "hidden" }); // times out (red) if Escape left the panel open
        assert(await page.isHidden("#bugdock"), `${name}, field ${focused ? "focused" : "unfocused"}: Escape left the panel open`);
        assert(!(await page.evaluate(() => document.body.classList.contains("bug-dock-open"))), `${name}: the panel's open mark stayed`);
        seen.push(`${name}/${focused ? "focused" : "unfocused"}`);
        await page.close();
      }
    }
    return seen.join(", ") + " all closed";
  });

  await run.check("no renderer errors on any page", async () => {
    assertEqual(errors, [], "page errors");
    return "0 page errors";
  });

  await browser.close();
  process.exitCode = run.report() ? 1 : 0;
}

main().catch((e) => {
  console.error(e);
  process.exit(1);
});

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
//   1. Bust a bug starts the exchange with Rich in the conversation on screen; no notice.
//   2. The draft is shown before anything is sent: nothing reaches `bug_report_send` until Send.
//   3. Private details are left out: the card shows stand-ins, each saying what it replaced only
//      to the user, and the sheet that is sent carries the stand-ins and never the names.
//   4. Send: the card says Sent with the issue number, from the RichOS reporting account.
//   5. A failed send is kept and resent: offline, the report waits on this Mac and goes out by
//      itself when the connection is back, and Rich and a notice say so.
//   6. Change it (in place, with the privacy heads-up, and by telling Rich), Cancel, and one
//      report at a time.
//   7. Where a window covers the conversation, the exchange happens in the Rich panel beside it.
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
//  14. The review of 665df1bb3 (rv-20261009T223503Z-665df1bb-17bf), each red there: Bust a bug
//      pressed again over Corrections shows the report already in the conversation, on top and
//      answerable; and the heads-up reads a line break typed in a change the way Send does, so a
//      name on two lines is named. `mock.js` matches a name across any whitespace, as the core does.
//  15. The review of b3b5c573d (rv-20261009T225657Z-b3b5c573-96c0), each red there: what Send reads
//      is what the card shows when a step holds a nested list, and a rich-text paste (a nested
//      list, a table, styled words) comes in as plain text. `mock.js` writes steps from
//      `preset.bugSteps`.
"use strict";

const path = require("path");
const fs = require("fs");
const { loadPlaywright, createRun, assert, assertEqual, UI_DIR, leaveHome, bootSettled, shellSettled, openThread, HOLD_CURTAIN } = require("./lib/harness");
const contrast = require("./lib/contrast");

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
    await page.fill("#input", text);
    await page.keyboard.press("Enter");
    await page.waitForSelector(".bugcard .bug-pill:has-text('Not sent yet')");
  }
  /// Rich's last words in the exchange: his line(s), without the speaker label.
  const lastSaid = (page, scope) =>
    page.locator((scope || "#bug-flows") + " .bug-rich").last().locator(".tl-prose").allInnerTexts().then((t) => t.join("\n"));
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

  await run.check("Bust a bug starts the exchange with Rich in the conversation on screen, not a notice", async () => {
    const page = await open("dark");
    const before = await page.locator("#messages .tl-user").count();
    await bustABug(page);
    await page.waitForSelector("#bug-flows .bugflow .bug-divider");
    assert(await page.isHidden("#set-menu"), "Settings stayed open");
    assertEqual(await page.locator("#bug-toast:visible").count(), 0, "the old notice is still what the button does");
    const asked = await page.locator("#bug-flows .bug-rich .tl-prose").first().innerText();
    assert(asked.startsWith("What went wrong? Tell me in your own words, typed or out loud"), asked);
    assert(asked.includes("I've noted that you were on the Acme deal conversation."), asked);
    assertEqual(await page.getAttribute("#input", "placeholder"), "Tell Rich what went wrong…", "the composer does not say what it is waiting for");
    assert(await page.isVisible("#bug-never-mind"), "no way out before answering");
    await shot(page, "ask-dark");
    // Never mind ends it with nothing written up, and the composer is Rich's again.
    await page.click("#bug-never-mind");
    await page.waitForSelector("#bug-flows .bug-rich:has-text('No problem. Nothing was written up or sent.')");
    assertEqual(await page.getAttribute("#input", "placeholder"), "Talk to Rich…", "the composer still belongs to the report");
    assertEqual(await page.locator("#messages .tl-user").count(), before, "a message went to Rich's conversation");
    await page.close();
    return asked;
  });

  await run.check("the whole report is shown before anything is sent, and private details are left out", async () => {
    const page = await open("dark");
    const before = await page.locator("#messages .tl-user").count();
    await bustABug(page);
    await page.fill("#input", ANSWER);
    await page.keyboard.press("Enter");
    await page.waitForSelector(".bug-working:has-text('Rich is checking…')");
    await page.waitForSelector(".bugcard .bug-pill:has-text('Not sent yet')");
    // The words were the report's: in the exchange, and not a message to Rich's conversation.
    assertEqual(await page.locator("#messages .tl-user").count(), before, "the answer went to Rich's conversation");
    assert((await page.locator("#bug-flows .bug-user").innerText()).includes("Acme deal chat"), "the answer is not shown as the user's");
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
    assertEqual(tip, "Stands in for a person's name that Rich left out when he wrote the report.", "the stand-in's tip");
    await page.mouse.move(5, 5);
    await page.focus(".bugcard .bug-sub >> nth=3");
    await page.waitForSelector("#bug-subtip.is-shown");
    assertEqual(await page.locator("#bug-subtip").innerText(), "Stands in for a file path that Rich left out when he wrote the report.", "the tip on Tab");
    await page.close();
    return `${subs.length} marked; line: ${line}; tip: ${tip}`;
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
    // The link opens the issue by its number, never by an address the page passes.
    await page.click(".bug-sentrow .bug-link");
    assertEqual((await calls(page, "bug_report_open_issue"))[0].number, 412, "Open issue");
    assertEqual(await page.getAttribute("#input", "placeholder"), "Talk to Rich…", "the composer still belongs to the report");
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
    await page.fill("#input", "Also say it happens in the light theme too");
    await page.keyboard.press("Enter");
    await page.waitForSelector(".bug-sec p.is-added");
    assertEqual(await page.locator(".bug-sec p.is-added").innerText(), "It happens in the light theme too.", "the change");
    await page.waitForSelector("#bug-flows .bug-rich:has-text('Added that to What happened, above. It still waits for you to send it.')");
    await shot(page, "changed-dark");
    // One report at a time: Bust a bug again goes back to this one.
    await bustABug(page);
    assertEqual(await page.locator("#bug-flows .bugflow").count(), 1, "a second report was started");
    await page.click("#bug-cancel");
    await page.waitForSelector(".bugcard.is-canceled .bug-pill:has-text('Canceled · nothing sent')");
    assert((await page.locator(".bug-ctitle").innerText()).includes("for Acme deal"), "the folded card does not show its title");
    await page.waitForSelector("#bug-flows .bug-rich:has-text('Canceled. Nothing was sent.')");
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
  const prose = (page) => page.locator("#bug-flows .bug-rich .tl-prose").allInnerTexts().then((t) => t.join("\n"));

  await run.check("a private name Rich found stays left out through every change, and the heads-up names it", async () => {
    // Finding 4: on the tip Rich's own private words applied to the first draft only.
    const page = await open("dark", { bugRichPrivate: [{ text: "Jane Doe", kind: "person_name" }] });
    await bustABug(page);
    await answer(page, "The names on the left get cut off. Jane Doe saw it first.");
    assert(!(await page.locator(".bugcard .bug-doc").innerText()).includes("Jane Doe"), "Rich's private name is on the first draft");
    await page.fill("#input", "Also say Jane Doe saw it in light mode too");
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
    await page.waitForSelector("#bug-flows .bug-rich:has-text('I checked it again before sending')");
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

  await run.check("leaving the report's conversation gives the voice back to Rich, and coming back gives it to the report", async () => {
    // Finding 3, fixture `voice-routing.js`: on the tip the shell's hold (`bug_report_voice`)
    // stayed on after another conversation opened, so the shell dropped every spoken turn there
    // while the window drew it as sent. The hold now follows the same conditions as the report's
    // own taking of a transcript, on every navigation.
    const page = await open("dark", { bugVoice: true });
    const holding = (on) => page.waitForFunction((want) => {
      const v = window.__RICHOS_MOCK_BUG__.calls.filter((c) => c.cmd === "bug_report_voice");
      return v.length > 0 && v[v.length - 1].on === want;
    }, on);
    const said = (text) => page.evaluate((t) => window.__RICHOS_MOCK_BUG__.say(t), text);
    await bustABug(page);
    await page.waitForSelector("#bug-flows .bugflow .bug-divider");
    await page.click("#talk-toggle");
    await holding(true);
    // Said in the report's conversation: the report's words, not a message to Rich.
    const before = await page.locator("#messages .tl-user").count();
    await said("The names on the left get cut off when the text is bigger.");
    await page.waitForSelector(".bugcard .bug-pill:has-text('Not sent yet')");
    assertEqual(await page.locator("#messages .tl-user").count(), before, "the report's answer went to Rich's conversation");
    await holding(true);
    // Another conversation: the shell stops holding, so what is said there is a turn for Rich.
    await openThread(page, "hiring");
    await holding(false);
    await said("Is the hiring plan ready?");
    await page.waitForSelector("#messages .tl-user:has-text('Is the hiring plan ready?')");
    assertEqual(await page.locator("#bug-flows .bug-user").count(), 1, "the report took words said in another conversation");
    // Back to the report's conversation: the report takes what is said again.
    await openThread(page, "acme");
    await holding(true);
    await said("Also say it happens in the light theme too");
    await page.waitForSelector(".bug-sec p.is-added");
    assertEqual(await page.locator(".bug-sec p.is-added").innerText(), "It happens in the light theme too.", "the spoken change");
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
    await page.waitForSelector("#bug-flows .bug-rich:has-text('I checked it again before sending')");
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
    await page.fill("#input", SAID);
    await page.keyboard.press("Enter");
    await page.waitForSelector(`#bug-flows .bug-rich:has-text("${NOT_CHECKED}")`);
    const said = await lastSaid(page);
    assertEqual(await page.locator(".bugcard").count(), 0, "a report Rich did not check was offered");
    for (const id of ["#bug-send", "#bug-change"]) assertEqual(await page.locator(id).count(), 0, id + " is offered");
    assert(!(await prose(page)).includes("I left out names"), "Rich says he left names out of a report he never checked");
    assertEqual(await kept(page), 1, "the report is not kept on this Mac");
    assertEqual((await calls(page, "bug_report_send")).length, 0, "something was sent");
    assert(await page.isVisible("#bug-unchecked-cancel"), "no way to cancel the report that waits");
    assertEqual(await page.getAttribute("#input", "placeholder"), "Talk to Rich…", "the composer still waits for the report");
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
    await page.fill("#input", "Also say Marta saw it too");
    await page.keyboard.press("Enter");
    await page.waitForSelector("#bug-flows .bug-rich:has-text(\"I couldn't change the report just now\")");
    assertEqual(await page.locator(".bug-sec p.is-added").count(), 0, "a change Rich did not check was added");
    await page.close();
    return said + " / then: " + checkedLine.split(".")[0];
  });

  await run.check("a report Rich could not check can be canceled, and one kept from before is shown once he checks it", async () => {
    const page = await open("dark", { bugClaude: "down" });
    await bustABug(page);
    await page.fill("#input", "The names on the left get cut off.");
    await page.keyboard.press("Enter");
    await page.waitForSelector("#bug-unchecked-cancel");
    await page.click("#bug-unchecked-cancel");
    await page.waitForSelector("#bug-flows .bug-rich:has-text(\"Canceled. Nothing was sent, and it's no longer saved on this Mac.\")");
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
    await page.fill("#input", "Also say it happens in the light theme too");
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
    await page.waitForSelector("#bug-flows .bug-rich:has-text(\"I couldn't cancel the report\")");
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
    await page.waitForSelector("#bug-flows .bug-rich:has-text('It had already gone out')");
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
    await page.waitForSelector("#bug-flows .bug-rich:has-text(\"so it can't be changed\")");
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

  await run.check("where a window covers the conversation, the exchange happens in the Rich panel beside it", async () => {
    const page = await open("dark");
    await page.click("#nav-corrections");
    await page.waitForSelector("#corrections-overlay:not([hidden])");
    await bustABug(page);
    await page.waitForSelector("#bugdock:not([hidden])");
    assertEqual(await page.locator("#bugdock-where").innerText(), "· Corrections", "panel header");
    assert(await page.isVisible("#corrections-overlay"), "the window did not stay open");
    assertEqual(await page.locator("#bug-flows .bugflow").count(), 0, "it also started in the conversation");
    const asked = await page.locator("#bugdock-msgs .bug-rich .tl-prose").first().innerText();
    assert(asked.includes("I've noted that you were in Corrections, and it stays open beside us."), asked);
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
      await page.waitForSelector("#bug-flows .bug-rich, #bugdock-msgs .bug-rich", { state: "attached" });
      // The report's own entrance only: a HELD opening screen's animations are paused, and their
      // `finished` never comes, so the suite-wide `settle` would wait on it forever.
      await page.evaluate(() => Promise.all(document.getAnimations()
        .filter((a) => a.effect && a.effect.target && a.effect.target.closest && a.effect.target.closest("#bugdock, #bug-flows"))
        .map((a) => a.finished.catch(() => null))));
      const top = await page.evaluate(() => {
        const q = document.querySelector("#bugdock-msgs .bug-rich .tl-prose, #bug-flows .bug-rich .tl-prose");
        const input = q.closest("#bugdock") ? document.getElementById("bugdock-input") : document.getElementById("input");
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

  // ---- the review of 665df1bb3 (rv-20261009T223503Z-665df1bb-17bf) ----
  await run.check("Bust a bug pressed again over Corrections shows the report already open in the conversation, on top and answerable", async () => {
    // Finding 1, fixture `reopen-report.js`: on 665df1bb3 the report already in the conversation
    // was focused UNDER Corrections (a full-window overlay at z-index 60), which stayed open, and
    // the user pressed Bust a bug and saw Corrections.
    const page = await open("dark");
    await bustABug(page);
    await page.waitForSelector("#bug-flows .bug-rich .tl-prose");
    await page.click("#nav-corrections");
    await page.waitForSelector("#corrections-overlay:not([hidden])");
    await bustABug(page);
    await page.waitForFunction(() => document.activeElement === document.getElementById("input"));
    await page.evaluate(() => new Promise((r) => requestAnimationFrame(() => requestAnimationFrame(r))));
    const top = await page.evaluate(() => {
      const q = document.querySelector("#bug-flows .bug-rich .tl-prose");
      const input = document.getElementById("input");
      const hitAt = (el) => {
        const r = el.getBoundingClientRect();
        const h = document.elementFromPoint(r.left + r.width / 2, r.top + r.height / 2);
        return h === el || el.contains(h) ? "ON-TOP" : "COVERED by " + (h ? h.tagName.toLowerCase() + (h.id ? "#" + h.id : h.className ? "." + String(h.className).split(" ")[0] : "") : "nothing");
      };
      return { asked: q.textContent.trim(), question: hitAt(q), input: hitAt(input) };
    });
    assert(top.asked.startsWith("What went wrong?"), "not the report's question: " + JSON.stringify(top.asked));
    assertEqual(top.question, "ON-TOP", "Rich's question is not what is painted where it is");
    assertEqual(top.input, "ON-TOP", "the box the answer goes in is not what is painted where it is");
    assertEqual(await page.locator("#bug-flows .bugflow").count(), 1, "a second report was started");
    assert(await page.isHidden("#bugdock"), "the report moved into the panel");
    // Answered the way a person does: a click into the box, then the keys.
    const said = "The names on the left get cut off when I make the text bigger.";
    await page.click("#input");
    await page.keyboard.type(said);
    await page.keyboard.press("Enter");
    await page.waitForSelector("#bug-flows .bug-user:has-text('" + said + "')");
    await page.waitForSelector("#bug-flows .bugcard .bug-pill:has-text('Not sent yet')");
    await page.close();
    return `question ${top.question}, answer box ${top.input}; answered, card written`;
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
    await page.waitForSelector("#bug-flows .bug-rich:has-text('I checked it again before sending')");
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
    await down.waitForSelector("#bug-flows .bug-rich:has-text(\"I couldn't check your changes for private details yet\")");
    await down.waitForSelector(".bugcard .bug-pill:has-text(\"Waiting for Rich's check · saved on this Mac\")");
    const said = await lastSaid(down);
    assertEqual(await down.locator("#bug-send").count(), 0, "Send is offered for words Rich hasn't checked");
    assert(await down.isVisible("#bug-unchecked-cancel"), "no way to cancel the words that wait");
    const kept = await down.evaluate(() => window.__RICHOS_MOCK_BUG__.unchecked());
    assert(kept.length === 1 && JSON.stringify(kept[0].edited).includes(SAID_BY_HAND), "the changed words are not kept: " + JSON.stringify(kept));
    assertEqual((await filed(down)).length, 0, "something went out");
    await down.evaluate(() => window.__RICHOS_MOCK_BUG__.claudeAnswers());
    await down.waitForSelector("#bug-flows .bug-rich:has-text(\"I've checked your changes now.\")");
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

  // ---- both themes: contrast, the type floor, indicators ----
  const ROOTS = "#bug-flows, #bugdock:not([hidden]), #bug-subtip.is-shown";
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
    ["changing", async (p) => { await bustABug(p); await answer(p, ANSWER); await p.fill("#input", "Also say it happens in the light theme too"); await p.keyboard.press("Enter"); await p.waitForSelector(".bugcard .bug-pill:has-text('Changing it…')"); }, { bugChangeHold: true }],
    ["canceling", async (p) => { await bustABug(p); await answer(p, ANSWER); await p.click("#bug-send"); await p.waitForSelector(".bugcard.is-queued"); await p.click("#bug-cancel"); await p.waitForSelector(".bugcard .bug-pill:has-text('Canceling…')"); }, { bugNet: "offline", bugCancel: "hold" }],
    ["canceled", async (p) => { await bustABug(p); await answer(p, ANSWER); await p.click("#bug-cancel"); await p.waitForSelector(".bugcard.is-canceled"); }],
    ["unchecked", async (p) => { await bustABug(p); await p.fill("#input", ANSWER); await p.keyboard.press("Enter"); await p.waitForSelector("#bug-unchecked-cancel"); }, { bugClaude: "down" }],
    ["checking-changes", async (p) => { await bustABug(p); await answer(p, ANSWER); await changeByHand(p, SAID_BY_HAND); await p.click("#bug-send"); await p.waitForSelector(".bugcard .bug-pill:has-text('Rich is checking your changes…')"); }, { bugSendCheckHold: true }],
    ["changes-unchecked", async (p) => { await bustABug(p); await answer(p, ANSWER); await p.evaluate(() => window.__RICHOS_MOCK_BUG__.setClaude("down")); await changeByHand(p, SAID_BY_HAND); await p.click("#bug-send"); await p.waitForSelector("#bug-unchecked-cancel"); }],
    ["panel", async (p) => { await p.click("#nav-corrections"); await bustABug(p); await p.fill("#bugdock-input", ANSWER); await p.keyboard.press("Enter"); await p.waitForSelector("#bugdock .bugcard .bug-pill:has-text('Not sent yet')"); }],
  ];
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
        // The unchecked state has no card: the divider, Rich's question, the user's words, his
        // line and Cancel report (7 text nodes measured), so its floor is that, not a card's.
        const floor = name === "unchecked" ? 6 : 10;
        assert(result.nodes > floor, `${theme} ${name}: EMPTY INVENTORY (${result.nodes} nodes)`);
        measured.push(`${name} ${result.worst}:1 text / ${result.worstIndicator}:1 indicator over ${result.nodes} nodes`);
        await page.close();
      }
      return measured.join("; ");
    });
  }

  await run.check("no renderer errors on any page", async () => {
    assertEqual(errors, [], "page errors");
    return "0 page errors";
  });

  await browser.close();
  process.exit(run.report() ? 1 : 0);
}

main().catch((e) => {
  console.error(e);
  process.exit(1);
});

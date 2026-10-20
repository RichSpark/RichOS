// THE QUIT QUESTION, against the REAL sheet under WebKit.
//
// The background-work spec (richos-hq `docs/plans/background-work-spec-2026-09-17.md`
// revision 5) §2.5, §2.5a and acceptance §7.4a: *"He quits while work is running, and he is
// told before it dies … with an assignment running, choose Quit; the choice appears; cancel
// it and the work is still running; choose it again and quit."*
//
// THREE THINGS THIS SUITE PINS.
//
//   1. **The choice APPEARS, and the safe answer is the one under the return key.** A sheet
//      that opened with "quit and stop the work" focused would answer itself.
//   2. **Canceling changes nothing.** §2.5a's whole shape is prevent-first, ask-second, so
//      "Keep working" must reach `cancel_quit` and nothing else — never a stop, never a
//      second exit.
//   3. **Contrast, computed in BOTH themes**, off WebKit's own resolved colors, composited
//      where the palette uses alpha. Nothing on this sheet is declared exempt: it is a
//      question he is being asked, which is the definition of text meant to be read.
//
// The sheet renders from `ui/quit-question.js` as shipped — not a copy of it — with the
// shipping stylesheet attached from disk, and both are proven present before any ratio is
// taken.

"use strict";

const path = require("path");
const { loadPlaywright, createRun, assert, assertEqual, UI_DIR } = require("./lib/harness");

function body(theme) {
  return `<!DOCTYPE html>
<html lang="en" data-theme="${theme}"><head><meta charset="utf-8"></head><body></body></html>`;
}

/// A bridge that records rather than invokes. The shell half is Rust and is tested there
/// (`src-tauri/src/lifecycle.rs`); what is driven here is the sheet.
function bridge() {
  return `
    window.__invoked = [];
    window.RichBridge = {
      invoke: (name, args) => { window.__invoked.push({name, args}); return Promise.resolve(true); },
      listen: (name, cb) => { window.__listening = name; window.__fire = cb; },
    };
  `;
}

function ratioHelper() {
  return `
    window.__ratio = (selector) => {
      const node = document.querySelector(selector);
      if (!node) throw new Error("no node for " + selector);
      const parse = (value) => {
        const n = value.match(/[\\d.]+/g).map(Number);
        return { r: n[0], g: n[1], b: n[2], a: n.length > 3 ? n[3] : 1 };
      };
      let background = null;
      for (let el = node; el; el = el.parentElement) {
        const c = parse(getComputedStyle(el).backgroundColor);
        if (c.a === 1) { background = c; break; }
      }
      if (!background) background = { r: 255, g: 255, b: 255, a: 1 };
      const fg = parse(getComputedStyle(node).color);
      const flat = {
        r: fg.r * fg.a + background.r * (1 - fg.a),
        g: fg.g * fg.a + background.g * (1 - fg.a),
        b: fg.b * fg.a + background.b * (1 - fg.a),
      };
      const channel = (v) => { v /= 255; return v <= 0.03928 ? v / 12.92 : Math.pow((v + 0.055) / 1.055, 2.4); };
      const lum = (c) => 0.2126 * channel(c.r) + 0.7152 * channel(c.g) + 0.0722 * channel(c.b);
      const a = lum(flat), b = lum(background);
      return (Math.max(a, b) + 0.05) / (Math.min(a, b) + 0.05);
    };
    window.__fontPx = (selector) =>
      parseFloat(getComputedStyle(document.querySelector(selector)).fontSize);
  `;
}

/// The shell's own sentence for one assignment running, as `lifecycle.rs`'s `quit_question`
/// builds it. The WORDING is unit-tested in Rust, where it is built; what this suite asserts
/// is what the sheet does with it.
const SAID =
  "You have 1 assignment still running in the background. Quitting stops the work. " +
  "Everything it has done so far is kept, and nothing is landed in your repository.";

async function openPage(browser, theme) {
  const page = await browser.newPage({ viewport: { width: 520, height: 700 } });
  await page.setContent(body(theme));
  await page.addStyleTag({ path: path.join(UI_DIR, "style.css") });
  await page.addScriptTag({ content: bridge() });
  await page.addScriptTag({ path: path.join(UI_DIR, "quit-question.js") });
  await page.addScriptTag({ content: ratioHelper() });
  const wired = await page.evaluate(() => ({
    sheet: typeof window.RichQuitQuestion?.show === "function",
    listening: window.__listening,
    styled: getComputedStyle(document.documentElement).getPropertyValue("--card").trim(),
  }));
  assert(wired.sheet, "the shipping quit sheet did not load");
  assertEqual(wired.listening, "rich://quit-question", "the sheet does not listen for the shell's event");
  assert(wired.styled.length > 0, "the shipping stylesheet did not load; contrast here would be the browser's");
  return page;
}

async function main() {
  const run = createRun("the quit question: it appears, it is safe by default, and it stops nothing when refused");
  const browser = await loadPlaywright().webkit.launch();

  await run.check("the shell's event puts the question up with the safe answer focused", async () => {
    const page = await openPage(browser, "dark");
    const errors = [];
    page.on("pageerror", (e) => errors.push(String(e)));
    // Hidden until asked — an app with nothing running never shows this at all.
    assertEqual(await page.locator("#quit-question").isVisible(), false, "the sheet was up before anything asked");
    await page.evaluate((said) => window.__fire({ payload: {
      say: said, quit: "Quit and stop the work", stay: "Keep working" } }), SAID);
    assertEqual(await page.locator("#quit-question").isVisible(), true, "the event did not raise the sheet");
    const text = await page.locator("#quit-question").innerText();
    assert(text.includes("still running in the background"), text);
    assert(text.includes("Everything it has done so far is kept"), text);
    assert(text.includes("nothing is landed"), text);
    // It must never claim the work is finished, or that quitting throws it away. The scan
    // is on PHRASES rather than words, and deliberately: "everything it has done so far is
    // kept" contains "done" and is the sentence that makes quitting safe to understand. A
    // bare-substring scan would have failed the very wording it exists to protect.
    for (const claim of ["is done", "all done", "finished", "will be lost", "you will lose"]) {
      assert(!text.toLowerCase().includes(claim), `the question said "${claim}": ${text}`);
    }
    // POSITIVE CONTROL for that scan: it fires on a sentence that DOES make the claim, so
    // the clean result above is a fact about the wording rather than about a scan that can
    // never match.
    const wrong = "your work is done and will be lost".toLowerCase();
    assert(
      ["is done", "will be lost"].every((claim) => wrong.includes(claim)),
      "the completion-claim scan cannot fire at all; the check above proves nothing"
    );
    // THE SAFE ANSWER HAS FOCUS.
    assertEqual(
      await page.evaluate(() => document.activeElement.id),
      "quit-question-stay",
      "the destructive answer was focused"
    );
    assertEqual(errors.length, 0, "sheet errors");
    await page.close();
    return "raised by the shell's event, worded as the shell worded it, with Keep working focused";
  });

  await run.check("keep working reaches cancel_quit and nothing else; quitting reaches the confirm", async () => {
    const page = await openPage(browser, "dark");
    await page.evaluate((said) => window.__fire({ payload: {
      say: said, quit: "Quit and stop the work", stay: "Keep working" } }), SAID);
    await page.locator("#quit-question-stay").click();
    assertEqual(
      JSON.stringify(await page.evaluate(() => window.__invoked.map((i) => i.name))),
      JSON.stringify(["cancel_quit"]),
      "keeping the work running did something other than that"
    );
    // And the sheet goes away, so he is not left answering the same question twice.
    assertEqual(await page.locator("#quit-question").isVisible(), false, "the sheet stayed up after he answered");

    // POSITIVE CONTROL: the other control DOES reach the confirming command, so the check
    // above is a fact about which button was pressed rather than about a page that can
    // never invoke anything.
    await page.evaluate((said) => window.__fire({ payload: {
      say: said, quit: "Quit and stop the work", stay: "Keep working" } }), SAID);
    await page.locator("#quit-question-quit").click();
    assertEqual(
      JSON.stringify(await page.evaluate(() => window.__invoked.map((i) => i.name))),
      JSON.stringify(["cancel_quit", "confirm_quit_and_stop"]),
      "the quit control did not reach the confirming command"
    );
    await page.close();
    return "Keep working invokes cancel_quit only; Quit and stop the work invokes the confirm";
  });

  await run.check("escape answers the safe way rather than hiding the question", async () => {
    // A question with a way out that answers nothing is worse than no question: the shell
    // has already prevented the exit, and a sheet dismissed without an answer would leave
    // him pressing Quit against an app that never goes away.
    const page = await openPage(browser, "dark");
    await page.evaluate((said) => window.__fire({ payload: {
      say: said, quit: "Quit and stop the work", stay: "Keep working" } }), SAID);
    await page.keyboard.press("Escape");
    assertEqual(
      JSON.stringify(await page.evaluate(() => window.__invoked.map((i) => i.name))),
      JSON.stringify(["cancel_quit"]),
      "escape dismissed the question without answering it"
    );
    await page.close();
    return "escape is Keep working, answered rather than dismissed";
  });

  for (const theme of ["dark", "light"]) {
    await run.check(`every word of the quit question clears WCAG AA in ${theme} mode`, async () => {
      const page = await openPage(browser, theme);
      await page.evaluate((said) => window.__fire({ payload: {
        say: said, quit: "Quit and stop the work", stay: "Keep working" } }), SAID);
      const measured = [];
      for (const [what, selector] of [
        ["title", "#quit-question-title"],
        ["the question", "#quit-question-say"],
        ["keep working", "#quit-question-stay"],
        ["quit and stop the work", "#quit-question-quit"],
      ]) {
        const ratio = await page.evaluate((s) => window.__ratio(s), selector);
        const px = await page.evaluate((s) => window.__fontPx(s), selector);
        measured.push(`${what} ${ratio.toFixed(2)}:1 at ${px}px`);
        assert(ratio >= 4.5, `${theme}: ${what} is ${ratio.toFixed(2)}:1, under the 4.5:1 floor`);
        // `ceo-decisions.md` §15: 16px is the floor for text meant to be easily read.
        assert(px >= 16, `${theme}: ${what} is ${px}px, under the 16px readable floor`);
      }
      await page.close();
      return measured.join("; ");
    });
  }

  await browser.close();
  process.exitCode = run.report() ? 1 : 0;
}

main().catch((e) => {
  console.error(e);
  process.exit(1);
});

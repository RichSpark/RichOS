// **A SWITCHED-OFF COMPOSER LOOKS SWITCHED OFF** — candidate-.2 defect #6.
//
// `docs/verification/2026-09-17-nightly-1.2.0-20260917.2-onscreen-audit-2.md` §4:
//
//   *"#6 — the dead composer looks alive. Ran it on screen. Defer the company sheet, then
//    click the message box and type. The box reads 'Talk to Rich…', shows no disabled
//    styling, and silently discards every keystroke. The sheet did warn that the box would be
//    'switched off', but the box itself never says so. Expected: a switched-off input looks
//    switched off and says why. Control: after adding a company, typing in the same box works
//    immediately."*
//
// ## WHAT WAS ACTUALLY MEASURED, WHICH IS NOT QUITE WHAT THE WALK DESCRIBES
//
// Driven here through the real shell before the fix, the state was:
//
//     disabled     false          the box accepted focus and accepted letters
//     placeholder  "Talk to Rich…"
//     opacity      1
//     cursor       auto
//     send         enabled
//     typing "hello" -> value "hello", then Enter -> value still "hello", 0 messages sent
//
// So the keystrokes were not discarded on the way in; they were accepted, held, and then
// refused on the way out with no change to anything the box looked like. From where a person
// sits that is the same event — a box that took your sentence and did nothing with it — and
// the fix is the same fix. The distinction is recorded because a suite that asserted
// "keystrokes never arrive" would have been asserting something that was never true.
//
// ## THE RULE WAS ALREADY WRITTEN DOWN IN THIS PRODUCT
//
// `main.js`'s `showUnboundView`, for the OTHER blocked state: *"The placeholder is part of the
// block: an inviting 'Talk to Rich…' above a dead field is the composer telling a small lie
// about what it will do."* The company block never picked it up. This suite holds both states
// to it, and holds the way back — a company chosen, the box alive again — to the same bar.
//
// Run: node composer-off.js   (or `npm test` for every suite in this directory)

"use strict";

const path = require("path");
const { loadPlaywright, leaveHome, createRun, assert, assertEqual, UI_DIR } = require("./lib/harness");
const C = require("./lib/contrast");

const APP = "file://" + path.join(UI_DIR, "index.html");

/// The inviting placeholder. If this suite ever reads it while send is blocked, the box is
/// lying, whatever else is right.
const LIVE_PLACEHOLDER = "Talk to Rich…";

async function openApp(browser, preset) {
  const page = await browser.newPage({ viewport: { width: 1400, height: 900 } });
  const errors = [];
  page.on("pageerror", (e) => errors.push(String(e)));
  page.on("console", (m) => {
    if (m.type() === "error") errors.push("console: " + m.text());
  });
  page.__errors = errors;
  if (preset) {
    await page.addInitScript((v) => {
      window.__RICHOS_MOCK_PRESET__ = v;
    }, preset);
  }
  await page.goto(APP);
  await leaveHome(page);
  await page.waitForFunction("typeof window.RichTimeline === 'object'");
  return page;
}

/// Everything about the box that says "on" or "off", in one read.
const READ_COMPOSER = `(() => {
  const i = document.getElementById("input");
  const s = document.getElementById("send");
  const cs = getComputedStyle(i);
  const blocked = document.getElementById("composer-blocked");
  return {
    placeholder: i.placeholder,
    disabled: i.disabled,
    sendDisabled: s.disabled,
    cursor: cs.cursor,
    ink: cs.color,
    describedBy: i.getAttribute("aria-describedby"),
    reason: blocked.hidden ? null : blocked.textContent.trim(),
    reasonHidden: blocked.hidden,
  };
})()`;

/// THE PLACEHOLDER'S RATIO, ON THE ARITHMETIC THE PRODUCT'S OWN CHECKER USES.
///
/// `lib/contrast.js`'s DOM walk cannot see a placeholder: it is not a text node, so it has no
/// line boxes and no `Range` to measure. It is still text a person is expected to read — it is
/// the only thing in an empty box — so it takes the 4.5:1 floor and it gets measured here.
///
/// The method is the walk's own, narrowed to one node: resolve the ground from
/// `document.elementsFromPoint` (the browser's real paint stack, not an ancestor guess),
/// composite every translucent layer down to the first opaque one, and run the SHIPPED
/// `contrastRatio` — `window.__contrastMath`, which `pageScript()` exposes precisely so a
/// check cannot quietly use a second copy of the arithmetic.
const MEASURE_PLACEHOLDER = `(() => {
  const M = window.__contrastMath;
  const i = document.getElementById("input");
  const b = i.getBoundingClientRect();
  const x = b.left + 24, y = b.top + b.height / 2;
  const stack = document.elementsFromPoint(x, y);
  const layers = [];
  for (const el of stack) {
    const c = M.parseCssColor(getComputedStyle(el).backgroundColor);
    if (!c || c.a === 0) continue;
    layers.push(c);
    if (c.a >= 1) break;
  }
  if (!layers.length) return { error: "nothing painted under the composer" };
  let ground = layers[layers.length - 1];
  for (let k = layers.length - 2; k >= 0; k--) ground = M.compositeOver(layers[k], ground);
  const fg = M.parseCssColor(getComputedStyle(i, "::placeholder").color);
  const ink = fg.a < 1 ? M.compositeOver(fg, ground) : fg;
  const px = parseFloat(getComputedStyle(i).fontSize);
  return {
    ratio: M.round2(M.contrastRatio(ink, ground)),
    ink: M.hex(ink),
    ground: M.hex(ground),
    px: px,
    large: M.isLargeText(px, getComputedStyle(i).fontWeight),
    layers: layers.length,
  };
})()`;

async function measurePlaceholder(page, theme) {
  await page.evaluate((t) => document.documentElement.setAttribute("data-theme", t), theme);
  await page.waitForTimeout(80);
  await page.evaluate(C.pageScript());
  const r = await page.evaluate(MEASURE_PLACEHOLDER);
  assert(!r.error, r.error);
  return r;
}

async function main() {
  const run = createRun("the composer when send is off: it says so, and it says why");
  const { webkit } = loadPlaywright();
  const browser = await webkit.launch();

  // =====================================================================================
  // THE WALK'S OWN PATH: first run, company deferred
  // =====================================================================================

  const page = await openApp(browser, { chosenEntity: null });
  await page.waitForSelector("#entity-picker:not([hidden])");
  await page.click("#entity-picker-later");
  await page.waitForSelector("#entity-picker", { state: "hidden" });

  await run.check("with the company deferred, the box does not say Talk to Rich", async () => {
    const c = await page.evaluate(READ_COMPOSER);
    assert(c.reason, "the block's own sentence is not on screen at all");
    assert(
      c.placeholder !== LIVE_PLACEHOLDER,
      `the box still invites a sentence it cannot take: ${JSON.stringify(c.placeholder)}`
    );
    assert(/send is off/i.test(c.placeholder), `the box must say send is off: ${JSON.stringify(c.placeholder)}`);
    return `placeholder ${JSON.stringify(c.placeholder)}; reason on screen: ${JSON.stringify(c.reason.slice(0, 48) + "…")}`;
  });

  await run.check("with the company deferred, the box IS switched off, and the send control with it", async () => {
    const c = await page.evaluate(READ_COMPOSER);
    assert(c.disabled, "the box is still enabled — it takes letters it will never send");
    assert(c.sendDisabled, "the send control is still armed over a box that cannot send");
    assertEqual(c.cursor, "not-allowed", "the pointer still says this box is typeable");
    return `disabled, send disabled, cursor ${c.cursor}, ink ${c.ink}`;
  });

  await run.check("and it swallows nothing, because nothing gets in", async () => {
    // The pre-fix behavior was the opposite and worse: letters went IN and were refused on
    // the way out, with nothing on screen changing. Typing at a disabled field is a no-op in
    // the browser itself, which is the whole reason to use the browser's own switch.
    await page.locator("#input").press("h", { force: true, timeout: 2000 }).catch(() => {});
    await page.keyboard.type("hello");
    await page.keyboard.press("Enter");
    await page.waitForTimeout(150);
    const after = await page.evaluate(() => ({
      value: document.getElementById("input").value,
      focused: (document.activeElement || {}).id || "",
      sent: document.querySelectorAll(".tl-message--ceo, .tl-user").length,
    }));
    assertEqual(after.value, "", `the box took ${JSON.stringify(after.value)} and could not send it`);
    assertEqual(after.sent, 0, "a message left a composer that is switched off");
    return `typed 6 characters and pressed Enter: value "", 0 messages, focus on ${JSON.stringify(after.focused)}`;
  });

  await run.check("the reason is attached to the control, not merely near it", async () => {
    const c = await page.evaluate(READ_COMPOSER);
    assertEqual(c.describedBy, "composer-blocked", "the box has no description, so a screen reader gets the box with no why");
    const described = await page.evaluate(() => {
      const id = document.getElementById("input").getAttribute("aria-describedby");
      const el = document.getElementById(id);
      return { exists: !!el, hidden: el ? el.hidden : null, text: el ? el.textContent.trim() : null };
    });
    assert(described.exists, "aria-describedby points at nothing");
    assert(!described.hidden, "the description is hidden, so it contributes nothing");
    assertEqual(described.text, c.reason, "the description and the sentence on screen are two different strings");
    return `aria-describedby -> #composer-blocked, ${described.text.length} characters, the same sentence that is on screen`;
  });

  for (const theme of ["dark", "light"]) {
    await run.check(`CONTRAST: the switched-off placeholder in ${theme} mode`, async () => {
      const m = await measurePlaceholder(page, theme);
      const floor = m.large ? 3 : 4.5;
      assert(m.ratio >= floor, `${m.ink} on ${m.ground} = ${m.ratio}:1 at ${m.px}px, below ${floor}:1`);
      return `${m.ink} on ${m.ground} = ${m.ratio}:1 (needs ${floor}, ${m.px}px, ${m.layers} painted layer(s))`;
    });
  }
  await page.evaluate(() => document.documentElement.setAttribute("data-theme", "dark"));

  // =====================================================================================
  // THE WAY BACK — the audit's own control: "after adding a company, typing works immediately"
  // =====================================================================================

  await run.check("CONTROL: choosing a company switches the box back on, placeholder and all", async () => {
    await page.click(".setbtn");
    await page.waitForSelector("#set-company");
    await page.selectOption("#set-company", "lumen");
    await page.waitForFunction(() => document.getElementById("composer-blocked").hidden, null, { timeout: 30000 });
    const c = await page.evaluate(READ_COMPOSER);
    assert(!c.disabled, "the box is still switched off after the question was answered");
    assert(!c.sendDisabled, "the send control is still off after the question was answered");
    assertEqual(c.placeholder, LIVE_PLACEHOLDER, "the inviting placeholder did not come back");
    assertEqual(c.reasonHidden, true, "the block's sentence is still on screen");
    // And it types, which is the thing the audit asked for by name. The settings panel is
    // closed first, as a person closes it: open, it covers the composer wherever its foot
    // reaches the box (on main at 1280 x 900 already; at this 1400 x 900 too once the Updates
    // row's action line wraps, which "See what's new" makes it do in a 320px column).
    await page.keyboard.press("Escape");
    await page.waitForSelector("#set-menu", { state: "hidden" });
    await page.click("#input");
    await page.keyboard.type("hello");
    const v = await page.evaluate(() => document.getElementById("input").value);
    assertEqual(v, "hello", "the box still will not take a sentence");
    await page.evaluate(() => {
      document.getElementById("input").value = "";
    });
    return `placeholder ${JSON.stringify(c.placeholder)}, box live, send live, typed "hello" straight into it`;
  });

  await run.check("no page errors on the deferral path", async () => {
    assertEqual(page.__errors, [], "the page reported errors");
    return "0 errors";
  });

  await page.close();

  // =====================================================================================
  // THE NEGATIVE CONTROL — every read above would pass over a composer that is ALWAYS off,
  // which is a different broken product. An ordinary launch has to come up live.
  // =====================================================================================

  const ordinary = await openApp(browser, null);

  await run.check("NEGATIVE CONTROL: an ordinary launch comes up with a live composer", async () => {
    const c = await ordinary.evaluate(READ_COMPOSER);
    assertEqual(c.placeholder, LIVE_PLACEHOLDER, "an ordinary launch does not offer to take a sentence");
    assert(!c.disabled, "an ordinary launch comes up with the box switched off");
    assert(!c.sendDisabled, "an ordinary launch comes up with send switched off");
    assertEqual(c.reasonHidden, true, "an ordinary launch shows a block sentence");
    // And `aria-describedby` costs nothing here: it points at a hidden element, which
    // contributes no description, so an unblocked box is not carrying a stale explanation.
    assertEqual(c.describedBy, "composer-blocked", "the description is wired on every launch, not only a blocked one");
    return `placeholder ${JSON.stringify(c.placeholder)}, box live, send live, no block sentence, description present but its target hidden`;
  });

  await run.check("no page errors on the ordinary path", async () => {
    assertEqual(ordinary.__errors, [], "the page reported errors");
    return "0 errors";
  });

  await ordinary.close();
  await browser.close();
  // `report()` returns the FAILED COUNT, not a verdict. Same form as every other suite here.
  process.exitCode = run.report() > 0 ? 1 : 0;
}

main().catch((e) => {
  console.error(e);
  process.exit(1);
});

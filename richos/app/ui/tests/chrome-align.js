// **WHERE THE TOP-RIGHT CHROME SITS, ON EVERY SURFACE, AND WHERE THE COMPOSER'S TWO BUTTONS
// SIT** — two CEO rulings from 2026-09-19, and the second one is a correction to the first.
//
// ## THE TWO RULINGS, IN THE ORDER HE GAVE THEM
//
// **1. The regular screens, the morning of 2026-09-19** — the settings button is centered in
// the thread header with 15px of right padding, and the two composer buttons match the text
// input. Checks 1 through 8 below, and the header that follows this one.
//
// **2. The home screen and the paused splash, the afternoon of the same day** — those two go
// back to where they were. Checks 2b through 2e. His words, with a before-and-after pair of his
// own screenshots (`docs/briefs/assets/ceo-2026-09-18-home-settings-button-before.png` and
// `ceo-2026-09-19-home-settings-button-after.png` in `richos-hq`):
//
//   *"Obviously changes to the settings button position on regular screens must have messed up
//    that button's position on the home screen (and probably also on the splash screen when
//    paused). The before position of the settings button on the home screen was good and
//    correct. The current position is messed up and wrong."*
//
// **THE SHAPE OF THE MISTAKE IS THE THING TO KEEP.** Ruling 1 was answered by moving the ONE
// fixed element `settings-button.js` mounts against `document.body` — and §15 puts that element
// on every screen, so an instruction about a 52px header reached two surfaces that have no
// header. A suite that only ever opened a thread could not see it, and this one did not. That
// is why `SURFACES` below is a table rather than three checks: the next instruction about this
// button will be about one surface too, and the table is what makes the other two answer for
// themselves.
//
// ## RULING 1, AS IT WAS WRITTEN
//
// CEO, 2026-09-19, on the nightly he watched at 8pm the evening
// before. His screenshot is `docs/briefs/assets/ceo-2026-09-18-composer-and-settings-button.png`
// in `richos-hq`; his three arrows mark the three spots this suite measures.
//
//   *"So, if those things haven't been fixed since, then the settings button needs to be
//    properly centered vertically and the right padding for that button needs to be reduced to
//    15px. And the 2 buttons at the bottom need to be either vertically center aligned relative
//    to the text input or have the same height as the text input because otherwise it looks
//    weird."*
//
// ## THE THREE NUMBERS, MEASURED ON `f918f185` BEFORE ANY OF THIS WAS WRITTEN
//
// Read through this same WebKit, at 1024x700 and at 1400x950, identical at both:
//
//     #stage-header        top 0, bottom 52            center y = 26
//     #set-btn             top 18, bottom 58           center y = 38   -> 12px LOW
//     #set-btn right edge  1009 wanted, 1006 measured                  -> 18px of padding
//     #input-shell         46px tall, center y = 661
//     #talk-toggle / #send 40px tall, center y = 664    -> 6px SHORT and 3px LOW
//
// The settings button did not merely sit low: its box ended at y=58 against a header whose
// bottom border is at y=52, so six pixels of it hung through the line. That is the overhang his
// arrow points at, and it is the reason "properly centered" is a measurement here and not an
// opinion.
//
// ## WHICH OF HIS TWO OPTIONS THE COMPOSER TOOK, AND WHY THE SUITE ACCEPTS EITHER
//
// `style.css` takes **the same height as the text input** — the reasoning is in the comment
// above `#composer-row`, and the short version is that the field grows with what he types, so
// centering is only available in one of the composer's states while matching the height
// degrades to bottom-flush in the rest. The checks below assert **his sentence**, not that
// choice: a button passes if its center matches the field's OR its height does. A later
// redesign that centers them instead is still right by the CEO, and this suite should not be
// the thing that stops it.
//
// THE FIELD IS `#input-shell`, NOT `#input`. The box a person sees is the bordered shell (46px);
// the textarea inside it is 44px. Both are reported by every check, so a reading can never be
// ambiguous about which one it matched.
//
// ## RULING 2, AND WHAT WAS MEASURED BEFORE IT WAS ANSWERED
//
// On `529dd6ec`, through this same WebKit, at 1024x700 and 1400x950 in both themes — twelve
// readings, all identical:
//
//     #set-btn on a thread                 top 6, right 15
//     #set-btn on the home screen          top 6, right 15
//     #set-btn on the held opening screen  top 6, right 15
//
// One element, one inset, three surfaces. So his parenthesis — *"and probably also on the splash
// screen when paused"* — is a confirmation rather than a guess, and the splash is checked here
// on the same footing as the home screen rather than as an afterthought.
//
// AFTER, same harness, same twelve readings: the thread is untouched at 6/15, and the two
// surfaces with no header of their own are back at 18/18, which is where `style.css` had them
// until `dcfb87c9` and where three committed home-screen shots that predate that commit still
// show them (`shots-home/home-named.png`, `home-anonymized.png`, `home-returned.png`, decoded
// at 1440x900: the button's top edge measures y=18 in each).
//
// ## THE NEGATIVE CONTROLS ARE FIRST IN EACH HALF, AND THEY ARE NOT DESCRIPTIONS
//
// Check 1 puts the three pre-fix declarations back through the CSSOM and requires the defect to
// measure EXACTLY as it measured on `f918f185` — 12px low, 18px of padding, 6px short and 3px
// low. If that check ever comes back clean, the four after it are proving nothing and say so.
//
// Check 2b does the same for ruling 2, on both corner surfaces: it forces the regular-screen
// pair onto the wrapper's own inline style — which outranks every selector in both stylesheets
// — and requires the button to measure the 6/15 the CEO photographed and called "messed up and
// wrong". Run against `529dd6ec`'s stylesheets, 2b and the nine checks after it are all red and
// the eighteen that were here before are all green; that is the split this file was extended to
// produce.
//
// Run: node chrome-align.js   (or `npm test` for every suite in this directory)

"use strict";

const path = require("path");
const {
  loadPlaywright,
  leaveHome,
  bootSettled,
  createRun,
  assert,
  assertEqual,
  SEED_THEME,
  HOLD_CURTAIN,
  UI_DIR,
} = require("./lib/harness");

const APP = "file://" + path.join(UI_DIR, "index.html");

/// The app's own minimum, and the size it restores itself to — `window_geometry.rs`'s
/// `PREFERRED_MIN_WIDTH` / `MIN_HEIGHT`. Everything here is measured at this AND at the roomy
/// size the rest of this directory uses, because a fixed-position control is the exact shape of
/// thing that lines up at one window size and not at another.
const SMALL = { width: 1024, height: 700 };
const LARGE = { width: 1400, height: 950 };

/// His number, verbatim: the gap from the button's right edge to the right edge of the content
/// area. No tolerance — it is a declared inset, not a computed layout.
const RIGHT_PADDING = 15;

/// What `f918f185` shipped, so the negative control reproduces a geometry rather than asserting
/// one. `style.css`'s own comments carry the same three numbers.
const BEFORE = { settingsTop: 18, settingsRight: 18, controlHeight: 40 };

/// **THE HOME SCREEN AND THE HELD OPENING SCREEN SIT AT 18/18** — CEO, 2026-09-19, later the
/// same day as the centering above, with a before-and-after pair of his own screenshots
/// (`docs/briefs/assets/ceo-2026-09-18-home-settings-button-before.png` and
/// `ceo-2026-09-19-home-settings-button-after.png` in `richos-hq`):
///
///   *"Obviously changes to the settings button position on regular screens must have messed
///    up that button's position on the home screen (and probably also on the splash screen when
///    paused). The before position of the settings button on the home screen was good and
///    correct. The current position is messed up and wrong."*
///
/// IT IS THE SAME PAIR AS `BEFORE` ABOVE AND IT IS DELIBERATELY A SEPARATE CONSTANT, because
/// the two mean different things and a shared name would make the next reader think one of them
/// is a copy of the other. `BEFORE` is a defect being reproduced; this is a LIVE position that
/// two surfaces are required to be at. `style.css` derives both from `--settings-top` /
/// `--settings-right`, which is the single place the numbers live.
const CORNER = { top: 18, right: 18 };

/// The regular-screen pair, as a pair, for the negative control on the corner surfaces: it is
/// what the CEO photographed and called "messed up and wrong" when it reached them.
const CENTERED = { top: 6, right: RIGHT_PADDING };

/// The two numbers `style.css` derives from whichever inset is live, so a surface check can
/// assert the JOIN rather than only the button. Anchor = inset + the 40px button + 8px of hang;
/// toast = inset + the 40px button + 12px of air.
const anchorFor = (top) => top + 48;
const toastTopFor = (top) => top + 52;

// ---------------------------------------------------------------------------------------
// The page
// ---------------------------------------------------------------------------------------

/// The bridge wrapper `affordances.js` uses, reduced to the one thing this suite needs: an
/// override for `start_voice_capture`, which `mock.js` does not implement, so voice mode cannot
/// otherwise be reached. Everything else still goes to the real mock.
const BRIDGE_OVERRIDES = () => {
  let real = null;
  window.__overrides = {};
  Object.defineProperty(window, "RichBridge", {
    configurable: true,
    get() {
      return real;
    },
    set(v) {
      real = v;
      const origInvoke = v.invoke.bind(v);
      v.invoke = function (cmd, args) {
        if (Object.prototype.hasOwnProperty.call(window.__overrides, cmd)) {
          const o = window.__overrides[cmd];
          return o && o.reject ? Promise.reject(o.value) : Promise.resolve(o ? o.value : undefined);
        }
        return origInvoke(cmd, args);
      };
    },
  });
};

/// Everything the three surfaces below share: a page that is the shipping shell, with its
/// console watched, the bridge wrapper installed, the text scale and the lighting seeded — up to
/// and including `goto`. What comes AFTER `goto` is what makes a surface a surface, and that is
/// the openers' business, not this one's.
///
/// `extraInit` is how the held-splash opener gets `HOLD_CURTAIN` in before the page runs; it is
/// applied last, after the seeds, because it wraps `RichSplash` rather than storage.
async function preparedPage(browser, viewport, theme, scale, extraInit) {
  const page = await browser.newPage({ viewport, colorScheme: theme === "light" ? "light" : "dark" });
  const errors = [];
  page.on("pageerror", (e) => errors.push(String(e)));
  page.on("console", (m) => {
    if (m.type() === "error") errors.push("console: " + m.text());
  });
  page.__errors = errors;
  await page.addInitScript(BRIDGE_OVERRIDES);
  // THE TEXT SCALE IS SEEDED BEFORE THE THEME AND THAT ORDER IS LOAD-BEARING: `SEED_THEME`
  // merges into `richos-mock-config` and only defaults `font_scale` when it finds none, so a
  // scale written first survives it and a scale written after would be the one that survives
  // — either way one of them has to know about the other, and this is the order the helper
  // was written for.
  //
  // IT IS SEEDED AT BOOT RATHER THAN SET AFTERWARDS so that check 6 measures the CSS
  // DERIVATION on a page that has never been any other size — a scale set afterwards would
  // measure the derivation and the re-layout together and could not tell them apart. The
  // afterwards case is a defect of its own and has its own check: see check 8, Ray's R1.
  if (scale && scale !== 100) {
    await page.addInitScript((s) => {
      try {
        window.localStorage.setItem("richos-font-scale", String(s));
        let prev = {};
        try {
          prev = JSON.parse(window.localStorage.getItem("richos-mock-config")) || {};
        } catch (e) {
          prev = {};
        }
        prev.font_scale = s;
        window.localStorage.setItem("richos-mock-config", JSON.stringify(prev));
      } catch (e) {
        /* storage unavailable: the default scale applies and the check below says so */
      }
    }, scale);
  }
  // THIS SUITE STATES ITS LIGHTING EVEN THOUGH IT PHOTOGRAPHS NOTHING AND ASSERTS NO COLOR.
  // Since CEO §63 the shipped default preference is `system`, which resolves against the
  // browser context's `colorScheme` — so "said nothing" now means "followed the host". None of
  // the geometry below depends on the palette, and a check that has never been run in a stated
  // lighting cannot say that; this is how it gets to.
  await page.addInitScript(SEED_THEME, theme || "dark");
  if (extraInit) await page.addInitScript(extraInit);
  await page.goto(APP);
  return page;
}

/// THE REGULAR SCREENS — a thread, with `#stage-header` in front. Unchanged in what it does from
/// what this file has always done; it is now one of three openers rather than the only one.
async function openApp(browser, viewport, theme, scale) {
  const page = await preparedPage(browser, viewport, theme, scale);
  await leaveHome(page);
  await page.waitForFunction("typeof window.RichTimeline === 'object'");
  await bootSettled(page);
  return page;
}

/// THE HOME SCREEN — and the difference from `openApp` is one line that is NOT there:
/// `leaveHome`. The app boots with this surface in front, so the whole of the work here is
/// sending the curtain away (a separate feature with its own suite) and then waiting for the
/// home screen's own two facts rather than for a timer.
///
/// `body.home-open` IS ASSERTED AND NOT ASSUMED, because it is the selector `style.css` keys the
/// 18/18 inset on. A page that had reached the home screen without that class would measure the
/// regular-screen inset and the check below would blame the CSS for it.
async function openHome(browser, viewport, theme) {
  const page = await preparedPage(browser, viewport, theme);
  await page.waitForFunction("typeof window.RichHome === 'object'", null, { timeout: 15000 });
  await page.evaluate(() => window.RichSplash && window.RichSplash.yieldNow("chrome-align"));
  await page.waitForFunction(() => !document.getElementById("splash"), null, { timeout: 30000 });
  await page.waitForFunction(
    () => window.RichHome.isOpen() && document.body.classList.contains("home-open"),
    null,
    { timeout: 10000 }
  );
  // Two frames, so nothing below reads a box the class change has not been laid out into.
  await page.evaluate(() => new Promise((r) => requestAnimationFrame(() => requestAnimationFrame(r))));
  return page;
}

/// THE OPENING SCREEN, HELD — CEO §62's one state in which this button is on that surface at
/// all. `HOLD_CURTAIN` is the harness's own helper: it mutes the exported `yieldNow` and takes
/// the single timer `start()` arms, which is the ceiling, so the curtain stays up for as long as
/// the check needs rather than for the 3s it ships with. The space key is then the product's own
/// path into `splash--paused`, pressed exactly once.
///
/// WHY THE PAUSED CLASS IS ASSERTED HERE: `splash.css` keys BOTH the button's presence and its
/// inset on `#splash.splash--paused ~ .settings`. A page that was not actually held would have
/// no button to measure and the failure would read as a missing element rather than as a hold
/// that did not take.
async function openHeldSplash(browser, viewport, theme) {
  const page = await preparedPage(browser, viewport, theme, null, HOLD_CURTAIN);
  await page.waitForSelector("#splash", { timeout: 15000 });
  await page.keyboard.press("Space");
  await page.waitForFunction(
    () => {
      const n = document.getElementById("splash");
      return !!n && n.classList.contains("splash--paused");
    },
    null,
    { timeout: 10000 }
  );
  await page.evaluate(() => new Promise((r) => requestAnimationFrame(() => requestAnimationFrame(r))));
  return page;
}

/// The three surfaces, in one table, so every check below runs on all of them by construction
/// rather than by three copies of itself. `header` says whether there is a 52px bar in FRONT of
/// the app for the button to be centered in — the home screen and the held curtain are drawn
/// over it, so `#stage-header` is in the DOM on all three and is the surface's own bar on only
/// one.
const SURFACES = [
  { name: "a thread", open: openApp, inset: CENTERED, header: true },
  { name: "the home screen", open: openHome, inset: CORNER, header: false },
  { name: "the held opening screen", open: openHeldSplash, inset: CORNER, header: false },
];

/// The two derived boxes, read after their own entry animations have landed rather than during
/// them. Both are real animations and both were caught mid-flight while this was being written:
/// `.setmenu` carries `setrise 0.2s` and `#bug-toast` carries `setrise 0.3s`, and a toast
/// sampled on arrival reads `top 72` on a thread where the declaration is 58.
async function measureDerived(page) {
  await page.click("#set-btn");
  await page.waitForSelector("#set-menu", { state: "visible" });
  await page.evaluate(async () => {
    const m = document.getElementById("set-menu");
    if (m) await Promise.all(m.getAnimations({ subtree: true }).map((a) => a.finished.catch(() => {})));
  });
  const menu = await page.evaluate(() => {
    const b = document.getElementById("set-btn").getBoundingClientRect();
    const m = document.getElementById("set-menu").getBoundingClientRect();
    return {
      btnBottom: Math.round(b.bottom),
      menuTop: Math.round(m.top),
      menuBottom: Math.round(m.bottom),
      maxHeight: Math.round(parseFloat(getComputedStyle(document.getElementById("set-menu")).maxHeight)),
      vh: window.innerHeight,
    };
  });
  // SINCE ROUND 21 (CEO §115) BUST A BUG OPENS THE REPORT, NOT THE TOAST, so pressing it raises
  // nothing to measure here and this waited out 30 s on all six surfaces (red on f619e9e5f). The
  // toast is still the menu's (`RichSettings.toast`): it is what says a report went out and what
  // dictation's notices use. So the menu is closed, as Bust a bug closed it, and the toast is
  // raised the way those callers raise it, with words they really show.
  await page.click("#set-btn");
  await page.waitForSelector("#set-menu", { state: "hidden" });
  await page.evaluate(() => window.RichSettings.toast("Your bug report went out: issue #412 on GitHub."));
  await page.waitForSelector("#bug-toast", { state: "visible" });
  await page.evaluate(async () => {
    const t = document.getElementById("bug-toast");
    await Promise.all(t.getAnimations({ subtree: true }).map((a) => a.finished.catch(() => {})));
  });
  const toast = await page.evaluate(() => {
    const t = document.getElementById("bug-toast").getBoundingClientRect();
    return { top: Math.round(t.top), right: Math.round(window.innerWidth - t.right) };
  });
  return Object.assign(menu, { toastTop: toast.top, toastRight: toast.right });
}

// ---------------------------------------------------------------------------------------
// The measurement
// ---------------------------------------------------------------------------------------

/// Every box the CEO's sentence is about, in one read, plus the two derived numbers so a
/// failure message carries the arithmetic rather than the raw rectangles.
const MEASURE = `(() => {
  const box = (el) => {
    if (!el) return null;
    const r = el.getBoundingClientRect();
    if (r.width === 0 && r.height === 0) return null;
    return {
      top: +r.top.toFixed(2), bottom: +r.bottom.toFixed(2),
      left: +r.left.toFixed(2), right: +r.right.toFixed(2),
      w: +r.width.toFixed(2), h: +r.height.toFixed(2),
      cy: +((r.top + r.bottom) / 2).toFixed(2),
    };
  };
  const q = (s) => box(document.querySelector(s));
  const header = q("#stage-header");
  const setbtn = q("#set-btn");
  const field = q("#input-shell");
  return {
    vw: window.innerWidth,
    vh: window.innerHeight,
    mode: document.getElementById("composer-row").getAttribute("data-mode"),
    voiceOpen: !document.getElementById("voice-panel").hidden,
    header,
    setbtn,
    settingsRightPadding: setbtn ? +(window.innerWidth - setbtn.right).toFixed(2) : null,
    settingsCenterDelta: header && setbtn ? +(setbtn.cy - header.cy).toFixed(2) : null,
    field,
    textarea: q("#input"),
    controls: ["talk-toggle", "stop", "send"].reduce((acc, id) => {
      const el = document.getElementById(id);
      const b = el && !el.hidden ? box(el) : null;
      if (b && field) acc[id] = { box: b, dCenter: +(b.cy - field.cy).toFixed(2), dHeight: +(b.h - field.h).toFixed(2),
                          dBottom: +(b.bottom - field.bottom).toFixed(2) };
      return acc;
    }, {}),
  };
})()`;

const measure = (page) => page.evaluate(MEASURE);

/// His sentence, as a predicate, per control: centered on the field OR the same height as it.
/// Tolerances are the brief's: +/-1px on a center (it is the sum of two layout computations),
/// +/-0 on a height (it is one declaration).
function holdsForControl(c) {
  return Math.abs(c.dCenter) <= 1 || c.dHeight === 0;
}

function describeControls(m) {
  return Object.keys(m.controls)
    .map((id) => {
      const c = m.controls[id];
      return `${id} ${c.box.h}px center ${c.box.cy} (dh ${c.dHeight}, dc ${c.dCenter})`;
    })
    .join("; ");
}

/// Every state of the composer his sentence has to hold in, each driven through the shipping
/// path rather than by setting an attribute — a fixture that painted the state would be proving
/// the fixture.
const STATES = {
  /// Nothing running, nothing typed. What his screenshot shows.
  async idle() {
    return "idle";
  },
  /// §9.2: a live turn with words in the box, so Stop and Send are BOTH up. The only state in
  /// which all three controls are on screen at once.
  async "working-stop-and-send"(page) {
    await page.evaluate(() => window.__RICHOS_MOCK__.simulateSlowTurn(null, "run the numbers", 200));
    await page.waitForSelector('#composer-row[data-mode="working"]');
    await page.fill("#input", "and check the Q4 line");
    await page.waitForSelector("#stop:not([hidden])");
    await page.waitForSelector("#send:not([hidden])");
    return "working, stop + send";
  },
  /// §11's `stopping`: the controls go quiet and the field does not. A disabled button is still
  /// a button in the row.
  async stopping(page) {
    await STATES["working-stop-and-send"](page);
    await page.evaluate(() => {
      // The backend returns its active input; the UI sends only the conversation ID.
      const live = [...window.__RICHOS_TIMELINE__().turns.entries()].filter(([, turn]) => turn.live);
      if (live.length !== 1) throw new Error("the geometry fixture needs exactly one live input");
      const [turnId] = live[0];
      if (typeof turnId !== "string") throw new Error("the geometry fixture needs its backend turn ID");
      window.__overrides.stop_turn = { value: { stopped: true, turnId, reachedLease: true } };
    });
    await page.click("#stop");
    await page.waitForSelector('#composer-row[data-mode="stopping"]');
    return "stopping, both controls disabled";
  },
  /// Voice mode: `#composer` is replaced inline by `#voice-panel` and `#talk-toggle` is the only
  /// control left in the row. The field is gone, so this state is measured against the ROW's own
  /// height rather than against the field — see the check.
  async voice(page) {
    await page.evaluate(() => {
      window.__overrides.start_voice_capture = { value: {} };
    });
    await page.click("#talk-toggle");
    await page.waitForSelector("#voice-panel:not([hidden])");
    return "voice";
  },
};

// ---------------------------------------------------------------------------------------
// The checks
// ---------------------------------------------------------------------------------------

async function main() {
  const run = createRun("The settings button and the composer buttons line up (CEO, 2026-09-19)");
  const { webkit } = loadPlaywright();
  const browser = await webkit.launch();

  // ---- 1. the negative control ----------------------------------------------------------
  await run.check(
    "NEGATIVE CONTROL: with the pre-fix declarations back, all three defects measure exactly as they did on f918f185",
    async () => {
      const page = await openApp(browser, SMALL);
      await page.addStyleTag({
        content:
          `.settings { top: ${BEFORE.settingsTop}px; right: ${BEFORE.settingsRight}px; }\n` +
          `#talk-toggle, #stop, #send { height: ${BEFORE.controlHeight}px; }\n`,
      });
      const m = await measure(page);
      assertEqual(m.settingsCenterDelta, 12, "the settings button is not 12px below the header's center");
      assertEqual(m.settingsRightPadding, 18, "the settings button's right padding is not the 18px that shipped");
      assert(
        m.setbtn.bottom > m.header.bottom,
        `the button no longer hangs through the header's bottom border (button ${m.setbtn.bottom}, header ${m.header.bottom})`
      );
      for (const id of Object.keys(m.controls)) {
        const c = m.controls[id];
        assertEqual(c.dHeight, -6, `${id} is not 6px shorter than the field`);
        assertEqual(c.dCenter, 3, `${id} is not 3px below the field's center`);
        assert(!holdsForControl(c), `${id} satisfies the CEO's sentence at the PRE-FIX geometry`);
      }
      await page.close();
      return (
        `settings center ${m.setbtn.cy} vs header ${m.header.cy} (+${m.settingsCenterDelta}), ` +
        `right padding ${m.settingsRightPadding}px, box ends ${m.setbtn.bottom} through a border at ${m.header.bottom}; ` +
        describeControls(m)
      );
    }
  );

  // ---- 2. the settings button, at both window sizes --------------------------------------
  for (const vp of [SMALL, LARGE]) {
    await run.check(
      `the settings button is centered in the header and 15px from the right edge, at ${vp.width}x${vp.height}`,
      async () => {
        const page = await openApp(browser, vp);
        const m = await measure(page);
        assertEqual(m.vw, vp.width, "the viewport is not the asked-for window");
        assert(m.header, "there is no #stage-header on this surface to be centered in");
        assert(
          Math.abs(m.settingsCenterDelta) <= 1,
          `the button's center is ${m.setbtn.cy} against the header's ${m.header.cy} ` +
            `(${m.settingsCenterDelta}px out; the CEO asked for properly centered)`
        );
        assertEqual(
          m.settingsRightPadding,
          RIGHT_PADDING,
          `the right padding is ${m.settingsRightPadding}px and the CEO asked for ${RIGHT_PADDING}px`
        );
        assert(
          m.setbtn.top >= m.header.top && m.setbtn.bottom <= m.header.bottom,
          `the button's box (${m.setbtn.top}..${m.setbtn.bottom}) is not inside the header's ` +
            `(${m.header.top}..${m.header.bottom}) — centered but overhanging is not centered`
        );
        assertEqual(page.__errors, [], "the page reported errors");
        await page.close();
        return (
          `header ${m.header.top}..${m.header.bottom} center ${m.header.cy}; ` +
          `button ${m.setbtn.top}..${m.setbtn.bottom} center ${m.setbtn.cy} ` +
          `(${m.settingsCenterDelta}px out); right padding ${m.settingsRightPadding}px of ${m.vw}`
        );
      }
    );
  }

  // ---- 2b. NEGATIVE CONTROL for the two corner surfaces -----------------------------------
  //
  // **THE SECOND HALF OF THE CEO'S 2026-09-19, AND IT IS A CORRECTION TO THE FIRST HALF.**
  // `dcfb87c9` centered this button for the thread header — correctly, and check 2 above is
  // that — by moving the ONE fixed element `settings-button.js` mounts against `document.body`.
  // §15 puts that element on every screen, so two surfaces that have no header moved with it.
  // He photographed the result:
  //
  //   *"Obviously changes to the settings button position on regular screens must have messed
  //    up that button's position on the home screen (and probably also on the splash screen
  //    when paused). The before position of the settings button on the home screen was good and
  //    correct. The current position is messed up and wrong."*
  //
  // MEASURED ON `529dd6ec` BEFORE ANY OF THIS WAS WRITTEN, at both window sizes in both themes,
  // twelve readings identical: `#set-btn` `top 6 right 15` on a thread, on the home screen AND
  // on the held opening screen. His "probably" is therefore a confirmation, not a guess — one
  // element, one inset, three surfaces.
  //
  // This check puts that inset back through the wrapper's own inline style, which outranks every
  // selector in both stylesheets, and requires the defect to measure EXACTLY as he photographed
  // it. If it ever comes back at 18/18 the two checks after it are proving nothing and say so.
  for (const surface of SURFACES.filter((s) => s.inset === CORNER)) {
    await run.check(
      `NEGATIVE CONTROL: with the regular-screen inset forced onto ${surface.name}, the button measures the 6/15 the CEO called wrong`,
      async () => {
        const page = await surface.open(browser, SMALL, "dark");
        const before = await measure(page);
        assertEqual(before.setbtn.top, CORNER.top, `${surface.name} is not at the restored inset to begin with`);
        await page.evaluate((c) => {
          const w = document.querySelector(".settings");
          w.style.setProperty("--settings-top", c.top + "px");
          w.style.setProperty("--settings-right", c.right + "px");
        }, CENTERED);
        const m = await measure(page);
        assertEqual(m.setbtn.top, CENTERED.top, `the forced inset did not take on ${surface.name}`);
        assertEqual(
          m.settingsRightPadding,
          CENTERED.right,
          `the forced right padding did not take on ${surface.name}`
        );
        assert(
          m.setbtn.top !== CORNER.top,
          `${surface.name} measures the restored inset even with the regular-screen pair forced on it — ` +
            "the positive checks below cannot be distinguishing the two"
        );
        assertEqual(page.__errors, [], "the page reported errors");
        await page.close();
        return (
          `${surface.name}: button ${before.setbtn.top}/${before.settingsRightPadding} restored, ` +
          `${m.setbtn.top}/${m.settingsRightPadding} with the regular-screen pair forced on — ` +
          "which is the geometry of his 'after' screenshot"
        );
      }
    );
  }

  // ---- 2c. each surface sits at its own inset, both window sizes, both themes ---------------
  //
  // THE TABLE IS THE CHECK. One loop over `SURFACES` is what makes "two positions, one control"
  // a measurement instead of three hand-written copies that can drift apart — and it is what
  // would catch a fourth surface being given the wrong one, because adding it to the table is
  // all anybody has to remember.
  //
  // BOTH THEMES, AND NEITHER IS A RE-RUN OF THE OTHER EVEN THOUGH NO GEOMETRY HERE IS A COLOR.
  // The home screen and the opening screen are clamped dark by §15 while the thread follows the
  // CEO's own preference, so "the same two numbers in both lightings" is the claim that the
  // clamp moves no boxes — which is exactly the kind of thing nothing else in this file asks.
  for (const surface of SURFACES) {
    for (const vp of [SMALL, LARGE]) {
      await run.check(
        `on ${surface.name} the settings button sits at top ${surface.inset.top}, right ${surface.inset.right}, at ${vp.width}x${vp.height} in both themes`,
        async () => {
          const seen = [];
          for (const theme of ["dark", "light"]) {
            const page = await surface.open(browser, vp, theme);
            const m = await measure(page);
            assertEqual(m.vw, vp.width, "the viewport is not the asked-for window");
            assertEqual(
              m.setbtn.top,
              surface.inset.top,
              `${surface.name} in ${theme}: the button's top edge is ${m.setbtn.top} and this surface's inset is ${surface.inset.top}`
            );
            assertEqual(
              m.settingsRightPadding,
              surface.inset.right,
              `${surface.name} in ${theme}: the right padding is ${m.settingsRightPadding}px against this surface's ${surface.inset.right}px`
            );
            assertEqual(m.setbtn.h, 40, "the button is not the 40px box every derivation here assumes");
            if (surface.header) {
              // The only surface with a bar of its own in front. Check 2 above owns the
              // centering; this repeats the containment so the table's own row is complete.
              assert(
                m.setbtn.top >= m.header.top && m.setbtn.bottom <= m.header.bottom,
                `the button's box (${m.setbtn.top}..${m.setbtn.bottom}) hangs out of the header's (${m.header.top}..${m.header.bottom})`
              );
            } else {
              // `#stage-header` IS in the DOM here — the shell is behind this surface, inert —
              // and it is NOT this surface's bar. Said out loud because a reader who greps for
              // `#stage-header` in this file would otherwise conclude the corner surfaces are
              // being measured against a header they do not have.
              assert(
                m.setbtn.bottom > m.header.bottom,
                `${surface.name} put the button inside the shell's header box, which is behind this surface, ` +
                  "not on it — the 18px inset is not a centering and must not measure as one"
              );
            }
            assertEqual(page.__errors, [], "the page reported errors");
            seen.push(`${theme} ${m.setbtn.top}..${m.setbtn.bottom}/${m.settingsRightPadding} of ${m.vw}`);
            await page.close();
          }
          return `${surface.name}: ` + seen.join("; ");
        }
      );
    }
  }

  // ---- 2d. the two derived boxes follow whichever inset is live ----------------------------
  //
  // A SURFACE-DEPENDENT POSITION MAKES EVERY NUMBER DERIVED FROM IT SURFACE-DEPENDENT TOO, and
  // a flat value is the failure mode: `.setmenu`'s bound and `#bug-toast`'s corner were both
  // literal pixels until this commit, right on one surface and quietly stale on the other two.
  // So the arithmetic is asserted here rather than the pixels — `anchorFor` and `toastTopFor`
  // take the surface's inset and nothing else.
  //
  // THE BOUND IS CHECKED AS `100vh - (inset + 66)` and the 66 is 40 + 8 + 18: the button, the
  // 8px this panel hangs below it, and the 18px bottom gutter that keeps it off the window edge.
  for (const surface of SURFACES) {
    for (const vp of [SMALL, LARGE]) {
      await run.check(
        `on ${surface.name} the settings panel and the bug toast follow the button's own inset, at ${vp.width}x${vp.height}`,
        async () => {
          const page = await surface.open(browser, vp, "dark");
          const d = await measureDerived(page);
          assertEqual(d.vh, vp.height, "the viewport is not the asked-for window");
          assertEqual(
            d.menuTop,
            anchorFor(surface.inset.top),
            `the panel's anchor is ${d.menuTop}; ${surface.name}'s button is at ${surface.inset.top} and the panel hangs 8px below its 40px box`
          );
          assertEqual(d.menuTop - d.btnBottom, 8, "the panel no longer hangs 8px below the button");
          assertEqual(
            d.maxHeight,
            vp.height - (surface.inset.top + 66),
            `the panel's bound is ${d.maxHeight}px; from an anchor of ${anchorFor(surface.inset.top)} with an 18px gutter it is ${vp.height - (surface.inset.top + 66)}px`
          );
          assert(d.menuBottom <= d.vh, `the panel ends at ${d.menuBottom} in a ${d.vh}px window`);
          assertEqual(
            d.toastTop,
            toastTopFor(surface.inset.top),
            `the toast opens at ${d.toastTop}; it keeps 12px of air under a 40px button at ${surface.inset.top}`
          );
          assertEqual(
            d.toastRight,
            surface.inset.right,
            `the toast's right inset is ${d.toastRight} against the button's ${surface.inset.right}`
          );
          assertEqual(page.__errors, [], "the page reported errors");
          await page.close();
          return (
            `${surface.name}: button ends ${d.btnBottom}; panel top ${d.menuTop} (8px below), ` +
            `max-height ${d.maxHeight}px of ${d.vh}, bottom ${d.menuBottom}; toast ${d.toastTop}/${d.toastRight}`
          );
        }
      );
    }
  }

  // ---- 2e. the handover, in both directions -------------------------------------------------
  //
  // **THE POSITION IS KEYED ON A CLASS, SO THE CLASS COMING OFF IS PART OF THE FEATURE.** A
  // surface-dependent inset has one failure this file can actually prevent: bookkeeping left
  // behind. If `body.home-open` outlived the home screen, every thread in that session would
  // carry an un-centered button and check 2 would still be green, because check 2 opens its own
  // page and never goes back.
  //
  // DRIVEN THROUGH `RichHome`'s OWN DOOR AND BACK — `hide()` then `show()` — rather than by
  // toggling the class, because the class is the thing under test.
  //
  // THE INSTANT IS THE START OF THE FADE, NOT THE END OF IT, and that is the same handover
  // `splash.css` chose for this button with `:not(.splash--yielding)`: `home.js` drops
  // `home-open` before it starts the leaving fade, so the button is back in its regular place in
  // the same instant the app behind it begins to appear.
  await run.check(
    "leaving the home screen puts the button back at 6/15, and coming back puts it at 18/18",
    async () => {
      const page = await openHome(browser, SMALL, "dark");
      const onHome = await measure(page);
      assertEqual(onHome.setbtn.top, CORNER.top, "the home screen did not start at its own inset");

      await page.evaluate(() => window.RichHome.hide("chrome-align"));
      await page.waitForFunction(() => !document.body.classList.contains("home-open"));
      await page.evaluate(() => new Promise((r) => requestAnimationFrame(() => requestAnimationFrame(r))));
      const left = await measure(page);
      assertEqual(left.setbtn.top, CENTERED.top, "the button did not return to the centered inset when the home screen left");
      assertEqual(left.settingsRightPadding, CENTERED.right, "the right padding did not return when the home screen left");
      assert(
        Math.abs(left.settingsCenterDelta) <= 1,
        `back on a thread the button's center is ${left.settingsCenterDelta}px from the header's — ` +
          "the home screen took the centering with it when it left"
      );

      await page.evaluate(() => window.RichHome.show("chrome-align"));
      await page.waitForFunction(() => document.body.classList.contains("home-open"));
      await page.evaluate(() => new Promise((r) => requestAnimationFrame(() => requestAnimationFrame(r))));
      const back = await measure(page);
      assertEqual(back.setbtn.top, CORNER.top, "coming back to the home screen did not restore its inset");
      assertEqual(back.settingsRightPadding, CORNER.right, "coming back to the home screen did not restore its right padding");
      assertEqual(page.__errors, [], "the page reported errors");
      await page.close();
      return (
        `home ${onHome.setbtn.top}/${onHome.settingsRightPadding} -> ` +
        `thread ${left.setbtn.top}/${left.settingsRightPadding} (${left.settingsCenterDelta}px off the header's center) -> ` +
        `home again ${back.setbtn.top}/${back.settingsRightPadding}`
      );
    }
  );

  // ---- 3. the composer, in every state, at both window sizes -----------------------------
  for (const vp of [SMALL, LARGE]) {
    for (const state of ["idle", "working-stop-and-send", "stopping"]) {
      await run.check(
        `the composer buttons match the text input in the "${state}" state, at ${vp.width}x${vp.height}`,
        async () => {
          const page = await openApp(browser, vp);
          const label = await STATES[state](page);
          const m = await measure(page);
          assert(m.field, "there is no #input-shell to match");
          const ids = Object.keys(m.controls);
          assert(ids.length > 0, "no composer control is on screen in this state");
          if (state !== "idle") {
            assert(ids.includes("stop"), `the "${state}" state did not put #stop on screen`);
          }
          for (const id of ids) {
            const c = m.controls[id];
            assert(
              holdsForControl(c),
              `${id} is ${c.box.h}px against the field's ${m.field.h}px and its center is ` +
                `${c.dCenter}px from the field's — neither centered nor the same height`
            );
          }
          assertEqual(page.__errors, [], "the page reported errors");
          await page.close();
          return `${label}: field ${m.field.h}px center ${m.field.cy}; ` + describeControls(m);
        }
      );
    }
  }

  // ---- 4. voice mode ----------------------------------------------------------------------
  await run.check(
    "in voice mode the talk control keeps the field's height and the row it lives in does not move",
    async () => {
      // THE FIELD IS GONE IN THIS STATE, so there is nothing for the CEO's sentence to be
      // about — `#composer` is replaced inline by `#voice-panel` and `#talk-toggle` is the only
      // control left. What is measured instead is that the taller button did not push the
      // composer around, and it is measured as a CONTROLLED comparison: the same page, same
      // state, with the pre-fix 40px put back through the CSSOM.
      //
      // MEASURED, AND IT CORRECTED A GUESS. The guess was that the 40px talk control set the
      // voice-mode row height, so growing it to 46 would grow the row. It does not: the voice
      // panel is the tallest thing in that row at 59px, and the row reads 91px with the button
      // at 40 and 91px with it at 46. That the composer is 78px in text mode and 91px in voice
      // mode is pre-existing, is not what the CEO's sentence is about, and is untouched here.
      const page = await openApp(browser, SMALL);
      const field = (await measure(page)).field.h;
      const row = () =>
        page.evaluate(() => {
          const r = document.getElementById("composer-row").getBoundingClientRect();
          const t = document.getElementById("talk-toggle").getBoundingClientRect();
          const v = document.getElementById("voice-panel").getBoundingClientRect();
          return {
            row: +r.height.toFixed(2),
            talk: +t.height.toFixed(2),
            panel: +v.height.toFixed(2),
            talkBottom: +t.bottom.toFixed(2),
            panelBottom: +v.bottom.toFixed(2),
          };
        });
      const textMode = await row();
      await STATES.voice(page);
      const voiceMode = await row();
      assertEqual(voiceMode.talk, field, "the talk control is not the field's height in voice mode");
      assertEqual(voiceMode.talk, textMode.talk, "the talk control changed height with the mode");
      assertEqual(
        voiceMode.talkBottom,
        voiceMode.panelBottom,
        "the talk control is not flush with the bottom of the voice panel beside it"
      );
      const style = await page.addStyleTag({
        content: `#talk-toggle, #stop, #send { height: ${BEFORE.controlHeight}px; }\n`,
      });
      const asShipped = await row();
      await page.evaluate((el) => el.remove(), style);
      assertEqual(
        voiceMode.row,
        asShipped.row,
        "the taller talk control changed the height of the composer in voice mode"
      );
      assertEqual(page.__errors, [], "the page reported errors");
      await page.close();
      return (
        `text mode: row ${textMode.row}px, talk ${textMode.talk}px (field ${field}px); ` +
        `voice mode: row ${voiceMode.row}px, panel ${voiceMode.panel}px, talk ${voiceMode.talk}px flush at ${voiceMode.talkBottom}; ` +
        `same row at the pre-fix ${BEFORE.controlHeight}px: ${asShipped.row}px`
      );
    }
  );

  // ---- 5. the field when it grows ---------------------------------------------------------
  await run.check(
    "when the field grows past one line the buttons stay flush with its bottom edge",
    async () => {
      // THE DOCUMENTED DEGRADATION OF THE OPTION THIS FILE TOOK, asserted rather than left to
      // be discovered. "Same height as the text input" is a statement about the resting field;
      // `#input` grows to `max-height: 112px` with what he types, and `align-items: flex-end`
      // is what keeps the two controls beside the LAST line rather than beside the middle. So
      // the promise in this state is bottom-flush, and this check holds it to that.
      const page = await openApp(browser, SMALL);
      const restingField = (await measure(page)).field.h;
      await page.fill("#input", "one\ntwo\nthree\nfour\nfive\nsix");
      await page.evaluate(() => {
        const i = document.getElementById("input");
        i.dispatchEvent(new Event("input", { bubbles: true }));
      });
      await page.waitForFunction(
        (h) => document.getElementById("input-shell").getBoundingClientRect().height > h,
        restingField
      );
      const m = await measure(page);
      assert(m.field.h > restingField, `the field did not grow (still ${m.field.h}px)`);
      for (const id of Object.keys(m.controls)) {
        const c = m.controls[id];
        assertEqual(c.dBottom, 0, `${id}'s bottom edge is ${c.dBottom}px from the grown field's`);
      }
      assertEqual(page.__errors, [], "the page reported errors");
      await page.close();
      return (
        `field grew ${restingField}px -> ${m.field.h}px; ` +
        Object.keys(m.controls)
          .map((id) => `${id} bottom ${m.controls[id].dBottom}px from the field's`)
          .join("; ")
      );
    }
  );

  // ---- 6. the derivation, not the number ---------------------------------------------------
  await run.check(
    "at 120% text size the buttons still track the field, so the match is derived and not a typed 46",
    async () => {
      // `--app-font-scale` multiplies the root font size and every size in `style.css` is rem,
      // so a button whose height was TYPED as `46px` would come apart from the field by 4px the
      // moment the CEO used the Text size row. That is what `--control-h:
      // calc(var(--field-h) + 2px)` exists to prevent, and this is the check that would catch
      // it: 120 is a real step in `RichTheme.STEPS` ([80, 90, 100, 110, 120, 135, 150]).
      //
      // THE TOLERANCE HERE IS 1px AND EVERY OTHER CHECK'S IS 0, WHICH IS A MEASUREMENT AND NOT
      // A CONCESSION. `main.js`'s `autoGrow()` writes the field's height inline from
      // `scrollHeight`, which is an INTEGER; the stylesheet's calc keeps the fraction. At 110%
      // the field reads 48px and the buttons 48.39px. At 100% — the default, and what the CEO
      // was looking at — both are exactly 46 and the other checks hold them to that.
      const page = await openApp(browser, SMALL, "dark", 120);
      const scale = await page.evaluate(() => window.RichTheme.scale());
      assertEqual(scale, 120, "the seeded text scale did not take, so this check is a re-run of the default");
      const m = await measure(page);
      assert(
        m.field.h > 46,
        `the field did not respond to the text scale (${m.field.h}px, same as the default)`
      );
      for (const id of Object.keys(m.controls)) {
        const c = m.controls[id];
        assert(
          Math.abs(c.dHeight) <= 1,
          `${id} came apart from the field at 120%: ${c.box.h}px against ${m.field.h}px`
        );
        assert(
          c.box.h > 46,
          `${id} is still ${c.box.h}px at 120%, so its height is typed rather than derived`
        );
      }
      assertEqual(page.__errors, [], "the page reported errors");
      await page.close();
      return `at 120%: field ${m.field.h}px (46 at the default); ` + describeControls(m);
    }
  );

  // ---- 7. the settings menu still hangs from the button it moved with ----------------------
  await run.check(
    "the settings menu's anchor follows the button to 54px, which is what its max-height is derived from",
    async () => {
      // `settings-fit.js` owns the FIT of that panel; this check owns the JOIN between the two
      // numbers, because the panel's `max-height: calc(100vh - 72px)` is derived from this
      // button's `top` and would have gone quietly stale when the button moved.
      const page = await openApp(browser, SMALL);
      await page.click("#set-btn");
      await page.waitForSelector("#set-menu", { state: "visible" });
      await page.evaluate(async () => {
        const m = document.getElementById("set-menu");
        if (m) await Promise.all(m.getAnimations({ subtree: true }).map((a) => a.finished.catch(() => {})));
      });
      const g = await page.evaluate(() => {
        const b = document.getElementById("set-btn").getBoundingClientRect();
        const m = document.getElementById("set-menu").getBoundingClientRect();
        return {
          btnBottom: Math.round(b.bottom),
          menuTop: Math.round(m.top),
          menuBottom: Math.round(m.bottom),
          vh: window.innerHeight,
        };
      });
      assertEqual(g.menuTop, 54, "the panel's anchor is not the 54px style.css derives the 72px bound from");
      assertEqual(g.menuTop - g.btnBottom, 8, "the panel no longer hangs 8px below the button");
      assert(g.menuBottom <= g.vh, `the panel ends at ${g.menuBottom} in a ${g.vh}px window`);
      assertEqual(page.__errors, [], "the page reported errors");
      await page.close();
      return `button ends ${g.btnBottom}, panel top ${g.menuTop} (8px below), bottom ${g.menuBottom} of ${g.vh}`;
    }
  );

  // ---- 8. the Text size row, used the way he uses it ---------------------------------------
  await run.check(
    "changing the text size keeps the field and its buttons the same height, in BOTH directions",
    async () => {
      // **RAY'S CANDIDATE .13, DEFECT R1**, and it is the CEO's own sentence breaking at the one
      // moment he is looking hardest at the composer. Measured by Ray at 5x, both themes, both
      // window sizes:
      //
      //     100% -> 110%   buttons 48px, field stays 46
      //     110% -> 100%   field stays 48, buttons drop to 46
      //     one character typed -> they agree again, 48/48 then 46/46
      //
      // The mechanism: `style.css` derives `--control-h` from `--field-h` and every size is rem
      // off a root the scale multiplies, so the BUTTONS follow on their own. The field does not,
      // because `main.js`'s `autoGrow()` writes its height INLINE in pixels and none of its call
      // sites was a scale change — they are all keystrokes, sends, restored drafts and thread
      // openings. Check 6 above cannot see this: it seeds the scale before boot, so there is no
      // stale inline declaration to survive.
      //
      // DRIVEN THROUGH THE ROW HE ACTUALLY USES — the settings button, then `#font-up` /
      // `#font-down` — rather than by calling `RichTheme.setScale`, because the defect is about
      // what happens when the product changes its own scale.
      const page = await openApp(browser, SMALL);
      const step = async (id) => {
        await page.click("#set-btn");
        await page.waitForSelector("#set-menu", { state: "visible" });
        await page.click(id);
        await page.keyboard.press("Escape");
        await page.waitForSelector("#set-menu", { state: "hidden" });
        // Two frames: one for the root's new font size to lay out, one for anything answering it.
        await page.evaluate(
          () => new Promise((r) => requestAnimationFrame(() => requestAnimationFrame(r)))
        );
        return measure(page);
      };

      const start = await measure(page);
      assertEqual(await page.evaluate(() => window.RichTheme.scale()), 100, "this page did not start at 100%");

      const bigger = await step("#font-up");
      assertEqual(await page.evaluate(() => window.RichTheme.scale()), 110, "the + control did not move the scale");
      assert(
        bigger.field.h > start.field.h,
        `the field did not follow the text size at all (${start.field.h}px -> ${bigger.field.h}px) — ` +
          "it is stuck at the height autoGrow() wrote before the change"
      );
      for (const id of Object.keys(bigger.controls)) {
        const c = bigger.controls[id];
        assert(
          holdsForControl(c),
          `LARGER: ${id} is ${c.box.h}px against the field's ${bigger.field.h}px (dh ${c.dHeight}, dc ${c.dCenter}) — ` +
            "neither centered nor the same height, which is the CEO's sentence broken"
        );
      }

      // AND BACK DOWN, because Ray measured the defect in both directions and the shrinking one
      // is the half where the FIELD is the taller of the two.
      const back = await step("#font-down");
      assertEqual(await page.evaluate(() => window.RichTheme.scale()), 100, "the − control did not move the scale back");
      assertEqual(
        back.field.h,
        start.field.h,
        `the field did not come back to its resting height (${start.field.h}px -> ${back.field.h}px)`
      );
      for (const id of Object.keys(back.controls)) {
        const c = back.controls[id];
        assert(
          holdsForControl(c),
          `SMALLER: ${id} is ${c.box.h}px against the field's ${back.field.h}px (dh ${c.dHeight}, dc ${c.dCenter})`
        );
      }
      assertEqual(page.__errors, [], "the page reported errors");
      await page.close();
      return (
        `100% field ${start.field.h}px; 110% field ${bigger.field.h}px, ${describeControls(bigger)}; ` +
        `back at 100% field ${back.field.h}px, ${describeControls(back)}`
      );
    }
  );

  await browser.close();
  // `report()` returns the FAILED COUNT, not a verdict. Same form as every other suite here.
  process.exitCode = run.report() > 0 ? 1 : 0;
}

main().catch((e) => {
  console.error(e && e.stack ? e.stack : e);
  process.exit(1);
});

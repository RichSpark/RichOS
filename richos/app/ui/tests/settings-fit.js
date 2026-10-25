// **THE SETTINGS PANEL FITS THE WINDOW THE APP RESTORES ITSELF TO** — candidate-.4 audit,
// defect 3.
//
// `docs/verification/2026-09-17-nightly-1.2.0-20260917.4-onscreen-audit.md` §4:
//
//   *"The window is 1024x700 at (664,108) — the app's own minimum, restored by the app. The
//    Settings panel measures: menu 'Settings' size 267,738 -> bottom edge at y=940; window
//    size 1024,700 -> bottom edge at y=808. 132 pt of the panel hangs below the window. It does
//    not scroll ... Below the edge are the technical reason, the update-server address, and the
//    'Bust a bug!' button — so from the default window a person cannot reach the control for
//    reporting a bug."*
//
// ## WHY THIS IS ITS OWN SUITE AND NOT A CHECK IN `appearance.js`
//
// `appearance.js` opens every one of its pages at **1400x950** and `ticker-wrap.js` at
// **1400x880**. The app's own minimum, and the size it restores itself to, is **1024x700**.
// Three of the candidate-.4 defects — the home picture, this panel, and the overlapped `CAPITAL`
// count — are all the same shape: something measured at a comfortable size and shipped to a
// smaller one. A suite that only ever opens a roomy window cannot see any of them.
//
// So the window size is the subject here, and every check states it.
//
// ## AND THE SURFACE IS THE HOME SCREEN, WHICH THIS HEADER SHOULD HAVE SAID FROM THE START
//
// `openMenu` sends the CURTAIN away and then clicks the button. It does not call `leaveHome`,
// so the surface in front of every check in this file is the home screen, not a thread. That
// has always been true here and cost nothing while the settings button had one position on
// every screen. It stopped costing nothing on 2026-09-19, when the CEO gave the button two —
// centered on the regular screens, back in the corner on the home screen and the held opening
// screen — at which point the anchor this suite pins is a number that only means something once
// the surface is named. It is named here, and again beside the assertion.
//
// Left ON the home screen deliberately, rather than made to leave it. Measured at 1024x700,
// dark, with the bound lifted so the panel reports its natural height:
//
//     home screen   12 rows, 620px natural, no theme row   against a 616px bound
//     a thread      13 rows, 658px natural, theme row      against a 628px bound
//
// §15 clamps this surface dark and `settings-button.js` OMITS the theme row rather than
// disabling it, which is the missing row. Both surfaces overflow their bound and both therefore
// scroll — this one by 4px, a thread by 30px — so this file is measuring the case it exists for
// on either, and it is measuring it on the surface the audited window was audited from. Moving
// it would be a change of subject, not an improvement.
//
// ## THE PAIR
//
// A negative control that reproduces Ray's geometry, and the positive control the brief asks
// for. The panel's natural height depends on state — 9 rows measure 523px, and his 738px is the
// same panel with the Updates block opened on an error — so the reproduction GROWS the panel to
// his measured 738px deterministically rather than hoping the fixture renders the same state.
//
// Run: node settings-fit.js   (or `npm test` for every suite in this directory)

"use strict";

const path = require("path");
const { loadPlaywright, createRun, assert, assertEqual, UI_DIR } = require("./lib/harness");

const APP = "file://" + path.join(UI_DIR, "index.html");

/// The window the app restores itself to, and its own minimum. Every check runs here.
const WINDOW = { width: 1024, height: 700 };

/// The roomy window `appearance.js` and the phone suite open. The live-code checks run at BOTH,
/// because a sheet that fits only the big one is the exact defect this file was created for.
const ROOMY = { width: 1400, height: 950 };

/// A Mac that is signed in to Tailscale, certified and named — the only Mac the shipped pairing
/// flow opens a window on. Copied from `phone.js`'s own fixture rather than invented, so the two
/// suites cannot be looking at different Macs.
const READY_TAILNET = {
  state: "ready",
  name: "mm1.tail9a3b2.ts.net",
  origin: "https://mm1.tail9a3b2.ts.net:8443",
  account: "Google as someone@gmail.com",
};

/// The pairing window pinned, so the sheet is in the LIVE-CODE state deterministically and the
/// countdown does not run out mid-check. 245 s = "4 more minutes" through `remaining()`.
const SECONDS_LEFT = 245;

/// The panel height Ray measured on the shipped candidate, in CSS pixels.
const AUDIT_PANEL_HEIGHT = 738;

/// Open the app, dismiss the curtain, and open the settings panel.
///
/// `preset` and `theme` are optional and exist for the live-code checks below: the mock's state
/// has to be in place BEFORE any of the page's own scripts run, and the theme has to be seeded in
/// both the mirror and the store because `syncAppearanceFromBackend` reconciles them and the
/// backend wins — the same two reasons `phone.js` and `contrast.js` give at length.
async function openMenu(browser, viewport, preset, theme) {
  const page = await browser.newPage({ viewport });
  const errors = [];
  page.on("pageerror", (e) => errors.push(String(e)));
  page.on("console", (m) => {
    if (m.type() === "error") errors.push("console: " + m.text());
  });
  page.__errors = errors;
  if (preset) await page.addInitScript((v) => { window.__RICHOS_MOCK_PRESET__ = v; }, preset);
  if (theme) {
    await page.addInitScript((t) => {
      try {
        window.localStorage.setItem("richos-theme", t);
        window.localStorage.setItem("richos-font-scale", "100");
        window.localStorage.setItem(
          "richos-mock-config",
          JSON.stringify({ theme: t, font_scale: 100, user_name: null })
        );
      } catch (e) {
        /* storage unavailable: theme-boot falls back to the shipped default, which is dark */
      }
    }, theme);
  }
  await page.goto(APP);
  await page.waitForSelector("#set-btn", { timeout: 15000 });
  await page.evaluate(() => window.RichSplash && window.RichSplash.yieldNow("settings-fit"));
  await page.click("#set-btn");
  await page.waitForSelector("#set-menu", { state: "visible" });
  // `.setmenu` rises over 0.2s; measuring mid-animation reads a translated box.
  await page.evaluate(async () => {
    const m = document.getElementById("set-menu");
    if (m) await Promise.all(m.getAnimations({ subtree: true }).map((a) => a.finished.catch(() => {})));
  });
  return page;
}

/// Grow the panel to a target height by adding one spacer, and report the geometry.
///
/// `bare` drops the bound and the scrolling, which is the shipped candidate's state — that is
/// how the negative control reproduces the defect rather than describing it.
const MEASURE = `(a) => {
  const menu = document.getElementById("set-menu");
  const bug = document.getElementById("bug-btn");
  const old = document.getElementById("zz-settings-fit-spacer");
  if (old) old.remove();
  // The shipped candidate also had no sticky floor (style.css "THE FLOOR STAYS IN VIEW", added
  // with the Token spend row): with the panel unbounded, a sticky Bust a bug would pin itself to
  // the window's bottom edge and the control would reproduce nothing, so bare drops it too.
  // Since round 24 the floor holds Bust a bug and Request a feature under it; since round 27 the
  // sticky box is the panel's foot band (.sm-foot), whose last cell is that floor.
  const floor = bug.closest(".sm-foot") || bug.closest(".set-floor") || bug;
  if (a.bare) { menu.style.maxHeight = "none"; menu.style.overflowY = "visible"; floor.style.position = "static"; }
  else { menu.style.maxHeight = ""; menu.style.overflowY = ""; floor.style.position = ""; }
  menu.scrollTop = 0;
  const natural = menu.getBoundingClientRect().height;
  if (a.grow) {
    // One spacer, inserted BEFORE the bug button's row so the button stays last exactly as
    // CEO ruling §15 requires, and sized so the panel lands on the audited height.
    const pad = Math.max(0, a.grow - natural);
    const s = document.createElement("div");
    s.id = "zz-settings-fit-spacer";
    s.style.height = pad + "px";
    menu.insertBefore(s, menu.lastElementChild);
  }
  menu.getBoundingClientRect();
  const r = menu.getBoundingClientRect();
  const br = bug.getBoundingClientRect();
  return {
    vh: window.innerHeight,
    top: Math.round(r.top),
    bottom: Math.round(r.bottom),
    height: Math.round(r.height),
    contentHeight: menu.scrollHeight,
    clientHeight: menu.clientHeight,
    scrollable: menu.scrollHeight > menu.clientHeight + 1,
    bugBottom: Math.round(br.bottom),
    bugInWindow: br.top >= 0 && br.bottom <= window.innerHeight,
    ideaInWindow: (() => { const ir = document.getElementById("idea-btn").getBoundingClientRect(); return ir.top >= 0 && ir.bottom <= window.innerHeight; })(),
    ideaUnderBug: document.getElementById("idea-btn").getBoundingClientRect().top >= br.bottom,
  };
}`;

const measure = (page, opts) =>
  page.evaluate((a) => eval("(" + a.fn + ")")(a), Object.assign({ fn: MEASURE }, opts));

/// Scroll the panel to its end the way a person would, then re-read the button.
const REACH = `() => {
  const menu = document.getElementById("set-menu");
  menu.scrollTop = menu.scrollHeight;
  menu.getBoundingClientRect();
  const br = document.getElementById("bug-btn").getBoundingClientRect();
  return {
    scrollTop: Math.round(menu.scrollTop),
    bugTop: Math.round(br.top),
    bugBottom: Math.round(br.bottom),
    bugInWindow: br.top >= 0 && br.bottom <= window.innerHeight,
  };
}`;

async function main() {
  const run = createRun("Settings fits the window the app restores itself to (1024x700)");
  const { webkit } = loadPlaywright();
  const browser = await webkit.launch();
  const page = await openMenu(browser, WINDOW);

  await run.check("the panel as it comes up at 1024x700 is inside the window", async () => {
    const m = await measure(page, {});
    assertEqual(m.vh, WINDOW.height, "the viewport is not the audited window");
    // **66, AND THIS SUITE MEASURES ON THE HOME SCREEN — which nothing said out loud until
    // 2026-09-19 and which is the reason this number moved twice in one day.** `openMenu` above
    // dismisses the CURTAIN and then clicks the button; it never calls `leaveHome`, so the
    // surface in front throughout every check in this file is the home screen. That was true
    // when this line was written and is not a change made here.
    //
    // WHY IT MATTERS NOW: since the CEO's afternoon ruling the settings button has two positions
    // rather than one — 6px on the regular screens, 18px on the home screen and the held
    // opening screen (`style.css`, `--settings-top`) — so "the anchor" is a question with two
    // answers and the surface has to be named before either is right. This panel hangs 8px
    // below the 40px button, so on THIS surface it is 18 + 40 + 8 = 66, and the bound is
    // `calc(100vh - (18px + 66px))` = `100vh - 84px`. Measured here: anchor 66, panel clamped to
    // 616px in a 700px window.
    //
    // IT WAS 66 UNTIL `dcfb87c9`, WENT TO 54 WITH THE MORNING'S CENTERING, AND IS 66 AGAIN.
    // The 54 was not wrong arithmetic; it was right arithmetic about a different surface, and it
    // passed here only because that commit moved the button on all three at once.
    //
    // THE NUMBER IS STILL PINNED RATHER THAN COMPUTED, for the reason it always was: a panel
    // that quietly stopped hanging from the button would still satisfy every fit check below it.
    // `chrome-align.js` owns the other half — that the button is where the CEO asked for it, on
    // each of the three surfaces — and pins the thread's 54 there, so nothing is lost by this
    // file naming only the surface it is actually on.
    assert(m.top === 66, `the panel's anchor moved to ${m.top}px; on the home screen the bound is derived from 66`);
    assert(m.bottom <= m.vh, `the panel's bottom edge is at ${m.bottom} in a ${m.vh}px window`);
    assert(m.bugInWindow, `"Bust a bug!" ends at ${m.bugBottom} in a ${m.vh}px window`);
    // Round 24: Request a feature directly under Bust a bug, in view with it.
    assert(m.ideaInWindow && m.ideaUnderBug, `"Request a feature" is not in view directly under "Bust a bug!": ${JSON.stringify(m)}`);
    return `panel ${m.height}px at top ${m.top}, bottom ${m.bottom} of ${m.vh}; Bust a bug ends at ${m.bugBottom}`;
  });

  await run.check("NEGATIVE CONTROL: at the audited 738px, an unbounded panel puts the button off-window", async () => {
    // Ray's geometry, reproduced: the same panel grown to the height he measured, with the
    // bound and the scrolling removed — which is exactly the shipped candidate. If this check
    // ever passes without failing the way he described, the check below proves nothing.
    const m = await measure(page, { grow: AUDIT_PANEL_HEIGHT, bare: true });
    // AT LEAST the audited height: since round 27 the one panel's own content is taller than
    // Ray's 738px at this width, so no spacer is needed to get there.
    assert(m.height >= AUDIT_PANEL_HEIGHT, `the panel did not reach the audited height: ${m.height}px`);
    assert(!m.scrollable, "the unbounded panel scrolls, so it is not the state that was audited");
    assert(
      !m.bugInWindow,
      `"Bust a bug!" is reachable even unbounded (ends at ${m.bugBottom} of ${m.vh}), so this reproduces nothing`
    );
    const over = m.bottom - m.vh;
    return `panel ${m.height}px, bottom ${m.bottom} of ${m.vh} — ${over}px below the edge, Bust a bug ends at ${m.bugBottom}`;
  });

  await run.check("POSITIVE CONTROL: bounded, the same 738px panel scrolls and the button is reachable", async () => {
    const m = await measure(page, { grow: AUDIT_PANEL_HEIGHT });
    assert(m.bottom <= m.vh, `the bounded panel still ends at ${m.bottom} in a ${m.vh}px window`);
    assert(
      m.scrollable,
      `the panel does not scroll (content ${m.contentHeight}px in ${m.clientHeight}px), so the rest of it is unreachable`
    );
    const after = await page.evaluate((fn) => eval("(" + fn + ")")(), REACH);
    assert(
      after.bugInWindow,
      `after scrolling to the end, "Bust a bug!" is at ${after.bugTop}..${after.bugBottom} in a ${m.vh}px window`
    );
    return (
      `panel clamped to ${m.height}px (content ${m.contentHeight}px), bottom ${m.bottom} of ${m.vh}; ` +
      `scrolled ${after.scrollTop}px and Bust a bug ends at ${after.bugBottom}`
    );
  });

  await run.check("the bound follows the window rather than a fixed number", async () => {
    // A `max-height` in px would pass every check above and still clip a shorter window. The
    // panel is grown past ANY plausible bound so the clamp is the thing being read.
    const page2 = await openMenu(browser, { width: 1024, height: 520 });
    const m = await measure(page2, { grow: 1200 });
    assert(m.bottom <= m.vh, `at a ${m.vh}px window the panel still ends at ${m.bottom}`);
    assert(m.scrollable, "the panel did not become scrollable at the smaller window");
    const after = await page2.evaluate((fn) => eval("(" + fn + ")")(), REACH);
    assert(after.bugInWindow, `"Bust a bug!" ends at ${after.bugBottom} in a ${m.vh}px window`);
    const errs = page2.__errors;
    await page2.close();
    assertEqual(errs, [], "the smaller window reported page errors");
    return `at 1024x520: panel clamped to ${m.height}px, bottom ${m.bottom} of ${m.vh}, Bust a bug ends at ${after.bugBottom}`;
  });

  // ---- G12: it opens where it was left, and it must not ----------------------------------

  await run.check("G12  the panel opens at its top, whatever scroll it was closed at", async () => {
    // Urban found this by being bitten by it rather than by looking for it: *"I toggled `Splash
    // screen` on by accident — the Settings panel reopens at its previous scroll position (G12)
    // and my click landed a row off."* A popover that remembers a scroll offset it never showed
    // the user is a popover whose rows are not where they were the last time they looked, and
    // this one genuinely scrolls at 1024x700 — the app's own minimum.
    const p = await openMenu(browser, WINDOW);
    // Grow it so it certainly scrolls in this window, scroll it to the end the way a person
    // would to reach `Bust a bug!`, then close and reopen from the same control he uses.
    await measure(p, { grow: AUDIT_PANEL_HEIGHT });
    const scrolledTo = await p.evaluate(() => {
      const m = document.getElementById("set-menu");
      m.scrollTop = m.scrollHeight;
      return Math.round(m.scrollTop);
    });
    await p.click("#set-btn");
    await p.waitForSelector("#set-menu", { state: "hidden" });
    await p.click("#set-btn");
    await p.waitForSelector("#set-menu", { state: "visible" });
    const reopened = await p.evaluate(async () => {
      const m = document.getElementById("set-menu");
      await Promise.all(m.getAnimations({ subtree: true }).map((a) => a.finished.catch(() => {})));
      const title = document.querySelector("#set-menu .setmenu-title");
      const r = m.getBoundingClientRect();
      const t = title.getBoundingClientRect();
      return { scrollTop: Math.round(m.scrollTop), titleOffset: Math.round(t.top - r.top) };
    });
    const errs = p.__errors;
    await p.close();
    assert(scrolledTo > 40, `the panel only scrolled ${scrolledTo}px, so reopening proves nothing`);
    assertEqual(reopened.scrollTop, 0, "THE DEFECT: the panel reopened at the scroll position it was closed at");
    // AND THE FIRST ROW IS ACTUALLY AT THE TOP, not merely `scrollTop === 0` over a panel whose
    // content moved: "Settings" is the first thing in it and it has to be within its padding.
    assert(
      reopened.titleOffset >= 0 && reopened.titleOffset < 24,
      `"Settings" is ${reopened.titleOffset}px from the panel's top edge on reopen`
    );
    assertEqual(errs, [], "the reopen reported page errors");
    return `closed at scrollTop ${scrolledTo}, reopened at ${reopened.scrollTop} with "Settings" ${reopened.titleOffset}px in`;
  });

  // ---- G10: a row you can press does not look like a heading -------------------------------

  await run.check("G10  the rows that open a sheet are told apart from the panel's two headings", async () => {
    // Urban's G10, his frame 10: `Connected repositories`, `Account connection`, `Memory folder`
    // and `Use Rich from your phone` were `1rem/600/--ink` — which is exactly what `Settings`
    // (`.setmenu-title`) and `Updates` (`.update-title`, taking its weight and ink from
    // `.set-name`) are. Four things a person can press, indistinguishable from two they cannot.
    //
    // The two headings are read off the panel rather than assumed, so a third heading added
    // later is covered by the same check instead of slipping past a hard-coded pair.
    const p = await openMenu(browser, WINDOW);
    const panel = await p.evaluate(() => {
      const menu = document.getElementById("set-menu");
      const headings = [...menu.querySelectorAll(".setmenu-title, .update-title")].map((h) => ({
        text: h.textContent.trim(),
        chevron: !!h.querySelector("svg"),
      }));
      const rows = [...menu.querySelectorAll("button.bugbtn")].map((b) => ({
        id: b.id,
        text: b.textContent.trim(),
        disclosure: b.classList.contains("bugbtn--disclosure"),
        icons: b.querySelectorAll("svg").length,
        // `aria-hidden` on the glyph: the row's own label is what a screen reader needs, and a
        // chevron announced as "chevron" is noise in a menu.
        hiddenFromReaders: [...b.querySelectorAll("svg")].every((s) => s.getAttribute("aria-hidden") === "true"),
      }));
      return { headings, rows };
    });
    const errs = p.__errors;
    await p.close();

    assert(panel.headings.length >= 2, "the panel no longer has the two headings this check is about: " +
      JSON.stringify(panel.headings));
    for (const h of panel.headings) {
      assert(!h.chevron, `the heading ${JSON.stringify(h.text)} carries a chevron, which makes it look pressable`);
    }
    const disclosures = panel.rows.filter((r) => r.disclosure);
    assertEqual(
      disclosures.map((r) => r.id).sort(),
      ["set-accounts-open", "set-instructions-open", "set-memory-open", "set-phone-open", "set-quota-open", "set-repositories-open"],
      "THE DEFECT: the rows that open a sheet are not marked as disclosures"
    );
    for (const r of disclosures) {
      assert(r.icons === 1, `${r.id} carries ${r.icons} glyphs; a disclosure has exactly one chevron`);
      assert(r.hiddenFromReaders, `${r.id}'s chevron is not aria-hidden`);
    }
    // AND `Bust a bug!` KEEPS ITS OWN ICON AND NO CHEVRON, which is now a distinction that
    // carries information: it is the one row here that is not a disclosure.
    const bug = panel.rows.find((r) => r.id === "bug-btn");
    assert(bug && !bug.disclosure && bug.icons === 1, "the bug row changed shape: " + JSON.stringify(bug));
    assertEqual(errs, [], "the panel reported page errors");
    return `${disclosures.length} disclosure rows with a chevron each, ${panel.headings.length} headings with none ` +
      `(${panel.headings.map((h) => JSON.stringify(h.text)).join(", ")}); "Bust a bug!" keeps its own icon`;
  });

  // ---- THE LIVE-CODE SHEET: the same defect one surface further in ------------------------
  //
  // Ray's candidate-.16 audit, "New defect: the live-code sheet's controls are below the fold,
  // and Tab cannot reach them"
  // (`docs/verification/2026-09-19-nightly-1.2.0-nightly.20260919.5-stale-engine-and-phone-audit.md`):
  //
  //     "At 1024x700 — this build's stated minimum window size — the pairing sheet shows the QR,
  //      the URL, the countdown and the six words, and no buttons at all. AX puts them at
  //      y=1175–1364 absolute, against a window whose bottom edge is y=792. `Show me another
  //      code`, `Stop and go back` and `Close` are all off-screen. The sheet's own copy meanwhile
  //      instructs the user to 'press Close and tell me' … On a sheet reopened while a code is
  //      live, focus starts on a bare container and three Tab presses never leave it."
  //
  // REPRODUCED HERE BEFORE IT WAS FIXED, at the same size, in this WebKit: panel clamped to 592px
  // in a 700px window with 1196px of content, and on a REOPENED sheet the three controls at window
  // y 1040..1116 with `document.activeElement` still BODY, outside the sheet. Same shape, same
  // magnitude, same two halves.
  //
  // THIS BELONGS IN THIS FILE AND NOT IN `phone.js` FOR THE REASON THE HEADER GIVES: the phone
  // suite opens every page at 1400x950, and a suite that only ever opens a roomy window cannot
  // see any defect of this class. Both sizes are checked below, and the roomy one is the control
  // — if the sheet fitted 1400x950 and nothing else, the small window's check would be the only
  // thing standing between this and the next audit.

  const LIVE_CODE = { phonePairing: true, phoneTailnet: READY_TAILNET, phonePairingSecondsLeft: SECONDS_LEFT };

  /// Drive the settings row into the pairing sheet, and hand back the page with it open.
  async function openPairingSheet(browser, viewport, theme) {
    const p = await openMenu(browser, viewport, LIVE_CODE, theme);
    await p.waitForSelector("#set-phone-open");
    await p.click("#set-phone-open");
    await p.waitForSelector("#phone-sheet", { state: "visible" });
    // The panel rises over 0.16s (`overlay-in`); measuring mid-animation reads a translated box.
    await p.evaluate(async () => {
      const panel = document.querySelector("#phone-sheet .overlay-panel");
      if (panel) await Promise.all(panel.getAnimations({ subtree: true }).map((a) => a.finished.catch(() => {})));
    });
    return p;
  }

  /// Every control on the sheet, and whether it is inside the window — plus what the panel is
  /// doing, so a failure says WHY rather than only that something is off screen.
  const SHEET = `() => {
    // TWO BOXES SINCE THE FIX, AND THEY ANSWER DIFFERENT QUESTIONS. The PANEL is the sheet's
    // outline — what "inside the window" is measured against. The SCROLL BOX is what actually
    // moves, and it is where contentHeight, clientHeight and scrollTop come from. Before Ray's
    // candidate-.16 defect they were one element; a check that kept reading the panel for both
    // would silently stop measuring the overflow.
    const panel = document.querySelector("#phone-sheet .overlay-panel");
    const scroller = document.getElementById("phone-scroll") || panel;
    const r = panel.getBoundingClientRect();
    const box = (id) => {
      const b = document.getElementById(id);
      const br = b.getBoundingClientRect();
      return {
        id,
        shown: b.offsetParent !== null,
        top: Math.round(br.top),
        bottom: Math.round(br.bottom),
        inWindow: br.top >= 0 && br.bottom <= window.innerHeight,
      };
    };
    const a = document.activeElement;
    return {
      vh: window.innerHeight,
      panelTop: Math.round(r.top),
      panelBottom: Math.round(r.bottom),
      contentHeight: scroller.scrollHeight,
      clientHeight: scroller.clientHeight,
      scrollTop: Math.round(scroller.scrollTop),
      // The six words are no longer beside the code (Sage's pairing review 3.1): they appear on the
      // card a phone reaches, beside They match. What must be on screen with a live code is the
      // code, its address, its countdown and the note that says where the words will appear.
      codeOnScreen: ["phone-qr-pair", "phone-pair-url", "phone-countdown", "phone-words-note"].every((id) => {
        const br = document.getElementById(id).getBoundingClientRect();
        return br.top >= 0 && br.bottom <= window.innerHeight && br.height > 0;
      }),
      controls: ["phone-refresh", "phone-pairing-back", "phone-close"].map(box),
      focus: a ? { tag: a.tagName, id: a.id, inSheet: !!(a.closest && a.closest("#phone-sheet")) } : null,
    };
  }`;

  const readSheet = (p) => p.evaluate((fn) => eval("(" + fn + ")")(), SHEET);

  for (const [label, viewport] of [["1024x700", WINDOW], ["1400x950", ROOMY]]) {
    for (const theme of ["dark", "light"]) {
      await run.check(
        `LIVE CODE  at ${label}, ${theme}: the code is on screen and so are all three controls`,
        async () => {
          const p = await openPairingSheet(browser, viewport, theme);
          const m = await readSheet(p);
          assertEqual(m.vh, viewport.height, "the viewport is not the window under test");
          // **TWO POSITIONS, AND THE SECOND ONE IS RAY'S FRAME.** The sheet scrolls itself to the
          // code on open (`scrollToCode`, Urban's G4), so reading it only where it happens to
          // land measures one scroll offset rather than the screen. `atTop` is the sheet at the
          // top of its own content — the QR, the address, the countdown and the six words, which
          // is exactly what Ray photographed with no buttons under it. The controls must be in
          // the window in BOTH.
          const atTop = await p.evaluate((fn) => {
            // The fallback is not defensive noise: it is what lets this check be run against the
            // SHIPPED tree, where the panel itself was the scroll box, and come back red with
            // Ray's own geometry instead of a TypeError.
            (document.getElementById("phone-scroll") ||
              document.querySelector("#phone-sheet .overlay-panel")).scrollTop = 0;
            return eval("(" + fn + ")")();
          }, SHEET);
          // THE PANEL GENUINELY OVERFLOWS, or this check is measuring a sheet that never had the
          // problem. At 1400x950 it still does — the how-to alone is 469px.
          assert(
            m.contentHeight > m.clientHeight + 1,
            `the sheet's content (${m.contentHeight}px) fits its ${m.clientHeight}px scrollport, so ` +
              `a pinned footer proves nothing at this size`
          );
          // THE CONTROLS FIRST, because they are the defect. Asserting the code's visibility
          // ahead of them made the pre-fix run fail on the wrong line — the shipped sheet
          // scrolls itself to the code and pushes the six words half out, which is a real but
          // much smaller thing, and it hid the three buttons sitting 340px below the window.
          for (const [where, s] of [["as it opens", m], ["at the top of the sheet", atTop]]) {
            for (const c of s.controls) {
              assert(c.shown, `${c.id} is not rendered on the pairing screen at all`);
              assert(
                c.inWindow,
                `THE DEFECT: ${where}, ${c.id} is at y ${c.top}..${c.bottom} in a ${s.vh}px window ` +
                  `(panel ${s.panelTop}..${s.panelBottom}, ${s.contentHeight}px of content, scrolled ${s.scrollTop})`
              );
            }
          }
          // Ray's own list of what he COULD see, kept as the thing that must not be traded away:
          // the code is the reason a person is on this screen, and a footer that won its space by
          // pushing the QR off the top would be this defect solved in the wrong direction. Read
          // at the top of the sheet, which is where the code lives in the markup's order.
          assert(atTop.codeOnScreen, "at the top of the sheet the QR, the address, the countdown or the six words is outside the window");
          const errs = p.__errors;
          await p.close();
          assertEqual(errs, [], "the sheet reported page errors");
          return (
            `panel ${m.panelTop}..${m.panelBottom} of ${m.vh} with ${m.contentHeight}px of content; ` +
            `at the top of the sheet the code is on screen and the controls are at y ` +
            atTop.controls.map((c) => `${c.id} ${c.top}..${c.bottom}`).join(", ") +
            ` (opened at scrollTop ${m.scrollTop})`
          );
        }
      );
    }
  }

  await run.check(
    "LIVE CODE  a sheet REOPENED while a code is live opens with focus on its primary control",
    async () => {
      // The sharper half of the defect. Ray: "On a sheet reopened while a code is live, focus
      // starts on a bare container and three Tab presses never leave it. Page Down does nothing
      // either." So a keyboard-only user cannot cancel a live pairing code.
      //
      // THE CAUSE IS ONE SELECTOR: `open()` focused `sheet.querySelector("button:not([disabled])")`,
      // which answers a question about DOM ORDER. `#phone-forget` is the first button in the
      // markup, it lives inside `#phone-paired`, that block is hidden on every screen but the
      // paired card — and `.focus()` on a hidden element does nothing at all, silently. Focus
      // stayed on `document.body`, which is also why Page Down did nothing: the body is not the
      // box that scrolls.
      //
      // ASSERTED ON FOCUS, NOT ON TAB. WebKit does not move Tab focus to buttons unless full
      // keyboard access is on, so a Tab-traversal assertion here would measure the harness's
      // preferences rather than the app. Where focus LANDS is the app's own decision and is
      // deterministic; the Tab walk is checked on the real app on screen.
      const p = await openPairingSheet(browser, WINDOW, "dark");
      const fresh = await readSheet(p);
      assert(fresh.focus && fresh.focus.inSheet, `a FRESHLY opened sheet put focus on ${JSON.stringify(fresh.focus)}`);

      await p.click("#phone-close");
      await p.waitForSelector("#phone-sheet", { state: "hidden" });
      await p.click("#set-btn");
      await p.waitForSelector("#set-phone-open");
      await p.click("#set-phone-open");
      await p.waitForSelector("#phone-sheet", { state: "visible" });
      await p.waitForTimeout(250);
      const m = await readSheet(p);
      const errs = p.__errors;
      await p.close();

      assert(
        m.focus && m.focus.inSheet,
        `THE DEFECT: the reopened sheet put focus on ${JSON.stringify(m.focus)} — outside the sheet, ` +
          `so no key press reaches it`
      );
      assertEqual(m.focus.tag, "BUTTON", "focus landed on something that is not a control");
      // The PRIMARY control, which on this screen is "Show me another code". Close would be
      // reachable too, but the first thing a keyboard lands on should be the thing the screen
      // is for.
      assertEqual(m.focus.id, "phone-refresh", "focus did not land on the screen's primary control");
      for (const c of m.controls) {
        assert(c.inWindow, `on the reopened sheet ${c.id} is at y ${c.top}..${c.bottom} of ${m.vh}`);
      }
      assertEqual(errs, [], "the reopen reported page errors");
      return (
        `fresh open focused ${fresh.focus.id || fresh.focus.tag}; reopened with a live code focused ` +
        `${m.focus.id}, all three controls inside the ${m.vh}px window`
      );
    }
  );

  // ---- D2: the row says whether a code is live, or a phone is paired ----------------------

  await run.check("D2  the phone row says a code is live, and for how long, in both themes", async () => {
    // Ray: "With port 8443 open and code 56WS37AH live, the Settings panel reads `Use Rich from
    // your phone >`. Nothing else. Byte-for-byte the same row as with no code at all, and the
    // same again while a phone is paired."
    const seen = [];
    for (const theme of ["dark", "light"]) {
      const p = await openMenu(browser, WINDOW, LIVE_CODE, theme);
      await p.waitForSelector("#set-phone-open");
      await p.waitForFunction(
        () => {
          const n = document.getElementById("set-phone-state");
          return n && n.textContent.trim().length > 0;
        },
        null,
        { timeout: 4000 }
      ).catch(() => {});
      const m = await p.evaluate(() => {
        const n = document.getElementById("set-phone-state");
        const row = document.getElementById("set-phone-open");
        const cs = n ? getComputedStyle(n) : null;
        return {
          line: n ? n.textContent.trim() : null,
          px: cs ? Math.round(parseFloat(cs.fontSize)) : null,
          theme: document.documentElement.getAttribute("data-theme") || document.body.getAttribute("data-theme"),
          rowInWindow: (() => {
            const r = row.getBoundingClientRect();
            return r.top >= 0 && r.bottom <= window.innerHeight;
          })(),
        };
      });
      const errs = p.__errors;
      await p.close();
      assert(m.line, `THE DEFECT: with a code live the row still says nothing (${theme})`);
      // The minutes, not just "something is happening": a user who walked away needs to know
      // whether the code he left is still worth going back to.
      assert(
        /\b4 more minutes\b/.test(m.line),
        `the row does not say how long is left (${theme}): ${JSON.stringify(m.line)}`
      );
      // §15's floor for text meant to be read. 14px is the "skippable" tier and this is not that.
      assert(m.px >= 16, `the state line is ${m.px}px in ${theme}; §15's floor for readable text is 16px`);
      assert(m.rowInWindow, `the phone row itself left the window in ${theme}`);
      assertEqual(errs, [], `the menu reported page errors (${theme})`);
      seen.push(`${theme}: ${JSON.stringify(m.line)} at ${m.px}px`);
    }
    return seen.join("; ");
  });

  await run.check("D2  the phone row names the paired phone, and says nothing when there is nothing to say", async () => {
    // The other two states, and the third is the control: a row that ALWAYS carries a line would
    // pass the check above while telling a user with no phone and no code that something is going
    // on. "Not pairing" is not a state worth a line.
    const p1 = await openMenu(browser, WINDOW, { phonePaired: true, phoneTailnet: READY_TAILNET }, "dark");
    await p1.waitForSelector("#set-phone-open");
    await p1.waitForFunction(
      () => {
        const n = document.getElementById("set-phone-state");
        return n && n.textContent.trim().length > 0;
      },
      null,
      { timeout: 4000 }
    ).catch(() => {});
    const paired = await p1.evaluate(() => {
      const n = document.getElementById("set-phone-state");
      return n ? n.textContent.trim() : null;
    });
    const e1 = p1.__errors;
    await p1.close();

    const p2 = await openMenu(browser, WINDOW, { phoneTailnet: READY_TAILNET }, "dark");
    await p2.waitForSelector("#set-phone-open");
    await p2.waitForTimeout(500);
    const quiet = await p2.evaluate(() => {
      const n = document.getElementById("set-phone-state");
      return { text: n ? n.textContent.trim() : null, hidden: n ? n.hidden : null };
    });
    const e2 = p2.__errors;
    await p2.close();

    assert(paired && /iPhone/.test(paired), `THE DEFECT: the row does not name the paired phone: ${JSON.stringify(paired)}`);
    assertEqual(quiet.text, "", "the row invents a state when nothing is paired and no code is live");
    assertEqual(quiet.hidden, true, "the empty state line is still in the accessible name of the row");
    assertEqual(e1, [], "the paired menu reported page errors");
    assertEqual(e2, [], "the quiet menu reported page errors");
    return `paired: ${JSON.stringify(paired)}; nothing paired and no code: the line is absent`;
  });

  await run.check("no page errors", async () => {
    assertEqual(page.__errors, [], "the page reported errors");
    return "0 errors";
  });

  await page.close();
  await browser.close();
  // `report()` returns the FAILED COUNT, not a verdict. Same form as every other suite here.
  process.exitCode = run.report() > 0 ? 1 : 0;
}

main().catch((e) => {
  console.error(e && e.stack ? e.stack : e);
  process.exit(1);
});

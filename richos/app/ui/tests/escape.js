// ESCAPE CLOSES EVERY POPUP — the CEO's rule, enforced against the real window.
//
// CEO, 2026-09-17, item 1 of his v1.0.2 list, verbatim:
//
//     "The user must always be able to close any popup of any kind by simply tapping the
//      escape key on the keyboard i.e. without having to click anything"
//
// WHAT HE HAD INSTEAD, measured on richos 7088a57f before this suite existed: `main.js`
// handled Escape for eight named surfaces, and the window ships more than eight. The gear's
// preferences popover — `#assertiveness-popover`, the most-used popup in the product — was
// not one of them. `#setup-sheet`, `#memory-setup`, `#set-menu`,
// `#repositories-sheet` and `#home-prefs` were not either; three of those carried their own
// Escape listener bound to the popup ELEMENT, so each worked only while focus happened to be
// inside it, and `#home-prefs` on a copy with no companies focused nothing at all and so
// could not be closed from the keyboard under any circumstances.
//
// SO THIS SUITE DOES NOT CARRY A LIST. Every check below derives its surfaces from
// `window.RichDismiss.selector` — the same query the shipped handler enumerates with — so a
// popup written next month is under these checks the day it is written, with nothing for its
// author to remember. Part A refuses a structurally-a-popup element that declares no
// dismissal, and its positive control plants one to prove the refusal fires.
//
// TWO VOCABULARY WORDS, both on the element, both the product's:
//
//   data-dismiss="escape"          Escape closes it outright, through the surface's own
//                                  close function.
//   data-dismiss="control:#a,#b"   Escape does exactly what the first of those controls that
//                                  is on screen and enabled does — and NOTHING when none of
//                                  them is. That is how `#setup-sheet` keeps 704b4596's
//                                  invariant (it cannot be dismissed into a dead app
//                                  mid-install) while still answering the CEO's rule the rest
//                                  of the time: Escape is the named way out, reached by the
//                                  keyboard, never a second quieter one.
//
// Cases:
//   A1  every popup in the booted window declares how it is dismissed
//   A2  positive control — an undeclared popup makes A1 red
//   B1  every declared surface answers Escape, with focus OUTSIDE it   [derived]
//   B2  Settings opened from the nameplate (round 27's second way in), by the real path
//   B3  the settings menu, by the real path
//   B4  topmost first, measured against the stacking rather than a chosen order
//   B5  an inline permission request leaves Escape and the composer available
//   B6  the home screen's own dialog, on a copy with no companies
//   B7  Escape at the home screen cannot answer a surface painted BEHIND it   [derived]
//   B8  the machinery itself: a named control is not pressed on a popup nobody can see
//   B9  the Output panel: a file back to its list, the list to closed, a popup above it first
//   C1  Escape with nothing open moves nothing and steals no key
//   C2  this suite actually checked something
//
// Run: node escape.js   (or `npm test` for every suite in this directory)

"use strict";

const path = require("path");
const { loadPlaywright, createRun, assert, assertEqual, UI_DIR, leaveHome } = require("./lib/harness");

const APP = "file://" + path.join(UI_DIR, "index.html");

async function openApp(browser, preset) {
  const page = await browser.newPage({ viewport: { width: 1400, height: 950 } });
  const errors = [];
  page.on("pageerror", (e) => errors.push(String(e)));
  page.on("console", (m) => {
    if (m.type() === "error") errors.push("console: " + m.text());
  });
  if (preset) {
    await page.addInitScript((v) => {
      window.__RICHOS_MOCK_PRESET__ = v;
    }, preset);
  }
  await page.goto(APP);
  await leaveHome(page);
  await page.waitForFunction(() => typeof window.RichDismiss === "object");
  page.__errors = errors;
  return page;
}

/// Put the hand somewhere that is NOT inside the popup under test. This is the condition the
/// three element-bound Escape listeners failed under, so every check in part B starts here.
async function focusTheComposer(page) {
  await page.evaluate(() => {
    const input = document.getElementById("input");
    if (input) input.focus();
    else document.body.focus();
  });
}

async function main() {
  const run = createRun("Escape closes every popup of every kind (CEO 2026-09-17, item 1)");
  const { webkit } = loadPlaywright();
  const browser = await webkit.launch();
  let assertions = 0;
  const bump = (n) => {
    assertions += n;
    return n;
  };

  // =======================================================================================
  // A — THE DECLARATION. A popup that says nothing about how it closes is a defect.
  // =======================================================================================

  await run.check("A1  every popup in the booted window declares how it is dismissed", async () => {
    const page = await openApp(browser);
    // Everything the boot can create, created: the settings menu is built lazily by the file
    // that owns it, and a surface that does not exist yet cannot be scanned.
    await page.evaluate(() => {
      if (window.RichSettings && window.RichSettings.openMenu) window.RichSettings.openMenu();
    });
    const undeclared = await page.evaluate(() => {
      window.RichDismiss.open(); // stamps the external declarations, changes nothing on screen
      const out = [];
      for (const node of document.querySelectorAll(window.RichDismiss.selector)) {
        if (!node.closest("[data-dismiss]")) {
          out.push(node.id || node.tagName.toLowerCase() + "." + node.className);
        }
      }
      return out;
    });
    assertEqual(
      undeclared,
      [],
      "these are popups by structure and declare no `data-dismiss`, so Escape can only fall " +
        "back to hiding them with none of their own cleanup"
    );
    const declared = await page.evaluate(() =>
      [...document.querySelectorAll("[data-dismiss]")].map((n) => n.id).sort()
    );
    // The permission request is now an inline card, so its former popup declaration
    // is intentionally absent, and round 27 retired the gear's Preferences popover into the
    // one Settings panel. The other eleven historical surfaces remain popups.
    assert(
      declared.length >= 11,
      "only " + declared.length + " declared surfaces: " + declared.join(", ")
    );
    // ...and the ones the old eight-line handler never covered are among them, by name,
    // because those are the popups the CEO actually hit.
    for (const id of [
      "setup-sheet",
      "memory-setup",
      "set-menu",
      "repositories-sheet",
    ]) {
      assert(declared.indexOf(id) >= 0, "#" + id + " is still outside the enumeration");
    }
    bump(8);
    assert(page.__errors.length === 0, "the shell logged errors: " + page.__errors.join(" | "));
    await page.close();
    return declared.length + " surfaces, every one of them declared: " + declared.join(", ");
  });

  await run.check("A2  positive control — an undeclared popup makes A1 red", async () => {
    const page = await openApp(browser);
    // A1's own scan, run against a window with a popup somebody added and did not declare.
    // If this comes back empty, A1 cannot fail and is decoration.
    const found = await page.evaluate(() => {
      const planted = document.createElement("div");
      planted.id = "planted-popup";
      planted.className = "overlay";
      planted.setAttribute("role", "dialog");
      document.body.appendChild(planted);
      window.RichDismiss.open();
      return [...document.querySelectorAll(window.RichDismiss.selector)]
        .filter((n) => !n.closest("[data-dismiss]"))
        .map((n) => n.id);
    });
    assertEqual(found, ["planted-popup"], "the declaration scan did not see an undeclared popup");
    // AND IT STILL CLOSES. The scan is what makes the omission visible; the CEO's rule has no
    // "unless somebody forgot" in it, so the handler's blunt fallback has to carry the day.
    await focusTheComposer(page);
    await page.keyboard.press("Escape");
    assert(await page.isHidden("#planted-popup"), "an undeclared popup survived Escape");
    bump(2);
    await page.close();
    return "an undeclared popup is reported by A1's scan, and still closes";
  });

  // =======================================================================================
  // B — WHAT IT DOES, from a hand that is not already inside the popup
  // =======================================================================================

  await run.check("B1  every declared surface answers Escape, with focus outside it", async () => {
    const page = await openApp(browser);
    const surfaces = await page.evaluate(() =>
      [...document.querySelectorAll("[data-dismiss]")].map((n) => ({
        id: n.id,
        spec: n.getAttribute("data-dismiss"),
      }))
    );
    assert(surfaces.length >= 11, "only " + surfaces.length + " surfaces were derived");
    const report = [];
    for (const surface of surfaces) {
      // OPENED BY UN-HIDING IT, deliberately. Every surface has its own entrance and several
      // need a backend state to reach; what is under test is the handler's reaction to a
      // popup being ON SCREEN, which is the one thing they all share. The real entrances are
      // B2, B3, B5 and B6, and `setup.js` case 14.
      await page.evaluate((s) => {
        const node = document.getElementById(s.id);
        node.hidden = false;
        window.__escapeClicked = null;
        if (s.spec.indexOf("control:") === 0) {
          for (const sel of s.spec.slice("control:".length).split(",")) {
            const control = document.querySelector(sel.trim());
            if (!control) continue;
            control.hidden = false;
            control.disabled = false;
            control.addEventListener(
              "click",
              () => {
                window.__escapeClicked = control.id;
              },
              { once: true }
            );
            break;
          }
        }
      }, surface);
      await focusTheComposer(page);
      await page.keyboard.press("Escape");
      if (surface.spec.indexOf("control:") === 0) {
        // The contract for these is not "it went away" — it is "Escape did what the named
        // control does". Asserting the click is what makes that exact rather than similar.
        const clicked = await page.evaluate(() => window.__escapeClicked);
        assert(
          clicked !== null,
          "#" + surface.id + " declares " + surface.spec + " and Escape pressed none of them"
        );
        report.push(surface.id + "→" + clicked);
      } else {
        assert(
          await page.isHidden("#" + surface.id),
          "#" + surface.id + " survived Escape from outside it"
        );
        report.push(surface.id);
      }
      bump(1);
      // Leave the window closed whatever the surface did, so the next one is measured alone.
      await page.evaluate((id) => {
        document.getElementById(id).hidden = true;
      }, surface.id);
    }
    assert(page.__errors.length === 0, "the shell logged errors: " + page.__errors.join(" | "));
    await page.close();
    return surfaces.length + " derived surfaces, all closed by Escape: " + report.join(", ");
  });

  await run.check("B2  Settings opened from the nameplate (round 27's second way in), by the real path", async () => {
    // Round 27 retired the gear's Preferences popover: the unset nameplate at the foot of the
    // rail opens the ONE Settings panel on the name field. Escape closes it from there too.
    const page = await openApp(browser);
    await page.waitForSelector("#rail-identity.is-unset");
    await page.click("#rail-identity");
    await page.waitForSelector("#set-menu:not([hidden])");
    assertEqual(
      await page.evaluate(() => document.activeElement && document.activeElement.id),
      "user-name-input",
      "the nameplate opened Settings on the name field"
    );
    await focusTheComposer(page);
    await page.keyboard.press("Escape");
    assert(await page.isHidden("#set-menu"), "Settings opened from the nameplate survived Escape");
    // The button must not go on announcing a panel that is not there.
    assertEqual(
      await page.getAttribute("#set-btn", "aria-expanded"),
      "false",
      "the settings button still reports itself expanded"
    );
    bump(2);
    assert(page.__errors.length === 0, "the shell logged errors: " + page.__errors.join(" | "));
    await page.close();
    return "opened from the nameplate, closed by Escape, and the button says so";
  });

  await run.check("B3  the settings menu, by the real path", async () => {
    const page = await openApp(browser);
    await page.click("#set-btn");
    await page.waitForSelector("#set-menu:not([hidden])");
    await focusTheComposer(page);
    await page.keyboard.press("Escape");
    assert(await page.isHidden("#set-menu"), "the settings menu survived Escape");
    assertEqual(
      await page.getAttribute("#set-btn", "aria-expanded"),
      "false",
      "the settings button still reports itself expanded"
    );
    bump(2);
    assert(page.__errors.length === 0, "the shell logged errors: " + page.__errors.join(" | "));
    await page.close();
    return "closed once, and the button's state came with it";
  });

  await run.check("B4  topmost first, measured against the stacking", async () => {
    const page = await openApp(browser);
    // `#inspector` is z-index 20 and `#search-overlay` is 60, so the overlay is painted over
    // the panel and must be the one Escape takes. The order is READ from the stylesheet here
    // rather than asserted from the handler's source, so a restyle that reverses them fails.
    const stack = await page.evaluate(() => {
      document.getElementById("inspector").hidden = false;
      document.getElementById("search-overlay").hidden = false;
      const z = (id) =>
        Number.parseInt(window.getComputedStyle(document.getElementById(id)).zIndex, 10) || 0;
      return { inspector: z("inspector"), search: z("search-overlay") };
    });
    assert(
      stack.search > stack.inspector,
      "this check assumes the overlay is painted above the panel: " + JSON.stringify(stack)
    );
    await focusTheComposer(page);
    await page.keyboard.press("Escape");
    assert(await page.isHidden("#search-overlay"), "the topmost surface was not the one closed");
    assert(
      await page.isVisible("#inspector"),
      "one Escape closed two surfaces — a keypress must undo one thing"
    );
    await focusTheComposer(page);
    await page.keyboard.press("Escape");
    assert(await page.isHidden("#inspector"), "the second Escape did not reach the panel below");
    bump(4);
    await page.close();
    return "z-index " + stack.search + " before " + stack.inspector + ", one surface per press";
  });

  await run.check("B6  the home screen's own dialog, on a copy with no companies", async () => {
    const page = await openApp(browser);
    await page.evaluate(() => window.RichSettings.openMenu());
    await page.click("#set-home-open");
    await page.waitForSelector("#home-prefs:not([hidden])");
    // `home.js` focuses the first company row, which does not exist on the paint that opens
    // this sheet — so its own wrapper-bound Escape listener never got a keystroke at all.
    await focusTheComposer(page);
    await page.keyboard.press("Escape");
    await page.waitForSelector("#home-prefs", { state: "hidden" });
    assert(await page.isHidden("#home-prefs"), "the home dialog survived Escape");
    // ITS SCRIM WENT WITH IT. Hiding the inner panel alone would leave the dimmed sheet over
    // the whole window with nothing on it, which is worse than not closing at all.
    assert(
      await page.isHidden(".home-prefs-scrim"),
      "the panel closed and left its scrim over the window"
    );
    bump(2);
    await page.close();
    return "closed, scrim and all, on the copy where its own listener could never fire";
  });

  // =======================================================================================
  // B7 — A KEY PRESSED AT A PICTURE DOES NOT ANSWER WHAT IS BEHIND THE PICTURE
  //
  // `isOnScreen()` tests `hidden`, `display` and `visibility` and — `splash.js`'s own words —
  // "has no idea what is painted over what". Any full-screen surface therefore leaves a popup
  // underneath it answerable by a key the CEO is pressing AT THE SURFACE. The curtain is the
  // instance that shipped (`setup.js` case 19, `5bd7d4e5`): one Escape took it down AND pressed
  // "Not now" on an engine offer that had never been on screen.
  //
  // THE HOME SCREEN IS THE SAME SHAPE — `#home`, z-index 150, over `.overlay`'s 60 — and had
  // no such guard. `home.js` now consumes Escape while it holds the keyboard, unless what is
  // open is painted ABOVE it.
  //
  // WHAT THIS IS AND IS NOT A FIX FOR, so the record does not overclaim. Measured on
  // `4f2d57c9`: no product path reaches home-up-with-a-sheet-behind-it. Every dialog in the
  // window is an `.overlay`, `giveWayToOpenDeskSheet` hands the screen over to any one that
  // opens, and `show()` gives way again in the SAME task when the logo is pressed with one
  // already up. So this is a FLOOR under that give-way, not the fix for what Ray saw at entry
  // on candidate .16 — that was `main.js`'s boot focus and is fixed at `main.js:7721`.
  //
  // WHICH IS EXACTLY WHY IT IS PLANTED. A2 already established planting as this suite's way of
  // proving a rule that today's surfaces do not happen to exercise; a floor nothing steps on
  // is still a floor, and the alternative is a guard with no check on it at all. The negative
  // control is the other half and is the more important one: a popup painted ABOVE the home
  // screen still answers Escape, so this consumes a key at a picture and nothing else.
  // =======================================================================================

  await run.check("B7  Escape at the home screen cannot answer a surface painted behind it", async () => {
    const page = await browser.newPage({ viewport: { width: 1400, height: 950 } });
    const errors = [];
    page.on("pageerror", (e) => errors.push(String(e)));
    page.on("console", (m) => {
      if (m.type() === "error") errors.push("console: " + m.text());
    });
    await page.goto(APP);
    // The curtain only — its own Escape guard is case 19's and would mask this one. The home
    // screen stays, because the home screen IS the surface under test.
    await page.waitForFunction("typeof window.RichSplash === 'object'", null, { timeout: 30000 });
    await page.evaluate(() => window.RichSplash.yieldNow("acceptance-suite"));
    await page.waitForSelector(".splash", { state: "detached", timeout: 10000 });
    await page.waitForFunction("typeof window.RichDismiss === 'object'", null, { timeout: 30000 });

    // NOT `.overlay`, DELIBERATELY: that class is what `home.js`'s give-way watches, so an
    // `.overlay` would take the screen away and there would be nothing painted over anything.
    // These are popups by the same structure `POPUP_SELECTOR` recognizes — `role="dialog"`
    // with a declaration — sitting at `.overlay`'s own 60 and at `#home-prefs`'s own 340.
    const planted = await page.evaluate(() => {
      const make = (id, z) => {
        const n = document.createElement("div");
        n.id = id;
        n.setAttribute("role", "dialog");
        n.setAttribute("data-dismiss", "escape");
        n.style.cssText = `position:fixed;inset:0;z-index:${z}`;
        document.body.appendChild(n);
        return n;
      };
      make("planted-behind", 60);
      make("planted-above", 400);
      return {
        homeOpen: !!(window.RichHome && window.RichHome.isOpen()),
        homeZ: getComputedStyle(document.getElementById("home")).zIndex,
        open: window.RichDismiss.open().map((n) => n.id + "@" + getComputedStyle(n).zIndex),
        focus: (document.activeElement && document.activeElement.id) || null,
      };
    });
    // Both halves of the premise, asserted rather than assumed: the screen is up, and both
    // planted surfaces are in the enumeration the shipped handler dismisses from.
    assert(planted.homeOpen, "the home screen is not up, so this check is not in the window it is about");
    assertEqual(planted.homeZ, "150", "the home screen no longer sits at the z-index this check reasons about");
    assert(
      planted.open.indexOf("planted-above@400") === 0 && planted.open.indexOf("planted-behind@60") > 0,
      "the two planted surfaces are not enumerated topmost-first as expected: " + planted.open.join(", ")
    );

    // ONE Escape, with the topmost being the one painted ABOVE the screen. It must go — the
    // CEO's rule has no exception for "while the home screen is up", and this is the half that
    // keeps the guard a floor instead of a wall.
    await page.keyboard.press("Escape");
    // The effect is the fact waited for (hang guard), not a 200ms sleep (2026-09-29, audit R10);
    // the assertion below still names a popup that never went.
    await page.waitForSelector("#planted-above", { state: "hidden", timeout: 10000 }).catch(() => {});
    assert(
      await page.isHidden("#planted-above"),
      "a popup painted ABOVE the home screen stopped answering Escape — the guard is a wall, not a floor"
    );
    assert(
      await page.evaluate(() => !!(window.RichHome && window.RichHome.isOpen())),
      "Escape took the home screen down on its way past"
    );

    // ...and now the only thing left is BEHIND the picture. This is the key a person presses at
    // a full-screen composition, and it must not answer a question he has never seen.
    await page.keyboard.press("Escape");
    // An ABSENCE of effect: give the shell two of its own frames to act (it renders at most once
    // per frame), rather than 200ms of this process's time.
    await page.evaluate(() => new Promise((r) => requestAnimationFrame(() => requestAnimationFrame(r))));
    assert(
      await page.isVisible("#planted-behind"),
      "one Escape at the home screen answered a surface painted behind it — he declined a question he was never shown"
    );
    assert(
      await page.evaluate(() => !!(window.RichHome && window.RichHome.isOpen())),
      "Escape took the home screen down instead"
    );

    // THE REAL SURFACE THE FIRST DERIVATION BROKE, by its real path. `#set-menu` carries NO
    // z-index of its own — measured `set-menu@auto` inside `settings@300` — so the obvious
    // "computed z-index greater than the screen's 150" read the most-used menu in the product
    // as behind the picture and ate the Escape that closes it. `affordances.js` PART 6 caught
    // it; it is pinned HERE too, because that is the file a future author of this guard reads.
    await page.evaluate(() => window.RichSettings.openMenu());
    await page.waitForSelector("#set-menu:not([hidden])");
    const menuStacking = await page.evaluate(() => ({
      menuZ: getComputedStyle(document.getElementById("set-menu")).zIndex,
      wrapperZ: getComputedStyle(document.querySelector(".settings")).zIndex,
    }));
    assertEqual(
      menuStacking.menuZ,
      "auto",
      "#set-menu now carries its own z-index, so this check no longer covers the case it was written for"
    );
    await page.keyboard.press("Escape");
    await page.waitForSelector("#set-menu", { state: "hidden", timeout: 10000 }).catch(() => {});
    assert(
      await page.isHidden("#set-menu"),
      `the settings menu stopped answering Escape at the home screen — it is painted at ${menuStacking.wrapperZ} by its wrapper and carries ${menuStacking.menuZ} itself, which is the trap`
    );

    // AND THE GUARD IS SCOPED TO THIS SCREEN. Once the screen is gone the surface behind it
    // was never behind anything, and Escape is Escape again.
    await leaveHome(page);
    await page.keyboard.press("Escape");
    await page.waitForSelector("#planted-behind", { state: "hidden", timeout: 10000 }).catch(() => {});
    assert(
      await page.isHidden("#planted-behind"),
      "the guard outlived the screen it belongs to — a surface that is covered by nothing is still refusing Escape"
    );
    bump(9);
    assert(errors.length === 0, "the shell logged errors: " + errors.join(" | "));
    await page.close();
    return "above the screen it closes (planted, and the real #set-menu at z-index auto), behind the screen it is untouched, and the guard leaves with the screen";
  });

  // =======================================================================================
  // B8 — THE DISMISSAL MACHINERY ITSELF, NOT A THIRD SURFACE'S GUARD
  //
  // THE SAME DEFECT, FOR THE THIRD TIME, AND THIS TIME AT ITS SOURCE. `isOnScreen()` tests
  // `hidden`, `display` and `visibility` and — its own words — "has no idea what is painted
  // over what". Two instances were closed AT THEIR OWN SURFACE: the opening curtain
  // (`splash.js`, `setup.js` case 19, `5bd7d4e5`) and the home screen (`home.js`, B7 above,
  // `617b4c9e`). Both are guards a surface has to REMEMBER to carry, so the next full-screen
  // composition somebody writes arrives without one and the defect is back.
  //
  // What is measured here is `dismissTopmostPopup` itself: with a popup covered, its declared
  // control is NOT pressed, whatever the covering thing is and whether or not that thing
  // thought to guard Escape. The cover planted below is deliberately NOT a popup, carries no
  // listener and consumes nothing — it is the surface written next month, with nothing
  // remembered.
  //
  // THE SURFACE UNDER IT IS THE ONE THIS COST THE CEO: `#setup-sheet`, declared
  // `control:#setup-later,#setup-close,#setup-start` (round 19, dictation plan slice 4, added
  // Start: once Claude Code and the engine are in, Start replaces Not now and Close as the
  // sheet's only way out, so Escape must be able to reach it there). On `missing-engine`
  // Start is hidden and the first control on screen is still "Not now". "Pressed a control
  // nobody could see" is not an
  // abstraction here — it is "Not now" on the one step that puts an engine on a Mac, answered
  // by a hand that was pointed at something else entirely.
  //
  // AND IT NAMES ITS OWN BLIND SPOT, because the instrument has one. The question is asked of
  // the compositor — `elementFromPoint` at the popup's own center, the derivation B7 settled
  // on — and that is a HIT TEST, not a paint. A cover with `pointer-events: none` is invisible
  // to it. The opening curtain is exactly that (`.splash` in `splash.css:22`), which is why
  // `splash.js` keeps its own guard and why this is a FLOOR under the two surface guards
  // rather than a replacement for either. The measurement is in the return line.
  // =======================================================================================

  await run.check("B8  Escape does not press a named control on a popup nobody can see", async () => {
    const page = await openApp(browser, { setup: "missing-engine" });
    await page.waitForSelector("#setup-sheet:not([hidden])", { timeout: 10000 });

    // THE PREMISE, ASSERTED: the offer is up, it declares a control rather than `escape`, and
    // the two surfaces that carry their own guard are out of the way — so what answers this
    // key can only be the machinery.
    const before = await page.evaluate(() => ({
      declared: document.getElementById("setup-sheet").getAttribute("data-dismiss"),
      home: !!(window.RichHome && window.RichHome.isOpen()),
      curtain: !!document.querySelector(".splash"),
      top: (window.RichDismiss.open()[0] || {}).id || null,
    }));
    assertEqual(
      before.declared,
      "control:#setup-later,#setup-close,#setup-start",
      "the offer no longer names its way out, so this check is measuring something else"
    );
    assert(!before.home && !before.curtain, "a surface with its own Escape guard is still up");
    assertEqual(before.top, "setup-sheet", "the offer is not the topmost popup");

    // The surface written next month: full-screen, opaque, ordinary. Not a popup, no
    // `data-dismiss`, no listener — nothing for its author to have remembered.
    const covered = await page.evaluate(() => {
      const cover = document.createElement("div");
      cover.id = "planted-cover";
      cover.style.cssText = "position:fixed;inset:0;z-index:500;background:#101010";
      document.body.appendChild(cover);
      const box = document.getElementById("setup-sheet").getBoundingClientRect();
      const hit = document.elementFromPoint(box.left + box.width / 2, box.top + box.height / 2);
      return { hit: hit ? hit.id || hit.className : null };
    });
    assertEqual(
      covered.hit,
      "planted-cover",
      "the cover is not painted over the offer, so this check is not in the window it is about"
    );

    await page.keyboard.press("Escape");
    await page.evaluate(() => new Promise((r) => requestAnimationFrame(() => requestAnimationFrame(r))));
    assert(
      await page.evaluate(() => !document.getElementById("setup-sheet").hidden),
      "Escape pressed the offer's named way out through an opaque cover — he declined the one " +
        "step that puts an engine on this Mac without ever seeing the question"
    );
    // THE OTHER HALF, and it is the half that keeps this a floor rather than a wall: uncover
    // it and Escape is the named button again, immediately, with no reload and no second key.
    await page.evaluate(() => document.getElementById("planted-cover").remove());
    await page.keyboard.press("Escape");
    await page.waitForFunction(() => document.getElementById("setup-sheet").hidden, null, { timeout: 10000 }).catch(() => {});
    assert(
      await page.evaluate(() => document.getElementById("setup-sheet").hidden),
      "with nothing over it, Escape stopped pressing the offer's named way out"
    );

    // THE BLIND SPOT, MEASURED RATHER THAN CLAIMED. A `pointer-events: none` cover is not
    // reported by `elementFromPoint`, so this floor cannot see the curtain and `splash.js`'s
    // own guard is load-bearing. Asserting it here is what stops a later author deleting that
    // guard on the belief that this one covers it.
    const ghost = await page.evaluate(() => {
      const el = document.getElementById("setup-sheet");
      el.hidden = false;
      const cover = document.createElement("div");
      cover.style.cssText =
        "position:fixed;inset:0;z-index:900;background:#101010;pointer-events:none";
      cover.id = "planted-ghost";
      document.body.appendChild(cover);
      const box = el.getBoundingClientRect();
      const hit = document.elementFromPoint(box.left + box.width / 2, box.top + box.height / 2);
      const answer = hit ? hit.id || hit.className : null;
      cover.remove();
      el.hidden = true;
      return answer;
    });
    assert(
      ghost !== "planted-ghost",
      "a pointer-events:none cover is now reported by elementFromPoint — the blind spot this " +
        "check documents is gone, and splash.js's own guard can be re-argued"
    );

    bump(6);
    assert(page.__errors.length === 0, "the shell logged errors: " + page.__errors.join(" | "));
    await page.close();
    return (
      "covered: elementFromPoint at the offer's center answers planted-cover and Escape presses " +
      "nothing; uncovered: Escape presses the named Not now; and a pointer-events:none cover " +
      'answers "' + ghost + '" — the blind spot splash.js still guards'
    );
  });

  await run.check("B9  the Output panel steps back one level per Escape, under anything above it", async () => {
    // Output side panel PRD §6.8: "Esc steps back: a menu, then the file view to the list (row
    // kept selected), then the panel." The panel is a declared surface (`data-dismiss="escape"`)
    // so B1 already closes it from outside; this is the layer B1 cannot see — the panel's own
    // step from a file back to its list — and the order against a popup painted above it.
    const page = await openApp(browser, { output: "round-17" });
    await page.click('.nav-thread[data-thread-id="acme"]');
    await page.waitForFunction(() => window.RichOutput.snapshot().count === 9);
    await page.click("#out-top");
    await page.waitForSelector("#outpanel .orow");
    await page.click("#outpanel .orow");
    await page.waitForSelector("#of-back");
    // A popup ABOVE the panel: the thread's own menu, opened by its real control (its `⋯`).
    await page.evaluate(() => {
      const row = document.querySelector('.nav-thread[data-thread-id="acme"]').closest(".nav-thread-row");
      row.querySelector(".nav-thread-more").click();
    });
    await page.waitForSelector("#thread-menu:not([hidden])");
    const read = () =>
      page.evaluate(() => ({
        menu: !document.getElementById("thread-menu").hidden,
        panel: !document.getElementById("outpanel").hidden,
        view: document.getElementById("op-body").dataset.view,
        selected: document.querySelector(".orow.is-open") ? document.querySelector(".orow.is-open").dataset.output : null,
      }));
    const steps = [];
    await focusTheComposer(page);
    await page.keyboard.press("Escape");
    let s = await read();
    assert(!s.menu && s.panel && s.view === "file", "the first Escape did not take the menu above the panel first: " + JSON.stringify(s));
    steps.push("menu");
    await page.keyboard.press("Escape");
    s = await read();
    assert(s.panel && s.view === "list", "Escape from a file did not step back to the list: " + JSON.stringify(s));
    assert(s.selected, "the file's row was not kept selected");
    steps.push("file→list");
    await page.keyboard.press("Escape");
    s = await read();
    assert(!s.panel, "Escape from the list did not close the panel");
    steps.push("list→closed");
    bump(4);
    assert(page.__errors.length === 0, "the shell logged errors: " + page.__errors.join(" | "));
    await page.close();
    return steps.join(", ");
  });

  // =======================================================================================
  // C — AND IT TAKES NOTHING IT WAS NOT GIVEN
  // =======================================================================================

  await run.check("C1  Escape with nothing open moves nothing and steals no key", async () => {
    const page = await openApp(browser);
    await page.fill("#input", "book the Acme call for Thursday");
    await focusTheComposer(page);
    const before = await page.evaluate(() => window.RichDismiss.open().length);
    assertEqual(before, 0, "something was already on screen, so this proves nothing");
    await page.keyboard.press("Escape");
    assertEqual(
      await page.inputValue("#input"),
      "book the Acme call for Thursday",
      "Escape with nothing open touched what he was typing"
    );
    assertEqual(
      await page.evaluate(() => document.activeElement && document.activeElement.id),
      "input",
      "Escape with nothing open moved the hand"
    );
    bump(3);
    assert(page.__errors.length === 0, "the shell logged errors: " + page.__errors.join(" | "));
    await page.close();
    return "nothing open, nothing moved";
  });

  await run.check("C2  this suite actually checked something", async () => {
    assert(
      assertions >= 25,
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
      : "\nevery popup in the window closes on Escape, and the next one will too."
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
// A1   index.html: delete `data-dismiss="escape"` from #techy-scope
//        -> "these are popups by structure and declare no `data-dismiss`" names it
// A2   main.js `dismissTopmostPopup`: delete the `top.hidden = true` fallback
//        -> an undeclared popup survives Escape, which is the CEO's rule failing quietly
// B1   main.js: restore the eight-line `if (!someEl.hidden) return closeSomething()` chain
//        -> #setup-sheet, #memory-setup, #set-menu
//           and #repositories-sheet all survive Escape
// B2   main.js `POPUP_CLOSERS`: drop the "set-menu" entry
//        -> the panel is hidden by the blunt fallback and the button keeps aria-expanded=true
// B4   main.js `openPopups`: sort ascending instead of descending
//        -> one Escape reaches the panel underneath and leaves the overlay on top of it
//        -> Escape silently answers a request while the user is composing unrelated words
// B6   main.js: empty `EXTERNAL_DISMISS`
//        -> #home-prefs-panel is hidden and its scrim is left over the whole window
// B8   main.js `dismissTopmostPopup`: drop the `isPainted(top)` guard
//        -> Escape presses "Not now" on an engine offer under an opaque cover, which is the
//           shipped behavior at 5f3a1a1e for any full-screen surface that has not been given
//           a guard of its own
// B8b  main.js `isPainted`: return `hit === node` without `node.contains(hit)`
//        -> a popup whose center lands on its own panel reads as covered, and Escape stops
//           closing every sheet in the window
// B9   output-panel.js `escape`: call `close()` unconditionally
//        -> "Escape from a file did not step back to the list"
// C1   main.js: make `dismissTopmostPopup` return true unconditionally
//        -> Escape starts eating keystrokes with nothing on screen

// THE FRONT DOOR, MEASURED WHERE THE OTHER SUITES COULD NOT SEE — audit-9 rows 1 and 3.
//
// ## WHY THIS FILE EXISTS: A GREEN HARNESS OVER A WINDOW THAT DID NOT DO IT
//
// The `ui1` slice (`2e377176`) closed audit-8 rows 3, 4, 5, 10, 11 and 13, every one of them
// against a green suite in this directory. `2e377176` IS in the candidate-.9 bundle — verified,
// not assumed:
//
//     $ git merge-base --is-ancestor 2e377176 794eac7f && echo IN     ->  IN
//     $ /usr/libexec/PlistBuddy -c Print .../RichOS.app/Contents/Info.plist | grep Commit
//         RichOSSourceCommit = 794eac7f3dfbab75567f3aa6cf723da289a7ea99
//
// and Ray then walked that bundle and reported two of those rows still open
// (`docs/verification/2026-09-18-nightly-1.2.0-nightly.20260918.3-onscreen-audit.md` §4). So the
// suites were measuring something the window does not do. The two reasons, one per row, are the
// reason this file is a file rather than three more checks bolted onto `home.js`:
//
// ### ROW 1 — THERE IS NO ACCESSIBILITY TREE ANYWHERE IN THIS HARNESS
//
// Ray reads the NATIVE tree (`System Events` -> `AXUIElement`). Nothing in this directory ever
// has. `control-names.js` says so in its own header — *"Playwright 1.61 ships no accessibility
// snapshot API, so this suite computes the name from the DOM"* — and that is exactly true;
// re-measured on the installed 1.61.1:
//
//     $ node -e "... const p = await b.newPage(); console.log(typeof p.accessibility)"
//     undefined
//
// So every accessibility claim in this directory has been a claim about the DOM. The DOM said
// `#home-door-cap` is `aria-hidden="true"` and that is what the native tree reported too, and
// the DOM had nothing at all to say about the node SITTING NEXT TO IT, which is the defect.
//
// `locator.ariaSnapshot()` is the nearest thing 1.61 does ship, and its visibility rule is the
// one that matters here — measured rather than read off the documentation:
//
//     <div style="opacity:0"><p>faded text</p></div><button>hi</button><div hidden><p>x</p></div>
//       ->  - paragraph: faded text
//           - button "hi"
//
// An `opacity: 0` subtree is IN, a `[hidden]` subtree is OUT. That is precisely the distinction
// `#home-loading.gone` falls the wrong side of, so this is the tool for row 1. It is an ARIA
// tree and not the platform's, and it is declared as one: what it cannot tell you is how
// VoiceOver renders the result. What it CAN tell you is the thing that was wrong.
//
// ### ROW 3 — `home.js` HAS NEVER READ THE LAUNCH KIND, AND `home.js`'s OWN SUITE PINNED THAT
//
// `home.js` check "a launch with NO curtain at all lands on the home screen" asserts
// `r.homeOpen && !r.homeHidden` and its comment says, in as many words, *"It is also what every
// reload, every crash-restart and every second window gets"*. A Dock restore IS a second
// window (`main.rs::reopen_window`), so the suite was holding the product to the behavior Ray
// then reported as a defect. A check cannot catch what it asserts.
//
// The native half of the front door — a real Escape through AppKit into a real WKWebView — is
// NOT here and cannot be: this harness injects keys into the page through the automation
// protocol, which is a path the shipped window does not have. That half is
// `scripts/front-door.test.sh`.
//
// ## CASES
//
//   A1  the faded loading layer is OUT of the accessibility tree once the picture is up
//   A2  positive control — while it is still up, its words ARE in the tree
//   A3  the door itself is a named control, and there is exactly one of it
//   B1  a second window does NOT land on the opening screen
//   B2  positive control — fresh, reload and crash-restart still land on it
//   B3  ...and the way back is intact: the logo brings the home screen up on a second window
//   C1  this suite actually checked something
//
// Run: node front-door.js   (or `npm test` for every suite in this directory)

"use strict";

const path = require("path");
const { loadPlaywright, createRun, assert, assertEqual, UI_DIR, leaveHome } = require("./lib/harness");

const APP = "file://" + path.join(UI_DIR, "index.html");

/// The string the loading layer holds, read off the product rather than typed here twice.
const WAKING = "waking loro…";

async function openApp(browser, launchKind) {
  const page = await browser.newPage({ viewport: { width: 1400, height: 950 } });
  const errors = [];
  page.on("pageerror", (e) => errors.push(String(e)));
  page.on("console", (m) => {
    if (m.type() === "error") errors.push("console: " + m.text());
  });
  if (launchKind) {
    // The shell injects this before any page script — `main.rs::launch_init_script`, which is
    // `window.__RICHOS_LAUNCH__ = Object.freeze({ kind: …, ordinal: …, splashEnabled: … })`.
    await page.addInitScript((k) => {
      window.__RICHOS_LAUNCH__ = Object.freeze({ kind: k, ordinal: null, splashEnabled: null });
    }, launchKind);
  }
  await page.goto(APP);
  await page.waitForFunction(() => typeof window.RichHome === "object", null, { timeout: 30000 });
  page.__errors = errors;
  return page;
}

/// The picture, driven the product's own way and waited out to `live`. `startField` is exported
/// for exactly this (`home.js`'s own suite drives it the same way) because the shipped path is
/// an idle callback that may never come in a headless run.
async function fieldLive(page) {
  await page.evaluate(() => window.RichHome.startField());
  await page.waitForFunction(() => window.RichHome.state.field === "live", null, { timeout: 60000 });
  // `field-engine.js:1442` adds `.gone` when it starts its first frame; the fade is 800ms and
  // the end state is what is under test, so this waits for the end state and not for a timer.
  await page.waitForFunction(
    () => {
      const n = document.getElementById("home-loading");
      return !!n && n.classList.contains("gone");
    }, null,
    { timeout: 10000 }
  );
  await page.waitForFunction(
    () => {
      const n = document.getElementById("home-loading");
      // `visibility: hidden` flips at the END of the 0.8s fade (home.css `#home-loading.gone`,
      // `visibility 0s linear 0.8s`), a separate step from opacity reaching 0; under load a
      // snapshot taken between the two still sees the layer (nightly 40 gate, A1).
      return !n || (getComputedStyle(n).opacity === "0" && getComputedStyle(n).visibility === "hidden");
    }, null,
    { timeout: 5000 }
  );
}

async function main() {
  const run = createRun("The front door, from the accessibility tree and from the launch kind (audit-9 rows 1, 3)");
  const { webkit } = loadPlaywright();
  const browser = await webkit.launch();
  let assertions = 0;
  const bump = (n) => {
    assertions += n;
    return n;
  };

  // =======================================================================================
  // A — WHAT A SCREEN READER IS TOLD IS ON THIS SCREEN
  // =======================================================================================

  await run.check("A1  the faded loading layer is out of the accessibility tree once the picture is up", async () => {
    const page = await openApp(browser);
    await fieldLive(page);
    const tree = await page.locator("#home").ariaSnapshot();
    // WHAT WENT WRONG, in Ray's words: "The node occupying that slot is `static text waking
    // loro…` — a string that is NOT on screen." Measured on the .9 window at 13:11Z:
    //   AXButton    | name=Talk to Rich
    //   AXStaticText| name=waking loro…
    assert(
      tree.indexOf(WAKING) === -1,
      "the loading layer has faded out and is still being announced — a screen reader arriving " +
        "here is told the app is \"" + WAKING + "\" while the screen says nothing of the kind.\n" +
        "          the layer, measured: " +
        JSON.stringify(
          await page.evaluate(() => {
            const n = document.getElementById("home-loading");
            const s = getComputedStyle(n);
            return { cls: n.className, display: s.display, visibility: s.visibility, opacity: s.opacity };
          })
        )
    );
    // AND IT IS GONE FROM THE HIT TEST TOO, which is the other half of a layer that is not
    // there: `pointer-events: none` stops it taking a click and leaves it findable.
    assertEqual(
      await page.evaluate(() => getComputedStyle(document.getElementById("home-loading")).visibility),
      "hidden",
      "the layer is still laid out and still visible to everything except the eye"
    );
    bump(2);
    assertEqual(page.__errors, [], "the shell logged errors");
    await page.close();
    return "the picture is up and \"" + WAKING + "\" is in neither the tree nor the layout";
  });

  await run.check("A2  positive control — while the layer is still up, its words ARE in the tree", async () => {
    const page = await openApp(browser);
    // Before the field has been driven, `#home-loading` is the surface — it covers the whole
    // composition on purpose. If A1's tool cannot see it HERE, A1 cannot fail and is decoration.
    const tree = await page.locator("#home").ariaSnapshot();
    assert(
      tree.indexOf(WAKING) >= 0,
      "the loading layer's own words are not in the tree while it is the thing on screen, so " +
        "A1 proves nothing: " + tree.slice(0, 400)
    );
    bump(1);
    await page.close();
    return "the tree carries \"" + WAKING + "\" exactly while the layer is the screen";
  });

  await run.check("A3  the door is a named control, and there is exactly one of it", async () => {
    const page = await openApp(browser);
    await fieldLive(page);
    const tree = await page.locator("#home").ariaSnapshot();
    // THE DOOR. `home.js` carries the ruling: the label is "Talk to Rich" and the word "Enter"
    // under it is DECORATION to assistive technology (`aria-hidden="true"` on `#home-door-cap`,
    // landed in `7ed03c68`), because the fact it renders — the Enter key opens this — is in the
    // tree as `aria-keyshortcuts` on the door itself. So the tree must carry the button and
    // must NOT carry a second control called Enter. Ray's audit-9 row 1 reports the second half
    // as a defect; it is the design, and this check is where that is written down so the next
    // walk does not re-open it.
    const doors = tree.split("\n").filter((l) => /button "Talk to Rich"/.test(l));
    assertEqual(doors.length, 1, "the tree carries " + doors.length + " doors: " + doors.join(" / "));
    assert(
      !/button "Enter"|link "Enter"/.test(tree),
      "a second control named Enter is in the tree — two controls with one name is the thing a " +
        "screen reader cannot tell apart"
    );
    assertEqual(
      await page.getAttribute("#home-enter", "aria-keyshortcuts"),
      "Enter",
      "the door no longer says the key works, so the word under it became a claim nothing backs"
    );
    assertEqual(
      await page.getAttribute("#home-door-cap", "aria-hidden"),
      "true",
      "the caption is announced as well as rendered"
    );
    bump(4);
    await page.close();
    return "one door, named, carrying aria-keyshortcuts=Enter, with the caption as decoration";
  });

  // =======================================================================================
  // B — WHICH SURFACE A LAUNCH LANDS ON, AND WHOSE DECISION THAT IS
  // =======================================================================================

  await run.check("B1  a second window does NOT land on the opening screen", async () => {
    const page = await openApp(browser, "second-window");
    // THE CONTRACT IS THE SHELL'S AND IT IS ALREADY WRITTEN DOWN. `main.rs::reopen_window`:
    // "It is a `SecondWindow` launch: nothing begins, no opening screen, no count" — and a Dock
    // restore takes that path, because closing the window DESTROYS it (`ExitRequested` prevents
    // the process exit, not the window's destruction), so `app.webview_windows()` is empty and
    // a NEW window is built with `LaunchKind::SecondWindow`. The webview boots from scratch.
    //
    // Ray, .9 §4: "On restoring the window from the Dock — with a job running, he is put back
    // on the splash rather than on the conversation he left, and nothing on that screen says
    // work is in progress."
    const r = await page.evaluate(() => ({
      kind: window.__RICHOS_LAUNCH__ && window.__RICHOS_LAUNCH__.kind,
      homeOpen: window.RichHome.state.open,
      homeHidden: document.getElementById("home").hidden,
      bodyOpen: document.body.classList.contains("home-open"),
      appInert: document.getElementById("app").hasAttribute("inert"),
      forcedDark: window.RichTheme.forcedDark(),
      splashDrew: window.RichSplash.state.shown,
    }));
    assertEqual(r.kind, "second-window", "the launch kind never reached the page");
    assert(!r.homeOpen, "the home screen opened on a window that is coming back to work already running");
    assert(r.homeHidden, "#home is not hidden, so it is over the conversation he left");
    assert(!r.bodyOpen, "the body still carries `home-open`, which is what covers the shell");
    assert(!r.appInert, "#app is inert, so the conversation he came back to cannot be touched");
    assert(!r.forcedDark, "§15's always-dark clamp is raised by a screen that is not up");
    assert(!r.splashDrew, "the curtain drew on a second window");
    bump(7);
    assertEqual(page.__errors, [], "the shell logged errors");
    await page.close();
    return "second window: no opening screen, no clamp, no curtain, the desk live";
  });

  await run.check("B2  positive control — fresh, reload and crash-restart still land on the opening screen", async () => {
    // THE CHANGE IS SCOPED TO ONE KIND, and this is what says so. The CEO ruled on 2026-09-01
    // that the home screen "must be shown in the app after the splash screen"; a second window
    // is not a launch, it is a window coming back on a run that never stopped. If this check
    // ever goes red the fix removed the screen rather than scoping it.
    const report = [];
    for (const kind of ["fresh", "reload", "crash-restart"]) {
      const page = await openApp(browser, kind);
      const r = await page.evaluate(() => ({
        homeOpen: window.RichHome.state.open,
        homeHidden: document.getElementById("home").hidden,
        appInert: document.getElementById("app").hasAttribute("inert"),
      }));
      assert(r.homeOpen && !r.homeHidden, kind + " did not land on the opening screen");
      assert(r.appInert, kind + " left the desk live behind the opening screen");
      report.push(kind);
      bump(2);
      await page.close();
    }
    return report.join(", ") + " all land on the opening screen, as ruled";
  });

  await run.check("B3  the way back is intact on a second window — the logo brings it up", async () => {
    const page = await openApp(browser, "second-window");
    // NOT SHOWN IS NOT NOT BUILT. "a click on the logo (in the upper left corner) brings the
    // user back to the home screen" (CEO, 2026-09-01), and that has to keep working on the
    // window that did not open on it — otherwise the fix for B1 took the screen away.
    await page.evaluate(() => window.RichHome.show("front-door-suite"));
    await page.waitForFunction(() => !document.getElementById("home").hidden, null, { timeout: 30000 });
    const r = await page.evaluate(() => {
      const door = document.getElementById("home-enter");
      const box = door && door.getBoundingClientRect();
      return {
        homeOpen: window.RichHome.state.open,
        forcedDark: window.RichTheme.forcedDark(),
        appInert: document.getElementById("app").hasAttribute("inert"),
        // A screen that was built while hidden must not come up mis-measured.
        doorWidth: box ? Math.round(box.width) : 0,
        doorHeight: box ? Math.round(box.height) : 0,
      };
    });
    assert(r.homeOpen, "the logo did not bring the home screen back");
    assert(r.forcedDark, "§15's clamp was not raised when the screen came up");
    assert(r.appInert, "the desk is live behind the screen that is now in front of it");
    assert(r.doorWidth > 40 && r.doorHeight > 10, "the door came up with no size: " + JSON.stringify(r));
    bump(4);
    assertEqual(page.__errors, [], "the shell logged errors");
    await page.close();
    return "the logo opens it, dark, with the desk inert and the door " + r.doorWidth + "x" + r.doorHeight;
  });

  await run.check("C1  this suite actually checked something", async () => {
    assert(
      assertions >= 18,
      "only " + assertions + " assertions ran. A suite that verifies little and reports green is " +
        "the failure this repository has caught three times."
    );
    return assertions + " assertions against the real renderer under WebKit";
  });

  await browser.close();
  const failed = run.report();
  console.log(
    failed
      ? "\n" + failed + " check(s) FAILED"
      : "\nthe front door: one named control, nothing announced that is not there, and a window " +
          "coming back lands where he left it."
  );
  process.exitCode = failed ? 1 : 0;
}

main().catch((e) => {
  console.error(e);
  process.exit(1);
});

// ---------------------------------------------------------------------------------------
// RUN RED — the mutation that makes each check fail, applied to the SHIPPED source
// ---------------------------------------------------------------------------------------
//
// A1   home.css `#home-loading.gone`: drop `visibility: hidden`
//        -> "the loading layer has faded out and is still being announced", and this is the
//           EXACT state of the .9 bundle: measured `{display:"flex", visibility:"visible",
//           opacity:"0"}` in the renderer, and `AXStaticText | name=waking loro…` in the .9
//           window's own native tree at 14:56Z on 2026-09-18
// A2   home.js `buildSwitch`/`build`: give `#home-loading` `aria-hidden="true"` outright
//        -> A2 goes red, which is the point: A1 would then pass for the wrong reason
// A3   home.js: take `aria-hidden="true"` off `#home-door-cap`
//        -> the caption is announced again, which is audit-8 row 3's original complaint
// B1   home.js `start()`: drop the `second-window` branch
//        -> "the home screen opened on a window that is coming back to work already running",
//           which is the .9 behavior Ray walked
// B2   home.js `start()`: decline on every kind instead of `second-window`
//        -> fresh/reload/crash-restart stop landing on the opening screen, against the ruling
// B3   home.js `show()`: drop the re-measure
//        -> the door comes up with no size, because it was laid out while hidden

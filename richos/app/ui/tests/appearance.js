// TWO LIGHTINGS, ONE TYPE KNOB, AND THE PERSON AT THE FOOT OF THE RAIL.
//
// CEO rulings §14 and §15 (`richos-hq/wiki/ceo-decisions.md`), plus his correction to round
// 10.1. Five things are asserted here that nothing else in this directory can assert,
// because each of them is a claim about the SHIPPING shell rather than about a module:
//
//   1. SYSTEM IS THE DEFAULT (§63, 2026-09-19) — "the app should detect the user's system
//      preference for dark or light theme and system should be the default for a freshly
//      installed app" — AND IT IS THE DEFAULT AGAINST BOTH A LIGHT AND A DARK OPERATING
//      SYSTEM. That second clause is the whole check, and it is the clause this suite got
//      wrong for the three weeks it asserted the opposite: an app that resolved DARK by
//      default looks perfectly correct on a dark OS, passes every walk taken on one, and
//      hands a light-OS user a dark app. One OS setting can only ever half-test a rule
//      about following the OS, whichever half it is. Both walks, or neither proves
//      anything. Two more checks stand beside it, because §63 is two sentences and a
//      default is only the first: 1b is the OS flipping WHILE THE APP IS OPEN, and 1c is
//      "only if the user switches ... should the app remember that" — remembered across a
//      relaunch, and System reachable again afterwards.
//
//      §15's "a newly installed app opens dark" is not repealed. It moved: the splash and
//      the home screen are clamped dark and no switch reaches them, which is checks 4, 4b
//      and 5.
//   2. THE PREFERENCE IS DURABLE, and config.rs wins any disagreement with the pre-paint
//      mirror. The mirror exists only because an async read cannot decide the first frame;
//      the moment it can also DECIDE something, there are two places the answer lives.
//   3. THE OPENING SCREEN IS ALWAYS DARK AND THE BUTTON IS ON IT ANYWAY. §15's one permanent
//      exception and its floor, together, because they are only meaningful together: the
//      switch is the part that can be absent, and "Bust a bug" never is.
//   4. ONE STATE, TWO ENTRANCES, for the font size and for Techy Mode. A settings row and a
//      keyboard shortcut that agree most of the time are two states, and the failure is
//      silent.
//   5. THE FOOT OF THE RAIL IS HIS. Initials and a name — and when there is no name, an
//      honest empty state rather than an invented one or a "??".
//
// WHAT THIS SUITE DELIBERATELY DOES NOT CLAIM. It runs in WebKit under Playwright, not in
// the Tauri shell, so it cannot prove that macOS itself leaves ⌘+ alone. What it CAN prove
// is the two halves that are actually ours: that the page handler calls `preventDefault`
// (check 8), and that `zoomHotkeysEnabled` is `false` in the shipped `tauri.conf.json`
// (check 9) — which is the flag that would otherwise inject a webview zoom polyfill on
// macOS and Linux and set WebView2's `IsZoomControlEnabled` on Windows. The remaining gap —
// a native menu item binding the same accelerator — is stated in the handoff rather than
// papered over. There is no menu in this app today and check 9 is what would notice one
// arriving with a zoom item attached.
//
// EVERY CHECK HERE WAS RUN RED ONCE by breaking the shipped source; the mutations are listed
// against their check numbers at the bottom of this file.
//
// Run: node appearance.js   (or `npm test` for every suite in this directory)
//      RICHOS_PLAYWRIGHT=/path/to/node_modules/playwright node appearance.js

"use strict";

const fs = require("fs");
const path = require("path");
const {
  leaveHome,
  loadPlaywright,
  shot,
  createRun,
  assert,
  assertEqual,
  awaitSettled,
  flushFrames,
  bootSettled,
  openThread,
  settleOnThread,
  leaveSplash,
  HOLD_CURTAIN,
  assertCurtainHeld,
  paintedGround,
  UI_DIR,
} = require("./lib/harness");
const SOURCES = require("./lib/ui-sources");
const OVERLAYS = require("./lib/overlays");
const { collectPageErrors, createErrorTracker } = require("./lib/page-errors");

const APP = "file://" + path.join(UI_DIR, "index.html");
const SHOTS = "../shots-10-1";
const STYLE_CSS = fs.readFileSync(path.join(UI_DIR, "style.css"), "utf8");
const INDEX_HTML = fs.readFileSync(path.join(UI_DIR, "index.html"), "utf8");
const TAURI_CONF = JSON.parse(
  fs.readFileSync(path.resolve(UI_DIR, "..", "src-tauri", "tauri.conf.json"), "utf8")
);
const CONFIG_RS = fs.readFileSync(
  path.resolve(UI_DIR, "..", "crates", "richos-core", "src", "config.rs"),
  "utf8"
);
const MAIN_JS = fs.readFileSync(path.join(UI_DIR, "main.js"), "utf8");

/// A DELIBERATE SLOW RUNNER, on demand: `RICHOS_APPEARANCE_LAG_MS=120 node appearance.js`.
///
/// THE SEAM MOVED, and the number that is useful moved with it. This knob used to be one
/// `waitForTimeout(LAG_MS)` after `goto` — a stand-in for a `macos-latest` runner reaching the
/// first assertion about 2,000 ms later than this machine does, which is why the useful value
/// was around 3,000. That reproduces LATENESS and nothing else: every check is delayed by the
/// same amount, the app and the driver stay in exactly the order they were in, and the failure
/// this directory actually has is an ORDERING failure.
///
/// It now sits inside `RichBridge.invoke` (`INSTRUMENT_BRIDGE` below), which is the seam
/// `contrast.js`'s knob already used and the one that decides things: the driver keeps its own
/// speed while every answer the app is waiting for arrives late, so a check that was relying on
/// a round trip finishing inside a sleep fails. A useful value here is now tens or low hundreds
/// of milliseconds rather than thousands. Zero, and no latency at all, unless it is set.
const LAG_MS = Number(process.env.RICHOS_APPEARANCE_LAG_MS || 0);

/// EVERY BRIDGE CALL THE PAGE COMPLETES, recorded — and optionally slowed down.
///
/// WHY A LEDGER AND NOT A LONGER SLEEP. Almost every check below presses a control and then
/// asserts what the DURABLE STORE holds: `.theme-opt` writes through `set_theme`, the font
/// keys through `set_font_scale`, the Techy row through `set_techy_default`, the name field
/// through `set_user_name`. Those are round trips, and the sleeps that stood after them were
/// bets on how long a round trip takes.
///
/// AND THE OBVIOUS FIX IS THE WRONG ONE, which is the whole reason this is a ledger. Waiting
/// for `localStorage` to hold `"light"` would make check 3's assertion — that the store holds
/// `"light"` — unfalsifiable: the wait and the assertion would be the same sentence, and a
/// product that wrote `"dark"` would hang rather than fail, which is a check that cannot go
/// red. So the wait is on a DIFFERENT fact from the assertion: that the call has completed.
/// What it wrote is still entirely open, and still asserted.
///
/// Recorded in a `finally`, so a command the mock does not implement (`set_splash_enabled` is
/// one — it rejects with "no such command", which `main.js` swallows on purpose) still counts
/// as completed. The alternative would hang on exactly the paths the product treats as fine.
const INSTRUMENT_BRIDGE = (lagMs) => {
  let real;
  window.__invokes = [];
  Object.defineProperty(window, "RichBridge", {
    configurable: true,
    get: () => real,
    set: (v) => {
      real = v;
      const invoke = v.invoke.bind(v);
      v.invoke = async (name, args) => {
        if (lagMs > 0) await new Promise((r) => setTimeout(r, lagMs));
        try {
          return await invoke(name, args);
        } finally {
          window.__invokes.push(name);
        }
      };
    },
  });
};

const invokeCount = (page, name) =>
  page.evaluate((n) => (window.__invokes || []).filter((x) => x === n).length, name);

/// Return once one more `name` call has COMPLETED than there were before, and the page has
/// stopped moving. `before` comes from `invokeCount` at the call site, so this cannot be
/// satisfied by a call that had already happened.
async function afterInvoke(page, name, before, opts) {
  opts = opts || {};
  await page.waitForFunction(
    ([n, b]) => (window.__invokes || []).filter((x) => x === n).length > b,
    [name, before],
    { timeout: opts.timeout || 10000 }
  );
  await settledPage(page);
}

/// Every finite animation and transition on the page has finished, and the result is painted.
async function settledPage(page) {
  await awaitSettled(page);
  await flushFrames(page);
}

/// An overlay that is un-hidden AND has finished its entry animation.
///
/// `.overlay-panel` runs `overlay-in 0.16s`, and a walk or a screenshot taken partway through
/// it is of a frame nobody looks at. `contrast.js` carries the measurement: sampling one of
/// these mid-fade is how `p#feedback-title` came to be reported at 1.39:1 against two colors
/// that exist in neither palette.
async function overlayOpen(page, selector) {
  await page.waitForSelector(selector + ":not([hidden])");
  await page.evaluate(async (sel) => {
    const el = document.querySelector(sel);
    if (!el) return;
    await Promise.all(el.getAnimations({ subtree: true }).map((a) => a.finished.catch(() => {})));
  }, selector);
  await flushFrames(page);
}

/// `Theme::default()`, read out of the Rust rather than typed here. §63's default lives in
/// THREE places by necessity — config.rs (the truth), theme-boot.js (the synchronous mirror,
/// because an async read cannot decide the first frame) and mock.js (the harness's stand-in
/// for the truth) — and three copies of one decision is exactly the shape that drifts. Check
/// 1 reads all three off disk and asserts they are the same word, so a change made on one
/// side arrives here as a failure instead of as a silent flash of the wrong palette.
function rustDefaultTheme() {
  const m = CONFIG_RS.match(/impl Default for Theme \{[\s\S]*?Theme::(\w+)\s*\n\s*\}/);
  assert(m, "no `impl Default for Theme` in config.rs");
  return m[1].toLowerCase();
}

/// The pre-paint mirror's own default — the `|| "..."` that decides the FIRST frame.
function mirrorDefaultTheme() {
  const src = fs.readFileSync(path.join(UI_DIR, "theme-boot.js"), "utf8");
  const m = src.match(/validTheme\(read\(THEME_KEY\)\)\s*\|\|\s*"(\w+)"/);
  assert(m, "no pre-paint theme default in theme-boot.js");
  return m[1];
}

/// The browser harness's stand-in for `Theme::default()`.
function mockDefaultTheme() {
  const src = fs.readFileSync(path.join(UI_DIR, "mock.js"), "utf8");
  const m = src.match(/const fresh = \{\s*theme:\s*"(\w+)"/);
  assert(m, "no fresh-config theme default in mock.js");
  return m[1];
}

/// The two grounds the shipped CSS paints, read off `--ground` rather than typed. `:root`
/// declares the dark one; `:root[data-theme="light"]` overrides it.
function shippedGrounds() {
  const css = STYLE_CSS;
  const hexToRgb = (hex) => {
    const n = parseInt(hex.slice(1), 16);
    return "rgb(" + ((n >> 16) & 255) + ", " + ((n >> 8) & 255) + ", " + (n & 255) + ")";
  };
  const light = css.match(
    /:root\s*\[\s*data-theme\s*=\s*"light"\s*\]\s*\{[\s\S]*?--ground:\s*(#[0-9a-fA-F]{6})/
  );
  const dark = css.match(/:root\s*\{[\s\S]*?--ground:\s*(#[0-9a-fA-F]{6})/);
  assert(light && dark, "style.css must declare --ground for both themes");
  return { light: hexToRgb(light[1]), dark: hexToRgb(dark[1]) };
}

/// The steps the control walks, read out of the Rust rather than typed here. A ladder changed
/// on one side only reaches this file as a failing check instead of as a `+` that lands
/// somewhere the store will snap away from.
function rustFontSteps() {
  const m = CONFIG_RS.match(/pub const FONT_SCALE_STEPS: \[u16; \d+\] = \[([^\]]+)\]/);
  assert(m, "no FONT_SCALE_STEPS in config.rs");
  return m[1].split(",").map((s) => Number(s.trim()));
}

/// Open the app. `stored` is what the DURABLE side already holds — seeded into the mock
/// harness's own key, which is the thing `get_appearance` answers from, not into the
/// pre-paint mirror. Seeding the mirror alone proves nothing: `syncAppearanceFromBackend`
/// would overwrite it a moment later, which is the product working correctly.
async function openApp(browser, opts) {
  opts = opts || {};
  // A CALLER-OWNED CONTEXT IS HOW A RELAUNCH IS SPELLED. `browser.newPage()` makes a
  // fresh context every time, and a fresh context has empty `localStorage` — which is both
  // the pre-paint mirror AND (through `richos-mock-config`) this harness's stand-in for
  // config.rs. So two `openApp` calls are two FIRST launches, and a check that needs "he
  // chose Dark last time" cannot be written with them: it could only seed the answer it
  // wanted to prove, which is check 2's shape and not check 1c's. A context that already
  // holds the previous launch's writes is the only way the durability claim is about the
  // product rather than about the seeding. The context carries the viewport and the OS
  // scheme, so neither is repeated on the page.
  const page = opts.context
    ? await opts.context.newPage()
    : await browser.newPage({
    viewport: { width: 1400, height: 950 },
    // A LIGHT operating system unless a walk says otherwise — and since §63 the DEFAULT
    // preference is `system`, so this is no longer merely a control: for any walk that does
    // not seed a theme, it is the thing that decides the palette. Every walk below that
    // asserts a lighting either seeds a preference (checks 2, 3, 4, 17) or names its OS
    // (checks 1, 1b, 1c); none of them leaves it implicit.
    colorScheme: opts.osScheme || "light",
      });
  collectPageErrors(page); // the live array is `page.__errors`, kept for the page's whole life
  await page.addInitScript(() => {
    // CAPTURE THE FIRST PAINT'S THEME, before anything can correct it.
    //
    // `theme-boot.js` runs in `<head>` and sets `data-theme` synchronously; `main.js`
    // reconciles against the store a few hundred ms later. Sampling after settle therefore
    // says nothing about the FIRST FRAME — and the first frame is the entire reason
    // theme-boot.js exists. An observer installed before any of it runs records the value
    // the CEO's eye actually meets, which is where a full-screen flash of the wrong palette
    // would show up and nowhere else.
    window.__firstTheme = null;
    const capture = () => {
      if (window.__firstTheme === null && document.documentElement) {
        window.__firstTheme = document.documentElement.getAttribute("data-theme");
      }
    };
    // Observing `document` with `subtree` rather than `documentElement` directly: at
    // init-script time the document is empty and `documentElement` can still be null, and an
    // observer attached to null never fires — which reads exactly like "the theme was never
    // set" and is the first thing this check got wrong about itself.
    new MutationObserver(capture).observe(document, {
      attributes: true,
      subtree: true,
      attributeFilter: ["data-theme"],
    });
    // Belt: `readyState === "interactive"` is after every synchronous script (theme-boot.js
    // among them) and before main.js's async reconciliation, so it is still the pre-settle
    // value even if the observer misses.
    document.addEventListener("readystatechange", capture, true);
    document.addEventListener("DOMContentLoaded", capture, true);
  });
  await page.addInitScript((o) => {
    try {
      if (o.stored) window.localStorage.setItem("richos-mock-config", JSON.stringify(o.stored));
      if (o.mirror) window.localStorage.setItem("richos-theme", o.mirror);
      if (o.mirrorScale) window.localStorage.setItem("richos-font-scale", String(o.mirrorScale));
    } catch (e) {
      /* storage unavailable — the shipped defaults apply, which is its own kind of evidence */
    }
  }, opts);
  // THE CURTAIN IS HELD BY THE HARNESS, AND THE HOLD INCLUDES THE CEILING. What used to be
  // here was a copy of contrast.js's four lines, which replaced the EXPORTED `yieldNow` and
  // left `start()`'s internal ceiling armed — so every walk below silently had four seconds
  // from `goto` to its last assertion, and a `macos-latest` runner does not have four seconds
  // spare. `lib/harness.js`'s `HOLD_CURTAIN` carries the diagnosis and the measurement.
  // A PRE-BOOT MOCK PRESET, for the one state a setter cannot reach: "no conversation is
  // open" is decided before `init()` branches on whether a thread is active, so it has to be
  // in place before any of the page's own scripts run. `mock.js` says the same thing from the
  // other side where it defines `__RICHOS_MOCK_PRESET__`.
  if (opts.preset) {
    await page.addInitScript((v) => {
      window.__RICHOS_MOCK_PRESET__ = v;
    }, opts.preset);
  }
  if (opts.holdSplash) await page.addInitScript(HOLD_CURTAIN);
  // The bridge ledger every wait below is built on, installed before any of the page's own
  // scripts run so no call can be missed. It carries the lag knob too, at the seam where lag
  // actually decides things — see `INSTRUMENT_BRIDGE`.
  await page.addInitScript(INSTRUMENT_BRIDGE, LAG_MS);
  await page.goto(APP);
  // The knob now sits INSIDE the bridge (`INSTRUMENT_BRIDGE` above) rather than as one delay
  // after `goto`. That is a strictly harder condition and a more honest one: a single pause
  // before the app starts delays every check equally and changes no ORDERING, while latency on
  // each call is what actually reorders a driver against the app — which is the condition that
  // decided run 34010691469 and the condition the fixed checks have to survive.
  // The home screen is the landing surface now; this suite is about the app UI behind it.
  await leaveHome(page);
  await page.waitForSelector(".nav-thread", { state: "attached" });
  if (!opts.holdSplash) {
    await page.waitForFunction(() => !document.getElementById("splash"), null, { timeout: 30000 }).catch(() => {});
  }
  // `init()` HAS DECIDED, rather than 400 ms have passed — and deliberately NOT "the theme is
  // what was asked for", which is what checks 1, 2 and 17 assert three lines later. A wait on
  // the asserted value is an assertion that cannot fail. `bootSettled` waits on `init()`
  // reaching its own branch point, which is many statements after `syncAppearanceFromBackend`,
  // so the reconciliation has provably landed and every theme assertion keeps its teeth.
  await bootSettled(page);
  await settledPage(page);
  // The hold is PROVEN on every walk that asked for one, not assumed. A curtain that left
  // early makes every assertion after this one a statement about the shell instead.
  if (opts.holdSplash) page.__curtain = await assertCurtainHeld(page);
  return page;
}

const themeOf = (page) => page.evaluate(() => document.documentElement.getAttribute("data-theme"));
/// The panel's ROWS, in order: column by column, then the foot band's cells (round 27). The
/// panel's heading, each column's caps label, the dividers and the technical view's hint are
/// not rows. The floor (round 24, `settings-button.js` `buildFloor`) holds Bust a bug and
/// Request a feature directly under it; each of its two buttons is a row of its own. A part
/// that was Preferences is named by its group title.
const menuRows = (page) =>
  page.evaluate(() =>
    [...document.querySelectorAll("#set-menu .sm-col, #set-menu .sm-foot-cell")]
      .flatMap((b) => [...b.children])
      .filter((n) => !n.matches(".sm-group, .sm-divider, .popover-hint"))
      .flatMap((n) => (n.classList.contains("set-floor") ? [...n.children] : [n]))
      .map((n) => {
        // The Codex row's name is its title; the rest of its text is its tooltip's words.
        const name = n.querySelector(".set-name, #cx-title, .popover-title");
        return name ? name.textContent.trim() : n.textContent.trim();
      })
  );

/// `.setmenu` carries `animation: setrise 0.2s ... both` — opacity 0 to 1 over a 14px
/// translate — so `visible` is true of a menu one frame into its own rise. The 150 ms was that
/// animation, guessed; this is the animation's own `finished`.
async function openMenu(page) {
  await page.click("#set-btn");
  await page.waitForSelector("#set-menu", { state: "visible" });
  await page.evaluate(async () => {
    const m = document.getElementById("set-menu");
    if (m) await Promise.all(m.getAnimations({ subtree: true }).map((a) => a.finished.catch(() => {})));
  });
  await flushFrames(page);
}

/// The three-way scope sheet §7.1 puts in front of EVERY techy switch, answered with the
/// option it opens on.
///
/// The CEO's sentence is the acceptance criterion: "when the user toggles the techy mode on
/// while being inside a conversation thread, then there should be something like a radio
/// button choice between 3 choices ... And the first choice should be preselected". So the
/// whole act from the settings menu is: click the row, the sheet opens, confirm what it
/// already offers — which is "For all conversations in all companies", the tier the row used
/// to write directly through `set_techy_default`.
///
/// It waits on `set_techy_scope` COMPLETING and then on the technical rows being in the
/// document, because those are two different facts: the write is the store's, the rows are
/// `loadTimeline`'s, and a check that read the page between them would be reading the calm
/// view and calling it the technical one.
async function confirmTechyScope(page) {
  await page.waitForSelector("#techy-scope:not([hidden])");
  const before = await invokeCount(page, "set_techy_scope");
  const preselected = await page.evaluate(() => {
    const on = document.querySelector('#techy-scope input[name="techy-scope"]:checked');
    return on ? on.value : null;
  });
  await page.click("#techy-scope-confirm");
  await page.waitForSelector("#techy-scope", { state: "hidden" });
  await afterInvoke(page, "set_techy_scope", before);
  return preselected;
}

/// Turn the technical view on everywhere, through the settings menu — the one piece of chrome
/// on every screen, and the door the CEO's own words name ("from settings").
async function techyOnEverywhere(page) {
  await openMenu(page);
  await page.click("#set-techy");
  const picked = await confirmTechyScope(page);
  assertEqual(picked, "all-companies", "the sheet's preselected option is not the all-companies tier");
  await page.waitForFunction(() => document.querySelectorAll(".tl-tech").length > 0, null, { timeout: 30000 });
  await settledPage(page);
}

async function main() {
  const { webkit } = loadPlaywright();
  const browser = await webkit.launch();
  const run = createRun("Appearance — two lightings, one type knob, and whose rail this is");
  // EVERY PAGE'S LIVE ERROR ARRAY, read when check 18 runs. `openApp` hands each page an
  // `errors` array that its `pageerror`/`console` listeners keep appending to for the page's
  // whole life; this used to copy that array once, right after the page opened, so an error
  // raised by any later click or shortcut never reached check 18 (part-2 hunt section 42).
  const tracker = createErrorTracker();
  const track = tracker.track;
  const allErrors = tracker.all;

  // ---- 1. system is the default, against BOTH operating systems -------------------------

  await run.check("1  a fresh install follows the SYSTEM — proven against a light OS AND a dark one", async () => {
    // §63: "the app should detect the user's system preference for dark or light theme and
    // system should be the default for a freshly installed app."
    //
    // THE TWO-OS WALK IS THE CHECK, and a one-OS version of it is worse than none. Until
    // 2026-09-19 this check asserted DARK against a light OS, which was §15 and was right
    // then. Its replacement must not be "assert dark against a dark OS": that passes for a
    // build that ignores the OS entirely and hands every light-OS user a dark app — the
    // precise defect §63 was given to fix. So both walks run, and the answers must DIFFER.
    const grounds = shippedGrounds();
    const walks = [
      ["light", "light", grounds.light],
      ["dark", "dark", grounds.dark],
    ];
    const seen = [];
    for (const [osScheme, wanted, ground] of walks) {
      const page = track(await openApp(browser, { osScheme: osScheme }));
      assertEqual(
        await themeOf(page),
        wanted,
        "§63: a fresh install under a " + osScheme + " OS must resolve " + wanted
      );
      const pref = await page.evaluate(() => window.RichTheme.theme());
      assertEqual(
        pref,
        "system",
        "and the stored PREFERENCE is 'system' — resolving correctly by coincidence is not " +
          "following the OS, and only 'system' keeps following it when the OS changes"
      );
      // Off the element that PAINTS the ground: `html` since the speckled ground made `body`
      // transparent on purpose (`lib/harness.js` `paintedGround`).
      assertEqual(
        await paintedGround(page),
        ground,
        "painted on the shipped --ground for " + wanted + ", not merely labeled it"
      );
      // ...and it was right in the FIRST frame, not corrected afterwards. This is the half a
      // settled sample cannot see: a pre-paint default of `dark` under a light OS gives a
      // full-screen flash of midnight blue on every launch and then quietly settles ivory.
      const first = await page.evaluate(() => window.__firstTheme);
      assertEqual(
        first,
        wanted,
        "the FIRST painted theme under a " + osScheme + " OS was " + first + " — theme-boot.js's " +
          "synchronous default is not 'system', so every launch flashes the wrong palette " +
          "before settling"
      );
      seen.push(osScheme + " OS -> " + wanted);
      await page.close();
    }
    assert(
      grounds.light !== grounds.dark,
      "the two walks would be indistinguishable: style.css paints one ground for both themes"
    );

    // AND THE DEFAULT IS ONE DECISION, NOT THREE COPIES OF ONE. config.rs holds the truth,
    // theme-boot.js mirrors it synchronously because an async read cannot decide the first
    // frame, and mock.js stands in for it in this browser. All three, off disk, same word.
    const rust = rustDefaultTheme();
    const mirror = mirrorDefaultTheme();
    const mock = mockDefaultTheme();
    assertEqual(rust, "system", "`impl Default for Theme` in config.rs is §63's actual ruling");
    assertEqual(mirror, rust, "theme-boot.js's pre-paint default has drifted from config.rs");
    assertEqual(mock, rust, "mock.js's fresh config has drifted from config.rs");

    return (
      "no stored preference: " + seen.join(", ") + " (" + grounds.light + " / " + grounds.dark +
      "), first frame and settled frame agree in both; pref='system' in both; and " +
      "config.rs / theme-boot.js / mock.js all default to '" + rust + "'"
    );
  });

  await run.check("1b  the OS flips WHILE THE APP IS OPEN and the app follows — but not over his own choice", async () => {
    // §63 says "detect", and a default alone detects once, at boot. A build that read
    // `prefers-color-scheme` at startup and never again passes check 1 completely and
    // leaves the CEO in the wrong palette every evening his Mac changes under him.
    //
    // The negative half is here too and is the more important one: an EXPLICIT choice must
    // NOT follow the OS. A build whose `resolved()` consulted `prefers-color-scheme`
    // whatever the preference said would silently overrule the theme he picked — §63's
    // second sentence failing in the one direction nobody would think to look. That is
    // mutation 1d at the foot of this file, and it turns both halves of this check red.
    const grounds = shippedGrounds();
    const page = track(await openApp(browser, { osScheme: "light" }));
    assertEqual(await themeOf(page), "light", "opened following a light OS");

    await page.emulateMedia({ colorScheme: "dark" });
    await settledPage(page);
    assertEqual(await themeOf(page), "dark", "the OS went dark and the app did not");
    assertEqual(
      await paintedGround(page),
      grounds.dark,
      "and the PAINT followed, not only the attribute"
    );
    assertEqual(
      await page.evaluate(() => window.RichTheme.theme()),
      "system",
      "and following the OS did not quietly become a stored preference for dark"
    );

    await page.emulateMedia({ colorScheme: "light" });
    await settledPage(page);
    assertEqual(await themeOf(page), "light", "and back again — it follows, it does not latch");

    // Now he CHOOSES dark. From here the OS is not his lighting any more.
    await openMenu(page);
    const wrote = await invokeCount(page, "set_theme");
    await page.click('.theme-opt[data-th="dark"]');
    await afterInvoke(page, "set_theme", wrote);
    assertEqual(await themeOf(page), "dark", "his choice took");

    await page.emulateMedia({ colorScheme: "light" });
    await settledPage(page);
    assertEqual(
      await themeOf(page),
      "dark",
      "a light OS overruled a preference he set by hand — §63 gives the OS the answer only " +
        "while nobody has given one"
    );
    await page.close();
    return (
      "pref='system': light -> dark -> light OS, each followed live and in the paint; after " +
      "an explicit switch to dark, a light OS no longer moves it"
    );
  });

  await run.check("1c  only an explicit switch is remembered — across a real relaunch, and System is reachable again", async () => {
    // §63's second sentence: "Only if the user switches the theme to some other option, only
    // then should the app remember that and from now on that would become their default."
    //
    // TWO PAGES IN ONE BROWSER CONTEXT, which is what makes this a relaunch and not a seed.
    // `localStorage` — the pre-paint mirror AND this harness's stand-in for config.rs — is
    // per-context, so the second page boots on exactly what the first one wrote and on
    // nothing this test typed. Under a LIGHT operating system throughout, so "it opened
    // dark" on the second launch can only be his remembered choice.
    const ctx = await browser.newContext({ viewport: { width: 1400, height: 950 }, colorScheme: "light" });

    const first = track(await openApp(browser, { context: ctx }));
    assertEqual(await themeOf(first), "light", "launch 1: nothing chosen, so the light OS decides");
    await openMenu(first);
    const wrote = await invokeCount(first, "set_theme");
    await first.click('.theme-opt[data-th="dark"]');
    await afterInvoke(first, "set_theme", wrote);
    assertEqual(await themeOf(first), "dark", "he switched to dark");
    const stored = await first.evaluate(() => JSON.parse(window.localStorage.getItem("richos-mock-config")));
    assertEqual(stored.theme, "dark", "and the DURABLE side holds it, not just the mirror");
    await first.close();

    const second = track(await openApp(browser, { context: ctx }));
    assertEqual(
      await second.evaluate(() => window.__firstTheme),
      "dark",
      "launch 2 FIRST FRAME: his remembered choice must decide the first paint, or the " +
        "relaunch flashes the OS's palette at him before settling"
    );
    assertEqual(await themeOf(second), "dark", "launch 2: his switch is his default from now on");
    assertEqual(await second.evaluate(() => window.RichTheme.theme()), "dark", "as a stored preference");

    // ...and System is a choice he can return to. A build that treated `system` as "no
    // preference" and declined to store it would leave him on dark forever.
    await openMenu(second);
    const wrote2 = await invokeCount(second, "set_theme");
    await second.click('.theme-opt[data-th="system"]');
    await afterInvoke(second, "set_theme", wrote2);
    assertEqual(await themeOf(second), "light", "picking System under a light OS returns to the OS");
    const stored2 = await second.evaluate(() => JSON.parse(window.localStorage.getItem("richos-mock-config")));
    assertEqual(stored2.theme, "system", "and System itself is stored, not treated as 'unset'");
    await second.close();

    const third = track(await openApp(browser, { context: ctx }));
    assertEqual(await themeOf(third), "light", "launch 3: following the OS again, and that survived too");
    await third.close();
    await ctx.close();
    return (
      "one context, three launches, light OS throughout: unset -> light; switch to dark -> " +
      "dark, remembered on the next launch's FIRST frame; back to System -> light again, and " +
      "that survives a relaunch as well"
    );
  });

  await run.check("1d  the HOME SCREEN is dark whatever the OS says, and hands the app back to the OS on the way out", async () => {
    // §63 GAVE THE OS A SURFACE IT MUST NOT REACH, and that is a new risk rather than an old
    // one. While the default was `dark`, a light-OS launch of the home screen was dark for
    // two independent reasons — the clamp AND the preference — so the clamp could have been
    // broken for weeks without anything looking wrong. The default is `system` now, the
    // preference under a light OS resolves LIGHT, and the clamp is the only thing between
    // the CEO and an ivory opening screen. Checks 4 and 4b hold the SPLASH to that; this one
    // is the home screen, which is a different surface with its own `forceDark` owner in
    // `home.js`, and nothing asserted it.
    //
    // The second half is what makes it a hand-off rather than a clamp: leaving the home
    // screen must drop the clamp and let the preference decide. A clamp that never lowers
    // looks identical on the surface it protects and takes the whole app with it.
    const seen = [];
    for (const [osScheme, afterHome] of [["light", "light"], ["dark", "dark"]]) {
      const page = await browser.newPage({ viewport: { width: 1400, height: 950 }, colorScheme: osScheme });
      const errors = [];
      page.on("pageerror", (e) => errors.push(String(e)));
      page.on("console", (m) => {
        if (m.type() === "error") errors.push("console: " + m.text());
      });
      await page.goto(APP);
      // The curtain is ABOVE the home screen and has its own clamp; clearing it leaves the
      // home screen up, which is the surface this check is about.
      await leaveSplash(page);
      await page.waitForFunction(() => typeof window.RichHome === "object", null, { timeout: 15000 });
      await page.waitForFunction(() => window.RichHome.isOpen(), null, { timeout: 15000 });
      await settledPage(page);
      const onHome = await page.evaluate(() => ({
        theme: document.documentElement.getAttribute("data-theme"),
        forced: window.RichTheme.forcedDark(),
        pref: window.RichTheme.theme(),
        themeRow: !!document.querySelector(".theme-opt"),
      }));
      onHome.ground = await paintedGround(page);
      assertEqual(onHome.theme, "dark", "§15's clamp: the home screen is dark under a " + osScheme + " OS");
      assertEqual(onHome.ground, "rgb(12, 19, 34)", "painted on the §14 ground, not merely named dark");
      assertEqual(onHome.forced, true, "and it is a FORCE, so it cannot be mistaken for a preference");
      assertEqual(onHome.pref, "system", "his own preference is untouched underneath the clamp");
      assertEqual(onHome.themeRow, false, "no theme switch reaches the home screen (§15)");

      await leaveHome(page);
      await page.waitForSelector(".nav-thread", { state: "attached" });
      await bootSettled(page);
      await settledPage(page);
      const afterward = await page.evaluate(() => ({
        theme: document.documentElement.getAttribute("data-theme"),
        forced: window.RichTheme.forcedDark(),
      }));
      assertEqual(afterward.forced, false, "the clamp must LOWER on the way out, or it is a trap");
      assertEqual(
        afterward.theme,
        afterHome,
        "and the normal screens take the " + osScheme + " OS's answer, which is what §63 gives them"
      );
      assertEqual(errors, [], "no page errors on the " + osScheme + "-OS home walk");
      seen.push(osScheme + " OS: home dark (forced), then " + afterward.theme);
      await page.close();
    }
    return seen.join("; ") + " — pref stayed 'system' throughout, and the theme row was absent on both home walks";
  });

  // ---- 2. durable, and the backend wins ------------------------------------------------

  await run.check("2  the choice is durable, and config.rs wins a disagreement with the mirror", async () => {
    // The store says LIGHT; the pre-paint mirror says DARK. This is a real shape — a
    // preference set on a launch whose mirror was later cleared, or two windows. The first
    // frame is the mirror's (it is all that exists synchronously) and the settled answer
    // must be the STORE's, because a mirror that can win is a second place the decision
    // lives and the CEO's preference starts flipping between launches.
    const page = track(
      await openApp(browser, {
        stored: { theme: "light", font_scale: 100, user_name: null },
        mirror: "dark",
      })
    );
    assertEqual(await themeOf(page), "light", "the durable answer won");
    const mirrored = await page.evaluate(() => {
      try {
        return window.localStorage.getItem("richos-theme");
      } catch (e) {
        return null;
      }
    });
    assertEqual(mirrored, "light", "and the mirror was CORRECTED, not left disagreeing");
    await page.close();
    return "store=light vs mirror=dark -> light wins and the mirror is rewritten to match";
  });

  await run.check("3  switching the theme writes through to the durable store", async () => {
    const page = track(await openApp(browser));
    await openMenu(page);
    const wrote = await invokeCount(page, "set_theme");
    await page.click('.theme-opt[data-th="light"]');
    // The WRITE has completed. What it wrote is the next four assertions' business and is not
    // waited for — see `afterInvoke` for why that distinction is the whole point.
    await afterInvoke(page, "set_theme", wrote);
    assertEqual(await themeOf(page), "light", "the document crossed over");
    const stored = await page.evaluate(() => JSON.parse(window.localStorage.getItem("richos-mock-config")));
    assertEqual(stored.theme, "light", "and the STORE holds it, not just the mirror");
    const pressed = await page.evaluate(() =>
      [...document.querySelectorAll(".theme-opt")].map((b) => b.dataset.th + ":" + b.getAttribute("aria-pressed"))
    );
    assertEqual(
      pressed,
      ["light:true", "system:false", "dark:false"],
      "and the ported segment reports its state through aria-pressed, as the reference does"
    );
    await page.close();
    return "the segment writes to config.rs and reports itself through aria-pressed";
  });

  // ---- 4-6. the always-dark opening screen, and the floor -------------------------------

  await run.check("4  the opening screen is ALWAYS dark, even for a CEO who chose light", async () => {
    const page = track(
      await openApp(browser, {
        stored: { theme: "light", font_scale: 100, user_name: null },
        mirror: "light",
        holdSplash: true,
      })
    );
    assert(await page.evaluate(() => !!document.getElementById("splash")), "the curtain is up for this walk");
    assertEqual(await themeOf(page), "dark", "§15's one permanent exception");
    // ...and his own preference is UNTOUCHED, which is what makes it a clamp and not a write.
    assertEqual(
      await page.evaluate(() => window.RichTheme.theme()),
      "light",
      "the clamp must not overwrite what he chose — light has to still be there afterwards"
    );
    const curtain = page.__curtain;
    await page.close();
    return "pref stays light, the resolved theme is dark, and nothing was written — " + curtain;
  });

  await run.check("4b  the held curtain outlives the ceiling, so check 4 is not on a four-second clock", async () => {
    // WHY THIS EXISTS, and it is not belt and braces. Check 4 above reads the clamp at
    // whatever moment the harness happens to arrive, and until 2026-09-06 the curtain it was
    // reading came down on its own 4,000 ms after boot — `holdMs + CEILING_GRACE_MS`, armed
    // over an internal reference that the `yieldNow` override cannot reach. On this machine
    // the walk arrives at ~730 ms and passes; on `macos-latest` it arrives late and run
    // 34010691469 reported `expected "dark" / actual "light"`.
    //
    // `HOLD_CURTAIN` takes that timer out, and `assertCurtainHeld` proves it took exactly one
    // timer of the right size. This check proves the CONSEQUENCE, which is the part a reader
    // actually cares about: the curtain is still up, and still dark, well past the instant it
    // used to leave. It waits on the PAGE's clock rather than the harness's, so it is asking
    // "has the ceiling's moment gone by?" and never "has the test been slow enough?".
    const page = track(
      await openApp(browser, {
        stored: { theme: "light", font_scale: 100, user_name: null },
        mirror: "light",
        holdSplash: true,
      })
    );
    const held = await page.evaluate(async () => {
      // The ceiling's own moment, read off the record `HOLD_CURTAIN` left rather than
      // reconstructed: the instant it was armed plus the delay it was armed for. A further
      // second puts the sample unambiguously past it on any machine.
      const record = window.__heldCurtain;
      const past = Math.round(record.armedAt + record.disarmed[0] + 1000);
      await new Promise((resolve) => {
        const tick = () => (performance.now() >= past ? resolve() : setTimeout(tick, 50));
        tick();
      });
      return {
        at: Math.round(performance.now()),
        past: past,
        wouldHaveFiredAt: Math.round(record.armedAt + record.disarmed[0]),
        theme: document.documentElement.getAttribute("data-theme"),
        pref: window.RichTheme.theme(),
        onScreen: !!document.getElementById("splash"),
        reason: window.RichSplash.state.reason,
      };
    });
    assert(held.onScreen, "the curtain left at " + held.at + "ms, reason " + held.reason + " — the hold did not hold");
    assertEqual(held.reason, null, "the curtain yielded for: " + held.reason);
    assertEqual(held.theme, "dark", "the always-dark clamp was dropped while the opening screen was still up");
    assertEqual(held.pref, "light", "and the CEO's own preference is still untouched");
    await page.close();
    return "sampled at " + held.at + "ms, " + (held.at - held.wouldHaveFiredAt) + "ms after the " +
      held.wouldHaveFiredAt + "ms the ceiling would have fired at — the curtain is still up, still dark, " +
      "and the stored preference is still light";
  });

  await run.check("5  NO settings button on the opening screen until the space key holds it — and then no theme switch", async () => {
    // CEO §62, 2026-09-19, and it is the ONE declared exception to §15's "always everywhere on
    // every page": *"The splash screen is one of those rare cases where the screen should NOT
    // have a settings button (because it's only there for 3 seconds by default ...). However
    // ... while the splash screen is 'paused', THAT'S when the settings button should show up
    // on the splash screen. Because that's the only time it would make sense."*
    //
    // THIS CHECK USED TO ASSERT THE OPPOSITE — "the button is there, it is on every screen, no
    // exception" — and it was right to, until he ruled. Both halves are asserted now, in the
    // order he gave them: absent while the screen runs, present the moment it is held. The
    // rest of the walk is unchanged, because the rest of the ruling is unchanged: no theme
    // switch reaches the start screen, and Bust a bug is still the floor.
    const page = track(
      await openApp(browser, {
        stored: { theme: "light", font_scale: 100, user_name: null },
        holdSplash: true,
      })
    );
    assert(
      await page.evaluate(() => !!document.querySelector(".settings") && !!document.getElementById("set-btn")),
      "settings-button.js did not mount at all — 'no button' would be true here for the wrong reason"
    );
    assert(
      !(await page.locator("#set-btn").isVisible()),
      "§62: the RUNNING opening screen must not carry a settings button"
    );
    // THE SHUTTER WAITS FOR THE BAR TO STOP BEFORE THE HOLD GOES ON, and that is about the
    // committed picture rather than about the ruling. The space key freezes the composition
    // where it stands, so a photograph taken of a screen held mid-ceremony is a photograph of
    // whenever this machine happened to get its turn — and the bar is the part that moves for
    // three seconds. Measured 2026-09-19: pausing mid-run moved 7,069 pixels of this file, all
    // of them inside the bar's own 349x70 box. `state.barStopped` is the product saying the one
    // pass is over (`splash.js`), after which nothing on this surface moves, so the picture is
    // the settled composition plus the one control §62 adds — and it differs from the file it
    // replaces by exactly that control's 40x40 box. `HOLD_CURTAIN` has disarmed the ceiling, so
    // there is no clock under this wait.
    await page.waitForFunction(() => window.RichSplash.state.barStopped === true, null, { timeout: 30000 });
    await page.keyboard.press("Space");
    assert(
      await page.evaluate(() => window.RichSplash.state.paused === true),
      "space did not hold the opening screen, so §62's state was never reached"
    );
    assert(await page.locator("#set-btn").isVisible(), "§62: the button shows up while the screen is held");
    await openMenu(page);
    const opts = await page.evaluate(() => document.querySelectorAll(".theme-opt").length);
    assertEqual(opts, 0, "no switch reaches the start screen — and it is OMITTED, not disabled");
    assert(
      await page.locator("#bug-btn").isVisible(),
      "...and the floor is intact: 'the very least that settings button always provides is a quick access to Bust a bug'"
    );
    const s = await shot(page, SHOTS + "/10-1-start-screen-always-dark", { fullPage: false });
    await page.close();
    return "absent while it ran, present once space held it, 0 theme options, Bust a bug reachable — " +
      path.basename(s.file);
  });

  await run.check("6  reporting a bug from the HELD opening screen does not leave the opening screen", async () => {
    // The ruling's reason, not just its letter: "reporting a bug must never require
    // navigating away from the screen the bug is on", and the screen a first-run user is
    // most likely to be stuck on is the one the curtain is covering. splash.js dismisses on
    // first input, so touching the settings button had to stop counting as first input.
    //
    // SINCE CEO §62 THE BUTTON IS ONLY THERE WHILE THE SCREEN IS HELD, so the walk begins with
    // the space key — which is also the only thing that changed here. Everything after it is
    // the same claim it always was, including the last line: an ordinary click still dismisses.
    const page = track(await openApp(browser, { holdSplash: true }));
    await page.keyboard.press("Space");
    assert(
      await page.evaluate(() => window.RichSplash.state.paused === true),
      "space did not hold the opening screen, so the settings button is not on it to press"
    );
    await openMenu(page);
    assert(await page.evaluate(() => !!document.getElementById("splash")), "opening the menu did not lift the curtain");
    await page.click("#bug-btn");
    // SINCE ROUND 21 (CEO §115) THE BUTTON OPENS THE REPORT, NOT A NOTICE: `bug-report.js` is
    // registered, so `bustABug` hands over to it and Rich's first question is drawn in the Rich
    // panel (`#bugdock-msgs`), the one place a report lives (CEO 2026-10-10), whatever
    // the screen under the curtain is. The wait is for that question to EXIST; what it says is
    // asserted below.
    await page.waitForSelector("#bugdock-msgs .bug-rich", { state: "attached" });
    await settledPage(page);
    const after = await page.evaluate(() => {
      const q = document.querySelector("#bugdock-msgs .bug-rich .tl-prose");
      return { splash: !!document.getElementById("splash"), asked: q ? q.textContent.trim() : "" };
    });
    assert(after.splash, "and neither did pressing Bust a bug");
    assert(
      after.asked.startsWith("What went wrong?"),
      "the report did not start — a control that appears to do nothing reads as broken: " + JSON.stringify(after.asked)
    );
    // ...while an ordinary click still dismisses, exactly as it did before.
    await page.mouse.click(700, 400);
    // THE WAIT AND THE CLAIM ARE THE SAME SENTENCE HERE, and that is fine BECAUSE IT IS
    // BOUNDED: a curtain that never leaves exhausts the budget and the check fails with the
    // sentence below, which is the outcome a 900 ms sleep produced too. What changes is that a
    // curtain which leaves in 950 ms now passes instead of failing, and one that never leaves
    // still fails. `removeSelf` takes the node out 220 ms after the yield, so 30 s is far
    // beyond the whole ceremony and only ends a hang.
    const dismissed = await page
      .waitForFunction(() => !document.getElementById("splash"), null, { timeout: 30000 })
      .then(() => true)
      .catch(() => false);
    assert(
      dismissed,
      "the first-input dismissal is otherwise UNCHANGED — the exception is exactly one control wide"
    );
    await page.close();
    return "held by the space key, the curtain survives the settings button and Bust a bug (the report opened: " +
      JSON.stringify(after.asked.slice(0, 40) + "…") + "), and still yields to any other input";
  });

  // ---- 7. the button is everywhere, above everything ------------------------------------

  await run.check("7  the settings button is on every surface, above every overlay", async () => {
    const surfaces = [
      ["shell", async () => {}],
      // `.tl-turn` is satisfied by the PREVIOUS thread's turn — the old rows sit in `#messages`
      // until `loadTimeline` replaces them — so that selector never proved this thread had
      // arrived. `settleOnThread` is the end state: the rail row, the bound model and the
      // painted turns, all three.
      ["thread", async (p) => { await p.click('.nav-thread[data-thread-id="hiring"]'); await settleOnThread(p, "hiring"); }],
      ["corrections", async (p) => { await p.click("#nav-corrections"); await overlayOpen(p, "#corrections-overlay"); }],
      ["feedback", async (p) => { await p.click("#nav-feedback"); await overlayOpen(p, "#feedback-overlay"); }],
      ["search", async (p) => { await p.click("#nav-search"); await overlayOpen(p, "#search-overlay"); }],
    ];
    const report = [];
    for (const [name, drive] of surfaces) {
      const page = track(await openApp(browser));
      await drive(page);
      await settledPage(page);
      // Visible is not enough. ON TOP is the claim — §15 says "above every overlay" — so
      // this is a hit test at the button's own centre, which is what a pointer would do.
      const onTop = await page.evaluate(() => {
        const b = document.getElementById("set-btn");
        if (!b) return "MISSING";
        const r = b.getBoundingClientRect();
        const hit = document.elementFromPoint(r.left + r.width / 2, r.top + r.height / 2);
        return hit && hit.closest(".settings") ? "ON-TOP" : "COVERED by " + (hit ? hit.tagName + "." + hit.className : "nothing");
      });
      assertEqual(onTop, "ON-TOP", "the settings button is not reachable on the " + name + " surface");
      report.push(name);
      await page.close();
    }

    // AND THE ONE SURFACE A HIT TEST CANNOT SPEAK FOR. The opening screen's curtain is
    // `pointer-events: none` for its whole life, so `elementFromPoint` walks straight
    // through it and returns the button whether the button is above the curtain or buried
    // under it. The hit test would pass over a settings button that was invisible — which
    // is the single most important place for it not to be, since §15's floor exists for
    // exactly the screen a first-run user is stuck on.
    //
    // CEO §62 NARROWED WHEN THAT BUTTON IS THERE AND NOT WHERE IT SITS WHEN IT IS: it is on
    // the opening screen only while the space key is holding the screen, and in that state it
    // still has to be above the curtain or it is not reachable at all. So this half of the
    // check is unchanged and is if anything load-bearing now — `tests/splash.js` check 25d
    // drives the live hit test in the held state, and this is the structural statement behind
    // it.
    //
    // So this half is structural: `.settings` must out-rank EVERY other z-index the app
    // ships, computed from the stylesheets rather than compared against a number typed
    // here. A new overlay that outranks it fails this check on the day it lands.
    //
    // "THE SHIPPED CSS" USED TO MEAN TWO FILES OUT OF THREE. This read `style.css` and
    // `splash.css` while `index.html` links a third — the 53 KB `home.css` — under a comment
    // claiming EVERY z-index the app ships. It is derived from `lib/ui-sources.js` now, and
    // widening it turned this check RED on the first run: `home.css` declares 340 against
    // `.settings`'s 300. That is the gate working, and what it found is below.
    const css = SOURCES.styleSources()
      .map((f) => fs.readFileSync(SOURCES.abs(f), "utf8"))
      .join("\n");

    // WHAT THE COMPONENT OWNS, AND WHY EACH ONE IS ALLOWED ABOVE THE BUTTON.
    //
    // §15's requirement is that the settings BUTTON is reachable from every screen. An
    // element that the button's own menu OPENED is not burying it — it is what pressing it
    // did — which is why `#bug-toast` was already excluded here. `.home-prefs` is the same
    // shape and was invisible to this check until `home.css` was: `settings-button.js`'s
    // `wireHome()` closes the menu and calls `home.open()`, and the dialog that opens is
    // `aria-modal` with its own Done control.
    //
    // AN OWNERSHIP CLAIM IS CHECKED, NOT ASSERTED. Each entry below names the property that
    // makes it owned, and the property is verified against the live DOM at the bottom of
    // this check — otherwise this list is a mute button with a comment on it, and any
    // future overlay that turned this check red could be silenced by being added to it.
    const SETTINGS_OWNED = [
      { sel: "#bug-toast", why: "the toast the menu's own Bust a bug raises", proof: "raised-by-the-menu" },
      // `.home-prefs` is NOT on this list since the review of 66ba53861
      // (rv-20261010T081721Z-66ba5386-a4d1, finding 2): at z-index 340 its scrim covered Settings
      // and a press there closed it instead of opening the menu. It is 240 now, below the button,
      // and not aria-modal (proven at the foot of this check, and by check 7c with every overlay).
      // `#techy-scope` is NOT on this list since the second review of 9463220ef (finding 2): at
      // z-index 310 it covered Settings. It is an ordinary overlay below the button; the menu
      // closes as it opens, so the menu covers none of it (proven at the foot of this check).
    ];

    const zOf = (sel) => {
      const m = css.match(new RegExp(sel + "\\s*\\{[^}]*z-index:\\s*(\\d+)"));
      assert(m, "no z-index found for " + sel + " in the shipped CSS — the selector moved or the rule is gone");
      return Number(m[1]);
    };
    const settingsZ = zOf("\\.settings");
    const ownedZ = SETTINGS_OWNED.map((o) => ({ ...o, z: zOf(o.sel) }));
    const ownedValues = new Set([settingsZ, ...ownedZ.map((o) => o.z)]);
    const others = [...css.matchAll(/z-index:\s*(\d+)/g)]
      .map((m) => Number(m[1]))
      .filter((z) => !ownedValues.has(z));
    const highest = Math.max(...others);
    assert(
      settingsZ > highest,
      "`.settings` is z-index " + settingsZ + " and something else in the shipped CSS is " +
        highest + ". §15 says the button is above EVERY overlay; the opening screen's curtain " +
        "alone is 200, and a curtain painted over the settings button satisfies the letter of " +
        "'it is on every page' and none of its point."
    );
    for (const o of ownedZ) {
      assert(
        o.z >= settingsZ,
        o.sel + " (" + o.z + ") is below the settings component (" + settingsZ + "), so it is not " +
          "something the menu paints over itself and does not belong on the owned list"
      );
    }

    // THE PROOF THAT EACH OWNED OVERLAY REALLY IS THE MENU'S. Driven, in the real shell.
    const owner = track(await openApp(browser));
    await owner.click("#set-btn");
    await owner.waitForSelector("#set-menu", { state: "visible" });
    const ownership = await owner.evaluate(() => {
      const menu = document.getElementById("set-menu");
      return {
        bugInMenu: !!(menu && menu.querySelector("#set-bug, .set-bug, [id*='bug']")),
        homeRowInMenu: !!(menu && menu.querySelector("#set-home-open")),
      };
    });
    assert(
      ownership.bugInMenu,
      "#bug-toast is excused as 'the toast the menu raises' and the menu has no Bust a bug control in it"
    );
    assert(ownership.homeRowInMenu, "the menu has no Home screen row to open `.home-prefs` from");
    await owner.click("#set-home-open");
    await owner.waitForSelector("#home-prefs:not([hidden])");
    const modal = await owner.evaluate(() => {
      const panel = document.querySelector(".home-prefs-panel");
      const b = document.getElementById("set-btn").getBoundingClientRect();
      const hit = document.elementFromPoint(b.left + b.width / 2, b.top + b.height / 2);
      return {
        ariaModal: panel ? panel.getAttribute("aria-modal") : null,
        role: panel ? panel.getAttribute("role") : null,
        done: !!document.getElementById("home-prefs-done"),
        menuHidden: document.getElementById("set-menu").hidden,
        settingsOnTop: !!(hit && hit.closest(".settings")),
      };
    });
    // And `#techy-scope`: opened by the Technical view switch inside the menu, which closes as it
    // opens, so nothing covers the sheet's own controls and the Settings button is above it.
    await owner.click("#home-prefs-done");
    await owner.waitForSelector("#home-prefs", { state: "hidden" });
    if (await owner.locator("#set-menu").isHidden()) await owner.click("#set-btn");
    await owner.waitForSelector("#set-menu", { state: "visible" });
    const techyInMenu = await owner.evaluate(() => !!document.querySelector("#set-menu #set-techy"));
    await owner.click("#set-techy");
    await owner.waitForSelector("#techy-scope:not([hidden])");
    const scope = await owner.evaluate(() => {
      const sheet = document.getElementById("techy-scope");
      const at = (el) => { const b = el.getBoundingClientRect(); return document.elementFromPoint(b.left + b.width / 2, b.top + b.height / 2); };
      const top = at(document.getElementById("techy-scope-confirm"));
      const btn = at(document.getElementById("set-btn"));
      return { role: sheet.getAttribute("role"), ariaModal: sheet.getAttribute("aria-modal"), menuHidden: document.getElementById("set-menu").hidden,
        cancel: !!document.getElementById("techy-scope-cancel"), confirmOnTop: !!(top && top.closest("#techy-scope")), settingsOnTop: !!(btn && btn.closest(".settings")) };
    });
    await owner.click("#techy-scope-cancel");
    await owner.close();
    assert(techyInMenu, "the menu has no Technical view switch to open `#techy-scope` from");
    assertEqual(scope.role, "dialog", "`#techy-scope` is not a dialog");
    assertEqual(scope.ariaModal, null, "`#techy-scope` claims aria-modal, which leaves Settings out of the accessibility tree");
    assert(scope.cancel, "`#techy-scope` has no control to close it");
    assert(scope.menuHidden, "the menu stayed open over `#techy-scope`");
    assert(scope.confirmOnTop, "`#techy-scope`'s Turn it on is covered by something while the sheet is open");
    assert(scope.settingsOnTop, "the Settings button is covered while `#techy-scope` is open");
    assertEqual(modal.role, "dialog", "`.home-prefs` is not a dialog");
    assertEqual(modal.ariaModal, null, "`.home-prefs` claims aria-modal, which leaves Settings out of the accessibility tree");
    assert(modal.done, "`.home-prefs` has no control to close it");
    assert(modal.menuHidden, "the menu stayed open over `.home-prefs`");
    assert(modal.settingsOnTop, "the Settings button is covered while `.home-prefs` is open");
    return (
      report.length + " surfaces, the button hit-testable on every one (" + report.join(", ") +
      "); z-index " + settingsZ + " out-ranks every other z-index in " +
      SOURCES.styleSources().join(" + ") + " (highest other: " + highest + "), and the " +
      ownedZ.length + " above it are the component's own, each proven so in the real shell: " +
      ownedZ.map((o) => o.sel.replace(/\\/g, "") + " " + o.z + " — " + o.why).join("; ")
    );
  });

  // ---- 7c. over EVERY overlay, found rather than listed ----------------------------------
  //
  // The review of 66ba53861 (rv-20261010T081721Z-66ba5386-a4d1), finding 2: Settings -> Home
  // screen opened an aria-modal panel at z-index 340 over Settings (300), and check 7 above
  // excused it by name. Each round before had found one more such overlay missing from a typed
  // list (D24's four, then the Technical view question), and behind them six sheets the menu
  // itself opens inerted Settings through main.js's question walk. So this check carries no list:
  // `lib/overlays.js` derives every rule in the shipped CSS that covers the window and every
  // `aria-modal` the shipped markup and code declare, and each element those rules match in the
  // real shell (one made from the selector where none is in the page yet) is put on screen with
  // focus inside it, the failing state D24 found. Over each: the Settings button is what is
  // painted at its place, it is in the accessibility tree (not hidden, not `inert`, not under
  // `aria-hidden`, not outside an `aria-modal` dialog that holds focus: WebKit's rules applied
  // from the DOM, as `bug-report.js`'s D24 check does), and a pointer press opens its menu.
  // Left out: only the two first-run questions, which main.js's own `QUESTION_SELECTOR` finds
  // and which keep holding Settings back (`setup.js` check 20; see FIRST_RUN_QUESTIONS below).
  await run.check("7c Settings is on top, in the accessibility tree and opens its menu over every overlay the shipped markup and CSS can open", async () => {
    // THE ONE THING LEFT OUT: THE TWO FIRST-RUN QUESTIONS, as main.js defines them. They keep
    // holding Settings back with the rest of the window (Ray's B1b, `setup.js` check 20; the CEO's
    // common-sense exception to §15, 2026-10-10). Which elements they are is read from the
    // product's own `QUESTION_SELECTOR` in the live shell, and that set must be exactly these two:
    // any other sheet that comes to match it fails here by name, so this is never a way out.
    const FIRST_RUN_QUESTIONS = ["memory-setup", "setup-sheet"];
    const qsel = (MAIN_JS.match(/^const QUESTION_SELECTOR = '([^']+)';$/m) || [])[1];
    assert(qsel, "main.js has no `const QUESTION_SELECTOR = '...';` to read the first-run questions from");
    const rules = OVERLAYS.coveringRules();
    const scout = track(await openApp(browser));
    const counts = await scout.evaluate(
      (sels) => sels.map((s) => { try { return document.querySelectorAll(s).length; } catch (e) { return -1; } }),
      rules.map((r) => r.selector)
    );
    const questions = await scout.evaluate((q) => [...document.querySelectorAll(q)].map((n) => n.id).sort(), qsel);
    await scout.close();
    // Every failure is collected and named together, so one red run lists the whole class.
    const wrong = questions
      .filter((id) => !FIRST_RUN_QUESTIONS.includes(id))
      .map((id) => "#" + id + " matches main.js QUESTION_SELECTOR, which holds Settings back, and is not a first-run question");
    // An aria-modal site is the first-run questions' own when it is on their tag, or in main.js's
    // selector that finds them.
    const theirs = (s) =>
      (s.file === "main.js" && s.text.startsWith("const QUESTION_SELECTOR")) ||
      FIRST_RUN_QUESTIONS.some((id) => s.text.includes('id="' + id + '"'));
    wrong.push(
      ...OVERLAYS.ariaModalSites()
        .filter((s) => !theirs(s))
        .map((s) => "aria-modal at " + s.file + ":" + s.line + " (WebKit leaves Settings out of the accessibility tree while focus is in it): " + s.text)
    );
    const targets = [];
    rules.forEach((r, k) => {
      assert(counts[k] !== -1, "the covering rule `" + r.selector + "` (" + r.file + ") is not a selector the page can query");
      if (counts[k] === 0) targets.push({ sel: r.selector, index: -1 });
      for (let i = 0; i < counts[k]; i++) targets.push({ sel: r.selector, index: i });
    });
    const seen = [];
    const heldBack = [];
    for (const t of targets) {
      const page = track(await openApp(browser));
      const shown = await page.evaluate(({ sel, index, qsel, firstRun }) => {
        let el = index >= 0 ? document.querySelectorAll(sel)[index] : null;
        if (el && el.matches(qsel) && firstRun.includes(el.id)) return { question: el.id };
        let made = false;
        if (!el) {
          // Not in the page until something opens it (`.home-prefs` is built on first open): one
          // made from the selector stands in for it, so the stylesheet's claim is still tested.
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
        const name = el.tagName.toLowerCase() + (el.id ? "#" + el.id : el.className ? "." + String(el.className).split(" ")[0] : "");
        return { name: made ? name + " (made from " + sel + ")" : name, focused: !!first && el.contains(document.activeElement) };
      }, { sel: t.sel, index: t.index, qsel, firstRun: FIRST_RUN_QUESTIONS });
      if (shown.question) {
        heldBack.push("#" + shown.question);
        await page.close();
        continue;
      }
      if (shown.error) {
        wrong.push("`" + t.sel + "`: " + shown.error);
        await page.close();
        continue;
      }
      // main.js's question walk runs from a MutationObserver: two frames for it to have run.
      await page.evaluate(() => new Promise((r) => requestAnimationFrame(() => requestAnimationFrame(r))));
      const reach = await page.evaluate(() => {
        const btn = document.getElementById("set-btn");
        if (!btn) return { top: "MISSING", tree: "MISSING" };
        const r = btn.getBoundingClientRect();
        const hit = document.elementFromPoint(r.left + r.width / 2, r.top + r.height / 2);
        const named = (n) => (n ? n.tagName.toLowerCase() + (n.id ? "#" + n.id : "") : "nothing");
        let why = null;
        for (let n = btn; n && n.nodeType === 1 && !why; n = n.parentElement) {
          if (n.hidden || n.hasAttribute("inert") || n.getAttribute("aria-hidden") === "true") why = "hidden or inert at " + named(n);
        }
        const focus = document.activeElement;
        for (const m of document.querySelectorAll('[aria-modal="true"]')) {
          if (!why && !m.hidden && m.getClientRects().length && m.contains(focus) && !m.contains(btn)) why = "outside aria-modal " + named(m);
        }
        return { top: hit && hit.closest(".settings") ? "ON-TOP" : "COVERED by " + named(hit), tree: why || "IN THE TREE" };
      });
      let opened = "NOT PRESSED";
      if (reach.top === "ON-TOP") {
        const b = await page.locator("#set-btn").boundingBox();
        await page.mouse.click(b.x + b.width / 2, b.y + b.height / 2);
        opened = await page
          .waitForSelector("#set-menu:not([hidden])")
          .then(() => "MENU OPENED", () => "MENU DID NOT OPEN");
      }
      const line = shown.name + ": " + reach.top + ", " + reach.tree + ", " + opened + (shown.focused ? ", focus inside" : "");
      seen.push(line);
      if (reach.top !== "ON-TOP" || reach.tree !== "IN THE TREE" || opened !== "MENU OPENED") wrong.push(line);
      await page.close();
    }
    assertEqual(wrong, [], "Settings is not on top and reachable over every overlay");
    return (
      targets.length + " overlays from " + rules.length + " covering rules, no aria-modal but the first-run questions'; " +
      "left out, the first-run questions that hold Settings back (setup.js check 20): " + heldBack.join(", ") + "; " + seen.join("; ")
    );
  });

  // ---- 8. the menu's contents, in §15's order ------------------------------------------

  await run.check("8  the panel is theme, then Text size, then Technical view, in round 27's places, then the floor", async () => {
    const page = track(await openApp(browser));
    await openMenu(page);
    const rows = await menuRows(page);
    assertEqual(
      rows,
      // ROUND 27 (richos-hq design/mockups/rounds/round-27, NOTES.md "Where every setting now
      // lives"): ONE Settings panel holds what the top-right menu and the rail gear's
      // Preferences held. Column 1 "How it looks", column 2 "Working with Rich", column 3 "Your
      // company and team", then the band across the foot; the floor is still last.
      // §15 still fixes the first three: Text size "directly under the theme switch", and
      // "directly under that, a Techy Mode toggle" (named "Technical view" since Ray's
      // candidate-.12 defect A). The technical-only Claude Code quota row follows that toggle.
      // "Splash screen" AND NOT "Opening screen" since audit-10 row 1 (`splash.js` 11b), and it
      // sits in the foot band, away from the first things the panel shows (the CEO's
      // 2026-10-06 feedback item 7). Rich's model is first in the middle column, never in the
      // first (§119: "in general settings, not in quick settings"). Request a feature is the
      // floor's second row, directly under Bust a bug (round 24). The Oxford comma switch sits
      // directly under Text size, its heading saying OFF or ON (round 33, CEO 2026-10-19).
      // Terminal setup is the last row of "Working with Rich" and Your name in Git sits directly
      // under Connected folders (round 41, CEO 2026-10-25).
      [
        "Theme", "Text size", "Oxford comma: OFF", "Technical view", "Claude Code quota", "Keep the stored output",
        "Rich's model", "How much should Rich interrupt you?", "Use Rich from your phone", "Terminal setup",
        "Company", "Home screen", "Token spend", "Let Codex review your team’s work", "Claude accounts",
        "Connected folders (repositories)", "Your name in Git", "Memory folder", "Updates",
        "Your name", "Splash screen", "Bust a bug!", "Request a feature",
      ],
      "round 27's table, column by column, then the foot band"
    );
    await page.close();
    return rows.join(" -> ");
  });

  // ---- 9-11. one state, two entrances -------------------------------------------------

  // ---- 8a. ONE SETTING, ONE NAME ------------------------------------------------------

  await run.check("8a  the technical view has ONE switch, and it is called the same thing as its own modal", async () => {
    // Ray's candidate-.12 defect A, and it is a defect about a PERSON rather than about code:
    // the gear panel said "Technical view" with a "Show it" checkbox, the settings menu said
    // "Techy Mode" with a toggle, and clicking that toggle opened a modal headed "Turn off the
    // technical view" (frame 13). One setting, three surfaces, two names. Round 27 (product
    // hunt 57 finding 18) went further: the gear and its "Show it" checkbox are gone, so there
    // is ONE switch, in the one Settings panel.
    //
    // THE SURFACES ARE COMPARED WITH EACH OTHER, not each against a literal: pinned literals
    // would pass happily while they drifted apart the next time one of them was edited.
    const page = track(await openApp(browser));
    await openMenu(page);
    const menuRow = (await page.textContent("#set-techy-label")).trim();
    const doors = await page.evaluate(() => ({
      switches: document.querySelectorAll("#set-techy").length,
      gone: ["techy-default", "rail-settings", "assertiveness-popover"].filter((id) => document.getElementById(id)),
      hint: !!document.querySelector("#set-menu #techy-hint"),
    }));
    assertEqual(doors, { switches: 1, gone: [], hint: true },
      "one switch, with the Preferences hint under it, and no second door");
    // The other door is the scope modal's heading, written by `renderTechyScope`. It is read
    // from the source rather than driven, because reaching it needs a live conversation and
    // this suite runs on the static page; the sentence is the thing under test either way.
    const modal = MAIN_JS.match(/techyScopeTitleEl\.textContent = on \? "([^"]+)" : "([^"]+)"/);

    assert(!/techy/i.test(menuRow), `a person-facing name must not be ${JSON.stringify(menuRow)}`);
    assert(modal, "renderTechyScope's two headings were not found in main.js");
    for (const heading of [modal[1], modal[2]]) {
      assert(
        heading.toLowerCase().includes(menuRow.toLowerCase()),
        `the modal says ${JSON.stringify(heading)} about a control called ${JSON.stringify(menuRow)}`
      );
    }
    await page.close();
    return `one switch, ${JSON.stringify(menuRow)}; the modal says ${JSON.stringify(modal[2])}`;
  });

  await run.check("9  the accelerator is the APP's — preventDefault, and it moves the stored number", async () => {
    const page = track(await openApp(browser));
    const steps = rustFontSteps();
    const before = await page.evaluate(() => window.RichTheme.scale());
    assertEqual(before, 100, "starts at the size the type scale is authored at");

    // Did the PAGE take the keystroke? If it did not call preventDefault, the webview's own
    // zoom is free to run: it would scale the whole document including fixed chrome, would
    // not persist, and would be invisible to the Text size row.
    const prevented = await page.evaluate(() => {
      let seen = null;
      const spy = (e) => {
        if ((e.metaKey || e.ctrlKey) && e.key === "=") seen = e.defaultPrevented;
      };
      window.addEventListener("keyup", () => {}, false);
      document.addEventListener("keydown", spy, false);
      window.__readPrevented = () => seen;
      return true;
    });
    assert(prevented, "spy installed");
    const grew = await invokeCount(page, "set_font_scale");
    await page.keyboard.down("Meta");
    await page.keyboard.press("=");
    await page.keyboard.up("Meta");
    await afterInvoke(page, "set_font_scale", grew);
    assertEqual(
      await page.evaluate(() => window.__readPrevented()),
      true,
      "the app did NOT call preventDefault on ⌘= — the webview's zoom is free to run and the two controls will disagree"
    );

    const after = await page.evaluate(() => window.RichTheme.scale());
    assertEqual(after, steps[steps.indexOf(100) + 1], "one step up the ladder config.rs declares");
    const root = await page.evaluate(() => getComputedStyle(document.documentElement).fontSize);
    assertEqual(root, 16 * (after / 100) + "px", "and the ROOT font size followed: 16 x " + after + "/100");
    const stored = await page.evaluate(() => JSON.parse(window.localStorage.getItem("richos-mock-config")));
    assertEqual(stored.font_scale, after, "...and it is durable, not just painted");

    const reset = await invokeCount(page, "set_font_scale");
    await page.keyboard.down("Meta");
    await page.keyboard.press("0");
    await page.keyboard.up("Meta");
    await afterInvoke(page, "set_font_scale", reset);
    assertEqual(await page.evaluate(() => window.RichTheme.scale()), 100, "⌘0 resets");
    await page.close();
    return "⌘= 100 -> " + after + "% (root " + root + "), durable, and ⌘0 resets";
  });

  await run.check("10  the Text size row and the shortcut are ONE state, not two that agree", async () => {
    const page = track(await openApp(browser));
    await openMenu(page);
    const up = await invokeCount(page, "set_font_scale");
    await page.click("#font-up");
    await afterInvoke(page, "set_font_scale", up);
    const viaRow = await page.evaluate(() => ({
      scale: window.RichTheme.scale(),
      reading: document.getElementById("font-val").textContent,
    }));
    assertEqual(viaRow.reading, viaRow.scale + "%", "the row shows what the state is");

    // Now move it by KEYSTROKE while the row is on screen. A second state would not follow.
    const down = await invokeCount(page, "set_font_scale");
    await page.keyboard.down("Meta");
    await page.keyboard.press("-");
    await page.keyboard.up("Meta");
    await afterInvoke(page, "set_font_scale", down);
    const after = await page.evaluate(() => ({
      scale: window.RichTheme.scale(),
      reading: document.getElementById("font-val").textContent,
    }));
    assertEqual(after.scale, 100, "the keystroke moved it back down");
    assertEqual(after.reading, "100%", "and the ROW followed the keystroke — one state, two entrances");
    await page.close();
    return "row 100->110 by click, 110->100 by ⌘-, and the row's reading tracked both";
  });

  await run.check("11  Technical view: the ONE switch asks through the choice §7.1 asks for, both ways", async () => {
    // THIS CHECK WAS RED ON MAIN, AND IT WAS THE CHECK THAT WAS WRONG. It asserted that
    // clicking `#set-techy` calls `set_techy_default` — the behavior this row had until
    // `68a91f0e`. Since 2026-09-18 every scope-ambiguous switch goes through
    // `requestTechyToggle` (main.js), and inside a conversation that opens the CEO's
    // three-way sheet instead: "when the user toggles the techy mode on while being inside a
    // conversation thread, then there should be something like a radio button choice between
    // 3 choices ... And the first choice should be preselected". The behavior IS the ruling,
    // so the assertion moved to it. `set_techy_default` is still what the row writes with no
    // conversation open, and the second half below is that branch, measured rather than
    // assumed.
    //
    // ROUND 27 made this the technical view's ONLY switch (the rail gear's "Show it" checkbox
    // is gone), so what this owns is that the switch reaches the store and shows what the
    // store took, in both directions. `techy.js` 23-29 own the sheet's contents.
    const page = track(await openApp(browser));
    await openThread(page, "acme");
    await openMenu(page);

    // ---- the switch asks, and the answer is what it shows next time ---------------------
    const directWrite = await invokeCount(page, "set_techy_default");
    await page.click("#set-techy");
    assertEqual(
      await confirmTechyScope(page),
      "all-companies",
      "his FIRST option is the one the sheet opens on — the whole act is a click and a confirm"
    );
    assertEqual(
      await invokeCount(page, "set_techy_default"),
      directWrite,
      "the switch wrote the global default DIRECTLY from inside a conversation, which is " +
        "the path §7.1 replaced: with one open, every switch asks where it applies"
    );
    await openMenu(page);
    assertEqual(
      await page.evaluate(() => document.getElementById("set-techy").checked),
      true,
      "the switch shows what the store took"
    );

    // ---- and back the other way, which is the direction a one-way binding still passes ---
    await page.click("#set-techy");
    assertEqual(await confirmTechyScope(page), "all-companies", "the same first option, in the off direction");
    await openMenu(page);
    assertEqual(
      await page.evaluate(() => document.getElementById("set-techy").checked),
      false,
      "and it shows the off the store took — a binding that only works one way is two states with a lag"
    );
    await page.close();

    // ---- with NO conversation open, the row writes the only tier that has a referent -----
    // `{ chosenEntity: null }` is the one boot with nothing active. Two of the three options
    // would have no referent there, so `requestTechyToggle` applies the global default
    // directly — asserted here because "it asks, except when it cannot" is exactly the kind
    // of exception that rots into "it asks sometimes".
    const bare = track(await openApp(browser, { preset: { chosenEntity: null } }));
    await openMenu(bare);
    const before = await invokeCount(bare, "set_techy_default");
    const reread = await invokeCount(bare, "techy_mode");
    await bare.click("#set-techy");
    await afterInvoke(bare, "set_techy_default", before);
    await afterInvoke(bare, "techy_mode", reread);
    assert(await bare.locator("#techy-scope").isHidden(), "nothing to scope, so nothing is asked");
    assertEqual(
      await bare.evaluate(() => document.getElementById("set-techy").checked),
      true,
      "and the row shows what was taken"
    );
    await bare.close();
    return "in a conversation: switch -> sheet (first option preselected) -> store, on and off; with none open: straight to set_techy_default, no sheet";
  });

  await run.check("11b  the splash off switch is in the foot band of the one Settings panel, once (CEO 2026-10-06 item 7, round 27)", async () => {
    // A GUARD-RAIL, and it has changed shape three times: READ THIS BEFORE CHANGING IT AGAIN.
    //
    // It was written for one state behind two doors (the rail gear popover AND the universal
    // menu), then Ray's candidate .11 defect 3.4 cut it to one door. The CEO's feedback of
    // 2026-10-06, item 7: "Move the splash screen toggle to the general settings section (i.e.
    // remove from quick settings because ideally I don't want them to turn it off)". Round 27
    // (product hunt 57 finding 18) made Settings ONE panel, so there is no quick tier; the
    // switch sits in the band across its foot, away from the first things it shows.
    //
    // So this pins: the switch exists ONCE, in the foot band and not in the first column, it
    // moves the state both ways, and the mirror `splash.js` reads synchronously on the next
    // launch agrees with it.
    const page = track(await openApp(browser));
    await openMenu(page);
    assertEqual(
      (await page.textContent(".setmenu-title")).trim(),
      "Settings",
      "the settings panel does not name itself, so the panel a person lands on is unlabeled"
    );
    const where = await page.evaluate(() => {
      const box = document.getElementById("splash-enabled");
      return {
        copies: document.querySelectorAll("#splash-enabled, #set-splash, #set-splash-row").length,
        inFoot: !!box.closest("#set-menu .sm-foot"),
        inFirstColumn: !!box.closest("#set-menu .sm-col:first-child"),
      };
    });
    assertEqual(where, { copies: 1, inFoot: true, inFirstColumn: false },
      "one splash switch, in the panel's foot band, away from the first things it shows");
    const title = await page.evaluate(() => {
      let n = document.getElementById("splash-enabled").closest(".popover-option").previousElementSibling;
      while (n && !n.classList.contains("popover-title")) n = n.previousElementSibling;
      return n ? n.textContent.trim() : null;
    });
    assertEqual(title, "Splash screen", "the switch's group does not name the surface it governs");
    const read = () => page.evaluate(() => document.getElementById("splash-enabled").checked);
    assertEqual(await read(), true, "it starts on, which is the shipped default");

    // `set_splash_enabled` is NOT implemented by the mock; `main.js` fires it and swallows the
    // rejection on purpose, because the local mirror is what decides the next launch. The
    // ledger records a completion either way, so this waits for the round trip to be over.
    const offCall = await invokeCount(page, "set_splash_enabled");
    await page.click("#splash-enabled");
    await afterInvoke(page, "set_splash_enabled", offCall);
    assertEqual(await read(), false, "the switch did not take his answer");
    assertEqual(await page.evaluate(() => window.localStorage.getItem("richos.splash.enabled")), "false",
      "the mirror splash.js reads disagrees with the switch");

    const onCall = await invokeCount(page, "set_splash_enabled");
    await page.click("#splash-enabled");
    await afterInvoke(page, "set_splash_enabled", onCall);
    assertEqual(await read(), true, "it does not come back on");
    const mirrored = await page.evaluate(() => window.localStorage.getItem("richos.splash.enabled"));
    assert(mirrored !== "false", "the local mirror disagrees with the switch: " + mirrored);
    await page.close();
    return "once, in the foot band, both directions observed, the mirror agrees";
  });

  // ---- 12-13. the type scale itself ------------------------------------------------------

  await run.check("12  every font-size in the SHIPPED CSS scales, and every unit is measured against §15's floor", async () => {
    // "THE SHIPPED CSS" MEANT ONE STYLESHEET OUT OF FOUR. This read `STYLE_CSS` — a single
    // `fs.readFileSync` of `style.css` — under a title claiming every font-size the app
    // ships. `index.html` links four: `style.css`, `home.css`, `splash.css` and
    // `fonts/fonts.css`. The list is derived from `lib/ui-sources.js` now, and widening it
    // turned this check RED on the first run, which is the gate working:
    //
    //     OLD (style.css)          2 px font-sizes, both declared    -> passed
    //     NEW (the four shipped)  16 px font-sizes                   -> FAILED on 14 of them
    //
    // 13 of the 14 are `home.css` and 1 is `splash.css`. NEITHER IS A DEFECT IN THE SURFACE
    // and both are recorded below rather than waved through, because a px font-size on the
    // CEO's own landing screen is exactly the thing this check was written to notice.
    //
    // THE KNOB. `--app-font-scale` multiplies the ROOT font size, so a `rem` size follows it
    // and a `px` size does not. A surviving px size is a node that silently refuses to
    // scale, and it is invisible until someone with poor eyesight is looking at the one line
    // that did not grow.
    // AND IT ENUMERATED ONE UNIT OUT OF FIVE, which is the half this check got wrong for
    // longer. The floor below read `px.filter(...)` — the PIXEL declarations — under a
    // sentence about §15, and reported "smallest 14px" on a tree whose smallest readable
    // size was 11px in `rem`. Eighteen `rem` sizes sat under the floor and nothing in this
    // directory could see one of them. `cssRules` carries the SELECTOR and the rule's other
    // declarations now, which is what makes both halves below possible: a `rem` resolves
    // against a root size that is itself a declaration in this tree, and an exemption gets
    // checked rather than believed.
    const rules = SOURCES.cssRules("font-size");
    assert(rules.length >= 150, "only " + rules.length + " font-size declarations found — that is not this tree");
    // EIGHT SINCE ROUND 41 (`terminal-and-git.css`: 29 font-sizes, all `rem`, none below 1rem, so
    // no px declaration and no floor exception; the walk below reads it like the rest).
    // SEVEN SINCE THE INSTRUCTIONS UPDATE (`instructions-update.css`, round 37: every font-size
    // in `rem`, none below 1rem, so no px declaration and no floor exception; the walk below
    // reads it like the rest).
    // SIX SINCE THE DICTATION OVERLAY (`dictation-overlay.css`: 11 font-sizes, all `1rem`, so no
    // px declaration and no floor exception; the walk below reads it like the rest). FIVE SINCE 2026-09-29, AND NAMED RATHER THAN COUNTED. `speckle.css` is the design
    // system's speckled-ground stylesheet, copied byte for byte from richos-hq design/system
    // (speckle-app.js says why); it declares no font-size at all today, and the walk below
    // reads it like every other file, so a size added to it is held to the same floor. The
    // list is compared by NAME, so a swap of one file for another cannot pass as "still five".
    assertEqual(
      SOURCES.styleSources(),
      ["fonts/fonts.css", "style.css", "speckle.css", "splash.css", "home.css", "instructions-update.css", "terminal-and-git.css", "dictation-overlay.css"],
      "the shell links a different set of stylesheets from the eight this check was measured against"
    );
    // THE TWO DERIVATIONS ARE JOINED, the way `lib/harness.js` joins its two PNG decoders: a
    // brace-counting walk and the property regex must see the same declarations, or the walk
    // is skipping some of the tree and every count below is about a subset nobody named.
    assertEqual(
      rules.length,
      SOURCES.cssDeclarations("font-size").length,
      "the rule walk and the property scan disagree about how many font-size declarations this tree has"
    );

    const px = rules.filter((d) => /^[0-9.]+px$/.test(d.value));

    /// THE DECLARATION, AND WHY IT IS NOT THE TYPED LIST AGAIN. It is per FILE, and it names
    /// an EXACT COUNT and a FLOOR, both recomputed from the tree on every run. A px size
    /// added to any of these files changes the count and fails; a file dropping to zero px
    /// sizes fails as stale; a size authored below §15's floor fails whatever the count says.
    /// So being on this list buys a reason being on the record, not silence.
    const DECLARED = {
      "style.css": {
        count: 2,
        why:
          "two fixed glyph boxes with no text of their own — the reference this component was " +
          "ported from documents why they are authored in px",
      },
      "home.css": {
        count: 13,
        why:
          "the home screen is the port of `richos-hq/design/mockups/rounds/round-11.1/v1`, whose " +
          "type sizes are the round's own and whose rounds are FROZEN (CLAUDE.md, 'Design Rounds " +
          "Are FROZEN'). Re-authoring 13 sizes on the CEO's landing surface is a design decision " +
          "and his to make, not a gate's. What this check holds instead is the §15 FLOOR, below, " +
          "which the composition clears everywhere.",
      },
      "splash.css": {
        count: 1,
        why:
          "`.splash-line`, and the declaration is DEAD: `splash.js` sets the size inline on every " +
          "composition, so the CSS value never paints. Held equal to the shipped value below so " +
          "the two cannot drift.",
      },
    };

    const byFile = {};
    for (const d of px) (byFile[d.file] = byFile[d.file] || []).push(d);

    // Both directions, the same discipline `lib/ui-sources.js`'s ROLES table uses.
    const undeclared = Object.keys(byFile).filter((f) => !DECLARED[f]);
    assertEqual(undeclared, [], "px font-size(s) in a stylesheet with no declaration: " + undeclared.join(", "));
    const stale = Object.keys(DECLARED).filter((f) => !byFile[f]);
    assertEqual(stale, [], "a declaration names a file that no longer has px font-sizes: " + stale.join(", "));
    for (const f of Object.keys(DECLARED)) {
      assertEqual(
        byFile[f].length,
        DECLARED[f].count,
        f + " has " + byFile[f].length + " px font-size(s), declared " + DECLARED[f].count +
          ": " + byFile[f].map((d) => d.line + "=" + d.value).join(" ")
      );
      assert(DECLARED[f].why.length >= 40, f + ": a declaration needs a reason, not a marker");
    }

    // =====================================================================================
    // §15's FLOOR, IN EVERY UNIT — "nothing readable sits below 14px"
    // =====================================================================================
    //
    // THE ROOT SIZE IS READ OUT OF THE TREE, never typed. `rem` means nothing without it,
    // and it is a declaration in this same stylesheet — so a build that moved the root from
    // 16px to 15px would move every `rem` size with it, and this resolution follows rather
    // than reporting yesterday's pixels.
    const rootDecl = rules.find((d) => /^calc\(\s*[0-9.]+px\s*\*\s*var\(--app-font-scale/.test(d.value));
    assert(rootDecl, "the root font size is not driven by --app-font-scale, so nothing scales");
    const ROOT = parseFloat(rootDecl.value.match(/^calc\(\s*([0-9.]+)px/)[1]);
    assertEqual(rootDecl.selector, "html", "the --app-font-scale root moved off `html` — the rem resolution below assumes it");

    /// One declaration, resolved to the pixels it paints at the 100% scale.
    ///
    /// `em` AND `%` ARE RESOLVED AGAINST A BASE DERIVED FROM THE SELECTOR, not against a
    /// typed table. `.tl-prose .tl-md-h { font-size: 1.06em }` is 1.06 x whatever `.tl-prose`
    /// declares, and `.tl-prose` is right there in the selector — so the base is found by
    /// dropping the last compound and looking the remainder up in this same list. A relative
    /// size whose base cannot be found that way FAILS, with the selector named: an
    /// unresolvable size is a failure to measure, never a pass. (`inherit` is the one value
    /// that introduces no size of its own; it is counted and reported, not silently dropped.)
    const resolve = (d, seen) => {
      seen = seen || [];
      if (seen.indexOf(d.site) >= 0) return { px: null, why: "a circular font-size: " + seen.join(" -> ") };
      const v = d.value.trim();
      let m;
      if ((m = /^([0-9.]+)px$/.exec(v))) return { px: parseFloat(m[1]), unit: "px" };
      if ((m = /^([0-9.]+)rem$/.exec(v))) return { px: parseFloat(m[1]) * ROOT, unit: "rem" };
      if (/^calc\(\s*[0-9.]+px\s*\*\s*var\(--app-font-scale/.test(v)) {
        return { px: parseFloat(v.match(/^calc\(\s*([0-9.]+)px/)[1]), unit: "root" };
      }
      if (v === "inherit") return { px: null, inherits: true };
      if ((m = /^([0-9.]+)(em)$/.exec(v)) || (m = /^([0-9.]+)(%)$/.exec(v))) {
        const factor = m[2] === "%" ? parseFloat(m[1]) / 100 : parseFloat(m[1]);
        const parts = d.selector.split(/\s+/);
        if (parts.length < 2) {
          return { px: null, why: d.selector + " is " + v + " with no ancestor in its own selector to resolve against" };
        }
        const ancestor = parts.slice(0, -1).join(" ");
        const base = rules.find((r) => r.selector === ancestor && r.value !== "inherit");
        if (!base) return { px: null, why: v + " resolves against `" + ancestor + "`, which declares no font-size" };
        const b = resolve(base, seen.concat(d.site));
        if (b.px === null) return { px: null, why: "its base " + ancestor + " is unresolvable: " + (b.why || "inherit") };
        return { px: factor * b.px, unit: m[2], base: ancestor + " = " + b.px + "px" };
      }
      return { px: null, why: "a font-size value this resolution does not know" };
    };

    const resolved = rules.map((d) => Object.assign({ d }, resolve(d)));
    const unresolvable = resolved.filter((r) => r.px === null && !r.inherits);
    assertEqual(
      unresolvable.map((r) => r.d.site + " (" + r.d.selector + ") = " + r.d.value + " — " + r.why),
      [],
      "a font-size this check cannot resolve to pixels: that is a failure to measure, never a pass"
    );
    const inherited = resolved.filter((r) => r.inherits);

    /// §15's TWO EXEMPT CLASSES, BY SELECTOR, WHERE A REVIEWER SEES THEM — and each one is
    /// CHECKED rather than taken on the word of the person who typed it.
    ///
    ///   `caps`  an all-caps micro-label. The claim is about the RULE, so the rule answers
    ///           it: it must declare `text-transform: uppercase` beside its own font-size.
    ///           A label that stops being uppercase stops being exempt, in the same edit.
    ///   `glyph` a symbol with no text of its own. Checked IN THE PAGE below: every instance
    ///           carries at most two characters and not one letter or digit.
    ///   `mono`  the monogram in the rail's identity circle — two uppercase initials, and
    ///           `config.rs`'s `initials_from` is what makes that true rather than this list.
    ///
    /// An undeclared size under the floor fails. A declaration for a size that is no longer
    /// under the floor fails as STALE — which is what would have caught this list going quiet
    /// after the three labels raised in the commit before this one.
    const BELOW_FLOOR = {
      ".send-glyph": { kind: "glyph", why: "the send arrow inside the send button — the button's accessible name carries the word" },
      ".rail-icon-btn": { kind: "glyph", why: "the rail's icon buttons (the drawer's and the pane's close): one symbol each, each with an aria-label of its own" },
      ".nav-disclosure": { kind: "glyph", why: "the group disclosure triangle in the rail — rotation is the state, and the group's label is beside it" },
      ".nav-status": { kind: "glyph", why: "the per-conversation status mark, a SHAPE (§18: a state is never carried by color alone), with the word in the row's accessible label" },
      ".tl-chevron": { kind: "glyph", why: "the turn's expand/collapse chevron; the row it belongs to carries the readable label" },
      ".tl-activity-mark": { kind: "glyph", why: "the activity row's status shape, the same symbol vocabulary as .nav-status, beside 16px text that says the state" },
      ".tl-tech-chevron": { kind: "glyph", why: "the technical row's expand chevron. Its ink is measured on the painted glass, on the plain ground with the speckle hidden, by contrast.js check 17 (6.46:1 dark, 5.14:1 light) against a 4.5:1 floor, which is stricter than the 3:1 a non-text indicator owes" },
      ".chrome-select-chevron": { kind: "glyph", why: "the `this opens a list` cue on a <select>; aria-hidden and pointer-events:none, so the control's own accessible name is the whole of what is read" },
    };

    const below = resolved.filter((r) => r.px !== null && r.px < 14);
    const undeclaredFloor = below.filter((r) => !BELOW_FLOOR[r.d.selector]);
    assertEqual(
      undeclaredFloor.map((r) => r.d.site + "  " + r.d.selector + " = " + r.d.value + " = " + r.px + "px"),
      [],
      "§15: a font-size under the 14px floor with no declaration. Raise it to 0.875rem, or " +
        "declare it in BELOW_FLOOR with the class that excuses it and why"
    );
    const staleFloor = Object.keys(BELOW_FLOOR).filter((sel) => !below.some((r) => r.d.selector === sel));
    assertEqual(
      staleFloor,
      [],
      "BELOW_FLOOR excuses a selector that is no longer under the floor (or no longer exists) — " +
        "a stale exemption is how a list like this stops meaning anything"
    );
    for (const r of below) {
      const dec = BELOW_FLOOR[r.d.selector];
      assert(dec.why.length >= 40, r.d.selector + ": an exemption needs a reason, not a marker");
      if (dec.kind === "caps") {
        assertEqual(
          (r.d.rule["text-transform"] || "").trim(),
          "uppercase",
          r.d.selector + " is excused as an all-caps micro-label and its own rule does not " +
            "declare `text-transform: uppercase`. The exemption is the claim; this is the claim's proof."
        );
      }
    }

    // ---- and the glyphs are checked WHERE THEY ARE PAINTED ------------------------------
    //
    // A "symbol with no text of its own" cannot be proven from a stylesheet — the text is in
    // the JavaScript that builds the node, and three of these eight are only in the document
    // once the technical view is on. So this half opens the app, takes the CEO's own path to
    // the technical view (the settings row asks WHERE, and the preselected first option is
    // all conversations in all companies — §7.1), and reads every declared glyph off the
    // page. Each one must be reachable — an exemption over a selector nothing renders is an
    // exemption nobody can check — and each instance must be at most two characters with no
    // letter and no digit in it.
    const page = track(await openApp(browser, { stored: { theme: "dark", font_scale: 100, user_name: "Alex Booster" } }));
    await openThread(page, "acme");
    await techyOnEverywhere(page);
    // `.nav-disclosure` is drawn only by the Archived group since round 32 gave every company
    // heading its own chevron control, so one chat is archived here, by its own ⋯ menu, to put
    // that group (and its triangle) on the page.
    await page.hover('.nav-thread[data-thread-id="hiring"]');
    await page.click('.nav-thread[data-thread-id="hiring"] + .nav-thread-more');
    await page.click('#thread-menu .menu-item:has-text("Archive")');
    await page.waitForSelector(".nav-group--archived .nav-disclosure");
    const glyphSelectors =Object.keys(BELOW_FLOOR).filter((s) => BELOW_FLOOR[s].kind !== "caps");
    const painted = await page.evaluate((sels) => {
      const out = {};
      for (const sel of sels) {
        out[sel] = [...document.querySelectorAll(sel)].map((n) =>
          [...n.childNodes].filter((c) => c.nodeType === 3).map((c) => c.textContent).join("").trim()
        );
      }
      return out;
    }, glyphSelectors);
    const unreachable = glyphSelectors.filter((s) => !painted[s].length);
    assertEqual(
      unreachable,
      [],
      "declared exempt as a glyph and NOT ON THE PAGE this check drives, so the claim cannot be " +
        "checked: reach it from here or stop excusing it"
    );
    const notGlyphs = [];
    for (const sel of glyphSelectors) {
      for (const text of new Set(painted[sel])) {
        const forbidden = BELOW_FLOOR[sel].kind === "mono" ? /[\p{N}]/u : /[\p{L}\p{N}]/u;
        const capsOk = BELOW_FLOOR[sel].kind !== "mono" || text === text.toUpperCase();
        if ([...text].length > 2 || forbidden.test(text) || !capsOk) notGlyphs.push(sel + " renders " + JSON.stringify(text));
      }
    }
    assertEqual(
      notGlyphs,
      [],
      "§15: declared exempt as a symbol, and painting something a person reads as text"
    );
    const glyphCount = glyphSelectors.reduce((a, s) => a + painted[s].length, 0);
    await page.close();

    // THE ONE DUPLICATE, CHECKED RATHER THAN ASSERTED. `splash.css`'s declaration is
    // overridden by `splash.js`'s inline `LINE_SIZE` on every composition, so the two are
    // free to disagree silently — and they DID, by 6px, with the stylesheet holding a value
    // below the floor above. Held equal here so the next person to change one changes both.
    const splashLine = px.filter((d) => d.file === "splash.css");
    const inline = fs.readFileSync(SOURCES.abs("splash.js"), "utf8").match(/LINE_SIZE\s*=\s*"([0-9.]+px)"/);
    assert(inline, "splash.js no longer names LINE_SIZE — this comparison has nothing to hold");
    assertEqual(
      splashLine.map((d) => d.value),
      [inline[1]],
      "splash.css's .splash-line disagrees with the size splash.js actually paints (" + inline[1] + ")"
    );

    // THE SMALLEST READABLE SIZE IN THE SHIPPED CSS, which is the number this check could not
    // say until it could resolve a `rem`. Everything under the floor is an excused symbol or
    // micro-label, so the minimum over what is LEFT is the real answer.
    const readable = resolved.filter((r) => r.px !== null && !BELOW_FLOOR[r.d.selector]);
    const smallest = Math.min(...readable.map((r) => r.px));
    const smallestSites = readable.filter((r) => r.px === smallest).map((r) => r.d.selector);

    return (
      rules.length + " font-size declaration(s) across " + SOURCES.styleSources().join(" + ") +
      "; " + px.length + " in px, every one declared (style.css 2, home.css 13, splash.css 1). " +
      "RESOLVED IN EVERY UNIT against a root of " + ROOT + "px: " + readable.length + " readable, " +
      "smallest " + smallest + "px (" + [...new Set(smallestSites)].slice(0, 3).join(", ") + ") — at §15's floor; " +
      below.length + " under it, every one declared AND checked: " +
      below.filter((r) => BELOW_FLOOR[r.d.selector].kind === "glyph").length + " icon glyph(s) and " +
      below.filter((r) => BELOW_FLOOR[r.d.selector].kind === "mono").length + " monogram, read off the page as " +
      glyphCount + " painted instance(s) carrying no word; " +
      below.filter((r) => BELOW_FLOOR[r.d.selector].kind === "caps").length +
      " all-caps micro-label(s), each proven uppercase by its own rule. " +
      inherited.length + " `inherit`, which introduces no size of its own"
    );
  });

  await run.check("13  the whole interface scales, not a subset of it", async () => {
    const page = track(await openApp(browser));
    const sample = () =>
      page.evaluate(() => {
        const g = (sel) => {
          const e = document.querySelector(sel);
          return e ? parseFloat(getComputedStyle(e).fontSize) : null;
        };
        return { prose: g(".tl-prose"), rail: g(".nav-thread"), kbd: g(".rail-kbd"), group: g(".nav-group-label") };
      });
    const at100 = await sample();
    assertEqual(at100.prose, 18, "§15: Rich's prose is 18px at 100%");
    assert(at100.rail >= 16, "the readable floor is 16px — the rail thread title is " + at100.rail);
    assert(at100.kbd >= 14, "the skippable floor is 14px — the ⌘K keycap is " + at100.kbd);

    const scaled = await invokeCount(page, "set_font_scale");
    await page.keyboard.down("Meta");
    await page.keyboard.press("=");
    await page.keyboard.up("Meta");
    await afterInvoke(page, "set_font_scale", scaled);
    const at110 = await sample();
    for (const k of Object.keys(at100)) {
      assert(
        Math.abs(at110[k] - at100[k] * 1.1) < 0.05,
        k + " did not scale with the knob: " + at100[k] + " -> " + at110[k] + ", expected " + at100[k] * 1.1
      );
    }
    await page.close();
    return "at 100%: prose 18, rail " + at100.rail + ", keycap " + at100.kbd +
      "; at 110% every one of the four moved by exactly 1.1x";
  });

  // ---- 14. the accelerator is claimed in the shell, not merely by the page ---------------

  await run.check("14  the Tauri shell does not hand ⌘+/- to the webview", async () => {
    const win = (TAURI_CONF.app.windows || [])[0] || {};
    assertEqual(
      win.zoomHotkeysEnabled,
      false,
      "`zoomHotkeysEnabled` must be present and false in tauri.conf.json. It is the seam: with it " +
        "on, Tauri injects a webview zoom polyfill on macOS/Linux and sets WebView2's " +
        "IsZoomControlEnabled on Windows — a second, non-persistent font size the Text size row " +
        "knows nothing about. Its DEFAULT is already false; it is written down so that it is a " +
        "decision rather than a default nobody chose."
    );
    return "zoomHotkeysEnabled: false, claimed explicitly rather than inherited";
  });

  // ---- 15-16. the wordmark, and whose rail this is ---------------------------------------

  await run.check("15  the wordmark replaced 'My Company', and carries the approved treatment", async () => {
    assert(
      /id="rail-wordmark"/.test(INDEX_HTML),
      "the wordmark is not in the rail header"
    );
    // THE DARK WALK IS SEEDED, NOT INHERITED. This check names its two samples `dark` and
    // `light` and asserts an exact ink for each, so which theme it opens in is load-bearing
    // — and since §63 the unseeded default is `system`, which under this harness's light OS
    // would hand the `dark` sample the light palette. The preference is stated rather than
    // assumed: a check about the WORDMARK must not also be a check about the default.
    const page = track(
      await openApp(browser, { stored: { theme: "dark", font_scale: 100, user_name: null }, mirror: "dark" })
    );
    const dark = await page.evaluate(() => {
      const w = document.getElementById("rail-wordmark");
      const c = document.getElementById("rail-company");
      return {
        wordmarkVisible: !!w && w.getBoundingClientRect().width > 0,
        label: w ? w.getAttribute("aria-label") : null,
        role: w ? w.getAttribute("role") : null,
        ink: w ? getComputedStyle(w).color : null,
        // THE SWOOSH. The mark is two-tone by the approved treatment (round-8.1/v0 dark,
        // round-9/v1 light): ink letterforms, signal swoosh. It shipped as a KNOCK-OUT
        // painted `var(--rail-bg)`, which made the app's mark monochrome while the standard
        // it is drawn from has gold in it, and that is exactly what this check exists to
        // stop happening twice.
        swoosh: w ? getComputedStyle(w.querySelector("#arrow")).fill : null,
        companyRendered: !!c && c.getBoundingClientRect().width > 0,
      };
    });
    assert(dark.wordmarkVisible, "the mark does not render");
    // THE NAME GREW A DESTINATION ON 2026-09-01, and that is the correct name rather than a
    // relaxed assertion. The CEO: "in the regular app UI a click on the logo (in the upper
    // left corner) brings the user back to the home screen." So the mark is no longer an
    // image carrying a name — `home.js` promotes it to `role="button"` at runtime — and a
    // button whose accessible name is only the product's own name tells a screen-reader user
    // nothing about what pressing it does. What still has to hold, and is what this checks,
    // is that it is announced AND that it still leads with the product's name.
    assert(
      /^RichOS\b/.test(dark.label || ""),
      "the mark must still be announced as RichOS: " + dark.label
    );
    assert(
      /home screen/i.test(dark.label || ""),
      "the mark is a control now and must say where it goes: " + dark.label
    );
    assertEqual(dark.role, "button", "the mark is the way back to the home screen and must say so");
    assertEqual(dark.companyRendered, false, "'My Company' must no longer be rendered here (§15)");
    assertEqual(dark.ink, "rgb(223, 228, 238)", "inked with the dark theme's own ink");
    assertEqual(
      dark.swoosh, "rgb(194, 163, 92)",
      "the swoosh inside the R is the ruled signal #C2A35C, not a knock-out — 7.88:1 on the " +
        "dark rail, non-text floor 3:1"
    );
    await openMenu(page);
    const crossed = await invokeCount(page, "set_theme");
    await page.click('.theme-opt[data-th="light"]');
    await afterInvoke(page, "set_theme", crossed);
    const light = await page.evaluate(() => {
      const w = document.getElementById("rail-wordmark");
      return {
        ink: getComputedStyle(w).color,
        swoosh: getComputedStyle(w.querySelector("#arrow")).fill,
      };
    });
    assertEqual(light.ink, "rgb(12, 19, 34)", "and re-inked when the theme crosses over");
    // THE EXPECTATION FOLLOWS THE RULING, and it did not for one commit. 6fd6fe7 restored
    // the CEO ruled light signal in `index.html` and rewrote this assertion PROSE to say so,
    // but left the VALUE at the struck-darker one the ruling replaced — so main shipped a
    // check asserting a value the app no longer draws, and a value the lead has since said
    // is gone from the whole app. #9C7C34 is rgb(156, 124, 52).
    assertEqual(
      light.swoosh, "rgb(156, 124, 52)",
      "and the swoosh crosses over too — the CEO ruled light signal #9C7C34, which is what " +
        "index.html sets and what the shipped light asset carries."
    );
    await page.close();
    return `wordmark present and announced as "${dark.label}" (role=${dark.role}); company label ` +
      `gone; ink #DFE4EE -> #0C1322 and swoosh ${dark.swoosh} -> ${light.swoosh} across the theme`;
  });

  await run.check("16  the foot of the rail is HIS — and says nothing it does not know", async () => {
    // The CEO's correction to round 10.1, and the honest-unset half is the load-bearing one:
    // there was no user-name preference in this product until today, so this is the state
    // almost every install is in.
    const page = track(await openApp(browser));
    const unset = await page.evaluate(() => {
      const row = document.getElementById("rail-identity");
      return {
        isUnset: row.classList.contains("is-unset"),
        initials: document.getElementById("rail-initials").textContent,
        label: document.getElementById("rail-user-name").textContent,
        richLabelGone: !document.querySelector(".rail-rich-label"),
      };
    });
    assert(unset.isUnset, "a fresh install has no name and must say so");
    assertEqual(unset.initials, "", "the circle carries NO letters — not invented ones, and not '??'");
    assert(!/\?\?/.test(unset.label), "and the label is not '??' either: " + unset.label);
    assertEqual(unset.label, "Set your name", "it says what is true, and it is an offer rather than a dead end");
    assert(unset.richLabelGone, "Rich's nameplate is gone from the CEO's own rail");

    // Round 27: the unset nameplate opens the ONE Settings panel on the name field.
    await page.click("#rail-identity");
    await overlayOpen(page, "#set-menu");
    assertEqual(await page.evaluate(() => document.activeElement && document.activeElement.id), "user-name-input",
      "the nameplate opens Settings with the name field ready");
    await page.fill("#user-name-input", "Alex Booster");
    // The `change` handler is `await set_user_name` and THEN `await renderUserIdentity()`,
    // which re-reads `get_user_identity`. The second read is what paints the initials and the
    // label this check asserts on, so it is the second read that is waited for — waiting for
    // the write alone would sample the rail one round trip early.
    const reread = await invokeCount(page, "get_user_identity");
    await page.dispatchEvent("#user-name-input", "change");
    await afterInvoke(page, "get_user_identity", reread);
    const named = await page.evaluate(() => ({
      isUnset: document.getElementById("rail-identity").classList.contains("is-unset"),
      initials: document.getElementById("rail-initials").textContent,
      label: document.getElementById("rail-user-name").textContent,
    }));
    assertEqual(named.initials, "AB", "first and last, two letters — the pattern his Codex app uses");
    assertEqual(named.label, "Alex Booster", "with the name beside it");
    assertEqual(named.isUnset, false, "and the row stops being an offer once it is a nameplate");
    await page.close();
    return "unset: empty circle + 'Set your name'; named: AB + Alex Booster";
  });

  // ---- 12h. every scrolling surface declares its color scheme (walk 39, D11) -----------------

  await run.check("12h  every scrolling surface draws its scroll bars in the theme's own scheme", async () => {
    // WebKit paints a native scroll bar in the element's computed `color-scheme`, and with none
    // declared that is light in both themes: a #FAFAFA track in dark. The scheme is declared on
    // the root with the theme, so this reads the COMPUTED value off every element that scrolls
    // (overflow auto/scroll, `html` included) with the technical view on, so Under the hood is
    // in the document. The PDF frame is the one declared exception: its page is the file's own
    // white in both themes (§6.9), and a dark scheme there would ask WebKit to darken it.
    for (const theme of ["dark", "light"]) {
      const page = track(await openApp(browser, { stored: { theme, font_scale: 100, user_name: "Alex Booster" } }));
      await openThread(page, "acme");
      await techyOnEverywhere(page);
      const found = await page.evaluate(() => {
        const out = { scrollers: 0, wrong: [] };
        const rootScheme = getComputedStyle(document.documentElement).colorScheme;
        for (const n of [document.documentElement, ...document.querySelectorAll("*")]) {
          const cs = getComputedStyle(n);
          if (!/(auto|scroll)/.test(cs.overflowX + " " + cs.overflowY)) continue;
          if (n.matches && n.matches(".of-pdf")) continue;
          out.scrollers++;
          if (cs.colorScheme !== rootScheme) {
            out.wrong.push((n.id ? "#" + n.id : n.tagName.toLowerCase()) + "." + String(n.className).replace(/\s+/g, ".") + " = " + cs.colorScheme);
          }
        }
        out.rootScheme = rootScheme;
        return out;
      });
      assertEqual(found.rootScheme, theme, theme + ": the root does not declare the theme's color-scheme");
      assert(found.scrollers >= 5, theme + ": only " + found.scrollers + " scrolling surfaces found — that is not this app");
      assertEqual(found.wrong, [], theme + ": a scrolling surface with a color-scheme other than the theme's");
      await page.close();
    }
    return "root declares dark/light with the theme and every scroller on the page inherits it";
  });

  // ---- 17. the evidence ------------------------------------------------------------------

  await run.check("17  every surface, photographed in BOTH themes", async () => {
    const surfaces = [
      // A SHOT OF THE WRONG THREAD IS THE WORST OF THESE TO GET WRONG, because it lands in
      // git as evidence and the next diff cannot tell a regression from a race. Same three
      // facts as everywhere else.
      ["conversation", async (p) => { await p.click('.nav-thread[data-thread-id="hiring"]'); await settleOnThread(p, "hiring"); }],
      ["corrections", async (p) => { await p.click("#nav-corrections"); await overlayOpen(p, "#corrections-overlay"); }],
      ["feedback", async (p) => { await p.click("#nav-feedback"); await overlayOpen(p, "#feedback-overlay"); }],
      ["search", async (p) => { await p.click("#nav-search"); await overlayOpen(p, "#search-overlay"); }],
      // Round 27: the gear's Preferences is gone; everything it held is in this one panel.
      ["settings-menu", async (p) => { await openMenu(p); }],
    ];
    const made = [];
    for (const theme of ["dark", "light"]) {
      for (const [name, drive] of surfaces) {
        const page = track(
          await openApp(browser, {
            stored: { theme, font_scale: 100, user_name: "Alex Booster" },
            mirror: theme,
          })
        );
        assertEqual(await themeOf(page), theme, name + " did not open in the theme it is labelled with");
        await drive(page);
        // `shot()` settles finite animations of its own accord (`captureSettled`), so this is
        // the paint after the driver rather than a second guess at the same fade.
        await settledPage(page);
        const s = await shot(page, SHOTS + "/10-1-" + name + "-" + theme, { fullPage: false });
        made.push(path.basename(s.file));
        await page.close();
      }
    }
    assertEqual(made.length, 10, "five surfaces, two themes");
    return made.length + " shots: " + made.join(", ");
  });

  // AUDIT D7, 2026-09-17: the Home screen row's own button read "Company butt…" — clipped by
  // an ellipsis nothing in style.css declared. The cause was structural rather than textual:
  // a flex item's automatic minimum width collapses to 0 the instant its `overflow` is not
  // `visible`, and a native `<button>` ships `overflow: hidden` / `text-overflow: ellipsis`
  // from the OS chrome whether or not this file says so — so `.set-row`'s flexbox happily
  // shrank the BUTTON below its own text before it ever touched the label next to it, which
  // could already absorb the pressure by wrapping onto two lines ("Home" / "screen", visible
  // in the same screenshot). The fix is `.set-row > button { flex-shrink: 0; overflow:
  // visible; text-overflow: clip; }` — a container rule, not a text change, so this check
  // reads `scrollWidth`/`clientWidth` rather than any particular string.
  await run.check("17b  no row's own button is silently clipped by an ellipsis", async () => {
    const page = track(await openApp(browser));
    await openMenu(page);
    const rows = await page.evaluate(() => {
      const out = [];
      for (const btn of document.querySelectorAll("#set-menu .set-row > button")) {
        out.push({
          id: btn.id || "(no id)",
          text: btn.textContent,
          scrollWidth: btn.scrollWidth,
          clientWidth: btn.clientWidth,
        });
      }
      return out;
    });
    assert(rows.length > 0, "no button found directly inside a `.set-row` — did the panel's markup move?");
    for (const row of rows) {
      assert(
        row.scrollWidth <= row.clientWidth,
        row.id + " (\"" + row.text + "\") is clipped: scrollWidth " + row.scrollWidth +
          " > clientWidth " + row.clientWidth + " — its own text does not fit its own box"
      );
    }
    const home = rows.find((r) => r.id === "set-home-open");
    assert(home, "#set-home-open (the row audit D7 named) was not among the buttons checked");
    assertEqual(
      home.text,
      "Company buttons…",
      "the Home screen row's button label changed; the check above still has to name what it read"
    );
    await page.close();
    return rows.length + " row button(s) checked, none clipped: " + rows.map((r) => r.id).join(", ");
  });

  await run.check("18  no page errors anywhere in this suite", async () => {
    const errors = allErrors();
    assertEqual(errors, [], "uncaught errors or console errors during the walks above");
    return errors.length + " uncaught or console errors across " + tracker.pageCount() + " page(s)";
  });

  await browser.close();
  const failed = run.report();
  if (failed) {
    console.error("\n" + failed + " check(s) FAILED.");
    process.exit(1);
  }
  console.log("\ntwo lightings, one knob, and the rail is his.");
}

main().catch((e) => {
  console.error(e && e.stack ? e.stack : e);
  process.exit(1);
});

// ---------------------------------------------------------------------------------------
// THE MUTATIONS — every check above was run RED by breaking the shipped source. Each line
// is a mutation that was actually applied and the checks it actually turned red.
// ---------------------------------------------------------------------------------------
//
//  1   theme-boot.js: the pre-paint default `|| "system"` -> `|| "dark"` -> checks 1 and 1c.
//      This is the FIRST-FRAME half: under a light OS the launch flashes midnight blue and
//      then settles ivory, which no settled sample can see. It is why check 1 captures
//      `data-theme` from a MutationObserver installed before any page script rather than
//      reading it at the end. Since §63 the same mutation is caught a second way, by check
//      1's three-copies comparison, which reads the `|| "..."` straight off disk.
//  1b  mock.js: the store's shipped default `theme: "system"` -> `"dark"` -> checks 1, 17.
//      The SETTLED half, and the one that stands in for `Theme::default()` in config.rs.
//  1c  config.rs: `impl Default for Theme` -> `Theme::Dark` -> check 1 (its `rustDefaultTheme`
//      assertion), plus SIX tests in `cargo test -p richos-core`, named in the commit that
//      made this change. A browser cannot watch config.rs run, so the two gates are
//      complementary: this check catches the drift, the Rust tests catch the behavior.
//  1d  theme-boot.js: `resolved()`'s `if (pref === "system")` -> `if (true)`, so the OS is
//      consulted whatever he chose -> checks 1b ("his choice took"), 1c ("he switched to
//      dark"), 15 and 17. The app overrules a lighting he set by hand, which is §63's second
//      sentence failing in the one direction nobody thinks to look.
//
//      THE OBVIOUS MUTATION HERE IS A FALSE ONE, and it was written into this ledger before
//      it was run. Dropping the same `pref === "system"` guard from the `matchMedia` CHANGE
//      LISTENER instead leaves every check green — correctly, because `paint()` calls
//      `resolved()`, which guards it a second time, so removing the listener's guard costs
//      one redundant repaint and changes no answer. A ledger entry nobody executed is a
//      claim about coverage that the coverage does not support; this one was executed and
//      the first version of it is recorded here rather than quietly replaced.
//  1f  home.js: neuter the body of the `RichTheme.onChange` listener that RE-ASSERTS the
//      clamp (`if (state.open && !t.forcedDark) window.RichTheme.forceDark(true)`)
//        -> "§15's clamp: the home screen is dark under a light OS", expected "dark",
//           actual "light".
//
//      THE TWO OBVIOUS MUTATIONS HERE ARE BOTH FALSE, and each was run before this entry
//      was written. Deleting `RichTheme.forceDark(true)` from `show()` leaves every check
//      green, because this walk never RE-ENTERS the home screen — `tests/home.js` owns that
//      path. Deleting the one where the home screen first opens ALSO leaves every check
//      green, and that one is more interesting: the splash drops the clamp when the curtain
//      yields (`splash.js`, "Drop the always-dark clamp as the curtain goes"), so the
//      listener puts it straight back and masks the missing raise entirely. home.js's own
//      comment already says the re-assertion "is NOT belt and braces"; this is that sentence
//      measured from the outside. All three raises have to go before the light-OS home
//      screen turns ivory, and the listener is the one that decides.
//
//      And it fails under a LIGHT OS only. Under a dark OS the break is invisible, because
//      the preference resolves dark anyway — which is why this check walks both, and why it
//      could not have existed before §63: while the default was `dark`, the clamp and the
//      preference agreed on every surface, and either one alone made the walk look right.
//  1e  theme-boot.js: neuter the `mq.addEventListener("change", ...)` body -> check 1b
//      ("the OS went dark and the app did not"). The app detects the OS once, at boot, and
//      never again — which §63's word "detect" does not survive, and which no check that
//      only ever looks at boot could see.
//  2   main.js `syncAppearanceFromBackend`: drop `RichTheme.sync(durable)` -> check 2. The
//      mirror wins, and a preference set on another launch is silently lost.
//  3   settings-button.js `applyTheme`: `if (T.setTheme(pref)) saveTheme(pref)` ->
//      `T.setTheme(pref)` -> check 3. The theme changes on screen and dies at the next
//      launch: the failure a screenshot cannot see.
//  4   splash.js `start()`: delete `RichTheme.forceDark(true)` -> checks 4 and 5. A CEO on
//      light mode gets an ivory settings button floating on a midnight composition.
//  5   settings-button.js `buildMenu`: `if (!T.forcedDark())` -> always append the theme row
//      -> check 5. A theme switch on the one screen the ruling says no switch reaches.
//  6   splash.js `onInput`: drop the `.closest(".settings")` guard -> checks 5 and 6.
//      Opening the settings menu dismisses the curtain, so a bug report started from the
//      opening screen captures the shell instead — exactly what §15 says must never be
//      required.
//  7   style.css `.settings`: `z-index: 300` -> `100` -> check 7.
//      AND THIS ONE TAUGHT THE CHECK SOMETHING. The first version of check 7 was a hit test
//      on six surfaces, and this mutation did NOT turn it red: 100 still clears the app's
//      overlays (60/70), and the opening screen's curtain is `pointer-events: none`, so
//      `elementFromPoint` walks straight through it and returns the button whether the
//      button is above the curtain or buried under it. A hit test cannot speak for the one
//      surface that matters most. Check 7 gained a structural half — `.settings` must
//      out-rank every z-index the app ships, computed from the stylesheets — and the
//      mutation now fires. It also immediately found that `#bug-toast` is 301, which is
//      correct and is why the component's own layers are excluded from the comparison
//      rather than from the rule.
// 7b   NOT A MUTATION — THE SHIPPED SOURCE TURNED IT RED, 2026-09-05. The structural half
//      above read `style.css` and `splash.css` under a comment saying "every z-index the app
//      ships", while `index.html` links a THIRD stylesheet. Deriving the list from
//      `lib/ui-sources.js` put `home.css` in front of it for the first time and the check
//      failed on the spot: `.home-prefs` is `z-index: 340` against `.settings`'s 300.
//      That is a false positive of the RULE rather than a defect in the surface, and the
//      distinction is the whole of the fix. §15 requires the settings BUTTON to be
//      reachable from every screen; `.home-prefs` is the modal dialog the menu's own Home
//      screen row opens (`wireHome()` closes the menu, then calls `home.open()`), which is
//      the same relationship `#bug-toast` already had. So it joins the owned list — and the
//      ownership claim is now CHECKED rather than asserted, because a list that silences a
//      red check is a mute button unless it costs something to be on it: the run drives the
//      real menu and proves the Bust a bug control and the `#set-home-open` row are inside
//      `#set-menu`, and that the dialog that row opens is `role=dialog`, `aria-modal=true`
//      and carries its own Done. An unrelated overlay added to that list to quiet this check
//      would fail those three assertions instead.
//      SUPERSEDED by the review of 66ba53861 (rv-20261010T081721Z-66ba5386-a4d1, finding 2):
//      the "false positive" was the defect. Its scrim covered Settings, and a press there closed
//      the panel instead of opening the menu. `.home-prefs` left the owned list, is 240 and not
//      aria-modal, and check 7c below holds every overlay to the same rule.
// 7c   NOT A MUTATION — THE SHIPPED SOURCE TURNED IT RED. Run against 66ba53861's ui/ with only
//      this file and `lib/overlays.js` copied in, check 7c named the sheets other than the two
//      first-run questions that main.js's QUESTION_SELECTOR held Settings back over, their
//      aria-modal declarations and `.home-prefs-panel`'s, and the overlays over which Settings was
//      not on top, not in the tree or would not open its menu; check 7 failed on the same tree at
//      340 against 300. The run's lines are in the commit.
//  8   settings-button.js `buildMenu`: append the Techy row before the font row -> check 8.
//      §15 says Text size sits "directly under the theme switch", and it is one line to get
//      wrong.
//  9   settings-button.js keydown: drop `e.preventDefault()` from the `+`/`=` arm -> check 9.
//      The scale still moves, so every other check stays green; what changes is that the
//      webview's own zoom is free to run underneath it.
//  10  settings-button.js `mount`: disable the `T.onChange(...)` subscription -> checks 3
//      and 10. The menu stops tracking state moved by anything but its own buttons.
//      THIS ONE ALSO CHANGED THE SOURCE. The first attempt was "delete the `paint()` inside
//      `stepFont`", and it did not turn anything red — because `T.stepScale` paints and
//      notifies, and this file is a subscriber, so that call was dead code that LOOKED
//      load-bearing. It is now deleted from `stepFont`, `resetFont` and `applyTheme`, and
//      the subscription is the single path, which is what makes check 10 able to fail.
//  11  main.js `requestTechyToggle`: drop the `if (!activeThreadId)` guard so every switch
//      writes `set_techy_default` directly -> checks 11 AND 12. This is the behavior check 11
//      used to ASSERT, before §7.1; the sheet never opens and the check waits 30s for it and
//      says so. It reds 12 as well, deliberately not hidden: 12's glyph half reaches the
//      technical view through the same door, because that door is the CEO's own path to it
//      and a second private entrance would be a check measuring a state he cannot reach.
//  11b NOT A MUTATION — THE SHIPPED SOURCE TURNED IT RED, and the CHECK was the thing that
//      was wrong. Check 11 was red on main from `68a91f0e` to 2026-09-18 asserting the
//      pre-§7.1 behavior (`page.waitForFunction: Timeout 10000ms exceeded` on a
//      `set_techy_default` that no longer happens inside a conversation). A red check whose
//      assertion has been overtaken by a ruling is not evidence of a defect; it is a check
//      that has stopped reading the product. The ruling is the specification, so the
//      assertion moved to it — and the branch that DOES still write the default (no
//      conversation open) is now asserted too, so the exception cannot rot into "it asks
//      sometimes".
//  12  style.css `.rail-company`: `1rem` -> `13px` -> check 12.
//  12b style.css `html`: `calc(16px * var(--app-font-scale, 1))` -> `16px` -> checks 9, 12,
//      13. Every size is still rem and NONE of them moves — the failure that looks most
//      like success.
//  12c NOT A MUTATION — THE SHIPPED SOURCE TURNED IT RED. Check 12 read `STYLE_CSS`, one
//      `readFileSync` of `style.css`, under a title claiming "every font-size in the shipped
//      CSS". `index.html` links four stylesheets. Deriving the list from
//      `lib/ui-sources.js` took it from 2 px font-sizes to 16, and one of the 14 it had
//      never been able to see was BELOW §15's floor:
//
//          §15: a font-size below the 14px floor in the shipped CSS
//          expected []
//          actual   ["splash.css:189 = 12px"]
//
//      `.splash-line` — the only text the opening screen has. It was dead: `splash.js` sets
//      the size inline from `LINE_SIZE` on every composition, so 18px is what paints and
//      `tests/splash.js` check 6 correctly measured 18px on the rendered frame. The
//      stylesheet had simply been wrong, and unreadably wrong, since the port, in a file no
//      gate opened. Fixed at `splash.css:198`, and check 12 now holds the declaration EQUAL
//      to `splash.js`'s `LINE_SIZE` so the two cannot drift apart again in silence.
//  12d home.css `#home-signals .sig .n`: add a second `font-size: 13px` -> check 12. The
//      count moves 13 -> 14 and the check names the file, the line and the value. Run
//      2026-09-05: `home.css has 14 px font-size(s), declared 13`. This is the proof that
//      the widened check can SEE the CEO's landing surface; the old one read `style.css` and
//      would have stayed green through it.
//  12e style.css `.tl-tech-title`: `0.875rem` -> `0.6875rem` (what it shipped as until
//      2026-09-18) -> check 12. The value the OLD check could not see, now named with the
//      pixels it resolves to. Run 2026-09-18:
//
//          §15: a font-size under the 14px floor with no declaration...
//          expected []
//          actual   ["style.css:4346  .tl-tech-title = 0.6875rem = 11px"]
//
//      This is the mutation that matters most here, because it is not hypothetical: it is
//      the shipped state of this stylesheet on `c8bcfe90`, and check 12 reported "smallest
//      14px" over it on every run for two weeks.
//  12f style.css `.nav-group-label`: delete `text-transform: uppercase` -> check 12. The
//      all-caps exemption loses its proof and the check refuses it by name — `expected
//      "uppercase" / actual ""` — rather than going on excusing an 11px label that is no
//      longer a micro-label. The mutation a designer would make without thinking about this
//      file at all.
//  12g appearance.js `BELOW_FLOOR`: re-declare `.nav-group-label` as `kind: "glyph"` ->
//      check 12, off the PAGE rather than the stylesheet: `.nav-group-label renders
//      "Northwind Traders"`, and six more. A false claim about what a node paints cannot be
//      made to stick by typing it in the exemption table.
//  13  style.css `.tl-prose`: `1.125rem` -> `1rem` -> check 13. Rich's answers back below
//      the CEO's stated 18px default.
//  14a tauri.conf.json: `"zoomHotkeysEnabled": true` -> check 14.
//  14b tauri.conf.json: delete the key entirely -> check 14. An absent key is an undecided
//      one even when its default happens to be the right answer.
//  15  style.css `.rail-wordmark`: `color: var(--ink)` -> `color: #dfe4ee` -> check 15. It
//      looks perfect in dark and stays near-white on the ivory rail in light.
//  15b index.html: un-hide `<span id="rail-company">` -> check 15. "My Company" is back in
//      the corner §15 gave to the wordmark.
//  16  main.js `renderUserIdentity`: initials `|| "??"` and a label of "User" -> check 16.
//      Both halves of the thing the CEO's correction explicitly forbids.
//  17  this file: seed every shot walk with `theme: "dark"` -> check 17. Twelve files claim
//      to show two themes while showing one.
//   4b NOT A MUTATION — THE SHIPPED HARNESS TURNED IT RED, AND THREE CHECKS WITH IT. The
//      hold this file used until 2026-09-06 replaced the EXPORTED `RichSplash.yieldNow` and
//      left `start()`'s ceiling armed over the internal one, so every walk that asked for a
//      held curtain had four seconds from `goto` to its last assertion and said so nowhere.
//      Reproduced on this machine at `934f127` by giving the ORIGINAL file 3,300 ms of lag
//      after `goto` — which is roughly what `macos-latest` costs it — and checks 4, 5 AND 6
//      went red together:
//
//          FAIL  4  expected "dark" / actual "light"    (the clamp, dropped by the ceiling)
//          FAIL  5  expected 0 / actual 3               (the app's theme switch, uncovered)
//          FAIL  6  opening the menu did not lift the curtain
//
//      Run 34010691469 on `macos-latest` reported only the first of those, which is what a
//      270 ms window looks like from outside. With `HOLD_CURTAIN` the same three pass at
//      3,300 ms and at 12,000 ms of lag, and 4b is what states the reason in the log.
//
// THE RUST HALF — `Theme::default()` being Dark, an absent key reading as dark rather than
// as a choice, the font ladder snapping instead of resetting, and `initials_from` returning
// `None` rather than a guess — is in `crates/richos-core/src/config.rs`, with its own eight
// mutations recorded in this branch's commit messages. The one worth repeating here is that
// a single test which set BOTH the theme and the scale passed even with `set_theme`'s
// `persist()` removed, because `persist()` rewrites the whole file and the scale's write
// carried the unpersisted theme to disk on its behalf. It is now two tests with one setter
// each.

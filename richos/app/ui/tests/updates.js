// THE UPDATE SURFACE — RICH-TODOs row 12, driven through the real renderer.
//
// The row said, verbatim: *"There is no updater of any kind — no `tauri-plugin-updater`, no
// Sparkle, nothing. The CEO's 'automatically download and install whatever the user needs'
// currently rests on zero infrastructure."* This suite is the browser half of the answer.
//
// WHAT THIS SUITE CANNOT DO, said first so nothing here is mistaken for more than it is.
// It CANNOT prove that an update applies. That is a claim about a signed `.app.tar.gz`
// replacing a running bundle and is proven where it happens — `app/scripts/updater-e2e.sh`,
// which builds 0.1.0 and 0.1.1, serves a manifest, and makes 0.1.0 become 0.1.1 on this
// machine, then tampers with one byte of the artifact and shows the updater REFUSE. Nothing
// in a browser can do either, and a suite that asserted a count it got from `mock.js` would
// be proving the mock.
//
// WHAT IT DOES PROVE IS THE HALF THAT LIVES IN THE PAGE, and the four that matter are:
//
//   * THE SURFACE EXISTS AND IS NOT SILENT. Nine states, each with its own sentence, and
//     "never checked" distinguishable from "checked and you are current". A silent updater
//     is one that can be broken for three months without anyone learning it.
//   * A REFUSED SIGNATURE IS NOT OFFERED A RETRY. Every other failure gets "Try again";
//     this one does not, because retrying a tampered artifact refuses identically and a
//     button that invites someone to keep pressing until a security check passes is the
//     wrong control. This is the check most likely to be broken by a well-meaning edit.
//   * NO PERCENTAGE WITHOUT A DENOMINATOR. A server that sends no `Content-Length` gets an
//     indeterminate bar with NO `aria-valuenow` — a determinate bar over an invented total
//     is a lie that looks like a measurement.
//   * THE BUTTONS DO WHAT THEY SAY. Asserted on the commands actually issued
//     (`__RICHOS_MOCK__.updateCalls()`), not on the button looking pressed.
//   * IT ANNOUNCES ITSELF WITHOUT BEING OPENED, and only when there is something to say
//     (checks 12-14, added 2026-09-04 for §26's placement ruling). Everything above this
//     line was green on the day the CEO said *"the user can't be bothered to hunt for some
//     update button somewhere"* — which is the point: the row was correct and it was inside
//     a menu, so nobody met it. The half that carries the ruling is the ABSENCE half: with
//     nothing to install there is no element at all, proven by count, by the wrapper's own
//     geometry, and by driving the state back after the element has once existed.
//
// Contrast for this surface is NOT here: it is in `contrast.js`'s three `updates-*`
// surfaces, walked in both themes against the same 4.5 / 3.0 floors as everything else.
// Two checkers for one property is how one of them rots.
//
// EVERY CHECK HERE WAS RUN RED ONCE by breaking the shipped source; the mutations are listed
// at the bottom of this file.
//
// Run: node updates.js   (or `npm test` for every suite in this directory)
//      RICHOS_PLAYWRIGHT=/path/to/node_modules/playwright node updates.js

"use strict";

const fs = require("fs");
const path = require("path");
const {
  leaveHome,
  loadPlaywright,
  shot,
  awaitWorkerChipSettled,
  createRun,
  assert,
  assertEqual,
  awaitSettled,
  flushFrames,
  openThread,
  assertOnThread,
  bootSettled,
  shellSettled,
  SLOW_BRIDGE,
  SEED_THEME,
  UI_DIR,
} = require("./lib/harness");

const APP = "file://" + path.join(UI_DIR, "index.html");
const UPDATES_RS = fs.readFileSync(
  path.join(UI_DIR, "..", "src-tauri", "src", "updates.rs"),
  "utf8"
);
const SHOTS = "../shots-updates";

/// A DELIBERATE SLOW RUNNER, on demand: `RICHOS_UPDATES_LAG_MS=120 node updates.js`.
///
/// Same knob and same seam as `contrast.js`'s `RICHOS_CONTRAST_LAG_MS`, and it is what turns
/// "these checks are green here" into "these checks wait for something". Every wait in this
/// file that used to be a number is now a wait on a state, and the way to show that is true is
/// to move the clock and watch nothing change. Zero, and no latency at all, unless it is set.
const LAG_MS = Number(process.env.RICHOS_UPDATES_LAG_MS || 0);

/// A view, with only the interesting fields spelled out at each call site.
function view(over) {
  return Object.assign(
    {
      state: "idle",
      currentVersion: "0.1.0",
      availableVersion: null,
      notes: null,
      pubDate: null,
      downloadedBytes: 0,
      totalBytes: null,
      percent: null,
      failure: null,
      endpoint: "https://updates.richos.invalid/{{target}}/{{arch}}/{{current_version}}",
      endpointIsPlaceholder: true,
      checkedAt: null,
      busy: false,
      busyReason: null,
      unchecked: [],
      readySince: null,
      rollbackVersion: null,
      readyIsRollback: false,
    },
    over
  );
}

async function openApp(browser, theme) {
  const page = await browser.newPage({ viewport: { width: 1280, height: 900 } });
  const errors = [];
  page.on("pageerror", (e) => errors.push(String(e)));
  page.on("console", (m) => {
    if (m.type() === "error") errors.push("console: " + m.text());
  });
  // THIS SUITE STATES ITS LIGHTING (CEO §63, 2026-09-19). The shipped default preference
  // used to be `dark`, so every walk in here rendered dark without anyone saying so, and the
  // committed reference PNGs were taken that way. §63 makes the default `system`, which
  // hands the palette to the host's `colorScheme` — light, in this harness — and flipped
  // every one of them in a single run with every check still green, because nothing this
  // suite asserts is about lighting. A reference shot is a picture a person LOOKS at, so the
  // palette is named here rather than inherited from a default that is free to move again.
  // `SEED_THEME` writes both the mirror and the store and says why; the store decides.
  await page.addInitScript(SEED_THEME, theme || "dark");
  if (LAG_MS > 0) await page.addInitScript(SLOW_BRIDGE, LAG_MS);
  await page.goto(APP);
  // The home screen is the landing surface now; this suite is about the app UI behind it.
  await leaveHome(page);
  await page.waitForSelector(".nav-thread", { state: "attached" });
  // A ROW EXISTING IS NOT THE APP BEING READY, and every check below that clicks one was
  // relying on the difference being small. `bootSettled` waits for `init()` to have decided
  // which screen the CEO is on; `lib/harness.js` carries the measurement of what happens when
  // nothing does. Without it, checks 1, 13 and 14 drive a thread click into the boot's tail
  // and land on `general` — Harbor Analytics, "Running", zero turns — under titles that name
  // Northwind Traders and Q4 hiring.
  await bootSettled(page);
  page.__errors = errors;
  return page;
}

/// Wait for the opening curtain to yield before taking EVIDENCE of the chrome.
///
/// `openApp` above deliberately does not wait — the checks it serves are about the DOM, and
/// the curtain is `pointer-events: none`, so it changes nothing they assert. A screenshot is
/// the one place it matters: the cue sits above the curtain (correctly — so does the settings
/// button, §15), and a shot taken through it is a picture of the opening screen with a pill on
/// it rather than a picture of the cue in the app it announces into.
///
/// THE 150 MS THAT USED TO BE HERE WAS GUARDING TWO DIFFERENT THINGS AND NAMING NEITHER: the
/// curtain's own 180 ms fade-out, and `init()` still running underneath it. Both have an end
/// state, and neither is a number.
///
/// AND "THE COMPOSER HAS FOCUS" IS THE WRONG ONE, which is worth writing down because it was
/// the first thing tried here and it looks exactly right. Something on the boot path focuses
/// `#input` early — traced at 293 ms with `#messages` still empty — so that wait is satisfied
/// long before `init()`'s own `inputEl.focus()` runs, and THAT one then steals focus from
/// whatever a check has focused since. At 300 ms of bridge latency it cost check 16 its
/// `focused` assertion over a product that had done nothing wrong.
///
/// `shellSettled` is the end of `init()` and not a stage of it: `#nav-corrections-count` is
/// filled by `refreshDesk()`, one statement after that focus, and `#retention-hint` by
/// `syncRetentionFromBackend()`, dead last. Both non-empty means nothing else is going to move.
async function settled(page) {
  await page.waitForFunction(() => !document.getElementById("splash"), null, { timeout: 30000 }).catch(() => {});
  await shellSettled(page);
  await awaitSettled(page);
  await flushFrames(page);
}

/// ONE SHUTTER FOR EVERY COMMITTED SHOT IN THIS SUITE, and it waits for the one piece of the
/// shell this suite never drove.
///
/// `main.js` polls `get_worker_status` every 3,000 ms and rebuilds the drill chip from the
/// answer. It used to do so with NO leading call — so for up to three seconds after the app
/// opened a conversation the chip zone was empty, and then a whole line of text arrived in it.
/// Every shot in this file is a full-page picture of that shell, taken seconds after the app
/// came up, which was precisely that window.
///
/// `openThread` now awaits `pollWorkerStatus()` before it returns, so the chip is on screen
/// with the conversation. This wait stays: the 3,000 ms interval still re-renders the chip for
/// as long as the page is open, and a shutter can still land mid-re-render. It simply costs
/// less now — the settle agrees with a chip that is already there instead of waiting for one.
///
/// MEASURED, 2026-09-20, on this tree, BEFORE the leading call: a probe that reached
/// `updates-cue-absent`'s state and
/// read `document.body.innerText` three times 1.6 s apart found the line `⋯ 1 working · 1
/// done · 1 I can't see` absent at the first two reads and present at the third. Nothing this
/// suite asserts is about the work chip, and a picture that has it in one run and not the
/// next cannot show a real change to the cue this file exists to prove.
///
/// The wait is the harness's own, on the PRODUCT'S re-render agreeing with itself rather than
/// on a duration — the same call `memory-strategy.js` puts in front of all nine of its
/// committed shots. It THROWS rather than photographing a surface that is still moving.
async function evidence(page, name) {
  const chip = await awaitWorkerChipSettled(page);
  if (!chip.settled) {
    throw new Error(
      name + ": the work chip was still changing after the budget (" + chip.renders +
        " re-renders, last text " + JSON.stringify(chip.text) + "). A shot of a surface that " +
        "is still moving is evidence of a moment, not of a state."
    );
  }
  return shot(page, name);
}

/// A COMMAND'S ROUND TRIP, WAITED OUT ON ITS OWN END STATE.
///
/// `updates.js`'s `call()` is `busy = true; paint(); await invoke(cmd); apply(out); finally {
/// busy = false; paint(); }`. Three observable things happen and the LAST of them is the one a
/// check reads: the command is on the mock's ledger, the row is no longer marked busy, and the
/// view the script queued has been applied. Waiting for all three is waiting for the product to
/// finish, which is a different statement from waiting 150 ms.
async function settledAfterCommand(page, cmd) {
  await page.waitForFunction(
    (name) => {
      const calls = window.__RICHOS_MOCK__.updateCalls();
      if (calls.indexOf(name) < 0) return false;
      const row = document.getElementById("set-updates");
      if (row && row.getAttribute("data-update-busy") === "true") return false;
      return true;
    },
    cmd,
    { timeout: 10000 }
  );
  await flushFrames(page);
}

/// THE MENU REBUILT UNDER A FORCED PALETTE, waited out on both of its ends.
async function forcedDark(page, on) {
  await page.evaluate((v) => window.RichSettings.forceDark(v), on);
  await page.waitForFunction(
    (want) => {
      const painted = document.documentElement.getAttribute("data-theme");
      if (want && painted !== "dark") return false;
      // Coming back off the clamp, the palette is whatever the CEO's own preference resolves
      // to — which is not this check's subject. What IS its subject is that the menu was
      // rebuilt and the row came back with it.
      return !!document.getElementById("set-updates");
    },
    on,
    { timeout: 10000 }
  );
  await flushFrames(page);
}

/// A NEGATIVE OBSERVATION WINDOW — NOT A WAIT FOR A SIGNAL, and it is spelled differently on
/// purpose so nobody later "fixes" it into one.
///
/// Every other sleep in this file was a guess standing in for an event. These two are the
/// opposite claim: that over a stretch of time NO event occurs. There is no end state to wait
/// for, because the assertion is about the absence of one — and a window that waits for a
/// signal would pass instantly and prove nothing. The number is the width of the window, and
/// it is the thing the assertion is about, so it is stated with its reason at the call site.
///
/// It is also the ONLY honest reason left in this file to spend wall-clock time, which is why
/// it is a named function: a `waitForTimeout` here reads identically to the twelve that were
/// wrong, and a reader cannot tell them apart without this name.
async function observe(page, ms, why) {
  void why;
  await page.waitForTimeout(ms);
}

/// Put the panel into a state and return when the panel is IN it.
///
/// The 80 ms here was waiting for nothing at all, and that is worth saying precisely rather
/// than shortening: `updateSet` calls the `rich://update` listeners synchronously, the listener
/// calls `apply` -> `paint` synchronously, and `repaintUpdates` -> `paint` after it. The whole
/// chain is done before `page.evaluate` resolves. What the sleep DID do was mask the one case
/// where that is not true — a state driven while a `call()` round-trip is in flight, where the
/// command's own `finally { busy = false; paint(); }` lands afterwards and repaints the row
/// from the PREVIOUS view.
///
/// So this waits on the product's own published answer instead. `window.RichUpdates.state()` is
/// `updates.js`'s read-only handle on the view it is rendering from, so agreement between it
/// and the requested state is the panel saying "this is what I am showing" — a fact no number
/// of milliseconds can assert.
async function setState(page, v, script) {
  await page.evaluate(
    ([vv, ss]) => window.__RICHOS_MOCK__.updateSet(vv, ss),
    [v, script || []]
  );
  await page.waitForFunction(
    (want) => {
      const s = window.RichUpdates && window.RichUpdates.state ? window.RichUpdates.state() : null;
      return !!s && s.state === want;
    },
    v.state,
    { timeout: 10000 }
  );
  await flushFrames(page);
}

/// `visible` is Playwright's answer to "does it have a box", which a panel one frame into its
/// own entry animation also has. The 120 ms was the entry animation, guessed; `awaitSettled`
/// is that animation's own `finished`.
async function openMenu(page) {
  await page.click("#set-btn");
  await page.waitForSelector("#set-menu", { state: "visible" });
  await awaitSettled(page);
  await flushFrames(page);
}

/// Technical view on, through the same read and repaint the app uses (`main.js` `refreshTechy`,
/// which repaints the settings menu and with it this row).
async function technicalOn(page) {
  await page.evaluate(async () => {
    await window.RichBridge.invoke("set_techy_default", { enabled: true });
    await refreshTechy(activeThreadId);
  });
  await page.waitForFunction(() => window.RichSettings.technicalView && window.RichSettings.technicalView());
}

/// What the row says right now: the two sentences, which buttons are visible, and whether
/// the settings button is marked.
async function readRow(page) {
  return page.evaluate(() => {
    const row = document.getElementById("set-updates");
    const vis = (id) => {
      const n = document.getElementById(id);
      return !!n && !n.hidden;
    };
    const txt = (id) => {
      const n = document.getElementById(id);
      return n ? n.textContent.trim() : null;
    };
    const btn = document.getElementById("set-btn");
    const prog = document.getElementById("update-progress");
    return {
      present: !!row,
      state: row ? row.getAttribute("data-update-state") : null,
      failureKind: row ? row.getAttribute("data-update-failure") : null,
      headline: txt("update-line"),
      sub: vis("update-sub") ? txt("update-sub") : null,
      check: vis("update-check") ? txt("update-check") : null,
      checkDisabled: (() => {
        const n = document.getElementById("update-check");
        return n ? n.disabled : null;
      })(),
      install: vis("update-install") ? txt("update-install") : null,
      back: vis("update-back") ? txt("update-back") : null,
      backDisabled: (() => {
        const n = document.getElementById("update-back");
        return n ? n.disabled : null;
      })(),
      relaunch: vis("update-relaunch") ? txt("update-relaunch") : null,
      why: vis("update-why") ? txt("update-why") : null,
      detail: vis("update-detail") ? txt("update-detail") : null,
      endpoint: vis("update-endpoint") ? txt("update-endpoint") : null,
      progress: prog && !prog.hidden,
      ariaValueNow: prog ? prog.getAttribute("aria-valuenow") : null,
      indeterminate: prog ? prog.getAttribute("data-indeterminate") : null,
      fillWidth: (() => {
        const n = document.getElementById("update-progress-fill");
        return n ? n.style.width : null;
      })(),
      mark: btn ? btn.getAttribute("data-update-mark") : null,
      busy: row ? row.getAttribute("data-update-busy") : null,
    };
  });
}

/// THE WAITING CUE, read the way each of the two audiences meets it.
///
/// `count` is asked FIRST for the reason `readCue` asks it first: the property under test is
/// EXISTENCE, and a helper that reported a hidden placeholder and a removed element the same
/// way would make the two indistinguishable.
///
/// `tag`, `role` and `tabIndex` are here because "not an actionable thing" is a claim about
/// the MARKUP and not about the styling. A `button` that happened to do nothing would still
/// be announced as a button and still be offered to a voice user, and asserting on its
/// appearance would never notice.
async function readWaiting(page) {
  return page.evaluate(() => {
    const n = document.getElementById("update-waiting");
    const count = document.querySelectorAll("#update-waiting, .update-waiting").length;
    const cueCount = document.querySelectorAll("#update-cue, .update-cue").length;
    if (!n) return { count, cueCount };
    const note = document.getElementById("update-waiting-note");
    const noteStyle = note ? getComputedStyle(note) : null;
    return {
      count,
      cueCount,
      tag: n.tagName.toLowerCase(),
      role: n.getAttribute("role"),
      tabIndex: n.tabIndex,
      // The whole sentence, which is what a screen reader announces on focus whether or not
      // the note is ever painted.
      label: n.getAttribute("aria-label"),
      open: n.getAttribute("data-open"),
      noteText: note ? note.textContent.trim() : null,
      noteShown: noteStyle ? noteStyle.display !== "none" : null,
      noteFontPx: noteStyle ? Math.round(parseFloat(noteStyle.fontSize)) : null,
      // `aria-hidden` on the note, so the identical sentence is not announced twice.
      noteAriaHidden: note ? note.getAttribute("aria-hidden") : null,
      focused: document.activeElement === n,
    };
  });
}

/// THE CUE, read the way a person meets it: is there an element at all, what does it say, and
/// is the thing at its own center actually it.
///
/// `count` is asked FIRST and everything else is conditional on it, because the property under
/// test is existence. A helper that returned `visible: false` for a hidden placeholder and for
/// a removed element alike would make the two indistinguishable, and telling them apart is the
/// entire point of the ruling.
async function readCue(page) {
  return page.evaluate(() => {
    const n = document.getElementById("update-cue");
    // THE GEOMETRY IS MEASURED ON BOTH BRANCHES, and that is not tidiness. It was measured
    // only on the absent branch first, so `wrapWidth` and `btnWidth` were both `undefined`
    // whenever a cue existed — and check 13's footprint assertion then compared undefined
    // with undefined and passed. The `always-present` mutation, which is precisely the defect
    // that check exists to catch, went red on check 12 alone and left 13 green. A negative
    // assertion that cannot fail is not a check.
    const wrap = document.getElementById("settings");
    const btn = document.getElementById("set-btn");
    const w = wrap ? wrap.getBoundingClientRect() : null;
    const b = btn ? btn.getBoundingClientRect() : null;
    const box = {
      // The wrapper's own footprint, so "absent" can be proven to mean "costs no layout"
      // rather than "present and painted like the background".
      wrapWidth: w ? Math.round(w.width) : null,
      btnWidth: b ? Math.round(b.width) : null,
      wrapLeft: w ? Math.round(w.left) : null,
      btnLeft: b ? Math.round(b.left) : null,
    };
    if (!n) {
      return Object.assign({ count: document.querySelectorAll("#update-cue, .update-cue").length }, box);
    }
    const r = n.getBoundingClientRect();
    const hit = document.elementFromPoint(r.left + r.width / 2, r.top + r.height / 2);
    const menu = document.getElementById("set-menu");
    return Object.assign({}, box, {
      count: document.querySelectorAll("#update-cue, .update-cue").length,
      cueState: n.getAttribute("data-update-cue"),
      text: n.textContent.trim(),
      hidden: n.hidden,
      disabled: n.disabled,
      // Is it actually the thing under the pointer at its own center — "in view", not merely
      // in the document.
      onTop: !!(hit && hit.closest && hit.closest("#update-cue")),
      inViewport: r.top >= 0 && r.left >= 0 && r.bottom <= window.innerHeight && r.right <= window.innerWidth,
      menuOpen: !!(menu && !menu.hidden),
      glyphs: n.querySelectorAll("svg.update-cue-glyph").length,
      ariaLabel: n.getAttribute("aria-label"),
      rowLine: (() => {
        const l = document.getElementById("update-line");
        return l ? l.textContent.trim() : null;
      })(),
    });
  });
}

/// The nine `state` strings the Rust actually produces, read out of `updates.rs` rather than
/// typed here. A state added there and not handled in `updates.js` reaches this file as a
/// failing check instead of as a row that renders a blank sentence.
function rustStates() {
  const m = UPDATES_RS.match(/\/\/\/ `unconfigured`[\s\S]*?pub state: &'static str,/);
  assert(m, "no `state` doc comment in updates.rs — the inventory has nowhere to come from");
  const names = [...m[0].matchAll(/`([a-zA-Z]+)`/g)].map((x) => x[1]);
  assert(names.length > 0, "the doc comment named no states");
  return names.sort();
}

async function main() {
  const { webkit } = loadPlaywright();
  const browser = await webkit.launch();
  const run = createRun("the update surface (RICH-TODOs row 12) — through the real renderer");

  // ---- 1. it exists, where the CEO can reach it from every screen -----------------------
  await run.check("1. the update row is in the UNIVERSAL settings menu, on every screen", async () => {
    const page = await openApp(browser);
    // Nothing before the button is pressed — this is chrome, not a banner.
    assertEqual(await page.isVisible("#set-updates"), false, "nothing until the menu opens");
    await openMenu(page);
    const row = await readRow(page);
    assert(row.present, "the row is in the menu");
    // ...and it is the menu that is on EVERY screen, not the rail's popover, which is what
    // `#set-btn` being `position: fixed` outside `#app` buys. Prove it from a second surface.
    await page.click("#set-btn"); // close
    // THE SECOND SURFACE HAS TO BE THE SECOND SURFACE. This claim is "the row is reachable from
    // inside a conversation", and the row is chrome — it is present on the shell too. So a
    // sleep that was not long enough for `openThread`'s six-await chain left this check
    // asserting the shell's row and reporting the conversation's. `openThread` returns on the
    // model being bound and the turns painted; `assertOnThread` states the fact in the result.
    const arrived = await openThread(page, "hiring");
    assertEqual(arrived, "conversation", "the hiring thread opens as a conversation, not §21's refusal");
    const on = await assertOnThread(page, "hiring", "check 1's second surface");
    await openMenu(page);
    assert((await readRow(page)).present, "and again from inside a conversation");
    await page.close();
    return "present from the shell and from the open thread " + JSON.stringify(on.crumb) + " (" + on.turns + " turn(s))";
  });

  // ---- 2. every state the Rust can produce has a sentence here --------------------------
  await run.check("2. every state `updates.rs` can produce renders a sentence, and none is blank", async () => {
    const page = await openApp(browser);
    const states = rustStates();
    assert(states.length >= 9, "expected the nine states, got " + states.length + ": " + states);
    await openMenu(page);
    const seen = [];
    for (const s of states) {
      await setState(page, view({ state: s, availableVersion: "0.1.1", checkedAt: Date.now() - 60000 }));
      const row = await readRow(page);
      assert(row.headline && row.headline.length > 0, s + ": the headline is empty");
      assert(row.sub && row.sub.length > 0, s + ": the second line is empty");
      assertEqual(row.state, s, s + ": the row did not take the state");
      seen.push(s + " -> " + row.headline);
    }
    // An UNKNOWN state must report itself as unknown. Falling back to "up to date" is the
    // single worst answer this surface could give, and it is the natural default arm.
    await setState(page, view({ state: "something-nobody-shipped" }));
    const odd = await readRow(page);
    assert(
      /cannot tell/i.test(odd.sub),
      "an unrecognized state must say it is unrecognized, not imply the app is current: " + odd.sub
    );
    await page.close();
    return states.length + " states, each with a sentence; unknown reports unknown";
  });

  // ---- 3. "never checked" is not "up to date" ------------------------------------------
  await run.check("3. `idle` and `upToDate` are different sentences — never checked is a state", async () => {
    const page = await openApp(browser);
    await openMenu(page);
    await setState(page, view({ state: "idle", endpointIsPlaceholder: false, endpoint: "https://u.example.com/x" }));
    const idle = await readRow(page);
    await setState(page, view({ state: "upToDate", checkedAt: Date.now() - 8 * 60000, endpointIsPlaceholder: false, endpoint: "https://u.example.com/x" }));
    const fresh = await readRow(page);
    assert(/has not checked/i.test(idle.sub), "idle must say it has not checked: " + idle.sub);
    assert(/up to date/i.test(fresh.headline), "upToDate must say so: " + fresh.headline);
    assert(/8 minutes ago/.test(fresh.sub), "and WHEN it looked: " + fresh.sub);
    assert(idle.sub !== fresh.sub, "the two are not the same sentence");
    await page.close();
    return "idle: “" + idle.sub + "” / upToDate: “" + fresh.sub + "”";
  });

  // ---- 4. the placeholder endpoint is a THIRD thing, not an error -----------------------
  await run.check("4. `unconfigured` reports a decision that has not been made, not a failure", async () => {
    const page = await openApp(browser);
    await openMenu(page);
    await setState(page, view({ state: "unconfigured" }));
    const row = await readRow(page);
    assertEqual(row.state, "unconfigured", "the row is in the unconfigured state");
    assert(/no update server/i.test(row.sub), "it says there is no server yet: " + row.sub);
    assert(!/failed|error/i.test(row.sub), "and does not call a pending decision a failure");
    assertEqual(row.failureKind, null, "nothing is filed as a failure");
    assertEqual(row.checkDisabled, true, "and Check is not offered against an endpoint that cannot resolve");
    // The placeholder is never printed at the CEO; a non-placeholder endpoint always is.
    assertEqual(row.endpoint, null, "the .invalid placeholder is not paraded on screen");
    // HUNT 57, FINDING 19 (2026-10-15): nightly 57 printed "…/nightly/latest.json" under
    // "Request a feature" for a business owner. The update server is a technical line: with
    // Technical view off it is not on screen, whatever it points at.
    await setState(page, view({ state: "upToDate", endpoint: "https://github.com/WebDevBooster/richos/releases/download/nightly/latest.json", endpointIsPlaceholder: false, checkedAt: Date.now() }));
    const plainRow = await readRow(page);
    assertEqual(plainRow.endpoint, null, "the update server's file name is on screen with Technical view off");
    assert(!(await page.evaluate(() => document.getElementById("set-menu").innerText)).includes("latest.json"), "a file name shows in Settings");
    await technicalOn(page);
    await setState(page, view({ state: "upToDate", endpoint: "https://staging.example.com/u", endpointIsPlaceholder: false, checkedAt: Date.now() }));
    const staged = await readRow(page);
    assert(
      staged.endpoint && staged.endpoint.indexOf("staging.example.com") >= 0,
      "with Technical view on, a build pointed somewhere unusual says so on screen: " + staged.endpoint
    );
    await page.close();
    return "unconfigured is not an error; the update server is shown only in Technical view, and there it is disclosed";
  });

  // ---- 5. THE ONE THAT MATTERS: a refused signature is not retryable --------------------
  await run.check("5. a REFUSED SIGNATURE is not offered a retry, and every other failure is", async () => {
    const page = await openApp(browser);
    await openMenu(page);

    await setState(
      page,
      view({
        state: "failed",
        endpointIsPlaceholder: false,
        endpoint: "https://u.example.com/x",
        failure: {
          kind: "signature",
          headline: "This download was not signed by RichOS, so it was not installed.",
          detail: "Signature verification failed",
        },
      })
    );
    const sig = await readRow(page);
    assertEqual(sig.failureKind, "signature", "the kind reaches the DOM, so this is styleable and testable");
    assert(sig.check !== "Try again", "a refused signature must NOT offer a retry — it got: " + sig.check);
    assert(/not signed by RichOS/.test(sig.headline), "and says what happened: " + sig.headline);
    assert(sig.install === null && sig.relaunch === null, "nothing offers to install it anyway");
    await evidence(page, SHOTS + "/updates-signature-refused");

    for (const kind of ["offline", "network", "manifest", "install", "configuration", "other"]) {
      await setState(
        page,
        view({
          state: "failed",
          endpointIsPlaceholder: false,
          endpoint: "https://u.example.com/x",
          failure: { kind, headline: "The update did not complete.", detail: kind + " detail" },
        })
      );
      const r = await readRow(page);
      assertEqual(r.check, "Try again", kind + " is plausibly transient and must offer a retry");
    }
    await page.close();
    return "signature: no retry; offline/network/manifest/install/configuration/other: retry";
  });

  // ---- 6. the vendor's own reason is kept, one click away -------------------------------
  await run.check("6. the technical reason is the vendor's text, verbatim, behind a disclosure", async () => {
    const page = await openApp(browser);
    await openMenu(page);
    const detail = "Signature verification failed: comment signature verification failed";
    await setState(
      page,
      view({
        state: "failed",
        endpointIsPlaceholder: false,
        endpoint: "https://u.example.com/x",
        failure: { kind: "signature", headline: "This download was not signed by RichOS, so it was not installed.", detail },
      })
    );
    let row = await readRow(page);
    assertEqual(row.detail, null, "closed by default — the CEO reads a sentence, not a stack");
    assert(row.why === "Show the technical reason", "and is offered it: " + row.why);
    await page.click("#update-why");
    // The disclosure's own state, which is the thing the next three lines read.
    await page.waitForFunction(() => {
      const w = document.getElementById("update-why");
      const d = document.getElementById("update-detail");
      return !!w && w.getAttribute("aria-expanded") === "true" && !!d && !d.hidden;
    });
    row = await readRow(page);
    assertEqual(row.detail, detail, "verbatim, not summarised — a truncated error cannot be searched for");
    assertEqual(
      await page.getAttribute("#update-why", "aria-expanded"),
      "true",
      "and the disclosure says it is open"
    );
    await page.close();
    return "hidden by default, verbatim when opened";
  });

  // ---- 7. no percentage without a denominator ------------------------------------------
  await run.check("7. a download with no Content-Length shows NO percentage and no aria-valuenow", async () => {
    const page = await openApp(browser);
    await openMenu(page);

    await setState(page, view({ state: "downloading", availableVersion: "0.1.1", downloadedBytes: 5242880, totalBytes: 13631488, percent: 38, endpointIsPlaceholder: false, endpoint: "https://u.example.com/x" }));
    const known = await readRow(page);
    assert(known.progress, "the bar is shown");
    assertEqual(known.ariaValueNow, "38", "a known total gets a real accessible value");
    assertEqual(known.fillWidth, "38%", "and a fill that means it");
    assertEqual(known.indeterminate, null, "not indeterminate");
    assert(/5\.0 MB of 13\.0 MB/.test(known.sub), "and says the bytes: " + known.sub);

    await setState(page, view({ state: "downloading", availableVersion: "0.1.1", downloadedBytes: 5242880, totalBytes: null, percent: null, endpointIsPlaceholder: false, endpoint: "https://u.example.com/x" }));
    const unknown = await readRow(page);
    assert(unknown.progress, "the bar is still shown");
    assertEqual(unknown.ariaValueNow, null, "NO accessible value is invented");
    assertEqual(unknown.indeterminate, "true", "and it is marked indeterminate");
    assert(/5\.0 MB so far/.test(unknown.sub), "bytes, with no total claimed: " + unknown.sub);
    await page.close();
    return "38% with a total; no value and no percentage without one";
  });

  // ---- 8. the mark on the settings button, and when it is NOT there ---------------------
  await run.check("8. the settings button is marked for exactly two states and no others", async () => {
    const page = await openApp(browser);
    // The mark must appear WITHOUT the menu ever being opened — that is the whole point of
    // it, and it arrives on the `rich://update` event the automatic launch check emits.
    await setState(page, view({ state: "available", availableVersion: "0.1.1", endpointIsPlaceholder: false, endpoint: "https://u.example.com/x", checkedAt: Date.now() }));
    assertEqual(await page.getAttribute("#set-btn", "data-update-mark"), "available", "marked while the menu is shut");
    assertEqual(await page.locator("#update-mark").count(), 1, "one mark, not a stack of them");
    await evidence(page, SHOTS + "/updates-mark-on-the-button");

    await setState(page, view({ state: "ready", availableVersion: "0.1.1", endpointIsPlaceholder: false, endpoint: "https://u.example.com/x" }));
    assertEqual(await page.getAttribute("#set-btn", "data-update-mark"), "ready", "and changes meaning for ready");
    assertEqual(await page.locator("#update-mark").count(), 1, "still exactly one");

    for (const s of ["idle", "checking", "upToDate", "downloading", "installing", "failed", "unconfigured"]) {
      await setState(page, view({ state: s, availableVersion: "0.1.1", failure: s === "failed" ? { kind: "network", headline: "x", detail: "y" } : null }));
      assertEqual(
        await page.getAttribute("#set-btn", "data-update-mark"),
        null,
        s + " must not mark the button — a permanently marked button is one nobody reads"
      );
    }
    await page.close();
    return "available and ready mark it; the other seven do not";
  });

  // ---- 9. the buttons issue the commands they claim to -----------------------------------
  await run.check("9. Download stages the update without a restart action", async () => {
    const page = await openApp(browser);
    await openMenu(page);
    await page.evaluate(() => window.__RICHOS_MOCK__.updateCalls()); // drain nothing; just prove it exists

    await setState(
      page,
      view({ state: "available", availableVersion: "0.1.1", endpointIsPlaceholder: false, endpoint: "https://u.example.com/x", checkedAt: Date.now() }),
      [view({ state: "ready", availableVersion: "0.1.1", percent: 100, endpointIsPlaceholder: false, endpoint: "https://u.example.com/x" })]
    );
    let row = await readRow(page);
    assertEqual(row.install, "Download update", "the install button is offered");
    assertEqual(row.check, null, "and Check is not — the next act is not another check");

    await page.click("#update-install");
    // `call()` is `busy = true; paint(); await invoke(); apply(); finally busy = false; paint()`
    // — so the end state is the row NOT busy on the view the script queued, and that is what
    // this waits for. A sleep here was a bet on how long a bridge round-trip takes.
    await settledAfterCommand(page, "update_install");
    row = await readRow(page);
    assertEqual(row.state, "ready", "the scripted install landed");
    assertEqual(row.relaunch, null, "no update action can terminate the running session");
    assert(row.sub.includes("automatically next time RichOS opens"), "activation is deferred and explained: " + row.sub);
    assertEqual(row.install, null, "the install button is gone");

    const calls = await page.evaluate(() => window.__RICHOS_MOCK__.updateCalls());
    assert(calls.indexOf("update_install") >= 0, "Download issued update_install: " + calls.join(","));
    assertEqual(calls.indexOf("update_relaunch"), -1, "no restart command was issued");
    await page.close();
    return calls.join(" -> ");
  });

  // ---- 9b/9c. the way back off a bad release --------------------------------------------
  //
  // The CEO runs the nightly as his daily driver, and before this the only documented way
  // off a bad one was `NIGHTLY.md`'s "install a stable version at least as new as the
  // nightly" — by hand, from a browser, on the machine he was trying to use.
  //
  // The browser half of the proof is the same half this suite always proves: the control
  // says where it goes, it is offered only where pressing it is true, and it issues the
  // command rather than merely looking pressed. That an earlier tag is fetched, its
  // signature verified and its bundle published is proven where it happens —
  // `app/scripts/updater-e2e.sh` case R, on a fixture release directory with two tags.
  await run.check("20. Go back names its destination, and only exists when there is one", async () => {
    const page = await openApp(browser);
    await openMenu(page);

    await setState(page, view({ state: "upToDate", checkedAt: Date.now(), rollbackVersion: null }));
    let row = await readRow(page);
    assertEqual(row.back, null, "no recorded earlier version means no control at all");

    await setState(page, view({ state: "upToDate", checkedAt: Date.now(), rollbackVersion: "0.1.0" }));
    row = await readRow(page);
    // THE VERSION IS IN THE LABEL. "Go back" on its own is a control nobody presses,
    // and it has to mean the same thing read aloud as read on screen.
    assertEqual(row.back, "Go back to 0.1.0", "the control names where back is");
    assertEqual(row.backDisabled, false, "and is pressable when nothing is running");

    // ABSENT WHILE RICHOS IS WORKING, not dimmed — the same rule the Install control
    // follows, for the same reason (CEO, 2026-09-05).
    await setState(page, view({ state: "upToDate", checkedAt: Date.now(), rollbackVersion: "0.1.0", busy: true, busyReason: "Rich is working on your last message." }));
    assertEqual((await readRow(page)).back, null, "removed while RichOS is working");

    // ABSENT ONCE SOMETHING IS ALREADY PREPARED. That is what the next launch will do, and
    // `richos-user-update` refuses to stage a rollback over a prepared update — so offering
    // the press here would be offering a refusal.
    await setState(page, view({ state: "ready", availableVersion: "0.1.2", percent: 100, rollbackVersion: "0.1.0" }));
    assertEqual((await readRow(page)).back, null, "removed once something is prepared");

    await page.close();
    return "Go back to 0.1.0 / absent when none, busy, or already prepared";
  });

  await run.check("21. Go back issues update_rollback, and the row promises the RIGHT thing", async () => {
    const page = await openApp(browser);
    await openMenu(page);
    await setState(
      page,
      view({ state: "upToDate", currentVersion: "0.1.2", checkedAt: Date.now(), rollbackVersion: "0.1.0", endpointIsPlaceholder: false, endpoint: "https://u.example.com/x" }),
      [view({ state: "ready", currentVersion: "0.1.2", availableVersion: "0.1.0", readyIsRollback: true, percent: 100, rollbackVersion: "0.1.0", endpointIsPlaceholder: false, endpoint: "https://u.example.com/x" })]
    );
    await page.click("#update-back");
    await settledAfterCommand(page, "update_rollback");

    const row = await readRow(page);
    assertEqual(row.state, "ready", "the scripted rollback landed");
    // THE PROMISE IS DIFFERENT FROM AN UPDATE'S, and it comes off `readyIsRollback` rather
    // than off comparing two version strings — the shell is the one place that knows which
    // direction the staged thing moves.
    assert(row.headline.indexOf("go back to 0.1.0") >= 0, "the row says where it is going: " + row.headline);
    assert(row.sub.indexOf("go back automatically next time RichOS opens") >= 0, "and when: " + row.sub);
    assertEqual(row.relaunch, null, "no update action can terminate the running session");

    const calls = await page.evaluate(() => window.__RICHOS_MOCK__.updateCalls());
    assert(calls.indexOf("update_rollback") >= 0, "Go back issued update_rollback: " + calls.join(","));
    assertEqual(calls.indexOf("update_install"), -1, "and not the update command");
    await page.close();
    return calls.join(" -> ") + " | “" + row.headline + "”";
  });

  // ---- 22. the relaunch (CEO feedback 2026-10-06, item 1) -------------------------------
  //
  // His words: "there was no work going on at the time. So, there was nothing that could be
  // interrupted. In this case, the app should automatically re-launch immediately after
  // downloading the new version. Avoiding/preventing a re-launch is only meant for when there
  // are workers running." `updates.rs` decides and relaunches (its own unit test holds the
  // decision); this proves the page half: what he is told before it happens, that the pill
  // says it with the menu shut, that his draft is parked at once, and that while work runs
  // the row promises the relaunch for when the work is done instead of the next launch.
  await run.check("22. a downloaded update restarts when nothing runs, after one short notice", async () => {
    const live = { endpointIsPlaceholder: false, endpoint: "https://u.example.com/x" };
    const page = await openApp(browser);
    await openThread(page, "hiring");
    await settled(page);

    // --- IDLE: download, then the notice -------------------------------------------------
    await setState(
      page,
      view(Object.assign({ state: "available", availableVersion: "0.1.2", checkedAt: Date.now() }, live)),
      [view(Object.assign({ state: "ready", availableVersion: "0.1.2", percent: 100, restartsWhenIdle: true }, live))]
    );
    await openMenu(page);
    await page.click("#update-install");
    await settledAfterCommand(page, "update_install");
    let row = await readRow(page);
    assertEqual(row.headline, "RichOS 0.1.2 is ready.", "downloaded");
    assert(row.sub.indexOf("will restart into it in a moment") >= 0, "and it says the restart is next: " + row.sub);
    assertEqual(row.sub.indexOf("next time RichOS opens"), -1, "not the next launch: " + row.sub);

    // His unsent sentence is parked the instant the notice arrives, not on the composer's
    // 400 ms debounce: typed and noticed in one turn of the page, then read straight back.
    const parked = await page.evaluate(() => {
      const input = document.getElementById("input");
      input.value = "half a sentence he has not sent";
      input.dispatchEvent(new Event("input", { bubbles: true }));
      window.localStorage.removeItem("richos.view.drafts");
      window.__RICHOS_MOCK__.updateSet(
        Object.assign({}, window.RichUpdates.state(), { state: "restarting" }),
        []
      );
      return window.localStorage.getItem("richos.view.drafts") || "";
    });
    assert(parked.indexOf("half a sentence he has not sent") >= 0, "the draft was parked before the relaunch: " + parked);
    await page.waitForFunction(() => window.RichUpdates.state().state === "restarting");
    await flushFrames(page);
    row = await readRow(page);
    assertEqual(row.state, "restarting", "the row took the notice");
    assertEqual(row.headline, "Restarting into RichOS 0.1.2…", "the notice names the version");
    assert(row.sub.indexOf("Your window and conversation will be right where you left them.") >= 0, "and says he loses nothing: " + row.sub);
    assertEqual(row.check, null, "no control: nothing to press while it restarts");
    assertEqual(row.install, null, "no control: nothing to press while it restarts");
    assertEqual(row.back, null, "no control: nothing to press while it restarts");
    assertEqual(row.mark, "ready", "the settings button keeps its mark");

    // With the menu SHUT, the pill carries the same sentence.
    await page.keyboard.press("Escape");
    await page.waitForFunction(() => {
      const m = document.getElementById("set-menu");
      return !m || m.hidden;
    });
    const cue = await readCue(page);
    assertEqual(cue.count, 1, "the notice is in the chrome with the menu shut");
    assertEqual(cue.cueState, "restarting", "as the restart notice");
    assertEqual(cue.text, "Restarting into RichOS 0.1.2…", "the pill and the row say the same thing");
    assertEqual(cue.onTop, true, "and nothing covers it");
    await settled(page);
    await evidence(page, SHOTS + "/updates-cue-restarting");
    await page.close();

    // --- WORKING: the relaunch waits for the work, and says so --------------------------
    const p2 = await openApp(browser);
    await settled(p2);
    await setState(
      p2,
      view(Object.assign({
        state: "ready", availableVersion: "0.1.2", percent: 100, restartsWhenIdle: true,
        busy: true, busyReason: "2 workers are still running.",
      }, live))
    );
    const waiting = await readWaiting(p2);
    assertEqual(waiting.count, 1, "working: the waiting cue, not the pill");
    assert(waiting.label.indexOf("as soon as that work is done") >= 0, "it promises the restart after the work: " + waiting.label);
    assertEqual(waiting.label.indexOf("next time RichOS opens"), -1, "and not the next launch: " + waiting.label);
    assertEqual(waiting.label.indexOf("—"), -1, "no dashes in what is new: " + waiting.label);
    await openMenu(p2);
    row = await readRow(p2);
    assert(row.sub.indexOf("2 workers are still running.") >= 0, "it names what is running: " + row.sub);
    assert(row.sub.indexOf("RichOS will restart into this update as soon as that work is done. Nothing will be interrupted.") >= 0, "and when it restarts: " + row.sub);
    assertEqual(row.sub.indexOf("next time RichOS opens"), -1, "not the next launch: " + row.sub);

    // --- NOT DOWNLOADED HERE: the next-launch promise stands ----------------------------
    await setState(p2, view(Object.assign({ state: "ready", availableVersion: "0.1.2", percent: 100 }, live)));
    row = await readRow(p2);
    assertEqual(row.headline, "RichOS 0.1.2 is ready for the next launch.", "a ready this app did not download");
    assert(row.sub.indexOf("next time RichOS opens") >= 0, "keeps the next-launch sentence: " + row.sub);
    await p2.close();
    return "idle: ready, then a named notice in the row and the pill, draft parked at once; working: restart after the work";
  });

  // ---- 10. the row survives a menu rebuild ----------------------------------------------
  await run.check("10. a menu rebuild does not leave an empty Updates row", async () => {
    const page = await openApp(browser);
    await setState(page, view({ state: "available", availableVersion: "0.1.1", endpointIsPlaceholder: false, endpoint: "https://u.example.com/x", checkedAt: Date.now() }));
    await openMenu(page);
    assert((await readRow(page)).headline.indexOf("0.1.1") >= 0, "painted before the rebuild");
    // `RichSettings.forceDark` throws the whole menu away and builds a new one — the same
    // path §15's opening screen takes. The row is a NEW element afterwards, so a renderer
    // that cached its nodes would paint into a detached tree and leave a blank row.
    // `forceDark` -> `T.forceDark` -> `paint()` sets `data-theme` on the root and notifies, and
    // `rebuild()` builds a NEW menu. Both ends are observable: the palette the root is painting
    // and a row that is present again. A sleep proved neither, and a rebuild that had left the
    // row out would have been read as "not repainted yet" for exactly as long as the number.
    await forcedDark(page, true);
    await forcedDark(page, false);
    const after = await readRow(page);
    assert(after.present, "the row survived the rebuild");
    assert(after.headline.indexOf("0.1.1") >= 0, "and is still painted: " + after.headline);
    await page.close();
    return "rebuilt and repainted: “" + after.headline + "”";
  });

  // ---- 11. no page errors ----------------------------------------------------------------
  await run.check("11. no page errors anywhere in this suite", async () => {
    const page = await openApp(browser);
    await openMenu(page);
    for (const s of ["unconfigured", "idle", "checking", "upToDate", "available", "downloading", "installing", "ready", "failed"]) {
      await setState(page, view({ state: s, availableVersion: "0.1.1", failure: s === "failed" ? { kind: "signature", headline: "h", detail: "d" } : null }));
    }
    const errs = page.__errors;
    assertEqual(errs, [], "uncaught errors: " + errs.join(" | "));
    await page.close();
    return "0 uncaught errors across all nine states";
  });

  // =======================================================================================
  // THE CUE — CEO ruling §26, the placement paragraph added 2026-09-04.
  //
  // *"The user can't be bothered to hunt for some update button somewhere."* Everything above
  // proves the ROW, and the row is inside a menu. Checks 1 to 11 would all have passed on the
  // day the ruling was written, which is exactly the point: a surface can be complete,
  // correct, tested, and still announce nothing to anybody.
  //
  // THE ABSENCE HALF IS THE ONE THAT MATTERS. An affordance that is always present passes a
  // naive "is it there when there is an update" test perfectly and fails the ruling, because
  // the property being asked for is not visibility — it is that a NEW element arrives in
  // chrome the user is already looking at. A control that is always there and occasionally
  // changes color is one an eye has already learned to skip. So the absence is asserted three
  // ways that a hidden placeholder cannot satisfy: element count zero, no layout footprint,
  // and the state driven BACK to absent after the cue has once existed.
  // =======================================================================================

  // ---- 12. it appears when there is something, and does not EXIST when there is not -----
  await run.check("12. the cue APPEARS for a waiting update and does not EXIST otherwise", async () => {
    const page = await openApp(browser);

    // Nothing has been opened, and nothing will be: this is the whole claim.
    await setState(page, view({ state: "upToDate", endpointIsPlaceholder: false, endpoint: "https://u.example.com/x", checkedAt: Date.now() }));
    let cue = await readCue(page);
    assertEqual(cue.count, 0, "with nothing to install there is NO element — not a dimmed one, not an 'up to date' one");

    await setState(page, view({ state: "available", availableVersion: "0.1.2", endpointIsPlaceholder: false, endpoint: "https://u.example.com/x", checkedAt: Date.now() }));
    cue = await readCue(page);
    assertEqual(cue.count, 1, "one element arrived, and exactly one");
    assertEqual(cue.menuOpen, false, "and NOTHING was opened to see it — that is the defect this closes");
    assertEqual(cue.hidden, false, "present means present");
    assertEqual(cue.disabled, false, "and live, not a disabled state");
    assertEqual(cue.onTop, true, "it is the thing under the pointer at its own middle, not covered");
    assertEqual(cue.inViewport, true, "and inside the window, with no scrolling to reach it");
    assertEqual(cue.cueState, "available", "carrying its state for a stylesheet and for this suite");
    assertEqual(cue.glyphs, 1, "one glyph");
    assert(/RichOS 0\.1\.2 is available/.test(cue.text), "and it NAMES THE VERSION: " + cue.text);
    // WCAG 2.5.3 "Label in Name": the accessible name STARTS with the visible words, so
    // someone driving this by voice can say what is on the screen and be understood.
    assert(
      cue.ariaLabel && cue.ariaLabel.indexOf(cue.text) === 0 && cue.ariaLabel.length > cue.text.length,
      "the accessible name carries the visible label verbatim and then says what pressing it does: " + cue.ariaLabel
    );
    await settled(page);
    await evidence(page, SHOTS + "/updates-cue-available");

    await setState(page, view({ state: "ready", availableVersion: "0.1.2", endpointIsPlaceholder: false, endpoint: "https://u.example.com/x" }));
    cue = await readCue(page);
    assertEqual(cue.count, 1, "still one for the other waiting state");
    assertEqual(cue.cueState, "ready", "and it changed meaning");
    assert(/RichOS 0\.1\.2 is ready\./.test(cue.text), "naming the version again: " + cue.text);
    await settled(page);
    await evidence(page, SHOTS + "/updates-cue-ready");

    // AND BACK. A build that only ever ADDS the element passes the half above and is the
    // exact failure the ruling names, so the return trip is asserted rather than assumed.
    for (const s of ["idle", "checking", "upToDate", "downloading", "installing", "failed", "unconfigured", "a-state-nobody-shipped"]) {
      await setState(
        page,
        view({
          state: s,
          availableVersion: "0.1.2",
          failure: s === "failed" ? { kind: "signature", headline: "This download was not signed by RichOS, so it was not installed.", detail: "d" } : null,
        })
      );
      const gone = await readCue(page);
      assertEqual(gone.count, 0, s + " must leave NO element behind — a placeholder is the defect, not the fix");
    }
    await settled(page);
    await evidence(page, SHOTS + "/updates-cue-absent");
    await page.close();
    return "available and ready raise exactly one element with the menu shut; the other eight states raise none";
  });

  // ---- 13. absent costs NOTHING, and present is in chrome that is on every screen --------
  await run.check("13. absence reserves no space, and the cue reaches every surface", async () => {
    const page = await openApp(browser);
    await setState(page, view({ state: "upToDate", endpointIsPlaceholder: false, endpoint: "https://u.example.com/x", checkedAt: Date.now() }));
    const empty = await readCue(page);
    // THE CHECK A HIDDEN PLACEHOLDER CANNOT PASS. `visibility: hidden`, `opacity: 0` and a
    // zero-content pill all leave the wrapper wider than its button; a removed element does
    // not. Geometry, not intent.
    assertEqual(empty.wrapWidth, empty.btnWidth, "with no update the settings wrapper is exactly its button, " + empty.wrapWidth + " vs " + empty.btnWidth);
    assertEqual(empty.wrapLeft, empty.btnLeft, "and starts where the button starts — nothing is held open to its left");

    // ...and when it does arrive it grows LEFTWARD: the button the CEO already aims at must
    // not move under his pointer.
    const btnRightBefore = await page.evaluate(() => Math.round(document.getElementById("set-btn").getBoundingClientRect().right));
    await setState(page, view({ state: "available", availableVersion: "0.1.2", endpointIsPlaceholder: false, endpoint: "https://u.example.com/x", checkedAt: Date.now() }));
    const btnRightAfter = await page.evaluate(() => Math.round(document.getElementById("set-btn").getBoundingClientRect().right));
    assertEqual(btnRightAfter, btnRightBefore, "the settings button did not move when the cue arrived");

    // It is mounted in the chrome §15 puts on EVERY screen, so prove it from a second surface
    // rather than from the one it happened to be built on.
    // SAME REASON AS CHECK 1, and here it is sharper: `onTop` is asserted to be true "over the
    // conversation", which is a claim about what is UNDERNEATH the cue. Sampled before the
    // stage had been rebuilt, it was a claim about the shell.
    await openThread(page, "hiring");
    const onThread = await assertOnThread(page, "hiring", "check 13's second surface");
    const inThread = await readCue(page);
    assertEqual(inThread.count, 1, "still there inside a conversation");
    assertEqual(inThread.onTop, true, "and still the thing under the pointer, over the conversation");
    await page.close();
    return "absent: wrapper " + empty.wrapWidth + "px = button " + empty.btnWidth + "px; present: the button's right edge never moves, and the cue is over the open conversation " + JSON.stringify(onThread.crumb);
  });

  // ---- 14. it LEADS to the existing flow, and decides nothing itself ---------------------
  await run.check("14. the cue opens the existing row and issues no command of its own", async () => {
    const page = await openApp(browser);
    // THE CURTAIN IS LET GO OF BEFORE ANYTHING IS PRESSED, and this is a focus test so that
    // ordering is load-bearing rather than cosmetic. `main.js` puts focus in the composer the
    // moment the opening screen yields; a check that clicked first and waited afterwards
    // would watch its own assertion be overwritten by the app booting.
    await settled(page);
    await setState(
      page,
      view({ state: "available", availableVersion: "0.1.2", notes: "Faster launch.", endpointIsPlaceholder: false, endpoint: "https://u.example.com/x", checkedAt: Date.now() }),
      [view({ state: "ready", availableVersion: "0.1.2", percent: 100, endpointIsPlaceholder: false, endpoint: "https://u.example.com/x" })]
    );
    const before = await page.evaluate(() => window.__RICHOS_MOCK__.updateCalls().length);

    await page.click("#update-cue");
    await page.waitForSelector("#set-menu", { state: "visible" });
    await awaitSettled(page);
    await flushFrames(page);

    // THE ROW IS THE AUTHORITY, and the cue speaks with its voice rather than a second one.
    const cue = await readCue(page);
    const row = await readRow(page);
    assert(row.present, "the row the cue leads to is the row this suite has been testing all along");
    // Read in the SAME `evaluate` as the cue's own text, so this is one snapshot rather than
    // two reads a repaint could sit between.
    assertEqual(cue.text, cue.rowLine, "the cue's words ARE the row's words — one sentence, not two that agree today");
    assertEqual(row.headline, cue.text, "and the row agrees when read on its own terms");
    assertEqual(row.install, "Download update", "and the row's own control is the offer");
    await evidence(page, SHOTS + "/updates-cue-opened-the-row");

    // NOTHING WAS INSTALLED BY PRESSING THE ANNOUNCEMENT. §26's mode 1 — install with no
    // click at all — was ruled a separate design session on 2026-09-04, and an announcement
    // that quietly starts an install is that decision taken by an engineer.
    const afterOpen = await page.evaluate(() => window.__RICHOS_MOCK__.updateCalls());
    assertEqual(afterOpen.indexOf("update_install"), -1, "the cue installed nothing: " + afterOpen.slice(before).join(","));
    assertEqual(afterOpen.indexOf("update_relaunch"), -1, "and restarted nothing");

    // The hand is put on the control, so the click that announced the update lands one
    // keystroke from the act rather than one hunt from it.
    //
    // THIS ASSERTION IS TRUE HERE AND MEASURED FALSE ON ANY REAL BRIDGE — a finding, recorded
    // where the next person to run this will meet it rather than in a report nobody opens.
    //
    // `updates.js`'s `openTheRow()` opens the menu and then defers the focus with
    // `setTimeout(..., 0)`, on a stated reasoning that is worth quoting because it is the
    // whole defect: *"One turn of the task queue later the answer has landed and the button is
    // live."* It has not. `RichSettings.open()` calls `onOpen()`, which calls
    // `call("update_state")` — `busy = true; paint(); await invoke(...); finally { busy =
    // false; paint(); }` — so for the length of a bridge ROUND TRIP `#update-install` is
    // `disabled` (`canAct = !busy && ...`), and `focus()` on a disabled button does nothing at
    // all, silently.
    //
    // A `setTimeout(0)` beats that round trip only when the round trip is not a round trip.
    // Measured 2026-09-06 by sweeping `RICHOS_UPDATES_LAG_MS`, sampling `install.disabled` at
    // the exact instant the product's own timer fires:
    //
    //     lag   focus after the cue   install.disabled at t+0
    //     0     update-install        false
    //     1     (body)                false
    //     5     (body)                false
    //     10    (body)                true
    //     25    (body)                true
    //     60    (body)                true
    //     120   input                 true
    //     250   (body)                true
    //
    // The threshold is between 5 ms and 10 ms, and the in-page mock this suite drives is the
    // only bridge in existence that sits below it: a Tauri IPC call is never 5 ms. So §26's
    // "one keystroke from the act" works in this fixture and nowhere else, and the 150 ms
    // sleep that used to stand here is what let the check agree.
    //
    // IT IS LEFT ASSERTING THE RIGHT THING. Weakening it to "focus is somewhere reasonable"
    // would delete the only evidence that the affordance is broken, and the suite would then
    // be green over a product that does not do what its own comment says. Running with
    // `RICHOS_UPDATES_LAG_MS=10` or higher turns this line red, which is the right result; the
    // fix belongs in `app/ui/updates.js`'s `openTheRow`, which has to wait for the control to
    // become actionable rather than for one turn of the task queue.
    assertEqual(
      await page.evaluate(() => document.activeElement && document.activeElement.id),
      "update-install",
      "focus is on the row's own control"
    );
    await page.keyboard.press("Enter");
    await settledAfterCommand(page, "update_install");
    const calls = await page.evaluate(() => window.__RICHOS_MOCK__.updateCalls());
    assert(calls.indexOf("update_install") >= 0, "and pressing it reaches the SAME command the row has always issued: " + calls.join(","));
    assertEqual((await readRow(page)).state, "ready", "the scripted install landed, through the existing flow");
    await page.close();
    return "cue -> menu -> the row's own control -> update_install; the cue itself issues nothing";
  });

  // ---- 15. BOTH DIRECTIONS, and it has to be both ---------------------------------------
  //
  // A suite in which everything defers passes for the wrong reason. This check drives the
  // SAME update through the two states the ruling distinguishes and asserts the opposite
  // outcome in each: idle installs immediately with nothing added, busy offers nothing at
  // all. Either half alone would be green over a broken product.
  await run.check("15. idle installs at once; working offers no control at all", async () => {
    const page = await openApp(browser);
    await settled(page);

    // --- IDLE: the fix must not become a stall ---------------------------------------
    await setState(
      page,
      view({ state: "available", availableVersion: "0.1.2", endpointIsPlaceholder: false, endpoint: "https://u.example.com/x", checkedAt: Date.now() }),
      [view({ state: "ready", availableVersion: "0.1.2", percent: 100, endpointIsPlaceholder: false, endpoint: "https://u.example.com/x" })]
    );
    let cue = await readCue(page);
    assertEqual(cue.count, 1, "idle: the actionable pill is there");
    let waiting = await readWaiting(page);
    assertEqual(waiting.count, 0, "idle: and the waiting cue is not");
    await openMenu(page);
    let row = await readRow(page);
    assertEqual(row.install, "Download update", "idle: the control is offered");
    assertEqual(row.busy, null, "idle: nothing marks the row as blocked");
    assertEqual(row.mark, "available", "idle: and the settings button carries its mark");
    await page.click("#update-install");
    await settledAfterCommand(page, "update_install");
    let calls = await page.evaluate(() => window.__RICHOS_MOCK__.updateCalls());
    assert(calls.indexOf("update_install") >= 0, "idle: the press reached the command with no delay of its own");
    assertEqual((await readRow(page)).state, "ready", "idle: and the install landed");
    await page.close();

    // --- WORKING: the same update, and nothing to press ------------------------------
    const p2 = await openApp(browser);
    await settled(p2);
    await setState(
      p2,
      view({
        state: "available", availableVersion: "0.1.2", endpointIsPlaceholder: false,
        endpoint: "https://u.example.com/x", checkedAt: Date.now(),
        busy: true, busyReason: "Rich is working on your last message. 2 workers are still running.",
      })
    );
    cue = await readCue(p2);
    assertEqual(cue.count, 0, "working: the actionable pill is GONE — removed, not dimmed");
    await openMenu(p2);
    row = await readRow(p2);
    assertEqual(row.install, null, "working: no Install control exists");
    assertEqual(row.mark, null, "working: and the mark on the settings button is gone too");
    assertEqual(row.busy, "true", "the row still knows, it just does not offer");
    // HIDING THE ACTION IS NOT HIDING THE FACT. 26: an update nobody discovers is the same
    // as no updater at all.
    assertEqual(row.headline, "RichOS 0.1.2 is available.", "the FACT survives in full");
    assert(row.sub.indexOf("Rich is working on your last message.") >= 0, "and it says WHAT is running: " + row.sub);
    assert(row.sub.indexOf("2 workers are still running.") >= 0, "including the half the composer cannot show: " + row.sub);
    assert(
      row.sub.indexOf("nothing will be interrupted") >= 0,
      "and it answers the question that made him distrust the button: " + row.sub
    );
    // MODE-PROOF: it must not promise an install that this product does not perform.
    assertEqual(row.sub.indexOf("will install"), -1, "it never promises 26's mode 1, which does not exist: " + row.sub);
    const before = await p2.evaluate(() => window.__RICHOS_MOCK__.updateCalls().length);
    await observe(p2, 200, "an install that starts itself would start from a timer, and a timer needs time to fire");
    const after = await p2.evaluate(() => window.__RICHOS_MOCK__.updateCalls());
    assertEqual(after.indexOf("update_install"), -1, "nothing installed itself: " + after.slice(before).join(","));
    await evidence(p2, SHOTS + "/updates-working-no-control");
    await p2.close();
    return "idle -> pill + control + mark + install issued; working -> none of the four, and the row still names the update and why it waits";
  });

  // ---- 16. the fact, with nothing to press, reachable without a pointer ------------------
  //
  // The CEO asked for "some other visual cue but not an actionable thing". The two ways that
  // requirement fails silently are that it becomes a button anyway, and that its explanation
  // lives only in a hover — which reaches a mouse and reaches nobody else.
  await run.check("16. the waiting cue explains itself by keyboard, and is not a button", async () => {
    const page = await openApp(browser);
    await settled(page);
    await setState(
      page,
      view({
        state: "ready", availableVersion: "0.1.2", percent: 100, endpointIsPlaceholder: false,
        endpoint: "https://u.example.com/x", checkedAt: Date.now(),
        busy: true, busyReason: "1 worker is still running.",
      })
    );
    let w = await readWaiting(page);
    assertEqual(w.count, 1, "it is there while work runs");
    assertEqual(w.cueCount, 0, "and the actionable pill is not — never both");

    // NOT A BUTTON, IN THE MARKUP. A `button` that did nothing would still be announced as a
    // button and still offered to every voice user.
    assertEqual(w.tag, "span", "not a button element");
    assertEqual(w.role, "note", "and it says so to a screen reader");
    assertEqual(w.tabIndex, 0, "but it IS reachable by keyboard");

    // THE WHOLE SENTENCE IS THE ACCESSIBLE NAME, so it is announced on focus even where the
    // painted note is never seen at all.
    assert(w.label.indexOf("RichOS 0.1.2") >= 0, "the name says WHICH version: " + w.label);
    assert(w.label.indexOf("1 worker is still running.") >= 0, "and what is running: " + w.label);
    assert(w.label.indexOf("work will continue uninterrupted") >= 0, "and that nothing is at risk: " + w.label);
    assertEqual(w.noteShown, false, "the note is closed until it is asked for");

    // FOCUS OPENS IT — the requirement a hover-only tooltip fails.
    await page.focus("#update-waiting");
    // The note's own end state — painted, and its transition finished, so `noteShown` below is
    // read off a settled surface rather than partway into one.
    await page.waitForFunction(() => {
      const n = document.getElementById("update-waiting-note");
      return !!n && getComputedStyle(n).display !== "none";
    });
    await awaitSettled(page);
    await flushFrames(page);
    w = await readWaiting(page);
    assert(w.focused, "focus landed on it");
    assertEqual(w.noteShown, true, "focus alone opened the explanation — no pointer needed");
    assertEqual(w.noteText, w.label, "and the painted words ARE the announced words, one string");
    assertEqual(w.noteAriaHidden, "true", "so it is not announced twice");
    assertEqual(w.noteFontPx, 16, "16px — the floor for text meant to be easily read");
    await evidence(page, SHOTS + "/updates-waiting-cue-focused");

    // ESCAPE CLOSES IT, the same key that dismisses everything else transient here.
    await page.keyboard.press("Escape");
    await page.waitForFunction(() => {
      const n = document.getElementById("update-waiting-note");
      return !n || getComputedStyle(n).display === "none";
    });
    await awaitSettled(page);
    assertEqual((await readWaiting(page)).noteShown, false, "Escape closes it");

    // PRESSING IT DOES NOTHING, which is the whole point.
    const before = await page.evaluate(() => window.__RICHOS_MOCK__.updateCalls().length);
    await page.click("#update-waiting");
    await observe(page, 150, "a handler that DID issue a command would go through `call()`, which is a bridge round-trip");
    const after = await page.evaluate(() => window.__RICHOS_MOCK__.updateCalls());
    assertEqual(after.length, before, "clicking it issues no command: " + after.slice(before).join(","));
    await page.close();
    return "span/role=note/tabindex=0; focus opens the note; name == note text; Escape closes; the click does nothing";
  });

  // ---- 17. it never claims to have waited for something it did not check -----------------
  //
  // The gate can see this session's turn and this session's workers. Where it cannot see
  // something it says so, rather than letting the reassurance imply a check it never made.
  // And an update the gate has hidden for days says how long — 26 rules that an update nobody
  // discovers is worse than none, so a silent indefinite wait is the failure, not the fix.
  await run.check("17. what it could not check is printed, and a long wait says how long", async () => {
    const page = await openApp(browser);
    await settled(page);
    await setState(
      page,
      view({
        state: "available", availableVersion: "0.1.2", endpointIsPlaceholder: false,
        endpoint: "https://u.example.com/x", checkedAt: Date.now(),
        busy: true, busyReason: "Rich is working on your last message.",
        unchecked: ["RichOS is not connected to a session, so it cannot see any workers."],
        readySince: Date.now() - 3 * 86400000 - 3600000,
      })
    );
    await openMenu(page);
    let row = await readRow(page);
    assert(
      row.sub.indexOf("cannot see any workers") >= 0,
      "the gap is stated rather than papered over: " + row.sub
    );
    assert(row.sub.indexOf("It has been ready for 3 days.") >= 0, "and the wait is not silent: " + row.sub);
    await evidence(page, SHOTS + "/updates-unchecked-and-long-wait");
    await page.close();

    // UNDER A DAY, NOTHING. A duration line that always appears is a line nobody reads, and
    // this one has to still mean something the day it matters.
    const p2 = await openApp(browser);
    await settled(p2);
    await setState(
      p2,
      view({
        state: "available", availableVersion: "0.1.2", endpointIsPlaceholder: false,
        endpoint: "https://u.example.com/x", checkedAt: Date.now(),
        busy: true, busyReason: "Rich is working on your last message.",
        readySince: Date.now() - 3600000,
      })
    );
    await openMenu(p2);
    row = await readRow(p2);
    assertEqual(row.sub.indexOf("It has been ready"), -1, "an hour old says nothing about its age: " + row.sub);
    await p2.close();
    return "unchecked clause printed verbatim; 3 days -> said; 1 hour -> silent";
  });

  // ---- 18. THE VERSION ON SCREEN IS THE RUNNING BUILD'S, AND NOTHING ELSE ---------------
  //
  // CEO, 2026-09-17, item 4 of his v1.0.2 list, verbatim:
  //
  //     "The RichOS app is currently lying about the current version saying "RichOS 1.0.3 is
  //      up to date." even though the current version is 1.0.2. Make sure the app always
  //      displays the correct version."
  //
  // WHERE 1.0.3 CAME FROM, measured rather than guessed, because the fix depends on it. The
  // bundle he was running IS 1.0.3: `~/Applications/RichOS.app/Contents/Info.plist` reports
  // `CFBundleShortVersionString = 1.0.3` and the bundle is dated 2026-09-08 12:01. That number
  // entered the tree at 7bd1e727 (2026-09-07, "prepare version 1.0.3"), which bumped
  // `app/src-tauri/Cargo.toml` from 1.0.2 to 1.0.3 as the NEXT UNRELEASED stable version — the
  // convention `scripts/nightly.py` enforces in as many words ("Cargo.toml must contain the
  // next unreleased stable version"). No `v1.0.3` tag and no 1.0.3 release ever existed
  // (`git tag -l 'v1.0.*'` -> v1.0.0, v1.0.1, v1.0.2), and main moved on to 1.2.0 at d9cd1b6c.
  // So he was handed a build stamped with a version reserved for a release that never
  // happened, and the updater — comparing it against a manifest whose newest is 1.0.2 —
  // correctly found nothing newer and said so.
  //
  // THIS SURFACE WAS NOT THE LIAR, AND THAT IS PRECISELY WHY IT IS PINNED HERE. The number it
  // printed was the running build's own, which is the only number it may ever print. This
  // check is what keeps that true: drive the up-to-date headline from a `currentVersion` and
  // assert the sentence contains THAT string and no other version-shaped token — never the
  // manifest's, never a constant, never a blank where a version should be.
  await run.check("18. the version on screen is the running build's, and only that", async () => {
    const page = await openApp(browser);
    await settled(page);
    await openMenu(page);
    // Including his own case, so the regression has his number in it.
    for (const running of ["1.0.3", "1.0.2", "1.2.0-nightly.20260917.1", "0.1.0"]) {
      await setState(
        page,
        view({
          state: "upToDate",
          currentVersion: running,
          // A manifest version sitting in the same payload. If the headline ever reaches for
          // it instead, this is the check that says so.
          availableVersion: "9.9.9",
          checkedAt: Date.now(),
          endpointIsPlaceholder: false,
          endpoint: "https://u.example.com/x",
        })
      );
      const row = await readRow(page);
      assertEqual(
        row.headline,
        "RichOS " + running + " is up to date.",
        "the up-to-date headline is the running version and the sentence, nothing else"
      );
      const versions = (row.headline.match(/\d+\.\d+\.\d+(?:[-.][0-9A-Za-z.]+)?/g) || []);
      assertEqual(versions, [running], "a second version reached the headline: " + row.headline);
      // ...and the same number, unaltered, in every other state that names the running build.
      for (const state of ["idle", "checking", "unconfigured"]) {
        await setState(page, view({ state, currentVersion: running }));
        const other = await readRow(page);
        assert(
          other.headline.indexOf("RichOS " + running) === 0,
          state + " renamed the running build: " + other.headline
        );
      }
    }
    // AND THERE IS EXACTLY ONE PLACE IT COMES FROM. `updates.rs` sets `current_version` once,
    // at `init`, from `app.package_info().version`; nothing in the check/download/install path
    // writes it again, so a manifest can never rename the build the CEO is running. Read off
    // the Rust rather than trusted: a second assignment appearing here fails this check.
    // COMMENTS STRIPPED FIRST, which `setup.js` case 14 learned the hard way and this check
    // then repeated within the hour: the note above the test in `updates.rs` quotes the shape
    // it forbids (`x.current_version = ...`), so a naive scan matches the explanation and
    // reports the defect it exists to prevent.
    const writes = UPDATES_RS.split("\n")
      .map((line) => line.trim())
      .filter((line) => !line.startsWith("//"))
      .filter((line) => /\bcurrent_version\s*=[^=]/.test(line) && !line.includes("assert"));
    assertEqual(
      writes.length,
      0,
      "`current_version` is assigned after construction in updates.rs, so the running build " +
        "can be renamed by something other than the bundle it is"
    );
    assert(
      /let version = app\.package_info\(\)\.version\.to_string\(\);/.test(UPDATES_RS),
      "the running version no longer comes from the package the bundle was built as"
    );
    await page.close();
    return "four versions, five states each, one source in the Rust and no second writer";
  });

  // AUDIT D7, 2026-09-17: the update-server URL hard-wrapped mid-word — `…/githubus` /
  // `ercontent.com` in the nightly screenshot — because `.update-endpoint` was
  // `word-break: break-all`, which slices at ANY character. The fix is `main.js`'s
  // `wrapPathText` (exposed as `window.RichWrapPath`, called from `updates.js`): it inserts
  // a real `<wbr>` after every `/`, `.`, `-` and `_` in the endpoint, so a browser only
  // breaks at one of those unless a run between them is itself too long to fit — which is
  // what `overflow-wrap: anywhere` is left on the CSS side to catch.
  await run.check("19. the endpoint URL breaks at its own joints, not mid-word", async () => {
    const page = await openApp(browser);
    await settled(page);
    await openMenu(page);
    // The update server is a Technical view line (hunt 57, finding 19).
    await technicalOn(page);
    // The audited URL itself, verbatim.
    const auditedUrl = "https://raw.githubusercontent.com/WebDevBooster/richos/nightly-channel/latest.json";
    await setState(page, view({ state: "upToDate", endpointIsPlaceholder: false, endpoint: auditedUrl, checkedAt: Date.now() }));
    const audited = await page.evaluate(() => {
      const n = document.getElementById("update-endpoint");
      return { html: n.innerHTML, text: n.textContent };
    });
    assertEqual(
      audited.text,
      "Update server: " + auditedUrl,
      "the full URL is still present as text — a break opportunity is invisible, not a truncation"
    );
    // A `<wbr>` after every one of the URL's own separators, and the separators are still
    // literally in the text (the break sits AFTER them, not instead of them).
    const afterEverySeparator = auditedUrl.replace(/([/.\-_])/g, "$1<wbr>");
    assert(
      audited.html.indexOf(afterEverySeparator) !== -1,
      "expected a <wbr> after every '/', '.', '-' and '_' in the endpoint; got: " + audited.html
    );
    assert(
      (audited.html.match(/<wbr>/g) || []).length === (auditedUrl.match(/[/.\-_]/g) || []).length,
      "the number of <wbr>s does not match the number of separators in the URL — something is " +
        "either double-inserting them or skipping one kind of separator"
    );
    // THE POSITIVE CONTROL — a run with NO separator at all still has to wrap rather than
    // overflow the panel, which is exactly what `overflow-wrap: anywhere` is still on the
    // CSS side for: `<wbr>` cannot help a string with nothing to break after.
    const unbrokenRun = "https://" + "a".repeat(90) + ".example.com/x";
    await setState(page, view({ state: "upToDate", endpointIsPlaceholder: false, endpoint: unbrokenRun, checkedAt: Date.now() }));
    const control = await page.evaluate(() => {
      const n = document.getElementById("update-endpoint");
      const menu = document.getElementById("set-menu");
      return {
        text: n.textContent,
        overflowWrap: getComputedStyle(n).overflowWrap,
        endpointOverflowsMenu: n.getBoundingClientRect().right > menu.getBoundingClientRect().right + 1,
      };
    });
    assertEqual(
      control.text,
      "Update server: " + unbrokenRun,
      "the unbroken-run control's full text is present"
    );
    assertEqual(
      control.overflowWrap,
      "anywhere",
      "`.update-endpoint` must keep `overflow-wrap: anywhere` as the fallback for a run `<wbr>` cannot break"
    );
    assert(
      !control.endpointOverflowsMenu,
      "a 90-character run with no separator pushed the endpoint past the menu's own right edge — " +
        "`overflow-wrap: anywhere` did not do its one job once `<wbr>` ran out of options"
    );
    await page.close();
    return "the audited URL breaks after its own separators (" +
      (auditedUrl.match(/[/.\-_]/g) || []).length + " <wbr>s), and a 90-character run with none " +
      "still wraps instead of overflowing the menu";
  });

  // ---- 23. "See what's new" (CEO, 2026-10-19; round 34) ----------------------------------
  //
  // His words: the user can "click something somewhere in the appropriate place in the app that
  // will open their default web browser and get them to this page:
  // https://github.com/WebDevBooster/richos/blob/main/docs/CHANGELOG.md", which is
  // https://github.com/RichSpark/RichOS/blob/main/docs/CHANGELOG.md since the GitHub move
  // (2026-10-28). Asserted on what the
  // opener was HANDED (`mock.js` records `open_external`), never on a real browser: the key, and
  // the page that key is (`opener.rs` proves the key opens exactly that page). Both themes, both
  // ways in (Tab then Enter, and a click), and the page never leaves the app.
  await run.check("23. See what's new: on Check for updates' line, reached by Tab, opens the changelog outside the app", async () => {
    const CHANGELOG = "https://github.com/RichSpark/RichOS/blob/main/docs/CHANGELOG.md";
    const out = [];
    for (const theme of ["dark", "light"]) {
      const page = await openApp(browser, theme);
      await settled(page);
      await openMenu(page);
      await setState(page, view({ state: "upToDate", currentVersion: "1.2.0", checkedAt: Date.now() - 12 * 60000 }));
      const link = await page.evaluate(() => {
        const a = document.getElementById("update-notes");
        if (!a) return null;
        const c = document.getElementById("update-check").getBoundingClientRect();
        const r = a.getBoundingClientRect();
        return {
          tag: a.tagName.toLowerCase(),
          text: a.querySelector(".update-notes-text").textContent,
          name: a.getAttribute("aria-label"),
          words: a.textContent,
          href: a.getAttribute("href"),
          tabindex: a.getAttribute("tabindex"),
          visible: r.width > 0 && r.height > 0,
          sameLine: Math.abs((r.top + r.bottom) / 2 - (c.top + c.bottom) / 2) <= 2 && r.left > c.right,
        };
      });
      assert(link, theme + ": there is no #update-notes link in the Updates row");
      assertEqual(link.tag, "a", theme + ": the control is a link");
      assertEqual(link.text, "See what’s new", theme + ": the link's words");
      assertEqual(link.name, "See what’s new (opens in your web browser)", theme + ": what a screen reader hears");
      assertEqual(link.words, "See what’s new", theme + ": no hidden words laid out inside the link (they ran past the column)");
      assertEqual(link.href, CHANGELOG, theme + ": the link names the changelog page");
      assertEqual(link.tabindex, "0", theme + ": WebKit skips a plain link on Tab without tabindex=0");
      assert(link.visible && link.sameLine, theme + ": on the same line as Check for updates, after it: " + JSON.stringify(link));

      // WCAG AA, computed: the label (16px, 4.5:1) and the focus ring (3:1) against the panel
      // they sit on, at rest and on the hover tint composited over that panel.
      const ratios = async () => page.evaluate(() => {
        const rgba = (s) => { const m = s.match(/[\d.]+/g).map(Number); return { r: m[0], g: m[1], b: m[2], a: m.length > 3 ? m[3] : 1 }; };
        const over = (top, under) => ({ r: top.r * top.a + under.r * (1 - top.a), g: top.g * top.a + under.g * (1 - top.a), b: top.b * top.a + under.b * (1 - top.a), a: 1 });
        const lum = (c) => { const f = (v) => { v /= 255; return v <= 0.03928 ? v / 12.92 : Math.pow((v + 0.055) / 1.055, 2.4); }; return 0.2126 * f(c.r) + 0.7152 * f(c.g) + 0.0722 * f(c.b); };
        const ratio = (x, y) => { const a = lum(x), b = lum(y); return (Math.max(a, b) + 0.05) / (Math.min(a, b) + 0.05); };
        const a = document.getElementById("update-notes");
        let n = a.parentElement, panel = null;
        while (n && !panel) { const bg = rgba(getComputedStyle(n).backgroundColor); if (bg.a === 1) panel = bg; n = n.parentElement; }
        const s = getComputedStyle(a);
        const ground = over(rgba(s.backgroundColor), panel);
        const ink = rgba(getComputedStyle(a.querySelector(".update-notes-text")).color);
        return { text: Math.round(ratio(ink, ground) * 100) / 100, ring: Math.round(ratio(rgba(s.outlineColor), panel) * 100) / 100 };
      });
      const rest = await ratios();
      await page.hover("#update-notes");
      await awaitSettled(page);
      const hover = await ratios();
      assert(rest.text >= 4.5 && hover.text >= 4.5, theme + ": the label is below 4.5:1 — rest " + rest.text + ", hover " + hover.text);

      // Tab from Check for updates lands on the link; Enter hands the opener the changelog.
      await page.mouse.move(0, 0);
      await page.focus("#update-check");
      await page.keyboard.press("Tab");
      const focus = await page.evaluate(() => {
        const a = document.activeElement;
        const s = getComputedStyle(a);
        return { id: a.id, ring: s.outlineStyle + " " + s.outlineWidth };
      });
      assertEqual(focus.id, "update-notes", theme + ": Tab from Check for updates");
      assertEqual(focus.ring, "solid 2px", theme + ": the keyboard focus ring");
      const ring = await ratios();
      assert(ring.ring >= 3, theme + ": the focus ring is below 3:1 — " + ring.ring);
      const before = { url: page.url(), pages: page.context().pages().length };
      await page.keyboard.press("Enter");
      await page.waitForFunction(() => (window.__RICHOS_OPENED__ || []).length === 1);
      await page.click("#update-notes");
      await page.waitForFunction(() => (window.__RICHOS_OPENED__ || []).length === 2);
      const opened = await page.evaluate(() => window.__RICHOS_OPENED__);
      for (const key of opened) assertEqual("https://" + key, CHANGELOG, theme + ": the opener was handed the changelog");
      assertEqual(page.url(), before.url, theme + ": the app window did not navigate");
      assertEqual(page.context().pages().length, before.pages, theme + ": no window was opened inside the app");

      if (theme === "dark") await evidence(page, SHOTS + "/updates-whats-new-focus");
      else {
        await page.mouse.move(0, 0);
        await page.evaluate(() => document.activeElement.blur());
        await flushFrames(page);
        await evidence(page, SHOTS + "/updates-whats-new-light");
      }
      await page.close();
      out.push(theme + " label " + rest.text + " rest / " + hover.text + " hover, ring " + ring.ring);
    }
    return "Enter and a click each handed the opener " + CHANGELOG + "; " + out.join("; ");
  });

  await browser.close();
  const failed = run.report();
  process.exitCode = failed ? 1 : 0;
}

// =========================================================================================
// THE MUTATIONS — every check above was RUN RED, by breaking the SHIPPED source
// =========================================================================================
//
// Recorded as run on 2026-08-31, with the ACTUAL set of checks each one reddened rather
// than the set I expected. Two of them reddened more than their own check, and that is
// reported here rather than tidied away: a mutation that takes ten checks down with it says
// something true about how much of this surface rests on one line.
//
//    #  mutation (all against the shipped source, then restored)          checks reddened
//   ---------------------------------------------------------------------------------------
//    1  settings-button.js: deleted `if (updates) menu.appendChild(          1,2,3,4,5,
//       buildUpdatesRow())` — the row stops existing                         6,7,8,9,10
//    2  updates.js: made `sentences()`'s `default:` arm return the           2
//       `upToDate` sentence — an unknown state claiming the app is current
//    3  updates.js: made the `idle` arm return the `upToDate` sentence       3
//    4  updates.js: dropped `|| v.state === "unconfigured"` from the         4
//       Check button's `disabled`
//    5  updates.js: removed `&& !isSignature(v)` from the Check label —      5
//       a refused signature offered "Try again"
//    6  updates.js: made `nodes.detail.textContent` a 40-character slice     6
//    7  updates.js: made the `pct === null` arm unreachable, so an           7
//       unknown total painted a determinate bar
//    8  updates.js: made `marker()` mark every state but `ready` and         8
//       `unconfigured`
//    9  updates.js: made the Restart button call `update_check`              9
//   10  updates.js: `render()` builds only when `!nodes`, never comparing    10
//       `nodes.container` — paints into the detached pre-rebuild tree
//   11  updates.js: threw inside `sentences()`'s `installing` arm            2,8,11
//
// Check 11 (no page errors) is the one that catches #11, and #11 also reddens 2 and 8
// because both drive the `installing` state on their way through.
//
// THE CUE (checks 12-14), run 2026-09-04, same rule: every mutation below was applied to the
// SHIPPED source, the suite run, the file restored and compared byte for byte against its
// backup. The reddened set is the one that was OBSERVED, not the one expected.
//
//   12  updates.js: `cue()`'s `want` can never be empty — the pill is built for       12,13
//       every state and merely changes its label. THE defect the ruling names: a
//       permanent control that occasionally changes color is one an eye stops seeing
//   13  updates.js: `cue()`'s host lookup returns null, so the element is never       12,13,14
//       built at all — the state this surface was in before the placement ruling
//   14  updates.js: `openTheRow` calls `update_install` instead of opening the        14
//       menu — the announcement quietly becomes §26's mode 1, which the CEO ruled
//       on 2026-09-04 is a design session of its own
//   15  updates.js: the cue writes its own label, `"Update"`, instead of              12,14
//       `sentences(v).headline` — the version stops being named and the chrome and
//       the row become two vocabularies that agree until one of them is edited
//   16  updates.js: the cue is appended AFTER the settings button instead of          13
//       inserted before it, so the button the CEO already aims at moves out from
//       under his pointer the moment an update arrives
//
// MUTATION 12 IS THE ONE THAT PAID FOR ITSELF, and it is recorded here rather than tidied
// away. On its first run it reddened check 12 and left check 13 GREEN — because `readCue`
// measured the wrapper's geometry only on the branch where no cue existed, so check 13's
// footprint assertion was comparing `undefined` with `undefined` and could not fail. The
// geometry is measured on both branches now. A negative assertion that cannot fail is not a
// check, and only running the mutation says which is which.
//
// THE WORK GATE (checks 15-17), run 2026-09-05, same rule: every mutation below was applied
// to the SHIPPED source, the suite run, the file restored and compared by sha256 against its
// backup. The reddened set is the one that was OBSERVED, not the one expected.
//
//   17  updates.js: dropped `|| !!v.busy` from `nodes.install.hidden` — the control        15
//       he cannot safely press is offered again. THE defect the CEO found: "I suspect
//       that the current thread would die if I were to press that update button"
//   18  updates.js: `marker()`'s `v.busy ? "" :` short-circuited to false — the mark on     15
//       the settings button survives, saying LOOK AT ME with nothing safe to look at
//   19  updates.js: dropped `!v.busy` from `cue()`'s `want` — the actionable pill          15,16
//       survives, so the chrome offers the press and the waiting cue sits beside it
//   20  updates.js: `waitingCue` builds a `<button type="button">` instead of a            16
//       `<span role="note">`. It LOOKS identical and it is announced as a control to
//       every screen reader and offered to every voice user — "not an actionable
//       thing" is a claim about the markup, which is why the check reads the tag
//   21  updates.js: removed the `focus` listener, leaving the note hover-only — the        16
//       exact failure the requirement names, invisible to a keyboard and to a tap
//   22  updates.js: `gateClauses` stops appending `v.unchecked` — the reassurance          17
//       silently starts implying a check the gate never made
//   23  updates.js: `readyFor`'s threshold moved from `days < 1` to `days < 0`, so         17
//       every update claims an age and the line stops meaning anything
//   24  updates.js: the two reassurance sentences replaced with "RichOS will install       15
//       it when your work is finished" — 26's mode 1 promised over a product that
//       still needs a button press. It is the sentence the brief originally asked for
//       and it is the one thing here that would have shipped a lie
//   25  updates.js: `nodes.back.hidden` reduced to `!backTo`, dropping the busy and          20
//       already-prepared clauses — the control comes back while RichOS is working,
//       which is precisely the button the CEO said he would not dare press
//   26  updates.js: the Go back listener issues `update_install` instead of                  21
//       `update_rollback` — the row still looks right and the wrong command goes out
//
// MUTATION 19 IS WORTH THE LINE IT TAKES. It reddened 16 as well as 15, unprompted, because
// check 16 asserts `cueCount === 0` beside its own existence — the two cues are one rule with
// two branches, and a mutation that breaks the rule breaks both branches. That is the
// difference between two checks that agree and two checks that are the same check twice.
//
// AND THE POSITIVE PROBE FOR THE CONTRAST HALF, because a green walk over an element nobody
// looked at is the same as no walk: `.update-cue-label`'s ink was repainted `#b09a6a` and
// `contrast.js` was run. It failed `9.updates-available` by name, in BOTH themes — 1.13:1 on
// the dark fill and 1.44:1 on the light one — which is the evidence that the 7.68 / 4.72 the
// shipped pair computes to is a measurement of this element and not of an assumption about
// it. The backgrounds WebKit resolved (`#c2a35c` and `#9c7c34`) are the two tokens the
// stylesheet's ledger says they are.
//
// The waiting cue got the same treatment on 2026-09-05: `contrast.js` grew an
// `updates-waiting` surface (the only one that can paint an element which exists only while
// `busy`, and it opens the note by FOCUS rather than hover so the pixels walked are the ones
// a keyboard user meets), and `.update-waiting-note`'s ink was repainted `#7c8496`. It failed
// `9.updates-waiting` by name in BOTH themes — 3.65:1 light, 4.1:1 dark, on a 16px node — so
// the 12.06 / 18.07 the shipped pair computes to is a measurement of that element. The
// grounds WebKit resolved (`rgb(253, 252, 248)` and `rgb(24, 36, 64)`) are `--surface` in
// each theme, which is what `style.css`'s ledger says they are.
// =========================================================================================

main().catch((e) => {
  console.error(e);
  process.exit(1);
});

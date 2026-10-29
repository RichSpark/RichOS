// THE CONTRAST FLOOR, ENFORCED — the standing rule in `CLAUDE.md` §"Contrast — WCAG AA,
// ALWAYS, BOTH THEMES", turned from a sentence into something that can fail.
//
// The CEO's order was: stop having to say this. It was answered by writing the rule into
// `CLAUDE.md` and into fifteen agent definitions, which is exactly the shape of answer that
// does not work — a rule written into a prompt is a request, and a request is complied with
// at whatever rate people comply with requests. The four light-mode failures that provoked
// the order (2.95, 2.95, 3.15, 3.60 against a floor of 4.5) were each produced by someone
// who had already been told the rule and who looked at the result and thought it looked
// fine. An eye adapts to the palette it has been staring at. Arithmetic does not.
//
// So this suite computes the ratio. Every visible run of text on the shipping shell, every
// non-text indicator, against the colours actually painted behind it, in both themes.
//
// THE EXEMPTION IS THE LOAD-BEARING PART, and getting it wrong is how this file dies. The
// standing rule exempts text deliberately not meant to be read closely — a privacy notice,
// legal boilerplate, fine print — and nothing else. A checker that ignored that would flag
// the whole of any legal footer, and a checker that flags what everybody already decided is
// fine gets muted inside a week; a muted checker's green is worse than no checker, because
// it converts "nobody looked" into "something looked and it was fine". So the exemption is
// MACHINE-READABLE: `data-contrast-exempt="<why this text is not meant to be read closely>"`
// in the shipped markup. A declared exemption passes AND IS PRINTED, with its reason, in the
// report of every run. An undeclared one fails. That inverts the CEO's declaration
// requirement into the enforcement mechanism: the only way to be excused is to say so where
// a reviewer reads it, and the count of exemptions becomes a number that can be watched for
// creep — check 12 prints it.
//
// AN UNRESOLVABLE COLOUR IS A FAILURE TO PROVE, NEVER A PASS. Gradients, blend modes,
// `background-clip: text`, an unparseable colour notation, a panel painted over the node:
// each one is a place a lazy implementation returns "fine". Check 7 introduces one of each
// into a live page and asserts the walk refuses all of them.
//
// WHAT THIS GATE COVERS AND WHAT IT CANNOT — the honest boundary, asserted rather than
// promised:
//
//   COVERS   the driven surfaces below — twenty-four of them as this is written, and the
//            number is printed by every run rather than quoted here, because a count in a
//            comment is the thing that goes stale first. This header said "thirteen" over a
//            list of twenty-one until 2026-09-05. Both themes, text, and the bounded
//            non-text-indicator subset check 3 defines. The three `updates-*` surfaces are
//            also the first drivers to reach the UNIVERSAL settings menu at all — the
//            `settings` surface drives the RAIL's preferences popover, which is a different
//            menu — and their first run found a shipped 1.24:1 indicator in it. The opening screen is WALKED and its one HTML line comes back
//            UNPROVABLE — see `knownUnresolvable` in contrast-debt.json. That is a stated
//            blind spot with a name on it, which is not the same thing as coverage.
//   COVERS   and this is check 10c's doing rather than anyone's diligence: `entity-view`,
//            `thread-menu` and `techy-nothing-recorded`. All three are panels the shipped
//            shell declares, all three carry text the CEO reads, all three are reachable in
//            a browser with nothing stubbed, and no driver opened any of them. 10c derives
//            the panel inventory from the shell and refuses an unwalked panel that nobody
//            wrote down; on the day it landed it named ten, and seven of those are honest
//            gaps now recorded in `contrast-debt.json`'s `unwalkedPanels` with their reasons.
//   CANNOT   `<canvas>` — no computed style to read. The shipping shell contains none, and
//            check 14 asserts that, so the day one lands the assertion is what tells you.
//            The round-7 and round-8.1 material studies ARE canvas-heavy and NOTHING here
//            says anything about them. "We have a contrast check now" does not cover canvas.
//   CANNOT   SVG `fill`/`stroke`. The opening screen is almost entirely SVG — check 10
//            counts what it therefore does not measure and prints the number.
//   CANNOT   states no driver reaches: the voice panel MID-LISTEN, the drill-down slideover,
//            the between-turns strip, streaming mid-flight. Named, not hidden.
//   COVERS   and this one was in that CANNOT list until 2026-09-17: the voice panel's SPEECH-MODEL
//            rows. The reason it was excluded — "reaching it needs a live audio device" — was
//            true of every row the panel had. It is not true of these three: the offer, the
//            transfer and the refusal all paint with no microphone opened, on a machine that
//            cannot transcribe, which is the entire point of them. Three drivers walk them, so
//            the ratios in `style.css`'s own comment block are now arithmetic this gate performs
//            rather than arithmetic an engineer performed once.
//
// A NOTE ON THE DEBT LEDGER, because it is the one thing here that could be mistaken for a
// weakened threshold. It is not. The threshold is 4.5/3.0 and never moves; every failure is
// computed, named and printed with its ratio on every run. `contrast-debt.json` records what
// the shipping app was ALREADY failing the day the gate was built, so that a red run means
// "someone made this worse today" rather than "this repository has old debt", which is the
// signal a developer learns to ignore. A signature not in the ledger fails. A signature in
// the ledger whose ratio got worse fails. The ledger's size is capped at what it was
// committed with, so it can only shrink.
//
// EVERY CHECK HERE WAS RUN RED ONCE by breaking the shipped source; the mutations are listed
// against their check numbers at the bottom of this file.
//
// Run: node contrast.js   (or `npm test` for every suite in this directory)
//      RICHOS_PLAYWRIGHT=/path/to/node_modules/playwright node contrast.js

"use strict";

const fs = require("fs");
const path = require("path");
const {
  leaveHome,
  loadPlaywright,
  shot,
  publishShotFile,
  createRun,
  assert,
  assertEqual,
  awaitSettled,
  bootSettled,
  shellSettled: harnessShellSettled,
  settleOnThread,
  assertOnThread,
  HOLD_CURTAIN,
  assertCurtainHeld,
  SLOW_BRIDGE,
  paintedGround,
  UI_DIR,
} = require("./lib/harness");
const C = require("./lib/contrast");
const SOURCES = require("./lib/ui-sources");

const APP = "file://" + path.join(UI_DIR, "index.html");
const SHOTS = path.join(__dirname, ".shots", "contrast");
const DEBT_FILE = path.join(__dirname, "contrast-debt.json");
// THE TWO SOURCE LISTS ARE DERIVED, AND BOTH WERE SHORT.
//
// `CSS_FILES` was `style.css` + `splash.css` against three linked stylesheets — the 53 KB
// `home.css` was missing, so check 10's count of second-theme rules was taken over two
// thirds of the shipped CSS. `SOURCE_FILES` was four files against twelve, so check 12's
// exemption inventory — the mechanism that makes an undeclared exemption a failure — could
// not have seen a `data-contrast-exempt` written into `home.js`, `updates.js`,
// `settings-button.js`, `splash-library.js`, `theme-boot.js` or the three `home/field-*.js`
// modules. An exemption is a claim that some text is skippable; a gate that cannot read the
// file the claim is written in is not counting the claims.
//
// `lib/ui-sources.js` derives both from `index.html` and the closure over it, and refuses to
// return a manifest that does not reconcile with the tree in both directions. Adding a
// stylesheet or a script to the shell adds it to both lists here.
const CSS_FILES = SOURCES.styleSources().map(SOURCES.abs);
const SOURCE_FILES = SOURCES.stateSources().map(SOURCES.abs);

const THEMES = ["light", "dark"];

/// A DELIBERATE SLOW RUNNER, on demand: `RICHOS_CONTRAST_LAG_MS=120 node contrast.js`. Same
/// idea as `splash.js`'s `RICHOS_SPLASH_LAG_MS`, applied at the seam that actually matters
/// here — every `RichBridge.invoke` round trip. A `macos-latest` runner is not just late to
/// start; it is slower at each of the calls a driver sets off and does not wait for, and that
/// is the difference that decided run 34010691469. Zero, and no delay at all, unless it is set.
const LAG_MS = Number(process.env.RICHOS_CONTRAST_LAG_MS || 0);

/// WAIT UNTIL THE HIRING THREAD IS ACTUALLY ON THE STAGE, and not for a length of time.
///
/// WHY THIS EXISTS. Every `assignment-*` surface below starts by clicking the hiring thread.
/// That click sets off `switch_thread`, `active_context`, `techy_mode` and `get_timeline`, and
/// until 2026-09-06 nothing waited for any of them: the driver installed its fixture bridge
/// and called `RichRuns.show("hiring")` immediately, and the walk measured whatever was on
/// screen. On this machine those calls finish in about a millisecond. On a slower one they do
/// not.
///
/// MEASURED at `934f127` with 120 ms on each bridge call, which is this suite's own knob above:
/// the `assignment-running` walk ran against a shell showing the PREVIOUS thread — the stage
/// header read "Harbor Analytics / Running" rather than "Northwind Traders / Q4 hiring", and
/// the hiring thread's turn (its user message, its stamp, its duration row, Rich's prose) was
/// not in the DOM at all. Sixteen text nodes missing, eight of the old thread's present in
/// their place, and 210 of 218 nodes considered per theme against a floor of 218.
///
/// THE FLOOR IS WHAT CAUGHT IT, AND ONLY JUST. Run 34010691469 reported 428 across both themes
/// against a floor of 436 — one theme raced, one did not. Had both cleared the floor the suite
/// would have reported a clean contrast sweep of `assignment-running` having walked a
/// different thread, which is the failure this file's own header calls the worst kind: not
/// "nobody checked" but "somebody checked and it was fine".
///
/// So the wait is on the END STATE the surface is named for — the crumb, the active rail row
/// and the thread's own turns, all three — rather than on a timeout somebody widened.
/// WAIT FOR THE SHELL TO HAVE FINISHED ARRIVING, rather than for 300 ms.
///
/// Two things in the rail are filled by their own asynchronous reads, independent of the
/// thread and of every driver below: the corrections count (`#nav-corrections-count`) and the
/// retention hint in the assertiveness popover (`#retention-hint`, "Nothing is ever removed.
/// Using 66 MB now."). Measured at `934f127` with 60 ms on each bridge call, both were still
/// empty when every driver returned — two text nodes per theme, four across the pair, which
/// is more margin than any floor in `contrast-debt.json` has. Those floors were set on a
/// settled shell, so this waits for the shell they were set on.
///
/// It is a wait on the two elements' own end state and not a longer sleep: on this machine it
/// returns on the first poll, and on a slower one it returns when the reads land instead of
/// when a number somebody picked runs out.
/// MOVED TO `lib/harness.js`, and this line is what is left of it. It turned out to be two
/// facts in one wait: the ink this file has to measure, and `init()` having run every one of
/// its statements — which is what `updates.js` needed, for a focus steal rather than for a
/// text node. The reasoning above is this file's; the implementation is shared.
const shellSettled = (p) => harnessShellSettled(p);

/// Wait for an overlay's ENTRY ANIMATION to have finished, on the animation's own clock.
///
/// `.overlay-panel` carries `animation: overlay-in 0.16s ease-out`, which runs opacity from 0
/// to 1. A walk that samples partway through measures the panel's text against a background
/// the panel is only half covering, and every ratio it computes is of a frame nobody will ever
/// look at. That is not a hypothetical: replacing this suite's `waitForTimeout(400)` with a
/// state wait, and nothing else, made the feedback desk report 13 NEW failures headed by
/// `p#feedback-title` at 1.39:1 — `#9b9994 on #b9b5ab`, two colors that exist nowhere in
/// either palette because one of them is a blend.
///
/// `Element.getAnimations({ subtree: true })` is the exact end state and this WebKit has it
/// (probed at `934f127`: one animation found, 192 ms awaited, opacity 1 and zero animations
/// left). The two frames afterwards are the paint.
async function overlaySettled(p, selector) {
  await p.waitForSelector(selector, { state: "visible" });
  await p.evaluate(async (sel) => {
    const el = document.querySelector(sel);
    if (!el) return;
    await Promise.all(el.getAnimations({ subtree: true }).map((a) => a.finished.catch(() => {})));
    await new Promise((r) => requestAnimationFrame(() => requestAnimationFrame(r)));
  }, selector);
}

/// A PSEUDO-ELEMENT'S OWN TRANSITION, waited out rather than counted out.
///
/// `.setbtn::after` is `transition: opacity 0.15s ease 0.2s` — a 200 ms delay before a 150 ms
/// fade — and this file's own comment did that arithmetic and then wrote 500. The arithmetic
/// was right and it is still a bet: it says how long the transition takes on a machine that
/// starts it immediately, and a busy one does not.
/// `getComputedStyle(el, "::after").opacity` is the pseudo-element's CURRENT value, which is
/// the thing being waited for, so this returns when the tooltip is actually opaque rather than
/// when a stopwatch says it should be.
///
/// It matters more here than almost anywhere: this surface exists to measure ONE string, and a
/// walk that samples it at 0.83 opacity measures a blend of the tooltip and whatever is behind
/// it — which is exactly how `p#feedback-title` came to be reported at 1.39:1 against two
/// colors that exist in neither palette.
/// The update row is showing the state it was set to, in a menu that has finished opening.
///
/// `#set-menu` being `visible` is a box, and `updateSet` reaching the row is a separate fact
/// from the menu having arrived — `RichSettings.openMenu` also calls `onOpen()`, which re-reads
/// `update_state` over the bridge and repaints the row from the answer. So the state a walk
/// measures is the one that read produced, not the one the driver set, and 400 ms was a bet on
/// the two agreeing in time. `data-update-state` is the row's own published answer.
async function updatesRowShowing(p, state) {
  await p.waitForSelector("#set-menu", { state: "visible" });
  await p.waitForFunction(
    (want) => {
      const row = document.getElementById("set-updates");
      return !!row && row.getAttribute("data-update-state") === want && row.getAttribute("data-update-busy") !== "true";
    },
    state,
    { timeout: 10000 }
  );
  // AND THE MENU'S OWN RISE, which is the other half and was the half the 400 ms was really
  // paying for. `.setmenu` carries `animation: setrise 0.2s ... both`, opacity 0 -> 1 over a
  // 14px translate — so a walk that stopped at "the row says `available`" measures the menu's
  // ink blended into whatever is behind it. MEASURED, on the first run of this file with the
  // state wait in and this line out: 23 NEW failures on `updates-available` and 23 on
  // `updates-downloading`, headed by `span#set-theme-label` at 3.82:1, 3.34:1 and 3.5:1 — three
  // different ratios for one element across two surfaces, which is the signature of a blend
  // rather than of a defect. With this line in, all 46 are gone and the surfaces read as they
  // did before. A determinism fix that manufactures failures is not a fix.
  await overlaySettled(p, "#set-menu");
}

async function pseudoOpaque(p, selector, pseudo) {
  await p.waitForFunction(
    ([sel, ps]) => {
      const el = document.querySelector(sel);
      if (!el) return false;
      return parseFloat(getComputedStyle(el, ps).opacity) >= 0.999;
    },
    [selector, pseudo],
    { timeout: 10000 }
  );
  await p.evaluate(() => new Promise((r) => requestAnimationFrame(() => requestAnimationFrame(r))));
}

/// Every FINITE animation and transition anywhere on the page has finished.
///
/// `overlaySettled` above asks the same question of one subtree and is the right tool when the
/// thing that moved is a panel. This is the whole-page form, for the surfaces where what
/// settles is not inside one container — a menu that rebuilds the chrome around it, a pane
/// swap, a row that appears beside a button.
async function pageSettled(p) {
  await awaitSettled(p);
  await p.evaluate(() => new Promise((r) => requestAnimationFrame(() => requestAnimationFrame(r))));
}

async function atHiringThread(p) {
  await p.waitForFunction(() => {
    const crumb = document.getElementById("scope-thread");
    const active = document.querySelector(".nav-thread-row.is-active .nav-thread");
    return (
      !!crumb &&
      crumb.textContent.trim() === "Q4 hiring" &&
      !!active &&
      active.dataset.threadId === "hiring" &&
      document.querySelectorAll("#messages .tl-turn").length > 0
    );
  });
}

/// The same three facts, asserted rather than waited for. Used after a driver has finished, so
/// a late answer that arrives from the click and re-renders the stage over the fixture is a
/// named failure instead of a node count that happens to clear its floor.
async function assertStillHiring(p, surface) {
  const on = await p.evaluate(() => ({
    crumb: ((document.getElementById("scope-thread") || {}).textContent || "").trim(),
    entity: ((document.getElementById("scope-entity") || {}).textContent || "").trim(),
    active: (document.querySelector(".nav-thread-row.is-active .nav-thread") || { dataset: {} }).dataset.threadId,
    turns: document.querySelectorAll("#messages .tl-turn").length,
  }));
  assert(
    on.crumb === "Q4 hiring" && on.active === "hiring" && on.turns > 0,
    surface + " is not on the thread it is named for when the walk starts — the stage reads " +
      JSON.stringify(on.entity + " / " + on.crumb) + ", active thread " + JSON.stringify(on.active) + ", " +
      on.turns + " turn(s). Whatever this walk measures, it is not the surface in its title"
  );
}

// ---------------------------------------------------------------------------------------
// The surfaces. A driver's job is to put the app into a state and RETURN — the walk is the
// same everywhere, which is what makes adding a surface one entry rather than one suite.
// ---------------------------------------------------------------------------------------

const quotaNow = Date.now();
const quotaOffer = { id: "launch", label: "Claude weekly reset", remaining: 1,
  startsAt: quotaNow - 86400000, expiresAt: quotaNow + 86400000,
  clears: ["five_hour", "seven_day"], usableNow: true, requiresLimit: false };
const quotaReading = {state: "fresh", checkedAt: quotaNow, retryAt: null, message: null,
  windows: [{id: "five_hour", label: "Five-hour", usedPercent: 94, resetsAt: quotaNow + 7200000, durationMs: 18000000},
    {id: "seven_day", label: "Weekly", usedPercent: 32, resetsAt: quotaNow + 345600000, durationMs: 604800000}],
  resets: {state: "fresh", checkedAt: quotaNow, offers: [quotaOffer], approval: null, lastAttempt: null, weeklyThreshold: 99}};
async function openQuota(p) {
  await p.click("#set-btn");
  // A click, not `check`: the switch asks first (the scope sheet, over the menu since round 20.2).
  await p.click("#set-techy");
  await p.waitForFunction(() => !document.getElementById("set-quota-open").hidden || !document.getElementById("techy-scope").hidden);
  if (await p.locator("#techy-scope").isVisible()) await p.click("#techy-scope-confirm");
  if (await p.locator("#set-menu").isHidden()) await p.click("#set-btn");
  await p.click("#set-quota-open");
  await p.waitForSelector(".quota-window");
  await overlaySettled(p, "#quota-sheet");
}

const SURFACES = [
  {
    name: "shell",
    what: "the shell as it opens: rail, scope line, first turn, composer",
    drive: async () => {},
  },
  ...["offer", "approved", "uncertain"].map(state => ({
    name: "quota-" + state,
    what: "desktop quota windows and the " + state + " weekly reset state",
    preset: {quota: {...quotaReading, resets: {...quotaReading.resets,
      approval: state === "approved" ? {offer: quotaOffer, approvedAt: quotaNow, weeklyThreshold: 99} : null,
      lastAttempt: state === "uncertain" ? {grantId: "launch", outcome: "uncertain"} : null}}},
    drive: async p => {
      await openQuota(p);
      await p.waitForSelector(state === "approved" ? "#quota-reset-revoke" : state === "offer" ? "#quota-reset-prepare-launch" : ".quota-reset-result");
    },
  })),
  {
    name: "quota-work-status",
    what: "a paused agent's conversation status, without technical quota controls",
    preset: {quota: quotaReading, quotaActivity: {held: [{kind: "agent", id: "agt_frank_1", name: "Frank", threadId: "general", sinceAt: quotaNow}], released: []}},
    drive: async p => {
      await p.evaluate(() => window.RichBridge.invoke("set_claude_quota_policy", {policy: {enabled: true, pausePercent: 93}}));
      await p.waitForSelector("#quota-work-status:not([hidden])");
      assert((await p.locator("#quota-work-status").innerText()).includes("Their work is saved."));
    },
  },
  {
    // Round 18 (the CEO's feedback item 9): Claude accounts, for everyone. Every state of it is
    // also walked, both themes, by tests/accounts.js; this is the shell-wide walk of its sheet.
    name: "accounts-sheet",
    what: "Claude accounts: two account cards, the handover line, the 5-hour choice and Recent changes",
    preset: {quota: {...quotaReading, accounts: [
      {id: "1", label: "Home", inUse: true, windows: quotaReading.windows, checkedAt: quotaNow, exhaustedUntil: null, message: null},
      {id: "2", label: "Work", inUse: false, windows: [{...quotaReading.windows[0], usedPercent: 0}, {...quotaReading.windows[1], usedPercent: 12}], checkedAt: quotaNow, exhaustedUntil: null, message: null}],
    changes: [{at: quotaNow - 3600000, kind: "added", id: "2", label: "Work"}]}},
    drive: async (p) => {
      await p.click("#set-btn");
      await p.click("#set-accounts-open");
      await p.waitForSelector("#accounts-sheet .acc-card");
      await overlaySettled(p, "#accounts-sheet");
    },
  },
  {
    // Round 41 piece 1. Every state of it is also measured, both themes, by
    // tests/terminal-and-git.js; this is the shell-wide walk of its sheet, read again once so
    // What changed (each change's sign, word and color) is on screen with What it found.
    name: "terminal-setup",
    what: "Terminal setup after Read it again: the status, What changed, the tool folders and the settings by name",
    drive: async (p) => {
      await p.click("#set-btn");
      await p.click("#set-terminal-open");
      await p.waitForFunction(() => document.querySelector("#ts-read") && !document.querySelector("#ts-read").disabled);
      // Pressed without the pointer, so the walk measures the button at rest, not hovered.
      await p.evaluate(() => document.querySelector("#ts-read").click());
      await p.waitForSelector("#ts-changes", {state: "visible"});
      await p.waitForFunction(() => !document.querySelector("#ts-read").disabled);
      await overlaySettled(p, "#terminal-setup");
    },
  },
  {
    // Round 41 piece 2, changed later from Settings; also measured by tests/terminal-and-git.js.
    name: "git-name-sheet",
    what: "Your name in Git from Settings: the name and email fields, the GitHub hint and Save",
    drive: async (p) => {
      await p.click("#set-btn");
      await p.click("#set-git-open");
      await p.waitForFunction(() => document.activeElement && document.activeElement.id === "gn-email");
      await overlaySettled(p, "#git-name-sheet");
    },
  },
  {
    name: "repositories",
    what: "the repository connection dialog and its explicit company selection",
    drive: async (p) => {
      await p.click("#set-btn");
      await p.click("#set-repositories-open");
      await p.waitForSelector("#repository-company option:nth-child(2)", {state: "attached"});
      await overlaySettled(p, "#repositories-sheet");
    },
  },
  {
    // ONE FOLDER, TWO COMPANIES (round 36 screen 2, plan item 11, CEO 2026-10-21): the folder is
    // connected to the first company, then chosen for the second, so the sheet shows the gold
    // notice "Already connected to ..." and the button "Connect to ... too" before anything is
    // connected. Driven through the sheet's own controls, no preset.
    name: "repositories-shared",
    what: "the connect sheet with a folder another company already has: the notice and the button",
    drive: async (p) => {
      await p.click("#set-btn");
      await p.click("#set-repositories-open");
      await p.waitForSelector("#repository-company option:nth-child(3)", {state: "attached"});
      await overlaySettled(p, "#repositories-sheet");
      const first = await p.locator("#repository-company option").nth(1).getAttribute("value");
      const second = await p.locator("#repository-company option").nth(2).getAttribute("value");
      await p.selectOption("#repository-company", first);
      await p.fill("#repository-folder", "/fictional/shared project");
      await p.click("#repository-connect");
      await p.waitForFunction(() => document.getElementById("repository-message").textContent.startsWith("Folder connected."));
      await p.selectOption("#repository-company", second);
      await p.fill("#repository-folder", "/fictional/shared project");
      await p.waitForSelector(".repository-shared");
      await overlaySettled(p, "#repositories-sheet");
    },
  },
  {
    // TWO SURFACES FOR ONE PANEL, because the panel has two states and they share almost no
    // words. The pairing state carries the code, the countdown, the four phone steps and the six
    // words he COMPARES; the paired state carries the device card and what forgetting it does.
    // Walking one of them and printing "phone: covered" would leave the other unmeasured.
    //
    // THE PAIRING STATE NEEDS A READY TAILNET NOW (CEO §61). It used to be reachable with no
    // Tailscale at all, because the removed path served that Mac; the one path refuses to, and a
    // fixture that cannot reach the screen would walk an empty panel and report it covered.
    //
    // AND THE COUNTDOWN IS PINNED, because this surface is PHOTOGRAPHED as well as walked.
    // `mock.js` starts the 300 s window when it loads, so `Math.ceil((300000 - elapsed)/1000)`
    // is 300 for the first second of the page's life and 299 after it, and `remaining()` floors
    // those to `5 more minutes` and `4 more minutes` — two sentences either side of one
    // millisecond boundary this walk lands within a hair of. MEASURED, 2026-09-20, with the five
    // owning suites of the seven undeclared unstable shots run concurrently: `phone-pairing.png`
    // came back different, 775 of 1,330,000 pixels at a worst channel delta of 164, inside one
    // 120x13 box — that sentence and nothing else. 270 s is `4 more minutes` with thirty seconds
    // of margin to either edge of its minute; `phone.js` pins the same knob for its own two
    // shots and `settings-fit.js` pins it at 245 for the same reason. It pins the ANSWER and
    // does not move the clock, so every color this suite measures is the one it measured before.
    //
    // AND THE SIX WORDS ARE NO LONGER ON IT (Sage's pairing review 3.1 steps 1 and 4): they are
    // derived over the key a phone registers, so they appear on the card the phone reaches —
    // surface `phone-awaiting-mac` below — beside the two buttons that answer them. The driver
    // now waits on the code itself, which is this screen's last-painted thing.
    name: "phone-pairing",
    what: "the pairing screen: the code, the countdown, the four phone steps and where the six words will appear",
    preset: {phonePairing: true, phonePairingSecondsLeft: 270, phoneTailnet: {
      state: "ready",
      name: "mm1.tail9a3b2.ts.net",
      origin: "https://mm1.tail9a3b2.ts.net:8443",
      account: "Google as someone@gmail.com",
    }},
    drive: async (p) => {
      await p.click("#set-btn");
      await p.click("#set-phone-open");
      // The code is the last thing the pairing screen paints, so its presence is the signal that
      // the whole pairing state is up rather than a guess at a delay.
      await p.waitForFunction(() => {
        const code = document.getElementById("phone-pair-url");
        return code && /#pair=/.test(code.textContent);
      });
      await p.waitForSelector("#phone-pairing", {state:"visible"});
      await overlaySettled(p, "#phone-sheet");
    },
  },
  {
    // THE CARD A PHONE REACHES, WAITING FOR THE PRESS ON THIS MAC — Sage's pairing review F1. The
    // six words (derived over the key that phone registered) and the two buttons that answer them:
    // They match activates the phone, They do not match runs the rejection teardown. Its own
    // surface because it is the one screen where those three things are drawn.
    name: "phone-awaiting-mac",
    what: "a phone has reached this Mac: the six words and They match / They do not match, before the press",
    preset: {phonePaired: true, phoneMacUnconfirmed: true, phoneFingerprintUnconfirmed: true},
    drive: async (p) => {
      await p.click("#set-btn");
      await p.click("#set-phone-open");
      await p.waitForFunction(() => {
        const words = document.getElementById("phone-paired-words");
        return words && !words.hidden && words.textContent.trim().split(/\s+/).length === 6;
      });
      await p.waitForSelector("#phone-mac-answer", {state:"visible"});
      await overlaySettled(p, "#phone-sheet");
    },
  },
  {
    name: "phone-paired",
    what: "the same screen once a phone is paired: what it can do, and what forgetting it does",
    preset: {phonePaired: true},
    drive: async (p) => {
      await p.click("#set-btn");
      await p.click("#set-phone-open");
      await p.waitForFunction(() => {
        const name = document.getElementById("phone-device-name");
        return name && name.textContent.trim().length > 0;
      });
      await p.waitForSelector("#phone-paired", {state:"visible"});
      await overlaySettled(p, "#phone-sheet");
    },
  },
  {
    // THE TWO SCREENS CEO §61.1 ADDED, and they are their own surfaces for the same reason the
    // pairing and paired states are two: they share almost no words with either.
    //
    // THIS SURFACE USED TO BE TWO SCREENS IN ONE WALK — the route chooser and then the identity
    // warning behind it — and the chooser was the only place in the app where a control's BORDER
    // was the thing separating two decisions, which is why `.phone-route > button` named
    // `--line-control` explicitly. CEO §61 removed the chooser, so this surface is the identity
    // warning alone and it is now the sheet's FIRST screen. The border note is kept because the
    // token outlived the screen: Urban's G6 moved `.desk-btn`'s own border to `--line-control`
    // (3.79:1 dark, 4.06:1 light — measured from the rendered pixels in both themes), because on
    // the PAIRED card that border is the only thing making a button a button.
    name: "phone-identity",
    what: "the identity warning that precedes any Tailscale account, and is the sheet's first screen",
    preset: {phoneTailnet: {state: "absent"}},
    drive: async (p) => {
      await p.click("#set-btn");
      await p.click("#set-phone-open");
      await p.waitForSelector("#phone-identity:not([hidden])");
      await overlaySettled(p, "#phone-sheet");
    },
  },
  {
    // THE SCREEN THE ALARM BUTTON LEAVES BEHIND — Ray's nightly `.8` defect 1. It is its own
    // surface rather than a state of another because it shares no words with any of them: one
    // heading, one sentence and one control, on a Mac that has just stopped serving.
    name: "phone-rejected",
    what: "what the Mac says after the phone reported that the six words did not match",
    preset: {phoneRejected: true, phoneTailnet: {
      state: "ready",
      name: "mm1.tail9a3b2.ts.net",
      origin: "https://mm1.tail9a3b2.ts.net:8443",
      account: "Google as someone@gmail.com",
    }},
    drive: async (p) => {
      await p.click("#set-btn");
      await p.click("#set-phone-open");
      // The sentence is the last thing this screen paints, so its presence is the signal that
      // the state is up rather than a guess at a delay.
      await p.waitForFunction(() => {
        const note = document.getElementById("phone-rejected-note");
        return note && note.textContent.trim().length > 0;
      });
      await p.waitForSelector("#phone-rejected", {state:"visible"});
      await overlaySettled(p, "#phone-sheet");
    },
  },
  {
    name: "phone-tailscale-ready",
    what: "this Mac's tailnet name, the account it signed in with, and what the phone has not done yet",
    preset: {phoneTailnet: {
      state: "ready",
      name: "mm1.tail9a3b2.ts.net",
      origin: "https://mm1.tail9a3b2.ts.net:8443",
      account: "Google as someone@gmail.com",
      phone: "Android phone",
      phoneOnline: false,
    }},
    drive: async (p) => {
      await p.click("#set-btn");
      await p.click("#set-phone-open");
      // The tailnet name is the last thing this screen paints, so its presence is the signal that
      // the state is up rather than a guess at a delay.
      await p.waitForFunction(() => {
        const name = document.getElementById("phone-ts-name");
        return name && name.textContent.trim().length > 0;
      });
      await p.waitForSelector("#phone-ts-name", {state:"visible"});
      await overlaySettled(p, "#phone-sheet");
    },
  },
  ...[
    { name:"phone-connect-setup", preset:{phoneConnect:{enabled:false,phase:"not-configured"}}, target:"#phone-connect-start" },
    { name:"phone-connect-pairing", preset:{phoneConnect:{enabled:true,phase:"active",endpoint:"https://c-00000000000000000000000000000000-g1.richos.ceo",health:{state:"connected"}},phonePairing:true,phonePairingSecondsLeft:270}, target:"#phone-connect-pair-help" },
    { name:"phone-connect-recovery", preset:{phoneConnect:{enabled:true,phase:"active",health:{state:"reconnecting"}},phonePaired:true,phonePairedVia:"connect",phoneListenerStopped:true}, target:"#phone-paired-state" },
  ].map(({name,preset,target}) => ({
    name, preset, what:"managed Connect " + name.split("-").pop() + " with its own visible controls",
    drive: async (p) => {
      await p.click("#set-btn"); await p.click("#set-phone-open");
      await p.waitForSelector("#phone-connect", {state:"visible"});
      await p.waitForSelector(target, {state:"visible"});
      await overlaySettled(p,"#phone-sheet");
    },
  })),
  {
    name: "quit-question",
    what: "the question he is asked before a quit stops running background work",
    // **Raised through the sheet's own entry point, not a copy of it.** The shell emits
    // `rich://quit-question` from its exit arm, which needs a real Tauri event and a real
    // menu press; `RichQuitQuestion.show` is the same function that listener calls, with the
    // same payload shape, so what is measured here is the sheet the app renders.
    drive: async (p) => {
      await p.evaluate(() =>
        window.RichQuitQuestion.show({
          say:
            "You have 1 assignment still running in the background. Quitting stops the work. " +
            "Everything it has done so far is kept, and nothing is landed in your repository.",
          quit: "Quit and stop the work",
          stay: "Keep working",
        })
      );
      await overlaySettled(p, "#quit-question");
    },
  },
  ...[false, true].map(showPanel => ({
    name: showPanel ? "saved-work" : "saved-work-chip",
    what: showPanel ? "saved work receipts in the work summary" : "the visible saved-work disclosure control",
    preset: {workSummaries: {hiring: {items: [{title: "Reviewed change", role: "worker", repository: "/fictional/project", state: "integrated", detail: "Reviewed commit integrated locally. Whole assignment remains open."}], omitted: 2}}},
    drive: async (p) => {
      await p.click('.nav-thread[data-thread-id="hiring"]');
      await atHiringThread(p);
      await p.waitForFunction(() => document.getElementById("drill-chip-zone").textContent.includes("1 team job"));
      if (showPanel) {
        await p.click(".drill-chip");
        await p.waitForFunction(() => document.getElementById("slideover-body").textContent.includes("Whole assignment remains open."));
        await overlaySettled(p, "#slideover");
      } else {
        await pageSettled(p);
      }
    },
  })),
  {
    // THE ONE STRING THE STYLESHEET ITSELF PUTS ON SCREEN, and until 2026-09-05 nothing in
    // this directory could see it. `style.css:3787` is `.setbtn::after { content: "Settings" }`
    // — the tooltip on the settings button §15 requires on every screen, including the
    // opening screen where it is the only control there is. The walk collects
    // `nodeType === 3` children and a pseudo-element is not a node, so it was rendered text
    // that no contrast check had ever measured; and it is `opacity: 0` until hover, so even
    // the widened walk reports it HIDDEN on every other surface. This driver is what turns
    // "named as hidden" into "measured".
    name: "settings-tooltip",
    what: "the settings button's tooltip — the one string the stylesheet itself renders",
    drive: async (p) => {
      await p.hover(".setbtn");
      // The tooltip's own opacity, read off the pseudo-element. See `pseudoOpaque` for why the
      // frame math that used to stand here was still a bet.
      await pseudoOpaque(p, ".setbtn", "::after");
    },
  },
  {
    name: "thread",
    what: "a seeded conversation with a settled turn and a delegation summary",
    drive: async (p) => {
      // A `.tl-turn` EXISTING is not this thread's turn existing — the previous thread's turns
      // are in the DOM until `loadTimeline` replaces them, so that selector was already
      // satisfied before the click's own work had started. `atHiringThread` is the same three
      // facts the assignment drivers wait for, and the reason is written above it.
      await p.click('.nav-thread[data-thread-id="hiring"]');
      await atHiringThread(p);
      await pageSettled(p);
      await assertStillHiring(p, "thread");
    },
  },
  {
    name: "pending-user-bubble",
    what: "the CEO's own message while it waits — audit-7 row 11, measured at 3.09:1 on his screen",
    drive: async (p) => {
      // WHY THIS IS ITS OWN SURFACE. Every other walk measures a SETTLED thread, and the
      // settled user bubble has always been fine (5.50:1 on Ray's frame, 5.55:1 computed).
      // The failure lived in a state that exists for a second or two and that no driver had
      // ever paused on: `.tl-user.is-pending`, which shipped `opacity: 0.72` over the whole
      // bubble and took the ink down with the fill. Ray caught it because he was looking at
      // the screen at the moment he pressed Return.
      await p.click('.nav-thread[data-thread-id="hiring"]');
      await atHiringThread(p);
      await pageSettled(p);
      // THE SEND IS STALLED, NOT FAKED. `send()` puts the optimistic bubble up synchronously
      // (`main.js`'s `addPendingUserMessage`) and then awaits `send_message`; a promise that
      // never settles holds the surface at exactly the moment being measured, through the
      // shipping path, with the shipping markup and the shipping class.
      await p.evaluate(() => {
        const real = window.RichBridge.invoke.bind(window.RichBridge);
        window.RichBridge.invoke = (name, args) =>
          name === "send_message" ? new Promise(() => {}) : real(name, args);
      });
      await p.fill("#input", "Please add a line to notes.txt and land it.");
      await p.press("#input", "Enter");
      await p.waitForSelector(".tl-user.is-pending .tl-user-text", { state: "visible", timeout: 8000 });
      await pageSettled(p);
      // And it is still pending when the walk starts — a bubble that settled mid-drive would
      // make this surface a second copy of `thread` and report a pass for the wrong state.
      const pending = await p.$$eval(".tl-user.is-pending .tl-user-text", (n) => n.length);
      if (pending < 1) throw new Error("the pending bubble settled before the walk — this surface measured nothing");
    },
  },
  {
    // SCREENSHOTS AND FILES ON THE COMPOSER (CEO §86, `ui/attachments.js`). Everything the
    // tray can put on screen at once: a pasted-style chip, a file chip, the refusal line for a
    // file that was not taken, and the drop target a drag over the window raises. The drop goes
    // through the mock's `dropFiles`, which emits the same `rich://file-drag` / `rich://file-drop`
    // pair the shell emits, so the chips are drawn by the shipping renderer from the shipping
    // commands' answers.
    name: "composer-attachments",
    what: "files on the composer: two chips, the refusal line and the drop target (CEO §86)",
    drive: async (p) => {
      await p.click('.nav-thread[data-thread-id="hiring"]');
      await atHiringThread(p);
      await pageSettled(p);
      await p.evaluate(() => {
        const png = [0x89, 0x50, 0x4e, 0x47, 0x0d, 0x0a, 0x1a, 0x0a, 0, 0, 0, 13];
        const pdf = [0x25, 0x50, 0x44, 0x46, 0x2d, 0x31, 0x2e, 0x37, 0x0a];
        window.__RICHOS_MOCK__.dropFiles([
          { name: "Screenshot invoice 4471.png", bytes: png },
          { name: "board-memo-lisbon-lease.pdf", bytes: pdf },
          { name: "archive.zip", bytes: [0x50, 0x4b, 3, 4] },
        ]);
      });
      await p.waitForFunction(
        () =>
          document.querySelectorAll("#attach-list .attach-chip:not(.is-adding)").length === 2 &&
          document.getElementById("attach-note").textContent.includes("archive.zip")
      );
      await p.evaluate(() => window.__RICHOS_MOCK__.dragOver(true));
      await p.waitForSelector("#attach-drop:not([hidden])");
      await pageSettled(p);
    },
  },
  {
    name: "corrections",
    what: "the correction desk, both families, with two asks waiting",
    drive: async (p) => {
      await p.click("#nav-corrections");
      await p.waitForSelector("#corrections-overlay:not([hidden])");
      // UN-HIDING THE OVERLAY IS NOT THE DESK BEING READY, the same distinction the `feedback`
      // driver below already carries. `openCorrections` renders what it already had, RE-READS
      // over the bridge, renders again, and only then moves focus into the panel — so focus
      // inside the overlay is that function's own last statement and the one signal that
      // cannot be true before the re-read has landed.
      await p.waitForFunction(() => {
        const o = document.getElementById("corrections-overlay");
        return !!o && !o.hidden && o.contains(document.activeElement);
      });
      await overlaySettled(p, "#corrections-overlay");
    },
  },
  {
    name: "feedback",
    what: "the feedback desk as it first opens",
    drive: async (p) => {
      await p.click("#nav-feedback");
      await p.waitForSelector("#feedback-overlay:not([hidden])");
      // THE OVERLAY IS NOT THE DESK. Un-hiding it is synchronous; the session question and
      // its four rating buttons are filled by a read that lands afterwards, and 400 ms was
      // enough for that read on this machine and not on a slower one. Measured at `934f127`
      // with 120 ms on each bridge call: 6 text nodes at the end of the old wait against 11
      // settled, and the walk reported "feedback measured only 8 node(s) ... but its floor is
      // 20". So the wait is for the question the desk exists to ask.
      await p.waitForFunction(() => {
        const q = document.getElementById("feedback-question");
        return !!q && q.textContent.trim() !== "";
      });
      await overlaySettled(p, "#feedback-overlay");
    },
  },
  {
    name: "search",
    what: "search with results, including the selected row",
    drive: async (p) => {
      await p.keyboard.press("Meta+k");
      await p.waitForSelector("#search-overlay:not([hidden])");
      await p.fill("#search-input", "a");
      // A 120 MS DEBOUNCE AND A BRIDGE ROUND TRIP, neither of which a sleep can see the end of.
      // This waits for the surface's own title instead: results, AND the selected row — the
      // `.is-active` one, which is where `--accent` lands and the whole reason this walk names
      // it. `aria-expanded` is `renderSearchResults`'s own answer to "are there hits".
      await p.waitForFunction(() => {
        const input = document.getElementById("search-input");
        if (!input || input.getAttribute("aria-expanded") !== "true") return false;
        return document.querySelectorAll("#search-results .result-item.is-active").length === 1;
      });
      await overlaySettled(p, "#search-overlay");
    },
  },
  {
    name: "inspector",
    what: "the worker inspector open over an expanded transcript",
    drive: async (p) => {
      // SAME AS `thread`: `.tl-duration-btn` belongs to A turn, not to THIS thread's turn, so
      // the three clicks below were being aimed at whatever was on the stage. Everything this
      // walk measures hangs off that first one landing on the right conversation.
      await p.click('.nav-thread[data-thread-id="hiring"]');
      await atHiringThread(p);
      await p.waitForSelector(".tl-duration-btn");
      await p.click(".tl-duration-btn");
      await p.waitForSelector(".tl-chip");
      await p.click(".tl-chip");
      await p.waitForSelector("#inspector:not([hidden])");
      await overlaySettled(p, "#inspector");
      await assertStillHiring(p, "inspector");
    },
  },
  // THE OUTPUT PANEL (output side-panel PRD §6, `output-panel.js`), on `mock.js`'s round-17
  // preset: the acme thread's nine files across four turns, Rich's replies linking them, one
  // file by a worker. Three states with different ink: the list (with one file that is no longer
  // where it was written, so the dimmed row is measured too), one file's own view, and a thread
  // that produced nothing. `tests/output.js` computes the same pairs by name in both themes.
  {
    name: "output-open",
    what: "the Output panel's list: four turns, nine files, a worker's file and a missing one",
    preset: { output: "round-17" },
    drive: async (p) => {
      await p.evaluate(() => window.__RICHOS_MOCK__.outputMissing("acme", "brief.md"));
      await p.click('.nav-thread[data-thread-id="acme"]');
      await p.waitForFunction(() => window.RichOutput.snapshot().thread === "acme" && window.RichOutput.snapshot().count === 9);
      await p.click("#out-top");
      await p.waitForSelector("#outpanel .orow.is-missing");
      await overlaySettled(p, "#outpanel");
      await pageSettled(p);
    },
  },
  {
    name: "output-file",
    what: "one file's own view in the Output panel: the way back, k of N, Preview | Source, its path, a Markdown preview (S5: a heading, a table, a list, a quotation) and its facts",
    preset: { output: "round-17" },
    drive: async (p) => {
      await p.click('.nav-thread[data-thread-id="acme"]');
      await p.waitForFunction(() => window.RichOutput.snapshot().thread === "acme" && window.RichOutput.snapshot().count === 9);
      await p.click("#out-top");
      await p.waitForSelector("#outpanel .orow");
      // comps-summary.md: the preview with the most kinds of ink (S5, §7). By its name: since S6
      // each row sits in its own `.orow-wrap` beside its actions, so `.orow:last-of-type` matched
      // every row and the click landed on the group's first file, a spreadsheet.
      await p.click('#outpanel .orow:has(.oname:text-is("comps-summary.md"))');
      await p.waitForSelector("#of-back");
      await p.waitForSelector("#op-viewer[data-preview] .of-md");
      await overlaySettled(p, "#outpanel");
      await pageSettled(p);
    },
  },
  {
    name: "output-empty",
    what: "the Output panel on a thread that produced nothing: the one sentence",
    preset: { output: "round-17" },
    drive: async (p) => {
      await p.click('.nav-thread[data-thread-id="hiring"]');
      await atHiringThread(p);
      await p.waitForFunction(() => window.RichOutput.snapshot().thread === "hiring" && window.RichOutput.snapshot().count === 0);
      await p.click("#out-top");
      await p.waitForSelector("#outpanel .op-empty");
      await overlaySettled(p, "#outpanel");
      await pageSettled(p);
    },
  },
  {
    name: "technical-view",
    what: "the technical view pinned on for one conversation (§3.3)",
    drive: async (p) => {
      await p.click('.nav-thread[data-thread-id="hiring"]');
      await atHiringThread(p);
      await p.keyboard.press("Meta+Shift+T");
      // ⌘⇧T ASKS WHERE IT APPLIES since 2026-09-18 (CEO §9.1: the choice has no exception
      // in it, not even for a shortcut), so this is two acts, not one: press the key, then
      // take "For this conversation only" — which leaves exactly the state this surface was
      // walked in before the sheet existed.
      await techyScopeThisConversation(p);
      // WHAT THE PRODUCT PUBLISHES WHEN THE MODE GOES ON, and it is deliberately not
      // `#techy-state`: `renderTechyState` only fills that element when the machinery read
      // carries a `sentence`, and on `hiring` it does not — measured 2026-09-06, the element
      // stays hidden with no `data-state` for as long as you care to watch, which is correct
      // and is why waiting for it timed out. `#techy-chip` is `hidden = !on` and `#between-turns`
      // is the techy-only lane, and both flip inside the SAME `loadTimeline` pass that repaints
      // the conversation — so the pair is the mode being on with its reload finished.
      await p.waitForFunction(() => {
        const chip = document.getElementById("techy-chip");
        const lane = document.getElementById("between-turns");
        return !!chip && !chip.hidden && !!lane && !lane.hidden;
      });
      await pageSettled(p);
      await assertStillHiring(p, "technical-view");
    },
  },
  {
    name: "unbound",
    what: "the refusal pane for a thread that predates entity scoping (§21)",
    drive: async (p) => {
      // §21'S REFUSAL IS AN END STATE, and `settleOnThread` knows about it: an unbound thread
      // never reaches `loadTimeline`, so waiting for turns would wait out a timeout over a
      // screen that is completely correct. What is waited for instead is the pane being on
      // screen with the refusal's own three lines filled — 600 ms was a guess at how long the
      // bridge takes to refuse.
      await p.click('.nav-thread[data-thread-id="legacy"]');
      const branch = await settleOnThread(p, "legacy");
      assertEqual(branch, "unbound", "the legacy thread is the one that has no entity home");
      await p.waitForFunction(() => {
        const t = document.getElementById("unbound-view-title");
        const b = document.getElementById("unbound-view-body");
        const d = document.getElementById("unbound-view-detail");
        return [t, b, d].every((n) => n && n.textContent.trim() !== "");
      });
      await pageSettled(p);
    },
  },
  {
    name: "settings",
    what: "the one Settings panel (round 27): its three columns and the band across its foot, which hold what the gear's Preferences held",
    drive: async (p) => {
      await p.click("#set-btn");
      await p.waitForSelector("#set-menu:not([hidden])");
      await overlaySettled(p, "#set-menu");
    },
  },
  // NO SEPARATE SURFACE FOR THE COMPANY RADIOS, and that is a measured decision rather than
  // an omission. One was written — it scrolled the group into view on the assumption that the
  // popover's `max-height: calc(100vh - 70px); overflow-y: auto` had pushed it out — and it
  // measured 66 nodes across both themes, the identical set the `settings` walk above already
  // measures. At 1400x950 the popover is 821px tall against an 819px scroll height: it does
  // not overflow, so the company rows are in view on `settings` and a second walk of the same
  // pixels would be a check that can only ever agree with its neighbour.
  // THE LAUNCH THE CEO PERFORMS — no company chosen, so the picker is open over a composer
  // that says why it will not send and carries the control that clears it. Three of this
  // pass's new ink/edge pairs live only here: the picker's note line, the blocked sentence,
  // and the border of the button beneath it.
  {
    name: "company-picker",
    what: "the launch-time company picker, its note, and the blocked composer behind it",
    drive: async (p) => {
      await p.waitForSelector("#entity-picker:not([hidden])");
      await overlaySettled(p, "#entity-picker");
    },
    preset: { chosenEntity: null },
  },
  // THE SAME LAUNCH WITH THE DIALOG DISMISSED. It is a separate surface and not a nicety:
  // with the picker up, the composer's blocked sentence and the button beneath it are
  // behind an `aria-modal` dialog and are filed `obscured` rather than measured — so
  // without this walk, check 11 correctly reports `button#choose-company-btn` as a node
  // that is obscured everywhere and measured nowhere. This is where it is measured.
  {
    name: "company-blocked",
    what: "the blocked composer and its Choose the company button, picker dismissed",
    drive: async (p) => {
      await p.waitForSelector("#entity-picker:not([hidden])");
      await p.keyboard.press("Escape");
      await p.waitForSelector("#entity-picker", { state: "hidden" });
      // The composer BEHIND the dismissed dialog is what this walk measures, so the wait is on
      // that: its blocked sentence and the control rendered under it, both filled.
      await p.waitForFunction(() => {
        const line = document.getElementById("composer-blocked");
        const action = document.getElementById("composer-choose-company");
        return !!line && !line.hidden && line.textContent.trim() !== "" && !!action && !action.hidden;
      });
      await pageSettled(p);
    },
    preset: { chosenEntity: null },
  },
  // THE FIRST-RUN MEMORY DIALOG, IN BOTH OF ITS STATES, because they paint different ink:
  // the ask carries the confirm button on the accent and, since the product hunt of
  // 2026-10-15 (finding 15), no location line; the no-compiler state carries the location
  // line (`--ink` monospace, computed 11.81:1 dark and 18.27:1 light before it was written)
  // and only the plain button. Neither is
  // reachable from any surface above — a machine that has been set up once renders neither
  // — so without these two walks the whole dialog would be uncovered.
  {
    name: "memory-setup",
    what: "the first-run memory question: its note and the two buttons",
    drive: async (p) => {
      await p.waitForSelector("#memory-setup:not([hidden])");
      await overlaySettled(p, "#memory-setup");
    },
    preset: { memory: "none" },
  },
  {
    name: "memory-no-compiler",
    what: "a corpus this install cannot read, said when he opens the Memory folder row",
    // REACHED FROM THE SETTINGS MENU, not by booting into it. This state stopped opening its
    // own dialog at launch on 2026-09-04 (ray-opus-a1, finding 4), and since the product hunt
    // of 2026-10-15 (finding 15) the press that sets the folder up closes the sheet whatever
    // the answer, so the "Memory folder" row is where a person meets this sentence.
    drive: async (p) => {
      await p.click("#set-btn");
      await p.waitForFunction(() => !document.getElementById("set-menu").hidden);
      await p.click("#set-memory-open");
      await p.waitForSelector("#memory-setup-close:not([hidden])");
      await overlaySettled(p, "#memory-setup");
    },
    preset: { memory: "no-compiler" },
  },
  // FIRST-RUN SETUP, IN TWO STATES, and neither is reachable from any surface above: a
  // machine that has Claude Code and an engine renders neither. §19 says every machine but
  // the CEO's is in the first of them, so an uncovered sheet here would be an uncovered
  // sheet on every customer's very first screen.
  //
  // Between them they paint every color the sheet can: the item rows and the account
  // caveat (missing-both), and `--danger` on the sentence that explains why no button is
  // drawn (unpinned).
  {
    name: "setup-sheet",
    what: "the first-run consent step: what is missing, why, and the account he still needs",
    drive: async (p) => {
      await p.waitForSelector("#setup-sheet:not([hidden])");
      await overlaySettled(p, "#setup-sheet");
    },
    preset: { setup: "missing-both" },
  },
  // THE ACCOUNT-TYPE SELECT, WALKED FOR THE FIRST TIME (audit D7, 2026-09-17). No surface
  // above ever reaches `#provider-account-select` visible: `setup-sheet`'s own preset
  // (`missing-both`) never gets far enough into the flow for `renderProviderAuth` to un-hide
  // `#provider-account-kind`, which only happens once the provider state is neither
  // `connected` nor `unavailable` nor `connecting` — `signed-out` is the mock's name for
  // that. This is also the walk that would have caught the control shipping with ZERO CSS
  // of its own (`grep -n provider-account-select style.css` returned nothing before this
  // change): the walker reads `getComputedStyle()`, which reports the browser's native
  // white-on-black-arrow defaults as "unset" `background-color`/`color` rather than as a
  // failure, so an entirely-unstyled native `<select>` was invisible to this suite even
  // though it was the audit's D7 finding. Recorded as a gap in `contrast-debt.json` rather
  // than silently fixed: this walk proves the text and the new chevron pass NOW that
  // `.chrome-select` gives the control real, CSS-declared paint — it does not retroactively
  // prove anything about what the browser painted before `appearance: none` existed.
  {
    name: "setup-account-connect",
    what: "first-run setup reopened to connect an Anthropic account: the account-type select, its text and its custom chevron",
    drive: async (p) => {
      // BY WHICHEVER DOOR IS OPEN, and on this preset the sheet is already through it.
      // `init()` -> `maybeAskAboutSetup()` finds nothing to install and an account that is
      // not connected, so it calls `openAccountConnection` — the SAME function the settings
      // menu's Account row calls, painting the same surface. This driver used to reach it
      // through that menu unconditionally, and since a question started holding the window
      // (`main.js` `syncQuestionInert`) the top-right control is inert while a sheet is up,
      // so the menu is not a door on this preset. The walked surface is unchanged.
      if (await p.isHidden("#setup-sheet")) {
        // Since round 18 the door is the first account's card in Claude accounts.
        await p.click("#set-btn");
        await p.click("#set-accounts-open");
        await p.click('#accounts-sheet [data-act="sign-one"]');
      }
      await p.waitForSelector("#setup-sheet:not([hidden])");
      await p.waitForSelector("#provider-connect:not([hidden])");
      await overlaySettled(p, "#setup-sheet");
    },
    preset: { providerAuth: "signed-out" },
  },
  {
    name: "setup-blocked",
    what: "a build that cannot install an engine, and the sentence naming who can",
    drive: async (p) => {
      await p.waitForSelector("#setup-sheet:not([hidden])");
      await p.waitForSelector("#setup-error:not([hidden])");
      await overlaySettled(p, "#setup-sheet");
    },
    preset: { setup: "unpinned" },
  },
  // ROUND 19'S SETUP SHEET AND RICH'S OFFER (dictation plan slice 4), drawn only once dictation
  // is there (`ask.dictation`), so no surface above reaches them. Four states, because each
  // paints what the others do not: state 1 the gold bullets and the serif title; state 2 the
  // gold box with Rich's line, the step marks (done, now, waiting), the counter and the bar on
  // its track; state 3 "Voice is ready." and Start; state 4 the offer in the conversation with
  // its key cap and both buttons.
  {
    name: "setup-r19-ask",
    what: "round 19, state 1: the setup sheet with his sentence for the voice and video tools",
    drive: async (p) => {
      await p.waitForSelector("#setup-sheet:not([hidden])");
      await overlaySettled(p, "#setup-sheet");
    },
    preset: { setup: "missing-both", dictation: true, videoTools: "downloading", memory: "ready" },
  },
  {
    name: "setup-r19-progress",
    what: "round 19, state 2: Rich's download line, the rows Installed / Installing / Waiting, the counter and its bar",
    drive: async (p) => {
      await p.waitForSelector("#setup-sheet:not([hidden])");
      await p.evaluate(() => window.__RICHOS_MOCK__.setupEmit({
        state: "started", component: "media-tools", what: "Getting my voice and video tools. 0%", index: 1, total: 1,
        received: 489999396, of: 1061655396, counter: "490 MB of 1.06 GB", both_models: true,
      }));
      // The mock's press finishes in a few frames, so once it has, Claude Code is shown done and
      // the engine installing: all three marks (done, now, waiting ring on none) and the
      // counting row are painted at once.
      await p.click("#setup-go");
      await p.waitForSelector("#setup-start:not([hidden])");
      await p.evaluate(() => {
        const rows = document.querySelectorAll("#setup-steps .setup-step");
        rows[0].className = "setup-step is-done";
        rows[1].className = "setup-step is-now";
      });
      // NOT `overlaySettled`: the installing row's spinner turns for as long as the step runs
      // (an infinite animation), and waiting for every animation to finish would wait forever
      // (measured: this walk hung on it). The finite ones, the sheet's own entrance, are waited
      // out; the spinner is measured where it is, by its arc's color, which does not change.
      await p.waitForSelector("#setup-sheet", { state: "visible" });
      await p.evaluate(async () => {
        const el = document.getElementById("setup-sheet");
        const finite = el.getAnimations({ subtree: true }).filter((a) => a.effect && a.effect.getComputedTiming().iterations !== Infinity);
        await Promise.all(finite.map((a) => a.finished.catch(() => {})));
        await new Promise((r) => requestAnimationFrame(() => requestAnimationFrame(r)));
      });
    },
    preset: { setup: "missing-both", dictation: true, videoTools: "downloading", memory: "ready" },
  },
  {
    name: "setup-r19-done",
    what: "round 19, state 3: \"You're all set.\", Rich's \"Voice is ready.\" and Start",
    drive: async (p) => {
      await p.waitForSelector("#setup-sheet:not([hidden])");
      await p.click("#setup-go");
      await p.waitForSelector("#setup-start:not([hidden])");
      await p.waitForSelector("#setup-rich-line:not([hidden])");
      await overlaySettled(p, "#setup-sheet");
    },
    preset: { setup: "missing-both", dictation: true, memory: "ready" },
  },
  {
    name: "dictation-offer",
    what: "round 19, state 4: Rich's one offer of dictation in the conversation, its key and both buttons",
    drive: async (p) => {
      await p.waitForSelector("#setup-sheet:not([hidden])");
      await p.click("#setup-go");
      await p.waitForSelector("#setup-start:not([hidden])");
      await p.click("#setup-start");
      await p.waitForSelector("#dictation-offer-yes");
      await pageSettled(p);
    },
    preset: { setup: "missing-both", dictation: true, memory: "ready" },
  },
  // THE UPDATE ROW, IN THREE STATES, because one state cannot paint the colours the others
  // do (RICH-TODOs row 12, `app/ui/updates.js`). It lives in the UNIVERSAL settings menu —
  // which the `settings` surface above does NOT reach, since that one drives the rail's
  // preferences popover — so without these three drivers the whole surface would be
  // uncovered while a nearby surface's name suggested otherwise.
  //
  // Between them they paint every colour the row can: `--attention` and the primary button
  // and the mark on the settings button (available), the progress fill and its track border
  // (downloading), `--danger` and the disclosure and the verbatim vendor detail (failed).
  {
    name: "updates-available",
    what: "the update row with a version waiting, and the mark on the settings button",
    drive: async (p) => {
      await p.evaluate(() =>
        window.__RICHOS_MOCK__.updateSet({
          state: "available", currentVersion: "0.1.0", availableVersion: "0.1.1",
          notes: "Faster launch, and the technical view remembers its width.",
          pubDate: "2026-08-31T12:00:00Z", downloadedBytes: 0, totalBytes: null, percent: null,
          failure: null, endpoint: "https://updates.example.com/darwin/aarch64/0.1.0",
          endpointIsPlaceholder: false, checkedAt: Date.now() - 120000,
        })
      );
      await p.click("#set-btn");
      await updatesRowShowing(p, "available");
    },
  },
  {
    name: "updates-downloading",
    what: "the update row mid-download: the progress bar, its track and its border",
    drive: async (p) => {
      await p.evaluate(() =>
        window.__RICHOS_MOCK__.updateSet({
          state: "downloading", currentVersion: "0.1.0", availableVersion: "0.1.1",
          notes: null, pubDate: null, downloadedBytes: 5242880, totalBytes: 13631488, percent: 38,
          failure: null, endpoint: "https://updates.example.com/darwin/aarch64/0.1.0",
          endpointIsPlaceholder: false, checkedAt: Date.now() - 120000,
        })
      );
      await p.click("#set-btn");
      await updatesRowShowing(p, "downloading");
    },
  },
  {
    name: "updates-failed",
    what: "a REFUSED SIGNATURE, with the vendor's own reason disclosed",
    drive: async (p) => {
      await p.evaluate(() =>
        window.__RICHOS_MOCK__.updateSet({
          state: "failed", currentVersion: "0.1.0", availableVersion: "0.1.1",
          notes: null, pubDate: null, downloadedBytes: 13631488, totalBytes: 13631488, percent: 100,
          failure: {
            kind: "signature",
            headline: "This download was not signed by RichOS, so it was not installed.",
            detail: "Signature verification failed",
          },
          endpoint: "https://updates.example.com/darwin/aarch64/0.1.0",
          endpointIsPlaceholder: false, checkedAt: Date.now() - 60000,
        })
      );
      await p.click("#set-btn");
      await updatesRowShowing(p, "failed");
      await p.click("#update-why");
      // The vendor's own reason, disclosed — which is the ink this walk exists to measure, so
      // it is waited for rather than assumed to be one sleep behind the click.
      await p.waitForFunction(() => {
        const d = document.getElementById("update-detail");
        return !!d && !d.hidden && d.textContent.trim() !== "";
      });
      await pageSettled(p);
    },
  },
  // THE WAITING CUE, which is the one place on this surface where a NON-TEXT indicator has to
  // carry the meaning on its own: while RichOS is working the actionable pill is removed and
  // this outlined circle stands in its place. Its glyph, its border and its focus ring each
  // owe 3:1, and the note it opens owes 4.5:1 at 16px. None of that is walked by the three
  // surfaces above, because none of them can paint an element that only exists while `busy`.
  //
  // THE NOTE IS OPENED BY FOCUS rather than by hover, deliberately: `page.focus` is the route
  // a keyboard user takes, so the pixels walked here are the pixels that audience meets. A
  // walk over a note that was never painted would report a clean surface and prove nothing.
  {
    name: "updates-waiting",
    what: "the non-actionable waiting cue and its open explanation, with the update control gone",
    drive: async (p) => {
      await p.evaluate(() =>
        window.__RICHOS_MOCK__.updateSet({
          state: "available", currentVersion: "0.1.0", availableVersion: "0.1.1",
          notes: null, pubDate: null, downloadedBytes: 0, totalBytes: null, percent: null,
          failure: null, endpoint: "https://updates.example.com/darwin/aarch64/0.1.0",
          endpointIsPlaceholder: false, checkedAt: Date.now() - 120000,
          busy: true,
          busyReason: "Rich is working on your last message. 2 workers are still running.",
          unchecked: [],
          readySince: Date.now() - 2 * 86400000,
        })
      );
      await p.waitForSelector("#update-waiting");
      await p.focus("#update-waiting");
      await p.waitForSelector("#update-waiting-note", { state: "visible" });
      // The note owes 4.5:1 at 16px and the glyph, border and focus ring each owe 3:1, so this
      // walk is worthless sampled mid-fade. `visible` is a box; this is the end of the motion.
      await pageSettled(p);
    },
  },
  // ---- THE THREE CHECK 10c FOUND ON THE DAY IT WAS WRITTEN --------------------------------
  //
  // The panel probe named ten panels in the shipped shell that no driver opened. Seven are
  // honest gaps and are declared as such in `contrast-debt.json`'s `unwalkedPanels`. These
  // three were not gaps — they were surfaces with real text on them, reachable in a browser
  // with nothing stubbed, that nobody had written a driver for. Every one of them is text
  // the CEO reads, and until this commit this file said nothing about any of it.
  /// GETTING THE SPEECH MODEL — the three rows a machine with a decoder and no weights shows.
  ///
  /// NO MICROPHONE IS OPENED BY ANY OF THEM, which is why they are walkable at all where the
  /// listening row is not: `enterVoiceMode` calls `start_voice_capture` only when readiness says
  /// `ready`, and in `model-missing` it opens this panel and touches no device.
  ///
  /// THREE RATHER THAN ONE, because they do not share their colors. The offer and the refusal use
  /// `--ink` with an `--attention` dot; the transfer uses `--ink-soft` plus a progress bar whose
  /// fill and track are two more tokens, and a bar is a NON-TEXT INDICATOR owing 3:1 that no other
  /// walk in this suite measures.
  {
    name: "voice-model-offer",
    what: "the speech-model offer: what will be downloaded, how big, and the control that starts it",
    preset: { voice: "model-missing" },
    drive: async (p) => {
      await p.waitForSelector("#talk-toggle:not([hidden])");
      await p.click("#talk-toggle");
      await p.waitForSelector("#voice-state-model-offer:not([hidden])");
      // An EMPTY label means the row on screen is not the one this walk is named for — the size
      // is composed from the offer payload, so its absence means the payload never arrived.
      await p.waitForFunction(() => {
        const e = document.getElementById("voice-model-offer-label");
        return !!e && e.textContent.indexOf("MB") >= 0;
      });
    },
  },
  {
    name: "voice-model-progress",
    what: "the transfer: the progress line, and the bar's fill and track as non-text indicators",
    preset: { voice: "model-missing" },
    drive: async (p) => {
      await p.waitForSelector("#talk-toggle:not([hidden])");
      await p.click("#talk-toggle");
      await p.waitForSelector("#voice-state-model-offer:not([hidden])");
      await p.click("#voice-model-get");
      await p.waitForSelector("#voice-state-model-progress:not([hidden])");
      // A MEASURABLE BAR, not an empty track. At 0% the fill has zero width and there is nothing
      // to measure, so the walk would report a pass over a shape it never saw. The figures are
      // `small.en`'s real ones from engine/voice/models/model-pins.json.
      await p.evaluate(() =>
        window.__RICHOS_MOCK__.voiceModelEmit({
          phase: "progress", modelId: "small.en", received: 243807100, total: 487614201,
          totalLabel: "487.6 MB", message: null, askAgain: false, at: Date.now(),
        })
      );
      await p.waitForFunction(() =>
        document.getElementById("voice-model-bar").style.width === "50%"
      );
    },
  },
  {
    name: "voice-model-failed",
    what: "a refusal he can act on, with the control that acts on it",
    preset: { voice: "model-missing" },
    drive: async (p) => {
      await p.waitForSelector("#talk-toggle:not([hidden])");
      await p.click("#talk-toggle");
      await p.waitForSelector("#voice-state-model-offer:not([hidden])");
      await p.click("#voice-model-get");
      await p.waitForSelector("#voice-state-model-progress:not([hidden])");
      // The captive portal, verbatim from `Finding::ceo_message` — the sentence a real first run
      // on hotel wifi produces, and the one whose "then ask me again" needs the retry control
      // beside it.
      await p.evaluate(() =>
        window.__RICHOS_MOCK__.voiceModelEmit({
          phase: "failed", modelId: "small.en", received: 3104, total: 487614201,
          totalLabel: "487.6 MB", askAgain: true, at: Date.now(),
          message: "The network sent me a sign-in page instead of my speech model — that's what " +
            "hotel, airport and conference wifi does. Sign in to the network, then ask me again. " +
            "Nothing was installed.",
        })
      );
      await p.waitForSelector("#voice-state-model-failed:not([hidden])");
      await p.waitForSelector("#voice-model-retry:not([hidden])");
    },
  },
  {
    name: "entity-view",
    what: "§3.5's company overview: its facts, its attention list and its thread list",
    drive: async (p) => {
      await p.click(".nav-group-label");
      await p.waitForSelector("#entity-view:not([hidden])");
      // `showEntityView` fills the name and the lede synchronously, so an EMPTY name means the
      // pane on screen is not the one this walk is named for — the shell's leftovers, or a pane
      // mid-swap. Its facts and thread list are what the surface promises to measure.
      await p.waitForFunction(() => {
        const name = document.getElementById("entity-view-name");
        const line = document.getElementById("entity-view-line");
        return !!name && name.textContent.trim() !== "" && !!line && line.textContent.trim() !== "";
      });
      await overlaySettled(p, "#entity-view");
    },
  },
  // ROUND 32 (CEO 2026-10-19): "+ New chat or company", its menu, the company form inside it
  // (with the app's refusal showing), a new chat's row and screen, and the plus's "+ New chat"
  // tooltip, which keyboard focus holds up so the parked pointer cannot take it down.
  {
    name: "new-chat-menu",
    what: "\"+ New chat or company\" open: New chat in each company, then New company",
    drive: async (p) => {
      await p.click("#rail-new-thread");
      await p.waitForSelector("#new-chat-menu:not([hidden]) .m-item");
      await overlaySettled(p, "#new-chat-menu");
    },
  },
  {
    name: "new-chat-company-form",
    what: "New company: the company form inside the same menu, its refusal showing",
    drive: async (p) => {
      await p.click("#rail-new-thread");
      await p.click("#new-chat-menu .m-item[data-new-company]");
      await p.click("#entity-add-go");
      await p.waitForSelector("#new-chat-menu #entity-add-error:not([hidden])");
      await overlaySettled(p, "#new-chat-menu");
    },
  },
  {
    name: "new-chat",
    what: "a new chat in a company: its New chat row with the gold dot, and its screen",
    drive: async (p) => {
      await p.click("#rail-new-thread");
      await p.click("#new-chat-menu .m-item[data-company]");
      await p.waitForSelector(".nav-thread-row.is-draft");
      await p.waitForSelector("#entity-view:not([hidden])");
      await pageSettled(p);
    },
  },
  {
    name: "rail-tooltip",
    what: "a company heading's two controls and the plus's \"+ New chat\" tooltip",
    drive: async (p) => {
      await p.focus(".nav-group--entity .nav-group-add");
      await p.waitForSelector("#rail-tip:not([hidden])");
      await pageSettled(p);
    },
  },
  {
    name: "thread-menu",
    what: "the thread's own actions menu — rename, pin, archive (§3.1)",
    drive: async (p) => {
      // Through the row's own `⋯` button, which is the affordance the CEO presses. The menu
      // is built on open, so it does not exist to be measured until this click happens —
      // which is exactly why nothing walked it.
      await p.hover('.nav-thread[data-thread-id="acme"]');
      await p.click('.nav-thread[data-thread-id="acme"] + .nav-thread-more');
      await p.waitForSelector("#thread-menu:not([hidden])");
      // The menu is BUILT on open, so "not hidden" can be true of an empty container for a
      // frame. Its own rows are what this walk measures.
      await p.waitForFunction(() => {
        const m = document.getElementById("thread-menu");
        return !!m && !m.hidden && m.querySelectorAll("button").length > 0;
      });
      await overlaySettled(p, "#thread-menu");
    },
  },
  {
    name: "techy-scope",
    what: "§7.1's three-way scope choice — the three options the CEO picks between",
    drive: async (p) => {
      // EVERY STRING ON THIS SHEET IS A CHOICE HE IS BEING ASKED TO MAKE, which is the
      // opposite of the narrow "not meant to be read closely" exemption — so all of it is
      // measured and none of it is declared exempt.
      //
      // Opened through the technical view's one switch, in Settings (round 27), which is the
      // CEO's path: `#set-techy` flips, `requestTechyToggle` sees a conversation open, and the
      // sheet asks where. Settings closes as the sheet opens (main.js `openTechyScope`).
      //
      // `hiring` AND NOT `acme`, and the reason is check 11 rather than taste. This is a
      // `role="dialog"` sheet, so everything behind it is filed `obscured` — and check 11
      // refuses a node that is obscured on every surface reaching it and measured on none.
      // The first draft opened `acme`, whose conversation NO other driver in this file
      // opens (every conversation walk here uses `hiring`, `legacy` or `partner`), so seven
      // of that thread's turn nodes became reachable-but-never-measured the moment this
      // surface existed and 11 went red. `hiring` is the thread the `thread` surface walks
      // in full, so everything this sheet covers is measured there, uncovered.
      //
      // It also has to be a thread WITH a company: "this company" only means something with
      // one, the middle option is HIDDEN for a conversation with no binding, and a walk that
      // opened the sheet on an unbound thread would measure two options while reporting
      // three. `hiring` is in `northwind`, so all three are on screen.
      await p.click('.nav-thread[data-thread-id="hiring"]');
      await atHiringThread(p);
      await p.click("#set-btn");
      await p.waitForSelector("#set-menu:not([hidden]) #set-techy");
      await p.check("#set-techy");
      await p.waitForSelector("#techy-scope:not([hidden])");
      // All three options on screen, since the whole claim is that each of them is legible.
      // A sheet that painted before `openTechyScope` un-hid the middle row would measure
      // two and pass.
      await p.waitForFunction(() => {
        const rows = document.querySelectorAll("#techy-scope-options .techy-scope-option");
        return rows.length === 3 && [...rows].every((r) => !r.hidden && r.getBoundingClientRect().height > 0);
      });
      await overlaySettled(p, "#techy-scope");
    },
  },
  {
    name: "techy-nothing-recorded",
    what: "techy mode on a conversation with no machinery — the sentence that is not 'I can't read it'",
    drive: async (p) => {
      // `partner` is the seeded thread with an empty machinery store. The whole point of
      // `#techy-state` is that "nothing was recorded" and "I can't read it" are DIFFERENT
      // statements, so the surface that says the first one has to be legible.
      // `.tl-turn` again belongs to whatever thread is on the stage. The keystroke below sets
      // the technical view for the ACTIVE thread, so a walk that pressed it before `partner`
      // had arrived would pin the technical view onto a different conversation and then measure
      // the answer it gave about that one.
      await p.click('.nav-thread[data-thread-id="partner"]');
      await settleOnThread(p, "partner");
      await p.keyboard.press("Meta+Shift+T");
      // ⌘⇧T ASKS WHERE IT APPLIES since 2026-09-18 (CEO §9.1: the choice has no exception
      // in it, not even for a shortcut), so this is two acts, not one: press the key, then
      // take "For this conversation only" — which leaves exactly the state this surface was
      // walked in before the sheet existed.
      await techyScopeThisConversation(p);
      await p.waitForSelector('#techy-state[data-state="nothing_recorded"]');
      await pageSettled(p);
      await assertOnThread(p, "partner", "techy-nothing-recorded");
    },
  },
  {
    name: "history-notice",
    what: "the notice that says part of his history did not load, at the top of the conversation",
    drive: async (p) => {
      // NOT CLEAN, ON PURPOSE. The default fixture is a clean load, so this surface has to
      // ask for the state it is measuring — the panel can never be shown to pass by a
      // fixture that was already painting it. Both halves are driven at once: the calm
      // "from a newer version" sentence and the loud damaged one share the panel, so the
      // worst-case foreground on the worst-case background is what gets walked.
      await p.evaluate(async () => {
        window.__RICHOS_MOCK__.historySet({
          records_read: 214,
          records_applied: 211,
          skipped: 3,
          from_future: 2,
          damaged: 1,
          ambiguous: 0,
          headline: "One record of this conversation could not be read.",
          detail:
            "2 records were written by a newer version of RichOS than the one you are " +
            "running, so this version does not know how to read them. Updating will bring " +
            "them back. 1 record is damaged and could not be read. Everything else loaded: " +
            "211 of 214 records. Nothing was deleted and nothing was rewritten — every " +
            "record is still exactly where it was on disk.",
        });
        await window.__RICHOS_HISTORY_NOTICE__();
      });
      await p.waitForSelector("#history-notice:not([hidden])");
      // Both halves on screen, since the walk's whole claim is that it measures the worst-case
      // foreground on the worst-case background and the two share the panel.
      await p.waitForFunction(() => {
        const n = document.getElementById("history-notice");
        return !!n && !n.hidden && n.textContent.trim().length > 80;
      });
      await overlaySettled(p, "#history-notice");
    },
  },
  // THE FIRST-RUN NOTICE, IN BOTH OF ITS PAINTED STATES. Neither is reachable from any
  // surface above: the preview's default is a described install, which renders none of it —
  // deliberately, so this panel can never be shown to pass by a fixture that was already
  // painting it. Without these two walks the whole surface would be uncovered, and check 10c
  // would refuse a shell-declared panel nobody had measured.
  //
  // The two ARE different ink and not one surface twice: the offer paints `--ink` prose, the
  // muted consequence line and two controls, one of them filled with the accent; the unusable
  // state paints `--attention` on the headline and on the border and draws no control at all.
  {
    name: "first-run-notice",
    what: "the offer of the bootstrap interview at the head of the conversation, and its two controls",
    drive: async (p) => {
      await p.waitForSelector("#first-run:not([hidden])");
      await p.waitForSelector("#first-run-actions:not([hidden])");
      await pageSettled(p);
    },
    preset: { onboarding: "not-yet" },
  },
  {
    name: "first-run-unusable",
    what: "notes about his company that could not be read — the one onboarding state that needs a person",
    drive: async (p) => {
      await p.waitForSelector('#first-run[data-state="unusable"]:not([hidden])');
      await pageSettled(p);
    },
    preset: { onboarding: "unusable" },
  },
  {
    name: "opening-screen",
    // THE HARDEST SURFACE, WALKED ANYWAY. It would have been easy to leave the opening
    // screen out and write "not covered" in the header — and the honest result of walking it
    // is that its one line of HTML text CANNOT BE PROVEN by this or any DOM checker: it sits
    // on `--splash-atmosphere`, a gradient, under a `mix-blend-mode: soft-light` lamp and a
    // `mix-blend-mode: overlay` grain. That fact is now a named entry in
    // `contrast-debt.json`'s `knownUnresolvable` rather than a surface nobody looked at,
    // which is the difference between a stated blind spot and an unstated one.
    holdSplash: true,
    what: "the opening screen, curtain held up (its one HTML text line; the rest is SVG)",
    drive: async (p) => {
      await p.waitForSelector("#splash", { timeout: 5000 });
      // THE BAR IS THE ONE THING ON THIS SURFACE `captureSettled` CANNOT SEE, and it is the
      // thing that was moving under the 700 ms this replaces.
      //
      // Every CSS animation here ends by 1,380 ms (measured 2026-09-06: four `splash-rise`
      // stages at 950/1080/1220/1380 and the bar's own transition at 1320), so `awaitSettled`
      // and `pinLoops` between them handle everything in `document.getAnimations()`. The
      // LOADING BAR IS NOT IN THERE: `splash.js` paints it from its own
      // `requestAnimationFrame` tick, on `shownAt`, so its fill is a function of wall-clock
      // time since the curtain went up and no animation-based settle can observe it. A shot
      // taken at any fixed offset photographs the bar at whatever fraction the machine had
      // reached, which is why `opening-screen.png` moved by 4,043 / 4,292 / 4,670 pixels
      // across three consecutive runs when the sample point was moved.
      //
      // `state.barStopped` is the product's own answer — "true once `tick()` has returned
      // without asking for another frame" — and after it there is nothing left moving. That is
      // roughly the 3 s hold plus the flare, which is affordable precisely because
      // `HOLD_CURTAIN` has disarmed the ceiling: there is no clock underneath this walk for a
      // longer wait to fall foul of, and `assertCurtainHeld` has already proved it.
      await pageSettled(p);
      await p.waitForFunction(
        () => {
          const s = window.RichSplash;
          if (!s || !s.state || !s.state.shown || s.state.reason) return false;
          if (!document.getElementById("splash")) return false;
          return s.state.barStopped === true;
        },
        undefined,
        { timeout: 15000 }
      );
    },
  },
];

// ---------------------------------------------------------------------------------------
// Driving
// ---------------------------------------------------------------------------------------

/// Answer the three-way scope sheet with his third option — the per-conversation tier — and
/// wait for it to be gone. Called straight after `Meta+Shift+T`, which opens it (§7.1, the
/// CEO's answer of 2026-09-18; before that date the keystroke applied this tier by itself).
///
/// KEYBOARD, NOT THE MOUSE, AND THE REASON WAS MEASURED RATHER THAN PREFERRED. The first
/// version clicked the radio and the confirm button, which moves the pointer to the middle
/// of the window — out of the rail, where `p.click('.nav-thread...')` had left it. That
/// un-hovers the conversation row and takes its `.nav-thread-more` (the ⋯ overflow) off the
/// screen: `technical-view` fell from 148 measured nodes to 146 and `techy-nothing-recorded`
/// from 118 to 116, with a 14-pixel shot change at [269..279, 308..309] — exactly the glyph
/// — for a reason with nothing to do with the surface under test. Arrow keys move the native
/// radio group and Enter confirms, so the pointer never moves and the walk measures what it
/// measured before the sheet existed.
///
/// The selection is ASSERTED before it is confirmed. A silent confirm of the wrong tier
/// would turn the technical view on for every conversation in the fixture and fail nothing
/// here, which is the kind of green that costs a day.
async function techyScopeThisConversation(page) {
  await page.waitForSelector("#techy-scope:not([hidden])");
  const checkedScope = () =>
    page.evaluate(() => {
      const on = document.querySelector('#techy-scope input[name="techy-scope"]:checked');
      return on ? on.value : null;
    });
  // The number of presses is DERIVED, not typed: the middle option is absent for a
  // conversation in no company, so "two downs" would be one too many there.
  for (let i = 0; i < 3 && (await checkedScope()) !== "thread"; i++) {
    await page.keyboard.press("ArrowDown");
  }
  const picked = await checkedScope();
  if (picked !== "thread") {
    throw new Error('scope sheet: expected "thread" to be selected, got ' + picked);
  }
  await page.keyboard.press("Enter");
  await page.waitForSelector("#techy-scope", { state: "hidden" });
}

/// A NON-TEXT INDICATOR IS MEASURED AGAINST THE PLAIN GROUND IT SITS ON, WITH THE SPECKLE
/// HIDDEN. The lead's ruling of 2026-09-29, correcting echo-opus-speckle3's brief: a speckle dot
/// under a border, an icon or the logo is not a contrast failure; the worst-single-pixel rule is
/// for TEXT only; a non-text indicator keeps its own floor, measured on the plain ground.
///
/// WHY A CROP OVER THE SPECKLE CANNOT BE READ THAT WAY, measured on this branch: the light
/// chevron `.tl-tech-chevron` is `--ink-tech`, rgba(12, 19, 34, 0.75). Shot as painted, its
/// 9x15 crop had four core ink values (#5b5d64, #5d5e62, #5c5f65 x2, #5d6066, 5.06-5.28:1 on
/// #eae6dd), because a translucent ink over a ground that changes pixel by pixel never repeats
/// a value. `measureIndicatorCrop` needs a value 3 times, so it passed over the ink and took an
/// antialiased edge, #c5c2be (3 samples), as the "ink": 1.42:1. Shot with the speckle hidden:
/// #5c5f65 on #eae6dd, 5.14:1. The 1.42 was an edge pixel against the ground, not the chevron.
///
/// The speckle is the canvas `body > canvas.speckle` (speckle-app.js). It is hidden for the one
/// shot and restored, so every other measurement in the walk still sees the app as shipped.
/// `tally.hidden` counts the shots it was really there for and the check prints it, so a run in
/// which the speckle never mounted says so rather than reading as a measurement around it.
async function onPlainGround(page, shoot, tally) {
  const hidden = await page.evaluate(() => {
    const c = document.querySelector("body > canvas.speckle");
    if (!c) return false;
    c.style.setProperty("visibility", "hidden", "important");
    return true;
  });
  try {
    if (hidden) {
      await page.evaluate(() => new Promise((r) => requestAnimationFrame(() => requestAnimationFrame(r))));
    }
    const buf = await shoot();
    if (tally) {
      tally.shots += 1;
      if (hidden) tally.hidden += 1;
    }
    return buf;
  } finally {
    if (hidden) {
      await page.evaluate(() => {
        const c = document.querySelector("body > canvas.speckle");
        if (c) c.style.removeProperty("visibility");
      });
    }
  }
}

async function openApp(browser, theme, holdSplash, preset) {
  const page = await browser.newPage({ viewport: { width: 1400, height: 950 }, colorScheme: theme });
  // The occlusion check joins node paths across independent pages. The preview
  // gives seeded turns random IDs, so use one repeatable stream in this suite:
  // a covered turn must have the same identity when another walk reveals it.
  await page.addInitScript(() => {
    let state = 0x6a09e667;
    Math.random = () => {
      state ^= state << 13; state ^= state >>> 17; state ^= state << 5;
      return (state >>> 0) / 4294967296;
    };
  });
  const errors = [];
  page.on("pageerror", (e) => errors.push(String(e)));
  page.on("console", (m) => {
    if (m.type() === "error") errors.push("console: " + m.text());
  });

  // DRIVE THE APP'S OWN THEME, NOT THE OS PREFERENCE — and this is the whole reason this
  // suite kept meaning something on the day dark mode landed.
  //
  // Until 2026-08-31 the app shipped ONE palette, so `colorScheme` on the page was a
  // reasonable stand-in for "the other theme": there was no other theme, and check 10 said
  // so out loud. Now there are two, and which one paints is a STORED PREFERENCE in
  // `config.rs` applied as `:root[data-theme="light"]` — never `@media (prefers-color-scheme:
  // dark)`, which appears nowhere in the shipped CSS and is what check 10 counts.
  //
  // THE SENTENCE THAT USED TO BE HERE WAS "An OS preference decides NOTHING about which
  // palette this app paints", AND SINCE §63 (2026-09-19) IT IS FALSE. The stored preference
  // can now BE `system`, which is the default a fresh install gets, and `theme-boot.js`
  // resolves it against `prefers-color-scheme` on every change. The OS decides the palette
  // whenever the user has not — which is most users, most of the time. What is still true,
  // and is the part this seeding depends on, is that the OS cannot OVERRULE a preference he
  // set: seed `light` or `dark` here and `colorScheme` is inert, whatever it says.
  //
  // So had this line stayed as it was, both "themes" would have walked the DARK palette,
  // the light half of every check above would have been a re-run of the dark half, and the
  // suite would have reported a clean sweep of both themes while never once measuring light
  // mode — the precise failure mode check 10 was written to catch, arriving through the
  // door check 10 was not watching. `colorScheme` is still passed, because `system` resolves
  // against it and a theme-independent element that DID respond to the OS should still be
  // seen; but the app's own preference is what is set here.
  //
  // It is seeded into `localStorage` in an init script, which is to say through the exact
  // mirror `theme-boot.js` reads before first paint. Setting `data-theme` after load would
  // measure a state the shipping app never actually boots into.
  // BOTH the mirror and the STORE, and the second one is the one that decides. Seeding
  // `richos-theme` alone does not work and finding out why is worth the two lines: the
  // mirror is a cache, `syncAppearanceFromBackend` reconciles it against the backend at
  // init, and the BACKEND WINS — so a mirror-only seed is overwritten by the store's answer
  // a few hundred milliseconds after boot, and the walk measures the store's theme while
  // claiming the seed's. That is the product behaving exactly as designed. The test has to
  // seed the thing that actually holds the preference.
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

  if (holdSplash) {
    // Hold the curtain deterministically rather than racing it. `main.js` calls
    // `RichSplash.yieldNow("app-ready")` the moment the shell is usable, which on this
    // machine is well inside a second — a walk that tried to be quick enough would be a
    // flake generator.
    //
    // WHAT USED TO BE HERE WAS HALF OF IT. Intercepting the assignment of
    // `window.RichSplash` neuters the EXPORTED yield, and `start()` also arms a ceiling over
    // the internal one — so this walk had 4,000 ms from `goto` to the end of a full
    // both-themes contrast pass plus a screenshot, and the `opening-screen` surface's floor
    // could not have noticed the curtain leaving mid-walk, because the curtain is an overlay
    // and the shell's text nodes are underneath it either way. `HOLD_CURTAIN` takes the
    // ceiling out and `assertCurtainHeld` below proves it, which is the difference between a
    // walk of the opening screen and a walk of the shell behind it.
    await page.addInitScript(HOLD_CURTAIN);
  }
  // A DELIBERATE SLOW RUNNER, on demand. Wrapping the bridge at its ASSIGNMENT, so every
  // caller in the page is behind it — including the ones a driver sets off and returns from.
  // This file wrote the slow runner first and inline; it now lives in `lib/harness.js` as
  // `SLOW_BRIDGE`, byte-identical in behavior, so `updates.js` can ask the same question of
  // itself. One copy, because two copies of a diagnostic drift and then disagree about what
  // was reproduced.
  if (LAG_MS > 0) await page.addInitScript(SLOW_BRIDGE, LAG_MS);
  // A PRE-BOOT MOCK PRESET, for the one state a setter cannot reach: "no company has ever
  // been chosen" is decided before `init()` branches on whether a thread is active, so it
  // has to be in place before any of the page's own scripts run (mock.js's own comment on
  // `__RICHOS_MOCK_PRESET__` says the same thing from the other side).
  if (preset) {
    await page.addInitScript((v) => {
      window.__RICHOS_MOCK_PRESET__ = v;
    }, preset);
  }
  await page.goto(APP);
  // The home screen is the landing surface now; this suite is about the app UI behind it.
  await leaveHome(page);
  await page.waitForSelector(".nav-thread", { state: "attached" });
  // Everything the shell fetches for itself, before anything is measured through it.
  await shellSettled(page);
  if (holdSplash) {
    // The hold is PROVEN before anything is measured through it: exactly one timer taken out
    // of `start()`, armed for longer than the hold it guarded, and a curtain still on screen
    // with `state.reason` still null. Without this, a walk that lost the curtain partway
    // would report the shell underneath it and read perfectly clean.
    page.__curtain = await assertCurtainHeld(page);
    // §15's always-dark clamp is in force for the whole of this walk, by ruling and not by
    // accident, so a light-labeled opening-screen walk correctly reports dark.
    assertTheme(await page.evaluate(() => document.documentElement.getAttribute("data-theme")), "dark", theme);
    page.__errors = errors;
    return page;
  }
  // The opening curtain is `pointer-events: none` and therefore invisible to a hit test
  // while still being painted over everything. Waiting for it to leave rather than racing
  // it: a walk taken underneath it would report the whole shell unresolvable.
  await page.waitForFunction(() => !document.getElementById("splash"), null, { timeout: 30000 }).catch(() => {});
  // WAIT FOR `init()` TO HAVE DECIDED, NOT FOR 300 MS — AND DELIBERATELY NOT FOR THE THEME.
  //
  // What the 300 ms was buying is `syncAppearanceFromBackend()`, which reads `get_appearance`
  // over the bridge and hands the answer to `RichTheme.sync`. THE BACKEND WINS, so a walk that
  // sampled before it landed would read the mirror's value and label itself with a palette the
  // store was about to overrule.
  //
  // WAITING FOR `data-theme` TO EQUAL THE ASKED-FOR THEME WOULD BE THE WRONG FIX, and it is
  // worth saying why out loud: the very next line ASSERTS that equality, so a wait on it would
  // make the assertion unfalsifiable — a check that cannot fail, which is this directory's
  // oldest defect wearing a determinism fix as a disguise. `bootSettled` waits on something
  // else entirely: `init()` reaching its own branch point, which is many statements AFTER the
  // appearance sync (`syncAppearanceFromBackend` is called before `refreshNavigation`, and the
  // branch is after it). So the reconciliation has provably happened and `assertTheme` still
  // has every one of its teeth.
  await bootSettled(page);
  await pageSettled(page);
  // THE THEME IS CHECKED HERE AND NOT EARLIER, and the reason is a real one this assertion
  // caught the day the splash gained a duration. While the opening screen's curtain is up
  // the resolved theme is CLAMPED TO DARK (§15's one permanent exception), so a light walk
  // sampled before the curtain lifts reports dark — truthfully, and about a state this walk
  // is not measuring. Once the splash held for three seconds rather than for however long
  // booting took, that window stopped being too narrow to hit and every light walk failed.
  // The clamp drops with the curtain, so this is the first moment the answer is about the
  // palette the walk is actually going to measure.
  assertTheme(await page.evaluate(() => document.documentElement.getAttribute("data-theme")), theme, theme);
  page.__errors = errors;
  return page;
}

/// The seed either took or this walk is measuring something other than what it claims. A
/// silent miss here would relabel a dark walk as a light one, which is worse than no walk.
function assertTheme(painted, expected, asked) {
  assert(
    painted === expected,
    "asked for the " + asked + " theme and the document painted " + painted + " — the walk " +
      "below would be labelled with a palette it is not measuring"
  );
}

async function walk(page, surface, theme) {
  await page.evaluate(C.pageScript());
  return page.evaluate((o) => window.__contrastProbe(o), { surface, theme });
}

// ---------------------------------------------------------------------------------------
// THE PANEL INVENTORY — derived from the shell, so a NEW panel cannot be born unwalked
// ---------------------------------------------------------------------------------------
//
// Check 10b already refuses a driver list that got SHORTER. Nothing refused one that got
// STALE — a panel added to `index.html` with no driver written for it is invisible to every
// check in this file, and the file would report the same confident totals over a shell with
// one more surface in it than it walks. That is the same defect as a typed source list,
// wearing a driver list's clothes.
//
// WHAT COUNTS AS A PANEL, derived by shape rather than named: a CONTAINER element
// (`div`/`section`/`aside`/`dialog`/`form`/`nav`) with an `id`, that is `hidden` when the
// shell first paints or carries `role="dialog"`, and that is not itself inside another such
// element. Hidden-at-load is the signal: it is a state the app can enter and is not in, which
// is exactly what a driver exists to reach. A hidden child of a hidden panel is part of that
// panel, not a panel of its own, which is what the ancestor walk removes.
const PANEL_TAGS = ["DIV", "SECTION", "ASIDE", "DIALOG", "FORM", "NAV"];

const PANEL_PROBE = (tags) => {
  const out = [];
  const isPanel = (e) =>
    tags.indexOf(e.tagName) >= 0 && !!e.id && (e.hasAttribute("hidden") || e.getAttribute("role") === "dialog");
  for (const e of document.querySelectorAll("[id]")) {
    if (!isPanel(e)) continue;
    let anc = e.parentElement;
    let nested = false;
    while (anc) {
      if (isPanel(anc)) { nested = true; break; }
      anc = anc.parentElement;
    }
    if (!nested) out.push(e.id);
  }
  return out.sort();
};

/// The inventory, read off a shell that has just loaded and been driven nowhere.
async function declaredPanels(browser) {
  const page = await openApp(browser, "dark", false);
  const ids = await page.evaluate(PANEL_PROBE, PANEL_TAGS);
  await page.close();
  return ids;
}

/// Which of those panels are actually ON SCREEN right now — present, not `hidden`, and with
/// area. A driver that OPENED a panel and one that merely left it in the document are
/// different things, and only the first is coverage.
///
/// The ids are handed in rather than re-derived here, because opening a panel is exactly
/// what removes the `hidden` attribute the inventory is derived from — a shape test run
/// against a driven page would stop recognizing the very panel the driver just opened.
async function panelsOnScreen(page, ids) {
  return page.evaluate((wanted) => {
    const out = [];
    for (const id of wanted) {
      const e = document.getElementById(id);
      if (!e || e.hidden) continue;
      const r = e.getBoundingClientRect();
      const s = getComputedStyle(e);
      if (r.width <= 0 || r.height <= 0 || s.visibility === "hidden" || s.display === "none") continue;
      out.push(id);
    }
    return out;
  }, ids);
}

/// One line per failure, in the shape a person can act on without opening a debugger: the
/// ratio it got, the floor it needed, the two colours, the size, and where it is.
function describe(sig, f) {
  return (
    "      " + f.ratio + ":1 (needs " + f.threshold + ":1)  " + f.fg + " on " + f.bg +
    "  " + (f.indicator ? "non-text indicator" : f.fontSize + "px/" + f.fontWeight) +
    "  x" + f.nodes + "\n        " + f.selector + (f.text ? "   “" + f.text + "”" : "")
  );
}

// ---------------------------------------------------------------------------------------

async function main() {
  const run = createRun("WCAG AA contrast — the shipping shell, both themes, computed not eyeballed");
  const { webkit } = loadPlaywright();
  const browser = await webkit.launch();
  fs.mkdirSync(SHOTS, { recursive: true });

  const debt = JSON.parse(fs.readFileSync(DEBT_FILE, "utf8"));
  /// Everything the whole run saw, so the cross-surface checks (11, 13) have real inputs
  /// rather than a second walk that might disagree with the first.
  const seen = {
    signatures: new Set(),
    obscured: new Set(),
    measured: new Set(),
    unresolvable: [],
    exemptions: [],
    canvas: 0,
    canvasInHome: 0,
    canvasInPhone: 0,
    canvasSpeckle: 0,
    canvasResources: 0,
    svgText: 0,
    /// Which of the shell's DECLARED panels any walk actually put on screen — see check 10c.
    panelsReached: new Map(),
    /// Text the STYLESHEETS render (`::before`/`::after`), collected across every walk — see
    /// check 16. `measured` and `hidden` are keyed by the TEXT so the join against
    /// `lib/ui-sources.js`'s source-side derivation is on the string, not on a selector.
    generated: { considered: 0, checked: 0, measured: new Map(), hidden: new Map(), unprovable: [] },
    totals: { considered: 0, checked: 0, passed: 0, failedNodes: 0, invisible: 0, obscured: 0, ancestor: 0, veiled: 0, indicators: 0, indicatorsChecked: 0 },
  };

  // ---- 1. the arithmetic agrees with the calculator the rule names ----------------------

  await run.check("1  the ratio is WCAG's ratio — checked against WebAIM's own published values", async () => {
    // https://webaim.org/resources/contrastchecker/ — the reference the standing rule names.
    // These are the numbers that page reports for these pairs, to its own two decimals.
    //
    // EVERY PAIR HERE IS ONE THE CALCULATOR PUBLISHES. An earlier version of this table
    // carried `#2f3a56 on #f7f6f2` — the app's own accent on its own paper — with an
    // expected 10.32 that was not read off anything; the real answer is 10.44 and the check
    // failed on its own invented number. A reference table whose entries are guesses tests
    // the guesser. #767676 and #777777 are here because they are the pair WebAIM documents
    // as straddling 4.5, and #949494 because check 3 leans on it being 3.03.
    const table = [
      ["#000000", "#ffffff", 21],
      ["#ffffff", "#ffffff", 1],
      ["#777777", "#ffffff", 4.48],
      ["#767676", "#ffffff", 4.54],
      ["#949494", "#ffffff", 3.03],
      ["#0000ff", "#ffffff", 8.59],
      ["#ff0000", "#ffffff", 3.998],
      ["#595959", "#ffffff", 7.0],
    ];
    const rgb = (h) => ({ r: parseInt(h.slice(1, 3), 16), g: parseInt(h.slice(3, 5), 16), b: parseInt(h.slice(5, 7), 16), a: 1 });
    for (const [fg, bg, expected] of table) {
      const got = C.round2(C.contrastRatio(rgb(fg), rgb(bg)));
      assert(Math.abs(got - expected) <= 0.01, `${fg} on ${bg}: expected ${expected}:1, computed ${got}:1`);
    }
    // Alpha is composited, not ignored — the app's own `rgba(0,0,0,0.025)` hovers depend on it.
    const over = C.compositeOver({ r: 0, g: 0, b: 0, a: 0.5 }, { r: 255, g: 255, b: 255, a: 1 });
    assertEqual([Math.round(over.r), Math.round(over.g), Math.round(over.b)], [128, 128, 128], "50% black over white must composite to mid grey");
    return table.length + " published pairs, all within 0.01 of WebAIM";
  });

  // ---- 2. the arithmetic in the browser is THE SAME arithmetic --------------------------

  await run.check("2  the math running in WebKit is the same source, not a second copy", async () => {
    const page = await openApp(browser, "light");
    await page.evaluate(C.pageScript());
    const pairs = [
      ["rgb(165, 162, 151)", "rgb(247, 246, 242)"],
      ["rgba(32, 31, 27, 0.5)", "rgb(255, 255, 255)"],
      ["rgb(47 58 86 / 0.8)", "rgb(240, 239, 233)"],
      ["#nonsense", "rgb(0,0,0)"],
    ];
    const inPage = await page.evaluate((ps) => {
      const M = window.__contrastMath;
      return ps.map(function (p) {
        const f = M.parseCssColor(p[0]);
        const b = M.parseCssColor(p[1]);
        if (!f || !b) return null;
        return M.round2(M.contrastRatio(M.compositeOver(f, b), b));
      });
    }, pairs);
    const inNode = pairs.map(([f, b]) => {
      const fc = C.parseCssColor(f);
      const bc = C.parseCssColor(b);
      if (!fc || !bc) return null;
      return C.round2(C.contrastRatio(C.compositeOver(fc, bc), bc));
    });
    assertEqual(inPage, inNode, "WebKit and node must compute identical ratios from identical inputs");
    assertEqual(inPage[3], null, "an unrecognised colour notation must return null in BOTH — never a silent black");
    await page.close();
    return pairs.length + " pairs identical across the two runtimes: " + JSON.stringify(inPage);
  });

  // ---- 3. the thresholds sit where WCAG puts them ---------------------------------------

  await run.check("3  4.5 normal, 3.0 at 24px or 18.66px bold, 3.0 for a declared indicator", async () => {
    assertEqual(
      [
        C.isLargeText(23.9, "400"), C.isLargeText(24, "400"),
        C.isLargeText(18.65, "700"), C.isLargeText(18.66, "700"), C.isLargeText(18.66, "600"),
      ],
      [false, true, false, true, false],
      "the large-text boundary must be 24px, or 18.66px at weight >= 700"
    );
    // And the walk must APPLY them, which is a different claim. #767676 on white is 4.54 —
    // over 4.5, under nothing — so it passes as normal text and passes as large. #949494 is
    // 3.03: it FAILS as normal text and PASSES as large, which is the pair that proves the
    // threshold is being read off the font and not hardcoded.
    const page = await openApp(browser, "light");
    const r = await page.evaluate(async (script) => {
      eval(script);
      const host = document.createElement("div");
      host.style.cssText = "position:fixed;left:20px;top:400px;z-index:99999;background:#ffffff;padding:8px";
      host.innerHTML =
        '<p id="t-normal" style="color:#949494;background:#ffffff;font-size:14px;font-weight:400;margin:0">normal fourteen</p>' +
        '<p id="t-big" style="color:#949494;background:#ffffff;font-size:24px;font-weight:400;margin:0">big twenty four</p>' +
        '<p id="t-bold" style="color:#949494;background:#ffffff;font-size:19px;font-weight:700;margin:0">bold nineteen</p>' +
        '<p id="t-ind" data-contrast-role="indicator" style="color:#949494;background:#ffffff;font-size:14px;margin:0">declared indicator</p>';
      document.body.appendChild(host);
      const out = window.__contrastProbe({ surface: "threshold-fixture", theme: "light" });
      host.remove();
      const failing = Object.values(out.failures).map(function (f) { return f.selector + "@" + f.threshold; });
      return failing.filter(function (s) { return s.indexOf("t-") >= 0; });
    }, C.pageScript());
    assertEqual(r, ["p#t-normal@4.5"], "only the 14px/400 node may fail: 24px, 19px-bold and the declared indicator all clear 3.0");
    await page.close();
    return "3.03:1 fails at 14px/400 and passes at 24px, at 19px bold, and as a declared indicator";
  });

  // ---- 4. a real failure is caught, by name ---------------------------------------------

  await run.check("4  a deliberately-introduced failure is caught and NAMED, not just counted", async () => {
    const page = await openApp(browser, "light");
    const r = await page.evaluate(async (script) => {
      eval(script);
      const host = document.createElement("div");
      host.style.cssText = "position:fixed;left:20px;top:400px;z-index:99999;background:#ffffff;padding:8px";
      host.innerHTML = '<p id="planted-failure" style="color:#b0b0b0;background:#ffffff;font-size:13px;margin:0">a caption nobody can read</p>';
      document.body.appendChild(host);
      const out = window.__contrastProbe({ surface: "planted", theme: "light" });
      host.remove();
      const hit = Object.entries(out.failures).filter(function (e) { return e[0].indexOf("#b0b0b0") >= 0; });
      return hit.map(function (e) { return { sig: e[0], f: e[1] }; });
    }, C.pageScript());
    assertEqual(r.length, 1, "the planted node must produce exactly one failure signature");
    const f = r[0].f;
    assertEqual(f.selector, "p#planted-failure", "the failure must name the node");
    // The expectation comes from the arithmetic check 1 proved against WebAIM, not from a
    // number typed here — a typed one tests whoever typed it.
    const expected = C.round2(C.contrastRatio(C.parseCssColor("rgb(176,176,176)"), C.parseCssColor("rgb(255,255,255)")));
    assertEqual([f.ratio, f.threshold], [expected, 4.5], "and carry the computed ratio and the floor it missed");
    assert(f.text.indexOf("a caption nobody can read") === 0, "and quote the text, so it can be found on screen");
    await page.close();
    return "p#planted-failure caught at " + f.ratio + ":1 against a 4.5:1 floor, quoted verbatim";
  });

  // ---- 5. a declared exemption passes AND prints -----------------------------------------

  await run.check("5  a declared exemption passes, is NOT hidden, and carries its reason", async () => {
    const page = await openApp(browser, "light");
    const r = await page.evaluate(async (script) => {
      eval(script);
      const host = document.createElement("div");
      host.style.cssText = "position:fixed;left:20px;top:400px;z-index:99999;background:#ffffff;padding:8px";
      host.innerHTML =
        '<p id="declared" data-contrast-exempt="privacy boilerplate, not meant to be read closely" ' +
        'style="color:#b0b0b0;background:#ffffff;font-size:13px;margin:0">Your data never leaves this machine. ' +
        '<a href="#" style="color:#b0b0b0">Details</a></p>';
      document.body.appendChild(host);
      const out = window.__contrastProbe({ surface: "declared", theme: "light" });
      host.remove();
      return {
        failures: Object.keys(out.failures).filter(function (k) { return k.indexOf("#b0b0b0") >= 0; }),
        exempt: Object.entries(out.exempt).map(function (e) { return e[1]; }),
      };
    }, C.pageScript());
    assertEqual(r.failures, [], "a declared exemption must not fail the run");
    assert(r.exempt.length >= 1, "and must not vanish either — it has to appear in the exempt list");
    const e = r.exempt[0];
    assertEqual(e.reason, "privacy boilerplate, not meant to be read closely", "with the reason as written in the markup");
    assert(e.ratio < 4.5, "and the ratio it was excused at (" + e.ratio + "), so the excuse is quantified");
    // The exemption covers the subtree — a link inside the notice is part of the notice.
    assert(r.exempt.length >= 2 || e.nodes >= 2, "and it must reach the anchor inside it, not just the paragraph");
    await page.close();
    return r.exempt.length + " exempt entr(ies), reason printed, excused ratio " + e.ratio + ":1 stated";
  });

  // ---- 6. an exemption with nothing to say is not an exemption ---------------------------

  await run.check("6  an empty or one-word exemption is itself a failure — no mute button", async () => {
    const page = await openApp(browser, "light");
    const r = await page.evaluate(async (script) => {
      eval(script);
      const host = document.createElement("div");
      host.style.cssText = "position:fixed;left:20px;top:400px;z-index:99999;background:#ffffff;padding:8px";
      host.innerHTML =
        '<p id="mute-empty" data-contrast-exempt="" style="color:#b0b0b0;background:#fff;font-size:13px;margin:0">empty claim</p>' +
        '<p id="mute-short" data-contrast-exempt="legal" style="color:#b0b0b0;background:#fff;font-size:13px;margin:0">one word claim</p>';
      document.body.appendChild(host);
      const out = window.__contrastProbe({ surface: "mute", theme: "light" });
      host.remove();
      return {
        unresolvable: Object.values(out.unresolvable).map(function (u) { return u.path; }),
        exempt: Object.keys(out.exempt).length,
      };
    }, C.pageScript());
    assertEqual(r.exempt, 0, "neither may be honoured as an exemption");
    assertEqual(r.unresolvable.sort(), ["p#mute-empty", "p#mute-short"], "both must be reported, by name, as unproven");
    await page.close();
    return "both the empty and the one-word claim are refused and named";
  });

  // ---- 7. unresolvable is failure-to-prove ----------------------------------------------

  await run.check("7  a colour that cannot be resolved is a FAILURE TO PROVE, never a pass", async () => {
    const page = await openApp(browser, "light");
    const r = await page.evaluate(async (script) => {
      eval(script);
      const host = document.createElement("div");
      host.style.cssText = "position:fixed;left:20px;top:300px;z-index:99999;background:#ffffff;padding:8px";
      host.innerHTML =
        '<p id="u-gradient" style="background:linear-gradient(90deg,#000,#fff);color:#888;font-size:14px;margin:0">over a gradient</p>' +
        '<p id="u-blend" style="mix-blend-mode:multiply;background:#fff;color:#888;font-size:14px;margin:0">blended</p>' +
        '<p id="u-backdrop" style="backdrop-filter:blur(3px);background:#fff;color:#888;font-size:14px;margin:0">backdropped</p>' +
        '<p id="u-filter" style="filter:invert(1);background:#fff;color:#888;font-size:14px;margin:0">filtered</p>' +
        // The gradient-text idiom. `background-clip: text` is invisible to WebKit's computed
        // style, so the guard on it never fires — but the idiom needs `color: transparent`
        // to work at all, and transparent text composites to exactly its own background.
        // It is caught as a 1:1 failure rather than a skip, which is the outcome that
        // matters; the mechanism is stated in lib/contrast.js rather than assumed here.
        '<p id="u-transparent" style="background:#fff;color:transparent;font-size:14px;margin:0">invisible ink</p>';
      document.body.appendChild(host);
      const out = window.__contrastProbe({ surface: "unresolvable", theme: "light" });
      host.remove();
      const failed = Object.values(out.failures).filter(function (f) { return f.selector.indexOf("u-") >= 0; });
      return {
        named: Object.values(out.unresolvable).map(function (u) { return u.path; }).filter(function (p) { return p.indexOf("u-") >= 0; }).sort(),
        transparent: failed.filter(function (f) { return f.selector === "p#u-transparent"; }).map(function (f) { return f.ratio; }),
      };
    }, C.pageScript());
    assertEqual(
      r.named,
      ["p#u-backdrop", "p#u-blend", "p#u-filter", "p#u-gradient"],
      "each of the four must land in the unresolvable bucket — none of them may be quietly skipped or passed"
    );
    assertEqual(r.transparent, [1], "and fully transparent text must read 1:1 and FAIL, never resolve to something readable");
    await page.close();
    return "gradient, mix-blend-mode, filter and backdrop-filter refused by name; transparent text fails at 1:1";
  });

  // ---- 8. a sheet over the text is part of the answer -------------------------------------

  await run.check("8  a faint veil is measured THROUGH; only an aria-modal claim excuses", async () => {
    const page = await openApp(browser, "light");
    const r = await page.evaluate(async (script) => {
      eval(script);
      const mk = function (modal, alpha) {
        const host = document.createElement("div");
        host.style.cssText = "position:fixed;left:20px;top:300px;z-index:99990;background:#ffffff;padding:8px";
        // #767676 on white is 4.54:1 — it PASSES, by four hundredths. Twelve percent of black
        // over the whole thing is enough to take it under, which is exactly the point: the
        // veil is not decoration, it is part of the number.
        host.innerHTML = '<p id="beneath" style="color:#767676;background:#ffffff;font-size:13px;margin:0">under a sheet</p>';
        const sheet = document.createElement("div");
        sheet.id = "sheet";
        sheet.style.cssText = "position:fixed;inset:0;z-index:99995;background:rgba(0,0,0," + alpha + ")";
        if (modal) sheet.setAttribute("aria-modal", "true");
        document.body.appendChild(host);
        document.body.appendChild(sheet);
        const out = window.__contrastProbe({ surface: "veil", theme: "light" });
        host.remove();
        sheet.remove();
        return out;
      };
      const faint = mk(false, 0.12);
      const claimed = mk(true, 0.12);
      const beneathFail = Object.values(faint.failures).filter(function (f) { return f.selector === "p#beneath"; });
      return {
        faintVeiled: faint.veiled,
        faintFailure: beneathFail.length ? beneathFail[0].ratio : null,
        claimedObscured: Object.keys(claimed.obscured).some(function (k) { return k.indexOf("p#beneath") >= 0; }),
        claimedFailures: Object.values(claimed.failures).filter(function (f) { return f.selector === "p#beneath"; }).length,
      };
    }, C.pageScript());
    const veil = { r: 0, g: 0, b: 0, a: 0.12 };
    const clear = C.round2(C.contrastRatio(C.parseCssColor("rgb(118,118,118)"), C.parseCssColor("rgb(255,255,255)")));
    const through = C.round2(
      C.contrastRatio(C.compositeOver(veil, C.parseCssColor("rgb(118,118,118)")), C.compositeOver(veil, C.parseCssColor("rgb(255,255,255)")))
    );
    assert(clear >= 4.5 && through < 4.5, "the fixture only proves anything if the veil is what takes it under: " + clear + " -> " + through);
    assert(r.faintVeiled >= 1, "a 12% sheet with no inertness claim must be composited, not waved through");
    assertEqual(r.faintFailure, through, "and the ratio reported must be the one the eye receives (" + through + ":1), not the stylesheet's " + clear + ":1");
    assert(r.claimedObscured, "the SAME sheet, once it claims aria-modal, makes the text behind it inert and unmeasured");
    assertEqual(r.claimedFailures, 0, "and inert text produces no failure");
    await page.close();
    return "12% sheet: " + clear + ":1 -> " + through + ":1 measured through; with aria-modal, filed obscured instead";
  });

  // ---- 9. the shipping shell, surface by surface, both themes ------------------------------

  // Derived once, off a shell driven nowhere, and used by both the walk loop and check 10c.
  const PANELS = await declaredPanels(browser);

  const perSurface = {};

  // THE SHELL'S OWN COUNTS, CAPTURED FROM THIS RUN, so every other surface's floor can be
  // what that surface ADDS to the shell rather than a count of the whole page. See the note
  // at the floor assertion below; `shell` is asserted to be the first surface walked because
  // everything after it reads these numbers.
  assert(
    SURFACES[0].name === "shell",
    "`shell` is no longer the first surface walked. Every other floor is relative to it, so " +
      "it has to be measured before they are checked."
  );
  let shellNow = null;
  // The text nodes a walk measured, as a multiset of their paths. Indicators are filed in the
  // same list with a suffix and are not text, so they are left out; what remains is exactly
  // `nodesChecked`, and the loop below asserts that, every walk.
  const textPaths = (out) => {
    const m = new Map();
    for (const p of out.measuredPaths || []) {
      if (!p.endsWith(" (indicator)")) m.set(p, (m.get(p) || 0) + 1);
    }
    return m;
  };
  // What a surface measured OVER the bare shell, in one theme: every measured node beyond the
  // number of times the shell measured the same path. A node the shell also measures is the
  // shell's, whether or not this surface's sheet happens to cover it, so new shell text can
  // neither raise nor lower this number.
  const measuredOver = (out, shellOut) => {
    const shellPaths = textPaths(shellOut);
    let n = 0;
    for (const [p, c] of textPaths(out)) n += Math.max(0, c - (shellPaths.get(p) || 0));
    return n;
  };

  for (const surface of SURFACES) {
    await run.check("9." + surface.name + "  " + surface.what, async () => {
      const lines = [];
      let newOnes = 0;
      let worse = 0;
      let debtHits = 0;
      let checked = 0;
      let considered = 0;
      let checkedOver = 0;
      const reachedPanels = [];
      // For a surface that holds the curtain: the proof that it was still held, in the log.
      let curtain = null;
      for (const theme of THEMES) {
        const page = await openApp(browser, theme, surface.holdSplash, surface.preset);
        if (page.__curtain) curtain = page.__curtain;
        await surface.drive(page);
        // WHICH DECLARED PANELS THIS DRIVER PUT ON SCREEN, recorded before the walk so that
        // check 10c can answer a question no other check here asks: not "was every named
        // surface walked?" (10b, which only catches the list getting SHORTER) but "is there a
        // panel in the shipped shell that no driver reaches at all?" — the list getting
        // stale by the product growing past it.
        for (const id of await panelsOnScreen(page, PANELS)) {
          reachedPanels.push(id + "/" + theme);
          if (!seen.panelsReached.has(id)) seen.panelsReached.set(id, []);
          seen.panelsReached.get(id).push(surface.name + "/" + theme);
        }
        const out = await walk(page, surface.name, theme);
        if (theme === "light") {
          // `parkPointer`: every surface above is reached by CLICKING it, and the pointer stays
          // where the last click left it. Where the new state puts a different control under
          // that point, the control is photographed hovered — `voice-model-progress.png`
          // alternated between a hovered and an unhovered `Stop the download` on every run,
          // 1,200 pixels at delta 109, purely from `style.css:1628`. Nothing in this suite is
          // about hovering: the contrast of a hover state is a question, but it is not the
          // question forty-two driven surfaces are answering, and it must not be answered by
          // accident on some of them. See `lib/harness.js`'s note above `parkPointer`.
          const s = await shot(page, "contrast-" + surface.name, { fullPage: false, parkPointer: true });
          publishShotFile(s.file, path.join(SHOTS, surface.name + ".png"));
        }
        await page.close();
        perSurface[surface.name + "/" + theme] = out;
        let pathCount = 0;
        for (const c of textPaths(out).values()) pathCount += c;
        assert(
          pathCount === out.nodesChecked,
          surface.name + "/" + theme + ": " + pathCount + " measured text path(s) for " + out.nodesChecked +
            " measured node(s). The floor below counts the paths, so they must be the same nodes"
        );
        if (surface.name !== "shell") checkedOver += measuredOver(out, perSurface["shell/" + theme]);

        checked += out.nodesChecked;
        considered += out.nodesConsidered;
        seen.canvas += out.canvasCount;
        seen.canvasInHome += out.canvasInHome || 0;
        seen.canvasInPhone += out.canvasInPhone || 0;
        seen.canvasSpeckle += out.canvasSpeckle || 0;
        seen.canvasResources += out.canvasResources || 0;
        seen.svgText += out.svgTextCount;
        seen.totals.considered += out.nodesConsidered;
        seen.totals.checked += out.nodesChecked;
        seen.totals.passed += out.nodesPassed;
        seen.totals.invisible += out.invisible;
        seen.totals.ancestor += out.ancestorResolved;
        seen.totals.veiled += out.veiled;
        seen.totals.obscured += Object.keys(out.obscured).length;
        seen.totals.indicators += out.indicators.considered;
        seen.totals.indicatorsChecked += out.indicators.checked;
        for (const k of Object.keys(out.obscured)) seen.obscured.add(out.obscured[k].path);
        for (const k of Object.keys(out.unresolvable)) seen.unresolvable.push(surface.name + "/" + theme + ": " + k);
        for (const k of Object.keys(out.exempt)) seen.exemptions.push(surface.name + "/" + theme + ": " + k);

        for (const [sig, f] of Object.entries(out.failures)) {
          seen.signatures.add(sig);
          seen.totals.failedNodes += f.nodes;
          const known = debt.signatures[sig];
          if (!known) {
            newOnes++;
            lines.push("    NEW (" + theme + ")\n" + describe(sig, f));
          } else if (f.ratio < known.ratio - 0.005) {
            worse++;
            lines.push("    WORSE than the ledger's " + known.ratio + ":1 (" + theme + ")\n" + describe(sig, f));
          } else {
            debtHits++;
          }
        }
        // Nodes measured somewhere, for check 11's cross-surface proof.
        for (const p of out.measuredPaths || []) seen.measured.add(p);

        // Text the stylesheets render, for check 16. Keyed by the string, because that is
        // what `lib/ui-sources.js` derives from the CSS and what the join has to hold.
        const g = out.generated || { considered: 0, checked: 0, measured: [], hidden: {}, unprovable: {} };
        seen.generated.considered += g.considered;
        seen.generated.checked += g.checked;
        for (const m of g.measured || []) {
          if (!seen.generated.measured.has(m.text)) seen.generated.measured.set(m.text, []);
          seen.generated.measured.get(m.text).push(Object.assign({ where: surface.name + "/" + theme }, m));
        }
        for (const k of Object.keys(g.hidden || {})) {
          const h = g.hidden[k];
          if (!seen.generated.hidden.has(h.text)) seen.generated.hidden.set(h.text, []);
          seen.generated.hidden.get(h.text).push(surface.name + "/" + theme + " " + h.path + " (" + h.why + ")");
        }
        for (const k of Object.keys(g.unprovable || {})) {
          seen.generated.unprovable.push(surface.name + "/" + theme + ": " + k + " — " + g.unprovable[k].why);
        }
      }
      // THE FLOOR, borrowed wholesale from run.js's `observed >= declared`. A driver whose
      // selector stops matching would otherwise put the app into a thinner state and report
      // a cleaner walk — the failure mode where "nobody checked" reads as "somebody checked
      // and it was fine". BOTH numbers are floored: `considered` catches a surface that
      // stopped rendering, `checked` catches one that still renders but became unmeasurable.
      //
      // ---------------------------------------------------------------------------------
      // AND EVERY FLOOR BUT THE SHELL'S IS WHAT THE SURFACE ADDS TO THE SHELL — 2026-10-19.
      //
      // Until this date the floors were counts of the WHOLE PAGE, and every surface is the
      // shell plus something. So text added to shared chrome raised the bare shell toward
      // every floor at once, check 9z (below) found the shell clearing the thinnest ones,
      // and a surface nobody had touched went red. On 2026-10-19 that happened twice in one
      // afternoon: an Oxford comma row and the "See what's new" link, both in the Settings
      // panel that ships hidden on every screen, each needed floors re-derived by hand on
      // surfaces they never appear on (552d1f4c8, 6e903d190). The compensation before this
      // one (2026-09-10, after commit 8397c530 shrank the shell and reddened two panels that
      // were painting perfectly) only handled a shell that SHRANK.
      //
      // So each floor is now the surface's own contribution, measured against the bare shell
      // walked earlier in THIS run, and text added to or removed from the shell cannot move it:
      //   considered  this surface's count minus the shell's. `considered` counts every text
      //               node in the DOM, hidden or not, and the shell's markup is in every
      //               surface's DOM, so the difference is exactly what the surface put there.
      //   checked     the measured nodes whose path the shell did not measure as often
      //               (`measuredOver`). A plain subtraction would be wrong here: a sheet that
      //               covers the shell does not measure the shell's new visible text, and
      //               would lose a node of its floor for every node the shell gained.
      // The shell's own floor stays a whole-page count: it is the anchor, and it is what
      // fails if the app paints nothing at all.
      //
      // WHAT IT STILL CATCHES. A driver that stops reaching its state leaves the bare shell
      // behind, which adds 0 and 0, and every floor not declared in `cannotBite` is above
      // the tolerance. Check 9z proves that against this run's own shell every time.
      // ---------------------------------------------------------------------------------
      if (surface.name === "shell") shellNow = { checked: checked, considered: considered };
      const consideredOver = considered - shellNow.considered;
      console.log("CONTRAST_COUNTS " + JSON.stringify({surface: surface.name, checked, considered, checkedOver, consideredOver, panels: reachedPanels}));
      const floor = debt.surfaceFloors[surface.name];
      assert(floor !== undefined, surface.name + " has no floor in contrast-debt.json — add one rather than letting a surface that stops rendering read as clean");
      // TOP-LEVEL, NOT INSIDE `surfaceFloors`: check 10b reconciles the walk inventory
      // against `Object.keys(debt.surfaceFloors)` and every key there has to be a surface.
      const TOL = debt.floorReference.tolerance;
      assert(typeof TOL === "number", "contrast-debt.json has no `floorReference.tolerance`");
      if (surface.name === "shell") {
        assert(
          considered >= floor.considered - TOL && checked >= floor.checked - TOL,
          "the bare shell measured " + checked + " of " + considered + " node(s) across both themes against a floor of " +
            floor.checked + " of " + floor.considered + " (plus " + TOL + " tolerance) — the app is not painting " +
            "what it painted when the floor was set, and every other floor below is relative to it"
        );
      } else {
        assert(
          typeof floor.consideredOverShell === "number" && typeof floor.checkedOverShell === "number",
          surface.name + "'s floor has no `consideredOverShell` and `checkedOverShell`. Floors are what the " +
            "surface adds to the bare shell, read off this run's CONTRAST_COUNTS line, never a whole-page count"
        );
        assert(
          consideredOver >= floor.consideredOverShell - TOL,
          surface.name + " added only " + consideredOver + " text node(s) to the bare shell across both themes but " +
            "its floor is " + floor.consideredOverShell + " (plus " + TOL + " tolerance) — the driver did not reach " +
            "the state it was written for, so a clean result here proves nothing"
        );
        assert(
          checkedOver >= floor.checkedOverShell - TOL,
          surface.name + " measured only " + checkedOver + " node(s) the bare shell does not, across both themes, " +
            "but its floor is " + floor.checkedOverShell + " (plus " + TOL + " tolerance) — the surface still " +
            "renders, but less of it can be measured than when the floor was set"
        );
      }
      assert(
        newOnes === 0 && worse === 0,
        newOnes + " NEW and " + worse + " WORSENED contrast failure(s) on " + surface.name + ":\n" + lines.join("\n")
      );
      return checked + " of " + considered + " node(s) measured across both themes, " + debtHits +
        " known-debt hit(s), 0 new, 0 worsened" + (curtain ? " — " + curtain : "");
    });
  }

  // ---- 9z. the floors are proven able to fail --------------------------------------------

  await run.check("9z  NEGATIVE CONTROL: a surface that collapsed to the bare shell FAILS its floor", async () => {
    // THE CONTROL THE FLOORS NEVER HAD, and the reason they were worth nothing for a year
    // before they were worth too much for four days.
    //
    // A floor is a claim that a driver reaching a thinner state than the one it was written
    // for gets caught. Nobody had ever checked that claim, and it was FALSE for most of this
    // list: `opening-screen` sat at 0 of 228 against an actual 0 of 412, so its driver could
    // have failed completely and cleared its floor by 184 nodes. `search` had 208 to spare.
    // Those are not floors, they are decorations — and a decoration is worse than nothing,
    // because it reads like a guard in a diff.
    //
    // What a failed driver actually leaves behind is the shell it opened and never changed.
    // So that is what is tested, against every floor at once, scored exactly as check 9
    // scores a surface: the shell measured in THIS run, put through the same subtraction and
    // the same `measuredOver`, so this re-proves itself against the real app every time
    // rather than against numbers somebody typed. Only the declared tolerance is conceded.
    //
    // Since 2026-10-19 the floors are what a surface ADDS to the shell (see the note at
    // check 9's floor), so a bare shell scores 0 and 0 whatever text the shell carries, and
    // new shell text can no longer turn this check red on a surface nobody touched.
    //
    // AND SOME OF THEM CANNOT FAIL, WHICH THIS CONTROL FOUND AND WHICH IS DECLARED RATHER
    // THAN QUIETLY EXCLUDED: a surface whose only change is stylesheet-generated content, or
    // that adds fewer nodes than the tolerance, is indistinguishable from "driver did
    // nothing" by any count. Saying so is the only honest option: a floor listed as a guard
    // that cannot fail is exactly the "nobody checked" that reads as "somebody checked".
    //
    // Each one names what proves its driver arrived INSTEAD, in `floorReference.cannotBite`,
    // and the set is asserted EXACTLY — a new surface that cannot bite has to be declared,
    // and a declared one that starts biting has to come off the list.
    const ref = debt.floorReference;
    let bareChecked = 0;
    for (const theme of THEMES) bareChecked += measuredOver(perSurface["shell/" + theme], perSurface["shell/" + theme]);
    const bareConsidered = shellNow.considered - shellNow.considered;
    const survivors = [];
    for (const surface of SURFACES) {
      if (surface.name === "shell") continue;
      const floor = debt.surfaceFloors[surface.name];
      // Would the bare shell — this run's own — have cleared this surface's floor?
      if (
        bareConsidered >= floor.consideredOverShell - ref.tolerance &&
        bareChecked >= floor.checkedOverShell - ref.tolerance
      ) {
        survivors.push(surface.name);
      }
    }
    const declared = Object.keys(ref.cannotBite || {}).sort();
    assertEqual(
      survivors.filter((n) => !declared.includes(n)),
      [],
      "these surfaces' floors would be cleared by the BARE SHELL, so a driver that never " +
        "reached its state would pass them. Either re-derive the floor from a run that " +
        "actually opened the surface, or — if the surface genuinely adds no text node to the " +
        "DOM — declare it in `floorReference.cannotBite` and name what proves its driver " +
        "arrived instead"
    );
    assertEqual(
      declared.filter((n) => !survivors.includes(n)),
      [],
      "a surface is declared unable to fail its floor and its floor CAN now fail. Take it " +
        "off `cannotBite` — a stale exemption is a guard nobody is getting"
    );
    for (const n of declared) {
      assert(
        typeof ref.cannotBite[n] === "string" && ref.cannotBite[n].length > 40,
        n + " is declared in `cannotBite` with no reason. The declaration IS the check here"
      );
    }
    return (
      SURFACES.length - 1 - declared.length + " surface floor(s) proven able to fail: the " +
      "bare shell measured in this run (" + shellNow.checked + " of " + shellNow.considered +
      ") adds " + bareChecked + " measured and " + bareConsidered + " considered node(s) to itself, " +
      "below every one of them, tolerance " + ref.tolerance + " included. " +
      declared.length + " declared unable to, each naming what proves its driver arrived " +
      "instead: " + declared.join(", ")
    );
  });

  // ---- 10. the dark run is not a fiction --------------------------------------------------

  await run.check("10  the second theme is a real second theme, or is reported as not one", async () => {
    // THIS CHECK CHANGED SHAPE ON 2026-08-31, AND THE OLD SHAPE IS WHY.
    //
    // It used to count `@media (prefers-color-scheme: dark)` blocks and, finding none,
    // assert the two runs were IDENTICAL — which was true and honest while the app shipped
    // one palette. Then dark mode landed, and it landed as `:root[data-theme="light"]`
    // driven by a stored preference rather than by a media query. The shipped DEFAULT for
    // that preference was dark (§15) until 2026-09-19 and is `system` now (§63), which
    // changes nothing about the mechanism this check counts and everything about the
    // fresh-install walk at the bottom of it.
    //
    // Left alone, this check would have kept passing — zero `prefers-color-scheme` blocks,
    // two identical runs — and would have kept PRINTING "THE APP SHIPS ONE THEME" over an
    // app that shipped two. A green assertion attached to a false sentence is the worst
    // outcome available here: it is the "nobody checked" that reads as "somebody checked".
    //
    // So it now counts the mechanism that actually ships, and it counts BOTH, because
    // either is a legitimate way to have a second theme and a build that switched from one
    // to the other must not slip through the gap between them.
    const css = CSS_FILES.map((f) => fs.readFileSync(f, "utf8")).join("\n");
    const mediaBlocks = (css.match(/@media[^{]*prefers-color-scheme\s*:\s*dark/g) || []).length;
    const attrBlocks = (css.match(/:root\s*\[\s*data-theme\s*=/g) || []).length;
    const themed = mediaBlocks + attrBlocks;

    assert(
      perSurface["shell/light"] && perSurface["shell/dark"],
      "check 9 must have run both themes before this one can compare them"
    );

    // WHAT THIS CHECK COMPARES, AND WHY IT IS NO LONGER THE FAILURE SIGNATURES.
    //
    // The original compared the two runs' sets of FAILING colour pairings and asserted they
    // differed. That was a serviceable proxy while the app had 456 failures — two palettes
    // fail differently. It has one fatal property, and this build hit it within the hour:
    // WHEN THE APP REACHES ZERO FAILURES, BOTH SETS ARE EMPTY, THEREFORE IDENTICAL,
    // THEREFORE THIS CHECK FAILS — and it fails hardest exactly when the work is most
    // correct, which trains whoever is on the other end to disable it.
    //
    // Fixing the contrast debt must not be what breaks the theme check. So the proof is now
    // taken from the PIXELS: the shell's own painted background, read out of both walks.
    // That is evidence a clean app still produces, and it is closer to the thing being
    // claimed anyway — "there are two palettes" is a statement about colours, not failures.
    // READ OFF THE ELEMENT THAT PAINTS THE GROUND, which stopped being `body` on 2026-09-29:
    // the speckled ground needs `body` transparent and keeps the ruled ground on `html`
    // (style.css "THE SPECKLED GROUND"). Reading `body` there gave rgba(0, 0, 0, 0) in both
    // themes and failed this check on a transparent layer, not on the theme.
    // `lib/harness.js` `paintedGround` says why it is not simply `html` by name.
    const grounds = {};
    for (const t of THEMES) {
      const page = await openApp(browser, t, false);
      grounds[t] = await paintedGround(page);
      await page.close();
    }
    if (themed === 0) {
      assert(
        grounds.light === grounds.dark,
        "the shipped CSS declares no second-theme rule of either kind, yet the two runs paint " +
          "different grounds (" + grounds.dark + " vs " + grounds.light + ") — something IS " +
          "responding to the theme and this note is now wrong"
      );
      return "0 second-theme rules in " + SOURCES.styleSources().join(" + ") + ". THE APP SHIPS ONE THEME.";
    }
    assert(
      grounds.light !== grounds.dark,
      themed + " second-theme rule(s) are shipped, but both walks paint the same ground (" +
        grounds.dark + ") — the theme is not reaching the elements, so half of every check above " +
        "is a fiction"
    );
    // AND A FRESH INSTALL FOLLOWS THE OPERATING SYSTEM — §63, 2026-09-19, and it takes BOTH
    // walks to say so. This assertion read `virgin === "dark"` under a light OS until that
    // date, which was §15 and was right; its replacement cannot be "dark under a dark OS",
    // because that is also what a build ignoring the OS entirely produces. The claim is that
    // the two answers DIFFER and each matches its host, and only the pair can carry it.
    //
    // AND IT HAS TO LEAVE THE HOME SCREEN FIRST, WHICH THE OLD WALK DID NOT. `.nav-thread`
    // is attached while the home screen is still up, and the home screen is CLAMPED DARK by
    // §15 — a FORCE flag over the top of whatever the preference says. So this walk used to
    // sample the clamp and report it as the default, and its `assertEqual(virgin, "dark")`
    // was green for a reason that had nothing to do with §15's default: it would have stayed
    // green with the preference set to light. The §63 shape is what exposed it — the fixed
    // assertion came back ["dark","dark"] on a build that resolves light correctly
    // everywhere check 1 of appearance.js looks. `leaveHome` also clears the curtain, which
    // carries a clamp of its own.
    const virgins = {};
    for (const os of ["light", "dark"]) {
      const fresh = await browser.newPage({ viewport: { width: 1400, height: 950 }, colorScheme: os });
      await fresh.goto(APP);
      await leaveHome(fresh);
      await fresh.waitForSelector(".nav-thread", { state: "attached" });
      await fresh.waitForFunction(() => window.RichTheme && !window.RichTheme.forcedDark(), null, { timeout: 15000 });
      virgins[os] = await fresh.evaluate(() => document.documentElement.getAttribute("data-theme"));
      await fresh.close();
    }
    assertEqual(
      [virgins.light, virgins.dark],
      ["light", "dark"],
      "an install with NO stored preference must take its lighting from the operating system " +
        "— CEO ruling §63. It opened " + virgins.light + " under a light OS and " + virgins.dark +
        " under a dark one, so the OS is not deciding a question the ruling gives it"
    );

    return (
      themed + " second-theme rule(s) shipped (" + attrBlocks + " `:root[data-theme=]`, " +
      mediaBlocks + " `prefers-color-scheme`), and both walks exercise them: the two runs " +
      "differ, the painted ground differs (" + grounds.dark + " vs " + grounds.light + "), and a " +
      "fresh install follows the OS both ways (light OS -> " + virgins.light + ", dark OS -> " +
      virgins.dark + ")."
    );
  });

  // ---- 10b. this suite measures the same amount however it was started ---------------------

  await run.check("10b  every surface was walked in both themes — and standalone is not a thinner run", async () => {
    // RAISED BY THE LEAD ON 2026-08-31, and the concern is right even though the symptom
    // that prompted it was not: "a gate that silently degrades when run directly is a gate
    // someone will one day trust while it measures nothing."
    //
    // The answer is that it does not degrade, and this check is that answer in a form nobody
    // has to take on trust. `run.js` passes `RICHOS_UI_TESTS_LEDGER` to its children, and
    // that variable does exactly one thing (`lib/harness.js`): it decides whether `report()`
    // appends a line of evidence for `run.js` to gate on. `recordEvidence` returns early
    // without it. It reaches no driver, no walk and no threshold — the harness's own comment
    // has said so since it was written: "`node workers.js` on its own is byte-for-byte
    // unchanged."
    //
    // What this asserts is the thing that would actually be worth knowing: that the walk
    // INVENTORY is complete. A suite that reached two surfaces out of ten and reported a
    // clean sweep of both themes is the failure being guarded against, and it is a failure
    // about coverage rather than about an environment variable — so it is measured as one,
    // and it holds whichever way the suite was started.
    // THE EXPECTED LIST COMES FROM THE LEDGER ON DISK, NOT FROM `SURFACES`.
    //
    // The first draft of this check built both sides out of `SURFACES` and was therefore
    // incapable of failing: deleting a surface deleted it from the expectation too, and the
    // mutation that was supposed to prove the check sailed through it. Both sides have to be
    // read off disk or neither is. `contrast-debt.json`'s `surfaceFloors` is the second
    // source — it is committed, it is what check 9's floors are already gated on, and a
    // surface removed from the driver list is still named there.
    const named = Object.keys(debt.surfaceFloors).sort();
    const walkedNames = [...new Set(Object.keys(perSurface).map((k) => k.split("/")[0]))].sort();
    assertEqual(
      named.filter((n) => !walkedNames.includes(n)),
      [],
      "these surfaces have a floor in contrast-debt.json and were never walked. A driver list " +
        "that quietly got shorter still runs every check it declares, so run.js's " +
        "declared-vs-observed gate cannot see it — this is the only thing that can"
    );
    const expected = [];
    for (const n of walkedNames) for (const t of THEMES) expected.push(n + "/" + t);
    assertEqual(
      expected.filter((k) => !perSurface[k]),
      [],
      "a surface was walked in one theme and not the other"
    );
    assertEqual(
      Object.keys(perSurface).length,
      named.length * THEMES.length,
      "the walk inventory and the committed floor list disagree"
    );
    return (
      Object.keys(perSurface).length + " walks completed (" + named.length + " surfaces x " + THEMES.length +
      " themes), and the count is the same started directly or under run.js: " +
      "RICHOS_UI_TESTS_LEDGER gates only whether evidence is APPENDED, never what is measured."
    );
  });

  // ---- 10c. no panel in the shell is unwalked without being written down --------------------

  await run.check("10d  the coverage control still runs when no panel is left unwalked", async () => {
    const { coverageControl } = require("./lib/panel-control");
    const full = coverageControl(["a", "b"], new Map([["a", 1], ["b", 1]]), new Map());
    assert(full.victim, "complete coverage: the control found no panel to drop");
    assertEqual(full.named, [full.victim], "complete coverage: the comparator did not name the dropped panel");
    const debt = coverageControl(["a", "b"], new Map([["a", 1]]), new Map([["b", "reason"]]));
    assertEqual(debt.named, ["b"], "with coverage debt the comparator did not name the dropped panel");
    return "the control runs with and without coverage debt";
  });

  await run.check("10c  every panel the shell declares is walked, or is declared unwalked with a reason", async () => {
    // 10b catches a driver list that got SHORTER. This catches one that went STALE, which is
    // the failure that actually happened elsewhere in this directory today: a hand-maintained
    // inventory that was right when it was written and that the product grew past. The
    // driver list is the last hand-maintained list in this file and it cannot be derived —
    // a driver is a function that knows how to open a thing — so what is derived instead is
    // the QUESTION it has to answer: here are the panels the shell declares; which of them
    // did nothing reach?
    //
    // A declared gap is fine and there are seven of them. An UNDECLARED gap is a surface the
    // CEO can open that this gate says nothing about while printing a total that sounds
    // complete.
    assert(
      PANELS.length >= 15,
      "the panel probe found " + PANELS.length + " panels in the shell — that is not this " +
        "shell, and a short inventory would make this check pass by having nothing to ask about"
    );

    const declaredUnwalked = new Map((debt.unwalkedPanels || []).map((p) => [p.id, p.why]));
    const reached = [...seen.panelsReached.keys()].sort();
    const unreached = PANELS.filter((id) => !seen.panelsReached.has(id));

    assertEqual(
      unreached.filter((id) => !declaredUnwalked.has(id)),
      [],
      "these panel(s) are in the shipped shell and NO surface driver opens them, so nothing " +
        "in this file has measured a pixel of them. Either add a driver to SURFACES, or add " +
        "them to contrast-debt.json's `unwalkedPanels` with the reason — but they are NOT " +
        "covered, and the entry is what says so where a reviewer reads it."
    );

    // ...and the declaration cannot go stale in the other direction either.
    const nowWalked = [...declaredUnwalked.keys()].filter((id) => seen.panelsReached.has(id));
    assertEqual(
      nowWalked,
      [],
      "contrast-debt.json declares these panels unwalked and a driver now reaches them. " +
        "Delete the entries — a standing admission of a gap that has been closed teaches " +
        "whoever reads this list to skim it."
    );
    const gone = [...declaredUnwalked.keys()].filter((id) => PANELS.indexOf(id) < 0);
    assertEqual(
      gone,
      [],
      "contrast-debt.json declares a panel unwalked that the shell no longer has: " + gone.join(", ")
    );

    for (const [id, why] of declaredUnwalked) {
      assert(
        typeof why === "string" && why.length >= 20,
        id + " is declared unwalked with no real reason. A gap that is written down is a " +
          "bounded gap; a gap written down as \"\" is the same gap with a tick beside it."
      );
    }

    // POSITIVE CONTROL, on the comparator this check is: drop one declaration and it must
    // name exactly that panel. Without this the check is only ever OBSERVED passing, and a
    // comparator built out of one set — the failure 10b's own comment records — passes by
    // construction. Run against a copy; the real declaration is untouched.
    const control = require("./lib/panel-control").coverageControl(PANELS, seen.panelsReached, declaredUnwalked);
    assertEqual(
      control.named,
      [control.victim],
      "the comparator did not notice a panel that is neither walked nor declared"
    );

    return (
      reached.length + " of " + PANELS.length + " declared panel(s) opened by a driver and " +
      "walked in both themes; " + unreached.length + " declared unwalked with a reason:\n          " +
      unreached.map((id) => "#" + id + " — " + declaredUnwalked.get(id)).join("\n          ")
    );
  });

  // ---- 11. the obscured bucket is not a hiding place ---------------------------------------

  await run.check("11  nothing disappears into `obscured` without being measured somewhere else", async () => {
    const measured = new Set();
    for (const out of Object.values(perSurface)) {
      for (const f of Object.values(out.failures)) measured.add(f.selector);
      for (const p of out.measuredPaths || []) measured.add(p);
    }
    const orphans = [...seen.obscured].filter((p) => !measured.has(p));
    assertEqual(
      orphans,
      [],
      "these node(s) were filed `obscured` on every surface that reaches them and measured on none, which " +
        "means the gate says nothing about them at all:\n      " + orphans.join("\n      ")
    );
    return seen.obscured.size + " node path(s) obscured behind a modal on some surface, every one of them measured on another";
  });

  // ---- 11b. text the STYLESHEET renders, joined source-side to walk-side --------------------

  await run.check("11b  every string the stylesheets themselves render is measured, both themes", async () => {
    // WHAT THIS CLOSES. `lib/contrast.js`'s walk collected `nodeType === 3` children, and a
    // pseudo-element is not a node — so text produced by a `content:` declaration was
    // outside every check in this file, was not on the library header's "WHAT IT DOES NOT
    // SEE" list, and therefore did not exist as a known gap either. `style.css:3787` renders
    // one: `.setbtn::after { content: "Settings" }`, the tooltip on the settings button §15
    // puts on every screen. It had never been measured by anything.
    //
    // TWO SIDES, DERIVED SEPARATELY, JOINED HERE. `lib/ui-sources.js`'s
    // `cssContentStrings()` reads the SHIPPED STYLESHEETS — all four, from the manifest — and
    // returns every non-empty authored `content` string with its file:line. The walk reports
    // what the BROWSER produced. Neither side can be short without the other noticing:
    //
    //   a string in the CSS that no walk ever saw    -> FAILS, naming file:line
    //   a string a walk saw that is in no stylesheet -> FAILS, naming the selector
    //
    // The second direction is not decoration. It is what would catch a `content` written by
    // a stylesheet the manifest does not reach — the exact defect this whole session is
    // about, one level down.
    const authored = SOURCES.cssContentStrings();
    assert(
      authored.length >= 1,
      "0 authored `content` strings found across " + SOURCES.styleSources().join(" + ") +
        ". This check is a join, and a join over an empty set passes for free — which is the " +
        "failure this repository has found eleven times today. Either the derivation broke or " +
        "the stylesheets moved."
    );
    assert(
      seen.generated.considered > 0,
      "the walk found 0 generated-content nodes across " + Object.keys(perSurface).length +
        " walks while the CSS declares " + authored.length + ". The in-page pass is not running."
    );

    // SOURCE -> SCREEN. Measured, or at worst seen-and-named.
    const unseen = authored.filter(
      (a) => !seen.generated.measured.has(a.text) && !seen.generated.hidden.has(a.text)
    );
    assertEqual(
      unseen.map((a) => a.site + " = " + JSON.stringify(a.text)),
      [],
      "authored `content` string(s) that NO walk in this suite ever encountered"
    );

    // SCREEN -> SOURCE. Anything the browser rendered has to come from a stylesheet the
    // manifest reaches.
    const authoredText = new Set(authored.map((a) => a.text));
    const foreign = [...seen.generated.measured.keys(), ...seen.generated.hidden.keys()].filter(
      (t) => !authoredText.has(t)
    );
    assertEqual(
      foreign.map((t) => JSON.stringify(t)),
      [],
      "generated text on screen that is in none of the shipped stylesheets — a `content` " +
        "declaration is reaching the app from a file lib/ui-sources.js does not reach"
    );

    // AND UNPROVABLE IS A FAILURE, never a skip — the rule the rest of this file runs on.
    assertEqual(
      seen.generated.unprovable,
      [],
      "generated text that is on screen and could not be resolved"
    );

    // MEASURED IN BOTH THEMES, at the same floors as any other text. `hidden` alone is not
    // enough for anything: a string that is `opacity: 0` on every surface has been named,
    // not checked, and this is the assertion that says so out loud.
    const lines = [];
    for (const a of authored) {
      const rows = seen.generated.measured.get(a.text) || [];
      const themes = new Set(rows.map((r) => r.where.split("/")[1]));
      assert(
        themes.has("light") && themes.has("dark"),
        a.site + " = " + JSON.stringify(a.text) + " was measured in " +
          (rows.length ? [...themes].join(" + ") : "no theme") +
          ". It is rendered text and the floor is both themes; the surfaces that report it " +
          "hidden are: " + (seen.generated.hidden.get(a.text) || ["(none)"]).slice(0, 2).join(", ")
      );
      for (const r of rows) {
        assert(
          r.ratio >= r.threshold,
          a.site + " " + r.where + " " + r.path + ": " + r.ratio + ":1 against a floor of " + r.threshold + ":1"
        );
      }
      const worst = rows.reduce((w, r) => (w === null || r.ratio < w.ratio ? r : w), null);
      lines.push(
        a.site + " " + JSON.stringify(a.text) + " — worst " + worst.ratio + ":1 (floor " +
          worst.threshold + ":1) " + worst.fg + " on " + worst.bg + " at " + worst.fontSize + "px on " +
          worst.where + ", via " + worst.via
      );
    }

    return (
      authored.length + " authored `content` string(s) across " + SOURCES.styleSources().join(" + ") +
      "; " + seen.generated.considered + " generated node(s) considered and " + seen.generated.checked +
      " measured across " + Object.keys(perSurface).length + " walks, 0 unprovable:\n          " +
      lines.join("\n          ")
    );
  });

  // ---- 12. the exemption inventory, so creep is a number ------------------------------------

  await run.check("12  every exemption in the shipped source is enumerated, with its reason", async () => {
    const declared = [];
    for (const file of SOURCE_FILES) {
      const src = fs.readFileSync(file, "utf8");
      const re = /data-contrast-exempt\s*=\s*(["'])([\s\S]*?)\1/g;
      let m;
      while ((m = re.exec(src))) declared.push({ file: path.basename(file), reason: m[2].trim() });
    }
    for (const d of declared) {
      assert(
        d.reason.length >= 12,
        d.file + " declares data-contrast-exempt=\"" + d.reason + "\" — an exemption has to say what it is claiming, " +
          "because the claim is the thing a reviewer weighs. Twelve characters is not a high bar; a blank is a mute button."
      );
    }
    assert(
      declared.length <= debt.exemptionCap,
      declared.length + " exemptions are declared in the shipped source; the ledger's cap is " + debt.exemptionCap +
        ". Raise the cap deliberately, in the same commit that adds the exemption, so the growth is a decision."
    );
    if (!declared.length) return "0 exemptions declared in the shipped source today (cap " + debt.exemptionCap + "). Nothing is currently excused.";
    return declared.length + " declared (cap " + debt.exemptionCap + "):\n          " +
      declared.map((d) => d.file + ": " + d.reason).join("\n          ");
  });

  // ---- 13. the ledger only shrinks ---------------------------------------------------------

  await run.check("13  the debt ledger can only shrink, and every entry in it is still real", async () => {
    const ledger = Object.keys(debt.signatures);
    assert(
      ledger.length <= debt.cap,
      ledger.length + " signatures in contrast-debt.json against a cap of " + debt.cap +
        " — the ledger is for what was already broken, not a place to put new work"
    );
    const gone = ledger.filter((s) => !seen.signatures.has(s));
    const noted = gone.length
      ? "\n          " + gone.length + " ledger entr(ies) no longer reachable — fixed, or the surface changed. Delete them:\n          " +
        gone.map((g) => "  " + g).join("\n          ")
      : "";
    // Deliberately a NOTE and not a failure: a coverage GAIN must never turn a run red, which
    // is the same reasoning run.js applies to an --allow-skip that was not needed. The cap
    // above is what stops the ledger drifting the other way.
    // Unresolvable has its OWN ledger, and it is the shortest and most closely watched list
    // in this file: every entry is a place the gate is admitting it cannot prove anything.
    // A new one is a failure. Nothing is ever "probably fine".
    const known = new Set(debt.knownUnresolvable.map((k) => k.key));
    const surprises = [...new Set(seen.unresolvable.map((u) => u.split(": ").slice(1).join(": ")))].filter((k) => !known.has(k));
    assertEqual(
      surprises,
      [],
      "colour(s) on the shipping surfaces that this gate cannot prove, and that nobody has written down:\n      " +
        surprises.join("\n      ") +
        "\n      Either resolve them in the product, or add them to contrast-debt.json's knownUnresolvable with a " +
        "reason — but they are NOT passing, and the entry says so."
    );
    return ledger.length + " known-debt signature(s) (cap " + debt.cap + "), " + seen.signatures.size +
      " seen this run; " + debt.knownUnresolvable.length + " declared-unprovable colour(s), 0 new" + noted;
  });

  // ---- 14. what a DOM checker cannot see, asserted rather than assumed ------------------------

  await run.check("14  no UNMEASURED <canvas> on any surface walked — the DOM check is not talking past the pixels", async () => {
    assertEqual(
      seen.canvas,
      0,
      "a <canvas> appeared on a walked surface OUTSIDE the home screen. Nothing in this suite can read a " +
        "pixel it painted, so its contrast is UNCHECKED and a green run here must not be read as covering " +
        "it. If it belongs to a surface that IS measured from the pixels somewhere, exclude it here BY " +
        "NAME and say where — never by raising a threshold."
    );
    return (
      "0 unmeasured <canvas> elements across " + Object.keys(perSurface).length + " surface/theme walks. " +
      "The DOM check therefore covers the whole of every surface it reaches — every failure it reports " +
      "is a node with a computed style, and every node with a computed style was reported.\n          " +
      seen.canvasInPhone + " <canvas> element(s) inside #phone-sheet were EXCLUDED and ARE covered, " +
      "elsewhere: it is the pairing QR code, and `tests/phone.js` measures it FROM THE " +
      "PIXELS in this same WebKit — only the #000000 modules and the #ffffff quiet zone this shell " +
      "paints are present, which is 21:1, and the quiet zone is white on all four edges, which is " +
      "what makes a code scan on a dark page.\n          " +
      seen.canvasSpeckle + " speckled-ground <canvas> element(s) (`body > canvas.speckle`) were EXCLUDED and ARE " +
      "covered, elsewhere: it carries no text, it sits behind the shell, and `tests/speckled-ground.js` proves every " +
      "text node drawn over it keeps 4.5:1 against the strongest point the engine can paint.\n          " +
      seen.canvasResources + " <canvas> element(s) of the Mac resources square and its card (`#res-square`, " +
      "`#res-card`) were EXCLUDED and ARE covered, elsewhere: `tests/resources.js` reads both back with " +
      "`getImageData` and holds every paint to 3:1 against the ground it is drawn on, in both themes.\n          " +
      seen.canvasInHome + " <canvas> element(s) inside #home were EXCLUDED and are not covered here: the " +
      "home screen is round-11.1/v1 \"Constellation\", a canvas composition the CEO chose, and it is " +
      "measured from the pixels by tests/home.js instead — 16 elements, worst 4.23:1, on the rendered " +
      "frame. Canvas-heavy work elsewhere in the repo (the round-7 / round-8.1 material studies) is NOT " +
      "covered by anything, here or there."
    );
  });

  // ---- 15. SVG is named, not silently skipped -------------------------------------------------

  await run.check("15  SVG fill/stroke is counted as uncovered, not passed over in silence", async () => {
    const page = await openApp(browser, "light");
    const splash = await page.evaluate(() => {
      const n = document.getElementById("splash");
      if (!n) return { present: false };
      const texts = [];
      n.querySelectorAll("*").forEach((e) => {
        for (const c of e.childNodes) if (c.nodeType === 3 && c.nodeValue.trim()) texts.push(c.nodeValue.trim());
      });
      return { present: true, htmlTextNodes: texts.length, svgs: n.querySelectorAll("svg").length };
    });
    await page.close();
    assertEqual(
      seen.svgText,
      0,
      seen.svgText + " <svg><text> node(s) were found on the walked surfaces. This walk reads `color`, " +
        "not `fill`, so those are UNMEASURED and must not be read as passing."
    );
    return (
      "0 svg <text> nodes on the walked surfaces. The opening screen is a separate matter and is named here " +
      "rather than counted as covered: it is " +
      (splash.present ? "still up at walk time" : "already gone by walk time (its curtain yields as soon as main.js reports the shell usable)") +
      ", and it is drawn almost entirely in SVG — its wordmark, logo and plinth carry no computed `color` this walk can read."
    );
  });

  await run.check("16  every radio paints its OWN indicator — the platform's measured 1.63:1 and this walk could not see it", async () => {
    // AUDIT-10 ROW 3, AND THE REASON IT SURVIVED THIRTY-NINE GREEN SURFACES ABOVE.
    //
    // Nine radios ship in this shell — assertiveness x3, retention x3, §7.1's scope x3 — and
    // until 2026-09-18 every one of them was a bare native `<input type="radio">` with no CSS.
    // The indicator walk in `lib/contrast.js` DOES reach them (`input` is in `CONTROLS`, and
    // `FIELD` is true, so the visible-label shortcut never applies). It then reads
    // `backgroundColor: rgba(0,0,0,0)` and `borderWidth: 0px`, because the platform paints the
    // ring and the stylesheet says nothing at all — and files them `textOnly`, the bucket whose
    // own comment calls itself "the honest size of this section's blind spot". So the blind spot
    // was real, it was declared, and nobody had looked in it. Ray did, with a camera:
    //
    //     LIGHT unselected  #c9c8c8 on #fdfcf8   1.63:1   against a 3:1 floor
    //     DARK  unselected  #ffffff on #182440  15.38:1 — and the two he had NOT chosen were
    //                                           SOLID WHITE DISCS while the selected one was a
    //                                           ring with a small blue dot, so at a glance the
    //                                           wrong two looked chosen
    //
    // THIS CHECK IS DELIBERATELY PIXEL-BASED, not computed-style, and that is the point rather
    // than a flourish. A computed-style assertion is satisfied by any stylesheet that declares
    // SOMETHING; the defect it has to catch is a control whose paint comes from the platform, and
    // the platform's paint has no computed style to read. Only the rendered frame can tell those
    // two apart. It is the same instrument Ray used, so a regression fails here before it reaches
    // a walk.
    //
    // AND IT CHECKS THE MEANING, NOT ONLY THE RATIO. The dark row above passes 15.38:1 while
    // being the worse defect, so a ratio alone would have called it fine. What carries the state
    // is FILL: the selected radio is a filled accent disc and an unselected one is an empty ring,
    // which is a difference in how much ink is on the control and survives grayscale. The two
    // tokens are 1.68:1 (dark) / 1.06:1 (light) apart by hue — measured, and recorded in
    // `style.css` beside the rule — so hue can never be allowed to be the distinction.
    const RADIO_FLOOR = 3;
    const lines = [];
    for (const theme of ["light", "dark"]) {
      const page = await openApp(browser, theme);
      await page.click('.nav-thread[data-thread-id="hiring"]');
      await settleOnThread(page, "hiring");
      await page.click("#set-btn");
      await page.waitForSelector("#set-menu:not([hidden]) #set-techy");
      await page.check("#set-techy");
      await page.waitForSelector("#techy-scope:not([hidden])");
      await awaitSettled(page);

      const boxes = await page.evaluate(() => {
        const panel = document.querySelector("#techy-scope .overlay-panel");
        return {
          ground: getComputedStyle(panel).backgroundColor,
          radios: [...document.querySelectorAll('#techy-scope input[name="techy-scope"]')].map((i) => {
            const r = i.getBoundingClientRect();
            return { value: i.value, checked: i.checked, x: r.x, y: r.y, w: r.width, h: r.height };
          }),
        };
      });
      assertEqual(boxes.radios.length, 3, "the scope sheet did not put three radios on screen");

      const measured = [];
      for (const r of boxes.radios) {
        // A three-pixel skirt, so the crop carries the panel the ring is drawn on as well as the
        // ring. The ground is then read from the skirt (the most common border value) rather than
        // assumed, exactly as the surface walks resolve a ground from the paint stack.
        // A radio is a non-text indicator: shot on the plain ground (`onPlainGround`). The scope
        // sheet's panel is opaque, so today this changes no pixel; it keeps the rule uniform.
        const buf = await onPlainGround(page, () => page.screenshot({
          clip: { x: Math.floor(r.x - 3), y: Math.floor(r.y - 3), width: Math.ceil(r.w + 6), height: Math.ceil(r.h + 6) },
        }));
        measured.push(Object.assign({ theme: theme }, C.measureIndicatorCrop(buf), { value: r.value, checked: r.checked }));
      }

      for (const m of measured) {
        assert(
          m.ratio >= RADIO_FLOOR,
          theme + ": the " + (m.checked ? "selected" : "unselected") + " radio (" + m.value + ") paints " +
            m.ink + " on " + m.ground + " = " + m.ratio + ":1, under the " + RADIO_FLOOR +
            ":1 floor a non-text indicator owes. This is the shape audit-10 row 3 measured at 1.63:1."
        );
      }

      // THE FILL TEST. `ink` is the strongest value on the control and `inkSamples` is how many
      // of it there are, so a FILLED disc carries strictly more of its own ink than an EMPTY ring
      // of the same diameter does. On the shipped control today: selected 72 / 64 samples,
      // unselected 32. On the platform's radios it was the other way round — unselected 38,
      // selected 12 — which is precisely Ray's "the wrong two look chosen", and it is what this
      // assertion refuses.
      const chosen = measured.find((m) => m.checked);
      const others = measured.filter((m) => !m.checked);
      assert(chosen, theme + ": no radio is selected on a sheet whose first option is preselected");
      for (const o of others) {
        assert(
          chosen.inkSamples > o.inkSamples,
          theme + ": the SELECTED radio carries " + chosen.inkSamples + " samples of its own ink and the " +
            "unselected one (" + o.value + ") carries " + o.inkSamples + ". The unselected control is at " +
            "least as filled as the chosen one, so at a glance the wrong option looks chosen — which is " +
            "audit-10 row 3's dark-mode half, and it passed its RATIO at 15.38:1."
        );
      }
      lines.push(
        theme + ": selected " + chosen.ratio + ":1 (" + chosen.inkSamples + " samples) vs unselected " +
          others.map((o) => o.ratio + ":1 (" + o.inkSamples + ")").join(", ")
      );
      await page.close();
    }
    return "floor " + RADIO_FLOOR + ":1, measured off the painted control — " + lines.join("; ");
  });

  await run.check("16b  every checkbox paints its own indicator too — the half check 16 did not cover", async () => {
    // RAY'S CANDIDATE .11 WALK, DEFECT 3.6. Check 16 above fixed the radios and this file said
    // nothing about the checkboxes beside them, so they stayed the bare native control and the
    // next walk found them. His measurement, and it is the same instrument this check uses:
    //
    //     "Their pixels are byte-identical in light and dark (the same 60 px of rgb(255,255,255),
    //      18 of (252,252,252), 16 of (209,209,209), 6 of (188,188,188) in both captures) — this
    //      is the unstyled native control."
    //     LIGHT unchecked border #bcbcbc on #fdfcf8 = 1.85:1, against a 3:1 floor.
    //     DARK  the same white box on the navy panel renders as a SOLID FILLED WHITE SQUARE,
    //           which reads as CHECKED. His positive control: he checked "Show it" and it became
    //           a blue box with a white tick, while the still-unchecked "Show it when RichOS
    //           starts" stayed a solid white square. So the unchecked state was the more
    //           prominent of the two and the panel read backwards.
    //
    // THREE ASSERTIONS, BECAUSE THE RATIO ALONE CALLED THE WORSE DEFECT FINE (15.38:1):
    //   1. the ratio, in both themes, both states, against 3:1;
    //   2. the FILL — a checked box carries strictly more of its own ink than an unchecked one,
    //      which is what "checked" has always looked like and what survives grayscale;
    //   3. THE THEMES DIFFER. A control whose crop is byte-identical in light and dark is a
    //      control nothing in this app is painting, whatever its ratio says. That is the one
    //      assertion that would have caught this defect as it shipped.
    const BOX_FLOOR = 3;
    const lines = [];
    const crops = {};
    for (const theme of ["light", "dark"]) {
      const page = await openApp(browser, theme);
      await page.click('.nav-thread[data-thread-id="hiring"]');
      await settleOnThread(page, "hiring");
      await page.click("#set-btn");
      await page.waitForSelector("#set-menu:not([hidden])");
      await awaitSettled(page);

      // ONE CONTROL, BOTH OF ITS STATES, and it is measured by setting `.checked` in the
      // DOM rather than by clicking. Two reasons, and neither is convenience.
      //
      //   1. THIS CHECK IS ABOUT PAINT, not about conduct. `.checked = true` fires no
      //      `change` listener, so the three-way technical-view scope sheet does not open
      //      over the panel — and measuring a control through a modal measures the modal.
      //   2. ONE CONTROL RATHER THAN TWO removes the last way a fill comparison can lie: the
      //      two crops are the same element, at the same size, on the same ground, one frame
      //      apart. Nothing but the state differs.
      //
      // It reads `#splash-enabled`, the checkbox of this kind that Settings still holds (in the
      // band across its foot). Round 27 removed the Preferences "Show it" checkbox this used
      // to read: the technical view has one switch now, `#set-techy`, which is a switch.
      //
      // THE STATE IS SET AND THE CROP IS TAKEN IN THE SAME TURN OF THIS LOOP. The first
      // draft set both states first and screenshotted afterwards, so both crops were of the
      // CHECKED control and the fill test compared 188 samples with 188. It failed, which is
      // the fill test doing its job on its own author.
      const measured = [];
      for (const want of [false, true]) {
        const b = await page.evaluate((state) => {
          const node = document.getElementById("splash-enabled");
          node.checked = state;
          const r = node.getBoundingClientRect();
          return { id: "splash-enabled", checked: node.checked, x: r.x, y: r.y, w: r.width, h: r.height };
        }, want);
        assertEqual(b.checked, want, "the checkbox would not take the state to be measured");
        // The same three-pixel skirt check 16 uses, so the ground is READ off the panel
        // rather than assumed, and the two checks are the same instrument — including the
        // plain ground (`onPlainGround`), as a non-text indicator is measured.
        const buf = await onPlainGround(page, () => page.screenshot({
          clip: {
            x: Math.floor(b.x - 3),
            y: Math.floor(b.y - 3),
            width: Math.ceil(b.w + 6),
            height: Math.ceil(b.h + 6),
          },
        }));
        crops[theme + ":splash-enabled:" + b.checked] = buf.toString("base64");
        measured.push(Object.assign({ theme: theme }, C.measureIndicatorCrop(buf), { id: b.id, checked: b.checked }));
      }
      assert(
        measured.length === 2 && !measured[0].checked && measured[1].checked,
        "the two states were not both measured: " + JSON.stringify(measured)
      );

      for (const m of measured) {
        assert(
          m.ratio >= BOX_FLOOR,
          theme + ": the " + (m.checked ? "checked" : "unchecked") + " checkbox (" + m.id +
            ") paints " + m.ink + " on " + m.ground + " = " + m.ratio + ":1, under the " +
            BOX_FLOOR + ":1 floor a non-text indicator owes. Ray measured this shape at 1.85:1."
        );
      }
      const on = measured.find((m) => m.checked);
      const off = measured.find((m) => !m.checked);
      assert(
        on.inkSamples > off.inkSamples,
        theme + ": the CHECKED box carries " + on.inkSamples + " samples of its own ink and the " +
          "unchecked one carries " + off.inkSamples + ". The unchecked control is at least as " +
          "filled as the checked one, so the panel reads backwards — which is defect 3.6's " +
          "dark-mode half, and it passed its RATIO at 15.38:1."
      );
      lines.push(
        theme + ": checked " + on.ratio + ":1 (" + on.inkSamples + " samples) vs unchecked " +
          off.ratio + ":1 (" + off.inkSamples + ")"
      );
      await page.close();
    }

    // 3. THE THEMES DIFFER, in BOTH states. Ray's own evidence that the control was
    // unstyled was that its pixels were the same in light and dark. A theme-aware control
    // cannot be, and asserting it of the checked state as well as the unchecked one is what
    // stops a half-themed control passing.
    for (const state of ["false", "true"]) {
      assert(
        crops["light:splash-enabled:" + state] !== crops["dark:splash-enabled:" + state],
        "the " + (state === "true" ? "checked" : "unchecked") + " checkbox paints " +
          "byte-identical pixels in light and dark. That is not a ratio failure, it is the " +
          "proof that nothing in this app is painting the control — which is how defect 3.6 " +
          "shipped past a green contrast run."
      );
    }

    return (
      "floor " + BOX_FLOOR + ":1, measured off the painted control — " + lines.join("; ") +
      "; and both themes' crops differ, which the native control's never did."
    );
  });

  // =======================================================================================
  // 16c — A FOCUS RING IS A DIFFERENT PAINT FROM THE CONTROL IT RINGS
  //
  // THE SHIPPED DEFECT, AND IT IS ON THE ONE SCREEN A CUSTOMER CANNOT GET PAST.
  // `.desk-btn--confirm` fills itself with `--accent` and `.desk-btn:focus-visible` drew its
  // ring in `--accent`:
  //
  //     dark   #c2a35c ring on #c2a35c fill    1.00:1
  //     light  #9c7c34 ring on #9c7c34 fill    1.00:1
  //
  // `Set it up`, focused, differed from `Set it up`, unfocused, by a 1px hairline of panel.
  //
  // WHY THE SURFACE WALK ABOVE COULD NOT SEE IT, which is the same shape as 16 and 16b: the
  // walk measures what is PAINTED AT REST. A focus ring exists only while a control has the
  // keyboard, so no resting frame in this suite has ever contained one. This check focuses
  // the control first, which is the whole of what it adds.
  //
  // AND THE NOTE IN `style.css` WAS NOT WRONG, WHICH IS THE INSTRUCTIVE PART. It quotes 6.36:1
  // dark and 3.83:1 light for that ring and both are correct — against the PANEL. A filled
  // button has a second neighbor the plain one does not: its own fill. A ratio is never a
  // property of a color, only of a pair, and the pair that was never computed is the one the
  // CEO was looking at.
  //
  // THE RULE, and it is derived rather than a list: a focused control's ring must clear 3:1
  // against the paint on EACH SIDE of it — its own fill on the inside, whatever is behind the
  // control on the outside. A two-tone ring answers each side with its own band, so the inner
  // band (a `box-shadow` of 0 blur filling the `outline-offset` gap) is measured against the
  // fill and the outline against the ground, plus the boundary between the two.
  //
  // THE SWEEP IS WHAT MAKES IT A FLOOR. Every visible focusable control in an ordinary window
  // is focused and measured, so the next filled button is covered the day it is written and
  // nobody has to remember this.
  // =======================================================================================

  await run.check("16c  a focus ring is a different paint from the control it rings — the offer's primary was 1.00:1", async () => {
    const RING_FLOOR = 3;

    /// Focus a control and read the three colors its ring actually touches. Returned as
    /// numbers computed IN THE PAGE by the same arithmetic the rest of this suite ships there.
    const RING_PROBE = function (selector) {
      const M = window.__contrastMath;
      const opaqueBehind = function (node) {
        for (let n = node; n; n = n.parentElement) {
          const c = M.parseCssColor(getComputedStyle(n).backgroundColor);
          if (c && c.a >= 0.999) return c;
        }
        return null;
      };
      const node = typeof selector === "string" ? document.querySelector(selector) : selector;
      if (!node) return { state: "absent" };
      node.focus();
      if (document.activeElement !== node) return { state: "refused focus" };
      const cs = getComputedStyle(node);
      const width = parseFloat(cs.outlineWidth) || 0;
      if (cs.outlineStyle === "none" || width === 0) return { state: "no ring" };
      const ring = M.parseCssColor(cs.outlineColor);
      const fill = opaqueBehind(node);
      const ground = opaqueBehind(node.parentElement);
      if (!ring || !fill || !ground) return { state: "unresolvable" };
      // The computed `box-shadow` leads with its color in WebKit. Only a shadow with no blur
      // is a RING; a blurred one is a glow and has no edge to measure.
      const shadow = cs.boxShadow && cs.boxShadow !== "none" ? cs.boxShadow : "";
      const shadowColor = shadow.match(/rgba?\([^)]+\)/);
      const numbers = shadow.replace(/rgba?\([^)]+\)/g, "").trim().split(/\s+/).map(parseFloat);
      const isRing = shadowColor && numbers.length >= 4 && numbers[2] === 0 && numbers[3] > 0;
      const inner = isRing ? M.parseCssColor(shadowColor[0]) : null;
      const r = function (a, b) {
        return M.round2(M.contrastRatio(a, b));
      };
      return {
        state: "measured",
        id: node.id || node.className,
        ring: M.hex(ring),
        fill: M.hex(fill),
        ground: M.hex(ground),
        inner: inner ? M.hex(inner) : null,
        // The three boundaries of the composite, outside in.
        ringVsGround: r(ring, ground),
        innerVsFill: inner ? r(inner, fill) : r(ring, fill),
        ringVsInner: inner ? r(ring, inner) : null,
      };
    };

    const lines = [];
    for (const theme of ["light", "dark"]) {
      // THE OFFER'S OWN TWO ANSWERS, on the surface they are asked on. `setup: "missing-engine"`
      // is the customer's Mac, and `#setup-go` is the control that holds the keyboard at entry.
      const offer = await openApp(browser, theme, false, { setup: "missing-engine" });
      await offer.waitForSelector("#setup-sheet:not([hidden])", { timeout: 10000 });
      await offer.evaluate(C.pageScript());
      for (const id of ["#setup-go", "#setup-later"]) {
        const m = await offer.evaluate(RING_PROBE, id);
        assertEqual(m.state, "measured", theme + ": " + id + " — " + m.state);
        assert(
          m.innerVsFill >= RING_FLOOR,
          theme + ": " + id + "'s ring touches its own fill at " + m.innerVsFill + ":1 (" +
            (m.inner || m.ring) + " on " + m.fill + "), under the " + RING_FLOOR +
            ":1 floor a non-text indicator owes. A gold ring around a gold button is 1.00:1."
        );
        assert(
          m.ringVsGround >= RING_FLOOR,
          theme + ": " + id + "'s ring is " + m.ringVsGround + ":1 against the panel behind it (" +
            m.ring + " on " + m.ground + ")"
        );
        if (m.ringVsInner !== null) {
          assert(
            m.ringVsInner >= RING_FLOOR,
            theme + ": " + id + "'s two ring bands are " + m.ringVsInner + ":1 apart, so the " +
              "composite reads as one band and the inner one buys nothing"
          );
        }
        lines.push(
          theme + " " + id + " " + (m.inner ? m.inner + "/" : "") + m.ring + ": " +
            m.innerVsFill + ":1 on its fill, " + m.ringVsGround + ":1 on the panel"
        );
      }
      await offer.close();

      // THE SWEEP. An ordinary window, every visible focusable control, same rule. Controls
      // that refuse focus, carry no ring, or sit on a translucent ground are reported rather
      // than counted — an unmeasurable control is not a passing one.
      const page = await openApp(browser, theme);
      await page.evaluate(C.pageScript());
      const swept = await page.evaluate((probeSource) => {
        const probe = eval("(" + probeSource + ")");
        const FOCUSABLE =
          "button:not([disabled]), [href], input:not([disabled]), select:not([disabled]), " +
          'textarea:not([disabled]), [tabindex]:not([tabindex="-1"])';
        const out = { measured: 0, skipped: 0, failures: [] };
        for (const node of document.querySelectorAll(FOCUSABLE)) {
          if (node.offsetParent === null && getComputedStyle(node).position !== "fixed") continue;
          const m = probe(node);
          if (m.state !== "measured") {
            out.skipped++;
            continue;
          }
          out.measured++;
          if (m.innerVsFill < 3 || m.ringVsGround < 3 || (m.ringVsInner !== null && m.ringVsInner < 3)) {
            out.failures.push(
              m.id + " ring " + m.ring + " fill " + m.fill + " ground " + m.ground +
                " -> " + m.innerVsFill + ":1 inside, " + m.ringVsGround + ":1 outside"
            );
          }
        }
        return out;
      }, RING_PROBE.toString());
      assertEqual(
        swept.failures.length,
        0,
        theme + ": " + swept.failures.length + " focused control(s) ring themselves in their " +
          "own paint or their ground's — " + swept.failures.join(" | ")
      );
      assert(swept.measured >= 5, theme + ": the sweep measured only " + swept.measured + " controls");
      lines.push(theme + " sweep " + swept.measured + " measured, " + swept.skipped + " without a ring of their own");
      assert(page.__errors.length === 0, "the shell logged errors: " + page.__errors.join(" | "));
      await page.close();
    }

    return "floor " + RING_FLOOR + ":1 on each side of the ring — " + lines.join("; ");
  });

  await run.check("17  the technical view's own labels, DECLARED and RENDERED — the surface walk sees neither", async () => {
    // AUDIT-10 ROW 4, AND TWO REASONS THIRTY-NINE GREEN SURFACES DID NOT HOLD IT.
    //
    // REASON ONE: THE SURFACE NAMED `technical-view` CONTAINS NO TECHNICAL ROWS. Its driver
    // opens `hiring`, and `hiring` is the fixture chosen deliberately to have NO machinery and
    // an EMPTY between-turn lane — `mock.js` says so where it defines them: "`acme` has
    // traffic; `hiring` deliberately has none, so the honest empty state is a screen this
    // harness can actually open". Measured here before this check was written: with the
    // technical view pinned on `hiring`, `document.querySelectorAll(".tl-tech").length` is 0.
    // So the 148 nodes that surface reports are the conversation AROUND the technical view,
    // and every label Ray measured was outside the walk. That is not a bug in that surface —
    // it is the right fixture for the empty state — it is a hole where the full state should
    // also be walked.
    //
    // REASON TWO, AND THE ONE WORTH KEEPING: THE DECLARED COLOR PASSED AND THE PAINTED GLYPH
    // DID NOT. Measured on `acme`, light mode, before the fix:
    //
    //                                             declared   rendered
    //   `system:init` / `system:status`  11px italic   4.98:1     4.36:1   under the floor
    //   `stream_event:message_delta`     11px italic   4.98:1     4.59:1   barely over it
    //   the bounded preview / the paths  14px          4.98:1     4.99:1   barely over it
    //   the chevron                      11px          4.98:1     3.75:1   under the floor
    //
    // One token, one ground, four different rendered numbers — because antialiasing over
    // thin, slanted, 11px strokes never lets the painted pixel reach the declared color. Ray
    // reported 4.36 / 4.49 / 4.60 / 4.99 on the shipped build and every one of them is
    // reproduced above to the second decimal. A computed-style walk reads the declared column
    // and can never read the other one, so this check reads BOTH and holds both to 4.5:1.
    //
    // THE DECLARED FLOOR IS DELIBERATELY HIGHER THAN 4.5. A declared ratio with nothing in
    // hand is a ratio that has not been measured where it is read; `DECLARED_FLOOR` is what
    // stops this cluster drifting back to a technically-passing 4.98:1.
    const RENDERED_FLOOR = 4.5;
    const DECLARED_FLOOR = 6;
    const lines = [];
    const plainGround = { shots: 0, hidden: 0 };
    for (const theme of ["light", "dark"]) {
      const page = await openApp(browser, theme);
      // `acme` AND NOT `hiring`, for the reason at the top of this check.
      await page.click('.nav-thread[data-thread-id="acme"]');
      await settleOnThread(page, "acme");
      await page.keyboard.press("Meta+Shift+T");
      await techyScopeThisConversation(page);
      await page.waitForFunction(() => {
        const chip = document.getElementById("techy-chip");
        return !!chip && !chip.hidden;
      });
      await awaitSettled(page);
      // THE CHEVRON IS WAITED FOR, BECAUSE THIS CHECK PASSED WITHOUT ONE. On 2026-09-29 two runs
      // of this same file on the same tree measured 2 nodes in each theme (the two vendor labels
      // and nothing else, so no chevron at all) and then 13 light / 21 dark with the chevron
      // among them: the conversation's technical rows arrive after `awaitSettled` returns on
      // some runs. `acme`'s rows are always expandable (main.js hands the timeline
      // `machineryRaw`), so a visible chevron is what this fixture renders. The wait is for the
      // FACT; its 60 s bound is a hang guard only, and running out fails here by name.
      await page
        .waitForFunction(() => [...document.querySelectorAll(".tl-tech-chevron")].some((n) => {
          const r = n.getBoundingClientRect();
          return r.width >= 2 && r.height >= 2;
        }), null, { timeout: 60000 })
        .catch(() => {
          throw new Error(
            theme + ": `acme` never showed a visible .tl-tech-chevron (hang guard, 60 s), so the one " +
              "non-text node in this cluster could not be measured"
          );
        });

      // THE POSITIVE PROBE, and it is the whole defense against this check passing by
      // measuring nothing. If the fixture ever stops rendering technical rows — the exact
      // state `technical-view` is in today — this fails here rather than reporting a clean
      // sweep of an empty screen.
      // HANDLES, NOT COORDINATES, AND THE FIRST DRAFT OF THIS CHECK PROVED WHY. Reading every
      // rect in one `evaluate` and then shooting those rectangles one at a time reported
      // `#eae6dd on #eae6dd` — a crop of bare ground — because the conversation moves under a
      // coordinate the moment anything above it settles. `elementHandle.screenshot()` scrolls
      // the node into view and shoots THAT node, so the frame and the element can never be one
      // scroll apart.
      const WANT = [
        // THE SIZES IN THESE LABELS ARE THE SHIPPED ONES AND THEY MOVED ON 2026-09-18. The
        // vendor label was 11px italic mono when this check was written and is 14px now
        // (§15's floor, escalation `esc-20260918T211347Z-d7e05a01`) — and the rendered
        // column moved with it, from 6.28:1 to 7.37:1 in light and 7.49:1 to 8.51:1 in
        // dark, against a declared 7.39/8.50 that did not change. That is this check's own
        // premise arriving as evidence: the gap between declared and rendered was the
        // thin-stroke penalty of the SIZE, and it closes to 0.02 at 14px. The chevron and
        // the status mark stay at 11px as declared icon glyphs (style.css, and
        // `appearance.js` check 12's `GLYPHS`), so the chevron is still measured here as
        // the one 11px node in the cluster.
        [".tl-tech[data-vendor] .tl-tech-title", "vendor label, 14px italic mono"],
        [".tl-tech-summary", "the bounded preview, 14px"],
        [".tl-tech-path", "a touched path, 14px mono"],
        // The one NON-TEXT node here, so it is shot on the plain ground (`onPlainGround`).
        [".tl-tech-chevron", "the expand chevron, 11px glyph", { nonText: true }],
      ];
      const nodes = [];
      for (const [sel, what, how] of WANT) {
        for (const handle of await page.$$(sel)) {
          const info = await handle.evaluate((n) => {
            const r = n.getBoundingClientRect();
            if (r.width < 2 || r.height < 2) return null;
            const cs = getComputedStyle(n);
            // The ground the run is painted over, resolved the way the walk resolves one: up
            // the ancestor chain to the first opaque background.
            let host = n.parentElement;
            let ground = null;
            while (host && !ground) {
              const c = getComputedStyle(host).backgroundColor;
              const m = c && c.match(/^rgba?\(([^)]+)\)$/);
              if (m) {
                const p = m[1].split(/[,/\s]+/).filter((x) => x.length);
                if (p.length < 4 || parseFloat(p[3]) >= 0.999) ground = c;
              }
              host = host.parentElement;
            }
            return { text: (n.textContent || "").trim().slice(0, 44), color: cs.color, ground,
                     size: cs.fontSize, weight: cs.fontWeight };
          });
          if (info) nodes.push(Object.assign({ sel, what, handle, nonText: !!(how && how.nonText) }, info));
        }
      }
      assert(
        nodes.filter((n) => n.sel.indexOf("tl-tech-title") >= 0).length >= 2,
        theme + ": the fixture rendered fewer than two vendor labels, so this check would be " +
          "measuring an empty screen — which is precisely the state the `technical-view` surface " +
          "is in. Conversation `acme` is the one with machinery (`mock.js` `machineryRaw`)."
      );

      let worstDeclared = null;
      let worstRendered = null;
      const chevrons = [];
      for (const n of nodes) {
        // ---- the declared column, the suite's own arithmetic on the resolved colors -------
        const fg = C.parseCssColor(n.color);
        const bg = C.parseCssColor(n.ground);
        assert(fg && bg, theme + ": could not resolve " + n.sel + " (" + n.color + " on " + n.ground + ")");
        const declared = C.round2(C.contrastRatio(fg.a < 1 ? C.compositeOver(fg, bg) : fg, bg));
        assert(
          declared >= DECLARED_FLOOR,
          theme + ": " + n.what + " declares " + declared + ":1 (" + n.color + " over " + n.ground +
            "). The floor for THIS cluster is " + DECLARED_FLOOR + ":1, not 4.5, because " +
            "`--ink-faint` declared 4.98:1 here and painted 4.36:1 — a declared ratio with " +
            "nothing in hand is a ratio nobody has measured where it is read."
        );
        if (!worstDeclared || declared < worstDeclared.r) worstDeclared = { r: declared, what: n.what };

        // ---- the rendered column, off the painted frame ----------------------------------
        // TEXT is shot as painted, speckle and all: text owes 4.5:1 at the worst single pixel,
        // and `speckled-ground.js` check 4 holds every text node on the ground to that against
        // the strongest point the engine can paint. The CHEVRON is non-text and is shot on the
        // plain ground it sits on, with the speckle hidden (`onPlainGround` says why and what
        // it measured when it was not).
        const buf = n.nonText ? await onPlainGround(page, () => n.handle.screenshot(), plainGround) : await n.handle.screenshot();
        const m = C.measureIndicatorCrop(buf, 3);
        if (n.nonText) chevrons.push(m.ratio + ":1 (" + m.ink + " on " + m.ground + ")");
        assert(
          !m.unresolvable,
          theme + ": " + n.what + " " + JSON.stringify(n.text) + " — " + m.unresolvable +
            ", which is a failure to prove and never a pass"
        );
        assert(
          m.ratio >= RENDERED_FLOOR,
          theme + ": " + n.what + " " + JSON.stringify(n.text) + " declares " + declared +
            ":1 and PAINTS " + m.ratio + ":1 (" + m.ink + " on " + m.ground + ") at " + n.size +
            ". Under the " + RENDERED_FLOOR + ":1 floor. This is audit-10 row 4 exactly: the " +
            "declared column passed and the glyph on the glass did not."
        );
        if (!worstRendered || m.ratio < worstRendered.r) worstRendered = { r: m.ratio, what: n.what, text: n.text };
      }
      assert(
        chevrons.length >= 1,
        theme + ": no chevron was measured, so the one non-text node in this cluster went unshot"
      );
      lines.push(
        theme + ": " + nodes.length + " label(s), worst declared " + worstDeclared.r + ":1 (" +
          worstDeclared.what + "), worst rendered " + worstRendered.r + ":1 (" + worstRendered.what +
          "); chevron(s) on the plain ground " + chevrons.join(", ")
      );
      await page.close();
    }
    return (
      "declared floor " + DECLARED_FLOOR + ":1, rendered floor " + RENDERED_FLOOR + ":1 — " + lines.join("; ") +
      ". Non-text crops on the plain ground: " + plainGround.shots + ", the speckle mounted and hidden for " +
      plainGround.hidden + " of them"
    );
  });

  // ---- 18: the sidebar toggle's glyph (output side-panel PRD §8, slice S8) ------------------
  //
  // The toggle at the top left of the conversation is a glyph and nothing else: an SVG stroke
  // in `currentColor`, transparent fill and border. The walk above files it `textOnly` (no fill,
  // no border, no text) and check 15 says why it cannot read a stroke at all. So it is measured
  // where it is painted, as check 17 measures the technical view's chevron: a crop of the
  // control, on the plain ground with the speckle hidden, the ink resolved by the same
  // `measureIndicatorCrop`. Both states, because the glyph changes with them (its left pane
  // fills at .28 while the sidebar is away), and at rest, not hovered. A non-text indicator owes
  // 3:1 (WCAG 1.4.11); round 17.1 measured 6.69:1 light and 7.91:1 dark on its own tokens.
  await run.check("18  the sidebar toggle's glyph, open and away, both themes, as painted", async () => {
    const FLOOR = 3.0;
    const lines = [];
    const plainGround = { shots: 0, hidden: 0 };
    for (const theme of THEMES) {
      const page = await openApp(browser, theme);
      for (const state of ["open", "away"]) {
        if (state === "away") {
          await page.click("#rail-toggle");
          await page
            .waitForFunction(() => getComputedStyle(document.getElementById("rail")).visibility === "hidden", null, { timeout: 60000 })
            .catch(() => {
              throw new Error(theme + ": the sidebar never went away after the toggle was clicked (60 s hang guard)");
            });
        }
        await page.mouse.move(700, 600);
        await awaitSettled(page);
        const btn = await page.$("#rail-toggle");
        assert(btn, theme + ": there is no #rail-toggle on the shell");
        const expanded = await btn.getAttribute("aria-expanded");
        assertEqual(expanded, state === "open" ? "true" : "false", theme + ": the toggle's state");
        const buf = await onPlainGround(page, () => btn.screenshot(), plainGround);
        const m = C.measureIndicatorCrop(buf, 3);
        assert(!m.unresolvable, theme + " " + state + ": " + m.unresolvable + ", which is a failure to prove and never a pass");
        assert(
          m.ratio >= FLOOR,
          theme + " " + state + ": the sidebar toggle's glyph PAINTS " + m.ratio + ":1 (" + m.ink + " on " + m.ground +
            "), under the " + FLOOR + ":1 a non-text indicator owes"
        );
        lines.push(theme + " " + state + " " + m.ratio + ":1 (" + m.ink + " on " + m.ground + ", " + m.inkSamples + " px of ink)");
      }
      await page.close();
    }
    return lines.join("; ") + ". Crops on the plain ground: " + plainGround.shots + ", speckle hidden for " + plainGround.hidden;
  });

  // ---- 19: the frame right after a theme switch ---------------------------------------------

  await run.check("19  the very frame after a theme switch, both directions, on every surface, meets AA with no wait", async () => {
    // WHAT THIS CLOSES. Every walk above measures a SETTLED page. A rule that transitions its
    // background (or border, or color) does not break the settled page: on the frame after
    // `data-theme` flips, `color` is already the new theme's and the background is still the old
    // one, so the Output button read `#dfe4ee` on `#f7f5ef` (1.17:1) for 150 ms and `.of-tool`
    // did the same. That window is invisible to a walk that waits.
    //
    // HOW. Per surface and per direction: drive it, flip `data-theme`, and in the SAME task run
    // the probe, with no frame and no timer between. Style is recomputed at that point and a
    // transition starts at its FROM value, so what is read is the first painted frame. The
    // probe is then run again after the transitions have finished. A failure present only in the
    // instant read is a lag; one present in both is the surface's own (the debt ledger's) and is
    // not this check's to count. Debt-bearing surfaces therefore cannot hide a lag, and clean
    // ones cannot invent one.
    const lines = [];
    let measured = 0;
    for (const surface of SURFACES) {
      if (surface.holdSplash) continue; // §15's always-dark clamp: there is no switch to measure
      for (const [from, to] of [["light", "dark"], ["dark", "light"]]) {
        const page = await openApp(browser, from, surface.holdSplash, surface.preset);
        await surface.drive(page);
        await page.evaluate(C.pageScript());
        const instant = await page.evaluate((o) => {
          document.documentElement.setAttribute("data-theme", o.to);
          return window.__contrastProbe({ surface: o.surface, theme: o.to });
        }, { to, surface: surface.name });
        await page.waitForFunction(() => document.getAnimations().every((a) => !(a instanceof CSSTransition)));
        const settled = await walk(page, surface.name, to);
        const lag = Object.keys(instant.failures).filter((k) => !(k in settled.failures));
        assertEqual(
          lag.map((k) => ({ node: k, ...instant.failures[k] })),
          [],
          surface.name + " " + from + "->" + to + ": measured at once after the switch, these fail AA and the settled page does not — a background, border or color is fading"
        );
        measured += (instant.measuredPaths || []).length;
        await page.close();
      }
    }
    lines.push(measured + " node reading(s) taken before the first frame, no lag failure");
    return lines.join("; ");
  });

  // ---- the run's own numbers, printed whether it passes or fails ---------------------------

  const t = seen.totals;
  console.log("\n== the shipping shell, by the numbers ==");
  console.log("  surfaces walked          " + SURFACES.length + " x " + THEMES.length + " themes = " + Object.keys(perSurface).length + " walks");
  console.log("  text nodes considered    " + t.considered);
  console.log("    of those, not rendered " + t.invisible + "  (display:none / zero-area / a closed panel)");
  console.log("    behind a modal         " + t.obscured + "  (inert; each one measured on a surface where it is not)");
  console.log("    MEASURED               " + t.checked);
  console.log("      passed               " + t.passed);
  console.log("      failed               " + t.failedNodes + " node(s) over " + seen.signatures.size + " distinct colour pairings");
  console.log("  non-text indicators      " + t.indicators + " considered, " + t.indicatorsChecked + " with a boundary of their own to check");
  console.log("  resolved by ancestor     " + t.ancestor + "  (off-viewport: no hit test, so no occlusion proof)");
  console.log("  measured through a veil  " + t.veiled);
  // DEDUPED, because the same unprovable colour is met once per walk and the summary read
  // "UNRESOLVABLE 2" against check 13's "1 declared-unprovable colour(s)" — two true numbers
  // that look like a discrepancy. The count that means something is distinct colours.
  const distinctUnresolvable = new Set(seen.unresolvable.map((u) => u.split(": ").slice(1).join(": ")));
  console.log(
    "  UNRESOLVABLE             " + distinctUnresolvable.size + "  (a failure to prove, not a pass; met " +
      seen.unresolvable.length + " time(s) across the walks)"
  );
  console.log("  declared exempt          " + seen.exemptions.length);
  console.log("  <canvas> unmeasured      " + seen.canvas + "   in #home (measured by tests/home.js)  " +
    seen.canvasInHome + "   in #phone-sheet (measured by tests/phone.js)  " + seen.canvasInPhone +
    "   svg <text> seen  " + seen.svgText);

  console.log("\n== every distinct failing colour pairing on the shipping shell today ==");
  const all = {};
  for (const out of Object.values(perSurface)) for (const [sig, f] of Object.entries(out.failures)) all[sig] = f;
  const sorted = Object.entries(all).sort((a, b) => a[1].ratio - b[1].ratio);
  for (const [sig, f] of sorted) {
    console.log("  " + String(f.ratio).padStart(5) + ":1  (needs " + f.threshold + ")  " + sig);
    console.log("           e.g. " + f.selector + (f.text && f.text !== "(non-text indicator)" ? "  “" + f.text + "”" : ""));
  }

  if (seen.exemptions.length) {
    console.log("\n== declared exemptions honoured this run (every one, with its reason) ==");
    for (const e of seen.exemptions) console.log("  " + e);
  }

  const failed = run.report();
  const pageErrors = 0;
  await browser.close();
  process.exitCode = failed || pageErrors ? 1 : 0;
}

main().catch((e) => {
  console.error(e);
  process.exit(1);
});

// ---------------------------------------------------------------------------------------
// RUN RED — the mutation that made each check fail, and what went red with it
// ---------------------------------------------------------------------------------------
//
// Twenty-two runs, one edit each, the file restored from a copy taken before the edit
// whether the run passed or failed, and `git status` clean at the end. Full transcript with
// the failure text of every run: `docs/verification/contrast-gate-2026-08-30/`.
//
// TWO OF THEM DID NOT GO RED THE FIRST TIME AND ARE RECORDED AS SUCH, in the transcript and
// here. A mutation that turns nothing red proves nothing, and quietly replacing it with one
// that works is how a run-red list becomes decoration.
//
//  1   lib/contrast.js `srgbToLinear`: `return v` instead of the piecewise gamma expansion
//        -> #777777 on white reads 2.03:1 where WebAIM publishes 4.48. Also reds 3, 8 and
//           every surface: the whole gate is built on this function
//  2   lib/contrast.js `pageScript`: drop `round2` from the math shipped into the page
//        -> `M.round2 is not a function` in WebKit. The two runtimes are proven to be one
//           source by the fact that removing it from one removes it from both
//  3   lib/contrast.js `thresholdFor`: `return NORMAL` unconditionally
//        -> the 24px node, the 19px-bold node and the declared indicator all fail at 4.5
//  3b  lib/contrast.js `isLargeText`: bold boundary 18px instead of 18.66px
//        -> 18.65px/700 counts as large text
//  4   lib/contrast.js the text loop: `if (true) { out.nodesPassed++; continue; }`
//        -> the planted 2.17:1 caption produces no failure at all
//  4b  lib/contrast.js: file the failure with `selector: "(a node)"`
//        -> the failure is counted and the node is not named, which is the report nobody
//           can act on
//  5   lib/contrast.js: honour a declared exemption without filing it into `out.exempt`
//        -> it passes SILENTLY, which is the exact failure mode the mechanism exists to
//           prevent: an excuse nobody sees is not a declaration
//  5b  lib/contrast.js `exemptionFor`: `hasAttribute` on the node instead of `closest`
//        -> the anchor inside the privacy notice is not covered by the notice's exemption
//  6   lib/contrast.js `MIN_REASON = 0`
//        -> `data-contrast-exempt=""` becomes a working mute button
//  7   lib/contrast.js `resolveBackground`: drop the `backgroundImage !== "none"` guard
//        -> text over a black-to-white gradient resolves to the paper behind it and passes
//  7b  lib/contrast.js: `continue` past an unprovable node without filing it anywhere
//        -> four unprovable nodes vanish from the report and the run stays green
//  8   lib/contrast.js `stackFor`: return the chain without `over`
//        -> the 12% sheet is ignored and #767676 reads 4.54:1 instead of the 4.25:1 the eye
//           gets — a node that passes on paper and fails on screen
//  9   style.css `--ink-faint: #a5a297` -> `#8f8c81`
//        -> 20 NEW pairings on the shell alone, named with their ratios; all nine driven
//           surfaces red
//  9b  style.css: `.nav-thread-title { display: none }`
//        -> the shell measures 66 nodes against a floor of 80. The surface still renders and
//           has nothing wrong with what is left, which is precisely the run that would
//           otherwise read as an improvement
//  9z  index.html: one line of text in the always-present Settings panel (`<p class="md-note">`
//      with a `<b>` and an `<i>` after `#md-note`; 3 text nodes per theme). 2026-10-19:
//        on main 811d8af57 (whole-page floors) -> the bare shell went 742 -> 748 and 9z
//           went RED on five surfaces nobody touched: phone-tailscale-ready,
//           phone-connect-setup, feedback, search, unbound
//        with floors over the shell -> every check green; the shell moved, no floor did, and
//           only the four surfaces that open the Settings panel measured more (+6 each)
// 9z+ the `search` driver returns before pressing anything (a driver that never opened its
//      surface) -> 9.search: "added only 0 text node(s) to the bare shell ... its floor is
//      88"; 9z stays green and 10c names `search-overlay` as unwalked
//  10  style.css gains a real `@media (prefers-color-scheme: dark)` block AND the suite
// 10b  drop three entries from SURFACES -> check 10b. A short inventory that still reported
//      its findings is a clean bill of health over whatever it happened to reach; run.js's
//      declared-vs-observed gate cannot see it, because the suite would run every check it
//      declares. Also: unsetting RICHOS_UI_TESTS_LEDGER changes NOTHING here, which is the
//      point of the check and was verified by running it both ways.
//      stops passing `colorScheme` to `newPage`
//        -> "1 dark block(s) are shipped, but the dark run produced the identical palette —
//           the emulation is not reaching them, so the dark half of every check above is a
//           fiction". This is the mutation that matters most for check 10: the branch that
//           reports today's single-theme reality is easy, and the one that catches a dark
//           mode nobody is actually testing is the one worth proving
//      theme-boot.js's pre-paint default `|| "system"` -> `|| "dark"`, with mock.js and
//      `Theme::default()` moved to match
//        -> "an install with NO stored preference must take its lighting from the operating
//           system — CEO ruling §63. It opened dark under a light OS and dark under a dark
//           one". The fresh-install walk at the foot of check 10 asserted `"dark"` under one
//           OS until 2026-09-19; one OS can only half-test a rule about following the OS,
//           and the half it tested was the half that agreed with the old default
// 10c  NOT A MUTATION — IT RAN RED ON THE SHIPPED SOURCE, first time, which is a stronger
//      result than a mutation and is why it exists. The panel probe named TEN panels in
//      `index.html` that no driver opened, and three of them were not gaps at all:
//      `#entity-view` (§3.5's company overview), `#thread-menu` (rename/pin/archive) and
//      `#techy-state`'s nothing-recorded arm. Each carries text the CEO reads, each is
//      reachable in a browser with nothing stubbed, and this file had said nothing about any
//      of them. Drivers were written; all three walk clean in both themes (110, 98 and 116
//      nodes measured, 0 new, 0 worsened). The remaining seven are in `unwalkedPanels` with
//      their reasons. The comparator is ALSO proven able to fail inside the check itself:
//      one declaration is dropped from a COPY of the ledger and the check asserts the
//      comparator names exactly that panel — 10b's own comment records what happens when a
//      comparator is built out of the single set it is checking.
//  11  index.html: a painted, click-through curtain (`pointer-events: none`,
//      `rgba(0,0,0,0.6)`, z-index 300) left over the whole app
//        -> every node on every surface is filed obscured and measured nowhere, and check 11
//           names them. A blunt mutation — it reds fifteen checks — and it is the honest one:
//           the bucket only becomes a hiding place when something covers everything.
//  11  FIRST ATTEMPT, INERT, RECORDED AS SUCH: `stackFor`'s modal branch changed to
//      `if (modal)`, dropping the `!modal.contains(el)` half. Nothing went red. The loop it
//      sits in only runs over elements painted ABOVE the node, and a desk card inside the
//      panel has none, so the mutated line was never reached. It proves nothing about check
//      11 and is not counted as a run.
//  11b NOT A MUTATION FIRST — THE SHIPPED SOURCE WAS ALREADY UNMEASURED. `lib/contrast.js`
//      collected `nodeType === 3` children, and a pseudo-element is not a node, so
//      `style.css:3787`'s `.setbtn::after { content: "Settings" }` — the tooltip on the
//      settings button §15 puts on EVERY screen — was rendered text that no check in this
//      directory had ever measured, and it was not on the library header's "WHAT IT DOES
//      NOT SEE" list either, so it was not even a known gap. Measured now, on its own
//      `settings-tooltip` surface, in both themes:
//
//          dark   #979faf on #182440  5.78:1  at 16px, floor 4.5:1
//          light  #595e66 on #fdfcf8  6.38:1  at 16px, floor 4.5:1
//
//      Independently re-derived by hand before the run rather than read off it:
//      `--ink-soft` dark is `rgba(223,228,238,0.64)` over `--surface: var(--card) = #182440`,
//      which composites to rgb(151.4,158.9,175.4) and gives 5.78; light is
//      `rgba(12,19,34,0.68)` over `#fdfcf8`, composites to rgb(89.1,93.6,102.5), 6.38. Both
//      agree with WebKit to the second decimal. The surface is CLEAN — and nothing could
//      have told you that before today.
//  11b(i)  DRIVER MUTATION — `p.hover(".setbtn")` -> `p.hover("#composer-input")` -> check
//      11b. `style.css:3787 = "Settings" was measured in no theme ... the surfaces that
//      report it hidden are: shell/light button#set-btn::after (cumulative opacity 0)`.
//      This is the one that matters: it proves `hidden` is NAMED rather than skipped, so a
//      driver that stops reaching the hover state fails instead of quietly measuring less.
//  11b(ii) SOURCE MUTATION — a `content: "Draft — do not ship"` on a selector nothing
//      matches, appended to `style.css` -> check 11b: `authored content string(s) that NO
//      walk in this suite ever encountered — actual ["style.css:4619 = \"Draft — do not
//      ship\""]`. The other direction of the join, and the one that would catch authored
//      text arriving in a stylesheet no gate opens.
//  12  index.html: `data-contrast-exempt="x"` on `#rail-company`
//        -> rejected by file and by reason for having nothing to say
//  12b index.html: `data-contrast-exempt="chrome, nobody reads the company name"` against
//      an `exemptionCap` of 0
//        -> "1 exemptions are declared; the ledger's cap is 0. Raise the cap deliberately,
//           in the same commit that adds the exemption, so the growth is a decision."
//  13  contrast-debt.json `cap`: 52 with 53 signatures in the file
//        -> the ledger is refused as a place to put new work
//  13b style.css: `.desk-card-target { background-image: linear-gradient(#fff,#fff) }`
//        -> a real caption on a real surface goes unprovable, and there is no ledger entry
//           absorbing it: "colour(s) ... that this gate cannot prove, and that nobody has
//           written down". Also reds 9.corrections
//  13b FIRST ATTEMPT, ANCHOR MISSED: `.desk-card-target {` does not appear in style.css —
//      the rule is written `.desk-card-target,` as the head of a selector list. The driver
//      refused to proceed rather than editing something else, which is the behaviour that
//      makes an anchor miss a recorded non-run instead of a silent one.
//  14  index.html: `<canvas width=10 height=10>` inside the conversation pane
//        -> "a <canvas> appeared on a walked surface ... a green run here must not be read
//           as covering it"
//  15  index.html: `<svg><text x=0 y=9>hi</text></svg>` inside the conversation pane
//        -> 20 svg text nodes across the walks, named as unmeasured rather than passed over
// 16c  style.css: delete the `.desk-btn--confirm:focus-visible` rule
//        -> "light: #setup-go's ring touches its own fill at 1:1 (#9c7c34 on #9c7c34), under
//           the 3:1 floor a non-text indicator owes." RUN, not reasoned about: the rule was
//           taken out, `node contrast.js` exited 1 on exactly that line, and it was put back.
// 16d  style.css: `box-shadow: 0 2px 0 2px var(--on-gold)` on the same rule (a shadow with
//      an OFFSET is not a ring)
//        -> the inner band is no longer counted, and the check falls back to the outline
//           against the fill: 1.00:1 again
//  18  style.css: `#rail-toggle { color: rgba(127, 127, 127, 0.25) }` in place of `--ink-soft`
//        -> "light open: the sidebar toggle's glyph PAINTS 1.29:1 (#cfccc6 on #eae6dd), under
//           the 3:1 a non-text indicator owes." RUN (2026-10-05, S8): `node contrast.js` exited
//           1 on check 18 alone, and the color was put back. Green it measures 5.83:1 light /
//           6.48:1 dark open, 6.6:1 / 7.23:1 away.

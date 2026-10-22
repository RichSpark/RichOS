// THE AFFORDANCE RULE, ENFORCED BY MACHINE.
//
//   A state the user could change must be rendered together with the control that changes
//   it. A state change requiring a human action is not a status, it is a request.
//
// Everything else in this pass was one-time cleanup. This is the part that outlives it: a
// rule with nothing enforcing it is the single defect this project has found eleven times
// in two days, and the eleventh was a set of guards that had landed, were inert, and were
// reported as a DATE rather than as a five-second action.
//
// SIX PARTS, AND WHICH OF THEM BLOCK
// ==================================
//   0  The source list reaches the UI     BLOCKING   derived from the tree, reconciled
//   1  Inventory vs registry              BLOCKING   exact set comparison
//   2  Every ACTIONABLE names a control   BLOCKING   exact, static
//   3  The control is really on screen    BLOCKING   exact, real DOM under WebKit
//   4  Negative and positive controls     BLOCKING   the suite proving it can fail
//   5  The heuristic second opinion       REPORT ONLY, and the number is printed
//
// PART 0 IS NEW, AND IT IS THE PART THAT WAS MISSING WHILE THE OTHER FIVE PASSED. Until
// 2026-09-05 this suite read three files — `index.html`, `main.js`, `timeline.js` — because
// `lib/state-strings.js` carried them as a typed list under a header reading "NEVER TYPE
// IT". The shell ships nine `<script src>` tags and the tree holds twelve product files, so
// nine were outside every check below: `updates.js` (the surface CEO ruling §26 governs),
// `home.js`, `settings-button.js`, `splash.js`, `splash-library.js`, `theme-boot.js` and the
// three `home/field-*.js` modules. A state in any of them could have asked the CEO to do
// something with no control beside it and this file would have reported the rule holding.
//
// The list is derived now (`lib/ui-sources.js`), part 0 proves the derivation reaches the
// whole tree and can refuse, and the widening brought 55 states under the rule — 25 of them
// in `updates.js` alone. What it found is in the commit that widened it; what it PROVES it
// can now catch is the positive control near the bottom of this file, which deletes
// the control the registry names on the shipped update row and watches the rule go red.
//
// WHY PART 5 DOES NOT BLOCK, WITH THE MEASUREMENT RATHER THAN A FEELING. Part 5 asks a
// different question from parts 1-3: not "does this classified state have its control?" but
// "does this string READ like a request, whatever the registry calls it?" — the second
// opinion that stops a future engineer silencing part 2 by writing INFORMATIONAL over an
// actionable sentence. Measured against the hand classification of all 112 states on
// 2026-08-30:
//
//     flagged 24, genuinely ACTIONABLE or NEEDS-SOMEONE-ELSE 17  ->  PRECISION 70.8%
//     of the 31 states that are one of those two, it flags 17    ->  RECALL    54.8%
//
// Roughly one flag in three is noise and it misses nearly half the real ones. A tightened
// clause-initial-imperative variant was also measured — precision 78.6%, recall 35.5% —
// which trades a third of the recall for eight points of precision and is not a better
// gate. Neither is fit to fail a build: a check that cries wolf gets deleted within a day
// and then the CEO is worse off than he is now. Two guards landed today were demoted to
// report-only on exactly this basis (file paths at 2.6% false positives, prose at 17%
// precision) and it was the right call both times.
//
// So part 5 prints its disagreements with the registry's recorded reason beside each, and
// the number, and returns success. Parts 1-4 are exact and fail the run.
//
// RUN IT:  node affordances.js      (or `npm test` in this directory, which discovers it)
//          RICHOS_PLAYWRIGHT=/path/to/node_modules/playwright node affordances.js

"use strict";

const fs = require("fs");
const path = require("path");
const { leaveHome, loadPlaywright, createRun, assert, assertEqual, UI_DIR } = require("./lib/harness");
const {
  inventory,
  normalize,
  jsStringLiterals,
  htmlVisibleStrings,
  looksLikeProse,
  looksLikeShader,
  shaderStrings,
  rustCeoLiterals,
} = require("./lib/state-strings");
const SOURCES = require("./lib/ui-sources");
const REGISTRY = require("./lib/state-registry");

const APP = "file://" + path.join(UI_DIR, "index.html");

const ACTION_CLASSES = ["ACTIONABLE", "NEEDS-SOMEONE-ELSE"];
const ALL_CLASSES = [
  "ACTIONABLE",
  "NEEDS-SOMEONE-ELSE",
  "INFORMATIONAL",
  "CONTROL",
  "FRAGMENT",
  "UNREACHABLE",
  "NOT-RENDERED",
];

/// A party the CEO can be pointed AT. Deliberately a small, closed set: the point is that a
/// future edit which deletes "whoever set RichOS up" from a NEEDS-SOMEONE-ELSE line fails
/// the suite rather than quietly leaving him with a fault and no owner.
// `Whoever set RichOS up` is here for the same reason `An operator` already was: the phrase
// is as likely to open a sentence as to sit inside one, and a case-sensitive alternation that
// carried only one of the two forms would quietly send a sentence that DOES name the party
// down the `explainedBy` path — a row pointing at another state instead of at the words in
// its own. Added 2026-09-06, when the first-run notice's two operator-facing sentences both
// began with it.
const PARTY = /whoever set RichOS up|Whoever set RichOS up|an operator|An operator|whoever installed RichOS/;

// ---------------------------------------------------------------------------------------
// Shell driving
// ---------------------------------------------------------------------------------------

/// The REAL shell — index.html, main.js, mock.js, style.css, timeline.js, all from disk,
/// with nothing stubbed but the Tauri bridge that mock.js already replaces.
///
/// `addInitScript` installs a property hook on `window.RichBridge` BEFORE mock.js assigns
/// it, so the test can (a) capture the listeners main.js registers and replay a
/// `rich://…` payload into them exactly as the Rust emitter would, and (b) override one
/// command's answer. Neither touches product code, and every handler that runs is the
/// shipping one.
/// `preset` is handed to `mock.js` BEFORE the page's own scripts run. It exists for one
/// class of state and cannot be replaced by a setter: "no company has ever been chosen" is
/// decided at boot, and by the time any setter could be called `init()` has already branched
/// on whether a thread was active. See `window.__RICHOS_MOCK_PRESET__` in mock.js.
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

async function openApp(browser, viewport, preset) {
  const page = await browser.newPage({ viewport: viewport || { width: 1400, height: 900 } });
  const errors = [];
  page.on("pageerror", (e) => errors.push(String(e)));
  page.on("console", (m) => {
    if (m.type() === "error") errors.push("console: " + m.text());
  });
  await page.addInitScript(() => {
    let real = null;
    window.__listeners = {};
    window.__overrides = {};
    Object.defineProperty(window, "RichBridge", {
      configurable: true,
      get() {
        return real;
      },
      set(v) {
        real = v;
        const origListen = v.listen.bind(v);
        const origInvoke = v.invoke.bind(v);
        v.listen = function (name, cb) {
          (window.__listeners[name] = window.__listeners[name] || []).push(cb);
          return origListen(name, cb);
        };
        v.invoke = function (cmd, args) {
          if (Object.prototype.hasOwnProperty.call(window.__overrides, cmd)) {
            const o = window.__overrides[cmd];
            return o && o.reject ? Promise.reject(o.value) : Promise.resolve(o ? o.value : undefined);
          }
          return origInvoke(cmd, args);
        };
      },
    });
    window.__emit = function (name, payload) {
      for (const cb of window.__listeners[name] || []) cb({ event: name, payload });
    };
  });
  if (preset) {
    await page.addInitScript((v) => {
      window.__RICHOS_MOCK_PRESET__ = v;
    }, preset);
  }
  await page.goto(APP);
  // The home screen is the landing surface now; this suite is about the app UI behind it.
  await leaveHome(page);
  await page.waitForFunction("typeof window.RichTimeline === 'object'");
  await page.waitForSelector(".nav-thread", { state: "attached" });
  const railHidden = await page.evaluate(() => {
    const r = document.getElementById("rail");
    return getComputedStyle(r).display === "none" || document.body.classList.contains("rail-closed");
  });
  if (railHidden) await page.click("#rail-toggle");
  await page.waitForSelector(".nav-thread", { state: "visible" });
  page.__errors = errors;
  return page;
}

/// EVERYTHING A READER CAN READ on this screen, normalized the way the inventory is: the
/// rendered text, plus the placeholders, titles and accessible names of visible elements.
/// A state that lives in a `placeholder` ("Send is off for this thread") is as visible to
/// the CEO as one in a paragraph, and a check that only read `innerText` would report it
/// absent and pass by accident.
async function readableText(page) {
  const raw = await page.evaluate(() => {
    const seen = [];
    const visible = (e) => {
      const r = e.getBoundingClientRect();
      const s = getComputedStyle(e);
      return r.width > 0 && r.height > 0 && s.visibility !== "hidden" && s.display !== "none";
    };
    for (const e of document.querySelectorAll("*")) {
      if (!visible(e)) continue;
      for (const a of ["placeholder", "title", "aria-label"]) {
        const v = e.getAttribute(a);
        if (v) seen.push(v);
      }
    }
    seen.push(document.body.innerText || "");
    return seen.join("\n");
  });
  return normalize(raw);
}

/// Is the named control PRESENT, VISIBLE, ENABLED and actually interactive?
///
/// "Interactive" is checked rather than assumed: a `<p>` matching the selector would
/// satisfy a naive querySelector and satisfy nobody else. A link to a doc is not an
/// affordance either — an `<a href>` counts only if it goes somewhere in the app, and today
/// nothing in this registry names one.
async function findControl(page, selector) {
  return page.evaluate((sel) => {
    const INTERACTIVE = ["BUTTON", "INPUT", "TEXTAREA", "SELECT"];
    const els = Array.from(document.querySelectorAll(sel));
    const out = { found: els.length, visible: 0, enabled: 0, interactive: 0, sample: null };
    for (const e of els) {
      const r = e.getBoundingClientRect();
      const s = getComputedStyle(e);
      const vis =
        r.width > 0 && r.height > 0 && s.visibility !== "hidden" && s.display !== "none" && !e.hidden;
      if (!vis) continue;
      out.visible++;
      if (e.disabled === true || e.getAttribute("aria-disabled") === "true") continue;
      out.enabled++;
      const interactive =
        INTERACTIVE.indexOf(e.tagName) >= 0 ||
        e.getAttribute("role") === "button" ||
        (e.tagName === "A" && e.hasAttribute("href")) ||
        e.hasAttribute("tabindex");
      if (!interactive) continue;
      out.interactive++;
      if (!out.sample) out.sample = e.tagName.toLowerCase() + (e.id ? "#" + e.id : "") + (e.className ? "." + String(e.className).split(/\s+/)[0] : "");
    }
    return out;
  }, selector);
}

/// THE CHECK, as one function, so the positive control below exercises the same code path
/// the real assertions do rather than a re-implementation of it.
async function assertAffordance(page, entry, opts) {
  opts = opts || {};
  if (opts.requireText) {
    const text = await readableText(page);
    assert(
      text.indexOf(entry.s) >= 0,
      "the state does not render in this fixture: " + JSON.stringify(entry.s.slice(0, 70) + "…")
    );
  }
  // A row with no control is a deliberate claim — INFORMATIONAL, or a NEEDS-SOMEONE-ELSE
  // state where there is genuinely nothing to press. Part 2 has already refused that
  // combination for ACTIONABLE, so reaching here without a control means the only thing
  // left to prove is that the sentence renders at all.
  if (!entry.control) return "(no control by design)";
  const c = await findControl(page, entry.control);
  assert(
    c.found > 0,
    "ACTIONABLE state renders with NO control in the DOM at all.\n" +
      "          state:   " + JSON.stringify(entry.s.slice(0, 80)) + "\n" +
      "          control: " + entry.control
  );
  assert(
    c.visible > 0,
    "ACTIONABLE state names a control that is in the DOM but NOT VISIBLE — a sentence " +
      "pointing at something the CEO cannot see is worse than silence.\n" +
      "          state:   " + JSON.stringify(entry.s.slice(0, 80)) + "\n" +
      "          control: " + entry.control
  );
  assert(
    c.enabled > 0,
    "ACTIONABLE state names a control that is visible but DISABLED.\n" +
      "          state:   " + JSON.stringify(entry.s.slice(0, 80)) + "\n" +
      "          control: " + entry.control
  );
  assert(
    c.interactive > 0,
    "the named control is not something anyone can press — no button, field, link or " +
      "role=button matched.\n" +
      "          state:   " + JSON.stringify(entry.s.slice(0, 80)) + "\n" +
      "          control: " + entry.control
  );
  return c.sample;
}

// ---------------------------------------------------------------------------------------
// Fixtures — each one drives the REAL shell into one state
// ---------------------------------------------------------------------------------------

const TURN = "turn_ok";
const THREAD = "thr_fem";

/// A `get_timeline` snapshot in the exact wire shape, with one CEO message and a duration
/// row in `state`. `working` with no live turn is §14's unknown outcome; `interrupted` is
/// §5.5's failure.
function snapshotWithDuration(state, activeMs) {
  const b = (id, extra) =>
    Object.assign(
      {
        id,
        entityId: "northwind",
        threadId: THREAD,
        turnId: TURN,
        bindingRevision: 1,
        createdAt: 1787949000000,
        sequence: null,
        slot: "stream",
        visibility: "ceo",
      },
      extra || {}
    );
  return {
    entityId: "northwind",
    threadId: THREAD,
    mode: "ceo",
    bindingRevision: 1,
    items: [
      Object.assign(b(TURN + ":user", { slot: "opening", createdAt: 1787948000000 }), {
        kind: "user_message",
        text: "Check the Acme numbers again.",
        source: "text",
      }),
      Object.assign(b(TURN + ":text:0", { sequence: 0 }), {
        kind: "rich_message",
        phase: "unknown",
        text: "Pulling the comparables now.",
      }),
      Object.assign(b(TURN + ":duration", { slot: "terminal", createdAt: 1787948000000 }), {
        kind: "work_duration",
        state,
        startedAt: 1787948100000,
        endedAt: activeMs === null ? null : 1787948100000 + activeMs,
        activeMs,
      }),
    ],
  };
}

/// Force the next thread open to load a crafted snapshot through the shell's OWN reload
/// path (`loadTimeline` -> `get_timeline` -> `applySnapshot`), then click a thread.
async function loadSnapshot(page, snap) {
  await page.evaluate((s) => {
    window.__overrides.get_timeline = { value: s };
  }, snap);
  await page.click('.nav-thread[data-thread-id="hiring"]');
  await page.waitForSelector(".tl-turn");
}

/// Open the correction desk through the rail button the CEO uses, after driving `mock.js`'s
/// own desk into whatever state this fixture is about. Nothing here writes into the DOM.
async function openDesk(browser, setup, preset) {
  const page = await openApp(browser, undefined, preset);
  if (setup) await page.evaluate("(" + setup.toString() + ")(window.__RICHOS_MOCK__)");
  // A launch with no company chosen opens the picker over everything. Dismiss it the way
  // the CEO would before reaching the rail — the composer's block and its control stay.
  await dismissEntityPicker(page);
  await page.click("#nav-corrections");
  await page.waitForSelector("#corrections-overlay:not([hidden])");
  return page;
}

/// Close the launch-time company picker if this fixture booted into it. `Escape` is the
/// dismissal the shell already wires; the composer's block and its control survive it,
/// which is the point of blocking send rather than trapping him in a modal.
async function dismissEntityPicker(page) {
  const open = await page.evaluate(() => !document.getElementById("entity-picker").hidden);
  if (!open) return;
  await page.keyboard.press("Escape");
  await page.waitForSelector("#entity-picker", { state: "hidden" });
}

/// Press one of the desk's answer buttons and wait for the re-read that follows it. The
/// notice is the observable: every answer either states its outcome or relays a refusal,
/// and this waits for whichever it was rather than for a fixed delay.
async function deskAnswer(page, selector) {
  await page.click(selector);
  await page.waitForFunction(
    () => !document.getElementById("corrections-notice").hidden, null,
    { timeout: 5000 }
  );
  return page;
}


/// Open the feedback panel through the button the CEO presses, after driving `mock.js`'s
/// store into one state.
async function openFeedback(browser, setup) {
  const page = await openApp(browser);
  if (setup) await page.evaluate("(" + setup.toString() + ")(window.__RICHOS_MOCK__)");
  await page.click("#nav-feedback");
  await page.waitForSelector("#feedback-overlay:not([hidden])");
  return page;
}

/// Press one of the feedback surface's answer buttons and wait for the notice it produces.
async function feedbackAnswer(page, selector) {
  await page.click(selector);
  await page.waitForFunction(() => !document.getElementById("feedback-notice").hidden, null, { timeout: 30000 });
  return page;
}

/// The smallest report that can be assembled: one class, one count, one diagnosis term and
/// no conditions. Deliberately minimal — the empty condition list is omitted from the
/// rendered report entirely, so this drives that branch as well.
async function chooseMinimalReport(page) {
  await page.check('#feedback-choose input[name="failure_class"]:first-of-type');
  await page.check('#feedback-choose input[name="occurrences_this_session"]:first-of-type');
  await page.check('#feedback-choose input[name="generic_diagnosis"][value="no-method-given"]');
  await page.waitForFunction(() => !document.getElementById("feedback-show-preview").disabled);
}

/// Defer the setting up, then send — the only path a CEO has to the refusal, and therefore
/// the only honest way to put its sentence on screen. Returns with the notice rendered AND
/// the sheet reopened, which is the pair the affordance rule is about: the sentence names
/// "Set it up", so "Set it up" has to be there.
async function setupRefusal(browser, preset) {
  const page = await openApp(browser, undefined, { setup: preset, memory: "ready" });
  await page.waitForSelector("#setup-sheet:not([hidden])");
  await page.click("#setup-later");
  await page.waitForSelector("#setup-sheet", { state: "hidden" });
  await page.fill("#input", "book the Acme call for Thursday");
  await page.click("#send");
  await page.waitForSelector("#setup-sheet:not([hidden])");
  // `.tl-notice-body` as well as `.tl-prose`: a refusal like this one is the app talking
  // about itself, and since 2026-09-17 it renders as a status line rather than as something
  // Rich said (`timeline.js`'s `renderLocalNotice`, Ray's candidate-.4 finding #8). The
  // sentence is unchanged; the lane it lands in is not.
  await page.waitForFunction(() =>
    [...document.querySelectorAll("#messages .tl-prose, #messages .tl-notice-body")].some((n) =>
      /take that on yet/.test(n.textContent)
    )
  );
  return page;
}

// ---------------------------------------------------------------------------------------
// The update row and the home screen's panel — the two surfaces the widened source list
// brought under this rule on 2026-09-05
// ---------------------------------------------------------------------------------------

/// A full `update_state` view with only the interesting fields spelled out at the call site.
///
/// THE DEFAULTS ARE `updates.rs`'s OWN, not invented here: a placeholder endpoint, nothing
/// checked, nothing busy. A fixture that quietly started from `available` would be showing
/// this rule a screen the product does not open on. This mirrors `tests/updates.js`'s
/// `view()` deliberately — one shape, two suites, and neither one is a second definition of
/// what a state is, because both hand it to the same `updateSet`.
function updateView(over) {
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
    over || {}
  );
}

/// Put the update row into one state and open the menu it lives in, through the real
/// settings button. `onOpen()` re-reads `update_state` when the menu opens, so the state is
/// set BEFORE the click and the row paints from the mock's answer rather than from the event.
async function openUpdates(browser, over) {
  const page = await openApp(browser);
  await page.evaluate((v) => window.__RICHOS_MOCK__.updateSet(v, []), updateView(over));
  await page.click("#set-btn");
  await page.waitForSelector("#set-menu", { state: "visible" });
  await page.waitForSelector("#update-line");
  // The row repaints on the menu's own `onOpen` read. Wait for the STATE to arrive rather
  // than for a duration: a fixed sleep here would be a fixture that passes on a fast machine.
  await page.waitForFunction(
    (want) => {
      const row = document.getElementById("set-updates");
      return !!row && row.getAttribute("data-update-state") === want;
    },
    updateView(over).state,
    { timeout: 5000 }
  );
  return page;
}

/// Open the home screen's company-buttons panel the only way the CEO can: the settings
/// menu's "Company buttons…" row. `setup` runs after the shell is up and before the panel
/// is opened, which is where a fixture overrides what `home_entity_row` answers.
async function openHomePrefs(browser, setup) {
  const page = await openApp(browser);
  if (setup) await setup(page);
  await page.click("#set-btn");
  await page.waitForSelector("#set-menu", { state: "visible" });
  await page.click("#set-home-open");
  await page.waitForSelector("#home-prefs:not([hidden])");
  await page.waitForFunction(() => {
    const foot = document.getElementById("home-prefs-foot");
    return !!foot && foot.textContent.trim().length > 0;
  }, null, { timeout: 30000 });
  return page;
}

async function questionState(browser, mode) {
  const page = await openApp(browser);
  await page.evaluate(mode => {
    const q = {id:"affordance-question", thread_id:"general", text:"When should the release ship?",
      options:[{id:"today",label:"Ship today",description:"Earlier fixes"},{id:"tomorrow",label:"Ship tomorrow",description:"More testing"}],
      multiple:false, free_answer:true, state:"open", revision:0, delivered:false};
    const saved = {answer:{question_id:q.id,client_id:"saved-answer",option_ids:["today"],text:""},
      method:"click",threadId:"general",display:"Ship today",deleted:mode==="deleted"};
    if (mode==="saved" || mode==="deleted") localStorage.setItem("richos-question-answer:"+q.id,JSON.stringify(saved));
    if (mode==="edit") {q.state="answered";q.revision=1;q.answer={option_ids:["today"],text:"",method:"click",surface:"mac"};}
    if (mode==="retry") window.__overrides.answer_question={reject:true,value:"Lost receipt"};
    if (mode==="storage") {
      const write=Storage.prototype.setItem;
      Storage.prototype.setItem=function(key,value) {if(key.startsWith("richos-question-answer:"))throw new Error("Storage full");return write.call(this,key,value);};
    }
    document.getElementById("messages").appendChild(RichQuestions.render({threadId:"general",question:q}));
    document.dispatchEvent(new Event("richos-questions-changed"));
  }, mode);
  await page.locator(".question-card.ask").scrollIntoViewIfNeeded();
  if(mode==="edit") await page.getByRole("button",{name:"Change answer",exact:true}).click();
  if(mode==="retry" || mode==="storage") await page.locator(".question-option").first().click();
  return page;
}

// What quota.rs publishes for an account Claude Code says is not signed in (`ReadError::SignedOut`).
const SIGNED_OUT_QUOTA = { state: "unavailable", windows: [], checkedAt: null, retryAt: null,
  message: "This Claude account is not signed in on this Mac.", signedIn: false };

const FIXTURES = {
  "question-edit": browser => questionState(browser,"edit"),
  "question-saved": browser => questionState(browser,"saved"),
  "question-retry": browser => questionState(browser,"retry"),
  "question-deleted": browser => questionState(browser,"deleted"),
  "question-storage": browser => questionState(browser,"storage"),
  async "connect-setup"(browser) {
    const page = await openApp(browser, undefined, {phoneConnect:{enabled:false,phase:"not-configured"}});
    await page.click("#set-btn"); await page.click("#set-phone-open");
    await page.waitForSelector("#phone-connect-start",{state:"visible"}); return page;
  },
  ...Object.fromEntries([
    "connect-setup-unreadable", "connect-helper-missing", "connect-history-full",
    "connect-history-unreadable", "connect-unavailable", "connect-address-invalid", "connect-credential-invalid",
  ].map(name => [name, async (browser) => {
    const page = await FIXTURES["connect-setup"](browser);
    const entry = REGISTRY.find(row => row.fixture === name);
    assert(entry, "Missing Connect error classification: " + name);
    await page.evaluate(text => { window.__overrides.phone_connect_enable = {reject:true,value:text}; },entry.s);
    await page.click("#phone-connect-start");
    await page.waitForFunction(text => document.getElementById("phone-message").textContent === text, entry.s);
    return page;
  }])),
  async "repository-empty"(browser) {
    const page = await openApp(browser);
    await page.click("#set-btn");
    await page.click("#set-repositories-open");
    await page.waitForSelector("#repository-company option:nth-child(2)", {state: "attached"});
    return page;
  },
  async "provider-start-error"(browser) {
    const page = await openApp(browser, undefined, {providerAuth: "signed-out"});
    await page.waitForSelector("#provider-connect:not([hidden])");
    await page.evaluate(() => { window.__overrides.provider_auth_start = {reject: true, value: "synthetic start failure"}; });
    await page.click("#provider-connect");
    await page.waitForFunction(() => document.getElementById("setup-account").textContent.includes("could not start"));
    return page;
  },
  async "provider-poll-error"(browser) {
    const page = await openApp(browser, undefined, {providerAuth: "signed-out", providerAuthHold: true});
    await page.waitForSelector("#provider-connect:not([hidden])");
    await page.evaluate(() => { window.__overrides.provider_auth_poll = {reject: true, value: "synthetic poll failure"}; });
    await page.click("#provider-connect");
    await page.waitForFunction(() => document.getElementById("setup-account").textContent.includes("could not be verified"));
    return page;
  },
  async "provider-cancel-error"(browser) {
    const page = await openApp(browser, undefined, {providerAuth: "signed-out", providerAuthHold: true});
    await page.waitForSelector("#provider-connect:not([hidden])");
    await page.evaluate(() => {
      window.__overrides.provider_auth_cancel = {reject: true, value: "synthetic cancel failure"};
      window.__overrides.provider_auth_poll = {value: {state: "connecting", message: "Sign-in could not be canceled. Try again."}};
    });
    await page.click("#provider-connect");
    await page.waitForSelector("#provider-cancel:not([hidden])");
    await page.click("#provider-cancel");
    await page.waitForFunction(() => document.getElementById("setup-account").textContent.includes("could not be canceled"));
    return page;
  },
  /// THE ACCOUNT IN USE, SIGNED OUT (a new user's first run, 2026-10-21). The view says what
  /// quota.rs says after Claude Code's own `auth status` answered "not signed in": no figures,
  /// `signedIn: false`. The card must say so with Sign in on it, never "No reading yet".
  async "accounts-signed-out"(browser) {
    const page = await openApp(browser, undefined, { quota: SIGNED_OUT_QUOTA });
    await page.click("#set-btn");
    await page.waitForFunction(() => document.getElementById("set-accounts-state").textContent.length > 0);
    await page.click("#set-accounts-open");
    await page.waitForSelector("#accounts-sheet:not([hidden]) .acc-card");
    return page;
  },
  /// THE CHECK STILL UNDER WAY: no reading, no answer yet, nothing known about the sign-in. The
  /// card says the check is under way, never "No reading yet" (2026-10-21).
  async "accounts-checking"(browser) {
    const page = await openApp(browser, undefined, { quota: { state: "unavailable", windows: [], checkedAt: null,
      retryAt: null, message: null } });
    await page.click("#set-btn");
    await page.waitForSelector("#set-accounts-open", { state: "visible" });
    await page.click("#set-accounts-open");
    await page.waitForSelector("#accounts-sheet:not([hidden]) .acc-card");
    return page;
  },
  async "quota-signed-out"(browser) {
    const page = await openApp(browser, undefined, { quota: SIGNED_OUT_QUOTA });
    await page.click("#set-btn");
    await page.click("#set-techy");
    await page.waitForFunction(() => !document.getElementById("set-quota-open").hidden || !document.getElementById("techy-scope").hidden);
    if (await page.locator("#techy-scope").isVisible()) await page.click("#techy-scope-confirm");
    if (await page.locator("#set-menu").isHidden()) await page.click("#set-btn");
    await page.click("#set-quota-open");
    await page.waitForSelector("#quota-empty:not([hidden])");
    return page;
  },
  /// The same state on the Settings row, before the sheet is opened.
  async "accounts-row-signed-out"(browser) {
    const page = await openApp(browser, undefined, { quota: SIGNED_OUT_QUOTA });
    await page.click("#set-btn");
    await page.waitForFunction(() => document.getElementById("set-accounts-state").textContent.length > 0);
    return page;
  },
  /// THE SAME FAILURE AGAIN (`InterruptionCause::Repeated`, 2026-10-21): a conversation whose
  /// last turn ended on the repeated card, served through the real `get_timeline` path and drawn
  /// by the real renderer. Its sentence is the registry row's own, so the row and the screen
  /// cannot drift apart (review rv-20261021T193611Z-6df3f1f3-b313, finding 2).
  async "interruption-repeated"(browser) {
    const page = await openApp(browser);
    const entry = REGISTRY.find(row => row.fixture === "interruption-repeated");
    assert(entry, "no registry row for the repeated interruption");
    const threads = page.locator(".nav-thread");
    await threads.nth(0).click();
    await page.waitForFunction(() => window.__RICHOS_TIMELINE__ && window.__RICHOS_TIMELINE__().threadId);
    await page.evaluate(async (ceoMessage) => {
      const threadId = window.__RICHOS_TIMELINE__().threadId;
      const real = await window.RichBridge.invoke("get_timeline", { threadId });
      const turn = "turn_repeated_fixture";
      const base = (id, extra) => Object.assign({ id, entityId: real.entityId, threadId, turnId: turn,
        bindingRevision: real.bindingRevision, createdAt: Date.now() - 5000, sequence: null,
        slot: "stream", visibility: "ceo" }, extra);
      const items = (real.items || []).concat([
        Object.assign(base(turn + ":user", { slot: "opening" }), { kind: "user_message",
          text: "Test to see if something is working.", source: "text" }),
        Object.assign(base(turn + ":duration", { slot: "terminal" }), { kind: "work_duration",
          state: "interrupted", startedAt: Date.now() - 5000, endedAt: Date.now() - 2000, activeMs: 3000 }),
        Object.assign(base(turn + ":interruption", { slot: "terminal" }), { kind: "turn_interruption",
          cause: "repeated", ceoMessage, lossMessage: "Your message is saved. Rich hadn't written any of his answer yet.",
          offersRetry: false }),
      ]);
      window.__overrides.get_timeline = { value: Object.assign({}, real, { items }) };
    }, entry.s);
    // Away and back, so the conversation is read again through the overridden command.
    await threads.nth(1).click();
    await threads.nth(0).click();
    await page.waitForFunction(text => document.body.innerText.includes(text), entry.s);
    return page;
  },
  /// The app as it opens. Proves the controls that the voice instructions NAME are on the
  /// screen those instructions render on.
  async shell(browser) {
    return openApp(browser);
  },

  /// GETTING THE SPEECH MODEL — a machine with a decoder and no weights, which is the state a
  /// fresh Mac that has run `brew install whisper-cpp` is actually in, and the one
  /// `.github/README.md`'s "Voice does not work yet" described.
  ///
  /// NO MICROPHONE IS OPENED BY ANY OF THESE THREE, and that is the invariant they exist to hold
  /// as much as the controls are. `enterVoiceMode` calls `start_voice_capture` only when readiness
  /// says `ready`; in `model-missing` it opens this panel and touches no device. A fixture that
  /// reached these rows through a hot mic would be rehearsing the 2026-09-04 defect.
  async "voice-model-offer"(browser) {
    const page = await openApp(browser, undefined, { voice: "model-missing" });
    await page.waitForSelector("#talk-toggle:not([hidden])");
    await page.click("#talk-toggle");
    await page.waitForSelector("#voice-state-model-offer:not([hidden])");
    return page;
  },

  /// The same machine, after a transfer that stopped. Driven through the real event the shell
  /// emits (`rich://voice-model`, phase `failed`) rather than by writing into the DOM, so what is
  /// asserted is the renderer's own response to the backend's own payload.
  async "voice-model-failed"(browser) {
    const page = await openApp(browser, undefined, { voice: "model-missing" });
    await page.waitForSelector("#talk-toggle:not([hidden])");
    await page.click("#talk-toggle");
    await page.waitForSelector("#voice-state-model-offer:not([hidden])");
    await page.evaluate(() =>
      window.__RICHOS_MOCK__.voiceModelEmit({
        phase: "failed",
        modelId: "small.en",
        received: 2438071,
        total: 487614201,
        totalLabel: "487.6 MB",
        message:
          "I've stopped the download. What arrived is saved, so asking me again picks up where it left off rather than starting over.",
        askAgain: true,
        at: Date.now(),
      })
    );
    await page.waitForSelector("#voice-state-model-failed:not([hidden])");
    return page;
  },

  /// And after it finished and verified.
  async "voice-model-installed"(browser) {
    const page = await openApp(browser, undefined, { voice: "model-missing" });
    await page.waitForSelector("#talk-toggle:not([hidden])");
    await page.click("#talk-toggle");
    await page.waitForSelector("#voice-state-model-offer:not([hidden])");
    await page.evaluate(() =>
      window.__RICHOS_MOCK__.voiceModelEmit({
        phase: "installed",
        modelId: "small.en",
        received: 487614201,
        total: 487614201,
        totalLabel: "487.6 MB",
        message: null,
        askAgain: false,
        at: Date.now(),
      })
    );
    await page.waitForSelector("#voice-state-model-installed:not([hidden])");
    return page;
  },

  /// THE LAUNCH THE CEO PERFORMS. A double-clicked bundle has working directory `/`, which
  /// owns no entity, so nothing resolves and nothing has ever been chosen — the app asks,
  /// blocks send, and renders the control that clears the block. Measured against the real
  /// f44f89a bundle on 2026-09-01: the picker opened, focus was taken off it by `init()`'s
  /// closing `inputEl.focus()`, and the first sentence typed was refused.
  async "company-unchosen"(browser) {
    const page = await openApp(browser, undefined, { chosenEntity: null });
    await page.waitForSelector("#entity-picker:not([hidden])");
    return page;
  },




  /// THE FRESH INSTALL. No corpus anywhere — which is precisely what the signed bundle
  /// reported on 2026-09-01 the moment the hand-made `loro-root` pointer was removed
  /// (`docs/verification/first-run-provisioning-2026-09-01/raw/boot-1-no-pointer.log`):
  /// three candidates, three "not present", and nothing in the product creating any of them.
  /// The memory question opens ahead of the company question, one dialog at a time.
  async "memory-unprovisioned"(browser) {
    const page = await openApp(browser, undefined, { memory: "none" });
    await page.waitForSelector("#memory-setup:not([hidden])");
    return page;
  },

  /// A corpus resolved and the program that reads it is not installed — the honest state of
  /// a machine whose corpus was just created and whose compiler ships from nowhere.
  ///
  /// REACHED BY PRESSING THE BUTTON, not by booting into it. Until 2026-09-04 this state
  /// opened its dialog at every launch, which on such a machine is every launch forever
  /// (ray-opus-a1, finding 4); it is now said once, as the answer to the press that produced
  /// it. So the fixture provisions with no compiler present, exactly as he would.
  async "memory-no-compiler"(browser) {
    // Since the product hunt of 2026-10-15 (finding 15) the press closes the sheet whatever the
    // backend answers, so the sentence is said where he looks: the "Memory folder" row.
    const page = await openApp(browser, undefined, { memory: "no-compiler" });
    await page.click("#set-btn");
    await page.waitForFunction(() => !document.getElementById("set-menu").hidden);
    await page.click("#set-memory-open");
    await page.waitForSelector("#memory-setup-close:not([hidden])");
    return page;
  },

  /// A CUSTOMER'S MAC — no Claude Code, no engine directory. `ceo-decisions.md` §19: "today
  /// RichOS runs on his Mac and would not run on anyone else's", and the engine "ships in no
  /// payload and has no route onto another machine at all". Every machine but the CEO's is
  /// in this state, so it is the first screen a customer ever sees.
  async "setup-missing-both"(browser) {
    const page = await openApp(browser, undefined, { setup: "missing-both" });
    await page.waitForSelector("#setup-sheet:not([hidden])");
    return page;
  },

  /// CLAUDE CODE IS THERE AND THE ENGINE IS NOT — a customer who installed Claude Code
  /// himself. It has its own fixture because the singular title and the plural one cannot
  /// both render at once, and asserting one against a fixture showing the other proves
  /// nothing.
  async "setup-missing-engine"(browser) {
    const page = await openApp(browser, undefined, { setup: "missing-engine" });
    await page.waitForSelector("#setup-sheet:not([hidden])");
    return page;
  },

  /// A build carrying no engine pin. It CANNOT install one, so it explains and names the
  /// party who can, rather than drawing a button that would certainly fail.
  async "setup-unpinned"(browser) {
    const page = await openApp(browser, undefined, { setup: "unpinned" });
    await page.waitForSelector("#setup-sheet:not([hidden])");
    return page;
  },

  /// The end of a successful run: everything installed, nothing left to press.
  async "setup-finished"(browser) {
    const page = await openApp(browser, undefined, { setup: "missing-both" });
    await page.waitForSelector("#setup-sheet:not([hidden])");
    await page.click("#setup-go");
    await page.waitForSelector("#setup-close:not([hidden])");
    return page;
  },

  /// HE DEFERRED THE SETTING UP AND THEN TRIED TO SEND. The three arms of
  /// `setup_view::incomplete_message`, each driven the only way the CEO can reach it: press
  /// "Not now", type, press Send.
  ///
  /// This is the state ray-opus-a2 was in on published v1.0.1 — except that there the refusal
  /// read "Quit RichOS and open it again", on a machine where quitting would find the same
  /// absent engine and fail the same way. The sentence now names the missing piece and puts
  /// the sheet back, so `#setup-go` is the control it names and the control on screen.
  async "setup-refused-engine"(browser) {
    return setupRefusal(browser, "missing-engine");
  },
  async "setup-refused-claude"(browser) {
    return setupRefusal(browser, "missing-claude");
  },
  async "setup-refused-both"(browser) {
    return setupRefusal(browser, "missing-both");
  },

  /// THE FIRST-RUN NOTICE, offering the interview. Every sentence in it is ACTIONABLE and
  /// names one of the two controls beside it, which is the whole shape of this surface: a
  /// state the CEO can change, rendered together with the things that change it.
  async "first-run-offer"(browser) {
    const page = await openApp(browser, undefined, { onboarding: "not-yet" });
    await page.waitForSelector("#first-run:not([hidden])");
    await page.waitForSelector("#first-run-actions:not([hidden])");
    return page;
  },

  /// AND THE ONE STATE ON IT HE CANNOT FIX. Its own fixture rather than a variant of the
  /// one above, for the reason `company-registry-unreadable` has its own: "I could not read
  /// your notes" and "you have not written any" are different repairs, and this one draws no
  /// control at all because no control could do it.
  async "first-run-partial"(browser) {
    const page = await openApp(browser, undefined, { onboarding: "partial" });
    await page.waitForSelector('#first-run[data-state="partial"]:not([hidden])');
    return page;
  },

  async "first-run-unusable"(browser) {
    const page = await openApp(browser, undefined, { onboarding: "unusable" });
    await page.waitForSelector('#first-run[data-state="unusable"]:not([hidden])');
    return page;
  },

  /// HE PRESSED "Not now" AND THE WRITE FAILED. Driven the only way the CEO reaches it —
  /// press the button — because the sentence is the whole point of the arm: the alternative
  /// is a notice that closes over a write that never happened.
  async "first-run-decline-refused"(browser) {
    const page = await openApp(browser, undefined, {
      onboarding: "not-yet",
      onboardingDeclineFails: true,
    });
    await page.waitForSelector("#first-run:not([hidden])");
    await page.click("#first-run-later");
    await page.waitForSelector("#first-run-error:not([hidden])");
    return page;
  },

  /// The same variable, naming something this build does not have — so it pins the answer
  /// AND fails to produce one. No picker opens, because every answer would be refused.
  async "company-pinned-unresolved"(browser) {
    return openApp(browser, undefined, { pinnedByEnvironment: true, chosenEntity: null });
  },

  /// A FIRST LAUNCH ON SOMEBODY ELSE'S MAC (2026-09-04). No company registered anywhere, so
  /// the picker has nothing to list — and until today that state could not exist: the
  /// registry was a `const` table of the app author's six companies, compiled into the
  /// binary, so every install on earth opened this dialog holding six wrong answers.
  async "company-none-registered"(browser) {
    const page = await openApp(browser, undefined, { chosenEntity: null, registry: "absent" });
    await page.waitForSelector("#entity-picker:not([hidden])");
    await page.waitForSelector("#entity-add:not([hidden])");
    return page;
  },

  /// The registry file is there and could not be read. A DIFFERENT state from the one above,
  /// and it has its own fixture for the reason it has its own sentence: answering "I could
  /// not read your list" with "you have not given me one" would invite him to re-enter a
  /// list that is already on disk one typo away from working.
  async "company-registry-unreadable"(browser) {
    const page = await openApp(browser, undefined, { chosenEntity: null, registry: "unreadable" });
    await page.waitForSelector("#entity-picker:not([hidden])");
    await page.waitForSelector("#entity-add:not([hidden])");
    return page;
  },

  /// §21 entity binding failure: the seeded `legacy` thread has `entity_id: null`, and the
  /// mock's `get_timeline` refuses it exactly as the real command does.
  async unbound(browser) {
    const page = await openApp(browser);
    await page.click('.nav-thread[data-thread-id="legacy"]');
    await page.waitForSelector("#unbound-view:not([hidden])");
    return page;
  },

  /// §14: a turn still `working` on disk with nothing live for it in this session.
  async "unknown-turn"(browser) {
    const page = await openApp(browser);
    await loadSnapshot(page, snapshotWithDuration("working", null));
    await page.waitForSelector(".tl-intervention--quiet");
    return page;
  },

  /// §5.5: a turn that ended without finishing, AND the §18 announcement that goes with it.
  /// The announcement is replayed through the shipping `rich://turn-status` handler rather
  /// than written into #live-region, because the thing worth proving is that when the app
  /// says "Rich stopped before finishing" to a screen reader, the card's control is in the
  /// DOM beside it — and only the real handler decides whether that sentence is said at all.
  async "failed-turn"(browser) {
    const page = await openApp(browser);
    await loadSnapshot(page, snapshotWithDuration("interrupted", 461000));
    await page.waitForSelector(".tl-intervention:not(.tl-intervention--quiet)");
    // The binding is read OFF the model rather than typed here, because §13's fence is real:
    // an event whose entityId/threadId do not match is rejected, and a hand-typed payload
    // that silently fails the fence would make this fixture prove nothing.
    await page.evaluate((t) => {
      const m = window.__RICHOS_TIMELINE__();
      window.__emit("rich://turn-status", {
        entityId: m.entityId,
        threadId: m.threadId,
        bindingRevision: m.bindingRevision,
        turnId: t,
        status: "failed",
      });
    }, TURN);
    await page.waitForFunction(() =>
      (document.getElementById("live-region").textContent || "").length > 0
    );
    return page;
  },

  /// `create_thread_in` refuses on the first send in a draft thread. Driven the way the CEO
  /// gets there: open an entity area, type, press Enter.
  async "thread-create-refused"(browser) {
    const page = await openApp(browser);
    await page.click(".nav-group-label");
    await page.waitForSelector("#entity-view:not([hidden])");
    await page.evaluate(() => {
      window.__overrides.create_thread_in = { reject: true, value: "scope mismatch on thread thr_x" };
    });
    await page.fill("#input", "start the Acme workstream");
    await page.press("#input", "Enter");
    await page.waitForSelector("#composer-blocked:not([hidden])");
    return page;
  },

  /// The mic is open and healthy and delivering nothing. `start_voice_capture` is unwired
  /// in the mock, so it is overridden to resolve — and then the panel's state comes from
  /// the SHIPPING `rich://voice-state` handler, replayed with the payload shape main.js's
  /// own contract comment documents. Nothing about the row is set by this test.
  async "voice-no-audio"(browser) {
    const page = await openApp(browser);
    await page.evaluate(() => {
      window.__overrides.start_voice_capture = { value: {} };
    });
    await page.click("#talk-toggle");
    await page.waitForSelector("#voice-panel:not([hidden])");
    await page.evaluate(() => {
      window.__emit("rich://voice-state", { state: "hearing", level: 0, noAudio: true, at: Date.now() });
    });
    await page.waitForSelector("#voice-state-no-audio:not([hidden])");
    return page;
  },

  /// A live turn, from the mock's own `simulateSlowTurn`, with words in the composer — the
  /// §9.2 mode where Stop and Send are both up.
  async "opening-conversation"(browser) {
    const page = await FIXTURES["working-turn"](browser);
    await page.evaluate(() => {
      const invoke = window.RichBridge.invoke.bind(window.RichBridge);
      window.RichBridge.invoke = (cmd, args) => cmd === "switch_thread" ? new Promise(() => {}) : invoke(cmd, args);
    });
    await page.click('.nav-thread[data-thread-id="acme"]');
    await page.waitForSelector('#composer-row[data-mode="opening"]');
    return page;
  },
  async "returning-conversation"(browser) {
    const page = await FIXTURES["opening-conversation"](browser);
    const thread = await page.evaluate(() => window.RichBridge.invoke("active_context"));
    await page.locator(`.nav-thread[data-thread-id="${thread.thread_id}"]`).click();
    await page.waitForFunction(() => document.querySelector(".wait-detail").textContent.startsWith("This conversation"));
    return page;
  },
  async "returning-stop-failed"(browser) {
    const page = await FIXTURES["returning-conversation"](browser);
    await page.evaluate(() => { window.__overrides.stop_turn = { reject: true, value: "temporary disk error" }; });
    await page.click("#stop");
    await page.waitForFunction(() => document.querySelector(".wait-detail").textContent.includes("couldn't stop this"));
    return page;
  },
  async "opening-stop-failed"(browser) {
    const page = await FIXTURES["opening-conversation"](browser);
    await page.evaluate(() => { window.__overrides.stop_turn = { reject: true, value: "temporary disk error" }; });
    await page.click("#stop");
    await page.waitForFunction(() => document.querySelector(".wait-detail").textContent.includes("couldn't stop"));
    return page;
  },

  async "working-turn"(browser) {
    const page = await openApp(browser);
    await page.evaluate(() => window.__RICHOS_MOCK__.simulateSlowTurn(null, "run the numbers", 200));
    await page.waitForSelector('#composer-row[data-mode="working"]');
    await page.fill("#input", "and check the Q4 line");
    await page.waitForSelector("#send:not([hidden])");
    return page;
  },

  // ---- techy mode (open-items row 3.1) ------------------------------------------------
  //
  // All three go through the CEO's own path — open a thread, press the shortcut, take a
  // scope — rather than setting a flag. The state under test is what the SHIPPING
  // `loadTimeline` branch produces, and a fixture that painted it would prove the fixture.
  //
  // THE SHORTCUT IS TWO ACTS SINCE 2026-09-18. ⌘⇧T opens the three-way scope sheet like
  // every other entrance (CEO §9.1 — his sentence has no exception in it), so each driver
  // below answers it with "For this conversation only", which is the state these three
  // fixtures were walked in before the sheet existed. One helper rather than three copies,
  // so the next change to the sheet has one place to land instead of three that can drift.

  /// The technical view of a thread that HAS machinery, with the two raw panes that carry
  /// their own sentences already open: an evicted Tier-B payload and a truncated one.
  async "techy-on"(browser) {
    const page = await openApp(browser);
    await page.click('.nav-thread[data-thread-id="acme"]');
    await page.waitForSelector(".tl-turn");
    await page.keyboard.press("Meta+Shift+T");
    await techyScopeThisConversation(page);
    await page.waitForSelector(".tl-tech");
    await page.click("#mach\\:mach_a5"); // raw window has passed over this one
    await page.click("#mach\\:mach_a6"); // over the 32 KB cap
    await page.waitForSelector("#raw\\:mach_a6 .tl-tech-note");
    return page;
  },

  /// A conversation from before the routing commit: the journal is readable and this thread
  /// is not in it. THE honest empty state.
  async "techy-empty"(browser) {
    const page = await openApp(browser);
    await page.click('.nav-thread[data-thread-id="partner"]');
    await page.waitForSelector(".tl-turn");
    await page.keyboard.press("Meta+Shift+T");
    await techyScopeThisConversation(page);
    await page.waitForSelector('#techy-state[data-state="nothing_recorded"]');
    return page;
  },

  /// The store is there and the OS refuses it — `breakMachinery` is the harness's stand-in
  /// for the `chmod 000` that `journal.rs`'s own test uses. NOT the empty state.
  async "techy-unreadable"(browser) {
    const page = await openApp(browser);
    await page.evaluate(() => window.__RICHOS_MOCK__.breakMachinery("acme"));
    await page.click('.nav-thread[data-thread-id="acme"]');
    await page.waitForSelector(".tl-turn");
    await page.keyboard.press("Meta+Shift+T");
    await techyScopeThisConversation(page);
    await page.waitForSelector('#techy-state[data-state="unreadable"]');
    return page;
  },

  /// §3.5 entity overview, reached the way the CEO reaches it.
  async "entity-view"(browser) {
    const page = await openApp(browser);
    await page.click(".nav-group-label");
    await page.waitForSelector("#entity-view:not([hidden])");
    return page;
  },

  /// §3.4 search with no hits.
  async "search-empty"(browser) {
    const page = await openApp(browser);
    await page.click("#nav-search");
    await page.waitForSelector("#search-overlay:not([hidden])");
    await page.fill("#search-input", "zzzqqqxyzzy");
    await page.waitForSelector("#search-empty:not([hidden])");
    return page;
  },

  /// The lease is down. The mock's `_notConnected` switch rejects `send_message` before any
  /// turn starts, which is what main.rs does when the spine holds no lease — so this is the
  /// real refusal path through the real send handler, started by typing and pressing Enter.
  async "not-connected"(browser) {
    const page = await openApp(browser);
    await page.evaluate(() => window.__RICHOS_MOCK__.setNotConnected(true));
    await page.fill("#input", "book the Acme call for Thursday");
    await page.press("#input", "Enter");
    await page.waitForFunction(() =>
      (document.getElementById("messages").innerText || "").includes("back in the box below")
    );
    return page;
  },

  /// The rail's status marks. Nothing is driven: the seeded `legacy` thread carries the
  /// `unbound` mark on load, and the structural claim is checked over every mark present.
  async "rail-mark"(browser) {
    return openApp(browser);
  },


  // -------------------------------------------------------------------------------------
  // THE CORRECTION DESK. Every one of these opens the REAL panel through the REAL rail
  // button and drives its state through `mock.js`'s own desk, which enforces the same
  // state machine `correction.rs` and `staging.rs` do — a fixture that let `confirm`
  // succeed twice would prove a product that does not exist.
  // -------------------------------------------------------------------------------------

  /// Both desks running, one loro proposal and one spoken candidate waiting.
  async corrections(browser) {
    return openDesk(browser);
  },

  /// Neither desk is installed. `loro_available` and `spoken_corrections_available` both
  /// return false and every read refuses with the backend's own sentence about THIS
  /// install — the state that must never render as an empty list.
  async "corrections-off"(browser) {
    return openDesk(browser, (m) => {
      m.setLoroAvailable(false);
      m.setSpokenCorrectionsAvailable(false);
    });
  },

  /// The desk IS there and a read refused. Driven with the entity sentence, because that
  /// is the real one: `loro_pending_corrections` resolves the entity before it touches the
  /// desk, so on a launch with no entity `loro_available` is true and the read is what
  /// fails.
  async "corrections-read-failed"(browser) {
    // BOOTED WITH NO COMPANY CHOSEN, which is the launch that actually produces this
    // refusal — `loro_pending_corrections` resolves the entity before it touches the desk.
    // Driving it that way rather than injecting the sentence alone is also what puts the
    // control the sentence now names (`#choose-company-btn`) on the screen underneath.
    return openDesk(browser, (m) => {
      m.setLoroReadFailure(
        "I can't tell which company this work belongs to, so I won't guess. Filing it under " +
          "the wrong one would mix two companies' records together and that's not a mistake " +
          "worth risking to save you a question. Pick the company and I'll keep everything " +
          "under it from then on."
      );
    }, { chosenEntity: null });
  },

  /// Both desks readable, nothing waiting — answered rather than absent.
  async "corrections-empty"(browser) {
    const page = await openDesk(browser);
    await deskAnswer(page, "#desk-loro-list .desk-btn--never");
    await deskAnswer(page, "#desk-spoken-list .desk-btn--never");
    // ...and then lifted again, so the empty lines are the only thing on the section.
    await deskAnswer(page, "#desk-loro-suppressed-list .desk-btn--lift");
    await deskAnswer(page, "#desk-spoken-suppressed-list .desk-btn--lift");
    await page.waitForSelector("#desk-loro-empty:not([hidden])");
    return page;
  },

  async "corrections-written"(browser) {
    const page = await openDesk(browser);
    await deskAnswer(page, "#desk-loro-list .desk-btn--confirm");
    return page;
  },

  /// The writer refused the confirmed write — exit 5, a prose section. The proposal lands
  /// in `failed` with the reason kept, and the reason is an instruction to the CEO.
  async "corrections-write-failed"(browser) {
    const page = await openDesk(browser, (m) => {
      m.setLoroWriterRefusal(
        'the loro writer refused (exit 5): loro write: "wiki:loro-structure.md#the-human-surface" is a PROSE section — edit the page'
      );
    });
    await deskAnswer(page, "#desk-loro-list .desk-btn--confirm");
    return page;
  },

  async "corrections-loro-declined"(browser) {
    const page = await openDesk(browser);
    await deskAnswer(page, '#desk-loro-list .desk-btn:not(.desk-btn--confirm):not(.desk-btn--never):not(.desk-btn--show)');
    return page;
  },

  async "corrections-loro-never"(browser) {
    const page = await openDesk(browser);
    await deskAnswer(page, "#desk-loro-list .desk-btn--never");
    await page.waitForSelector("#desk-loro-suppressed:not([hidden])");
    return page;
  },

  async "corrections-learned"(browser) {
    const page = await openDesk(browser);
    await deskAnswer(page, "#desk-spoken-list .desk-btn--confirm");
    return page;
  },

  async "corrections-already-knew"(browser) {
    const page = await openDesk(browser, (m) => m.setVocabularyAlreadyKnew(true));
    await deskAnswer(page, "#desk-spoken-list .desk-btn--confirm");
    return page;
  },

  async "corrections-spoken-declined"(browser) {
    const page = await openDesk(browser);
    await deskAnswer(page, '#desk-spoken-list .desk-btn:not(.desk-btn--confirm):not(.desk-btn--never)');
    return page;
  },

  async "corrections-spoken-never"(browser) {
    const page = await openDesk(browser);
    await deskAnswer(page, "#desk-spoken-list .desk-btn--never");
    await page.waitForSelector("#desk-spoken-suppressed:not([hidden])");
    return page;
  },

  // -------------------------------------------------------------------------------------
  // THE FEEDBACK CHANNEL. Every one of these opens the REAL panel through the REAL rail
  // button and drives `mock.js`'s own store, which enforces the same rules `feedback.rs`
  // does — a fixture where a `3` could carry a report, or where an approval could be
  // recorded for text nobody was shown, would prove a product that does not exist.
  // -------------------------------------------------------------------------------------

  /// The panel as he first meets it: the question, the four keys, and an empty store.
  async feedback(browser) {
    return openFeedback(browser);
  },

  /// The one file would not open. The backend's own sentence, and NO keys — an answer that
  /// cannot be kept must not be asked for.
  async "feedback-unavailable"(browser) {
    return openFeedback(browser, (m) => m.setFeedbackAvailable(false));
  },

  /// A `3`. The offer is never made, because `FeedbackEntry::with_report` would refuse a
  /// report attached to it and a surface that offered one would be inviting a refusal.
  async "feedback-rated"(browser) {
    const page = await openFeedback(browser);
    await feedbackAnswer(page, '#feedback-keys .desk-btn[data-key="3"]');
    return page;
  },

  /// A `1`, the offer accepted, the vocabulary on screen.
  async "feedback-choosing"(browser) {
    const page = await openFeedback(browser);
    await page.click('#feedback-keys .desk-btn[data-key="1"]');
    await page.waitForSelector("#feedback-offer:not([hidden])");
    await page.click("#feedback-offer-yes");
    await page.waitForSelector("#feedback-choose:not([hidden])");
    return page;
  },

  /// ...and the report rendered, before he is asked to approve it.
  async "feedback-previewing"(browser) {
    const page = await FIXTURES["feedback-choosing"](browser);
    await chooseMinimalReport(page);
    await page.click("#feedback-show-preview");
    await page.waitForSelector("#feedback-preview-block:not([hidden])");
    return page;
  },

  async "feedback-approved"(browser) {
    const page = await FIXTURES["feedback-previewing"](browser);
    await feedbackAnswer(page, "#feedback-approve");
    return page;
  },

  /// The offer made and refused. A declined report is not a report — the payload is
  /// dropped and the rating is all that is kept.
  async "feedback-declined-report"(browser) {
    const page = await openFeedback(browser);
    await page.click('#feedback-keys .desk-btn[data-key="1"]');
    await page.waitForSelector("#feedback-offer:not([hidden])");
    await feedbackAnswer(page, "#feedback-offer-no");
    return page;
  },

  async "corrections-lifted"(browser) {
    const page = await openDesk(browser);
    await deskAnswer(page, "#desk-loro-list .desk-btn--never");
    await page.waitForSelector("#desk-loro-suppressed-list .desk-btn--lift");
    await deskAnswer(page, "#desk-loro-suppressed-list .desk-btn--lift");
    return page;
  },

  // -------------------------------------------------------------------------------------
  // THE UPDATE SURFACE — CEO ruling §26, and the file this rule could not see until
  // 2026-09-05.
  //
  // `updates.js` was outside `UI_SOURCES`, so every state below was outside the affordance
  // rule: `#update-relaunch`, `#update-install` and `#update-check` could each have been
  // deleted from `paint()` and this suite would have gone green over a sentence offering a
  // restart with nothing to press. These six drive the REAL row through the REAL settings
  // menu, with `mock.js`'s own `updateSet` standing in for `update_state` — the same driver
  // `tests/updates.js` uses, so neither suite is proving a second implementation.
  // -------------------------------------------------------------------------------------

  async "updates-idle"(browser) {
    return openUpdates(browser, {});
  },

  async "updates-unconfigured"(browser) {
    return openUpdates(browser, { state: "unconfigured" });
  },

  async "updates-ready"(browser) {
    return openUpdates(browser, { state: "ready", availableVersion: "0.1.2" });
  },

  /// THE WAY BACK, STAGED. The same `ready` state, the opposite direction, and a different
  /// promise — "RichOS will go back to 0.1.0 when you next open it" rather than "0.1.2 is
  /// ready for the next launch". It is its own fixture rather than an `over` on the one
  /// above for the reason `updates-install-failed` is: the registry claims a SENTENCE
  /// renders, and a fixture painting the update wording would have proved a claim nobody
  /// made. The control is absent here by design — what the next launch will do is already
  /// decided, and the crate refuses to stage a second thing over it.
  async "updates-rolling-back"(browser) {
    return openUpdates(browser, {
      state: "ready",
      availableVersion: "0.1.0",
      readyIsRollback: true,
      rollbackVersion: "0.1.0",
    });
  },

  async "updates-failed"(browser) {
    return openUpdates(browser, {
      state: "failed",
      failure: { kind: "network", headline: "The update did not complete.", detail: "connection reset" },
    });
  },

  /// THE OTHER FAILURE HEADLINE, and a separate fixture rather than a second `over` on the
  /// one above. `install_failure()` (updates.rs:271) writes a different sentence for a
  /// different cause — the download arrived and staging it failed — and the registry row
  /// claims THAT sentence renders with `#update-check`. A fixture that painted the network
  /// headline would have proved a claim nobody made.
  async "updates-install-failed"(browser) {
    return openUpdates(browser, {
      state: "failed",
      failure: {
        kind: "install",
        headline: "RichOS could not prepare this update.",
        detail: "staging directory is not writable",
      },
    });
  },

  /// A BUSY UPDATE RECORD (`Failure::of_journal`, kind `busy`), the state that replaced the
  /// 2026-10-21 "could not prepare" over "os error 35" on a plain check. Its row claims the
  /// headline renders with `#update-check`, so it gets its own fixture for the same reason
  /// the install failure does.
  async "updates-busy"(browser) {
    return openUpdates(browser, {
      state: "failed",
      failure: {
        kind: "busy",
        headline: "RichOS is busy with another update task. Try again in a minute.",
        detail: "the update record stayed in use by another part of RichOS",
      },
    });
  },

  /// THE §26 STATE ITSELF: an update is waiting and RichOS is working, so the control is
  /// REMOVED rather than dimmed and the sentence has to carry the whole answer on its own.
  async "updates-waiting-available"(browser) {
    return openUpdates(browser, {
      state: "available",
      availableVersion: "0.1.2",
      busy: true,
      busyReason: "Two specialists are running.",
    });
  },

  async "updates-waiting-ready"(browser) {
    return openUpdates(browser, {
      state: "ready",
      availableVersion: "0.1.2",
      busy: true,
      busyReason: "Two specialists are running.",
    });
  },

  // -------------------------------------------------------------------------------------
  // THE HOME SCREEN'S COMPANY-BUTTONS PANEL — `home.js`, also outside the old source list.
  // Opened through the settings menu's own row, which is the only way the CEO reaches it.
  // -------------------------------------------------------------------------------------

  async "home-prefs"(browser) {
    return openHomePrefs(browser);
  },

  /// The backend cannot name his companies. The panel opens anyway and says so, rather than
  /// rendering an empty list that reads as "you have none".
  async "home-prefs-no-entities"(browser) {
    return openHomePrefs(browser, async (page) => {
      await page.evaluate(() => {
        window.__overrides["home_entity_row"] = { value: [] };
      });
      await page.evaluate(() => window.RichHome.reloadEntities());
    });
  },
};

/// Which fixtures actually put the state's own words on screen. `shell` does not — the
/// voice `ceo_message()` lines need a real CaptureError from a live audio device — so for
/// those the suite proves the weaker, still-worth-proving thing: the control the sentence
/// names is present and usable on the screen the sentence appears on. Said here rather than
/// left as an unexplained asymmetry.
const TEXT_RENDERING_FIXTURES = new Set([
  "question-edit", "question-saved", "question-retry", "question-deleted", "question-storage",
  "connect-setup", "connect-setup-unreadable", "connect-helper-missing", "connect-history-full",
  "connect-history-unreadable", "connect-unavailable", "connect-address-invalid", "connect-credential-invalid",
  "repository-empty", "permission-pending", "provider-start-error", "provider-poll-error", "provider-cancel-error",
  // The signed-out account renders its own words from the view alone (2026-10-21).
  "accounts-signed-out", "accounts-row-signed-out", "accounts-checking", "interruption-repeated",
  // The assignment surface renders its own words out of `work-summary.js`, with nothing but
  // software behind it — and §7.8 says the wording IS the test, so the sentence is asserted
  // present rather than only the control beneath it.
  "assignment-waiting",
  // §58's question form of the same surface: the sentence is different and the control is the
  // same, which is precisely the pair worth asserting together.
  "assignment-question-waiting",
  // Every correction-desk fixture renders its own words — there is no hardware behind any
  // of them, so the weaker control-presence-only proof would be a choice rather than a
  // limit.
  "corrections",
  "corrections-off",
  "corrections-read-failed",
  "corrections-empty",
  "corrections-written",
  "corrections-write-failed",
  "corrections-loro-declined",
  "corrections-loro-never",
  "corrections-learned",
  "corrections-already-knew",
  "corrections-spoken-declined",
  "corrections-spoken-never",
  "corrections-lifted",
  // Techy mode's three states are all pure software — no hardware behind any of them — so
  // the sentence itself is asserted present, not just a control's existence.
  "techy-on",
  "techy-empty",
  "techy-unreadable",
  // Every feedback fixture renders its own words too — nothing on that surface needs
  // hardware, so control-presence-only would be a choice rather than a limit.
  "feedback",
  "feedback-unavailable",
  "feedback-rated",
  "feedback-choosing",
  "feedback-previewing",
  "feedback-approved",
  "feedback-declined-report",
  "unbound",
  "unknown-turn",
  "failed-turn",
  "voice-no-audio",
  "not-connected",
  "search-empty",
  "entity-view",
  "thread-create-refused",
  // The first-run memory dialog renders its own words out of `main.js` constants, with no
  // hardware behind either state — so the sentence itself is asserted present rather than
  // only the button beneath it.
  "memory-unprovisioned",
  "memory-no-compiler",
  // The first-run notice renders its own words — the offer out of `main.js` constants, the
  // unusable state out of `main.rs`'s two consts — with no hardware behind either, so the
  // sentences themselves are asserted present rather than only the buttons beneath them.
  // That matters most for the unusable state, which has no button at all: without the text
  // assertion there would be nothing to prove about it.
  "first-run-offer",
  "first-run-partial",
  "opening-conversation",
  "opening-stop-failed",
  "returning-conversation",
  "returning-stop-failed",
  "first-run-unusable",
  "first-run-decline-refused",
  // The first-run setup sheet renders the BACKEND's words — `Component::why`,
  // `SETUP_ACCOUNT_NOTE`, and each `SetupError`'s own Display — with no hardware behind any
  // of them, so the sentences themselves are asserted present rather than only the button.
  "setup-missing-both",
  "setup-missing-engine",
  "setup-unpinned",
  "setup-finished",
  // And the three refusals a deferred setting-up produces on a send. Nothing but software
  // behind them either, and the sentence is the whole point of the fix, so it is the
  // sentence that is asserted.
  "setup-refused-engine",
  "setup-refused-claude",
  "setup-refused-both",
  // The update row and the home screen's panel. Both are pure software driven through the
  // real settings menu — no hardware, no network, no lease — so the SENTENCE is asserted
  // present, not merely the control under it. That matters most for `updates-waiting-*`,
  // where the whole of §26 is that the sentence is what stands in place of a control.
  "updates-idle",
  "updates-unconfigured",
  "updates-ready",
  "updates-rolling-back",
  "updates-failed",
  "updates-install-failed",
  "updates-waiting-available",
  "updates-waiting-ready",
  "home-prefs",
  "home-prefs-no-entities",
]);

// ---------------------------------------------------------------------------------------
// The heuristic second opinion (part 5)
// ---------------------------------------------------------------------------------------

/// Does this string READ like a request, whatever the registry calls it? Text only — it
/// never consults the classification, which is the whole point of a second opinion.
const READS_ACTIONABLE = [
  /\b(check|try|press|tap|click|plug|quit|restart|launch|install|enable|choose|pick|set)\b/i,
  /\byou (can|could|need|must|should|'ll need)\b/i,
  /\b(not connected|unavailable|offline|not signed in|update available|restart required|will sync|waiting for you|pending)\b/i,
  /\bsay (it|the word) again\b/i,
];

function readsActionable(s) {
  return READS_ACTIONABLE.some((re) => re.test(s));
}

// ---------------------------------------------------------------------------------------

async function main() {
  const run = createRun("the affordance rule — a state the user could change renders its control");

  const inv = inventory();
  const byString = new Map(REGISTRY.map((r) => [r.s, r]));

  // ---- PART 1: the inventory and the registry agree ------------------------------------

  await run.check("NEGATIVE CONTROL: the scrape examined a real corpus, not an empty one", async () => {
    assert(
      inv.length >= 50,
      "the derived inventory is " + inv.length + " states — that is not a scrape of this UI, " +
        "it is a broken scrape reporting green over nothing. This session already caught one " +
        "scanner reporting CLEAN because its corpus was empty."
    );
    // A sentinel that must always be there: index.html's own first-run voice footnote.
    const sentinel = "I can't hear anything. Check your mic isn't muted, then try again.";
    assert(
      inv.some((r) => r.normal === sentinel),
      "the sentinel state is missing from the inventory — the extractor is not reading index.html"
    );
    return (
      inv.length + " states derived from " + SOURCES.stateSources().length +
      " shipped file(s) and the Rust bridge scrape"
    );
  });

  // ---- PART 0: the SOURCE LIST is derived, and it reaches the whole shipped UI ------------
  //
  // Everything below part 0 asks "is this state classified, and does it have its control?"
  // Part 0 asks the question that comes before it and that nothing was asking on 2026-09-04:
  // IS THIS RULE LOOKING AT THE WHOLE PRODUCT? It was not. `UI_SOURCES` read three files
  // while the shell shipped nine `<script src>` tags, so a state in any of the other nine
  // could ask the CEO to do something with no control beside it and every check under this
  // one would have passed, loudly, over a file it could not open.

  await run.check("PART 0: the source list is DERIVED from the tree, and reaches every shipped script", async () => {
    const m = SOURCES.manifest();
    const ui = SOURCES.stateSources();

    // The nine that the typed list could not see, named individually rather than counted —
    // a count would pass again the day a tenth arrives and one of these leaves.
    const WAS_INVISIBLE = [
      "home.js",
      "settings-button.js",
      "splash-library.js",
      "splash.js",
      "theme-boot.js",
      "updates.js",
      "home/field-engine.js",
      "home/field-prep.js",
      "home/field-ref.js",
    ];
    assertEqual(
      WAS_INVISIBLE.filter((f) => ui.indexOf(f) < 0),
      [],
      "the derivation does not reach a file the shipped shell loads. The whole point of " +
        "lib/ui-sources.js is that this list cannot be short."
    );

    // Two-sided: nothing on disk is unaccounted for, and nothing accounted for is missing
    // from disk. `manifest()` throws on either, so reaching here is the assertion — but the
    // numbers are printed so a reader can see the reconciliation actually had something in
    // it rather than reconciling two empty sets.
    assert(m.disk.length >= 12, "the disk walk found " + m.disk.length + " shipped files — that is not this tree");
    assertEqual(
      m.disk.filter((f) => m.order.indexOf(f) < 0),
      [],
      "a file on disk is not in the derivation, which manifest() should have refused to return"
    );
    assertEqual(m.remote.length, 0, "the shell loads something over the network: " + m.remote.join(", "));

    return (
      m.order.length + " shipped file(s) derived from index.html and the closure over it (" +
      ui.length + " ui, " + SOURCES.previewDataSources().length + " preview-data, " +
      SOURCES.styleSources().length + " style), reconciled against " + m.disk.length +
      " .js/.css/.html on disk. The nine the typed list missed are all here, `updates.js` " +
      "and `home/field-engine.js` among them."
    );
  });

  await run.check("PART 0: the widened list SEES states the old three-file list could not", async () => {
    // THE BEFORE AND AFTER, COMPUTED. The old list is re-derived here — the same scanner,
    // the same prose filter, the same Rust scrape, with only the source list changed — so
    // the difference is the widening and nothing else.
    const OLD = ["index.html", "main.js", "timeline.js"];
    const before = new Set();
    for (const name of OLD) {
      const src = fs.readFileSync(path.join(UI_DIR, name), "utf8");
      const lits = name.endsWith(".html") ? htmlVisibleStrings(src) : jsStringLiterals(src);
      for (const s of lits) {
        const n = normalize(s.text);
        if (looksLikeProse(n)) before.add(n);
      }
    }
    const gained = inv.filter((r) => !before.has(r.normal) && !/\.rs:/.test(r.sites[0]));
    assert(
      gained.length >= 40,
      "widening the source list gained only " + gained.length + " states. Either the tree " +
        "changed or the derivation stopped reaching most of it — this number was 55."
    );

    // AND THE ONE THAT MATTERS, BY NAME. CEO ruling §26 governs `updates.js`, its clause is
    // that a waiting update must not be actionable, and its sentence is the state that
    // stands where the control is not. It was invisible; it is classified now.
    //
    // THE SENTINEL IS DERIVED, AND IT USED TO BE TYPED — 2026-09-10. It was the literal
    // "I'll wait to restart until everything has finished, so nothing will be interrupted.",
    // which was the §26 sentence on the day this was written and stopped being it at commit
    // 01e9b8d8, when updates stopped asking for a restart at all. The product change was
    // correct and this check went red for it: a self-test pinned to one spelling of a
    // sentence tests the spelling. So the sentinel is now the string of whatever row the
    // registry files under the `updates-waiting-ready` fixture, which is the §26 state BY
    // ROLE rather than by wording. Nothing is weakened by that and one thing is added: the
    // row is itself driven against the real DOM by the fixture walk below, so if the copy
    // moves again and nobody updates the registry, THAT check fails on the missing sentence
    // and this one follows it — instead of this one failing alone and saying the derivation
    // is broken when it is not.
    const waitingRow = REGISTRY.find(
      (r) => r.fixture === "updates-waiting-ready" && r.c === "INFORMATIONAL"
    );
    assert(
      waitingRow,
      "no INFORMATIONAL row is filed under the `updates-waiting-ready` fixture. §26's state " +
        "is the one that stands where a control is not; if it has no row, either the ruling " +
        "stopped being implemented or the registry stopped describing it."
    );
    const SENTINEL = waitingRow.s;
    assert(!before.has(SENTINEL), "the sentinel was ALREADY visible to the old list — it proves nothing");
    assert(
      inv.some((r) => r.normal === SENTINEL),
      "the §26 waiting sentence is not in the widened inventory — the derivation is not reading updates.js"
    );
    assert(byString.has(SENTINEL), "the §26 waiting sentence reached the inventory and nobody classified it");

    const byFile = {};
    for (const g of gained) {
      const f = g.sites[0].split(":")[0];
      byFile[f] = (byFile[f] || 0) + 1;
    }
    return (
      gained.length + " state(s) are visible to this rule that were not: " +
      Object.keys(byFile).sort().map((f) => f + " " + byFile[f]).join(", ") +
      ". The §26 waiting sentence — " + JSON.stringify(SENTINEL.slice(0, 48)) + "… — is one " +
      "of them, and it is classified INFORMATIONAL because the control is REMOVED by ruling " +
      "rather than merely missing."
    );
  });

  await run.check("PART 0: what the derived manifest does NOT cover, counted rather than described", async () => {
    // A DERIVED FILE LIST IS NOT DERIVED COVERAGE, and the difference is where the next
    // blind spot will be. `lib/ui-sources.js` proves that every file the shell loads is
    // READ. It says nothing about whether every string those files render is EXTRACTED, and
    // an inventory that leaves that unstated invites exactly the reading that produced this
    // session's eleven green-over-nothing findings: "the source list is derived, therefore
    // the rule sees everything".
    //
    // So the boundary is a NUMBER here, recomputed every run, rather than a sentence in a
    // header that stops being true.
    //
    // BOUNDARY 1 — THE PROSE FLOOR. `looksLikeProse` needs 12 characters and 3 words. That
    // is what keeps identifiers, CSS values, GLSL and machinery out of an inventory of
    // STATES, and it is the right shape for a sentence. It is the wrong shape for a LABEL:
    // "Stop Rich", "Send", "Search", "Close corrections" are text the CEO reads and acts on,
    // and every one is below the floor. Measured across the 12 shipped `ui` files today:
    const belowFloor = [];
    for (const name of SOURCES.stateSources()) {
      const src = fs.readFileSync(SOURCES.abs(name), "utf8");
      const lits = name.endsWith(".html") ? htmlVisibleStrings(src) : jsStringLiterals(src);
      for (const l of lits) {
        const n = normalize(l.text);
        if (!n || !/[A-Za-z]/.test(n) || looksLikeProse(n)) continue;
        const words = n.split(/\s+/).filter(Boolean);
        // Label-SHAPED: short, capitalized, no code punctuation. Deliberately conservative —
        // it undercounts rather than sweeping identifiers in, so the number below is a floor
        // on the gap and never an inflation of it.
        if (n.length >= 3 && words.length <= 2 && /^[A-Z]/.test(n) && !/[{}<>();=_.]/.test(n)) {
          belowFloor.push(name + ":" + l.line + " " + JSON.stringify(n));
        }
      }
    }
    assert(
      belowFloor.length > 0,
      "0 label-shaped literals below the prose floor. Either the floor changed or this " +
        "measurement broke — and a boundary check that measures nothing is the thing it exists " +
        "to prevent."
    );
    assert(
      inv.length > belowFloor.length * 0.5,
      "the inventory (" + inv.length + ") is small beside the excluded set (" + belowFloor.length +
        "), which would mean the prose floor is eating states rather than bounding them"
    );

    // BOUNDARY 2 — A STRING THIS PRODUCT COMPOSES AT RUNTIME IS UNREACHABLE BY ANY SOURCE
    // SCRAPE, and no widening of the file list changes that. Template interpolation is the
    // visible half of it; the invisible half is text that arrives as DATA over the bridge —
    // a sentence composed in Rust, or by the model, and handed to the renderer. The
    // renderer's source contains the slot, never the sentence. Counted so the shape is a
    // number too:
    const interpolated = SOURCES.uiMatches(/`[^`]*\$\{[^`]*`/).length;
    assert(
      interpolated > 0,
      "0 interpolated template literals across the shipped UI — this product certainly has " +
        "some, so this measurement is broken rather than the boundary being absent"
    );

    // BOUNDARY 3 — richos-core's `Display` impls are DELIBERATELY out of the Rust scrape
    // (`lib/state-strings.js`'s RUST_ROOTS comment, blind spot B5). They are machinery, and
    // although one can reach a local notice through `String(e)`, that surface is unbounded.
    // Named here so "the Rust bridge scrape" is not read as "the Rust".
    const rustSites = new Set(inv.filter((r) => /\.rs:/.test(r.sites[0])).map((r) => r.sites[0].split(":")[0]));
    assert(rustSites.size > 0, "the Rust scrape contributed nothing to the inventory");

    return (
      inv.length + " state(s) in the inventory from " + SOURCES.stateSources().length +
      " derived ui file(s) + " + rustSites.size + " Rust file(s). NOT COVERED, and none of it " +
      "is fixable by widening the file list: " + belowFloor.length +
      " label-shaped literal(s) below the 12-character/3-word prose floor (e.g. " +
      belowFloor.slice(0, 3).map((s) => s.split(" ").slice(1).join(" ")).join(", ") +
      "); " + interpolated + " interpolated template literal(s), whose sentences are composed at " +
      "run time; and every string that arrives as DATA over the bridge — composed in Rust or " +
      "by the model — which no source-text scrape of any width can see."
    );
  });

  await run.check("PART 0 NEGATIVE CONTROL: the reconciliation refuses a short list, a missing role and a stale one", async () => {
    // The same function `manifest()` calls, with synthetic inputs — not a re-implementation
    // of its three rules, which would prove only that the copy agrees with itself.
    const ok = { "a.js": { role: "ui", why: "x" }, "b.css": { role: "style", why: "x" } };

    const cases = [
      {
        what: "a file on disk that nothing loads",
        run: () => SOURCES.reconcile(["a.js"], ["a.js", "b.css"], { "a.js": ok["a.js"] }),
        expect: "nothing loads them",
      },
      {
        what: "a loaded file with no role",
        run: () => SOURCES.reconcile(["a.js", "b.css"], ["a.js", "b.css"], { "a.js": ok["a.js"] }),
        expect: "have no role",
      },
      {
        what: "a role for a file nobody loads",
        run: () => SOURCES.reconcile(["a.js"], ["a.js"], ok),
        expect: "the derivation does not reach",
      },
    ];
    const proved = [];
    for (const c of cases) {
      let threw = null;
      try { c.run(); } catch (e) { threw = e.message; }
      assert(threw, "reconcile() ACCEPTED " + c.what + " — it would accept the next blind spot too");
      assert(
        threw.indexOf(c.expect) >= 0,
        "reconcile() refused " + c.what + ", but for the wrong reason: " + threw.split("\n")[0]
      );
      proved.push(c.what);
    }
    // ...and it accepts a set that agrees, so the three above are not passing because the
    // function refuses everything.
    assertEqual(SOURCES.reconcile(["a.js", "b.css"], ["a.js", "b.css"], ok), 2, "reconcile() refused a set that agrees");
    return "refused: " + proved.join("; ") + ". Accepted the agreeing set.";
  });

  await run.check("PART 0: the GLSL filter drops shaders and nothing else, measured", async () => {
    // `home/field-engine.js` carries its shaders as string literals, and a shader clears
    // every prose test written for English. The filter's cost is measured rather than
    // assumed, because a filter that quietly ate real states would shrink this inventory and
    // no other check here would notice.
    const dropped = shaderStrings();
    assert(dropped.length > 0, "0 shader literals found — the scrape is not reading home/field-engine.js");
    const files = [...new Set(dropped.map((d) => d.site.split(":")[0]))];
    assertEqual(files, ["home/field-engine.js"], "the GLSL filter is dropping strings outside the shader module");
    assertEqual(
      inv.filter((r) => looksLikeShader(r.normal)).map((r) => r.sites.join(" ")),
      [],
      "a shader survived into the state inventory"
    );
    return (
      dropped.length + " GLSL literal(s) dropped, all " + dropped.length + " in " + files[0] +
      ", 0 elsewhere; 0 shaders left in the " + inv.length + "-state inventory."
    );
  });

  await run.check("every derived state is classified, and every classification is a real state", async () => {
    const invSet = new Set(inv.map((r) => r.normal));
    const unclassified = inv.filter((r) => !byString.has(r.normal));
    const stale = REGISTRY.filter((r) => !invSet.has(r.s));
    // Both halves are printed before either refuses, so a change that adds AND removes strings
    // is fixed in one pass rather than discovering the second half on the next run.
    if (unclassified.length && stale.length) {
      console.log("          ALSO STALE (a row whose string the product no longer renders): " +
        JSON.stringify(stale.map((r) => r.s.length > 160 ? r.s.slice(0, 160) + "…" : r.s)));
    }
    assertEqual(
      unclassified.map((r) => r.normal + "   <- " + r.sites.join(" ")),
      [],
      "NEW USER-VISIBLE STATE, NOT CLASSIFIED. Add a row to lib/state-registry.js saying " +
        "whether the CEO can act on it. If he can, it needs a control in the same view."
    );
    assertEqual(
      stale.map((r) => r.s),
      [],
      "the registry classifies a string the product no longer renders — delete the row"
    );
    assertEqual(REGISTRY.length, inv.length, "registry and inventory must be the same size");
    return inv.length + "/" + inv.length + " classified, 0 unclassified, 0 stale";
  });

  await run.check("no duplicate rows, and every class is one of the declared buckets", async () => {
    const seen = new Set();
    for (const r of REGISTRY) {
      assert(!seen.has(r.s), "duplicate registry row: " + JSON.stringify(r.s.slice(0, 60)));
      seen.add(r.s);
      assert(ALL_CLASSES.indexOf(r.c) >= 0, "unknown class " + r.c + " on " + JSON.stringify(r.s.slice(0, 60)));
      assert(typeof r.why === "string" && r.why.length > 12, "every row states its reasoning: " + r.s.slice(0, 60));
    }
    const counts = {};
    for (const r of REGISTRY) counts[r.c] = (counts[r.c] || 0) + 1;
    return ALL_CLASSES.map((c) => c + " " + (counts[c] || 0)).join(", ");
  });

  // ---- PART 2: the rule, statically -----------------------------------------------------

  await run.check("THE RULE: every ACTIONABLE state names a control", async () => {
    const bare = REGISTRY.filter((r) => r.c === "ACTIONABLE" && !r.control);
    assertEqual(
      bare.map((r) => r.s),
      [],
      "an ACTIONABLE state with no control is the defect this suite exists for: a state the " +
        "user could change, rendered apart from the thing that changes it"
    );
    return REGISTRY.filter((r) => r.c === "ACTIONABLE").length + " ACTIONABLE states, each naming a control";
  });

  await run.check("every NEEDS-SOMEONE-ELSE state names the party, or names what explains it", async () => {
    const orphaned = REGISTRY.filter(
      (r) => r.c === "NEEDS-SOMEONE-ELSE" && !r.explainedBy && !PARTY.test(r.s)
    );
    assertEqual(
      orphaned.map((r) => r.s.slice(0, 90)),
      [],
      "a state the CEO cannot fix must say WHO can. Either the sentence names a party " +
        "(" + PARTY.source + ") or the row points at the state in the same view that does."
    );
    const withParty = REGISTRY.filter((r) => r.c === "NEEDS-SOMEONE-ELSE" && PARTY.test(r.s)).length;
    const viaExplainer = REGISTRY.filter((r) => r.c === "NEEDS-SOMEONE-ELSE" && r.explainedBy).length;
    return withParty + " name a party in the sentence, " + viaExplainer + " are explained by a state beside them";
  });

  await run.check("no INFORMATIONAL state is secretly an instruction", async () => {
    // The narrow half of the heuristic: a bare second-person imperative aimed at the reader.
    // An INFORMATIONAL row is a promise that there is nothing to do, and "Press …" breaks it.
    const IMPERATIVE = /(^|[.;:]\s+)(press|tap|click|check|plug|quit|restart|sign in|type in)\b/i;
    const liars = REGISTRY.filter((r) => r.c === "INFORMATIONAL" && IMPERATIVE.test(r.s));
    assertEqual(
      liars.map((r) => r.s.slice(0, 90)),
      [],
      "an INFORMATIONAL state that issues an instruction is mis-classified — it is ACTIONABLE " +
        "and owes a control"
    );
    return REGISTRY.filter((r) => r.c === "INFORMATIONAL").length + " INFORMATIONAL states, none issuing an instruction";
  });

  // THE FAILURE CLASS `docs/development/failure-classes/lost-input-without-a-next-step.md`.
  // On 2026-10-21 a new user was told, in voice mode, that whatever he had just said "didn't
  // reach me and I haven't sent anything. I'm listening now." and nothing about what to do. A
  // line that tells someone their words did not get through is a loss, and a loss with no
  // next step is a dead end. So every such line is ACTIONABLE (PART 3 then proves its control
  // is on screen) and, when it is a whole sentence, says the step in words. A lead-in that
  // ends with ":" introduces the recovery drawn right after it, and its row still names that
  // control.
  const LOST_INPUT = /haven't sent anything|didn't reach me|(answer|message|words?) (was|were) not sent/i;
  const NEXT_STEP = /\b(say it again|send it again|try again|copy it|press|tap|click|retry)\b/i;
  await run.check("every voice input refusal has a next step regardless of its wording", async () => {
    const file = path.resolve(UI_DIR, "../crates/richos-voice/src/controller.rs");
    const notices = rustCeoLiterals(file, fs.readFileSync(file, "utf8"));
    // ReplyCutOff reports a failed ANSWER. Its caller appends the interruption's own route.
    // Every other notice here refuses INPUT, including future variants and rewordings.
    const inputs = notices.filter(r => !r.text.startsWith("I was cut off partway through that answer"));
    assert(inputs.length >= 4, "the voice input-notice family was not read");
    for (const row of inputs) {
      assert(NEXT_STEP.test(row.text), "input refusal has no next step: " + row.text);
      const registered = REGISTRY.find(r => normalize(r.s) === normalize(row.text));
      assert(registered && registered.c === "ACTIONABLE", "input refusal must carry its recovery control: " + row.text);
    }
    return inputs.length + " input notices read from shipped Rust, independent of the lost-input phrase scan";
  });
  await run.check("every line saying his words did not get through says what to do next", async () => {
    const lost = REGISTRY.filter((r) => LOST_INPUT.test(r.s));
    const deadEnds = lost.filter(
      (r) => r.c !== "ACTIONABLE" || (!r.s.trim().endsWith(":") && !NEXT_STEP.test(r.s))
    );
    assertEqual(
      deadEnds.map((r) => r.c + ": " + r.s.slice(0, 110)),
      [],
      "a line that tells him his words did not get through must be ACTIONABLE and must say " +
        "the next step (" + NEXT_STEP.source + ")"
    );
    // POSITIVE CONTROL, so an empty match cannot pass this: the 2026-10-21 sentence itself is
    // caught, and the voice family it belongs to is in the corpus.
    const incident = "While I was speaking, I couldn't tell your voice from my own, so if you said " +
      "something just then, it didn't reach me and I haven't sent anything. I'm listening now.";
    assert(LOST_INPUT.test(incident) && !NEXT_STEP.test(incident), "the rule no longer catches the incident's sentence");
    assert(lost.length >= 3, "only " + lost.length + " lost-input lines found; the voice family alone is three");
    return lost.length + " lines report lost input, each ACTIONABLE with its next step";
  });

  // ---- PART 3: the control is really on screen ------------------------------------------

  const { webkit } = loadPlaywright();
  const browser = await webkit.launch();
  let statesChecked = 0;

  await run.check("Connect setup and recovery keep their controls in the same view", async () => {
    for (const recovering of [false, true]) {
      const page = await openApp(browser, undefined, {
        phoneConnect: { enabled:recovering, phase:recovering ? "active" : "not-configured", health:recovering ? {state:"reconnecting"} : null },
        phonePaired:recovering, phonePairedVia:"connect", phoneListenerStopped:recovering,
      });
      await page.click("#set-btn"); await page.click("#set-phone-open");
      await page.waitForSelector("#phone-connect", {state:"visible"});
      const control = recovering ? "#phone-connect-disable" : "#phone-connect-start";
      const text = recovering ? "Your phone's pairing is saved. RichOS is trying to restore its connection." : "First, set up this RichOS app on your Mac. Then pair your phone with it.";
      await assertAffordance(page, {s:text, c:"ACTIONABLE", control}, {requireText:true});
      assert(await page.locator("#phone-close").isVisible(), "Connect has no visible way out");
      if (recovering) assert(await page.locator("#phone-forget").isVisible(), "Saved phone cannot be revoked from this view");
      await page.close();
    }
    return "Setup offers enable; recovery offers disable and phone revocation; both retain Close";
  });

  // Every row that names a fixture, INCLUDING the ones with no control. A
  // NEEDS-SOMEONE-ELSE state with nothing to press still has to actually render the
  // sentence that names the party — otherwise the registry is describing a screen nobody
  // has seen.
  const needFixture = REGISTRY.filter((r) => r.fixture);
  const byFixture = new Map();
  for (const r of needFixture) {
    if (!byFixture.has(r.fixture)) byFixture.set(r.fixture, []);
    byFixture.get(r.fixture).push(r);
  }

  for (const [name, entries] of byFixture) {
    await run.check(
      `[${name}] ${entries.length} state(s) render with a usable control`,
      async () => {
        assert(FIXTURES[name], "no fixture named " + name);
        const page = await FIXTURES[name](browser);
        const requireText = TEXT_RENDERING_FIXTURES.has(name);
        const samples = [];
        for (const entry of entries) {
          const sample = await assertAffordance(page, entry, { requireText });
          samples.push(entry.control ? entry.control + " -> " + sample : sample);
          statesChecked++;
        }
        assert(
          page.__errors.length === 0,
          "the shell logged errors while in this state: " + page.__errors.join(" | ")
        );
        await page.close();
        return (requireText ? "state text asserted present; " : "control-presence only (state needs live hardware); ") +
          Array.from(new Set(samples)).join(", ");
      }
    );
  }

  await run.check("NEGATIVE CONTROL: the browser half examined a non-zero number of states", async () => {
    assert(
      statesChecked >= 20,
      "only " + statesChecked + " states were checked in a real DOM. A suite that verifies " +
        "nothing and reports green is the failure this session has already caught twice — once " +
        "in a scanner with an empty corpus, once in a reporting layer whose fixtures lacked a field."
    );
    return statesChecked + " states asserted against the real DOM under WebKit";
  });

  await run.check("the §21 way out actually goes somewhere", async () => {
    const page = await FIXTURES.unbound(browser);
    await page.click("#unbound-new-thread");
    await page.waitForSelector("#entity-picker:not([hidden])");
    const n = await page.evaluate(() => document.querySelectorAll("#entity-picker .picker-item").length);
    assert(n > 0, "the picker opened with no entities in it — the button leads nowhere");
    await page.close();
    return "#unbound-new-thread opens the §3.3 picker with " + n + " entities, defaulting to none";
  });

  // ---- Ray's candidate-.11 defect 3.5: the word, and the question with one answer --------
  //
  // §3.5, verbatim: *"'entity' is schema vocabulary in front of a non-technical CEO
  // (everything else in the app says company), and the dialog is shown when there is exactly
  // one choice."* Two faults in one small dialog, and they are checked together because the
  // fix is one flow.
  //
  // THE SINGLE-COMPANY HALF NEEDS A SINGLE-COMPANY INSTALL, which `mock.js` deliberately is
  // not — it carries six, because several suites assert positions in a six-long row. So this
  // check wraps the bridge and filters `navigation_tree` down to one group, which is the
  // shape of a real first install. It does that rather than editing the mock for the reason
  // the mock's own comment gives for holding six.
  await run.check("the company picker says \"company\", and does not ask when there is one answer", async () => {
    // (a) THE WORD, on the install the rest of this suite runs on — six companies, so the
    //     dialog opens and its title can be read off the screen. Since round 32 the rail's
    //     "+ New chat or company" opens its own menu, so the dialog is reached by the door that
    //     still asks it: the unbound screen's way out.
    const many = await FIXTURES.unbound(browser);
    await many.click("#unbound-new-thread");
    await many.waitForSelector("#entity-picker:not([hidden])");
    const title = (await many.textContent("#entity-picker-title")).trim();
    assertEqual(title, "Which company is this work in?", "the picker speaks the schema's language");
    assert(!title.toLowerCase().includes("entity"), `"entity" is in front of him: ${title}`);
    await many.keyboard.press("Escape");
    await many.close();

    // (b) THE QUESTION WITH ONE ANSWER.
    const page = await browser.newPage({ viewport: { width: 1400, height: 900 } });
    const errors = [];
    page.on("pageerror", (e) => errors.push(String(e)));
    await page.addInitScript(() => {
      let real = null;
      Object.defineProperty(window, "RichBridge", {
        configurable: true,
        get: () => real,
        set(v) {
          real = v;
          const invoke = v.invoke.bind(v);
          v.invoke = async (cmd, args) => {
            const out = await invoke(cmd, args);
            if (cmd === "navigation_tree" && out && out.groups && out.groups.length > 1) {
              return Object.assign({}, out, { groups: out.groups.slice(0, 1) });
            }
            return out;
          };
        },
      });
    });
    await page.goto(APP);
    await leaveHome(page);
    await page.waitForSelector(".nav-thread", { state: "attached" });
    await dismissEntityPicker(page);
    // The app's OWN count of companies, not a count of rail sections: the rail draws a group
    // for unbound threads too, and the number this check is about is `navTree.groups.length`.
    const only = await page.evaluate(async () => (await window.RichBridge.invoke("navigation_tree")).groups.length);

    // ⌘N, the door that asks which company (round 32: with one company it asks nothing).
    await page.keyboard.press("Meta+n");
    // He is on the new-thread screen for the only company he has, with NO dialog in between.
    // BOUNDED, AND THE FAILURE IS NAMED RATHER THAN A TIMEOUT: with the skip removed this wait
    // can never be satisfied, and "Timeout 10000ms exceeded waiting for #entity-view" is true
    // and useless to whoever has to read it. The state is read and asserted instead.
    const landed = await page
      .waitForSelector("#entity-view:not([hidden])", { timeout: 8000 })
      .then(() => true)
      .catch(() => false);
    const after = await page.evaluate(() => ({
      picker: !document.getElementById("entity-picker").hidden || !document.getElementById("new-chat-menu").hidden,
      rows: document.querySelectorAll("#entity-picker .picker-item").length,
      canType: !document.getElementById("input").disabled,
    }));
    assertEqual(
      after.picker,
      false,
      `the dialog was shown for a choice that has one option — ${after.rows} row(s) in it, and ` +
        `the only thing pressing that row can do is close the dialog`
    );
    assert(landed, "he never reached the new-thread screen at all");
    assert(after.canType, "he reached the new-thread screen but cannot type into it");
    assertEqual(errors, [], "the shell logged errors on the one-company path");
    await page.close();
    return `with ${only} company on the install the picker is skipped and he lands on its new-thread screen; with six it opens, titled "${title}"`;
  });

  // ---- the company setting, which the string inventory cannot see ------------------------
  //
  // `UI_SOURCES` is index.html, main.js and timeline.js, so the row this pass added to
  // `settings-button.js` is outside the derivation and outside the registry — the same
  // blind spot Techy Mode's and the opening screen's rows in that file already sit in.
  // These two checks are the cover for it: they drive the REAL menu and assert the control
  // and, in the state where there is no control, the statement that names who owns it.

  await run.check("the settings menu carries the company, and choosing one writes it through", async () => {
    const page = await openApp(browser, undefined, { chosenEntity: null });
    await page.waitForSelector("#entity-picker:not([hidden])");
    await dismissEntityPicker(page);
    await page.click(".setbtn");
    await page.waitForSelector("#set-company");
    const before = await page.evaluate(() => {
      const sel = document.getElementById("set-company");
      return {
        options: Array.from(sel.options).map((o) => o.value),
        value: sel.value,
        disabled: sel.disabled,
        hasName: !!document.getElementById("set-company-label"),
      };
    });
    assert(before.hasName, "the row has no label, so the control has no accessible name");
    assert(!before.disabled, "the control is disabled on a launch that has chosen nothing");
    assertEqual(before.value, "", "a company nobody chose must not be pre-selected");
    assert(
      before.options.indexOf("lumen") >= 0 && before.options.indexOf("harbor") >= 0,
      "the menu does not list the registry: " + JSON.stringify(before.options)
    );
    // THE WRITE, THROUGH THE REAL PATH: the row's host capability calls `choose_entity`,
    // the shell clears the composer's block, and the rail's scope line names the company.
    await page.selectOption("#set-company", "lumen");
    await page.waitForFunction(() => document.getElementById("composer-blocked").hidden, null, { timeout: 30000 });
    const after = await page.evaluate(() => ({
      scope: (document.getElementById("scope-entity") || {}).textContent,
      blocked: document.getElementById("composer-blocked").hidden,
      button: document.getElementById("composer-choose-company").hidden,
    }));
    assertEqual(after.scope, "Lumen Labs", "the scope line does not name the company that was just chosen");
    assert(after.blocked && after.button, "the block and its control survived an answered question");
    await page.close();
    return "listed " + before.options.length + " option(s), chose Lumen Labs, block cleared, scope reads Lumen Labs";
  });

  await run.check("a company pinned outside the window is stated, never offered as a dead control", async () => {
    const page = await openApp(browser, undefined, { pinnedByEnvironment: true });
    await page.click(".setbtn");
    await page.waitForSelector("#set-company-pinned");
    const r = await page.evaluate(() => {
      const said = document.getElementById("set-company-pinned");
      return {
        control: !!document.getElementById("set-company"),
        text: said.textContent.trim(),
        title: said.getAttribute("title") || "",
      };
    });
    assert(!r.control, "a pinned company must not render a control that would refuse every answer");
    assertEqual(r.text, "Harbor Analytics", "the row does not state which company is in force");
    assert(
      /[Ww]hoever set RichOS up/.test(r.title),
      "a state he cannot change must name who can — the row says: " + JSON.stringify(r.title)
    );
    await page.close();
    return "no control, states RichOS, and names whoever set RichOS up";
  });

  await run.check("a rail status mark can never render outside the control that acts on it", async () => {
    const page = await FIXTURES["rail-mark"](browser);
    const r = await page.evaluate(() => {
      const marks = Array.from(document.querySelectorAll(".nav-status"));
      return {
        total: marks.length,
        inButton: marks.filter((m) => m.closest("button.nav-thread")).length,
      };
    });
    assert(r.total > 0, "no status marks rendered — this check would pass by finding nothing");
    assertEqual(r.inButton, r.total, "a status mark rendered outside a thread button");
    await page.close();
    return r.total + "/" + r.total + " marks are inside the button that opens their thread";
  });

  await run.check("a refused send leaves no ghost bubble claiming the message went", async () => {
    const page = await FIXTURES["not-connected"](browser);
    const r = await page.evaluate(() => ({
      userBubbles: document.querySelectorAll(".tl-user").length,
      pending: window.__RICHOS_TIMELINE__().pendingUser.length,
      composer: document.getElementById("input").value,
      sendVisible: !document.getElementById("send").hidden,
    }));
    assertEqual(r.userBubbles, 0, "the withdrawn bubble is still on screen, looking like a sent message");
    assertEqual(r.pending, 0, "the model still holds a pending user message that will never be adopted");
    assertEqual(r.composer, "book the Acme call for Thursday", "the CEO's words were not returned to the box");
    assert(r.sendVisible, "the notice names Send and Send is hidden");
    await page.close();
    return "bubble withdrawn, words restored verbatim, Send up";
  });

  await run.check(
    "mock.js rehearses the SAME lease-down sentence the product ships",
    async () => {
      const fs = require("fs");
      const rust = fs.readFileSync(path.join(UI_DIR, "..", "src-tauri", "src", "main.rs"), "utf8");
      const mock = fs.readFileSync(path.join(UI_DIR, "mock.js"), "utf8");
      const entry = REGISTRY.find((r) => r.s.indexOf("RichOS couldn't reach Claude Code") === 0);
      assert(entry, "the lease-down state is missing from the registry");
      assert(rust.indexOf("LEASE_UNAVAILABLE_MESSAGE") > 0, "the Rust const is gone");
      assert(
        normalize(mock).indexOf(entry.s) >= 0,
        "app/ui/mock.js still rehearses a lease-down sentence the product no longer says. The " +
          "browser preview would teach the CEO one thing and the app another."
      );
      return "the const, the registry and the preview all carry one sentence";
    }
  );

  // ---- PART 4: the positive control -----------------------------------------------------

  await run.check(
    "POSITIVE CONTROL: an ACTIONABLE state with no affordance IS flagged",
    async () => {
      const page = await browser.newPage();
      await page.setContent(
        "<!doctype html><html><body><p id=liar>Not connected — check the thing and try again.</p></body></html>"
      );
      const fake = {
        s: "Not connected — check the thing and try again.",
        c: "ACTIONABLE",
        control: "#a-button-that-does-not-exist",
      };
      let threw = null;
      try {
        await assertAffordance(page, fake, { requireText: true });
      } catch (e) {
        threw = e.message;
      }
      await page.close();
      assert(threw, "the checker PASSED a state that renders with no control — it proves nothing");
      assert(
        threw.indexOf("NO control") >= 0,
        "flagged, but for the wrong reason: " + threw
      );
      return "flagged: " + threw.split("\n")[0];
    }
  );

  await run.check(
    "POSITIVE CONTROL: a control that exists but is hidden IS flagged",
    async () => {
      const page = await browser.newPage();
      await page.setContent(
        "<!doctype html><html><body><p>Not connected — press Fix.</p>" +
          "<button id=fix hidden>Fix</button></body></html>"
      );
      let threw = null;
      try {
        await assertAffordance(page, { s: "Not connected — press Fix.", c: "ACTIONABLE", control: "#fix" }, {});
      } catch (e) {
        threw = e.message;
      }
      await page.close();
      assert(threw && threw.indexOf("NOT VISIBLE") >= 0, "a hidden control passed the check: " + threw);
      return "flagged: " + threw.split("\n")[0];
    }
  );

  await run.check(
    "POSITIVE CONTROL: a paragraph dressed as a control IS flagged",
    async () => {
      const page = await browser.newPage();
      await page.setContent(
        "<!doctype html><html><body><p>Not connected — see the docs.</p>" +
          "<p id=fix>Read the documentation</p></body></html>"
      );
      let threw = null;
      try {
        await assertAffordance(page, { s: "x", c: "ACTIONABLE", control: "#fix" }, {});
      } catch (e) {
        threw = e.message;
      }
      await page.close();
      assert(threw && threw.indexOf("not something anyone can press") >= 0, "prose passed as a control: " + threw);
      return "flagged: " + threw.split("\n")[0];
    }
  );

  await run.check(
    "POSITIVE CONTROL ON THE REAL UPDATE SURFACE: delete the control it names and the rule fails",
    async () => {
      // THE PROOF THAT THE WIDENING IS WORTH ANYTHING. The three positive controls above run
      // against `page.setContent` markup written for them — they prove the CHECKER can fail.
      // This one runs against the shipped `updates.js` in the shipped shell, on a surface that
      // was outside this rule until 2026-09-05, and proves the RULE can fail there.
      //
      // THE VICTIM IS CHOSEN, NOT NAMED — 2026-09-10. This check used to open the
      // `updates-ready` fixture, look up the row for "Restart when you are ready — nothing is
      // lost." and delete `#update-relaunch`. Commit 01e9b8d8 removed that button from the
      // product on purpose: an update activates at the next launch and there is no restart to
      // offer. The rule was fine, the product was fine, and this check failed — because it
      // had a specific id and a specific sentence written into it, so it was testing that
      // ONE control still existed rather than that the RULE still bites. It now takes the
      // first ACTIONABLE row the registry files under any `updates-*` fixture and deletes
      // whatever control THAT row names, so it survives the update surface being redesigned
      // and still fails if the rule stops working.
      const target = REGISTRY.find(
        (r) => r.c === "ACTIONABLE" && r.control && /^updates-/.test(String(r.fixture))
      );
      assert(
        target,
        "no ACTIONABLE row on any update fixture names a control. Either §26 now removes " +
          "every affordance on this surface — in which case this positive control has no " +
          "victim and needs rewriting rather than deleting — or the rows lost their controls."
      );
      const page = await FIXTURES[target.fixture](browser);
      const entry = byString.get(target.s);
      assert(entry && entry.control, "the update surface's ACTIONABLE row lost its control");

      // It passes as shipped...
      const sample = await assertAffordance(page, entry, { requireText: true });

      // ...and fails the moment the control it names is not there. `paint()` hiding the
      // control in a state that still shows the sentence, a renamed id, a row that stopped
      // being built: all of them arrive at the DOM as this.
      await page.evaluate((sel) => document.querySelector(sel).remove(), entry.control);
      let threw = null;
      try {
        await assertAffordance(page, entry, { requireText: true });
      } catch (e) {
        threw = e.message;
      }
      await page.close();
      assert(
        threw,
        "an ACTIONABLE update state passed with NO control on screen. Before 2026-09-05 this " +
          "rule could not open updates.js at all; if it still cannot fail here, widening it " +
          "bought nothing."
      );
      assert(threw.indexOf("NO control") >= 0, "flagged, but for the wrong reason: " + threw.split("\n")[0]);
      return (
        "on " + target.fixture + ", as shipped: " + sample + "; with " + entry.control +
        " removed: " + threw.split("\n")[0]
      );
    }
  );

  await run.check("§26: a waiting update offers NO actionable control, on the real surface", async () => {
    // The CEO's ruling, as a check rather than as a comment: while work is running the
    // update affordance is REMOVED, not dimmed. "A dimmed control still invites the press and
    // then refuses it, which is the anxiety being removed rather than a milder version of
    // it." A future edit that softened this to `disabled = true` would leave every other
    // check in this suite green.
    const out = [];
    for (const name of ["updates-waiting-available", "updates-waiting-ready"]) {
      const page = await FIXTURES[name](browser);
      const seen = await page.evaluate(() => {
        const look = (id) => {
          const n = document.getElementById(id);
          if (!n) return "absent";
          if (n.hidden) return "hidden";
          const r = n.getBoundingClientRect();
          if (r.width === 0 || r.height === 0) return "zero-area";
          return n.disabled ? "DISABLED-BUT-PRESENT" : "PRESSABLE";
        };
        const note = document.getElementById("update-waiting");
        const painted = document.getElementById("update-waiting-note");
        return {
          install: look("update-install"),
          relaunch: look("update-relaunch"),
          cue: look("update-cue"),
          note: !!note,
          noteRole: note ? note.getAttribute("role") : null,
          noteName: note ? note.getAttribute("aria-label") || "" : "",
          notePainted: painted ? painted.textContent : null,
        };
      });
      for (const which of ["install", "relaunch", "cue"]) {
        assert(
          seen[which] !== "PRESSABLE" && seen[which] !== "DISABLED-BUT-PRESENT",
          "[" + name + "] #update-" + which + " is " + seen[which] + " while RichOS is working. " +
            "§26: the affordance is removed, never offered and refused."
        );
      }
      assert(seen.note, "[" + name + "] nothing stands where the control is not — the state is silent");
      assertEqual(seen.noteRole, "note", "[" + name + "] the waiting cue is not a `role=note`");
      // THE ACCESSIBLE NAME CARRIES THE WHOLE SENTENCE, DERIVED — 2026-09-10.
      //
      // This used to be `/nothing will be interrupted/.test(seen.noteName)`, a phrase typed
      // out of `waitingSaid()` on the day it was written. Commit 01e9b8d8 rewrote the `ready`
      // tail to "…Your work will continue uninterrupted." — same promise, same state, still
      // no control — and this check went red at a product that had not broken. A regex over
      // product copy is a check that fails every time a writer improves a sentence, which
      // trains people to edit the check without reading it.
      //
      // What §26 actually requires of this element is that the sentence standing where the
      // control is not is REALLY THERE and reaches everyone. So that is what is asserted, and
      // all of it comes off the DOM rather than out of this file:
      //
      //   - the painted note and the accessible name are the SAME string. `waitingCue()`
      //     claims this in as many words — "one value, generated once … they cannot disagree
      //     because there is only one of them" — and until now nothing checked it. The note
      //     is `aria-hidden`, so if the two ever diverged a screen-reader user would get the
      //     wrong sentence and no walk of the painted text would notice.
      //   - it is a real sentence, not a label: `waitingSaid()` composes a head, the reason
      //     and a tail, and the shortest it can be is far above this floor.
      //   - it carries the BUSY REASON the fixture supplied, which is the clause that says
      //     why nothing is being asked of him. A cue that dropped it would still read fine
      //     and would have stopped answering the question §26 is about.
      //   - it names the version, so the cue cannot degrade into a generic "an update".
      assert(
        seen.notePainted !== null,
        "[" + name + "] the waiting cue has no painted note — #update-waiting-note is absent"
      );
      assertEqual(
        seen.noteName,
        seen.notePainted,
        "[" + name + "] the accessible name and the painted note are different strings. " +
          "`waitingCue()` generates one value for both; a screen reader and an eye must not " +
          "be told different things about a state that offers nothing to press."
      );
      assert(
        seen.noteName.length > 60,
        "[" + name + "] the waiting cue's accessible name is too short to be the explanation " +
          "that stands in place of a control: " + JSON.stringify(seen.noteName)
      );
      assert(
        seen.noteName.indexOf("Two specialists are running.") >= 0,
        "[" + name + "] the waiting cue dropped the reason RichOS is busy, which is the " +
          "clause that explains why nothing is offered: " + JSON.stringify(seen.noteName)
      );
      assert(
        seen.noteName.indexOf("0.1.2") >= 0,
        "[" + name + "] the waiting cue does not name the version that is waiting: " +
          JSON.stringify(seen.noteName)
      );
      out.push(name + ": install " + seen.install + ", relaunch " + seen.relaunch + ", a role=note in their place");
      await page.close();
    }
    return out.join("; ");
  });

  // -------------------------------------------------------------------------------------
  // PART 6 — THE OPENING SCREEN HAS NO DEAD CONTROL.  BLOCKING, real DOM, real clicks.
  //
  // WHAT IT IS FOR, AND WHY PARTS 1-3 COULD NOT SEE IT. Parts 1-3 ask "does this classified
  // STATE have a control?". Audit-7 row 3 was the mirror image: a control with no state
  // behind it. The word `Enter` sat under `Talk to Rich` as a bare `<p>` with no handler, so
  // Ray clicked it four times across two launches and the front door did not open — eleven
  // minutes to get into the app, with the only working way in being the button that also
  // turns the microphone on. Every check in this file was green over it, and so was
  // `home.js`'s own `Enter`-is-a-promise check, because BOTH drive the screen through
  // `RichHome.hide()` / `page.keyboard` and neither had ever clicked that word.
  //
  // THE RULE, and it is derived from the DOM rather than from a list: on the opening screen,
  // anything that PRESENTS as a control must ACT as one. Presenting as a control is
  // `<button>`, `<a href>`, `[role=button]`, or `cursor: pointer` — the four things a person
  // reads as "this is for pressing", asked of the computed style so a new control is covered
  // the day it is written with nothing to remember here. Acting as one is: a real
  // `page.mouse.click` at its middle changes something observable — the screen leaves, or a
  // panel opens, or the menu opens.
  //
  // A control that must NOT answer a press declares itself with `data-inert-control` and a
  // reason, the same discipline `data-dismiss` uses. There are none today.
  //
  // IT ASKS "IS IT WIRED?", NOT "DID THE SCREEN MOVE?", AND THE FIRST DRAFT ASKED THE WRONG
  // ONE. Clicking three observables (screen left / panel open / menu open) called all seven
  // company chips dead: a chip changes `aria-pressed` and re-filters the picture, and the one
  // that is ALREADY selected legitimately changes nothing at all. An observable set is a list,
  // and this file's own thesis is that a list drifts. So `addEventListener` is wrapped before
  // the page's scripts run and every element that takes a press handler is recorded — the
  // question the defect actually poses, asked of the DOM, with no observable to enumerate and
  // no no-op to misread. Delegation counts: a handler on an ancestor wires its children.
  // -------------------------------------------------------------------------------------
  /// THE LOADING LAYER COVERS THE WHOLE COMPOSITION UNTIL THE PICTURE IS UP, and it is
  /// `z-index: 10` with pointer events until it gets `.gone`. A click aimed at the door in the
  /// first second lands on `#home-loading` instead, which is a real thing a person can hit and
  /// is NOT what these checks are about — so they wait for the screen to settle, and say so if
  /// it never does rather than clicking into a curtain and reporting the result.
  ///
  /// `startField()` is driven directly because the picture is normally started from
  /// `requestIdleCallback`, which a headless run may never deliver; `home.js` exports it for
  /// exactly this. A machine with no WebGL takes `degrade()` instead, which also stops that
  /// layer taking the pointer and leaves the way through — both endings settle.
  async function settleOpeningScreen(page) {
    await page.evaluate(() => window.RichHome.startField && window.RichHome.startField());
    await page.waitForFunction(
      () => {
        const l = document.querySelector("#home-loading");
        return !l || l.classList.contains("gone") || getComputedStyle(l).pointerEvents === "none";
      }, null,
      { timeout: 20000 }
    );
    // The 0.8s opacity transition still has to run out before `elementFromPoint` agrees.
    await page.waitForTimeout(250);
  }

  const HANDLER_SPY = () => {
    const PRESS = ["click", "pointerdown", "mousedown", "pointerup", "mouseup"];
    const orig = EventTarget.prototype.addEventListener;
    const wired = new WeakSet();
    // Delegation counts: a press handler on an ancestor wires everything under it. A
    // document- or window-level handler does NOT count — `settings-button.js` has one for
    // "click outside closes the menu", and letting that vouch for every element on the page
    // would make this check pass over anything.
    window.__wiredForPress = (node) => {
      for (let n = node; n; n = n.parentElement) if (wired.has(n)) return true;
      return false;
    };
    EventTarget.prototype.addEventListener = function (type, fn, opts) {
      if (PRESS.indexOf(type) >= 0 && this instanceof Element) wired.add(this);
      return orig.call(this, type, fn, opts);
    };
  };

  await run.check("a job that has only been written down is never counted as running", async () => {
    // **AUDIT-7 ROW 2, ON THE UI SIDE.** Ray's finding was "Rich tells him a job is running
    // when it has already failed ... a CEO who reads the first answer and looks away has been
    // told something false." The same overclaim was on the chip, independently of any backend
    // state: `renderDrillChip` counted `["registered", "preparing", "running"]` together and
    // printed "N assignments running", so an assignment that had been written down and had no
    // workspace yet appeared on his screen as work under way.
    //
    // `AssignmentState`'s doc comments keep the three apart and `work-summary.js` has always
    // relayed them apart — "Written down. Nothing has been prepared yet." / "Getting a
    // workspace ready." / "Running." The chip was the one surface that flattened them, and it
    // flattened them to the strongest of the three.
    //
    // THREE ROWS, ONE OF EACH STATE, so the check can fail in both directions: a chip that
    // said "3 assignments running" and a chip that said "3 assignments starting" are both
    // wrong, and only "2 assignments starting · 1 assignment running" is the screen.
    const row = (id, state, title) => ({
      id,
      title,
      state,
      detail: "",
      repositories: [],
      registeredAtMs: 1,
      canStop: true,
      onTheConnection: false,
      });
    const page = await openApp(browser, undefined, {
      assignments: {
        hiring: [
          row("a-registered", "registered", "adding a line to notes.txt"),
          row("a-preparing", "preparing", "reviewing the hiring plan"),
          row("a-running", "running", "landing the three branches"),
          // CEO §56's state, counted in its own words. It is `is_open` and NOT
          // `awaits_his_word`, so it appears in neither `awaitingYou` nor the other two counts
          // and had no count of its own — see the check below this one for what that costs.
          row("a-screen", "waiting-for-screen", "the overnight import"),
          // Neither counted, and both are on the fixture deliberately: a terminal state must
          // not creep into either number if somebody widens one of the filters later.
          row("a-settled", "settled", "the quarterly summary"),
          row("a-unknown", "unknown", "the branch that was running when we last looked"),
        ],
      },
    });
    await dismissEntityPicker(page);
    await page.click('.nav-thread[data-thread-id="hiring"]');
    await page.waitForFunction(() =>
      document.getElementById("drill-chip-zone").textContent.includes("assignment")
    );
    const chip = await page.evaluate(() => {
      const n = document.querySelector(".drill-chip");
      return { text: (n.textContent || "").trim(), label: n.getAttribute("aria-label") || "" };
    });
    assert(
      chip.text.includes("2 assignments starting"),
      `the chip does not say that two jobs are only starting: ${JSON.stringify(chip.text)}`
    );
    assert(
      chip.text.includes("1 assignment running"),
      `the chip does not say that exactly one job is running: ${JSON.stringify(chip.text)}`
    );
    assert(
      !chip.text.includes("3 assignments running"),
      `the chip counts a job that has only been written down as running: ${JSON.stringify(chip.text)}`
    );
    // CEO §56's state gets its own words and is never folded into either of the other two.
    assert(
      chip.text.includes("1 assignment waiting for the screen"),
      `the chip does not say a job is waiting for the screen: ${JSON.stringify(chip.text)}`
    );
    assert(
      !chip.text.includes("2 assignments running") && !chip.text.includes("3 assignments starting"),
      `a screen-waiting job was folded into another count: ${JSON.stringify(chip.text)}`
    );
    // The spoken form says the same thing. §Options must survive being spoken.
    assert(
      chip.label.includes("2 assignments starting") && chip.label.includes("1 assignment running"),
      `the accessible name disagrees with the chip: ${JSON.stringify(chip.label)}`
    );

    // AND THE PANE BEHIND IT SAYS WHICH IS WHICH, in the words `work-summary.js` already had.
    await page.click(".drill-chip");
    await page.waitForSelector("#slideover-body .assignment-title");
    const said = await page.evaluate(() =>
      Array.from(document.querySelectorAll("#slideover-body .slide-item")).map((n) => (n.textContent || "").trim())
    );
    const written = said.find((s) => s.includes("adding a line to notes.txt")) || "";
    assert(
      written.includes("Written down. Nothing has been prepared yet."),
      `the pane does not say that the written-down job has not been prepared: ${JSON.stringify(written)}`
    );
    assert(
      !/\brunning\b/i.test(written),
      `the pane calls a written-down job running: ${JSON.stringify(written)}`
    );
    assertEqual(page.__errors, [], "the shell logged errors while the chip was read");
    await page.close();
    return `chip: ${JSON.stringify(chip.text)}; the pane says "Written down. Nothing has been prepared yet." for the registered one`;
  });

  await run.check("a job waiting for the screen, alone, still reaches the pane it lives in", async () => {
    // **THE CASE THAT COSTS THE PANE, and it is the whole reason CEO §56's state needs a count
    // of its own rather than a sentence in the pane.** `waiting-for-screen` is `is_open` in
    // `AssignmentState` and deliberately NOT `awaits_his_word` — it resolves itself when the Mac
    // is unlocked, and "waiting for you" would be the nag §56 was given to avoid. So it lands in
    // neither `awaitingYou` nor the running/starting counts.
    //
    // With it as his ONLY open work, `parts` was empty, `drill-chip-zone` was hidden, and the
    // chip is the only way to open the pane the assignments live in — so there was no chip, no
    // pane, and no way to see the work at all. Every other check in this suite was green over
    // it, because every other fixture has something else on the chip. Raised by
    // `echo-opus-screenwait1` as `esc-20260918T114550Z-64ae379a`.
    //
    // ONE ROW, AND NOTHING ELSE ON THE FIXTURE. That is the point: a second row of any other
    // state would put a part on the chip and the check would pass without touching the defect.
    const page = await openApp(browser, undefined, {
      assignments: {
        hiring: [
          {
            id: "only-waiting",
            title: "the overnight import",
            state: "waiting-for-screen",
            detail: "",
            repositories: [],
            registeredAtMs: 1,
            canStop: true,
            onTheConnection: false,
                  },
        ],
      },
    });
    await dismissEntityPicker(page);
    await page.click('.nav-thread[data-thread-id="hiring"]');
    // BOUNDED, AND THE FAILURE IS NAMED. With the count removed this wait can never be
    // satisfied, and a 30s `waitForFunction` reports "Timeout exceeded" — true, and useless to
    // whoever reads it, when the thing to say is "the chip is hidden and the pane is
    // unreachable". Verified by removing the `forScreen` part from `renderDrillChip`.
    const appeared = await page
      .waitForFunction(
        () => document.getElementById("drill-chip-zone").textContent.includes("waiting for the screen"), null,
        { timeout: 8000 }
      )
      .then(() => true)
      .catch(() => false);
    if (!appeared) {
      const stuck = await page.evaluate(() => {
        const zone = document.getElementById("drill-chip-zone");
        return { hidden: zone.hidden, text: (zone.textContent || "").trim() };
      });
      throw new Error(
        `an assignment waiting for the screen is his ONLY open work and the chip does not name it: ` +
          `zone hidden=${stuck.hidden}, text=${JSON.stringify(stuck.text)}. The chip is the only way into ` +
          `the assignments pane, so this is no chip, no pane, and no way to see the work at all`
      );
    }
    const chip = await page.evaluate(() => {
      const zone = document.getElementById("drill-chip-zone");
      const n = zone.querySelector(".drill-chip");
      return {
        zoneHidden: zone.hidden,
        text: n ? (n.textContent || "").trim() : null,
        label: n ? n.getAttribute("aria-label") || "" : null,
      };
    });
    assert(!chip.zoneHidden, "the chip zone is hidden with open work on it — the pane is unreachable");
    assert(chip.text, "there is no chip at all, so there is no way to open the pane");
    assert(
      chip.text.includes("1 assignment waiting for the screen"),
      `the chip does not name the wait in its own words: ${JSON.stringify(chip.text)}`
    );
    assert(
      !/\brunning\b/.test(chip.text) && !/\bstarting\b/.test(chip.text),
      `a job waiting for the screen is described as running or starting: ${JSON.stringify(chip.text)}`
    );
    assert(
      !chip.text.includes("waiting for you"),
      `a wait that resolves itself is presented as his to act on: ${JSON.stringify(chip.text)}`
    );
    // Spoken-safe, and the two waits must not read as one another.
    assert(
      chip.label.includes("1 assignment waiting for the screen") && !chip.label.includes("waiting for you"),
      `the accessible name disagrees with the chip: ${JSON.stringify(chip.label)}`
    );

    // AND THE CHIP OPENS THE PANE, which is the thing that was actually lost.
    await page.click(".drill-chip");
    await page.waitForSelector("#slideover-body .assignment-title");
    const reached = await page.evaluate(() =>
      Array.from(document.querySelectorAll("#slideover-body .assignment-title")).map((n) => (n.textContent || "").trim())
    );
    assertEqual(reached, ["the overnight import"], "the pane does not carry the waiting assignment");
    // The pane's own SENTENCE for this state belongs to `work-summary.js` and arrives with CEO
    // §56's own branch, not this one — this check deliberately asserts the row is REACHABLE
    // rather than what it says, so it cannot go red on a tree where that sentence has not landed
    // yet and cannot go green on one where the row never arrives.
    assertEqual(page.__errors, [], "the shell logged errors while the waiting chip was read");
    await page.close();
    return `chip: ${JSON.stringify(chip.text)}, and it opens a pane carrying "${reached[0]}"`;
  });

  await run.check("PART 6: nothing on the opening screen presents as a control and does nothing", async () => {
    const page = await browser.newPage({ viewport: { width: 1024, height: 700 } });
    const pageErrors = [];
    page.on("pageerror", (e) => pageErrors.push(String(e)));
    await page.addInitScript(HANDLER_SPY);
    await page.goto(APP);
    await page.waitForFunction("typeof window.RichHome === 'object'", null, { timeout: 30000 });
    // The curtain, out of the way — it sits ABOVE this screen and would eat the first click.
    await page.evaluate(() => window.RichSplash && window.RichSplash.yieldNow("acceptance-suite"));
    await page.waitForFunction(() => !document.getElementById("home").hidden);
    await page.waitForSelector("#home-switch #home-enter", { state: "visible" });
    await settleOpeningScreen(page);

    const seen = await page.evaluate(() => {
      const root = document.getElementById("home");
      const out = [];
      for (const n of root.querySelectorAll("*")) {
        const r = n.getBoundingClientRect();
        const s = getComputedStyle(n);
        if (r.width < 1 || r.height < 1) continue;
        if (s.visibility === "hidden" || s.display === "none" || s.pointerEvents === "none") continue;
        const native =
          n.tagName === "BUTTON" ||
          (n.tagName === "A" && n.hasAttribute("href")) ||
          n.getAttribute("role") === "button";
        if (!native && s.cursor !== "pointer") continue;
        // A part INSIDE a control is the outer one's business (`#home-enter > .dot`).
        if (n.parentElement && n.parentElement.closest("button, a[href], [role=button]")) continue;
        out.push({
          sel: n.id ? "#" + n.id : n.tagName + "." + String(n.className || "").trim().split(/\s+/).join("."),
          tag: n.tagName,
          text: (n.textContent || "").trim().slice(0, 34),
          why: native ? "natively interactive" : "cursor: pointer",
          inert: n.getAttribute("data-inert-control"),
          wired: window.__wiredForPress(n),
        });
      }
      return out;
    });
    assertEqual(pageErrors, [], "the opening screen threw while it was being read");
    assert(
      seen.length >= 3,
      `the opening screen offered ${seen.length} controls to test — it has at least the door, its caption and the settings rows`
    );
    const dead = seen
      .filter((c) => !c.wired && !c.inert)
      .map((c) => `${c.sel} (${c.tag}, ${c.why}) ${JSON.stringify(c.text)}`);
    assertEqual(
      dead,
      [],
      "something on the opening screen presents as a control and has no press handler — this is " +
        "audit-7 row 3, and it cost the CEO eleven minutes at his own front door:\n            " +
        dead.join("\n            ")
    );
    await page.close();
    return `${seen.length} presenting-as-a-control element(s) on the opening screen, every one wired: ` +
      seen.map((c) => c.sel).join(", ");
  });

  await run.check("PART 6 POSITIVE CONTROL: a dead control IS caught", async () => {
    // Plant exactly audit-7 row 3 — a word that looks like a control and has no handler —
    // and watch the same criterion find it. Without this, part 6 is a check that has only
    // ever been observed passing.
    const page = await browser.newPage({ viewport: { width: 1024, height: 700 } });
    await page.addInitScript(HANDLER_SPY);
    await page.goto(APP);
    await page.waitForFunction("typeof window.RichHome === 'object'", null, { timeout: 30000 });
    await page.evaluate(() => window.RichSplash && window.RichSplash.yieldNow("acceptance-suite"));
    await page.waitForFunction(() => !document.getElementById("home").hidden);
    const verdict = await page.evaluate(() => {
      const box = document.getElementById("home-switch");
      const dead = document.createElement("p");
      dead.id = "planted-dead-control";
      dead.textContent = "Press me";
      dead.style.cursor = "pointer";
      dead.style.width = "max-content";
      box.appendChild(dead);
      const alive = document.createElement("p");
      alive.id = "planted-live-control";
      alive.textContent = "Press me too";
      alive.style.cursor = "pointer";
      alive.style.width = "max-content";
      alive.addEventListener("click", () => {});
      box.appendChild(alive);
      const r = dead.getBoundingClientRect();
      return {
        deadWired: window.__wiredForPress(dead),
        aliveWired: window.__wiredForPress(alive),
        deadHasBox: r.width > 0 && r.height > 0,
        deadCursor: getComputedStyle(dead).cursor,
      };
    });
    assert(verdict.deadHasBox, "the planted control did not get a box, so nothing was tested");
    assertEqual(verdict.deadCursor, "pointer", "the sweep's own criterion would not have picked the planted control up");
    assertEqual(verdict.deadWired, false, "the spy reported a handler on a paragraph that has none");
    assertEqual(verdict.aliveWired, true, "the spy missed a handler that IS there — it would pass everything");
    await page.close();
    return "a `cursor: pointer` paragraph reads as unwired; the same paragraph with one listener reads as wired";
  });

  await run.check("PART 6: the word `Enter` under the door both answers a click and keeps its promise", async () => {
    // Row 3 in full, with real input rather than through `RichHome.hide()`: the caption is a
    // `<p>`, so a native-button check could never have covered it, and every existing check
    // on this screen drove it through the API. Two launches of the app answered neither the
    // click nor — once the settings control had held focus — the key.
    const page = await browser.newPage({ viewport: { width: 1024, height: 700 } });
    const pageErrors = [];
    page.on("pageerror", (e) => pageErrors.push(String(e)));
    await page.goto(APP);
    await page.waitForFunction("typeof window.RichHome === 'object'", null, { timeout: 30000 });
    await page.evaluate(() => window.RichSplash && window.RichSplash.yieldNow("acceptance-suite"));
    await page.waitForSelector("#home-door-cap", { state: "visible" });
    await settleOpeningScreen(page);

    const capBox = await page.evaluate(() => {
      const r = document.getElementById("home-door-cap").getBoundingClientRect();
      return { x: r.x + r.width / 2, y: r.y + r.height / 2 };
    });
    // The click has to land on the caption itself and not on something painted over it.
    const onTop = await page.evaluate(
      (p) => {
        const el = document.elementFromPoint(p.x, p.y);
        return el ? el.id || el.tagName : null;
      },
      capBox
    );
    assertEqual(onTop, "home-door-cap", "something is painted over the word `Enter`, so this check is not testing it");
    await page.mouse.click(capBox.x, capBox.y);
    await page.waitForFunction(() => document.getElementById("home").hidden, null, { timeout: 30000 });
    const capWhy = await page.evaluate(() => window.RichHome.state.lastLeaveReason);

    // ...and the KEY still works after the settings control has held focus, which is where it
    // stopped working. `settings-button.js` gives focus back to `#set-btn` when the menu
    // closes, and the old refusal keyed on `.settings` containing the focused element, so one
    // click on the only other control on the screen killed the key for the rest of the launch.
    await page.evaluate(() => window.RichHome.show("acceptance-suite"));
    await page.waitForFunction(() => !document.getElementById("home").hidden);
    await page.click("#set-btn");
    await page.waitForFunction(() => !document.getElementById("set-menu").hidden);
    await page.keyboard.press("Escape");
    await page.waitForFunction(() => document.getElementById("set-menu").hidden);
    const focus = await page.evaluate(() => (document.activeElement && document.activeElement.id) || null);
    assertEqual(focus, "set-btn", "the settings control no longer keeps focus after Escape, so this is testing nothing");
    await page.keyboard.press("Enter");
    await page.waitForFunction(() => document.getElementById("home").hidden, null, { timeout: 30000 });
    const keyWhy = await page.evaluate(() => window.RichHome.state.lastLeaveReason);
    assertEqual(keyWhy, "enter-key", "Return did not enter while the settings control held focus");

    // AND THE MENU'S OWN Enter IS STILL THE MENU'S. Open it, put focus on a row, press Return:
    // the screen must stay, because that keystroke belongs to the row.
    await page.evaluate(() => window.RichHome.show("acceptance-suite"));
    await page.waitForFunction(() => !document.getElementById("home").hidden);
    await page.click("#set-btn");
    await page.waitForFunction(() => !document.getElementById("set-menu").hidden);
    await page.evaluate(() => document.getElementById("font-up").focus());
    const leaveBefore = await page.evaluate(() => window.RichHome.state.lastLeaveReason);
    await page.keyboard.press("Enter");
    const after = await page.evaluate(() => ({
      open: window.RichHome.isOpen(),
      reason: window.RichHome.state.lastLeaveReason,
    }));
    assertEqual(
      after.reason,
      leaveBefore,
      `Return on a row of the OPEN settings menu recorded a leave ("${after.reason}") — the screen went`
    );
    assert(after.open, "Return inside the open settings menu walked out of the opening screen");

    // AND THE ACCESSIBILITY TREE CARRIES ONE DOOR, NOT A STRAY WORD BESIDE IT — audit-8 row 3's
    // other half. Ray's accessibility inspector on candidate .8 reported `static text Enter`
    // beside `button Talk to Rich`, so a screen-reader user met an unlabeled "Enter" with no
    // role. The fact the word carries is in the tree where the platform looks for it, on the
    // door; the word itself is a redundant rendering of it and is declared decoration.
    await page.evaluate(() => window.RichHome.show("acceptance-suite"));
    await page.waitForFunction(() => !document.getElementById("home").hidden);
    const tree = await page.evaluate(() => {
      const cap = document.getElementById("home-door-cap");
      const door = document.getElementById("home-enter");
      return {
        capHidden: cap.getAttribute("aria-hidden"),
        capRole: cap.getAttribute("role"),
        capTabIndex: cap.getAttribute("tabindex"),
        capText: (cap.textContent || "").trim(),
        doorRole: door.tagName,
        doorName: (door.textContent || "").trim(),
        doorKeys: door.getAttribute("aria-keyshortcuts"),
      };
    });
    assertEqual(tree.capHidden, "true", "the word `Enter` is still announced as a stray unlabeled node beside the door");
    assertEqual(tree.capRole, null, "the caption claims a role — then it is a second control with the door's own name");
    assertEqual(tree.capTabIndex, null, "the caption is a tab stop — then it is a second control in the tab order");
    assertEqual(tree.capText, "Enter", "the word is no longer on screen, which is the ruling's own wording");
    assertEqual(tree.doorKeys, "Enter", "the door no longer carries the shortcut, so the hidden word's fact left the tree with it");
    assertEqual(tree.doorRole, "BUTTON", "the door is not a button");

    assertEqual(pageErrors, [], "the opening screen threw during the walk");
    await page.close();
    return (
      `a click on the word leaves ("${capWhy}"); Return leaves ("${keyWhy}") with #set-btn focused; ` +
      `Return inside the open menu does not; the tree carries one BUTTON "${tree.doorName}" with ` +
      `aria-keyshortcuts="${tree.doorKeys}" and the word "${tree.capText}" is aria-hidden with no role and no tab stop`
    );
  });

  await run.check("POSITIVE CONTROL: an unclassified new state IS flagged", async () => {
    // The drift comparator, run against a corpus with one extra string, so the part-1 check
    // is proven able to fail rather than merely observed passing.
    const fakeInv = inv.concat([{ normal: "A brand new thing the CEO must go and do.", sites: ["fake.js:1"] }]);
    const unclassified = fakeInv.filter((r) => !byString.has(r.normal));
    assertEqual(
      unclassified.map((r) => r.normal),
      ["A brand new thing the CEO must go and do."],
      "the drift comparator did not notice an unclassified state"
    );
    return "the comparator names the exact string and its site";
  });

  await browser.close();

  // ---- PART 5: the heuristic second opinion — REPORT ONLY --------------------------------

  // Only strings the registry knows. An UNCLASSIFIED string means part 1 has already
  // failed and named it; part 5 must not crash on the way to reporting that — a reporting
  // layer that dies before it prints the verdict is how a check goes silently dead, which
  // this session has already caught once.
  const flagged = inv.map((r) => r.normal).filter((s) => byString.has(s) && readsActionable(s));
  const genuine = flagged.filter((s) => ACTION_CLASSES.indexOf(byString.get(s).c) >= 0);
  const allAction = REGISTRY.filter((r) => ACTION_CLASSES.indexOf(r.c) >= 0);
  const precision = (100 * genuine.length / flagged.length).toFixed(1);
  const recall = (100 * genuine.length / allAction.length).toFixed(1);

  const failed = run.report();

  console.log("\n== part 5: heuristic second opinion — REPORT ONLY, and here is why ==");
  console.log(
    `  flagged ${flagged.length}, genuinely ACTIONABLE or NEEDS-SOMEONE-ELSE ${genuine.length}` +
      `  ->  precision ${precision}%, recall ${recall}%`
  );
  console.log(
    "  Roughly one flag in three is noise and it misses nearly half the real ones, so it does"
  );
  console.log(
    "  NOT fail the run. A check that cries wolf is deleted within a day, and then nothing runs."
  );
  const disagreements = flagged.filter((s) => ACTION_CLASSES.indexOf(byString.get(s).c) < 0);
  console.log(`  ${disagreements.length} disagreement(s) with the registry, each with its recorded reason:`);
  for (const s of disagreements) {
    const r = byString.get(s);
    console.log(`    [${r.c}] ${JSON.stringify(s.slice(0, 74))}`);
    console.log(`           ${r.why.slice(0, 110)}`);
  }

  console.log(
    failed
      ? `\n${failed} check(s) FAILED — an actionable state is rendering without its control.`
      : "\nthe affordance rule holds."
  );
  process.exitCode = failed ? 1 : 0;
}

main().catch((e) => {
  console.error(e);
  process.exit(1);
});

"use strict";
// DICTATION: THE SETTINGS ROW AND THE DICTATION SHEET. Round 19 (richos-hq
// design/mockups/rounds/round-19/dictation.html, states 5 to 12) and its more lines
// (dictation-more-lines.html: line 2, "Works only while RichOS is open", now for every copy, the
// CEO's 2026-10-08 decision that dictation runs only while RichOS runs; line 4, another app
// hiding keys; line 1, "On even when RichOS is closed", is no longer true anywhere and is not
// drawn), built to slice 2 of the dictation plan
// (richos-hq docs/plans/2026-10-08-dictation-anywhere.md revision 2, section 9; section 2 rows 5
// to 12; section 4). Measured against the CEO's words: "it all looks awesome. ... yes, also keep
// the sentence Iris wrote for when macOS shows when RichOS asks for the microphone." (2026-10-08)
// and "The user doesn't need to know that there is 'another hidden copy of RichOS in the
// background'. They only need to know what it looks like from their non-technical point of
// view." (2026-10-08): no line here names a copy of RichOS or anything running in the background.
//
// - THE ROW, behind DICTATION_READY: "Dictation" after Claude accounts, with its state line (Off,
//   On. Tap F1 to talk, Needs a permission, Waiting for macOS, Starting, Not working yet,
//   Paused: <app> is hiding your keys).
// - NEVER "ON" WITHOUT A WORKING TOOL (recheck finding 6): with no tool holding the key the
//   sheet says "Starting dictation…", and with a tool that watches no key "Not working yet: the
//   key is not being watched. Turn the switch off and on again." Both lines are new here.
// - THE SHEET: the switch, Your key (the strip, Press a different key), What macOS asks you for,
//   Accuracy, and on the right Try it here, the three bar states, the difference from talking to
//   Rich, and privacy.
// - TURNING IT ON asks macOS in the drawn order, the microphone and then Accessibility
//   (dictation_app.rs `Host`); while it waits for System Settings it reads the state once a
//   second, and when Accessibility is allowed RichOS comes back to the front on the sheet.
// - `window.RichDictation.turnOn`, which Rich's offer calls (main.js `answerDictationOffer`):
//   opens the sheet, runs the same flow, and answers `{on}` once it has settled.
// - THE OFF NOTICE: the chosen key pressed in RichOS's own window while dictation is off.
// - NO LINE UNDER THE MESSAGE BOX. Dictation is for other apps; inside RichOS, Talk to Rich is
//   the way to speak (the CEO, 2026-10-13, onboarding4.png: "Dictation is for other apps only,
//   not for RichOS"), so the composer's dictation line is gone.
//
// Every word is round 19's, quoted from the two mockup files. Every fact is the app's own
// (`dictation_status`); nothing here invents a state.
(function () {
  const bridge = window.RichBridge;
  if (!bridge) return;
  const esc = s => String(s == null ? "" : s).replace(/[&<>"]/g, c => ({ "&": "&amp;", "<": "&lt;", ">": "&gt;", "\"": "&quot;" }[c]));
  const ICON = {
    mic: '<svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2.4" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true"><rect x="9" y="3" width="6" height="11" rx="3"/><path d="M5 11a7 7 0 0 0 14 0"/><path d="M12 18v3"/></svg>',
    cursor: '<svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2.2" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true"><path d="M9 4h2a2 2 0 0 1 1 .3A2 2 0 0 1 13 4h2"/><path d="M9 20h2a2 2 0 0 0 1-.3 2 2 0 0 0 1 .3h2"/><path d="M12 4.5v15"/><path d="M8 12h8"/></svg>',
    check: '<svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2.6" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true"><path d="M5 12.5l4.5 4.5L19 7.5"/></svg>',
    info: '<svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true"><circle cx="12" cy="12" r="9"/><path d="M12 11v5"/><path d="M12 7.6v.4"/></svg>',
    lock: '<svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true"><rect x="5" y="11" width="14" height="10" rx="2"/><path d="M8 11V7a4 4 0 0 1 8 0v4"/></svg>',
    close: '<svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2.2" stroke-linecap="round" aria-hidden="true"><path d="M6 6l12 12M18 6L6 18"/></svg>',
  };
  // Round 19's words, word for word (dictation.html and dictation-more-lines.html).
  const OFF_NOTICE = "Dictation is off. Turn it on in Settings, under Dictation.";
  const MODIFIER_REFUSED = "Command, Option, Control and Shift can't be used: anything you type while one is down would set off shortcuts. Use a function key, F1 to F19.";
  const TYPING_REFUSED = "That key types text. Use a function key, F1 to F19.";
  const MODIFIERS = ["Shift", "Control", "Alt", "Meta", "CapsLock", "Fn"];

  let view = null; // dictation_status
  let axWait = null; // { blurred } while the window waits for Accessibility in System Settings
  let micWait = null; // { blurred } while it waits for the microphone in System Settings
  let capture = null; // { err } while "Press a different key" waits
  let feedback = null; // "on": the drawn "Dictation is on. Try it in the box on the right or in any app."
  let saved = false, savedTimer = null, lastRead = 0, reading = false, registered = false, nudged = false;
  let waiters = []; // RichDictation.turnOn's answers, given when the flow settles
  let flow = false;

  // ---- the facts, as the window draws them -----------------------------------------------
  const keyName = () => "F" + (view ? view.key : 1);
  // While the window waits for System Settings, a refused permission reads as being asked; while
  // turning it on, one not asked yet is the one macOS is about to ask for, so the card and the
  // rows never disagree for the moment between two prompts.
  function waiting(value, wait, next) {
    if (value === "allowed") return value;
    if (wait || (flow && next && view.on && value === "unknown")) return "asking";
    return value;
  }
  // Accessibility is next only once the microphone is allowed (round 19 state 7: the microphone
  // is being asked, Accessibility still says "Asked when you turn it on").
  const mic = () => !view ? "unknown" : waiting(view.mic, micWait, true);
  const ax = () => !view ? "unknown" : waiting(view.ax, axWait, view.mic === "allowed");
  // On, with another copy's tool holding the key: that tool types, so this copy's own
  // permissions decide nothing (plan section 7, "Two copies with dictation on": the other copy
  // says line 1). Seen in walk-7350bc140616: a second copy without its own grants said "Needs a
  // permission".
  const elsewhere = () => !!view && view.on && view.owner === "other";
  // On and both allowed (round 19 `ready()`), or on elsewhere.
  const ready = () => !!view && view.on && (elsewhere() || (mic() === "allowed" && ax() === "allowed"));
  // A tool holds the key and watches it: the sheet never says On without one (recheck finding 6).
  const toolWorking = () => !!view && view.owner !== "none" && !!view.keyTap;
  // Ready, a working tool, and nothing outside RichOS is stopping the key (more lines `works()`).
  const works = () => ready() && toolWorking() && !view.secure;
  const secureApp = () => view && view.secure && view.secure.app ? view.secure.app : null;
  const kcap = k => `<span class="dict-kcap">${esc(k)}</span>`;
  // Line 4, said the same way in the row's sheet and (slice 3) in the menu bar menu.
  function secureHead(k) {
    const a = secureApp();
    return a ? `<span class="dict-who">${esc(a)}</span> is hiding your keys from other apps, so ${kcap(k)} can't reach me.`
      : `Another app is hiding your keys, so ${kcap(k)} can't reach me.`;
  }
  function secureFix() {
    const a = secureApp();
    return a ? `Quitting ${esc(a)} fixes it.` : "Quitting the app where you last typed a password usually fixes it.";
  }

  async function call(cmd, args) {
    try { return await bridge.invoke(cmd, args || {}); }
    catch (e) { console.warn("[dictation] " + cmd + ": " + e); return null; }
  }
  async function read() {
    if (reading) return view;
    reading = true;
    try {
      const v = await call("dictation_status");
      if (v) {
        // The first read of a launch: dictation left on may never have been set up (he turned it
        // on, Accessibility was refused, and he quit, allowed it and reopened RichOS), so it is
        // set up once it works. The app opens the "Dictation feature" conversation only when it
        // never did (`dictation_offer::feature_action`), so this never makes a second one
        // (second review rv-20261013T193550Z, finding 1).
        if (!view && v.on) setUpPending = true;
        view = v; lastRead = Date.now();
      }
    } finally { reading = false; }
    if (view && view.ready && !registered && window.RichSettings && window.RichSettings.registerDictation) {
      registered = true;
      window.RichSettings.registerDictation({ open, paint: paintRow });
    }
    // Accessibility allowed while nothing was waiting (allowed in System Settings with the sheet
    // closed): the tool makes its key tap now, once per connection.
    if (view && view.on && view.owner === "self" && view.ax === "allowed" && !view.keyTap) {
      if (!nudged) { nudged = true; call("dictation_permissions_changed", { forward: false }); }
    } else nudged = false;
    paint();
    // A turn-on that settled short of ready, finished later with the sheet closed (allowed in
    // System Settings, then back here): set up now (second review rv-20261013T183104Z, finding 1).
    if (sheet.hidden) announceSetUp();
    return view;
  }

  // ---- the Settings row ------------------------------------------------------------------
  // Round 19 `dictSub`, with the more lines.
  function rowLine() {
    if (!view.on) return ["Off", false];
    if (!elsewhere() && (mic() !== "allowed" || ax() !== "allowed")) return [mic() === "asking" || ax() === "asking" ? "Waiting for macOS" : "Needs a permission", true];
    if (view.secure) return [secureApp() ? `Paused: ${secureApp()} is hiding your keys` : "Paused: an app is hiding your keys", true];
    if (view.owner === "none") return ["Starting", false];
    if (!view.keyTap) return ["Not working yet", true];
    return [`On. Tap ${keyName()} to talk`, false];
  }
  function paintRow() {
    const line = document.getElementById("set-dictation-state");
    if (!line || !view) return;
    const [text, attention] = rowLine();
    line.textContent = text;
    line.hidden = false;
    line.classList.toggle("is-attention", attention);
  }

  // ---- the sheet -------------------------------------------------------------------------
  // BUILT THE FIRST TIME IT OPENS, NOT AT LOAD: a build where dictation is not there
  // (DICTATION_READY false) carries not one node of it, and the shell every other screen is
  // measured against (tests/contrast.js counts every text node, hidden or not) is unchanged.
  const sheet = document.createElement("div");
  sheet.id = "dictation-sheet"; sheet.className = "overlay"; sheet.hidden = true;
  sheet.setAttribute("role", "dialog");
  sheet.setAttribute("aria-labelledby", "dict-title"); sheet.setAttribute("data-dismiss", "control:#dict-close");
  const $ = id => document.getElementById(id);
  let col = null, tryBox = null, tryNote = null;
  function build() {
    if (sheet.isConnected) return;
    sheet.innerHTML = `<section class="overlay-panel dict-panel">
    <header class="dict-heading"><div><p class="dict-eyebrow">Settings</p><h2 id="dict-title">Dictation</h2>
      <p class="dict-lede">Type with your voice in any app on your Mac. Your words appear where your cursor is.</p></div>
      <button id="dict-close" type="button" aria-label="Close Dictation">${ICON.close}</button></header>
    <div class="dict-body"><div id="dict-col" class="dict-col"></div>
      <div class="dict-how">
        <h3 class="dict-how-title">Try it here</h3>
        <div id="dict-try" class="dict-try" contenteditable="true" spellcheck="false" role="textbox" aria-multiline="true" aria-label="Try dictation here" data-ph=""></div>
        <p id="dict-try-note" class="dict-how-p is-soft" hidden></p>
        <div class="dict-how-sec">
          <p class="dict-how-h">While you talk</p>
          <p class="dict-how-p is-soft">A small bar at the bottom of your screen shows I'm listening. It never takes your cursor away.</p>
          <div class="dict-samples" aria-hidden="true">
            <div class="dict-bar is-listen" id="dict-sample-listen"><span class="dict-orb">${ICON.mic}</span><span class="dict-meter"><i></i><i></i><i></i><i></i><i></i><i></i><i></i></span><span class="dict-label">Listening</span><span class="dict-time">0:03</span></div>
            <div class="dict-bar is-write"><span class="dict-orb"></span><span class="dict-label">Writing it down&hellip;</span></div>
            <div class="dict-bar is-done"><span class="dict-orb">${ICON.check}</span><span class="dict-label">Added</span></div>
          </div>
        </div>
        <div class="dict-how-sec">
          <p class="dict-how-h">Not the same as talking to me</p>
          <p class="dict-how-p is-soft">The round button beside my message box sends what you say to me. Dictation only types. You read it, fix it if you like and send it yourself.</p>
        </div>
        <div class="dict-how-sec dict-priv">${ICON.lock}<p class="dict-how-p"><b>Your voice stays on this Mac.</b> Nothing you say is sent anywhere.</p></div>
      </div></div></section>`;
    document.body.appendChild(sheet);
    col = $("dict-col"); tryBox = $("dict-try"); tryNote = $("dict-try-note");
    $("dict-close").addEventListener("click", () => close());
  }

  function permState(v, pane) {
    if (v === "allowed") return `<span class="dict-perm-ok">${ICON.check}Allowed</span>`;
    if (v === "asking") return `<span class="dict-perm-wait"><span class="dict-ring" aria-hidden="true"></span>macOS is asking you</span>`;
    if (v === "denied") return `<span class="dict-perm-no">Not allowed</span><button class="dict-btn" type="button" data-act="open-sys" data-pane="${pane}">Open System Settings</button>`;
    return `<span class="dict-perm-wait">Asked when you turn it on</span>`;
  }
  // The strip: esc, then seven keys from F1; a chosen key past F7 moves the window of keys.
  function keysHtml() {
    const n = view.key;
    let start = 1;
    if (n > 7) start = Math.min(n - 3, 13);
    const keys = [];
    if (start === 1) keys.push(`<span class="dict-key is-esc" aria-hidden="true">esc</span>`);
    for (let i = start; i < start + 7 && i <= 19; i++) keys.push(`<button class="dict-key" type="button" role="radio" aria-checked="${n === i}" data-act="key" data-k="${i}">F${i}</button>`);
    return `<div class="dict-keys" role="radiogroup" aria-label="Dictation key">${keys.join("")}</div>`;
  }
  function sheetHtml() {
    const k = keyName();
    let st, cls = "";
    if (!view.on) st = "Off. Turn it on to type with your voice in Mail, Slack, your browser, anywhere.";
    else if (!elsewhere() && (mic() === "asking" || mic() === "unknown")) st = "Waiting for you to allow the microphone&hellip;";
    else if (!elsewhere() && mic() === "denied") { st = "Not working yet: macOS has not allowed the microphone."; cls = " is-attention"; }
    else if (!elsewhere() && (ax() === "asking" || ax() === "unknown")) st = "Waiting for you to allow Accessibility&hellip;";
    else if (!elsewhere() && ax() === "denied") { st = "Not working yet: macOS has not let me type into other apps."; cls = " is-attention"; }
    // line 4: another app left Secure Event Input on
    else if (view.secure) { st = `Paused: ${secureHead(k)} <span class="dict-fix">${secureFix()}</span>`; cls = " is-attention"; }
    // no tool holds the key yet (it is starting, or it could not start): never "On"
    else if (view.owner === "none") { st = "Starting dictation&hellip;"; }
    // the tool runs but watches no key: never "On" (recheck finding 6)
    else if (!view.keyTap) { st = "Not working yet: the key is not being watched. Turn the switch off and on again."; cls = " is-attention"; }
    // On, whichever copy's tool holds the key: round 19's line, true for both
    else { st = `On. Tap ${kcap(k)} in any app, talk and tap it again.`; cls = " is-on"; }
    const warn = view.on && ((!elsewhere() && (mic() === "denied" || ax() === "denied")) || (ready() && !!view.secure) || (ready() && view.owner !== "none" && !view.keyTap));
    // The gold card only with a tool that holds the key and watches it (recheck finding 6).
    let html = `<div class="dict-card${view.on && !warn && toolWorking() ? " is-on" : ""}${warn ? " is-warn" : ""}"><span class="dict-spine"></span>
      <div class="dict-card-main"><h3 class="dict-card-title">Dictate in any app</h3><p class="dict-card-sub${cls}" id="dict-state">${st}</p></div>
      <button class="dict-switch" id="dict-switch" type="button" role="switch" data-act="toggle-on" aria-checked="${view.on}" aria-label="Dictate in any app"></button></div>`;
    // line 2: dictation works only while RichOS is open (every copy; the CEO, 2026-10-08), on or off
    html += `<p class="dict-copy-note" id="dict-copy-note">${ICON.info}<span><b>Works only while RichOS is open.</b> When you quit RichOS, dictation stops until you open it again.</span></p>`;
    if (feedback === "on" && works()) html += `<p class="dict-feedback" id="dict-feedback" role="status">${ICON.check}<span>Dictation is on. Try it in the box on the right or in any app.</span></p>`;
    // the key
    html += `<section class="dict-sec"><div class="dict-sec-h"><h4 class="dict-sec-t">Your key</h4><span class="dict-sec-note">Tap once to start, once more to stop. Nothing to hold down.</span></div>`;
    if (capture) {
      html += `<div class="dict-key-wait"><span class="dict-key" aria-hidden="true">?</span><p class="dict-key-say"><b>Press the key you want to use.</b> Esc to cancel.</p><button class="dict-btn" id="dict-capture-cancel" type="button" data-act="capture-cancel">Cancel</button></div>`;
      html += capture.err ? `<p class="dict-key-err" id="dict-key-err" role="alert">${esc(capture.err)}</p>` : `<p class="dict-key-say is-soft">Any function key works, F1 to F19.</p>`;
    } else {
      html += keysHtml();
      html += `<div class="dict-key-row"><p class="dict-key-say">${k === "F1" ? "<b>F1</b> is the key just right of <b>esc</b>." : `<b>${esc(k)}</b> starts and stops dictation.`} Pick one you rarely use.</p><button class="dict-btn dict-btn-quiet" id="dict-capture" type="button" data-act="capture">Press a different key</button></div>`;
    }
    html += `</section>`;
    // permissions
    html += `<section class="dict-sec"><div class="dict-sec-h"><h4 class="dict-sec-t">What macOS asks you for</h4><span class="dict-sec-note">Once, when you turn it on.</span></div>
      <div class="dict-perm" data-perm="microphone"><span class="dict-perm-ic">${ICON.mic}</span><span><span class="dict-perm-t">Microphone</span><span class="dict-perm-s">So I can hear you, only while you dictate.</span></span><span class="dict-perm-state">${permState(mic(), "microphone")}</span></div>
      <div class="dict-perm" data-perm="accessibility"><span class="dict-perm-ic">${ICON.cursor}</span><span><span class="dict-perm-t">Accessibility</span><span class="dict-perm-s">So I can type your words where your cursor is, in any app.</span></span><span class="dict-perm-state">${permState(ax(), "accessibility")}</span></div></section>`;
    // accuracy
    html += `<section class="dict-sec"><div class="dict-sec-h"><h4 class="dict-sec-t" id="dict-acc-h">Accuracy</h4>${saved ? `<span class="dict-saved" role="status">Saved.</span>` : ""}</div>
      <div class="dict-choice" role="radiogroup" aria-labelledby="dict-acc-h">
        <button class="dict-opt" type="button" role="radio" data-act="accuracy" data-v="accurate" aria-checked="${view.accuracy !== "fast"}"><span class="dict-radio"></span><span><span class="dict-opt-t">More accurate</span><span class="dict-opt-s">Gets names and jargon right more often. Your words appear a second or two after you stop.</span></span></button>
        <button class="dict-opt" type="button" role="radio" data-act="accuracy" data-v="fast" aria-checked="${view.accuracy === "fast"}"><span class="dict-radio"></span><span><span class="dict-opt-t">Faster</span><span class="dict-opt-s">Your words appear in about half a second. Names are misheard a little more often.</span></span></button>
      </div></section>`;
    return html;
  }
  function paintSheet() {
    if (sheet.hidden || !view || !col) return;
    const html = sheetHtml();
    if (html !== col.dataset.painted) {
      const active = document.activeElement && col.contains(document.activeElement) ? document.activeElement : null;
      const back = active ? (active.id || (active.dataset.act + ":" + (active.dataset.k || active.dataset.v || active.dataset.pane || ""))) : null;
      col.innerHTML = html;
      col.dataset.painted = html;
      if (back) {
        const [act, val] = back.split(":");
        const again = $(back) || col.querySelector(`[data-act="${act}"]${val ? `[data-k="${val}"],[data-act="${act}"][data-v="${val}"],[data-act="${act}"][data-pane="${val}"]` : ""}`);
        if (again) again.focus();
      }
    }
    tryBox.dataset.ph = works() ? `Click here, tap ${keyName()} and say something.`
      : ready() && view.secure ? "You can try it here once your keys are back."
      : ready() && !toolWorking() ? "You can try it here once dictation has started."
      : view.on ? "Allow both permissions on the left to try it here." : "Turn dictation on to try it here.";
    tryNote.textContent = works() ? "This box is only for trying. Your words go wherever your cursor is." : "";
    tryNote.hidden = !works();
  }
  function paint() {
    paintRow();
    paintSheet();
  }

  // ---- the sample bar breathes, while the sheet shows and motion is welcome ---------------
  // Only while the sheet shows: `open()` starts it, and it stops itself once the sheet is shut.
  const still = window.matchMedia && window.matchMedia("(prefers-reduced-motion: reduce)").matches;
  let breathing = false;
  function breathe(now) {
    if (sheet.hidden || still) { breathing = false; return; }
    {
      const W = [.5, .75, .92, 1, .92, .75, .5];
      const level = .55 + .35 * Math.abs(Math.sin(now / 420)) * (.7 + .3 * Math.sin(now / 1300));
      const bars = sheet.querySelectorAll("#dict-sample-listen .dict-meter i");
      bars.forEach((b, i) => { b.style.transform = `scaleY(${Math.max(.12, level * W[i] * (.85 + .15 * Math.sin(now / 80 + i)))})`; });
      const sample = $("dict-sample-listen");
      if (sample) sample.style.setProperty("--lvl", level.toFixed(3));
    }
    requestAnimationFrame(breathe);
  }

  // ---- turning it on: the microphone, then Accessibility, in the drawn order --------------
  function settle() {
    flow = false;
    const answer = { on: ready() };
    const pending = waiters; waiters = [];
    pending.forEach(resolve => resolve(answer));
  }
  const sleep = ms => new Promise(r => setTimeout(r, ms));
  function placeCaret() {
    if (sheet.hidden || !tryBox) return;
    tryBox.focus({ preventScroll: true });
    const range = document.createRange(); range.selectNodeContents(tryBox); range.collapse(false);
    const sel = getSelection(); sel.removeAllRanges(); sel.addRange(range);
  }
  // The line under the card ("Dictation is on. Try it in the box on the right or in any app.")
  // is DECIDED here, where the flow reaches On, on round 19's own condition (dictation.html
  // `turnOn`, `afterGrant`, `ss-tog`: `if (ready()) S.feedback = "on"`), and DRAWN by `sheetHtml`
  // only once the tool holds the key and watches it (`works()`; never On without a working tool).
  // D18 (Ray's candidate 46 walk, runs A2 and T3): deciding it on `works()` here left it unset
  // for good, because the tool reports its key tap after the window has told it the permission
  // changed (0.09 s after the switch in System Settings, walk-2882d6930ecc), and nothing decided
  // it again once the tap came.
  function becameReady() {
    if (ready()) feedback = "on";
    paint();
    settle();
    placeCaret();
    if (sheet.hidden) announceSetUp();
  }
  // **SET UP, SAID ONCE** (the CEO, 2026-10-13, onboarding5.png): a turn-on that worked is
  // announced as `richos:dictation-set-up` when the sheet closes, so the key named in the
  // "Dictation feature" conversation is the one he picked in the sheet (main.js listens). If it
  // became ready with the sheet already closed (Accessibility allowed later), it is said then.
  let setUpPending = false;
  function announceSetUp() {
    if (!setUpPending || !ready()) return;
    setUpPending = false;
    window.dispatchEvent(new CustomEvent("richos:dictation-set-up"));
  }
  async function turnOn() {
    if (flow) return;
    flow = true; feedback = null; setUpPending = true;
    const v = await call("dictation_set_on", { on: true });
    if (v) view = v;
    await proceed();
  }
  async function proceed() {
    flow = true;
    paint();
    if (!view || !view.on) return settle();
    // Another copy has the key: nothing here to ask macOS for.
    if (elsewhere()) return becameReady();
    if (view.mic === "unknown") {
      const v = await call("dictation_ask_microphone");
      if (v) view = v;
      paint();
      // macOS answers on its own; read it once a second until it has.
      for (let i = 0; view && view.on && view.mic === "asking" && i < 600; i++) { await sleep(1000); await read(); }
    }
    if (!view || !view.on || view.mic !== "allowed") { paint(); return settle(); }
    if (view.ax !== "allowed") {
      const prompted = await call("dictation_ask_accessibility");
      if (prompted === true) { axWait = { blurred: false }; paint(); watch(); return; }
      await read();
      if (view && view.ax === "allowed") return becameReady();
      return settle();
    }
    becameReady();
  }
  async function turnOff() {
    axWait = null; micWait = null; feedback = null; setUpPending = false;
    const v = await call("dictation_set_on", { on: false });
    if (v) view = v;
    paint();
    settle();
  }

  // While the window waits for System Settings it reads the state once a second. Accessibility
  // allowed: the tool makes its key tap, RichOS comes to the front on the sheet. The microphone
  // allowed there: the flow goes on to Accessibility.
  let watching = null;
  function watch() {
    if (watching) return;
    watching = setInterval(async () => {
      if (!axWait && !micWait) { clearInterval(watching); watching = null; return; }
      await read();
      if (!view) return;
      if (axWait && view.ax === "allowed") {
        axWait = null;
        await call("dictation_permissions_changed", { forward: true });
        await read();
        becameReady();
      } else if (micWait && view.mic === "allowed") {
        micWait = null;
        await call("dictation_permissions_changed", { forward: true });
        if (view.on) proceed();
        else paint();
      }
    }, 1000);
  }
  // macOS's Accessibility prompt and System Settings take the front from RichOS. When RichOS has
  // the front again and is still not allowed, he said no (or closed System Settings): the drawn
  // denied line, with Open System Settings.
  addEventListener("blur", () => { if (axWait) axWait.blurred = true; if (micWait) micWait.blurred = true; });
  addEventListener("focus", () => {
    // Back from System Settings with a turn-on still pending and the sheet closed: read once, so
    // a permission allowed there finishes setting dictation up.
    if (setUpPending && sheet.hidden && !axWait && !micWait) { read(); return; }
    if (!(axWait && axWait.blurred) && !(micWait && micWait.blurred)) return;
    setTimeout(async () => {
      await read();
      if (axWait && axWait.blurred && view && view.ax !== "allowed") { axWait = null; paint(); settle(); }
      if (micWait && micWait.blurred && view && view.mic !== "allowed") { micWait = null; paint(); settle(); }
    }, 1500);
  });

  // ---- choosing the key -------------------------------------------------------------------
  async function pickKey(n) {
    const wasCapturing = !!capture;
    capture = null;
    if (wasCapturing) call("dictation_capture_key", { on: false });
    const v = await call("dictation_set_key", { key: n });
    if (v) view = v;
    paint();
    // The "Dictation feature" message names the key; a new key replaces it (main.js listens).
    if (v) window.dispatchEvent(new CustomEvent("richos:dictation-key-changed"));
    const strip = (col && col.querySelector(`[data-act="key"][data-k="${n}"]`)) || $("dict-capture");
    if (strip) strip.focus();
    if (window.RichSettings && window.RichSettings.toast) window.RichSettings.toast(`Dictation now starts and stops with F${n}.`);
  }
  async function startCapture() {
    capture = { err: null };
    paint();
    const cancel = $("dict-capture-cancel");
    if (cancel) cancel.focus();
    // Through the tool's key tap when there is one, so an Apple top-row key is captured as the
    // key it is; the window's own keys answer too.
    await call("dictation_capture_key", { on: true });
  }
  function stopCapture() {
    if (!capture) return;
    capture = null;
    call("dictation_capture_key", { on: false });
    paint();
    const again = $("dict-capture");
    if (again) again.focus();
  }
  if (bridge.listen) {
    // The tool's menu bar menu (slice 3) writes `on` and the accuracy itself, and its Fix it and
    // Dictation settings… ask for this sheet: read again, or open it.
    bridge.listen("dictation-settings-changed", () => read());
    // A sheet request is kept by the app until a page takes it (third review, finding 4): this
    // page takes it on the event, and as it boots, so a request made while no page existed
    // (RichOS kept running by its work with the window closed) still opens the sheet.
    bridge.listen("dictation-open-sheet", () => { call("dictation_take_sheet_request"); open(); });
    call("dictation_take_sheet_request").then(pending => { if (pending === true) open(); });
    bridge.listen("rich://dictation", event => {
      const p = event && event.payload !== undefined ? event.payload : event;
      if (p && p.key && capture) { pickKey(Number(p.key)); return; }
      read();
    });
  }

  // ---- the sheet's controls ---------------------------------------------------------------
  sheet.addEventListener("click", async event => {
    if (event.target === sheet) { close(); return; }
    const b = event.target.closest("[data-act]");
    if (!b || !sheet.contains(b)) return;
    switch (b.dataset.act) {
      case "toggle-on": {
        // Said at once, so a second press reads the switch it sees rather than the one in flight.
        const on = !(view && view.on);
        if (view) view.on = on;
        if (on) turnOn(); else turnOff();
        break;
      }
      case "open-sys": {
        if (b.dataset.pane === "accessibility") axWait = { blurred: false };
        else if (b.dataset.pane === "microphone") micWait = { blurred: false };
        paint(); watch();
        await call("dictation_open_settings", { pane: b.dataset.pane });
        break;
      }
      case "key": pickKey(Number(b.dataset.k)); break;
      case "capture": startCapture(); break;
      case "capture-cancel": stopCapture(); break;
      case "accuracy": {
        const v = await call("dictation_set_accuracy", { accuracy: b.dataset.v });
        if (v) view = v;
        saved = true; paint();
        clearTimeout(savedTimer);
        savedTimer = setTimeout(() => { saved = false; paint(); }, 1600);
        break;
      }
    }
  });
  sheet.addEventListener("keydown", event => {
    if (event.key !== "Tab") return;
    const controls = [...sheet.querySelectorAll("button, [contenteditable]")].filter(n => !n.disabled && n.getClientRects().length);
    if (!controls.length) return;
    if (event.shiftKey && document.activeElement === controls[0]) { event.preventDefault(); controls[controls.length - 1].focus(); }
    else if (!event.shiftKey && document.activeElement === controls[controls.length - 1]) { event.preventDefault(); controls[0].focus(); }
  });
  // The keyboard, before anything else hears it: key capture first, then the off notice.
  const isDictKey = e => !!view && (e.key === keyName() || (view.key === 1 && (e.key === "F13" || e.code === "F13")));
  document.addEventListener("keydown", e => {
    if (capture && !sheet.hidden) {
      e.preventDefault(); e.stopPropagation();
      if (e.key === "Escape") { stopCapture(); return; }
      const f = /^F([1-9]|1[0-9])$/.exec(e.key);
      if (f) { pickKey(Number(f[1])); return; }
      if (e.repeat) return;
      capture.err = MODIFIERS.includes(e.key) ? MODIFIER_REFUSED : TYPING_REFUSED;
      paint();
      return;
    }
    if (view && view.ready && !view.on && isDictKey(e) && !e.repeat) {
      e.preventDefault();
      if (window.RichSettings && window.RichSettings.toast) window.RichSettings.toast(OFF_NOTICE);
    }
  }, true);

  function open() {
    if (window.RichSettings && window.RichSettings.close) window.RichSettings.close();
    build();
    sheet.hidden = false;
    col.dataset.painted = "";
    paint();
    if (!breathing) { breathing = true; requestAnimationFrame(breathe); }
    $("dict-close").focus();
    read().then(() => {
      // On but never asked (a flow that ended with RichOS): it is on, so asking goes on.
      if (view && view.on && !flow && (view.mic === "unknown" || (view.mic === "allowed" && view.ax === "unknown"))) proceed();
    });
  }
  function close() {
    if (capture) stopCapture();
    sheet.hidden = true;
    feedback = null;
    if (!axWait && !micWait) { settle(); announceSetUp(); }
    queueMicrotask(() => { const btn = document.getElementById("set-btn"); if (btn) btn.focus(); });
  }

  window.RichDictation = {
    open,
    /** Rich's offer (main.js): open the sheet and turn dictation on, in the drawn order.
     *  Answers `{on}` once the flow has settled: `true` when dictation is on and both
     *  permissions are allowed. */
    turnOn() {
      open();
      return new Promise(resolve => { waiters.push(resolve); turnOn(); });
    },
  };

  // The row and the off notice need the state once at start; while the row
  // or the sheet shows, it is read again every 2 s (another app's hidden keys come and go).
  read();
  setInterval(() => {
    if (document.hidden || !view || !view.ready) return;
    const row = document.getElementById("set-dictation-open");
    const visible = !sheet.hidden || (row && row.getClientRects().length);
    if (visible && Date.now() - lastRead >= 2000) read();
  }, 1000);
})();

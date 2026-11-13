"use strict";
// THE FIVE TOOLS THAT COME WITH RICH, on a first install (round 70, richos-hq
// design/mockups/rounds/round-70; the CEO, 2026-11-13: "Update the design of the onboarding flow
// for when a user installs RichOS for the first time. Because when the current design was created
// we didn't have RichScribe, RichBrief etc.").
//
// Three parts, each drawn by the surface that owns its moment (`main.js`):
//   - THE STRIP on the setup sheet: "Five tools come with me, free." and the five icons. On the
//     first sheet, pointing at one (or Tab to it) says what it does in the line under the strip,
//     RichScribe chosen when it opens. While things install, the same five sit gray and Waiting,
//     and each turns Ready when the piece it runs on is in.
//   - THE TOOLS CARD under Rich's note while he reads up (and after Not now to onboarding):
//     RichScribe leads with Turn on RichScribe, which asks the key, the microphone and
//     Accessibility in the card itself (`window.RichDictation.card`, dictation.js); the four that
//     are already working, one line each, and pointing at one shows where it lives.
//   - THE LINE ABOVE THE MESSAGE BOX when Rich comes back with his questions and RichScribe is on:
//     "Or just say it: press F5 and talk." and Talk now.
//
// Every word is the round's (build/mockup.js, the CEO's wording of 2026-11-13). RichFiles is in the
// five and RichPass is not (the CEO: "RichPass is optional, but RichFiles is coming online now").
(function () {
  const TOOLS = {
    scribe: {
      name: "RichScribe",
      icon: "assets/app-icons/richscribe-128.png",
      rail: "nr-scribe",
      line: "Talk instead of typing, anywhere on this Mac. A free & private/local replacement for Wispr Flow. It saves you $140+/year👍",
      what: "Talk instead of typing, in any app on this Mac. A free & private/local replacement for Wispr Flow (saves you $140+/year).",
    },
    brief: {
      name: "RichBrief",
      icon: "assets/app-icons/richbrief-128.png",
      rail: null,
      line: "Automatic meeting notes whenever you want. A free & private Granola Business replacement that saves you $160+/year👍",
      what: "When a call starts, I ask. Say yes and the notes are here when it ends.",
      where: "Comes to you when a call starts",
    },
    finder: {
      name: "RichFinder",
      icon: "assets/app-icons/richfinder-128.png",
      rail: "nr-finder",
      kbd: "⌘K",
      line: "Instantly find everything. Even if you don't remember anything. Just describe what you want and RichFinder will find it!",
      what: "One search for your chats, files and folders.",
      where: "RichFinder lives here · ⌘K",
    },
    cleaner: {
      name: "RichCleaner",
      icon: "assets/app-icons/richcleaner-128.png",
      rail: "nr-cleaner",
      line: "Never organize another screenshot or file again. Just click it, hit your RichScribe key, say a few words and you're done!",
      what: "Clears your Desktop. Say why a file matters and I file it.",
      where: "RichCleaner lives here",
    },
    files: {
      name: "RichFiles",
      icon: "assets/app-icons/richfiles-128.png",
      rail: "nr-richfiles",
      line: "The best free & open-source file manager for Mac. You'll never want to use Apple's Finder for the rest of your LIFE!",
      what: "A file browser for this Mac: folder tree, full path, full file names.",
      where: "RichFiles lives here",
    },
  };
  const ORDER = ["scribe", "brief", "finder", "cleaner", "files"];
  const WORKING = ["brief", "finder", "cleaner", "files"];
  /// What each turns Ready on while things install: the three that run on the engine when it is in,
  /// RichScribe and RichBrief when the voice & video tools are in too.
  const NEEDS_VOICE = new Set(["scribe", "brief"]);

  const STRIP_HEAD = "Five tools come with me, free.";
  const WAITING = "Waiting";
  const READY = "Ready";
  /// Rich's note, after he says he is reading up, and after Not now to onboarding (the CEO's words).
  const NOTE_READING = "One more thing: five tools came with me. Three are already working, but RichScribe (which also powers RichCleaner) wants your OK.";
  const NOTE_DECLINED = "One more thing, then I'm out of your way: five tools came with me. Three are already working, but RichScribe (which also powers RichCleaner) wants your OK.";
  const TURN_ON = "Turn on RichScribe";
  const NOT_NOW = "Not now";
  const ON = "On";
  const KEY_QUESTION = "Which key starts it?";
  const KEY_HINT = "Tap once to start, once more to stop. Nothing to hold down. Any function key works.";
  const MIC_NAME = "Microphone.";
  const MIC_WHY = "So I can hear you, only while you dictate.";
  const AX_NAME = "Accessibility.";
  const AX_WHY = "So I can type your words where your cursor is, in any app.";
  const ALLOW = "Allow";
  const ALLOWED = "Allowed";
  const ASKING = "macOS is asking you";
  const DENIED = "Not allowed";
  const OPEN_SETTINGS = "Open System Settings";
  const STARTING = "Starting RichScribe…";
  const READY_BEFORE = "RichScribe is on. Click into any app, press ";
  const READY_AFTER = " and talk. Your voice is transcribed on this Mac.";
  const TRY = "Try it here";
  const REST_HEAD = "Already working";
  const REST_FOOT = "RichScribe, RichFinder, RichFiles and RichCleaner stay in the bar on the far left. RichBrief comes to you when a call starts.";
  const BRIEF_HEAD = "RichBrief · what you will see";
  const BRIEF_TITLE = "Call detected in Zoom.";
  const BRIEF_TAKE = "Take Notes";
  const BRIEF_ASIDE = "Not this one";
  const HINT_BEFORE = "Or just say it: press ";
  const HINT_AFTER = " and talk.";
  const TALK_NOW = "Talk now";
  /// Said when the bar could not be started from a button: the key still works.
  const PRESS_THE_KEY = (key) => "Press " + key + " and talk.";

  function el(tag, cls, text) {
    const n = document.createElement(tag);
    if (cls) n.className = cls;
    if (text != null) n.textContent = text;
    return n;
  }
  function icon(id, cls) {
    const img = el("img", cls);
    img.src = TOOLS[id].icon;
    img.alt = "";
    return img;
  }
  function kcap(text) {
    return el("kbd", "suite-kcap", text);
  }

  // ---- the strip ---------------------------------------------------------------------------
  /// The strip for the first sheet (`arrive` false: five buttons and the line under them) or for
  /// the sheet while things install (`arrive` true: five icons, each Waiting until it is in).
  function strip(arrive) {
    const box = el("div", "suite-strip" + (arrive ? " suite-strip--arrive" : ""));
    box.appendChild(el("p", "suite-strip-head", STRIP_HEAD));
    const icons = el("div", "suite-icons");
    if (arrive) icons.setAttribute("role", "list");
    const line = arrive ? null : el("p", "suite-line");
    if (line) line.setAttribute("aria-live", "polite");
    function pick(id) {
      icons.querySelectorAll(".suite-ico").forEach((b) => {
        const on = b.dataset.tool === id;
        b.classList.toggle("is-on", on);
        b.setAttribute("aria-pressed", String(on));
      });
      line.replaceChildren(el("b", null, TOOLS[id].name + "."), " " + TOOLS[id].line);
    }
    for (const id of ORDER) {
      const b = el(arrive ? "div" : "button", "suite-ico");
      b.dataset.tool = id;
      if (arrive) b.setAttribute("role", "listitem");
      else b.type = "button";
      b.appendChild(icon(id));
      if (arrive) {
        const tick = el("span", "suite-ico-tick", "✓");
        tick.setAttribute("aria-hidden", "true");
        b.appendChild(tick);
      }
      b.appendChild(el("span", "suite-ico-name", TOOLS[id].name));
      if (arrive) b.appendChild(el("span", "suite-ico-state", WAITING));
      else for (const ev of ["mouseenter", "focus", "click"]) b.addEventListener(ev, () => pick(id));
      icons.appendChild(b);
    }
    box.appendChild(icons);
    if (line) {
      box.appendChild(line);
      pick("scribe");
    }
    return box;
  }

  /// While things install: which have arrived. `engine` is true once every piece but the voice &
  /// video tools is in; `voice` once they are in too. Drawn from the rows each time, both ways:
  /// the sheet measures its finished height by marking every row done for one instant
  /// (`lockSetupHeight`), and the strip follows the rows back.
  function arrive(box, engine, voice) {
    if (!box) return;
    box.querySelectorAll(".suite-ico").forEach((b) => {
      const ready = NEEDS_VOICE.has(b.dataset.tool) ? voice && engine : engine;
      if (ready === b.classList.contains("is-in")) return;
      b.classList.toggle("is-in", ready);
      b.querySelector(".suite-ico-state").textContent = ready ? READY : WAITING;
    });
  }

  // ---- where a tool lives ------------------------------------------------------------------
  /// Its item in the navigation rail lights and is named; RichBrief, which has no item, shows the
  /// prompt it brings when a call starts (meeting-panel.js draws the real one).
  function whereOff() {
    document.querySelectorAll(".suite-where, .suite-brief").forEach((n) => n.remove());
    document.querySelectorAll("#nav-rail .suite-here").forEach((n) => n.classList.remove("suite-here"));
    document.querySelectorAll(".suite-tile.is-shown").forEach((n) => n.classList.remove("is-shown"));
  }
  function whereOn(id, tile) {
    whereOff();
    const t = TOOLS[id];
    tile.classList.add("is-shown");
    if (!t.rail) {
      const p = el("div", "suite-brief");
      p.setAttribute("aria-hidden", "true");
      const head = el("div", "suite-brief-head");
      const img = icon("brief");
      head.append(img, el("span", null, BRIEF_HEAD));
      const acts = el("div", "suite-brief-acts");
      acts.append(el("span", null, BRIEF_TAKE), el("span", null, BRIEF_ASIDE));
      p.append(head, el("p", "suite-brief-title", BRIEF_TITLE), acts, el("div", "suite-brief-life"));
      document.body.appendChild(p);
      return;
    }
    const item = document.getElementById(t.rail);
    if (!item || item.hidden || !item.getClientRects().length) return;
    item.classList.add("suite-here");
    const r = item.getBoundingClientRect();
    const w = el("div", "suite-where", t.where);
    w.setAttribute("aria-hidden", "true");
    w.style.top = r.top + r.height / 2 + "px";
    w.style.left = r.right + 8 + "px";
    document.body.appendChild(w);
  }

  // ---- the tools card ----------------------------------------------------------------------
  const dictation = () => (window.RichDictation && window.RichDictation.card) || null;

  /// Puts the cursor in the message box and starts RichScribe's bar there, so what is said lands
  /// in the box. When the bar cannot be started from here, the key still works, and he is told so.
  async function talkHere() {
    const input = document.getElementById("input");
    if (input) {
      input.focus();
      const end = input.value.length;
      try { input.setSelectionRange(end, end); } catch (_) { /* not a text field */ }
    }
    const d = dictation();
    const s = d && d.status();
    const started = d ? await d.listen() : false;
    if (started !== true && window.RichSettings && window.RichSettings.toast) {
      window.RichSettings.toast(PRESS_THE_KEY(s ? "F" + s.key : "your RichScribe key"));
    }
  }

  /// The card: built once per note and kept, so what he has opened stays open while the
  /// conversation around it is drawn again. It paints itself from RichScribe's own state.
  function card() {
    const c = el("div", "suite-card");
    let open = false;

    const lead = el("div", "suite-lead");
    const leadText = el("div");
    leadText.append(el("p", "suite-lead-name", TOOLS.scribe.name), el("p", "suite-lead-what", TOOLS.scribe.what));
    const acts = el("div", "suite-lead-acts");
    lead.append(icon("scribe"), leadText, acts);

    const turn = el("div", "suite-turn");
    const turnClip = el("div");
    const turnIn = el("div", "suite-turn-in");
    const q = el("p", "suite-turn-q", KEY_QUESTION);
    q.id = "suite-key-q";
    const keys = el("div", "suite-keys");
    keys.setAttribute("role", "radiogroup");
    keys.setAttribute("aria-labelledby", "suite-key-q");
    for (let n = 1; n <= 12; n++) {
      const b = el("button", "suite-key", "F" + n);
      b.type = "button";
      b.id = "suite-key-" + n;
      b.dataset.k = String(n);
      b.setAttribute("role", "radio");
      b.addEventListener("click", () => { const d = dictation(); if (d) d.setKey(n); });
      keys.appendChild(b);
    }
    const perms = ["microphone", "accessibility"].map((pane) => {
      const row = el("div", "suite-perm");
      row.dataset.perm = pane;
      const mark = el("span", "suite-perm-mark");
      mark.setAttribute("aria-hidden", "true");
      const words = el("span");
      words.append(el("span", "suite-perm-name", pane === "microphone" ? MIC_NAME : AX_NAME), " ",
        el("span", "suite-perm-why", pane === "microphone" ? MIC_WHY : AX_WHY));
      const state = el("span", "suite-perm-state");
      row.append(mark, words, state);
      return row;
    });
    const starting = el("p", "suite-turn-hint suite-starting", STARTING);
    turnIn.append(q, keys, el("p", "suite-turn-hint", KEY_HINT), ...perms, starting);
    turnClip.appendChild(turnIn);
    turn.appendChild(turnClip);

    const ready = el("div", "suite-ready");
    const readyLine = el("p");
    const readyKey = kcap("F1");
    readyLine.append(READY_BEFORE, readyKey, READY_AFTER);
    const tryBtn = el("button", "desk-btn suite-try", TRY);
    tryBtn.type = "button";
    tryBtn.id = "suite-try";
    tryBtn.addEventListener("click", talkHere);
    ready.append(readyLine, tryBtn);

    const rest = el("div", "suite-rest");
    const tiles = el("div", "suite-tiles");
    for (const id of WORKING) {
      const t = TOOLS[id];
      const b = el("button", "suite-tile");
      b.type = "button";
      b.dataset.tool = id;
      const name = el("span", "suite-tile-name", t.name);
      if (t.kbd) name.append(" ", kcap(t.kbd));
      const text = el("span");
      text.append(name, el("span", "suite-tile-what", t.what));
      b.append(icon(id), text);
      b.setAttribute("aria-label", t.name + ". " + t.what + " " + t.where);
      for (const ev of ["mouseenter", "focus", "click"]) b.addEventListener(ev, () => whereOn(id, b));
      for (const ev of ["mouseleave", "blur"]) b.addEventListener(ev, whereOff);
      tiles.appendChild(b);
    }
    rest.append(el("p", "suite-rest-head", REST_HEAD), tiles, el("p", "suite-rest-foot", REST_FOOT));

    c.append(lead, turn, ready, rest);
    // It arrives once. The conversation puts the same card back each time it is drawn, and a
    // CSS animation starts again on every insertion, so it is settled after its first run.
    c.addEventListener("animationend", (e) => { if (e.target === c) c.classList.add("is-settled"); });

    function permState(row, value) {
      const pane = row.dataset.perm;
      const mark = row.querySelector(".suite-perm-mark");
      const state = row.querySelector(".suite-perm-state");
      row.classList.toggle("is-ok", value === "allowed");
      mark.textContent = value === "allowed" ? "✓" : "";
      const drawn = state.dataset.drawn;
      if (drawn === value) return;
      state.dataset.drawn = value;
      state.replaceChildren();
      if (value === "allowed") state.appendChild(el("span", "suite-perm-ok", ALLOWED));
      else if (value === "asking") state.appendChild(el("span", "suite-perm-wait", ASKING));
      else if (value === "denied") {
        state.appendChild(el("span", "suite-perm-no", DENIED));
        const settings = el("button", "desk-btn", OPEN_SETTINGS);
        settings.type = "button";
        settings.id = "suite-open-" + pane;
        settings.addEventListener("click", () => { const d = dictation(); if (d) d.openSettings(pane); });
        state.appendChild(settings);
      } else {
        const allow = el("button", "desk-btn", ALLOW);
        allow.type = "button";
        allow.id = "suite-allow-" + pane;
        allow.addEventListener("click", () => { const d = dictation(); if (d) d.allow(pane); });
        state.appendChild(allow);
      }
    }

    function paint() {
      const d = dictation();
      const s = d ? d.status() : null;
      const works = !!(s && s.works);
      const phase = works ? "on" : open ? "choosing" : "off";
      if (c.dataset.phase !== phase) {
        c.dataset.phase = phase;
        acts.replaceChildren();
        if (phase === "on") {
          const badge = el("span", "suite-on");
          const tick = el("i", null, "✓");
          tick.setAttribute("aria-hidden", "true");
          badge.append(tick, ON);
          acts.appendChild(badge);
        } else if (phase === "choosing") {
          const no = el("button", "desk-btn", NOT_NOW);
          no.type = "button";
          no.id = "suite-not-now";
          no.addEventListener("click", () => { open = false; paint(); document.getElementById("suite-turn-on")?.focus(); });
          acts.appendChild(no);
        } else {
          const on = el("button", "desk-btn desk-btn--confirm", TURN_ON);
          on.type = "button";
          on.id = "suite-turn-on";
          on.addEventListener("click", () => {
            open = true;
            paint();
            const k = keys.querySelector('[aria-checked="true"]') || keys.firstChild;
            if (k) k.focus({ preventScroll: true });
          });
          acts.appendChild(on);
        }
      }
      turn.classList.toggle("is-open", phase === "choosing");
      turnClip.inert = phase !== "choosing";
      const key = s ? s.key : 1;
      keys.querySelectorAll(".suite-key").forEach((b) => b.setAttribute("aria-checked", String(Number(b.dataset.k) === key)));
      permState(perms[0], s ? s.mic : "unknown");
      permState(perms[1], s ? s.ax : "unknown");
      starting.hidden = !(s && s.mic === "allowed" && s.ax === "allowed" && !works);
      readyKey.textContent = "F" + key;
    }
    window.addEventListener("richos:dictation-state", paint);
    paint();
    c.paint = paint;
    return c;
  }

  /// Rich's note and its card, as one of his messages. `avatar` when it is his first on screen.
  function note(declined, avatar) {
    const art = el("article", "tl-rich suite-note");
    art.appendChild(el("span", "sr-only", "Rich said"));
    const meta = el("div", "tl-rich-meta");
    if (avatar) {
      const img = el("img", "tl-avatar");
      img.src = "assets/rich-hand.png";
      img.alt = "";
      meta.appendChild(img);
    }
    meta.appendChild(el("span", "tl-who", "Rich"));
    art.append(meta, el("p", "tl-prose", declined ? NOTE_DECLINED : NOTE_READING), card());
    return art;
  }

  // ---- the line above the message box ------------------------------------------------------
  /// Paints `#voice-hint`: shown only when `show` and RichScribe works.
  function paintHint(box, show) {
    if (!box) return;
    const d = dictation();
    const s = d ? d.status() : null;
    const on = !!(show && s && s.works);
    box.hidden = !on;
    if (!on) return;
    const key = "F" + s.key;
    if (box.dataset.key === key) return;
    box.dataset.key = key;
    const talk = el("button", "suite-hint-talk", TALK_NOW);
    talk.type = "button";
    talk.id = "voice-hint-talk";
    talk.addEventListener("click", talkHere);
    box.replaceChildren(HINT_BEFORE, kcap(key), HINT_AFTER, " ", talk);
  }

  window.RichSuiteTools = { TOOLS, ORDER, strip, arrive, note, paintHint, whereOff, talkHere };
})();

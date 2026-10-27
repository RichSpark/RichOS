// BUST A BUG — the conversation, the report card and the Rich panel (round 21).
//
// The CEO's §115 (2026-10-09), the acceptance criterion: "When the user has Claude set up in the
// app (which is the expected default), we should just let the user say what's wrong and where and
// let their Rich check and articulate everything properly and then submit a GitHub issue on their
// behalf." Built exactly as richos-hq design/mockups/rounds/round-21/ shows it (approved "go!"):
//
//   ask -> checking -> draft <-> editing -> sending -> sent
//            |           \-> canceled          \-> waiting (offline / GitHub down) -> sent
//            \-> unchecked (Claude didn't answer: kept on this Mac, nothing to send) -> draft
//   sending -> draft again (changed words went through the scanner and then Rich, and his
//              version differs from the card)
//   sending -> unchecked (changed words Claude didn't answer about: kept, nothing sent) -> draft
//
// No report is offered for sending without Rich's check (review rv-20261009T162841Z-69294215-70e6
// finding 2): when Claude fails, times out or answers with no report, the shell keeps the user's
// words on this Mac and answers `{state: "unchecked"}`; Rich says he couldn't check it yet, and the
// shell asks him again by itself. When he has, `rich://bug-report` brings `{ checked }` and the
// card comes then, unsent. A report kept before a quit comes in the Rich panel.
//
// What is SENT is decided by the shell alone, at Send (`bug_report_send`, `richos_core::
// bug_report::at_send`), in the CEO's order (§115): the scanner first, Rich last, and nothing
// after him changes the words. The card's words go as they are when they are the words Rich last
// gave of this report (`f.report`). Words that changed (by hand, or a change told to Rich) go
// through the scanner and then Rich first ("Rich is checking your changes…"); when his version
// differs from the card, the card comes back with his version, to send or not; when Claude can't
// answer they wait on this Mac, unsent, like any report he couldn't check. This file never decides
// that a report may go.
//
// ONE PLACE (CEO, 2026-10-10: "pressing Bust a bug always shows the bug report on top, ready to
// answer"). The exchange always happens in the Rich panel (`#bugdock`), never in the conversation.
// The panel is the app's top layer (`style.css`), so whatever screen or window the bug is on stays
// up behind it, untouched. A press opens the panel and puts the keys in its answer box; from then
// until the report is sent, canceled or its panel closed, Bust a bug is disabled in the Settings
// menu (`reportOpen`), so there is no second press. Anything in the panel taking focus turns
// Dictation's key capture and conversation voice mode off through their own off paths.
//
// WHAT IS NOT IN THIS FILE: what is private and its stand-ins, Rich's write-up, the issue's words,
// the send and the keeping of what could not go. All of that is `richos_core::bug_report`, reached
// through the `bug_report_*` commands, so the rule that decides what reaches GitHub, and what the
// card's heads-up calls private, is in Rust and tested there. This file draws what those commands
// answer. Everything it puts on screen is
// built with textContent; nothing Rich or the user wrote is ever parsed as markup.
//
// REQUEST A FEATURE (CEO 2026-10-11, round 24: "Under the "Bust a bug" button, I want to add a
// "Request a feature" button. But I don't want feature requests to be submitted or branded as
// "issues" on GitHub." and "can things not be copy-pasted from the "Bust a bug" feature?"). The
// same exchange, the same panel, the same card and the same states, carrying an idea: a flow's
// `kind` is "bug" or "idea", every word that differs is in WORDS, and every command is told the
// kind (`post`) so the shell writes it up as an idea and posts it in RichOS's GitHub Discussions,
// under Ideas. Nothing a feature request puts on screen says "issue" or "bug". One at a time
// across both buttons: while either is open, both are disabled in Settings.
"use strict";

window.RichBug = (function () {
  var host = null; // main.js's hooks (init)
  var bridge = null;
  var context = null; // bug_report_context: the account a report goes out as
  var flows = [];
  var seq = 0;
  var dockVoice = false;

  var GLYPH_BUG = [
    "m8 2 1.88 1.88", "M14.12 3.88 16 2", "M9 7.13v-1a3.003 3.003 0 1 1 6 0v1",
    "M12 20c-3.3 0-6-2.7-6-6v-3a4 4 0 0 1 4-4h4a4 4 0 0 1 4 4v3c0 3.3-2.7 6-6 6", "M12 20v-9",
    "M6.53 9C4.6 8.8 3 7.1 3 5", "M6 13H2", "M3 21c0-2.1 1.7-3.8 3.8-4", "M20.97 5c0 2.1-1.6 3.8-3.5 4",
    "M22 13h-4", "M17.2 17c2.1.2 3.8 1.9 3.8 4",
  ];
  var GLYPH_EYEOFF = [
    "M10.733 5.076a10.744 10.744 0 0 1 11.205 6.575 1 1 0 0 1 0 .696 10.747 10.747 0 0 1-1.444 2.49",
    "M14.084 14.158a3 3 0 0 1-4.242-4.242",
    "M17.479 17.499a10.75 10.75 0 0 1-15.417-5.151 1 1 0 0 1 0-.696 10.75 10.75 0 0 1 4.446-5.143",
    "m2 2 20 20",
  ];
  var GLYPH_MIC = ["M19 10v1a7 7 0 0 1-14 0v-1", "M12 18v4"];
  var GLYPH_BULB = ["M15 14c.2-1 .7-1.7 1.5-2.5 1-.9 1.5-2.2 1.5-3.5A6 6 0 0 0 6 8c0 1 .2 2.2 1.5 3.5.7.7 1.3 1.5 1.5 2.5", "M9 18h6", "M10 22h4"];

  /// **EVERY WORD THAT DIFFERS BETWEEN A BUG REPORT AND A FEATURE IDEA** (round 24's `KIND`). A
  /// bug report's are the words this file has always said; an idea's never say "issue" or "bug".
  var WORDS = {
    bug: {
      name: "Bust a bug", card: "Bug report", glyph: GLYPH_BUG, close: "Close the bug report panel",
      ask: "What went wrong? Tell me in your own words, typed or out loud and where it happened if it wasn't here. ",
      phAsk: "Tell Rich what went wrong…", phDraft: "Tell Rich what to change or press Send…",
      none: "No problem. Nothing was written up or sent.",
      hereItIs: "Here's the report as I'd file it. Nothing goes out until you press Send. Anyone can read GitHub issues, so I left out names, company details and file paths.",
      notChecked: "I couldn't check this report for private details yet, because Claude didn't answer, so it isn't ready to send. It's saved on this Mac and I'll check it by myself as soon as Claude answers, then show it to you here.",
      changesNotChecked: "I couldn't check your changes for private details yet, because Claude didn't answer, so I didn't send the report. It's saved on this Mac and I'll check it by myself as soon as Claude answers, then show it to you here.",
      shownAgain: "I checked it again before sending and some of its words would go out differently from how the card showed them. Here it is exactly as it would go. Nothing goes out until you press Send.",
      earlier: "I've checked the bug report you told me about earlier. ",
      drop: "Cancel report", go: "Send report",
      droppedUnchecked: "Canceled. Nothing was sent and it's no longer saved on this Mac.",
      notDropped: "I couldn't take the report off this Mac, so it's still saved here. Press Cancel report to try again.",
      changing: "Rich is changing the report…",
      changedQueued: " The report changed, so it waits for you to send it again.", changedDraft: " It still waits for you to send it.",
      notChanged: "I couldn't change the report just now. Press Change it and change the words yourself or tell me again.",
      notCanceled: "I couldn't cancel the report: it's still saved on this Mac and will go out by itself. Press Cancel report to try again.",
      notTakenBack: "I couldn't take the report back to change it: it's still saved on this Mac and will go out by itself. Press Change it to try again.",
      readers: "GitHub issues", thing: "report", notInIt: "it isn't in the report.",
      pill: { draft: "Not sent yet", changed: "Not sent yet · changed", sending: "Sending…", queued: "Waiting to send · saved on this Mac", canceled: "Canceled · nothing sent" },
      notSaved: "I couldn't save the report on this Mac, so nothing was sent. Press Send report to try again.",
      canceled: "Canceled. Nothing was sent.",
      savedToast: "Your bug report is saved on this Mac and goes out by itself when it can.",
      uncheckedToast: "Your bug report is saved on this Mac. Rich will check it as soon as Claude answers and show it to you here.",
    },
    idea: {
      name: "Request a feature", card: "Feature idea", glyph: GLYPH_BULB, close: "Close the feature request panel",
      ask: "What would you like RichOS to do? Tell me in your own words, typed or out loud. ",
      phAsk: "Tell Rich your idea…", phDraft: "Tell Rich what to change or post it…",
      none: "No problem. Nothing was written up or posted.",
      hereItIs: "Here's your idea as I'd post it. Nothing goes out until you press Post idea. Anyone can read GitHub Discussions, so I left out names, company details and file paths.",
      notChecked: "I couldn't check your idea for private details yet, because Claude didn't answer, so it isn't ready to post. It's saved on this Mac and I'll check it by myself as soon as Claude answers, then show it to you here.",
      changesNotChecked: "I couldn't check your changes for private details yet, because Claude didn't answer, so I didn't post your idea. It's saved on this Mac and I'll check it by myself as soon as Claude answers, then show it to you here.",
      shownAgain: "I checked it again before posting and some of its words would go out differently from how the card showed them. Here it is exactly as it would go. Nothing goes out until you press Post idea.",
      earlier: "I've checked the idea you told me about earlier. ",
      drop: "Cancel post", go: "Post idea",
      droppedUnchecked: "Canceled. Nothing was posted and it's no longer saved on this Mac.",
      notDropped: "I couldn't take your idea off this Mac, so it's still saved here. Press Cancel post to try again.",
      changing: "Rich is changing the post…",
      changedQueued: " The post changed, so it waits for you to post it again.", changedDraft: " It still waits for you to post it.",
      notChanged: "I couldn't change the post just now. Press Change it and change the words yourself or tell me again.",
      notCanceled: "I couldn't cancel the post: it's still saved on this Mac and will be posted by itself. Press Cancel post to try again.",
      notTakenBack: "I couldn't take the post back to change it: it's still saved on this Mac and will be posted by itself. Press Change it to try again.",
      readers: "GitHub Discussions", thing: "post", notInIt: "it isn't in the post.",
      pill: { draft: "Not posted yet", changed: "Not posted yet · changed", sending: "Posting…", queued: "Waiting to post · saved on this Mac", canceled: "Canceled · nothing posted" },
      notSaved: "I couldn't save your idea on this Mac, so nothing was posted. Press Post idea to try again.",
      canceled: "Canceled. Nothing was posted.",
      savedToast: "Your idea is saved on this Mac and is posted by itself when it can.",
      uncheckedToast: "Your idea is saved on this Mac. Rich will check it as soon as Claude answers and show it to you here.",
    },
  };
  function W(f) { return WORDS[(f && f.kind) || "bug"]; }
  /// The section of an idea whose points are bullets, not numbered steps (the core's
  /// `bug_report::BULLETED`, round 24's "What the user pictured").
  var BULLETED = "What the user pictured";
  /// What every command is told about the flow: what kind of post it is.
  function postOf(f) { return (f && f.kind) || "bug"; }

  function svg(paths, cls, label) {
    var ns = "http://www.w3.org/2000/svg";
    var s = document.createElementNS(ns, "svg");
    s.setAttribute("viewBox", "0 0 24 24");
    s.setAttribute("fill", "none");
    s.setAttribute("stroke", "currentColor");
    s.setAttribute("stroke-width", "2");
    s.setAttribute("stroke-linecap", "round");
    s.setAttribute("stroke-linejoin", "round");
    if (cls) s.setAttribute("class", cls);
    if (label) {
      s.setAttribute("role", "img");
      s.setAttribute("aria-label", label);
    } else s.setAttribute("aria-hidden", "true");
    if (paths === GLYPH_MIC) {
      var r = document.createElementNS(ns, "rect");
      r.setAttribute("x", "9"); r.setAttribute("y", "2"); r.setAttribute("width", "6"); r.setAttribute("height", "12"); r.setAttribute("rx", "3");
      s.appendChild(r);
    }
    for (var i = 0; i < paths.length; i++) {
      var p = document.createElementNS(ns, "path");
      p.setAttribute("d", paths[i]);
      s.appendChild(p);
    }
    return s;
  }

  function node(tag, cls, text) {
    var n = document.createElement(tag);
    if (cls) n.className = cls;
    if (text != null) n.textContent = text;
    return n;
  }
  function $(id) { return document.getElementById(id); }

  // ---- what is private: ASKED, never decided here ----------------------------------------------
  // The card's heads-up after a change ("Acme looks private") is the answer of the same Rust
  // scrubber that cleans the report (`bug_report_private_in`, `richos_core::bug_report::
  // private_in_edit`), with the same private words. This file keeps no copy of the rules: its own
  // copy fell behind the core three times (paths, file addresses and drives, then an address like
  // alice@büro.de; fourth review finding 2). `f.private` is what Rich found private in this
  // report (the draft's and every change's), so a name he left out once is named when the user
  // types it back in. It never decides what is sent: the user may send anyway, having been told.
  function askPrivate(text, f) {
    return bridge.invoke("bug_report_private_in", { text: text, private: (f && f.private) || [] }).then(function (found) {
      return Array.isArray(found) ? found : [];
    });
  }

  // ---- where the user was -------------------------------------------------------------------
  // [element, key, what the user calls it (the panel's header and Rich's sentence), the same
  // place for the public report]. The three windows round 21 names first, then the other sheets
  // that cover the conversation. This names where the user was; it decides nothing about where
  // the report goes (always the panel).
  var WINDOWS = [
    ["corrections-overlay", "corrections", "Corrections", "the Corrections window"],
    ["feedback-overlay", "feedback", "Feedback", "the Feedback window"],
    ["search-overlay", "search", "Search", "the Search window"],
    ["entity-picker", "picker", "the company picker", "the window that asks which company a new conversation belongs to"],
    ["setup-sheet", "setup", "the setup window", "the window that sets RichOS up"],
    ["memory-setup", "setup", "the memory folder window", "the window that asks where RichOS keeps what it is told"],
  ];
  function openWindow() {
    for (var i = 0; i < WINDOWS.length; i++) {
      var n = $(WINDOWS[i][0]);
      if (n && !n.hidden) return WINDOWS[i];
    }
    // ANY OTHER DIALOG ON SCREEN IS A WINDOW TOO (review rv-20261010T081721Z-66ba5386-a4d1
    // finding 2): Settings is reachable over every overlay now, the sheets its own menu opens
    // included (Claude accounts, the quota, the phone, repositories, dictation, the Home screen
    // buttons, the Technical view question), so a report started over one names it.
    var dialogs = document.querySelectorAll('[role="dialog"]');
    for (var d = 0; d < dialogs.length; d++) {
      var el = dialogs[d];
      if (el.closest("[hidden], #bugdock, .settings") || !el.getClientRects().length) continue;
      var titleId = el.getAttribute("aria-labelledby");
      var title = (titleId && $(titleId) && $(titleId).textContent.trim()) || el.getAttribute("aria-label") || "";
      return [el.id || "", "window", title ? "the " + title + " window" : "a RichOS window", title ? "the RichOS window titled " + title : "a RichOS window"];
    }
    return null;
  }
  // ---- what was on the screen: for the user's own Rich to check against ----------------------
  // Round 21: "Looked at the screen you were on". The words the user could SEE when they pressed
  // Bust a bug, top to bottom: every text that is rendered, inside the window and inside every
  // scrolling box around it (a conversation scrolled away is not on screen). The exchange itself,
  // the Settings menu, notices and screen-reader-only text are not the screen. Private names stay
  // in: this goes to the user's own Claude, never into the issue (second review finding 6).
  var NOT_THE_SCREEN = "#bugdock, #set-menu, #bug-toast, #bug-subtip, .sr-only, [aria-hidden='true'], [hidden], script, style, noscript, template";
  var SCREEN_MAX = 12000;
  function screenContent() {
    var clipOf = new Map();
    function visibleBox(el) {
      if (!el || el === document.body || el === document.documentElement) return { l: 0, t: 0, r: window.innerWidth, b: window.innerHeight };
      if (clipOf.has(el)) return clipOf.get(el);
      var box = visibleBox(el.parentElement);
      var st = getComputedStyle(el);
      if (st.overflowX !== "visible" || st.overflowY !== "visible") {
        var r = el.getBoundingClientRect();
        box = { l: Math.max(box.l, r.left), t: Math.max(box.t, r.top), r: Math.min(box.r, r.right), b: Math.min(box.b, r.bottom) };
      }
      clipOf.set(el, box);
      return box;
    }
    var lines = [], line = "", lastTop = null, total = 0;
    var walker = document.createTreeWalker(document.body, NodeFilter.SHOW_TEXT);
    for (var t = walker.nextNode(); t && total < SCREEN_MAX; t = walker.nextNode()) {
      var words = t.textContent.replace(/\s+/g, " ").trim();
      var el = t.parentElement;
      if (!words || !el || el.closest(NOT_THE_SCREEN)) continue;
      var st = getComputedStyle(el);
      if (st.visibility === "hidden" || st.display === "none" || Number(st.opacity) === 0) continue;
      var r = el.getBoundingClientRect();
      var box = visibleBox(el.parentElement);
      if (r.width === 0 || r.height === 0 || r.right <= box.l || r.left >= box.r || r.bottom <= box.t || r.top >= box.b) continue;
      if (lastTop !== null && Math.abs(r.top - lastTop) > 4) { lines.push(line); line = ""; }
      line = line ? line + " " + words : words;
      lastTop = r.top;
      total += words.length + 1;
    }
    if (line) lines.push(line);
    return lines.join("\n").slice(0, SCREEN_MAX);
  }

  // ---- the two screens that cover the whole window ------------------------------------------
  /// The opening screen, up and not yet going: the same reading as `home.js`'s `splashStillUp`.
  function curtainUp() {
    var s = window.RichSplash && window.RichSplash.state;
    return !!(s && s.shown && !s.reason && $("splash"));
  }

  /// Where the user was when they pressed Bust a bug: what Rich says and the report's Where.
  function screenNow() {
    var scale = window.RichTheme && window.RichTheme.scale ? window.RichTheme.scale() : 100;
    var theme = document.documentElement.dataset.theme || "";
    var techy = !!document.querySelector("#techy-chip:not([hidden])");
    var base = { textSize: scale, theme: theme, technicalView: techy, conversation: null, content: screenContent() };
    // Pressed on the held opening screen (the settings button is on it only while it is held,
    // CEO §62): that is the screen the user was looking at, whatever is under it.
    if (curtainUp())
      return Object.assign(base, { key: "opening", label: "the opening screen", here: "the opening screen", public: "the opening screen RichOS shows as it starts" });
    var w = openWindow();
    if (w) return Object.assign(base, { key: w[1], label: w[2], here: w[2], public: w[3] });
    var view = host ? host.view() : "conversation";
    var thread = host ? host.activeThread() : null;
    var title = thread && host ? host.threadTitle(thread) : "";
    if (window.RichHome && window.RichHome.isOpen && window.RichHome.isOpen())
      return Object.assign(base, { key: "home", label: "the home screen", here: "the home screen", public: "the home screen, with a button for each company" });
    if (view === "unbound")
      return Object.assign(base, { key: "unbound", label: title || "this conversation", here: title ? "the " + title + " conversation" : "a conversation", conversation: title || null, public: "a conversation that isn't filed under a company yet, where sending is off" });
    if (view === "entity")
      return Object.assign(base, { key: "company", label: "this company's page", here: "a company's page", public: "a company's page, with its conversations" });
    if (view !== "conversation" || !thread)
      return Object.assign(base, { key: "other", label: "this screen", here: "this screen", public: "a RichOS screen" });
    return Object.assign(base, { key: "conversation", label: title, here: "the " + (title || "current") + " conversation", conversation: title || null, public: "a conversation, the main RichOS screen" });
  }

  // ---- flows --------------------------------------------------------------------------------
  // "changing": Rich is folding a change in; Send, Change it and Cancel wait for him.
  // "withdrawing": a waiting report is being taken off this Mac (to cancel it or to change it),
  // and nothing is said about it until the shell confirms what became of it.
  // "unchecked": Rich could not check the report yet; it waits on this Mac with nothing to send.
  var LIVE = ["ask", "checking", "unchecked", "draft", "editing", "changing", "sending", "queued", "withdrawing"];
  var TAKES_WORDS = ["ask", "draft", "queued", "editing", "changing"];
  function live(f) { return LIVE.indexOf(f.step) !== -1; }
  function liveFlow() { return flows.filter(live)[0] || null; }
  function dockFlow() { return flows[flows.length - 1] || null; }

  /// **A REPORT IS OPEN**: from the press (`looking`, while the window's picture is taken) until
  /// the report is sent, canceled or its panel closed. While it is, Bust a bug in the Settings
  /// menu is disabled (`settings-button.js` asks this, and `paintMenu` tells it when it changes):
  /// shown, and impossible to press. The panel itself is the way out.
  var looking = false;
  function reportOpen() {
    var dock = $("bugdock");
    return looking || !!(dock && !dock.hidden && liveFlow());
  }
  function paintMenu() {
    if (window.RichSettings && window.RichSettings.repaintBugReport) window.RichSettings.repaintBugReport();
  }

  // Every report lives in the panel: its messages box and its answer box.
  function box() { return $("bugdock-msgs"); }

  function scrollDown() {
    window.setTimeout(paintStuck, 450);
    var sc = box();
    if (!sc) return;
    window.requestAnimationFrame(function () { sc.scrollTop = sc.scrollHeight; });
  }
  /// Ready to answer: the keys go to the panel's answer box.
  function focusAnswer() {
    window.setTimeout(function () { var i = $("bugdock-input"); if (i && !$("bugdock-form").hidden) i.focus(); }, 0);
  }

  /// One of Rich's lines, drawn the way the conversation draws his messages.
  function richSays(f, lines, opts) {
    opts = opts || {};
    var wrap = node("div", "bug-turn");
    if (opts.worked) {
      var row = node("div", "tl-duration bug-worked");
      row.dataset.tone = "done";
      row.appendChild(node("span", "tl-duration-label", "Worked for " + opts.worked));
      row.appendChild(node("span", "tl-rule"));
      wrap.appendChild(row);
    }
    if (opts.digest) wrap.appendChild(node("p", "bug-digest", opts.digest));
    var art = node("article", "tl-rich bug-rich");
    art.appendChild(node("span", "sr-only", "Rich said"));
    var meta = node("div", "tl-rich-meta");
    meta.appendChild(node("span", "tl-who", "Rich"));
    art.appendChild(meta);
    lines.forEach(function (line) {
      var p = node("p", "tl-prose bug-prose");
      if (typeof line === "string") p.textContent = line;
      else p.appendChild(line);
      art.appendChild(p);
    });
    wrap.appendChild(art);
    box().appendChild(wrap);
    scrollDown();
    if (host) host.announce(lines.filter(function (l) { return typeof l === "string"; }).join(" "));
    return art;
  }

  function userSays(f, text, spoken) {
    var art = node("article", "tl-user bug-user");
    art.appendChild(node("span", "sr-only", spoken ? "You said aloud" : "You said"));
    var bubble = node("div", "tl-user-bubble");
    var t = node("div", "tl-user-text");
    if (spoken) t.appendChild(svg(GLYPH_MIC, "bug-said", "Said aloud"));
    t.appendChild(document.createTextNode(text));
    bubble.appendChild(t);
    art.appendChild(bubble);
    box().appendChild(art);
    scrollDown();
  }

  function working(f, words) {
    var w = node("p", "bug-working");
    w.setAttribute("role", "status");
    w.appendChild(node("span", "bug-working-dot"));
    w.appendChild(document.createTextNode(words));
    box().appendChild(w);
    scrollDown();
    return w;
  }

  function placeholderFor(f) {
    if (!f) return null;
    if (f.step === "ask") return W(f).phAsk;
    if (f.step === "draft" || f.step === "queued" || f.step === "editing") return W(f).phDraft;
    if (f.step === "changing") return "Tell Rich what else to change…";
    return null;
  }

  function paintComposers() {
    var f = liveFlow();
    var dockInput = $("bugdock-input");
    if (dockInput) dockInput.placeholder = placeholderFor(f) || "Talk to Rich…";
    paintDock();
    syncVoiceHold();
    paintMenu();
  }

  /// **START.** The report is in the Rich panel, on top, with the keys in its answer box; the
  /// screen behind is left as it is.
  ///
  /// **A PRESS IS ALWAYS A FIRST PRESS** (CEO, 2026-10-10: "WHY THE FUCK IS IT EVEN POSSIBLE TO
  /// CLICK THE FUCKING BUTTON AGAIN IF THE ACTION RELATED TO THAT IS ALREADY GOING ON???"). From
  /// the press until the report is sent, canceled or its panel closed, Bust a bug in the Settings
  /// menu is disabled (`reportOpen`), so nothing here handles a press over a report in progress.
  /// The one report a press can find is one saved on this Mac whose panel the user closed (the
  /// ✕ is offered only then: waiting to send, or waiting for Rich's check): the press opens the
  /// panel on it again, which is the way back to it.
  function start(seen, kind) {
    var cur = liveFlow();
    if (cur) {
      showDock(cur);
      scrollDown();
      focusAnswer();
      return cur;
    }
    var scr = seen || screenNow();
    var f = { id: ++seq, kind: kind === "idea" ? "idea" : "bug", screen: scr, step: "ask" };
    flows.push(f);
    $("bugdock-msgs").textContent = "";
    $("bugdock-where").textContent = "· " + scr.label;
    showDock(f);
    var where =
      scr.key === "conversation" ? "I've noted that you were on " + scr.here + (f.kind === "idea" ? ", in case it's about this screen." : ".")
      : scr.key === "unbound" ? "I've noted that you were on " + scr.here + ", which can't take messages yet, so we're talking here."
      : openWindow() ? "I've noted that you were in " + scr.here + " and it stays open beside us."
      : "I've noted that you were on " + scr.here + ".";
    var art = richSays(f, [W(f).ask + where]);
    var acts = node("div", "bug-actions-inline");
    var nm = node("button", "desk-btn", "Never mind");
    nm.type = "button";
    nm.id = "bug-never-mind";
    nm.addEventListener("click", function () { neverMind(f); });
    acts.appendChild(nm);
    art.appendChild(acts);
    f.neverMind = acts;
    paintComposers();
    focusAnswer();
    return f;
  }

  /// **LOOK, THEN ASK.** What the user was looking at is read BEFORE the exchange draws anything:
  /// the screen's words at once, and the shell's picture of the window (`bug_report_look`) before
  /// Rich's question appears, so neither shows the exchange instead of the screen. A picture that
  /// does not come within 1.5 s is not waited for; Rich then has the words.
  function lookThenStart(kind) {
    if (reportOpen()) return;
    if (liveFlow()) return start();
    var seen = screenNow();
    var started = false;
    // The press has happened: Bust a bug is disabled from now, while the picture is taken too.
    looking = true;
    paintMenu();
    function go() {
      if (started) return;
      started = true;
      looking = false;
      start(seen, kind);
    }
    bridge.invoke("bug_report_look").then(go, go);
    window.setTimeout(go, 1500);
  }

  function neverMind(f, quiet) {
    if (f.neverMind) { f.neverMind.remove(); f.neverMind = null; }
    f.step = "closed";
    if (!quiet) richSays(f, [W(f).none]);
    paintComposers();
  }

  /// **THE WORDS OF ONE PART OF THE CARD, WITH ITS LINE BREAKS KEPT.** A line break typed while
  /// changing it is a `<br>` or a new `<div>` (WebKit makes a `<div>` for both Enter and
  /// Shift+Enter inside a paragraph), and `textContent` drops both, so "Click" and "Send" on two
  /// lines were read, and sent, as "ClickSend". Each is read as a line break here.
  function wordsOf(el) {
    var out = "";
    (function walk(n) {
      for (var c = n.firstChild; c; c = c.nextSibling) {
        if (c.nodeType === 3) out += c.data;
        else if (c.nodeName === "BR") out += "\n";
        else if (c.nodeType === 1) {
          var block = /^(DIV|P|LI)$/.test(c.nodeName);
          if (block && out && out.slice(-1) !== "\n") out += "\n";
          walk(c);
          if (block && out && out.slice(-1) !== "\n") out += "\n";
        }
      }
    })(el);
    return out.trim();
  }

  /// **A SECTION'S OWN FIELDS, EACH ONCE**: its paragraphs and its list's steps, as the card
  /// draws them (`buildCard`), and never an element inside one of them. Whatever a field holds is
  /// read with it by `wordsOf`; reading every `li` as well read a list inside a step twice, so
  /// Send read words the card showed once (review rv-20261009T225657Z-b3b5c573-96c0).
  function fieldsIn(sec) {
    var paragraphs = [], steps = [];
    Array.prototype.forEach.call(sec.children, function (n) {
      if (n.nodeName === "P") paragraphs.push(n);
      // An idea's pictured points are a bulleted list (round 24), read as steps are.
      else if (n.nodeName === "OL" || n.nodeName === "UL") Array.prototype.forEach.call(n.children, function (li) { if (li.nodeName === "LI") steps.push(li); });
    });
    return { paragraphs: paragraphs, steps: steps };
  }
  /// Every field of the card (or of a copy of its `.bug-doc`): what can be changed by hand, what
  /// the heads-up reads and what Send reads are the same fields.
  function fieldsOf(root) {
    var all = [root.querySelector(".bug-title")];
    Array.prototype.forEach.call(root.querySelectorAll(".bug-sec"), function (s) {
      var own = fieldsIn(s);
      all = all.concat(own.paragraphs, own.steps);
    });
    return all;
  }

  function sheetOf(f) {
    var c = f.card;
    return {
      title: wordsOf(c.querySelector(".bug-title")),
      sections: Array.prototype.map.call(c.querySelectorAll(".bug-sec"), function (s) {
        var own = fieldsIn(s);
        return {
          heading: s.querySelector("h4").textContent.trim(),
          paragraphs: own.paragraphs.map(wordsOf).filter(Boolean),
          steps: own.steps.map(wordsOf).filter(Boolean),
        };
      }),
    };
  }

  function worked(ms) {
    var t = window.RichTimeline && window.RichTimeline.formatDuration ? window.RichTimeline.formatDuration(ms) : null;
    return t || "1s";
  }

  /// **THE USER'S WORDS**: the answer to Rich's question, or a change to the report.
  function say(f, text, spoken) {
    if (f.step === "ask") {
      if (f.neverMind) { f.neverMind.remove(); f.neverMind = null; }
      userSays(f, text, spoken);
      f.step = "checking";
      paintComposers();
      var w = working(f, "Rich is checking…");
      bridge.invoke("bug_report_write", { answer: text, screen: publicScreen(f.screen), post: postOf(f) }).then(function (answer) {
        w.remove();
        if (answer && answer.state === "unchecked") return keptUnchecked(f, answer.id);
        showDraft(f, answer, W(f).hereItIs, worked(answer.workedMs));
      }).catch(function (e) {
        w.remove();
        f.step = "ask";
        // The shell's words name a report; an idea's are its own.
        richSays(f, [f.kind === "idea" ? "I couldn't check your idea or save it on this Mac, so nothing was written up or posted. Tell me again and I'll try once more."
          : typeof e === "string" && e ? e : "I couldn't write that up just now. Tell me again and I'll try once more."]);
        paintComposers();
      });
      return;
    }
    // A change, said to Rich: he folds it into the report, which waits for approval again.
    if (f.step === "withdrawing") return; // the card says what is happening; the words wait for it
    userSays(f, text, spoken);
    if (f.step === "changing") {
      // He is still making the last change: this one follows it, in the order said.
      (f.nextChanges = f.nextChanges || []).push(text);
      return;
    }
    if (f.step === "editing") finishEdit(f);
    if (f.step === "queued" && f.pendingId) {
      // A changed report is approved again, so the copy waiting on this Mac must not go out
      // first: it is taken off, CONFIRMED, before Rich changes a word.
      return withdraw(f, "change", function () { askChange(f, text, true); });
    }
    askChange(f, text, false);
  }

  // What Rich says when he has written it up (`hereItIs`), could not check it (`notChecked`,
  // `changesNotChecked`), or checked changed words again and his version differs from the card in
  // any character (`shownAgain`: more left out, or a title's backtick written as an apostrophe)
  // are in WORDS, for a bug report and for an idea.
  var CHECKING_CHANGES = "Rich is checking your changes…";

  /// Rich's checked report on its card, waiting for the user: `answer` is `{report, draft,
  /// digest}`. `report` is the id the shell keeps his check under; Send names it, and the shell
  /// sends only words that are what he checked (`bug_report_send`).
  function showDraft(f, answer, lead, workedFor) {
    var old = f.card;
    f.report = answer.report || null;
    f.handEdited = false;
    f.draft = answer.draft;
    f.private = (answer.draft && answer.draft.private) || [];
    richSays(f, [lead], { worked: workedFor, digest: answer.digest });
    // A card shown again (Rich's version of changed words) replaces
    // the one before it: one report, one card.
    if (old) old.remove();
    f.card = buildCard(f);
    box().appendChild(f.card);
    f.step = "draft";
    paintCard(f);
    paintComposers();
    scrollDown();
    window.setTimeout(function () { if (f.card) f.card.classList.remove("is-new"); }, 2600);
  }

  /// **RICH COULD NOT CHECK IT YET** (review rv-20261009T162841Z-69294215-70e6 finding 2): there
  /// is no card and nothing to send. The shell keeps it on this Mac and asks him again; the way
  /// out is Cancel report, which takes it off this Mac.
  function keptUnchecked(f, id) {
    f.uncheckedId = id;
    f.step = "unchecked";
    var art = richSays(f, [W(f).notChecked]);
    var acts = node("div", "bug-actions-inline");
    acts.appendChild(button(W(f).drop, false, function () { dropUnchecked(f); }, "bug-unchecked-cancel"));
    art.appendChild(acts);
    f.uncheckedActs = acts;
    paintComposers();
  }

  /// The ids of reports Rich could not check that this window has done with (shown on a card,
  /// or canceled). The shell says a checked one on every pass until it is taken off this Mac, so
  /// a repeat only takes it off again.
  var taken = {};
  function takeOff(id, post) {
    // Not taken off: it stays kept, the shell says it again, and this takes it then.
    bridge.invoke("bug_report_take_unchecked", { id: id, post: post || "bug" }).catch(function () {});
  }

  /// **CHANGED WORDS RICH COULD NOT CHECK AT SEND**: nothing was sent. The card stays, with no
  /// Send on it, until he has checked them (`onChecked` shows it again); the way out is Cancel
  /// report, which takes them off this Mac.
  function changesKept(f, id) {
    f.uncheckedId = id;
    f.step = "unchecked";
    paintCard(f);
    var art = richSays(f, [W(f).changesNotChecked]);
    var acts = node("div", "bug-actions-inline");
    acts.appendChild(button(W(f).drop, false, function () { dropUnchecked(f); }, "bug-unchecked-cancel"));
    art.appendChild(acts);
    f.uncheckedActs = acts;
    paintComposers();
  }

  function dropUnchecked(f) {
    if (f.step !== "unchecked" || !f.uncheckedActs || f.uncheckedActs.hidden) return;
    var id = f.uncheckedId;
    taken[id] = true;
    f.uncheckedActs.hidden = true;
    bridge.invoke("bug_report_take_unchecked", { id: id, post: postOf(f) }).then(function () {
      if (f.step !== "unchecked") return;
      f.uncheckedActs.remove();
      f.uncheckedActs = null;
      f.uncheckedId = null;
      f.step = f.card ? "canceled" : "closed";
      if (f.card) paintCard(f);
      richSays(f, [W(f).droppedUnchecked]);
      paintComposers();
    }).catch(function (e) {
      delete taken[id];
      if (f.step !== "unchecked") return;
      f.uncheckedActs.hidden = false;
      richSays(f, [f.kind !== "idea" && typeof e === "string" && e ? e : W(f).notDropped]);
    });
  }

  /// **RICH HAS CHECKED IT**: `{id, draft, digest, here}` from the shell's check loop. The card
  /// comes in the Rich panel, brought forward; one kept from before a quit or a reload comes
  /// there too, unless another report is in progress (then the shell says it again later: one
  /// report at a time).
  function onChecked(c) {
    if (!c || !c.id) return;
    var kind = c.post === "idea" ? "idea" : "bug";
    if (taken[c.id]) return takeOff(c.id, kind);
    var f = flows.filter(function (x) { return x.uncheckedId === c.id; })[0];
    var earlier = !f;
    if (earlier) {
      if (liveFlow()) return;
      var label = c.here || "this screen";
      f = { id: ++seq, kind: kind, screen: { key: "other", label: label, here: label, public: "a RichOS screen" }, step: "unchecked", uncheckedId: c.id };
      flows.push(f);
      $("bugdock-msgs").textContent = "";
      $("bugdock-where").textContent = "· " + label;
    }
    if (f.step !== "unchecked") return;
    taken[c.id] = true;
    takeOff(c.id, postOf(f));
    f.uncheckedId = null;
    if (f.uncheckedActs) { f.uncheckedActs.remove(); f.uncheckedActs = null; }
    showDock(f);
    var lead = earlier ? W(f).earlier : f.card ? "I've checked your changes now. " : "I've checked it now. ";
    showDraft(f, c, lead + W(f).hereItIs, null);
  }

  /// Rich folds one change in. While he does, the report is "changing": Send, Change it and
  /// Cancel wait for him, so what is sent is what he changed, and a reply that finds the report
  /// no longer changing (it can no longer be, but the reply is checked anyway) changes nothing.
  function askChange(f, text, wasQueued) {
    f.step = "changing";
    paintCard(f);
    paintComposers();
    var w2 = working(f, W(f).changing);
    function next() {
      var more = f.nextChanges && f.nextChanges.shift();
      if (more && f.step === "draft") askChange(f, more, false);
    }
    bridge.invoke("bug_report_change", { said: text, sheet: sheetOf(f), private: f.private || [], post: postOf(f) }).then(function (answer) {
      w2.remove();
      if (f.step !== "changing") return;
      if (answer.private) f.private = answer.private;
      var secs = f.card.querySelectorAll(".bug-sec");
      var sec = null;
      for (var i = 0; i < secs.length; i++) if (secs[i].querySelector("h4").textContent === answer.section) sec = secs[i];
      if (!sec) sec = secs[0];
      var p = node("p", "is-added");
      appendSegments(p, answer.add);
      var ol = sec.querySelector("ol, ul");
      sec.insertBefore(p, ol);
      f.edited = true;
      // The card changed: at Send it goes through the scanner and then Rich, last (CEO §115).
      f.handEdited = true;
      f.step = "draft";
      paintCard(f);
      paintComposers();
      richSays(f, ["Added that to " + sec.querySelector("h4").textContent + ", above." + (wasQueued ? W(f).changedQueued : W(f).changedDraft)]);
      flash(f);
      next();
    }).catch(function () {
      w2.remove();
      if (f.step !== "changing") return;
      f.step = "draft";
      paintCard(f);
      paintComposers();
      richSays(f, [W(f).notChanged]);
      next();
    });
  }

  /// **TAKE A WAITING REPORT BACK, CONFIRMED** (second review finding 2), to cancel it or to
  /// change it. Nothing is said about it until the shell answers, which it does only after any
  /// send already in flight has finished: "canceled" when its copy is off this Mac (then `then`),
  /// the issue when it had already gone out (then the card says Sent and Rich says so), an error
  /// when the copy could not be removed (then it is still waiting, and Rich says that).
  function withdraw(f, purpose, then) {
    var id = f.pendingId;
    if (!id) return then();
    f.step = "withdrawing";
    f.withdrawFor = purpose;
    paintCard(f);
    paintComposers();
    bridge.invoke("bug_report_cancel", { id: id, post: postOf(f) }).then(function (r) {
      if (f.step !== "withdrawing") return; // it went out meanwhile, and the card already says so
      if (r && r.state === "sent") return wentOutFirst(f, r);
      f.pendingId = null;
      then();
    }).catch(function () {
      if (f.step !== "withdrawing") return;
      f.step = "queued";
      paintCard(f);
      paintComposers();
      richSays(f, [purpose === "cancel" ? W(f).notCanceled : W(f).notTakenBack]);
    });
  }

  /// A waiting report went out before it could be taken back: the card says Sent, and Rich says
  /// it went out rather than that nothing was sent.
  function wentOutFirst(f, d) {
    var purpose = f.withdrawFor;
    f.withdrawFor = null;
    f.nextChanges = [];
    markSent(f, d, null, purpose === "cancel" ? "It had already gone out before I could cancel it." : "It had already gone out, so it can't be changed.");
  }

  function publicScreen(s) {
    return { key: s.key, here: s.here, public: s.public, conversation: s.conversation, textSize: s.textSize, theme: s.theme, technicalView: s.technicalView, content: s.content || "" };
  }

  function flash(f) {
    if (!f.card) return;
    f.card.classList.remove("is-flash");
    void f.card.offsetWidth;
    f.card.classList.add("is-flash");
  }

  // ---- the report card: the send-before-approve step ------------------------------------------
  /// A stand-in is a segment with a `kind` (`Segment::is_stand_in`), whether or not the shell knows
  /// the words it replaced (`was`): one Rich wrote himself, with nothing listed beside it, is still
  /// underlined and counted (nightly 48's walk, D23: on d0d06501c it was drawn as plain words, and
  /// the line under the card said nothing private was in a report carrying four).
  function appendSegments(parent, segments) {
    (segments || []).forEach(function (s) {
      if (!s.kind) {
        parent.appendChild(document.createTextNode(s.text));
        return;
      }
      parent.appendChild(subSpan(s.text, s.kind, s.was));
    });
  }
  /// One stand-in on the card: underlined, reachable with Tab, its kind, and what it replaced
  /// only when that is known (`was`).
  function subSpan(text, kind, was) {
    var sub = node("span", "bug-sub", text);
    sub.tabIndex = 0;
    if (was != null) sub.dataset.was = was;
    sub.dataset.kind = kind;
    sub.dataset.stand = text;
    return sub;
  }

  /// The card draws the draft word for word and changes none: every draft the shell gives it is
  /// Rich's last word on what the scanner made of the report (`richos_core::bug_report::finished`),
  /// and Send posts these words, or, when they changed, shows his version of them first.
  function buildCard(f) {
    var d = f.draft;
    var c = node("article", "bugcard is-new" + (f.kind === "idea" ? " is-idea" : ""));
    c.setAttribute("aria-label", W(f).card);
    var head = node("header", "bug-head");
    if (f.kind === "idea") {
      // Posted, the bulb lights (round 24): the wrap carries its two rings.
      var bulb = node("span", "bulb-wrap");
      bulb.appendChild(svg(GLYPH_BULB, "bug-glyph"));
      head.appendChild(bulb);
    } else head.appendChild(svg(GLYPH_BUG, "bug-glyph"));
    head.appendChild(node("span", "bug-kind", W(f).card));
    var pill = node("span", "bug-pill");
    pill.setAttribute("role", "status");
    head.appendChild(pill);
    c.appendChild(head);
    c.appendChild(node("p", "bug-ctitle"));
    var route = node("dl", "bug-route");
    route.appendChild(node("dt", null, "From"));
    route.appendChild(node("dd", "r-from"));
    route.appendChild(node("dt", null, "To"));
    route.appendChild(f.kind === "idea" ? ideasTo() : node("dd", null, "RichOS on GitHub, as an issue anyone can read"));
    c.appendChild(route);
    var doc = node("div", "bug-doc");
    var title = node("h3", "bug-title");
    appendSegments(title, d.title);
    doc.appendChild(title);
    d.sections.forEach(function (s) {
      var sec = node("section", "bug-sec");
      sec.appendChild(node("h4", null, s.heading));
      var bulleted = f.kind === "idea" && s.heading === BULLETED;
      (s.paragraphs || []).forEach(function (p) {
        var para = node("p");
        appendSegments(para, p);
        sec.appendChild(para);
      });
      if (s.steps && s.steps.length) {
        var ol = node(bulleted ? "ul" : "ol");
        s.steps.forEach(function (st) {
          var li = node("li");
          appendSegments(li, st);
          ol.appendChild(li);
        });
        sec.appendChild(ol);
      }
      doc.appendChild(sec);
    });
    c.appendChild(doc);
    var lo = node("p", "bug-leftout");
    lo.appendChild(svg(GLYPH_EYEOFF));
    lo.appendChild(node("span", "lo-text"));
    c.appendChild(lo);
    var warn = node("p", "bug-warn");
    warn.hidden = true;
    c.appendChild(warn);
    var hint = node("p", "bug-edithint", "Change any of the words above or tell Rich what to change.");
    hint.hidden = true;
    c.appendChild(hint);
    c.appendChild(node("div", "bug-actions"));
    var sentRow = node("div", "bug-sentrow");
    sentRow.hidden = true;
    c.appendChild(sentRow);
    doc.addEventListener("input", function () { paintWarn(f); });
    title.addEventListener("keydown", function (e) { if (e.key === "Enter") e.preventDefault(); });
    return c;
  }

  /// **AN IDEA'S TO LINE** (round 24): RichOS on GitHub, as a post in its Discussions, under
  /// Ideas (a small gold-ruled label with the bulb, like GitHub's own category label), where
  /// anyone can read and upvote it.
  function ideasTo() {
    var dd = node("dd");
    dd.appendChild(document.createTextNode("RichOS on GitHub, as a post in its Discussions, under "));
    var cat = node("span", "idea-cat");
    cat.appendChild(svg(GLYPH_BULB));
    cat.appendChild(document.createTextNode("Ideas"));
    dd.appendChild(cat);
    dd.appendChild(document.createTextNode(", where anyone can read and upvote it"));
    return dd;
  }

  function accountIsUser() {
    return !!(context && context.account && context.account.kind === "user");
  }
  function paintFrom(dd) {
    dd.textContent = "";
    if (accountIsUser()) {
      dd.appendChild(document.createTextNode("your GitHub account, "));
      dd.appendChild(node("b", null, "@" + context.account.login));
    } else {
      dd.appendChild(node("b", null, "the RichOS reporting account"));
      dd.appendChild(document.createTextNode(", because RichOS isn't signed in to a GitHub account of yours"));
    }
  }

  var KIND_WORDS = {
    conversation_name: ["conversation name", "conversation names"],
    company_name: ["company name", "company names"],
    person_name: ["person's name", "people's names"],
    folder_name: ["folder name", "folder names"],
    file_path: ["file path", "file paths"],
    email_address: ["email address", "email addresses"],
    private_word: ["private word", "private words"],
  };
  var NUM = ["no", "one", "two", "three", "four", "five", "six", "seven", "eight", "nine"];

  // ---- the stand-ins: read from the card's words, whoever wrote them -------------------------
  /// **EVERY STAND-IN IS READ FROM THE CARD'S FINAL WORDS** (reviews
  /// rv-20261010T183313Z-fe4cd6b1-1e66 finding 1 and the two before it): which words were marked
  /// used to come from where they were written (the first draft, Rich's change), so a stand-in the
  /// user typed, or one edited into another (`[a person]` into `[a company]`), was plain words and
  /// the line under the card said nothing private was in it. Now every stand-in written anywhere in
  /// the title, the headings, the paragraphs and the steps is one, and the line counts exactly
  /// those. The stand-ins, word for word as the core writes them (`richos_core::bug_report::Kind::
  /// stand_in`), with their kinds.
  var STAND_IN_KIND = {
    "[a conversation]": "conversation_name",
    "[a company]": "company_name",
    "[a person]": "person_name",
    "[a folder]": "folder_name",
    "[a file on this Mac]": "file_path",
    "[an email address]": "email_address",
    "[a private word]": "private_word",
  };
  var STAND_IN_RE = new RegExp(Object.keys(STAND_IN_KIND).sort(function (a, b) { return b.length - a.length; }).map(function (s) {
    return s.replace(/[.*+?^${}()|[\]\\]/g, "\\$&");
  }).join("|"), "g");
  /// Each stand-in written in `text`, in order (the core's `stand_ins_in`): `[at, words, kind]`.
  function standInsIn(text) {
    var out = [], m;
    STAND_IN_RE.lastIndex = 0;
    while ((m = STAND_IN_RE.exec(text))) out.push([m.index, m[0], STAND_IN_KIND[m[0]]]);
    return out;
  }
  /// Every part of the card's words, in the order it shows them: the title, then each section's
  /// heading, paragraphs and steps.
  function cardParts(f) {
    var parts = [f.card.querySelector(".bug-title")];
    f.card.querySelectorAll(".bug-sec").forEach(function (s) {
      var own = fieldsIn(s);
      parts = parts.concat([s.querySelector("h4")], own.paragraphs, own.steps);
    });
    return parts;
  }
  /// The card's stand-ins marked from its words as they are now. Within one part: a marked span
  /// that is no longer exactly one stand-in is plain words again; one that is keeps what it
  /// replaced only while it is still the stand-in that was written for those words (its receipt,
  /// `data-stand`), and otherwise says its kind alone; and every stand-in in the plain words is
  /// marked with its kind. Never run while the card is being changed, so the caret stays put.
  function markStandIns(f) {
    cardParts(f).forEach(function (part) {
      var spans = part.querySelectorAll("span");
      for (var i = spans.length - 1; i >= 0; i--) {
        var s = spans[i];
        var words = s.childNodes.length === 1 && s.firstChild.nodeType === 3 ? s.textContent : null;
        var kind = words !== null && STAND_IN_KIND.hasOwnProperty(words) ? STAND_IN_KIND[words] : null;
        if (!kind) {
          while (s.firstChild) s.parentNode.insertBefore(s.firstChild, s);
          s.remove();
          continue;
        }
        if (s.dataset.stand !== words) delete s.dataset.was;
        s.className = "bug-sub";
        s.tabIndex = 0;
        s.dataset.kind = kind;
        s.dataset.stand = words;
      }
      part.normalize();
      var texts = [];
      var walk = document.createTreeWalker(part, NodeFilter.SHOW_TEXT);
      for (var t = walk.nextNode(); t; t = walk.nextNode()) if (!t.parentNode.classList.contains("bug-sub")) texts.push(t);
      texts.forEach(function (t) {
        var found = standInsIn(t.data);
        if (!found.length) return;
        var frag = document.createDocumentFragment(), at = 0;
        found.forEach(function (x) {
          if (x[0] > at) frag.appendChild(document.createTextNode(t.data.slice(at, x[0])));
          frag.appendChild(subSpan(x[1], x[2], null));
          at = x[0] + x[1].length;
        });
        if (at < t.data.length) frag.appendChild(document.createTextNode(t.data.slice(at)));
        t.parentNode.replaceChild(frag, t);
      });
    });
  }

  /// The line under the sheet, in Rust's words (`left_out_line`), COUNTED FROM THE CARD'S WORDS:
  /// every stand-in in its title, headings, paragraphs and steps, by kind (`standInsIn`), and
  /// nothing else. A stand-in the user deleted is no longer counted; one anybody wrote is.
  function leftOutText(f) {
    var counts = {}, order = [], total = 0;
    cardParts(f).forEach(function (part) {
      standInsIn(part.nodeName === "H4" ? part.textContent : wordsOf(part)).forEach(function (x) {
        var k = x[2];
        if (!counts[k]) { counts[k] = 0; order.push(k); }
        counts[k] += 1;
        total += 1;
      });
    });
    if (!order.length) return "Nothing private was in it, so nothing was left out. Anyone can read " + W(f).readers + ".";
    var parts = order.map(function (k) {
      var n = counts[k], w = KIND_WORDS[k] || ["private word", "private words"];
      return (NUM[n] || n) + " " + (n === 1 ? w[0] : w[1]);
    });
    var list = parts.length === 1 ? parts[0] : parts.slice(0, -1).join(", ") + " and " + parts[parts.length - 1];
    return "Left out, because anyone can read " + W(f).readers + ": " + list + ". The underlined words stand in for " + (total === 1 ? "it" : "them") + "; point at one to see what it replaced.";
  }

  /// A stand-in whose words the user changed is the user's own text from then on: no longer
  /// marked or explained as the stand-in it was, and read by the heads-up like any other words
  /// (third review: a name typed inside "[a person]" was sent with no heads-up). It stays a plain
  /// span so the caret the user is typing at does not move. When the card is no longer being
  /// changed, `markStandIns` reads it again: if its words are a stand-in, it is one, by its kind.
  function releaseEdited(f) {
    var released = false;
    f.card.querySelectorAll(".bug-doc .bug-sub").forEach(function (s) {
      if (s.textContent === s.dataset.stand) return;
      s.className = "";
      delete s.dataset.was;
      delete s.dataset.kind;
      delete s.dataset.stand;
      s.removeAttribute("tabindex");
      released = true;
    });
    // Its "Stands in for" tooltip, if it was showing, no longer describes anything.
    if (released) hideSubTip();
  }

  function warns(f) { return f.step === "editing" || f.step === "draft" || f.step === "queued"; }

  /// The heads-up, from the core's answer about the words on the card now. Each change asks
  /// again; an answer that comes after a later question is dropped, so the heads-up is always
  /// about the latest words.
  ///
  /// Send waits for that answer (review rv-20261009T151254Z-84d1bdce-20df finding 2: on the tip
  /// the report could go out while the answer was on its way, and its heads-up was never shown).
  /// A press while the latest question is out is held (`f.sendHeld`, the heads-up as it read at
  /// the press); when the answer arrives the report goes, unless the answer brought a heads-up
  /// the user had not seen when pressing: then it stays unsent under it, and Send is the user's
  /// again, after it. An answer that never comes (`catch`) sends what was pressed, as before:
  /// the heads-up decides nothing.
  function paintWarn(f) {
    releaseEdited(f);
    var w = f.card.querySelector(".bug-warn");
    if (!warns(f)) {
      f.warnAsked = (f.warnAsked || 0) + 1;
      f.warnAnswered = f.warnAsked;
      f.sendHeld = null;
      w.hidden = true;
      w.textContent = "";
      return;
    }
    // Read the way Send reads the card (`wordsOf`), so a line break typed in a change is a break
    // here too: "Jane" and "Doe" on two lines are not asked about as "JaneDoe" (review of 665df1bb3).
    var doc = f.card.querySelector(".bug-doc").cloneNode(true);
    doc.querySelectorAll(".bug-sub").forEach(function (s) { s.remove(); });
    var text = fieldsOf(doc).map(wordsOf).join("\n");
    var asked = (f.warnAsked = (f.warnAsked || 0) + 1);
    askPrivate(text, f).then(function (all) {
      if (asked !== f.warnAsked || !warns(f)) return;
      var found = all.slice(0, 3);
      w.hidden = found.length === 0;
      w.textContent = "";
      if (!found.length) return;
      w.appendChild(node("strong", null, found.map(function (x) { return "“" + x + "”"; }).join(", ") + (found.length === 1 ? " looks" : " look") + " private."));
      w.appendChild(document.createTextNode(" Anyone can read this " + W(f).thing + " on GitHub."));
    }).catch(function () {
      // Not answered: the heads-up stays as it was. It decides nothing; Send is the user's.
    }).then(function () {
      if (asked !== f.warnAsked) return;
      f.warnAnswered = asked;
      var held = f.sendHeld;
      f.sendHeld = null;
      if (held === null || held === undefined || f.step !== "draft") return;
      if (!w.hidden && w.textContent !== held) return;
      send(f);
    });
  }

  /// **THE CARD TAKES PLAIN TEXT ONLY** (review rv-20261009T225657Z-b3b5c573-96c0). A rich-text
  /// field let a paste bring its markup onto the card (a list inside a step, a table, styled
  /// words), and what Send read of it was not what the card showed. `plaintext-only` (WebKit, and
  /// WebView2's Chromium) refuses markup from every way in: paste, drop and the formatting keys.
  /// Where an engine does not know it, the field is ordinary rich text and `pastePlain` still
  /// keeps a paste's markup out.
  function plainTextOnly(n) {
    n.setAttribute("contenteditable", "plaintext-only");
    if (n.contentEditable !== "plaintext-only") n.setAttribute("contenteditable", "true");
  }
  /// A paste into a field being changed is the clipboard's plain text, put in as if typed: a
  /// line is a line (WebKit makes each one a `<div>`, which `wordsOf` reads as a break) and a tab
  /// is a space, so a pasted table's cells are words apart on the card and in what Send reads.
  function pastePlain(e) {
    var t = e.target && (e.target.nodeType === 1 ? e.target : e.target.parentElement);
    if (!t || !t.closest || !t.closest(".bugcard.is-editing [contenteditable]")) return;
    e.preventDefault();
    var text = (e.clipboardData && e.clipboardData.getData("text/plain")) || "";
    text = text.replace(/\r\n?/g, "\n").replace(/\t/g, " ");
    if (text) document.execCommand("insertText", false, text);
  }

  function button(label, primary, fn, id) {
    var b = node("button", "desk-btn" + (primary ? " desk-btn--confirm" : ""), label);
    b.type = "button";
    if (id) b.id = id;
    b.addEventListener("click", fn);
    return b;
  }

  function paintCard(f) {
    var c = f.card;
    if (!c) return;
    ["is-sending", "is-queued", "is-sent", "is-editing", "is-canceled"].forEach(function (k) { c.classList.remove(k); });
    var pill = c.querySelector(".bug-pill"), row = c.querySelector(".bug-sentrow");
    // **THE BUTTON ROW IS A NEW BOX AT EVERY PAINT, NOT THE OLD ONE EMPTIED** (nightly 49's walk,
    // D29). The row is sticky; while it floats over a long report, a step that changes the card's
    // height (Done hides the editing hint and the dashed outline) moves its place in the flow in
    // the same update that swaps its buttons. The shipped WebKit then repainted the new buttons
    // only where the old ones had been: after Done, "Send rep" was drawn inside Done's old 74px
    // and Change it and Cancel not at all, while the accessibility tree had all three at full size
    // (runs A and E; and 3 of 3 in Safari 18.6 on a guest, on the same files, where the card's
    // height held at Done drew all three). A box put in fresh is painted whole (also measured
    // there). Its classes (`is-stuck`) carry over so its edge does not blink before `paintStuck`.
    var old = c.querySelector(".bug-actions");
    var acts = node("div", old.className);
    old.replaceWith(acts);
    row.hidden = true;
    acts.hidden = false;
    c.querySelector(".bug-edithint").hidden = f.step !== "editing";
    if (f.step !== "sent" && f.step !== "sending") paintFrom(c.querySelector(".r-from"));
    if (f.step !== "editing") markStandIns(f);
    c.querySelector(".lo-text").textContent = leftOutText(f);
    var editable = f.step === "editing";
    fieldsOf(c).forEach(function (n) {
      if (editable) plainTextOnly(n);
      else n.removeAttribute("contenteditable");
    });
    switch (f.step) {
      case "draft":
        pill.textContent = f.edited ? W(f).pill.changed : W(f).pill.draft;
        acts.appendChild(button(W(f).go, true, function () { send(f); }, "bug-send"));
        acts.appendChild(button("Change it", false, function () { edit(f); }, "bug-change"));
        acts.appendChild(button("Cancel", false, function () { cancel(f); }, "bug-cancel"));
        break;
      case "editing":
        c.classList.add("is-editing");
        pill.textContent = "Changing it";
        acts.appendChild(button("Done", true, function () { finishEdit(f); }, "bug-done"));
        break;
      case "sending":
        c.classList.add("is-sending");
        pill.textContent = f.handEdited ? CHECKING_CHANGES : W(f).pill.sending;
        acts.hidden = true;
        break;
      case "unchecked":
        // Changed words Rich couldn't check yet: nothing to send until he has (`changesKept`).
        c.classList.add("is-queued");
        pill.textContent = "Waiting for Rich's check · saved on this Mac";
        acts.hidden = true;
        break;
      case "changing":
        // Send, Change it and Cancel wait for Rich: what is sent is what he changed.
        pill.textContent = "Changing it…";
        acts.hidden = true;
        break;
      case "withdrawing":
        // Still waiting on this Mac until the shell confirms it is off.
        c.classList.add("is-queued");
        pill.textContent = f.withdrawFor === "cancel" ? "Canceling…" : "Taking it back to change it…";
        acts.hidden = true;
        break;
      case "queued":
        c.classList.add("is-queued");
        pill.textContent = W(f).pill.queued;
        acts.appendChild(button("Try now", true, function () { tryNow(f); }, "bug-try-now"));
        acts.appendChild(button("Change it", false, function () { edit(f); }, "bug-change"));
        acts.appendChild(button(W(f).drop, false, function () { cancel(f); }, "bug-cancel"));
        break;
      case "sent":
        c.classList.add("is-sent");
        acts.hidden = true;
        row.hidden = false;
        row.textContent = "";
        if (f.kind === "idea") {
          // Posted: no number in the words (Discussions share one numbering with the repository,
          // so the number shows only in the address), and the bulb lights.
          pill.textContent = "Posted · in Ideas";
          var open = node("a", "bug-link", "Open it on GitHub");
          open.href = "#";
          open.dataset.url = f.issue.url;
          open.dataset.post = "idea";
          row.appendChild(open);
          row.appendChild(node("span", null, "posted from " + (f.issue.account && f.issue.account.kind === "user" ? "@" + f.issue.account.login : "the RichOS reporting account")));
          break;
        }
        pill.textContent = "Sent · #" + f.issue.number;
        var a = node("a", "bug-link", "Open issue #" + f.issue.number + " on GitHub");
        a.href = "#";
        a.dataset.url = f.issue.url;
        row.appendChild(a);
        row.appendChild(node("span", null, "filed from " + (f.issue.account && f.issue.account.kind === "user" ? "@" + f.issue.account.login : "the RichOS reporting account")));
        break;
      case "canceled":
        c.classList.add("is-canceled");
        pill.textContent = W(f).pill.canceled;
        c.querySelector(".bug-ctitle").textContent = c.querySelector(".bug-title").textContent;
        break;
    }
    paintWarn(f);
    window.requestAnimationFrame(paintStuck);
  }

  function edit(f) {
    if (f.step !== "draft" && f.step !== "queued") return;
    // Changing a waiting report takes it out of the queue, confirmed, first: a changed report is
    // approved again, and one that went out meanwhile is not opened as if it had not.
    if (f.step === "queued" && f.pendingId) return withdraw(f, "change", function () { openEditor(f); });
    openEditor(f);
  }
  function openEditor(f) {
    f.sendHeld = null; // a Send pressed before changing it is not a Send of the changed words
    f.beforeEdit = JSON.stringify(sheetOf(f));
    f.step = "editing";
    paintCard(f);
    paintComposers();
    var t = f.card.querySelector(".bug-title");
    t.focus();
    var r = document.createRange();
    r.selectNodeContents(t);
    r.collapse(false);
    var sel = window.getSelection();
    sel.removeAllRanges();
    sel.addRange(r);
  }
  function finishEdit(f) {
    if (f.step !== "editing") return;
    f.edited = true;
    // Only says what Send will be doing first ("Rich is checking your changes…"): whether the
    // words may go is the shell's to decide, against Rich's own check, never this flag's.
    if (JSON.stringify(sheetOf(f)) !== f.beforeEdit) f.handEdited = true;
    f.step = "draft";
    paintCard(f);
    paintComposers();
  }

  // ---- send, and what happens when it can't go ----------------------------------------------
  var WAITING_FIRST = {
    offline: "This Mac is offline, so the report didn't go out. Nothing is lost: it's saved on this Mac exactly as you approved it. I'll send it by myself as soon as you're back online and tell you here when it's gone.",
    "github-down": "GitHub isn't answering right now, so the report didn't go out. Nothing is lost: it's saved on this Mac exactly as you approved it. I'll keep trying every few minutes and tell you here when it's filed.",
    "not-set-up": "The RichOS reporting account isn't set up on this Mac yet, so the report didn't go out. Nothing is lost: it's saved on this Mac exactly as you approved it. I'll send it by myself once the account is set up and tell you here when it's filed.",
    refused: "GitHub didn't accept the report from the RichOS reporting account, so it didn't go out. Nothing is lost: it's saved on this Mac exactly as you approved it. I'll try again in a few minutes and tell you here when it's filed.",
  };
  var WAITING_AGAIN = {
    offline: "Still offline. It stays saved here and goes out by itself when you're back online.",
    "github-down": "GitHub still isn't answering. It stays saved here and I'll keep trying.",
  };
  var CAME_BACK = {
    offline: "You're back online, so I sent your bug report.",
    "github-down": "GitHub is answering again, so I sent your bug report.",
  };
  // The same, for an idea (round 24): posted, never filed or sent as an issue. "refused" is an
  // account GitHub will not let post in RichOS's Discussions (a token without that permission),
  // said as it is, with the account it was.
  var IDEA_WAITING_FIRST = {
    offline: "This Mac is offline, so your idea wasn't posted. Nothing is lost: it's saved on this Mac exactly as you approved it. I'll post it by myself as soon as you're back online and tell you here when it's up.",
    "github-down": "GitHub isn't answering right now, so your idea wasn't posted. Nothing is lost: it's saved on this Mac exactly as you approved it. I'll keep trying every few minutes and tell you here when it's up.",
    "not-set-up": "The RichOS reporting account isn't set up on this Mac yet, so your idea wasn't posted. Nothing is lost: it's saved on this Mac exactly as you approved it. I'll post it by myself once the account is set up and tell you here when it's up.",
  };
  function ideaRefused() {
    var who = accountIsUser() ? "your GitHub account, @" + context.account.login + "," : "the RichOS reporting account";
    return "GitHub didn't let " + who + " post in RichOS's Discussions, so your idea wasn't posted. Nothing is lost: it's saved on this Mac exactly as you approved it. I'll try again in a few minutes and tell you here when it's up.";
  }
  var IDEA_CAME_BACK = {
    offline: "You're back online, so I posted your idea.",
    "github-down": "GitHub is answering again, so I posted your idea.",
  };

  function send(f) {
    if (f.step !== "draft") return; // held while Rich changes it; never twice
    if (f.warnAnswered !== f.warnAsked) {
      // The heads-up on these words is still on its way: the press waits for it (`paintWarn`).
      var w = f.card.querySelector(".bug-warn");
      f.sendHeld = w.hidden ? "" : w.textContent;
      return;
    }
    f.step = "sending";
    paintCard(f);
    paintComposers();
    var checking = f.handEdited ? working(f, CHECKING_CHANGES) : null;
    bridge.invoke("bug_report_send", { report: f.report || null, sheet: sheetOf(f), screen: publicScreen(f.screen), post: postOf(f) }).then(function (a) {
      if (checking) checking.remove();
      if (f.step !== "sending") return;
      // Not sent: the changed words went through the scanner and then Rich, and his version
      // differs from the card. The card comes back as it would go.
      if (a && a.state === "checked") return showDraft(f, a, W(f).shownAgain, null);
      // Not sent: Rich couldn't check the changed words yet; they wait on this Mac.
      if (a && a.state === "unchecked") return changesKept(f, a.id);
      delivered(f, a, null);
    }).catch(function () {
      if (checking) checking.remove();
      f.step = "draft";
      paintCard(f);
      paintComposers();
      richSays(f, [W(f).notSaved]);
    });
  }

  function tryNow(f) {
    if (f.step !== "queued") return;
    if (!f.pendingId) { f.step = "draft"; return send(f); }
    var id = f.pendingId;
    f.step = "sending";
    paintCard(f);
    paintComposers();
    bridge.invoke("bug_report_try_now", { id: id, post: postOf(f) }).then(function (d) {
      if (!d) return; // it went out by itself a moment ago; the event says so
      delivered(f, d, null);
    }).catch(function () {
      f.step = "queued";
      paintCard(f);
      paintComposers();
    });
  }

  function delivered(f, d, why) {
    if (d.state === "sent") return markSent(f, d, why);
    // "Still …" only when it failed again FOR THE SAME REASON: a new reason (offline, then an
    // account GitHub refuses) is explained in full, as the first failure was (review
    // rv-20261011T131221Z-ba6a6a47-c22a, P2).
    var again = f.queuedOnce && f.reason === d.reason;
    f.pendingId = d.id;
    f.reason = d.reason;
    f.queuedOnce = true;
    f.step = "queued";
    paintCard(f);
    paintComposers();
    if (why) return; // a retry by itself that failed again says nothing new
    var first = f.kind === "idea"
      ? (d.reason === "refused" ? ideaRefused() : IDEA_WAITING_FIRST[d.reason] || IDEA_WAITING_FIRST["github-down"])
      : WAITING_FIRST[d.reason] || WAITING_FIRST["github-down"];
    richSays(f, [(again && (WAITING_AGAIN[d.reason] || "It still didn't go out. It stays saved here and I'll keep trying.")) || first]);
  }

  /// THE ADDRESS AS RICH SAYS IT: the link the relay answered with, without `https://`. A report
  /// sent before the GitHub move names the old repository and one sent after names the new one
  /// (CEO, 2026-10-27), so the words always match the page the link opens.
  function shownAddress(url) {
    return String(url || "").replace(/^https:\/\//, "");
  }

  /// `lead`, when given, is Rich's first sentence instead (a report that went out before it
  /// could be canceled or changed).
  function markSent(f, d, why, lead) {
    if (f.step === "sent") return; // already said: the event and a command's answer can both bring it
    f.issue = { number: d.number, account: d.account, url: d.url };
    f.pendingId = null;
    f.step = "sent";
    paintCard(f);
    paintComposers();
    var pill = f.card.querySelector(".bug-pill");
    pill.classList.remove("pop");
    void pill.offsetWidth;
    pill.classList.add("pop");
    if (f.kind === "idea") return ideaPosted(f, d, why, lead);
    var acct = d.account && d.account.kind === "user" ? "filed from your GitHub account, @" + d.account.login + "."
      : "filed from the RichOS reporting account, because RichOS isn't signed in to a GitHub account of yours.";
    var opening = lead ? lead + " It's issue #" + d.number + " on GitHub, "
      : why ? (CAME_BACK[why] || "I sent your bug report.") + " It's issue #" + d.number + " on GitHub, "
      : "Sent. It's issue #" + d.number + " on GitHub, ";
    var link = node("a", "bug-link", shownAddress(d.url));
    link.href = "#";
    link.dataset.url = d.url;
    richSays(f, [opening + acct, link]);
    if (why && host) host.toast("Your bug report went out: issue #" + d.number + " on GitHub.");
  }

  /// **AN IDEA, POSTED** (round 24): the bulb lights, and Rich says where it went and from which
  /// account, with the link. The words never give the post a number.
  function ideaPosted(f, d, why, lead) {
    var bulb = f.card.querySelector(".bulb-wrap");
    if (bulb) {
      bulb.classList.remove("lit");
      void bulb.offsetWidth;
      bulb.classList.add("lit");
    }
    var from = d.account && d.account.kind === "user" ? "posted from your GitHub account, @" + d.account.login + "."
      : "posted from the RichOS reporting account, because RichOS isn't signed in to a GitHub account of yours.";
    var opening = lead ? lead + " " : why ? (IDEA_CAME_BACK[why] || "I posted your idea.") + " " : "Posted. ";
    var link = node("a", "bug-link", shownAddress(d.url));
    link.href = "#";
    link.dataset.url = d.url;
    link.dataset.post = "idea";
    richSays(f, [opening + "It's in RichOS's Discussions on GitHub, under Ideas, " + from + " Anyone can read it and upvote it there.", link]);
    if (why && host) host.toast("Your idea is posted in Ideas, on GitHub.");
  }

  /// *Cancel* on a report not yet sent ends it at once: nothing of it is anywhere but this card.
  /// *Cancel report* on a waiting one says "Nothing was sent" only once its copy is confirmed off
  /// this Mac; if it went out first, the card says Sent and Rich says so (finding 2).
  function cancel(f) {
    if (f.step !== "draft" && f.step !== "queued") return;
    withdraw(f, "cancel", function () {
      f.step = "canceled";
      paintCard(f);
      paintComposers();
      richSays(f, [W(f).canceled]);
    });
  }

  /// A waiting report went out by itself (the shell's retry): the card and Rich say so, and a
  /// notice says the same wherever the user is. After a relaunch there is no card to update, so
  /// the notice is the whole of it.
  function onDelivered(payload) {
    if (payload && payload.checked) return onChecked(payload.checked);
    if (!payload || !payload.delivery || payload.delivery.state !== "sent") return;
    var d = payload.delivery;
    var f = flows.filter(function (x) { return x.pendingId === d.id; })[0];
    if (f && f.step === "withdrawing") wentOutFirst(f, d);
    else if (f) markSent(f, d, f.reason || "github-down");
    else if (host && (d.post === "idea" || payload.post === "idea")) host.toast("Your idea is posted in Ideas, on GitHub.");
    else if (host) host.toast("Your bug report went out: issue #" + d.number + " on GitHub.");
  }

  // ---- the Rich panel ------------------------------------------------------------------------
  /// The panel's words are written when it is first shown rather than shipped in index.html:
  /// a hidden panel's text is still text on the page, and the contrast gate's floors count every
  /// text node of the bare shell (tests/contrast.js check 9z).
  function fillDock() {
    var dock = $("bugdock");
    if (dock.dataset.filled) return;
    dock.dataset.filled = "1";
    $("bugdock-x").textContent = "✕";
    $("bugdock-send").firstElementChild.textContent = "▷";
    $("bugdock-voice-label").textContent = "listening…";
  }
  /// ON TOP OF THE SCREEN THE BUG IS ON, whatever that screen is: the panel is the top layer of
  /// the app by one rule in `style.css` (`#bugdock`, z-index 298), above every window, sheet and
  /// backdrop, so nothing here decides when to raise it. The screen stays up behind it, as it is
  /// the screen the report is about and the one in its picture.
  function showDock(f) {
    fillDock();
    // The panel says which of the two it is (round 24: "Request a feature · Corrections").
    var words = W(f || dockFlow());
    $("bugdock").querySelector(".bugdock-title").textContent = words.name;
    $("bugdock").setAttribute("aria-label", words.name + ", with Rich");
    $("bugdock-x").setAttribute("aria-label", words.close);
    quietOtherInputs();
    $("bugdock").hidden = false;
    document.body.classList.add("bug-dock-open");
    paintDock();
    paintMenu();
  }
  /// **ONE INPUT AT A TIME.** A report in the panel does not share the keyboard with another
  /// mode: Dictation's "Press a different key" ends by its own Cancel, which also disarms the
  /// tool's key tap, so typing, Enter and a function key are the panel's again; and conversation
  /// voice mode goes off by its own off path, so nothing said becomes a message to Rich. The
  /// panel's own talk button is how the user speaks to the report. Both off paths put focus on
  /// their own controls (Dictation's key button, the conversation's answer box), so focus goes
  /// back to `keep`, the thing in the panel the user chose.
  function quietOtherInputs(keep) {
    var was = document.activeElement;
    var cancel = $("dict-capture-cancel");
    if (cancel) cancel.click();
    if (host && host.voiceOn()) host.voiceOff();
    if (keep && document.activeElement !== was && $("bugdock").contains(keep)) keep.focus();
  }
  function hideDock() {
    $("bugdock").hidden = true;
    document.body.classList.remove("bug-dock-open");
    setDockVoice(false);
    paintMenu();
  }
  function paintDock() {
    var f = dockFlow();
    var busy = f && ["checking", "draft", "editing", "changing", "sending", "withdrawing"].indexOf(f.step) !== -1;
    var x = $("bugdock-x");
    if (x) x.hidden = !!busy; // while a report waits on a decision, Send or Cancel is the way out
  }

  // ---- paint the sticky actions' edge only while they float over the report -----------------
  function paintStuck() {
    document.querySelectorAll(".bug-actions").forEach(function (a) {
      if (a.hidden) return;
      var sc = a.closest(".bugdock-msgs");
      var card = a.closest(".bugcard");
      if (!sc || !card) return;
      a.classList.toggle("is-stuck", card.getBoundingClientRect().bottom > sc.getBoundingClientRect().bottom + 2);
    });
  }

  // ---- voice ---------------------------------------------------------------------------------
  /// While a report waits on the user's words, what they SAY goes to it rather than to Rich's
  /// conversation. The shell holds the spoken words back from the spine while this is on.
  var voiceHeld = false;
  /// **Whether what the user says now is the report's.** The ONE condition both the shell's hold
  /// (`syncVoiceHold`) and the report's taking of a transcript (`takeSpoken`) follow: a report
  /// waiting on words, with the panel's own talk button on. Conversation voice mode is Rich's
  /// conversation's, never the report's (fourth review finding 3: two conditions drifted apart
  /// once, and the shell dropped every spoken turn in another conversation).
  function waitsForWords(f) {
    return !!(f && TAKES_WORDS.indexOf(f.step) !== -1);
  }
  function takesSpeech(f) {
    return waitsForWords(f) && dockVoice;
  }
  /// **THE PANEL'S MICROPHONE IS ON ONLY WHILE THE REPORT WAITS FOR THE USER'S WORDS** (reviews
  /// rv-20261010T180327Z-f29db708-24e0 finding 2, rv-20261010T180313Z-f29db708-733e finding 3): at
  /// every other step (Rich checking, sending, a report closed) it is stopped first, and only once
  /// the shell has closed it is the hold released. On f29db7082 moving to checking released the
  /// hold with the microphone still on, and the shell submits every sentence nothing holds back
  /// (`main.rs`, `start_voice_capture`'s submit), so the next sentence became a turn in Rich's
  /// conversation. `micStopping` is that stop while it runs: the hold stays on through it.
  var micStopping = null;
  function syncVoiceHold() {
    var f = liveFlow();
    if (dockVoice && !waitsForWords(f)) return setDockVoice(false);
    var want = takesSpeech(f) || !!micStopping;
    if (want === voiceHeld || !bridge) return;
    voiceHeld = want;
    bridge.invoke("bug_report_voice", { on: want }).catch(function () {});
  }
  function setDockVoice(on) {
    if (dockVoice === on) return;
    // Nothing waits for words: the microphone stays off.
    if (on && !waitsForWords(liveFlow())) return;
    // The talk button is a way into the panel just as focus is (WebKit does not focus a button
    // that is clicked): the other input modes go off by their own off paths first.
    if (on) quietOtherInputs($("bugdock-talk"));
    dockVoice = on;
    var t = $("bugdock-talk");
    if (t) t.setAttribute("aria-pressed", String(on));
    $("bugdock-form").hidden = on;
    $("bugdock-voice").hidden = !on;
    if (!bridge) return;
    var args = { threadId: host ? host.activeThread() : null };
    if (!on) {
      // Stopped first; the hold goes only once the shell says the microphone is closed. A stop
      // that fails keeps the hold: words said then go nowhere rather than to the conversation.
      var stop = bridge.invoke("stop_voice_capture", args);
      micStopping = stop;
      stop.then(function () {
        if (micStopping === stop) micStopping = null;
        syncVoiceHold();
      }, function () {});
      return;
    }
    // The hold is on before the microphone opens, and a stop still running finishes first.
    // **A START THAT WAITED FOR A STOP RUNS ONLY IF THE TALK BUTTON IS STILL ON THEN** (review
    // rv-20261010T183313Z-fe4cd6b1-1e66 finding 2): on fe4cd6b14 talk turned on and off again
    // while a stop ran left the start queued, and it opened the microphone with the hold released.
    syncVoiceHold();
    var open = function () { return dockVoice ? bridge.invoke("start_voice_capture", args) : Promise.resolve(); };
    (micStopping ? micStopping.then(open, open) : open()).catch(function (e) {
      dockVoice = false;
      if (t) t.setAttribute("aria-pressed", "false");
      $("bugdock-form").hidden = false;
      $("bugdock-voice").hidden = true;
      syncVoiceHold();
      var f = dockFlow();
      if (f) richSays(f, [typeof e === "string" && e ? e : "I couldn't open the microphone. Type it instead."]);
    });
  }

  /// A recognized utterance. True when it was the bug report's.
  function takeSpoken(text) {
    var f = liveFlow();
    if (!takesSpeech(f)) return false;
    say(f, text, true);
    return true;
  }

  // ---- what a stand-in replaced: seen only here --------------------------------------------
  function showSubTip(s) {
    var tip = $("bug-subtip");
    var k = KIND_WORDS[s.dataset.kind] || ["private word", "private words"];
    var a = (/^[aeiou]/.test(k[0]) ? "an " : "a ") + k[0];
    tip.textContent = "";
    if (s.dataset.was == null) {
      // The app does not know the words this stand-in replaced (Rich wrote it, the user typed
      // it, or it was edited from another), so the tip says its kind and claims nothing more
      // (D23; review rv-20261010T183313Z-fe4cd6b1-1e66: who wrote it is not known either).
      tip.appendChild(node("b", null, "Stands in for " + a + "."));
    } else {
      tip.appendChild(node("b", null, "Stands in for “" + s.dataset.was + "”"));
      var card = s.closest(".bugcard");
      tip.appendChild(document.createTextNode(", " + a + ". Only you see this; " + (card && card.classList.contains("is-idea") ? WORDS.idea : WORDS.bug).notInIt));
    }
    tip.hidden = false;
    var r = s.getBoundingClientRect(), w = tip.offsetWidth;
    tip.style.left = Math.max(12, Math.min(window.innerWidth - w - 12, r.left + r.width / 2 - w / 2)) + "px";
    tip.style.top = r.bottom + 10 + "px";
    void tip.offsetWidth;
    tip.classList.add("is-shown");
  }
  function hideSubTip() {
    var t = $("bug-subtip");
    if (!t) return;
    t.classList.remove("is-shown");
    t.hidden = true;
  }

  function wire() {
    window.addEventListener("resize", paintStuck);
    $("bugdock-msgs").addEventListener("scroll", paintStuck);
    $("bugdock-x").addEventListener("click", function () {
      var f = dockFlow();
      if (f && f.step === "ask") neverMind(f, true);
      if (f && f.step === "queued" && host) host.toast(W(f).savedToast);
      if (f && f.step === "unchecked" && host) host.toast(W(f).uncheckedToast);
      hideDock();
    });
    function dockSubmit() {
      var input = $("bugdock-input");
      var v = input.value.trim();
      if (!v) return;
      var f = dockFlow();
      if (!f || TAKES_WORDS.indexOf(f.step) === -1) return;
      input.value = "";
      paintDockSend();
      say(f, v, false);
    }
    $("bugdock-send").addEventListener("click", dockSubmit);
    $("bugdock-input").addEventListener("keydown", function (e) {
      if (e.key === "Enter" && !e.shiftKey) {
        e.preventDefault();
        dockSubmit();
      }
    });
    $("bugdock-input").addEventListener("input", paintDockSend);
    // ONE INPUT AT A TIME, again whenever the user comes back to the report, by ONE hook on the
    // panel: focus reaching anything in it (the answer box, the talk button, the card's title and
    // paragraphs while Change it is open, its buttons) ends a Dictation "Press a different key"
    // started after the panel was shown, and turns conversation voice mode off, each by its own
    // off path (rv-20261010T161328Z-06066849-659f). The talk button also does it as it turns on
    // (`setDockVoice`), since WebKit does not focus a button that is clicked.
    $("bugdock").addEventListener("focusin", function (e) { quietOtherInputs(e.target); });
    $("bugdock-talk").addEventListener("click", function () { setDockVoice(!dockVoice); });
    document.addEventListener("mouseover", function (e) { var s = e.target.closest && e.target.closest(".bug-sub"); if (s) showSubTip(s); });
    document.addEventListener("mouseout", function (e) { if (e.target.closest && e.target.closest(".bug-sub")) hideSubTip(); });
    document.addEventListener("focusin", function (e) { if (e.target.classList && e.target.classList.contains("bug-sub")) showSubTip(e.target); });
    document.addEventListener("focusout", function (e) { if (e.target.classList && e.target.classList.contains("bug-sub")) hideSubTip(); });
    document.addEventListener("scroll", hideSubTip, true);
    document.addEventListener("paste", pastePlain, true);
    document.addEventListener("click", function (e) {
      var a = e.target.closest && e.target.closest(".bug-link");
      if (!a) return;
      e.preventDefault();
      var idea = a.dataset.post === "idea";
      bridge.invoke("bug_report_open_issue", { url: a.dataset.url, post: idea ? "idea" : "bug" }).catch(function (err) {
        if (host) host.toast(typeof err === "string" && err ? err
          : idea ? "This Mac wouldn't open your idea. It's at " + shownAddress(a.dataset.url) + "."
          : "This Mac wouldn't open the issue. It's at " + shownAddress(a.dataset.url) + ".");
      });
    });
  }
  function paintDockSend() {
    var i = $("bugdock-input");
    $("bugdock-send").classList.toggle("is-live", i.value.trim().length > 0);
    i.style.height = "auto";
    i.style.height = Math.min(i.scrollHeight, 120) + "px";
  }

  function refreshContext() {
    return bridge.invoke("bug_report_context").then(function (c) {
      context = c;
      flows.forEach(function (f) { if (f.card && f.step !== "sent" && f.step !== "canceled") paintCard(f); });
    }).catch(function () {});
  }

  return {
    /// Called by main.js once the shell is up. `h` carries the conversation's own facts and
    /// controls, so this file never reaches into main.js's variables.
    init: function (h) {
      host = h;
      bridge = h.bridge;
      wire();
      refreshContext();
      // A webview reload must not leave the shell holding spoken words back for a report that
      // is no longer on screen.
      bridge.invoke("bug_report_voice", { on: false }).catch(function () {});
      bridge.listen("rich://bug-report", function (e) { onDelivered(e.payload); });
      bridge.listen("rich://voice-state", function (e) {
        if (!dockVoice || !e.payload) return;
        var label = $("bugdock-voice-label");
        if (label) label.textContent = e.payload.state === "hearing" ? "hearing you…" : e.payload.state === "thinking" ? "got it…" : "listening…";
        if (e.payload.state === "off") setDockVoice(false);
      });
      // The panel's own talk button: what is said is the report's. Exactly one listener takes
      // each utterance: this one, or, when the conversation's voice mode was turned on again
      // as well, main.js's, which asks `takeSpoken` itself before it draws anything.
      bridge.listen("rich://voice-transcript", function (e) {
        if (dockVoice && !(host && host.voiceOn()) && e.payload && e.payload.text) takeSpoken(e.payload.text);
      });
      if (window.RichSettings && window.RichSettings.registerBugReport) {
        // `open("idea")` is Request a feature (round 24); `open()` is Bust a bug.
        window.RichSettings.registerBugReport({ open: function (kind) { refreshContext(); lookThenStart(kind); }, busy: reportOpen });
      }
    },
    start: start,
    takeSpoken: takeSpoken,
    /// READ-ONLY, for the acceptance harness.
    state: function () {
      return flows.map(function (f) { return { kind: postOf(f), step: f.step, pendingId: f.pendingId || null, issue: f.issue || null, screen: f.screen.key }; });
    },
  };
})();

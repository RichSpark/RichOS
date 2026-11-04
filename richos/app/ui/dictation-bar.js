// THE DICTATION BAR AND THE WORDS' FLIGHT (dictation plan rev 2, slice 3).
//
// Round 19's bar (`design/mockups/rounds/round-19/dictation.html`, `renderPill`, `meterTick`,
// `comet`), states 14 to 19, the other drawn moments, and Iris's two bar lines
// (`dictation-more-lines.html`, `desk-nomodel`, `desk-nowrite`), every sentence word for word.
// The dictation tool (`src-tauri/src/dictation/bar.rs`) says what to show; this page draws it,
// measures it and reports the window it needs, and the tool places that window bottom center
// over whatever app is in front, never taking the cursor.
//
// One page, two windows: `?role=flight` is the click-through window over the screen where the
// words landed, which draws the flight from the bar to them and lights them for a moment.
"use strict";

window.RichDictationBar = (function () {
  var ROLE = new URLSearchParams(location.search).get("role") === "flight" ? "flight" : "bar";
  var tauri = window.__TAURI__ || null;
  // The pill's place in its window: room for its shadow (dictation-overlay.css `#dhud`).
  var PAD_X = 40;
  var PAD_TOP = 24;
  var PAD_BOTTOM = 56;

  var IC = {
    mic: '<svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2.4" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true"><rect x="9" y="3" width="6" height="11" rx="3"/><path d="M5 11a7 7 0 0 0 14 0"/><path d="M12 18v3"/></svg>',
    check: '<svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2.6" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true"><path d="M5 12.5l4.5 4.5L19 7.5"/></svg>',
    alert: '<svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2.4" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true"><path d="M12 7.5v6"/><path d="M12 17v.5"/></svg>',
    sent: '<svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true"><path d="M4 12h16m-6-6 6 6-6 6"/></svg>',
    lock: '<svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" aria-hidden="true"><rect x="5" y="11" width="14" height="10" rx="2"/><path d="M8 11V7a4 4 0 0 1 8 0v4"/></svg>',
    down: '<svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2.4" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true"><path d="M12 4v11"/><path d="m7 10.5 5 5 5-5"/><path d="M5 20h14"/></svg>',
  };

  function esc(s) {
    return String(s).replace(/[&<>"']/g, function (c) {
      return { "&": "&amp;", "<": "&lt;", ">": "&gt;", '"': "&quot;", "'": "&#39;" }[c];
    });
  }
  function kcap(k) {
    return '<span class="dov-kcap">' + esc(k) + "</span>";
  }

  /// The drawn problem lines, by the tool's problem tag (`richos_voice::dictation::Problem::tag`).
  /// Round 19 `PROBLEMS` for the first five; Iris's lines 5a and 5b for the last two.
  var PROBLEMS = {
    "did-not-catch": function (k) {
      return "I didn't catch anything. Tap " + kcap(k) + " and talk.";
    },
    "no-sound": function () {
      return "I can't hear anything. Check that your microphone is on.";
    },
    "no-text-box": function () {
      return "No text box was selected, so I copied your words. Press " + kcap("⌘V") + " to paste them.";
    },
    "no-microphone": function () {
      return "I need the microphone to hear you.";
    },
    "no-accessibility": function () {
      return "I can't type into other apps yet, so I copied your words.";
    },
    "model-missing": function () {
      return "I'm still downloading the voice AI. Try again in a few minutes.";
    },
    "could-not-write": function (k) {
      return "Sorry, I couldn't write that down. Tap " + kcap(k) + " and say it again.";
    },
    "voice-still-listening": function (k) {
      return "Voice mode is still listening, so I didn't start. Tap " + kcap(k) + " again in a moment.";
    },
  };
  /// Fix it is on the microphone and Accessibility bars only (`dictation_bar::has_fix`).
  var FIX = { "no-microphone": true, "no-accessibility": true };

  var D = { seq: 0, view: "hidden", started: 0, level: 0, target: 0, raf: 0, bars: [] };

  function look(msg) {
    if (window.RichTheme && msg && msg.theme) {
      window.RichTheme.sync({ theme: msg.theme, font_scale: msg.fontScale });
    }
  }

  function invoke(cmd, args) {
    if (tauri && tauri.core) return tauri.core.invoke(cmd, args).catch(function () {});
    return Promise.resolve();
  }

  function deliveryHtml(message) {
    var result = message.delivery || {}, app = esc(result.destination || "the destination app");
    var head = "", sub = "", tone = "is-problem", recover = false;
    if (result.state === "sent") { head = "Sent to " + app; tone = "is-neutral"; }
    else if (result.state === "secure-field") { head = "RichScribe can't dictate into a password field."; tone = "is-lock"; }
    else if (result.state === "discarded-secure") { head = "Recording discarded because the destination is a password field."; tone = "is-lock"; }
    else if (result.state === "check") {
      head = "Check " + app;
      if (result.earlierDraft) {
        head += result.copied ? ". Copied." : ". Couldn't copy.";
        sub = "An earlier draft is in Recover last draft.";
        if (!result.copied) sub += " These newest words aren't kept anywhere else.";
      } else { sub = "Your draft is in Recover last draft."; recover = !!result.recoverable; }
    } else {
      head = result.copied ? "Not added. Copied." : "Not added. Couldn't copy.";
      if (result.earlierDraft) {
        sub = "An earlier draft is in Recover last draft.";
        if (!result.copied) sub += " These newest words aren't kept anywhere else.";
      } else if (result.copied) sub = "Press " + kcap("⌘V") + " to paste your words.";
      else { sub = "Your draft is in Recover last draft."; recover = !!result.recoverable; }
    }
    if (result.replacedCopiedDraft) sub = "This replaces the previous copied draft. The earlier draft is still in Recover last draft.";
    if (result.destinationChanged) sub = "The destination changed. " + sub;
    var acts = recover ? '<span class="dhx-acts"><button class="dov-btn is-quiet" type="button" data-bar-act="recover">Recover last draft</button></span>' : "";
    var icon = tone === "is-neutral" ? IC.sent : tone === "is-lock" ? IC.lock : IC.alert;
    return '<div class="dh dhm ' + tone + '"><span class="dh-orb">' + icon + '</span><span class="dhm-text" role="status" aria-live="polite" aria-atomic="true"><span class="dhm-head">' + head + '</span>' +
      (sub ? '<span class="dhm-sub">' + sub + '</span>' : "") + '</span>' + acts + (sub ? '<span class="dhm-linger" aria-hidden="true"></span>' : "") + '</div>';
  }

  function noticeClock(milliseconds) {
    clearTimeout(D.noticeTimer);
    D.noticeLeft = milliseconds;
    function start() {
      if (D.noticeLeft <= 0) return;
      D.noticeAt = performance.now();
      var seq = D.seq;
      D.noticeTimer = setTimeout(function () {
        D.noticeLeft = 0;
        if (D.seq === seq && D.view === "delivery") invoke("dictation_bar_act", {seq:seq,act:"dismiss"});
      }, D.noticeLeft);
    }
    var pill = document.querySelector("#dhud > *");
    if (!pill) return;
    pill.addEventListener("mouseenter", function () {
      clearTimeout(D.noticeTimer);
      D.noticeLeft = Math.max(0, D.noticeLeft - (performance.now() - D.noticeAt));
    });
    pill.addEventListener("mouseleave", start);
    start();
    if (pill.matches(":hover")) {
      clearTimeout(D.noticeTimer);
      D.noticeLeft = milliseconds;
    }
  }

  function updatePreview(message) {
    if (message.seq !== D.seq || D.view !== "listening") return;
    var pill = document.getElementById("dhEl");
    if (!pill) return;
    var line = document.getElementById("dhPreview");
    if (!line) {
      line = document.createElement("div"); line.id = "dhPreview"; line.className = "dhx-top";
      line.setAttribute("aria-live", "off"); pill.insertBefore(line, pill.firstChild);
    }
    line.innerHTML = message.paused ? '<span class="dhx-paused">Preview paused</span>'
      : message.text ? '<span class="dhx-tag">Preview</span><span class="dhx-words" aria-live="off">' + esc(message.text) + '</span>' : "";
    line.hidden = !message.paused && !message.text;
    report();
  }

  /// Draw one view: { seq, view, problem, key, startedMs, theme, fontScale }.
  function render(msg) {
    look(msg);
    var h = document.getElementById("dhud");
    var k = msg.key || "F1";
    var html = "";
    D.seq = msg.seq || 0;
    D.view = msg.view;
    if (msg.view === "listening") {
      D.started = msg.startedMs || Date.now();
      var warning = msg.blockedKeys ? '<p class="dhx-warn">' + IC.alert + '<span>' +
        (msg.blockedBy ? '<b>' + esc(msg.blockedBy) + '</b>' : 'Another app') + ' is hiding your keys, so ' + kcap(k) + " can't reach me.</span></p>" : "";
      var destination = msg.destination ? '<span class="dhx-dest"><span>To <b>' + esc(msg.destination) + '</b></span>' +
        (msg.vocabulary ? '<span class="dhx-dot" aria-hidden="true">·</span><span>Vocabulary: ' + esc(msg.vocabulary) + '</span>' : "") + '</span>' : "";
      html = '<div class="dh dhx is-listen" id="dhEl">' + warning +
        '<div class="dhx-row"><span class="dh-orb">' + IC.mic + '</span><span class="dh-meter" aria-hidden="true">' +
        new Array(10).join("<i></i>") + '</span><span class="dh-label" role="status" aria-live="polite">Listening</span>' +
        '<span class="dh-time" id="dhTime">0:00</span>' + destination +
        (msg.blockedKeys ? "" : '<span class="dh-hint">Tap ' + kcap(k) + ' to finish</span>') +
        '<span class="dhx-acts"><button class="dov-btn" type="button" data-bar-act="finish">Finish</button><button class="dov-btn is-quiet" type="button" data-bar-act="cancel">Cancel</button></span></div></div>';
    } else if (msg.view === "writing") {
      html = '<div class="dh is-write"><span class="dh-orb"></span><span class="dh-label">Writing it down…</span></div>';
    } else if (msg.view === "delivery") {
      html = deliveryHtml(msg);
    } else if (msg.view === "added") {
      html = '<div class="dh is-done"><span class="dh-orb">' + IC.check + '</span><span class="dh-label">Added</span></div>';
    } else if (msg.view === "problem" && PROBLEMS[msg.problem]) {
      var fix = FIX[msg.problem] ? '<button class="dov-btn dh-fix" type="button" id="fixIt">Fix it</button>' : "";
      html =
        '<div class="dh is-problem" data-problem="' + esc(msg.problem) + '"><span class="dh-orb">' +
        (msg.problem === "model-missing" ? IC.down : IC.alert) +
        '</span><span class="dh-label is-plain">' + PROBLEMS[msg.problem](k) + "</span>" + fix + "</div>";
    } else if (msg.view === "off") {
      html = '<div class="dov-notice">RichScribe is off.</div>';
    }
    clearTimeout(D.noticeTimer);
    h.innerHTML = html;
    h.querySelectorAll(".dh-label, .dov-notice").forEach(function (label) { label.setAttribute("role", "status"); });
    if (msg.view === "listening") updatePreview({seq:D.seq,text:msg.preview,paused:msg.previewPaused});
    if (msg.view === "delivery") noticeClock(h.querySelector(".dhm-sub") ? 12000 : msg.delivery && msg.delivery.state === "sent" ? 1500 : 4200);
    D.bars = Array.prototype.slice.call(h.querySelectorAll(".dh-meter i"));
    var fixBtn = document.getElementById("fixIt");
    if (fixBtn) {
      fixBtn.addEventListener("click", function () {
        invoke("dictation_fix_it", {});
      });
    }
    cancelAnimationFrame(D.raf);
    if (msg.view === "listening") D.raf = requestAnimationFrame(tick);
    // AT ONCE, never from an animation frame: the tool shows this window only after the report,
    // and WebKit runs no animation frame in a window that is not on screen (walk-d25aed81eaa1:
    // the bar never appeared). `layout()` reads offsets, which lay the page out synchronously.
    if (html) report();
    return layout();
  }

  /// Where `el` sits inside `#dhud`, from layout and never from the screen: the pill rises in
  /// with a transform, and a box read mid-rise is the wrong size for the window.
  function within(el) {
    var x = 0, y = 0;
    for (var e = el; e && e.id !== "dhud"; e = e.offsetParent) {
      x += e.offsetLeft;
      y += e.offsetTop;
    }
    return { x: x, y: y, w: el.offsetWidth, h: el.offsetHeight };
  }

  /// The window this view needs, the orb's center and Fix it's box, all in window points.
  function layout() {
    // This page lives in a content-sized native window. Its old viewport is
    // not the desktop bound or a small result could permanently shrink it.
    var width = window.screen.availWidth || window.screen.width || 1024;
    document.documentElement.style.setProperty("--dictation-screen-width", width + "px");
    var pill = document.querySelector("#dhud > *");
    if (!pill) return null;
    var out = {
      seq: D.seq,
      w: Math.ceil(pill.offsetWidth + PAD_X * 2),
      h: Math.ceil(pill.offsetHeight + PAD_TOP + PAD_BOTTOM),
      orbX: PAD_X + pill.offsetHeight / 2,
      orbY: PAD_TOP + pill.offsetHeight / 2,
      fix: null,
    };
    var orb = pill.querySelector(".dh-orb");
    if (orb) {
      var o = within(orb);
      out.orbX = PAD_X + o.x + o.w / 2;
      out.orbY = PAD_TOP + o.y + o.h / 2;
    }
    var fix = document.getElementById("fixIt");
    if (fix) {
      var f = within(fix);
      out.fix = { x: PAD_X + f.x, y: PAD_TOP + f.y, w: f.w, h: f.h };
    }
    return out;
  }

  function report() {
    var l = layout();
    if (l) invoke("dictation_bar_laid_out", l);
  }

  /// The meter (round 19 `meterTick`): the tool's level, at most 30 a second, eased here.
  function tick(now) {
    D.level += (D.target - D.level) * (D.target > D.level ? 0.55 : 0.14);
    var L = D.level;
    var W = [0.42, 0.62, 0.8, 0.93, 1, 0.93, 0.8, 0.62, 0.42];
    D.bars.forEach(function (b, i) {
      var j = 0.82 + 0.18 * Math.sin(now / 90 + i * 1.9);
      b.style.transform = "scaleY(" + Math.max(0.12, Math.min(1, L * W[i] * j * 1.15)) + ")";
    });
    var el = document.getElementById("dhEl");
    if (el) el.style.setProperty("--lvl", L.toFixed(3));
    var tm = document.getElementById("dhTime");
    if (tm) {
      var sec = Math.max(0, Math.floor((Date.now() - D.started) / 1000));
      tm.textContent = Math.floor(sec / 60) + ":" + String(sec % 60).padStart(2, "0");
    }
    D.raf = requestAnimationFrame(tick);
  }

  function level(l) {
    D.target = Math.max(0, Math.min(1, Number(l) || 0));
  }

  /// The words' flight and their light (round 19 `comet` and `.dict-new`):
  /// { from: [x, y], to: { x, y, w, h }, theme, fontScale }, in this window's points.
  function fly(msg) {
    look(msg);
    var host = document.getElementById("flight");
    host.innerHTML = "";
    var to = msg.to;
    var light = document.createElement("div");
    light.className = "dov-light";
    light.style.left = to.x - 2 + "px";
    light.style.top = to.y + "px";
    light.style.width = to.w + 4 + "px";
    light.style.height = to.h + "px";
    var reduced = window.matchMedia && matchMedia("(prefers-reduced-motion: reduce)").matches;
    if (reduced) {
      host.appendChild(light);
      return { reduced: true, light: light };
    }
    var x0 = msg.from[0], y0 = msg.from[1], x1 = to.x + 6, y1 = to.y + to.h / 2;
    var cx = (x0 + x1) / 2 + (x1 > x0 ? -90 : 90), cy = Math.min(y0, y1) - 70;
    var N = 16, dots = [];
    for (var i = 0; i < N; i++) {
      var d = document.createElement("i");
      d.className = "dov-comet";
      d.style.opacity = String(1 - i / N);
      host.appendChild(d);
      dots.push(d);
    }
    var t0 = performance.now(), dur = 560, landed = false;
    (function step(now) {
      var k = (now - t0) / dur;
      dots.forEach(function (d, i) {
        var kk = Math.max(0, Math.min(1, k - i * 0.016)), e = 1 - Math.pow(1 - kk, 3);
        var x = (1 - e) * (1 - e) * x0 + 2 * (1 - e) * e * cx + e * e * x1;
        var y = (1 - e) * (1 - e) * y0 + 2 * (1 - e) * e * cy + e * e * y1;
        d.style.transform = "translate(" + x + "px," + y + "px) scale(" + (1 - i * 0.05) + ")";
      });
      if (k >= 0.92 && !landed) {
        landed = true;
        host.appendChild(light);
      }
      if (k < 1 + N * 0.016) requestAnimationFrame(step);
      else dots.forEach(function (d) { d.remove(); });
    })(t0);
    return { reduced: false, light: light };
  }

  document.addEventListener("click", function (event) {
    var button = event.target.closest && event.target.closest("[data-bar-act]");
    if (button) invoke("dictation_bar_act", {seq:D.seq,act:button.dataset.barAct});
  });

  if (tauri && tauri.event) {
    // Ready only once every listener is registered, so nothing the tool says first is lost.
    var heard = ROLE === "bar"
      ? [
          tauri.event.listen("dictation-bar", function (e) { render(e.payload); }),
          tauri.event.listen("dictation-level", function (e) { level(e.payload.level); }),
          tauri.event.listen("dictation-live-preview", function (e) { updatePreview(e.payload); }),
        ]
      : [tauri.event.listen("dictation-flight", function (e) { fly(e.payload); })];
    Promise.all(heard).then(function () {
      invoke("dictation_page_ready", { role: ROLE });
    });
  }

  return { ROLE: ROLE, PROBLEMS: PROBLEMS, render: render, layout: layout, level: level, fly: fly, updatePreview: updatePreview };
})();

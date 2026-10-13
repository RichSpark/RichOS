// MEETING NOTES' FLOATING PANEL (Granola PRD V4; the CEO's chosen design, richos-hq
// design/mockups/rounds/round-25.1, `#fl` and its script, ported as drawn).
//
// Rust (`src-tauri/src/meetings.rs`) owns the state and says what the panel shows; this page
// draws it, measures it and reports the window it needs (the panel's box plus room for its
// shadow), and Rust keeps the panel's right edge where it was on the display.
//
//   prompt    "Call detected in Zoom." / a calendar title with "Starts in 1 minute.", Take Notes
//             and Not this one; the hairline sweeps until an answer (it never expires).
//   bar       Not this one (or Escape) puts the prompt aside into the 8px voice bar: pointing at
//             its main part for 200 ms brings the card back, its grip drags it, its × closes the
//             prompt for good (that is the decline Rust records).
//   starting  "Starting…" until the capture says it is recording.
//   recording the breathing dot, "Recording meeting", the time and Stop; it folds after five
//             seconds and opens while pointed at or reached with Tab.
//   problem   a sentence, a reason and its actions (permission, nothing recorded), beside the call.
//
// THE VOICE BAR'S LEVEL follows the call's sound (the CEO, 2026-10-13, answering
// esc-20261013T090538Z-8defdf42: "move with the call's sound as designed"). Rust reads it live
// through the system-audio tap (`richos-companion level`), only while the bar is up and only once a
// capture has shown he granted that access; nothing is recorded or kept. Its `meeting-level` events
// (rms, about 20 a second) drive the round's own motion: a quick attack and a slower release for
// the gold level, a slower glow behind it. Before the grant no event comes and the bar rests at the
// round's resting height (6%).
"use strict";

window.RichMeetingPanel = (function () {
  var tauri = window.__TAURI__ || null;
  var fl = document.getElementById("fl");
  var PAD = { top: 6, right: 10, bottom: 40, left: 24 };   // room for the panel's shadow
  var state = { view: "hidden" };
  var aside = false, hoverTimer = null, foldTimer = null, clock = null;
  var SVG_MARK = '<svg class="rail-logo" viewBox="0 0 744 744" aria-hidden="true"><use href="#lgmark"/></svg>';

  function invoke(cmd, args) {
    if (tauri && tauri.core) return tauri.core.invoke(cmd, args || {}).catch(function () {});
    return Promise.resolve();
  }
  function esc(s) {
    return String(s == null ? "" : s).replace(/[&<>"']/g, function (c) {
      return { "&": "&amp;", "<": "&lt;", ">": "&gt;", '"': "&quot;", "'": "&#39;" }[c];
    });
  }
  function fmt(s) { var m = Math.floor(s / 60), r = s % 60; return m + ":" + (r < 10 ? "0" : "") + r; }

  /// Tell Rust the window this panel needs, after layout (and again after a fold settles).
  function report() {
    requestAnimationFrame(function () {
      if (fl.hidden) { invoke("meeting_panel_laid_out", { w: 0, h: 0, aside: false }); return; }
      var r = fl.getBoundingClientRect();
      invoke("meeting_panel_laid_out", {
        w: Math.ceil(r.width) + PAD.left + PAD.right,
        h: Math.ceil(r.height) + PAD.top + PAD.bottom,
        aside: aside,
      });
    });
  }
  fl.addEventListener("transitionend", function (e) { if (e.target === fl) report(); });

  function bump() { fl.classList.remove("swap"); void fl.offsetWidth; fl.classList.add("swap"); }
  function panel(cls, html, label) {
    stopFold(); aside = false;
    fl.removeAttribute("style");
    fl.hidden = false; fl.className = "fl" + (cls ? " " + cls : ""); fl.innerHTML = html;
    fl.setAttribute("aria-label", label); bump(); report();
  }
  function btns(list) {
    return '<div class="fl-actions">' + list.map(function (a) {
      return '<button type="button" class="desk-btn' + (a.primary ? " desk-btn--primary" : "") + '" data-act="' + esc(a.act) + '">' + esc(a.label) + "</button>";
    }).join("") + "</div>";
  }
  function card(title, sub, actions, life) {
    panel("", '<div class="fl-body"><div class="fl-head">' + SVG_MARK + '<span class="fl-app">RichOS</span></div>' +
      '<p class="fl-title">' + esc(title) + "</p>" + (sub ? '<p class="fl-sub">' + esc(sub) + "</p>" : "") + btns(actions) + "</div>" +
      (life ? '<span class="fl-life" aria-hidden="true"><i></i></span>' : ""), "RichOS: " + title);
  }
  function recPanel(starting, note) {
    panel("is-rec" + (starting ? " is-starting" : "") + (note && !starting ? " has-note" : ""),
      '<div class="fl-rec"><span class="fl-rec-dot" aria-hidden="true"></span>' +
      '<span class="fl-rec-fold"><span class="fl-rec-label">' + (starting ? "Starting…" : "Recording meeting") + "</span></span>" +
      (starting ? "" : '<span class="fl-rec-time" id="flTime">0:00</span>') +
      '<button type="button" class="desk-btn fl-stop" data-act="stop"><span class="fl-stop-word"><span>Stop</span></span><span class="fl-stop-sq" aria-hidden="true"></span></button></div>' +
      (note && !starting ? '<div class="fl-note-fold"><div><p class="fl-note">' + esc(note) + "</p></div></div>" : ""),
      starting ? "RichOS: starting meeting notes" : "RichOS: recording meeting");
  }

  // ---- the recording's clock and fold ----
  function stopClock() { clearInterval(clock); clock = null; }
  function startClock(startedAt) {
    stopClock();
    var paint = function () { var t = document.getElementById("flTime"); if (t) t.textContent = fmt(Math.max(0, Math.floor((Date.now() - startedAt) / 1000))); };
    paint(); clock = setInterval(paint, 1000);
  }
  function stopFold() { clearTimeout(foldTimer); foldTimer = null; }
  function armFold(ms) {
    stopFold(); if (state.view !== "recording") return;
    foldTimer = setTimeout(function () {
      foldTimer = null;
      if (state.view !== "recording" || fl.matches(":hover") || fl.matches(":focus-within")) return;
      fl.classList.add("is-compact"); report();
    }, ms);
  }
  function openFold() { if (state.view !== "recording") return; stopFold(); if (fl.classList.contains("is-compact")) { fl.classList.remove("is-compact"); report(); } }
  fl.addEventListener("pointerenter", openFold);
  fl.addEventListener("pointerleave", function () { if (!fl.matches(":focus-within")) armFold(5000); });
  fl.addEventListener("focusin", openFold);
  fl.addEventListener("focusout", function (e) { if (!fl.contains(e.relatedTarget) && !fl.matches(":hover")) armFold(5000); });

  // ---- the voice bar ----
  // ---- the bar's level: the call's sound, smoothed as round 25.1's `vbStep` smooths it ----
  var meter = { want: 0, lvl: 0, ghost: 0, raf: 0 };
  /// rms (0..1, linear) to the column's 0..1: -50 dBFS and below rests, -10 dBFS fills it.
  function levelOf(rms) {
    if (!(rms > 0)) return 0;
    var db = 20 * Math.log10(rms);
    return Math.max(0, Math.min(1, (db + 50) / 40));
  }
  function meterStep() {
    var lvlEl = fl.querySelector(".fl-vb-lvl"), ghostEl = fl.querySelector(".fl-vb-ghost");
    if (!aside || !lvlEl) { meter.raf = 0; meter.lvl = meter.ghost = meter.want = 0; return; }
    meter.lvl += (meter.want - meter.lvl) * (meter.want > meter.lvl ? 0.38 : 0.11);
    meter.ghost += (Math.max(meter.want, meter.lvl) - meter.ghost) * (meter.want > meter.ghost ? 0.22 : 0.045);
    lvlEl.style.height = (6 + meter.lvl * 94).toFixed(1) + "%";
    ghostEl.style.height = (6 + meter.ghost * 94).toFixed(1) + "%";
    meter.raf = requestAnimationFrame(meterStep);
  }
  function level(payload) {
    if (!aside || !payload) return;
    meter.want = levelOf(Number(payload.rms));
    if (!meter.raf) meter.raf = requestAnimationFrame(meterStep);
  }

  function putAside() {
    if (fl.hidden || aside || state.view !== "prompt" && state.view !== "problem") return;
    var r = fl.getBoundingClientRect();
    fl.style.width = r.width + "px"; fl.style.height = r.height + "px";
    fl.classList.add("is-folding"); void fl.offsetWidth;
    var vb = document.createElement("div");
    vb.className = "fl-vb";
    vb.innerHTML = '<button type="button" class="fl-vb-x" data-act="close" aria-label="Close this prompt for good" title="Close for good"><svg viewBox="0 0 8 8" aria-hidden="true"><path d="M1.6 1.6 6.4 6.4M6.4 1.6 1.6 6.4" fill="none" stroke="currentColor" stroke-width="1.3" stroke-linecap="round"/></svg></button>' +
      '<button type="button" class="fl-vb-main" data-act="back" aria-label="Meeting prompt, set aside. Bring it back"><i class="fl-vb-ghost"></i><i class="fl-vb-lvl"></i></button>' +
      '<span class="fl-vb-grip" aria-hidden="true" title="Drag to move"><i></i><i></i><i></i></span>';
    fl.appendChild(vb);
    aside = true;
    fl.setAttribute("aria-label", "RichOS meeting prompt, set aside");
    fl.classList.add("is-bar");
    fl.style.width = "8px";
    var main = vb.querySelector(".fl-vb-main");
    main.addEventListener("pointerenter", function () { clearTimeout(hoverTimer); hoverTimer = setTimeout(function () { hoverTimer = null; bringBack(); }, 200); });
    main.addEventListener("pointerleave", function () { clearTimeout(hoverTimer); hoverTimer = null; });
    vb.querySelector(".fl-vb-grip").addEventListener("pointerdown", function (e) {
      if (e.button !== 0) return;
      e.preventDefault();
      invoke("meeting_panel_drag");   // the window itself moves; Rust reads where it went
    });
    report();
  }
  function bringBack() {
    if (!aside) return;
    clearTimeout(hoverTimer); hoverTimer = null;
    aside = false;
    fl.classList.remove("is-bar");
    fl.style.width = "380px";
    fl.setAttribute("aria-label", "RichOS meeting prompt");
    var vb = fl.querySelector(".fl-vb");
    setTimeout(function () { fl.classList.remove("is-folding"); fl.style.width = ""; fl.style.height = ""; if (vb) vb.remove(); report(); }, 600);
    report();
  }

  // ---- what Rust says ----
  function render(next) {
    var prev = state; state = next || { view: "hidden" };
    if (state.view !== "recording") stopClock();
    switch (state.view) {
      case "prompt":
        if (prev.view === "prompt" && prev.title === state.title && prev.sub === state.sub) return;   // the same prompt: keep the bar
        card(state.title, state.sub, [{ label: "Take Notes", act: "take", primary: true }, { label: "Not this one", act: "aside" }], true);
        break;
      case "starting":
        recPanel(true, null);
        break;
      case "recording":
        if (prev.view !== "recording" || prev.note !== state.note) { recPanel(false, state.note); armFold(5000); }
        startClock(state.startedAt);
        break;
      case "problem":
        card(state.title, state.sub, state.actions || [], false);
        break;
      default:
        stopFold(); aside = false;
        if (fl.hidden) return;
        fl.classList.add("leaving");
        setTimeout(function () { if (state.view === "hidden") { fl.hidden = true; fl.className = "fl"; fl.innerHTML = ""; fl.removeAttribute("style"); report(); } }, 360);
    }
  }

  fl.addEventListener("click", function (e) {
    var b = e.target.closest("[data-act]"); if (!b) return;
    var act = b.dataset.act;
    if (act === "aside") return putAside();
    if (act === "back") return bringBack();
    if (act === "close") return invoke("meeting_answer", { answer: "not-this-one" });
    invoke("meeting_answer", { answer: act });   // take, stop, retry, settings, enable
  });
  document.addEventListener("keydown", function (e) {
    // Escape does what Not this one does (round 25.1): the bar, never a dismissal.
    if (e.key === "Escape" && fl.contains(document.activeElement)) putAside();
  });

  if (tauri && tauri.event) {
    tauri.event.listen("meeting-panel", function (e) { render(e.payload); });
    tauri.event.listen("meeting-level", function (e) { level(e.payload); });
    invoke("meeting_panel_state").then(function (s) { if (s) render(s); });
  }
  return { render: render, putAside: putAside, bringBack: bringBack, levelOf: levelOf };
})();

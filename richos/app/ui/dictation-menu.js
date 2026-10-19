// THE DICTATION MENU BAR MENU (dictation plan rev 2, slice 3; round 19 state 17, `renderSb`,
// and Iris's line 4 in `dictation-more-lines.html`, `secure-bar` and `secure-bar-anon`).
//
// The dictation tool (`src-tauri/src/dictation/bar.rs`) opens it under its menu bar item and
// says what it holds; every row is sent back to the tool, which does what it says. Escape
// closes it, and so does a click anywhere else (the tool hears the menu lose the keyboard).
"use strict";

window.RichDictationMenu = (function () {
  var tauri = window.__TAURI__ || null;
  var IC = {
    mic: '<svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2.4" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true"><rect x="9" y="3" width="6" height="11" rx="3"/><path d="M5 11a7 7 0 0 0 14 0"/><path d="M12 18v3"/></svg>',
    check: '<svg class="sb-check" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2.6" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true"><path d="M5 12.5l4.5 4.5L19 7.5"/></svg>',
    lock: '<svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true"><rect x="5" y="11" width="14" height="10" rx="2"/><path d="M8 11V7a4 4 0 0 1 8 0v4"/></svg>',
  };
  // Room around the menu for its shadow (the window is the page; bar.rs hangs it).
  var PAD_X = 8;
  var PAD_BOTTOM = 36;
  var seq = 0;

  function esc(s) {
    return String(s).replace(/[&<>"']/g, function (c) {
      return { "&": "&amp;", "<": "&lt;", ">": "&gt;", '"': "&quot;", "'": "&#39;" }[c];
    });
  }
  function kcap(k) {
    return '<span class="dov-kcap">' + esc(k) + "</span>";
  }
  function invoke(cmd, args) {
    if (tauri && tauri.core) return tauri.core.invoke(cmd, args).catch(function () {});
    return Promise.resolve();
  }

  /// Iris's line 4, word for word: with the app's name when macOS gives it, otherwise without.
  function secureLine(k, app) {
    if (app) {
      return '<b>' + esc(app) + "</b> is hiding your keys from other apps, so " + kcap(k) +
        " can't reach me. Quitting " + esc(app) + " fixes it.";
    }
    return "Another app is hiding your keys, so " + kcap(k) +
      " can't reach me. Quitting the app where you last typed a password usually fixes it.";
  }

  function item(act, label, value, checked, radio) {
    return '<button class="sb-item" type="button" role="' + (radio ? "menuitemradio" : "menuitem") + '"' +
      (radio ? ' aria-checked="' + (checked ? "true" : "false") + '"' : "") +
      ' data-act="' + act + '"' + (value ? ' data-v="' + value + '"' : "") + ">" +
      (checked ? IC.check : '<span class="sb-check"></span>') + label + "</button>";
  }

  /// Draw the menu: { seq, on, key, choice: "accurate"|"fast", paused, secureApp, theme, fontScale }.
  function render(msg) {
    if (window.RichTheme && msg.theme) window.RichTheme.sync({ theme: msg.theme, font_scale: msg.fontScale });
    seq = msg.seq || 0;
    var k = msg.key || "F1";
    var paused = msg.on && msg.paused;
    var m = document.getElementById("sbMenu");
    m.innerHTML =
      '<div class="sb-head"><span class="sb-name">' + IC.mic + 'RichScribe</span><span class="sb-on' + (msg.on ? "" : " is-off") + '">' +
      (paused ? "Paused" : msg.on ? "On" : "Off") + "</span></div>" +
      (paused
        ? '<p class="sb-warn" role="status">' + IC.lock + "<span>" + secureLine(k, msg.secureApp) + "</span></p>"
        : '<p class="sb-say">' + (msg.on ? "Tap " + kcap(k) + " to start, and again to stop." : "Turn it on to type with your voice.") + "</p>") +
      '<div class="sb-hr" role="separator"></div>' +
      item("mode", "More accurate", "accurate", msg.choice !== "fast", true) +
      item("mode", "Faster", "fast", msg.choice === "fast", true) +
      '<div class="sb-hr" role="separator"></div>' +
      item("settings", "RichScribe settings…") +
      item("onoff", msg.on ? "Turn RichScribe off" : "Turn RichScribe on") +
      '<div class="sb-hr" role="separator"></div>' +
      item("open", "Open RichOS");
    // At once, never from an animation frame: this window is hidden until the report arrives,
    // and a hidden window gets no animation frames (see dictation-bar.js).
    report();
    return size();
  }

  function size() {
    var m = document.getElementById("sbMenu");
    var r = m.getBoundingClientRect();
    return { seq: seq, w: Math.ceil(r.width + PAD_X * 2), h: Math.ceil(r.height + PAD_BOTTOM) };
  }

  function report() {
    invoke("dictation_menu_laid_out", size());
    var first = document.querySelector(".sb-item");
    if (first) first.focus({ preventScroll: true });
  }

  document.addEventListener("click", function (e) {
    var b = e.target.closest && e.target.closest(".sb-item");
    if (!b) return;
    invoke("dictation_menu_act", { act: b.dataset.act, value: b.dataset.v || null });
  });
  document.addEventListener("keydown", function (e) {
    var items = Array.prototype.slice.call(document.querySelectorAll(".sb-item"));
    var at = items.indexOf(document.activeElement);
    if (e.key === "Escape") {
      e.preventDefault();
      invoke("dictation_menu_act", { act: "close", value: null });
    } else if (e.key === "ArrowDown" || e.key === "ArrowUp") {
      e.preventDefault();
      var next = e.key === "ArrowDown" ? (at + 1) % items.length : (at - 1 + items.length) % items.length;
      if (items[next]) items[next].focus();
    }
  });

  if (tauri && tauri.event) {
    tauri.event.listen("dictation-menu", function (e) { render(e.payload); }).then(function () {
      invoke("dictation_page_ready", { role: "menu" });
    });
  }

  return { render: render, size: size, secureLine: secureLine };
})();

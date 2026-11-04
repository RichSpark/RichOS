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
    close: '<svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2.2" stroke-linecap="round" aria-hidden="true"><path d="M6 6l12 12M18 6L6 18"/></svg>',
    draft: '<svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true"><path d="M3 12a9 9 0 1 0 3-6.7"/><path d="M3 4v5h5"/><path d="M12 8v4l3 2"/></svg>',
  };
  // Room around the menu for its shadow (the window is the page; bar.rs hangs it).
  var PAD_X = 8;
  var PAD_BOTTOM = 36;
  var seq = 0;
  var expiryTimer = 0;
  var recovering = false;
  var expanded = false;

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
      ' data-act="' + act + '"' + (value != null ? ' data-v="' + esc(value) + '"' : "") + ">" +
      (checked ? IC.check : '<span class="sb-check"></span>') + label + "</button>";
  }

  /// Draw the menu: { seq, on, key, choice: "accurate"|"fast", paused, secureApp, theme, fontScale }.
  function render(msg) {
    clearTimeout(expiryTimer);
    var m = document.getElementById("sbMenu");
    if (msg.closed) {
      m.replaceChildren();
      recovering = false;
      return size();
    }
    if (window.RichTheme && msg.theme) window.RichTheme.sync({ theme: msg.theme, font_scale: msg.fontScale });
    seq = msg.seq || 0;
    recovering = !!msg.recovery;
    expanded = !!msg.vocabularyExpanded;
    m.className = recovering ? "rpanel recovery-panel" : "sbmenu";
    m.setAttribute("role", recovering ? "dialog" : "menu");
    m.setAttribute("aria-label", recovering ? "Recover last draft" : "RichScribe");
    var focused = document.activeElement && document.activeElement.dataset;
    var focusAct = focused && focused.act, focusValue = focused && focused.v;
    if (recovering) {
      m.innerHTML = recoveryHtml(msg.recovery);
      var expiry = msg.recovery.expiresMs;
      if (msg.recovery.text != null && expiry != null) {
        var current = seq;
        expiryTimer = setTimeout(function () {
          if (seq !== current || !recovering) return;
          m.innerHTML = recoveryHtml({status:"expired"});
          report();
        }, Math.max(0, expiry - Date.now()));
      }
      report(focusAct, focusValue);
      return size();
    }
    var k = msg.key || "F1";
    var paused = msg.on && msg.paused;
    var companies = msg.companies || [];
    var company = companies.find(function (c) { return c.id === msg.vocabularyCompany; });
    var vocabulary = '<button class="sb-item" type="button" role="menuitem" aria-haspopup="true" aria-expanded="' + expanded + '" data-act="' + (expanded ? "vocabulary-collapse" : "vocabulary-expand") + '"><span class="sb-check"></span><span class="sb-grow">Vocabulary: <span class="sb-val">' + esc(company ? company.name : "None") + '</span></span><span class="sb-chev" aria-hidden="true">›</span></button>';
    if (expanded) {
      [{id:"",name:"None"}].concat(companies).forEach(function (c) {
        vocabulary += item("vocabulary", esc(c.name), c.id, (msg.vocabularyCompany || "") === c.id, true).replace('class="sb-item"', 'class="sb-item is-sub"');
      });
      if (msg.recording) vocabulary += '<p class="sb-mid" role="status">Changes apply to your next recording.</p>';
    }
    if (msg.vocabularyError) vocabulary += '<p class="sb-mid" role="status">' + esc(msg.vocabularyError) + '</p>';
    var recover = msg.recoverAvailable ? item("recover", "Recover last draft…") : '<button class="sb-item is-unavail" type="button" role="menuitem" aria-disabled="true"><span class="sb-check"></span>Recover last draft<span class="sb-aside">' + (msg.recording ? "After recording" : "No recent draft") + '</span></button>';
    m.innerHTML =
      '<div class="sb-head"><span class="sb-name">' + IC.mic + 'RichScribe</span><span class="sb-on' + (msg.on ? "" : " is-off") + '">' +
      (paused ? "Paused" : msg.on ? "On" : "Off") + "</span></div>" +
      (paused
        ? '<p class="sb-warn" role="status">' + IC.lock + "<span>" + secureLine(k, msg.secureApp) + "</span></p>"
        : '<p class="sb-say">' + (msg.on ? "Tap " + kcap(k) + " to start and again to stop." : "Turn it on to type with your voice.") + "</p>") +
      '<div class="sb-hr" role="separator"></div>' +
      item("mode", "More accurate", "accurate", msg.choice !== "fast", true) +
      item("mode", "Faster", "fast", msg.choice === "fast", true) +
      '<div class="sb-hr" role="separator"></div>' +
      vocabulary + '<div class="sb-hr" role="separator"></div>' + recover +
      '<div class="sb-hr" role="separator"></div>' +
      item("settings", "RichScribe settings…") +
      item("onoff", msg.on ? "Turn RichScribe off" : "Turn RichScribe on") +
      '<div class="sb-hr" role="separator"></div>' +
      item("open", "Open RichOS");
    // At once, never from an animation frame: this window is hidden until the report arrives,
    // and a hidden window gets no animation frames (see dictation-bar.js).
    report(focusAct, focusValue);
    return size();
  }

  function time(ms) {
    return new Date(ms).toLocaleTimeString("en", {hour:"numeric",minute:"2-digit"});
  }
  function recoveryHtml(draft) {
    var copy = draft.status === "copied", text = draft.text;
    var meta = text != null ? "For " + esc(draft.destination || "the destination app") + " · Finished at " + esc(time(draft.completedMs)) : "";
    var html = '<div class="rp-head"><span class="rp-ic">' + IC.draft + '</span><div><h2 class="rp-title">Recover last draft</h2><p class="rp-meta">' + meta + '</p></div><button class="rp-x" type="button" aria-label="Close" data-act="close">' + IC.close + '</button></div><div class="rp-body">';
    if (text == null) {
      html += '<p class="rp-expired" role="status">' + (copy ? "Copied." : "This draft has expired.") + '</p>';
    } else {
      html += '<p class="rp-text" tabindex="0" aria-label="Draft text">' + esc(text) + '</p><p class="rp-life">This draft stays in memory until ' + esc(time(draft.expiresMs)) + ' or until you quit RichOS. It isn\'t saved after you quit.</p>';
      if (draft.status === "copy-failed") html += '<p class="rp-status is-bad" role="alert">Couldn\'t copy. Your draft is still here.</p>';
    }
    return html + '</div><div class="rp-acts">' + (text != null ? '<button class="dov-btn is-quiet" type="button" data-act="recovery-clear">Clear</button>' : '') + '<span class="rp-spacer"></span><button class="dov-btn is-quiet" type="button" data-act="close">Close</button>' + (text != null ? '<button class="dov-btn" type="button" data-act="recovery-copy">Copy</button>' : '') + '</div>';
  }

  function size() {
    // Recovery must grow from the menu's previous small native viewport.
    var width = window.screen.availWidth || window.screen.width || 1024;
    document.documentElement.style.setProperty("--dictation-screen-width", width + "px");
    var m = document.getElementById("sbMenu");
    var r = m.getBoundingClientRect();
    return { seq: seq, w: Math.ceil(r.width + PAD_X * 2), h: Math.ceil(r.height + PAD_BOTTOM) };
  }

  function report(act, value) {
    invoke("dictation_menu_laid_out", size());
    var first = Array.prototype.find.call(document.querySelectorAll("[data-act]"), function (b) {
      return b.dataset.act === act && b.dataset.v === value;
    }) || document.querySelector(recovering ? ".rp-text, .rp-acts button" : ".sb-item");
    if (first) first.focus({ preventScroll: true });
  }

  document.addEventListener("click", function (e) {
    var b = e.target.closest && e.target.closest("[data-act]");
    if (!b || b.getAttribute("aria-disabled") === "true") return;
    if (b.dataset.act === "recovery-copy") b.disabled = true;
    invoke("dictation_menu_act", { act: b.dataset.act, value: b.dataset.v || null });
  });
  document.addEventListener("keydown", function (e) {
    var items = Array.prototype.slice.call(document.querySelectorAll(".sb-item"));
    var at = items.indexOf(document.activeElement);
    if (e.key === "Escape") {
      e.preventDefault();
      invoke("dictation_menu_act", { act: "close", value: null });
    } else if (!recovering && (e.key === "ArrowRight" || e.key === "ArrowLeft")) {
      if (e.key === "ArrowRight" || expanded) {
        e.preventDefault();
        invoke("dictation_menu_act", {act:e.key === "ArrowRight" ? "vocabulary-expand" : "vocabulary-collapse",value:null});
      }
    } else if (!recovering && (e.key === "ArrowDown" || e.key === "ArrowUp")) {
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

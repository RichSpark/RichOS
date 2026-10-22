"use strict";
// =======================================================================================
// IMPROVED RICH AND STU INSTRUCTIONS: THE QUESTION, THE UPDATE, THE BEFORE AND AFTER
// =======================================================================================
//
// The CEO, 2026-10-22: when a new version ships improved instructions and the user had changed
// their own `rich.md`/`stu.md`, the app asks once, in his words. On OK an editor rewrites the
// file, keeping everything from the RichOS update and editing out the user's instructions that
// conflict with it, "and when done, the user is presented with "before" and "after" with all
// the edits highlighted".
//
// Built as drawn from his chosen design, richos-hq design/mockups/rounds/round-37/
// updated-instructions.html (5126e1391), with the app's data in place of the mockup's example
// files. The shell decides whether to ask (`instructions_update_question`, `richos-core`'s
// `instructions_update.rs`: a copy the user never changed was already replaced, unasked), does
// the update one file at a time (`update_instructions_file`) and returns each file before and
// after as paragraphs of `[kind, text]` segments: `fm` front matter, `s` the same, `a` added from
// the RichOS update, `d` older RichOS wording taken out, `y` one of the user's own lines edited
// out (the "your line" tag). No is `decline_instructions_update`.
//
// Styles: instructions-update.css. Contrast is re-measured under WebKit, both themes, by
// ui/tests/instructions-update.js; nothing here is declared exempt.
(function () {
  const $ = (s, r) => (r || document).querySelector(s);
  const $$ = (s, r) => [].slice.call((r || document).querySelectorAll(s));
  function h(html) { const t = document.createElement("template"); t.innerHTML = html.trim(); return t.content.firstChild; }
  function esc(s) { return String(s).replace(/&/g, "&amp;").replace(/</g, "&lt;").replace(/>/g, "&gt;"); }
  // Inline code, as the file has it: the backticks stay, the span is set in the mono face.
  function md(s) { return esc(s).replace(/`([^`]+)`/g, "<code>`$1`</code>"); }
  function plural(n, one, many) { return n + " " + (n === 1 ? one : many); }
  const invoke = (name, args) => window.RichBridge.invoke(name, args || {});

  const I = {
    doc: '<svg class="iu-icon" viewBox="0 0 20 20" fill="none" stroke="currentColor" stroke-width="1.6" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true"><path d="M5 2.5h7l3.5 3.5v11.5h-10.5z"/><path d="M12 2.5V6h3.5"/><path d="M8 10h5M8 13h5"/></svg>',
    check: '<svg viewBox="0 0 20 20" fill="none" stroke="currentColor" stroke-width="2.2" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true"><path d="M4 10.5l4 4 8-9"/></svg>',
    spin: '<svg class="iu-spin" viewBox="0 0 20 20" fill="none" stroke="currentColor" stroke-width="2.2" stroke-linecap="round" aria-hidden="true"><path d="M10 3a7 7 0 1 1-7 7"/></svg>',
    wait: '<svg viewBox="0 0 20 20" fill="none" stroke="currentColor" stroke-width="1.8" stroke-linecap="round" aria-hidden="true"><circle cx="10" cy="10" r="7"/><path d="M10 6.5V10l2.5 1.5"/></svg>',
    up: '<svg viewBox="0 0 20 20" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true"><path d="M5 12.5l5-5 5 5"/></svg>',
    down: '<svg viewBox="0 0 20 20" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true"><path d="M5 7.5l5 5 5-5"/></svg>',
  };
  function mark() {
    // The RichOS mark, its two paths read from the app's own rail wordmark.
    const r = $("#rail-wordmark #R"), a = $("#rail-wordmark #arrow");
    return r && a
      ? '<svg viewBox="0 0 744 744" aria-hidden="true"><path fill="var(--ink)" d="' + r.getAttribute("d") + '"/><path fill="var(--gold)" d="' + a.getAttribute("d") + '"/></svg>'
      : "";
  }

  let asked = [];          // [{name, path}] from the shell
  let updated = [];        // [{name, path, rows, edits, said}] as each file is done
  let qOverlay = null, rOverlay = null, returnFocus = null;
  let current = null, edits = [], at = -1;
  const timers = [];
  function later(fn, ms) { timers.push(setTimeout(fn, ms)); }

  // ------------------------------------------------------------------ 1. the question --------
  function buildQuestion() {
    qOverlay = h(
      '<div class="overlay iu-overlay" id="instructions-update" hidden role="dialog" aria-modal="true" aria-labelledby="iu-q-text" data-dismiss="control:#iu-no">' +
        '<div class="overlay-panel iu-q" data-s="ask">' +
          '<div class="iu-q-ask">' +
            '<div class="iu-q-mark">' + mark() + "</div>" +
            '<p class="iu-q-text" id="iu-q-text">You previously modified Rich’s instructions (<code>rich.md</code> and/or <code>stu.md</code>), but the new RichOS version has improved/updated instructions now. OK to update your modified instructions too?</p>' +
            '<ul class="iu-files" id="iu-q-files" aria-label="Your modified files"></ul>' +
            '<p class="iu-q-error" id="iu-q-error" hidden></p>' +
            '<div class="iu-q-actions"><button class="desk-btn" type="button" id="iu-no">No</button><button class="desk-btn desk-btn--confirm" type="button" id="iu-ok">OK</button></div>' +
          "</div>" +
          '<div class="iu-q-work" role="status" aria-live="polite">' +
            '<div class="iu-q-work-head">' + I.spin + '<h2 id="iu-work-title">Updating your instructions…</h2></div>' +
            '<p class="iu-q-lede">Keeping everything from the RichOS update and editing out your instructions that conflict with it.</p>' +
            '<div class="iu-q-progress" aria-hidden="true"><i></i></div>' +
            '<ul class="iu-files" id="iu-w-files"></ul>' +
            '<p class="iu-q-error" id="iu-w-error" hidden></p>' +
            '<div class="iu-q-actions" id="iu-w-actions" hidden><button class="desk-btn desk-btn--confirm" type="button" id="iu-w-close">Close</button></div>' +
          "</div>" +
        "</div>" +
      "</div>");
    document.body.appendChild(qOverlay);
    $("#iu-ok", qOverlay).addEventListener("click", runUpdate);
    $("#iu-no", qOverlay).addEventListener("click", decline);
    $("#iu-w-close", qOverlay).addEventListener("click", () => { qOverlay.hidden = true; restoreFocus(); });
    qOverlay.addEventListener("keydown", (e) => {
      // Escape on the question is No: the answer that changes nothing.
      if (e.key === "Escape" && $(".iu-q", qOverlay).dataset.s === "ask") { e.stopPropagation(); decline(); }
    });
  }

  function fileRow(file, status) {
    const st = status === "working" ? I.spin + "<span>Updating</span>"
      : status === "done" ? I.check + "<span>Done</span>"
      : status === "waiting" ? I.wait + "<span>Next</span>" : "";
    return '<li data-k="' + esc(file.name) + '" data-s="' + (status || "") + '">' + I.doc + "<code>" + esc(file.path) + "</code>" +
      (st ? '<span class="iu-file-status">' + st + "</span>" : "") + "</li>";
  }

  function show(files) {
    asked = files;
    if (!qOverlay) buildQuestion();
    $("#iu-q-files", qOverlay).innerHTML = asked.map((f) => fileRow(f)).join("");
    $("#iu-q-error", qOverlay).hidden = true;
    $(".iu-q", qOverlay).dataset.s = "ask";
    returnFocus = document.activeElement;
    qOverlay.hidden = false;
    // OK has the focus, so Return answers OK, the way a Mac alert's default button works.
    $("#iu-ok", qOverlay).focus();
  }

  function restoreFocus() { if (returnFocus && returnFocus.focus) returnFocus.focus(); returnFocus = null; }

  async function decline() {
    const no = $("#iu-no", qOverlay), ok = $("#iu-ok", qOverlay);
    no.disabled = ok.disabled = true;
    try {
      await invoke("decline_instructions_update");
      qOverlay.hidden = true;
      restoreFocus();
    } catch (error) {
      const line = $("#iu-q-error", qOverlay);
      line.textContent = "That could not be done: " + String(error);
      line.hidden = false;
    } finally {
      no.disabled = ok.disabled = false;
    }
  }

  async function runUpdate() {
    const panel = $(".iu-q", qOverlay);
    panel.dataset.s = "working";
    const bar = $(".iu-q-progress i", panel);
    $("#iu-w-error", panel).hidden = true;
    $("#iu-w-actions", panel).hidden = true;
    updated = [];
    const paint = (i, doneAll) => {
      $("#iu-w-files", panel).innerHTML = asked.map((f, j) => fileRow(f, doneAll || j < i ? "done" : j === i ? "working" : "waiting")).join("");
      bar.style.width = (doneAll ? 100 : Math.round((i + 0.45) / asked.length * 100)) + "%";
      $("#iu-work-title", panel).textContent = doneAll ? "Your instructions are updated" : "Updating your instructions…";
    };
    for (let i = 0; i < asked.length; i++) {
      paint(i, false);
      try {
        updated.push(await invoke("update_instructions_file", { name: asked[i].name }));
      } catch (error) {
        // Said, never swallowed. A file already updated stays updated; this one is asked about
        // again the next time the app starts.
        $("#iu-work-title", panel).textContent = "Your instructions could not all be updated";
        const line = $("#iu-w-error", panel);
        line.textContent = "That could not be done: " + String(error);
        line.hidden = false;
        $("#iu-w-actions", panel).hidden = false;
        $("#iu-w-close", panel).focus();
        return;
      }
    }
    paint(asked.length, true);
    later(() => openResult(true), 650);
  }

  // ------------------------------------------------------------------ 2. before and after ----
  function buildResult() {
    rOverlay = h(
      '<div class="overlay iu-overlay" id="instructions-update-result" hidden role="dialog" aria-modal="true" aria-labelledby="iu-r-title" data-dismiss="control:#iu-done">' +
        '<div class="overlay-panel iu-sheet">' +
          '<div class="iu-head">' +
            '<div class="iu-head-row"><h2 class="iu-title" id="iu-r-title">Before and after</h2>' +
              '<span class="iu-done-chip">' + I.check + "<span>Updated</span></span>" +
              '<div class="iu-tabs" role="tablist" aria-label="Your updated files"></div></div>' +
            '<p class="iu-lede">Everything from the RichOS update is in. Your instructions that conflicted with it are edited out.</p>' +
            '<div class="iu-fileline"><span>Your file,</span><code id="iu-path"></code>' +
              '<button class="iu-link" type="button" id="iu-open">Open the file</button>' +
              '<button class="iu-link" type="button" id="iu-reveal">Show in Finder</button>' +
              '<span class="iu-legend"><span><span class="iu-key iu-key--add" aria-hidden="true">+</span>Added from the RichOS update</span>' +
              '<span><span class="iu-key iu-key--del" aria-hidden="true">−</span>Edited out</span></span></div>' +
          "</div>" +
          '<div class="iu-well">' +
            '<div class="iu-scroll" tabindex="0" aria-label="Before and after, side by side">' +
              '<div class="iu-cols-head" aria-hidden="true"><div>Before <small>your file as it was</small></div><div>After <small>your file now</small></div></div>' +
              '<div class="iu-rows"></div>' +
            "</div>" +
            '<div class="iu-map" aria-label="Where the edits are"></div>' +
          "</div>" +
          '<div class="iu-foot">' +
            '<div class="iu-nav"><button class="iu-nav-btn" type="button" id="iu-prev" aria-label="Previous edit">' + I.up + "</button>" +
              '<button class="iu-nav-btn" type="button" id="iu-next" aria-label="Next edit">' + I.down + "</button>" +
              '<span class="iu-nav-label" id="iu-nav-label" aria-live="polite"></span></div>' +
            '<button class="desk-btn desk-btn--confirm" type="button" id="iu-done">Done</button>' +
          "</div>" +
        "</div>" +
      "</div>");
    document.body.appendChild(rOverlay);
    $("#iu-prev", rOverlay).addEventListener("click", () => goEdit(at < 0 ? edits.length - 1 : at - 1));
    $("#iu-next", rOverlay).addEventListener("click", () => goEdit(at + 1));
    $("#iu-done", rOverlay).addEventListener("click", closeResult);
    $("#iu-open", rOverlay).addEventListener("click", () => openFile(false));
    $("#iu-reveal", rOverlay).addEventListener("click", () => openFile(true));
    rOverlay.addEventListener("mousedown", (e) => { if (e.target === rOverlay) closeResult(); });
    // On the document, as drawn: a click on a control in WebKit does not move the focus into the
    // sheet, so a key handler on the sheet would miss Option-arrows after a click.
    document.addEventListener("keydown", (e) => {
      if (rOverlay.hidden) return;
      if (e.key === "Escape") { e.stopPropagation(); closeResult(); return; }
      if ((e.key === "ArrowDown" || e.key === "ArrowUp") && e.altKey) {
        goEdit(e.key === "ArrowDown" ? at + 1 : (at < 0 ? edits.length - 1 : at - 1));
        e.preventDefault();
      }
    });
    $(".iu-scroll", rOverlay).addEventListener("scroll", paintView);
    addEventListener("resize", () => { if (rOverlay && !rOverlay.hidden) paintMap(); });
  }

  function openFile(reveal) {
    invoke("instructions_file_open", { name: current.name, reveal }).catch((error) => {
      $("#iu-nav-label", rOverlay).textContent = "That could not be done: " + String(error);
    });
  }

  function cellHTML(row, side) {
    const parts = [];
    let hasAdd = false, hasDel = false, any = false;
    row.forEach(([k, t]) => {
      if (k === "fm") { parts.push('<span class="iu-fm">' + esc(t) + "</span>"); any = true; return; }
      if (k === "s") { parts.push(md(t)); any = true; return; }
      if (side === "after" && k === "a") { parts.push('<span class="iu-ins">' + md(t) + "</span>"); hasAdd = any = true; }
      if (side === "before" && (k === "d" || k === "y")) {
        parts.push((k === "y" ? '<span class="iu-yours">your line</span>' : "") + '<del class="iu-del">' + md(t) + "</del>");
        hasDel = any = true;
      }
    });
    const marker = hasAdd
      ? '<span class="iu-bar iu-bar--add" data-contrast-role="indicator" aria-hidden="true"></span><span class="iu-sign iu-sign--add" aria-hidden="true">+</span>'
      : hasDel ? '<span class="iu-bar iu-bar--del" data-contrast-role="indicator" aria-hidden="true"></span><span class="iu-sign iu-sign--del" aria-hidden="true">−</span>' : "";
    return { html: marker + parts.join(""), empty: !any, add: hasAdd, del: hasDel };
  }

  function paintFile(file, arriving) {
    current = file;
    const rowsEl = $(".iu-rows", rOverlay), scroller = $(".iu-scroll", rOverlay);
    $("#iu-path", rOverlay).textContent = file.path;
    const html = [];
    let n = 0;
    edits = [];
    file.rows.forEach((row, idx) => {
      const b = cellHTML(row, "before"), a = cellHTML(row, "after");
      const changed = b.del || a.add;
      const cls = (idx === 0 ? " iu-row-first" : "") + (idx === file.rows.length - 1 ? " iu-row-last" : "");
      const i = changed ? n++ : 0;
      const tag = changed ? ' data-edit="' + i + '" style="--i:' + i + '"' : "";
      html.push('<div class="iu-cell iu-before' + cls + (b.empty ? " is-empty" : "") + '" data-row="' + idx + '"' + tag + ">" + b.html + "</div>");
      html.push('<div class="iu-cell iu-after' + cls + (a.empty ? " is-empty" : "") + '" data-row="' + idx + '"' + tag + ">" + a.html + "</div>");
      if (changed) edits.push({ row: idx, add: a.add, del: b.del });
    });
    rowsEl.innerHTML = html.join("");
    rowsEl.classList.toggle("is-arriving", !!arriving);
    if (arriving) later(() => rowsEl.classList.remove("is-arriving"), 900 + edits.length * 260);
    $$(".iu-tab", rOverlay).forEach((t) => { t.setAttribute("aria-selected", String(t.dataset.k === file.name)); t.tabIndex = t.dataset.k === file.name ? 0 : -1; });
    scroller.style.scrollBehavior = "auto"; scroller.scrollTop = 0; scroller.style.scrollBehavior = "";
    at = -1; paintNav();
    requestAnimationFrame(paintMap);
  }

  function paintTabs() {
    const tabs = $(".iu-tabs", rOverlay);
    tabs.innerHTML = updated.map((f) =>
      '<button class="iu-tab' + (updated.length === 1 ? " iu-tab--static" : "") + '" type="button" role="tab" data-k="' + esc(f.name) + '"><code>' + esc(f.name) +
      '</code><span class="iu-tab-n">' + plural(f.edits, "edit", "edits") + "</span></button>").join("");
    $$(".iu-tab", tabs).forEach((t) => {
      t.addEventListener("click", () => { if (t.dataset.k !== current.name) paintFile(updated.find((f) => f.name === t.dataset.k), true); });
      t.addEventListener("keydown", (e) => {
        if (e.key !== "ArrowRight" && e.key !== "ArrowLeft") return;
        const all = $$(".iu-tab", tabs), i = all.indexOf(t), j = (i + (e.key === "ArrowRight" ? 1 : all.length - 1)) % all.length;
        all[j].focus(); all[j].click(); e.preventDefault();
      });
    });
  }

  function paintNav() {
    $("#iu-nav-label", rOverlay).textContent = at < 0
      ? plural(edits.length, "edit", "edits") + " in " + current.name
      : "Edit " + (at + 1) + " of " + edits.length;
  }
  function cellsOf(i) { return $$('.iu-cell[data-edit="' + i + '"]', $(".iu-rows", rOverlay)); }
  function goEdit(i) {
    if (!edits.length) return;
    at = (i + edits.length) % edits.length;
    $$(".iu-cell.is-current", rOverlay).forEach((c) => c.classList.remove("is-current"));
    const cells = cellsOf(at);
    cells.forEach((c) => { void c.offsetWidth; c.classList.add("is-current"); });
    const scroller = $(".iu-scroll", rOverlay);
    const top = cells[0].offsetTop - 56;
    scroller.scrollTo({ top: Math.max(0, top - (scroller.clientHeight - cells[0].offsetHeight) / 3) });
    paintNav();
  }

  function paintMap() {
    const scroller = $(".iu-scroll", rOverlay), mapEl = $(".iu-map", rOverlay);
    const total = scroller.scrollHeight, track = mapEl.clientHeight;
    if (!total || !track) return;
    const html = edits.map((e, i) => {
      const c = cellsOf(i)[0];
      const top = Math.round(c.offsetTop / total * track), ht = Math.max(6, Math.round(c.offsetHeight / total * track) - 2);
      return '<button type="button" class="' + (e.add && e.del ? "is-mix" : e.add ? "is-add" : "is-del") + '" style="top:' + top + "px;height:" + ht + 'px" data-i="' + i + '" aria-label="Edit ' + (i + 1) + '"></button>';
    });
    mapEl.innerHTML = '<span class="iu-map-view"></span>' + html.join("");
    $$("button", mapEl).forEach((b) => b.addEventListener("click", () => goEdit(+b.dataset.i)));
    paintView();
  }
  function paintView() {
    const scroller = $(".iu-scroll", rOverlay), mapEl = $(".iu-map", rOverlay), v = $(".iu-map-view", mapEl);
    if (!v) return;
    const total = scroller.scrollHeight, track = mapEl.clientHeight;
    v.style.top = Math.round(scroller.scrollTop / total * track) + "px";
    v.style.height = Math.round(scroller.clientHeight / total * track) + "px";
  }

  function openResult(grow) {
    if (!rOverlay) buildResult();
    const qPanel = $(".iu-q", qOverlay), from = qPanel.getBoundingClientRect();
    qOverlay.hidden = true;
    paintTabs();
    rOverlay.hidden = false;
    paintFile(updated[0], true);
    const sheet = $(".iu-sheet", rOverlay);
    if (grow) {
      // Grow out of the question's panel: start clipped to its rectangle.
      const to = sheet.getBoundingClientRect();
      sheet.style.setProperty("--iu-from", Math.max(0, from.top - to.top) + "px " + Math.max(0, to.right - from.right) + "px " +
        Math.max(0, to.bottom - from.bottom) + "px " + Math.max(0, from.left - to.left) + "px round 12px");
      sheet.classList.remove("is-entering"); void sheet.offsetWidth; sheet.classList.add("is-entering");
    }
    $("#iu-done", rOverlay).focus({ preventScroll: true });
    // The sheet opens on the first edit, so the first thing seen is a change, not front matter.
    later(() => goEdit(0), grow ? 420 : 60);
  }
  function closeResult() { rOverlay.hidden = true; restoreFocus(); }

  // Asked when the window opens and again when engine setup finishes (main.js `runSetup`): the
  // second ask changes nothing while the question, the update or the before and after is up.
  async function ask() {
    if ((qOverlay && !qOverlay.hidden) || (rOverlay && !rOverlay.hidden)) return;
    try {
      const files = await invoke("instructions_update_question");
      if (Array.isArray(files) && files.length) show(files);
    } catch (error) {
      // A shell without the command (the browser stand-in) asks nothing.
    }
  }

  if (window.RichBridge && window.RichBridge.invoke) ask();

  // Exported for the suite, which drives the real screens rather than a copy of them.
  window.RichInstructionsUpdate = { show, ask, goEdit, updated: () => updated };
})();

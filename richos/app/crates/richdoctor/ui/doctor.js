// RichDoctor's help window: round 39's screens, driven by the app (crates/richdoctor/src/lib.rs
// commands, and the "doctor" event the engine emits). Every text that came from a person, a log
// or Claude is set with textContent, never as markup.
(function () {
  "use strict";
  const T = window.__TAURI__;
  const invoke = function (cmd, args) { return T.core.invoke(cmd, args || {}); };
  const $ = function (id) { return document.getElementById(id); };

  // ---- theme: follows the Mac ---------------------------------------------------------------
  const mq = matchMedia("(prefers-color-scheme: light)");
  function applyTheme() { document.documentElement.setAttribute("data-theme", mq.matches ? "light" : "dark"); }
  applyTheme(); mq.addEventListener("change", applyTheme);

  // ---- glyphs: round 39's ----------------------------------------------------------------------
  const G = {
    check: '<svg viewBox="0 0 20 20" aria-hidden="true"><path fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round" d="M4 10.5l4 4 8-9"/></svg>',
    checkc: '<svg viewBox="0 0 20 20" aria-hidden="true"><circle cx="10" cy="10" r="8" fill="none" stroke="currentColor" stroke-width="1.6"/><path fill="none" stroke="currentColor" stroke-width="1.8" stroke-linecap="round" stroke-linejoin="round" d="M6.3 10.3l2.6 2.6 4.8-5.4"/></svg>',
    dash: '<svg viewBox="0 0 20 20" aria-hidden="true"><circle cx="10" cy="10" r="8" fill="none" stroke="currentColor" stroke-width="1.6"/><path stroke="currentColor" stroke-width="1.8" stroke-linecap="round" d="M6.5 10h7"/></svg>',
    quest: '<svg viewBox="0 0 20 20" aria-hidden="true"><circle cx="10" cy="10" r="8" fill="none" stroke="currentColor" stroke-width="1.6"/><path fill="none" stroke="currentColor" stroke-width="1.8" stroke-linecap="round" d="M7.6 8a2.4 2.4 0 1 1 3.4 2.2c-.7.4-1 .9-1 1.6"/><circle cx="10" cy="14.3" r="1" fill="currentColor"/></svg>',
    doc: '<svg viewBox="0 0 20 20" aria-hidden="true"><path fill="none" stroke="currentColor" stroke-width="1.6" stroke-linejoin="round" d="M5 2.5h7l3.5 3.5v11.5H5z M12 2.5V6h3.5"/><path stroke="currentColor" stroke-width="1.5" stroke-linecap="round" d="M7.5 10h5M7.5 13h5"/></svg>',
    warn: '<svg viewBox="0 0 20 20" aria-hidden="true"><path fill="none" stroke="currentColor" stroke-width="1.6" stroke-linejoin="round" d="M10 2.8l8 14H2z"/><path stroke="currentColor" stroke-width="1.8" stroke-linecap="round" d="M10 8v4"/><circle cx="10" cy="14.3" r="1" fill="currentColor"/></svg>',
    chev: '<svg class="chev" viewBox="0 0 20 20" aria-hidden="true"><path fill="none" stroke="currentColor" stroke-width="1.8" stroke-linecap="round" stroke-linejoin="round" d="M7.5 4.5l5.5 5.5-5.5 5.5"/></svg>',
    out: '<svg viewBox="0 0 20 20" aria-hidden="true"><path fill="none" stroke="currentColor" stroke-width="1.6" stroke-linecap="round" stroke-linejoin="round" d="M11 3.5h5.5V9M16.5 3.5L9 11M8 5H4.5v10.5H15V12"/></svg>',
    info: '<svg viewBox="0 0 20 20" aria-hidden="true"><circle cx="10" cy="10" r="8" fill="none" stroke="currentColor" stroke-width="1.6"/><path stroke="currentColor" stroke-width="1.8" stroke-linecap="round" d="M10 9v5"/><circle cx="10" cy="6.3" r="1" fill="currentColor"/></svg>',
  };

  // ---- state ---------------------------------------------------------------------------------
  const S = { boot: null, busy: false, unsaved: null, review: null, delivery: null, thumb: null, covers: [], deleteId: null, deleteKind: null, lastQuestionChoices: null };

  function el(html) { const d = document.createElement("div"); d.innerHTML = html; return d.firstElementChild; }
  function show(id) {
    document.querySelectorAll(".screen").forEach(function (s) { s.hidden = s.id !== id; });
    closeSheets();
    const sc = $(id).querySelector(".scroll"); if (sc) sc.scrollTop = 0;
    unsavedBanner();
  }
  function openSheet(id) {
    $(id).hidden = false;
    document.querySelectorAll("#winbody > .screen:not([hidden])").forEach(function (sc) { sc.setAttribute("data-contrast-exempt", "behind a modal sheet and dimmed by the scrim on purpose; not meant to be read while the sheet is open"); sc.setAttribute("inert", ""); });
  }
  function closeSheets() {
    document.querySelectorAll(".sheet-scrim").forEach(function (s) { s.hidden = true; });
    document.querySelectorAll("#winbody > .screen[data-contrast-exempt]").forEach(function (sc) { sc.removeAttribute("data-contrast-exempt"); sc.removeAttribute("inert"); });
  }
  let toastTimer = null;
  function toast(text) { const t = $("toast"); t.textContent = text; t.hidden = false; clearTimeout(toastTimer); toastTimer = setTimeout(function () { t.hidden = true; }, 3200); }
  function unsavedBanner() {
    document.querySelectorAll("[id^='unsaved-slot-']").forEach(function (slot) {
      slot.innerHTML = "";
      if (!S.unsaved) return;
      const b = el('<div class="banner" role="alert">' + G.warn + '<div><p><b>Not saved on this Mac.</b> <span class="why"></span> Your work is kept in this window only; if you quit now, it is lost.</p><div class="acts"><button class="btn" data-act="copy-report">Copy report</button><button class="btn" data-act="save-file">Save to…</button></div></div></div>');
      b.querySelector(".why").textContent = S.unsaved;
      slot.appendChild(b);
    });
  }
  function storageLine() {
    return S.unsaved ? '<span class="status unsaved">' + G.warn + "Not saved on this Mac</span>" : '<span class="status saved">' + G.doc + "Saved on this Mac</span>";
  }

  // ---- first opening -------------------------------------------------------------------------
  function renderLogin(on, denied) {
    const sw = $("login-sw"), help = $("login-help");
    sw.setAttribute("aria-checked", on ? "true" : "false");
    sw.setAttribute("aria-disabled", denied ? "true" : "false");
    help.textContent = denied
      ? "macOS did not allow RichDoctor to start at login. You can still open it from the menu bar, Spotlight, or Applications, and allow it later in System Settings, under Login Items."
      : "RichDoctor can be ready in the menu bar when your Mac starts. It does nothing until you open it. You can change this in its menu.";
  }
  function renderFirst() {
    const b = S.boot;
    $("richos-line").textContent = b.installation;
    renderLogin(b.login.on, b.login.denied);
    $("resume-card").hidden = !b.resume;
    if (b.resume) {
      $("resume-desc").textContent = "“" + b.resume.description + "”";
      $("resume-when").textContent = "Saved on this Mac at " + b.resume.saved_at + ", " + (b.resume.left || "during the investigation").replace(/^draft, /, "") + ". Your answers and the information collected so far are kept.";
    }
    $("first-foot-note").textContent = b.claude_found
      ? "Uses your own Claude Code account. No setup, commands, or GitHub login needed."
      : "Claude Code was not found on this Mac. RichDoctor can still collect diagnostic information and prepare the report with you.";
    show("s-first");
    setTimeout(function () { $("desc").focus(); }, 30);
  }

  // ---- the conversation ----------------------------------------------------------------------
  function scrollLog() { const sc = $("s-conv").querySelector(".scroll"); sc.scrollTop = sc.scrollHeight; }
  function addMe(text) { const m = el('<div class="me"></div>'); m.textContent = text; $("log").appendChild(m); scrollLog(); return m; }
  function addDr(text) { const d = el('<div class="dr"><p></p></div>'); d.querySelector("p").textContent = text; $("log").appendChild(d); scrollLog(); return d; }
  function clearChoices() { document.querySelectorAll("#log .choices").forEach(function (c) { c.remove(); }); }
  function addQuestion(text, choices) {
    const d = addDr(text);
    if (choices && choices.length) {
      const g = el('<div class="choices" role="group" aria-label="Choices"></div>');
      choices.forEach(function (c) { const b = el('<button class="btn"></button>'); b.textContent = c; b.dataset.choice = c; g.appendChild(b); });
      d.appendChild(g);
    }
    showAnswer();
  }
  function showAnswer() { $("answer").hidden = false; $("answer-field").value = ""; setTimeout(function () { $("answer-field").focus({ preventScroll: true }); }, 50); }
  let progressRow = null;
  function progress(text) {
    if (!progressRow) {
      progressRow = el('<div class="progress-wrap" aria-live="polite"><div class="progress"><span class="spin" aria-hidden="true"></span><span class="pt"></span></div><div class="bar" aria-hidden="true"><i></i></div></div>');
      $("log").appendChild(progressRow);
    }
    progressRow.querySelector(".pt").textContent = text;
    scrollLog();
  }
  function done(text) {
    if (progressRow) { progressRow.remove(); progressRow = null; }
    const d = el('<div class="checked"></div>'); d.textContent = text; $("log").appendChild(d); scrollLog();
  }
  function failCard(title, text, retry) {
    clearChoices(); $("answer").hidden = true;
    if (progressRow) { progressRow.remove(); progressRow = null; }
    document.querySelectorAll("#log .fail").forEach(function (f) { f.remove(); });
    const c = el('<div class="fail" role="status"><h2></h2><p class="why"></p><div class="acts"></div></div>');
    c.querySelector("h2").textContent = title;
    c.querySelector(".why").textContent = text;
    return c;
  }
  function aiFailed(message, retry) {
    const answered = Array.from(document.querySelectorAll("#log .me")).length > 1;
    const kept = answered ? "Your answers and the diagnostic information collected so far are kept." : "Your description is kept, and diagnostic information can still be collected.";
    const c = failCard("We can still prepare your report.", message + " " + kept);
    c.insertBefore(el('<p class="soft" style="margin:0">Prepare report now builds the report from what you described and what was collected. Anything not asked is listed as unknown.</p>'), c.querySelector(".acts"));
    c.querySelector(".acts").innerHTML = (retry ? '<button class="btn" data-act="retry-ai">Retry</button>' : "") + '<button class="btn primary" data-act="prepare-now">Prepare report now</button>';
    $("log").appendChild(c); scrollLog();
  }
  function canceledCard() {
    const c = failCard("Investigation canceled.", "Your description and answers are kept on this Mac. Nothing was sent.");
    c.querySelector(".acts").innerHTML = '<button class="btn" data-act="resume-inv">Resume investigation</button><button class="btn primary" data-act="prepare-now">Prepare report now</button>';
    $("log").appendChild(c); scrollLog();
  }
  function readyFooter() {
    $("conv-prepare").hidden = true; $("conv-review").hidden = false;
    $("answer").hidden = true; clearChoices();
    setTimeout(function () { $("conv-review").focus(); }, 30);
  }
  function startConversation(sub) {
    $("log").innerHTML = ""; progressRow = null;
    $("answer").hidden = true; $("conv-review").hidden = true; $("conv-prepare").hidden = false; $("conv-image").hidden = false;
    $("conv-sub").textContent = sub;
    show("s-conv");
  }
  function replay(entries) {
    entries.forEach(function (e) { onEvent(e, true); });
  }

  // ---- events from the app -------------------------------------------------------------------
  function onEvent(e, replaying) {
    switch (e.t) {
      case "me": addMe(e.text); break;
      case "dr": addDr(e.text); break;
      case "question": clearChoices(); addQuestion(e.text, e.choices); break;
      case "progress": if (!replaying) progress(e.text); break;
      case "done": done(e.text); break;
      case "note":
        if (e.text === "canceled") { if (!replaying) canceledCard(); }
        else addMe(e.text);
        break;
      case "ai-failed": aiFailed(e.message, e.retry); $("conv-sub").textContent = "Claude Code is not available right now"; break;
      case "canceled": canceledCard(); break;
      case "ready": readyFooter(); break;
      case "answer-open": showAnswer(); break;
      case "busy":
        S.busy = e.on;
        $("answer-field").disabled = e.on;
        document.querySelectorAll("#log .choices .btn, [data-act='send-answer']").forEach(function (b) { b.disabled = e.on; });
        if (!e.on && progressRow) { progressRow.remove(); progressRow = null; }
        break;
      case "sub": $("conv-sub").textContent = e.text; break;
      case "review": invoke("review").then(function (v) { renderReview(v); }); break;
      case "storage": S.unsaved = e.ok ? null : e.sentence; unsavedBanner(); break;
      case "delivery": renderResult(e.view); break;
      case "checking-status": renderChecking(e.report_id); break;
      case "login": renderLogin(e.on, e.denied); break;
      case "go": if (e.screen === "saved") renderSaved(); break;
      case "send-refused": toast(e.why); break;
    }
  }
  T.event.listen("doctor", function (ev) { onEvent(ev.payload, false); });

  // ---- the image review ----------------------------------------------------------------------
  const stage = $("img-stage");
  let drawing = null;
  function pt(e) { const r = stage.getBoundingClientRect(); return { x: Math.max(0, Math.min(r.width, e.clientX - r.left)), y: Math.max(0, Math.min(r.height, e.clientY - r.top)) }; }
  stage.addEventListener("pointerdown", function (e) { if (e.target.classList.contains("cover")) return; drawing = { a: pt(e), box: el('<div class="drawing"></div>') }; stage.appendChild(drawing.box); stage.setPointerCapture(e.pointerId); });
  stage.addEventListener("pointermove", function (e) { if (!drawing) return; const b = pt(e), a = drawing.a; Object.assign(drawing.box.style, { left: Math.min(a.x, b.x) + "px", top: Math.min(a.y, b.y) + "px", width: Math.abs(a.x - b.x) + "px", height: Math.abs(a.y - b.y) + "px" }); });
  stage.addEventListener("pointerup", function (e) { if (!drawing) return; const b = pt(e), a = drawing.a; drawing.box.remove(); if (Math.abs(a.x - b.x) > 6 && Math.abs(a.y - b.y) > 6) addCover(Math.min(a.x, b.x), Math.min(a.y, b.y), Math.abs(a.x - b.x), Math.abs(a.y - b.y)); drawing = null; });
  function addCover(x, y, w, h) {
    const c = el('<button class="cover" type="button" aria-label="Cover. Arrow keys move it, Shift and arrows resize it, Delete removes it."></button>');
    Object.assign(c.style, { left: x + "px", top: y + "px", width: w + "px", height: h + "px" });
    stage.appendChild(c); S.covers.push(c); c.focus();
    let drag = null;
    c.addEventListener("pointerdown", function (e) { drag = { dx: e.clientX - c.offsetLeft, dy: e.clientY - c.offsetTop }; c.setPointerCapture(e.pointerId); e.stopPropagation(); });
    c.addEventListener("pointermove", function (e) { if (!drag) return; c.style.left = (e.clientX - drag.dx) + "px"; c.style.top = (e.clientY - drag.dy) + "px"; });
    c.addEventListener("pointerup", function () { drag = null; });
    c.addEventListener("keydown", function (e) {
      const step = 8; let used = true;
      const L = c.offsetLeft, Tp = c.offsetTop, W = c.offsetWidth, H = c.offsetHeight;
      if (e.key === "Delete" || e.key === "Backspace") { c.remove(); S.covers = S.covers.filter(function (k) { return k !== c; }); stage.focus(); }
      else if (e.shiftKey && e.key === "ArrowRight") c.style.width = (W + step) + "px";
      else if (e.shiftKey && e.key === "ArrowLeft") c.style.width = Math.max(12, W - step) + "px";
      else if (e.shiftKey && e.key === "ArrowDown") c.style.height = (H + step) + "px";
      else if (e.shiftKey && e.key === "ArrowUp") c.style.height = Math.max(12, H - step) + "px";
      else if (e.key === "ArrowRight") c.style.left = (L + step) + "px";
      else if (e.key === "ArrowLeft") c.style.left = Math.max(0, L - step) + "px";
      else if (e.key === "ArrowDown") c.style.top = (Tp + step) + "px";
      else if (e.key === "ArrowUp") c.style.top = Math.max(0, Tp - step) + "px";
      else used = false;
      if (used) e.preventDefault();
    });
  }
  stage.addEventListener("keydown", function (e) { if ((e.key === "c" || e.key === "C") && e.target === stage) { e.preventDefault(); keyboardCover(); } });
  function keyboardCover() { const r = stage.getBoundingClientRect(); addCover(r.width / 2 - 80, r.height / 2 - 20, 160, 40); }
  function clearImage() { S.covers.forEach(function (c) { c.remove(); }); S.covers = []; const img = $("img-src"); if (img.src) URL.revokeObjectURL(img.src); img.removeAttribute("src"); }
  function loadImage(file) {
    if (!file || !/^image\//.test(file.type)) return;
    clearImage();
    const img = $("img-src");
    img.onload = function () { openSheet("sheet-image"); setTimeout(function () { stage.focus(); }, 30); };
    img.onerror = function () { toast("That image could not be opened. Your answers are kept."); };
    img.src = URL.createObjectURL(file);
  }
  $("img-file").addEventListener("change", function () { loadImage(this.files && this.files[0]); this.value = ""; });
  document.addEventListener("paste", function (e) {
    if ($("s-conv").hidden) return;
    const item = Array.from((e.clipboardData && e.clipboardData.items) || []).find(function (i) { return /^image\//.test(i.type); });
    if (item) { e.preventDefault(); loadImage(item.getAsFile()); }
  });
  function useImage() {
    const img = $("img-src");
    const r = stage.getBoundingClientRect();
    const sx = img.naturalWidth / r.width, sy = img.naturalHeight / r.height;
    const canvas = document.createElement("canvas");
    canvas.width = img.naturalWidth; canvas.height = img.naturalHeight;
    const g = canvas.getContext("2d");
    g.drawImage(img, 0, 0);
    // Solid covers, burned into the pixels (never a translucent highlight).
    g.fillStyle = "#0c1322";
    S.covers.forEach(function (c) { g.fillRect(c.offsetLeft * sx, c.offsetTop * sy, c.offsetWidth * sx, c.offsetHeight * sy); });
    const n = S.covers.length;
    closeSheets(); clearImage();
    // PRD §6: fit to the 5 MiB limit before approval, keeping text legible: a PNG first, then a
    // JPEG, then smaller sizes no narrower than 1280 pixels. The app makes the final bytes and
    // returns them; the thumbnail is drawn from those bytes, never from the canvas.
    const tries = [["image/png", 1, 1]];
    [1, 0.85, 0.72, 0.6].forEach(function (scale) { if (scale === 1 || canvas.width * scale >= 1280) tries.push(["image/jpeg", 0.9, scale]); });
    function attempt(i) {
      if (i >= tries.length) { S.thumb = null; toast("That screenshot is too large to send legibly, even made smaller. Remove it or choose a smaller one."); return; }
      const t = tries[i];
      let c = canvas;
      if (t[2] !== 1) { c = document.createElement("canvas"); c.width = Math.round(canvas.width * t[2]); c.height = Math.round(canvas.height * t[2]); c.getContext("2d").drawImage(canvas, 0, 0, c.width, c.height); }
      const data = c.toDataURL(t[0], t[1]).split(",")[1];
      invoke("use_image", { imageBase64: data, covers: n }).then(function (finalUrl) {
        const img = new Image();
        img.onload = function () {
          const th = document.createElement("canvas");
          th.width = 180; th.height = Math.max(1, Math.round(180 * img.naturalHeight / img.naturalWidth));
          th.getContext("2d").drawImage(img, 0, 0, th.width, th.height);
          const last = Array.from(document.querySelectorAll("#log .me")).pop();
          if (last && /^Screenshot added/.test(last.textContent)) { const i2 = el('<img class="thumb" alt="">'); i2.src = th.toDataURL("image/png"); last.appendChild(i2); }
        };
        img.src = finalUrl;
      }).catch(function (err) { if (String(err) === "too-large") attempt(i + 1); else { S.thumb = null; toast(String(err)); } });
    }
    attempt(0);
  }

  // ---- review --------------------------------------------------------------------------------
  function note(text, glyph) { const n = el('<div class="note">' + (glyph || G.info) + "<p></p></div>"); n.querySelector("p").textContent = text; return n; }
  function renderReview(v) {
    if (!v) return;
    S.review = v;
    const notes = $("rv-notes"); notes.innerHTML = "";
    v.notes.forEach(function (t) { notes.appendChild(note(t)); });
    v.problems.forEach(function (p) {
      notes.appendChild(note("This needs a change before it can be sent: " + p.detail + (p.line ? " The line: “" + p.line + "”" : ""), G.warn));
    });
    const banner = $("rv-banner"); banner.innerHTML = "";
    if (v.mode === "revision-uncertain") {
      const b = note("", G.quest); b.style.border = "1px solid var(--gold)";
      b.querySelector("p").innerHTML = "<b>Editing a copy.</b> ";
      b.querySelector("p").appendChild(document.createTextNode("Your first report (" + v.old_id + ") may already have reached the team, and RichDoctor is checking. This edit is saved on this Mac and cannot be sent as a replacement until that is known."));
      banner.appendChild(b);
    } else if (v.mode === "revision-notsent") {
      const b = note("");
      b.querySelector("p").innerHTML = "<b>Revised report.</b> ";
      b.querySelector("p").appendChild(document.createTextNode("The earlier one (" + v.old_id + ") was not sent. Sending this one gives it a new report ID and retires the old one on this Mac."));
      banner.appendChild(b);
    }
    $("rv-send").hidden = v.mode === "revision-uncertain";
    $("rv-save-edit").hidden = v.mode !== "revision-uncertain";
    $("rv-back").hidden = v.mode !== "draft"; $("rv-save").hidden = v.mode !== "draft";
    $("rv-send").disabled = v.problems.length > 0;
    $("rv-exact").hidden = true; $("rv-sections").hidden = false; $("rv-title").hidden = false;
    $("rv-exact-toggle").setAttribute("aria-pressed", "false"); $("rv-exact-toggle").textContent = "Show as it will be sent";
    $("rv-title").textContent = v.title;
    const host = $("rv-sections"); host.innerHTML = "";
    function editable(s) {
      const sec = el('<div class="sec"><h3></h3><div class="txt" contenteditable="true" spellcheck="false"></div></div>');
      sec.querySelector("h3").textContent = s.heading;
      const t = sec.querySelector(".txt"); t.textContent = s.text; t.dataset.key = s.key; t.setAttribute("aria-label", s.heading + ", editable");
      host.appendChild(sec);
    }
    v.sections.forEach(editable);
    const kept = v.excerpts.filter(function (x) { return !x.removed; });
    const found = el('<div class="sec"><button class="disc" aria-expanded="false" data-act="disclose">' + G.chev + '<b>What RichDoctor found</b><span class="count"></span></button><div class="found" hidden></div></div>');
    found.querySelector(".count").textContent = kept.length ? kept.length + (kept.length === 1 ? " excerpt" : " excerpts") + ": " + kept.map(function (x) { return x.source_line.split(" · ")[0].toLowerCase(); }).join(", ") : "nothing included";
    const box = found.querySelector(".found");
    if (!kept.length) { const h = el('<p class="hint" style="margin-top:8px"></p>'); h.textContent = v.excerpts.length ? "Every excerpt was removed. The report says no diagnostic excerpts are included." : "No diagnostic information was collected for this report."; box.appendChild(h); }
    v.excerpts.forEach(function (x) {
      const ex = el('<div class="ex"></div>');
      if (!x.removed) {
        ex.innerHTML = '<div class="ex-head"><b></b><button class="btn link" data-act="remove-ex">Remove</button></div><pre></pre>';
        ex.querySelector("b").textContent = x.source_line;
        const pre = ex.querySelector("pre");
        // The removal marks in gold, as round 39 draws them; the rest as text.
        x.text.split(/(\[(?:path|email|secret|name|credential|private key) removed\])/).forEach(function (part, i) {
          if (i % 2) { const m = document.createElement("mark"); m.textContent = part; pre.appendChild(m); } else pre.appendChild(document.createTextNode(part));
        });
      } else {
        ex.innerHTML = '<div class="ex-head"><b class="removed"></b><button class="btn link" data-act="restore-ex">Put back</button></div>';
        ex.querySelector("b").textContent = x.source_line.split(" · ")[0] + " removed by you";
      }
      ex.querySelector("button").dataset.id = x.evidence_id;
      box.appendChild(ex);
    });
    box.appendChild(el('<p class="hint">Diagnostic excerpts. Each one is part of the report text; remove any you do not want to send.</p>'));
    host.appendChild(found);
    if (v.screenshots) {
      const sec = el('<div class="sec"><h3>Screenshots</h3><p class="txt" style="margin:0"></p></div>');
      sec.querySelector("p").textContent = v.screenshots === 1 ? "One screenshot you reviewed is part of this report." : v.screenshots + " screenshots you reviewed are part of this report.";
      host.appendChild(sec);
    }
    v.after.forEach(editable);
    const foot = el('<div class="sec"><p class="soft" style="margin:0"></p></div>');
    foot.querySelector("p").textContent = "Footer, included as sent: " + v.footer;
    host.appendChild(foot);
    $("rv-exact-text").textContent = v.exact;
    show("s-review");
    setTimeout(function () { $("rv-title").focus({ preventScroll: true }); }, 60);
  }
  function editSection(key, text) {
    invoke("edit", { section: key, text: text }).then(function (v) {
      // Keep the screen as it is (the caret moved on); refresh what the edit changed.
      const open = document.querySelector(".disc") && document.querySelector(".disc").getAttribute("aria-expanded") === "true";
      renderReview(v);
      if (open) { const d = document.querySelector(".disc"); d.setAttribute("aria-expanded", "true"); d.nextElementSibling.hidden = false; }
    }).catch(function (err) { toast(String(err)); });
  }
  document.addEventListener("focusout", function (e) {
    const t = e.target;
    if (!S.review || $("s-review").hidden) return;
    if (t.id === "rv-title" && t.textContent.trim() !== S.review.title) editSection("title", t.textContent);
    else if (t.classList && t.classList.contains("txt") && t.dataset.key) {
      const was = (S.review.sections.concat(S.review.after).find(function (s) { return s.key === t.dataset.key; }) || {}).text;
      if (t.innerText !== was) editSection(t.dataset.key, t.innerText);
    }
  });

  // ---- delivery results ----------------------------------------------------------------------
  function kv(id, extra) {
    return '<dl class="kv"><dt>Report ID</dt><dd class="mono">' + id + (extra || "") + "</dd><dt>On this Mac</dt><dd>" + storageLine() + "</dd></dl>";
  }
  function renderChecking(id) {
    $("result-body").innerHTML = '<div class="result"><div class="mark gold"><span class="spin" aria-hidden="true"></span></div><div><h1>Checking status…</h1><p></p></div></div>';
    $("result-body").querySelector("p").textContent = "Asking the reporting service about report " + id + ". Nothing is sent again.";
    $("result-foot").innerHTML = "";
    show("s-result");
  }
  function renderResult(v) {
    if (!v) return;
    const prev = S.delivery;
    S.delivery = v;
    const body = $("result-body"), foot = $("result-foot");
    const id = v.report_id;
    let html = "", acts = "";
    if (v.kind === "sending") {
      html = '<div class="result"><div class="mark gold"><span class="spin" aria-hidden="true"></span></div><div><h1>Sending your report…</h1><p>Your approved report is on its way to the RichOS team’s reporting service. You can close this window: once the service accepts it, delivery continues on its own.</p>' + kv(id) + "</div></div>";
      acts = '<p class="grow">The result is kept in Saved reports.</p>';
    } else if (v.kind === "sending-screenshots") {
      html = '<div class="result"><div class="mark gold"><span class="spin" aria-hidden="true"></span></div><div><h1>Sending screenshots</h1><p class="why"></p>' + kv(id) + "</div></div>";
      acts = '<p class="grow">Nothing is filed until every approved screenshot has arrived.</p>';
    } else if (v.kind === "created-checking") {
      html = '<div class="result"><div class="mark" style="color:var(--accent-text)">' + G.quest + '</div><div><h1>Report created; checking screenshots</h1><p class="why"></p><dl class="kv"><dt>Report ID</dt><dd class="mono">' + id + '</dd><dt>Public report</dt><dd><a href="#" data-act="view-report" class="issue"></a></dd></dl></div></div>';
      acts = '<button class="btn primary" data-act="check-status">Check status</button><button class="btn" data-act="email">Email support</button><div class="grow"></div><button class="btn" data-act="view-report">' + G.out + '<span>View report</span></button>';
    } else if (v.kind === "checking") {
      html = '<div class="result"><div class="mark gold"><span class="spin" aria-hidden="true"></span></div><div><h1>Checking whether your report was received</h1><p>The reporting service did not give a clear answer. RichDoctor is checking whether the report arrived, which takes up to two minutes. It will not send the report a second time.</p>' + kv(id) + "</div></div>";
      acts = '<p class="grow">You can close this window. The result is in Saved reports.</p><button class="btn" data-act="saved">Saved reports</button>';
    } else if (v.kind === "submitted") {
      html = '<div class="result"><div class="mark" style="color:var(--success)">' + G.check + '</div><div><h1>Report submitted</h1><p>The RichOS team has your report. This does not yet mean the cause is known or the issue is fixed.</p><dl class="kv"><dt>Report ID</dt><dd class="mono">' + id + ' <span class="soft">· include it if you ask support for help</span></dd><dt>Public report</dt><dd><a href="#" data-act="view-report" class="issue"></a></dd></dl></div></div>';
      acts = '<button class="btn link" data-act="saved">Saved reports</button><div class="grow"></div><button class="btn" data-act="view-report">' + G.out + '<span>View report</span></button><button class="btn primary" data-act="done">Done</button>';
    } else if (v.kind === "notsent") {
      html = '<div class="result"><div class="mark" style="color:var(--attention)">' + G.dash + '</div><div><h1>Not sent yet</h1><p class="why"></p><p>Nothing was sent. Your approved report is kept exactly as you reviewed it.</p>' + kv(id) + "</div></div>";
      acts = (v.extra.review ? '<button class="btn primary" data-act="edit-notsent">Review report</button>' : (v.extra.try_again ? '<button class="btn primary" data-act="try-again">Try again</button>' : "")) + '<button class="btn" data-act="save-file">Save report</button><button class="btn" data-act="email">Email support</button><div class="grow"></div><button class="btn link" data-act="saved">Saved reports</button>';
    } else if (v.kind === "unconfirmed") {
      html = '<div class="result"><div class="mark" style="color:var(--accent-text)">' + G.quest + '</div><div><h1>Delivery could not be confirmed</h1><p>Your report may already have reached the RichOS team. RichDoctor could not confirm it within two minutes. To avoid filing it twice, Send report is not offered; Check status asks the service about this same report, and RichDoctor checks again whenever you open it.</p>' + kv(id) + '<p class="soft" style="margin-top:12px">If you need help sooner, email support and include the report ID. Edits you make are saved on this Mac and cannot be sent as a replacement until RichDoctor knows whether the first report arrived.</p></div></div>';
      acts = '<button class="btn primary" data-act="check-status">Check status</button><button class="btn" data-act="email">Email support</button><button class="btn" data-act="edit-uncertain">Edit report</button><div class="grow"></div><button class="btn link" data-act="saved">Saved reports</button>';
      if (prev && prev.kind === "unconfirmed" && v.extra.checked_at) toast("Still not confirmed, checked at " + v.extra.checked_at + ". The report ID has not changed.");
    }
    body.innerHTML = html; foot.innerHTML = acts;
    if (v.kind === "notsent") body.querySelector(".why").textContent = v.extra.reason;
    if (v.kind === "submitted" || v.kind === "created-checking") body.querySelector(".issue").textContent = v.extra.issue_url.replace("https://", "");
    if (v.kind === "sending-screenshots") body.querySelector(".why").textContent = "Screenshot " + Math.min(v.extra.done + 1, v.extra.total) + " of " + v.extra.total + " is on its way to RichOS support. Your report is filed once all of them have arrived.";
    if (v.kind === "created-checking") body.querySelector(".why").textContent = "The report is on GitHub, and " + v.extra.confirmed + " of " + v.extra.expected + " screenshots are confirmed so far. RichDoctor keeps the screenshots on this Mac and will not file the report again.";
    const ap = el('<div class="approved well"><p class="lab"></p><p class="t"></p><p class="x"></p><p class="more"></p></div>');
    ap.querySelector(".lab").textContent = v.kind === "submitted" ? "Your report, as sent" : "Your report, exactly as you approved it";
    ap.querySelector(".t").textContent = v.title;
    ap.querySelector(".x").textContent = v.first;
    ap.querySelector(".more").textContent = v.summary;
    body.appendChild(ap);
    show("s-result");
  }

  // ---- saved reports -------------------------------------------------------------------------
  function renderSaved() {
    invoke("saved").then(function (data) {
      const list = $("saved-list"); list.innerHTML = "";
      const glyph = { submitted: G.checkc, unconfirmed: G.quest, notsent: G.dash, saved: G.doc };
      const cls = { submitted: "ok", unconfirmed: "unsure", notsent: "notsent", saved: "saved" };
      const actWords = { view: "View report", resume: "Resume", check: "Check status", try: "Try again", review: "Review report" };
      data.rows.concat(data.sent).forEach(function (r) {
        const it = el('<div class="item"><div class="t"></div><div class="acts"></div><div class="d"></div><div class="st"></div></div>');
        it.querySelector(".t").textContent = r.title;
        it.querySelector(".d").textContent = r.date + (r.left ? " · " + r.left : "");
        const unsaved = r.state === "saved" && S.unsaved;
        const st = el('<span class="status ' + (unsaved ? "unsaved" : cls[r.state] || "saved") + '">' + (unsaved ? G.warn : glyph[r.state] || G.doc) + "<span></span></span>");
        st.querySelector("span").textContent = unsaved ? "Not saved on this Mac" : r.state_words;
        it.querySelector(".st").appendChild(st);
        const acts = it.querySelector(".acts");
        const act = r.kind === "sent-link" ? "view" : r.action;
        const a = el('<button class="btn"></button>');
        if (act === "resume") a.classList.add("primary");
        a.innerHTML = act === "view" ? G.out + "<span></span>" : "<span></span>";
        a.querySelector("span").textContent = actWords[act];
        a.dataset.act = "row-" + act; a.dataset.id = r.id || ""; a.dataset.url = r.issue_url || "";
        acts.appendChild(a);
        if (r.id) {
          const del = el('<button class="btn danger">Delete local copy</button>');
          del.dataset.act = "delete"; del.dataset.id = r.id; del.dataset.kind = r.state === "submitted" ? "sent" : r.state === "unconfirmed" ? "unsure" : "draft";
          del.setAttribute("aria-label", "Delete local copy of " + r.title);
          acts.appendChild(del);
        }
        list.appendChild(it);
      });
      if (!list.children.length) list.appendChild(el('<p class="soft">No reports are saved on this Mac.</p>'));
      show("s-saved");
    });
  }
  function openSaved(id, then) {
    return invoke("open_saved", { id: id }).then(function (d) {
      if (d.delivery) { renderResult(d.delivery); if (then) then(d); return d; }
      if (d.stage === "review" && d.has_report) { invoke("review").then(renderReview); return d; }
      startConversation(d.without_ai ? "Preparing without AI" : "Investigating with your Claude Code account");
      replay(d.conversation);
      invoke("resume_pending");
      const last = d.conversation[d.conversation.length - 1];
      if (!d.pending_question && !(last && (last.t === "ai-failed"))) canceledCard();
      if (d.pending_question) showAnswer();
      return d;
    }).catch(function (err) { toast(String(err)); });
  }

  // ---- email support -------------------------------------------------------------------------
  function openEmail() {
    const b = $("em-body"), f = $("em-foot");
    const id = (S.delivery && S.delivery.report_id) || (S.review && S.review.report_id) || "";
    invoke("mail_available").then(function (mail) {
      if (mail) {
        b.innerHTML = '<h2 id="em-h">Email support</h2><p></p><p class="soft" style="margin:0">Emailing is not the same as the public report: it does not create a GitHub report or change this report’s delivery state.</p>';
        b.querySelector("p").innerHTML = "RichDoctor opens a new email in Mail to <b>support@richos.ceo</b> with the reviewed report text, its report ID (<span></span>) and the screenshots you approved, attached. You read it and send it yourself. The larger diagnostic packet is not included.";
        b.querySelector("p span").textContent = id;
        f.innerHTML = '<div class="grow"></div><button class="btn" data-act="close-sheet">Cancel</button><button class="btn primary" data-act="open-mail">Open in Mail</button>';
      } else {
        b.innerHTML = '<h2 id="em-h">Email support</h2><p></p><p><span class="addr">support@richos.ceo <button class="btn link" data-act="copy-addr" style="height:28px">Copy</button></span></p><p class="soft" style="margin:0">Save report saves the reviewed text and your approved screenshots in a folder to attach. Copy report copies the text only: the screenshots still need attaching.</p>';
        b.querySelector("p").textContent = "No email app is set up on this Mac. Send the report from any email account you use, to this address, and include the report ID " + id + ":";
        f.innerHTML = '<button class="btn" data-act="copy-report">Copy report</button><button class="btn" data-act="save-file">Save report</button><div class="grow"></div><button class="btn primary" data-act="close-sheet">Done</button>';
      }
      openSheet("sheet-email"); setTimeout(function () { f.querySelector(".btn.primary").focus(); }, 30);
    });
  }

  // ---- actions -------------------------------------------------------------------------------
  function fail(err) { toast(String(err)); }
  function begin(withoutAi) {
    const desc = $("desc").value.trim();
    if (!desc) { $("desc").focus(); toast("Describe what went wrong first. A sentence or two is enough."); return; }
    startConversation(withoutAi ? "Preparing without AI" : "Investigating with your Claude Code account");
    if (withoutAi) $("conv-image").hidden = false;
    invoke("start", { description: desc, noDiagnostics: $("no-diag").checked, withoutAi: withoutAi }).catch(function (err) { fail(err); renderFirst(); });
  }
  function sendAnswer(text) {
    if (!text || S.busy) return;
    clearChoices(); $("answer").hidden = true;
    invoke("answer", { text: text });
  }
  document.addEventListener("click", function (e) {
    const ch = e.target.closest("[data-choice]");
    if (ch) {
      sendAnswer(ch.dataset.choice); return;
    }
    const b = e.target.closest("[data-act]"); if (!b) return;
    const act = b.dataset.act;
    switch (act) {
      case "continue": begin(false); break;
      case "no-ai": begin(true); break;
      case "resume": openSaved(S.boot.resume.id); break;
      case "new-report": $("resume-card").hidden = true; $("desc").value = ""; $("desc").focus(); break;
      case "send-answer": sendAnswer($("answer-field").value.trim()); break;
      case "cancel-inv": invoke("cancel"); break;
      case "resume-inv": case "retry-ai": document.querySelectorAll("#log .fail").forEach(function (f) { f.remove(); }); invoke("retry"); break;
      case "prepare-now": case "to-review":
        document.querySelectorAll("#log .fail").forEach(function (f) { f.remove(); });
        clearChoices(); $("answer").hidden = true;
        progress("Preparing your report…");
        invoke("prepare"); break;
      case "add-image": $("img-file").click(); break;
      case "add-cover": keyboardCover(); break;
      case "use-image": useImage(); break;
      case "remove-image": clearImage(); closeSheets(); toast("Screenshot removed. Your answers are kept."); $("answer-field").focus(); break;
      case "close-sheet": if (!$("sheet-image").hidden) clearImage(); closeSheets(); break;
      case "toggle-exact": {
        const on = $("rv-exact").hidden;
        $("rv-exact").hidden = !on; $("rv-sections").hidden = on; $("rv-title").hidden = on;
        b.setAttribute("aria-pressed", on ? "true" : "false"); b.textContent = on ? "Back to the readable view" : "Show as it will be sent"; break;
      }
      case "disclose": { const open = b.getAttribute("aria-expanded") !== "true"; b.setAttribute("aria-expanded", open ? "true" : "false"); b.nextElementSibling.hidden = !open; break; }
      case "remove-ex": case "restore-ex":
        invoke("excerpt", { id: b.dataset.id, removed: act === "remove-ex" }).then(function (v) { renderReview(v); const d = document.querySelector(".disc"); d.setAttribute("aria-expanded", "true"); d.nextElementSibling.hidden = false; });
        break;
      case "back-conv": show("s-conv"); break;
      case "save-later": invoke("save_later").then(toast); break;
      case "send": invoke("send_report").catch(fail); break;
      case "save-edit": invoke("save_later").then(function () { toast("Edit saved on this Mac. The first report is still being checked."); invoke("delivery").then(renderResult); }); break;
      case "try-again": invoke("try_again").catch(fail); break;
      case "edit-notsent": case "edit-uncertain": invoke("review").then(renderReview); break;
      case "check-status": invoke("check_status").catch(fail); break;
      case "email": openEmail(); break;
      case "open-mail": invoke("open_mail").then(function () { closeSheets(); toast("A new email to support@richos.ceo opened in Mail. Send it when you are ready."); }).catch(function (err) {
        if (String(err) === "parts") { closeSheets(); openSheet("sheet-save"); toast("The screenshots are more than 10 MiB together, and email may refuse that. Save them as numbered parts and send one part per email."); }
        else fail(err);
      }); break;
      case "copy-addr": invoke("copy_address").then(function (ok) { toast(ok ? "Address copied: support@richos.ceo" : "The address could not be copied."); }); break;
      case "copy-report": invoke("copy_report").then(function (id) { toast("Report text copied. It includes the report ID " + id + "." + (S.review && S.review.screenshots ? " The screenshots still need attaching." : "")); }).catch(fail); break;
      case "save-file": closeSheets(); openSheet("sheet-save"); break;
      case "confirm-save": closeSheets(); invoke("save_report_file").then(function (id) { toast("Saved RichOS report " + id + " where you chose, with its screenshots."); }).catch(function (err) { if (String(err) !== "canceled") fail(err); }); break;
      case "view-report": e.preventDefault(); if (S.delivery && S.delivery.extra.issue_url) invoke("open_issue", { url: S.delivery.extra.issue_url }).catch(fail); break;
      case "done": invoke("close_help"); $("desc").value = ""; invoke("boot").then(function (v) { S.boot = v; renderFirst(); }); break;
      case "saved": renderSaved(); break;
      case "close-saved": invoke("boot").then(function (v) { S.boot = v; renderFirst(); }); break;
      case "row-view": invoke("open_issue", { url: b.dataset.url }).catch(fail); break;
      case "row-resume": case "row-check": case "row-try": case "row-review":
        openSaved(b.dataset.id, function () {
          if (act === "row-try") invoke("try_again").catch(fail);
          if (act === "row-review") invoke("review").then(renderReview);
        });
        break;
      case "delete": {
        S.deleteId = b.dataset.id; S.deleteKind = b.dataset.kind;
        $("del-p").textContent = S.deleteKind === "sent" ? "The local copy is deleted. The public report on GitHub stays, and its link is kept here." : S.deleteKind === "unsure" ? "The local copy is deleted. If the report already reached the team, it stays public; deleting here does not withdraw it." : "The local copy is deleted. Nothing was sent, so nothing public exists.";
        openSheet("sheet-delete"); setTimeout(function () { $("sheet-delete").querySelector(".btn:not(.danger)").focus(); }, 30); break;
      }
      case "confirm-delete": closeSheets(); invoke("delete_saved", { id: S.deleteId }).then(function () { toast("Deleted from this Mac."); renderSaved(); }).catch(fail); break;
    }
  });
  $("login-sw").addEventListener("click", function () {
    if (this.getAttribute("aria-disabled") === "true") return;
    const on = this.getAttribute("aria-checked") !== "true";
    invoke("set_login", { on: on }).then(function (r) { renderLogin(r.on, r.denied); });
  });
  $("answer-field").addEventListener("keydown", function (e) { if (e.key === "Enter" && !e.shiftKey) { e.preventDefault(); sendAnswer(this.value.trim()); } });
  $("desc").addEventListener("keydown", function (e) { if (e.key === "Enter" && (e.metaKey || e.ctrlKey)) { e.preventDefault(); begin(false); } });
  document.addEventListener("keydown", function (e) { if (e.key === "Escape") { const open = Array.from(document.querySelectorAll(".sheet-scrim")).find(function (s) { return !s.hidden; }); if (open) { if (open.id === "sheet-image") clearImage(); closeSheets(); e.preventDefault(); } } });

  // ---- boot ----------------------------------------------------------------------------------
  invoke("boot").then(function (v) {
    S.boot = v;
    if (new URLSearchParams(location.search).get("screen") === "saved") renderSaved(); else renderFirst();
  });
})();

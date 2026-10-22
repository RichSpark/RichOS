// RichDoctor's help window: round 39's screens and round 40's screenshot flow, driven by the app (crates/richdoctor/src/lib.rs
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
  const S = { boot: null, busy: false, unsaved: null, review: null, delivery: null, deleteId: null, deleteKind: null, lastQuestionChoices: null,
    // round 40: the approved screenshots, the files that could not be used, Claude helping, a Cancel asked for
    shots: [], notAdded: [], claudeHelping: false, cancelAsked: false, removedUploaded: false, plan: null };

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
    S.claudeHelping = false;
    const kept = answered ? "Your answers, your screenshots and the diagnostic information collected so far are kept." : "Your description is kept, and diagnostic information can still be collected.";
    const c = failCard("We can still prepare your report.", message + " " + kept);
    c.insertBefore(el('<p class="soft" style="margin:0">Prepare report now builds the report from what you described, what was collected and any screenshots you approved. Anything not asked is listed as unknown.</p>'), c.querySelector(".acts"));
    c.querySelector(".acts").innerHTML = (retry ? '<button class="btn" data-act="retry-ai">Retry</button>' : "") + '<button class="btn primary" data-act="prepare-now">Prepare report now</button>';
    $("log").appendChild(c); scrollLog();
  }
  function canceledCard() {
    const c = failCard("Investigation canceled.", "Your description, answers and screenshots are kept on this Mac. Nothing was sent.");
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
    S.claudeHelping = sub === "Investigating with your Claude Code account";
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
      case "upload":
        if (S.delivery && S.delivery.kind === "uploading") { S.delivery.rows = e.rows; paintUpload(S.delivery); }
        break;
    }
  }
  T.event.listen("doctor", function (ev) { onEvent(ev.payload, false); });

  // ---- screenshots (round 40 §1 to §3): pick, drop or paste; cover; check the exact image -------
  // The pipeline is round 40's design of the real one, in order: the header is checked first (not
  // PNG or JPEG, unreadable, or over 40 megapixels is refused before any pixel is decoded), then
  // the working copy, covers in image pixels, flattened into new pixels, encoded, made final by the
  // app (the shared format profile, no metadata) and reduced until it fits 5 MiB. Step two shows
  // the decoded final bytes the app returned; those bytes are what is stored, hashed and shared.
  const LIM = { count: 10, each: 5 * 1024 * 1024, total: 25 * 1024 * 1024 };
  const IMG_GLYPH = '<svg viewBox="0 0 20 20" aria-hidden="true"><rect x="2.5" y="4" width="15" height="12" rx="2" fill="none" stroke="currentColor" stroke-width="1.6"/><path fill="none" stroke="currentColor" stroke-width="1.6" stroke-linejoin="round" d="M3 14l4.5-4.5 3.5 3.5 2.5-2.5 3.5 3.5"/><circle cx="13" cy="8" r="1.4" fill="currentColor"/></svg>';
  const PROBLEM_WORD = { type: "not a PNG or JPEG", unreadable: "could not be read", pixels: "too many pixels", fit: "could not be made small enough", room: "no room left" };
  function esc(s) { return String(s).replace(/&/g, "&amp;").replace(/</g, "&lt;").replace(/"/g, "&quot;"); }
  function plural(n, one, many) { return n + " " + (n === 1 ? one : many || one + "s"); }
  // Sizes as the Mac shows them (decimal KB and MB); the limits themselves are the PRD's MiB.
  function fmtBytes(b) { return b < 1e6 ? Math.round(b / 1e3).toLocaleString("en-US") + " KB" : (b / 1e6).toFixed(1) + " MB"; }
  function fmtDims(w, h) { return w.toLocaleString("en-US") + " × " + h.toLocaleString("en-US"); }
  const problemText = {
    type: function (e) { return "<p><b>" + esc(e.file) + " is not a PNG or JPEG image.</b></p><p>RichDoctor can use PNG and JPEG screenshots. Save it as a PNG, or take a screenshot of it, and add that instead. Nothing else in your report changes.</p>"; },
    unreadable: function (e) { return "<p><b>" + esc(e.file) + " could not be read as an image.</b></p><p>The file may be damaged or still being written. Open it on this Mac to check; if it opens, save a copy as a PNG and add that instead.</p>"; },
    pixels: function (e) { return "<p><b>" + esc(e.file) + " is too large to use: " + fmtDims(e.declared.w, e.declared.h) + ", about " + Math.round(e.declared.w * e.declared.h / 1e6) + " megapixels.</b></p><p>RichDoctor can use images up to 40 megapixels. Crop it to the part that shows the problem, or resize it, then add it again.</p>"; },
    fit: function (e) { return "<p><b>" + esc(e.file) + " cannot be made small enough while staying readable.</b></p><p>Each screenshot can be up to 5 MB after covering. Crop it to the part that shows the problem, then add it again, or leave it out.</p>"; },
    room: function (e) { return "<p><b>There is no room for " + esc(e.file) + ".</b></p><p>A report can carry up to 10 screenshots and 25 MB in all. Remove one you need less, then add this one again.</p>"; },
  };
  function bytesToB64(buf) {
    const u = new Uint8Array(buf); let s = ""; const step = 0x8000;
    for (let i = 0; i < u.length; i += step) s += String.fromCharCode.apply(null, u.subarray(i, i + step));
    return btoa(s);
  }
  function loadImg(src) { return new Promise(function (res, rej) { const im = new Image(); im.onload = function () { res(im); }; im.onerror = function () { rej(new Error("decode")); }; im.src = src; }); }
  function canvasOf(im, w, h) { const c = document.createElement("canvas"); c.width = w; c.height = h; c.getContext("2d").drawImage(im, 0, 0, w, h); return c; }
  // One file in the queue: { file, note, work (canvas), covers [{x,y,w,h} in image px], final, reduced, problem, approved, declared, n }
  function fromFile(file) {
    const e = { file: file.name || "Pasted image.png", note: fmtBytes(file.size), covers: [], approved: false, declared: null };
    const typed = /^image\/(png|jpeg)$/.test(file.type) || /\.(png|jpe?g)$/i.test(e.file);
    if (!typed) { e.problem = "type"; return Promise.resolve(e); }
    // The header, before any decode: the app reads the type and the declared size from it.
    return file.slice(0, 512 * 1024).arrayBuffer().then(function (head) {
      return invoke("inspect_image", { imageBase64: bytesToB64(head) }).then(function (h) {
        if (!h.ok && h.problem === "unreadable" && file.size > head.byteLength) return file.arrayBuffer().then(function (all) { return invoke("inspect_image", { imageBase64: bytesToB64(all) }); });
        return h;
      });
    }).then(function (h) {
      if (!h.ok) { e.problem = h.problem; if (h.width) e.declared = { w: h.width, h: h.height }; return e; }
      const url = URL.createObjectURL(file);
      return loadImg(url).then(function (im) {
        URL.revokeObjectURL(url);
        e.work = canvasOf(im, im.naturalWidth, im.naturalHeight);
        e.note = fmtDims(im.naturalWidth, im.naturalHeight) + " · " + fmtBytes(file.size);
        return e;
      }, function () { URL.revokeObjectURL(url); e.problem = "unreadable"; return e; });
    }).catch(function () { e.problem = "unreadable"; return e; });
  }
  // An approved screenshot from the draft, as the app returned it.
  function fromSaved(f) {
    return { n: f.n, name: f.name, file: f.name, note: fmtDims(f.width, f.height), covers: new Array(f.covers).fill(null), approved: true, reducedFlag: f.reduced, final: { src: f.url, w: f.width, h: f.height, bytes: f.bytes } };
  }
  // A saved screenshot that cannot be read is named (the app's error), never dropped silently.
  function refreshShots() { return invoke("images").then(function (list) { S.shots = (list || []).map(fromSaved); return S.shots; }, function (err) { fail(err); return S.shots; }); }
  function shotsBytes() { return S.shots.reduce(function (a, s) { return a + s.final.bytes; }, 0); }
  // Flatten the covers into new pixels, have the app make the final bytes, and reduce until the
  // file fits 5 MiB (no narrower than 30% of the original: past that there is no legible fit).
  function finalize(e) {
    const base = e.work;
    function attempt(scale, first) {
      const w = Math.max(1, Math.round(base.width * scale)), h = Math.max(1, Math.round(base.height * scale));
      const c = document.createElement("canvas"); c.width = w; c.height = h;
      const g = c.getContext("2d"); g.drawImage(base, 0, 0, w, h);
      g.fillStyle = "#0c1322";
      e.covers.forEach(function (k) { if (k) g.fillRect(Math.floor(k.x * scale), Math.floor(k.y * scale), Math.ceil(k.w * scale) + 1, Math.ceil(k.h * scale) + 1); });
      const b64 = c.toDataURL("image/png").split(",")[1];
      return invoke("prepare_image", { imageBase64: b64 }).then(function (f) {
        e.final = { src: f.url, w: f.width, h: f.height, bytes: f.bytes, b64: f.b64 };
        const from = e.reduceFrom || (scale < 1 ? { w: base.width, h: base.height, bytes: first || f.bytes } : null);
        e.reduced = from ? { from: from, to: { w: f.width, h: f.height, bytes: f.bytes } } : null;
        return true;
      }, function (err) {
        const m = /^too-large:(\d+)$/.exec(String(err));
        if (!m) { e.problem = "unreadable"; e.final = null; return false; }
        const bytes = +m[1];
        const next = scale * Math.sqrt(LIM.each / bytes) * 0.92;
        if (next < 0.3) { e.problem = "fit"; e.final = null; return false; }
        return attempt(next, first || bytes);
      });
    }
    return attempt(1, 0);
  }

  // the queue
  const Q = { items: [], i: 0, mode: "add", from: "conv" };
  const stage = $("img-stage");
  let drawing = null;
  function cur() { return Q.items[Q.i]; }
  function nOf() {
    const e = cur();
    if (Q.mode !== "add") return "Screenshot " + e.n;
    return Q.items.length > 1 ? "Screenshot " + (Q.i + 1) + " of " + Q.items.length : "Screenshot";
  }
  function canAddHere() { return !$("s-conv").hidden || !$("s-review").hidden; }
  function beginAdding(promises) {
    if (!promises.length) return;
    Promise.all(promises).then(function (entries) {
      const room = LIM.count - S.shots.length;
      let ok = 0;
      entries.forEach(function (e) { if (!e.problem) { ok++; if (ok > room) e.problem = "room"; } });
      Q.items = entries; Q.i = 0; Q.mode = "add"; Q.from = $("s-review").hidden ? "conv" : "review";
      openQueueItem();
    });
  }
  function queueStrip() {
    const host = $("img-queue");
    if (Q.items.length < 2) { host.innerHTML = ""; host.hidden = true; return; }
    host.hidden = false;
    host.innerHTML = Q.items.map(function (e, i) {
      const mark = e.problem ? '<span class="bad">' + G.warn + "</span>" : e.approved ? '<span class="ok">' + G.checkc + "</span>" : "";
      return '<button type="button" data-act="queue-go" data-i="' + i + '" aria-current="' + (i === Q.i ? "true" : "false") + '" aria-label="Screenshot ' + (i + 1) + " of " + Q.items.length + (e.problem ? ", cannot be used" : e.approved ? ", approved" : ", not checked yet") + '"><span class="badge' + (i === Q.i ? "" : " soft") + '">' + (i + 1) + "</span>" + mark + "</button>";
    }).join("");
  }
  function openQueueItem() {
    const e = cur();
    if (!e) { finishQueue(); return; }
    queueStrip();
    $("img-cover").hidden = true; $("img-check").hidden = true; $("img-problem").hidden = true;
    const foot = $("img-foot");
    if (e.problem) {
      $("img-h").textContent = nOf() + ": this file cannot be used";
      $("img-p").textContent = "";
      $("img-problem-text").innerHTML = problemText[e.problem](e);
      $("img-problem").hidden = false;
      foot.innerHTML = '<button class="btn link" data-act="img-replace">Choose a different file…</button><div class="grow"></div><button class="btn" data-act="img-cancel-all">Cancel</button><button class="btn primary" data-act="img-skip">' + (Q.i + 1 < Q.items.length ? "Leave it out, next" : "Leave it out") + "</button>";
      openSheet("sheet-image"); setTimeout(function () { foot.querySelector(".primary").focus(); }, 30);
      return;
    }
    if (Q.mode === "inspect") { showCheck(e, true); return; }
    showCover(e);
  }
  function toClaude() { return Q.from === "conv" && S.claudeHelping; }
  function showCover(e) {
    $("img-h").textContent = nOf() + ": cover anything private";
    $("img-p").textContent = "Drag over names, email addresses, file paths, or anything else you do not want the team to see. Covers are solid: the covered areas are replaced with new pixels and the original's metadata is removed. " + (toClaude() ? "You check the finished image next, before Claude sees it." : "You check the finished image next.");
    $("img-cover").hidden = false; $("img-check").hidden = true; $("img-problem").hidden = true;
    const im = $("img-work"); im.src = e.work.toDataURL("image/png"); im.alt = "Screenshot preview: " + e.note;
    stage.querySelectorAll(".cover, .drawing").forEach(function (c) { c.remove(); });
    $("img-foot").innerHTML = '<button class="btn link" data-act="img-remove">Remove image</button><div class="grow"></div><button class="btn" data-act="img-cancel-all">Cancel</button><button class="btn primary" data-act="img-next">Check the exact image</button>';
    openSheet("sheet-image");
    im.onload = function () {
      const scale = e.work.width / stage.getBoundingClientRect().width;
      e.covers.forEach(function (k) { if (k) placeCover(k.x / scale, k.y / scale, k.w / scale, k.h / scale, false); });
      stage.focus();
    };
  }
  function readCovers(e) {
    const scale = e.work.width / stage.getBoundingClientRect().width;
    e.covers = Array.from(stage.querySelectorAll(".cover")).map(function (c) { return { x: c.offsetLeft * scale, y: c.offsetTop * scale, w: c.offsetWidth * scale, h: c.offsetHeight * scale }; });
  }
  function nextNumber(e) { return Q.mode !== "add" ? e.n : S.shots.length + Q.items.slice(0, Q.i).filter(function (x) { return x.approved; }).length + 1; }
  function showCheck(e, inspect) {
    const go = inspect || !e.work ? Promise.resolve(true) : (function () { if (!$("img-cover").hidden) readCovers(e); return finalize(e); })();
    $("img-foot").querySelectorAll("button").forEach(function (b) { b.disabled = true; });
    go.then(function (ok) {
      if (!ok) { openQueueItem(); return; }
      $("img-cover").hidden = true; $("img-check").hidden = false; $("img-problem").hidden = true;
      $("img-h").textContent = inspect ? "Screenshot " + e.n + ", exactly as it will be shared" : nOf() + ": check the exact image";
      $("img-p").innerHTML = inspect
        ? "This is the file that will be shared with the RichOS support team, exactly as you approved it. Edit covers reopens it; a changed image needs your approval again."
        : "This is the finished file, decoded from the exact bytes that will be used: covers flattened, metadata removed. " + (toClaude() ? "<b>Use this image</b> lets Claude look at it during the investigation and keeps it for the report. It is shared with the RichOS support team only when you choose Send report." : "<b>Use this image</b> keeps it for the report. It is shared with the RichOS support team only when you choose Send report.");
      const f = e.final, fi = $("img-final"); fi.src = f.src; fi.alt = "The exact image: " + e.note;
      const n = nextNumber(e);
      $("img-final-n").textContent = n;
      const covered = e.covers.filter(Boolean).length;
      $("img-final-meta").innerHTML = "<b>screenshot-" + n + ".png</b> <span class=\"soft\">· " + fmtDims(f.w, f.h) + " · " + fmtBytes(f.bytes) + " · " + (covered ? plural(covered, "area", "areas") + " covered" : "nothing covered") + " · metadata removed · named this way on purpose, never the original file name</span>";
      $("img-fit").innerHTML = e.reduced ? '<div class="note" style="margin:10px 0 0">' + G.info + "<p><b>Reduced to fit the 5 MB limit</b>: from " + fmtDims(e.reduced.from.w, e.reduced.from.h) + " (" + fmtBytes(e.reduced.from.bytes) + ") to " + fmtDims(e.reduced.to.w, e.reduced.to.h) + " (" + fmtBytes(e.reduced.to.bytes) + "). Check that it is still readable; if not, crop the original and add it again.</p></div>" : "";
      const foot = $("img-foot");
      if (inspect) foot.innerHTML = '<button class="btn link" data-act="img-edit">Edit covers</button><button class="btn link" data-act="img-remove">Remove image</button><div class="grow"></div><button class="btn primary" data-act="img-done">Done</button>';
      else foot.innerHTML = (e.work ? '<button class="btn link" data-act="img-back">Back to covers</button>' : "") + '<button class="btn link" data-act="img-remove">Remove image</button><div class="grow"></div><button class="btn" data-act="img-cancel-all">Cancel</button><button class="btn primary" data-act="img-use">Use this image</button>';
      openSheet("sheet-image"); setTimeout(function () { foot.querySelector(".primary").focus(); }, 30);
    });
  }
  function useCurrent() {
    const e = cur();
    const covers = e.covers.filter(Boolean).length;
    // A file already approved in this queue (revisited through the strip) is replaced, never
    // added a second time beside its less-covered version.
    const replacing = Q.mode === "edit" || Q.mode === "reduce" || (e.approved && e.n > 0);
    const call = replacing
      ? invoke("replace_image", { index: e.n - 1, imageBase64: e.final.b64, covers: covers, reduced: !!e.reduced || !!e.reducedFlag })
      : invoke("use_image", { imageBase64: e.final.b64, covers: covers, reduced: !!e.reduced });
    call.then(function (f) {
      e.approved = true; e.n = f.n; e.name = f.name; e.final = { src: f.url, w: f.width, h: f.height, bytes: f.bytes };
      if (replacing) { const at = S.shots.findIndex(function (s) { return s.n === e.n; }); if (at >= 0) S.shots[at] = e; else if (S.shots.indexOf(e) < 0) S.shots.push(e); S.changedAfterApproval = true; }
      else S.shots.push(e);
      Q.i++; openQueueItem();
    }).catch(function (err) { toast(String(err)); });
  }
  function skipCurrent() { const e = cur(); if (e.problem && S.notAdded.indexOf(e) < 0) S.notAdded.push(e); e.skipped = true; Q.i++; openQueueItem(); }
  function finishQueue() {
    closeSheets();
    const added = Q.items.filter(function (e) { return e.approved && Q.mode === "add"; });
    const refused = Q.items.filter(function (e) { return e.problem; });
    if (Q.mode !== "add") { afterQueue(); return; }
    if (Q.from === "conv") {
      if (added.length) {
        const covered = added.reduce(function (a, e) { return a + e.covers.filter(Boolean).length; }, 0);
        const m = addMe(plural(added.length, "screenshot") + " added" + (covered ? " (" + plural(covered, "area", "areas") + " covered)" : ""));
        const t = el('<div class="thumbs"></div>'); added.forEach(function (e) { const im = document.createElement("img"); im.src = e.final.src; im.alt = e.name + ", " + e.note; t.appendChild(im); }); m.appendChild(t);
        invoke("note_added", { count: added.length, covers: covered });
      }
      if (refused.length) {
        const n = el('<div class="notadded" role="status">' + G.warn + "<p></p></div>");
        n.querySelector("p").innerHTML = "<b>Not added:</b> " + refused.map(function (e) { return esc(e.file) + " (" + PROBLEM_WORD[e.problem] + ")"; }).join("; ") + ". Nothing else changed.";
        $("log").appendChild(n);
      }
      scrollLog();
      if (!$("answer").hidden) $("answer-field").focus();
    } else {
      refused.forEach(function (e) { if (S.notAdded.indexOf(e) < 0) S.notAdded.push(e); });
      afterQueue();
    }
  }
  function afterQueue() { invoke("review").then(renderReview); }
  function inspectShot(n, edit) {
    const s = S.shots[n - 1];
    Q.items = [s]; Q.i = 0; Q.from = "review";
    if (edit) {
      Q.mode = "edit";
      // A screenshot added in this window keeps its working copy and covers; one from a saved draft
      // starts from its approved image, with its covers already in its pixels.
      if (!s.work) { loadImg(s.final.src).then(function (im) { s.work = canvasOf(im, im.naturalWidth, im.naturalHeight); s.covers = []; showCover(s); }); return; }
      showCover(s); return;
    }
    Q.mode = "inspect"; openQueueItem();
  }
  function reduceShot(n) {
    // Past the five-minute cap: half the width and height, approved again (round 40 §5).
    const s = S.shots[n - 1];
    loadImg(s.final.src).then(function (im) {
      s.reduceFrom = { w: s.final.w, h: s.final.h, bytes: s.final.bytes };
      s.work = canvasOf(im, Math.max(1, Math.round(im.naturalWidth / 2)), Math.max(1, Math.round(im.naturalHeight / 2)));
      s.covers = [];
      Q.items = [s]; Q.i = 0; Q.mode = "reduce"; Q.from = "review";
      showCheck(s, false);
    });
  }

  // covers on the stage: by pointer and by keyboard (round 39's mechanics)
  function pt(e) { const r = stage.getBoundingClientRect(); return { x: Math.max(0, Math.min(r.width, e.clientX - r.left)), y: Math.max(0, Math.min(r.height, e.clientY - r.top)) }; }
  stage.addEventListener("pointerdown", function (e) { if (e.target.classList.contains("cover")) return; drawing = { a: pt(e), box: el('<div class="drawing"></div>') }; stage.appendChild(drawing.box); stage.setPointerCapture(e.pointerId); });
  stage.addEventListener("pointermove", function (e) { if (!drawing) return; const b = pt(e), a = drawing.a; Object.assign(drawing.box.style, { left: Math.min(a.x, b.x) + "px", top: Math.min(a.y, b.y) + "px", width: Math.abs(a.x - b.x) + "px", height: Math.abs(a.y - b.y) + "px" }); });
  stage.addEventListener("pointerup", function (e) { if (!drawing) return; const b = pt(e), a = drawing.a; drawing.box.remove(); if (Math.abs(a.x - b.x) > 6 && Math.abs(a.y - b.y) > 6) placeCover(Math.min(a.x, b.x), Math.min(a.y, b.y), Math.abs(a.x - b.x), Math.abs(a.y - b.y), true); drawing = null; });
  function placeCover(x, y, w, h, focus) {
    const c = el('<button class="cover" type="button" aria-label="Cover. Arrow keys move it, Shift and arrows resize it, Delete removes it."></button>');
    Object.assign(c.style, { left: x + "px", top: y + "px", width: w + "px", height: h + "px" });
    stage.appendChild(c); if (focus) c.focus();
    let drag = null;
    c.addEventListener("pointerdown", function (e) { drag = { dx: e.clientX - c.offsetLeft, dy: e.clientY - c.offsetTop }; c.setPointerCapture(e.pointerId); e.stopPropagation(); });
    c.addEventListener("pointermove", function (e) { if (!drag) return; c.style.left = (e.clientX - drag.dx) + "px"; c.style.top = (e.clientY - drag.dy) + "px"; });
    c.addEventListener("pointerup", function () { drag = null; });
    c.addEventListener("keydown", function (e) {
      const step = 8; let used = true;
      const L = c.offsetLeft, Tp = c.offsetTop, W = c.offsetWidth, H = c.offsetHeight;
      if (e.key === "Delete" || e.key === "Backspace") { c.remove(); stage.focus(); }
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
  function keyboardCover() { const r = stage.getBoundingClientRect(); placeCover(r.width / 2 - 80, r.height / 2 - 20, 160, 40, true); }

  // picking (the Mac's own open panel, PNG and JPEG, several at a time), dropping and pasting
  $("img-file").addEventListener("change", function () {
    const files = Array.from(this.files || []); this.value = "";
    if (S.replacing) { S.replacing = false; if (files.length) fromFile(files[0]).then(function (e) { Q.items.splice(Q.i, 0, e); openQueueItem(); }); else openQueueItem(); return; }
    beginAdding(files.map(fromFile));
  });
  let dragDepth = 0;
  const hasFiles = function (e) { return e.dataTransfer && Array.from(e.dataTransfer.types || []).indexOf("Files") >= 0; };
  $("win").addEventListener("dragenter", function (e) { if (!hasFiles(e)) return; e.preventDefault(); dragDepth++; if (canAddHere()) $("drop").hidden = false; });
  $("win").addEventListener("dragover", function (e) { if (!hasFiles(e)) return; e.preventDefault(); e.dataTransfer.dropEffect = canAddHere() ? "copy" : "none"; });
  $("win").addEventListener("dragleave", function () { dragDepth = Math.max(0, dragDepth - 1); if (!dragDepth) $("drop").hidden = true; });
  $("win").addEventListener("drop", function (e) {
    e.preventDefault(); dragDepth = 0; $("drop").hidden = true;
    const files = Array.from((e.dataTransfer && e.dataTransfer.files) || []); if (!files.length) return;
    if (!canAddHere()) { toast("Screenshots can be added while describing the problem or reviewing the report."); return; }
    beginAdding(files.map(fromFile));
  });
  document.addEventListener("paste", function (e) {
    const files = Array.from((e.clipboardData && e.clipboardData.files) || []).filter(function (f) { return /^image\//.test(f.type); });
    if (!files.length) return;
    if (!canAddHere()) { toast("Screenshots can be added while describing the problem or reviewing the report."); return; }
    e.preventDefault();
    beginAdding(files.map(function (f) { return fromFile(new File([f], f.name && f.name !== "image.png" ? f.name : "Pasted image.png", { type: f.type })); }));
  });

  // ---- review (round 39, plus round 40 §4: the Screenshots section and the two destinations) ----
  function note(text, glyph) { const n = el('<div class="note">' + (glyph || G.info) + "<p></p></div>"); n.querySelector("p").textContent = text; return n; }
  function boldNote(bold, rest, glyph) { const n = note("", glyph); const p = n.querySelector("p"); const b = document.createElement("b"); b.textContent = bold; p.appendChild(b); p.appendChild(document.createTextNode(" " + rest)); return n; }
  function shotRow(s, acts) {
    const row = el('<div class="shot" role="listitem"><span class="badge" aria-hidden="true"></span><img alt=""><div class="meta"><b></b><span></span></div><div class="acts"></div></div>');
    row.querySelector(".badge").textContent = s.n;
    row.querySelector("img").src = s.final.src;
    row.querySelector("img").alt = "Screenshot " + s.n + ": " + s.note;
    row.querySelector(".meta b").textContent = s.name;
    const covered = s.covers.filter(Boolean).length + (s.covers.length && !s.covers.some(Boolean) ? s.covers.length : 0);
    row.querySelector(".meta span").textContent = fmtDims(s.final.w, s.final.h) + " · " + fmtBytes(s.final.bytes) + " · " + (covered ? plural(covered, "area", "areas") + " covered" : "nothing covered") + (s.reduced || s.reducedFlag ? " · reduced to fit" : "");
    if (acts) row.querySelector(".acts").innerHTML = '<button class="btn link" data-act="inspect-shot" data-n="' + s.n + '" aria-label="Inspect screenshot ' + s.n + '">Inspect</button><button class="btn link" data-act="remove-shot" data-n="' + s.n + '" aria-label="Remove screenshot ' + s.n + '">Remove</button>';
    else row.querySelector(".acts").remove();
    return row;
  }
  function shotsSection(sec) {
    const n = S.shots.length;
    sec.innerHTML = '<div class="sec-head"><h3>Screenshots</h3><button class="btn" data-act="add-shots">' + IMG_GLYPH + "<span>Add screenshots</span></button></div>";
    if (!n) sec.appendChild(el('<p class="hint" style="margin-top:0">None. Add any you have already made; you check each one before it is used. You can also drop or paste them here.</p>'));
    else {
      const list = el('<div class="shots" role="list" aria-label="Screenshots, exactly as they will be shared"></div>');
      S.shots.forEach(function (s) { list.appendChild(shotRow(s, true)); });
      sec.appendChild(list);
      const sum = el('<p class="shots-sum"></p>');
      sum.textContent = plural(n, "screenshot") + ", " + fmtBytes(shotsBytes()) + " of 25 MB. Shared privately with the RichOS support team, not shown on GitHub, and kept for one year unless the team needs longer for an unresolved case. Inspect shows the exact file; changing a cover means approving that image again.";
      sec.appendChild(sum);
    }
    if (S.notAdded.length) {
      const na = el('<div class="notadded" role="status" style="margin-top:10px">' + G.warn + "<p></p></div>");
      na.querySelector("p").innerHTML = "<b>Not added:</b> " + S.notAdded.map(function (e) { return esc(e.file) + " (" + PROBLEM_WORD[e.problem] + ")"; }).join("; ") + ".";
      sec.appendChild(na);
    }
  }
  const GLOBE = '<svg viewBox="0 0 20 20" aria-hidden="true"><path fill="none" stroke="currentColor" stroke-width="1.5" d="M10 1.8a8.2 8.2 0 1 0 0 16.4 8.2 8.2 0 0 0 0-16.4zM1.8 10h16.4M10 1.8c2.6 2.4 3.6 5.1 3.6 8.2s-1 5.8-3.6 8.2c-2.6-2.4-3.6-5.1-3.6-8.2s1-5.8 3.6-8.2z"/></svg>';
  function pubWell() {
    const n = S.shots.length;
    if (!n) return GLOBE + "<div><p><b>This report will be public on GitHub. Review it before sending.</b></p><p>It goes to the RichOS team's public issue tracker. No GitHub account is needed. No screenshots are included. Send report approves this exact text and nothing else.</p></div>";
    return GLOBE + "<div><p><b>The report text will be public on GitHub. Screenshots are shared privately with the RichOS support team.</b></p><p>The text goes to the RichOS team's public issue tracker; no GitHub account is needed. The " + plural(n, "screenshot") + " " + (n === 1 ? "goes" : "go") + " to the support team only: the public report lists " + (n === 1 ? "one numbered link" : "numbered links") + " that only the team can open, kept for one year unless the team needs longer for an unresolved case. Send report approves this exact text and " + (n === 1 ? "this screenshot" : "these " + n + " screenshots") + ", nothing else.</p></div>";
  }
  function renderReview(v) {
    if (!v) return;
    S.review = v;
    // the approved screenshots, as the app holds them (the working copies of ones added in this window are kept)
    const held = {}; S.shots.forEach(function (s) { if (s.work && s.final) held[s.final.src] = s; });
    return refreshShots().then(function (list) {
      list.forEach(function (s, i) { const h = held[s.final.src]; if (h) { h.n = s.n; h.name = s.name; S.shots[i] = h; } });
      paintReview(v);
    });
  }
  function paintReview(v) {
    const notes = $("rv-notes"); notes.innerHTML = "";
    v.notes.forEach(function (t) { notes.appendChild(note(t)); });
    v.problems.forEach(function (p) {
      notes.appendChild(note("This needs a change before it can be sent: " + p.detail + (p.line ? " The line: “" + p.line + "”" : ""), G.warn));
    });
    const banner = $("rv-banner"); banner.innerHTML = "";
    const received = v.received || 0;
    if (v.mode === "revision-uncertain") {
      const b = boldNote("Editing a copy.", "Your first report (" + v.old_id + ") may already have reached the team, and RichDoctor is checking. This edit, text and screenshots, is saved on this Mac and cannot be sent as a replacement until that is known.", G.quest);
      b.style.border = "1px solid var(--gold)"; banner.appendChild(b);
    } else if (v.mode === "revision-notsent") {
      banner.appendChild(boldNote("Revised report.", "The earlier one (" + v.old_id + ") was not sent. Sending this one gives it a new report ID and retires the old one on this Mac."));
    } else if (v.mode === "revision-upload" && v.changed) {
      banner.appendChild(boldNote("Changed after an interrupted send.", "This report now differs from the earlier attempt (" + v.old_id + "), which created nothing public. Send report sends it as a revised report with a new report ID; the " + plural(received, "screenshot") + " that reached the support service under the old ID " + (received === 1 ? "is" : "are") + " sent again under the new one." + (S.removedUploaded ? " A screenshot you removed had already reached the support service: retiring the earlier report ID stops any further access to it, but a copy already viewed or downloaded by the team cannot be recalled." : "")));
    } else if (v.mode === "revision-upload") {
      banner.appendChild(boldNote("After an interrupted send.", "Nothing public was created for " + v.old_id + "; " + plural(received, "screenshot") + " of " + (v.attempt || S.shots.length) + " already reached the support service. Send this report unchanged and RichDoctor resumes with the missing screenshots. Change anything, text or screenshots, and it becomes a revised report with a new report ID."));
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
    v.after.forEach(editable);
    const shots = el('<div class="sec"></div>'); shotsSection(shots); host.appendChild(shots);
    const n = S.shots.length;
    const foot = el('<div class="sec"><p class="soft" style="margin:0"></p></div>');
    foot.querySelector("p").textContent = "Footer, included as sent: " + (n ? "Screenshots: " + n + ", " + (n === 1 ? "one numbered link" : "numbered links") + " only the RichOS support team can open · " : "") + v.footer;
    host.appendChild(foot);
    $("rv-exact-text").textContent = v.exact;
    $("rv-pub").innerHTML = pubWell();
    show("s-review");
    setTimeout(function () { $("rv-title").focus({ preventScroll: true }); }, 60);
  }
  function editSection(key, text) {
    invoke("edit", { section: key, text: text }).then(function (v) {
      const open = document.querySelector(".disc") && document.querySelector(".disc").getAttribute("aria-expanded") === "true";
      renderReview(v).then(function () { if (open) { const d = document.querySelector(".disc"); d.setAttribute("aria-expanded", "true"); d.nextElementSibling.hidden = false; } });
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

  // ---- delivery results (round 39, plus round 40 §5's states and the per-image list) -----------
  function kv(id, extra) {
    return '<dl class="kv"><dt>Report ID</dt><dd class="mono">' + esc(id) + (extra || "") + "</dd><dt>On this Mac</dt><dd>" + storageLine() + "</dd></dl>";
  }
  function renderChecking(id) {
    $("result-body").innerHTML = '<div class="result"><div class="mark gold"><span class="spin" aria-hidden="true"></span></div><div><h1>Checking status…</h1><p></p></div></div>';
    $("result-body").querySelector("p").textContent = "Asking the reporting service about report " + id + ". Nothing is sent again.";
    $("result-foot").innerHTML = "";
    show("s-result");
  }
  function viewShots(v) { return (v.shots || []).map(function (f) { const s = S.shots.find(function (x) { return x.n === f.n; }); return s || fromSaved(Object.assign({ url: "" }, f)); }); }
  // states: per image { st: "wait"|"live"|"ok"|"earlier"|"stopped"|"unsure", p: 0..1, word }
  function shotRows(v, states) {
    const list = el('<div class="shots" role="list" aria-label="Screenshots"></div>');
    viewShots(v).forEach(function (s, i) {
      const st = states[i] || { st: "wait", p: 0, word: "Waiting" };
      const glyph = st.st === "ok" || st.st === "earlier" ? G.checkc : st.st === "stopped" ? G.dash : st.st === "unsure" ? G.quest : st.st === "live" ? '<span class="spin" aria-hidden="true"></span>' : "";
      const row = el('<div class="shot" role="listitem"><span class="badge" aria-hidden="true"></span><img alt=""><div class="meta"><b></b><span></span><div class="pbar" aria-hidden="true"><i></i></div></div><span class="state"></span></div>');
      row.querySelector(".badge").textContent = s.n;
      if (st.st === "wait") row.querySelector(".badge").classList.add("soft");
      if (s.final.src) row.querySelector("img").src = s.final.src; else row.querySelector("img").remove();
      row.querySelector("img") && (row.querySelector("img").alt = "Screenshot " + s.n);
      row.querySelector(".meta b").textContent = s.name;
      row.querySelector(".meta span").textContent = fmtBytes(s.final.bytes);
      const bar = row.querySelector(".pbar");
      if (st.st === "ok" || st.st === "earlier") bar.classList.add("done");
      if (st.st === "stopped") bar.classList.add("stopped");
      bar.querySelector("i").style.width = Math.round(st.p * 100) + "%";
      const word = row.querySelector(".state"); word.className = "state " + (st.st === "earlier" ? "ok" : st.st); word.innerHTML = glyph; word.appendChild(document.createTextNode(st.word));
      list.appendChild(row);
    });
    return list;
  }
  function rowStates(rows, finalWords) {
    return (rows || []).map(function (r) {
      const p = r.total ? Math.min(1, r.sent / r.total) : 0;
      if (r.st === "ok") return { st: "ok", p: 1, word: finalWords || "Received" };
      if (r.st === "earlier") return { st: "earlier", p: 1, word: "Received earlier" };
      if (r.st === "live") return { st: "live", p: p, word: Math.round(p * 100) + "%" };
      if (r.st === "stopped") return { st: "stopped", p: p, word: "Stopped" };
      return { st: "wait", p: 0, word: "Waiting" };
    });
  }
  function paintUpload(v) {
    const host = $("up-rows"); if (!host) return;
    const rows = v.rows || [];
    const n = rows.length, got = rows.filter(function (r) { return r.st === "ok" || r.st === "earlier"; }).length;
    const live = rows.findIndex(function (r) { return r.st === "live"; });
    host.innerHTML = ""; host.appendChild(shotRows(v, rowStates(rows)));
    $("up-sum").textContent = plural(got, "screenshot") + " of " + n + " received" + (live >= 0 ? " · sending screenshot " + (live + 1) : "");
  }
  function approvedWell(v) {
    const n = (v.shots || []).length;
    const ap = el('<div class="approved well"><p class="lab"></p><p class="t"></p><p class="x"></p><p class="more"></p></div>');
    ap.querySelector(".lab").textContent = v.kind === "submitted" ? "Your report, as sent" : "Your report, exactly as you approved it";
    ap.querySelector(".t").textContent = v.title;
    ap.querySelector(".x").textContent = v.first;
    if (n) {
      const strip = el('<div class="strip" aria-hidden="true"></div>');
      viewShots(v).forEach(function (s) { if (s.final.src) { const im = document.createElement("img"); im.src = s.final.src; im.alt = ""; strip.appendChild(im); } });
      ap.insertBefore(strip, ap.querySelector(".more"));
    }
    ap.querySelector(".more").textContent = v.summary.replace("the report ID footer", (n ? plural(n, "screenshot") : "no screenshots") + ", the report ID footer").replace("The exact text is", n ? "The exact text and the screenshots are" : "The exact text is");
    return ap;
  }
  function renderResult(v) {
    if (!v) return;
    const prev = S.delivery;
    S.delivery = v;
    // Cancel during Sending screenshots returns to the review with everything kept (round 40 §5).
    if (v.kind === "notsent" && v.extra.canceled && S.cancelAsked) {
      S.cancelAsked = false;
      toast("Canceled. Your report and screenshots are kept on this Mac; nothing public was created.");
      invoke("review").then(renderReview);
      return;
    }
    if (v.kind === "uploading" && prev && prev.kind === "uploading" && !$("s-result").hidden && $("up-rows")) { paintUpload(v); return; }
    const body = $("result-body"), foot = $("result-foot");
    const id = v.report_id;
    const n = (v.shots || []).length;
    let html = "", acts = "";
    if (v.kind === "uploading") {
      html = '<div class="result"><div class="mark gold"><span class="spin" aria-hidden="true"></span></div><div><h1>Sending screenshots…</h1><p>Your ' + plural(n, "approved screenshot") + " " + (n === 1 ? "goes" : "go") + " to the RichOS support team first. The public report is created only after " + (n === 1 ? "it has" : "all of them have") + ' arrived; nothing public exists yet.</p><p class="soft" id="up-sum" aria-live="polite"></p>' + kv(id) + '<div id="up-rows" style="margin-top:14px"></div></div></div>';
      acts = '<p class="grow">Cancel keeps the report and the screenshots on this Mac. Nothing public is created.</p><button class="btn" data-act="cancel-upload">Cancel</button>';
    } else if (v.kind === "sending") {
      html = '<div class="result"><div class="mark gold"><span class="spin" aria-hidden="true"></span></div><div><h1>Sending your report…</h1><p>' + (n ? "All " + plural(n, "screenshot") + " arrived. " : "") + "Your approved report is on its way to the RichOS team’s reporting service. You can close this window: once the service accepts it, delivery continues on its own.</p>" + kv(id) + "</div></div>";
      acts = '<p class="grow">The result is kept in Saved reports.</p>';
    } else if (v.kind === "created-checking") {
      html = '<div class="result"><div class="mark gold"><span class="spin" aria-hidden="true"></span></div><div><h1>Report created; checking screenshots</h1><p>Your report is on GitHub. RichDoctor is confirming that the support team can open all ' + plural(n, "screenshot") + ', which takes up to two minutes. No second report is created.</p><dl class="kv"><dt>Report ID</dt><dd class="mono">' + esc(id) + '</dd><dt>Public report</dt><dd><a href="#" data-act="view-report" class="issue"></a></dd><dt>On this Mac</dt><dd>' + storageLine() + "</dd></dl></div></div>";
      acts = '<p class="grow">You can close this window. The result is in Saved reports.</p><button class="btn" data-act="view-report">' + G.out + "<span>View report</span></button>";
    } else if (v.kind === "created") {
      const c = v.extra.confirmed, x = v.extra.expected;
      const missing = Array.from({ length: Math.max(0, x - c) }, function (_, i) { return c + i + 1; });
      html = '<div class="result"><div class="mark" style="color:var(--accent-text)">' + G.quest + '</div><div><h1>Report created; checking screenshots</h1><p class="why"></p><dl class="kv"><dt>Report ID</dt><dd class="mono">' + esc(id) + ' <span class="soft">· include it if you ask support for help</span></dd><dt>Public report</dt><dd><a href="#" data-act="view-report" class="issue"></a></dd><dt>Screenshots</dt><dd>' + c + " of " + x + " confirmed</dd><dt>On this Mac</dt><dd>" + storageLine() + '</dd></dl><div id="created-rows" style="margin-top:14px"></div></div></div>';
      acts = '<button class="btn primary" data-act="check-status">Check status</button><button class="btn" data-act="email">Email support</button><div class="grow"></div><button class="btn" data-act="view-report">' + G.out + '<span>View report</span></button><button class="btn link" data-act="saved">Saved reports</button>';
      S.createdMissing = missing;
    } else if (v.kind === "checking") {
      html = '<div class="result"><div class="mark gold"><span class="spin" aria-hidden="true"></span></div><div><h1>Checking whether your report was received</h1><p>The reporting service did not give a clear answer. RichDoctor is checking whether the report arrived, which takes up to two minutes. It will not send the report a second time.' + (n ? " Your " + plural(n, "screenshot") + " already reached the support service under this report ID." : "") + "</p>" + kv(id) + "</div></div>";
      acts = '<p class="grow">You can close this window. The result is in Saved reports.</p><button class="btn" data-act="saved">Saved reports</button>';
    } else if (v.kind === "submitted") {
      html = '<div class="result"><div class="mark" style="color:var(--success)">' + G.check + '</div><div><h1>Report submitted</h1><p>The RichOS team has your report' + (n ? " and your " + plural(n, "screenshot") : "") + '. This does not yet mean the cause is known or the issue is fixed.</p><dl class="kv"><dt>Report ID</dt><dd class="mono">' + esc(id) + ' <span class="soft">· include it if you ask support for help</span></dd><dt>Public report</dt><dd><a href="#" data-act="view-report" class="issue"></a></dd><dt>Screenshots</dt><dd>' + (n ? '<span class="status ok">' + G.checkc + plural(n, "screenshot") + ' received</span> <span class="soft">· shared privately with RichOS support, kept for one year</span>' : "none") + "</dd></dl></div></div>";
      acts = '<button class="btn link" data-act="saved">Saved reports</button><div class="grow"></div><button class="btn" data-act="view-report">' + G.out + '<span>View report</span></button><button class="btn primary" data-act="done">Done</button>';
    } else if (v.kind === "notsent") {
      const up = v.extra.upload && n > 0;
      html = '<div class="result"><div class="mark" style="color:var(--attention)">' + G.dash + '</div><div><h1>Not sent yet</h1><p class="why"></p><p class="kept"></p>' + kv(id) + (up ? '<div id="ns-rows" style="margin-top:14px"></div><div id="ns-reduce"></div>' : "") + "</div></div>";
      if (up) acts = '<button class="btn primary" data-act="resume-upload">Resume sending</button><button class="btn" data-act="review-shots">Review screenshots</button><button class="btn" data-act="email">Email support</button><div class="grow"></div><button class="btn link" data-act="saved">Saved reports</button>';
      else acts = (v.extra.review ? '<button class="btn primary" data-act="edit-notsent">Review report</button>' : (v.extra.try_again ? '<button class="btn primary" data-act="try-again">Try again</button>' : "")) + '<button class="btn" data-act="save-file">Save report</button><button class="btn" data-act="email">Email support</button><div class="grow"></div><button class="btn link" data-act="saved">Saved reports</button>';
    } else if (v.kind === "unconfirmed") {
      html = '<div class="result"><div class="mark" style="color:var(--accent-text)">' + G.quest + '</div><div><h1>Delivery could not be confirmed</h1><p>Your report may already have reached the RichOS team. RichDoctor could not confirm it within two minutes. To avoid filing it twice, Send report is not offered; Check status asks the service about this same report, and RichDoctor checks again whenever you open it.</p>' + kv(id) + '<p class="soft" style="margin-top:12px"></p></div></div>';
      acts = '<button class="btn primary" data-act="check-status">Check status</button><button class="btn" data-act="email">Email support</button><button class="btn" data-act="edit-uncertain">Edit report</button><div class="grow"></div><button class="btn link" data-act="saved">Saved reports</button>';
      if (prev && prev.kind === "unconfirmed" && v.extra.checked_at) toast("Still not confirmed, checked at " + v.extra.checked_at + ". The report ID has not changed.");
    }
    body.innerHTML = html; foot.innerHTML = acts;
    if (v.kind === "uploading") paintUpload(v);
    if (v.kind === "submitted" || v.kind === "created-checking" || v.kind === "created") body.querySelector(".issue").textContent = v.extra.issue_url.replace("https://", "");
    if (v.kind === "created") {
      const c = v.extra.confirmed, x = v.extra.expected;
      body.querySelector(".why").textContent = "Your report is on GitHub, but RichDoctor could not confirm within two minutes that the support team can open " + (x - c === 1 ? "screenshot " + x + " of " + x : (x - c) + " of the " + x + " screenshots") + ". The team has been told to check this report. Your screenshots stay on this Mac, and no second report is created; Check status asks again about this same report.";
      const states = viewShots(v).map(function (s, i) { return i < c ? { st: "ok", p: 1, word: "Confirmed" } : { st: "unsure", p: 1, word: "Not confirmed" }; });
      $("created-rows").appendChild(shotRows(v, states));
    }
    if (v.kind === "unconfirmed") body.querySelector(".soft").textContent = (n ? "Your " + plural(n, "screenshot") + " reached the support service under this report ID before the report was sent; they are not sent again. " : "") + "If you need help sooner, email support and include the report ID. Edits you make are saved on this Mac and cannot be sent as a replacement until RichDoctor knows whether the first report arrived.";
    if (v.kind === "notsent") {
      const up = v.extra.upload && n > 0;
      const rows = (prev && prev.rows && prev.report_id === v.report_id && prev.rows.length) ? (v.rows && v.rows.some(function (r) { return r.st === "stopped" || r.st === "live"; }) ? v.rows : prev.rows) : v.rows;
      const states = rowStates(rows);
      const stoppedAt = states.findIndex(function (s) { return s.st === "stopped" || s.st === "live"; });
      const got = states.filter(function (s) { return s.st === "ok" || s.st === "earlier"; }).length;
      if (up && !v.extra.canceled) {
        const k = stoppedAt >= 0 ? stoppedAt + 1 : (v.extra.item ? +String(v.extra.item).replace(/\D+/g, "") : got + 1);
        const arrived = got ? (got === 1 ? "Screenshot 1 arrived and does not need to be sent again." : plural(got, "screenshot") + " arrived and do not need to be sent again.") : "";
        body.querySelector(".why").textContent = (v.extra.toolong ? "Screenshot " + k + " of " + n + " did not finish within five minutes, the longest RichDoctor waits for one image. " : "Sending stopped while screenshot " + k + " of " + n + " was on its way: no progress for 60 seconds, and the cause is not known. ") + (arrived ? arrived + " " : "") + "The public report was not created, because it is created only once every screenshot has arrived.";
        if (stoppedAt >= 0) states[stoppedAt].st = "stopped", states[stoppedAt].word = "Stopped";
        if (v.extra.toolong) {
          const r = el('<div class="row" style="margin-top:12px"><span class="grow"></span><button class="btn" data-act="reduce-shot"></button></div>');
          r.querySelector("span").textContent = "Screenshot " + k + " may be too large for this connection. A smaller copy needs your approval again before it is sent.";
          r.querySelector("button").textContent = "Reduce screenshot " + k; r.querySelector("button").dataset.n = k;
          $("ns-reduce").appendChild(r);
        }
      } else body.querySelector(".why").textContent = v.extra.reason;
      body.querySelector(".kept").textContent = up ? "Nothing public was created. Your approved report and all " + plural(n, "screenshot") + " are kept exactly as you reviewed them." : "Nothing was sent. Your approved report" + (n ? " and its " + plural(n, "screenshot") + " are" : " is") + " kept exactly as you reviewed " + (n ? "them" : "it") + ".";
      if (up) $("ns-rows").appendChild(shotRows(v, states));
    }
    if (v.kind !== "uploading" && v.kind !== "created-checking") body.appendChild(approvedWell(v));
    show("s-result");
  }

  // ---- saved reports -------------------------------------------------------------------------
  function renderSaved() {
    invoke("saved").then(function (data) {
      const list = $("saved-list"); list.innerHTML = "";
      const glyph = { submitted: G.checkc, unconfirmed: G.quest, created: G.quest, notsent: G.dash, saved: G.doc };
      const cls = { submitted: "ok", unconfirmed: "unsure", created: "unsure", notsent: "notsent", saved: "saved" };
      const actWords = { view: "View report", resume: "Resume", check: "Check status", try: "Try again", review: "Review report", "resume-upload": "Resume sending" };
      data.rows.concat(data.sent).forEach(function (r) {
        const it = el('<div class="item"><div class="t"></div><div class="acts"></div><div class="d"></div><div class="st"></div></div>');
        it.querySelector(".t").textContent = r.title;
        it.querySelector(".d").textContent = r.date + (r.left ? " · " + r.left : "") + (r.shots_words ? " · " + r.shots_words : "");
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
          del.dataset.act = "delete"; del.dataset.id = r.id; del.dataset.kind = r.state === "submitted" ? "sent" : r.state === "unconfirmed" || r.state === "created" ? "unsure" : "draft";
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
      return refreshShots().then(function () {
        if (d.delivery) { renderResult(d.delivery); if (then) then(d); return d; }
        if (d.stage === "review" && d.has_report) { invoke("review").then(renderReview); return d; }
        startConversation(d.without_ai ? "Preparing without AI" : "Investigating with your Claude Code account");
        replay(d.conversation);
        invoke("resume_pending");
        const last = d.conversation[d.conversation.length - 1];
        if (!d.pending_question && !(last && (last.t === "ai-failed"))) canceledCard();
        if (d.pending_question) showAnswer();
        return d;
      });
    }).catch(function (err) { toast(String(err)); });
  }

  // ---- email support and saving (round 40 §7) ----------------------------------------------------
  function filesWords(plan) {
    const n = plan.count;
    return plural(n, "approved screenshot") + " attached as files (" + (n === 1 ? plan.names[0] : plan.names[0] + " to " + plan.names[n - 1]) + ", " + fmtBytes(plan.bytes) + ")";
  }
  function openEmail() {
    const b = $("em-body"), f = $("em-foot");
    const id = (S.delivery && S.delivery.report_id) || (S.review && S.review.report_id) || "";
    Promise.all([invoke("mail_available"), invoke("email_plan")]).then(function (got) {
      if (got[1].error) { fail(got[1].error); return; }
      const mail = got[0], plan = got[1], n = plan.count, big = plan.parts > 1;
      if (mail) {
        b.innerHTML = '<h2 id="em-h">Email support</h2><p class="lead"></p>' + (big ? '<div class="note">' + G.info + '<p class="big"></p></div>' : "") + '<p class="soft" style="margin:0">Emailing is not the same as the public report: it does not create a GitHub report or change this report’s delivery state.</p>';
        b.querySelector(".lead").innerHTML = "RichDoctor opens a new email in Mail to <b>support@richos.ceo</b> with the reviewed report text, its report ID (<span></span>)" + (n ? ", and the " + esc(filesWords(plan)) : "") + ". You read it and send it yourself. The larger diagnostic packet" + (n ? " and the original images are" : " is") + " not included.";
        b.querySelector(".lead span").textContent = id;
        if (big) b.querySelector(".big").innerHTML = "<b>Your screenshots total " + fmtBytes(plan.bytes) + ".</b> Email services often limit a message's size. RichDoctor can save " + plan.parts + " numbered packages of up to 10 MB each instead, each with the exact screenshot files, the report text and the report ID; send one package per email to support@richos.ceo. The 10 MB figure is a convenience, not a promise about your email service.";
        f.innerHTML = '<div class="grow"></div><button class="btn" data-act="close-sheet">Cancel</button>' + (big ? '<button class="btn" data-act="open-mail-anyway">Open in Mail anyway</button><button class="btn primary" data-act="save-packages">Save ' + plan.parts + " packages</button>" : '<button class="btn primary" data-act="open-mail">Open in Mail</button>');
      } else {
        b.innerHTML = '<h2 id="em-h">Email support</h2><p class="lead"></p><p><span class="addr">support@richos.ceo <button class="btn link" data-act="copy-addr" style="height:28px">Copy</button></span></p><p class="how"></p><p class="soft" style="margin:0"></p>';
        b.querySelector(".lead").textContent = "No email app is set up on this Mac. Send the report from any email account you use, to this address, and include the report ID " + id + ":";
        b.querySelector(".how").innerHTML = (n ? "<b>Save report and screenshots</b> writes a folder with the reviewed text and the " + esc(filesWords(plan)) + "; attach that folder's files to your email. " : "<b>Save report</b> writes the reviewed text as a file; attach it to your email. ") + (big ? "Because they total " + fmtBytes(plan.bytes) + ", it saves " + plan.parts + " numbered packages of up to 10 MB each, each with the report text and ID; send one per email. " : "") + "Copy report copies the text only" + (n ? "; the screenshots still need attaching" : "") + ".";
        b.querySelector(".soft").textContent = "The larger diagnostic packet" + (n ? " and the original images are" : " is") + " not included. Emailing does not create a GitHub report or change this report’s delivery state.";
        f.innerHTML = '<button class="btn" data-act="copy-report">Copy report</button><button class="btn" data-act="save-file">' + (n ? "Save report and screenshots" : "Save report") + '</button><div class="grow"></div><button class="btn primary" data-act="close-sheet">Done</button>';
      }
      S.plan = plan;
      openSheet("sheet-email"); setTimeout(function () { f.querySelector(".btn.primary").focus(); }, 30);
    });
  }
  function openSave() {
    invoke("email_plan").then(function (plan) {
      if (plan.error) { fail(plan.error); return; }
      const n = plan.count, big = plan.parts > 1;
      $("sv-h").textContent = n ? "Save report and screenshots" : "Save report as a text file";
      $("sv-p").textContent = n
        ? "A folder with the reviewed report text (with its report ID) and the " + plural(n, "approved screenshot") + ", exactly as you approved " + (n === 1 ? "it" : "them") + ", named " + (n === 1 ? plan.names[0] : plan.names[0] + " to " + plan.names[n - 1]) + " (" + fmtBytes(plan.bytes) + ")." + (big ? " Because they total more than 10 MB, it is saved as " + plan.parts + " numbered packages of up to 10 MB each, each with the report text and ID." : "") + " The larger diagnostic packet and the original images are not included. macOS asks where to save it."
        : "The reviewed report text, with its report ID. No screenshots were added. The larger diagnostic packet on this Mac is not included. macOS asks where to save it.";
      S.plan = plan;
      openSheet("sheet-save"); setTimeout(function () { $("sheet-save").querySelector(".btn.primary").focus(); }, 30);
    });
  }

  // ---- actions -------------------------------------------------------------------------------
  function fail(err) { toast(String(err)); }
  function begin(withoutAi) {
    const desc = $("desc").value.trim();
    if (!desc) { $("desc").focus(); toast("Describe what went wrong first. A sentence or two is enough."); return; }
    S.shots = []; S.notAdded = []; S.removedUploaded = false;
    startConversation(withoutAi ? "Preparing without AI" : "Investigating with your Claude Code account");
    invoke("start", { description: desc, noDiagnostics: $("no-diag").checked, withoutAi: withoutAi }).catch(function (err) { fail(err); renderFirst(); });
  }
  function sendAnswer(text) {
    if (!text || S.busy) return;
    clearChoices(); $("answer").hidden = true;
    invoke("answer", { text: text });
  }
  function reopenReview() { invoke("review").then(renderReview); }
  document.addEventListener("click", function (e) {
    const ch = e.target.closest("[data-choice]");
    if (ch) { sendAnswer(ch.dataset.choice); return; }
    const b = e.target.closest("[data-act]"); if (!b || b.disabled) return;
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
      // screenshots
      case "add-shots": $("img-file").click(); break;
      case "add-cover": keyboardCover(); break;
      case "img-next": showCheck(cur(), false); break;
      case "img-back": { const e2 = cur(); showCover(e2); break; }
      case "img-use": useCurrent(); break;
      case "img-done": Q.i++; openQueueItem(); break;
      case "img-edit": inspectShot(cur().n, true); break;
      case "img-skip": skipCurrent(); break;
      case "img-replace": Q.items.splice(Q.i, 1); S.replacing = true; $("img-file").click(); break;
      case "img-remove": {
        const e4 = cur();
        if (S.shots.indexOf(e4) >= 0) {
          invoke("remove_image", { index: e4.n - 1 }).then(function () { closeSheets(); toast("Screenshot removed. It will not be shared."); reopenReview(); }).catch(fail);
        } else { e4.skipped = true; Q.i++; openQueueItem(); toast("Screenshot left out. Your answers are kept."); }
        break;
      }
      case "img-cancel-all": Q.items = Q.items.filter(function (x) { return x.approved; }); Q.i = Q.items.length; finishQueue(); break;
      case "queue-go": Q.i = +b.dataset.i; openQueueItem(); break;
      case "inspect-shot": inspectShot(+b.dataset.n, false); break;
      case "remove-shot": {
        const n = +b.dataset.n;
        const up = S.review && S.review.mode === "revision-upload" && n <= (S.review.received || 0);
        if (up) S.removedUploaded = true;
        invoke("remove_image", { index: n - 1 }).then(function () {
          toast(up ? "Screenshot removed from this report. It had already reached the support service; see the note at the top." : "Screenshot removed. It will not be shared.");
          reopenReview();
        }).catch(fail);
        break;
      }
      case "reduce-shot": invoke("review").then(function (v) { S.review = v; return refreshShots(); }).then(function () { reduceShot(+b.dataset.n); }); break;
      case "close-sheet": closeSheets(); break;
      case "toggle-exact": {
        const on = $("rv-exact").hidden;
        $("rv-exact").hidden = !on; $("rv-sections").hidden = on; $("rv-title").hidden = on;
        b.setAttribute("aria-pressed", on ? "true" : "false"); b.textContent = on ? "Back to the readable view" : "Show as it will be sent"; break;
      }
      case "disclose": { const open = b.getAttribute("aria-expanded") !== "true"; b.setAttribute("aria-expanded", open ? "true" : "false"); b.nextElementSibling.hidden = !open; break; }
      case "remove-ex": case "restore-ex":
        invoke("excerpt", { id: b.dataset.id, removed: act === "remove-ex" }).then(function (v) { return renderReview(v); }).then(function () { const d = document.querySelector(".disc"); d.setAttribute("aria-expanded", "true"); d.nextElementSibling.hidden = false; });
        break;
      case "back-conv": show("s-conv"); break;
      case "save-later": invoke("save_later").then(function (t) { toast(S.shots.length && /^Saved on this Mac/.test(t) ? "Saved on this Mac, with the " + plural(S.shots.length, "screenshot") + ". Nothing was sent." : t); }); break;
      case "send": invoke("send_report").catch(fail); break;
      case "save-edit": invoke("save_later").then(function () { toast("Edit saved on this Mac. The first report is still being checked."); invoke("delivery").then(renderResult); }); break;
      case "try-again": invoke("try_again").catch(fail); break;
      case "resume-upload": invoke("try_again").catch(fail); break;
      case "cancel-upload": S.cancelAsked = true; invoke("cancel_upload").then(function (ok) { if (!ok) S.cancelAsked = false; }); break;
      case "review-shots": case "edit-notsent": case "edit-uncertain": reopenReview(); break;
      case "check-status": invoke("check_status").catch(fail); break;
      case "email": openEmail(); break;
      case "open-mail": case "open-mail-anyway": {
        const n = S.plan ? S.plan.count : S.shots.length;
        invoke("open_mail", { anyway: act === "open-mail-anyway" }).then(function () { closeSheets(); toast("A new email to support@richos.ceo opened in Mail" + (n ? ", with the " + plural(n, "screenshot") + " attached" : "") + ". Send it when you are ready."); }).catch(function (err) {
          if (String(err) === "parts") { closeSheets(); openSave(); } else fail(err);
        });
        break;
      }
      case "save-packages": closeSheets(); invoke("save_report_file").then(function () { toast("Saved " + (S.plan ? S.plan.parts : 2) + " packages where you chose. Send one per email to support@richos.ceo."); }).catch(function (err) { if (String(err) !== "canceled") fail(err); }); break;
      case "copy-addr": invoke("copy_address").then(function (ok) { toast(ok ? "Address copied: support@richos.ceo" : "The address could not be copied."); }); break;
      case "copy-report": invoke("copy_report").then(function (id) { const n = S.shots.length; toast("Report text copied, with the report ID " + id + "." + (n ? " The " + plural(n, "screenshot") + " still need attaching." : "")); }).catch(fail); break;
      case "save-file": closeSheets(); openSave(); break;
      case "confirm-save": closeSheets(); invoke("save_report_file").then(function (id) {
        const n = S.plan ? S.plan.count : S.shots.length;
        toast(/ in \d+ parts$/.test(id) ? "Saved " + id.replace(/^.* in (\d+) parts$/, "$1") + " packages where you chose. Send one per email to support@richos.ceo." : n ? "Saved the folder “RichOS report " + id + "” where you chose: the report text and " + plural(n, "screenshot") + "." : "Saved RichOS report " + id + " where you chose.");
      }).catch(function (err) { if (String(err) !== "canceled") fail(err); }); break;
      case "view-report": e.preventDefault(); if (S.delivery && S.delivery.extra.issue_url) invoke("open_issue", { url: S.delivery.extra.issue_url }).catch(fail); break;
      case "done": invoke("close_help"); $("desc").value = ""; S.shots = []; S.notAdded = []; invoke("boot").then(function (v) { S.boot = v; renderFirst(); }); break;
      case "saved": renderSaved(); break;
      case "close-saved": invoke("boot").then(function (v) { S.boot = v; renderFirst(); }); break;
      case "row-view": invoke("open_issue", { url: b.dataset.url }).catch(fail); break;
      case "row-resume": case "row-check": case "row-try": case "row-review": case "row-resume-upload":
        openSaved(b.dataset.id, function () {
          if (act === "row-try" || act === "row-resume-upload") invoke("try_again").catch(fail);
          if (act === "row-review") reopenReview();
          if (act === "row-check") invoke("check_status").catch(fail);
        });
        break;
      case "delete": {
        S.deleteId = b.dataset.id; S.deleteKind = b.dataset.kind;
        $("del-p").textContent = S.deleteKind === "sent" ? "The local copy, text and screenshots, is deleted. The public report on GitHub stays, and its link is kept here. The screenshots the support team received stay with the team." : S.deleteKind === "unsure" ? "The local copy, text and screenshots, is deleted. If the report already reached the team, it stays public and the screenshots the team received stay with the team; deleting here does not withdraw them." : "The local copy, text and screenshots, is deleted. Nothing public was created, so nothing public exists.";
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
  document.addEventListener("keydown", function (e) {
    if (e.key !== "Escape") return;
    const open = Array.from(document.querySelectorAll(".sheet-scrim")).find(function (s) { return !s.hidden; });
    if (!open) return;
    e.preventDefault();
    // Escape is Cancel for the whole image queue: the images approved so far are kept.
    if (open.id === "sheet-image") { Q.items = Q.items.filter(function (x) { return x.approved; }); Q.i = Q.items.length; finishQueue(); return; }
    closeSheets();
  });

  // ---- boot ----------------------------------------------------------------------------------
  invoke("boot").then(function (v) {
    S.boot = v;
    if (new URLSearchParams(location.search).get("screen") === "saved") renderSaved(); else renderFirst();
  });
})();

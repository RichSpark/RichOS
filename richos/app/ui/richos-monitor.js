"use strict";
// MY RICHOS, the activity monitor under "My Mac" on the navigation rail (design round 109,
// richos-hq design/mockups/rounds/round-109; the CEO approved it on 2026-11-26).
//
// The CEO, 2026-11-26: "I need a way to track or see at a glance 1) whether or not tokens are
// being consumed now 2) the velocity with which they've been being consumed in the last say 1
// minute and last 5 minutes and last 15 minutes 3) see more detail on those stats if desired 4)
// also see how much memory/RAM is being used now by each app of the RichOS suite individuall and
// all combined and what was the average RAM usage in the last 1-2 minutes or so (maybe a little
// graph for that...)" and "additionally to "My Mac" activity monitor we'd have a "My RichOS"
// activity monitor underneath on the navigation rail." On the build: "I really hope that those
// graphs can be drawn without using much CPU/RAM resources."
//
// Three depths:
//   1. THE RAIL, with no hover and no click: a square in the Mac square's own frame holding the
//      last minute of token use, a light that is lit while tokens are being used, and under it
//      the speed over the last minute as one number, "1.3M", or "Idle". (The CEO, 2026-11-26:
//      'You can remove "/min" from the nav rail. The user will see that the number is tokens per
//      minute after hovering.')
//   2. THE CARD, on hover, keyboard focus or a click (pinned), as the Mac square's card: the
//      speed over the last minute, 5 minutes and 15 minutes with the tokens used in each, a
//      15-minute graph; then memory, all RichOS apps together, its 2-minute average and graph,
//      and each app.
//   3. THE DETAILS, by clicking either half of the card: a sheet with Tokens (a graph over 15
//      minutes, an hour or a day, which chats used them, by model, by kind) and Memory (the
//      graph, and each app with its own 2-minute line, average and highest).
//
// THE NUMBERS ARE THE APP'S OWN (`richos_monitor`, richos-core `my_richos.rs`): tokens from the
// usage of every request the app's own Rich, team and helpers made, as each lease's reader
// counts it; memory from RichDoctor's per-app reading (`suite_memory::Live`), every two seconds.
// Both start when the switch is turned on; nothing is counted or read while it is off.
//
// CHEAP TO DRAW: nothing here animates on a frame loop. One timer ticks once a second while the
// switch is on and the window is visible, and asks the app once; the square is 10 bars; the card
// and the sheet are painted only while open, once per tick. The one moving thing is the light's
// fade, a CSS opacity animation. OFF IS OFF: with the switch off this file has read the switch
// once, put nothing in the document and runs no timer; turning it off stops the timer.
(function () {
  const bridge = window.RichBridge;
  const slot = document.getElementById("nr-richos");
  if (!bridge || !slot) return;

  const css = (name) => getComputedStyle(document.documentElement).getPropertyValue(name).trim();
  const esc = (s) => String(s).replace(/[&<>"]/g, (ch) => ({ "&": "&amp;", "<": "&lt;", ">": "&gt;", '"': "&quot;" })[ch]);
  const MB = 1048576;

  // ---- words ----------------------------------------------------------------------------------
  /// 850, 8.5k, 85k, 752k, 1.26M. `tight` is the rail's one decimal: 1.3M.
  function tokens(n, tight) {
    n = Math.max(0, n || 0);
    if (n < 999.5) return String(Math.round(n));
    if (n < 9950) return (n / 1000).toFixed(1) + "k";
    if (n < 999500) return Math.round(n / 1000) + "k";
    if (n < 9995000) return (n / 1e6).toFixed(tight ? 1 : 2) + "M";
    return (n / 1e6).toFixed(1) + "M";
  }
  function spoken(n) {
    if (n < 999.5) return Math.round(n) + "";
    if (n < 999500) return Math.round(n / 1000) + " thousand";
    return (n / 1e6).toFixed(1) + " million";
  }
  /// Bytes as the card says them; a reading the app could not take is a hyphen, never 0.
  function ram(bytes) {
    if (bytes == null) return "-";
    const mb = bytes / MB;
    if (mb < 999.5) return Math.round(mb) + " MB";
    return (mb / 1024).toFixed(1) + " GB";
  }
  function ago(sec) {
    if (!isFinite(sec)) return "Not used yet";
    if (sec < 90) return "Last used a minute ago";
    if (sec < 3600) return "Last used " + Math.round(sec / 60) + " minutes ago";
    const hrs = Math.round(sec / 3600);
    return "Last used " + hrs + (hrs === 1 ? " hour ago" : " hours ago");
  }
  const clock = (ms, secs) => new Date(ms).toLocaleTimeString("en-US", secs ? { hour: "numeric", minute: "2-digit", second: "2-digit" } : { hour: "numeric", minute: "2-digit" });

  // ---- the reading ----------------------------------------------------------------------------
  // The last answer of `richos_monitor`. `T` is its tokens, `M` its memory (held while the card
  // and the details are closed, when the app is not asked for it).
  let V = null, T = null, M = null;
  const R = { active: false, m1: 0, m5: 0, m15: 0, idleFor: Infinity };
  function absorb(view) {
    V = view;
    T = (view && view.tokens) || null;
    if (view && view.memory) M = view.memory;
    R.active = !!(T && T.active);
    R.m1 = T ? T.m1 : 0;
    R.m5 = T ? T.m5 : 0;
    R.m15 = T ? T.m15 : 0;
    R.idleFor = R.active ? 0 : T && T.last_ms ? Math.max(0, (view.now_ms - T.last_ms) / 1000) : Infinity;
  }
  /// What the counts start from: the switch was turned on then. A stretch before it was never
  /// counted, so the details say so rather than show it as nothing used.
  function sinceNote(spanSec) {
    if (!V || !T || !T.since_ms || T.since_ms <= V.now_ms - spanSec * 1000) return "";
    return " Counted since " + clock(T.since_ms) + ", when My RichOS was turned on.";
  }

  let on = false, built = false;
  let btn, tileCanvas, speedEl, card, scrim, sheet, $, $s;
  let pinned = false, hovering = false, hideTimer = 0;
  // Escape (or closing the details) put focus back on the square: its card stays closed until
  // focus leaves the square and comes back.
  let focusDismissed = false;
  const cardOpen = () => built && card.classList.contains("is-open");
  const sheetOpen = () => built && !sheet.hidden;

  function sizeCanvas(c, w, h) {
    const dpr = Math.max(1, window.devicePixelRatio || 1);
    if (c.width !== Math.round(w * dpr) || c.height !== Math.round(h * dpr)) {
      c.width = Math.round(w * dpr);
      c.height = Math.round(h * dpr);
    }
    const ctx = c.getContext("2d");
    ctx.setTransform(dpr, 0, 0, dpr, 0, 0);
    ctx.clearRect(0, 0, w, h);
    return ctx;
  }
  /// The theme's paints, read once per theme (the theme watcher forgets them), never every tick.
  let paints = null;
  function palette() {
    paints = paints || {
      ground: css("--res-ground"), flow: css("--rom-flow"), track: css("--res-track"), ram: css("--res-ram"),
      ink: css("--res-glyph"), hatch: css("--res-hatch"), rest: css("--nr-ink"),
    };
    return paints;
  }

  // ---- the rail square ------------------------------------------------------------------------
  /// Ten bars, six seconds each: the last minute, newest at the right, on the scale of the last
  /// 15 minutes so a burst after quiet stands tall. A slice with nothing is a dot on the floor.
  function paintTile() {
    const ctx = sizeCanvas(tileCanvas, 34, 34);
    const P = palette();
    ctx.fillStyle = P.ground;
    ctx.fillRect(0, 0, 34, 34);
    const s = (T && T.tile) || [];
    const top = Math.max(1, (T && T.scale) || 0);
    // whole pixels, so the bars stay crisp wherever the square is drawn at another scale (a
    // Make a video recording draws the window at 1x)
    const x0 = 2, floor = 31, room = 19;
    for (let i = 0; i < 10; i++) {
      const x = x0 + i * 3;
      const v = s[i] || 0;
      const hgt = v > 0 ? Math.max(2, Math.round((v / top) * room)) : 0;
      if (hgt) {
        ctx.fillStyle = P.flow;
        ctx.fillRect(x, floor - hgt, 2, hgt);
      } else {
        ctx.fillStyle = P.rest;
        ctx.fillRect(x + 1, floor - 1, 1, 1);
      }
    }
  }
  /// What the square last showed: a tick that changes nothing touches nothing (an idle rail is
  /// the same ten dots second after second).
  let railShown = "";
  function paintRail() {
    const shown = [R.active, R.m1, T ? T.scale : 0, T ? T.tile.join() : ""].join("|");
    if (shown === railShown) return;
    railShown = shown;
    btn.dataset.state = R.active ? "using" : "idle";
    speedEl.textContent = R.active ? tokens(R.m1, true) : "Idle";
    btn.setAttribute(
      "aria-label",
      R.active ? "My RichOS: using tokens, " + spoken(R.m1) + " in the last minute" : "My RichOS: idle, no tokens being used"
    );
    paintTile();
  }

  // ---- graphs ---------------------------------------------------------------------------------
  /// Bars on a floor line: the shape of every token graph here.
  function paintBars(c, w, h, bars, P, lit) {
    const ctx = sizeCanvas(c, w, h);
    ctx.fillStyle = P.ground;
    ctx.fillRect(0, 0, w, h);
    ctx.fillStyle = P.hatch;
    for (const q of [0.25, 0.5, 0.75]) ctx.fillRect(4, Math.round(4 + (h - 8) * q), w - 8, 1);
    const n = bars.length;
    if (!n) return;
    const top = Math.max(1, Math.max.apply(null, bars));
    const step = (w - 8) / n;
    const bw = Math.max(1, Math.floor(step) - 1);
    for (let i = 0; i < n; i++) {
      const x = Math.round(4 + i * step);
      const hgt = bars[i] > 0 ? Math.max(2, Math.round((bars[i] / top) * (h - 12))) : 0;
      ctx.fillStyle = i === lit ? P.ink : hgt ? P.flow : P.rest;
      if (hgt) ctx.fillRect(x, h - 4 - hgt, bw, hgt);
      else ctx.fillRect(x, h - 5, bw, 1);
    }
  }
  /// A filled line for memory, in bytes. `fromZero` is the honest scale of the sheet's large
  /// graph; the small lines stretch between their own low and high so a change can be seen at
  /// all. A stretch with no reading (`null`) is left out, never drawn as 0.
  function paintLine(c, w, h, list, P, fromZero, lit) {
    const ctx = sizeCanvas(c, w, h);
    ctx.fillStyle = P.ground;
    ctx.fillRect(0, 0, w, h);
    const pad = fromZero ? 4 : 2;
    if (fromZero) {
      ctx.fillStyle = P.hatch;
      for (const q of [0.25, 0.5, 0.75]) ctx.fillRect(pad, Math.round(pad + (h - 2 * pad) * q), w - 2 * pad, 1);
    }
    const known = list.filter((v) => v != null);
    if (!known.length) return;
    const hi = Math.max.apply(null, known) * (fromZero ? 1.12 : 1.02) || 1;
    const lo = fromZero ? 0 : Math.min.apply(null, known) * 0.9;
    const X = (i) => pad + (list.length > 1 ? i / (list.length - 1) : 1) * (w - 2 * pad);
    const Y = (v) => h - pad - ((v - lo) / (hi - lo || 1)) * (h - 2 * pad);
    // runs of readings: each its own filled shape and line
    const runs = [];
    let run = [];
    list.forEach((v, i) => {
      if (v == null) { if (run.length) runs.push(run); run = []; }
      else run.push(i);
    });
    if (run.length) runs.push(run);
    for (const r of runs) {
      if (r.length === 1) r.unshift(r[0]); // a lone reading is a dot, not nothing
      ctx.beginPath();
      ctx.moveTo(X(r[0]), h - pad);
      for (const i of r) ctx.lineTo(X(i), Y(list[i]));
      ctx.lineTo(X(r[r.length - 1]), h - pad);
      ctx.closePath();
      ctx.globalAlpha = 0.22;
      ctx.fillStyle = P.ram;
      ctx.fill();
      ctx.globalAlpha = 1;
      ctx.beginPath();
      r.forEach((i, k) => (k ? ctx.lineTo(X(i), Y(list[i])) : ctx.moveTo(X(i), Y(list[i]))));
      ctx.strokeStyle = P.ram;
      ctx.lineWidth = 1.5;
      ctx.lineJoin = "round";
      ctx.stroke();
    }
    if (lit != null && lit >= 0 && list[lit] != null) {
      ctx.fillStyle = P.ink;
      ctx.fillRect(Math.round(X(lit)), pad, 1, h - 2 * pad);
      ctx.beginPath();
      ctx.arc(X(lit), Y(list[lit]), 3, 0, Math.PI * 2);
      ctx.fill();
    }
  }

  // ---- the card -------------------------------------------------------------------------------
  const CHEV = '<svg class="rom-chev" viewBox="0 0 12 12" aria-hidden="true"><path d="M4.5 2.5 8 6l-3.500 3.500" fill="none" stroke="currentColor" stroke-width="1.6" stroke-linecap="round" stroke-linejoin="round"/></svg>';
  function chip(el) {
    el.dataset.tone = R.active ? "using" : "idle";
    el.textContent = R.active ? "Using tokens" : "Idle";
  }
  /// RichFinder and RichPass are pages in RichOS's own window: their memory is in RichOS's figure.
  /// RichPass is a document in his browser: its memory is the browser's, which RichOS does not read.
  const within = (a) => (a.within === "browser" ? "In your browser" : "In RichOS");
  /// Each suite app as the memory reading names it, open or not.
  function apps() {
    return ((M && M.apps) || []).map((a) => ({ id: a.id, name: a.name, open: a.processes > 0 || a.within === "richos", within: a.within || null, now: a.now, avg: a.avg, peak: a.peak, hist: a.hist || [] }));
  }
  let lastWins = "", lastApps = "";
  function paintCard() {
    const P = palette();
    chip($("chip"));
    $("hero").textContent = tokens(R.m1);
    $("hero-unit").textContent = R.active ? "tokens a minute" : R.m1 > 0 ? "in the last minute, stopped now" : "tokens a minute";
    paintBars($("tok-graph"), 306, 56, (T && T.card) || [], P);
    const row = (name, total, mins) =>
      "<span>" + name + "</span><b>" + tokens(total / mins) + "/min</b><span>" + tokens(total) + "</span>";
    const wins =
      '<i></i><i>Speed</i><i>Used</i>' + row("Last minute", R.m1, 1) + row("Last 5 minutes", R.m5, 5) + row("Last 15 minutes", R.m15, 15);
    if (wins !== lastWins) { $("wins").innerHTML = wins; lastWins = wins; }
    // What the token half shows, in its name, for VoiceOver (the figures sit inside the button).
    speak(card.querySelector('[data-go="tokens"]'), "Tokens: " + tokens(R.m1) + " tokens a minute. Last minute " + tokens(R.m1) + " used, last 5 minutes " +
      tokens(R.m5) + ", last 15 minutes " + tokens(R.m15) + ". Open the details");
    $("age").textContent = R.active ? "Updated just now" : ago(R.idleFor);
    if (!M) return;
    $("mem-now").textContent = ram(M.all.now);
    $("mem-avg").textContent = ram(M.all.avg);
    paintLine($("mem-spark"), 84, 24, M.all.hist || [], P, false);
    const list = apps();
    const top = Math.max.apply(null, list.map((a) => a.now || 0).concat([1]));
    const html = list
      .map((a) =>
        a.within
          ? '<span>' + esc(a.name) + '</span><span></span><span class="is-off">' + within(a) + "</span>"
          : a.open
          ? "<span>" + esc(a.name) + '</span><span class="rom-bar"><i style="width:' + Math.max(2, ((a.now || 0) / top) * 100).toFixed(1) + '%"></i></span><b>' + ram(a.now) + "</b>"
          : '<span class="is-off">' + esc(a.name) + '</span><span></span><span class="is-off">Not open</span>'
      )
      .join("");
    if (html !== lastApps) { $("apps").innerHTML = html; lastApps = html; }
    speak(card.querySelector('[data-go="memory"]'), "Memory, all RichOS apps: " + ram(M.all.now) + " now, 2-minute average " + ram(M.all.avg) + ". " +
      list.map((a) => a.name + " " + (a.within ? within(a) : a.open ? ram(a.now) : "not open")).join(", ") + ". Open the details");
  }
  function speak(el, words) {
    if (el.getAttribute("aria-label") !== words) el.setAttribute("aria-label", words);
  }

  // open, pin, close: the Mac square's own behavior (`resources.js`), with SwearMeter's card rules
  function place() {
    const r = btn.querySelector(".res-tile").getBoundingClientRect();
    const foot = Math.min(window.innerHeight - 10, r.bottom + 14);
    card.style.left = Math.round(r.right + 14) + "px";
    card.style.top = "auto";
    card.style.bottom = Math.round(window.innerHeight - foot) + "px";
    // Never taller than the window above its foot: in a short window it scrolls inside.
    const room = Math.max(160, foot - 10);
    card.style.maxHeight = room + "px";
    // Scrolling only when it has to: a scrolling card clips its arrow, which a card that fits keeps.
    card.style.overflowY = card.scrollHeight > room ? "auto" : "";
    card.style.setProperty("--res-arrow", Math.round(Math.max(10, foot - (r.top + r.height / 2) - 6)) + "px");
  }
  function sync() {
    if (!built) return;
    const home = document.body.classList.contains("home-open");
    if (home || !on) { pinned = false; hovering = false; }
    const open = on && !home && sheet.hidden && (pinned || hovering || (btn.matches(":focus-visible") && !focusDismissed));
    const was = cardOpen();
    if (open) { card.hidden = false; place(); }
    card.classList.toggle("is-open", open);
    btn.setAttribute("aria-expanded", String(pinned && open));
    // Opened: paint what is held at once, then ask for the card's graph and the memory.
    if (open && !was) { lastWins = lastApps = ""; paintCard(); tick(); }
  }

  // ---- the details sheet ----------------------------------------------------------------------
  const TOK_RANGES = [
    { id: "m15", name: "15 minutes", span: 900, n: 60, from: "15 minutes ago", words: "the last 15 minutes" },
    { id: "h1", name: "1 hour", span: 3600, n: 60, from: "1 hour ago", words: "the last hour" },
    { id: "d1", name: "24 hours", span: 86400, n: 96, from: "24 hours ago", words: "the last 24 hours" },
  ];
  const MEM_RANGES = [
    { id: "m2", name: "2 minutes", span: 120, n: 60, from: "2 minutes ago" },
    { id: "m15", name: "15 minutes", span: 900, n: 90, from: "15 minutes ago" },
    { id: "h1", name: "1 hour", span: 3600, n: 120, from: "1 hour ago" },
  ];
  const seg = (name, list, label) =>
    '<span class="rom-seg" role="radiogroup" aria-label="' + label + '" data-seg="' + name + '">' +
    list.map((r) => '<button type="button" role="radio" data-v="' + r.id + '">' + r.name + "</button>").join("") + "</span>";

  const view = { tab: "tokens", tok: "m15", mem: "m2", tokLit: -1, memLit: -1 };
  /// What the open details ask the app for: the chosen stretches of each graph.
  function detailRequest() {
    if (!sheetOpen()) return null;
    const t = TOK_RANGES.find((r) => r.id === view.tok);
    const m = MEM_RANGES.find((r) => r.id === view.mem);
    return { token_span_s: t.span, token_bars: t.n, memory_span_s: m.span, memory_points: m.n };
  }
  const ICONS = { richscribe: "richscribe", richbrief: "richbrief", richfiles: "richfiles", richglance: "richglance", richfinder: "richfinder", richpass: "richpass", richcleaner: "richcleaner", "richglance-capture": "richglance", "window-gestures": "richfiles" };
  const ICON = (id) =>
    id === "richos"
      ? '<svg class="rom-ico rom-ico-r" viewBox="-130 -125 1004 1004" aria-hidden="true">' + markPaths() + "</svg>"
      : ICONS[id]
        ? '<img class="rom-ico" src="assets/app-icons/' + ICONS[id] + '-128.png" alt="" />'
        : '<span class="rom-ico" aria-hidden="true"></span>';
  /// The page's own R mark (index.html's #R and #arrow), copied as paths rather than referenced
  /// with <use>, which Make a video's painter does not draw.
  function markPaths() {
    return ["R", "arrow"].map((id) => {
      const n = document.getElementById(id);
      if (!n) return "";
      const c = n.cloneNode(true);
      c.removeAttribute("id");
      return c.outerHTML;
    }).join("");
  }
  /// A chat's name and company, as the sidebar names them (`main.js`), never made up.
  function named(r) {
    const row = r.thread && typeof threadRow === "function" ? threadRow(r.thread) : null;
    const company = r.entity && typeof entityLabel === "function" ? entityLabel(r.entity) : "";
    return {
      chat: row && (row.display_title || row.title) ? row.display_title || row.title : r.thread ? "A chat" : "Outside a chat",
      company: company && company !== "No entity" ? company : "",
    };
  }

  function paintSheet() {
    if (!sheetOpen()) return;
    const P = palette();
    chip($s("chip"));
    for (const t of sheet.querySelectorAll("[data-tab]")) t.setAttribute("aria-selected", String(t.dataset.tab === view.tab));
    for (const p of sheet.querySelectorAll("[data-pane]")) p.hidden = p.dataset.pane !== view.tab;
    for (const g of sheet.querySelectorAll("[data-seg]")) {
      for (const b of g.querySelectorAll("button")) b.setAttribute("aria-checked", String(b.dataset.v === view[g.dataset.seg]));
    }
    if (view.tab === "tokens") paintTokens(P);
    else paintMemory(P);
  }
  let lastWhere = "";
  function paintTokens(P) {
    const range = TOK_RANGES.find((r) => r.id === view.tok);
    $s("t-big").textContent = tokens(R.m1);
    $s("t-big-unit").textContent = "tokens a minute";
    $s("t-line").textContent = R.active
      ? "Tokens are being used right now."
      : "No tokens are being used right now. " + ago(R.idleFor) + ".";
    const led = (name, total, mins) =>
      "<div><dt>" + name + "</dt><dd><b>" + tokens(total / mins) + "</b> /min</dd><dd>" + tokens(total) + " used</dd></div>";
    $s("t-ledger").innerHTML = led("Last minute", R.m1, 1) + led("Last 5 minutes", R.m5, 5) + led("Last 15 minutes", R.m15, 15);

    // The details' own reading is the one for the stretch on screen; until it arrives (a
    // stretch just picked), nothing of another stretch is drawn as this one.
    const d = T && T.detail && T.detail.span_s === range.span ? T.detail : null;
    const s = d ? d.series : { bars: [], bar_ms: (range.span * 1000) / range.n, ends_at_ms: Date.now() };
    const c = $s("t-graph");
    const w = c.parentElement.clientWidth;
    paintBars(c, w, 128, s.bars, P, view.tokLit);
    const barSec = s.bar_ms / 1000;
    const perMin = (v) => (v * 60) / barSec;
    const total = s.bars.reduce((a, b) => a + b, 0);
    if (d && view.tokLit >= 0 && view.tokLit < s.bars.length) {
      const end = s.ends_at_ms - (s.bars.length - 1 - view.tokLit) * s.bar_ms;
      const start = end - s.bar_ms;
      const secs = barSec < 60;
      $s("t-read").innerHTML = "<b>" + clock(start, secs) + " to " + clock(Math.min(end, Date.now()), secs) + "</b> " + tokens(s.bars[view.tokLit]) + " used, " + tokens(perMin(s.bars[view.tokLit])) + " a minute";
    } else {
      $s("t-read").innerHTML = "<b>" + tokens(total) + "</b> used in " + range.words + ". Fastest: " + tokens(perMin(s.bars.length ? Math.max.apply(null, s.bars) : 0)) + " a minute." + esc(sinceNote(range.span)) + " Point at the graph for any moment.";
    }
    $s("t-from").textContent = range.from;

    $s("t-where-title").textContent = "Where they went, " + range.words;
    const rows = d ? d.sources : [];
    const top = Math.max.apply(null, rows.map((r) => r.tokens).concat([1]));
    const where = rows.length
      ? '<div class="rom-where-head"><span>Chat</span><span>Model</span><span>Now</span><span>Used</span></div>' +
        rows
          .map((r) => {
            const n = named(r);
            const who = r.team ? (r.working ? "Team, " + r.working + " working" : "Team") : "Rich";
            return (
              '<div class="rom-where-row"><span class="rom-where-chat"><b>' + esc(n.chat) + "</b><i>" + (n.company ? esc(n.company) + " · " : "") + who + "</i></span><span>" + esc(r.model) + "</span>" +
              '<span class="rom-where-now' + (r.per_min > 0 ? "" : " is-off") + '">' + (r.per_min > 0 ? tokens(r.per_min) + "/min" : "Idle") + "</span>" +
              '<span class="rom-where-used">' + tokens(r.tokens) + "</span>" +
              '<span class="rom-bar"><i style="width:' + Math.max(1, (r.tokens / top) * 100).toFixed(1) + '%"></i></span></div>'
            );
          })
          .join("")
      : '<p class="rom-none">Nothing was used in ' + range.words + ".</p>";
    if (where !== lastWhere) { $s("t-where").innerHTML = where; lastWhere = where; }
    const split = (list) => {
      const sum = list.reduce((a, b) => a + b.tokens, 0);
      if (sum < 1) return '<p class="rom-none">Nothing to split.</p>';
      return list
        .map((x) => "<span>" + esc(x.name) + '</span><span class="rom-bar"><i style="width:' + Math.max(1, (x.tokens / sum) * 100).toFixed(1) + '%"></i></span><b>' + tokens(x.tokens) + "</b><i>" + Math.round((x.tokens / sum) * 100) + "%</i>")
        .join("");
    };
    $s("t-models").innerHTML = split(d ? d.models : []);
    $s("t-kinds").innerHTML = split(d ? d.kinds : []);
  }
  function paintMemory(P) {
    const range = MEM_RANGES.find((r) => r.id === view.mem);
    const all = M ? M.all : { now: null, avg: null, peak: null, hist: [] };
    const list = apps();
    $s("m-big").textContent = M && M.status === "measuring" ? "Measuring…" : ram(all.now);
    const open = list.filter((a) => a.open).length;
    $s("m-line").textContent = M && M.status === "unavailable"
      ? "RichOS can't read the apps' memory right now."
      : "All RichOS apps together. " + open + " of " + list.length + " are open.";
    const total = M && M.mac_total_bytes;
    $s("m-ledger").innerHTML =
      "<div><dt>2-minute average</dt><dd><b>" + ram(all.avg) + "</b></dd></div>" +
      "<div><dt>2-minute high</dt><dd><b>" + ram(all.peak) + "</b></dd></div>" +
      "<div><dt>Share of this Mac</dt><dd><b>" + (total && all.now != null ? Math.round((all.now / total) * 100) + "%" : "-") + "</b>" + (total ? " of " + Math.round(total / (1024 * MB)) + " GB" : "") + "</dd></div>";
    const series = M && M.series && M.series.span_s === range.span ? M.series.list : [];
    const c = $s("m-graph");
    const w = c.parentElement.clientWidth;
    paintLine(c, w, 128, series, P, true, view.memLit);
    const known = series.filter((v) => v != null);
    if (view.memLit >= 0 && view.memLit < series.length && series[view.memLit] != null) {
      const at = V.now_ms - ((series.length - 1 - view.memLit) * range.span * 1000) / series.length;
      $s("m-read").innerHTML = "<b>" + clock(at, range.span <= 900) + "</b> " + ram(series[view.memLit]) + " in use";
    } else if (known.length) {
      const lo = Math.min.apply(null, known), hi = Math.max.apply(null, known);
      $s("m-read").innerHTML = "Between <b>" + ram(lo) + "</b> and <b>" + ram(hi) + "</b> over this stretch." + esc(sinceNote(range.span)) + " Point at the graph for any moment.";
    } else {
      $s("m-read").textContent = "No reading yet.";
    }
    $s("m-from").textContent = range.from;

    const host = $s("m-apps");
    if (!host.firstChild || host.dataset.apps !== list.map((a) => a.id).join()) {
      host.dataset.apps = list.map((a) => a.id).join();
      host.innerHTML =
        '<div class="rom-app-head"><span>App</span><span>Last 2 minutes</span><span>Now</span><span>Average</span><span>Highest</span></div>' +
        list.map((a) => '<div class="rom-app-row" data-app="' + esc(a.id) + '">' + ICON(a.id) + '<span class="rom-app-name">' + esc(a.name) + '</span><canvas aria-hidden="true"></canvas><b data-c="now"></b><span data-c="avg"></span><span data-c="peak"></span></div>').join("") +
        '<div class="rom-app-row rom-app-all"><span class="rom-ico"></span><span class="rom-app-name">All together</span><canvas aria-hidden="true"></canvas><b data-c="now"></b><span data-c="avg"></span><span data-c="peak"></span></div>';
    }
    const fill = (rowEl, a) => {
      rowEl.classList.toggle("is-off", a.open === false);
      const cv = rowEl.querySelector("canvas");
      // An app closed in the last two minutes keeps its line, average and highest of them: a
      // spike just before it closed is still there to read.
      const recent = (a.hist || []).some((v) => v != null && v > 0);
      if (a.within || (a.open === false && !recent)) {
        sizeCanvas(cv, 150, 26);
        rowEl.classList.toggle("is-off", true);
        rowEl.querySelector('[data-c="now"]').textContent = a.within ? within(a) : "Not open";
        rowEl.querySelector('[data-c="avg"]').textContent = "";
        rowEl.querySelector('[data-c="peak"]').textContent = "";
        return;
      }
      if (a.open === false) {
        paintLine(cv, 150, 26, a.hist, P, false);
        rowEl.querySelector('[data-c="now"]').textContent = "Not open";
        rowEl.querySelector('[data-c="avg"]').textContent = ram(a.avg);
        rowEl.querySelector('[data-c="peak"]').textContent = ram(a.peak);
        return;
      }
      paintLine(cv, 150, 26, a.hist || [], P, false);
      rowEl.querySelector('[data-c="now"]').textContent = ram(a.now);
      rowEl.querySelector('[data-c="avg"]').textContent = ram(a.avg);
      rowEl.querySelector('[data-c="peak"]').textContent = ram(a.peak);
    };
    for (const a of list) fill(host.querySelector('[data-app="' + a.id + '"]'), a);
    fill(host.querySelector(".rom-app-all"), all);
  }

  let returnTo = null;
  function openSheet(tab) {
    view.tab = tab === "memory" ? "memory" : "tokens";
    view.tokLit = view.memLit = -1;
    returnTo = document.activeElement === document.body ? btn : document.activeElement;
    pinned = false;
    hovering = false;
    sheet.hidden = false;
    scrim.hidden = false;
    sync();
    lastWhere = "";
    paintSheet();
    tick();
    sheet.querySelector('[data-tab="' + view.tab + '"]').focus({ preventScroll: true });
  }
  function closeSheet() {
    if (!sheetOpen()) return;
    sheet.hidden = true;
    scrim.hidden = true;
    // Focus goes back to the square (never to nowhere), without opening its card again until
    // focus leaves the square and comes back (resources.js's rule).
    if (on) {
      focusDismissed = true;
      btn.focus({ preventScroll: true });
    }
    sync();
  }

  // ---- building it, the first time it is turned on --------------------------------------------
  function build() {
    if (built) return;
    built = true;
    btn = document.createElement("button");
    btn.type = "button";
    btn.id = "rom-square";
    btn.className = "res rom";
    btn.setAttribute("aria-haspopup", "dialog");
    btn.setAttribute("aria-controls", "rom-card");
    btn.setAttribute("aria-expanded", "false");
    btn.innerHTML =
      '<span class="rom-readout" aria-hidden="true"><b data-k="speed"></b></span>' +
      '<span class="res-tile rom-tile"><canvas aria-hidden="true"></canvas><i class="rom-light" aria-hidden="true"></i></span>';
    slot.appendChild(btn);
    tileCanvas = btn.querySelector("canvas");
    speedEl = btn.querySelector('[data-k="speed"]');

    card = document.createElement("div");
    card.id = "rom-card";
    card.className = "res-card rom-card";
    card.setAttribute("role", "dialog");
    card.setAttribute("aria-label", "My RichOS");
    card.setAttribute("data-dismiss", "escape");
    card.innerHTML =
      '<div class="res-card-head"><span class="res-card-title">My RichOS</span><span class="rom-chip" data-k="chip"></span></div>' +
      '<button type="button" class="rom-sec" data-go="tokens" aria-label="Tokens, open the details">' +
      '<span class="rom-sec-head"><span class="rom-hero"><b data-k="hero"></b><span data-k="hero-unit"></span></span>' + CHEV + "</span>" +
      '<canvas class="rom-graph" data-k="tok-graph" aria-hidden="true"></canvas>' +
      '<span class="res-axis" aria-hidden="true"><span>15 minutes ago</span><span>Now</span></span>' +
      '<span class="rom-win" data-k="wins"></span>' +
      "</button>" +
      '<button type="button" class="rom-sec" data-go="memory" aria-label="Memory, open the details">' +
      '<span class="rom-sec-head"><span class="rom-mem-title">Memory<i>all RichOS apps</i></span><b class="rom-mem-now" data-k="mem-now"></b>' + CHEV + "</span>" +
      '<span class="rom-mem-avg"><canvas class="rom-spark" data-k="mem-spark" aria-hidden="true"></canvas><span>2-minute average</span><b data-k="mem-avg"></b></span>' +
      '<span class="rom-apps" data-k="apps"></span>' +
      "</button>" +
      '<div class="res-foot"><span data-k="age"></span><span>Click a part for details</span></div>';
    document.body.appendChild(card);
    $ = (k) => card.querySelector('[data-k="' + k + '"]');

    scrim = document.createElement("div");
    scrim.id = "rom-scrim";
    scrim.hidden = true;
    sheet = document.createElement("section");
    sheet.id = "rom-sheet";
    sheet.hidden = true;
    sheet.setAttribute("role", "dialog");
    sheet.setAttribute("aria-labelledby", "rom-sheet-title");
    sheet.setAttribute("data-dismiss", "control:#rom-close");
    sheet.innerHTML =
      '<header class="rom-sh-head"><h2 id="rom-sheet-title">My RichOS</h2><span class="rom-chip" data-s="chip"></span>' +
      '<span class="rom-tabs" role="tablist" aria-label="My RichOS details"><button type="button" role="tab" data-tab="tokens">Tokens</button><button type="button" role="tab" data-tab="memory">Memory</button></span>' +
      '<button type="button" class="rom-close" id="rom-close" aria-label="Close"><svg viewBox="0 0 14 14" aria-hidden="true"><path d="M2 2l10 10M12 2 2 12" fill="none" stroke="currentColor" stroke-width="1.7" stroke-linecap="round"/></svg></button></header>' +
      '<div class="rom-sh-body">' +
      // Tokens
      '<div class="rom-pane" data-pane="tokens" role="tabpanel">' +
      '<div class="rom-top"><div class="rom-big"><b data-s="t-big"></b><span data-s="t-big-unit"></span><p data-s="t-line"></p></div>' +
      '<dl class="rom-ledger" data-s="t-ledger"></dl></div>' +
      '<div class="rom-block"><div class="rom-block-head"><h3>Tokens used</h3>' + seg("tok", TOK_RANGES, "Stretch of time") + "</div>" +
      '<p class="rom-read" data-s="t-read"></p>' +
      '<div class="rom-plot"><canvas data-s="t-graph" aria-hidden="true"></canvas></div>' +
      '<div class="res-axis" aria-hidden="true"><span data-s="t-from"></span><span>Now</span></div></div>' +
      '<div class="rom-cols"><div class="rom-block"><h3 data-s="t-where-title"></h3><div class="rom-where" data-s="t-where"></div></div>' +
      '<div class="rom-block"><h3>By model</h3><div class="rom-split" data-s="t-models"></div><h3 class="rom-h3-gap">By kind</h3><div class="rom-split" data-s="t-kinds"></div></div></div>' +
      "</div>" +
      // Memory
      '<div class="rom-pane" data-pane="memory" role="tabpanel">' +
      '<div class="rom-top"><div class="rom-big"><b data-s="m-big"></b><p data-s="m-line"></p></div>' +
      '<dl class="rom-ledger" data-s="m-ledger"></dl></div>' +
      '<div class="rom-block"><div class="rom-block-head"><h3>Memory, all RichOS apps</h3>' + seg("mem", MEM_RANGES, "Stretch of time") + "</div>" +
      '<p class="rom-read" data-s="m-read"></p>' +
      '<div class="rom-plot"><canvas data-s="m-graph" aria-hidden="true"></canvas></div>' +
      '<div class="res-axis" aria-hidden="true"><span data-s="m-from"></span><span>Now</span></div></div>' +
      '<div class="rom-block"><h3>Each app</h3><div class="rom-apptable" data-s="m-apps"></div></div>' +
      '<p class="rom-fine">Read every two seconds while My RichOS is on. Each graph is redrawn once per reading and nothing is animated, so watching costs next to nothing.</p>' +
      "</div></div>";
    document.body.append(scrim, sheet);
    $s = (k) => sheet.querySelector('[data-s="' + k + '"]');

    const leaveSoon = () => { hideTimer = setTimeout(() => { hovering = false; sync(); }, 160); };
    btn.addEventListener("pointerenter", () => { clearTimeout(hideTimer); hovering = true; sync(); });
    btn.addEventListener("pointerleave", leaveSoon);
    card.addEventListener("pointerenter", () => { clearTimeout(hideTimer); if (cardOpen()) hovering = true; });
    card.addEventListener("pointerleave", leaveSoon);
    btn.addEventListener("focus", sync);
    btn.addEventListener("blur", () => { focusDismissed = false; setTimeout(sync, 0); });
    btn.addEventListener("click", (e) => {
      pinned = !pinned;
      if (!pinned) hovering = false;
      sync();
      // Pinned from the keyboard: the card's first part is next.
      if (pinned && e.detail === 0) card.querySelector(".rom-sec").focus();
    });
    document.addEventListener("pointerdown", (e) => {
      if (pinned && !btn.contains(e.target) && !card.contains(e.target)) { pinned = false; sync(); }
    });
    window.addEventListener("resize", () => { if (cardOpen()) place(); if (sheetOpen()) paintSheet(); });
    for (const sec of card.querySelectorAll(".rom-sec")) sec.addEventListener("click", () => openSheet(sec.dataset.go));
    // A press on any rail item goes where the item goes: the details step aside first (RichFinder
    // opens behind their scrim otherwise). Capture, so it runs before the item's own handler.
    btn.closest("nav").addEventListener("click", (e) => {
      if (sheetOpen() && e.target.closest(".nr-item")) closeSheet();
    }, true);

    scrim.addEventListener("click", closeSheet);
    sheet.querySelector(".rom-close").addEventListener("click", closeSheet);
    sheet.addEventListener("click", (e) => {
      const tab = e.target.closest("[data-tab]");
      if (tab) { view.tab = tab.dataset.tab; view.tokLit = view.memLit = -1; paintSheet(); tick(); return; }
      const opt = e.target.closest("[data-seg] button");
      if (opt) { view[opt.parentElement.dataset.seg] = opt.dataset.v; view.tokLit = view.memLit = -1; lastWhere = ""; paintSheet(); tick(); }
    });
    sheet.addEventListener("keydown", (e) => {
      // Left and Right move along the tabs and along a stretch-of-time control.
      if (e.key !== "ArrowLeft" && e.key !== "ArrowRight") return;
      const group = e.target.closest(".rom-tabs, .rom-seg");
      if (!group) return;
      const list = Array.from(group.querySelectorAll("button"));
      const next = list[(list.indexOf(e.target) + (e.key === "ArrowRight" ? 1 : list.length - 1)) % list.length];
      e.preventDefault();
      next.focus();
      next.click();
    });
    const point = (canvasKey, litKey, count) => {
      const c = $s(canvasKey);
      c.addEventListener("pointermove", (e) => {
        const r = c.getBoundingClientRect();
        const n = count();
        const i = Math.max(0, Math.min(n - 1, Math.floor(((e.clientX - r.left - 4) / (r.width - 8)) * n)));
        if (i !== view[litKey]) { view[litKey] = i; paintSheet(); }
      });
      c.addEventListener("pointerleave", () => { view[litKey] = -1; paintSheet(); });
    };
    point("t-graph", "tokLit", () => (T && T.detail ? T.detail.series.bars.length : TOK_RANGES.find((r) => r.id === view.tok).n));
    point("m-graph", "memLit", () => MEM_RANGES.find((r) => r.id === view.mem).n);

    // Escape: the details first, then the card, before the window's own Escape sees it.
    document.addEventListener("keydown", (e) => {
      if (e.key !== "Escape") return;
      // Settings open over My RichOS: the Escape is its own (settings-button.js closes it).
      const settings = document.getElementById("set-btn");
      if (settings && settings.getAttribute("aria-expanded") === "true") return;
      if (sheetOpen()) { e.preventDefault(); e.stopPropagation(); closeSheet(); return; }
      if (!cardOpen()) return;
      e.preventDefault();
      e.stopPropagation();
      const inside = card.contains(document.activeElement);
      pinned = false;
      hovering = false;
      if (inside || document.activeElement === btn) {
        focusDismissed = true;
        btn.focus();
      }
      sync();
    }, true);
    new MutationObserver(() => {
      if (document.body.classList.contains("home-open")) { closeSheet(); sync(); }
    }).observe(document.body, { attributes: true, attributeFilter: ["class"] });
    new MutationObserver(() => {
      if (!on) return;
      lastWins = lastApps = lastWhere = railShown = "";
      paints = null;
      paintRail();
      if (cardOpen()) paintCard();
      paintSheet();
    }).observe(document.documentElement, { attributes: true, attributeFilter: ["data-theme"] });
  }

  // ---- the one timer --------------------------------------------------------------------------
  let timer = 0, asking = false, again = false;
  async function tick() {
    if (!on || document.hidden) return;
    // One question at a time; a tick that arrives while one is out asks again when it is back.
    if (asking) { again = true; return; }
    asking = true;
    let answer = null;
    try {
      answer = await bridge.invoke("richos_monitor", { card: cardOpen(), detail: detailRequest() });
    } catch (_e) {
      answer = null;
    }
    asking = false;
    if (!on) return;
    if (answer && answer.on) absorb(answer);
    paintRail();
    if (cardOpen()) { paintCard(); place(); }
    if (sheetOpen()) paintSheet();
    if (again) { again = false; tick(); }
  }
  function run() {
    clearInterval(timer);
    timer = 0;
    slot.hidden = !on;
    if (!on) {
      if (built) { closeSheet(); sync(); }
      V = T = M = null;
      return;
    }
    build();
    if (!document.hidden) {
      paintRail();
      tick();
      timer = setInterval(tick, 1000);
    }
  }
  // A hidden window stops the timer; shown again, it reads at once and ticks again.
  document.addEventListener("visibilitychange", () => { if (on) run(); });

  // ---- the setting: off by default, kept ---------------------------------------------------------
  // `config.json`'s `show_richos_monitor`, read and written as the context meter's switch is.
  function set(next) {
    on = next === true;
    run();
    if (window.RichSettings) window.RichSettings.paint();
  }
  if (window.RichSettings && window.RichSettings.registerRichosMonitor) {
    window.RichSettings.registerRichosMonitor({
      read: () => on,
      // The switch moves at once and is put back if the choice could not be saved.
      write: (next) => {
        const before = on;
        set(next === true);
        return Promise.resolve(bridge.invoke("set_richos_monitor", { show: on })).catch(() => {
          set(before);
          if (window.RichSettings) window.RichSettings.toast("The My RichOS setting could not be saved.");
        });
      },
    });
  }
  Promise.resolve(bridge.invoke("get_richos_monitor"))
    .then((shown) => { if (shown === true) set(true); })
    .catch(() => {});

  window.RichosMonitor = {
    get on() { return on; },
    /// For the acceptance suite: what the rail, the card and the details read now.
    state: () => ({
      on,
      built,
      timer: !!timer,
      rail: built && !slot.hidden ? { state: btn.dataset.state, speed: speedEl.textContent, label: btn.getAttribute("aria-label") } : null,
      card: cardOpen(),
      sheet: sheetOpen() ? view.tab : null,
    }),
    repaint() { lastWins = lastApps = lastWhere = railShown = ""; return tick(); },
    openSheet: (tab) => { if (built && on) openSheet(tab); },
    closeSheet: () => { if (built) closeSheet(); },
  };
})();

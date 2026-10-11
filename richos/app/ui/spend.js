"use strict";
// API-EQUIVALENT TOKEN SPEND: ITS OWN SCREEN, AND ITS ROW IN QUICK SETTINGS. Round 22.1
// (richos-hq design/mockups/rounds/round-22.1/index.html and NOTES.md; the CEO, 2026-10-10:
// "design/mockups/rounds/round-22.1: go"), measured against his words, ruling §118: "I want to
// add this feature that will allow the user to see and brag about (via screenshots and screen
// recording videos) about their "API-equivalent token spend"" and "That's needs to be its own
// screen/page.", and his round 22 feedback (docs/ceo-input/2026-10-10_02/feedback.md).
//
// - ITS OWN SCREEN: while it is open the app view and the home screen are covered, so a
//   screenshot or a recording shows only this. Settings stays on top, as on every screen.
// - THE METER: one figure as large as the column allows (196px, fitted smaller, never wrapped),
//   every character its own span at +0.02em in proportional figures, so no two touch (his
//   screenshot); the two sentences under it on two lines; the bar and legend by model; the
//   tokens line on two lines; the live line; the footer.
// - NO LIVE COUNTER ON THE FIGURE: the numbers are a snapshot, taken when the screen opens, when
//   a period is picked and when Refresh is pressed. Requests landing underneath put a gold dot
//   on Refresh, with no count.
// - "x agents working" STAYS LIVE, the last request priced in dollars ("0.1¢" becomes "$0.001").
// - THE WAY IN is the Token spend row in quick settings, with this month's figure taken when the
//   menu opens. On this screen the row is the current page and pressing it refreshes.
//
// Every figure is the app's own (`spend_view`, `spend_live`; richos-core spend.rs): each request
// this Mac's Claude Code made, priced at the published API rates by its exact model and its five
// token buckets. Nothing here is typed in or estimated.
(function () {
  const bridge = window.RichBridge;
  if (!bridge) return;

  const ORDER = ["fable", "opus", "sonnet", "haiku"]; // one fixed order: the bar and the legend
  const FAMILY_WORD = { fable: "Fable", opus: "Opus", sonnet: "Sonnet", haiku: "Haiku" };
  const MONTHS = ["January", "February", "March", "April", "May", "June", "July", "August", "September", "October", "November", "December"];
  const DAYS = ["Sunday", "Monday", "Tuesday", "Wednesday", "Thursday", "Friday", "Saturday"];
  const SHORT = { today: "today", week: "this week", month: "this month", all: "all time" };
  const LIVE_MS = 3000;
  const CHEVRON = '<svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true"><path d="m9 18 6-6-6-6"/></svg>';

  const screen = document.createElement("section");
  screen.id = "spend";
  screen.hidden = true;
  screen.setAttribute("aria-labelledby", "spend-title");
  // Escape is Back (main.js's derived popup enumeration reads this).
  screen.setAttribute("data-dismiss", "control:#spend-back");
  screen.innerHTML =
    '<header class="sp-head">' +
      '<button class="sp-back" id="spend-back" type="button"><svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true"><path d="m15 18-6-6 6-6"/></svg>Back</button>' +
      '<h1 class="sp-title" id="spend-title">API-equivalent token spend <span id="spend-title-when"></span></h1>' +
      '<div class="sp-fresh">' +
        '<span class="sp-updated" id="spend-updated" aria-live="polite"></span>' +
        '<button class="sp-refresh" id="spend-refresh" type="button" aria-label="Refresh the numbers"><svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true"><path d="M21 12a9 9 0 1 1-2.64-6.36"/><path d="M21 3v6h-6"/></svg>Refresh<span class="sp-new" aria-hidden="true"></span></button>' +
      '</div>' +
      '<div class="sp-period" role="group" aria-label="Period" id="spend-period">' +
        '<button type="button" data-p="today" aria-pressed="false">Today</button>' +
        '<button type="button" data-p="week" aria-pressed="false">This week</button>' +
        '<button type="button" data-p="month" aria-pressed="true">This month</button>' +
        '<button type="button" data-p="all" aria-pressed="false">All time</button>' +
      '</div>' +
    '</header>' +
    '<div class="sp-body" id="spend-body"></div>' +
    '<footer class="sp-foot">' +
      '<span class="sp-wordmark" id="spend-wordmark"></span>' +
      '<p class="sp-foot-note" id="spend-foot"></p>' +
    '</footer>';
  // NOT IN THE DOCUMENT UNTIL IT IS FIRST OPENED: the shell every other screen is measured against
  // (tests/contrast.js counts its text nodes, hidden ones included) stays exactly as it was for
  // everyone who never opens it.
  const $ = id => screen.querySelector("#" + id);

  // The footer's wordmark is the rail's own, taken inline so the theme's ink and gold reach it;
  // the copy carries none of the rail's ids, so each stays unique in the document.
  function wordmark() {
    const rail = document.getElementById("rail-wordmark");
    const into = $("spend-wordmark");
    if (!rail || !into || into.firstChild) return;
    const copy = rail.cloneNode(true);
    copy.removeAttribute("id");
    copy.querySelectorAll("[id]").forEach(n => n.removeAttribute("id"));
    into.appendChild(copy);
  }

  const S = { period: "month", open: false, shown: null, at: null, timer: null, live: null, opener: null, gen: 0, rowAmt: "" };

  async function call(cmd, args) {
    try { return await bridge.invoke(cmd, args || {}); }
    catch (e) { console.warn("[spend] " + cmd + ": " + e); return null; }
  }

  // ── the period, in local time: today from midnight, the week from Monday, the month from the 1st ──
  function periodStart(p, now) {
    const d = new Date(now);
    if (p === "all") return 0;
    if (p === "month") return new Date(d.getFullYear(), d.getMonth(), 1).getTime();
    const day = new Date(d.getFullYear(), d.getMonth(), d.getDate());
    if (p === "week") day.setDate(day.getDate() - ((day.getDay() + 6) % 7));
    return day.getTime();
  }
  function periodWords(p, now, firstAt) {
    const d = new Date(now);
    if (p === "today") return DAYS[d.getDay()] + ", " + MONTHS[d.getMonth()] + " " + d.getDate();
    if (p === "week") {
      const s = new Date(periodStart("week", now));
      return MONTHS[s.getMonth()] + " " + s.getDate() + " to " + (s.getMonth() === d.getMonth() ? "" : MONTHS[d.getMonth()] + " ") + d.getDate();
    }
    if (p === "month") return MONTHS[d.getMonth()] + " " + d.getFullYear();
    if (firstAt == null) return "since the first request";
    const f = new Date(firstAt);
    return "since " + MONTHS[f.getMonth()] + " " + f.getDate() + (f.getFullYear() === d.getFullYear() ? "" : ", " + f.getFullYear());
  }

  // ── numbers as words (round 22.1's) ──
  function money(v) { return "$" + v.toLocaleString("en-US", { minimumFractionDigits: 2, maximumFractionDigits: 2 }); }
  function moneyRound(v) { return "$" + Math.round(v).toLocaleString("en-US"); }
  function int(n) { return Math.round(n).toLocaleString("en-US"); }
  function big(n) {
    if (n >= 1e9) return (n / 1e9).toLocaleString("en-US", { maximumFractionDigits: n >= 1e10 ? 1 : 2 }) + " billion";
    if (n >= 1e6) return (n / 1e6).toLocaleString("en-US", { maximumFractionDigits: 1 }) + " million";
    return int(n);
  }
  // One request's cost in dollars (his words: "0.1¢" becomes "$0.001"): two decimals from a dollar
  // up, three under a dollar, and as many as it takes to show the first digit under a twentieth of a cent.
  function dollars(v) {
    if (v >= 1) return money(v);
    if (v >= 0.0005) return "$" + v.toFixed(3);
    let d = 4;
    while (d < 8 && Number(v.toFixed(d)) === 0) d++;
    return "$" + v.toFixed(d);
  }
  function pct(part, whole) {
    if (!whole) return "0%";
    const p = part / whole * 100;
    return (p >= 10 ? p.toFixed(0) : p >= 1 ? p.toFixed(1) : p < 0.05 ? "<0.1" : p.toFixed(1)) + "%";
  }
  function hm(d) { const h = d.getHours(), m = String(d.getMinutes()).padStart(2, "0"); return (h % 12 || 12) + ":" + m + (h < 12 ? " AM" : " PM"); }
  function hms(d) { const h = d.getHours(), m = String(d.getMinutes()).padStart(2, "0"), s = String(d.getSeconds()).padStart(2, "0"); return (h % 12 || 12) + ":" + m + ":" + s + (h < 12 ? " AM" : " PM"); }
  function esc(s) { return String(s).replace(/[&<>"]/g, c => ({ "&": "&amp;", "<": "&lt;", ">": "&gt;", '"': "&quot;" })[c]); }
  function k(key, text) { return '<span data-k="' + key + '">' + esc(text) + "</span>"; }
  // "high", "medium" -> "high and medium"; three -> "high, medium and low"
  function list(words) { return words.length < 2 ? words.join("") : words.slice(0, -1).join(", ") + " and " + words[words.length - 1]; }
  const byCost = obj => Object.keys(obj || {}).sort((a, b) => obj[b] - obj[a]);

  // ── the model marks: each family's versions on record and the effort applied ──
  function families(t) {
    const out = {};
    ORDER.forEach(m => { out[m] = { cost: 0, requests: 0, name: FAMILY_WORD[m], effort: "" }; });
    (t.families || []).forEach(f => {
      const slot = out[f.family];
      if (!slot) return;
      const names = byCost(f.names);
      // "Opus 5.5 and 5": the family's word once, then each version, most of the spend first.
      const versions = names.map((n, i) => i === 0 ? n : n.replace(FAMILY_WORD[f.family] + " ", ""));
      slot.cost = f.cost; slot.requests = f.requests;
      slot.name = list(versions) || FAMILY_WORD[f.family];
      const efforts = byCost(f.efforts);
      slot.effort = efforts.length
        ? list(efforts) + " effort" + (f.effortUnrecorded ? ", some not recorded" : "")
        : "effort not recorded";
    });
    return out;
  }
  function values(t) {
    const fam = families(t);
    const v = { reqs: int(t.requests), tokens: big(t.tokens), cacheRead: big(t.cacheRead) };
    ORDER.forEach(m => {
      v[m + ".name"] = fam[m].name;
      v[m + ".amt"] = money(fam[m].cost);
      v[m + ".pct"] = pct(fam[m].cost, t.cost);
      v[m + ".eff"] = fam[m].effort;
    });
    return { v, fam };
  }

  // ── the figure: every character its own span, so nothing clips and a refresh brings in only what changed ──
  function figureChars(s) {
    const dot = s.lastIndexOf(".");
    return s.split("").map((c, i) => ({ c, cls: c === "$" ? "cur" : i >= dot ? "dec" : "num" }));
  }
  function paintFigure(fig, v, animate) {
    const text = typeof v === "string" ? v : money(v);
    const next = figureChars(text);
    const prev = Array.prototype.map.call(fig.querySelectorAll(".ch"), n => n.textContent);
    // Aligned from the right, so $9,999.99 to $10,000.12 brings in the places that really changed.
    const off = next.length - prev.length;
    fig.innerHTML = next.map((ch, i) => {
      const isNew = animate && prev[i - off] !== ch.c;
      const delay = isNew ? ' style="animation-delay:' + ((next.length - 1 - i) * 34) + 'ms"' : "";
      return '<span class="ch ' + ch.cls + (isNew ? " is-new" : "") + '"' + delay + ' aria-hidden="true">' + esc(ch.c) + "</span>";
    }).join("");
    fig.setAttribute("aria-label", text);
    fitFigure(fig);
  }
  // Its stylesheet size (12.25rem: 196px at 100% text size, scaled with Text size like every other
  // size) when it fits the column; smaller, never wrapped, when it does not.
  function fitFigure(fig) {
    const col = fig.parentElement;
    if (!col || !col.clientWidth) return;
    const cs = getComputedStyle(col);
    const room = col.clientWidth - parseFloat(cs.paddingLeft) - parseFloat(cs.paddingRight);
    fig.style.fontSize = "";
    const size = parseFloat(getComputedStyle(fig).fontSize);
    const w = fig.getBoundingClientRect().width;
    if (w > room) fig.style.fontSize = Math.max(64, Math.floor(size * room / w)) + "px";
  }
  if (window.ResizeObserver) new ResizeObserver(() => { const f = $("spend-fig"); if (f) fitFigure(f); }).observe($("spend-body"));

  function barHTML(t, fam) {
    return '<div class="sp-bar" role="img" aria-label="Share of the spend by model">' + ORDER.map(m =>
      '<div class="sp-seg" data-m="' + m + '" style="flex-basis:' + (t.cost ? fam[m].cost / t.cost * 100 : 0) + '%"' + (fam[m].requests ? "" : " hidden") + "></div>").join("") + "</div>";
  }
  function legendHTML(t, v, fam) {
    return '<div class="sp-legend">' + ORDER.map(m =>
      '<p class="sp-key" data-m="' + m + '"' + (fam[m].requests ? "" : " hidden") + '><span class="sw-m" aria-hidden="true"></span>' +
        '<span class="l1"><b>' + k(m + ".name", v[m + ".name"]) + '</b><span class="amt">' + k(m + ".amt", v[m + ".amt"]) + "</span></span>" +
        '<span class="l2"><span class="pct">' + k(m + ".pct", v[m + ".pct"]) + '</span><span class="eff"><span class="eff-sep">· </span>' + k(m + ".eff", v[m + ".eff"]) + "</span></span></p>").join("") + "</div>";
  }

  // "x agents working" stays live (his words); the last request is priced in dollars.
  function liveText(live) {
    const n = live ? live.agentsWorking || 0 : 0;
    if (!n) return "No agents working right now";
    const who = n === 1 ? "1 agent working" : int(n) + " agents working";
    const last = live.last;
    if (!last) return esc(who);
    if (last.cost == null) return esc(who + " · last request " + last.name + ", " + int(last.tokens) + " tokens");
    return esc(who + " · last request " + last.name + ", " + int(last.tokens) + " tokens, ") + "<b>" + esc(dollars(last.cost)) + "</b>";
  }
  function paintLive() {
    const l = $("spend-live");
    if (!l) return;
    const working = !!(S.live && S.live.agentsWorking);
    l.classList.toggle("is-idle", !working);
    l.querySelector(".lv").innerHTML = liveText(S.live);
  }

  function renderMeter(view) {
    const t = view.totals;
    const { v, fam } = values(t);
    return '<div class="sp-meter">' +
      '<p class="sp-eyebrow sp-enter sp-e1">API-equivalent token spend · <span id="spend-when">' + esc(periodWords(S.period, view.asOf, t.firstAt)) + "</span></p>" +
      '<div class="sp-figure sp-enter sp-e2" id="spend-fig" role="img"></div>' +
      // Two sentences, two lines (CEO: "why is this text needlessly broken into 3 lines when it should be 2").
      '<p class="sp-sub sp-enter sp-e3"><span>What the same work would have cost at Claude’s API prices.</span><span><b>Billed by the API: nothing.</b> It ran on your Claude subscription.</span></p>' +
      '<div class="sp-enter sp-e3">' + barHTML(t, fam) + legendHTML(t, v, fam) + "</div>" +
      '<div class="sp-meter-foot sp-enter sp-e4"><p class="sp-tokens"><span><b>' + k("tokens", v.tokens) + "</b> tokens across <b>" + k("reqs", v.reqs) + "</b> requests.</span>" +
      "<span><b>" + k("cacheRead", v.cacheRead) + "</b> of them were read back from cache, which is what lets Rich keep the whole company in mind.</span></p>" +
      '<p class="sp-live" id="spend-live"><span class="working-dot" aria-hidden="true"></span><span class="lv"></span></p></div>' +
      "</div>";
  }
  // The first count reads the whole history once (round 22.1 draws no such state; nothing partial is shown).
  function renderCounting() {
    return '<div class="sp-meter"><p class="sp-eyebrow">API-equivalent token spend</p>' +
      '<p class="sp-sub sp-counting" role="status"><span>Counting every request on this Mac.</span><span>The first count reads the whole history once.</span></p></div>';
  }

  function footNote(view) {
    let note = "What the same work would have cost at Claude’s API prices as published on <b>" + esc(view.priceDate) +
      "</b>. Counted on this Mac from every request. <b>Not a bill.</b>";
    // Tokens counted and not priced: a model with no published price, or output a session reported
    // only in total that could not be put on one price (spend::ledger::Totals::unpriced_tokens).
    const u = view.totals && view.totals.unpricedTokens;
    if (u) note += " " + big(u) + " tokens are counted but not priced: no single published price matches them.";
    return note;
  }

  function stamp(view) {
    S.at = new Date(view.asOf);
    $("spend-updated").textContent = "Updated " + hms(S.at);
    $("spend-updated").title = "Brought up to date at " + hms(S.at);
    $("spend-refresh").classList.remove("has-new");
    $("spend-refresh").setAttribute("aria-label", "Refresh the numbers");
  }

  async function fetchView() {
    return call("spend_view", { from: periodStart(S.period, Date.now()) });
  }

  // Open, or a period picked: the whole screen drawn from a new snapshot.
  async function render() {
    const view = await fetchView();
    if (!view || !S.open) return;
    const body = $("spend-body");
    $("spend-title-when").textContent = "· " + SHORT[S.period];
    screen.querySelectorAll("#spend-period button").forEach(b => b.setAttribute("aria-pressed", String(b.dataset.p === S.period)));
    if (!view.ready) {
      S.shown = null;
      body.innerHTML = renderCounting();
      $("spend-foot").innerHTML = footNote(view);
      $("spend-updated").textContent = "";
      return;
    }
    S.shown = view;
    body.innerHTML = renderMeter(view);
    paintFigure($("spend-fig"), view.totals.cost, false);
    paintLive();
    $("spend-foot").innerHTML = footNote(view);
    stamp(view);
  }

  // Refresh: the numbers come up to date; what changed comes in, digit by digit from the right.
  async function refresh() {
    const btn = $("spend-refresh");
    btn.classList.remove("is-spinning"); void btn.offsetWidth; btn.classList.add("is-spinning");
    if (!S.shown) return render();
    const view = await fetchView();
    if (!view || !S.open) return;
    if (!view.ready) return render();
    S.shown = view;
    const t = view.totals;
    const { v, fam } = values(t);
    const body = $("spend-body");
    body.querySelectorAll("[data-k]").forEach(n => {
      const s = v[n.dataset.k];
      if (s == null || n.textContent === s) return;
      n.textContent = s;
      n.classList.remove("is-new"); void n.offsetWidth; n.classList.add("is-new");
    });
    $("spend-when").textContent = periodWords(S.period, view.asOf, t.firstAt);
    paintFigure($("spend-fig"), t.cost, true);
    body.querySelectorAll(".sp-seg").forEach(s => { s.style.flexBasis = (t.cost ? fam[s.dataset.m].cost / t.cost * 100 : 0) + "%"; s.hidden = !fam[s.dataset.m].requests; });
    body.querySelectorAll(".sp-key").forEach(s => { s.hidden = !fam[s.dataset.m].requests; });
    $("spend-foot").innerHTML = footNote(view);
    stamp(view);
  }

  // The live line, and the dot on Refresh when something landed since the snapshot.
  // One chain of reads per opening: `gen` moves at every open and close, and a read from an
  // earlier opening ends its chain instead of starting a second one.
  async function tick(gen) {
    if (gen !== S.gen || !S.open) return;
    const live = await call("spend_live");
    if (gen !== S.gen || !S.open) return;
    if (live) {
      S.live = live;
      paintLive();
      if (!S.shown && live.ready) render();
      else if (S.shown && live.counted > S.shown.totals.counted) {
        const btn = $("spend-refresh");
        if (!btn.classList.contains("has-new")) {
          btn.classList.add("has-new");
          btn.setAttribute("aria-label", "Refresh the numbers: new requests since " + hm(S.at));
        }
      }
    }
    S.timer = setTimeout(() => tick(gen), LIVE_MS);
  }

  function open() {
    if (window.RichSettings) window.RichSettings.close(false);
    if (S.open) return refresh();
    S.opener = document.activeElement;
    S.open = true;
    const gen = ++S.gen;
    if (!screen.isConnected) document.body.appendChild(screen);
    wordmark();
    screen.hidden = false;
    document.body.classList.add("spend-open");
    $("spend-body").innerHTML = "";
    render().then(() => tick(gen));
    $("spend-back").focus();
    paintRow();
  }
  // `keepFocus === true` when a sheet takes this screen's place: the focus belongs to the sheet.
  function close(keepFocus) {
    if (!S.open) return;
    S.open = false;
    S.gen++;
    clearTimeout(S.timer);
    screen.hidden = true;
    document.body.classList.remove("spend-open");
    const back = S.opener;
    S.opener = null;
    if (keepFocus !== true && back && document.contains(back) && typeof back.focus === "function" && back.id !== "set-spend-open") back.focus();
    paintRow();
  }
  $("spend-back").addEventListener("click", close);

  // A SETTINGS SHEET OPENED OVER THIS SCREEN TAKES ITS PLACE (the review of e7c316d11). Settings
  // stays above this screen and its rows still work, but the sheets they open live on the desk (an
  // `.overlay` at the body, z-index 60 or 160, or inside #app, which this screen hides), so one
  // opened while this screen is up sat under it and Settings looked dead. The same rule as Home's
  // giveWayToOpenDeskSheet (home.js): an `.overlay` that stops being hidden closes this screen,
  // and the sheet gets the focus its own open() could not give it while #app was hidden.
  const FOCUSABLE = "button:not([disabled]), [href], input:not([disabled]), select:not([disabled]), textarea:not([disabled]), [tabindex]:not([tabindex='-1'])";
  function giveWayToSheet(records) {
    if (!S.open) return;
    for (const r of records) {
      const nodes = r.type === "attributes" ? [r.target] : [...r.addedNodes];
      for (const n of nodes) {
        if (!(n instanceof Element) || !n.classList.contains("overlay") || n.hidden || screen.contains(n)) continue;
        close(true);
        if (!n.contains(document.activeElement)) {
          const first = [...n.querySelectorAll("button:not([disabled])")].find(e => e.offsetParent !== null)
            || [...n.querySelectorAll(FOCUSABLE)].find(e => e.offsetParent !== null);
          if (first) first.focus();
        }
        return;
      }
    }
  }
  if (typeof MutationObserver === "function") {
    new MutationObserver(giveWayToSheet).observe(document.body, { attributes: true, attributeFilter: ["hidden"], childList: true, subtree: true });
  }
  $("spend-refresh").addEventListener("click", refresh);
  $("spend-refresh").addEventListener("animationend", function () { this.classList.remove("is-spinning"); });
  screen.querySelectorAll("#spend-period button").forEach(b => b.addEventListener("click", () => {
    if (S.period === b.dataset.p) return;
    S.period = b.dataset.p;
    render();
  }));

  // ── the way in: the row in quick settings, with this month's figure as of the moment the menu opens ──
  function renderRow(slot) {
    S.slot = slot;
    slot.innerHTML = '<button class="set-spend" id="set-spend-open" type="button" role="menuitem">' +
      '<span class="set-name">Token spend</span><span class="set-spend-amt" id="set-spend-amt">' + S.rowAmt + "</span>" + CHEVRON + "</button>";
    slot.querySelector("#set-spend-open").addEventListener("click", open);
    paintRow();
  }
  function paintRow() {
    const row = document.getElementById("set-spend-open");
    if (!row) return;
    if (S.open) row.setAttribute("aria-current", "page");
    else row.removeAttribute("aria-current");
  }
  async function onMenuOpen() {
    paintRow();
    const view = await call("spend_view", { from: periodStart("month", Date.now()) });
    const amt = document.getElementById("set-spend-amt");
    if (!amt) return;
    S.rowAmt = view && view.ready ? "<b>" + esc(moneyRound(view.totals.cost)) + "</b> this month" : "";
    amt.innerHTML = S.rowAmt;
  }
  if (window.RichSettings && window.RichSettings.registerSpend) {
    window.RichSettings.registerSpend({ render: renderRow, onOpen: onMenuOpen });
  }

  // For the acceptance suite: paint any amount into the figure as the screen would (the gap walk).
  window.RichSpend = {
    open, close, refresh,
    isOpen: () => S.open,
    paintFigure: v => { const f = $("spend-fig"); if (f) paintFigure(f, v, false); return f ? f.textContent : null; },
  };
})();

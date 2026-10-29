"use strict";
// Desktop technical settings: the Claude Code quota sheet, built to round 16 (richos-hq
// design/mockups/rounds/round-16/, the design the CEO chose on 2026-10-04). With one account it
// is round 16's one-account sheet (its thirteen round-14 states in round 16's words, quota.html
// `STATES` `entry` ... `fast-one`) with "+ Add account" beside Refresh in every state, a reading
// or not (walk of nightly 36, D1; round 16's `unavailable` hides it, and without it a first
// account with no reading can never become two). With two or more: the account lanes in
// handover order, Add account and Remove inline, round 14's switch as the subject of ONE sentence
// whose verb is Pause or Switch, the lines that move while usage is fast (with a ghost of the
// normal line), the sheet after a switch, and the hold when every account is used up.
// Figures and pause evidence are independent reads.
(function () {
  const bridge = window.RichBridge;
  const sheet = document.createElement("div");
  sheet.id = "quota-sheet"; sheet.className = "overlay"; sheet.hidden = true;
  sheet.setAttribute("role", "dialog");
  sheet.setAttribute("aria-labelledby", "quota-title"); sheet.setAttribute("data-dismiss", "control:#quota-close");
  sheet.innerHTML = `<section class="overlay-panel quota-panel">
    <header class="quota-heading"><div><p class="quota-eyebrow">Settings · Technical view</p>
      <h2 id="quota-title">Claude Code quota</h2>
      <p id="quota-lede" class="quota-lede">Straight from Claude Code, shared across every app and session on this account.</p></div>
      <button id="quota-close" type="button" aria-label="Close Claude Code quota">×</button></header>
    <div class="quota-body"><div class="quota-windows-col">
      <div class="quota-toolbar"><span id="quota-freshness">Loading quota…</span>
        <div class="quota-toolbar-actions"><button id="quota-account-start" class="quota-btn quota-btn-quiet" type="button">+ Add account</button>
          <button id="quota-refresh" class="quota-btn" type="button"><svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2.2" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true"><path d="M21 12a9 9 0 1 1-2.64-6.36"/><path d="M21 3v6h-6"/></svg><span id="quota-refresh-label">Refresh</span></button></div></div>
      <p id="quota-message" class="quota-message" role="status" hidden></p>
      <div id="quota-lanes" class="quota-lanes" role="list" aria-label="Claude accounts" hidden></div>
      <div id="quota-account-new" class="quota-addform" role="group" aria-labelledby="quota-addform-title" hidden>
        <p id="quota-addform-title" class="quota-addform-title">Add a second Claude account</p>
        <div id="quota-addform-rows" class="quota-addform-rows">
          <label id="quota-account-current-row" class="quota-addform-row"><span>The account signed in now</span>
            <input id="quota-account-current" type="text" maxlength="40" placeholder="A name only you see"></label>
          <label class="quota-addform-row"><span>The new account</span>
            <input id="quota-account-label" type="text" maxlength="40" placeholder="A name only you see, like Work"></label></div>
        <div class="quota-addform-actions"><button id="quota-account-add" class="quota-btn quota-btn-primary" type="button">Add and sign in</button>
          <button id="quota-account-cancel" class="quota-btn" type="button">Cancel</button>
          <span class="quota-muted">Claude Code signs in through your browser.</span></div></div>
      <p id="quota-account-feedback" class="quota-account-feedback" role="status" aria-live="polite"></p>
      <div id="quota-windows" aria-label="Claude Code quota windows"></div>
      <div id="quota-empty" hidden><h3>Nothing to show yet.</h3><p>RichOS asks Claude Code for its own usage figures and shows exactly what comes back. <span id="quota-empty-why">Claude Code has not answered yet.</span> There is no number here: <b>not zero, not full, nothing guessed.</b></p><p id="quota-empty-next">If this stays empty after a refresh, Claude Code may not be signed in on this Mac.</p></div>
      <p class="quota-legend">The gold bar is what you have used; the tick is how far the clock has run. <b>Bar past the tick means you are spending faster than the window is passing.</b></p>
      <section id="quota-reset-offers" class="quota-reset-offers" aria-label="Weekly quota resets" hidden><button id="quota-usage-open" class="quota-btn" type="button" title="claude.ai/new#settings/usage">Open Claude Usage</button></section>
      <p id="quota-reset-feedback" role="status" aria-live="polite"></p>
    </div><form id="quota-policy" class="quota-policy" novalidate>
      <h3 id="quota-policy-title">Automatic pause</h3>
      <div class="quota-switch-row"><button id="quota-enabled" class="quota-switch" type="button" role="switch" aria-checked="false" aria-label="Automatically pause Rich’s agents"></button>
        <div class="quota-switch-label"><span id="quota-sentence-lead">Pause Rich’s agents once the five-hour window passes</span>
          <label class="quota-threshold"><span class="sr-only">Pause threshold, percent used</span><input id="quota-threshold" inputmode="numeric" type="text" maxlength="3" value="93" aria-describedby="quota-validation">% used</label><span id="quota-sentence-end"><span class="quota-muted">, unless the reset is under 20 minutes away.</span></span>
          <div id="quota-verbs" class="quota-opts" role="radiogroup" aria-label="What happens at the line" hidden>
            <button id="quota-verb-pause" class="quota-opt" type="button" role="radio" data-act="pause" aria-checked="true"><span class="quota-radio" aria-hidden="true"></span><span>pause Rich’s agents</span></button>
            <button id="quota-verb-switch" class="quota-opt" type="button" role="radio" data-act="switch" aria-checked="false"><span class="quota-radio" aria-hidden="true"></span><span id="quota-verb-switch-text">switch to the next account</span></button></div>
          <span id="quota-sentence-tail" class="quota-sentence-tail" hidden>unless the reset is under 20 minutes away.</span>
          <p id="quota-hint" class="quota-hint" hidden></p>
          <div id="quota-draft-actions" hidden><button id="quota-save" class="quota-btn quota-btn-primary" type="submit">Save</button>
            <button id="quota-keep" class="quota-btn" type="button">Keep 93%</button></div>
          <p id="quota-validation" role="status" hidden></p>
        </div></div>
      <p id="quota-save-status" role="status" aria-live="polite" hidden></p>
      <div id="quota-status-card" class="quota-status-card">
        <h4 id="quota-hold-status">Loading pause status…</h4>
        <div id="quota-hold-detail" class="quota-hold-detail"></div><ul id="quota-held" aria-label="Observed pauses"></ul>
        <div class="quota-status-actions"><button id="quota-hold-refresh" type="button" class="quota-btn" hidden>Refresh now</button>
          <button id="quota-release" type="button" class="quota-btn" hidden>Let them continue now</button></div>
      </div>
      <div id="quota-boundary-one" class="quota-boundary"><p><b>A pause is not a stop.</b> Each agent finishes the step it is on, then waits before the next, keeping its place and everything it knows. Because a step is allowed to finish, usage can climb a little past the line.</p>
        <p>Your conversation with Rich and any Claude Code you run outside RichOS, are never paused.</p></div>
      <div id="quota-boundary-many" class="quota-boundary" hidden><p><b>A pause is not a stop; a switch is not a restart.</b> Paused, an agent keeps its place and waits; switched, its next step runs on the next account with everything carried over.</p>
        <p>At <b>99%</b> of an account’s week Rich switches whatever you choose here, always to the account whose week ends soonest; when every account is used up, work waits for the soonest reset.</p></div>
    </form></div></section>`;
  document.body.appendChild(sheet);
  const field = id => sheet.querySelector("#" + id);
  // Feedback item 8 (the CEO 2026-10-06): which account is in use now and which is next, said
  // in one line above the lanes, in round 18's words (richos-hq design/mockups/rounds/round-18/).
  // "Use this one now" changes the first; the automatic rule (plan §15.1) picks what is next.
  const order = document.createElement("p");
  order.id = "quota-order"; order.className = "quota-order"; order.hidden = true;
  field("quota-lanes").before(order);
  // Open Claude Usage is the app's own control (round 16 has none). It is drawn only inside the
  // weekly-reset section, and that section only when there is a reset offer, an approval or a
  // reset attempt to show (`renderResets`), none of which round 16 draws: an uncertain reset
  // blocks automatic retry and tells the user to check Claude's Usage page, and this is the way
  // there. In every state round 16 draws, neither the section nor the button is on the sheet.
  const usageOpen = field("quota-usage-open");
  usageOpen.addEventListener("click", async () => {
    try { await bridge.invoke("open_external", { target: "claude.ai/new#settings/usage" }); }
    catch { field("quota-reset-feedback").textContent = "Could not open Claude Usage. Open claude.ai/new#settings/usage in your browser."; }
  });
  let view = null, activity = null, busy = false, saving = false, dirty = false, generation = 0;
  let resetDraft = null, resetSaving = false, resetPaint = "";
  let timer = null, lastPoll = 0, lastActivity = 0, activityBusy = false;
  // A span as round 16 writes one (quota.html fmtDur): "under a minute", "47 min",
  // "3 h 22 min", "4 d 2 h", to the nearest minute.
  const duration = ms => {
    const m = Math.round(Math.max(0, ms) / 60000);
    if (m < 1) return "under a minute";
    if (m < 60) return `${m} min`;
    const h = Math.floor(m / 60), r = m % 60;
    if (h < 24) return r ? `${h} h ${r} min` : `${h} h`;
    const d = Math.floor(h / 24), rh = h % 24;
    return rh ? `${d} d ${rh} h` : `${d} d`;
  };
  // Provider readings can differ by a second at a minute boundary. Round only
  // the displayed date; countdowns, window progress and quota policy keep the exact timestamp.
  const minuteDate = t => new Date(Math.round(t / 60000) * 60000);
  const clock = t => minuteDate(t).toLocaleTimeString(undefined, { hour: "numeric", minute: "2-digit" });
  const stamp = (t, weekly) => minuteDate(t).toLocaleString(undefined, { ...(weekly ? { weekday: "short" } : {}), hour: "numeric", minute: "2-digit" });
  // A time today reads as a clock; any other day carries its weekday (round 16's lanes).
  const when = t => minuteDate(t).toDateString() === new Date().toDateString() ? clock(t) : stamp(t, true);
  const esc = s => String(s).replace(/[&<>"]/g, c => ({ "&": "&amp;", "<": "&lt;", ">": "&gt;", "\"": "&quot;" }[c]));
  const poss = label => esc(label) + "’s";
  function node(tag, cls, text = "") { const n = document.createElement(tag); n.className = cls; n.textContent = text; return n; }
  // The last two words of a short line stay together, so a wrap never leaves one word alone
  // ("not / enforced." under the sentence, the walk of 2026-10-05 after nightly 36; the D4 kind).
  function keepTail(el, text) {
    const cut = text.lastIndexOf(" ", text.lastIndexOf(" ") - 1);
    if (cut < 0) { el.textContent = text; return; }
    el.replaceChildren(text.slice(0, cut + 1), node("span", "quota-keep", text.slice(cut + 1)));
  }
  function stale(now = Date.now()) { return !view || view.state !== "fresh" || !view.checkedAt || now < view.checkedAt || now - view.checkedAt >= view.refreshIntervalMs || view.windows.some(w => w.resetsAt && w.resetsAt <= now); }
  function validDraft() { return /^[0-9]{1,2}$/.test(field("quota-threshold").value) && Number(field("quota-threshold").value) >= 1; }
  function say(text) { field("quota-save-status").textContent = text; field("quota-save-status").hidden = !text; }

  // ---- Fill-first: several Claude accounts (plan richos-hq 2026-10-04 §15, round 16) ----------
  // Account 1 is always there, so one subscription reads as round 14. The view carries one row
  // per account only when there are two or more; every top-level field describes the account
  // IN USE.
  let accountBusy = false, signInTimer = null, selectedId = null, removeId = null, signingId = null;
  const pct = n => `${Math.round(n)}%`;
  const accounts = () => view?.accounts || [];
  const multi = () => accounts().length > 1;
  const inUseAcct = () => accounts().find(a => a.inUse) || null;
  const winOf = (a, id) => (a?.windows || []).find(w => w.id === id) || null;
  // No room: the app will not hand work to it until exhaustedUntil (claude_accounts.rs gone),
  // which includes a five-hour window past the line while the setting is switch.
  const isGone = a => a.exhaustedUntil > Date.now();
  // Used up, as round 16 draws it (quota.html isGone): a window at 100%, or Claude Code's own
  // usage limit before any reading, or every account held. An account past the line but under
  // 100% has no room yet still reads "week resets" with no tag (round 16's switched state).
  const isUsedUp = a => isGone(a) && (!a.windows.length || a.windows.some(w => w.usedPercent >= 100) || view?.heldUntil > Date.now());
  const weekReset = a => winOf(a, "seven_day")?.resetsAt || Infinity;
  // The next account, as the app chooses it (claude_accounts.rs next_account): among the others
  // that have a reading and room, the one whose weekly window resets soonest; ties keep list
  // order. There is no ordering control (his answer 1).
  function nextAcct() {
    const room = accounts().filter(a => !a.inUse && a.checkedAt != null && a.windows.length && !isGone(a));
    return room.reduce((best, a) => !best || weekReset(a) < weekReset(best) ? a : best, null);
  }
  // The lanes, in handover order: the one in use first, then the others by when their week
  // resets, soonest first, the used-up ones last.
  function lanesOrder() {
    const rest = accounts().filter(a => !a.inUse).map((a, i) => [a, i]);
    rest.sort(([a, i], [b, j]) => (isUsedUp(a) - isUsedUp(b)) || (weekReset(a) - weekReset(b)) || (i - j));
    return [...accounts().filter(a => a.inUse), ...rest.map(([a]) => a)];
  }
  const selectedAcct = () => accounts().find(a => a.id === selectedId && a.windows.length) || inUseAcct();
  // §108's speed: "fast" is 5 points or more per five-minute check (quota.rs FAST_POINTS_PER_CHECK);
  // an expected rise checks fast before anything is measured. Either way the app checks every
  // MINUTE (quota.rs FAST_REFRESH_INTERVAL_MS) — round 16's mockup says 2 minutes; the app's
  // measured value is the one shown.
  const fastNow = () => Object.values(view?.speeds || {}).some(s => s * 300000 >= 5);
  const expectedNow = () => !fastNow() && !!view && view.refreshIntervalMs < 300000;
  const fiveLine = () => Math.floor(view.actAt?.five_hour ?? view.policy.pausePercent);
  const weekLine = () => Math.floor(view.actAt?.seven_day ?? 99);
  const verb = () => multi() && view.atThreshold === "switch" ? "switch" : "pause";

  function ruler(window, hero, old, line) {
    const chart = node("div", "quota-chart" + (hero ? "" : " quota-chart-small"));
    const bar = node("div", "quota-bar");
    bar.setAttribute("role", "meter"); bar.setAttribute("aria-label", window.label + (old ? " last known usage" : " usage"));
    bar.setAttribute("aria-valuemin", "0"); bar.setAttribute("aria-valuemax", "100"); bar.setAttribute("aria-valuenow", String(window.usedPercent));
    bar.appendChild(node("span", "quota-track"));
    const fill = node("span", "quota-fill"); fill.style.width = Math.min(100, window.usedPercent) + "%"; bar.appendChild(fill);
    const now = Date.now();
    if (window.resetsAt > now && window.durationMs > 0 && window.resetsAt - window.durationMs <= now) {
      const elapsed = Math.max(0, Math.min(100, (1 - (window.resetsAt - now) / window.durationMs) * 100));
      const marker = node("span", "quota-marker"); marker.style.left = elapsed + "%"; marker.setAttribute("aria-hidden", "true");
      // The "now" label is hidden near either end so it never runs into "began" or "resets", as
      // round 16 draws it; where is measured, not a percentage (placeNow below).
      marker.append(node("i", ""), node("span", "", "now"));
      bar.appendChild(marker);
    }
    if (line) {
      // A line that moved while usage is fast leaves a dashed ghost where it was (round 16).
      if (line.was != null) {
        const ghost = node("span", "quota-pause-ghost"); ghost.style.left = line.was + "%"; ghost.setAttribute("aria-hidden", "true");
        ghost.appendChild(node("i", "")); bar.appendChild(ghost);
      }
      const mark = node("span", "quota-pause-line" + (line.on ? "" : " is-off") + (line.weekly ? " is-weekly" : ""));
      mark.style.left = line.at + "%";
      if (line.at < 35) mark.classList.add("is-left");
      mark.appendChild(node("i", ""));
      if (line.label) mark.appendChild(node("span", "", line.label));
      bar.appendChild(mark);
    }
    chart.appendChild(bar);
    if (window.resetsAt && window.durationMs > 0) {
      // Round 16: "began <b>9:57 PM</b>" and "resets <b>Mon 2:57 AM</b>", a weekday on any
      // day but today.
      const ends = node("div", "quota-ends"), ended = window.resetsAt <= now;
      const end = (word, t) => { const span = node("span", "", word + " "); span.appendChild(node("b", "", when(t))); return span; };
      ends.append(end("began", window.resetsAt - window.durationMs), end(ended ? "ended" : "resets", window.resetsAt));
      chart.appendChild(ends);
    }
    return chart;
  }
  // Walk of nightly 37, D5: round 16 hid "now" outside 11-86% of the hero ruler (16-76% on the
  // small ones), and a percentage cannot know how wide "began 10:20 AM" is. On a 639 px ruler the
  // label ran into "began" up to 21% and into "resets" from 80%, and the tick, which reaches
  // below the bar to its label, cut the tops of "began" up to 19%. So both are placed from the
  // laid-out stamps: the label shows only with NOW_CLEAR_PX of space to either stamp, and a tick
  // that is not that clear stops at the bottom of the bar (`is-short`, style.css).
  const NOW_CLEAR_PX = 8, TICK_HALF_PX = 2.5; // the 2 px tick and its 1.5 px halo, each side
  function placeNow(chart) {
    const marker = chart.querySelector(".quota-marker"), stamps = chart.querySelectorAll(".quota-ends > span");
    if (!marker || stamps.length !== 2) return;
    const began = stamps[0].getBoundingClientRect(), resets = stamps[1].getBoundingClientRect();
    if (!began.width) return; // not laid out yet (a hidden sheet): the observer places it when it is
    const clear = (left, right) => left >= began.right + NOW_CLEAR_PX && right <= resets.left - NOW_CLEAR_PX;
    const x = marker.getBoundingClientRect().left, label = marker.querySelector("span");
    marker.classList.toggle("is-short", !clear(x - TICK_HALF_PX, x + TICK_HALF_PX));
    label.hidden = false;
    const box = label.getBoundingClientRect();
    label.hidden = !clear(box.left, box.right);
  }
  const placeAll = () => { for (const chart of field("quota-windows").querySelectorAll(".quota-chart")) placeNow(chart); };
  // A window or pane resize moves the stamps without a render, so the list is watched too.
  if (window.ResizeObserver) new ResizeObserver(placeAll).observe(field("quota-windows"));
  // The five-hour line, on the account in use only: the verb at the threshold, "off" while the
  // switch is off, and while usage is fast the moved point with the normal one as its ghost.
  function fiveHourLine() {
    const t = view.policy.pausePercent, on = view.policy.enabled, at = on ? fiveLine() : t;
    const label = !on ? (multi() ? `off · ${t}%` : `pause off · ${t}%`) : at !== t ? `${verb()} at ${at}% · was ${t}%` : `${verb()} at ${t}%`;
    return { at, was: on && at !== t ? t : null, on, label };
  }
  function heroSub(sel) {
    const cur = inUseAcct(), nx = nextAcct();
    if (sel.inUse) return `in use: the one the ${view.policy.enabled ? fiveLine() : view.policy.pausePercent}% line watches`;
    if (isUsedUp(sel)) return `used up, usable again ${when(sel.exhaustedUntil)}`;
    if (nx && nx.id === sel.id) return `next: takes over when ${cur ? cur.label : "the account in use"} reaches its line`;
    return "not in use";
  }
  function renderWindows() {
    const list = field("quota-windows"); list.replaceChildren();
    const sel = multi() ? selectedAcct() : null, isCur = !sel || sel.inUse;
    const windows = sel ? sel.windows : (view.windows || []), hero = windows.find(w => w.id === "five_hour");
    const ordered = hero ? [hero, ...windows.filter(w => w !== hero)] : windows;
    for (const window of ordered) {
      const primary = window === hero, old = stale() || !!window.resetsAt && window.resetsAt <= Date.now();
      const row = node("section", "quota-window " + (primary ? "quota-hero" : "quota-weekly") + (old ? " quota-window--stale" : ""));
      const heading = node("div", "quota-window-summary");
      // Round 16's labels: "Five-hour window <sub>the one the pause watches</sub>", "Weekly
      // window", a model's own "Weekly · Fable"; with accounts each carries the account's name.
      const name = primary ? "Five-hour window" : window.id === "seven_day" ? "Weekly window" : window.label;
      const title = node("h3", "quota-window-label", sel ? `${name} · ${sel.label}` : name);
      if (primary) title.appendChild(node("span", "quota-muted", sel ? heroSub(sel) : "the one the pause watches"));
      heading.appendChild(title);
      const value = node("div", "quota-used", String(Math.round(window.usedPercent)));
      value.append(node("span", "quota-percent", "%"), node("span", "quota-unit", "used"));
      if (old) value.appendChild(node("span", "quota-tag", "stale"));
      heading.appendChild(value);
      // With a second account the week has a switch line: 99%, earlier while usage is fast.
      let line = primary && isCur ? fiveHourLine() : null;
      if (sel && window.id === "seven_day") {
        const at = isCur ? weekLine() : 99;
        line = { at, was: at !== 99 ? 99 : null, on: true, weekly: true };
        const sw = node("span", "quota-row-switch", `switches at ${at}%`);
        if (at !== 99) sw.appendChild(node("span", "quota-was", " · was 99%"));
        heading.appendChild(sw);
      }
      // Round 16: the hero "Resets in <b>3 h 22 min</b> · at Mon 2:57 AM", a row "resets in
      // <b>4 d 2 h</b>", and an ended window "This window <b>ended 4 min ago</b> — the new one
      // has no reading yet".
      const reset = node("div", "quota-reset"), left = window.resetsAt - Date.now();
      if (!window.resetsAt) reset.textContent = "Reset time unavailable";
      else if (left <= 0) reset.innerHTML = `${primary ? "This window" : "Window"} <b>ended ${esc(duration(-left))} ago</b>, the new one has no reading yet`;
      else reset.innerHTML = `${primary ? "Resets" : "resets"} in <b>${esc(duration(left))}</b>${primary ? ` · at ${esc(when(window.resetsAt))}` : ""}`;
      heading.appendChild(reset);
      row.append(heading, ruler(window, primary, old, line)); list.appendChild(row);
    }
    placeAll();
    if (windows.length && !sel) {
      // Round 16's `one-window` rows: the window's label, then "not reported by Claude Code for
      // this account". Round 16 names the model it drew (Sonnet); the app cannot know which
      // model is missing, so that row says "Weekly · per model".
      const absent = label => { const row = node("p", "quota-absent"); row.append(node("b", "", label), ": not reported by Claude Code for this account"); return row; };
      if (!hero) list.prepend(absent("Five-hour window"));
      if (!windows.some(w => w.id === "seven_day")) list.appendChild(absent("Weekly window"));
      if (!windows.some(w => w.id !== "five_hour" && w.id !== "seven_day")) list.appendChild(absent("Weekly · per model"));
    }
    field("quota-empty").hidden = !!windows.length;
    // A check that came back with no figures (quota.rs `empty_at`) is said as what it was:
    // Claude Code answered, without them. Round 16 draws only "has not answered yet".
    const answered = !windows.length && view.emptyAt && !view.message;
    field("quota-empty-why").textContent = view.message || (view.signedIn === true ? "This account is signed in on this Mac."
      : view.signedIn === false ? "This account is not signed in on this Mac."
      : answered ? "Claude Code answered without its usage figures this time." : "Claude Code has not answered yet.");
    const nextAsk = view.nextCheckAt > Date.now() ? duration(view.nextCheckAt - Date.now()) : null;
    field("quota-empty-next").textContent = view.signedIn === false ? "Close this panel, choose Claude accounts and sign in, then refresh."
      : answered && nextAsk ? `RichOS asks again in ${nextAsk}; Ask Claude Code asks now.`
      : "Ask Claude Code checks the usage figures again.";
    // Round 16 hides the ruler key while the Add account form is open.
    sheet.querySelector(".quota-legend").hidden = !windows.length || !field("quota-account-new").hidden;
  }

  // ---- the lanes ------------------------------------------------------------------------
  function accountNote(a) {
    if (isUsedUp(a)) { const limit = "Usage limit reached"; return `${limit} · usable again ${stamp(a.exhaustedUntil, true)}`; }
    if (a.message) return a.message;
    if (a.signedIn === true) return "This account is signed in on this Mac.";
    if (a.signedIn === false) return "This account is not signed in on this Mac.";
    return a.checkedAt ? "No allowance reported." : "Checking whether this account is signed in on this Mac.";
  }
  function figure(w, name, old) {
    const used = w ? Math.min(100, w.usedPercent) : 0;
    return `<span class="quota-fig${w && w.usedPercent >= 100 ? " is-gone" : ""}"><i class="quota-lane-mini${old ? " is-stale" : ""}" aria-hidden="true"><b style="width:${used}%"></b></i>${name} <b>${w ? pct(w.usedPercent) : "—"}</b></span>`;
  }
  function renderLanes() {
    const box = field("quota-lanes"), focus = document.activeElement;
    const keep = box.contains(focus) ? (focus.dataset.focus || "") : null;
    box.replaceChildren(); box.hidden = !multi(); order.hidden = !multi();
    if (!multi()) return;
    const nx = nextAcct(), many = accounts().length > 2, old = stale(), sel = selectedAcct(), cur = inUseAcct();
    // The order, plainly: the account in use now (his pick, or where the last switch went),
    // then the one Rich switches to when it must be left.
    order.innerHTML = !cur ? "" : `In use now: <b>${esc(cur.label)}</b>. `
      + (nx ? `Next: <b>${esc(nx.label)}</b>, whose week is fresh again soonest.` : "No other account has room right now.");
    order.hidden = !cur;
    for (const a of lanesOrder()) {
      const isNext = nx && nx.id === a.id, gone = isUsedUp(a), read = a.windows.length > 0, signing = signingId === a.id;
      const lane = node("div", "quota-lane" + (a.inUse ? " is-inuse" : "") + (isNext ? " is-next" : "") + (gone ? " is-gone" : "") + (removeId === a.id ? " is-confirm" : ""));
      lane.setAttribute("role", "listitem"); lane.dataset.id = a.id;
      if (read && !signing) {
        // Click a lane and the rulers below show that account; the in-use mark stays put.
        lane.tabIndex = 0; lane.dataset.focus = "lane-" + a.id;
        lane.setAttribute("aria-current", String(sel?.id === a.id));
        lane.setAttribute("aria-label", `${a.label}: show its windows below`);
      }
      const week = winOf(a, "seven_day");
      // A reading older than the normal check (a null or failed check since, quota.rs `empty`)
      // is shown with its age, never as current (handoff round 2, run 10: Work read 0% for 27
      // minutes while its use rose).
      const age = a.checkedAt && Date.now() - a.checkedAt >= 300000 ? duration(Date.now() - a.checkedAt) : null;
      let html =`<span class="quota-lane-mark" aria-hidden="true"></span><span class="quota-lane-main"><span class="quota-lane-head"><span class="quota-lane-label">${esc(a.label)}</span>`;
      if (a.inUse) html += `<span class="quota-lane-tag">in use</span>`;
      else if (isNext) html += `<span class="quota-lane-tag is-next">next</span>`;
      else if (gone) html += `<span class="quota-lane-tag is-gone">used up</span>`;
      if (signing) html += `<span class="quota-lane-when">signing in</span>`;
      else if (gone) html += `<span class="quota-lane-when">usable again <b>${esc(when(a.exhaustedUntil))}</b></span>`;
      else if (week?.resetsAt) html += `<span class="quota-lane-when">week resets <b>${esc(when(week.resetsAt))}</b>${isNext && many ? " · the soonest" : ""}</span>`;
      html += `</span>`;
      if (signing) html += `<span class="quota-lane-note"><span class="quota-lane-pulse" aria-hidden="true"></span>Claude Code is signing in through your browser. The reading arrives when it is done.</span>`;
      else if (!read) html += `<span class="quota-lane-note">${esc(accountNote(a))}</span>`;
      else html += `<span class="quota-lane-figs">${figure(winOf(a, "five_hour"), "five-hour", old || !!age)}${figure(week, "weekly", old || !!age)}${age ? `<span class="quota-fig quota-lane-age">read <b>${esc(age)}</b> ago</span>` : ""}</span>`;
      html += `</span><span class="quota-lane-side"></span>`;
      lane.innerHTML = html;
      const side = lane.querySelector(".quota-lane-side");
      // Account 1 is the user's own Claude Code sign-in and is never removed here
      // (claude_accounts.rs remove); every added account carries Sign in and Remove.
      // Use this one now (feedback item 8, round 18's words): any other account with a reading
      // and room can be the one in use. One with no room would be left again at once.
      if (!a.inUse && read && !signing && !isGone(a)) {
        const first = node("button", "quota-btn quota-btn-quiet quota-lane-first", "Use this one now"); first.type = "button"; first.disabled = accountBusy;
        first.setAttribute("aria-label", `Use ${a.label} now`); first.dataset.focus = "first-" + a.id;
        first.addEventListener("click", () => accountUseFirst(a.id));
        side.appendChild(first);
      }
      if (a.id !== "1") {
        // An account Claude Code says is signed in is never offered Sign in for want of a
        // reading (mid-job review rv-20261022T014956Z-56662dcd-dcc3, finding 1).
        if (!read && !signing && a.signedIn !== true) {
          const signIn = node("button", "quota-btn quota-btn-quiet", "Sign in"); signIn.type = "button"; signIn.disabled = accountBusy;
          signIn.setAttribute("aria-label", `Sign in to ${a.label}`); signIn.dataset.focus = "sign-" + a.id;
          signIn.addEventListener("click", () => accountSignIn(a.id));
          side.appendChild(signIn);
        }
        const remove = node("button", "quota-btn quota-btn-quiet quota-lane-remove", "Remove"); remove.type = "button"; remove.disabled = accountBusy;
        remove.setAttribute("aria-label", `Remove ${a.label}`); remove.dataset.focus = "remove-" + a.id;
        remove.addEventListener("click", () => { removeId = a.id; field("quota-account-new").hidden = true; render(); field("quota-remove-yes")?.focus(); });
        side.appendChild(remove);
      }
      if (removeId === a.id) {
        // One inline question, then gone.
        const ask = node("div", "quota-lane-confirm");
        ask.innerHTML = `<span>Remove <b>${esc(a.label)}</b> from this Mac? Its sign-in here is forgotten; nothing on the account itself changes.</span>`;
        const yes = node("button", "quota-btn quota-btn-primary", `Remove ${a.label}`); yes.type = "button"; yes.id = "quota-remove-yes"; yes.disabled = accountBusy;
        yes.addEventListener("click", () => accountRemove(a.id));
        const no = node("button", "quota-btn", "Keep it"); no.type = "button"; no.id = "quota-remove-no";
        no.addEventListener("click", () => { removeId = null; render(); box.querySelector(`[data-focus="remove-${a.id}"]`)?.focus(); });
        ask.append(yes, no); lane.appendChild(ask);
      }
      const choose = () => { if (lane.tabIndex === 0) { selectedId = a.id; render(); box.querySelector(`[data-focus="lane-${a.id}"]`)?.focus(); } };
      lane.addEventListener("click", event => { if (!event.target.closest("button")) choose(); });
      lane.addEventListener("keydown", event => { if ((event.key === "Enter" || event.key === " ") && event.target === lane) { event.preventDefault(); choose(); } });
      box.appendChild(lane);
    }
    if (keep) box.querySelector(`[data-focus="${keep}"]`)?.focus();
  }

  // ---- the decision: one sentence, the switch its subject, Pause or Switch its verb -----
  function renderDecision() {
    const many = multi(), cur = inUseAcct(), nx = nextAcct(), on = !!view.policy.enabled, t = view.policy.pausePercent;
    field("quota-policy-title").textContent = many ? "Automatic pause or switch" : "Automatic pause";
    field("quota-enabled").setAttribute("aria-label", many ? "Automatic pause or switch at the line" : "Automatically pause Rich’s agents");
    field("quota-sentence-lead").textContent = many ? `Once ${cur ? cur.label + "’s" : "the"} five-hour window passes` : "Pause Rich’s agents once the five-hour window passes";
    field("quota-sentence-end").innerHTML = many ? "," : `<span class="quota-muted">, unless the reset is under 20 minutes away.</span>`;
    field("quota-sentence-tail").hidden = !many;
    const verbs = field("quota-verbs");
    verbs.hidden = !many;
    verbs.classList.toggle("is-off", !on);
    for (const option of verbs.querySelectorAll(".quota-opt")) {
      option.setAttribute("aria-checked", String(option.dataset.act === (view.atThreshold || "pause")));
      option.disabled = !on || accountBusy || saving;
    }
    field("quota-verb-switch-text").innerHTML = nx ? `switch to <b>${esc(nx.label)}</b>, the next account` : `switch to the next account <span class="quota-muted">(none has room now)</span>`;
    // Round 16's hint under the sentence: with one account "Change the number to move the line."
    // while on and "Off: the line is only drawn, not enforced." while off; with several, only
    // the off line. A draft in progress shows Save and Keep (or the validation line) instead.
    const hint = field("quota-hint");
    hint.hidden = dirty || (many && on);
    keepTail(hint, hint.hidden ? "" : many ? `Off: nothing happens at ${t}%; the line is only drawn.`
      : on ? "Change the number to move the line." : "Off: the line is only drawn, not enforced.");
    field("quota-boundary-one").hidden = many;
    field("quota-boundary-many").hidden = !many;
  }

  // ---- the status card ------------------------------------------------------------------
  function speedNote() {
    const many = multi(), cur = inUseAcct(), t = view.policy.pausePercent, on = !!view.policy.enabled;
    const lines = (five, week) => many ? `<b>${five}%</b> and <b>${week}%</b>` : `<b>${five}%</b>`;
    if (fastNow()) {
      const [id, perMs] = Object.entries(view.speeds).sort((a, b) => b[1] - a[1])[0];
      const w = (view.windows || []).find(x => x.id === id);
      const name = id === "five_hour" ? "five-hour" : id === "seven_day" ? "weekly" : (w?.label || id);
      const whose = many && cur ? `${poss(cur.label)} ${name} window` : `The ${name} window`;
      // Round 16: "15 agents reading at once took Home's five-hour window from 40% to 71% in
      // 12 minutes." The rise is the two readings the speed was measured from and the count is
      // the agents working at the last count (quota.rs view: rises, agentsWorking), the same
      // two facts the conversation's alert says (quota.rs note_speed).
      const rise = view.rises?.[id], n = view.agentsWorking || 0;
      const took = whose.startsWith("The ") ? "the" + whose.slice(3) : whose; // never an account name
      let what = `${whose} is filling about ${Math.max(1, Math.round(perMs * 60000))}% a minute${w ? `, at ${pct(w.usedPercent)}` : ""}.`;
      if (rise) {
        const minutes = Math.max(1, Math.round(rise.ms / 60000));
        const span = `from ${Math.floor(rise.from)}% to ${Math.floor(rise.to)}% in ${minutes} ${minutes === 1 ? "minute" : "minutes"}`;
        what = n > 1 ? `${n} agents reading at once took ${took} ${span}.` : n === 1 ? `1 agent took ${took} ${span}.` : `${whose} went ${span}.`;
      }
      const five = fiveLine(), week = weekLine(), moved = five !== t || (many && week !== 99);
      let then;
      if (!on && !many) then = "Automatic pause is off, so nothing acts before it reaches 100%.";
      else if (!on) then = `The weekly switch ${week !== 99 ? `moved to <b>${week}%</b>` : "stays at <b>99%</b>"}; the five-hour line is off, so nothing acts there.`;
      else if (moved) then = `The ${many ? "lines" : "line"} moved to ${lines(five, week)} so nothing reaches 100%; ${many ? "they return" : "it returns"} to ${t}%${many ? " and 99%" : ""} when the speed comes back down.`;
      else then = `At this speed the next check still comes before 100%, so the ${many ? "lines stay" : "line stays"} at ${lines(t, 99)}.`;
      return { head: "Usage is fast. Checking every minute.", body: `${what} ${then}` };
    }
    if (expectedNow()) return { head: "A big rise is expected. Checking every minute.",
      body: `Several agents just started at once, so Rich checks every minute before the speed is even measured. The ${many ? "lines stay" : "line stays"} at ${lines(t, 99)} until a jump is measured.` };
    return null;
  }
  function renderStatus() {
    const held = activity?.held || [], released = activity?.released || [];
    const weeklyHeld = view?.windows?.some(w => w.id === "seven_day" && w.usedPercent >= 99 && w.resetsAt > Date.now());
    const enabled = view?.policy.enabled, state = view?.admission?.state, now = Date.now();
    const title = field("quota-hold-status"), detail = field("quota-hold-detail"), list = field("quota-held");
    const agents = held.filter(r => r.kind === "agent").length;
    const assignments = held.filter(r => r.kind === "assignment").length;
    const many = multi(), cur = inUseAcct(), nx = nextAcct(), allGone = many && view?.heldUntil > now;
    // Unknown or missing telemetry never creates a hold.
    const quiet = !held.length && !allGone && enabled;
    const noReading = quiet && (state === "unknown" || state === "noReading");
    const admitted = !enabled || state !== "held";
    let head, body, holding = !!held.length || allGone, acting = false;
    const five = (view?.windows || []).find(w => w.id === "five_hour");
    // Refresh is never locked: a failure's backoff holds only the automatic check (quota.rs
    // `due`), and round 16 says "Refresh asks sooner" beside it.
    field("quota-hold-refresh").hidden = !noReading;
    field("quota-hold-refresh").disabled = busy || saving;
    field("quota-release").hidden = !enabled || !held.length;
    field("quota-release").disabled = saving;
    field("quota-release").textContent = held.length ? "Let them continue now" : "Turn it off";
    if (allGone) {
      // Every account used up: held until the soonest reset, named by account.
      const back = accounts().filter(isGone).sort((a, b) => a.exhaustedUntil - b.exhaustedUntil), first = back[0];
      head = "Every account is used up.";
      body = back.map((a, i) => `<b>${esc(a.label)}</b> ${i ? "at" : "is usable again at"} ${esc(when(a.exhaustedUntil))}`).join("; ") + ". "
        + (agents ? `${agents} ${agents === 1 ? "agent holds its" : "agents hold their"} place and ${agents === 1 ? "resumes" : "resume"}` : "Agents hold their place and resume")
        + ` at <b>${esc(clock(view.heldUntil))}</b>, when ${first ? poss(first.label) : "the first"} window resets, the soonest.`;
    } else if (held.length) {
      const whose = many && cur ? `${poss(cur.label)} five-hour` : "The five-hour";
      const near = five && five.resetsAt > now && five.resetsAt - now < 20 * 60000;
      if (enabled && state === "ready" && near && agents) {
        // Round 16's `releasing`: under 20 minutes to the reset, the pause lifts.
        head = `Releasing: the reset is ${duration(five.resetsAt - now)} away.`;
        body = `Under 20 minutes to go, so the pause lifts: <b>${agents} ${agents === 1 ? "agent is" : "agents are"} picking up exactly where ${agents === 1 ? "it" : "they"} stopped.</b> The window resets at ${esc(clock(five.resetsAt))}.`;
      } else if (enabled && state === "held" && !weeklyHeld && agents && five && activity.resumesAt) {
        // Round 16's `holding` (one account) and `two-holding`.
        head = `Holding ${agents} ${agents === 1 ? "agent" : "agents"} since ${clock(Math.min(...held.map(r => r.sinceAt)))}.`;
        body = `${whose} window is at <b>${pct(five.usedPercent)} used</b>, past your ${fiveLine()}% line. ${many ? "" : agents === 1 ? "It has kept its place. " : "Each has kept its place. "}`
          // Inside the last 20 minutes they go one at a time, and only while the speed before the
          // pause keeps the account under 100% at the reset (quota/stagger.rs, Rich 2026-10-29).
          + (agents === 1 ? `It resumes at <b>${esc(clock(activity.resumesAt))}</b>, 20 minutes before the reset, if the speed before the pause keeps the account under 100%; otherwise at the reset`
            : `They resume one at a time from <b>${esc(clock(activity.resumesAt))}</b>, 20 minutes before the reset, while the speed before the pause keeps the account under 100%; the rest at the reset`)
          + (many ? "." : " or sooner if a fresh reading is back under the line.");
      } else {
        head = admitted ? "Releasing the pause…" : agents ? `${agents} ${agents === 1 ? "agent is" : "agents are"} paused` : assignments ? `${assignments} ${assignments === 1 ? "assignment is" : "assignments are"} paused` : "Waiting to start an agent";
        body = esc(admitted ? "The allowance permits work. Waiting for each pause to clear." : weeklyHeld ? "Weekly usage reached 99%. Waiting for a confirmed reset and available allowance." : activity.resumesAt ? `Can continue just after ${clock(activity.resumesAt)} when the reset is under 20 minutes away.` : "Waiting for a current five-hour reading. Their work is saved.");
        if (enabled) body += " Continuing now turns automatic pause off.";
      }
      if (many && enabled && view.atThreshold !== "switch" && nx) body += ` <b>${esc(nx.label)}</b> has room: choose <i>switch</i> above and they continue there now.`;
    } else if (released.length && admitted) {
      head = "Pause released"; body = "These waits have cleared. Work can continue from its saved place.";
    } else if (!enabled) {
      head = "Off. Nothing is paused.";
      // Round 16's `fresh` card with one account. With several the app keeps its own second
      // sentence, because a turned-away step runs again on the next account with room.
      body = many ? `Rich’s agents keep working through the line. If ${cur ? poss(cur.label) : "the"} five-hour window runs out, the step Claude Code turns away runs again on the next account with room. The switch at 99% of the week still happens; that is what a second account is for.`
        : "Rich’s agents keep working through the limit. When the five-hour window is spent, Claude Code turns them away until it resets and Rich tells you.";
    } else if (noReading) {
      // Not drawn in round 16: the check came back with no figures, so nothing is held on it.
      head = "No current reading, so nothing is held.";
      const ask = view.retryAt > now ? view.retryAt : view.nextCheckAt;
      body = `${state === "unknown" ? "There is no current usage reading" : view.message ? "The last check brought no usage figures" : "Claude Code’s last answer had no usage figures"} and the pause acts only on a number. Rather than wait on Claude Code, Rich’s agents keep working; the pause acts again once a reading shows the five-hour window past <b>${fiveLine()}%</b>.`
        + (ask > now ? ` RichOS asks again in <span class="quota-keep">${esc(duration(ask - now))}.</span>` : "");
    } else if (state === "held") {
      head = "Ready to pause"; body = esc(weeklyHeld ? "Weekly usage reached 99%. Agents will pause at their next step until allowance is confirmed." : `The five-hour allowance has reached ${view.policy.pausePercent}%. Agents will pause when they finish their current step. No pauses observed yet.`);
    } else {
      acting = true;
      const five = many ? winOf(cur, "five_hour") : (view.windows || []).find(w => w.id === "five_hour");
      const used = five ? `<b>${pct(five.usedPercent)} used</b>` : "not read yet", line = fiveLine();
      const whose = many ? poss(cur?.label || "The account in use") : "The";
      const from = accounts().find(a => a.id === view.lastSwitch?.from);
      if (many && from && cur && view.lastSwitch.to === cur.id) {
        // After a switch the card says who switched, from where, when, and what is next.
        const why = { fiveHour: `at ${Math.floor(view.lastSwitch.used)}% of its five-hour window`, weekly: `at ${Math.floor(view.lastSwitch.used)}% of its week`, limit: "when it reached a usage limit" }[view.lastSwitch.why] || "";
        head = `In use: ${cur.label}, since ${clock(view.lastSwitch.at)}.`;
        body = `Rich switched from <b>${esc(from.label)}</b> ${esc(duration(now - view.lastSwitch.at))} ago ${why}. Every agent’s next step ran on ${esc(cur.label)}; nothing restarted. `
          + (nx ? `When ${esc(cur.label)} reaches its line, the next is <b>${esc(nx.label)}</b>.` : "No other account has room right now, so at the line Rich pauses.");
      } else if (many && view.atThreshold === "switch") {
        head = "On. Rich switches at the line.";
        body = `${whose} five-hour window is at ${used}. At <b>${line}%</b>, every agent’s next step runs on <b>${esc(nx ? nx.label : "the next account")}</b>, the account whose week ends soonest and Rich says so in the conversation. Nothing stops.` + (nx ? "" : " No other account has room right now, so Rich would pause instead.");
      } else {
        // Round 16's `hold-idle` with one account, `two` / `choice-pause` with several.
        head = "On. Nothing is waiting.";
        body = `${whose} five-hour window is at ${used}. Agents pause the moment it passes <b>${line}%</b>, unless the reset is under 20 minutes away. Then it is not worth stopping.` + (many && nx && cur ? ` ${esc(nx.label)} stays idle until ${poss(cur.label)} week reaches 99%.` : "");
      }
    }
    const speed = view ? speedNote() : null;
    let paragraphs = [`<p>${body}</p>`];
    if (speed) {
      // Fast usage leads the card while nothing is held; under a hold it follows the hold.
      if (!holding && (acting || !enabled || state === "ready")) { head = speed.head; paragraphs.unshift(`<p>${speed.body}</p>`); holding = true; }
      else paragraphs.push(`<p>${speed.body}</p>`);
    }
    const unavailable = "Live pause details are unavailable.";
    if (!activity || activity.error) paragraphs.push(`<p>${unavailable}</p>`);
    field("quota-status-card").classList.toggle("is-holding", holding);
    // The status dot before the head (round 14 and 16): filled gold while on, a gold ring while
    // holding, breathing while usage is fast, an ink ring while off or with nothing to act on.
    const dot = node("span", "quota-status-dot" + (speed && !held.length && !allGone ? " is-fast" : holding ? " is-holding" : enabled && !noReading ? " is-on" : ""));
    dot.setAttribute("aria-hidden", "true");
    title.replaceChildren(dot, head);
    detail.innerHTML = paragraphs.join("");
    list.replaceChildren();
    for (const row of (held.length ? held : admitted ? released : [])) {
      const item = node("li", ""), description = node("span", "quota-held-description");
      // Round 16's one line: "Mark · Outbox retry — wiring the reconnect path".
      description.appendChild(node("strong", "", row.name));
      if (row.task) description.append(" · ", node("span", "", row.task));
      if (row.kind !== "agent") description.append(" · ", node("span", "", row.kind === "assignment" ? "Assignment" : "Agent dispatch"));
      item.append(description, node("span", "quota-held-time", (held.length ? "since " : "released ") + clock(held.length ? row.sinceAt : row.releasedAt)));
      list.appendChild(item);
    }
    list.hidden = !list.children.length;
  }
  function paintMenu() {
    const row = document.getElementById("set-quota-open"), text = document.getElementById("set-quota-state");
    if (!row || !text) return;
    // The WEEKLY window, labeled (the CEO 2026-10-06: "not supposed to be the weekly?"): the
    // account switch happens at 99% of the week. The sheet itself keeps both windows.
    const week = view?.windows?.find(w => w.id === "seven_day"), n = activity?.held?.filter(r => r.kind === "agent").length || 0;
    const cur = inUseAcct(), nx = multi() ? nextAcct() : null;
    let reading = week ? `${Math.round(week.usedPercent)}% weekly used` : "";
    if (week && multi() && cur) reading = `${cur.label} ${Math.round(week.usedPercent)}% weekly${nx ? " · next " + nx.label : ""}`;
    text.textContent = multi() && view?.heldUntil > Date.now() ? "every account used up" : n ? `holding ${n} ${n === 1 ? "agent" : "agents"}` : week ? `${reading}${stale() ? " · stale" : ""}${fastNow() ? " · fast" : ""}` : "no reading";
    let mini = row.querySelector(".quota-mini");
    if (!mini) { mini = node("span", "quota-mini"); mini.setAttribute("aria-hidden", "true"); mini.appendChild(node("i", "")); row.insertBefore(mini, row.lastChild); }
    mini.hidden = !week;
    mini.classList.toggle("is-stale", stale()); mini.firstChild.style.width = Math.min(100, week?.usedPercent || 0) + "%";
  }
  const resetLimitName = id => ({ five_hour: "five-hour quota", seven_day: "weekly quota", seven_day_overage_included: "weekly allowance including overage" }[id] || id.replaceAll("_", " "));
  function renderResets() {
    const r = view?.resets || { state: "unknown", offers: [] }, now = Date.now();
    const fresh = r.state === "fresh" && r.checkedAt <= now && now - r.checkedAt < 300000;
    const offers = (r.offers || []).filter(o => o.remaining > 0 && o.expiresAt > now);
    const key = JSON.stringify([r, fresh, offers.map(o => o.id), resetDraft, resetSaving]);
    if (key === resetPaint) return;
    resetPaint = key;
    const container = field("quota-reset-offers"), focus = document.activeElement?.id;
    container.replaceChildren();
    const keepUsage = () => container.appendChild(usageOpen);
    // Nothing to approve, revoke or check: round 16 draws nothing below the ruler key, so
    // neither does this (no "No reset offer is available." line, no Open Claude Usage).
    const compact = !offers.length && !r.approval && !r.lastAttempt;
    container.hidden = compact;
    if (compact) return;
    container.appendChild(node("h3", "", "Weekly quota reset"));
    container.appendChild(node("p", "quota-muted", "Approve in advance. Rich will use one reset automatically at 99% weekly usage while the desktop app or terminal watcher is running. They share one approval and attempt record. Checks every 5 minutes. Five-hour usage never triggers it."));
    if (r.lastAttempt) {
      const text = { used: "Your approved reset was used. Checking the new allowance…", alreadyUsed: "Anthropic reports this reset was already used.", notUsed: "Anthropic did not use the reset. The approval has ended.", uncertain: "The reset outcome is unconfirmed. Automatic retry is blocked. Check Claude’s Usage page before taking further action." }[r.lastAttempt.outcome] || "Reset outcome unavailable. Check Claude’s Usage page.";
      container.appendChild(node("p", "quota-reset-result", text));
    }
    if (!fresh) container.appendChild(node("p", "quota-muted", r.message || "Weekly reset availability is unknown. Refresh to check again."));
    if (fresh && !offers.length) container.appendChild(node("p", "quota-muted", "No reset offer is available."));
    for (const offer of offers) {
      const card = node("div", "quota-reset-offer");
      card.appendChild(node("h4", "", offer.label || "Claude usage-limit reset"));
      card.appendChild(node("p", "", `${offer.remaining} ${offer.remaining === 1 ? "reset" : "resets"} ${fresh ? "available" : "last reported"} · Expires ${new Date(offer.expiresAt).toLocaleString(undefined, { month: "short", day: "numeric", year: "numeric", hour: "numeric", minute: "2-digit" })}`));
      card.appendChild(node("p", "quota-muted", "Resets " + offer.clears.map(resetLimitName).join(", ") + "."));
      const approved = r.approval?.offer.id === offer.id && r.approval.offer.expiresAt > now;
      const weekly = offer.clears.includes("seven_day");
      const attempted = r.lastAttempt?.grantId === offer.id || r.lastAttempt?.outcome === "uncertain";
      if (approved) {
        card.appendChild(node("p", "quota-reset-armed", fresh ? "Approved and ready · one use at 99% weekly usage. Approval expires with this offer." : "Approved in advance · waiting for a fresh eligibility check."));
      } else if (!weekly) card.appendChild(node("p", "quota-muted", "This offer does not reset weekly quota."));
      else if (offer.requiresLimit) card.appendChild(node("p", "quota-muted", "Anthropic requires an exhausted limit for this offer; it may not be usable at 99%."));
      if (weekly && !approved && !attempted) {
        const prepare = node("button", "quota-btn", "Allow Rich to use this reset…"); prepare.type = "button";
        prepare.id = "quota-reset-prepare-" + offer.id; prepare.disabled = !fresh || resetSaving || offer.startsAt > now;
        prepare.addEventListener("click", () => { resetDraft = structuredClone(offer); renderResets(); field("quota-reset-confirm")?.focus(); });
        card.appendChild(prepare);
      }
      container.appendChild(card);
    }
    // Revocation remains available if the account reader is offline or the offer vanished.
    if (r.approval) {
      const revoke = node("button", "quota-btn", "Revoke reset approval"); revoke.type = "button"; revoke.id = "quota-reset-revoke";
      revoke.disabled = resetSaving; revoke.addEventListener("click", () => resetPermission(false)); container.appendChild(revoke);
    }
    if (resetDraft) {
      const confirm = node("div", "quota-reset-confirm");
      confirm.appendChild(node("p", "", `Allow Rich to use one “${resetDraft.label}” reset automatically when weekly usage reaches 99%? This approval is only for this offer, expires with it and can be revoked before use. Using a reset cannot be undone.`));
      const yes = node("button", "quota-btn quota-btn-primary", "Approve one automatic reset"); yes.type = "button"; yes.id = "quota-reset-confirm";
      yes.disabled = resetSaving || !fresh; yes.addEventListener("click", () => resetPermission(true));
      const no = node("button", "quota-btn", "Keep unapproved"); no.type = "button"; no.disabled = resetSaving;
      no.addEventListener("click", () => { resetDraft = null; renderResets(); container.querySelector("button")?.focus(); });
      confirm.append(yes, no); container.appendChild(confirm);
    }
    keepUsage();
    if (focus?.startsWith("quota-reset-") || focus === "quota-usage-open") field(focus)?.focus();
  }
  async function resetPermission(allow) {
    if (resetSaving || (allow && !resetDraft)) return;
    const offer = resetDraft;
    resetSaving = true; generation++; busy = false; field("quota-reset-feedback").textContent = ""; renderResets();
    try {
      const next = await bridge.invoke(allow ? "approve_claude_reset" : "revoke_claude_reset", allow ? { offer } : {});
      view.resets = next; resetDraft = null;
      field("quota-reset-feedback").textContent = allow ? "Approved in advance. Rich is ready to use one reset at 99% weekly usage." : "Reset approval revoked.";
    } catch (error) {
      field("quota-reset-feedback").textContent = typeof error === "string" ? error : "Could not change reset approval. Refresh and try again.";
    } finally {
      resetSaving = false; renderResets();
      if (!sheet.hidden) (field("quota-reset-revoke") || field("quota-reset-confirm") || field("quota-reset-offers").querySelector("button"))?.focus();
    }
  }

  // ---- Add account, Sign in, Remove, the verb -------------------------------------------
  // A line under the lanes for what Add, Sign in and Remove did. Round 16 says these as a
  // passing toast, so a success fades after 3.6 s (its toast's time) and is never left under
  // the rows in a steady state; a failure stays until the next action.
  let feedbackTimer = null;
  function feedback(text, passing = false) {
    const line = field("quota-account-feedback");
    clearTimeout(feedbackTimer); line.textContent = text;
    if (passing && text) feedbackTimer = setTimeout(() => { if (line.textContent === text) line.textContent = ""; }, 3600);
  }
  async function accountCall(command, args, done) {
    if (accountBusy) return;
    accountBusy = true; feedback(""); render();
    try { const next = await bridge.invoke(command, args); if (next && next.windows) view = next; if (done) done(next); }
    catch (error) { feedback(typeof error === "string" ? error : "That change could not be made. Nothing changed."); }
    finally { accountBusy = false; if (!sheet.hidden) render(); }
  }
  function watchSignIn() {
    clearInterval(signInTimer);
    signInTimer = setInterval(async () => {
      let answer = null;
      try { answer = await bridge.invoke("claude_account_sign_in_poll", {}); } catch (_) {}
      if (answer && answer[1]?.state === "connecting") { if (signingId !== answer[0]) { signingId = answer[0]; render(); } return; }
      clearInterval(signInTimer); signInTimer = null; signingId = null;
      const connected = answer?.[1]?.state === "connected", reading = "Signed in. Reading its allowance…";
      if (answer) feedback(connected ? reading : (answer[1]?.message || ""), connected);
      // The reading that arrives ends "Reading its allowance…": the lane takes its place.
      await refresh(true);
      if (field("quota-account-feedback").textContent === reading) feedback("");
    }, 2000);
  }
  function openAdd() {
    // Going from one account to two names both: the first one never needed a label before.
    const first = !multi();
    removeId = null;
    field("quota-account-new").hidden = false;
    field("quota-addform-title").textContent = first ? "Add a second Claude account" : "Add another Claude account";
    field("quota-account-current-row").hidden = !first;
    field("quota-addform-rows").classList.toggle("is-one", !first);
    feedback("");
    render();
    (first ? field("quota-account-current") : field("quota-account-label")).focus();
  }
  function closeAdd() {
    field("quota-account-new").hidden = true; field("quota-account-label").value = ""; field("quota-account-current").value = "";
    render(); field("quota-account-start").focus();
  }
  function accountAdd() {
    const label = field("quota-account-label").value;
    const currentLabel = multi() ? null : field("quota-account-current").value;
    accountCall("claude_account_add", { label, currentLabel }, next => {
      field("quota-account-label").value = ""; field("quota-account-current").value = "";
      field("quota-account-new").hidden = true;
      const added = (next?.accounts || []).reduce((top, a) => !top || Number(a.id) > Number(top.id) ? a : top, null);
      signingId = added ? added.id : null;
      feedback("Claude Code opened its sign-in page in your browser. Finish there; the reading arrives here.", true);
      watchSignIn();
    });
  }
  function accountSignIn(id) {
    accountCall("claude_account_sign_in", { id }, () => {
      signingId = id;
      feedback("Finish the sign-in in your browser.", true);
      watchSignIn();
    });
  }
  function accountUseFirst(id) {
    const pick = accounts().find(a => a.id === id);
    accountCall("claude_account_use_first", { id }, () => {
      if (selectedId === id) selectedId = null;
      // Round 18's confirmation: the account near its limit says when Rich switches again
      // (the CEO's own test, the 97% account); otherwise which account is next.
      const week = winOf(inUseAcct(), "seven_day")?.usedPercent, nx = nextAcct();
      const then = week >= 90 ? ` ${pick.label} has already used ${Math.floor(week)}% of its week, so Rich will switch again when it reaches 99%.`
        : nx ? ` ${nx.label} is next.` : "";
      feedback(pick ? `Rich now uses ${pick.label}.${then}` : "", true);
      field("quota-lanes").querySelector(".quota-lane-first")?.focus();
    });
  }
  function accountRemove(id) {
    const gone = accounts().find(a => a.id === id);
    accountCall("claude_account_remove", { id }, () => {
      removeId = null; if (selectedId === id) selectedId = null;
      feedback(gone ? `${gone.label} removed from this Mac. Sign in again to add it back.` : "Account removed.", true);
    });
  }
  field("quota-account-start").addEventListener("click", openAdd);
  field("quota-account-add").addEventListener("click", accountAdd);
  field("quota-account-cancel").addEventListener("click", closeAdd);
  for (const id of ["quota-account-label", "quota-account-current"]) field(id).addEventListener("keydown", event => {
    if (event.key === "Enter") { event.preventDefault(); accountAdd(); }
  });
  for (const option of sheet.querySelectorAll(".quota-opt")) option.addEventListener("click", () => {
    if (!view?.policy.enabled || option.getAttribute("aria-checked") === "true") return;
    accountCall("set_claude_at_threshold", { value: option.dataset.act }, () => say("Saved."));
  });

  function render() {
    paintMenu();
    // Refresh is never locked by a failure's backoff: that wait holds only the automatic check
    // (quota.rs `due`); a person who asks is owed a real check (round 16: "Refresh asks
    // sooner"). Walk of nightly 36, D1: a null answer locked it for 10 minutes.
    field("quota-refresh").disabled = busy || saving;
    // Round 16's Refresh: the circular-arrow icon, "Asking Claude Code…" while it asks, and
    // "Ask Claude Code" while there is no reading at all.
    field("quota-refresh").classList.toggle("is-busy", busy);
    field("quota-refresh-label").textContent = busy ? "Asking Claude Code…" : view && !view.windows.length ? "Ask Claude Code" : "Refresh";
    if (!view) return;
    const now = Date.now(), age = view.checkedAt && view.windows.length ? duration(now - view.checkedAt) + " ago" : null;
    const freshness = field("quota-freshness");
    freshness.classList.toggle("is-stale", !!age && stale());
    if (!age) freshness.replaceChildren(node("b", "", view.signedIn === false ? "Not signed in on this Mac."
      : view.signedIn === true ? "Signed in; no usage reading." : "No usage reading yet."));
    else {
      // The cadence: every 5 minutes at every level of use; every MINUTE while usage is fast or
      // a rise is expected, in gold (round 16's reading line, the app's measured interval).
      // Round 16's words: "Checked 3 min ago", or "Last reading 47 min ago — stale".
      // Two parts that each keep together, so the line wraps between them and never leaves a
      // lone word ("usage is / fast", "every 5 / min": walk of nightly 36, D4).
      const quick = fastNow() ? "every minute (usage is fast)" : expectedNow() ? "every minute (a rise is expected)" : null;
      const when = node("span", "quota-reading-part");
      when.append(node("b", "", stale() ? `Last reading ${age} (stale)` : `Checked ${age}`), " ·");
      freshness.replaceChildren(when, " ",
        quick ? node("span", "quota-reading-part quota-fast", quick) : node("span", "quota-reading-part", `checks every ${Math.round(view.refreshIntervalMs / 60000)} min`));
    }
    // Round 16's `refresh-failed` notice: what happened, how old the figures are, when RichOS
    // tries again, and that Refresh asks sooner. The same shape says a null answer over figures
    // from earlier ("answered without its usage figures"), and an unreadable answer is still
    // named as unreadable (quota.rs `ReadError`).
    const message = field("quota-message"), again = view.retryAt > now ? view.retryAt : view.nextCheckAt > now ? view.nextCheckAt : null;
    const said = view.message || (view.emptyAt && view.windows.length ? "Claude Code answered without its usage figures just now." : "");
    if (said && view.windows.length && view.checkedAt) {
      message.innerHTML = `<b>${esc(said)}</b> The figures below are from ${esc(duration(now - view.checkedAt))} ago and may have moved on.`
        + (again ? ` RichOS ${view.message ? "tries" : "asks"} again in ${esc(duration(again - now))}; Refresh asks sooner.` : "");
    } else message.textContent = said + (said && again ? ` RichOS tries again in ${duration(again - now)}.` : "");
    message.hidden = !message.textContent;
    const many = multi();
    sheet.querySelector(".quota-panel").classList.toggle("is-multi", many);
    field("quota-lede").textContent = many ? "Straight from Claude Code, for each account signed in on this Mac." : "Straight from Claude Code, shared across every app and session on this account.";
    if (selectedId && !accounts().some(a => a.id === selectedId)) selectedId = null;
    // + Add account is offered in every state, a reading or not (walk of nightly 36, D1): it is
    // the only way to a second account, and the first one may have no reading for a while.
    field("quota-account-start").hidden = !field("quota-account-new").hidden;
    field("quota-account-start").disabled = accountBusy;
    field("quota-account-add").disabled = accountBusy;
    renderLanes();
    renderWindows();
    renderResets();
    field("quota-enabled").setAttribute("aria-checked", String(view.policy.enabled));
    field("quota-enabled").disabled = saving || !!view.policyUnavailable;
    if (!dirty) field("quota-threshold").value = view.policy.pausePercent;
    field("quota-threshold").disabled = !view.policy.enabled || saving || !!view.policyUnavailable;
    field("quota-threshold").setAttribute("aria-invalid", String(dirty && !validDraft()));
    field("quota-draft-actions").hidden = !dirty;
    field("quota-save").disabled = saving || !validDraft();
    field("quota-save").textContent = `Save ${field("quota-threshold").value}%`;
    field("quota-keep").textContent = `Keep ${view.policy.pausePercent}%`;
    field("quota-keep").disabled = saving;
    field("quota-validation").hidden = !dirty || validDraft();
    field("quota-validation").textContent = `Pick a whole number from 1 to 99. It is still ${view.policy.pausePercent}% until you save.`;
    renderDecision();
    renderStatus();
  }
  async function readActivity() {
    if (activityBusy) return;
    activityBusy = true;
    try { activity = await bridge.invoke("claude_quota_activity", { threadId: null }); }
    catch (_) { activity = { held: [], released: [], error: true }; }
    finally { activityBusy = false; lastActivity = Date.now(); if (!sheet.hidden && view) renderStatus(); paintMenu(); }
  }
  async function refresh(force) {
    if (busy || saving || resetSaving) return;
    const request = generation; busy = true; if (!sheet.hidden) render();
    try {
      const next = await bridge.invoke("claude_quota", { refresh: force });
      if (request === generation) view = next;
    } catch (_) {
      if (request !== generation) return;
      view = { ...(view || { windows: [], checkedAt: null, refreshIntervalMs: 300000, policyUnavailable: true, policy: { enabled: false, pausePercent: 93 } }), state: "unavailable", message: "Could not read Claude Code quota. Try refreshing again." };
    } finally {
      if (request === generation) { busy = false; lastPoll = Date.now(); if (!sheet.hidden) render(); paintMenu(); }
    }
  }
  async function save(policy) {
    if (saving || !view || view.policyUnavailable) return;
    const request = ++generation; busy = false; saving = true; say(""); render();
    try {
      const next = await bridge.invoke("set_claude_quota_policy", { policy });
      if (request !== generation) return;
      view = next; dirty = false; say("Saved."); readActivity();
    } catch (_) { if (request === generation) say("Could not save. Your previous setting is still active."); }
    finally { saving = false; if (!sheet.hidden) render(); paintMenu(); }
  }
  function keep() { dirty = false; say(""); render(); }
  function close() {
    resetDraft = null; resetPaint = ""; removeId = null; selectedId = null;
    field("quota-account-new").hidden = true;
    sheet.hidden = true; generation++; busy = false; clearInterval(timer); timer = null;
    queueMicrotask(() => { if (sheet.hidden) document.getElementById("set-btn")?.focus(); });
  }
  async function open() {
    clearInterval(timer); generation++; busy = false; sheet.hidden = false; dirty = false; say("");
    selectedId = null; removeId = null;
    field("quota-close").focus(); render(); readActivity();
    timer = setInterval(() => {
      if (document.getElementById("set-quota-open")?.hidden) { close(); return; }
      // A form in progress is never repainted under the cursor; the clocks catch up after it.
      if (sheet.contains(document.activeElement) && document.activeElement.matches("#quota-account-new input")) return;
      if (Date.now() - lastPoll >= 30000) refresh(false); else render();
      if (Date.now() - lastActivity >= 3000) readActivity();
    }, 1000);
    await refresh(false);
  }
  field("quota-close").addEventListener("click", close);
  for (const id of ["quota-refresh", "quota-hold-refresh"]) field(id).addEventListener("click", () => refresh(true));
  field("quota-enabled").addEventListener("click", () => view && save({ ...view.policy, enabled: !view.policy.enabled }));
  field("quota-release").addEventListener("click", () => view && save({ ...view.policy, enabled: false }));
  field("quota-threshold").addEventListener("input", () => { dirty = field("quota-threshold").value !== String(view.policy.pausePercent); say(""); render(); });
  field("quota-keep").addEventListener("click", keep);
  field("quota-policy").addEventListener("submit", event => { event.preventDefault(); if (validDraft() && dirty) save({ ...view.policy, pausePercent: Number(field("quota-threshold").value) }); });
  sheet.addEventListener("keydown", event => {
    if (event.key === "Escape") {
      event.stopPropagation(); event.preventDefault();
      if (resetDraft && !resetSaving) { resetDraft = null; renderResets(); }
      else if (!field("quota-account-new").hidden) closeAdd();
      else if (removeId) { const id = removeId; removeId = null; render(); field("quota-lanes").querySelector(`[data-focus="remove-${id}"]`)?.focus(); }
      else if (dirty && !saving) keep(); else close();
    }
    if (event.key === "Tab") {
      const controls = [...sheet.querySelectorAll("button, input, [tabindex='0']")].filter(n => !n.disabled && n.getClientRects().length);
      if (event.shiftKey && document.activeElement === controls[0]) { event.preventDefault(); controls.at(-1)?.focus(); }
      else if (!event.shiftKey && document.activeElement === controls.at(-1)) { event.preventDefault(); controls[0]?.focus(); }
    }
  });
  window.RichSettings.registerQuota({ open, paint: paintMenu });
  // The settings preview stays current while visible. These reads use the same
  // provider cache/backoff as the sheet; pause reads never contact Claude.
  setInterval(() => {
    if (document.hidden || !sheet.hidden) return;
    const row = document.getElementById("set-quota-open");
    if (!row || !row.getClientRects().length) return;
    if (!view || Date.now() - lastPoll >= 30000) refresh(false);
    if (Date.now() - lastActivity >= 3000) readActivity();
  }, 1000);
})();

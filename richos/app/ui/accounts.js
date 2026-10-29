"use strict";
// CLAUDE ACCOUNTS, FOR SOMEONE WHO IS NOT TECHNICAL: round 18 (richos-hq
// design/mockups/rounds/round-18/multi-account.html and NOTES.md), measured against the CEO's
// words, item 9 of richos-hq docs/ceo-input/2026-10-06_01/feedback.md: "Non-technical users
// should also be able to take advantage of the multi-account setup".
//
// - THE ROW, FOR EVERYONE. Settings carries "Claude accounts" whatever Technical view is set to,
//   in the place of the old account row, with one plain line under it and the week's mini bar.
//   Technical view still adds the detailed "Claude Code quota" row (quota.js), unchanged.
// - THE SHEET IS CARDS. One card per account, in the order Rich uses them: In use now (the gold
//   spine), then Next, Resting or Ready. Two meters each: Weekly limit and 5-hour limit.
// - ADDING A SECOND ACCOUNT IN TWO STEPS: name both, then Open sign-in (the app's existing
//   browser sign-in, claude_account_add). "That is the account you already use" when the
//   sign-in comes back as an account this Mac already has (provider_auth::same_account).
// - WHY IT SWITCHED, IN THREE PLACES: Rich's line in the conversation (written by the back end,
//   claude_accounts.rs; this file draws See your accounts under it), the banner at the top of
//   the sheet, and Recent changes.
// - Rich's one-time suggestion of a second account and the "close to their weekly limit" line are
//   written by the back end (quota.rs note_accounts); this file draws their buttons.
//
// Every figure is the app's own (claude_quota); the mockup's sample data is not used anywhere.
// Times are en-US, as round 18 draws them (the audience is US CEOs).
(function () {
  const bridge = window.RichBridge;
  const HOUR = 3600000, DAY = 24 * HOUR;
  const NEAR = 90; // claude_accounts.rs NEAR_WEEKLY_PERCENT: "Almost used up"
  const ATTENTION = 85; // the row's line turns gold from here with one account (round 18)
  const RECENT = 6 * HOUR; // the row and the banner say a switch for this long (round 18)
  let view = null, lastPoll = 0, busy = false, polling = false;
  let add = null; // { step: name|signin|wait|same|failed, cur, nu, err, id, sameAs, focus }
  let removeId = null, feedback = null, saved = false, savedTimer = null, signTimer = null, signingId = null;
  const esc = s => String(s == null ? "" : s).replace(/[&<>"]/g, c => ({ "&": "&amp;", "<": "&lt;", ">": "&gt;", "\"": "&quot;" }[c]));
  const minute = t => new Date(Math.round(t / 60000) * 60000);
  const clock = t => minute(t).toLocaleTimeString("en-US", { hour: "numeric", minute: "2-digit" });
  const day = t => minute(t).toLocaleDateString("en-US", { weekday: "short" });
  const sameDay = (a, b) => new Date(a).toDateString() === new Date(b).toDateString();
  // Round 18's `when`: "11:01 AM" today, "tomorrow 9:00 AM", "Thu 9:00 AM".
  function when(t) {
    if (sameDay(t, Date.now())) return clock(t);
    if (sameDay(t, Date.now() + DAY)) return "tomorrow " + clock(t);
    return day(t) + " " + clock(t);
  }
  // Recent changes: a clock today, "Yesterday", or the weekday.
  function ago(t) {
    if (sameDay(t, Date.now())) return clock(t);
    if (sameDay(t, Date.now() - DAY)) return "Yesterday";
    return day(t);
  }
  const ICON = {
    plus: '<svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2.2" stroke-linecap="round" aria-hidden="true"><path d="M12 5v14M5 12h14"/></svg>',
    swap: '<svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2.2" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true"><path d="M17 3l4 4-4 4"/><path d="M3 7h18"/><path d="M7 21l-4-4 4-4"/><path d="M21 17H3"/></svg>',
    alert: '<svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2.2" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true"><path d="M12 8v5"/><path d="M12 16.5v.5"/><circle cx="12" cy="12" r="9"/></svg>',
    check: '<svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2.4" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true"><path d="M5 12.5l4.5 4.5L19 7.5"/></svg>',
    globe: '<svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true"><circle cx="12" cy="12" r="9"/><path d="M3 12h18"/><path d="M12 3a14 14 0 0 1 0 18a14 14 0 0 1 0-18"/></svg>',
    down: '<svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2.2" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true"><path d="M12 5v14"/><path d="M6 13l6 6 6-6"/></svg>',
    eye: '<svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true"><path d="M2 12s3.6-7 10-7 10 7 10 7-3.6 7-10 7S2 12 2 12z"/><circle cx="12" cy="12" r="3"/></svg>',
    ring: '<svg class="acc-who-ring" viewBox="0 0 20 20" aria-hidden="true"><circle class="trk" cx="10" cy="10" r="8"/><circle class="arc" cx="10" cy="10" r="8"/></svg>',
  };

  // ---- which account is which: Show email (round 31, item 18) ----------------------------
  // The CEO, 2026-10-18: "The user needs to know which account is which but we don't want to
  // display the emails associated with each account directly on the corresponding screen.
  // Instead, I imagine the user would need to do an extra click there somehow to temporarily
  // reveal which email is connected to which account name." Round 31's item-18 mockup: each card
  // says "Show email"; one click shows "Signed in as <email>" beside a gold ring that empties
  // over 8 seconds, with Hide; then it hides again. Closing the sheet hides every email.
  //
  // The email is read at the click from Claude Code's own `claude auth status` for that
  // account's folder (main.rs `claude_account_email`), held here only while it shows, and never
  // in the page before the click or after it hides. When it cannot be read, the line says so.
  const EMAIL_SECS = 8, WIPE_MS = 320;
  const shown = new Map(); // id -> { answer, at }
  const reading = new Set(), emailTimers = new Map();
  let sheetOpens = 0, wipeIn = null;

  // ---- the accounts, as the app reports them --------------------------------------------
  // With one account `accounts` is empty and the top-level reading is that account's.
  // The account the add flow is signing in is not a card until that sign-in is the right one:
  // the back end keeps it while "That is the account you already use" waits for Try again, and
  // drawn as a card it said "Next" with Use this one now beside a screen saying it was not added
  // (the 2026-10-06 VM walk).
  const settled = () => view ? view.accounts.filter(a => !add || !add.id || a.id !== add.id) : [];
  const multi = () => settled().length > 1;
  function list() {
    if (!view) return [];
    if (multi()) return settled();
    // While a second account is being added, step 1 has already named this one ("Home"), and
    // the screen says "You signed in as Home again"; its card says the same name.
    const named = view.accounts.length > 1 && settled()[0] ? settled()[0].label : "";
    return [{ id: "1", label: view.firstLabel || named, inUse: true, windows: view.windows || [], checkedAt: view.checkedAt, exhaustedUntil: null, message: view.message, signedIn: view.signedIn }];
  }
  const inUse = () => list().find(a => a.inUse) || list()[0] || null;
  const nameOf = a => multi() ? a.label : (a.label || "Your Claude account");
  const win = (a, id) => (a && a.windows || []).find(w => w.id === id) || null;
  const weekOf = a => win(a, "seven_day");
  const resetOf = a => (weekOf(a) && weekOf(a).resetsAt) || Infinity;
  const resting = a => !a.inUse && a.exhaustedUntil > Date.now();
  const read = a => (a.windows || []).length > 0;
  // Next, as claude_accounts.rs next_account chooses it: among the others with a reading and
  // room, the one whose week is fresh again soonest.
  function nextAcct() {
    return list().filter(a => !a.inUse && read(a) && !resting(a)).sort((a, b) => resetOf(a) - resetOf(b))[0] || null;
  }
  // Compared by id: with one account `list()` builds its card's object afresh on every call.
  // While a move is under way, the account being switched to comes right after the one the work
  // still runs on, which stays first.
  function ordered() {
    const nx = nextAcct(), cur = inUse(), to = moving() ? target() : null;
    const first = [cur, ...(to ? [to] : []), ...(nx && !(to && to.id === nx.id) ? [nx] : [])].filter(Boolean);
    const rest = list().filter(a => !first.some(f => f.id === a.id))
      .sort((a, b) => resting(a) - resting(b) || resetOf(a) - resetOf(b));
    return [...first, ...rest];
  }
  const recentSwitch = () => {
    const s = view && view.lastSwitch, cur = inUse();
    return s && cur && multi() && s.to === cur.id && Date.now() - s.at < RECENT ? s : null;
  };
  const labelOf = id => (list().find(a => a.id === id) || {}).label || "";
  // WHO IS REALLY ON EACH ACCOUNT (claude_accounts.rs `working`): "rich" when a reply of his, in
  // any conversation, is running on it, "team" when Stu's session runs on it. The account marked in
  // use is the one the work actually runs on (quota.rs `view_at`, `Accounts::running_on`); while a
  // switch is decided and the work has not moved yet, `view.moving` names the account being switched
  // to. So the account still in use stays first and marked, the new one says "Switching to this
  // one", and nothing shows a switch that has not happened (the CEO, 2026-10-29: "I saw "switching
  // to home" and the Home account was listed first in the list. But apparently it didn't actually
  // switch."; review rv-20261019T005929Z-e3d9ad03-3e7b, finding 3, before it).
  const workingOn = a => (a && a.working) || [];
  const target = () => { const m = view && view.moving; return m && multi() ? list().find(a => a.id === m.to) || null : null; };
  const moving = () => !!target() && !!inUse() && target().id !== inUse().id;
  // The move waits for work still running on the account being left (Renewal::Waits).
  const blocked = () => moving() && !!view.moving.blocked;
  function movingNote(a) {
    const to = target(), here = workingOn(a), away = list().filter(b => b.id !== to.id).flatMap(workingOn);
    if (a.id !== to.id) {
      const name = esc(to.label), said = [];
      if (blocked()) said.push(`Can't move yet: work is still running on ${esc(a.label)}.`);
      if (here.includes("team")) said.push(`The team is finishing the step it is on here, then moves to ${name}.`);
      if (here.includes("rich")) said.push(`Rich moves to ${name} with his next reply.`);
      return said.join(" ");
    }
    const said = [];
    if (here.length) said.push(`${here.length > 1 ? "Rich and the team are" : here[0] === "rich" ? "Rich is" : "The team is"} working on this one now.`);
    if (away.includes("team")) said.push("The team moves here once it finishes the step it is on.");
    if (away.includes("rich")) said.push("Rich moves here with his next reply.");
    return said.join(" ");
  }

  // ---- the Settings row -------------------------------------------------------------------
  // No second line with one account and room (see below), "86% of this week used" (gold, from
  // 85%), "Using Home", or "Using Work, switched 11:01 AM" for six hours after a switch (gold);
  // the in-use account's week as a bar.
  function paintRow() {
    const text = document.getElementById("set-accounts-state"), row = document.getElementById("set-accounts-open");
    if (!text || !row) return;
    const cur = inUse(), week = cur ? weekOf(cur) : null, sw = recentSwitch();
    // WITH ONE ACCOUNT AND ROOM IN ITS WEEK THE ROW HAS NO SECOND LINE, where round 18 draws "One
    // account". Measured: the second line makes the row 50 px instead of 36, and at the app's
    // smallest window (1024 by 700, on the home screen) that put Bust a bug at 707 px, below the
    // window's edge (tests/settings-fit.js; the floor of CEO §15). The line comes back the moment
    // it says something: 85% of the week, or a second account.
    let line = "", time = "", attention = false;
    if (multi()) {
      line = blocked() ? `Can't move yet: work is still running on ${cur.label}` : moving() ? `Switching to ${target().label}`
        : sw ? `Using ${cur.label}, switched ` : `Using ${cur.label}`;
      time = sw && !moving() ? clock(sw.at) : "";
      attention = !!sw || moving();
    } else if (cur && cur.signedIn === false) {
      // Signed out is said on the row too: it is why Rich cannot answer (2026-10-21).
      line = "Not signed in on this Mac";
      attention = true;
    } else if (week && week.usedPercent >= ATTENTION) {
      line = `${Math.floor(week.usedPercent)}% of this week used`;
      attention = true;
    }
    text.textContent = line;
    // The time keeps its AM or PM: "Using Work, switched 2:07" with "PM" alone on the next line
    // in the 2026-10-06 walk.
    if (time) { const at = document.createElement("span"); at.className = "acc-nowrap"; at.textContent = time; text.appendChild(at); }
    text.hidden = !line;
    text.classList.toggle("is-attention", attention);
    // The bar sits beside the name, so the line under it has the row's whole width and stays on
    // one line in the 267 px panel ("86% of this week used" wrapped beside the bar in the
    // 2026-10-06 walk, and every extra line pushes Bust a bug toward the bottom of a small window).
    let mini = row.querySelector(".acc-mini");
    if (!mini) {
      mini = document.createElement("span"); mini.className = "acc-mini"; mini.setAttribute("aria-hidden", "true");
      mini.appendChild(document.createElement("i"));
      (row.querySelector(".set-name") || row).appendChild(mini);
    }
    mini.hidden = !week;
    mini.firstChild.style.width = Math.min(100, week ? week.usedPercent : 0) + "%";
  }

  // ---- the sheet --------------------------------------------------------------------------
  const sheet = document.createElement("div");
  sheet.id = "accounts-sheet"; sheet.className = "overlay"; sheet.hidden = true;
  sheet.setAttribute("role", "dialog");
  sheet.setAttribute("aria-labelledby", "acc-title"); sheet.setAttribute("data-dismiss", "control:#acc-close");
  sheet.innerHTML = `<section class="overlay-panel acc-panel">
    <header class="acc-heading"><div><p class="acc-eyebrow">Settings</p><h2 id="acc-title">Claude accounts</h2><p id="acc-lede" class="acc-lede"></p></div>
      <button id="acc-close" type="button" aria-label="Close Claude accounts"><svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2.2" stroke-linecap="round" aria-hidden="true"><path d="M6 6l12 12M18 6L6 18"/></svg></button></header>
    <div class="acc-body"><div id="acc-col" class="acc-col"></div><div id="acc-how" class="acc-how"></div></div>
    <p id="acc-who-status" class="sr-only" role="status"></p></section>`;
  document.body.appendChild(sheet);
  const $ = id => document.getElementById(id);
  const col = $("acc-col"), how = $("acc-how");

  function meter(a, k) {
    const w = win(a, k === "week" ? "seven_day" : "five_hour"), label = k === "week" ? "Weekly limit" : "5-hour limit";
    if (!w) {
      return `<div class="acc-meter" data-k="${k}"><span class="acc-meter-label">${label}</span><span class="acc-meter-bar" role="img" aria-label="${label}: not reported yet"><b style="width:0"></b></span>
        <span class="acc-meter-num"></span><span class="acc-meter-when">Not reported yet</span></div>`;
    }
    const pct = Math.floor(w.usedPercent), t = w.resetsAt;
    const near = k === "week" && pct >= NEAR && pct < 99;
    let line = t ? `Fresh again <b>${esc(when(t))}</b>` : "";
    if (k === "five" && pct === 0) line = "Not used in the last 5 hours";
    else if (k === "week" && pct >= 99) line = `<b>Used up for this week.</b>${t ? ` Fresh again ${esc(when(t))}` : ""}`;
    else if (near) line = `Almost used up.${t ? ` Fresh again ${esc(when(t))}` : ""}`;
    return `<div class="acc-meter" data-k="${k}"><span class="acc-meter-label">${label}</span>
      <span class="acc-meter-bar" role="img" aria-label="${label}: ${pct}% used"><b style="width:${Math.min(100, pct)}%"></b></span>
      <span class="acc-meter-num">${pct}<small>%</small><em>used</em></span>
      <span class="acc-meter-when${near ? " is-near" : ""}">${line}</span></div>`;
  }
  function card(a) {
    const nx = nextAcct(), n = view.agentsWorking || 0;
    // While a move is under way the account marked in use is the one being left, still really in
    // use (the gold card), and the account being switched to is drawn as Next is.
    const leaving = moving() && a.inUse;
    const arriving = moving() && a.id === target().id;
    const kind = leaving ? "leaving" : arriving ? "arriving" : a.inUse ? "inuse" : !read(a) ? "unread" : resting(a) ? "resting" : nx && nx.id === a.id ? "next" : "ready";
    const team = n ? `: <b>${n} ${n === 1 ? "agent" : "agents"}</b>` : "";
    let pill = "", note;
    // The account being left is still really in use (the gold card); the one being moved to is
    // drawn as Next is, with its own words.
    if (kind === "leaving") { pill = `<span class="acc-pill is-inuse"><i></i>Still in use</span>`; note = movingNote(a); }
    else if (kind === "arriving") { pill = `<span class="acc-pill is-next"><i></i>Switching to this one</span>`; note = movingNote(a); }
    // THE ACCOUNT IN USE, SIGNED OUT, SAYS SO (a new user's first run, 2026-10-21: "In use now"
    // over "No reading yet. If this account is not signed in on this Mac, sign in here." on a Mac
    // where nobody had signed in). `signedIn` is Claude Code's own `auth status` (quota.rs
    // `View::signed_in`); with no verdict yet the card says the check is still under way, and
    // when the check answered something else it says what (`message`), never "No reading yet".
    else if (!read(a) && a.signedIn == null) {
      pill = `<span class="acc-pill is-resting"><i></i>Sign-in not confirmed</span>`;
      note = a.message ? `${esc(a.message)} RichOS has not confirmed whether this account is signed in.`
        : "Checking whether this account is signed in on this Mac.";
    }
    else if (kind === "inuse" && a.signedIn === false) {
      pill = `<span class="acc-pill is-resting"><i></i>Not signed in</span>`;
      note = signingId === a.id ? "Finish signing in in your browser, then come back here."
        : "This account is not signed in on this Mac, so Rich cannot answer you yet. Sign in here.";
    } else if (kind === "inuse") {
      pill = `<span class="acc-pill is-inuse"><i></i>In use now</span>`;
      note = !read(a) ? esc(a.message || (a.signedIn === true ? "This account is signed in on this Mac." : "Checking whether this account is signed in on this Mac."))
        : multi() ? `Rich and the team are working on this one${team}.` : `Rich and the team work on this account${n ? `${team} right now` : ""}.`;
    } else if (kind === "next") { pill = `<span class="acc-pill is-next"><i></i>Next</span>`; note = "Its week is fresh again soonest, so it goes next."; }
    else if (kind === "resting") { pill = `<span class="acc-pill is-resting"><i></i>Resting</span>`; note = "Rich can use it again once its week is fresh."; }
    else if (kind === "ready") { pill = `<span class="acc-pill"><i></i>Ready</span>`; note = "Waiting its turn."; }
    // The same signed-in verdict on a card that is not in use (handover review
    // rv-20261021T234558Z-10df0b1f-03b0, finding 2): no reading, but Claude Code says it is
    // signed in, so it says so and what the last check said, never "Not signed in".
    // Auth status alone establishes sign-in, not whether subscription limits exist.
    else if (a.signedIn === true) { pill = `<span class="acc-pill"><i></i>Signed in</span>`; note = esc(a.message || "This account is signed in on this Mac."); }
    else { pill = `<span class="acc-pill is-resting"><i></i>Not signed in</span>`; note = signingId === a.id ? "Finish signing in in your browser, then come back here." : "Sign in to this account and Rich can use it."; }
    let acts = "";
    if (multi() && kind !== "inuse" && kind !== "arriving" && kind !== "unread" && kind !== "resting") acts += `<button class="acc-btn" type="button" data-act="use" data-id="${esc(a.id)}" aria-label="Use ${esc(a.label)} now">${ICON.swap}Use this one now</button>`;
    // A signed-in verdict stands on its own: an account Claude Code says is signed in is never
    // offered "Sign in" because no usage reading came with it (review rv-20261021T213648Z).
    if (!read(a) && a.id === "1" && a.signedIn !== true) acts += `<button class="acc-btn" type="button" id="acc-sign-one" data-act="sign-one">Sign in</button>`;
    else if (((kind === "unread" && a.signedIn !== true) || a.signedIn === false) && signingId !== a.id) acts += `<button class="acc-btn" type="button" data-act="sign" data-id="${esc(a.id)}">Sign in</button>`;
    // Account 1 is this Mac's own Claude sign-in and is never removed here (claude_accounts.rs).
    if (multi() && a.id !== "1") acts += `<button class="acc-btn acc-btn-quiet" type="button" data-act="remove" data-id="${esc(a.id)}" aria-label="Remove ${esc(a.label)}">Remove</button>`;
    let confirm = "";
    if (removeId === a.id) {
      confirm = `<div class="acc-confirm"><span>Remove <b>${esc(a.label)}</b> from RichOS? RichOS signs out of it here. Nothing changes on the account itself and you can add it back any time.</span>
        <button class="acc-btn acc-btn-primary" type="button" id="acc-remove-yes" data-act="remove-yes" data-id="${esc(a.id)}">Remove ${esc(a.label)}</button><button class="acc-btn" type="button" id="acc-remove-no" data-act="remove-no">Keep it</button></div>`;
    }
    const look = kind === "leaving" ? "inuse" : kind === "arriving" ? "next" : kind;
    return `<article class="acc-card is-${look}" data-id="${esc(a.id)}" aria-label="${esc(nameOf(a))}"><span class="acc-spine"></span>
      <header class="acc-head"><h3 class="acc-name">${esc(nameOf(a))}</h3>${pill}<span class="acc-acts">${acts}</span></header>
      ${who(a)}<p class="acc-note">${note}</p>${a.signedIn === false || !read(a) ? "" : `<div class="acc-meters">${meter(a, "week")}${meter(a, "five")}</div>`}${confirm}</article>`;
  }
  // The card's one line under its name: "Show email" at rest, the email while it shows.
  function who(a) {
    const id = esc(a.id), name = esc(nameOf(a)), s = shown.get(a.id);
    if (!s) {
      return `<div class="acc-who" data-id="${id}"><button class="acc-reveal" type="button" id="acc-email-${id}" data-act="email-show" data-id="${id}" aria-expanded="false" aria-label="Show the email ${name} is signed in with"${reading.has(a.id) ? ` aria-busy="true"` : ""}>${ICON.eye}<span>Show email</span></button></div>`;
    }
    const said = s.answer.state === "email"
      ? `<span class="acc-who-label">Signed in as</span><span class="acc-who-addr">${esc(s.answer.email)}</span>`
      : `<span class="acc-who-label">${s.answer.state === "signedOut" ? "This account is not signed in on this Mac." : "Claude Code did not say which email this account uses."}</span>`;
    return `<div class="acc-who is-counting" data-id="${id}" data-at="${s.at}" style="--acc-who-secs:${EMAIL_SECS}s"><span class="acc-who-mail">${ICON.ring}${said}<button class="acc-who-hide" type="button" id="acc-email-hide-${id}" data-act="email-hide" data-id="${id}" aria-label="Hide the email for ${name}">Hide</button></span></div>`;
  }
  // After a repaint, each ring is set to the time that has really passed, so a repaint while
  // an email shows never starts its 8 seconds again; the one just shown wipes in.
  function paintWho() {
    for (const line of col.querySelectorAll(".acc-who[data-at]")) {
      const arc = line.querySelector(".arc");
      if (arc) arc.style.animationDelay = -(Date.now() - Number(line.dataset.at)) + "ms";
      if (line.dataset.id === wipeIn) {
        line.classList.add("is-wiping");
        requestAnimationFrame(() => requestAnimationFrame(() => line.classList.remove("is-wiping")));
      }
    }
    wipeIn = null;
  }
  const say = text => { const s = $("acc-who-status"); if (s) s.textContent = text; };
  async function showEmail(id) {
    if (reading.has(id) || shown.has(id)) return;
    const opens = sheetOpens, a = list().find(x => x.id === id);
    reading.add(id); render();
    let answer = null;
    try { answer = await bridge.invoke("claude_account_email", { id }); } catch (_) { /* said below as not read */ }
    reading.delete(id);
    // Closed (or closed and opened again) while it was being read: nothing is shown.
    if (sheet.hidden || opens !== sheetOpens) { render(); return; }
    if (!answer || !["email", "signedOut", "unknown"].includes(answer.state) || (answer.state === "email" && !answer.email)) answer = { state: "unknown" };
    // This is the provider's auth answer for this card. The backend keeps the same boolean;
    // reflect it immediately, rather than contradicting the address until the next quota poll.
    const account = multi() ? list().find(x => x.id === id) : id === "1" ? view : null;
    if (account && answer.state !== "unknown") {
      account.signedIn = answer.state === "email";
      if (!account.signedIn) {
        account.windows = []; account.checkedAt = null;
        account.message = "This Claude account is not signed in on this Mac.";
      } else if (account.message && /not signed in/i.test(account.message)) account.message = null;
    }
    shown.set(id, { answer, at: Date.now() });
    wipeIn = id;
    render();
    const hide = $("acc-email-hide-" + id);
    if (hide) hide.focus({ preventScroll: true });
    const name = a ? nameOf(a) : "This account";
    say(answer.state === "email" ? `${name} is signed in as ${answer.email}. It hides again in ${EMAIL_SECS} seconds.`
      : answer.state === "signedOut" ? `${name} is not signed in on this Mac.` : `Claude Code did not say which email ${name} uses.`);
    clearTimeout(emailTimers.get(id));
    emailTimers.set(id, setTimeout(() => hideEmail(id, false), EMAIL_SECS * 1000));
  }
  function hideEmail(id, now) {
    clearTimeout(emailTimers.get(id)); emailTimers.delete(id);
    if (!shown.has(id)) return;
    const line = col.querySelector(`.acc-who[data-id="${CSS.escape(id)}"]`);
    const hadFocus = !!line && line.contains(document.activeElement);
    const done = () => {
      if (!shown.has(id)) return;
      shown.delete(id);
      render();
      if (hadFocus) { const show = $("acc-email-" + id); if (show) show.focus({ preventScroll: true }); }
      say("Email hidden.");
    };
    if (now || !line) return done();
    line.classList.add("is-wiping");
    setTimeout(done, WIPE_MS);
  }
  // Closing the sheet hides every email, and it always opens with all of them hidden.
  function hideAllEmails() {
    for (const t of emailTimers.values()) clearTimeout(t);
    const had = shown.size > 0 || reading.size > 0;
    emailTimers.clear(); shown.clear(); reading.clear(); wipeIn = null;
    sheetOpens++;
    say("");
    // Out of the page at once, not left in a hidden sheet until it next opens.
    if (had && view) renderCol();
  }
  function why(s) {
    const from = labelOf(s.from), fromAcct = list().find(a => a.id === s.from);
    const w = fromAcct && win(fromAcct, s.why === "fiveHour" ? "five_hour" : "seven_day");
    const back = fromAcct && fromAcct.exhaustedUntil > Date.now() ? fromAcct.exhaustedUntil : w && w.resetsAt;
    const fresh = back ? ` It is fresh again ${esc(when(back))}.` : "";
    if (s.why === "limit") return `Why: ${esc(from)} reached a Claude usage limit, so the step it was on ran again on ${esc(labelOf(s.to))}.${fresh}`;
    const what = s.why === "fiveHour" ? "5-hour limit" : "weekly limit";
    return `Why: ${esc(from)} had used ${Math.floor(s.used || 0)}% of its ${what}.${fresh} Nothing stopped and the team carried on.`;
  }
  const bannerKey = "richos-accounts-banner-seen";
  function banner() {
    const sw = recentSwitch();
    if (sw && localStorage.getItem(bannerKey) !== String(sw.at)) {
      return `<div class="acc-banner" role="status"><span class="acc-banner-icon">${ICON.swap}</span><div class="acc-banner-main">
        <p class="acc-banner-head">Rich switched to ${esc(labelOf(sw.to))} at ${esc(clock(sw.at))}</p>
        <p class="acc-banner-body">${why(sw)}</p>
        <div class="acc-banner-acts"><button class="acc-btn" type="button" data-act="banner-ok" data-at="${sw.at}">Got it</button></div></div></div>`;
    }
    const all = list();
    if (multi() && all.every(a => weekOf(a) && weekOf(a).usedPercent >= NEAR)) {
      const order = ordered(), back = all.filter(a => weekOf(a).resetsAt).sort((a, b) => resetOf(a) - resetOf(b))[0];
      const figs = order.map((a, i) => i ? `${esc(a.label)} ${Math.floor(weekOf(a).usedPercent)}%` : `${esc(a.label)} has used ${Math.floor(weekOf(a).usedPercent)}%`);
      const said = figs.length > 1 ? figs.slice(0, -1).join(", ") + " and " + figs[figs.length - 1] : figs[0];
      const head = all.length === 2 ? "Both accounts are close to their weekly limit" : `All ${all.length} accounts are close to their weekly limit`;
      const then = back ? ` When ${all.length === 2 ? "both" : "they all"} run out, the team pauses until ${esc(back.label)} is fresh again ${esc(when(back.weekAt || resetOf(back)))}. Nothing is lost.` : "";
      return `<div class="acc-banner" role="status"><span class="acc-banner-icon">${ICON.alert}</span><div class="acc-banner-main">
        <p class="acc-banner-head">${head}</p><p class="acc-banner-body">${said}.${then}</p>
        <div class="acc-banner-acts"><button class="acc-btn acc-btn-primary" type="button" data-act="add-open">${ICON.plus}Add another account</button></div></div></div>`;
    }
    return "";
  }
  function addHtml() {
    const A = add, first = !multi();
    const nu = A.nu.trim() || "the new account";
    const steps = n => `<div class="acc-steps" aria-hidden="true"><i class="is-done"></i><i class="${n > 1 ? "is-done" : ""}"></i></div>`;
    const top = (title, n) => `<div class="acc-add-top"><h3 class="acc-add-title">${title}</h3><span class="acc-add-step">Step ${n} of 2</span></div>${steps(n)}`;
    if (A.step === "name") {
      return `<div class="acc-add" id="acc-add">${top(first ? "Add a second Claude account" : "Add another Claude account", 1)}
        <p class="acc-add-body">Give ${first ? "each account" : "it"} a name only you see, so you can tell them apart.</p>
        <div class="acc-fields${first ? "" : " is-one"}">
          ${first ? `<label class="acc-field">The account you use now<input id="acc-cur" type="text" maxlength="24" placeholder="For example: Home" value="${esc(A.cur)}" autocomplete="off"></label>` : ""}
          <label class="acc-field${A.err ? " is-invalid" : ""}">The new account<input id="acc-nu" type="text" maxlength="24" placeholder="For example: Work" value="${esc(A.nu)}" autocomplete="off"></label></div>
        ${A.err ? `<p class="acc-err" role="alert">${esc(A.err)}</p>` : ""}
        <div class="acc-add-acts"><button class="acc-btn acc-btn-primary" type="button" id="acc-continue" data-act="add-next">Continue</button><button class="acc-btn" type="button" data-act="add-cancel">Cancel</button></div></div>`;
    }
    if (A.step === "signin" || A.step === "failed") {
      return `<div class="acc-add" id="acc-add">${top(`Sign in to ${esc(nu)}`, 2)}
        <p class="acc-add-body">Your browser opens Claude's own sign-in page. Sign in with your <b>${esc(nu)}</b> account, not the one you already use, then come back here.</p>
        <p class="acc-add-body is-soft">RichOS never sees your password.</p>
        ${A.step === "failed" ? `<p class="acc-err" role="alert">The sign-in did not finish. Press Open sign-in to try again.</p>` : ""}
        <div class="acc-add-acts"><button class="acc-btn acc-btn-primary" type="button" id="acc-open-signin" data-act="add-go">${ICON.globe}Open sign-in</button><button class="acc-btn" type="button" data-act="${A.id ? "add-cancel" : "add-back"}">${A.id ? "Cancel" : "Back"}</button></div></div>`;
    }
    if (A.step === "wait") {
      return `<div class="acc-add" id="acc-add">${top(`Sign in to ${esc(nu)}`, 2)}
        <p class="acc-add-body">Sign in with your <b>${esc(nu)}</b> account in the browser window that just opened, then come back here.</p>
        <div class="acc-waiting" role="status"><span class="acc-ring" aria-hidden="true"></span><p>Waiting for you to finish signing in…</p></div>
        <div class="acc-add-acts"><button class="acc-btn" type="button" data-act="add-again">${ICON.globe}Open the sign-in page again</button><button class="acc-btn" type="button" data-act="add-cancel">Cancel</button></div></div>`;
    }
    if (A.step === "same") {
      return `<div class="acc-add" id="acc-add">${top("That is the account you already use", 2)}
        <p class="acc-add-body">You signed in as <b>${esc(A.sameAs || "your first account")}</b> again. To add ${esc(nu)}, sign in with your other Claude account.</p>
        <p class="acc-add-body is-soft">In your browser, sign out of Claude first, then press Try again.</p>
        <div class="acc-add-acts"><button class="acc-btn acc-btn-primary" type="button" id="acc-try-again" data-act="add-again">${ICON.globe}Try again</button><button class="acc-btn" type="button" data-act="add-cancel">Cancel</button></div></div>`;
    }
    return "";
  }
  function renderCol() {
    const focus = document.activeElement && col.contains(document.activeElement) ? (document.activeElement.id || document.activeElement.dataset.act + (document.activeElement.dataset.id || "")) : null;
    let html = banner();
    const cards = ordered(), nx = nextAcct();
    html += `<div class="acc-cards">${cards.map((a, i) => card(a) + (i === 0 && !moving() && nx && cards[1] && cards[1].id === nx.id
      ? `<div class="acc-handover" aria-hidden="true"><span>${ICON.down}Then ${esc(nx.label)}, when ${esc(a.label)} is nearly full</span></div>` : "")).join("")}</div>`;
    if (add) html += addHtml();
    else if (!multi()) html += `<button class="acc-slot" type="button" data-act="add-open"><span class="acc-slot-plus">${ICON.plus}</span><span><span class="acc-slot-title">Add another Claude account</span><span class="acc-slot-body">Have a second Claude account, for example one for work? Add it here and Rich switches to it when this one is nearly full.</span></span></button>`;
    else html += `<div class="acc-more"><button class="acc-btn acc-btn-quiet" type="button" data-act="add-open">${ICON.plus}Add another account</button></div>`;
    if (feedback) html += `<p class="acc-feedback" role="status">${ICON.check}<span>${feedback.html}</span>${feedback.undo ? `<button class="acc-btn acc-btn-quiet" type="button" data-act="undo" data-id="${esc(feedback.undo)}">Undo</button>` : ""}</p>`;
    // The DOM is only replaced when what it says changed, so a press, a hover or a focus is never
    // lost to a repaint that would draw the same thing.
    if (html === col.dataset.painted && !(add && add.focus)) return;
    col.dataset.painted = html;
    col.innerHTML = html;
    paintWho();
    if (add && add.focus) {
      const input = $(multi() || add.cur ? "acc-nu" : "acc-cur") || $("acc-open-signin") || $("acc-try-again");
      if (input) { input.focus(); if (input.setSelectionRange) input.setSelectionRange(input.value.length, input.value.length); }
      add.focus = false;
    } else if (focus) {
      const back = $(focus) || col.querySelector(`[data-act="${focus.replace(/[^a-z-].*$/, "")}"]`);
      if (back) back.focus();
    }
  }
  function changeText(c) {
    const name = `<b>${esc(c.label)}</b>`;
    if (c.kind === "added") return `You added ${name}.`;
    if (c.kind === "removed") return `You removed ${name}.`;
    if (c.kind === "chose") return `You chose ${name}.`;
    if (c.why === "limit") return `Rich switched to ${name}. ${esc(c.from)} reached a Claude usage limit.`;
    return `Rich switched to ${name}. ${esc(c.from)} reached ${Math.floor(c.used || 0)}% of its ${c.why === "fiveHour" ? "5-hour limit" : "week"}.`;
  }
  function renderHow() {
    if (!multi()) {
      paintHow(`<h3 class="acc-how-title">When your account fills up</h3>
        <p class="acc-how-p">When it is nearly used up, Rich <b>pauses the team</b> until it is fresh again. Nothing is lost: everyone picks up right where they left off.</p>
        <p class="acc-how-p is-soft">Your own conversation with Rich never pauses.</p>
        <div class="acc-promo"><p class="acc-how-h">Keep the team going with a second account</p>
          <p class="acc-how-p">If you have more than one Claude account, add them all. Rich uses one at a time and switches to the next when it is nearly full, so the team does not have to wait.</p>
          ${add ? "" : `<div><button class="acc-btn acc-btn-primary" type="button" data-act="add-open">${ICON.plus}Add another account</button></div>`}</div>`);
      return;
    }
    const nx = nextAcct(), on = !!view.policy.enabled, choice = on ? view.atThreshold : null;
    const rows = (view.changes || []).slice().sort((a, b) => b.at - a.at).slice(0, 4);
    paintHow(`<h3 class="acc-how-title">How Rich uses your accounts</h3>
      <p class="acc-how-p">Rich uses one account at a time. When its weekly limit is almost used up, he <b>switches to the next one by himself</b>. Nothing stops and nothing is lost.</p>
      <p class="acc-how-p is-soft">Next is always the account whose week is fresh again soonest, so none of your weekly limit goes to waste. To use a different account first, press Use this one now on it.</p>
      <div class="acc-how-sec"><p class="acc-how-h" id="acc-five-h">When a 5-hour limit is almost used up</p>
        <div class="acc-choice" role="radiogroup" aria-labelledby="acc-five-h">
          <button class="acc-opt" type="button" role="radio" id="acc-five-pause" data-act="five" data-v="pause" aria-checked="${choice === "pause"}"><span class="acc-radio"></span><span><span class="acc-opt-t">Pause the team until it is fresh again</span><span class="acc-opt-s">The default. Never longer than 5 hours.</span></span></button>
          <button class="acc-opt" type="button" role="radio" id="acc-five-switch" data-act="five" data-v="switch" aria-checked="${choice === "switch"}"><span class="acc-radio"></span><span><span class="acc-opt-t">Switch to ${nx ? esc(nx.label) : "the next account"}</span><span class="acc-opt-s">The team keeps going and uses up the next account's week sooner.</span></span></button>
        </div>${saved ? `<p class="acc-saved" role="status">Saved.</p>` : ""}</div>
      <div class="acc-how-sec"><p class="acc-how-h">Recent changes</p>
        ${rows.length ? `<ul class="acc-log">${rows.map(c => `<li><time>${esc(ago(c.at))}</time><span>${changeText(c)}</span></li>`).join("")}</ul>`
          : `<p class="acc-how-p is-soft">Nothing has changed yet.</p>`}</div>`);
  }
  function paintHow(html) {
    if (html === how.dataset.painted) return;
    const focus = document.activeElement && how.contains(document.activeElement) ? document.activeElement.id : null;
    how.dataset.painted = html;
    how.innerHTML = html;
    if (focus && $(focus)) $(focus).focus();
  }
  function render() {
    paintRow();
    paintNotes();
    if (sheet.hidden || !view) return;
    $("acc-lede").textContent = multi()
      ? "Rich and the team work on one Claude account at a time. When it is nearly full, Rich switches to the next one by himself, so the work keeps going."
      : "Rich and the team do their work on your Claude account. Claude limits how much each account can do in 5 hours and in a week.";
    // A form being typed in is never repainted under the cursor.
    if (!(add && add.step === "name" && col.contains(document.activeElement) && document.activeElement.tagName === "INPUT")) renderCol();
    renderHow();
  }

  // ---- the data ---------------------------------------------------------------------------
  // A read asked for while one is in flight is made right after it, so a just-added account's
  // card never waits for the next poll.
  let reloadAfter = false;
  async function load(force) {
    if (busy) { reloadAfter = true; return; }
    busy = true;
    try { view = await bridge.invoke("claude_quota", { refresh: !!force }); }
    catch (_) { /* the last reading stays; nothing here is guessed */ }
    finally {
      busy = false; lastPoll = Date.now(); render();
      if (reloadAfter) { reloadAfter = false; load(false); }
    }
  }
  async function call(command, args) {
    try {
      const next = await bridge.invoke(command, args || {});
      if (next && next.windows) view = next;
      return { ok: true, value: next };
    } catch (error) {
      feedback = { html: esc(typeof error === "string" ? error : "That change could not be made. Nothing changed.") };
      render();
      return { ok: false };
    }
  }

  // ---- adding a second account: two steps -------------------------------------------------
  function openAdd() { removeId = null; feedback = null; add = { step: "name", cur: "", nu: "", err: null, id: null, focus: true }; render(); }
  function readFields() { const c = $("acc-cur"), n = $("acc-nu"); if (c) add.cur = c.value; if (n) add.nu = n.value; }
  function next() {
    readFields();
    const nu = add.nu.trim(), cur = add.cur.trim() || "Home";
    const taken = multi() ? list().some(a => a.label.toLowerCase() === nu.toLowerCase()) : nu.toLowerCase() === cur.toLowerCase();
    if (!nu) add.err = "Give the new account a name, for example Work.";
    else if (taken) add.err = "That name is taken. Pick a different one.";
    else { add.err = null; add.step = "signin"; }
    add.focus = true; render();
  }
  async function go() {
    const first = !multi();
    const result = await call("claude_account_add", { label: add.nu.trim(), currentLabel: first ? (add.cur.trim() || "Home") : null });
    if (!result.ok || !add) return;
    const added = (view.accounts || []).reduce((top, a) => !top || Number(a.id) > Number(top.id) ? a : top, null);
    add.id = added ? added.id : null; add.step = "wait"; render(); watch();
  }
  async function again() {
    if (!add || !add.id) return go();
    const result = await call("claude_account_sign_in", { id: add.id });
    if (result.ok && add) { add.step = "wait"; add.focus = true; render(); watch(); }
  }
  async function discard() {
    clearInterval(signTimer); signTimer = null;
    const id = add && add.id;
    add = null; render();
    if (id) await call("claude_account_discard", { id });
    render();
  }
  function watch() {
    clearInterval(signTimer);
    signTimer = setInterval(async () => {
      let answer = null;
      try { answer = await bridge.invoke("claude_account_sign_in_poll", {}); } catch (_) {}
      const state = answer && answer[1] && answer[1].state;
      if (state === "connecting") return;
      clearInterval(signTimer); signTimer = null;
      const id = (answer && answer[0]) || (add && add.id) || signingId;
      if (add && add.id === id) {
        if (state === "connected") {
          const name = add.nu.trim();
          add = null;
          await load(true);
          const cur = inUse();
          feedback = { html: `<b>${esc(name)}</b> is ready. Rich switches to it when ${esc(cur ? cur.label : "the one in use")} is nearly full.` };
        } else if (state === "same-account") { add.step = "same"; add.sameAs = answer[2]; add.focus = true; }
        else { add.step = "failed"; }
      } else if (signingId) {
        if (state === "connected") feedback = { html: `<b>${esc(labelOf(signingId))}</b> is signed in.` };
        else if (state === "same-account") feedback = { html: `That is the account you already use as <b>${esc(answer[2] || "")}</b>. Sign in with your other Claude account.` };
        signingId = null;
        await load(true);
      }
      render();
    }, 2000);
  }

  // ---- the sheet's actions ----------------------------------------------------------------
  // WHAT "USE THIS ONE NOW" SAYS FOLLOWS THE MOVE ITSELF (review rv-20261029T150133Z-2f57a3fd-6d74,
  // finding 2; item 3 of the CEO's 2026-10-29: "It never shows a switch that didn't happen"). The
  // command decides the move; the work moves once what runs on the account being left lets it
  // (`view.moving`). So the line is read from the view each time it is drawn: "Switching to Work."
  // while the move is under way, "Can't move yet: work is still running on Home." while it is
  // blocked, and "Rich now uses Work." only once Work is the account the work runs on (`inUse`).
  // Before, "Rich now uses Work." came the moment the command answered, even when its own answer
  // said the move was blocked.
  function moveSaid(id) {
    const to = list().find(a => a.id === id), cur = inUse();
    if (!to || !cur) return "";
    if (cur.id !== to.id) {
      if (moving() && target().id === to.id) return blocked()
        ? `Can't move yet: work is still running on <b>${esc(cur.label)}</b>.`
        : `Switching to <b>${esc(to.label)}</b>.`;
      return `Rich still uses <b>${esc(cur.label)}</b>.`;
    }
    const week = weekOf(to), nx = nextAcct();
    const then = week && week.usedPercent >= NEAR
      ? ` ${esc(to.label)} has already used ${Math.floor(week.usedPercent)}% of its week, so Rich will switch again when it reaches 99%.`
      : nx ? ` ${esc(nx.label)} is next.` : "";
    return `Rich now uses <b>${esc(to.label)}</b>.${then}`;
  }
  async function useNow(id, undoing) {
    const from = inUse(), to = list().find(a => a.id === id);
    if (!to) return;
    const result = await call("claude_account_use_first", { id });
    if (!result.ok) return;
    if (undoing) { feedback = null; render(); return; }
    feedback = { get html() { return moveSaid(id); }, undo: from ? from.id : null };
    render();
  }
  async function setFive(value) {
    if (!view.policy.enabled) {
      // The choice means what it says only while the automatic pause or switch is on.
      const on = await call("set_claude_quota_policy", { policy: { ...view.policy, enabled: true } });
      if (!on.ok) return;
    }
    const result = await call("set_claude_at_threshold", { value });
    if (!result.ok) return;
    saved = true; clearTimeout(savedTimer);
    savedTimer = setTimeout(() => { saved = false; render(); }, 1800);
    render();
    const option = $(value === "pause" ? "acc-five-pause" : "acc-five-switch"); if (option) option.focus();
  }
  sheet.addEventListener("click", async event => {
    if (event.target === sheet) { close(); return; }
    const b = event.target.closest("[data-act]");
    if (!b || b.disabled) return;
    const id = b.dataset.id;
    switch (b.dataset.act) {
      case "add-open": openAdd(); break;
      case "add-next": next(); break;
      case "add-back": add.step = "name"; add.focus = true; render(); break;
      case "add-cancel": discard(); break;
      case "add-go": go(); break;
      case "add-again": again(); break;
      case "use": useNow(id, false); break;
      case "undo": useNow(id, true); break;
      case "remove": removeId = removeId === id ? null : id; feedback = null; render(); if (removeId) $("acc-remove-yes").focus(); break;
      case "remove-no": { const was = removeId; removeId = null; render(); const back = col.querySelector(`[data-act="remove"][data-id="${was}"]`); if (back) back.focus(); break; }
      case "remove-yes": {
        const gone = list().find(a => a.id === id);
        removeId = null;
        const result = await call("claude_account_remove", { id });
        if (result.ok) feedback = { html: `${esc(gone ? gone.label : "The account")} is removed from RichOS. You can add it back any time.` };
        render(); break;
      }
      case "sign": signingId = id; render(); if ((await call("claude_account_sign_in", { id })).ok) watch(); else { signingId = null; render(); } break;
      case "sign-one": close(); if (window.RichSettings && window.RichSettings.openAccount) window.RichSettings.openAccount(); break;
      case "email-show": showEmail(id); break;
      case "email-hide": hideEmail(id, false); break;
      case "five": if (b.getAttribute("aria-checked") !== "true") setFive(b.dataset.v); break;
      case "banner-ok": localStorage.setItem(bannerKey, b.dataset.at); render(); break;
    }
  });
  sheet.addEventListener("input", event => { if (add && (event.target.id === "acc-cur" || event.target.id === "acc-nu")) readFields(); });
  sheet.addEventListener("keydown", event => {
    if (event.target.matches && event.target.matches("#acc-cur, #acc-nu") && event.key === "Enter") { event.preventDefault(); next(); return; }
    if (event.key === "Escape") {
      event.stopPropagation(); event.preventDefault();
      if (removeId) { const was = removeId; removeId = null; render(); const back = col.querySelector(`[data-act="remove"][data-id="${was}"]`); if (back) back.focus(); }
      else if (add) discard();
      else close();
      return;
    }
    if (event.key === "Tab") {
      const controls = [...sheet.querySelectorAll("button, input")].filter(n => !n.disabled && n.getClientRects().length);
      if (event.shiftKey && document.activeElement === controls[0]) { event.preventDefault(); controls[controls.length - 1].focus(); }
      else if (!event.shiftKey && document.activeElement === controls[controls.length - 1]) { event.preventDefault(); controls[0].focus(); }
    }
  });
  $("acc-close").addEventListener("click", () => close());

  function open(options) {
    if (window.RichSettings && window.RichSettings.close) window.RichSettings.close();
    sheet.hidden = false; feedback = null; removeId = null;
    hideAllEmails();
    if (options && options.add) add = { step: "name", cur: "", nu: "", err: null, id: null, focus: true };
    render();
    if (!add) $("acc-close").focus();
    load(false).then(() => { if (options && options.add && add) { add.focus = true; render(); } });
  }
  function close() {
    if (add && add.id) discard();
    add = null; removeId = null; feedback = null;
    hideAllEmails();
    sheet.hidden = true;
    queueMicrotask(() => { const btn = document.getElementById("set-btn"); if (btn) btn.focus(); });
  }

  // ---- Rich's lines in the conversation ---------------------------------------------------
  // The back end records which of Rich's lines are about accounts (claude_accounts.rs `noted`);
  // the conversation asks for a slot under each and this file fills it, now and on every update.
  function noteKind(turnId) {
    const n = view && (view.notes || []).find(x => x.turn === turnId);
    return n ? n.kind : null;
  }
  function fillNote(slot) {
    const kind = noteKind(slot.dataset.accountsNote);
    const key = kind + "|" + (multi() ? list().map(a => a.label).join(",") : "") + "|" + ((view && view.nudge && view.nudge.answer) || "");
    if (slot.dataset.painted === key) return;
    slot.dataset.painted = key;
    if (kind === "nudge") {
      if (multi()) slot.innerHTML = `<p class="acc-after">${esc(list()[1].label)} is added. I will switch to it when ${esc(list()[0].label)} is nearly full.</p>`;
      else if (view.nudge && view.nudge.answer === "notNow") slot.innerHTML = `<p class="acc-after">Okay. You can add one any time in Settings, under Claude accounts.</p>`;
      else slot.innerHTML = `<div class="acc-note-acts"><button class="acc-btn acc-btn-primary" type="button" data-note-act="add">${ICON.plus}Add a second account</button><button class="acc-btn" type="button" data-note-act="not-now">Not now</button></div>`;
    } else if (kind === "switched") {
      slot.innerHTML = `<div class="acc-note-acts"><button class="acc-btn acc-btn-quiet" type="button" data-note-act="see">See your accounts</button></div>`;
    } else if (kind === "bothNear") {
      slot.innerHTML = `<div class="acc-note-acts"><button class="acc-btn acc-btn-primary" type="button" data-note-act="add">${ICON.plus}Add another account</button><button class="acc-btn acc-btn-quiet" type="button" data-note-act="see">See your accounts</button></div>`;
    } else slot.innerHTML = "";
  }
  function paintNotes() { for (const slot of document.querySelectorAll("[data-accounts-note]")) fillNote(slot); }
  function noteSlot(turnId) {
    if (!turnId) return null;
    const slot = document.createElement("div");
    slot.className = "acc-note-slot"; slot.dataset.accountsNote = turnId;
    fillNote(slot);
    return slot;
  }
  document.addEventListener("click", async event => {
    const b = event.target.closest && event.target.closest("[data-note-act]");
    if (!b) return;
    if (b.dataset.noteAct === "see") open();
    else if (b.dataset.noteAct === "add") open({ add: true });
    else if (b.dataset.noteAct === "not-now") { await call("claude_account_nudge_answer", { answer: "notNow" }); render(); }
  });
  // A line about accounts is noted just after it is written: read the record again shortly after.
  if (bridge && bridge.listen) bridge.listen("rich://proactive-message", () => setTimeout(() => load(false), 400));

  window.RichSettings.registerAccounts({ open, paint: paintRow });
  window.RichAccounts = { open, noteSlot, refresh: () => load(true) };
  // The row stays current while the Settings menu shows it, and the conversation's buttons need
  // the record once at start. These reads use the quota service's cache (no extra provider call).
  load(false);
  setInterval(() => {
    if (document.hidden || polling) return;
    const row = document.getElementById("set-accounts-open");
    const visible = !sheet.hidden || (row && row.getClientRects().length);
    if (visible && Date.now() - lastPoll >= (sheet.hidden ? 30000 : 15000)) { polling = true; load(false).finally(() => { polling = false; }); }
    else if (!sheet.hidden && !(add && add.step === "name")) render();
  }, 1000);
})();

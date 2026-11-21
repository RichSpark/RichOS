"use strict";
// MEETING NOTES IN THE MAIN WINDOW (Granola PRD V4; round 25, approved with round 25.1:
// richos-hq design/mockups/rounds/round-25/ and round-25.1/, "the Granola PRD with round-25.1 is
// a go", the CEO, 2026-10-13).
//
// - SETTINGS, two rows under the Codex row: "Start meeting notes" (the full-row action shape with
//   a microphone glyph; while a recording runs it reads "Stop meeting notes" with
//   "Recording · 0:42" under it) and "Pause meeting prompts" (a switch). Pausing never touches a
//   running recording.
// - THE PAUSED CHIP in the stage header, "Meeting prompts paused · Resume", wherever the user is,
//   until Resume or the switch.
// - THE QUIET STATUS at the foot of the conversation while the notes are prepared: the working
//   dot and "Preparing your meeting notes…", nothing else moves; the composer still works. The
//   notes themselves, and any failure, arrive as Rich's own message.
//
// Every word is the round's; every fact is the app's (`meeting_state`, the `meeting-state`
// event, src-tauri/src/meetings.rs). A build without meeting notes answers available: false and
// nothing here appears.
(function () {
  const bridge = window.RichBridge;
  if (!bridge) return;

  const MIC = '<svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true"><rect x="9" y="2" width="6" height="12" rx="3"/><path d="M19 10v1a7 7 0 0 1-14 0v-1"/><path d="M12 18v4"/></svg>';
  const ROWS =
    '<button class="bugbtn" id="mn-start" type="button" role="menuitem">' + MIC +
      '<span class="set-row-text"><span class="set-name" id="mn-start-name">Start meeting notes</span>' +
      '<span class="set-row-state" id="mn-start-state" hidden></span></span>' +
    '</button>' +
    '<div class="set-row" id="mn-pause-row">' +
      '<span class="set-name" id="mn-pause-label">Pause meeting prompts</span>' +
      '<button class="sw" id="mn-pause" type="button" role="switch" aria-checked="false" aria-labelledby="mn-pause-label"></button>' +
    '</div>' +
    '<div class="set-rule" role="presentation"></div>';

  let st = null; // meeting_state: { available, paused, recording, startedAt, processing }
  let root = null;
  let registered = false;
  let ticker = null;
  const retrying = new Set();
  const $ = id => document.getElementById(id);
  const page = $("brief-page");
  const icon = $("nr-brief");
  let pending = null;

  function pageStatus(text) {
    const status = $("bp-status");
    status.textContent = text;
    status.hidden = !text;
  }

  async function runPageAction(command, args, progress) {
    if (pending) return;
    pending = progress;
    paint();
    try {
      const state = await bridge.invoke(command, args);
      if (state) apply(state);
      pending = null;
      paint();
    } catch (error) {
      pending = null;
      paint();
      pageStatus(String(error));
    }
  }

  async function call(cmd, args) {
    try { return await bridge.invoke(cmd, args || {}); }
    catch (e) { console.warn("[meeting-notes] " + cmd + ": " + e); return null; }
  }

  /** "0:42", "12:05", "1:02:09". */
  function clock(secs) {
    const s = Math.max(0, Math.floor(secs));
    const h = Math.floor(s / 3600), m = Math.floor((s % 3600) / 60), r = s % 60;
    const two = n => (n < 10 ? "0" : "") + n;
    return h ? h + ":" + two(m) + ":" + two(r) : m + ":" + two(r);
  }

  function apply(v) {
    if (!v) return;
    st = v;
    if (st.available && !registered && window.RichSettings && window.RichSettings.registerMeetingNotes) {
      registered = true;
      window.RichSettings.registerMeetingNotes({ render, onOpen: read });
    }
    paint();
  }
  async function read() { apply(await call("meeting_state")); }

  function render(container) {
    root = container;
    root.innerHTML = ROWS;
    root.querySelector("#mn-start").addEventListener("click", async () => {
      if (window.RichSettings && window.RichSettings.close) window.RichSettings.close();
      apply(await call("meeting_start_or_stop"));
    });
    root.querySelector("#mn-pause").addEventListener("click", async () => {
      apply(await call("meeting_set_paused", { paused: !(st && st.paused) }));
    });
    paint();
  }

  function paintRows() {
    if (!root || !root.querySelector("#mn-start")) return;
    const rec = !!(st && st.recording);
    root.querySelector("#mn-start-name").textContent = rec ? "Stop meeting notes" : "Start meeting notes";
    root.querySelector("#mn-start").classList.toggle("is-rec", rec);
    const line = root.querySelector("#mn-start-state");
    line.hidden = !rec;
    line.textContent = !rec ? "" : st.startedAt ? "Recording · " + clock((Date.now() - st.startedAt) / 1000) : "Starting…";
    root.querySelector("#mn-pause").setAttribute("aria-checked", String(!!(st && st.paused)));
  }

  function paintClock() {
    paintRows();
    if (page && st?.recording) {
      $("bp-live-t").textContent = st.startedAt
        ? "Recording · " + clock((Date.now() - st.startedAt) / 1000)
        : "Starting…";
    }
  }

  function paintPage() {
    if (!page) return;
    const recording = !!st?.recording;
    icon.hidden = !st?.available;
    $("nr-brief-dot").hidden = !recording;
    icon.setAttribute("aria-label", recording ? "RichBrief, recording a meeting" : "RichBrief");
    icon.dataset.tip = recording ? "RichBrief · recording" : "RichBrief";

    const action = $(recording ? "bp-stop" : "bp-start");
    const other = $(recording ? "bp-start" : "bp-stop");
    const movingFocus = document.activeElement === other;
    action.hidden = false;
    other.hidden = true;
    $("bp-cap").hidden = recording || !!st?.paused;
    $("bp-live").hidden = !recording;
    $("bp-pause").setAttribute("aria-checked", String(!!st?.paused));
    $("bp-ready").hidden = !st?.notesReady || recording;
    for (const control of [action, $("bp-pause"), $("bp-open")]) {
      if (pending) control.setAttribute("aria-disabled", "true");
      else control.removeAttribute("aria-disabled");
    }
    if (movingFocus && !page.hidden) action.focus({ preventScroll: true });
    pageStatus(pending || processingText() || "");
  }

  function processingText() {
    if (st?.processing === "waiting-model") return "Recording saved. Finishing setup for your notes…";
    if (st?.processing === "preparing") return "Preparing your meeting notes…";
    if (st?.processing === "transcript-failed" || st?.processing === "notes-failed") {
      return "RichBrief couldn’t prepare your meeting notes.";
    }
    return "";
  }

  function fillRetry(slot) {
    const id = slot.dataset.meetingRetry;
    const pending = retrying.has(id);
    const canRetry = (st?.retryTurns || []).includes(id);
    slot.hidden = !(canRetry || pending);
    slot.firstChild.disabled = pending;
  }

  function retrySlot(turnId) {
    if (!turnId) return null;
    const slot = document.createElement("div");
    slot.dataset.meetingRetry = turnId;
    const button = document.createElement("button");
    button.type = "button";
    button.className = "desk-btn";
    button.textContent = "Try again";
    const error = document.createElement("p");
    error.setAttribute("role", "status");
    error.hidden = true;
    button.addEventListener("click", async () => {
      if (retrying.has(turnId)) return;
      retrying.add(turnId);
      fillRetry(slot);
      error.hidden = true;
      try {
        await bridge.invoke("meeting_retry_saved", { turnId });
        if (st) st.retryTurns = (st.retryTurns || []).filter(id => id !== turnId);
      } catch (e) {
        error.textContent = String(e);
        error.hidden = false;
      } finally {
        retrying.delete(turnId);
        paint();
        fillRetry(slot);
      }
    });
    slot.append(button, error);
    fillRetry(slot);
    return slot;
  }

  const calendarSnapshots = new WeakMap();
  function paintCalendarRecovery(host, onPage = false) {
    if (!host) return;
    const notices = st?.calendarRecovery || [];
    const key = JSON.stringify(notices);
    if (calendarSnapshots.get(host) === key) return;
    calendarSnapshots.set(host, key);
    host.replaceChildren();
    host.hidden = !notices.length;
    const signingIn = notices.some(notice => notice.reconnecting);
    for (const notice of notices) {
      const name = notice.vendor === "google" ? "Google Calendar" : "Microsoft Calendar";
      const card = document.createElement("div");
      card.className = onPage ? "bp-notice" : "tl-notice";
      const text = document.createElement(onPage ? "b" : "p");
      text.className = "tl-notice-body";
      text.setAttribute("role", "status");
      text.textContent = onPage ? name + " is disconnected"
        : name + " is disconnected, so meetings on this account won’t prompt from the calendar.";
      const why = document.createElement("p");
      why.className = "tl-notice-body";
      why.textContent = onPage
        ? "Meetings on this account won’t prompt from the calendar. Calls are still detected from the microphone and Start meeting notes is right here."
        : "Calls are still detected from the microphone and Start meeting notes is in Settings.";
      const actions = document.createElement("div");
      actions.className = "desk-card-actions";
      const button = document.createElement("button");
      button.type = "button";
      button.className = onPage ? "bp-btn bp-btn-gold" : "desk-btn desk-btn--primary";
      // Its disabled label carries the current sign-in status.
      button.style.opacity = notice.reconnecting ? "1" : "";
      button.textContent = notice.reconnecting ? "Finish signing in in your browser…" : "Reconnect " + name;
      button.setAttribute("aria-label", button.textContent + " (" + notice.account + ")");
      button.disabled = signingIn;
      const waitingForOther = signingIn && !notice.reconnecting;
      button.hidden = waitingForOther;
      const error = document.createElement("p");
      error.className = "tl-notice-body";
      error.setAttribute("role", "status");
      error.hidden = !notice.failed && !notice.authorizationUrl && !waitingForOther;
      if (notice.authorizationUrl) {
        error.style.userSelect = "text";
        error.style.webkitUserSelect = "text";
        error.style.overflowWrap = "anywhere";
        error.textContent = "Couldn’t open your browser. Open this address to sign in:";
        const address = document.createElement("span");
        address.style.display = "block";
        address.style.userSelect = "all";
        address.style.webkitUserSelect = "all";
        address.textContent = notice.authorizationUrl;
        error.append(address);
      } else if (notice.failed) {
        error.textContent = "Calendar sign-in didn’t finish. Try reconnecting again.";
      } else if (waitingForOther) {
        error.textContent = "Finish signing in to the other calendar first.";
      }
      button.addEventListener("click", async () => {
        button.disabled = true;
        error.hidden = true;
        try {
          apply(await bridge.invoke("meeting_reconnect_calendar", { vendor: notice.vendor, account: notice.account }));
        } catch (e) {
          error.textContent = String(e);
          error.hidden = false;
        } finally {
          if (button.isConnected) button.disabled = (st?.calendarRecovery || []).some(item => item.reconnecting);
        }
      });
      actions.append(button);
      if (notices.filter(item => item.vendor === notice.vendor).length > 1) {
        const account = document.createElement("p");
        account.className = "tl-notice-body";
        account.textContent = notice.account;
        card.append(account);
      }
      card.append(text, why, actions, error);
      host.append(card);
    }
  }

  function paint() {
    paintClock();
    paintPage();
    paintCalendarRecovery($("mn-calendar-recovery"));
    paintCalendarRecovery($("bp-cal"), true);
    for (const slot of document.querySelectorAll("[data-meeting-retry]")) fillRetry(slot);
    const chip = document.getElementById("mn-paused-chip");
    if (chip) chip.hidden = !(st && st.available && st.paused);
    const status = document.getElementById("mn-status");
    if (status) {
      const waiting = st?.processing === "waiting-model";
      status.hidden = !waiting && st?.processing !== "preparing";
      status.lastElementChild.textContent = waiting
        ? "Recording saved. Finishing setup for your notes…"
        : "Preparing your meeting notes…";
    }
    // The row's time moves only while a recording runs.
    if (st && st.recording && !ticker) ticker = setInterval(paintClock, 1000);
    if (!(st && st.recording) && ticker) { clearInterval(ticker); ticker = null; }
  }

  const chip = document.getElementById("mn-paused-chip");
  if (chip) chip.addEventListener("click", async () => apply(await call("meeting_set_paused", { paused: false })));
  if (page) {
    window.RichNavRail.registerToolPage("brief", "RichBrief", page, { onShow: read });
    $("bp-start").addEventListener("click", () => runPageAction("meeting_start_or_stop", {}, "Starting…"));
    $("bp-stop").addEventListener("click", () => runPageAction("meeting_start_or_stop", {}, "Stopping…"));
    $("bp-pause").addEventListener("click", () => runPageAction("meeting_set_paused", { paused: !st?.paused }, "Saving…"));
    $("bp-open").addEventListener("click", async () => {
      if (pending || !st?.notesReady) return;
      const target = st.notesReady;
      pending = "Opening the notes…";
      paint();
      try {
        await window.RichOpenMeetingNotes(target);
      } catch (error) {
        pending = null;
        paint();
        pageStatus(String(error));
        return;
      }
      pending = null;
      paint();
    });
  }
  if (bridge.listen) bridge.listen("meeting-state", event => apply(event && event.payload !== undefined ? event.payload : event));
  read();

  // For the page's own suite: the clock's spelling.
  window.RichMeetingNotes = { clock, retrySlot };
})();

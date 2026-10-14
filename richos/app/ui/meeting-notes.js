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

  function paint() {
    paintRows();
    const chip = document.getElementById("mn-paused-chip");
    if (chip) chip.hidden = !(st && st.available && st.paused);
    const status = document.getElementById("mn-status");
    if (status) status.hidden = !(st && st.processing === "preparing");
    // The row's time moves only while a recording runs.
    if (st && st.recording && !ticker) ticker = setInterval(paintRows, 1000);
    if (!(st && st.recording) && ticker) { clearInterval(ticker); ticker = null; }
  }

  const chip = document.getElementById("mn-paused-chip");
  if (chip) chip.addEventListener("click", async () => apply(await call("meeting_set_paused", { paused: false })));
  if (bridge.listen) bridge.listen("meeting-state", event => apply(event && event.payload !== undefined ? event.payload : event));
  read();

  // For the page's own suite: the clock's spelling.
  window.RichMeetingNotes = { clock };
})();

"use strict";
// "LET CODEX REVIEW YOUR TEAM'S WORK": THE SETTINGS ROW. Round 20.2 (richos-hq
// design/mockups/rounds/round-20.2/index.html and NOTES.md, approved 2026-10-09: "round-20.2 is
// good to go!"), measured against the CEO's words, ruling §114: "in our app, we should give the
// user a toggle/switch to manually enable that. Because those reviews consume a bit of their Codex
// tokens, but mostly to make them aware that this would be happening in the first place." and
// "Near that toggle for Codex usage in RichOS app, we should also mention that this review process
// won't be visible in their regular ChatGPT/Codex app."
//
// - ONE ROW below Technical view, between two rules: the name, the switch, "Reviewing now: Claude"
//   or "Reviewing now: Codex", and an ⓘ right after the reviewer's name in every state. Nothing
//   sits under the row in any state; each state's words are in the ⓘ's tooltip, word for word.
// - FIVE STATES, from two facts (`codex_reviews_status`, richos-core codex_reviews.rs): what the
//   user chose (off on first run) and what the Mac reports about Codex (ready, signed out,
//   missing). ready+off "off"; ready+on "on"; not ready+off "missing" or "signedout" (the switch
//   cannot be turned on: dashed track, hollow knob); not ready+on "lapsed" (the choice stands,
//   Claude covers, it can still be turned off).
// - THE TOOLTIP: hover on the ⓘ (with a short grace so the pointer can move into it), keyboard
//   focus on it, a click pins it, Esc or a press elsewhere puts it away, and Esc puts away only
//   the tooltip. Neither the ⓘ nor the tooltip flips the switch. For a screen reader the ⓘ is
//   "About Codex reviews", and the current state's words describe both the ⓘ and the switch.
// - THE NUDGE: pressing the unavailable switch shakes it, tints the row and opens the tooltip with
//   the reason. A flip pulses the ⓘ's gold ring twice. With reduced motion, neither moves.
//
// Every word is round 20.2's; every fact is the app's own. Nothing here invents a state.
(function () {
  const bridge = window.RichBridge;
  if (!bridge) return;

  const INFO = '<svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" aria-hidden="true"><circle cx="12" cy="12" r="9.25"/><path d="M12 11v5.5"/><circle cx="12" cy="7.6" r=".6" fill="currentColor"/></svg>';
  // Round 20.2's markup, its words verbatim (index.html, "ROUND 20 — the Codex review switch").
  const ROW =
    '<div class="set-rule" role="presentation"></div>' +
    '<section class="cx" id="cx-row" aria-labelledby="cx-name">' +
      '<div class="cx-head">' +
        '<div class="cx-name" id="cx-name"><span id="cx-title">Let Codex review your team’s work</span>' +
          '<div class="cx-who"><span id="cx-who">Reviewing now: <b>Claude</b></span><span class="cx-info-wrap" id="cx-info-wrap"><button class="cx-info" id="cx-info" type="button" ' +
              'aria-label="About Codex reviews" aria-describedby="cx-tip-off" aria-expanded="false">' + INFO + '</button></span>' +
            '<div class="cx-tip" id="cx-tip" role="tooltip" hidden>' +
              '<div data-st="off" id="cx-tip-off">' +
                '<p class="cx-p">Before your team’s finished work is accepted, a second AI reviews it. Claude does that now. Turn this on and Codex does it instead, so a model from a different company checks the work.</p>' +
                '<div class="cx-aware"><p><strong>Codex will read your team’s work,</strong> and each review uses a little of your Codex allowance.</p>' +
                  '<p class="cx-unseen">These reviews won’t show up in your ChatGPT or Codex app: no conversations or history appear there.</p></div>' +
              '</div>' +
              '<div data-st="on" id="cx-tip-on" hidden>' +
                '<p class="cx-p">Codex reviews your team’s finished work before it’s accepted, through the Codex app on this Mac, signed in to ChatGPT.</p>' +
                '<div class="cx-aware"><p><strong>Codex reads your team’s work,</strong> and each review uses a little of your Codex allowance. Turn this off and Claude reviews it again.</p>' +
                  '<p class="cx-unseen">These reviews won’t show up in your ChatGPT or Codex app: no conversations or history appear there.</p></div>' +
              '</div>' +
              '<div data-st="missing" id="cx-tip-missing" hidden>' +
                '<p class="cx-p">Before your team’s finished work is accepted, a second AI reviews it. Claude does that now. Codex could do it instead, so a model from a different company checks the work.</p>' +
                '<p class="cx-why"><strong>The Codex app isn’t on this Mac.</strong> Install it and sign in to ChatGPT, and this switch can be turned on.</p>' +
              '</div>' +
              '<div data-st="signedout" id="cx-tip-signedout" hidden>' +
                '<p class="cx-p">Before your team’s finished work is accepted, a second AI reviews it. Claude does that now. Codex could do it instead, so a model from a different company checks the work.</p>' +
                '<p class="cx-why"><strong>Codex isn’t signed in.</strong> Open the Codex app and sign in to ChatGPT, and this switch can be turned on.</p>' +
              '</div>' +
              '<div data-st="lapsed" id="cx-tip-lapsed" hidden>' +
                '<p class="cx-p">You turned this on, so Codex reviews your team’s work whenever it can.</p>' +
                '<p class="cx-why cx-warn" id="cx-lapsed"></p>' +
                '<p class="cx-p cx-unseen">These reviews won’t show up in your ChatGPT or Codex app: no conversations or history appear there.</p>' +
              '</div>' +
            '</div>' +
          '</div>' +
        '</div>' +
        '<button class="sw" id="cx-switch" type="button" role="switch" aria-checked="false" aria-labelledby="cx-title cx-who" aria-describedby="cx-tip-off"></button>' +
      '</div>' +
    '</section>' +
    '<div class="set-rule" role="presentation"></div>';
  // The on-but-not-ready line, by what went away (round 20.2's paintCodex).
  const LAPSED = {
    missing: '<strong>The Codex app isn’t on this Mac right now,</strong> so Claude is reviewing in the meantime. Install it, sign in to ChatGPT, and Codex takes over again.',
    signedout: '<strong>Codex isn’t signed in right now,</strong> so Claude is reviewing in the meantime. Sign in to ChatGPT in the Codex app and Codex takes over again.',
  };

  let st = null; // codex_reviews_status: { on, codex: "ready" | "signedout" | "missing" }
  let root = null; // the slot settings-button.js gave this row
  let registered = false;
  let reading = false, again = false;
  const tip = { pinned: false, over: false, t: null };
  let nudgeTimer = null, ringTimer = null, flipTimer = null;

  async function call(cmd, args) {
    try { return await bridge.invoke(cmd, args || {}); }
    catch (e) { console.warn("[codex-reviews] " + cmd + ": " + e); return null; }
  }
  // A read asked for while one is on its way is not dropped: one more follows it, so the row
  // ends on what Codex reports after the last time it was asked, never before.
  async function read() {
    if (reading) { again = true; return; }
    reading = true;
    try {
      do {
        again = false;
        const v = await call("codex_reviews_status");
        if (v) st = v;
      } while (again);
    } finally { reading = false; }
    if (st && !registered && window.RichSettings && window.RichSettings.registerCodexReviews) {
      registered = true;
      window.RichSettings.registerCodexReviews({ render, onOpen: read });
    }
    paint();
  }

  // The state the row is in (round 20.2's cxView).
  function view() {
    if (!st) return "off";
    if (st.codex === "ready") return st.on ? "on" : "off";
    return st.on ? "lapsed" : (st.codex === "signedout" ? "signedout" : "missing");
  }
  const $ = id => root && root.querySelector("#" + id);
  // A choice made can always be undone; on needs Codex ready.
  const usable = () => !!st && (st.codex === "ready" || st.on);

  function paint() {
    if (!root || !$("cx-switch")) return;
    // **A SWITCH THAT CANNOT BE USED IS NOT SHOWN** (hunt 57, finding 19, 2026-10-15). Nightly 57
    // showed this row to a user without Codex, a switch that cannot be turned on beside a product
    // name he may never have heard of. So with Codex not ready and the switch off, the row, its
    // rules included, is not there; it appears the next time Codex is read ready (Settings
    // opening, the window coming back, `reread` below). A choice he made stays reachable: on and
    // not ready ("lapsed") is still shown, so it can be turned off. Round 20.2's words are kept.
    root.hidden = !usable();
    const v = view();
    const sw = $("cx-switch");
    sw.setAttribute("aria-checked", String(!!(st && st.on)));
    sw.setAttribute("aria-disabled", String(!usable()));
    // Only the switch says aria-disabled: on the name block a screen reader reads it into every
    // descendant, the ⓘ and its words included, which always work (the 2026-10-09 VM walk).
    $("cx-name").classList.toggle("is-unavailable", !usable());
    sw.title = usable() ? "" : (st && st.codex === "signedout" ? "Codex isn’t signed in" : "The Codex app isn’t on this Mac");
    $("cx-who").innerHTML = "Reviewing now: <b>" + (v === "on" ? "Codex" : "Claude") + "</b>";
    $("cx-lapsed").innerHTML = st && st.codex === "missing" ? LAPSED.missing : LAPSED.signedout;
    const tipId = "cx-tip-" + v;
    root.querySelectorAll("#cx-tip > div").forEach(d => { d.hidden = d.dataset.st !== v; });
    sw.setAttribute("aria-describedby", tipId);
    $("cx-info").setAttribute("aria-describedby", tipId);
  }

  async function press() {
    const sw = $("cx-switch");
    if (!sw) return;
    if (sw.getAttribute("aria-disabled") === "true") {
      // Codex may have been installed or signed in since the row was read: Settings can stay open
      // while the user follows the tooltip's words. So the app is asked again before the press is
      // refused, and a Codex ready now turns it on (the second review of ecb68ec68, finding 2).
      const now = await call("codex_reviews_status");
      if (now) { st = now; paint(); }
      if (st && st.on) return; // already on (chosen elsewhere): the press meant on, so nothing flips
    }
    if (sw.getAttribute("aria-disabled") === "true") {
      // It cannot be turned on: shake it, tint the row, and open the tooltip with the reason.
      sw.classList.remove("shake"); void sw.offsetWidth; sw.classList.add("shake");
      const box = $("cx-row");
      box.classList.add("is-nudged");
      clearTimeout(nudgeTimer);
      nudgeTimer = setTimeout(() => box.classList.remove("is-nudged"), 1100);
      tip.pinned = true; tipShow(true);
      return;
    }
    const next = await call("codex_reviews_set", { on: !(st && st.on) });
    if (!next) return;
    const flipped = !st || next.on !== st.on;
    // The switch's colors fade only for the flip itself (style.css `.sw.is-flipping`), never on
    // a theme switch.
    if (flipped) {
      sw.classList.add("is-flipping");
      clearTimeout(flipTimer);
      flipTimer = setTimeout(() => sw.classList.remove("is-flipping"), 400);
    }
    st = next;
    paint();
    if (flipped) ring();
  }
  // A flip changes the words in the ⓘ, so its gold ring pulses twice.
  function ring() {
    if (window.matchMedia && matchMedia("(prefers-reduced-motion: reduce)").matches) return;
    const wrap = $("cx-info-wrap");
    wrap.classList.remove("is-new"); void wrap.offsetWidth; wrap.classList.add("is-new");
    clearTimeout(ringTimer);
    ringTimer = setTimeout(() => wrap.classList.remove("is-new"), 2600);
  }

  // WHENEVER THE ROW CAN BE SEEN AGAIN, IT READS CODEX AGAIN (the second review of acfdd9e70,
  // rv-20261009T151041Z-acfdd9e7-95ff): Settings can stay open while the user signs out of Codex,
  // or back in, in the Codex app, and the next review picks its reviewer from Codex's login at
  // that moment. A row trusted from when Settings opened then names the wrong reviewer. So it is
  // read again when the app window regains focus or becomes visible with Settings open, and when
  // the tooltip opens; with Settings shut nothing is asked (the row is read when Settings opens).
  // Each is one `codex_reviews_status` (Codex's own `login status`, no network), never a timer.
  // The menu around the row, not the row itself: a row hidden because Codex was not ready has to
  // be read again, or it could never come back while Settings stays open.
  const visible = () => !!root && !!root.parentElement && !root.parentElement.closest("[hidden]");
  function reread() { if (visible()) return read(); }
  window.addEventListener("focus", reread);
  document.addEventListener("visibilitychange", () => { if (!document.hidden) return reread(); });

  function tipShow(fresh) {
    clearTimeout(tip.t);
    const el = $("cx-tip"), info = $("cx-info");
    if (!el || !info) return;
    if (el.hidden) {
      // The tooltip opening is the row being looked at: its words and the reviewer are read
      // again (the nudge passes `fresh`, having just read).
      if (!fresh) read();
      el.hidden = false;
      const tl = el.getBoundingClientRect().left, ir = info.getBoundingClientRect();
      el.style.setProperty("--ax", Math.round(ir.left + ir.width / 2 - tl) + "px");
      void el.offsetWidth;
    }
    el.classList.add("is-shown");
    info.setAttribute("aria-expanded", "true");
  }
  function tipHide(now) {
    clearTimeout(tip.t);
    const go = () => {
      const el = $("cx-tip"), info = $("cx-info");
      if (!el) return;
      if (!now && (tip.pinned || tip.over || document.activeElement === info)) return;
      tip.pinned = false;
      el.classList.remove("is-shown"); el.hidden = true;
      info.setAttribute("aria-expanded", "false");
    };
    if (now) go(); else tip.t = setTimeout(go, 220);
  }
  const tipOpen = () => { const el = $("cx-tip"); return !!el && !el.hidden; };

  function render(slot) {
    root = slot;
    tip.pinned = false; tip.over = false;
    slot.innerHTML = ROW;
    ["cx-info", "cx-tip"].forEach(id => {
      $(id).addEventListener("mouseenter", () => { tip.over = true; tipShow(); });
      $(id).addEventListener("mouseleave", () => { tip.over = false; tipHide(false); });
    });
    $("cx-info").addEventListener("focus", () => tipShow());
    $("cx-info").addEventListener("blur", () => tipHide(false));
    $("cx-info").addEventListener("click", () => {
      tip.pinned = !tip.pinned || $("cx-tip").hidden;
      if (tip.pinned) tipShow(); else tipHide(false);
    });
    $("cx-switch").addEventListener("click", press);
    $("cx-name").addEventListener("click", e => {
      if (e.target.closest(".cx-info, .cx-tip")) return; // the ⓘ and its words never flip the switch
      press();
    });
    $("cx-switch").addEventListener("animationend", function () { this.classList.remove("shake"); });
    paint();
  }

  // A press elsewhere puts the tooltip away; Esc puts away the tooltip first, and only the
  // tooltip: taken at the window, before the menu's and the app's own Escape (on the document).
  document.addEventListener("pointerdown", e => {
    if (tipOpen() && !(e.target.closest && e.target.closest("#cx-tip, #cx-info"))) {
      tip.pinned = false; tip.over = false; tipHide(true);
    }
  }, true);
  window.addEventListener("keydown", e => {
    if (e.key === "Escape" && tipOpen()) { tipHide(true); e.stopPropagation(); }
  }, true);

  window.RichCodexReviews = { read, state: () => (st ? Object.assign({}, st) : null), view };
  read();
})();

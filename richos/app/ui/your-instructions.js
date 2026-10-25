"use strict";
// =======================================================================================
// YOUR INSTRUCTIONS: THE SETTINGS ROW, THE SHEET, AND RICH'S CARD
// =======================================================================================
//
// The CEO, 2026-10-21: "Implement this simplified version for the app." Built as drawn from
// richos-hq design/mockups/rounds/round-36/ (README section 1, `everything-for-every-user.html`
// `?screen=instructions`), with the app's data in place of the mockup's examples. One change he
// approved on top of the round (2026-10-21 12:17:30Z): an edit is followed from the user's next
// NEW conversation, because a conversation that is already open keeps the instructions it started
// with, so the saved line says that rather than "from his next reply".
//
// - Settings → Working with Rich → "Your instructions", first in the column, with one plain line
//   under it ("6 rules, followed in every company", "Not written yet"): `settings-button.js`
//   draws the row, this file writes its line.
// - The sheet edits the user's own `instructions.md`, the file every session of Rich and Stu
//   appends in full (`user_settings::standing_plugin`), saved as they type
//   (`your_instructions_read`, `your_instructions_save`); "Open the file" and "Show in Finder" are
//   `instructions_file_open`, the same command the before and after of an update uses.
// - A new user gets an empty editor with three example lines as its hint and "Have Rich ask me",
//   which goes to the Onboarding conversation (`main.js` `RichHaveRichAsk`). How Rich then asks
//   is in his instruction files (the interview skill and `rich.md`), never here (§124).
// - Rich's save (`mcp__richos_onboarding__save_your_instructions`, `your_instructions.rs`) comes
//   back as the "Your instructions · Saved" card under his reply, its rules arriving one by one;
//   "See your instructions" opens the sheet with those lines lit in gold for a moment.
//
// The app never names Stu (§110's addendum): the sheet says "when he directs your team". The one
// place that does is the CEO's own "Info for technical people" behind its ⓘ, in his words.
// Styles: style.css "YOUR INSTRUCTIONS". Contrast is measured by ui/tests/your-instructions.js
// in both themes; nothing here is declared exempt.
(function () {
  const $ = (s, r) => (r || document).querySelector(s);
  const invoke = (name, args) => window.RichBridge.invoke(name, args || {});
  function h(html) { const t = document.createElement("template"); t.innerHTML = html.trim(); return t.content.firstChild; }

  const FILE = "instructions.md";
  const SAVED = "Saved. Rich follows the change from your next new conversation.";
  const EMPTY = "Nothing written yet.";
  const HINT = "For example:\n- Ask me before spending more than $500.\n- Show me any email to a customer before it goes out.\n- Start every report with the answer.";
  const SAVE_AFTER_MS = 600;

  const I = {
    layers: '<svg viewBox="0 0 20 20" fill="none" stroke="currentColor" stroke-width="1.6" stroke-linejoin="round" aria-hidden="true"><path d="M10 3l7 3.5-7 3.5-7-3.5z"/><path d="M3 10l7 3.5 7-3.5"/><path d="M3 13.5l7 3.5 7-3.5"/></svg>',
    doc: '<svg viewBox="0 0 20 20" fill="none" stroke="currentColor" stroke-width="1.6" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true"><path d="M5 2.5h7l3.5 3.5v11.5h-10.5z"/><path d="M12 2.5V6h3.5"/><path d="M8 10h5M8 13h5M8 16h3"/></svg>',
    check: '<svg viewBox="0 0 20 20" fill="none" stroke="currentColor" stroke-width="2.2" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true"><path d="M4 10.5l4 4 8-9"/></svg>',
    spin: '<svg viewBox="0 0 20 20" fill="none" stroke="currentColor" stroke-width="2.2" stroke-linecap="round" aria-hidden="true"><path d="M10 3a7 7 0 1 1-7 7"/></svg>',
  };

  // THE TWO ⓘ (the CEO, 2026-10-21, richos-hq docs/ceo-input/2026-10-21_1/feedback.md): after the
  // file's place, his "Info for technical people"; and under the line that ends "There is no
  // length limit.", an asterisk and his note on "every time". His words, set with the app's
  // typographic quotes. The ⓘ is Settings' own (`codex-reviews.js`, after "Reviewing now:
  // Claude"): the same glyph, its classes (`cx-info`, `cx-info-wrap`, `cx-tip`, `cx-p`) and its
  // behavior: hover with a short grace, keyboard focus, a click pins it, Esc or a press elsewhere
  // puts it away and Esc puts away only the tooltip.
  const INFO = '<svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" aria-hidden="true"><circle cx="12" cy="12" r="9.25"/><path d="M12 11v5.5"/><circle cx="12" cy="7.6" r=".6" fill="currentColor"/></svg>';
  const TECHNICAL =
    '<p class="cx-p yi-tip-head">Info for technical people</p>' +
    '<ul class="yi-tip-list">' +
      "<li>At the start of each Rich or Stu session, the RichOS app makes a fresh copy of your <code>rich.md</code> and <code>stu.md</code> for that session. (Stu is Rich’s twin brother who works behind the scenes as the orchestrator of your AI team. Rich talks to you and makes sure the work gets done to spec.)</li>" +
      "<li><code>rich.md</code> and <code>stu.md</code> each work like a <code>CLAUDE.md</code> or <code>AGENTS.md</code> for one agent. Each file is that agent’s own standing instructions.</li>" +
      "<li><code>instructions.md</code> contains your shared extra rules for both. You write a rule once and Rich and Stu both follow it from their next session. So, <code>rich.md</code> and <code>stu.md</code> contain rules that are different for each at the top. And the contents of <code>instructions.md</code> are added to each of those files at the bottom under the heading “The user’s own instructions”.</li>" +
    "</ul>";
  const EVERY_TIME =
    '<p class="cx-p">“Every time” is an ambition because it cannot be guaranteed. That has to do with fundamental limitations of modern-day AI.</p>' +
    '<p class="cx-p">So, in practice, <strong>you have to expect that sometimes one of those rules will be ignored</strong>.</p>' +
    '<p class="cx-p">RichOS is constantly developing new ways and updating the app to prevent that from happening as much as technically possible.</p>';
  function info(id, label, words, more) {
    return '<span class="cx-info-wrap yi-info-wrap" id="' + id + '-wrap"><button class="cx-info" id="' + id + '" type="button" aria-label="' + label +
      '" aria-describedby="' + id + '-tip" aria-expanded="false">' + INFO + "</button>" +
      '<div class="cx-tip yi-tip' + more + '" id="' + id + '-tip" role="tooltip" hidden>' + words + "</div></span>";
  }

  /// One ⓘ's tooltip, as Settings' ⓘ behaves.
  function wireInfo(id) {
    const button = $("#" + id, sheet), el = $("#" + id + "-tip", sheet);
    const tip = { pinned: false, over: false, t: null };
    function show() {
      clearTimeout(tip.t);
      if (el.hidden) {
        el.hidden = false;
        const tl = el.getBoundingClientRect().left, ir = button.getBoundingClientRect();
        el.style.setProperty("--ax", Math.round(ir.left + ir.width / 2 - tl) + "px");
        void el.offsetWidth;
      }
      el.classList.add("is-shown");
      button.setAttribute("aria-expanded", "true");
    }
    function hide(now) {
      clearTimeout(tip.t);
      const go = () => {
        if (!now && (tip.pinned || tip.over || document.activeElement === button)) return;
        tip.pinned = false;
        el.classList.remove("is-shown");
        el.hidden = true;
        button.setAttribute("aria-expanded", "false");
      };
      if (now) go(); else tip.t = setTimeout(go, 220);
    }
    [button, el].forEach((n) => {
      n.addEventListener("mouseenter", () => { tip.over = true; show(); });
      n.addEventListener("mouseleave", () => { tip.over = false; hide(false); });
    });
    button.addEventListener("focus", show);
    button.addEventListener("blur", () => hide(false));
    button.addEventListener("click", () => {
      tip.pinned = !tip.pinned || el.hidden;
      if (tip.pinned) show(); else hide(false);
    });
    document.addEventListener("pointerdown", (e) => {
      if (!el.hidden && !(e.target.closest && (e.target.closest("#" + id + "-tip") || e.target.closest("#" + id)))) {
        tip.pinned = false; tip.over = false; hide(true);
      }
    }, true);
    // Esc puts away the tooltip first, and only the tooltip: taken at the window, before the
    // app's own Escape closes the sheet.
    window.addEventListener("keydown", (e) => {
      if (e.key === "Escape" && !el.hidden) { hide(true); e.stopPropagation(); }
    }, true);
    return () => hide(true);
  }
  let hideTips = [];

  // What the file holds, as last read or saved. `null` until the first read answers.
  let text = null;
  let exists = false;
  let shown = "~/myrichos/" + FILE;
  let timer = null;
  let chain = Promise.resolve();
  let returnFocus = null;
  let glowRules = null;

  /// A line's words without its list mark, as `your_instructions::bare` reads it.
  function bare(line) {
    return line.trim().replace(/^[-*+]\s+/, "").trim();
  }

  /// The rules in the file: its list items, or, in a file written without a list, its lines
  /// (headings aside).
  function ruleCount(t) {
    const lines = t.split("\n").map((l) => l.trim()).filter(Boolean);
    const items = lines.filter((l) => /^[-*+]\s+\S/.test(l));
    return items.length || lines.filter((l) => !l.startsWith("#")).length;
  }

  function rowLine() {
    if (text === null) return "";
    if (!text.trim()) return "Not written yet";
    const n = ruleCount(text);
    return (n === 1 ? "1 rule" : n + " rules") + ", followed in every company";
  }

  function paintRow() {
    const line = document.getElementById("set-instructions-state");
    if (line) line.textContent = rowLine();
  }

  async function read() {
    const got = await invoke("your_instructions_read");
    text = got.text || "";
    exists = text.length > 0 || got.exists === true;
    if (got.shown) shown = got.shown;
    return got;
  }

  function refreshRow() {
    // Nothing is waiting to be saved: a read then cannot put back older words over newer ones.
    if (timer || (sheet && !sheet.hidden)) { paintRow(); return; }
    read().then(paintRow, paintRow);
  }

  // ------------------------------------------------------------------ the sheet ------------
  let sheet = null, editor = null, saveEl = null, glow = null, panel = null;

  function build() {
    sheet = h(
      '<div class="overlay yi-overlay" id="your-instructions" hidden role="dialog" aria-labelledby="yi-title" data-dismiss="control:#yi-done">' +
        '<div class="overlay-panel yi-sheet" data-empty="0">' +
          '<div class="yi-head"><h2 class="yi-title" id="yi-title">Your instructions</h2>' +
            '<span class="yi-everywhere">' + I.layers + "Every company</span></div>" +
          '<p class="yi-lede" id="yi-lede">Rich follows these <b>in full, every time*, in every company</b>: when he talks with you and when he directs your team. Write them in your own words. There is no length limit.</p>' +
          '<div class="yi-note">*' + info("yi-every", "About “every time”", EVERY_TIME, "") + "</div>" +
          '<div class="yi-editor-wrap"><textarea class="yi-editor" id="yi-editor" spellcheck="true" aria-label="Your instructions" aria-describedby="yi-lede"></textarea><div class="yi-glow" aria-hidden="true"></div></div>' +
          '<div class="yi-ask"><p>Not sure what to write? Rich can ask you a few questions and write them down for you.</p><button class="desk-btn desk-btn--confirm" type="button" id="yi-ask-rich">Have Rich ask me</button></div>' +
          '<div class="yi-file"><span class="yi-file-place">Kept in your own file, <code id="yi-path"></code>' + info("yi-tech", "Info for technical people", TECHNICAL, " yi-tip-up") + '</span><button class="yi-link" type="button" id="yi-open">Open the file</button><button class="yi-link" type="button" id="yi-reveal">Show in Finder</button></div>' +
          '<div class="yi-foot"><span class="yi-save" id="yi-save" role="status" aria-live="polite"></span><button class="desk-btn desk-btn--confirm" type="button" id="yi-done">Done</button></div>' +
        "</div></div>"
    );
    document.body.appendChild(sheet);
    editor = $("#yi-editor", sheet);
    saveEl = $("#yi-save", sheet);
    glow = $(".yi-glow", sheet);
    panel = $(".yi-sheet", sheet);
    editor.placeholder = HINT;
    hideTips = [wireInfo("yi-every"), wireInfo("yi-tech")];
    editor.addEventListener("input", () => {
      glowRules = null;
      paintGlow();
      paintEmpty();
      setSave("saving");
      clearTimeout(timer);
      timer = setTimeout(flush, SAVE_AFTER_MS);
    });
    editor.addEventListener("scroll", () => { glow.scrollTop = editor.scrollTop; });
    $("#yi-done", sheet).addEventListener("click", close);
    // A click outside the panel closes it, as Done does.
    sheet.addEventListener("mousedown", (e) => { if (e.target === sheet) close(); });
    $("#yi-ask-rich", sheet).addEventListener("click", () => {
      close();
      if (window.RichHaveRichAsk) window.RichHaveRichAsk("Have Rich ask me");
    });
    $("#yi-open", sheet).addEventListener("click", () => openFile(false));
    $("#yi-reveal", sheet).addEventListener("click", () => openFile(true));
    sheet.addEventListener("keydown", (event) => {
      if (event.key !== "Tab") return;
      const targets = [...sheet.querySelectorAll("button, textarea")].filter((n) => !n.disabled && n.offsetParent !== null);
      if (event.shiftKey && document.activeElement === targets[0]) { event.preventDefault(); targets.at(-1).focus(); }
      else if (!event.shiftKey && document.activeElement === targets.at(-1)) { event.preventDefault(); targets[0].focus(); }
    });
  }

  function setSave(state, why) {
    saveEl.dataset.s = state;
    if (state === "saved") saveEl.innerHTML = I.check + "<span></span>";
    else if (state === "saving") saveEl.innerHTML = I.spin + "<span></span>";
    else saveEl.innerHTML = "<span></span>";
    saveEl.lastChild.textContent =
      state === "saved" ? SAVED : state === "saving" ? "Saving…" : state === "empty" ? EMPTY : state === "error" ? why : "";
  }

  function paintEmpty() {
    panel.dataset.empty = editor.value.trim() ? "0" : "1";
  }

  /// The lines Rich just saved, lit behind the text for a moment (`@keyframes yi-mark`).
  function paintGlow() {
    glow.replaceChildren();
    if (!glowRules) return;
    const lines = editor.value.split("\n");
    lines.forEach((line, i) => {
      const span = document.createElement("span");
      span.textContent = line;
      if (line.trim() && glowRules.includes(bare(line))) {
        const mark = document.createElement("mark");
        mark.appendChild(span);
        glow.appendChild(mark);
      } else glow.appendChild(span);
      if (i < lines.length - 1) glow.appendChild(document.createTextNode("\n"));
    });
  }

  /// Save what is in the editor now. Saves go one after another, so an older one never lands
  /// after a newer one.
  function flush() {
    clearTimeout(timer);
    timer = null;
    const want = editor.value;
    chain = chain.then(async () => {
      try {
        await invoke("your_instructions_save", { text: want });
        text = want;
        exists = true;
        if (editor.value === want && !timer) setSave(want.trim() ? "saved" : "empty");
      } catch (error) {
        setSave("error", "That could not be saved: " + String(error));
      }
      paintRow();
    });
    return chain;
  }

  async function openFile(reveal) {
    if (timer) flush();
    await chain;
    try {
      // A new user's file may not be there yet: an empty one is what the app makes anyway.
      if (!exists) { await invoke("your_instructions_save", { text: editor.value }); exists = true; text = editor.value; }
      await invoke("instructions_file_open", { name: FILE, reveal });
    } catch (error) {
      setSave("error", "That could not be done: " + String(error));
    }
  }

  async function open(opts) {
    opts = opts || {};
    if (!sheet) build();
    returnFocus = document.activeElement;
    glowRules = opts.glow || null;
    sheet.hidden = false;
    editor.readOnly = true;
    setSave("");
    try {
      await read();
    } catch (error) {
      setSave("error", "That could not be done: " + String(error));
      return;
    }
    $("#yi-path", sheet).textContent = shown;
    editor.value = text;
    editor.readOnly = false;
    editor.scrollTop = 0;
    paintEmpty();
    paintGlow();
    setSave(text.trim() ? "saved" : "empty");
    if (!opts.noFocus) {
      editor.focus();
      editor.setSelectionRange(editor.value.length, editor.value.length);
    }
    paintRow();
  }

  function close() {
    if (!sheet || sheet.hidden) return;
    if (timer) flush();
    hideTips.forEach((put) => put());
    sheet.hidden = true;
    glowRules = null;
    paintGlow();
    paintRow();
    if (returnFocus && returnFocus.focus && document.contains(returnFocus)) returnFocus.focus();
  }

  // ------------------------------------------------------------------ Rich's card ----------
  // Its rules arrive one by one, once, when the card arrived live (`main.js` calls `arrived` on
  // `rich://rich-file`); a reload draws them still. The turn is rebuilt as the reply streams, so
  // each rule's start is kept here and carried into each new node, never restarted.
  const LEAD_MS = 600;
  const STEP_MS = 260;
  const RULE_MS = 450;
  const starts = new Map();

  function arrived(id) {
    if (id) starts.set(String(id), Date.now());
  }

  function card(item) {
    const rules = Array.isArray(item.rules) ? item.rules : [];
    const node = h(
      '<div class="yi-card"><div class="yi-card-head">' + I.doc +
        '<span class="yi-card-title">Your instructions</span><span class="yi-card-saved">' + I.check + "Saved</span></div>" +
        '<ul class="yi-card-rules" aria-label="Your instructions, as saved"></ul>' +
        '<div class="yi-card-foot"><p>Followed in every company.</p><button class="desk-btn yi-card-open" type="button">See your instructions</button></div></div>'
    );
    node.dataset.cardId = item.id || "";
    const list = $(".yi-card-rules", node);
    const start = starts.get(String(item.id));
    const now = Date.now();
    if (start != null && now > start + LEAD_MS + rules.length * STEP_MS + RULE_MS) starts.delete(String(item.id));
    rules.forEach((rule, i) => {
      const li = document.createElement("li");
      li.textContent = rule;
      if (starts.has(String(item.id))) {
        li.classList.add("is-arriving");
        li.style.setProperty("--rule-delay", start + LEAD_MS + i * STEP_MS - now + "ms");
      }
      list.appendChild(li);
    });
    $(".yi-card-open", node).addEventListener("click", () => open({ glow: rules.map(bare), noFocus: true }));
    return node;
  }

  window.RichYourInstructions = { open, close, card, arrived, ruleCount };
  if (window.RichSettings && window.RichSettings.registerInstructions) {
    window.RichSettings.registerInstructions({ open: () => open(), paint: paintRow, onOpen: refreshRow });
  }
  refreshRow();
})();

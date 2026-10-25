"use strict";
// SETTINGS → TERMINAL SETUP (richos-hq design round 41, piece 1), built as drawn.
//
// Rich's team works with the user's Terminal environment, read from the login shell when the app
// starts (`richos-core`'s `login_environment.rs`). This is the last row under "Working with Rich",
// with the time it was last read, and its sheet: "Read it again" (at most 10 seconds, never pressed
// twice), what the read found (the tool folders in order, the settings by NAME ONLY), and what
// changed since the read before, each change with a mark and a word as well as a color.
//
// The shell answers two commands: `terminal_setup` (the last read) and
// `terminal_setup_read_again`. Every word the sheet shows is composed here from what they return;
// no value of any setting ever reaches this page.
(function () {
  if (!window.RichSettings || !window.RichSettings.registerTerminalSetup) return;
  // main.js installs the native bridge after this module loads: resolve it at use time.
  const bridge = () => window.RichBridge;

  const SVG = (paths) =>
    '<svg viewBox="0 0 24 24" width="24" height="24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true">' + paths + "</svg>";
  const ICON = {
    ok: SVG('<circle cx="12" cy="12" r="10"/><path d="m8.5 12.5 2.5 2.5 5-5.5"/>'),
    warn: SVG('<path d="m21.73 18-8-14a2 2 0 0 0-3.48 0l-8 14A2 2 0 0 0 4 21h16a2 2 0 0 0 1.73-3"/><path d="M12 9v4"/><path d="M12 17h.01"/>'),
  };
  const WHAT = {
    "new folder": "New tool folder",
    "gone folder": "Tool folder gone",
    "new setting": "New setting",
    "changed setting": "Changed setting",
    "changed order": "Tool folders in a new order",
    "gone setting": "Setting gone",
  };
  const MARK = { new: "+", gone: "−", changed: "~" };

  const time = (ms) => new Date(ms).toLocaleTimeString(undefined, { hour: "numeric", minute: "2-digit" });
  const count = (n, one, many) => n + " " + (n === 1 ? one : many);
  function el(tag, cls, text) {
    const n = document.createElement(tag);
    if (cls) n.className = cls;
    if (text != null) n.textContent = text;
    return n;
  }
  function path(text) {
    return el("span", "tg-path", text);
  }

  let setup = null; // the last answer from the shell
  let reading = false;
  let returnFocus = null;
  let opened = false;

  // ---- the sheet ---------------------------------------------------------------------------
  const sheet = el("div", "tg-overlay");
  sheet.id = "terminal-setup";
  sheet.hidden = true;
  sheet.setAttribute("role", "dialog");
  sheet.setAttribute("aria-modal", "true");
  sheet.setAttribute("aria-labelledby", "ts-title");
  sheet.setAttribute("data-dismiss", "control:#ts-done");
  sheet.innerHTML =
    '<div class="tg-sheet">' +
    '<h2 class="tg-title" id="ts-title">Terminal setup</h2>' +
    '<p class="tg-lede">Rich’s team works with the same tools and settings as your Terminal: what a new Terminal window gets from your shell setup (<span class="tg-path">~/.zprofile</span>, <span class="tg-path">~/.zshrc</span>). After you install a tool or change your shell setup, read it again here.</p>' +
    '<div class="tg-status" id="ts-status" role="status" aria-live="polite"><span class="tg-status-icon" id="ts-icon" data-contrast-role="indicator"></span><div><div class="tg-status-main" id="ts-main"></div><div class="tg-status-sub" id="ts-sub"></div></div><button class="tg-btn tg-btn--gold" type="button" id="ts-read">Read it again</button><span class="tg-status-bar" aria-hidden="true"></span></div>' +
    '<section class="tg-changes" id="ts-changes" aria-labelledby="ts-changes-title" hidden><div class="tg-changes-head"><h3 class="tg-changes-title" id="ts-changes-title"></h3><span class="tg-changes-when" id="ts-changes-when"></span></div><div id="ts-change-rows"></div></section>' +
    '<div class="tg-found">' +
    '<section class="tg-found-col" aria-labelledby="ts-folders-title"><div class="tg-found-head"><h3 class="tg-found-title" id="ts-folders-title">Folders it looks in for tools</h3><span class="tg-found-count" id="ts-folders-count"></span></div><p class="tg-found-why">In this order, the first one that has a tool wins.</p><ol class="tg-list" id="ts-folders"></ol></section>' +
    '<section class="tg-found-col" aria-labelledby="ts-settings-title"><div class="tg-found-head"><h3 class="tg-found-title" id="ts-settings-title">Settings it has</h3><span class="tg-found-count" id="ts-settings-count"></span></div><p class="tg-found-why">Names only. Their values go to your team as they are and are never shown or kept.</p><div class="tg-chips" id="ts-settings"></div></section>' +
    "</div>" +
    '<div class="tg-foot"><p class="tg-note">Your team uses this from its next job. A job already running keeps the setup it started with.</p><button class="tg-btn" type="button" id="ts-done">Done</button></div>' +
    "</div>";
  document.body.appendChild(sheet);
  const $ = (id) => sheet.querySelector("#" + id);

  /// Which of the round's states the last answer is.
  function stateOf(s) {
    if (reading) return "reading";
    if (!s) return "reading";
    if (s.failed && s.readAtMs == null) return s.failed.atStart ? "startfail" : "neverread";
    if (s.failed) return "timeout";
    if (s.changes && s.changes.length) return "changes";
    if (s.changes) return "same";
    return "read";
  }

  function paint() {
    const s = setup;
    const state = stateOf(s);
    const status = $("ts-status");
    const shell = s ? s.shell : "zsh";
    const from = () => {
      const f = document.createDocumentFragment();
      f.append("From ", path(s.shell), " (", path(s.shellPath), "), the way a new Terminal window starts.");
      return f;
    };
    let tone = "ok", main = "", sub = null, button = "Read it again";
    switch (state) {
      case "reading":
        tone = "busy";
        main = "Reading your Terminal setup…";
        sub = "Starting " + shell + " the way a new Terminal window does. This takes a few seconds, never more than 10.";
        button = "Reading…";
        break;
      case "read":
        main = s.atStart ? "Read at " + time(s.readAtMs) + ", when RichOS started" : "Read at " + time(s.readAtMs);
        sub = from();
        break;
      case "changes":
        main = "Read again at " + time(s.readAtMs);
        sub = count(s.changes.length, "change", "changes") + " since " + time(s.sinceMs) + ", listed below.";
        break;
      case "same":
        main = "Read again at " + time(s.readAtMs) + ". Nothing changed.";
        sub = "It is the same as at " + time(s.sinceMs) + ". If you just changed something, open a new Terminal window and check it is there: that is what RichOS reads.";
        break;
      case "timeout":
        tone = "warn";
        main = "Your shell didn’t finish within 10 seconds";
        sub = "Nothing changed: your team keeps the setup read at " + time(s.readAtMs) + ". Something in your shell setup may be waiting for input or for the network. If a new Terminal window is slow to open too, that is the cause.";
        button = "Try again";
        break;
      case "neverread":
        tone = "warn";
        main = "Your shell didn’t finish within 10 seconds";
        sub = "Until it is read, your team has only the Mac’s basic tools, not the ones your Terminal has. Something in your shell setup may be waiting for input or for the network.";
        button = "Try again";
        break;
      case "startfail":
        tone = "warn";
        main = "Not read yet: your shell didn’t finish when RichOS started";
        sub = "Until it is read, your team has only the Mac’s basic tools, not the ones your Terminal has. Something in your shell setup may be waiting for input or for the network.";
        break;
    }
    status.dataset.tone = tone;
    $("ts-icon").innerHTML = tone === "busy" ? '<span class="tg-spin" aria-hidden="true"></span>' : tone === "warn" ? ICON.warn : ICON.ok;
    $("ts-main").textContent = main;
    const subNode = $("ts-sub");
    subNode.replaceChildren();
    if (sub != null) subNode.append(sub);
    const read = $("ts-read");
    read.disabled = state === "reading";
    read.setAttribute("aria-busy", String(state === "reading"));
    read.replaceChildren();
    if (state === "reading") read.append(el("span", "tg-spin"));
    read.append(button);

    // What changed: only after a read that found differences.
    const changes = $("ts-changes");
    changes.hidden = state !== "changes";
    const isNew = new Set(), isChanged = new Set();
    if (state === "changes") {
      $("ts-changes-title").textContent = count(s.changes.length, "change", "changes");
      $("ts-changes-when").textContent = "since " + time(s.sinceMs);
      const rows = $("ts-change-rows");
      rows.replaceChildren();
      s.changes.forEach((c, i) => {
        const row = el("div", "tg-change");
        row.dataset.kind = c.kind;
        row.style.animationDelay = 0.08 + i * 0.07 + "s";
        const mark = el("span", "tg-change-mark", MARK[c.kind]);
        mark.setAttribute("aria-hidden", "true");
        mark.dataset.contrastRole = "indicator";
        row.append(mark, el("span", "tg-change-what", WHAT[c.kind + " " + c.of]));
        const name = el("span", "tg-change-name", c.name);
        if (c.kind === "changed") name.append(el("span", "tg-change-note", c.of === "order" ? "now comes earlier, so a tool may come from it" : "its value is different now"));
        row.append(name);
        rows.append(row);
        if (c.kind === "new") isNew.add(c.of + ":" + c.name);
        if (c.kind === "changed" && c.of === "setting") isChanged.add(c.name);
      });
    }

    // What it found.
    const folders = $("ts-folders"), settings = $("ts-settings");
    folders.replaceChildren();
    settings.replaceChildren();
    const none = !s || (s.readAtMs == null);
    if (none) {
      folders.append(el("li", "tg-empty", "Nothing yet."));
      settings.append(el("p", "tg-empty", "Nothing yet."));
      $("ts-folders-count").textContent = "";
      $("ts-settings-count").textContent = "";
    } else {
      s.folders.forEach((f, i) => {
        const fresh = isNew.has("folder:" + f);
        const li = el("li", fresh ? "is-new" : "");
        const ord = el("span", "tg-ord", String(i + 1));
        ord.setAttribute("aria-hidden", "true");
        li.append(ord, el("span", null, f));
        if (fresh) li.append(el("span", "tg-new", "New"));
        folders.append(li);
      });
      for (const n of s.settings) {
        const fresh = isNew.has("setting:" + n), changed = isChanged.has(n);
        const chip = el("span", "tg-chip" + (fresh || changed ? " is-new" : ""), n);
        if (fresh) chip.append(" · new");
        if (changed) chip.append(" · changed");
        settings.append(chip);
      }
      $("ts-folders-count").textContent = count(s.folders.length, "folder", "folders");
      $("ts-settings-count").textContent = count(s.settings.length, "setting", "settings");
    }
    paintRow();
  }

  /// The Settings row's one plain line: when it was read, and what the last read again found.
  function paintRow() {
    const line = document.getElementById("set-terminal-state");
    if (!line) return;
    line.replaceChildren();
    const s = setup;
    if (!s) return;
    if (s.readAtMs == null) {
      const warn = el("span", "tg-row-warn");
      warn.innerHTML = ICON.warn;
      warn.append("Not read: your shell didn’t finish");
      line.append(warn);
    } else if (s.changes && !s.failed) {
      line.textContent = "Read again at " + time(s.readAtMs) + " · " + (s.changes.length ? count(s.changes.length, "change", "changes") : "nothing changed");
    } else {
      line.textContent = s.atStart ? "Read at " + time(s.readAtMs) + ", when RichOS started" : "Read at " + time(s.readAtMs);
    }
  }

  async function load() {
    try {
      setup = await bridge().invoke("terminal_setup");
    } catch (e) {
      setup = setup || null;
    }
    paint();
  }

  async function readAgain() {
    if (reading) return;
    reading = true;
    paint();
    try {
      setup = await bridge().invoke("terminal_setup_read_again");
    } catch (e) {
      // The shell answers every read, failed or not; a refused call changes nothing shown.
    }
    reading = false;
    paint();
    $("ts-read").focus();
  }

  function open() {
    returnFocus = document.activeElement;
    paint();
    opened = true;
    sheet.hidden = false;
    const panel = sheet.querySelector(".tg-sheet");
    panel.style.animation = "none";
    void panel.offsetWidth;
    panel.style.animation = "";
    // Read it again is the default button once the last read is in (it is disabled until then).
    load().then(() => $("ts-read").focus());
  }

  function close() {
    if (!opened) return;
    opened = false;
    sheet.hidden = true;
    // Back to what opened it when that is still on screen; the Settings row is not (the panel
    // closes as the sheet opens), so then to the Settings button.
    const back = returnFocus && returnFocus !== document.body && returnFocus.isConnected && returnFocus.getClientRects().length ? returnFocus : document.getElementById("set-btn");
    if (back && back.focus) back.focus();
  }

  // However the sheet is hidden (Done, Escape, a click outside, or the window's own Escape
  // handler), the focus comes back out of it.
  new MutationObserver(() => { if (sheet.hidden && opened) close(); }).observe(sheet, { attributes: true, attributeFilter: ["hidden"] });
  $("ts-read").addEventListener("click", readAgain);
  $("ts-done").addEventListener("click", close);
  sheet.addEventListener("pointerdown", (e) => {
    if (e.target === sheet) close();
  });
  sheet.addEventListener("keydown", (e) => {
    if (e.key === "Escape") {
      e.preventDefault();
      e.stopPropagation();
      close();
    }
  });

  window.RichSettings.registerTerminalSetup({ open, paint: paintRow, onOpen: load });
  window.RichTerminalSetup = { open, close };
})();

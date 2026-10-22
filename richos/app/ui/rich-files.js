"use strict";
// A FILE FROM RICH — the reverse of the user's attachments (§138, build plan item 12; round 36
// screen 3). Rich's reply carries the files he put in front of the user under his words, each as
// a card built like the composer's attachment chip: the page with the gold corner, the name, the
// type and the size. A click anywhere on the card opens the file with the Mac's default app
// (`rich_file_open`, which goes through `opener.rs`); "Show in Finder" sits beside it. A file
// that has since been moved or deleted says so and offers "Ask Rich again".
//
// The page holds attachment ids only. Rust resolves an id against the conversation's own record
// before anything is opened.
(function () {
  const PAGE = '<svg viewBox="0 0 40 48" aria-hidden="true"><path class="pg" d="M4 4.5A4 4 0 0 1 8 .5h18l13.5 13.5v29.5a4 4 0 0 1-4 4H8a4 4 0 0 1-4-4z"/><path class="fold" d="M26 .5v9.5a4 4 0 0 0 4 4h9.5z"/></svg>';
  const OUT = '<svg viewBox="0 0 14 14" fill="none" stroke="currentColor" stroke-width="1.8" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true"><path d="M4 10L10 4M5 4h5v5"/></svg>';
  const GONE = "No longer on this Mac. It was moved or deleted.";

  function node(tag, cls, text) {
    const el = document.createElement(tag);
    if (cls) el.className = cls;
    if (text != null) el.textContent = text;
    return el;
  }

  // What a click on the card says when it could not open: the sentence Rust wrote.
  function failed(card, meta, error) {
    const sentence = String((error && error.message) || error || "This Mac would not open it.");
    if (sentence === GONE) {
      card.dataset.s = "missing";
      meta.textContent = GONE;
    } else {
      card.dataset.s = "ready";
      meta.textContent = sentence;
    }
  }

  // A CARD THAT HAS JUST ARRIVED rises into place once (round 36 screen 3, `r36-file-in`: 0.7 s,
  // the first 0.25 s after it arrives, each next one 0.12 s after the one before). Only a card
  // that arrived live on `rich://rich-file` (`main.js` calls `arrived`), never one a reload draws
  // again. The turn is rebuilt as the reply streams, so the card's start is kept here and carried
  // into each new node (`--arrival-delay`, as `carryArrival` does for the rail), never restarted.
  const ARRIVE_MS = 700;
  const ARRIVE_LEAD_MS = 250;
  const ARRIVE_STEP_MS = 120;
  const starts = new Map();
  let lastStart = 0;

  function arrived(id) {
    if (!id) return;
    const start = Math.max(Date.now() + ARRIVE_LEAD_MS, lastStart + ARRIVE_STEP_MS);
    lastStart = start;
    starts.set(String(id), start);
  }

  function carryArrival(el, id) {
    const start = starts.get(id);
    if (start == null) return;
    const now = Date.now();
    if (now >= start + ARRIVE_MS) {
      starts.delete(id);
      return;
    }
    el.classList.add("is-arriving");
    el.style.setProperty("--arrival-delay", start - now + "ms");
    el.addEventListener("animationend", () => el.classList.remove("is-arriving"), { once: true });
  }

  function card(item, opts) {
    const f = item.file || {};
    const name = f.name || item.text || "File";
    const app = f.app || "";
    const openLabel = app ? "Open in " + app : "Open";
    const el = node("div", "tl-rfile");
    el.setAttribute("role", "listitem");
    el.dataset.fileId = f.id || item.id;
    el.dataset.s = f.exists === false ? "missing" : "ready";
    carryArrival(el, el.dataset.fileId);

    const main = node("button", "tl-rfile-main");
    main.type = "button";
    main.title = name;
    main.setAttribute("aria-label", app ? "Open " + name + " in " + app : "Open " + name);
    const glyph = node("span", "tl-rfile-glyph");
    glyph.innerHTML = PAGE;
    // Declared 14px and skippable (round 36 README): the type repeats at 16px beside it.
    glyph.appendChild(node("span", "tl-rfile-ext", f.ext || "FILE"));
    const text = node("span", "tl-rfile-text");
    text.appendChild(node("span", "tl-rfile-name", name));
    const meta = node("span", "tl-rfile-meta", f.exists === false ? GONE : (f.meta || ""));
    // The button's name is what pressing it does; its description is this line (type, size, pages,
    // or why it cannot open), which the name would otherwise hide from a screen reader.
    meta.id = "tl-rfile-meta-" + el.dataset.fileId;
    main.setAttribute("aria-describedby", meta.id);
    text.appendChild(meta);
    main.append(glyph, text);

    const acts = node("span", "tl-rfile-acts");
    const open = node("span", "tl-rfile-open");
    open.setAttribute("aria-hidden", "true");
    open.textContent = openLabel;
    open.insertAdjacentHTML("beforeend", OUT);
    const finder = node("button", "tl-rfile-finder", "Show in Finder");
    finder.type = "button";
    const ask = node("button", "desk-btn tl-rfile-ask", "Ask Rich again");
    ask.type = "button";
    acts.append(open, finder, ask);
    el.append(main, acts);

    const invoke = (command) =>
      window.RichBridge.invoke(command, { threadId: item.threadId, fileId: f.id || item.id });

    el.addEventListener("click", (e) => {
      if (el.dataset.s === "missing") {
        if (e.target.closest(".tl-rfile-ask") && opts && opts.sendText) {
          opts.sendText("Please send " + name + " again.");
        }
        return;
      }
      if (e.target.closest(".tl-rfile-finder")) {
        invoke("rich_file_reveal").catch((error) => failed(el, meta, error));
        return;
      }
      if (el.dataset.s === "opening") return;
      el.dataset.s = "opening";
      invoke("rich_file_open")
        .then(() => {
          setTimeout(() => {
            if (el.dataset.s === "opening") el.dataset.s = "ready";
          }, 600);
        })
        .catch((error) => failed(el, meta, error));
    });
    return el;
  }

  function render(items, opts) {
    const list = node("div", "tl-rfiles");
    list.setAttribute("role", "list");
    list.setAttribute("aria-label", items.length === 1 ? "1 file from Rich" : items.length + " files from Rich");
    for (const item of items) list.appendChild(card(item, opts));
    return list;
  }

  window.RichFiles = { render, arrived };
})();

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
  const LOCATE = "Locate the original. Your note is saved.";
  const libraryStatus = new Map();

  function node(tag, cls, text) {
    const el = document.createElement(tag);
    if (cls) el.className = cls;
    if (text != null) el.textContent = text;
    return el;
  }

  // What a click on the card says when it could not open: the sentence Rust wrote.
  function message(error) {
    return String((error && error.message) || error || "This Mac would not open it.");
  }

  function failed(card, meta, error) {
    const sentence = message(error);
    if (sentence === GONE) {
      card.dataset.s = "missing";
      meta.textContent = GONE;
    } else {
      card.dataset.s = "ready";
      meta.textContent = sentence;
    }
    named(card.querySelector(".tl-rfile-main"), meta);
  }

  function applyLibraryStatus(card, status) {
    card.dataset.s = status?.state || "ready";
    const meta = card.querySelector(".tl-rfile-meta");
    meta.textContent = status?.text || card.dataset.fileMeta;
    named(card.querySelector(".tl-rfile-main"), meta);
  }

  function updateLibraryStatus(key, status) {
    libraryStatus.delete(key);
    if (status) libraryStatus.set(key, status);
    if (libraryStatus.size > 64) {
      for (const [old, value] of libraryStatus) {
        if (value.state !== "opening") { libraryStatus.delete(old); break; }
      }
    }
    for (const card of document.querySelectorAll(".tl-rfile[data-library-card]")) {
      if (card.dataset.libraryCard === key) applyLibraryStatus(card, status);
    }
  }

  function downloading(id) {
    for (const [key, status] of libraryStatus) {
      if (status.file === id && status.state === "opening" && status.text !== "Downloading from iCloud…") {
        updateLibraryStatus(key, { ...status, text: "Downloading from iCloud…" });
      }
    }
  }

  // The card button's name: what pressing it does, then its line (type, size, pages, or why it
  // cannot open). The name stands in for the button's children, so without the line a screen reader
  // never says it; WebKit puts no `aria-describedby` text where the Mac's accessibility reads it
  // (the VM walk of 2026-10-22: the button's help stayed its title). Set again whenever the line changes.
  function named(main, meta) {
    const line = meta.textContent.trim();
    main.setAttribute("aria-label", main.dataset.action + (line ? ", " + line : ""));
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
    const missing = !f.library && f.exists === false;
    el.dataset.s = missing ? "missing" : "ready";
    carryArrival(el, el.dataset.fileId);

    const main = node("button", "tl-rfile-main");
    main.type = "button";
    main.title = name;
    main.dataset.action = app ? "Open " + name + " in " + app : "Open " + name;
    const glyph = node("span", "tl-rfile-glyph");
    glyph.innerHTML = PAGE;
    // Declared 14px and skippable (round 36 README): the type repeats at 16px beside it.
    glyph.appendChild(node("span", "tl-rfile-ext", f.ext || "FILE"));
    const text = node("span", "tl-rfile-text");
    text.appendChild(node("span", "tl-rfile-name", name));
    const meta = node("span", "tl-rfile-meta", missing ? GONE : (f.meta || ""));
    text.appendChild(meta);
    if(f.library && f.note)text.appendChild(node("span","tl-rfile-note",f.note));
    named(main, meta);
    main.append(glyph, text);

    const acts = node("span", "tl-rfile-acts");
    const open = node("span", "tl-rfile-open");
    open.setAttribute("aria-hidden", "true");
    open.textContent = openLabel;
    open.insertAdjacentHTML("beforeend", OUT);
    const finder = node("button", "tl-rfile-finder", f.library ? "Show in Folder" : "Show in Finder");
    finder.type = "button";
    const ask = node("button", "desk-btn tl-rfile-ask", f.library ? "Locate original" : "Ask Rich again");
    ask.type = "button";
    acts.append(open, finder, ask);
    el.append(main, acts);

    if (f.library) {
      const key = String(item.id || f.id);
      el.dataset.libraryCard = key;
      el.dataset.fileMeta = f.meta || "";
      meta.setAttribute("aria-live", "polite");
      applyLibraryStatus(el, libraryStatus.get(key));
      el.addEventListener("click", async (event) => {
        if (el.dataset.s === "opening") return;
        const missing = el.dataset.s === "missing";
        if (missing && !event.target.closest(".tl-rfile-ask")) return;
        const action = missing ? "locate" : event.target.closest(".tl-rfile-finder") ? "reveal" : "open";
        const opening = action === "open";
        updateLibraryStatus(key, { file: opening ? f.id : null, state: "opening", text: opening ? "Opening…" : "" });
        try {
          const result = await window.RichBridge.invoke("cleaner_action", { id: f.id, action, note: null });
          updateLibraryStatus(key, missing && !result?.id ? { state: "missing", text: LOCATE } : null);
        } catch (error) {
          const text = message(error);
          const unavailable = missing || text === "Locate the original file";
          updateLibraryStatus(key, { state: unavailable ? "missing" : "ready", text: text === "Locate the original file" ? LOCATE : text });
        }
      });
      return el;
    }

    const invoke = (command) => window.RichBridge.invoke(command, { threadId: item.threadId, fileId: f.id || item.id });

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

  window.RichFiles = { render, arrived, downloading };
})();

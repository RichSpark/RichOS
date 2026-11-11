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
    named(card.querySelector(".tl-rfile-main"), meta);
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
    if (f.moments) return mediaCard(item, f);
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
    main.dataset.action = app ? "Open " + name + " in " + app : "Open " + name;
    const glyph = node("span", "tl-rfile-glyph");
    glyph.innerHTML = PAGE;
    // Declared 14px and skippable (round 36 README): the type repeats at 16px beside it.
    glyph.appendChild(node("span", "tl-rfile-ext", f.ext || "FILE"));
    const text = node("span", "tl-rfile-text");
    text.appendChild(node("span", "tl-rfile-name", name));
    const meta = node("span", "tl-rfile-meta", f.exists === false ? (f.library ? "Locate the original. Your note is saved." : GONE) : (f.meta || ""));
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
    const ask = node("button", "desk-btn tl-rfile-ask", "Ask Rich again");
    ask.type = "button";
    if(f.library && f.exists===false)ask.textContent="Locate in RichCleaner";
    acts.append(open, finder, ask);
    el.append(main, acts);

    const invoke = (command) => f.library
      ? window.RichBridge.invoke("cleaner_action",{id:f.id,action:command==="rich_file_reveal" ? "reveal" : "open",note:null})
      : window.RichBridge.invoke(command, { threadId: item.threadId, fileId: f.id || item.id });

    el.addEventListener("click", (e) => {
      if (el.dataset.s === "missing") {
        if(f.library && e.target.closest(".tl-rfile-ask")){window.RichBridge.invoke("cleaner_open_window").catch(error=>failed(el,meta,error));return;}
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

  function mediaClock(ms) {
    const total = Math.floor(ms / 1000);
    const hours = Math.floor(total / 3600);
    const minutes = Math.floor(total / 60) % 60;
    const seconds = String(total % 60).padStart(2, "0");
    return hours ? hours + ":" + String(minutes).padStart(2, "0") + ":" + seconds : minutes + ":" + seconds;
  }

  function mediaCard(item, file) {
    const el = node("div", "tl-media");
    el.setAttribute("role", "listitem");
    el.dataset.fileId = file.id;
    const { durationMs, points, waveform } = file.moments;
    const head = node("div", "tl-media-head");
    head.append(node("strong", null, file.name), node("span", null, mediaClock(durationMs)));
    const track = node("div", "tl-media-track");
    track.setAttribute("aria-hidden", "true");
    if (typeof waveform === "string" && /^[0-9]{96}$/.test(waveform)) {
      const wave = node("div", "tl-media-wave");
      for (const level of waveform) {
        const bar = node("i");
        bar.style.height = (6 + Number(level) * 10) + "%";
        wave.appendChild(bar);
      }
      track.appendChild(wave);
    }
    const list = node("div", "tl-media-points");
    const status = node("p", "tl-media-status");
    status.setAttribute("role", "status");
    const available = file.exists !== false;
    if (!available) status.textContent = "The recording was moved, deleted or changed. Ask Rich to find the moments again.";
    points.forEach((point, momentIndex) => {
      const dot = node("i", "tl-media-mark");
      dot.style.left = (100 * point.startMs / durationMs) + "%";
      track.appendChild(dot);
      const time = mediaClock(point.startMs);
      const button = node("button", "tl-media-moment");
      button.type = "button";
      button.setAttribute("aria-label", time + " " + point.label);
      button.disabled = !available;
      const body = node("span", "tl-media-copy");
      const excerpt = node("span", null, point.quote);
      excerpt.id = file.id + "-excerpt-" + momentIndex;
      button.setAttribute("aria-describedby", excerpt.id);
      body.append(node("strong", null, point.label), excerpt);
      button.append(node("span", "tl-media-time", time), body);
      button.addEventListener("click", async () => {
        status.textContent = "";
        if (file.source === "youtube") {
          button.disabled = true;
          try {
            await window.RichBridge.invoke("media_moment_open", { fileId: file.id, momentIndex });
          } catch (error) {
            status.textContent = String(error?.message || error);
          } finally {
            button.disabled = false;
          }
        } else if (!window.RichOutput?.openMoment(item.threadId, {
          id: file.id, name: file.name, startMs: point.startMs, label: time + " · " + point.label,
        }, button)) {
          status.textContent = "Reopen this conversation to play the recording.";
        }
      });
      dot.addEventListener("click", () => button.click());
      list.appendChild(button);
    });
    el.append(head, track, list, status);
    return el;
  }

  function render(items, opts) {
    const list = node("div", "tl-rfiles");
    list.setAttribute("role", "list");
    list.setAttribute("aria-label", items.some(item => item.file?.moments)
      ? "Files and recordings from Rich"
      : items.length === 1 ? "1 file from Rich" : items.length + " files from Rich");
    for (const item of items) list.appendChild(card(item, opts));
    return list;
  }

  window.RichFiles = { render, arrived };
})();

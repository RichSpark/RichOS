// THE OUTPUT PANEL — slice S4 of the Output side panel PRD
// (richos-hq `docs/prds/2026-10-05-output-side-panel.md` §6, §12.4; design: round 17's
// `output.html`, frozen).
//
// The CEO, 2026-10-05: "a button here … ideally, at the top and at the bottom, so that the user
// can easily just click a button. And the entire, this entire side panel would open … with a
// list of all the files … that were output by this entire thread. This is what we need."
//
// WHAT THIS FILE OWNS: the two Output buttons (`#out-top`, `#out-bottom`) and their count, the
// docked panel (`#outpanel`) with its list, its empty and loading and unreadable states, a file's
// own view inside the panel, and the hooks the conversation's links call (`links`). The list
// comes from `list_output` (S3, `src-tauri/src/output_files.rs`), which converges the thread's
// output record and re-stats every file; `rich://output` says when the record gained files.
//
// PREVIEWS BY KIND (S5, §7): the file view draws the file itself — Markdown rendered, text,
// a CSV's first rows, a picture, a PDF, a video, a recording, an Office or iWork document's first
// page — from `output_preview`, or says the §6.7 sentence. `viewPreview` below; `hooks.viewer`
// still overrides it.
//
// THE WIDE PULL (S9, PRD §9, round 17.1) is its own section near the end of this file: the
// divider, the stop, the snap, *open completely* with the conversation one click back, and the
// composer floating in the panel. The panel's width is `--output-width`, written there.
//
// THE ACTIONS (S6) AND ADD TO CHAT (S7), at the foot of this file: Open in <app>, Open with…,
// Show in Finder, Save a copy…, Copy path, through `hooks.rowActions`, `hooks.fileTools` and
// `hooks.pathTools`; Add to chat last in the same menus, through `hooks.menuItems`.
//
// NO STRING FROM THE RECORD EVER BECOMES MARKUP. File names, folders, worker names and his own
// words are text nodes; the only markup built here is this file's own, through `node()`.
"use strict";

(function () {
  const SVG_NS = "http://www.w3.org/2000/svg";
  /// Nothing is said for the first 150 ms of a read — a local read is milliseconds (§6.7).
  const LOOKING_AFTER_MS = 150;
  /// The tile shows at most three letters; the name beside it carries the real extension (§4.5).
  const KIND_LABEL = { docx: "doc", xlsx: "xls", pptx: "ppt" };
  /// A family of formats rather than one: the tile shows the file's own extension instead.
  const FAMILY_KINDS = { png: true, mp4: true, audio: true, other: true };
  /// The facts line's word for a kind (§6.4). S5 replaces the facts with the preview's own.
  const KIND_WORD = {
    md: "Markdown",
    txt: "Text",
    csv: "Comma-separated",
    xlsx: "Spreadsheet",
    docx: "Document",
    pptx: "Presentation",
    pdf: "PDF",
    png: "Image",
    mp4: "Video",
    audio: "Audio",
  };
  /// VERBATIM from `MISSING` in `src-tauri/src/output_files.rs` (§6.7). The record says the
  /// file is gone; the sentence is the shell's, so the two can never say different things.
  const MISSING_SENTENCE =
    "This file is no longer where it was written. If it was moved, open it from its new place; if Rich writes it again, it will be listed here.";
  const MISSING_LINE = "No longer where it was written";

  const PATHS = {
    file: ["M14 3H7a2 2 0 0 0-2 2v14a2 2 0 0 0 2 2h10a2 2 0 0 0 2-2V8z", "M14 3v5h5"],
    left: ["m15 18-6-6 6-6"],
    right: ["m9 18 6-6-6-6"],
  };

  let bridge = null;
  let ctx = {};

  const state = {
    thread: null,
    /// The projected record as `list_output` answered it, or null before the first answer.
    list: null,
    /// The shell's own sentence when the record could not be read (§6.7), else null.
    error: null,
    loading: false,
    ticket: 0,
    open: false,
    view: "list", // "list" | "file"
    file: null, // the output id the file view shows
    /// The row kept selected when the file view steps back to the list (§6.4).
    selected: null,
    /// When the read in flight started, so *Looking…* waits its 150 ms whenever it is drawn.
    loadingSince: 0,
    /// Where focus goes when the panel closes: the button that opened it, or "conversation".
    returnTo: null,
    /// Bumped whenever the list changes, so the conversation re-renders its links (`links.rev`).
    rev: 0,
  };

  const el = (id) => document.getElementById(id);

  function node(tag, cls, text) {
    const n = document.createElement(tag);
    if (cls) n.className = cls;
    if (text != null) n.textContent = text;
    return n;
  }

  function icon(name) {
    const svg = document.createElementNS(SVG_NS, "svg");
    svg.setAttribute("viewBox", "0 0 24 24");
    svg.setAttribute("fill", "none");
    svg.setAttribute("stroke", "currentColor");
    svg.setAttribute("stroke-width", "2");
    svg.setAttribute("stroke-linecap", "round");
    svg.setAttribute("stroke-linejoin", "round");
    svg.setAttribute("aria-hidden", "true");
    svg.setAttribute("focusable", "false");
    for (const d of PATHS[name]) {
      const p = document.createElementNS(SVG_NS, "path");
      p.setAttribute("d", d);
      svg.appendChild(p);
    }
    return svg;
  }

  function files() {
    return state.list ? state.list.files : [];
  }

  function entry(id) {
    return files().find((f) => f.id === id) || null;
  }

  function filesWord(n) {
    return n === 1 ? "1 file" : n + " files";
  }

  function extension(name) {
    const at = String(name || "").lastIndexOf(".");
    return at > 0 ? name.slice(at + 1).toLowerCase() : "";
  }

  function tileLabel(e) {
    if (FAMILY_KINDS[e.kind]) return extension(e.name).slice(0, 3);
    return KIND_LABEL[e.kind] || e.kind;
  }

  /// Decimal units, as Finder shows them — the same rule as `human_size` in output_files.rs.
  function humanSize(bytes) {
    if (typeof bytes !== "number") return "";
    if (bytes < 1000) return bytes + (bytes === 1 ? " byte" : " bytes");
    if (bytes < 1e6) return Math.round(bytes / 1e3) + " KB";
    if (bytes < 1e9) return (bytes / 1e6).toFixed(1) + " MB";
    return (bytes / 1e9).toFixed(1) + " GB";
  }

  function sameDay(a, b) {
    return a.getFullYear() === b.getFullYear() && a.getMonth() === b.getMonth() && a.getDate() === b.getDate();
  }

  /// *Today 9:12 AM*, *Yesterday 4:12 PM*, *Just now*, *Oct 3 4:12 PM* — the group's time.
  function whenLabel(ms) {
    if (typeof ms !== "number") return "";
    const nowMs = Date.now();
    if (nowMs - ms >= 0 && nowMs - ms < 60000) return "Just now";
    const d = new Date(ms);
    const n = new Date(nowMs);
    const clock = d.toLocaleTimeString(undefined, { hour: "numeric", minute: "2-digit" });
    if (sameDay(d, n)) return "Today " + clock;
    const y = new Date(nowMs);
    y.setDate(y.getDate() - 1);
    if (sameDay(d, y)) return "Yesterday " + clock;
    const opts = { month: "short", day: "numeric" };
    if (d.getFullYear() !== n.getFullYear()) opts.year = "numeric";
    return d.toLocaleDateString(undefined, opts) + " " + clock;
  }

  // ---- the buttons -----------------------------------------------------------------------

  function buttons() {
    return [el("out-top"), el("out-bottom")].filter(Boolean);
  }

  /// The count on both buttons, their names and `aria-pressed`. With no answer yet, or a record
  /// that could not be read, there is no count and the name says only "Output" — never a number
  /// nobody has read (§6.7: "the buttons show no count").
  function paintButtons() {
    const known = !!state.list && !state.error;
    const n = known ? state.list.count : 0;
    const name = !known
      ? "Output"
      : n === 0
        ? "Output: nothing produced yet in this thread"
        : "Output: " + filesWord(n) + " from this thread";
    for (const b of buttons()) {
      b.hidden = !state.thread;
      b.classList.toggle("is-empty", !known || n === 0);
      b.querySelector("[data-count]").textContent = known && n > 0 ? String(n) : "";
      b.setAttribute("aria-label", name);
      b.setAttribute("aria-pressed", state.open ? "true" : "false");
    }
  }

  /// One tick on both counts when files arrive (§6.2). `prefers-reduced-motion` removes the
  /// animation in the stylesheet; the number still changes.
  function tick() {
    for (const c of document.querySelectorAll(".out-btn .out-count")) {
      c.classList.remove("tick");
      void c.offsetWidth;
      c.classList.add("tick");
    }
  }

  // ---- reading the record ----------------------------------------------------------------

  /// Read the active thread's record. `opts.arrival` marks a read caused by `rich://output`:
  /// what is new arrives with the rise and the scroll position of the list is kept (§6.6).
  async function load(opts) {
    opts = opts || {};
    const thread = state.thread;
    if (!thread || !bridge) return;
    const ticket = ++state.ticket;
    const before = new Set(files().map((f) => f.id));
    const beforeGroups = new Set(groupsOf(files()).map((g) => g.key));
    state.loading = true;
    state.loadingSince = Date.now();
    const looking = window.setTimeout(() => {
      if (ticket === state.ticket && state.loading && state.open && state.view === "list" && !state.list) render();
    }, LOOKING_AFTER_MS);
    let list = null;
    let error = null;
    try {
      list = await bridge.invoke("list_output", { threadId: thread });
    } catch (e) {
      error = String(e && e.message ? e.message : e);
    }
    window.clearTimeout(looking);
    if (ticket !== state.ticket || thread !== state.thread) return;
    state.loading = false;
    // What the shell says about each file (`output_file`) was asked before this read: it goes,
    // so every menu and Open asks again (`detailOf`), whatever the list says below.
    act.details.clear();
    const fresh = error ? null : normalize(list);
    // `fresh`: the re-read on opening (D6). When the disk matches what is drawn, nothing is
    // redrawn — not the list, not the conversation's links — so his focus and place are kept.
    if (opts.fresh && !error && !state.error && state.list && JSON.stringify(fresh) === JSON.stringify(state.list)) {
      act.detailsRev = state.rev;
      return;
    }
    const previousCount = state.list && !state.error ? state.list.count : null;
    if (error) {
      state.error = error;
      state.list = null;
    } else {
      state.error = null;
      state.list = fresh;
    }
    state.rev += 1;
    paintButtons();
    if (opts.arrival && state.list && previousCount !== null && state.list.count !== previousCount) {
      tick();
      if (ctx.announce) ctx.announce(filesWord(state.list.count) + " from this thread");
    }
    if (ctx.onLinksChanged) ctx.onLinksChanged();
    if (!state.open) return;
    // Redrawn under him: focus stays on the same control of the same file (D6's re-read can land
    // a few milliseconds after the panel put focus on its first row).
    const anchor = focusAnchor();
    if (state.view === "file") {
      if (entry(state.file)) renderFile(state.file, { keepFocus: true });
      else showList({ keepFocus: true });
      restoreFocus(anchor);
      reattachMenu();
      return;
    }
    const body = el("op-body");
    const top = body.scrollTop;
    render({ arrived: opts.arrival ? { files: before, groups: beforeGroups } : null });
    if (opts.arrival || opts.fresh) body.scrollTop = top;
    restoreFocus(anchor);
    reattachMenu();
  }

  /// A menu open over a redrawn list keeps working: its row and the `⋯` that opened it are the
  /// new nodes of the same file, so Escape still returns focus to the control he used.
  function reattachMenu() {
    const m = act.menu;
    if (!m) return;
    const body = el("op-body");
    const output = m.row && m.row.dataset ? m.row.dataset.output : null;
    if (m.row && !m.row.isConnected && output) {
      m.row = body.querySelector('.orow[data-output="' + cssEscape(output) + '"]');
      if (m.row) m.row.classList.add("menu-open");
    }
    if (m.opener && !m.opener.isConnected) {
      const act_ = m.opener.dataset.act;
      const scope = m.row ? m.row.closest(".orow-wrap") || body : body;
      m.opener = act_ ? scope.querySelector('[data-act="' + act_ + '"]') : null;
      if (m.opener) m.opener.setAttribute("aria-expanded", "true");
    }
  }

  /// Which control of the panel's body has focus, by what it is rather than which node it is:
  /// a redraw replaces every node. Null when focus is anywhere else.
  function focusAnchor() {
    const a = document.activeElement;
    const body = el("op-body");
    if (!a || !body || a === body || !body.contains(a)) return null;
    const row = a.closest(".orow-wrap") || a.closest(".orow");
    const rowEl = row ? (row.classList.contains("orow") ? row : row.querySelector(".orow")) : null;
    return {
      id: a.id || null,
      output: rowEl ? rowEl.dataset.output : null,
      act: a.dataset.act || null,
      seg: a.dataset.seg || null,
      label: a.getAttribute("aria-label"),
      view: body.dataset.view,
    };
  }

  function restoreFocus(k) {
    if (!k) return;
    const body = el("op-body");
    let target = null;
    if (k.id) target = document.getElementById(k.id);
    else if (k.output) {
      const row = body.querySelector('.orow[data-output="' + cssEscape(k.output) + '"]');
      const wrap = row ? row.closest(".orow-wrap") || row : null;
      target = !row ? null : k.act ? wrap.querySelector('[data-act="' + k.act + '"]') : row;
    } else if (k.act) target = body.querySelector('[data-act="' + k.act + '"]');
    else if (k.seg) target = body.querySelector('[data-seg="' + k.seg + '"]');
    else if (k.label) target = [...body.querySelectorAll("[aria-label]")].find((n) => n.getAttribute("aria-label") === k.label) || null;
    if (target && body.contains(target)) target.focus({ preventScroll: true });
    else if (body.dataset.view === "file" && el("of-back")) el("of-back").focus({ preventScroll: true });
    else focusFirst();
  }

  function normalize(list) {
    const out = list && typeof list === "object" ? list : {};
    const filesIn = Array.isArray(out.files) ? out.files : [];
    return {
      files: filesIn,
      count: typeof out.count === "number" ? out.count : filesIn.length,
      missing: typeof out.missing === "number" ? out.missing : filesIn.filter((f) => !f.exists).length,
    };
  }

  // ---- the list ----------------------------------------------------------------------------

  /// One group per turn, newest first, in the order the record lists its files (newest first by
  /// latest write); a write no turn could be named for sits in a last group (§6.3).
  function groupsOf(list) {
    const groups = [];
    const byKey = new Map();
    let between = null;
    for (const f of list) {
      if (!f.turnId) {
        if (!between) between = { key: "between", turnId: null, files: [] };
        between.files.push(f);
        continue;
      }
      let g = byKey.get(f.turnId);
      if (!g) {
        g = { key: "turn:" + f.turnId, turnId: f.turnId, files: [] };
        byKey.set(f.turnId, g);
        groups.push(g);
      }
      g.files.push(f);
    }
    if (between) groups.push(between);
    return groups;
  }

  /// His words and the time, read from the conversation the shell has loaded. A turn with no
  /// words of his is one Rich started (§6.3: *Rich reached out*); a turn the conversation does
  /// not hold says only when its files were written, rather than guessing who asked.
  function groupHead(g) {
    const head = node("div", "og-head");
    const latest = Math.max.apply(null, g.files.map((f) => f.writtenAt || 0));
    if (!g.turnId) {
      head.appendChild(node("span", "og-words", "Between turns"));
      const t = node("time", null, whenLabel(latest));
      head.appendChild(t);
      head.title = "Between turns · " + whenLabel(latest);
      return head;
    }
    const info = ctx.turnInfo ? ctx.turnInfo(g.turnId) : null;
    const at = info && typeof info.at === "number" ? info.at : latest;
    let titleParts = [];
    if (info && info.asked) {
      head.appendChild(node("q", "og-words", info.asked));
      titleParts.push("“" + info.asked + "”");
    } else if (info) {
      head.appendChild(node("span", "og-words", "Rich reached out"));
      titleParts.push("Rich reached out");
    } else {
      head.appendChild(node("span", "og-words"));
    }
    head.appendChild(node("time", null, whenLabel(at)));
    titleParts.push(whenLabel(at));
    if (info && typeof info.workedMs === "number" && window.RichTimeline && window.RichTimeline.formatDuration) {
      titleParts.push("worked for " + window.RichTimeline.formatDuration(info.workedMs));
    }
    head.title = titleParts.join(" · ");
    return head;
  }

  function renderRow(f) {
    const row = node("div", "orow");
    row.dataset.output = f.id;
    row.setAttribute("role", "button");
    row.tabIndex = 0;
    if (!f.exists) row.classList.add("is-missing");
    if (state.view === "list" && state.selected === f.id) row.classList.add("is-open");
    const tile = node("span", "okind", tileLabel(f));
    tile.setAttribute("aria-hidden", "true");
    row.appendChild(tile);
    const body = node("span", "obody");
    body.appendChild(node("span", "oname", f.name));
    body.appendChild(node("span", "opath", f.exists ? f.folder + "/" : MISSING_LINE));
    // The teammate by its name ("by Mark"), read by timeline.js's one rule (plan §7).
    if (f.actor === "worker" && f.workerName) body.appendChild(node("span", "oby", "by " + window.RichTimeline.teammateName(f.workerName)));
    row.appendChild(body);
    // S6's hover actions (Open in <app>, ⋯) and the row's context menu land here. They come back
    // BESIDE the row, in a wrapper, never inside it: a `role="button"` row's children are
    // presentational, so a button nested in it is invisible to VoiceOver (seen on the VM, S6).
    const placed = hooks.rowActions ? hooks.rowActions(f, row) : null;
    row.addEventListener("click", (e) => {
      if (e.target.closest("[data-act]")) return;
      showFile(f.id);
    });
    row.addEventListener("keydown", (e) => {
      if (e.target !== row) return;
      if (e.key === "Enter" || e.key === " ") {
        e.preventDefault();
        showFile(f.id);
      }
    });
    return placed || row;
  }

  function setHead(eyebrow, title, sub) {
    el("op-eyebrow").textContent = eyebrow;
    el("op-title").textContent = title;
    el("op-sub").textContent = sub || "";
    el("op-sub").hidden = !sub;
  }

  function render(opts) {
    opts = opts || {};
    const body = el("op-body");
    const view = node("div", "op-view");
    body.dataset.view = "list";
    const title = ctx.threadTitle ? ctx.threadTitle(state.thread) : "";
    if (state.error) {
      setHead("Output", "Output", null);
      const box = node("div", "op-state");
      box.appendChild(node("p", "op-state-line", state.error));
      const again = node("button", "op-again", "Try again");
      again.type = "button";
      again.addEventListener("click", () => load());
      box.appendChild(again);
      view.appendChild(box);
    } else if (!state.list) {
      setHead("Output", "Output", null);
      // Nothing for the first 150 ms; after that the body says it is looking (§6.7). Never a spinner.
      if (state.loading && Date.now() - state.loadingSince >= LOOKING_AFTER_MS) {
        view.appendChild(node("p", "op-state-line op-looking", "Looking…"));
      }
    } else if (!state.list.files.length) {
      setHead("Output", "Nothing produced yet", title);
      const empty = node("div", "op-empty");
      empty.tabIndex = -1;
      const glyph = node("div", "op-empty-glyph");
      glyph.appendChild(icon("file"));
      empty.appendChild(glyph);
      const p = node("p", "op-empty-line");
      p.appendChild(document.createTextNode("Nothing in "));
      p.appendChild(node("b", null, title));
      p.appendChild(
        document.createTextNode(
          " has produced a file yet. The moment Rich or the team writes one, it is listed here and the count on the Output button says so."
        )
      );
      empty.appendChild(p);
      view.appendChild(empty);
    } else {
      const n = state.list.count;
      const missing = state.list.missing || 0;
      setHead(
        "Output",
        filesWord(n) + " from this thread" + (missing ? " · " + missing + " no longer where " + (missing === 1 ? "it was" : "they were") + " written" : ""),
        "Everything written here, newest first."
      );
      for (const g of groupsOf(state.list.files)) {
        const section = node("section", "og");
        section.dataset.group = g.key;
        if (g.turnId) section.dataset.turn = g.turnId;
        if (opts.arrived && !opts.arrived.groups.has(g.key)) section.classList.add("arrive-group");
        section.appendChild(groupHead(g));
        for (const f of g.files) section.appendChild(renderRow(f));
        view.appendChild(section);
      }
    }
    body.textContent = "";
    body.appendChild(view);
  }

  function showList(opts) {
    opts = opts || {};
    state.view = "list";
    state.file = null;
    render();
    const body = el("op-body");
    if (opts.focusFile) {
      const row = body.querySelector('.orow[data-output="' + cssEscape(opts.focusFile) + '"]');
      if (row) {
        row.classList.add("is-open");
        state.selected = opts.focusFile;
        if (!opts.keepFocus) row.focus({ preventScroll: true });
        row.scrollIntoView({ block: "nearest" });
        return;
      }
    }
    if (!opts.keepFocus) focusFirst();
  }

  function focusFirst() {
    const body = el("op-body");
    const target = body.querySelector(".orow") || body.querySelector(".op-empty") || body.querySelector(".op-again");
    if (target) target.focus({ preventScroll: true });
    else body.focus({ preventScroll: true });
  }

  // ---- one file ----------------------------------------------------------------------------

  function showFile(id, opts) {
    if (!entry(id)) return showList(opts);
    // Showing a file asks the shell about it NOW (D6): its tools must never be lit from an
    // answer given before the file went. `detailOf`'s answer reconciles the list if it differs.
    act.details.delete(id);
    state.view = "file";
    state.file = id;
    state.selected = id;
    renderFile(id, opts);
  }

  /// The file's own view inside the panel (§6.4): the way back to the list, `‹ ›` and *k of N*,
  /// its path, and what S4 can say about it — its kind and size, or that it is no longer there.
  function renderFile(id, opts) {
    opts = opts || {};
    const f = entry(id);
    const list = files();
    const idx = list.indexOf(f);
    const body = el("op-body");
    body.dataset.view = "file";
    let sub = f.exists ? "Written " + whenLabel(f.writtenAt).replace(/^(Today|Yesterday|Just now)/, (m) => m.toLowerCase()) : MISSING_LINE;
    if (f.exists && typeof f.bytes === "number") sub += " · " + humanSize(f.bytes);
    if (f.firstTurnId && f.turnId && f.firstTurnId !== f.turnId) sub += " · also written earlier in this thread";
    setHead("Output", f.name, sub);

    const view = node("div", "op-view");
    const bar = node("div", "of-bar");
    const back = node("button", "of-back");
    back.type = "button";
    back.id = "of-back";
    back.appendChild(icon("left"));
    back.appendChild(node("span", null, "All output · " + list.length));
    back.addEventListener("click", () => showList({ focusFile: id }));
    bar.appendChild(back);
    const nav = node("span", "of-nav");
    const prev = node("button", "of-step");
    prev.type = "button";
    prev.setAttribute("aria-label", "Previous file");
    prev.title = "Previous file";
    prev.appendChild(icon("left"));
    const next = node("button", "of-step");
    next.type = "button";
    next.setAttribute("aria-label", "Next file");
    next.title = "Next file";
    next.appendChild(icon("right"));
    prev.addEventListener("click", () => showFile(list[(idx - 1 + list.length) % list.length].id, { focusStep: "prev" }));
    next.addEventListener("click", () => showFile(list[(idx + 1) % list.length].id, { focusStep: "next" }));
    nav.appendChild(prev);
    nav.appendChild(node("span", "of-k", idx + 1 + " of " + list.length));
    nav.appendChild(next);
    bar.appendChild(nav);
    view.appendChild(bar);

    // S6's Open in <app> / ▾ / Show in Finder / Save a copy… / ⋯ and S5's Preview | Source.
    const tools = node("div", "of-tools");
    if (hooks.fileTools) hooks.fileTools(f, tools);
    if (f.exists) sourceSwitch(f, tools);
    if (tools.childNodes.length) view.appendChild(tools);

    const pathLine = node("div", "of-path");
    const code = node("code");
    // Broken at its own joints (`/`, `.`, `-`, `_`), never mid-word: main.js's `wrapPathText`.
    if (window.RichWrapPath) code.appendChild(window.RichWrapPath(f.folder + "/" + f.name));
    else code.textContent = f.folder + "/" + f.name;
    code.title = f.path;
    pathLine.appendChild(code);
    // S6's *Copy the full path* sits at the end of the path line (§6.4).
    if (hooks.pathTools) hooks.pathTools(f, pathLine);
    view.appendChild(pathLine);

    const viewer = node("div", "of-view");
    viewer.id = "op-viewer";
    if (!f.exists) {
      viewer.appendChild(node("p", "of-missing", MISSING_SENTENCE));
    } else {
      (hooks.viewer || viewPreview)(f, viewer);
    }
    view.appendChild(viewer);

    body.textContent = "";
    body.appendChild(view);
    body.scrollTop = 0;
    if (opts.keepFocus) return;
    if (opts.focusStep === "prev") prev.focus({ preventScroll: true });
    else if (opts.focusStep === "next") next.focus({ preventScroll: true });
    else back.focus({ preventScroll: true });
  }

  // ---- S5: THE PREVIEWS, ONE VIEWER PER KIND (§7, §6.4, §6.7, §12.5) ------------------------
  //
  // `output_preview(output_id)` (`src-tauri/src/output_files.rs`) answers with a tagged view:
  // `text` (md, txt: the first 2 MiB), `table` (csv: the first 200 rows, parsed in Rust),
  // `image`, `pdf`, `video`, `audio` and `rendition` (a scheme URL the shell checked and serves),
  // or `none` with the §6.7 sentence and why. THE PAGE NEVER BUILDS A URL: every `src` below is
  // the one the shell returned, so the scheme's own checks (§5.2, a bad id is a 404 with no
  // bytes) are the only way bytes reach a viewer.
  //
  // NO FILE CONTENT EVER BECOMES MARKUP. Markdown goes through `timeline.js`'s DOM-only renderer
  // with its document extension (tables, quotations, rules, fenced code; links drawn as their
  // text); text and source are one text node in a `<pre>`; a table cell is a text node.
  //
  // Nothing is said for the first 150 ms of a read; after that the viewer says *Reading…* (§6.7).
  // The last answer is kept, so the Preview | Source switch and an arriving turn's re-render of the
  // SAME file view (§6.6) redraw from it without asking the shell again. Opening a file from the
  // list, or stepping to it, always asks: the file may have changed or gone since it was last shown.

  /// Preview or Source for `md` and `txt`, kept while the panel is open, as round 17 keeps it.
  /// `last` is the answer the viewer on screen was drawn from: `{ key, answer }`.
  const preview = { source: false, last: null };
  const SOURCE_KINDS = { md: true, txt: true };
  /// The format a picture is, for the facts line (*1280 × 800 · PNG · 96 KB*).
  const IMAGE_WORD = { png: "PNG", jpg: "JPEG", jpeg: "JPEG", gif: "GIF", webp: "WebP", heic: "HEIC", svg: "SVG" };
  /// What a QuickLook rendition is a page of, by the file's own extension.
  const DOC_WORD = {
    docx: "Word document",
    doc: "Word document",
    pages: "Pages document",
    rtf: "Rich text document",
    xlsx: "Excel spreadsheet",
    xls: "Excel spreadsheet",
    numbers: "Numbers spreadsheet",
    pptx: "PowerPoint presentation",
    ppt: "PowerPoint presentation",
    key: "Keynote presentation",
  };
  /// *The whole sheet opens in Numbers* (§7): what "the whole" of each kind is called.
  const WHOLE_WORD = { csv: "sheet", xlsx: "sheet", docx: "document", pptx: "presentation" };
  const TEXT_CAP_LINE = "Showing the first 2 MB";

  function previewKey(f) {
    return f.id + "@" + (f.modifiedAt == null ? "" : f.modifiedAt);
  }

  /// `0:31`, `4:05`, `1:02:09` — a length as a player shows it; never a guess.
  function clock(ms) {
    const total = Math.round(ms / 1000);
    const h = Math.floor(total / 3600);
    const m = Math.floor((total % 3600) / 60);
    const s = String(total % 60).padStart(2, "0");
    return h ? h + ":" + String(m).padStart(2, "0") + ":" + s : m + ":" + s;
  }

  function countWords(text) {
    const m = String(text).match(/\S+/g);
    return m ? m.length : 0;
  }

  function plural(n, one, many) {
    return n.toLocaleString("en-US") + " " + (n === 1 ? one : many);
  }

  /// The facts line under a viewer (§6.4): its first part bold, the rest after a middle dot.
  ///
  /// EACH DOT TRAVELS WITH THE PART AFTER IT, never alone. When the separators were flex items
  /// of their own, a part that wrapped left its dot at the end of the line above: "2 rows ·
  /// Comma-separated ·" over "The whole sheet opens in TextEdit" (walk 38, D9). Now a part that
  /// starts a line carries its dot into the line's left margin, which the line's own box clips
  /// (`.of-meta` in style.css), so no line starts or ends with a lone "·".
  function factsLine(parts) {
    const p = node("p", "of-meta");
    const row = node("span", "of-facts");
    parts.filter(Boolean).forEach((text, n) => {
      if (!n) return row.appendChild(node("b", null, text));
      const part = node("span", "of-fact");
      const dot = node("span", "of-dot", "·");
      dot.setAttribute("aria-hidden", "true");
      part.appendChild(dot);
      part.appendChild(document.createTextNode(text));
      row.appendChild(part);
    });
    p.appendChild(row);
    return p;
  }

  /// *Open in Preview*, or *Open* when Launch Services named no app (§5.4's degraded mode).
  /// Takes the app's NAME, as `output_preview` answers it. Not `openLabel(detail)` in "THE
  /// ACTIONS" below: the two shared one name when S5 and S6 were combined, and the later
  /// declaration silently replaced this one, so the sentence lost its app.
  function openInApp(app) {
    return app ? "Open in " + app : "Open";
  }

  /// A picture, a page or a recording the webview could not draw: said, with where it opens.
  function couldNotShow(app) {
    return node("p", "of-none", "I couldn't show this file here. " + openInApp(app) + " has it.");
  }

  /// The hook S4 left for this slice: fill `box` (the file view's `#op-viewer`) for `f`.
  function viewPreview(f, box) {
    const key = previewKey(f);
    // The view being replaced is still in the document while its successor is built: when it
    // showed this very file at this very modification time, its answer is the one to draw.
    const shown = el("op-viewer");
    if (shown && shown.dataset.key === key && preview.last && preview.last.key === key) {
      return drawPreview(f, preview.last.answer, box);
    }
    const reading = window.setTimeout(() => {
      if (box.isConnected && !box.childNodes.length) box.appendChild(node("p", "of-reading", "Reading…"));
    }, LOOKING_AFTER_MS);
    bridge.invoke("output_preview", { outputId: f.id }).then(
      (answer) => {
        window.clearTimeout(reading);
        const a = answer && typeof answer === "object" ? answer : { view: "none", why: "readFailed", reason: "" };
        if (box.isConnected) drawPreview(f, a, box);
        // The viewer found the file gone: the list, and this view's head and tools, say so (D6).
        if (a.why === "missing") reconcile(f.id, false);
      },
      (e) => {
        // The shell refused the id itself (not in this thread's record): its own sentence.
        window.clearTimeout(reading);
        if (box.isConnected) drawPreview(f, { view: "none", why: "refused", reason: String(e && e.message ? e.message : e) }, box);
      }
    );
  }

  function drawPreview(f, a, box) {
    const key = previewKey(f);
    preview.last = { key, answer: a };
    box.textContent = "";
    box.dataset.key = key;
    box.dataset.preview = a.view;
    if (a.why) box.dataset.why = a.why;
    else delete box.dataset.why;
    const size = typeof a.bytes === "number" ? humanSize(a.bytes) : typeof f.bytes === "number" ? humanSize(f.bytes) : "";
    const ext = extension(f.name);
    switch (a.view) {
      case "text": {
        const source = SOURCE_KINDS[f.kind] && preview.source;
        if (f.kind === "md" && !source) {
          const md = node("div", "of-md");
          if (window.RichTimeline && window.RichTimeline.renderMarkdownInto) {
            window.RichTimeline.renderMarkdownInto(md, a.text, null, { document: true });
          } else {
            md.textContent = a.text;
          }
          box.appendChild(md);
        } else {
          box.appendChild(node("pre", source ? "of-src" : "of-text", a.text));
        }
        if (a.truncated) box.appendChild(node("p", "of-note", TEXT_CAP_LINE));
        const lines = a.text ? a.text.split("\n").length - (a.text.endsWith("\n") ? 1 : 0) : 0;
        box.appendChild(
          f.kind === "md"
            ? factsLine([plural(countWords(a.text), "word", "words"), "Markdown"])
            : factsLine([plural(lines, "line", "lines"), KIND_WORD.txt])
        );
        return;
      }
      case "table": {
        const rows = Array.isArray(a.rows) ? a.rows : [];
        const head = rows[0] || [];
        const body = rows.slice(1);
        const width = rows.reduce((w, r) => Math.max(w, r.length), 0);
        // A column is a number column when every filled cell in it reads as one: right-aligned.
        const numeric = [];
        for (let c = 0; c < width; c += 1) {
          const cells = body.map((r) => (r[c] || "").trim()).filter(Boolean);
          numeric[c] = cells.length > 0 && cells.every((v) => /^[-+−(]?[$€£¥]?[\d.,]+[%)]?$/.test(v));
        }
        const wrap = node("div", "of-tbl-wrap");
        wrap.tabIndex = 0;
        wrap.setAttribute("role", "region");
        wrap.setAttribute("aria-label", f.name + ", the first rows");
        const table = node("table", "of-tbl");
        const thead = node("thead");
        const htr = node("tr");
        for (let c = 0; c < width; c += 1) {
          const th = node("th", numeric[c] ? "num" : null, head[c] || "");
          th.scope = "col";
          htr.appendChild(th);
        }
        thead.appendChild(htr);
        table.appendChild(thead);
        const tbody = node("tbody");
        for (const r of body) {
          const tr = node("tr");
          for (let c = 0; c < width; c += 1) tr.appendChild(node("td", numeric[c] ? "num" : null, r[c] || ""));
          tbody.appendChild(tr);
        }
        table.appendChild(tbody);
        wrap.appendChild(table);
        box.appendChild(wrap);
        // `totalRows` counts the header row; the facts count the rows under it.
        const total = Math.max(0, (a.totalRows || rows.length) - 1);
        if (body.length < total || !a.countedAll) {
          box.appendChild(node("p", "of-note", "Showing the first " + plural(body.length, "row", "rows")));
        }
        box.appendChild(
          factsLine([
            (a.countedAll === false ? "More than " : "") + plural(total, "row", "rows"),
            KIND_WORD.csv,
            a.app ? "The whole " + WHOLE_WORD.csv + " opens in " + a.app : null,
          ])
        );
        return;
      }
      case "image": {
        const frame = node("div", "of-img");
        const img = node("img");
        img.alt = f.name;
        img.decoding = "async";
        img.addEventListener("error", () => frame.replaceWith(couldNotShow(a.app)));
        img.src = a.url;
        frame.appendChild(img);
        box.appendChild(frame);
        box.appendChild(
          factsLine([a.width && a.height ? a.width + " × " + a.height : null, IMAGE_WORD[ext] || ext.toUpperCase() || KIND_WORD.png, size])
        );
        return;
      }
      case "pdf": {
        const frame = node("iframe", "of-pdf");
        frame.title = f.name;
        frame.src = a.url;
        box.appendChild(frame);
        box.appendChild(factsLine([KIND_WORD.pdf, size]));
        return;
      }
      case "video": {
        const player = node("video", "of-video");
        player.controls = true;
        player.preload = "metadata";
        player.playsInline = true;
        player.setAttribute("aria-label", f.name);
        // The player stays, with its controls, and the sentence says why it shows nothing.
        player.addEventListener("error", () => {
          if (!box.querySelector(".of-none")) player.after(couldNotShow(a.app));
        });
        player.src = a.url;
        box.appendChild(player);
        box.appendChild(
          factsLine([
            typeof a.durationMs === "number" ? clock(a.durationMs) : KIND_WORD.mp4,
            a.width && a.height ? a.width + " × " + a.height : null,
            size,
          ])
        );
        return;
      }
      case "audio": {
        const player = node("audio", "of-audio");
        player.controls = true;
        player.preload = "metadata";
        player.setAttribute("aria-label", f.name);
        player.addEventListener("error", () => {
          if (!box.querySelector(".of-none")) player.after(couldNotShow(a.app));
        });
        player.src = a.url;
        box.appendChild(player);
        box.appendChild(factsLine([typeof a.durationMs === "number" ? clock(a.durationMs) : null, KIND_WORD.audio, size]));
        return;
      }
      case "rendition": {
        // QuickLook's picture of the first page, on the file's own white in both themes (§6.9).
        const paper = node("div", "paper of-rendition");
        const img = node("img");
        img.alt = "The first page of " + f.name;
        img.decoding = "async";
        img.addEventListener("error", () => paper.replaceWith(couldNotShow(a.app)));
        img.src = a.url;
        paper.appendChild(img);
        box.appendChild(paper);
        box.appendChild(
          factsLine([
            DOC_WORD[ext] || KIND_WORD[f.kind] || "Document",
            size,
            a.app ? "The whole " + (WHOLE_WORD[f.kind] || "document") + " opens in " + a.app : null,
          ])
        );
        return;
      }
      default:
        // none: missing, refused (a link, a swapped file, not a regular file), readFailed,
        // tooLarge or noViewer — the shell's §6.7 sentence, verbatim.
        box.appendChild(node("p", "of-none", a.reason || ""));
    }
  }

  /// Preview | Source, for Markdown and plain text (§6.4, §11 row 11), in the file view's tools
  /// row before S6's `⋯` when it is there. One switch, two pressed states; it redraws the viewer
  /// from the kept answer and leaves focus where it was.
  function sourceSwitch(f, tools) {
    if (!SOURCE_KINDS[f.kind]) return;
    const seg = node("span", "of-seg");
    seg.setAttribute("role", "group");
    seg.setAttribute("aria-label", "Preview or source");
    const make = (label, isSource) => {
      const b = node("button", null, label);
      b.type = "button";
      b.dataset.seg = isSource ? "source" : "preview";
      b.setAttribute("aria-pressed", String(preview.source === isSource));
      b.addEventListener("click", () => {
        if (preview.source === isSource) return;
        preview.source = isSource;
        for (const other of seg.querySelectorAll("button")) {
          other.setAttribute("aria-pressed", String((other.dataset.seg === "source") === isSource));
        }
        const box = el("op-viewer");
        if (box && preview.last && preview.last.key === previewKey(f)) drawPreview(f, preview.last.answer, box);
      });
      return b;
    };
    seg.appendChild(make("Preview", false));
    seg.appendChild(make("Source", true));
    const more = tools.querySelector('[data-act="menu"]');
    if (more) tools.insertBefore(seg, more);
    else tools.appendChild(seg);
  }

  // ---- open, close, step back ----------------------------------------------------------------

  function isWide() {
    return ctx.isWide ? ctx.isWide() : true;
  }

  /// The scrim exists only where the panel overlays the conversation (below 1180px, §6.1).
  /// Called on open and whenever the window crosses a breakpoint.
  function syncScrim() {
    const scrim = el("outpanel-scrim");
    const panel = el("outpanel");
    if (!scrim || !panel) return;
    scrim.hidden = panel.hidden || isWide();
    // The stop and the snap exist only where the panel docks (§6.1): a window that narrows
    // under 1180px while it is open completely brings the conversation back.
    pullBreakpoint();
  }

  /// Open the panel. `opts.returnTo` is the element focus goes back to on close, or
  /// "conversation" when it was opened from a link in the conversation (§6.8).
  function open(opts) {
    opts = opts || {};
    if (!state.thread) return;
    const panel = el("outpanel");
    const already = state.open && !panel.hidden;
    // One right-hand pane at a time (§6.1): the worker inspector and the under-the-hood
    // slide-over close before this one opens.
    if (!already && ctx.onOpen) ctx.onOpen();
    state.open = true;
    state.returnTo = opts.returnTo || state.returnTo || null;
    panel.hidden = false;
    syncScrim();
    document.body.classList.add("output-open");
    // Reopening is at the split width it had, never open completely (§9.4).
    if (!already) pullPaint({ instant: true });
    if (!already) {
      panel.classList.remove("is-opening");
      void panel.offsetWidth;
      panel.classList.add("is-opening");
    }
    paintButtons();
    if (opts.file) showFile(opts.file, opts);
    else showList(opts);
    // OPENING READS THE DISK AGAIN (§4.6: "every read re-stats … never cached across calls").
    // The list it holds is only as fresh as the last `rich://output`: a file deleted, moved or
    // swapped while the panel was closed announces nothing, so it was drawn present — its
    // actions lit and its view reading "Written … · 30 bytes" over "This file is no longer where
    // it was written" (walk 38, D6). What it holds is drawn at once and the re-read corrects it
    // in place, changing nothing on screen when nothing changed on disk. A read already in
    // flight (the thread was just chosen) is fresh, and answers for itself.
    if (!already && !state.loading && (state.list || state.error)) load({ fresh: true });
  }

  function close(opts) {
    opts = opts || {};
    const panel = el("outpanel");
    const wasOpen = state.open || !panel.hidden;
    // A menu first, without taking focus: the panel is about to hide under it (S6).
    closeMenu({ focus: false });
    // Closing brings the WHOLE conversation back and clears *open completely* (§9.4), before
    // the panel hides: the composer goes home first, so focus can return to it.
    pullReset();
    state.open = false;
    state.view = "list";
    state.file = null;
    state.selected = null;
    panel.hidden = true;
    panel.classList.remove("is-opening");
    el("outpanel-scrim").hidden = true;
    document.body.classList.remove("output-open");
    paintButtons();
    const back = state.returnTo;
    state.returnTo = null;
    if (!wasOpen || opts.keepFocus) return;
    if (back && back !== "conversation" && back.isConnected && back.getClientRects().length) back.focus({ preventScroll: true });
    else if (ctx.focusConversation) ctx.focusConversation();
  }

  function toggle(opts) {
    if (state.open && !el("outpanel").hidden) close();
    else open(opts);
  }

  /// Escape, one level at a time (§6.8): the file view to the list with the row kept selected,
  /// then the list to closed. Read off the DOM as well as the state, because the Escape rule's
  /// own suite opens a surface by un-hiding it.
  function escape() {
    // A menu first (S6, §6.8): a submenu back to its menu, a menu closed. It is the topmost
    // layer, so it goes before the floating composer's words.
    if (closeMenu({ step: true })) return;
    // From the floating composer with words in it, Escape clears the words first (§6.8).
    if (pullEscape()) return;
    if (state.open && state.view === "file" && state.file) {
      showList({ focusFile: state.file });
      return;
    }
    close();
  }

  // ---- the links in the conversation (§6.5) -------------------------------------------------

  function flash(n) {
    if (!n) return;
    n.classList.remove("flash");
    void n.offsetWidth;
    n.classList.add("flash");
  }

  /// A file named in Rich's message: the link pulses once and the panel opens on that file.
  function openFile(id, from) {
    flash(from);
    open({ file: id, returnTo: "conversation" });
  }

  /// The turn's *Wrote N files*: the panel opens at the list and that turn's group pulses once.
  function openGroup(turnId, from) {
    flash(from);
    open({ returnTo: "conversation", keepFocus: true });
    if (state.view !== "list") showList({ keepFocus: true });
    const g = el("op-body").querySelector('.og[data-turn="' + cssEscape(turnId) + '"]');
    if (!g) return focusFirst();
    g.scrollIntoView({ block: "start" });
    flash(g);
    const first = g.querySelector(".orow");
    if (first) first.focus({ preventScroll: true });
  }

  /// What `timeline.js` asks of the record. Only RECORDED files become links: a name nothing
  /// witnessed stays a code span. A name is matched only when it is unique in the thread.
  const links = {
    rev: () => state.rev,
    forText(text) {
      const t = String(text || "").trim();
      if (!t || !state.list) return null;
      let byName = null;
      let names = 0;
      for (const f of state.list.files) {
        if (t === f.path || t === f.folder + "/" + f.name) return f;
        if (t === f.name) {
          names += 1;
          byName = f;
        }
      }
      return names === 1 ? byName : null;
    },
    forTurn(turnId) {
      if (!turnId || !state.list) return [];
      return state.list.files.filter((f) => f.turnId === turnId);
    },
    openFile,
    openGroup,
  };

  /// The hooks the later slices fill. `viewer` overrides S5's `viewPreview`; "THE ACTIONS"
  /// below fills `rowActions`, `fileTools` and `pathTools` (S6) and `menuItems` (S7).
  const hooks = { viewer: null, rowActions: null, fileTools: null, pathTools: null, menuItems: null };

  function cssEscape(s) {
    return String(s).replace(/(["\\])/g, "\\$1");
  }

  // ---- the thread ------------------------------------------------------------------------------

  /// The thread the conversation shows, or null when none is (the home screen, a company's
  /// overview, the unbound screen). A different thread empties the list and reads its own; an
  /// open panel shows the new thread's list (§4.6, §6.7).
  function setThread(threadId) {
    const next = threadId || null;
    if (next === state.thread) {
      paintButtons();
      return;
    }
    state.thread = next;
    // The pill names the thread the panel now shows (§9.4).
    if (next) pullPaintPill();
    state.list = null;
    state.error = null;
    state.loading = false;
    state.ticket += 1;
    state.rev += 1;
    state.view = "list";
    state.file = null;
    state.selected = null;
    if (!next) {
      close({ keepFocus: true });
      paintButtons();
      return;
    }
    paintButtons();
    if (state.open) render();
    load();
  }

  /// THE TOP BUTTON IS NEVER UNDER THE SETTINGS CLUSTER. `#stage-header` reserves the settings
  /// button's 40px lane (`--chrome-lane`, 66px), and the cluster grows LEFTWARD when the update
  /// cue arrives beside the button (`.settings` is a fixed row, `.update-cue` up to 22rem) — which
  /// put the cue straight over `#out-top`, so the button could be neither seen nor pressed
  /// (found in `shots-updates`). The cluster's measured width is published as
  /// `--chrome-lane-live`, and the header reserves whichever lane is wider.
  function watchChromeLane() {
    const settings = document.querySelector(".settings");
    if (!settings || typeof ResizeObserver !== "function") return false;
    const publish = () => {
      const box = settings.getBoundingClientRect();
      const lane = box.width > 0 ? Math.ceil(window.innerWidth - box.left + 8) : 0;
      document.documentElement.style.setProperty("--chrome-lane-live", lane + "px");
    };
    new ResizeObserver(publish).observe(settings);
    window.addEventListener("resize", publish);
    publish();
    return true;
  }

  function init(options) {
    bridge = options.bridge;
    ctx = options;
    if (!watchChromeLane()) {
      // `settings-button.js` mounts the cluster on its own schedule; look again once it has.
      let tries = 0;
      const retry = () => {
        if (watchChromeLane() || ++tries > 120) return;
        window.requestAnimationFrame(retry);
      };
      window.requestAnimationFrame(retry);
    }
    for (const b of buttons()) {
      b.addEventListener("click", () => toggle({ returnTo: b }));
    }
    el("op-close").addEventListener("click", () => close());
    el("outpanel-scrim").addEventListener("click", () => close());
    bridge.listen("rich://output", ({ payload }) => {
      if (!payload || payload.threadId !== state.thread) return;
      load({ arrival: true });
    });
    pullInit();
    paintButtons();
  }

  // ============================================================================================
  // THE WIDE PULL — slice S9 (PRD §9; round 17.1's `output.html:1337-1402`, taken as written)
  // ============================================================================================
  //
  // The CEO, 2026-10-05: "when I'm dragging up to here, then there's a stop, I'm feeling a stop.
  // But if I keep dragging … then eventually it snaps open completely the output sidebar. So
  // this is the behavior we want to copy, for now." And: "if I close the output sidebar, then
  // I'm back, I have back the whole thing."
  //
  // THE MATH, every width measured from the app's right edge, as the mockup measures it:
  //   the stop        app − rail − STAGE_MIN       the conversation at its narrowest (§9.1)
  //   open completely app − rail                   the conversation at zero width (§9.3)
  //   the snap        SNAP_PAST px of pull on past the stop; over those px the conversation dims
  //                   (opacity 1 − snap × .62) and the divider's gold spine thickens, so the snap
  //                   is never a surprise; the divider lets go of the pointer for SNAP_MS
  //   the return      SNAP_PAST px back from where the snap happened: the conversation comes back
  //                   at its narrowest and the divider is at the stop, under the pointer; nothing
  //                   arms again until the pointer has come back across the stop
  // `rail` is the LIVE rail, 224–420px and 0 while the sidebar is away — not the mockup's 324px
  // (§11 row 1). At 1440px with the default 300px rail: the stop is 1440 − 300 − 360 = 780px and
  // open completely is 1140px; with the sidebar away, 1080px and the whole 1440px.
  //
  // Only where the panel docks (1180px and wider, §6.1). Below that it overlays at its split
  // width and there is no stop to feel, so there is no divider either.
  //
  // `full` is never persisted; the split width is, through nav.rs `set_output_width`, which
  // floors it at 320px and leaves the ceiling to the stop measured here (§9.5).

  /// §9.1: Iris's reading of his Codex frames. A tunable constant, his to retune (§13).
  const STAGE_MIN = 360;
  /// §9.3: his "keep dragging" five times, tuned on a trackpad. Tunable, his to retune (§13).
  const SNAP_PAST = 120;
  const PANEL_MIN = 320;
  const PANEL_DEFAULT = 400;
  /// §9.2: under this much stage the conversation reflows to its narrowest.
  const NARROW_STAGE = 520;
  const KEY_STEP = 24;
  /// The 0.38 s curve with margin: how long the snap takes, and so how long the divider lets go.
  const SNAP_MS = 420;
  const PERSIST_AFTER_MS = 150;
  /// What the one polite region says when the pull changes the layout (round 17.1's notices).
  const SAID_FULL = "Open completely. The conversation is one click back and closing the panel brings it all back.";
  const SAID_BACK = "The conversation is back.";

  const pull = {
    /// The split width, remembered apart from `full` (§9.5) and written to nav.rs.
    split: PANEL_DEFAULT,
    full: false,
    dragging: false,
    settling: false,
    settleTimer: 0,
    persistTimer: 0,
    lastRail: null,
  };

  /// The rail's width as the layout will have it: 0 the moment the sidebar is told to go, not
  /// when its slide ends, so the panel moves on the same curve as the rail instead of after it.
  function pullRail() {
    const rail = el("rail");
    if (!rail) return 0;
    const away =
      document.body.classList.contains("rail-closed") || document.documentElement.getAttribute("data-sidebar") === "hidden";
    return away ? 0 : rail.getBoundingClientRect().width;
  }

  function pullApp() {
    const app = el("app");
    return app ? app.clientWidth : window.innerWidth;
  }

  function pullMax() {
    return Math.max(PANEL_MIN, Math.floor(pullApp() - pullRail() - STAGE_MIN));
  }

  function pullFullWidth() {
    return Math.max(PANEL_MIN, pullApp() - pullRail());
  }

  function pullClamp(w) {
    return Number.isFinite(w) ? Math.max(PANEL_MIN, Math.round(w)) : PANEL_DEFAULT;
  }

  /// What the panel is painted at: open completely, or the split width held at the stop. Below
  /// 1180px the split width itself, which the stylesheet caps to the window (§6.1).
  function pullWidth() {
    if (!isWide()) return pullClamp(pull.split);
    return pull.full ? pullFullWidth() : Math.min(pullMax(), pullClamp(pull.split));
  }

  /// No slide and no fade for the next two frames: on the panel and on the conversation.
  function pullInstant() {
    document.body.classList.add("pull-instant");
    window.requestAnimationFrame(() => window.requestAnimationFrame(() => document.body.classList.remove("pull-instant")));
  }

  /// Paint the width, `full` and the divider's value. `instant` for a window resize, a launch or
  /// an open; a drag is instant through `is-resizing`; everything else takes the rail's curve.
  function pullPaint(opts) {
    opts = opts || {};
    const panel = el("outpanel");
    const rz = el("op-resizer");
    if (!panel || !rz) return;
    if (opts.instant) pullInstant();
    const w = Math.round(pullWidth());
    panel.style.setProperty("--output-width", w + "px");
    document.body.classList.toggle("panel-full", pull.full);
    const max = pullMax();
    rz.setAttribute("aria-valuemin", String(PANEL_MIN));
    rz.setAttribute("aria-valuemax", String(max));
    rz.setAttribute("aria-valuenow", String(w));
    rz.setAttribute(
      "aria-valuetext",
      pull.full ? "Open completely" : w >= max ? w + " pixels, at the stop; pull on to open it completely" : w + " pixels"
    );
    pullPaintPill();
  }

  /// ‹ and the thread's own name at the head of the panel, only while it is open completely.
  function pullPaintPill() {
    const pill = el("op-conv");
    if (!pill) return;
    const title = state.thread && ctx.threadTitle ? ctx.threadTitle(state.thread) : "";
    pill.hidden = !pull.full;
    el("op-conv-t").textContent = title;
    pill.setAttribute("aria-label", "Show the conversation" + (title ? ": " + title : ""));
  }

  /// The overshoot past the stop, 0 to 1: the conversation dims and the spine thickens with it.
  function pullArm(p) {
    p = Math.max(0, Math.min(1, p));
    document.body.classList.toggle("snap-arming", p > 0);
    document.body.style.setProperty("--snap", String(p));
  }

  /// The snap animates even mid-drag: the divider lets go of the pointer while it does.
  function pullSettle() {
    const panel = el("outpanel");
    pull.settling = true;
    panel.classList.remove("is-resizing");
    window.clearTimeout(pull.settleTimer);
    pull.settleTimer = window.setTimeout(() => {
      pull.settling = false;
      if (pull.dragging) panel.classList.add("is-resizing");
    }, SNAP_MS);
  }

  /// Open completely, or back to the split. `opts.width` sets the split width it returns to
  /// (the stop, from the pill, the pull back and →). The composer goes with it (§9.4): one node,
  /// moved into the panel and back by main.js, so one draft, one tray and one send path.
  function pullSetFull(on, opts) {
    opts = opts || {};
    if (on && (!isWide() || !state.open)) return;
    if (opts.width != null) pull.split = pullClamp(opts.width);
    const changed = on !== pull.full;
    pull.full = on;
    // The class first, so the composer lands in a home that is laid out: the field measures
    // its height in its new width (measured into `display: none`, it came out one line short).
    document.body.classList.toggle("panel-full", on);
    if (changed && ctx.moveComposer) ctx.moveComposer(on ? el("op-float") : null);
    pullPaint({ instant: !!opts.instant });
    if (!changed) return;
    if (!opts.instant) pullSettle();
    if (!opts.quiet && ctx.announce) ctx.announce(on ? SAID_FULL : SAID_BACK);
  }

  /// A split width, from the keys, a double-click or Home. From open completely it is the
  /// return (the mockup's `setWidth` clears `full`).
  function pullSetWidth(w) {
    if (pull.full) return pullSetFull(false, { width: Math.min(pullMax(), pullClamp(w)) });
    pull.split = Math.min(pullMax(), pullClamp(w));
    pullPaint();
  }

  /// Write the split width; render what the store accepted (nav.rs floors it at 320px).
  function pullPersist() {
    window.clearTimeout(pull.persistTimer);
    pull.persistTimer = window.setTimeout(async () => {
      if (!bridge) return;
      let accepted = null;
      try {
        accepted = await bridge.invoke("set_output_width", { width: pull.split });
      } catch (_e) {
        return; // an unreadable navigation file is left as it is (nav.rs); the width still shows
      }
      if (typeof accepted === "number" && accepted !== pull.split) {
        pull.split = accepted;
        pullPaint();
      }
    }, PERSIST_AFTER_MS);
  }

  function pullSetSplit(w) {
    pull.split = pullClamp(Number(w));
    pullPaint({ instant: true });
  }

  /// The drag (`output.html:1360-1389`), on pointer capture so it holds outside the strip.
  function pullPointerDown(e) {
    if (!isWide() || e.button !== 0) return;
    e.preventDefault();
    const rz = el("op-resizer");
    const panel = el("outpanel");
    rz.setPointerCapture(e.pointerId);
    panel.classList.remove("is-opening");
    panel.classList.add("is-resizing");
    pull.dragging = true;
    const right = el("app").getBoundingClientRect().right;
    // Where the pointer was when the panel went open completely: SNAP_PAST back from there
    // returns the conversation.
    let anchorX = e.clientX;
    // After a return the pointer is behind the divider; nothing arms again until it has
    // crossed back over the stop.
    let armable = !pull.full;
    const move = (ev) => {
      if (pull.settling) return;
      const want = right - ev.clientX;
      const max = pullMax();
      if (pull.full) {
        if (ev.clientX - anchorX >= SNAP_PAST) {
          armable = false;
          pullSetFull(false, { width: max });
        }
        return;
      }
      if (want > max) {
        // At the stop the divider holds: that hold is the stop he feels.
        pull.split = max;
        pullPaint();
        if (!armable) return;
        const over = want - max;
        pullArm(over / SNAP_PAST);
        if (over >= SNAP_PAST) {
          pullArm(0);
          anchorX = ev.clientX;
          pullSetFull(true);
        }
      } else {
        armable = true;
        pullArm(0);
        pull.split = Math.max(PANEL_MIN, Math.round(want));
        pullPaint();
      }
    };
    const up = () => {
      pullArm(0);
      pull.dragging = false;
      panel.classList.remove("is-resizing");
      rz.removeEventListener("pointermove", move);
      rz.removeEventListener("pointerup", up);
      rz.removeEventListener("pointercancel", up);
      pullPersist();
    };
    rz.addEventListener("pointermove", move);
    rz.addEventListener("pointerup", up);
    rz.addEventListener("pointercancel", up);
  }

  /// The keys (`output.html:1390-1396`): ← at the stop snaps, End snaps from anywhere, → from
  /// open completely returns to the stop, Home goes back to the default 400px.
  function pullKey(e) {
    if (!isWide()) return;
    const max = pullMax();
    const now = pullWidth();
    if (e.key === "ArrowLeft") {
      if (!pull.full) {
        if (now >= max) pullSetFull(true);
        else pullSetWidth(now + KEY_STEP);
      }
    } else if (e.key === "ArrowRight") {
      if (pull.full) pullSetFull(false, { width: max });
      else pullSetWidth(now - KEY_STEP);
    } else if (e.key === "End") {
      pullSetFull(true);
    } else if (e.key === "Home") {
      pullSetWidth(PANEL_DEFAULT);
    } else {
      return;
    }
    e.preventDefault();
    pullPersist();
  }

  /// Closing clears `full` and brings the composer home (§9.4); the split width stays. The
  /// panel is gone at once, so the whole conversation is back at once too, not faded in.
  function pullReset() {
    pullArm(0);
    // The conversation gets its width back BEFORE the composer goes home — the class off and the
    // panel painted at its split width, at once — so the field measures itself in a laid-out
    // conversation: moved into a zero-width one, the empty field came home 111px tall (the
    // real-app walk's close picture, 2026-10-05). The panel hides right after this.
    const wasFull = pull.full;
    pull.full = false;
    document.body.classList.remove("panel-full");
    if (wasFull) {
      pullPaint({ instant: true });
      if (ctx.moveComposer) ctx.moveComposer(null);
    }
    pullPaintPill();
  }

  /// A window crossing under 1180px while open completely: the conversation comes back.
  function pullBreakpoint() {
    if (pull.full && !isWide()) pullSetFull(false, { instant: true, quiet: true });
  }

  /// Escape in the floating composer clears its words before it steps the panel back (§6.8,
  /// `output.html:1436`). The words go through the field's own `input` path, so the parked
  /// draft and the composer's buttons follow them.
  function pullEscape() {
    if (!pull.full) return false;
    const field = document.activeElement;
    const float = el("op-float");
    if (!field || field.tagName !== "TEXTAREA" || !float || !float.contains(field) || !field.value) return false;
    field.value = "";
    field.dispatchEvent(new Event("input", { bubbles: true }));
    return true;
  }

  function pullInit() {
    const rz = el("op-resizer");
    const panel = el("outpanel");
    if (!rz || !panel) return;
    rz.addEventListener("pointerdown", pullPointerDown);
    rz.addEventListener("keydown", pullKey);
    rz.addEventListener("dblclick", () => {
      pullSetWidth(PANEL_DEFAULT);
      pullPersist();
    });
    // The conversation one click back, at the stop; focus goes to its composer once it is home.
    el("op-conv").addEventListener("click", () => {
      pullSetFull(false, { width: pullMax() });
      window.setTimeout(() => {
        const input = el("input");
        if (input && input.getClientRects().length) input.focus({ preventScroll: true });
      }, SNAP_MS);
    });
    // The opening slide is an animation on `width`; once it has run, the width is the panel's
    // own again, so a drag or a snap is never held under an animation's fill.
    panel.addEventListener("animationend", (e) => {
      if (e.animationName === "op-open") panel.classList.remove("is-opening");
    });
    if (typeof ResizeObserver === "function") {
      // §9.2: the conversation's own width decides its reflow, not the window's breakpoints.
      const stage = el("stage");
      if (stage) {
        new ResizeObserver((entries) => {
          const w = entries[0].contentRect.width;
          stage.classList.toggle("narrow", w > 0 && w < NARROW_STAGE);
        }).observe(stage);
      }
      // §9.1: the stop follows the window, and the rail's own divider.
      const follow = () => {
        if (state.open) pullPaint({ instant: true });
      };
      if (el("app")) new ResizeObserver(follow).observe(el("app"));
      if (el("rail")) new ResizeObserver(follow).observe(el("rail"));
      // The floating composer's height, so the end of the list is never under it.
      const float = el("op-float");
      if (float) {
        new ResizeObserver(() => {
          panel.style.setProperty("--op-float-h", Math.ceil(float.getBoundingClientRect().height) + "px");
        }).observe(float);
      }
    }
    // §9.1: and the sidebar. Toggling it changes the stop at once; the panel takes the rail's
    // curve (`.38s`) unless the sidebar was applied rather than chosen (`rail-instant`).
    new MutationObserver(() => {
      const r = pullRail();
      if (r === pull.lastRail) return;
      pull.lastRail = r;
      if (state.open) pullPaint({ instant: document.body.classList.contains("rail-instant") });
    }).observe(document.body, { attributes: true, attributeFilter: ["class"] });
    // Nothing is painted here: `init` runs while main.js is still evaluating, before the
    // breakpoints `isWide` reads exist. The first paint is `setSplitWidth` at launch, or `open`.
    pull.lastRail = pullRail();
  }

  // ==========================================================================================
  // THE ACTIONS — slice S6 of the Output side panel PRD (§5.4, §6.3, §6.4, §6.7, §6.8, §12.6)
  // ==========================================================================================
  //
  // Open in <app>, Open with…, Show in Finder, Save a copy…, Copy path, and Preview from a row's
  // menu. Their entrances: a row's hover `Open` and `⋯`, a right-click on the row (or the
  // context-menu key and Shift+F10 on a focused row), the file view's gold *Open in <app>* with
  // its `▾` and its `⋯`, and the path line's *Copy the full path*. EVERY ACTION SAYS WHAT IT
  // DID in the panel's notice (`#op-notice`, `role="status"`): the shell's own sentence for the
  // four commands (*Opening brief.md in Obsidian.*, *Finder opens acme/counter/ with brief.md
  // selected.*, *Saved a copy of brief.md to you/Desktop/.*, *Nothing was saved.*), *Copied
  // the path.* for the one with no command. A refused action says the shell's sentence there
  // and nothing else changes (§6.7).
  //
  // THE PAGE NEVER NAMES A PATH (§5.1). Open, Open with, Show in Finder and Save a copy… send
  // the output id; Open with sends the place of the app in the list the shell itself answered
  // (`output_file`), which the shell recomputes and compares. When that list changed, the
  // refusal says *Choose one again.* and the list is shown again, fresh, at the same control.
  // Copy path has no command: the recorded path goes to the clipboard from his gesture (§5.4).
  //
  // WHAT IS LIT (§6.7), from the shell's `output_file.problem`, never a matched sentence: a file
  // no longer where it was written keeps Copy path, and Preview (its own view says where it is
  // not); a link or a swapped file keeps Show in Finder; everything else is disabled with the
  // reason as its tooltip — `aria-disabled`, not `disabled`, so it stays focusable and the
  // tooltip shows.
  //
  // ADD TO CHAT (S7) joins through `hooks.menuItems(entry, detail) -> [{ id, label, icon, act,
  // disabled }]`, shown after a separator in the row's `⋯`, its right-click and the file view's
  // `⋯`; its section follows the entrances below.

  /// VERBATIM from `APPS_CHANGED` and `APP_NOT_OFFERED` in `src-tauri/src/output_files.rs`:
  /// the two refusals after which the app list is shown again.
  const APPS_CHANGED = "The apps that open this file changed since the list was shown. Choose one again.";
  const APP_NOT_OFFERED = "That app is not one this Mac offers for this file. Choose one again.";
  const COPIED = "Copied the path.";
  const COPY_FAILED = "I couldn't copy the path.";

  const ACT_SHAPES = {
    open: [["path", { d: "M14 5h5v5" }], ["path", { d: "M19 5l-9 9" }], ["path", { d: "M19 14v5a1 1 0 0 1-1 1H6a1 1 0 0 1-1-1V6a1 1 0 0 1 1-1h5" }]],
    more: [5, 12, 19].map((cx) => ["circle", { cx, cy: 12, r: 1.6, fill: "currentColor", stroke: "none" }]),
    eye: [["path", { d: "M2.5 12s3.5-6.5 9.5-6.5S21.5 12 21.5 12s-3.5 6.5-9.5 6.5S2.5 12 2.5 12Z" }], ["circle", { cx: 12, cy: 12, r: 3 }]],
    down: [["path", { d: "m6 9 6 6 6-6" }]],
    right: [["path", { d: "m9 18 6-6-6-6" }]],
    copy: [["rect", { x: 9, y: 9, width: 11, height: 11, rx: 2 }], ["path", { d: "M5 15V5a2 2 0 0 1 2-2h10" }]],
    folder: [["path", { d: "M3 7a2 2 0 0 1 2-2h4l2 2h8a2 2 0 0 1 2 2v9a2 2 0 0 1-2 2H5a2 2 0 0 1-2-2z" }]],
    save: [["path", { d: "M12 4v11" }], ["path", { d: "m7 10 5 5 5-5" }], ["path", { d: "M5 20h14" }]],
    apps: [[3, 3], [14, 3], [3, 14], [14, 14]].map(([x, y]) => ["rect", { x, y, width: 7, height: 7, rx: 1.5 }]),
    // Round 17's `I.clip`, for Add to chat.
    clip: [["path", { d: "m21 12-8.5 8.5a5 5 0 0 1-7-7L14 5a3.3 3.3 0 0 1 4.7 4.7L10.3 18a1.6 1.6 0 0 1-2.3-2.3L16 7.7" }]],
  };

  function actIcon(name) {
    const svg = icon("file");
    svg.textContent = "";
    for (const [tag, attrs] of ACT_SHAPES[name] || []) {
      const shape = document.createElementNS(SVG_NS, tag);
      for (const k of Object.keys(attrs)) shape.setAttribute(k, String(attrs[k]));
      svg.appendChild(shape);
    }
    return svg;
  }

  const act = {
    /// output id -> Promise of `output_file`'s answer; emptied whenever the list is read again.
    details: new Map(),
    detailsRev: -1,
    /// The open menu: { el, items, opts, opener, row, parent }, or null.
    menu: null,
    /// `id:exists`, the last difference between the list and the shell's answer that was read
    /// again (`reconcile`), until the two agree.
    reconciled: null,
    /// A save sheet is open: a second *Save a copy…* waits for it rather than stacking sheets.
    saving: false,
    noticeTimer: 0,
    noticeHide: 0,
  };

  function said(e) {
    return String(e && e.message ? e.message : e);
  }

  /// What the shell says about one file: its apps and whether anything may be done with it.
  function detailOf(id, fresh) {
    if (act.detailsRev !== state.rev) {
      act.details.clear();
      act.detailsRev = state.rev;
    }
    if (fresh) act.details.delete(id);
    let p = act.details.get(id);
    if (!p) {
      p = bridge.invoke("output_file", { outputId: id }).then(
        (d) => {
          d = d || {};
          // The shell re-stated the file just now (§5.4). Gone, or back, against what the list
          // holds: the list is read again, so its row and its view say what is true (D6).
          if (typeof d.exists === "boolean") reconcile(id, d.exists);
          return d;
        },
        (e) => ({ error: said(e) })
      );
      act.details.set(id, p);
    }
    return p;
  }

  /// One file's presence as the shell just found it, against the list on screen. A difference
  /// reads the list again — once per difference: should `list_output` and `output_file` keep
  /// disagreeing, the panel does not loop through reads; once they agree, the next difference
  /// is read again.
  function reconcile(id, exists) {
    const f = entry(id);
    if (!f) return;
    const claim = id + ":" + exists;
    if (!!f.exists === exists) {
      if (act.reconciled === claim) act.reconciled = null;
      return;
    }
    if (act.reconciled === claim) return;
    act.reconciled = claim;
    load();
  }

  /// For each action, null when it is lit, else the reason shown as its tooltip (§6.7).
  function blockers(f, d) {
    const problem = !f.exists ? "missing" : d && d.problem;
    const why = (d && d.reason) || MISSING_SENTENCE;
    if (problem === "missing") return { open: MISSING_SENTENCE, reveal: MISSING_SENTENCE, save: MISSING_SENTENCE, copy: null };
    if (problem === "refused") return { open: why, reveal: null, save: why, copy: why };
    if (problem === "readFailed") return { open: why, reveal: why, save: why, copy: null };
    return { open: null, reveal: null, save: null, copy: null };
  }

  function defaultApp(d) {
    return d && d.defaultApp && d.defaultApp.name ? d.defaultApp.name : null;
  }

  /// *Open in <app>*, or *Open* when Launch Services named no default (§5.4's degraded mode).
  function openLabel(d) {
    return defaultApp(d) ? "Open in " + defaultApp(d) : "Open";
  }

  function otherApps(d) {
    return d && Array.isArray(d.otherApps) ? d.otherApps : [];
  }

  function setDisabled(control, reason) {
    if (reason) control.setAttribute("aria-disabled", "true");
    else control.removeAttribute("aria-disabled");
  }

  // ---- the notice: what an action did ------------------------------------------------------

  /// Made empty the first time the panel shows a file, so the polite status region exists
  /// before the first sentence is put in it — a region born with its words is not announced.
  function ensureNotice() {
    let n = el("op-notice");
    if (n) return n;
    n = node("div", "op-notice");
    n.id = "op-notice";
    n.setAttribute("role", "status");
    n.setAttribute("aria-live", "polite");
    n.setAttribute("aria-atomic", "true");
    n.hidden = true;
    el("outpanel").appendChild(n);
    return n;
  }

  function notice(text) {
    const n = ensureNotice();
    window.clearTimeout(act.noticeTimer);
    window.clearTimeout(act.noticeHide);
    n.textContent = text;
    n.classList.remove("leaving");
    n.hidden = false;
    n.style.animation = "none";
    void n.offsetWidth;
    n.style.animation = "";
    // Long enough to read: the shell's longest sentence (the missing file's, 139 characters)
    // stays 7.8 s; a short one 3.4 s, the mockup's.
    const ms = Math.max(3400, 1500 + text.length * 45);
    act.noticeTimer = window.setTimeout(() => {
      n.classList.add("leaving");
      act.noticeHide = window.setTimeout(() => {
        n.hidden = true;
        n.classList.remove("leaving");
      }, 400);
    }, ms);
  }

  // ---- the actions themselves ----------------------------------------------------------------

  /// One shell command; its sentence in the notice either way. A file found gone reads the
  /// list again, so its row dims where he is looking.
  async function perform(cmd, args) {
    try {
      notice(String(await bridge.invoke(cmd, args)));
      return { ok: true };
    } catch (e) {
      const s = said(e);
      notice(s);
      if (s === MISSING_SENTENCE) load();
      return { ok: false, said: s };
    }
  }

  function openFileInApp(f, appIndex) {
    const args = { outputId: f.id };
    if (typeof appIndex === "number") args.appIndex = appIndex;
    return perform("output_open", args);
  }

  async function openWith(f, index, from) {
    const r = await openFileInApp(f, index);
    if (r.ok || (r.said !== APPS_CHANGED && r.said !== APP_NOT_OFFERED)) return;
    // *Choose one again.* — the list as the Mac gives it now, at the control he used.
    const d = await detailOf(f.id, true);
    if (from && from.isConnected) showMenu(appMenuItems(f, d, from), { opener: from, title: "Open " + f.name + " with", kind: "open-with" });
  }

  function reveal(f) {
    return perform("output_reveal", { outputId: f.id });
  }

  async function saveCopy(f) {
    if (act.saving) return;
    act.saving = true;
    try {
      await perform("output_save_copy", { outputId: f.id });
    } finally {
      act.saving = false;
    }
  }

  /// The recorded path, from his gesture (§5.4: no command).
  function copyPath(f) {
    const failed = () => notice(COPY_FAILED);
    try {
      const clip = navigator.clipboard;
      if (!clip || typeof clip.writeText !== "function") return failed();
      clip.writeText(f.path).then(() => notice(COPIED), failed);
    } catch (_e) {
      failed();
    }
  }

  // ---- the menus (§6.8: role="menu", arrow keys, Escape back to the opener) -------------------

  /// The row's `⋯` and right-click (`withPreview`), and the file view's `⋯`: round 17's set.
  function fileMenuItems(f, d, withPreview) {
    const b = blockers(f, d);
    const items = [];
    if (withPreview) items.push({ id: "preview", icon: "eye", label: "Preview", act: () => showFile(f.id) });
    items.push({ id: "open", icon: "open", label: openLabel(d), disabled: b.open, act: () => openFileInApp(f) });
    // §5.4's degraded mode: no app list, no *Open with…*.
    if (otherApps(d).length) {
      items.push({ id: "open-with", icon: "apps", label: "Open with…", disabled: b.open, sub: (from) => openSubmenu(f, d, from) });
    }
    items.push({ sep: true });
    items.push({ id: "reveal", icon: "folder", label: "Show in Finder", disabled: b.reveal, act: () => reveal(f) });
    items.push({ id: "save", icon: "save", label: "Save a copy…", disabled: b.save, act: () => saveCopy(f) });
    items.push({ id: "copy", icon: "copy", label: "Copy path", disabled: b.copy, act: () => copyPath(f) });
    const extra = typeof hooks.menuItems === "function" ? hooks.menuItems(f, d) || [] : [];
    if (extra.length) items.push({ sep: true }, ...extra);
    return items;
  }

  /// The `▾` beside Open: the default app first, the others under it, then Finder and Save.
  function openMenuItems(f, d, from) {
    const b = blockers(f, d);
    const items = [{ id: "open", icon: "open", label: openLabel(d), disabled: b.open, act: () => openFileInApp(f) }];
    otherApps(d).forEach((app, i) => {
      items.push({ id: "app-" + i, icon: "apps", label: "Open in " + app.name, disabled: b.open, act: () => openWith(f, i, from) });
    });
    items.push({ sep: true });
    items.push({ id: "reveal", icon: "folder", label: "Show in Finder", disabled: b.reveal, act: () => reveal(f) });
    items.push({ id: "save", icon: "save", label: "Save a copy…", disabled: b.save, act: () => saveCopy(f) });
    return items;
  }

  /// *Open with…*'s list: every other app, by the place the shell gave it.
  function appMenuItems(f, d, from) {
    const b = blockers(f, d);
    return otherApps(d).map((app, i) => ({ id: "app-" + i, icon: "apps", label: app.name, disabled: b.open, act: () => openWith(f, i, from) }));
  }

  function openSubmenu(f, d, item) {
    const parent = act.menu;
    if (!parent) return;
    showMenu(appMenuItems(f, d, parent.opener || parent.row), {
      opener: parent.opener,
      row: parent.row,
      at: parent.opts.at,
      title: "Open " + f.name + " with",
      kind: "open-with",
      parent: { items: parent.items, opts: parent.opts, focusId: item.dataset.item },
    });
  }

  /// Close the open menu. `step`: a submenu goes back to its menu instead (Escape, ←). Focus
  /// returns to the control that opened it unless `focus: false`. True when a menu was open.
  function closeMenu(opts) {
    opts = opts || {};
    const m = act.menu;
    if (!m) return false;
    if (opts.step && m.parent) {
      showMenu(m.parent.items, Object.assign({}, m.parent.opts, { focusId: m.parent.focusId }));
      return true;
    }
    m.el.remove();
    act.menu = null;
    if (m.opener) m.opener.setAttribute("aria-expanded", "false");
    if (m.row) m.row.classList.remove("menu-open");
    if (opts.focus !== false) {
      const back = [m.opener, m.row].find((n) => n && n.isConnected && n.getClientRects().length);
      if (back) back.focus({ preventScroll: true });
    }
    return true;
  }

  function showMenu(items, opts) {
    closeMenu({ focus: false });
    const m = node("div", "op-menu");
    m.id = "op-menu";
    m.setAttribute("role", "menu");
    m.dataset.kind = opts.kind || "";
    if (opts.title) {
      m.setAttribute("aria-label", opts.title);
      const head = node("div", "op-menu-head", opts.title);
      head.setAttribute("aria-hidden", "true");
      m.appendChild(head);
    }
    for (const it of items) {
      if (it.sep) {
        const sep = node("div", "op-menu-sep");
        sep.setAttribute("role", "separator");
        m.appendChild(sep);
        continue;
      }
      const b = node("button", "op-menu-item");
      b.type = "button";
      b.tabIndex = -1;
      b.setAttribute("role", "menuitem");
      b.dataset.item = it.id;
      const label = node("span", "mi");
      if (it.icon) label.appendChild(actIcon(it.icon));
      label.appendChild(node("span", null, it.label));
      b.appendChild(label);
      if (it.sub) {
        b.setAttribute("aria-haspopup", "menu");
        const k = node("span", "k");
        k.appendChild(actIcon("right"));
        b.appendChild(k);
      }
      setDisabled(b, it.disabled);
      if (it.disabled) b.title = it.disabled;
      b.addEventListener("click", (e) => {
        e.stopPropagation();
        activate(it, b);
      });
      m.appendChild(b);
    }
    m.addEventListener("keydown", menuKeys);
    el("outpanel").appendChild(m);
    act.menu = { el: m, items, opts, opener: opts.opener || null, row: opts.row || null, parent: opts.parent || null };
    place(m, opts);
    if (opts.opener) opts.opener.setAttribute("aria-expanded", "true");
    if (opts.row) opts.row.classList.add("menu-open");
    const list = [...m.querySelectorAll(".op-menu-item")];
    const first =
      list.find((b) => b.dataset.item === opts.focusId) ||
      list.find((b) => b.getAttribute("aria-disabled") !== "true") ||
      list[0];
    if (first) first.focus({ preventScroll: true });
  }

  /// Under its opener, right-aligned to it, or at the pointer; never off the window.
  function place(m, opts) {
    const w = m.offsetWidth;
    const h = m.offsetHeight;
    let x;
    let y;
    if (opts.at) {
      x = opts.at.x;
      y = opts.at.y;
    } else if (opts.opener) {
      const r = opts.opener.getBoundingClientRect();
      x = r.right - w;
      y = r.bottom + 6;
      if (y + h > window.innerHeight - 8) y = r.top - h - 6;
    } else {
      x = 8;
      y = 8;
    }
    m.style.left = Math.max(8, Math.min(x, window.innerWidth - w - 8)) + "px";
    m.style.top = Math.max(8, Math.min(y, window.innerHeight - h - 8)) + "px";
  }

  function activate(it, b) {
    if (it.disabled) return;
    if (it.sub) return it.sub(b);
    closeMenu();
    it.act();
  }

  function menuKeys(e) {
    const m = act.menu;
    if (!m || e.currentTarget !== m.el) return;
    const list = [...m.el.querySelectorAll(".op-menu-item")];
    const at = list.indexOf(document.activeElement);
    const go = (n) => {
      e.preventDefault();
      if (list.length) list[(n + list.length) % list.length].focus({ preventScroll: true });
    };
    const current = at >= 0 ? m.items.filter((it) => !it.sep)[at] : null;
    switch (e.key) {
      case "ArrowDown":
        return go(at + 1);
      case "ArrowUp":
        return go(at < 0 ? list.length - 1 : at - 1);
      case "Home":
        return go(0);
      case "End":
        return go(list.length - 1);
      case "ArrowRight":
        if (current && current.sub && !current.disabled) {
          e.preventDefault();
          current.sub(list[at]);
        }
        return;
      case "ArrowLeft":
        if (m.parent) {
          e.preventDefault();
          closeMenu({ step: true });
        }
        return;
      case "Escape":
        // Answered here, one level, and not again by the shell's Escape rule (main.js).
        e.preventDefault();
        e.stopPropagation();
        closeMenu({ step: true });
        return;
      case "Tab":
        e.preventDefault();
        closeMenu();
        return;
      default:
    }
  }

  async function openFileMenu(f, opts, withPreview) {
    const d = await detailOf(f.id);
    showMenu(fileMenuItems(f, d, withPreview), Object.assign({ title: f.name, kind: withPreview ? "row" : "file" }, opts));
  }

  function toggleFrom(opener, open) {
    if (act.menu && act.menu.opener === opener) return closeMenu();
    open();
  }

  // ---- the entrances --------------------------------------------------------------------------

  function paintRowOpen(f, button, d) {
    const reason = blockers(f, d).open;
    const app = defaultApp(d);
    button.setAttribute("aria-label", "Open " + f.name + (app ? " in " + app : ""));
    button.title = reason || openLabel(d);
    setDisabled(button, reason);
  }

  /// A row's hover `Open` and `⋯` (§6.3), its right-click, and the context-menu key. Returns the
  /// wrapper the list places instead of the row: the row and its actions side by side, laid out
  /// as round 17's one row (`.orow-wrap` in style.css), because a `role="button"` row's children
  /// are presentational — nested inside it, Open and `⋯` reach no assistive technology at all.
  hooks.rowActions = function (f, row) {
    ensureNotice();
    const wrap = node("div", "orow-wrap");
    const acts = node("span", "oacts");
    const open = node("button", "oact");
    open.type = "button";
    open.dataset.act = "open";
    open.appendChild(actIcon("open"));
    paintRowOpen(f, open, null);
    const more = node("button", "oact");
    more.type = "button";
    more.dataset.act = "menu";
    more.title = "More";
    more.setAttribute("aria-label", "More actions for " + f.name);
    more.setAttribute("aria-haspopup", "menu");
    more.setAttribute("aria-expanded", "false");
    more.appendChild(actIcon("more"));
    acts.appendChild(open);
    acts.appendChild(more);
    wrap.appendChild(row);
    wrap.appendChild(acts);
    // The app's name, once asked: on the first hover or focus, never for every row at once.
    const learn = () => detailOf(f.id).then((d) => open.isConnected && paintRowOpen(f, open, d));
    wrap.addEventListener("pointerenter", learn);
    wrap.addEventListener("focusin", learn);
    open.addEventListener("click", (e) => {
      e.stopPropagation();
      if (open.getAttribute("aria-disabled") !== "true") openFileInApp(f);
    });
    more.addEventListener("click", (e) => {
      e.stopPropagation();
      toggleFrom(more, () => openFileMenu(f, { opener: more, row }, true));
    });
    wrap.addEventListener("contextmenu", (e) => {
      e.preventDefault();
      openFileMenu(f, { row, at: { x: e.clientX, y: e.clientY } }, true);
    });
    row.addEventListener("keydown", (e) => {
      if (e.target !== row || !(e.key === "ContextMenu" || (e.shiftKey && e.key === "F10"))) return;
      e.preventDefault();
      const r = row.getBoundingClientRect();
      openFileMenu(f, { row, at: { x: r.left + 52, y: r.bottom } }, true);
    });
    return wrap;
  };

  /// The file view's tools (§6.4): the gold *Open in <app>* with its `▾`, and `⋯`.
  hooks.fileTools = function (f, box) {
    ensureNotice();
    const pill = node("span", "of-open");
    const main = node("button");
    main.type = "button";
    main.dataset.act = "open";
    main.appendChild(actIcon("open"));
    const word = node("span", null, "Open");
    main.appendChild(word);
    const down = node("button");
    down.type = "button";
    down.dataset.act = "open-menu";
    down.title = "Other ways to open";
    down.setAttribute("aria-label", "Other ways to open");
    down.setAttribute("aria-haspopup", "menu");
    down.setAttribute("aria-expanded", "false");
    down.appendChild(actIcon("down"));
    pill.appendChild(main);
    pill.appendChild(down);
    const more = node("button", "of-tool icon-only");
    more.type = "button";
    more.dataset.act = "menu";
    more.title = "More";
    more.setAttribute("aria-label", "More actions");
    more.setAttribute("aria-haspopup", "menu");
    more.setAttribute("aria-expanded", "false");
    more.appendChild(actIcon("more"));
    box.appendChild(pill);
    box.appendChild(more);
    const paint = (d) => {
      const b = blockers(f, d);
      word.textContent = openLabel(d);
      main.title = b.open || "";
      setDisabled(main, b.open);
      // The `▾` holds Open, Show in Finder and Save a copy…: lit while any of them is.
      const none = b.open && b.reveal && b.save;
      setDisabled(down, none);
      down.title = none || "Other ways to open";
      pill.classList.toggle("is-disabled", !!b.open);
    };
    paint(null);
    detailOf(f.id).then((d) => main.isConnected && paint(d));
    main.addEventListener("click", () => {
      if (main.getAttribute("aria-disabled") !== "true") openFileInApp(f);
    });
    down.addEventListener("click", () => {
      if (down.getAttribute("aria-disabled") === "true") return;
      toggleFrom(down, async () => {
        const d = await detailOf(f.id);
        showMenu(openMenuItems(f, d, down), { opener: down, kind: "open-menu" });
      });
    });
    more.addEventListener("click", () => toggleFrom(more, () => openFileMenu(f, { opener: more }, false)));
  };

  /// *Copy the full path*, at the end of the file view's path line (§6.4).
  hooks.pathTools = function (f, line) {
    const copy = node("button", "oact of-copy");
    copy.type = "button";
    copy.dataset.act = "copy";
    copy.setAttribute("aria-label", "Copy the full path");
    copy.appendChild(actIcon("copy"));
    const paint = (d) => {
      const reason = blockers(f, d).copy;
      copy.title = reason || "Copy the full path";
      setDisabled(copy, reason);
    };
    paint(null);
    detailOf(f.id).then((d) => copy.isConnected && paint(d));
    copy.addEventListener("click", () => {
      if (copy.getAttribute("aria-disabled") !== "true") copyPath(f);
    });
    line.appendChild(copy);
  };

  // ---- Add to chat (slice S7, PRD §12.7), the last item of every menu above ------------------
  //
  // A file from the panel goes into the next message through the attachment desk:
  // `RichAttachments.addRecorded` stages it by its OUTPUT ID (`output_attach` in
  // `mac_attachments.rs`), so the desk's limits and its sentences apply unchanged, and a refusal
  // is said on the tray's line under the composer exactly as a dropped file's is. Then focus goes
  // to the composer, as round 17's `addToChat` does, so his words follow the file. Below 1180px
  // the panel lies over the composer (§6.1), so it steps aside first and the tray is seen.
  //
  // WHERE IT IS REACHED: round 17 puts *Add to chat* last, after a rule, in the row's `⋯`, the
  // file view's `⋯` and the right-click (`output.html` `menuItems`) — `fileMenuItems` above,
  // through `hooks.menuItems`. One list builder, so each menu exists once.
  //
  // WHAT IT SAYS: *brief.md is attached to your next message.*, in the panel's on-screen notice
  // (`#op-notice`, S6's, a live region itself, beside every other action's sentence), and ONLY
  // there while the panel is open; below 1180px the panel steps aside, its notice is off screen
  // and silent, so the conversation's live region (`ctx.announce`) says it instead. Never both.
  const ADD_TO_CHAT = "Add to chat";

  /// The menu item, in S6's shape. Attaching reads the file's bytes, as Save a copy… does, so it
  /// is lit exactly when Save a copy… is: a file no longer where it was written (§4.6), a link or
  /// a swapped file cannot be attached, and the reason is its tooltip, the same sentence as every
  /// other action the menu disables (§6.7). Should the file change after the menu was drawn,
  /// the desk still refuses it in its own words on the tray's line.
  function addToChatItem(f, d) {
    return { id: "add", icon: "clip", label: ADD_TO_CHAT, disabled: blockers(f, d).save, act: () => addToChat(f) };
  }

  async function addToChat(f) {
    if (!f || !f.exists || !window.RichAttachments) return;
    if (!isWide()) close({ keepFocus: true });
    const said = await window.RichAttachments.addRecorded(f);
    // A refusal is already said, and announced, by the tray's own `role="status"` line.
    // ONE announcement: the panel's notice is itself a polite live region, so while the panel is
    // open the sentence goes there only. Stepped aside below 1180px, the notice is off screen
    // with the panel and does not speak, so the conversation's live region carries it instead.
    if (said && said.ok && said.sentence) {
      if (isWide()) notice(said.sentence);
      else if (ctx.announce) ctx.announce(said.sentence);
    }
    const input = el("input");
    if (input) input.focus({ preventScroll: true });
  }

  hooks.menuItems = (f, d) => [addToChatItem(f, d)];

  // A menu closes when he presses anywhere else, scrolls the list under it, or resizes.
  document.addEventListener(
    "pointerdown",
    (e) => {
      const m = act.menu;
      if (!m || m.el.contains(e.target) || (m.opener && m.opener.contains(e.target))) return;
      closeMenu({ focus: false });
    },
    true
  );
  window.addEventListener("resize", () => closeMenu({ focus: false }));
  if (el("op-body")) el("op-body").addEventListener("scroll", () => closeMenu({ focus: false }), { passive: true });

  window.RichOutput = {
    init,
    setThread,
    open,
    close,
    toggle,
    escape,
    syncScrim,
    isOpen: () => state.open && !el("outpanel").hidden,
    links,
    hooks,
    reload: () => load(),
    /// The split width nav.rs remembered (§9.5), applied at launch by main.js.
    setSplitWidth: pullSetSplit,
    /// Read-only, for the acceptance suite: the wide pull's own state beside what it painted.
    pull: () => ({
      full: pull.full,
      settling: pull.settling,
      split: pull.split,
      painted: Math.round(el("outpanel").getBoundingClientRect().width),
      max: pullMax(),
      stageMin: STAGE_MIN,
      snapPast: SNAP_PAST,
    }),
    /// Read-only, for the acceptance suite: what the panel believes, beside what it painted.
    snapshot: () => ({
      thread: state.thread,
      open: state.open,
      view: state.view,
      file: state.file,
      count: state.list && !state.error ? state.list.count : null,
      error: state.error,
      rev: state.rev,
    }),
  };
})();

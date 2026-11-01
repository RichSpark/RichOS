// SCREENSHOTS AND FILES ON THE MAC COMPOSER — CEO ruling §86, 2026-09-24.
//
// His daily-driver app has to take what he hands Rich all day in the terminal: a screenshot, a
// PDF, a CEO input file (`richos-hq/docs/plans/2026-09-24-daily-driver-readiness.md` §4 X3). The
// phone already could; the Mac window could not. This file is the window's half: the tray above
// the composer, paste, drop, a file the thread produced (Add to chat from the Output panel,
// `addRecorded`), remove, and the refusal line. Everything about what a file IS —
// type, size, bytes, where it is kept and the words Rich reads — is the phone's attachment desk
// in Rust (`src-tauri/src/phone/attachments.rs`), reached through `src-tauri/src/mac_attachments.rs`.
// Nothing here decides whether a file is acceptable except to save a 25 MB round trip, and that
// check uses the desk's own sentence (`tests/attachments.js` joins the two).
//
// THE DESIGN REFERENCE is round 12.1's attachment append (`design/mockups/rounds/round-12.1/`,
// frozen, read and never edited): a tray grows above the text line, every item has its own
// remove control, a file is a chip with its name, type and size, and "what you type goes in the
// same bubble". The Mac composer's own tokens are used for every color; there is no new one.
//
// THE PLUS (round 50/51) opens macOS's own file picker (`attach_pick_files` in
// `mac_attachments.rs`); what he picks is shelved in Rust exactly like a drop and takes the
// drop's road from here (`addPicked` → `addDropped`), so the page never names a path either way.
//
// WHAT THIS DOES NOT DO, said rather than discovered: no folders (the desk takes files; a
// dropped folder is refused in its words, and the picker offers files only), no thumbnail for a
// DROPPED or PICKED image (the page never receives its bytes, by design — see
// the Rust module), and the tray does not survive a relaunch (the staged files do, and are swept
// after seven days by the desk).
"use strict";

(function () {
  /// The desk's limits, restated ONLY to refuse before a large paste crosses the bridge. The
  /// desk re-checks every one of them.
  const MAX_FILES = 10;
  const MAX_FILE_BYTES = 25 * 1024 * 1024;
  /// VERBATIM from `Origin::Mac` in `phone/attachments.rs` and the count limit beside it;
  /// `tests/attachments.js` asserts they are byte-identical.
  const TOO_LARGE = "This file is larger than 25 MB, the most RichOS takes in one file. Nothing was attached.";
  const TOO_MANY = "A message can carry at most 10 files.";
  const STILL_ADDING = "A file is still being added. Press Send again in a moment.";
  const DROP_HINT = "Drop to attach to your message";
  const GONE = " is no longer waiting to be sent. Attach it again; your words are still in the box.";

  let bridge = null;
  let composerKey = () => null;
  let onChange = () => {};
  let renderedKey = undefined;
  let dirty = true;
  let dragging = false;
  let nextSeq = 0;
  const trays = new Map();
  let noteLines = [];

  const el = (id) => document.getElementById(id);

  /// 32 hex characters: a valid desk id (`[A-Za-z0-9_-]{1,128}`) without depending on
  /// `crypto.randomUUID`, which a `file://` page (the browser harness) does not have.
  function freshId() {
    const bytes = new Uint8Array(16);
    crypto.getRandomValues(bytes);
    return Array.from(bytes, (b) => b.toString(16).padStart(2, "0")).join("");
  }

  function trayFor(key, create) {
    if (!key) return null;
    let tray = trays.get(key);
    if (!tray && create) {
      tray = { key, draftId: freshId(), items: [] };
      trays.set(key, tray);
    }
    return tray || null;
  }

  function changed() {
    dirty = true;
    render();
    onChange();
  }

  /// Sizes in the units his Finder shows, one decimal where it helps.
  function formatSize(bytes) {
    if (bytes < 1024) return bytes + " bytes";
    if (bytes < 1024 * 1024) return Math.round(bytes / 1024) + " KB";
    const mb = bytes / (1024 * 1024);
    return (mb < 10 ? mb.toFixed(1) : Math.round(mb)) + " MB";
  }

  function extensionOf(name) {
    const dot = name.lastIndexOf(".");
    return dot > 0 ? name.slice(dot + 1, dot + 5).toUpperCase() : "FILE";
  }

  function note(line) {
    noteLines.push(line);
    if (noteLines.length > 3) noteLines = noteLines.slice(-3);
    renderNote();
  }

  function clearNote() {
    if (!noteLines.length) return;
    noteLines = [];
    renderNote();
  }

  function renderNote() {
    const node = el("attach-note");
    if (!node) return;
    node.textContent = noteLines.join("\n");
    node.hidden = noteLines.length === 0;
  }

  function releaseThumb(item) {
    if (item.thumb) URL.revokeObjectURL(item.thumb);
    item.thumb = null;
  }

  // ---- the tray -------------------------------------------------------------------------

  function chip(item) {
    const li = document.createElement("li");
    li.className = "attach-chip" + (item.state === "adding" ? " is-adding" : "");
    li.dataset.seq = String(item.seq);
    // The chip's edge is a non-text indicator (3:1) and `tests/attachments.js` measures it as
    // one. It deliberately does NOT carry `data-contrast-role="indicator"`: the walk would then
    // hold the NAME and SIZE inside it to 3:1 as well, and they are text he reads (4.5:1).

    const tile = document.createElement("span");
    tile.className = "attach-tile";
    tile.setAttribute("aria-hidden", "true");
    if (item.thumb) {
      const img = document.createElement("img");
      img.src = item.thumb;
      img.alt = "";
      tile.appendChild(img);
    } else {
      tile.textContent = extensionOf(item.name);
    }
    li.appendChild(tile);

    const text = document.createElement("span");
    text.className = "attach-text";
    const name = document.createElement("span");
    name.className = "attach-name";
    name.textContent = item.name;
    name.title = item.name;
    const meta = document.createElement("span");
    meta.className = "attach-meta";
    meta.textContent = item.state === "adding" ? "Adding…" : (item.label || "File") + " · " + formatSize(item.size);
    text.appendChild(name);
    text.appendChild(meta);
    li.appendChild(text);

    const remove = document.createElement("button");
    remove.type = "button";
    remove.className = "attach-remove";
    remove.setAttribute("aria-label", "Remove " + item.name);
    remove.title = "Remove";
    remove.disabled = item.state === "adding";
    const glyph = document.createElement("span");
    glyph.setAttribute("aria-hidden", "true");
    glyph.textContent = "×";
    remove.appendChild(glyph);
    remove.addEventListener("click", () => removeItem(item.seq));
    li.appendChild(remove);
    return li;
  }

  function render() {
    const key = composerKey();
    if (!dirty && key === renderedKey) return;
    renderedKey = key;
    dirty = false;
    const zone = el("attach-tray");
    const list = el("attach-list");
    const hint = el("attach-drop");
    if (!zone || !list) return;
    const tray = trayFor(key, false);
    const items = tray ? tray.items : [];
    list.replaceChildren(...items.map(chip));
    list.hidden = items.length === 0;
    list.setAttribute("aria-label", items.length === 1 ? "1 file attached" : items.length + " files attached");
    if (hint) {
      hint.hidden = !(dragging && key);
      hint.textContent = hint.hidden ? "" : DROP_HINT;
    }
    zone.hidden = items.length === 0 && !(dragging && key);
  }

  function removeItem(seq) {
    const tray = trayFor(composerKey(), false);
    if (!tray) return;
    const at = tray.items.findIndex((i) => i.seq === seq);
    if (at < 0) return;
    const [item] = tray.items.splice(at, 1);
    releaseThumb(item);
    changed();
    // Keyboard focus stays in the tray rather than falling to the top of the document.
    const next = document.querySelector(".attach-chip .attach-remove:not([disabled])");
    (next || el("input")).focus();
    if (item.id) {
      bridge.invoke("discard_attachment", { draftId: tray.draftId, attachmentId: item.id }).catch(() => {
        // Not his problem: the file was never sent, and the desk's seven-day sweep takes it.
      });
    }
  }

  function place(tray, item) {
    if (tray.items.length >= MAX_FILES) {
      note(item.name + ": " + TOO_MANY);
      return false;
    }
    tray.items.push(item);
    changed();
    return true;
  }

  function settle(tray, item, answer) {
    item.id = answer.id;
    item.name = answer.name;
    item.label = answer.label;
    item.size = answer.size;
    item.sha256 = answer.sha256;
    item.state = "ready";
    if (!/^image\//.test(answer.mediaType)) releaseThumb(item);
    changed();
  }

  function fail(tray, item, reason) {
    const at = tray.items.indexOf(item);
    if (at >= 0) tray.items.splice(at, 1);
    releaseThumb(item);
    note(item.name + ": " + (typeof reason === "string" && reason.trim() ? reason.trim() : "RichOS couldn't add this file. Nothing was attached."));
    changed();
  }

  // ---- paste ---------------------------------------------------------------------------

  async function addPasted(files) {
    const tray = trayFor(composerKey(), true);
    if (!tray) return;
    clearNote();
    for (const file of files) {
      const name = file.name || "Pasted file";
      if (file.size > MAX_FILE_BYTES) {
        note(name + ": " + TOO_LARGE);
        continue;
      }
      const item = {
        seq: ++nextSeq,
        id: null,
        name,
        label: "",
        size: file.size,
        sha256: null,
        state: "adding",
        thumb: /^image\//.test(file.type) ? URL.createObjectURL(file) : null,
      };
      if (!place(tray, item)) {
        releaseThumb(item);
        continue;
      }
      try {
        const bytes = new Uint8Array(await file.arrayBuffer());
        const answer = await bridge.invoke("attach_pasted_file", bytes, {
          headers: {
            "x-richos-draft": tray.draftId,
            "x-richos-attachment": freshId(),
            "x-richos-name": encodeURIComponent(name),
            "x-richos-type": file.type || "",
          },
        });
        settle(tray, item, answer);
      } catch (e) {
        fail(tray, item, e);
      }
    }
  }

  function editableElsewhere(target) {
    if (!target || target.id === "input") return false;
    const tag = target.tagName;
    return tag === "INPUT" || tag === "TEXTAREA" || target.isContentEditable;
  }

  // The home screen covers the composer, and while it does the composer has no key. Leaving
  // it focuses the composer, which is the moment the tray must be drawn again.
  document.addEventListener("focusin", () => render());

  document.addEventListener("paste", (e) => {
    const files = e.clipboardData ? Array.from(e.clipboardData.files || []) : [];
    if (!files.length || editableElsewhere(e.target) || !composerKey()) return;
    // A copied file also carries its name as text; the file is what he meant.
    e.preventDefault();
    addPasted(files);
  });

  // ---- drop ----------------------------------------------------------------------------

  async function addDropped(payload) {
    const tray = trayFor(composerKey(), true);
    if (!tray || !payload || !Array.isArray(payload.files)) return;
    clearNote();
    for (const file of payload.files) {
      const item = { seq: ++nextSeq, id: null, name: file.name, label: "", size: 0, sha256: null, state: "adding", thumb: null };
      if (!place(tray, item)) continue;
      try {
        const answer = await bridge.invoke("attach_dropped_file", {
          draftId: tray.draftId,
          attachmentId: freshId(),
          drop: payload.drop,
          index: file.index,
        });
        settle(tray, item, answer);
      } catch (e) {
        fail(tray, item, e);
      }
    }
  }

  // ---- the plus: macOS's own file picker --------------------------------------------------

  /// The picker answers like a drop (`{ drop, files }`), or null when he cancels; a refusal to
  /// open it at all comes back as the shell's sentence, on the tray's line.
  async function addPicked() {
    if (!composerKey()) return;
    let view = null;
    try {
      view = await bridge.invoke("attach_pick_files");
    } catch (e) {
      note(String(e));
      return;
    }
    if (view && Array.isArray(view.files) && view.files.length) await addDropped(view);
  }

  // ---- a file the thread produced (Add to chat, Output side panel PRD §12.7) --------------

  /// The sentence the panel says when a file is on the tray (PRD §6.7, "Action succeeded").
  const ATTACHED = " is attached to your next message.";

  /// **A file from the Output panel goes on the tray of the composer on screen**, through the
  /// same desk as a drop (`output_attach` in `mac_attachments.rs`): the page names the file's
  /// OUTPUT ID, never a path, and the desk's refusal comes back in its own words on the tray's
  /// line, with the file's name, exactly as a dropped file's does. A file already on the tray
  /// is not added twice. Answers `{ ok, sentence }` — what the panel says it did — or a null
  /// sentence when no composer is on screen to take it.
  async function addRecorded(entry) {
    const tray = trayFor(composerKey(), true);
    if (!tray || !entry || !entry.id) return { ok: false, sentence: null };
    clearNote();
    const name = entry.name || "This file";
    if (tray.items.some((i) => i.outputId === entry.id)) return { ok: true, sentence: name + ATTACHED };
    const item = { seq: ++nextSeq, id: null, name, label: "", size: 0, sha256: null, state: "adding", thumb: null, outputId: entry.id };
    if (!place(tray, item)) return { ok: false, sentence: noteLines[noteLines.length - 1] || null };
    try {
      const answer = await bridge.invoke("output_attach", { draftId: tray.draftId, attachmentId: freshId(), outputId: entry.id });
      settle(tray, item, answer);
      return { ok: true, sentence: item.name + ATTACHED };
    } catch (e) {
      fail(tray, item, e);
      return { ok: false, sentence: noteLines[noteLines.length - 1] || null };
    }
  }

  function setDragging(on) {
    if (dragging === on) return;
    dragging = on;
    dirty = true;
    render();
    const zone = el("composer-zone");
    if (zone) zone.classList.toggle("is-drop-target", on && !!composerKey());
  }

  // ---- the surface main.js talks to ------------------------------------------------------

  window.RichAttachments = {
    /// `composerKey()` names what the composer is writing TO — a thread id, or the entity
    /// draft key — or null when no composer is on screen. `onChange()` lets the shell re-read
    /// whether Send has something to send.
    init(opts) {
      bridge = opts.bridge;
      composerKey = opts.composerKey;
      onChange = opts.onChange || (() => {});
      bridge.listen("rich://file-drag", ({ payload }) => setDragging(!!payload && payload.phase === "enter"));
      bridge.listen("rich://file-drop", ({ payload }) => {
        setDragging(false);
        if (composerKey()) addDropped(payload);
      });
      dirty = true;
      render();
    },
    /// Re-render for whatever the composer is now writing to. Cheap when nothing moved.
    sync() {
      render();
    },
    hasItems() {
      const tray = trayFor(composerKey(), false);
      return !!tray && tray.items.length > 0;
    },
    /// What Send needs: the tray for the composer on screen, or null when it holds nothing.
    forSend() {
      const tray = trayFor(composerKey(), false);
      if (!tray || !tray.items.length) return null;
      return {
        key: tray.key,
        draftId: tray.draftId,
        busy: tray.items.some((i) => i.state !== "ready"),
        names: tray.items.map((i) => i.name),
        attachments: tray.items.filter((i) => i.state === "ready").map((i) => ({ id: i.id, sha256: i.sha256 })),
      };
    },
    /// Send committed these files: the tray empties and the next message gets a new folder.
    sent(key) {
      const tray = trays.get(key);
      if (!tray) return;
      tray.items.forEach(releaseThumb);
      trays.delete(key);
      clearNote();
      changed();
    },
    /// The desk no longer holds these ids. They leave the tray, with a sentence each.
    forget(key, ids) {
      const tray = trays.get(key);
      if (!tray) return;
      const gone = tray.items.filter((i) => ids.includes(i.id));
      tray.items = tray.items.filter((i) => !ids.includes(i.id));
      gone.forEach((item) => {
        releaseThumb(item);
        note(item.name + GONE);
      });
      changed();
    },
    addRecorded,
    /// The plus: the file picker, then the drop's road.
    pick: addPicked,
    note,
    STILL_ADDING,
  };
})();

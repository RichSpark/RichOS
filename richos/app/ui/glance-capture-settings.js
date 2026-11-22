/* The rail page and Settings sheet control the same RichGlance preferences. */
(function () {
  "use strict";
  const settings = window.RichSettings;
  const bridge = window.RichBridge;
  if (!settings || !bridge) return;

  const page = document.getElementById("glance-page");
  const call = (command, args = {}) => bridge.invoke(command, args);
  const surfaces = [];
  let view, workflow, sheet, sheetControls;
  let shortcutBusy = false, workflowBusy = false;
  let workflowRead = 0;
  let shortcutRead = 0;

  function shortcutLabel(shortcut) {
    return (shortcut.control ? "⌃" : "") + (shortcut.option ? "⌥" : "") +
      (shortcut.shift ? "⇧" : "") + (shortcut.command ? "⌘" : "") +
      shortcut.code.replace(/^(Key|Digit)/, "");
  }

  function paint() {
    const row = document.getElementById("set-richglance-capture-state");
    if (row && view) {
      row.textContent = !view.enabled ? "Off" : view.registered ? shortcutLabel(view.shortcut) : "Shortcut unavailable";
    }
    for (const surface of surfaces) surface.paint();
  }

  async function refreshShortcut() {
    if (shortcutBusy) return;
    const ticket = ++shortcutRead;
    try {
      const status = await call("glance_capture_status");
      if (ticket !== shortcutRead || shortcutBusy || !status?.available) return;
      view = status;
      paint();
    } catch {}
  }

  function bindBackgroundSettings(button, report) {
    button.addEventListener("click", async () => {
      button.disabled = true;
      try {
        await call("glance_capture_open", { settings: true });
      } catch (error) {
        report(String(error));
      } finally {
        button.disabled = false;
      }
    });
  }

  async function loadWorkflow(surface) {
    if (workflowBusy) return;
    const ticket = ++workflowRead;
    try {
      const status = await call("glance_workflow_status");
      if (ticket !== workflowRead) return;
      workflow = status;
      paint();
    } catch (error) {
      if (ticket === workflowRead) surface.message(error);
    }
  }

  function bind(root, ids) {
    const el = name => root.querySelector("#" + ids[name]);
    const onPage = root === page;
    const toggle = el("toggle"), record = el("record"), feedback = el("feedback");
    const saved = onPage ? el("saved") : root.querySelector(".dict-saved");
    const keycaps = el("keycaps"), keyHint = el("keyHint"), summary = el("summary");
    const open = el("open"), copy = el("copy"), history = el("history"), historySummary = el("historySummary");
    const confirmation = el("confirmation"), cancel = el("cancel"), remove = el("remove");
    const message = el("message"), shortcut = el("shortcut");
    let listening = false;

    function paintSurface() {
      for (const button of [open, copy, history, remove]) button.disabled = !workflow || workflowBusy;
      open.setAttribute("aria-pressed", String(workflow?.opensEditor === true));
      copy.setAttribute("aria-pressed", String(workflow?.opensEditor !== true));
      history.setAttribute("aria-checked", String(workflow?.keepsHistory === true));
      history.closest(".dict-card")?.classList.toggle("is-on", workflow?.keepsHistory === true);
      if (workflow?.keepsHistory) {
        historySummary.textContent = workflow.keepsForever
          ? "On. Originals and editable marks stay on this Mac until you delete them."
          : "On. Originals and editable marks stay on this Mac for seven days.";
      } else {
        historySummary.textContent = workflow?.keepsForever
          ? "Off. Turn on to keep screenshots and editable marks until you delete them."
          : "Off. Turn on to keep screenshots and editable marks for seven days.";
      }
      toggle.disabled = record.disabled = !view || shortcutBusy;
      if (!view) return;
      toggle.setAttribute("aria-checked", String(view.enabled));
      toggle.closest(".dict-card")?.classList.toggle("is-on", view.registered);
      const label = shortcutLabel(view.shortcut);
      summary.textContent = !view.enabled ? "Off. Capture… is always in RichGlance’s File menu."
        : view.registered ? `On. Press ${label} in any app to start a capture.`
          : "Shortcut unavailable. Choose another combination.";
      record.textContent = listening ? "Cancel" : "Change";
      record.setAttribute("aria-label", listening ? "Cancel recording a capture shortcut" : "Change capture shortcut");
      keyHint.textContent = listening ? "Press the new shortcut. Esc to cancel."
        : view.enabled ? "Starts a capture on the display under your pointer."
          : "Used when Capture from any app is on.";
      keycaps.classList.toggle("is-listening", listening);
      keycaps.replaceChildren(...[...(listening ? "?" : label)].map(text => {
        const key = document.createElement("kbd");
        key.className = onPage ? "gp-key" : "dict-key";
        if (!onPage) key.style.cssText = "width:36px;height:38px;align-items:center;justify-content:center;padding:0";
        key.textContent = text;
        return key;
      }));
      if (shortcut) {
        const keys = document.createElement("kbd");
        keys.textContent = label;
        shortcut.replaceChildren(...(view.enabled && view.registered
          ? ["or press ", keys, " in any app"]
          : ["Turn on Capture from any app to start one with a shortcut"]));
      }
    }

    function setText(node, text) {
      node.textContent = text;
      node.hidden = !text;
    }

    function reset() {
      listening = false;
      confirmation.hidden = true;
      saved.hidden = true;
      setText(feedback, "");
      setText(message, "");
    }

    async function updateShortcut(enabled, keys) {
      if (!view || shortcutBusy) return;
      const replacing = listening;
      listening = false;
      shortcutBusy = true;
      ++shortcutRead;
      setText(feedback, "");
      setText(saved, "Saving…");
      paint();
      try {
        view = await call("glance_capture_configure", { enabled, shortcut: keys });
        setText(saved, "Saved.");
      } catch (error) {
        setText(saved, "");
        setText(feedback, String(error));
        listening = replacing && !root.hidden;
      } finally {
        shortcutBusy = false;
        paint();
        if (replacing && !root.hidden) record.focus();
      }
    }

    async function updateWorkflow(opensEditor, keepsHistory, returnFocus) {
      if (!workflow || workflowBusy) return;
      workflowBusy = true;
      ++workflowRead;
      setText(message, "Saving…");
      paint();
      try {
        workflow = await call("glance_workflow_configure", { opensEditor, keepsHistory });
        setText(onPage ? saved : message, "Saved.");
        if (onPage) setText(message, "");
      } catch (error) {
        setText(message, String(error));
      } finally {
        workflowBusy = false;
        paint();
        if (!root.hidden && document.activeElement === document.body) returnFocus.focus();
      }
    }

    toggle.addEventListener("click", () => updateShortcut(!view.enabled, view.shortcut));
    record.addEventListener("click", () => {
      listening = !listening;
      setText(feedback, "");
      saved.hidden = true;
      if (listening) record.focus();
      paint();
    });
    record.addEventListener("blur", () => {
      if (!listening || shortcutBusy) return;
      listening = false;
      setText(feedback, "");
      paint();
    });
    copy.addEventListener("click", () => updateWorkflow(false, null, copy));
    open.addEventListener("click", () => updateWorkflow(true, null, open));
    history.addEventListener("click", () => {
      if (!workflow.keepsHistory) updateWorkflow(null, true, history);
      else {
        confirmation.hidden = false;
        cancel.focus();
      }
    });
    cancel.addEventListener("click", () => { confirmation.hidden = true; history.focus(); });
    remove.addEventListener("click", () => {
      confirmation.hidden = true;
      updateWorkflow(null, false, history);
    });

    root.addEventListener("keydown", event => {
      if (event.key === "Escape" && (listening || !confirmation.hidden || !onPage)) {
        event.preventDefault();
        event.stopPropagation();
        if (listening) {
          listening = false;
          setText(feedback, "");
          paint();
        } else if (!confirmation.hidden) cancel.click();
        else close();
        return;
      }
      if (event.key === "Tab") {
        if (onPage) return;
        const buttons = [...root.querySelectorAll("button:not(:disabled)")].filter(button => !button.closest("[hidden]"));
        if (document.activeElement === (event.shiftKey ? buttons[0] : buttons.at(-1))) {
          event.preventDefault();
          (event.shiftKey ? buttons.at(-1) : buttons[0])?.focus();
        }
        return;
      }
      if (!listening) return;
      event.preventDefault();
      event.stopPropagation();
      if (event.repeat) return;
      if (!/^(Key[A-Z]|Digit[0-9])$/.test(event.code)) return;
      if ([event.ctrlKey, event.altKey, event.shiftKey, event.metaKey].filter(Boolean).length < 2) {
        setText(feedback, "Use two or more modifiers and a letter or number.");
        return;
      }
      updateShortcut(view.enabled, { code: event.code, control: event.ctrlKey, option: event.altKey,
        shift: event.shiftKey, command: event.metaKey });
    }, true);

    const surface = { paint: paintSurface, reset, message: text => setText(message, String(text)) };
    surfaces.push(surface);
    surface.paint();
    return surface;
  }

  function build() {
    if (sheet) return;
    sheet = document.createElement("div");
    sheet.id = "richglance-capture-settings";
    sheet.className = "overlay";
    sheet.hidden = true;
    sheet.dataset.dismiss = "control:#richglance-capture-settings .dict-close";
    sheet.innerHTML = `
      <section class="overlay-panel dict-panel" role="dialog" aria-labelledby="richglance-capture-title">
        <header class="dict-heading">
          <div>
            <p class="dict-eyebrow">Settings</p>
            <h2 id="richglance-capture-title" class="feature-title"><img class="feature-icon" src="assets/app-icons/richglance-128.png" alt="" />RichGlance</h2>
            <p class="rf-hint">Capture, mark up and share screenshots.</p>
          </div>
          <button class="dict-close" type="button" aria-label="Close RichGlance settings">×</button>
        </header>
        <div class="dict-body">
          <div class="dict-card">
            <span class="dict-spine"></span>
            <div class="dict-card-main">
              <h3 class="dict-card-title">Capture from any app</h3>
              <p class="dict-card-sub" id="richglance-capture-summary" aria-live="polite"></p>
            </div>
            <button class="dict-switch" role="switch" type="button" id="richglance-capture-on" aria-label="Capture from any app"></button>
          </div>
          <p class="dict-copy-note"><span>Use Capture Settings to enable the menu bar and shortcut when RichOS is quit.</span></p>
          <button type="button" class="dict-btn" id="richglance-background-settings">Capture Settings…</button>
          <section class="dict-sec" id="richglance-capture-fields">
            <div class="dict-sec-h"><h4 class="dict-sec-t">Your shortcut</h4><span class="dict-saved" role="status" hidden>Saved.</span></div>
            <div class="dict-key-row">
              <div class="dict-keys" id="richglance-capture-keys" aria-hidden="true"></div>
              <p class="dict-key-say is-soft" id="richglance-capture-key-hint" aria-live="polite"></p>
              <button class="dict-btn" type="button" id="richglance-capture-record"></button>
            </div>
          </section>
          <p class="dict-key-err" id="richglance-capture-feedback" role="alert"></p>
          <section class="dict-card" aria-labelledby="richglance-after-title">
            <span class="dict-spine"></span>
            <div class="dict-card-main">
              <h3 class="dict-card-title" id="richglance-after-title">After a capture</h3>
              <p class="dict-card-sub">Choose what Return does after you select a target.</p>
              <div class="richglance-workflow-actions">
                <button class="dict-btn" type="button" id="richglance-copy" aria-pressed="true">Copy Image</button>
                <button class="dict-btn" type="button" id="richglance-open-editor" aria-pressed="false">Open in Editor</button>
              </div>
            </div>
          </section>
          <section class="dict-card" aria-labelledby="richglance-history-title">
            <span class="dict-spine"></span>
            <div class="dict-card-main">
              <h3 class="dict-card-title" id="richglance-history-title">Recent Captures</h3>
              <p class="dict-card-sub" id="richglance-history-summary"></p>
            </div>
            <button class="dict-switch" role="switch" type="button" id="richglance-history" aria-label="Show Recent Captures"></button>
          </section>
          <section class="dict-sec" id="richglance-history-confirm" hidden>
            <p class="dict-card-sub">Turning off Recent Captures deletes the originals and editable marks it kept on this Mac.</p>
            <div class="richglance-workflow-actions">
              <button class="dict-btn" type="button" id="richglance-keep-history">Cancel</button>
              <button class="dict-btn" type="button" id="richglance-remove-history">Turn Off and Delete</button>
            </div>
          </section>
          <p class="dict-card-sub" id="richglance-workflow-feedback" role="status"></p>
        </div>
      </section>`;
    document.body.appendChild(sheet);
    sheet.querySelector(".dict-close").addEventListener("click", close);
    sheet.addEventListener("click", event => { if (event.target === sheet) close(); });
    sheetControls = bind(sheet, {
      toggle: "richglance-capture-on", record: "richglance-capture-record", feedback: "richglance-capture-feedback",
      keycaps: "richglance-capture-keys", keyHint: "richglance-capture-key-hint",
      summary: "richglance-capture-summary", open: "richglance-open-editor", copy: "richglance-copy",
      history: "richglance-history", historySummary: "richglance-history-summary", confirmation: "richglance-history-confirm",
      cancel: "richglance-keep-history", remove: "richglance-remove-history", message: "richglance-workflow-feedback",
    });
    bindBackgroundSettings(sheet.querySelector("#richglance-background-settings"), sheetControls.message);
  }

  function close() {
    sheet.hidden = true;
    sheetControls.reset();
    document.getElementById("set-btn")?.focus();
  }

  function open() {
    settings.close();
    build();
    sheet.hidden = false;
    sheetControls.reset();
    paint();
    sheet.querySelector(".dict-close").focus();
    loadWorkflow(sheetControls);
    refreshShortcut();
  }

  function registerPage() {
    if (!page) return;
    const $ = id => document.getElementById(id);
    const surface = bind(page, {
      toggle: "gp-shortcut", record: "gp-change", feedback: "gp-key-error", saved: "gp-saved",
      keycaps: "gp-keys", keyHint: "gp-key-hint", summary: "gp-shortcut-summary", shortcut: "gp-cap",
      open: "gp-after-edit", copy: "gp-after-copy", history: "gp-history", historySummary: "gp-history-summary",
      confirmation: "gp-history-confirm", cancel: "gp-history-keep", remove: "gp-history-remove", message: "gp-workflow-status",
    });
    const viewer = $("gp-viewer");
    const viewerStatus = $("gp-viewer-status");
    const viewerCount = $("gp-viewer-count");
    let viewerState, viewerBusy = false;
    async function readViewer(enabled = null) {
      if (viewerBusy) return;
      viewerBusy = true;
      viewer.disabled = true;
      viewerStatus.textContent = enabled === null ? "" : "Checking image viewer choices with macOS…";
      viewerStatus.hidden = !viewerStatus.textContent;
      try {
        viewerState = await call("richfiles_settings", { option: enabled === null ? null : "pictures", enabled });
        viewer.setAttribute("aria-checked", String(viewerState.pictures));
        viewerCount.hidden = !viewerState.pictures || !viewerState.pictureFormats;
        if (!viewerCount.hidden) {
          viewerCount.textContent = `RichGlance opens ${viewerState.picturesOpened} of ${viewerState.pictureFormats} formats.`;
        }
        viewerStatus.textContent = viewerState.message || "";
      } catch (error) {
        viewerStatus.textContent = String(error);
      } finally {
        viewerBusy = false;
        viewer.disabled = !viewerState;
        viewerStatus.hidden = !viewerStatus.textContent;
      }
    }
    viewer.addEventListener("click", () => readViewer(!viewerState.pictures));
    window.addEventListener("focus", () => {
      if (!page.hidden) readViewer();
      if (!page.hidden || sheet?.hidden === false) refreshShortcut();
    });

    function external(button, command, args, progress) {
      button.addEventListener("click", async () => {
        if (button.getAttribute("aria-disabled") === "true") return;
        button.setAttribute("aria-disabled", "true");
        surface.message(progress);
        try {
          await call(command, args);
          surface.message("");
        } catch (error) {
          surface.message(error);
        } finally {
          button.removeAttribute("aria-disabled");
        }
      });
    }
    bindBackgroundSettings($("gp-background-settings"), surface.message);
    external($("gp-capture"), "glance_capture_open", {}, "Opening capture…");
    external($("gp-richfiles"), "richfiles_open", { imageViewerSettings: true }, "Opening RichFiles Settings…");

    window.RichNavRail.registerToolPage("glance", "RichGlance", page, {
      onShow() {
        paint();
        loadWorkflow(surface);
        refreshShortcut();
        readViewer();
      },
      onHide: surface.reset,
    });
    $("nr-glance").hidden = false;
  }

  call("glance_capture_status").then(status => {
    if (!status?.available) return;
    view = status;
    settings.registerGlanceCapture({ open, paint });
    registerPage();
  }).catch(() => {});
})();

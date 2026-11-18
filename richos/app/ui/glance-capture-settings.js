/* RichOS registers the optional shortcut. RichGlance opens the native capture overlay. */
(function () {
  "use strict";
  const settings = window.RichSettings, bridge = window.RichBridge;
  if (!settings || !bridge) return;

  let view, sheet, toggle, record, feedback, saved, keycaps, keyHint, summary, card;
  let listening = false, busy = false;

  function shortcutLabel(shortcut) {
    return (shortcut.control ? "⌃" : "") + (shortcut.option ? "⌥" : "") +
      (shortcut.shift ? "⇧" : "") + (shortcut.command ? "⌘" : "") +
      shortcut.code.replace(/^(Key|Digit)/, "");
  }

  function paint() {
    if (!view) return;
    const row = document.getElementById("set-richglance-capture-state");
    if (row) row.textContent = !view.enabled ? "Off" : view.registered ? shortcutLabel(view.shortcut) : "Shortcut unavailable";
    if (!sheet) return;
    toggle.setAttribute("aria-checked", String(view.enabled));
    card.classList.toggle("is-on", view.registered);
    toggle.disabled = busy;
    record.disabled = busy;
    summary.textContent = !view.enabled ? "Off. Capture Region… is always in RichGlance’s File menu."
      : view.registered ? `On. Press ${shortcutLabel(view.shortcut)} in any app to capture a region.`
        : "Shortcut unavailable. Choose another combination.";
    record.textContent = listening ? "Cancel" : "Change";
    record.setAttribute("aria-label", listening ? "Cancel recording a capture shortcut" : "Change capture shortcut");
    keyHint.textContent = listening ? "Press the new shortcut. Esc to cancel."
      : view.enabled ? "Starts a capture on the display under your pointer."
        : "Used when Capture from any app is on.";
    keycaps.replaceChildren(...[...(listening ? "?" : shortcutLabel(view.shortcut))].map(text => {
      const key = document.createElement("kbd");
      key.className = "dict-key";
      key.style.cssText = "width:36px;height:38px;align-items:center;justify-content:center;padding:0";
      key.textContent = text;
      return key;
    }));
  }

  async function update(enabled, shortcut) {
    const replacing = listening;
    busy = true;
    listening = false;
    feedback.textContent = "";
    saved.hidden = true;
    paint();
    try {
      view = await bridge.invoke("glance_capture_configure", { enabled, shortcut });
      saved.hidden = false;
    } catch (error) {
      feedback.textContent = String(error);
      listening = replacing && !sheet.hidden;
    } finally {
      busy = false;
      paint();
      if (replacing && !sheet.hidden) record.focus();
    }
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
            <p class="rf-hint">Look closely at any part of your screen, measure it and capture it as a picture.</p>
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
          <p class="dict-copy-note"><span><b>Works while RichOS is running,</b> also when its window is minimized or closed. Turn this off or quit RichOS to free the shortcut.</span></p>
          <section class="dict-sec" id="richglance-capture-fields">
            <div class="dict-sec-h"><h4 class="dict-sec-t">Your shortcut</h4><span class="dict-saved" role="status" hidden>Saved.</span></div>
            <div class="dict-key-row">
              <div class="dict-keys" id="richglance-capture-keys" aria-hidden="true"></div>
              <p class="dict-key-say is-soft" id="richglance-capture-key-hint" aria-live="polite"></p>
              <button class="dict-btn" type="button" id="richglance-capture-record"></button>
            </div>
          </section>
          <p class="dict-key-err" id="richglance-capture-feedback" role="alert"></p>
        </div>
      </section>`;
    document.body.appendChild(sheet);
    toggle = sheet.querySelector("#richglance-capture-on");
    record = sheet.querySelector("#richglance-capture-record");
    feedback = sheet.querySelector("#richglance-capture-feedback");
    card = sheet.querySelector(".dict-card");
    saved = sheet.querySelector(".dict-saved");
    keycaps = sheet.querySelector("#richglance-capture-keys");
    keyHint = sheet.querySelector("#richglance-capture-key-hint");
    summary = sheet.querySelector("#richglance-capture-summary");
    sheet.querySelector(".dict-close").addEventListener("click", close);
    sheet.addEventListener("click", event => { if (event.target === sheet) close(); });
    toggle.addEventListener("click", () => update(!view.enabled, view.shortcut));
    record.addEventListener("click", () => {
      listening = !listening;
      feedback.textContent = "";
      saved.hidden = true;
      paint();
    });
    record.addEventListener("blur", () => {
      if (!listening) return;
      listening = false;
      feedback.textContent = "";
      paint();
    });
    sheet.addEventListener("keydown", handleKey, true);
  }

  function handleKey(event) {
    if (event.key === "Escape") {
      event.preventDefault();
      event.stopPropagation();
      if (listening) {
        listening = false;
        feedback.textContent = "";
        paint();
      } else close();
      return;
    }
    if (event.key === "Tab") {
      const buttons = [...sheet.querySelectorAll("button:not(:disabled)")];
      const boundary = event.shiftKey ? buttons[0] : buttons.at(-1);
      if (document.activeElement === boundary) {
        event.preventDefault();
        (event.shiftKey ? buttons.at(-1) : buttons[0])?.focus();
      }
      return;
    }
    if (!listening) return;
    event.preventDefault();
    event.stopPropagation();
    if (event.repeat || !/^(Key[A-Z]|Digit[0-9])$/.test(event.code)) return;
    if ([event.ctrlKey, event.altKey, event.shiftKey, event.metaKey].filter(Boolean).length < 2) {
      feedback.textContent = "Use two or more modifiers and a letter or number.";
      return;
    }
    update(view.enabled, { code: event.code, control: event.ctrlKey, option: event.altKey,
      shift: event.shiftKey, command: event.metaKey });
  }

  function close() {
    listening = false;
    sheet.hidden = true;
    document.getElementById("set-btn")?.focus();
  }

  function open() {
    settings.close();
    build();
    sheet.hidden = false;
    feedback.textContent = "";
    saved.hidden = true;
    paint();
    sheet.querySelector(".dict-close").focus();
  }

  bridge.invoke("glance_capture_status").then(status => {
    if (status?.available) {
      view = status;
      settings.registerGlanceCapture({ open, paint });
    }
  }).catch(() => {});
})();

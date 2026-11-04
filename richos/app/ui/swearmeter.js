/* A maintained local result. The switch changes display and its saved preference only. */
(function () {
  "use strict";
  const bridge = window.RichBridge;
  if (!bridge) return;
  let result = null, show = false, saving = false, retrying = false, preferenceEdits = 0;
  const call = (name, args) => bridge.invoke(name, args || {});
  function accept(next) {
    if (!next || !Number.isSafeInteger(next.revision)) return;
    if (result && next.revision < result.revision) return;
    result = next;
    paint();
  }
  function words() {
    if (!result || !result.available) return "SwearMeter unavailable";
    return "SwearMeter: " + result.count.toLocaleString();
  }
  function coverage() {
    if (!result || !result.available) return "The recorded result could not be read. Retry in settings.";
    const lines = [];
    if (result.unknown_card_inputs) lines.push("Partial history: some older card answers could not be attributed (" + result.unknown_card_inputs.toLocaleString() + " records).");
    if (result.unknown_message_inputs) lines.push("Partial history: some message text could not be attributed (" + result.unknown_message_inputs.toLocaleString() + " inputs).");
    if (result.incomplete_history) lines.push("Partial history: some recorded inputs could not be read or recovered.");
    return lines.join(" ");
  }
  function paintHome() {
    const signals = document.getElementById("home-signals");
    if (!signals) return;
    let node = document.getElementById("home-swearmeter");
    if (!node) {
      node = document.createElement("section");
      node.id = "home-swearmeter";
      node.className = "sig";
      node.setAttribute("aria-live", "polite");
      signals.appendChild(node);
    }
    node.hidden = !show;
    node.textContent = "";
    if (show) {
      const title = document.createElement("div");
      title.className = "swearmeter-number";
      title.textContent = words();
      const detail = document.createElement("p");
      detail.className = "swearmeter-detail";
      detail.textContent = coverage();
      const scope = document.createElement("p");
      scope.className = "swearmeter-detail";
      scope.textContent = "Words matched in your recorded RichOS messages and card answers. English dictionary on this installation.";
      node.append(title, detail, scope);
    }
  }
  function paint() {
    const input = document.getElementById("set-swearmeter");
    if (input) { input.checked = show; input.disabled = saving; }
    const row = document.getElementById("set-swearmeter-result");
    if (row) {
      row.hidden = !show;
      row.textContent = show ? words() + (coverage() ? ". " + coverage() : "") : "";
    }
    const retry = document.getElementById("set-swearmeter-retry");
    if (retry) {
      retry.hidden = !show || !!(result && result.available);
      retry.disabled = retrying;
    }
    paintHome();
    // The home door reserves space beneath the statistics and uses this existing event.
    window.dispatchEvent(new Event("resize"));
  }
  async function write(on) {
    if (saving) return;
    const previous = show;
    preferenceEdits++;
    show = on;
    saving = true;
    paint(); // Synchronous cached reveal. No source read or counting work here.
    try { await call("set_swearmeter_visible", { show: on }); }
    catch (error) {
      show = previous;
      window.RichSettings.toast("SwearMeter display preference was not saved: " + String(error));
    } finally { saving = false; paint(); }
  }
  async function retry() {
    if (retrying) return;
    retrying = true;
    paint();
    try { accept(await call("retry_swearmeter")); }
    catch (error) { window.RichSettings.toast("SwearMeter is still unavailable: " + String(error)); }
    finally { retrying = false; paint(); }
  }
  window.RichSwearMeter = { paintHome };
  window.RichSettings.registerSwearMeter({ paint, write, retry });
  (async function () {
    const ticket = preferenceEdits;
    try {
      // Subscribe first. A late initial read may not overwrite a newer notification.
      await bridge.listen("rich://swearmeter", event => accept(event.payload));
      const initial = await call("get_swearmeter");
      if (ticket === preferenceEdits) show = initial.show === true;
      accept(initial.result);
    } catch (_) { paint(); }
  })();
})();

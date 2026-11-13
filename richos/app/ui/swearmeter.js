/* A maintained local result. The switch changes display and its saved preference only. */
(function () {
  "use strict";
  const bridge = window.RichBridge;
  if (!bridge) return;
  let result = null, show = false, saving = false, retrying = false, savedShow = null, successfulWrites = 0;
  const call = (name, args) => bridge.invoke(name, args || {});
  function accept(next) {
    if (!next || !Number.isSafeInteger(next.revision)) return;
    if (result && next.revision < result.revision) return;
    result = next;
    paint();
  }
  function words() {
    if (!result || !result.available) return "SwearMeter unavailable";
    if (result.history_loading) return "SwearMeter is preparing your history";
    if (result.periods) return "All history: " + result.count.toLocaleString() + " · Last 30 days: " + result.periods.last_30_days.toLocaleString();
    return "SwearMeter: " + result.count.toLocaleString();
  }
  function coverage() {
    if (!result || !result.available) return "The recorded result could not be read. Retry in settings.";
    const lines = [];
    if (result.provider_history_incomplete) lines.push("Partial history: some provider records could not be verified.");
    if (result.ambiguous_matches) lines.push(result.ambiguous_matches.toLocaleString() + " possible duplicate matches withheld.");
    if (result.periods?.undated) lines.push(result.periods.undated.toLocaleString() + " undated matches are included in All history only.");
    if (result.unknown_card_inputs) lines.push("Partial history: some older card answers could not be attributed (" + result.unknown_card_inputs.toLocaleString() + " records).");
    if (result.unknown_message_inputs) lines.push("Partial history: some message text could not be attributed (" + result.unknown_message_inputs.toLocaleString() + " inputs).");
    if (result.incomplete_history) lines.push("Partial history: some recorded inputs could not be read or recovered.");
    return lines.join(" ");
  }
  function paintMonths(container) {
    const rows = show && result?.available && !result.history_loading ? result.periods?.months : null;
    let panel = container.querySelector(".swearmeter-months");
    if (!rows?.length) { panel?.remove(); return; }
    if (!panel) {
      panel = document.createElement("details");
      panel.className = "swearmeter-months";
      const title = document.createElement("summary");
      title.textContent = "By month";
      panel.appendChild(title);
      panel.appendChild(document.createElement("ul"));
      container.appendChild(panel);
    }
    const format = new Intl.DateTimeFormat(undefined, { month: "long", year: "numeric", timeZone: "UTC" });
    const list = panel.querySelector("ul");
    list.replaceChildren(...rows.map(row => {
      const label = format.format(new Date(Date.UTC(Math.floor(row.month / 100), row.month % 100 - 1, 1)));
      const item = document.createElement("li");
      item.textContent = label + ": " + (row.count === 0 && coverage() ? "No verified matches" : row.count.toLocaleString());
      return item;
    }));
  }
  function paintHome() {
    const signals = document.getElementById("home-signals");
    if (!signals) return;
    let node = document.getElementById("home-swearmeter");
    // Hidden display contributes no signal node or layout position. Accounting
    // remains in result, independent of whether this optional view exists.
    if (!show) { if (node) node.remove(); return; }
    if (!node) {
      node = document.createElement("section");
      node.id = "home-swearmeter";
      node.className = "sig";
      node.setAttribute("aria-live", "polite");
      signals.appendChild(node);
    }
    node.hidden = !show;
    if (show) {
      const monthPanel = node.querySelector(".swearmeter-months");
      for (const child of [...node.children]) if (child !== monthPanel) child.remove();
      const title = document.createElement("div");
      title.className = "swearmeter-number";
      title.textContent = words();
      const detail = document.createElement("p");
      detail.className = "swearmeter-detail";
      detail.textContent = coverage();
      const scope = document.createElement("p");
      scope.className = "swearmeter-detail";
      scope.textContent = result?.periods ? "Words matched in available RichOS, Claude Code and Codex history. English dictionary."
        : "Words matched in your recorded RichOS messages and card answers. English dictionary on this installation.";
      node.insertBefore(title, monthPanel);
      node.insertBefore(detail, monthPanel);
      node.insertBefore(scope, monthPanel);
      paintMonths(node);
    }
  }
  function paint() {
    const input = document.getElementById("set-swearmeter");
    if (input) { input.checked = show; input.disabled = saving; }
    const row = document.getElementById("set-swearmeter-result");
    if (row) {
      row.hidden = !show;
      row.textContent = show ? words() + (coverage() ? ". " + coverage() : "") : "";
      paintMonths(row.parentElement);
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
    show = on;
    saving = true;
    paint(); // Synchronous cached reveal. No source read or counting work here.
    if (on) {
      const row = document.getElementById("set-swearmeter-result");
      row?.scrollIntoView({ block: "nearest" });
      const menu = row?.closest(".setmenu"), foot = menu?.querySelector(".sm-foot");
      // The sticky foot covers part of the scrollport that nearest scrolling counts as visible.
      if (menu && foot) {
        const covered = row.getBoundingClientRect().bottom - foot.getBoundingClientRect().top;
        if (covered > 0) menu.scrollTop += Math.ceil(covered);
      }
    }
    try { await call("set_swearmeter_visible", { show: on }); savedShow = on; successfulWrites++; }
    catch (error) {
      show = savedShow === null ? previous : savedShow;
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
    const ticket = successfulWrites;
    try {
      // Subscribe first. A late initial read may not overwrite a newer notification.
      await bridge.listen("rich://swearmeter", event => accept(event.payload));
      const initial = await call("get_swearmeter");
      if (ticket === successfulWrites) {
        savedShow = initial.show === true;
        if (!saving) show = savedShow;
      }
      accept(initial.result);
    } catch (_) { paint(); }
  })();
})();

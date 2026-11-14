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

  // ROUND 73: one more signal at the foot of the home column, between the last signal and Talk to
  // Rich, in the column's own two lines: the number with "swear words" beside it, then "in the last
  // 30 days". Only while SwearMeter is on and the count is ready; otherwise no node and no space.
  function ready() {
    return show && !!result && result.available && !result.history_loading && !!result.periods;
  }
  function paintHome() {
    const signals = document.getElementById("home-signals");
    if (!signals) return;
    let node = document.getElementById("home-swearmeter");
    // Whenever SwearMeter is on, even in a column with no other signal (the CEO's brief: "IF the
    // user has enabled SwearMeter").
    if (!ready()) { if (node) node.remove(); return; }
    const count = result.periods.last_30_days;
    if (!node) {
      node = document.createElement("div");
      node.id = "home-swearmeter";
      node.className = "sig";
      node.innerHTML = '<div class="n"><span class="v"></span><small></small></div><div class="l">in the last 30 days</div>';
    }
    if (signals.lastElementChild !== node) signals.appendChild(node);
    node.querySelector(".v").textContent = count.toLocaleString("en-US");
    node.querySelector("small").textContent = count === 1 ? "swear word" : "swear words";
  }

  // ROUND 73: THE RAIL BUTTON AND ITS CARD. The button sits above Bust a bug while SwearMeter is on.
  // Hover opens the card and leaving closes it; a click pins it until a click elsewhere or Escape:
  // the Mac resources square's own behavior (`resources.js`), copied. The card: the last 30 days,
  // the daily average, the all-time total, then the months newest first, six at a glance, and one
  // button that reveals six more each press, in place (no pages).
  const GLANCE = 6;
  const railBtn = document.getElementById("nr-swear");
  let card = null, pinned = false, hovering = false, hideTimer = 0, shown = GLANCE, wasOpen = false;
  const fmt = (n) => Number(n).toLocaleString("en-US");
  const reduced = () => window.matchMedia && window.matchMedia("(prefers-reduced-motion: reduce)").matches;
  const $ = (k) => card.querySelector('[data-k="' + k + '"]');
  function monthRows() {
    const rows = (result && result.periods && result.periods.months) || [];
    return rows.slice().sort((x, y) => y.month - x.month);
  }
  function monthName(m, thisYear) {
    const y = Math.floor(m / 100);
    const name = new Intl.DateTimeFormat("en-US", { month: "long", timeZone: "UTC" }).format(new Date(Date.UTC(y, m % 100 - 1, 1)));
    return y === thisYear ? name : name + " " + y;
  }
  function buildCard() {
    card = document.createElement("div");
    card.id = "swear-card";
    card.className = "swear-card";
    card.setAttribute("role", "dialog");
    card.setAttribute("aria-label", "SwearMeter");
    card.innerHTML =
      '<div class="swear-head"><span class="swear-title">SwearMeter</span><span class="swear-range">Last 30 days</span></div>' +
      '<p class="swear-state" data-k="state" hidden></p>' +
      '<div data-k="body">' +
      '<div class="swear-hero"><b class="swear-big" data-k="big"></b><span class="swear-unit" data-k="unit"></span></div>' +
      '<dl class="swear-totals"><dt>A day, on average</dt><dd data-k="avg"></dd><dt>All time</dt><dd data-k="all"></dd></dl>' +
      '<div class="swear-sub">By month</div>' +
      '<ol class="swear-months" data-k="months"></ol>' +
      '<button type="button" class="swear-more" data-k="more"><span data-k="more-label"></span><svg viewBox="0 0 12 12" aria-hidden="true"><path d="M2.5 4.5 6 8l3.5-3.5" fill="none" stroke="currentColor" stroke-width="1.6" stroke-linecap="round" stroke-linejoin="round"/></svg></button>' +
      "</div>" +
      '<p class="swear-foot" data-k="foot"></p>';
    document.body.appendChild(card);
    card.addEventListener("pointerenter", () => { clearTimeout(hideTimer); if (card.classList.contains("is-open")) hovering = true; });
    card.addEventListener("pointerleave", () => { hideTimer = setTimeout(() => { hovering = false; sync(); }, 160); });
    $("more").addEventListener("click", () => {
      const total = monthRows().length, from = shown;
      shown = shown >= total ? GLANCE : Math.min(total, shown + GLANCE);
      paintCard(shown > from ? from : undefined);
      sync();
    });
  }
  function paintCard(revealFrom) {
    if (!card) return;
    const ok = ready();
    $("state").hidden = ok;
    $("body").hidden = !ok;
    card.querySelector(".swear-range").hidden = !ok;
    if (!ok) { $("state").textContent = words() + "."; $("foot").textContent = coverage(); return; }
    const p = result.periods, n = p.last_30_days;
    $("big").textContent = fmt(n);
    $("unit").textContent = n === 1 ? "swear word" : "swear words";
    $("avg").textContent = n === 0 ? "0" : n < 15 ? "Under 1" : fmt(Math.round(n / 30));
    $("all").textContent = fmt(result.count);
    const rows = monthRows(), now = new Date(), thisYear = now.getFullYear(), thisMonth = thisYear * 100 + now.getMonth() + 1;
    // A user with no counted month yet (seen on the real app, 2026-11-14): no "By month" over nothing.
    card.querySelector(".swear-sub").hidden = rows.length === 0;
    const max = Math.max(1, ...rows.map((r) => r.count));
    $("months").replaceChildren(...rows.slice(0, shown).map((r, i) => {
      const fresh = revealFrom !== undefined && i >= revealFrom;
      const li = document.createElement("li");
      if (fresh) { li.className = "is-new"; li.style.setProperty("--i", i - revealFrom); }
      else if (revealFrom !== undefined) li.className = "is-still";
      const name = document.createElement("span");
      name.className = "swear-m";
      name.textContent = monthName(r.month, thisYear);
      if (r.month === thisMonth) { const sf = document.createElement("i"); sf.textContent = "so far"; name.appendChild(sf); }
      const bar = document.createElement("span");
      bar.className = "swear-bar";
      const fill = document.createElement("i");
      fill.style.setProperty("--w", r.count ? Math.max(1.5, (r.count / max) * 100) + "%" : "0%");
      fill.style.setProperty("--i", fresh ? i - revealFrom : i);
      bar.appendChild(fill);
      const c = document.createElement("b");
      c.textContent = fmt(r.count);
      li.append(name, bar, c);
      return li;
    }));
    const more = $("more"), left = rows.length - shown, next = Math.min(GLANCE, left);
    more.hidden = rows.length <= GLANCE;
    more.classList.toggle("is-less", left <= 0);
    $("more-label").textContent = left > 0
      ? "Show " + next + " earlier month" + (next === 1 ? "" : "s")
      : "Show the last " + GLANCE + " months only";
    const notes = coverage();
    $("foot").textContent = (notes ? notes + " " : "") + "Words matched in your RichOS, Claude Code and Codex history. English dictionary.";
  }
  function place() {
    const r = railBtn.getBoundingClientRect();
    const foot = Math.min(window.innerHeight - 10, r.bottom + 14);
    card.style.left = Math.round(r.right + 14) + "px";
    card.style.bottom = Math.round(window.innerHeight - foot) + "px";
    card.style.setProperty("--swear-arrow", Math.round(Math.max(10, foot - (r.top + r.height / 2) - 6)) + "px");
  }
  function countUp() {
    const el = $("big"), to = result.periods.last_30_days;
    if (reduced() || to < 2) return;
    const t0 = performance.now(), dur = 520;
    (function step(t) {
      const k = Math.min(1, Math.max(0, (t - t0) / dur)), e = 1 - Math.pow(1 - k, 3);
      el.textContent = fmt(Math.round(to * e));
      if (k < 1) requestAnimationFrame(step);
    })(t0);
  }
  function sync() {
    if (!railBtn) return;
    // The home screen has no rail: a card pinned from the keyboard (no pointer press follows to
    // unpin it) is put away when Home opens, and does not come back with the app.
    const home = document.body.classList.contains("home-open");
    if (home) { pinned = false; hovering = false; }
    const open = !home && show && !railBtn.hidden && (pinned || hovering || railBtn.matches(":focus-visible"));
    if (open && !card) buildCard();
    if (!card) return;
    if (open && !wasOpen) {
      shown = GLANCE;
      paintCard();
      place();
      if (ready()) countUp();
    } else if (open) place();
    card.classList.toggle("is-open", open);
    railBtn.setAttribute("aria-expanded", String(pinned && open));
    wasOpen = open;
  }
  function paintRail() {
    if (!railBtn) return;
    railBtn.hidden = !show;
    if (!show) { pinned = false; hovering = false; }
    if (card && card.classList.contains("is-open")) paintCard();
    sync();
  }
  if (railBtn) {
    railBtn.addEventListener("pointerenter", () => { clearTimeout(hideTimer); hovering = true; sync(); });
    railBtn.addEventListener("pointerleave", () => { hideTimer = setTimeout(() => { hovering = false; sync(); }, 160); });
    railBtn.addEventListener("focus", sync);
    railBtn.addEventListener("blur", () => setTimeout(sync, 0));
    railBtn.addEventListener("click", (e) => {
      pinned = !pinned;
      if (!pinned) hovering = false;
      sync();
      // Pinned from the keyboard: the card's one control is next, so it is not a hunt.
      if (pinned && e.detail === 0 && card && !$("more").hidden) $("more").focus();
    });
    document.addEventListener("keydown", (e) => {
      if (e.key !== "Escape" || !card || !card.classList.contains("is-open")) return;
      // Answered here, so the window's Escape does not also close something behind the card.
      e.preventDefault();
      e.stopPropagation();
      const inside = card.contains(document.activeElement);
      pinned = false;
      hovering = false;
      if (inside) railBtn.focus();
      if (document.activeElement === railBtn) railBtn.blur();
      sync();
    }, true);
    document.addEventListener("pointerdown", (e) => {
      if (pinned && !railBtn.contains(e.target) && !(card && card.contains(e.target))) { pinned = false; sync(); }
    });
    window.addEventListener("resize", () => { if (card && card.classList.contains("is-open")) place(); });
    new MutationObserver(() => { if (document.body.classList.contains("home-open")) sync(); }).observe(document.body, { attributes: true, attributeFilter: ["class"] });
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
    paintRail();
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

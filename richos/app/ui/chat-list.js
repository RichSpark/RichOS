"use strict";

window.RichChatList = (() => {
  const shapes = {
    needs: '<circle cx="8" cy="8" r="6" fill="none" stroke="currentColor" stroke-width="1.6"/><path d="M8 4.5v4" stroke="currentColor" stroke-width="1.6"/><circle cx="8" cy="11" r="1" fill="currentColor"/>',
    stopped: '<path d="M8 2.4 14.4 13.4H1.6z" fill="none" stroke="currentColor" stroke-width="1.6" stroke-linejoin="round"/><path d="M8 6.6v3.1" stroke="currentColor" stroke-width="1.6" stroke-linecap="round"/><circle cx="8" cy="11.5" r=".9" fill="currentColor"/>',
    paused: '<rect x="3.6" y="2.8" width="3.1" height="10.4" rx=".9" fill="currentColor"/><rect x="9.3" y="2.8" width="3.1" height="10.4" rx=".9" fill="currentColor"/>',
    working: '<circle cx="8" cy="8" r="5.6" fill="none" stroke="currentColor" stroke-width="1.6"/><path d="M8 2.4a5.6 5.6 0 0 1 0 11.2z" fill="currentColor"/>',
    back: '<path d="M6 2.8 2.4 6.4 6 10M2.6 6.4h7.2a3.6 3.6 0 0 1 0 7.2H6.6" fill="none" stroke="currentColor" stroke-width="1.7" stroke-linecap="round" stroke-linejoin="round"/>',
    new: '<path d="M8 1.9 14.1 8 8 14.1 1.9 8z" fill="currentColor"/>',
    later: '<circle cx="8" cy="8" r="5.8" fill="none" stroke="currentColor" stroke-width="1.6"/><path d="M8 4.6V8l2.3 1.5" fill="none" stroke="currentColor" stroke-width="1.6" stroke-linecap="round" stroke-linejoin="round"/>',
  };
  const word = { needs: "Needs you", stopped: "Stopped", paused: "Paused", working: "Working", back: "Back", new: "New" };
  const clock = new Intl.DateTimeFormat("en-US", { hour: "numeric", minute: "2-digit" });
  const longDay = new Intl.DateTimeFormat("en-US", { weekday: "long" });
  const shortDay = new Intl.DateTimeFormat("en-US", { weekday: "short" });
  const shortDate = new Intl.DateTimeFormat("en-US", { month: "short", day: "numeric" });
  const time = (at) => clock.format(at).toLowerCase();
  const nextDay = (start, offset, hour, minute = 0) => {
    const day = new Date(start);
    day.setDate(day.getDate() + offset);
    day.setHours(hour, minute, 0, 0);
    return day.getTime();
  };
  function when(at, short = false) {
    if (new Date(at).toDateString() === new Date().toDateString()) return short ? time(at) : "today at " + time(at);
    if (short) return shortDay.format(at) + " " + time(at);
    return new Date(at).toDateString() === new Date(nextDay(Date.now(), 1, 8)).toDateString()
      ? "tomorrow at " + time(at) : longDay.format(at) + " at " + time(at);
  }
  function elapsed(ms) {
    const minutes = Math.max(1, Math.floor(ms / 60_000));
    if (minutes < 60) return minutes + " min";
    return minutes < 1440 ? Math.floor(minutes / 60) + " h" : counted(Math.floor(minutes / 1440), "day", "days");
  }
  function node(tag, className, text) {
    const element = document.createElement(tag);
    element.className = className;
    if (text) element.textContent = text;
    return element;
  }
  function status(row, openQuestions = 0, live = null) {
    const chat = row.chat || {};
    const count = chat.question_count ?? openQuestions;
    if (chat.later_until) return { key: "later", label: when(chat.later_until, true) };
    if (count || chat.waiting_since) return { key: "needs", label: word.needs, count };
    if (chat.stopped_at || (!row.chat && (row.last_turn_state === "interrupted" || live === "failed") && live !== "working")) {
      return { key: "stopped", label: word.stopped };
    }
    if (chat.paused) return { key: "paused", label: word.paused + (chat.resume_at ? " · " + when(chat.resume_at, true) : "") };
    if (chat.working_since || row.has_pending_turn || live === "working") return { key: "working", label: word.working };
    if (chat.back_at) return { key: "back", label: word.back };
    if (chat.unread || live === "unseen") return { key: "new", label: word.new };
    return null;
  }
  function mark(key, label, count) {
    const element = node("span", "chat-mark chat-mark--" + key);
    if (count) {
      element.append(node("span", "chat-count", String(count)));
    } else {
      const glyph = document.createElementNS("http://www.w3.org/2000/svg", "svg");
      glyph.setAttribute("viewBox", "0 0 16 16");
      glyph.setAttribute("class", "chat-shape");
      glyph.innerHTML = shapes[key];
      element.append(glyph);
    }
    if (label) element.append(node("span", "chat-word", label));
    element.setAttribute("aria-hidden", "true");
    return element;
  }
  function dressRow(row, options, wrap, button, parts, company, state) {
    const chat = row.chat || {};
    const tip = [row.display_title];
    wrap.classList.add("chat-list-row");
    button.classList.add("chat-list-row-button");
    if (state && !row.archived) {
      wrap.dataset.state = state.key;
      const label = state.key === "needs" && state.count && options.needsList
        ? (state.count === 1 ? "question" : "questions") : state.label;
      button.append(mark(state.key, label, state.count));
      const description = state.count ? counted(state.count, "open question", "open questions")
        : state.key === "later" ? "Comes back " + when(chat.later_until) : state.label;
      parts.push(description);
      tip.push(description + (state.key === "working" && chat.working_since ? " · " + elapsed(Date.now() - chat.working_since) : ""));
    }
    if (options.needsList || options.laterList) {
      let line = company;
      if (options.needsList) {
        const wait = state.key === "back" ? "back now" : state.key === "stopped"
          ? "stopped " + elapsed(Date.now() - (chat.stopped_at || row.last_activity)) + " ago"
          : "waiting " + elapsed(Date.now() - (chat.waiting_since || row.last_activity));
        line += " · " + wait;
      }
      button.append(node("span", "chat-list-sub", line));
      parts.push(line);
      tip.push(line);
    } else if (options.showEntity) {
      const tag = node("span", "nav-thread-entity", company);
      tag.setAttribute("aria-hidden", "true");
      button.append(tag);
      parts.push("in " + company);
    }
    if (state && chat.technical && chat.detail && !row.archived && state.key !== "later") {
      button.append(node("span", "chat-list-sub chat-list-sub--technical", chat.detail));
      parts.push(chat.detail);
      tip.push(chat.detail);
    }
    button.title = tip.join("\n");
  }

  function change(threadId, action, extra = {}) {
    return saveNavChange("set_chat_list_state", { threadId, action, ...extra });
  }
  async function changeAndRefresh(threadId, action, extra) {
    if (!await change(threadId, action, extra)) return false;
    await refreshNavigation();
    return true;
  }

  let laterMenu;
  function closeLater(restoreFocus = false) {
    if (!laterMenu) return false;
    const { panel, anchor } = laterMenu;
    panel.remove();
    laterMenu = null;
    if (restoreFocus && anchor.isConnected) anchor.focus();
    return true;
  }
  document.addEventListener("pointerdown", (event) => {
    if (laterMenu && !laterMenu.panel.contains(event.target)) closeLater();
  }, true);
  function laterChoices(now) {
    const evening = nextDay(now, 0, 18);
    const monday = nextDay(now, (8 - new Date(now).getDay()) % 7 || 7, 8);
    const choices = [];
    if (now < evening - 3_600_000) choices.push(["This evening", evening]);
    choices.push(["Tomorrow morning", nextDay(now, 1, 8)], ["Monday morning", monday]);
    return choices;
  }
  function openLater(row, anchor, point) {
    closeLater();
    const panel = node("div", "menu chat-later");
    panel.setAttribute("role", "group");
    panel.setAttribute("aria-label", "Bring " + row.display_title + " back later");
    panel.append(node("p", "chat-later-head", "Bring this back"));
    const now = Date.now();
    async function put(until) {
      if (until <= Date.now()) {
        error.textContent = "Choose a future time.";
        return;
      }
      if (panel.getAttribute("aria-busy") === "true") return;
      panel.setAttribute("aria-busy", "true");
      if (!await changeAndRefresh(row.id, "later", { until })) {
        panel.setAttribute("aria-busy", "false");
        error.textContent = "This time could not be saved.";
        return;
      }
      closeLater();
      offerUndo("Back " + when(until) + ".", () => changeAndRefresh(row.id, "now"));
      document.querySelector(".nav-group--later [data-thread-id=" + CSS.escape(row.id) + "]")?.focus();
    }
    for (const [label, until] of laterChoices(now)) {
      const button = node("button", "menu-item chat-later-item");
      button.type = "button";
      button.append(node("span", "", label), node("span", "chat-later-time", when(until, true)));
      button.onclick = () => put(until);
      panel.append(button);
    }
    const other = node("button", "menu-item chat-later-item", "Pick a time…");
    other.type = "button";
    other.setAttribute("aria-expanded", "false");
    const picker = buildPicker(now, put);
    picker.hidden = true;
    const error = node("div", "chat-later-error");
    error.setAttribute("role", "status");
    panel.append(other, picker, error);
    document.body.append(panel);
    laterMenu = { panel, anchor };
    const rect = anchor.getBoundingClientRect();
    function place() {
      panel.style.left = Math.max(4, Math.min(point?.x ?? rect.left, innerWidth - panel.offsetWidth - 4)) + "px";
      panel.style.top = Math.max(4, Math.min(point?.y ?? rect.bottom + 4, innerHeight - panel.offsetHeight - 4)) + "px";
    }
    other.onclick = () => {
      picker.hidden = !picker.hidden;
      other.setAttribute("aria-expanded", String(!picker.hidden));
      place();
      if (!picker.hidden) picker.querySelector("[aria-pressed='true']").focus();
    };
    panel.addEventListener("keydown", (event) => {
      if (event.key === "Escape") {
        event.preventDefault();
        event.stopPropagation();
        closeLater(true);
      }
      if (!["ArrowUp", "ArrowDown"].includes(event.key) || !event.target.matches(".chat-later-item")) return;
      const items = [...panel.querySelectorAll(".chat-later-item")];
      const step = event.key === "ArrowDown" ? 1 : -1;
      event.preventDefault();
      items[(items.indexOf(event.target) + step + items.length) % items.length].focus();
    });
    place();
    panel.querySelector("button").focus();
  }
  function buildPicker(now, put) {
    const picker = node("div", "chat-pick");
    const days = node("div", "chat-pick-days");
    days.setAttribute("role", "group");
    days.setAttribute("aria-label", "Day");
    const input = node("select", "chrome-select chat-pick-time");
    input.id = "chat-pick-time";
    const label = node("label", "chat-pick-label", "At");
    label.htmlFor = input.id;
    const done = node("button", "chat-pick-go", "Bring it back then");
    done.type = "button";
    let selected = 1;
    function fill() {
      const current = input.value || "480";
      input.replaceChildren();
      for (let minutes = 360; minutes <= 1320; minutes += 30) {
        const at = nextDay(now, selected, 0, minutes);
        if (at > Date.now()) input.add(new Option(time(at), String(minutes)));
      }
      input.value = current;
      if (input.selectedIndex === -1 && input.length) input.selectedIndex = 0;
      input.disabled = done.disabled = !input.length;
    }
    for (let i = 0; i < 7; i++) {
      const at = nextDay(now, i, 12);
      const day = node("button", "chat-pick-day");
      day.type = "button";
      day.setAttribute("aria-label", longDay.format(at) + ", " + shortDate.format(at));
      day.setAttribute("aria-pressed", String(i === selected));
      day.append(node("span", "", shortDay.format(at)), node("span", "chat-pick-date", String(new Date(at).getDate())));
      day.onclick = () => {
        selected = i;
        for (const button of days.children) button.setAttribute("aria-pressed", String(button === day));
        fill();
      };
      days.append(day);
    }
    done.onclick = () => put(nextDay(now, selected, 0, Number(input.value)));
    const controls = node("div", "chat-pick-row");
    controls.append(label, input, done);
    picker.append(days, controls);
    fill();
    return picker;
  }

  let undo;
  // The Mac app's page never sees Command-Z: the menu's Undo takes it (src-tauri/src/chat_undo.rs).
  // So the shell is told while an Undo is on offer, and asks here with `rich://menu-undo`.
  // Each call waits for the one before, so the shell ends on the page's last word.
  let menuUndoSent = Promise.resolve();
  function offerToMenu(offered) {
    menuUndoSent = menuUndoSent.then(() => Bridge.invoke("set_chat_undo", { armed: offered })).catch(() => {});
  }
  // main.js defines Bridge right after this file. A new page listens at once and withdraws any
  // offer a closed window left behind.
  setTimeout(() => {
    if (typeof Bridge === "undefined") return;
    Bridge.listen("rich://menu-undo", () => {
      if (undo && !textOwnsUndo(document.activeElement)) runUndo();
      else Bridge.invoke("native_undo").catch(() => {});
    });
    offerToMenu(false);
  });
  // A field edited since the Undo was offered owns Command-Z, even when the edit emptied it.
  let editedSinceOffer = null;
  document.addEventListener("input", (event) => { if (undo) editedSinceOffer = event.target; }, true);
  function textOwnsUndo(target) {
    if (!target) return false;
    if (target.isContentEditable || target === editedSinceOffer) return true;
    return !!(target.matches?.("textarea, input") && target.value);
  }
  function clearUndo(replacing) {
    if (!undo) return;
    clearTimeout(undo.timer);
    undo.element.remove();
    undo = null;
    if (replacing !== true) offerToMenu(false);
  }
  function runUndo() {
    const action = undo?.action;
    clearUndo();
    if (action) action();
  }
  function offerUndo(message, action) {
    clearUndo(true);
    const element = node("div", "chat-undo");
    const notice = node("span", "chat-undo-text", message);
    notice.setAttribute("role", "status");
    const button = node("button", "chat-undo-btn", "Undo");
    button.type = "button";
    button.title = "Undo (⌘Z)";
    button.onclick = runUndo;
    element.append(notice, button, node("span", "chat-undo-bar"));
    document.getElementById("rail").append(element);
    undo = { element, action, timer: setTimeout(clearUndo, 5000) };
    editedSinceOffer = null;
    offerToMenu(true);
  }
  document.addEventListener("keydown", (event) => {
    if (!undo || !(event.metaKey || event.ctrlKey) || event.shiftKey || event.altKey || event.key.toLowerCase() !== "z") return;
    if (textOwnsUndo(event.target)) return;
    event.preventDefault();
    event.stopPropagation();
    runUndo();
  }, true);

  function menuItems(row, add, anchor, point) {
    if (row.archived) return;
    if (row.chat?.later_until) {
      add("Bring back now", () => changeAndRefresh(row.id, "now"));
      return;
    }
    const unread = row.chat?.unread;
    add(unread ? "Mark as read" : "Mark as unread", () => changeAndRefresh(row.id, unread ? "read" : "unread"));
    add("Bring back later…", () => openLater(row, anchor, point));
  }

  let visit;
  function open(row, anchor) {
    const chat = row.chat || {};
    visit = { id: row.id, since: chat.seen_at, manual: chat.unread && !((chat.result_at || 0) > chat.seen_at),
      observed: chat.seen_at || 0, entered: false, placed: false, first: null, show: anchor };
  }

  function afterPaint(model, messages, mayScroll) {
    if (!visit || visit.id !== model.threadId) return false;
    const here = visit;
    const items = [...model.items.values()].filter((item) =>
      ["rich_message", "question_card", "interruption", "system_status"].includes(item.kind));
    if (!here.first && !here.entered && here.show) {
      const ordered = items.filter((item) => item.createdAt > here.since).sort((a, b) => a.createdAt - b.createdAt);
      const item = ordered[0] || (here.manual && items.reduce((last, item) => !last || item.createdAt >= last.createdAt ? item : last, null));
      if (item) here.first = item;
    }
    let scroll = false;
    const item = here.first;
    if (item) {
      const itemNode = messages.querySelector("[data-message-id=" + CSS.escape(item.id) + "]");
      const questionNode = item.question?.id && messages.querySelector("[data-question-id=" + CSS.escape(item.question.id) + "]");
      const target = itemNode || questionNode || messages.querySelector(".tl-turn[data-turn-id=" + CSS.escape(item.turnId) + "]");
      if (target) {
        let line = messages.querySelector(".chat-since");
        if (!line) {
          line = node("div", "chat-since", "Since you were last here");
          target.before(line);
        }
        if (!here.placed && mayScroll) {
          here.placed = true;
          line.scrollIntoView({ block: "start" });
          scroll = true;
        }
      }
    }
    return scroll;
  }
  function markVisible(model, row) {
    if (!visit || visit.id !== model.threadId || document.hidden || !document.hasFocus() || homeIsUp()) return;
    const here = visit;
    const newest = Math.max(row?.chat?.result_at || 0, row?.chat?.back_at || 0, row?.chat?.question_at || 0, row?.chat?.stopped_at || 0);
    if (!here.entered || newest > here.observed) {
      const at = here.entered ? newest : Date.now();
      here.entered = true;
      here.observed = Math.max(here.observed, at);
      if (row?.chat && (row.chat.unread || row.chat.back_at || newest > row.chat.seen_at)) {
        change(here.id, "seen", { seenAt: at }).catch(() => {});
      }
    }
  }

  let band;
  function paintBand(row) {
    const zone = document.getElementById("composer-zone");
    if (!zone) return;
    if (!band) {
      band = node("div", "chat-band");
      band.setAttribute("role", "status");
      zone.before(band);
    }
    const chat = row?.chat;
    const paused = chat?.paused;
    band.hidden = !(paused || chat?.recovering || (chat?.team_working && chat.working_since));
    if (band.hidden) return;
    const elapsedText = paused ? (chat.resume_at ? " Continues by itself at about " + when(chat.resume_at, true) + ". Nothing is lost."
      : " Continues by itself when usage is available. Nothing is lost.") : " · " + elapsed(Date.now() - chat.working_since);
    const copy = node("span", "chat-band-text");
    if (!paused && chat.recovering) {
      copy.append(node("strong", "", "Rich is picking this back up."), document.createTextNode(" It was running when RichOS restarted."));
    } else {
      copy.append(node("strong", "", paused ? "Paused." : "Your team is working on this"), document.createTextNode(elapsedText));
    }
    band.replaceChildren(mark(paused ? "paused" : "working"), copy);
    if (chat.technical && chat.detail) band.append(node("span", "chat-band-tech", chat.detail));
  }

  function buildSettings() {
    const section = node("section", "chat-notification-settings");
    section.append(node("h3", "chat-notification-title", "Notifications"));
    const status = node("div", "chat-notification-note");
    status.setAttribute("role", "status");
    for (const [key, pref, label, hint] of [
      ["mac", "mac_chat_notifications", "Mac notifications", "When RichOS is in the background"],
      ["sounds", "chat_notification_sounds", "Notification sounds", "Play a sound for notifications and question answers"],
    ]) {
      const row = node("label", "chat-notification-row");
      const copy = node("span", "chat-notification-copy");
      copy.append(node("span", "chat-notification-name", label), node("span", "chat-notification-hint", hint));
      const input = node("input", "chat-notification-switch");
      input.type = "checkbox";
      input.setAttribute("role", "switch");
      input.setAttribute("aria-label", label);
      input.checked = navPrefs?.[pref] ?? true;
      input.disabled = !navPrefs;
      input.onchange = async () => {
        const enabled = input.checked;
        input.disabled = true;
        try {
          await Bridge.invoke("set_chat_notifications", { [key]: enabled });
          if (navPrefs) navPrefs[pref] = enabled;
          status.textContent = "";
        } catch (_) {
          input.checked = !enabled;
          status.textContent = "This setting could not be saved.";
        } finally {
          input.disabled = false;
        }
      };
      row.append(copy, input);
      section.append(row);
    }
    section.append(status);
    return section;
  }
  return { status, dressRow, menuItems, offerUndo, closeLater, open, afterPaint, markVisible, paintBand, buildSettings };
})();

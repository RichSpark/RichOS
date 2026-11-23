"use strict";

window.RichProfile = (() => {
  const photoUrl = "assets/rich-hand.png";
  const triggerLabel = "Rich Hand: open his profile";
  const wide = matchMedia("(min-width: 1180px)");
  const reduceMotion = matchMedia("(prefers-reduced-motion: reduce)");
  const byId = (id) => document.getElementById(id);
  let panel;
  let scrim;
  let context;
  let opener;
  let source;
  let setupCard;
  let flight;
  let opening;
  let closing;

  function button(className) {
    const node = document.createElement("button");
    node.type = "button";
    node.className = "rich-profile-trigger " + className;
    node.setAttribute("aria-label", triggerLabel);
    node.title = triggerLabel;
    node.addEventListener("click", () => open(node));
    return node;
  }

  // Keep the name and its optional portrait in one keyboard stop.
  function identity(withAvatar, prefix = "") {
    const node = button("");
    const identity = document.createDocumentFragment();
    if (withAvatar) {
      const avatar = document.createElement("img");
      avatar.className = "tl-avatar";
      avatar.src = photoUrl;
      avatar.alt = "";
      if (prefix) {
        const portraitButton = button("");
        portraitButton.tabIndex = -1;
        portraitButton.append(avatar);
        identity.append(portraitButton);
      } else node.append(avatar);
    }
    const name = document.createElement("span");
    name.className = "tl-who";
    name.textContent = "Rich";
    node.append(name);
    if (!prefix) return node;
    const label = document.createElement("span");
    label.className = "tl-who";
    label.append(prefix + " · through ", node);
    identity.append(label);
    return identity;
  }

  function build() {
    scrim = document.createElement("div");
    scrim.id = "rich-profile-scrim";
    scrim.hidden = true;
    scrim.addEventListener("click", () => close());
    panel = document.createElement("aside");
    panel.id = "rich-profile";
    panel.hidden = true;
    panel.dataset.dismiss = "escape";
    panel.setAttribute("aria-labelledby", "rich-profile-name");
    panel.innerHTML = `
      <div class="rich-profile-inner">
        <header class="op-head rich-profile-head">
          <span class="op-eyebrow">Profile</span>
          <button class="op-close" type="button" aria-label="Close Rich's profile" title="Close (Esc)">
            <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" aria-hidden="true"><path d="M18 6 6 18m0-12 12 12"/></svg>
          </button>
        </header>
        <div class="rich-profile-body">
          <div class="rich-profile-portrait"><img class="rich-profile-photo" src="${photoUrl}" alt="Rich Hand"></div>
          <div class="rich-profile-id">
            <h2 id="rich-profile-name" class="rich-profile-name">Rich Hand</h2>
            <p class="rich-profile-title">Your AI Chief of Staff &amp; Business Operations Lead</p>
            <p class="rich-profile-status"><span class="rich-profile-dot" aria-hidden="true"></span>Here now · answers at once</p>
          </div>
          <div class="rich-profile-actions">
            <button class="rich-profile-btn rich-profile-btn--primary" type="button" data-action="message"><svg class="rich-profile-btn-icon" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true"><path d="M21 15a2 2 0 0 1-2 2H7l-4 4V5a2 2 0 0 1 2-2h14a2 2 0 0 1 2 2z"/></svg>Message Rich</button>
            <button class="rich-profile-btn" type="button" data-action="talk">Talk to Rich</button>
          </div>
          <h3 class="rich-profile-section">About</h3>
          <p class="rich-profile-about">I help you realize your strategic vision. Together with my twin brother Stu I manage a team of specialist AI teammates to get things done. So, I come back to you only when something needs you.</p>
          <h3 class="rich-profile-section" data-now-heading hidden>Right now</h3>
          <ul class="rich-profile-now" hidden></ul>
          <h3 class="rich-profile-section">Details</h3>
          <dl class="rich-profile-facts">
            <div class="rich-profile-fact"><dt>Works with</dt><dd>Stu (who takes the longer jobs) and the team</dd></div>
            <div class="rich-profile-fact"><dt>Thinks with</dt><dd data-model></dd></div>
            <div class="rich-profile-fact"><dt>Reach him</dt><dd>Here by text, by voice or on your phone</dd></div>
            <div class="rich-profile-fact" data-since hidden><dt>With you since</dt><dd></dd></div>
          </dl>
        </div>
      </div>`;
    panel.querySelector(".op-close").addEventListener("click", () => close());
    const talk = byId("talk-toggle");
    const talkButton = panel.querySelector('[data-action="talk"]');
    const icon = talk.querySelector(".talk-icon").cloneNode(true);
    icon.classList.add("rich-profile-talk-icon");
    talkButton.prepend(icon);
    talkButton.addEventListener("click", () => {
      close({ keepFocus: true });
      talk.focus({ preventScroll: true });
      talk.click();
    });
    panel.querySelector('[data-action="message"]').addEventListener("click", () => {
      close({ keepFocus: true });
      byId("input").focus();
    });
  }

  function renderWork() {
    if (!panel?.isConnected || panel.hidden || panel.inert) return;
    const list = panel.querySelector(".rich-profile-now");
    const work = context.work();
    // Keep a focused row in place across chat-list refreshes with the same data.
    if (list.children.length === work.length && work.every(({ id, title, state }, index) => {
      const row = list.children[index].firstElementChild;
      return row.dataset.id === id && row.dataset.state === state &&
        row.querySelector(".rich-profile-now-title").textContent === title;
    })) return;
    const focusedId = list.contains(document.activeElement) ? document.activeElement.dataset.id : null;
    list.replaceChildren(...work.map(({ id, title, state }) => {
      const li = document.createElement("li");
      const row = document.createElement("button");
      row.type = "button";
      row.className = "rich-profile-now-row";
      row.dataset.id = id;
      row.dataset.state = state;
      const mark = document.createElement("span");
      mark.className = "rich-profile-now-mark rich-profile-now-mark--" + state;
      mark.setAttribute("aria-hidden", "true");
      const text = document.createElement("span");
      text.className = "rich-profile-now-text";
      const name = document.createElement("span");
      name.className = "rich-profile-now-title";
      name.textContent = title;
      const detail = document.createElement("span");
      detail.className = "rich-profile-now-sub";
      detail.textContent = state === "working" ? "Stu is on it for me" : "Stopped, waiting for your word";
      text.append(name, detail);
      row.append(mark, text);
      row.addEventListener("click", () => {
        close({ instant: true, keepFocus: true });
        context.openChat(id);
      });
      li.append(row);
      return li;
    }));
    list.hidden = panel.querySelector("[data-now-heading]").hidden = work.length === 0;
    if (focusedId) {
      const back = Array.from(list.querySelectorAll("button")).find((row) => row.dataset.id === focusedId);
      (back || panel.querySelector(".op-close")).focus({ preventScroll: true });
    }
  }

  async function renderSince(request) {
    const row = panel.querySelector("[data-since]");
    row.hidden = true;
    panel.dataset.since = "pending";
    let date;
    try {
      const { installedAt } = await context.launchState() || {};
      if (Number.isFinite(installedAt) && installedAt > 0) {
        date = new Intl.DateTimeFormat(undefined, { dateStyle: "long" }).format(installedAt);
      }
    } catch (_) {
      // An unreadable launch record cannot supply a date.
    }
    if (opener !== request) return;
    panel.dataset.since = date ? "available" : "unavailable";
    if (date) {
      row.querySelector("dd").textContent = date;
      row.hidden = false;
    }
  }

  function fly(from) {
    if (reduceMotion.matches || !from.width) return;
    const photo = panel.querySelector(".rich-profile-photo");
    const to = photo.getBoundingClientRect();
    if (!to.width) return;
    const ghost = photo.cloneNode();
    ghost.className = "rich-profile-ghost";
    ghost.alt = "";
    Object.assign(ghost.style, {
      left: to.left + "px", top: to.top + "px", width: to.width + "px", height: to.height + "px",
      objectPosition: getComputedStyle(photo).objectPosition,
    });
    document.body.append(ghost);
    photo.style.visibility = "hidden";
    flight = ghost.animate([
      {
        transform: `translate(${from.left - to.left}px, ${from.top - to.top}px) scale(${from.width / to.width}, ${from.height / to.height})`,
        borderRadius: "50%",
      },
      { transform: "none", borderRadius: getComputedStyle(photo).borderRadius },
    ], { duration: 420, easing: "cubic-bezier(0.22, 1, 0.36, 1)" });
    const restore = () => {
      ghost.remove();
      photo.style.visibility = "";
    };
    flight.onfinish = flight.oncancel = restore;
  }

  function syncLayout() {
    if (!opener) return;
    const overlay = !!setupCard || !wide.matches;
    panel.dataset.layout = overlay ? "overlay" : "docked";
    scrim.hidden = !overlay;
    document.body.classList.toggle("rich-profile-docked", !overlay);
  }

  function open(trigger) {
    if (opener?.trigger === trigger) {
      close();
      return;
    }
    if (!panel) build();
    const setup = trigger.closest("#setup-sheet");
    const avatar = setup ? byId("setup-rich-avatar") : trigger.closest(".tl-rich-meta").querySelector(".tl-avatar");
    const bounds = trigger.getBoundingClientRect();
    const from = avatar?.getBoundingClientRect() || {
      left: bounds.left - 4, top: bounds.top + bounds.height / 2 - 9, width: 18, height: 18,
    };
    close({ instant: true, keepFocus: true });
    context.closeOtherPanes();
    const out = byId("outpanel");
    if (setup) setup.append(scrim, panel);
    else out.after(scrim, panel);
    source = trigger.closest("[data-message-id], [data-question-owner], .suite-note");
    opener = { trigger };
    setupCard = setup?.querySelector(".overlay-panel");
    if (setupCard) setupCard.inert = true;
    panel.inert = false;
    panel.hidden = false;
    syncLayout();
    panel.querySelector(".rich-profile-body").scrollTop = 0;
    panel.querySelector("[data-model]").textContent = context.model();
    panel.querySelector(".rich-profile-actions").hidden = !!setup;
    panel.querySelector('[data-action="talk"]').hidden = byId("talk-toggle").hidden;
    renderWork();
    renderSince(opener);
    fly(from);
    if (!reduceMotion.matches) {
      const frames = panel.dataset.layout === "docked" ? [{ width: "0px" }, { width: getComputedStyle(panel).width }] :
        [{ transform: "translateX(100%)" }, { transform: "translateX(0)" }];
      opening = panel.animate(frames, { duration: 420, easing: "cubic-bezier(0.22, 1, 0.36, 1)" });
    }
    panel.querySelector(".op-close").focus({ preventScroll: true });
  }

  function close({ instant = false, keepFocus = false } = {}) {
    if (!opener && !closing) return;
    opening?.cancel();
    if (flight) {
      // Cancel and clean up synchronously so a new flight cannot be uncovered by the old one.
      flight.oncancel();
      flight.oncancel = null;
      flight.cancel();
      flight = null;
    }
    closing?.cancel();
    closing = null;
    const trigger = opener?.trigger;
    let back = trigger;
    if (setupCard) {
      setupCard.inert = false;
      setupCard = null;
      back = byId("setup-rich-name");
    } else if (trigger?.isConnected && trigger.tabIndex < 0) {
      back = trigger.closest(".tl-rich-meta").querySelector(".rich-profile-trigger:not([tabindex])");
    } else if (trigger && !trigger.isConnected) {
      const key = source?.dataset.messageId ?? source?.dataset.questionOwner;
      back = key ? document.querySelector(`[data-message-id="${CSS.escape(key)}"], [data-question-owner="${CSS.escape(key)}"]`)?.querySelector(".rich-profile-trigger:not([tabindex])") :
        document.querySelector(".tl-rich-meta .rich-profile-trigger");
    }
    opener = null;
    source = null;
    panel.inert = true;
    panel.removeAttribute("data-dismiss");
    scrim.hidden = true;
    const finish = () => {
      panel.remove();
      scrim.remove();
      panel.hidden = true;
      panel.dataset.dismiss = "escape";
      document.body.classList.remove("rich-profile-docked");
      closing = null;
    };
    if (instant || reduceMotion.matches) finish();
    else {
      const frames = panel.dataset.layout === "docked" ? [{ width: getComputedStyle(panel).width }, { width: "0px" }] :
        [{ transform: "translateX(0)" }, { transform: "translateX(100%)" }];
      closing = panel.animate(frames, { duration: 260, easing: "cubic-bezier(0.55, 0, 0.75, 0.2)" });
      closing.onfinish = finish;
    }
    if (!keepFocus && trigger) (back?.isConnected ? back : byId("conversation")).focus({ preventScroll: true });
  }

  function init(options) {
    context = options;
    wide.addEventListener("change", syncLayout);
    for (const id of ["setup-rich-avatar-button", "setup-rich-name"]) {
      byId(id).addEventListener("click", (event) => open(event.currentTarget));
    }
  }

  function reportDetails() {
    if (!opener) return "";
    const layout = panel.dataset.layout === "docked" ? "docked" : "over the conversation";
    const start = { pending: "loading", available: "shown", unavailable: "unavailable" }[panel.dataset.since];
    const activity = panel.querySelector(".rich-profile-now").hidden ? "no current chats shown" : "current chats shown";
    return `${layout}, first-launch date ${start}, ${activity}`;
  }

  return { identity, init, close, renderWork, reportDetails };
})();
